# E07: デバッグ出力（Debug.print／trace）
| 項目 | 内容 |
|---|---|
| ID | E07 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | D01, E02 |
| 後続 | なし |
| 状態 | todo |
| 主な影響ファイル | `std/Debug.tz`, `src/check.rs`, `src/llvm.rs`, `src/driver.rs`, `src/main.rs`, `src/runtime/debug.ll`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

開発中の値確認のため、`Debug.print` と `Debug.trace` を標準ライブラリとして提供する。

Tsuzuri は通常の I/O を host 境界に置くが、デバッグ中に値を表示する最小の副作用 API は実用上必要。

D01 の `Display` と `to_string` を使い、任意の表示可能な値を文字列化して出力する。

native は stderr に出す。

WASM は D-18 に従い、既定では import を増やさず no-op にする。

WASM で出力したい場合は明示 `--debug-output` で host import を追加する。

引数評価は no-op 時も必ず行い、評価順序・trap・move/borrow を変えない。

## 現状

`src/llvm.rs` の `console_main` は application entry の結果を stdout に出すだけ。

`src/runtime/console.ll` は `putchar` による `@tz.console.write(ptr, i64)` を持ち、stdout 相当へ改行付きで書く。

言語内の通常関数から出力する API はない。

`docs/language.md` は OS/GUI/DOM/network/file I/O は host 側に置くと説明している。

E02 後、std module `Debug` は予約される。

D01 後、`Display<'a>` と `to_string` または equivalent display runtime が存在する。

`src/llvm.rs` の `emit_builtin` は builtin ごとに internal function を生成できる。

`src/driver.rs` の `BuildOptions` は target/emit/optimization/cpu だけを持つ。

`src/main.rs` の CLI parser は `--json`, `--target`, `--emit`, `-O`, `--cpu` を扱う。

WASM tests は import が空であることを既定として検査する。

## 仕様

### API

`std/Debug.tz` に次を提供する。

```text
def print :: Display<'a> => &'a -> unit
fn print value = Debug.__print_string (display value)

def trace :: Display<'a> => 'a -> 'a
fn trace value = {
    Debug.print (&value);
    value
}
```

`Debug.__print_string` は private builtin namespace function。

実際の D01 API 名が `Display.display` / `to_string` のどちらになっても、D01 の確定名に合わせる。

既定案は D-11 に従い `Display<'a> { def display :: &'a -> string }` とし、`display value` または `Display.display value` を使う。

`Debug.print` は表示文字列の後に改行を出す。

`Debug.trace` は値を一度受け取り、表示してから同じ値を返す。

`Debug.trace` は Copy/非 Copy を問わず動く。

非 Copy 値は `trace` に move され、同じ値が戻る。

`Debug.trace` は値を clone しない。

`Debug.trace` は引数を一回だけ評価する。

`Debug.print` は引数を共有借用で受けるため、表示後も所有者を使える。

### Debug.__print_string builtin

E02 の qualified builtin mechanism を使う。

private std function:

```text
private def __print_string :: string -> unit
```

または:

```text
private def __print_string :: &string -> unit
```

既定案は `string -> unit`。

理由: `display` は owned string を返すため、その所有権を builtin が消費し、出力後に解放できる。

もし D01 の `display` が borrowed buffer を返す設計になった場合は `&string -> unit` に調整する。

`__print_string` は public API ではない。

ユーザーが `Debug.__print_string` を直接呼ぶと E01 の `E1022`。

LLVM builtin name は `@tz.builtin.Debug.__print_string`。

### Native behavior

native target では stderr に UTF-8 bytes を書く。

stdout の console entry とは分ける。

`console_main` の stdout 出力と `Debug.print` の stderr 出力は別 stream。

実装は POSIX `write(2, ptr, len)` を使う。

runtime IR:

```llvm
declare i64 @write(i32, ptr, i64)

define internal i8 @tz.debug.write(%tz.string %text) nounwind {
entry:
  %data = extractvalue %tz.string %text, 0
  %len = extractvalue %tz.string %text, 1
  ; write fd 2 bytes, then newline
  ; result <= 0 is failure unless errno == EINTR is explicitly checked and retried
  ; free text data
  ret i8 0
}
```

`write` の result が `<= 0` なら trap する。

EINTR retry を実装する場合だけ errno を確認して retry してよい。

short write は rare だが、正確さのため loop して remaining bytes を書く。

改行も write する。

zero progress で spin してはいけない。

`unit` の LLVM 表現は `i8 0`。

Windows native は G10 まで未検証。`write` が使えない target で debug output を含む native link が失敗した場合は `E2002` とする。

### WASM behavior

既定の WASM build では `Debug.__print_string` は no-op builtin に lower する。

no-op でも argument evaluation と display string allocation は行う。

no-op builtin は受け取った owned string を drop/free する。

つまり `Debug.print (&expensive())` の `expensive()` と `display` は実行される。

`--debug-output` が指定された WASM build では import を追加する。

Import:

```text
module: "tsuzuri_debug"
name: "write"
signature: (ptr, i64) -> unit
```

WASM wrapper calls import with pointer/length for UTF-8 bytes, then frees string.

Host import must not retain pointer after returning unless it copies bytes.

`driver::build` は通常 WASM memory を export しないため、debug import が reachable かつ `--debug-output` が指定された場合だけ `wasm-ld` に `--export-memory` を渡す。

host import は `instance.exports.memory` から `(ptr, len)` の bytes を同期的に copy する。

WASM import is added only when `Debug.__print_string` is reachable.

`--debug-output` without reachable debug call is allowed and does not add import。

`--debug-output` on native is allowed but no-op option, or rejected? 既定案: allowed and ignored for native to simplify cross-target scripts。

Docs must state it only changes WASM。

### CLI

Add `debug_output: bool` to `driver::BuildOptions`.

Add `--debug-output` CLI option.

Allowed for `build` and `run`。

`check --debug-output` is invalid `E2000` because check does not emit code。

`--emit header --debug-output` is invalid `E2000` because header has no runtime behavior。

`--emit llvm --debug-output --target wasm32` should include import declaration in IR because it affects codegen。

`run --debug-output` native behaves same as `run`。

### Evaluation order and optimization

`Debug.print` is side-effecting for native and opt-in WASM.

Even in default WASM no-op mode, it must not be removed before evaluating its argument.

The `display` call must happen before builtin call.

LLVM call to native/opt-in debug write must not be marked `readnone` or `readonly`。

The no-op builtin may be a function that frees/drops its string argument and returns unit.

Do not replace `Debug.print expr` with `()` at typecheck level.

Do not make `Debug.trace value` an identity optimization before evaluating display.

### Tasks and interleaving

`Debug.print` inside `Task.parallel` may interleave between tasks.

Each individual call writes one UTF-8 string and newline.

Native implementation should try to issue each call as a contiguous write for bytes and newline, but no global ordering guarantee across threads。

WASM current task backend is sequential, so output order follows execution order。

No locking is required in Tsuzuri runtime for Phase 1.

### check/library builds

`tsuzuri check` typechecks `Debug.print` like any std function。

`--emit object` and `--emit llvm` include debug runtime only if reachable.

library output may contain calls to native stderr or WASM import if debug is reachable.

This is intentional: debug output is a side effect of the library function when host calls it.

### 前提とする他チケットのインターフェース

E02 provides std `Debug` module, qualified builtin `Debug.__print_string`, std pruning, and reserved module names.

D01 provides `Display<'a>` and display function returning `string`.

E01 provides private helper hiding for `Debug.__print_string`.

### 他チケットへの提供インターフェース

E06 host imports and E07 WASM debug import must share D-18 documentation language but do not share syntax.

G06 test runner may use `Debug.print` in examples but should not depend on output order in parallel tasks.

F06 WASM threads must revisit debug import thread-safety.

## 設計

### std/Debug.tz

Initial content:

```text
private def __print_string :: string -> unit

def print :: Display<'a> => &'a -> unit
fn print value =
    __print_string (Display.display value)

def trace :: Display<'a> => 'a -> 'a
fn trace value = {
    Debug.print (&value);
    value
}
```

If D01 chooses top-level `display` function instead of `Display.display`, adjust call site but keep API unchanged.

`__print_string` has no source body; it is a builtin declaration. E02 must support builtin namespace fallback for `Debug.__print_string`.

If checker requires every `def` to have `fn`, E07 should represent `__print_string` as builtin only and call `Debug.__print_string` from source without `def` in std. 既定案は E02 builtin namespace allows no source declaration.

### Builtin scheme

Add `Builtin::DebugPrintString`.

```rust
Builtin::name() -> "Debug.__print_string"
Builtin::scheme() -> BuiltinScheme {
    parameters: vec![BuiltinType::Concrete(Type::String)],
    result: BuiltinType::Concrete(Type::Unit),
    variables: vec![],
    constraints: vec![],
}
```

No type variables and no constraints.

### LLVM native

Add `src/runtime/debug.ll`.

It declares `@write` and defines internal helper `@tz.debug.write(%tz.string)`.

新しい runtime `.ll` は `.gitignore` の `*.ll` に隠れるため、`.gitignore` に `!src/runtime/debug.ll` を追加する。

実装 PR では `git ls-files src/runtime/debug.ll .gitignore` で `debug.ll` が追跡対象になっていることを確認する。

`emit_builtin(DebugPrintString)` emits:

```llvm
define internal i8 @tz.builtin.Debug.__print_string(%tz.string %text) nounwind {
entry:
  %r = call i8 @tz.debug.write(%tz.string %text)
  ret i8 %r
}
```

or directly emits the write loop.

Prefer runtime helper to keep builtin emission small.

`emit_target` appends `debug.ll` when output contains `@tz.debug.write`。

`debug.ll` uses `@tz.free` to free string data after writing。

Therefore `emit_target` must also append heap runtime through existing `@tz.free` detection。

### LLVM WASM default

Default no-op builtin:

```llvm
define internal i8 @tz.builtin.Debug.__print_string(%tz.string %text) nounwind {
entry:
  %data = extractvalue %tz.string %text, 0
  call void @tz.free(ptr %data)
  ret i8 0
}
```

It frees the owned display string.

It does not call imports.

It still causes heap runtime if string allocation happened.

### LLVM WASM opt-in

With `BuildOptions.debug_output && wasm`:

```llvm
declare void @tsuzuri_debug_write(ptr, i64) #dbg
attributes #dbg = { "wasm-import-module"="tsuzuri_debug" "wasm-import-name"="write" }

define internal i8 @tz.builtin.Debug.__print_string(%tz.string %text) nounwind {
entry:
  %data = extractvalue %tz.string %text, 0
  %len = extractvalue %tz.string %text, 1
  call void @tsuzuri_debug_write(ptr %data, i64 %len)
  call void @tz.free(ptr %data)
  ret i8 0
}
```

Import does not receive newline separately. The builtin appends newline before import or host decides? 既定案: Tsuzuri appends newline by calling import twice or allocating concat.

Avoid extra allocation: import contract receives bytes without newline; host glue prints line. However native API prints newline.

To keep cross-target behavior, WASM import contract receives two calls? That complicates host.

既定案: `tsuzuri_debug.write(ptr, len)` receives the display bytes only, and docs require host to append newline. But API says Debug.print prints newline.

Therefore better contract:

```text
tsuzuri_debug.write(ptr, len) writes one line and appends newline itself.
```

Host import is part of debug-output option and documented accordingly.

Native runtime appends newline internally.

`driver::build` は debug import が emitted/reachable の場合だけ `wasm-ld` に `--export-memory` を追加する。

`--debug-output` が指定されても debug call が pruning された場合は import も memory export も増やさない。

### Driver / options

`llvm::emit_target` signature currently `emit_target(module, entry, wasm)`.

Change to:

```rust
pub struct EmitOptions {
    pub wasm: bool,
    pub debug_output: bool,
}

pub fn emit_target(module: &CheckedModule, entry: Entry, options: EmitOptions) -> Result<String, Diagnostic>;
```

Keep `llvm::emit(module, entry)` as default native no debug output for tests.

`driver::build` passes `EmitOptions { wasm: options.target == Target::Wasm32, debug_output: options.debug_output }`.

`BuildOptions::validate` rejects `debug_output && emit == Emit::Header` and action check in main rejects for check.

### Source pruning

E02 pruning may remove unused `Debug.print` std function.

If a user function calls `Debug.print`, std `Debug.print`, `Debug.trace`, and builtin instance become reachable.

If no debug call is reachable, no debug runtime/import appears.

## 実装手順

1. Add `Debug.__print_string` builtin scheme in E02 mechanism.
   - 確認: `Debug.__print_string "x"` is resolvable inside std/custom test but private to user.

2. Add `std/Debug.tz` with `print` and `trace`.
   - 確認: `let value = 42; Debug.print (&value)` typechecks after D01.

3. Add `--debug-output` option and `BuildOptions.debug_output`.
   - 確認: `check --debug-output` and `--emit header --debug-output` reject with `E2000`.

4. Refactor `llvm::emit_target` to accept `EmitOptions`.
   - 確認: existing tests using `llvm::emit` remain unchanged.

5. Implement native debug runtime.
   - 確認: native `run` prints Debug output to stderr and main result to stdout.

6. Implement WASM default no-op builtin.
   - 確認: Debug call in WASM without `--debug-output` evaluates argument and returns normally; imports are empty.

7. Implement WASM opt-in import attributes.
   - 確認: imports contain `tsuzuri_debug.write` only when reachable and option set, and `WebAssembly.Module.exports(module)` includes `memory` only in that case.

8. Add task interleaving tests without ordering assertion.
   - 確認: no deadlock, each call returns.

9. Add `.gitignore` exception for `src/runtime/debug.ll`.
   - 確認: `git ls-files src/runtime/debug.ll .gitignore` shows the runtime file and `.gitignore`.

10. Update docs and examples.
   - 確認: README shows native stderr and WASM host import glue.

実装時の検証は GUIDE §3 に従い、Rust は `cargo test --locked --test <file>` で対象 test file を指定する。

Node E2E の直前には必ず `cargo build --release --locked` を実行し、`node tests/*.mjs target/release/tsuzuri` は古い binary で走らせない。

## テスト計画

### Rust accepted

`let value = 42; Debug.print (&value)` typechecks.

`let x = Debug.trace "hello"` returns `string`.

`Debug.trace` works with non-Copy<string> by moving in and returning owned string.

`Debug.print` can borrow a value and the owner can be used afterward.

`Debug.print` inside `task { ... }` typechecks if captured values are Send.

### Rust rejected

Calling private `Debug.__print_string` from user code is `E1022`.

`Debug.print 42` without borrow is `E1003` or normal type mismatch.

`Debug.trace (&value)` returns a reference and obeys lifetime rules; invalid escape remains `E1013`.

If D01 Display constraint missing, `Debug.print (&record_without_display)` is class constraint error `E1005`/`E1016` per D01.

### Native E2E

fixture:

```text
export def answer :: i64
fn answer = {
    let message = "hello";
    Debug.print (&message);
    Debug.trace 42
}
```

Build native exe and run.

stdout is `42\n` if entry/main prints answer, or C host result is 42 for library fixture.

stderr contains `hello\n` and `42\n` depending fixture.

Run `-O0` and `-O3`.

Verify heap tracking live == 0 when display allocates string.

### WASM default E2E

Build same fixture `--target wasm32` without `--debug-output`.

`WebAssembly.Module.imports(module)` is `[]`.

Calling export returns 42.

Argument evaluation test:

```text
def boom :: i64
fn boom = { assert false; 0 }
export def test :: i64
fn test = {
    let value = boom();
    Debug.print (&value);
    1
}
```

Calling `test` traps even though debug output is no-op.

### WASM opt-in E2E

Build with `--debug-output`.

`WebAssembly.Module.imports(module)` equals `[ { module: "tsuzuri_debug", name: "write", kind: "function" } ]` plus no others.

Instantiate with host import that copies bytes from memory and appends newline.

The host import reads bytes from `instance.exports.memory.buffer` during the call and copies them before returning.

Captured lines equal expected display strings.

`WebAssembly.Module.exports(module)` includes `memory` for this fixture.

Run `-O0` and `-O3`.

### IR tests

Native IR contains `@tz.debug.write` only when Debug reachable.

WASM default IR has no `wasm-import-module` attribute for debug.

WASM opt-in IR has deterministic import attribute.

No `readnone`/`readonly` on debug write call.

Native debug runtime checks `write` result `<= 0` as failure and cannot spin on zero progress.

## ドキュメント

`docs/language.md` に `Debug.print` / `Debug.trace` の API、newline、stderr、WASM no-op/default を追加する。

`docs/language.md` に effects are not typed and debug output is for development と書く。

`docs/architecture.md` の WASM invariant に `--debug-output` opt-in import を追加する。

`README.md` の CLI options に `--debug-output` を追加する。

`README.md` に native stderr と WASM host import example を追加する。

WASM host example must copy bytes from `instance.exports.memory`.

## 受け入れ条件

- [ ] `Debug.print : Display<'a> => &'a -> unit` が使える。
- [ ] `Debug.trace : Display<'a> => 'a -> 'a` が値を一度評価して返す。
- [ ] native は stderr に line output する。
- [ ] WASM default は imports 空で no-op だが argument evaluation/traps は維持する。
- [ ] WASM `--debug-output` は明示 import を追加する。
- [ ] WASM `--debug-output` は reachable debug import がある場合だけ memory を export する。
- [ ] debug call は LLVM で pure 扱いされない。
- [ ] task 内呼び出しが動き、順序保証を過剰にテストしない。
- [ ] native/WASM × `-O0`/`-O3` が通る。
- [ ] heap tracking live == 0。
- [ ] `src/runtime/debug.ll` is tracked despite `*.ll` ignore, verified with `git ls-files`.

## 落とし穴

WASM no-op を型検査前や最適化前に `()` へ置換すると引数評価が消える。

`Debug.trace` を identity として特殊化すると表示が消える。

display string を no-op builtin が free しないと leak する。

native stdout に書くと console entry の結果と混ざるため stderr を使う。

WASM import を既定で追加すると D-18 と既存 tests に反する。

host import が pointer を保持すると free 後に dangling になる。

`&42` や `&"hello"` のような一時値借用は現在 `E1013` なので、examples/tests では先に `let` で束縛する。

`write` loop が result 0 を retry し続けると無限 loop になる。

`--debug-output` で memory export を忘れると host import が `(ptr, len)` の bytes を読めない。

## 対象外

本番 logging framework。

format string API。

log levels。

source location。

structured trace events。

browser console glue generator。

file/network I/O。

## 未決事項

`Debug.trace` に label 付き overload を追加するかは未決。既定案は追加しない。

native Windows で `write(2,...)` をどう扱うかは未決。既定案は G10 まで未対応で link error を E2002 とする。

WASM import host が newline を付ける契約でよいかは未決。既定案は host import `write` が one line として newline を付ける。

台帳の見直し提案はない。D-18 の opt-in 方針に従う。
