# G33 付録 D: ハーネスの詳細（トラップの種類・検証エラー・fixture・自己テスト・CI・出力例）

本文: [G33-spec-rule-ids-conformance.md](G33-spec-rule-ids-conformance.md)。ここにある表は実装の入力で、本文の「仕様」「設計」と一緒に読む。

## D.1 トラップの種類（`trap kind=K` の K と、stderr の `trap:` の行の説明）

正本は `src/trap.rs` の `TrapKind::description`（16 件）。確認: `grep -c 'Self::[A-Za-z]* =>' src/trap.rs` が 16。`TRAP_KINDS` は次の表をそのまま置く（K → 説明）。合格の条件は、stderr のどれかの行が `trap: <説明>` で始まること。

| K | `TrapKind` | 説明 |
| --- | --- | --- |
| `assert` | `Assert` | `assertion failed` |
| `div_zero` | `IntegerDivisionByZero` | `integer division by zero` |
| `div_overflow` | `IntegerDivisionOverflow` | `integer division overflow` |
| `bounds` | `BoundsCheck` | `index out of bounds` |
| `alloc_size` | `AllocationSize` | `allocation size overflow` |
| `alloc_failure` | `AllocationFailure` | `allocation failed` |
| `range_step` | `RangeStepZero` | `range step is zero` |
| `match_failure` | `MatchFailure` | `non-exhaustive match` |
| `pattern_mismatch` | `PatternMismatch` | `pattern mismatch` |
| `string_overflow` | `StringConcatOverflow` | `string length overflow` |
| `numeric_runtime` | `NumericRuntime` | `numeric runtime trap` |
| `wasm_runtime` | `WasmRuntime` | `wasm runtime trap` |
| `encoding` | `Encoding` | `invalid Unicode encoding` |
| `debug_output` | `DebugOutput` | `debug output failed` |
| `stack_overflow` | `StackOverflow` | `stack overflow` |
| `overflow` | `Overflow` | `unhandled OverflowException: arithmetic operation resulted in an overflow` |

228 件が使う K は `assert`・`bounds`・`div_overflow`・`div_zero`・`numeric_runtime`・`pattern_mismatch`・`range_step` の 7 種。残り 9 種は未使用で、`trap_table_matches_src_trap_rs`（No.16）が表と `src/trap.rs` のずれを検出する。

## D.2 ディレクティブの検証エラー（`parseTest`・`discoverTests` の `name`）

| 名前 | 条件 |
| --- | --- |
| `unknown_directive` | 末尾の `// key:` の key が `rule`・`expect`・`edition`・`targets`・`cli`・`note` のどれでもない |
| `duplicate_directive` | `edition`・`targets`・`cli` が 2 回以上 |
| `rule_format` | `// rule:` の値が §1 の形式でない（`OWN-000`・小文字・桁数違いを含む） |
| `missing_rule` | `// rule:` がない |
| `missing_expect` | `// expect:` がない |
| `unknown_expectation` | expect の kind が、そのモード（通常・cli）の表にない |
| `bad_argument` | 引数の不足・過剰・形式違反。整数でない・`line=` が 0・同じオプションの重複・未知のオプション・`file=` が絶対パスか `..` か `\` を含む・`kind=` が snake_case でない・`trap` の K が表にない・`stdout` が最後の対でない・JSON 文字列の誤り・`cli` に引用符・`edition` が `20NN` でない・`targets` が不正か重複・`deterministic` に `$OUT` がない・`E`／`W` と 4 桁でないコード |
| `mixed_expectations` | 通常モードで `trap` と `stdout` を同時に書いた |
| `targets_not_allowed` | `targets` が cli モードにある、`stdout`・`trap`・`tests-pass` のない通常モードにある、`stdout`・`trap` に `wasm-*` がある |
| `edition_conflict` | ディレクトリ形で `Tsuzuri.toml` と `// edition:` が両方ある（`discoverTests`） |
| `unexpected_entry` | §3 の「禁止」（`discoverTests`） |
| `test_name` | stem の形式・長さ・重複・領域ディレクトリとの不一致（`discoverTests`） |

`test_name`・`unexpected_entry`・`edition_conflict` は `check-spec` では名前 `test_name`／`test_directive` に入る（メッセージの先頭にこの表の名前を付ける）。

## D.3 fixture `tests/fixtures/conformance_mini/good/`（正しい最小の木。各テストが複製して壊す）

`spec/rules/OWN.md`

```
# OWN: Ownership

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| OWN-001 | A moved `string` MUST NOT be used again (E1012). | [page.md § Moves](../../docs/page.md#moves) | NUM-001-i8-wraps, OWN-001-use-after-move | active |
| OWN-002 | An old rule that no longer applies. | - | - | retired |
```

`spec/rules/NUM.md`

```
# NUM: Numerics

| ID | Rule | Doc | Tests | Status |
| --- | --- | --- | --- | --- |
| NUM-001 | An `i8` addition MUST wrap, and `a \| b` MUST be a bitwise or. | [page.md](../../docs/page.md) | NUM-001-dir, NUM-001-i8-wraps | active |
```

`tests/conformance/OWN/OWN-001-use-after-move.tz`

```
let a = "x"
let b = a
a

// rule: OWN-001
// expect: check-error E1012 line=3
```

`tests/conformance/NUM/NUM-001-i8-wraps.tz`

```
def add8 :: i8 -> i8 -> i8 = \a b -> a + b

test "wraps" = Test.equal (ref (-128)) (ref (add8 127 1))

// rule: NUM-001
// rule: OWN-001
// expect: tests-pass 1
// targets: native-O0
```

`tests/conformance/NUM/NUM-001-dir/Main.tz` と同じ場所の `Lib.tz`

```
Lib.add 1 2

// rule: NUM-001
// expect: stdout "3\n"
```

```
def add :: i64 -> i64 -> i64 = \a b -> a + b
```

`docs/page.md`（コードフェンスの中の `NUM-999` は数えない）

````
# Page

## Moves

<!-- rule: OWN-001 -->
A moved value cannot be used again.

- <!-- rule: NUM-001 -->Integer addition wraps.

```text
<!-- rule: NUM-999 -->
```
````

`_tsuzuri/language-reference/compiler/conformance.md`

```
# Conformance

<!-- generated:spec-coverage -->
| Area | Active | Retired | Tested | Tests | Marked |
| --- | ---: | ---: | ---: | ---: | ---: |
| LEX | 0 | 0 | 0 | 0 | 0 |
| SYN | 0 | 0 | 0 | 0 | 0 |
| TYP | 0 | 0 | 0 | 0 | 0 |
| OWN | 1 | 1 | 1 | 1 | 1 |
| EVL | 0 | 0 | 0 | 0 | 0 |
| NUM | 1 | 0 | 1 | 2 | 1 |
| STR | 0 | 0 | 0 | 0 | 0 |
| CTL | 0 | 0 | 0 | 0 | 0 |
| MOD | 0 | 0 | 0 | 0 | 0 |
| TSK | 0 | 0 | 0 | 0 | 0 |
| DIA | 0 | 0 | 0 | 0 | 0 |
| CLI | 0 | 0 | 0 | 0 | 0 |
| ABI | 0 | 0 | 0 | 0 | 0 |
| STD | 0 | 0 | 0 | 0 | 0 |
| EDN | 0 | 0 | 0 | 0 | 0 |
| Total | 2 | 1 | 2 | 3 | 2 |
<!-- /generated:spec-coverage -->
```

`good/` で `check-spec --require-markers` は終了コード 0、`conformance.mjs --root good --targets native` は 3 件 ok になる（`wasm-ld` は要らない。`tests-pass` の `targets:` が `native-O0` だけ）。

## D.4 自己テスト `tests/conformance_harness.mjs`（45 件。入力は文字列、期待は手で書いた値）

`mutate(file, from, to)` は `good/` を一時ディレクトリへ複製し、そのファイルの文字列を 1 か所置換した木の場所を返す。「木」は `--root` で指す。

| No. | 名前 | 入力 | 期待 |
| --- | --- | --- | --- |
| 1 | `footer_splits_directives_from_body` | `"let x = 1\nx\n\n// rule: OWN-001\n// expect: check-ok\n"` | `start` が 2。directives が `rule`（値 `OWN-001`・行 4）と `expect`（値 `check-ok`・行 5） |
| 2 | `footer_stops_at_plain_comment_or_code` | `"x\n// rule: OWN-001\n// done\n"`、`"x\n// expect: check-ok\ny\n"`、`"x\n// foo: 1\n"` | 1 つ目は `start` 3・directives なし。2 つ目も directives なし。3 つ目は `foo` の directive 1 件 |
| 3 | `tokens_words_and_json_strings` | `stdout "a b\n" x`（`\n` は文字 2 つ） | 語 `stdout`、文字列 `a b` + 改行、語 `x` |
| 4 | `tokens_reject_bad_strings` | `"abc`、`"a"b`、`"\x"` | それぞれエラー（unterminated／glued／invalid JSON） |
| 5 | `parse_check_options` | `check-error E1012 line=3 file=Lib.tz kind=use_after_move`。拒否: `line=1 line=2`・`line=0`・`file=../x`・`file=/x`・`check-error W1001`・`check-warning E1012`・`check-error E12` | 1 つ目は code・line 3・file・diagKind。拒否は `bad_argument` |
| 6 | `parse_trap_with_stdout_tail` | `trap kind=div_zero stdout "1\n"`。拒否: `trap kind=nope`・`trap`・`trap kind=bounds stdout`・`trap stdout "x" kind=bounds` | 1 つ目は trapKind `div_zero`・stdout `1` + 改行。拒否は `bad_argument` |
| 7 | `parse_cli_mode` | `cli: check $DIR --json` と `exit 1`・`stderr-has "x"`・`file-has out.h "y"`・`stderr-lines 1`。拒否: cli と `check-ok`・`deterministic`（`$OUT` なし）・`cli: a "b"` | 1 つ目は expects 4 件。拒否は `unknown_expectation`・`bad_argument`・`bad_argument` |
| 8 | `parse_unknown_directive_and_expectation` | 末尾に `// foo: x`。`// expect: frobnicate` | `unknown_directive`。`unknown_expectation` |
| 9 | `parse_mixed_modes_and_trap_stdout` | `// expect: trap kind=assert` と `// expect: stdout "x"` | `mixed_expectations` |
| 10 | `parse_targets_rules` | `targets: native-O0 wasm-O0` と `stdout`／と `tests-pass 1`、`targets: native-O0` と `check-ok`／と cli、`targets: native-O0 native-O0`、`targets: x86` | `targets_not_allowed`／ok／`targets_not_allowed`／`targets_not_allowed`／`bad_argument`／`bad_argument` |
| 11 | `parse_requires_rule_and_expect` | 末尾が `// note: x` だけ | `missing_rule` と `missing_expect` の両方 |
| 12 | `parse_rule_id_format` | `OWN-000`・`own-001`・`ABC-001`・`OWN-1`。`OWN-001` と `NUM-002` の 2 行 | 前 4 つは `rule_format`。後は rules が `[OWN-001, NUM-002]` |
| 13 | `match_diagnostic_fields` | 診断 `{"severity":"error","code":"E1012","message":"m","path":"/t/Main.tz","span":{"line":5}}` に、コードだけ・`line=5`・`line=4`・`file=Main.tz`・`file=Lib.tz`・`kind=x`（診断に `kind` なし）・`kind=x`（診断に `"kind":"x"`） | 合・合・否・合・否・否・合 |
| 14 | `match_diagnostic_default_file` | 診断の `path` が `/t/Lib.tz`・行 2 に、`line=2`・`line=2 file=Lib.tz`・コードだけ。`path` が `C:\t\Lib.tz` で `file=Lib.tz` | 否・合・合・合 |
| 15 | `trap_line_parsing` | `trap: index out of bounds at /t/Main.tz:3:1`、前に別の行がある stderr、`trap:` がない stderr、`trap: stack overflow at C:\t\Main.tz:3:1` | `{description, path, line 3, column 1}`、同じ、`null`、path が `C:\t\Main.tz` |
| 16 | `trap_table_matches_src_trap_rs` | `src/trap.rs` から正規表現 `/Self::([A-Za-z]+) =>\s*\{?\s*"([^"]+)"/g` で `(Variant, 説明)` を 16 組読む（`Overflow` だけ、説明が波括弧の中の次の行にある）と、`TRAP_KINDS` | 16 組すべて、`TrapKind` の名前と説明が表と一致 |
| 17 | `test_summary_parsing` | `test --json` の行（`test` 2 行と `summary`、`duration_ms` つき）と、壊れた行・summary なし | `{passed, failed, ignored}`。壊れた行は無視。summary なしは `null` |
| 18 | `discover_file_and_directory_forms` | `good/` | 3 件。stem `NUM-001-dir`（dir）・`NUM-001-i8-wraps`（file）・`OWN-001-use-after-move`（file）。昇順 |
| 19 | `discover_rejects_bad_entries` | `good/` に `tests/conformance/OWN/readme.txt`、`tests/conformance/FOO/x.tz`、`OWN-001-Bad_Name.tz`、`.DS_Store`、`NUM-001-i8-wraps.tz` を `OWN/` にも、`NUM-009-x.tz` を `OWN/` に | `unexpected_entry`・`unexpected_entry`・`test_name`・（無視）・`test_name`（重複）・`test_name`（領域違い） |
| 20 | `run_check_expectations` | 小さい `.tz` を 4 つ（本体のあとにディレクティブ）: `check-ok`（`let x = 1` と `x`）、`check-error E1012 line=3`（OWN の fixture の本体）、`check-warning W1001 line=2`（`def f :: unit -> i64 = \() -> {` の中に `let unused = 1;` と `2`、`}`、`f ()`）、`check-error E1012 line=2`（誤り） | ok・ok・ok・fail |
| 21 | `run_failure_reports_detail` | `check-error E1003` を E1012 の本体に。`--timeout 0.01` で OWN の fixture | detail に `E1003` と `got E1012`。タイムアウトは `timed out after 0.01 s` で fail |
| 22 | `run_stdout_and_trap` | `1 + 2` に `stdout "3\n"`。`let zero = 0` と `1 / zero` に `trap kind=div_zero line=2`。付録 B.3 の EVL-019 の本体に `trap kind=div_zero stdout "1\n"` | すべて ok（native `-O0`・`-O3`） |
| 23 | `run_tests_pass_native` | `good` の `NUM-001-i8-wraps`。`tests-pass 2` に直したもの | ok。`passed=1 != 2` で fail |
| 24 | `run_cli_expectations` | `cli: check $DIR --json` と `exit 1`・`stderr-has "\"code\":\"E1012\""`・`stdout-empty`・`stderr-lines 1`（本体は OWN の fixture）。付録 C.8 の ABI-002 の本体に `cli: build $DIR --emit header -o $DIR/out.h` と `exit 0`・`file-has out.h "#pragma once"`・`file-lacks out.h "hidden"` | ok |
| 25 | `run_deterministic` | `cli: build $DIR --emit llvm -o $OUT` と `deterministic` | ok |
| 26 | `run_edition_manifest` | `// edition: 2026` と `check-ok`。`// edition: 2030` と `check-error E0002`。ディレクトリ形に `Tsuzuri.toml` と `// edition:` | ok・ok・`edition_conflict`（終了コード 2） |
| 27 | `cli_selection_and_exit_codes` | `good/` に `--area OWN`・`--rule OWN-001`・`--rule NUM-001`・`--filter dir`・`--list`・`--area NUM --filter zzz`・`--bogus`。失敗するテストを入れた木。全部 ok の木 | 選択 1・2・2・1 件。`--list` が `stem<TAB>rules<TAB>kinds`。0 件は終了コード 2。未知のオプションは 2。失敗は 1。全部 ok は 0 |
| 28 | `cli_json_output` | `--json` で `good/`（`--targets native`）を 2 回 | `test` の行が stem の昇順、最後が `summary`。2 回の出力が同じ |
| 29 | `wasm_probe_failure_message` | 環境変数 `TSUZURI_WASM_LD=/nonexistent` で、`targets:` のない `tests-pass` のテストを 1 つ持つ木を `--targets all` と `--targets native` で | `all` は終了コード 2 で、stderr に `--targets native` を含む。`native` は ok |
| 30 | `output_has_no_absolute_paths` | 失敗する木の text と `--json` の出力 | `tsuzuri-conformance-` と一時ディレクトリの絶対パスを含まない（`$DIR` に置換）。2 回で同じ |
| 31 | `spec_reads_tables_and_unescapes_pipes` | (a) `good/` の `NUM.md` を `readRules`。(b) `spec/rules/` を消した木に `check-spec` | (a) Rule が `` An `i8` addition MUST wrap, and `a | b` MUST be a bitwise or. ``（`\|` が `|` に戻る）。行番号 5。(b) 終了コード 2 で、stderr が `check-spec: spec/rules/ not found` |
| 32 | `spec_id_checks` | `mutate(OWN.md, "OWN-001 |", "OWN-1 |")`、`"OWN-001 |"` → `"NUM-001 |"`、`"OWN-002 |"` → `"OWN-001 |"`、`OWN-001` と `OWN-002` の行を入れ替え | `rule_id`・`rule_id`（領域違い）と `rule_duplicate`・`rule_duplicate`・`rule_order` |
| 33 | `spec_sentence_checks` | OWN-001 の文を、`.` なし・`MUST` なし（`SHOULD` に）・241 字・`MAY` に | `rule_sentence`・`rule_sentence`・`rule_sentence`・ok |
| 34 | `spec_doc_link_checks` | OWN-001 の Doc を、`-`・リンク 2 つ・`../../docs/missing.md`・`../../docs/page.md#nope`・`../../src/lib.rs`・`../../docs/page.md` | `doc_link`×5 と、最後は ok（`#` は省略できる）。`retired` の `-` は ok |
| 35 | `spec_tests_cell_and_write` | Tests のセルを `-` に。`--write` を実行 | `tests_cell` のエラー（期待の値を含む）。`--write` の後に元の文字列と同じ（ほかの文字が 1 byte も変わらない）。2 回目の `--write` は何も書かない |
| 36 | `spec_untested_and_unknown_rule` | (a) `OWN-001-use-after-move.tz` を削除。(b) `NUM-001-dir/Main.tz` の `// rule: NUM-001` を `// rule: NUM-777` に。(c) `NUM-001-dir` と `NUM-001-i8-wraps` の両方を削除 | (a) `tests_cell` 1 件（OWN-001。`NUM-001-i8-wraps` が 2 つ目の `// rule:` で指すので `rule_untested` は出ない）。(b) `test_rule` 2 件（NUM-777 が表にない・最初の ID が stem と違う）と `tests_cell` 1 件（NUM-001）。(c) `rule_untested`（NUM-001）と `tests_cell` 2 件（NUM-001・OWN-001） |
| 37 | `spec_retired_rules` | (a) `OWN-001-use-after-move.tz` に `// rule: OWN-002` を足す。(b) OWN.md の OWN-001 の Status を `retired` に | (a) `test_rule`（OWN-002 は retired）・`rule_retired_tested`（OWN-002）・`tests_cell`（OWN-002 の `-` が `OWN-001-use-after-move` になる）。(b) `rule_retired_tested`（OWN-001）と `test_rule` 2 件（OWN-001 を指す 2 つのテスト）。Tests のセルは正しいので `tests_cell` は出ない |
| 38 | `spec_test_name_and_primary_rule` | `NUM-001-i8-wraps.tz` の最初の `// rule:` を `OWN-001` にして 2 つ目と入れ替える。65 KiB のテスト | `test_rule`（最初の ID が stem と違う）。65 KiB は `test_name` |
| 39 | `spec_markers` | (a) `docs/page.md` の `<!-- rule: OWN-001 -->` を `<!-- rule: OWN-777 -->` に。(b) NUM-001 のマーカー `<!-- rule: NUM-001 -->` を消して、`--require-markers` あり・なし。(c) OWN.md の OWN-001 を `retired` にして OWN-001 のマーカーを残す | (a) `marker_rule`（OWN-777）。(b) なしは 0 件、ありは `marker_missing`（NUM-001）。(c) `marker_rule`（retired）と、OWN-001 を指す 2 テストの `test_rule` と `rule_retired_tested` |
| 40 | `spec_coverage_formats` | `good/` に `--coverage`、`--coverage-markdown`、`--coverage --json`（それぞれ 2 回） | text は D.6 の形（領域 15 行 + `Total`）で D.3 の数。markdown は D.3 のブロックの中身（`\| Area \|` の行から `\| Total \|` の行まで）と同じ。JSON は `{"schema":"tz-spec-coverage/1","areas":[…15 件],"total":{"active":2,"retired":1,"tested":2,"tests":3,"marked":2}}`。2 回で同じ |
| 41 | `spec_link_command` | `--link docs/page.md Moves`、`--link docs/page.md`、`--link docs/page.md Nope`、`--link docs/none.md` | `[page.md § Moves](../../docs/page.md#moves)`、`[page.md](../../docs/page.md)`、終了コード 1、終了コード 1 |
| 42 | `spec_generated_block` | `conformance.md` の `\| NUM \| 1 \| 0 \| 1 \| 2 \| 1 \|` を `\| NUM \| 9 \| 0 \| 1 \| 2 \| 1 \|` に。`--write` | `coverage_block` 1 件。`--write` の後に元に戻り、エラー 0。ページを消した木ではこの検査が出ない |
| 43 | `slug_matches_check_docs` | 実ページ 3 つ（`ownership.md`・`docs/language.md`・`README.md`） | `headingsOf` の集合が、`scripts/check-docs.mjs` の `headings` を同じ式で評価した集合と同じ（テストの中に式を 1 つ複製して比べる） |
| 44 | `spec_strip_markers` | `docs/page.md` に `--strip-markers` | マーカーの行が消え、`- Integer addition wraps.` になる。フェンスの中の `NUM-999` は残る |
| 45 | `real_tree_is_clean` | 実リポジトリで `node scripts/check-spec.mjs` | 終了コード 0（Phase の進み具合で規則の数は変わるが、エラーは 0） |

## D.5 CI

`.github/workflows/targets.yml`（`jobs.targets.steps`。「End-to-end compiler tests」の次に足す。step の形は既存と同じ）:

```yaml
      - name: Language conformance suite
        if: ${{ !cancelled() && steps.bundle.outcome == 'success' }}
        run: |
          export TSUZURI_CLANG="$LLVM_PREFIX/bin/clang"
          export TSUZURI_WASM_LD="${TSUZURI_WASM_LD:-$LLVM_PREFIX/bin/wasm-ld}"
          node scripts/conformance.mjs target/release/tsuzuri --jobs 2
          node tests/conformance_harness.mjs target/release/tsuzuri
```

同じファイルの `on.pull_request.paths` に `spec/**`・`scripts/conformance.mjs`・`scripts/conformance/**`・`scripts/check-spec.mjs` を足す（`tests/**` は既にある）。所要時間: 試作で 228 件が `--jobs 4` で 3 分 42 秒〜7 分 6 秒（3 回）。遅い runner（`macos-15-intel`）では 2 倍を見込み、`--jobs 2` なら 10〜15 分になりうる。Phase 4 の完了報告に実測を書く。

`.github/workflows/spec.yml`（新規。コンパイラを使わない）:

```yaml
name: Tsuzuri spec rules

# G33: the rule tables, the directives of the conformance tests and the document markers. No compiler is needed.
on:
  workflow_dispatch:
  pull_request:
    paths:
      - spec/**
      - tests/conformance/**
      - scripts/check-spec.mjs
      - scripts/conformance/**
      - docs/**
      - _tsuzuri/**
      - README.md
      - .github/workflows/spec.yml

permissions:
  contents: read

concurrency:
  group: spec-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  check-spec:
    name: check-spec (node ${{ matrix.node }})
    strategy:
      fail-fast: false
      matrix:
        node: [20, 24]
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-node@v7
        with:
          node-version: ${{ matrix.node }}
      - name: Rule tables, tests and document markers
        run: node scripts/check-spec.mjs --require-markers
```

## D.6 出力例

`node scripts/conformance.mjs target/release/tsuzuri --area OWN`（全部 ok のとき）と、1 件落ちたとき:

```
ok OWN-001-let-binding-from-non (OWN-001)
ok OWN-002-... (OWN-002)
...
conformance: 30 tests, 30 ok, 0 failed
```

```
FAIL OWN-005-example (OWN-005)
  check-error E1012 line=3: no error diagnostic matches; got E1003@Main.tz:3
ok OWN-006-... (OWN-006)
conformance: 2 tests, 1 ok, 1 failed
```

`--json`（1 行 1 件。キーの順序は固定）:

```
{"type":"test","stem":"OWN-001-let-binding-from-non","rules":["OWN-001"],"status":"ok","details":[]}
{"type":"summary","tests":30,"ok":30,"failed":0}
```

`node scripts/check-spec.mjs`（エラー 2 件のとき。エラーは stderr、最後の行は stdout）:

```
spec/rules/OWN.md:9: rule_untested: OWN-005 has no test
tests/conformance/OWN/OWN-007-x.tz:12: test_directive: bad_argument: line must be >= 1
check-spec: 2 errors
```

`node scripts/check-spec.mjs --coverage`（OWN だけの Phase 1 の終わり。`Area` は左そろえ、数は右そろえ、幅は見出しと値の最大の長さ、列の間は空白 2 つ）:

```
check-spec: ok (30 active, 0 retired, 30 tests)
Area   Active  Retired  Tested  Tests  Marked
LEX         0        0       0      0       0
SYN         0        0       0      0       0
TYP         0        0       0      0       0
OWN        30        0      30     30       0
EVL         0        0       0      0       0
NUM         0        0       0      0       0
STR         0        0       0      0       0
CTL         0        0       0      0       0
MOD         0        0       0      0       0
TSK         0        0       0      0       0
DIA         0        0       0      0       0
CLI         0        0       0      0       0
ABI         0        0       0      0       0
STD         0        0       0      0       0
EDN         0        0       0      0       0
Total      30        0      30     30       0
```
