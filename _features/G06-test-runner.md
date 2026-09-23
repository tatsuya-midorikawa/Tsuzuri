# G06: 言語内テスト（test 宣言と tsuzuri test）
| 項目 | 内容 |
|---|---|
| ID | G06 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | 弱依存 D01 |
| 後続 | G07 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/llvm.rs`, `src/driver.rs`, `src/main.rs`, `tests/test_runner.rs`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md`, 将来 `std/Test.tz` |

## 目的

Tsuzuri のソース内に軽量な `test` 宣言を書き、`tsuzuri test` で実行できるようにする。
テスト本体は通常の型検査・所有権検査を受け、trap を失敗として扱う。

初期版は `assert` を使う fallback を中心にし、D01 / E02 後に `Test.equal` などの helper を標準ライブラリへ追加する。

## 現状

トップレベル宣言は `record`, `class`, `instance`, `def` / `fn` / `let` 実装、entry code である。

`src/syntax.rs`:

- `TokenKind` に `Test` はない。
- `Program` に tests field はない。
- `FunctionDecl` は通常関数だけ。

`src/lexer.rs`:

- `Lexer::identifier` の keyword table に `test` はない。

`src/parser.rs`:

- `Parser::program` は top-level loop で `record`, `class`, `instance`, `let` function implementation,
  `fn` / `def` / `export` / `and`, それ以外 entry として扱う。

`src/check.rs`:

- `check_modules` は `CheckedModule { records, functions, entry }` を返す。
- tests の格納先はない。
- top-level entry は `Main.tz` だけ。

`src/main.rs`:

- CLI action は `Check`, `Build`, `Run`。

## 仕様

### 構文

D-15 に従い `test` を新しい予約語にする。

```text
test "adds numbers" = assert (add 1 2 == 3)

test "block form" =
    let value = add 20 22
    assert (value == 42)
```

EBNF:

```text
TestDecl ::= "test" StringLiteral "=" BodyExpression
```

制約:

- test 名は string literal。
- 空文字列は許可するが、report では `Module:<index>` を併記する。
- 同じ module 内の test 名重複は許可する。識別は `(source order index, module, name)`。
- `test` はトップレベル宣言であり、block 内には書けない。
- `test` に `export`, `private`, `rec`, `and` は付けられない。
- test body の型は `unit`。
- body は通常式 / block / indentation body。
- `return` は task / computation expression 内の既存意味だけ。test からの早期 return は導入しない。

### 許可する source kind

初期版:

| source kind | `test` |
|---|---|
| `.tz` | 許可 |
| `.tc` | 許可 |
| `.tt` | `E1018` で拒否 |
| 拡張子なし in-memory `analyze` | 互換のため許可 |

理由:

- `.tz` は通常コードの tests。
- `.tc` は builder 操作の tests を同じ builder module に置ける。
- `.tt` は型クラス宣言だけという既存仕様を維持する。

### 型検査 / build での扱い

- test 宣言は通常の `check` / `build` でも常に型検査する。
- test body の型が `unit` でなければ `E1003`。
- test は通常 build の LLVM 出力に含めない。
- test は module 間の通常関数名前空間に入らない。
- test から private item を参照できるかは E01 後の visibility に従う。既定は同じ module 内の private は参照可。
- test が未使用関数扱いで W1002 を発生させないよう、G03 の reachability root に含める。

### CLI

```text
tsuzuri test <file|dir> [--filter TEXT] [--json] [-O0|-O1|-O2|-O3] [--target native|wasm32]
```

入力は `check` と同じく file または directory。
directory は直下の `.tz/.tt/.tc` を読み込み、`Main.tz` がなくてもよい。

options:

| option | 意味 |
|---|---|
| `--filter TEXT` | `Module.test name` に substring match した tests だけ実行 |
| `--json` | 1 行 1 JSON object で結果を stdout、diagnostics を stderr |
| `-O0..-O3` | runner build の最適化。既定 `-O0` |
| `--target native|wasm32` | 既定 `native`。`wasm32` は Node.js が必要 |

`--cpu` は初期版では `test` に対応しない。指定したら `E2000`。
理由: test は再現性を優先し、CPU native tuning は performance tests / benchmarks で扱う。

### 実行 semantics

- test は source order で index を持つ。
- selected tests は index 順に report する。
- 各 test は別 process で実行する。
- trap / signal / non-zero exit / timeout は test failure。
- 1 test failure が他 test の実行を止めない。
- 最終 exit code:
  - 全 test pass: `0`
  - 1 件以上 fail / timeout / runner build error: `1`
  - CLI 引数エラー: `2`
- test failure は summary diagnostic `E2006` として扱う。

### report

human stdout:

```text
ok 1 - Main adds numbers
not ok 2 - Main division traps
  failure: trapped

1 passed; 1 failed; 0 ignored
```

stderr:

- compile diagnostics。
- runner build diagnostics。
- final `E2006` diagnostic if failed。

JSON stdout:

```json
{"type":"test","index":0,"module":"Main","name":"adds numbers","status":"passed","duration_ms":3}
{"type":"test","index":1,"module":"Main","name":"division traps","status":"failed","failure":"trapped","duration_ms":2}
{"type":"summary","passed":1,"failed":1,"ignored":0,"duration_ms":5}
```

JSON stderr:

```json
{"severity":"error","code":"E2006","message":"1 test failed","path":"Main.tz","span":{"start":...}}
```

`E2006` の span は最初に失敗した test 宣言の name span。

### timeout

初期版は固定 timeout を持つ。

- 既定: 30 秒 / test。
- CLI option は追加しない。
- 環境変数 `TSUZURI_TEST_TIMEOUT_MS` を認めるかは未決。既定案は認めない。

timeout は failure:

```text
failure: timed out after 30s
```

### parallel execution

`std::thread::available_parallelism()` を使い、bounded worker pool を作る。

- worker 数: `min(available_parallelism, 32, selected_test_count)`。
- worker 数が取得できない場合は 1。
- 実行は並列でも report は index 順。
- 共有 runner executable は read-only。
- 各 test は `std::process::Command` で別 process。

### native runner

1 つの native executable を build し、argv の index で 1 test を実行する。

擬似 IR / C ABI:

```llvm
define i32 @main(i32 %argc, ptr %argv) {
entry:
  ; parse argv[1] strictly as unsigned decimal test index
  switch i32 %index, label %bad [
    i32 0, label %test0
    i32 1, label %test1
  ]
test0:
  call i8 @tz.test.Main.0()
  ret i32 0
test1:
  call i8 @tz.test.Main.1()
  ret i32 0
bad:
  ret i32 2
}
```

test function は `unit` を返す internal function。
trap すれば process が異常終了し、親 process が failure として扱う。

index parsing:

- `argc == 2` でなければ exit code 2。
- `argv[1]` は `strtoul` または `strtoull` で strict に読む。
- `errno = 0` を設定してから呼ぶ。
- `endptr` が `argv[1]` と同じなら digit なしで exit 2。
- `*endptr != '\0'` なら trailing garbage で exit 2。
- 先頭 `-` / `+` は許可しない。
- `errno == ERANGE` または `value >= test_count` なら exit 2。
- parsed value が `i32` / `usize` runner table の範囲を超える場合も exit 2。

`atoi` は malformed input と `0` を区別できず、overflow も検出できないため使わない。

### WASM runner

`--target wasm32` は明示要求時だけ。

- Node.js (`node`) が `PATH` にない場合は `E2002`。
- WASM module は import なし。
- runner は export `tsuzuri_test_count` と `tsuzuri_test_run` を持つ。

```text
export def tsuzuri_test_count :: i32
export def tsuzuri_test_run :: i32 -> i32
```

ただしユーザー名前空間の `tz_` と混ぜないため、実際の wasm export は D-17 に従い
`tsuzuri_test_count`, `tsuzuri_test_run` とする。

Node wrapper は test ごとに別 process を起動し、指定 index を実行する。
`WebAssembly.RuntimeError` は failure。

### `Test` std module helpers

D01 / E02 後:

```text
Test.equal : (Eq 'a, Display 'a) => &'a -> &'a -> unit
Test.not_equal : (Eq 'a, Display 'a) => &'a -> &'a -> unit
Test.is_true : bool -> unit
```

`Test.equal` / `Test.not_equal` は A11 の borrowed `Eq` を使い、期待値・実測値を消費しない。リテラルや一時値を直接借用できない現行仕様では、必要に応じて `let expected = ...` のように束縛してから `Test.equal (&expected) (&actual)` と呼ぶ。

`Display` がない現時点では、G06 は std `Test` module を必須にしない。
fallback:

```text
test "adds numbers" = assert (add 1 2 == 3)
```

D01 が完了している場合だけ、別 phase で `std/Test.tz` を追加する。

## 設計

### AST

`src/syntax.rs` に追加。

```rust
pub struct Program {
    pub source_kind: Option<SourceKind>,
    pub records: Vec<RecordDecl>,
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
    pub instances: Vec<InstanceDecl>,
    pub active_patterns: Vec<ActivePattern>,
    pub tests: Vec<TestDecl>,
    pub entry: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct TestDecl {
    pub name: String,
    pub name_span: Span,
    pub body: Expr,
    pub span: Span,
}
```

`TokenKind::Test` を追加し、`Lexer::identifier` に `"test" => TokenKind::Test` を追加する。

### parser

`Parser::program` の top-level loop で `TokenKind::Test` を `entry` より前に扱う。

```rust
} else if self.eat(&TokenKind::Test) {
    let token = self.expect(&TokenKind::String(String::new()), "a test name string")?;
    self.expect(&TokenKind::Equal, "'=' after the test name")?;
    let body = self.body_expression()?;
    program.tests.push(TestDecl { ... });
    self.eat(&TokenKind::Semicolon);
}
```

`test` は keyword になるため、既存の `test` 識別子は使えなくなる。
`tests/frontend.rs` に keyword rejection を追加する。

### check

`CheckedModule` に tests を追加。

```rust
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
    pub tests: Vec<CheckedTest>,
}

pub struct CheckedTest {
    pub module: String,
    pub name: String,
    pub index: usize,
    pub body: TypedExpr,
    pub span: Span,
    pub name_span: Span,
}
```

`check_modules`:

- `.tt` に tests があれば `E1018`。
- `.tz` / `.tc` / source kind none は許可。
- function body 検査と同じ `Checker` で body を `Type::Unit` 期待で検査。
- tests は `functions` に入れない。
- recursion / polymorph / closures / ownership は tests も通す必要がある。

実装方針:

- tests を後段（recursion / polymorph / closures / ownership）へ通すため、callable として扱う。
- ただし `polymorph::specialize` は現在 non-generic function をすべて root にし、`closures::lower` は lambda を
  provenance なしで `module.functions` に追加し、`emit_target` は全 function を出す。
  そのまま tests を `functions` に混ぜると通常 build に test 本体や test 内 lambda が漏れる。
- G06 では callable origin / reachability を明示し、normal build と test runner build の emission set を分ける。

決定（GUIDE D-22）: 由来情報は `CheckedFunction.origin: FunctionOrigin` の一つにまとめる。G06 は
`FunctionOrigin` に `test: Option<usize>` を追加する（E02／G03 が先に `FunctionOrigin` を導入していればそれを拡張し、
未導入なら D-22 の定義どおりに構造体ごと導入する）。独自の `CheckedCallableKind`／`CallableOrigin` は作らない。

```rust
pub struct FunctionOrigin {
    pub module: ModuleOrigin,      // E02
    pub provenance: Provenance,    // G03
    pub parent: Option<usize>,     // E02: 生成関数の持ち主
    pub test: Option<usize>,       // G06: この関数が属する test の番号
}
```

- `test` 宣言の本体は `origin.test = Some(test_index)`。
- `specialize` と `closures::lower` が作る関数は、持ち主（`parent`）の `test` を継承する。
- 通常ビルドと test runner ビルドの出力集合は、E02 の `reachable_functions` の根（root）の選び方で切り替える
  （通常ビルドは `test.is_none()` の根だけ、test runner は選択した test の根だけ）。到達可能性の計算を二重に実装しない。

- 通常 function 由来の lambda / builtin wrapper / export bridge / intrinsic wrapper は normal emission set に属する。
- test 由来の lambda / specialized function / wrapper は test emission set に属する。
- shared helper は、normal と test の両方から到達する場合だけ両方に含めてよい。

実装は次のどちらかにする。

1. **origin propagation:** `specialize` と `closures::lower` が parent origin を追加 function へ伝播する。
   `emit_target(Entry::Library/Console)` は normal reachable set だけ、`Entry::TestRunner` は selected tests reachable set だけを出す。
2. **separate reachability:** 型検査済み module から normal roots と test roots を別々に walk し、
   specialization / closure lifting も root set ごとに実行する。

M 規模では 1 を既定案にする。いずれの場合も、normal library build に test function / test-only lambda /
test-only specialization が含まれないことを受け入れ条件にする。

### ownership

tests は引数なし unit function と同じ扱いで ownership check を受ける。
`ownership::check_functions` は `module.functions` に tests が含まれる場合そのまま検査できる。

### LLVM

`Entry` に追加。

```rust
pub enum Entry {
    Library,
    Console,
    TestRunner { target: TestTarget },
}

pub enum TestTarget {
    Native,
    Wasm,
}
```

通常 emit:

- `Library` / `Console` は normal reachable set のみ出す。
  `origin.test.is_some()` の関数と、test の根からだけ到達する生成関数は出さない。

test emit:

- selected tests の reachable set を出す。
- native は `main(argc, argv)` を出す。
- wasm は `@tsuzuri_test_count` / `@tsuzuri_test_run` を出す。

test function symbol:

```text
@tz.test.<Module>.<index>
```

`tz_` は public ABI に使うため、internal symbol だけにする。WASM export は `tsuzuri_*`。

### driver

追加:

```rust
pub struct TestOptions {
    pub target: Target,
    pub optimization: u8,
    pub filter: Option<String>,
    pub json: bool,
}

pub struct TestCase {
    pub index: usize,
    pub module: String,
    pub name: String,
    pub span: Span,
}

pub struct TestResult {
    pub case: TestCase,
    pub status: TestStatus,
    pub duration_ms: u128,
}

pub fn test(project: &Project, options: TestOptions) -> Result<Vec<TestResult>, Diagnostic>;
```

build directory:

- `env::temp_dir()` の `.tsuzuri-test-<pid>-<id>`。
- runner executable / wasm / node wrapper を置く。
- Drop で cleanup。

### main

`Action::Test` を追加。

parse:

- first arg `test`。
- input required。
- `--filter` requires value。
- `--target native|wasm32` allowed。
- `-O0..-O3` allowed。
- `--json` allowed。
- `-o`, `--emit`, `--cpu` は error。

`HELP` を更新する。

### 前提とする他チケットのインターフェース

- D01 / E02（弱依存）:
  - `Display`, `to_string`, std module infrastructure がある場合、A11 の borrowed `Eq` に合わせた `Test.equal` を追加できる。
  - ない場合は `assert` fallback のみ。
- G03:
  - W1002 reachability に tests を root として渡せる。

### 他チケットへの提供インターフェース

- G07 は test discovery に `CheckedModule.tests` または `driver::discover_tests` を使える。
- D01 は `Test.equal` の Display-based message を後から足せる。

### 各コンパイラ段への変更

| 段 | 変更 |
|---|---|
| lexer | `test` keyword |
| parser | `TestDecl`, `Program.tests`, top-level parse |
| check | tests の source kind validation / unit 型検査 / callable kind |
| computation | test body 内の computation expression を通常どおり展開 |
| control | test body 内の control を通常どおり検査 |
| polymorph | tests から参照される generic functions も特殊化 root に含める |
| closures | tests 内 lambda を lower するが、parent origin を保持して normal build に漏らさない |
| ownership | tests を引数なし function として check |
| llvm | `Entry::TestRunner`, native/wasm runner, normal/test emission set の分離 |
| runtime | 変更なし |
| driver | `test` build/run orchestration, process spawning, timeout, parallelism |
| main | `test` subcommand |

## 実装手順

1. **keyword / AST / parser を追加する。**
   - `TokenKind::Test`。
   - `Lexer::identifier`。
   - `Program.tests` / `TestDecl`。
   - `Parser::program`。
   - 確認:
     - `test "x" = assert true` parse OK。
     - `let test = 1` は keyword 化により parse error。

2. **source kind validation を追加する。**
   - `.tt` で `test` があれば `E1018`。
   - `.tz` / `.tc` は OK。
   - docs 更新。

3. **type checking を追加する。**
   - test body を `Type::Unit` 期待で `Checker::expression`。
   - `computation::expand` を test body にも適用。
   - tests を callable roots として後段へ渡す。
   - 確認:
     - `test "bad" = 42` -> `E1003`。
     - `test "ok" = assert true` -> OK。

4. **callable kind / emission exclusion を実装する。**
   - `FunctionOrigin` に `test: Option<usize>` を追加する（GUIDE D-22。構造体が未導入なら D-22 の定義で導入する）。
   - `polymorph::specialize` が test callable / test 由来 generic instantiation の origin を保持する。
   - `closures::lower` が test body 内 lambda、test 由来 builtin wrapper、test 由来 generated callable の origin を保持する。
   - normal build で test functions / test-only lambdas / test-only specializations が IR に出ないこと。
   - `llvm::emit(... Entry::Library)` に `@tz.test.`、test 名、test-only `$lambda` が含まれないこと。
   - tests が参照する helper は型検査されるが、normal build の emission set には入らない。

5. **native test runner IR を実装する。**
   - `Entry::TestRunner { Native }`。
   - argv index parse は `strtoul` / `strtoull` + `errno` + `endptr` で strict に実装する。`atoi` は使わない。
   - malformed / negative / trailing garbage / overflow / index out of range は exit code 2。
   - selected test function は trap しなければ exit 0。
   - 確認: 手動で runner `0`, `1`, `-1`, `+1`, `1x`, 空引数, 範囲外を実行。

6. **driver test orchestration を実装する。**
   - `Project::load` は `Main.tz` 不要で使えるよう注意する。
     - 現在 directory input は `Main.tz` を選ぶ。`test` では directory に `Main.tz` がなくてもよい。
     - `Project::load_for_test` または `Project::load_any_directory` を追加する。
   - selected tests を filter。
   - runner build。
   - process per test。
   - timeout。
   - bounded parallel worker。
   - deterministic report ordering。

7. **CLI `test` を追加する。**
   - `Action::Test`。
   - options parsing。
   - human / JSON output。
   - failure summary `E2006`。

8. **WASM mode を追加する。**
   - `Entry::TestRunner { Wasm }`。
   - `tsuzuri_test_count`, `tsuzuri_test_run` exports。
   - driver が Node.js を見つける。
   - test ごとに Node process を起動。
   - Node unavailable は explicit wasm32 の場合 `E2002`。
   - WASM imports empty を test。

9. **D01/E02 後の `Test` module plan を docs に追加する。**
   - G06 実装時点で D01/E02 が未完了なら code は追加しない。
   - 完了済みなら `std/Test.tz` を別 phase として実装してよい。

## テスト計画

### Rust tests

新規 `tests/test_runner.rs`。

parser:

```rust
let program = parser::parse("test \"adds\" = assert true").unwrap();
assert_eq!(program.tests.len(), 1);
```

typing:

```rust
analyze("test \"ok\" = assert true").unwrap();
assert_eq!(analyze("test \"bad\" = 42").unwrap_err().code, "E1003");
```

source kinds:

```rust
analyze_modules(&[("Main.tz", "test \"ok\" = assert true")]).unwrap();
analyze_modules(&[("Builder.tc", "def Return :: unit -> unit\nfn Return x = x\ntest \"ok\" = assert true")]).unwrap();
assert_eq!(
    analyze_modules(&[("Traits.tt", "test \"bad\" = assert true")]).unwrap_err().code,
    "E1018"
);
```

IR exclusion:

```rust
let module = analyze("test \"ok\" = assert true\nexport def f :: i64\nfn f = 42").unwrap();
let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
assert!(!ir.contains("@tz.test."));
```

test-only lambda / specialization exclusion:

```text
def id :: 'a -> 'a
fn id x = x

test "uses generic and lambda" =
    let f = x -> id x
    assert (f 1 == 1)

export def f :: i64
fn f = 42
```

期待:

- library IR に `@tz.test.` がない。
- library IR に test body 内 lambda の symbol がない。
- library IR に test-only `id` specialization がない。
- test runner IR には selected test と必要な lambda / specialization がある。

runner IR:

- `llvm::emit_test_runner` が `@tz.test.Main.0` と `@main` を含む。
- WASM runner が `@tsuzuri_test_run` を含む。
- native runner の malformed index tests:
  - 引数なし、余分な引数、`-1`, `+1`, `1x`, overflow, 範囲外が exit code 2。

### CLI E2E

`tests/e2e.mjs` または新 `tests/test_runner.mjs`。

fixture:

```text
def add :: i64 -> i64 -> i64
fn add x y = x + y

test "adds numbers" = assert (add 1 2 == 3)
test "trap is failure" = assert false
```

native:

- `tsuzuri test fixture --filter "adds"` -> status 0, `ok 1`。
- `tsuzuri test fixture` -> status 1, summary 1 failed, stderr `E2006`。
- JSON output lines parse。
- report order is source order even with parallel workers。

timeout:

- `def rec loop :: unit\nfn rec loop = loop()` in test。
- process times out and failure is reported.
  - この test は時間がかかるため、専用 runner で timeout を短縮できる hook が必要なら未決事項で判断。

WASM:

- `tsuzuri test fixture --target wasm32 --filter "adds"` -> pass if Node available。
- Node missing simulation via PATH override -> `E2002`。
- `WebAssembly.Module.imports` empty。

directory without Main:

- temp dir with `Math.tz` containing tests but no `Main.tz`。
- `tsuzuri test dir` works。
- `tsuzuri run dir` は従来どおり Main.tz 必須。

### validation

```sh
cargo test --locked --test test_runner
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
```

`cargo test --locked --test test_runner` は GUIDE §3 の通り `--test <ファイル名>` を使い、
出力の `running N tests` が 0 でないことを確認する。Node E2E の直前には
`cargo build --release --locked` を実行する。

G06 は runtime / numeric semantics を変えないが、runner は trap を扱うため native と WASM の `-O0` / `-O3` を確認する。

## ドキュメント

- `docs/language.md`:
  - `test` keyword を予約語一覧へ追加。
  - `test` declaration syntax。
  - `.tz` / `.tc` で許可、`.tt` で拒否。
  - test body は unit。
- `README.md`:
  - CLI に `tsuzuri test`。
  - `assert` fallback example。
- `docs/architecture.md`:
  - tests are type-checked but excluded from normal builds。
  - runner executable / separate process per test。
  - WASM test runner via Node and no imports。

## 受け入れ条件

- [ ] `test` が予約語になる。
- [ ] `test "name" = body` が parse される。
- [ ] `.tz` / `.tc` で tests が許可され、`.tt` では `E1018`。
- [ ] test body は `unit` 型でなければ `E1003`。
- [ ] `check` / `build` は tests を型検査する。
- [ ] normal build の LLVM / object / wasm に test runner が含まれない。
- [ ] normal build に test functions、test-only lambdas、test-only specializations が含まれない。
- [ ] `tsuzuri test` が native runner を build し、各 test を別 process で実行する。
- [ ] native runner は `atoi` ではなく strict parse を使い、malformed/out-of-range index は exit code 2。
- [ ] trap は test failure になる。
- [ ] failure summary は `E2006`。
- [ ] `--filter`, `--json`, `-O0..3`, `--target native|wasm32` が動く。
- [ ] parallel execution でも report order は deterministic。
- [ ] timeout がある。
- [ ] WASM mode は Node がある場合に動き、imports を増やさない。
- [ ] Node がない explicit wasm32 は error。
- [ ] D01/E02 がなくても `assert` fallback で使える。

## 落とし穴

- `export` は host ABI であり test discovery とは無関係。
- tests を通常 `functions` に混ぜたまま emit すると normal build に test code が入る。
- `specialize` は non-generic functions をすべて root にするため、test origin を持たせないと test-only specialization が漏れる。
- `closures::lower` は lambda を新しい `CheckedFunction` として追加するため、parent origin を伝播しないと test-only lambda が漏れる。
- tests を型検査しないと、壊れた tests が CI で見逃される。
- 同じ process 内で全 tests を実行すると、trap で runner 全体が落ちて後続 tests を実行できない。
- parallel execution の完了順をそのまま出すと report が非決定的になる。
- `Project::load` は directory を `Main.tz` に変換するため、test 用の directory loading を分ける必要がある。
- WASM に Node import を足すと D-18 に違反する。Node は runner host であり、生成 wasm は import なし。

## 対象外

- property based testing。
- snapshot testing。
- test attributes / ignore / expected failure。
- custom assertion messages（D01 後の `Test` module で扱う）。
- coverage。
- benchmark runner。
- async test / task cancellation。
- package-wide recursive test discovery。

## 未決事項

- **timeout の設定手段。** 既定案は 30 秒固定。CI のため短縮 hook が必要なら `TSUZURI_TEST_TIMEOUT_MS` を追加する。
- **`.tc` で test を許可するか。** 既定案は許可。builder 操作を同じ module で検査できるため。
- **`--cpu native` の扱い。** 既定案は G06 では非対応。必要なら後続で追加。
- **D01/E02/A11 が完了済みの場合の `Test` module。** 既定案は G06 phase 1 では `assert` fallback のみ、`Test.equal` は D01/E02/A11 後の小 phase。
- **台帳の見直し提案:** なし。
