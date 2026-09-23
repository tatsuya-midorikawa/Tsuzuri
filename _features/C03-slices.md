# C03: スライス（`&xs[a..b]`）
| 項目 | 内容 |
|---|---|
| ID | C03 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | – |
| 後続 | C04, D02, E05, F02 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs` `ExprKind`、`src/parser.rs` `postfix` / `expression_inner`、`src/check.rs` `Type::Reference` handling / `TypedExprKind`、`src/ownership.rs`、`src/ownership_control.rs`、`src/polymorph.rs`、`src/closures.rs`、`src/call_specialization.rs`、`src/llvm.rs`、`src/llvm_control.rs`、`src/llvm_frame.rs`、`docs/language.md`、`docs/architecture.md`、`tests/slices.rs`、`tests/fixtures/slices/Main.tz` |

## 目的

配列の部分範囲をコピーせずに読み取る borrowed view を提供する。D-13 に従い、利用者に見える型は **`&[T]` そのもの** とし、`&xs[a..b]`、`&xs[a..]`、`&xs[..b]` で作る。C04 の読み取り API、E05 の host buffer ABI、F02 の並列 API の共通入力にする。

スライスは所有しない参照であり、元配列の loan に寿命が結び付く。コレクションの不変性は維持し、`&mut [T]` は従来通り「配列全体の置換用 pointer-to-descriptor」のままにする。

## 現状

- `src/check.rs` `Type::Reference(Box<Type>, bool)` は共有・排他参照を表す。`src/llvm.rs` `llvm_type` は全 reference を `ptr` に下げる。
- `&xs` は `TypedExprKind::Borrow(value, false)` で、LLVM では `FunctionEmitter::expression_mode` が `self.place(value)` を返す。つまり現在の `&[T]` は配列 descriptor への `ptr`。
- `src/check.rs` `Checker::autoderef` は field / index などで `Type::Reference` を自動的に `TypedExprKind::Dereference` に変える。
- `src/llvm.rs` `FunctionEmitter::place` は `Dereference(value)` を `expression_mode(value, false)` の pointer とみなす。
- `src/llvm.rs` `TypedExprKind::Index` は `checked_element_pointer` で bounds check 後に GEP / load し、要素を `clone_value` して返す。
- `src/llvm_control.rs` `for_each` は source を `read_operand` し、descriptor の data/length から連続走査する。
- レコード field は `src/check.rs` の record collection で `ty.contains_reference()` なら `E1013`。スライスもこの規則に乗る。
- `TokenKind::DotDot` は既にあり、`parser.rs` `expression_inner` で range expression `start .. finish` に使われている。postfix `xs[index]` は単一式だけを読む。

## 仕様

### 前提とする他チケットのインターフェース

- なし。C03 は単独で導入する。
- A09 が後で名前付き lifetime / borrowed fields を導入するまでは、スライスを record field / task result / task capture に格納することは既存通り拒否する。

### 他チケットへの提供インターフェース

- C04 は読み取り専用 API の引数に `&['a]` を使う。
- D02 は `&string` 用の文字列 slice を別途設計する。C03 では `&string[a..b]` は導入しない。
- E05 は host ABI buffer を `&[T]` の fat view と対応させる。
- F02 は `&[T]` を並列処理の入力 view として使う。

### 表面構文

```ebnf
slice_expr  ::= "&" slice_target
slice_target ::= postfix_expr "[" slice_range "]"
slice_range ::= expression ".." expression
              | expression ".."
              | ".." expression
```

例:

```text
let xs = [10, 20, 30, 40]
let middle = &xs[1..3]  // &[i64], length 2
let tail = &xs[2..]     // &[i64], length xs.length - 2
let head = &xs[..2]     // &[i64], length 2
```

slice range は `&` prefix parser の内部でだけ有効にする。裸の `xs[a..b]`、`xs[a..]`、`xs[..b]` は **必ず構文エラー `E0002`** とし、診断メッセージは `"array slices must be borrowed; write '&xs[a..b]'"` のように修正方法を含める。これにより `Parser::prefix` が postfix operand 全体を読んでから `ExprKind::Borrow` で包む現状の形で `Borrow(Slice)`（型としては `&&[T]`）を作る事故を防ぐ。

`&xs[..]` は phase 1 では不許可。全体参照は既存の `&xs` を使う。

### 型規則

- `&xs[a..b]` の `xs` は所有配列 `[T]` または共有配列参照 `&[T]`。
- 戻り型は `&[T]`。内部表現は fat borrowed view `{ ptr, i64 }`。
- `&mut xs[a..b]` は導入しない。`&mut [T]` は従来通り配列全体置換用。
- list slice は対象外。`&list[a..b]` は `E1005`。
- string slice は対象外。`&text[a..b]` は `E1005`。D02 で UTF-8 境界規則とともに扱う。
- slice は所有値ではないので `Vec.to_array` のような transfer はない。
- `*slice` の dereference は owned array `[T]` を新しく作る。要素は `clone_value` されるため、非 Copy 要素では既存所有権規則に従い `E1012` または Copy 制約が必要になる。Phase 1 では `*slice` を値として使う場合 `Copy T` を要求する。

### 評価順序と trap

`&xs[a..b]`:

1. `xs` を評価し、共有 loan を作る。
2. `a` を評価。
3. `b` を評価。
4. `0 <= a <= b <= len(xs)` を検査。
5. 成功後、`data + a` と `b - a` の view を返す。

`&xs[a..]`:

1. `xs`
2. `a`
3. `0 <= a <= len`
4. view length `len - a`

`&xs[..b]`:

1. `xs`
2. `b`
3. `0 <= b <= len`
4. view start `data`

負数は `i64` の unsigned compare で reject してよいが、`a <= b` と `b <= len` を明示的に検査する。検査失敗は `llvm.trap`。GEP はすべての検査後にだけ生成する。

### 所有権・寿命

- `&xs[a..b]` は `&xs` と同じく `xs` に共有 loan を張る。
- slice が生存している間、`xs` の move、`xs` への代入、`&mut xs` は `E1014`。
- slice を返す関数は借用入力をちょうど一つ持つ必要がある。既存 `Signature::validate_borrows` を維持する。
- slice を local block から外へ出すと、元所有者が block 内なら `E1013`。
- slice は `Copy`。ただし copied slice も同じ loan を運ぶ。
- `Task` capture / result は `Type::can_send` / `contains_reference` により `E1013`。

### `&[T]` の LLVM 表現

共有配列参照 `Type::Reference(Box::new(Type::Array(T)), false)` だけを特別扱いし、LLVM 型を `%tz.array` にする。これは owned array descriptor と同じ物理 layout だが、drop しない borrowed view として扱う。

```rust
fn llvm_type(ty: &Type, module: &CheckedModule) -> String {
    match ty {
        Type::Reference(inner, false) if matches!(**inner, Type::Array(_)) => "%tz.array".into(),
        Type::Reference(..) => "ptr".into(),
        // existing...
    }
}
```

`&mut [T]` は `ptr` のまま。`&string` も C03 では `ptr` のまま。

## 設計

### AST / Typed IR

`src/syntax.rs`:

```rust
pub enum ExprKind {
    // existing ...
    Slice {
        value: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
}
```

`start` / `end` は EBNF の省略に対応する。`None,None` は parser で拒否する。

`src/check.rs`:

```rust
pub enum TypedExprKind {
    // existing ...
    Slice {
        value: Box<TypedExpr>,
        start: Option<Box<TypedExpr>>,
        end: Option<Box<TypedExpr>>,
    },
}
```

typed `Slice` の型は `Type::Reference(Box::new(Type::Array(element)), false)`。

`TypedExpr::children` / `children_mut` に `value`、`start`、`end` を必ず追加する。

### Parser

`src/parser.rs` の `Parser::prefix` を変更し、shared borrow `&` の直後で slice 構文を認識した場合だけ `ExprKind::Slice` を直接生成する。`ExprKind::Slice` をさらに `ExprKind::Borrow` で包んではいけない。`&mut` は slice 構文を認識せず、`&mut xs[a..b]` は `E0002` または `E1014` で拒否する。

実装方針:

```rust
enum SliceContext { Forbid, Borrow }

fn prefix(&mut self, allow_record: bool, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
    let token = self.take();
    let mutable = token.kind == TokenKind::Ampersand && self.eat(&TokenKind::Mut);
    if token.kind == TokenKind::Ampersand && !mutable {
        let previous = std::mem::replace(&mut self.slice_context, SliceContext::Borrow);
        let value = self.expression_inner(13, allow_record, stop_at_newline)?;
        self.slice_context = previous;
        if matches!(value.kind, ExprKind::Slice { .. }) {
            return Ok(value); // already has type &[T]; do not wrap in Borrow.
        }
        return self.make(ExprKind::Borrow(Box::new(value), false), token.span.through(value.span), value.depth + 1);
    }
    let value = self.expression_inner(13, allow_record, stop_at_newline)?;
    self.make(ExprKind::Borrow(Box::new(value), mutable), ...)
}
```

`src/parser.rs` `postfix` の `TokenKind::LeftBracket` 分岐は range bracket を検出する。ただし `slice_context == Borrow` の時だけ `ExprKind::Slice` を返し、それ以外では `E0002` を返す。

```rust
if self.eat(&TokenKind::DotDot) {
    if self.slice_context != SliceContext::Borrow {
        return Err(Diagnostic::new("E0002", "array slices must be borrowed; write '&xs[..end]'", left.span));
    }
    let end = self.expression_until_slice_end()?;
    return ExprKind::Slice { value: left, start: None, end: Some(end) };
}
let first = self.expression_until_dotdot_or_right_bracket()?;
if self.eat(&TokenKind::DotDot) {
    if self.slice_context != SliceContext::Borrow {
        return Err(Diagnostic::new("E0002", "array slices must be borrowed; write '&xs[start..end]'", left.span));
    }
    let end = if self.at(&TokenKind::RightBracket) { None } else { Some(self.expression_until_slice_end()?) };
    return ExprKind::Slice { value: left, start: Some(first), end };
}
ExprKind::Index(left, first)
```

`expression(0,true)` は `..` を通常の range として食べてしまうため、index bracket 内では `stop_at_slice_dotdot` flag を追加し、`expression_inner` が `DotDot` の前で止まるようにする。`stop_at_arm` / `stop_at_arrow` と同じ Parser field で実装する。

### Type checking

`Checker::value_expression` に `ExprKind::Slice` を追加。

- `value` は `self.expression(value, None)` 後、`Self::autoderef` で `&[T]` も入力にできる。
- 入力型が `Type::Array(element)` なら受理。`Type::String` / `Type::List` は `E1005`。
- `start` / `end` は `Type::I64`。
- `start` 省略時は `0i64` の `TypedExprKind::Int(0)` を内部生成してもよいが、children 追跡を単純にするため `Option` のまま lowering で扱う。
- `end` 省略時は len を使う。

### Polymorph / closures / recursion / specialization

追加 variant をすべての walker に加える。

- `computation.rs` `expand`: `ExprKind::Slice` の子を展開。
- `polymorph.rs` `walk` は `TypedExpr::children_mut` 経由なので children 更新で足りる。
- `closures.rs` `free_locals` は children 更新で足りる。
- `recursion.rs` は children 更新で足りる。
- `call_specialization.rs` `all_children` は children 更新で足りる。`read_only` は `Slice(value,start,end)` を `value` read、start/end consume として扱う。

### Ownership

`src/ownership.rs`:

- `eval_value` に `E::Slice` を追加。
- `value` は `self.eval(value, Use::Borrow, ...)` ではなく、`E::Borrow` 分岐と同じ手順で loan を作る。`read_places` は `Use::Read` の時だけ loan を作るため、`Use::Borrow` を渡すだけでは slice loan が作られない。
- `self.place(value, &during)?` で source places を得る。
- 各 `(place, via)` に `self.access(&place, &via, Use::Borrow, span)?` を実行する。
- `self.loan(place, false, via)` で共有 loan を作り、result に入れる。`via` は slice of slice の親 loan set なので保持する。
- `start` / `end` は slice loan を `held` に入れた状態で `Use::Consume` 評価する。これにより `&xs[{ xs = []; 0 }..]` は `E1014`。
- result.loans は source loan。

擬似コード:

```rust
let mut result = Value::default();
for (place, via) in self.place(value, &during)? {
    self.access(&place, &via, Use::Borrow, expression.span)?;
    result.loans.insert(self.loan(place, false, via));
}
self.held.push(result.clone());
self.eval(start, Use::Consume, &during)?;
self.eval(end, Use::Consume, &during)?;
self.held.pop();
```

`place` / `is_place`:

- `Dereference` of shared array reference is not an addressable place. `*slice` value copy is handled in LLVM expression, not `place`.
- `Index` of slice for `&slice[i]` needs addressable element. Since slice itself is borrowed view, `Index(value, i)` where `value.ty` is array after autoderef is tricky. Phase 1 design: do not autoderef slice for `&slice[i]`; add `TypedExprKind::SliceIndexPlace` is too large. Instead, allow `slice[i]` as read-only value and `&slice[i]` by treating `Index(Dereference(slice), i)` as place rooted in original loan in ownership.
- 実装を単純にするため、`Checker::autoderef` は `&[T]` を `Dereference` に変えるが、`ownership::place(Dereference(reference))` は reference の loans を元 owner place として返す既存経路を使う。LLVM の `place(Dereference)` だけ special-case する。

### LLVM lowering

Helper:

```rust
fn shared_array_ref_element(ty: &Type) -> Option<&Type>;
fn array_view(&mut self, expression: &TypedExpr) -> (String, Vec<Frame>);
fn slice_bounds(&mut self, len: &str, start: Option<&TypedExpr>, end: Option<&TypedExpr>) -> (String, String);
```

`shared_array_ref_element` は `Type::Reference(inner, false)` かつ `inner.as_ref() == Type::Array(element)` の時だけ `Some(element)`。

`expression_mode` の先頭は現在 `if Self::is_place(expression) { return self.read_place(...) }` である。C03 後はこの前に shared-array-reference dereference を処理する。

```rust
if let Some((reference, element)) = self.shared_array_deref(expression) {
    let view = self.expression_mode(reference, false); // %tz.array descriptor
    return if take {
        self.copy_array_view(element, &view) // *slice
    } else {
        view                              // non-consuming read of *slice
    };
}
if Self::is_place(expression) {
    return self.read_place(expression, take, true);
}
```

`Borrow(value,false)`:

- If `value.ty == Type::Array(_)`, return descriptor value from `read_operand(value)` instead of slot pointer.
- Still ownership has loaned `value`; LLVM only changes representation.

`Slice` lowering:

```llvm
%array = ; descriptor from value
%data = extractvalue %tz.array %array, 0
%len  = extractvalue %tz.array %array, 1
%start = ...
%end = ...
%start_ok = icmp ule i64 %start, %len
%order_ok = icmp ule i64 %start, %end
%end_ok = icmp ule i64 %end, %len
%ok = and i1 %start_ok, and i1 %order_ok, %end_ok
guard %ok
%ptr = getelementptr inbounds T, ptr %data, i64 %start
%slice_len = sub i64 %end, %start
%view0 = insertvalue %tz.array zeroinitializer, ptr %ptr, 0
%view = insertvalue %tz.array %view0, i64 %slice_len, 1
```

`Length`:

- If operand is `Dereference(ref)` where ref type is shared array ref, use `%tz.array` value directly and `extractvalue 1`; do not allocate clone.

`Index`:

- If operand is shared array ref or deref of it, use view descriptor and `checked_element_pointer` equivalent.
- Element load result is `clone_value(element, loaded)` like arrays.
- Bounds check is relative to slice length.

`place(Index(Dereference(slice), index))` for `&slice[i]`:

- `place` must special-case this before the generic `Index` path that assumes `place(value)` returns a pointer to an aggregate descriptor.
- Evaluate the slice reference value as `%tz.array` descriptor, evaluate `index`, bounds-check relative to descriptor length, and return the element pointer directly.
- This is the only shared-array-reference dereference that is addressable as a place. `place(Dereference(slice))` for the whole slice is not a pointer to a descriptor and must not return `%tz.array` as if it were `ptr`.

`Dereference` of shared array ref as value:

- Allocate new owned array of slice length with `allocate_array`.
- Loop from `0..len`, load element from slice data, `clone_value`, store into new array.
- This requires `Copy` from ownership for user-observable non-Copy moves. If generic, ownership infers `Copy 'a`.

`ForEach`:

- Source can be shared array ref view. `for_each` extracts data/length from `%tz.array` value and iterates.

### LLVM / layout coverage table

| site | 変更 |
|---|---|
| `llvm_type` | `Type::Reference(Array(_), false)` は `%tz.array`、その他 reference は `ptr` |
| `check.rs` `layout_size` | shared `&[T]` は 16 bytes。その他 reference は 8 bytes |
| `check.rs` `validate_size` | shared `&[T]` は inner element を検査しつつ 16 bytes を返す。その他 reference は 8 bytes |
| `llvm_frame.rs` `stack_size` | shared `&[T]` は 16 bytes。その他 reference は 8 bytes |
| `expression_mode` pre-place | `Dereference(shared &[T])` を `read_place` に流さず、non-consuming read は descriptor、take は deep copy |
| `read_place` | local/field に格納された shared `&[T]` は `%tz.array` として load される |
| `place(Dereference)` | shared `&[T]` whole deref は `ptr` ではないため generic path 禁止 |
| `place(Index(Dereference(slice), i))` | `&slice[i]` のため slice descriptor から element pointer を直接計算 |
| `Borrow(value,false)` | `&array` は descriptor value を返す。`&mut array` は従来 `ptr` |
| `Dereference` | shared `&[T]` の value deref は owned array copy |
| `Length` | shared `&[T]` は descriptor length を読むだけ |
| `Index` | shared `&[T]` は descriptor bounds check + element load |
| `ForEach` | shared `&[T]` は descriptor data/length を走査 |
| closure capture / partial application / function args / returns | `llvm_type` が `%tz.array` を返すため closure env、adapter、call signature の型も 16-byte descriptor になる |
| tuples / arrays containing slices | `layout_size` / `validate_size` / `stack_size` が 16 bytes として扱う。record field は `contains_reference` により既存通り拒否 |

### Frame storage

`src/llvm_frame.rs`:

- `frame_value` for slice itself returns normal expression; slice owns no stack storage.
- `drop_framed` for Type::Reference is no-op, including shared array refs。
- If `*slice` creates owned array, it is heap allocation; no frame candidate.

## 実装手順

1. **Parser**
   - `ExprKind::Slice`、`slice_context`、`stop_at_slice_dotdot` を追加。
   - `&` prefix 内だけ `ExprKind::Slice` を直接生成し、裸 `xs[1..3]` は `E0002`。
   - 確認: parser unit test で `&xs[1..3]`, `&xs[1..]`, `&xs[..3]` と裸 slice rejection。

2. **Typed IR / walkers**
   - `TypedExprKind::Slice` と children 更新。
   - `computation.rs` expand 更新。
   - 確認: `Builder { let x = &xs[1..2]; return x.length }` の内側が展開される。

3. **型検査**
   - Slice typing と `&[T]` representation special-case。
   - `llvm_type(Type::Reference(Array,false)) = "%tz.array"` はこの段階ではまだ LLVM test だけ。
   - 確認: `def f :: &[i64] -> i64` 既存コードが型検査され続ける。

4. **Ownership**
   - `E::Slice` 評価は `E::Borrow` と同じく `place` / `access(Use::Borrow)` / `loan` を使う。index 式中の借用保護、lifetime propagation。
   - 確認: `let s = &xs[0..1]; xs = []; s.length` が `E1014`。

5. **LLVM representation migration**
   - coverage table の全 site を更新する。
   - `Borrow` / `Dereference` / `Length` / `Index` / `ForEach` / `place(Index(Dereference(slice), i))` を special-case。
   - 確認: `fn len(s: &[i64]) = s.length` の引数型が `%tz.array` で、body に array clone がない。

6. **Dereference copy**
   - `*slice` owned array copy を実装。
   - 確認: `let s = &xs[1..3]; let ys = *s; ys.length` で allocation + copy loop。

7. **Docs / E2E**
   - fixture と native/WASM tests。
   - 確認: 全指定検証。

## テスト計画

### Rust: `tests/slices.rs`

受理:

- `def len :: &[i64] -> i64\nfn len xs = xs.length`
- `let xs = [1,2,3,4]\nlet s = &xs[1..3]\ns[0] + s.length`
- `&xs[1..]`, `&xs[..2]`
- `def first :: &[string] -> &string\nfn first xs = &xs[0]`
- `for x in &xs[1..3] do total = total + x`
- `def copy :: &[i64] -> [i64]\nfn copy xs = *xs`
- slice of slice: `let s = &xs[1..4]; let t = &s[1..2]`
- `let r = &s[0]` where `s: &[string]`。`&slice[i]` が element pointer を直接計算する。
- closure capture: `let s = &xs[1..3]; let f = u -> s.length; f ()`
- function value call: `def len :: &[i64] -> i64\nfn len s = s.length\nlet f = len; f (&xs[0..2])`
- partial application: `def add_len :: &[i64] -> i64 -> i64\nfn add_len s n = s.length + n\nlet f = add_len (&xs[0..2]); f 10`
- tuple/array containing slices: `let pair = (&xs[0..1], &xs[1..2]); pair.0.length + pair.1.length`、`let slices = [&xs[0..1], &xs[1..2]]`
- direct call: `len (&xs[0..2])`

拒否:

- `xs[0..1]` → `E0002`（`array slices must be borrowed; write '&xs[0..1]'`）
- `&xs[..]` → `E0002`
- `&list[0..1]` → `E1005`
- `&text[0..1]` → `E1005`
- `&mut xs[0..1]` → `E1005` または `E1014`
- `record R { s: &[i64] }` → `E1013`
- `task { return &xs[0..1] }` / task capture → `E1013`
- `let s = { let xs = [1,2]; &xs[0..1] }; s.length` → `E1013`
- `let mut xs = [1,2]; let s = &xs[0..1]; xs = [3]; s.length` → `E1014`

IR:

- `def len :: &[i64] -> i64` の worker 引数が `%tz.array`。
- `s.length` に `@tz.alloc` がない。
- `s[0]` は `icmp ult` → GEP → load。
- `&s[0]` は `%tz.array` descriptor から GEP し、`place(Dereference)` の generic ptr path を通らない。
- `*s` には `@tz.alloc` と copy loop がある。
- `&xs[1..3]` で `icmp ule` checks が GEP より前。

検証コマンドは GUIDE §3 に従い、Rust は `cargo test --locked --test slices` で `running N tests` の `N > 0` を確認する。Node E2E の直前には必ず `cargo build --release --locked` を実行し、`target/release/tsuzuri` を更新してから `node tests/*.mjs target/release/tsuzuri` を走らせる。

### E2E fixture: `tests/fixtures/slices/Main.tz`

公開関数:

- `slice_sum(n,a,b)` → JS BigInt で `[0..n)` の `a..b` sum。
- `slice_nested()` → slice of slice checksum。
- `slice_copy_owned()` → `*slice` の copy 後に元配列を drop しても値が残る。
- `slice_string_refs()` → `[string]` の slice から `&string` を返す内部関数と `clone_string`。
- `slice_bounds_start_neg`, `slice_bounds_end_large`, `slice_bounds_inverted` は trap。

Targets:

- native/WASM × `-O0`/`-O3`
- native heap tracking: read-only slice functions allocation 0、`*slice` は 1 allocation 以上で `live == 0`。
- WASM imports empty。

## ドキュメント

- `docs/language.md`
  - 「配列・連結リストとレコード」に `&xs[a..b]`。
  - 「Ownership / Borrowing」に slice loan と禁止例。
  - 「for…in」に `&[T]` 走査。
- `docs/architecture.md`
  - `&[T]` の fat representation `{ ptr, i64 }`。
  - `&mut [T]` と `&[T]` の LLVM 表現差。
  - bounds-before-GEP invariant。
- `README.md`
  - C04 前提の短い slice 例。

## 受け入れ条件

- [ ] `&xs[a..b]`, `&xs[a..]`, `&xs[..b]` が受理される。
- [ ] 裸 `xs[a..b]` は `E0002` で拒否され、`Borrow(Slice)` による `&&[T]` を作らない。
- [ ] `&[T]` は shared array reference として `%tz.array` に下がる。
- [ ] `&mut [T]`、`&string` は既存 pointer reference のまま。
- [ ] `layout_size` / `validate_size` / `llvm_frame::stack_size` は shared `&[T]` を 16 bytes として扱う。
- [ ] bounds check が GEP/load より前。
- [ ] slice は owner の loan を運び、move/assignment/`&mut` 競合を拒否。
- [ ] `s.length`, `s[i]`, `for x in s` は owned array copy なし。
- [ ] `*s` は owned copy を作り、drop/clone が正しい。
- [ ] record field / task boundary の reference 禁止が維持される。
- [ ] native/WASM × `-O0`/`-O3` E2E、heap tracking、WASM imports empty。

## 落とし穴

- `Type::Reference(Array,false)` を `%tz.array` に変えると、`FunctionEmitter::place(Dereference)` の前提「reference は ptr」が壊れる。すべての deref / length / index / for-in site を列挙して special-case する。
- `expression_mode` の `is_place` fast path より前に shared `&[T]` deref を処理しないと、`read_place` が `%tz.array` を pointer と誤解する。
- `self.eval(value, Use::Borrow, ...)` は loan を作らない。slice ownership は `E::Borrow` 分岐と同じく `place` / `access` / `loan` を直接呼ぶ。
- `Checker::autoderef` が `&[T]` を owned `[T]` と見せる箇所で、LLVM が clone を挟まないよう注意する。
- `&xs[a..b]` の index 式評価中に `xs` を変更できないよう、ownership で source loan を held に入れる。
- `&xs[a..]` で `a > len` のとき `len - a` を先に計算すると underflow する。検査後に `sub`。
- `&xs[..b]` の `b == len` は有効。`icmp ult` ではなく `icmp ule`。
- `&string[a..b]` を誤って byte slice として受理しない。UTF-8 境界は D02。

## 対象外

- `&mut [T]` の部分 slice。
- list slice。
- string slice。
- 保存可能な range object。
- Slice literal / pattern。
- Host ABI buffer 変換（E05）。
- Parallel splitting（F02）。

## 未決事項

- 既定案は direct migration: `Type::Reference(Array,false)` の LLVM 型を `%tz.array` にする。内部 `Type::Slice` は作らない。
- もし実装中に `Type::Reference` special-case が既存関数値・closure ABI と衝突する場合、段階移行案として `Type::Slice(Box<Type>)` を内部型に追加し、`display` だけ `&[T]` にする。その場合は GUIDE §9 D-13 に「台帳の見直し提案」を追記する。
- `&xs[..]` は不許可の既定案。必要なら将来 `&xs` と同義にできるが、構文上の価値が低い。
- 台帳の見直し提案: 現時点では不要。上記 fallback を採用した場合のみ D-13 に追記する。
