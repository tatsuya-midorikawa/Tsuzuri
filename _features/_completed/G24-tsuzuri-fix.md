# G24: `tsuzuri fix`（機械的な修正の適用と edition の移行基盤）

| 項目 | 内容 |
| --- | --- |
| ID | G24 |
| 優先度 | P1 |
| 規模 | 全体 L（Phase 1: M、Phase 2: S、Phase 3: M、Phase 4: S の合計。D-50 の表の M は Phase 1〜2 の規模）。Phase ごとに別の PR にする |
| 依存 | G22 の Phase 1（診断 v2 のデータモデル: `Fix`・`Edit`・`Applicability`・`Diagnostic.fixes`・`Diagnostic::fix`・JSON の `fixes`）。Phase 2 の LSP の整理は G22 Phase 4 の有無で分岐（手順 11）。Phase 3 の本番の配線は A22（`Edition::E2027`・`W1007`・`E0004`）が足すまで `E2000` で止まる（A22 が本チケットに依存する）。弱い依存: G23（kind の登録簿。G23 があれば本チケットの kind を S1 の手順で登録簿へ足して `of` で出し、なければ `with_kind` で直接付ける。D-51） |
| 後続 | A22（`--edition` と `fix::canonical_warnings` を使う）、G25 Phase 4（`E1002` の `maybe` の fix を `--applicability maybe --dry-run` で表示）、A20・G23（fix を足す診断）、G29（`tz_fix_plan` が `fix::run` を書き込みなしで呼ぶ）、G21（ガイドの作業の流れ） |
| 状態 | done（Phase 1〜4、2026-10-27） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc` で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D16 はすべて承認済み |
| 改善する劣位 | Rust 比（`cargo fix`）: 警告や edition の移行を機械的に直す手段がない。診断が `fixes`（G22）を持っても、適用するには編集の再実装が要る。AI の作業の流れ（`check --json` → 直す → `test`）の「直す」が欠けている（D-50 の結論 (1)） |
| 手本にする既存実装 | 書き込みの保護と原子的な置換: `src/driver.rs` の `format_sources`・`require_regular_source`・`replace_source_atomically`・`TemporaryDirectory`・`read_source_text`。入口の流儀（独自の引数解析・`E2000`・終了コード 2）: `src/main.rs` の `api_diff_command`・`api_diff_usage`、`src/bindgen.rs` の `parse_arguments`。読み込みと解析: `Project::load`・`Project::analyze_cached`・`Project::source_for`・`Project::single_main`（`src/driver.rs`）、`src/main.rs` の `main` の check 経路と `print_diagnostics`。編集後の再解析検査: `src/lsp.rs` の `verify_edits`・`Project::load_with_overlays`。マニフェスト: `src/package.rs` の `parse_manifest`。テスト: `tests/api_diff.rs`（`root`・`Command`）、`tests/lsp.rs` の `code_action_prefixes_unused_locals`、`tests/editions.rs` |
| 主な影響ファイル | Phase 1: 新規 `src/diff.rs`・`src/fix.rs`・`tests/fix.rs`。変更 `src/lib.rs`・`src/main.rs`・`src/driver.rs`。Phase 2: `src/warnings.rs`・`src/check.rs`（呼び出し 1 か所）。Phase 3: `src/package.rs`・`src/fix.rs`。Phase 4: `src/lsp.rs`・`tests/lsp.rs`。文書は「ドキュメント」 |

## 実装者向け早見

- **作るもの**: `tsuzuri fix`。診断の `fixes`（G22）を集め、重ならないものを順に適用し、再検査して悪化がなければ書く。`--dry-run` は差分を出す。Phase 3 は `--edition`（移行）、Phase 4 は LSP の `source.fixAll.tsuzuri`。
- **最初に読む**: 「仕様」→「設計」→「実装手順」。コードは `src/driver.rs` の `format_sources`〜`replace_source_atomically`、`src/main.rs` の `api_diff_command`、`src/lsp.rs` の `verify_edits`。G22 のチケットの `Fix`・`Edit` の節。
- **最初に書くテスト**: `src/diff.rs` の `matches_the_lcs_length_and_round_trips`（手順 2）、次に `src/fix.rs` の `conflict_is_closed_interval_intersection`（手順 3）。
- **触ってはいけない**: 型検査・所有権検査・IR の生成。`tsuzuri fmt`（`replace_source_atomically` を含む）の出力と終了コード。`Diagnostic` の既存 5 キー。`Edition::LATEST`・`MANIFEST_DEFAULT`。
- **停止の目印**: G22 Phase 1 が main にない（`grep -n "pub fixes" src/diagnostic.rs` が 0 件）。既存テストの期待値を変えないと通らない。検証の規則が `certain` の通常の fix を拒否する。
- **Phase 1 の完了**: `tsuzuri fix <dir>` が（fix を出す診断がなければ）`no fixes to apply` を出して終了コード 0。偽の解析器で `fix::run` のラウンド・重なり・検証・書き込み・巻き戻しが通る。
- **Phase 2 の完了**: 未使用ローカル（`W1001`）が `tsuzuri fix` で `_` 付きに直り、2 回目は何も変えない。`--applicability likely` で `W1005` の置換が入る。
- **Phase 3 の完了**: 移行の流れ（canonical の fix → 目標 edition で再検査 → ソース → マニフェスト）が偽の解析器のテストで通る。`--edition 2030` は `E2000`。
- **Phase 4 の完了**: `textDocument/codeAction` が `source.fixAll.tsuzuri` を返し、`tests/lsp.rs` の新しい 2 件が通る。
- **新しい名前**: 診断 `E2008`（kind 4 個）、`E1017` の kind `fix_candidate_limit`、`E2000` の kind 5 個、サブコマンド `fix`、フラグ `--applicability`・`--dry-run`・`--edition`（D-55）。環境変数・新しい crate・予約語はない。

## 目的

診断（G22）が `fixes` を持っても、適用する道具がなければ、利用者も AI も編集を手で再現することになる。`tsuzuri fix` は次を満たす。

1. `certain`（意味を保つ機械的な書き換え。D-51）を既定で適用し、`likely` は `--applicability likely` のときだけ、`maybe` は提案として `--dry-run` で見せるだけにする。
2. 適用する前に、書き換え後のプログラムをメモリ上で解析し直し、診断が悪化しないことを確かめる。確かめられなければ何も書かない（全か無）。
3. 書き込みは `tsuzuri fmt` と同じ保護（通常のファイルだけ・原子的な置換・読んだ後に変わっていないこと）に、複数ファイルの失敗の巻き戻しを足す。
4. `--dry-run` は差分を、`--json` は適用した fix と捨てた fix（理由つき）を 1 行 1 オブジェクトで出す。
5. edition の移行の枠組み（`--edition`）を持つ。A22 が `W1007` の fix と 2027 の拒否を足す。

HEAD で測った数字: `Diagnostic::new`／`Diagnostic::warning` の構築箇所は 483（`grep -rn "Diagnostic::new\|Diagnostic::warning" src --include=*.rs | wc -l`）。`src/diagnostic.rs` に `fixes`・`Fix`・`Applicability` はない（`grep -c "fixes\|struct Fix\|Applicability" src/diagnostic.rs` が 0）。機械的に直せるのは警告 6 種（`W1001`〜`W1006`）のうち `W1001` と `W1005` だけ（付録 A）。LSP の quickfix は `W1001` の 1 種類で、`rename` と再解析で意味を確かめている。`tsuzuri fix a` は今 `E2000`。

**非目標**: コードの整形（`fmt`）。fix の内容を作ること（各診断を出すチケットの仕事。本チケットは適用の仕組みと安全性）。`maybe` の自動適用。依存・std・`Tsuzuri.lock` の書き換え、`Tsuzuri.toml` の `edition` 以外の書き換え。対話的な選択・VCS の確認・テストの実行。

## 着手条件と停止条件

### 着手条件

- G22 の Phase 1 がマージ済み: `grep -n "pub fixes" src/diagnostic.rs` と `grep -n "pub enum Applicability" src/diagnostic.rs` と `grep -n "pub fn fix" src/diagnostic.rs` が各 1 件。
- GUIDE §2.3 の基準（`cargo build --release --locked`・`cargo test --locked`）が通る。件数を控える（手順 1）。
- 読むもの: GUIDE の D-51・D-54・D-55、`docs/stability.md`、`src/driver.rs` の `format_sources`〜`replace_source_atomically`、`src/main.rs` の `api_diff_command`。承認は不要（新しい crate・`unsafe`・予約語・既定の WASM の import はない）。

### 停止条件

止めて、状況と選択肢を報告する（GUIDE §13）。

1. G22 Phase 1 が main にない。選択肢: A（推奨）G22 Phase 1 を先に実装する。B 本チケットで D-51 の型だけを足す（G22 と衝突するので、人間が許したときだけ）。
2. `fmt` の出力・終了コード・エラーメッセージが 1 byte でも変わる（手順 5 は `replace_source_atomically` に触れない設計）。
3. 検証（V1〜V3）が `certain` の通常の fix を拒否する偽陽性になった。選択肢: A 規則に例外を足す（人間の承認）。B その fix を出す側が `likely` に下げる。規則を黙って緩めない。
4. Phase 2 で `W1001`・`W1005` に `fixes` を付けると、既存テストの期待値を変えないと通らない（「既存テストへの影響」にあるものを除く）。
5. `Project::load`（CLI）と `Project::load_with_overlays`（LSP）で、同じソースへの `fix::run` の結果が違う fixture が見つかった（LSP は std を全部読む）。選択肢: A LSP の `fixAll` をその入力で出さない。B LSP も使う std だけを読む経路を足す。
6. `src/diff.rs` が、全行が違う 2,000 行のファイルで 64 MiB より多く使う。選択肢: `MAX_DIFF_CHANGES` を 1,000 に下げる（文書も直す）。
7. A22 が先に実装済みで、`W1007` を出す入口が `fix::canonical_warnings(&Project, &CheckedModule) -> Vec<Diagnostic>` の形と合わない。選択肢: A 本チケットの署名を A22 に合わせる。B A22 の入口を包む。
8. 新しい crate・`unsafe`・既定の WASM の import・予約語が必要になった。同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

作業ディレクトリ `$WORK`（例: `/tmp/tz-G24`）に下の入力を作り、`tsuzuri` は `target/release/tsuzuri`。診断は標準エラーに出る（`--json` も標準エラー）。

**1. `fix` は未知のサブコマンド**（入力のパスと読まれる）:

```text
$ tsuzuri fix a
<command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively
```

**2. 警告と JSON**。`a/Main.tz`（`unused` が未使用）: `def helper :: i64 -> i64 = \n -> n + 1` / 空行 / `let unused = 5` / `let x = 40` / `x + 2`。

```text
$ tsuzuri check a --json
{"severity":"warning","code":"W1001","message":"unused local 'unused'; prefix it with '_' to silence this warning","path":"a/Main.tz","span":{"start":44,"end":50,"line":3,"column":5,"end_line":3,"end_column":11}}
```

終了コード 0。**マニフェストのあるプロジェクトの `path` は絶対パス**（`fs::canonicalize` した根から作る。実測: `$WORK/w/app/Main.tz`）で、ないプロジェクトは渡した入力からの相対。本チケットの出力は、どちらでも根からの相対で `/` 区切りのパス（`SourceFile::relative_path`）を使う（決定的にするため）。

**3. エラーがあると警告は出ない**。`b/Main.tz` が `let unused = 5` / `let y: i64 = true` / `let z = missing_name` / `42` のとき、`check b --json` は `E1003` と `E1002`（`unknown value 'missing_name'`）の 2 行だけで、`W1001` はない（終了コード 1）。エラーが直ると初めて見える警告は、次のラウンドで集める。

**4. 直す範囲を持たない診断**: `W1003`（到達しない腕）の位置はパターンだけ（`| 0 -> 2` は `0`）。`E1019`（`rec` の欠落）は最初の 1 件だけを関数名の位置に出す（`src/recursion.rs` の `check_graph`）。余分な `rec`（`def rec twice` が再帰しない）は受理される。

**5. `W1005`**。path 依存 `helper`（`Legacy.tz` の `old` の説明が `/// @deprecated use Helper::Legacy.next instead`）を使う `w/app/Main.tz`（`using Helper` の後に `let a = Legacy.old 1`）:

```text
$ tsuzuri check w/app
$WORK/w/app/Main.tz:3:16: warning[W1005]: 'Helper::Legacy.old' is deprecated: use Helper::Legacy.next instead
  3 | let a = Legacy.old 1
    |                ^
```

主スパンは `old`（メンバー名だけ）。`SemanticIndex` の `Definition` は `kind`・`module`・`name`・`parameters`・`result` を持つ（`src/semantic.rs`）ので、置換先が同じ型の関数かを調べられる。

**6. 書き込みの保護**（`tsuzuri fmt`）。入力の直下だけを数え（再帰しない）、通常のファイル以外を拒否し、読んだ後に変わっていれば書かない。`s/Link.tz` が `Real.tz` へのシンボリックリンクのとき `tsuzuri fmt s --check` は `E2003`（`fmt refuses symlinks, directories, and special files`）。`check` はシンボリックリンクのソースを `E1011` で読まない。ディレクトリに `Main.tz` がないと `check lib` は `E2001`、ファイルを渡すと通る（`check lib/Util.tz` は 0）。

**7. frontend cache** はソースの内容をキーにする（`src/frontend_cache.rs` の冒頭）。書き換えても古い構文木は使われない。`fix` は途中の状態を残さないよう、cache を使わず解析する（`analyze_cached(false, ..)`）。

**8. LSP**。`W1001` の診断に `Prefix 'spare' with '_'`（`kind: quickfix`、`isPreferred: true`）を返す（`tests/lsp.rs::code_action_prefixes_unused_locals`）。`initialize` は `{"codeActionKinds": ["quickfix"]}`（`tests/lsp.rs` が値を固定）。

**9. マニフェスト**は `src/package.rs` の `parse_manifest` が読む行指向の TOML の部分集合。`[package]` の `edition = "2026"` を `Edition::parse` が読む。`Edition(pub(crate) u16)`、`SUPPORTED = [2026]`、`LATEST` と `MANIFEST_DEFAULT` は 2026。

## 変えてはいけない意味

- `check`・`build`・`run`・`test`・`bench`・`doc`・`fmt`・`api-diff`・`repl`・`script` の出力・終了コード・生成物は、Phase 1・3・4 で 1 byte も変わらない。Phase 2 は `W1001` と `W1005` の診断に `fixes` が付くので、その 2 種の JSON に `fixes` が、人間向けの表示に `  fix (...)` の行（D-51）が増える。ほかの診断は変わらない。
- 生成 IR・ABI・WASM の import。`fmt` のメッセージ・終了コード。診断の既存 5 キーと `E1001`〜`E2007`・`W1001`〜`W2002` の意味。`Edition::LATEST`・`MANIFEST_DEFAULT`・`SUPPORTED`（A22 の仕事）。
- `fix` を使わない限り、どのファイルも書き換えない。`--dry-run` はどのファイルも作らず・変えない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 差分（`src/diff.rs`）、適用エンジン（`src/fix.rs`）、書き込み（`src/driver.rs`）、`tsuzuri fix`（`--dry-run`・`--applicability`・`--json`） | M | G22 Phase 1 | 新しいサブコマンドだけを足す。fix を出す診断がなければ `no fixes to apply` で何もしない |
| 2 | `W1001`（`certain`）・`W1005`（`likely`）の `fixes`。LSP の `W1001` の整理 | S | Phase 1 | `check` の診断に `fixes` が付くだけ。`fix` が実際に直せるようになる |
| 3 | `--edition`（移行の枠組み）、`package::set_edition`、`fix::canonical_warnings`（A22 が差し替える点） | M | Phase 1 | `Edition::parse` が受理する値（今は 2026 だけ）で完結する。A22 の値はその側の変更で有効になる |
| 4 | LSP の `source.fixAll.tsuzuri` | S | Phase 1（Phase 2 で中身が増える） | `codeAction` に 1 種類の action を足すだけ |

## 仕様

### CLI

```text
tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--edition YEAR] [--json] source.tz|source.tt|source.tc|directory
```

入力は `check` と同じ（ディレクトリは `Main.tz` が要る。ファイルは親が根で、**書き換えるのはそのファイルだけ**）。解析はプロジェクト全体。Phase 1 は `--edition` を知らない（未知のオプションで `E2000`）。

| フラグ | 値 | 既定 | 意味 |
| --- | --- | --- | --- |
| `--applicability` | `certain`／`likely`／`maybe` | `certain` | 適用する fix の下限。`likely` は `certain` と `likely`。`maybe` は提案を含めて表示するだけで、**`--dry-run` と一緒のときだけ受理**し、書かない（D1） |
| `--dry-run` | なし | 無効 | 書かず、統一差分を標準出力へ出す |
| `--edition` | 年 | なし | edition の移行（Phase 3）。`--applicability` と併用しない |
| `--json` | なし | 無効 | 標準出力を 1 行 1 オブジェクトの JSON にする。診断は標準エラーに JSON で出る |
| `--` | なし | なし | 以降をパスとして読む |

引数の誤りは `E2000`、終了コード 2（`--json` が引数にあれば誤りも JSON。`api-diff` と同じ）。`TSUZURI_THREADS` の誤りは `check` と同じく、ソースを読む前に `E2000`（2）。

| 入力 | メッセージ（`E2000`） | kind |
| --- | --- | --- |
| パスが 0 個・2 個以上 | `fix takes one .tz, .tt, or .tc file or project directory; usage: tsuzuri fix [--applicability certain\|likely\|maybe] [--dry-run] [--edition YEAR] [--json] PATH`（Phase 1・2 は `[--edition YEAR]` を除く） | `fix_bad_arguments` |
| 未知のオプション（`--` より前の `-` で始まる語） | `unknown option '{option}' for fix; supported: --applicability, --dry-run, --edition, --json` | `fix_bad_arguments` |
| `--applicability` の値がない・不正 | `--applicability needs one of certain, likely, or maybe` | `fix_bad_arguments` |
| `--edition` の値がない | `--edition needs a year such as 2027` | `fix_bad_arguments` |
| 同じフラグが 2 回 | `{flag} specified more than once` | `fix_bad_arguments` |
| `--applicability maybe` で `--dry-run` なし | `--applicability maybe only shows suggestions; add --dry-run to review them, because maybe fixes are never applied` | `fix_maybe_needs_dry_run` |
| `--edition` と `--applicability` | `--edition applies only certain fixes; remove --applicability` | `fix_edition_with_applicability` |
| `--edition` の値が `Edition::parse` で `None` | `unsupported edition '{value}'; supported editions: {Edition::supported_names()}` | `fix_edition_unsupported` |
| `--edition` の値が入力の edition より小さい（読み込み後） | `cannot migrate from edition {current} to {target}; --edition moves to a later edition` | `fix_edition_downgrade` |

**終了コード**: 0 = 完了（適用した、または適用するものがない。診断が残っていてもよい）。1 = 中止（`E2008`・`E1017`）、入力を読めない・解析できない（`check` と同じ `E2001`・`E1011`・`E0003`）、edition の移行が残りの誤りで止まった。2 = 引数の誤り。

### 書いてよいファイル

fix の各 edit は、span の `source`（`None` なら `project.root`）を `s` として、次の行を**上から順に、全 edit について**調べる。最初に当てはまった行の理由で fix 全体を捨てる（`Skipped(reason)`）。

| 行 | 条件 | 理由 |
| --- | --- | --- |
| 1 | `s >= project.sources.len()`（マニフェスト・lock・範囲外の索引） | `outside_project` |
| 2 | `project.sources[s].origin != ModuleOrigin::User`（std） | `outside_project` |
| 3 | `project.dependency_package(&project.sources[s]).is_some()`（path・git・registry の依存） | `outside_project` |
| 4 | `Scope::File(f)` で `s != f` | `outside_input` |
| 5 | `start > end`、`end > text.len()`、始点か終点が UTF-8 の境界でない | `invalid_edit` |
| 6 | edit の `text` が 4,096 bytes を超える、または適用後のファイルが `MAX_SOURCE_BYTES`（4 MiB）を超える | `invalid_edit` |
| 7 | 同じ fix の edit が `diagnostic::apply_edits` で適用できない（`(start, end)` で安定整列して `前.end > 後.start` なら重なり。接する区間と同じ位置の挿入は重ならない。別の fix どうしの交わりは下の D3 の閉区間） | `invalid_edit` |
| 8 | fix の `edits` が空 | `invalid_edit` |

書き込み時の保護は `fmt` と同じ: 通常のファイルだけ（シンボリックリンク・ディレクトリ・特殊ファイルは `E2008`）、読んだときの内容と同じときだけ置換する。ソースの拡張子は読み込みの段階で `.tz`／`.tt`／`.tc` に限られる。

### 診断の収集

`fix::analyze_project(&Project, canonical: bool) -> Analysis` は `check` と同じ経路で解析する（frontend cache を使わない）。`Project::analyze_cached(false, AnalysisKind::Program)` が `Ok(module)` なら `errors` は空の `DiagnosticSet`、`warnings = module.warnings`（checker が `(source, start)` で整列済み）。`Err(set)` なら `errors = set`（最大 `MAX_REPORTED_ERRORS` = 50 件と `omitted`）、`warnings` は空（「現状」3）。`canonical` が真のときだけ `canonical_warnings(project, &module)` の結果を `warnings` に足す（Phase 3）。`--warn implicit-copy` は受け取らない（`W1006` に fix がない）。

### 候補の選択

`collect(project, analysis, options)` は、`analysis.errors.diagnostics`、`analysis.warnings` の順に、`fixes` が空でない診断ごとに次を行う。

1. `options.only_codes` が `Some(list)` で `d.code` が `list` にないなら無視する（報告もしない）。
2. `d.fixes`（出す側が好ましい順に並べる）の先頭から、`options.threshold.admits(fix.applicability)` を満たす**最初の 1 つ**を選ぶ。残りは代案で使わない。1 つもなければ `d.fixes[0]` を `Skipped(Applicability)` として記録する。
3. 選んだ fix を「書いてよいファイル」で調べ、通らなければ `Skipped(reason)`、通れば候補にする。
4. 候補を整列キー（最初の edit の `(source, start, end)`、`code`、`title`、edits の `(source, start, end, text)` の列）で整列し、編集が完全に同じ候補は 1 つにまとめる。
5. 整列順に、採用済みの edit の区間と**閉区間で交わる**候補をこのラウンドでは捨てる（記録しない。診断が残れば次のラウンドで現れる）。閉区間なので、接する区間・同じ位置の挿入どうしも交わる。
6. 採用した候補が 20,000 件を超えたら `E1017`（`fix_candidate_limit`）。

`threshold.admits`: `Certain` は `Certain` だけ、`Likely` は `Certain`・`Likely`、`Maybe` は 3 つすべて。

### ラウンドと検証

1. `current = analyze(project)`、`rounds = 0`。
2. `collect` する。採用した候補がなければ終了（`stop = FixedPoint`）。
3. `rounds == MAX_ROUNDS`（5）なら、採用した候補を `Skipped(RoundLimit)` にして終了（`stop = RoundLimit`）。
4. 採用した edits をファイルごとに前から適用して `project.sources[s].text` に入れる。元のテキストは、そのファイルを最初に変えるときに 1 度だけ保存する。
5. `next = analyze(project)`。下表の**検証**に失敗したら、保存した元のテキストをすべて戻し、`E2008`（`fix_verification_failed`）で中止する。
6. 適用した fix を `Applied { round: rounds + 1 }` で記録し、`rounds += 1`、`current = next` として 2 へ。

| 検証 | 条件（`total = diagnostics.len() + omitted`） | 違反 |
| --- | --- | --- |
| V1 | `after.errors.total <= before.errors.total`。`before` に誤りがなければ `after` にもない | `E2008` |
| V2 | `before.errors.omitted == 0` のとき、`after` の誤りの `(path, code)` ごとの件数が `before` 以下 | `E2008` |
| V3 | `before.errors.total == 0` のとき、`after.warnings` の `(path, code)` ごとの件数が `before.warnings` 以下 | `E2008` |

`path` は `SourceFile::relative_path`。エラーがあると警告は出ないので、エラーを直したラウンドで警告が現れても V3 は調べない。誤りが 50 件を超えるとき（`omitted != 0`）は V2 の件数比較をしない（先頭 50 件の入れ替わりの誤検出を避ける）。最後の `collect` で採用されなかったもの（`applicability`・`outside_project`・`outside_input`・`invalid_edit`・`round_limit`）が `Skipped` の記録になる。途中のラウンドの交わりは記録しない（一時的）。

### 出力

人間向け（標準出力。`--json` なし）。ない行は出さない。複数形は `1 fix`／`2 fixes`、`1 file`／`2 files`。

| 順 | 条件 | 行 |
| --- | --- | --- |
| 1 | 書く・ファイルごと（source の索引順） | `fixed {path}: {n} fix(es)`（`{n}` はそのファイルに edit を持つ適用済みの fix の数） |
| 1' | `--dry-run`・ファイルごと | その差分（`diff::unified(path, original, fixed, 3)`。`None` なら `{path}: diff omitted (more than 2000 changed lines)`） |
| 2 | 適用が 0 件 | `no fixes to apply` |
| 3 | 適用あり | `applied {n} fix(es) in {m} file(s)`（`--dry-run` は `would apply ...`） |
| 4 | `Skipped` あり | `skipped {k} fix(es) (use --json for the list)` |
| 5 | `Skipped(Applicability)` の `Likely` が `j` 件（`threshold == Certain`） | `note: rerun with --applicability likely to apply {j} more fix(es)` |
| 6 | `stop == RoundLimit` | `note: stopped after 5 rounds; run tsuzuri fix again to apply the remaining {n} fix(es)` |

標準エラー: `--dry-run` でないときだけ、最後の解析の診断（誤り→警告）を `check` と同じ形式で出す（`print_diagnostics`。書いたテキストを引用する）。

`--json`（標準出力）は 1 行 1 オブジェクト。`event` の順は `fix`（適用済み→捨てた）、`file`、`manifest`（Phase 3）、`summary`（必ず最後）。

| `event` | キー（この順。値がないキーは出さない） |
| --- | --- |
| `fix` | `event`、`status`（`applied`／`skipped`）、`reason`（`skipped` のみ: `applicability`／`outside_project`／`outside_input`／`invalid_edit`／`round_limit`）、`round`（`applied` のみ）、`code`、`kind`（診断が持つときだけ）、`applicability`（`certain`／`likely`／`maybe`）、`title`、`edits`（D-51 の `{"path","span","text"}` の配列。`span` は診断と同じ 6 キー。**そのラウンドの開始時のテキスト**の位置。範囲外の索引は `"path":"<invalid>"` と行・列 0） |
| `file` | `event`、`path`、`fixes`、`diff`（`--dry-run` のときだけ。文字列、大きすぎれば `null`） |
| `manifest`（Phase 3） | `event`、`path`（`Tsuzuri.toml`）、`edition`、`status`（`updated`／`would_update`／`unchanged`／`none`／`blocked`）、`diff`（`--dry-run` で `would_update` のとき） |
| `summary` | `event`、`dry_run`、`applicability`、`rounds`、`stop`（`fixed_point`／`round_limit`）、`applied`、`skipped`、`files`、`remaining_errors`、`remaining_warnings`、`edition`（`--edition` のときだけ） |

`path` は根からの相対で `/` 区切り。文字列は `diagnostic::json_string` と同じエスケープ。`remaining_*` は最後の解析の件数（`--dry-run` は書かなかった場合の状態）。中止（`E2008`）のときは標準出力に何も出さない。

**例 1**（想定出力。Phase 2 の後。「現状」2 の `a`）:

```text
$ tsuzuri fix a
fixed Main.tz: 1 fix
applied 1 fix in 1 file
$ tsuzuri fix a
no fixes to apply
```

**例 2**（想定出力。`--dry-run`。空の文脈行は空白 1 文字で始まる）:

```text
$ tsuzuri fix a --dry-run
--- a/Main.tz
+++ b/Main.tz
@@ -1,5 +1,5 @@
 def helper :: i64 -> i64 = \n -> n + 1
 
-let unused = 5
+let _unused = 5
 let x = 40
 x + 2
would apply 1 fix in 1 file
```

**例 3**（想定出力。`--json`）:

```text
$ tsuzuri fix a --json
{"event":"fix","status":"applied","round":1,"code":"W1001","applicability":"certain","title":"Prefix 'unused' with '_'","edits":[{"path":"Main.tz","span":{"start":44,"end":50,"line":3,"column":5,"end_line":3,"end_column":11},"text":"_unused"}]}
{"event":"file","path":"Main.tz","fixes":1}
{"event":"summary","dry_run":false,"applicability":"certain","rounds":1,"stop":"fixed_point","applied":1,"skipped":0,"files":1,"remaining_errors":0,"remaining_warnings":0}
```

### 診断（このチケットが出すもの）

メッセージは英語・小文字始まり・末尾にピリオドなし・直し方を含む（D-51）。`kind` は `with_kind`。`related`・`help`・`fixes` は付けない。

| コード | kind | 条件 | メッセージの雛形 | 位置 |
| --- | --- | --- | --- | --- |
| `E2008` | `fix_verification_failed` | ラウンドの検証の失敗 | `the fixes would introduce {code} in '{path}' at line {line}: {message}; no file was changed, review the fixes with --dry-run --json`（増えた最初の診断。V2・V3 は件数が増えた `(path, code)` の `after` で最初のもの、V1 だけなら `after` の最初の誤り） | `<command line>` |
| `E2008` | `fix_source_changed` | 書く直前の読み直しが、解析したときの内容と違う | `'{path}' changed while fixing; nothing was left replaced, rerun tsuzuri fix` | そのファイルのパス、引用なし |
| `E2008` | `fix_write_failed` | I/O の失敗、シンボリックリンク・通常でないファイル、巻き戻しの失敗、`set_edition` の失敗 | `cannot replace '{path}': {detail}; {n} earlier replacements were restored`（巻き戻せないファイルがあれば `; could not restore {paths}`） | そのファイルのパス、引用なし |
| `E2008` | `fix_edition_blocked` | 移行後も目標 edition が拒否する誤りが残る | `edition {target} still rejects {n} places after the fixes; Tsuzuri.toml was not changed, fix them by hand and rerun tsuzuri fix --edition {target}`（この前に残った誤りを通常の診断として出す） | `<command line>` |
| `E1017` | `fix_candidate_limit` | 1 ラウンドの採用候補が 20,000 件超 | `too many fixes (more than 20000) in one round; run tsuzuri fix on one file at a time` | `<command line>` |

**例 4**（想定出力。`fix_verification_failed`。偽の fix が型の誤りを足したとき）:

```text
<command line>:1:1: error[E2008]: the fixes would introduce E1003 in 'Main.tz' at line 3: expected i64, found bool; no file was changed, review the fixes with --dry-run --json
```

**例 5**（想定出力。`fix_source_changed`。`--json`。G22 Phase 1 が前提なので、D-51 の順序で `kind` が `span` の次に入る）:

```text
{"severity":"error","code":"E2008","message":"'Main.tz' changed while fixing; nothing was left replaced, rerun tsuzuri fix","path":"Main.tz","span":{"start":0,"end":0,"line":1,"column":1,"end_line":1,"end_column":1},"kind":"fix_source_changed"}
```

**例 6**（想定出力。`fix_edition_blocked`。Phase 3。A22 の後）:

```text
Main.tz:7:16: error[E0004]: '..' is not allowed in edition 2027; write '..='
  7 |     for i in 0 .. n do
    |                ^

<command line>:1:1: error[E2008]: edition 2027 still rejects 1 places after the fixes; Tsuzuri.toml was not changed, fix them by hand and rerun tsuzuri fix --edition 2027
```

### 上限・決定性・互換

| 名前 | 値 | 超えたとき |
| --- | --- | --- |
| `MAX_ROUNDS` | 5 | 残りを `round_limit` で報告して終了（0） |
| `MAX_CANDIDATES` | 20,000（1 ラウンド） | `E1017` |
| `MAX_EDIT_TEXT` | 4,096 bytes（1 edit） | その fix は `invalid_edit` |
| `MAX_DIFF_CHANGES` | 2,000（挿入と削除の行数） | 差分を出さない（注記か `"diff":null`） |
| ファイルの大きさ | `MAX_SOURCE_BYTES`（4 MiB） | その fix は `invalid_edit` |

- 出力は入力のソースとコンパイラだけで決まる（日時・絶対パス・環境変数・スレッドの数を含めない。`TSUZURI_THREADS` は結果を変えない）。整列は fix が整列キー、ファイルが `project.sources` の順、診断が解析の順。2 回目の実行は `no fixes to apply` になるので、決定性は `--dry-run` の 2 回で比べる。
- `docs/stability.md` に従う: `fix --json` のキーは増えることがあるが、消さず・改名しない（`api-diff` と同じ。文書の表に 1 行足す）。メッセージの文言は保証の対象外。
- edition の規則（D-45・D-54）: `fix` は言語の意味を変えない（`certain` は意味を保つ。`likely` は人が確かめる）。`--edition` は全 edition が受理する正準形へ書き換えてから manifest の `edition` を上げるだけで、予約語も警告の既定も変えない。
- 新しいサブコマンド `fix` は、以前 `fix` というパスを暗黙の `build` の入力にしていた書き方を変える（`tsuzuri build fix` か `./fix`。`toolchain` と同じ扱い。`docs/language.md` に 1 行）。

### Phase 2: fix の供給（W1001・W1005）

付録 A に HEAD の診断コードごとの判断を表にした。供給するのは次の 2 つ。`{name}` は `W1001` では未使用の局所、`W1005` では参照の識別子。

| 診断 | edit | title | applicability | 付ける条件 |
| --- | --- | --- | --- | --- |
| `W1001`（`src/warnings.rs` の `unused_locals`） | `Edit { span: local.span, text: format!("_{name}") }` | `Prefix '{name}' with '_'` | `Certain` | (a) 同じ関数のどの局所（引数・入れ子の無名関数・パターンの束縛を含む）の名前も `_{name}` でない。(b) `names.functions` のどのキーも、最後の `.` より後ろが `_{name}` でない（名前の捕獲で束縛が変わらないことの保守的な保証）。満たさないときは fix を付けない（診断は同じ） |
| `W1005`（`src/warnings.rs` の `deprecated_uses`） | `Edit { span: reference.span, text: target }` | `Replace '{name}' with '{target}'` | `Likely` | (1) 宣言が関数（`owner.kind == Function`）。(2) 注記が、前後の空白を除いて `use {path} instead` に完全一致し、`{path}` に空白がない。(3) `{path}` を最後の `.` で割った前半が、診断メッセージの名前（`spelled`）の最後の `.` より前と同じで、後半 `{target}` が小文字か `_` で始まる識別子。(4) `index.definitions` に `module == owner.module`・`name == target`・`kind == Function`・`public`・`parameters == owner.parameters`・`result == owner.result` の定義がある |

**例 7**（想定出力。`W1005`。「現状」5。`--applicability likely --json`）:

```text
{"event":"fix","status":"applied","round":1,"code":"W1005","applicability":"likely","title":"Replace 'old' with 'next'","edits":[{"path":"Main.tz","span":{"start":29,"end":32,"line":3,"column":16,"end_line":3,"end_column":19},"text":"next"}]}
{"event":"file","path":"Main.tz","fixes":1}
{"event":"summary","dry_run":false,"applicability":"likely","rounds":1,"stop":"fixed_point","applied":1,"skipped":0,"files":1,"remaining_errors":0,"remaining_warnings":0}
```

既定（`certain`）では、`status: skipped`・`reason: applicability` の 1 行と、`note: rerun with --applicability likely to apply 1 more fix` が出る。

### Phase 3: edition の移行（`--edition`）

`tsuzuri fix --edition TARGET [--dry-run] [--json] PATH`。適用するのは、`options.only_codes = Some(&["W1007"])` で `certain` の fix だけ（ほかの診断の fix は適用も報告もしない）。

| 段 | 内容 |
| --- | --- |
| 1 | 引数: `Edition::parse` が `None` なら `E2000`（`fix_edition_unsupported`）。`--applicability` と併用なら `E2000`（`fix_edition_with_applicability`） |
| 2 | 読み込み後: `current = project.sources[project.root].edition`。`target == current` なら `edition is already {target}; nothing to migrate` を出して 0（何も書かず、`manifest` は `unchanged`）。`target < current` なら `E2000`（`fix_edition_downgrade`） |
| 3 | `analyze_project(p, true)`（canonical の警告を足した解析器）でラウンドを回す |
| 4 | 再検査: `recheck_as(project, target, ..)` が、`origin == User` かつ `dependency_package == None` のソースの `edition` をメモリ上で `target` にして `analyze_project(p, false)` し、元の `edition` に戻して返す。**誤りがない**ことを求める（警告は合否にしない）。この解析が `remaining`（最後の診断）になる |
| 5a | 誤りがない: ソースを `replace_files`（巻き戻しつき）、最後に根のマニフェスト（`project.manifests` のうち `package == project.root_package` で `relative_path == "Tsuzuri.toml"`）の `edition` を `package::set_edition` で `target` にして `replace_files`。ソース → マニフェストの順なので、途中で止まっても全 edition が受理する正準形のままでプロジェクトは有効。マニフェストの失敗でソースは戻さない |
| 5b | 誤りが残る: ソースは書く（`certain` だけで旧 edition でも有効）が `Tsuzuri.toml` は変えず、`E2008`（`fix_edition_blocked`）、終了コード 1 |
| 5c | `Tsuzuri.toml` がない入力: ソースを書くだけ。`manifest` は `none`。`edition: no Tsuzuri.toml; pass --edition {target} to check, build, run, and test` |

人間向けの行: `edition: Tsuzuri.toml now says edition = "{target}"`／`edition: would set edition = "{target}" in Tsuzuri.toml`（`--dry-run`）／5c の行／`edition: blocked; Tsuzuri.toml was not changed`。`--dry-run` はソースの差分とマニフェストの差分（`diff::unified("Tsuzuri.toml", ..)`）を出し、何も書かない。

`package::set_edition(text: &str, edition: &str) -> Option<String>` は行指向（TOML のパーサは書かない）。**`[package]` の節の中だけ**を見る（次の `[` で始まる行で節が終わる）。呼び出し側が `parse_manifest` で読み直して `edition == target` を自己検査し、食い違えば `E2008`（`fix_write_failed`）。

| `[package]` の節 | 結果 |
| --- | --- |
| `edition = "2026"` の行がある（`edition="2026"`・タブのインデント・行末のコメントも） | 値の `2026` だけを置き換える（空白・インデント・コメント・改行はそのまま） |
| `edition` の行がなく、`version = ...` の行がある | その行の直後に `edition = "2027"` の行を足す。改行はその行と同じ（CRLF なら `\r\n`）。ファイルの最後の行で改行がなければ、改行を前に足し、末尾の改行は足さない |
| `# edition = "2026"` のコメントだけ・`[dependencies]` などほかの節の `edition = ...` | 無視する（`edition` の行がない扱い） |
| すでに目標の値 | 同じ文字列を返す（冪等） |
| `[package]` の節がない、`version` も `edition` の行もない、`edition` の値が `"` で始まらない・閉じない | `None`（呼び出し側が `E2008`・`fix_write_failed`） |

**A22 との接点**: `fix::canonical_warnings(project: &Project, module: &CheckedModule) -> Vec<Diagnostic>` は本チケットでは**空を返す**（`W1007` はまだない）。A22 が本体を差し替える。`Edition::E2027`・`E0004` による 2027 の拒否も A22 が足す。それまで `--edition 2027` は `E2000`、`--edition 2026` は「すでに」になる。

### Phase 4: LSP

`initialize` の `codeActionProvider` を `{"codeActionKinds": ["quickfix", "source.fixAll.tsuzuri"]}` にする。`textDocument/codeAction` は、`context.only` がない、または `source`・`source.fixAll`・`source.fixAll.tsuzuri` のどれかを含むとき、その文書のプロジェクトの `ProjectState.diagnostics` に `certain` の fix が 1 つでもあれば、`fix::run`（`threshold: Certain`、`Scope::File`＝その文書）を、開いているバッファを重ねて読み込んだプロジェクトで実行し、変わったときだけ `{"title": "Fix all auto-fixable problems", "kind": "source.fixAll.tsuzuri", "edit": {"changes": {uri: [TextEdit...]}}}` を 1 件返す（`isPreferred` なし）。中止・変更なし・`certain` の fix なしは、この action を返さない（エラーにしない）。`only` が `quickfix` だけなら従来どおり。`TextEdit` は `diff::line_edits` の各編集を `PositionMapper::range`（元のテキストで）で LSP の範囲にしたもの。`line_edits` が `None` なら文書全体を 1 つの編集で置き換える（`formatting` と同じ形）。

## 設計

### 新規・変更するファイル

| ファイル | 新規／変更 | 責務 |
| --- | --- | --- |
| `src/diff.rs` | 新規（P1） | 行の最短編集列（Myers）・統一差分・行単位の編集。外部の `diff` も新しい crate も使わない |
| `src/fix.rs` | 新規（P1） | 型、`collect`・`run`（ラウンドと検証）、出力、`parse_arguments`、`execute_with`・`execute`、Phase 3 の `recheck_as`・`canonical_warnings` |
| `src/lib.rs` | 変更 | `pub mod diff;`・`pub mod fix;` |
| `src/driver.rs` | 変更（P1） | `FileUpdate`・`ReplaceFailure`・`ReplaceError`・`replace_files`・`replace_file`。`replace_source_atomically`（fmt）は**変えない**（構造の写しを `replace_file` に作る。D6） |
| `src/main.rs` | 変更（P1） | `fix_command`、`main` の分岐（`api-diff` の次）、`HELP` に使い方 1 行と説明 3 行 |
| `src/warnings.rs`・`src/check.rs` | 変更（P2） | `unused_locals(function, names)` と `deprecated_uses` に `fixes`。`check.rs` の呼び出し 1 か所に `&names` |
| `src/package.rs` | 変更（P3） | `set_edition` |
| `src/lsp.rs` | 変更（P4） | `Session::fix_all`・capability・`code_actions` の分岐 |

### データ構造（名前と型を変えない）

```rust
// src/diff.rs
pub const MAX_DIFF_CHANGES: usize = 2000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Step { Equal, Delete, Insert }
pub struct LineEdit { pub start: usize, pub end: usize, pub text: String } // 元のバイト範囲（行の境界）と置換
pub fn split_lines(text: &str) -> Vec<&str>;                                // split_inclusive('\n')
pub fn shortest_script(old: &[&str], new: &[&str]) -> Option<Vec<Step>>;   // Myers。変更行が上限超で None
pub fn unified(path: &str, old: &str, new: &str, context: usize) -> Option<String>;
pub fn line_edits(old: &str, new: &str) -> Option<Vec<LineEdit>>;

// src/fix.rs
pub const MAX_ROUNDS: usize = 5;  pub const MAX_CANDIDATES: usize = 20_000;  pub const MAX_EDIT_TEXT: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Threshold { Certain, Likely, Maybe }
impl Threshold { pub fn admits(self, applicability: Applicability) -> bool; }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Scope { Project, File(usize) } // File は source の索引
pub struct Options { pub threshold: Threshold, pub scope: Scope, pub only_codes: Option<&'static [&'static str]> }
pub struct Analysis { pub errors: DiagnosticSet, pub warnings: Vec<Diagnostic> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Skip { Applicability, OutsideProject, OutsideInput, InvalidEdit, RoundLimit }
impl Skip { pub fn name(self) -> &'static str; } // "applicability" "outside_project" "outside_input" "invalid_edit" "round_limit"
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Status { Applied { round: usize }, Skipped(Skip) }
pub struct FixRecord { pub status: Status, pub code: &'static str, pub kind: Option<&'static str>,
                       pub applicability: Applicability, pub title: String, pub edits: Vec<RecordedEdit> }
// 記録した時点（そのラウンドの開始時のテキスト）の位置。行と列は診断と同じ（1 始まり、列は Unicode スカラー値）。
// 範囲外の索引は path = "<invalid>"、行と列は 0
pub struct RecordedEdit { pub path: String, pub start: usize, pub end: usize, pub line: usize,
                          pub column: usize, pub end_line: usize, pub end_column: usize, pub text: String }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Stop { FixedPoint, RoundLimit }
pub struct FileChange { pub source: usize, pub original: String, pub fixed: String, pub fixes: usize }
pub struct Report { pub rounds: usize, pub stop: Stop, pub records: Vec<FixRecord>,
                    pub files: Vec<FileChange>, pub last: Analysis }
pub fn analyze_project(project: &Project, canonical: bool) -> Analysis;
pub fn run(project: &mut Project, options: &Options,
           analyze: &mut dyn FnMut(&Project) -> Analysis) -> Result<Report, SourceError>;
pub struct Arguments { pub input: PathBuf, pub threshold: Threshold, pub dry_run: bool,
                       pub json: bool, pub edition: Option<Edition> }
pub fn parse_arguments(arguments: &[OsString]) -> Result<Arguments, Diagnostic>; // E2000 と kind
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestStatus { Updated, WouldUpdate, Unchanged, None, Blocked }
pub struct Execution { pub stdout: String, pub exit: u8, pub remaining: Option<Analysis>,
                       pub blocked: Option<SourceError> }
pub fn execute_with(project: &mut Project, arguments: &Arguments,
                    analyze: &mut dyn FnMut(&Project, bool) -> Analysis) -> Result<Execution, SourceError>;
pub fn execute(project: &mut Project, arguments: &Arguments) -> Result<Execution, SourceError>; // analyze_project を渡す
pub fn recheck_as(project: &mut Project, target: Edition,
                  analyze: &mut dyn FnMut(&Project) -> Analysis) -> Analysis;           // Phase 3
pub fn canonical_warnings(project: &Project, module: &CheckedModule) -> Vec<Diagnostic>; // Phase 3。空を返す

// src/driver.rs
pub struct FileUpdate { pub path: PathBuf, pub original: String, pub replacement: String }
pub enum ReplaceFailure { Changed { path: PathBuf }, Failed { path: PathBuf, detail: String } }
pub struct ReplaceError { pub failure: ReplaceFailure, pub restored: usize, pub unrestored: Vec<PathBuf> }
pub fn replace_files(updates: &[FileUpdate]) -> Result<(), ReplaceError>;
```

失敗の型は `SourceError`（`src/driver.rs`。`path` と `diagnostic`）。引数・出力の段階の `E2000`・`E1017`・`E2008`（`fix_verification_failed`・`fix_edition_blocked`）の `path` は `<command line>`。出力は `pub(crate) fn render_text(..)`・`render_json(..) -> String`（`Report`・`Project`・`dry_run`・`Threshold`・移行の結果を受ける。手順 6）。`Skip`・`Status` の `name()`・出力の文字列は上の表と一致させる。

`main.rs` の `fix_command`（糊だけ）:

```text
json = 引数に --json がある
args = fix::parse_arguments(引数)           // Err: print_diagnostic(<command line>) して終了コード 2
invalid_thread_setting(json)                 // 2
project = Project::load(&args.input)         // Err: print_diagnostic(error.path) して 1
match fix::execute(&mut project, &args):
  Ok(done): print!("{}", done.stdout)
            done.remaining があれば print_diagnostics（誤り→警告。警告は DiagnosticSet::from_diagnostics(.., 0)）
            done.blocked があれば print_diagnostic(&blocked.diagnostic, &blocked.path, "", json)
            ExitCode::from(done.exit)
  Err(e):   print_diagnostic(&e.diagnostic, &e.path, "", json); 終了コード（コードが E2000 なら 2、それ以外は 1）
```

### アルゴリズム

**A1. `shortest_script`（Myers の貪欲な前向き版）。** 付録 B のコード（検証済み）をそのまま使う。要点: `limit = min(n + m, MAX_DIFF_CHANGES)`、`d = 0..=limit`・`k = -d, -d+2, ..., d` で `x = if k == -d || (k != d && v[k-1] < v[k+1]) { v[k+1] } else { v[k-1] + 1 }`、`y = x - k`、斜めに進めて `v[k] = x`、`x >= n && y >= m` で完了。各 `d` の後に `v[-d..=d]` の写しを `trace` に積み、完了したら `trace` を後ろへ戻って `Step` を作る。`d` が `limit` を超えたら `None`。計算量 `O((n+m)·D)`、記憶 `O(D²)`（20,000 行・`D = 2000` で約 15 ms・約 40 MiB。付録 C）。

**A2. `unified`。** `old == new` なら空文字列、`shortest_script` が `None` なら `None`。行は改行を含めて比べる。各 `Step` の前の `(i, j)` を持ち、`Equal` でない `Step` を、間の `Equal` が `2*context` 行以下なら 1 つにまとめ、前後 `context`（既定 3）行を足して hunk にする。`@@ -{a} +{b} @@`、範囲は `range(start0, len)`: `len == 0` なら `{start0},0`、`len == 1` なら `{start0 + 1}`、それ以外は `{start0 + 1},{len}`。先頭に `--- a/{path}`・`+++ b/{path}`（時刻なし）。行は ` `・`-`・`+` を付け、`\n` で終わらない行は `\n\ No newline at end of file\n` を続ける。出力は `diff -u` と一致する（付録 C）。

**A3. `line_edits`。** `Equal` でない連続を 1 つの `LineEdit` にする。`start`・`end` は削除する行の範囲の行頭のバイト位置（挿入だけなら `start == end`）、`text` は挿入する行の連結。`apply(old, edits) == new`（往復）。

**A4. 交わりの解消。** `Candidate { record: FixRecord, edits: Vec<ResolvedEdit> }`（`ResolvedEdit { source, start, end, text }`）を整列・重複除去した後、`accepted: BTreeMap<source, BTreeMap<start, end>>` を空で始め、各候補の全 edit について `accepted[source].range(..=end).next_back()` が `(s, e)` で `e >= start` なら交わる。交わらなければ全 edit を入れて採用、交われば捨てる。`O(E log E)`。

**A5. 適用。** ファイルごとに採用した edit を `start` の昇順に並べ、`text[..e0.start] + e0.text + text[e0.end..e1.start] + ... + text[ek.end..]` を 1 回の走査で作る（空の `start == end` は挿入）。

**A6. `run`。**

```text
saved = {}; applied = []; current = analyze(project); rounds = 0
loop:
    (chosen, skipped) = collect(project, current, options)?           // E1017 はここ
    if chosen.is_empty():     stop = FixedPoint; break
    if rounds == MAX_ROUNDS:  stop = RoundLimit; skipped += chosen を Skipped(RoundLimit) にしたもの; break
    apply(project, chosen, saved)
    next = analyze(project)
    if let Err(violation) = verify(project, current, next):
        message = violation を（いまのテキストで）文字列にする
        restore(project, saved)                                       // 全ファイルを元に戻す
        return Err(E2008 fix_verification_failed(message))
    applied += chosen を Applied{round: rounds + 1} にしたもの; rounds += 1; current = next
files = saved の各ファイルで、project のテキストが元と違うもの（source の昇順）
return Report { rounds, stop, records: applied + skipped, files, last: current }
```

**A7. `replace_files`。** `updates` の順に `replace_file(path, original, replacement)` を呼ぶ。`replace_file`: (1) `symlink_metadata` が通常のファイルでなければ `Failed`（`detail`: `refusing symlinks, directories, and special files`）。(2) `read_source_text(path)` が `original` と違えば `Changed`。(3) `TemporaryDirectory::new(親)` に新しい内容を書き、元の権限を `set_permissions`、`sync_all`。(4) (1)(2) をもう一度確かめる。(5) `fs::rename`。I/O の失敗は `Failed { detail }`。`i` 番目で失敗したら `updates[..i]` を逆順に `replace_file(path, replacement, original)` で戻し、戻せた数を `restored`、戻せなかったパスを `unrestored` に入れる。`Changed`・`Failed` は `E2008` の `fix_source_changed`・`fix_write_failed` になる。

**A8. `execute_with`。** (1) `scope = File(project.root)`（入力がファイル）か `Project`。(2) 通常: `report = run(project, &Options { threshold, scope, only_codes: None }, &mut |p| analyze(p, false))?`。`--dry-run` でなければ `report.files` を `FileUpdate`（パスは `project.sources[source].path`）にして `replace_files`（失敗は `E2008`）。`stdout` を「出力」で作り、`remaining = Some(report.last)`（`--dry-run` は `None`）、`exit = 0`。(3) `--edition`: 「Phase 3」の段 2〜5 のとおり（`only_codes = Some(&["W1007"])`、`Threshold::Certain`、解析器 `|p| analyze(p, true)`、再検査 `recheck_as(project, target, &mut |p| analyze(p, false))`）。

**A9. LSP の `fix_all`。** (1) `state.diagnostics` に `certain` の fix がなければ `None`。(2) `overlays` は `verify_edits` と同じ。`project = Project::load_with_overlays(directory, &overlays)`（失敗は `None`）。(3) 要求の文書と `path` が同じソースの索引を探す（旧い `source_id` は使わない）。(4) `fix::run(&mut project, &Options { threshold: Certain, scope: File(index), only_codes: None }, &mut |p| analyze_project(p, false))`。`Err`・`files` が空なら `None`。(5) `diff::line_edits(&change.original, &change.fixed)` の各 `LineEdit` を `{"range": PositionMapper::new(&change.original, self.encoding).range(&change.original, Span::new(start, end)), "newText": text}` にする（`None` なら文書全体の 1 編集）。(6) action を返す。

### 前提とする他チケットのインターフェース

| チケット | 使う名前（D-51。変えない） | 未実装のときの動き |
| --- | --- | --- |
| G22 Phase 1 | `Applicability::{Certain, Likely, Maybe}`（`Clone`・`Copy`・`PartialEq`・`Eq`・`Debug` を持つ。JSON の文字列 `certain`／`likely`／`maybe` は G22 の描画と同じものを使う）、`Edit { span: Span, text: String }`、`Fix { title: String, applicability: Applicability, edits: Vec<Edit> }`、`Diagnostic.fixes: Vec<Fix>`、`Diagnostic.kind: Option<&'static str>`、`Diagnostic::fix(self, title, applicability, edits) -> Self`、`Diagnostic::with_kind(self, kind) -> Self`。JSON の `fixes`: `[{"title","applicability","edits":[{"path","span","text"}]}]`。人間向けの `  fix (certain): <title>` | 着手しない（停止条件 1）。機能を出さない |
| G22 Phase 4 | LSP の `fixes` 由来の quickfix（`fix_edit` が警告の fix も検証する）。**既存の `W1001` の特別扱い（`code_actions` の `diagnostic.code != "W1001"` の分岐と `rename` 経路）は変えない**（G22 の決定） | 手順 11 が、手順 9 で `W1001` に `fixes` が付いたあとにその特別扱いを消し、`fixes` 由来の quickfix に一本化する（`code_action_prefixes_unused_locals` の期待値を保つ） |
| G23 | 本チケットの 10 個の kind（診断の表 5 個と `E2000` の表 5 個）。G23 があれば自分で登録簿へ足して `Diagnostic::of` で出す（D-51） | `with_kind` の文字列のまま |
| A22 | `Edition::E2027`・`SUPPORTED`・`W1007`（`certain` の fix つき）・`E0004`。`fix::canonical_warnings` の本体の差し替え | `--edition 2027` は `E2000`、`--edition 2026` は「すでに」 |
| G25 Phase 4 | `E1002` の `candidates` と `maybe` の fix | fix がなければ何も起きない |

## 実装手順

各手順の最後で `cargo build`・`cargo test --locked` が通る。`cargo test <名前>` は 0 件でも成功するので、出力の `running N tests` の N を確かめる。先にテストを書く。1 手順は 300 行以内を目安にする。テストの入力と期待値は付録 D。

### Phase 1: 差分・エンジン・書き込み・CLI

**手順 1: ベースライン。** 変更なし。`cargo build --release --locked && cargo test --locked` の `passed` の合計を控える。`mkdir -p /tmp/tz-G24 && cp target/release/tsuzuri /tmp/tz-G24/tsuzuri-before`。`examples/` の 13 個と `tests/fixtures/` の各ディレクトリを `tsuzuri-before check <dir> --json > /tmp/tz-G24/before-<名前>.txt 2>&1` で保存する（手順 7・9・10 で比べる）。「現状」の 1・2・3・5・6 を `$WORK` に作り直し、出力が一致することを確かめる。
- 確認: すべて通る。

**手順 2: `src/diff.rs`（A1〜A3）。** 変更: `src/diff.rs`（新規）、`src/lib.rs`。単体テスト 4 件を先に書いて失敗させ、付録 B の `shortest_script` を出発点に `split_lines`・`unified`・`line_edits` を実装する。
- 確認: `cargo test --locked --lib diff::` が `running 4 tests` で通る。`cargo clippy --all-targets -- -D warnings`。

**手順 3: `src/fix.rs`（型・検証・交わり・適用）。** 変更: `src/fix.rs`（新規）、`src/lib.rs`。型一式、`Threshold::admits`、`Skip::name`、`validate`（「書いてよいファイル」）、`collect`、A4・A5、定数。先にテスト 7 件（`threshold_admits_by_applicability`・`rejects_edits_outside_the_writable_files`・`conflict_is_closed_interval_intersection`・`resolve_keeps_the_first_in_sorted_order_and_defers_the_rest`・`identical_fixes_collapse`・`apply_edits_builds_the_text_in_one_pass`・`candidate_limit_is_e1017`）。
- 確認: `cargo test --locked --lib fix::` が `running 7 tests` で通る。

**手順 4: `run`・`verify`・`analyze_project`。** 変更: `src/fix.rs`。A6、V1〜V3、`analyze_project`（`canonical` は Phase 3 まで無視）、`restore`。先に偽の解析器のテスト 8 件（`rounds_stop_at_the_fixed_point`・`touching_fixes_apply_in_two_rounds`・`a_round_that_adds_an_error_aborts_without_changes`・`a_round_that_adds_a_warning_aborts_only_when_errors_were_absent`・`error_fixes_may_reveal_hidden_warnings`・`round_limit_stops_at_five_and_reports_the_rest`・`omitted_errors_skip_the_pair_check`・`file_scope_skips_other_files`）。
- 確認: `cargo test --locked --lib fix::` が `running 15 tests` で通る。

**手順 5: `src/driver.rs` の `replace_files`。** 変更: `src/driver.rs`、`tests/fix.rs`（新規。`root(label, files)` は `tests/api_diff.rs` と同じ形）。A7。`replace_source_atomically`・`format_sources` は触らない。先にテスト 4 件（`replace_files_replaces_every_file`・`replace_files_restores_earlier_files_when_a_later_one_changed`・`replace_files_refuses_symlinks`（unix）・`replace_files_keeps_permissions`（unix））。
- 確認: `cargo test --locked --test fix` が `running 4 tests`（Windows は 2）で通る。`cargo test --locked --test formatter` が手順 1 と同じ件数で通る。

**手順 6: `parse_arguments`・出力・`execute_with`。** 変更: `src/fix.rs`。「CLI」の表どおりの `parse_arguments`（Phase 1 は `--edition` を知らない）、「出力」の表どおりの `render_text`・`render_json`、A8 の `execute_with`・`execute`。先にテスト 2 件（`parse_arguments_table`・`render_text_and_json_are_stable`）。
- 確認: `cargo test --locked --lib fix::` が `running 17 tests` で通る。

**手順 7: `src/main.rs` の配線と CLI テスト。** 変更: `src/main.rs`、`tests/fix.rs`。設計の `fix_command` を `api_diff_command` の隣に書き、`main` の `api-diff` の分岐の次に `fix` を足す。`HELP` の使い方に `tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--json] source.tz|source.tt|source.tc|directory` を 1 行、`api-diff` の説明の次に 3 行の説明（診断の `fixes` を適用する・既定は `certain`・`--dry-run`）を足す。`mod tests` に `the_help_lists_the_fix_command`。`tests/fix.rs` に `usage_errors_exit_2`・`no_fixes_prints_a_note_and_changes_nothing`・`a_directory_without_main_fails_like_check`・`dependencies_and_std_are_never_candidates`・`run_with_a_fake_analyzer_writes_and_reports_on_a_real_project`。
- 確認: `cargo test --locked --test fix` が `running 9 tests` で通る。`cargo test --locked`（全体）が手順 1 の件数＋ 31 件（diff 4、fix 17、fix.rs 9、main 1）で通る。手順 1 の保存と `tsuzuri check` の出力が 1 byte も変わらない。

**手順 8: Phase 1 の文書。** 「ドキュメント」の Phase 1 の行。`node scripts/check-docs.mjs <変更したページ>` が通る。

### Phase 2: fix の供給

**手順 9: `W1001`。** 変更: `src/warnings.rs`（`unused_locals(function, names: &Names)`）、`src/check.rs`（呼び出し 1 か所に `&names`）、`tests/fix.rs`。先にテスト 2 件（`w1001_carries_a_certain_prefix_fix`・`w1001_has_no_fix_when_the_prefixed_name_is_taken`）。`unused_locals` は、「Phase 2」の W1001 の条件 (a)(b) を満たすとき `.fix(format!("Prefix '{name}' with '_'"), Applicability::Certain, vec![Edit { span: local.span, text: format!("_{name}") }])` をつなぐ。(a) は関数内の `locals` の名前の集合、(b) は `names.functions` のキーの最後の `.` より後ろ（診断が 1 件もなければ走査しない）。
- 確認: `cargo test --locked --test fix` が `running 11 tests` で通る。`tsuzuri check` の出力は `W1001` の診断だけが変わる（JSON に `fixes`、人間向けに `  fix (certain): ...`）。

**手順 10: `W1005`。** 変更: `src/warnings.rs`（`DeprecatedUse` に `replacement: Option<String>`。条件 (1)〜(4) は純粋な補助関数 `replacement_for(owner: &Definition, spelled: &str, note: &str, index: &SemanticIndex) -> Option<String>` に切り出す）、`tests/fix.rs`。先にテスト 2 件（`w1005_carries_a_likely_replacement_for_a_same_signature_function`・`w1005_has_no_fix_when_the_signature_differs_or_the_note_is_free_text`）。
- 確認: `cargo test --locked --test fix` が `running 13 tests` で通る。

**手順 11: LSP の `W1001`。** 変更: `src/lsp.rs`（`code_actions`）、`tests/lsp.rs`。内容: G22 Phase 4 がマージ済みなら、手順 9 で `W1001` に `fixes` が付いたので、`code_actions` の `W1001` 専用のループ（`diagnostic.code != "W1001"` の分岐と `rename` を使う経路）を**削除**し、G22 Phase 4 が足した `fixes` 由来の quickfix だけを残す（title `Prefix 'spare' with '_'`・`isPreferred: true`・編集は同じ。**同じ quickfix が 2 つ出ない**）。G22 Phase 4 が未マージなら、この手順は行わない（LSP は従来の `W1001` の quickfix のまま。手順 9 の `fixes` は `tsuzuri fix` だけが使う）。
- 確認: `cargo test --locked --test lsp code_action_prefixes_unused_locals` が `running 1 test` で期待値を変えずに通る。`tests/lsp.rs` に `code_action_returns_one_quickfix_for_w1001`（新規。`W1001` の診断に対する quickfix が 1 つだけで、title と編集が従来と同じ）を足して通る。

**手順 12: CLI の結合テストと Phase 2 の文書。** 変更: `tests/fix.rs`、文書。テスト 7 件（`fix_prefixes_unused_locals_and_a_second_run_changes_nothing`・`fix_dry_run_prints_the_diff_and_writes_nothing`・`fix_json_events_are_one_object_per_line`・`fix_applies_deprecation_replacements_only_with_likely`・`fix_never_writes_dependencies`・`fix_with_a_file_input_writes_only_that_file`・`fix_then_check_with_a_warm_cache_sees_the_new_text`）。
- 確認: `cargo test --locked --test fix` が `running 20 tests` で通る。`node scripts/check-docs.mjs <変更したページ>` が通る。

### Phase 3: edition の移行

**手順 13: `package::set_edition`。** 変更: `src/package.rs`。先に `set_edition_table`（「Phase 3」の表の各行。付録 D）、次に付録 B の `set_edition` を出発点に実装する。
- 確認: `cargo test --locked --lib package::` が通る。

**手順 14: 移行の流れ。** 変更: `src/fix.rs`。`recheck_as`・`canonical_warnings`（空）・`analyze_project` の `canonical`・`execute_with` の `--edition` の分岐（A8）。先にテスト 4 件（`migration_applies_canonical_fixes_then_checks_the_target`・`a_blocked_migration_writes_sources_but_not_the_manifest`・`migration_without_a_manifest_reports_none`・`migrating_to_the_current_edition_is_a_no_op`。`Edition(2027)` はクレート内で作れる）。
- 確認: `cargo test --locked --lib fix::` が `running 21 tests` で通る。

**手順 15: `--edition` の配線と文書。** 変更: `src/fix.rs`（`parse_arguments`）、`src/main.rs`（`HELP` の 1 行に `[--edition YEAR]`）、`tests/fix.rs`、文書。テスト 3 件（`edition_option_rejects_unsupported_years`・`edition_option_to_the_current_edition_is_a_no_op`・`edition_option_conflicts_with_applicability`）。
- 確認: `cargo test --locked --test fix` が `running 23 tests` で通る。`--edition 2030` が `E2000`。

### Phase 4: LSP

**手順 16: `source.fixAll.tsuzuri`。** 変更: `src/lsp.rs`、`tests/lsp.rs`。A9 の `fix_all`、capability、`code_actions` の `context.only` の判定。`tests/lsp.rs` の `initialize` の期待値（`codeActionKinds`）を直し、`code_action_fix_all_prefixes_every_unused_local_of_the_document`・`code_action_fix_all_is_offered_only_when_asked_and_when_there_is_something_to_fix` を足す。
- 確認: `cargo test --locked --test lsp` が通る。`cargo build --release --locked && node tests/lsp_sessions.mjs target/release/tsuzuri` が通る。

**手順 17: Phase 4 の文書と全体の確認。** 「ドキュメント」の Phase 4 の行。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked` を通し、「受け入れ条件」を埋め、チケットの末尾に `## 実装と検証（YYYY-MM-DD）`（GUIDE §13.2）を足す。

## テスト計画

期待値は独立した参照から作る: 差分は `diff -u`（システムの `diff`）と、テスト内の LCS の動的計画、書き換え後のテキストとマニフェストは手で書いた値。コンパイラの現在の出力を写さない。**入力と期待値の全表は付録 D**。ここには名前と件数だけを書く。

| ファイル | テスト（名前） | 件数 |
| --- | --- | --- |
| `src/diff.rs`（単体） | `matches_the_lcs_length_and_round_trips`・`unified_output_matches_diff_dash_u`・`the_change_limit_returns_none`・`line_edits_cover_each_changed_run` | 4 |
| `src/fix.rs`（単体。Phase 1） | 手順 3 の 7 件、手順 4 の 8 件、`parse_arguments_table`・`render_text_and_json_are_stable` | 17 |
| `src/fix.rs`（単体。Phase 3） | `migration_applies_canonical_fixes_then_checks_the_target`・`a_blocked_migration_writes_sources_but_not_the_manifest`・`migration_without_a_manifest_reports_none`・`migrating_to_the_current_edition_is_a_no_op` | 4 |
| `src/package.rs`（単体） | `set_edition_table`（12 行） | 1 |
| `src/main.rs`（単体） | `the_help_lists_the_fix_command` | 1 |
| `tests/fix.rs` Phase 1 | 手順 5 の 4 件、手順 7 の 5 件 | 9 |
| `tests/fix.rs` Phase 2 | 手順 9・10 の 4 件、手順 12 の 7 件 | 11 |
| `tests/fix.rs` Phase 3 | 手順 15 の 3 件 | 3 |
| `tests/lsp.rs` | 新規 2 件（手順 16）。変更 1 件（`initialize` の期待値） | 2（＋1） |

新規の合計は 52 件。偽の解析器は、`project.sources[0].text` を見て `x` の出現ごとに `W9990`（警告）と、その 1 文字を `y` にする `certain` の fix を返す関数で、テストごとにエラー・省略数・別の警告を足す。`Project::single_main(PathBuf::from("Main.tz"), text)` を使う。結合テストは `std::env::temp_dir()` の下に一意なディレクトリを作り、`TSUZURI_CACHE_DIR` を別の一時ディレクトリにする。

### 既存テストへの影響

| 変更 | 影響を受けるテスト | 内容 |
| --- | --- | --- |
| Phase 4 | `tests/lsp.rs` の `initialize` の期待値 1 行 | `codeActionKinds` に `source.fixAll.tsuzuri` が増える（意図した変更） |
| Phase 2 | 診断の表示を全文で固定するテスト | 調べた範囲では**なし**: `tests/script.mjs` の `W1001` は `includes`（75・76 行）、`tests/frontend_cache.mjs` は `"code":"W1001"` の個数（184 行）、`tests/warnings.rs` は手で作った診断（161 行）とメッセージの文字列、`tests/lsp.rs` の `W1001` の診断は `diagnostic_json` の 5 キー。実装者は `grep -rn "W1001\|W1005" tests` で再確認する |
| Phase 1・3 | なし | 新しいサブコマンドと新しいファイルだけ。`HELP` を見るテストは断片の一致（`tests/size.mjs`・`tests/targets.mjs`・`tests/e2e.mjs`） |
| すべて | IR・生成物（`tests/*.mjs`） | なし。生成コードに触れない（手順 7 で `--emit llvm` の出力が 1 byte も変わらないことを確かめる） |

性能: 差分は 20,000 行・変更 2,000 行で約 15 ms・最大約 40 MiB（付録 C）。解析の回数は `ラウンド数 + 1`（通常 2 回）。速度の合否の閾値をテストに入れない。

## ドキュメント

新しい例は `node scripts/check-docs.mjs <ページ>` を通る形にする（` ```tsuzuri ` は `check` が通ること。コマンドの出力は ` ```text `）。言語リファレンスは日本語（です・ます）、出力例は英語。

| Phase | ページ | 内容 |
| --- | --- | --- |
| 1 | `_tsuzuri/language-reference/compiler/usage.md` | `### fmt` の次に `### fix`（使い方・フラグ・出力・書いてよいファイル・検証と全か無・終了コード）。`## サブコマンド` の一覧と `## 終了コード` に `fix` |
| 1 | `compiler/option.md`（18 行目付近） | サブコマンドごとの受け付けるオプションに `fix` |
| 1 | `compiler/diagnostics.md` | エラーコードの表に `E2008`、「`E2000`〜`E2007`」を `E2008` まで、`E1017` の行に `fix` の候補の上限 |
| 1 | `README.md` | 「コマンド書式」に 1 行。「公開 API の差分」の前に「機械的な修正 (`tsuzuri fix`)」の節（例 1・2） |
| 1 | `docs/language.md`・`docs/architecture.md`・`docs/stability.md` | 診断の表に `E2008`、サブコマンドの節に `fix`（暗黙の `build` の注記）。ファイルの表に `src/diff.rs`・`src/fix.rs`、`src/driver.rs` の `replace_files`。安定として保つ表に `fix --json` のキー |
| 2 | `diagnostics.md`・`usage.md` の `### fix` | `W1001`・`W1005` の行に「`tsuzuri fix` が直す」。44 行目の人間向けの例に `  fix (certain): Prefix 'unused' with '_'` の行。「直せる診断」の表（付録 A の 2 行）と例 7 |
| 3 | `usage.md` の `### fix`・`docs/stability.md`・`organizing-tsuzuri/packages.md` | 「edition の移行」の小節（段 1〜5、マニフェストの表）。`stability.md` の「edition を追加するとき」に「移行は `tsuzuri fix --edition`」。`packages.md` の `edition` に 1 文 |
| 4 | `diagnostics.md` の「LSP での表示」・`vsc/README.md` | `source.fixAll.tsuzuri`（`"editor.codeActionsOnSave": {"source.fixAll.tsuzuri": "explicit"}` の例） |

## 受け入れ条件

- [ ] **Phase 1**: 手順 1〜8 が緑。`--lib diff::`（4）・`--lib fix::`（17）・`--test fix`（9）・`--test formatter`（手順 1 と同じ件数）が通る。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`。`check`・`build` の出力が手順 1 の保存と 1 byte も変わらない。`tsuzuri fix <dir>` が `no fixes to apply`（0）。
- [ ] **Phase 2**: 手順 9〜12 が緑。`f1`（付録 C）で `fix` が `_b`・`_unused` に直し、2 回目は何も変えず、`run` の出力が同じ（`40`）。`W1005` は `--applicability likely` のときだけ入る。`code_action_prefixes_unused_locals` が期待値を変えずに通る。
- [ ] **Phase 3**: 手順 13〜15 が緑。`set_edition_table` の 12 行と偽の解析器の移行 4 通りが通る。`--edition 2030` が `E2000`。
- [ ] **Phase 4**: 手順 16・17 が緑。`tests/lsp.rs` の新規 2 件と変更 1 件が通り、`node tests/lsp_sessions.mjs` が通る。
- [ ] `fixes` を持たない診断の JSON が 1 byte も変わらない。`fmt` とそのテストが変わらない。同じ入力で 2 回の `--dry-run` の標準出力が同じ。
- [ ] 「ドキュメント」の全ページを更新し、`node scripts/check-docs.mjs` が通る。完了報告に、統括者が台帳へ追記する名前（`E2008` の kind、`E2000` の kind、`E1017` の kind、fix の title の規則）を列挙した。

## 落とし穴

| 間違えやすい点 | 検出方法 |
| --- | --- |
| **エラーがあると警告が出ない**。エラーを直したラウンドで新しい警告が現れる | `error_fixes_may_reveal_hidden_warnings`。V3 は `before.errors.total == 0` のときだけ |
| **接する fix** を同じラウンドに入れるとトークンがくっつく | `touching_fixes_apply_in_two_rounds`。交わりは閉区間 |
| **位置はラウンドごとに変わる**。JSON の `edits` はそのラウンドの開始時のテキストの位置 | 複数ラウンドの位置を元のファイルの位置と思わない |
| `format_sources` は直下のファイルだけを数える。`fix` は `check` と同じ**再帰的な**プロジェクトを解析する | `fix_with_a_file_input_writes_only_that_file`。`fmt` の動きを手本にしない |
| 依存パッケージの警告（`W1001` は path 依存のソースにも出る）に付いた fix | `fix_never_writes_dependencies`。「書いてよいファイル」の行 3 |
| 書いた後に診断を引用すると、ファイルは新しいテキスト | 標準エラーの診断は `project` のテキスト（書いたテキスト）で引用する。`--dry-run` は診断を出さない |
| `Edition(pub(crate) u16)`: 偽の `Edition(2027)` は `src/` の中でしか作れない | Phase 3 の単体テストは `src/fix.rs` の `mod tests` に置く |
| `Project::load_with_overlays`（LSP）は std を全部読み、`load`（CLI）は使うものだけ読む | 停止条件 5。診断は同じはず（opt-in の std と同名の利用者モジュールは CLI も `E1011` で拒否する。実測: `Net.tz`） |
| 誤りは 1 ラウンド 50 件まで（`MAX_REPORTED_ERRORS`） | `omitted` を V2 で扱う。`tsuzuri fix` を繰り返す |

## 対象外

- コードの整形。`maybe` の自動適用・対話的な選択・VCS の確認（`--dry-run` と全か無の検証と原子的な置換がある）。
- `--warn` の受け取り（`W1006` に fix がない。`W1007` は `--edition` が強制する）。
- 依存・std・`Tsuzuri.lock` の書き換え。`Tsuzuri.toml` は `edition` の行だけ。複数パッケージの一括移行（各パッケージで実行する）。
- `E1019`（`rec` の追加）・`W1002`（未使用の private の削除）・`W1003`（腕の削除）の fix（付録 A。診断が直す範囲を持たない）。
- 検証で見つかった問題の fix だけを捨てて続ける二分探索（D2）。LSP の複数ファイルの編集（D14）。

## 決定事項

すべて承認済み（D-50）。実装者は蒸し返さない。

- **D1: `maybe` は適用しない。`--applicability maybe` は `--dry-run` のときだけ受理する。** 理由: D-55 の値の集合（`certain|likely|maybe`）と D-51（`maybe` は自動では適用しない）を両立し、提案を差分で見られる。却下: 値を `certain|likely` だけにする（設計の種。D-55 と食い違う）、`--yes` で適用（提案の自動適用）。
- **D2: 検証に失敗したら何も書かず `E2008`（全か無）。** 理由: `certain` は意味を保つ契約で、違反は出す側の欠陥。失敗した fix だけを捨てると欠陥を隠す。却下: 失敗した fix だけを捨てる（原因を特定できない）、二分探索（解析が増える）。
- **D3: 別の fix どうしの交わりは閉区間で判定し、整列順で先の fix を採用、後の fix は次のラウンドへ。** 理由: 接する編集はトークンをくっつけて意味を変えうる。却下: 接する区間を許す、編集の併合（出す側の意図を壊す）。
- **D4: ラウンドは最大 5。** 理由: 連鎖は通常 2 ラウンド。収まらなければ `round_limit` で報告し、再実行を促す。却下: 上限なし。
- **D5: 解析は frontend cache を使わない。** 理由: メモリ上の途中のテキストをキャッシュに残さない。内容がキーなので使っても結果は同じだが、書き込みの副作用を避ける。却下: 使う。
- **D6: 書き込みは全ファイルを検証した後に順に置換し、失敗したら巻き戻す。`replace_source_atomically` は変えず、構造を写した `replace_file` を作る。** 理由: `fmt` の文言と動きを 1 byte も変えない。複数ファイルの原子的な置換は POSIX にない。却下: 関数の一般化（`fmt` の文言が変わりうる。停止条件 2）、全部を一時ディレクトリへ書いて一括 rename。
- **D7: 差分は自前の Myers（変更行 2,000 まで）。** 理由: 新しい crate を足せず、外部の `diff` は Windows にない。20,000 行で約 15 ms・約 40 MiB。却下: LCS の動的計画（記憶 `O(NM)`）、外部コマンド、crate。
- **D8: 標準出力に報告（人間向けか JSON Lines）、標準エラーに残りの診断。パスは根からの相対で `/` 区切り。** 理由: 決定的で、`check` の診断の流儀を保つ。却下: 診断と同じ `path`（マニフェストの有無で絶対か相対かが変わる）。
- **D9: edition の移行は `W1007` の `certain` の fix だけを適用し、ソース → マニフェストの順に書く。誤りが残ればソースは書き、マニフェストは変えず、終了コード 1。** 理由: 正準形は全 edition が受理する（D-54）ので、ソースだけが進んだ状態でも有効。部分的な前進を捨てない。却下: 何も書かない、全診断の fix を適用（移行と無関係な書き換えが混ざる）。
- **D10: マニフェストは行指向で編集し、結果を `parse_manifest` で自己検査する。** 理由: コメント・順序・空白を変えない。却下: TOML の再シリアライズ、crate。
- **D11: `W1001` の fix は `certain`。同じ関数の局所と大域の関数名に `_{name}` がないときだけ付ける。** 理由: 名前の捕獲で束縛が変わらないことの保守的な保証。却下: 衝突を無視する、診断の生成で `SemanticIndex` を作って束縛を比べる（重い）。
- **D12: `W1005` の fix は `likely`。使用先が関数で、注記が `use {path} instead`、置換先が同じモジュールの同じ署名の公開関数のときだけ。** 理由: 署名が同じなら型検査は通り、意味は人が確かめる。却下: 自由文からの推測、`certain`（意味が違いうる）。
- **D13: fix の `title` は先頭大文字の命令形の英語。** 理由: 既存の LSP の quickfix の title（`Prefix 'spare' with '_'`）を 1 byte も変えない（D-51 は大文字小文字を決めていない。G22・A22・G25 も合わせる）。
- **D14: LSP の `source.fixAll.tsuzuri` は、要求された文書だけ・`certain` だけ・行単位の編集。** 理由: `editor.codeActionsOnSave` は 1 文書の編集を前提にし、行単位の編集は列の単位（UTF-16／UTF-8）に依存しない。却下: 複数ファイルの編集、文書全体の置換（カーソルと undo が荒れる。`line_edits` が `None` のときだけ使う）。
- **D15: 書いてよいのは根のパッケージの利用者ソースだけ。ファイルを渡したらそのファイルだけ。`Tsuzuri.toml` は `edition` の行だけ。** 理由: 依存・std・`Tsuzuri.lock` を壊さない。`fmt` と同じ流儀。
- **D16: 引数の解析は `fix::parse_arguments`（ライブラリ側）で行い、`main.rs` の `Action` と `parse_arguments` には足さない。** 理由: `bindgen`・`api-diff` と同じ流儀で、`Arguments` の構造体リテラルと他コマンドの `E2000` の分岐を触らずに済む。却下: `Action::Fix`。

## 付録

- [付録 A〜D（診断コードごとの判断表、検証済みの試作、実測と参照出力、テストの入力と期待値）](G24-appendix.md)

## 実装と検証（2026-10-27）

Phase 1〜4 をブランチ `feat/g24-tsuzuri-fix`（G23 の `feat/g23-diagnostic-kinds` の上）に実装した。

### HEAD との食い違いと、その扱い

| 食い違い | 扱い |
| --- | --- |
| A18 が `tsuzuri fix --edition YEAR [--from YEAR] [--dry-run] [--json] PATH`（`src/edition_migration.rs`。値渡しの契約を `own`・`_owned` で保って移行し、目標の edition で再検査し、ソース → マニフェストの順に巻き戻しつきで書く）を既に出荷していた。本チケットは `fix` を未知のサブコマンドとして書かれていた | `tsuzuri fix` の引数解析を 1 つにした（`fix::parse_arguments`）。`--edition` がなければ本チケットの修正案の適用、あれば A18 の移行。Phase 3 の「段 2〜5」（目標での再検査・マニフェストの更新・出力）は A18 の実装と出力をそのまま使い、本チケットからは段 3 の **canonical のラウンド**（`fix::canonical_round`。`only_codes = ["W1007"]` の `certain` の修正案。`canonical_warnings` は A22 まで空）を A18 の `prepare` の最初に足した。`package::set_edition` は A18 の `set_manifest_edition` と同じ役割なので足さなかった（`set_edition_table` も書かない）。`fix_edition_downgrade`・`fix_edition_blocked` の kind は、A18 の `E2008` の `migration_edition_backwards` と、移行前の再検査の誤りがその役を持つので登録しない |
| `--from` | A18 の `--from YEAR` を同じ解析器で受け、`--edition` なしの `--from` は `fix_bad_arguments` にした |
| 引数の誤りの kind | A18 の解析器が使っていた `duplicate_option` などを、本チケットの `fix_bad_arguments`・`fix_edition_unsupported`・`fix_edition_with_applicability`・`fix_maybe_needs_dry_run` に揃えた（メッセージは本チケットの表のとおり。`--edition needs a year such as 2027` と `{flag} specified more than once` は A18 と同じ文言） |
| G23 が済んでいる | 新しい診断は `Diagnostic::of` で、kind を登録簿へ足した（8 個）。`W1001`・`W1005` は kind を持ち、`fix --json` の `fix` の行に `kind` が入る |
| `W1005` の「同じ型」 | `Definition.parameters` は `名前: 型` なので、引数名を除いた型の列と結果の型で比べた（引数名だけが違う置き換えも受ける）。表示した型は型クラスとメンバーの制約を含まないので、`Definition.constraints`（`CheckedFunction::constraint_key`）と、`detail` の `pure`・`const` と宣言のシグネチャも同じときだけ置き換える（`Numeric<'a> =>` の付いた関数への置き換えは、元の呼び出しを型エラーにして実行全体を止めるため） |
| 修正案の span の確かめ | `W1001` はローカルの span が名前の長さと同じとき、`W1005` は参照の span が宣言の名前の長さと同じときだけ修正案を付けた（span が名前でないと置き換えが壊れ、検証で実行全体が止まるため） |
| LSP の `W1001` | 手順 11 のとおり専用の経路（`rename`）を消し、`fixes` 由来の quickfix に一本化した。題・`isPreferred`・編集は同じで、`diagnostics` に公開した診断と同じ `data`（`kind`・`fixes`）が入る（`code_action_prefixes_unused_locals` の期待値の `data` だけを直した） |
| `no_fixes_prints_a_note_and_changes_nothing` | Phase 2 の後は `W1001` が修正案を持つので、修正案のない `W1003`（到達しない腕）だけのプログラムで検査した |

### 実行したコマンドと結果

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 通過。
- `cargo test --locked --lib diff::`: 4 件、`--lib fix::`: 19 件（付録 D2 の 17 件と canonical のラウンドの 2 件）、`--test fix`: 23 件、`--test lsp`: 43 件（新規 2 件）、`--test edition_migration`: 10 件、`--bin tsuzuri`（`the_help_lists_the_fix_command` を含む）。
- `cargo nextest run --locked`: 全件通過（下の最終確認）。
- `node tests/lsp_sessions.mjs`・`ai_cost.mjs`・`script.mjs`・`frontend_cache.mjs`（`target/debug/tsuzuri`）: 通過。`ai_cost.mjs` の能力の検出（`benchmarks/ai/run.mjs` の `probe`）は、`tsuzuri fix` の使い方の誤りが `usage: tsuzuri fix --edition` で始まらなくなったので `diagFix` を有効と判定する。
- `node scripts/check-docs.mjs`（`compiler/usage.md`・`option.md`・`diagnostics.md`・`directives.md`）: 通過。

### 統括者が台帳へ足す名前

`E2008`: `fix_verification_failed`・`fix_source_changed`・`fix_write_failed`。`E2000`: `fix_bad_arguments`・`fix_edition_unsupported`・`fix_edition_with_applicability`・`fix_maybe_needs_dry_run`。`E1017`: `fix_candidate_limit`。修正案の題: `Prefix '{name}' with '_'`（`W1001`、`certain`）、`Replace '{name}' with '{target}'`（`W1005`、`likely`）。LSP の code action の kind: `source.fixAll.tsuzuri`（題 `Fix all auto-fixable problems`）。

### 更新した文書

`compiler/usage.md`（`### fix` の修正案の適用、終了コード）、`compiler/option.md`、`compiler/diagnostics.md`（`E2008`・`E1017`・`W1001`・`W1005`・人間向けの例の `fix` の行・LSP の `source.fixAll.tsuzuri`）、`compiler/directives.md`（例の `fix` の行）、`README.md`、`docs/language.md`、`docs/stability.md`（`fix --json`）、`docs/architecture.md`（`diff.rs`・`fix.rs`）、`vsc/README.md`、`src/explain/codes/E2008.md`。

### 変更したファイル

新規: `src/diff.rs`、`src/fix.rs`、`tests/fix.rs`。変更: `src/lib.rs`、`src/main.rs`（`fix_command`・`HELP`）、`src/driver.rs`（`FileUpdate`・`ReplaceFailure`・`ReplaceError`・`replace_files`）、`src/edition_migration.rs`（canonical のラウンド）、`src/warnings.rs`・`src/check.rs`（`W1001`・`W1005` の修正案）、`src/lsp.rs`（`source.fixAll.tsuzuri`、`W1001` の経路の削除）、`src/diagnostic_kinds.rs`（8 kind）、`tests/lsp.rs`。
