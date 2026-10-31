# G33: 仕様の規則 ID と適合性スイート

| 項目 | 内容 |
| --- | --- |
| ID | G33 |
| 優先度 | P2 |
| 規模 | 全体 L（D-50 の表）。Phase 1: M（約 4 日。形式・ハーネス・`check-spec`・OWN の 30 規則）、Phase 2: M（約 4 日。NUM・EVL・CTL・TYP の 98 規則）、Phase 3: M（約 3 日。残り 10 領域の 100 規則）、Phase 4: M（約 3 日。文書のマーカー 228 か所・カバレッジ・CI。種は S だったが、マーカーが 228 か所あるので M にした）。Phase ごとに別の PR にしてよい |
| 依存 | なし（どの Phase も他チケットの実装を待たない）。弱い依存: G23（診断の `kind=` の照合。入るまで使わない）、A22（`// edition: 2027`。入るまで使わない） |
| 後続 | G35（カバレッジの表を `<!-- generated:spec-coverage -->` へ流す）。言語の挙動を変える以降のすべてのチケット（規則とテストを足す。「ドキュメント」の節） |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc`（main）で、差は `docs/benchmarks.md` だけ。確認: `git diff --stat a2313d19 HEAD`） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D14 はすべて承認済み |
| 改善する劣位 | 仕様の主張が散文のままで（`docs/language.md` 4,118 行、言語リファレンス 111 ページ）、どの文がどのテストで守られているか、文とコンパイラが食い違っていないかを機械で確かめられない（D-50 の調査結論 (4)）。規則に ID がないので、AI も人も「この規則」を指して報告・検索できない |
| 手本にする既存実装 | `scripts/check-docs.mjs` の `headings`（見出しの slug とフェンスの除き方）・`collect`・環境変数 `TSUZURI_BIN`。`tests/e2e.mjs` の `execute`（`spawnSync` の組み立て）・コンパイラを `process.argv[2]` で受ける流儀・`TSUZURI_CLANG`。`src/trap.rs` の `TrapKind::description`（トラップの種類の正本）。`src/test_runner.rs` の `tsuzuri test --json`（`type` が `test`／`summary` の行）。CI の step の形: `.github/workflows/targets.yml` の「End-to-end compiler tests」 |
| 主な影響ファイル | Phase 1: `scripts/conformance.mjs`（新規）, `scripts/conformance/directives.mjs`・`scripts/conformance/discover.mjs`・`scripts/conformance/expect.mjs`・`scripts/conformance/run.mjs`・`scripts/conformance/spec.mjs`（すべて新規）, `scripts/check-spec.mjs`（新規）, `spec/README.md`（新規）, `spec/rules/OWN.md`（新規）, `tests/conformance/OWN/**`（新規）, `tests/conformance_harness.mjs`（新規）, `tests/fixtures/conformance_mini/**`（新規）, `README.md`, `docs/architecture.md`。Phase 2: `spec/rules/NUM.md`・`spec/rules/EVL.md`・`spec/rules/CTL.md`・`spec/rules/TYP.md`・`tests/conformance/{NUM,EVL,CTL,TYP}/**`（すべて新規）。Phase 3: `spec/rules/{LEX,SYN,STR,MOD,TSK,DIA,CLI,ABI,STD,EDN}.md`・`tests/conformance/{LEX,SYN,STR,MOD,TSK,DIA,CLI,ABI,STD,EDN}/**`（すべて新規）。Phase 4: `docs/language.md`・`docs/architecture.md`・`_tsuzuri/language-reference/**`（マーカーの行だけ）, `_tsuzuri/language-reference/compiler/conformance.md`（新規）, `_tsuzuri/language-reference/index.md`, `docs/stability.md`, `.github/workflows/targets.yml`, `.github/workflows/spec.yml`（新規）。**変更しない**: `src/`・`std/`・既存のテスト・`scripts/check-docs.mjs`・チケット・`_features/GUIDE*.md` |

## 方針の最新化（2026-10-30。実装前の再検討）

起票時（HEAD `a2313d19`）の後に入った A18・A21・A22・A23・D-59 などで、付録の 228 行のうち 24 行が現在のコンパイラと食い違った（停止条件 1・10）。人間の包括許可に従い、規則の趣旨を保ったまま次のとおり改めた（欠番は作らない）。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| マニフェストのない入力は edition 2026 | A22 で edition 2027 が最新になり、素の `..`・短い接尾辞（`l`・`f`）・旧ラムダ `x ->` は `E0004` | 構文が主題でない規則のテストは、両 edition が受理する正準形に書き換えた（`..=`、`start .. step ..= finish`、`i64`・`f32` の接尾辞、`\x ->`）。短い接尾辞そのものが主題の NUM-030 は `// edition: 2026` を付け、規則の文を `In edition 2026, ` で始めた。CTL-004・CTL-005 の文は `..=` の形にした |
| 無印の引数は値渡し | A18 で 2027 の無印の引数は読み取り（借用） | 値渡し（move）が主題の OWN-003・008・015・022・024・029・ABI-005・TYP-007・CTL-026 は `own` を書いた（両 edition で同じ意味） |
| `Array.get` は `Maybe` を返す、`Vec.push` は消費する | D-59 の後、`Array.get` はトラップし `Maybe` は `Array.try_item`。2027 の `Vec.push` は `Vec.push_mut` | STD-003 の文とテストを `Array.try_item`、STD-010 を `Vec.push_owned` にした（テストの名前も） |
| 診断の JSON は 5 キー、対応 edition は 2026、`tsuzuri new` は 2026 | G23 の `kind` が `span` の後に続く。edition は `2026, 2027`。`new` は `edition = "2027"` | DIA-001 は 5 キーが「先頭に」この順で並ぶことを確かめる。EDN-003・EDN-005 の期待を現行にした（EDN-005 の文は「最新の edition」） |
| `TrapKind` は 16 | A21 で `ContractRequires`・`ContractEnsures` が増えて 18 | `TRAP_KINDS` に `contract_requires`・`contract_ensures` を足した（No.16 は `src/trap.rs` の数と一致を検査） |
| `tests-pass` は `test --json` | A23 で `tsuzuri test` は折り返した算術のテストを既定で失敗にする（NUM の折り返しの規則が落ちる） | ハーネスは `tests-pass` を `--allow-overflow` で実行する（値の規則を確かめるため。報告は警告にとどまる） |
| マーカーは規則を述べる文の直前（リストの項目の先頭を含む） | CommonMark では項目の先頭の `<!--` が HTML ブロックになり、同じ行の本文のインラインの書式が表示されなくなる | マーカーは Doc の節の中で規則のコード（`E1012` など）とコード片を最も多く含むブロックの直前の独立した行、表なら行の先頭のセルに置く（リストは項目ではなくリスト全体の前）。インラインのコードの中の `<!-- rule: … -->` はマーカーとして数えない |

## 実装者向け早見

1. 最初に読む: 「仕様」の §1〜§6（ID・規則表・テストの置き場と**行からファイルを作る手順**・ディレクティブ・期待・実行）と、付録 A.2 の例。規則の一覧は付録 B（OWN・NUM・EVL・CTL・TYP の 128 件）と付録 C（残り 10 領域の 100 件）。
2. 最初に書くテスト: `tests/conformance_harness.mjs` の `footer_splits_directives_from_body`（手順 2）。骨格は「設計」の擬似コード。
3. 作るのは `scripts/conformance.mjs`・`scripts/conformance/*.mjs`・`scripts/check-spec.mjs`・`spec/**`・`tests/conformance/**`。**変えない**: `src/`・`std/`・既存のテスト・`scripts/check-docs.mjs`・チケット・`_features/GUIDE*.md`。
4. 規則は付録の 228 行をそのまま使う。1 行 = 1 規則 = 1 テスト。付録の行を勝手に直さない。食い違ったら止めて報告する（停止条件 1・2）。
5. 規則の文（英語）と実機の挙動が食い違うなら、その規則は**足さない**。テストを実機に合わせて書き換えない。
6. ディレクティブは `.tz` の**末尾**に書く（先頭ではない）。本体のバイト位置・行番号が、コンパイラの出力と付録の値のままになる。
7. `line=N` は**ファイルの行番号**。既定のファイルは `Main.tz`。別のファイルなら `file=Lib.tz` を足す。
8. コマンド（コンパイラは `target/release/tsuzuri`）:
   `node scripts/conformance.mjs target/release/tsuzuri [--area OWN] [--rule OWN-001] [--filter 語] [--jobs 4] [--targets all|native]`
   `node scripts/check-spec.mjs [--write] [--json] [--require-markers]`
   `node tests/conformance_harness.mjs target/release/tsuzuri [テスト名の一部]`
9. 完了の見分け方: Phase 1 = `--area OWN` が 30 件 ok、`check-spec` が終了コード 0、自己テスト緑。Phase 2 = 128 件。Phase 3 = 228 件。Phase 4 = `check-spec --require-markers` が 0、CI の 2 つの job が緑。
10. 規則を 1 件足す手順（Phase 4 の後も同じ）は「仕様」§8 の P1〜P6。

## 目的

- **問題**: 言語の仕様は散文（`docs/language.md` 4,118 行・350,810 字、`_tsuzuri/language-reference/` 111 ページ）で、規則に ID がない。ある文を守るテストがあるか、文と実機が一致しているかを機械で確かめられない。小さいモデルが規則を実装・文書化するときも、「この規則の期待」を探す先がない。
- **価値**: 規則に ID（D-58）を付け、(1) 規則 → 文書の節 → テストを 1 つの表で辿れる、(2) 「テストのない規則」「規則のないテスト」「存在しない規則を指すマーカー」を `check-spec` が落とす、(3) 規則・領域の単位で `conformance.mjs --rule OWN-012` と絞って実行できる、(4) edition ごとの差を規則の増減として表せる。
- **HEAD で測った数字**（付録 A の方法で実際に実行した）: 規則 228 件（OWN 30・NUM 30・EVL 20・CTL 26・TYP 22・LEX/SYN/STR/MOD/TSK/DIA/CLI/ABI/STD/EDN 各 10）の全部が `target/release/tsuzuri` で通った。期待の内訳は check-error 67・stdout 97・tests-pass 21・trap 10（標準出力つき 1 件を含む）・check-warning 2・check-ok 2、cli モード 29（`deterministic` の 1 件を含む）。出典は 43 ファイル（言語リファレンス 40 ページ 163 件、`docs/language.md` 63 件、`docs/stability.md` 1 件、`docs/architecture.md` 1 件）。試作のハーネスで 228 件を `--jobs 4` で実行して 3 回とも 3 分 42 秒〜7 分 6 秒（他の作業と同時だと遅い）。
- **非目標**: (1) 規則の網羅（全文の規則化）。Phase 3 の後も棚卸しは続く活動で、終わりの条件を置かない。(2) コンパイラ・std・診断の変更。(3) WASM での `stdout`・`trap` の照合（WASM を通るのは `tests-pass` の 21 件だけ。「仕様」§6）。(4) 他の実装への配布物。(5) カバレッジ率や実行時間の合否閾値（共有 CI に置かない）。

## 着手条件と停止条件

### 着手条件

- 依存なし。**ベースライン**: `target/release/tsuzuri` がある（`cargo build --release --locked`）。`node scripts/check-docs.mjs` が緑。Node 20 以上（`node --version`）。
- Phase 2 の手順 8（NUM の `tests-pass` を WASM の 4 組で流す）と CI には、`clang`（`TSUZURI_CLANG`）と `wasm-ld`（`TSUZURI_WASM_LD`）が要る（`tests/e2e.mjs` と同じ前提）。手元に無ければ `--targets native` で native だけ実行し、WASM は CI で確かめる。
- 読むもの: `_features/GUIDE.md` の D-58（規則 ID の形式）・D-55（名前の割り当て）・§7（テスト）・§8（文書の更新）・§13（停止条件と完了報告）、`scripts/check-docs.mjs` の `headings`、`tests/e2e.mjs` の `execute`、`src/trap.rs`、付録 A。

### 停止条件

1. 付録の行をファイルにして実行しても期待が合わない（付録を作った時点と、コンパイラの挙動が違う）。選択肢: A（その規則だけ外して報告。ID は欠番にする）、B（コンパイラの変更が原因なら、その変更のチケットへ報告）。
2. 規則の文と実機の挙動が食い違うことが分かった。選択肢: A（規則を足さず、食い違いを報告。文書の修正は別の PR）、B（コンパイラのバグとして報告）。テストを実機に合わせて書き換えない。
3. 同じテストの結果が `-O0` と `-O3`、または native と WASM で違う。コンパイラのバグとして報告し、テストを弱めない。
4. 228 件の実行が `--jobs 4` で 10 分を超える（試作は 3 分 42 秒〜7 分）。選択肢: A（`tests-pass` の `targets:` を絞り、理由を `// note:` に書く）、B（CI の `--jobs` を上げる）。
5. CI に `wasm-ld` や `node` が無く、`--targets all` が動かない。`--targets native` で CI を通さない。選択肢: A（CI の toolchain を直す）、B（報告）。
6. 新しい診断コード・CLI フラグ・環境変数・文法・std の追加が要ると思った（例: WASM の標準出力を読むホストを作りたい）。対象外。止めて報告する。
7. マーカー（HTML コメント）を足すと `node scripts/check-docs.mjs <ページ>` が失敗する、または表・リスト・コードブロックの中に置くことになる。選択肢: A（その文の直前の空行に置く）、B（その規則の Doc を別の節に付け替える）。
8. `check-spec --write` が Tests の列以外のセルや、表の外の文を書き換える。止める（実装のバグ）。
9. 既存の ID の番号を詰めたい・再利用したいと思った。禁止。`retired` にして新しい番号を使う。
10. A22 か G23 が先に入っていて、既存のテストが落ちる。選択肢: A（A22: 2026 の挙動を見るテストに `// edition: 2026` を足す）、B（G23: `kind=` は足さず、そのままにする）。

## 現状（HEAD `a2313d19` で確認。件数は作業ツリー `9dd0fbbc` のコマンドで数えた）

### 規則 ID も適合性テストもない

```sh
ls spec tests/conformance                                   # どちらも No such file or directory
grep -rn "<!-- rule:" _tsuzuri docs README.md | wc -l       # 0
grep -rl "<!--" _tsuzuri docs README.md                     # docs/benchmarks.md だけ（HTML コメントを使う文書は 1 つ。4 か所）
wc -l -m docs/language.md                                   # 4118 行 / 350810 字（## の節は 15）
git ls-files '_tsuzuri/language-reference/*.md' '_tsuzuri/language-reference/**/*.md' | wc -l   # 111
```

### 既存のテスト基盤（再利用できるものとできないもの）

```sh
git ls-files 'tests/*.mjs' | wc -l                                              # 80
git ls-files 'tests/*.mjs' | xargs grep -l 'process.argv\[2\]' | wc -l           # 64（コンパイラのパスを argv[2] で受ける）
git ls-files 'tests/*.rs' | wc -l                                               # 101
grep -c '^export ' tests/features.mjs                                           # 0（トップレベルのスクリプトで、import できない）
grep -n 'const compiler\|TSUZURI_CLANG' tests/e2e.mjs | head -3                 # 流儀: 引数 2 つ目（既定 target/debug/tsuzuri）と環境変数
```

`tests/features.mjs` は 58 の suite を持つが、規則の ID・期待の宣言を持たず、C のホストと確保追跡に結びついているので**再利用しない**（ハーネスは新規）。`scripts/check-docs.mjs` は言語リファレンスの例だけを検査し、`docs/language.md` の文は検査しない。

### コンパイラの実出力（一時ディレクトリで実行。パスは引数に与えたまま出る）

```sh
$ printf 'let a = "x"\nlet b = a\na\n' > a/Main.tz
$ target/release/tsuzuri check a --json        # 診断は stderr に 1 行 1 件の JSON。終了コード 1
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'a'","path":"a/Main.tz","span":{"start":22,"end":23,"line":3,"column":1,"end_line":3,"end_column":2}}
$ target/release/tsuzuri check a               # 人間向け（成功時は何も出さず終了コード 0）
a/Main.tz:3:1: error[E1012]: use of moved or partially moved value 'a'
  3 | a
    | ^
$ target/release/tsuzuri run b -O3             # b/Main.tz は `let xs = [1, 2, 3]` `let i = 5` `xs[i]`。終了コード 1
trap: index out of bounds at b/Main.tz:3:1
b/Main.tz:3:1: error[E2005]: program terminated with signal: 5 (SIGTRAP); integer division, indexing, assert, or allocation may have trapped
  3 | xs[i]
    | ^
$ target/release/tsuzuri test c --json         # c/Main.tz は `test "…" = Test.equal (ref a) (ref b)` が 2 つ。stdout に JSON Lines
{"type":"test","index":0,"module":"Main","name":"i8 max + 1","status":"passed","duration_ms":225}
{"type":"test","index":1,"module":"Main","name":"adds","status":"passed","duration_ms":228}
{"type":"summary","passed":2,"failed":0,"ignored":0,"duration_ms":327}
$ target/release/tsuzuri run c --target wasm32  # run は native だけ。終了コード 2
<command line>:1:1: error[E2000]: run supports only a native target; build WebAssembly with 'tsuzuri build --target wasm32' and load the module in a host
```

分かること: (1) `trap:` の行は必ず stderr にあり、終了コードは 1、シグナルは環境で違う（`SIGTRAP` は macOS）ので**判定に使わない**。(2) `--json` の診断の `path` は与えたディレクトリのまま（絶対パスを渡せば絶対）で、`span.line` は 1 始まり。(3) `test --json` の行には `duration_ms` があるので、**行そのものは比べず**、`type`・`status`・`passed`・`failed` だけを読む。(4) `// rule: …` のようなコメントは字句解析で無視される（先頭・末尾のどちらでも通ることを確かめた）。(5) WASM で標準出力を読む手段は `run` にない。`tsuzuri test --target wasm32` は `node` と `wasm-ld` を使い、`test` 宣言の合否だけが分かる。

### 検証済みの 228 規則（付録 A の方法）

`docs/language.md`（63 件の出典）・言語リファレンス（40 ページ）を読み、規則の候補 273 件ごとに最小のプログラムを書いて `target/release/tsuzuri` で実行した。**実機で確かめられた 228 件だけ**を付録 B・C に載せた（残りは、重複・文書に根拠が薄いもの・確かめきれなかったもの）。試作のハーネス（本チケットの設計と同じ文法で、ディレクティブをファイル末尾から読んで実行する）で、228 件のすべてが通る（`tests: 228, failed: 0`）。

## 変えてはいけない意味

- コンパイラ・std・IR・ABI・診断のコードと文言・既存のテストは**一切変えない**。成果物はスクリプト・テスト・仕様の表・文書のマーカーだけ。
- 文書のマーカー（`<!-- rule: ID -->`）は表示を変えない。Phase 4 の前後で `node scripts/check-docs.mjs` の出力（ページ・リンク・例・実行の件数）が 1 件も変わらない。
- ID は再利用しない。廃止は `retired`（行は消さない）。規則の意味が変わるときは、古い行を `retired` にして新しい番号を使う。
- 既存の `node tests/*.mjs` の引数・出力・fixture を変えない。`tests/fixtures/` に足すのは `conformance_mini/` だけで、そこに `Main.tz` を置かない（`tests/parallel_frontend.mjs` が `tests/fixtures/` の全ディレクトリに `check` を回し、スレッド数による差だけを見る。`Main.tz` がなければ `E2001` で毎回同じ）。
- 共有 CI に実行時間の合否閾値を置かない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 形式（`spec/`・ディレクティブ）・ハーネス `scripts/conformance.mjs`・`scripts/check-spec.mjs`・自己テスト・OWN の 30 規則 | M | なし | スクリプト・表・テストの追加だけで、既存の挙動と文書を変えない。OWN だけで検査が閉じる |
| 2 | NUM・EVL・CTL・TYP の 98 規則（付録 B） | M | Phase 1 | 規則とテストの追加だけ。領域ごとに独立して足せる |
| 3 | 残り 10 領域の 100 規則（付録 C）と、各領域の棚卸しの手順 | M | Phase 1 | 同上 |
| 4 | 文書のマーカー（228 か所）・`--require-markers`・カバレッジの表と文書ページ・CI（`targets.yml` の step と軽い `spec.yml`） | M | Phase 1〜3（マーカーを付ける規則が要る） | マーカーは表示されない。CI の step は足すだけ |

Phase 1〜3 の間、文書は変更しない（README と `docs/architecture.md` の検証コマンドの行を除く）。

## 仕様

### §1 規則の ID と領域（D-58）

- 形式は `^(LEX|SYN|TYP|OWN|EVL|NUM|STR|CTL|MOD|TSK|DIA|CLI|ABI|STD|EDN)-[0-9]{3}$`。番号は 001〜999（`000` は使わない）。
- 番号は領域ごとに 001 から付け、**再利用しない**。新しい規則の番号 = その領域の表の全行（`retired` を含む）の最大番号 + 1。欠番は許す。
- 規則の意味が変わるときは、古い行を `retired` にして新しい番号を使う。規則の文は、edition を限る場合は `In edition 2027, ` で始める（限らない文はすべての対応 edition の規則）。

| AREA | 名前 | 範囲 |
| --- | --- | --- |
| LEX | Lexical | 字句・リテラルの形・エスケープ・コメント（`E0001`） |
| SYN | Syntax | 宣言・式の構文・`main`（`E0002`・`E2004`） |
| TYP | Types | 型検査・推論・型クラス・別名（`E1003`〜`E1005`・`E1015`・`E1016` ほか） |
| OWN | Ownership | ムーブ・借用・`Drop`（`E1012`〜`E1014`） |
| EVL | Evaluation order | 評価の順序・短絡・トラップの時点 |
| NUM | Numerics | 折り返し・除算・シフト・浮動小数点・キャスト・リテラルの範囲 |
| STR | Strings | `string`・`utf8string`・補間・連結 |
| CTL | Control | `if`・ループ・`match`・`try`・`let!` |
| MOD | Modules | モジュール・名前空間・`private`・`using` |
| TSK | Tasks | `Task`・捕捉・送信 |
| DIA | Diagnostics | 診断の形式・順序・重大度・出力 |
| CLI | CLI | サブコマンド・終了コード |
| ABI | ABI | `export def`・C ヘッダー |
| STD | Standard library | std の公開関数の意味 |
| EDN | Editions | マニフェスト・edition |

### §2 規則表 `spec/rules/<AREA>.md`

ファイルは `# <AREA>: <名前>`（名前は §1 の表）で始め、説明の段落（任意）の後に、見出し行が `| ID | Rule | Doc | Tests | Status |` ちょうどの表を 1 つ置く。表の外の文は読まない。行は `|` で始まる連続した行。セルは、直前が `\` でない `|` で分け、前後の空白を除き、`\|` を `|` に戻す。5 セルでなければ `rule_row`。

| 列 | 規則 |
| --- | --- |
| ID | §1 の形式。ファイル名の AREA と同じ。表の中で昇順（`rule_order`）。全ファイルで一意（`rule_duplicate`） |
| Rule | 英語の 1 文。末尾は `.`。`MUST`・`MUST NOT`・`MAY` のどれかを含み、`SHOULD` を含まない。240 字（コードポイント）以内、改行なし。文の中の `|` は `\|` と書く。診断コードは `(E1012)` のように文の中に書いてよい |
| Doc | 状態が `active` なら `[表示](相対パス#slug)` ちょうど 1 つ。相対パスは `spec/rules/` から（`../../docs/language.md`）。許可するのは `docs/*.md`・`_tsuzuri/language-reference/**/*.md`・`README.md` で、ファイルが実在すること。`#slug` は省略できる。あるなら、そのファイルの見出しの slug（`scripts/check-docs.mjs` の `headings` と同じ規則）に存在すること。表示の形は `<ファイル名> § <見出し>`（見出しなしは `<ファイル名>`）で、`check-spec --link`（§7）が作る。`retired` は `-` でもよい |
| Tests | テストの stem を昇順に `, ` でつなぐ。なければ `-`。**`check-spec --write` が書く**（手で書かない） |
| Status | `active` か `retired` |

```
| OWN-001 | A `let` binding from a non-Copy value (such as a `string`) MUST move the value, and a later read of the source binding MUST be rejected with E1012. | [ownership.md § ムーブ後の使用エラー（E1012）](../../_tsuzuri/language-reference/ownership-and-memory/ownership.md#ムーブ後の使用エラーe1012) | OWN-001-let-binding-from-non | active |
```

### §3 適合性テストの置き場と名前

| 形 | パス | 条件 |
| --- | --- | --- |
| 1 ファイル | `tests/conformance/<AREA>/<stem>.tz` | 単一ファイルのプログラム。コンパイラへは `Main.tz` として渡す |
| ディレクトリ | `tests/conformance/<AREA>/<stem>/` | `Main.tz` が必須（ディレクティブを持つ）。ほかの `.tz`・`.tt`・`.tc`・`Tsuzuri.toml`・補助ファイルを置ける。ディレクトリ全体を複製して実行する |
| 無視 | 名前が `.` で始まるもの（例: `.DS_Store`） | — |
| 禁止 | 上のどれでもないもの。`<AREA>` が §1 の 15 個以外のディレクトリ | `unexpected_entry` |

stem は `^[A-Z]{3}-[0-9]{3}-[a-z0-9]+(-[a-z0-9]+)*$` で 80 字以内、全体で一意（`test_name`）。先頭の ID の AREA がディレクトリ名と同じ。最初の `// rule:` の ID が stem の ID と同じ（`test_rule`）。**付録の行からファイルを作る手順**:

1. stem = `<ID>-<slug>`。slug = 規則の文から英数字の語（`[A-Za-z0-9]+`）を取り、冠詞 `a`・`an`・`the` を除いた先頭 4 語を小文字にして `-` でつなぐ（例: `OWN-001-let-binding-from-non`）。
2. 行の `` `src` `` が `(empty)` なら本体は空。`[名前]⏎`（名前は `[A-Za-z0-9_./-]+` で拡張子が `.tz`・`.tt`・`.tc`・`.toml`）で始まるなら複数ファイル（各 `[名前]⏎` から次の `⏎[名前]⏎` の前までが 1 ファイルの内容）でディレクトリ形。そうでなければ 1 ファイル形。
3. `⏎` を改行にする。各ファイルの終わりに改行 1 つ。
4. 1 ファイル形の本体、またはディレクトリ形の `Main.tz`（`src` に無ければ空の `Main.tz` を作る）の末尾に、空行 1 つ（本体が空なら不要）、続けて `// rule: <ID>` の行、続けて行の最後のセル（`→` の後ろの `` `…` ``）を ` ⏎ ` で分けた項目を 1 つ 1 行にし、各行の先頭に `// ` を付けて置く。

### §4 ディレクティブ

**位置。** ディレクティブはファイルの**最後の行の並び**。`splitFooter(text)`:

```
lines = text.split("\n"); 最後が "" なら除く
start = lines.length
while start > 0 and (lines[start-1] が空白だけ、または /^\/\/ [a-z][a-z-]*: ?/ に一致): start -= 1
directives = lines[start..] のうち /^\/\/ ([a-z][a-z-]*): ?(.*)$/ に一致する行 → { key, value, line: 添字 + 1 }
bodyLines = start
```

最後の行が通常のコメント（例: `// done`）なら、その上のディレクティブは読まれず `missing_rule` になる。本体の途中の `// foo: bar` は読まない。

| キー | 値 | 回数 | 意味 |
| --- | --- | --- | --- |
| `rule` | 規則の ID | 1 以上 | このテストが守る規則。最初が stem の ID |
| `expect` | §5 の期待 | 1 以上 | すべて満たすこと |
| `edition` | `20NN` | 0〜1 | 1 ファイル形では `Tsuzuri.toml`（`[package]`・`name = "conformance"`・`version = "0.1.0"`・`edition = "20NN"`）を足す。ディレクトリ形で `Tsuzuri.toml` があれば `edition_conflict` |
| `targets` | `native-O0 native-O3 wasm-O0 wasm-O3` の部分集合（重複なし） | 0〜1 | `stdout`・`trap`・`tests-pass` を実行する組。既定は `stdout`・`trap` が `native-O0 native-O3`、`tests-pass` が 4 つ。`stdout`・`trap` に `wasm-*` は不可。cli モードでは不可（`targets_not_allowed`） |
| `cli` | コンパイラの引数（空白で区切る。引用符は不可） | 0〜1 | あれば cli モード。`$DIR` は作業ディレクトリ（絶対パス）、`$OUT` は `deterministic` の出力先 |
| `note` | 自由な 1 行 | 任意 | 読まない |

未知のキーは `unknown_directive`。値は**語**（空白で区切る）と **JSON 文字列**（`"` で始まり、`\` のエスケープを `JSON.parse` で読む。直後は空白か行末）の並び。検証エラーの名前と条件は付録 D.2。

### §5 期待

**通常モード**（`// cli:` がない）:

| kind | 構文 | 合格の条件 |
| --- | --- | --- |
| `check-ok` | `check-ok` | `check DIR --json` の終了コードが 0 で、重大度 `error` の診断がない |
| `check-error` | `check-error E1012 [line=N] [file=NAME] [kind=K]`（コードは `E` と 4 桁） | 終了コードが 1 で、重大度 `error` かつコードが一致する診断のうち 1 件が、指定の `line`・`file`・`kind` をすべて満たす |
| `check-warning` | `check-warning W1001 [line=N] [file=NAME] [kind=K]`（コードは `W` と 4 桁） | 終了コードが 0 で、重大度 `warning` の診断が同様に 1 件ある |
| `stdout` | `stdout "<JSON 文字列>"` | `run DIR -O0` と `-O3`（`targets:` の native の組）の各回で、終了コード 0・標準出力が文字列と**完全に**等しい |
| `trap` | `trap kind=K [line=N] [file=NAME] [stdout "<JSON 文字列>"]` | 各回で、終了コード 1、stderr に `trap: <K の説明>` で始まり ` at <path>:<line>:<column>` で終わる行がある（K の説明は付録 D.1）。`stdout` があれば標準出力がそれと等しい |
| `tests-pass` | `tests-pass N` | `targets:` の各組で `test DIR --json --target native\|wasm32 -O0\|-O3` が終了コード 0・`summary` の `failed` が 0・`passed` が N |

`line` は診断の `span.line`（ファイルの 1 始まりの行。ディレクティブの行も数える）。`line=` があって `file=` がなければ `Main.tz`。`file` は `\` を `/` にした `path` が `file` に等しいか `/file` で終わること。`kind=K` は診断の JSON の `kind` キー（G23 の後。キーがなければ不一致）。`stdout` と `trap` は同時に書けない（`trap` の `stdout` を使う）。

**cli モード**（`// cli:` がある。`expect` に使えるのは次だけ）:

| kind | 構文 | 合格の条件 |
| --- | --- | --- |
| `exit` | `exit N` | 終了コードが N |
| `stdout`・`stdout-has`・`stderr-has`・`stderr-lacks` | `stdout "…"`（ほかも同じ形） | stdout が完全に等しい／stdout に含む／stderr に含む／stderr に含まない |
| `stdout-empty`・`stderr-empty` | 引数なし | 空 |
| `stderr-lines` | `stderr-lines N` | stderr の空でない行が N |
| `file-has`・`file-lacks` | `file-has PATH "…"` | `DIR/PATH` が存在して含む／存在しないか含まない（PATH は相対） |
| `deterministic` | 引数なし | `$OUT` を `DIR/a.out`・`DIR/b.out` にして 2 回実行し、どちらも終了コード 0 で 2 ファイルが同じバイト列。`cli` に `$OUT` が必要 |

cli は作業ディレクトリ `DIR` で 1 回実行する（`deterministic` は追加で 2 回）。

### §6 実行（`scripts/conformance.mjs`）

- **作業ディレクトリ**: `os.tmpdir()/tsuzuri-conformance-XXXXXX/<stem>/` にテストを複製する（1 ファイル形は `Main.tz`）。終了時に消す（`--keep` で残して場所を出す）。
- **コンパイラ**: 位置引数 → 環境変数 `TSUZURI_BIN` → `target/release/tsuzuri`（ルートから）の順。なければ終了コード 2。`--root DIR` は `tests/conformance/` を持つ木の場所（既定はリポジトリのルート。自己テストが fixture の木を指すのに使う）。
- **選択**: `--area A[,B]`・`--rule ID[,ID]`（どの `// rule:` でも）・`--filter 語`（stem の部分一致）。全部 AND。0 件なら終了コード 2。`--list` は選択だけを `stem<TAB>rules<TAB>kinds`（`rules` は `// rule:` を `,` でつなぎ、`kinds` は `expect` の kind を書かれた順に `,` でつなぐ）で出して終わる。
- **targets**: `--targets all`（既定）か `native`。`all` のとき、WASM の組を使うテストが 1 つでもあれば、最初に小さい `test --target wasm32 -O0` を 1 回流す（`probeWasm`）。失敗したら終了コード 2 で `wasm32 tests are unavailable (<stderr の先頭行>); install wasm-ld (TSUZURI_WASM_LD) or pass --targets native`。`native` は `wasm-*` の組を除き、stderr に `note: wasm targets skipped` を 1 行出す。除いた結果、実行する組が 0 になるテストは選択から外す（件数を同じ行に書く）。
- **並列**: `--jobs N`（既定 `min(4, 論理 CPU 数)`、1〜64）。1 テストの中は逐次。1 回のコマンドは `--timeout SEC`（小数可。既定 120）で打ち切り、出力は 4 MiB まで（超えたら不合格）。
- **終了コード**: 0 = 選択したテストがすべて ok。1 = 1 件以上が不合格。2 = 使い方・ディレクティブの検証エラー（実行前に全部を出す）・コンパイラなし・`probeWasm` の失敗・選択 0 件。
- **出力**: 1 テスト 1 行 `ok <stem> (<ID,…>)` か `FAIL <stem> (<ID,…>)`（続けて 2 空白で字下げした理由の行）を、stem の昇順で、全テストの後に出す。最後に `conformance: N tests, P ok, F failed`。メッセージの作業ディレクトリは `$DIR` に置き換える。`--json` は `{"type":"test","stem","rules","status":"ok|fail","details":[…]}` の行と `{"type":"summary","tests","ok","failed"}`。時間は `--timing` のときだけ付く（既定の出力は決定的）。
- **対象 OS**: macOS と Linux。Windows は未検証（診断の `path` は `\` を `/` にして比べるが、標準出力の改行や終了コードは確かめていない）。

### §7 検査器 `scripts/check-spec.mjs`

`node scripts/check-spec.mjs [--root DIR] [--write] [--json] [--coverage | --coverage-markdown] [--require-markers] [--link PAGE [HEADING]] [--strip-markers PATH]`。`--root` の既定はリポジトリのルート。入力は `spec/rules/*.md`・`tests/conformance/**`（§3 の形）・マーカーを探す文書（`docs/*.md`・`README.md`・`_tsuzuri/**/*.md`）。コンパイラは使わない。

| 名前 | エラーにする条件 |
| --- | --- |
| `rule_file` | `spec/rules/` に `<AREA>.md` 以外のファイルがある、または表（§2 の見出し行）がない |
| `rule_row` | 行のセルが 5 つでない |
| `rule_id` | ID が §1 の形式でない、AREA がファイル名と違う |
| `rule_duplicate` | 同じ ID が全ファイルで 2 回以上 |
| `rule_order` | 表の中で、ID が前の行の ID より小さい |
| `rule_status` | Status が `active`・`retired` のどちらでもない |
| `rule_sentence` | `active` の Rule が §2 の規則（`.` で終わる・`MUST`／`MAY` を含む・`SHOULD` を含まない・240 字以内・改行なし）に合わない |
| `doc_link` | `active` の Doc が §2 の規則に合わない（リンクが 1 つでない・許可外のパス・ファイルがない・`#slug` が見出しにない） |
| `tests_cell` | Tests のセルが、計算した値（その規則を `// rule:` で指すテストの stem の昇順、なければ `-`）と違う。`--write` が直す |
| `rule_untested` | `active` の規則を指すテストが 1 つもない |
| `rule_retired_tested` | `retired` の規則を指すテストがある |
| `test_name` | テストの置き場・名前が §3 に合わない、または stem が重複 |
| `test_directive` | `parseTest` のエラー（名前は付録 D.2）。メッセージの先頭に D.2 の名前を付ける |
| `test_rule` | `// rule:` の ID が表にない、`retired`、または最初の ID が stem の ID と違う |
| `marker_rule` | マーカーの ID が表にない、または `retired` |
| `marker_missing` | `--require-markers` のとき、`active` の規則の Doc のファイルに、その ID のマーカーがない |
| `coverage_block` | `_tsuzuri/language-reference/compiler/conformance.md` があり、`<!-- generated:spec-coverage -->` と `<!-- /generated:spec-coverage -->` の間が `--coverage-markdown` の出力と違う（ページがなければ検査しない） |

- **出力**: エラーを `<パス>:<行>: <名前>: <メッセージ>` で stderr に（パス・行・名前の昇順）。最後に stdout へ `check-spec: ok (A active, R retired, T tests)` か `check-spec: E errors`。終了コード 0 = エラーなし、1 = あり、2 = 使い方・読めない入力。`--json` はエラーを `{"path","line","name","message"}` の行で stdout に出す。
- **入力がない木**: `spec/rules/` がなければ stderr に `check-spec: spec/rules/ not found` を出して終了コード 2。`tests/conformance/` がなければテスト 0 件として読む（`rule_untested` が規則の数だけ出る）。
- **`--write`**: Tests の列（4 番目のセル）だけを計算した値に書き換え、`coverage_block` があればそのブロックも書き換える。ほかの文字は 1 byte も変えない（セルの区切り位置を保ったまま 4 番目だけ置換する）。書いた後に検査をやり直す。
- **カバレッジ**（`--coverage` は検査の後に表を stdout へ、`--coverage-markdown` は表だけ）: 領域を D-58 の順（`LEX SYN TYP OWN EVL NUM STR CTL MOD TSK DIA CLI ABI STD EDN`）に並べ、`Active`（`active` の件数）・`Retired`・`Tested`（テストを 1 つ以上持つ `active` の件数）・`Tests`（先頭の `// rule:` がその領域のテストの数。合計はテスト総数）・`Marked`（Doc のファイルにマーカーがある `active` の件数）と、最後の `Total` 行。Markdown は `| Area | Active | Retired | Tested | Tests | Marked |` と `| --- | ---: | ---: | ---: | ---: | ---: |`。`--json` と併用すると `{"schema":"tz-spec-coverage/1","areas":[{"area","active","retired","tested","tests","marked"}],"total":{"active","retired","tested","tests","marked"}}`（キーの順序はこのとおり、値は整数）。text の表は、`Area` の列を左そろえ、数の列を右そろえにし、幅を見出しと値の最大の長さにして、列の間を空白 2 つにする（付録 D.6）。
- **`--link PAGE [HEADING]`**: Doc のセルを標準出力に 1 行出す（`PAGE` はルートからの相対パス）。例: `--link _tsuzuri/language-reference/ownership-and-memory/ownership.md "ムーブ後の使用エラー（E1012）"` → `[ownership.md § ムーブ後の使用エラー（E1012）](../../_tsuzuri/language-reference/ownership-and-memory/ownership.md#ムーブ後の使用エラーe1012)`。見出しは**完全一致で 1 つだけ**あること。なければ終了コード 1。
- **`--strip-markers PATH`**: そのファイルの本文からマーカーをすべて取り除いて標準出力へ出す（マーカーだけの行は行ごと消す。行の途中のマーカーはそれだけ消す）。マーカーを足した文書が元と同じことの確認に使う: `node scripts/check-spec.mjs --strip-markers docs/language.md | cmp - <(git show HEAD:docs/language.md)`。

### §8 規則を 1 件足す手順（P1〜P6。Phase 2・3 の各領域と、Phase 4 以降の追加で共通）

- **P1 確かめる**: 文書の 1 文を選び、最小のプログラムを書いて `target/release/tsuzuri` で実行する（`check --json`・`run -O0`・`run -O3`、数値なら `test --target wasm32` も）。文の主張と一致しなければ**足さない**（停止条件 2）。
- **P2 表に行を足す**: `spec/rules/<AREA>.md` の末尾に、番号 = 最大 + 1 の行を足す。Rule は英語 1 文、Doc は `--link` の出力、Tests は `-`、Status は `active`。
- **P3 テストを書く**: §3 の逆の形でファイルを作る（本体 + 空行 + `// rule:` + `// expect:`）。1 規則に 1 つ以上。
- **P4 実行する**: `node scripts/conformance.mjs target/release/tsuzuri --rule <ID>` が `ok`。
- **P5 表を整える**: `node scripts/check-spec.mjs --write` の後、`node scripts/check-spec.mjs` が終了コード 0。
- **P6 マーカーを付ける**（Phase 4 以降）: 文書の該当する文の直前に §9 の形でマーカーを足し、`node scripts/check-docs.mjs <ページ>` と `node scripts/check-spec.mjs --require-markers` が通る。

### §9 文書のマーカー

`<!-- rule: OWN-012 -->`（`rule:` の前後に空白 1 つ、1 つのコメントに ID 1 つ）。置き場所は次の 3 つだけ。

- (a) 段落・表・コードブロックの直前の独立した行。その前に空行を置く（空行、マーカーの行、文の行の順）。
- (b) 箇条書き・番号付きリストの項目の先頭（`- <!-- rule: OWN-012 -->値は…`。`-->` の直後に空白を入れない）。
- (c) 表のセルの先頭（セルの最初の文字としてコメントを書く）。

置いてはいけない場所: コードフェンスの中（`markersIn` は無視する）、リストの項目の間や表の行の間の独立した行（リストや表が分かれる）、見出しの行の途中。1 つの文が複数の規則を言うときは、コメントを並べる。マーカーは、規則の Doc が指すファイルの中の、その規則を述べる文（`Rule` の元になった文）の直前に置く。`docs/language.md` と言語リファレンスのどちらでもよく、Doc のファイルと同じであること（`marker_missing`）。

### §10 決定性・上限・互換

- **決定性**: 整列は UTF-16 コード単位の昇順（stem・ID・パス）。出力に日時・絶対パス・並列の完了順を入れない。
- **上限**: テストの 1 ファイル 64 KiB、ディレクトリ形は 64 ファイルまで、stem 80 字、Rule 240 字、1 つの `spec/rules/*.md` は 1,000 行まで。超えたら該当の検査（`test_name`・`rule_sentence`・`rule_file`）で拒否する。
- **互換**: コンパイラ・診断・JSON・edition・ABI に触れない。新しい診断コード・サブコマンド・フラグ・環境変数・std・属性・予約語はない（既存の `TSUZURI_BIN`・`TSUZURI_CLANG`・`TSUZURI_WASM_LD` だけを使う）。カバレッジ JSON の `schema` は `tz-spec-coverage/1`（キーを足すだけの変更は番号を保つ）。ID は `docs/stability.md` の「安定として保つもの」に加える（Phase 4）。

## 設計

### ファイルと公開する名前（すべて新規。ES module。外部パッケージなし）

| ファイル | 公開する名前（引数 → 結果） |
| --- | --- |
| `scripts/conformance/directives.mjs`（新規） | `AREAS`（§1 の順の配列）、`TRAP_KINDS`（付録 D.1 の表）、`TARGETS`、`splitFooter(text)` → `{ start, directives }`、`tokens(value)`、`parseTest(text)` → `{ rules, expects, cli, edition, targets, bodyLines, errors }`（`errors` は `{ line, name, message }`） |
| `scripts/conformance/discover.mjs`（新規） | `discoverTests(root)` → `{ tests, errors }`。`tests` の要素は `{ stem, area, form, path, mainPath, text, parsed }`（`path` は 1 ファイルかディレクトリ） |
| `scripts/conformance/expect.mjs`（新規） | `parseDiagnostics(stderr)`、`matchDiagnostic(d, e)`、`trapLine(stderr)` → `{ description, path, line, column }` か `null`、`summaryOf(stdout)`、`checkCli(e, r, dir)`（どれも純関数。コンパイラを呼ばない） |
| `scripts/conformance/run.mjs`（新規） | `runCompiler(compiler, args, { cwd, timeout })` → `{ status, stdout, stderr, timedOut }`、`materialize(test, dir)`、`runTest(test, ctx)` → `{ status: "ok"\|"fail", details }`、`probeWasm(ctx)` |
| `scripts/conformance/spec.mjs`（新規） | `readRules(root)` → `{ rules, errors }`、`splitRow(line)`、`headingsOf(text)`（`check-docs.mjs` の `headings` と同じ結果）、`markersIn(text)`、`docFiles(root)`、`linkFor(root, page, heading)` |
| `scripts/conformance.mjs`（新規） | `main(argv)`: 引数・選択・並列・出力・終了コード（`process.exitCode` を設定し `process.exit()` を呼ばない） |
| `scripts/check-spec.mjs`（新規） | `main(argv)`、`checkAll(root, options)` → `{ errors, coverage, rules, tests }` |

### 擬似コード

```
tokens(value):                                     # §4 の値の字句
  out = []; i = 0
  while i < len(value):
    if value[i] == " ": i += 1; continue
    if value[i] == '"':
      j = i + 1
      while j < len(value) and value[j] != '"': j += (2 if value[j] == "\\" else 1)
      j >= len(value) → エラー "unterminated string"
      text = JSON.parse(value[i..j])               # 失敗 → エラー "invalid JSON string"
      j + 1 < len(value) and value[j+1] != " " → エラー "a string must be followed by a space"
      out.push({ string: true, text }); i = j + 1
    else:
      j = i; while j < len(value) and value[j] != " ": j += 1
      out.push({ string: false, text: value[i..j-1] }); i = j

parseTest(text):                                   # エラーは集めて返す（途中で投げない）
  { start, directives } = splitFooter(text)
  各 directive を file の順に: key が KEYS にない → unknown_directive
    rule → 形式と 000 の検査（rule_format）、edition → /^20[0-9]{2}$/、targets → TARGETS の部分集合で重複なし、
    cli → 引用符なし・空でない（空白で split）、2 回目以降 → duplicate_directive、expect → 後で
  rule が 0 → missing_rule、expect が 0 → missing_expect
  mode = cli があれば "cli" でなければ "normal"。各 expect を mode の表（§5）で読む
    kind が表にない → unknown_expectation、引数の不足・過剰・形式 → bad_argument
  交差の検査: deterministic に $OUT なし → bad_argument、trap と stdout の併用 → mixed_expectations、
    targets の置き場所（cli・check-* だけ・stdout/trap に wasm）→ targets_not_allowed

runTest(test, ctx):
  dir = ctx.work/stem に複製。edition があれば Tsuzuri.toml を書く
  cli モード: argv = cli の各語の $DIR を dir に。deterministic があれば $OUT を a.out/b.out にして 2 回。
              それ以外の expect があれば argv を 1 回実行し、各 expect を §5 の表で判定
  通常モード: check の結果は最初に必要になったときに 1 回だけ流して共有する。
              expect を書かれた順に判定し、不合格の理由（1 行）を details に積む
  details が空なら ok

main(argv):                                        # scripts/conformance.mjs
  引数を読む → discoverTests → エラーがあれば全部出して終了コード 2
  選択（--area・--rule・--filter）→ 0 件なら終了コード 2 → --list なら出して終了
  targets が all で wasm の組を使うテストがあれば probeWasm（失敗は終了コード 2）
  jobs 個のワーカーが共有の添字で次のテストを取り runTest する（結果は配列の stem の位置に入れる）
  全部終わったら stem の昇順に出力。1 件でも fail なら終了コード 1
```

`checkAll(root, options)`: (1) `readRules` → `rule_file`・`rule_row`・`rule_id`・`rule_duplicate`・`rule_order`・`rule_status`・`rule_sentence`・`doc_link`。(2) `discoverTests` → `test_name`・`test_directive`。(3) `byRule`（ID → stem の配列）を作り、規則ごとに `tests_cell`・`rule_untested`・`rule_retired_tested`、テストごとに `test_rule`。(4) 文書を走査して `marker_rule`・`marker_missing`・カバレッジ・`coverage_block`。(5) エラーを（パス・行・名前）で整列。

### 自己テストの置き場（`tests/conformance_harness.mjs`・`tests/fixtures/conformance_mini/`）

`node tests/conformance_harness.mjs [コンパイラ] [テスト名の一部]`。`test(name, fn)` の補助だけを持つ素のスクリプト（`node:test` は使わない。ほかの `tests/*.mjs` と同じ）。fixture は**正しい最小の木** `tests/fixtures/conformance_mini/good/`（付録 D.3）1 つだけで、各テストが一時ディレクトリへ複製して文字列を 1 か所置換して壊す。テストの名前・入力・期待は付録 D.4。

## 前提とする他チケットのインターフェース

- **G23（診断の `kind`）**: 本チケットは `kind=K` の照合を Phase 1 で実装する（診断の JSON の `kind` キーが `K` と等しい）。G23 の前は診断に `kind` がないので、`kind=` を使うテストは作らない。G23 の後に `kind=` を足すかどうかは G23 の実装者が決める（本チケットの 228 件は使わない）。未実装時の動き: `kind=` のテストは不一致になる（機能を出さないだけで他は動く）。
- **A22（edition 2027）**: `// edition: 2027` は Tsuzuri.toml の `edition` に書くだけ。未対応の edition はコンパイラが `E0002`（`unsupported edition '…'; supported editions: 2026`）を出すので、テストは不合格になる。A22 の実装者は、2027 で挙動が変わる 2026 の規則のテストに `// edition: 2026` を足し、2027 の規則（`In edition 2027, …`）を EDN の規則として足す。EDN-003 の期待（`supported editions: 2026`）は A22 が直す。EDN-010 は `2025`（A22 の後も未対応）を使うので変わらない。
- **G35（生成物による文書）**: `node scripts/check-spec.mjs --coverage-markdown` の出力が `<!-- generated:spec-coverage -->` と `<!-- /generated:spec-coverage -->` の間の正本。**書き手は `check-spec --write` だけ**で、G35 の `scripts/gen-docs.mjs`（G35 が新規に作る）はこのブロックを `PLACEMENTS` に持たず走査しない（記法だけを G35 と揃える）。未実装時は `check-spec` が単独で検査する。
- **G32（KB）**: `repo-kb` v1 は `spec/` と `tests/conformance/` を索引しない（本チケットは G32 に依存しない）。索引に足すかは G32 の後続で決める。
- **G34（doc-test）**: 文書の例の検査。本チケットのテストと独立（`fails=E1012` のような G34 の書式を使わない）。

## 実装手順

各手順の最後で `node tests/conformance_harness.mjs target/release/tsuzuri`（その時点の全テスト）が緑。手順 6 の後は `node scripts/check-spec.mjs` も緑。テストを先に書いて失敗を見る。1 手順の変更は 300 行以内（規則の追加の手順を除く）。

### Phase 1: 形式・ハーネス・OWN

### 手順 1: ベースライン

- 変更: なし。
- 内容: `node --version`（20 以上）。`node scripts/check-docs.mjs` の出力（件数）を控える。「現状」のコマンドを実行して件数を控える。`target/release/tsuzuri` が動くこと（`target/release/tsuzuri check examples/hello`）。付録 A の方法で付録 B の OWN-001 の行を 1 つ手でファイルにし、`target/release/tsuzuri check` で E1012 が出ることを確かめる。
- 確認: すべて通る。

### 手順 2: ディレクティブの読み取り

- 変更: `scripts/conformance/directives.mjs`（新規）、`tests/conformance_harness.mjs`（新規。`test(name, fn)` の補助、引数 2 つ目のコンパイラ、3 つ目の名前の部分一致）。
- 内容: 付録 D.4 の No.1〜12 を先に書いて失敗させ、§4・§5 と「擬似コード」の `tokens`・`parseTest` を実装する。`TRAP_KINDS` は付録 D.1 の表をそのまま置く。
- 確認: `node tests/conformance_harness.mjs` が `12 tests passed`。

### 手順 3: 期待の照合（純関数）

- 変更: `scripts/conformance/expect.mjs`（新規）、`tests/conformance_harness.mjs`。
- 内容: No.13〜17 を先に書く。`trap_table_matches_src_trap_rs` は、付録 D.4 No.16 の正規表現で `src/trap.rs` から `(Variant, 説明)` を 16 組読み、`TRAP_KINDS` と一致することを確かめる（新しい `TrapKind` が増えたらこのテストが落ちる）。
- 確認: `17 tests passed`。

### 手順 4: 探索・実行・`conformance.mjs`

- 変更: `scripts/conformance/discover.mjs`・`scripts/conformance/run.mjs`・`scripts/conformance.mjs`（すべて新規）、`tests/conformance_harness.mjs`。
- 内容: No.18〜30 を先に書く（No.20〜29 は実コンパイラを使う。入力は付録 D.4 の小さい `.tz`）。§6 のとおりに実装する。`runCompiler` は `child_process.spawn`（`stdio` は `ignore`・`pipe`・`pipe`）、タイムアウトで `SIGKILL`、出力は 4 MiB で打ち切る。
- 確認: `30 tests passed`。手順 1 で作った OWN-001 のファイルを、一時ディレクトリ `T` の `T/tests/conformance/OWN/OWN-001-let-binding-from-non.tz` に置き、`node scripts/conformance.mjs target/release/tsuzuri --root T` が `conformance: 1 tests, 1 ok, 0 failed`（終了コード 0）になる。

### 手順 5: 規則表の読み取りと `check-spec`

- 変更: `scripts/conformance/spec.mjs`・`scripts/check-spec.mjs`（新規）、`tests/fixtures/conformance_mini/good/**`（新規。付録 D.3）、`tests/conformance_harness.mjs`。
- 内容: No.31〜44 を先に書く。§2・§7・§9 のとおりに実装する。`headingsOf` は `scripts/check-docs.mjs` の `headings` を**複製**する（あちらは import できないスクリプト。`slug_matches_check_docs` が実ページ 3 つで同じ集合になることを守る）。
- 確認: `44 tests passed`。

### 手順 6: `spec/` の骨格と OWN の 30 規則

- 変更: `spec/README.md`（新規。ID の形式・表の見方・規則の足し方 P1〜P6 を 40 行以内）、`spec/rules/OWN.md`（新規）、`tests/conformance/OWN/**`（新規）、`tests/conformance_harness.mjs`（No.45）。
- 内容: 付録 B.1 の 30 行について §3 の手順でテストを作り（`spec/rules/OWN.md` の行は P2 の形。Doc は `check-spec --link` の出力）、`node scripts/check-spec.mjs --write` で Tests を埋める。No.45 `real_tree_is_clean`（実リポジトリで `check-spec` が終了コード 0）を足す。
- 確認: `node scripts/conformance.mjs target/release/tsuzuri --area OWN` が `conformance: 30 tests, 30 ok, 0 failed`。`node scripts/check-spec.mjs` が `check-spec: ok (30 active, 0 retired, 30 tests)`。`node tests/conformance_harness.mjs` が 45 件。`node tests/parallel_frontend.mjs target/release/tsuzuri` が従来どおり通る（「変えてはいけない意味」の確認）。

### 手順 7: Phase 1 の文書

- 変更: `README.md`・`docs/architecture.md`（検証コマンドの sh ブロックに 3 行）。
- 内容: 「ドキュメント」の Phase 1 の行。
- 確認: `node scripts/check-docs.mjs` が従来と同じ件数で通る。

### Phase 2: NUM・EVL・CTL・TYP

### 手順 8: NUM の 30 規則

- 変更: `spec/rules/NUM.md`・`tests/conformance/NUM/**`（新規）。
- 内容: 付録 B.2 の 30 行を P2〜P5 で足す。21 件の `tests-pass` は WASM の 4 組を通る（`wasm-ld` が要る）。
- 確認: `--area NUM` が `30 tests, 30 ok`。`check-spec` が `ok (60 active, 0 retired, 60 tests)`。

### 手順 9: EVL の 20 規則

- 変更: `spec/rules/EVL.md`・`tests/conformance/EVL/**`（新規）。
- 内容: 付録 B.3 の 20 行を P2〜P5 で足す。
- 確認: `--area EVL` が `20 tests, 20 ok`。`check-spec` が `ok (80 active, …, 80 tests)`。

### 手順 10: CTL の 26 規則

- 変更: `spec/rules/CTL.md`・`tests/conformance/CTL/**`（新規）。
- 内容: 付録 B.4 の 26 行を P2〜P5 で足す。
- 確認: `--area CTL` が `26 tests, 26 ok`。`check-spec` が `ok (106 active, …, 106 tests)`。

### 手順 11: TYP の 22 規則と Phase 2 の確認

- 変更: `spec/rules/TYP.md`・`tests/conformance/TYP/**`（新規）。
- 内容: 付録 B.5 の 22 行を P2〜P5 で足す。
- 確認: `--area TYP` が `22 tests, 22 ok`。全体 `node scripts/conformance.mjs target/release/tsuzuri` が `128 tests, 128 ok`。`check-spec` が `ok (128 active, 0 retired, 128 tests)`。

### Phase 3: 残りの 10 領域

### 手順 12: LEX・SYN・STR の 30 規則

- 変更: `spec/rules/{LEX,SYN,STR}.md`・`tests/conformance/{LEX,SYN,STR}/**`（新規）。
- 内容: 付録 C の C.1〜C.3 の各 10 行を P2〜P5 で足す。
- 確認: 全体が `158 tests, 158 ok`。`check-spec` が `ok (158 active, …, 158 tests)`。

### 手順 13: MOD・TSK・DIA の 30 規則

- 変更: `spec/rules/{MOD,TSK,DIA}.md`・`tests/conformance/{MOD,TSK,DIA}/**`（新規）。
- 内容: 付録 C の C.4〜C.6。MOD は 9 件がディレクトリ形（複数ファイル）、DIA の 8 件が cli モード。
- 確認: 全体が `188 tests, 188 ok`。

### 手順 14: CLI・ABI・STD・EDN の 40 規則と Phase 3 の確認

- 変更: `spec/rules/{CLI,ABI,STD,EDN}.md`・`tests/conformance/{CLI,ABI,STD,EDN}/**`（新規）。
- 内容: 付録 C の C.7〜C.10。cli モードは CLI 10 件（`deterministic` 1 件を含む）・ABI 8 件・EDN 3 件。cli モードのテストは作業ディレクトリの外へ書かない（`new $DIR/app`・`-o $DIR/out.h`）。
- 確認: 全体が `228 tests, 228 ok`。`check-spec` が `ok (228 active, 0 retired, 228 tests)`。`--coverage` の `Total` 行が `228 228 228`（Active・Tested・Tests）。

### Phase 4: マーカー・カバレッジ・CI

### 手順 15: 言語リファレンスのマーカー

- 変更: `_tsuzuri/language-reference/**`（40 ページ。マーカーの行・項目の先頭だけ）。
- 内容: 最初に `BASE=$(git rev-parse HEAD)` で起点のコミットを控える。付録 B・C の各行の出典のページについて、規則の文（付録の「規則」の元になった 1 文）の直前に §9 のマーカーを足す。1 ページずつ `node scripts/check-docs.mjs <ページ>` を通す。
- 確認: `node scripts/check-docs.mjs` の出力の件数が手順 1 と同じ。`node scripts/check-spec.mjs --require-markers` の `marker_missing` が、言語リファレンスを Doc に持つ規則（163 件）で 0。各ページで `node scripts/check-spec.mjs --strip-markers <ページ> | cmp - <(git show $BASE:<ページ>)` が差なし。

### 手順 16: `docs/language.md` ほかのマーカーと `--require-markers`

- 変更: `docs/language.md`（63 件）・`docs/stability.md`（1 件）・`docs/architecture.md`（1 件）。
- 内容: 同上。
- 確認: `node scripts/check-spec.mjs --require-markers` が終了コード 0（228 件すべて）。変更した 3 ファイルで `--strip-markers` の出力が `$BASE` の版と同じ（手順 15 と同じ `cmp`）。

### 手順 17: カバレッジの表と文書ページ

- 変更: `_tsuzuri/language-reference/compiler/conformance.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/stability.md`、`README.md`。
- 内容: 「ドキュメント」の Phase 4 の行のとおりにページを書き、`node scripts/check-spec.mjs --write` で `<!-- generated:spec-coverage -->` のブロックを埋める。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/conformance.md _tsuzuri/language-reference/index.md` が通る。`node scripts/check-spec.mjs --require-markers` が 0（`coverage_block` を含む）。

### 手順 18: CI

- 変更: `.github/workflows/targets.yml`、`.github/workflows/spec.yml`（新規）。
- 内容: 付録 D.5 の step と yml を置く。`targets.yml` は「End-to-end compiler tests」の次に「Language conformance suite」を足し、`paths` に `spec/**`・`scripts/conformance.mjs`・`scripts/conformance/**`・`scripts/check-spec.mjs` を足す。`spec.yml` は PR で `spec/**`・`tests/conformance/**`・`docs/**`・`_tsuzuri/**`・`README.md` が変わったときに `node scripts/check-spec.mjs --require-markers` を流す。
- 確認: 手元で `node scripts/conformance.mjs target/release/tsuzuri --jobs 2` と `node scripts/check-spec.mjs --require-markers` が緑。PR で 2 つの job が緑（`gh run list`）。Phase 4 の完了報告に、CI の所要時間を書く。

## テスト計画

`tests/conformance_harness.mjs`（新規）45 件。名前・入力・期待は付録 D.4。A ディレクティブ 12（No.1〜12。コンパイラ不要）、B 期待の照合 5（No.13〜17。コンパイラ不要。No.16 は `src/trap.rs` を読む）、C 探索・実行・CLI 13（No.18〜30。No.20〜29 は実コンパイラで、native と小さい `.tz` だけ。WASM は No.29 の「使えないときの失敗」だけ）、D 規則表・検査器 14（No.31〜44。コンパイラ不要）、E 実リポジトリ 1（No.45）。

- **期待値は独立に作る**: ハーネスと検査器のテストの期待は、付録 D.4 の文字列から手で書く（実装の出力を写さない）。228 件の規則の期待は、付録 A の方法で**実機から得た値**を、文書の 1 文との一致を確かめたうえで固定したもの（コンパイラが期待の源だが、文との一致を人が確かめる）。
- **拒否のテスト**: `check-spec` の 17 個の名前（§7）それぞれに、その名前だけが出る木を作って確かめる（No.32〜42 がまとめて持つ）。ディレクティブの検証エラー（付録 D.2）は No.5〜12 で名前を照合する。
- **決定性**: `conformance.mjs`（`--timing` なし）と `check-spec` を 2 回実行して stdout が byte 単位で同じ（No.30 と No.40）。
- **資源上限**: タイムアウト（No.21 が `--timeout 0.01` で `timed out after 0.01 s` を確かめる）、出力の上限、64 KiB を超えるテスト（No.38）。
- **既存テストへの影響: なし**。根拠: `src/`・既存のテスト・fixture・`scripts/check-docs.mjs` を変えない。`tests/fixtures/conformance_mini/` は `Main.tz` を持たないので、`tests/parallel_frontend.mjs` の `check` は `E2001` で毎回同じ（手順 6 で実行して確かめる）。マーカーは `check-docs.mjs` の件数を変えない（手順 15・16 で確かめる）。

## ドキュメント

- **Phase 1**: `README.md`（検証コマンドの sh ブロックの `node scripts/check-docs.mjs` の行の前）と `docs/architecture.md`（「検証コマンド」の sh ブロックの `node tests/e2e.mjs target/release/tsuzuri` の次）に同じ 3 行を足す: `node scripts/conformance.mjs target/release/tsuzuri`（コメント: 仕様の規則の適合性テスト。`--area`・`--rule`・`--filter`・`--targets native`）、`node scripts/check-spec.mjs`（規則表・テスト・マーカーの整合）、`node tests/conformance_harness.mjs target/release/tsuzuri`。`spec/README.md`（新規）に ID の形式・表の見方・P1〜P6 を書く。
- **Phase 4**: `_tsuzuri/language-reference/compiler/conformance.md`（新規。日本語。節: 概要、規則の ID と表、適合性テストの書き方（§4・§5 の表の要約）、実行（コマンドと終了コード）、カバレッジ（`<!-- generated:spec-coverage -->` のブロック）、限界（WASM は `tests-pass` の 21 件だけ、Windows は未検証））。例は ` ```text ` の囲みで書き、` ```tsuzuri ` は使わない（`check-docs.mjs` の例の検査の対象外にする）。`_tsuzuri/language-reference/index.md` の「コンパイラ」の一覧（`[デバッグ](./compiler/debugging.md)` の次）に `- [仕様の規則と適合性テスト](./compiler/conformance.md)`。`docs/stability.md` の「安定として保つもの」の表に 1 行（対象: 仕様の規則の ID。保つこと: `spec/rules/` の ID を再利用せず、廃止は `retired`、意味が変わる規則は新しい ID）。`README.md` の検証コマンドの説明に 1 文。
- **統括者への申し送り**（実装者は編集しない）: `_features/GUIDE.md` の D-58 に「実装: `spec/rules/`・`tests/conformance/`・`scripts/check-spec.mjs`」の 1 行、§7（テストの書き方）の表に `node scripts/conformance.mjs` の行、GUIDE-LITE の検証コマンドの一覧に 3 行。完了報告に案を書く。
- **検証**: `node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-spec.mjs --require-markers`、`node scripts/check-spec.mjs --strip-markers <文書>` が起点と同じ。

## 受け入れ条件

**Phase 1**

- [ ] `node tests/conformance_harness.mjs target/release/tsuzuri` が 45 件で緑（`wasm-ld` がない環境でも No.29 は緑）。
- [ ] `node scripts/conformance.mjs target/release/tsuzuri --area OWN` が `30 tests, 30 ok, 0 failed`。`node scripts/check-spec.mjs` が終了コード 0。
- [ ] 出力に絶対パス・日時がない（No.30）。`node scripts/check-docs.mjs` と `node tests/parallel_frontend.mjs target/release/tsuzuri` が従来どおり通る。

**Phase 2**

- [ ] NUM・EVL・CTL・TYP の 98 件が加わり、全体が `128 tests, 128 ok`。`check-spec` が `ok (128 active, 0 retired, 128 tests)`。
- [ ] NUM の 21 件の `tests-pass` が WASM の 4 組（`--targets all`）を通る。

**Phase 3**

- [ ] 全体が `228 tests, 228 ok`。各領域に 10 件以上。`--coverage` の `Total` が `228 228 228`（Active・Tested・Tests）。
- [ ] 各領域の棚卸しの手順（付録 C）に従って、さらに 1 件ずつ足す練習をして P1〜P5 が回ることを確かめた（足した件は PR に含めず、手順の誤りを報告に書く）。

**Phase 4**

- [ ] `node scripts/check-spec.mjs --require-markers` が終了コード 0。マーカーを足した全文書で `--strip-markers` の出力が起点と同じ。`node scripts/check-docs.mjs` の件数が変わらない。
- [ ] `conformance.md` と `index.md` が `check-docs` を通り、生成ブロックが `--coverage-markdown` と一致する。
- [ ] `targets.yml` の新しい step と `spec.yml` が PR で緑。完了報告（GUIDE §13.2）に、実行時間・件数・停止条件に当たった事項・統括者への申し送りを書いた。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| ディレクティブを先頭に書く（診断のバイト位置・行が付録の値とずれる。DIA の 3 件が崩れる） | 末尾に置く。`footer_splits_directives_from_body`・付録 B・C の DIA-001・DIA-002 |
| 最後の行に通常のコメント（`// done`）を書く。ディレクティブが読まれず `missing_rule` | `footer_stops_at_plain_comment_or_code` |
| 末尾に `// note: …` 以外の `// word: …` 形のコメントを書く | `unknown_directive`（`parse_unknown_directive_and_expectation`） |
| JSON 文字列のエスケープ（`\n`・`\"`・`\\`）を落とす | `tokens_reject_bad_strings`、実行時の不一致 |
| トラップの判定にシグナルや OS 固有の終了コードを使う | 判定は stderr の `trap:` の行と終了コード 1 だけ（`trap_line_parsing`） |
| 別ファイルの診断に `line=` だけを付ける（`Main.tz` を探して不一致） | `file=` を付ける（MOD-008 の `file=Lib.tz`）。`match_diagnostic_default_file` |
| `test --json` の行を丸ごと比べる（`duration_ms` が毎回違う） | `type`・`status`・`passed`・`failed` だけを読む（`test_summary_parsing`） |
| `stdout`・`trap` の組に `wasm-*` を付ける（WASM に標準出力を読む手段がない） | `targets_not_allowed`（`parse_targets_rules`） |
| Tests のセルを手で直す | 常に `check-spec --write`。`tests_cell`（`spec_tests_cell_and_write`） |
| 規則の文の `\|`（`\|>`・`\|\|`）を表の区切りと取り違える（NUM-016・EVL-005・EVL-015・SYN-004） | `spec_reads_tables_and_unescapes_pipes` |
| マーカーをリストや表の行の間に置き、リスト・表が分かれる | §9 の 3 か所だけ。目で確かめるために、手順 15 の `check-docs` と GitHub 上の表示 |
| 規則の番号を詰める・再利用する | `rule_order`・`rule_duplicate`・「変えてはいけない意味」 |
| `-O0` では通るが `-O3` で落ちる、native では通るが WASM で落ちる | 停止条件 3。テストを弱めない |
| `tests/features.mjs` のハーネスを流用しようとする（`import` できない。C のホストと確保追跡に結びつく） | 流用しない（D3）。`grep -c '^export ' tests/features.mjs` が 0 |

## 対象外

- 規則の網羅（領域ごとの棚卸しは Phase 3 の後も続く活動）。コンパイラ・std・診断・文法・CLI の変更。新しい診断コード・フラグ・環境変数。
- WASM での標準出力・トラップの照合（ホストの実装が要る）。Windows の対応。他の実装への配布物。
- `tests/*.rs`・`tests/*.mjs` の置き換え、`tsuzuri explain`（G23）との連携、`repo-kb`（G32）への索引、診断の `kind` の一斉付与、カバレッジ率や実行時間の閾値。

## 決定事項

### D1: 規則・テスト・文書の 3 点を D-58 のとおりに結ぶ

規則の本体は `spec/rules/<AREA>.md` の表、テストは `tests/conformance/<AREA>/`、文書は HTML コメントのマーカー。理由: どれも機械で検査でき、散文を変えずに足せる。却下: 規則をテストのコメントだけに置く（表で一覧できない）、規則を文書の中の表にする（文書が読みにくくなる）。状態: 承認済み。

### D2: ディレクティブは `.tz` の末尾に置く（種は先頭）

理由: 診断のバイト位置・行番号が本体のまま（付録の期待がそのまま使える）。先頭に意味のあるもの（BOM・`#!`）を置く規則（LEX）も書ける。試作で先頭に置いたとき、DIA の 3 件（バイト位置・行を含む）が崩れた。却下: 先頭（位置がずれ、行番号の補正が要る）、別ファイル（2 ファイルに分かれ、片方を忘れる）。状態: 承認済み。

### D3: ハーネスは新規で、`tests/features.mjs` を流用しない

理由: `features.mjs` は `import` できないスクリプトで、C のホストと確保追跡に結びつく。必要なのはコンパイラの CLI を呼ぶことだけ。却下: `features.mjs` の `run()` の複製（C のホストが要る）、Rust の統合テスト（ビルドが要る）。状態: 承認済み。

### D4: 実行はコンパイラの CLI（`check --json`・`run`・`test --json`・`build`・`new`・`fmt`・`api-diff`）だけ

理由: 利用者が見るものと同じ面を検査する。コンパイラの内部 API に依存しない。却下: ライブラリとして Rust から呼ぶ。状態: 承認済み。

### D5: WASM を通すのは `tests-pass` だけ

理由: `run` は native だけで、WASM の標準出力を読む手段がない。`tsuzuri test --target wasm32` は合否を返す。数値の規則（NUM の 21 件）は `test` で書き、4 組（native/WASM × `-O0`/`-O3`）を通す。却下: WASM のホストをハーネスに書く（D-50 の「既定の WASM import を足さない」には触れないが、ホストの実装が本チケットの範囲を超える）。計画（未対応）: 標準出力を読む WASM ホスト。状態: 承認済み。

### D6: `check-error`・`trap` は `line=`・`file=`（と `kind=`）で位置を照合する

理由: コードだけだと、別の場所の同じコードで通ってしまう。`kind=` は G23 の後に使える（本チケットのテストは使わない）。却下: コードだけの照合、メッセージの照合（文言は保証の対象外）。状態: 承認済み。

### D7: 表の Tests の列は生成する（`check-spec --write`）

理由: D-58 が表に「テストの一覧」を求め、手で書くとずれる。却下: 列なし（D-58 に反する）、手書き（ずれる）。状態: 承認済み。

### D8: 規則の文は英語で `MUST`・`MUST NOT`・`MAY`

理由: D-56（AI 向けの成果物は英語）と RFC 2119 の語。`SHOULD` は検査できないので不可。却下: 日本語（検索・モデルの習熟度）、`SHOULD` の許可。状態: 承認済み。

### D9: Doc は `#slug` を省略できる。文はマーカーで特定する

理由: 付録の 228 件のうち 149 件は節を持たないページ単位の出典。見出しを必須にすると小さいモデルが節を選ぶ負担が増える。マーカー（Phase 4）が文を指す。却下: 見出しの必須化。状態: 承認済み。

### D10: 初期の規則は 1 規則 1 テスト（228 件）。2 件以上のテストを 1 規則に付けてよい

理由: 検証済みの 1 行 1 テストを確実に移す。追加の境界値は後から足せる。却下: 初期から複数のテスト（検証の負担が増える）。状態: 承認済み。

### D11: `check-spec` はコンパイラを使わない Node のスクリプト。CI は軽い `spec.yml` と `targets.yml` の step

理由: 文書の変更でも走らせたい（`targets.yml` の `paths` に `docs/**`・`_tsuzuri/**` はない）。コンパイラを使う `conformance.mjs` は 4 ホストの `targets.yml` で走らせる。却下: `targets.yml` だけ（文書だけの変更で走らない）、Rust のテスト（ビルドが要る）。状態: 承認済み。

### D12: マーカーは HTML コメントで、置き場所を 3 つに限る

理由: 表示を変えず、`--strip-markers` で元に戻ることを機械で確かめられる。却下: 文書の中の表（読みにくい）、属性つきの見出し（表示が変わる）。状態: 承認済み。

### D13: 載せるのは実機で確かめた規則だけ。食い違う規則は足さない

理由: 適合性スイートが落ちるのは、コンパイラが文書の約束を破ったときだけにする。却下: 実機に合わせて文を直した規則（約束の検査にならない）。状態: 承認済み。

### D14: 新しい名前を作らない

診断コード・コンパイラのフラグ・環境変数・属性・std・予約語を足さない（`scripts/conformance.mjs` と `scripts/check-spec.mjs` の引数は開発用スクリプトのもので対象外）。コンパイラの指定は既存の `TSUZURI_BIN`（`scripts/check-docs.mjs` と同じ）と位置引数（`tests/*.mjs` と同じ）。カバレッジ JSON の `schema` は `tz-spec-coverage/1`。理由: D-55 の割り当てを増やさない。状態: 承認済み。

## 付録

詳細は次のファイル。**付録 A**（検証の方法・行からファイルへの変換・結果）と**付録 B**（OWN・NUM・EVL・CTL・TYP の 128 規則）は [G33-appendix.md](G33-appendix.md)、**付録 C**（残り 10 領域の 100 規則と各領域の棚卸しの手順）は [G33-appendix-areas.md](G33-appendix-areas.md)、**付録 D**（トラップの種類の表・ディレクティブの検証エラー・fixture・自己テストの一覧・CI の yml・出力例）は [G33-appendix-harness.md](G33-appendix-harness.md)。

## 実装と検証（2026-10-30）

- **範囲**: Phase 1〜4 をすべて実装した（1 つの PR）。冒頭の「方針の最新化」のとおり、起票後の変更に合わせた。
- **Phase 1**: `scripts/conformance/{directives,discover,expect,run,spec}.mjs`、`scripts/conformance.mjs`、`scripts/check-spec.mjs`、`tests/conformance_harness.mjs`（付録 D.4 の 45 件）、`tests/fixtures/conformance_mini/good/`（付録 D.3）、`spec/README.md`、OWN の 30 規則。
- **Phase 2・3**: 付録 B・C の残りの 198 規則（NUM 30・EVL 20・CTL 26・TYP 22、LEX・SYN・STR・MOD・TSK・DIA・CLI・ABI・STD・EDN 各 10）。行からファイルへの変換（§3）は手順どおり機械的に行い、食い違った 24 行だけを上の表のとおり直した。
- **Phase 4**: 文書のマーカー 228 個（43 ファイル。`check-spec --strip-markers` で各ファイルが元と byte 単位で同じ）、`--require-markers`、`_tsuzuri/language-reference/compiler/conformance.md`（カバレッジの生成ブロック）、`index.md`、`docs/stability.md`、`README.md`、`docs/architecture.md`、`.github/workflows/targets.yml` の step と push の paths、`.github/workflows/spec.yml`。
- **検証**: `node scripts/conformance.mjs target/debug/tsuzuri --jobs 4`（macOS、native と wasm32、`TSUZURI_WASM_LD`）が `conformance: 228 tests, 228 ok, 0 failed`（約 64 秒）。`node tests/conformance_harness.mjs` が 45 件。`node scripts/check-spec.mjs --require-markers` が `check-spec: ok (228 active, 0 retired, 228 tests)`。マーカーを足した 40 ページの `node scripts/check-docs.mjs` が通る（40 ページ・775 リンク・241 例・460 実行・7 テスト）。
- **統括者への申し送り**: GUIDE の D-58 に「実装: `spec/rules/`・`tests/conformance/`・`scripts/check-spec.mjs`」、§7 の表に `node scripts/conformance.mjs target/release/tsuzuri --rule <ID>`、GUIDE-LITE の検証コマンドに `node scripts/conformance.mjs`・`node scripts/check-spec.mjs`・`node tests/conformance_harness.mjs` の 3 行を足す案。
- **レビュー（PR #69）**: (1) 時間切れでコンパイラだけを止めると、`tsuzuri run` が起動したプログラム（パイプを継いでいる）が残り、`close` を待ち続けた。コンパイラを自分のプロセスグループで起動し（POSIX の `detached`。Windows は `taskkill /T`）、時間切れ・出力の上限・終了のときにグループごと止め、終了後も 2 秒でパイプを閉じて打ち切る。ランナーの終了と SIGINT・SIGTERM でも止める。回帰テストは、パイプを持った子を残す偽のコンパイラ（`run_timeout_stops_the_programs_that_the_compiler_started`）。(2) 読めない `cli:` の行と `deterministic` の組み合わせで検証が例外で落ちた（`cli` が `null`）のを、集めた誤りを返すようにした。(3) `check-error`・`check-warning` に `column=N`（`line=` と一緒）と、最後に置く `message "…"`（メッセージの部分一致）を足し、DIA-004（`_` の案内）と DIA-009（名前の桁）で使う。失敗の報告は `コード@ファイル:行:桁`。(4) 規則の文を実装に合わせて狭めた: ABI-004（`string` のように C ABI が運べないフィールドを持つ record。スカラーだけの record は使える）、CLI-002（`unit` の結果は何も出さない。テスト `CLI-002-unit-result-prints-nothing` を追加）、CTL-008（`i` は境界の整数型。接尾辞のないリテラルなら `i32`）、DIA-008（診断が 1 つもない `check`）、EDN-001（マニフェストの既定は 2026。テストを 2026 だけが受理する `1l` に）、EDN-007（テストを 2027 が `E0004` で拒否する `1l` に。`task` はどちらの edition でも予約語）、NUM-002〜005（`@checked` の外の固定幅の整数）、OWN-012（`Drop` のない利用者の record）、STD-009（整数型で、範囲外の数も `None`。テストに範囲外を追加）、TSK-008（`Atomic.fetch_add` で足す場合）、TYP-016（`Char.to_u16` と `Char.of_u16`）。ABI-008 は「`int32_t` で渡り、入りで幅へ切り詰め、出で 32 bit へ拡張する」とし、ヘッダーのテストに加えて、生成した LLVM IR の公開ラッパーの `trunc`・`sext` を確かめるテスト `ABI-008-narrow-integer-wrapper` を足した（ハーネスは C のホストを持たないので、実行時の呼び出しは確かめない）。EDN-005（`tsuzuri new` は `edition = "2027"` を書く）は正しいので変えていない。(5) マーカー EVL-013 を比較表の行から評価順序の段落へ、SYN-007 を `let` の一覧からネスト深度の表の行へ移した。規則 228・テスト 230 で `node scripts/conformance.mjs` が全件合格、`tests/conformance_harness.mjs` 46 件。
- **レビュー 2・3 回目（PR #69）**: (1) CLI-002（`Display` を持ち `IO` でない結果だけを表示する）、STD-009（`i64` の 10 進・数でない文字列・範囲外）、SYN-002（同じ行の `let` と続く式の `;`）を、テストが確かめることに狭めた。PR の説明の、ターゲットごとに走る期待の種類を直した。(2) 228 個のマーカーをすべて点検し、規則を述べる文・表の行・箇条へ 158 個を移した（5 行の Doc の欄も、述べている節へ）。述べる文が文書にない DIA-009・TYP-018 は節の近い文に残し、EVL-003・NUM-025・NUM-028・SYN-003・TSK-008 は述べ方が弱い（文を足すのは後の作業）。TSK-010 には規則を述べる 1 文を足した。(3) 行頭の `<!--` は CommonMark では HTML ブロックになり、その行の Markdown が表示されないので、箇条のマーカーは行の末尾に置く（68 行を移した）。`check-spec --strip-markers` は行末のマーカーを前の空白ごと取り、全 42 ページで元の文と byte 単位で一致する。生成ブロック（`diagnostic-kinds`）の中には置かない（LEX-008 を `E0001` の分類の行へ）。置き方は `spec/README.md` と `conformance.md` に書いた。
- **レビュー 4 回目（PR #69）**: (1) `check-error` は最初の誤りと照合する（回復で後に出た誤りでは合格しない。`check-warning` はどの警告でもよい）。全 230 テストがこの判定でも合格。(2) テストを強めた: ABI-002 はヘッダーに `tz_add(` があることも確かめ、NUM-028 は `(x + 1) - x`（`x = 2^24`）が 0 になる連鎖を足し、TYP-005 は f32 と f64 で表示の違う `1.0 / 3.0` を出力で比べる。(3) 規則の文を狭めた: LEX-005（`union`・`yield`・`new` はドットの後のメンバー名にできる）、LEX-010（補間文字列の本文の `}`）、SYN-005（トップレベルで、完結した `let` や式の後の行）。(4) TYP-006 の空の配列の `E1004` を `type-inference.md` に 1 文で書いてマーカーをそこへ、TYP-018 のペイロードの検査を `union.md` に書いてマーカーと Doc の欄をそこへ移した。
