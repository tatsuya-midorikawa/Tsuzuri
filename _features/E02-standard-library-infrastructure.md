# E02: 標準ライブラリの同梱機構
| 項目 | 内容 |
|---|---|
| ID | E02 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | E01 |
| 後続 | B01, C01, C02, C04, D01, D02, D03, D04, D05, A08, E03, E06, E07, F04 |
| 状態 | done |
| 主な影響ファイル | `std/`, `src/stdlib.rs`, `src/lib.rs`, `src/driver.rs`, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/main.rs`, `tests/modules.rs`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |
## 目的
Tsuzuri コンパイラに標準ライブラリのソースを同梱し、ユーザーが `std/` ファイルをコピーしなくても `Option`、`Array`、`Int`、`Debug` などを利用できる基盤を作る。
台帳 D-07 に従い、標準ライブラリはリポジトリ直下 `std/` に `.tz` / `.tt` / `.tc` として置き、`include_str!` でコンパイラに埋め込む。
driver 経由の実ファイル入力と、Rust API の `analyze_modules` の両方で、ユーザーソースに加えて std を常に読み込む。
std の public API は予約モジュール名で提供し、補助実装は E01 の `private` で隠す。
LLVM を必要としない intrinsic は `Task.run` と同じ修飾名付き builtin として扱い、標準ライブラリのモジュール名前空間に置く。
未使用の std 関数は IR に出さず、`--emit llvm` の出力と E2E のリンク時間を不要に増やさない。
`check` と `--emit llvm` は引き続き LLVM インストール不要で動作する。
## 現状
調査コミットは `19d8cdd`。
リポジトリに `std/` は存在しない。
`src/lib.rs` の `analyze(source)` は `analyze_modules(&[("Main", source)])` を呼ぶ。
`src/lib.rs` の `analyze_modules(sources: &[(&str, &str)])` は各 source を parse し、拡張子から `SourceKind` を設定して `check::check_modules` に渡す。
`src/driver.rs` の `Project::load` は入力ディレクトリ直下の `.tz` / `.tt` / `.tc` だけを列挙し、サブディレクトリを無視する。
`src/driver.rs` の `Project::load` は `Main.tz` を root として選び、`SourceFile { path, name, text }` を file name 順に並べる。
`src/driver.rs` の `Project::source_for` は `Diagnostic.span.source` を `Project.sources` の index として扱う。
`src/driver.rs` の `protect_sources` は `Project.sources` の全ファイルに対して出力保護を行う。
`src/check.rs` の `check_modules` は `modules: &[(&str, &Program)]` を受け、module name の検査、builder 収集、record/function/class/instance 収集、型検査、単相化、closure lowering、ownership を実行する。
`src/check.rs` の `Names` は module の origin を持たない。
`src/check.rs` の `Names::record` は自モジュール → 完全修飾名 → 一意 alias の順でレコードを解決する。
`src/check.rs` の `Builtin` は enum で `Sqrt`, `Floor`, `Ceil`, `Abs`, `ToFloat`, `ToInt`, `Assert`, `CloneString`, `TaskRun`, `TaskParallel` を持つ。
`src/check.rs` の `Builtin::name` は `"Task.run"` / `"Task.parallel"` も含む文字列を返す。
`src/check.rs` の `Builtin::signature` は `Signature` だけを返し、制約や複数型変数を持たない。
`src/check.rs` の `Checker::name` は無修飾 builtin を `Builtin::ALL.iter().find(|builtin| builtin.name() == name.text)` で解決する。
`src/check.rs` の `Checker::value_expression` は `Task.run` / `Task.parallel` を `ExprKind::Field(Name(Task), field)` の特別分岐で解決する。
`src/polymorph.rs` の `Checker::builtin` は `Builtin::signature().as_type()` を fresh 化して `TypedExprKind::Function(FunctionRef::Builtin(builtin))` を返す。
`src/llvm.rs` の `FunctionEmitter::call` は `FunctionRef::Builtin(builtin)` を `@tz.builtin.<name>` にして `builtins: BTreeSet<Builtin>` に登録する。
`src/llvm.rs` の `emit_builtin` は builtin ごとに固定 IR を生成し、LLVM intrinsic 宣言を `BTreeSet<String>` に登録する。
`src/llvm.rs` の `emit_target` は全 `module.functions` を常に出力する。
`src/llvm.rs` の `emit_target` は `@tz.string.`, `@tz.free`, `@tz.alloc`, `@tz_soft_`, `@tz.closure.`, `@tsuzuri_task_parallel(` の有無で runtime IR を条件付き連結する。
`tests/modules.rs` は sibling module loading、qualified function、record resolution、source id、output protection を検査する。
`tests/e2e.mjs` は `WebAssembly.Module.imports(module)` が空であることを各 fixture で確認している。
## 仕様
### std 配置
標準ライブラリのソースはリポジトリ直下の `std/` に置く。
ファイル種別は通常と同じ。
`std/Option.tc` は `Option` モジュール、`std/Math.tz` は `Math` モジュールになる。
E03 までは `std/` 内もサブディレクトリなしとする。
E03 後も台帳 D-07 の予約表は flat module 名を維持し、`Std.Option` には移動しない。
std source はコンパイラ binary に `include_str!` で埋め込み、実行時にファイルシステムを読まない。
ユーザーの作業ディレクトリに `std/` があっても、このチケットでは探索しない。
### 予約モジュール
台帳 D-07 の表にある標準ライブラリモジュール名を予約する。
予約名は次の通り。
```text
Option
Result
Array
List
Vec
String
Char
Math
Int
Debug
Parallel
Simd
Map
Set
Test
Gpu
```
ユーザー source file の stem が上記に一致したら `E1011`。
予約名の拒否は、その std モジュールが E02 時点で実ファイルを持たない場合も行う。
`Task` は既存通り組み込み型・名前空間として予約し続ける。
組み込みクラス名 `Display`, `Parse`, `Hash`, `Default` などは std module ではなく class namespace の予約名として後続チケットが扱う。
### 読み込み順と source id
全ユーザー source を従来通り決定的順序で読み込む。
`Span.source` は parse した配列 index のまま保持する。
driver の human/json 診断 path は std source なら `std/Option.tc` のように表示する。
std source には実ファイル path がないため、出力保護の対象に含めない。
`Project::input()` はユーザー root のままにする。
`Main.tz` entry 選択はユーザー `Main.tz` だけを見る。
### 名前解決
台帳 D-07 に従い、無修飾の型・case・record・class の解決順は参照元 origin で変える。
参照元が `ModuleOrigin::User` の場合:
1. 自モジュール。
2. ユーザー module 全体で一意。
3. std module 全体で一意。
4. 曖昧なら `E1004`。
5. private 参照なら E01 の `E1022`。
参照元が `ModuleOrigin::Std` の場合:
1. 自モジュール。
2. std module 全体で一意。
3. 曖昧なら `E1004`。
4. private 参照なら E01 の `E1022`。
std code はユーザー宣言を一切検索しない。
std source では他 std module の型・case・record・class を常に修飾して書く。
例: `Option.tc` から `Result` を使う場合は `Result.Result`、`Result.Ok` と書く。
関数は従来通り他モジュールから修飾必須。
std 関数も `Option.map`、`Array.sum` のように修飾必須。
`Module.name` の解決順は「source 定義の関数 → builtin namespace function」。
ユーザー module が予約名を使えないため、`Int.popcount` の `Int` は std/builtin namespace として安定する。
source 定義と builtin が同じ `Module.name` を持つ場合は source 定義を優先する。
ただし builtin-backed API functions は `Builtin` entry としてだけ存在させる。
std source に同じ qualified name の `def` を置いて builtin を shadow してはいけない。
compiler build/test 時に `Builtin::ALL` と `stdlib::SOURCES` を照合し、std source の `def` / `let` implementation が builtin qualified name と衝突したら失敗させる Rust test を追加する。
`Task.run` と `Task.parallel` はこの仕組みに移行する。
`Task` は引き続き module name として予約されるが、builtin namespace としては `Module.name` 解決に参加する。
`Checker::value_expression` は class method 解決より前に、その field expression が namespace function かを判定する。
namespace function とは、ローカル束縛で first segment が隠されておらず、visible source function または full-name builtin が存在する `Module.name`。
namespace function なら `Classes::method` を呼ばず、source function を先に解決し、なければ builtin を解決する。
これにより `Int.popcount` や `Task.run` を class method と誤認しない。
存在しない `Task.foo` の診断は既存の「Task has no function ...」を維持する。
### builtin namespace functions
`Builtin` は修飾名、複数引数、型変数、型クラス制約を表せる。
既存の `sqrt` など無修飾 builtin は互換のため残す。
新規 builtin は原則 `Module.name` の修飾名にする。
`Int.popcount` のような builtin は source に `.tz` 本体を持たなくても、型検査と codegen に参加する。
builtin の型スキームは `Scheme` と同等の情報を持つ。
builtin の制約は通常関数制約と同じ `Constraint` として `Checker.constraints` に追加する。
LLVM builtin symbol は concrete 型を含む決定的な名前にする。
例: `Int.popcount : Integer<'a> => 'a -> 'a` を `i64` で使う場合、内部名は `@tz.builtin.Int.popcount[i64]` または `@tz.builtin.Int.popcount.i64` のどちらかに統一する。
推奨は LLVM 識別子の quoting を避けるため `@tz.builtin.Int.popcount.i64`。
型名 mangling は `llvm.rs` に helper を置き、既存の単相化名と同じ順序で決定的にする。
builtin の IR は `emit_builtin(instance, intrinsics)` で concrete 型を受けて生成する。
LLVM intrinsic 宣言は従来通り `BTreeSet<String>` に入れて重複排除する。
演算子と builtin が同じ intrinsic を使う場合、必ず同じ宣言文字列を使う。
`unreachable : unit -> 'a` は D-21 の通り E02 の多相 builtin 機構で表せる必要がある。
生成名は concrete result type ごとに `@tz.builtin.unreachable.<mangled-T>` とし、LLVM 形は `define internal <T> @tz.builtin.unreachable.<mangled-T>(i8 %unit)`。
本体は `call void @llvm.trap()` と `unreachable` だけ。
`unreachable` は通常の一引数 builtin として、関数値化・closure lowering・所有権解析で特例を増やさない。
### std 関数の到達可能性 pruning
std source は常に型検査する。
unused std function は LLVM IR に出さない。
ユーザー関数は未使用でも従来通り IR に出す。
これは既存 IR テストの churn を避けるため。
到達可能性は単相化と `closures::lower` の後、最終 `CheckedModule.functions` に対して計算する。
root は全ユーザー origin 関数、entry、exported 関数。
全ユーザー関数を root にするため、ユーザー関数からだけ参照される std helper も出る。
std の private/public は IR pruning とは独立。
`$lambda`、`$task`、`$export` bridge、`$builtin` wrapper は生成元 function の `origin.module` と `origin.test` を継承し、`origin.parent = Some(owner_function_id)` を設定する。

E02 時点で `Provenance` を入れるなら、これら generated helper は `provenance: Provenance::Generated`、source function は `Provenance::User` とする。G03 が enum の中身を詳細化する。

`$builtin` wrapper は parent を持つ prunable generated function とし、typed reference から到達した場合だけ出す。
named record/union type instance の収集は D-03 に従い、実際に emit される関数・ラッパー・root の signature/body の型だけを scan する。
未使用 std module の関数・型定義・`@tz.builtin.*` wrapper は IR に出してはいけない。
後続 A02/B01 で std 型が増えたら、型定義 pruning を追加してよいが、関数 pruning の interface は変えない。
### 初期 std 内容
E02 の本体では大きな実 std API は追加しない。
機構を検査するため、最小の `std/Math.tz` を追加する。
```text
private def identity_f64 :: f64 -> f64
fn identity_f64 value = value

def zero :: f64
fn zero = identity_f64 0.0
```
`Math.zero` は実用 API ではなく、機構検査用の最小 public 関数。
ゼロ引数関数は `fn() -> T` の値なので、利用例は必ず `Math.zero()` と呼ぶ。
IR churn を避けるため、ユーザーが `Math.zero()` を使わない限り `@tz.fn.Math.zero` は出ない。
Rust テストでは test-only std injection を使い、実 std 内容に依存しない検査を優先する。
### check / --emit llvm
`tsuzuri check` は std source の parse/check を含むが、LLVM executable は不要。
std の `include_str!` は Cargo build 時に読み込むだけで、`check` 実行時にファイル I/O を増やさない。
### 前提とする他チケットのインターフェース
E01 の `Visibility` を使い、std helper は `private` にする。
A02 は `union` を std source に置けるようにする。
A05 は `type` alias を std source に置けるようにする。
D01 は `Display` と `to_string` を builtin class/function として追加し、E02 の builtin scheme を使う。
### 他チケットへの提供インターフェース
#### `src/stdlib.rs`
新規ファイル `src/stdlib.rs` を追加し、`src/lib.rs` から `pub mod stdlib;` で公開する。
公開 API は次を安定インターフェースにする。
```rust
pub const SOURCES: &[(&str, &str)] = &[
    ("std/Math.tz", include_str!("../std/Math.tz")),
];

pub const RESERVED_MODULES: &[&str] = &[
    "Option", "Result", "Array", "List", "Vec", "String", "Char",
    "Math", "Int", "Debug", "Parallel", "Simd", "Map", "Set", "Test", "Gpu",
];

pub fn is_reserved_module(name: &str) -> bool;

pub fn module_name(path: &str) -> Option<&str>;
```
`module_name("std/Option.tc")` は `Some("Option")`。
`module_name("std/Nested/Bad.tz")` は E02 では `None`。
E03 が階層 std を許す場合は、この関数を拡張し、既存呼び出し側の signature は変えない。
後続チケットが新しい std file を追加する例:
```rust
pub const SOURCES: &[(&str, &str)] = &[
    ("std/Math.tz", include_str!("../std/Math.tz")),
    ("std/Array.tz", include_str!("../std/Array.tz")),
];
```
#### Rust API
既存 API は維持する。
```rust
pub fn analyze(source: &str) -> Result<check::CheckedModule, diagnostic::Diagnostic>;

pub fn analyze_modules(
    sources: &[(&str, &str)],
) -> Result<check::CheckedModule, diagnostic::Diagnostic>;
```
この二つは default std を自動で追加する。
テスト hook として次を追加する。
```rust
pub fn analyze_modules_with_std(
    sources: &[(&str, &str)],
    std_sources: &[(&str, &str)],
) -> Result<check::CheckedModule, diagnostic::Diagnostic>;
```
`std_sources` は `("std/Name.tz", source)` の path 形式で渡す。
`std_sources == &[]` は std なしではなく、空の custom std として扱う。
通常の `analyze_modules` は `stdlib::SOURCES` を渡す。
`src/lib.rs` 内部には一つだけ parse entrypoint を置く。
```rust
pub struct SourceInput<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub origin: ModuleOrigin,
}

fn analyze_inputs(inputs: &[SourceInput<'_>]) -> Result<check::CheckedModule, diagnostic::Diagnostic>;
```
`analyze_inputs` は渡された順序そのままで parse し、`Span.source` と `Project.sources` の index を一致させる。
`analyze_modules_with_std` は user inputs と custom/default std inputs を一度だけ結合して `analyze_inputs` へ渡す。
`Project::analyze` は `Project.sources` をそのまま `SourceInput` に変換し、std を二重 append しない。
#### check module input
`check::check_modules` の内部入力を次に変更する。
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleOrigin {
    User,
    Std,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    User,
    Generated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionOrigin {
    pub module: ModuleOrigin,
    pub provenance: Provenance,
    pub parent: Option<usize>,
    pub test: Option<usize>,
}

pub struct ModuleInput<'a> {
    pub name: &'a str,
    pub program: &'a Program,
    pub origin: ModuleOrigin,
}

pub fn check_modules(modules: &[ModuleInput<'_>]) -> Result<CheckedModule, Diagnostic>;
```
既存 public API からは `ModuleInput` を直接触らなくてよい。
`ModuleInput` を tuple slice に戻す helper は作らない。
origin を落とさないため、既存の `(&str, &Program)` consumer はすべて `ModuleInput` または `ModuleView` を受けるように変更する。
必須変更箇所:
- `check::check_modules` 本体の全 loop。
- `computation::collect`.
- `Classes::collect`.
- `Classes::instances`.
- active pattern 収集。
- entry 選択。
- recursion 用 declaration metadata。
必要なら軽量 view を共有する。
```rust
#[derive(Clone, Copy)]
pub struct ModuleView<'a> {
    pub name: &'a str,
    pub program: &'a Program,
    pub origin: ModuleOrigin,
}
```
`CheckedFunction` には D-22 に従い `pub origin: FunctionOrigin` を一つだけ追加する。

E02 は `FunctionOrigin` を導入し、`module` と `parent` を設定する。

G03 が `provenance` を詳細化し、G06 が `test` を埋める。E02 時点で full struct を先に定義する場合は `provenance: Provenance::User`、`test: None` を既定値にする。

上の `Provenance` は E02 用の最小形で、G03 が `Generated(GeneratedKind)` などの詳細な variant に拡張する。

別 field として `function_origin`、`module_origin`、`generated_from` などを `CheckedFunction` に並立させない。

`CheckedRecord` に `pub origin: ModuleOrigin` を追加する。
後続の union/type alias は plain `origin: ModuleOrigin` のままでよい。
#### builtin API
`Builtin::signature` は廃止し、context-resolved template として次を追加する。
```rust
pub enum BuiltinType {
    Var(&'static str),
    Concrete(Type),                         // primitives/bool/unit/string only; no project ids
    Std { module: &'static str, name: &'static str, args: Vec<BuiltinType> },
    Array(Box<BuiltinType>),
    List(Box<BuiltinType>),
    Tuple(Vec<BuiltinType>),
    Reference(Box<BuiltinType>, bool),
    Function(Vec<BuiltinType>, Box<BuiltinType>),
    UnsignedOf(Box<BuiltinType>),           // same-width unsigned integer
    WidenOf(Box<BuiltinType>),              // double-width integer; undefined for 128-bit
}

#[derive(Clone, Debug)]
pub struct BuiltinScheme {
    pub parameters: Vec<BuiltinType>,
    pub result: BuiltinType,
    pub variables: Vec<&'static str>,
    pub constraints: Vec<BuiltinConstraint>,
}

#[derive(Clone, Debug)]
pub struct BuiltinConstraint {
    pub class: &'static str,
    pub ty: BuiltinType,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuiltinInstance {
    pub builtin: Builtin,
    pub types: Vec<Type>,
}

impl Builtin {
    pub const ALL: &'static [Builtin];
    pub fn name(self) -> &'static str;
    pub fn scheme(self) -> BuiltinScheme;
    pub fn llvm_name(self, types: &[Type]) -> String;
}
```
`BuiltinType::Concrete` には primitive/bool/unit/string だけを入れ、project-specific id を持つ `Type::Record` / `Type::Union` は入れない。
`BuiltinType::Std` は `Std { module: "Option", name: "Option", args: vec![BuiltinType::Var("a")] }` のように std-origin resolver で解決する。
`Checker::instantiate_builtin(&BuiltinScheme, span) -> Result<(Vec<Type>, Type), Diagnostic>` を追加する。
この関数は `Var` を fresh inference variable に写像し、`Std` を GUIDE D-07 の std-origin resolver で解決し、`Array`/`List`/`Tuple`/`Reference`/`Function` を再帰的に具体化する。
`UnsignedOf` と `WidenOf` は、その引数が call site で concrete integer に解決済みの場合だけ解決する。
引数が rigid type variable や未解決 inference variable の場合、phase 1 では `E1015`。
つまり type family builtin は generic code からは使えない。後続チケットが必要なら constraints solver の拡張を別途設計する。
`BuiltinInstance` が `Vec<Type>` を持つため `FunctionRef` は `Copy` ではなくなる。
`FunctionRef` は `Clone` を derive し、`FunctionRef` consumer は by-value match ではなく reference match へ直す。
`FunctionRef::Builtin(Builtin)` は次へ変更する。
```rust
Builtin(BuiltinInstance)
```
`BuiltinInstance { builtin, types }` を `FunctionRef::Builtin` の唯一の payload とし、別 field や parallel map に concrete 型を分散して保持しない。
`Checker::builtin` は次へ変更する。
```rust
pub(super) fn builtin(
    &mut self,
    builtin: Builtin,
    span: Span,
) -> Result<(TypedExprKind, Type), Diagnostic>;
```
この関数は builtin scheme の variables を fresh 化し、constraints を `self.constraints` に追加し、`BuiltinInstance.types` に型変数の concrete/fresh 対応を保持する。
arity は必ず `BuiltinScheme.parameters.len()` から得る。
`polymorph.rs::captures` の `FunctionRef::Builtin(_) => 1` hardcode を削除する。
`closures.rs::lower_expression` の builtin wrapper 生成も一引数固定にしない。
multi-argument builtin の関数値化では curried wrapper stages を生成し、全 parameter が揃った段階でだけ builtin を呼ぶ。
一引数だけ partial application した二引数 builtin は、残り一引数を受け取る closure を返す。
`llvm.rs::FunctionEmitter::call` は `arguments[..count]` の slice 前に `count <= arguments.len()` を型で保証し、builtin の `count` は scheme arity を使う。
`FunctionRef` consumer の必須確認箇所:
- `polymorph.rs::captures`, `walk`, `expression_types`, `Specializer::lower`.
- `closures.rs::free_locals`, `lower_expression`.
- `llvm.rs::expression_mode`, `FunctionEmitter::call`.
- `ownership.rs` の function id 抽出と use/capture traversal。
- `recursion.rs::references`.
- `call_specialization.rs` の `target`, `transparent`, `non_escaping`, `read_only`。
`emit_builtin` は次へ変更する。
```rust
fn emit_builtin(instance: &BuiltinInstance, intrinsics: &mut BTreeSet<String>) -> String;
```
後続チケットが `Int.popcount` を追加する例:
```rust
pub enum Builtin {
    // existing...
    IntPopcount,
}

impl Builtin {
    pub fn name(self) -> &'static str {
        match self {
            Builtin::IntPopcount => "Int.popcount",
            // ...
        }
    }

    pub fn scheme(self) -> BuiltinScheme {
        match self {
            Builtin::IntPopcount => {
                BuiltinScheme {
                    parameters: vec![BuiltinType::Var("a")],
                    result: BuiltinType::Var("a"),
                    variables: vec!["a"],
                    constraints: vec![BuiltinConstraint { class: "Integer", ty: BuiltinType::Var("a") }],
                }
            }
            // ...
        }
    }
}
```
`emit_builtin(IntPopcount<i64>)` は `llvm.ctpop.i64` を `intrinsics` に登録し、`@tz.builtin.Int.popcount.i64` を生成する。
`Option` を返す builtin の例:
```rust
Builtin::IntCheckedAdd => BuiltinScheme {
    parameters: vec![BuiltinType::Var("a"), BuiltinType::Var("a")],
    result: BuiltinType::Std {
        module: "Option",
        name: "Option",
        args: vec![BuiltinType::Var("a")],
    },
    variables: vec!["a"],
    constraints: vec![BuiltinConstraint { class: "Integer", ty: BuiltinType::Var("a") }],
}
```
`UnsignedOf` の例:
```rust
Builtin::IntToUnsigned => BuiltinScheme {
    parameters: vec![BuiltinType::Var("a")],
    result: BuiltinType::UnsignedOf(Box::new(BuiltinType::Var("a"))),
    variables: vec!["a"],
    constraints: vec![BuiltinConstraint { class: "Integer", ty: BuiltinType::Var("a") }],
}
```
`WidenOf(i64)` は `i128`、`WidenOf(i64u)` は `i128u`。`WidenOf(i128)` と `WidenOf(i128u)` は `E1015`。
#### reachability API
`llvm.rs` に std pruning helper を追加する。
```rust
fn reachable_functions(module: &CheckedModule) -> BTreeSet<usize>;

fn emit_function(function_id: usize, reachable: &BTreeSet<usize>, module: &CheckedModule) -> bool;
```
`emit_function` は `module.functions[id].origin.module == ModuleOrigin::User || reachable.contains(&id)`。
`TypedExpr::children` を使って `FunctionRef::User(id)` と `Closure(id, ...)` を辿る。
`call_specialization` の追加 worker は、元になった std function が emitted である場合だけ出る。
## 設計
### std source の parse 統合
`analyze_modules_with_std` は user sources と std sources を一つの Vec に結合し、`analyze_inputs` へ一度だけ渡す。
path から module name と SourceKind を得る。
ユーザー source の path は従来互換のため `("Name", source)` も許す。
std source に拡張子なしは許さない。
ユーザー source と std source の module name が衝突したら、ユーザー側の Span で `E1011`。
ユーザー source が `stdlib::RESERVED_MODULES` のいずれかなら、std source が未実装でも `E1011`。
parse order は `SourceInput` の順序そのもの。
driver 経由では `Project.sources` が「user sources sorted → std sources」の順序なので、diagnostic `Span.source` と `Project::source_for` は同じ index を共有する。
### driver の SourceFile
`SourceFile` に origin と virtual path の概念を追加する。
```rust
#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub name: String,
    pub text: String,
    pub origin: ModuleOrigin,
}
```
std source の `path` は `PathBuf::from("std/Math.tz")`。
`Project::load` の最後に `stdlib::SOURCES` を `SourceFile { origin: Std }` として append する。
`Project::analyze` は `source.path` を文字列化して `analyze_inputs(&Project.sources)` 相当に渡す。
`Project::analyze` で `analyze_modules` や `analyze_modules_with_std` を呼んで std を再追加してはいけない。
`source_for` は std source index でも動く。
### module origin と名前解決
`Names` に module origin を追加する。
```rust
modules: BTreeMap<String, ModuleOrigin>
```
自モジュール判定は name だけでよい。
user unique と std unique を分けるため、record/type/union/class aliases は origin を保持する。
`ModuleOrigin::User` からの unqualified record/type/union/class 解決では、visible user 候補が一つあれば std 候補を見ない。
`ModuleOrigin::Std` からの unqualified 解決では user 候補を検索しない。
std 候補が複数なら `E1004`。
std private 候補だけなら `E1022`。
`Classes::find` は現在「current-module qualified」と builtin class name しか知らないため、そのままでは D-07 を満たさない。
`Names` と共有する origin-aware resolver、または `Classes` 専用 alias table を追加する。
class 解決順も record/type/union と同じく、requester が user なら self → unique visible user class → unique std class、requester が std なら self → unique std class。
同名 user class が複数 visible なら `E1004`、std class が複数なら `E1004`、private だけなら `E1022`。
組み込みクラス名は従来通り origin に関係なく最優先の reserved builtin class として扱う。
D-20/A11 後の `Eq` / `Ord` は借用引数を受けるため、class resolver と builtin constraint propagation は比較メソッドを値渡し二引数と仮定しない。
### builtin resolution
`Checker::name` は無修飾 builtin だけを扱う。
`Checker::value_expression` の module field branch は、source function が見つからなければ `Builtin::ALL` から full name 一致を探す。
この namespace-function 判定は `Classes::method` より前に行う。
既存の `module_function` precheck は source functions だけを見るため、visible source function OR full-name builtin の両方を見る helper に置換する。
first segment が local に存在する場合は従来通り field access 優先で、builtin/class/module 判定をしない。
`Task.run` / `Task.parallel` の専用分岐は削除するか、エラーメッセージだけの thin wrapper にする。
存在しない `Task.foo` は `E1002` で「Task has no function ...」を維持してよい。
### builtin specialization
`polymorph::Specializer::lower` または `expression_types` の concrete validation 後に、`BuiltinInstance.types` を concrete 型へ置換する。
`emit_target` に `Type::Variable` / `Type::Infer` を含む builtin instance が来たらコンパイラ bug なので `unreachable!` ではなく `debug_assert!` と `Diagnostic E1015` のどちらかを検討する。
実装の既定案は型検査側で `E1015` を出し、LLVM 側は `debug_assert!` に留める。
### std pruning
`reachable_functions` は出力関数集合を計算する唯一の場所にする。

通常 build の root は `origin.test.is_none()` の user-origin/export/entry roots。

G06 test runner は同じ `reachable_functions` に selected tests を root として渡す。

test runner 専用の別 emission-set 計算を作らない。

`reachable_functions` は BTreeSet で決定的に処理する。
worklist から function body を走査し、`FunctionRef::User(id)`、`GenericFunction` の lowering 後 id、`Closure(id, _)`、`TaskRun`/`TaskParallel` 内の callback 参照を拾う。
std function id が見つかれば reachable に追加し、worklist に積む。
`emit_target` の main loop は `emit_function(id, &reachable, module)` が true の関数だけ emit する。
closure wrappers も同じ条件にする。
export wrapper は std function には通常不要だが、std に `export def` を置くことは E02 では禁止する。
std source の `export def` は `E1022` または `E1008` ではなく `E1018` で拒否する。標準ライブラリがホスト ABI を増やさないため。
### runtime imports
`WebAssembly.Module.imports(module)` は既存同様 `[]`。
std unused pruning により、未使用 `Math.zero()` も runtime 連結条件を発火させない。
## 実装手順
1. `std/Math.tz` を追加する。
   - 確認: `target/release/tsuzuri check /tmp/...` を使う場合は scratch project から `Math.zero()` を参照し、std path の診断が出ないことを確認する。
2. `src/stdlib.rs` を追加し、`SOURCES`, `RESERVED_MODULES`, `is_reserved_module`, `module_name` を実装する。
   - 確認: Rust unit test で `module_name("std/Math.tz") == Some("Math")`、予約名判定が表と一致する。
3. `src/lib.rs` に `pub mod stdlib;`、`SourceInput`、内部 `analyze_inputs`、`analyze_modules_with_std` を追加する。
   - 確認: `analyze_modules_with_std(&[("Main", "42")], &[])` が std なし custom 経路として従来相当で通り、parse order は渡した順序と一致する。
4. `ModuleOrigin`、`ModuleInput`/`ModuleView` を `src/check.rs` に追加し、`check_modules`、`computation::collect`、`Classes::collect`、`Classes::instances`、active pattern 収集、entry 選択の signature を変更する。
   - 確認: origin を落とす `(&str, &Program)` tuple slice が残っていないことを `rg "\\&\\[\\(&str, &Program\\)\\]" src` で確認する。
5. module name 検査に `stdlib::is_reserved_module` を追加する。
   - 確認: `analyze_modules(&[("Option", "")])`、`("Int.tz", "...")` が `E1011`。
6. `Project::load` に std SourceFile append を追加し、`Project::analyze` を `analyze_inputs(Project.sources)` にする。
   - 確認: user source の sorted order と root index が既存テスト通りで、std source は root にならず、std が二重追加されない。
7. `Project::source_for` と `protect_sources` を std origin に対応させる。
   - 確認: std 内の意図的な型エラーを test hook で注入し、path が `std/Broken.tz` と表示される。出力保護は user source だけを拒否する。
8. `Names` と `Classes` に origin-aware alias resolver を追加し、record/union/type/class の requester-origin-sensitive 解決順を実装する。
   - 確認: user module `Local` の record/class が std `Local` より優先される一方、std source からは user `Local` を検索しない custom std テストを作る。
9. `BuiltinScheme`、`BuiltinConstraint`、`BuiltinInstance` を追加し、`FunctionRef` を `Clone` 化して全 consumer を reference match に直す。
   - 確認: 既存 builtin `sqrt`, `assert`, `clone_string`, `Task.run`, `Task.parallel` の型が従来テストで変わらない。
10. `Task.run` / `Task.parallel` を一般の qualified builtin 解決へ移行する。
    - 確認: `Task.run` を関数値として渡す既存/新規テストが通る。
11. `FunctionRef::Builtin` を `BuiltinInstance` 化し、polymorph substitution、`captures` arity、closure lowering の curried wrapper stages、LLVM call count を scheme arity に対応させる。
    - 確認: `Task.parallel` の多相利用、`clone_string`、full application、one-arg partial application、pipeline、constrained two-arg builtin の function value 渡しが concrete LLVM に到達する。
12. `llvm.rs` の builtin emission を instance-based にする。
    - 確認: `llvm::emit` の出力が決定的で、同じ builtin instance は一度だけ出る。
13. `closures::lower` 後の最終 functions に `FunctionOrigin` を伝播し、std/generated helper pruning を `emit_target` に入れる。
    - 確認: `analyze("42")` の library IR に `@tz.fn.Math.zero`、std record/union type、`@tz.builtin.*` wrapper が含まれない。`Math.zero()` を呼ぶと必要分だけ含まれる。
14. named record/union type instance collection を「emitted functions/wrappers/roots の signature/body scan」へ変更する。
    - 確認: custom std に未使用 record/union と function を置いても、未使用なら IR 型定義・関数定義が出ない。
15. 予約 module 互換対応を入れる。
    - 確認: `tests/currying.rs` の `curries_builtins_methods_module_functions_and_pipelines` にある user module `Math` を `UserMath` などへ rename する。`tests`/`examples` 全体を D-07 reserved table で grep し、他の module stem 衝突を修正する。
16. driver tests を origin-aware に更新する。
    - 確認: exact `Project.sources` list を比較する既存 test は `ModuleOrigin::User` で filter し、std が user sources の後に append されても `root` が変わらない test を追加する。
17. E2E に std import 空検査を追加する。
    - 確認: native/WASM × `-O0`/`-O3` で `Math.zero()` を使う fixture が動き、WASM imports は空。
18. docs/README を更新する。
    - 確認: 予約 std module 表と `check`/`--emit llvm` の LLVM 不要説明が最新。
実装時の検証は GUIDE §3 に従い、Rust は `cargo test --locked --test <file>` で対象 test file を指定する。
Node E2E の直前には必ず `cargo build --release --locked` を実行し、`node tests/*.mjs target/release/tsuzuri` は古い binary で走らせない。
## テスト計画
### Rust 受理テスト
`analyze("Math.zero()")` が通る。
`llvm::emit(&module, Entry::Library)` に未使用 std 関数が出ない。
`Math.zero()` を呼ぶと `@tz.fn.Math.zero` と private helper が必要に応じて出る。
default `analyze_modules` が std を自動追加する。
`analyze_modules_with_std(user, &[])` が std なし custom 経路として使える。
custom std `("std/Custom.tz", "def value :: i64\nfn value = 1")` を注入して `Custom.value` を呼べる。
user module と std module の同名 record 解決は user を優先する。
user module と std module の同名 class 解決は user を優先する。
std source 内の無修飾 record/class 解決は user 宣言を見ず、std 内の候補だけを見る。
std source から他 std module の型を使う fixture は、必ず `Other.Type` の完全修飾で書く。
std private helper を std public 関数から呼べる。
std private helper を user から呼ぶと `E1022`。
`Task.run` / `Task.parallel` が一般 builtin 解決でも従来通り動く。
無修飾 `sqrt`、`assert`、`clone_string` が従来通り動く。
Rust test で `Builtin::ALL` を走査し、std source に同じ qualified name の source `def` が存在しないことを検査する。
二引数以上の test builtin を追加する場合は、full application、one-arg partial application、pipeline、function value として高階関数へ渡す constrained builtin を検査する。
D-21 の `unreachable : unit -> 'a` は、`i64` と `string` など異なる result 型で別々の `@tz.builtin.unreachable.<mangled-T>` が出ることを検査する。
### Rust 拒否テスト
ユーザー source `Option.tz` は `E1011`。
ユーザー source `Debug.tc` も `E1011`。
std source path `std/Nested/Bad.tz` を `analyze_modules_with_std` に渡すと `E1011`。
std source path `Math.tz` のように `std/` prefix がない場合は `E1011`。
std source の `export def f :: i64` は `E1018`。
user source から std private function を呼ぶと `E1022`。
unknown `Int.missing` は `E1002`。
reserved module によって既存 user function `fn Int() ...` は関数名なので許す。ただし file/module stem `Int` は拒否する。
std source 内で user-only 型名を無修飾参照しても解決されず、std 候補がなければ `E1004`。
std class と user class が同名でも、std source からは std class だけが候補になる。
### IR 決定性テスト
同じ module を 2 回 `llvm::emit` して一致する。
builtin intrinsic 宣言が重複しない。
std pruning の結果、function order は既存 user function order を保つ。
std 関数が emit される場合は `stdlib::SOURCES` と reachability BTree order に従って決定的。
custom std に未使用 module を置いた program で、未使用 std function、未使用 std record/union type、`@tz.builtin.*` wrapper が一切 IR に出ない。
この test は B01 前に動くよう、custom std は record と通常関数だけで構成する。
`closures::lower` で生成される `$lambda`/`$task`/`$builtin` helper は owner の `origin.module`/`origin.test` を継承し、`origin.parent` を持ち、reachable typed references がない限り emit されない。
### E2E
fixture `tests/fixtures/stdlib/Main.tz`:
```text
export def answer :: i64
fn answer = (Math.zero() as i64) + 42
```
実際には `f64 as i64` の変換が入るため期待値は 42。
C host は `tz_answer() == 42` を確認する。
WASM host は `api.tz_answer() == 42n` または i64 BigInt を確認する。
`WebAssembly.Module.imports(module)` は `[]`。
native/WASM × `-O0`/`-O3`。
`--emit llvm` を 2 回実行し IR が一致する。
### 互換性テスト更新
`tests/currying.rs` の user module `Math` は reserved module になるため、`UserMath` へ rename して同じ currying/pipeline 期待を維持する。
driver tests の `Project.sources` exact list は user source だけを filter して比較し、別 test で std source が user source 後に append され root index が変わらないことを確認する。
`tests` と `examples` を D-07 reserved table の module stem で grep し、衝突があれば migration を同じ PR に含める。
## ドキュメント
`docs/language.md` の「1 ファイル = 1 モジュール」に、std module は予約名で常に利用可能と追記する。
`docs/language.md` の「組み込み関数」に、今後の std 修飾 builtin の方針を追記する。
`docs/language.md` の「診断」に、予約 std module の `E1011` を追記する。
`docs/architecture.md` のパイプライン図に `stdlib::SOURCES` の追加を入れる。
`docs/architecture.md` の不変条件に、std source は常に型検査し unused std function は IR から除くと書く。
`README.md` の「ファイルとモジュール」に予約 std module 表の簡略版を追加する。
`README.md` の CLI 節に、`check` と `--emit llvm` は std 同梱後も LLVM 不要と明記する。
後続チケットが std module を増やす場合は `_features/GUIDE.md` D-07 の表も同時更新する。
## 受け入れ条件
- [x] `std/` と `src/stdlib.rs` が追加され、default std が `include_str!` で埋め込まれる。
- [x] `analyze`、`analyze_modules`、`Project::load` のすべてで std が読み込まれる。
- [x] Rust tests から custom std を注入できる。
- [x] D-07 の予約 std module 名が user module として `E1011` で拒否される。
- [x] 無修飾 type/record/class 解決が user → std の順になる。
- [x] std origin からの無修飾 type/record/union/class 解決は user 宣言を検索しない。
- [x] 関数は修飾必須のまま。
- [x] qualified builtin が source function fallback として解決される。
- [x] std source の source `def` は builtin-backed API と同じ qualified name を持たないことを Rust test で保証する。
- [x] qualified builtin が class method 解決より前に namespace function として分類される。
- [x] `Task.run` / `Task.parallel` が一般 builtin namespace mechanism で動く。
- [x] builtin が複数引数・型変数・制約を表せる。
- [x] multi-argument builtin の partial application、pipeline、function value 化が closure lowering 後も正しい。
- [x] builtin IR emission が concrete specialization ごとに決定的に行われる。
- [x] `unreachable : unit -> 'a` を D-21 の symbol 形で実装できる。
- [x] unused std function/type/builtin wrapper が closure lowering 後の reachable set から pruning される。
- [x] `CheckedFunction` の origin は D-22 の `FunctionOrigin` 一つだけで、module 判定は `.origin.module` を使う。
- [x] generated helpers inherit `origin.module`/`origin.test` from parent and set `origin.parent`.
- [x] `reachable_functions` is the single emission-set computation; G06 changes only root selection.
- [x] user function の emission は従来通り維持される。
- [x] named record/union type definitions are collected only from emitted functions/wrappers/roots.
- [x] WASM default imports は空のまま。
- [x] `check` / `--emit llvm` に LLVM/Clang/LLD が不要。
- [x] docs と README が更新される。
## 落とし穴
std source を `Project.sources` に入れるだけでは、output protection が仮想 path を保護しようとして壊れる。
std source を user source より前に入れると、既存の `Span.source` 期待と diagnostics source id が変わる。
予約 module は std file が未追加でも拒否する必要がある。
`Builtin::signature` のままでは、複数引数・制約・concrete builtin name を扱えない。
`FunctionRef::Builtin(Builtin)` のままでは、LLVM codegen で polymorphic builtin の concrete 型が失われる。
std function pruning で user functions も削ると既存 IR/header テストが大きく変わる。
std private helper を型検査しないと、未使用 std の破損を検出できない。
std source の `export def` を許すと、ユーザーが知らない `tz_` ABI が増える。
`Task.run` 特別分岐を残したまま一般 builtin 分岐も追加すると、関数値としての解決や error message が二重化する。
WASM import を builtin で増やす設計は D-18 に反する。E06/E07 の opt-in まで禁止。
`FunctionRef::Builtin` を `BuiltinInstance` にすると `FunctionRef` は non-Copy になる。既存の by-value match を放置すると borrow/move エラーまたは clone 漏れが出る。
`polymorph.rs::captures` と `closures.rs::lower_expression` の builtin arity 1 前提を残すと、二引数 builtin の partial application と function value が壊れる。
class method 解決を namespace builtin 判定より先に走らせると、`Int.popcount` のような builtin を class method と誤診断する。
D-20 後の `Eq` / `Ord` を値渡し二引数として wrapper 生成すると、比較だけで非 Copy 値を消費する退行を起こす。
std pruning を closure lowering 前に行うと、`$lambda` / `$task` / `$builtin` helper と named-type instances の到達性がずれる。
`Project::analyze` が public `analyze_modules` を呼ぶと std を二重 append し、`Span.source` と `Project::source_for` がずれる。
## 対象外
本物の `Option` / `Result` 実装は B01。
本物の `Display` / `Parse` / `to_string` は D01。
本物の `Int.popcount` や rotate は D04。
本物の `Math.sin` などは D03。
配列 bulk API は C04。
階層 std module や `Std.*` namespace は E03 でも導入しない。
外部 package の std override は E04 でも既定では許さない。
WASM imports を伴う host/debug 機能は E06/E07。
stdlib の性能最適化は各 std API チケットで扱う。
## 未決事項
初期 `std/Math.tz` の placeholder API を最終リリースで残すかは未決。既定案は D03 が本物の Math API を入れる時に整理する。
std record/type の pruning は E02 で「emitted functions/wrappers/roots から参照される named type だけを出す」ところまで実装する。A02/B01 後の union/type alias も同じ scan に追加する。
builtin mangle 形式は `@tz.builtin.Name.i64` を既定案にする。引用符付き LLVM name は避ける。
custom std hook を public API として長期維持するかは未決。既定案は Rust tests 用として `#[doc(hidden)]` ではなく公開し、後続 ticket のテストで使う。
台帳の見直し提案はない。更新済み D-03、D-07、D-17、D-18、D-20、D-21 と整合する。

### 実装時の判断（E02）

- 上の未決事項は既定案どおり実装した。`std/Math.tz` は仮 API の `Math.zero : f64` と private の `identity_f64` だけを持ち、
  custom std の注入口 `analyze_modules_with_std` は公開 API にした。builtin の mangle は引用符なしの `@tz.builtin.name.<型>` 形式。
- `BuiltinType` に `Task` を追加した（`Task.run`／`Task.parallel` の scheme を表すため）。
- mangle は `Builtin::llvm_name` ではなく `llvm.rs` の `builtin_symbol`／`mangled_type` に置いた。`canonical_type` の `->`・`[`・`]`・`,` を
  `$A`・`$L`・`$R`・`$C` に置き換え、複数の型引数は `$C` で連結する。`$` は canonical type に現れないので単射になる。
- 組み込み関数は直接呼び出しも含めて、すべて scheme の引数個数を持つ一つの `$builtin` ラッパーを経由する（従来の設計を維持）。
  部分適用・関数値は通常の関数値の closure wrapper に任せ、引数段階ごとのラッパーは作らない。
  ラッパー名は型変数のない builtin では `name`、ある builtin では具体化ごとに `name.<id>`。
- 多引数・制約付き・型族の検査のため、`#[cfg(test)]` の builtin `Int.test_add`／`Int.test_unsigned`／`Int.test_widen` を追加した。
  テストビルドの `Builtin::ALL` にだけ含まれ、本番のコンパイラには存在しない。
- D-21 の `unreachable : unit -> 'a` は、テスト計画の symbol 検査のため B01 ではなく E02 で追加した。
- 型族 `UnsignedOf`／`WidenOf` は、呼び出しの引数の型検査後とパイプラインの単一化後に解き、未確定なら保留する。
  関数末尾（既定型の適用後）に残れば `E1015`。具体的な非整数型は、型族ではなく `Integer` 制約違反の `E1005` として報告する。
- 利用者由来の関数は到達不能でも従来どおりすべて出力し、std 由来の関数だけを到達可能性で間引く。
  root は利用者由来でテストでない関数・export・入口。型定義は利用者由来の型を常に出力し、std の型は出力する関数から参照される場合だけ出力する。
- 入れ子の lambda の `origin.parent` は top-level の持ち主の関数 ID。複数の関数が共有する `$builtin`／`$case`／`$intrinsic` は
  最初に要求した関数の origin を持ち、他の参照元からは到達可能性で出力される。
- `emit_builtin` は `BuiltinInstance` と具体的な callee 型を受け取る。
- std の関数が組み込み関数と同じ修飾名を持つ場合は、テストでの検出に加えて型検査でも `E1001` の重複として拒否する。
- D-07 に従い、無修飾のクラス名も自モジュール → 利用者のモジュールで一意 → std の順に解決するようにした。
  以前は他モジュールのクラスを無修飾では参照できなかった（`instance Score<Point>` が `E1016`）。
- 予約モジュール名は std の有無に関係なく `E1011`。利用者と std のモジュール名の衝突と不正な std のパスも `E1011` で、
  衝突は利用者のソース位置を指す。std の `export def` は `E1018` で、診断表の `E1018` の説明を拡張した。
- 生成関数の番号（`$lambda.N`、`$mono.N` など）は std の関数の数だけずれる。出力は決定的だが、std に関数を追加すると
  利用者の IR の補助関数名が変わる。補助関数を含まないプログラムの IR は std の有無で一致する（`tests/stdlib.rs`）。
  既存の fixtures と examples の IR は、番号を正規化すると E02 前と一致した。
- E2E は `tests/stdlib.mjs` ではなく `tests/features.mjs` の `stdlib` suite（`tests/fixtures/stdlib/`）に置き、`unreachable` のトラップも検査する。
  生成ヘッダー `stdlib.h` がシステムの `<stdlib.h>` を隠すため、harness の生成ヘッダー名を `tz-<name>.h` に変更した。
- 予約名との衝突の移行として、`tests/currying.rs` の `Math` は `UserMath` ではなく `Arith` に、
  `tests/union_types.rs` の `Option` は `Choice` に改名した。`tests`／`examples` にほかの衝突はない。
- 台帳の見直し提案はない。
