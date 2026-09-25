# A02: 判別共用体（union）と列挙型

| 項目 | 内容 |
|---|---|
| ID | A02 |
| 優先度 | P0 |
| 規模 | XL |
| 依存 | A01 |
| 後続 | A03, A04, A07, B01, B04, C06 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `src/closures.rs`, `src/recursion.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/llvm_frame.rs`, `src/computation.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/*.rs`, `tests/*.mjs` |

## 目的

Tsuzuri に代数的データ型の中核である判別共用体を追加する。
`Option 'a`、`Result 'a 'e`、列挙型、軽量なドメインモデルを表せるようにし、B01 と A03 の基盤にする。

このチケットでは `union` 宣言、ケース構築、ケースパターン、所有権、単相化、LLVM lowering を実装する。
再帰的 union は A04 まで `E1010` で拒否する。
union の `==` / `!=` / `<` などは組み込みにしない。A11 により比較演算は非消費・借用シグネチャへ変わるが、union で使うには `Eq` / `Ord` インスタンスが必要で、A07 の deriving がそれを生成する。
`Display` は D01 / A07 で扱う。

## 現状

- `src/syntax.rs` の `TokenKind` に `Union` はない。
  `union` は現状では通常識別子であり、トップレベルに書くと entry-point code と解釈され、現行バイナリでは `E0002 expected the end of the file; declarations must precede the entry-point code` になる。
- `src/syntax.rs` の `Program` は `records`, `functions`, `classes`, `instances`, `active_patterns`, `entry` だけを持ち、union 宣言を持たない。
- `src/check.rs` の `Type` に union 表現はない。
- `src/check.rs` の `Names` は `records`, `record_aliases`, `functions`, `active_patterns` を持つが、union 型名・ケース名を解決できない。
- `src/parser.rs` の `Parser::program` は `record`, `class`, `instance`, `def`/`fn`/`export`/`and` をトップレベル宣言として処理する。
- `src/parser.rs` の `Parser::qualified_ident` は `A.B` までしか読まず、`Module.Union.Case` の 2 段修飾を表せない。
- 値の名前解決は `src/check.rs` の `Checker::name` で、ローカル → 同一モジュール関数 → builtin の順。
  他モジュール関数は `ExprKind::Field(Name(module), field)` を `Checker::value_expression` で処理する。
- パターンでは `src/parse_control.rs` の `Parser::named_pattern` が大文字始まり + 引数ありを `PatternKind::Apply` とし、引数なしの大文字名は `PatternKind::Binding` になる。
  `src/control.rs` の `Checker::pattern_alternatives` は `PatternKind::Binding(name)` で active recognizer が存在する場合だけ recognizer として処理する。
  そのため `None` のような nullary union case は現状では変数束縛として扱われる。
- `src/llvm_control.rs` の `switch_cases` は、match 対象 local の型が `Integer`, `Bool`, `Unit` の場合だけ、単一の `PatternStep::Test(Binary(Equal, Local(id), constant))` を `switch` にする。
  union tag には未対応。
- 文字列・配列・リスト・関数値の drop/clone は `src/llvm.rs` の `FunctionEmitter::drop_value` / `clone_value` に実装済み。
  `@tz.free(null)` は native では C `free(null)`、WASM では `heap-wasm.ll` の `@tz.free` が null を明示的に無視する。
  `@tz.closure.drop` も環境 pointer null を無視する。
- `src/runtime/heap-wasm.ll` は 16 MiB 上限のヒープを持つ。

## 仕様

### 構文

台帳 D-05 に従う。

```text
UnionDecl      ::= "union" TypeName TypeParameter* "=" UnionCases
UnionCases     ::= UnionCase ( "|" UnionCase )*
UnionCase      ::= "|" ? CaseName ( "of" TypeExpr )?
TypeParameter  ::= TypeVariable
CasePattern    ::= CaseName Pattern?
                 | ModuleName "." CaseName Pattern?
                 | ModuleName "." UnionName "." CaseName Pattern?
CaseExpr       ::= CaseName
                 | ModuleName "." CaseName
                 | ModuleName "." UnionName "." CaseName
```

例:

```text
union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

union Option 'a = None | Some of 'a
```

- `union` は新しい予約語として `TokenKind::Union` に追加する。
- `of` は台帳 D-15 の新規予約語一覧に含まれていないため、このチケットでは **union 宣言内だけの文脈キーワード**として扱う。
  `lexer.rs` に `TokenKind::Of` は追加しない。
  `Parser` は `Ident("of")` を union case の payload 導入として消費する。
- 先頭の `|` は省略可。
  複数行・単一行の両方を受け付ける。
- 各 case は payload を 0 個または 1 個だけ持つ。
  複数値は `f64 * f64` のタプル型、またはレコード型で表す。
- case 名は `PascalCase`、少なくとも先頭 ASCII 大文字。
  `snake_case` case は `E1024`。
- 型パラメーター構文と型適用は A01 の `TypeExprKind::Apply` を前提にする。
- `.tz` と `.tc` で `union` 宣言を許可する。
  `.tt` にある union は `E1018`。
- `union` 宣言はトップレベル宣言であり、entry-point code より前に置く。

### 型規則

`src/check.rs` に追加する。

```rust
pub enum Type {
    // A01 までの既存 variant ...
    Record(usize, Vec<Type>),
    Union(usize, Vec<Type>),
    // ...
}

pub struct CheckedUnion {
    pub name: String,
    pub parameters: Vec<String>,
    pub cases: Vec<(String, Option<Type>)>,
    pub span: Span,
}

pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub unions: Vec<CheckedUnion>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
}

pub struct TypeContext<'a> {
    pub records: &'a [CheckedRecord],
    pub unions: &'a [CheckedUnion],
}
```

`src/syntax.rs` に追加する。

```rust
pub struct UnionDecl {
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub cases: Vec<UnionCaseDecl>,
}

pub struct UnionCaseDecl {
    pub name: Ident,
    pub payload: Option<TypeExpr>,
    pub span: Span,
}

pub struct Program {
    pub source_kind: Option<SourceKind>,
    pub records: Vec<RecordDecl>,
    pub unions: Vec<UnionDecl>,
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
    pub instances: Vec<InstanceDecl>,
    pub active_patterns: Vec<ActivePattern>,
    pub entry: Option<Expr>,
}
```

- `Type::Union(id, args)` の `args` は `CheckedUnion.parameters` と同数。
- `Option i64` は `Type::Union(option_id, vec![Type::I64])`。
- payload 型は宣言の型パラメーターを型引数で置換して得る。
- `Type::display` は `Option i64`、`Result i64 string` の形。
- 単一化は union ID と型引数ごとの単一化。
- `Type::exportable` は union を常に `false`。
- A01 の `TypeContext` を拡張し、`Type::display`、型性質、layout、class validation、ownership、LLVM helper はすべて `&TypeContext` を受け取る。
- 型コンストラクター arity は `resolve_type`、case constructor 型スキーム生成、pattern case 解決、annotation/signature、`Specializer::instantiate` などの構築境界で検査する。
  検査後に使う helper は非 fallible にする。

```rust
pub(crate) fn union_instance_cases(
    id: usize,
    args: &[Type],
    types: &TypeContext<'_>,
) -> Vec<(String, Option<Type>)>;

pub(crate) fn union_case_payload_type(
    id: usize,
    args: &[Type],
    case_id: usize,
    types: &TypeContext<'_>,
) -> Option<Type>;
```

### 名前解決

同一モジュール内では次の重複を `E1001` または `E1024` で拒否する。

| 重複 | 診断 |
|---|---|
| union 型名と record 型名 | `E1001` |
| union 型名と同一モジュールの union 型名 | `E1001` |
| case 名と同一 union 内 case 名 | `E1024` |
| case 名と同一モジュールの別 union case 名 | `E1001` |
| case 名と同一モジュールの record 型名または union 型名 | `E1001` |
| case 名と同一モジュールの関数名 | `E1001` |
| case 名と同一モジュールの active pattern case 名 | `E1001` |

この制限により、`Module.Case` が関数か case かで曖昧にならない。
ローカル束縛だけは意図的なシャドーイングを許す。

値式の解決優先順位:

| 形式 | 優先順位 |
|---|---|
| `Name` | 1. ローカル束縛、2. 同一モジュール関数、3. builtin、4. 同一モジュール union case、5. プロジェクト内で一意な union case、6. `E1002` |
| `Module.Name` | 1. `Module` がローカルならフィールドアクセス、2. `Module.Name` 関数、3. `Module.Name` union case、4. `E1002` |
| `Module.Union.Case` | 1. union case、2. `E1002` |

`Name` で case が複数モジュールに存在する場合は `E1004`。
メッセージは `"ambiguous union case 'Some'; qualify it as Option.Some or Other.Some"`。
E02 の std 同梱後は GUIDE D-07 に従い、利用者モジュールからは自モジュール → 利用者モジュールで一意 → std、std モジュールからは自モジュール → std のみを探す。
std の `Option.tc` / `Result.tc` は他 std module の型・case を必ず `Module.Type` / `Module.Case` で修飾し、利用者が `Ok` や `Result` を宣言しても std の型検査結果が変わらないようにする。

実際の parser は `A.B.C` を 1 個の identifier として出さず、`ExprKind::Field(ExprKind::Field(ExprKind::Name(A), B), C)` の入れ子を作る。
型検査では method/field 解決の前に path を flatten する。

| segment 数 | 意味 |
|---|---|
| 1 (`Case`) | 無修飾 case。ローカル/関数/builtin 優先の後、自モジュール case → 利用者モジュールで一意 → std の順 |
| 2 (`Module.Case`) | `Module` がローカルなら通常 field。そうでなければ module case または module function。std の `Option.Some` / `Result.Ok` はこの形で、`Option` は module 名である |
| 2 (`Union.Case`) | 現在モジュールに同名 union があり、同時に `Union` という module も見える場合は曖昧なので `E1004` で `Module.Union.Case` を要求する。module がなければ current-module union case |
| 3 (`Module.Union.Case`) | 指定 module 内の指定 union case。関数・record field とは解釈しない |

`Module.Case` と current-module `Union.Case` が両方可能な場合、`Module.Union.Case` を要求する。
診断例: `"ambiguous path 'Option.Some'; use Module.Union.Case when a module and a local union share the prefix"`。

パターンの解決優先順位:

| 形式 | 優先順位 |
|---|---|
| `_` / `otherwise` | wildcard |
| `Name`（大文字始まり、引数なし） | 1. union case、2. active recognizer、3. binding |
| `Name p`（大文字始まり、引数あり） | 1. union case payload pattern、2. active recognizer、3. `E1020` |
| `Module.Name` | 1. union case、2. active recognizer、3. `E1020` |
| `Module.Union.Case` | union case |
| 小文字名 | binding |

nullary case `None` は変数束縛ではなく case pattern になる。
case と active recognizer が同時に見える場合は宣言重複を原則拒否する。
別モジュール由来で曖昧な場合は `E1004`。

### 構築

- payload なし case は値。
  `Empty` の型は `Shape`。
- payload あり case は 1 引数関数値。
  `Circle` の型は `f64 -> Shape`。
  `Circle 1.0` は union 値。
- case constructor は通常の関数値として高階関数に渡せる。
  例: `Option.map Some value` は B01 で利用する。
- 実装は hidden generic function を合成する。
  名前はユーザーが書けない `$case.<Module>.<Union>.<Case>`。
  例: `$case.Main.Option.Some`.
- ただし direct full application は hidden function call ではなく `TypedExprKind::Construct` へ直接 lower する。
  これにより `Some expensive` は追加 call を持たず、既存の左から右評価順序を保つ。

追加する typed kind:

```rust
pub enum TypedExprKind {
    // ...
    CaseConstructor {
        union_id: usize,
        case_id: usize,
        args: Vec<Type>,
    },
    Construct {
        union_id: usize,
        case_id: usize,
        payload: Option<Box<TypedExpr>>,
    },
    UnionTag(Box<TypedExpr>),
    UnionPayload {
        value: Box<TypedExpr>,
        case_id: usize,
    },
}
```

- `CaseConstructor` は関数値として残る場合に `closures::lower` で hidden function 参照へ変換する。
- `CaseConstructor` は単相化後まで残し、`polymorph::expression_types` で `args` を走査して concrete 化する。
- `Construct` の `ty` は `Type::Union(union_id, args)`。
- `UnionTag` の `ty` は `Type::Integer(32, true)`。
- `UnionPayload` の `ty` は置換済み payload 型。

### パターン

例:

```text
match shape with
| Circle r -> r * r * 3.141592653589793
| Rect (w, h) -> w * h
| Empty -> 0.0
```

- case pattern は最初に tag を検査する。
- payload がある case で payload pattern がない場合は `E1020`。
  例: `| Some -> ...` は payload を束縛しない曖昧な形なので拒否し、`| Some _ -> ...` を要求する。
- payload がない case に payload pattern を付けたら `E1020`。
- payload projection は `UnionPayload` typed kind で表す。
- `control.rs::pattern_alternatives` は union case pattern を次の形へ展開する。

```text
PatternStep::Test(UnionTag(subject) == tag)
payload projection: UnionPayload(subject, case_id)
```

- union pattern の OR/AND/as/annotation は既存の組み合わせ規則を使う。
- guard 中は既存どおり読み取り専用 view を使い、guard 成功後にだけ所有する pattern binding へ move / copy する。

### 所有権・Copy・drop

- union 値は payload を所有する。
- 全 payload が Copy なら union も Copy。
  nullary enum は Copy。
- いずれかの payload が `needs_drop` なら union も `needs_drop`。
- `carries_loans`, `can_capture`, `can_send` は全 payload 型を見て構造的に決める。
- payload を非 Copy として取り出すと union 値全体を消費する。
  payload だけを move して tag だけ残す値はユーザーには観測させない。
- LLVM lowering では payload を取り出して union の残りを drop する場合、取り出した payload slot を `zeroinitializer` で上書きしてから union 全体を drop する。
  これにより二重解放を避ける。
- zeroed values の drop 安全性:
  - string / array / list は pointer null, length 0 になり、native `free(null)` と WASM `@tz.free` の null check で安全。
  - closure は env null なら `@tz.closure.drop` が何もしない。
  - tuple / record / union は中身の zeroed drop が再帰的に安全。
- union は `Eq` / `Ord` の組み込み対象にしない。
  D-20/A11 により `Eq.eq` / `Ord.lt` は `&'a -> &'a -> bool` だが、union に対してはインスタンスがない限り `==` などは `E1005 no instance for Eq ...`。
  A07 の deriving が union 用の借用シグネチャ instance を生成する。
- Display も組み込みにしない。

## 設計

### レイアウト

タグは常に `i32`。

| union 形 | LLVM 型 |
|---|---|
| 全 case が nullary | `i32` |
| nullary 以外の payload 型がすべて同じ LLVM 型 | `{ i32, T }` |
| 一般形 | `{ i32, [K x i128] }` |

- `layout_size` / `validate_size` / `llvm_frame::stack_size` は LLVM 型の見かけだけでなく、値全体の conservative allocation size を同じ規則で返す。
  - nullary enum: 8 bytes。LLVM は `i32` だが既存の小値ルールに合わせ、値 layout は少なくとも pointer-sized slot として 8 に丸める。
  - common-payload layout: `16 + round_up_16(payload_upper_bound)` bytes 以上。tag と padding を 16 bytes と見積もり、payload は A01/A02 の conservative upper bound を 16-byte 境界へ丸める。
  - general layout: `16 + 16 * K` bytes。`K = ceil(max_payload_upper_bound / 16)`、payload がない case は 0 として扱う。
  これにより 64 KiB 判定、stack literal 判定、LLVM aggregate 型定義が同じ上限を使う。
- 一般形の payload storage は `[K x i128]` とする。
  `K = ceil(max_payload_layout_size / 16)`。
  `layout_size` は現行の保守的な値レイアウト規則であり、native 64-bit と wasm32 の実 LLVM サイズ以上になる。
  `i128` storage を使うため 16-byte alignment が必要な payload も安全に格納できる。
- opaque pointer LLVM IR では payload storage field の pointer に対して typed `store T` / `load T` を行う。
  例:

```llvm
%payload = getelementptr inbounds %"tz.union.Main.Shape", ptr %slot, i32 0, i32 1
store { double, double } %rect, ptr %payload, align 16
%loaded = load { double, double }, ptr %payload, align 16
```

- enum layout:

```llvm
%"tz.union.Main.Color" = type i32
```

- single payload type layout:

```llvm
%"tz.union.Main.MaybeF64" = type { i32, double }
```

- general layout:

```llvm
%"tz.union.Main.Shape" = type { i32, [1 x i128] }
```

`Rect of f64 * f64` は `{ double, double }` で 16 bytes、`Circle of f64` は 8 bytes、`Empty` は 0 bytes なので `K = 1`。

### LLVM lowering

Construct:

```llvm
; Circle 2.0, tag 0
%u0 = insertvalue %"tz.union.Main.Shape" zeroinitializer, i32 0, 0
%payload.ptr = alloca %"tz.union.Main.Shape", align 16 ; 必要なら entry-block spill
store %"tz.union.Main.Shape" %u0, ptr %payload.ptr
%field = getelementptr inbounds %"tz.union.Main.Shape", ptr %payload.ptr, i32 0, i32 1
store double %r, ptr %field, align 16
%u1 = load %"tz.union.Main.Shape", ptr %payload.ptr, align 16
```

タグ検査:

```llvm
%tag = extractvalue %"tz.union.Main.Shape" %value, 0
%is_circle = icmp eq i32 %tag, 0
```

payload projection:

```llvm
%slot = alloca %"tz.union.Main.Shape", align 16
store %"tz.union.Main.Shape" %value, ptr %slot
%payload.ptr = getelementptr inbounds %"tz.union.Main.Shape", ptr %slot, i32 0, i32 1
%payload = load double, ptr %payload.ptr, align 16
```

- SSA aggregate に対して型の異なる payload を読む必要があるため、projection は entry-block `alloca` へ spill して typed load/store する。
- `UnionPayload` は tag 検査済み path でだけ生成する。
  型検査済み IR の不変条件として、コード生成時に case 不一致 payload を読む path は存在しない。
- `drop_value(Type::Union(id,args), value)` は tag の `switch` で case ごとに payload を読み、payload 型が `needs_drop` なら drop する。
- `clone_value(Type::Union(id,args), value)` も tag の `switch` で payload を clone する。
- `emit_target` は A01 の named type collection を union にも拡張し、`BTreeSet<Type>` で concrete union instances を出力する。
- union named type の mangle は A01 の `canonical_type_text` / `named_type_llvm_name` を共有する。
  例: `%"tz.union.Main.Option[string]"`、`%"tz.union.Main.Result[Main.Pair[i64,string],string]"`。
  LLVM 型文字列（`%tz.string` や引用符付き named type）を mangle に入れてはならない。
- 初期実装では `llvm_frame.rs::frame_inner` から `TypedExprKind::Construct` を除外する。
  union payload に string / array / list literal があっても union construct 全体は heap path に任せる。
  `Frame::Aggregate` は heterogeneous payload union を正しく表せないため使わない。
  将来の最適化として `Frame::Union { case_id, payload_type, frames }` を追加すれば、case ごとの payload frame tracking を安全に実装できる。

### `llvm_control.rs` switch との接続

現行 `switch_cases` は `Local(id) == constant` だけを認識し、`(BTreeMap<u128, usize>, Option<usize>)` を返す。
A02 ではこれを `SwitchPlan` に置換する。

```rust
struct SwitchPlan {
    selector: SwitchSelector,
    selector_type: Type,
    cases: BTreeMap<u128, usize>,
    default: Option<usize>,
    bindings: BTreeMap<(usize, usize), TypedExpr>, // (arm index, binding local id) -> projection
}

enum SwitchSelector {
    Local(Local),
    UnionTag { subject: Local, tag: TypedExpr },
}
```

`match_expression` は `SwitchPlan.selector` を一度だけ materialize する。
union では `%tag = extractvalue ...` または `UnionTag(Local(subject_id))` の expression を i32 selector として 1 回だけ生成し、`switch` と `lookup_match` の両方が同じ selector SSA 値を使う。

union では次を認識する。

```text
PatternStep::Test(Binary(Equal, UnionTag(Local(subject_id)), Int(tag)))
```

条件:

- arm に guard がない。
- その arm の alternatives が tag test だけ、または tag test + payload binding projection だけ。
- payload binding projection は `UnionPayload(Local(subject_id), case_id)` である。
- 複数 alternative が同じ tag を使う場合は先に出た arm を優先する。

switch lowering では branch label に入った後、payload binding があれば `place(projection)` で pointer を得て通常の pattern binding と同じ処理を行う。
payload binding を単純に subject local の pointer へ alias してはならない。

`lookup_match` も `SwitchPlan` を受け取り、selector が union tag の場合はすでに materialize した i32 tag を使う。
payload binding がある場合は lookup table 化しない。

### union payload の place

`UnionPayload` は通常の projection place として所有権・LLVM の両方に参加する。

- `ownership.rs::is_place`, `place`, `read_places` に `UnionPayload(value, case_id)` を追加する。
- `llvm.rs::is_place`, `place`, `read_place` にも同じ projection を追加する。
- 抽象 place path は `PlaceComponent::UnionPayload(case_id)` を追加する。
  既存の `Vec<usize>` sentinel では array/list element と union payload を区別できないため、A02 または A04 で enum component へ移行する。
- root union 全体への consume / move / exclusive borrow は payload path と overlap する。
  payload move 後の source union の後続利用は、Copy でない限り `E1012`。
- guard 失敗や OR alternative 失敗では、payload projection から作った pattern temporaries を既存の `clear_pattern_temporaries` と同じ規則で zero/drop する。
- guard 成功後に payload を所有 binding へ移す場合は、既存 `read_place(take = true)` の exact-slot zeroing を再利用し、payload slot だけを zero にしてから source union remainder を drop する。
- tests:
  - guard 失敗後に次 arm が source union を使える。
  - OR alternative の片側が payload temporary を作って失敗しても leak しない。
  - nested payload move 後に source union 全体を使うと `E1012`。
  - Copy payload は move 後も source union を使える。

### 再帰制限

- A04 まで、union/record をまたぐ再帰的な値レイアウトは `E1010`。
- 例:

```text
union Tree 'a = Leaf | Node of Tree 'a * 'a * Tree 'a
```

は A02 では `E1010 recursive union layout for 'Main.Tree'; recursive heap types require A04`。
- `Option 'a` や `Result 'a 'e` は再帰しないので許可。

### 各コンパイラ段の変更有無

| 段 | 変更 |
|---|---|
| lexer | `union` を `TokenKind::Union` に追加。`of` は文脈キーワードのため変更なし |
| parser | `UnionDecl`, `UnionCaseDecl`, `Program.unions`, `Parser::program` union 分岐、nested `ExprKind::Field` path の flatten 前提 |
| computation::expand | `Program.unions` 自体は展開対象外。case pattern / payload expression 内の既存式走査を維持 |
| check | `TypeContext` を unions で拡張、union 名・case 名収集、型解決、case constructor、`Construct`、union pattern、class 名収集分割 |
| polymorph | `Union(_, args)` と `CaseConstructor.args` の map/substitute/resolve/unify/type_expression/intrinsic validate |
| control | case pattern を tag test + payload projection へ展開 |
| closures | `CaseConstructor` を `(union_id, case_id, concrete_args)` で dedup した hidden monomorphic `CheckedFunction` に置換 |
| recursion | source 関数だけを対象にする。closures lowering 後に追加される hidden case functions は source recursion checking に参加しない |
| ownership | union の Copy/drop/loans/capture/send、payload move |
| ownership_control | `UnionPayload` projection を pattern binding の place として扱う |
| call_specialization | `all_children`, `may_mutate`, `read_only`, `single_use_locals` に union kind を追加 |
| llvm | union named types、construct/tag/payload/drop/clone/frame |
| llvm_control | `switch_cases` / `lookup_match` を union tag に対応 |
| llvm_frame | union construct payload の stack frame tracking |
| runtime | 変更なし |
| driver | 変更なし |
| main | 変更なし |

### 他チケットへの提供インターフェース

A03 は以下を前提にして網羅性検査を実装する。

- `CheckedModule.unions: Vec<CheckedUnion>`。
- `Type::Union(usize, Vec<Type>)`。
- union case の宣言順が tag 値で、`case_id as i32` が実行時 tag。
- case payload 型は `union_case_payload_type(unions, id, args, case_id, span)` のような helper で取得できる。
- `TypedExprKind::UnionTag` と `TypedExprKind::UnionPayload` が存在する。

A04 は以下を拡張する。

- A02 では `E1010` にしている recursive union layout。
- `layout_size` と union payload storage の計算。
- `drop_value` / `clone_value` の再帰的 union path。

B01 は以下を前提にする。

- `union Option 'a = None | Some of 'a`
- `union Result 'a 'e = Ok of 'a | Error of 'e`
- `Some` / `Ok` が一引数の関数値として使える。

## 実装手順

### 1. 構文と AST

1. `TokenKind::Union` を追加し、`Lexer::identifier` で `"union"` を予約語にする。
2. `UnionDecl`, `UnionCaseDecl`, `Program.unions` を追加する。
3. `Parser::program` に union 分岐を追加する。
4. `Parser::qualified_ident` を `qualified_path(max_segments)` へ分け、union case 用に 3 segments まで読めるようにする。
5. `parse_control.rs::named_pattern` で nullary uppercase 名を後で union case として解決できる AST にする。
   既存の `PatternKind::Binding` を使ってもよいが、型検査側で union case を優先する。

確認:

```sh
cargo test --locked --test frontend union_syntax_parser
cargo test --locked --test frontend
# 各コマンドの `running N tests` が 0 でないことを記録する
```

### 2. 型名・case 名の収集

1. A01 の phase split を拡張する。
   - `collect_class_names`: 組み込みクラス、ユーザー class、record、union 型名を先に収集し、`record Add` / `union Add 'a = ...` / cross-order class collision を `E1001`。
   - record/union fields & payloads を解決し layout を検査する。
   - `check_class_declarations` で class method signatures を解決・検証する。
   - instances を検証する。
2. `Names` に `unions`, `union_aliases`, `cases`, `case_aliases` を追加する。
3. `check_modules` 冒頭で records と unions を同時に収集し、同一モジュール重複を拒否する。
4. case 名を収集し、重複ルールを実装する。
5. `.tt` 内 union を `E1018` にする。
6. `CheckedUnion` を作る。
7. 型パラメーターの重複・未使用・未宣言を `E1024` で検査する。

確認:

```sh
cargo test --locked --test modules
cargo test --locked --test frontend
```

### 3. `Type::Union` と多相処理

1. `Type::Union(usize, Vec<Type>)` を追加する。
2. `resolve_type` で union 型適用を解決する。
3. `Type` の表示・性質・layout・exportable を更新する。
4. `polymorph.rs` の `map_type`, `substitute`, `bounded_type`, `Inference::resolve`, `Inference::unify`, `variables`, `type_expression` を更新する。
5. `CaseConstructor.args` を `expression_types` が走査し、単相化後に concrete であることを `require_concrete` する。
6. `Classes::intrinsic` の marker class 判定を union 対応にする。

確認:

```sh
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
```

### 4. constructor と pattern の型検査

1. case constructor の型スキームを作る。
2. `Checker::name` と `ExprKind::Field` の module / union case 解決を実装する。
3. `TypedExprKind::CaseConstructor` と `Construct` を追加し、`TypedExpr::children` / `children_mut` に追加する。
4. direct full application を `Construct` にする。
5. first-class constructor 用 hidden function は `closures::lower` で合成する。
   `BTreeMap<(union_id, case_id, concrete_args), usize>` で dedup し、monomorphic `CheckedFunction` を 1 つ追加する。
   その body は payload parameter を受け取って `TypedExprKind::Construct` を返すだけにする。
   元の `CaseConstructor` value は `FunctionRef::User(hidden_id)` に置換する。
   hidden function は source recursion checking には参加しない。E02 reachability 後は通常の prunable generated function として未使用なら出力しない。
6. `control.rs::pattern_alternatives` で union case pattern を tag test + payload projection にする。
7. `PatternKind::Binding` の nullary case 優先を実装する。

確認:

```sh
cargo test --locked --test union_types
cargo test --locked --test control
```

### 5. 所有権と単相化後検査

1. `ownership.rs` の `closed_returns::owned`, `Checker::is_copy`, `require_copy`, `eval` を union 対応にする。
2. `ownership_control.rs` の pattern binding projection に `UnionPayload` を追加する。
3. `call_specialization.rs` の全走査に union kind を追加する。
4. source recursion checking は hidden constructor functions 追加前に完了していることを確認する。`CaseConstructor` 自体は関数参照ではないため `recursion.rs` の source graph に `$case...` を出さない。
5. `Specializer::instantiate` で concrete union layout と borrow validation を再検査する。

確認:

```sh
cargo test --locked --test union_types
cargo test --locked --test call_specialization
cargo test --locked --test tasks
```

### 6. LLVM

1. union concrete instance collection と named type mangling を実装する。
2. `llvm_type` に `Type::Union` を追加する。
3. `FunctionEmitter::expression_mode` に `Construct`, `UnionTag`, `UnionPayload` を追加する。
4. `drop_value` / `clone_value` に union switch を追加する。
5. `llvm_control.rs::switch_cases` を `SwitchPlan` へ置換し、`lookup_match` と `match_expression` が共通 plan を使う。
6. `llvm_frame.rs` は `Construct` を `frame_inner` 対象外にする。`Frame::Union` は後続最適化として docs にだけ残す。
7. union layout の 64 KiB boundary を native/wasm32 の assemble で検査する。

確認:

```sh
cargo test --locked --test union_types
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
```

### 7. ドキュメントと E2E

1. `tests/fixtures/unions/Main.tz` と `tests/unions.mjs` を追加する。
2. `README.md`, `docs/language.md`, `docs/architecture.md` を更新する。
3. full validation を実行する。

確認:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test union_types
cargo test --locked --test control
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
cargo test --locked --test call_specialization
cargo test --locked --test tasks
# 各コマンドの `running N tests` が 0 でないことを記録する
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
cargo build --release --locked
node tests/unions.mjs target/release/tsuzuri
```

## テスト計画

すべての Rust 検証は `cargo test --locked --test <file>` 形式で実行し、出力の `running N tests` が 0 でないことを記録する。
Node E2E block の直前には毎回 `cargo build --release --locked` を実行する。

### Rust 受理テスト

`tests/unions.rs` を追加する。

Shape:

```text
union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

def area :: Shape -> f64
fn area shape =
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0

area (Rect (3.0, 4.0))
```

Option-like:

```text
union Maybe 'a = None | Some of 'a

def default_value :: 'a -> Maybe 'a -> 'a
fn default_value fallback value =
    match value with
    | Some x -> x
    | None -> fallback

default_value 10 (Some 42)
```

Enum:

```text
union Color = Red | Green | Blue

def code :: Color -> i64
fn code color =
    match color with
    | Red -> 1
    | Green -> 2
    | Blue -> 3

code Green
```

First-class constructor:

```text
union Maybe 'a = None | Some of 'a
def apply :: ('a -> 'b) -> 'a -> 'b
fn apply f x = f x
match apply Some 42 with
| Some n -> n
| None -> 0
```

Qualified cases:

```text
// Shapes.tz
union Shape = Empty | Circle of f64

// Main.tz
match Shapes.Shape.Circle 2.0 with
| Shapes.Circle r -> r as i64
| Shapes.Empty -> 0
```

### Rust 拒否テスト

| ソース | 期待 |
|---|---|
| `union option = None` | `E1024` |
| `union Option 'a = None` | `E1024`, unused type parameter |
| `union Option = Some of 'a` | `E1024`, undeclared type parameter |
| `union U = A | A` | `E1024`, duplicate case in union |
| `record None {} union U = None` | `E1001` |
| `.tt` 内 `union U = A` | `E1018` |
| `union Add 'a = Value of 'a` | `E1001`, builtin class/type collision |
| `class C 'a { def f :: 'a -> i64 } union C 'a = Value of 'a` | `E1001`, cross-order class/type collision |
| `union Maybe 'a = Some of 'a let x = Some` を具体化なしで使う | `E1015` または `E1006` |
| `union Maybe 'a = None | Some of 'a match Some 1 with | Some -> 0 | None -> 1` | `E1020` |
| `union Maybe 'a = None | Some of 'a match None with | None x -> x` | `E1020` |
| `union Tree 'a = Leaf | Node of Tree 'a` | `E1010` until A04 |
| `union U = A of i64 export def f :: U ...` | `E1008` |
| `union Maybe 'a = None | Some of 'a let x = Some 1 == Some 1` | `E1005`; D-20/A11 後も union には `Eq` instance が必要 |
| payload move 後に source union を再使用する | `E1012` |
| guard 失敗のある payload pattern で次 arm が source union を使う | 受理し leak なし |

### LLVM IR テスト

Shape layout:

```rust
assert!(ir.contains("%\"tz.union.Main.Shape\" = type { i32, [1 x i128] }"));
assert!(ir.contains("switch i32"));
assert!(ir.contains("extractvalue %\"tz.union.Main.Shape\""));
```

Enum layout:

```rust
assert!(ir.contains("%\"tz.union.Main.Color\" = type i32"));
assert!(ir.contains("switch i32"));
```

Generic concrete instance:

```rust
assert!(ir.contains("%\"tz.union.Main.Maybe[i64]\""));
assert!(ir.contains("%\"tz.union.Main.Maybe[string]\""));
```

Layout boundary:

```text
union NearLimit = Big of <payload whose upper bound makes union value exactly 64 KiB>
union TooLarge = Big of <payload whose upper bound makes union value exceed 64 KiB by 16 bytes>
```

期待:

- `NearLimit` は native / wasm32 の `--emit object` または LLVM assemble が成功。
- `TooLarge` は `E1010`。
- nullary enum は conservative size 8 として扱われ、64 KiB boundary の計算に `i32` 実サイズを使わない。

SwitchPlan / payload binding:

```text
union U = A of string | B of i64 | C
def f :: U -> i64
fn f value =
    match value with
    | A text -> text.length
    | B n -> n
    | C -> 0
```

IR では tag selector が 1 回だけ materialize され、payload binding は `UnionPayload` projection から読み、subject slot 全体への alias ではないことを文字列で確認する。

Qualified path:

```text
// Option.tc (std 相当)
union Option 'a = None | Some of 'a
// Main.tz
let x = Option.Some 1
match x with | Option.Some n -> n | Option.None -> 0
```

`Option.Some` は `Module.Case`。
current module に `union Option = Some` もあり、同時に `Option` module も見える曖昧ケースは `E1004` で `Module.Union.Case` を要求する。

Determinism:

```rust
assert_eq!(llvm::emit(&module, llvm::Entry::Library).unwrap(),
           llvm::emit(&module, llvm::Entry::Library).unwrap());
```

### E2E fixture

`tests/fixtures/unions/Main.tz`:

```text
union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

union Maybe 'a = None | Some of 'a
union Color = Red | Green | Blue

def area :: Shape -> f64
fn area shape =
    match shape with
    | Circle r -> r * r * 3.141592653589793
    | Rect (w, h) -> w * h
    | Empty -> 0.0

def default_value :: 'a -> Maybe 'a -> 'a
fn default_value fallback value =
    match value with
    | Some x -> x
    | None -> fallback

def color_code :: Color -> i64
fn color_code color =
    match color with
    | Red -> 1
    | Green -> 2
    | Blue -> 3

export def shape_area_scaled :: i64
fn shape_area_scaled =
    let a = area (Circle 2.0)
    let b = area (Rect (3.0, 4.0))
    to_int ((a + b) * 1000.0)

export def maybe_number :: i64
fn maybe_number =
    default_value 10 (Some 42)

export def maybe_string_length :: i64
fn maybe_string_length =
    let value = default_value "fallback" (Some "owned")
    value.length

export def color_sum :: i64
fn color_sum =
    color_code Red * 100 + color_code Green * 10 + color_code Blue
```

独立期待値:

- `shape_area_scaled`: `(π * 2^2 + 3 * 4) * 1000` を JS `Math.PI` ではなく fixture と同じ十進リテラル `3.141592653589793` で計算し、`to_int` のゼロ方向丸めを使う。期待値は `24566`。
- `maybe_number`: `42`
- `maybe_string_length`: `"owned"` の UTF-8 byte length = `5`
- `color_sum`: `1*100 + 2*10 + 3 = 123`

E2E:

- native `-O0`, `-O3`
- wasm32 `-O0`, `-O3`
- WASM imports empty
- IR deterministic
- native heap tracking `live == 0`
- string payload を含む `Maybe string` の clone/drop を 40,000 回繰り返して leak / double free を検査する。
- Node block の直前に毎回 `cargo build --release --locked` を実行する。

### 既存テスト

最低限:

```sh
cargo test --locked --test union_types
cargo test --locked --test control
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
cargo test --locked --test call_specialization
cargo test --locked --test tasks
# 各コマンドの `running N tests` が 0 でないことを記録する
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
cargo build --release --locked
node tests/computations.mjs target/release/tsuzuri
cargo build --release --locked
node tests/tasks.mjs target/release/tsuzuri
cargo build --release --locked
node tests/examples.mjs target/release/tsuzuri
```

## ドキュメント

- `docs/language.md`
  - 型表に union を追加。
  - `union` 宣言構文、case constructor、case pattern、payload は 0/1 個、`of` 文脈キーワードを記載。
  - `==` / `Display` は builtin ではないことを明記。
  - `E1024` の用途に union case 不正を追加。
  - `.tz` / `.tt` / `.tc` の許可表を更新し、`.tc` に union を許可する。
- `docs/architecture.md`
  - `Type::Union`, `CheckedUnion`, LLVM layout、drop/clone switch、tag switch lowering の不変条件を追加。
  - A04 まで recursive union は `E1010` と記載。
- `README.md`
  - 代数的データ型が実装済みであること、Option/Result は B01 まで標準同梱されないことを明記。

## 受け入れ条件

- [ ] `union` が予約語になり、`.tz` / `.tc` で宣言でき、`.tt` では `E1018`。
- [ ] `CheckedModule.unions` と `Type::Union(usize, Vec<Type>)` がある。
- [ ] generic union が A01 の型適用と同じ経路で推論・単相化される。
- [ ] payload なし case は値、payload あり case は一引数関数値として使える。
- [ ] direct full constructor application は `Construct` に lower され、余分な call を出さない。
- [ ] constructor を高階関数へ渡せる。残った `CaseConstructor` は specialization 後に `(union_id, case_id, concrete_args)` で dedup された hidden monomorphic function へ `closures::lower` で置換される。
- [ ] nullary case pattern が変数束縛ではなく case として解決される。
- [ ] union match が `SwitchPlan` により tag switch に lower され、payload binding は `UnionPayload` projection を使う。
- [ ] payload move / drop / clone / zeroing が string, array, list, closure で leak / double free しない。
- [ ] `UnionPayload` は ownership と LLVM の place/read_place/read_places に参加し、payload move は source union 全体への後続 access と正しく overlap する。
- [ ] union layout size は nullary 8、common payload `16 + round_up_16(payload)`, general `16 + 16*K` を `layout_size` / `validate_size` / `llvm_frame::stack_size` で共有し、64 KiB boundary tests が native/wasm32 で通る。
- [ ] recursive union は A04 まで `E1010`。
- [ ] `==` / `Ord` / Display は builtin ではない。D-20/A11 後も union 比較には `Eq` / `Ord` instance が必要で、A07 deriving がそれを提供する。
- [ ] native/WASM × `-O0`/`-O3`、WASM imports empty、heap tracking `live == 0` を確認した。

## 落とし穴

- `of` を通常キーワードにすると既存ユーザーコードの識別子 `of` を壊す。
  台帳 D-15 にないため文脈キーワードにする。
- `Parser::qualified_ident` は現状 1 dot まで。
  `Module.Union.Case` を処理するには別 helper が必要。
- nullary uppercase pattern を `Binding` のままにすると `None` が常に新しい変数になり、Option が壊れる。
- hidden constructor function だけで実装すると direct constructor に余分な call が残り、A02 の性能目標を満たさない。
- hidden constructor function を source recursion check 前に追加すると、ユーザーが書いていない `$case...` が recursion diagnostics に出る。`closures::lower` で specialization 後に追加する。
- payload storage を `[K x i64]` にすると 16-byte alignment が必要な payload で不十分になる可能性がある。
  既定は `[K x i128]`。
- `switch_cases` の既存 fast path は binding を subject local へ alias する。
  payload binding では `UnionPayload` projection pointer を使うよう拡張しないと誤った値を読む。
- `Frame::Aggregate` は case ごとに payload 型が変わる union を表せない。A02 初期実装では `Construct` を frame path から外す。
- union の `needs_drop` を tag switch なしに全 payload 型へ実行すると未初期化 payload を drop する。
- payload を move した後に union 全体を drop する場合、payload slot を zero にしないと二重解放する。
- `Eq` / `Ord` を builtin にしてしまうと D-20/A11 と A07 の deriving 設計に衝突する。

## 対象外

- 再帰的 union / record の許可（A04）。
- `Option` / `Result` 標準モジュール（B01）。
- `match` 網羅性検査（A03）。
- `deriving Eq/Ord/Display/Hash/Default`（A07）。
- 複数 payload を case が直接持つ構文。
- GADT、存在型、row polymorphism、open union。
- case にフィールド名を持たせる構文。
- `of` の通常予約語化。

## 未決事項

- case 名と同一モジュール関数名の衝突を拒否する方針で書いている。
  代替は値式で関数を優先、pattern で case を優先することだが、`Module.Case` が曖昧になるため既定案は宣言時 `E1001`。
- 一般 union payload storage は常に `[K x i128]` とする。
  台帳 D-05 の `{ i32, [K x i64|i128] }` のうち安全側を選ぶ。
  実装後に IR サイズが大きすぎる場合だけ、payload alignment を計算して `[K x i64]` を選ぶ最適化を別 PR にする。
- `CaseConstructor` を typed kind として残すか、hidden function だけで表すか。
  既定案は direct full application 最適化のため `CaseConstructor` を追加し、closures lowering 前に残ったものだけ hidden function に変換する。

### 実装時の判断（A02）

- 上の 3 点は既定案どおり実装した。case と同一モジュールの関数・アクティブパターン・自身の union 名との衝突は `E1001`
  （同一モジュールの名前空間の表に従い、`union Pair = Pair of …` も拒否する）。
- 一般形の `K` は、例の `Shape` が `[1 x i128]` になるよう、payload の実 LLVM storage size（64-bit、`i128` は 16 bytes 境界）から
  `ceil(size / 16)` で求める。保守的な layout size から求めると `Rect of f64 * f64` が `K = 2` になり、例と矛盾するため。
  64 KiB 判定と `stack_size` は `16 + round_up_16(conservative upper bound)` のままなので、見積もりは常に実サイズ以上になる。
- ownership の place 経路は payload を番兵 `PAYLOAD = usize::MAX - 1` で表し、配列要素の `ELEMENT = usize::MAX` と区別する。
  payload を move した union は payload slot を 0 で埋めるため、後続の union 全体の drop は `free(null)` の no-op になる。
- `SwitchPlan` は tag を 1 回 load して `switch i32` へ下げる。OR パターンの各候補で payload 以下の Field 経路が同じ場合も switch に含める。
  guard と参照越し（`ref (Maybe i64)`）の match は既存の順次テストへ fallback し、非網羅の default は A03 まで `llvm.trap`。
- match は subject slot から tag と payload を GEP + load で読むため、テスト計画の `extractvalue %"tz.union.Main.Shape"` は生成しない。
  IR テストは構築の `insertvalue` と、slot からの `load i32`／payload GEP を検査する。
- テスト計画の `Option.tc` 例は、`.tc` がビルダー操作を必須とするため `Option.tz` で検証した。`.tc` 内の union は別の `Maybe.tc` ビルダーで検証する。
- E2E は `tests/unions.mjs` ではなく、既存の `tests/features.mjs` の `unions` suite（`tests/fixtures/unions/`）に置いた。
- 再帰 union の診断文は `recursive union layout for '{name}'; recursive heap types are not supported in 0.1`（`E1010`）。
  型引数を決められない `let x = Some` は `E1015`。パターンの未知名の診断文は `unknown union case or active pattern '{name}'` に変更した。
- 4095 フィールドの payload は native・wasm32 とも build できるが、V8 は `-O0` の WASM で引数が 1000 を超える関数の instantiate を拒否する。
  同じフィールド数のレコードでも起きる既存の ABI 制約で、union 固有ではない。
- nullary enum の conservative size 8 は、集約の各メンバーが 16 bytes に丸められるため外部から観測できない。
- 台帳の見直し提案はない。
