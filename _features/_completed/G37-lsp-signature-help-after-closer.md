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
- 状態: 既定案（実装者はこの案に従う）。実装では、前の行で開いた `(` の中の行の結果を変えないように精密化した（「実装と検証」の判断 1）。

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
  `cargo build` の後の `node tests/lsp_sessions.mjs target/debug/tsuzuri` が成功（utf-16・utf-8 の session と features の 4 つ）。VS Code 拡張のテストは手順 5 のとおり走らせていない。
- **差分の検査**（追加）: 修正前と修正後の dev のバイナリで、`tests/fixtures/*` と `examples/*` の 176 のプロジェクトの 8,000 byte 以下のすべての `.tz` の、すべての位置（400,803 か所）に
  `textDocument/signatureHelp` を送って比べた。変わったのは 1,914 か所で、すべて修正前の `null` が、カーソルの行の呼び出しのシグネチャになったもの（たとえば
  `assert (point == point)` の次の行の `score ref point`、`tzroutine { … }` の中の `Channel.send sleeper_started ()` の次の行の `host_wait_released ()`）。修正前に `null` でなかった結果は 1 つも変わらない。
  手で作った 10 のプロジェクトでは、`Routine.yield ()` の次の行の各位置の結果が、`let m = 1` の次の行の同じ位置の結果と一致した（`Steps.yield ()` の直後の `activeParameter` 1 も同じ）。
- **判断 1（D1 の精密化）**: チケットの D1 のとおり「深さ 0 で行頭より前の `(` でない token で止まる」判定を `closes` の前に置くと、`Steps.yield(\n        ()\n    ) + n` の 3 行目の `)` の前
  （前の行で開いた `name(` の中で、前の行が `()` で終わる）が、修正前の `yield` から `null` に変わった（手順 2 のテストの最後の 1 通りで確かめた）。これは停止条件の「複数行にまたがる括弧の中の呼び出しの結果が変わる」に当たる。
  そこで、戻る loop は今の判定の順のままにして、深さ 0 で行頭より前の閉じ括弧に最初に会ったところで、その次の token の位置（`boundary`）を覚え、loop が開いたままの `(`（`open`）に届かなかったときだけ、
  `start` を `boundary` にする。前の行で開いた `(` の中の行は、改行が意味を持たないので、修正前と同じ結果になる。それ以外では D1 と同じ結果になり、仕様の 3 つの項（前の行の閉じ括弧で止まる・行頭より前の `(` は開き括弧・
  行の中の閉じ括弧は変えない）を満たす。修正前と結果が変わるのは、修正前の loop が開いた `(` に届かずに前の行の token で止まった場合だけで、そのとき修正前の `tokens[start]` は前の行の括弧の開きになり、結果は `null` だった
  （括弧の釣り合わない編集中の文書を除く）。上の差分の検査もこれと合う。
- **判断 2**: 閉じ括弧の種類として、チケットの 5 通りにリストの `|]`（`TokenKind::RightList`）を足し、`closes` が扱う 5 種類（`)`・`]`・`}`・`|]`・補間の終わり）をすべて覆った。CRLF は落とし穴のとおり 1 通り確かめた。
- **判断 3**: `Steps.yield (` のように空白を空けた `(` で改行した呼び出しの次の行は、修正の前後とも `null` のまま（2 回目の loop で、行頭より前の `yield` で止まるため）。対象外の「複数行にまたがる括弧の中の呼び出しの改善」に当たるので直さず、
  テストにも固定していない（`null` を期待値にすると、正しくない振る舞いを固定するため）。差分の検査で修正の前後が同じであることだけを確かめた。
- **文書**: 変更なし。言語リファレンスと `docs/language.md`・`docs/architecture.md`・`README.md` のシグネチャヘルプの記述（`grep -rn "シグネチャヘルプ\|signature help" _tsuzuri/language-reference docs README.md`）は行の位置による制限を書いていない。
  `_features/README.md` の行を更新した。言語リファレンスのページを変えていないので `check-docs` は走らせていない。
- **変更したファイル**: `src/lsp.rs`（`call_context`）、`tests/lsp.rs`（新しいテスト 1 件と `keyword_member_yield_has_completion_hover_and_signature_help` の source）、`_features/README.md`、このチケット（`_features/_completed/` へ移動）。
