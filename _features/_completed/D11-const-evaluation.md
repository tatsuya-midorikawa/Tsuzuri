# D11: コンパイル時評価の拡張（const 関数・表の生成）

| 項目 | 内容 |
| --- | --- |
| ID | D11 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | D06, (A16) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D10（Phase 2 の static データ。GUIDE D-28 の「static lifetime を導入しない」の変更で、D-30 が承認待ちとしている） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: C++ の `constexpr`、Rust の `const fn` に比べてコンパイル時評価の範囲が狭い |
| 手本にする既存実装 | 依存の明示スタック評価と上限: `src/constants.rs` の `Evaluator::run`・`charge`・`limit`。数値意味: 同ファイルの `unary`・`binary`・`cast`・`float_binary`（そのまま再利用する）。定数を内部関数として集める経路: `src/check.rs` の `constants::validate` を呼ぶループと `Names::constants`。修飾子を実装側へ継承する構文: `src/parser.rs` の `visibility` と `private def` の扱い。テスト: `tests/constants.rs` の `evaluates_integer_constants_and_inlines_forward_references`（値の取り出しと IR の決定性）、`tests/features.mjs` の suite `constants` |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/check.rs`, `src/constants.rs`, `tests/constants.rs`, `tests/fixtures/const_functions/Main.tz`（新規）, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/values-and-constants.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

lookup table・CRC 表・係数表などを、実行時の初期化コストなしに関数で生成できるようにする。
C++ の `constexpr`、Rust の `const fn` と同じ用途を、実行時と同じ数値・トラップの意味で提供する。
`const def` で宣言した関数を `const` の初期化式から呼び、結果を D06 と同じ型付きリテラルとして展開する。

実装者は Phase 1 だけを実装する。Phase 2（static データ）は D10 の承認後、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D06 が `_features/README.md` の状態欄で done であること（HEAD で done）。A16 は括弧付き（任意）で開始条件にしない。
  確認: `grep -nE "^\| (D06|D11|A16) " _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR と stack-depth テスト）を保存していること。
- Phase 2 は D10 の承認が記録されるまで着手しない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 評価器が Rust の再帰で呼び出しやループを処理しないと書けない。または stack-depth の 3 テスト（手順 1）が失敗する。
- `MAX_NESTING`（128）・`remaining` の初期値（`1 << 20`）・探索 1024 件のどれかを上げたくなった。
- 既存の `tests/constants.rs` の期待値、既存 suite `constants` の値、`const def` を書かないプログラムの IR を変える必要がある。
- `unary`・`binary`・`cast`・`float_binary` の結果を変えないと実行時と一致しない（数値意味の不一致は実行時側を正とし、報告する）。
- `const def` の本体で、同じ関数の中の `Local::id` が lambda の本体と外側で衝突する（D3 の環境が id で引けない）。
- `ref` を書かない `const def` の型付き本体で、`Borrow`・`BorrowOperand`・`Dereference` が D2 の「場所の読み出し」以外の位置に現れる。
- 手順 1 で、実行時が trap しないのに本チケットが trap として扱う操作（負の長さの `new [T](n, f)`、範囲の step 0、範囲外の索引）が見つかった。
- `constants::fold` を型検査の前へ動かす必要が出た（A16 Phase 2 の const ジェネリクスの要求はこのチケットでは扱わない）。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 構文: `src/parser.rs` の最上位の分岐は `self.eat(&TokenKind::Const)` の後に `const_declaration` を呼び、名前・`:`・型・`=`・本体を読む。
  `const def` は名前の位置で `E0002`（下の再現）。`const` と `def` は `src/lexer.rs` の既存予約語で、予約語の追加は要らない。
- 収集: `src/check.rs` は各 `ConstDecl` を `constants::validate` で AST 検査し、引数なしの `FunctionDecl` として `function_declarations` に
  積み、その id を `Names::constants` に入れる。型変数を含む型は `constants::failure`（`E1026`）で拒否する。
- `constants::validate`（AST）は `Integer`・`Float`・文字列・文字・`Bool`・`Unit`・`Name`・単項・`Cast`・`Field`・`Pipe` 以外の二項・`If`・
  配列・タプル・レコード・束縛なしの `Block` だけを許す。関数呼び出しは `E1026`
  「this expression is not allowed in a phase-1 const; precompute it or use a runtime let」。
- 評価: 型検査の最後で `constants::fold(&mut functions, &names.constants)` を呼ぶ（所有権検査より前）。`fold` は `Evaluator` を作り、
  `Evaluator::run` が依存を明示スタック `pending` で辿る（一つの根あたり `references` 1024 件、深さ `MAX_NESTING`）。
  値の計算は `Evaluator::evaluate` で、型付き IR を Rust の再帰で解釈する（`depth` 引数で 128 に制限）。対応は
  `Int`・`Float`・`String`・`Bool`・`Unit`・`Array`・`Tuple`・`Record`・`Unary`・`Binary`（`&&`・`||` は短絡）・`If`・束縛なしの `Block`・`Cast`
  と定数参照。それ以外は「this operation is not allowed in a phase-1 const; use a runtime let」。
- 予算: `Evaluator::remaining` は `1 << 20` で、プログラム全体の全定数で共有する。評価 1 ノードと、`charge` による値の複製 1 ノードごとに 1 減る。
  0 になると `limit`（「const evaluation exceeds the compiler limit; split or simplify the constants」）。`inline` は関数ごとに新しい
  `1 << 20` を持ち、定数参照（`constant_id` が返す引数なしの `Call`）を評価済みの値で置き換える。
- 数値: 整数は `mask`・`signed` で幅ごとに折り返し、シフト量を `bits - 1` でマスクする。整数の `/`・`%` のゼロ除算と `MIN / -1` は
  「const division trapped on zero or signed overflow; use a nonzero divisor without overflow」。binary 浮動小数点は `rustc_apfloat` の
  `Half`・`Single`・`Double`・`Quad` で最近接偶数丸め（f16・f128 も対応済み）。decimal はリテラル・符号・恒等 cast だけで、演算は
  「this const arithmetic is unsupported; keep decimal arithmetic and overloaded operations in runtime code」。
- 展開: 値は型付きリテラルの木（`TypedExpr`）で、利用ごとに通常の所有値として生成する。LLVM 側は既存のリテラル・frame・relocate を使い、
  専用の global は作らない（GUIDE D-28）。static lifetime はない（`docs/language.md` の「コンパイル時定数」）。
- テスト: `tests/constants.rs` に 8 件（`parses_typed_constants_before_functions_and_entry`、`constants_require_types_and_reserve_the_keyword`、
  `constants_follow_source_kind_and_formatting_rules`、`evaluates_integer_constants_and_inlines_forward_references`、
  `validates_unused_constants_cycles_and_value_names`、`evaluates_binary_float_rounding_special_values_and_casts`、
  `temporary_constant_borrows_do_not_escape`、`bounds_constant_expansion`）。E2E は `tests/features.mjs` の suite `constants`（8 cases、
  fixture `tests/fixtures/constants/Main.tz`）。
- 旧版の記述「f16/f128 は Phase 1 でも拒否」は HEAD と合わない。f16・f128 は D06 で評価済みで、本チケットもそのまま使う。

### 再現（検証済み）

`/tmp/tz-d11/constdef/Main.tz`:

```tsuzuri
const def twice :: i64 -> i64
fn twice x = x * 2
twice 21
```

`/tmp/tz-d11/call/Main.tz`:

```tsuzuri
def twice :: i64 -> i64
fn twice x = x * 2
const Answer: i64 = twice 21
Answer
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri check /tmp/tz-d11/constdef   # 1:7 error[E0002]: expected an identifier
target/release/tsuzuri check /tmp/tz-d11/call       # 3:21 error[E1026]: this expression is not allowed in a phase-1 const; ...
```

実行時の CRC32 表は現在の構文で書ける。次は `run` が終了コード 0 で終わる（`0xCBF43926` は CRC-32 の標準検査値）。
E2E fixture の実行時側（手順 9）はこの関数を使う。

```tsuzuri
def crc_entry :: i32u -> i32u
fn crc_entry index =
    let mut value = index
    for _ in 1 .. 8 do
        value = if (value & 1) == 1 then (value >>> 1) ^ 0xEDB88320 else value >>> 1
    value

def crc_table :: [i32u]
fn crc_table = new [i32u](256, \i -> crc_entry (i as i32u))

def crc_of :: [i32u] -> i32u
fn crc_of table =
    let text = "123456789"
    let mut crc = ~(0 as i32u)
    for unit in text do
        let byte = (unit as i32u) & 0xFF
        crc = table[((crc ^ byte) & 0xFF) as i64] ^ (crc >>> 8)
    ~crc

assert (crc_of (crc_table()) == 0xCBF43926)
```

同様に `def rec fib :: i64 -> i64` と `fn rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)`、整数の `match n with | 0 -> 10 | 1 -> 20 | _ -> n * 2`
も検証済み（`fn` 側にも `rec` が要る。ないと `E1019`）。

## 仕様

### 前提とする他チケットのインターフェース

- D06（done）: `const` 宣言、`Names::constants`、型検査の最後（所有権検査の前）で呼ぶ `constants::fold`、型付きリテラルへの展開と
  GUIDE D-28 の規則（利用ごとに所有値、static lifetime なし、直接借用は単一段階の完全適用だけ）。本チケットはこの位置と規則を変えない。
- A16（任意）: 依存しない。A16 Phase 2（A16 の D10、const ジェネリクス）が型の長さに `const def` を使うには評価を型検査より前へ
  動かす必要があり、本チケットは提供しない（停止条件）。提供するのは「型検査後に値として評価する `const def`」だけ。

### 構文

新構文（実装後に有効。未検証）。`const` は `def` の署名だけに付け、実装の `fn`・`and`・`let` は署名から継承する（`private` と同じ）。

```text
const_function = ["private"] "const" "def" ["rec"] name "::" signature ["=" body]
               | "export" "const" "def" ["rec"] name "::" signature ["=" body]
continuation   = "and" name "::" signature ...        (const def rec のグループの and は const を継承する)
implementation = "fn" ["rec"] name parameters "=" body   (従来どおり。const は書かない)
```

- `const` の直後が `def` のときだけ関数として読む。それ以外は従来の `const Name: Type = value`。
- `const fn`・`const let`・`const and` は `E0002`、`const export def` は `E0002`（診断の表）。`private export const def` は既存の `E1022`。
- ソース種別の規則は `def` と同じ（新しい規則を足さない）。

### 型規則

- `const def` は通常の関数として型検査し、実行時にも通常どおり呼べる。IR は `const` を付けない場合と同一にする（D7）。
- 追加の検査（すべて `E1026`、型検査の後）:
  - 型変数と制約を持たない（具体的な署名）。
  - 引数と結果の型は「定数値型」だけ: 整数（`Type::Integer`）、binary 浮動小数点（`Type::Binary`）、decimal（`Type::Decimal`。演算は不可）、
    `Type::Char`・`Type::Utf8Char`、bool、unit、string・utf8string、配列 `[T]`、タプル、レコード、union（型引数も定数値型）。
    関数型・参照（`ref`・`&`）・Task・リスト `[|T|]`・std の opaque record（`Vec`・`Map` など）は不可。
  - 実装が署名より少ない引数しか名前を付けない（`Parser::define` が残りを関数型の結果にする）場合は、結果が関数型なので上の規則で不可。
  - 本体は D2 の部分集合だけ。未使用の `const def` も検査する（未使用の `const` を評価する D06 と同じ方針）。
- `const` の初期化式（AST の `constants::validate`）は、呼び先が `ExprKind::Name` で最後の名前がどれかのモジュールの `const def` 名と一致する
  `ExprKind::Call` だけを許す。それ以外の呼び出しは従来メッセージの `E1026` のまま（`identity 1` の既存期待値を保つ）。
  呼び先の解決は型検査後に確かめ、`const def` でなければ `E1026`。

### 評価順序・所有権・借用

- 順序は実行時と同じ: 引数は左から右に評価してから呼ぶ。`&&`・`||` は短絡、`if` は選んだ枝だけ。`while` は各反復の前に条件。
  範囲は start → step → finish を一度ずつ。配列の `for` は索引の昇順。`match` は arm の順、各 arm の alternative の順、束縛の後で guard。
  `break`・`continue` は最内のループ。レコード・配列・タプル・`Construct` の要素は型付き IR の並び順。
- 代入 `a[i] = v`・`r.f = v` は、場所の索引式を左から右、次に右辺、最後に境界検査の順（`src/llvm.rs` の `Assign` の lowering と
  違えば実行時側に合わせる）。評価に副作用はなく、順序が見えるのは二つ以上の trap がある場合だけである。
- 所有権・借用: `const def` の本体は通常の関数として所有権検査を受ける。評価器は値を複製して扱い、move 済みの値を読むことはない
  （所有権検査が保証する）。借用は D2 の場所の読み出しの基部だけで、値としての借用・可変借用は `E1026`。
- 結果は D06 と同じく各利用で型付きリテラルとして生成する（D6）。

### 数値・トラップ・native と WASM の差

- 整数・浮動小数点・cast は `unary`・`binary`・`cast` をそのまま呼ぶ。したがって D06 の const と実行時の意味（折り返し、シフト量のマスク、
  APFloat の最近接偶数丸め、NaN・符号付きゼロ・subnormal、float から整数への飽和）に一致する。f16・f128 も対象。
  decimal はリテラル・符号・恒等 cast だけで、演算は既存メッセージの `E1026`。
- 実行時に trap する操作は、trap した式の span で `E1026` にする。メッセージに trap の種類を入れる（診断の表）。
- native と WASM: 値は target に依存しない型付きリテラルで、両 target に同じ値を出す。`.length` の型は実行時と同じ（型検査が決める）。
- 決定性: `BTreeMap`・`BTreeSet` だけを使い、`HashMap`・時刻・乱数を使わない。同じソースは同じ結果・同じ最初の診断になる。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E0002` | `const fn`・`const let`・`const and` | put 'const' on the 'def' signature; its 'fn', 'and', or 'let' implementation inherits it | `const` の token |
| `E0002` | `const export def` | write 'export const def' to export a const function | `const` から `export` まで |
| `E1026` | `const def` の型変数・制約 | a const def must have a concrete signature; remove type variables and constraints | 関数の名前 |
| `E1026` | 引数・結果が定数値型でない | const def parameters and results must be plain values; remove functions, references, tasks, and lists | 関数の名前 |
| `E1026` | 本体に D2 以外の操作 | this operation is not allowed in a const def; move it to a runtime def | その式 |
| `E1026` | `const def` でない関数・builtin・メソッドの呼び出し | '{name}' is not a const def; mark it 'const def' or compute the value at runtime | 呼び出し |
| `E1026` | `const def` の部分適用・関数値としての使用 | a const def must be fully applied in const evaluation; pass all arguments | その式 |
| `E1026` | 整数の `/`・`%` の trap | const division trapped on zero or signed overflow; use a nonzero divisor without overflow（既存） | 演算 |
| `E1026` | 範囲外・負の索引 | const evaluation trapped: index out of bounds; check the index against the length | 索引式 |
| `E1026` | 範囲の step が 0 | const evaluation trapped: range step is zero; use a nonzero step | `for` |
| `E1026` | `new [T](n, f)` の n が負 | const evaluation trapped: negative array length; use a length of at least 0 | `new` 式 |
| `E1026` | どの arm にも一致しない（for の分解） | const evaluation trapped: no pattern matched; handle every value or compute it at runtime | `match` |
| `E1026` | 呼び出しの深さ超過 | const evaluation exceeds the compiler limit of 128 nested const def calls; reduce the recursion depth or compute the value at runtime | 129 段目の呼び出し |
| `E1026` | 予算・値の入れ子・配列長の超過 | const evaluation exceeds the compiler limit; split or simplify the constants（既存の `limit`） | 超過した式 |
| `E1026` | `const def` を経由した循環 | cyclic const definition; break the cycle or make one value a function（既存） | 参照 |

### 資源上限

- 予算: 既存の `Evaluator::remaining`（`1 << 20` = 1,048,576、プログラム全体で一つ）を共有する。評価する式 1 個（`Task::Eval` 1 回）ごとに 1、
  値全体の複製（`Local` の読み出し、定数参照、`for` の列挙元）ごとにその値の `Value::size` を引く。展開の `charge`・`inline` は従来どおり。
- 呼び出しの深さ: 128（`MAX_NESTING`）。根の初期化式は数えず、`const def` の呼び出しで積んだフレーム数が 128 を超えたら `E1026`。
- 値の入れ子: 128 以下。集成体を作るときに `Value::depth` で検査する（Rust の `Clone`・`Drop`・`charge` の再帰を 128 段に保つ）。
- 配列長: `new [T](n, f)` は n が `remaining` を超えたら確保前に `limit`。
- 依存探索: 深さ 128・一つの根あたり 1024 件のまま。`const def` の本体から見つけた定数参照も `references` に数える。
- メモリ: 生きている値のノード数は引いた予算以下なので、上限は 1,048,576 × `size_of::<TypedExpr>()` 程度。旧版の「ステップ 16,777,216・
  確保 64 MiB」は既存上限との一貫性のため採らない（D4）。

### 例

新構文（実装後に有効。未検証）。受理する例（値は独立に計算済み: CRC-32 表の 1 番目は `0x77073096`、255 番目は `0x2D02EF8D`、fib 20 は 6765）。

```tsuzuri
const def crc_entry :: i32u -> i32u
fn crc_entry index =
    let mut value = index
    for _ in 1 .. 8 do
        value = if (value & 1) == 1 then (value >>> 1) ^ 0xEDB88320 else value >>> 1
    value

const def crc_table :: [i32u]
fn crc_table = new [i32u](256, \i -> crc_entry (i as i32u))

const CrcTable: [i32u] = crc_table()

const def rec fib :: i64 -> i64
fn rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)

const Fib20: i64 = fib 20
```

拒否する例（各行は独立したプログラムの要点）。

| ソースの要点 | 結果 |
| --- | --- |
| `def plain :: i64 -> i64` と `const def uses :: i64 -> i64`、`fn uses x = plain x` | `E1026`（'plain' is not a const def） |
| `const def total :: [i64] -> i64`、`fn total xs = Array.sum xs` | `E1026`（builtin の呼び出し） |
| `const def pick :: [i64] -> i64`、`fn pick xs = xs[3]`、`const P: i64 = pick [1, 2]` | `E1026`（index out of bounds） |
| `const def rec down :: i64 -> i64`、`fn rec down n = if n == 0 then 0 else down (n - 1)`、`const D: i64 = down 128` | `E1026`（128 nested calls）。`down 127` は受理 |
| `const def spin :: i64 -> i64`、`while` が終わらない本体 | `E1026`（compiler limit） |
| `const def id :: 'a -> 'a` | `E1026`（concrete signature） |
| `const fn f x = x` | `E0002` |

### Phase 2（設計方針、要承認）

大きな定数配列を読み取り専用の静的データとして一度だけ出力し、`ref` で借用して使えるようにする。static な region と、drop しない借用元が要り、
GUIDE D-28 の「static lifetime を導入しない」を変える（D10）。承認前は設計も実装もしない。Phase 1 の値の表現（`Value`）と展開は
Phase 2 でも入力として使える形に保つ。

## 設計

### データ構造

```rust
// src/syntax.rs
pub struct SignatureDecl { /* 既存 */ pub constant: bool }   // （新規 field）
pub struct FunctionDecl { /* 既存 */ pub constant: bool }    // （新規 field）

// src/check.rs
struct Names { /* 既存 */ const_functions: BTreeSet<usize> } // （新規 field）

// src/constants.rs（すべて新規）
struct Value {
    expr: TypedExpr, // リテラルの木: Int・Float・String・Bool・Unit・Array・Tuple・Record・Construct だけ
    depth: usize,    // 入れ子の上界（葉は 1）
    size: usize,     // ノード数（葉は 1）
}

struct Frame {
    locals: BTreeMap<usize, Value>, // Local::id → 値
    stack: usize,                   // 呼び出し時の値スタックの高さ
}

enum Task<'a> {
    Eval(&'a TypedExpr),
    Resume(&'a TypedExpr, u8),                // 子を評価した後の続き（u8 は段階）
    Bind(usize),                              // 値スタックの先頭を Local に束縛
    Pop,                                      // 値を捨てる（Block の文、ループ本体の unit）
    Loop(Loop<'a>),                           // 次の反復。break・continue の目印を兼ねる
    Fill { expr: &'a TypedExpr, next: u64, length: u64 }, // new [T](n, f) の要素
    Arm { expr: &'a TypedExpr, arm: usize, alternative: usize, step: usize },
    Return,
}

enum Loop<'a> {
    While { expr: &'a TypedExpr, stack: usize },
    Range { expr: &'a TypedExpr, current: u128, step: u128, finish: u128, done: bool, stack: usize },
    Each { expr: &'a TypedExpr, source: Value, index: usize, stack: usize },
}

struct Machine<'a> {
    functions: &'a [CheckedFunction],
    const_functions: &'a BTreeSet<usize>,
    constants: &'a BTreeSet<usize>,
    values: &'a BTreeMap<usize, TypedExpr>, // 評価済みの定数
    remaining: &'a mut usize,
    tasks: Vec<Task<'a>>,
    stack: Vec<Value>,
    frames: Vec<Frame>,
}
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 字句 | `src/lexer.rs` | – | 変更なし（`const`・`def` は既存の予約語） |
| 構文 | `src/syntax.rs` | `SignatureDecl`, `FunctionDecl` | `constant: bool` |
| 構文 | `src/parser.rs` | 最上位の分岐（`self.eat(&TokenKind::Const)` の直前） | `Const` の次が `Def` なら `Const` を取り、局所変数 `constant` を立てて `def` の分岐へ進む。次が `Fn`・`Let`・`And`・`Export` なら `E0002` |
| 構文 | `src/parser.rs` | `export` の分岐（`let exported = self.eat(&TokenKind::Export)`） | `export` の後の `Const` を読み、`Def` が続かなければ `E0002` |
| 構文 | `src/parser.rs` | `signature`（`SignatureDecl` を作る）、`define`（`FunctionDecl` を作る）、括弧付き `fn` の `program.functions.push(FunctionDecl {` | `signature.constant` を立て、`define` は署名から写す。括弧付き `fn` は `false` |
| 構文 | `src/parser.rs` | `signature_group` と同じ場所 | 局所変数 `constant_group`（新規）。`const def rec` で立て、`and` の署名が継承する。`signature_group = None` と同じ所で戻す |
| 整形 | `src/formatter.rs` | `format_source` | `const def` を保つこと（手順 2 のテスト）。保たなければ署名の出力に `const ` を足す |
| 文書生成 | `src/docgen.rs` | `signature`（`declaration.exported` で `"export "` を出す箇所） | `export` の後に `const ` |
| 検査 | `src/check.rs` | 定数を `FunctionDecl` にする箇所と他の `FunctionDecl {` 構築 | `constant: false` |
| 検査 | `src/polymorph.rs` | `Ok(FunctionDecl {` を返す関数 | `constant: false` |
| 検査 | `src/check.rs` | `Names`、関数 id を割り当てる箇所 | `declaration.constant` の id を `const_functions` へ |
| 検査 | `src/check.rs` | `names.constants.contains(&id) && !variables.is_empty()` の検査 | `const_functions` の型変数・制約を `E1026` |
| 検査 | `src/check.rs` | `constants::fold` の呼び出し | 直前に `constants::validate_function`（新規）を `const_functions` の各 id に呼ぶ。`fold` に `&names.const_functions` を渡す |
| 定数 | `src/constants.rs` | `validate`（引数 `const_names: &BTreeSet<String>` を追加） | callee が `Expr::Name` で最後の名前が `const_names` にある `Expr::Call` を許し、引数を検査する。`src/check.rs` は定数のループの前に全モジュールの `const def` 名を集めて渡す |
| 定数 | `src/constants.rs` | `fold` | 引数 `const_functions`。`const def` の本体は置き換えない（`inline` は従来どおり全関数） |
| 定数 | `src/constants.rs` | `Evaluator::run` | 依存の走査で `const def` の呼び出しを見たら、その本体も走査する（根ごとの訪問済み集合） |
| 定数 | `src/constants.rs` | `Evaluator::evaluate` | `Machine::run`（新規）に置き換える。`Evaluator::constant`・`unary`・`binary`・`cast` は再利用 |
| 定数 | `src/constants.rs` | `validate_function`・`value_type`・`trapped`（新規） | 署名と本体の静的検査、定数値型の判定、trap の診断 |
| 所有権・LLVM | `src/ownership.rs`, `src/llvm.rs` | – | 変更なし（`const def` は通常の関数、結果は既存のリテラル種別） |

### 生成 IR とランタイム

新しい IR の形・ランタイム関数・WASM import はない。`const def` は通常の関数として既存規則で出力され、定数の値は既存のリテラル種別として
各利用へ展開される。`const def` を書かないプログラムの IR は HEAD と byte 単位で同一にする（手順 1 と手順 11 で比較）。

### アルゴリズム

`Machine::run(root_body)` は一つの根の初期化式を評価する。Rust の再帰は `charge` と `Value` の `Clone`・`Drop`（入れ子 128 以下）だけ。

```text
frames = [Frame { locals: {}, stack: 0 }]; tasks = [Eval(root_body)]
while task = tasks.pop():
  Eval(e): remaining == 0 → limit(e.span); remaining -= 1
    リテラル → push Value 葉
    Local(id) → v = frame.locals[id]; charge v.size; push v.clone()
    定数参照（constant_id が constants にある）→ Evaluator::constant と同じく values から取り、size・depth を数えて push
    Field・Index・Length の基部が場所（Local、またはその Field・Index、Borrow(_, false)・BorrowOperand・Dereference を剥がしたもの）
      → 索引式だけを評価してから場所を辿って要素を読む（容器を複製しない）
    If → push Resume(e, 0), Eval(condition)。Resume で bool を取り、選んだ枝を Eval
    Binary And/Or → Resume(e, 0), Eval(left)。Resume で短絡を判断し、必要なら Resume(e, 1), Eval(right)
    Block → 各束縛について Bind(id)・Eval(value) を逆順に積み、最後に result
    Call(User(id), args)（const_functions、完全適用）→ Resume(e, 0) と args の Eval を右から積む
      Resume: frames.len() - 1 == 128 なら深さの E1026。args を pop し、新しい Frame に引数の Local::id で束縛、push Return, Eval(body)
    Return → 結果を pop、stack を frame.stack へ切り詰め、frame を捨てて結果を push
    While → push Loop(While { stack: 現在の高さ })
    ForRange → start・step・finish を評価後、step 0 は trap。Loop(Range { .. }) を積む
    ForEach（配列だけ）→ 列挙元を評価（size を課金）し、owner に束縛、Loop(Each { index: 0 }) を積む
    Match → value を評価し local に束縛、Arm { arm: 0, alternative: 0, step: 0 } を積む
    NewArray(n, init) → n を評価、負なら trap、n > remaining なら limit。Fill { next: 0, length: n } を積む
    Assign(place, value) → 索引式を左から右、value を評価、境界検査、場所へ書く。push Unit
    Unary・Binary・Cast → 子を評価し Resume で unary・binary・cast を呼ぶ
    Array・Tuple・Record・Construct・RecordUpdate → 子を評価し Resume で組み立て、depth = 1 + max、size = 1 + Σ。depth > 128 なら limit
  Loop(While) → push Loop(While) を戻し、Resume(expr, 0), Eval(condition)。false なら Loop を外して Unit
  Loop(Range) → done なら Unit。そうでなければ local に current を束縛し、次の値（型の範囲を越えたら done）で Loop を積み直し、Pop, Eval(body)
  Loop(Each) → index == length なら Unit。そうでなければ要素を local に束縛して同様
  Break → Loop を pop するまで tasks を捨て、stack を Loop の高さへ切り詰め、Unit
  Continue → 先頭が Loop になるまで tasks を捨て（Loop は残す）、stack を切り詰める
  Fill → next == length なら n 個を pop して Array。そうでなければ lambda の引数に next を束縛して body を Eval（const def なら Call と同じ）
  Arm → alternative の step を順に評価（Test は bool、Bind は束縛）。false なら次の alternative・arm。全 arm が不一致なら trap
result = stack.pop(); charge(&result.expr, remaining, 0)
```

整数範囲の終端と次の値は、局所の型の幅と符号で `binary` と同じ `mask`・`signed` を使って計算する（`docs/language.md` の
「`125i8 .. 2i8 .. 127i8` は 125、127」を満たす）。`validate_function` は明示スタックの前順走査で、最初の不許可の式（ソース順）を報告する。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、既存 fixture の IR を保存する。実行時の trap を確かめる 3 つの scratch project（未検証。
  この手順で確かめる）を作る: `/tmp/tz-d11/traps/index/Main.tz`（`def pick :: [i64] -> i64`、`fn pick xs = xs[3]`、`pick [1, 2]`）、
  `/tmp/tz-d11/traps/step/Main.tz`（`def sum_step :: i64 -> i64` で `for i in 0 .. step .. 3 do` を回し、`sum_step 0`）、
  `/tmp/tz-d11/traps/length/Main.tz`（`def make :: i64 -> [i64]`、`fn make n = new [i64](n, \i -> i)`、`let values = make (0 - 1)`、`values.length`）。
- 確認: 次がすべて成功し、3 つの `run` は非 0 で終わる（`E2005`）。trap しないものがあれば停止する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-d11/base
for wasm in "" "--target wasm32"; do
  target/release/tsuzuri build tests/fixtures/constants $wasm --emit llvm -o "/tmp/tz-d11/base/constants${wasm:+-wasm}.ll"
done
for t in index step length; do target/release/tsuzuri run /tmp/tz-d11/traps/$t; echo "$t rc=$?"; done
cargo test --locked --test constants
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
```

### 手順 2: 構文 `const def`

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/docgen.rs`、`FunctionDecl` を作る全箇所（`src/check.rs` の 3 か所、`src/polymorph.rs`、
  `src/parser.rs` の括弧付き `fn`）、`tests/constants.rs`。
- 内容: 「段ごとの変更」の構文・整形・文書生成・`constant: false` の行。field の追加で compile error になった構築箇所をすべて直す
  （`..` での省略はしない）。テスト `parses_const_def_signatures_and_formatting`（新規）を足す。
- 確認: `cargo test --locked --test constants` が `9 passed`。`cargo test --locked` が成功する。

### 手順 3: `Names::const_functions` と署名の検査

- 変更: `src/check.rs`（`Names`、型変数の検査）、`src/constants.rs`（`value_type`）、`tests/constants.rs`。
- 内容: `const_functions` を集め、D8 の署名規則を検査する。`value_type` は `Type` の全 variant を列挙し、`_ =>` を使わない。
  テスト `const_def_signatures_must_be_concrete_values`（新規）。
- 確認: `cargo test --locked --test constants` が `10 passed`。

### 手順 4: 本体の静的検査

- 変更: `src/constants.rs` の `validate_function`（新規）、`src/check.rs` の `constants::fold` の直前、`tests/constants.rs`。
- 内容: D2 の表どおりに許可・拒否する。`TypedExprKind` の全 variant を列挙し、`_ =>` を使わない。走査は明示スタック。
  テスト `const_def_bodies_reject_unsupported_operations`（新規）。
- 確認: `cargo test --locked --test constants` が `11 passed`。

### 手順 5: 初期化式の呼び出しと依存の走査

- 変更: `src/constants.rs` の `validate`・`Evaluator::run`・`fold`、`src/check.rs` の定数のループ。
- 内容: 全モジュールの `const def` 名を集めて `validate` に渡す。`run` の依存走査は `const def` の本体へ降りる（根ごとの訪問済み集合）。
  この時点で `const def` の呼び出しは旧 `evaluate` の fallback で `E1026` になる（次の手順で解消）。
- 確認: `cargo test --locked --test constants` が `11 passed`（`identity 1` は `E1026` のまま）。

### 手順 6: `Machine` へ置き換える（D06 の範囲だけ）

- 変更: `src/constants.rs`（`Value`・`Frame`・`Task`・`Machine`、`Evaluator::evaluate` の削除）。
- 内容: D06 の対応範囲だけを明示スタックで評価する。数値は既存 helper を呼ぶだけ。
- 確認: `cargo test --locked --test constants` が `11 passed`。`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri constants`
  が成功する。手順 1 の 2 つの IR と `cmp` で一致する。

### 手順 7: 呼び出し・束縛・再帰

- 変更: `src/constants.rs`（`Call`・`Return`・`Block`・`Local`・場所の読み出し）、`tests/constants.rs`。
- 内容: 深さ 128 の検査を入れる。テスト `const_def_calls_evaluate_like_runtime`（新規、この段では呼び出し・`let`・`if`・再帰のケース）と
  `const_dependencies_flow_through_const_defs`（新規）。
- 確認: `cargo test --locked --test constants` が `13 passed`。

### 手順 8: ループ・代入・`match`・`new [T](n, f)`・trap・上限

- 変更: `src/constants.rs`（`Loop`・`Fill`・`Arm`・`Assign`・`trapped`）、`tests/constants.rs`。
- 内容: アルゴリズムの残り。`const_def_calls_evaluate_like_runtime` にループと CRC のケースを足し、
  `const_def_traps_report_the_trap_kind`（新規）と `bounds_const_def_calls_steps_and_values`（新規）を足す。
- 確認: `cargo test --locked --test constants` が `15 passed`。`cargo test --locked` が成功する。

### 手順 9: E2E suite `const_functions`

- 変更: `tests/fixtures/const_functions/Main.tz`（新規）、`tests/features.mjs`（suite `const_functions`、GUIDE §7.4）。
- 内容: 「E2E」の 10 cases。各 export は定数と実行時の呼び出し結果が一致することを `assert` し、定数の値を返す。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri const_functions` が成功する
  （native・WASM × `-O0`・`-O3`、`live == 0`、WASM import なし）。

### 手順 10: stack-depth と特殊化の上限

- 変更: なし。
- 確認: 手順 1 の stack-depth 3 テストと `cargo test --locked honors_the_exact_specialization_limit` が成功する（debug build、2 MiB stack）。

### 手順 11: 既存 IR の不変

- 変更: なし。
- 確認: 手順 1 と同じ 2 つの IR を `/tmp/tz-d11/after/` に出し、`cmp /tmp/tz-d11/base/constants.ll /tmp/tz-d11/after/constants.ll` と wasm 版が一致する。

### 手順 12: ドキュメント

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/language-reference/values-and-constants.md _docs/feature-status.md` が成功する。

### 手順 13: 最終確認

- 確認: GUIDE §10 のコマンドがすべて成功する。`node tests/features.mjs target/release/tsuzuri`（全 suite）が成功する。

## テスト計画

### Rust テスト

すべて `tests/constants.rs`。値は既存テストと同じく、入口の本体（束縛なしの `Block` を剥がす）か `read` 関数の本体の `TypedExprKind` で見る。

| テスト | 内容 |
| --- | --- |
| `parses_const_def_signatures_and_formatting` | `const def`・`private const def`・`export const def`・`const def rec` と `and` の署名が `constant == true`。`const fn f x = x`・`const let f = \x -> x`・`const export def f :: i64` が `E0002`。`format_source` の冪等性と `ast_fingerprint` の一致 |
| `const_def_signatures_must_be_concrete_values` | `'a -> 'a`、関数型の引数、`ref [i64]` の引数、実装が引数を一つだけ名付ける 2 引数の署名、リストの結果が `E1026`。配列・タプル・レコード・string の署名は受理 |
| `const_def_bodies_reject_unsupported_operations` | 未使用の `const def` でも、`Array.sum xs`、`const def` でない関数の呼び出し、`let f = \x -> x`、`[\|1\|]`、`&mut` の借用、string の `for`、extern の呼び出しが `E1026` |
| `const_def_calls_evaluate_like_runtime` | `twice 21` → 42、`fib 20` → 6765、1..100 の和 → 5050、Collatz 27 の歩数 → 111、`classify`（`match`）、`i8` の 127 + 1 → 128（`u128` 表現）、CRC 表の 1 番目 → `0x77073096`。各ケースで native・wasm の IR を 2 回出して一致 |
| `const_dependencies_flow_through_const_defs` | `const A: i64 = f()`・`fn f = B * 2`・`const B: i64 = 21` → 42。`fn f = A + 1` は `E1026`（cyclic） |
| `const_def_traps_report_the_trap_kind` | 本体内のゼロ除算（`trapped on zero`）、範囲外の索引（`index out of bounds`）、step 0（`range step is zero`）、負の長さ（`negative array length`） |
| `bounds_const_def_calls_steps_and_values` | `down 127` 受理、`down 128` は `128 nested`。終わらない `while` と `new [i64](2000000, \i -> i)` は `compiler limit` で速やかに終わる。10,000 回のループは受理。自己再帰 union を 129 段つなぐと `compiler limit` |

### E2E

suite `const_functions`（新規）、fixture `tests/fixtures/const_functions/Main.tz`（新規）。期待値は下の独立な参照で、コンパイラ出力から写さない。

| case | 期待値 | 参照 |
| --- | --- | --- |
| `crc_first` | `1996959894n`（`0x77073096`） | CRC-32 表の既知値 |
| `crc_last` | `755167117n`（`0x2D02EF8D`） | 同上 |
| `crc_check` | `3421780262n`（`0xCBF43926`） | CRC-32 の標準検査値（`"123456789"`） |
| `fib_rec` | `6765n` | F(20) |
| `fib_iter` | `2880067194370816120n` | F(90)（JS BigInt で計算） |
| `collatz` | `111n` | 27 の Collatz 歩数 |
| `wrapping` | `-128n` | `i8` の 127 + 1 を `i64` へ符号拡張 |
| `min_max` | `897n` | `[5, -3, 9, 0]` の (min, max) から `max * 100 + min` |
| `classify_sum` | `118n` | `classify` を 0..9 で合計（10 + 20 + 2 × 44） |
| `poly_value` | `1.7 * 1.7 * 0.1 + 1.7 / 3` | suite 内の JS の double 演算（左から同じ順） |

`inspect(ir)` で `assert.match(ir, /\b6765\b/)`（定数が展開されている）。native・WASM × `-O0`・`-O3`、`live == 0`、WASM import なしは
harness が確かめる。既存 suite `constants` も手順 6・13 で回す。

### 既存テストへの影響

なし。`validates_unused_constants_cycles_and_value_names` の `identity 1` は D9 の前置検査で従来メッセージの `E1026` のまま。

### 性能

数値の目標はない。CRC 表の fixture の `check` 時間を `/usr/bin/time -l` で記録するのは任意で、閾値にしない。

## ドキュメント

- `docs/language.md` の「コンパイル時定数」: `const def` の構文、部分集合（D2）、署名の規則、上限、trap の報告、実行時にも呼べること。
  「関数／クラスメソッド呼び出し…は定数式に書けません」を「`const def` 以外の関数…」に改める。
- `docs/architecture.md`: `src/constants.rs` の行に「`const def` の明示スタック評価」、`**定数:**` の段落に `Machine` と予算の共有。
- `_docs/language-reference/values-and-constants.md`: `const def` の節と実行できる例（`node scripts/check-docs.mjs` で検査）。
- `_docs/feature-status.md` の D11 行と `_features/README.md` の D11 の状態（GUIDE §8・§10）。

## 受け入れ条件

- [ ] `const def` を `const` の初期化式から呼べ、結果が実行時の同じ関数の結果と一致する（suite `const_functions`、native・WASM × `-O0`・`-O3`）。
- [ ] 診断の表の各行が `E1026`・`E0002` とそのメッセージで報告される（Rust テスト 7 件）。
- [ ] 評価器に Rust の再帰がなく（`charge` と値の複製・解放は入れ子 128 以下）、stack-depth の 3 テストが成功する。
- [ ] `const def` を書かないプログラムの IR が byte 単位で変わらない（手順 11）。既存テストの期待値を変えていない。
- [ ] Phase 2 に着手していない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `constant_id` は引数なしの `const def` の呼び出しにも一致する。必ず `constants` か `const_functions` で絞る。`fold` が本体を値で置き換えるのは
  `constants` だけで、`const def` の本体は残す（実行時に呼ぶ）。
- `TypedExpr` の派生 `Clone`・`Drop` は再帰する。ループで深い union を作ると、上限検査の前に Rust の stack を使い切る。集成体を作る時点で
  `Value::depth` を検査する（`bounds_const_def_calls_steps_and_values` の最後のケース）。
- 大きな配列を `Local` から読むたびに複製すると予算も時間も膨らむ。`Field`・`Index`・`Length` は場所を辿って読み、複製しない。
- 予算はプログラム全体で一つなので、大きな表の後の定数で `compiler limit` が出ることがある。報告位置は超過した式で正しい。文書にも書く。
- `validate` に呼び出しを素通りで許すと、未定義名の呼び出しが `E1002` に変わり既存テストが壊れる（D9）。
- `Parser::define` は実装の引数が署名より少ないと残りを関数型の結果にする。`fn f x = \y -> ...` 形の `const def` は署名の検査で拒否される。
- 大きな定数配列はループ内で使うと反復ごとに生成される（D-28）。例と文書では `let table = CrcTable` をループの外に置く。
- 範囲の終端は型の幅で扱う。`i64` で計算して最後に切り詰めると `125i8 .. 2i8 .. 127i8` が 3 要素になる。
- `cargo test --locked constants` は他の binary の同名テストも拾う。`--test constants` を使い、件数を見る。

## 対象外

- 型レベルの計算（A16 Phase 2 の const ジェネリクス）、マクロ、コンパイル時の IO・乱数・時刻。
- `const def` 以外の関数の自動的なコンパイル時評価（LLVM の最適化による畳み込みは従来どおり）。
- builtin・std 関数（`assert`・`Array.*`・`String.*` など）、クラスメソッド、ジェネリックな `const def`、リスト・`Vec`・`Map`、string の走査と連結、
  decimal の演算。
- static データ（Phase 2、D10）。

## 決定事項

### D1: 修飾子の表記

- 決定: `const def`（`private const def`、`export const def`）。`const` は署名だけに付け、`fn`・`and`・`let` の実装は継承する。
  `const def rec` のグループの `and` 署名も `const` になる。
- 理由: 既存の予約語だけで表せ、`private` と同じ継承規則で覚えることが増えない。`const fn` は Rust と似るが、Tsuzuri の `fn` は実装側なので混乱する。
- 状態: 既定案（実装者はこの案に従う）

### D2: 評価できる式（Phase 1 の部分集合）

- 決定: 許可する `TypedExprKind` は `Int`・`Float`・`String`・`Bool`・`Unit`・`Local`・`Unary`・`Binary`・`Cast`・`If`・`Block`・`While`・
  `ForRange`・配列だけの `ForEach`・`Break`・`Continue`・`Match`・`Record`・`RecordUpdate`・`Array`・`Tuple`・`NewLiteral`（配列）・`Construct`・
  `UnionTag`・`UnionPayload`・`Field`・`Index`・`Length`・`Assign`（場所へ）・`NewArray`（初期化は lambda か 1 引数の `const def`）・
  `const def` の完全適用の `Call`・定数参照。`Borrow(_, false)`・`BorrowOperand`・`Dereference` は `Field`・`Index`・`Length` の基部だけ。
  それ以外（`HostCall`・`Lambda` の他の位置・`Closure`・Task・`Parallel`・`Structural*`・`List`・`ListTail`・`NewList`・`Slice`・
  `StringLength`・`Method`・`GenericFunction`・`TypeFunction`・builtin の呼び出し・可変借用）は `E1026`。
- 理由: 表の生成に要る制御と値の組み立てを覆い、値に番地のない評価器で実行時と同じ意味を保てる範囲に限る。
- 状態: 既定案（実装者はこの案に従う）

### D3: 評価器

- 決定: `Evaluator::evaluate` を明示スタックの `Machine`（`Task`・`Frame`・`Value`）に置き換え、D06 の const も同じ経路で評価する。
  環境はフレームごとの `BTreeMap<usize, Value>`（`Local::id`）。数値は `unary`・`binary`・`cast` を変えずに呼ぶ。
- 理由: 呼び出し 128 段 × 式 128 段を Rust の再帰で処理すると 2 MiB stack を越える。経路を一つにすると D06 と D11 で意味がずれない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 資源上限

- 決定: 予算は既存の `remaining`（1,048,576、プログラム全体で一つ）を評価と複製で共有する。呼び出しの深さ 128、値の入れ子 128、
  配列長は予算以下、依存探索は従来どおり。超過は `E1026`。
- 理由: 既存の上限（GUIDE D-28）と揃え、新しい数値を増やさない。旧版の 16,777,216 ステップ・64 MiB はこの値と矛盾するので採らない。
- 状態: 既定案（実装者はこの案に従う）

### D5: 数値と trap の報告

- 決定: 実行時と同じ意味。実行時に trap する操作は、trap した式の span で `E1026`、メッセージに trap の種類を入れる。f16・f128 は
  APFloat で評価し、decimal は D06 のまま（リテラル・符号・恒等 cast）。
- 理由: 同じ関数をコンパイル時と実行時に呼んで結果が変わらないことが `const def` の契約である。
- 状態: 既定案（実装者はこの案に従う）

### D6: 結果の展開

- 決定: 結果は D06 と同じ型付きリテラルとして各利用へ展開する。static データ・専用 global は作らない。
- 理由: GUIDE D-28 を変えずに Phase 1 を出せる。
- 状態: 既定案（実装者はこの案に従う）

### D7: 実行時の `const def`

- 決定: `const def` は通常の関数として型検査・所有権検査・出力する。`const` は IR を変えない。`const` の初期化式以外での呼び出しは
  コンパイル時に評価しない。
- 理由: 暗黙の評価はコンパイル時間と診断の位置を読めなくする。
- 状態: 既定案（実装者はこの案に従う）

### D8: 署名の規則

- 決定: 型変数・制約なし。引数と結果は定数値型（型規則の列挙）だけ。部分適用と関数値としての使用は不可。
- 理由: 値に番地・関数値・所有者の追跡を持たない評価器で扱える型に限る。
- 状態: 既定案（実装者はこの案に従う）

### D9: 検査の位置

- 決定: 初期化式の AST は `const def` 名の前置検査だけで呼び出しを許し、型検査後に呼び先を確定する。`const def` の本体は未使用でも
  型検査後に `validate_function` で検査する。
- 理由: 未定義名の呼び出しが従来どおり `E1026` になり、既存の期待値を保てる。未使用の定数も評価する D06 と方針を揃える。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2 の static データ

- 決定: 大きな定数配列を読み取り専用データとして一度だけ出力し、`ref` で借用させる。static な region を導入する。
- 理由: 表をループで使うときの生成コストをなくせる。ただし GUIDE D-28 の「static lifetime を導入しない」を変える（D-30 で承認待ち）。
- 状態: 要承認（承認前は Phase 2 に着手しない）
