# A06: 型クラスの拡張（条件付きインスタンス・スーパークラス・デフォルトメソッド）

| 項目 | 内容 |
|---|---|
| ID | A06 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | A11, (A01) |
| 後続 | A07, C06, C07, A10 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/computation.rs`, `src/recursion.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/polymorphism.rs`, `tests/modules.rs`, `tests/fixtures/*` |

## 目的

現在の「クラス 1 型引数 + 具体型インスタンス」だけでは、`Eq (Box 'a)`、`Eq (Option 'a)`、`Ord 'a => ...` のような標準ライブラリに不可欠な制約を表せない。このチケットで次を追加する。

1. 条件付き/ジェネリックインスタンス: `instance Eq 'a => Eq (Option 'a) { ... }`
2. スーパークラス: `class Eq 'a => Ord 'a { ... }`
3. デフォルトメソッド: class block 内に `fn`/`let` 本体を持てる
4. 配列・リスト・タプルの組み込み条件付き `Eq`/`Ord`

実行時辞書は導入しない。既存どおり単相化でメソッド呼び出しを具体関数または intrinsic に解決する。

## 現状

- `src/syntax.rs` の `ClassDecl` は `name`, `variable`, `methods: Vec<SignatureDecl>` だけを持つ。superclass と default body はない。
- `src/syntax.rs` の `InstanceDecl` は `class`, `ty`, `methods` だけを持つ。instance context はない。
- `src/parser.rs` の `Parser::program` の class 分岐は `class Name 'a { def ... }` だけを受理し、class block 内で `fn`/`let` を読まない。
- `src/parser.rs` の instance 分岐は `let class = qualified_ident(); let ty = type_atom();` で、`Eq (Option 'a)` や `Eq ['a]` 以外の複雑な型は A01 前に読めない。context prefix もない。
- `src/polymorph.rs` の `Classes` は `implementations: BTreeMap<(usize, Type), Vec<usize>>` で、key は具体 `Type`。`Classes::instances` は `require_concrete(&ty, instance.ty.span)?` により `instance C 'a` を `E1015` で拒否する。
- `Classes::validate` は制約が具体型なら `intrinsic` または `implementations.contains_key` で確認し、型変数を含む制約は後段へ伝播する。
- `Specializer::lower` は `TypedExprKind::Method(class, method, ty)` を `classes.implementations.get(&(class, ty.clone()))` または intrinsic wrapper に置換する。条件付き instance の探索はない。
- `src/computation.rs` の `collect` は `.tt` 内に `program.functions.first()` があれば `E1018`。default method body を `.tt` に書けない。
- `src/recursion.rs` の `references` は `Classes::recursion_targets` を使い、抽象型のメソッド呼び出しでは既存実装を保守的に候補にする。

## 仕様

### 構文

```text
class-declaration ::=
    "class" constraint-prefix? class-name type-variable "{" class-member* "}"

constraint-prefix ::=
    constraint "=>"
  | "(" constraint ("," constraint)* ")" "=>"

constraint ::= class-name type

class-member ::=
    "def" method-name "::" type
  | "fn" method-name parameter* "=" expression
  | "let" method-name "=" lambda-expression

instance-declaration ::=
    "instance" constraint-prefix? class-name type "{" instance-member* "}"

instance-member ::=
    "fn" "rec"? method-name parameter* ("=" expression | guarded-body)
  | "let" method-name "=" lambda-expression
```

例:

```text
class Eq 'a => Ord 'a {
    def lt :: &'a -> &'a -> bool
    def le :: &'a -> &'a -> bool
    fn le x y = Ord.lt x y || Eq.eq x y
}

record Box 'a { value: 'a }

instance Eq 'a => Eq (Box 'a) {
    fn eq left right = Eq.eq (&left.value) (&right.value)
    fn ne left right = !(Eq.eq left right)
}

instance Eq 'a => Eq (Option 'a) {
    fn eq left right =
        match left with
        | None ->
            match right with
            | None -> true
            | _ -> false
        | Some x ->
            match right with
            | Some y -> Eq.eq (&x) (&y)
            | _ -> false
    fn ne left right = !(Eq.eq left right)
}
```

A01 後は `Option 'a`、`Result 'a 'e`、`Pair 'a 'b` などの型適用を使う。A01 前にこのチケットを実装する場合、配列・リスト・タプルの組み込み条件付き intrinsic の obligation 伝播と非ジェネリックレコード instance に限定してもよいが、構文・データ構造は A01 と矛盾しない形にする。user generic instance（`Box 'a`、`Option 'a`）は A01 後に有効化する。

### スーパークラス

- `class Eq 'a => Ord 'a` は「`Ord T` が成り立つなら `Eq T` も成り立つ」を意味する。
- 関数が `Ord 'a` を要求すると、`Eq 'a` は暗黙に entail される。`def f :: Ord 'a => ...` の利用箇所で `Eq 'a` を別途書く必要はない。
- `instance Ord Point { ... }` は同じ `Point` について `Eq Point` が解決できなければ `E1027`。
- superclass cycle は `E1027`:
  ```text
  class B 'a => A 'a { def a :: 'a -> i64 }
  class A 'a => B 'a { def b :: 'a -> i64 }
  ```
- 組み込み `Ord` は `Eq` を superclass とする。数値型は既存の組み込み `Eq`/`Ord` の両方を持つため受理される。

### 条件付きインスタンス

- instance head は型変数を含められる。head に現れる型変数は instance context で制約できる。
- context にだけ現れ、head に現れない型変数は `E1027`。実行時辞書なしで解決できないため。
- 同一クラスで head が単一化可能な user instance は overlap として `E1016`。例:
  ```text
  instance Eq 'a => Eq (Box 'a) { ... }
  instance Eq (Box i64) { ... }        // E1016
  ```
- 組み込み intrinsic と重なる user instance も `E1016`。`instance Eq i64`、`instance Ord f64` に加え、配列・リスト・タプルの条件付き intrinsic と重なる `instance Eq 'a => Eq ['a]`、`instance Ord 'a => Ord [|'a|]`、`instance Eq ('a * 'b)` も禁止する。
- 解決は「具体 constraint `C T` に対し、intrinsic → user concrete/generic instance のうち一意」の順。intrinsic と user が同時に一致すれば user を拒否済みなので曖昧にならない。
- instance context は呼び出し元の constraint へ伝播する。user instance `Eq 'a => Eq (Box 'a)` のもとで `Box 'a` を比較する多相関数は `Eq 'a` を要求する。組み込み `Eq ['a]`/`Ord ['a]` も同じく element obligation を生成する。
- 探索深さ、生成 constraint 数、候補数に上限を設け、超過時は `E1017`。既定上限:
  - instance 解決深さ: 64
  - 1 制約から生成される追加制約: 128
  - overlap 検査で展開するペア: 1024

### デフォルトメソッド

- class block 内の `fn`/`let` は、その直前または同じ class 内の `def` と同じ名前を持つ method の default body。
- default body も `.tt` 内に書く。`.tt` に置ける関数本体は **class default method body だけ**で、トップレベル `def`/`fn` は引き続き禁止。
- instance が method を省略し、class に default があれば、その instance 用に合成メソッド関数を生成する。
- default body は class parameter を抽象型として検査し、class context と superclass context を利用できる。
- class method ごとに **1 つの generic default-method template を無条件に生成して検査する**。instance が 0 個でも不正な default body は拒否する。各 instance で default を使う場合は、この検証済み template を参照または clone し、元の `.tt` span を診断に保つ。
- default body が再帰する場合、class 内の `fn rec method ...` を要求する。`recursion.rs` は合成された `$instance` 関数を通常通り検査する。
- default がない method を instance が省略したら従来通り `E1016`。

### 組み込み条件付き Eq/Ord

このチケットで compiler intrinsic として導入する。A07 はレコード/union の deriving を担当し、配列・リスト・タプルの組み込み構造比較はここが担当する。利用者が同じ head の instance を宣言した場合は、条件付き intrinsic との overlap として `E1016`。

| 型 | `Eq` | `Ord` |
|---|---|---|
| `T * U * ...` | 全要素が `Eq` ならフィールド順に `&&` | 全要素が `Ord` なら辞書式 |
| `[T]` | 長さ一致後、添字昇順に要素 `Eq` | 辞書式。短い prefix は短い方が小さい |
| `[|T|]` | 長さ一致後、ノード順に要素 `Eq` | 辞書式 |

浮動小数点要素の意味は既存の `==`/`<` と同じ。NaN は `Eq` false、`Ord` 比較 false、`-0 == +0`。順序は総順序ではないが、既存 `Ord` の意味を変えない。

リストの `Eq`/`Ord` lowering は、`list[index]` を繰り返す O(n²) ではなく、左右の node pointer を lockstep で 1 回だけ前進させる内部ループにする。各反復で左右 node の payload を借用して A11 の `Eq`/`Ord` method に渡し、次 pointer を読む。IR テストで、リスト比較 body に先頭からの再走査や `list[index]` 相当の helper 呼び出しがないことを確認する。

## 設計

### データ構造

`src/syntax.rs`:

```rust
pub struct ClassDecl {
    pub name: Ident,
    pub variable: Ident,
    pub superclasses: Vec<ConstraintExpr>,
    pub methods: Vec<ClassMethodDecl>,
}

#[derive(Debug)]
pub struct ClassMethodDecl {
    pub signature: SignatureDecl,
    pub default: Option<Definition>,
}

pub struct InstanceDecl {
    pub constraints: Vec<ConstraintExpr>,
    pub class: Ident,
    pub ty: TypeExpr,
    pub methods: Vec<Definition>,
}
```

`src/polymorph.rs`:

```rust
#[derive(Clone, Debug)]
struct InstanceTemplate {
    id: usize,
    class: usize,
    head: Type,
    variables: Vec<String>,
    constraints: Vec<Constraint>,
    methods: Vec<usize>,
    span: Span,
}

struct Class {
    name: String,
    variable: String,
    superclasses: Vec<Constraint>,
    methods: Vec<Method>,
    builtin: bool,
}

pub(super) struct Classes {
    declarations: Vec<Class>,
    names: BTreeMap<String, usize>,
    implementations: BTreeMap<(usize, Type), Vec<usize>>, // 既存 concrete cache
    templates: Vec<InstanceTemplate>,                     // 新規 generic/concrete source
}
```

`implementations` は単相化中の concrete cache として残す。`templates` は overlap 検査・制約伝播・解決に使う。

### インスタンス解決

追加する内部 API:

```rust
struct InstanceResolution {
    methods: Vec<usize>,
    obligations: Vec<Constraint>,
    method_type_arguments: Vec<Type>,
}

impl Classes {
    fn resolve_instance(
        &self,
        class: usize,
        ty: &Type,
        records: &[CheckedRecord],
        span: Span,
    ) -> Result<InstanceResolution, Diagnostic>;
}
```

解決手順:

1. `head` と要求型 `ty` を kind/type-aware に単一化する。`Type::Variable` は instance-local pattern 変数として扱い、単一化結果を template の `variables` 順に `method_type_arguments: Vec<Type>` として正規化する。
2. `ty` が型変数を含む場合でも、**一意に一致する template head** があれば abstract obligation normalization として context を置換し、残余 obligations を返す。例: `Eq ['a]` は組み込み配列 intrinsic に一意に一致し、`Eq 'a` を残す。`Eq (Box 'a)` は user template `Eq x => Eq (Box x)` に一致し、`Eq 'a` を残す。
3. concrete method selection は、要求 head が concrete になってからだけ method id と `method_type_arguments` を使って行う。型変数を含む段階では method を選ばず、obligations だけ正規化する。
4. concrete `ty` なら `intrinsic_conditional(class, ty)` を試す。成功時は合成 intrinsic method ID または `IntrinsicKind`、obligations、空または正規化済み `method_type_arguments` を返す。
5. user `templates` の一致候補が 0 なら `E1005`（通常の no instance）または superclass/instance 宣言中なら `E1027`。
6. 一致候補が 2 以上なら、実装登録時 overlap を取り逃がしているので `E1016`。
7. template の context を単一化結果で置換した obligations を返す。
8. obligations は `Classes::validate`/`Specializer::instantiate` の既存 constraint 伝播に追加する。

overlap 検査では全 user template ペアを class ごとに比較し、head 同士が単一化可能なら `E1016`。constraint の強弱で overlap を許す機能は入れない。

### 単相化との接続

- `Checker::method` は従来通り `TypedExprKind::Method(class, method, ty)` と constraint を生成する。
- `Checker::finish` は concrete constraint なら `resolve_instance` で obligations を得て、その obligations も validate する。型変数を含む obligations は関数 constraint に保持する。
- `polymorph::specialize` の constraint fixed point で、callee から inherited constraints を追加する際にも `resolve_instance` 由来 obligations を展開する。
- `Specializer::lower` の `Method(class, method, ty)` は `resolve_instance` を呼び、user method なら `FunctionRef::User(request(method_id, method_type_arguments, span))`、intrinsic conditional method なら `intrinsic_function` と同じ合成 wrapper を生成する。
- 演算子 lowering (`operator_call`) も `resolve_instance` を使う。現在のように `classes.implementations[&(class, ty)]` を直接 index し、`request(id, Vec::new(), span)` を呼ぶ実装を廃止する。
- method call の callee type は、選択した template method の `Scheme.signature` に `method_type_arguments` を `template.variables` 順で substitute したものを使う。これを省くと `Box i64` 用 method を `Box 'a` のまま call して LLVM 前に抽象型が残る。
- generic method template の cache key は、template id と正規化済み型引数の組 `(template_id, type_args)` にする。`Type::display` 文字列ではなく `Type` の構造値を BTreeMap key に使う。

### default method 合成

`Classes::instances` の method 関数生成時:

1. `Classes::collect` 後、class の各 default method について generic template 関数を 1 つ合成し、class parameter と superclass/class self constraint を持つ `Scheme` として通常の型検査へ入れる。未使用 default も検査される。
2. class の各 method について、instance に明示 definition があるか探す。
3. 明示 definition がなければ、検証済み default template の function id と type argument substitution を instance method として参照する。body を再構築する場合も `.tt` の元 Span を保つ。
4. default もなければ `E1016 missing method`。
5. method signature は class parameter を instance head へ置換したもの。
6. 生成名は既存規則を拡張し、明示 method は `$instance.<function_id>.<method>` を維持する。default 由来 method は default template の特殊化として扱い、必要なら wrapper 名だけ `$instance` にする。
7. instance 側省略で型エラーが出る場合は default body の span、missing superclass は instance head span。

### スーパークラス entailment

`Class.superclasses` は class variable を含む `Constraint` として保持する。`Ord T` を validate するとき:

1. `Ord T` の instance/intrinsic が存在することを確認。
2. `Ord` の `superclasses` を `T` で置換し、`Eq T` を obligations に追加。
3. obligations を再帰的に validate。

cycle 検出は `(class, Type pattern)` の訪問 stack で行い、class declaration 時にも superclass graph を検査する。cycle は `E1027`。

### 変更ステージ

| ステージ | 変更 |
|---|---|
| lexer | 新キーワードなし。変更なし |
| parser | `Parser::signature` の constraint prefix helper を class/instance でも再利用。class block で `def` と default `fn`/`let` を読む。instance head は `type_expr` を読む |
| syntax | `ClassMethodDecl`, `ClassDecl.superclasses`, `InstanceDecl.constraints` |
| computation::collect | `.tt` の `program.functions` 禁止は維持。ただし class default body は `program.classes` 内に入るため許可。トップレベル関数は引き続き `E1018` |
| check::check_modules | class/instance の制約を resolve。superclass と instance context の型変数スコープ検査 |
| polymorph | `Classes` を template/解決型へ拡張。`validate`, `instances`, `recursion_targets`, `Specializer::lower` を更新 |
| control | 変更なし |
| closures | 合成 default method も通常関数なので変更なし |
| recursion | `Classes::recursion_targets` が generic templates と default methods を候補に含める |
| ownership | 変更なし。合成/既定 method body は通常の関数として検査 |
| call_specialization | 変更なし |
| llvm | 条件付き intrinsic Eq/Ord の wrapper 生成を `emit_builtin` ではなく通常の合成 `CheckedFunction` または `Specializer::intrinsic_function` で処理。辞書引数はなし |
| llvm_control / llvm_frame | 配列/リスト/タプル比較が通常のループ/フィールドアクセスへ下がるだけ。switch lowering は変更なし |
| runtime | 変更なし |
| driver / main | 変更なし |

### 前提とする他チケットのインターフェース

- A01: `TypeExprKind::Apply` と `Type::Record(usize, Vec<Type>)`。`instance Eq (Pair 'a 'b)` を読むために必要。
- A11: `Eq`/`Ord` の method signature は `&'a -> &'a -> bool`。条件付き `Eq`/`Ord` instance と default method は比較対象を消費しない前提で生成する。
- A02: `Type::Union(usize, Vec<Type>)` は A07 deriving の後続で使う。このチケットの解決器は union も `map_type`/単一化対象にできる形で実装する。
- A05: 型別名は `resolve_type` で展開済み。instance overlap は展開後 Type で見る。

### 他チケットへの提供インターフェース

- A07 は `instance Eq 'a => Eq (MyRecord 'a)` のような条件付き synthesized instance を `Program.instances` 互換の形で追加できる。
- C06 は `Ord 'k`/`Hash 'k`/`Eq 'k` の制約を Map/Set に要求できる。
- C04/C06 は A11 とこのチケットを前提に、比較のためだけに `Copy` を要求しない。
- C07 は `Iterator`/`Iterable` 風の型クラスを条件付き instance で表現できる。
- A10 は `Classes` の instance template と overlap/unification 基盤を高階型へ拡張する。

## 実装手順

1. **AST と parser**
   - `ClassMethodDecl`, `ClassDecl.superclasses`, `InstanceDecl.constraints` を追加。
   - `class Eq 'a => Ord 'a` と `instance Eq 'a => Eq (Box 'a)` を parse する。
   - class block 内 default `fn`/`let` を method 名に結び付ける。未知 method default は parser ではなく `Classes::collect` で `E1016`。
   - 確認: parse-only テストで AST の constraint 数と default 有無を照合。
2. **class 収集と superclass**
   - `Classes::collect` を新 AST に合わせる。
   - 組み込み `Ord` に `Eq 'a` superclass を追加。
   - superclass の未知 class、head 変数外の型変数、cycle を `E1027`。
   - 確認: `class A 'a => B 'a` などの拒否テスト。
3. **instance template と overlap**
   - `Classes::instances` で `require_concrete` を廃止し、`variables(head)` を instance-local として保持。
   - context 変数が head に含まれるか検査。
   - user template 同士、user template と intrinsic concrete の overlap を検査。
   - 確認: `instance Eq (Box 'a)` と `instance Eq (Box i64)` が `E1016`。また、`instance Eq 'a => Eq ['a]` は組み込み配列 intrinsic との overlap として単独でも `E1016`。
4. **制約解決と伝播**
   - `resolve_instance` を実装し、`Classes::validate`, `Checker::finish`, `specialize` fixed point へ obligations を統合。abstract obligation normalization と concrete method selection を分ける。
   - 解決深さ/obligation 数上限を `E1017`。
   - 確認: `def f :: ['a] -> ['a] -> bool` の `xs == ys` から `Eq 'a` が残余 obligation として推論される。`Box 'a` の user instance でも同様。
5. **default method 合成**
   - class method default から generic template を無条件に生成して検査する。
   - instance が default を使う場合は検証済み template を参照/clone し、型引数を渡して特殊化する。
   - 確認: `class C 'a { def f :: 'a -> i64; fn f x = 1 } instance C bool {}` が通る。
6. **組み込み条件付き Eq/Ord**
   - タプル/配列/リストの Eq/Ord を intrinsic conditional instance として解決器に追加。
   - LLVM 生成は通常の合成関数を Specializer が作る。配列は連続添字ループ、リストは左右 node pointer の lockstep 単一 traversal、タプルはフィールド順。
   - 確認: `[1,2] == [1,2]`, `[1,2] < [1,3]`, `(1,"a") == ...`。
   - IR 確認: リスト Eq/Ord に `list[index]` 相当の先頭再走査がなく、左右 node pointer phi が 1 つずつ前進する。
7. **回帰・文書**
   - `tests/polymorphism.rs` に条件付き instance、superclass、default、上限の網羅テスト。
   - Node E2E fixture で native/WASM × `-O0`/`-O3`。

## テスト計画

### 受理プログラム

```text
class Eq 'a => Total 'a {
    def same :: &'a -> &'a -> bool
    fn same x y = Eq.eq x y
}

instance Total i64 {}

def main :: bool
fn main = {
    let left = 20;
    let right = 20;
    Total.same (&left) (&right)
}
```

```text
class ShowSize 'a {
    def size :: &'a -> i64
    def empty :: 'a -> bool
    fn empty value = ShowSize.size (&value) == 0
}

instance ShowSize string {
    fn size text = text.length
}
```

```text
record Box 'a { value: 'a }

instance Eq 'a => Eq (Box 'a) {
    fn eq left right = Eq.eq (&left.value) (&right.value)
    fn ne left right = !(Eq.eq left right)
}

def same_box :: Eq 'a => &(Box 'a) -> &(Box 'a) -> bool
fn same_box a b = Eq.eq a b
```

### 拒否プログラム

| プログラム | 期待 |
|---|---|
| `class Missing 'a => C 'a { def f :: 'a -> i64 }` | `E1027` |
| superclass cycle | `E1027` |
| `instance Ord Point { ... }` で `Eq Point` がない | `E1027` |
| `instance Eq 'b => Eq (Box i64) { ... }` | `E1027` |
| `instance Eq 'a => Eq (Box 'a)` と `instance Eq (Box i64)` | `E1016` |
| `instance Eq 'a => Eq ['a] { ... }` | `E1016`（組み込み配列 intrinsic と overlap） |
| `instance Ord 'a => Ord [|'a|] { ... }` | `E1016`（組み込みリスト intrinsic と overlap） |
| `instance Eq i64 { ... }` | `E1016` |
| default のない method 省略 | `E1016` |
| default body の型不一致 | `E1003` |
| instance 解決が深さ上限超過 | `E1017` |

### E2E

GUIDE §3 に従い、Node E2E の直前に必ず `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

- fixture `tests/fixtures/typeclasses`.
- exported scalar checksum:
  - `array_eq_i64([1,2,3], [1,2,3])` 相当を内部で作り、bool を `i32` checksum に変換。
  - list lexicographic compare: `[|1,2|] < [|1,2,0|]` は true。
  - default method: `le` が `lt || eq` で動く。
- JS 側期待値は手書き配列/リスト比較で計算。
- リスト比較の IR は左右 node pointer の lockstep loop を検査し、`list[index]` や先頭からの再走査を含まないことを確認する。
- native/WASM × `-O0`/`-O3`、IR 2 回一致、WASM imports 空、heap tracking `live == 0`。

## ドキュメント

- `docs/language.md`
  - 型クラス節に conditional instance、superclass、default method、overlap 規則を追加。
  - 組み込みクラス表で `Ord : Eq` と配列/リスト/タプルの conditional `Eq`/`Ord` を記載。
  - 診断表に `E1027`。
- `docs/architecture.md`
  - 多相性の不変条件に instance template、obligation 伝播、辞書なし単相化を追加。
  - 資源上限に instance 解決深さ/obligation 数を追加。
- `README.md`
  - 未対応一覧から条件付き instance/superclass/default method を削除。

## 受け入れ条件

- [ ] `instance Eq 'a => Eq (Box 'a)` と `instance Eq 'a => Eq (Option 'a)` を parse/typecheck できる。
- [ ] user `instance Eq 'a => Eq ['a]`、`instance Ord 'a => Ord [|'a|]`、tuple の user Eq/Ord instance は組み込み条件付き intrinsic と overlap して `E1016`。
- [ ] overlapping generic/concrete instance を `E1016` で拒否する。
- [ ] `resolve_instance` が `method_type_arguments` を返し、`Specializer::request(method_id, method_type_arguments, span)` で generic method template を特殊化する。
- [ ] abstract obligation normalization により `Eq ['a]` から `Eq 'a`、`Eq (Box 'a)` から `Eq 'a` が残余 obligation として伝播する。
- [ ] `Ord T` が `Eq T` を entail し、`Ord` instance 宣言時に `Eq` 不足を `E1027`。
- [ ] default method は class ごとに 1 つの generic template として無条件に検査され、instance では検証済み template を参照する。
- [ ] 配列/リスト/タプルの `Eq`/`Ord` が element constraint を伝播する。
- [ ] リスト `Eq`/`Ord` は O(n) の lockstep node traversal で、IR テストが先頭再走査を禁止する。
- [ ] 辞書・boxing・実行時型検査を生成しない。
- [ ] native/WASM × `-O0`/`-O3` の E2E、IR 決定性、heap `live == 0`。

## 落とし穴

- context を見て overlap を許すと coherence が壊れる。Tsuzuri では単一化可能な head は常に overlap。
- `Classes::validate` だけ直して `Specializer::lower` を直さないと、型検査は通るが LLVM で未解決 method が残る。
- default body を `.tt` の top-level function として `program.functions` に入れない。`.tt` の種別検査が壊れる。
- `Ord` superclass `Eq` を忘れると A07 の `deriving Ord` が `Eq` なしで受理される。
- generic instance の obligations を callee から caller へ伝播しないと、抽象本体は通るが特殊化時に突然 `E1005` になる。
- 配列/リスト比較で要素を move してはいけない。A11 の borrowed `Eq`/`Ord` method を使い、比較中は読み取り借用し、所有値の解放順を維持する。

## 対象外

- 複数型パラメーターを持つ型クラス。
- associated type / type family。
- overlapping instance の優先順位、orphan rule、negative instance。
- runtime dictionary、動的ディスパッチ、boxing。
- HKT。A10 で別途設計する。

## 未決事項

- **既定案: 条件付き Eq/Ord for arrays/lists/tuples は A06 で実装する。** A07 は record/union deriving に集中する。
- **既定案: `Ord` は `Eq` を superclass にする。** 既存の組み込み数値・文字列以外の `Ord` user instance は `Eq` も要求される。
- **既定案: instance context だけに現れる型変数は `E1027`。** 将来 HKT/関連型で必要になったら A10 以降で見直す。
