# A05: 型別名

| 項目 | 内容 |
|---|---|
| ID | A05 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | (A01) |
| 後続 | A06, A07, E01, 標準ライブラリ各チケット |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/computation.rs`, `src/driver.rs`, `src/main.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/types_ownership.rs`, `tests/polymorphism.rs`, `tests/modules.rs` |

## 目的

`type Meters = f64`、`type Pair2 'a = Pair 'a 'a` のような透過的な型別名を導入し、標準ライブラリと利用者コードで長い型、部分適用した型、将来の `Option`/`Result`/`Vec`/`Map` 型を読みやすく表せるようにする。別名は **newtype ではなく完全に透過的**であり、型検査・型クラス・所有権・LLVM IR では展開後の型だけが意味を持つ。

このチケットは GUIDE §9 D-06 に従う。区別される型が必要な場合は A02 の単一ケース `union UserId = UserId of i64` を使い、型別名で実行時表現や型クラス解決を分岐させない。

## 現状

- `src/syntax.rs` の `TokenKind` には `Type` がなく、`Program` は `records`, `functions`, `classes`, `instances`, `active_patterns`, `entry` だけを持つ。
- `src/parser.rs` の `Parser::program` は先頭キーワードごとに `record`、`class`、`instance`、`let` 実装、`def`/`fn`、最後にエントリー式を読む。新しいトップレベル宣言はここへ分岐を足す。
- 型式は `src/syntax.rs` の `TypeExprKind::{Named, Variable, Constrained, Array, List, Tuple, Task, Function, Reference}`。A01 完了前は `TypeExprKind::Apply` が存在しない。
- `src/parser.rs` の `Parser::type_atom` は `&`、型変数、括弧、`fn(...) -> ...`、配列/リスト、`Task T`、修飾名、`Class 'a` 制約を処理する。型名に続く一般の型引数は読まない。
- `src/check.rs` の `resolve_type` は `TypeExprKind::Named` を `numeric::primitive` または `Names::record` へ解決する。ここが注釈、レコードフィールド、関数シグネチャ、クラス/インスタンス型、ローカル注釈の共通入口になっている。
- `src/polymorph.rs` の `Classes::inline_constraints` は `TypeExprKind::Constrained` を制約として集め、`TypeExpr` の内側を再帰する。別名が制約付き型式を含む場合もここで展開が必要。
- `src/polymorph.rs` の `Classes::instances` は `instance.class` と `instance.ty` を `resolve_type` し、`require_concrete` 後に `implementations: BTreeMap<(usize, Type), Vec<usize>>` へ登録する。別名を展開しないと `instance Add Meters` と組み込み `Add f64` の重複を見逃す。
- `src/computation.rs` の `collect` は `.tt` には class 以外を禁止し、`.tc` はビルダー操作を要求する。新宣言のソース種別違反はここで `E1018` にする。
- `src/numeric.rs` の `primitive` は `bool`, `unit`, `string`, `byte`, `ubyte`, 数値名をプリミティブへ写す。これらの名前で型別名を宣言してはいけない。

## 仕様

### 構文

```text
type-declaration ::= visibility? "type" type-name type-parameter* "=" type
visibility       ::= "private"                  // E01 完了後だけ有効
type-name        ::= PascalCase identifier
type-parameter   ::= type-variable              // 'a, 'value
type             ::= 既存の TypeExpr。A01 後は型適用も含む
```

例:

```text
type Meters = f64
type UserNames = [string]
type Pair2 'a = Pair 'a 'a
type BorrowedText = &string
```

許可するソース種別:

| 種別 | 方針 |
|---|---|
| `.tz` | 許可 |
| `.tc` | 許可。D-08 と同様にビルダー補助型として使える |
| `.tt` | 禁止。`.tt` は型クラス宣言と A06 の default method body だけを含む |
| 拡張子なし `analyze`/`analyze_modules` 互換経路 | 許可。既存の単一ファイルテスト互換のため |

`private type` は E01 の可視性制御に従う。E01 未実装時にこのチケットだけを実装する場合、`private` はまだ予約語でないため受理しない。E01 後は `TypeAliasDecl.visibility` を持たせ、同一モジュール外からの使用を `E1022` にする。

### 型規則

- 型別名は透過的で、`Alias` 型や LLVM 型を新設しない。
- `type Meters = f64` の後、`Meters` はすべて `f64` と同じ型として扱う。診断表示も原則として展開後の `f64` を表示する。
- 型別名は別名を参照できる。`type Distance = Meters` は `f64` へ展開する。
- 循環する別名は `E1024`:
  ```text
  type A = B
  type B = A
  ```
- 宣言した型パラメーターは右辺で少なくとも一回使う。未使用は `E1024`:
  ```text
  type Boxed 'a = i64  // E1024
  ```
- 未宣言の型変数を右辺で使うと `E1024`:
  ```text
  type Bad = 'a        // E1024
  ```
- 同じ型パラメーター名の重複も `E1024`。
- 非ジェネリック別名は A01 前でも実装する。ジェネリック別名は A01 の `TypeExprKind::Apply` と `Type::Record(usize, Vec<Type>)`/将来の `Type::Union` が入ってから有効化する。
- ジェネリック別名の arity 不一致は `E1024`。例: `type Pair2 'a = Pair 'a 'a` に対して `Pair2` や `Pair2 i64 string` は拒否。
- `type byte = i8` のようにプリミティブ名、`Task`、`_`、既存レコード/union/class と衝突する名前は `E1001`。

### 型クラス・所有権・評価順序

- 別名展開は型検査前の名前解決で完了するため、評価順序、短絡、所有権、借用、move/drop は展開後の型と完全に同じ。
- `instance Add Meters { ... }` は `instance Add f64` と同じインスタンスヘッドとして扱う。`Add f64` は組み込みなので `E1016`。
- 制約に含まれる型も展開する。`def f :: Eq Meters => Meters -> bool` は `Eq f64 => f64 -> bool` と同じ。
- 別名が参照型を含む場合、`type TextRef = &string` は既存の `Signature::validate_borrows` と `ownership.rs` の loan 追跡を通る。

### LLVM IR

- 型別名専用の IR は生成しない。
- `Type::display`、`llvm_type`、`abi_type`、`c_type` は展開後の `Type` だけを見る。
- 生成 IR の型名、関数名、特殊化キーは別名名に依存しない。`Meters` と `f64` を混ぜても決定的に同じ IR になる。

### 診断

| コード | 条件 |
|---|---|
| `E1001` | 別名名が予約名・プリミティブ・同名の型/クラス等と衝突 |
| `E1004` | 展開後の型名が未知または既存の曖昧な型名 |
| `E1016` | 別名展開後にインスタンスが重複または組み込みを上書き |
| `E1018` | `.tt` に型別名を書くなどソース種別違反 |
| `E1022` | E01 後、`private type` の外部参照 |
| `E1017` | 別名展開の深さ・構成要素数が資源上限を超過 |
| `E1024` | 循環、型パラメーター未使用/未宣言/重複、arity 不一致 |

## 設計

### データ構造

`src/syntax.rs`:

```rust
pub enum TokenKind {
    // ...
    Type,
}

pub struct Program {
    pub source_kind: Option<SourceKind>,
    pub type_aliases: Vec<TypeAliasDecl>,
    pub records: Vec<RecordDecl>,
    pub functions: Vec<FunctionDecl>,
    pub classes: Vec<ClassDecl>,
    pub instances: Vec<InstanceDecl>,
    pub active_patterns: Vec<ActivePattern>,
    pub entry: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct TypeAliasDecl {
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub target: TypeExpr,
    // E01 後:
    // pub visibility: Visibility,
}
```

A01 後は `TypeExprKind::Apply(Ident, Vec<TypeExpr>)` をそのまま使う。A01 前の最小実装では `parameters.is_empty()` の alias だけを受理し、パラメーター付き宣言を `E1024` で拒否するか、A01 と同じ PR に含める。

`src/check.rs` の `Names` に別名表を追加する:

```rust
struct TypeAlias {
    module: String,
    name: String,
    parameters: Vec<String>,
    target: TypeExpr,
    span: Span,
    // visibility: Visibility,
}

struct Names {
    modules: BTreeSet<String>,
    builders: BTreeMap<String, BTreeSet<String>>,
    records: BTreeMap<String, usize>,
    record_aliases: BTreeMap<String, Vec<String>>,
    type_aliases: BTreeMap<String, TypeAlias>,
    type_alias_names: BTreeMap<String, Vec<String>>,
    functions: BTreeMap<String, usize>,
    active_patterns: BTreeMap<String, (usize, bool)>,
}
```

`type_aliases` のキーは `Module.Name`。`type_alias_names` は無修飾名解決用で、`Names::record` と同じ優先順位を使う: 自モジュール → 修飾名 → プロジェクト内で一意。E02 後は標準ライブラリ優先順位も D-07 に合わせる。

### 解決アルゴリズム

`resolve_type` をラッパーにし、実体は訪問中 alias を持つ `resolve_type_inner` にする。

```rust
fn resolve_type(expression: &TypeExpr, module: &str, names: &Names) -> Result<Type, Diagnostic> {
    resolve_type_inner(expression, module, names, &BTreeMap::new(), &mut Vec::new())
}
```

処理順:

1. `TypeExprKind::Named(name)` がプリミティブなら現在通り即返す。
2. `name` が型別名なら、arity 0 として `expand_alias` へ進む。
3. そうでなければ `Names::record`/A02 の union 解決。
4. `TypeExprKind::Apply(head, args)` は、head が alias なら arity を検査して `parameters -> resolved args` の置換で target を展開する。head が A01/A02 の型ならそちらへ委譲する。
5. 展開中スタックに同じ `Module.Name` があれば `E1024`。
6. 展開後の `Type` に `polymorph::bounded_type` を必ず適用し、深さ/構成要素の上限を超えたら GUIDE D-19 に従って `E1017` を使う。循環や宣言形状の誤りだけを `E1024` にする。

置換は `TypeExpr` レベルで行う。`Type` へ解決してから `Type::Variable` を置換すると、A01 の型適用/クラス制約の分類を失うため避ける。

### 変更ステージ

| ステージ | 変更 |
|---|---|
| lexer | `Lexer::identifier` に `"type" => TokenKind::Type` を追加。D-15 に従い予約語化テストを追加 |
| parser | `Parser::program` の先頭分岐に `type` を追加。`type_alias` helper を作り、`=` の右側は `type_expr` で読む |
| syntax | `Program.type_aliases`, `TypeAliasDecl`, `TokenKind::Type` を追加 |
| computation::collect | `.tt` では `type_aliases.first()` を `E1018`。`.tc` では許可し、ビルダー操作存在チェックは従来通り |
| check::check_modules | 全モジュールの alias 名をレコード/クラス名と同時に収集。重複と予約名を `duplicate()`/`E1001`。alias target 検証をレコード構築前に行うが、実際の展開は `resolve_type` で遅延して相互参照を許す |
| check::resolve_type | alias 展開、cycle/arity/param 検査、A01/A02 型適用との連携を追加 |
| polymorph::Classes::inline_constraints | alias を含む `TypeExpr` を展開してから制約収集する。別名 target 内の `Constrained` も拾う |
| polymorph::type_expression | 変更なし。単相化で alias 名は復元しない |
| polymorph::Classes::instances | 変更なしに見えるが `resolve_type` が正規化するため、overlap 判定は展開後 Type で行う |
| control / closures / recursion / ownership | `Type` に alias variant を作らないので変更なし |
| call_specialization | 変更なし |
| llvm / llvm_control / llvm_frame | 変更なし。展開後 Type だけを受ける |
| runtime | 変更なし |
| driver / main | `.tz/.tt/.tc` の説明に `type` を反映。読み込み処理は変更なし |

### 前提とする他チケットのインターフェース

- A01 完了後は `TypeExprKind::Apply(Ident, Vec<TypeExpr>)` と `Type::Record(usize, Vec<Type>)` を使う。A01 未完了でこのチケットだけ実装する場合、非ジェネリック alias だけを有効にする。
- A02 完了後は `Type::Union(usize, Vec<Type>)` も alias 展開対象にする。
- E01 完了後は `private type` の可視性情報を `TypeAliasDecl`/`Names` に追加し、D-09 の規則で外部参照を `E1022` にする。

### 他チケットへの提供インターフェース

- すべての型解決入口で alias が展開済みの `Type` を返す。
- `Names` には alias の存在確認 API を追加してもよいが、A06/A07 は必ず `resolve_type` 経由で展開後 Type を得る。
- 診断表示は展開後 Type を使うため、A06 の instance overlap と A07 の deriving 制約判定は alias 名を特別扱いしない。

## 実装手順

1. **構文だけ追加**
   - `TokenKind::Type`、`Lexer::identifier`、`TypeAliasDecl`、`Program.type_aliases`、`Parser::program` の `type` 分岐を追加。
   - `parse` テストで `type Meters = f64`、`type Pair2 'a = Pair 'a 'a`、`type` が識別子として使えなくなることを確認。
   - 確認: `cargo test --locked lexer::tests::` と `cargo test --locked parser::tests::` を実行し、GUIDE §3 の通り `running N tests` が 0 でないことを確認する。統合テストへ移した場合は `cargo test --locked --test frontend <テスト名>` を使う。
2. **名前収集とソース種別**
   - `check_modules` で alias 名を収集し、プリミティブ/`Task`/`_`/同一 alias/record/class との衝突を `E1001`。
   - `computation::collect` で `.tt` の alias を `E1018`。
   - 確認: `analyze_modules(&[("Types.tt", "type Meters = f64")])` が `E1018`。
3. **非ジェネリック alias 展開**
   - `resolve_type_inner` と cycle stack を実装。
   - `type Meters = f64; def f :: Meters -> Meters` が通る。
   - `type A = B; type B = A` が `E1024`。
   - 確認: `llvm::emit` が `double` を使い、`Meters` という IR 型名を含まない。
4. **ジェネリック alias 展開**
   - A01 の `TypeExprKind::Apply` を使い、パラメーター置換と arity 検査を実装。
   - 未使用/未宣言/重複パラメーターを宣言収集時に `E1024`。
   - 確認: `type Pair2 'a = Pair 'a 'a` が `Pair i64 i64` と同じ型になる。
5. **型クラス連携**
   - `Classes::inline_constraints` と `Classes::instances` の既存呼び出しが alias 展開後 Type を使うことをテストで固定。
   - 確認: `type Meters = f64; instance Add Meters { fn add x y = x }` が `E1016`。
6. **ドキュメントと回帰**
   - README/docs の予約語、型節、診断表を更新。
   - 確認: `cargo fmt --all -- --check`, `cargo test --locked`, 関連 Node E2E。

## テスト計画

### Rust 受理テスト

```text
type Meters = f64
def add_distance :: Meters -> Meters -> Meters
fn add_distance x y = x + y
def main :: Meters
fn main = add_distance 20.5 21.5
```

```text
record Pair { left: i64, right: i64 }
type PairAlias = Pair
def sum :: PairAlias -> i64
fn sum p = p.left + p.right
```

A01 後:

```text
record Pair 'a 'b { left: 'a, right: 'b }
type Pair2 'a = Pair 'a 'a
def first :: Pair2 i32 -> i32
fn first p = p.left
```

`.tc`:

```text
// Id.tc
type Boxed = i64
def Return :: Boxed -> Boxed
fn Return x = x
```

### Rust 拒否テスト

| プログラム | 期待 |
|---|---|
| `type A = B; type B = A` | `E1024` |
| `type Box 'a = i64` | `E1024` |
| `type Bad = 'a` | `E1024` |
| `type Pair2 'a 'a = 'a` | `E1024` |
| `type i64 = i32` | `E1001` |
| `type Task = i64` | `E1001` |
| `type R = Missing` | `E1004` |
| `Types.tt` に `type Meters = f64` | `E1018` |
| `type Meters = f64; instance Add Meters { fn add x y = x }` | `E1016` |
| A01 後 `type Pair2 'a = Pair 'a 'a; def f :: Pair2 -> i64` | `E1024` |

### E2E

GUIDE §3 に従い、Node E2E の直前に必ず `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

- fixture `tests/fixtures/type_aliases` を作る。
- `export def add_meters :: f64 -> f64 -> f64` は内部で `Meters` を使い、期待値 `x + y` を JS の `Number` で計算。
- `export def alias_record :: i64 -> i64` は alias したレコードを作り、フィールド和を返す。
- native/WASM × `-O0`/`-O3` で実行し、同じ入力で `--emit llvm` を 2 回出して完全一致を確認。
- alias は IR 表現を持たないため、IR 中に `%tz.record.Main.Meters` や `Meters` 名が不要に出ないことを確認。ただし関数名やコメントに出る場合は避ける。
- ヒープ追跡対象は alias 自体で増えない。string/array/list alias を使う fixture で各呼び出し後 `live == 0`。

## ドキュメント

- `docs/language.md`
  - 「ソースと宣言」に `type` と `.tz/.tc` での許可、`.tt` 禁止を追加。
  - 「型とメモリ」に透過的 alias と newtype ではないことを追加。
  - 診断表に `E1024` の具体例を追加。
- `docs/architecture.md`
  - `Names` と `resolve_type` の alias 展開不変条件を追加。
  - 「多相性」に alias 展開後に制約/特殊化することを追加。
- `README.md`
  - 実装済み範囲に型別名を追加し、未実装一覧から該当記述を削除。

## 受け入れ条件

- [ ] `type Name = T` がすべての型注釈位置で使える。
- [ ] A01 後、`type Name 'a... = T` の置換と arity 検査が動作する。
- [ ] alias cycle、未使用/未宣言/重複パラメーターが `E1024`。
- [ ] `.tt` の alias が `E1018`。
- [ ] alias 展開後の型で instance overlap と組み込み上書きが検査される。
- [ ] 生成 LLVM IR に alias 専用表現がなく、native/WASM × `-O0`/`-O3` の E2E が通る。
- [ ] README/docs の予約語・診断・型説明が更新されている。

## 落とし穴

- `Type::Alias` を追加しない。追加すると `Type::is_copy`, `needs_drop`, `llvm_type`, `unify`, `Classes::intrinsic` など全経路で展開漏れが起きる。
- `resolve_type` だけでなく `Classes::inline_constraints` も alias target の `Constrained` を見る必要がある。
- alias の右辺を宣言時に `Type` へ固定しすぎると、A01 の型適用や A02 の union 追加と衝突する。`TypeExpr` を保持して展開時に置換する。
- `type Pair2 'a = Pair 'a 'a` の `Pair` が型かクラスかは A01 の D-02 分類に従う。曖昧なら `E1004`。
- 無修飾 alias 名の解決順をレコードとずらすと、モジュール間の診断が不安定になる。

## 対象外

- newtype、表現の異なる型、`opaque type`。
- 型別名の再エクスポート/import。
- 型レベル計算、条件付き alias、関連型。
- alias 名を診断に保持してプリティ表示する機能。初版は展開後型を表示する。

## 未決事項

- **既定案: alias の診断表示は展開後型だけ。** 利用者に書いた alias 名を残したい要望が出たら、別チケットで `TypeOrigin` を設計する。
- **既定案: `.tt` では alias 禁止。** A06 の default method body を `.tt` に許しても、型宣言まで許すと typeclass-only ファイルの境界が曖昧になるため。
- **A01 未完了時の扱い:** 非ジェネリック alias だけを先に入れる。ジェネリック alias は A01 と同時または後続 PR で有効化する。
