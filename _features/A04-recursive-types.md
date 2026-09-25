# A04: 再帰的なヒープ型（木・AST）

| 項目 | 内容 |
|---|---|
| ID | A04 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | A02 |
| 後続 | C06 |
| 状態 | todo |
| 主な影響ファイル | `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/llvm_frame.rs`, `src/call_specialization.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/*.rs`, `tests/*.mjs` |

## 目的

`Tree<'a>`、式 AST、linked structures のような再帰的データ構造を、所有権と明示的な heap allocation によって安全に扱えるようにする。
A02 までは recursive union / record を `E1010` で拒否するが、このチケットで union payload を通る再帰を許可する。

構文は透明に保ち、ユーザーは `Tree<'a>` と書く。
コンパイラ内部だけで再帰箇所を boxed heap cell に変換し、clone/drop は native stack に比例しない反復処理にする。

## 現状

- `src/check.rs` の `record_size` / `layout_size` は値レイアウト再帰を `E1010` で拒否する。
  `record A { b: B } record B { a: A }` と `record A { a: [A] }` は既存テストで `E1010`。
- A02 の union 実装後も、recursive union は A04 まで `E1010` とする設計。
- `Type` は A02 時点で `Record(usize, Vec<Type>)` と `Union(usize, Vec<Type>)` を持つ。
  ただし recursive occurrence を表す内部型はまだない。
- `src/llvm.rs` の `drop_value` / `clone_value` は record / tuple / array / list を再帰的に呼ぶ。
  リスト drop は `list_loop` で反復処理だが、record/union 再帰をそのまま許すと Rust 実装の再帰ではなく生成 IR の関数呼び出し再帰または emitter 再帰が深くなる。
- `src/runtime/heap-wasm.ll` の heap は 16 MiB 上限で、`@tz.alloc` は失敗時に trap する。
- `src/llvm_frame.rs` は stack storage の literal tree を 64 KiB までに制限し、ヒープへの移送 `relocate` を実装している。
- `src/ownership.rs` は structural Copy を型構造で推論する。
  再帰型をそのまま structural Copy にすると無限展開になる。

## 仕様

### ユーザー向け構文

A02 の union 構文をそのまま使う。

```text
union Tree<'a> =
    | Leaf
    | Node of Tree<'a> * 'a * Tree<'a>

union Expr =
    | Int of i64
    | Add of Expr * Expr
    | Let of string * Expr * Expr
```

records を union payload と組み合わせることもできる。

```text
record Branch<'a> { left: Tree<'a>, value: 'a, right: Tree<'a> }
union Tree<'a> = Leaf | Node of Branch<'a>
```

ユーザーは `box`, `new`, `ptr` のような追加構文を書かない。
再帰 occurrence は compiler が暗黙に boxed heap cell へ変換する。

### 許可する再帰

許可する:

- union が少なくとも 1 つの非再帰 case を持ち、再帰 occurrence が union payload から到達する。
- record が recursive SCC に含まれていても、その cycle が union payload を通り、finite value を作れる。
- generic 引数が同じ形で戻ってくる structural recursion。

例:

```text
union List<'a> = Nil | Cons of 'a * List<'a>
union Rose<'a> = Node of 'a * [Rose<'a>]
record PairNode<'a> { left: Tree<'a>, right: Tree<'a> }
union Tree<'a> = Empty | Pair of PairNode<'a>
```

拒否する:

- union を通らない recursive record。

```text
record Bad { next: Bad }
```

- 非再帰 alternative を持たない uninhabited recursive union。

```text
union Bad = Bad of Bad
```

- generic 引数が成長する polymorphic recursion。

```text
union Bad<'a> = Bad of Bad<Tree<'a>>
```

- 型引数置換が 128 depth / 4096 node 上限を超える再帰。

診断:

- recursive record without union alternative: `E1010`
- recursive union without finite base case: `E1010`
- generic argument growth: `E1017`

### 暗黙 boxing の規則

内部表現に `Type::Boxed(Box<Type>)` を追加するが、**`CheckedRecord.fields` / `CheckedUnion.cases` の generic declaration 自体は書き換えない**。
A01/A02 の D-03 canonical name は source form の型引数に依存するため、declaration に `Boxed` を挿入すると `Maybe` のような非再帰 instance まで誤って box したり、`Maybe<Boxed<Link>>` のような source に存在しない型名を mangle に混ぜたりする。
再帰 boxing は `TypeContext` 内の instance-sensitive `RecursiveLayout` で管理し、field/payload helper が concrete instance へ型引数を代入した後にだけ適用する。

```rust
pub enum Type {
    // A02 までの variant ...
    Record(usize, Vec<Type>),
    Union(usize, Vec<Type>),
    Boxed(Box<Type>),
    // ...
}

pub struct TypeContext<'a> {
    pub records: &'a [CheckedRecord],
    pub unions: &'a [CheckedUnion],
    pub recursive: &'a RecursiveLayout,
}

pub struct RecursiveLayout {
    pub boxed: BTreeSet<BoxedOccurrence>,
    pub recursive_instances: BTreeSet<NamedInstance>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NamedInstance {
    Record { id: usize, args: Vec<Type> },
    Union { id: usize, args: Vec<Type> },
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BoxedOccurrence {
    pub owner: NamedInstance,
    pub path: OccurrencePath,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OccurrenceStep {
    RecordField(usize),
    UnionCasePayload(usize),
    TupleField(usize),
    // Arrays/lists are storage barriers; they do not appear in boxed occurrence paths.
}
```

- `Boxed(T)` は source syntax には現れない。
- `Type::display` は `Boxed(T)` を `T` と同じように表示する。
  診断で `boxed Tree<i64>` のような内部名を出さない。
- `canonical_type_text` は通常は `Boxed` を含まない。
  ただし実装上 `Boxed` が named type の型引数内部に現れ得る場合は、D-03 の単射性を壊さないため `boxed[...]` marker を追加する。
  例: `Main.Wrap[boxed[Main.Tree[i64]]]`。
  これは source syntax には表示しない内部 canonical form であり、records/unions で同じ escaping 規則を使う。
- `Type::Boxed(T)` の LLVM 型は `ptr`。
- `Boxed(T)` は owned heap pointer。
- `Boxed(T)` は `needs_drop = true`。
- `Boxed(T)` は A04 の既定では **Copy ではない**。
  理由: recursive structure の Copy は深い clone を伴い、意図しない O(n) 複製を生みやすい。
  既存の array/list は要素 Copy なら Copy だが、recursive ADT は cycle-safe iterative clone/drop のコストが大きいため、明示的な clone API/deriving を A07 以降で検討する。
- property methods は「この表現そのものが borrow pointer か」と「owned value が推移的に借用を格納しているか」を分ける。
  `Boxed(T)` は前者では false だが、後者では `T` を調べる。
  そのため task result / validate_borrows / record-field reference ban / `can_send` は boxed inner の borrowed data を検出できる。

### どの occurrence を box するか

型宣言グラフは **storage-containment edge** だけで作る。

ノード:

```text
NamedDecl = Record(id) | Union(id)
InstanceShape = NamedDecl applied to its declared type parameters
```

エッジ:

- record field 型に現れる named type occurrence。
- union payload 型に現れる named type occurrence。
- tuple と inline record/union の内部は辿る。
- **function / task / reference は barrier**。これらの中の named type は value layout の inline containment ではない。
- **array / list は既に記述子 + heap storage の indirection なので barrier**。要素型へは graph edge を張らず、per-element boxing もしない。
  `union Rose<'a> = Rose of 'a * [Rose<'a>]` は array descriptor が inline payload であり、array elements は collection heap storage 内にある。
  drop/clone は動的 elements を stack-safe traversal で走査するが、declaration graph 上の recursive inline occurrence ではない。

box する occurrence:

- SCC に少なくとも 1 つ union があり、その union に再帰 cycle を含まない case が 1 つ以上ある場合、SCC 内の **inline storage occurrence** を box する。
- 決定性のため、declaration order に依存せず、既定は「SCC-internal inline occurrence をすべて box する」。
  代替の feedback-edge set を採る場合も、qualified name + occurrence path の `BTreeSet` 順で選び、allocation/trap order が宣言順に依存しないようにする。
- `BoxedOccurrence` は concrete named instance + occurrence path で key 付けする。
  `record_instance_fields` / `union_instance_cases` はまず型引数を代入し、その concrete owner/path が `RecursiveLayout.boxed` にあれば、その field/payload 型を `Type::Boxed(Box::new(substituted_type))` として返す。

box しない occurrence:

- 再帰 cycle に関与しない named type。
- `record Box<'a> { value: 'a }` の `'a`。
- function value の closure descriptor。
- array/list element 内の occurrence。
- function/task/reference 内の occurrence。

generic args:

- `Tree<'a>` から `Tree<'a>` に戻る inline occurrence は同一として box する。
- 型パラメーターの名前は α-renaming を許す。
- `Bad<'a>` から `Bad<Pair<'a, 'a>>` に戻る occurrence は型が成長するため `E1017`。
- `Nested<'a, 'b>` から `Nested<'b, 'a>` のような permutation は A04 では拒否してよい。
  メッセージは `"recursive generic type changes its arguments; use a non-growing recursive occurrence"`。

### 型検査での透明性

source では `Tree<'a>` と書く。
`record_instance_fields` / `union_instance_cases` が concrete owner + occurrence path に基づいて `Boxed(Tree<'a>)` を返す場合、型検査は expected type に基づき自動 box/unbox typed kind を挿入する。

追加 typed kind:

```rust
pub enum TypedExprKind {
    // ...
    BoxRecursive(Box<TypedExpr>),
    UnboxRecursive(Box<TypedExpr>),
}
```

- `BoxRecursive` の型は `Type::Boxed(inner)`.
- `UnboxRecursive` の型は `inner`.
- `Checker::expression` は expected が `Boxed(inner)` で、実式の型が `inner` の場合に `BoxRecursive` を挿入する。
- `Checker::expression` は expected が `inner` で、実式の型が `Boxed(inner)` の projection の場合に `UnboxRecursive` view を挿入する。
- pattern matching では `UnionPayload` や record field projection の型が `Boxed(inner)` のとき、source pattern は `inner` に対して書かれているとみなし、最初に `UnboxRecursive` projection を挟む。
- case constructor の source-facing function type は **unboxed** のままにする。
  例: `Node : Tree<'a> * 'a * Tree<'a> -> Tree<'a>`。
  hidden constructor body も同じ source-facing payload 型を受け取り、body 内で direct `Construct` と同じ occurrence-path boxing を行う。

例:

```text
let t = Node (Leaf, 1, Leaf)
```

内部:

```text
Construct Node (box payload occurrence paths [0, 2] in Tuple(Leaf, 1, Leaf))
```

### 評価順序

- direct `Construct` と hidden constructor body は同じ lowering を使う。
- payload expression 全体は既存規則どおり一度だけ、左から右に評価する。
- payload value が得られた後、`RecursiveLayout` の occurrence path を deterministic `BTreeSet` 順に辿り、必要な heap cell を allocate/store して payload value 内の該当 slot を boxed pointer に置き換える。
  この allocation order は source declaration order ではなく occurrence path order によって決まる。
- tuple payload の各要素は既存どおり左から右に評価する。
  したがって `Node (left(), value(), right())` は:
  1. `left()` 評価
  2. `value()` 評価
  3. `right()` 評価
  4. payload tuple を組み立てる
  5. occurrence path `0`（left）を allocate/store し payload slot を ptr に置換
  6. occurrence path `2`（right）を allocate/store し payload slot を ptr に置換
  7. union construct
- allocation failure は `@tz.alloc` trap。
  成功形の代替値を返してはならない。
- implicit boxing の allocation trap は言語上観測可能な trap なので、上記順序を deterministic に保つ。
- first-class constructor 経由でも同じ。
  `Node payload`、`apply Node payload`、`let make: Tree<i64> * i64 * Tree<i64> -> Tree<i64> = Node; make payload` は payload expression の評価回数・順序・allocation path order が同一でなければならない。

### pattern projection と move

```text
match tree with
| Leaf -> 0
| Node (left, value, right) -> ...
```

- tag check 後、payload を projection する。
- recursive occurrence は `UnionPayload` / record field projection の **owning pointer slot** を place として保持し、そこから box pointer を load して inner value を pattern に渡す。
- 非 Copy subtree を pattern binding として move する場合、親 union 値全体を消費する。
- move した box pointer slot は `ptr null` に zeroing し、後続 drop で二重解放しない。
- 共有借用 match の場合は box 内部を読み取り借用し、所有 subtree を move しない。
- `as` pattern で全体と subtree を同時に非 Copy move する場合は既存の move/borrow rules と同じく拒否または Copy 要求にする。

### Copy / clone / drop

既定:

- recursive ADT は非 Copy。
- nullary-only enum は A02 と同じく Copy。
- recursive occurrence を含まない union/record は A02/A01 の structural Copy。
- `Type::Boxed` は非 Copy。

Clone:

- recursive clone は必須。
  Tsuzuri の関数値は Copy であり、`src/llvm.rs` の `closure_wrappers` は環境 clone で捕捉 field ごとに `clone_value` を呼ぶ。
  tree を捕捉した lambda をコピーすると、2 つの closure environment が独立に deep-cloned tree を所有し、両方が解放されなければならない。
- `clone_value(Type::Boxed(inner))` と recursive named type の clone は native/WASM stack に比例してはならない。
- type-specific iterative clone helper を生成する。
  worklist item は `{ next, source_ptr, destination_ptr }`。
  pop 後すぐ work node を free し、source cell / destination cell の field を処理する。
  destination cell には children の destination boxes を先に allocate して placeholder を store し、後で worklist で埋める。
- allocation failure は `@tz.alloc` trap。
  trap 時の unwinding は保証しないが、正常終了時は source と clone が独立し、どちらを drop しても `live == 0`。

Drop:

- recursive structure の drop は native/WASM の実行 stack に比例して再帰してはならない。
- type-specific iterative drop helper を LLVM emitter が生成する。
- helper 名は deterministic:

```text
@tz.drop.rec.<mangled-type>
@tz.clone.rec.<mangled-type>
```

drop algorithm:

```text
drop_root(value):
  process value directly
  when a Boxed(inner) child is encountered:
      if pointer != null:
          push pointer onto intrusive worklist without allocating
  while worklist not empty:
      pointer = pop
      inner = load inner from pointer
      collect boxed child pointers and dynamic collection elements from inner
      drop non-recursive owned fields in inner
      store any intrusive next link / zero markers needed before free
      free pointer cell
      push collected recursive boxed children / collection elements
```

- Drop helper は allocation-free にする。
  既に free 予定の box cell 自体、または dynamic array/list storage の element slot を intrusive work item として使う。
  これにより、正常に構築済みの値が暗黙 drop 中の worklist allocation failure で trap することを避ける。
  intrusive link を書く場合は、cell の inner value から必要な child pointers / owned non-recursive fields を先に読み出してから、link を上書きし、free 後は cell を読まない。
- ただし正常終了時は全 heap cell と payload owned values を解放し、heap tracking `live == 0`。
- 既存 array/list `drop_value` / `clone_value` を、element 型が recursive boxes を含む場合にそのまま呼んではならない。
  `union Rose = Rose of [Rose]` のようなケースで array loop から element `drop_value(Rose)` を再帰呼び出しすると stack-safe ではない。
  recursive traversal は nested aggregates と dynamic array/list elements を走査し、element 内の boxed child pointers や collection element addresses を worklist に入れる 1 つの traversal として生成する。
  array/list descriptor 自体の buffer/node storage は、elements を処理し終えてから既存 allocator へ返す。

Clone algorithm:

```text
clone_root(value):
  clone root payload
  for each boxed child:
      allocate destination box
      store placeholder zero
      push (source_ptr, dest_ptr)
  while worklist not empty:
      load source inner
      clone non-recursive fields
      allocate destination boxes for recursive children
      store completed destination inner
```

- native stack に比例しない。
- allocation failure は trap。
- array/list element 型が recursive boxes を含む場合、clone traversal は dynamic elements を直接 scan し、element clone のために recursive `clone_value` を呼ばない。

### LLVM layout

- `Type::Boxed(T)` は `ptr`。
- box cell layout は inner LLVM type exactly:

```llvm
%cell = call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr (<inner-llvm-type>, ptr null, i32 1) to i64))
store <inner-llvm-type> %value, ptr %cell, align 16
```

- inner が zero-sized になることはない。
  unit-only enum でも tag `i32` を持つ。
- `layout_size(Boxed(T))` は pointer descriptor として 8 bytes conservative。
  wasm32 でも existing layout_size は pointer-sized values を 8 bytes 以上に見積もるため upper bound。
- union payload storage size は A02 の規則に `Boxed(T)` = pointer を含めて計算する。
- recursive occurrence を box した後の record/union concrete layout は有限でなければならない。

### WASM heap

- 既存 WASM heap limit は 16 MiB。
- A04 はこの上限を変更しない。
- recursive structures は per-node `@tz.alloc` を使うため、深い tree/list は heap limit に到達すると trap する。
- `heap-wasm.ll` の `@tz.alloc` は `%needed = (%size + 31) & -16` で block size を計算する。
  16-byte header と 16-byte alignment を含み、各 block は最小 32 bytes。
- テストは limit 内で正常系を確認し、limit 超過は trap として確認する。

### A03 網羅性との接続

A03 の coverage normalization は pattern boundary で `Boxed(T)` を消し、constructor lookup は inner `T` に対して行う。
`Boxed(Tree<i64>)` payload に対する pattern `Leaf | Node _` は `Tree<i64>` の constructor domain として扱う。

- coverage / missing witness 生成は recursive type を展開しすぎない。
  `visited: BTreeSet<Type>`、depth limit `MAX_NESTING`、node count 4,096 を持ち、再訪した recursive type は wildcard witness `_` として打ち切る。
- tests:
  - `Leaf | Node _` は `Tree` に対して exhaustive。
  - nested recursive pattern `Node (Leaf, _, _)` だけでは missing `Leaf` または `Node (Node _, _, _)`。
  - `Leaf | Node _ | Leaf` の最後は `W1003`。
  - boxed payload pattern の unreachable arm が raw `Boxed` ではなく source constructor 名で表示される。

## 設計

### 宣言グラフ検査

`check.rs` に `recursive_types` phase を追加する。
records/unions の `CheckedRecord` / `CheckedUnion` を作成し、型パラメーター検査後、layout validation 前に実行する。

出力:

```rust
pub struct RecursiveLayout {
    pub boxed_occurrences: BTreeSet<OccurrenceId>,
    pub recursive_types: BTreeSet<NamedTypeId>,
}

pub enum NamedTypeId {
    Record(usize),
    Union(usize),
}

pub struct OccurrenceId {
    pub owner: NamedTypeId,
    pub path: Vec<usize>,
}
```

`path` は owner の field/case/payload/tuple/list/array 内の occurrence 位置を deterministic に表す。
declaration `Type` 自体へ `Boxed` を書き込まない。`OccurrenceId` / `BoxedOccurrence` は `RecursiveLayout` の実データであり、`record_instance_fields` / `union_instance_cases` が concrete owner/path を見て boxing を適用する。

手順:

1. 各 named type 宣言を declared type parameters で instantiate した symbolic type を作る。
2. storage-containment edge だけを DFS する。tuple と inline record/union は辿り、function/task/reference/array/list は barrier。
3. named type occurrence が現在 stack 上の named type に戻る場合:
   - generic args が同一形ならその occurrence path を `RecursiveLayout.boxed` 候補にする。
   - args が成長・変形していれば `E1017`。
4. SCC ごとに union base case を検査する。
   - union case の payload 型を辿って同 SCC に戻らない case があれば base case。
   - base case がない SCC は `E1010`。
5. SCC に union がなく、record だけなら `E1010`。
6. declaration order ではなく qualified name + occurrence path の `BTreeSet` 順で `RecursiveLayout.boxed` を確定する。
7. `CheckedRecord.fields` / `CheckedUnion.cases` は source form のまま保存する。
8. `record_instance_fields` / `union_instance_cases` が型引数を代入した後、`RecursiveLayout.boxed` を参照して該当 occurrence だけ `Type::Boxed(Box::new(substituted_type))` にする。

### 型性質

`Type` property methods は `TypeContext` に memoization table を持たせ、concrete `Type` を key にして cycle-aware に計算する。
再訪した型はその property の neutral result を返す。

| property | neutral on revisit |
|---|---|
| `is_copy` | `true`（ただし `Boxed` は常に false なので recursive ADT は非 Copy） |
| `needs_drop` | `false` |
| `contains_borrow_pointer` | `false` |
| `stores_borrowed_data` / `carries_loans` | `false` |
| `contains_mutable_reference` | `false` |
| `can_capture` | `true` |
| `can_send` | `true` |

「この representation 自体が borrow pointer か」と「owned value が推移的に borrowed data を格納しているか」を分離する。
既存の `contains_reference` 名を残す場合は、record field ban / task result / validate_borrows にどちらを使うかを明示する。
推奨シグネチャ:

```rust
impl Type {
    pub fn contains_borrow_pointer(&self, types: &TypeContext<'_>) -> bool;
    pub fn stores_borrowed_data(&self, types: &TypeContext<'_>) -> bool;
    pub(crate) fn carries_loans(&self, types: &TypeContext<'_>) -> bool; // stores_borrowed_data と同じ意味
}
```

`Type` methods for `Boxed(inner)`:

| method | `Boxed(inner)` |
|---|---|
| `display` | `inner.display(records, unions)` |
| `is_copy` | `false` |
| `needs_drop` | `true` |
| `contains_borrow_pointer` | `false`。Box is owned pointer, not borrow |
| `stores_borrowed_data` / `carries_loans` | `inner.stores_borrowed_data(...)` |
| `contains_mutable_reference` | `inner.contains_mutable_reference(...)`。boxed payload に `&mut` を保存するのは既存 collection rule と同じく拒否対象 |
| `can_capture` | `inner.can_capture(...)` |
| `can_send` | `inner.can_send(...)` |
| `exportable` | `false` |

`contains_borrow_pointer` を false にするのは box pointer 自体を借用と誤認しないため。
一方、中身に reference を保存することは lifetime 上危険なので `stores_borrowed_data` / `carries_loans` / `can_send` / `validate_borrows` / record-field reference ban / `validate_size` では中身を見る。

### 型検査

- `Checker::expression` に expected/actual の transparent boxing coercion を追加する。
- `same(actual, expected)` は `Boxed(T)` と `T` を直接 unify しない。
  coercion は expression boundary でのみ挿入する。
  型システム内部で `Boxed(T) == T` にすると layout と ownership が壊れる。
- record field access:
  - 内部 field 型が `Boxed(T)` でも、source-level expected がない場合は field access の型を `T` として返し、`UnboxRecursive` を挿入する。
  - ただし place として借用する場合は box cell 内部への place を作る。
- union payload pattern:
  - `UnionPayload` が `Boxed(T)` なら pattern_alternatives は `UnboxRecursive(UnionPayload(...))` を matched value にする。
- constructor:
  - expected payload 型が `Boxed(T)` の位置で source expression `T` を `BoxRecursive` する。

### 所有権

- `BoxRecursive` は inner expression を consume し、結果は owned boxed value。
- `UnboxRecursive`:
  - read / borrow context では box cell 内部を borrow する。
  - consume context では **owning pointer slot の address** に対して操作する。
    slot から pointer を load し、cell から inner value を load し、slot に `ptr null` を store してから cell を free する。
    parent slot に dangling pointer を残してはならない。
  - 非 place temporary の `UnboxRecursive` は、temporary が所有する boxed pointer value を専用一時 slot に materialize してから同じ take path を通す。
- `Place` に `BoxDeref` 相当を表す field sentinel を追加するか、`E::UnboxRecursive(value)` を `place` で扱う。
  既存 `usize::MAX` は array/list conservative element loan に使われているため、別 sentinel enum への移行を推奨する。

推奨リファクタ:

```rust
enum PlaceComponent {
    Field(usize),
    UnionPayload(usize),
    CollectionElement,
    BoxValue,
}

struct Place {
    root: usize,
    fields: Vec<PlaceComponent>,
}
```

これにより record field `0`、collection element、boxed inner を混同しない。
`UnboxRecursive(UnionPayload(...))` は `PlaceComponent::UnionPayload(case_id)` の後に `BoxValue` を追加した exact projection path を持つ。
root union 全体への access は payload / boxed-inner path と overlap し、payload move 後の source union 使用を正しく `E1012` にする。
A02 の `SwitchPlan.bindings` は binding projection として `UnboxRecursive(UnionPayload(...))` を保持し、guard 成功後の `read_place(take = true)` が exact pointer slot を zeroing できるようにする。

### LLVM

追加 helper:

```rust
fn boxed_inner_pointer(&mut self, inner: &Type, boxed: &str) -> String;
fn box_value(&mut self, inner: &Type, value: &str) -> String;
fn unbox_place(&mut self, inner: &Type, boxed_slot: &str, take: bool) -> String;
```

`BoxRecursive`:

```llvm
%cell = call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr (<inner>, ptr null, i32 1) to i64))
store <inner> %value, ptr %cell, align 16
```

`UnboxRecursive` read:

```llvm
%cell = load ptr, ptr %owner.slot, align 8
%value = load <inner>, ptr %cell, align 16
; if result is owned copy, clone_value(inner, %value) as needed by expression_mode
```

`UnboxRecursive` take:

```llvm
%cell = load ptr, ptr %owner.slot, align 8
%value = load <inner>, ptr %cell, align 16
store <inner> zeroinitializer, ptr %cell, align 16
store ptr null, ptr %owner.slot, align 8
call void @tz.free(ptr %cell)
```

非 place temporary の boxed value を take する場合は entry block slot に `store ptr %boxed, ptr %temp.slot` してから上の `unbox_place` を使う。
これにより parent aggregate 内の pointer slot、union payload slot、temporary slot のいずれにも dangling pointer を残さない。

drop of `Boxed(inner)`:

```llvm
%is_null = icmp eq ptr %boxed, null
br i1 %is_null, label %exit, label %drop
drop:
  %inner = load <inner>, ptr %boxed, align 16
  call void @tz.drop.rec.<inner>(<inner> %inner) ; if recursive helper needed
  call void @tz.free(ptr %boxed)
```

Non-recursive inner can inline existing `drop_value`.
Recursive inner uses generated helper.

### Iterative drop/clone helper generation

`emit_target` already has `builtins`, `intrinsics`, `globals`, `specializations`.
Add:

```rust
recursive_drops: BTreeSet<Type>
recursive_drop_definitions: BTreeMap<Type, String>
recursive_clones: BTreeSet<Type>
recursive_clone_definitions: BTreeMap<Type, String>
```

When `drop_value` or `clone_value` sees a recursive `Union` / `Record` / `Boxed` type, request helper.
After functions and specializations, emit helpers in `BTreeSet` order.

Drop helper body shape:

```llvm
define internal void @"tz.drop.rec.Main.Tree[i64]"(%"tz.union.Main.Tree[i64]" %root) nounwind {
entry:
  ; initialize intrusive worklist = null
  ; process root
  br label %loop
loop:
  ; pop until null
exit:
  ret void
}
```

Clone helper body shape:

```llvm
define internal %"tz.union.Main.Tree[i64]" @"tz.clone.rec.Main.Tree[i64]"(%"tz.union.Main.Tree[i64]" %root) nounwind {
entry:
  ; clone root, enqueue { next, source_ptr, destination_ptr } work items
  br label %loop
loop:
  ; pop until null
exit:
  ret %"tz.union.Main.Tree[i64]" %result
}
```

Do not add a runtime `.ll` file unless necessary.
Drop helper must not allocate work nodes; it reuses the cells/storage it is already going to free.
Clone helper may allocate work nodes with `@tz.alloc` because clone itself allocates the destination graph; each work node is freed immediately after pop and its fields are saved before free.
Both helpers must scan nested aggregates and dynamic array/list elements in the same traversal rather than calling recursive `drop_value` / `clone_value` for recursive elements.
drop/clone helpersを `CheckedFunction` として表す実装を選ぶ場合は GUIDE D-22 に従い、`CheckedFunction.origin.provenance = Generated(RecursiveDrop|RecursiveClone)`、`parent` はそれを要求した emitted function、`module` / `test` は親から継承する。
LLVM-only helperとして `emit_target` 内で直接生成する場合も、E02 reachability と G03/G06 の由来情報を壊さないよう、source user function として扱ってはならない。

### 各コンパイラ段の変更有無

| 段 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | 変更なし |
| computation::expand | `BoxRecursive` は typed IR のため変更なし |
| check | recursive declaration graph, `Type::Boxed`, transparent boxing/unboxing |
| polymorph | `Boxed` の map/substitute/resolve/unify/bounded/type_expression |
| control | union/record/list pattern projectionで `Boxed` を unbox |
| closures | `TypedExpr::children` に `BoxRecursive` / `UnboxRecursive` を追加 |
| recursion | 関数再帰は変更なし。型再帰検査は check phase |
| ownership | `Boxed`, `BoxRecursive`, `UnboxRecursive`, place component |
| ownership_control | pattern bindings の boxed projection |
| call_specialization | `all_children`, `may_mutate`, `read_only` に boxed typed kinds |
| llvm | box allocation/unbox/drop helper/clone helper |
| llvm_control | pattern payload projectionが place として動くことを確認。switch 自体は A02 のまま |
| llvm_frame | boxed recursive occurrence は heap cell なので frame candidate にしない。inner literal は box 前に通常評価 |
| runtime | 原則変更なし |
| driver | 変更なし |
| main | 変更なし |

## 実装手順

### 1. `Type::Boxed` と型走査

1. `Type::Boxed(Box<Type>)` を追加する。
2. `Type::display`, properties, `polymorph::map_type`, `substitute`, `bounded_type`, `Inference::resolve`, `Inference::unify`, `variables`, `type_expression` を更新する。
3. `TypedExprKind::BoxRecursive` / `UnboxRecursive` を追加し、`children` / `children_mut` に登録する。
4. まだ recursive declarations は許可せず、既存挙動を維持する。

確認:

```sh
cargo test --locked --test recursive_types boxed_type_walk_keeps_existing_rejections
cargo test --locked --test recursive_types
# 各コマンドの `running N tests` が 0 でないことを記録する
```

### 2. 宣言グラフと occurrence boxing

1. `check_modules` の record/union checked declarations 作成後、layout validation 前に recursive graph pass を追加する。
2. SCC と generic args 同一性を検査する。
3. box すべき occurrence を declaration へ書き込まず、`TypeContext.recursive: RecursiveLayout` に concrete instance + occurrence path で記録する。
4. record-only cycle と base case なし union cycle を `E1010` にする。
5. generic args growth を `E1017` にする。
6. `record_instance_fields` / `union_instance_cases` が substitution 後に `RecursiveLayout` を適用するよう更新する。

確認:

```sh
cargo test --locked --test recursive_types
```

### 3. 型検査の transparent boxing

1. expected `Boxed(inner)` に source `inner` を渡したら `BoxRecursive` を挿入する。
2. field/payload projection が `Boxed(inner)` なら source-level use で `UnboxRecursive` を挿入する。
3. pattern matching で recursive payload を通常 pattern として扱う。
4. `validate_size` を box 後 layout に適用する。

確認:

```sh
cargo test --locked --test recursive_types
cargo test --locked --test unions
```

### 4. 所有権

1. `Place` component を enum 化する。
2. `BoxRecursive` / `UnboxRecursive` の eval を実装する。
3. subtree move, borrow, guard failure, OR alternatives の cleanup を検査する。
4. Copy を非 Copy に固定し、必要なエラーを安定化する。

確認:

```sh
cargo test --locked --test recursive_types
cargo test --locked --test types_ownership
cargo test --locked --test control
```

### 5. LLVM allocation / projection / iterative drop

1. `llvm_type(Boxed)` = `ptr`。
2. `BoxRecursive` allocation / store。
3. `UnboxRecursive` は owning pointer slot address から load/take/free し、slot に `ptr null` を store する。
4. recursive drop helper generation。drop helper は allocation-free intrusive worklist にする。
5. recursive clone helper generation。function environment clone が recursive captures を deep clone できるよう必ず実装する。
6. array/list elements に recursive boxes が含まれる場合、既存 `drop_value` / `clone_value` を再帰呼び出しせず、同じ traversal が dynamic elements を scan する。
7. `drop_value` / `clone_value` が native stack recursion しないことを IR で検査する。

確認:

```sh
cargo test --locked --test recursive_types
cargo build --release --locked
node tests/recursive_types.mjs target/release/tsuzuri
```

### 6. E2E と docs

1. `tests/fixtures/recursive_types/Main.tz` を追加する。
2. native/WASM の正常・trap・heap tracking を追加する。
3. docs を更新する。
4. full validation。

確認:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test recursive_types
cargo test --locked --test control
cargo test --locked --test types_ownership
# 各コマンドの `running N tests` が 0 でないことを記録する
cargo build --release --locked
node tests/recursive_types.mjs target/release/tsuzuri
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
```

## テスト計画

### Rust 受理テスト

`tests/recursive_types.rs` を追加し、`cargo test --locked --test recursive_types` で実行する。
出力の `running N tests` が 0 でないことを記録する。

Tree:

```text
union Tree<'a> =
    | Leaf
    | Node of Tree<'a> * 'a * Tree<'a>

def rec sum :: Tree<i64> -> i64
fn rec sum tree =
    match tree with
    | Leaf -> 0
    | Node (left, value, right) -> sum left + value + sum right

sum (Node (Node (Leaf, 1, Leaf), 2, Node (Leaf, 3, Leaf)))
```

Expected: `6`.

List:

```text
union List<'a> = Nil | Cons of 'a * List<'a>

def rec length :: List<'a> -> i64
fn rec length xs =
    match xs with
    | Nil -> 0
    | Cons (_, tail) -> 1 + length tail
```

Record through union:

```text
record Branch<'a> { left: Tree<'a>, value: 'a, right: Tree<'a> }
union Tree<'a> = Empty | Branch of Branch<'a>
```

Array/list inside recursive union:

```text
union Rose<'a> = Rose of 'a * [Rose<'a>]
```

この case は array/list が storage barrier なので per-element boxing しない。
受理し、layout recursion ではなく traversal/drop/clone の stack safety で検査する。

Borrow subtree:

```text
def root_value :: &Tree<i64> -> i64
fn root_value tree =
    match tree with
    | Leaf -> 0
    | Node (_, value, _) -> value
```

Constructor equivalence:

```text
union Tree = Leaf | Node of Tree * i64 * Tree

def apply :: (Tree * i64 * Tree -> Tree) -> (Tree * i64 * Tree) -> Tree
fn apply f payload = f payload

def checksum :: Tree -> i64
fn checksum tree =
    match tree with
    | Leaf -> 0
    | Node (left, value, right) -> checksum left + value + checksum right

let payload = (Leaf, 42, Leaf)
let direct = Node payload
let via_apply = apply Node payload
let make: Tree * i64 * Tree -> Tree = Node
let via_annotation = make payload
checksum direct + checksum via_apply * 10 + checksum via_annotation * 100
```

期待: `42 + 420 + 4200 = 4662`。
`tick` 関数で payload subexpressions の評価回数を数える variant も追加し、direct / apply / annotated constructor function value が同じ左から右評価と occurrence-path allocation order を持つことを確認する。

### Rust 拒否テスト

| ソース | 期待 |
|---|---|
| `record Bad { next: Bad }` | `E1010` |
| `record A { b: B } record B { a: A }` | `E1010` |
| `union Bad = Bad of Bad` | `E1010` |
| `union Bad<'a> = Bad of Bad<Tree<'a>>` | `E1017` |
| `union Tree<'a> = Leaf | Node of Tree<'a>; let t = Leaf; let a = t; let b = t` | `E1012` because recursive type non-Copy |
| `union Tree = Leaf | Node of Tree; export def f :: Tree ...` | `E1008` |
| recursive constructor function value を `recursion.rs` が `$case...` として報告する | 発生しないこと |

### LLVM / IR テスト

検査:

- recursive occurrence の payload LLVM type が `ptr`。
- `@tz.alloc` が constructor path に出る。
- drop helper `@tz.drop.rec.` が 1 回だけ定義される。
- generated drop helper body に自分自身への `call @tz.drop.rec...` がない。
- generated clone helper body に自分自身への `call @tz.clone.rec...` がない。
- deep list/tree fixture の IR に source-level recursive drop call がない。
- tail-loop fixture で recursive values を持つ末尾ループの alloca が entry block にあり、loop 化が保たれる。
- `UnboxRecursive(UnionPayload(...))` の take path で owner pointer slot に `store ptr null` が出る。

例:

```rust
assert!(ir.contains("@tz.drop.rec."));
assert!(ir.contains("@tz.clone.rec."));
let helper = ir.split("define internal void @\"tz.drop.rec.").nth(1).unwrap();
assert!(!helper.contains("call void @\"tz.drop.rec."));
assert!(ir.contains("call ptr @tz.alloc"));
```

### E2E fixture

`tests/fixtures/recursive_types/Main.tz`:

```text
union List =
    | Nil
    | Cons of i64 * List

def rec make :: i64 -> List -> List
fn rec make n acc =
    if n == 0 then acc else make (n - 1) (Cons (n, acc))

def rec sum :: List -> i64 -> i64
fn rec sum xs total =
    match xs with
    | Nil -> total
    | Cons (head, tail) -> sum tail (total + head)

export def small_sum :: i64
fn small_sum =
    sum (make 1000 Nil) 0

export def deep_drop :: i64 -> i64
fn deep_drop n =
    let xs = make n Nil
    xs.length_if_this_function_does_not_exist
```

上の `deep_drop` は擬似例なので、実際には `length` 関数か `sum` で全走査後に drop する。

正しい fixture:

```text
export def deep_sum :: i64 -> i64
fn deep_sum n =
    sum (make n Nil) 0

export def deep_discard :: i64 -> i64
fn deep_discard n =
    let xs = make n Nil
    42
```

期待値:

- `small_sum()` = `1000 * 1001 / 2 = 500500`
- `deep_sum(100000)` = `100000 * 100001 / 2 = 5000050000`
- `deep_discard(n)` = `42`。この関数は intact chain をスコープ終了で一括 drop し、destructor traversal を直接検査する。

追加 fixture:

```text
union Rose = Rose of i64 * [Rose]
union RList = RNil | RCons of i64 * [|RList|]
```

- branching/container-recursive destructor case: array/list 内に recursive values を入れ、関数は値を走査せず discard して drop helper が dynamic elements を stack-safe に処理することを確認する。
- closure clone case:

```text
def make_counter :: Tree -> (unit -> i64)
fn make_counter tree =
    fx () -> checksum tree

export def closure_clone :: i64
fn closure_clone =
    let tree = make 10000 Nil
    let f = make_counter tree
    let g = f
    f () + g ()
```

期待: `f` と `g` が独立した cloned environments を持ち、両 environment 解放後に `live == 0`。

Native:

- `deep_sum(1_000_000)` が stack overflow せず成功。
- `deep_discard(1_000_000)` が stack overflow せず成功し、intact chain destructor が走る。
- closure clone case を 100 回以上繰り返して、deep clone/drop の stack safety と leak なしを確認する。
- 期待値は JS BigInt で `n * (n + 1) / 2`。
- 各 call 後 heap tracking `live == 0`。
- ASan が使える環境では `TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/recursive_types.mjs target/release/tsuzuri` を追加実行する。

WASM:

- 16 MiB heap 内で成功する固定 safe size と、必ず trap する size を allocator formula から計算してテストする。
  `heap-wasm.ll` の `@tz.alloc` は `%needed = (%size + 31) & -16` なので、各 allocation block は少なくとも 32 bytes。
  `block(payload_bytes) = (payload_bytes + 31) & -16`、`limit = 16 * 1024 * 1024` とし、safe size は
  `n * block(node_payload) + collection_blocks + clone_destination_blocks + clone_work_blocks < limit * 3 / 4` のように余裕を持って固定する。
  drop は allocation-free なので drop workspace は 0。
  trap size は `n * block(min_node_payload) > limit`、または clone case なら `source_blocks + destination_blocks + work_blocks > limit` になる値を使い、construct または clone 中の trap を確認する。
- WASM imports empty。

### 1,000,000-node 検証

要求仕様として native と WASM の deep construction/drop を検証対象にする。
現行 `heap-wasm.ll` は `%needed = (%size + 31) & -16` なので、各 allocation block は最小 32 bytes。
1,000,000 個の個別 heap cell は最小でも約 32 MiB を必要とし、16 MiB 上限を超える。

A04 の既定受け入れは:

- native: 1,000,000 nodes 成功、stack overflow なし、`live == 0`
- wasm32: 16 MiB 内に収まる最大規模の正常系 + 1,000,000 nodes は明示的 trap

もし reviewer が「WASM でも 1,000,000 nodes 正常」を必須にする場合は、A04 に bump/slab allocator または compact recursive node arena を追加する必要がある。
これは runtime と所有権 API に広く影響するため、このチケットの既定案では行わない。

## ドキュメント

- `docs/language.md`
  - 再帰的 union の例 `Tree` / `List` を追加。
  - recursive record-only cycle は引き続き不可と明記。
  - recursive ADT は非 Copy と明記。
  - implicit boxing は構文に現れないが、heap allocation と trap の可能性があることを説明。
- `docs/architecture.md`
  - `Type::Boxed`, declaration graph, iterative drop helper, stack safety を記載。
  - WASM 16 MiB heap と deep recursive structures の制約を記載。
- `README.md`
  - 代数的データ型の説明に木/AST の例を追加。
  - 未対応一覧から「再帰的なヒープ型」を削除。

## 受け入れ条件

- [ ] `union Tree<'a> = Leaf | Node of Tree<'a> * 'a * Tree<'a>` が受理される。
- [ ] declarations は source form のまま保持され、`RecursiveLayout` が concrete instance + occurrence path ごとに boxing を指示する。
- [ ] `record_instance_fields` / `union_instance_cases` は型引数 substitution 後に boxing を適用する。
- [ ] source syntax と diagnostics は `Tree<'a>` のままで、内部 `Boxed` を表示しない。
- [ ] `Boxed` が canonical type text の型引数に現れ得る場合は `boxed[...]` marker で単射的に mangle される。
- [ ] record-only recursive cycle は `E1010` のまま。
- [ ] base case なし recursive union は `E1010`。
- [ ] generic args growth は `E1017`。
- [ ] recursive ADT は非 Copy。
- [ ] constructor は payload expression 全体を一度だけ左から右に評価し、その後 deterministic occurrence-path order で implicit allocation する。
- [ ] direct constructor、`apply` 経由、注釈付き constructor function value 経由で評価順序と allocation path order が同一。
- [ ] pattern matching で boxed subtree を透明に分解できる。
- [ ] `UnboxRecursive` take は owning pointer slot に `ptr null` を store し、subtree move 後に dangling pointer / 二重解放しない。
- [ ] `UnionPayload` / `UnboxRecursive` は ownership と LLVM place path に参加し、guard failure / OR alternatives / nested payload move / tail loops で exact slot zeroing が保たれる。
- [ ] drop は native stack depth に比例せず、1,000,000 node native case で stack overflow しない。
- [ ] clone は native stack depth に比例せず、tree を捕捉した lambda をコピーして両 environment を解放しても `live == 0`。
- [ ] recursive traversal は nested aggregates と dynamic array/list elements を scan し、recursive elements に既存 `drop_value` / `clone_value` を再帰呼び出ししない。
- [ ] successfully constructed value の implicit drop は worklist allocation failure で trap しない（drop helper は allocation-free）。
- [ ] `deep_discard(1_000_000)` が native で成功し、intact destructor traversal を検査する。
- [ ] normal completion 後 heap tracking `live == 0`。
- [ ] WASM imports empty、`%needed = (%size + 31) & -16` に基づく safe/trap sizes が検査され、16 MiB limit 超過は trap。
- [ ] A03 は `Boxed` を pattern boundary で消して recursive constructors の網羅性を検査する。
- [ ] docs が更新される。

## 落とし穴

- `Boxed(T)` と `T` を unification で同一扱いすると、layout と ownership が破綻する。
  expression boundary で `BoxRecursive` / `UnboxRecursive` を挿入する。
- generic declarations へ `Boxed` を書き込むと、非再帰 instance まで box され、D-03 canonical mangle も壊れる。boxing は `RecursiveLayout` で instance-sensitive に適用する。
- `contains_borrow_pointer` を inner に委譲すると task result が owned recursive value を borrowed pointer と誤認する。
  一方で `stores_borrowed_data` / `can_send` / `validate_borrows` は inner を見る必要がある。
- recursive type を structural Copy にすると、`let b = a` が巨大 deep clone になり、ownership inference も循環する。
  A04 では非 Copy。
- drop helper が recursive call を生成すると、A04 の目的である deep stack safety を満たさない。
- drop helper で heap worklist node を allocate すると、構築済み値の暗黙 drop が allocation failure で trap し得る。drop は intrusive / allocation-free にする。
- clone helper で work node を free する前に `next/source/destination` を保存しないと use-after-free になる。
- `usize::MAX` sentinel は既に collection element loan に使われている。
  boxed inner place には別 component enum を使う。
- `llvm_frame` に boxed cell 内部を stack frame candidate として登録すると、box pointer が heap cell を指す不変条件が壊れる。
- WASM 16 MiB と 1,000,000 node は現行 per-node allocation では両立しない。最小 block size は 32 bytes。

## 対象外

- Cyclic runtime values（自己参照 pointer）や graph sharing。
- Reference counting / GC。
- User-visible `box` / `unbox` / pointer 型。
- Recursive records without union base。
- Structural Copy / deriving Clone for recursive ADT。
- Arena allocator / slab allocator / compact node pool。
- Tail recursion optimization の拡張。
- Exhaustiveness algorithm の変更（A03）。

## 未決事項

- 台帳の見直し提案（WASM 1,000,000-node 検証）:
  現行 `heap-wasm.ll` は 16 MiB 上限で、`@tz.alloc` は `%needed = (%size + 31) & -16` により各 block を少なくとも 32 bytes に丸める。
  1,000,000 recursive nodes を個別 `@tz.alloc` すると最小でも約 32 MiB を必要とするため、WASM 正常系としては実現不能。
  既定案は native で 1,000,000 nodes 正常、WASM では 16 MiB 内正常 + 1,000,000 nodes trap を検証する。
  WASM でも 1,000,000 nodes 正常を必須にするなら、別チケットで compact arena/slab allocation を設計する。
- recursive ADT を非 Copy にする既定案を採る。
  代替は payload がすべて Copy なら deep Copy を許すことだが、暗黙 O(n) clone と stack-safe clone helper が必要になる。
- `Type::Boxed` を public enum variant にするため、外部 crate が `tsuzuri::check::Type` を直接 match している場合は破壊的変更。
  既定案は compiler crate 内部 API とみなし、variant 追加を許容する。
