# E19: `DataChannel` の端に使った `Channel.send`・`Channel.recv` の書き換えも、置き換えの名前を確かめる

| 項目 | 内容 |
| --- | --- |
| ID | E19 |
| 優先度 | P3（プロジェクトが `DataChannel` という名前のモジュールかローカルの束縛を持ち、そこで旧い `Channel.send`・`Channel.recv` を `DataChannel` の端に使ったときだけ。確かな修正案がプロジェクトの関数を呼ぶように書き換えるので、`tsuzuri fix` がプログラムの意味を変えうる） |
| 規模 | S（`src/check.rs` の `name_builtin`、`src/diagnostic.rs` の `Detail`、`src/hints.rs` の `renamed_channel_calls`、回帰テスト） |
| 依存 | E18（done） |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-22（E18 の「残る制限」。利用者の依頼で着手し、同じ PR で起票と実装をした） |
| 承認 | 不要（E18 の D2 と同じ規則を、`renamed_to_data_channel` のもう 1 つの出どころにも当てはめる直し） |
| 改善する劣位 | 不具合: `hints.rs` の書き換えは、型の検査の後で字句から `DataChannel.send` を作るので、プロジェクトのモジュールやローカルの束縛が裸の `DataChannel` を持つところでも、そのまま書いていた |
| 手本にする既存実装 | E18 の `Names::removed_written`・`Names::names_std_module`（D2）と、値の位置のローカルの束縛の確かめ |
| 主な影響ファイル | `src/check.rs`, `src/diagnostic.rs`, `src/hints.rs`, `tests/removed_features.rs`, `tests/lsp.rs`, `_tsuzuri/language-reference/compiler/diagnostics.md`, `_tsuzuri/language-reference/built-in-types-and-modules/channel.md`, `docs/language.md`, `docs/architecture.md` |

## 観測

名前空間 `App` に `App/DataChannel.tz`（関数 `recv` だけ）を置き、`def f :: own std::DataChannel.Receiver<i64> -> i64` の本体で旧い `Channel.recv (ref receiver)` と書くと、`main`（`69645da3`）は `E1040`（`'Channel.recv' of a DataChannel.Receiver is now 'DataChannel.recv'`）と確かな修正案 `write 'DataChannel.recv'` を出す。直した `DataChannel.recv` は `App::DataChannel.recv` を指すので、`tsuzuri fix` がプロジェクトの関数への呼び出しに書き換える。`let DataChannel = …` のローカルの束縛でも同じ。

## 仕様

- 検査器は、`Channel.send`・`Channel.recv`（組み込み）の引数の型の誤りに、その呼び出しの位置から `DataChannel`（裸。同じ名前のローカルの束縛がないとき）と `std::DataChannel` が std のモジュールを指すか（`Names::names_std_module`: `using` の曖昧さがなく、`module_path` が std のモジュールに解決する）を `detail.data_channel` に書く（`Checker::data_channel_spellings`）。JSON には書かない。
- 書き換え（`hints::renamed_channel_calls`）は、裸で書いた `Channel` なら、裸の `DataChannel` が指すときは今までどおり、指さず `std::DataChannel` が指すときは `std::` を前に足し、どちらも指さないときは修正案を出さない（`E1040` の診断と help は出す）。`std::Channel` と書いたなら、`std::DataChannel` が指すときだけ書いたまま直し、指さないときは修正案を出さない。メッセージと修正案の題は選んだ書き方で書く。

## 実装と検証（2026-11-22）

`main` `69645da3` で着手した。

- `src/diagnostic.rs`: `Detail::data_channel`（`(裸, std::)`）。
- `src/check.rs`: `Checker::data_channel_spellings` と、`name_builtin` の引数（4 か所の呼び出しは、可変の借用の前に計算する）。
- `src/hints.rs`: 上の仕様の綴りの選択。
- テスト（`tests/removed_features.rs` の `the_channel_call_rewrite_writes_a_data_channel_that_keeps_its_meaning`）: プロジェクトのモジュール `DataChannel` があるときの `std::DataChannel.recv`（メッセージ・題・直したプログラムが通ること）、ローカルの束縛 `DataChannel` があるとき、プロジェクトの名前空間 `std` の `DataChannel` もあって修正案がないとき（`Main` は `DataChannel` を書かず、別の名前空間の関数から受け手を得る）、書いた `std::` がその名前空間に取られて修正案がないとき、何もないときの裸の `DataChannel.recv`。既存の `names_the_data_channel_names`（`std::Channel.recv` を含む）は変わらない。
- 文書: `compiler/diagnostics.md`（`E1040`）、`channel.md`（旧 `Channel` のプログラム）、`docs/language.md`（`E1040` の行）、`docs/architecture.md`（`E1040` の節）。E18 の「残る制限」から本チケットを指した。

### レビュー

独立のレビュー（コンパイラ・文書とテストの 2 つの観点と、指摘ごとの反証）で、プロジェクトの `DataChannel` もローカルの束縛もないプログラムは `tests/`・`examples/` の 541 ファイルの `check --json` が `main` と同じこと、修正案のあるときは直したプログラムが通って std の関数を呼ぶこと、`tsuzuri fix` が 1 ラウンドで収束することを確かめた。指摘を受けて次を直した。

- テストが直接の呼び出しだけで、パイプライン（`|>`）の 2 か所と `Channel.send` を、プロジェクトの `DataChannel`・ローカルの束縛と一緒に通していなかった。両方を足した。
- `channel.md` が、修正案のない場合（プロジェクトの名前空間 `std` も `DataChannel` を持つとき）を書いていなかった。
- E19 より前から: この書き換えの `E1040` のクイックフィックスを、言語サーバーが一度も出していなかった（`main` でも同じ）。`hints::annotate` が `E1003` を `E1040` に変えるのは `DiagnosticSet::counts` を数えた後で、言語サーバーは修正案がそのコードの数を減らすときだけクイックフィックスを出すので、`E1040` の数が 0 のままだった。`Diagnostics::finish` が、集めた診断（表示の上限の 50 件を超えたものも含む）のうち型で書き換えの候補になるもの（`hints::renames_to_data_channel`）の呼び出す名前の位置と関数を `DiagnosticSet::data_channel_candidates` に記録し、`renamed_channel_calls` が、そのすべてに表示の書き換えと同じ字句と出どころの条件（`old_channel_name`、プログラムのソース）を当てて、書き換わる数だけ `counts` の `E1003` から `E1040` へ移すようにし（Copilot のレビュー 2 回で、表示した分だけを移すと 51 件以上でクイックフィックスが出ないこと、型だけで数えると `(+Channel.recv)` のように書き換わらない候補を `E1040` に数えることを直した。`the_counts_follow_every_channel_call_rewrite_past_the_display_limit`・`the_counts_follow_only_the_calls_that_the_rewrite_turns_into_e1040`）、`tests/lsp.rs` の `the_channel_call_rewrite_is_a_quick_fix` で、プロジェクトの `DataChannel` があるときの `write 'std::DataChannel.recv'` を含めて確かめた。
- E19 より前から（別のチケット）: 利用者の共用体と同じ名前の std の組み込みモジュール（`DataChannel`・`Channel`・`Mutex`・`Atomic`）の関数を、`std::` を付けても共用体のケースとして探して `E1002` になる。共用体はモジュールでもローカルの束縛でもないので、E18 D2・E19 の確かめを通り、修正案は裸の名前のままになる（`tsuzuri fix` は `E2008` で止まり、どのファイルも書かない）。[E20](../E20-std-qualified-builtin-beside-user-union.md) にした。

検証（macOS arm64、dev ビルド）: `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（ホストと `x86_64-pc-windows-msvc`）、`cargo nextest run --locked`（全体）、`tests/removed_features.rs`（22 件）・`tests/lsp.rs`、`node scripts/check-docs.mjs`（変えた 2 ページ）。
