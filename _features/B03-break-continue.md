# B03: break／continue
| 項目 | 内容 |
|---|---|
| ID | B03 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | C07, G03 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/check.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `src/polymorph.rs`, `src/closures.rs`, `src/recursion.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `tests/control.rs`, `tests/fixtures/control/`, `tests/control.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

通常の `for...in`、`for...to`/`downto`、`while` ループで、最内ループを終了する `break` と次の反復へ進む `continue` を追加する。ループ条件用の可変フラグに頼らず、探索・検査・スキップを明示できるようにする。

このチケットは通常ループだけを対象にし、コンピュテーション式の `For`/`While` や `task`/lambda 境界を越える制御移動は導入しない。

## 現状

- `docs/language.md` の「for / while」は「break／continue はなく、必要なら条件をローカル状態で表します」と明記している。
- `src/lexer.rs::Lexer::identifier` は `break`/`continue` をキーワードとして登録していない。
- `src/syntax.rs::TokenKind` に `Break`/`Continue` はない。
- `src/syntax.rs::ExprKind` は `While`, `For`, `Range`, `Match` などを持つが、非局所ジャンプ式はない。
- `src/parse_control.rs::while_expression`, `for_expression`, `for_source` が通常 loop を AST にする。
- `src/parser.rs::computation_statement` は builder 内の `for`/`while` を `ComputationStatementKind::For`/`While` にし、`src/computation.rs::Lowering::block` が `B.For`/`B.While` 呼び出しへ変換する。
- `src/check.rs::Checker::control_expression` は:
  - `ExprKind::While` → `TypedExprKind::While { condition, body }`
  - `ExprKind::For` + counted/range → `TypedExprKind::ForRange`
  - `ExprKind::For` + array/list/string → `TypedExprKind::ForEach`
  - いずれも `Type::Unit`
- `src/ownership_control.rs::check_loop` は loop entry と body/back edge を固定点で検査するが、途中 exit/continue edge はまだない。
- `src/llvm_control.rs::while_loop`, `range_loop`, `narrow_range_loop`, `for_each` が LLVM 分岐を直接生成する。
- `src/llvm.rs::FunctionEmitter::tail` は自己末尾再帰を `loop` label へ戻す。通常 loop の `break`/`continue` と混同してはいけない。

### 設計決定台帳

- D-15 により `break`／`continue` は新しい予約語。
- D-16 により不正な `break`／`continue` は `E1023`。

## 仕様

### 構文

```text
Expression ::=
    ...
  | "break"
  | "continue"

LoopBody ::=
    "while" Expression "do" BodyExpression
  | "for" Pattern "in" Expression "do" BodyExpression
  | "for" Ident "=" Expression ("to" | "downto") Expression "do" BodyExpression
```

例:

```text
let mut total = 0
for n in values do
    if n < 0 then break
    else if n == 0 then continue
    else total = total + n
total
```

### 有効範囲

- `break` は最内の通常 loop から抜ける。
- `continue` は最内の通常 loop の次反復へ進む。
- label 付き break/continue は phase 2 まで対象外。
- `break`/`continue` は以下で `E1023`:
  - ループ外。
  - loop の内側で作った lambda の本体。例: `while true do { let f = x -> break; f 0 }`
  - loop の内側で作った `task { ... }` の本体。例: `while true do { let t = task { break }; () }`
  - custom computation expression の builder statement としての `for`/`while` の body。例: `Option { while true do break }`
  - ただし computation body の通常式の中にある普通の `ExprKind::For`/`ExprKind::While` は通常 loop target を作る。例: `Option { let x = { while true do break; 0 }; return x }` の `break` は内側の普通の `while` を対象にできる。
- `break`/`continue` は関数境界を越えない。名前付き関数、lambda、task、builder 継続はいずれも境界。

### 型

phase 1 では `break`/`continue` は **unit 型のジャンプ式** とする。

- `while condition do break` は受理。
- `if condition then break else ()` は受理。
- `if condition then break else 1` は `E1003`。理由: 現在の `TypedExprKind::If` と `FunctionEmitter::expression_mode` は両分岐の値を `phi` で合流させる値式であり、片側だけ非局所ジャンプする任意型の bottom を導入すると LLVM/所有権の制御フロー表現が大きく変わるため。
- 将来 phase 2 で `Never`/bottom 型を導入する余地は残すが、このチケットでは実装しない。
- loop 自体は従来どおり `unit`。

### 評価順序

- `break`/`continue` に引数はなく、評価する子式もない。
- `if`/`match`/短絡演算の既存評価順序を保つ。到達した `break`/`continue` 以降の同じ実行経路の式は評価しない。
- `break`/`continue` の直前までに評価済みの一時所有値は、ジャンプ前に解放する。
- 選択されない分岐にある `break`/`continue` は実行しないが、型検査・所有権検査は行う。

### 所有権・借用

- `break` edge:
  - loop body の現在スコープで作った所有ローカルを内側から外側へ drop してから loop exit へ分岐する。
  - for pattern の一時束縛、アクティブパターン recognizer の一時所有値、`match` arm の pattern temporaries を drop する。
  - `for...in` の列挙元 owner（`TypedExprKind::ForEach { owner, ... }`）は loop exit 後に通常の loop scope として解放する。temporary source の場合も解放漏れしない。
  - range loop の start/step/finish は整数値なので drop 不要だが、将来型拡張しても評価済み所有値は exit scope に含める。
  - 生存中の borrow が loop body のローカルや iteration binding を指す場合は `E1013`。
- `continue` edge:
  - body 内の現在スコープで作った所有ローカル・pattern temporaries を drop してから loop の advance/test へ分岐する。
  - `for...in` の element binding は borrow alias なので drop しないが、その binding への loan は反復末で終了する。
  - `for...to`/`downto`/range loop は induction variable の次値計算へ進む。`narrow_range_loop` では i64 拡張 induction の `advance` label へ進む。
  - `while` は condition test label へ進む。
- 外側の所有値を `break` する経路で move し、loop 継続経路で使う場合は既存の分岐合流と同じく move 後使用を拒否する。

### 診断

`E1023` のメッセージは英語、小文字始まり、修正方法を含める。

推奨メッセージ:

- ループ外: `"break can only target the innermost for or while loop"`
- `continue` ループ外: `"continue can only target the innermost for or while loop"`
- 関数境界: `"break cannot cross a function, task, or computation boundary; return a value and handle it in the loop"`
- builder: `"break cannot target a computation builder loop; use the builder's Option/Result value to stop the computation"`

## 設計

### AST

`src/syntax.rs`:

```rust
pub enum TokenKind {
    ...
    Break,
    Continue,
}

pub enum ExprKind {
    ...
    Break,
    Continue,
}
```

`ExprKind::Break`/`Continue` は `depth = 1`、子なし。

### Lexer

`src/lexer.rs::Lexer::identifier` に追加:

```rust
"break" => TokenKind::Break,
"continue" => TokenKind::Continue,
```

予約語化により既存コードの `let break = ...` は構文エラーになる。`tests/frontend.rs` または `tests/control.rs` に予約語テストを追加する。

### Parser

`src/parser.rs::primary` に追加:

```rust
TokenKind::Break => {
    self.take();
    ExprKind::Break
}
TokenKind::Continue => {
    self.take();
    ExprKind::Continue
}
```

`src/parser.rs::computation_statement` は `break`/`continue` を通常式として読めてしまうため、後段の型検査で通常 loop target の有無を見て `E1023` にする。parser で builder と通常 loop の target を判定しようとしない。

### Computation expansion

`src/computation.rs::expand`:

- `ExprKind::Break | ExprKind::Continue` は子なしとして扱う。

`src/computation.rs::expand_block`:

- `ComputationStatementKind::For` / `While` は builder の `B.For` / `B.While` 関数呼び出しへ展開されるだけで、通常 loop target を作らない。
- `expand_block` では `break`/`continue` を一律拒否しない。`let` 右辺や通常式の中に `ExprKind::For` / `While` があれば、その内側の `break`/`continue` は普通の loop を対象にできるため。
- 代わりに型検査で `normal_loop_depth` を tracking し、lambda/task/nested computation 境界では depth をリセットする。builder `For`/`While` の body を lower して作る continuation lambda も境界なので、そこから外側の通常 loop へ飛ばせない。

### 型検査

`src/check.rs::Checker` に loop context を追加する。

```rust
struct LoopContext {
    span: Span,
}

struct Checker<'a> {
    ...
    normal_loop_stack: Vec<LoopContext>,
}
```

- `Checker::control_expression`:
  - `ExprKind::While`、通常 `ForRange`、通常 `ForEach` の body を検査する間だけ `normal_loop_stack.push(...)`。
  - 検査後に pop。
- `Checker::composed_expression`:
  - `ExprKind::Lambda`、`ExprKind::Task`、`ExprKind::Computation` へ入るときは、現在の normal loop stack を見えないものとして扱う。実装は `let saved = std::mem::take(&mut self.normal_loop_stack)` して body 検査後に戻す。
  - computation expression の builder `For`/`While` は normal loop stack を push しない。computation 内の `let x = while ...` のような通常式だけが `Checker::control_expression` 経由で push する。
- `Checker::value_expression`:
  - `ExprKind::Break`/`Continue` を処理する。
  - `normal_loop_stack.is_empty()` なら `E1023`。
  - `expected` があれば `Type::Unit` と単一化。なければ `Type::Unit`。
  - `TypedExprKind::Break`/`Continue` を返す。

`TypedExprKind`:

```rust
pub enum TypedExprKind {
    ...
    Break,
    Continue,
}
```

`TypedExpr::children`/`children_mut` は子なしにする。既存の `_ => Vec::new()` に頼らず、チェックリストに従い明示的に確認する。

### Polymorph / closures / recursion / call_specialization

- `src/polymorph.rs`:
  - 型置換・walk は `children` 経由なら基本変更なし。
  - `TypedExprKind::Break`/`Continue` は型を持つが子なし。
- `src/closures.rs`:
  - `free_locals`/`lower_expression` は子なしとして扱う。
  - lambda/task に入る前の型検査で `E1023` にするため、lowering 時点に境界越え jump は残らない。
- `src/recursion.rs`:
  - `references` は子なし。
- `src/call_specialization.rs`:
  - `single_use_locals` は loop 内使用を保守的に複数使用へ引き上げる既存処理を維持。
  - `may_mutate`, `non_escaping`, `read_only` は `all_children` 経由で子なし。
  - jump 後の dead block により IR 上は到達不能でも、最適化で意味を変えない。

### Ownership

`src/ownership.rs`:

- `eval_value` または `eval_composed` に `Break`/`Continue` を追加し、専用の control jump を `ownership_control.rs` へ伝える。
- 既存 `Value` だけでは通常値と jump edge を区別できないため、明示的な fallthrough と edge 集合を持つ内部表現を導入する。

推奨内部表現:

```rust
#[derive(Clone)]
struct Flow {
    fallthrough: Option<(State, Value)>,
    breaks: Vec<State>,
    continues: Vec<State>,
}
```

実装しやすい形として、`Checker` に scoped loop-flow stack と `reachable: bool` を持たせてもよい。ただしその場合も上の `Flow` と同じ情報を保持する。

- `Break`/`Continue` 到達時:
  1. 現在スコープの drop/loan cleanup を適用した `State` を対応する edge collector に push。
  2. `reachable = false` にする。
  3. 後続式・後続 statement は型検査済み IR に残すが、所有権検査では破棄用の到達不能 state 上で検査し、fallthrough へ merge しない。
- block/if/match:
  - `fallthrough` が `None` の分岐は後続の通常 state へ合流しない。
  - `if c then break else ()` の後にさらに statement がある場合、then 側は `breaks` へ入り `fallthrough = None`、else 側だけが後続 statement の入力 state になる。後続 statement は else state で通常検査される。
  - 両分岐が `break`/`continue` なら block の `fallthrough` は `None`。型検査は既に済んでいるが、所有権の正常継続 state は存在しない。
- nested loop:
  - 内側 loop は自身の `breaks`/`continues` を消費して 1 つの通常 `Flow` に戻す。外側 loop の edge collectors に漏らさない。
  - 固定点 1 iteration ごとに edge collectors を空にして body を再評価する。前回 iteration の break/continue edge を再利用しない。

`src/ownership_control.rs::check_loop` を拡張する。

- 入口 state `entry`、condition false の exit state、body の通常 fallthrough state、`continue` states、`break` states を分ける。
- body の通常 fallthrough state と cleaned `continue` states は back edge として `entry` へ merge し、固定点に含める。
- condition false state と cleaned `break` states は loop の exit state 候補であり、固定点収束後に merge して `self.state` にする。
- `finish_control_scope` を使い、iteration binding と pattern temporaries への borrow が外へ出ないことを検査する。
- 解析上限は既存通り `syntax::MAX_NESTING` 回。超過は `E1017`。

drop/loan の対象:

- body block の `bindings`。
- `PatternStep::Bind` で作る recognizer 一時 local。
- `match` arm の binding alias。
- `for` destructuring 用の `$iteration...` local。
- `ForEach` の temporary `owner`（source が place でなく temporary の場合）。
- `ForEach` の protected loan (`held`)。
- `ForRange` の loop local。

### LLVM

`src/llvm.rs::FunctionEmitter` に loop target stack を追加する。

```rust
struct LoopTargets {
    break_label: String,
    continue_label: String,
    scope_base: usize,
}
```

`scope_base` は loop に入る前の `self.scopes.len()`。`break`/`continue` で内側 scopes を drop するために使う。

`FunctionEmitter::expression_mode`:

```rust
TypedExprKind::Break => {
    self.emit_loop_jump(true);
    "0".into()
}
TypedExprKind::Continue => {
    self.emit_loop_jump(false);
    "0".into()
}
```

`emit_loop_jump`:

1. 現在の block から target までにある scopes を内側から `drop_scope`。
2. pattern temporaries が active な場合は `clear_pattern_temporaries` 相当を通る。match 内の `break`/`continue` は `match_expression` の arm scope と temporaries scope に入っているため、scope stack に入るよう整理する。
3. `jump(target_label)`。
4. 直後に `begin(self.label())` で dead block を作る。これにより後続の既存 emitter が命令を追加しても invalid IR にならない。

各 loop lowering:

- `llvm_control.rs::while_loop`:
  - `continue_label = test`
  - `break_label = exit`
  - body 生成中だけ push。
- `range_loop`:
  - wide（64/128-bit または unsigned 64/128）unit-step 経路（`one || minus_one`）では、現在の実装が body の直後に `current == last` を検査し、その後に `advance` で加減算する。`continue_label = advance` にすると endpoint 検査を飛ばし、MAX/MIN で折り返すため禁止。
  - wide unit-step 経路では body の直後に専用 `latch` label を置く。通常の body completion と `continue` はどちらも `latch` へ分岐し、`latch` で `current == last` を検査する。final iteration なら `exit`、final でなければ `advance` で次値を計算して `work` へ戻る。
  - arbitrary step 経路: `continue_label = advance`
  - `break_label = exit`
- `narrow_range_loop`:
  - `continue_label = advance`
  - `break_label = exit`
  - induction は i64 のまま、`continue` で narrowing assume を再実行しない。body に入った後なので次は advance。
- `for_each`:
  - 配列/list 共通で `continue_label = advance` が必要。現在の `array_loop`/`list_loop` は callback に `advance` label を公開していないため、`array_loop`/`list_loop` を `LoopLabels` を扱える形に分割する。
  - `break_label = exit`
  - `release_operand(source, collection, frames)` は exit block で一度だけ実行する。break edge も必ず exit に来る。
- `hint_loop`:
  - `continue` で advance/test に来ても loop metadata は既存と同じ。固定 unroll や再結合はしない。
- tail recursion:
  - `FunctionEmitter::tail` の `"loop"` label は関数先頭の自己末尾再帰用。通常 loop の `continue` stack と別管理し、`break`/`continue` から tail recursion loop へ飛ばない。

### IR 形の例

`while`:

```llvm
br label %test
test:
  %c = ...
  br i1 %c, label %body, label %exit
body:
  ; if continue
  br label %test
  ; if break
  br label %exit
  ; normal body end
  br label %test
exit:
```

`for range`:

```llvm
work:
  store i32 %current, ptr %i
  ; body
  ; continue jumps to %latch in wide unit-step loops, or %advance in overflow-checked arbitrary-step loops
  ; break jumps to %exit
  br label %latch
latch:
  ; wide unit-step only: endpoint check before increment/decrement
  br i1 (%current == %last), label %exit, label %advance
advance:
  ; increment/decrement or overflow-checked arbitrary-step handling
```

## 実装手順

1. **キーワードと AST を追加する**
   - `TokenKind::Break`, `TokenKind::Continue`。
   - `Lexer::identifier` に登録。
   - `ExprKind::Break`, `ExprKind::Continue`。
   - `Parser::primary` で読む。
   - 確認: `cargo test --locked --test control break_continue_parse_keywords`（`running N tests` の N が 0 でないこと）。予約語として識別子利用が拒否されることも確認。
2. **型検査の loop context を追加する**
   - `Checker::control_expression` で通常 loop body の間だけ context push。
   - lambda/task/computation 境界では context を切る。
   - `Break`/`Continue` を unit 型にする。
   - 不正位置は `E1023`。
   - 確認: `cargo test --locked --test control break_continue_type_contexts`（`running N tests` の N が 0 でないこと）。
3. **IR 走査漏れを埋める**
   - `TypedExpr::children`/`children_mut`、`polymorph`, `closures`, `recursion`, `call_specialization` で子なし扱いを確認。
   - 確認: `cargo test --locked --test control control_syntax_has_bounded_depth`（`running N tests` の N が 0 でないこと）が引き続き通る。
4. **LLVM lowering を実装する**
   - `FunctionEmitter` に loop stack。
   - `while_loop`, `range_loop`, `narrow_range_loop`, `for_each` で break/continue target を push/pop。
   - jump 後 dead block を作る。
   - 確認: `cargo test --locked --test control break_continue_ir_shapes`（`running N tests` の N が 0 でないこと）。IR に invalid predecessor/phi がないこと。
5. **所有権検査を拡張する**
   - `ownership_control.rs::check_loop` に break/continue edge を含める。
   - per-iteration owned values、pattern temporaries、for-each source owner の drop/loan を確認する。
   - 確認: `cargo test --locked --test control break_continue_ownership`（`running N tests` の N が 0 でないこと）。
6. **E2E fixture を追加する**
   - `tests/fixtures/control/BreakContinue.tz` または既存 `Main.tz` に export 関数を追加。
   - `tests/control.mjs` の `cases`/`traps` に追加。
   - 確認: `cargo build --release --locked && node tests/control.mjs target/release/tsuzuri` で native/WASM × `-O0`/`-O3`、heap tracking `live == 0`。
7. **文書更新**
   - `docs/language.md` の「break／continue はない」を置き換える。
   - D-16 の診断表に `E1023` が既にあることを確認し、説明を追加。

## テスト計画

### Rust 受理

```text
let mut n = 0
while true do {
    n = n + 1
    if n == 42 then break else ()
}
n
```

```text
let mut total = 0
for i = 1 to 10 do
    if i % 2 == 0 then continue else total = total + i as i64
total
```

```text
let mut total = 0
for i = 10 downto 1 do
    if i == 5 then break else total = total + i as i64
total
```

```text
let mut total = 0
for n in [1, 2, 3, 4] do {
    if n == 3 then break else ()
    total = total + n
}
total
```

```text
let mut total = 0
for (x, y) in [(1, 2), (3, 4)] do {
    if x == 3 then continue else ()
    total = total + x + y
}
total
```

### Rust 拒否

| プログラム | 期待コード | 理由 |
|---|---|---|
| `break` | `E1023` | ループ外 |
| `continue` | `E1023` | ループ外 |
| `while true do { let f = x -> break; f 0 }` | `E1023` | lambda 境界 |
| `while true do { let work = task { break }; () }` | `E1023` | task 境界 |
| `Identity { while true do break }` | `E1023` | builder loop |
| `while true do { if true then break else 1; }` | `E1003` | phase 1 では unit 型 |
| `let r: i64 = while true do { let x = 1; break; }\nr` | `E1003` | loop は unit なので i64 注釈と不一致 |
| `let r = while true do { let x = 1; break; }\nr + 1` | `E1003` | `r` は unit なので加算できない |

### 所有権

受理:

```text
let mut n = 0
while n < 10 do {
    let text = "owned"
    if n == 3 then break else ()
    n = n + text.length
}
n
```

受理（continue で per-iteration string を解放）:

```text
let mut total = 0
for n in [1, 2, 3] do {
    let text = "owned"
    if n == 2 then continue else ()
    total = total + text.length
}
total
```

拒否:

```text
let seed = 0
let mut r = &seed
while true do {
    let x = 1
    r = &x
    break
}
*r
```

期待 `E1013`。

拒否:

```text
let mut xs = ["a", "b"]
for s in xs do {
    xs = ["c"]
    continue
}
```

期待 `E1014`（列挙元 borrow 中の置換）。

### E2E 期待値

`tests/fixtures/control` に追加:

- `break_while(limit)`:
  - `limit = 0` -> 0
  - `limit = 100` -> 42
- `continue_sum(n)`:
  - JS 参照: `sum i in [0,n) where i % 3 != 0`
- `break_range_endpoints()`:
  - `for i = 2147483646 to 2147483647` で break/continue を組み合わせ、端点処理が折り返さないこと。
- `continue_i64_endpoint()` / `continue_i128_endpoint()`:
  - `for i in (9223372036854775806 .. 1 .. 9223372036854775807)` と `for i in (170141183460469231731687303715884105726i128 .. 1i128 .. 170141183460469231731687303715884105727i128)` の各反復で `continue` を実行し、endpoint iteration も 1 回だけ実行されて終了することを checksum で確認する。
  - `downto`/minus-one でも `i64::MIN` / `i128::MIN` endpoint の iteration が実行され、折り返さないことを確認する。
- `break_owned(count)`:
  - 反復ごとに string/array を作り、break 後 `live == 0`。
- `continue_owned(count)`:
  - 反復ごとに string/array を作り、continue 後 `live == 0`。
- traps:
  - break でスキップされる `1 / 0` はトラップしない。
  - break しない経路では従来どおりトラップする。

`tests/control.mjs`:

- native `-O0`/`-O3`。
- WASM `-O0`/`-O3`。
- `WebAssembly.Module.imports(module) == []`。
- heap tracking `live == 0`。
- IR 2 回生成で一致。

## ドキュメント

- `docs/language.md`:
  - 「for / while」の `break／continue はなく` を削除し、新仕様を追加。
  - builder `for`/`while` では使えないことを「コンピュテーション式」に追記。
  - 診断表に `E1023` の説明を追加。
- `docs/architecture.md`:
  - 制御構文の不変条件に break/continue edge と ownership fixed point を追加。
  - LLVM lowering の loop target stack と drop-before-jump を説明。
  - 検証コマンドは `cargo build --release --locked && node tests/control.mjs target/release/tsuzuri` と明記する。
- `README.md`:
  - 制御構文の例に `break`/`continue` を追加。

## 受け入れ条件

- [ ] `break`/`continue` が通常 `while`, `for...in`, `for...to`, `downto` の最内 loop を対象にする。
- [ ] ループ外、lambda/task/computation 境界、builder loop で `E1023`。
- [ ] phase 1 の型規則として unit 型に制限し、`if c then break else 1` は拒否される。
- [ ] break/continue edge を含む所有権固定点が move/loan を正しく検査する。
- [ ] ジャンプ前に body locals、pattern temporaries、owned temporaries が drop され、heap tracking `live == 0`。
- [ ] `for_each` の temporary source owner が break exit でも解放される。
- [ ] `range_loop`/`narrow_range_loop` の endpoint/overflow semantics が変わらない。
- [ ] tail recursion lowering と通常 loop continue target が混同されない。
- [ ] native/WASM × `-O0`/`-O3`、WASM import なし、IR 決定性を確認した。

## 落とし穴

- `continue` の target は body 先頭ではなく、loop 種類ごとの advance/test。range の endpoint check を飛ばさない。
- `break` 後に emitter が命令を続けると invalid IR になるため、必ず dead block を開始する。
- `drop_all` を使うと外側 loop や関数スコープまで落としすぎる。loop entry 以降の scopes だけ drop する。
- `for_each` の element binding は借用 alias。drop してはいけないが、loan は反復末で終える。
- builder の `For`/`While` は通常 loop ではない。ここへ `break` を許すと関数境界を越える。
- `if c then break else 1` を受理したくなっても、このチケットでは bottom 型を導入しない。

## 対象外

- label 付き break/continue。
- 値付き break（`break value`）や loop expression の値返却。
- `try`/`finally` のような unwind。
- コンピュテーション式 builder loop からの break/continue。
- `task` のキャンセルや cooperative stop。B06 の担当。
- bottom/Never 型。

## 未決事項

- **既定案:** phase 1 は unit 型に制限する。より表現力の高い bottom 型は、`TypedExprKind::If` の phi incoming、所有権の到達不能経路、LLVM dead block 管理をまとめて設計する phase 2 に回す。
- **phase 2 候補:** label 構文は `break label` ではなく、将来のブロック label 設計と合わせて決める。現時点では予約しない。
