# A01: 型適用とジェネリックなレコード

| 項目 | 内容 |
|---|---|
| ID | A01 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | – |
| 後続 | A02, A05, A06, A10, C05, B01 |
| 状態 | done |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/call_specialization.rs`, `src/computation.rs`, `src/recursion.rs`, `src/closures.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/*.rs`, `tests/*.mjs` |

## 目的

Tsuzuri で `Pair<'a, 'b>`、`Box<'a>`、`Point<'unit>` のような名前付きレコードを型引数付きで定義・利用できるようにする。
これは A02 の `union Option<'a>`、B01 の `Option`／`Result` 標準型、C05 のレコード更新、A06 の汎用インスタンス拡張の基礎である。

このチケットでは **型適用構文** と **ジェネリックなレコード** だけを実装する。
union、型別名、高階型、条件付きインスタンス、標準ライブラリ同梱は別チケットに残す。

## 導入前の調査メモ

- 調査時点は `_features/GUIDE.md` にある `19d8cdd`。
- この節の構文例は当時の旧表記を記録する。以降の仕様・使用例は現在の `<...>` 表記に従う。
- `src/syntax.rs` の `TypeExprKind` は `Named(String)`、`Variable(String)`、`Constrained(Box<Ident>, Box<TypeExpr>)`、`Task(Box<TypeExpr>)` などを持つが、一般の型適用を表せない。
- `src/parser.rs` の `Parser::type_atom` は `Task T` だけを特別扱いし、`Name 'a` は `TypeExprKind::Constrained` として構文解析する。
  そのため `record Pair 'a 'b { ... }` は当時のバイナリで `E0002 expected '{' after the record name` になる。
- `src/syntax.rs` の `RecordDecl` は `name` と `fields` だけを持ち、型パラメーターを持たない。
- `src/check.rs` の `Type` は `Record(usize)` だけを持つ。
  `usize` は `CheckedModule.records` の宣言 ID であり、型引数は保持されない。
- `src/check.rs` の `CheckedRecord` は `name`, `fields`, `span` だけを持つ。
  フィールド型は宣言時に `resolve_type` で解決済みの単相型である。
- `src/check.rs` の `Names::record` は `Module.Record`、完全修飾名、またはプロジェクト内で一意な別名を解決する。
  型クラス名との衝突は現状では型適用の文脈がないため表面化していない。
- `src/check.rs` の `resolve_type` は `TypeExprKind::Named(name)` を `numeric::primitive(name)` か `Type::Record(id)` に解決し、`TypeExprKind::Constrained(_, ty)` は中身の型だけに落とす。
- `src/polymorph.rs` の `Classes::inline_constraints` は `TypeExprKind::Constrained(class, ty)` を型クラス制約として回収する。
  `polymorph::type_expression` はインスタンスメソッド合成時に `Type` を `TypeExpr` へ戻すが、`Type::Record(id)` は `ty.display(records)` に依存して `Named` に戻る。
- レイアウトは `src/check.rs` の `record_size`, `layout_size`, `validate_size` が宣言単位の `record_sizes: Vec<usize>` で計算する。
  `record A { a: [A] }` や相互再帰は `E1010` で拒否される。
- レコード式は `src/check.rs` の `Checker::value_expression` の `ExprKind::Record` 分岐で、宣言フィールド型そのものを期待型にして各フィールドを検査する。
- レコードフィールドアクセスは同じく `Checker::value_expression` の `ExprKind::Field` 分岐で、`Type::Record(id)` の宣言フィールド型を返す。
- レコードパターンは `src/control.rs` の `Checker::pattern_alternatives` の `PatternKind::Record` 分岐で、`Type::Record(id)` と宣言フィールド型を使う。
- 単相化・所有権・LLVM の `Type::Record` 参照箇所は、少なくとも以下に存在する。
  - `src/check.rs`: `Type::display`, `is_copy`, `needs_drop`, `carries_loans`, `can_capture`, `can_send`, `resolve_type`, `layout_size`, `validate_size`, `Checker::value_expression`, `TypedExprKind::Record`
  - `src/polymorph.rs`: `map_type`, `substitute`, `bounded_type`, `Inference::resolve`, `Inference::unify`, `type_expression`, `Classes::intrinsic`, `Classes::validate`, `Specializer::instantiate`
  - `src/control.rs`: `Checker::pattern_alternatives`
  - `src/ownership.rs`: `Checker::is_copy`, `require_copy`, `closed_returns::owned`, `Checker::eval`
  - `src/call_specialization.rs`: `may_mutate::mutable_reference`
  - `src/llvm.rs`: `emit_target`, `llvm_type`, `drop_value`, `clone_value`, `TypedExprKind::Record`, `TypedExprKind::Field`, `place`
  - `src/llvm_frame.rs`: `stack_size`, `frame_inner`, `field_types`, `relocate`
  - `src/computation.rs`: `expand_pattern`
- `src/llvm.rs` の `emit_target` は全レコード宣言を IR 冒頭へ `%tz.record.<Module.Name> = type { ... }` として出力する。
  ジェネリックな具体インスタンス集合はまだ存在しない。
- `Type::exportable` はスカラーだけを許すため、レコードは現在も export 不可である。

## 仕様

### 構文

台帳 D-02 と D-04 に従う。

```text
TypeExpr        ::= FunctionType
FunctionType    ::= ProductType ("->" ProductType)+ | ProductType
ProductType     ::= TypePrimary ("*" TypePrimary)*
TypePrimary     ::= TypeVariable
                  | QualifiedTypeName TypeArguments?
                  | "Task" TypeArguments
                  | "[" TypeExpr "]"
                  | "[|" TypeExpr "|]"
                  | "&" "mut"? TypePrimary
                  | "(" TypeExpr ")"
                  | "fn" "(" (TypeExpr ("," TypeExpr)*)? ")" "->" TypeExpr
TypeArguments   ::= "<" TypeExpr ("," TypeExpr)* ","? ">"
TypeParameters  ::= "<" TypeVariable ("," TypeVariable)* ","? ">"
RecordDecl      ::= "record" TypeName TypeParameters? "{" FieldDecl* "}"
FieldDecl       ::= FieldName ":" TypeExpr ","?
QualifiedTypeName ::= Ident ("." Ident)?
```

- 型適用は **`<...>` 内のカンマ区切り**に統一する。
  `Pair<i64, string>`、`Task<Pair<i64, string>>`、`[Option<i64>]` は有効。
  旧来の空白区切り、`Pair[i64]`、後置型引数は受理しない。
- `Task<T>` も同じ型引数リストを使い、引数は一つだけとする。構文木上では従来どおり `TypeExprKind::Task`。
  一般の `Task` レコードやモジュール名は既存仕様どおり予約済み。
- `Add<'a>` は構文解析時点では `TypeExprKind::Apply(Ident("Add"), [Variable("a")])` にする。
  その後 `Classes::inline_constraints` と `resolve_type` で「型クラス制約」か「型適用」かを分類する。
- `Parser::type_primary` は、名前の直後の `<...>` による型適用を含めて一つの型を読む。
  `Fn` は次トークンが `LeftParen` の場合だけ関数型の開始として扱う。それ以外の `fn` は型名ではないので既存の構文エラーにする。
- `<...>` 内の各引数は完全な `type_expr` として読み、カンマと `>` で区切る。
  `Option<Pair<i64, string>>` や `Box<i64 -> i64>` に追加の丸括弧は不要。
  `type_product` は `type_primary` を呼ぶ。型引数内の改行は区切りにならない。
  型の文脈でだけ `>>`・`>>>`・`>=` から閉じ括弧を分割し、式の比較・シフトを維持する。
  名前と `<` は隣接させ、空のリストは拒否、末尾のカンマは許す。
- `Parser::parameters` は現行コードで comma 区切りだけを受け付ける。A01 では record field grammar もこの既存 helper に合わせ、`;` 区切りは導入しない。
- `record Pair<'a, 'b> { first: 'a, second: 'b }` の型パラメーターはレコード名直後の `<...>` にカンマ区切りで並べる。
  同じパラメーター名の重複、`'_`、フィールドで未宣言の型変数を使うこと、宣言したが使わない型パラメーターは `E1024`。
- 非ジェネリックな既存レコード構文 `record Point { x: f64, y: f64 }` はそのまま有効。

### 型規則

- `Type::Record(id, args)` を導入する。
  `id` は宣言 ID、`args` は宣言の型パラメーターと同じ個数の型引数である。
- `Point` は `Type::Record(point_id, vec![])`。
  `Pair<i64, string>` は `Type::Record(pair_id, vec![Type::I64, Type::String])`。
- `Pair` のような型コンストラクターだけの型は値の型として使えない。
  必要な型引数が不足していれば `E1004`、多すぎても `E1004`。
  メッセージは `"type 'Pair' expects 2 type arguments, found 1"` の形にする。
- `Type::display(records)` は `Pair<i64, string>` のように表示する。
  関数型・タプル型が型引数に入る場合は既存表示規則と同じく必要な括弧を付ける。
- `resolve_type` は `TypeExprKind::Apply(name, args)` を次の順に分類する。
  1. `name` が型クラスだけに解決でき、`args.len() == 1` なら `TypeExprKind::Constrained` 相当として制約回収対象にする。
  2. `name` がレコード型だけに解決できるなら `Type::Record(id, resolved_args)` にする。
  3. 型クラス名と型名の両方に解決できる場合は `E1004`。
  4. どちらにも解決できなければ既存の unknown record/type 診断を `E1004` で返す。
- 型クラス名とレコード名の衝突は宣言時にも `E1001` で拒否する。
  `Classes::collect` がユーザー定義クラスを追加するとき、`Names.records` と同じ完全修飾名または同一モジュールの単純名を検出する。
- レコード宣言のフィールド型は宣言時には型変数を含んでよい。
  ただしフィールドに現れる型変数は `CheckedRecord.parameters` に含まれていなければ `E1024`。
- レコードリテラル `Pair { first: 1, second: "x" }` は型引数を推論する。
  期待型が `Pair<i64, string>` ならその型引数を使う。
  期待型がない場合は各パラメーターに新しい `Infer` を割り当て、フィールド式との単一化で決める。
- レコードフィールドアクセス `pair.first` の型は、宣言フィールド型の型パラメーターを `pair` の実引数で置換した型。
- レコードパターン `Pair { first: x }` と `{ first = x }` も同じ置換を使う。
- `Inference::unify` は `Type::Record(a_id, a_args)` と `Type::Record(b_id, b_args)` の `id` が等しく、かつ引数個数が等しい場合に各引数を単一化する。
  `id` が異なる場合は既存と同じ `E1003 expected ..., found ...`。
- `variables`, `bounded_type`, `map_type`, `substitute`, `Inference::resolve` は `Record(_, args)` の中を再帰的に処理する。
- `Type::Record(_, args)` の型引数は単相化後にはすべて具体型でなければならない。
  `Specializer::instantiate` の `expression_types` で `require_concrete` と `validate_size` を再適用する。

### 所有権・借用

- レコードの所有権規則は構造的なまま。
  `Pair<string, i64>` は `first` が move される非 Copy レコード、`Pair<i64, bool>` は Copy レコード。
- `Type::is_copy`, `needs_drop`, `carries_loans`, `can_capture`, `can_send` は、置換済みフィールド型に対して判定する。
  宣言フィールド型をそのまま使ってはならない。
- `ownership::Checker::is_copy` と `require_copy` は `Type::Record(id, args)` を展開し、型変数を含むフィールドに必要な `Copy` 制約を伝播する。
  例: `def first_twice :: Pair<'a, 'b> -> ('a * 'a)` で `first` を二度使うなら `Copy<'a>` が推論される。
- `closed_returns::owned` は置換済みフィールド型を見る。
- フィールド単位の move、部分 move 後の残りフィールド利用、借用競合は既存の `Field` place 表現を維持する。
- 可変参照を含むレコードフィールドは既存どおり `E1013` で拒否する。
  ジェネリック実体化後にも `validate_size` で配列・リスト要素に `&mut` が隠れていないことを再検査する。

### 評価順序・実行時意味

- 型引数はコンパイル時情報であり、評価順序・短絡・トラップの順序を変えない。
- レコードリテラルのフィールド式は既存どおりソースに書かれた順に評価する。
  フィールドの宣言順に並べ替えるのは型付き IR の `TypedExprKind::Record(Vec<(usize, TypedExpr)>)` 内だけで、式評価順序は保つ。
- 型推論や単相化のためにレコード式・フィールドアクセス・パターンの対象式を複数回評価してはならない。

### LLVM 表現

- 非ジェネリックレコードの LLVM 表現は既存互換でよい。
  `%tz.record.Main.Point = type { double, double }` の形を維持できる。
- ジェネリックレコードは **具体インスタンスごと**に LLVM named type を定義する。
  例:

```llvm
%"tz.record.Main.Pair[i64,string]" = type { i64, %tz.string }
%"tz.record.Main.Pair[i64,i64]" = type { i64, i64 }
```

- 型名は台帳 D-03 に従い、引用符付き識別子と Tsuzuri canonical type text を使う。
  型引数部分に `llvm_type(arg)` を入れてはならない。
  `%`、空白、`{}` を含む LLVM 型文字列を mangle に使うと、入れ子の named type で無効な IR になる。
- `emit_target` は全関数シグネチャ・本体・クロージャ環境・フレーム候補から `Type::Record(id, args)` を収集し、`BTreeSet<Type>` でソートしてから型定義を出力する。
  宣言順や HashMap 順に依存させない。
- `llvm_type(Type::Record(id, args))` は同じマングリング関数を使う。
- `drop_value`, `clone_value`, `llvm_frame::stack_size`, `llvm_frame::field_types`, `llvm_frame::relocate` は置換済みフィールド型を使う。
- 公開 ABI は変更しない。
  `Type::exportable` は `Record(_, _)` に対して引き続き `false`。

### 診断

- 型宣言自体の不正は D-16 の `E1024` を使う。
  - 未使用の型パラメーター
  - 未宣言の型パラメーター
  - 型パラメーター重複
  - レコード宣言中で型引数個数が明らかに不正な自己参照
- 名前解決・型適用の利用側の不正は既存コードを優先する。
  - unknown type: `E1004`
  - ambiguous type/class name: `E1004`
  - wrong number of type arguments: `E1004`
  - 型不一致: `E1003`
  - レイアウト超過・再帰値レイアウト: `E1010`
- メッセージは英語、小文字始まり、修正方法を含める。
  例: `"type parameter 'a is not used by any field; remove it or add a field that mentions it"`。

## 設計

### データ構造

`src/syntax.rs`:

```rust
pub struct RecordDecl {
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub fields: Vec<Parameter>,
}

pub enum TypeExprKind {
    Named(String),
    Variable(String),
    Apply(Ident, Vec<TypeExpr>),
    Constrained(Box<Ident>, Box<TypeExpr>), // 互換のため段階的に残してよい
    Array(Box<TypeExpr>),
    List(Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Task(Box<TypeExpr>),
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    Reference(Box<TypeExpr>, bool),
}
```

`Constrained` は最終的に使わなくしてよいが、段階 1 では既存テストを壊さないため残してもよい。
新しい構文は必ず `Apply` を生成する。

`src/check.rs`:

```rust
pub enum Type {
    Variable(String),
    Infer(usize),
    Integer(u16, bool),
    Binary(u16),
    Decimal(u16),
    Bool,
    Unit,
    String,
    Record(usize, Vec<Type>),
    Array(Box<Type>),
    List(Box<Type>),
    Tuple(Vec<Type>),
    Task(Box<Type>),
    Function(Vec<Type>, Box<Type>),
    Reference(Box<Type>, bool),
}

pub struct CheckedRecord {
    pub name: String,
    pub parameters: Vec<String>,
    pub fields: Vec<(String, Type)>,
    pub span: Span,
}

pub struct TypeContext<'a> {
    pub records: &'a [CheckedRecord],
}
```

A01 は `TypeContext` を records だけで導入する。A02 は同じ struct に `unions: &'a [CheckedUnion]` を追加して拡張する。
`Type::display`、`is_copy`、`needs_drop`、`contains_reference`、`carries_loans`、`can_capture`、`can_send`、layout、class validation、ownership、LLVM helper は、ばらばらに `records` slice を渡すのではなく `&TypeContext` を受け取るよう段階的に移行する。

具体的な新シグネチャ:

```rust
impl Type {
    pub fn display(&self, types: &TypeContext<'_>) -> String;
    pub fn is_copy(&self, types: &TypeContext<'_>) -> bool;
    pub fn needs_drop(&self, types: &TypeContext<'_>) -> bool;
    pub(crate) fn carries_loans(&self, types: &TypeContext<'_>) -> bool;
    pub(crate) fn can_capture(&self, types: &TypeContext<'_>) -> bool;
    pub(crate) fn can_send(&self, types: &TypeContext<'_>) -> bool;
}

fn layout_size(
    ty: &Type,
    types: &TypeContext<'_>,
    cache: &mut BTreeMap<Type, usize>,
    visiting: &mut BTreeSet<Type>,
    depth: usize,
    span: Span,
) -> Result<usize, Diagnostic>;

fn validate_size(
    ty: &Type,
    types: &TypeContext<'_>,
    cache: &mut BTreeMap<Type, usize>,
    span: Span,
) -> Result<usize, Diagnostic>;
```

共有ヘルパーを `src/check.rs` に追加する。A02 も union で同じ置換を使うため、`pub(crate)` にする。

```rust
pub(crate) fn type_parameter_substitutions(
    parameters: &[String],
    args: &[Type],
    span: Span,
) -> BTreeMap<String, Type>;

pub(crate) fn substitute_type_parameters(
    ty: &Type,
    substitutions: &BTreeMap<String, Type>,
) -> Type;

pub(crate) fn record_field_types(
    types: &TypeContext<'_>,
    id: usize,
    args: &[Type],
    span: Span,
) -> Vec<(String, Type)>;

pub(crate) fn record_field_type(
    types: &TypeContext<'_>,
    id: usize,
    args: &[Type],
    field: usize,
    span: Span,
) -> Type;

pub(crate) fn validate_record_arity(
    record: &CheckedRecord,
    args: &[Type],
    span: Span,
) -> Result<(), Diagnostic>;
```

型コンストラクターの arity は `resolve_type`、record literal の期待型処理、instance head、annotation/signature など **構築境界**で必ず `validate_record_arity` により検査する。
その後の `record_field_types` / `record_field_type` は arity 検査済み前提の非 fallible helper にする。

### パーサ

- `Parser::program` の `record` 分岐は、任意の `<...>` で型変数のカンマ区切りリストを読む。
  型変数以外・空のリスト・区切りの欠落は `E0002`。
- `Parser::type_primary` は名前付きの型適用を読み、`Task` には一つの型引数を要求する。
  `Task<Pair<i64, string>>`・`Task<i64>` を同じリスト構文で読む。
  `Fn` は `LeftParen` が続くときだけ関数型として読む。
- `angle_list` は宣言の型変数リストと使用側の型引数リストで共有する。
  型引数には `type_expr` を再帰的に適用する。
  例: `Pair<i64, string>` は `Apply(Pair, [Named(i64), Named(string)])`、`Pair<Box<i64>, string>` は `Apply(Pair, [Apply(Box,[i64]), string])`。
- `Parser::type_product` は `Pair<i64, string> * i64` を `(Pair<i64, string>) * i64` と解釈する。
  つまり名前付きの型適用が `*` より強い。
- `Parser::signature` は現行コードどおり `constraint_prefix()` を使って `=>` 付き制約 prefix を先に解析し、`ConstraintExpr { class, ty }` として保持する。
  この明示 prefix 経路は `TypeExprKind::Apply` 化せず、`Add<'a> =>` の `<...>` を共有 helper で読む。
- `instance Add<Pair<i64, i64>> { ... }` は class 名の後に一つの型引数を要求する。
  `instance Add<(Pair<i64, i64>)> { ... }` の冗長な丸括弧も同じ型になる。

### 名前解決・宣言チェック

- `check_modules` の名前収集を、型名・クラス名の衝突を解決できるよう分割する。
  1. `collect_class_names`: 組み込みクラス名、ユーザー定義クラス名、record 名を先に収集し、`E1001` で衝突を拒否する。
  2. record field 型を解決し、record の layout / generic field rule を検査する。
  3. `check_class_declarations`: class method signatures を解決・検証する。
  4. `Classes::instances` を検証する。
  A02 は同じ phase split に union 型名・case 名を追加する。
- `check_modules` のレコード名収集時に、`record.parameters` をまだ解決せず `CheckedRecord.parameters` に保存する。
- レコード宣言チェック:
  - パラメーター名重複は `E1024`。
  - `'_` は `E1024`。
  - フィールド型に現れる `Type::Variable(name)` が `parameters` に含まれなければ `E1024`。
  - 各 `parameters` は少なくとも 1 回フィールド型に出現しなければ `E1024`。
  - フィールド型は `polymorph::require_concrete` してはならない。
    型変数を含めるためである。
    代わりに `polymorph::bounded_type` と宣言内の型変数検査を実行する。
- 関数シグネチャやローカル型注釈では、従来どおりその関数シグネチャに現れない型変数を `E1015` で拒否する。
  `Pair<'a, i64>` の `'a` は関数シグネチャの型変数として扱われる。
- `Classes::collect` はユーザー定義クラス名とレコード型名の衝突を `E1001` で拒否する。
  組み込みクラス名 `Add`, `Copy` などと同名のレコードも `duplicate()` で拒否する。
- `Names::record` は引き続きレコード宣言 ID だけを返す。
  アリティ検査は `resolve_type` が `CheckedRecord.parameters.len()` と `Apply.args.len()` を比較して行う。
- E02 の std 同梱後は、record/type/class 解決は GUIDE D-07 に従う。
  利用者モジュールからは「自モジュール → 利用者モジュールで一意 → std」、std モジュールからは「自モジュール → std」のみを探し、利用者宣言を参照しない。
  A01 の `Names::record` / class name collection はこの分岐を追加できるよう、source module が std かどうかを引数または `Names` 内の module metadata で判断する設計にする。

### 型検査

- `Checker::value_expression` の `ExprKind::Record` 分岐:
  1. レコード ID を解決する。
  2. 期待型が `Type::Record(id, expected_args)` ならその `expected_args` を使う。
  3. 期待型が別レコードなら `same` に任せて `E1003`。
  4. 期待型がない場合、`CheckedRecord.parameters` ごとに `Inference::fresh()` を作る。
  5. `record_field_types` で置換済みフィールド型を得る。
  6. 各フィールド式を置換済み型で検査する。
  7. 最終型は `Type::Record(id, resolved_args)`。
- フィールド式の評価順序は入力順のまま。
  `TypedExprKind::Record(Vec<(usize, TypedExpr)>)` に詰める順序を宣言順にしたい場合でも、値を作る前に式を並べ替えない。
  既存実装は入力順に `values.push((index, expression(...)))` しているため、この形を維持する。
- `ExprKind::Field` の `Type::Record(id, args)` 分岐は `record_field_type` を使う。
- `control.rs` の `PatternKind::Record`:
  - 名前付きパターンは `Type::Record(id, fresh_args)` を作って対象型と単一化する。
  - 無名 `{ field = p }` は対象型が `Type::Record(id, args)` に解決済みの場合だけ許す。
  - フィールド投影の型は置換済み型。
- `Type::Record` が入れ子になっても `validate_size` は具体化後に見る。
  宣言時の `validate_size` は型変数を含むため、再帰検出だけを別に行う。
- A01 はライフタイム機能ではないため、型引数置換後の field 型が共有参照または排他参照を含む場合は `E1013`。
  `&T` も `&mut T` も拒否する。入れ子 record、array/list、tuple、function/task result を経由する参照も検出する。
  検査箇所:
  - record literal の型引数推論後
  - annotation / function signature / instance head の concrete type validation
  - `Specializer::instantiate` の全 concrete type 再検査
  - record field access / pattern で置換済み field type を得る境界

### 多相・単相化

- `polymorph::map_type`, `substitute`, `bounded_type`, `Inference::resolve`, `Inference::unify`, `variables` は `Record(_, args)` の `args` を必ず走査する。
- `Classes::intrinsic` の `Copy`, `Capture`, `Send` は置換済みフィールド型に基づく `Type` メソッドへ委譲する。
- `Classes::instances` は `instance Add<Pair<i64, i64>>` を受け付ける。
  生成される `$instance.<id>.<method>` のシグネチャ戻し `type_expression` は `Pair<i64, i64>` の `Apply` を生成する。
- `polymorph::type_expression` は `Type::Record(id, args)` を次の形へ戻す。

```rust
TypeExprKind::Apply(
    Ident { text: records[id].name.clone(), span },
    args.iter().map(|arg| type_expression(arg, records, span)).collect(),
)
```

  `records[id].name` は `Module.Name` の完全修飾名なので、インスタンス合成内で曖昧にならない。
- `Specializer::instantiate` は `validate_size` に固定の `record_sizes: &[usize]` を渡す現状を変える。
  `validate_size` は `Type::Record(id, args)` を直接受け、必要なら per-instance のレイアウトキャッシュを更新する。
  同じ箇所で `reject_record_references_after_substitution(ty, types, span)` を呼び、generic record field に参照が混入した concrete 型を `E1013` で拒否する。
- 特殊化上限 `MAX_SPECIALIZATIONS = 1024` は既存のまま。
  型引数が増大し続ける多相再帰は既存どおり `E1017`。

### レイアウト

- 宣言単位の `record_size(id, ...)` は、非ジェネリックの高速経路として残してよいが、正しさは具体型単位で保証する。
- 新しいレイアウトキャッシュ:

```rust
type LayoutKey = Type; // Type::Record(id, concrete_args) を含む具体型
type LayoutSizes = BTreeMap<LayoutKey, usize>;
```

- `layout_size(ty, records, cache, visiting, depth, span)` は `Type::Record(id, args)` をキーにする。
  `args` に `Variable` / `Infer` が残る呼び出しは `validate_size` ではなく `bounded_type` だけにする。
- 具体型の `visiting` は `BTreeSet<Type>`。
  `record Box<'a> { value: Box<'a> }`、`record A<'a> { b: B<'a> } record B<'a> { a: A<'a> }` は `E1010`。
- `record Holder<'a> { value: 'a }` 自体は宣言時にサイズ未確定。
  `Holder<[i64]>` や `Holder<Pair<i64, string>>` に具体化した時点で 64 KiB 上限と深い不変性を検査する。
- `layout_size` は `Array`／`List` の要素型にも再帰的に `layout_size` を呼び、現行仕様どおり再帰的なヒープ型を A04 まで拒否する。

### LLVM

- `emit_target` のレコード型定義出力を、宣言一覧から「使用された具体インスタンス一覧」へ変更する。
- 収集対象:
  - **実際に出力する** `CheckedFunction` と、closure / export / builtin / call-specialization wrapper の型だけ。
  - 到達可能性（E02）が入った後は、未使用 std 関数・未使用 std 型から concrete instance を出さない。
  - 各 emitted `CheckedFunction.signature.parameters` と `signature.result`
  - 各 emitted `CheckedFunction.parameters`
  - emitted body 内の全 `TypedExpr.ty`
  - `TypedExprKind::GenericFunction` は単相化後には残らないため、LLVM 段では出現しないことを `unreachable!` ではなくテストで保証する。
  - `PatternStep`, `PatternAlternative.bindings`, `Local.ty`
  - クロージャ環境型 `environment_type`
- 収集ヘルパー:

```rust
fn collect_record_instances(ty: &Type, out: &mut BTreeSet<Type>);
fn canonical_type_text(ty: &Type, types: &TypeContext<'_>) -> String;
fn named_type_llvm_name(kind: NamedTypeKind, id: usize, args: &[Type], types: &TypeContext<'_>) -> String;
fn record_instance_fields(id: usize, args: &[Type], types: &TypeContext<'_>) -> Vec<Type>;
```

- `canonical_type_text` は D-03 の Tsuzuri 正規表記を使う。
  型引数に LLVM 型文字列を使ってはならない。
  例: `Pair<i64, string>` → `Main.Pair[i64,string]`、`Pair<Option<string>, [i64]>` → `Main.Pair[Option.Option[string],array[i64]]`。
- `named_type_llvm_name` は records/unions で共有し、`%"tz.record.Main.Pair[i64,string]"` のような引用符付き名を返す。
  非ジェネリックも同じ関数で `%"tz.record.Main.Point"` としてよいが、既存 IR 互換を優先する場合は `args.is_empty()` の旧名を許す。どちらを選んでも records/unions で一貫させる。
- 型定義は `BTreeSet<Type>` 順に出す。
  `Type` は `Ord` を派生しているため、`Record(id, args)` の順序は決定的。
- `drop_value` と `clone_value` は `record_instance_fields` を使う。
- `TypedExprKind::Field` の LLVM は `extractvalue` と `getelementptr` の index は宣言フィールド index のまま。
  型だけを置換する。
- `llvm_frame::field_types` も `record_instance_fields` を使う。

### 各コンパイラ段の変更有無

| 段 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | `TypeExprKind::Apply`、`type_primary`、共通の `<...>` リスト、レコード型パラメーター、instance head を追加 |
| computation::expand | `TypeExpr` 内の `Apply` とレコードパターンを再帰走査するよう更新 |
| check | `TypeContext`, `Type::Record(id,args)`, `CheckedRecord.parameters`, class 名収集分割、型適用解決、レコードリテラル推論、フィールド置換、参照 field 禁止、具体レイアウト |
| polymorph | `Record` 引数走査、インスタンス型式戻し、単相化後検査 |
| control | レコードパターンで型引数を推論・置換 |
| closures | `TypedExpr::children` が漏れなければ原則変更なし。ただし型置換で `Local.ty` の `Record` 引数を走査することを確認 |
| recursion | 関数参照グラフは変更なし。型再帰検出は `check.rs` 側 |
| ownership | 構造的 Copy 推論を `Record(id,args)` に対応 |
| ownership_control | 変更なし |
| call_specialization | `mutable_reference` が `Record(id,args)` の置換済みフィールドを見る |
| llvm | concrete record instance named type、drop/clone/field/frame 対応 |
| llvm_control | 変更なし |
| llvm_frame | `Record(id,args)` の stack/frame/relocate 対応 |
| runtime | 変更なし |
| driver | 変更なし |
| main | 変更なし |

### 他チケットへの提供インターフェース

A02 はこのチケットの以下を前提にする。

- `Type::Record(usize, Vec<Type>)` が存在する。
- `TypeExprKind::Apply(Ident, Vec<TypeExpr>)` が存在する。
- `CheckedRecord.parameters: Vec<String>` が存在する。
- `substitute_type_parameters` と `type_parameter_substitutions` が `pub(crate)` で利用できる。
- arity 検査済み前提の非 fallible `record_instance_fields` / `record_field_type` が利用できる。
- `TypeContext<'_>` が導入済みで、A02 が `unions` を追加できる。
- `Type::display` が `Pair<i64, string>` の形で型引数を表示する。
- `canonical_type_text` と `named_type_llvm_name` が records/unions で共有できる。

## 実装手順

### 1. Parser / AST だけを追加

1. `src/syntax.rs` に `TypeExprKind::Apply` と `RecordDecl.parameters` を追加する。
2. `src/parser.rs` の `Parser::program` の `record` 分岐で `<...>` 内の型変数列を読む。
3. `Parser::type_primary` に型引数リストの解析を追加し、`type_product` と instance head parser でも共有する。
4. `Parser::signature` の `ConstraintExpr` prefix 解析は現行どおり分離したままにする。
5. `computation.rs` の `expand` が `TypeExprKind::Apply` 内の型式を漏らさないことを確認する。
6. AST shape tests を追加する。
   - `def f :: Pair<i64, string> -> i64`
   - `def f :: Task<Pair<i64, string>> -> unit`
   - `record Holder<'a> { value: Pair<'a, string> }`
   - `def` 境界で `Pair<i64>\ndef g :: ...` が前の型適用に飲まれないこと
   - `instance Add<Pair<i64>> { ... }` と `instance Add<(Pair<i64>)> { ... }` が同じ head になること

確認:

```sh
cargo test --locked --test frontend generic_record_type_application_parser
cargo test --locked --test frontend
```

`running N tests` の `N` が 0 でないことを確認する。
この段階では型検査でまだジェネリックレコードを受け付けなくてよい。
ただし既存の `Add<'a>`、`Task<i64>`、`[i64]`、`i64 * string` の構文テストはすべて通す。

### 2. `Type::Record` リファクタリング（挙動変更なし）

1. `Type::Record(usize)` を `Type::Record(usize, Vec<Type>)` に変更する。
2. 非ジェネリックレコードをすべて `Type::Record(id, Vec::new())` に置換する。
3. GUIDE §6.3 の全箇所と `grep 'Type::Record\|Self::Record\|Record(' src` の結果を更新する。
4. `Type::display` は `args.is_empty()` のとき既存表示を返す。
5. `Inference::unify` は `args` が空同士なら既存と同じ挙動にする。

確認:

```sh
cargo test --locked --test frontend
cargo test --locked --test types_ownership
```

既存テストの期待 IR は変えない。
この段階で IR が変わる場合は、非ジェネリックの LLVM 名を誤って変えた可能性が高い。

### 3. ジェネリックなレコード宣言と型検査

1. `CheckedRecord.parameters` を追加する。
2. レコード宣言の型変数検査、未使用検査、未宣言検査を `E1024` で実装する。
3. `resolve_type` と `Classes::inline_constraints` で `Apply` を分類する。
4. レコードリテラルの型引数推論を実装する。
5. フィールドアクセス、レコードパターン、`Type::display` を置換対応にする。
6. 型引数置換後の field 型が共有参照または排他参照を含む concrete record 型を `E1013` で拒否する。
7. `Classes::instances` と `polymorph::type_expression` を `Apply` 出力にする。
8. `ownership::Checker::is_copy` / `require_copy` を置換対応にする。

確認:

```sh
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
cargo test --locked --test control
```

### 4. 具体インスタンス単位のレイアウトと LLVM

1. `record_size` / `layout_size` / `validate_size` を具体型キャッシュに移行する。
2. `Specializer::instantiate` で関数シグネチャと式中の具体型を再検査する。
3. `emit_target` で実際に出力する関数・ラッパーだけから `BTreeSet<Type>` のレコードインスタンスを収集する。
4. `llvm_type`, `drop_value`, `clone_value`, `llvm_frame` を置換済みフィールド型にする。
5. D-03 の `canonical_type_text` による structural mangling を実装する。
6. 決定的 IR と Clang assemble tests を追加する。

確認:

```sh
cargo test --locked --test generic_records
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
```

Node E2E は既存 release バイナリを更新した後に実行する。
このチケットを実装する担当者はビルドが必要なので、GUIDE §3 の full validation を最後に実行する。

### 5. ドキュメント

1. `docs/language.md` の「多相関数と型クラス」「配列・連結リストとレコード」「型とメモリ」の未対応記述を更新する。
2. `docs/architecture.md` の `Type` とレイアウト不変条件を更新する。
3. `README.md` の未実装一覧から「ジェネリックなレコード型」を外し、短い例を追加する。

確認:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test generic_records
cargo test --locked --test frontend
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
cargo test --locked --test control
# 各コマンドの `running N tests` が 0 でないことを記録する
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
```

## テスト計画

すべての Rust 検証は `cargo test --locked --test <file>` 形式で実行し、出力の `running N tests` が 0 でないことを記録する。
Node E2E block の直前には毎回 `cargo build --release --locked` を実行する。

### Rust 受理テスト

`tests/generic_records.rs` を追加する。

```text
record Pair<'a, 'b> { first: 'a, second: 'b }

def first :: Pair<'a, 'b> -> 'a
fn first pair = pair.first

def swap :: Pair<'a, 'b> -> Pair<'b, 'a>
fn swap pair = Pair { first: pair.second, second: pair.first }

let p = Pair { first: 20, second: "xx" }
first p + p.second.length
```

期待:

- `Pair { first: 20, second: "xx" }` は `Pair<i64, string>`。
- `swap` は `Pair<'a, 'b> -> Pair<'b, 'a>`。
- `Type::display` を含む失敗メッセージで `Pair<i64, string>` と表示される。

追加受理:

```text
record Box<'a> { value: 'a }
record Nested<'a> { item: Box<Pair<'a, i64>> }
def get :: Nested<string> -> string
fn get n = n.item.value.first
get (Nested { item: Box { value: Pair { first: "ok", second: 1 } } })
```

```text
record Pair<'a, 'b> { first: 'a, second: 'b }
instance Add<Pair<i64, i64>> {
    fn add left right =
        Pair { first: left.first + right.first, second: left.second + right.second }
}
let p = Pair { first: 20, second: 1 } + Pair { first: 22, second: 2 }
p.first * 10 + p.second
```

```text
record Holder<'a> { value: 'a }
def id_holder :: Holder<'a> -> Holder<'a>
fn id_holder h = h
id_holder (Holder { value: [1, 2, 3] }).value.length
```

```text
record Pair<'a, 'b> { first: 'a, second: 'b }
match Pair { first: "a", second: 42 } with
| Pair { first: text } -> text.length
```

```text
record Pair<'a, 'b> { first: 'a, second: 'b }
def use_unqualified :: Pair<i64, i64> -> i64
fn use_unqualified pair =
    match pair with
    | { first = x; second = y } -> x + y
use_unqualified (Pair { first: 20, second: 22 })
```

### Rust 拒否テスト

すべて診断コードを検査する。

| ソース | 期待 |
|---|---|
| `record Box<'a> { value: i64 }` | `E1024`, unused type parameter |
| `record Box<'a, 'a> { value: 'a }` | `E1024`, duplicate type parameter |
| `record Box { value: 'a }` | `E1024`, undeclared type parameter |
| `record Pair<'a, 'b> { first: 'a, second: 'b } def f :: Pair<i64> -> i64 ...` | `E1004`, wrong arity |
| `record Point { x: i64 } def f :: Point<i64> -> i64 ...` | `E1004`, non-generic type applied |
| `record Add<'a> { value: 'a }` | `E1001`, conflicts with builtin class |
| `class C<'a> { def f :: 'a -> i64 } record C<'a> { value: 'a }` | `E1001`, class/type conflict |
| `record R<'a> { next: R<'a> }` | `E1010`, recursive value layout before A04 |
| `record R<'a> { next: [R<'a>] }` | `E1010`, recursive heap type before A04 |
| `record Pair<'a, 'b> { first: 'a, second: 'b } let p: Pair<i64, string> = Pair { first: 1, second: 2 }` | `E1003` |
| `record Holder<'a> { value: 'a } def f :: Holder<&i64> -> i64 ...` | `E1013`, substituted field contains a shared reference |
| `record Holder<'a> { value: ['a] } def f :: Holder<&mut i64> -> i64 ...` | `E1013`, nested collection contains a mutable reference |
| `record C<'a> { value: 'a } class C<'a> { def f :: 'a -> i64 }` | `E1001`, class/type collision regardless of declaration order |

### LLVM / IR テスト

`tests/generic_records.rs` で `llvm::emit` を 2 回呼び、IR が一致することを確認する。

検査する文字列:

```rust
assert!(ir.contains("%\"tz.record.Main.Pair[i64,string]\" = type { i64, %tz.string }"));
assert!(ir.contains("%\"tz.record.Main.Pair[i64,i64]\" = type { i64, i64 }"));
assert_eq!(
    ir.matches("tz.record.Main.Pair[i64,string]").count(),
    expected_count,
);
```

`expected_count` は厳密な数にしすぎず、型定義が 1 回だけであることを個別に検査する。
入れ子型の mangle も検査する。

```text
record Wrap<'a> { value: 'a }
record Pair<'a, 'b> { first: 'a, second: 'b }
def f :: Wrap<Pair<i64, string>> -> i64
```

IR は `tz.record.Main.Wrap[Main.Pair[i64,string]]` を含み、native と wasm32 の両方で `--emit object` または `--emit llvm` → Clang/wasm toolchain の assemble が成功すること。

### E2E fixture

`tests/fixtures/generic_records/Main.tz`:

```text
record Pair<'a, 'b> { first: 'a, second: 'b }
record Box<'a> { value: 'a }

def add_pair :: Pair<i64, i64> -> Pair<i64, i64> -> Pair<i64, i64>
fn add_pair a b =
    Pair { first: a.first + b.first, second: a.second + b.second }

def owned :: Box<string> -> i64
fn owned box =
    let text = box.value
    text.length

export def numbers :: i64
fn numbers =
    let p = add_pair (Pair { first: 20, second: 1 }) (Pair { first: 22, second: 2 })
    p.first * 100 + p.second

export def strings :: i64
fn strings =
    owned (Box { value: "hello" })
```

独立期待値:

- `numbers()` = `(20 + 22) * 100 + (1 + 2)` = `4203`
- `strings()` = UTF-8 bytes length of `"hello"` = `5`

Node E2E:

- native `-O0` / `-O3`
- wasm32 `-O0` / `-O3`
- WASM imports が空
- IR 2 回出力が同一
- native 確保追跡で各 exported call 後 `live == 0`
- Node block の直前に毎回 `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

### 既存テスト

少なくとも以下を通す。

```sh
cargo test --locked --test generic_records
cargo test --locked --test frontend
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
cargo test --locked --test control
# 各コマンドの `running N tests` が 0 でないことを記録する
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
cargo build --release --locked
node tests/computations.mjs target/release/tsuzuri
cargo build --release --locked
node tests/examples.mjs target/release/tsuzuri
```

## ドキュメント

- `docs/language.md`
  - 「型とメモリ」の型表に `Pair<'a, 'b>` のようなジェネリックレコード例を追加。
  - 「多相関数と型クラス」の未対応一覧から「ジェネリックなレコード宣言」を外す。
  - レコード宣言構文、型適用構文、`Add<'a>` 制約との関係を明記。
  - 診断表に `E1024` を追加。
- `docs/architecture.md`
  - `Type::Record(usize, Vec<Type>)`、具体型単位レイアウト、LLVM named type の決定性を記載。
  - `emit_target` が具体インスタンスを `BTreeSet` で集める不変条件を追加。
- `README.md`
  - 冒頭の未実装一覧から「ジェネリックなレコード型」を削除。
  - 短い例 `record Pair<'a, 'b>` を追加。

## 受け入れ条件

- [x] `TypeExprKind::Apply` が導入され、`Add<'a>` と `Pair<i64, string>` が同じ構文から正しく分類される。
- [x] `RecordDecl.parameters` と `CheckedRecord.parameters` があり、未使用・未宣言・重複パラメーターが `E1024`。
- [x] `Type::Record(usize, Box<[Type]>)` に移行し、既存の非ジェネリックレコードの挙動と IR が実質的に変わらない。
- [x] レコードリテラル、フィールドアクセス、レコードパターンで型引数が推論・置換される。
- [x] 型引数置換後の record field が共有・排他参照を含む concrete 型は、literal / annotation / signature / specialization の全境界で `E1013`。
- [x] `instance Add<Pair<i64, i64>>` のような具体インスタンスが使える。
- [x] 所有権検査が型引数内の Copy / Capture / Send / loan を正しく伝播する。
- [x] 具体インスタンス単位で 64 KiB レイアウト上限と再帰検出が行われる。
- [x] LLVM named type は D-03 の Tsuzuri canonical type text で具体インスタンスごとに 1 回だけ、実際に出力する関数から決定的順序で出力される。
- [x] nested generic type の native/wasm32 Clang assemble が成功する。
- [x] レコード型は export 不可のまま。
- [x] native/WASM × `-O0`/`-O3` の E2E、WASM imports なし、heap tracking `live == 0` を確認した。
- [x] `README.md`, `docs/language.md`, `docs/architecture.md` が更新された。

## 落とし穴

- `Add<'a>` を parser で `Constrained` に残すと、`Pair<i64, string>` と同じ処理経路にならない。
  D-02 どおり構文は `Apply` に揃え、型解決で分類する。
- `Type::Record(id, args)` の `args` を `map_type` / `resolve` / `substitute` で走査し忘れると、単相化後に `Variable` や `Infer` が LLVM へ漏れる。
- `Type::is_copy` と `needs_drop` は同義ではない。
  関数値は Copy だが drop が必要である。
- レコードリテラルのフィールドを宣言順に評価し直してはいけない。
  値構築の `insertvalue` 順序だけを宣言順にできる。
- 非ジェネリックレコードの LLVM 型名を変えると既存 IR テストや fixture の文字列検査が壊れる。
- `record_size` を宣言単位のまま残すと `record Box<'a> { value: 'a }` のような型でサイズを決められない。
- `Classes::inline_constraints` と `resolve_type` の両方で `Apply` を処理しないと、制約が落ちたり型適用が型クラスとして誤解されたりする。
- `Pair<i64 -> i64, string>` の表示と canonical mangling は別物である。LLVM 型文字列を mangle に入れず、D-03 の `fn[i64->i64]` 形式を使う。
- レコード名と型クラス名の衝突を後回しにすると、`Add<'a>` が型適用か制約か曖昧になる。
- 型引数の境界はカンマと `>` で明示し、入れ子の `Box<i64>` を外側の引数列へ平坦化しない。
- `Parser::parameters` は comma 区切りのみなので、record fields に `;` を許すと parser helper と仕様がずれる。

## 対象外

- `union` と列挙型（A02）。
- 型別名 `type`（A05）。
- 条件付きインスタンス、汎用インスタンス、スーパークラス（A06）。
- 高階型 `'f<'a>` のような型コンストラクター変数（A10）。
- レコード更新 `{ p with x = ... }`（C05）。
- ジェネリック関数の export。
- 再帰的なヒープ型の許可（A04）。

## 未決事項

- 型パラメーター未使用は既定で `E1024` とする。
  代替案は G03 後に `W1002` 相当の警告へ下げることだが、A01 では型宣言の誤りとして拒否する。
- `TypeExprKind::Constrained` を完全削除するか、互換の内部表現として残すか。
  既定案は段階 1 では残し、A01 完了時点では新規 parser 出力を `Apply` に統一する。
- 非ジェネリックレコードの LLVM 型名を既存互換で残すか、すべて引用符付きマングリングに揃えるか。
  既定案は既存互換を優先し、`args.is_empty()` の場合だけ旧名を使う。

### 実装時の判断（A01）

- 型適用は TypeExprKind::Apply に統一し、名前解決でクラスの制約と record／union の適用を区別する。
  型引数は Box<[Type]> とし、Type を 32 バイトに保って深い検査のスタック消費を増やさない。
- TypeContext の record_fields／record_field で型引数を置換し、レイアウト・Copy／drop／loan・所有権・LLVM が
  同じ具体フィールド型を使う。参照を格納する具体化と再帰型は拒否し、64 KiB の上限を具体化後にも検査する。
- 非ジェネリックな LLVM 型名は従来の名前を維持する。具体化した型だけを順序付き集合で集め、
  LLVM 型文字列ではなく構造的な正規表記を引用符付きの名前に使う。
- 未使用／重複／未宣言パラメーターは E1024。匿名の `'_` は字句規則に従い E0001 とする。
  推論途中の型引数不一致では、不完全なレコード全体の型を診断に組み立てず、最初に異なる引数を報告する。
