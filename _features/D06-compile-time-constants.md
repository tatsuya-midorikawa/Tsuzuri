# D06: コンパイル時定数（const）
| 項目 | 内容 |
|---|---|
| ID | D06 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | – |
| 後続 | G06, E05, F04 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_control.rs`, `src/numeric.rs`, `tests/constants.rs`, `tests/constants.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

トップレベルの名前付き定数を導入し、配列サイズ相当のテーブル、数値定数、文字列、構造値を宣言順に依存しない形で共有できるようにする。
`const` は実行時の共有可変状態ではなく、コンパイル時に評価される不変値である。
評価に失敗する式は `E1026` でコンパイルエラーにする。
スカラー定数は利用箇所へ inlining し、aggregate は private read-only global template から通常の所有値へ materialize する。
所有権モデルを壊さず、const array/string を drop して静的領域を free する事故を防ぐ。
生成 IR は決定的にする。

## 現状

`src/syntax.rs` の `Program` は record/function/class/instance/active pattern/entry を持つが、const declaration はない。
`src/lexer.rs` の `Lexer::identifier` は予約語を判定する。
GUIDE D-15 で `const` は新予約語として割り当て済み。
キーワード追加時はモジュール名・識別子として使えなくなる。

`src/parser.rs` の `Parser::program` はトップレベル宣言を先頭 keyword で振り分け、最後の `else` 的な経路を `Main.tz` entry code として解釈する。
新しいトップレベル宣言は `Program` と parser loop に追加が必要。
`.tz`/`.tt`/`.tc` の許可種別は `computation.rs` の collect 周辺と docs の表に従う。

`src/check.rs` の `Names` は modules/builders/records/functions/active_patterns を持つ。
`check_modules` は全モジュールの名前を先に収集し、関数 signature を集めてから body を検査する。
定数名の namespace はまだない。
ローカル `let` は `Checker::bind` で `Local` になり、`TypedExprKind::Local` として使われる。

`src/llvm.rs` は string literal を `Globals.definitions` に private constant として出す。
`llvm_frame.rs` は `new` なしの literal を stack frame または static string から扱い、drop/free を frame 判定で回避する。
配列・リスト・string は所有値なので、静的 global をそのまま通常 owned value として渡すと drop 時に static pointer を free してしまう。

`src/numeric.rs` は literal の binary rounding に `rustc_apfloat` を使う。
decimal literal は独自に parse/round する。
runtime arithmetic の正確な decimal/f16/f128 は `numeric.c` 側にあり、Rust const evaluator から直接再利用できない。

## 仕様

### 前提とする他チケットのインターフェース

D06 は依存なしで実装する。
E02 が未完了でもユーザーモジュール内 const は使える。
B01/A08/C03/C02 は必須ではない。
ただし A08 完了後は char literal を const expression に追加する。
E02 完了後は std 内にも const を置ける。

### 他チケットへの提供インターフェース

G06 は test declaration の expected data に const を使える。
E05 は host ABI buffer shape の固定 table を const で表せる。
F04 は SIMD shuffle mask や lane constants を const で受け取れる。
D03/D04 は Math/Int の定数や lookup table を Rust 側生成 global として持つ場合、D06 の deterministic global naming を参照する。

### 構文

```text
TopDecl ::= ConstDecl | ExistingTopDecl
ConstDecl ::= "const" Ident ":" TypeExpr "=" ConstExpr
```

例:

```text
const Answer: i64 = 42
const Greeting: string = "hello"
const Table: [i64] = [1, 2, 3]
const Pair: i64 * i64 = (20, 22)
```

`const` は新しい予約語。
`.tz` と `.tc` で許可する。
`.tt` では拒否し、`E1018`。
`.tc` の const は同じ builder module 内の補助定数として扱う。
`Main.tz` の top-level entry code より前に const を置ける。
const は module の値 namespace に属し、関数名と重複不可。
他モジュールからは `Module.Name` で参照する。
自モジュールでは無修飾で参照できる。
関数と同じく他モジュール const の無修飾参照はしない。

### Const expression フェーズ 1

許可する式:
literal: integer, float, bool, unit, string。
A08 後は char。
tuple literal。
record literal。
array literal。
list literalはフェーズ 1 では対象外。
理由: list node の静的 template と materialization が配列より複雑であり、必要なら後続で追加する。
unary `-`, `!`, `~`。
binary arithmetic/bit/comparison/logical operators。
`if condition then a else b`。
型注釈。
`as` cast。
同じ module または修飾名の const 参照。
括弧。

許可しない式:
関数呼び出し。
class method 呼び出し。
`new`。
index。
field access（const record の field projection は phase 2）。
match/for/while。
lambda/task/computation expression。
borrow/dereference/assignment。
string concat。
array length。
range。

未許可式は構文エラーではなく const evaluation failure `E1026`。
ただし const expression 外では通常通り受理する。

### 数値意味

整数演算は runtime と同じ bit width で wrapping。
除算/剰余は runtime と同じく zero divisor と signed MIN/-1 で trap 相当になり、`E1026`。
shift count は runtime と同じ `count & (bit_width - 1)`。
比較は runtime と同じ signed/unsigned。
bool `&&`/`||` は短絡する。
if は condition だけ先に評価し、選ばれた分岐だけを評価する。

binary float f16/f32/f64/f128 は `rustc_apfloat` を使い、runtime と同じ nearest ties to even、NaN、signed zero、subnormal を扱う。
f16/f128 を f64 経由で評価してはいけない。
float divide by zero は IEEE 754 結果であり `E1026` ではない。
invalid operation は NaN。

decimal:
フェーズ 1 では decimal literal と identity cast のみ許可する。
decimal arithmetic、decimal comparison、binary/decimal cast は `E1026`。
理由: runtime の正確な decimal evaluator は C にあり、Rust 側へ同等実装を移植しないと二重丸め・意味差が出るため。
既定案として、decimal const arithmetic は後続で Rust 実装を追加する。

float to integer cast は runtime と同じ saturating/NaN to 0。
integer to float cast は対象型へ一度だけ丸める。
unsupported decimal cast は `E1026`。

### 参照と cycle

const は宣言順に依存しない。
全 const 名を先に収集する。
const A が const B を参照でき、B が後に書かれていてもよい。
cycle は `E1026`。
自己参照も `E1026`。
cycle 診断 span は cycle を閉じる参照箇所。
評価 cache を持ち、同じ const を複数回評価しない。
評価深さ上限は 128。
参照展開数上限は 1,024。
超過は `E1017` ではなく `E1026` とし、message に compiler limit を含める。

### 所有権と materialization

const は runtime の共有所有値ではない。
各利用箇所で通常の値として materialize される。
スカラー const は IR literal として inlining する。
bool/unit/integers/f32/f64 も直接。
f16/f128/decimal は integer bit pattern literal。

string const:
private read-only global bytes を指す `%tz.string` descriptor を生成する。
利用箇所が即座に消費・drop する所有 string として扱う場合、static pointer を free してはいけない。
既存 string literal と同じ frame/static tracking を使う。
関数へ値渡ししてスコープ外へ移る場合は `clone_value`/relocate で heap copy にする。

array const:
private read-only global `[N x T]` を template として出す。
const 名を値として読むたびに、通常の `[T]` owned value を materialize する。
materialization は利用場所に応じる。
ローカル束縛に直接置く場合は `llvm_frame` と同じく stack frame へ copy してよい。
戻り値・引数・capture など escape する場合は heap copy。
bare const array を共有 static descriptor として返してはいけない。
drop は通常の owned array に対して行う。

record/tuple const:
スカラーと aggregate fields からなる LLVM constant として表す。
非 Copy field を含む場合、利用箇所で通常の clone/materialization を行う。
record 内 string/array は上記の template 参照から materialize。

const への borrow:
`&CONST` は hidden temporary を現在の式/statement scope に materialize し、その temporary を借用する。
static lifetime は導入しない。
したがって借用結果を入力なしで返す関数は既存 lifetime rule により拒否される。
例 `def f :: &string; fn f = &Greeting` は `E1013`。
`length (&Greeting)` は受理し、呼び出し後 hidden temporary を drop する。

### 診断

`E1026` を使う。
message は英語、小文字始まり、修正方法を含める。
例:
`"const evaluation trapped on division by zero; use a nonzero divisor or move the computation to runtime"`。
`"calls are not allowed in phase-1 const expressions; precompute the value or use a runtime let"`。
`"cyclic const definition; break the cycle or make one value a function"`。
`"decimal const arithmetic is not implemented; keep decimal arithmetic in runtime code"`。

型不一致は既存 `E1003`。
unknown const/function name は既存 `E1002`。
duplicate const/function name は `E1001`。
source kind violation は `E1018`。
const evaluation failure だけ `E1026`。

## 設計

### syntax.rs

追加:

```rust
pub struct ConstDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub value: Expr,
}

pub struct Program {
    pub constants: Vec<ConstDecl>,
    // existing fields...
}
```

`TokenKind` に `Const` を追加するか、既存 keyword token 方式に合わせる。
`MAX_NESTING` は const value expression にも適用する。
AST depth 計算漏れに注意する。

### lexer/parser

`Lexer::identifier` に `const` を追加する。
`Parser::program` の top-level loop で `const` を `ConstDecl` として読む。
`const NAME: Type = expr`。
改行/semicolon の扱いは `def`/`record` と同じ top-level declaration。
entry code と曖昧にならないよう、`const` keyword で必ず宣言として扱う。
`.tt` で見つけた場合は parser ではなく check/computation source-kind validation で `E1018`。

### check.rs

`Names` に `constants: BTreeMap<String, usize>` を追加する。
`CheckedModule` に `constants: Vec<CheckedConst>` を追加する。

```rust
pub struct CheckedConst {
    pub module: String,
    pub name: String,
    pub ty: Type,
    pub value: ConstValue,
    pub span: Span,
}

pub enum ConstValue {
    Int(u128, Type),
    Float(String, Type),
    Bool(bool),
    Unit,
    String(String),
    Tuple(Vec<ConstValue>),
    Record(usize, Vec<(usize, ConstValue)>),
    Array(Type, Vec<ConstValue>),
}
```

`ConstValue` は型を重複保持しすぎない形にしてよいが、LLVM materialization で型が分かること。
float `String` は既存 `TypedExprKind::Float` と同じ bit literal 表現を使う。

`check_modules` は record/function と同じ最初の pass で const 名を収集する。
関数名と const 名は同じ value namespace なので重複は `E1001`。
`Checker::name` は local → same module function → same module const → builtin の順にする。
他モジュール const は `ExprKind::Field(Name(module), field)` で関数と同じように解決する。
関数と const が同じ `Module.name` にあれば収集時に重複で拒否される。

typed IR に const 参照を足す。

```rust
TypedExprKind::Const(usize)
```

または `ConstValue` を expression に展開して既存 literal kind にする。
推奨は `Const(usize)` を保持し、LLVM で materialization context に応じて最適化する。
新 variant を追加する場合、GUIDE 6.2 の全走査を更新する。
`children`/`children_mut` は子なし。
polymorph/closures/recursion/ownership/call_specialization は const を leaf として扱う。

### const evaluator

型検査後に const expression を評価する。
ただし関数 body の型検査前に const の型と値が必要なので、const collection → type resolve → const eval → function signatures/body の順にする。
const expression は通常の `Checker::expression` を使わず、専用 `ConstEvaluator` で許可 subset だけを評価する。
理由: 通常 checker は function call や borrow 等を許してから後段で扱うため、const subset を明確に拒否しにくい。

`ConstEvaluator` は `Inference` を使わず、宣言された `: Type` を expected type として top-down に評価する。
array empty は annotation から element type を得る。
record literal は既存 record field resolution を再利用する。
numeric literal は `numeric.rs` の literal conversion を再利用する。
binary float arithmetic は `rustc_apfloat` で型ごとに行う。
integer arithmetic は `u128` と bits/signed で wrap/trap 判定を実装する。
bool short-circuit と if は選択分岐だけ評価する。

### llvm.rs

`CheckedModule.constants` を受け取り、必要な global definitions を `Globals.definitions` へ決定的に追加する。
global name は `@tz.const.<Module>.<Name>`。
同名型違いはない。
string bytes は `@tz.const.<Module>.<Name>.bytes`。
array template は `@tz.const.<Module>.<Name>.array`。
record/tuple nested template は suffix `.field0` 等を決定的に付ける。

`FunctionEmitter::expression_mode` で `TypedExprKind::Const(id)` を materialize する。
scalar は literal string を返す。
string は string literal/frame と同じ frames を返せるよう `frame_value` と連携する。
array は `frame_value` に const array template を追加するか、専用 copy loop を出す。
まず安全な実装として、const array の bare use は heap copy を作る。
その後、`let local = Table` のような局所束縛では `llvm_frame` と同じ stack allocation へ最適化してよい。
最適化は観測可能な所有権を変えない。

`llvm_frame.rs` は const string/array template を frame candidate として扱えるよう拡張する。
難しい場合はフェーズ 1 では heap materialization だけにし、acceptance では「static pointer を free しない」ことを優先する。

### ownership

`TypedExprKind::Const` は読み出すたびに新しい値を作る rvalue。
非 Copy const を複数回使っても move エラーにはならない。
例:

```text
const S: string = "x"
fn f = S + S
```

これは `S` を 2 回 materialize するので受理。
ただし各 materialized value は通常の所有値。
`let s = S; s + s` は `s` を 2 回使うため既存 move/copy 規則でエラー。
ownership checker は `Const` 自体を local owner として追跡しない。

## 実装手順

1. `const` keyword を lexer/parser/syntax に追加する。
   確認: `const` を変数名やモジュール名にすると既存規則で拒否される。
2. `Program.constants` と parser を実装する。
   確認: `const Answer: i64 = 42` が AST に入る。
3. source kind validation を追加する。
   確認: `.tt` の const が `E1018`。
4. `Names.constants` と duplicate collection を追加する。
   確認: const/function/const の重複が `E1001`。
5. `ConstEvaluator` の literal/scalar arithmetic を実装する。
   確認: integer wrap、division trap `E1026`、bool short-circuit。
6. binary float const evaluator を `rustc_apfloat` で実装する。
   確認: f16/f128 の midpoint と signed zero。
7. arrays/records/tuples/string const を実装する。
   確認: nested aggregate の型と layout validation。
8. const reference と cycle detection を実装する。
   確認: forward reference は通り、cycle は `E1026`。
9. `TypedExprKind::Const` と name resolution を追加する。
   確認: 関数 body から local/function/const/builtin の優先順位が仕様どおり。
10. LLVM scalar/string/array materialization を実装する。
    確認: static pointer が `@tz.free` されず、escape 時は heap copy。
11. ownership/closures/specialization の走査漏れを更新する。
    確認: 非 Copy const 複数回利用と local move の差。
12. tests/docs を追加する。
    確認: native/WASM `-O0`/`-O3`、IR determinism。

## テスト計画

### Rust tests

`tests/constants.rs`。
受理:
`const Answer: i64 = 40 + 2; fn f = Answer`。
forward reference。
module qualified const。
integer wrap: `const X: i8 = 127 + 1` evaluates to -128 bit pattern。
shift mask。
bool short-circuit: `const X: bool = true || (1 / 0 == 0)` succeeds。
if selected branch only。
f16/f32/f64/f128 literals and arithmetic。
tuple/record/array/string const。
non Copy const used twice directly。
borrow hidden temporary: `length (&Greeting)`。

拒否:
`const X: i64 = 1 / 0` is `E1026`。
`const X: i64 = X` is `E1026`。
`const A: i64 = B; const B: i64 = A` is `E1026`。
`const X: i64 = f 1` is `E1026`。
`const X: d128 = 0.1d128 + 0.2d128` is `E1026` in phase 1。
`.tt` const is `E1018`。
duplicate function/const is `E1001`。
type mismatch is `E1003`。
`def f :: &string; fn f = &Greeting` is `E1013`。

IR:
scalar const is inlined。
array/string const emits private constants with deterministic names。
IR emit twice is equal。
const array by value does not return static descriptor directly。
No `@tz.free` on private const global pointer。

### Node E2E

`tests/constants.mjs`。
native/WASM `-O0`/`-O3`。
WASM imports 空。
heap tracking live == 0。
exported functions:
return scalar const。
sum const array。
return const string length/checksum。
use const record/tuple。
use const string twice。
use const array in tail recursion 100,000 times to ensure no stack growth/leak。
borrow const in function call。

Trap/diagnostic cases are Rust tests where possible。
Runtime should not trap for accepted const programs except normal runtime operations unrelated to const。

Reference:
JS BigInt for integer wrap。
Python `fractions`/`rustc_apfloat` equivalent expected bit strings for binary floats if exported through checksum。

## ドキュメント

`docs/language.md` に `const NAME: Type = expr` を追加する。
phase 1 const expression subset と decimal arithmetic deferred を明記する。
診断表に `E1026` を追加する。
予約語一覧に `const` を追加する。
`docs/architecture.md` に const eval の順序、cycle detection、global materialization、所有権 interaction を追加する。
`README.md` に小例を追加する。
`docs/benchmarks.md` は不要。

## 受け入れ条件

- [ ] `const` が予約語になり、トップレベル宣言として parse される。
- [ ] `.tz`/`.tc` で許可、`.tt` で `E1018`。
- [ ] const 名は関数名と同じ value namespace で重複拒否。
- [ ] phase 1 subset 以外は `E1026`。
- [ ] integer/binary float const arithmetic が runtime semantics と一致。
- [ ] decimal arithmetic は `E1026` で拒否し、誤った近似をしない。
- [ ] forward reference と cycle detection がある。
- [ ] aggregate const は所有権を壊さず materialize される。
- [ ] native/WASM `-O0`/`-O3` で同じ結果。
- [ ] IR が決定的。
- [ ] docs に subset と制限が明記される。

## 落とし穴

const を単なる textual substitution にすると span/diagnostic と cycle detection が壊れる。
static array descriptor を owned value として返すと drop が static memory を free する。
decimal arithmetic を f64 で代用してはいけない。
bool short-circuit と if selected branch を eager eval すると不要な `E1026` が出る。
local/function/const/builtin の名前優先順位を変えない。
new `TypedExprKind` の走査漏れはコンパイルエラーにならない箇所がある。
const array の非 Copy 利用は「読むたび materialize」であり、const 自体を move 済みにしない。

## 対象外

関数呼び出し可能な const eval。
ユーザー関数の inlining。
fuel 付き CTFE。
decimal arithmetic。
list const。
map/set const。
const generic。
pattern matching in const。
compile-time reflection。
static mutable storage。

## 未決事項

decimal const arithmetic をいつ追加するか。
既定案は runtime `numeric.c` と同等の Rust evaluator を別チケットで追加する。
list const を phase 1 に含めるか。
既定案は含めない。
aggregate const の初期実装を heap materialization のみにするか。
既定案は安全優先で heap materialization、後で `llvm_frame` による stack/static template 最適化。
const field projection を許すか。
既定案は phase 2。
ユーザー関数呼び出しを CTFE に入れるか。
既定案は phase 2 で fuel/recursion limit と純粋性を設計してから。

台帳の見直し提案: なし。
