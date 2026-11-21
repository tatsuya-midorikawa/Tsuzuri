# E18: `E1040` の旧名の検出を std モジュールへの解決にそろえる（`Channel`・`Net`・`Task`・`Async`）

| 項目 | 内容 |
| --- | --- |
| ID | E18 |
| 優先度 | P3（プロジェクトが自分の名前空間に `Channel`・`Net` という名前のモジュールを置いたときと、`std::` を付けて旧名を書いたときだけ起きる。前者は別のモジュールの関数への「確かな」修正案が出るので、`tsuzuri fix` がプログラムの意味を変える） |
| 規模 | S（`src/check.rs` の `removed_feature` の呼び出し側、`tests/removed_features.rs`、`E1040` の文書） |
| 依存 | E17（done） |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-21（E17 の PR のレビューで見つかった、E17 より前からある不具合。`feat/e17-io-function-names` の `2ffa444f` と `main` の `b6bbdcb5` の両方で再現） |
| 承認 | 不要（E17 が `IO` について決めた規則（D-55 の `E1040`、E17 の仕様 2「利用者定義モジュールとの分離」）を、ほかの旧名にも当てはめる直し） |
| 改善する劣位 | 不具合: プロジェクトのモジュール `Channel`・`Net` に無い関数が `E1040` になり、`Channel.bounded` には std の `DataChannel.bounded` へ書き換える確かな修正案が付く。`std::Channel.bounded`・`std::Net.connect_async` は `E1040` にならず `E1002` になる |
| 手本にする既存実装 | E17 の `Checker::renamed_io_function`（`src/check.rs`。`self.names.module_path(self.module, spelled) == Some("IO")` のときだけ報告する）と、`tests/removed_features.rs` の `a_module_io_of_the_program_keeps_its_unknown_member` |
| 主な影響ファイル | `src/check.rs`, `tests/removed_features.rs`, `src/explain/codes/E1040.md`, `_tsuzuri/language-reference/compiler/diagnostics.md`, `docs/language.md`, `docs/architecture.md` |

## 観測

`check::removed_feature(module, name)` は、書いたモジュール名の綴り（`spelled`）をそのまま照合する。`value_expression` の `ExprKind::Field` の分岐（`removed_name(&spelled, &field.text, whole)`）も、型の `Names::type_head` も、綴りが std のモジュールに解決するかを見ない（E17 は `IO` だけを `module_path` で確かめるようにした）。

1. 名前空間 `App` のプロジェクト（`Tsuzuri.toml` の `namespace = "App"`）に `Tools/Channel.tz`（`namespace App::Tools`、関数 `hello` だけ）を置き、同じ名前空間の `Tools/Use.tz` で `let pair = Channel.bounded 3;` と書くと、`tsuzuri check` は `error[E1040]: 'Channel.bounded' is now 'DataChannel.bounded'` と `fix (certain): write 'DataChannel.bounded'` を出す。正しくは `E1002`（`module 'Channel' has no function or union case 'bounded'`）で、修正案はない。`tsuzuri fix` は、プロジェクトのモジュールへの呼び出しを std の `DataChannel` への呼び出しに書き換えてしまう。
2. 同じ置き方の `Tools/Net.tz` で `Net.connect_async` と書くと、`error[E1040]: 'Net.connect_async' was removed` になる（正しくは `E1002`）。`Task`・`Async` も、プロジェクトのモジュール `Task`・`Async` があればそれが先に解決するので多くは影響しないが、そのモジュールに無い名前（`Task.run` など）は同じく `E1040` になる。
3. `std::Channel.bounded 3` は `error[E1002]: module 'std::Channel' has no function or union case 'bounded'`、`std::Net.connect_async` は `error[E1002]: module 'std::Net' has no function or union case 'connect_async'` になる。`std::` を付けない書き方と同じ `E1040`（help と、`Channel.bounded` なら確かな修正案）になるべき。

## 進め方

1. `Checker::renamed_io_function` と同じ形で、`removed_feature` を聞く前に、書いたモジュールが std のモジュールに解決するかを確かめる。`Channel`・`Net` は `module_path(self.module, spelled)` が `Some("Channel")`・`Some("Net")` のときだけ、モジュールの無くなった `Task`・`Async` は `module_path` が `None` のとき（綴りが `Task`・`Async`・`std::Task`・`std::Async`）だけ報告する。照合には綴りでなく std のモジュール名を渡し、メッセージと修正案の文字列は E17 と同じく書いた綴り（`std::` を含む）で作る。
2. 型の位置（`Channel.Sender`・`std::Channel.Sender`、`Async.Operation`）の `Names::type_head` も同じ条件にする。
3. 回復（`recovering`）で置き換えた名前の式は、E17 と同じく左辺のモジュールの式を保つ（`std::Channel.bounded` の回復が `DataChannel.bounded` を解決すること）。
4. テスト: 観測 1〜3 のそれぞれと、プロジェクトのモジュール `Channel` がその名前を定義していればそれが使われること（今の `a_definition_of_the_program_wins` の型の例を、名前空間の中のモジュールの例にも広げる）。
5. 文書: `E1040.md`・`diagnostics.md` の `E1040`・`docs/language.md` の `E1040` の行・`docs/architecture.md` の `E1040` の節に、プロジェクトのモジュールは対象外で `E1002`、`std::` を付けても同じ `E1040` と書く。

## 改善の候補（同じチケットで扱うか、決めてから進める。→ D1 で扱った）

確かな改名（`renamed_to_data_channel` と E17 の `renamed_io_function`）の回復は `self.poisoned = true` にするので、その関数は `finish` を通らず、`finish` が報告するエラーが `E1040` の陰に隠れる。例: `def show :: (Display<'a>, Capture<'a>) => own 'a -> IO<unit> = \x -> IO.writeln x` と、`main` の中の `let r = ref 3`・`do! show r`・`do! IO.write_line "x"`。`tsuzuri check` は `6:9: error[E1040]` だけを出し、`tsuzuri fix` は `error[E2008]: the fixes would introduce E1013 in 'Main.tz' at line 4 ...; no file was changed` で止まる（`IO.writeln` で書けば `4:17: error[E1013]` がすぐ出る）。これは `tsuzuri fix` の文書（`compiler/usage.md` の `fix`）どおりの保守的な振る舞いだが、置き換えが同じ関数・同じ型のときは毒にしなくてよい。毒にしないなら、`finish` が置き換えた式で正しく動くこと、`removed_name` と `renamed_io_function` の両方を同じにすることを確かめる。

## 受け入れ条件

- [ ] 観測 1・2 のプロジェクトのモジュールに無い名前が `E1002` になり、修正案が付かない。
- [ ] `std::Channel.bounded`・`std::Net.connect_async`・`std::Channel.Sender` が、`std::` を付けない書き方と同じ kind・help の `E1040` になり、`std::Channel.bounded` の確かな修正案が `std::` を保ったまま `tsuzuri fix` の 1 ラウンドで直る。
- [ ] 今の `tests/removed_features.rs` がすべて通る（`Channel`・`Task`・`Async`・`Net` の既存の振る舞いが、上の 2 つ以外で変わらない）。
- [ ] 文書を同じ PR で更新し、`node scripts/check-docs.mjs` を変えたページに通す。

## 対象外

- E17 の `IO` の旧名（E17 で `module_path` の確認を実装済み）。
- 新しい旧名の追加。

## 決定事項

| 番号 | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 「改善の候補」を扱う: 値の位置の確かな改名（`removed_name` の置き換えと E17 の `renamed_io_function`）の回復は、関数を毒にしない（`poisoned` を立てない） | 置き換えは、それ自身の型を持つ正しい式として検査するので、毒にする理由がない（型の位置の改名は前から毒にしない）。`finish` とムーブ・アクセスの検査のエラー（`E1015`、ムーブ後の使用の `E1012`）が `E1040` と一緒に出て、`tsuzuri fix` は改名を適用してそれを残す（以前は `E2008` で止まった）。ただし候補の例の `E1013`（借用と寿命）は、型のエラーがなくなってからの段で出るので、改名がそれを見えるようにするときは今も `E2008` で止まる。これは段の構成で、`tsuzuri fix` の文書どおり | 決定（2026-11-21、利用者の委任） |
| D2 | 置き換えのモジュール（`DataChannel`・`Routine`）は、裸の名前、`std::` 付きの名前の順に、std のモジュールを曖昧さなく指す書き方で書く（`Names::names_std_module`: `using` の曖昧さがなく、`module_path` が std のモジュール、std にモジュールのない `Routine` はどのモジュールにも解決しない）。どちらも指さない（プロジェクトの名前空間 `std` が同じ名前を持つ）ときは、修正案も回復も出さない。メッセージ・回復・修正案とも同じ書き方 | 観測 1 と同じ種類の害（確かな修正案がプロジェクトの関数を呼ぶように書き換える）を、置き換えの側でも防ぐ。レビューで見つかった（E18 より前から）。Copilot のレビューで、`using` の曖昧さ（`module_path` が `None` を返す）、相対の `std::`（プロジェクトの `App::std`）、値の位置の同じ名前のローカルの束縛（`let DataChannel = …` の欄の `bounded` を呼んでしまう）も確かめるようにした | 決定（同上） |
| D3 | `Channel.send`・`Channel.recv` を `DataChannel` の端に使った `E1040`（`hints.rs` の `renamed_channel_calls`）も、メッセージと修正案の題に書いた `std::` を付ける | 文書の「`std::` を付けても、メッセージと修正案も `std::` を保つ」を、この kind のもう 1 つの出どころでも成り立たせる | 決定（同上） |

## 実装と検証（2026-11-21）

`main` `52155373` で着手した。

- `Names::removed_module`（書いたモジュールの解決: `Channel`・`Net` は std のモジュール、`Task`・`Async` はどのモジュールにも解決しないとき。綴りは裸か `std::` だけ）、`Names::removed_qualified`（型の名前）、`Names::removed_written`（診断・メッセージ・置き換えを書いた綴りで作る。D2）、`Names::removed_async_type`（`Async` の型）。値の位置（`value_expression` の `ExprKind::Field`）と型の位置（`Names::type_head`・`type_result`）の両方で使う。`is_namespace` は `std::Task`・`std::Async` も名前空間として扱う。
- D1: 値の位置の回復の 2 か所で `self.poisoned = true` をやめた。D3: `hints.rs` の接頭辞。
- テスト（`tests/removed_features.rs`）: `std::` 付きの旧名 7 つ（kind・メッセージ・範囲・help が裸の書き方と同じ・修正案）、名前空間のモジュール `Channel`・`Net`・`Task`・`Async` の値と型の 6 つ（`E1002`・`E1004`、メッセージ・範囲、修正案なし）と、そのモジュールが定義する関数・型が使われること、`std::Channel` は std のまま、`std::` 付きの `tsuzuri fix` が 1 ラウンド・4 件、D1（`E1015` が一緒に出て、`tsuzuri fix` が改名を適用する）、D2（プロジェクトの `DataChannel`・`Routine` があるときの `std::` 付きの置き換えと、直したプログラムが通ること）、D3（`std::Channel.recv`）。
- 文書: `diagnostics.md`・`channel.md`・`tzroutine.md`・`docs/language.md`・`docs/architecture.md`・`E1040.md`（1500 字の上限にちょうど収めた）。`docs/language.md` の「利用者のモジュール `Async` にない `Async.run` も `E1040`」（TZ13 の記述で、決定の記録はない）を、E18 の規則（`E1002`）に直した。レビューで見つかった、`keywords.md`・`computation-expressions.md` の「`Async { }` は利用者の `Async.tc` がなければ `E1040`」の誤り（ビルダーでないモジュール `Async` があれば `E1018`。E18 より前から）も直した。

検証（macOS arm64、dev ビルド）:

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（ホストと `x86_64-pc-windows-msvc`）。
- `cargo nextest run --locked`（全体）、`tests/removed_features.rs`（21 件）。
- `node scripts/check-docs.mjs`（変えた 5 ページ）。
- 独立のレビュー（コンパイラ・文書とテストの 2 つの観点と、指摘ごとの反証）で、文書の言い過ぎ（`E1012` も後から出るとした誤り、`毒` という内部の用語、見えるファイルの条件）、`E1040.md` の未更新、テストの不足（help・範囲・型の位置の正の例）、D2・D3 の元になった不具合を直した。

### 残る制限

- `Channel.send`・`Channel.recv` を `DataChannel` の端に使った `E1040`（`hints.rs`）の置き換え `DataChannel.send` は、型の検査の後で字句から作るので、プロジェクトのモジュール `DataChannel` が見えるファイルでもそのまま書く（D2 の確かめをしない）。`DataChannel` という名前のモジュールを置き、そこから旧い `Channel.send` を書いたプログラムだけに起きる。→ [E19](E19-data-channel-call-rewrite-spelling.md) で直した（2026-11-22）。
