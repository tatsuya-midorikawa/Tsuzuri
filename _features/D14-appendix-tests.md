# D14 付録 2: テスト（E・F）

本文は [D14-text-class.md](D14-text-class.md)、付録 1 は [D14-appendix.md](D14-appendix.md)。ここにあるファイルは、すべて試作のコンパイラで**実際に通した全文**（`node tests/features.mjs <試作> text_class` が `native/WASM at O0/O3 passed`、`features: 4595 cases passed`）。HEAD のバイナリでは `E1016: unknown type class 'Text'` で落ちる（テスト先行の赤）。

| 節 | ファイル | Phase |
| --- | --- | --- |
| E.1 | `tests/fixtures/text_class/Main.tz`（新規） | 1 |
| E.2 | `tests/features.mjs` の差分（suite `text_class`、参照値） | 1 |
| E.3 | `tests/fixtures/text_class_direct/Main.tz`・`tests/fixtures/text_class_calls/Main.tz`（新規） | 1 |
| E.4 | `tests/text_class.mjs`（新規。IR の構造の検査と測定） | 1 |
| E.5 | `tests/text_class.rs`・`tests/fixtures/text_user_instance/Main.tz`（新規。型検査・診断のテスト） | 1 |
| F.1〜F.3 | `tests/fixtures/regex_text/Main.tz`、`tests/features.mjs` の差分、`tests/text_class.rs` への追記 | 2 |

## E. Phase 1 のテスト

### E.1 `tests/fixtures/text_class/Main.tz`（新規）

`Text.m` を `string` と `utf8string` に使い、`tests/features.mjs` の suite `text_class` が値を照合する。文字は添字で選ぶ（`sample` は `string_library` の `stringSamples` と同じ 10 個）。字下げはスペース 4 個。

```tsuzuri
def sample :: i64 -> string
fn sample index = match index with
    | 0 -> ""
    | 1 -> "hello hello"
    | 2 -> "l"
    | 3 -> "a\0b"
    | 4 -> "\u{1F600}"
    | 5 -> "\uE000"
    | 6 -> " \tAbC\r\n"
    | 7 -> "\uD800"
    | 8 -> "\uDE00"
    | _ -> "aa"

def utf8_sample :: i64 -> utf8string
fn utf8_sample index =
    let text = sample index
    Utf8String.from_string (ref text)

def hash :: string -> i64
fn hash text =
    let mut value = text.length
    for unit in text do value = value * 31 + unit as i64
    value

def hash_utf8 :: utf8string -> i64
fn hash_utf8 text =
    let mut value = text.length
    for byte in text do value = value * 31 + byte as i64
    value

def position :: Maybe<i64> -> i64
fn position found = Maybe.default_value (-1) found

def flag :: bool -> i64
fn flag value = if value then 1 else 0

// Generic code over Text: one body, two instantiations (string and utf8string).
def order_flags :: Text<'s> => ref 's -> ref 's -> i64
fn order_flags left right =
    (if deref left < deref right then 1 else 0) + (if deref left == deref right then 2 else 0)

def trimmed_length :: Text<'s> => ref 's -> i64
fn trimmed_length text =
    let trimmed = Text.trim text
    Text.length (ref trimmed)

def piece_count :: Text<'s> => ref 's -> ref 's -> i64
fn piece_count separator text = (Text.split separator text).length

export def text_operation :: i32 -> i64 -> i64 -> i64
fn text_operation operation first second =
    let text = sample first
    let needle = sample second
    let replacement = "-"
    match operation with
    | 0 -> position (Text.find (ref needle) (ref text))
    | 1 -> position (Text.rfind (ref needle) (ref text))
    | 2 -> flag (Text.contains (ref needle) (ref text))
    | 3 -> flag (Text.starts_with (ref needle) (ref text))
    | 4 -> flag (Text.ends_with (ref needle) (ref text))
    | 5 -> hash (Text.replace (ref needle) (ref replacement) (ref text))
    | 6 ->
        let parts = Text.split (ref needle) (ref text)
        let mut total = parts.length
        for part in parts do total = total * 31 + hash (Text.clone (ref part))
        total
    | 7 ->
        let parts = [text, needle]
        hash (Text.concat (ref parts))
    | 8 ->
        let parts = [text, needle]
        hash (Text.join (ref replacement) (ref parts))
    | 9 -> hash (Text.trim (ref text))
    | 10 -> hash (Text.to_ascii_lower (ref text))
    | 11 -> hash (Text.to_ascii_upper (ref text))
    | 12 -> Text.compare (ref text) (ref needle)
    | 13 -> hash (Text.repeat (ref text) 2)
    | _ -> 0

export def utf8_operation :: i32 -> i64 -> i64 -> i64
fn utf8_operation operation first second =
    let text = utf8_sample first
    let needle = utf8_sample second
    let replacement = u8"-"
    match operation with
    | 0 -> position (Text.find (ref needle) (ref text))
    | 1 -> position (Text.rfind (ref needle) (ref text))
    | 2 -> flag (Text.contains (ref needle) (ref text))
    | 3 -> flag (Text.starts_with (ref needle) (ref text))
    | 4 -> flag (Text.ends_with (ref needle) (ref text))
    | 5 -> hash_utf8 (Text.replace (ref needle) (ref replacement) (ref text))
    | 6 ->
        let parts = Text.split (ref needle) (ref text)
        let mut total = parts.length
        for part in parts do total = total * 31 + hash_utf8 (Text.clone (ref part))
        total
    | 7 ->
        let parts = [text, needle]
        hash_utf8 (Text.concat (ref parts))
    | 8 ->
        let parts = [text, needle]
        hash_utf8 (Text.join (ref replacement) (ref parts))
    | 9 -> hash_utf8 (Text.trim (ref text))
    | 10 -> hash_utf8 (Text.to_ascii_lower (ref text))
    | 11 -> hash_utf8 (Text.to_ascii_upper (ref text))
    | 12 ->
        let order = Text.compare (ref text) (ref needle)
        assert ((order < 0) == (text < needle) && (order > 0) == (text > needle))
        order
    | 13 -> hash_utf8 (Text.repeat (ref text) 2)
    | _ -> 0

export def text_slice :: i64 -> i64 -> i64 -> i64
fn text_slice which first last =
    let text = sample which
    match Text.slice (ref text) first last with | Some value -> hash value | None -> -1

export def utf8_slice :: i64 -> i64 -> i64 -> i64
fn utf8_slice which first last =
    let text = utf8_sample which
    match Text.slice (ref text) first last with | Some value -> hash_utf8 value | None -> -1

export def text_sub :: i64 -> i64 -> i64 -> i64
fn text_sub which first count =
    let text = sample which
    match Text.sub (ref text) first count with | Some value -> hash value | None -> -1

export def utf8_sub :: i64 -> i64 -> i64 -> i64
fn utf8_sub which first count =
    let text = utf8_sample which
    match Text.sub (ref text) first count with | Some value -> hash_utf8 value | None -> -1

export def text_extra :: i32 -> i64 -> i64
fn text_extra operation which =
    let text = sample which
    match operation with
    | 0 -> Text.length (ref text)
    | 1 -> flag (Text.is_empty (ref text))
    | 2 -> hash (Text.trim_start (ref text))
    | 3 -> hash (Text.trim_end (ref text))
    | 4 -> hash (Text.clone (ref text))
    | 5 -> Text.compare (ref text) (ref text)
    | 6 -> trimmed_length (ref text)
    | _ -> 0

export def utf8_extra :: i32 -> i64 -> i64
fn utf8_extra operation which =
    let text = utf8_sample which
    match operation with
    | 0 -> Text.length (ref text)
    | 1 -> flag (Text.is_empty (ref text))
    | 2 -> hash_utf8 (Text.trim_start (ref text))
    | 3 -> hash_utf8 (Text.trim_end (ref text))
    | 4 -> hash_utf8 (Text.clone (ref text))
    | 5 -> Text.compare (ref text) (ref text)
    | 6 -> trimmed_length (ref text)
    | _ -> 0

export def text_order :: i64 -> i64 -> i64
fn text_order first second =
    let left = sample first
    let right = sample second
    order_flags (ref left) (ref right)

export def utf8_order :: i64 -> i64 -> i64
fn utf8_order first second =
    let left = utf8_sample first
    let right = utf8_sample second
    order_flags (ref left) (ref right)

export def text_pieces :: i64 -> i64 -> i64
fn text_pieces first second =
    let text = sample first
    let separator = sample second
    piece_count (ref separator) (ref text)

export def utf8_pieces :: i64 -> i64 -> i64
fn utf8_pieces first second =
    let text = utf8_sample first
    let separator = utf8_sample second
    piece_count (ref separator) (ref text)

// A literal or another temporary as an argument of a method that takes two or more arguments.
export def literal_arguments :: i64
fn literal_arguments =
    let found = position (Text.find "l" "hello")
    let contained = flag (Text.contains "ell" "hello") + flag (Text.contains u8"ell" u8"hello")
    let repeated = (Text.repeat "ab" 3).length + (Text.repeat u8"ab" 2).length
    let replaced = (Text.replace "l" "LL" "hello").length
    let joined = (Text.join "," ["a", "b", "c"]).length
    found * 10000 + contained * 1000 + joined * 100 + repeated * 10 + replaced

export def trap_repeat_negative :: i64
fn trap_repeat_negative =
    let text = ""
    (Text.repeat (ref text) (-1)).length

export def trap_repeat_negative_utf8 :: i64
fn trap_repeat_negative_utf8 =
    let text = u8""
    (Text.repeat (ref text) (-1)).length
```

`literal_arguments` の期待値 22607 の内訳: `found` = 2（最初の `l`）→ 20000、`contained` = 2 → 2000、`joined` = `"a,b,c"` の長さ 5 → 500、`repeated` = `"ababab"` の 6 ＋ `u8"abab"` の 4 = 10 → 100、`replaced` = `"heLLLLo"` の 7。合計 22607。`trap_repeat_negative*` は負の回数のトラップ（`features.mjs` の `traps`）。

### E.2 `tests/features.mjs` の差分（Phase 1 の分。`diff -u` の結果）

1 つめの塊は、`active_patterns` の suite の次（`string_library` の前）。`inspect` は、その suite の IR を渡される既存の仕組み。2 つめの塊は `string_library` の参照値を作るループの次（`hashBytes` の前）。`stringSamples`・`textHash`・`wrapHash` は同じファイルの既存の定義。

```diff
@@ suites（active_patterns の次）
+  // D14: Text.f gives the values of the direct String.f and Utf8String.f calls. The cases of string_library, and the
+  // cases below the loops that fill string_library, are the references.
+  text_class: {
+    cases: [
+      ["literal_arguments", [], 22607n],
+    ],
+    traps: [["trap_repeat_negative", []], ["trap_repeat_negative_utf8", []]],
+    inspect(ir) {
+      // Instances are chosen at compile time: no dictionary, no vtable, and the Text instance methods are direct functions.
+      assert.doesNotMatch(ir, /%tz\.hkt|dictionary|@tz\.dyn\./);
+      for (const type of ["String", "Utf8String"]) {
+        const methods = new Set([...ir.matchAll(new RegExp(`^define internal [^\\n]*@tz\\.fn\\.${type}\\.\\$instance\\.\\d+\\.(\\w+)\\(`, "gm"))].map((match) => match[1]));
+        for (const method of ["length", "compare", "clone", "find", "rfind", "contains", "starts_with", "ends_with", "slice", "sub", "join", "concat", "split", "trim", "trim_start", "trim_end", "to_ascii_lower", "to_ascii_upper", "repeat", "replace"]) {
+          assert.ok(methods.has(method), `${type} instance method ${method}`);
+        }
+      }
+    },
+  },
@@ string_library の参照値のループの次
+// D14: tests/fixtures/text_class/Main.tz runs the string_library operations through Text, so those references apply.
+{
+  suites.text_class.cases.push(...suites.string_library.cases.filter(([name]) => /^(text|utf8)_(operation|slice)$/.test(name)));
+  const whitespace = "[\\t\\n\\v\\f\\r ]";
+  for (const utf8 of [false, true]) {
+    const indices = stringSamples.map((_, index) => index).filter((index) => !utf8 || ![7, 8].includes(index));
+    const units = (text) => (utf8 ? Buffer.byteLength(text, "utf8") : text.length);
+    const prefix = utf8 ? "utf8" : "text";
+    for (const which of indices) {
+      const text = stringSamples[which];
+      const bytes = Buffer.from(text, "utf8");
+      const trimmedStart = text.replace(new RegExp(`^${whitespace}+`), "");
+      const trimmedEnd = text.replace(new RegExp(`${whitespace}+$`), "");
+      const trimmed = text.replace(new RegExp(`^${whitespace}+|${whitespace}+$`, "g"), "");
+      const extra = [units(text), Number(text === ""), textHash(trimmedStart, utf8), textHash(trimmedEnd, utf8), textHash(text, utf8), 0, units(trimmed)];
+      for (const [operation, expected] of extra.entries()) suites.text_class.cases.push([`${prefix}_extra`, [operation, BigInt(which)], BigInt(expected)]);
+      const length = units(text);
+      for (let first = -1; first <= length + 1; first++) for (let count = -1; count <= length + 1; count++) {
+        const boundary = (offset) => offset === 0 || offset === length || (bytes[offset] & 0xc0) !== 0x80;
+        const last = first + count;
+        const valid = first >= 0 && count >= 0 && last <= length && (!utf8 || (boundary(first) && boundary(last)));
+        const expected = valid ? textHash(utf8 ? bytes.subarray(first, last).toString("utf8") : text.slice(first, last), utf8) : -1n;
+        suites.text_class.cases.push([`${prefix}_sub`, [BigInt(which), BigInt(first), BigInt(count)], expected]);
+      }
+    }
+    for (const first of indices) for (const second of indices) {
+      const text = stringSamples[first], other = stringSamples[second];
+      const less = utf8 ? Buffer.compare(Buffer.from(text, "utf8"), Buffer.from(other, "utf8")) < 0 : text < other;
+      suites.text_class.cases.push([`${prefix}_order`, [BigInt(first), BigInt(second)], BigInt(Number(less) + 2 * Number(text === other))]);
+      const parts = other === "" && utf8 ? Array.from(text) : text.split(other);
+      suites.text_class.cases.push([`${prefix}_pieces`, [BigInt(first), BigInt(second)], BigInt(parts.length)]);
+    }
+  }
+}
```

参照値の作り方（JavaScript の標準の機能だけ。コンパイラの出力に依存しない）: `string` は JavaScript の文字列（UTF-16）の `indexOf`・`split`・`replaceAll`・`slice`・`<`、`utf8string` は `Buffer`（UTF-8 のバイト）の `indexOf`・`compare`・`subarray`。`utf8` の `slice`・`sub` は、先頭バイトが `10xxxxxx` でない位置（スカラーの境界）だけを有効にする。`utf8string` の 7・8 番（孤立サロゲート）は `Utf8String.from_string` がトラップするので除く。`extra` の 5 番目（`compare` を自分自身に）は 0。

実行すると `features: 4595 cases passed` になる（`string_library` から借りる `operation`・`slice` の件数と、`extra`・`sub`・`order`・`pieces`・`literal_arguments` の合計）。

### E.3 `tests/fixtures/text_class_direct/Main.tz` と `tests/fixtures/text_class_calls/Main.tz`（新規）

同じ計算を、`String.*`・`Utf8String.*`・`clone_string` を直接呼ぶ版（`text_class_direct`）と、`Text.*` で呼ぶ版（`text_class_calls`）で書いた対。`tests/text_class.mjs` が使う。字下げはタブ 1 個。**2 つのファイルは、関数の名前（`String.find` ↔ `Text.find` の対応）と、`String.length (ref text) == 0` ↔ `Text.is_empty (ref text)` の 1 か所以外は同じ**にしておく。

`text_class_direct/Main.tz`:

```tsuzuri
export def score :: i64 -> i64
fn score n =
	let base = "ab"
	let text = String.repeat (ref base) n
	let needle = "ba"
	let parts = [clone_string (ref text), clone_string (ref needle)]
	let joined = String.join (ref ",") (ref parts)
	let spaced = String.concat (ref [" ", clone_string (ref joined), " "])
	let a = Maybe.default_value (-1) (String.find (ref needle) (ref text))
	let b = Maybe.default_value (-1) (String.rfind (ref needle) (ref text))
	let c = (if String.contains (ref needle) (ref text) then 1 else 0) + (if String.starts_with (ref needle) (ref text) then 2 else 0) + (if String.ends_with (ref needle) (ref text) then 4 else 0)
	let d = match String.slice (ref text) 1 3 with | Some part -> String.length (ref part) | None -> -1
	let e = match String.sub (ref text) 1 2 with | Some part -> String.length (ref part) | None -> -1
	let f = (String.split (ref "a") (ref text)).length
	let g = String.length (ref (String.trim (ref spaced))) + String.length (ref (String.trim_start (ref spaced))) + String.length (ref (String.trim_end (ref spaced)))
	let h = String.length (ref (String.to_ascii_upper (ref text))) + String.length (ref (String.to_ascii_lower (ref text)))
	let i = String.length (ref (String.replace (ref "a") (ref "xyz") (ref text)))
	let j = String.compare (ref text) (ref needle) + (if String.length (ref text) == 0 then 100 else 0)
	a + b + c + d + e + f + g + h + i + j + String.length (ref joined)

export def score8 :: i64 -> i64
fn score8 n =
	let base = u8"ab"
	let text = Utf8String.repeat (ref base) n
	let needle = u8"ba"
	let parts = [Utf8String.clone (ref text), Utf8String.clone (ref needle)]
	let joined = Utf8String.join (ref u8",") (ref parts)
	let spaced = Utf8String.concat (ref [u8" ", Utf8String.clone (ref joined), u8" "])
	let a = Maybe.default_value (-1) (Utf8String.find (ref needle) (ref text))
	let b = Maybe.default_value (-1) (Utf8String.rfind (ref needle) (ref text))
	let c = (if Utf8String.contains (ref needle) (ref text) then 1 else 0) + (if Utf8String.starts_with (ref needle) (ref text) then 2 else 0) + (if Utf8String.ends_with (ref needle) (ref text) then 4 else 0)
	let d = match Utf8String.slice (ref text) 1 3 with | Some part -> Utf8String.length (ref part) | None -> -1
	let e = match Utf8String.sub (ref text) 1 2 with | Some part -> Utf8String.length (ref part) | None -> -1
	let f = (Utf8String.split (ref u8"a") (ref text)).length
	let g = Utf8String.length (ref (Utf8String.trim (ref spaced))) + Utf8String.length (ref (Utf8String.trim_start (ref spaced))) + Utf8String.length (ref (Utf8String.trim_end (ref spaced)))
	let h = Utf8String.length (ref (Utf8String.to_ascii_upper (ref text))) + Utf8String.length (ref (Utf8String.to_ascii_lower (ref text)))
	let i = Utf8String.length (ref (Utf8String.replace (ref u8"a") (ref u8"xyz") (ref text)))
	let j = Utf8String.compare (ref text) (ref needle) + (if Utf8String.length (ref text) == 0 then 100 else 0)
	a + b + c + d + e + f + g + h + i + j + Utf8String.length (ref joined)
```

`text_class_calls/Main.tz`:

```tsuzuri
export def score :: i64 -> i64
fn score n =
	let base = "ab"
	let text = Text.repeat (ref base) n
	let needle = "ba"
	let parts = [Text.clone (ref text), Text.clone (ref needle)]
	let joined = Text.join (ref ",") (ref parts)
	let spaced = Text.concat (ref [" ", Text.clone (ref joined), " "])
	let a = Maybe.default_value (-1) (Text.find (ref needle) (ref text))
	let b = Maybe.default_value (-1) (Text.rfind (ref needle) (ref text))
	let c = (if Text.contains (ref needle) (ref text) then 1 else 0) + (if Text.starts_with (ref needle) (ref text) then 2 else 0) + (if Text.ends_with (ref needle) (ref text) then 4 else 0)
	let d = match Text.slice (ref text) 1 3 with | Some part -> Text.length (ref part) | None -> -1
	let e = match Text.sub (ref text) 1 2 with | Some part -> Text.length (ref part) | None -> -1
	let f = (Text.split (ref "a") (ref text)).length
	let g = Text.length (ref (Text.trim (ref spaced))) + Text.length (ref (Text.trim_start (ref spaced))) + Text.length (ref (Text.trim_end (ref spaced)))
	let h = Text.length (ref (Text.to_ascii_upper (ref text))) + Text.length (ref (Text.to_ascii_lower (ref text)))
	let i = Text.length (ref (Text.replace (ref "a") (ref "xyz") (ref text)))
	let j = Text.compare (ref text) (ref needle) + (if Text.is_empty (ref text) then 100 else 0)
	a + b + c + d + e + f + g + h + i + j + Text.length (ref joined)

export def score8 :: i64 -> i64
fn score8 n =
	let base = u8"ab"
	let text = Text.repeat (ref base) n
	let needle = u8"ba"
	let parts = [Text.clone (ref text), Text.clone (ref needle)]
	let joined = Text.join (ref u8",") (ref parts)
	let spaced = Text.concat (ref [u8" ", Text.clone (ref joined), u8" "])
	let a = Maybe.default_value (-1) (Text.find (ref needle) (ref text))
	let b = Maybe.default_value (-1) (Text.rfind (ref needle) (ref text))
	let c = (if Text.contains (ref needle) (ref text) then 1 else 0) + (if Text.starts_with (ref needle) (ref text) then 2 else 0) + (if Text.ends_with (ref needle) (ref text) then 4 else 0)
	let d = match Text.slice (ref text) 1 3 with | Some part -> Text.length (ref part) | None -> -1
	let e = match Text.sub (ref text) 1 2 with | Some part -> Text.length (ref part) | None -> -1
	let f = (Text.split (ref u8"a") (ref text)).length
	let g = Text.length (ref (Text.trim (ref spaced))) + Text.length (ref (Text.trim_start (ref spaced))) + Text.length (ref (Text.trim_end (ref spaced)))
	let h = Text.length (ref (Text.to_ascii_upper (ref text))) + Text.length (ref (Text.to_ascii_lower (ref text)))
	let i = Text.length (ref (Text.replace (ref u8"a") (ref u8"xyz") (ref text)))
	let j = Text.compare (ref text) (ref needle) + (if Text.is_empty (ref text) then 100 else 0)
	a + b + c + d + e + f + g + h + i + j + Text.length (ref joined)
```

`text_class_calls` は 20 メソッドと `is_empty` を、`string` と `utf8string` のそれぞれに 1 回以上使う。

### E.4 `tests/text_class.mjs`（新規。`node tests/text_class.mjs [コンパイラ] [--objects]`）

IR の構造を検査する（既定）と、オブジェクトコードの一致を測って表にする（`--objects`。表明ではない）。`tests/` の他の `.mjs` と同じく、リポジトリのルートで実行する。作業用の `target/text-class-tests` は、終わりに消す。

```js
// D14: a call of `Text.f` is a direct call of an instance method, and that method only calls `String.f` or
// `Utf8String.f` (the Text calls of tests/fixtures/text_class_calls against the direct calls of text_class_direct).
// Always checked, on the LLVM IR that `--emit llvm` writes: every instance method has exactly one call, whose callee is
// the function of the same name; the callers contain no indirect call; the direct program has no instance method.
// With `--objects` it also builds both programs at the optimization levels and prints whether the object code (native)
// and the WebAssembly module are byte-identical. That table is a measurement, not an assertion: LLVM's inlining
// decisions can move a few instructions in a large program.
// Usage: node tests/text_class.mjs [compiler] [--objects]
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

const compiler = resolve(process.argv.slice(2).find((argument) => !argument.startsWith('--')) ?? 'target/release/tsuzuri');
const scratch = resolve('target/text-class-tests');
rmSync(scratch, { recursive: true, force: true });
mkdirSync(scratch, { recursive: true });

function build(fixture, output, options) {
  const result = spawnSync(compiler, ['build', resolve('tests/fixtures', fixture), '-o', output, ...options], { encoding: 'utf8', timeout: 180000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${fixture} ${options.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return readFileSync(output);
}

const methods = ['length', 'compare', 'clone', 'find', 'rfind', 'contains', 'starts_with', 'ends_with', 'slice', 'sub', 'join', 'concat', 'split', 'trim', 'trim_start', 'trim_end', 'to_ascii_lower', 'to_ascii_upper', 'repeat', 'replace'];
const ir = build('text_class_calls', join(scratch, 'calls.ll'), ['--emit', 'llvm', '-O0']).toString('utf8');
const bodies = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.(String|Utf8String)\.\$instance\.\d+\.(\w+)\([^\n]*\{\n([\s\S]*?)^\}/gm)];
assert.equal(bodies.length, 2 * methods.length, 'the instance methods of both types that the program calls');
for (const [, type, method, body] of bodies) {
  const calls = body.split('\n').filter((line) => /\bcall\b/.test(line));
  assert.equal(calls.length, 1, `${type}.${method} makes one call`);
  const callee = method === 'clone' ? (type === 'String' ? '@tz.fn.$builtin.clone_string' : '@tz.fn.$builtin.Utf8String.clone')
    : method === 'compare' ? `@tz.fn.$builtin.${type}.compare` : `@tz.fn.${type}.${method}`;
  assert.ok(calls[0].includes(`${callee}(`), `${type}.${method} calls ${callee}: ${calls[0]}`);
}
for (const type of ['String', 'Utf8String']) {
  assert.deepEqual([...new Set(bodies.filter((body) => body[1] === type).map((body) => body[2]))].sort(), [...methods].sort(), `${type} instance methods`);
}
for (const name of ['score', 'score8']) {
  const body = ir.match(new RegExp(`^define internal [^\\n]*@tz\\.fn\\.Main\\.${name}\\([^\\n]*\\{\\n([\\s\\S]*?)^\\}`, 'm'))[1];
  assert.doesNotMatch(body, /\bcall [^\n]*%[A-Za-z0-9_.]+\(/, `${name} has no indirect call`);
  assert.match(body, /\bcall [^\n]*@tz\.fn\.String\.\$instance\.\d+\.length\(|\bcall [^\n]*@tz\.fn\.Utf8String\.\$instance\.\d+\.length\(/, `${name} calls an instance method directly`);
}
assert.doesNotMatch(ir, /@tz\.dyn\.|%tz\.hkt/, 'Text calls are resolved at compile time');
const directIr = build('text_class_direct', join(scratch, 'direct.ll'), ['--emit', 'llvm', '-O0']).toString('utf8');
assert.doesNotMatch(directIr, /\$instance\.\d+\.length/, 'the direct program calls no instance method');
console.log(`text class: ${bodies.length} instance methods, each one call; no indirect call`);

if (process.argv.includes('--objects')) {
  for (const [kind, extension, extra, levels] of [
    ['native object', 'o', ['--emit', 'object'], ['-O1', '-O2', '-O3', '-Os', '-Oz']],
    ['wasm32', 'wasm', ['--target', 'wasm32'], ['-O1', '-O2', '-O3']],
  ]) {
    for (const level of levels) {
      const direct = build('text_class_direct', join(scratch, `direct${level}.${extension}`), [...extra, level]);
      const calls = build('text_class_calls', join(scratch, `calls${level}.${extension}`), [...extra, level]);
      console.log(`${kind} ${level}: ${direct.equals(calls) ? 'identical' : 'differs'} (${direct.length} and ${calls.length} bytes)`);
    }
  }
}
rmSync(scratch, { recursive: true, force: true });
```

試作の出力（既定）: `text class: 40 instance methods, each one call; no indirect call`。`--objects` の表は B.3。`bodies.length == 40` は、`text_class_calls` が 20 メソッドを 2 つの型に使う数。`is_empty` は既定の実装で、IR では `@tz.fn.Text.$instance.default.<番号>.is_empty.$mono.<番号>`（モジュール名が `Text`）という別の関数になり、`length` の instance のメソッドを 1 回呼んで 0 と比べる。名前が `String`・`Utf8String` で始まらないので、40 に入らない。**instance のメソッドの IR の名前**は `@tz.fn.<型>.$instance.<番号>.<メソッド>`（番号は動く）。組み込みの関数を呼ぶ 4 つのメソッドの呼び先は `@tz.fn.$builtin.clone_string`・`@tz.fn.$builtin.Utf8String.clone`・`@tz.fn.$builtin.String.compare`・`@tz.fn.$builtin.Utf8String.compare`（試作の IR で確認済み）。

### E.5 `tests/text_class.rs`（新規）と `tests/fixtures/text_user_instance/Main.tz`（新規）

型検査と診断のテスト。値は E.2、IR の形は E.4 が受け持つ。補助の `accepts` は `tests/strings.rs` の同名の関数と同じ（型検査し、両ターゲットのライブラリの IR が決定的で、重複した宣言がないことを確かめる）。**試作では Rust のテストを `cargo test` でコンパイルしていない**（この作業では `cargo` を走らせない決まり）ので、実装者は最初に `cargo test --locked --test text_class` を走らせ、`running 9 tests`（Phase 2 のあとは 12）と出ることを確かめる。コンパイルエラーは、型の名前（`tsuzuri::analyze`・`tsuzuri::analyze_modules`・`tsuzuri::llvm`。`tests/strings.rs`・`tests/stdlib.rs` が同じ名前を使っている）を直す。期待する診断のコードとメッセージは、すべて試作の `tsuzuri check` の出力から写した。

`tests/fixtures/text_user_instance/Main.tz`（ユーザーの型が `Text` になる例。`run` の出力は `3 false Ada`。メソッドの本体はタブ 1 個で字下げする）:

```tsuzuri
// A user type can be a Text too: it needs Ord and every method (is_empty has a default).
record Name { value: string } deriving (Eq, Ord)

instance Text<Name> {
	fn length text = text.value.length
	fn compare left right = String.compare (ref left.value) (ref right.value)
	fn clone text = Name { value: clone_string (ref text.value) }
	fn find needle text = String.find (ref needle.value) (ref text.value)
	fn rfind needle text = String.rfind (ref needle.value) (ref text.value)
	fn contains needle text = String.contains (ref needle.value) (ref text.value)
	fn starts_with prefix text = String.starts_with (ref prefix.value) (ref text.value)
	fn ends_with suffix text = String.ends_with (ref suffix.value) (ref text.value)
	fn slice text first last = match String.slice (ref text.value) first last with | Some part -> Some (Name { value: part }) | None -> None
	fn sub text first count = match String.sub (ref text.value) first count with | Some part -> Some (Name { value: part }) | None -> None
	fn join separator parts = Name { value: String.join (ref separator.value) (ref (Array.map_ref (\part -> clone_string (ref part.value)) parts)) }
	fn concat parts = Name { value: String.concat (ref (Array.map_ref (\part -> clone_string (ref part.value)) parts)) }
	fn split separator text =
		let pieces = String.split (ref separator.value) (ref text.value)
		new [Name](pieces.length, index -> Name { value: clone_string (ref pieces[index]) })
	fn trim text = Name { value: String.trim (ref text.value) }
	fn trim_start text = Name { value: String.trim_start (ref text.value) }
	fn trim_end text = Name { value: String.trim_end (ref text.value) }
	fn to_ascii_lower text = Name { value: String.to_ascii_lower (ref text.value) }
	fn to_ascii_upper text = Name { value: String.to_ascii_upper (ref text.value) }
	fn repeat text count = Name { value: String.repeat (ref text.value) count }
	fn replace needle replacement text = Name { value: String.replace (ref needle.value) (ref replacement.value) (ref text.value) }
}

let name = Name { value: "  Ada  " }
let trimmed = Text.trim name
$"{Text.length trimmed} {Text.is_empty trimmed} {trimmed.value}"
```

`tests/text_class.rs`（Phase 1 の 9 件。`LITERAL_LINES` は E.1 の `literal_arguments` と同じ規則の、全メソッドの 30 行。`AUTO_BORROWED_LINES` は `ref` を付けないリテラルの 12 行）:

```rust
// D14: the Text class of std/Text.tt and its instances for string and utf8string (std/String.tz, std/Utf8String.tz).
// The values of the methods are tested by `node tests/features.mjs <compiler> text_class`, and the shape of the
// generated code by `node tests/text_class.mjs`; this file tests what type checking accepts and reports.
use tsuzuri::{analyze, analyze_modules, llvm};

fn checks(source: &str) {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
}

/// The program type-checks; the library IR of both targets is deterministic and has no duplicate declaration.
fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let mut native = String::new();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
        assert!(llvm::duplicate_declarations(&ir).is_empty(), "{ir}");
        if !wasm {
            native = ir;
        }
    }
    native
}

fn rejects(source: &str, code: &str, message: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{source}\n{}",
        error.message
    );
}

fn rejects_modules(sources: &[(&str, &str)], code: &str, message: &str) {
    let error = analyze_modules(sources).expect_err(&format!("{sources:?}"));
    assert_eq!(error.code, code, "{sources:?}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{sources:?}\n{}",
        error.message
    );
}

const LITERAL_LINES: &str = r#"let m01 = Text.length (ref "ab")
let m02 = Text.is_empty (ref "ab")
let m03 = Text.compare (ref "a") (ref "b")
let m04 = Text.clone (ref "ab")
let m05 = Text.find (ref "a") (ref "ab")
let m06 = Text.rfind (ref "a") (ref "ab")
let m07 = Text.contains (ref "a") (ref "ab")
let m08 = Text.starts_with (ref "a") (ref "ab")
let m09 = Text.ends_with (ref "b") (ref "ab")
let m10 = Text.slice (ref "abc") 0 1
let m11 = Text.sub (ref "abc") 0 1
let m12 = Text.join (ref ",") (ref ["a", "b"])
let m13 = Text.concat (ref ["a", "b"])
let m14 = Text.split (ref ",") (ref "a,b")
let m15 = Text.trim (ref " a ")
let m16 = Text.trim_start (ref " a ")
let m17 = Text.trim_end (ref " a ")
let m18 = Text.to_ascii_lower (ref "A")
let m19 = Text.to_ascii_upper (ref "a")
let m20 = Text.repeat (ref "ab") 2
let m21 = Text.replace (ref "a") (ref "b") (ref "aa")
let d01 = String.compare (ref "a") (ref "b")
let d02 = String.repeat (ref "ab") 2
let d03 = String.replace (ref "a") (ref "b") (ref "aa")
let d04 = String.find (ref "a") (ref "ab")
let d05 = String.slice (ref "abc") 0 1
let d06 = Utf8String.compare (ref u8"a") (ref u8"b")
let m22 = Text.compare (ref u8"a") (ref u8"b")
let m23 = Text.repeat (ref u8"ab") 2
let m24 = Text.find (ref u8"a") (ref u8"ab")
"#;

const AUTO_BORROWED_LINES: &str = r#"let m03 = Text.compare "a" "b"
let m05 = Text.find "a" "ab"
let m07 = Text.contains "a" "ab"
let m10 = Text.slice "abc" 0 1
let m12 = Text.join "," ["a", "b"]
let m14 = Text.split "," "a,b"
let m20 = Text.repeat "ab" 2
let m21 = Text.replace "a" "b" "aa"
let m22 = Text.repeat u8"ab" 2
let m23 = Text.contains u8"a" u8"ab"
let m24 = Text.contains (ref "a") "ab"
let m25 = Text.contains "a" (ref "ab")
"#;

const USER_INSTANCE: &str = include_str!("fixtures/text_user_instance/Main.tz");

#[test]
fn every_method_type_checks_for_both_encodings() {
    // 20 methods and is_empty on a string and a utf8string, and generic functions over Text.
    accepts(include_str!("fixtures/text_class_calls/Main.tz"));
    accepts(include_str!("fixtures/text_class/Main.tz"));
}

#[test]
fn generic_code_compares_texts_through_the_ord_superclass() {
    accepts(
        "def earlier :: Text<'s> => ref 's -> ref 's -> bool\nfn earlier left right = deref left < deref right\n\n\
         export def a :: bool\nfn a = earlier \"apple\" \"banana\"\n\
         export def b :: bool\nfn b = earlier u8\"banana\" u8\"apple\"\n",
    );
}

#[test]
fn every_method_takes_a_literal_argument_without_a_let() {
    for line in LITERAL_LINES.lines().chain(AUTO_BORROWED_LINES.lines()) {
        checks(&format!("{line}\n0\n"));
    }
}

#[test]
fn partial_application_and_loan_returning_methods_still_need_a_let() {
    let borrow = "borrow requires a local place";
    rejects("let f = Text.find \"a\"\n0\n", "E1013", borrow);
    // The same rule for a class of the user (a method that returns a reference may hold a loan on a temporary).
    let class = "class Pick<'a> {\n    def pick :: ref 'a -> ref 'a -> ref 'a\n    def both :: ref 'a -> ref 'a -> i64\n}\n";
    let main = "instance Pick<string> {\n    fn pick left right = left\n    fn both left right = left.length + right.length\n}\n\nlet total = Pick.both \"ab\" \"cde\"\ntotal\n";
    analyze_modules(&[("Main.tz", main), ("Traits.tt", class)]).unwrap();
    let kept = main.replace("total\n", "let kept = Pick.pick \"ab\" \"cde\"\ntotal\n");
    rejects_modules(&[("Main.tz", &kept), ("Traits.tt", class)], "E1013", borrow);
}

#[test]
fn a_user_type_can_be_a_text() {
    checks(USER_INSTANCE);
    // The superclass Ord (and its superclass Eq) is required.
    let no_ord = USER_INSTANCE.replace(
        "record Name { value: string } deriving (Eq, Ord)",
        "record Name { value: string }",
    );
    assert_ne!(no_ord, USER_INSTANCE);
    rejects(&no_ord, "E1027", "no instance for Eq<Main.Name>");
    // Every method except is_empty is required.
    let no_trim_end = USER_INSTANCE.replace(
        "\tfn trim_end text = Name { value: String.trim_end (ref text.value) }\n",
        "",
    );
    assert_ne!(no_trim_end, USER_INSTANCE);
    rejects(&no_trim_end, "E1016", "missing method 'Text.Text.trim_end'");
}

#[test]
fn wrong_uses_report_the_existing_diagnostics() {
    rejects("let n = Text.length (ref 5)\nn\n", "E1005", "no instance for Text.Text<i32>");
    rejects("let found = Text.find \"a\" u8\"abc\"\n0\n", "E1003", "expected string, found utf8string");
    rejects("let flipped = Text.reverse \"abc\"\n0\n", "E1002", "type class 'Text' has no method 'reverse'");
    rejects("def size :: dyn Text -> i64\nfn size text = 0\n\n0\n", "E1028", "dyn Text is not allowed");
}

#[test]
fn text_is_a_reserved_module_and_namespace() {
    let reserved = "a name reserved for the standard library";
    rejects_modules(&[("Main.tz", "namespace Text\n\n0\n")], "E1011", reserved);
    rejects_modules(&[("Main.tz", "namespace Text::Tools\n\n0\n")], "E1011", reserved);
}

#[test]
fn declarations_named_text_coexist_with_the_class() {
    // A record: the std class is still Text.f.
    checks("record Text { value: string }\ndef length_of :: ref Text -> i64\nfn length_of text = text.value.length\nlet a = Text { value: \"abc\" }\nlet b = Text.length (ref \"abcd\")\nlength_of (ref a) + b\n");
    // A union: Text.f names the union, so the std class is Text.Text.f.
    checks("union Text = Plain | Rich\n\ndef label :: Text -> i64\nfn label text = match text with\n    | Plain -> 1\n    | Rich -> 2\n\nlabel Plain + Text.Text.length \"abc\"\n");
    rejects("union Text = Plain | Rich\nlet size = Text.length \"abc\"\n0\n", "E1002", "union 'Main.Text' has no case 'length'");
    // A class: its methods are Text.size, and the std class is Text.Text.length.
    let class = "class Text<'a> {\n    def size :: ref 'a -> i64\n}\n";
    let main = "instance Text<string> {\n    fn size value = value.length\n}\n\ndef twice :: Text<'a> => ref 'a -> i64\nfn twice value = Text.size value * 2\n\ndef std_length :: Text.Text<'a> => ref 'a -> i64\nfn std_length value = Text.Text.length value\n\nlet word = \"abc\"\nlet other = u8\"abcd\"\ntwice (ref word) + std_length (ref word) + std_length (ref other)\n";
    analyze_modules(&[("Main.tz", main), ("Traits.tt", class)]).unwrap();
}

#[test]
fn unused_text_instances_never_reach_the_ir() {
    let ir = accepts("export def answer :: i64\nfn answer = String.length \"abc\" + Utf8String.length u8\"abcd\"\n");
    assert!(!ir.contains("@tz.fn.String.$instance"), "{ir}");
    assert!(!ir.contains("@tz.fn.Utf8String.$instance"), "{ir}");
}
```

テスト名（本文の T 番号との対応は本文の「テスト計画」）: `every_method_type_checks_for_both_encodings`（T1）、`generic_code_compares_texts_through_the_ord_superclass`（T2）、`every_method_takes_a_literal_argument_without_a_let`（T3）、`partial_application_and_loan_returning_methods_still_need_a_let`（T4）、`a_user_type_can_be_a_text`（T5）、`wrong_uses_report_the_existing_diagnostics`（T6）、`text_is_a_reserved_module_and_namespace`（T7）、`declarations_named_text_coexist_with_the_class`（T8）、`unused_text_instances_never_reach_the_ir`（T9）。

## F. Phase 2 のテスト

### F.1 `tests/fixtures/regex_text/Main.tz`（新規）

`Regex.*_text` を `string` と `utf8string` に使い、JavaScript の `RegExp` の結果と照合する（F.2）。`same_as_direct` は、同じ引数で `_text` の版と直接の版（`Regex.find`・`Regex.find_utf8` …）が同じ値を返すことを 1 つの関数で確かめる（`Maybe` と配列には `==` がないので、`first_span`・`group_count`・`span_hash`・`hash` に畳んで比べる）。字下げはスペース 4 個。

```tsuzuri
def sample :: i64 -> string
fn sample index = match index with
    | 0 -> ""
    | 1 -> "abc"
    | 2 -> "a1b22c333"
    | 3 -> "\u{1F600}7\u{1F600}"
    | 4 -> "007 and 42"
    | _ -> "x"

def utf8_sample :: i64 -> utf8string
fn utf8_sample index =
    let text = sample index
    Utf8String.from_string (ref text)

def hash :: string -> i64
fn hash text =
    let mut value = text.length
    for unit in text do value = value * 31 + unit as i64
    value

def hash_utf8 :: utf8string -> i64
fn hash_utf8 text =
    let mut value = text.length
    for byte in text do value = value * 31 + byte as i64
    value

def digits :: unit -> Regex
fn digits _ = Result.get (Regex.compile (ref "\\d+"))

def pairs :: unit -> Regex
fn pairs _ = Result.get (Regex.compile (ref "(\\d)(\\d*)"))

def span_hash :: ref [(i64 * i64)] -> i64
fn span_hash spans =
    let mut total = spans.length
    for index in 0 .. (spans.length - 1) do
        match spans[index] with | (first, last) -> total = total * 31 + first * 7 + last
    total

def first_span :: Maybe<(i64 * i64)> -> i64
fn first_span found = match found with | Some (first, last) -> first * 1000 + last | None -> -1

def group_count :: Maybe<[(i64 * i64)]> -> i64
fn group_count found = match found with | Some spans -> spans.length | None -> -1

export def text_spans :: i64 -> i64
fn text_spans which =
    let re = digits ()
    let text = sample which
    span_hash (ref (Regex.find_all_text (ref re) (ref text)))

export def utf8_spans :: i64 -> i64
fn utf8_spans which =
    let re = digits ()
    let text = utf8_sample which
    span_hash (ref (Regex.find_all_text (ref re) (ref text)))

export def text_first :: i64 -> i64
fn text_first which =
    let re = digits ()
    let text = sample which
    first_span (Regex.find_text (ref re) (ref text))

export def utf8_first :: i64 -> i64
fn utf8_first which =
    let re = digits ()
    let text = utf8_sample which
    first_span (Regex.find_text (ref re) (ref text))

export def text_replaced :: i64 -> i64
fn text_replaced which =
    let re = digits ()
    let text = sample which
    hash (Regex.replace_all_text (ref re) (ref text) "<${0}>")

export def utf8_replaced :: i64 -> i64
fn utf8_replaced which =
    let re = digits ()
    let text = utf8_sample which
    hash_utf8 (Regex.replace_all_text (ref re) (ref text) u8"<${0}>")

export def text_pieces :: i64 -> i64
fn text_pieces which =
    let re = digits ()
    let text = sample which
    (Regex.split_text (ref re) (ref text)).length

export def utf8_pieces :: i64 -> i64
fn utf8_pieces which =
    let re = digits ()
    let text = utf8_sample which
    (Regex.split_text (ref re) (ref text)).length

export def text_groups :: i64 -> i64
fn text_groups which =
    let re = pairs ()
    let text = sample which
    group_count (Regex.captures_text (ref re) (ref text))

export def utf8_groups :: i64 -> i64
fn utf8_groups which =
    let re = pairs ()
    let text = utf8_sample which
    group_count (Regex.captures_text (ref re) (ref text))

// Every *_text function gives what the function for the same text type gives.
export def same_as_direct :: i64 -> i64
fn same_as_direct which =
    let re = digits ()
    let both = pairs ()
    let text = sample which
    let bytes = utf8_sample which
    let dash = "-"
    let dash8 = u8"-"
    let strings =
        Regex.is_match_text (ref re) (ref text) == Regex.is_match (ref re) (ref text)
        && first_span (Regex.find_text (ref re) (ref text)) == first_span (Regex.find (ref re) (ref text))
        && first_span (Regex.find_at_text (ref re) (ref text) 1) == first_span (Regex.find_at (ref re) (ref text) 1)
        && span_hash (ref (Regex.find_all_text (ref re) (ref text))) == span_hash (ref (Regex.find_all (ref re) (ref text)))
        && group_count (Regex.captures_text (ref both) (ref text)) == group_count (Regex.captures (ref both) (ref text))
        && hash (Regex.replace_all_text (ref re) (ref text) (ref dash)) == hash (Regex.replace_all (ref re) (ref text) (ref dash))
        && (Regex.split_text (ref re) (ref text)).length == (Regex.split (ref re) (ref text)).length
    let utf8 =
        Regex.is_match_text (ref re) (ref bytes) == Regex.is_match_utf8 (ref re) (ref bytes)
        && first_span (Regex.find_text (ref re) (ref bytes)) == first_span (Regex.find_utf8 (ref re) (ref bytes))
        && first_span (Regex.find_at_text (ref re) (ref bytes) 1) == first_span (Regex.find_at_utf8 (ref re) (ref bytes) 1)
        && span_hash (ref (Regex.find_all_text (ref re) (ref bytes))) == span_hash (ref (Regex.find_all_utf8 (ref re) (ref bytes)))
        && group_count (Regex.captures_text (ref both) (ref bytes)) == group_count (Regex.captures_utf8 (ref both) (ref bytes))
        && hash_utf8 (Regex.replace_all_text (ref re) (ref bytes) (ref dash8)) == hash_utf8 (Regex.replace_all_utf8 (ref re) (ref bytes) (ref dash8))
        && (Regex.split_text (ref re) (ref bytes)).length == (Regex.split_utf8 (ref re) (ref bytes)).length
    if strings && utf8 then 1 else 0
```

### F.2 `tests/features.mjs` の差分（Phase 2 の分）

1 つめの塊は、Phase 1 の `text_class` の suite の次（`string_library` の前）。2 つめの塊は、Phase 1 の `D14: tests/fixtures/text_class/Main.tz …` の塊の次（`hashBytes` の前）。

```diff
@@ suites（text_class の次）
+  // D14 Phase 2: Regex.*_text gives the values of the direct Regex.* and Regex.*_utf8 calls; the references are JavaScript RegExp.
+  regex_text: {
+    cases: [],
+    traps: [],
+  },
@@ text_class の参照値の塊の次
+// D14 Phase 2: independent references for tests/fixtures/regex_text/Main.tz. A span is in UTF-16 units for string and
+// in bytes for utf8string.
+{
+  const samples = ["", "abc", "a1b22c333", "\u{1f600}7\u{1f600}", "007 and 42", "x"];
+  for (const utf8 of [false, true]) {
+    const prefix = utf8 ? "utf8" : "text";
+    const offset = (text, index) => (utf8 ? Buffer.byteLength(text.slice(0, index), "utf8") : index);
+    for (const [which, text] of samples.entries()) {
+      const spans = [...text.matchAll(/\d+/g)].map((found) => [offset(text, found.index), offset(text, found.index + found[0].length)]);
+      const spanHash = spans.reduce((total, [first, last]) => wrapHash(total * 31n + BigInt(first * 7 + last)), BigInt(spans.length));
+      const first = spans.length ? BigInt(spans[0][0] * 1000 + spans[0][1]) : -1n;
+      suites.regex_text.cases.push([`${prefix}_spans`, [BigInt(which)], spanHash]);
+      suites.regex_text.cases.push([`${prefix}_first`, [BigInt(which)], first]);
+      suites.regex_text.cases.push([`${prefix}_replaced`, [BigInt(which)], textHash(text.replace(/\d+/g, "<$&>"), utf8)]);
+      suites.regex_text.cases.push([`${prefix}_pieces`, [BigInt(which)], BigInt(text.split(/\d+/).length)]);
+      suites.regex_text.cases.push([`${prefix}_groups`, [BigInt(which)], /(\d)(\d*)/.test(text) ? 3n : -1n]);
+    }
+  }
+  for (const which of samples.keys()) suites.regex_text.cases.push(["same_as_direct", [BigInt(which)], 1n]);
+}
```

実行すると `regex_text: native/WASM at O0/O3 passed`、`features: 66 cases passed`（6 個の文 × 2 つの型 × 5 種類 ＝ 60 件 ＋ `same_as_direct` の 6 件）。`\d` は ASCII の数字だけなので、JavaScript の `\d` と同じ。

### F.3 `tests/text_class.rs` への追記（Phase 2。3 件）

```rust
#[test]
fn regex_text_functions_take_both_encodings() {
    accepts(include_str!("fixtures/regex_text/Main.tz"));
}

#[test]
fn regex_text_functions_reject_a_text_without_an_instance() {
    rejects(
        "let re = Result.get (Regex.compile (ref \"a\"))\nlet n = 5\nlet found = Regex.is_match_text (ref re) (ref n)\nfound\n",
        "E1005",
        "no instance for RegexText.RegexText<i32>",
    );
}

#[test]
fn the_regex_text_class_is_named_with_its_module_outside_std() {
    rejects(
        "def count :: RegexText<'s> => ref Regex -> ref 's -> i64\nfn count re text = (Regex.find_all_text re text).length\n\n0\n",
        "E1016",
        "unknown type class 'RegexText'",
    );
    rejects(
        "let re = Result.get (Regex.compile (ref \"a\"))\nlet found = RegexText.find_all (ref re) (ref \"a1\")\nfound.length\n",
        "E1002",
        "module 'RegexText' has no function or union case 'find_all'",
    );
    checks("def count :: RegexText.RegexText<'s> => ref Regex -> ref 's -> i64\nfn count re text = (Regex.find_all_text re text).length\n\n0\n");
    checks("let re = Result.get (Regex.compile (ref \"a\"))\nlet found = RegexText.RegexText.find_all (ref re) (ref \"a1a\")\nfound.length\n");
}
```

上の 3 つ目の最後の 2 つの `checks` の入力は、試作で `tsuzuri check` が通り、`run` が `2`（`"a1a"` の `a` の数）を出した。



