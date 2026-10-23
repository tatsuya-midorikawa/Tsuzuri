# D13 付録: 実測に使ったプログラム・複製での実測・fixture・テスト・文書の置き換え文

本文は [D13-interpolation-hole-temporaries.md](D13-interpolation-hole-temporaries.md)。HEAD は `a2313d19`（作業ツリーの `9dd0fbbc` との差は `docs/benchmarks.md` だけ）。

以下は起票時の実測と設計資料です。現在の実装・検証結果とコマンドの読み替えは、本文末尾の「実装と検証（2026-10-23）」にあります。

## 付録 A: 決定表の全形を 1 つのファイルにしたプログラム

`/tmp/tz-D13/rows/Main.tz`（1 プログラム 1 ディレクトリ。`check` はファイルを渡しても同じ親ディレクトリの兄弟ファイルをすべて読む）に置いて `tsuzuri check /tmp/tz-D13/rows` を流す。**関数ごとに穴の形を 1 つだけ持つ**ので、所有権の検査の `E1013`・`E1012`・`E1014` は関数ごとに 1 件ずつ出て、全形の結果が 1 回で見える（型の誤りがあると所有権の検査へ進まないので、型は通してある）。

```text
record Pt { x: i64, name: string }
record Line { a: Pt, b: Pt }
record Pair { a: i64, b: i64 }
record Res { id: i64, name: string }

instance Drop<Res> {
    fn drop _value = ()
}

const Nums: [i64] = [10, 20, 30]
const Corner: Pair = Pair { a: 3, b: 4 }
const Origin: Pt = Pt { x: 5, name: "origin" }
const Words: [string] = ["a", "bb", "ccc"]
const Title: string = "title"

def id :: i64 -> i64
fn id n = n

def mk :: i64 -> Pt
fn mk n = Pt { x: n, name: "seven" }

def mkres :: i64 -> Res
fn mkres n = Res { id: n, name: "res" }

def make3 :: i64 -> [i64]
fn make3 n = [n, n + 1, n + 2]

def names :: unit -> [string]
fn names _unit = ["a", "bb"]

def mkref :: &i64 -> &i64
fn mkref r = r

def bump :: &mut i64 -> i64
fn bump r =
    deref r = deref r + 1
    deref r

def poke :: &mut [i64] -> i64
fn poke a =
    Array.write 0 9 a
    7

// places and references: accepted at HEAD
def p01 :: string -> i64 -> string = \s n -> $"{s}{n}"
def p02 :: Pt -> Line -> string = \p ln -> $"{p.name}{ln.a.x}"
def p03 :: [i64] -> [string] -> i64 -> string = \xs ws i -> $"{xs[0]}{xs[i]}{ws[1]}"
def p04 :: &[i64] -> string = \rx -> $"{rx[0]}"
def p05 :: &i64 -> &Pt -> string = \r rp -> $"{deref r}{*r}{rp.x}{(deref rp).x}{deref (mkref r)}"
def p06 :: i64 -> string = \i -> $"{Nums[1]}{Nums[i]}{Corner.a}"
def p07 :: [i64] -> string = \xs -> $"{xs[xs.length - 1]}"
def r01 :: &i64 -> &string -> string = \r rs -> $"{r}{rs}"
def r02 :: i64 -> [i64] -> string = \n xs -> $"{ref n}{&n}{ref xs[1..3]}"
def r03 :: i64 -> string = \n ->
    let mut m = n
    $"{ref mut m}"

// temporaries that were accepted at HEAD
def t01 :: i64 -> [i64] -> string -> string = \n xs s -> $"{id n}{Array.length xs}{String.length s}{(\(x: string) -> x.length) s}"
def t02 :: i64 -> bool -> string -> string = \n b s -> $"{n + 1}{-n}{n == 1}{!b}{n as i32}{s.length + 1}{(id n) + 1}"
def t03 :: i64 -> string = \n -> $"{1}{"abc"}{'c'}{true}{1.5}{()}{[1, 2, 3]}{make3 n}"
def t04 :: i64 -> bool -> string = \n b -> $"{if b then 1 else 2}{match n with | 1 -> "one" | _ -> "other"}{$"{n}"}"
def t05 :: unit -> string = \() -> $"{Title}{Words}"

// E1013 at HEAD, accepted after the change
def n01a :: string -> string = \s -> $"{s.length}"
def n01b :: utf8string -> string = \u -> $"{u.length}"
def n02a :: [i64] -> string = \xs -> $"{xs.length}"
def n02b :: [| i64 |] -> string = \ls -> $"{ls.length}"
def n02c :: [i64; 3] -> string = \fx -> $"{fx.length}"
def n03a :: &string -> string = \rs -> $"{rs.length}"
def n03b :: &[i64] -> string = \rx -> $"{rx.length}"
def n03c :: [i64] -> string = \xs ->
    let part = ref xs[1..3]
    $"{part.length}"
def n04a :: Pt -> string = \p -> $"{p.name.length}"
def n04b :: Line -> string = \ln -> $"{ln.a.name.length}"
def n04c :: [string] -> string = \ws -> $"{ws[1].length}"
def n05a :: string -> string = \s -> $"{s[0]}"
def n05b :: &string -> string = \rs -> $"{rs[1]}"
def n05c :: utf8string -> string = \u -> $"{u[0]}"
def n06 :: i64 -> string = \n -> $"{(mk n).x}"
def n07a :: i64 -> string = \n -> $"{(mk n).name}"
def n07b :: i64 -> string = \n -> $"{(mk n).name.length}"
def n08 :: i64 -> string = \n -> $"{(make3 n)[1]}"
def n09a :: i64 -> string = \n -> $"{(make3 n).length}"
def n09b :: unit -> string = \() -> $"{(names ()).length}"
def n09c :: unit -> string = \() -> $"{[1, 2, 3].length}"
def n10 :: i64 -> string = \n -> $"{(mkres n).id}"
def n11a :: unit -> string = \() -> $"{Nums.length}"
def n11b :: unit -> string = \() -> $"{Words.length}"
def n11c :: unit -> string = \() -> $"{Title.length}"
def n12a :: unit -> string = \() -> $"{Origin.x}"
def n12b :: unit -> string = \() -> $"{Origin.name}"
def n13a :: string -> string = \s -> $"{s.length:>5}"
def n13b :: [i64] -> string = \xs -> $"{xs.length:x}"
def n13c :: string -> string = \s -> $"{s.length:+}"
def n13d :: [i64] -> string = \xs -> $"{xs.length:*^7}"
def n14 :: [i64] -> string = \xs ->
    let show = \() -> $"{xs.length}"
    show ()

// E1013 at HEAD, E1012 after the change
def e01 :: unit -> string = \() -> $"{(names ())[0]}"
def e02 :: unit -> string = \() -> $"{Words[1]}"
def e03 :: i64 -> string = \n -> $"{(mkres n).name}"

// E1013 before and after
def x01a :: i64 -> string = \n -> $"{ref (id n)}"
def x01b :: string -> string = \s -> $"{ref s.length}"

// borrows that last for the whole interpolation
def k01 :: [i64] -> string = \a ->
    let mut b = a
    $"{b[0]}{poke (&mut b)}"
def k02a :: [i64] -> string = \a ->
    let mut b = a
    $"{b.length}{poke (&mut b)}{b[0]}"
def k02b :: i64 -> string = \n ->
    let mut m = n
    $"{m + 0}{bump (&mut m)}{m}"

"ok"
```

| 時点 | `check` の出力 |
| --- | --- |
| HEAD（実測） | 39 件。`E1013` が 38 件（`n01a`〜`n14` の 32 関数、`e01`〜`e03`、`x01a`・`x01b`、`k02a`）、`E1014` が 1 件（`k01`）。`p*`・`r*`・`t*`・`k02b` は出ない |
| 修正後（設計のコードを当てた複製のコンパイラの実測。付録 B） | 6 件だけ。`e01`・`e02` は `E1012`（`cannot move a non-Copy array or list element; borrow it instead`）、`e03` は `E1012`（`cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead`）、`x01a`・`x01b` は `E1013`、`k01` は `E1014` |

手順 1 で 39 件、手順 3 で 6 件になることを確かめる。

## 付録 B: 設計のコードを当てた複製での実測

設計のコード（`hole_operand` の 1 か所の変更と `hole_place`・`constant_part`）、付録 C の fixture、付録 D の suite、付録 E のテストを、ワーキングツリーを写した作業用の複製（`target/` の下）に当て、`cargo build --release --locked --test string_interpolation` を 1 回だけ実行した（3 分 17 秒、成功）。リポジトリ本体は変えておらず、ビルドもしていない。`cargo test` は使わず、できた `string_interpolation` のテストのバイナリを直接実行した。

| 確認 | 結果 |
| --- | --- |
| `string_interpolation` のテストのバイナリ（`RUST_MIN_STACK=4194304`） | 18 件がすべて通る（既存 11 + 付録 E の 7）。HEAD で通る 2 つ（`holes_that_are_places_stay_borrows`・`explicit_borrows_of_temporaries_stay_rejected`）と落ちる 5 つは、HEAD のコンパイラの `check` の結果から導いた見込みで、HEAD のテストのバイナリは作っていない |
| 付録 A のプログラムを `check` | HEAD の 39 件が 6 件だけになる（`e01`・`e02`・`e03` が `E1012`、`x01a`・`x01b` が `E1013`、`k01` が `E1014`） |
| 付録 C の fixture を `check` | HEAD の 15 件の `E1013` が 0 件になる |
| `node tests/features.mjs <複製のコンパイラ> interpolation_temporaries` | native/WASM × `-O0`/`-O3` で 39 ケースが通る（`live == 0`、import なし） |
| 同じ手順で `string_interpolation`・`constants`・`const_statics`・`borrowed_records` | 82・9・52・57 ケースが通る |
| コーパス 102 件（fixtures 93 + examples 9）の `build --emit llvm -O0` を、HEAD のコンパイラと複製のコンパイラで `cmp` | 102 件とも 1 byte も違わない |
| `Main.tz` のないライブラリの fixture 7 件と `tests/fixtures/Semantics.tz` の `check`（標準出力・標準エラー・終了コード） | HEAD と複製で同一 |
| `\r -> $"{deref r}"`・`\r -> $"{deref (mkref r)}"`・`\r -> $"{id2 (deref r)}"` の IR（`--emit llvm -O0`） | 前の 2 つは同一の IR（参照をそのまま `Display.display` へ渡す＝借用）。3 つ目だけが値を `alloca` へ退避する（一時値）。`{deref (mkref r)}` は `Dereference(Call)` で場所（`P05`） |
| 付録 C の `length_holes`（穴は 6 つで 5 つが一時値）の IR（`--emit llvm -O0`） | `@tz.string.allocate` は 1 回、`Display.display` の呼び出しは穴ごとに 1 回（6 回）、`@tz.alloc` は 0 回。`--emit llvm -O3` の出力は `-O0` と同一 |
| 付録 F の F1・F2 を複製の 2 ページに当てて `node scripts/check-docs.mjs` | 2 ページ・22 リンク・10 例・20 回の native 実行（`-O0`/`-O3`）が通る |
| 複製で `cargo fmt --all -- --check` と `cargo clippy --locked --lib --test string_interpolation -- -D warnings` | どちらも差分・警告なし（設計のコードと付録 E は整形済みの形で書いてある） |

実機で試していないもの: HEAD のテストのバイナリ、`cargo test` の全体、GUIDE §3.1 の 4 つの回帰テスト、`--all-targets` の clippy、`docs/language.md`・`docs/architecture.md` の置き換え。実装者が手順 2〜7 で確かめる。

## 付録 C: fixture `tests/fixtures/interpolation_temporaries/Main.tz`

付録 E の補助とは別に、`tests/fixtures/interpolation_temporaries/Main.tz` として次を新規に置く（`tests/fixtures/string_interpolation/Main.tz` と同じ書き方）。HEAD の `check` は、15 個の `export def` にちょうど 1 件ずつ、15 件の `E1013` を出す。`export def` の結果はすべて `i64`。

```text
record Pt { x: i64, name: string }

record Pair { a: i64, b: i64 }

record Res { id: i64, name: string }

instance Drop<Res> {
    fn drop _value = ()
}

const Nums: [i64] = [10, 20, 30]

const Corner: Pair = Pair { a: 3, b: 4 }

const Origin: Pt = Pt { x: 5, name: "origin" }

def digest :: &string -> i64
fn digest text =
    let mut hash = 0
    let mut index = 0
    while index < text.length do
        hash = (hash * 131 + (text[index] as i64)) % 1000000007
        index = index + 1
    hash

def mk :: i64 -> Pt
fn mk n = Pt { x: n, name: "seven" }

def make3 :: i64 -> [i64]
fn make3 n = [n, n + 1, n + 2]

def names :: unit -> [string]
fn names _unit = ["a", "bb"]

def mkres :: i64 -> Res
fn mkres n = Res { id: n, name: "res" }

def stamp :: &mut i64 -> i64 -> i64
fn stamp log digit =
    deref log = deref log * 10 + digit
    digit

def bump :: &mut i64 -> i64
fn bump r =
    deref r = deref r + 1
    deref r

def poke :: &mut [i64] -> i64
fn poke a =
    Array.write 0 9 a
    7

export def length_holes :: i64 -> i64
fn length_holes n =
    let s = "abc"
    let xs = [1, 2, 3]
    let rs = &s
    let rx = &xs
    let part = &xs[1..3]
    let text = $"{s.length}|{xs.length}|{rs.length}|{rx.length}|{part.length}|{n}"
    digest (&text)

export def other_lengths :: i64 -> i64
fn other_lengths n =
    let l = [| n, n, n, n, n |]
    let u = u8"héllo"
    let text = $"{l.length}|{u.length}"
    digest (&text)

export def string_index :: i64 -> i64
fn string_index n =
    let s = "abc"
    let text = $"{s[0]}|{s[2]}|{n}"
    digest (&text)

export def field_of_call :: i64 -> i64
fn field_of_call n =
    let text = $"{(mk n).x}|{(mk n).name}|{(mk n).name.length}"
    digest (&text)

export def index_of_call :: i64 -> i64
fn index_of_call n =
    let text = $"{(make3 n)[1]}|{(make3 n).length}|{(names ()).length}"
    digest (&text)

export def property_of_place :: i64 -> i64
fn property_of_place n =
    let p = mk n
    let text = $"{p.name.length}|{p.x}"
    digest (&text)

export def property_specs :: i64 -> i64
fn property_specs n =
    let xs = [n, n, n, n]
    let s = "abcdefghijklmnopq"
    let text = $"{xs.length:>5}|{s.length:x}|{s.length:+}|{s.length:b}|{xs.length:*^7}"
    digest (&text)

export def constant_parts :: i64 -> i64
fn constant_parts i =
    let text = $"{Nums[1]}|{Nums.length}|{Corner.a}|{Origin.x}|{Origin.name}|{Nums[i]}"
    digest (&text)

export def snapshot_holes :: i64 -> i64
fn snapshot_holes n =
    let mut m = n
    let mut a = [1, 2, 3]
    let text = $"{m + 0}{bump (&mut m)}{m}|{a.length}{poke (&mut a)}{a[0]}"
    digest (&text)

export def drop_temporary :: i64 -> i64
fn drop_temporary n =
    let text = $"{(mkres n).id}"
    digest (&text)

export def evaluation_order :: i64
fn evaluation_order =
    let mut log = 0
    let text = $"{stamp (&mut log) 1}{(mk (stamp (&mut log) 2)).x}{stamp (&mut log) 3}"
    log * 1000 + text.length

export def utf8_holes :: i64
fn utf8_holes =
    let s = "abc"
    let text = u8$"{s.length}|{s[1]}"
    text.length

export def nested_holes :: i64
fn nested_holes =
    let s = "abc"
    let text = $"<{$"[{s.length}]"}>"
    digest (&text)

export def captured_holes :: i64 -> i64
fn captured_holes n =
    let xs = [n, n, n]
    let show = \() -> $"{xs.length}:{xs[0]}"
    let text = show ()
    digest (&text)

export def owned_loop :: i64 -> i64
fn owned_loop count =
    let mut total = 0
    let mut index = 0
    while index < count do
        let text = $"{index}:{(names ()).length}:{(mk index).name}"
        total = total + text.length
        index = index + 1
    total
```

## 付録 D: `tests/features.mjs` の suite

`suites` の中、`string_interpolation` の次（`hierarchical` の前）に足す。期待値は suite の中の JavaScript で作る（`interpolationDigest`・`interpolationPad` は `features.mjs` の上のほうにある既存の補助）。

```js
  interpolation_temporaries: {
    cases: (() => {
      const ns = [-5n, 0n, 41n], table = [10n, 20n, 30n];
      const digests = (name, text) => ns.map((n) => [name, [n], interpolationDigest(text(n))]);
      return [
        ...digests("length_holes", (n) => `3|3|3|3|2|${n}`),
        ...digests("other_lengths", () => "5|6"),
        ...digests("string_index", (n) => `97|99|${n}`),
        ...digests("field_of_call", (n) => `${n}|seven|5`),
        ...digests("index_of_call", (n) => `${n + 1n}|3|2`),
        ...digests("property_of_place", (n) => `5|${n}`),
        ...digests("property_specs", () => `${interpolationPad("4", 5, "right")}|11|+17|10001|${interpolationPad("4", 7, "center", "*")}`),
        ...[0n, 1n, 2n].map((i) => ["constant_parts", [i], interpolationDigest(`20|3|3|5|origin|${table[Number(i)]}`)]),
        ...digests("snapshot_holes", (n) => `${n}${n + 1n}${n + 1n}|379`),
        ...digests("drop_temporary", (n) => `${n}`),
        ["evaluation_order", [], 123003n], ["utf8_holes", [], 4n],
        ["nested_holes", [], interpolationDigest("<[3]>")],
        ...digests("captured_holes", (n) => `3:${n}`),
        ...[0n, 1n, 1000n].map((count) => ["owned_loop", [count], (() => {
          let total = 0n;
          for (let index = 0n; index < count; index++) total += BigInt(`${index}:2:seven`.length);
          return total;
        })()]),
      ];
    })(),
    inspect(ir) {
      assert.doesNotMatch(ir, /@printf|@snprintf|@strtod|@strtof/);
    },
  },
```

各ケースの意味（`n` は引数）:

| export | 文字列 | 確かめること |
| --- | --- | --- |
| `length_holes` | `3\|3\|3\|3\|2\|n` | 文字列・配列・参照・スライスの借用の `.length` |
| `other_lengths` | `5\|6` | リストと `utf8string` の `.length`（`héllo` は 6 byte） |
| `string_index` | `97\|99\|n` | 文字列の添字（`i16u` の値） |
| `field_of_call` | `n\|seven\|5` | 呼び出し結果のフィールドと、その `.length` |
| `index_of_call` | `n+1\|3\|2` | 呼び出し結果の添字と `.length` |
| `property_of_place` | `5\|n` | 場所の経路の先の `.length`（`p.name.length`）と場所の `p.x` |
| `property_specs` | `    4\|11\|+17\|10001\|***4***` | 一時値の穴の書式指定（幅・16 進・符号・2 進・fill と中央揃え） |
| `constant_parts` | `20\|3\|3\|5\|origin\|`＋`Nums[i]` | 平たい定数の一部（場所）と、定数の `.length`・平たくない定数のフィールド（一時値）の混在 |
| `snapshot_holes` | `n`・`n+1`・`n+1`、`\|379` | 一時値の穴は元の場所を借りたままにしない（`bump`・`poke` が通る）。場所の `{m}` は更新後の値 |
| `drop_temporary` | `n` | `Drop` を持つ一時値の Copy の部分。drop は 1 回（`live == 0`） |
| `evaluation_order` | `123`（結果は `123003`） | 左から右へ 1 回ずつ。副作用の順 |
| `utf8_holes` | `3\|98`（長さ 4） | `u8$"..."` の一時値の穴 |
| `nested_holes` | `<[3]>` | 入れ子の補間の中の一時値の穴 |
| `captured_holes` | `3:n` | クロージャの中の穴 |
| `owned_loop` | 合計 | 1000 回の繰り返しで、所有する一時値が毎回解放される（`live == 0`） |

## 付録 E: `tests/string_interpolation.rs` に足すもの

ファイルの先頭の `use` に `use tsuzuri::check::{TypedExpr, TypedExprKind};` を足し、末尾に次を足す（既存の `accepts`・`rejects` を使う）。`cargo fmt --all` で整形する。

```rust
/// The definitions and bindings that the hole tests share; the hole is the last line.
const HOLE_PRELUDE: &str = r#"record Pt { x: i64, name: string }
record Line { a: Pt, b: Pt }
record Pair { a: i64, b: i64 }
record Res { id: i64, name: string }
instance Drop<Res> {
    fn drop _value = ()
}
const Nums: [i64] = [10, 20, 30]
const Corner: Pair = Pair { a: 3, b: 4 }
const Origin: Pt = Pt { x: 5, name: "origin" }
const Words: [string] = ["a", "bb", "ccc"]
const Title: string = "title"
def id :: i64 -> i64
fn id n = n
def mk :: i64 -> Pt
fn mk n = Pt { x: n, name: "seven" }
def mkres :: i64 -> Res
fn mkres n = Res { id: n, name: "res" }
def make3 :: i64 -> [i64]
fn make3 n = [n, n + 1, n + 2]
def names :: unit -> [string]
fn names _unit = ["a", "bb"]
let n: i64 = 5
let i: i64 = 1
let b = true
let s = "abc"
let u = u8"abc"
let xs: [i64] = [1, 2, 3]
let ws = ["a", "bb"]
let ls: [| i64 |] = [| 1, 2 |]
let p = mk 7
let ln = Line { a: mk 1, b: mk 2 }
let r = ref n
let rp = ref p
let rx = ref xs
let rs = ref s
let part = ref xs[1..3]
let mut m: i64 = 5
"#;

fn hole_source(holes: &str) -> String {
    format!("{HOLE_PRELUDE}$\"{holes}\"\n")
}

/// How each hole of the interpolation at the end of `source` takes its operand: `Borrow` (a borrow
/// of storage that is named where it is) or `BorrowOperand` (a hidden slot that is dropped after the join).
fn hole_kinds(source: &str) -> Vec<&'static str> {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
    });
    let mut pending: Vec<&TypedExpr> = vec![&module.functions[module.entry.unwrap()].body];
    while let Some(expression) = pending.pop() {
        if let TypedExprKind::Interpolated(interpolation) = &expression.kind {
            return interpolation
                .holes
                .iter()
                .map(|hole| match &hole.operand.kind {
                    TypedExprKind::Borrow(..) => "Borrow",
                    TypedExprKind::BorrowOperand(_) => "BorrowOperand",
                    _ => "other",
                })
                .collect();
        }
        pending.extend(expression.children());
    }
    panic!("{source}\nno interpolation in the entry");
}

fn all_holes_are(holes: &str, kind: &str) {
    let kinds = hole_kinds(&hole_source(holes));
    assert!(
        !kinds.is_empty() && kinds.iter().all(|found| *found == kind),
        "{holes}: {kinds:?}"
    );
}

#[test]
fn holes_that_are_places_stay_borrows() {
    for holes in [
        "{s}{n}",
        "{p.name}{ln.a.x}",
        "{xs[0]}{xs[i]}{ws[1]}",
        "{rx[0]}",
        "{deref r}{*r}{rp.x}{(deref rp).x}",
        "{Nums[1]}{Nums[i]}{Corner.a}",
        "{xs[xs.length - 1]}",
        "{r}{rs}",
        "{ref n}{&n}{ref mut m}",
    ] {
        all_holes_are(holes, "Borrow");
    }
}

#[test]
fn holes_that_are_not_places_are_temporaries() {
    for holes in [
        "{id n}{Array.length xs}{String.length s}",
        "{n + 1}{-n}{n == 1}{!b}{n as i32}",
        "{1}{\"abc\"}{'c'}{true}{1.5}{()}{[1, 2, 3]}",
        "{if b then 1 else 2}",
        "{s.length}{u.length}",
        "{xs.length}{ls.length}",
        "{rs.length}{rx.length}{part.length}",
        "{p.name.length}{ln.a.name.length}{ws[1].length}",
        "{s[0]}{rs[1]}{u[0]}",
        "{(mk n).x}",
        "{(mk n).name}{(mk n).name.length}",
        "{(make3 n)[1]}",
        "{(make3 n).length}{(names ()).length}{[1, 2, 3].length}",
        "{(mkres n).id}",
        "{Nums.length}{Words.length}{Title.length}",
        "{Origin.x}{Origin.name}",
        "{s.length:>5}{xs.length:x}{s.length:+}",
    ] {
        all_holes_are(holes, "BorrowOperand");
    }
}

#[test]
fn property_and_projection_holes_are_accepted() {
    for holes in [
        "{s.length}{u.length}{xs.length}{ls.length}{rs.length}{rx.length}{part.length}",
        "{p.name.length}{ln.a.name.length}{ws[1].length}{s[0]}{rs[1]}{u[0]}",
        "{(mk n).x}{(mk n).name}{(mk n).name.length}{(make3 n)[1]}",
        "{(make3 n).length}{(names ()).length}{[1, 2, 3].length}{(mkres n).id}",
        "{Nums.length}{Words.length}{Title.length}{Origin.x}{Origin.name}",
        "{s.length:>5}|{xs.length:x}|{s.length:+}|{xs.length:*^7}",
    ] {
        accepts(&hole_source(holes));
    }
    accepts("let xs = [1, 2, 3]\nlet show = \\() -> $\"{xs.length}\"\nshow ()\n");
    accepts("let s = \"abc\"\nu8$\"{s.length}|{s[1]}\"\n");
    accepts("let s = \"abc\"\n$\"<{$\"[{s.length}]\"}>\"\n");
}

#[test]
fn holes_that_move_out_of_a_temporary_are_rejected() {
    rejects(&hole_source("{(names ())[0]}"), "E1012");
    rejects(&hole_source("{Words[1]}"), "E1012");
    rejects(&hole_source("{(mkres n).name}"), "E1012");
}

#[test]
fn explicit_borrows_of_temporaries_stay_rejected() {
    rejects(&hole_source("{ref (id n)}"), "E1013");
    rejects(&hole_source("{ref s.length}"), "E1013");
}

#[test]
fn a_temporary_hole_leaves_its_source_free() {
    let poke = "def poke :: &mut [i64] -> i64\nfn poke a =\n    Array.write 0 9 a\n    7\n";
    accepts(&format!(
        "{poke}let mut a = [1, 2, 3]\n$\"{{a.length}}{{poke (&mut a)}}{{a[0]}}\"\n"
    ));
    rejects(
        &format!("{poke}let mut a = [1, 2, 3]\n$\"{{a[0]}}{{poke (&mut a)}}\"\n"),
        "E1014",
    );
}

#[test]
fn format_specs_apply_to_property_holes() {
    accepts("let s = \"abc\"\nlet xs = [1, 2, 3]\n$\"{s.length:x}|{xs.length:>5}|{s.length:+}\"\n");
    rejects("let s = \"abc\"\n$\"{s.length:.2}\"\n", "E1003");
}
```

- `hole_kinds` は、型付き木の形に依存する。`holes_that_are_places_stay_borrows` が HEAD で通らなければ（停止条件 1）、補助の前提（`Borrow` かどうか）が違うので、補助か期待を直す。コンパイラは触らない。
- `HOLE_PRELUDE` の束縛は、決定表の「束縛の型」と同じ（`ln` は `Line { a: mk 1, b: mk 2 }` で、`p` を二重に移さない）。整数の束縛は `i64` と書く（注釈のない `let n = 5` を `ref` で借りた `let r = ref n` の `{r}` は、HEAD で `error[E1005]: no instance for Display<ref i32>` になる。本チケットの範囲外の既存の挙動で、補助の束縛を注釈つきにして避ける。「対象外」）。未使用の束縛の警告は誤りではない。

## 付録 F: 文書の置き換え文

### F1: `_tsuzuri/language-reference/literals-and-strings/interpolated-strings.md`

①「穴は 1 行に収めます」の段落から、次の 1 文を削除する。

```text
添字や `.length` も、穴に直接書くと一時値の借用で `E1013` になることがあります。
```

②「この記事のポイント」の箇条書きの末尾に 1 行足す。

```text
- `.length`・文字列の添字・呼び出し結果のフィールドや添字も、穴にそのまま書けます（一時値）。
```

③「穴の評価と所有権」の「各穴の値は消費しません。ローカル変数、フィールド、添字、参照外しは共有借用です。…補間のあとで使えます。」の段落を、次に置き換える。

````text
各穴の値は消費しません。穴の式は、書き方ではなく型が決める次の 3 つのどれかになります。

| 穴の式 | 扱い | 例 |
| --- | --- | --- |
| 場所（ローカル変数、場所のフィールド・添字、参照外し、平たい定数の `C[i]`・`C.field`） | その場で共有借用します。借用は補間の全体のあいだ続きます | `{s}`、`{p.name}`、`{xs[0]}`、`{deref r}` |
| 参照（`ref T`、`ref mut T`） | `ref T` はそのまま、`ref mut T` は共有の再借用です | `{ref n}`、`{ref mut m}` |
| それ以外の値（一時値） | 補間の中の位置で 1 回評価して隠れた領域に置き、表示し、全体を結合したあとで解放します。元の場所は借りたままになりません | `{s.length}`、`{xs.length}`、`{s[0]}`、`{(f x).name}`、`{n + 1}`、`{if b then 1 else 2}` |

`.length`、文字列の添字、呼び出し結果のフィールドや添字は、場所に似た書き方でも値なので一時値です。`let` で束縛しなくても書けます。`to_string` と違い、Copy でないローカルも補間のあとで使えます。

```tsuzuri run=5%203%20104
let s = "hello"
let xs = [1, 2, 3]
$"{s.length} {xs.length} {s[0]}"
```

実行結果:

```text
5 3 104
```

次の 3 つは、一時値の穴に書けません。

- `ref` を付けた一時値（`{ref (f x)}`、`{ref s.length}`）は、ほかの場所と同じく `E1013` です。`ref` を外します。
- 一時値から Copy でない要素やフィールドを取り出す穴（`{(names ())[0]}`、`Drop` を持つ値の `{(mk n).name}`）は `E1012` です。`let` で束縛してから、場所として書きます。
- `{if b then s else t}` のように分岐が Copy でないローカルを選ぶと、選んだローカルは一時値へ移ります。補間のあとで使うと `E1012` です。
````

④「型に `Display` が無いと `E1005` です。…」の段落の「借用は補間式の全体のあいだ続くので、`$"{x}{bump (ref mut x)}"` のように同じ場所を排他借用すると `E1014` です。」を、次に置き換える。

```text
場所の穴の借用は補間式の全体のあいだ続くので、`$"{x}{bump (ref mut x)}"` のように同じ場所を排他借用すると `E1014` です。一時値の穴は評価した時点の値だけを持つので、`$"{a.length}{poke (ref mut a)}"` は通ります。
```

### F2: `_tsuzuri/language-reference/literals-and-strings/strings.md`

添字の段落の次の 2 文を、

```text
添字や `.length` を補間の穴へ直接書くと、一時値を借用できず `E1013` になることがあります。先に `let` で束縛してください。
```

次の 1 文に置き換える。

```text
添字や `.length` は、補間の穴へそのまま書けます（一時値として評価されます。[補間文字列](interpolated-strings.md#穴の評価と所有権)）。
```

### F3: `docs/language.md`（「文字列補間」の箇条書き）

```text
- 各穴の値は消費せず借用して表示します。穴の式は、書き方ではなく型付けした式で分類します。場所（ローカル変数・場所のフィールド・索引・参照外し・平たい定数の `C[i]`・`C.field`）は共有借用、`ref T` の値はそのまま、`ref mut T` の値は共有の再借用、それ以外（`.length`、文字列の索引、呼び出し結果のフィールドと索引、算術、`if`、`match`、呼び出し）は一時値として評価して隠れた領域へ置き、表示後に drop します。`ref` を付けた一時値は `E1013`、一時値から取り出す非 Copy の要素・フィールドは `E1012` です。`to_string` と違い、非 Copy のローカルも補間の後で使えます。
```

### F4: `docs/architecture.md`（補間の段落）

「評価場所（変数名、フィールド、添字、参照外し）は `ref` 引数と同様に借用され、…（引数のない関数呼び出しや単独の union case のように値を生成する式も一時値として扱われます）。」を、次に置き換える。

```text
穴が場所かどうかは書き方ではなく型付けした式で決まります（`hole_operand`・`hole_place`。`TypedExpr::is_place` と、平たい定数の一部 `C[i]`・`C.field`）。場所は `ref` 引数と同様に借用され、既存の参照はそのまま再利用され、それ以外の評価結果は `BorrowOperand` による一時変数として確保された後に文字列結合完了時点で安全に drop されます（`.length`・文字列の添字・呼び出し結果のフィールドや添字、引数のない関数呼び出しや単独の union case のように値を生成する式も一時値として扱われます）。
```
