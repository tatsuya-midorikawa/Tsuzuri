# G37: LSP のシグネチャヘルプが、閉じ括弧で終わる行の次の行の呼び出しで何も返さない

| 項目 | 内容 |
| --- | --- |
| ID | G37 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | G07, G12 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-05（TZ13 の作業中に発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（LSP の応答を正しくする修正。言語・CLI は変えない） |
| 改善する劣位 | 不具合: `)`・`]`・`}`・補間の閉じ（`$"v{n}"`）で終わる行の次の行で、`Steps.yield ()` のような呼び出しの引数の位置にいても、`textDocument/signatureHelp` が `null` を返す |
| 手本にする既存実装 | `src/lsp.rs` の `call_context` の、行頭より前の項で止まる条件（`token.span.start < line_start`）。テスト: `tests/lsp.rs` の `signature_help_counts_curried_arguments`、`keyword_member_yield_has_completion_hover_and_signature_help` |
| 主な影響ファイル | `src/lsp.rs`, `tests/lsp.rs`, `_features/README.md` |

## 目的

関数本体の 2 行目以降の呼び出しでも、前の行の終わり方に関係なくシグネチャヘルプを出す。前の行が `Routine.yield ()` や `let xs = [1, 2]` で
終わるのは普通の書き方なので、今は本体の多くの行でシグネチャヘルプが出ない。

## 着手条件と停止条件

### 着手条件

- G07・G12 が `_features/README.md` で done であること（HEAD で done）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の再現（`null` になること）を確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `tests/lsp.rs` の既存のシグネチャヘルプ・補完のテストの期待値を変えないと通らない。
- 複数行にまたがる括弧の中の呼び出し（`foo (a,` の次の行など）の結果が、修正の前後で変わる（このチケットは、深さ 0 の閉じ括弧が前の行にある場合だけを直す）。

## 現状（HEAD `5ee6b0f2` で確認）

- `src/lsp.rs:2578` の `signature_help` は、`call_context(&tokens, text, offset)` が `None` なら `Value::Null` を返す。
- `src/lsp.rs:2508` の `call_context` は、カーソルの前の token から後ろへ戻り、呼び出しの頭を探す。戻る loop（2514〜2533 行）は、各 token について
  次の順に判定する。
  1. `closes(&token.kind)`（`)`・`]`・`}`・`]`（リスト）・補間の終わり）なら `depth += 1`。
  2. `opens` で `depth > 0` なら `depth -= 1`。
  3. `(` なら、そこを開き括弧として止まる。
  4. `depth == 0` で、行頭（`line_start`）より前の token か、項・名前・前置でない token なら、その次を `start` として止まる。
- 1 と 2 が 4 の行頭の判定より先にあるので、前の行の末尾が閉じ括弧だと、前の行の括弧の中へ入ってしまう。たとえば前の行が `Routine.yield ()` なら、
  `)` で `depth = 1`、`(` で `depth = 0` と戻り、次の `yield`（前の行）で 4 に当たって `start` が前の行の `(` を指す。`tokens[start]` は `Ident` でなく、
  `open` も無いので `None` を返す。
- `tests/lsp.rs:503` の `keyword_member_yield_has_completion_hover_and_signature_help` は、この不具合を避けるために呼び出しを本体の 1 行目
  （`let m = Steps.yield () + n`）に置いている（TZ13）。

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64）

`Steps.tz`:

```text
/// Gives way.
def yield :: unit -> i64
fn yield _unit = 0
```

`Main.tz`（4 行目の `)` の位置、0 始まりで line 3、`Steps.yield (` の次の `)` の character で `textDocument/signatureHelp` を送る）:

```text
def work :: i64 -> i64
fn work n =
    Routine.yield ()
    Steps.yield () + n
```

`tsuzuri lsp` の応答は `{"id":1,"jsonrpc":"2.0","result":null}`。3 行目を次のどれにしても同じく `null` になる:
`let xs = [1, 2]`、`let s = $"v{n}"`、`let t = (n, n)`。3 行目を `let m = 1` にすると
`{"activeParameter":0,"activeSignature":0,"signatures":[{"label":"yield (_unit: unit) -> i64",...}]}` を返す。

`initialize`（`rootUri` に 2 つのファイルのディレクトリ）、2 つの `textDocument/didOpen`、`textDocument/signatureHelp`、`shutdown`、`exit` を
`Content-Length` 付きで標準入力に書く Node のスクリプトで確かめた（`tests/lsp.rs` の `session` と同じ順）。

## 仕様

- 呼び出しの頭を探すとき、深さ 0 で行頭より前にある token は、閉じ括弧であっても、そこで止まる（その行の呼び出しには属さない）。
- 深さ 0 で行頭より前にある `(` は、今と同じく開き括弧として扱う（`f (` で改行した呼び出しの引数を探す既存の振る舞いを保つ）。
- 行の中の閉じ括弧（`Steps.yield () + n` の `()` や `f (g x) y` の `(g x)`）の扱いは変えない。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/lsp.rs` | `call_context` の戻る loop | `depth == 0 && token.span.start < line_start && token.kind != TokenKind::LeftParen` の判定を、`closes` の判定より前に置く |
| `tests/lsp.rs` | 新しいテスト | 前の行の末尾の閉じ括弧の種類ごとのシグネチャヘルプ |
| `tests/lsp.rs` | `keyword_member_yield_has_completion_hover_and_signature_help` | 呼び出しを TZ13 の前の形（`Routine.yield ()` の次の行）に戻す |

### アルゴリズム

loop の先頭で、`depth == 0` かつ行頭より前の token が `(` でなければ、`start = index + 1` として止まる。残りの判定（`closes`・`opens`・`(`・
項でない token）は今の順のままにする。`depth > 0` の間は行頭より前でも括弧の対応をたどる（行の中で開いた括弧の中身が前の行に続くことはないが、
今の振る舞いを変えないため）。

## 実装手順

### 手順 1: ベースラインと再現

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。手順 2 のテストを先に書き、失敗を確かめる。
- 確認: `cargo test --locked --test lsp` が全件成功する（手順 2 の前）。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/lsp.rs`。
- 内容: `signature_help_after_a_line_that_ends_with_a_closer`（新規）。`scripted` で `Steps.tz` と、3 行目を
  `Routine.yield ()`・`let xs = [1, 2]`・`let s = $"v{n}"`・`let t = (n, n)`・`let r = Pair { a: n, b: n }`（レコードがあれば）にした `Main.tz` を開き、
  4 行目の `Steps.yield (` の `)` の位置で `textDocument/signatureHelp` を送る。どれも `label` が `yield ` で始まり、`activeParameter` が 0。
  比較のため 3 行目が `let m = 1` の場合も同じ結果になることを確かめる。
- 確認: `cargo test --locked --test lsp signature_help_after_a_line` が `1 failed`。

### 手順 3: `call_context` を直す

- 変更: `src/lsp.rs`。
- 内容: 「アルゴリズム」のとおり。
- 確認: 手順 2 のテストが成功し、`cargo test --locked --test lsp` が全件成功する（`running N tests` の N が手順 1 より 1 多い）。

### 手順 4: TZ13 の回避を戻す

- 変更: `tests/lsp.rs`。
- 内容: `keyword_member_yield_has_completion_hover_and_signature_help` の source を
  `"def work :: i64 -> i64\nfn work n =\n    Routine.yield ()\n    Steps.yield () + n\n"` に戻し、`position(source, …)` の目印を合わせる。
  コメントの「the builtin `Routine.yield` … has hover」は残す。
- 確認: `cargo test --locked --test lsp keyword_member_yield` が成功する。

### 手順 5: 最終確認

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、T1。VS Code 拡張のテスト（`vsc/`）は LSP の応答の形を変えないので不要。

## テスト計画

### Rust テスト

- `tests/lsp.rs`: `signature_help_after_a_line_that_ends_with_a_closer`（新規。閉じ括弧の種類 4〜5 通りと、比較の 1 通り）。
- `tests/lsp.rs`: `keyword_member_yield_has_completion_hover_and_signature_help`（呼び出しの位置を戻す）。
- 既存の `signature_help_counts_curried_arguments`、補完・hover のテストは変えずに成功する。

### E2E

- なし（`tests/lsp.rs` が `tsuzuri::lsp::serve` を直接動かす）。

### 既存テストへの影響

- `keyword_member_yield_has_completion_hover_and_signature_help` の source（手順 4）だけ。

### 性能

- 影響なし。

## ドキュメント

- `_tsuzuri/language-reference/`: 変更不要。シグネチャヘルプの説明（`grep -rn "シグネチャ" _tsuzuri/language-reference`）は行の位置による制限を書いていない。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [x] 「再現」の 4 通りの前の行で、シグネチャヘルプが `yield (_unit: unit) -> i64` を返す。
- [x] `signature_help_after_a_line_that_ends_with_a_closer` が修正前に失敗し、修正後に成功する。
- [x] `tests/lsp.rs` の既存のテストが期待値を変えずに成功する。
- [x] `keyword_member_yield_has_completion_hover_and_signature_help` が、`Routine.yield ()` の次の行の呼び出しで成功する。
- [x] fmt・clippy・T1 が成功する。

## 落とし穴

- 行頭の判定を loop の先頭に置くとき、`(` を除き忘れると、`f (` で改行した呼び出しの引数の位置でシグネチャヘルプが消える。
- `line_start` はカーソルの行の先頭である。CRLF の文書では `\r` が前の行の末尾に残るが、`rfind('\n')` で求めているので影響しない。CRLF の文書でも
  手順 2 の 1 通りを確かめるとよい。
- 補完（`completion`）は `call_context` を使わない（`grep -n "call_context(" src/lsp.rs` が `signature_help` の 1 か所）。補完の不具合は別に扱う。

## 対象外

- 複数行にまたがる括弧の中の呼び出しの改善。
- 組み込み関数だけのモジュール（`Routine` など）のシグネチャヘルプと補完。定義が索引に無いので今は hover だけで、
  `keyword_member_yield_has_completion_hover_and_signature_help` のコメントが記録している。

## 決定事項

### D1: 行頭の判定は閉じ括弧より先にする

- 決定: 深さ 0 で行頭より前の token は、`(` を除いて、括弧の対応をたどる前に止まる。
- 理由: 前の行の括弧の中身は、カーソルの行の呼び出しの引数になりえない。`(` を除くのは、`f (` で改行した呼び出しの既存の振る舞いを保つため。
- 状態: 既定案（実装者はこの案に従う）。実装では精密化した（「実装と検証」の判断 1。レビューの指摘への対応で改めた）。前の行で開いた括弧の中の行の結果も、
  カーソルの行の呼び出しが正しくなる場合は変えた。これは停止条件の 2 つ目に当たり、利用者から判断を委ねられたオーケストレーターが受け入れた（「レビューの指摘への対応」の判断 R1）。

## 実装と検証（2026-11-06）

- **範囲**: 手順 1〜5 をすべて実装した（1 つのブランチ `fix/g37-signature-help-after-closer`）。
- **手順 1**: `cargo test --locked --test lsp` が 49 件すべて成功（ベースライン）。
- **手順 2**: `tests/lsp.rs` に `signature_help_after_a_line_that_ends_with_a_closer` を足した。`Steps.tz`（`/// Gives way.`・`def yield :: unit -> i64`・`fn yield _unit = 0`）と、3 行目を
  `Routine.yield ()`・`let xs = [1, 2]`・`let s = $"v{n}"`・`let t = (n, n)`・`let ys = [|1, 2|]`・`let m = 1`・`let r = Pair { a: n, b: n }`（`record Pair` は `Main.tz` の末尾）にした `Main.tz`、
  `Routine.yield ()` の版を CRLF にした文書、複数行の `Steps.yield(` の 2 通り（`Steps.yield(` の次の行の `())` の前と、`()` の次の行の `)` の前）の 10 通りで、
  `Steps.yield (` の `)`（複数行の 2 通りはそれぞれの位置）にシグネチャヘルプを送り、結果全体が
  `{"signatures": [{"label": "yield (_unit: unit) -> i64", "parameters": [{"label": "_unit: unit"}]}], "activeSignature": 0, "activeParameter": 0}` に一致することを確かめる
  （形は `signature_help_counts_curried_arguments` の `read (number: i64) -> i64` と同じ規則から手で書いた）。修正前は `cargo test --locked --test lsp signature_help_after_a_line` が
  `1 failed`（閉じ括弧の 6 通りと CRLF の 1 通りの 7 通りが `null`。`let m = 1` と複数行の 2 通りは修正前から成功）。
- **手順 3**: `src/lsp.rs` の `call_context` を直した（下の判断 1。チケットの D1 をそのままにはしていない）。手順 2 のテストが成功し、`cargo test --locked --test lsp` が 50 件すべて成功（49 + 1）。
- **手順 4**: `keyword_member_yield_has_completion_hover_and_signature_help` の source を
  `"def work :: i64 -> i64\nfn work n =\n    Routine.yield ()\n    Steps.yield () + n\n"` に戻した。`position(source, …)` の 4 つの目印（`"yield () +"`・`") +"`・`" () +"`・`"yield ()\n"`）は、
  新しい source でも同じ呼び出し（4 行目の `Steps.yield` と 3 行目の `Routine.yield`）を指すので変えていない。コメントは残した。修正前のコードではこのテストが失敗し（シグネチャヘルプが `null` で label の
  `unwrap` が panic）、修正後は `cargo test --locked --test lsp keyword_member_yield` が 1 件成功。
- **手順 5 と T2**: `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings` が成功。`cargo nextest run --locked` が 2935 件すべて成功（4 件 skip）。
  `cargo build` の後の `node tests/lsp_sessions.mjs target/debug/tsuzuri` が成功（utf-16・utf-8 の session と features の 4 つ）。チケットの移動の後に `node scripts/check-guide-lite.mjs`（88 件の参照すべて成功）と `node tests/repo_kb.mjs`（84 件成功）。VS Code 拡張のテストは手順 5 のとおり走らせていない。
- **差分の検査**（追加）: 修正前と修正後の dev のバイナリで、`tests/fixtures/*` と `examples/*` の 176 のプロジェクトの 8,000 byte 以下のすべての `.tz` の、すべての位置（400,803 か所）に
  `textDocument/signatureHelp` を送って比べた。変わったのは 1,914 か所で、すべて修正前の `null` が、カーソルの行の呼び出しのシグネチャになったもの（たとえば
  `assert (point == point)` の次の行の `score ref point`、`tzroutine { … }` の中の `Channel.send sleeper_started ()` の次の行の `host_wait_released ()`）。修正前に `null` でなかった結果は 1 つも変わらない。
  手で作った 10 のプロジェクトでは、`Routine.yield ()` の次の行の各位置の結果が、`let m = 1` の次の行の同じ位置の結果と一致した（`Steps.yield ()` の直後の `activeParameter` 1 も同じ）。
- **判断 1（D1 の精密化。レビューの指摘への対応で改めた。出荷する規則）**: チケットの D1 のとおり「深さ 0 で行頭より前の `(` でない token で止まる」判定を `closes` の前に置くと、
  `Steps.yield(\n        ()\n    ) + n` の 3 行目の `)` の前（前の行で開いた `name(` の中で、カーソルの行のカーソルの前に何も無い）が、修正前の `yield` から `null` に変わる（手順 2 のテストの最後の 1 通り）。
  そこで、戻る loop は今の判定の順のままにして、深さ 0 で行頭より前の閉じ括弧に最初に会ったところで、その次の token の位置（`boundary`）を覚える。カーソルの行の呼び出しは `boundary` から始まり、前の行の括弧は
  その引数にならない。loop が開いたままの `(` に届いたときは、その `(` から `boundary` までには括弧の組しか無い（深さ 0 で行頭より前のほかの token では loop が止まる）ので、その `(` はカーソルの行を含む。
  - loop が `(` に届かなかったとき、または届いた `(` が組・括弧・空白の後の `(` のときは、カーソルの行の呼び出しを返す。修正前の結果は `null` だった（修正前の `tokens[start]` は前の行の括弧の開き）。
    括弧の釣り合わない文書で loop が先頭まで戻ったときは、修正前は文書の最初の名前を頭にした誤った結果（2 引数の関数に `activeParameter` 3 など）だった。
  - 届いた `(` が空白なしで名前に続く `name(` の呼び出しで、`boundary` の token が識別子（カーソルの行がカーソルの前に自分の呼び出しの頭を書いている）なら、内側のその呼び出しを返す（修正前は外側の `name(`）。
    頭が関数を指さないとき（局所変数や、`Steps` のようなモジュール名だけ）は、`name(` の呼び出し（修正前と同じ結果）を返す。`call_context` は呼び出しを内側から順に返し、`signature_help` は関数を指す最初のものを使う。
  - 届いた `(` が `name(` で、カーソルの行のカーソルの前が空か識別子で始まらない（`) + n` の `)` の前、`(n) ` の後）なら、`name(` の呼び出し（修正前と同じ）を返す。
  これで仕様の 3 つの項（前の行の閉じ括弧で止まる・行頭より前の `(` は開き括弧・行の中の閉じ括弧は変えない）を満たす。カーソルの行の結果は、修正前の `name(` の結果を残す最後の 2 つの場合を除いて、
  前の行が項で終わる場合と同じになる（「レビューの指摘への対応」の差分の表）。修正前に `null` でなかった結果が変わるのは、`name(` の中で関数の頭を書いた行と、括弧の釣り合わない文書だけ。
- **判断 2**: 閉じ括弧の種類として、チケットの 5 通りにリストの `|]`（`TokenKind::RightList`）を足し、`closes` が扱う 5 種類（`)`・`]`・`}`・`|]`・補間の終わり）をすべて覆った。CRLF は落とし穴のとおり 1 通り確かめた。
- **判断 3**: `Steps.yield (` のように空白を空けた `(` で改行した呼び出しの次の行（`        ()) + n` のように、カーソルの行が自分の呼び出しを書いていない行）は、修正の前後とも `null` のまま（2 回目の loop で、行頭より前の `yield` で止まるため）。対象外の「複数行にまたがる括弧の中の呼び出しの改善」に当たるので直さず、
  テストにも固定していない（`null` を期待値にすると、正しくない振る舞いを固定するため）。差分の検査で修正の前後が同じであることだけを確かめた。
- **文書**: 変更なし。言語リファレンスと `docs/language.md`・`docs/architecture.md`・`README.md` のシグネチャヘルプの記述（`grep -rn "シグネチャヘルプ\|signature help" _tsuzuri/language-reference docs README.md`）は行の位置による制限を書いていない。
  `_features/README.md` の行を更新した。言語リファレンスのページを変えていないので `check-docs` は走らせていない。
- **変更したファイル**: `src/lsp.rs`（`call_context`）、`tests/lsp.rs`（新しいテスト 1 件と `keyword_member_yield_has_completion_hover_and_signature_help` の source）、`_features/README.md`、このチケット（`_features/_completed/` へ移動）。

### レビューの指摘への対応（2026-11-06）

独立したレビューの指摘 2 件（どちらも軽微。どちらも複数行の括弧の扱い）に対応した。比べたのは、main（`72f06862`）のソースから作った dev のバイナリ（以下 main）、
レビュー前のこのブランチ（`0e52dbb4`。以下レビュー前）、修正後のこのブランチの dev のバイナリの 3 つ。編集中の文書は、レビューと同じく、正しい primer の `Main.tz` を解析させた後に
`textDocument/didChange` で送った（`Steps.tz` は `yield :: unit -> i64`・`add :: i64 -> i64 -> i64`・`one :: i64 -> i64`）。

- **指摘 1（loop が届いた `(` が呼び出しかを確かめていない）**: レビュー前のコードは、loop が開いたままの `(` に届くと、それが `name(` の呼び出しでなくても修正前の結果を残した。
  再現（最後の行の末尾）: `let t = (⏎    (n)⏎    Steps.one (` と `Steps.add (⏎    (n)⏎    Steps.one (` がレビュー前も `null`、`Steps.add(⏎    (n)⏎    Steps.one (` がレビュー前も外側の `add @0`
  （下の表の 26・24・22）。2 行目を `n` にすると 3 つとも `one @0`（表の 27・25・23）で、レビューの証拠と同じだった。
  - 対応: 判断 1 の規則に改めた。組・括弧・空白の後の `(` に届いたときは、カーソルの行の呼び出しを返す。`name(` に届いたときは、カーソルの行がカーソルの前に識別子で始まる頭を書いていればその呼び出しを返し、
    頭が関数を指さなければ `name(` の呼び出しを返す。`call_context` は呼び出しを内側から順に返す `Vec` になり、`signature_help` は関数を指す最初のものを使う（定義を引く部分は `signature` に分けた）。
  - テスト: `tests/lsp.rs` の `signature_help_inside_brackets_that_an_earlier_line_opened`（新規）。primer の解析の後に 9 つの編集中の文書を順に送り、最後の行の末尾でシグネチャヘルプを送る。
    組の場合（レビューの提案の `let t = (⏎    (n)⏎    Steps.one (`）、空白の後の `(`、`name(` の中で関数の頭を書いた行の 2 通り（`Steps.one (`・`Steps.one n`）、`name(` とコンマの 2 通り（指摘 2 の形）、
    `name(` の中で頭が関数でない行と組で始まる行の 2 通り（main の `add @0` を残す）、括弧の釣り合わない文書（`Steps.add n)⏎    Steps.one (`。main は `add @3`）。期待値は
    `one (a: i64) -> i64` と `add (a: i64) (b: i64) -> i64`（`activeParameter` 0）の結果全体で、`signature_help_counts_curried_arguments` の形の規則から手で書いた。
    レビュー前のコードでは `cargo test --locked --test lsp -- signature_help keyword_member_yield` が `3 passed; 1 failed` で、組と空白の後の `(` が `null`、`name(` の 2 通りが `add @0` の 4 件が期待と違った。
    修正後は `4 passed`。前からの `signature_help_after_a_line_that_ends_with_a_closer` は期待値を変えずに成功する（コメントだけ、出荷する規則に合わせた）。
  - 後の証拠: 表の 22・24・26 が `one @0` になり、2 行目が `n` の 23・25・27 と同じ。
- **指摘 2（判断 1 と README の行が、前の行で開いた `(` の中の行は従来どおりと書いていた）**: 再現: 表の 28〜31（`Steps.add(n,⏎    (n)⏎    Steps.one (` など、コンマか別の token で loop が `(` の前に止まる形）は
  main で `null`、レビュー前で `one @0` だった。新しい結果は正しいが、記録は「修正前と同じ結果になる」と書き、停止条件の 2 つ目にも触れていなかった。
  - 対応: 判断 1 を出荷する規則の説明に書き直し、D1 の状態と判断 3 を合わせた。`_features/README.md` の行の要約を「前の行で開いた組・括弧・`name(` の中の行でも同じで、カーソルの行がカーソルの前に関数の頭を
    書いていなければ、その `name(` の呼び出しを返す」にした。停止条件は判断 R1 として記録した。
  - 後の証拠: 28〜31 は修正後も `one @0`（2 行目が `n` の形と同じ）。

#### 差分の表

本体は `record Pair { a: i64, b: i64 }`・`def work :: i64 -> i64`・`fn work n =` の後の行（`fn work n =` の 4 つの空白の字下げは省いた）。「全文」は文書全体。8・9・11・62〜71 は、前の行が項で終わる参照。
main・レビュー前・修正後の列は最後の行の末尾の結果（`one @0` は `one (a: i64) -> i64` で `activeParameter` 0、`null` は結果なし）。最後の列は、本体のすべての行のすべての位置（73 通りで合計 3,281 か所）のうち、
main と修正後で結果が違う位置の数。

| # | 本体（`⏎` は改行） | main | レビュー前 | 修正後 | main から変わった位置 |
| --- | --- | --- | --- | --- | --- |
| 1 | `Routine.yield ()⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 2 | `let xs = [1, 2]⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 3 | `let r = Pair { a: n, b: n }⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 4 | `let ys = [\|1, 2\|]⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 5 | `let s = $"v{n}"⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 6 | `let s = $"a{n}b{n}c"⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 7 | `let t = (n, n)⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 8 | `let m = 1⏎Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 9 | `n⏎Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 10 | `Routine.yield ()⏎Steps.yield () + n`（CRLF） | `null` | `null` | `null` | 9 |
| 11 | `let m = 1⏎Steps.yield () + n`（CRLF） | `null` | `null` | `null` | 0 |
| 12 | `Routine.yield () // (x⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 13 | `Routine.yield () /* ( */⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 14 | `Routine.yield ()⏎// ) (⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 15 | `let s = "(("⏎Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 16 | `let c = ')'⏎Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 17 | `let s = $"v{Steps.one (n)}"⏎Steps.add n (` | `null` | `add @1` | `add @1` | 7 |
| 18 | `let v = Steps.one (Steps.one (n))⏎Steps.add n (` | `null` | `add @1` | `add @1` | 7 |
| 19 | `let v = [(n), [n]]⏎Steps.add n (` | `null` | `add @1` | `add @1` | 7 |
| 20 | `let v = Steps.one(n)⏎Steps.add n (` | `null` | `add @1` | `add @1` | 7 |
| 21 | `let v = Steps.one(n)⏎Steps.add (n) n` | `null` | `add @1` | `add @1` | 8 |
| 22 | `Steps.add(⏎    (n)⏎    Steps.one (` | `add @0` | `add @0` | `one @0` | 5 |
| 23 | `Steps.add(⏎    n⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 24 | `Steps.add (⏎    (n)⏎    Steps.one (` | `null` | `null` | `one @0` | 5 |
| 25 | `Steps.add (⏎    n⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 26 | `let t = (⏎    (n)⏎    Steps.one (` | `null` | `null` | `one @0` | 5 |
| 27 | `let t = (⏎    n⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 28 | `Steps.add(n,⏎    (n)⏎    Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 29 | `Steps.add (n, (n)⏎    Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 30 | `Steps.add(n, (n)⏎    Steps.one n` | `null` | `one @0` | `one @0` | 5 |
| 31 | `let t = ((n),⏎    (n)⏎    Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 32 | `Steps.yield(⏎    ()⏎) + n` | `null` | `null` | `null` | 0 |
| 33 | `Steps.yield(⏎    ()) + n` | `null` | `null` | `null` | 0 |
| 34 | `Steps.yield (⏎    ()⏎) + n` | `null` | `null` | `null` | 0 |
| 35 | `Steps.yield (⏎    ()) + n` | `null` | `null` | `null` | 0 |
| 36 | `Steps.add(⏎    ()⏎    n` | `add @0` | `add @0` | `add @0` | 0 |
| 37 | `Steps.add(⏎    ()⏎    Steps.one` | `add @0` | `add @0` | `one @0` | 3 |
| 38 | `Steps.add(⏎    ()⏎    (n) ` | `add @0` | `add @0` | `add @0` | 0 |
| 39 | `Steps.add(⏎    ()⏎    Steps.one n` | `add @0` | `add @0` | `one @0` | 5 |
| 40 | `let xs = [⏎    1,⏎    2]⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 41 | `let p = Pair { a: n, b: n }⏎let r = { p with⏎    a = n }⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 42 | `let p = Pair { a: n, b: n }⏎let r = { p with a = n }⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 43 | `Steps.add (n,⏎    n) n` | `null` | `null` | `null` | 0 |
| 44 | `let v = (n) \|> Steps.one⏎Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 45 | `let v = (n)⏎        \|> Steps.add (` | `add @0` | `add @0` | `add @0` | 0 |
| 46 | `let v = (n) +⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 47 | `let v = n +⏎    (n) +⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 48 | `for i in [1, 2] do⏎    Routine.yield ()⏎    Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 49 | `while (n > 0) do⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 50 | `match (n) with⏎\| 0 -> Steps.one (n)⏎\| _ -> Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 51 | `match n with⏎\| 0 -> (n)⏎\| _ ->⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 52 | `if (n > 0) then⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 53 | `let v = if n > 0 then (n) else (n)⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 54 | `let f = \x -> (x)⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 55 | `tzroutine {⏎    Routine.yield ()⏎    Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 56 | `tzroutine {⏎    Routine.yield ()⏎}⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 57 | `Steps.add(⏎    ()⏎    Steps.one ` | `add @0` | `add @0` | `one @0` | 4 |
| 58 | `Steps.add(⏎    (n)⏎    n (` | `add @0` | `add @0` | `add @0` | 0 |
| 59 | `Steps.add(⏎    Steps.one(⏎        n)⏎    Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 60 | `let v = Steps.one ((\x -> x) n)⏎Steps.add n (` | `null` | `add @1` | `add @1` | 7 |
| 61 | `let v = Steps.add(⏎    n⏎)⏎Steps.one (` | `null` | `one @0` | `one @0` | 5 |
| 62 | `n⏎Steps.add n (` | `add @1` | `add @1` | `add @1` | 0 |
| 63 | `n⏎Steps.add (n) n` | `add @1` | `add @1` | `add @1` | 0 |
| 64 | `Steps.add(⏎    n⏎    Steps.one n` | `one @0` | `one @0` | `one @0` | 0 |
| 65 | `Steps.add(⏎    n⏎    Steps.one` | `one @0` | `one @0` | `one @0` | 0 |
| 66 | `Steps.add(⏎    n⏎    Steps.one ` | `one @0` | `one @0` | `one @0` | 0 |
| 67 | `Steps.add(⏎    n⏎    n` | `null` | `null` | `null` | 0 |
| 68 | `Steps.add(⏎    n⏎    (n) ` | `null` | `null` | `null` | 0 |
| 69 | `Steps.add(⏎    n⏎    n (` | `null` | `null` | `null` | 0 |
| 70 | `let v =⏎    n⏎    Steps.one (` | `one @0` | `one @0` | `one @0` | 0 |
| 71 | `Steps.yield(⏎    n⏎) + n` | `null` | `null` | `null` | 0 |
| 72 | 全文 `Steps.add n)⏎    Steps.one (` | `add @3` | `one @0` | `one @0` | 16 |
| 73 | 全文 `x)⏎    Steps.add n (` | `null` | `add @1` | `add @1` | 7 |

- **変わった結果の確認**: main と修正後で結果が違うのは 222 か所。表の各文書を読み、次のとおり正しいことを確かめた。
  - `null` からの 189 か所は、すべてカーソルの行の呼び出し（`Steps.o|ne (` から `Steps.one (|` まで、`Steps.a|dd n (` から後、`Steps.y|ield () + n` から `Steps.yield () |+ n` まで。`|` がカーソル）。各文書の最後の行の結果を、
    同じ行を持つ参照（前の行が項で終わる文書。main の結果）と位置ごとに比べ、すべて一致した（10 の CRLF は 11 の CRLF の参照と一致）。
  - `null` でなかった 33 か所: 22（5 か所）・37（3）・39（5）・57（4）は、`name(` の中の行が関数の頭 `Steps.one` を書いた位置で、外側の `add @0` が内側の `one @0` になった（参照 64〜66・70 と同じ）。
    72（16 か所）は括弧の釣り合わない文書で、main の誤った `add @2`・`add @3`（`add` は 2 引数）が、行頭から `Steps.|one` まで（字下げと、頭がモジュール名だけの位置。参照でも `null`）で `null`、`Steps.o|ne` から後で `one @0` になった。
  - 参照と違う結果は、すべて main と同じ結果を残した位置: 22・37・39・57 の字下げと `Steps.` の上、32 の `)` の前、36・58 の行全体と 38 の行（`(n|)` を除く）。どれも `name(` の呼び出しの `add @0`・`yield @0`。
- **corpus の差分**: `tests/fixtures/*` と `examples/*` の 8,000 byte 以下の 191 の `.tz` のすべての位置（400,814 か所）で、main と修正後の違いは 1,914 か所、すべて `null` からで、
  新しい結果の関数はどれもカーソルの行に書かれている（スクリプトで確かめ、抜き出して読んだ。例: `examples/polymorphism/Main.tz` の `score ref point`、`tests/fixtures/param_facts/Main.tz` の
  `copy_elements (ref mut output) (ref input)`）。レビュー前と修正後の違いは 0 か所（この節の変更は、正しいプログラムの corpus の結果を変えない）。
- **判断 R1（停止条件の 2 つ目）**: 複数行にまたがる括弧の中の行の結果が修正の前後で変わる（表の 22・24・26・28〜31・37・39・57・59 と corpus の一部）。チケットの停止条件に当たるが、
  利用者から判断を委ねられたオーケストレーターが、`null` や誤ったシグネチャから正しいシグネチャへの変更は受け入れると決めた。変わるのは `null`・外側の `name(`・括弧の釣り合わない文書の誤った結果から、
  カーソルの行の呼び出しへの変更だけで、main で正しかった結果は残した。
- **判断 R2（関数でない頭のときの `name(` の呼び出し）**: カーソルの行の頭が関数を指さないとき（36 の `n`、`Steps` だけの位置）は、`null` ではなく `name(` の呼び出しを返す。カーソルは `name(` の括弧の中にあり、
  行は呼び出しを書いていないので、main の `add @0` が正しい結果で、それを残すため。
- **判断 R3（直さなかった形）**: main から変わらず、前の行の閉じ括弧が原因でない次の形は直していない（対象外の「複数行にまたがる括弧の中の呼び出しの改善」）。
  34・35（空白の後の `(` の中で、行が自分の呼び出しを書いていない。判断 3）、43（`Steps.add (n,⏎    n) n`。前の行で開いた組がカーソルの行で閉じ、頭が前の行にある。正しくは `add @1`）、
  67〜69（`name(` の中で前の行が項で終わり、行の頭が関数でない。`null` だが、前の行が `()` なら 36・38・58 の `add @0`）。また、`Steps.add(n, ` のように `name(` の中でコンマの後に書く位置は、1 行でも main・修正後とも
  `null` で（loop が深さ 0 のコンマで止まる）、括弧の釣り合った文書では `activeParameter` がコンマを数えることは無い。G12 の走査の問題で、このチケットの範囲ではない。
- **コマンド**: `cargo fmt --all -- --check` 成功。`cargo clippy --locked --all-targets -- -D warnings` 成功。`cargo test --locked --test lsp -- signature_help keyword_member_yield` が修正前 `3 passed; 1 failed`、
  修正後 `4 passed`。`cargo nextest run --locked` が 2936 件すべて成功（4 件 skip。前の 2935 件 + 新しいテスト 1 件）。`cargo build` の後の `node tests/lsp_sessions.mjs target/debug/tsuzuri` が成功（utf-16・utf-8 の session と features の 4 つ）。`node scripts/check-guide-lite.mjs` が 88 件の参照すべて成功、`node tests/repo_kb.mjs` が 84 件成功。
  言語リファレンス・`docs/`・`README.md` のシグネチャヘルプの記述は行の位置による違いを書いていないので変更せず、`check-docs`・`gen-docs.mjs --check`・`check-cli-docs.mjs` は対象のページ・生成物が無いので走らせていない。
- **変更したファイル**: `src/lsp.rs`（`call_context` が呼び出しを内側から順に返す。`signature_help` と、定義を引く `signature`）、`tests/lsp.rs`（新しいテスト 1 件と、前のテストのコメント）、`_features/README.md`（G37 の行）、
  このチケット（D1 の状態・判断 1・判断 3・この節）。
