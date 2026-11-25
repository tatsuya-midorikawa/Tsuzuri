# NC17: edition 2028 と範囲の構文の一本化（終端を含む `..`、`..<`、`<..`、`<..<`、`^k`）

| 項目 | 内容 |
| --- | --- |
| ID | NC17 |
| 優先度 | P1（自然なスライスの書き方の土台。[NC18](NC18-multi-axis-bracket-indexing.md) の多次元の角括弧がこのチケットの範囲の形・`^k`・境界の規則を使う） |
| 規模 | XL（Phase 1 = M、Phase 2 = L、Phase 3 = M） |
| 依存 | – |
| 後続 | [NC18](NC18-multi-axis-bracket-indexing.md)（`RangeKey`・`IndexComponent`・`ExprKind::FromEnd`・`Parser::index_component`・参照表 `tests/range-reference-cases.mjs`・文書の例の `edition=` を使う） |
| 状態 | todo |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認） |
| 承認 | 承認済み（D-66a〜c。利用者の決定 2026-11-25: 末尾からの位置は F# と同じ `^0` が最後、edition 2028 で `..` を一度に終端を含む意味へ変える、そのための [docs/stability.md](../docs/stability.md#edition) の edition 方針の例外を認める）。未決だった 3 つの質問は決定（D-66 の委任で決定、2026-11-25。D8・D9・D13）。決定事項はすべて承認済み・決定・既定案のどれか。新しい警告コード `W1013`・`W1014`、edition 2028、新しい字句は GUIDE D-55・D-15 に登録済み（2026-11-25 の統合）。`W1014` の kind `negative_literal_index` は D-55 の `W1014` の行に足した（D-66 の委任による決定の記録。新しい診断コードは要らない） |
| 手本にする既存実装 | [A22](_completed/A22-edition-2027-canonical-forms.md)（`src/canonical.rs`・`src/parse_canonical.rs` の記録と `W1007`／`E0004`、`tests/edition_2027.rs::corpus_migrates_without_changing_the_ir`、suite `range_forms`）、`src/check.rs` の `slice`（`..=` の巻き戻る `+ 1`、16075〜16093 行付近）、`TypedExprKind::SaturatingIndex`（添字の位置だけに現れる包み。`src/llvm.rs:7942` 付近）、`src/llvm_control.rs` の `range_loop`／`half_open_last`、`tests/tensor-cases.mjs`（E2E の独立参照を別ファイルに置く形） |
| 主な影響ファイル | `src/lexer.rs`、`src/syntax.rs`、`src/parser.rs`、`src/parse_control.rs`、`src/parse_canonical.rs`、`src/canonical.rs`、`src/check.rs`、`src/control.rs`、`src/llvm.rs`、`src/llvm_control.rs`、`src/ranges.rs`、`src/const_machine.rs`、`src/const_rules.rs`、`src/ownership.rs`、`src/effects.rs`、`src/last_use.rs`、`src/pure_rules.rs`、`src/prove_smt.rs`、`src/semantic.rs`、`src/warnings.rs`、`src/kg/sites.rs`、`src/formatter.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/diagnostic_kinds.rs`、`src/explain.rs`、`src/explain/codes/W1013.md`（新規）、`src/explain/codes/W1014.md`（新規）、`src/fix.rs`、`src/edition_migration.rs`、`src/main.rs`、`src/repl.rs`、`src/guide.rs`、`guide/`、`scripts/check-guide.mjs`、`scripts/check-docs.mjs`、`std/*.tz`（Phase 3）、`tests/range_family.rs`（新規）、`tests/edition_2028.rs`（新規）、`tests/range-reference-cases.mjs`（新規）、`tests/fixtures/range_reference/`（新規）、`tests/editions.rs`、`tests/range_forms.rs`、`tests/canonical_forms.rs`、`tests/features.mjs`、`tests/guide.rs`、`spec/rules/`、`docs/stability.md`、`docs/language.md`、`docs/architecture.md`、`_tsuzuri/language-reference/`（「ドキュメント」） |

## 実装者向け早見

- **範囲**: Phase 1（全 edition）は既知の不具合 2 つ（`i64u` の終端を含むスライス、添字の角括弧の旗 `stop_at_slice_dotdot` の漏れ = B3）の修正と、非正準な表記の検出を「edition ごとに全部拒否か全部許す」から「表記の種類ごとの edition の範囲」に直すこと。Phase 2 は edition 2028 を作り、範囲の形（終端を含む素の `..`、`..<`、`<..`、`<..<`、`^k`、角括弧の中の刻み）、境界の規則、警告 `W1013`・`W1014`、`tsuzuri fix --edition 2028` を入れる。同じ PR で、2028 の差の全体（NC18 の形を含む）を docs/stability.md に宣言する（D13）。Phase 3 は std・fixture・examples・文書を移行してから `Edition::LATEST` と `NEW_PROJECT` を 2028 に動かし、マニフェストのないソースに意味の変化を知らせる。
- **最初に読む**: 仕様 1（基準表）→ 仕様 2（境界の規則）→ 仕様 5（edition ごとの扱い）→ 設計の「データ構造」と「正しさの根拠」→ 自分の Phase の手順。コードは `src/parser.rs` の `index_or_slice`（4511〜4592 行付近）と `expressions`（4595〜4612 行付近）、`src/parse_control.rs` の `range_expression`（171〜221 行付近）、`src/check.rs` の `index_operand`（15993 行付近）と `slice`（16028 行付近）、`src/llvm.rs` の `array_slice`（7028 行付近）の順に読む。
- **最初に書くテスト**: Phase 1 は `tests/range_family.rs::inclusive_slice_end_of_i64u_max_traps` と `src/parser.rs` の単体テスト `for_range_inside_parentheses_in_brackets_parses`、`tests/editions.rs::noncanonical_kinds_have_edition_ranges`。Phase 2 は `tests/range_family.rs::new_forms_need_edition_2028` と suite `range_reference`（参照表）。Phase 3 は `tests/edition_2028.rs::corpus_migrates_without_changing_the_ir`。
- **触ってはいけない**: 2026・2027 のソースのうち新しい形を使わないものの受理・拒否・生成 IR（例外は Phase 1 の `i64u` の修正だけ）、`for` の範囲の意味（逆向きは 0 回、型の最大値で止まる）、`..=` と `..<` の意味（全 edition で同じ）、添字 `xs[i]` の IR（`i64u` の添字も `as i64` のまま）、`xs[-1]` のトラップ（足すのは警告 `W1014` の `negative_literal_index` だけ）、オペランドの後ろの `^`（XOR）、関数版（`String.slice`・`Array.sub`・`Slice.range`）の契約、`MANIFEST_DEFAULT = 2026`、`EDITION_KEYWORDS`（空のまま）。
- **作る新しい名前**: `Edition::E2028`・`Edition::range_family`、`TokenKind::{LessDotDot, LessDotDotLess}`、`RangeOp::{StartOpen, BothOpen}`、`RangeEnds`・`RangeKey`・`IndexComponent`、`ExprKind::FromEnd`、`ExprKind::Slice { value, range, mutable }`、`TypedExprKind::FromEnd`、`TypedExprKind::ForRange.start_open`、`NoncanonicalKind::{LegacyBareSliceRange, BareSliceRangeMeaningChanged}` と `warned_in`・`rejected_in`・`code`、`Parser::index_component`・`Parser::record_slice_range`、`Checker::bound_operand`・`Checker::negative_bound_warnings`・`Checker::range_warnings`、診断の kind 13 個（仕様 7）、`tests/range-reference-cases.mjs`、docs/stability.md の小節「リリースの数え方」（仕様 10）。
- **完了の見分け方**: 仕様 1 の基準表の全行が、参照表の suite で配列・固定長配列・文字列（要素の行）について native と WASM の `-O0`・`-O3` で一致する。2027 のコーパスを `tsuzuri fix --edition 2028` で移行すると IR が 1 byte も変わらない。

## 目的

利用者は F# と同じく、範囲の構文が見たとおりの端を含むことを求めた（D-66a。利用者の指定をそのまま採る）:

> - `1..3` : 1 〜 3 (3 を含む) / `1..<3` : 1 〜 2 (3 を含まない) / `1<..3` : 2 〜 3 (1 を含まず、3 を含む)
> - `..3` : 0 〜 3 (3 を含む) / `..<3` : 0 〜 2 (3 を含まない) / `0..` : 0 〜 N: 最終インデックス (N を含む)

今の Tsuzuri は、`for` の素の `..` が両端を含み（edition 2027 では `E0004`）、スライスの素の `..` が半開である。同じ記号が文脈で別の意味になり、NumPy・Python・F# から来た人も AI も書き間違える。末尾から数える書き方もなく、NumPy の `a[:-1]` を書くには `xs.length` を使った式が要る。

このチケットは、範囲の書き方を 1 つの族にまとめる。`a..b`（両端を含む）、`a..<b`（終端を含まない）、`a<..b`（始端を含まない）、`a<..<b`（両端を含まない）、`..b`・`..<b`・`a..`・`a<..` と、末尾からの位置 `^k`（`^0` が最後。F# と同じ。D-66b）。どのコンテナでも「範囲を半開の境界 (lo, hi) に直し、`0 ≤ lo ≤ hi ≤ n` でなければトラップ」という 1 つの規則で決まる。`..` の意味を変えるのは新しい edition 2028 だけで（D-66c）、旧 edition のソースは既定で有効な警告と `tsuzuri fix` で移行できる。多次元の角括弧 `t[i, a..b, ..]` は [NC18](NC18-multi-axis-bracket-indexing.md) が、このチケットの規則の上に作る。

## 着手条件と停止条件

### 着手条件

- 承認は記録済み（D-66a〜c）。未決だった 3 つの質問も決定済み（D-66 の委任で決定、2026-11-25。D8・D9・D13）。依存チケットはない。
- Phase 2 は Phase 1 の後。Phase 3 は Phase 2 の後。基準は `gh run list --branch main --workflow ci.yml --limit 1` の最新の `main`（GUIDE §2.3）。
- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。

### 停止条件

- **S1** 新しい形を使わない 2026・2027 のプログラムの生成 IR が変わった（Phase 1 の `i64u` の終端を含むスライスの修正を除く）。fixture のコーパスの `--emit llvm -O0` を変更前と比べて見つける。
- **S2** `..` を `..<` に書き換える移行（`W1013` の修正案、Phase 3 の一括移行）で、2026・2027 の IR が 1 byte でも変わった。
- **S3** 参照表の独立参照（`tests/range-reference-cases.mjs`）と実装が、どれかの入力（型の最大値・最小値、`i64u` と `i128` の範囲外、長さ 0 と 1）で食い違い、設計の「正しさの根拠」の前提が崩れた。
- **S4** 今受理しているプログラムの構文解析の結果が、`<..`・`<..<` のトークンや前置の `^` を足したことで変わった（`xs[k ^1]` のような XOR を含む）。
- **S5** 2028 を含むリリース（仕様 10 の数え方）の後で、次のどちらかが要ると分かった。(a) 2028 が受理していたプログラムの拒否か、意味の変更（バグの修正を除く）。(b) Phase 2 で宣言していない 2028 の差。NC18 などの実装で分かったものも含む。新しい edition か人間の承認が要る（決定事項 D13）。リリースの前なら、NC17 の規則を直してよい。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `858a949c` で確認）

確認は `target/debug/tsuzuri`（HEAD の後にビルド）で行った。プローブは一時ディレクトリごとにマニフェストのない `Main.tz`（ソースは各項目に書いた。名前は大文字で始める）を置き、`node scripts/slot.mjs -- target/debug/tsuzuri run -O0 <dir>/Main.tz` で動かした（マニフェストがないので edition 2027）。着手時の HEAD で作り直して確かめる（GUIDE §0 の 6）。

### 字句と構文

- `Lexer::symbol`（`src/lexer.rs` 1048〜1083 行付近）の最長一致の表は `..=`（1067 行）・`..<`（1068 行）・`..`（1069 行）を持ち、`<..`・`<..<` はない。`^` は 1 文字の `Caret`（1114 行）で、二項の XOR（`src/parser.rs:5435`、優先度 6）にだけ使う。
- `range_op`（`src/parser.rs` 9〜15 行）は `..=` を `Closed`、`..<` を `Open`、**それ以外のすべてを `Bare`** にする（網羅的でない）。`range_operator_ahead`（1632〜1638 行付近）は `..`・`..=`・`..<` を見る。
- `index_or_slice`（4511〜4592 行付近）は、角括弧の中を「式 1 つ」か「始端・範囲の演算子 1 つ・終端」として読む。刻みも `^` も読まない。始端を読む前に `stop_at_slice_dotdot` を立て（4515 行）、`expression_inner` は旗が立っている間、範囲の `for` を読まない（4242 行）。
- **B3（既知の不具合）**: `expressions`（4595〜4612 行付近）・`block`（3133 行付近）・`record`（3570 行付近）・`computation_block`（3702 行付近）・`interpolation`（`src/parse_control.rs:225` 付近）・`try_expression`（同 923 行付近）は `stop_at_arm`・`stop_at_arrow` を退避して戻すが、`stop_at_slice_dotdot` は戻さない。プローブ `B3`:
  ```text
  let xs = [10, 20, 30, 40, 50]
  xs[({ let mut a = 0; for i in 0 ..= 1 do { a = a + i; () }; a })]
  ```
  → `error[E0002]: expected 'do' after the enumerable expression`（2:33）。for-in.md 126〜127 行と docs/language.md 2814 行に既知の不具合として書かれている。
- `range_expression`（`src/parse_control.rs` 171〜221 行付近）は `for` の範囲を読む。ステップは最初の演算子が素の `..` のときだけ（179 行）。最後の演算子が素の `..` なら `record_bare_range`（`src/parse_canonical.rs` 14〜28 行付近）で記録する（199〜202 行）。`for i = a to b` は `op: RangeOp::Closed`（570 行付近）。
- プローブ `LessDot` の `ref xs[1<..3]` と `Caret` の `xs[^1]` は、どちらも `E0002: expected an expression`。プローブ `Xor` の `let k = 2` / `xs[k ^1]` は `40`（XOR。変えてはいけない）。`.tz`・`.tt`・`.tc` のファイルに `<..` を含むものはない（`rg '<\.\.'` で 0 件）。
- `tsuzuri fmt` はプローブ `Fmt` を `ref xs[1 .. 3]`、`ref xs[.. 2]`、`ref xs[2 ..]`、`ref xs[1 ..< 3]` に整える。前置の演算子の空白の表（`src/formatter.rs` 790〜805 行付近）に `Caret` はない。

### 型検査と生成

- `ExprKind::Slice` の型検査の入口（`src/check.rs` 14526〜14538 行付近）は `*op == RangeOp::Closed` だけを `slice` に渡す。素の `..` と `..<` は同じ型付きの木になる。`for` は `src/control.rs:259` 付近で `*op == RangeOp::Open` だけを半開にする（素の `..` と `..=` は閉区間）。
- `slice`（16028〜16110 行付近）は配列と（共有の）固定長配列だけを受け、ほかは `E1005`（`slice_non_array`）。終端を含む終端は `b + 1` を巻き戻る足し算（`TypedExprKind::Binary(Add, b, Int(1))`）で作る（16075〜16093 行付近）。
- `index_operand`（15993〜16024 行付近）は、64 bit 以下の整数を `as i64` にする。**`i64u` はビットのまま**（`i64.max` を超える値は負になる）。128 bit と型変数は `SaturatingIndex`。添字は、配列・リスト・`Vec`・固定長配列・`string`・`utf8string` が組み込み（14748〜14764 行付近）、ほかは `Index` クラス。
- `array_slice`（`src/llvm.rs` 7028〜7063 行付近）は `start ≤u end`・`end ≤u length`（符号なしの比較。7050〜7052 行）を検査し、外れたら `BoundsCheck` でトラップする。組み込みの添字の読み出しは 6507（文字列）・6528（固定長配列）・6548（配列・リスト・`Vec`）行付近、場所は 6700・6705 行付近。長さはどれも O(1) で読める（配列・リスト・`Vec` は `extractvalue …, 1`、文字列は `string_count`、固定長配列は型の長さ）。
- **`i64u` の不具合**: プローブ `U64end` の `ref xs[0..=18446744073709551615i64u]` は長さ `0` を返す（期待はトラップ）。`index_operand` が `i64u` を `-1` にし、`+ 1` で `0` に巻き戻る。
- プローブ `Neg` の `ref xs[..-1]` は今トラップする（半開の終端 `-1` は符号なしで長さを超える）。edition 2028 の規則では hi = `-1 + 1` = 0 で**空**になる（NumPy の `a[:-1]` は「最後を除く」）。`W1014` の理由。
- プローブ `Huge` の `new [unit](9223372036854775807, \_ -> ())` は `allocation failed` でトラップする（要素の大きさが 0 でも長さ `i64.max` の配列は作れない。設計の「正しさの根拠」の前提）。
- **負の整数リテラルの添字**: プローブ `NegIndex` の `let xs = [10, 20, 30]` / `xs[(-1)]` は警告なしで受理し、実行すると `trap: index out of bounds` になる（docs/language.md 3070 行「負のインデックス … は … 安全にトラップします」）。括弧の `ExprKind` はない（`src/syntax.rs` の括弧は 140〜141 行のトークン `LeftParen`・`RightParen` だけ）ので、`xs[(-1)]` と `xs[-1]` は同じ AST になる。組み込みの 6 型の添字は `ExprKind::Index` の型検査（`src/check.rs` 14748〜14764 行付近）が `index_operand` を呼ぶ。2027 の代入 `xs[i] = v` の組み込みの場合も、場所を同じ経路で型付けする（14990 行付近の `self.expression(place, None)`）。`index_operand` はほかに、`new [T](n, f)` の長さ（14616 行付近）とスライスの境界（16070・16073 行付近）も変換する。リポジトリの `.tz`・`.tt`・`.tc` に負の整数リテラルの添字はない（`rg` で 0 件）。Rust の文字列の中では `src/ranges.rs:699` の単体テスト 1 件だけで、警告は確かめていない。

### edition

- `Edition`（`src/syntax.rs` 27〜66 行付近）: `E2026`・`E2027`、`LATEST = E2027`、`MANIFEST_DEFAULT = E2026`、`SUPPORTED = [E2026, E2027]`、`NEW_PROJECT = E2027`。**`borrows_parameters` は `matches!(self, Self::E2027)`**（43〜45 行）で、このままでは 2028 が A18 の引数規約を失う（`src/check.rs:8305`・`9373`・`9396`、`src/api_diff.rs:1570`、`src/guide_std.rs:83` が呼ぶ）。`rejects_noncanonical` は `self >= Self::E2027`（49〜51 行）。
- 非正準な表記の拒否は edition ごとに全部か無しか（`src/parser.rs` 1239〜1246 行付近）。拒否のメッセージは `edition 2027` を文字列に埋め込んでいる（`src/canonical.rs:50`）。`canonical::KINDS` は 12 個（14〜27 行）。
- プローブ `ForBare` の `for i in 1 .. 3` は `E0004: '..' is not allowed in edition 2027; write '..='`。
- edition の移行 `edition_migration::prepare`（`src/edition_migration.rs` 166〜308 行付近）は canonical round（`fix::canonical_round`、`src/fix.rs` 1141〜1156 行付近。`W1007` だけ）の後に、**元の edition を見ずに** `preserve_parameters` を呼ぶ（246 行）。このままでは 2027 → 2028 の移行が、すでに借用の引数に `own` を足し、`Vec.push` を `Vec.push_owned` に変える。
- `tsuzuri guide` の `filter_edition`（`src/guide.rs` 254〜285 行付近）は `<!--if edition=YYYY-->` を**年の完全一致**で残す（268 行）。`guide/AGENTS.md.tmpl`・`guide/card.md`・`guide/language.md`・`guide/ownership.md`・`guide/pitfalls.md` が 2027 のブロックを持つ。`scripts/check-guide.mjs:21` は `EDITIONS = ['2026', '2027']`、`tests/guide.rs` は `Edition::SUPPORTED` を回す。このまま 2028 を足すと、2028 のガイドから 2027 のブロックが消える。
- `--help` と使い方の文は `--edition 2026|2027` を文字列で持つ（`src/main.rs:68`・`287`・`2963`）。
- フロントエンドのキャッシュ: `FRONTEND_FORMAT = 27`（`src/frontend_cache.rs:32`）。`unit_enum!(RangeOp { Bare, Closed, Open })`（`src/syntax_codec.rs:283`）、全タグの見本の検査（2866〜2890 行付近。`RangeOp` 3、`ExprKind` 49）。
- リリース: コンパイラのタグはない。タグは VS Code 拡張の `vsc-v0.1.2`（2026-10-21）〜`vsc-v0.1.9`（2026-11-21）の 8 つだけで、`target/debug/tsuzuri --version` は `tsuzuri 0.1.0` を出す。拡張はコンパイラを同梱する（`vsc/README.md` 10 行、`vsc/CHANGELOG.md` 0.1.9「Refresh the bundled compiler」）。`git diff --name-only vsc-v0.1.7 vsc-v0.1.8 -- src std guide Cargo.toml Cargo.lock` は空（0.1.8 はコンパイラを変えていない）。`vsc-v0.1.8..vsc-v0.1.9` は 13 ファイル。
- edition 2027 の経緯: `vsc-v0.1.5`（2026-10-25）の `src/syntax.rs` は `LATEST = E2027` で、`src/` に `E0004` はない（CHANGELOG 0.1.5「inputs without a manifest now use edition 2027」）。`vsc-v0.1.6`（2026-11-01）が A22 の拒否 `E0004` を足した（`src/canonical.rs`・`src/diagnostic_kinds.rs`・`src/explain.rs`・`src/parser.rs`・`src/syntax.rs`。CHANGELOG 0.1.6 の Compatibility で知らせた）。A22 は人間の包括許可（D-50）のもとで承認された（[A22](_completed/A22-edition-2027-canonical-forms.md) 12・19 行）。docs/stability.md 100 行の「リリースに含めた edition は凍結し、差を足しません」は `f1ff140e`（2026-10-11）で入り、この 2 つのタグのどちらにもある（`git log -S`）。つまり 0.1.6 は、リリースした edition に拒否を足した。

### 素の `..` のスライスの数（`rg` の概数。終端のある `ref xs[a..b]`・`ref xs[..b]`）

std 18（例: `std/Matrix.tz:82`、`std/Vec.tz:364`、`std/Array.tz:223`・`1438`（`///` の例）、`std/Tensor.tz:705-706`）、tests 214、`_tsuzuri/language-reference` 22、`guide/` 3、`docs/` 8。std は `LATEST` で読むので、Phase 3 で `LATEST` を動かす前に全部 `..<` へ移す。

## 変えてはいけない意味

- 2026・2027 のソースのうち新しい形を使わないものの受理・拒否・診断（`W1013`・`W1014` が増えるのを除く）・生成 IR。例外は Phase 1 の `i64u` の修正（終端を含む終端（`..=`）が `i64u` のスライスだけ IR が変わる）と、B3 の修正（受理が増えるだけ）。
- `for` の範囲の意味（for-in.md、規則 CTL-004〜006）: 開始・刻み・終了を 1 回ずつ順に評価する、逆向きは 0 回、次の値が型を外れたら止まる、刻み 0 はトラップ。A22 付録 F の値の表。
- `..=` と `..<` の意味。どの edition でも同じで、警告もしない（`..=` は恒久的な別名）。
- 添字 `xs[i]` の IR（`i64u` の添字は `as i64`。`src/check.rs:15991` 付近の注釈「`xs[i]` と `xs[i as i64]` は同じコード」と array.md 169 行の主張）。`xs[-1]` はトラップのまま（[NC04](_completed/NC04-generic-index-class-and-slice.md) D4）。負の位置は決して末尾から数えない。足すのは警告 `W1014`（`negative_literal_index`。D8）だけで、受理も IR も変えない。
- オペランドの後ろの `^` は XOR（`xs[k ^1]` は `40`）。
- 関数版の契約: `String.slice first last`（半開、`Maybe`）、`Array.sub start count`、`Slice.range`（半開、範囲外を丸める）。範囲の**構文**だけが既定で終端を含む。
- スライスは借用（`ref`／`ref mut` が要る。`SliceMustBeBorrowed`）、`ref xs[..]` は `E0002`、一時値の配列のスライスは `E1005`。所有権と借用の規則。
- `@checked` の中でも、合成した `+ 1`・`n - k` は検査付きの算術にしない（A22 と同じ）。
- `MANIFEST_DEFAULT = 2026`。依存パッケージは自分の edition で読む。std は 1 つのコピー。新しい予約語はない。
- 生成 IR の決定性。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 不具合の修正（`i64u` の終端、B3）。非正準な表記の種類ごとの edition の範囲（`warned_in`・`rejected_in`）。拒否のメッセージの edition を実際の edition にする | M | なし | 全 edition。受理が増えるのと、範囲外の `i64u` の終端がトラップするようになるだけ |
| 2 | edition 2028（選べるだけ。`LATEST` は 2027 のまま）。範囲の形、`^k`、角括弧の中の刻みの構文、境界の規則、`for` の `<..`・`<..<`、`W1013`（`legacy_bare_slice_range`）・`W1014`（`negative_literal_range_bound`・`negative_literal_index`）、`tsuzuri fix --edition 2028`、ガイドの `>=` のブロック、参照表。docs/stability.md の 5 つ目の差の宣言（範囲の構文の族の全体）、凍結の規則の明確化、2027 の経緯、リリースの数え方 | L | Phase 1 | 2028 を選んだパッケージだけ意味が変わる。2026・2027 は警告が増えるだけ |
| 3 | std・fixture・examples・ベンチマーク・文書の移行（`..<`）、`LATEST` と `NEW_PROJECT` を 2028 に、マニフェストのないソースへの `W1013`（`bare_slice_range_meaning_changed`）、文書の例を 2028 で実行 | M | Phase 2 | 移行は IR を変えない。`LATEST` を動かすのは移行を検証した後 |

**Phase 2 は 1 つの PR にする**。2028 のすべての拒否と意味の差と、5 つ目の差の宣言（NC18 の形を含む範囲の構文の族の全体。仕様 5）は、この PR に入れる。宣言した差の実装（NC18 の 2028 の受理）は、2028 を含むリリースの後に足してよい（凍結の規則。S5・D13）。

## 仕様

### 1. 範囲の形と意味（基準表。edition 2028）

`xs = [10, 20, 30, 40, 50]`（長さ n = 5、最後の添字 N = 4）。**この表が唯一の基準で、チケット・テスト・文書の例はこれと一致させる**（D-66 の共通の基準表）。

| 書き方 | 結果 | 境界 (lo, hi)（半開） |
|---|---|---|
| `xs[^0]` / `xs[^1]` | `50` / `40` | – |
| `ref xs[1..3]` | `[20, 30, 40]` | (1, 4) |
| `ref xs[1..<3]` | `[20, 30]` | (1, 3) |
| `ref xs[1<..3]` | `[30, 40]` | (2, 4) |
| `ref xs[1<..<3]` | `[30]` | (2, 3) |
| `ref xs[..2]` / `ref xs[..<2]` | `[10, 20, 30]` / `[10, 20]` | (0, 3) / (0, 2) |
| `ref xs[2..]` / `ref xs[2<..]` | `[30, 40, 50]` / `[40, 50]` | (2, 5) / (3, 5) |
| `ref xs[..^1]` | `[10, 20, 30, 40]` | (0, 4) |
| `ref xs[1..^1]` | `[20, 30, 40]` | (1, 4) |
| `ref xs[^2..]` / `ref xs[^2<..]` | `[30, 40, 50]` / `[40, 50]` | (2, 5) / (3, 5) |
| `ref xs[0..^0]` / `ref xs[..<^0]` | 全部 / `[10, 20, 30, 40]` | (0, 5) / (0, 4) |
| `ref xs[2..1]` / `ref xs[2..<2]` | `[]` / `[]` | (2, 2) |
| `ref xs[3..1]` | トラップ | (3, 2) |
| `ref xs[0..5]` | トラップ | (0, 6) |
| `ref xs[..^5]` | `[]` | (0, 0) |
| `ref xs[^5..]` | トラップ | (-1, 5) |
| テンソルの刻み `ref t[0 .. 2 .. 4]` / `ref t[.. 2 ..]` | 添字 0, 2, 4 / 0, 2, 4 | – |
| `ref t[.. -1 ..]` / `ref t[3 .. -1 .. 1]` | 添字 4, 3, 2, 1, 0 / 3, 2, 1 | – |

テンソルの行は [NC18](NC18-multi-axis-bracket-indexing.md) が実装する（このチケットは規則と参照を定め、配列への刻みを `E1005` にする）。

NumPy との対応: `a[-1]` → `xs[^0]`、`a[-k]` → `xs[^(k-1)]`、`a[:-1]` → `ref xs[..^1]`、`a[1:-1]` → `ref xs[1..^1]`、`a[-3:]` → `ref xs[^3<..]` または `ref xs[^2..]`、`a[x:y]` → `ref xs[x..<y]`。

`for` の範囲（2028）: `a .. b`・`a ..= b` は a〜b、`a ..< b` は a〜b−1（A22 と同じ）、`a <.. b` は a+1〜b、`a <..< b` は a+1〜b−1。`<..`・`<..<` は刻みを取らない。`a` が型の最大値なら `a <.. b` は 0 回（最初の値 a+1 が型を外れる。「次の値が型を外れたら止まる」と同じ）。

### 2. 境界の規則

1. 各形を半開の境界 (lo, hi) に直す。`a..b` → (a, b+1)、`a..<b` → (a, b)、`a<..b` → (a+1, b+1)、`a<..<b` → (a+1, b)、`..b` → (0, b+1)、`..<b` → (0, b)、`a..` → (a, n)、`a<..` → (a+1, n)。境界に書いた `^k` は先に位置 `n − 1 − k` にする。
2. **境界のモデルだけを使う**: lo・hi を正確な整数で求め（はみ出さない）、`0 ≤ lo ≤ hi ≤ n` なら有効、そうでなければ `BoundsCheck` でトラップ（`index out of bounds`）。`lo = hi` なら空。「計算した位置が負ならトラップ」という別の規則は置かない（`ref xs[-1<..2]` は lo = 0 で有効、`ref xs[..^5]` は空）。
3. 型の最大値の `b + 1` も正確に扱う（`b = MAX` なら hi > n でトラップ）。`i64` に収まらない被演算子（`i64u` の `2^63` 以上、`i128` の範囲外）は必ずトラップする側に倒れる。
4. 添字 `xs[^k]` の位置は `n − 1 − k`。`0 ≤ n − 1 − k < n`（つまり `0 ≤ k < n`）でなければ `BoundsCheck` でトラップする。負の位置の添字 `xs[-1]` は、どの edition でもトラップする（末尾から数えない）。
5. 評価順: 土台、始端、終端の順（今のスライスと同じ）。`^k` の `k` は自分の位置で評価する。長さは土台の評価の後に読む（読むのは土台の値だけで、副作用はない）。
6. ループの範囲は値を列挙し、スライスの範囲はメモリを指す。ループの既存の規則（逆向きは 0 回、型の最大値で止まる）は変えない。
7. 負の数を直接書いた境界（`..-1`、`-1<..2`、`^-1`）には警告 `W1014`（`negative_literal_range_bound`）。組み込みの添字に負の整数リテラルを書いた `xs[-1]` にも `W1014`（`negative_literal_index`）。どちらも仕様 7。

### 3. 末尾からの位置 `^k`

- `^k` は位置 `n − 1 − k`。`^0` が最後、`^1` が最後から 2 番目（F#・RFC FS-1076 と同じ。D-66b）。`..^1` は「最後を除く」、`0..^0` は `0..` と同じ。
- **書ける場所**: 組み込みの添字の型（配列、リスト、`Vec`、固定長配列、`string`、`utf8string`。どれも長さを O(1) で読める）の
  - 要素の添字 `xs[^k]`（読み出し、代入の対象 `xs[^0] = v`、要素の借用 `ref xs[^0]`）、
  - 配列と固定長配列のスライスの境界（`ref xs[^3..^1]`、`ref mut xs[^2..]`）。
- 文字列の範囲のスライスは今と同じ `E1005`（`slice_non_array`）。文字列で書けるのは要素の `s[^k]` だけ。
- `Index` クラスの型（`Tensor` など）の `^k` は [NC18](NC18-multi-axis-bracket-indexing.md)。このチケットでは `E1005`（`from_end_needs_length`）。
- 構文: `^` は角括弧の成分の先頭か、角括弧の中の範囲の演算子の直後にだけ書け、**1 つの項**（`ref` と同じ TERM。後置の `.field`・`[i]`・隣接する `(...)` を含む）を取る。`^k`、`^(i + 1)`、`^ys[0]`（= `^(ys[0])`）。`^` の項の後に続けてよいのは `]` と範囲の演算子だけ（`^i + 1` は `E0002`。`^(i + 1)` と書く）。それ以外の位置の前置の `^`（配列リテラル `[^1]`、括弧の中 `xs[(^0)]`、`for i in 0 ..< ^1`）は `E0002`。オペランドの後ろの `^` は XOR のまま。
- `k` の型はどの整数型でもよい（添字と同じ）。定数式（`const`・`const def`）の中の `^k` は既存の `E1026`（`const_operation_not_allowed`。決定事項 D12）。

### 4. 刻み（角括弧の中。規則の定義と、配列での拒否）

- 文法は `for` と同じ `a .. s .. b`（最初の `..` は刻みの印、2 つ目の演算子が終端の扱い）。角括弧の中では始端・終端を省ける（`.. 2 ..`、`3 .. -1 ..`）。
- 正の刻み: 仕様 2 で (lo, hi) を決め、lo, lo+s, … < hi を選ぶ。境界の規則は**書いた境界**に当てる（`for i in 0 .. 4 ..< 10` は n = 9 で 0, 4, 8 を回るが、`ref t[0 .. 4 ..< 10]` は hi = 10 > 9 でトラップ）。
- 負の刻み: 終端を含む形（`..`・`..=`）か省略形だけ。省いた始点は N、終点は 0。軸を i ↦ N − i に反転してから同じ規則を当てる。負の整数リテラルの刻みと `..<` の組み合わせは `E0002`（`slice_step_down_excludes_end`）。`<..`・`<..<` は刻みを取らない（`E0002`、`start_open_range_with_step`。`for` でも同じ）。
- 刻み 0 はトラップ（刻みを書けるのはテンソル・行列の窓だけなので、トラップの文は NC18 の仕様 3.4 のとおり）。
- `^` で刻みを始めることはできない（`E0002`、`from_end_step`）。
- **配列と固定長配列は連続なので、刻みのあるスライスは `E1005`（`array_slice_with_step`）**。テンソルと行列の刻み付きの窓は NC18 が実装する。実行時に負になった刻みと `..<` の組み合わせの扱いは決定事項 D14（NC18 が実装する）。

### 5. edition ごとの扱い

| 書き方 | 2026・2027 | 2028 |
| --- | --- | --- |
| スライス `a..b`・`..b`（素の `..`、終端あり） | 半開（今のまま）。`W1013`（`legacy_bare_slice_range`。既定で有効）、修正案 `..<`（`certain`） | **終端を含む**（`..=` と同じ） |
| スライス `a..` | 末尾まで | 末尾まで |
| スライス `a..=b`・`a..<b` | 今のまま | 同じ。警告なし |
| `<..`・`<..<`（スライスと `for`） | `E0002`（`range_form_needs_edition_2028`） | 仕様 1 |
| `^k` | `E0002`（`range_form_needs_edition_2028`） | 仕様 3 |
| 角括弧の中の刻み | `E0002`（`range_form_needs_edition_2028`） | 仕様 4（配列は `E1005`） |
| `for` の素の `..` | 2026: 閉区間（`--warn canonical` で `W1007`）。2027: `E0004` | **閉区間で正準**（警告も拒否もない） |
| A22 のほかの 5 種（短い接尾辞・記号の借用・参照外し・参照型・旧ラムダ） | 2026: `W1007`。2027: `E0004` | `E0004`（一度拒否した表記は拒否し続ける） |
| 負の数を直接書いた境界 | `W1014`（`negative_literal_range_bound`） | `W1014`（`negative_literal_range_bound`） |
| 組み込みの添字の負の整数リテラル（`xs[-1]`） | トラップ。`W1014`（`negative_literal_index`。help は `xs[xs.length - {k}]`） | トラップ。`W1014`（`negative_literal_index`。help は `xs[^{k-1}]`） |

- 2 つの edition が受理するプログラムの意味は、旧来の素の `..` のスライス（承認済みの例外）を除いて同じ。`for` の素の `..` は 2026 と 2028 で同じ意味（2027 は拒否）。
- **宣言する差の全体**（D2・D13）: Phase 2 の PR は、docs/stability.md の 5 つ目の差として、角括弧の中の範囲の構文の族の全体を宣言する。含めるもの: スライスの素の `..` の意味、`<..`・`<..<`・`^k`・角括弧の中の刻み、多成分のキーの範囲・`^k`・単独の `..` の成分（NC18）、組み込みでない型への 1 成分の範囲（NC18 の D8）。2026・2027 ではこれらを拒否するか、今の意味のままにする。2028 のすべての拒否と意味の差は、この PR に入れる。NC18 の 2028 の受理は宣言した差の実装なので、2028 を含むリリースの後に入ってよい。整数の位置と式だけのカンマのキーは、NC18 が全 edition で受理する（NC18 の D1）。全 edition に入る受理の拡張（A40・NC19・NC20 など）は、この宣言と関係なく入れてよい。A39（フィールドの排他借用）は 2027 以降に限るが、宣言済みの 2027 の差（A18 の射影への代入）の中で受理を増やすものなので、これも凍結の規則に反しない。
- 素の `..` の意味は**構文解析がソースの edition で決め**、AST には決めた結果（`RangeEnds`）だけを残す。edition の違うパッケージを結合しても、総称関数をどのパッケージから実体化しても、書いたソースの edition の意味になる（決定事項 D4）。
- 2026・2027 では、NC18 の新しい形のうち単独の `..` の成分と、範囲・`^`・単独の `..` を含むカンマのキーも `E0002`（NC18 が同じ kind を使う）。式だけのカンマのキー（`t[i, j]`）は NC18 が全 edition で受理する（D-66 の edition の扱い）。

### 6. 字句と文法

字句: `Lexer::symbol` の最長一致の表で、`("..", DotDot)` の直後（`<=`・`<<` より前）に `("<..<", LessDotDotLess)`、`("<..", LessDotDot)` の順に足す。空白を挟んだ `a < ..b` は従来どおり（2 トークン。`E0002`）。字句はどの edition でも同じで、edition の判定は構文解析が行う。

```ebnf
index        = term "[" component "]"                (* NC17 は 1 成分。NC18 がカンマを足す *)
component    = slice_range | bound
slice_range  = [ bound ] RANGE_OP [ bound ]
             | [ bound ] ".." step END_OP [ bound ]   (* 2028。角括弧の中の刻み *)
bound        = from_end | expr
from_end     = "^" term                               (* 2028。成分の先頭か範囲の演算子の直後だけ *)
step         = expr
RANGE_OP     = ".." | "..=" | "..<" | "<.." | "<..<"  (* "<.." と "<..<" は 2028 *)
END_OP       = ".." | "..=" | "..<"
for_range    = expr ( ".." | "..=" | "..<" | "<.." | "<..<" ) expr
             | expr ".." step END_OP expr             (* A22 のまま *)
```

制約（違反は `E0002`。仕様 7）: `<..`・`<..<` は始端が要る。`..=`・`..<`・`<..<` は終端が要る（既存の `slice_range_needs_end` に `<..<` の文を足す）。`<..`・`<..<` は刻みを取らない。始端も終端もない 1 成分の `..` は既存の `slice_without_bounds`。角括弧の中のカンマは従来の `expected ']'`（NC18 が変える）。

### 7. 診断

kind はすべて ASCII の `snake_case`・48 文字以内（D-51）。メッセージは英語。`{edition}` はソースの edition、`{form}` は書いたトークン。

| コード | kind | 条件 | メッセージ | 主の位置・help・修正案 |
| --- | --- | --- | --- | --- |
| `E0002` | `range_form_needs_edition_2028` | 2026・2027 のソースの `<..`・`<..<`・前置の `^`（角括弧の中）・角括弧の中の刻み | `'{form}' requires edition 2028`（`^` は `'^' (a position from the end) requires edition 2028`、刻みは `a step inside brackets requires edition 2028`） | トークン（刻みは 2 つ目の範囲の演算子）。help: `set edition = "2028" in Tsuzuri.toml; 'tsuzuri fix --edition 2028' migrates the sources first` |
| `E0002` | `start_open_range_needs_start` | 始端のない `<..`・`<..<` | `'{form}' excludes its start, so it needs one; write '..b' or '..<b' to start at the first element` | 演算子 |
| `E0002` | `start_open_range_with_step` | `<..`・`<..<` と刻みの組み合わせ（`a <.. s .. b`、`a .. s <.. b`。`for` も） | `'{form}' takes no step; start the range at its first value, as in 'first .. step .. last'` | `<..` のトークン |
| `E0002` | `slice_step_down_excludes_end` | 角括弧の中で、刻みが負の整数リテラルで終端の演算子が `..<` | `a slice range that counts down cannot exclude its end; write its last index with '..' or leave the end out` | `..<` |
| `E0002` | `from_end_outside_index` | 成分の先頭と範囲の演算子の直後以外の前置の `^`（全 edition） | `'^' counts from the end only at the start of an index or a slice bound, as in 'xs[^0]' or 'ref xs[1..^1]'` | `^` |
| `E0002` | `from_end_in_expression` | `^` の項の後に `]`・範囲の演算子以外のトークン | `'^' takes one term; parenthesize a computed offset, as in '^(k + 1)'` | 続くトークン |
| `E0002` | `from_end_step` | `^` で始まる刻み | `a step is a count, not a position; it cannot start with '^'` | `^` |
| `E1005` | `array_slice_with_step` | 配列・固定長配列のスライスに刻み | `array slices are contiguous; a range with a step cannot slice an array` | 刻みの式。help: `copy the selected elements with a loop over 'first .. step .. last'` |
| `E1005` | `from_end_needs_length` | 組み込みの添字の型でない値に `^`（NC18 が `Index` の型へ広げる） | `'^' counts from the end of an array, a list, a Vec, a string, a utf8string or a fixed-length array, not of {type}` | `^k` |
| `W1013` | `legacy_bare_slice_range` | 2026・2027 の根のパッケージの User のソースの、終端のある素の `..` のスライス。**既定で有効** | `'..' in this slice excludes its end in edition {edition}; from edition 2028 it includes the end` | `..`。help: `write '..<' to keep excluding the end in every edition, or '..=' to include it`。修正案（`certain`）: `Replace '..' with '..<'` |
| `W1013` | `bare_slice_range_meaning_changed` | Phase 3。edition がマニフェストのないことで `LATEST`（2028 以降）になったソースの、終端のある素の `..` のスライス | `'..' in this slice includes its end: a source without a Tsuzuri.toml is read with edition {edition}` | `..`。help: `write '..<' to exclude the end as before edition 2028, or '..=' to state that it is included; set the edition in Tsuzuri.toml or pass '--edition' to silence this note`。修正案なし |
| `W1014` | `negative_literal_range_bound` | スライスの境界か `^` の被演算子が、負の整数リテラル（値が 0 のリテラルを除く。`-0`・`-0x0`・`-00` など）。全 edition。刻みは対象外 | 境界: `'{text}' is a negative position; slice bounds never count from the end`。`^`: `'^{text}' counts past the end; '^0' is the last element` | リテラル（`-` を含む）。help（2028）: `count from the end with '^': '^0' is the last element, so 'ref xs[..^1]' drops it (NumPy 'a[:-1]')`。help（2026・2027）: `edition 2028 counts from the end with '^'; here compute the position from 'xs.length'` |
| `W1014` | `negative_literal_index` | 組み込みの 6 型（配列・固定長配列・リスト・`Vec`・`string`・`utf8string`）の添字の位置に、負の整数リテラル（整数リテラルに単項の `-` を直接付けた式。値が 0 のリテラルは除く: `-0`・`-0x0`・`-00` など）を書いた。`index_operand` を通る添字のすべて: 読み出し `xs[-1]`、`ref xs[-1]`、2027 以降の代入 `xs[-1] = v`、NC20 の `xs[-1] += v`。括弧は AST に残らないので `xs[(-1)]` も入る。全 edition。NC18 が窓のモードの整数の成分（`ref t[-1, ..]`）に広げる。対象外: `Index` のキー（`map[-1]`、`t[[-1, 0]]`、`m[(-1, 0)]`、NC18 の要素のモードの `t[-1, 0]`）、リテラルでない式（`xs[0 - 1]`、定数の名前、負の値の変数）、`new [T](n, f)` の長さ | `'{text}' is a negative position; an index never counts from the end` | リテラル（`-` を含む）。`{k}` はリテラルの絶対値。help（2028）: `'^0' is the last element: write 'xs[^{k-1}]' for NumPy 'a[-{k}]'`。help（2026・2027）: `write 'xs[xs.length - {k}]'; edition 2028 writes 'xs[^{k-1}]'`。修正案なし（どの位置を意図したか分からない） |

既存の診断の変更:

- `E0004` のメッセージを `'{spelling}' is not allowed in edition {edition}; write '{canonical}'` にする（2027 の文は今と同じ）。
- `slice_range_needs_end` に `<..<` の文 `a slice with '<..<' needs an end bound; write 'ref xs[start<..<end]', or 'ref xs[start<..]' to slice to the end` を足す。
- edition に依存しない help と explain の例は、どの edition でも同じ意味の `..<`・`..=` で書く（`SliceMustBeBorrowed` の `write 'ref xs[start..end]'`（`src/parser.rs:4564`）を `'ref xs[start..<end]'` に、`src/explain/slice_must_be_borrowed.md` の「The range is half-open」を edition の説明に）。

`W1013` と `W1014` は既定で有効な警告で、`--deny-warnings` のビルドを失敗にすることがある（docs/stability.md の「新しい警告が増えることがあります」の範囲）。`W1013`（`legacy_bare_slice_range`）は `W1007` と同じく根のパッケージの `ModuleOrigin::User` のソースだけに出す（std と依存パッケージには出ない）。`W1014` の 2 つの kind は同じ範囲に出す（`W1001` と同じく User の関数とトップレベルのコード）。どちらも修正案を持たず、誤りにはしない（誤りにすると、到達しないコードを含め受理していたプログラムを拒否するので edition が要る）。`negative_literal_index` を `Index` のキーに出さないのは、キーの意味をインスタンスが決めるから（`Map<i64, _>` の `m[-1]` は正しい読み出し）。

### 8. `tsuzuri fix --edition 2028` と `fmt`

- `tsuzuri fix --edition 2028 [--from 2026|2027] PATH` は、次の順で移行する（どれも IR を変えない）:
  1. canonical round: **移行先の edition が拒否する** `W1007` の kind の `certain` の修正案（2026 から: `bare_range_in_for` を除く 5 種）と、移行先が `range_family` で移行元がそうでないときの `W1013`（`legacy_bare_slice_range`）の `..` → `..<`。
  2. A18 の引数の移行（`preserve_parameters`）は、移行元が `borrows_parameters` でなく移行先がそうであるとき**だけ**。2027 → 2028 では行わない。
  3. edition を移行先にして全体を検査し、マニフェストの `edition` を書き換える（今のまま）。
- `///` の中の例は書き換えない（例は別のプロジェクトとして検査されるので、修正案が元のファイルの位置を持たない）。`tsuzuri test --doc` が例に `W1013` を出すので、手で直す（決定事項 D10）。
- `tsuzuri fix PATH`（`--edition` なし）も `W1013` の `certain` の修正案を当てる（G24 の既存の動作）。
- `tsuzuri fmt` は空白だけを整え、範囲の演算子を書き換えない（`..`・`..=`・`..<` を互いに直さない）。`1 <.. 3`、`1 <..< 3` と空白を置き（`..<` と見分けやすくする）、`^` の後には空白を置かない（`xs[^1]`、`ref xs[1 .. ^1]`）。

### 9. マニフェストのないソース（Phase 3）

- マニフェストのないプロジェクト・単独のファイル・`script`・`repl`・文書の例は `Edition::LATEST` で読む（docs/stability.md EDN-007）。`LATEST` を 2028 にすると、これらの `ref xs[0..2]` は黙って 1 要素広がる。そこで、edition が `LATEST` から来たソース（根のパッケージがなく、`--edition` もない）の、終端のある素の `..` のスライスに `W1013`（`bare_slice_range_meaning_changed`）を既定で出す。
- 出すのは `check`・`build`・`run`・`test`・`script` と `repl`。`--edition 2028` を渡すか、マニフェストに `edition` を書くか、`..=`・`..<` で書けば出ない。LSP は出さない（対象外）。
- 期間（決定事項 D9）: `LATEST` を 2028 にした変更を含む最初のリリースを 1 回目と数える。注意を含むリリースが 2 回以上出て、かつその最初のリリースの日から 90 日以上たつまで、注意を取り除かない。リリースは仕様 10 の数え方で数える。
- 注意を取り除くのは別の変更。その PR に、数えたタグ・日付・仕様 10 の `git diff --name-only` の結果を書く。

### 10. リリースの数え方（凍結の規則と注意の期間。Phase 2 で docs/stability.md に書く）

決定事項 D9・D13 の「リリース」は次の規則で数える（D-66 の委任で決定、2026-11-25）。docs/stability.md には利用者向けの定義（最初の 3 項）だけを書き、判定の手順（4 項）は GUIDE D-66 とこのチケットに置く。

1. リリースは、コンパイラを利用者に配布する版。今は、コンパイラを同梱する VS Code 拡張の版（タグ `vsc-v*`）だけが当たる。コンパイラのタグや単独の配布物の版ができたら、それも数える。
2. 同じコンパイラを複数の経路で配るときは 1 回と数える。
3. 前に数えたリリースからコンパイラが変わっていない版は数えない（例: 0.1.8）。コンパイラが前の版と同じ版は、何かを初めて含むリリースにもならない。
4. 判定の手順: `git diff --name-only <前のタグ> <今のタグ> -- src std guide Cargo.toml Cargo.lock` が空なら、コンパイラは変わっていない。材料はバイナリに入るもの（`src/`・`std/`・`guide/`・`Cargo.toml`・`Cargo.lock`）。`src/` が `include_str!`・`include_bytes!` で読むリポジトリのディレクトリは `guide/`・`std/` と、`#[cfg(test)]` の中だけで読む `tests/`（HEAD `858a949c` で確認）。`scripts/toolchain/` や同梱の LLVM だけの変更は数えない。数えない版のコンパイラは前に数えたリリースと同じなので、比べる相手は直前のタグでよい。

この数え方を使う場所: 凍結の規則の「リリースに含めた edition」（S5・D13）、マニフェストのないソースへの注意の期間（仕様 9・D9）、NC03・NC04 を含む最初のリリースの条件（A37 の手順 5・A38・NC16。各チケットの決定のとおり）。

## 設計

### データ構造

```rust
// src/syntax.rs
impl Edition {
    pub const E2028: Self = Self(2028);
    pub const SUPPORTED: &'static [Self] = &[Self::E2026, Self::E2027, Self::E2028]; // Phase 2
    // LATEST・NEW_PROJECT は Phase 3 で E2028。MANIFEST_DEFAULT は E2026 のまま。
    pub fn borrows_parameters(self) -> bool { self >= Self::E2027 } // matches! から変える（Phase 2）
    /// edition 2028 の範囲の構文（D-66）: 終端を含むスライスの素の `..`、`<..`、`<..<`、`^k`、
    /// 角括弧の中の刻み。`>=` は意図（後の edition も同じ）。
    pub fn range_family(self) -> bool { self >= Self::E2028 }
    // rejects_noncanonical は Phase 1 で NoncanonicalKind::rejected_in に置き換えて消す。
}

pub enum TokenKind { /* … */ LessDotDot, LessDotDotLess }

/// 範囲の演算子の綴り。構文解析の中だけで使い、AST には残さない（D4）。
pub enum RangeOp { Bare, Closed, Open, StartOpen, BothOpen }

/// 範囲が含む端。構文解析がソースの edition で決める。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RangeEnds { pub start_open: bool, pub end_closed: bool }

/// 角括弧の中の 1 つの範囲。1 成分のスライスと NC18 の多成分のキーが同じ表現を使う。
pub struct RangeKey {
    pub start: Option<Box<Expr>>, // `^k` は ExprKind::FromEnd
    pub end: Option<Box<Expr>>,
    pub step: Option<Box<Expr>>,  // 2028 の角括弧の中の刻み
    pub ends: RangeEnds,
}

/// `Parser::index_component` が読む 1 成分。NC18 の `ExprKind::MultiIndex` もこれを持つ。
pub enum IndexComponent {
    Expr(Expr),            // 位置。`^k` は ExprKind::FromEnd
    Range(Box<RangeKey>),
    Whole(Span),           // 単独の `..`（1 成分では既存の slice_without_bounds）
}

pub enum ExprKind {
    // `op: RangeOp` を `ends` に置き換える。`for i = a to b` は { false, true }。
    Range { start, step, finish, counted, descending, ends: RangeEnds },
    // `start`・`end`・`op` を `range` にまとめる（variant を小さく保つため Box）。
    Slice { value: Box<Expr>, range: Box<RangeKey>, mutable: bool },
    /// `^k`: 位置 `n - 1 - k`。添字の成分の先頭と範囲の境界にだけ現れる（NC17）。
    FromEnd(Box<Expr>),
}

pub enum NoncanonicalKind {
    /* 既存の 6 種 */
    /// 2026・2027 の終端のある素の `..` のスライス（W1013、既定で有効、修正案 `..<`）。
    LegacyBareSliceRange,
    /// 2028 以降の終端のある素の `..` のスライス。マニフェストのないソースだけに W1013 として出す。
    BareSliceRangeMeaningChanged,
}
impl NoncanonicalKind {
    pub fn warned_in(self, edition: Edition) -> bool;   // 警告にする edition（W1007 は --warn canonical のとき）
    pub fn rejected_in(self, edition: Edition) -> bool; // E0004 にする edition
    // 構文解析が記録するのは warned_in(e) || rejected_in(e) のときだけ。
    pub fn code(self) -> &'static str;                  // "W1007" か "W1013"
}

// src/check.rs
pub enum TypedExprKind {
    ForRange {
        /* 既存の field */
        exclusive: bool,
        /// `start <.. finish`（2028）: 最初の値は start + 1。start が型の最大値なら 0 回。
        start_open: bool,
    },
    /// `^k`: 位置 `n - 1 - k`、`after` なら境界 `n - k`。n は直近の組み込みの添字か
    /// スライスの土台の長さ。組み込みの `Index` の添字とスライスの境界にだけ現れる。
    FromEnd { offset: Box<TypedExpr>, after: bool },
}
```

非正準な表記の edition の範囲（`tests/editions.rs` の表と同じ）:

| kind | 警告する edition（`warned_in`） | 拒否する edition（`rejected_in`、`E0004`） | 記録する edition |
| --- | --- | --- | --- |
| `BareRange`（`for` の素の `..`） | 2026（`W1007`、`--warn canonical`） | 2027 だけ | 2026・2027 |
| `ShortSuffix`・`SymbolicBorrow`・`SymbolicDeref`・`SymbolicReferenceType`・`LegacyLambda` | 2026（`W1007`、`--warn canonical`） | 2027 以降 | すべて |
| `LegacyBareSliceRange`（Phase 2） | 2026・2027（`W1013`、既定で有効） | なし | 2026・2027 |
| `BareSliceRangeMeaningChanged`（Phase 2 で記録、Phase 3 で報告） | 2028 以降（`W1013`、マニフェストのないソースだけ） | なし | 2028 以降 |

Phase 1 では 2028 がないので、`BareRange` の「2027 だけ」と「2027 以降」は同じ結果になる（挙動は変わらない）。

### 構文解析が決める `RangeEnds`

| 文脈 | 綴り | 2026・2027 | 2028 |
| --- | --- | --- | --- |
| `for` | `..`・`..=` | { false, true } | { false, true } |
| `for` | `..<` | { false, false } | { false, false } |
| `for` | `<..`・`<..<` | `E0002` | { true, true }・{ true, false } |
| スライス | `..`（終端あり） | { false, false } と記録 `LegacyBareSliceRange` | { false, true } と記録 `BareSliceRangeMeaningChanged` |
| スライス | `..`（終端なし）・`..=`・`..<` | 終端なしは末尾まで。`..=` は { false, true }、`..<` は { false, false } | 同じ |
| スライス | `<..`・`<..<` | `E0002` | { true, true }・{ true, false } |

`range_op` は網羅的な `match` にする（`_ => Bare` を消す）。`range_operator_ahead` に `LessDotDot`・`LessDotDotLess` を足す。

### 変更表

| Phase | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1 | `src/parser.rs` の `block`・`record`・`computation_block`・`expressions`・`index_or_slice`、`src/parse_control.rs` の `interpolation`・`try_expression` | 入れ子の区切りに入るとき `stop_at_slice_dotdot` も退避して `false` にし、出るとき戻す（B3）。`stop_at_arm`・`stop_at_arrow`・`slice_context`・`stop_at_slice_dotdot` を 1 つの構造体と `enter_delimited`／`leave_delimited` にまとめ、退避の書き忘れを防ぐ。区切りのない `if`・`match` の枝では戻さない（`ref xs[if c then 1 else 2..3]` の今の読み方を保つ） |
| 1 | `src/check.rs` の `index_operand`、新規 `bound_operand` | `bound_operand` は `index_operand` と同じだが、`i64u` を `SaturatingIndex` にする。`+ 1` を足す境界（終端を含む終端）に使う。添字 `xs[i]` と半開の境界は `index_operand` のまま |
| 1 | `src/syntax.rs` の `NoncanonicalKind`、`src/parse_canonical.rs` の `push_noncanonical`、`src/parser.rs` 1239〜1246 行付近、`src/canonical.rs` の `message`・`rejections` | `warned_in`・`rejected_in` を足し、記録は `warned_in` か `rejected_in` が真の edition だけ、拒否は `rejected_in` で決める。`Edition::rejects_noncanonical` を消す。`E0004` の文に実際の edition を入れる |
| 2 | `src/syntax.rs` | `E2028`、`SUPPORTED`、`borrows_parameters` を `>=` に、`range_family`、`RangeOp`・`RangeEnds`・`RangeKey`・`IndexComponent`・`ExprKind` の変更、AST の訪問（1116・1265 行付近の `Slice` は `range` の start・end・step を訪ねる。`FromEnd`） |
| 2 | `src/lexer.rs` の `symbol` | `<..<`・`<..` |
| 2 | `src/parser.rs` の `range_op`・`range_operator_ahead`・`index_or_slice`、新規 `index_component`・`from_end`、`primary`（前置の `Caret`） | 仕様 6・7。`index_component` は 1 成分を `IndexComponent` として読み、`]` か `,` の前で止まる（NC18 が再利用する）。`index_or_slice` は `Expr` を `ExprKind::Index`、`Range` を `ExprKind::Slice`、`Whole` を既存の `slice_without_bounds` にする。edition の判定と `RangeEnds` の決定。記録は新規 `Parser::record_slice_range(token, &RangeKey)`（`src/parse_canonical.rs`）が `LegacyBareSliceRange`／`BareSliceRangeMeaningChanged` を残す |
| 2 | `src/parse_control.rs` の `range_expression` | `<..`・`<..<`、`RangeEnds`、`start_open_range_with_step`、2026・2027 の `E0002`。`record_bare_range` は `BareRange` を警告か拒否する edition（2026・2027）でだけ記録する |
| 2 | `src/check.rs` の `ExprKind::Slice` の入口・`slice`・組み込みの `Index`・`ExprKind::FromEnd`、`src/control.rs` の `for` | `ends` を読む。始端 `start_open` は巻き戻る `+ 1`、終端 `end_closed` は巻き戻る `+ 1`（どちらも `bound_operand`。`+ 1` を足さない境界は `index_operand` のままで IR を変えない）。境界か添字が `FromEnd` なら `TypedExprKind::FromEnd { offset: bound_operand(k), after }`（始端は `after = start_open`、終端は `after = end_closed`、添字は `false`）。刻みは `E1005`。`^` が組み込みの添字の型でなければ `from_end_needs_length`。`W1014` は新規 `Checker::negative_bound_warnings(&RangeKey)`（と `^` の被演算子）が新規の `Checker::range_warnings` に集め、`W1001` を出す `src/warnings.rs` と同じく User の関数の警告として `module.warnings`（`CheckedModule::warnings`、`src/check.rs` 4995 行付近）に足す。`negative_literal_index` は、組み込みの添字の型付け（`ExprKind::Index` の `built_in_ty` の枝、14762〜14764 行付近）で、添字の式に同じ判定（単項の `-` を直接付けた整数リテラルで、値が 0 でない）を当てて同じ場所に集める。判定は `index_operand` の中に置かない（`new [T](n, f)` の長さとスライスの境界も通るので、長さに誤って出し、境界には `negative_literal_range_bound` と二重に出る）。`ForRange.start_open` |
| 2 | `src/check.rs` の `children`・`children_mut` と、`SaturatingIndex` を扱うすべての走査（`src/const_machine.rs`・`src/const_rules.rs`・`src/effects.rs`・`src/kg/sites.rs`・`src/last_use.rs`・`src/ownership.rs`・`src/prove_smt.rs`・`src/pure_rules.rs`・`src/semantic.rs`・`src/warnings.rs`） | `FromEnd` を、子を 1 つ持つ純粋な（トラップしない）式として扱う。トラップするのは親の境界検査。`const` の中は `E1026`（`ConstOperationNotAllowed`） |
| 2 | `src/llvm.rs` の `array_slice` と組み込みの添字の 5 か所（6507・6528・6548・6700・6705 行付近） | 新規 `bound_value(bound, length)`: `FromEnd { offset, after }` なら `t = sub i64 length, k`、`after` でなければ `sub i64 t, 1`。ほかは `self.expression(bound)`。添字の 5 か所は、添字が `FromEnd` のときだけ長さを先に読んで位置を作り、既存の境界検査（`icmp ult`）に渡す。`FromEnd` が `expression` に直接来たら `unreachable!` |
| 2 | `src/llvm_control.rs` の `range_loop`、`src/ranges.rs`（277〜296 行付近）、`src/const_machine.rs`（850〜876 行付近） | `start_open`: `a` が型の最大値なら 0 回、そうでなければ `a + 1` から。範囲の事実は `start` をそのまま使ってよい（実際の値域を含む上位集合） |
| 2 | `src/canonical.rs` | `KINDS` に `W1013` の 2 つ（14 個）。`warnings` は `code() == "W1007"` だけ。新規 `default_warnings`（`LegacyBareSliceRange` → `W1013` と修正案）と `meaning_changed_notes`（Phase 3） |
| 2 | `src/check.rs` の `CheckedModule` の組み立て（10417 行付近） | `root_noncanonical` の結果から `default_warnings` を `module.warnings` に足す（`(source, start)` 順）。LSP・`fix`・`tsuzuri test --doc` もそのまま見る |
| 2 | `src/diagnostic_kinds.rs` | 仕様 7 の 13 個 |
| 2 | `src/explain.rs`、`src/explain/codes/` | `W1013.md`（2 つの kind を「What it covers」に書く）・`W1014.md`（2 つの kind と、`Index` のキーを外す理由を書く）を足し、`CODE_PAGES` に登録する（`tests/explain.rs::every_code_has_a_page` の見出し・修正の数・1500 字の規則） |
| 2 | `src/fix.rs` の `canonical_round`、`src/edition_migration.rs` の `prepare` | `canonical_round` に移行先の edition を渡し、仕様 8 の kind だけを当てる（`only_codes` に `W1013`）。`preserve_parameters` を移行元・移行先の `borrows_parameters` で囲む |
| 2 | `src/formatter.rs` | 前置のヒントに `Caret`（`FromEnd` の `^` の位置を `hints.prefixes` に入れる）。`<..`・`<..<` は既定の空白 |
| 2 | `src/syntax_codec.rs`、`src/frontend_cache.rs` | `RangeEnds`・`RangeKey`（`Slice` の `range`）・`FromEnd`・`NoncanonicalKind` の 2 つ。全タグの見本の数（`ExprKind` 50、`RangeOp` は AST から消えるので表から外す）。`FRONTEND_FORMAT` を 1 上げる |
| 2 | `src/main.rs` | `--edition 2026\|2027\|2028` の文（68・287・2963 行付近） |
| 2 | `src/guide.rs` の `filter_edition`、`scripts/check-guide.mjs`、`guide/*.md`・`guide/*.tmpl`、`tests/fixtures/guide_editions/` | `<!--if edition>=YYYY-->` を足し、今の `edition=2027` のブロックを `edition>=2027` にする。`EDITIONS` に `'2028'`。2028 のブロックで新しい形を教える（D-56 の上限内） |
| 2 | `scripts/check-docs.mjs` | 例の属性 `edition=YYYY`（`project`・`file`・`run` に足す）。付いた例は一時プロジェクトに `[package]` と `edition = "YYYY"` の `Tsuzuri.toml` を書いてから `check`・`run`・`test` する。値が対応外なら検査の誤り。同じ `project` の例は同じ `edition` |
| 3 | `std/*.tz`、`tests/`、`examples/`、`benchmarks/`、文書の `tsuzuri` ブロック、Rust・JavaScript の文字列の中のプログラム | 終端のある素の `..` のスライスを `..<` に（IR は同じ）。A22 と同じく、構文解析の記録 `LegacyBareSliceRange` の編集を当てる一時的な道具で書き換え、IR を比べる |
| 3 | `src/syntax.rs` | `LATEST = E2028`、`NEW_PROJECT = E2028` |
| 3 | `src/main.rs`（4300 行付近、`--warn canonical` の警告を組み立てる場所）、`src/repl.rs` | 根のパッケージがなく `--edition` もないとき、`canonical::meaning_changed_notes` を警告に足す |

### 境界の計算と生成 IR

- `ref xs[a..b]`（2028）は今の `ref xs[a..=b]` と同じ型付きの木（終端 `b + 1`）。`ref xs[a..<b]` は今と同じ。`ref xs[a<..b]` は始端にも `add i64 %a, 1` が 1 つ増える。
- `ref xs[..^k]` は `sub i64 %n, %k`、`ref xs[^k..]` は `sub i64 %n, %k` と `sub i64 %t, 1`。`xs[^k]` は位置 `n - 1 - k` を既存の `icmp ult i64 %pos, %n` で検査する。
- 範囲の事実（`ranges.rs` の `index_in_bounds`）は `FromEnd` を証明しない（今の `_ => false`）。境界検査を省くことは主張しない。性能の主張もしない。

### 正しさの根拠（境界の規則を既存の符号なしの検査で正確に判定できること）

- 境界は、1 つの被演算子 v（a・b・k）から高々 2 回の巻き戻る i64 の加減算（`+ 1`、`n - k`、`- 1`）で作る。v は i64 の正確な値か、i64 に収まらないとき `SaturatingIndex` で `i64::MIN`／`i64::MAX` に飽和した値。
- 正確な境界 E は常に (−2^64 + n, 2^64) の中にある（`b + 1` ∈ [MIN+1, 2^63]、`n − k` ∈ [n − MAX, n + 2^63]、`n − 1 − k` ∈ [n − 1 − MAX, n − 1 + 2^63]。n < 2^63）。この範囲では「巻き戻った値を符号なしで読んだもの ∈ [0, n]」⇔「E ∈ [0, n]」。lo も同じ範囲にあるので、`hi ≤u n` の後の `lo ≤u hi` は「0 ≤ lo ≤ hi」と同値。よって既存の `lo ≤u hi ∧ hi ≤u n`（`src/llvm.rs` 7050〜7052 行）がそのまま `0 ≤ lo ≤ hi ≤ n` を判定する。
- 飽和した値から作った境界は必ず無効になる（`MAX + 1` → 2^63 > n、`n − MAX` < 0、`n − 1 − MIN` > n など）。前提は n < `i64::MAX`（プローブ `Huge`: 長さ `i64.max` の配列は確保できない）。
- `i64u` をビットのまま使うと、`2^64 − 1` が −1 になり `+ 1` で 0（有効）に化ける（今の不具合）。だから**派生した境界**（終端を含む終端、始端を含まない始端、`^` の被演算子）の `i64u` だけを飽和させる。半開の境界はビットのままでも正しい（負になった値は符号なしで n を超える）ので IR を変えない。

## 前提とする他チケットのインターフェース

- [A22](_completed/A22-edition-2027-canonical-forms.md): `Program.noncanonical`・`Noncanonical`・`FixLevel`・`TextEdit`、`canonical::{warnings, rejections, KINDS}`、`W1007`／`E0004`、`--warn canonical`、`fix::canonical_round`。
- A18（完了）と G24（完了）: `edition_migration::prepare`、`fix --edition YEAR --from YEAR`、`preserve_parameters`。
- [NC04](_completed/NC04-generic-index-class-and-slice.md): `xs[-1]` はトラップ（D4）、`Index` クラスの解決（このチケットは組み込みの添字だけを扱う）。
- docs/stability.md「edition を追加するとき」（93〜100 行）。

## 他チケットへ提供するインターフェース

[NC18](NC18-multi-axis-bracket-indexing.md) が使う:

- 字句 `TokenKind::{LessDotDot, LessDotDotLess}`、網羅的な `range_op`、`RangeEnds`、`Edition::range_family`。
- AST `ExprKind::FromEnd(Box<Expr>)`（`^` の成分）、`RangeKey { start, end, step, ends }`、`IndexComponent { Expr, Range(Box<RangeKey>), Whole(Span) }`、`ExprKind::Slice { value, range, mutable }`。
- `Parser::index_component() -> IndexComponent`: 1 成分（式・`^k`・範囲・刻み付きの範囲・単独の `..`）を読み、`]` か `,` の前で止まる（`,` の直後の `^` も成分の先頭として読む）。NC18 はカンマで繰り返す。
- `Parser::record_slice_range(token, &RangeKey)`（`W1013` の記録。NC18 は 2028 の多成分のキーの範囲の成分にも呼び、終端のある素の `..` に `BareSliceRangeMeaningChanged` を残す）と `Checker::negative_bound_warnings(&RangeKey)`（`W1014` の `negative_literal_range_bound`）。
- `W1014` の kind `negative_literal_index` と負の整数リテラルの判定。NC18 は、窓のモードの整数の成分（`Slice.Sel.Item (Slice.Pos.Front k)` に下がるもの。`ref t[-1, ..]`）に、キーを受けるインスタンスによらず同じ kind を出す。要素のモードのキー（`t[-1, 0]`）には出さない。
- docs/stability.md の 5 つ目の差の宣言（仕様 5）。NC18 の 2028 の受理はこの宣言の範囲で入れる。宣言にない差が要れば S5 に当たる。
- 仕様 10 のリリースの数え方（A37・A38・NC16 のリリースの条件も使う）。
- `scripts/check-docs.mjs` の例の属性 `edition=YYYY`（`LATEST` が 2027 の間に 2028 の例を検査・実行する手段）。
- `range_form_needs_edition_2028`（E0002）: 2026・2027 の単独の `..` の成分と範囲を含むカンマのキーにも使う。
- `from_end_needs_length`（E1005）: NC18 が `Index` の型で `^` を許すときに条件を狭める（`Index` の型には NC18 自身の kind を使ってよい）。
- 境界の規則（仕様 2・4）と独立参照 `tests/range-reference-cases.mjs`（`rows`・`bounds`・`item`・`stepped`）。NC18 はテンソルの列を足すだけで、期待値を変えない。
- `Checker::bound_operand`（派生する境界の整数の変換）。

## 実装手順

### Phase 1: 不具合の修正と、種類ごとの edition の範囲（全 edition）

1. **変更**: `tests/range_family.rs`（新規）、`src/check.rs`（`bound_operand`、`slice` の終端を含む終端）。**内容**: `i64u` の終端を含む終端を `SaturatingIndex` で変換する。**確認**: T0。`inclusive_slice_end_of_i64u_max_traps`（`ref xs[0..=18446744073709551615i64u]` が `BoundsCheck`、`ref xs[0..=4i64u]` が全部）、`plain_index_of_i64u_keeps_its_ir`（`xs[i]` の IR が変更前と同じ）、`cargo test --locked --test range_forms`。
2. **変更**: `src/parser.rs`・`src/parse_control.rs`（入れ子の区切りの旗）。**内容**: B3 を直す。旗を 1 つの構造体にまとめる。**確認**: `src/parser.rs` の単体テスト `for_range_inside_parentheses_in_brackets_parses`（for-in.md 127 行の例、ブロック、呼び出しの引数、括弧の中のラムダの本体、配列リテラル、補間、レコード、`try` の中の `for`）と、`ref xs[if c then 1 else 2..3]` の構文木が変わらないこと、`node scripts/slot.mjs -- cargo nextest run --locked -E 'test(parser)'`。
3. **変更**: `src/syntax.rs`・`src/parse_canonical.rs`・`src/parser.rs`・`src/canonical.rs`、`tests/editions.rs`。**内容**: `warned_in`・`rejected_in`、`rejects_noncanonical` の削除、`E0004` の文の edition。**確認**: `tests/editions.rs::noncanonical_kinds_have_edition_ranges`（kind × edition の表）、`cargo test --locked --test edition_2027 --test canonical_forms`（期待値は変えない）。
4. **変更**: 文書。**内容**: for-in.md と docs/language.md の B3 の既知の不具合を消し、array.md の `i64u` の記述に終端を含むスライスを足す。**確認**: T1、T2（`node tests/features.mjs target/debug/tsuzuri range_forms`、`node scripts/check-docs.mjs <変えたページ>`）、fixture のコーパスの IR が変わらない（S1）。

### Phase 2: edition 2028 と範囲の構文

1. **変更**: `src/syntax.rs`、`tests/editions.rs`、`docs/stability.md`、`src/main.rs`。**内容**: edition の追加の手順どおり `E2028`・`SUPPORTED`（`LATEST` は 2027 のまま）、`borrows_parameters` を `>=`、`range_family`、`--edition` の文。docs/stability.md は、edition の表、5 つ目の差の宣言（範囲の構文の族の全体。仕様 5、決定事項 D2・D13）、明確にした凍結の規則、2027 の経緯（D13）、小節「リリースの数え方」（仕様 10 の 1〜3）を書く。**確認**: `edition_names_parse_exactly`・`the_edition_key_changes_no_generated_code` を 2028 に広げる（2027 と 2028 で範囲を使わないプログラムの IR が同じ）、`cargo test --locked --test parameter_conventions --test edition_migration`。
2. **変更**: `src/guide.rs`・`scripts/check-guide.mjs`・`guide/`・`tests/fixtures/guide_editions/`。**内容**: `<!--if edition>=YYYY-->`。**確認**: `cargo test --locked --test guide`、`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-guide.mjs`（スクリプトは引数を読まず、`TSUZURI_BIN` がなければ `target/release/tsuzuri` を使う）。
3. **変更**: `src/lexer.rs`・`src/syntax.rs`・`src/parser.rs`・`src/parse_control.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`。**内容**: トークン、`RangeOp`・`RangeEnds`（AST の `op` を置き換え、型検査（`src/check.rs` 14537 行付近、`src/control.rs:259` 付近）は `ends` を読む。2028 の素の `..` のスライスはこの段で終端を含む）、`range_op` を網羅的に、`range_operator_ahead`。2026・2027 の新しい形は `range_form_needs_edition_2028`。2028 での新しい形の受理は次の段で入れる（この段では 2028 でも一時的に `E0002`）。**確認**: `tests/range_family.rs::lexer_longest_match_for_new_range_operators`（`a<..b`・`a<..<b`・`a < ..b`・`<<`・`<=`・`..<`）、コーパスの IR が変わらない、`syntax_codec` の全タグの見本。
4. **変更**: `src/parser.rs`（`index_component`・`from_end`・`primary`）、`src/parse_control.rs`。**内容**: 2028 の受理（仕様 6）と `E0002` の 7 種。**確認**: `new_forms_need_edition_2028`、`from_end_is_only_a_component_start`（`xs[k ^1]` が XOR のまま）、`malformed_range_family_forms`。
5. **変更**: `src/check.rs`・`src/control.rs`・`src/llvm.rs`・`src/llvm_control.rs`・`src/ranges.rs`・`src/const_machine.rs`・`src/const_rules.rs` と走査の各ファイル。**内容**: 型付けと生成（変更表）。**確認**: `bare_slice_in_2028_has_the_ir_of_a_closed_slice`、`start_open_for_loops`（`i8` の `127 <.. 127` が 0 回、`0 <..< 3` が 1, 2）、`stepped_ranges_in_array_brackets_are_e1005`、`from_end_needs_a_builtin_length`、`cargo test --locked --test range_forms --test mutable_slices`。
6. **変更**: `tests/range-reference-cases.mjs`・`tests/fixtures/range_reference/`・`tests/features.mjs`。**内容**: 参照表の suite（「テスト計画」）。**確認**: `node tests/features.mjs target/debug/tsuzuri range_reference`（native と WASM の `-O0`・`-O3`）、`node tests/compact_strings.mjs target/debug/tsuzuri` ほかの compact-strings の suite（文字列の `s[^k]`）。
7. **変更**: `src/canonical.rs`・`src/check.rs`・`src/diagnostic_kinds.rs`・`src/explain.rs`・`src/explain/codes/W1013.md`（新規）・`src/explain/codes/W1014.md`（新規）。**内容**: `W1013`（`legacy_bare_slice_range`）を既定で有効に、`W1014` の 2 つの kind。`negative_literal_index` は `negative_literal_range_bound` と同じ判定を組み込みの添字に当てる（変更表）。警告がないことを表明する既存のテストのプログラムは、期待値ではなくプログラムを `..<` に書き換える（IR は同じ）。負の整数リテラルの添字でトラップを起こすテストに警告が出たら、添字を変数にする。**確認**: `legacy_bare_slice_range_warns_with_a_certain_fix`（2026・2027 で出る、2028 と `a..` には出ない、修正案を当てると IR が同じ）、`negative_literal_bounds_warn`、`negative_literal_index_warns`（「テスト計画」）、`cargo test --locked --test canonical_forms --test diagnostic_v2 --test explain`、`node scripts/gen-docs.mjs`。
8. **変更**: `src/fix.rs`・`src/edition_migration.rs`、`tests/edition_2028.rs`（新規）。**内容**: 仕様 8。**確認**: `fix_2027_to_2028_adds_no_own`、`fix_2026_to_2028_keeps_the_bare_for_range`、`corpus_2027_migrates_to_2028_without_changing_the_ir`（`tests/fixtures/canonical_2026` と、素の `..` のスライスを含む 2027 の fixture）。
9. **変更**: `src/formatter.rs`。**内容**: `^` と `<..` の空白。**確認**: `cargo test --locked --test formatter`、`fmt_spacing_of_the_range_family`。
10. **変更**: `scripts/check-docs.mjs`（例の属性 `edition=YYYY`。値は `Edition::SUPPORTED` の名前だけ）、`spec/rules/`・`tests/conformance/`、文書（「ドキュメント」の Phase 2）。**確認**: T1、T2（`node scripts/conformance.mjs target/debug/tsuzuri --area EDN`（CTL・DIA も）、`node scripts/check-spec.mjs --require-markers`、`node scripts/check-docs.mjs <変えたページ>`、`node scripts/check-cli-docs.mjs`、`node scripts/gen-docs.mjs --check`）。

### Phase 3: 移行と `LATEST` の移動

1. **変更**: `std/*.tz`、`tests/`、`examples/`、`benchmarks/`、文書の `tsuzuri` ブロック、Rust・JavaScript の文字列の中のプログラム、`guide/`。**内容**: 一時的な道具で `LegacyBareSliceRange` の編集を当てる（std の `///` の例も）。A22 の古い意味を固定する fixture（`tests/fixtures/range_forms/` の `slice_bare` など）はマニフェストに `edition = "2027"` を書いて残す（マニフェストを足すと suite が関数を見つけられないなどの事情があれば、`slice_bare` の期待値を 2028 の意味に移し、2027 の半開は `tests/range_family.rs` で固定する）。**確認**: 書き換えの前後で fixture・examples・ベンチマークの `--emit llvm -O0` が 1 byte も同じ（S2）、`tsuzuri test --doc std`、T1。
2. **変更**: `src/syntax.rs`、`tests/editions.rs`、`spec/rules/EDN.md`。**内容**: `LATEST = E2028`、`NEW_PROJECT = E2028`。この変更を含む最初のリリース（仕様 10）が、注意の期間の 1 回目になる。EDN-005（`edition = "2027"` を書く）を `retired` にして 2028 の規則を足す。**確認**: `new_projects_declare_edition_2028`、`latest_cli_and_synthetic_inputs_…` の期待値、`cargo test --locked --test kg_std --test kg_brief --test mcp_knowledge --test guide`（`LATEST` の文字列を含む期待値の更新は、この段の既知の影響）。
3. **変更**: `src/canonical.rs`（`meaning_changed_notes`）、`src/main.rs`、`src/repl.rs`。**内容**: 仕様 9。**確認**: `tests/edition_2028.rs::manifest_less_sources_note_the_meaning_change`（マニフェストなしで出る、`--edition 2028`・マニフェスト・`..=`・`..<` で出ない）。
4. **変更**: 文書（「ドキュメント」の Phase 3）。**内容**: Phase 2 で `edition=2028` を付けた例から属性を外し、マニフェストのない検査でも同じ結果になることを確かめる。**確認**: T2（`node scripts/check-docs.mjs` に変えたページ、`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-guide.mjs`、`node tests/features.mjs target/debug/tsuzuri`、`node tests/examples.mjs target/debug/tsuzuri`）。

## テスト計画

- **Rust（`tests/range_family.rs`、新規）**: 字句の最長一致、2026・2027 の `E0002`（新しい形ごとに 1 件、位置付き）、2028 の受理、`E0002` の 7 種と `E1005` の 2 種の位置とメッセージ、`W1013`・`W1014` の有無と修正案、`^` の位置の規則と XOR（`xs[k ^1]`、`xs[^1 ^ 2]` は `E0002`）、`ref xs[a..b]`（2028）と `ref xs[a..=b]`（2027）の IR が同じ、`ref xs[a..<b]` の IR がどの edition でも同じ、`i64u`・`i128` の境界、`@checked` の中のスライスでも合成した `+ 1`・`n - k` が例外を出さないこと、`for` の `<..`・`<..<`（始端が型の最大値なら 0 回で、`a + 1` を計算しない）、edition の違うパッケージ（2027 の依存の素の `..` のスライスを 2028 の根が使っても半開、2027 の総称関数を 2028 から実体化しても半開）。
- **Rust（`tests/range_family.rs::negative_literal_index_warns`）**: `W1014`（`negative_literal_index`）が、配列・固定長配列・リスト・`Vec`・`string`・`utf8string` の読み出し `xs[-1]`、`ref xs[-1]`、2027 の代入 `xs[-1] = v`、`xs[(-1)]` に、リテラルの位置で出る。メッセージは全 edition で同じで、help は 2026・2027 が `xs[xs.length - {k}]`、2028 が `xs[^{k-1}]`（`xs[-1]` なら `^0`、`xs[-3]` なら `^2`）。修正案はない。出ない: `xs[-0]`・`xs[-0x0]`・`xs[-00]`、`xs[0 - 1]`、負の値の変数 `xs[i]`、定数の名前、`Map<i64, _>` の `m[-1]`、`t[[-1, 0]]`、利用者の `Index` の型のキー、`new [i64](-1, f)` の長さ（負の長さは今と同じくトラップする）。スライスの境界 `ref xs[..-1]` には `negative_literal_range_bound` だけが出て、`negative_literal_index` は出ない。警告が出ても `xs[-1]` は今と同じくトラップし、IR は変わらない。std と依存パッケージの関数には出ない。NC20 は `xs[-1] += 1` の場合を、NC18 は窓の成分の場合を足す。
- **Rust（`tests/editions.rs`）**: `noncanonical_kinds_have_edition_ranges`（kind × edition → なし・`W1007`・`E0004`・`W1013`）、2028 の名前と `--edition 2028`、`the_edition_key_changes_no_generated_code` の 2028。
- **Rust（`tests/edition_2028.rs`、新規）**: 2028 が A22 の 5 種を `E0004`（`edition 2028` の文）にし、`for` の素の `..` を受理する。`fix --edition 2028` の 3 つの確認（Phase 2 の手順 8）。コーパスの移行で IR が同じ（Phase 3）。マニフェストのないソースの注意（Phase 3）。
- **参照表（`tests/range-reference-cases.mjs`、新規）**: 仕様 1 の全行をデータとして持つ（形・n・期待する (lo, hi) かトラップ・要素の行の位置）。独立参照 `bounds(n, form)`・`item(n, k, fromEnd)`・`stepped(n, form)` は BigInt で正確に計算し、まず表の行そのものと一致することを表明する。そのうえで、形ごとに型（`i32`・`i64`・`i64u`・`i128`）と値（0、1、n−1、n、n+1、−1、型の最大値・最小値、`i64u` の `2^64 − 1`）と n（0、1、5）を組み合わせた行を作る。コンテナの列は、配列（スライスと要素）、共有の固定長配列（スライスと要素）、リスト・`Vec`・`string`・`utf8string`（要素の行だけ）。テンソルの列と刻みの行は `pending: "NC18"` と書き、どのコンテナも回さない行が `pending` 以外に残らないことを表明する。
- **E2E（`tests/features.mjs` の suite `range_reference`、fixture `tests/fixtures/range_reference/`、`edition = "2028"`）**: 形ごとの関数（`closed`・`open`・`open_closed`・`open_open`・`to`・`to_open`・`from`・`from_open`・`drop_end`・`from_end_start`・`from_end_start_open`・`between_from_end`・`until_from_end`・`item_from_end` を、型ごとに）が、土台 `let xs = new [i64](n, \i -> (i + 1) * 10)` の選んだ要素を `t * 31 + v` で畳んだ値か、トラップ（`index out of bounds`）を返す。native と WASM の `-O0`・`-O3`、`live == 0`。`inspect` で `ref xs[a..b]` の IR に `add i64 %…, 1` があること。
- **既存の suite**: `range_forms`（A22 の値の表。Phase 3 でマニフェストを 2027 に）、compact-strings の suite（文字列の添字の経路を変えるので）、`node tests/control.mjs`（`for`）。
- **単体**: `src/parser.rs` の B3、`src/canonical.rs` の `kinds_are_unique_and_short`（コードに `W1013`）と `messages_follow_the_templates`（edition の文）、`src/syntax_codec.rs` の全タグの見本。
- **規則（`spec/rules/`）**: 2028 の受理（EDN）、2028 のスライスの素の `..` が終端を含む、`^k` の位置、境界の規則のトラップ、`for` の `<..`（CTL）、`W1013`・`W1014` の 2 つの kind（DIA）。規則の文は「In edition 2028, 」で始めてよい（`spec/README.md` の接頭辞の一覧に 2028 を足す）。番号は実装の時点の各領域の次の空きにし、このチケットには固定の番号を書かない。
- **既存のテストへの影響**: 警告がないことを表明するテストのうち、2026・2027 で素の `..` のスライスを書くもの（プログラムを `..<` に書き換える）。負の整数リテラルの添字は、HEAD では `src/ranges.rs:699` の単体テスト 1 件だけで、警告を確かめないので影響しない。`LATEST`・`NEW_PROJECT` の値や `edition 2027` の文字列を含む期待値（Phase 3。`tests/editions.rs`・`tests/kg_*.rs`・`tests/mcp_knowledge.rs`・`tests/guide.rs` など）。`--edition` の値の一覧を含む文（`2026, 2027` → `2026, 2027, 2028`）。それ以外の期待値を変える必要が出たら止める（GUIDE-LITE §9）。
- **性能**: 主張しない。生成 IR の形だけを確かめる。

## ドキュメント

### Phase 1

- `_tsuzuri/language-reference/loops-and-conditionals/for-in.md`（126〜127 行の既知の不具合を消す）、`docs/language.md`（2814 行の既知の不具合を消す）。
- `_tsuzuri/language-reference/built-in-types-and-modules/array.md`（境界外の節 165〜169 行付近に、終端を含むスライスの `i64u` の終端もトラップすることを足す）、`values-and-functions/op-and-expressions.md`（246 行の `..=` の終端の説明）。

### Phase 2

2028 だけで通る例には、この Phase で `scripts/check-docs.mjs` に足す例の属性 `edition=2028` を付ける（`LATEST` が 2027 の間も、一時プロジェクトに `edition = "2028"` の `Tsuzuri.toml` を書いて検査・実行する）。

- `built-in-types-and-modules/array.md`「共有スライス」「排他スライス」: 基準表、edition の違い（2026・2027 は半開と `W1013`）、`^k`、境界の規則、NumPy との対応。`numpy-migration.md` があればそこにも対応表を置く（NC07 が作る予定。なければ array.md だけ）。
- `loops-and-conditionals/for-in.md`: 2028 の素の `..` が正準、`<..`・`<..<`、125 行付近の NOTE の書き換え。
- `values-and-functions/op-and-expressions.md`（245〜246 行の表に `<..`・`<..<`・前置の `^`）、`values-and-functions/tokens.md`（30〜31 行の表と 117 行の文）、`built-in-types-and-modules/string.md`（`s[^k]`、範囲の構文だけが既定で終端を含む）、`slice.md`（`Slice.range` は半開で、範囲の構文とは違う）、`vec.md`・`list.md`（`^k`）。
- `W1014`（`negative_literal_index`）: `built-in-types-and-modules/array.md`「境界外アクセスとトラップ」（165 行付近）、`string.md`・`utf8string.md`・`list.md`・`vec.md` の添字の説明に、負の整数リテラルの添字はトラップのままで警告が出ること、2028 の書き方 `xs[^0]` を書く。`src/explain/codes/W1014.md` に、`Index` のキー（`map[-1]`、`t[[-1, 0]]`）に出さない理由（キーの意味はインスタンスが決める）を書く。
- `compiler/diagnostics.md`（285 行のコードの範囲、292 行の `E0004`、439 行付近の警告の表、`node scripts/gen-docs.mjs` の生成ブロック）、`compiler/usage.md`（341 行の `--edition`、610〜640 行の `fix --edition 2028`）、`organizing-tsuzuri/packages.md`（50 行と 58〜65 行の edition）、`values-and-functions/keywords.md`（44 行。予約語は増えない）。
- `docs/stability.md`:
  - edition の表に 2028。
  - 「edition の差として許すのは、次の 4 つだけです」（72〜77 行）を 5 つにし、5 つ目に角括弧の中の範囲の構文の族の全体を書く（文は決定事項 D2。NC18 の多成分のキーの範囲の成分と、`Index` の型への 1 成分の範囲を含む。2026・2027 の振る舞いも書く）。
  - 「両方の edition が受理するプログラムの意味と生成 IR は同じ」（81 行）の例外に、素の `..` のスライスを足す。
  - 100 行の「リリースに含めた edition は凍結し、差を足しません。」を、明確にした凍結の規則に置き換える（文は決定事項 D13）。
  - 2027 の経緯の注記（0.1.5 → 0.1.6。承認を得た一度きりの例外。文は決定事項 D13）。
  - 小節「リリースの数え方」（仕様 10 の 1〜3。判定の `git diff` の手順は書かない）。
- `docs/language.md`（123 行、317〜324 行、2808〜2812 行、3070 行（負の添字に `W1014` が出ること）、3082・3109 行）、`docs/architecture.md`（56 行の `src/canonical.rs` の行。範囲の意味は構文解析で決める不変条件）、`README.md`（CLI の概要の `--edition`）、`spec/README.md`。
- GUIDE D-15（`<..`・`<..<`・角括弧の中の前置 `^`）、D-54（A22 は `..` の意味を変えない → D-66c で改訂の注記）、D-55（`W1013`・`W1014`、edition 2028）は、起票の PR（2026-11-25 の統合）で記録済み。`W1014` の kind `negative_literal_index`（NC18 が窓の成分へ広げる）、凍結の規則の明確化、リリースの数え方の手順は、D-66 の委任による決定として GUIDE D-55・D-66 に記録した（決定を記録する PR で入れた。このチケットの Phase 2 では、記録と実装が食い違わないことだけを確かめる）。GUIDE-LITE §10 の「`for x in a .. b` は両端を含む（スライス `xs[a..b]` は半開）」は今の振る舞いの説明なので、2028 のスライスの意味が入る Phase 2 の PR で「edition 2028 では両方とも終端を含む。2026・2027 のスライスは半開」に直す（`node scripts/check-guide-lite.mjs` を通す）。

### Phase 3

- 上の各ページの例の `edition=2028` は外してよい（`LATEST` で同じ結果になる）。`docs/stability.md`・`packages.md`・`usage.md`・`docs/language.md` の「現在の最新」「新規プロジェクト」を 2028 に。マニフェストのないソースの注意（`W1013` の 2 つ目の kind）を `diagnostics.md` と docs/stability.md の EDN-007 の行（66 行）の近くに書く。docs/stability.md には、注意を取り除く計画（仕様 9 の期間。`LATEST = 2028` を含む最初のリリースの版と日付）も書く。版と日付はそのリリースが出た後に書き足す。`guide/`（2028 のブロックを既定として読めるように）。

## 受け入れ条件

- [ ] Phase 1: `ref xs[0..=18446744073709551615i64u]` が native と WASM の `-O0`・`-O3` でトラップし、`xs[i]`（`i64u`）の IR は変わらない。添字の角括弧の中の括弧・ブロック・呼び出し・配列リテラル・補間の中の範囲の `for` が通る。非正準な表記の kind × edition の表のテストが通り、2026・2027 の挙動が変わらない。
- [ ] Phase 2: 仕様 1 の基準表の配列・固定長配列・文字列の行が参照表の suite で native と WASM の `-O0`・`-O3` で一致し、独立参照と型の境界の組み合わせ（`i64u`・`i128` の範囲外を含む）も一致する。
- [ ] Phase 2: 2026・2027 は新しい形をすべて `range_form_needs_edition_2028` にし、新しい形を使わないプログラムの IR は 1 byte も変わらない。2028 の `ref xs[a..b]` は 2027 の `ref xs[a..=b]` と同じ IR。
- [ ] Phase 2: `W1013`（`legacy_bare_slice_range`）が 2026・2027 の根のソースにだけ出て、修正案を当てても IR が同じ。`W1014` の `negative_literal_range_bound` が仕様どおりに出る。
- [ ] Phase 2: `W1014` の `negative_literal_index` が、組み込みの 6 型の添字の負の整数リテラル（`xs[-1]`、`ref xs[-1]`、2027 の `xs[-1] = v`、`xs[(-1)]`）に全 edition で出て、help が edition で変わる。`-0`・`-0x0`、`xs[0 - 1]`、`Map<i64, _>` の `m[-1]`、`t[[-1, 0]]`、利用者の `Index` のキーには出ない。`xs[-1]` は今と同じくトラップし、IR は変わらない。
- [ ] Phase 2: docs/stability.md に、5 つ目の差（角括弧の中の範囲の構文の族の全体。NC18 の形を含む）、明確にした凍結の規則、2027 の経緯（0.1.5 → 0.1.6）、小節「リリースの数え方」がある。2028 のすべての拒否と意味の差が、Phase 2 の 1 つの PR に入っている。
- [ ] Phase 2: `fix --edition 2028 --from 2027` が `own` を足さず、`--from 2026` が `for` の素の `..` を書き換えず、どちらも IR を変えない。
- [ ] Phase 2: `tsuzuri guide --edition 2028` が 2027 の引数規約のブロックと 2028 の範囲のブロックを出し、D-56 の上限内。
- [ ] Phase 3: 移行の前後で fixture・examples・ベンチマークの IR が 1 byte も同じで、`LATEST`・`NEW_PROJECT` が 2028。マニフェストのないソースに注意が出て、`--edition`・マニフェスト・明示の演算子で消える。docs/stability.md に、注意を取り除く条件（仕様 10 の数え方で 2 リリース以上、かつ 90 日以上）がある。
- [ ] 各 Phase で T0・T1・T2 が通り、生成 IR が決定的で、`node scripts/check-docs.mjs <変えたページ>`・`node scripts/gen-docs.mjs --check`・`node scripts/check-cli-docs.mjs` が通る。

## 落とし穴

- **`W1014` の help の `{k}`**: `negative_literal_index` の help の `{k}` と `{k-1}` は、リテラルの大きさから求める。`src/parser.rs` の `integer` と同じく、接尾辞を除き、`0x`・`0b` の基数で符号なしの `u128` として読む（`-0x10` なら 16。`_` は字句が除いている）。符号付きの値の `abs()` や、添字の型の値から求めない（`xs[-9223372036854775808]` で溢れる）。除くのは綴りの `-0` ではなく、値が 0 のリテラルのすべてである（`-0`・`-00`・`-0x0`・`-0b0`・`-0_0`。HEAD はどれも添字 0 として受理する）。そのため `k ≥ 1` で、`k - 1` は溢れない。
- **`borrows_parameters` の `matches!`**: 2028 を足すだけだと、2028 のパッケージが A18 の引数規約・`mut T`・版別 API を失う（`src/syntax.rs:44`）。Phase 2 の手順 1 で `>=` にし、`parameter_conventions` のテストを 2028 でも回す。
- **`preserve_parameters` が無条件**: 2027 → 2028 の移行で `own` を足してしまう（`src/edition_migration.rs:246`）。`fix_2027_to_2028_adds_no_own` で検出する。
- **ガイドの完全一致**: `filter_edition` が年の完全一致なので、2028 では 2027 のブロックが消える。`tests/guide.rs` が `SUPPORTED` を回すので落ちる。
- **`range_op` の `_ => Bare`**: 新しいトークンを `range_operator_ahead` に足して `range_op` を直し忘れると、`ref xs[1<..3]` が黙って半開になり、`for i in 0 <.. 3` が素の `..` になる。網羅的な `match` にして、型検査は `ends` だけを読む。
- **edition を型検査や生成で見ない**: 素の `..` の意味を型検査で根のパッケージや `LATEST` の edition から決めると、2027 の依存パッケージの総称関数を 2028 から実体化したときに意味が変わる。構文解析で `RangeEnds` に決める。
- **B3 を深さで直さない**: 「入れ子の深さが違えば範囲を読む」とすると、`ref xs[if c then 1 else 2..3]` のように区切りのない枝の `..` が `for` の範囲になり、今のプログラムの意味が変わる。区切り（括弧・ブロック・レコード・補間・`try`）で旗を戻す。
- **`^` を一般の前置演算子にしない**: 字句の位置だけで許すと、配列リテラル `[^1]`、NC04 の `t[[^1, 0]]`、括弧の中の引数に漏れる（B3 と同じ形の不具合）。`index_or_slice` の成分の先頭と範囲の演算子の直後でだけ読む。
- **`FromEnd` の評価順**: 長さは土台を評価した後、`k` の前か後のどちらで読んでもよい（純粋）が、`k` は 1 回だけ評価する。文字列の添字は今、添字を評価してから長さを読むので、`FromEnd` のときだけ長さを先に読み、`FromEnd` でない添字の IR は変えない。
- **`i64u` の飽和の範囲**: `index_operand` 全体に入れると、すべての `xs[i]`（`i64u`）に `icmp`・`select` が増え、「`xs[i]` と `xs[i as i64]` は同じコード」が崩れる。派生する境界だけ。
- **`W1013` を `push_noncanonical` の拒否に混ぜない**: 拒否は kind ごとの `rejected_in` で決める。`W1013` の kind はどの edition でも拒否しない（2027 は凍結した edition）。
- **`W1013` で既存のテストが落ちる**: 警告なしを表明するテストは、期待値を変えずにプログラムを `..<` にする。2026 のマニフェストを持つ golden の fixture（`kg_basic` など）は書き換えない。
- **文書の例は `LATEST` で検査される**: `check-docs.mjs` の例はマニフェストを持てず（属性は `project`・`file`・`run` だけ、`file` は `.tz`・`.tt`・`.tc` だけ）、`LATEST` で読まれる。Phase 2 で属性 `edition=YYYY` を足すまで、2028 の例は書けない。属性は一時プロジェクトに `Tsuzuri.toml`（`scripts/check-guide.mjs` の `project` と同じ形）を書くだけにし、例の本文は変えない。
- **凍結の規則**: Phase 2 は 1 つの PR なので、その途中の状態をリリースしない。2028 の拒否や意味の差を Phase 2 の PR の外に残すと、リリースの後に足せなくなる（S5）。リリースは仕様 10 で数え、`git tag --list 'vsc-v*'` と `git diff --name-only` で確かめる。
- **`negative_literal_index` の判定の場所**: `index_operand` の中に置くと、`new [T](n, f)` の長さに出て、スライスの境界に `negative_literal_range_bound` と二重に出る。組み込みの添字の枝でだけ判定する。`Index` のキーに広げない（`Map<i64, _>` の `m[-1]` は正しい読み出し）。
- **拡張だけの版を数えない**: 0.1.8 のようにコンパイラの材料が変わらない版は、注意の期間にも、何かを初めて含むリリースの判定にも数えない（仕様 10）。
- **一括書き換えの誤認**: A22 の移行では、JavaScript の文字列の一括置換が型の文字列やシェルの記号を誤認した。構文解析の記録の編集だけを当て、文書の `..` は `tsuzuri` のブロックの中だけを対象にする。

## 対象外

- 多次元の角括弧 `t[i, a..b, ..]`、単独の `..` の成分、`Slice.Sel`、テンソル・行列の刻み付きの窓、`Index` の型の `^k`（[NC18](NC18-multi-axis-bracket-indexing.md)）。
- 文字列の範囲のスライス（借用の文字列の窓の型がないので `E1005` のまま）。
- 範囲外を丸める「寛容なスライス」（F# の既定）。Tsuzuri はトラップする。丸めたい場合は関数版を使う。
- `end`・`begin` の語（予約語が要る）、`...` のトークン、コロンのスライス（D-65、NC04 D3）。
- 中置の `^`（XOR の別名）を非正準にすること。
- `Index` のキーの負の整数リテラル（`map[-1]`、`t[[-1, 0]]`、NC18 の要素のモードの `t[-1, 0]`）への警告。キーの意味はインスタンスが決める（決定事項 D8）。
- 負の整数リテラルの添字を誤りにすること（受理していたプログラムを拒否するので edition が要る。D8）。
- LSP がマニフェストのないファイルに出す注意（仕様 9）。
- 演算子の拡張（NC19）、複合代入（NC20）。

## 決定事項

- **D1（承認済み。D-66b）**: `^k` は位置 `n − 1 − k`。`^0` が最後、`^1` が最後から 2 番目（F#・RFC FS-1076）。`..^1` は最後を除く。
- **D2（承認済み。D-66c）**: 新しい edition は 1 つだけで、名前は docs/stability.md 93 行の規則どおり **2028**（作成年 2026 以上で、最新の 2027 より大きい最小の年）。中間の edition は作らない。docs/stability.md の「edition の差として許すのは、次の 4 つだけ」に 5 つ目として次を足す: 「角括弧の中の範囲の構文の族（D-66c、2026-11-25 に承認された 2028 の例外）。2028 では、スライスの素の `..`（終端のある `a..b`・`..b`）が終端を含み、`for` の素の `..` を正準形として再び受理する。`<..`・`<..<`、末尾からの位置 `^k`、角括弧の中の刻み、多成分のキーの範囲・`^k`・単独の `..` の成分、組み込みでない型への 1 成分の範囲も、2028 から書ける。2026・2027 はこれらを拒否するか、今の意味のままにする（素の `..` のスライスは半開で、`W1013` が `..<` への書き換えを案内する）」。宣言の範囲は D13 で決めた（D-66 の委任で決定、2026-11-25）。「両方の edition が受理するプログラムの意味と生成 IR は同じ」の例外にも足す。A22 D3 の「`..` は半開という 1 つの意味」は、2028 で「終端を含むという 1 つの意味」に置き換わる。
- **D3（承認済み。D-66a）**: 利用者の 6 形に、`a<..<b`・`a<..` と（NC18 の成分として）単独の `..` を足す。`<..`・`<..<` は `for` でも使える。`..=` は恒久的な別名で、どの edition でも警告しない。
- **D4（既定案）**: 素の `..` の意味は構文解析がソースの edition で決め、AST の `ExprKind::Range` は `op: RangeOp` の代わりに `ends: RangeEnds` を、`ExprKind::Slice` は `RangeKey`（`ends` を含む）を持つ。綴りは記録（`Noncanonical`）とトークンにだけ残る。
- **D5（既定案）**: 境界の規則は境界のモデルだけ（仕様 2）。生成は巻き戻る i64 の演算と既存の符号なしの検査で、派生する境界の `i64u` だけを `SaturatingIndex` にする（設計の「正しさの根拠」）。合成した演算は `@checked` の対象にしない。
- **D6（既定案）**: `^` は角括弧の成分の先頭と範囲の演算子の直後だけで、1 つの項を取る。ほかの位置は `E0002`。2026・2027 は `E0002`。
- **D7（既定案）**: `W1013`（`legacy_bare_slice_range`）は既定で有効、根のパッケージの User のソースだけ、修正案は `certain` の `..<`。D-51 の重複除去と `(source, start)` 順に従う。
- **D8（決定（D-66 の委任で決定、2026-11-25）。未決だった質問 1）**: `W1014` は全 edition で既定で有効。対象は 2 つ。(1) スライスの境界と `^` の被演算子の負の整数リテラル（`negative_literal_range_bound`）。(2) 言語が「先頭から数える位置」と決める添字の負の整数リテラル（`negative_literal_index`）。組み込みの 6 型の添字（このチケット）と、NC18 の窓のモードの整数の成分（NC18 が広げる）に出す。刻み、`for` の範囲、`Index` のキー、リテラルでない式は対象外。修正案は付けず、誤りにもしない。新しい診断コードは要らない（D-55 の予約の行は変えない）。
  - 理由: 組み込みの 6 型では、負の位置は言語の定義で必ずトラップする（docs/language.md 3070 行）ので、誤検出がない。実行して初めて分かる誤りを検査の時点で示せる。`xs[-1]` は NumPy から来た人と AI が最も多く書く癖で、2028 には正しい書き方 `xs[^0]` があり、help で案内できる。範囲の境界と同じ `W1014` にすれば、「負の数は末尾から数えない」を 1 つのコードで調べられる。NC18 の窓では、キー `[Slice.Sel]` を受けるのが利用者のインスタンスでもよいが、負の `Front` を作るのは構文で、`Slice.Pos.Front` の文書の意味に負の数はない。`Index` のキーに出さないのは、キーの意味をインスタンスが決めるから（`Map<i64, _>` の `m[-1]` は正しい読み出し）。誤りにしないのは、到達しないコードも含めて受理していたプログラムを拒否することになり、edition が要るから。警告が増えるのは互換の保証の範囲（docs/stability.md 46 行）。
  - 採らなかった案: 警告しない（元の既定案）、std の `Tensor`・`Matrix` の要素のキーにも出す（std の型の名前を埋め込むか新しい印が要る）、角括弧の中のすべての負のリテラルに出す（`Map<i64, _>` で誤検出）、誤りにする（edition が要る）。
- **D9（決定（D-66 の委任で決定、2026-11-25）。未決だった質問 3）**: マニフェストのないソースへの注意（`bare_slice_range_meaning_changed`）は、edition が `LATEST` から来たときだけ CLI と `repl` が出す。`LATEST` を 2028 にした変更を含む最初のリリースを 1 回目と数える。取り除くのは、注意を含むリリースが仕様 10 の数え方で 2 回以上出て、かつその最初のリリースの日から 90 日以上たった後の別の変更。その PR に、数えたタグ・日付・`git diff --name-only <前のタグ> <今のタグ> -- src std guide Cargo.toml Cargo.lock` の結果を書く。docs/stability.md には「リリース」の利用者向けの定義と、取り除く予定を書く。
  - 理由: 拡張はコンパイラを同梱し（`vsc/README.md` 10 行）、利用者がコンパイラを受け取る経路は今これだけ（タグは `vsc-v*` だけ、`tsuzuri --version` は `0.1.0`）。コンパイラの変わらない版を数えると、振る舞いが同じまま期間が進む（0.1.8 は材料の差が 0 ファイル）。2 リリースだけでは短すぎることがある（0.1.7〜0.1.9 は 2026-11-19〜21 の 3 日に出た）。`ref xs[i..j]` はトラップせずに黙って 1 要素広がることがあるので、90 日の下限を置く。90 日に 1 回以上更新する利用者は、注意を含む版を必ず 1 回は受け取る。時間の下限だけにしないのは、その間にリリースがなければ誰も注意を受け取らないから。
  - 採らなかった案: すべての `vsc-v*` の版を数える、コンパイラの変わった版だけを数えて日数の下限を置かない、コンパイラのタグができるまで待つ、日数だけで決める。
- **D10（既定案）**: `fix --edition 2028` の canonical round は移行先が拒否する kind だけを当て、A18 の移行は `borrows_parameters` をまたぐときだけ。`///` の中の例は書き換えない。
- **D11（既定案）**: Phase 3 で `NEW_PROJECT` も `LATEST` と同時に 2028 にする（`tsuzuri new` は最新を書く。EDN-005 を新しい規則に置き換える）。
- **D12（既定案）**: `const`・`const def` の中の `^k` は `E1026`（`ConstOperationNotAllowed`）。定数の配列の末尾は `xs[xs.length - 1]` で書ける。
- **D13（決定（D-66 の委任で決定、2026-11-25）。未決だった質問 2）**: NC18 の 2028 だけの受理を待ってリリースを止めることはしない。2028 の差の全体は、edition を作る 1 つの変更（Phase 2 の PR）で宣言する（仕様 5、D2 の文）。2028 のすべての拒否と意味の差も、この PR に入れる。NC18 はその一部を後から実装する。受理を増やすだけなので、凍結の規則に反しない。
  - docs/stability.md の凍結の文（100 行）は次の意味に書き直す: 「リリースに含めた edition には、受理していたプログラムの拒否も、意味の変更も足さない（バグの修正は除く）。宣言していない新しい差も足さない。受理を増やす拡張は、全 edition に入れても、edition を作ったときに宣言した差の範囲で新しい edition に限って入れてもよい（宣言した差が受理する場所を、同じ edition 以降で借用の形でも受理する拡張を含む）」。
  - NC18 の範囲・`^k`・単独の `..` を含むキーと、組み込みでない型への 1 成分の範囲は、宣言した差の実装なので 2028 から受理する。整数の位置と式だけのキーは全 edition で受理する（NC18 の D1）。A40・NC19・NC20 のような全 edition の拡張は、この規則と関係なく入れてよい。A39 は 2027 以降に限るが、宣言済みの 2027 の差（A18 の射影への代入）の範囲で受理を増やすので、同じく入れてよい。
  - 2028 を含むリリースの後で、2028 の規則（拒否か意味）を変える必要や、宣言にない差が要ると分かったら、停止条件 S5 にする。リリースの前なら、NC17 の規則を直してよい。「リリース」は仕様 10 で数える。
  - edition 2027 の経緯を docs/stability.md に記録する: 「0.1.5（2026-10-25）は 2027 を `LATEST` にして配布した。その後、0.1.6（2026-11-01）が A22 の拒否（`E0004`）を足した。これは人間の包括許可（D-50）のもとで承認を得た一度きりの例外で、vsc/CHANGELOG.md の 0.1.6 で知らせた。今後、リリースした edition に拒否を足すことはしない」（事実は「現状」の edition の項で確認した）。
  - 理由: 凍結の目的は、同じ edition のソースがコンパイラを更新しても同じ意味で受理されること（docs/stability.md 7 行）で、受理だけを増やす拡張はこれを壊さない。edition の差は 72 行で閉じた一覧なので、後から edition に限った受理を足すと一覧の外の差になるおそれがある。作るときに差の全体を宣言しておけば、NC18 は宣言済みの差を実装するだけになる。「承認済みの差に依存すれば限ってよい」という一般の規則より狭く、新しい機能をリリース済みの edition に少しずつ足す道を開かない（A39 は、宣言済みの 2027 の差が代入を許す場所を、同じ edition で借用の形でも受理するだけで、新しい場所を受理しない）。前例として、Rust 1.88 は edition 2024 のリリースの後に、let chains を 2024 だけで安定化した。リリースを止めると、NC18（A37・A38・NC16・NC17 の Phase 2 に依存する大きなチケット）が入るまで不具合の修正を含むすべてのリリースが止まる。
  - 採らなかった案: NC18 が入るまでリリースを止める、2028 を不安定版の旗の後ろに置く（その仕組みがない）、2028 をプレビューの edition にする（保証が弱くなる）、「承認済みの差に依存すれば新しい edition に限ってよい」という一般の規則にする。
- **D14（既定案。NC18 が実装）**: 刻み付きの範囲で終端が `..<` のとき、実行時の刻みが負ならトラップする（テンソル・行列の窓では NC18 の std の `assert` による `assertion failed`。NC18 の仕様 3.4）。負の整数リテラルの刻みは構文解析の `E0002`（`slice_step_down_excludes_end`）。
- **D15（既定案）**: `TypedExprKind::FromEnd` は親（組み込みの添字と `array_slice`）が長さを持って下げる。文脈のスタックは使わず、`FromEnd` は組み込みの `Index` の添字とスライスの境界の直下にだけ置く（型検査が保証する）。
- **D16（既定案）**: ガイドの edition のブロックに `<!--if edition>=YYYY-->` を足し、2027 の引数規約のブロックを `>=2027` にする。

### 未決だった質問への決定（D-66 の委任で決定、2026-11-25）

利用者は 2026-11-25 に「「承認が必要なこと」と「決めていただきたいこと」については、あなたが考えられる最高の選択肢、手法、実装に基づいて決定することを承認します。」と述べた（GUIDE D-66）。起票時に人間へ尋ねた 3 つの質問は、これに基づいて次のとおり決めた。

1. 添字 `xs[-1]` にも `W1014` を出すか → 決定（D-66 の委任で決定、2026-11-25）: 出す。kind は `negative_literal_index`。言語が先頭からの位置と決める場所（組み込みの 6 型の添字と、NC18 の窓のモードの整数の成分）だけに出し、`Index` のキーには出さない（D8、仕様 7）。
2. リリースに含めた 2028 に NC18 が後から 2028 限定の受理を足すことは「差を足す」に当たるか → 決定（D-66 の委任で決定、2026-11-25）: 当たらない。2028 の差の全体を Phase 2 の PR で宣言し、NC18 はその実装として後から入る。リリースは止めない。凍結の規則を docs/stability.md で明確にし、2027 の経緯を記録する（D13、仕様 5）。
3. 注意の期間の「リリース」を VS Code 拡張の版で数えてよいか → 決定（D-66 の委任で決定、2026-11-25）: コンパイラを利用者に配布する版で数える。今は `vsc-v*` の版で、コンパイラの材料が変わらない版は数えない。期間は 2 リリース以上かつ 90 日以上（D9、仕様 10）。
