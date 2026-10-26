# G27: ビルド時の KB（`target/tz/kb`、brief カード、増分、鮮度）

| 項目 | 内容 |
| --- | --- |
| ID | G27 |
| 優先度 | P1（D-50 の表） |
| 規模 | L。Phase 1 M（書き込み・manifest・鮮度・フラグ）、Phase 2 M（brief カードと `kb build`／`verify`／`clean`／`export`）、Phase 3 M（増分）、Phase 4 M（std の KB と `KbView` の std）、Phase 5 S（計測の記録と台帳） |
| 依存 | [G26](G26-facts-model.md) Phase 1（`src/kb/`・ID 規則・`KbModel::build`・`write_symbols`・`analyze_inputs_core`・`check_modules_captured`）。graph・tests・facts・layout の書き手（G26 Phase 2〜6）は**あれば使い、なければそのファイルとカードの該当行を出さない**（縮退。「前提とする他チケットのインターフェース」）。弱い依存: [G22](_completed/G22-diagnostic-v2.md)（`E2009` に kind を付ける。未実装なら付けない） |
| 後続 | G28（照会と `impact`）、G29（MCP）、G30（図表）、G31（AI 作業コスト指標の KB 方式）、G35（生成物による文書） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。確認に使った作業ツリーは `9dd0fbbc`（main）で、`git diff --stat 9dd0fbbc a2313d19` は `docs/benchmarks.md` の 41 行だけ。ソースは同じ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | コンパイラは解析のたびにシンボル・呼び出し・効果・トラップ箇所を内部で得るが、`check` が終わると捨てる。AI は同じ事実を `grep` とファイルの全文読みで毎回作り直す（D-50 の調査結論 (1)）。KB はその事実と小さな要約（brief カード）を `target/tz/kb` に決定的に残し、最初の 700〜2,400 字から読み始められるようにする |
| 手本にする既存実装 | 置換とマーカー: `src/driver.rs` の `document`・`protect_documentation`・`TemporaryDirectory`。ハッシュ: `src/cache.rs` の `Sha256`・`file_digest`・`read_regular`。識別と interface: `src/frontend_cache.rs` の `compiler_identity`・`interface_hash`・`ModuleRecord`。並列: `src/parallel.rs` の `Parallelism::command_line`・`workers`・`ordered_for_each`。通知（W2001）: `src/main.rs` の `main`。サブコマンドと引数のテスト: `api_diff_command`・`new_project`・`rejects_ambiguous_or_unused_arguments`。探索: `collect_sources`。計測: `benchmarks/run-cache.mjs` |
| 主な影響ファイル | 新規 `src/kb/{settings,input,manifest,write,emit,build}.rs`（Phase 1）・`{store,brief,open,command}.rs`（Phase 2）・`incremental.rs`（Phase 3）・`std_kb.rs`（Phase 4）、`tests/kb_{build,brief,incremental,std}.rs`、`tests/fixtures/kb_shop/`、`tests/golden/kb/`、`_tsuzuri/language-reference/compiler/kb.md`。変更 G26 の `src/kb/{mod,model}.rs`・`src/kb_capture.rs`、`src/main.rs`・`driver.rs`・`lib.rs`・`frontend_cache.rs`・`package.rs`、`scripts/check-docs.mjs`、文書（Phase 5: `docs/benchmarks.md`・`GUIDE.md`） |

## 実装者向け早見

- **最初に読む**: [GUIDE-LITE.md](GUIDE-LITE.md)、`GUIDE.md` の D-52、G26 の「仕様」（行のキー）、このチケットの「仕様」「設計」。例は [G27-appendix.md](G27-appendix.md)。
- **最初に書くテスト**: 手順 2 の `tests/kb_build.rs::check_creates_the_kb_and_a_second_check_writes_nothing`。
- **D-55 に足す名前**: フラグ `--force`（`kb build`）・`--std`（`kb build`／`kb clean`）・`--no-refresh`（`kb export`。G28 も使う）、`E2009` の kind `kb_missing`・`kb_stale`・`kb_broken`・`kb_not_ours`・`kb_incomplete`。
- **ファイルは 2 種類**: 名前が `.` で始まらないもの（どの機械でも同じ byte）と、機械ごとのもの（`.tsuzuri-kb`＝マーカーと stamp、`.tsuzuri-reuse.json`＝Phase 3 の鍵）。絶対パス・日時・stamp を前者に入れない。
- **費用はファイルの個数で決まる**（作成 0.21 ms、削除 0.13 ms）。L2 は 256 件まで、行のないモジュール別ファイルは書かず、書き込みは並列にする（§5）。
- **完了の見分け方**: Phase 1＝`check` が KB を作り、2 回目は何も書かず、`--no-kb` と出力が同じ。Phase 2＝付録 B〜D のカードが golden と一致し `kb verify` が通る。Phase 3＝増分と `--force` が byte 単位で同じ。Phase 4＝`kb build --std` と `KbView` の std。Phase 5＝記録と台帳。迷ったら「何も書かない・警告 1 行」に倒す（KB は作り直せるキャッシュ）。

## 目的

利用者（AI を含む）が、プロジェクトの構造を**コンパイラが知っているとおりに、少ない文字数で**読めるようにする。`target/tz/kb/` に、D-52 のファイル（`symbols.jsonl`・`graph.jsonl`・`facts/`・`layout/`・`tests.jsonl`・`diag.jsonl`）と、そこから決定的に作る brief カード（L0＝プロジェクト 2,400 字、L1＝モジュール 1,600 字、L2＝関数・型 700 字）を置く。`check`・`build`・`run`・`test` が自動で作り、入力が同じなら**何も書き換えない**。

HEAD の測定（付録 A）: `check` は 1／200／1,000 モジュールで 39／70／446 ms。何も変わっていないときの KB の費用（ソースのハッシュ 1 回と manifest の読み取り 1 回）は **5 ms 未満**に収める。実行ファイル全体のハッシュ（ビルドキャッシュの鍵）は 50 ms で `check` より重く、鮮度には使わない（D3）。`tsuzuri new` の `.gitignore` は `target/` を無視せず、`collect_sources` は `target/` を読む。20,000 ファイルの KB は `check` に 19 ms を足すので、KB のディレクトリは読まない（D4）。

## 着手条件と停止条件

### 着手条件

- 依存: G26 Phase 1 が `done`（`grep -n "fn build" src/kb/model.rs` と `grep -n "fn analyze_inputs_core" src/lib.rs` が各 1 件）。Phase 2 以降はあるものだけ使う。G26 のテストは `tests/kb.rs`・`tests/golden/kb/{point,polymorphism}/` を持つので、このチケットは `tests/kb_build.rs`・`tests/kb_brief.rs`・`tests/kb_incremental.rs`・`tests/kb_std.rs` を使い、golden は同じディレクトリの `brief/`（Phase 2）に足す。
- ベースライン（GUIDE §2.3）: `cargo build --release --locked && cargo test --locked` の件数を控える。`examples/*` と `tests/fixtures/*` を `check` した標準出力・標準エラー・終了コードを `target/tz-g27/before/` に保存する（手順 6 の比較に使う）。

### 停止条件（止めて報告する）

1. G26 Phase 1 が `check`・`build` の解析の中で KB を作る口（`KbModel::build` が `pub(crate)`、`analyze_inputs_core` が `capture` を取る）を出荷していない。`kb::collect(&Project)` は解析をやり直すので `check` では使わない。選択肢: A＝口を G26 に足す（既定案）、B＝G27 を待たせる。
2. KB の有無で `check`・`build`・`run`・`test` の標準出力・標準エラー・終了コードが変わる（手順 6 の比較が 0 でない）。
3. 同じソースで 2 回作った KB（別のルートに置いた場合も）が、`.` で始まらないファイルのどれか 1 つでも 1 byte 違う（絶対パス・時刻・`HashMap` の順の漏れ）。
4. 8 プロセスの並行で `kb verify` が通らない、または `.tsuzuri-` で始まる一時ディレクトリが残る。
5. 既存の suite（`tests/parallel_frontend.mjs`・`cache.mjs`・`frontend_cache.mjs`）が KB のせいで落ちる（`TSUZURI_KB=0` で通る）。選択肢: A＝その suite の環境に `TSUZURI_KB=0`、B＝書き込みを見直す。期待値は変えない。
6. G26 の行のキーが「カードが使う行のキー」（§7 の末尾）と食い違う。選択肢: A＝表と付録の行を G26 に合わせる、B＝G26 に追加を求める。
7. Phase 3: 差分テストで増分と `--force` が 1 byte でも違う。選択肢: A＝鍵の依存を広げる、B＝Phase 3 を出荷しない（Phase 1・2 は壊れない）。
8. Phase 3: 着手の条件（§10）を満たさない、または手順 13 の計測で増分が KB の生成時間の 10% も縮めない。選択肢: A＝出荷して計測を記録、B＝再利用のコードを入れない。
9. Phase 4: std 全体の解析（`stdlib::SOURCES` を `ModuleOrigin::Std` の入力として `analyze_inputs_core`）が失敗する。
10. `cargo check --all-targets --target x86_64-pc-windows-msvc` が通らない（`rename`・パス）。または `unsafe`・新しい crate・既定の WASM の import・予約語が必要に見える。

## 現状（HEAD `a2313d19`。作業ツリー `9dd0fbbc`）

実測はすべて `target/release/tsuzuri`（9,465,008 byte）。環境: Apple 製 macOS、`rustc 1.98.1`。作業ディレクトリは `target/tz-G27-work/`（`/tmp` を使えない環境のため。手順では `/tmp/tz-G27-*` と読み替えてよい）。

**KB は存在しない。** `grep -rn "target/tz\|E2009\|--kb\|TSUZURI_KB" src tests docs _tsuzuri README.md` は `_perfs/PR08` の `target/tz/` の言及以外 0 件。

**CLI**（`src/main.rs`）: `parse_arguments` が `Arguments` を作る（`Arguments { .. }` を書く所は `lsp` の早期 return と末尾の 2 つ）。`new`・`fetch`・`publish`・`api-diff`・`bindgen`・`repl` は `main` が `parse_arguments` の前に別関数へ渡す。解析は `Project::load`（`check`・`build`・`run`）か `load_for_tests`（`test`・`bench`）で読み、`project.analyze_cached(cache, kind)` が `Result<CheckedModule, DiagnosticSet>` を返す。**失敗すると `CheckedModule` は無い**。`main` は `run_action` が返す `Vec<String>` を、テキストではそのまま、`--json` では `{"severity":"warning","code":"W2001","message":…}`（スパンなし）で `eprintln!` する。

**既存の置換・探索・識別**: `document`（`tsuzuri doc`）は `TemporaryDirectory`（`.tsuzuri-<pid>-<n>`）の `next/` に書き、マーカー `.tsuzuri-docs` を足し、既存の出力を `previous` へ rename で退避して `next` を rename する（失敗時は戻す）。`protect_documentation` は、マーカー（4,096 byte 以下の通常ファイル）のないディレクトリとソースを含むディレクトリを `E2003` で拒む。`Project` と `.tsuzuri-docs` に固定なので、KB は同じ手順を `write.rs` に書く（D1）。`collect_sources` はドット始まりの名前だけを飛ばし、`target/` は読む（上限 1,024 ディレクトリ・深さ 16・4,096 ファイル）。`create_project`（`tsuzuri new`）は 3 ファイルを作り、`.gitignore` は `.tsuzuri/\n`（テスト `creates_projects_that_declare_their_namespace` は `created.len() == 3` だけを見る）。`cache::build_key` は実行ファイル全体の `file_digest` を使い、frontend cache は `compiler_identity()`（版・長さ・更新時刻・inode・ctime）で各モジュールの `(source, interface)` を記録する。

**ID の名前空間**: manifest のないフォルダのルートは、フォルダ名を PascalCase にした名前空間（`folder_namespace`。`point` → `Point`）を持つので、CLI で作る KB の ID は `Point::Main` になり、G26 の golden（名前空間なし）の `Main` とは違う。カードの例は golden の ID を使う（付録 B・C）。

**測定**（付録 A）: `check` の中央値（9 回）は 1／200／1,000 モジュールで 38.9／70.2／445.7 ms。9,465,008 byte の実行ファイルの SHA-256 は 49.5〜52.8 ms。ファイルの作成は 0.21 ms/個、削除は 0.13〜0.2 ms/個、hard link は 0.9 ms/個。費用は**ファイルの個数**で決まり、hard link は再利用の道具にならず、並列の書き込みは約 3 倍速い。

## 変えてはいけない意味

- `check`・`build`・`run`・`test` の標準出力・標準エラー・終了コード・`--json` の診断・生成物（IR・object・`.wasm`・実行ファイル）。KB で増えるのは、書けないときの警告 1 行だけ。
- 既存の診断コードの意味と `W2001` の形（`E2009` は D-55 に割り当て済みの新規）。ビルドキャッシュと frontend cache の鍵・形式（`FRONTEND_FORMAT` を上げない。Phase 3 は frontend cache を**読むだけ**）。`tsuzuri doc` と `.tsuzuri-docs`。D-52 のキーとファイル名（足すだけ）。
- `tsuzuri new` が作るファイルの個数と名前（`.gitignore` の中身だけが `.tsuzuri/\ntarget/\n` になる）。
- Phase 1 だけ出荷しても `kb` サブコマンドは無く、KB は完全（カードが無いだけ）。Phase 3・4 を出荷しなくても Phase 1・2 は壊れない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | フラグ・環境変数。`check`／`build`／`run`／`test` が `target/tz/kb`（マーカー、`manifest.json`、G26 の出力、`diag.jsonl`）を作る。鮮度、置換、並行、失敗の通知、失敗した解析の最小 KB。`collect_sources` の除外、`tsuzuri new` の `.gitignore` | M | G26 Phase 1 | 読み手のいないキャッシュが増えるだけ |
| 2 | brief カード（L0・L1・L2）、`kb build`・`verify`・`clean`・`export`、読み手 `store.rs`（`KbData` と、`StdSource::None` だけの `KbView`）・`open.rs`（G28・G29 が使う） | M | Phase 1 | ファイルが増えるだけ |
| 3 | 増分: facts／layout の再利用。interface の配管 | M | Phase 2（G26 Phase 3 以降で意味がある） | 再利用できなければ全部作る。出力は同じ |
| 4 | std の KB（`kb-std/<id>/`）、`KbView` の std（`StdSource::Cache`・`Memory`）、`--std` | M | Phase 2 | 無ければ std の ID が引けないだけ |
| 5 | 計測の記録、台帳 | S | 出荷した Phase | 文書だけ |

## 仕様

### 1. フラグ・環境変数・既定（Phase 1）

| 名前 | 受け付けるコマンド | 意味 |
| --- | --- | --- |
| `--kb` | `check`・`build`・`run`・`test` | KB を作る。ルートが「プロジェクト」でなくても作る |
| `--no-kb` | 同上 | 作らない |
| `--kb-dir DIR` | 同上と `tsuzuri kb …` | 置き場。相対パスはカレントディレクトリから読む。空文字列は拒否 |
| `TSUZURI_KB` | 同上 | `0`＝作らない、`1`＝作る、未設定または空＝既定。ほかの値は `E2000` |
| `TSUZURI_KB_DIR` | 同上と `kb` | `--kb-dir` と同じ。空は未設定。`--kb-dir` が優先する |

- **ルート** = `Project::root_directory()`（ファイル入力は親、ディレクトリ入力はそのもの）。**プロジェクト** = ルートに `Tsuzuri.toml` がある（`Project::root_package` が `Some`）か、ルートに `Main.tz` が通常ファイルとしてある。
- `doc`・`fmt`・`bench`・`script`・`repl`・`lsp` は KB を作らず、フラグも受け付けない（単独のファイルや `script` の入力は `tsuzuri kb build FILE` で作る。D-52 も同じ）。

自動で作るかの決定表（上から最初に当てはまる行）:

| `--kb` と `--no-kb` | `TSUZURI_KB` | ルートがプロジェクト | 結果 |
| --- | --- | --- | --- |
| 両方 | 任意 | 任意 | `E2000` `--kb and --no-kb cannot be used together`（終了コード 2） |
| 任意 | `0`・`1`・空・未設定以外 | 任意 | `E2000` `TSUZURI_KB must be 0 or 1`（終了コード 2。ソースを読む前に検査する） |
| `--kb` | 任意 | 任意 | 作る |
| `--no-kb` | 任意 | 任意 | 作らない |
| なし | `1` | 任意 | 作る |
| なし | `0` | 任意 | 作らない |
| なし | 未設定・空 | はい | 作る |
| なし | 未設定・空 | いいえ | 作らない |

置き場: `--kb-dir` → `TSUZURI_KB_DIR`（空でない）→ `<ルート>/target/tz/kb`。**置き場がソースを含む（ソースの親、または同じ）ときは作らない**（`E2009` kind `kb_not_ours`。`protect_documentation` と同じ判定）。

引数の拒否（`E2000`、終了コード 2。メッセージは英語で固定）:

| 入力 | メッセージ |
| --- | --- |
| `--kb`・`--no-kb`・`--kb-dir` の重複 | `kb specified more than once`・`no-kb specified more than once`・`kb-dir specified more than once` |
| `--kb-dir` に値がない | `--kb-dir needs a value`（`next_value` の既存の形） |
| `--kb-dir ""` | `--kb-dir must not be empty` |
| `doc`・`fmt`・`bench` で指定 | `--kb, --no-kb, and --kb-dir are only valid with check, build, run, or test` |

### 2. KB のディレクトリ（D-52 のとおり。G27 が書くものを太字にする）

| パス | 書き手 | 書く条件 |
| --- | --- | --- |
| `.tsuzuri-kb` | **G27** | 常に。マーカー（下記）。機械ごとのファイル |
| `manifest.json` | **G27** | 常に（§3） |
| `diag.jsonl` | **G27** | 常に。0 行でも書く。診断 1 件が 1 行で、`--json` の診断と同じ形。ただし `path` は**ルートからの相対パス**（`/` 区切り。`SourceFile::relative_path`）で、`W1006`（`--warn implicit-copy`）は入れない（D14）。`DiagnosticSet::omission_note` があれば `{"severity":"note","message":…}` の 1 行を最後に足す |
| `symbols.jsonl` | G26 Phase 1 | 解析が成功したとき |
| `graph.jsonl`・`tests.jsonl` | G26 Phase 2 | 同上。書き手があれば 0 行でも書く |
| `facts/<stem>.jsonl`・`layout/<stem>.jsonl` | G26 Phase 3〜6 | 同上。`KbModel::fact_modules()`・`layout_modules()` が返す**モジュールの key**（`Geometry.Metrics`）ごと。**行のあるモジュールだけ**（G26 は行のないモジュールを返さない。読み手は無いファイルを 0 行として扱う） |
| `brief/L0.md`・`brief/L1/<stem>.md`・`brief/L2/<stem>.md` | **G27 Phase 2** | 同上（§7）。L1 はモジュールの key、L2 は symbol の ID から作る |
| `.tsuzuri-reuse.json` | **G27 Phase 3** | 増分の鍵。機械ごとのファイル |

**決定性**: 名前が `.` で始まらないファイルは、同じソースと同じ版のコンパイラから**どの機械・どのディレクトリでも同じ byte** になる。絶対パス・日時・stamp・スレッド数・`HashMap` の反復順を入れない。行は G26 の整列、`manifest.json` の `files`・`modules` は byte 順に整列する。

**ファイル名の幹（`<stem>`）**: モジュールの key（`Cart`。`::` を含まない）または symbol の ID から作る。(1) `::` を `.` にする。(2) `[A-Za-z0-9_.]` 以外の文字は、その UTF-8 の各 byte を `%XX`（大文字の 16 進）にする。(3) 同じディレクトリで、小文字にした名前が同じになる名前が複数あれば、byte 順に処理し、2 つ目以降の拡張子の前に `~1`、`~2` を足す（大文字小文字を区別しないファイルシステムでの衝突を避ける）。例: ID `Acme::Shop::Cart.total` → `Acme.Shop.Cart.total.md`、key `Cart` → `Cart.jsonl`。key と ID の対応は `manifest.json` の `modules`（§3）にある。

**マーカー** `.tsuzuri-kb`（UTF-8、2 行）:

```text
generated by tsuzuri kb
stamp <64 桁の 16 進>
```

有効なマーカー = 通常ファイル（シンボリックリンクでない）、4,096 byte 以下、1 行目が `generated by tsuzuri kb`。**stamp** = `SHA-256(field("kb-format", 1u32 の little endian), field("compiler", frontend_cache::compiler_identity()))`。実行ファイルの内容全体は読まない（D3）。`compiler_identity()` が `None` なら KB を作らず通知する（`kb not updated: cannot identify the running compiler`）。

### 3. `manifest.json`（Phase 1）

キーはこの順（`version` と `counts` の一部は条件つき）。型・意味:

| キー | 型 | 内容 |
| --- | --- | --- |
| `schema` | 文字列 | `"tz-kb/1"` |
| `compiler` | 文字列 | `"tsuzuri "` + `env!("CARGO_PKG_VERSION")` |
| `edition` | 文字列 | ルートの `Project::sources[root].edition` の表示（`2026`） |
| `project` | 文字列 | `Tsuzuri.toml` の `name`。なければルートの `file_name`（canonicalize した後。取れなければ `project`） |
| `version` | 文字列 | マニフェストの `version`。マニフェストがなければ**キーごと省く** |
| `complete` | 真偽 | 解析が成功したか（§6） |
| `source_hash` | 文字列 | 入力のハッシュ（下記）。64 桁の小文字 16 進 |
| `counts` | 対象 | `modules`（`modules` の長さ）・`symbols`（`symbols.jsonl` の行数）・`public_symbols`（`public` が真で `kind` が `module` でない行）・`edges`（`graph.jsonl` の行数。ファイルがあるときだけ）・`tests`・`benches`（`kind` 別の行数）・`errors`・`warnings`（`diag.jsonl` の `severity` 別。`note` は数えない）・`l2_cards`（書いた L2 の数）・`l2_omitted`（L2 の対象の行数 − `l2_cards`）。この順。失敗した解析では `errors`・`warnings` だけ |
| `modules` | 配列 | モジュール（利用者と依存パッケージ。`KbModel::modules()`）1 つが `{"name":ID,"key":key,"origin":"project"または"dependency:<name>","path":相対パス,"source":SHA-256}`。`name` の byte 順。失敗した解析では `[]`。`path` は `inputs[info.source].path`、`source` はその**ファイルの内容そのまま**の SHA-256。1 つのモジュールは 1 ファイル（2 つ目は `E1011`） |
| `files` | 対象 | `manifest.json` と、名前が `.` で始まるファイル（`.tsuzuri-kb`・`.tsuzuri-reuse.json`。機械ごと）を除く全ファイル（`/` 区切りの相対パス）→ その内容の SHA-256。byte 順 |

**書式**（固定。`manifest.json` は読み手が `serde_json` で読み、書き手は手で整える）: 2 スペースのインデント。`counts` は 1 キー 1 行、`modules` は 1 要素 1 行（要素は空白なしの 1 行）、`files` は 1 キー 1 行（`"path": "hex"`）。末尾は改行 1 つ。文字列は `diagnostic::json_string`。完全な例は付録 D-5。

**`source_hash`** = 次の `Sha256` の `finalize()`（`cache::Sha256::field` は「タグの長さ・タグ・値の長さ・値」を足す）:

```text
h.field("tz-kb-input", 1u32 の little endian)
入力ごとに（(パッケージ名, 相対パス) の byte 順。パッケージ名は `SourceFile::package` の `PackageId::name`、なければ ""）:
  h.field("package", パッケージ名); h.field("path", 相対パス); h.field(種類, 内容)
  種類 = ソース（`ModuleOrigin::User` の `.tz`・`.tt`・`.tc`）は "source"、`Project::manifests` の要素（`Tsuzuri.toml`・`Tsuzuri.lock`）は "manifest"
```

入力 = `Project::sources` のうち `origin == User` のもの（依存パッケージのソースを含む）と `Project::manifests`。std は入れない（コンパイラの版が決める）。スレッド数・`--deny-warnings`・`--warn` は入れない。

### 4. 鮮度（Phase 1。`check` が何も書かない経路）

`fresh(dir)`: マーカーを読む → `manifest.json` を `cache::read_regular(path, 16 MiB)` で読んで `serde_json` で解析する → `schema`・`compiler`・`source_hash`・`complete` を取る。**ファイルごとの SHA-256 は検査しない**（`kb verify` だけ）。

| 状態 | 自動の作成（`check`・`build`・`run`・`test`） | `kb verify` |
| --- | --- | --- |
| 置き場がない | 作る | `E2009` `kb_missing` |
| 置き場があるが有効なマーカーがない | **作らない**。通知 `kb not updated: '<dir>' exists and is not a KB …`（`E2009` `kb_not_ours`。全文は付録 I-1） | `kb_missing`（メッセージに「no .tsuzuri-kb marker」） |
| マーカーはあるが manifest が読めない・JSON でない・`schema` が `tz-kb/` で始まらない | 作り直す | `kb_broken` |
| `schema` が `tz-kb/<n>` で n≠1 | 作り直す | `kb_stale` |
| `compiler` が違う、または stamp が違う | 作り直す | `kb_stale` |
| `source_hash` が違う | 作り直す | `kb_stale` |
| すべて一致、`complete: false` | **何も書かない**（同じソースの診断が `diag.jsonl` にある） | `kb_incomplete` |
| すべて一致、`complete: true` | **何も書かない** | 全ファイルを検査して `ok`（欠落・SHA-256 の違いは `kb_broken`） |

### 5. 書き込みと置換（Phase 1）

`publish(output, files)`（`src/kb/write.rs`）の手順:

1. 親（`output` の親）を `create_dir_all`。`TemporaryDirectory::new(&parent)` で `.tsuzuri-<pid>-<n>` を作り、その中の `next/` を staging にする。
2. `files`（相対パス → byte 列）の親ディレクトリを重複なく作り、**並列に**書く（`Parallelism::command_line().workers(files.len(), 32)` 本の worker、1 本ならその場で。書く順は結果に影響しない）。1 つでも失敗したら最初の失敗（ファイルの byte 順）で中止し、staging を消す。
3. マーカー `.tsuzuri-kb` を最後に書く。
4. 置換を最大 3 回試す: `output` を調べる — (a) なし → (c)。(b) 実ディレクトリで有効なマーカーあり → `previous-<k>` へ `rename`（`NotFound` は (a) として扱う）。(c) それ以外（ファイル、シンボリックリンク、マーカーのないディレクトリ）→ 中止して `kb_not_ours`。次に `rename(next, output)` する。`AlreadyExists`・`DirectoryNotEmpty`・Windows の `PermissionDenied` は「別のプロセスが先に置いた」ので次の試行へ（**後勝ち**）。ほかの失敗は退避を戻して `E2001`。3 回とも失敗したら通知 `kb not updated: another process keeps replacing '<dir>'`。
5. 成功したら退避したものを消す（最上位の子ディレクトリごとに並列で `remove_dir_all`）。消せなくても通知しない（次の `TemporaryDirectory` の掃除に任せる）。

読み手が `output` の無い瞬間を見ても壊れない（「ない」として作り直す）。**失敗は呼び出し元で 1 回の通知になる**（§9）。置き場の親が作れない・読み取り専用なら `E2001` を通知にする。

### 6. 解析が失敗したとき（D2）

HEAD の解析は、失敗すると `CheckedModule` を返さない。そのため**失敗した解析の KB は最小**にする: マーカー、`manifest.json`（`complete: false`、`counts` は `errors`・`warnings` だけ、`modules` は `[]`、`files` は `diag.jsonl` だけ）、`diag.jsonl`（`DiagnosticSet` の全診断。省略の note を含む）。symbols・graph・facts・brief は**書かない**（前の KB のデータを残さない。古い事実を現在のように見せない）。`kb` の照会は `E2009` `kb_incomplete` を返す（G28）。

### 7. brief カード（Phase 2）

**共通の規則**:

- 英語（D-56）。ASCII のみ。例外は省略記号 `…`（U+2026）と、利用者の名前・署名・説明をそのまま写した部分。
- 長さは `str::chars().count()`（Unicode スカラー値。改行を含む）。上限は L0 2,400・L1 1,600・L2 700（D-52）。ファイルは末尾に改行を 1 つ。
- `norm(s)` = 空白（改行・タブを含む）の連続を空白 1 つにして前後を削る。`clip(s, n)` = `s` が n 文字以下ならそのまま、超えるなら先頭 n−1 文字と `…`。`plural(n, 単数, 複数)` = n が 1 なら単数形。
- 整列はすべて UTF-8 の byte 順。「宣言順」= `(span.start, ID)`。
- 署名・型は**書かれた文字列を写す**（`norm` と `clip` だけ）。型を作り直さない。
- 行が使う値が無い（graph・facts が KB に無い、空の配列）なら、その行を出さない。
- 名前が `$` で始まる行（`$entry`。トップレベルのコード）は、L0 の `symbols` の数・`entry:` の行・graph の辺の端（依存・callers・used by の相手）にだけ使い、モジュールの項目の一覧・L1 の項目・L2 のファイルには出さない。

**切り詰めの規則**（`Card`）: カードは「ブロック」の並びで、ブロックは「見出し（なしも可）」と「行（テキストと優先度）」。優先度 0 の行は落とさない。ブロックに行が 0 行なら、見出しも出さない。出力 = ブロックを空行 1 つ（一覧は改行 1 つ）でつなぎ、末尾に改行。落とした行があるブロックは、残った行の後ろに `…N more`（N＝そのブロックで落とした行数）を 1 行足す。

1. `spread` が真のブロックでは、位置 i（0 から）の行の優先度を `基本の優先度 + i / 5`（整数の割り算）にする。
2. 全体の長さが上限以下になるまで、落とせる行（優先度 > 0）のうち **(優先度, ブロックの位置, 行の位置) が最大**のものを 1 行ずつ落とす（長さは `…N more` の行を含めて毎回測り直す）。
3. 落とせる行がなくなっても超えるなら、先頭 `上限−1` 文字と `…` に切る（改行なし。優先度 0 の行は短く保証してあるので、通常は起きない。テスト `mandatory_lines_fit_every_limit`）。

**L0**（プロジェクト。上限 2,400）:

| ブロック | 行 | 優先度 | 並び |
| --- | --- | --- | --- |
| 見出しなし | `# {clip(project,64)}` と（あれば）空白 `{clip(version,32)}` | 0 | |
| | `tz-kb/1 \| edition {edition} \| modules {M} \| symbols {S} \| tests {T}`（benches があれば ` \| benches {B}`）。M＝`module` 行の数、S＝全行の数、T＝`test` 行の数 | 0 | |
| | `status: ok`（警告があれば `, {W} warning` または `, {W} warnings`） | 0 | |
| | `entry: {ID}`（`name` が `main` の `def` で、その module 行の `name` が `Main` のもの）。なければ、`name` が `$entry` の行があれば `entry: top-level code of {その行の module}`。それもなければ `entry: none (library)` | 0 | |
| `## Modules`（spread） | `- {ID} ({clip(path,80)}): {要約}` | 1 | (公開の最上位の行の数の降順, ID) |
| `## Exports`（spread） | `- {ID} :: {clip(署名の " :: " より後, 120)}`（`export` が真の `def`・`const`） | 2 | ID |
| `## Dependencies`（spread。graph があるとき） | `- {モジュール} -> {依存先を ", " でつなぐ}`（`clip(…,200)`）。依存 = graph の `calls`・`uses_type`・`implements`・`instance_of`・`imports` の行で、from と to が KB の行にあり、モジュールが違うもの | 3 | モジュールの ID |
| `## Tests`（spread） | `- {モジュールの ID}: {テストの数}` | 4 | (数の降順, ID) |
| `## Effects (functions)`（spread。facts があるとき） | `- {効果}: {n}`（n＝facts の `effects` にその効果を持つ `def`・`method` の数。0 は出さない） | 5 | 効果の名前 |
| 見出しなし | `next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl ('::' is '.' in file names)`（定数 `NEXT`。G28 が照会コマンドの案内に差し替える） | 0 | |

モジュールの**要約**: 公開（`public` が真）の最上位の行（`record`・`union`・`class`・`alias`・`handle`・`const`・`def`・`extern`・`instance`）を、この順の種類ごとに `{種類} {名前を宣言順に ", " でつなぐ}`（名前は `clip(name,32)`）にし、`; ` でつないで `clip(…,100)`。1 つもなければ `(no public items)`。

**L1**（モジュール 1 つ。上限 1,600）:

| ブロック | 行 | 優先度 | 並び |
| --- | --- | --- | --- |
| 見出しなし | `# {ID} ({clip(path,80)})` | 0 | |
| | `{plural(n,"item","items")}, {p} public \| {plural(t,"test","tests")}`。n＝最上位の行の数、p＝そのうち公開、t＝`test` の数 | 0 | |
| `## Types`（spread） | record・union・class は `- {種類} {名前} {{メンバー名を宣言順に ", "}}`、alias・handle は `- {clip(norm(署名),140)}`。非公開は末尾に ` [private]` | 公開 1・非公開 4 | (公開が先, 宣言順) |
| `## Functions`（spread） | `- {clip(norm(署名),140)}`（`const`・`def`・`extern`）+ 非公開なら ` [private]` + graph があり呼び出し元の関数が n≥1 なら ` \| callers {n}` + facts の `effects` があれば ` \| effects {a,b}`（`,` に空白なし） | 公開 1・非公開 4 | 同上 |
| `## Instances`（spread） | `- instance {name}` | 2 | 同上 |
| `## Tests`（spread） | `- {name}`（`bench` は `- bench {name}`） | 3 | 宣言順 |
| 見出しなし（graph があるとき） | `uses: {モジュールの ID を ", "}`、`used by: {…}`（各 `clip(…,200)`） | 3 | |

**L2**（symbol 1 つ。上限 700）。1 ブロック（見出しなし、spread なし）:

| 行 | 条件 | 優先度 |
| --- | --- | --- |
| `{ID}` | | 0 |
| `{種類} \| public または private[ export][ pure] \| {clip(path,80)}:{line}` | | 0 |
| `sig: {clip(norm(署名),120)}` | | 0 |
| `params: {名前:conv, …}`（名前がなければ conv だけ） | `def`・`extern`・`method`・`const` で `params` が空でない | 1 |
| `fields: {各 field の署名}`／`cases: {各 case の署名}`／`methods: {各 method の名前}`（宣言順。子 = ID が `親.` で始まり、その後に `.` を含まない行） | record／union／class で子がある | 1 |
| `doc: {説明の最初の空でない行を clip(norm,100)}` | 説明がある | 2 |
| `deprecated: {clip(norm,100)}` | `deprecated` が空でない | 1 |
| `effects: {a, b}` | facts の `effects` が空でない | 1 |
| `sites: trap {n}, alloc {n}, copy {n}`（0 は出さない。`proven: true` の site は数えない） | 1 つ以上ある | 2 |
| `recursion: {self\|self_tail\|mutual}` | `none` でない | 2 |
| `callees (n): …`・`callers (n): …`・`tests (n): …`（graph の `calls`／`tested_by`。ID の byte 順の先頭 5 件、残りは `, …{m} more`） | 空でない | 3 |
| `used by (n): …`（record・union・class・alias・handle。graph の `uses_type`・`implements`・`instance_of` の to がその ID の from） | 空でない | 3 |

**カードの置き場**: `brief/L0.md`、`brief/L1/<stem(key)>.md`（`modules` の全モジュール）、`brief/L2/<stem(ID)>.md`。**L2 のファイルは、種類が `def`・`const`・`record`・`union`・`class`・`method`・`alias`・`extern`・`handle` の行のうち 256 件まで**（`export`＝0、公開＝1、非公開＝2 の順位、次に ID の byte 順で先頭 256 件。`$` で始まる名前は数えない）。残りは `counts.l2_omitted` に数え、G28 の `kb brief ID` が同じ規則で都度生成する。実例（上限に収まることの手計算つき）は付録 B〜E。

**カードが使う行のキー**（G26 の「仕様」と同じ。違うなら止める: 停止条件 6）: symbols = `id`・`kind`・`module`・`name`・`signature`・`params`（`name`・`conv`）・`public`・`export`・`pure`・`doc`・`deprecated`・`span`（`path`・`start`・`line`）。graph = `rel`・`from`・`to`。facts = `id`・`effects`・`trap_sites`・`alloc_sites`・`copy_sites`（`proven`）・`recursion`。`field` 行の `signature` は `x: i32` の形、`instance` 行の `name` は `Add<Point>`、`module` 行の `module` はその ID と同じで `span` は `start: 0`・`line: 1`、`$entry` は `kind: "def"`・`public: false`。`doc`・`deprecated` は `null` もある。

### 8. `tsuzuri kb`（Phase 2）

```text
tsuzuri kb build  [PATH] [--kb-dir DIR] [--std] [--force] [--json]
tsuzuri kb verify [PATH] [--kb-dir DIR] [--json]
tsuzuri kb clean  [PATH] [--kb-dir DIR] [--std]
tsuzuri kb export [PATH] [--kb-dir DIR] (--symbols | --graph | --facts) [--no-refresh]
```

`PATH` は `check` と同じ入力（ディレクトリかファイル。既定 `.`）。オプションは PATH の前後どちらでもよく、`--` で終わる。**繰り返し・未知のオプション・PATH が 2 つ・サブコマンドがない**は `E2000`（終了コード 2。`unknown option 'X'; use --help`、`unknown kb command 'X'; use build, verify, clean, or export`、`usage: tsuzuri kb build [PATH] ...` の形）。G28 と G30 が `brief`・`show`・…・`diagram` を同じ `kb` の分岐に足す。`--kb`・`--no-kb` は受け付けない（`kb` は明示のコマンド。`TSUZURI_KB` も読まない）。

| コマンド | 動き | 標準出力（テキスト／`--json`） | 終了コード |
| --- | --- | --- | --- |
| `build` | `Project::load_for_tests(PATH)`（`Main.tz` 不要）→ §4 で新鮮なら何もしない（`--force` は新鮮でも作り直し、再利用もしない）→ そうでなければ `analyze_cached_captured(.., Tests, ..)` → 診断を `check` と同じに標準エラーへ → KB を作る。`--std` は std の KB（Phase 4。PATH を取らない） | `kb: wrote {n} files to {dir}`（n＝マーカーと manifest を含む数）／`kb: up to date ({dir})`／`kb: incomplete, {e} errors ({dir})`。`--json`: `{"type":"kb_build","status":"written"or"unchanged"or"incomplete","dir":…,"files":n,"complete":真偽}` | 0（written・unchanged）。1（エラーのあるプロジェクト、`E2001`・`E2009`）。2（使い方） |
| `verify` | プロジェクトを読み、§4 の `kb verify` の列のとおりに検査（全ファイルの SHA-256 を含む） | `kb: ok ({symbols} symbols, {files} files, source {source_hash の先頭 12 桁})`／`{"type":"kb_verify","status":"ok","source_hash":…,"symbols":n,"files":n}`。失敗は標準エラーに `E2009` | 0／1／2 |
| `clean` | 置き場にマーカーがあれば消す。なければ何もしない。マーカーのない既存のディレクトリは `E2009` `kb_not_ours`。`--std` は `<キャッシュ>/kb-std/` を消す | なし | 0／1／2 |
| `export` | 置き場を `open`（§「設計」。既定は新鮮でなければ作り直す。`--no-refresh` は作らず読むだけ）。`--symbols`＝`symbols.jsonl`、`--graph`＝`graph.jsonl`、`--facts`＝`facts/*.jsonl` を byte 順に連結して**そのまま**標準出力へ。ファイルがなければ（G26 の書き手が未実装）何も出さず 0 | JSON Lines のみ | 0。KB がない（`--no-refresh`）・不完全は `E2009`（1）。フラグが 0 個か 2 個以上は `E2000`（2: `kb export needs exactly one of --symbols, --graph, --facts`） |

`{dir}` は、解決した置き場を**与えられたとおり**に表示する（canonicalize しない）。

### 9. 通知と診断（`E2009`・`W2001`）

**自動の作成の失敗**（書けない、置き場が KB でない、置換に 3 回負けた、stamp が取れない）は、診断にもエラーにもせず、**1 回の通知**にする: テキストは標準エラーに `kb not updated: {理由}` の 1 行、`--json` は `{"severity":"warning","code":"W2001","message":"kb not updated: {理由}"}`（スパンなし）。出す位置は、解析の診断と警告の後、プログラムやテストの実行の前。終了コードと診断は変えない。明示の `kb` コマンドでは `E2009`・`E2001` を**エラー**として出す（終了コード 1）。

`E2009`（D-55。スパンなし、`related`・`fixes` なし）の kind は 5 つ: `kb_missing`（置き場がない。`verify`・照会）、`kb_stale`（古い）、`kb_broken`（manifest が不正、ファイルの欠落・不一致）、`kb_not_ours`（置き場が KB でない、またはソースを含む）、`kb_incomplete`（`complete: false`）。**メッセージの全文（固定）と、テキスト・`--json`・通知の例は付録 I-1。** kind は D-51 の 3 通り（G23 があれば S1 の手順で登録して `of`、G22 だけなら `with_kind`、どちらもなければ付けず、JSON に `kind` のキーを出さない）。`help` は G22 の後に、各メッセージの `;` 以降を命令形の 1 文にして足す。

### 10. 増分（Phase 3）

**再利用するもの**は、モジュール（key）ごとの `facts/<stem>.jsonl` と `layout/<stem>.jsonl` の**バイト列**と、その SHA-256（前の manifest の `files`）だけ。symbols・graph・tests・diag・カードは毎回作る（安く、他のモジュールに依存する）。省くのは、再利用するモジュールの `write_facts`・`write_layout` の呼び出しと、そのバイト列の SHA-256 の計算だけで、**G26 の `KbModel::build` の中の facts・layout の計算は省かない**（G26 の作りが eager で、鍵に呼び出しグラフ全体が要るため）。したがって節約は小さい。**着手の条件**: 手順 11 の最初の計測で、`kb-facts`（`emit` の `write_facts`・`write_layout` と、その SHA-256）が KB の生成時間（`kb-model`＋`kb-write`＋`kb-brief`＋`kb-publish`）の 10% 以上を占めること（占めなければ着手せず、停止条件 8 の B として記録する）。

**配管**: `Project::analyze_cached_captured` の本体（`analyze_cached_core`）が `FrontendCache` を閉じる前に各モジュールの `(source, interface)` を `ModuleDigest { name, origin, source: [u8; 32], interface: [u8; 32] }` にして返すようにし、`analyze_cached_digested(&self, cache, kind, capture) -> (Result<CheckedModule, DiagnosticSet>, Option<Vec<ModuleDigest>>)` を足す（`FrontendCache::digests` が `finish` の後の `ModuleRecord` から作る）。キャッシュが無効・開けないなら `None`（再利用しない）。`FRONTEND_FORMAT` は変えない。

**鍵**（`origin` が std でない M）: `K(M)` = SHA-256( `field("tz-kb-facts", 1u32 の little endian)`、`field("stamp", stamp)`、`field("edition", edition)`、`field("source", M.source)`、`field("world", world)`、`fwd(M)` の D を name の byte 順に `field("callee", D.name)`・`field("callee-source", D.source)`、M がジェネリックなら `rev(M)` の C を name の byte 順に `field("caller", C.name)`・`field("caller-source", C.source)` )。`world` = 全 user モジュールを name の byte 順に `field("module", name)`・`field("interface", interface)` した SHA-256。`fwd(M)`＝M から呼び出し・instance 経由で届くモジュール、`rev(M)`＝M に届くモジュール（M がジェネリックのときだけ）。**効果は呼び先の本文で決まる**ので、interface だけの鍵では足りない（D8）。`fwd`・`rev` の定義、無効化の表（何を変えるとどのモジュールを作り直すか）、根拠は付録 G-3。

**`.tsuzuri-reuse.json`**（機械ごと）: `{"format":1,"stamp":"<64 桁>","keys":{"<モジュールの key>":"<K(M)>",…}}`（`keys` は key の byte 順、末尾に改行）。**再利用**は、前の置き場に有効なマーカー・`complete: true` の manifest・同じ stamp の `.tsuzuri-reuse.json` があり、M の鍵が同じで、前のファイルが `files` の SHA-256 と一致するときだけ（そのバイト列をそのまま使う）。`--force`・`digests` が `None`・`kb_broken`／`kb_stale` の置き場では再利用しない。差分テスト: 任意の編集列の後で、増分の KB と `kb build --force` の KB が `.` で始まらない全ファイルで 1 byte 違わない（手順 12）。

### 11. std の KB（Phase 4）

- **置き場**: `cache::default_root()` の下の `kb-std/<id>/`（`default_root()` が `None` なら置かない）。`<id>` は stamp の先頭 16 桁。中身はプロジェクトの KB と同じ形。manifest の `project` は `"std"`、行の `origin` は `"std"`（G26 の語彙への追加）、ID は G26 の付録 B1 のとおり名前空間なし（モジュール `Array`、関数 `Array.map`）。
- **作り方**: `Project::std_library()`（新規。`stdlib::SOURCES` の各ファイルを `ModuleOrigin::Std`・`namespace: ""`・`edition: Edition::LATEST`・`package: None` の `SourceFile` にし、`manifests` は空、`root: 0`）を `analyze_inputs_core` で解析する。G26 の `Capture` に `pub(crate) include_std: bool`（既定 `false`）を足し、`collect_abstract` が `ModuleOrigin::Std` のモジュールを飛ばす条件を `!capture.include_std &&` にする（std の `CapturedModule.origin` は `"std"`）。`KbModel::build` の結果を、プロジェクトと同じ `emit`・`publish` で書く（`brief/` は作らない。カードは G28 の `brief ID` が都度生成する）。
- **いつ**: `kb build --std`（`--force` で作り直し）か、`KbView` が std を必要としたとき（初回だけ）。有効な `<id>/` があれば読むだけ。作った後、`kb-std/` の下で更新が新しい 3 世代を残して消す。`--no-refresh` では作らず、std の ID は引けない。
- **`KbView`**: `KbView::new(project: Option<Arc<KbData>>, std: StdSource)`（`Arc` は G29 が `KbData` を共有するため）。メソッドは `symbol(&self, id)`・`edges_to`・`edges_from`・`project(&self) -> Option<&KbData>`・`std(&self) -> Option<&KbData>`。**Phase 2 は `StdSource::None` だけ**（プロジェクトの行だけを引く。G28 の Phase 1 と G29 の KB 系ツールが使う）。**Phase 4 が足す**: `StdSource::Cache`（上のとおり。キャッシュが使えなければメモリ上に作って書かない）、`StdSource::Memory`（常にメモリ上に作り、**ディスクへ書かない**。MCP のような読み取り専用の読み手が使う。最初に std の ID が要るとき 1 回だけ作り、`KbView` の中で持つ）。プロジェクトを先に、std を後に引く。ID は衝突しない（std の ID に名前空間がなく、利用者のモジュールは std の名前を使えない: `E1011`）。

### 12. 上限と互換性・edition

- 上限: `manifest.json` を読む最大 16 MiB（`cache::read_regular`）、マーカー 4,096 byte、L2 のファイル 256 件、置換の試行 3 回、再利用するモジュール 4,096 まで、std の KB の世代 3、`diag.jsonl` は `DiagnosticSet` の全件。新しい解析ではないので `E1017` は使わない。
- `docs/stability.md` の「安定として保つもの」に KB のキー（`tz-kb/1` と D-52 の各行のキー）を足す（足すだけ。消す・改名は番号を上げる。カードの文言と `counts` の追加は対象外。G26 が先に足していれば重複させない）。
- IR・ABI・WASM・診断の意味・edition は変えない。KB は読むだけで、受理するプログラムを変えない（D-45・D-54）。新しい診断コードは D-55 に割り当て済みの `E2009` だけ。新しい予約語・crate・`unsafe`・WASM の import はない。

## 設計

### データ構造と関数の骨格（名前・型を変えない。型の全フィールドは付録 H）

```rust
// src/kb/settings.rs（新規）
pub struct KbFlags { pub kb: bool, pub no_kb: bool, pub dir: Option<PathBuf> }   // Arguments.kb
pub enum KbMode { Default, On, Off }
pub struct KbSettings { pub mode: KbMode, pub dir: Option<PathBuf> }
pub fn resolve(flags: &KbFlags, kb_env: Option<&OsStr>, dir_env: Option<&OsStr>) -> Result<KbSettings, Diagnostic>; // E2000
impl KbSettings { pub fn wants(&self, project: &Project) -> bool; pub fn directory(&self, project: &Project) -> PathBuf }
// src/kb/input.rs（新規）
pub const KB_FORMAT: u32 = 1;
pub fn source_hash(project: &Project) -> [u8; 32];
pub fn stamp() -> Option<[u8; 32]>;                        // frontend_cache::compiler_identity を使う
// src/kb/manifest.rs（新規）
pub struct Manifest { /* §3 のキー */ }  impl Manifest { pub fn render(&self) -> String; pub fn parse(text: &str) -> Result<Manifest, String> }
pub enum Freshness { Missing, NotOurs, Broken(String), Stale(&'static str), Incomplete, Fresh }
pub fn inspect(dir: &Path, source_hash: &str, stamp: &[u8; 32]) -> Freshness;
// src/kb/write.rs（新規）
pub struct Files(pub BTreeMap<String, Vec<u8>>);
pub fn publish(output: &Path, files: &Files, stamp: &[u8; 32]) -> Result<usize, Diagnostic>; // 書いた数（マーカーと manifest を含む）
pub fn stem(id: &str) -> String;  pub fn unique_names(ids: &[String]) -> BTreeMap<String, String>;
pub fn has_marker(dir: &Path) -> bool;  pub fn remove_tree(path: &Path);
// src/kb/emit.rs（新規。G26 との接点はここだけ）
pub fn emit(model: &KbModel) -> std::io::Result<Files>;    // symbols.jsonl・graph.jsonl・tests.jsonl・facts/<stem>.jsonl・layout/<stem>.jsonl（G26 の出荷した Phase の分）
// src/kb/build.rs（新規）
pub struct Target { pub dir: PathBuf, pub hash: String, pub stamp: [u8; 32] }
pub enum Plan { Off, Fresh, Build(Target) }                // 解析の前に決める（Fresh は何も書かず、キャプチャなしで解析する）
pub fn plan(settings: &KbSettings, project: &Project, force: bool) -> Result<Plan, Diagnostic>;
pub enum Analysis<'a> { Checked { module: &'a CheckedModule, capture: Capture, warnings: &'a DiagnosticSet }, Failed { errors: &'a DiagnosticSet } }
pub enum Outcome { Written { files: usize }, Unchanged, Incomplete { files: usize } }
pub fn finish(target: &Target, project: &Project, analysis: Analysis<'_>) -> Result<Outcome, Diagnostic>;
pub fn notice(error: &Diagnostic) -> String;               // "kb not updated: …"
// Phase 2: src/kb/store.rs  KbData { meta, symbols, graph: Option<..>, facts: Option<..>, diag } と Symbol・Edge・Fact・Meta
//   KbData::open(dir), KbData::from_parts(..), KbData::symbol(id)。KbView::new(Some(Arc<KbData>), StdSource::None)（Phase 4 が StdSource::{Cache, Memory} を足す）
// Phase 2: src/kb/brief.rs  Card（block(header, lines, spread)・render）、l0(data, limit)・l1(data, module_id, limit)・l2(data, id, limit)・l2_file_ids(data)
// Phase 2: src/kb/open.rs   pub enum Refresh { Auto, Never }  pub fn open(path, kb_dir, refresh, json) -> Result<KbData, Diagnostic>
```

### アルゴリズム

**`plan` と `finish`**（`src/kb/build.rs`）:

1. `plan`: `dir` を決める（`KbSettings::directory`）。`protect(dir, project)`: `dir` が存在し、マーカーがない、または `dir` が `Project::sources`・`manifests` のどれかの祖先なら `Err(E2009 kb_not_ours)`。`hash = hex(input::source_hash(project))`、`stamp = input::stamp()`（`None` なら `Err`: 通知 `cannot identify the running compiler`）。`!force` かつ `manifest::inspect(dir, &hash, &stamp)` が `Fresh` か `Incomplete` なら `Plan::Fresh`、そうでなければ `Plan::Build`。`settings.wants(project)` が偽なら `Plan::Off`。
2. `main` は `Plan::Build` のときだけ `Project::analyze_cached_captured(cache, kind, &mut capture)`（`capture` は `Capture::default()`）で解析し、それ以外は `analyze_cached`。**解析は 1 回**で、`check` が 2 倍遅くなる `kb::collect(&Project)` は使わない。
3. `finish`: `Checked` は `model = KbModel::build(&project.inputs(), module, capture)`（G26）→ `emit::emit(&model)` → `diag.jsonl` = `warnings` の各診断を `json_with_severity(severity, 相対パス, ソース)` で 1 行（`W1006` は入れない）+ `omission_note`。Phase 2: `brief::l0`・`l1`・`l2` を `Files` に足す。`Failed`: `diag.jsonl` と manifest（`complete: false`）だけ。
4. manifest: `files` を名前が `.` で始まらない全ファイルの SHA-256 で作り、`modules` を `model.modules()` と `project.inputs()` から作り（`origin` は `ModuleInfo.origin`）、`Files` に `manifest.json` を足す。
5. `write::publish(dir, &files, &stamp)`。結果を `Outcome` にする。時間は `timings::Phase::start("kb-model")`（`KbModel::build`）・`"kb-write"`（`emit` と `diag`。facts・layout の `write_*` は入れ子の `"kb-facts"`）・`"kb-brief"`・`"kb-publish"` で測る（`TSUZURI_TIME_PASSES=1` に出る。手順 11・13 が使う）。

**`Card::render`** と**各カードの生成**は §7 の規則（`spread`・優先度・`…N more`）。実装は付録 F の Python の参照実装と同じ手順にし、`Vec<(String, u8)>` の行と `BTreeSet<(usize, usize)>`（落とした行）で持つ。落とす行は毎回 `max_by_key((優先度, ブロック, 行))`。行数は多くても数百なので 2 乗の計算量で足りる。

**`open`**（`src/kb/open.rs`）: `Refresh::Never` → マーカー・manifest を読み、`complete: false` なら `kb_incomplete`、なければ `KbData::open`。`Refresh::Auto` → `Project::load_for_tests(path)` → `plan`。`Plan::Build` なら `analyze_cached_captured(.., Tests, ..)` の後 `finish` で書き（失敗した解析なら診断を標準エラーに出して `kb_incomplete` を返す）→ `KbData::open`。`KbData::from_parts(meta, &Files)` は、ディスクを介さずに同じ読み取りを `emit` の結果から作る（G29 のような書かない読み手のため。`open` はこれを使わない）。

### 段ごとの変更表

| ファイル | 関数・型 | 変更 |
| --- | --- | --- |
| `src/kb/mod.rs`（G26） | `mod` 宣言 | Phase 1: `settings`・`input`・`manifest`・`write`・`emit`・`build`。Phase 2: `store`・`brief`・`open`・`command`。Phase 3: `incremental`。Phase 4: `std_kb` |
| `src/main.rs` | `Arguments` | `kb: kb::settings::KbFlags` を足す。2 つの `Arguments { .. }` を直す |
| | `parse_arguments` | `--kb`・`--no-kb`・`--kb-dir` の分岐（重複と値の検査）と、検証（`doc`・`fmt`・`bench`・`lsp` では拒否。`--kb` と `--no-kb` の併用）。`Action::Check`・`Build`・`Run`・`Test` だけ許す |
| | `main` | `parse_arguments` の後に `kb::settings::resolve`（`E2000` は終了コード 2。ソースを読む前）。プロジェクトを読んだ後・解析の前に `kb::build::plan`、解析の結果の 2 つの分岐（`Err(errors)`・`Ok(module)`）で `finish` を呼ぶ（失敗の通知は §9 の形）。`Ok` の分岐では**警告の出力の直後、`--deny-warnings` の判定の前**。`kb` サブコマンドの分岐（`raw.first() == "kb"`）を `api-diff` の隣に足す（Phase 2） |
| | `HELP` | `kb`・`--kb`・`--no-kb`・`--kb-dir`・`TSUZURI_KB`・`TSUZURI_KB_DIR` の行。テスト `the_help_lists_every_target_of_the_table` の隣に `the_help_lists_the_kb_options` |
| `src/driver.rs` | `collect_sources` | ディレクトリを `pending` に積む前に、`path.join(".tsuzuri-kb")` が通常ファイルなら積まない |
| | `Project::root_directory` | `pub(crate)` にする |
| | `Project::analyze_cached` | 本体を `analyze_cached_core(cache, kind, capture: Option<&mut Capture>)` に移し、`analyze_cached` は `None`、新しい `analyze_cached_captured(cache, kind, &mut Capture)` は `Some` で呼ぶ（`analyze_inputs_with` の代わりに G26 の `analyze_inputs_core`）。結果と診断は同じ |
| | Phase 3: `analyze_cached_digested`・`ModuleDigest` | §10 |
| | Phase 4: `Project::std_library` | §11 |
| `src/lib.rs` | `analyze_inputs_core` | G26 が非公開にしていれば `pub(crate)` にする |
| `src/kb/model.rs`（G26） | `KbModel::modules` | `pub fn modules(&self) -> Vec<ModuleInfo>`（`ModuleInfo { id, key, origin, source: usize }`。`Capture.modules` の写し。`source` は `inputs` の添字）。Phase 4: `Capture.include_std`。Phase 4: `Capture.include_std` |
| `src/frontend_cache.rs` | Phase 3: `FrontendCache::digests` | `finish` の後に各モジュールの `(name, origin, source, interface)` を返す |
| `src/package.rs` | `create_project` | `.gitignore` の内容を `.tsuzuri/\ntarget/\n` にする。テスト `creates_projects_that_declare_their_namespace` に内容の確認を足す |
| `scripts/check-docs.mjs` | `invoke` | `execFileSync` に `env: { ...process.env, TSUZURI_KB: '0' }` を渡す（文書の例ごとに一時ディレクトリへ KB を作らない） |

## 前提とする他チケットのインターフェース

| 相手 | 使うもの | 未実装・違うときの代替 |
| --- | --- | --- |
| G26 Phase 1 | `KbModel::build(inputs: &[SourceInput], module: &CheckedModule, capture: Capture) -> KbModel`（`pub(crate)`）、`analyze_inputs_core(inputs, semantic, cache, parallelism, capture)`、`check::kb_capture::Capture`、`write_symbols(&self, &mut impl Write) -> io::Result<()>`、`symbols` の行（キーは §7 の末尾）。`module`・`test`・`bench`・`$entry` の行 | 引数が違えば呼び出しだけ直す（意味は変えない）。無ければ停止条件 1 |
| G26 Phase 2 | `write_graph`・`write_tests` | 無ければ `graph.jsonl`・`tests.jsonl` とカードの graph の行を出さない |
| G26 Phase 3〜6 | `fact_modules()`・`layout_modules()`（key の昇順）、`write_facts(&self, module: &str, out)`・`write_layout`、`fact_keys()` | 無ければ `facts/`・`layout/`、効果・site の行を出さない |
| G26 に足すもの（G27 の手順） | `KbModel::modules()`（Phase 1）、`Capture.include_std`（Phase 4）。`KbCounts` は使わない（G26 が項目を定めていない）。manifest の `counts` は書いたバイト列の行数から作る | |
| G22／G23 | `Diagnostic::with_kind`、kind の登録簿に `kb_*` の 5 つ | G23 があれば S1 の手順で登録して `of` で出す。G22 だけなら `with_kind` で付ける。どちらもなければ kind を付けない（D-51） |
| G28・G29 | `store.rs` の `KbData::open`・`from_parts`・`symbol`、`KbView`（Phase 2 は `StdSource::None`。`Memory` は Phase 4）、`open.rs`、`brief.rs` の `Card`、`manifest.rs` の `inspect`、`input.rs` の `source_hash`・`stamp`、`E2009` の kind | G27 Phase 2 が先（std の ID の引きは Phase 4 から）。名前は G29 の「前提とする他チケットのインターフェース」と同じ |

G26 の各 Phase の手順の最後で、`emit.rs` に足す行（付録 H）: Phase 2＝`graph.jsonl`・`tests.jsonl`、Phase 3〜5＝`facts/<stem(key)>.jsonl`、Phase 6＝`layout/<stem(key)>.jsonl`。

## 実装手順

### 手順 1: ベースライン（Phase 1）

- 変更: なし（`tests/fixtures/kb_shop/` を付録 D のソースで作る。`kb_shop_head` は G28 が作る）。
- 内容: GUIDE §2.3 の基準。付録 A の計測を `target/tz-g27/` で再実行して「現状」の表と同程度か確かめる。`examples/*` と `tests/fixtures/*` の `check` の標準出力・標準エラー・終了コードを `target/tz-g27/before/` へ保存する。`kb_shop` を `check` して警告が 1 件、`test --list` が 3 件であることを確かめる。
- 確認: `cargo build --release --locked && cargo test --locked` の件数を控える。

### 手順 2: テスト先行（Phase 1）

- 変更: `tests/kb_build.rs`（新規）。
- 内容: テスト計画の Phase 1 の表のうち、手順 3〜6 で緑になるものを書く。最初は `check_creates_the_kb_and_a_second_check_writes_nothing`。
- 確認: 新しいテストだけが失敗し、既存の件数と結果は変わらない。

### 手順 3: フラグと設定（Phase 1）

- 内容: `src/kb/settings.rs`・`src/main.rs`（`Arguments`・`parse_arguments`・`HELP`・`mod tests`）。§1 のとおり。`parse_arguments` が `--kb`・`--no-kb`・`--kb-dir` を `KbFlags` に読み、重複・値・対象のコマンドを検査する。`resolve` は `main` で `parse_arguments` の直後に呼ぶ（環境変数の `E2000` は終了コード 2。`invalid_thread_setting` の隣）。単体テスト `parses_kb_options`・`the_help_lists_the_kb_options` を足し、`rejects_ambiguous_or_unused_arguments` に拒否の行を足す。
- 確認: `cargo test --locked kb`・`cargo test --locked main::tests`（`running N tests` の N を確認）。既存の結果は変わらない。

### 手順 4: ソース探索と `.gitignore`（Phase 1）

- 内容: `src/driver.rs`（`collect_sources`・`Project::root_directory`）、`src/package.rs`（`create_project`）。`collect_sources` は、`path.join(".tsuzuri-kb")` が通常ファイルのディレクトリを `pending` に積まない。`root_directory` を `pub(crate)` にする。`.gitignore` を `.tsuzuri/\ntarget/\n` にし、テスト `creates_projects_that_declare_their_namespace` で内容を確かめる。
- 確認: `kb_dir_is_skipped_by_source_discovery`。付録 A-3 の 20,000 ファイルの置き場に `.tsuzuri-kb` を置くと `check` が約 36 ms に戻る。`cargo test --locked` の件数が変わらない。

### 手順 5: 入力・manifest・書き込み（Phase 1）

- 内容: `src/kb/input.rs`・`manifest.rs`・`write.rs`（新規）。§2〜§5。単体テスト: `source_hash_is_independent_of_the_order_of_the_sources`、`manifest_renders_the_appendix_example`（付録 D-5 と 1 byte 一致）、`publish_replaces_an_old_kb_and_refuses_a_foreign_directory`、`publish_survives_two_writers`（スレッド 2 本）、`stem_encodes_unsafe_characters`、`unique_names_separate_names_that_differ_only_in_case`。
- 確認: `cargo test --locked kb::`。

### 手順 6: `emit`・`build` と `main` への組み込み（Phase 1）

- 内容: `src/kb/emit.rs`・`build.rs`（新規）、`src/main.rs`（`main`）、`src/driver.rs`（`analyze_cached_core`・`analyze_cached_captured`）、`src/lib.rs`（`analyze_inputs_core` の可視性）、G26 の `src/kb/model.rs`（`KbModel::modules`）。§「設計」の `plan`・`finish` の手順。`main` は、プロジェクトを読んだ後に `plan` を呼び、`Plan::Build` のときだけ `analyze_cached_captured` で解析する。`Err(errors)` の分岐で `print_diagnostics` の後に `Analysis::Failed`、`Ok(module)` の分岐で警告の出力の後・`--deny-warnings` の判定の前に `Analysis::Checked` を `finish` へ渡す。失敗の通知は §9 の形で、`run`・`test` では実行の前。`emit.rs` は G26 Phase 1 の `write_symbols` だけを呼ぶ。`timings::Phase` の区間 4 つを足す。
- 確認: 手順 2 のテストが緑。`target/tz-g27/before/` と比べて、`examples/*`・`tests/fixtures/*` の標準出力・標準エラー・終了コードが 1 byte も変わらない。付録 A-1 の `check` を `--no-kb` と比べて記録する（2 倍を超えたら停止条件 1）。`cargo test --locked`。

### 手順 7: 足場と文書（Phase 1）

- 変更: `scripts/check-docs.mjs`、「ドキュメント」の Phase 1 の項目。
- 確認: `node scripts/check-docs.mjs <変更したページ>`、`cargo test --locked`、`node tests/parallel_frontend.mjs target/release/tsuzuri`・`tests/frontend_cache.mjs`・`tests/cache.mjs`、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check --all-targets --target x86_64-pc-windows-msvc`。

### 手順 8: 読み手とカード（Phase 2）

- 内容: `src/kb/store.rs`・`brief.rs`（新規）、`tests/kb_brief.rs`、`tests/golden/kb/{point,polymorphism}/brief/`・`tests/golden/kb/shop/`。`store.rs` は行を型にして読む（知らないキーは無視、無いキーは既定値）。`KbView`（§11。Phase 2 は `StdSource::None` だけ）もここに置く。`brief.rs` は §7 の規則どおりに `Card` と `l0`・`l1`・`l2`・`l2_file_ids` を書く（付録 F の参照実装と同じ手順）。golden の入力は G26 の `tests/golden/kb/point/{symbols,graph}.jsonl`（グラフあり）・`polymorphism/symbols.jsonl`（symbols だけの KB）と、`shop/` の全ファイル（付録 D）。期待は付録 B〜D のカードをそのまま写す。
- 確認: `cargo test --locked --test kb_brief`（`cards_match_the_golden_files`、`card_fit_drops_the_lowest_priority_first`、`mandatory_lines_fit_every_limit`、`l0_fits_for_1000_modules`、`worked_truncation_example`＝付録 E の文字列と一致）。

### 手順 9: `build` にカードを足す・`kb` コマンド・`open`（Phase 2）

- 内容: `src/kb/build.rs`・`command.rs`・`open.rs`（新規）、`src/main.rs`（`raw.first() == "kb"` の分岐を `api-diff` の隣に、`HELP`）。`finish` が `brief/L0.md`・`L1/`・`L2/`（256 件）を足し、`counts` を埋める。`command.rs` は §8 の 4 コマンド。`open.rs` は `Refresh::Auto`・`Never`。
- 確認: `kb_build_verify_clean_export_round_trip`、`verify_reports_each_e2009_kind`、`export_needs_exactly_one_flag`、`l2_files_are_capped_at_256`、`check_and_kb_build_write_the_same_kb`。

### 手順 10: 文書（Phase 2）

- 変更: 「ドキュメント」の Phase 2 の項目。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/kb.md …`。

### 手順 11: 配管と鍵（Phase 3）

- 内容: `src/frontend_cache.rs`（`FrontendCache::digests`）、`src/driver.rs`（`ModuleDigest`・`analyze_cached_digested`）、`src/kb/incremental.rs`（新規）、`build.rs`・`emit.rs`。 **最初に**付録 G-1 の計測で §10 の着手の条件（`kb-facts` が KB の生成時間の 10% 以上）を確かめ、満たさなければここで止める（結果を手順 16 に記録）。満たせば §10 のとおり。`fwd`・`rev` は幅優先、`K(M)` は `Sha256::field`。`main`・`kb build`・`open` が `analyze_cached_digested` を使う。
- 確認: `cargo test --locked`（`frontend_cache` の既存のテストが変わらない）、`incremental` の単体テスト `keys_follow_the_invalidation_table`（無効化の表の各行を、小さな手書きの graph で確かめる）。

### 手順 12: 再利用と差分テスト（Phase 3）

- 変更: `src/kb/build.rs`・`write.rs`（`.tsuzuri-reuse.json`）、`tests/kb_incremental.rs`（新規）。
- 内容: §10 の再利用の条件で `facts/`・`layout/` のバイト列と SHA-256 を使う。鍵は `KbModel::build` の後に新しい graph で作る。`kb build --json` に `reused`（再利用したモジュールの数）を足す。
- 確認: `incremental_equals_force_after_each_edit`（編集列: 葉のコメント、呼び先の本文に `IO.write_line` を足す、署名を変える、モジュールを足す・消す、instance を足す、ジェネリック関数の本文を変える。各編集の後で `.` で始まらない全ファイルが `kb build --force` と 1 byte 同じ）、`unchanged_modules_are_reused`（`reused` の数が無効化の表のとおり）、`reuse_is_off_without_the_frontend_cache`（`TSUZURI_CACHE_DIR=` で `reused: 0`）。

### 手順 13: 計測（Phase 3）

- 変更: なし（付録 G の手順で測る）。
- 内容: 小さいプロジェクト・200・1,000 モジュールで、`--no-kb`・`kb build --force`・増分（1 モジュールの本文を編集）の `check` と `kb build` の時間を各 9 回。結果は手順 16 で `docs/benchmarks.md` に記録する。
- 確認: 停止条件 8（増分が KB の生成時間の 10% も縮めない）の判断を記録する。

### 手順 14: std の KB と `KbView`（Phase 4）

- 変更: `src/driver.rs`（`Project::std_library`）、`src/kb/std_kb.rs`（新規）、`src/kb/store.rs`（`KbView` に `StdSource::{Cache, Memory}` を足す）、`src/kb/emit.rs`・`open.rs`、G26 の `src/kb_capture.rs`（`Capture.include_std`）。
- 内容: §11 のとおり。`StdSource::{Cache, Memory}` を足し、`KbView::new(project, std)` が std の KB を持てるようにする（`symbol`・`edges_to`・`edges_from`・`std` は std の部分も引く）。
- 確認: `tests/kb_std.rs`（`build_std_creates_the_cache_directory`、`std_kb_is_reused_and_old_generations_are_removed`、`kb_view_prefers_the_project_and_falls_back_to_std`、`std_kb_without_a_cache_is_built_in_memory`、`memory_mode_writes_nothing`）。

### 手順 15: `--std` と文書（Phase 4）

- 変更: `src/kb/command.rs`（`build --std`・`clean --std`）、文書。確認: `kb build --std` が 2 回目に何もしない。`node scripts/check-docs.mjs`。

### 手順 16: 記録（Phase 5）

- 変更: `docs/benchmarks.md`（新しい節 `## KB の生成と増分（G27）`: 測定の条件・表・再現コマンド。閾値なし）、`_features/GUIDE.md`（D-55 の `kb` の行に `--force`・`--std`・`--no-refresh`、`E2009` の kind）、このチケットの末尾の「実装と検証」。
- 確認: GUIDE §13.2 の完了報告。

## テスト計画

期待値は付録の手書きの行・カード（Python の参照実装 F が出したもの）と、テスト内の独立した計算（`sha256` は別実装の値、ファイルの byte 比較）から作る。CLI のテストは `env!("CARGO_BIN_EXE_tsuzuri")` を呼び、プロジェクトは 1 つずつ別の一時ルートに置く（手本 `tests/api_diff.rs` の `root`・`tsuzuri`）。拒否のテストは診断コードを照合する。

**`tests/kb_build.rs`（Phase 1。各テストの入力と期待は付録 I-2）**: `check_creates_the_kb_and_a_second_check_writes_nothing`（最初に書く）、`editing_a_source_rebuilds_the_kb`、`no_kb_env_and_loose_files`、`kb_flag_conflicts_and_bad_env_are_e2000`、`kb_dir_flag_and_env_choose_the_directory`、`a_foreign_directory_is_never_overwritten`、`an_unwritable_target_warns_once`、`a_failed_analysis_writes_an_incomplete_kb`、`the_kb_is_identical_in_two_directories_and_thread_counts`、`concurrent_checks_leave_a_valid_kb`、`the_output_of_commands_does_not_depend_on_the_kb`、`kb_dir_is_skipped_by_source_discovery`、`test_run_and_build_create_the_kb`。

**Phase 2**（`tests/kb_brief.rs` と `tests/kb_build.rs`）: `cards_match_the_golden_files`（付録 B〜D）、`card_fit_drops_the_lowest_priority_first`（付録 E の 330・280 字）、`mandatory_lines_fit_every_limit`（名前 64 字・ID 300 字の最悪の入力でも切り詰めに入らない）、`l0_fits_for_1000_modules`（付録 E の 2,394 字）、`worked_truncation_example`（付録 E）、`l2_files_are_capped_at_256`（300 関数 → `l2_cards: 256`・`l2_omitted`）、`kb_build_verify_clean_export_round_trip`、`verify_reports_each_e2009_kind`（5 つの kind とメッセージ）、`export_needs_exactly_one_flag`、`check_and_kb_build_write_the_same_kb`（`.` で始まらない全ファイルが同じ）、`kb_build_from_a_loose_file`。**Phase 3**（`tests/kb_incremental.rs`）: 手順 12 の 3 つ。**Phase 4**（`tests/kb_std.rs`）: 手順 14 の 5 つ。

**既存テストへの影響**: 期待値が変わるものは**ない**。根拠: (1) `check`・`build`・`run`・`test` の標準出力・標準エラー・終了コード・IR は KB の有無で変わらない（`the_output_of_commands_does_not_depend_on_the_kb`）。(2) `create_project` のテストは件数 3 を見るだけで、内容の確認を足すだけ。(3) `src/main.rs` の引数のテストは新しいオプションの行を足すだけ。(4) `tests/parallel_frontend.mjs` は `examples/hello` の標準エラーが空であることを検査するが、KB の書き込みは何も出さない（全 example・fixture の `check` で `target/tz/kb/` ができるが、`.gitignore` の `target` に一致して追跡されない）。

**性能**: 速度の合否は置かない。`check` の KB の有無の差と KB の生成時間を手順 13・16 で記録する（`docs/benchmarks.md`）。

## ドキュメント

**Phase 1**: `_tsuzuri/language-reference/compiler/usage.md`（`check`・`build`・`run`・`test` の節に `--kb`・`--no-kb`・`--kb-dir`、`### new` の `.gitignore`）、`compiler/option.md`（「どのサブコマンドが受け付けるか」の表に 3 行、環境変数の表に `TSUZURI_KB`・`TSUZURI_KB_DIR`、組み合わせの表）、`compiler/diagnostics.md`（`E2009` の行と kind、`W2001` の KB の例）、`organizing-tsuzuri/packages.md`・`vsc/README.md`（`.gitignore` の中身）、`README.md`（CLI 一覧と「KB」の短い節）、`docs/language.md`（CLI の節）、`docs/architecture.md`（`src/kb/*.rs` の行と「KB の置換と鮮度」）、`docs/stability.md`（KB のキーの約束）。**Phase 2**: 新しいページ `compiler/kb.md`（置き場・ファイル・manifest・鮮度・カード・`kb` の 4 コマンド。例は付録 B〜D）と `index.md`、`usage.md` の `### kb`、`README.md`。**Phase 3・4**: `kb.md` の増分と std の節、`architecture.md`。**Phase 5**: `docs/benchmarks.md`、`GUIDE.md` の台帳。検証: `node scripts/check-docs.mjs <変更したページ>`（`kb.md` の出力例は ` ```text ` で検査されない。`tsuzuri` の例は通る形にする）。

## 受け入れ条件

**Phase 1**

- [ ] `check`・`build`・`run`・`test` がルートに `target/tz/kb/` を作り、2 回目は何も書かない（更新時刻と inode）。§1 の決定表と `E2000` のとおり。
- [ ] 置き場が KB でない・書けない・置換に負けた場合は通知 1 回で、終了コードと診断が変わらない。失敗した解析は `complete: false` と `diag.jsonl` だけ。
- [ ] `.` で始まらない全ファイルが、別のルート・スレッド数で同じ byte。8 プロセスの並行でも壊れず、一時ディレクトリが残らない。
- [ ] `collect_sources` が KB を読まず、`tsuzuri new` の `.gitignore` に `target/` がある。KB の有無で出力・終了コード・IR が変わらない。
- [ ] `cargo test --locked`・`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・Windows の `cargo check`・Phase 1 の文書と `check-docs.mjs`。

**Phase 2**

- [ ] 付録 B〜D のカードが golden と 1 byte 一致し、1,000 モジュールの L0 も 2,400 字以内。L2 のファイルは 256 件まで。
- [ ] `kb` の 4 コマンドの出力と終了コードが §8 のとおり。`E2009` の 5 つの kind。`kb.md`・`index.md`・`usage.md`・`README.md`。

**Phase 3**: 差分テストで増分と `--force` が一致し、無効化の表の各行が単体テストにある。着手の条件と停止条件 8 の判断を記録。**Phase 4**: `kb-std/<id>/` ができ 2 回目は作らない。世代は 3 つ。`StdSource::Memory` は何も書かない。`KbView` はプロジェクトが先。**Phase 5**: `docs/benchmarks.md` の節、台帳（D-55 の `kb` の行・kind）。

## 落とし穴

- **絶対パス・stamp・日時の漏れ**: `grep -rn "$PWD" target/tz/kb` と `the_kb_is_identical_in_two_directories_and_thread_counts`。
- **`check` の出力が KB で変わる**（通知を標準出力へ出す）: `the_output_of_commands_does_not_depend_on_the_kb`。
- **文字数を `len()`（byte。`…` は 3 byte）で測る**: 必ず `chars().count()`。`mandatory_lines_fit_every_limit`。
- **`HashMap` の反復順がファイルに出る**: `BTreeMap` だけを使い、2 回作って byte 比較。
- **失敗した解析を毎回作り直す**: 2 回目の `check` の前後で更新時刻が同じこと。
- **`Main.tz` のない入力に `check` を使うと `E2001`**: `kb build` は `load_for_tests`。
- **大文字小文字だけが違う名前でファイルが衝突する（macOS・Windows）**: `unique_names_separate_names_that_differ_only_in_case`。
- **Windows の `rename` は既存の置き場に失敗する**: 退避してから rename。`cargo check --target x86_64-pc-windows-msvc`。

## 対象外

照会と `impact`（G28）、MCP（G29）、図表と `views/*.mmd`（G30）、`tsuzuri find`（G25）。部分的な解析結果の KB。ファイル単位の差分更新。LLM の文章。std をプロジェクトの KB に入れること。`script`・`repl` の自動 KB。KB の圧縮・パック化。

## 決定事項

すべて承認済み（D-50）。

- **D1 KB はキャッシュで、ディレクトリ単位で置換する。** D-52 の契約。却下: ファイルごとの in-place 更新（読み手が新旧の混在を見る。D-52 に反する）、hard link での再利用（作成より遅い: 0.9 ms/個）。`tsuzuri doc` の置換は共通化しない（`Project` と `.tsuzuri-docs` に固定）。
- **D2 失敗した解析は最小の KB（`complete: false`、`diag.jsonl` だけ。G26 の D15 と同じ）。** HEAD の解析は失敗すると結果を返さず、古い symbols を残すと現在のように見える。
- **D3 鮮度は `frontend_cache::compiler_identity` に KB の版を足した stamp で見る。** ビルドキャッシュの鍵（実行ファイル全体の SHA-256。50 ms）は `check` の 39 ms より重いので使わない。stamp は機械ごとなので `.tsuzuri-kb` に置く。却下: `CARGO_PKG_VERSION` だけ（開発中のビルドを見分けられない）。
- **D4 ソース探索は `.tsuzuri-kb` のあるディレクトリを読まず、`tsuzuri new` の `.gitignore` に `target/` を足す。**
- **D5 ファイル数を抑える**（L2 は 256 件、行のないモジュール別ファイルは書かない、並列に書く）。却下: L2 を全件（数万件、数秒）、モジュールごとのパック（D-52 のファイル名に反する）。
- **D6 `source_hash` は入力（ソース・マニフェスト）だけから作る。** `--warn`・`--deny-warnings`・スレッド数は入れない。
- **D7 設定の優先: フラグ → 環境変数 → 既定（プロジェクトなら作る）。** `--kb-dir` は `--kb` を含意しない。`script`・`repl` は対象外（`kb build FILE` が代わり）。
- **D8 増分の鍵は、M の本文・全モジュールの interface・呼び先の本文（`fwd`）・ジェネリックの呼び出し側（`rev`）。** 効果が呼び先の本文で決まるので、種の「interface が同じなら再利用」では足りない。G26 の `build` が eager に計算するので、省けるのは書き出しと SHA-256 だけで節約は小さい。着手の条件（§10）で測り、満たさなければ出荷しない。
- **D9 std の KB はキャッシュの下に世代つきで 1 つ。** なければメモリ上（`StdSource::Memory` は書かない）。却下: プロジェクトの KB に std を入れる（D-52 に反する）。
- **D10 `check-docs.mjs` だけ `TSUZURI_KB=0`。** 却下: `.cargo/config.toml` で全体を切る（KB のテストが書けなくなる）。
- **D11 `kb build` は `load_for_tests`**（`Main.tz` 不要）。`export` はフラグ 1 個を必須にする（スキーマの混在を避ける）。
- **D12 `E2009` は 5 つの kind。自動の作成の失敗は診断にせず `W2001` の通知。** D-52 の「終了コードと診断を変えない」。
- **D13 カードは `chars().count()` で測り、切り詰めは優先度と `spread`（5 行ごとに 1 下げる）。** 却下: ブロックだけの優先度（大きなプロジェクトで Exports・Dependencies・Tests が全部 `…N more` になる。付録 E）。
- **D14 `W1006`（`--warn implicit-copy`）は `diag.jsonl` に入れない。** 同じ情報は facts の `copy_sites` にあり、入れると `source_hash` にフラグが要る。
- **D15 ファイル名の幹は、facts・layout・L1 がモジュールの key、L2 が ID。** G26 が facts・layout を key と定めた。key と ID の対応は manifest の `modules` に持つ。

## 付録

[G27-appendix.md](G27-appendix.md): A 実測の再現、B `polymorphism`（symbols だけの KB）の全カード、C `point`（symbols と graph）の全カード、D `kb_shop` の全ファイル、E 切り詰めの手計算、F カード規則・ファイル名・`source_hash` の Python の参照実装、G 計測の手順と増分の鍵（`fwd`・`rev`・無効化の表）、H Rust の型の全フィールドと `emit.rs` の足し方、I `E2009` のメッセージと各テストの入力・期待。

