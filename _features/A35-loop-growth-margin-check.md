# A35: 実際のプログラムの収束するループの借用の増え方が A31 のループの打ち切り（`LoanGrowth`）の上限に近づいたら、テストを失敗させる

| 項目 | 内容 |
| --- | --- |
| ID | A35 |
| 優先度 | P2（今の利用者のプログラムの結果は変わらない。後の所有権の検査の変更が実際のループを打ち切りに近づけたとき、安全なプログラムが `E1017` で拒否される前に、その変更の CI で気づくための回帰テスト） |
| 規模 | M（集める側は小さい: `LoanGrowth` の数、`check_loop` の 1 か所、`Checker` の 1 フィールド、テスト用の関数。3 つのコーパスを読むテスト、今の値とテストの時間の記録、文書がある） |
| 依存 | A31 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-16（A31（PR #105）の打ち切り `LoanGrowth`（`93f25d12`・`1506be7e`・`f88d8a82`）の後。2026-11-15 に利用者が受け入れた後続。受け入れたときの案は 1 つの上限（`LOOP_LOAN_GROWTH`）だけを比べるものでしたが、打ち切りが借用の増え方の形で止めるようになったので、そのすべての上限に合わせました。A31 の「借用の数の上限を外す（2026-11-16）」で `LOOP_LOAN_GROWTH`・`LOOP_LATE_GROWTH`・`LOOP_GROWTH_FROM` を外したので、比べるのは増え方の上限だけです） |
| 承認 | 不要（テストと、テストだけが呼ぶ内部の関数を足すだけで、検査の結果・診断・生成コード・言語の意味は変えない） |
| 改善する劣位 | 検証の欠落: A31 の打ち切りの上限は、リポジトリの外の計測用のビルドと記録（A31 のワークツリーの `target/scratch/a31/lp-budget/`・`lb-fix1/`・`lg-fix/`）で一度だけ決めたもの。後の変更が実際のループの借用を長く・速く増やすようにしても、そうしたループを含むテストが無ければ CI は通り、利用者の安全なプログラムが `E1017` で拒否されて初めて分かる |
| 手本にする既存実装 | `src/ownership.rs` の `copy_reads`・`events_abstract`（`check_body` に集める先 `copies`・`events` を渡し、検査そのものは変えない）、`src/backend/mod.rs` の `layout_of`（テストだけが呼ぶ `#[doc(hidden)]` の公開関数）、`tests/capabilities.rs` の `corpus`・`corpus_table`（`tests/fixtures` と `examples` の `Main.tz` のあるプロジェクトを 4 つのスレッドで解析する）、`tests/std_examples.rs`（`stdlib::SOURCES` と `doctest` から std の例を集める） |
| 主な影響ファイル | `src/ownership.rs`, `src/ownership_control.rs`, `tests/loop_growth.rs`（新規）, `tests/common/`（`tests/rc.rs` の形の生成関数を共有するなら）, `docs/architecture.md`, `README.md`（変更の種類別の表）, `_features/README.md` |

## 目的

A31 は、収束しないループの所有権の検査を `E1017`（`loop_ownership_limit`）で止める打ち切り（`src/ownership_control.rs` の `LoanGrowth`）を入れました。上限は 3 つの定数（増え方の連続の 2 つと、その片方を早める借用の数）で、どれも「実際のプログラムの収束するループは、この形では増えない」という計測から、届くのが実際のプログラムが書かない形だけになるように選んであります。その計測はリポジトリの外の道具で一度だけしたものです。後の変更（ループの要約が比べる事実を増やす、借用の作り方を変える、新しい種類の loan を足すなど）が実際のループの反復を増やしたり、反復ごとの借用を増やしたりしても、CI はそれを見ません。上限を超えるまで何も失敗せず、超えたときに初めて、安全なプログラムが `E1017` で拒否されます。

このチケットは、収束したループの借用の増え方の統計（決定的な数で、時間ではありません）をテストだけが読めるようにし、`tests/` のプログラム・`examples/`・std の例で、その最大が上限の決まった割合（反復の数の上限には 1/2。`LOOP_GROWING_LARGE` の借用の数は 1/8 で数える）に近づいたら失敗する Rust のテストを足します。統計の今の値はテストとこのチケットに記録します。

利用者にとっては、所有権の検査の変更が安全なプログラムを `E1017` にする前に、その変更の PR の CI で分かるようになります。

## 背景

- 打ち切りのいきさつは A31 のチケットの「借用の数による打ち切り（2026-11-15）」「借用の増え方による打ち切り（2026-11-16）」「最後のレビュー（2026-11-16）」にあります（[A31](_completed/A31-in-place-writes-keep-stored-borrows.md)）。打ち切りは検査を緩めず、`E1017` を足すだけです。上限を超える形は、実際のプログラムが書かない形として `lifetimes.md` の「現在の制限」と `docs/architecture.md` のループの不動点の段落に書いてあります。
- A31 の作業中、打ち切りが安全なループを止めていたことは、どれもレビューの探査（リポジトリの外の道具）で見つかりました。借用の数だけを比べた `1b21f734` は、一度だけ広がるインタプリタのループ（探査の 38 個のディレクトリ）を止めていました。`1506be7e` は、2 回目の反復で 500 個を超えて広がった後に数個ずつ増えるループ（22 個）を止めていました。`f88d8a82` も、状態の履歴を持つ状態機械（15 個）を止めていたので、A31 は借用の数の上限を外しました。リポジトリのテスト（`tests/rc.rs` の `a_loop_` の 6 件）は、そうして見つけた形の結果を固定していますが、`tests/fixtures`・`examples`・std のループが上限からどれだけ離れているかは測っていません。A31 がマージされてワークツリーを消すと、計測の道具も無くなります。
- AGENTS.md の「Do not add speed pass/fail thresholds to shared CI runners」は当てはまりません。このテストが比べるのは、検査が作った借用の数と反復の数で、機械や負荷で変わりません。

## 着手条件と停止条件

- A31 が `main` に入っていること（`grep -nE "^\| A31 " _features/README.md` が done で、`src/ownership_control.rs` に `LoanGrowth` があること）。着手時の `main` の定数と `LoanGrowth::note` を「現状」と比べ、違えば、このチケットを直してから始める。

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 今の値が、すでに「仕様」の割合に届いている（たとえば、`examples/` に、増え方が 3 回続けて速くなるループや、前の反復より 63 個多く作るループがある）。割合を緩めず、そのループと値をこのチケットに記録して報告する（上限を決めた計測の前提が崩れている）。
- 統計を集めることで、検査の結果（受理・拒否、診断、`--emit llvm` の IR）が変わる。
- テストが、開発用のビルドの 4 つのスレッドで 2 分を超える。コーパスを削る前に報告する。

## 現状

A31 の先端（ブランチ `fix/a31-in-place-writes-keep-borrows`。借用の数の上限を外したコミット）で確かめました。行番号はそのコミットのものです。

### 打ち切りと上限（`src/ownership_control.rs`）

`Checker::check_loop`（647 行）は、反復ごとに、条件と本体の検査の前後の `self.loans.len()` の差をその反復が作った借用の数とし、収束しなかった反復ごとに `LoanGrowth::note`（144 行）へ渡します（703 行）。要約が前の反復と同じなら（695 行の `self.loop_summary() == before`）、その反復は `note` に渡さずに `Ok` を返します。`note` が止める条件は次の 3 つの定数で決まります（定数はどれも非公開）。借用の数そのものの上限はありません。

| 定数 | 行 | 値 | 止める条件 |
| --- | --- | --- | --- |
| `LOOP_FASTER_ITERATIONS` | 107 | 5 | 5 回続けて、前の反復より多く、しかも前の反復が増えた数より多く借用を作った（`LoanGrowth::faster`） |
| `LOOP_GROWING_ITERATIONS` | 123 | 24 | 24 回続けて、前の反復より多く、前の反復が増えた数以上に作った（`LoanGrowth::growing`） |
| `LOOP_GROWING_LARGE` | 132 | (8, 200) | 同じ数え方で 8 回続き、しかもその反復が 200 個を超えて作った |

`LoanGrowth`（82 行）は、前の反復の数とその前から増えた数（`last`）と、2 つの連続（`faster`・`growing`）だけを持ちます。止めずに収束したループのこれらの値は、どこにも残りません。

確かめ方: `grep -nE "^const LOOP_|^struct LoanGrowth|fn note|fn check_loop|growth.note|loop_summary\(\) == before" src/ownership_control.rs`。

### 上限の決め方と、その計測が残っていないこと

| 上限 | 決めた回 | 計測の道具（リポジトリの外） | 実際のプログラムの値 | 上限が止める形の値 |
| --- | --- | --- | --- | --- |
| `LOOP_FASTER_ITERATIONS`・`LOOP_GROWING_ITERATIONS`・`LOOP_GROWING_LARGE` | 借用の増え方による打ち切り（`93f25d12`・`1506be7e`） | `lb-fix1/bin/tsuzuri-instr3`（`TZ_LB_TRACE` で反復ごとの借用の数を書き、`TZ_LB_NOSTOP` で止めない）と、記録から規則を試す `sim.mjs` | 探査の 1458 組の記録の、収束した 424,031 回のループの検査で、速くなる増え方の連続は多くても 1 回、前以上の増え方の連続も 1 回（レビューが作った、参照を局所値に写していく `hr_*` の 10〜30 回を除く） | `loopplace` の倍になる形は 5 回続けて速くなり、局所値に渡す形は 24 回（200 個を超える反復なら 8 回）続けて増える |

借用の数の上限を外した後の 1909 組の記録（2026-11-16。止めずに取った記録なので、収束したループの値は今の打ち切りのものと同じです）でも、収束したループの速くなる増え方の連続は、レビューが作った、2 つか 4 つのフィールドのどれかを指す参照の連なり（`bc_if_*`・`fc4_*`。打ち切りは 7 個以上の `bc_if` を止め、`fc4_5` は 4 回）を除いて多くても 1 回、前以上の増え方の連続は、レビューが作った形（参照を写していく局所値の `hr_*`・`cc_*`、戻り先や履歴を局所値に順に持つインタプリタの `vmbt_*`・`vmr_*`・`pipe_5_8_80_last`。多くても 30 回）を除いて多くても 2 回です。最初の計測にあった後の反復が最初の反復より多く作った借用の数（gate 3、`tests/fixtures` と `examples` 2、std の例 0）は、借用の数の上限だけが比べていたので、もう比べません。

A31 の先端の `src/` は、`15d84aaf` と `src/ownership_control.rs` の打ち切りと `src/explain/codes/E1017.md` だけが違うので、収束するループが作る借用の数は、上の計測のときと同じです。

### 今あるテストが見ないもの

`tests/rc.rs` の `a_loop_` の 6 件は、上限のすぐ内側と外側に作った形の結果を固定しています（参照を写していく 24 個と 25 個の局所値、200 個の借用を足した 8 個と 9 個、`loopplace` の形の打ち切り、一度広がるインタプリタ・`widening`・状態機械と、増え方が小さくなっていく `pipeline(16, 2, 4)`・`pipeline(5, 8, 20)` の受理）。これらは、実際のプログラムのループが上限からどれだけ離れているかを測りません。たとえば、ループの要約が比べる事実を増やす変更（A31 の Copilot のレビュー 2 の修正がそうでした）がどのループの収束も 1〜2 回遅らせ、遅れた反復が少しずつ借用を増やすなら、`tests/fixtures` や `examples` のループは増え方の連続に近づきます。それでも、上限に合わせて作った形の結果は変わらないことがあり、そのときは利用者のプログラムが上限を超えるまで何も失敗しません。

## 仕様

### 統計

テストだけが呼ぶ関数が、1 つの解析（`CheckedModule`）の所有権の検査が走らせた、収束したループの検査（`check_loop` が `Ok` を返したもの）すべてについて、次の値の最大を返します。打ち切りで止めたループと、反復の上限に届いたループは数えません。値はどれも `LoanGrowth` が数えるものと同じ定義です。

| 統計 | 定義 | 比べる上限 |
| --- | --- | --- |
| `faster` | `LoanGrowth::faster` の最大（速くなる増え方の連続） | `LOOP_FASTER_ITERATIONS` |
| `growing` | `LoanGrowth::growing` の最大（前以上の増え方の連続） | `LOOP_GROWING_ITERATIONS` |
| `growing_large` | その反復が `LOOP_GROWING_LARGE` の借用の数の 1/8（25 個）を超えて作ったときの `LoanGrowth::growing` の最大 | `LOOP_GROWING_LARGE` の反復の数 |
| `iterations` | 収束した反復を含む反復の数の最大 | なし（記録だけ） |

`iterations` は、どの打ち切りも反復の数だけでは止めないので比べず、記録して出力するだけにします。

統計は、最大を取った所（解析した入力、関数、ループの本体の位置）と、そのループの反復ごとの借用の数も持ち、テストの失敗の文はそれを出します。

### 失敗する条件

テストは、統計がどれか 1 つでも次の値に届けば失敗します。上限はテストに書かず、テストだけが呼ぶ関数から読むので、上限を変えれば割合も一緒に動きます。割合はテストの定数です。

| 統計 | 割合 | 失敗する値（今の上限で） |
| --- | --- | --- |
| `faster` | 1/2 | `LOOP_FASTER_ITERATIONS` の半分以上、つまり 3 以上 |
| `growing` | 1/2 | 12 以上 |
| `growing_large` | 1/2 | 4 以上 |

整数の比べ方は「値 × 分母 ≥ 上限 × 分子」です（5 の半分は 2.5 なので 3 で失敗します）。

コーパスのプログラムが `loop_ownership_limit` で拒否されたら、それも失敗にします（実際のプログラムが打ち切りに届いた）。ほかの理由で拒否されるプログラム（`tests/conformance` の拒否の例、`tests/diagnostic_baseline`）は数えません。

### 分かっている今の値

A31 の計測から分かっている値です（借用の数の上限を外した 2026-11-16 の記録まで）。手順 1 で、このチケットのコーパスとテストの関数で測り直して記録します。

| 統計 | 失敗する値 | 分かっている値 |
| --- | --- | --- |
| `faster` | 3 | 多くても 2（どのループも 3 回以内の反復で収束するので。探査の 1909 組の記録では、レビューが作った `bc_if_*`・`fc4_*` を除いて 1） |
| `growing` | 12 | 多くても 2（探査の記録でも、レビューが作った `hr_*`・`cc_*`・`vmbt_*`・`vmr_*`・`pipe_5_8_80_last` を除いて 2） |
| `growing_large` | 4 | 多くても 2 |
| `iterations` | – | 3 |

### コーパス

- `tests/fixtures` と `examples` の、`Main.tz` のあるプロジェクト（`tests/capabilities.rs` の `corpus` と同じ集め方。A31 の時点で 155 個）。マニフェストの edition で解析する。
- `tests/` の下の、プロジェクトに入っていない `.tz` のプログラム（`tests/conformance/**`、`tests/diagnostic_baseline/**`。拒否されるものは数えない）。
- std の例（`tsuzuri test --doc std` が実行する例。`tests/std_examples.rs` と同じく `stdlib::SOURCES` と `doctest` から集める）。

どの入力の解析にも std の関数の本体の検査が入るので、std の関数のループもここで測られます。`tests/` の Rust のテストに文字列で書いたプログラム（`tests/rc.rs` の上限の形を含む）は、コーパスに入れません（「対象外」）。

## 設計

### 集める側（`src/ownership_control.rs`・`src/ownership.rs`）

- `LoanGrowth` に、そのループの統計の最大（`faster`・`growing`・`growing_large`）と、集めるときだけ反復ごとの借用の数を足す。`note` は、今の数え方のあとでそれを更新する（`growing_large` は `made * 8 > LOOP_GROWING_LARGE.1` の反復の `growing`）。止める条件と `help` の文は変えない。
- `Checker` に `growth_stats: Option<SettledLoopGrowth>` を足す（`stores`・`dropped` と同じく、呼んだ側が求めたときだけ `Some`）。`check_loop` は、収束して `Ok` を返す所で、`Some` なら `LoanGrowth` の値と反復の数とループの本体の位置を合わせる。`None` のときの費用は、収束したループごとの分岐 1 つです。
- テストだけが呼ぶ公開の関数（`#[doc(hidden)]`。D1）:

```rust
/// A35: the largest growth of the loans of the loops that settled in the ownership check of
/// `module` (`LoanGrowth`), with where it was reached. Read by `tests/loop_growth.rs` only.
#[doc(hidden)]
pub fn settled_loop_growth(module: &CheckedModule) -> SettledLoopGrowth;

/// A35: the limits of the `LoanGrowth` stops, so that the test follows them.
#[doc(hidden)]
pub fn loop_growth_limits() -> LoopGrowthLimits;
```

  `SettledLoopGrowth` は統計ごとに値・関数の番号と名前・ループの本体の `Span`・反復ごとの借用の数を持ち、`LoopGrowthLimits` は 3 つの定数を持つ。同じ値のときは先に見つけた方を残す（関数の番号順、検査の順）。
- `settled_loop_growth` は `copy_reads` と同じく、`call_facts` の後で関数ごとに `check_body` を順に（並列にせず）呼ぶので、`TSUZURI_THREADS` やスレッドの数に依らない。`call_facts` の不動点の検査（`check_facts`・`drops_stop_reads`）も打ち切りで止まりうるので、そこも数える（D2）。
- `check_functions` は、リリースビルドでは借用を扱わない本体（`ownership_free`）を飛ばします。そうした本体のループは借用を作らないので統計を変えませんが、`settled_loop_growth` はプロファイルに依らず同じ本体を検査する。

### テスト（`tests/loop_growth.rs`）

- `settling_loops_of_the_corpus_stay_far_from_the_loop_stops`: コーパスの入力を `tests/capabilities.rs` の `corpus_table` と同じく 4 つのスレッドで解析し、`settled_loop_growth` の結果を、入力の順（パスの順）に最大で合わせる（同じ値なら先の入力。スレッドの順に依らない）。「失敗する条件」で比べ、成功しても統計を出力する（`eprintln!`）。失敗の文は、統計の名前・値・割合・上限、入力・関数・位置、そのループの反復ごとの借用の数と、「所有権の検査の変更が実際のループを A31 の打ち切りに近づけた。割合や上限を変える前にそのループを見て、正しいなら A31 のチケットの上限の決め方に沿って較正し直し、`docs/architecture.md` とこのテストに記録する」を出す。
- 今の値は、テストの doc comment に、測った日とコミットとともに書く（値が変わってもテストは失敗させない。比べるのは割合だけ）。

## 実装手順

### 手順 1: ベースラインと記録

- 変更: なし（`target/scratch/a35/` の下書きだけ）。
- 内容: `node scripts/repo-kb.mjs brief A35`。着手時の `main` の定数と `LoanGrowth::note` を「現状」と比べる。コーパスを今の `analyze` だけで解析する下書きのテストで、入力の数と時間（開発用のビルド、4 つのスレッド）を測る。`tests/fixtures`・`examples` の `check --json` と `--emit llvm`（native と `--target wasm32`）を保存する。
- 確認: 時間が 2 分以内。拒否された入力の内訳（`loop_ownership_limit` が 0 件）を記録する。

### 手順 2: 集める側

- 変更: `src/ownership_control.rs`（`LoanGrowth` と `check_loop`）、`src/ownership.rs`（`Checker` のフィールド、`settled_loop_growth`、`loop_growth_limits`、不動点の検査への集める先の受け渡し）。
- 確認: T0。手順 1 で保存した `check --json` と IR が `cmp` で一致する。`cargo nextest run --locked` の全件が成功する。

### 手順 3: 統計そのもののテスト

- 変更: `tests/loop_growth.rs`（必要なら `tests/common/` へ `tests/rc.rs` の形の生成関数を移す）。
- 内容: 「テスト計画」の `the_statistic_counts_what_the_stops_count` と `the_margins_fail_at_their_fractions`。
- 確認: 2 件が成功する（`running 2 tests`）。

### 手順 4: コーパスのテストと今の値

- 変更: `tests/loop_growth.rs`。
- 内容: `settling_loops_of_the_corpus_stay_far_from_the_loop_stops`。出力した統計を、テストの doc comment と、このチケットの末尾の記録に書く。
- 確認: 成功し、値が「分かっている今の値」と矛盾しない（矛盾すれば、どのループかを調べて記録する。割合に届いていれば停止条件）。2 回続けて実行して同じ出力になる。

### 手順 5: 割合が効くこと

- 変更: なし（手元で試して戻す）。
- 内容: 上限を 1 つずつ、今の値で割合に届く値まで下げてテストを実行する（たとえば `faster` が 1 なら `LOOP_FASTER_ITERATIONS` を 2 に、`growing_large` が 2 なら `LOOP_GROWING_LARGE` の反復の数を 4 に）。
- 確認: そのたびにコーパスのテストがその統計の名前で失敗する。試した値をこのチケットに記録し、上限を戻す。

### 手順 6: ドキュメントと最終確認

- 「ドキュメント」の節のとおり。T2。

## テスト計画

### Rust テスト

- `tests/loop_growth.rs`（新規）:
  - `settling_loops_of_the_corpus_stay_far_from_the_loop_stops`（「設計」）。
  - `the_statistic_counts_what_the_stops_count`: 増え方の分かっている形で、統計が `LoanGrowth` の数え方と同じ値になること。閉じた形と A31 の記録から求めた値（実装のときに手で数え直す）: `tests/rc.rs` の `a_loop_whose_borrows_keep_growing_stops_with_e1017` の 12 個の連なり（反復 `k` が最初より `k(k−1)/2 + k(12−k)` 個多く、反復 12 で収束）は `faster` 1、`growing` 1、`iterations` 13。`keeping_interpreter(20, 4, 0, 4)`（180・716・720・724・728 個）は `faster` 1、`growing` 2。`state_machine(16, 72, 6, 2)`（105・693・735・775・813・849・883 個）と `pipeline(16, 2, 4)`（71・139・…・671 個）は `faster` 1、`growing` 1。`copy_chain(n, 0)`（1 個ずつ増え、反復 `n` で収束）は `growing` が `n − 1`。打ち切りで止まる形（`late_hand_on(40)` など）は統計に入らないこと。
  - `the_margins_fail_at_their_fractions`: 比べる関数を、上限を引数に取る純粋な関数にし、境界の値で確かめる（`faster` 2 は成功・3 は失敗、`growing` 11・12、`growing_large` 3・4。上限は `loop_growth_limits` から）。
- 既存のテスト: 変わらない（停止条件）。`tests/rc.rs` の `a_loop_` の 6 件は、打ち切りの境界を固定するテストとしてそのまま残す。

### E2E

- 無し（検査の結果も生成コードも変えない）。手順 2 の `check --json` と IR の比較で確かめる。

### 性能

- `settled_loop_growth` を呼ばない解析の所有権の検査の時間が、揺れの範囲で変わらないこと（`TSUZURI_TIME_PASSES=1` の `ownership` の段を、A31 の `perf.sh` の 12 個のプログラムで、入れる前と後に交互に測る）。CI に速度の合否は入れない。
- コーパスのテストの時間（開発用のビルド、4 つのスレッド）を記録する。

## ドキュメント

- `docs/architecture.md` のループの不動点の段落（`LoanGrowth` の説明の後）: 収束したループの統計、`tests/loop_growth.rs` が比べる割合、記録した今の値、上限を変えるときはそのテストの値も見ること。
- `src/ownership_control.rs` の 3 つの定数の doc comment: 実際のプログラムの余裕を `tests/loop_growth.rs` が確かめること（`LoanGrowth` の doc comment の、このチケットを指す 1 行を置き換える）。
- `README.md` の「変更の種類別」の「所有権・借用・region」の行に `tests/loop_growth.rs` を足す。
- `_features/README.md` の状態。
- 言語リファレンス（`_tsuzuri/language-reference/`）は変えない（利用者から見える振る舞いを変えないので、PR の説明にそう書く）。

## 受け入れ条件

- [ ] `settled_loop_growth` と `loop_growth_limits` が `#[doc(hidden)]` の公開関数で、CLI の旗も環境変数も足していない。
- [ ] 統計を集めても、`tests/fixtures`・`examples` の `check --json` と IR が変わらない。
- [ ] `the_statistic_counts_what_the_stops_count` と `the_margins_fail_at_their_fractions` が成功する。
- [ ] `settling_loops_of_the_corpus_stay_far_from_the_loop_stops` が成功し、出力が 2 回の実行で同じで、今の値をテストの doc comment とこのチケットに記録した。
- [ ] 上限を 1 つずつ今の値の割合まで下げると、コーパスのテストがその統計の名前で失敗する（手順 5。試した値を記録した）。
- [ ] テストの時間が開発用のビルドの 4 つのスレッドで 2 分以内（記録した）。
- [ ] `docs/architecture.md`、定数の doc comment、`README.md` の表を直した。
- [ ] T2 が成功する。

## 落とし穴

- 数えるのは収束したループだけです。打ち切りで止めたループや反復の上限に届いたループを数えると、`tests/rc.rs` の上限の形のような入力が統計を上限まで押し上げます（それらはコーパスに入れませんが、std やコーパスの入力が `E1017` になったときも、統計ではなく失敗として報告する）。
- 中のループは外のループの反復ごとに、関数は不動点の回ごとに検査し直すので、同じループの検査が何度も入ります。統計は検査ごとの最大で、ループの数ではありません。
- 決定的であること: 統計の型に `HashMap` の順序を入れない。関数は番号順に順に検査し、入力の結果は入力の順に合わせる。同じ値のときの位置の選び方を決めておく。
- テストに 5・24・8・200 を書かない（上限を変えたときに割合が動かなくなる）。`loop_growth_limits` から読む。
- 不動点の検査（`check_facts`）が打ち切りで止まると、その本体の事実は `top` になり、検査は `failed` になります。`check_functions` の検査だけを数えると、不動点の回だけで上限に近づくループを見落とします（D2）。
- edition: プロジェクトはマニフェストの edition で解析する。gate はどちらの edition でも同じ最大でしたが、時間に余裕があれば `tests/conformance` のプログラムは両方の edition で解析してよい。

## 対象外

- 打ち切りの規則と上限の値を変えること、較正し直すこと。
- A31 のリポジトリの外のコーパス（gate の 15550 個、生成した 15552 個、探査の 873 個のディレクトリ）と計測用のビルドをリポジトリに入れること。
- `tests/` の Rust のテストに文字列で書いたプログラム（取り出す道具が要ります。打ち切りの境界は `tests/rc.rs` の `a_loop_` の 6 件が固定しています）。
- 検査の時間の合否（AGENTS.md）。止まるまで時間のかかる形（A34 の `loopplace` の「現状」の、借用をほとんど作らない文の多い本体）の時間は A34 の範囲です。
- 利用者向けの CLI の旗や環境変数。
- A33・A34 のループの検査の変更（それらが借用の数を変えたら、このテストの値を測り直して記録する）。

## 決定事項

### D1: 統計は `#[doc(hidden)]` の公開関数で見せる

- 決定: `tsuzuri::ownership` に、テストだけが呼ぶ `#[doc(hidden)]` の `settled_loop_growth` と `loop_growth_limits` を置く（`backend::layout_of` と同じ扱い）。
- 理由: `#[cfg(test)]` は統合テスト（`tests/`）から見えない。環境変数は、リリースビルドの利用者のバイナリに隠れたつまみを残し、`check-cli-docs.mjs` が文書を求め、並列の検査やほかのテストと同じプロセスで混ざる。CLI の旗は利用者に見える。
- 状態: 既定案（実装者はこの案に従う）。

### D2: 所有権の検査が走らせる、すべての収束したループの検査を数える

- 決定: `check_functions` の本体の検査に加えて、`call_facts` の不動点の検査（`check_facts`・`drops_stop_reads`）のループも数える。集める先は `start_checker` に渡すか `CallFacts` に持たせ、検査の結果は変えない。不動点の検査まで届かせると事実の求め方を変えずには済まないと分かったら、本体の検査だけを数え、その理由と見落としうる形をこのチケットに記録する。
- 理由: 打ち切りはどの検査も止め、不動点の回で止まった本体は事実が `top` になる。
- 状態: 既定案（実装者はこの案に従う）。

### D3: 割合に届いた正しいループは、割合を上げずに名前と値で記録する

- 決定: コーパスのループが割合に届き、調べた結果それが正しい（打ち切りの規則が想定する形で、上限には遠い）なら、テストに、入力と関数とその値を書いた例外として記録し、理由を書く。割合と上限は変えない。
- 理由: 割合を上げると、ほかのループの余裕も一緒に見えなくなる。例外は 1 つのループの値だけを許し、その値がさらに増えれば失敗する。
- 状態: 既定案（実装者はこの案に従う）。
