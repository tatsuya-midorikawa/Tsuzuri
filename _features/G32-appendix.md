# G32 付録: 試作の結果・テスト一覧と fixture・CI・AGENTS.md の追記案

本文: [G32-repo-kb-and-ticket-brief.md](G32-repo-kb-and-ticket-brief.md)。この付録の数字と出力は、チケット起票時（HEAD `9dd0fbbc`、作業ツリー）に、捨てた試作（約 600 行の Node スクリプト）を実際に動かして得た。試作はリポジトリに置かない。実装した `build`・`check-guide-lite` が同じ数を出すことが手順の確認になる。

## 付録 A: 試作の結果

### A.1 依存の解析器を README の Mermaid で検証（決定 D4）

`_features/README.md` の依存関係図の辺（`A --> B` 実線、`A -.-> B` 破線）を独立の正解にして、チケットのメタ表の `依存` セルを解析した結果と比べた（実装後は `build_real_repo_invariants` に入れる）。

```js
// 辺: /^\s*([A-Z]{1,2}\d{2})\s*(-->|-\.->)\s*([A-Z]{1,2}\d{2})/gm  → from, weak(= "-.->"), to
// 各辺について、tickets の to の deps に {id: from, weak} があるか
```

結果（125 件）: README の辺 104 本（破線 10）のうち、解析結果に**存在しない辺 0**、弱い印が**食い違う辺 0**。解析結果の辺は全部で 211 本（弱い 54）で、A〜G のチケットでは README に描かれていない辺が 71 本ある（README の図は一部だけを描く）。依存先の ID で実在しないものは 0。`依存` が空のチケットは 19。

### A.2 GUIDE-LITE の検査（`check-guide-lite` の試作）

`_features/GUIDE-LITE.md` を C1〜C9（本文）で調べた件数。統括者の修正後の版（8,959 字）: ファイル 22・ディレクトリ 2・コマンド 14・診断コード 3・決定 19・節番号 8・リンク 4・記号 5・定数 3・テスト名 2・チケット ID 14・裸のファイル名 3 の合計 99 で、**失敗 0 件**。起票時の版（7,539 字）は合計 80 で失敗 1 件だった（調査の途中は `A20` のチケットがなく 2 件。第3期のチケットが揃った時点で `A20-effect-summary-and-checked-attributes.md` ができて消えた）:

```text
symbol: Options::validate — type Options not found
```

GUIDE-LITE の 92 行目「`driver.rs` の `Options::validate`」で、HEAD の型は `BuildOptions`（`grep -n "struct BuildOptions\|pub fn validate" src/driver.rs`）。GUIDE §6.8 の「`driver.rs` の `Options`」も同じ省略。統括者が 2026-10-18 に GUIDE-LITE と GUIDE §6.8 を `BuildOptions` に直した（以後この失敗は出ない。検査器の検証には、この例を再現した複製を使う）。

### A.3 Rust の走査器の比較（決定 D8）

`src/**/*.rs` 93 ファイルを 3 方式で走査し、診断コードの文字列（879 か所）からテスト以外を数えた。

| 方式 | テスト以外 | 備考 |
| --- | --- | --- |
| トークン化して波括弧を数える（採用） | 757（46 種・41 ファイル） | 0.39〜0.42 秒。`function` が決まらないもの 0 |
| 最も近い `fn` に `#[test]` か `#[cfg(test)]` がある | 759 | `src/check.rs` の `verify_children`（テスト mod の補助関数）と `src/polymorph.rs` の `reaches_the_specialization_limit` が漏れる |
| 字下げで `#[cfg(test)] mod` の終わりを決める | 行ごとの比較で 76 行ずれる | `src/syntax_codec.rs` のテストの文字列に列 0 の `}` がある |

再現（手順 3 の確認）:

```sh
node --input-type=module -e '
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { diagSites } from "./scripts/repo-kb/rust.mjs";
let n = 0, nullFn = 0; const codes = new Set();
for (const f of execSync("git ls-files src", { encoding: "utf8" }).split("\n").filter((f) => f.endsWith(".rs")))
  for (const s of diagSites(readFileSync(f, "utf8"))) if (!s.test) { n++; codes.add(s.code); if (!s.function) nullFn++; }
console.log(n, codes.size, nullFn);   // 757 46 0
'
```

### A.4 `brief` の段の表の試作（決定 D7）

`B = 24000`・`16000`・`60000`・`12000` を、作業ツリーの 149 件（第3期のチケットが増え続けていた時点）に当てた見積り（決定の言及は字数の合計で近似。段 5 の上限は本文の表のとおり。最初の試作は段 5 が大きく、`B = 16000` で 3 件が 120〜140 字あふれたので、quick 2,000・start 1,500・steps 1,800 に下げた）。`B = 24000`・`16000`・`60000` は全件が入った。段 0〜6 の件数は `B=24000` で 9・11・6・16・62・45・0、`B=16000` で 0・2・0・1・6・27・113、`B=60000` で 126・22・1・0・0・0・0。`B = 12000` は 149 件すべてが段 6 でも入らない（`--max-chars` の下限が 16000 の理由）。

## 付録 B: テスト一覧と fixture

### B.1 fixture（`tests/fixtures/repo_kb/mini/`。実装者が作る。確認は「期待」の列）

| パス | 特徴 |
| --- | --- |
| `_features/A01-alpha.md` | 第3期の形。`\| --- \| --- \|`、`規模` が `全体 M（…）`、`依存` に注記（B.2） |
| `_features/A01-appendix.md` | 付録（H1 は `# A01 付録`） |
| `_features/_completed/B02-beta.md` | 旧形: `\|---\|---\|`、H1 の直後に表、`状態` が `todo`、`主な影響ファイル（実装時）` |
| `_features/_completed/C03-gamma.md` | `状態` が `done（Phase 1・2）`、`規模` が `L（Phase 1 は M）`、`## 確定した仕様`、`依存` が `A01, 弱依存 B02` |
| `_perfs/PB04-delta.md`・`_perfs/_completed/PX05-eps.md` | `分類`・`関連`・`計測対象` の行 |
| `_features/GUIDE.md` | §4.1 の表 1 つと `### D-01`〜`### D-03`（D-02 は表を本文に持つ。**フェンスの中に `### D-99 fake`** も入れる） |
| `_features/GUIDE-LITE.md` | 正しい参照だけ（`scripts/repo-kb.mjs`・`tests/alpha.rs`・`E1012`・`D-02`・`§4.1`・`A01`・`Checker::value_expression`） |
| `_features/README.md`・`notes.md` | 前者は対象外、後者は `file_skipped` |
| `docs/architecture.md` | 表 `\| ファイル \| 責務 \|`（B.3 の 4 行） |
| `src/check.rs`・`src/lib.rs` | B.4。`src/lib.rs` は `pub mod check;` だけ |
| `tests/alpha.rs`（`#[test]` 2）・`tests/run.mjs`（`argv[2]` を使う）・`tests/helper.mjs`（`run.mjs` が import）・`tests/features.mjs`（suite `storage`（`input: "storage/Storage.tz"`）と `vec`） | `tests/fixtures/storage/Storage.tz`・`tests/fixtures/vec/Vec.tz` も置く |
| `_tsuzuri/language-reference/index.md`・`ownership/rules.md` | 後者にフェンスの中の `# comment` と重複見出しを入れる。`compiler/diagnostics.md` に診断コードの表の行 `E1012`（意味: ムーブ後の使用） |
| `README.md` | 見出し 1 つ |

### B.2 `A01-alpha.md`（抜粋。構造の全体）

````markdown
# A01: Alpha feature

| 項目 | 内容 |
| --- | --- |
| ID | A01 |
| 優先度 | P1 |
| 規模 | 全体 M（Phase 1: S、Phase 2: S） |
| 依存 | B02（done）、(C03)。C03 は弱い依存: 先に入れると速い。[X99](X99.md) は別件 |
| 後続 | D04 |
| 状態 | todo |
| 主な影響ファイル | `src/check.rs`・`src/new.rs`（新規）, `tests/alpha.rs`, `src/*.rs`, `tests/fixtures/storage/` |

## 実装者向け早見
早見の本文。 D-02 😀
## 着手条件と停止条件
### 停止条件
- `E1012` が変わったら止める。
## 仕様
共通。D-01〜D-03。
### Phase 1: 基本
Phase 1 の仕様。
### Phase 2: 拡張
Phase 2 の仕様。
### 共通の注意
どの Phase にも要る。
## 設計
設計の本文。
## 実装手順
前文。
### Phase 1: 土台
**手順 1: 準備。**
### 手順の補足
Phase 1 に属す。
### Phase 2: 仕上げ
**手順 2: 仕上げ。**
## テスト計画
`tests/alpha.rs`・suite `storage`。
## 受け入れ条件
- [ ] 全部。
````

期待（`parseTicket`。fixture に `src/check.rs`・`src/lib.rs`・`tests/alpha.rs`・`tests/fixtures/storage/Storage.tz` がある前提）: `deps` = `[{B02,false},{C03,true}]`（`X99` は注記の中なので入らず、`dep_unknown` も出ない）、`size` = `M`・`size_note` = セルの全文、`files` = `[src/check.rs✓, src/new.rs✗, tests/alpha.rs✓, src/lib.rs✓, tests/fixtures/storage/✓]`（`src/*.rs` の glob は一致を昇順に並べ、重複の `src/check.rs` を省く）、`refs.tickets` = `[B02, C03]`（自分自身と、索引にない `D04`・`X99` を除く）、`refs.decisions` = `[D-01, D-02, D-03]`、`refs.diags` = `[E1012]`、`refs.files` = `[src/check.rs, tests/alpha.rs, tests/fixtures/storage/]`、`headings` はレベル 2〜3 だけ。`steps --phase 1` は「前文」「Phase 1: 土台」「手順の補足」を含み「Phase 2: 仕上げ」を含まない。`spec --phase 1` は「Phase 1: 基本」と「共通の注意」を含み「Phase 2: 拡張」を含まない（`[Phase 2 omitted: Phase 2: 拡張 (<n> chars)]` が 1 行残る）。

### B.3 `docs/architecture.md` の表（4 行）

```markdown
| ファイル | 責務 |
|---|---|
| `src/check.rs` | 型検査 |
| `src/control.rs` / `recursion.rs` | 制御構文と再帰検査 |
| `src/runtime/string.ll` / `heap-*.ll` / `utf8string.ll` | ランタイム |
| `std/` / `src/missing.rs` | 標準ライブラリ |
```

期待: 行 2 の `files` = `src/control.rs`・`src/recursion.rs`（後者は `src/` を引き継ぐ。fixture に両方を置く）。行 3 は `src/runtime/string.ll`・`src/runtime/heap-wasm.ll`・`src/runtime/heap-native.ll`（glob の一致を昇順）・`src/runtime/utf8string.ll`。行 4 は `dirs` = `std/`、`missing` = `src/missing.rs`、警告 `table_path_missing src/missing.rs (docs/architecture.md table 1)`。

### B.4 `src/check.rs`（45 行。`diagSites` と `publicItems` の期待つき）

```rust
use std::fmt;

/* outer /* nested "E1001" */ still a comment */
const RAW: &str = r#"E1004"#;
const NOTE: &str = "fn fake() { Diagnostic::new(\"E1002\") }";
const BRACE: char = '{';

pub struct Checker<'a> {
    source: &'a str,
}

impl<'a> Checker<'a> {
    pub fn value_expression(&self) -> Result<i64, Diagnostic> {
        let closer = '}';
        let _ = (closer, b"E1005");
        Err(Diagnostic::new("E1012", "use of moved value", 0))
    }

    fn private_helper(&self) -> &'a str {
        self.source
    }
}

pub trait Rule {
    fn check(&self) -> bool {
        warn("W1006")
    }
}

pub(crate) fn warn_copy() -> Diagnostic {
    Diagnostic::warning("W1006", "implicit copy", 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks() {
        assert_eq!(Checker { source: "" }.value_expression().unwrap_err().code, "E1999");
    }
}

pub fn after_tests() -> &'static str {
    "E1013"
}
```

期待（試作で確認）。`diagSites`（`test` でないもの。`{code, line, function}`）: `E1004` 4 `null`、`E1005` 15 `Checker::value_expression`、`E1012` 16 `Checker::value_expression`、`W1006` 26 `Rule::check`、`W1006` 31 `warn_copy`、`E1013` 45 `after_tests`。除かれるもの: `E1001`（コメント）、`E1002`（長い文字列の一部）、`E1999`（`test` 領域。`test: true` で返る）。`publicItems`: `L8 pub struct Checker<'a> {`・`L13 pub fn value_expression(&self) -> Result<i64, Diagnostic> {`・`L24 pub trait Rule {`・`L30 pub(crate) fn warn_copy() -> Diagnostic {`・`L44 pub fn after_tests() -> &'static str {`（`private_helper`・トレイトの `fn check` は `pub` でないので入らない）。

### B.5 テストの一覧（`tests/repo_kb.mjs`。名前・入力・期待。期待は決定表と fixture から手で書く）

| # | 名前 | 入力 | 期待 |
| --- | --- | --- | --- |
| 1 | `markdown_headings_ignore_fenced_lines` | 8 行: `# A`、3 連バッククォートで開閉した中の `## not`、チルダ 3 連で開閉した中の `### also`、`## B` | 見出しは `A`（レベル 1・1 行目・`end` 7）と `B`（レベル 2・8 行目）だけ |
| 2 | `markdown_table_splits_on_unescaped_pipes_outside_code` | 1 行の表で、セルが `a \| b`（エスケープ）・`` `x|y` ``（バッククォートの中の `|`）・`c` | 3 セル `a | b`・`` `x|y` ``・`c` |
| 3 | `slug_matches_check_docs` | 見出し `ムーブ後の使用エラー（E1012）`・`Phase 1: 差分`・`X`・`X` | `ムーブ後の使用エラーe1012`・`phase-1-差分`・`x`・`x-1`。さらに実ページ 3 つで `scripts/check-docs.mjs` の `headings` と同じ集合 |
| 4 | `list_files_falls_back_to_walk` | fixture の一時コピー（`.git` なし） | `listFiles` が `target/`・`.git/` を除いた昇順の一覧 |
| 5 | `rust_scan_masks_comments_strings_and_chars` | B.4 | `code` の長さ = 元の長さ、`code` に `E1001`・`E1002`・`fn fake` がない |
| 6 | `rust_scan_handles_raw_strings_and_lifetimes` | `` let a = r##"x"#y"##; let b: &'a str = ""; `` | 文字列リテラル 2 つ（値 `x"#y` と空）。`'a` はそのまま `code` に残る |
| 7 | `rust_diag_sites_skip_cfg_test_modules` | B.4 | `E1999` が `test: true`、除くと 6 か所 |
| 8 | `rust_diag_sites_name_impl_and_fn` | B.4 | B.4 の期待の `function` と `line` |
| 9 | `rust_public_items_list_signature_lines` | B.4 | B.4 の期待の 5 行 |
| 10 | `ticket_classification_by_file_name` | `README.md`・`GUIDE.md`・`GUIDE-LITE.md`・`A01-alpha.md`・`A01-appendix.md`・`A01-appendix-more.md`・`notes.md` | 対象外・対象外・対象外・チケット `A01`・`A01` の付録・`A01` の付録・警告 `file_skipped` |
| 11 | `ticket_meta_table_both_separator_forms` | A01（`\| --- \| --- \|`）と B02（`\|---\|---\|`） | どちらも `id`・`priority` を得る |
| 12 | `ticket_meta_table_without_blank_line_after_title` | B02（H1 の直後）と、H1 と表の間に段落があるチケット | どちらも表を見つける。B02 の `主な影響ファイル（実装時）` が `files` になる |
| 13 | `ticket_size_and_state_with_notes` | `全体 M（…）`・`XL（Phase 1 は L）`・`L（Phase 1 は M）`、`done（Phase 1・2）` | `size` が `M`・`XL`・`L`、`size_note` 全文。`state_cell` が `done` |
| 14 | `ticket_state_conflict_is_reported_and_location_wins` | B02（`_completed/` で `todo`） | `state: "done"`、`state_cell: "todo"`、警告 `state_conflict B02: …` |
| 15 | `deps_table_forms` | 本文 T3a の表の全行（HEAD の実例の文字列をそのまま） | 各行の「結果」の列 |
| 16 | `files_cell_forms` | 本文 T3b の表の全行 | 各行の分類と `exists` |
| 17 | `ticket_sections_roles_and_aliases` | A01 と C03（`確定した仕様`） | `spec` が A01 は `仕様`、C03 は `確定した仕様`。`quick` が C03 にはない |
| 18 | `ticket_headings_index_has_levels_lines_chars` | A01 | レベル 2〜3 だけ。`line`・`end`・`chars` が手で数えた値（絵文字は 1 字） |
| 19 | `mentions_filter_unknown_ids` | `A01 A64 D-01〜D-03 E1012 src/check.rs src/missing.rs` | `tickets [A01]`・`decisions [D-01,D-02,D-03]`・`diags [E1012]`・`files [src/check.rs]` |
| 20 | `decisions_parse_and_ranges` | GUIDE fixture | D-01〜D-03 の行、`D-01〜D-03` の言及が 3 件に展開 |
| 21 | `decisions_skip_fenced_headings` | GUIDE fixture（フェンスの中の `### D-99 fake`） | D-99 がない |
| 22 | `module_tables_resolve_relative_names_and_globs` | B.3 | B.3 の期待 |
| 23 | `tests_index_rust_mjs_and_suites` | fixture の `tests/` | `alpha` の `count` 2、`run.mjs` の `compiler_arg` 真、`helper.mjs` の `helper` 真・`command` 空、suite `storage`（`fixture` = `tests/fixtures/storage/Storage.tz`）と `vec` |
| 24 | `docs_index_headings_and_anchors` | `ownership/rules.md` | フェンスの中の `# comment` は見出しにならない。重複見出しの anchor が `x`・`x-1` |
| 25 | `diagnostics_index_merges_sites_docs_and_mentions` | fixture 全体 | `E1012` の `site_count` 1・`meaning` が表の行・`tickets` に `A01`・`decisions` に `D-02` |
| 26 | `build_writes_files_in_fixed_key_order` | fixture を `build` | 8 ファイル。`tickets.jsonl` の 1 行目のキーの順が「出力ファイルとキー」と一致 |
| 27 | `build_is_byte_identical_twice` | 2 回 `build` | 全ファイルが同じ |
| 28 | `build_refuses_foreign_directory` | `--out` が `x.txt` だけの空でないディレクトリ | 終了コード 3 |
| 29 | `verify_detects_stale_input` | `build` → fixture の 1 文字を変更 → `verify` | 終了コード 1 と `stale` |
| 30 | `build_real_repo_invariants` | 実リポジトリ | チケット ≥ 125・決定 ≥ 58・`id` 一意・全 `deps` 実在・`warnings` の slug が `state_conflict` のみ・README の Mermaid の全辺が再現される（A.1）・件数が「現状」と矛盾しない |
| 31 | `ticket_lookup_text_format` | `ticket A01` | 本文の表の各行 |
| 32 | `decision_lookup_by_number_forms` | `D-02`・`d-02`・`2`・`02` | 同じ出力。`D-99` は終了コード 1 |
| 33 | `files_lookup_matches_cell_and_mentions` | `files src/check.rs`・`files src/` | A01 が `cell`、GUIDE の D-02 が `mention`。末尾 `/` は前方一致 |
| 34 | `diag_lookup_lists_sites_and_mentions` | `diag E1012`・`diag E9999` | 箇所 `src/check.rs:Checker::value_expression L16`。`E9999` は終了コード 1 |
| 35 | `search_matches_all_words_case_insensitive` | `search ALPHA feature`・`search zzz` | A01 の 1 行。`(no matches)` と終了コード 0 |
| 36 | `section_command_prints_exact_section` | `section A01 spec`・`section A01 設計`・`section A01 nope` | 区切り行と本文。最後は終了コード 1 |
| 37 | `phase_filter_sticky_in_steps` | B.2 | `steps --phase 1` は B.2 の期待 |
| 38 | `phase_filter_non_sticky_in_spec` | B.2 | `spec --phase 2` は「Phase 2: 拡張」と「共通の注意」を含み「Phase 1: 基本」を含まない |
| 39 | `brief_contains_required_parts_in_order` | `brief A01` | 区切り行が表の順（`REPO-KB BRIEF`・`HOW TO USE`・`CONTENTS`・`TICKET A01 / quick`…`END`） |
| 40 | `brief_respects_max_chars_for_every_fixture_ticket` | fixture の全チケット × `B` が 16000・24000・60000 | `cp(出力) <= B` |
| 41 | `brief_truncation_stages_follow_the_table` | 長い仮のチケット（`spec` が 20,000 字）と `B = 14000` | `spec` が上限以下で `[shortened:` の行がある。`B = 60000` では `[shortened` がない |
| 42 | `brief_no_lite_option` | `brief A01 --no-lite` | `GUIDE-LITE` の区切り行がなく、短い |
| 43 | `brief_is_deterministic` | 2 回 | 同じ |
| 44 | `brief_rejects_too_small_max_chars` | `--max-chars 15999`・`--max-chars x` | 終了コード 2 |
| 45 | `brief_real_repo_all_tickets_default_and_16000` | 実リポジトリの全チケット | すべて `B` 以下・2 回で同じ |
| 46 | `guide_lite_check_accepts_fixture` | fixture の GUIDE-LITE | 失敗 0 |
| 47 | `guide_lite_check_reports_each_class` | 各種に 1 つずつ誤りを足した GUIDE-LITE（C1・C1b・C2・C3・C4・C5・C6（3 種の理由）・C7（2 種）・C8（3 種）・C9） | 各 `FAIL <種別> … — <理由>`（行番号つき）。終了コード 1 |
| 48 | `guide_lite_planned_entry_fails_when_it_exists` | fixture に `guide/` を足す | `now exists; remove it from PLANNED` |
| 49 | `cli_exit_codes` | 未知のコマンド・オプション・引数不足 | 終了コード 2。`brief NOPE` は 1。`--root` が存在しない場合は 3 |
| 50 | `outputs_have_no_absolute_paths_or_timestamps` | fixture の `build` と `brief` の出力 | `process.cwd()`・一時ディレクトリの絶対パス・`\d{4}-\d{2}-\d{2}T` を含まない |
| 51 | `node_version_gate` | `process.versions.node` が 20 未満 | `repo-kb: Node.js 20 or later is required` で終了コード 3（`main` の先頭の検査。テストは検査関数に偽の版を渡す） |

## 付録 C: `.github/workflows/repo-kb.yml`

```yaml
name: Repo KB

# G32: the development knowledge base and the GUIDE-LITE drift check. Only Node and git are needed.

on:
  workflow_dispatch:
  pull_request:

permissions:
  contents: read

concurrency:
  group: repo-kb-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  repo-kb:
    name: repo-kb (Node ${{ matrix.node }})
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    strategy:
      fail-fast: false
      matrix:
        node: [20, 24]
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-node@v7
        with:
          node-version: ${{ matrix.node }}
      - name: Tests of the knowledge base
        run: node tests/repo_kb.mjs
      - name: Build and verify
        run: |
          node scripts/repo-kb.mjs build --out target/repo-kb
          node scripts/repo-kb.mjs verify --out target/repo-kb
      # Add this step when `node scripts/check-guide-lite.mjs` passes on main (step 11 of the ticket).
      # - name: GUIDE-LITE references
      #   run: node scripts/check-guide-lite.mjs
```

## 付録 D: `AGENTS.md` に足す節（英語。AGENTS.md の末尾）

```markdown
## For AI agents working on tickets

Do not start by reading `_features/GUIDE.md` or a whole ticket. Run
`node scripts/repo-kb.mjs brief <TICKET-ID>` first (`--phase N` for a later phase, `--max-chars N`
for a longer pack). The pack holds the ticket parts you need, the decisions it cites, the public
symbols of the files it names, the commands of its tests, and `_features/GUIDE-LITE.md`. A line that
starts with `[shortened` or `[omitted` names the command that prints the missing text, for example
`node scripts/repo-kb.mjs section G22 spec`.

Other lookups: `ticket <ID>`, `decision D-45`, `files <path>`, `diag E1012`, `search <words>`.
The index under `target/repo-kb/` is rebuilt when the sources change. If the repository contradicts
the pack or GUIDE-LITE, stop and report it; `node scripts/check-guide-lite.mjs` finds stale references.
```
