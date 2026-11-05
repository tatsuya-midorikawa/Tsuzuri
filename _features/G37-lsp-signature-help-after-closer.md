# G37: LSP のシグネチャヘルプが、閉じ括弧で終わる行の次の行の呼び出しで何も返さない

| 項目 | 内容 |
| --- | --- |
| ID | G37 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | G07, G12 |
| 後続 | – |
| 状態 | todo |
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

- [ ] 「再現」の 4 通りの前の行で、シグネチャヘルプが `yield (_unit: unit) -> i64` を返す。
- [ ] `signature_help_after_a_line_that_ends_with_a_closer` が修正前に失敗し、修正後に成功する。
- [ ] `tests/lsp.rs` の既存のテストが期待値を変えずに成功する。
- [ ] `keyword_member_yield_has_completion_hover_and_signature_help` が、`Routine.yield ()` の次の行の呼び出しで成功する。
- [ ] fmt・clippy・T1 が成功する。

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
- 状態: 既定案（実装者はこの案に従う）。
