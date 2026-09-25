# A09: 名前付きライフタイムと借用フィールド

| 項目 | 内容 |
|---|---|
| ID | A09 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | – |
| 後続 | C03, E05, A10 以降の高度な型機能 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/types_ownership.rs`, `tests/control.rs`, `tests/primitives.mjs` |

## 目的

借用を含むレコードを安全に表現し、複数の borrowed input/output の関係を明示できるようにする。短期的には `record View { data: &[i64] }` のような「借用 view」を可能にし、中期的には複数入力からどれを返すかを名前付きライフタイムで表せるようにする。

現在の寿命省略は「borrowed result がある関数は borrowed input がちょうど 1 個」という制約で安全性を保っている。A09 はその保守的な安全性を壊さず、段階的に表現力を増やす。

## 現状

- `src/check.rs::Signature::validate_borrows` は `self.result.contains_reference()` かつ `parameters` のうち `carries_loans(records)` が 1 個でなければ `E1013` を返す。
- `src/check.rs::Type::contains_reference` は `Reference`, `Array`, `List`, `Tuple` を見るが、現状 `Record` へは再帰していない。これは `check_modules` が record field に参照を禁止しているため成立している。
- `src/check.rs::check_modules` の record field 処理は `field.mutable || ty.contains_reference()` なら `E1013`:
  `"record fields are immutable owned values; borrowed fields require lifetime parameters, which are not supported"`
- `src/check.rs::Type::carries_loans` は `Record(id)` の fields へ再帰する。つまり record field 参照を解禁すれば、既存 loan 追跡はかなり使える。
- `src/ownership.rs` は `Loan { place, mutable, parents }` を持ち、`Value { loans }` を local に保存する。`check_body` は borrowed parameter に external loan を作り、戻り値が external 以外を指すと `E1013`。
- `ownership.rs::closed_returns` は owned 判定で `Record(id)` fields へ再帰する。参照 field を持つ record は closed ではなくなる。
- `ownership.rs::read_places` は `expression.ty.carries_loans(records)` の場合、local に保存された loans を値へ伝播する。nested borrowed values に explicit lifetime がない場合は `E1013` を出す箇所がある。
- `ownership.rs::eval_value` の `Assign` は、参照経由の代入で borrowed value を保存する場合 `E1013 "assigning borrowed values through references requires explicit lifetimes"`。
- `ownership_control.rs::finish_control_scope` は pattern/iteration binding 由来の borrow が外へ出ると `E1013`。
- `&'a T` のような Rust-style lifetime は現在の型変数構文 `'a` と字句的に完全衝突する。

## 仕様

### フェーズ分割

| フェーズ | 内容 | このチケットでの精度 |
|---|---|---|
| 1 | 借用フィールド（単一の省略 region） | 完全指定・実装対象 |
| 2 | 名前付き region 構文と関数シグネチャ | 設計指定 |
| 3 | record region parameter と複数入力関係 | 設計指定 |
| 4 | 高度な推論・一時値寿命延長 | 対象外寄りの将来 |

### フェーズ 1: 借用フィールド（単一省略 region）

既存構文のまま、record field に共有参照を許可する。

```text
record View { data: &[i64], title: &string }
```

規則:

- field 型に `&T` を許可する。
- field 型に `&mut T` はフェーズ 1 では禁止。`E1014` または既存メッセージに近い `E1013` で拒否する。既定は `E1013`。
- record 値は field の loan set を保持する。所有者より長生きできない。
- 参照 field を持つ record を返す関数は、既存省略規則と同じく borrowed input がちょうど 1 個なら受理し、その input に結び付ける。
- borrowed input が 0 個または 2 個以上の関数から borrowed record を返すと `E1013`。
- borrowed record を task に捕捉/返却することは `Send`/`contains_reference` により `E1013`。
- borrowed record を reusable closure に捕捉する場合、参照先の寿命内だけ許可される。既存 closure loan 検査に従う。
- borrowed record の field を通じて借用した値も、record 所有者と元所有者の両方より長生きできない。

例:

```text
record View { data: &[i64] }

def view :: &[i64] -> View
fn view data = View { data: data }

def first :: View -> i64
fn first v = v.data[0]

def main :: i64
fn main = {
    let values = [10, 20];
    let v = view (&values);
    first v
}
```

拒否:

```text
record View { data: &[i64] }

def bad :: View
fn bad = {
    let values = [1, 2];
    View { data: &values }   // E1013
}
```

複数入力はフェーズ 1 では拒否:

```text
record PairView { left: &string, right: &string }
def pair :: &string -> &string -> PairView
fn pair a b = PairView { left: a, right: b } // E1013
```

### フェーズ 2: 名前付き region 構文

Rust-style `&'a T` は `TypeVariable("a")` と衝突するため採用しない。A09 の region は braces sigil を使う。

```text
borrow-type ::= "&" region? type-atom
              | "&" "mut" region? type-atom

region      ::= "{" region-name "}"
region-name ::= ASCII lower identifier, "_" 以外
```

例:

```text
def first {r} :: &{r} [i64] -> &{r} i64
fn first values = &values[0]

def choose {r} :: bool -> &{r} string -> &{r} string -> &{r} string
fn choose flag a b = if flag { a } else { b }
```

`&{r} T` は `'r` 型変数と衝突せず、既存 lexer の `{`/`}` token を使える。`&mut{r} T` と `&mut {r} T` の両方を受理してよいが、docs では `&mut {r} T` を推奨する。

### フェーズ 3: record region parameter

record 宣言で region parameter を明示できる。

```text
record View {r} { data: &{r} [i64] }
record PairView {a b} { left: &{a} string, right: &{b} string }

def view {r} :: &{r} [i64] -> View {r}
fn view data = View { data: data }
```

型引数 `'a` と region 引数 `{r}` は別 namespace。A01 の型適用 `Box<'a>` と混ざる場合:

```text
record RefBox<'a> {r} { value: &{r} 'a }
def make {r} :: &{r} i64 -> RefBox<i64> {r}
```

型パラメーター／型引数は型名直後の `<...>`、region application はその後の `{...}` とする。型引数がなければ `View {r}` のまま。これは型式であり、record literal `View { field: value }` は式なので parser context で区別できる。

## 設計

### フェーズ 1 の最小データ構造変更

フェーズ 1 では `Type` に lifetime parameter を追加しない。既存 `Type::Reference(Box<Type>, bool)` と `Value.loans` で runtime-free な寿命を追跡する。

必須変更:

```rust
impl Type {
    pub fn contains_reference(&self, records: &[CheckedRecord]) -> bool;
}
```

現在の `contains_reference(&self)` は `Record` を見られない。record field 参照を許可するには、`records` を受け取る版へ変更するか、別名 `contains_reference_with_records` を追加する。

更新箇所:

- `Signature::validate_borrows`
- `validate_size` の `Type::Task(result)` チェック
- `ownership.rs::closed_returns::owned`
- `Type::can_send` は既に Record に再帰するので確認のみ
- `Type::contains_mutable_reference` も `Record` に再帰する版へ拡張する

record field 検査:

```rust
if field.mutable {
    E1013
}
if ty.contains_mutable_reference(records) {
    E1013 // phase 1 では &mut field 禁止
}
// shared reference は許可
```

ただし record の全 field 型を解決する前に `records` が未完成なので、二段階にする:

1. field 型を `Type` に解決して `fields` へ格納。
2. 全 `CheckedRecord` を作った後、`validate_record_borrows(record_id)` で mutable reference を検査。

### フェーズ 1 の所有権

既存の `Value.loans` propagation を利用する。

- `TypedExprKind::Record(fields)` は `ownership.rs::eval_value` で各 field の `Value.loans` を `result.loans` に union している。これは borrowed field record に必要な動作そのもの。
- `TypedExprKind::Field` は place として扱われ、record field の loan を `read_places` 経由で伝播できる。
- block を抜けるとき `eval_composed` は result.loans が block 内 local を指すと `E1013`。borrowed record を返すローカル所有者検査に効く。
- function return は `check_body` が external loan 以外を拒否する。

追加で必要な確認:

- `Type::contains_reference` が Record 再帰になれば `Signature::validate_borrows` が borrowed record result を検出する。
- `State.locals` に保存する `Value.loans` が record move/copy で消えない。`read_places` の `carries_loans` 分岐をテストで固定する。
- partial move: borrowed field だけ取り出す場合、record の loan が外へ残るか検査する。

### フェーズ 2/3 のデータ構造案

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Region {
    Elided,
    Named(String),
    Infer(usize),
}

pub enum Type {
    // ...
    Reference(Box<Type>, bool, Region),
    Record(usize, Vec<Type>, Vec<Region>), // A01 後
}

pub struct Signature {
    pub region_parameters: Vec<String>,
    pub parameters: Vec<Type>,
    pub result: Type,
}

pub struct RecordDecl {
    pub name: Ident,
    pub region_parameters: Vec<Ident>,
    pub type_parameters: Vec<Ident>,
    pub fields: Vec<Parameter>,
}
```

フェーズ 2 では `Signature.region_parameters` だけ導入し、フェーズ 3 で `RecordDecl.region_parameters` を有効化する。

region 検査は型検査時に宣言済み region name をスコープ管理する。未宣言 region は `E1013`。未使用 region は warning ではなく `E1013` または将来 W1001 まで保留。既定は `E1013`。

### LLVM 表現

- 参照 field は `ptr`。record LLVM 型に `ptr` field が入るだけ。
- `llvm_type(Type::Reference)` は既に `ptr`。
- drop/clone は reference に対して no-op。record drop/clone は field の `needs_drop` だけを見るため、参照 field は no-op。
- export ABI は record なので引き続き不可。
- WASM imports なし。

### 変更ステージ

| ステージ | フェーズ 1 変更 |
|---|---|
| lexer | 変更なし |
| parser | 変更なし。既存 `&T` 型構文を record field で使う |
| syntax | 変更なし |
| computation::expand | 変更なし |
| check | record field の `ty.contains_reference()` 禁止を shared reference に限り解除。Record 再帰版 `contains_reference`/`contains_mutable_reference` |
| polymorph | `map_type`, `substitute`, `bounded_type`, `unify` は Reference を既に扱う。Record generic 後は A01 の args と合わせて確認 |
| control | 変更なし |
| closures | borrowed record capture を既存 loan 検査で確認。変更なしのはず |
| recursion | 変更なし |
| ownership | 基本変更なし。テストで `Record` carries loans 経路を固定し、必要なら error message/span を調整 |
| ownership_control | 変更なし。pattern/iteration binding から borrowed record が漏れないことをテスト |
| call_specialization | `Type::contains_reference` signature 変更に追随 |
| llvm | record field ptr の出力。`llvm_type` は変更なし。`validate_main` も record 不可のまま |
| llvm_control / llvm_frame | frame/drop/relocate が reference field を no-op として扱うことを確認 |
| runtime | 変更なし |
| driver/main | 変更なし |

フェーズ 2/3 では lexer 変更なし、parser の type reference grammar、syntax/check/polymorph/ownership に region を追加する。

### 前提とする他チケットのインターフェース

- A01 後の generic record では `Type::Record(usize, Vec<Type>)` に region args を追加する必要がある。フェーズ 1 は非 generic/既存 record でも実装可能。
- C03 の slice `&[T]` は A09 フェーズ 1 の borrowed field と相性がよい。`record SliceView { data: &[i64] }` を使える。

### 他チケットへの提供インターフェース

- C03/D02 は borrowed view record を標準 API の戻り値として使える。ただしフェーズ 1 では borrowed input は 1 個に限定。
- E05 は host ABI 拡張時に borrowed record を公開しない。公開 ABI は所有権/寿命が表せるまで不可。

## 実装手順

### フェーズ 1（完全指定）

1. **Type reference 検査の Record 再帰化**
   - `Type::contains_reference(&self)` を `contains_reference(&self, records: &[CheckedRecord])` に変更、または新 method を追加。
   - `Record(id)` で fields へ再帰。
   - `contains_mutable_reference` も records 付きへ変更。
   - 確認: 既存テストがコンパイルし、task result に borrowed record があれば `E1013`。
2. **record field shared borrow 解禁**
   - `check_modules` の field loop から `ty.contains_reference()` 一律拒否を削除。
   - 全 records 構築後に `&mut` field と mutable reference を含む field を `E1013` で拒否。
   - 確認: `record View { data: &[i64] }` は受理、`record Bad { data: &mut i64 }` は `E1013`。
3. **borrowed record result の省略規則**
   - `Signature::validate_borrows` が `View` result を reference-containing と認識する。
   - `def view :: &[i64] -> View` は受理。
   - `def bad :: &[i64] -> &[i64] -> PairView` は `E1013`。
4. **所有権回帰**
   - borrowed record を block 外へ返す、owner move 後に使う、borrow conflict を追加テスト。
   - 必要なら `ownership.rs::read_places` の `"nested borrowed values require explicit lifetime parameters"` の span/message を調整。
5. **LLVM/E2E**
   - `record View { data: &[i64] }` の field は `%tz.record.Main.View = type { ptr }` または `{ %tz.array? }` ではなく reference to array descriptor pointer になる。現在 `&[i64]` は `ptr`。
   - E2E で view 経由の read、heap `live == 0`。
6. **文書**
   - phase 1 の制約を明記。「複数 borrowed input を束ねる record は名前付き region まで不可」。

### フェーズ 2（設計）

1. `Region` と `Type::Reference(_, _, Region)` を追加。
2. `Parser::type_atom` の `&` 後に optional `{ident}` region を読む。
3. `Signature.region_parameters` を `def name {r s} :: ...` 形式で parse。
4. `Signature::validate_borrows` を「result の named regions が parameter の同名 region に存在する」検査へ置換。
5. elided `&T` は現在の省略規則を維持。

### フェーズ 3（設計）

1. `record View {r} { data: &{r} [i64] }` を parse。
2. `Type::Record(id, type_args, region_args)` を A01/A02 と統合。
3. record literal で field region が入力 loan と一致することを ownership state に記録。
4. 複数 borrowed input の record result を許可。

## テスト計画

### フェーズ 1 受理

```text
record View { data: &[i64] }
def view :: &[i64] -> View
fn view xs = View { data: xs }
def first :: View -> i64
fn first v = v.data[0]
def main :: i64
fn main = { let xs = [10, 20]; first (view (&xs)) }
```

```text
record TextView { text: &string }
def len :: TextView -> i64
fn len v = v.text.length
```

```text
record Nested { view: View }
def nested :: &[i64] -> Nested
fn nested xs = Nested { view: View { data: xs } }
```

### フェーズ 1 拒否

| プログラム | 期待 |
|---|---|
| `record Bad { value: &mut i64 }` | `E1013` |
| borrowed record を local owner から返す | `E1013` |
| borrowed record result で borrowed input 0 個 | `E1013` |
| borrowed record result で borrowed input 2 個 | `E1013` |
| borrowed record を task result にする | `E1013` |
| borrowed record field の owner を move 後に field 使用 | `E1014` または `E1012` |
| loop iteration binding 由来の borrow を record に入れて外へ出す | `E1013` |
| borrowed record を `&mut` 経由で保存 | `E1013` |

### フェーズ 2/3 受理予定

```text
record PairView {a b} { left: &{a} string, right: &{b} string }
def pair {a b} :: &{a} string -> &{b} string -> PairView {a b}
fn pair a b = PairView { left: a, right: b }
```

```text
def choose {r} :: bool -> &{r} string -> &{r} string -> &{r} string
fn choose flag a b = if flag { a } else { b }
```

### E2E

- fixture `borrowed_records`.
- `export def sum_view :: i64 -> i64` は内部で array を作り、`View` 経由で checksum。
- native/WASM × `-O0`/`-O3`。
- heap tracking `live == 0`。borrowed field はポインターだけなので追加 free なし。
- IR 決定性。record type に `ptr` field が入ることを確認。

## ドキュメント

- `docs/language.md`
  - Ownership/Borrowing に borrowed fields phase 1 の規則。
  - 未対応一覧を「名前付き lifetime と複数 borrowed input は未対応」に更新。
  - フェーズ 2/3 実装時は `&{r}` 構文を追加。
- `docs/architecture.md`
  - `Value.loans` が record fields を通じて伝播する不変条件。
  - `Type::contains_reference(records)` が Record 再帰すること。
- `README.md`
  - 所有権節の「借用を含むレコードは未対応」を phase 1 実装後に更新。

## 受け入れ条件

- [ ] `record View { data: &[i64] }` が受理される。
- [ ] `&mut` field は拒否される。
- [ ] borrowed record result は borrowed input 1 個だけで受理され、0/2 個は `E1013`。
- [ ] local owner への borrowed record を外へ返せない。
- [ ] borrowed record の field access/record nesting/partial move が既存所有権規則に従う。
- [ ] task/capture/send が borrowed record を安全に拒否する。
- [ ] LLVM は `ptr` field を持つ record を生成し、drop/free を誤らない。
- [ ] native/WASM × `-O0`/`-O3`、heap `live == 0`。

## 落とし穴

- `Type::contains_reference` を Record 再帰にしないと `Signature::validate_borrows` が borrowed record result を見逃す。
- `carries_loans` は既に Record 再帰するため、似た method との差異に注意。
- `&mut` field を phase 1 で許すと aliasing と置換時 drop の設計が不足する。
- borrowed record を `Copy` として扱っても loan は複製されるだけで owner 寿命は延びない。`Value.loans` を local に保存する必要がある。
- record field の LLVM `ptr` は所有ポインターではない。`drop_value` で free してはいけない。

## 対象外

- Rust 互換 lifetime 構文 `&'a T`。
- lifetime elision の高度化、一時値寿命延長。
- self-referential struct。
- mutable borrowed fields。
- host ABI で borrowed record を公開すること。

## 未決事項

- **既定案: 名前付き lifetime 構文は `&{r} T`。** Rust-style `&'r T` は既存 type variable token と衝突するため採用しない。
- **既定案: フェーズ 1 は shared borrowed fields のみ。** `&mut` field は名前付き region と aliasing 仕様を入れるまで拒否。
- **既定案: 未宣言/未使用 region は `E1013`。** 将来 warnings が整備されたら未使用 region は warning にしてもよい。
