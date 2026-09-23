# C05: レコードのコピーと更新 `{ p with x = … }`
| 項目 | 内容 |
|---|---|
| ID | C05 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | (A01) |
| 後続 | – |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs` `ExprKind`、`src/parser.rs` `primary` / `record` / `block`、`src/check.rs` `Checker::value_expression`、`src/ownership.rs`、`src/polymorph.rs`、`src/closures.rs`、`src/call_specialization.rs`、`src/llvm.rs` `expression_mode`、`docs/language.md`、`tests/records.rs` または `tests/frontend.rs` |

## 目的

既存レコード値から一部フィールドを置換した新しいレコードを作る構文を追加する。

```text
{ point with x = point.x + 1; y = 0 }
```

レコードは immutable value のままにし、フィールド代入は導入しない。base を消費して新しい値を作るため、非 Copy フィールドでも余分な deep copy を避けられる。D-15 のレコード更新構文に従う。

## 現状

- レコード宣言は `src/syntax.rs` `RecordDecl`、型は `Type::Record(usize)`。
- レコード literal は `ExprKind::Record { name, fields }`。parser は `Name { field: value }` だけを record literal として読む。
- フィールドアクセスは `TypedExprKind::Field(Box<TypedExpr>, usize)`。
- field assignment は `src/check.rs` `ExprKind::Assign` で `Local` / `Dereference` 以外を `E1012` にするため拒否される。
- `src/llvm.rs` `TypedExprKind::Record` は `insertvalue` chain で aggregate を作る。`TypedExprKind::Field` は field を extract し、残りの record を drop する。
- `src/ownership.rs` は record field の部分 move を `Place { root, fields }` で追跡できる。
- A01 後は `Type::Record(usize, Vec<Type>)` になる予定。C05 は非ジェネリック現状でも実装できるが、A01 後は type-changing update を禁止する必要がある。

## 仕様

### 前提とする他チケットのインターフェース

- A01 が完了していれば、record 型は `Type::Record(id, args)`。C05 は base と結果の record id/args が完全一致する更新だけを許す。
- A01 未完了なら現状 `Type::Record(id)` に対して実装してよい。

### 他チケットへの提供インターフェース

- C06 の Map/Set 内部ノード更新で immutable record の field 差し替えに使える。
- A01 の generic record 実装者は `RecordUpdate` が型引数を変えないことを前提にできる。

### 構文

```ebnf
record_update ::= "{" expression "with" record_update_fields "}"
record_update_fields ::= record_update_field (field_separator record_update_field)* field_separator?
record_update_field ::= ident ("=" | ":") expression
field_separator ::= ";" | ","
```

採用:

- `{ base with x = value; y = other }`
- `{ base with x: value, y: other }`
- `=` と `:` を両方受理する。理由: record literal が `:` なので、更新でも同じ見た目を許す。ただし docs の標準形は D-15 に合わせ `{ base with x = value }`。
- separator は `;` と `,` を受理する。record literal / pattern との親和性のため。docs の標準形は `;`。

不採用:

- `{ base with }` は `E0002`。
- field punning `{ p with x }` は対象外。
- type-changing update `{ pair with first = "x" }` は phase 1 で拒否。

### Parser disambiguation

`{ ... }` は既存 block と衝突するが、token scan で `with` を探してはいけない。`{ match x with | ... }` の `with` は同じ brace depth に見えるため、scan では record update と誤判定する。

採用する規則:

1. `primary` で `TokenKind::LeftBrace` を見たら、既存の `Parser::block` を `brace_expression` / `block_after_open` に分割する。
2. `{` を消費し、`stop_at_arm` / `stop_at_arrow` を既存 `block` と同じように一時的に false にする。
3. 最初の token が `let` / `return` / `do` / `}` なら、既存 block と同じ処理を続ける。
4. それ以外なら、まず最初の expression を通常どおり parse する。`parse_control.rs` `Parser::match_expression` は自分の `with` と arm を最後まで消費するため、`{ match x with | ... -> ... }` はここで 1 個の expression になる。
5. 最初の expression の直後に未消費の `TokenKind::With` があれば record update として parse する。この `with` は `match` 内のものではなく、base expression の後続 token である。
6. `With` がなければ、その最初の expression を block の先頭要素として扱い、既存 block parsing を継続する。再 parse はしない。

`with` は既に予約語である。`syntax.rs` `TokenKind::With` は `match ... with` 用に存在し、`lexer.rs` も既に `"with" => TokenKind::With` を返すため、字句解析の変更は不要。

この規則により、次を正しく区別する。

- `{ match x with | P { x = a } -> a | _ -> 0 }` は block。
- `{ (match x with | _ -> p) with field = value }` は record update。
- anonymous record pattern `{ x = a }` は `parse_control.rs` の pattern parser の内部だけで処理され、式 parser の `{ ... }` 分岐には影響しない。

### 型規則

- base は record 型でなければ `E1005`。
- field 名はその record に存在しなければ `E1007`。
- 同じ field を複数回指定したら `E1001`。
- 指定しない field は base から移すか copy する。
- 更新 field の expression 型は既存 field 型と完全一致。暗黙変換なし、A01 後も型引数変更なし。
- 結果型は base と同じ record 型。

### 評価順序

`{ base with f1 = e1; f2 = e2 }`:

1. `base` を評価する。
2. `e1` を評価する。
3. `e2` を評価する。
4. 新しい record を作る。
5. 置換された旧 field は drop する。
6. 指定されない field は base から移す。base は消費済み。

すべて左から右。`e1` 評価中は base の所有値はすでにこの式に保持されているため、同じ束縛への move/borrow conflict は既存所有権が検査する。`base` が Copy place で後続使用される場合は既存 `read_place` が clone する。

### 所有権

- record update は base を consume する。
- `let q = { p with x = 1 }; p.y` は、`p` が Copy でなければ `E1012`。
- `let q = { p with x = 1 };` で、置換されない non-Copy field は q へ move する。
- 置換される old field は drop される。新 field value は q が所有する。
- base が `&mut record` の deref `*r` なら、`{ *r with x = 1 }` は dereference value を consume しようとするため非 Copy field では既存規則により拒否される。全体置換は `*r = { *r with x = 1 }` ではなく、一度 owned local に move できる場合だけ可能。Phase 1 で read-modify-write sugar は入れない。

## 設計

### AST / Typed IR

`src/syntax.rs`:

```rust
pub enum ExprKind {
    // existing ...
    RecordUpdate {
        base: Box<Expr>,
        fields: Vec<(Ident, Expr)>,
    },
}
```

`src/check.rs`:

```rust
pub enum TypedExprKind {
    // existing ...
    RecordUpdate {
        base: Box<TypedExpr>,
        fields: Vec<(usize, TypedExpr)>,
    },
}
```

`fields` は record field index で昇順に並べ替えず、source order を保持する。評価順序のため。LLVM 生成時に `insertvalue` は source order で行う。

`TypedExpr::children` / `children_mut` は `base` の後に field values を source order で返す。

### Parser

`Parser::brace_expression` を追加し、`primary` の `LeftBrace` 分岐から呼ぶ。既存 `Parser::block` は「`{` を読む wrapper」と「すでに `{` を消費した後の body parser」に分ける。

擬似コード:

```rust
fn brace_expression(&mut self) -> Result<Expr, Diagnostic> {
    self.enter()?;
    let start = self.expect(&TokenKind::LeftBrace, "'{'")?.span;
    let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
    let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);

    let result = if self.at(&TokenKind::Let)
        || self.at(&TokenKind::Return)
        || self.at(&TokenKind::Do)
        || self.at(&TokenKind::RightBrace)
    {
        self.block_after_open(start, None)?
    } else {
        let first = self.expression(0, true)?;
        if self.eat(&TokenKind::With) {
            self.record_update_after_base(start, first)?
        } else {
            self.block_after_open(start, Some(first))?
        }
    };

    self.stop_at_arm = outer_arm;
    self.stop_at_arrow = outer_arrow;
    self.nesting -= 1;
    Ok(result)
}
```

`Parser::block` は互換 wrapper として残す。

```rust
fn block(&mut self) -> Result<Expr, Diagnostic> {
    self.enter()?;
    let start = self.expect(&TokenKind::LeftBrace, "'{'")?.span;
    let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
    let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
    let result = self.block_after_open(start, None);
    self.stop_at_arm = outer_arm;
    self.stop_at_arrow = outer_arrow;
    self.nesting -= 1;
    result
}
```

`block_after_open(start, first)` は現行 `Parser::block` の loop を再利用する。

- `first == None` なら現行どおり先頭から `let` / `return` / `do` / expression を読む。
- `first == Some(expr)` なら、まず `expr` を「直前に読んだ block 内 expression」として処理する。
  - 直後が `;`、または改行かつ `}` でない場合は、現行 block と同じく `_` への binding として `bindings` に追加し、loop を続ける。
  - それ以外なら `expr` が block の result。直後に `}` を要求する。

現行 `Parser::block` は expression を読んだ後、

```rust
if !self.eat(&TokenKind::Semicolon)
    && !(self.newline_before_current() && !self.at(&TokenKind::RightBrace))
{
    break value;
}
bindings.push(Binding { name: "_", value, ... });
```

という形で「先頭式を result または捨てる binding」に分類しているため、この分類を helper に切り出せば再 parse なしで実装できる。

`record_update_after_base`:

- `With` は caller が消費済み。
- field を 1 個以上読む。空なら `E0002`。
- field value は `expression(0,true)`。
- separator `;` or `,`。
- `RightBrace` を要求する。
- `span` は `{` から `}`。

`record_update` のために `stop_at_with` flag は追加しない。base expression は通常 parser に任せ、未消費の `With` だけを record update delimiter とする。

### Type checking

`Checker::value_expression`:

1. `base = self.expression(base, None)`。
2. `base = Self::autoderef(base)`。
3. record id を取り出す。なければ `E1005` `"record update requires a record value"`。
4. `seen: BTreeSet<String>` で duplicate 検査。`E1001`。
5. field name -> index。なければ `E1007`。
6. value を field type expected で typecheck。
7. `TypedExprKind::RecordUpdate` を返す。

A01 後:

- `Type::Record(id,args)` の `args` を保持。
- field type は `record_field_types(id,args)` helper で置換済み型を得る。

### Ownership

`src/ownership.rs` `eval_value`:

- base を `Use::Consume` で評価。
- field values を source order で `Use::Consume` 評価し、`held` に積む。
- result loans は field values と base のうち結果型が loans を運ぶ場合に統合する。
- base は消費されるため後続使用は `E1012`。

既存 partial move と同じにするため、より単純な実装として `RecordUpdate` を型検査後に `Block` + `Field` + `Record` へ desugar してもよいが、評価順序と old field drop が複雑になる。専用 IR を推奨。

### LLVM

`FunctionEmitter::expression_mode`:

```rust
TypedExprKind::RecordUpdate { base, fields } => {
    let base_value = self.expression(base);
    let mut result = base_value.clone();
    for (index, value_expr) in fields {
        if old_field.needs_drop { extract old and drop_value }
        let new_value = self.expression(value_expr);
        result = insertvalue record_ty result field_ty new_value, index
    }
    result
}
```

注意:

- `base` が place の場合、`self.expression(base)` は consume する。Copy かつ非最後使用なら clone される。
- old field drop は new field 評価後ではなく、仕様の「field expressions 評価後に旧 field drop」でもよい。だが old field と new value が同じ所有値を参照しうる場合を避けるには、new value 評価を先にすべて行い一時に保持し、その後 old drop/store が安全。
- 実装順:
  1. base を評価。
  2. field values を source order で評価して Vec に保持。
  3. 置換 field の old value を extract/drop。
  4. insertvalue chain。

この順序は仕様通り。

### Walkers

以下に追加:

- `computation.rs` `expand`
- `polymorph.rs` `expression_types` は children で足りるが local type なし。
- `closures.rs` `free_locals` children で足りる。
- `recursion.rs` references children で足りる。
- `call_specialization.rs` `read_only`: base consume、field values consume。`all_children` だけでは access 種別が不足するため必要なら special-case。
- `llvm_frame.rs` `frame_inner`: record update result が literal tree になることは少ないため heap path でよい。Phase 1 変更なしでも可。

## 実装手順

1. **構文追加**
   - `ExprKind::RecordUpdate`、`brace_expression`、`block_after_open(start, first)`、`record_update_after_base` を追加。
   - `with` token scan と `stop_at_with` は追加しない。
   - 確認: parser tests で block / match with / record update の disambiguation。

2. **型検査**
   - record id 解決、field 重複/未知、field 型一致。
   - 確認: accepted/rejected unit tests。

3. **IR walkers / ownership**
   - children、computation expand、ownership eval。
   - 確認: move 後使用と borrow conflict tests。

4. **LLVM lowering**
   - eval all new values first、drop old replaced fields、insertvalue chain。
   - 確認: IR に `extractvalue` old field → drop → `insertvalue`。

5. **Docs**
   - `docs/language.md` record section。
   - 確認: examples compile。

## テスト計画

### Rust tests

受理:

- `record P { x: i64, y: i64 }\nlet p = P { x: 1, y: 2 }\n{ p with x = 3 }.x`
- `field: value` と comma separator。
- `{ match x with | P { x = a } -> a | _ -> 0 }` が block として parse される。
- anonymous record pattern を含む match: `match p with | { x = a } -> a` が record update と誤認されない。
- `{ (match flag with | true -> p | false -> p) with x = 10 }` が record update として parse される。
- field order source evaluation:
  ```text
  let mut n = 0
  let q = { p with x = { n = n + 1; n }; y = { n = n + 10; n } }
  ```
  結果 x=1,y=11。
- non-Copy old field drop:
  `record R { s: string, n: i64 }` で `s` 置換。
- unspecified non-Copy field move:
  `{ r with n = 2 }` 後に old `r.s` は q が所有。
- generic record A01 後: `Pair i64 string` の update。

拒否:

- `{ 1 with x = 2 }` → `E1005`
- `{ p with z = 1 }` → `E1007`
- `{ p with x = 1; x = 2 }` → `E1001`
- `{ p with x = true }` → `E1003`
- A01 後 `{ pair with first = "x" }` where `pair: Pair i64 i64` → `E1003`
- `let q = { p with x = 1 }; p.y` for non-Copy p → `E1012`
- `let r = &p.y; let q = { p with x = 1 }; r` → `E1014`

IR:

- 更新は record aggregate の `insertvalue` chain。
- 新 field value の call が old field drop より前。
- 置換されない field は clone されない（base move の aggregate を利用）。

### E2E

小規模なので Rust IR tests で十分。所有 string の解放を確認するため、既存 `tests/primitives.mjs` の storage fixture に record update case を追加する。native/WASM `-O0/-O3`、heap tracking `live == 0`。

検証コマンドは GUIDE §3 に従い、Rust は `cargo test --locked --test records` または実際に追加した test file 名で `--test <file>` を使い、`running N tests` の `N > 0` を確認する。Node E2E を走らせる直前には必ず `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

## ドキュメント

- `docs/language.md`
  - レコード節に `{ base with field = value; ... }`。
  - 評価順序、base consume、field duplicate。
- `docs/architecture.md`
  - immutable record update は `insertvalue` chain、in-place field mutation ではない。
- `README.md`
  - 必要なら短い例。

## 受け入れ条件

- [ ] `{ base with field = value }` が parse/typecheck/lower される。
- [ ] block / match の `with` と誤認しない。
- [ ] base first、field values source order。
- [ ] unknown field `E1007`、duplicate `E1001`、type mismatch `E1003`。
- [ ] base consume と borrow conflict が既存診断で出る。
- [ ] old replaced field が drop され、未置換 field は結果へ move/copy。
- [ ] A01 後も type-changing update を拒否。
- [ ] docs 更新。

## 落とし穴

- `with` は既存 token。新規 keyword 追加ではないが、brace 内 lookahead が match arm の `with` を拾うと壊れる。
- `{ ... }` の種別判定で token scan を使わない。必ず最初の expression を通常 parse し、未消費 `With` の有無で record update か block continuation かを決める。
- new field を store してから old field drop すると、同じ所有値を再利用するケースで壊れる。新値評価 → old drop → insert。
- `RecordUpdate` の children 漏れは closure capture / polymorph / ownership のバグになる。
- `{ p with x: y }` を record literal `Name { ... }` と混同しない。先頭が `{` なら update/block だけ。

## 対象外

- Field punning。
- Nested update sugar `{ p with address.city = "x" }`。
- レコード field assignment。
- Type-changing update。
- with での field 削除/追加。

## 未決事項

- Separator は `;` と `,` 両方を受理する既定案。formatter G05 では `;` に正規化する。
- `:` も受理する既定案。ただし docs は `=` を標準形にする。
- 台帳の見直し提案: なし。D-15 と整合する。
