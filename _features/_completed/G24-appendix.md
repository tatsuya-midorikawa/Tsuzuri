# G24 付録: 診断コードごとの判断、検証済みの試作、実測と参照出力、テストの入力と期待値

[G24](G24-tsuzuri-fix.md) の付録。ここに書いた実測は、HEAD `a2313d19`（作業ツリー `9dd0fbbc`）の `target/release/tsuzuri`、Apple M1 Max、macOS 27.0.1、rustc 1.98.1（試作の `rustc --edition 2024`）で 2026-10-18 に実行した結果。

## 付録 A: HEAD の診断コードごとの `fix` の判断

`grep -rhoE '"(E|W)[0-9]{4}"' src --include=*.rs | sort | uniq -c`（テストのコードを含む）で見つかるコードを、出現の多い族ごとにまとめた。**供給する**のは `W1001` と `W1005` の 2 つだけ。ほかは「診断が直す範囲を持たない」か「直し方が意図の判断」で、機械的な書き換えにならない。

| コード | HEAD の内容 | `fix` | 理由・将来 |
| --- | --- | --- | --- |
| `E0001`・`E0002`・`E0003` | 字句・構文・ソースの大きさ | しない | 直し方が一意でない。構文の誤りのヒントは G23 のヒント DB（`help` と、可能なら `fixes`）が持つ |
| `E1001`〜`E1010` | 名前・型シグネチャ・型の不一致・演算子・呼び出し・フィールド・ABI・リテラル・レイアウト | しない | 意図の判断が要る。`E1003` の `expected`/`found` は G22 Phase 3 |
| `E1002`・`E1004` | 未知の値・型（HEAD では未知の値は `E1002`、型は `E1004`） | しない | 近い名前の候補と `maybe` の fix は G25 Phase 4（`--applicability maybe --dry-run` で見る） |
| `E1011` | モジュール名・予約名・パス・シンボリックリンク | しない | ファイル名の変更は `edits`（テキストの編集）で表せない |
| `E1012`〜`E1014` | 移動後の使用・寿命・借用の競合 | しない | 複製か借用かの選択は意図。`related` と `help` は G22 Phase 2 |
| `E1015`〜`E1018`、`E1020`、`E1022`〜`E1028` | 型変数・型クラス・資源上限・ファイル種別・パターン・可視性・宣言・導出・定数・条件付きインスタンス・`dyn` | しない | 意図の判断、または直す箇所が診断の位置と別 |
| `E1019` | `rec` の欠落（自己再帰の関数に `def rec` がない） | しない | 診断は最初の 1 件だけ（`src/recursion.rs` の `check_graph` が `Err` を 1 つ返す）。相互再帰は `and` への構造の変更が要る。自己再帰の 1 関数だけなら、関数名の前に `rec ` を挿入する `certain` の fix にできる（別チケット）。余分な `rec` は受理される（診断なし） |
| `E1021` | `match` の網羅 | しない（G22 Phase 3） | 足りない腕の追加は `maybe` |
| `E2000`〜`E2007` | CLI・I/O・リンカー・実行時・テスト・依存 | しない | ソースの編集で直るものでない |
| `W1001` | 未使用のローカル・引数・パターンの束縛 | **する（`certain`）** | 位置は識別子だけ（`local.span`）。`_` を前に付ける。名前の衝突を避ける条件つき（G24 の「Phase 2: fix の供給」） |
| `W1002` | 未使用の private の関数・型 | しない | 診断の位置は名前だけで、宣言全体（doc コメント・属性を含む）の範囲を持たない。消すのは意図の判断 |
| `W1003` | 到達しない `match` の腕 | しない | 位置はパターンだけ（実測: `\| 0 -> 2` は `0` の 1 文字。「現状」4）で、腕全体の範囲がない。`MatchArm.span`（`src/syntax.rs`）を腕全体にするか別の範囲を持たせれば `certain`（腕の削除）にできる（別チケット） |
| `W1004` | 紛らわしいシャドーイング（既定は無効） | しない | 名前の変更は意図の判断 |
| `W1005` | 非推奨の宣言の使用 | **する（`likely`、条件つき）** | 注記が `use {path} instead` の形で、同じモジュールの同じ署名の関数のときだけ |
| `W1006` | 長さに比例する暗黙の複製（`--warn implicit-copy`） | しない | 明示の複製にするか、借用にするかは利用者の判断（A15） |
| `W2001`・`W2002` | ツールの警告・bindgen の省略 | しない | ソースの編集で直るものでない |
| `W1007`（A22） | 非正準な表記 | A22 が付ける（`certain`） | `tsuzuri fix --edition` が適用する |
| `E0004`（A22） | edition で拒否する表記 | しない | `W1007` の fix を先に適用する |

## 付録 B: 検証済みの試作（Myers と `set_edition`）

`rustc --edition 2024 --test proto.rs`（std だけ）で、次の 5 つのテストが通った: ① 3,000 組の乱数（種 7）で、`Equal` の数がテスト内の LCS の動的計画と一致し、`line_edits` の適用が `new` に戻る。② `unified` の出力がシステムの `diff -u` と一致する（付録 C の 6 件）。③ 変更行の上限（1,500 行 × 全部違う → `None`、900 行 × 全部違う → `Some`）。④ 閉区間の交わり。⑤ `set_edition` の 12 行（G24 本文の表）。関数名 `myers` を、チケットの `shortest_script` に改めた（それ以外は試作のまま）。

```rust
const MAX_DIFF_CHANGES: usize = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step { Equal, Delete, Insert }

/// The shortest edit script from `a` to `b` (Myers, forward greedy), one `Step` per line.
pub fn shortest_script(a: &[&str], b: &[&str]) -> Option<Vec<Step>> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let limit = ((n + m) as usize).min(MAX_DIFF_CHANGES) as isize;
    let offset = limit + 1;
    let mut v = vec![0isize; (2 * limit + 3) as usize];
    // trace[d][k + d] = the furthest x on diagonal k after round d.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for d in 0..=limit {
        let mut done = false;
        for k in (-d..=d).step_by(2) {
            let index = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x >= n && y >= m {
                done = true;
                break;
            }
        }
        trace.push(v[(offset - d) as usize..=(offset + d) as usize].to_vec());
        if done {
            return Some(backtrack(&trace, n, m));
        }
    }
    None
}

fn backtrack(trace: &[Vec<isize>], n: isize, m: isize) -> Vec<Step> {
    let mut steps = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let k = x - y;
        if d == 0 {
            while x > 0 && y > 0 {
                steps.push(Step::Equal);
                x -= 1;
                y -= 1;
            }
            break;
        }
        let previous = &trace[(d - 1) as usize];
        let at = |j: isize| previous[(j + d - 1) as usize];
        let (previous_k, step) = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            (k + 1, Step::Insert)
        } else {
            (k - 1, Step::Delete)
        };
        let previous_x = at(previous_k);
        let previous_y = previous_x - previous_k;
        // The snake of round d ends at (x, y); the move came from (previous_x, previous_y).
        let (start_x, start_y) = match step {
            Step::Insert => (previous_x, previous_y + 1),
            _ => (previous_x + 1, previous_y),
        };
        while x > start_x && y > start_y {
            steps.push(Step::Equal);
            x -= 1;
            y -= 1;
        }
        steps.push(step);
        x = previous_x;
        y = previous_y;
    }
    steps.reverse();
    steps
}

/// Sets `edition` in the `[package]` section; line oriented. `None` when there is no
/// `[package]` section or no `version` line to insert after.
pub fn set_edition(text: &str, edition: &str) -> Option<String> {
    let mut in_package = false;
    let mut offset = 0;
    // end of the `version` line, whether it ends with a newline, and which newline
    let mut version_end: Option<(usize, bool, &str)> = None;
    for line in text.split_inclusive('\n') {
        let end = offset + line.len();
        let body = line.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim_start_matches([' ', '\t']);
        if trimmed.starts_with('[') {
            in_package = trimmed.starts_with("[package]");
        } else if in_package && !trimmed.starts_with('#') && !trimmed.is_empty() {
            let key_end = trimmed
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                .count();
            let key = &trimmed[..key_end];
            let after_key = trimmed[key_end..].trim_start_matches([' ', '\t']);
            if key == "edition" && after_key.starts_with('=') {
                let value_part = after_key[1..].trim_start_matches([' ', '\t']);
                if let Some(rest) = value_part.strip_prefix('"') {
                    let close = rest.find('"')?;
                    let value_start = offset + (body.len() - rest.len());
                    let value_end = value_start + close;
                    return Some(format!("{}{}{}", &text[..value_start], edition, &text[value_end..]));
                }
                return None;
            }
            if key == "version" && after_key.starts_with('=') {
                let newline = &line[body.len()..];
                version_end = Some((end, !newline.is_empty(), if newline == "\r\n" { "\r\n" } else { "\n" }));
            }
        }
        offset = end;
    }
    let (end, has_newline, newline) = version_end?;
    let inserted = format!("edition = \"{edition}\"");
    Some(if has_newline {
        format!("{}{}{}{}", &text[..end], inserted, newline, &text[end..])
    } else {
        format!("{}{}{}", &text[..end], newline, inserted)
    })
}
```

試作の `split_lines`・`line_edits`・`unified`・`apply`・`conflict` は、本文の A2・A3・A5 と「候補の選択」の 5 の記述のとおりで、同じ試作が通した。`set_edition` の `in_package` は、最初の `[` で始まる行が `[package]` のときだけ真になる。`[package]` より前の行（コメント）は無視する。

## 付録 C: 実測と参照出力

### C1. `f1`（Phase 2 の結合テストの入力）

`f1/Main.tz`:

```text
def pair :: i64 -> i64 -> i64 = \a b -> a

let unused = 5
let x = 40
pair x 2
```

`tsuzuri check f1 --json`（標準エラー。終了コード 0）:

```text
{"severity":"warning","code":"W1001","message":"unused local 'b'; prefix it with '_' to silence this warning","path":"f1/Main.tz","span":{"start":35,"end":36,"line":1,"column":36,"end_line":1,"end_column":37}}
{"severity":"warning","code":"W1001","message":"unused local 'unused'; prefix it with '_' to silence this warning","path":"f1/Main.tz","span":{"start":47,"end":53,"line":3,"column":5,"end_line":3,"end_column":11}}
```

期待する `fix` の後（手で書いた値）の `Main.tz`:

```text
def pair :: i64 -> i64 -> i64 = \a _b -> a

let _unused = 5
let x = 40
pair x 2
```

`tsuzuri run f1` と、直した版の `tsuzuri run` は、どちらも `40` を出し（実測）、直した版の `check` は診断が 0 件（終了コード 0）。期待する `--dry-run` の差分は、システムの `diff -u f1/Main.tz fixed/Main.tz | tail -n +3` と一致した:

```text
@@ -1,5 +1,5 @@
-def pair :: i64 -> i64 -> i64 = \a b -> a
+def pair :: i64 -> i64 -> i64 = \a _b -> a
 
-let unused = 5
+let _unused = 5
 let x = 40
 pair x 2
```

（`tsuzuri fix` の差分は、先頭に `--- a/Main.tz`・`+++ b/Main.tz` が付く。2 行目の ` ` は空白 1 文字。）

### C2. `unified` の参照（システムの `diff -u` の出力と一致を確認した）

old = `a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n`（1 行ずつ `a`〜`j`）。

① `e` を `E` に変える:

```text
@@ -2,7 +2,7 @@
 b
 c
 d
-e
+E
 f
 g
 h
```

② `a` を `A`、`j` を `J` に変える（context 3 で hunk が離れる）:

```text
@@ -1,4 +1,4 @@
-a
+A
 b
 c
 d
@@ -7,4 +7,4 @@
 g
 h
 i
-j
+J
```

③ `"x\ny"` → `"x\nz"`（どちらも末尾の改行なし）:

```text
@@ -1,2 +1,2 @@
 x
-y
\ No newline at end of file
+z
\ No newline at end of file
```

④ `""` → `"a\n"`: `@@ -0,0 +1 @@` と `+a`。⑤ `"a\n"` → `""`: `@@ -1 +0,0 @@` と `-a`。⑥ 同一: 空文字列。

### C3. 性能と記憶

`proto.rs` の `bench`（`rustc -O`）。old = 20,000 行（`line {i}`）、new = 20 行ごとに 1 行を書き換えた 1,000 行の変更（挿入 1,000 と削除 1,000 で D = 2,000）。

| ケース | 結果 | 時間 |
| --- | --- | --- |
| D = 2,000（20,000 行） | `Some`（21,000 ステップ） | 14〜15 ms |
| D = 2,002（上限を 2 超える） | `None` | 11〜15 ms |
| 200 行に 1 行の変更（計 100 行）の `line_edits` | 100 編集 | 1.1 ms |

`/usr/bin/time -l` の最大常駐メモリは 42,254,336 bytes（約 40 MiB。D = 2,000 の `trace`、`isize` の約 4 M 要素）。GUIDE §14 の性能の測定ではなく、設計の上限を決めるための 1 回の測定で、速度の主張はしない。

### C4. 再現

```sh
# 試作のテスト（リポジトリの外で実行する）
rustc --edition 2024 --test proto.rs -o proto_test && ./proto_test   # running 5 tests ... 5 passed
# システムの diff との比較
diff -u old.txt new1.txt | tail -n +3
```

## 付録 D: テストの入力と期待値

テスト名はすべて本文の「テスト計画」と同じ。期待値は、`diff -u`・LCS の動的計画・手で書いた値から作った（コンパイラの現在の出力を写していない）。

### D1. `src/diff.rs`（単体 4 件）

| テスト | 入力 | 期待値 |
| --- | --- | --- |
| `matches_the_lcs_length_and_round_trips` | 線形合同法（`state = state * 6364136223846793005 + 1442695040888963407`、`state >> 33` を使う。種 7）で作る 3,000 組。各 0〜11 行、行は `l0`〜`l{a}`（`a` は 1〜5）、末尾の改行は old の 1/3・new の 1/4 で省く | `Equal` の数 = テスト内の LCS の長さ。`line_edits` を適用すると `new` に戻る |
| `unified_output_matches_diff_dash_u` | 付録 C2 の ①〜⑥（context 3） | 付録 C2 の全文（`--- a/M.tz`・`+++ b/M.tz` の 2 行を先頭に付ける） |
| `the_change_limit_returns_none` | 全行が違う 1,500 行と 1,500 行（D = 3,000）／ 900 行と 900 行（D = 1,800） | 前者は `None`、後者は `Some` |
| `line_edits_cover_each_changed_run` | `"a\nb\nc\n"` → `"a\nX\nY\nc\n"`／`"a\n"` → `"a\nb\n"`／`"a\nb\n"` → `""` | `[(2, 4, "X\nY\n")]`／`[(2, 2, "b\n")]`／`[(0, 4, "")]` |

### D2. `src/fix.rs`（単体。Phase 1 の 17 件）

偽の解析器: `project.sources[0].text` を見て、`x` の出現ごとに `W9990`（警告）と、その 1 文字を `y` にする `certain` の fix を返す関数。テストが、エラー・省略数・別の警告を足す。`Project::single_main(PathBuf::from("Main.tz"), text)` の `sources[0]` が利用者、後ろは std。

| テスト | 入力 | 期待値 |
| --- | --- | --- |
| `threshold_admits_by_applicability` | `Threshold` 3 × `Applicability` 3 | `Certain`: C だけ。`Likely`: C・L。`Maybe`: 3 つ |
| `rejects_edits_outside_the_writable_files` | 利用者 2 つ（A・B）と std 1 つの `Project`（`SourceFile` を直接組む）。edit を次のように変える: ① `source: Some(9999)` ② std の索引 ③ `Scope::File(0)` で B の edit ④ `start > end` ⑤ `end > len` ⑥ 多バイト文字の途中 ⑦ 4,097 bytes の `text` ⑧ 同じ fix の交わる 2 edit ⑨ `edits` が空 | ①②: `OutsideProject`、③: `OutsideInput`、④〜⑨: `InvalidEdit` |
| `conflict_is_closed_interval_intersection` | `(5,10)`と`(10,12)`／`(11,12)`／`(5,5)`と`(5,5)`／`(5,10)`と`(7,7)`／`(5,10)`と`(3,4)`／`(5,10)`と`(3,5)`／`(0,0)`と`(1,1)` | 交わる／交わらない／交わる／交わる／交わらない／交わる／交わらない |
| `resolve_keeps_the_first_in_sorted_order_and_defers_the_rest` | fix A `[5,10]`・B `[8,12]`・C `[20,21]`（別の診断） | 採用は A・C。B は記録されない |
| `identical_fixes_collapse` | 2 つの診断が同じ edit 列の fix を持つ | 候補は 1 件。`Applied` は 1 件 |
| `apply_edits_builds_the_text_in_one_pass` | `"abc def ghi"` に `(0,3,"X")`・`(4,7,"Y")`・`(11,11,"!")` | `"X Y ghi!"` |
| `candidate_limit_is_e1017` | `"x "` を 20,001 回並べたテキスト（交わらない 20,001 件） | `Err`。コード `E1017`、kind `fix_candidate_limit` |
| `rounds_stop_at_the_fixed_point` | `"x x"` | `rounds == 1`、`stop == FixedPoint`、`files` は 1 件で `fixed == "y y"`、`Applied` は 2 件、`last` に警告なし |
| `touching_fixes_apply_in_two_rounds` | `"xx"`（`[0,1]` と `[1,2]` は接する） | `rounds == 2`、`fixed == "yy"`、各ラウンドの `Applied` は 1 件 |
| `a_round_that_adds_an_error_aborts_without_changes` | `"x"`。解析器は、テキストに `y` があれば `E1003` の誤りを返す | `Err`。`E2008`・`fix_verification_failed`。`project.sources[0].text == "x"` |
| `a_round_that_adds_a_warning_aborts_only_when_errors_were_absent` | (a) 誤りなし。直すと `W9991` が増える (b) 誤りあり（`E1002` に `certain` の fix）。直すと誤りが消え、`W9991` が現れる | (a) `Err`（V3）(b) `Ok`（V3 は調べない） |
| `error_fixes_may_reveal_hidden_warnings` | (b) の続き: 2 ラウンド目に `W9991` の fix がある | 適用され、`rounds == 2` |
| `round_limit_stops_at_five_and_reports_the_rest` | 解析器が毎ラウンド新しい fix を返す（テキストが `x` のとき `"xx"`、`xx` のとき `"xxx"` … と長さに応じて 1 文字足す fix） | `rounds == 5`、`stop == RoundLimit`、6 つ目の候補が `Skipped(RoundLimit)` |
| `omitted_errors_skip_the_pair_check` | before: 誤り 1 件＋ `omitted = 3`。after: 別のコードの誤り 1 件＋ `omitted = 2`（合計 3 ≤ 4） | V1 を満たし、V2 は調べないので `Ok` |
| `file_scope_skips_other_files` | 利用者 2 つ。`Scope::File(0)`。解析器が両方に fix | A だけ適用。B は `Skipped(OutsideInput)` |
| `parse_arguments_table` | 下の D3 | `Ok` の各フィールド、または `Err` の `code`・`kind`・メッセージの断片 |
| `render_text_and_json_are_stable` | 手で作った `Report`: `rounds == 1`、`stop == FixedPoint`、記録 3 件（`Applied(1)` の `W1001` `Prefix 'b' with '_'`（35..36、行 1・列 36、`_b`）、`Applied(1)` の `W1001` `Prefix 'unused' with '_'`（47..53、行 3・列 5、`_unused`）、`Skipped(Applicability)` の `W1005` `Likely` `Replace 'pair' with 'pair2'`（72..76、行 5・列 1、`pair2`））、`files` は `Main.tz` の 1 件（`fixes == 2`）、`last` は診断なし。`threshold == Certain`、書く | 下の「D2 の出力」 |

D2 の出力（`render_text`）:

```text
fixed Main.tz: 2 fixes
applied 2 fixes in 1 file
skipped 1 fix (use --json for the list)
note: rerun with --applicability likely to apply 1 more fix
```

D2 の出力（`render_json`）:

```text
{"event":"fix","status":"applied","round":1,"code":"W1001","applicability":"certain","title":"Prefix 'b' with '_'","edits":[{"path":"Main.tz","span":{"start":35,"end":36,"line":1,"column":36,"end_line":1,"end_column":37},"text":"_b"}]}
{"event":"fix","status":"applied","round":1,"code":"W1001","applicability":"certain","title":"Prefix 'unused' with '_'","edits":[{"path":"Main.tz","span":{"start":47,"end":53,"line":3,"column":5,"end_line":3,"end_column":11},"text":"_unused"}]}
{"event":"fix","status":"skipped","reason":"applicability","code":"W1005","applicability":"likely","title":"Replace 'pair' with 'pair2'","edits":[{"path":"Main.tz","span":{"start":72,"end":76,"line":5,"column":1,"end_line":5,"end_column":5},"text":"pair2"}]}
{"event":"file","path":"Main.tz","fixes":2}
{"event":"summary","dry_run":false,"applicability":"certain","rounds":1,"stop":"fixed_point","applied":2,"skipped":1,"files":1,"remaining_errors":0,"remaining_warnings":0}
```

### D3. `parse_arguments_table`

| 引数 | 結果 |
| --- | --- |
| `["a"]` | `Ok`（`Certain`・書く・`json == false`） |
| `["a","--dry-run","--applicability","likely","--json"]` | `Ok`（`Likely`・`dry_run`・`json`） |
| `["a","--applicability","maybe","--dry-run"]` | `Ok`（`Maybe`） |
| `["--","--json"]` | `Ok`（パスが `--json`） |
| `[]`・`["a","b"]`・`["a","--x"]`・`["a","--applicability"]`・`["a","--applicability","sure"]` | `E2000`・`fix_bad_arguments` |
| `["a","--dry-run","--dry-run"]` | `E2000`・`fix_bad_arguments`（`--dry-run specified more than once`） |
| `["a","--applicability","maybe"]` | `E2000`・`fix_maybe_needs_dry_run` |
| Phase 3 で追加: `["a","--edition","2030"]` | `E2000`・`fix_edition_unsupported`（`unsupported edition '2030'` を含む） |
| Phase 3: `["a","--edition"]` | `E2000`・`fix_bad_arguments` |
| Phase 3: `["a","--edition","2026","--applicability","likely"]` | `E2000`・`fix_edition_with_applicability` |

### D4. `src/fix.rs`（単体。Phase 3 の 4 件）

偽の解析器 `analyze(project, canonical)`: `canonical == true` のとき、`x` の出現ごとに `W1007`（警告）と `x` → `y` の `certain` の fix。`canonical == false` のとき、根のソースの `edition` が `Edition(2027)` で、テキストに `z` があれば `E0004` の誤りを 1 件、なければ誤りなし。`execute_with` は実ファイルへ書くので、一時ディレクトリに `Main.tz` と（あれば）`Tsuzuri.toml`（`"[package]\nname = \"a\"\nversion = \"0.1.0\"\n"`）を作り、`Project` はそのパスを持つ `SourceFile` で組む。

| テスト | 入力 | 期待値 |
| --- | --- | --- |
| `migration_applies_canonical_fixes_then_checks_the_target` | `Main.tz` = `"x x"`、根の edition 2026、目標 `Edition(2027)` | `Main.tz` は `"y y"`。`Tsuzuri.toml` に `edition = "2027"` の行が `version` の次に入る。`exit == 0`、`blocked == None`。標準出力に `edition: Tsuzuri.toml now says edition = "2027"` |
| `a_blocked_migration_writes_sources_but_not_the_manifest` | `Main.tz` = `"x z"` | `Main.tz` は `"y z"`（書く）。`Tsuzuri.toml` は元のまま。`blocked` が `E2008`・`fix_edition_blocked`、`exit == 1`、標準出力に `edition: blocked; Tsuzuri.toml was not changed` |
| `migration_without_a_manifest_reports_none` | `manifests` が空 | `Main.tz` は書かれ、標準出力に `edition: no Tsuzuri.toml; pass --edition 2027 to check, build, run, and test` |
| `migrating_to_the_current_edition_is_a_no_op` | 目標 = 根の edition（2026） | 標準出力は `edition is already 2026; nothing to migrate\n`、何も書かない、`exit == 0` |

### D5. `package::set_edition`（`set_edition_table`。単体 1 件 12 行）

| # | 入力の `[package]` の節（`name = "a"` と `version = "0.1.0"` の後ろ） | 結果 |
| --- | --- | --- |
| 1 | `edition = "2026"` | `edition = "2027"` |
| 2 | `edition="2026"` | `edition="2027"` |
| 3 | `edition = "2026" # keep` | `edition = "2027" # keep` |
| 4 | `edition` なし。後ろに空行と `[dependencies]` | `version` の次の行に `edition = "2027"`（空行と `[dependencies]` はその後ろ） |
| 5 | CRLF の改行で `edition` なし | 足す行も `\r\n` |
| 6 | 最後の行が `version = "0.1.0"`（改行なし）で `edition` なし | `\nedition = "2027"` を続け、末尾に改行を足さない |
| 7 | `[package]` に `edition` なし、`[dependencies]` に `edition = { path = "../e" }` | `[package]` の `version` の次に足し、`[dependencies]` の行は変えない |
| 8 | `# edition = "2026"` のコメントだけ | `version` の次に足す（コメントは残す） |
| 9 | タブのインデントの `\tedition = "2026"` | インデントを保って値だけ変える |
| 10 | すでに `"2027"` | 同じ文字列 |
| 11 | `[package]` の節がない（`[dependencies]` だけ） | `None` |
| 12 | `edition = 2026`（引用符なし） | `None` |

### D6. `tests/fix.rs`（結合。23 件）

共通: `root(label, files)`（`tests/api_diff.rs` と同じ形）と `tsuzuri(args)`（`env!("CARGO_BIN_EXE_tsuzuri")` の `(status, stdout, stderr)`）。

| テスト | 入力 | 期待値 |
| --- | --- | --- |
| `replace_files_replaces_every_file` | 2 ファイル | 両方が新しい内容。権限は変わらない |
| `replace_files_restores_earlier_files_when_a_later_one_changed` | 2 ファイル。2 つ目を呼ぶ前に内容を変える | `Err(Changed)`、`restored == 1`、1 つ目は元の内容 |
| `replace_files_refuses_symlinks`（unix） | 2 つ目がシンボリックリンク | `Err(Failed)`、1 つ目は元の内容 |
| `replace_files_keeps_permissions`（unix） | `0o600` のファイル | 置換後も `0o600` |
| `usage_errors_exit_2` | `["fix"]`・`["fix","a","b"]`・`["fix","a","--x"]`・`["fix","a","--applicability","maybe"]`、各々に `--json` を足したもの | 終了コード 2、標準エラーに `error[E2000]`。`--json` では 1 行の JSON で `"code":"E2000"` |
| `no_fixes_prints_a_note_and_changes_nothing` | 「現状」2 の `a`（Phase 1 は `fixes` のない診断だけ） | 標準出力は `no fixes to apply\n`、終了コード 0、`Main.tz` のバイト列が同じ。標準エラーに `warning[W1001]` |
| `a_directory_without_main_fails_like_check` | `Util.tz` だけのディレクトリ | 終了コード 1、`E2001`（`check` と同じ） |
| `dependencies_and_std_are_never_candidates` | `app` と path 依存 `helper`。偽の解析器が `helper` と std のソースの edit を返す | 両方 `Skipped(OutsideProject)`、ファイルは変わらない |
| `run_with_a_fake_analyzer_writes_and_reports_on_a_real_project` | 実プロジェクトに `execute_with` と偽の fix | ファイルが書き換わり、`stdout` が本文の例 1 の形 |
| `w1001_carries_a_certain_prefix_fix` | `f1`（付録 C1） | `tsuzuri::analyze_modules_all` の `warnings` が 2 件で、それぞれ `fixes.len() == 1`、`Certain`、`edits` は 1 件（`span` は局所の span、`text` は `_b`・`_unused`）、title は `Prefix 'b' with '_'`・`Prefix 'unused' with '_'` |
| `w1001_has_no_fix_when_the_prefixed_name_is_taken` | ① 同じ関数に `let _unused = 1` と `let unused = 5` ② `def _unused :: i64 = 1`（別の関数）と `let unused = 5` | どちらも `W1001` はあり、`fixes` は空 |
| `w1005_carries_a_likely_replacement_for_a_same_signature_function` | 「現状」5 の `w/` | `W1005` の `fixes[0]` が `Likely`、edit は `3:16-19`（start 29, end 32）の `next`、title `Replace 'old' with 'next'` |
| `w1005_has_no_fix_when_the_signature_differs_or_the_note_is_free_text` | `next` の型を `i64 -> i32` に／注記を `use next` に／注記を `use Helper::Legacy.next instead (faster)` に／record の非推奨 | どれも `fixes` は空 |
| `fix_prefixes_unused_locals_and_a_second_run_changes_nothing` | `f1` | 1 回目: 標準出力 `fixed Main.tz: 2 fixes\napplied 2 fixes in 1 file\n`、`Main.tz` が付録 C1 の直した後、標準エラーは空、終了コード 0。2 回目: `no fixes to apply\n`、バイト列が同じ。`tsuzuri run` の出力が前後で `40` |
| `fix_dry_run_prints_the_diff_and_writes_nothing` | `f1` に `--dry-run` | 標準出力は `--- a/Main.tz`・`+++ b/Main.tz` ＋ 付録 C1 の差分 ＋ `would apply 2 fixes in 1 file\n`。ファイルは変わらない。標準エラーは空 |
| `fix_json_events_are_one_object_per_line` | `f1` に `--json` | 4 行（`fix` 2・`file` 1・`summary` 1）。各行が JSON としてパースでき、キーの順が本文の表どおり。`summary.applied == 2`・`files == 1`・`remaining_warnings == 0` |
| `fix_applies_deprecation_replacements_only_with_likely` | 「現状」5 の `w/app` | 既定: `no fixes to apply`・`skipped 1 fix`・`note: rerun with --applicability likely to apply 1 more fix`、ファイルは変わらない。`--applicability likely`: `Legacy.next 1` になる |
| `fix_never_writes_dependencies` | `w/` の `helper` に未使用の局所を足したもの | `helper` のファイルは変わらない。`--json` に `skipped`・`outside_project` |
| `fix_with_a_file_input_writes_only_that_file` | 2 つのファイル（どちらも `W1001`）。片方を渡す | 渡したファイルだけが直り、もう一方は `outside_input`（`--json`） |
| `fix_then_check_with_a_warm_cache_sees_the_new_text` | 同じ `TSUZURI_CACHE_DIR` で `check` → `fix` → `check` | 3 回目の標準エラーが空 |
| `edition_option_rejects_unsupported_years` | `--edition 2030` | 終了コード 2、`E2000`、`unsupported edition '2030'` を含む |
| `edition_option_to_the_current_edition_is_a_no_op` | `Tsuzuri.toml` が `edition = "2026"` のプロジェクトに `--edition 2026` | `edition is already 2026; nothing to migrate`、終了コード 0、全ファイルのバイト列が同じ |
| `edition_option_conflicts_with_applicability` | `--edition 2026 --applicability likely` | 終了コード 2、kind `fix_edition_with_applicability` |

### D7. `tests/lsp.rs`（Phase 4）

| テスト | 入力 | 期待値 |
| --- | --- | --- |
| `code_action_fix_all_prefixes_every_unused_local_of_the_document` | `scripted` の既存の `Warn.tz`（未使用ローカルが 1 つ）に、未使用ローカルをもう 1 つ足したもの。`context.only: ["source.fixAll"]` | 結果が 1 件。`kind == "source.fixAll.tsuzuri"`。`edit.changes[uri]` を適用したテキストで両方に `_` が付く（テスト内で LSP の範囲を適用する） |
| `code_action_fix_all_is_offered_only_when_asked_and_when_there_is_something_to_fix` | (a) `only: ["quickfix"]` (b) `only: ["refactor"]` (c) 診断のない文書で `only` なし | (a) quickfix だけで fixAll なし (b) `[]` (c) `[]` |
| （変更）`initialize` の期待値（`tests/lsp.rs` の `"codeActionProvider"` の行） | — | `{"codeActionKinds": ["quickfix", "source.fixAll.tsuzuri"]}` |
