# A10: 高階型（HKT）

| 項目 | 内容 |
|---|---|
| ID | A10 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | A01, A06 |
| 後続 | 長期的な標準ライブラリ抽象化（Functor/Applicative/Monad/Traversable 等） |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/computation.rs`, `src/recursion.rs`, `src/ownership.rs`, `src/llvm.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/polymorphism.rs`, `tests/computations.rs`, `tests/fixtures/*` |

## 目的

`Option`, `Result<'e>`, `Array`, `List`, 将来の `Vec` など「型を受け取って型を返す型コンストラクター」を抽象化し、`Functor<'f>` のような型クラスを表現できるようにする。

ただし Tsuzuri は実行時辞書・boxing・動的ディスパッチを持たない方針であり、HKT も既存の単相化モデルに収まる範囲に限定する。P3 とする理由は、A01/A06/A02/B01/E02 が安定しないと設計の土台が動くうえ、コンパイル時間・特殊化爆発・診断複雑化のリスクが高いため。

## 現状

- GUIDE D-02 は型適用を `TypeExprKind::Apply(Ident, Vec<TypeExpr>)` として予定している。head は `Ident` であり、`'f<'a>` のような type-constructor variable を head にできない。
- `src/check.rs::Type` は runtime 型を表し、`Variable(String)` は kind `Type` の型変数だけを想定する。
- `src/polymorph.rs::variables`, `map_type`, `substitute`, `Inference::unify` は型変数を具体型へ置換するが、型コンストラクター変数や kind は持たない。
- `src/polymorph.rs::Classes` は型クラスが type parameter 1 個を持つ前提。A06 後も class parameter は `Type` kind の値型を想定する。
- `Specializer` は `Vec<Type>` の型引数で関数を単相化し、LLVM には `Type::Variable`/`Infer` を渡さない。

## 仕様

### 目標構文

フェーズ 1 では kind annotation を必須にし、推論の曖昧さを避ける。

```text
kind ::= "*"
       | kind "->" kind
       | "(" kind ")"

class-declaration ::=
    "class" class-name "<" class-parameter ","? ">" "{" ... "}"

class-parameter ::=
    type-variable (":" kind)?
```

例:

```text
class Functor<'f: * -> *> {
    def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b>
}

class Applicative<'f: * -> *> {
    def pure :: 'a -> 'f<'a>
    def apply :: 'f<'a -> 'b> -> 'f<'a> -> 'f<'b>
}

instance Functor<Option> {
    fn map f value =
        match value with
        | None -> None
        | Some x -> Some (f x)
}

instance Functor<Result<'e>> {
    fn map f value =
        match value with
        | Error e -> Error e
        | Ok x -> Ok (f x)
}
```

`'f<'a>` は type-constructor variable application。`Result<'e>` は部分適用された型コンストラクターで kind `* -> *`。

### kinds

- `*` は値として存在できる通常の型。`i64`, `string`, `Option<i64>`, `[i64]` など。
- `* -> *` は型を 1 個受け取って型を返す型コンストラクター。`Option`, `Result<string>`, `Array`, `List`。
- `* -> * -> *` は 2 引数型コンストラクター。`Result`。
- kind はコンパイル時だけに存在し、LLVM IR に出ない。

### 型適用

既存/A01:

```text
Option<i64>
Result<string, i64>
Pair<'a, 'b>
```

HKT:

```text
'f<'a>
'f<Option<'a>>
Result<'e>       // 部分適用。kind * -> *
```

型コンストラクターの部分適用は **型クラス head と型式内だけ**で使える。値の型として kind `*` でないものを要求したら `E1015`。

### 型クラス

- フェーズ 1 では型クラス parameter は 1 個のまま。ただし kind は `*` 以外も許す。
- method signature 内で class parameter を型コンストラクターとして適用できる。
- instance head は class parameter kind と一致する型コンストラクターでなければならない。
- `instance Functor<Result>` は kind `* -> * -> *` なので `Functor<'f: * -> *>` には `E1015`。`instance Functor<Result<string>>` は可。
- A06 の conditional instance と同様、overlap は kind-aware unification で検査する。

### 単相化

- 実行時辞書はない。
- `Functor.map` の呼び出しは、具体 `Option`, `Result<string>`, `[ ]` 等が分かった時点で instance method へ解決される。
- 型コンストラクター変数は LLVM まで残らない。`Type::ApplyVariable` などの抽象表現は `Specializer::instantiate` で必ず飽和 `Type` へ置換する。
- 特殊化上限は既存 1,024 を共有し、HKT 由来の追加展開も `E1017`。

## 設計

### kind データ構造

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Type,
    Arrow(Box<Kind>, Box<Kind>),
}
```

複数引数は右結合で表す。`* -> * -> *` は `Arrow(Type, Arrow(Type, Type))`。

### TypeExpr の見直し

GUIDE D-02 の `TypeExprKind::Apply(Ident, Vec<TypeExpr>)` は HKT には不足する。フェーズ 1 では互換を保ちながら次へ拡張する。

```rust
pub enum TypeExprKind {
    Named(String),
    Variable(String),
    Apply(Box<TypeExpr>, Vec<TypeExpr>),
    // ...
}
```

parser は A01 の `Apply(Ident, ...)` を `Apply(Box::new(Named(...)), ...)` として作る。`'f<'a>` は `Apply(Box::new(Variable("f")), [Variable("a")])`。

### Type の抽象表現

runtime に出る飽和型は A01/A02 の表現を使う。抽象 HKT のために型検査中だけ次を追加する。

```rust
pub enum Type {
    // 既存
    Variable(String),                 // kind *
    ConstructorVariable(String, Kind), // kind != * も可
    ApplyVariable(String, Vec<Type>),  // 'f<'a>。kind は checker が別表で保証
    Partial(TypeConstructor, Vec<Type>),
}

pub enum TypeConstructor {
    Record(usize),
    Union(usize),
    BuiltinArray,
    BuiltinList,
    Task,
}
```

ただし最終的に `require_concrete` は `ConstructorVariable`, `ApplyVariable`, `Partial` を LLVM 前に拒否する。`Partial` は kind `* -> ...` の値であり、method signature の中間表現にだけ残る。

代替案として `Type` に HKT 表現を入れず、`KindedType` wrapper を `polymorph.rs` 内だけに閉じる方法もある。フェーズ 1 は実装の単純さを優先して `Type` に追加する。

### kind 環境

```rust
struct KindEnv {
    type_variables: BTreeMap<String, Kind>,
    type_constructors: BTreeMap<String, Kind>,
}
```

登録:

- primitive: `i64 : *`, `string : *`, `char : *`
- array/list syntax: `[ ] : * -> *`, `[| |] : * -> *`
- `Task : * -> *`
- A01 record: `Pair : * -> * -> *` if `record Pair<'a, 'b>`
- A02 union: `Result : * -> * -> *`
- type alias: alias parameter count から kind を推定。HKT alias はフェーズ 1 対象外。

### kind checking

`resolve_type` の前に `kind_check_type_expr(expression, expected_kind)` を実行する。

規則:

- `Named("i64")` は kind `*`。
- `Named("Option")` は kind `* -> *`。expected `*` で未適用なら `E1015`。
- `Apply(f, arg)` は `kind(f) = k_arg -> k_result`、`kind(arg) = k_arg` を要求し、結果 kind は `k_result`。
- `Variable("'a")` は環境にあればその kind。なければ expected kind を割り当てる。フェーズ 1 では class kind annotation 外の constructor variable 推論はしないため、method signature の `'f` は class parameter として宣言済みでなければ `*`。
- 最終的な value type position は kind `*` を要求。

kind mismatch は `E1015`。kind 推論/展開上限は `E1017`。

### HKT instance 解決

A06 の `InstanceTemplate.head` を kind-aware にする。

例:

```text
instance Functor<Result<'e>>
```

head は class `Functor` の parameter kind `* -> *` を満たす `Partial(Union(Result), ['e])`。constraint `Functor<Result<string>>` と単一化すると `'e = string`。

overlap:

```text
instance Functor<Result<'e>>
instance Functor<Result<string>>   // E1016
```

`Option` と `Result<'e>` は単一化しない。

### 標準クラス候補

HKT が入っても、すぐに全標準ライブラリを Haskell 風にしない。候補:

```text
class Functor<'f: * -> *> {
    def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b>
}
```

`Applicative`/`Monad` は computation expression との関係が深く、P3 では設計だけに留める。Tsuzuri の `.tc` builder は HKT なしでも使えるため、HKT は標準 API の重複削減が明確になってから導入する。

## 実装手順

### フェーズ 1（完全指定）

1. **Kind AST と parser**
   - `Kind` を `syntax.rs` または `check.rs` に追加。
   - `class Functor<'f: * -> *>` の class parameter parser を追加。
   - `TypeExprKind::Apply(Box<TypeExpr>, Vec<TypeExpr>)` へ移行。A01 の構文を壊さない。
   - 確認: parse tests for `class Functor<'f: * -> *>`.
2. **Kind 環境と kind checker**
   - primitive/record/union/builtin type constructors を `KindEnv` に登録。
   - `resolve_type` 前に `kind_check_type_expr` を通す。
   - `Option` を value type として未適用で使うと `E1015`。
   - 確認: `def bad :: Option -> i64` が `E1015`。
3. **Type 抽象表現**
   - `ConstructorVariable`, `ApplyVariable`, `Partial` を追加。
   - `map_type`, `substitute`, `bounded_type`, `variables`, `Inference::resolve`, `Inference::unify` を更新。
   - kind が違う unification を `E1015`。
   - 確認: `def idf :: 'f<'a> -> 'f<'a>` は `'f` 未宣言なら拒否。
4. **Classes 統合**
   - `Class.variable` に kind を持たせる。
   - A06 の instance template head を kind-aware にする。
   - `instance Functor<Option>`、`instance Functor<Result<'e>>` を受理。
   - overlap を検査。
5. **Specializer**
   - `substitute` で constructor variable を concrete constructor/partial application に置換。
   - `ApplyVariable` が飽和したら `Type::Union/Record/Array/List/Task` へ normalize。
   - LLVM 前 `require_concrete` で HKT 中間型が残れば `E1015`。
6. **最小標準実験**
   - std ではなくテスト内で `Functor` class と `Option`/`Result` instances を定義。
   - `Functor.map` が concrete method call に下がること、辞書引数がないことを IR で確認。

### フェーズ 2（設計）

- kind annotation の省略推論を限定的に導入。`class Functor<'f>` の method 使用から `* -> *` を推定できるようにするか検討。
- type alias の HKT parameter を許可:
  ```text
  type Compose<'f, 'g, 'a> = 'f<'g<'a>>
  ```
- std の `Functor` を導入するか go/no-go で判断。

### フェーズ 3（設計）

- `Applicative`, `Monad`, `Traversable` を標準ライブラリへ入れるか検討。
- computation expression との重複、builder lowering、特殊化爆発を測定。

## テスト計画

### フェーズ 1 受理

```text
union Option<'a> = None | Some of 'a

class Functor<'f: * -> *> {
    def map :: ('a -> 'b) -> 'f<'a> -> 'f<'b>
}

instance Functor<Option> {
    fn map f value =
        match value with
        | None -> None
        | Some x -> Some (f x)
}

def inc_option :: Option<i64> -> Option<i64>
fn inc_option value = Functor.map (x -> x + 1) value
```

```text
union Result<'a, 'e> = Ok of 'a | Error of 'e

instance Functor<Result<'e>> {
    fn map f value =
        match value with
        | Ok x -> Ok (f x)
        | Error e -> Error e
}
```

### 拒否

| プログラム | 期待 |
|---|---|
| `def bad :: Option -> i64` | `E1015` |
| `instance Functor<Result> { ... }` | `E1015` |
| `class Functor<'f: * -> *> { def bad :: 'f -> i64 }` | `E1015` |
| `instance Functor<Result<'e>>` と `instance Functor<Result<string>>` | `E1016` |
| `def f :: 'f<'a> -> 'f<'a>` で `'f` 未宣言 | `E1015` |
| 型コンストラクター再帰で kind/型展開上限超過 | `E1017` |

### E2E

- HKT 自体は runtime 表現を持たないため、E2E は `Functor.map` を通じた `Option`/`Result`/array wrapper の exported scalar checksum。
- IR に dictionary parameter や `%tz.hkt` 風の型がないこと。
- native/WASM × `-O0`/`-O3`、IR 決定性、heap `live == 0`。

## ドキュメント

- `docs/language.md`
  - kind, `*`, kind annotation, constructor variable application。
  - HKT は rank-1 type constructor polymorphism であり、higher-rank types ではないこと。
  - 診断 `E1015`/`E1017` の例。
- `docs/architecture.md`
  - kind checking が `resolve_type` の前提であること。
  - HKT 中間型は LLVM 前に消える不変条件。
  - 特殊化上限と辞書なし方針。
- `README.md`
  - 実装済みになった段階で P3 機能として短く記載。実装前に標準 API を HKT 前提に書かない。

## 受け入れ条件

- [ ] kind annotation 付き class parameter を parse できる。
- [ ] `TypeExprKind::Apply` が variable head を扱える。
- [ ] kind checker が未適用/過適用/mismatch を `E1015` で拒否する。
- [ ] `instance Functor<Option>` と `instance Functor<Result<'e>>` が動作する。
- [ ] overlap が kind-aware に `E1016`。
- [ ] 単相化後、HKT 中間型が `CheckedModule` の LLVM 入力に残らない。
- [ ] 辞書/boxing/ランタイム型情報を生成しない。
- [ ] native/WASM × `-O0`/`-O3`、IR 決定性、heap `live == 0`。

## 落とし穴

- D-02 の `Apply(Ident, Vec<TypeExpr>)` のままだと `'f<'a>` を表せない。A10 では AST migration が必要。
- kind と Type を混ぜると、`Option` を値型として扱って LLVM へ流すバグになる。
- partial application を concrete value type として許すと layout/ownership が定義できない。
- HKT instance 解決で constructor variable を通常 type variable と同じに扱うと overlap を見逃す。
- `Functor.map` を runtime dictionary にすると Tsuzuri の単相化方針と性能目標に反する。

## 対象外

- higher-rank polymorphism。
- dependent types、type-level computation 全般。
- associated type/type family。
- kind polymorphism (`forall k`)。
- 標準ライブラリ全体の HKT 化。導入は go/no-go 後。

## 未決事項

- **台帳の見直し提案:** GUIDE D-02 の `TypeExprKind::Apply(Ident, Vec<TypeExpr>)` は HKT に不足する。A10 実装時は `Apply(Box<TypeExpr>, Vec<TypeExpr>)` へ改訂する必要がある。既定案は A01 の `Apply(Ident, ...)` を parser 互換で `Apply(Named(...), ...)` に移行する。
- **既定案: フェーズ 1 は kind annotation 必須。** 推論は後続で検討。
- **既定案: HKT std 導入の go/no-go 基準**
  - go: `Functor`/`Result<'e>`/`Option` の prototype が辞書なし IR、特殊化増加 20% 未満、診断が理解可能。
  - no-go: 単純な `.tc` builder と比べて標準 API が複雑化する、または特殊化/compile time が制御不能。
  - no-go の場合も kind checker 実装を revert し、標準ライブラリは concrete module functions (`Option.map`, `Result.map`) を維持する。
