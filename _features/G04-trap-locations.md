# G04: トラップ発生位置の報告
| 項目 | 内容 |
|---|---|
| ID | G04 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | なし |
| 後続 | E07, G08 |
| 状態 | todo |
| 主な影響ファイル | `.gitignore`, `src/diagnostic.rs`, `src/check.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/llvm_frame.rs`, `src/driver.rs`, `src/main.rs`, `src/runtime/trap-native.ll` または `src/runtime/*.ll`, `src/runtime/numeric.c`, `src/runtime/generate.py`, `tests/trap_locations.rs`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

実行時 trap が発生したとき、利用者が「どの部分関数が失敗したか」をソース位置付きで分かるようにする。

現在の native 実行は `llvm.trap` によりプロセスが終了し、`tsuzuri run` は
`integer division, indexing, assert, or allocation may have trapped` という一般的な `E2005` しか出せない。
WASM でも `WebAssembly.RuntimeError` だけで、Tsuzuri source span へ戻れない。

## 現状

`src/llvm.rs` は `emit_target` で常に `declare void @llvm.trap()` を出す。
trap emission は複数箇所に散っている。

| 場所 | 関数 / 箇所 | 現在の IR |
|---|---|---|
| `src/llvm.rs` | `FunctionEmitter::guard` | `call void @llvm.trap(); unreachable` |
| `src/llvm.rs` | `emit_builtin(Builtin::Assert)` | builtin wrapper 内で `llvm.trap` |
| `src/llvm.rs` | `FunctionEmitter::binary` | 整数 `/` `%` のゼロ除算 / `MIN / -1` を `guard` |
| `src/llvm.rs` | `allocation_size` | 負 length / byte size overflow を `guard` |
| `src/llvm.rs` | `checked_element_pointer` | array/list bounds を `guard` |
| `src/llvm.rs` | string index | string bounds を `guard` |
| `src/llvm.rs` | `place` の `ListTail` | list tail pattern の長さ不足を `guard` |
| `src/llvm_control.rs` | `range_loop` | arbitrary step の 0 を `guard` |
| `src/llvm_control.rs` | `match_expression` failure block | `llvm.trap` |
| `src/runtime/string.ll` | `@tz.string.concat` | length overflow で `llvm.trap` |
| `src/runtime/heap-native.ll` | `@tz.alloc` | malloc null で `llvm.trap` |
| `src/runtime/heap-wasm.ll` | `@tz.alloc` | memory.grow 失敗 / 16 MiB 超過で `llvm.trap` |
| `src/runtime/wasm.ll` | 128-bit helper | helper 内の failure で `llvm.trap` |
| `src/runtime/numeric.c` / `numeric.ll` | soft numeric helper | `__builtin_trap()` |

`driver::run` は `Command::status()` で実行し、失敗時に `E2005` を返す。
子プロセス stderr は現在、捕捉せず親 stderr に流れる。

## 仕様

### trap site

LLVM 生成時に trap site を採番し、site id と source span を side table に保存する。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrapKind {
    Assert,
    IntegerDivisionByZero,
    IntegerDivisionOverflow,
    BoundsCheck,
    AllocationSize,
    AllocationFailure,
    RangeStepZero,
    MatchFailure,
    PatternMismatch,
    StringConcatOverflow,
    NumericRuntime,
    WasmRuntime,
}

pub struct TrapSite {
    pub id: u32,
    pub kind: TrapKind,
    pub span: Span,
}
```

表示名:

| TrapKind | 表示 |
|---|---|
| `Assert` | `assertion failed` |
| `IntegerDivisionByZero` | `integer division by zero` |
| `IntegerDivisionOverflow` | `integer division overflow` |
| `BoundsCheck` | `index out of bounds` |
| `AllocationSize` | `allocation size overflow` |
| `AllocationFailure` | `allocation failed` |
| `RangeStepZero` | `range step is zero` |
| `MatchFailure` | `non-exhaustive match` |
| `PatternMismatch` | `pattern mismatch` |
| `StringConcatOverflow` | `string length overflow` |
| `NumericRuntime` | `numeric runtime trap` |
| `WasmRuntime` | `wasm runtime trap` |

`CheckedModule` と `Span` は path / source text を持たない。`Span.source` は source id だけなので、
trap-aware emission には driver の `Project` から作る immutable source map を明示的に渡す。

```rust
pub struct TrapSource<'a> {
    pub path: &'a str,
    pub text: &'a str,
}
```

`TrapRegistry::site` は `Span.source` で `TrapSource` を引き、`diagnostic::location(text, span.start)` /
`location(text, span.end)` で line / column を計算する。native IR に埋め込む文字列も、
side table JSON もこの source map から作る。source map がない `llvm::emit` 互換 wrapper では
`trap_info` を有効にできない。

source span は利用者が書いた式の span にする。
runtime `.ll` 内の trap は直接 source span を持たないため、runtime helper に呼び出し元で決めた site id を渡す。

### native

trap 情報が有効な native IR は、失敗 block で次を行う。

```llvm
call void @tz.trap.report(i32 <site_id>)
call void @llvm.trap()
unreachable
```

`@tz.trap.report` は internal/hidden runtime helper とし、stderr に次の形式で 1 行を出す。

```text
trap: <kind> at <path>:<line>:<column>
```

例:

```text
trap: integer division by zero at Main.tz:12:17
```

native は libc を利用できる。既存 runtime も native で `malloc/free` を参照し、console wrapper は
`putchar` を参照している。`@tz.trap.report` は `fprintf(stderr, ...)` または `fwrite` ベースでよい。

library object build でも、`--trap-info` 有効時は libc の `stderr` / `fprintf` 参照が残る。
これは `malloc/free` と同じく native host link 時に解決する。

### WASM

WASM は既定で import なしを維持する。
trap 情報が有効な WASM IR は、trap 直前に internal global へ site id を保存する。

```llvm
@tsuzuri_trap_site = global i32 0

store i32 <site_id>, ptr @tsuzuri_trap_site
unreachable
```

WASM の data/global symbol を export しても JavaScript host が得るのは値ではなく address / global object の扱いで混乱しやすい。
G04 では値を返す getter function を export する。

```llvm
define i32 @tsuzuri_trap_site() {
entry:
  %id = load i32, ptr @tsuzuri_trap_site
  ret i32 %id
}
```

生成 `.wasm` では `tsuzuri_trap_site` function を export する。
WASM module imports は引き続き空でなければならない。

side table は output の隣に JSON Lines ではなく単一 JSON file として出す。

出力名:

```text
<output>.trap.json
```

例:

```json
{
  "version": 1,
  "sites": [
    {"id":1,"kind":"integer division by zero","path":"Main.tz","span":{"start":30,"end":35,"line":2,"column":10,"end_line":2,"end_column":15}}
  ]
}
```

理由:

- custom section は host 側 tooling が読みづらい。
- data segment に JSON を埋めると module size と runtime memory を増やす。
- 隣接 JSON は deterministic でレビューしやすい。

`--emit llvm` の場合は side table を `<output>.trap.json` に出す。
stdout/stdin には出さない。

### flag / default policy

既定:

- `tsuzuri run`: trap info **有効**。
- `tsuzuri build`: trap info **無効**。
- `tsuzuri build --trap-info`: trap info 有効。

理由:

- `run` はローカル開発用で、source path を表示する価値が高い。
- `build` の成果物は配布物になり得るため、既定で source path / side table / extra libc refs を増やさない。
- 明示 `--trap-info` なら配布者の意思で有効化できる。

`check` に `--trap-info` を指定したら `E2000`。

### performance / IR policy

- 成功 hot path に追加の report call / global store を入れない。
- runtime helper に site id を渡すため、trap-capable helper call の引数が 1 個以上増えることは許容する。
  これは success path の call operand 増加であり、report/store を success path に置くことではない。
- site id store / report call は runtime helper 内を含め failure block にだけ置く。
- `-O3` で trap failure block は cold path のまま。
- `--trap-info` false の IR は既存と byte-identical であることを確認する。
  `--trap-info` true では helper call の site id operand 増加を許容し、report/store が failure block だけにあることを確認する。
- site id は deterministic。source id, span start, kind の順ではなく、IR 生成の決定的順序で 1 から採番してよい。
  ただし同じ入力で 2 回 build した side table は byte-identical。

## 設計

### `llvm` API

既存:

```rust
pub fn emit_target(module: &CheckedModule, entry: Entry, wasm: bool) -> Result<String, Diagnostic>
```

追加:

```rust
pub struct EmitOptions {
    pub entry: Entry,
    pub wasm: bool,
    pub trap_info: bool,
}

pub struct EmitOutput {
    pub ir: String,
    pub trap_sites: Vec<TrapSite>,
}

pub fn emit_with_options(
    module: &CheckedModule,
    options: EmitOptions,
    sources: &[TrapSource<'_>],
) -> Result<EmitOutput, Diagnostic>;
```

既存 `emit` / `emit_target` は `trap_info: false` で wrapper にする。
`trap_info: true` なのに `sources` が空、または `Span.source` が範囲外なら `E2000` ではなく compiler internal bug として
`Diagnostic` を返す。`driver::build` / `run` は常に `Project` の ordered source list から `TrapSource` を渡す。

### `FunctionEmitter`

`FunctionEmitter` に trap registry を渡す。

```rust
struct TrapRegistry {
    enabled: bool,
    sites: Vec<TrapSite>,
}

impl TrapRegistry {
    fn site(&mut self, kind: TrapKind, span: Span) -> u32;
}
```

`FunctionEmitter::guard` を置き換える。

```rust
fn guard(&mut self, valid: &str, kind: TrapKind, span: Span)
```

既存の `guard(&valid)` 呼び出しはすべて kind/span 付きへ変更する。

| 呼び出し元 | kind | span |
|---|---|---|
| `binary` の rhs zero | `IntegerDivisionByZero` | binary expression span |
| `binary` の `MIN / -1` | `IntegerDivisionOverflow` | binary expression span |
| `allocation_size` | `AllocationSize` | `new` expression span |
| string index | `BoundsCheck` | index expression span |
| `checked_element_pointer` | `BoundsCheck` | index expression span |
| `ListTail` | `PatternMismatch` | tail pattern span |
| `range_loop` step 0 | `RangeStepZero` | range expression span |
| `match_expression` failure | `MatchFailure` or `PatternMismatch` | match / generated pattern span |

`guard` に適切な span を渡すため、必要なら helper signature を広げる。

例:

```rust
fn checked_element_pointer(
    &mut self,
    ty: &Type,
    collection: &str,
    index: &str,
    span: Span,
) -> String
```

整数除算 / 剰余は既存 `binary` が zero と `MIN / -1` を 1 つの `valid` にまとめているため、
G04 では次の順序に分ける。

1. rhs == 0 なら `IntegerDivisionByZero` の failure block へ分岐。
2. signed `MIN / -1` なら `IntegerDivisionOverflow` の failure block へ分岐。
3. どちらでもなければ実際の `sdiv` / `udiv` / `srem` / `urem`。

この順序により、`MIN / 0` は zero division として報告される。

### assert

既存 `Builtin::Assert` は `emit_builtin` の wrapper 内で trap するため、call site span を失う。
G04 では `assert` を call site lowering にする。

方針:

- `Specializer::lower` または `closures::lower_expression` の前に、`assert` 呼び出しを専用 `TypedExprKind::Assert(Box<TypedExpr>)`
  に変換する。
- LLVM では `Assert` expression span で `TrapKind::Assert` を出す。
- `emit_builtin(Builtin::Assert)` は削除するか、通常経路で到達しないようにする。
- 既存 `assert : bool -> unit` の型と評価順序は維持する。

### runtime trap

runtime `.ll` 内の trap は source span を持たない。
pre-call global `current site` store は success path に store を入れるため禁止する。
代わりに、trap-capable runtime helper は site id を引数として受け取り、失敗 block の中でだけ report/store する。

代表的な signature 変更:

```llvm
declare ptr @tz.alloc(i64 %size, i32 %site)
declare %tz.string @tz.string.allocate(i64 %length, i32 %allocation_site)
declare %tz.string @tz.string.new(ptr %source, i64 %length, i32 %allocation_site)
declare %tz.string @tz.string.concat(
  %tz.string %left,
  %tz.string %right,
  i32 %overflow_site,
  i32 %allocation_site
)
```

複数の失敗理由を持つ helper は、失敗理由ごとに site id を別引数にする。
helper 内部からさらに trap-capable helper を呼ぶ場合は、受け取った site id をそのまま渡す。

inventory と propagation:

| trap-capable helper / generated call | 失敗 | site id の渡し方 |
|---|---|---|
| `@tz.alloc` (`heap-native.ll`, `heap-wasm.ll`) | allocation failure / WASM memory.grow failure / 16 MiB 超過 | `@tz.alloc(size, allocation_site)` |
| `@tz.string.allocate` | allocation failure | `allocation_site` を `@tz.alloc` へ渡す |
| `@tz.string.new` | allocation failure | `allocation_site` を `@tz.string.allocate` へ渡す |
| `@tz.string.concat` | length overflow / allocation failure | `overflow_site` と `allocation_site` を別々に受ける |
| closure environment clone wrappers `@tz.env.clone.*` | environment allocation failure | wrapper parameter または baked constant site を `@tz.alloc` へ渡す |
| `FunctionEmitter::make_closure` | environment allocation failure | closure 作成 expression span の allocation site を `@tz.alloc` へ渡す |
| `FunctionEmitter::append_list` | list node allocation failure | list literal / clone / relocation の span 由来 site を渡す |
| `FunctionEmitter::allocate_array` | array allocation failure | array/new expression span の allocation site を渡す |
| `llvm_frame::heap_copy` / `relocate` | frame から heap へ移す string/array/list の allocation failure | move/copy の元 expression span から site を作り、`string.new` / `append_list` / `allocate_array` へ渡す |
| soft numeric helpers `tz_soft_op`, `tz_soft_cmp`, `tz_soft_cast`, `tz_soft_format` | `numeric.c` 内の `__builtin_trap()` | helper signature に `i32 site` を追加し、`numeric.c` の trap path で report/store してから trap。`generate.py` で `numeric.ll` を再生成 |
| `runtime/wasm.ll` 128-bit helper | helper internal trap | call site から `WasmRuntime` site を渡す。helper signature 変更が大きすぎる場合は G04 で対象 helper を限定し、未対応 helper は未決事項に残す |

新しい runtime `.ll`（例: `src/runtime/trap-native.ll`）を追加する場合は、G01 と同じ規則で
`.gitignore` に `!src/runtime/trap-native.ll` を追加し、`git --no-pager ls-files --error-unmatch src/runtime/trap-native.ll`
を受け入れ条件に入れる。

### driver / side table

`BuildOptions` に field を追加する。

```rust
pub struct BuildOptions {
    pub target: Target,
    pub emit: Emit,
    pub optimization: u8,
    pub cpu: Cpu,
    pub trap_info: bool,
}
```

`Default` は `trap_info: false`。
`run` は受け取った options をコピーし、`trap_info: true` にして build する。

`driver::build` は `llvm::emit_with_options` を呼ぶ。
`trap_info` が true かつ `Emit != Header` の場合:

- IR artifact を生成。
- native / wasm の成果物と同じ publish timing で `<output>.trap.json` を atomic rename する。
- build 失敗時は side table も既存 output も変更しない。
- `protect_sources` は side table path にも適用する。
- `driver::Project` は `TrapSource { path, text }` の配列を作り、`emit_with_options` へ渡す。

`--emit header` は codegen がないため `--trap-info` と併用したら `E2000`。

### `tsuzuri run`

`driver::run` は trap info 有効の executable を一時 directory に build する。
子プロセスは `Command::output()` で実行し、stdout/stderr を捕捉する。

- 成功時: child stdout/stderr を親へそのまま出す（既存 console output を維持）。
- 失敗時:
  - child stderr に `trap: ...` があれば、それを `E2005` message に含める。
  - human では trap line を先に出してから `E2005` を出してよい。
  - JSON では `E2005.message` に trap line を含め、stderr を JSON object だけにする。

これにより `--json` で非 JSON 行が混ざることを避ける。

### 前提とする他チケットのインターフェース

なし。

### 他チケットへの提供インターフェース

- G08 debug info は `TrapSite` side table の source span mapping を再利用できる。
- E07 debug output が host stderr を扱う場合、native stderr policy を合わせる。

### 各コンパイラ段への変更

| 段 | 変更 |
|---|---|
| lexer / parser | 変更なし |
| check | `TypedExprKind::Assert` を追加する場合のみ children 更新 |
| computation | `assert` を通常関数呼び出しとして展開する。変更なし |
| control | match / pattern mismatch の span を `llvm_control` へ保持 |
| polymorph | `assert` 専用 lowering が必要なら builtin lowering を調整 |
| closures | builtin assert wrapper を作らないよう調整 |
| ownership | `Assert` が追加される場合、children / eval を追加。評価順序は condition のみ |
| llvm | trap registry, trap-aware guard, native report, wasm global / side table |
| runtime | native report helper、heap/string/closure/numeric など trap-capable helper への site id 引数追加 |
| driver | `trap_info`, side table atomic publish, run output capture |
| main | `--trap-info` option と validation |

## 実装手順

1. **trap data structures を追加する。**
   - `TrapKind`, `TrapSite`, `TrapRegistry` を `src/llvm.rs` または新 `src/trap.rs` に追加。
   - JSON side table writer は `driver` 側に置く。
   - 確認: unit test で deterministic JSON を生成。

2. **source map 付き `emit_with_options` を追加する。**
   - 既存 `emit` / `emit_target` は wrapper にする。
   - `EmitOutput { ir, trap_sites }` を返す。
   - `TrapSource { path, text }` を `Project` から渡し、native IR と side table の path/line/column はここから生成する。
   - trap info false では IR が既存と byte-identical であることを test する。

3. **`FunctionEmitter::guard` を kind/span 付きにする。**
   - すべての call site を更新する。
   - まず `TrapKind::BoundsCheck` など conservative な分類でよいが、整数 division zero と overflow は分ける。
   - 確認: `rg "guard\\(" src/llvm*.rs` で旧 signature が残らない。
   - 整数除算 / 剰余は zero と overflow の 2 つの ordered failure branch に分ける。

4. **assert call site span を保持する。**
   - `TypedExprKind::Assert` を追加するか、`Call` lowering で builtin assert id を特別扱いする。
   - 既定案は `TypedExprKind::Assert`。
   - `TypedExpr::children` / `children_mut`, `ownership`, `closures`, `polymorph`, `call_specialization` を更新する。
   - 確認: `assert false` の trap site が assert call の span になる。

5. **native reporter を実装する。**
   - `src/runtime/trap-native.ll` を追加するか、`llvm.rs` が小さな IR fragment を生成する。
   - `.ll` ファイルを追加する場合は `.gitignore` 例外と `git ls-files --error-unmatch` 検査を追加する。
   - 文字列 table を LLVM global として出す。
   - `@tz.trap.report(i32)` が `trap: <kind> at <path>:<line>:<col>\n` を stderr に出す。
   - `--trap-info` false では reporter を含めない。
   - 確認: native IR に success path の reporter call がない。

6. **WASM global、getter、side table を実装する。**
   - internal `@tsuzuri_trap_site` global と `define i32 @tsuzuri_trap_site()` getter を trap info true / wasm のみ出す。
   - `wasm-ld` export に getter function `--export=tsuzuri_trap_site` を追加する。
   - side table JSON を `<output>.trap.json` に atomic publish する。
   - `WebAssembly.Module.imports(module)` が空であることを test する。

7. **runtime helper へ site id を plumb する。**
   - pre-call global store は使わない。
   - `@tz.alloc`、string helpers、closure clone adapters、list allocation、frame relocation / heap_copy、
     soft numeric helpers、必要な WASM helpers の signature に site id を追加する。
   - 各 helper の failure block だけで native report または WASM global store を行う。
   - `numeric.c` を変更した場合は `python3 src/runtime/generate.py` で `numeric.ll` を再生成し、
     byte-identical / intentional diff を確認する。

8. **CLI option と policy を追加する。**
   - `--trap-info` を build/run options に追加。
   - check/header との併用を `E2000`。
   - run は常に trap_info true。
   - help / README を更新。

9. **`driver::run` を JSON-safe にする。**
   - `Command::status()` から `Command::output()` に変える。
   - 成功時の stdout/stderr relay を維持。
   - 失敗時の trap line を `E2005` に含める。
   - 既存 run tests を更新。

10. **IR hot path 差分を確認する。**
    - 同じ fixture で `--trap-info` false の IR が実装前後で同一。
    - `--trap-info` true の IR で report/store が failure block だけにある。
    - `-O3` build の動作を native/WASM で確認。

## テスト計画

### Rust tests

新規 `tests/trap_locations.rs`。

IR false:

```rust
let module = analyze("export def f :: i64 -> i64\nfn f x = 10 / x").unwrap();
let old = llvm::emit_target(&module, llvm::Entry::Library, false).unwrap();
let new = llvm::emit_with_options(&module, EmitOptions { trap_info: false, ... }).unwrap().ir;
assert_eq!(old, new);
```

IR true:

- `10 / x` に `@tz.trap.report` または `@tsuzuri_trap_site` store が failure block だけに出る。
- side table に `integer division by zero` がある。
- `(-9223372036854775808) / -1` は `integer division overflow`、`(-9223372036854775808) / 0` は
  zero-first ordering により `integer division by zero`。
- `assert false` は `assertion failed`。
- `xs[i]` は `index out of bounds`。
- `match x with | 0 -> 0` は `non-exhaustive match`。
- `for i in 1 .. 0 .. 10 do ()` は `range step is zero`。

determinism:

- `emit_with_options` を 2 回呼び、IR と side table が一致。

### Node E2E

`tests/e2e.mjs` または新 `tests/traps.mjs`。

native run:

```text
let _ = assert(false)
```

期待:

- `tsuzuri run Main.tz --json` は status 1。
- stderr は JSON 1 行。
- `code == "E2005"`。
- message に `trap: assertion failed at ...Main.tz:1:` を含む。

native build:

- `tsuzuri build Main.tz -o app` は trap info なし。
- `tsuzuri build Main.tz --trap-info -o app` は trap info あり。
- trap 時に stderr に source location。

WASM:

- `tsuzuri build Main.tz --target wasm32 --trap-info -o app.wasm`
- `app.wasm.trap.json` が存在。
- `WebAssembly.Module.imports(module)` は `[]`。
- trap する export call 後、`instance.exports.tsuzuri_trap_site()` が side table id と一致。
- `--trap-info` なしでは `tsuzuri_trap_site` export と side table がない。

output protection:

- 既存 `app.wasm.trap.json` を置き、build 失敗時に内容が保持される。
- source file へ side table を書こうとした場合 `E2003`。

### native/WASM × O0/O3

trap 情報あり:

```sh
node tests/e2e.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
```

Node E2E の直前には GUIDE §3 に従い、必ず `cargo build --release --locked` で
`target/release/tsuzuri` を更新する。

G04 専用 test では `-O0` / `-O3` の両方で trap site id が一致することを確認する。

## ドキュメント

- `docs/language.md` の trap 節に、`run` で source location を報告することを追記する。
- `README.md` の CLI option 表に `--trap-info` を追加する。
- `docs/architecture.md` に trap site id / native reporter / WASM side table / no imports を記載する。
- `docs/architecture.md` の WASM 不変条件に、`--trap-info` でも imports を増やさないことを明記する。

## 受け入れ条件

- [ ] `tsuzuri run` の trap が source path / line / column / kind を報告する。
- [ ] `--json` の run failure stderr が JSON object のみになる。
- [ ] `tsuzuri build --trap-info` が native reporter または WASM side table を出す。
- [ ] `tsuzuri build` 既定では trap info を出さない。
- [ ] WASM trap info は import を追加しない。
- [ ] WASM は `tsuzuri_trap_site()` getter function を export し、trap 後に id を読める。
- [ ] side table JSON は deterministic で atomic に publish される。
- [ ] assert, bounds, integer division, match failure, allocation size/failure, range step zero が分類される。
- [ ] `--trap-info` false の IR は既存と同一。
- [ ] `--trap-info` true の report/store は failure block のみ。trap-capable helper call の site id operand 増加は許容する。
- [ ] 新しい runtime `.ll` を追加した場合、`.gitignore` 例外と `git ls-files --error-unmatch` 検査がある。
- [ ] native/WASM × `-O0`/`-O3` の trap tests が通る。

## 落とし穴

- builtin assert wrapper 内で trap すると call site span を失う。
- runtime `.ll` 内の trap は source span を知らないため、呼び出し側で作った site id を helper 引数として渡す必要がある。
- `CheckedModule` / `Span` には path/text がない。source map なしに native IR の `path:line:column` は作れない。
- pre-call global current-site store は success path store になり、cold-path-only 方針に反する。
- `tsuzuri run --json` で child stderr をそのまま流すと JSON 以外の行が混ざる。
- build 既定で source path を成果物へ埋め込むと配布物の情報漏えいになる。
- WASM import を増やすと D-18 に違反する。
- report call / global store を hot path に置くと性能退行になる。site id operand の追加と混同しない。
- side table publish を output publish と別管理にすると、片方だけ更新される失敗モードが生まれる。

## 対象外

- 回復可能な例外。
- stack trace。
- DWARF / source map（G08）。
- panic unwinding / destructor 実行。
- browser console への自動出力。
- trap を `Result` に変えること。

## 未決事項

- **native reporter の実装方式。** 既定案は `fprintf(stderr, ...)`。より小さくするなら `fwrite` + global string table。
- **side table path。** 既定案は `<output>.trap.json`。`--emit object` でも同じ。
- **runtime helper signature 変更の範囲。** 既定案は G04 で `@tz.alloc`、string helpers、closure clone adapters、
  list allocation、frame relocation / heap_copy、soft numeric helpersまで site id を通す。`runtime/wasm.ll` の
  128-bit helper で signature 変更が過大な場合だけ、未対応 helper を明示して後続に分ける。
- **台帳の見直し提案:** なし。
