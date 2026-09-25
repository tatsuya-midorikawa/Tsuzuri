# A03: match の網羅性・到達不能節の検査

| 項目 | 内容 |
|---|---|
| ID | A03 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | A02 |
| 後続 | G03 |
| 状態 | done |
| 主な影響ファイル | `src/check.rs`, `src/control.rs`, `src/ownership_control.rs`, `src/llvm_control.rs`, `src/main.rs`, `src/diagnostic.rs`, `src/syntax.rs`, `tests/control.rs`, `tests/fixtures/control/Main.tz`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

`match` と関数ガードで、実行時まで行かないと分からなかった「どの節にも一致しない」ケースをコンパイル時に検出する。
union 導入後に `Option` / `Result` を安全に使えるようにし、到達不能な節も warning として報告する。

このチケットでは A02 の union 型を含むパターン網羅性・有用性検査を実装する。
G03 の本格的な警告基盤は未実装なので、A03 では最小限の warnings channel だけを追加し、G03 が拡張できる形にする。

## 現状

- `src/control.rs` の `Checker::match_value` はすべての arm を型検査し、`TypedExprKind::Match` にする。
  どの arm にも一致しない場合は `src/llvm_control.rs::match_expression` が `llvm.trap` / `unreachable` を生成する。
- 現在は非網羅 match をコンパイルエラーにしない。
  `tests/fixtures/control/Main.tz` の `trap_match` は `match 2 with | 1 -> 0` を runtime trap として検証している。
- `src/control.rs` の `pattern_alternatives` は source `Pattern` を `PatternStep::Test` と `PatternStep::Bind` の列へ変換する。
  OR は alternatives を増やし、最大 `MAX_PATTERN_ALTERNATIVES = 1024`。
- `src/llvm_control.rs::switch_cases` は integer/bool/unit の単純な constant pattern を `switch` へ下げる。
  A02 で union tag に拡張される。
- `src/parse_control.rs::guarded_definition` は関数ガードを `ExprKind::Match` に変換する。
  bool predicate だけの節は wildcard pattern + guard として表される。
- `src/parse_control.rs::fx` は分解パターン引数を synthetic `ExprKind::Match` に変換する。
  `src/control.rs` の `For` 分岐も、単純束縛でない `for pattern in ...` を 1-arm match に変換する。
  これらは現行仕様上「不一致なら runtime trap」であり、A03 で compile error にしてはいけない。
- CLI の診断は `src/diagnostic.rs::Diagnostic::render` / `json` が常に `"error"` を出す。
  `src/main.rs` は driver から返る tool warning を `W2001` として文字列出力するだけで、型検査 warning は存在しない。
- `docs/language.md` の診断表では `E1021` と `W1003` はまだ未追加。

## 仕様

### 対象

コンパイル時検査の対象:

- ソースに明示された `match value with ...`
- 関数ガード `fn f x | pattern -> ... | otherwise -> ...`
- 関数ガードの bool predicate 節

対象外で runtime trap を維持:

- `for pattern in values do ...` の分解パターン
- `fx pattern -> ...` の分解パターン
- コンピュテーション式の `for pattern in ... do` が lowering で作る分解パターン
- active recognizer の部分認識器失敗
- guard だけで網羅される可能性がある match

理由:

- `for` と `fx` は現行仕様で「分解不一致時に trap」と文書化されており、A03 で破壊的に拒否すると既存の `trap_pattern`, `trap_lambda`, `trap_for_active` と利用者コードを壊す。
- guard は任意式であり、コンパイラは一般には真偽を証明できない。
  guard 付き arm は網羅性に数えない。
- B01/D-21 の `unreachable : unit -> 'a` は、`Option.get` のような部分関数を **網羅的な match** の中で明示 trap させるために使う。
  A03 は std や特定関数に非網羅許可を与えない。

### 診断

- 非網羅 match / 関数ガードは `E1021`。
- 到達不能 arm は `W1003` warning。
- 資源上限超過は `E1017`。
- warning はビルド・check を失敗させない。

`E1021` メッセージ例:

```text
match is not exhaustive; missing: Some (Rect _)
```

単純例:

```text
match flag with
| true -> 1
```

診断:

```text
E1021: match is not exhaustive; missing: false
```

union 例:

```text
union Maybe<'a> = None | Some of 'a
match value with
| Some x -> x
```

診断:

```text
E1021: match is not exhaustive; missing: None
```

到達不能:

```text
match value with
| _ -> 0
| Some x -> x
```

warning:

```text
W1003: unreachable match arm; previous patterns already cover this arm
```

### warning channel

G03 までの最小実装:

```rust
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub unions: Vec<CheckedUnion>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
    pub warnings: Vec<Diagnostic>,
}
```

- `Diagnostic` は error / warning の両方に再利用してよい。
- `src/diagnostic.rs` に `render_with_severity(&self, severity: &str, path: &str, source: &str)` と `json_with_severity(...)` を追加する。
  既存の `render` / `json` は error を呼ぶ wrapper として残す。
- `src/main.rs` は `Project::analyze()` 成功後、action 実行前に `module.warnings` を出力する。
  各 warning は `project.source_for(warning)` で path と source text を取り、warning ごとの source file に対して severity variant を呼ぶ。
  build/run の LLVM tool warning `W2001` は既存どおり action 後に出力する。
- JSON warning:

```json
{"severity":"warning","code":"W1003","message":"unreachable match arm; previous patterns already cover this arm","path":"Main.tz","span":{"start":...}}
```

- text warning:

```text
Main.tz:4:3: warning[W1003]: unreachable match arm; previous patterns already cover this arm
```

### 網羅性の意味

- arm は上から順に評価される。
- guard 付き arm は **到達不能検査には使う**が、**網羅性には数えない**。
  ただし、以前の unguarded patterns が現在の guarded arm の pattern を完全に覆う場合、その guarded arm は `W1003`。
- `otherwise` と `_` は wildcard。
- OR pattern は左から順に試す。
  usefulness では OR の各 alternative を展開する。
- AND pattern は両方を満たす値だけを覆う。
- `as` は coverage には影響しない。
- 型注釈 pattern は注釈が対象型と一致する前提で中身だけを見る。
- active pattern:
  - partial recognizer `(|Name|_|)` は coverage に寄与しない。
  - total recognizer `(|Name|)` は payload pattern が irrefutable な場合だけ wildcard として coverage に寄与する。
    payload pattern が refutable な場合は recognizer result の constructor domain を source 型の constructor domain へ戻せないため opaque/non-covering とする。
  - missing example 生成では active recognizer を優先して表示しなくてよい。対象型の通常 constructor で例を出す。
- literal pattern:
  - `bool` は有限 domain なので `true` と `false` を両方見れば網羅。
  - `unit` は `()` だけ。
  - integer, float, string は原則 infinite domain として扱い、literal の列挙だけでは網羅しない。
    `i8` の全 256 値網羅も A03 では実装しない。
  - literal key は型検査・丸め後に作る。key は resolved `Type` と canonical bits/text を含む。
  - signed zero は runtime equality に合わせて canonicalize し、`0.0` と `-0.0` は同じ literal key。
  - NaN literal pattern は runtime equality で自分自身にも一致しないため coverage 上は何も覆わない。
- tuple は各要素の直積。
- record はフィールド集合の直積。
  部分 record pattern は指定していないフィールドを wildcard とみなす。
- array / list:
  - array は `ArrayLen(n)` constructor と「other lengths」を持つ infinite-like domain として扱う。`[p1; ...; pn]` は長さ n だけを覆る。任意長を網羅するには wildcard が必要。
  - list は `Nil` / `Cons(head, tail)` の disjoint constructors に正規化する。
    `[||]` は `Nil`、`[|p1; ...; pn|]` は `Cons(p1, Cons(..., Nil))`、`head :: tail` は `Cons(head, tail)`。
    `Nil` と `Cons(_, _)` が unguarded に覆われれば list は網羅。
- union:
  - 各 case が constructor。
  - payload なし case は 1 constructor。
  - payload あり case は payload pattern の coverage を見る。
  - 全 case が unguarded に覆われれば網羅。

## 設計

### 表現選択

Maranget-style の usefulness / exhaustiveness は、raw source `Pattern` を再解決せず、型検査済みの coverage 表現で実行する。
`control.rs::Checker::pattern_alternatives` は既に case / active pattern / record fields / literals を型付き lowering alternatives へ解決するので、その同じ解決結果から coverage 用の `CoveragePat` を返す。
これにより、A02 の union case と active recognizer を網羅性検査側で二重解決して名前解決順をずらすバグを避ける。

新規ファイル:

```rust
// src/exhaustiveness.rs または src/check/exhaustiveness.rs 相当
pub(super) fn check_match(
    matched_ty: &Type,
    arms: &[CheckedCoverageArm],
    context: MatchOrigin,
    span: Span,
) -> Result<Vec<Diagnostic>, Diagnostic>;

pub enum MatchOrigin {
    Explicit,
    FunctionGuard,
    FxDestructuring,
    ComputationDestructuring,
}

pub struct CheckedCoverageArm {
    pub pattern: CoveragePat,
    pub guard: bool,
    pub span: Span,
}
```

`Checker` の非公開フィールドにアクセスする必要があるため、`control.rs` と同じく `check.rs` の child module にする。

### パターン正規化

型検査済み pattern を coverage 用の簡略表現に変換する。

```rust
enum CoveragePat {
    Wildcard,
    Constructor(Constructor, Vec<CoveragePat>),
    Or(Vec<CoveragePat>),
    And(Box<CoveragePat>, Box<CoveragePat>),
    Literal(LiteralKey),
    OpaqueNonCovering,
    Empty,
}

enum Constructor {
    Bool(bool),
    Unit,
    Union { union_id: usize, case_id: usize },
    Tuple(usize),
    Record { record_id: usize, field_count: usize },
    ArrayLen(usize),
    ListNil,
    ListCons,
}

struct LiteralKey {
    ty: Type,
    bits_or_text: String,
}
```

- `PatternKind::Binding`, `Wildcard`, `As(_, _)` は wildcard。
- `PatternKind::Annotated(inner, ty)` は型を確認済みとして `inner`。
- record は full-arity constructor に正規化し、omitted fields は wildcard にする。
- list literal は nested `ListCons` / `ListNil` に正規化し、`ListLen(n)` は使わない。
- array literal だけ `ArrayLen(n)` を使い、missing example 生成では covered lengths 以外を `"other lengths"` として `_` または `[_, _, ...]` で示す。
- `PatternKind::Apply` / uppercase `Binding` は `pattern_alternatives` と同じ typed resolution table から union case または active pattern に分類する。
- active partial は `OpaqueNonCovering`。
- active total は payload pattern が irrefutable なら `Wildcard`、refutable なら `OpaqueNonCovering`。
- impossible AND intersection は `Empty`。

### Usefulness / exhaustiveness algorithm

基本は Maranget の pattern matrix:

- `useful(matrix, vector)` が true なら新しい arm は到達可能。
- `missing(matrix, ty)` があれば非網羅。
- `matrix` には **unguarded arms** だけを coverage として追加する。
- `useful` 判定では、現在 arm 自体に guard があってもその pattern vector が過去 unguarded matrix に対して useful かを見る。

Tsuzuri の match は単一 scrutinee なので top-level vector 長は 1。
tuple / record / union payload で列を展開する。

資源上限:

- 行数 × constructor 展開数が 1024 を超えたら `E1017`。
- 再帰 depth が `MAX_NESTING` を超えたら `E1017`。
- OR 展開後 alternatives は既存 `MAX_PATTERN_ALTERNATIVES = 1024` と揃える。

疑似コード:

```text
check_match(type T, arms):
  matrix = []
  warnings = []
  for arm in arms:
    vectors = expand_or(arm.pattern)
    useful_any = any(useful(matrix, [v], T) for v in vectors)
    if !useful_any:
       warnings.push(W1003 at arm.pattern.span)
    if arm.guard is None:
       matrix.extend(vectors)
  if let Some(example) = missing(matrix, T):
       error E1021 "match is not exhaustive; missing: {example}"
  return warnings
```

`otherwise when condition` は guard 付きなので matrix には追加しない。
したがって次は非網羅:

```text
fn f x
    | otherwise when x > 0 -> 1
```

missing は `_`。
部分関数を意図する場合も非網羅にせず、missing constructor を明示し、その branch で `unreachable ()` を呼ぶ。

### Missing example 生成

出力は人間向けで、完全最小でなくてよい。

| 型 | missing example |
|---|---|
| `bool` | `true` または `false` |
| `unit` | `()` |
| union nullary | `None` |
| union payload | `Some _`、nested なら `Some (Rect _)` |
| tuple | `(_, missing, _)` |
| record | `TypeName { field = missing }` または `{ field = missing }` |
| array len | `[_, _]` |
| list nil | `[||]` |
| list cons | `_ :: _` |
| array other lengths | `_` または `[_, _, ...]` |
| integer/float/string | `_` unless uncovered literal example is obvious |

union case 表示は同一モジュールなら単純名、曖昧なら `Module.Case`。
最初の実装では常に `Module.Case` でもよいが、テストの期待文字列は安定させる。

### 関数ガード

`parse_control.rs::guarded_definition` は関数引数を 0 個なら `unit`、1 個ならその値、複数なら tuple にして `ExprKind::Match` を作る。
A03 の explicit match 検査をそのまま適用する。

- bool predicate 節は wildcard + guard として扱われるため、網羅性に数えない。
- `| otherwise -> ...` があれば網羅。
- pattern 節 `| (a,b) -> ...` は unguarded なら網羅に数える。

### runtime trap の扱い

- 明示 match / 関数ガードが compile-time exhaustive と分かる場合でも、LLVM の failure block は defense in depth として残す。
  理由:
  - guard 付き arm を含む match は網羅性に数えないため、failure block が必要。
  - 将来の optimizer / bug で不正 IR が来ても silent default を返さない。
- `llvm_control.rs::match_expression` の `failure` block は削除しない。
- exhaustive と判定できる単純 switch でも default label は failure block のまま。

### AND / OR / record intersection

- OR は alternatives へ展開する。総 alternatives が 1,024 を超えたら `E1017`。
- AND は bounded intersection として扱う。
  - constructor が同じなら children を再帰的に intersect する。
  - constructor が異なれば `Empty`。
  - wildcard と `p` の intersection は `p`。
  - literal 同士は typed `LiteralKey` が等しければその literal、異なれば `Empty`。
  - OR を含む場合は分配し、分配後 alternatives が 1,024 を超えたら `E1017`。
- record は full-arity constructor なので、`{ x = 1 } & { y = 2 }` は同一 record constructor の child intersection になる。

### 各コンパイラ段の変更有無

| 段 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | `MatchOrigin::{Explicit, FunctionGuard, FxDestructuring, ComputationDestructuring}` を source AST に持たせる |
| computation::expand | computation expression の destructuring lowering で作る match を `ComputationDestructuring` にする |
| check | `CheckedModule.warnings`、typed `CoveragePat`、exhaustiveness module、`Checker::match_value` から呼び出し |
| polymorph | 変更なし |
| control | `match_value` に origin/context を追加し、`Explicit` / `FunctionGuard` のみ `E1021` 対象にする。source `for` は match AST を作らず destructuring-trap origin を直接渡す |
| closures | 変更なし |
| recursion | 変更なし |
| ownership | 変更なし |
| ownership_control | 変更なし |
| call_specialization | 変更なし |
| llvm | 変更なし |
| llvm_control | failure trap を維持。switch default を変えない |
| llvm_frame | 変更なし |
| runtime | 変更なし |
| driver | 変更なし |
| main | warnings 出力を追加 |

### for / fx の trap semantics

`control.rs::Checker::control_expression` の `For` 分岐は、分解 pattern を 1-arm match と同じ lowering で型検査する。
この call には source AST を作らず `MatchOrigin::FxDestructuring` とは別の destructuring-trap context を直接 `match_value` に渡す。

`parse_control.rs::fx` が生成する synthetic match は source 上の `fx` parameter に由来する。
`Checker::lambda` が body を検査するときに通常の `ExprKind::Match` と区別できない場合、AST に marker がない。
既定案:

- `ExprKind::Match` に `origin: MatchOrigin` を追加する。

```rust
pub enum MatchOrigin {
    Explicit,
    FunctionGuard,
    FxDestructuring,
    ComputationDestructuring,
}

ExprKind::Match {
    value: Box<Expr>,
    arms: Vec<MatchArm>,
    origin: MatchOrigin,
}
```

- `Parser::match_expression` と `guarded_definition` は `Explicit` / `FunctionGuard`。
- `fx` synthetic match は `FxDestructuring`。
- computation expression lowering が pattern destructuring 用に生成する match は `ComputationDestructuring`。
- source `for pattern in ...` は AST origin ではなく `control.rs` から destructuring-trap context を直接渡す。

この marker は `computation::expand`, `TypedExpr::children`, LLVM に影響しない。

### A04 への前提

A04 の recursive heap types は internal `Type::Boxed(T)` を導入する。
A03 の coverage normalization は pattern boundary で `Boxed(T)` を消し、constructor lookup / missing witness 生成を inner `T` に対して行う。
recursive witness は `visited` set、depth limit、node count limit を持ち、再訪した型は `_` で打ち切る。
これにより `Leaf | Node _` は `Tree` に対して exhaustive と判定でき、nested recursive patterns でも無限 witness 生成を避ける。

## 実装手順

### 1. 最小 warnings channel

1. `CheckedModule.warnings: Vec<Diagnostic>` を追加する。
2. `Diagnostic::render_with_severity` / `json_with_severity` を追加する。
3. `src/main.rs` で analyze 成功直後に warnings を出力する。
4. `check::check_modules` の全 return path で warnings を保持する。
   `polymorph::specialize` と `closures::lower` が `CheckedModule` を再構築するため、warnings を失わないよう引き継ぐ。

確認:

```sh
cargo test --locked --test frontend warning_rendering_uses_source_path
cargo test --locked --test frontend
# 各コマンドの `running N tests` が 0 でないことを記録する
```

### 2. MatchOrigin の追加

1. `src/syntax.rs` に `MatchOrigin` を追加し、`ExprKind::Match` に `origin` を持たせる。
2. `parse_control.rs::match_expression`, `guarded_definition`, `fx` を origin 付きにする。
3. `computation.rs` の destructuring lowering が生成する match を `ComputationDestructuring` にする。
4. `control_expression` の source `for` は `match_value` に destructuring-trap context を直接渡す。
5. `TypedExpr` 生成側を更新する。

確認:

```sh
cargo test --locked --test control match_origin_destructuring
cargo test --locked --test control
```

### 3. bool/unit/literal/wildcard の usefulness

1. `exhaustiveness` module を追加する。
2. bool / unit / wildcard / typed literal infinite domain を実装する。
3. `Checker::match_value` で `Explicit` と `FunctionGuard` のみ `E1021` 検査を呼ぶ。
4. `pattern_alternatives` が lowering alternatives と typed `CoveragePat` を一緒に返すようにする。
5. `E1021` と `W1003` の基本テストを追加する。

確認:

```sh
cargo test --locked --test match_exhaustiveness
```

### 4. tuple / record / array / list

1. tuple constructor の分解を実装する。
2. record partial field pattern を wildcard field で補う。
3. array は `ArrayLen(n)` と other lengths を実装する。
4. list は `Nil` / `Cons(head, tail)` 正規化を実装する。
5. OR/AND/as/annotation と bounded AND intersection を追加する。
6. 資源上限 `E1017` を追加する。

確認:

```sh
cargo test --locked --test match_exhaustiveness
cargo test --locked --test control
```

### 5. union / active pattern

1. A02 の `CheckedUnion` / `Type::Union` / case resolver を使い、union constructor coverage を実装する。
2. nested missing example を作る。
3. active partial / total の規則を実装する。
4. 関数ガードの網羅性をテストする。

確認:

```sh
cargo test --locked --test union_types
cargo test --locked --test match_exhaustiveness
```

### 6. fixture 更新と docs

1. 非網羅 match を含む既存 fixture を更新する。
2. `tests/control.rs` の runtime trap 期待を compile error 期待へ分離する。
3. docs を更新する。
4. full validation を実行する。

確認:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test match_exhaustiveness
cargo test --locked --test control
cargo test --locked --test frontend
# 各コマンドの `running N tests` が 0 でないことを記録する
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
cargo build --release --locked
node tests/primitives.mjs target/release/tsuzuri
```

## テスト計画

すべての Rust 検証は `cargo test --locked --test <file>` 形式で実行し、出力の `running N tests` が 0 でないことを記録する。
Node E2E block の直前には毎回 `cargo build --release --locked` を実行する。

### Rust 受理テスト

```text
match true with
| true -> 1
| false -> 0
```

```text
match () with
| () -> 42
```

```text
union Maybe<'a> = None | Some of 'a
match Some 42 with
| Some n -> n
| None -> 0
```

```text
union Shape = Empty | Circle of f64 | Rect of f64 * f64
match Rect (3.0, 4.0) with
| Empty -> 0.0
| Circle r -> r
| Rect (w, h) -> w * h
```

```text
match (true, ()) with
| (true, ()) -> 1
| (false, ()) -> 0
```

```text
record R { ok: bool, value: i64 }
match R { ok: true, value: 42 } with
| { ok = true } -> 1
| { ok = false } -> 0
```

```text
match [1, 2] with
| [] -> 0
| [x] -> x
| _ -> 42
```

```text
match [|1, 2|] with
| [||] -> 0
| _ :: _ -> 1
```

Function guard:

```text
def classify :: bool -> i64
fn classify flag
    | true -> 1
    | false -> 0
classify true
```

Guarded first arm:

```text
match Some 42 with
| Some n when n > 0 -> n
| Some _ -> 0
| None -> -1
```

### Rust 拒否テスト

| ソース | 期待 |
|---|---|
| `match true with | true -> 1` | `E1021`, missing `false` |
| `match () with | _ when true -> 1` | `E1021`, missing `_` |
| `union Maybe<'a> = None | Some of 'a; match x with | Some n -> n` | `E1021`, missing `None` |
| `union Shape = Empty | Circle of f64 | Rect of f64 * f64; match s with | Empty -> 0 | Circle _ -> 1` | `E1021`, missing `Rect _` |
| `match [1] with | [] -> 0 | [x] -> x` | `E1021`, missing `_` because arrays have arbitrary length |
| `def f :: bool -> i64 fn f x | true -> 1` | `E1021`, missing `false` |
| OR expansion over 1024 alternatives | `E1017` |

### Warning テスト

`analyze` だけで warnings を確認する helper を追加する。

```text
match Some 1 with
| _ -> 0
| Some n -> n
```

期待:

- compile success
- `module.warnings.len() == 1`
- `warnings[0].code == "W1003"`

```text
match true with
| true -> 1
| true -> 2
| false -> 0
```

期待:

- second `true` arm が `W1003`

guarded arm:

```text
match true with
| true when expensive -> 1
| true -> 2
| false -> 0
```

期待:

- warning なし。guarded first arm は coverage に数えないため second true は useful。

Typed literal keys:

```text
match 0.0 with
| -0.0 -> 1
| 0.0 -> 2
| _ -> 3
```

期待:

- second `0.0` arm が `W1003`。signed zero は runtime equality と同じ key。

```text
match (0.0 / 0.0) with
| 0.0 / 0.0 -> 1
| _ -> 2
```

期待:

- NaN literal pattern は coverage 上 empty なので warning なし、match は wildcard で網羅。

decimal / wide rounded duplicates:

```text
match 0.1d128 + 0.2d128 with
| 0.3d128 -> 1
| 0.3000000000000000000000000000000000d128 -> 2
| _ -> 0
```

期待:

- 型検査・丸め後の canonical bits が同じなら second arm は `W1003`。

Destructuring origins:

```text
for [x] in [[1], [2, 3]] do ()
```

期待: compile success、runtime trap は既存 E2E のまま。`E1021` にしない。

```text
(fx [x] -> x) [1, 2]
```

期待: compile success、runtime trap。`E1021` にしない。

```text
Flow { for (x, y) in [(20, 22)] do { yield x + y } }
```

期待: computation-lowered destructuring は builder typing に従い、pattern mismatch は runtime trap semantics。網羅性 `E1021` にしない。

### CLI JSON warning

`tests/frontend.rs` または main tests で、`tsuzuri check --json` に warning が stderr JSON として出ることを確認する。
`cargo test` から binary 実行が重い場合は `Diagnostic::json_with_severity` の unit test と `main.rs` parser test に分ける。
複数ファイルの warning は `src/main.rs` が `project.source_for(warning)` を使うため、warning の source ID ごとの path と source text で rendering される。

期待 JSON:

```json
{"severity":"warning","code":"W1003",...}
```

### 既存 fixture の更新インベントリ

調査した既存 `.tz` / Rust tests のうち、A03 で更新が必要なもの:

- `tests/fixtures/control/Main.tz`
  - `export def trap_match :: i64`
  - 現状: `fn trap_match = match 2 with | 1 -> 0`
  - A03 後は `E1021` になる。
  - 対応: runtime trap fixture から外し、Rust reject test `rejects("match 2 with | 1 -> 0", "E1021")` へ移す。
- `tests/control.rs`
  - `matches_patterns_and_guards` に非網羅拒否テストを追加する。
  - `control_lowering_is_direct_and_tail_calls_stay_loops` の `match n with | 0 -> 42 | _ -> ...` はそのまま有効。
  - `fx` / `for` / computation destructuring trap tests は `MatchOrigin::FxDestructuring` / `ComputationDestructuring` と source `for` の direct destructuring-trap context により維持する。
- `tests/fixtures/control/Main.tz` の `trap_pattern`, `trap_lambda`, `trap_for_active` は更新しない。
  これらは A03 対象外の runtime trap として残す。
- `benchmarks/control/Main.tz`, `examples/control/Main.tz`, `tests/fixtures/storage/Storage.tz`, `tests/fixtures/control/Recursion.tz` は調査範囲では wildcard / variable / exhaustive constructors があり、更新不要。

### E2E

既存 `tests/control.mjs` を更新する。

- `trap_match` exported call を削除するか、compile reject test に移す。
- `trap_pattern`, `trap_lambda`, `trap_for_active` は runtime trap のまま native/WASM `-O0`/`-O3` で確認する。
- Warnings は E2E の build/check で stderr に出る可能性があるため、テスト harness が warning stderr を失敗扱いしないことを確認する。
- Node block の直前に毎回 `cargo build --release --locked` を実行する。

## ドキュメント

- `docs/language.md`
  - `match` 節に「明示 match と関数ガードは網羅性をコンパイル時に検査する」を追加。
  - guard 付き arm は網羅性に数えないことを明記。
  - `for` / `fx` destructuring は不一致時 runtime trap のまま、と明記。
  - 診断表に `E1021`, `W1003` を追加。
- `docs/architecture.md`
  - pattern lowering の不変条件に compile-time usefulness/exhaustiveness を追加。
  - LLVM failure block は defense in depth で残すことを記載。
- `README.md`
  - 制御構文の説明で非網羅 match が compile error になることを短く記載。

## 受け入れ条件

- [x] 明示 `match` が非網羅なら `E1021`。
- [x] 関数ガードが非網羅なら `E1021`。
- [x] guard 付き arm は網羅性に数えない。
- [x] 到達不能 arm は `W1003` warning として報告され、コンパイルは成功する。
- [x] warning が text / JSON CLI の両方で severity `"warning"` として出る。
- [x] bool, unit, union, tuple, record, array/list length, cons, OR/AND/as/annotation, active pattern を扱う。
- [x] list は `Nil` / `Cons` 正規化、array は `ArrayLen(n)` + other lengths として扱う。
- [x] active total recognizer は payload pattern が irrefutable な場合だけ coverage に寄与し、partial recognizer は決して coverage に寄与しない。
- [x] literal keys は型検査・丸め後の resolved Type + canonical bits/text で作り、signed zero と NaN の runtime equality を反映する。
- [x] pattern checking は lowering alternatives と typed `CoveragePat` を同時に返し、網羅性検査は raw AST を再解決しない。
- [x] integer/float/string literal の有限列挙だけでは網羅とみなさない。
- [x] `for` / `fx` / computation-expression destructuring の runtime trap semantics が残る。
- [x] resource limit 超過が `E1017`。
- [x] LLVM の match failure trap は削除されていない。
- [x] 既存 E2E の native/WASM × `-O0`/`-O3` が通る。

## 落とし穴

- `PatternAlternative` だけから網羅性を判断すると、array/list 長さや union case missing example を作りにくい。
  source pattern + type で実装する。
- guard 付き `_ when condition` を coverage に数えると、実行時に guard false で trap するプログラムを誤って受理する。
- `fx [x] -> ...` を E1021 にすると既存仕様を壊す。
  `MatchOrigin` で synthetic destructuring を区別する。
- computation expression lowering が作る destructuring match に `Explicit` origin を付けると、builder code が不必要に `E1021` になる。
- warning を `Diagnostic::render` の既存 `"error"` のまま出すと JSON severity が誤る。
- `src/main.rs` で root input source だけを使って warning rendering すると、別ファイルの warning path/span が壊れる。必ず `project.source_for(warning)` を使う。
- `polymorph::specialize` や `closures::lower` で `CheckedModule` を再構築すると warnings を落としやすい。
- float literal coverage を厳密にやろうとすると NaN / signed zero / decimal semantics が複雑になる。
  A03 では typed literal key と wildcard 必須で扱い、NaN pattern は coverage empty にする。
- active total recognizer の payload pattern を coverage に使う場合でも、missing example は recognizer 名でなく通常 constructor を出す。
- raw AST から union case / active pattern を網羅性検査側で再解決すると、`control.rs` の実際の lowering と結果がずれる。
- 到達不能 warning の span は arm 全体ではなく arm pattern の span にする。

## 対象外

- G03 の完全な警告管理、warning 抑制、warning as error。
- 複数エラー同時報告（G02）。
- integer finite domain の完全列挙検査。
- pattern guard の定数畳み込みや SMT 的証明。
- active pattern の Option 返却・複数ケース拡張（B04）。
- match failure trap の削除。

## 未決事項

- `ExprKind::Match` に `MatchOrigin` を追加する既定案を採る。
  代替は `Checker::match_value` に呼び出し元 context を渡すだけだが、`fx` が生成した synthetic match は通常の lambda body と区別できない。
- warning channel は `CheckedModule.warnings: Vec<Diagnostic>` の最小実装とする。
  G03 で severity enum、複数 warning 種、抑制設定へ拡張する。
- `W1003` を JSON で出す順序は source traversal 順とする。
  G03 で安定ソート規則が変わる可能性があるが、A03 では `match_value` 到達順で十分。

### 実装時の判断（A03）

- 上の未決事項は既定案どおり `ExprKind::Match` に `MatchOrigin { Explicit, FunctionGuard, FxDestructuring, ComputationDestructuring }` を持たせ、
  warning channel は `CheckedModule.warnings: Vec<Diagnostic>` にした。重大度は enum ではなく
  `Diagnostic::render_with_severity`／`json_with_severity` の引数で、`render`／`json` は従来どおり `"error"`。
- 被覆パターンは `control::pattern_alternatives`（と `case_pattern`／`active_pattern`）が lowering の選択肢と同時に返す
  `CoveragePat` で、case・認識器・リテラルは lowering と同じ解決結果を使う。`match_value` は入口で被覆の枠を予約し、節ごとに
  `CoverageArm { pattern, guarded, span }` を埋める。記録するのは `MatchOrigin::checks_coverage()` が真の `Explicit`／`FunctionGuard` だけ。
- 検査は `match_value` の中ではなく、関数ごとの `finish`（既定型の適用）の後に `Checker::check_coverage` で行う。
  リテラルの鍵に既定型の確定後の型が必要なため。`E1021` は記録順（到達順）で最初の非網羅の match だけを報告する。
- `W1003` は到達順ではなく `(span.source, span.start)` で安定ソートする。外側の match の後方の節が内側の match より先に報告され、
  関数の検査順もソース順と一致しないため。単相化・closure 変換は `warnings` をそのまま運ぶ。
- 正規化は `Row`（網羅の行）と `Query`（到達可能性の問い合わせ）の二つのモードを持つ。部分認識器と、結果のパターンが単独で網羅的でない
  全域認識器は `Row` では空（何も覆わない）、`Query` では `_`。結果のパターンがないか単独で網羅的な全域認識器は両方で `_`。
  AND は各側の正規化の組ごとの交差、`as` と型注釈は透過。一つの節の正規化が 1,024 通りを超えると `E1017`
  （"pattern alternatives exceed the compiler limit; split the match"）だが、通常は lowering の OR 展開の `E1017` が先に出る。
- usefulness は行列の特殊化で、`Rc` の永続的な行と明示的なスタックによる反復 DFS（行の `Drop` も反復）にし、
  ネイティブのスタック深さを入力に依存させない。完全なシグネチャは宣言順に分岐する（bool は `true`, `false`、union は case の宣言順、
  リストは `[||]`, `::`）。unit・タプル・レコードは単一の構成子、配列の長さとリテラルは常に不完全で既定行列へ進む。
- 資源制限はチケットの「行数 × 構成子数 > 1024」ではなく作業量の予算にした（部分問題ごとに行数 + 1、特殊化ごとに行数 × arity、
  上限 2^24、超過は `E1017` "the match is too large to check for exhaustiveness; split the match"）。
  「最初の列だけで決まる」行列で指数的な探索を避けるため、全列が `_` の行を含む部分問題は網羅済みとして打ち切る（行ごとの `concrete` 数）。
  25 列の bool の行列は debug ビルドでも約 1 秒で予算の `E1017` になる。
- リテラルの鍵は解決済みの型と正規形のテキスト。整数は `integer_literal` のビット、型が未確定の汎用整数は（絶対値, 負か）。
  f32／f64 は f64 のビット、f16／f128 は u128 のビット。符号は `Unary(Negate)` で反転し、±0 は同じ鍵、NaN は空のパターン
  （NaN のリテラル構文はないため防御的な扱い）。decimal は BID を復号して末尾の 0 を除き、cohort が同じ鍵になる
  （`tz_soft_cmp` は値で比較するため）。それ以外は一意の鍵で、重複の判定に使わない。
- `W1003` は `Query` の正規化のどの選択肢も、前にある `when` なしの節に対して有用でない節に、節のパターンの span で出す。
  OR の一部の側だけが到達不能な場合と、正規化が空の節（NaN のリテラルなど）は警告しない。ガード付きの節も到達不能なら警告する。
- 不足の例は探索の手順から組み立てるため、型から展開せず有限になる（A04 の再帰的な union でも visited 集合は不要）。
  case 名は match のモジュールから無修飾で同じ case に解決できれば無修飾、それ以外は `Module.Case`。
  payload は原子的でなければ括弧で囲む（`Some (Some false)`、レコードの payload も `Some ({ ok = false })`）。
  レコードは型名なしの `{ field = ... }`、配列の他の長さは `_`、リストは `[||]`／`_ :: _`。
- 関数ガードの `E1021` の位置は最初の節の `|`。bool 条件の節はガード付きの `_` と同じなので、`otherwise` がなければ `missing: _`。
- `tests/fixtures/control/Main.tz` の `trap_match` と `tests/control.mjs` の対応する trap 検査を削除した。
  `trap_pattern`／`trap_lambda`／`trap_for_active` は実行時トラップのまま残る。fixtures・examples・benchmarks は警告を出さない。
- テストは `tests/match_exhaustiveness.rs` に加え、分解の origin を `tests/control.rs::match_origin_destructuring`、
  警告の表示先を `tests/frontend.rs::warning_rendering_uses_source_path` に置いた（テスト計画のコマンド名に合わせた）。
  frontend の変異 fuzz の原文に union の match を追加した。
- 台帳の見直し提案はない。
