# A14: 型クラスによる動的ディスパッチ（`dyn` 値）

| 項目 | 内容 |
| --- | --- |
| ID | A14 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A06, (B07) |
| 後続 | E13, G17 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（予約語 `dyn`・構築 `Dyn.of`・診断 `E1028` の確定。GUIDE D-30 の仮割り当て） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: 実行時多相がなく、異種コレクションとコードサイズの制御ができない |
| 手本にする既存実装 | 非 Copy・clone 不可の葉の型: `Type::Task`（`src/check.rs` の型性質、`src/llvm.rs` の `clone_value`）。関数ポインターと helper 関数: `src/llvm.rs` の `make_closure`・`apply_value` と環境 drop helper（`call void @tz.free(ptr %env)`）。生成インスタンスと既定メソッド関数: `src/polymorph.rs` の `Classes::instances`、`src/derive.rs` の `instances`。決定的な型名: `src/llvm.rs` の `canonical_type`。型式の追加: `TypeExprKind::Task` の各 match |
| 主な影響ファイル | `src/lexer.rs`, `src/syntax.rs`, `src/parser.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_debug.rs`, `tests/dyn_dispatch.rs`（新規）, `tests/fixtures/dyn_dispatch/Shapes.tt`・`Main.tz`（新規）, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/generics-and-typeclasses.md`, `_docs/guides/from-fsharp.md`, `_docs/feature-status.md`, `_features/README.md`, `vsc/` の予約語文法（GUIDE §6.1） |

## 目的

異なる型の値を同じ型クラスの実装として一つのコレクションへ格納し、メソッドを実行時に選べるようにする。
C++ の仮想関数、Rust の `dyn Trait`、C#/F# のインターフェースに相当する。
単相化だけに頼らない経路を用意し、プラグイン的な拡張、コードサイズの抑制、特殊化上限（1,024）への対処を可能にする。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- A06（既定メソッド・スーパークラス）が `_features/README.md` の状態欄で done であること。HEAD では `Class::superclasses`・
  `Method::default` があり、既定メソッドの文書例も動く。確認: `grep -n "A06\|A14\|B07" _features/README.md`。
- B07 は開始条件にしない（D10）。B07 の状態は drop slot の挙動（利用者 drop の有無）だけを変える。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `Type` を 4 語より大きくしないと `Type::Dyn`（新規）を表せない。
- クラス名の正規化（D2）を制約 `ConstraintName::Class` の解決と共有できず、`resolve_type` の引数や呼び出し元の広い変更が必要になる。
- 合成した `InstanceDecl` が `Classes::instances` の既存経路を通らない（ソース種別・orphan・可視性の検査で拒否される、関数 id を得られない）。
- 利用者関数の直接呼び出しに hidden 引数・`sret` などがあり、slot adapter（D4）から既存の直接呼び出し emission を再利用できない。
- `FunctionEmitter::clone_value` が `Type::Dyn` で呼ばれる経路が見つかった（clone slot の追加は D8 の変更になる）。
- `@tz.alloc` が `storage_layout` の align（最大 16）を満たさない。
- 既存テストの期待値（IR・診断コード・メッセージ）を変える必要がある。予約語一覧への `dyn` の追加だけは除く。
- stack-depth の 3 テストか `honors_the_exact_specialization_limit` が失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 型クラスは単相化だけで解決する。`src/polymorph.rs` の `specialize` が `Specializer::request` で特殊化を要求し、
  `MAX_SPECIALIZATIONS`（1,024）を超えると `E1017 "more than 1024 specializations; remove type-growing polymorphic recursion"`
  を返す。辞書渡し・trait object・vtable はない。
- クラス表は `Classes { declarations, names, instances }`。`Class { name, variable, arity, superclasses, methods, builtin }`、
  `Method { name, signature, operation, default }`、`InstanceTemplate { class, head, constraints, methods, span, derived }`。
  組み込みクラスは `BUILTIN_CLASSES` の 25 個。通常クラスの型変数は一つ（`variable`）で、`arity != 0` は高階型クラス。
- インスタンスメソッドと既定メソッドは `Classes::instances` が関数として作る（名前 `$instance.{function_id}.{method}`、
  `$instance.default.{function}.{method}`）。`$instance.` で始まる関数は公開型の検査から外れる（`src/check.rs` のコメント
  「Instance methods are reached only through class dispatch」）。
- メソッド参照は `TypedExprKind::Method(class, method, ty)` で、制約を `self.constraints.push(Constraint { .. })` で積む。
  特殊化は `instance_function` などで `TypedExprKind::Function(FunctionRef::User(id))` に解決し、組み込み実装は
  `FunctionRef::Builtin(BuiltinInstance { builtin, types })` になる。
- `Type`（`src/check.rs`）は引数を box して 4 語に保つ（`Record` のコメント）。型性質の match は fallback を持つ:
  `is_copy` は `_ => true`、`needs_drop` は `_ => false`、`can_capture`・`can_send` は `_ => true`、
  `contains_reference`・`carries_loans` は `_ => false`。`exportable` は許可する型の `matches!`。
- `TypeExprKind`（`src/syntax.rs`）は `Named`・`Variable`・`Apply(Box<Ident>, Box<[TypeExpr]>)`・`Regions`・`Array`・`List`・
  `Tuple`・`Task`・`Function`・`Reference`。`Apply` は parser の再帰 stack を抑えるため box している。
- LLVM: `emit_program` は固定ヘッダーで `%tz.closure = type { ptr, ptr, ptr, ptr }` などを常に出し、`reachable_functions`
  の到達関数だけを出力する。`llvm_type`・`canonical_type`（D-03 の injective な型名）・`storage_layout` は `Type` の網羅 match。
  `FunctionEmitter::clone_value` は `Type::Task(_) => unreachable!("single-use tasks cannot be cloned")`。確保は
  `@tz.alloc(i64)`、解放は `@tz.free(ptr)`。
- 閉じた集合は union で表せるが、別モジュールからケースを追加できない。関数値（`%tz.closure`）は複数メソッドをまとめられない。
- クラス名は型として引けない。`dyn`・`Dyn` は Tsuzuri のソース（`std/`・`tests/`・`examples/`・文書）で未使用。
- 通常の型クラスではメソッド固有の型変数・制約が未対応（`_docs/language-reference/generics-and-typeclasses.md` の `## 制限`）。
  dyn 互換規則のうち「メソッド固有の型変数・追加制約に現れない」は、この既存の制限で満たされる。

### 再現（検証済み）

`/tmp/tz-a14/sample/Shapes.tt` と `/tmp/tz-a14/sample/Main.tz`。`target/release/tsuzuri run /tmp/tz-a14/sample` は `29`。
手順 1 のベースラインと手順 11 の比較に使う。

```tsuzuri
class Shape<'a> {
    def area :: ref 'a -> i64
}
```

```tsuzuri
record Square { side: i64 }
record Rect { width: i64, height: i64 }

instance Shapes.Shape<Square> {
    fn area square = square.side * square.side
}

instance Shapes.Shape<Rect> {
    fn area rect = rect.width * rect.height
}

def area_of :: Shapes.Shape<'a> => ref 'a -> i64 = \shape -> Shapes.Shape.area shape

let square = Square { side: 3 }
let rect = Rect { width: 4, height: 5 }
area_of (ref square) + Shapes.Shape.area (ref rect)
```

今日の挙動。各ケースを別ディレクトリの `Main.tz` に置き、`target/release/tsuzuri check <dir>`（1 行目だけ `run <dir>`）で確かめた。

| ソース | HEAD の結果 |
| --- | --- |
| `let dyn = 20` の次行に `dyn + 22` | `42`（`dyn` は通常の識別子） |
| `record Square { side: i64 }` と `let shape: dyn Square = Square { side: 3 }` | `E0002 expected '=' in the binding`（`Square` の位置） |
| 上の `Shapes.tt` と同じ root で `def f :: Shapes.Shape -> i64 = \shape -> 0` | `E1004 unknown record, union, or type alias 'Shapes.Shape'` |
| `let x: Nope = 1` | `E1004 unknown record, union, or type alias 'Nope'` |
| `export def f :: string -> i64 = \text -> 0` | `E1008`（export ABI） |

## 仕様

Phase 1（実装対象）を定める。Phase 2 は末尾の設計方針だけ。

### 前提とする他チケットのインターフェース

- A06（HEAD で実装済み）: `Class::superclasses`・`Method::default` と、`Classes::instances` が作る既定メソッド関数。
- B07（任意）: 利用者 `Drop` を持つ型の drop glue は、`Drop.drop`（値への `ref mut`）を先に呼び、その後 field・payload を
  `FunctionEmitter::drop_value` の既存順序（宣言順）で drop する。A14 の drop slot は `drop_value(T)` と同じ drop glue を呼んでから
  data を解放するだけなので、B07 の実装後は自動的に利用者 drop を含む。B07 が未実装なら既存の drop glue だけになる。
- D-03: LLVM 上の型名は `canonical_type` の出力を使う。

### 構文

新構文（実装後に有効。未検証）:

```text
type_atom  ::= ... | "dyn" class_name
class_name ::= identifier { "." identifier }
dyn_of     ::= "Dyn.of" argument        (* 期待型が dyn C の位置での、ちょうど一引数の適用だけ *)
```

- `dyn` は予約語（GUIDE §6.1）。型の `ref`・`ref mut` と同じ前置位置に書き、クラス名を一つだけ取る。型引数は取らない。
- `ref dyn C` は `ref (dyn C)`。`[dyn C]`、`Vec<dyn C>`、`Maybe<dyn C>`、`dyn C -> i64` はそのまま書ける。
- `Dyn` は予約語ではない。`Task.run` と同じ組み込みの修飾名 `Dyn.of` として解決する。

### 型規則

- `dyn C` は「`C` のインスタンスを持つ何らかの型の所有値」を表す。`C` が型クラスで、`dyn_slots(C)`（新規、D6）が `Ok` のときだけ
  正しい型になる。内部の名前は `Class::name`（D2）。
- `Dyn.of e` は期待型が `dyn C` の位置でだけ書ける（D9）。`e : T` について制約 `C<T>` を積む。`T` の具体化後、各 slot は利用者関数へ
  解決できなければならない（D6）。
- `dyn C` が満たすクラスは `vtable_classes(C)`（新規。`C` と推移的なスーパークラス）だけで、合成インスタンス（D7）で満たす。
  `Copy`・`Send`・`Capture`・`Eq`・`Ord` などは満たさない。既存の制約付きジェネリック関数は `'a = dyn C` で一つだけ具体化され、
  全実装型で共有される。
- `dyn C` と `dyn D` は名前が同じときだけ等しい。部分型も暗黙変換（`dyn C` から `dyn S`）もない。
- `T` が `dyn D` でもよい（`C` が `vtable_classes(D)` に含まれるとき）。値は二重に包まれる。
- 性質（`src/check.rs` の `Type` の各メソッド）:

| 性質 | `dyn C` | 理由 |
| --- | --- | --- |
| `is_copy` | false | 所有する data 領域を持つ |
| `needs_drop` | true | vtable の drop slot を呼ぶ |
| `contains_reference`・`carries_loans` | false | 借用を持つ値は `Dyn.of` が拒否する（E1013） |
| `can_capture` | false | clone slot がない（D8） |
| `can_send` | false | Phase 1 は `Send` なし |
| `exportable` | false | host ABI の対象外（既存の E1008） |
| 値の大きさ | native 16 byte、wasm32 8 byte | `{ ptr, ptr }` |
| `storage_layout` | `(16, 8)` | 64-bit の上界（既存の規約） |

### 評価順序・所有権・借用

- `Dyn.of e`: `e` を評価し、`@tz.alloc` で data 領域を確保し、値を move して格納し、`{ data, vtable }` を作る。`e` がトラップしたら
  確保しない。元の束縛は move 済みで、再使用は E1012。
- メソッド呼び出し: 引数は既存の呼び出しと同じく左から評価する。その後 dispatch 関数が vtable から slot を読み、間接呼び出しする。
- 受け手: `ref 'a`・`ref mut 'a` のメソッドは `ref (dyn C)`・`ref mut (dyn C)`（`{ ptr, ptr }` を指す `ptr`）を受け、adapter へ
  data の `ptr` を渡す。`ref mut` のメソッドは data 領域の値をその場で変える。`'a` のメソッドは `dyn C` を move で受け、adapter が
  値を読み出して data 領域を `@tz.free` し、値をメソッドへ move する（値は drop しない）。
- 借用規則は通常の値と同じ（共有借用中の move・排他借用は既存の E1014）。
- drop: scope 終了時（既存の `drop_scope` の順序）に drop slot を呼ぶ。drop slot は `drop_value(T)` の drop glue を実行してから data を
  `@tz.free` する。
- clone はない（D8）。トラップ時は巻き戻さず、未実行の drop は保証しない（既存の方針）。

### 数値・トラップ・native と WASM の差

- 数値の意味とトラップは変えない。`@tz.alloc` の既存の確保失敗の扱いに従う。
- pointer は native 8 byte、wasm32 4 byte。vtable の `size`・`align` と data 領域の確保量は `storage_layout(T)`（64-bit の上界）で、
  両 target で同じ定数を出す。wasm32 では過大だが安全側である。
- WASM の間接呼び出しは関数型を実行時に検査する。adapter の定義と dispatch 側の `call` の型を完全に一致させる（D4）。
- 既定の WASM import は追加しない（D-18）。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E0002` | `dyn` を識別子に使った | 既存の予約語と同じメッセージ | その token |
| `E0002` | `dyn` の後にクラス名がない | `expected a type class name after dyn` | `dyn` の次の token |
| `E0002` | `dyn C<...>` | `dyn takes a class name without type arguments; write dyn Shapes.Shape` | `<` |
| `E1004` | 名前が見つからない | `unknown type class '{name}'; dyn needs a type class such as dyn Shapes.Shape` | クラス名 |
| `E1004` | record・union・型別名の名前 | `'{name}' is not a type class; dyn needs a type class such as dyn Shapes.Shape` | クラス名 |
| `E1028` | `dyn_slots(C)` が `Err(reason)` | `dyn {C} is not allowed: {reason}; use a generic function with a {C} constraint instead` | `dyn C` の各出現 |
| `E1028` | slot が組み込み実装に解決された | `cannot store {T} in dyn {C}: {X}.{m} for {T} is built into the compiler; wrap the value in a record with its own {X} instance` | `Dyn.of` の呼び出し |
| `E1015` | 期待型が `dyn C` でない、または未確定 | `Dyn.of needs an expected dyn type here; annotate the binding, parameter, or field, for example let shape: dyn Shapes.Shape = Dyn.of value` | `Dyn.of` の呼び出し |
| `E1015` | 一引数の直接適用以外（値としての参照、pipe の右辺など） | `Dyn.of must be applied to exactly one argument where a dyn type is expected` | `Dyn.of` |
| `E1013` | 引数の値が借用を持つ | `a dyn value cannot hold borrowed data in phase 1; pass an owned value to Dyn.of` | 引数 |
| 既存 | move 後の使用、インスタンスなし、`Copy`・`Send`・捕捉の不成立、export・extern、利用者が書いた `C<dyn C>`（重複） | 既存のコードとメッセージ（E1012、E1008、E1016 など） | 既存 |

`{reason}` は次のどれか。`{X}` はメソッドを宣言したクラス、`{m}` はメソッド名。

- `{C} is higher-kinded`、`superclass {X} is higher-kinded`
- `{C} has no methods to dispatch`
- `superclass {X} has no methods, so a dyn value cannot satisfy it`
- `method {X}.{m} does not take the class type by value, ref, or ref mut as its first parameter`
- `method {X}.{m} uses the class type outside its first parameter`
- `method {X}.{m} has an invalid signature`

### 資源上限

- 新しい上限は追加せず、既存の上限も上げない。
- vtable は到達した `Dyn.of` の (C, T) ごとに一つ。slot 関数の要求は既存の特殊化上限（追加 1,024）に数える。`T` の深さと構成要素は
  既存の `bounded_type`（128・4,096）で抑える。slot 数はクラスとスーパークラスのメソッド数で、ソースの大きさで抑えられる。

### 例

新構文（実装後に有効。未検証）。`Shapes.tt` は「再現」の検証済みサンプルと同じで、`Main.tz` の最後の 3 行だけを次に変える。

```tsuzuri
let square: dyn Shapes.Shape = Dyn.of (Square { side: 3 })
let rect: dyn Shapes.Shape = Dyn.of (Rect { width: 4, height: 5 })
area_of (ref square) + Shapes.Shape.area (ref rect)
```

結果は `29`（単相版と同じ）。`area_of` は `'a = dyn Shapes.Shape` で一度だけ具体化される。

拒否例（新構文。未検証）:

| ソース | 結果 |
| --- | --- |
| `let shape = Dyn.of (Square { side: 3 })` | `E1015`（期待型がない） |
| `let shape: dyn Square = Dyn.of (Square { side: 3 })` | `E1004`（クラスではない） |
| `class Same<'a> { def same :: ref 'a -> ref 'a -> bool }` の後に `dyn Same` | `E1028`（`uses the class type outside its first parameter`） |
| `let shown: dyn Display = Dyn.of 42` | `E1028`（`i64` の `Display` は組み込み実装） |
| `let dyn = 1` | `E0002` |
| `let boxed: dyn Shapes.Shape = Dyn.of square` の後に `square` を使う | `E1012` |
| `export def f :: ref (dyn Shapes.Shape) -> i64 = \shape -> 0` | `E1008` |

### Phase 2（設計方針）

- `Send` を満たす dyn（`dyn (C, Send)` を検討）、借用を格納する `dyn C {r}`（A12 の region）、複数クラスの組み合わせ、
  `dyn C` から `dyn S` へのアップキャスト、clone slot、組み込み実装（`Display<i64>` など）を slot に入れる adapter。
- どれも Phase 1 の表現（`{ ptr, ptr }` と D4 の vtable）へ field か slot を足して入れる。

## 設計

### データ構造

以下の variant・field・関数はすべて新規。

```rust
// src/check.rs
pub enum Type {
    // ... existing variants ...
    /// `dyn C`: an owned value of some type with a `C` instance, stored as
    /// `{ data, vtable }`. Holds `Class::name`; a leaf, so `Type` stays four words.
    Dyn(Box<str>),
}

pub enum Builtin {
    // ... existing variants ...
    /// `Dyn.of`; typed only by the checker's `dyn_of` helper.
    DynOf,
}

pub struct CheckedModule {
    // ... existing fields ...
    /// `(class name, concrete type)` to specialized method functions in slot order.
    pub vtables: BTreeMap<(String, Type), Vec<usize>>,
    /// Some module wrote `dyn`; gates the `%tz.dyn` header line.
    pub uses_dyn: bool,
}

// the file with `pub enum TypedExprKind` (grep -rn "pub enum TypedExprKind" src)
pub enum TypedExprKind {
    // ... existing variants ...
    /// Whole body of a generated `X<dyn C>` method: call slot `slot` of a vtable with `slots` slots.
    DynDispatch { slot: u32, slots: u32 },
}
```

```rust
// src/syntax.rs
pub enum TypeExprKind {
    // ... existing variants ...
    /// `dyn Shapes.Shape`; boxed like `Apply` so `TypeExpr` does not grow.
    Dyn(Box<Ident>),
}

pub enum ExprKind {
    // ... existing variants ...
    /// Generated body of an `X<dyn C>` method; the parser never produces it.
    DynDispatch { slot: u32, slots: u32 },
}

pub struct Program {
    // ... existing fields ...
    /// Every class name written after `dyn` in this module, in source order.
    pub dyn_classes: Vec<Ident>,
}
```

```rust
// src/polymorph.rs
struct Class {
    // ... existing fields ...
    /// Vtable slots `(declaring class, method index)` in slot order, or why `dyn` is rejected.
    dyn_slots: Result<Vec<(usize, usize)>, String>,
}

// src/llvm.rs
struct Globals {
    // ... existing fields ...
    /// `(class name, concrete type)` pairs whose vtables are emitted after all functions.
    vtables: BTreeSet<(String, Type)>,
}
```

ほかに `src/lexer.rs` の `TokenKind::Dyn`、検査器の非再帰 helper `dyn_of`、`src/polymorph.rs` の `vtable_classes`・`dyn_slots`・
`dyn_instances`、`src/llvm.rs` の `FunctionEmitter::dyn_of`・`emit_dyn_dispatch`・`emit_dyn_vtables` を追加する。

### 段ごとの変更

GUIDE §6.1（予約語）と §6.3（`Type` の variant）のチェックリストを、HEAD の match に当てはめた一覧。fallback が正しい箇所も
「変更なし」として載せる（実装者は arm を足さない）。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 字句 | `src/lexer.rs` | `identifier`, `TokenKind` | `"dyn" => TokenKind::Dyn`。GUIDE §6.1 の予約語手順に従う |
| 構文 | `src/syntax.rs` | `TypeExprKind`, `ExprKind`, `Program` | `Dyn(Box<Ident>)`、`DynDispatch { slot, slots }`、`dyn_classes` |
| 構文 | `src/parser.rs` | 型の `ref` 前置を読む関数（`TypeExprKind::Reference(Box::new(inner), mutable)` を作る箇所） | `TokenKind::Dyn` の分岐。修飾名を一つ読み、`<` が続けば E0002。`dyn_classes` へ記録し、`Program` の構築に field を足す |
| 整形 | `src/formatter.rs` | `ty`、式を整形する match | `dyn` とクラス名。`ExprKind::DynDispatch` は `unreachable!`（生成専用） |
| 文書生成 | `src/docgen.rs` | `type_text` | `format!("dyn {}", name.text)` |
| LSP | `src/semantic.rs` | `type_entry` | クラス名を制約のクラス名と同じ種類の参照として登録する（その仕組みがなければ登録しない） |
| 検査 | `src/check.rs` | `Type`, `Type::display` | `Dyn` と表示 `dyn {name}` |
| 検査 | `src/check.rs` | `Type::is_copy` | `Self::Dyn(_)` を false の列へ（fallback は true） |
| 検査 | `src/check.rs` | `Type::needs_drop` | `Self::Dyn(_) => true`（fallback は false） |
| 検査 | `src/check.rs` | `Type::can_capture`, `Type::can_send` | `Self::Dyn(_) => false`（fallback は true） |
| 検査 | `src/check.rs` | `contains_error`, `contains_constructor`, `contains_reference`, `contains_mutable_reference`, `carries_loans`, `exportable`, `TypeContext::recursive` | 変更なし（葉として fallback が正しい） |
| 検査 | `src/check.rs` | `Type::Function(..)` と `Type::Task(_)` に 32 を返す値サイズの match | `Type::Dyn(_) => 16` |
| 検査 | `src/check.rs` | `resolve_type` | `TypeExprKind::Dyn` から `Type::Dyn`（D2）。E1004 の 2 メッセージ |
| 検査 | `src/check.rs` | `validate_public_type`、型別名の `expand`、`TypeExprKind` の子を列挙する 2 つの match | 葉（子なし、そのまま返す） |
| 検査 | `src/check.rs` | `Builtin`（`Self::TaskRun => "Task.run"` の対応表と逆引き）、`scheme` | `DynOf`、`"Dyn.of"`、`'value -> 'dyn` |
| 検査 | `src/check.rs` | 適用式で callee の builtin を判定する分岐（`Builtin::ParallelMap` などを見る箇所）、式検査の `ExprKind` match | `DynOf` を `dyn_of` へ。`DynDispatch` は期待型を型とする `TypedExprKind::DynDispatch` |
| 検査 | `src/check.rs` | `CheckedModule` | `vtables`、`uses_dyn` |
| 多相 | `src/polymorph.rs` | `Class`, `Classes::collect` | `dyn_slots`、`vtable_classes`、`dyn_classes` の各出現の検査と E1028 |
| 多相 | `src/polymorph.rs` | `Classes::instances` | `dyn_instances` の `InstanceDecl` を既存経路へ流す |
| 多相 | `src/polymorph.rs` | `type_expression` | `Type::Dyn` から `TypeExprKind::Dyn` |
| 多相 | `src/polymorph.rs` | `map_type`, `substitute`, `bounded_type`, `require_concrete`, `variables`, `Inference::resolve`, `Inference::unify`, `TypeExprKind` の走査 | 変更なし（葉。`unify` は同名なら等値で成功し、異名は既存の E1003） |
| 多相 | `src/polymorph.rs` | `specialize`, `Specializer::request`, `instance_function`, `TypedExprKind::Method` の解決 | `Dyn.of` の (C, T) の slot 関数を要求して `vtables` へ。組み込み実装は E1028 |
| 所有権 | `src/ownership.rs` | Task の借用捕捉拒否（`matches!(expression.ty, Type::Task(_)) && !value.loans.is_empty()`）の近く | `Dyn.of` の引数の `loans` が空でなければ E1013 |
| 所有権 | `src/ownership.rs` | `Checker::is_copy` | 変更なし（`Type::is_copy` に委ねる） |
| その他 | `src/higher_kinds.rs`, `src/recursive.rs`, `src/regions.rs`, `src/warnings.rs`, `src/derive.rs`, `src/exhaustiveness.rs` | `decompose`、再帰型・region・警告の走査、`instances`、`specialize` | 変更なし（dyn は葉で不透明。導出は既存の「インスタンスなし」で拒否される） |
| LLVM | `src/llvm.rs` | `llvm_type`, `canonical_type`, `storage_layout` | `%tz.dyn`、`dyn[{name}]`、`(16, 8)` |
| LLVM | `src/llvm.rs` | `FunctionEmitter::drop_value`, `FunctionEmitter::clone_value` | drop slot の間接呼び出し。`unreachable!("dyn values cannot be cloned")` |
| LLVM | `src/llvm.rs` | `FunctionEmitter::call` の builtin 経路 | `DynOf` を `FunctionEmitter::dyn_of` へ |
| LLVM | `src/llvm.rs` | `emit_program`, `reachable_functions`, `Globals` | ヘッダーの `%tz.dyn`、`emit_dyn_dispatch`、`emit_dyn_vtables`、`Dyn.of` から slot 関数への到達、`Globals::vtables` |
| LLVM | `src/llvm.rs` | `named_types` などその他の `Type` 走査 | 変更なし（葉） |
| LLVM | `src/llvm_frame.rs` | `Type::Function(..)`・`Type::Task(_)`・`Type::Vec(_)` に 32 を返す slot size の match | `Type::Dyn(_) => 16` |
| デバッグ | `src/llvm_debug.rs` | field 名の match、size と align の match | `["data", "vtable"]`、`(2 * pointer, pointer)` |

### 生成 IR とランタイム

新しいランタイム関数・link 条件・WASM import はない。記号は次のとおり。`{T}` は `canonical_type(T)`、`{C}`・`{X}` は
`Class::name`、`{m}` はメソッド名。

| 記号 | 形 | 単位 |
| --- | --- | --- |
| `%tz.dyn` | `type { ptr, ptr }`（data, vtable） | module。`CheckedModule::uses_dyn` のときだけヘッダーへ出す |
| `@"tz.vtable.{C}[{T}]"` | `internal unnamed_addr constant { ptr, i64, i64, [N x ptr] }` | (C, T) |
| `@"tz.dyn.drop[{T}]"` | `define internal void (ptr)` | T |
| `@"tz.dyn.slot.{X}.{m}[{T}]"` | `define internal R (ptr, A1, ..., An)` | (X, m, T) |
| dispatch 関数 | 通常の関数（`$instance.` 名）。本体だけを `emit_dyn_dispatch` が出す | (X, m, C) |

vtable の field は drop slot、`size`、`align`、slot の配列。`N` は `dyn_slots(C)` の長さ。`R`・`Ai` はメソッドの結果と第 2 引数以降の
LLVM 型で、主型変数を含まないので `T` に依存しない。次は `Square`（`side: i64`）の例。関数記号と record 型名は既存の命名に従う。

```llvm
%tz.dyn = type { ptr, ptr }

@"tz.vtable.Shapes.Shape[Main.Square]" = internal unnamed_addr constant { ptr, i64, i64, [1 x ptr] } { ptr @"tz.dyn.drop[Main.Square]", i64 8, i64 8, [1 x ptr] [ptr @"tz.dyn.slot.Shapes.Shape.area[Main.Square]"] }

; Dyn.of (Square { side: 3 }): evaluate, allocate, store, pair
  %cell = call ptr @tz.alloc(i64 8)
  store %tz.record.Main.Square %square, ptr %cell
  %pair.0 = insertvalue %tz.dyn undef, ptr %cell, 0
  %pair = insertvalue %tz.dyn %pair.0, ptr @"tz.vtable.Shapes.Shape[Main.Square]", 1

; body of the dispatch function for Shapes.Shape<dyn Shapes.Shape>.area (receiver: ptr to the pair)
  %value = load %tz.dyn, ptr %receiver
  %data = extractvalue %tz.dyn %value, 0
  %vtable = extractvalue %tz.dyn %value, 1
  %slot = getelementptr inbounds { ptr, i64, i64, [1 x ptr] }, ptr %vtable, i32 0, i32 3, i64 0
  %method = load ptr, ptr %slot
  %result = call i64 %method(ptr %data)
  ret i64 %result

define internal i64 @"tz.dyn.slot.Shapes.Shape.area[Main.Square]"(ptr %data) nounwind {
entry:
  %result = call i64 @"$instance.12.area"(ptr %data) ; same symbol as a direct call (illustrative)
  ret i64 %result
}

define internal void @"tz.dyn.drop[Main.Square]"(ptr %data) nounwind {
entry:
  ; drop_value(Main.Square) emits nothing for i64 fields
  call void @tz.free(ptr %data)
  ret void
}

; FunctionEmitter::drop_value for a dyn value
  %data.1 = extractvalue %tz.dyn %shape, 0
  %vtable.1 = extractvalue %tz.dyn %shape, 1
  %drop = load ptr, ptr %vtable.1
  call void %drop(ptr %data.1)
```

- adapter の受け手変換: `ref 'a` は `llvm_type(ref T)` が `ptr` ならそのまま渡し、`T` が配列（`%tz.array`）なら
  `load %tz.array, ptr %data` の値を渡す。`ref mut 'a` は `ptr` をそのまま渡す。`'a` は `load` した値を、`@tz.free(ptr %data)`
  の後に渡す。
- 所有受け手の dispatch 関数は `%tz.dyn` を値で受け、`extractvalue` で data と vtable を取り出す。dyn 値の drop は出さない。
- `insertvalue` の初期値、関数属性（`nounwind` など）、linkage は既存の helper 生成（`emit_builtin` の `define internal`）に合わせる。
- vtable・drop 関数・adapter は `emit_program` の最後に `Globals::vtables` の順で出し、同じ記号は一度だけ出す。

### アルゴリズム

```text
vtable_classes(C):                         # 後順の深さ優先。最初の出現だけ残す
    order = [], seen = {}
    visit(X):
        if X in seen: return
        seen.add(X)
        for S in X.superclasses (宣言順): visit(S.class)
        order.push(X)
    visit(C)
    return order                           # 最後が C

dyn_slots(C):                              # Classes::collect の最後に全クラスで計算する
    if C.arity != 0: return Err("{C} is higher-kinded")
    slots = []
    for X in vtable_classes(C):
        if X != C and X.arity != 0: return Err("superclass {X} is higher-kinded")
        if X != C and X.methods is empty:
            return Err("superclass {X} has no methods, so a dyn value cannot satisfy it")
        for (i, m) in X.methods (宣言順。既定メソッドを含む):
            sig = m.signature, or return Err("method {X}.{m} has an invalid signature")
            v = Type::Variable(X.variable)
            if sig.parameters is empty or sig.parameters[0] not in {v, Reference(v, false), Reference(v, true)}:
                return Err("method {X}.{m} does not take the class type by value, ref, or ref mut as its first parameter")
            if v occurs in sig.parameters[1..] or in sig.result:
                return Err("method {X}.{m} uses the class type outside its first parameter")
            slots.push((X, i))
    if slots is empty: return Err("{C} has no methods to dispatch")
    return Ok(slots)

dyn_instances(modules):                    # Classes::instances の前
    for C in 正規化した Program::dyn_classes の集合 (名前順):
        if dyn_slots(C) is Err: continue   # E1028 は各出現で報告済み
        for X in vtable_classes(C):
            InstanceDecl X<dyn C>:
                X の各メソッド m (宣言順): parameters (_receiver, _argument1, ...)
                body ExprKind::DynDispatch { slot: dyn_slots(C) 内の (X, m) の位置, slots: dyn_slots(C) の長さ }

specialize Dyn.of (argument: T, result: dyn C), T は具体型:
    if (C, T) not in vtables:
        ids = []
        for (X, i) in dyn_slots(C):
            TypedExprKind::Method(X, i, T) を既存の特殊化経路で解決する
            FunctionRef::User(id) と型引数  -> ids.push(Specializer::request(id, types, span)?)
            FunctionRef::Builtin(..)        -> E1028 "cannot store {T} in dyn {C}: ..."
        vtables[(C, T)] = ids
```

- 合成インスタンスの関数は既存の命名（`$instance.{function_id}.{method}`）になる。所属モジュールはクラスの宣言モジュールで、組み込み
  クラスは `dyn` が最初に現れたモジュールとする。`src/derive.rs` の生成インスタンスと同じく、ソース種別の検査の後に渡す。
- `dyn_of` は期待型を `Inference::resolve` して `Type::Dyn(C)` を確かめ、引数を期待型なしで検査し、`Constraint { class, ty: T, span }`
  をメソッド参照と同じ一覧へ積み、`BuiltinInstance { builtin: DynOf, types: [T, dyn C] }` を作る。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の検証済みサンプル 2 ファイルを `/tmp/tz-a14/sample/` に置き、IR を保存する。
- 確認: 次がすべて成功する。`run` は `29`、4 つのテストはそれぞれ一つの binary で `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
target/release/tsuzuri run /tmp/tz-a14/sample
target/release/tsuzuri build /tmp/tz-a14/sample --emit llvm -o /tmp/tz-a14/before.ll
target/release/tsuzuri build /tmp/tz-a14/sample --emit llvm -O3 -o /tmp/tz-a14/before-O3.ll
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: 予約語 `dyn`

- 変更: `src/lexer.rs` の `identifier` と `TokenKind::Dyn`、GUIDE §6.1 が挙げる予約語の箇所（`vsc/` の文法を含む）、
  `tests/dyn_dispatch.rs`（新規）。
- 内容: `"dyn" => TokenKind::Dyn` を足す。G19 が done なら、無条件の予約ではなく G19 の `EDITION_KEYWORDS`（新しい edition だけの
  予約）に `dyn` を登録する（G19 の D12）。型の位置ではまだ受けない。テストファイルには `tests/polymorphism.rs` の `accepts`
  （IR を 2 回出して一致を確認）と `rejects` を写し、`diagnostic(source) -> Diagnostic`（`analyze(source).expect_err(source)`）と
  IR を出さない `checks(source) -> CheckedModule`（`analyze` だけ）を足す。
- 確認: `cargo test --locked --test dyn_dispatch` が `1 passed`。`cargo test --locked` が成功する。

### 手順 3: 型 `dyn C`（共有変更）

- 変更: 「段ごとの変更」のうち、字句・合成インスタンス・`Dyn.of`・vtable 以外の行すべて。構文、整形、文書生成、LSP、`Type::Dyn` と型性質、
  `resolve_type`、型式の走査、`type_expression`、`llvm_type`、`canonical_type`、`storage_layout`、`drop_value`、`clone_value`、
  ヘッダーの `%tz.dyn`（`CheckedModule::uses_dyn`）、`src/llvm_frame.rs`、`src/llvm_debug.rs`。
- 内容: parser の `dyn` 分岐は修飾名を読むだけで `type_expr` を再帰呼び出ししない。`Program::dyn_classes` に記録する。
  `resolve_type` は D2 の正規名を作り、E1004 の 2 メッセージを出す。
- 確認: `cargo test --locked --test dyn_dispatch` が `6 passed`。`cargo test --locked` が成功する。手順 1 の stack-depth 3 テストが成功する。

### 手順 4: dyn 互換規則と E1028

- 変更: `src/polymorph.rs` の `Class`（`dyn_slots`）、`Classes::collect`、`vtable_classes`、`dyn_slots`。
- 内容: 全クラスで `dyn_slots` を計算し、各モジュールの `Program::dyn_classes` を出現順に検査して、`Err` なら E1028 を出す。
- 確認: `cargo test --locked --test dyn_dispatch` が `7 passed`。`cargo test --locked --test polymorphism` が成功する。

### 手順 5: 合成インスタンスと dispatch 関数

- 変更: `ExprKind::DynDispatch`・`TypedExprKind::DynDispatch`、式検査の `ExprKind` match、`dyn_instances` と `Classes::instances`、
  `emit_program` と `emit_dyn_dispatch`、`src/formatter.rs` の `unreachable!` arm。
- 内容: アルゴリズムの `dyn_instances` を実装する。引数名を `_` で始めて W1001 を避ける。`DynDispatch` の型は期待型（関数の結果型）。
  LLVM は通常の関数と同じ `define` 行を使い、本体だけを「生成 IR」の dispatch に置き換え、scope drop を出さない。
- 確認: `cargo test --locked --test dyn_dispatch` が `10 passed`。`cargo test --locked` が成功する。

### 手順 6: `Dyn.of` の型付けと所有権

- 変更: `Builtin::DynOf`（名前の対応表と逆引き、`scheme`）、適用式の builtin 分岐、`dyn_of`、`src/ownership.rs`。
- 内容: 「アルゴリズム」の `dyn_of` と D9 のとおり。値としての `Dyn.of` は E1015。ownership は Task の借用捕捉拒否と同じ形で `loans` を
  検査する（E1013）。LLVM はまだ `Dyn.of` を出せないので、この手順で足すテストは `checks` と `rejects` だけを使う。
- 確認: `cargo test --locked --test dyn_dispatch` が `14 passed`。`cargo test --locked --test tasks` と
  `cargo test --locked --test types_ownership` が成功する。

### 手順 7: vtable の要求

- 変更: `src/polymorph.rs` の `specialize`・`Specializer`（`TypedExprKind::Method` の解決経路、`instance_function`、
  `Specializer::request`）、`CheckedModule::vtables`。
- 内容: 「アルゴリズム」の `specialize Dyn.of`。組み込み実装は E1028。`Dyn.of` がない (C, T) は要求しない。
- 確認: `cargo test --locked --test dyn_dispatch` が `15 passed`。`cargo test --locked honors_the_exact_specialization_limit` が `1 passed`。

### 手順 8: LLVM の構築・vtable・adapter・drop

- 変更: `FunctionEmitter::call` の builtin 経路と `FunctionEmitter::dyn_of`、`Globals::vtables`、`emit_dyn_vtables`、`reachable_functions`。
- 内容: 「生成 IR とランタイム」のとおり。`reachable_functions` は `Dyn.of` の (C, T) から `module.vtables` の関数へ到達させる。
- 確認: `cargo test --locked --test dyn_dispatch` が `19 passed`。`cargo build --release --locked` の後、「例」の 3 行に変えた
  `Main.tz` と `Shapes.tt` を `/tmp/tz-a14/dyn/` に置き、`target/release/tsuzuri run /tmp/tz-a14/dyn` が `29`。

### 手順 9: E2E suite `dyn_dispatch`

- 変更: `tests/fixtures/dyn_dispatch/Shapes.tt`・`Main.tz`（新規）、`tests/features.mjs`（GUIDE §7.4 の手順で `const suites` に追加）。
- 内容: 「E2E」の表のとおり。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri dyn_dispatch` が native・WASM × `-O0`・`-O3`
  の全ケースで成功し、`live == 0`、WASM import なし。

### 手順 10: 型が増大する再帰

- 変更: `tests/dyn_dispatch.rs` だけ。
- 内容: `dyn_breaks_type_growing_recursion` を足す。
- 確認: `cargo test --locked --test dyn_dispatch` が `20 passed`。
  `cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion` が変わらず `1 passed`。

### 手順 11: 既存 IR の不変と生成コード

- 変更: なし。
- 内容: 手順 1 と同じサンプルで IR を作り直して比べ、手順 8 の `/tmp/tz-a14/dyn` の `-O3` IR で vtable と間接呼び出しを確かめる。
- 確認: `cmp` は何も出さない。`grep` の結果（vtable の定義、残った間接呼び出しの数と位置）を報告に書く。`-O3` では LLVM が定数 vtable
  経由の呼び出しを直接呼び出しへ畳むことがあり、その場合もそのまま報告する。速さは主張しない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-a14/sample --emit llvm -o /tmp/tz-a14/after.ll
target/release/tsuzuri build /tmp/tz-a14/sample --emit llvm -O3 -o /tmp/tz-a14/after-O3.ll
cmp /tmp/tz-a14/before.ll /tmp/tz-a14/after.ll
cmp /tmp/tz-a14/before-O3.ll /tmp/tz-a14/after-O3.ll
target/release/tsuzuri build /tmp/tz-a14/dyn --emit llvm -O3 -o /tmp/tz-a14/dyn-O3.ll
grep -n 'tz.vtable\|call i64 %' /tmp/tz-a14/dyn-O3.ll
```

### 手順 12: ドキュメント

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md _docs/language-reference/generics-and-typeclasses.md _docs/guides/from-fsharp.md _docs/feature-status.md`
  が成功する。

### 手順 13: 最終確認

- 変更: なし。
- 確認: GUIDE §2.3 の全コマンド（fmt・clippy を含む）、`cargo test --locked`、`node tests/features.mjs target/release/tsuzuri`（全 suite）、
  手順 1 の stack-depth 3 テストと `honors_the_exact_specialization_limit`、`git diff --check` がすべて成功する。

## テスト計画

### Rust テスト

`tests/dyn_dispatch.rs`（新規）。ソースは単一ソースの `analyze` に渡す（`tests/polymorphism.rs` と同じく、クラスも同じソースに書ける）。
既存の診断コードに依存する確認は、同種の既存の拒否との相対比較にする。括弧内は追加する手順。

- `dyn_is_reserved_keyword`（2）: `let dyn = 1\n()` の診断コードが `let match = 1\n()` と同じで、`E0002`。
- `dyn_types_parse_and_display`（3）: `tsuzuri::parser::parse` で `ref dyn Shape`・`[dyn Shape]`・`Maybe<dyn Shape>` を一つずつ含む
  ソースの `program.dyn_classes.len()` が 3。`let x: dyn Shape = 42` は `E1003` でメッセージが `dyn` と `Shape` を含む。
  `dyn Shape<i64>` と `let x: dyn = 1` は `E0002`。
- `type_stays_four_words`（3）: `std::mem::size_of::<Type>() <= 4 * std::mem::size_of::<usize>()`。
- `dyn_names_must_be_classes`（3）: `dyn Nope` は `E1004` で `unknown type class` を含む。record `Square` への `dyn Square` は `E1004` で
  `is not a type class` を含む。
- `programs_without_dyn_emit_no_dyn_ir`（3）: 「再現」のサンプルを単一ソースにした IR が `tz.dyn` も `tz.vtable` も含まない。
- `dyn_in_exports_and_externs_is_rejected`（3）: `export def f :: ref (dyn Shape) -> i64 = \s -> 0` は `E1008`。extern の引数の
  `dyn Shape` は拒否される（`analyze(...).is_err()`）。
- `rejects_dyn_incompatible_classes`（4）: 次の各クラスへの `dyn` が `E1028` で、メッセージが括弧内を含む。
  `class Same<'a> { def same :: ref 'a -> ref 'a -> bool }`（`uses the class type outside its first parameter`）、
  `class Make<'a> { def make :: i64 -> 'a }` と `class Late<'a> { def late :: i64 -> ref 'a -> i64 }` と
  `class Nested<'a> { def nested :: ref ['a] -> i64 }`（`does not take the class type`）、`class Tag<'a> {}` と `dyn Copy`
  （`has no methods to dispatch`）、`dyn Eq`・`dyn Ord`・`dyn Add`（`uses the class type outside its first parameter`）、
  `class Eq<'a> => Keyed<'a> { def key :: ref 'a -> i64 }`（`Eq.eq`）、`class Copy<'a> => Plain<'a> { def plain :: ref 'a -> i64 }`
  （`superclass Copy has no methods`）、`_docs/language-reference/higher-kinds.md` の高階型クラス（`higher-kinded`）。
- `superclass_and_default_methods_are_dispatched`（5）: `class Named<'a> { def id :: ref 'a -> i64 }`、既定メソッド
  `describe`（`Shape.area s * 1000 + Named.id s`）を持つ `class Named<'a> => Shape<'a>`、
  `def total :: ref (dyn Shape) -> i64 = \s -> Shape.describe s + Named.id s` の IR に、`{ ptr, i64, i64, [3 x ptr] }` の
  getelementptr が slot 0（`Named.id`）と slot 2（`describe`）で現れる。
- `user_instances_for_dyn_heads`（5）: 別クラス `class Tagged<'a> { def tag :: ref 'a -> i64 }` の `instance Tagged<dyn Shape>` は受理され、
  `instance Shape<dyn Shape>` は `E1016`。
- `generic_functions_instantiate_once_for_dyn`（5）: `Shape` のインスタンスを持つ record 5 個と `def area_of :: Shape<'a> => ref 'a -> i64`。
  `area_of` を `ref (dyn Shape)` の引数だけで呼ぶ版の IR では `area_of` を含む `define` 行が 1、5 型それぞれで直接呼ぶ版では 5。
  関数記号に元の名前が入らない場合は `CheckedModule` の関数名で数える。
- `dyn_of_needs_expected_dyn_type`（6）: `let s = Dyn.of (Square { side: 1 })`、`let f = Dyn.of`、`Square { side: 1 } |> Dyn.of` がすべて
  `E1015`。
- `dyn_of_needs_an_instance`（6）: `Shape<i64>` のない `let s: dyn Shape = Dyn.of 42` の診断コードが、`let n = 42` の後の
  `Shape.area (ref n)` と同じ。
- `dyn_rejects_borrowed_data`（6）: `tests/tasks.rs` が Task への借用捕捉を拒否している値と同じ形（借用を捕捉した関数値を持つ record）を
  `Dyn.of` に渡すと `E1013` で、`cannot hold borrowed data` を含む。
- `dyn_values_are_owned`（6）: `Dyn.of square` の後の `square` と、dyn 値の二度目の move は `E1012`。`Copy<'a>` 制約の関数へ dyn 値を
  渡したときのコードは `string` を渡したときと同じ。再利用可能な closure が dyn 値を捕捉したときのコードは `Task` 値を捕捉したときと同じ。
  `Task.run` の本体が dyn 値を捕捉すると拒否される。
- `builtin_instances_cannot_fill_vtables`（7）: `let shown: dyn Display = Dyn.of 42` は `E1028` で `built into the compiler` を含む。
  `deriving (Display)` の record を `dyn Display` に入れるソースは `checks` で受理される。
- `vtables_are_unique_and_deterministic`（8）: 二つの関数がそれぞれ `Dyn.of (Square ..)` と `Dyn.of (Rect ..)` を作る。IR で
  `@"tz.vtable.` で始まる定義は 2 行、`@"tz.dyn.drop[` の定義は型ごとに 1 つ。`accepts` の 2 回の出力が一致する。
- `accepts_dyn_values_in_collections`（8）: `[dyn Shape]`、`Maybe<dyn Shape>`、record field の `dyn Shape`、`Vec.empty` を
  `Vec<dyn Shape>` として返す関数、`dyn Shape` の field を持つ record をさらに `Dyn.of` で包む入れ子を `accepts`。
- `display_and_hash_dispatch_user_instances`（8）: `deriving (Display, Hash)` の record を `dyn Display`・`dyn Hash` に入れて
  `to_string`・`Hash.hash` を呼ぶソースを `accepts`。IR に `@"tz.vtable.Display[` がある。
- `deriving_rejects_dyn_fields`（8）: `record Holder { shape: dyn Shape } deriving (Eq)` の診断コードが
  `record Holder { f: i64 -> i64 } deriving (Eq)` と同じ。
- `dyn_breaks_type_growing_recursion`（10）: `tests/polymorphism.rs` の `bounds_type_growing_polymorphic_recursion` と同じ形（値を包み
  続ける再帰）で、包む型に `instance Shape<'a> => Shape<Wrap<'a>>` を与える。再帰呼び出しの引数を `Dyn.of` で `dyn Shape` にした版は
  `accepts`、`Dyn.of` を外した版は元のテストと同じ `E1017`。

### E2E

`tests/fixtures/dyn_dispatch/`（新規）を GUIDE §7.4 の手順で `tests/features.mjs` の suite `dyn_dispatch` にする。`Shapes.tt` は次の
とおり（検証済み: `Square` の 2 インスタンスだけを持つ `Main.tz` で `Shapes.Shape.describe (ref square)` が `9001`）。

```tsuzuri
class Named<'a> {
    def id :: ref 'a -> i64
}

class Named<'a> => Shape<'a> {
    def area :: ref 'a -> i64
    def grow :: ref mut 'a -> i64 -> unit
    def into_area :: 'a -> i64
    def describe :: ref 'a -> i64 = \shape -> Shape.area shape * 1000 + Named.id shape
}
```

`Main.tz` は `Named`・`Shape` のインスタンスを次の型に与える。括弧内は `id` と `area`。`grow` は `Square` だけ side を `k` 倍にし、
他の型は値を変えない。`into_area` は `area` と同じ値を返して値を消費する。

- `Square { side }`（1、`side * side`）、`Rect { width, height }`（2、`width * height`）、`Label { text: string, weight }`（3、`weight`）
- `Bag { items: [dyn Shapes.Shape] }`（4、要素の `area` の和）、`Faulty { divisor }`（5、`assert (divisor != 0)` の後に `100 / divisor`）
- `Wide { low: i64, pad: i128 }`（6、`low`）、`[i64]`（7、要素の和）

export はすべて `i64 -> i64`。期待値は式から JS の `BigInt` で計算する（コンパイラの出力から作らない）。

| export | 内容 | 入力 x | 期待 |
| --- | --- | --- | --- |
| `total_area` | `[dyn Shape]` に `Square { side: x }`・`Rect { width: x, height: x + 1 }`・`Label { text: "label", weight: 10 * x }` を入れ、`area` の和 | 0, 3, -2, 10 | `2*x*x + 11*x`: 0, 51, -14, 310 |
| `describe_total` | 同じ配列で既定メソッド `describe` の和 | 0, 3, -2 | `2000*x*x + 11000*x + 6`: 6, 51006, -13994 |
| `generic_total` | 同じ配列の各要素に `area_of`（`'a = dyn Shape`）を呼んだ和 | 0, 3, -2 | `2*x*x + 11*x`: 0, 51, -14 |
| `names` | 同じ配列でスーパークラスの `Named.id` の和 | 0, 3 | `6`: 6, 6 |
| `bag_area` | `Bag { items: [Square { side: x }, Rect { width: 1, height: x }] }` を dyn に包んで `area` | 0, 5, -3 | `x*x + x`: 0, 30, 6 |
| `into_area` | 所有受け手 `into_area` で `Label { text: "owned", weight: x }` の dyn 値を消費 | 7, 0, -1 | `x`: 7, 0, -1 |
| `grown_area` | `Square { side: x }` の dyn 値に `grow (ref mut ..) 2` の後で `area` | 3, 0, -5 | `4*x*x`: 36, 0, 100 |
| `maybe_area` | `x > 0` なら `Some boxed`（`Square { side: x }` の dyn）、それ以外は `None`。match で `area` か -1 | 4, 0, -2 | 16, -1, -1 |
| `array_area` | `[x, x + 1, x + 2]` を dyn に包んで `area`（共有受け手が `%tz.array`） | 1, 0, -1 | `3*x + 3`: 6, 3, 0 |
| `wide_area` | `Wide { low: x, pad: 0 }` を dyn に包んで `area`（16 byte 整列の data 領域） | 9, -4 | `x`: 9, -4 |
| `faulty_area` | `Faulty { divisor: x }` を dyn に包んで `area` | 4, 5, 0 | 25, 20, トラップ（`assert`） |
| `display_agrees` | `deriving (Display)` の record を dyn 経由で表示した文字列と直接表示した文字列が等しければ 1 | 0, 42, -7 | 1, 1, 1 |
| `hash_agrees` | `deriving (Hash)` の record の `Hash.hash` を dyn 経由と直接で比べ、等しければ 1 | 0, 42 | 1, 1 |

- native・WASM × `-O0`・`-O3` の 4 構成で全ケースを実行し、トラップ以外の各呼び出しの後に `live == 0` を確かめる。WASM import は空（D-18）。
- トラップケースは suite の既存のトラップ期待の書き方に従う。IR の決定性（`build --emit llvm` の 2 回の一致）は GUIDE §7.4 の既定の確認に従う。

### 既存テストへの影響

- なし。予約語を列挙する既存のテストや文法があれば `dyn` を足すのは正当な変更とする。それ以外の期待値の変更が必要なら停止する。
- `dyn` を書かない入力の IR は変わらない（手順 11 と `programs_without_dyn_emit_no_dyn_ir`）。

### 性能

- 実装後に、同じ仕事量（N 個の図形の `area` の和）を `dyn` 配列、union の match、ジェネリック関数（単相化）で書いた 3 版を `-O3` の native で
  比べる。`docs/benchmarks.md` の手順（9 回以上、中央値とばらつき、環境情報）で生データを `target/perf/A14/` に保存して記録する。
- コードサイズは同じ 3 版の object を `/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d` と `wc -c` で比べる。
- CI に速度のしきい値は置かない。測定前に速さや小ささを主張しない（AGENTS.md）。

## ドキュメント

- `docs/language.md`: 予約語の列挙（`grep -n "deriving" docs/language.md` で見つかる一覧）に `dyn`。型と型クラスの説明に `dyn C`・`Dyn.of`・
  dyn 互換規則・性質表。診断の一覧（`grep -n "E1027" docs/language.md`）に E1028 の行。
- `docs/architecture.md`: `%tz.closure` の説明（`grep -n "tz.closure" docs/architecture.md`）の近くに `%tz.dyn`、vtable、adapter、drop slot、
  `CheckedModule::vtables`、合成インスタンス。
- `_docs/language-reference/generics-and-typeclasses.md`: `## 制限` の前に `## 動的ディスパッチ（dyn）` を足し、「例」を
  `tsuzuri project=dyn file=Shapes.tt` と `tsuzuri project=dyn file=Main.tz run=29` の実行サンプルにする。`## 制限` に Phase 1 の制限
  （`Send`・借用・clone・組み込み実装の slot がない）を書く。
- `_docs/guides/from-fsharp.md`: インターフェースの説明（`grep -n "インターフェース" _docs/guides/from-fsharp.md`）に `dyn C` との対応。
- `_docs/feature-status.md` と `_features/README.md`: A14 の状態（両者の ID の順を保つ）。
- `README.md`: 言語機能の一覧に型クラスの行があれば（`grep -n "型クラス" README.md`）、`dyn` を一行足す。

## 受け入れ条件

- [ ] D1 が承認されている。
- [ ] `dyn` が予約語になり、`let dyn = 1` が既存の予約語と同じ `E0002` になる。
- [ ] dyn 互換なクラスの値を `[dyn C]`・`Maybe<dyn C>`・record field に格納し、自身・スーパークラス・既定メソッドを `ref`・`ref mut`・
  所有の受け手で呼べる（E2E）。
- [ ] 非互換なクラスを理由付きの `E1028` で拒否し、組み込み実装に解決される slot も `E1028` で拒否する。
- [ ] 制約付きジェネリック関数が `'a = dyn C` で一度だけ具体化される。
- [ ] vtable は (C, T) ごとに一つで、名前は `canonical_type` から決まり、IR は 2 回のビルドで一致する。
- [ ] `dyn_dispatch` suite が native・WASM × `-O0`・`-O3` で成功し、`live == 0`、WASM import なし。
- [ ] 既存の単相化経路の IR と性能が変わらない: `dyn` を書かないプログラムの IR が変更前と byte 単位で一致する（手順 11 の `cmp`）。
- [ ] stack-depth の 3 テストと `honors_the_exact_specialization_limit` が成功し、上限と stack サイズを変えていない。
- [ ] 「ドキュメント」の更新が `node scripts/check-docs.mjs` を通る。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `_ =>` fallback の黙った誤り: `is_copy`（true）、`can_capture`・`can_send`（true）、`needs_drop`（false）は `Type::Dyn` の arm を忘れても
  compile が通る。`dyn_values_are_owned` と E2E の `live == 0` で検出する。`drop_value` の arm を忘れると data 領域が漏れる。
- 網羅 match（`llvm_type`・`canonical_type`・`storage_layout`・`Type::display`）は compile error で気づけるが、`unreachable!` で埋めない。
- クラス名の表記揺れ: `Shape` と `Main.Shape` が別の `Type::Dyn` になると `expected dyn Shape, found dyn Main.Shape` のような E1003 になる。
  常に `Class::name` を使う。`dyn_types_parse_and_display` と E2E で検出する。
- vtable の slot 順を宣言順以外（`HashMap` の反復など）で決めると IR が非決定的になる。vtable・adapter・drop 関数は `BTreeSet`・`BTreeMap`
  の順で出す。
- WASM の `call_indirect` は関数型を検査する。adapter の引数・結果の LLVM 型が dispatch 側の `call` と一か所でも違うと WASM だけが実行時に
  トラップし、native は未定義動作のまま通る。WASM の E2E を必ず回す。
- 共有参照の受け手は、配列だと `ptr` ではなく `%tz.array` になる（`llvm_type` の `shared_array_element`）。adapter で `load` しないと壊れる。
  E2E の `array_area` で検出する。
- 所有受け手: adapter は data 領域を解放するが値は drop しない。dispatch 関数も dyn 値を drop しない。どちらかを誤ると二重解放か漏れになる
  （`into_area` の `live == 0`）。
- dispatch 関数の本体を通常の emission に通すと、所有引数を scope 終了で drop して呼び先と二重解放になる。`emit_dyn_dispatch` だけで出す。
- `%tz.dyn` をヘッダーへ常に出すと全プログラムの IR が変わる。`uses_dyn` で出し分け、`programs_without_dyn_emit_no_dyn_ir` と手順 11 で
  検出する。
- vtable が指す関数を `reachable_functions` に入れ忘れると、LLVM が未定義の関数記号で失敗する。`Dyn.of` を含む関数からだけ到達する
  インスタンス（直接呼び出しがない型）で確かめる。
- 特殊化上限: `Dyn.of` がない (C, T) の slot 関数を先回りして要求しない。std のジェネリックを先に特殊化すると利用者の 1,024 を消費する
  （`honors_the_exact_specialization_limit`）。
- stack: `dyn_of` は適用式の検査から呼ぶ小さな非再帰 helper にし、引数検査の入口に処理を足さない。parser の `dyn` 分岐は `type_expr` を
  再帰呼び出ししない。`TypeExprKind::Dyn`・`ExprKind::DynDispatch` を大きくしない。
- 整列: `i128` のような 16 byte 整列の値を data 領域に置く。`@tz.alloc` の整列が足りないと `-O3` の native で落ちうる（E2E の `wide_area`）。
- data 領域の大きさは `storage_layout`（64-bit の上界）で決める。host の `size_of` や wasm32 の実寸で決めない。
- `dyn C` の drop で data の型ごとの drop（所有文字列など）を呼び忘れると漏れる。drop slot は必ず `drop_value(T)` と同じ glue を通す
  （`into_area`・`total_area` の `Label`）。
- 既定メソッド（A06）とスーパークラスのメソッドも slot を持つ。`describe_total` と `names` で検出する。
- 導出インスタンスのメソッドが `FunctionRef::Builtin` に解決される場合は、`display_and_hash_dispatch_user_instances` が E1028 で失敗する。
  その場合は D6 の前提が崩れるので停止して報告する。
- debug 情報付きの出力で、dispatch 関数・adapter・drop 関数に `!dbg` のない call が混ざると LLVM の検証が失敗しうる。既存の helper と同じく
  DISubprogram を付けない。
- `clone_value` の `unreachable!` に到達したら、その経路を報告して止まる（停止条件）。clone slot を足して回避しない。

## 対象外

- downcast、実行時型情報、継承、複数ディスパッチ。
- Phase 2 の項目（`Send` を満たす dyn、借用を格納する dyn、複数クラス、アップキャスト、clone、組み込み実装の slot）。
- dyn 値の比較と順序（`Eq`・`Ord` は非互換）、host ABI での受け渡し、高階型クラスの dyn。

## 決定事項

### D1: 構文と構築

- 決定: 型は予約語 `dyn` とクラス名（`dyn Shapes.Shape`）。構築は期待型が `dyn C` の位置での `Dyn.of value` だけで、暗黙の変換はしない。
  dyn の規則違反は `E1028`（GUIDE D-30 の仮割り当て）。
- 理由: 代替の `Dyn<Shape>` は D-02 の「型クラス名は制約」の分類に例外が要る。暗黙変換は確保の位置を隠す。`dyn` は Tsuzuri のソースで未使用。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: 型の内部表現

- 決定: `Type::Dyn(Box<str>)`。文字列はクラスの `Class::name`（`Classes::names` のキー）と同一にする。`resolve_type` は
  `ConstraintName::Class` の解決と同じ修飾規則で正規名を作る。`Names` がクラスを型名として引けない場合（HEAD の E1004 が示すとおり）は、
  クラス名の表を `Names` に持たせる。
- 理由: 葉の variant なので `map_type`・`substitute`・`Inference::resolve`・`bounded_type` の fallback が正しいまま使え、表示と D-03 名も
  自己完結する。`Box<str>` は 2 語で、`Type` は 4 語のまま。class id にすると、表示と解決順（record の型はクラス収集より前に解決する）の
  ために `TypeContext` の変更が要る。
- 状態: 既定案（実装者はこの案に従う）

### D3: 値の表現と確保

- 決定: `%tz.dyn = type { ptr, ptr }`（data, vtable）。ヘッダーへは `CheckedModule::uses_dyn` のときだけ出す。data は
  `@tz.alloc(storage_layout(T).0)` で確保し、値を `llvm_type(T)` で格納する。
- 理由: 既存の固定ヘッダーを変えず、`dyn` を書かないプログラムの IR を byte 単位で保つ。`storage_layout` は wasm32 の上界なので両 target に
  同じ定数を出せる。
- 状態: 既定案（実装者はこの案に従う）

### D4: vtable・adapter・drop 関数

- 決定: vtable は `{ ptr drop, i64 size, i64 align, [N x ptr] slots }` の `internal unnamed_addr constant` で、(C, T) ごとに一つの
  `@"tz.vtable.{C}[{T}]"`。slot はすべて adapter `@"tz.dyn.slot.{X}.{m}[{T}]"` を指し、adapter は `(ptr, A1, ..., An) -> R` の同一型で
  受け手を変換してインスタンスメソッドを直接呼ぶ。drop slot は `@"tz.dyn.drop[{T}]"` で、`drop_value(T)` の後に `@tz.free`。
  `{T}` は `canonical_type(T)`。すべて `emit_program` の最後に `BTreeSet` の順で出す。
- 理由: 受け手の ABI（`ptr`・`%tz.array`・値）を adapter に閉じ込めると、dispatch 側の間接呼び出しの型がクラスとメソッドだけで決まり、
  WASM の型検査と一致する。adapter は直接呼び出し一つなので `-O3` で呼び先を inline できる。`[N x ptr]` にすると slot の getelementptr が
  一つの形になる。
- 状態: 既定案（実装者はこの案に従う）

### D5: slot の順序

- 決定: `vtable_classes(C)`（スーパークラスを宣言順に後順で深さ優先に辿り、最初の出現を残し、最後に `C`）の順に、各クラスのメソッドを
  宣言順に並べる。既定メソッドも slot を持つ。
- 理由: 宣言順だけで決まるので決定的。スーパークラスのメソッドが先に並ぶので、同じスーパークラスの slot 群はどの vtable でも同じ相対順になる。
- 状態: 既定案（実装者はこの案に従う）

### D6: dyn 互換規則と Phase 1 の範囲

- 決定: 「アルゴリズム」の `dyn_slots` の規則（高階型クラス不可、各メソッドの第 1 引数が `'a`・`ref 'a`・`ref mut 'a` で他に `'a` が
  現れない、メソッドのないスーパークラス不可、メソッドが一つもないクラス不可）。組み込みクラスも同じ規則で判定するので、`Display`・`Hash`
  は互換、`Eq`・`Ord`・`Add`・`Copy` などは非互換。Phase 1 の slot は利用者関数（利用者・導出・既定メソッドのインスタンス）だけで、組み込み
  実装（`Display<i64>` など）に解決される `Dyn.of` は `E1028`。
- 理由: 規則は元の既定案どおり。組み込み実装は関数記号を持たず adapter から直接呼べないため、別の生成経路が要る（Phase 2）。導出インスタンスは
  `src/derive.rs` の `instances` が関数として生成するので slot に入れられる。
- 状態: 既定案（実装者はこの案に従う）

### D7: 合成インスタンスと一回の具体化

- 決定: `Program::dyn_classes` に現れた互換クラス `C` ごとに、`vtable_classes(C)` の各クラス `X` について `X<dyn C>` の `InstanceDecl` を
  合成し、`Classes::instances` の既存経路（関数名 `$instance.{function_id}.{method}`）へ流す。本体は `ExprKind::DynDispatch { slot, slots }`
  で、LLVM では `emit_dyn_dispatch` が本体を出す。ジェネリック関数は通常の制約解決で `'a = dyn C` に一度だけ具体化される。
- 理由: 関数 id・型検査・特殊化・到達性・出力の既存経路をそのまま使える。`dyn` を書いたクラスだけを合成するので、他のプログラムの関数表は
  変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D8: 所有権と性質

- 決定: `dyn C` は非 Copy、`needs_drop`、`can_capture` false、`can_send` false、`exportable` false。借用を持つ値の `Dyn.of` は E1013。
  clone slot は作らず、`clone_value` は `unreachable!`。
- 理由: 元の既定案（非 Copy、drop が必要、借用を格納しない、`Send` なし）に従う。closure 環境の clone には clone slot が要るので、
  `can_capture` は `Task` と同じく false にする。clone slot は Phase 2。
- 状態: 既定案（実装者はこの案に従う）

### D9: `Dyn.of` の型付け

- 決定: `Dyn.of` は一引数の直接適用に限り、その時点で期待型が `dyn C` に解決していなければ E1015。値としての参照と pipe の右辺も E1015。
  引数は期待型なしで検査し、`C<T>` をメソッド参照と同じ制約の一覧へ積む。
- 理由: 期待型からの後追い推論（遅延制約）を持ち込まずに、「期待型が `dyn C` の位置だけ」という既定案を満たせる。
- 状態: 既定案（実装者はこの案に従う）

### D10: B07 との関係

- 決定: drop slot は `FunctionEmitter::drop_value` と同じ drop glue を呼んでから data を解放する。B07 は開始条件にしない。
- 理由: B07 が drop glue に利用者 `Drop.drop`（`ref mut`、field より先）を加えれば、A14 は変更なしで従う。
- 状態: 既定案（実装者はこの案に従う）

### D11: E1028 の報告位置

- 決定: parser が `Program::dyn_classes` に記録した `dyn C` の各出現で、`Classes::collect` の最後に報告する。組み込み実装に解決される slot は、
  `Dyn.of` の特殊化時にその呼び出し位置で報告する。
- 理由: 型式が関数本体の注釈・record field・型別名のどこにあっても一つの経路で漏れなく報告でき、報告順も出現順で決定的になる。
- 状態: 既定案（実装者はこの案に従う）
