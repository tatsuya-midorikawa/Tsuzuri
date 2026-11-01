# G32: 開発リポジトリの KB と `ticket-brief`（小さいモデルへ渡す文脈の自動生成）

| 項目 | 内容 |
| --- | --- |
| ID | G32 |
| 優先度 | P0 |
| 規模 | 全体 M（D-50 の表）。Phase 1: M（約 3 日）、Phase 2: M（約 3 日）、Phase 3: S（約 1 日）。Phase ごとに別の PR にしてよい（合計は約 7 日で、D-50 の M は Phase 1〜2 を 1 人で続けたときの見積り） |
| 依存 | なし（第3期のどのチケットの実装も待たない。**最初に実装してよい**）。弱い依存: G23（`kind` が入ったら `diagnostics.jsonl` に `kinds` を足す。「前提とする他チケットのインターフェース」） |
| 後続 | 第3期の全チケットの実装（`brief` が入力になる）。G31（`brief` の出力量を「入力文字数」の基準に使える）、G35（`check-guide-lite` と同じ流儀の検査を足す） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc`（main）で、差は `docs/benchmarks.md` だけ。確認: `git diff --stat a2313d19 HEAD`） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D16 はすべて承認済み |
| 改善する劣位 | 小さいモデルは GUIDE.md（15.5 万字）・チケット（125 件で 412 万字）・`docs/architecture.md`（24.7 万字）を読めず、読んでも要点を拾えない。KB も検索もない（D-50 の調査結論 (4)「文書が 1,000 万字を超え、食い違いを機械で検出できない」。GUIDE-LITE の参照も検査されない） |
| 手本にする既存実装 | 流儀・見出しの slug・フェンスの除き方・`TSUZURI_BIN`: `scripts/check-docs.mjs` の `headings`・`collect`。JS のテスト（一時ディレクトリ・`try/finally`）: `tests/windows.mjs`・`tests/script.mjs`。解析する対象: `tests/features.mjs` の `const suites = {`、`docs/architecture.md` の「ファイル \| 責務」の表、`_features/GUIDE.md` の `### D-NN`・§4.1〜§4.3、`_features/GUIDE-LITE.md`。CI: `.github/workflows/targets.yml`（`permissions`・`concurrency`・`actions/checkout@v7`・`actions/setup-node@v7`）。KB の作法（名前・鮮度・置換）: `_features/GUIDE.md` の D-52（Rust 側の `tz-kb/1`。本チケットは別物で `tz-repo-kb/1`） |
| 主な影響ファイル | Phase 1: `scripts/repo-kb.mjs`（新規）, `scripts/repo-kb/text.mjs`・`repo.mjs`・`markdown.mjs`・`rust.mjs`・`tickets.mjs`・`decisions.mjs`・`modules.mjs`・`tests-index.mjs`・`docs-index.mjs`・`build.mjs`（すべて新規）, `tests/repo_kb.mjs`（新規）, `tests/fixtures/repo_kb/mini/**`（新規）。Phase 2: `scripts/repo-kb/query.mjs`・`brief.mjs`（新規）, `scripts/check-guide-lite.mjs`（新規）。Phase 3: `.github/workflows/repo-kb.yml`（新規）, `AGENTS.md`, `README.md`, `docs/architecture.md`。**変更しない**: `src/`・`tests/*.rs`・既存の `tests/*.mjs`・`scripts/check-docs.mjs`・チケット・`_features/GUIDE*.md` |

## 実装者向け早見

1. **読むもの**: `scripts/check-docs.mjs`（115 行。流儀と slug）、本チケットの「仕様」の決定表 T1〜T9、「設計」の関数一覧。Rust・`src/` は読まない。
2. **最初に書くテスト**: `tests/repo_kb.mjs` の `markdown_headings_ignore_fenced_lines`（手順 2）。以後すべて「テストを先に書き、失敗を見てから実装」。
3. **作るもの**: Node 20 で動く ES module のスクリプト群（外部パッケージなし。使える子プロセスは `git` だけ）。`build` が `target/repo-kb/` に決定的な JSON Lines を書き、`brief <ID>` が 24,000 字以内の文脈パックを標準出力へ出す。
4. **触ってはいけない**: `src/`・既存のテスト・チケット・GUIDE・GUIDE-LITE・`scripts/check-docs.mjs`。GUIDE-LITE の食い違いは**直さず報告する**（`check-guide-lite` の仕事は見つけること）。
5. **出力の約束**: 絶対パス・日時・ホスト名・環境変数を出力に入れない。パスはリポジトリの根からの相対で区切りは `/`。`\r\n` は `\n` に直して読む。配列は決まった順に整列する（文字列は UTF-16 コード単位の昇順）。
6. **使ってはいけない API**（Node 20 にない）: `fs.globSync`・`Object.groupBy`・`Set.prototype.union`・`RegExp.escape`・`Promise.withResolvers`・`import.meta.dirname`（`fileURLToPath(import.meta.url)` を使う）。
7. **Phase 1 の完了**: `build` の標準出力の件数が、「現状」のコマンドを同じ木で実行した値と一致し（125・58 は起票時にコミット済みだった分で、第3期のチケットが入った木では増える）、2 回の実行で全ファイルがバイト単位で同じ、`node tests/repo_kb.mjs` が 30 件で通る。
8. **Phase 2 の完了**: 作業ツリーの全チケットで `brief` が 24,000 字と 16,000 字の両方で上限内、`node scripts/check-guide-lite.mjs` が失敗 0 件（修正後の GUIDE-LITE。件数は「現状」）で終了コード 0、故意に壊した複製で失敗を報告し、`tests/repo_kb.mjs` が 48 件で通る。
9. **Phase 3 の完了**: `repo-kb.yml` が PR で緑、`AGENTS.md` に節が入り、`tests/repo_kb.mjs` が 51 件で通る。
10. 迷ったら「決定事項」→ 最も保守的な選択。停止条件に当てはまったら止めて報告する。

## 目的

**問題。** 第3期のチケットは 1 件が 25〜100 KB（A19 は 75,660 字、G24 は約 9.8 万バイト）で、実装の前提は GUIDE.md（作業ツリーで 154,658 字）・GUIDE-LITE（7,539 字）・D-5x にある。小さいモデルが全部読むと文脈が尽き、読まないと規則を落とす。「どのチケットが何に依存するか」「この診断コードはどこで出るか」「この関数を言及する決定はどれか」を引く手段がなく、GUIDE-LITE の記述が HEAD とずれても気づけない（「現状」に、実在しない名前 `Options::validate` を載せていた例がある）。

**価値。**

- `node scripts/repo-kb.mjs brief G22` が、実装に要る文脈（要点・停止条件・仕様・設計・指定 Phase の手順・テスト・受け入れ条件・依存先のインターフェース・言及する決定・公開シンボル・関連テストのコマンド・GUIDE-LITE）を**24,000 字以内**で、切り捨てた所の取り出し方つきで出す。
- `ticket`・`decision`・`files`・`diag`・`search`・`section` が、索引から 1 コマンドで答える（全文検索はしない）。
- `check-guide-lite` が、GUIDE-LITE の挙げるコマンド・パス・関数名・診断コード・D-n・節番号・チケット ID の存在を検査する。構成が変わってずれると CI が落ちる。

**HEAD で測った規模**（コマンドは「現状」）: チケット 125 件（4,116,735 字）、決定 58 件、`src/**/*.rs` 93 ファイル、診断コードの文字列 879 か所（47 種）、`tests/*.rs` 101・`tests/*.mjs` 80・suite 58、言語リファレンス 111 ページ（約 108 万字）。`build` は 1〜2 秒の見込み（試作の走査が 0.4〜0.7 秒。実測は手順 7 で記録）。

**非目標。**

- LLM・埋め込み・意味検索・全文検索を使わない（索引の項目の部分一致だけ）。
- Rust のコンパイラも、Rust 側の KB（G26〜G28 の `tsuzuri kb`）も変えない。`kb` の名前も使わない（コマンドは `node scripts/repo-kb.mjs`）。
- チケット・GUIDE・GUIDE-LITE・文書を自動で直さない（検査と報告だけ）。Windows の CI は足さない（出力に `\` を出さない設計だけ守る）。
- 利用者向けの言語機能ではない。`_tsuzuri/language-reference/` は更新しない（理由は PR の説明に書く）。

## 着手条件と停止条件

### 着手条件

- 依存なし。承認済み。`node --version` が 20 以上（試作は 20.19.6）、`git` が使える。
- GUIDE §2.3 の Rust の基準コマンドは**不要**（Rust を変えない）。手順 1 で `node scripts/check-docs.mjs _tsuzuri/language-reference/index.md` が通ることだけを控える。
- 「現状」のコマンドの出力を手元で再現する（第3期のチケットが入ると件数は 125 より増える）。

### 停止条件

次の場合は即興で回避せず、止めて状況と選択肢を報告する（GUIDE §13）。

1. 実リポジトリのチケットの 1 件でも、決定表 T2 のメタ表を見つけられない、または同じ ID が 2 件ある。選択肢: A（その形を決定表に足し、テストを足す）、B（そのチケットを直すよう統括者に依頼する）。
2. `build` が 10 秒を超える、または出力が 2 回の実行や Node 20 と 24 でバイト単位で違う。
3. 外部パッケージ（npm）・`unsafe`・Rust の変更・既存のテストの期待値の変更が要ると思った。
4. `brief` が、作業ツリーのチケット 1 件でも `--max-chars 24000` または `16000` を超える（段 6 まで進んでも入らない）。選択肢: A（段の表の上限を下げる。表とテストを直す）、B（`--max-chars` の下限を上げる）。
5. `check-guide-lite` を通すために GUIDE-LITE・GUIDE・チケットを書き換えたくなった。**書き換えず**、失敗の一覧を報告する（直すのは統括者）。
6. `tests/fixtures/repo_kb/`（新規）を足したあと `node tests/parallel_frontend.mjs target/release/tsuzuri` が失敗する。選択肢: A（fixture を `tests/data/repo_kb/`（新規）へ移す）、B（報告）。
7. 実チケットの Phase の見出しが決定表 T4b の形に合わず、Phase の切り分けが 3 件以上で空になる。選択肢: A（表に形を足す）、B（その形を使うチケットの書き方を直すよう依頼）。
8. 出力にホストの絶対パス・日時・`\` が出る。
9. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `9dd0fbbc` で確認。件数は `git ls-files` で数えたコミット済みのもの）

### 数と再現コマンド

```sh
git rev-parse --short HEAD                                              # 9dd0fbbc
T=$(git ls-files _features _perfs | grep -vE '(README|GUIDE[A-Za-z-]*)\.md$')
echo "$T" | wc -l                                                       # 125（_features 2 + _features/_completed 96 + _perfs 9 + _perfs/_completed 18）
echo "$T" | xargs cat | wc -m                                           # 4116735
echo "$T" | xargs grep -l -m1 '^|---|---|$' | wc -l                     # 52  （メタ表の区切りが |---|---|）
echo "$T" | xargs grep -l -m1 '^| --- | --- |$' | wc -l                 # 73  （| --- | --- |）
echo "$T" | xargs grep -h -m1 '^| 状態 | todo |$' | wc -l                # 15  （うち 4 件は _completed/ にある: G13 G16 G17 G18）
grep -c '^### D-[0-9][0-9] ' _features/GUIDE.md                          # 58（作業ツリー。HEAD の blob は 49）
git ls-files 'src/*.rs' 'src/**/*.rs' | wc -l                           # 93
git ls-files 'src/*.rs' 'src/**/*.rs' | xargs grep -ohE '"[EW][0-9]{4}"' | wc -l      # 879
git ls-files 'src/*.rs' 'src/**/*.rs' | xargs grep -ohE '"[EW][0-9]{4}"' | sort -u | wc -l   # 47
git ls-files 'tests/*.rs' | wc -l; git ls-files 'tests/*.mjs' | wc -l  # 101 / 80
git ls-files '_tsuzuri/language-reference/*.md' '_tsuzuri/language-reference/**/*.md' | wc -l   # 111
awk 'NR>412 && NR<2045 && /^  [A-Za-z_][A-Za-z0-9_]*: \{/' tests/features.mjs | wc -l          # 58（const suites の中身）
wc -m _features/GUIDE.md _features/GUIDE-LITE.md docs/architecture.md   # 起票時 154658 / 7539 / 246922（台帳の追記後は約 165000 / 8959 / 246922）
```

### 形の揺れ（全 125 ファイルを走査した結果。決定表 T1〜T9 の根拠）

- **H1**: 125 件すべてが `# <ID>: <題名>` で、ファイル名の ID・H1 の ID・メタ表の `ID` 行が一致する。ID は `^[A-Z]{1,2}[0-9]{2}$`（A 19・B 9・C 11・D 12・E 14・F 13・G 20・PB 7・PM 9・PR 8・PX 3）。
- **メタ表**: ヘッダは常に `| 項目 | 内容 |`。区切りは `|---|---|` が 52 件、`| --- | --- |` が 73 件。H1 の直後に空行がなく表が始まるものが 42 件、H1 と表の間に段落があるもの（PX01）。表の直後に空行がなく `## 目的` が続くものが 1 件。行の名前（件数）: `ID`・`優先度`・`規模`・`依存`・`状態` が 125、`主な影響ファイル` 113（＋`主な影響ファイル（実装時）` 2）、`後続` 98、`起票`・`承認` 71、`手本にする既存実装` 61、`改善する劣位` 38、`計測対象` 28、`分類`・`関連` 27、`実装の基準` 12、`利用者向け仕様` 10。
- **規模**: 121 件が `S`/`M`/`L`/`XL` だけ。4 件は注釈つき（例: `XL（Phase 1 は L）`・`L（Phase 1 は M）`）。作業ツリーの G24 は `全体 L（Phase 1: M、…）`（先頭が「全体」）。
- **状態**: `todo` 15、`done` 73、`done（…）` 37（注釈つき。例: `done（Phase 1・2）`）。**`_completed/` にあるのに `todo` のままの 4 件**（G13・G16・G17・G18。各ファイルに `## 実装と検証（2026-10-08）` がある。README の一覧は `done`）。
- **依存**: `–`（12）・`なし`（7）・`ID`・`ID, ID`・`(ID)`（弱い依存。README の Mermaid の破線）・`弱依存 ID`・`ID（注釈）`・`[ID](リンク)（注釈）`・`。` の後に注記が続く A18・A19・G24。**README の Mermaid の辺 104 本（破線 10）は、試作の解析器が 104/104 で存在と弱い印の両方を再現した**（`-->`／`-.->` を独立の正解として使った）。解析結果の依存先 ID はすべて実在するチケット。
- **セクション名**（H2。括弧の注釈を除く）: `受け入れ条件` 123・`設計`/`実装手順`/`テスト計画`/`ドキュメント`/`落とし穴`/`対象外` 各 115・`目的` 114・`仕様` 88・`決定事項` 62・`着手条件と停止条件` 61・`未決事項` 54。**`仕様` がない 37 件**は `確定した仕様`・`確定した API と意味`（`目的と実装範囲` の組）や、性能チケットの `目標と指標` を持つ。H3 の `前提とする他チケットのインターフェース` が 89 件、`他チケットへの提供インターフェース` が 49 件。`実装手順` の下は `### 手順 N: …` が 648 見出し（64 チケット）、`Phase N` で始まる見出しは A17・B06 の 4 つだけ。作業ツリーの G24 は `### Phase N: …` の下に太字の `**手順 N: …**`。
- **`主な影響ファイル` のトークン**（115 チケット）: ファイル 1,839（HEAD に実在 1,664）、ディレクトリ 27（実在 18）、glob 18（すべて 1 件以上に一致）、それ以外の記号・略記 281。実在しないのは計画中の新規ファイル（A18・A19）と旧 `_docs/` のパス 40 と相対の略記。
- **D-n**: `### D-NN <題名>` が 58 件、番号は 2 桁で 01〜58 に欠番・重複なし、コードフェンスの中に `### D-` はない。本文は箇条書き・表（D-16・D-30・D-50・D-55）・コードブロックが混ざる。
- **表**: `docs/architecture.md` の先頭の表 `| ファイル | 責務 |`（75 行）、GUIDE の §4.1（21 行）・§4.2（9 行）・§4.3（35 行）。第 1 列は `` `a/b.rs` `` 1 つか、` / ` でつないだ複数（`` `src/control.rs` / `recursion.rs` ``。2 つ目以降の `/` なしの名前は直前のディレクトリ）。glob（`heap-*.ll`）、ディレクトリ（`std/`）、行末の説明（`` `…` の `tz.realloc` ``）も混ざる。試作で全行を解決し、実在しないパスは 0。
- **診断コード**: 文字列 `"E1012"` 形は 879 か所。うち `#[cfg(test)] mod` の中が 122、**テスト以外が 757（46 種、41 ファイル）**。`#[cfg(test)] mod` はファイルの末尾とは限らない（`src/check.rs` は 2 つ目のテスト mod の後に通常のコードが続く。`src/syntax_codec.rs` はテストの文字列に列 0 の `}` がある）ので、行の字下げでの判定は誤る。
- **`tests/features.mjs`**: `const suites = {`（412 行目）から `};`（2045 行目）に suite が 58 個、各キーは 2 字下げ。`input:` を持つのは 1 個（`storage`）。fixture は `tests/fixtures/<input の値。なければ suite 名>` で、58 個とも実在。`traps` 33・`inspect` 39・`cases` 56。64/80 の `tests/*.mjs` が `process.argv[2]` でコンパイラを受け取る。
- **CI**: `.github/workflows/` は `targets.yml`（4 ホストの重い行列。Node 24）・`vscode.yml`・`windows.yml` の 3 つ。Node のスクリプトだけを回す軽い job はない。
- **GUIDE-LITE の参照**（試作の検査器。付録 A）: 修正後の `GUIDE-LITE.md`（8,959 字）は 99 件を調べて**失敗 0 件**。修正前は 80 件で失敗 1 件だった: `Options::validate` は HEAD に実在しなかった（型は `BuildOptions`）。統括者が GUIDE-LITE と GUIDE §6.8 を 2026-10-18 に直した。調査の途中は `A20` のチケットがなく 2 件だったが、第3期のチケットが揃って消えた。完了の基準は件数でなく、「直したあとの GUIDE-LITE で失敗 0 件」と「故意に壊した複製の失敗を報告する」こと。
- **試作**（捨てた。付録 A）: Rust の走査器は 93 ファイルを 0.4 秒で走査し、テスト以外の診断コード 757 か所（46 種）を得た。`#[cfg(test)]` の判定は、波括弧を数える方式（採用）が、字下げ方式（76 行ずれる）と「最も近い `fn` が `#[test]`」方式（2 件漏れる）より正確（付録 A.3）。

## 変えてはいけない意味

- `src/`・`tests/*.rs`・既存の `tests/*.mjs`・`scripts/check-docs.mjs`・チケット・`_features/GUIDE*.md`・`docs/*`・`README.md`（Phase 3 の追記を除く）は 1 byte も変えない（Phase 1・2）。`cargo test`・既存の `node tests/*.mjs` の結果は変わらない。
- 新しい CLI・診断コード・環境変数・予約語・crate・`unsafe` を Rust 側に足さない。名前 `kb`・`explain`・`guide` は使わない（D-55）。
- `target/repo-kb/` は `.gitignore` の `target` に含まれる（コミットされない）。ほかのディレクトリへ書かない（`--out` で指定されたときだけ）。
- `build` と照会は、ネットワーク・子プロセス（`git ls-files` を除く）・書き込み（`--out` の下以外）を使わない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `build`・`verify`・索引 6 種（`tickets`・`decisions`・`modules`・`diagnostics`・`tests`・`docs`）とそのテスト | M | なし | JSON Lines だけで `jq`・`grep` から使える。ほかのコマンドがなくても価値がある |
| 2 | `brief`・`ticket`・`section`・`decision`・`files`・`diag`・`search`、`check-guide-lite` とそのテスト | M | Phase 1 | `brief` は Phase 1 の索引だけに依存し、CI への配線がなくても手元で使える |
| 3 | CI の job（`repo-kb.yml`）、`AGENTS.md`・`README.md`・`docs/architecture.md` の追記、実リポジトリへの総合テスト | S | Phase 2 | 既存の挙動に触らない追記だけ |

## 仕様

### CLI

```text
node scripts/repo-kb.mjs <コマンド> [引数] [共通オプション]
```

| コマンド | 引数 | 働き | Phase |
| --- | --- | --- | --- |
| `build` | なし | 索引 6 種と `manifest.json` を `--out` に書く。標準出力に 1 行 `repo-kb: tickets=125 decisions=58 modules=… diagnostics=… tests=… docs=… warnings=4 -> target/repo-kb`（実際の件数） | 1 |
| `verify` | なし | 入力の SHA-256 を計算し直して `manifest.json` と比べる。同じなら `repo-kb: fresh`（終了コード 0）、違う・無いなら `repo-kb: stale (run: node scripts/repo-kb.mjs build)`（終了コード 1） | 1 |
| `brief` | `<ID>` | 文脈パックを標準出力へ（「`brief` の出力」） | 2 |
| `ticket` | `<ID>` | チケットのメタ情報・依存先の状態・逆向きの依存・見出しの索引 | 2 |
| `section` | `<ID> <名前>` | チケットの 1 節の全文。名前は役割名（`quick`・`purpose`・`start`・`current`・`frozen`・`spec`・`design`・`steps`・`tests`・`docs`・`accept`・`pitfalls`・`nongoals`・`decisions`・`needs`・`gives`）か、見出しの文字列（括弧の注釈を除いたもの。完全一致） | 2 |
| `decision` | `<D-n または n>` | 決定の題名・本文・言及するチケットと診断コード。`D-45`・`d-45`・`45`・`D-5`（`D-05` として）を受ける | 2 |
| `files` | `<パス>` | そのパスを言及するチケットと決定。パスが `/` で終わるときは前方一致 | 2 |
| `diag` | `<コード>` | 診断コード（`E1012`・`W1006`）の発行箇所・言語リファレンスの表の行・言及するページ・チケット・決定 | 2 |
| `search` | `<語>...` | 索引の項目（チケットの ID・題名・見出し、決定の題名、診断の説明、モジュールの説明、テスト名、文書の見出し）の部分一致。語は空白区切りで**すべて含む**もの。大文字小文字を区別しない。最大 40 件 | 2 |

共通オプション: `--root DIR`（既定は `scripts/` の親。テスト用）、`--out DIR`（既定は `<root>/target/repo-kb`）、`--no-refresh`（古い KB を作り直さず、終了コード 1 で止める）、`--json`（`ticket`・`decision`・`files`・`diag` だけ。索引の行の JSON をそのまま 1 行で出す）。`brief` のオプション: `--phase N`（既定 1）、`--max-chars N`（既定 24000。下限 16000・上限 400000）、`--no-lite`（GUIDE-LITE を付けない）、`--with 役割[,役割…]`（`frozen`・`pitfalls`・`decisions`・`docs`・`current`・`nongoals` を追加する。既定は付けない）。`section` のオプション: `--phase N`（Phase の切り分けを使う。既定は使わない）。

| 終了コード | 意味 |
| --- | --- |
| 0 | 成功 |
| 1 | 見つからない（ID・決定・コード）、`verify` が古い、`--no-refresh` で古い、`brief` が段 6 まで進んでも `--max-chars` に入らない |
| 2 | 使い方の誤り（未知のコマンド・オプション、引数の不足・過剰、`--max-chars` の範囲外、数値でない値） |
| 3 | 失敗（読めない `--root`、`--out` が `.repo-kb` のない空でないディレクトリ、チケット ID の重複、`git` が使えず walk も失敗） |

エラーは標準エラーに `repo-kb: <メッセージ>` の 1 行（診断 JSON ではない）。警告は `repo-kb: warning: <slug> <対象>: <説明>`（`build` が出し、`manifest.json` の `warnings` にも入る）。標準出力に出すのは結果だけ。

**鮮度。** `build` と `verify` 以外のコマンドは、最初に `ensureFresh` を呼ぶ。`<out>/manifest.json` がなく、または `input_sha256` が計算し直した値と違えば、標準エラーに `repo-kb: rebuilding (stale)` を出して `build` を実行してから続ける（`--no-refresh` なら終了コード 1）。

### 入力

`--root` の下で次の集合を入力とする（相対パス・`/` 区切り）。

| 集合 | 内容 |
| --- | --- |
| チケット | `_features/**/*.md` と `_perfs/**/*.md`（決定表 T1 で分類） |
| 決定 | `_features/GUIDE.md` |
| 表 | `docs/architecture.md`、`_features/GUIDE.md`（先頭の列見出しが `ファイル` の表すべて） |
| Rust | `src/**/*.rs` |
| テスト | `tests/*.rs`・`tests/*.mjs`・`tests/features.mjs`（suite）・`tests/fixtures/` の 1 階層 |
| 文書 | `_tsuzuri/language-reference/**/*.md`・`docs/*.md`・`README.md` |
| 自分自身 | `scripts/repo-kb.mjs`・`scripts/repo-kb/*.mjs`（どちらも新規。鮮度の対象。解析器を直すと KB が作り直される） |

ファイルの一覧は `git ls-files --cached --others --exclude-standard -z`（`cwd` が `--root`）で作る。`git` が失敗したら、`.git`・`target`・`node_modules` を除いてディレクトリを再帰で歩く。どちらも `/` 区切りの相対パスを UTF-16 コード単位の昇順に整列する。1 ファイルが 8 MiB を超えたら読まず、警告 `file_too_large` を出す。読むときは UTF-8 で読み、`\r\n` を `\n` にする。

**入力のハッシュ。** `input_sha256` は、次の行を `\n` でつないだ文字列の SHA-256（16 進小文字）。(1) 一覧の全ファイルについて `F <パス>`、(2) 上の表の入力ファイルについて `H <ファイル内容の SHA-256> <パス>`。どちらもパスの昇順。

### 決定表 T1: ファイルの分類（チケット）

`_features/` と `_perfs/` の `*.md` を再帰で集め、ファイル名（`/` より後）で分ける。

| ファイル名 | 分類 |
| --- | --- |
| `README.md`・`GUIDE.md`・`GUIDE-LITE.md` | 対象外（警告なし） |
| `^([A-Z]{1,2}[0-9]{2})-appendix(-[a-z0-9][a-z0-9-]*)?\.md$`（**この行を先に判定する**。`G23-appendix-hints.md` のような分割名がある） | 付録。ID のチケットの `appendices` に入る |
| `^([A-Z]{1,2}[0-9]{2})-[a-z0-9][a-z0-9-]*\.md$` | チケット。`id` は括弧の 1 |
| それ以外 | 対象外。警告 `file_skipped <パス>` |

`completed` は、パスに `/_completed/` を含むとき真。同じ `id` が 2 つ以上あれば終了コード 3（`dup_ticket_id`）。H1 は 1 行目が `^# ([A-Z]{1,2}[0-9]{2}): (.+)$` に一致すること。一致しない、または ID がファイル名と違えば警告 `ticket_id_mismatch`（`id` はファイル名の ID、`title` は H1 の `# ` 以降）。

### 決定表 T2: メタ表の探し方

2 行目以降で、ヘッダ行が `| 項目 | 内容 |`（前後の空白は無視）で、次の行が `^\|\s*-{3,}\s*\|\s*-{3,}\s*\|$` の**最初の表**。H1 との間に空行があってもなくても、段落があってもよい。表は `|` で始まる連続した行（空行・非 `|` 行で終わる）。各行は、**エスケープされていない `|`**（`\|` は `|` に戻す）で分け、最初のセルが名前、残りのセルを `|` でつないだものが値。名前の末尾の `（実装時）` は取り除く。同じ名前が複数あれば最初を使う。表がなければ警告 `ticket_table_missing`（索引には `id`・`title`・`path`・`completed` だけが入り、ほかは `null`／空）。

### 決定表 T3: メタ表の値

| 名前 | 規則 | 結果（キー） | 不正なとき |
| --- | --- | --- | --- |
| `優先度` | 値が `^P[0-3]$` | `priority` | 警告 `ticket_field_invalid priority`、`null` |
| `規模` | 値の中の最初の `/(?<![A-Za-z])(XL\|S\|M\|L)(?![A-Za-z])/` | `size`。値がそのトークンだけでなければ `size_note` に値（200 字まで）、そうでなければ `""` | 警告、`size` は `null` |
| `状態` | 値の先頭が `todo` か `done` | `state_cell`（その語。ほかは `unknown`）。`state_raw` は値の先頭 160 字 | `unknown` なら警告 `ticket_field_invalid state` |
| 場所と状態 | `completed` が真なら `state = "done"`、偽なら `state = state_cell` | `state` | `completed` と `state_cell` が食い違う（`completed` で `todo`、または未完了で `done`）と警告 `state_conflict <ID>: cell '<語>', but the file is <under _completed/ / not under _completed/>`（**場所を優先**） |
| `依存` | 決定表 T3a | `deps`・`deps_raw`（値の先頭 600 字） | `ID` 以外の名前が欠ける（`ID`・`優先度`・`規模`・`依存`・`状態`）と警告 `ticket_field_missing <名前>` |
| `後続` | 値をそのまま | `followers_raw`（先頭 600 字。なければ `""`） | — |
| `主な影響ファイル` | 決定表 T3b | `files` | — |

`followers`（逆向きの依存）は索引に持たず、`ticket` と `brief` が `deps` から計算する（`deps` に自分の ID を持つチケット。ID の昇順）。

### 決定表 T3a: 依存のセル

アルゴリズム D（擬似コード）:

```
parseDeps(cell, known):                       # known = 索引の全チケット ID の集合
  s = cell の [文](URL) を 文 に置換
  if s.trim() が "–" "-" "なし" "" のどれか: return []
  depth = 0; cut = -1
  for i, c in s:                              # 全角・半角の括弧を数える
    if c in "(（": depth++
    elif c in ")）": depth = max(0, depth-1)
    elif c == "。" and depth == 0: cut = i; break
  list = s[:cut] (cut<0 なら s 全体);  note = s[cut+1:] (cut<0 なら "")
  stripped = list の全角括弧（…）（入れ子なし）を同じ長さの空白に置換
  deps = 順序つき map
  for m in /(?<![A-Za-z0-9])([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])/g in stripped:
    before = stripped[max(0, m.index-6) : m.index]
    weak = before が "(" で終わる または /弱依存\s*$/ または /弱い依存[:：]\s*$/ に一致
    deps.setdefault(m[1], weak)               # 最初に現れた判定を使う
  for m in /弱い依存[:：]\s*\[?([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])/g in note:
    deps.setdefault(m[1], true)
  for id in deps: if id not in known: 警告 dep_unknown <ID>: <id>  （depsには入れない）
  return deps を [{id, weak}] の配列で（出現順）
```

| セルの断片（HEAD の実例） | 結果 |
| --- | --- |
| `–`・`なし`（19 件） | `[]` |
| `E02, B01` | E02・B01（強い） |
| `(A01)`・`A11, (A01)` | A01 は弱い |
| `G02, 弱依存 A03, 弱依存 E01（`W1002` の非公開判定）` | G02 強い、A03・E01 弱い（`W1002` は 4 桁なので ID にならない） |
| `F02, E05, B01（`Gpu.request` が `Result` を返すため）` | 3 件とも強い（括弧の注釈は無視） |
| `A15（done）、G19（done。edition）、(A17)。A17 は弱い依存: …`（A18） | A15・G19 強い、A17 弱い。`。` の後の `[A19](…)` は注記なので依存にならない |
| `[D11](_completed/D11-…md)（done。…）、…。弱い依存: [A17](…)（…）。[A18](…) とは…（D16）…`（A19） | D11・A15・B07・F10・C10 強い、A17 弱い。A18 と `D16`（決定の番号）は依存にならない |
| `G22 の Phase 1（…）。… 弱い依存: G23（…）`（G24） | G22 強い、G23 弱い |

### 決定表 T3b: 影響ファイルのセル

セルの `[文](URL)` を `文` に直し、バッククォートで囲まれたトークンを順に分類する。結果は `files: [{"path","exists"}]`（重複は最初だけ。最大 200）。

| トークン | 分類 | `path`・`exists` |
| --- | --- | --- |
| `^[A-Za-z0-9_./-]+\.(rs\|mjs\|md\|toml\|json\|jsonl\|yml\|yaml\|tz\|tt\|tc\|ll\|c\|h\|sh\|py\|ts\|mts\|cs\|cpp\|txt\|lock\|spvasm\|js)$` で、`/` を含む、または `Cargo.toml`・`Cargo.lock`・`README.md`・`AGENTS.md`・`LICENSE` | ファイル | そのまま。`exists` は一覧にあるか |
| `^[A-Za-z0-9_./-]+/$` | ディレクトリ | そのまま。一覧に前方一致するものがあれば `exists` |
| `*` と `/` を含み空白なし | glob | `*` は `[^/]*`。一致した一覧のファイルを 1 件ずつ（`exists` 真）。一致なしなら glob の文字列を `exists` 偽で 1 件 |
| `/` を含み、`::` を含まず、`接頭辞{a,b}接尾辞` の形 | brace | 展開した各ファイル（上のファイルの規則） |
| ほか（例: `Checker::name`・`FRONTEND_FORMAT`・`### Map / Set`） | 記号 | 無視 |

### 決定表 T4: 見出し・セクション・役割

**Markdown の走査**（`markdown.mjs`）: 行を上から読み、`^ {0,3}(`{3,}|~{3,})` でフェンスを開閉する（閉じは同じ文字で、開きと同じか長く、後ろが空白だけ）。フェンスの中の行はすべての規則から除く。見出しは `^(#{1,6}) (.+)$`（setext は扱わない）。見出しごとに `level`・`title`・`line`（1 始まり）・`end`（次の同じ以下のレベルの見出しの前の行。なければ最終行）・`chars`（`line`〜`end` の文字数。コードポイント）を持つ。文字数はすべてコードポイント数（`[...s].length`）。

**正規化。** `name = title` の最初の `（` より前を `trim` したもの。

**役割の表**（上から順に最初に見つかった名前の、**レベル 2 の見出し**を使う。`needs`・`gives` はレベル 2 か 3）:

| 役割 | 名前（優先順） |
| --- | --- |
| `quick` | 実装者向け早見 |
| `purpose` | 目的、目的と実装範囲 |
| `start` | 着手条件と停止条件 |
| `current` | 現状、現状と計測 |
| `frozen` | 変えてはいけない意味 |
| `spec` | 仕様、確定した仕様、確定した API と意味、実装仕様、現在の構文 |
| `design` | 設計 |
| `steps` | 実装手順 |
| `tests` | テスト計画 |
| `docs` | ドキュメント |
| `accept` | 受け入れ条件 |
| `pitfalls` | 落とし穴 |
| `nongoals` | 対象外 |
| `decisions` | 決定事項、未決事項、決定事項の結果 |
| `needs` | 前提とする他チケットのインターフェース |
| `gives` | 他チケットへの提供インターフェース、他チケットへ提供するインターフェース、後続に提供するインターフェース |

セクションの本文は、見出し行から `end` の行まで（末尾の空行を除く）。

### 決定表 T4b: Phase の切り分け

`brief --phase N` と `section --phase N` が、取り出した本文から他の Phase の部分を除く。本文を、レベル 3 と 4 の見出し（フェンスの外）で**ブロック**に分ける（最初の見出しの前は「前文」）。

| セクション | 規則 |
| --- | --- |
| `steps`（`実装手順`） | **粘着**: 見出しのテキストが `^Phase\s*([0-9]+)` に一致するブロックで現在の Phase `cur = そのN` に変わり、続く Phase 以外の見出し（`### 手順 N` を含む）は `cur` に属す。前文と、最初の `Phase` の見出しの前は Phase 0（共通。常に出す） |
| `spec`・`design`・`tests`・`docs` | **粘着しない**: `^Phase\s*([0-9]+)` に一致するブロックだけがその Phase。ほかのブロックは Phase 0（常に出す） |

N が Phase 0 または `--phase` の値のブロックだけを残す。除いたブロックごとに 1 行 `[Phase K omitted: <見出し> (<文字数> chars)]` を残す。`--phase` の既定は 1。Phase の見出しが 1 つもないチケット（旧チケット）は全文を出す。

### 決定表 T5: 言及の抽出

本文（チケット全体・決定の見出しと本文）から次を集める。すべて整列し重複を除く。

| キー | 正規表現・規則 | フィルタ | 上限 |
| --- | --- | --- | --- |
| `tickets` | `(?<![A-Za-z0-9-])([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])` | 索引のチケット ID のみ（`A64`・`D16` のような偶然の一致を除く）。**自分自身は除く**。ただし決定 D-45 の「旧 D10」のように、決定の番号とチケット D10 が衝突しうる（近似であり、`brief` は使わない） | 100 |
| `decisions` | `D-([0-9]{2,3})(?![0-9])`、`D-aa〜D-bb`・`D-aa–D-bb`・`D-aa~D-bb` は両端を含めて展開（最大 60） | 決定の索引にある `D-NN`（2 桁に 0 埋め） | 60 |
| `diags` | `\b([EW][0-9]{4})\b` | なし | 100 |
| `files` | バッククォートのトークンのうち T3b の「ファイル」「ディレクトリ」の形 | 一覧に存在するもののみ | 300 |

### 決定表 T6: 決定（`GUIDE.md`）

フェンスの外の `^### (D-[0-9]{2,3}) (.+)$` が決定の見出し。本文は次の `^#{1,3} `（フェンスの外）の前の行まで。`id` は `D-NN`、`title` は括弧の注釈を含む見出しの残り。番号が重複する、または 1 から最大までに欠番があれば警告 `decision_gap <番号>`。`text` は本文（見出し行を除き、前後の空行を除く）。`tickets`・`diags`・`decisions`・`files` は T5（見出し＋本文。`decisions` は自分を除く）。

### 決定表 T7: 表（`modules.jsonl`）

対象のファイル（`docs/architecture.md`・`_features/GUIDE.md`）から、フェンスの外の Markdown 表（`|` で始まる行と、次行が `^\|\s*:?-{3,}`）のうち**ヘッダの最初のセルが `ファイル`** のものを集める（HEAD は 4 つ: architecture 1、GUIDE §4.1〜§4.3）。セルの分割は T2 と同じだが、**バッククォートの中の `|` は分けない**。第 1 列のセルは ` / ` で分け、各断片の**最初のバッククォートのトークン**を取る。

| トークン | 規則 |
| --- | --- |
| `/` を含む | そのまま。以後の「`/` を含まない」断片の基準ディレクトリになる（最後の `/` までの部分） |
| `/` を含まない（`recursion.rs`・`File.tz`） | 基準ディレクトリ + トークン（基準がなければそのまま） |
| `/` で終わる | ディレクトリ（`dirs`） |
| `*` を含む | glob（T3b と同じ展開。一致なしは `missing`） |
| 一覧にない | `missing` に入れ、警告 `table_path_missing <パス> (<表>)` |

行の `text` は、第 1 列以外のセルを ` / ` でつなぎ、空白を 1 つにたたみ、600 字で切ったもの。

### 決定表 T8: Rust の走査

`rust.mjs` の `scanRust(text)` は、1 回の走査で**マスクしたコード**（コメントと文字列・文字リテラルの中身を、改行を残して空白に置換した同じ長さの文字列）と**文字列リテラルの一覧**を返す。

| 先頭の形 | 扱い |
| --- | --- |
| `//` | 行末までコメント |
| `/*` | 入れ子を数えて `*/` までコメント |
| `r"`・`r#"`・`r##"`…・`br"`…（直前が識別子の文字でない） | raw 文字列。`"` + `#` の同数で閉じる |
| `"`・`b"`（直前が識別子の文字でない） | 通常の文字列。`\` の次の 1 文字を飛ばし、`"` で閉じる（複数行を含む） |
| `'` + `(\\(x[0-9a-fA-F]{2}\|u\{[0-9a-fA-F_]+\}\|.)\|[^\\'\n])` + `'` | 文字リテラル（これに一致しない `'a` はライフタイム。そのまま） |

`annotate` は、マスクしたコードを 1 回なぞり、波括弧のスタックに `fn`・`impl`（`trait` を含む）・`mod`・`block` を積む。`fn 名前` の後ろを `(`/`[` の深さで数え、深さ 0 の `;` が先なら本体なし、深さ 0 の `{` が先なら `fn` を積む。`impl` は、ヘッダ（`{` か `;` まで）から先頭の `<…>` を除き、`for` があればその後ろの最初の識別子（`::` の最後の部分）を型名とする。`#[cfg(test)]`（マスク後のコードで）に続く `mod 名前 {` はテスト領域として積み、その中はすべて `test`。文字列リテラルごとに、その位置のスタックから `function`（最も内側の `fn`。内側に `impl` があれば `型::名前`）と `test` を決める。

**診断の箇所**: 文字列リテラルの値が `^[EW][0-9]{4}$` に完全一致し、`test` でないもの。`{code, path, function, line}`。HEAD の期待（試作）: 757 か所、46 種、`function` が `null` のもの 0。例: `E1012` は `src/ownership.rs` の `Checker::access`（3）・`Checker::read_places`（4）・`Checker::eval_value`（3）、`src/check.rs` の `Checker::value_expression`、`src/closures.rs` の `lower_expression`、`src/closures_rec.rs` の `lower_group`。

**公開項目**（`brief` の公開シンボル）: マスクしたコードの行のうち `^\s*pub(\([^)]*\))?\s+(const\s+\|async\s+\|unsafe\s+\|extern\s+("[^"]*"\s+)?)*(fn\|struct\|enum\|trait\|type\|const\|static\|mod\|union)\s+([A-Za-z_][A-Za-z0-9_]*)` に一致し、`test` でないもの。出力は**元の行**の前後の空白を除いたもの（140 字まで。超えたら 137 字 + `...`）。HEAD: `src/` 全体で 1,356 行。

### 決定表 T9: テストと文書の索引

| 行の `kind` | 対象 | 規則 |
| --- | --- | --- |
| `rust_test` | `tests/*.rs`（101） | `count` は `#[test]` の行数（マスクしたコードの `^\s*#\[test\]`）、`names` は直後の `fn` 名（300 まで）、`command` は `cargo test --locked --test <stem>` |
| `mjs_test` | `tests/*.mjs`（80） | `compiler_arg` は本文に `argv[2]`、`helper` は別の `tests/*.mjs` が `from "./<名前>.mjs"` で読むもの、`command` は helper なら `""`、`compiler_arg` なら `node tests/<名前>.mjs target/release/tsuzuri`、なければ `node tests/<名前>.mjs`。`tests/features.mjs` も 1 行 |
| `suite` | `tests/features.mjs` の `const suites = {` から最初の `^};$` まで | 2 字下げの `^  ([A-Za-z_][A-Za-z0-9_]*): \{` が suite 名。次の suite までの `^    input: "([^"]+)"` で `input`。`fixture = tests/fixtures/<input の値。なければ suite 名>`（なければ警告 `suite_fixture_missing`）。`traps`・`inspect`・`cases` は `^    traps:`・`^    inspect(`・`^    cases:` の有無。`command` は `node tests/features.mjs target/release/tsuzuri <名前>` |
| 文書 | `_tsuzuri/language-reference/**/*.md`（`kind: reference`）・`docs/*.md`（`docs`）・`README.md`（`readme`） | `title` は最初の H1、`chars`、`headings`（レベル 2〜3。最大 200）に `anchor`（`check-docs.mjs` の `headings` と同じ slug: 小文字化 → `[^\p{L}\p{M}\p{N}\s_-]` を除く → 空白を `-` → 重複は `-1`・`-2`） |

**診断の索引**（`diagnostics.jsonl`）: 次のいずれかに現れるコードの集合を行にする: T8 の箇所、`_tsuzuri/language-reference/compiler/diagnostics.md` の表の行（`^\| \`([EW][0-9]{4})\` \| (.+) \|$`）、チケット・決定・文書の言及。`meaning` は表の行の 2 列目（なければ `""`）、`pages` は文書で言及するページ（最大 30）、`tickets`・`decisions` は言及。

### 出力ファイルとキー

`<out>/` に次の 8 つ（`.repo-kb` はマーカーで内容は `tz-repo-kb/1\n`）。JSON Lines は 1 行 1 オブジェクト、キーは**下の順**、`JSON.stringify` で書き（改行は `\n`）、行は主キーの昇順。数値は整数。値のない配列は `[]`、文字列は `""`、ないものは `null`。

| ファイル | 1 行のキー（順） |
| --- | --- |
| `manifest.json` | `schema`（`"tz-repo-kb/1"`）・`input_sha256`・`inputs`（`H` 行の数）・`counts`（`tickets`・`appendices`・`decisions`・`modules`・`diagnostics`・`tests`・`docs`）・`warnings`（文字列の配列。整列） |
| `tickets.jsonl` | `id`・`title`・`path`・`completed`・`priority`・`size`・`size_note`・`state`・`state_cell`・`state_raw`・`deps`（`[{"id","weak"}]`）・`deps_raw`・`followers_raw`・`files`（`[{"path","exists"}]`）・`refs`（`{"tickets","decisions","diags","files"}`）・`appendices`・`chars`・`headings`（`[{"level","title","line","end","chars"}]`。レベル 2〜3。最大 300） |
| `decisions.jsonl` | `id`・`title`・`line`・`end`・`chars`・`text`・`tickets`・`diags`・`decisions`・`files` |
| `modules.jsonl` | `source`・`table`（その表の通し番号 1〜）・`row`・`files`（`[{"path","exists"}]`）・`dirs`・`missing`・`text` |
| `diagnostics.jsonl` | `code`・`meaning`・`site_count`・`sites`（`[{"path","function","line"}]`。最大 100。`path`・`line` の昇順）・`pages`・`tickets`・`decisions` |
| `tests.jsonl` | 共通 `kind`・`path`・`name`。`rust_test`: `count`・`names`・`command`。`mjs_test`: `compiler_arg`・`helper`・`command`。`suite`: `input`・`fixture`・`fixture_exists`・`traps`・`inspect`・`cases`・`command`（`kind` の順は `rust_test`・`mjs_test`・`suite`、同じ kind は `path`／`name` の昇順） |
| `docs.jsonl` | `path`・`kind`・`title`・`chars`・`headings`（`[{"level","title","anchor"}]`） |

**書き込み。** `<out>` が存在し、空でなく、`.repo-kb` がなければ終了コード 3。作るときは各ファイルを `<名前>.tmp` に書いて `rename` する（同じディレクトリ内）。最後に `manifest.json` を書く（`verify` はこれを見る）。

**警告の slug（閉じた集合）**: `file_skipped`・`file_too_large`・`ticket_id_mismatch`・`ticket_table_missing`・`ticket_field_missing`・`ticket_field_invalid`・`state_conflict`・`dep_unknown`・`decision_gap`・`table_path_missing`・`suite_fixture_missing`。`dup_ticket_id` は警告でなくエラー（終了コード 3）。HEAD の期待: `state_conflict` が 4 件（G13・G16・G17・G18）だけ。

### `brief` の出力

`brief <ID> [--phase N] [--max-chars B] [--no-lite] [--with 役割…]` は、標準出力へ次の部分を**この順**に出す。区切りは `=== … ===` の 1 行（行頭 `=== `、行末 ` ===`）。見出しや案内は英語、取り込んだ本文は原文のまま。文字数はコードポイントで数え、出力全体（区切り行・案内を含む）が `B` 以下。

| 順 | 区切り行 | 内容 |
| --- | --- | --- |
| 0 | `=== REPO-KB BRIEF v1: <ID> phase=<N> max-chars=<B> ===` | 続く 4 行: `<題名>`、`state: <state> \| priority: <P> \| size: <size> \| file: <path>`、`depends: <ID> <state>[ (weak)], …`（なければ `depends: (none)`）、`followers: <ID>, …`（なければ `(none)`）、`affects: <path>[ (new)], …`（最大 20、超えたら ` … N more`）。続けて `=== HOW TO USE THIS PACK ===` と固定の 3 行（下の「固定文」） |
| 1 | `=== CONTENTS ===` | 部分ごとに 1 行 `<key> <見出し> <表示文字数>/<全文字数> <full\|shortened\|omitted>`（最大 24 行） |
| 2〜8 | `=== TICKET <ID> / <role>: <見出し> (<path>:<開始行>-<終了行>) ===` | 役割 `quick`・`start`・`spec`・`design`・`steps`・`tests`・`accept` の本文（T4・T4b）。ない役割は出さず、CONTENTS に `omitted`（`absent`）と書く。`--with` の役割は `accept` の後に指定順で |
| 9 | `=== INTERFACES ===` | `needs` の節（自分の「前提とする他チケットのインターフェース」）と、強い依存先で `gives` の節を持つものの本文（`=== INTERFACE PROVIDED BY <ID> ===` で区切る） |
| 10 | `=== DECISION D-NN <題名> (_features/GUIDE.md:<行>) ===` | チケットの本文（メタ表の値と取り出した節）で**最初に言及された順**の決定（T5） |
| 11 | `=== PUBLIC SYMBOLS <path> ===` | `files` のうち実在する `.rs` ごとに、公開項目（T8）。チケット本文に名前が出るものを先に、残りは行の昇順。1 ファイル 10 件まで。各行 `L<行> <署名>` |
| 12 | `=== TESTS AND COMMANDS ===` | 関連テスト（下の規則）ごとに `<コマンド>   # <根拠>`（最大 12 行） |
| 13 | `=== GUIDE-LITE (_features/GUIDE-LITE.md) ===` | 全文。ファイルがなければこの部分を出さず CONTENTS にも書かない |
| 14 | `=== END ===` | 最後の行 |

**固定文**（`HOW TO USE THIS PACK` の 3 行。そのまま）:

```text
1. Read the ticket parts in order; GUIDE-LITE at the end holds the rules every ticket assumes.
2. A line starting with "[shortened" or "[omitted" names the command that prints the missing text; do not open whole tickets or GUIDE.md first.
3. Report anything that contradicts the repository instead of improvising (GUIDE-LITE section 9).
```

**関連テスト**: `files` と `refs.files` のうち `tests/<名前>.rs` は `cargo test --locked --test <名前>`、`tests/<名前>.mjs` は `tests.jsonl` の `command`（helper は出さない）、`tests/fixtures/<dir>/` は `fixture` がその `<dir>` の suite の `command`。本文のバッククォートのトークンが suite 名に完全一致するものも suite の `command`。実在しないパスは `# <path> (new)`。重複は除く（出現順）。

**切り詰め。** `cut(text, cap)` は、`text` が `cap` 字以下ならそのまま。超えるときは**行単位**で `cap` 字以内に入る行までを残し（最初の行が `cap` を超えるなら先頭の `cap` 字）、次の 1 行を足す: `[shortened: <残した字数> of <全体の字数> chars; rest: node scripts/repo-kb.mjs section <ID> <役割>]`（決定は `decision D-NN`、公開シンボルは `files <path>`、GUIDE-LITE は `[shortened: …; rest: _features/GUIDE-LITE.md]`）。落とした部分が丸ごと（上限 0）のときは `[omitted: <全体の字数> chars; …同じ形…]` 1 行だけ。

**段の表**（`B` に入るまで、0 から 6 へ段を進める。各段は前の段の上限を上書きする。空欄はそのまま。`cap` は字数。`—` は上限なし）:

| 段 | quick | start | spec | design | steps | tests | accept | INTERFACES | DECISION | SYMBOLS | COMMANDS | GUIDE-LITE |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | — | — | — | — | — | — | — | 6,000 | 8 件・全文 | 4,000 | 1,200 | 全文 |
| 1 | | | | | | | | 2,500 | 4 件・各 1,500 | 0 | | |
| 2 | | | 9,000 | 9,000 | | 3,000 | | 1,200 | 2 件・各 1,200 | | | |
| 3 | | | 5,000 | 4,000 | 6,000 | 1,500 | 2,000 | 0 | 0 | | 600 | |
| 4 | | 3,500 | 2,500 | 2,000 | 4,500 | 800 | 1,200 | | | | | |
| 5 | 2,000 | 1,500 | 800 | 600 | 1,800 | 400 | 600 | | | | 300 | |
| 6 | | | | | | | | | | | | 残りの字数（3,000 以上。足りなければ終了コード 1） |

段 `k` の上限で全体を組み立て直し、長さが `B` 以下になった最初の段を出力する。**試作の見積り**（作業ツリーの 149 件に表を当てた。付録 A.4）: `B = 24000` で全件が入り、段 0〜6 の件数は 9・11・6・16・62・45・0。`B = 16000` でも全件が入り（段 6 が 113 件）、`B = 60000` では 126 件が段 0。`B = 12000` は 149 件のどれも入らない（`--max-chars` の下限を 16000 にした理由）。実装後は同じ確認を `brief_real_repo_all_tickets_default_and_16000` が行う。

### 照会コマンドの出力（text。`--json` は索引の行）

| コマンド | 出力（1 行ずつ。順序固定） |
| --- | --- |
| `ticket A19` | `A19 <題名>`／`path: <path>`／`state: <state> \| priority: <P> \| size: <size>[ (<size_note>)] \| completed: yes\|no`／`depends: <ID> <state>[ (weak)]; …`／`followers: <ID>; …`／`files: <path>[ (new)]; …`（最大 30、超えたら ` … N more`）／`appendices: <path>; …`／`headings:` の後に `  <level> <title> L<line>-L<end> (<chars> chars)`（レベル 2〜3） |
| `decision D-45` | `D-45 <題名>`／`source: _features/GUIDE.md L<line>-L<end> (<chars> chars)`／`mentions: tickets <ID> …; diags <コード> …; decisions <D-NN> …`／`---`／本文 |
| `files <path>` | `tickets:` の後に `  <ID> (<path>) cell`（影響ファイルの表）か `mention`（本文）、`decisions:` の後に `  D-NN mention`、`modules:` の後に `  <source> table <n> row <m>`。見つからなければ各見出しの下に `  (none)` |
| `diag E1012` | `E1012 <meaning または (no row in compiler/diagnostics.md)>`／`sites (<n>):` の後に `  <path>:<function> L<line>`／`pages: <path>; …`／`tickets: <ID> …`／`decisions: D-NN …`（なければ `(none)`） |
| `search <語>` | `<kind>\t<key>\t<text>`（`kind` は `ticket`・`decision`・`diagnostic`・`module`・`test`・`doc`。`text` は 100 字まで）。ID・コードの完全一致を先頭に、残りは `kind`・`key` の昇順。0 件でも終了コード 0 で `(no matches)` |
| `section <ID> <名前>` | `=== SECTION <ID> <見出し> (<path>:<開始行>-<終了行>, <n> chars) ===` の後に本文（見つからなければ終了コード 1 と `repo-kb: no such section: <名前> (have: <役割と見出し>)`） |

### `check-guide-lite`（Phase 2）

`node scripts/check-guide-lite.mjs [--root DIR] [--file PATH]`（既定 `_features/GUIDE-LITE.md`）。KB を書かずに、メモリ上で `collect` した索引（チケット・決定・診断・ファイル一覧・Rust のマスク済みコード・`tests/*.mjs` の本文・GUIDE.md の見出し）と照らす。フェンスの中は ```` ```sh ```` の行だけを C3 で調べ、ほかのフェンスは読まない。

| 種別 | 抽出 | 規則 | 失敗の理由（`FAIL <種別> <トークン> — <理由> (line N)`） |
| --- | --- | --- | --- |
| C1 ファイル | バッククォートの `^[A-Za-z0-9_.][A-Za-z0-9_./-]*\.(rs\|mjs\|md\|sh\|toml\|yml\|yaml\|py\|ll\|c\|json\|jsonl\|tz\|tt\|tc\|lock)$` で `/` を含む（`target/`・`/tmp/` で始まる、`<>$=*{}` を含むものは除く） | 一覧にある | `file does not exist` |
| C1b 裸の名前 | 同じ形で `/` がなく拡張子が `rs`・`mjs`・`sh`・`ll` | 一覧のどれかの basename | `no file with this name` |
| C2 ディレクトリ | `^[A-Za-z0-9_.][A-Za-z0-9_./<>-]*/$`（`<` があれば手前まで） | 一覧に前方一致 | `directory does not exist` |
| C3 コマンド | バッククォートが `cargo `・`node `・`npx `・`sh `・`python3 `・`git ` で始まる、または ```` ```sh ```` の各行（` #` 以降を除く） | 引数のうち `/` を含み `-` で始まらないものを C1・C2 と同じに | 同上 |
| C4 診断コード | `^[EW][0-9]{4}$` | `diagnostics.jsonl` に、発行箇所があるか `meaning` が空でない行がある | `not emitted in src and not in the language-reference table` |
| C5 決定 | 本文の `D-NN` | 決定の索引にある | `no such decision in GUIDE.md` |
| C6 節番号 | `§\s*(\d+(\.\d+)?)` | GUIDE.md に `^## N\. ` か `^### N\.M ` があり、見出しが `SECTION_KEYWORDS[N]` を含む | `no such heading in GUIDE.md`／`heading '<見出し>' does not contain '<語>'`／`no keyword registered in SECTION_KEYWORDS` |
| C7 リンク | `[文](URL)` | 相対パスのファイルが存在。`GUIDE.md#断片` は、GUIDE.md の見出しの slug（T9 と同じ）に断片がある | `target file does not exist`／`no heading with this slug in GUIDE.md` |
| C8 記号 | `^([A-Z][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)$`、`^[A-Z][A-Z0-9_]{3,}$`、`^[a-z][a-z0-9]*(_[a-z0-9]+){2,}$` | Rust のマスク済みコード（`src/**/*.rs`・`tests/*.rs`）に: 型は `(struct\|enum\|trait\|type\|union)\s+<型>\b` か `impl…<型>`、項目は `(fn\|const\|static)\s+<名>\b` か識別子としての出現（列挙子）／定数は `(const\|static)\s+<名>\b`／関数・テスト名は `fn\s+<名>\b` か `tests/*.mjs` の文字列 | `type <T> not found`／`item <i> not found`／`no const/static with this name`／`no fn or mjs mention` |
| C9 チケット ID | 全文の `(?<![A-Za-z0-9-])([A-Z]{1,2}[0-9]{2})(?![A-Za-z0-9])` | チケットの索引にある | `no such ticket` |

`SECTION_KEYWORDS = {"1": "絶対に守る規則", "3.1": "テスト選択", "6.7": "診断コード", "6.9": "走査箇所", "8": "ドキュメント更新規約", "11.1": "落とし穴", "12": "構文の早見表", "13.1": "止めて報告する条件"}`（GUIDE-LITE が引く 8 節）。**`PLANNED = ["guide/"]`**（G21 が作る予定のディレクトリ。C2 の対象で、実在しない間だけ許す。**実在するようになったら `now exists; remove it from PLANNED` で失敗させる**＝許可リストの腐敗を防ぐ）。出力は失敗の行のあと `check-guide-lite: <n> references checked, <m> failed`。終了コード 0（失敗なし）・1（失敗あり）・2（使い方）。**HEAD の期待（試作）**: 統括者の修正後の GUIDE-LITE で 99 件を調べて失敗 0 件（`check-guide-lite: 99 references checked, 0 failed`。件数は GUIDE-LITE を直すと変わる）。修正前の GUIDE-LITE（`Options::validate` を載せていた版）の複製では失敗 1 件（`FAIL symbol Options::validate — type Options not found`）。

### 決定性・上限・互換

- **決定性**: 同じ入力から、`build` の 8 ファイルも `brief` の出力もバイト単位で同じ。日時・乱数・絶対パス・環境変数・ロケール（`localeCompare`・`toLocaleString`）を使わない。整列は `a < b`（UTF-16 コード単位）の比較。ファイルの走査順に依らない（一覧は整列してから使う）。
- **上限**: 1 ファイル 8 MiB、チケット 2,000 件、`headings` 300、`refs` は T5 の表、`sites` 100、`search` 40 件、`brief` の `--max-chars` は 16,000〜400,000、`affects` 20、関連テスト 12、公開シンボルは 1 ファイル 10 件。再帰は使わない（Markdown と Rust の走査は反復）。
- **互換**: 開発用ツールで、`docs/stability.md` の対象外。キーと出力形式は `schema`（`tz-repo-kb/1`）で管理する: 項目を足すだけなら番号を保ち、読み手は知らないキーを無視する。形を変える・消すときは `tz-repo-kb/2` にする。Rust・IR・ABI・WASM・診断コード・edition に影響しない。

## 設計

### ファイルと公開関数（すべて新規。ES module。`export` する名前）

| ファイル（すべて新規） | 公開する名前（引数 → 結果） |
| --- | --- |
| `scripts/repo-kb.mjs`（新規） | `main(argv)`（引数解析・`ensureFresh`・各コマンドの呼び出し・終了コード）。`process.exitCode` を設定し、`process.exit()` を呼ばない |
| `scripts/repo-kb/text.mjs`（新規） | `readText(path)`（UTF-8、`\r\n`→`\n`）、`cp(s)`（コードポイント数）、`take(s, n)`（先頭 n コードポイント）、`sha256(buf)`、`cmp(a, b)`（`a<b?-1:a>b?1:0`）、`jsonl(rows)`（各行 `JSON.stringify(row) + "\n"`）、`slug(title, seen)` |
| `scripts/repo-kb/repo.mjs`（新規） | `listFiles(root)`（T の入力一覧。`git` か walk）、`inputFiles(files)`、`inputHash(root, files)`、`globToRegExp(glob)`、`expandCell(token, ctx)`（T3b・T7 の共通の展開） |
| `scripts/repo-kb/markdown.mjs`（新規） | `parseMarkdown(text)` → `{ lines, headings }`、`tables(md)` → `[{ line, header, rows }]`、`splitRow(line)`、`sectionLines(md, heading)` |
| `scripts/repo-kb/rust.mjs`（新規） | `scanRust(text)` → `{ code, literals }`、`annotate(text)` → `[{ value, line, function, test }]`、`diagSites(text)`、`publicItems(text)` |
| `scripts/repo-kb/tickets.mjs`（新規） | `ROLES`（T4 の表）、`roleHeading(md, role)`、`parseTicket(ctx, path, completed)` → 行と警告、`parseDeps(cell, known)`、`parseFilesCell(cell, ctx)`、`filterPhase(lines, phase, sticky)` |
| `scripts/repo-kb/decisions.mjs`（新規） | `parseDecisions(text)`、`mentions(text, ctx)`（T5） |
| `scripts/repo-kb/modules.mjs`（新規） | `parseModuleTables(path, text, ctx)` |
| `scripts/repo-kb/tests-index.mjs`（新規） | `indexTests(ctx)`、`parseSuites(text)` |
| `scripts/repo-kb/docs-index.mjs`（新規） | `indexDocs(ctx)` |
| `scripts/repo-kb/build.mjs`（新規） | `collect(root)` → `Kb`（メモリ上の全索引と警告）、`build(root, out)`、`loadKb(out)`、`ensureFresh(root, out, { refresh })` |
| `scripts/repo-kb/query.mjs`（新規） | `ticketText(kb, id)`・`decisionText`・`filesText`・`diagText`・`searchText`・`sectionText`（文字列を返す。見つからなければ `null`） |
| `scripts/repo-kb/brief.mjs`（新規） | `STAGES`（段の表）、`brief(ctx, kb, id, opts)` → 文字列、`cut(text, cap, pointer)` |
| `scripts/check-guide-lite.mjs`（新規） | `checkGuideLite(ctx, kb, file)` → `{ checked, failures }`、CLI の `main` |

`Ctx = { root, files, fileSet, ticketIds, decisionIds }`（読み取り専用）。`Kb = { manifest, tickets, decisions, modules, diagnostics, tests, docs }`（各行は「出力ファイルとキー」の形のオブジェクト）。

### `collect` の手順

1. `files = listFiles(root)`、`fileSet`、`inputs = inputFiles(files)`、`input_sha256 = inputHash(...)`。
2. チケット: T1 で分類。**第 1 パス**で全チケットの `id` を集め（重複は終了コード 3）、`ticketIds` を作る。**第 2 パス**で `parseTicket`（T2・T3・T4 の見出し・T5 の `refs`）。付録を持つチケットは `appendices` を足す。
3. 決定: `parseDecisions(GUIDE.md)`。`decisionIds` を作ってから `mentions` を各行に付ける。チケットの `refs.decisions` はここで確定する（第 2 パスの後で `decisionIds` を使う）。
4. 表: `parseModuleTables`。5. Rust: `src/**/*.rs` を `annotate` し `diagSites` を集める。6. テスト: `indexTests`。7. 文書: `indexDocs`。
8. 診断: T9 の「診断の索引」の集合を作り行にする。
9. `warnings` を整列し、`manifest.counts` を数える。

計算量は入力の大きさに比例（試作の走査は 4.2 MB の Rust で 0.4 秒）。メモリは入力の数倍。

### `brief` の手順（擬似コード）

```
brief(ctx, kb, id, o):
  t = kb.tickets[id] or return null
  md = parseMarkdown(readText(t.path))
  body[role] = lines of the T4 section; if role in {spec,design,tests,docs}: filterPhase(.., o.phase, sticky=false);
               steps: filterPhase(.., o.phase, sticky=true)
  decisions = mentions in [t の meta 値, 全 body] を最初に出た順に（kb.decisions にあるものだけ）
  for stage in 0..6:
    caps = STAGES[0..stage] を左から上書きしてマージ
    out = render(caps)                       # 区切り行・案内・CONTENTS を含む全文
    if cp(out) <= o.maxChars: return out
  fail(exit 1, "cannot fit in --max-chars")
render(caps): 順 0..14 の部分を cut(.., caps[key]) で作り、CONTENTS は作った結果の字数から作る（CONTENTS 自身は最大 24 行なので固定幅で先に確保）
```

## 前提とする他チケットのインターフェース

- **なし**（依存なし）。ただし次を使う（どれも既存）: GUIDE の決定 D-50〜D-58 の見出しと表（本文を読むだけ）、第3期のチケットの形（ブリーフの構成。`## 実装者向け早見`・`## 前提とする他チケットのインターフェース`・`### Phase N: …`）。
- **G23（`kind`）**: 完了後、`diagnostics.jsonl` の行に `kinds`（その診断コードに属す kind の配列。`tsuzuri explain --catalog` の出力から作る）を**末尾に足す**（`schema` は `tz-repo-kb/1` のまま。足すだけの変更）。それまでは足さない。
- **G27・G28（Rust 側の `tsuzuri kb`）**: 別の物。本チケットは `kb` サブコマンドも `target/tz/kb` も作らない。名前の衝突を避けるため、出力先は `target/repo-kb`、`schema` は `tz-repo-kb/1`。
- **G31**: `brief` の出力の文字数を「入力文字数」として使える（G31 が `repo-kb brief` を呼ぶ方式を足すかは G31 が決める。本チケットは何もしない）。

## 実装手順

各手順の最後で `node tests/repo_kb.mjs`（その時点の全テスト）が緑。テストを先に書いて失敗を見る。1 手順の変更は 300 行以内。出力に絶対パスを出さない。

### Phase 1: build と索引

### 手順 1: ベースライン

- 変更: なし。
- 内容: `node --version`（20 以上）。`node scripts/check-docs.mjs _tsuzuri/language-reference/index.md` の出力を控える。「現状」のコマンドを実行して件数を控える。`mkdir -p /tmp/tz-G32 && git ls-files > /tmp/tz-G32/files-before.txt`。
- 確認: すべて通る。

### 手順 2: 共通部品

- 変更: `scripts/repo-kb/text.mjs`・`scripts/repo-kb/repo.mjs`・`scripts/repo-kb/markdown.mjs`（すべて新規。以降 `scripts/repo-kb/` の中は名前だけ書く）、`tests/repo_kb.mjs`（新規。`test(name, fn)` の補助と、引数 1 つ目による名前の部分一致の絞り込み）、`tests/fixtures/repo_kb/mini/`（新規）。
- 内容: T4 の「Markdown の走査」・T2 の表分割・T9 の slug・T7 のバッククォートを意識した分割。付録 B の No.1〜4 を先に書いて失敗させる（`slug_matches_check_docs` は `scripts/check-docs.mjs` の `headings` と実ページ 3 つで比べる）。
- 確認: `node tests/repo_kb.mjs` が `4 tests passed`。

### 手順 3: Rust の走査器

- 変更: `rust.mjs`、`tests/fixtures/repo_kb/mini/src/check.rs`・`src/lib.rs`（新規。付録 B.4）。
- 内容: T8 のとおり（`scanRust`→`annotate`→`diagSites`・`publicItems`）。付録 B の No.5〜9 を先に書く。
- 確認: `node tests/repo_kb.mjs` が緑。実リポジトリで `diagSites` を全 `src` に回すと、テスト以外 757・46 種・`function: null` が 0（付録 A.3 のコマンド）。

### 手順 4: チケットの解析

- 変更: `tickets.mjs`、fixture のチケット 5 件（付録 B.1）。
- 内容: T1〜T5。付録 B の No.10〜19 を先に書く（`deps_table_forms` は T3a の表の全行、`files_cell_forms` は T3b の表の全行）。
- 確認: 緑。**実リポジトリで** `parseTicket` を全チケットに回し、警告が `state_conflict` の 4 件だけ（G13・G16・G17・G18）で、依存先 ID がすべて実在すること（失敗したら停止条件 1）。

### 手順 5: 決定・表・テスト・文書の索引

- 変更: `decisions.mjs`・`modules.mjs`・`tests-index.mjs`・`docs-index.mjs`、fixture（GUIDE の抜粋・architecture の表・`tests/`・言語リファレンス 2 ページ）。
- 内容: T6（決定）・T7（表）・T9（テストと文書の索引）。付録 B の No.20〜25 を先に書く。
- 確認: 緑。実リポジトリで: 表 4 つ（architecture 75 行・§4.1 21 行・§4.2 9 行・§4.3 35 行）・`missing` 0、suite 58・`fixture_exists` がすべて真、文書 111 ページ＋`docs/*.md`＋README。

### 手順 6: `build`・`verify`

- 変更: `build.mjs`、`scripts/repo-kb.mjs`（`build`・`verify`・`ensureFresh` まで）。
- 内容: 「出力ファイルとキー」・書き込み（`.repo-kb`・`.tmp`→`rename`・`manifest.json` を最後）・終了コード。付録 B の No.26〜29 を先に書く。
- 確認: 緑。

### 手順 7: 実リポジトリでの確認

- 変更: `tests/repo_kb.mjs`（No.30 `build_real_repo_invariants`）。
- 内容: `node scripts/repo-kb.mjs build` を実行し、標準出力の件数を「現状」と照合する。
- 確認: `time` が 10 秒未満（記録する）、2 回の実行で `target/repo-kb` の全ファイルが同じ（`cmp`）、`TSUZURI_*` の環境変数を変えても同じ、`--out` を別にしても内容が同じ、Node 24 でも同じ（`npx --yes --package=node@24 node scripts/repo-kb.mjs build --out target/repo-kb24` と `diff -r`）。**停止条件 2 に注意**。

### Phase 2: brief・照会・ドリフト検査

### 手順 8: 照会コマンド

- 変更: `scripts/repo-kb/query.mjs`（新規）、`scripts/repo-kb.mjs`（`ticket`・`section`・`decision`・`files`・`diag`・`search`・共通オプション・`--json`）。
- 内容: 「照会コマンドの出力」の形。付録 B の No.31〜36 を先に書く。
- 確認: 緑。

### 手順 9: Phase の切り分けと節の取り出し

- 変更: `tickets.mjs`（`filterPhase`・`extractRole`）。
- 内容: T4b。付録 B の No.37〜38 を先に書く。
- 確認: 緑。作業ツリーの G24 で `section G24 steps --phase 1` が `### Phase 1` のブロックだけを含み、`--phase 2` が `### Phase 2` のブロックだけを含む。

### 手順 10: `brief`

- 変更: `brief.mjs`（新規）、`scripts/repo-kb.mjs`（`brief`）。
- 内容: 「`brief` の出力」と「`brief` の手順（擬似コード）」。付録 B の No.39〜45 を先に書く。
- 確認: 緑。**実リポジトリの全チケット**で `B = 24000` と `16000` の両方で出力が `B` 以下（停止条件 4）。

### 手順 11: `check-guide-lite`

- 変更: `scripts/check-guide-lite.mjs`（新規）。
- 内容: 「`check-guide-lite`」の C1〜C9 と `PLANNED`。付録 B の No.46〜48 を先に書く。
- 確認: 緑。実リポジトリで実行し、失敗の一覧を**そのまま報告する**（GUIDE-LITE は直さない。停止条件 5）。

### Phase 3: CI と文書

### 手順 12: CI

- 変更: `.github/workflows/repo-kb.yml`（新規。付録 C）。
- 内容: 付録 C の yml を置く。`check-guide-lite` の step は、手順 11 の確認が緑になってから足す（失敗が残るなら、その step を足さず報告する）。
- 確認: `node tests/repo_kb.mjs && node scripts/repo-kb.mjs build --out target/repo-kb && node scripts/repo-kb.mjs verify --out target/repo-kb` が手元で緑。

### 手順 13: 文書

- 変更: `AGENTS.md`（付録 D の節）、`README.md`・`docs/architecture.md` の検証コマンドの sh ブロックに 3 行を足す。
- 内容: 「ドキュメント」の表のとおり。
- 確認: `node scripts/check-docs.mjs` が従来どおり通る。

### 手順 14: 総合

- 変更: `tests/repo_kb.mjs`（No.49〜51 `cli_exit_codes`・`outputs_have_no_absolute_paths_or_timestamps`・`node_version_gate`）。
- 内容: 付録 B の No.49〜51 を足す。
- 確認: `node tests/repo_kb.mjs` が 51 件で緑。Node 20 と 24 の両方。`node tests/parallel_frontend.mjs target/release/tsuzuri` が従来どおり通る（停止条件 6）。

## テスト計画

`tests/repo_kb.mjs`（新規。`node tests/repo_kb.mjs [名前の部分文字列]`）。fixture は `tests/fixtures/repo_kb/mini/`（付録 B）。期待値はすべて、決定表の行と fixture の文面から**手で書く**（コードの出力を写さない）。全テストの名前と入力・期待は付録 B の表。件数: Phase 1 が 30、Phase 2 が 48（累計）、Phase 3 が 51（累計）。

- **実リポジトリのテスト**（件数に依存せず不変条件だけ）: `チケット ≥ 125`・`決定 ≥ 58`・`id` 一意・全 `deps` が実在・`warnings` の slug が `state_conflict` のみ（`ticket_field_missing` を含む他の slug は 0）・全チケットで `brief` が 24,000 と 16,000 に入る・同じ入力で 2 回の `brief` が同じ。
- **既存テストへの影響: なし**。根拠: Rust・既存のテスト・スクリプトを変えない。`tests/fixtures/repo_kb/` は `.tz` を持たないので、`tests/parallel_frontend.mjs` が `tests/fixtures` の全ディレクトリに回す `check` は `E2001`（`Main.tz` がない）で終わり、スレッド数による差は出ない（試作で `tsuzuri check` の結果が決定的なことを確かめた）。手順 14 で再確認する。

## ドキュメント

利用者向けの言語機能ではないので、`_tsuzuri/language-reference/` と `docs/language.md` は**更新しない**（AGENTS.md の規則に従い、PR の説明に「開発用スクリプトのみ。利用者から見える挙動の変更なし」と書く）。

- `AGENTS.md`: 「For AI agents working on tickets」の節（付録 D。英語）。
- `README.md`: 検証コマンドの sh ブロック（`node scripts/check-docs.mjs` の行の近く）に `node tests/repo_kb.mjs`・`node scripts/check-guide-lite.mjs`・`node scripts/repo-kb.mjs build` を足す。
- `docs/architecture.md`: 「検証コマンド」の sh ブロックに同じ 3 行。
- `_features/GUIDE-LITE.md` §1（読む順序）の先頭に「`node scripts/repo-kb.mjs brief <ID>` の出力を最初に読む」を足す案は、**統括者が編集する**（実装者は編集せず、報告に案を書く）。
- 検証: `node scripts/check-docs.mjs README.md` は対象外（`_tsuzuri` 配下のみ）。代わりに `node scripts/check-guide-lite.mjs` と `node tests/repo_kb.mjs`。

## 受け入れ条件

**Phase 1**

- [ ] `node scripts/repo-kb.mjs build` が 10 秒未満で、「現状」の件数（チケット・決定・表・suite・文書）と一致し、`warnings` が `state_conflict` 4 件だけ。
- [ ] 2 回の `build` で 8 ファイルがバイト単位で同じ（Node 20・24）。絶対パス・日時を含まない（`grep -rE '/Users/|/home/|20[0-9]{2}-[0-9]{2}-[0-9]{2}T' target/repo-kb` が 0 件）。
- [ ] `node tests/repo_kb.mjs` が 30 件で緑。`verify` が古さを検出する。`.repo-kb` のない空でないディレクトリを上書きしない。

**Phase 2**

- [ ] 作業ツリーの全チケットで `brief` が `--max-chars 24000` と `16000` に入り、必須の区切り行がこの順で並ぶ。`--phase 2` で別の Phase の手順が出る。
- [ ] `ticket`・`section`・`decision`・`files`・`diag`・`search` が「照会コマンドの出力」の形で答え、見つからなければ終了コード 1。
- [ ] `check-guide-lite` が全 9 種を検査し、意図的に壊した複製（付録 A の `Options::validate` の例など）の失敗を報告する。`PLANNED` の項目が実在すると失敗する。
- [ ] `node tests/repo_kb.mjs` が 48 件で緑。

**Phase 3**

- [ ] `repo-kb.yml` が PR で緑（Node 20・24）。`AGENTS.md`・`README.md`・`docs/architecture.md` に追記がある。
- [ ] `node tests/repo_kb.mjs` が 51 件で緑。`node tests/parallel_frontend.mjs` が従来どおり通る。
- [ ] 完了報告（GUIDE §13.2）に、`check-guide-lite` の失敗の一覧と GUIDE-LITE への修正案を書いた。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| 見出しの検出がフェンスの中の `#`・`### D-99` に反応する（言語リファレンスに 4 行、テストの文字列にもある） | `markdown_headings_ignore_fenced_lines`・`decisions_skip_fenced_headings` |
| 依存セルの注記（A18・A19・G24）の ID を強い依存にしてしまう | `deps_table_forms`・README の Mermaid 104 本との照合（付録 A のコマンド） |
| `D10`〜`D14` は決定の番号とチケット D10〜D14 が衝突する | `mentions` は決定を `D-NN`（ハイフンつき）でだけ拾う。チケットの言及は近似で、`brief` の決定の選択には使わない |
| 字下げで `#[cfg(test)]` の範囲を決める（`src/syntax_codec.rs` で 76 行ずれる） | `rust_diag_sites_skip_cfg_test_modules` |
| `String.length`・`slice` が UTF-16 とコードポイントで食い違う（絵文字） | 文字数は常に `cp`・`take`。fixture に絵文字を入れる |
| 整列に `localeCompare` や比較関数なしの `sort()` を使う | `cmp` を使う。`build_is_byte_identical_twice` |
| `brief` の CONTENTS の字数が自分自身を含む（循環） | 24 行まで・固定幅で先に確保し、本文を後から作る |

## 対象外

- LLM・要約・意味検索・全文検索。Rust 側の KB（G26〜G28）、MCP（G29）、図表（G30）。
- チケット・GUIDE・GUIDE-LITE・README の自動修正。`state_conflict` の 4 件（G13・G16・G17・G18 の状態セル）を直すこと。
- Windows の CI、npm パッケージ、`jq` への依存。診断の `kind`（G23 の後に足す）。

## 決定事項

### D1: Node の ES module（外部パッケージなし）

理由: `scripts/check-docs.mjs` と同じ流儀で、CI も Node だけで足りる。却下: Rust のサブコマンド（`kb` は G27 の名前。ビルドが要る）、Python。状態: 承認済み。

### D2: 索引は JSON Lines。本文はファイルから都度読む（決定の `text` だけ索引に持つ）

理由: 古い本文を出さない。却下: 本文の複製（古くなる）、SQLite（Node 20 に標準でない）。状態: 承認済み。

### D3: 場所（`_completed/`）を状態セルより優先し、食い違いは警告 `state_conflict`

理由: セルの更新漏れが 4 件ある。却下: 4 件のセルを直す（他チケットの書き換え）、セルを信じる（依存の状態が誤る）。状態: 承認済み。

### D4: 依存は「最初の `。`（括弧の外）までの一覧」＋「弱い依存の印」

理由: README の Mermaid（104 本・破線 10）を独立の正解として全部再現した。却下: セル全体の ID を拾う（A18・A19・G24 の注記で誤る）。状態: 承認済み。

### D5: セクションは「役割 → 名前の別名表」で最初の 1 つ

理由: `仕様` がない 37 件、`未決事項`／`決定事項` の揺れ。却下: 固定の 1 名前、ファジー一致。状態: 承認済み。

### D6: Phase は `^Phase\s*N` で始まる見出しで切り、`実装手順` だけ粘着する

理由: G24 の形に合い、`仕様` の一般的な小節を巻き込まない。却下: 全節で粘着（誤帰属）、HTML コメントの印（チケットの書き換えが要る）。状態: 承認済み。

### D7: 切り詰めは「段の表」

理由: 予測でき、テストで固定できる。却下: 比例配分（不安定）、全体の先頭を切る（後ろの受け入れ条件が消える）。状態: 承認済み。

### D8: Rust は自前の小さなトークナイザで走査する

理由: テストの mod・文字列の中の `fn`・raw 文字列を正しく除け、757 か所を再現できた。却下: grep（テストの 122 か所が混ざる）、`syn`・rustc（crate が要る）。状態: 承認済み。

### D9: ファイル一覧は `git ls-files`（未追跡も含む）、失敗したら walk

理由: `target/`・`*.o` を除ける。却下: walk だけ。状態: 承認済み。

### D10: `brief` の案内は英語、本文は原文

理由: AI 向け成果物は英語（D-56）。却下: 本文の機械翻訳（LLM が要る）。状態: 承認済み。

### D11: 鮮度は D-52 に倣う（`input_sha256`・マーカー・自動再作成・`--no-refresh`）

却下: 毎回作る、手動だけ（古い答えを返す）。状態: 承認済み。

### D12: CI は新しい軽い workflow `repo-kb.yml`（`ubuntu-24.04`・Node 20 と 24）

却下: `targets.yml` に足す（4 ホストの重い行列）、CI なし（ずれが残る）。状態: 承認済み。

### D13: GUIDE-LITE の検査は「閉じた 9 種＋`PLANNED` の棚卸し」

理由: 偽陽性を抑え、許可リストの腐敗を防ぐ。却下: 全単語の grep、LLM による判定。状態: 承認済み。

### D14: 種の一覧にない `section` コマンドを足す

理由: 切り詰めた節の取り出しを 1 コマンドにする。却下: `ticket` に節名のオプションを足す（出力の形が複雑になる）。状態: 承認済み。

### D15: fixture はファイルで `tests/fixtures/repo_kb/mini/` に置く

却下: テスト内の文字列（Rust の raw 文字列で壊れやすい）。状態: 承認済み。

### D16: 既定の `--max-chars` は 24000（下限 16000）、GUIDE-LITE は最後に切る

理由: 種の採択どおり。GUIDE-LITE（起票時 7,539 字、台帳の追記後 8,959 字）は上限の約 3〜4 割。必要なら `--no-lite` か上限を上げる。下限は、試作で作業ツリーの全チケットが 16000 に入り、12000 には入らなかったことによる。却下: 既定を 60000 にする（小さいモデルの文脈を圧迫する）。状態: 承認済み。

## 付録

詳細は [G32-appendix.md](G32-appendix.md): A 試作の結果（GUIDE-LITE の参照の分類と README の Mermaid との照合）、B テスト一覧と fixture、C CI の yml、D AGENTS.md の追記。
