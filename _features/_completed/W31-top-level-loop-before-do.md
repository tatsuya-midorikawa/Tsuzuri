# W31: トップレベルのコードで、最初の `let!`・`do!` より前にループがあると `E1018` になる

| 項目 | 内容 |
| --- | --- |
| ID | W31 |
| 優先度 | P2（回避策はある: ループを最初の `do!` の後へ動かすか、`def main :: unit -> i32` に書く。スクリプトの形で、状態をループで作ってから `do!` で出す、という最も自然な書き方で当たる） |
| 規模 | S |
| 依存 | – |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（文書の規則「結果型が未定なら、最初の `let!` か `do!` の右辺の型から選ぶ」（`computation-expressions/computation-expressions.md` の「ビルダー名を省略する」）と `docs/language.md` の「未確定の場合はブロック内の最初の `bind` 式の右辺型から推論されます」に実装を合わせる修正。今 `E1018` で拒否しているプログラムの一部を受け入れるだけで、受け入れていたプログラムの意味・IR・診断は変えない。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `src/computation.rs` の `Checker::implicit_prefix`（ビルダーを決める前に、先頭の通常の文を検査して `prefix` に積む）、`direct_block`（直接実行の本体の `while`・`for`・`if` の下げ方）、`Checker::implicit_root` の `entry && builder == "IO"` の枝（先頭を検査した後で直接実行へ切り替える） |
| 主な影響ファイル | `src/computation.rs`（`direct_block`・`Checker::implicit_prefix`・`Checker::implicit_root`）、`tests/computations.rs`、`tests/computations.mjs`、`spec/rules/CTL.md`、`tests/conformance/CTL/`、`_tsuzuri/language-reference/computation-expressions/computation-expressions.md`、`_tsuzuri/language-reference/built-in-types-and-modules/io.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 式で終わるトップレベルのコード（以下「直接実行の入口」）。`Main.tz` のトップレベルに `let!`・`do!` などの文があり、最後の文が式のものを指す。チェッカーの中では `$implicit.root.$entry` の暗黙の計算である。その中で、最初のトップレベルの束縛（`let!`・`do!`・`match!`・`return!`・`and!`）より前に `while`・`for`・`if` の文があるものを直す。
- 原因: `src/computation.rs` の `Checker::implicit_prefix` は、ビルダーを決める前に先頭の通常の文（`let`・式の文・`let rec` の組）だけを検査し、`while`・`for`・`if` の文（`ComputationStatementKind::While`・`For`・`If`）で止まる。`Checker::implicit_root` は、止まった位置の文からビルダーを決めようとする。その文はビルダーを決める文ではないので、決められない。後ろの `do!` は見ずに `E1018` を返す。
- 直し方: 直接実行の入口で `implicit_prefix` が `while`・`for`・`if` で止まったら、ビルダーは `IO` だと仮定する。仮定の下では、その文から最初のトップレベルの束縛の手前までを `direct_block` と同じ下げ方で検査し、`prefix` に足す（最後の文は入口の値なので含めない）。その後、今と同じく束縛の右辺の型でビルダーを選ぶ。
  - `IO` なら、今の `entry && builder == "IO"` の枝で直接実行する。
  - `IO` でなければ、今と同じ `E1018`。
  - トップレベルの束縛が無ければ、直接実行する。
- 最初に読む: `src/computation.rs` の次の 4 か所。
  - `Checker::implicit_root` の `builder.is_none()` の枝と、`E1018` を返す fallback
  - `Checker::implicit_prefix` の `_ => break`
  - `direct_entry`
  - `direct_block` の `If`・`For`・`While` の枝
- 最初に書くテスト: `tests/computations.rs` の `entry_statements_before_the_first_bind_run_directly`。
- 触ってはいけない:
  - ビルダーで組み立てる計算の `E1018` の文・kind（`cannot_infer_builder`）・位置。対象は、`do!`・`return` で終わるトップレベルのコードと、結果型が未定のラムダ・ブロックである。
  - 今受け入れているプログラムの IR と診断。
  - `direct_block` が作る式。
- 完了の見分け方: 次のプログラムを `tsuzuri run` すると `n=3` と `3` を出して終了コード 0 で終わる（native の `-O0` と `-O3`）。

```tsuzuri
let mut n = 0
while n < 3 do
    n = n + 1
do! IO.writeln $"n={n}"
n
```

## 目的

文書は、結果型の無い本体のビルダーを「最初の `let!` か `do!` の右辺の型から選ぶ」と定める。トップレベルのコードが式で終わり、選んだビルダーが `IO` なら、`let!`・`do!` をその場で実行する（`docs/language.md` の「直接形式」）。ところが実装は、最初の `let!`・`do!` の前に `while`・`for`・`if` の文があると、その文で探すのをやめて `E1018`（cannot infer the computation builder）を出す。ループを `do!` の後へ動かすだけで通るので、利用者には理由が分からない。

直接実行の入口について、最初の束縛の前の制御の文を受け入れ、文書の規則に実装を合わせる。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W31`。「現状」の再現を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。
- [W02](W02-break-in-do-function.md) が `main` に入っているかを確かめる（「W02 との関係」）。入っていれば、その `runs_directly` と `check_implicit` の変更の上に作る。

### 停止条件

- **S1** 修正の後、今受け入れているプログラム（`tests/fixtures`・`examples`・`tests/conformance`・std の例）のどれかで、IR か診断が 1 byte でも変わる。
- **S2** ビルダーで組み立てる計算の、最初の束縛の前の制御の文が受け入れられる。または、その `E1018` の文・kind・位置が変わる（「仕様」の表）。
- **S3** 受け入れた形の生成コードが、native の `-O0` と `-O3` で違う結果を出す。または wasm32 の build が決定的でない。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 再現

```sh
mkdir -p target/scratch/w31/repro
# target/scratch/w31/repro/Main.tz に「実装者向け早見」のプログラムを書く
target/debug/tsuzuri.exe check target/scratch/w31/repro
#   ...Main.tz:1:1: error[E1018]: cannot infer the computation builder; add a result type annotation or use
#   Builder { ... }
```

- 最初の `do!` の前に `do! IO.writeln "start"` を足すと（ループが最初の束縛の後になると）、`check` は通る。
- 同梱の MinGW（`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe`）で `tsuzuri run` すると、`start`、`n=3`、`3` を出して終了コード 0。

### 形ごとの結果

`target/scratch/w-w31w32/` で確認し、確認後に消した。

| 形（トップレベルのコード） | 今の結果 | 修正後 |
| --- | --- | --- |
| 再現（`while` の後に `do!`、式で終わる） | `E1018`（1:1） | 受け入れる（直接実行） |
| `for i in 1 ..= 3` の後に `do!`、式で終わる | `E1018` | 受け入れる |
| 複数行の `if` 文の後に `do!` | `E1018` | 受け入れる |
| 1 行の `if n == 0 then n = 4` の後に `do!` | `E1018` | 受け入れる |
| `while` と `do!` の間に `let label = $"n={n}"` | `E1018` | 受け入れる |
| 本体に `do! IO.writeln name` を持つ `for` の後に、トップレベルの `do!` | `E1018` | 受け入れる（入れ子の `do!` も直接実行） |
| `while` の後に素の `do Debug.print "x"`、式で終わる（トップレベルの束縛が無い） | `E1018` | 受け入れる（直接実行） |
| `while` の中の `if n == 3 then break`、後に `do!` | `E1018` | W02 が入るまで `break` の位置で `E1023`、入った後は受け入れる |
| 本体に `let! v = Maybe.Some 1` を持つ `while` の後に、トップレベルの `do!` | `E1018` | `E1003`（入れ子の束縛は `IO.run` に下がる。`def main :: unit -> i32` の直接実行の本体に `Maybe` の `let!` を書いたときの `expected IO<...>, found Maybe<...>` と同じ。回復の検査で、ほかの診断が続かないかは仕様 4） |
| `while` の後に `let! x = Maybe.Some n`、式で終わる | `E1018` | `E1018`（変えない） |
| `while` の後に `do!` で終わる（`$implicit`。式で終わらない） | `E1018`（1:1） | `E1018`（変えない） |
| 結果型の無いラムダ `let f = \() -> ...` の本体で、`while` の後に `do!` | `E1018`（本体の位置） | `E1018`（変えない） |

今も受け入れている形（変わらない）:

- `do!` の後の `while`、式で終わる（直接実行）。
- `def main :: unit -> i32` の中の、`while` の後の `do!`（`direct_context`）。
- ループの本体にだけ `do!` がある形。トップレベルの文に束縛が無いので、トップレベルは計算ではない。ループの本体が自分で暗黙の計算の根になり、期待する型 `unit` で直接実行になる。
- `match`（`!` なし）の文と代入の文の後の `do!`（どちらも式の文で、`implicit_prefix` が検査する）。

### 原因（コード）

- 構文:
  - トップレベルは `src/parser.rs` の `Parser::entry` → `layout_block(0)` で読む。0 桁目の文に `do`・`return`・`yield`・`let!`・`match!`・`use!` があると、`implicit_computation_ahead(Some(0))` が真になる。その場合、全体を `$implicit` の計算として読む。
  - 計算の文は `computation_statement` が作る。行頭の `while` は `ComputationStatementKind::While`、`for` は `For`、`if` は（本体に `!` があってもなくても）`computation_if` の `If` になる。
- 入口の印: `src/check.rs` の `check_modules_collect` が、`computation::expand` の後に `computation::direct_entry` を呼ぶ。最後の文が式なら `$implicit.root.$entry` に変え、`checker.expression(&expression, None)` で検査する（期待する型は無い）。
- `src/computation.rs` の `Checker::implicit_root`:
  - `$entry` は `forced` から外す（`entry = true`）。期待する型も無いので、`builder` は `None` のまま `implicit_prefix` に進む。
  - `implicit_prefix` が止まった位置の文が、`Let(_, true)`・`LetAnd`・`Do`・`Match`・`Operation("ReturnFrom" | "YieldFrom")` のどれかなら、その右辺を検査して `implicit_source_builder` でビルダーを選ぶ。
  - どれでもなく、残りが空でも `return` 1 つでもなければ、`E1018`（`DiagnosticKind::CannotInferBuilder`、本体の位置）を返す。
  - ビルダーが `IO` で `entry` なら、`direct_root(prefix, &remaining, expected)` で直接実行する。
- `Checker::implicit_prefix`: `let rec` の組（`recursive_group`）、`Let(binding, false)`、`Expression` だけを検査する。ほかの文は `_ => break` で止まる。
- したがって、`implicit_prefix` が `While`・`For`・`If` で止まり、`builder` が `None` なら、結果は必ず `E1018` になる。この位置を変えても、今受け入れているプログラムには届かない。
- `E1018` の `cannot_infer_builder` を出すテストは、HEAD に無い（`grep -rn "cannot infer the computation builder" tests` が 0 件）。

## 変えてはいけない意味

- 受け入れていたプログラムの意味・IR・診断。修正は、今 `E1018` で拒否しているプログラムの一部を受け入れるだけである。
- ビルダーで組み立てる計算では、`while cond do C` は `B.While (\() -> cond) (B.Delay (\() -> C))` に下がる（`computation-expressions.md` の対応表）。ビルダーで組み立てる計算は、`do!`・`return` で終わるトップレベルのコードと、結果型が未定のラムダ・ブロックである。その中の制御の文を、通常のループとして先に走らせない。
- 評価の順序: 文は書いた順に評価する。最初の束縛の右辺は、その前のループの後に評価する（`$"n={n}"` は 3 を見る）。

## 仕様

1. 直接実行の入口（式で終わるトップレベルのコード）では、最初のトップレベルの束縛より前の `while`・`for`・`if` の文を、ビルダーが `IO` だという仮定の下で通常の文として検査する。下げ方は直接実行の本体と同じ（`direct_block`）。本体の中の `let!`・`do!` は `IO.run` に下がり、本体は `$implicit.root.$direct` になる。
2. 1 の後、最初のトップレベルの束縛の右辺の型で、今と同じ規則でビルダーを選ぶ。
   - `IO` なら直接実行する。今の `entry && builder == "IO"` と同じ結果である。
   - `IO` でなければ（`Maybe` など）、今と同じ文・kind・位置の `E1018`。
   - 候補が無いとき、または複数のときは、`implicit_source_builder` の今の `E1018`（`no_builder_for_value` などの文）になる。
3. 1 の文の後にトップレベルの束縛が無い場合（素の `do` だけの形など）は、直接実行する。
4. 1 の仮定の下で、入れ子の `let!`・`do!` の右辺が `IO` でないと、その右辺の位置で `E1003` になる。`def main :: unit -> i32` の直接実行の本体に `Maybe` の `let!` を書いたときと同じである（`computation-expressions.md` の「直接実行できるのは `IO` の束縛だけです」）。
   - トップレベルのコードは回復しながら検査する（`src/check.rs` の `check_modules_collect` の `checker.recovering = true`）。`Checker::finish_recovery` が `E1003` を記録し、その式を `TypedExpr::error` にして検査を続ける。
   - 後ろのトップレベルの束縛が `IO` なら、最初の診断はこの `E1003` で、`E1018` は出ないと見込む。束縛が `IO` でなければ、続いて `E1018` も記録される。
   - どちらも実装の後に確かめていない（未確認）。テストでは、診断の列（コードと位置）を固定する。
5. ビルダーで組み立てる計算の、最初の束縛の前の `while`・`for`・`if` の文は、今と同じ `E1018`（本体の位置、`cannot_infer_builder`、同じ文）。
6. 1 の制御の文の中の `break`・`continue` は、直接実行の本体のループと同じ扱いになる。W02 の前は `E1023`、W02 の後は受け入れる（「W02 との関係」）。

## 設計

`src/computation.rs`:

```rust
/// The ordinary expression for a `while`, `for` or `if` statement of a body that runs its IO binds
/// directly: its bodies run directly too (`$implicit.root.$direct`). `None` for other statements.
fn direct_control(statement: &ComputationStatement) -> Result<Option<Expr>, Diagnostic> {
    // direct_block の If・For・While の枝をそのまま移す（同じ式・同じ深さ）
}
```

- `direct_block` の `If`・`For`・`While` の枝は、`direct_control(statement)?` を呼ぶだけにする。出力は 1 byte も変えない（S1）。
- `Checker::implicit_prefix` に引数 `direct_controls: bool` を足す。
  - `false`（今の呼び出し）は、今と同じ動きのままにする。
  - `true` のときの違いは 2 つ。(a) `direct_control` が `Some` を返す文を、`Expression` と同じ `_: unit` の束縛として検査する。(b) 本体の最後の文は取らない。最後の文は入口の値である。
  - 束縛を作る文（`Let(_, true)`・`LetAnd`・`Do`・`Match`・`Operation`）では、今と同じく止まる。
- `Checker::implicit_root` の `builder.is_none()` の枝:

```rust
let count = self.implicit_prefix(body, &mut prefix, false)?;
remaining.statements.drain(..count);
// W31: top-level code that ends in an expression runs directly when its builder is IO, so the
// `while`, `for` and `if` statements before its first bind are checked as that code runs them.
let assumed_direct = entry
    && remaining.statements.first().is_some_and(|statement| {
        matches!(
            statement.kind,
            ComputationStatementKind::If(..)
                | ComputationStatementKind::For(..)
                | ComputationStatementKind::While(..)
        )
    });
if assumed_direct {
    let count = self.implicit_prefix(&remaining, &mut prefix, true)?;
    remaining.statements.drain(..count);
}
// （ここから最初の束縛の右辺でビルダーを選ぶ今のコード）
if assumed_direct && builder.is_none() {
    return self.direct_root(prefix, &remaining, expected);
}
```

- fallback の後の分岐は次のようにする。`entry && builder == "IO"` は今のまま `direct_root` へ進む。その後に `if assumed_direct { return Err(cannot_infer_builder(body.span)); }` を置く。仮定が外れたので、今と同じ `E1018` を返す。
- 文の重複を避けるため、`E1018` の診断は `fn cannot_infer_builder(span: Span) -> Diagnostic` にまとめ、fallback と共有する。
- `direct_root(prefix, &remaining, ...)` は今の `$entry` の経路と同じ。束縛の右辺は `$implicit.source` として `prefix` に積まれ、`remaining` の最初の文は `do! $implicit.source` になる。そのため、右辺はループの後に評価され、その直後に `IO.run` で実行される。
- `$entry` は `check_implicit` で `normal_loop_depth` を 0 にする（今のまま。W02 も変えない）。1 の制御の文の本体は `direct_control` が `$implicit.root.$direct` で包む。

### 採らない案

- **最初の束縛の右辺を、前の制御の文より先に検査する**: 右辺の評価が前に出て、評価の順序が変わる（再現の `$"n={n}"` が 0 を出す）。型推論の順序も変わり、ループの本体で決まる型（`let mut xs = []` など）を先に読むことになる。
- **制御の文を、ビルダーによらず通常のループとして先に走らせる**: ビルダーで組み立てる計算の `while` は `B.While` で、本体は `Delay` のクロージャになる。外の `let mut` への代入は `E1014` になり、利用者のビルダーの `While` は数える・記録するなどの意味を持てる。結果型からビルダーが決まる計算と、最初の束縛から決まる計算で、同じ文の意味と IR が食い違う。
- **チェッカーの状態を写して、両方の仮定で検査し直す**: `Checker`（`src/check.rs`）は 40 を超える状態を持ち、`Clone` でない（推論・局所の番号・診断・名前の使用・網羅性の記録）。範囲に比べて大きすぎ、検査の決定性も崩しやすい。
- **ビルダーで組み立てる計算でも、最初の束縛の型が分かるまで制御の文を保留する**: 制御の文は、ビルダーが決まらないと型付きの木を作れない（`B.While` か通常のループか）。上の 2 つのどちらかになる。
- **`E1018` の文を、制御の文を指す専用の文に変えるだけ**: 文書の規則（最初の束縛から選ぶ）と実装の食い違いが残る。

## 前提とする他チケットのインターフェース

- なし。

### W02 との関係

- 同じ `src/computation.rs` を触る。W02 の変更は 3 つ。`check_implicit` の `normal_loop_depth` の扱い、`Checker::implicit_root` の先頭の判定（`runs_directly`）、新しい `runs_directly`。このチケットの変更は 3 つ。`direct_block`（枝を `direct_control` へ移す）、`Checker::implicit_prefix`、`Checker::implicit_root` の `builder.is_none()` の枝とその後。
- 変える行は重ならない。`implicit_root` の先頭の判定（W02）と `let entry = ...` の後（W31）の間には、変えない行が残る。テキストの衝突は起きにくいが、後から入る方が rebase で確かめる。
- 意味の上では独立している。1 の制御の文の本体は `$implicit.root.$direct` なので、W02 の `runs_directly` がそのまま効く。W02 が入った後は、その中の `break`・`continue` を受け入れる。
- 推奨の順: W02 を先に入れる（Win32 のメッセージループの例が W02 に頼る）。どちらが先でもよい。
- 後から入る方の PR が、組み合わせの形をテストに足す。組み合わせの形は、トップレベルの `while true do` の中に `if n == 3 then break` があり、その後に `do!`、式で終わるもの。
- W02 の「対象外」の行（「トップレベルのコードで、最初の `let!`・`do!` より前にループがあると `E1018` になる」）は、このチケットを指す。W02 の適合性テストの注意（トップレベルで「ループの後に `do!`」と書かない）は、このチケットが入った後は要らなくなる。

## 他チケットへ提供するインターフェース

- なし。W06・W12 などのスクリプトの形の例は、トップレベルで状態をループで作ってから `do!` で出してよい。

## 実装手順

1. **変更**: ブランチ `fix/w31-entry-statements-before-first-bind`。基準を記録するだけ。
   **内容**: 着手条件を確かめる。HEAD のコンパイラで、`tests/fixtures`・`examples` の `Main.tz` を `--emit llvm -O1`（native と `--target wasm32`）にする。それを `check --json` の結果と一緒に `target/scratch/w31/before/` へ保存する。`node scripts/conformance.mjs target/debug/tsuzuri --area CTL` の件数を控える。
   **確認**: 保存した数を控える。
2. **変更**: `tests/computations.rs` に新しいテスト（テスト計画）を先に書く。
   **内容**: 受け入れる形は今 `E1018` で失敗し、変えない形は今のまま通ることを確かめる。
   **確認**: `cargo test --locked --test computations entry_statements_before` が `running 1 test` で失敗する。
3. **変更**: `src/computation.rs`（設計）。
   **内容**: `direct_control` を作り、`direct_block` から呼ぶ。`implicit_prefix` に `direct_controls` を足す。`implicit_root` の分岐を変え、`cannot_infer_builder` を作る。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test computations`・`--test control`・`--test specs` の全件。手順 1 の IR と `check --json` が `cmp` で一致する（S1）。
4. **変更**: E2E（`tests/computations.mjs`）と適合性テスト（`spec/rules/CTL.md`・`tests/conformance/CTL/`）。
   **内容**: テスト計画のとおり。
   **確認**: `node tests/computations.mjs target/debug/tsuzuri`。`node scripts/conformance.mjs target/debug/tsuzuri --rule CTL-0NN`（番号は着手時の次の番号）。
5. **変更**: 文書（下）。
   **確認**: 次の 3 つを通す。変更したページを渡した `node scripts/check-docs.mjs`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`。その後、T1 と T2。

## テスト計画

### Rust（`tests/computations.rs`）

`entry_statements_before_the_first_bind_run_directly`: 既存の `accepts`（利用者のビルダーの `.tc` を 5 つ置いたプロジェクトで解析し、native と wasm32 の IR を作り、決定的であることを見る）と `rejects` を使う。`IO.writeln` の右辺は、利用者のビルダーがあっても `IO` だけが具体的な候補なので、ビルダーの選び方は変わらない。

受け入れる:

- 早見の再現。`for i in 1 ..= 3 do` 版。複数行の `if` 文版と、1 行の `if n == 0 then n = 4` 版。
- `while` と `do!` の間に `let label = $"n={n}"` がある形。
- `let rec` の組、ループ、`do!` の順の形。
- 本体に `do! IO.writeln name` を持つ `for name in ["a", "b"] do` の後に、トップレベルの `do! IO.writeln "done"` があり、`0` で終わる形。
- `while` の後に素の `do Debug.print "x"` があり、`n` で終わる形（束縛なし）。
- `while` の後に `let! base = IO.pure 10` があり、`n + base` で終わる形。
- 入れ子のループ（外の `for` と内の `while`）の後に `do!` がある形。

拒否する:

- `while` の後に `let! x = Maybe.Some n` があり、`x` で終わる形 → `E1018`。今と同じ文で、位置は 1:1。
- `while` の後に `do!` で終わる形 → `E1018`（今と同じ）。
- 結果型の無いラムダ（`def main :: unit -> i32` の中の `let f = \() -> ...`）の本体で、`while` の後に `do!` がある形 → `E1018`（今と同じ位置）。
- 本体に `let! v = Maybe.Some 1` を持つ `while` の後に、トップレベルの `do!` がある形 → `E1003`。位置は入れ子の `let!` の右辺。
- W02 が入っていないとき: 再現の `while` の中の `if n == 3 then break` → `E1023`。W02 が入っていれば受け入れる（「W02 との関係」）。

既存の `implicit_computations_reject_ambiguity_and_preserve_safety_limits`、`tests/control.rs`、`tests/specs.rs` の検査は、そのまま通ること。

### E2E（`tests/computations.mjs`）

一時プロジェクトを `tsuzuri run` で `-O0` と `-O3` の両方で動かす。期待値は手で書く（GUIDE-LITE §2 規則 10）。

| 内容 | 期待 |
| --- | --- |
| 早見の再現 | 標準出力 `n=3\n3\n`、終了コード 0 |
| `for name in ["a", "b"] do` の本体に `do! IO.writeln name`、その後に `do! IO.writeln "done"` と `0` | 標準出力 `a\nb\ndone\n0\n` |
| `if` 文で `let mut n` を 4 にしてから `do! IO.writeln $"n={n}"` と `n` | 標準出力 `n=4\n4\n` |

- 再現を `--target wasm32 -O3` で 2 回 build し、bytes が同じであることを確かめる。

### 適合性テスト

- `spec/rules/CTL.md` に次の番号の規則を足す。文: 「In top-level code that ends in an expression, `while`, `for` and `if` statements before the first `let!` or `do!` MUST run in place, in order, when that bind is an `IO` action.」
- `tests/conformance/CTL/CTL-0NN-statements-before-first-bind.tz`（早見の再現。`// expect: stdout "n=3\n3\n"`）。
- 拒否の形（`while` の後に `let! x = Maybe.Some n` で `E1018`）は、適合性テストにしない。今の制限（D2）を仕様の規則として固定しないためで、`tests/computations.rs` だけで確かめる。
- 文書の規則の文の直前に `<!-- rule: CTL-0NN -->` を置く（`computation-expressions.md` の「選び方」）。

## ドキュメント

- `_tsuzuri/language-reference/computation-expressions/computation-expressions.md`:
  - 「選び方は次の順です」（240 行目付近）の 2 の後に、次を足す。式で終わるトップレベルのコードは、最初の `let!`・`do!` より前に `while`・`for`・`if` の文があっても、その束縛が `IO` ならその場で実行する。ビルダーで組み立てる計算（`do!` で終わるトップレベルのコード、結果型の無いラムダなど）では、最初の束縛より前に制御の文を書けない。その場合は `E1018` で、結果型を書くか `Builder { ... }` を使う。
  - 直接実行の段落（260 行目付近）に、トップレベルの直接形式を 1 文で書く。
  - `run=n%3D3%0A3` の例（早見の再現）を 1 つ足す。
- `_tsuzuri/language-reference/built-in-types-and-modules/io.md`: 「終了コードとトップレベルの表示」（238 行目付近）の箇条に 1 文足す。内容は、トップレベルの `let!`・`do!` の後に式で終える形は、IO をその場で実行し、最初の `let!`・`do!` の前にループを置けること。
- `docs/language.md`:
  - 「直接形式」の段落（1273 行目付近）に同じ 1 文を足す。
  - ビルダーの選び方（2496 行目付近）に、最初の束縛の前の制御の文の扱いを 1 文で足す。
- README は変えない。`compiler/diagnostics.md` の `E1018` の表は、kind を足さないので変えない。

## 受け入れ条件

- [ ] 「現状」の表の「受け入れる」の形が、すべて受け入れられる。「変えない」の形は、すべて同じ文・kind・位置の診断のまま。
- [ ] `tests/computations.rs` の新しいテストと、`tests/computations.rs`・`tests/control.rs`・`tests/specs.rs` の既存の全件が通る。
- [ ] `node tests/computations.mjs target/debug/tsuzuri` が通る。早見の再現の `tsuzuri run` が `n=3`・`3` と終了コード 0 を出す（`-O0`・`-O3`）。
- [ ] 今受け入れているプログラム（`tests/fixtures`・`examples` のすべての `Main.tz`）の IR・診断が、1 byte も変わらない（手順 1 との比較）。
- [ ] 適合性テスト（`CTL-0NN`）、`check-spec --require-markers`、変更した全ページの `check-docs` が通る。
- [ ] T2 が通る。

## 落とし穴

- **最後の文を取らない**: `implicit_prefix` は、式の文を `_: unit` の束縛として検査する。仮定の下でも同じにすると、入口の値（`n`）を `unit` として検査してしまう。その結果、`let mut n = 0` の `0` の位置に `expected an integer, found unit` が出る。HEAD でも、素の `do` を含み束縛の無いトップレベルの計算で、この形が起きる（「対象外」。[W38](W38-top-level-plain-do-misplaced-e1003.md) が直す）。`direct_controls` のときは、最後の文で止める。
- **`$entry` だけ**: `direct_controls` を `$implicit`（`do!` で終わるトップレベル）や、結果型の無いラムダに広げない。ビルダーで組み立てる計算になり、`while` は `B.While` でなければならない（「採らない案」）。
- **仮定が外れたときに検査をやり直さない**: 制御の文は、すでに直接実行として検査し、局所と推論の状態を残している。ビルダーが `IO` でなければ、そこで `E1018` を返して終える。別のビルダーで検査し直そうとしない。
- **`direct_block` の出力を変えない**: `direct_control` へ移すのは純粋な移動にとどめる。手順 1 の IR の比較で確かめる（S1）。
- **入れ子の束縛**: 仮定の下では、制御の文の本体の `let!`・`do!` が、トップレベルの束縛より先に検査される。`IO` でなければ `E1003` が先に出る（仕様 4）。テストで位置を固定する。

## 対象外

- ビルダーで組み立てる計算（`do!`・`return` で終わるトップレベルのコード、結果型の無いラムダ・ブロック）で、最初の束縛の前に制御の文を書けるようにすること（「採らない案」）。
  - 書けても、その `while` は `B.While` で、本体から外の `let mut` へ代入すると `E1014` になる。HEAD でも、`do!` で終わるトップレベルのコードで、`do!` の後の `while` が `n = n + 1` を持つと `E1014` になる。
  - その場合に `E1018` の文へ `help` を足すことも、診断の基準を動かすので別にする。
- **素の `do` を含み束縛の無いトップレベルの計算の、最後の式**: 例は `let mut n = 0`、`do Debug.print "x"`、`n` の 3 行である。素の `do` があるとトップレベルは計算として読まれる。`implicit_prefix` が最後の `n` を `_: unit` の束縛として検査し、`0` の位置に `E1003`（expected an integer, found unit）が出る。最後の文が `unit` の形（`do Debug.print "x"` で終わる）は今受け入れており、その IR を変えずに直す方法を別に決める必要があるので、このチケットでは直さない。[W38](W38-top-level-plain-do-misplaced-e1003.md) が直す（同じ `implicit_root` の枝を触るので、W38 の「W31・W02 との関係」がマージの順と組み合わせのテストを決めている）。
- `break` に値やラベルを付けること。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 式で終わるトップレベルのコードでは、最初の束縛より前の `while`・`for`・`if` の文を、ビルダーが `IO` だという仮定の下で直接実行の文として検査する。その後、最初の束縛の型で確かめる（`IO` なら直接実行、そうでなければ今と同じ `E1018`） | 文書の規則（最初の束縛の型から選ぶ）を保ったまま、検査の順と評価の順を書いた順に保てる。直接実行の入口でビルダーが `IO` のとき、制御の文の意味は直接実行と同じで、型付きの木も `direct_block` と同じになる | 既定案（実装者はこの案に従う） |
| D2 | ビルダーで組み立てる計算の、最初の束縛の前の制御の文は、今と同じ `E1018`（文・kind・位置を変えない） | 制御の文の型付きの木はビルダーが決まらないと作れない。先に検査すると評価の順か `While` の意味が変わる（「採らない案」） | 既定案（実装者はこの案に従う） |
| D3 | 仮定の下の入れ子の `let!`・`do!` は `IO.run` に下げ、`IO` でなければその位置で `E1003` | `def main :: unit -> i32` の直接実行の本体と同じ規則で、文書（直接実行できるのは `IO` の束縛だけ）と一致する | 既定案（実装者はこの案に従う） |
| D4 | 制御の文の後にトップレベルの束縛が無ければ、直接実行する | 確かめるべき束縛が無く、入れ子の束縛はすでに `IO` として検査した。トップレベルが計算でない形（ループの本体にだけ `do!`）と同じ結果になる | 既定案（実装者はこの案に従う） |
| D5 | 規則 `CTL-0NN`（着手時の次の番号）と、受け入れる形の適合性テスト 1 つを足す。拒否の形は規則にしない | ビルダーの選び方は文書の規則で、実装が外れたことを CI（`conformance.mjs`）で捕まえるため（D-58 の形式。W02 の D4 と同じ）。D2 の拒否は今の制限で、将来広げうるので、仕様の規則として固定しない | 既定案（実装者はこの案に従う） |
| D6 | W02 とはどちらが先でもよく、W02 を先に入れることを推奨する。後から入る方が、`break` を含む組み合わせの形をテストに足す | 意味は独立していて、変える行も重ならない。組み合わせは両方が入って初めて受け入れられる | 既定案（実装者はこの案に従う） |
