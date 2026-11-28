# F16: LLVM が展開できないループに展開ヒント `llvm.loop.unroll.enable` が付き、`build`・`run` で Clang が `loop not unrolled` の警告を出す

| 項目 | 内容 |
| --- | --- |
| ID | F16 |
| 優先度 | P3（修正。ビルドは成功し、プログラムの結果も変わらない。ただし、該当するループを 1 つ持つだけで、既定の `-O3` の `build`・`run`・`test` の標準エラーへ Clang の警告が出る。該当するのは、代入が 1 つの `while`、自己末尾呼び出しの reduction、境界検査の残る添字の `for` で、どれもふつうに書く形である。利用者は自分の誤りと区別できず、位置も `<unknown>:0:0` で原因の行が分からない。std の中にも該当するループがあり、`tsuzuri test --doc std -O3` は 40 行の警告を出す） |
| 規模 | M（先にヒントの効果を測る。その結果で、ヒントをやめるか、付ける条件を狭めて残りの警告を止めるかが決まる。`src/llvm.rs` の `hint_loop`、場合により `src/driver.rs` の Clang の引数、Rust のテスト、生成コードの比較、文書 2〜3 か所） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-28（[NC09](_completed/NC09-fft-and-polynomial.md) と [NC11](_completed/NC11-tensor-codec-and-io.md) の実装中に、std の新しいループで当たり、その PR（#153）ではループの書き方を変えて避けた。利用者の依頼「作業中に見つけた既存の不具合 2 件は `_features/` にチケットを起票」で起票。`main` の `7252006b` で再現） |
| 承認 | 不要（LLVM が実行できない変換の要求をやめる修正。言語の意味・診断・プログラムの結果は変えない。ヒントをやめるか条件を狭めるかは、手順 1 の計測で決める（D1）。遅くなる種目が残る場合は停止条件 S1） |
| 改善する劣位 | 不具合: 正しいプログラムのビルドが、利用者に直せない Clang の警告を出す。std と利用者のコードが、警告を避けるためだけにループを書き換えている（`std/Fft.tz` の `doubled_to`、`std/TensorCodec.tz`） |
| 手本にする既存実装 | `src/llvm.rs` の `FunctionEmitter::hint_loop`（条件: 小さい本体・代入が 1 つ・整数の reduction・チャンネルを待たない・境界検査が消えていない）、その呼び出し元（`src/llvm_control.rs` の `while_loop` と 3 つの `for` の形、`src/llvm.rs` の自己末尾呼び出しの 2 か所）、`tests/control.rs` の `control_lowering_is_direct_and_tail_calls_stay_loops`、`tests/bounds_checks.rs` の `unroll_hint_is_dropped_only_where_a_guard_was_removed`（F12） |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_control.rs`, `src/driver.rs`・`src/test_runner.rs`（設計の (b) を採ったときだけ）, `tests/control.rs`, `tests/bounds_checks.rs`, `tests/control.mjs`, `tests/tensor_io.mjs`（警告の検査のコメント）, `docs/architecture.md`（`hint_loop` の 2 か所の記述）, `docs/benchmarks.md`, `_features/README.md` |

## 実装者向け早見

- 範囲: `hint_loop` がヒントを付けるループのうち、LLVM が展開できないもの。確かめた形は 3 つ: `while`（終了条件がデータに依る）、自己末尾呼び出しのループ、境界検査の残る添字の `for`（検査の失敗がループの 2 つ目の出口になる）。
- 原因: `hint_loop` は本体の形（小さい・代入 1 つ・整数の reduction）だけを見て、LLVM がそのループを展開できるか（回数が求まり、出口が 1 つか）を見ない。LLVM は展開できないと、要求された変換ができなかったことを `-Wpass-failed=transform-warning` で報告する。インライン化の後に出口が増えることもあるので、コンパイラが IR を出す時点では決めきれない。
- 最初に読む: `hint_loop` の全体と 6 つの呼び出し元 → `docs/architecture.md` の「境界検査を省略した…`hint_loop`」の段落（F12）→ `docs/benchmarks.md` の「境界検査の残り方（F12）」の `array_index_sum` の記録。
- 最初にすること: ヒントの効果を測る（手順 1）。ヒントが付くプログラムは `benchmarks/` の 19 個のうち 9 個、`examples/` の 9 個のうち 1 個である。効果が無ければ、ヒントをやめるのが最も単純で確実な直し方になる（D1）。
- 最初に書くテスト: 「現状」の u1・u2・u5・u6 を `-O3` でビルドして、標準エラーに `loop not unrolled` が無いこと（今は赤）。
- 触ってはいけない: F12 の規則（境界検査が消えたループにはヒントを付けない）。速度の合否の閾値をテストに入れない。
- 完了の見分け方: 「現状」の u1・u2・u5・u6・x の `build`・`run` が `-O0`〜`-O3` で警告を 1 行も出さず、`tsuzuri test --doc std -O3` の標準エラーに `loop not unrolled` が無い。

## 目的

次のプログラムは正しく、`tsuzuri run` は期待どおりの値を出す。

```tsuzuri
def run :: own i64 -> i64
fn run start =
    let mut x = start
    while x % 7 != 0 do
        x = x + 3
    x

let! line = IO.readln ()
match line with
| Maybe.Some text -> IO.writeln (run text.length)
| Maybe.None -> IO.writeln "?"
```

ところが、`tsuzuri build`（既定の `-O3`）と `tsuzuri run -O3` は、標準エラーへ次を出す。

```text
warning: <unknown>:0:0: loop not unrolled: the optimizer was unable to perform the requested transformation; the transformation might be disabled or specified as part of an unsupported transformation ordering [-Wpass-failed=transform-warning]
1 warning generated.
```

警告は Tsuzuri のソースの位置を持たず、利用者には直す方法がない。コンパイラが、LLVM にできない変換を要求しないようにする。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief F16`。「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。

### 停止条件

- **S1** 手順 1 の計測で、ヒントをやめると遅くなる種目があり、かつ、その種目のループを「付ける条件」（D1 の (b)）で残せない。そのループの形と計測を記録して報告する。
- **S2** 警告を止めるために、`-Wno-pass-failed` だけを足して、展開できないと分かっているループ（`while`・自己末尾呼び出し）のヒントを残したくなった（D2）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（`main` の `7252006b`、macOS arm64、Apple clang 21.0.0 で確認）

`target/debug/tsuzuri` で確かめた。作業は `target/scratch/bug-tickets/` で行い、確認後に消した。表の数は、標準エラーの `loop not unrolled` の行数である。

| # | プログラム | IR のヒント | `--emit object` `-O0` | `-O1` | `-O2` | `-O3` | `--target wasm32` `-O3` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| u1 | `export def count :: ref [i64] -> i64` / `fn count xs =` / `let mut i = 0` / `while i < xs.length && xs[i] != 0 do` / `i = i + 1` / `i` | 1 | 0 | 1 | 1 | 1 | 1 |
| u2 | `export def run :: own i64 -> i64` / `fn run start =` / `let mut x = start` / `while x % 7 != 0 do` / `x = x + 3` / `x` | 1 | 0 | 1 | 1 | 1 | 1 |
| u5 | `def rec walk :: own i64 -> own i64 -> i64` / `fn rec walk n acc =` / `let mut total = acc` / `total = total + n` / `if n % 7 == 0 then total else walk (n + 3) total`（と、それを呼ぶ `export def start`） | 1 | – | 1 | – | 1 | – |
| u6 | `export def total :: ref [i64] -> own i64 -> i64` / `fn total xs n =` / `let mut sum = 0` / `for i in 0 ..< n do` / `sum = sum + xs[i]` / `sum`（`n` は `xs.length` と無関係なので、境界検査が残る） | 1 | – | 1 | – | 2 | – |
| u7 | `export def total :: own i64 -> i64` / `fn total n =` / `let mut total = 0` / `for i in 0 ..< n do` / `total = total + i * 3` / `total`（配列を読まない） | 1 | – | 0 | – | 0 | – |
| u8 | u7 の本体を `sum = sum + i / d`（`d` は引数）にした形 | 1 | – | 0 | – | 0 | – |
| u3 | `export def sum :: ref [i64] -> i64` / `fn sum xs =` / `let mut total = 0` / `for x in xs do` / `total = total + x` / `total` | 0 | 0 | 0 | 0 | 0 | 0 |
| u9 | u6 の範囲を `0 ..< xs.length` にした形（境界検査が消える。F12 の規則でヒントを付けない） | 0 | – | 0 | – | 0 | – |

「–」は確かめていない組み合わせである。

- u1・u2・u5・u6 はヒントを持ち、警告を出す。u7・u8 はヒントを持ち、警告を出さない（LLVM が展開できている）。u3・u9 はヒントを持たない。
- 警告は文脈に依る。u6 の関数を、定数の配列を渡す実行ファイルの中に置くと（`total (ref [3, 4, 5, 6, 7, 8]) text.length`）、`build -O1`・`-O3` は警告を出さない。逆に、`std/String.tz` の `matches_at` は、インライン化された先によって警告を出す（[NC11](_completed/NC11-tensor-codec-and-io.md) の実装記録）。
- x（「目的」のプログラム。実行ファイル）: `tsuzuri build -O1`・`-O3` と `tsuzuri run -O3` が、それぞれ警告を 1 つ出す。実行結果は正しい（入力 `12345` に `14`）。
- `tsuzuri test --doc std -O3`: 1516 個の例がすべて通り、標準エラーに `loop not unrolled` が 40 行出る（std の中の該当するループ）。`-O0` では出ない。
- ヒントを持つプログラムの数（`build --emit llvm -O3` の IR に `llvm.loop.unroll.enable` があるもの）: `benchmarks/` の直下の 19 個のうち 9 個、`examples/` の 9 個のうち 1 個。
- `tests/control.rs` の `control_lowering_is_direct_and_tail_calls_stay_loops` がヒントの存在を検査しているのは、`for i in 0..n do total = total + i`（と `Add.add total i`）の形だけである（u7 と同じ種類）。`tests/bounds_checks.rs` の `unroll_hint_is_dropped_only_where_a_guard_was_removed`（F12）は、境界検査が消えた形に無いこと、検査が残る形（u6 と同じ種類）と配列を読まない形に有ることを検査している。
- `docs/benchmarks.md` は、ヒントの由来を「小さい整数 reduction にだけ LLVM の展開ヒントを渡します。全ループへの一律の展開ヒントはミキサーを遅くしたため採用していません」と書き、F12 の節は「境界検査が残存しているループではこのヒントの有無にかかわらず `-O3` のコード生成結果は変わりませんでした」と書いている。ヒントで速くなった種目とその値は、どちらにも無い。

### 原因（コード）

- `src/llvm.rs` の `hint_loop(body, reads)` は、次をすべて満たすループの戻りの分岐に `!llvm.loop` を付ける: 境界検査が消えていない（`self.proven_reads == reads`）、本体が 64 節以下（`small`）、代入がちょうど 1 つ（`assignments`）、その代入が整数の reduction（`v = v op e`。`op` は `+`・`*`・`&`・`|`・`^` で、`e` は `v` を使わない）、チャンネルを待たない（`waits`）。
- 呼び出し元は 6 つ: `src/llvm_control.rs` の `while_loop`、範囲の `for` の 2 つの形（刻みが定数のものと、あふれを検査するもの）、配列などを回る `for`、`src/llvm.rs` の自己末尾呼び出しの 2 か所（直接の呼び出しとパイプ）。
- `while` の本体の代入が 1 つだけなら、終了条件はその変数（か、メモリの中身）に依る。u2 の `x % 7 != 0` は回数が式で求まらず、u1 の `i < xs.length && xs[i] != 0` は出口が 2 つある。自己末尾呼び出しのループ（u5）も、終了が引数の値に依る。LLVM の展開は、回数を実行時に計算でき、出口が実質 1 つのループにしか使えないので、どれも展開できない。
- u6 は回数の決まった `for` だが、残った境界検査の失敗（トラップへの分岐）がループの 2 つ目の出口になる。Clang はこのループも展開できないと報告する。F12 の記録（検査が残るループではヒントの有無でコードが変わらなかった）は、このことと合う: 展開されていない。
- Clang は、強制された変換が残ったループを `-Wpass-failed=transform-warning` で報告する。インライン化で呼び出し先の検査がループの中に入ると出口が増えるので、IR を出す時点で出口が 1 つのループでも、後で報告されうる（`String.matches_at`）。
- `waits` は、同じ理由（「回数が無い」）でチャンネルを待つループを外している（TZ03）。
- `docs/architecture.md` は、この振る舞いを 2 か所に書いている: 「1 つの代入だけを持つ `while`（終了条件がデータに依る）には、コンパイラが `llvm.loop.unroll.enable` を付け、LLVM が展開できないと Clang が警告を出します（既存の振る舞い）」（NC09 が足した記述）と、F12 の「境界検査が残る通常のループに対するヒント付与規則は変更していません」。

### 今の回避

- `std/Fft.tz` は、該当する反復を再帰（`doubled_to`。代入を持たないのでヒントが付かない）か回数の決まった `for` で書いている。NC11 も、`std/TensorCodec.tz` の空白を読み飛ばすループなどを同じ理由で書き換え、`.npy` の名前の判定を `String.ends_with` からバイトの比較に変えた。
- `tests/tensor_io.mjs` は、ビルドの標準エラーに警告が無いことを検査している（Windows では `pass-failed` と `loop not` の行だけを見る）。`tests/features.mjs` の suite は標準エラーの警告を検査しない。

## 仕様

1. `tsuzuri build`・`run`・`test` は、コンパイラが付けたヒントが原因の `loop not unrolled`（`-Wpass-failed=transform-warning`）を、`-O0`〜`-O3`、native と wasm32 で出さない。
2. 言語の意味、診断、プログラムの結果は変えない。
3. F12 の規則（境界検査が消えたループにはヒントを付けない）は変えない。
4. ヒントをやめる・条件を狭めることで時間が変わる種目があれば、`docs/benchmarks.md` に計測を記録する。速度の合否はテストに入れない。

## 設計

直し方は、手順 1 の計測で決める（D1）。

- **(a) ヒントをやめる**: `hint_loop` と 6 つの呼び出しを消す。計測で、ヒントのあるコンパイラとないコンパイラの差が [性能指標の記録](../docs/benchmarks.md#性能指標の記録) の読み方で「差なし」なら、これを採る。警告は原理的に出なくなり、条件の関数（`reduction`・`assignments`・`small`・`waits`）と F12 の例外も要らなくなる。`tests/control.rs` と `tests/bounds_checks.rs` のヒントの検査は、「IR に `llvm.loop.unroll.enable` が無い」に改める。
- **(b) 条件を狭めて残す**: ヒントで速くなる種目があるときだけ採る。ヒントを付けるのは、LLVM が展開できている形（u7・u8 の種類: 回数の決まった `for` で、本体を出す間に検査の分岐を 1 つも出さなかったもの）だけにする。`while_loop` と自己末尾呼び出しの 2 か所は `hint_loop` を呼ばない。本体の検査の有無は、F12 の `proven_reads` と同じやり方で、本体を出す前後の「出した検査の数」を比べて決める（`fn guard` で数える）。それでもインライン化の後に出口が増えて報告されうるので、プログラムの IR をコンパイルする Clang の呼び出し（`src/driver.rs` の `-x ir` の所と `src/test_runner.rs`）に `-Wno-pass-failed` を足す。これは、展開できないと分かっている形のヒントを消した後の、残りに対してだけ許す（D2）。
- どちらでも、`docs/architecture.md` の 2 か所と `docs/benchmarks.md` の「小さい整数 reduction にだけ…」の文を、新しい振る舞いに直す。`tests/tensor_io.mjs` の警告の検査は、Windows 以外では元どおり「警告が 1 行も無い」のままにし、コメントの展開ヒントの説明を消す。

## 実装手順

1. **変更**: なし（計測）。**内容**: `hint_loop` を何もしない関数にしたコンパイラ（計測用。commit しない）と今のコンパイラを release でビルドし、ヒントを持つ 9 個の `benchmarks/` のプログラムを含む suite（`run-control.mjs`・`run-numeric.mjs`・`run.mjs` など、該当するもの）を `--metrics` で 9 標本以上測って `node benchmarks/metrics.mjs report <after> --baseline <before>` で比べる。ヒントを持つ関数の `-O3` の逆アセンブルも比べる。**確認**: 種目ごとの「差なし・改善・悪化」と、機械語が変わった関数の一覧を、このチケットに記録する。(a) か (b) かを決める。
2. **変更**: `tests/control.rs`、`tests/control.mjs`（か `tests/e2e.mjs`）。**内容**: u1・u2・u5・u6・x を `-O3`（と `-O1`）でビルドして標準エラーに `loop not unrolled` が無いこと。IR の検査は、決めた案に合わせる（(a): ヒントがどこにも無い。(b): u1・u2・u5・u6 に無く、u7 に有る）。**確認**: 新しい検査が赤。
3. **変更**: `src/llvm.rs`, `src/llvm_control.rs`（(b) なら `src/driver.rs`・`src/test_runner.rs` も）、`tests/bounds_checks.rs`。**内容**: 「設計」の (a) か (b)。**確認**: T0、`cargo test --locked --test control --test bounds_checks`、`tests/fixtures`・`examples`・`benchmarks` の `--emit llvm -O1`（native と wasm32）の修正前後の差が `!llvm.loop` とそのメタデータの定義・番号だけであること、T1。
4. **変更**: `tests/tensor_io.mjs`、`docs/architecture.md`、`docs/benchmarks.md`。**内容**: 警告の検査のコメント、文書の 3 か所、手順 1 の計測の記録。`std/Fft.tz`・`std/TensorCodec.tz` の回避の書き方は戻さない（D3）。**確認**: `node tests/tensor_io.mjs target/debug/tsuzuri`、`tsuzuri test --doc std -O3 2>&1 | grep -c 'loop not unrolled'` が 0、T2 の `gen-docs --check`・`check-links`。

## テスト計画

- E2E: u1・u2・u5・u6・x の `build -O1`・`-O3`（native と wasm32）の標準エラーに `loop not unrolled` が無い。`tests/tensor_io.mjs`（ビルドの標準エラーに警告が無い）。
- `tests/control.rs`・`tests/bounds_checks.rs`: 決めた案の IR の検査（手順 2）。
- 比較: 手順 1 の計測と逆アセンブル、手順 3 の IR の差。

## 受け入れ条件

- 「現状」の u1・u2・u5・u6・x が、`-O0`〜`-O3`、native と wasm32 で警告を出さない。
- `tsuzuri test --doc std -O3` の標準エラーに `loop not unrolled` が無い。
- 手順 1 の計測がこのチケットに記録され、採った案がその結果と合っている。
- T1 と T2 が通る。

## 対象外

- ほかの最適化の警告（`-Wpass-failed` のベクトル化の側など）。今は出ていない。
- Windows の MSVC のヘッダーが出す、ランタイムの C の非推奨の警告（`tests/tensor_io.mjs` が Windows で読み飛ばしているもの）。
- ヒントに代わる、別の展開・ベクトル化の指示の追加。

## 決定事項

- **D1（既定案）**: まず測り、ヒントの効果が「差なし」ならヒントをやめる（設計の (a)）。効果のある種目があるときだけ、LLVM が展開できる形に条件を狭めて残す（(b)）。今の記録には、ヒントで速くなった種目の値が無い。
- **D2（既定案）**: `-Wno-pass-failed` だけで警告を隠さない。展開できないと分かっているループ（`while`・自己末尾呼び出し・検査の分岐を持つ本体）のヒントは消す。フラグは、(b) を採ったときに、インライン化の後にしか分からない残りに対してだけ足す。
- **D3（既定案）**: `std/Fft.tz`・`std/TensorCodec.tz` の回避の書き方は戻さない。戻しても結果は同じで、検査をやり直す費用だけがかかる。
