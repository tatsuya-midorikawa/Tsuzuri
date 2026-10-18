# G26: Facts モデル（コンパイラの事実の機械可読な定義と exporter）

| 項目 | 内容 |
| --- | --- |
| ID | G26 |
| 優先度 | P1 |
| 規模 | XL（Phase 1 M・Phase 2 M・Phase 3 M・Phase 4 L・Phase 5 L・Phase 6 M） |
| 依存 | [G22](G22-diagnostic-v2.md)（Phase 5 だけ。所有権の起点の span の記録と挿入点を共有する。Phase 1〜4・6 は診断 v2 の型を使わない）。[A20](A20-effect-summary-and-checked-attributes.md)（Phase 3 の `effects` を完全にする。未完でも縮退版で出荷できる）。[A15](_completed/A15-copy-cost-visibility.md)（done。`copies::sites`）、[PM01](../_perfs/_completed/PM01-value-layout.md)（done。`record_slots`・`UnionLayout`）、[F12](_completed/F12-bounds-check-elimination.md)（done。`ranges.rs`）、[G20](_completed/G20-error-recovery.md)（done） |
| 後続 | [G27](G27-build-time-kb.md)（Phase 1 から。書き込みと `check`・`build` への組み込み）、[G28](G28-kb-query-and-impact.md)、[G29](G29-mcp-server.md)、[G30](G30-diagrams.md)（Phase 1・2・5・6）、[G35](G35-generated-docs-single-source.md)、[E15](E15-trust-boundary-and-capabilities.md)（Phase 3）、A20（facts への hook） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーとの差は「現状」） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | コンパイラは移動・借用・複製・確保・トラップ・型のレイアウトの事実を内部に持つが、機械が読める形で出さない。AI は、関数の署名・呼び出し元・効果・トラップ箇所を知るために、ソースを探索して読むしかない（D-50 の結論 (1)）。D-52 の KB の中身を定義し、`check` の解析だけから決定的に作る |
| 手本にする既存実装 | 副産物の取り出し: `src/check.rs` の `check_modules_indexed_all`・`check_modules_collect` の `semantic: Option<&mut SemanticIndex>` と `src/semantic.rs` の `collect`。sink: `src/ownership.rs` の `check_body` の `copies: Option<&mut Vec<CopyRead>>` と `copy_reads`。箇所: `src/copies.rs` の `sites`。呼び出しグラフ: `src/recursion.rs` の `check`・`check_graph`。署名: `src/docgen.rs` の `signature`・`type_text`。JSON Lines: `src/diagnostic.rs` の `json_string`・`location`、`src/api_diff.rs`。レイアウト: `src/llvm_layout.rs` の `layout_bound`・`record_slots`・`union_layout`。範囲: `src/ranges.rs` の `analyze`・`index_in_bounds`・`enter_condition`。測定のテスト: `tests/copy_inventory.rs` |
| 主な影響ファイル | 新規: `src/kb/mod.rs`・`src/kb/model.rs`・`src/kb/json.rs`・`src/kb/ids.rs`・`src/kb/symbols.rs`（Phase 1）、`src/kb/graph.rs`（Phase 2）、`src/kb/facts.rs`・`src/kb/effects.rs`（Phase 3）、`src/kb/sites.rs`（Phase 4）、`src/kb/events.rs`（Phase 5）、`src/kb/layout.rs`（Phase 6）、`src/kb_capture.rs`（新規。`check` の子モジュール）、`tests/kb.rs`（新規）、`tests/fixtures/kb_basic/`（新規。付録 A）、`tests/golden/kb/point/`・`tests/golden/kb/polymorphism/`（新規。付録 H）、`_tsuzuri/language-reference/compiler/kb-format.md`（新規）。変更: `src/lib.rs`（`pub mod kb;`・`analyze_inputs_core`・`analyze_inputs_captured`）、`src/check.rs`（`pub(crate) mod kb_capture;`・`check_modules_collect` の引数・`check_modules_captured`・`indexing`・フック 2 か所）、`src/polymorph.rs`（`Classes` の読み取り口。Phase 1）、`src/warnings.rs`（テストの呼び出し 1 か所）、`src/ownership.rs`（Phase 5）、`src/llvm.rs`（Phase 6。可視性だけ）、`docs/architecture.md`、`docs/stability.md`、`_tsuzuri/language-reference/index.md` |

詳細な表・実例・想定出力は [付録](G26-appendix.md) にある（以下「付録 A〜J」）。

## 実装者向け早見

1. 最初に読む: 本チケットの「仕様」→「設計」→ 実装する Phase の手順。コードは `src/check.rs` の `check_modules_collect`（`polymorph::specialize` を呼ぶ前後）、`src/semantic.rs`、`src/copies.rs`、`src/ownership.rs` の `copy_reads`・`check_body`、`src/docgen.rs`。
2. 最初に書くテスト: `tests/kb.rs::symbols_of_examples_point_match_the_golden_file`（手順 2。`#[ignore]`）と、`src/kb/json.rs` の `escapes_like_diagnostic_json_string`（手順 3）。次に `src/kb/mod.rs` の単体テスト `analysis_with_capture_keeps_the_ir_identical`（手順 4）。
3. 触ってはいけない: 型検査・所有権検査・特殊化・`closures::lower` の意味と結果、生成 IR（1 byte も変えない）、診断のコードと位置、`Diagnostic` の型、`--json` のキー。capture は**読むだけ**で、checker の状態を書き換えない。
4. 新しい crate・`unsafe`・予約語・CLI のフラグ・環境変数・診断コードは足さない。ファイルへの書き込みと CLI は G27。
5. 出力は決定的: `BTreeMap`・`BTreeSet`・整列した `Vec` だけ。`HashMap` の反復順で行を決めない。日時・絶対パス・スレッド数を入れない。
6. Phase 1 の完了の見分け方: `tests/kb.rs` の `symbols_of_examples_point_match_the_golden_file` と `symbols_of_examples_polymorphism_match_the_golden_file` が緑で、`kb_basic` の kind 別の行数が付録 H の表と一致し、`cargo test --locked` の件数が手順 1 の件数＋新規テストだけ増える。
7. Phase 2〜6 の完了の見分け方: 2＝6 種類の `rel` が `kb_basic` に出て `graph`・`tests` が付録 H の表と一致。3＝`Main.traced` の `debug` と `effects_complete`（A20 の有無）。4＝`copy_sites`・`trap_sites`・`recursion`・`loops` が表と一致。5＝`Main.ownership` の事象が表と一致し、既存の所有権のテストと診断が不変。6＝`Packed` が `reordered: true` で `specializations` が表と一致。詳細は「受け入れ条件」。
8. 想定出力（付録 H）は**手計算**。実装の出力と違うときは、規則と照らして決め、報告する（停止条件 6）。コンパイラの出力を黙って golden に写さない。
9. 各手順の最後で `cargo build --locked`・`cargo test --locked` が緑であること。`cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings` も通す。

## 目的

**問題。** AI がプロジェクトを理解するとき、「この関数は何を受け取り、誰に呼ばれ、何をするか」を知るために、複数のファイルを探索して読む。コンパイラは、型付き IR と所有権解析の過程で、その答えの大半（署名・呼び出し・効果・トラップ箇所・暗黙の複製・移動と借用・型のレイアウト）をすでに持っているが、捨てている。HEAD には、これらを機械可読に出す手段がない（診断は主スパン 1 個、`tsuzuri doc` は公開宣言の署名だけ）。

**価値。** D-52 の `symbols.jsonl`・`graph.jsonl`・`facts/<Module>.jsonl`・`layout/<Module>.jsonl`・`tests.jsonl` の**定義と exporter**（ライブラリ関数）を作る。G27 がファイルに書き、G28 が照会し、G29 が MCP で返し、G30 が図にし、E15 が権限の報告に使う。したがって本チケットは G27〜G30・E15 の土台で、**キー・型・整列・省略規則・各事実の抽出元**を 1 か所に固定する。

**HEAD で測った数字**（付録 J）: `examples/functional`（7 関数＋std）の `check` は 55.0 ms（`TSUZURI_TIME_PASSES=1`。`check` 17.9 ms・`specialize` 3.7 ms）。`TypedExprKind` は 64 variant、`Builtin` は 246 項目（付録 E で全件を分類）。

**非目標。**
- ファイルの書き込み・manifest・鮮度・brief・増分、`--kb`・`--no-kb`・`--kb-dir`・`TSUZURI_KB`・`TSUZURI_KB_DIR`（G27）。照会・`impact`（G28）。MCP（G29）。図（G30）。std の KB（G27 Phase 4）。
- 効果の**推論と検査**（A20）。本チケットは A20 の結果を facts に出すか、A20 が無いときの縮退版（10 個の効果だけ）を出す。
- `diag.jsonl`（G27）。LLM が書く文章。LLVM の IR・clang を使う解析（D1）。
- 型検査・所有権検査・最適化の変更。速度の合否の閾値（測って記録するだけ）。

## 着手条件と停止条件

### 着手条件

- 依存: Phase 1〜4・6 は依存チケットを待たない（A15・PM01・F12・G20 は done）。Phase 3 の完全版（Level A）は A20 Phase 1 が done のとき。Phase 5 は G22 Phase 2 が done なら、挿入点を共有する（「前提とする他チケットのインターフェース」）。未完でも Phase 5 は単独で出荷できる。
- 基準（GUIDE §2.3）: `cargo build --release --locked && cargo test --locked` を実行し、`running N tests` の合計 N を控える。失敗があれば自分の変更と無関係であることを記録する（手順 1）。
- 読むもの: [GUIDE](GUIDE.md) §6.9・§7・§8、D-51・D-52・D-53・D-55、`docs/stability.md`、本チケットの付録 B〜E、`src/check.rs` の `check_modules_collect`（フックの周辺）。

### 停止条件

次のとき、即興で回避せず作業を止めて報告する（GUIDE §13）。選択肢を併記する。

1. capture の導入（`indexing` の変更・フック）で、`analysis_with_capture_keeps_the_ir_identical` が失敗する、または既存の診断・警告・IR が 1 つでも変わる。選択肢: A（`indexing` を変えず、semantic の索引を別に作る）／B（フックを `specialize` の後ろへ移す）。
2. `Names`・`Classes` から必要な情報を取るために、フィールドの可視性を広げる必要が出た（`pub(super)` の読み取り口より多く）。選択肢: A（読み取り口を足す）／B（`kb_capture.rs` を `polymorph` の子にする）。
3. `ownership.rs` の sink の追加で、診断の内容・順序・件数が変わる、または `check_body` の結果が変わる。
4. `SemanticIndex.references` に、付録 D の表 D4 が要る使用が載っていない（`kb_basic` の `Point` の使用が、`examples/point` の LSP の結果より少ない）。選択肢: A（索引の収集を足す。`src/semantic.rs` の変更を最小に）／B（その使用を `uses_type` から外し、決定事項 D6 に限界を書く）。
5. 最終モジュールの関数を宣言に結びつけられない（付録 C の規則に当てはまらない user の関数が 1 つでもある）。
6. 付録 H の想定出力と実装の出力が食い違い、規則（仕様）からはどちらが正しいか決まらない。
7. A20 が未完で、`src/effects.rs`（A20 が新規に足す）が無いのに、Level A を実装しようとした。または、A20 が完了済みで `FunctionEffects` の名前・意味が「前提とする他チケットのインターフェース」と違う。
8. KB の費用が `check` の 100% を超える（`check` が 100 ms 以上の入力で。付録 J の手順）。30% は目標で、超えても止めない。記録する。
9. 新しい crate・`unsafe`・既定の WASM の import・予約語が必要になった。IR の変更が必要になった。
10. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

作業ツリーの HEAD は `9dd0fbbc`（`git --no-pager diff --stat a2313d19 HEAD` は `docs/benchmarks.md` の 41 行だけ）。確認と実行はすべて `9dd0fbbc` の作業ツリーで、`target/release/tsuzuri` を、`target/tz-G26/`（`.gitignore` 済み。確認の後に消した）に作った入力へ使った。再現の作業ディレクトリも `target/tz-G26/`（`/tmp` は使わない）。

### `check` の段階と取れるもの

`src/check.rs` の `check_modules_collect` は、型検査のあと次の順で進む（関数名で確認）。

| 段 | 呼び出し | 取れるもの |
| --- | --- | --- |
| 1 | `semantic::collect`（`indexing` のとき） | 宣言と名前の使用位置（`SemanticIndex`）。`indexing` は `semantic.is_some()` か `@deprecated` の宣言があるときだけ真 |
| 2 | `recursion::check` | `references: Vec<Vec<usize>>`。関数 id ごとの参照先（`classes.recursion_targets` を含む）。単相化**前** |
| 3 | `CheckedModule { .. }` の構築 | 単相化前の `records`・`unions`・`functions`（抽象本体）・`tests`・`benches` |
| 4 | `ownership::infer_copy_all` | 抽象本体の所有権解析（`infer = true`）。**ここがフックの位置** |
| 5 | `polymorph::specialize`（`module` を消費） | 単相化後の `functions`。非ジェネリックな user の関数は全部、ほかは到達したものだけ。インスタンスの名前は `{name}.$mono.{N}` |
| 6 | `closures::lower` | ラムダを関数に持ち上げる。名前は `module = "$lambda"`（`$task`）、`origin.parent = Some(親の関数 id)` |
| 7 | `ownership::check_all_with` | 診断 |

最終の `CheckedModule`（`Project::analyze` の結果）は段 6 のあとで、**抽象のジェネリック関数を持たない**。facts には、単相化前の視点（宣言・呼び出し・所有権）と、単相化後の視点（効果・箇所・特殊化・レイアウト）の両方が要る（D2）。

### 入口の確認（呼べるか）

| 入口 | `check` の段階で呼べるか | 根拠 |
| --- | --- | --- |
| `copies::sites(&CheckedModule)` | 呼べる（最終モジュールに対して） | `pub fn`。`crate::ownership::copy_reads`・`call_specialization::single_use_locals`・`last_use::LastUses` を使う |
| `ranges::analyze`・`RangeFacts::index_in_bounds`・`enter_condition`・`leave_condition` | 呼べる（型付き IR の走査を自分で書く） | `lib.rs` の `mod ranges;`（crate 内）。`llvm.rs` が `Block` の束縛で `bind`、`If` の then の前後で `enter_condition`・`leave_condition` を呼ぶ |
| `llvm_layout.rs` の `layout_bound`・`record_slots`・`union_layout`・`tag_width` | **呼べない**（`llvm.rs` の `mod layout;` が非公開） | Phase 6 の手順 18 で `pub(crate) mod layout;` にする（中の関数はすでに `pub(crate)`。変更はこの 1 行だけ） |
| `llvm_traps.rs` | **使えない** | 生成した LLVM IR の行から trap の箇所を逆引きする後処理（`Marks::source`）。`check` の段階には IR がない（D1） |
| `ownership::check_body` | 呼べる（`ownership.rs` の中で） | `copies: Option<&mut Vec<CopyRead>>` の sink がある。Phase 5 は同じ形の sink を足す |
| `Classes`（`polymorph.rs`） | 読み取り口が無い | フィールドが非公開。Phase 1 で `pub(super)` の読み取り口を足す |

### 実測（再現できるコマンドと実出力）

- 完全名: `Tsuzuri.toml` に `namespace = "Acme::Shapes"`、`Geometry/Point.tz` に `export def norm`。LSP の hover は `def Acme::Shapes::Geometry::Point.norm :: Geometry::Point -> f64`。モジュールの完全名は `Names::module_display(key)`（`Acme::Shapes::Geometry::Point`）で、型の表示（`Type::display`）は名前空間を含まない（`Geometry::Point`）。`tsuzuri doc` のページ名はモジュールの key（`Geometry.Point.md`）。
- テスト名: `test "same"` を 2 つ書いても受理され、`tsuzuri test --list` は `0 Main.same` と `1 Main.same`。ID の衝突を扱う必要がある（付録 B の規則）。
- `$entry`: `examples/point/Main.tz` の先頭に `def main` を足すと `E2004` が `"start":36,"end":97`（足した 36 byte を引くと 0..61）。トップレベルのコードは `$entry` 関数で、スパンはその式の全体。
- IR（`kb_basic`〔付録 A〕を `tsuzuri build --emit llvm -O0`）: `Main.total`（`for index in 0 .. values.length - 1`）に境界検査がなく、`Main.at`（`values[index]`）に `icmp ult`＋`llvm.trap`、`Main.ratio`（`left / right`）にゼロと `MIN / -1` の検査。`Main.sum_to` に自己呼び出しがなく `br label %loop` が 2 つ、`Main.fib` に自己呼び出し 2 つ。`%tz.record.Shapes.Packed = type { i64, i8, i8 }`（宣言順 `a: i8, n: i64, b: i8` が並べ替わる）、`%"tz.union.Shapes.Shape" = type { i8, double }`。
- 暗黙の複製: `tsuzuri check target/tz-G26/kb_basic --warn implicit-copy` は `Main.tz:54:17: warning[W1006]`（`let moved = values`）を 1 件。
- 名前の使用: LSP の `textDocument/references` で、`examples/point` の `Point` は宣言のほかに `Main.tz` の `Point {`（8..13）、`Point.tz` の署名の型（3 行 17 列）と record の構築（7 行 15 列）が返る。シグネチャの型・instance の頭・構築式は索引に載る。型エイリアスの使用は載らない（`semantic.rs` の「aliases have no references yet」）。
- 時間: `TSUZURI_TIME_PASSES=1 tsuzuri check examples/functional` の合計は 55.0 ms（内訳は付録 J）。

## 変えてはいけない意味

- `check`・`build`・`run`・`test`・`doc`・`fmt`・`lsp`・`repl` の出力、診断（コード・位置・順序・件数）、生成 IR・object・`.wasm` は 1 byte も変わらない。capture を使わない呼び出し（既存の `analyze*`）は、capture の分の仕事を 1 つもしない。
- `indexing` を `capture.is_some()` でも真にするが、`@deprecated` の警告（`W1005`）は宣言があるときだけ出るので、警告の集合は変わらない。
- Phase ごとに「既定の出力は 1 byte も変わらない」: 本チケットはファイルを書かず、CLI を足さない。ライブラリの `kb` を呼ばない限り、何も起きない。
- Phase を足すとき、前の Phase の行のキー・値・並びを変えない（schema は `tz-facts/1` のまま。足すだけ）。
- ownership の sink は診断を変えない。sink を渡さない呼び出しは、sink の分の仕事をしない。

## Phase 分割

各 Phase は schema（`tz-facts/1`）にキーと行を足すだけで、前の Phase の出力を変えない。出荷した Phase の行だけを出し、未出荷のキーは出さない（空配列を出さない。出すと「無い」と誤読される）。

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 基盤（`src/kb/`（新規）・capture・JSON 書き手・ID）と `symbols.jsonl` | M | なし | ライブラリ関数だけで、既定の出力を変えない。`symbols` は G27 の brief の最小の材料 |
| 2 | `graph.jsonl`（`calls`・`uses_type`・`implements`・`imports`・`instance_of`・`tested_by`）と `tests.jsonl` | M | Phase 1 | グラフは `symbols` の ID だけを参照する |
| 3 | `facts/<Module>.jsonl` の `id`・`effects`・`effect_sites`・`effects_complete`・`truncated` と、単相化後の走査の基盤 | M | Phase 1。A20 は任意 | A20 が無ければ縮退版（10 個の効果）と `effects_complete: false` |
| 4 | `copy_sites`・`alloc_sites`・`trap_sites`・`recursion`・`loops` | L | Phase 3 | 各キーが独立。`alloc_sites` は A20 が無ければ明示的な `new` だけ（`alloc_complete: false`） |
| 5 | `ownership`（move・borrow・borrow_mut・reborrow・drop・copy） | L | Phase 3。G22 Phase 2 は任意 | `ownership.rs` に sink を足すだけで、診断は変わらない |
| 6 | `layout/<Module>.jsonl` と `specializations` | M | Phase 3 | `layout` モジュールの可視性を広げるだけで、IR は変わらない |

実装の順序は 1 → 2 → 3 → 4 → 5 → 6。Phase 5 と 6 は入れ替えてよい。着手する Phase は人間が指示する（指示がなければ Phase 1 だけ）。

## 仕様

### 共通の規則（JSON Lines）

1. UTF-8、改行は `\n`、1 行に 1 つの JSON オブジェクト。最後の行も `\n` で終わる。行がなければ 0 byte。`:` と `,` の後に空白を入れない。
2. 文字列は `diagnostic::json_string` と同じエスケープ（`"`・`\`・`\n`・`\r`・`\t`、U+0000〜U+001F は `\u00xx`、ほかは UTF-8 のまま）。数値は 0 以上の 10 進整数だけ（小数・指数・符号を出さない）。真偽は `true`/`false`。値のないものは `null`。
3. キーの順序は各表のとおりに固定する。省いてよいキーは、`graph` の `args`（`calls` 以外）、所有権の事象の `to`（呼び先がないとき）、`layout` の種類別のキー（record に `layout`・`tag_bytes`・`niche`・`cases` を出さず、union に `fields`・`reordered` を出さない）だけ。ほかのキーは常に出す（値がなければ `[]`・`null`・`false`・`{}`）。
4. `span` は `{"path","start","end","line","column"}`。`path` はプロジェクトの根からの相対で `/` 区切り（`SourceInput.path` の `\` を `/` にしたもの）。`start`・`end` は UTF-8 の byte offset（`end` は排他）、`line` は 1 始まり、`column` は `start` の 1 始まりの文字数（`diagnostic::location` と同じ）。`source` が `None` のスパン、または範囲外のスパンの行は出さない。
5. 整列: `symbols`・`tests`・`facts`・`layout` は `id`、`graph` は `rel`→`from`→`to` の byte 順（`str::cmp`）、同じなら `span.path`→`span.start`→`span.end`。まったく同じ行は 1 行にする。
6. 配列の上限: 1 行の 1 つのキーの配列は 100 件まで。整列してから切り、捨てた件数を `truncated`（キー名 → 件数のオブジェクト。常に出し、なければ `{}`）に足す。
7. 決定性: 同じソースと同じコンパイラから、byte 単位で同じ出力。入力の順序（ソースの列挙順・`ModuleInput` の順）に依らない。exporter は 1 スレッドで、スレッド数に依らない。日時・絶対パス・環境の値を入れない。`HashMap` の反復順で出力を決めない。
8. 資源上限: 1 関数で走査する式のノードは 1,048,576 まで、プロジェクト全体で 8,388,608 まで（宣言の視点の走査と実体の視点の走査のそれぞれ）。超えたら、その関数の走査を止めて行を出し、`truncated` に `{"nodes": 1}` を足す。エラーにしない（facts は補助で、解析ではない）。
9. 解析が失敗した入力（診断に error がある）では、`collect` は `Err(DiagnosticSet)` を返し、部分的な KB を作らない（D15）。

### ID（付録 B の表 B1）

| 対象 | ID | 例（`kb_basic`。名前空間 `Acme::Kb`） |
| --- | --- | --- |
| module | `Names::module_display(key)`（名前空間を含む完全名。`::` 区切り） | `Acme::Kb::Geometry::Metrics` |
| def・const・extern・record・union・alias・class・handle（`extern type`） | `<module ID>.<name>` | `Acme::Kb::Shapes.area` |
| field・case | `<record ID>.<field>`・`<union ID>.<case>` | `Acme::Kb::Shapes.Point.x`・`Acme::Kb::Shapes.Shape.Circle` |
| class のメソッド | `<class ID>.<method>` | `Acme::Kb::Traits.Sized.size` |
| instance | `instance:<class ID><<head>>`（head は `type_id_text`。付録 B の表 B2） | `instance:Acme::Kb::Traits.Sized<Acme::Kb::Shapes.Point>` |
| instance のメソッド | `<instance ID>.<method>` | `instance:Acme::Kb::Traits.Sized<Acme::Kb::Shapes.Point>.size` |
| test・bench | `test:<module ID>.<name>`・`bench:<module ID>.<name>` | `test:Acme::Kb::Main.total adds` |
| トップレベルのコード | `<module ID>.$entry` | `Acme::Kb::Main.$entry` |
| active pattern | `<module ID>.(\|A\|B\|)`（宣言の名前の描画は `docgen`） | `Main.(\|Even\|Odd\|)` |
| 組み込みクラス | 名前そのまま（モジュールがない） | `Add`・`Display` |
| 組み込み関数（`graph` の `to` だけ） | `Builtin::name()` | `sqrt`・`Math.sqrt` |
| std の項目（`graph` の `to` だけ） | `<std のモジュールの key>.<name>`（`std::` を付けない） | `Array.map` |

ID は位置に依存しない。**衝突**（同じ ID が 2 件以上。同名の `test`・`bench` が現実に起きる）: ソースの位置順（`source`・`start`）の 2 件目以降に `#2`・`#3`… を付ける（`test:Main.same`・`test:Main.same#2`）。位置が動くと `#n` は変わりうる（テストの名前を一意にすることを勧める）。

### `symbols.jsonl`（Phase 1）

キーと型（この順）:

| キー | 型 | 内容 |
| --- | --- | --- |
| `id` | string | 上の表 |
| `kind` | string | `module`・`def`・`const`・`record`・`union`・`case`・`field`・`class`・`method`・`instance`・`alias`・`test`・`bench`・`extern`・`handle` |
| `module` | string | 宣言のモジュールの ID（完全名）。module 行は自分 |
| `name` | string | 宣言の名前（test・bench は名前の文字列、instance は `Class<head>`、module は最後の `::` 以降） |
| `signature` | string | 付録 B の表 B3 の雛形（`docgen` の描画を再利用） |
| `params` | array | `[{"name","type","conv"}]`。`conv` は `own`・`ref`・`ref_mut`・`copy`。callable でないものは `[]` |
| `result` | string / null | 結果の型（field・case・const・alias は値の型。callable でも値でもないものは `null`） |
| `public` | bool | `Visibility::Public`（module・class・instance は `true`、test・bench・`$entry` は `false`。field・case は親） |
| `export` | bool | `exported`（`export def`） |
| `pure` | bool | A19 の `pure def`。A19 が未完の間は常に `false`（A19 完了後は `FunctionDecl.pure`） |
| `doc` | string / null | 宣言の `Documentation.text` をそのまま（G34 の決定と同じ。切り詰めない） |
| `deprecated` | string / null | `Documentation::deprecation()` の注記（注記が空なら `""`） |
| `origin` | string | `project`、または `dependency:<パッケージ名>`。std の行は出さない |
| `span` | object | 名前の識別子のスパン（module は先頭のファイルの `0..0`、`$entry` はトップレベルの式全体） |

`conv` の決定表（宣言の型を解決した `Type` で決める。ジェネリックな型変数は `own`。付録 B の表 B4）: `Type::Reference(_, false)` → `ref`／`Type::Reference(_, true)` → `ref_mut`／`ty.is_copy(types)` → `copy`／それ以外 → `own`。A18（承認待ち）が無印の引数を読み取りにしても、この表の入力は解決後の `Type` で、構文でないので、A18 の edition 2027 の規則は表の追加（拡張点）で足せる。

### `graph.jsonl`・`tests.jsonl`（Phase 2）

`graph.jsonl` のキー: `rel`・`from`・`to`・`args`（`calls` だけ）・`span`。

| `rel` | `from` | `to` | `span` | `args` |
| --- | --- | --- | --- | --- |
| `calls` | 呼び出しを含む関数・test・bench・`$entry`・instance のメソッドの ID | 呼び先の ID（付録 D の表 D1） | callee の式 | 実引数ごとの `conv`（付録 D の表 D2）。呼ばずに値として参照しただけなら `[]` |
| `uses_type` | その使用を囲む宣言（最も内側）の ID | 型の ID（record・union・class・`extern type`） | 型名の出現 | なし |
| `implements` | 型の ID（`type_id_text`。`deriving` の分も） | class の ID | instance の class 名／`deriving` の class 名 | なし |
| `imports` | モジュールの ID | `using` の名前空間（書かれたまま、`::` 区切り） | `using` の宣言 | なし |
| `instance_of` | instance の ID（`deriving` は除く） | class の ID | instance の class 名 | なし |
| `tested_by` | 関数の ID | test の ID | test の名前 | なし |

`tested_by`: 各 test の本体から、`recursion::check` の参照グラフ（`references`）を幅優先でたどって到達する user の関数（深さ 8 まで、1 つの test につき 200 件まで。付録 D の D3）。演算子のオーバーロードが呼ぶ instance のメソッドは `calls` に出さない（D7）。

`tests.jsonl` のキー: `id`・`kind`（`test`・`bench`）・`module`・`name`・`index`（`tsuzuri test --index`・`bench --index` の番号）・`span`（名前のスパン）。

### `facts/<Module>.jsonl`（Phase 3〜6）

ファイル名は**モジュールの key**（`Geometry.Metrics`。`docgen` のページ名と同じ。完全名は `::` を含み、Windows のファイル名に使えない）。行は関数的な宣言（`def`・`extern`・`test`・`bench`・instance の `method`・`$entry`）に 1 行。キーは次の順で、Phase が出荷したものだけを出す（`KbModel::fact_keys()`）:

| キー | Phase | 型と内容 |
| --- | --- | --- |
| `id` | 3 | symbol の ID |
| `effects` | 3 | D-53 の効果の名前の配列（辞書順）。推論した要約 |
| `effect_sites` | 3 | `[{"effect","span","via"}]`。効果ごとの最初の証拠 1 件（辞書順）。`span` はこの関数の中の位置（直接ならその式、呼び出し経由なら最初の呼び出し）、`via` は呼び出しの連鎖の関数 ID（最初の呼び先から証拠を含む関数まで。直接なら `[]`） |
| `effects_complete` | 3 | bool。Level A で `EffectRow.complete`（A20 が 14 個すべてを計算する Phase まで出荷済み）なら `true`、それ以外は `false` |
| `copy_sites` | 4 | `[{"span","type","kind","cost"}]`。`copies::sites` の結果（`kind` は `local`・`field`・`element`・`tail`・`payload`・`dereference`・`temporary`、`cost` は `length`・`environment`、`type` は具体型の表示） |
| `alloc_sites` | 4 | `[{"span","kind"}]`。`kind` は A20 の `Why.code` の `-` を `_` にしたもの（`new_collection`）。A20 が無ければ `new_array`・`new_list`・`new_literal` |
| `alloc_complete` | 4 | bool |
| `trap_sites` | 4 | `[{"span","kind","proven"}]`。`kind` は `assert`・`unreachable`・`integer_division_by_zero`・`integer_division_overflow`・`bounds_check`・`range_step_zero`・`overflow`。`proven` は範囲の証明か定数の除数・step で起きないと示せたもの（付録 E の表 E3） |
| `ownership` | 5 | `[{"var","event","to"?,"span"}]`。`event` は `move`・`borrow`・`borrow_mut`・`reborrow`・`drop`・`copy`。ソース順 |
| `recursion` | 4 | `none`・`self`・`self_tail`・`mutual` |
| `loops` | 4 | `[{"kind","span"}]`。`kind` は `while`・`for_range`・`for_each` |
| `specializations` | 6 | 整数。この関数のジェネリックな実体（`.$mono.N`）の数。非ジェネリックと、使われないジェネリックは 0 |
| `truncated` | 3 | オブジェクト |

並びは D-52 の順（`id`・`effects`・`effect_sites`・`effects_complete`・`copy_sites`・`alloc_sites`・`alloc_complete`・`trap_sites`・`ownership`・`recursion`・`loops`・`specializations`・`truncated`）。`effects_complete`・`alloc_complete` は D-52 の一覧への追加（足すだけ。D8）。F15 の Phase 4 は、`parallel`（常に出す。空は `[]`）を `specializations` の次・`truncated` の直前に足す（`KbModel::fact_keys()` に足し、golden を更新する）。単相化後の走査で得る配列（効果・複製・確保・トラップ）は、ジェネリックなインスタンスの結果を宣言ごとに集め、スパンで重複を除く。一度も実体にならないジェネリックな関数は、これらが空で `specializations` が 0。

### `layout/<Module>.jsonl`（Phase 6）

ファイル名は facts と同じ（モジュールの key）。行は、そのモジュールで宣言された user の record・union の**型**（型引数のないものは常に 1 行。ジェネリックなものは、単相化後の関数の署名・局所・式に現れた実体ごとに 1 行）。ネイティブ（64 bit）の値。サイズと整列は**測った値ではなく** `layout_bound` の下限と上限（`lower = true`・`false`）。

| キー | 型 | 内容 |
| --- | --- | --- |
| `id` | string | `type_id_text`（型引数があれば `<..>` 付き） |
| `kind` | string | `record`・`union` |
| `type` | string | `Type::display` |
| `size` | `[min,max]` | `layout_bound` の下限・上限のサイズ（byte） |
| `align` | `[min,max]` | 同じく整列 |
| `fields` | array | record だけ。`[{"name","type","slot"}]`（宣言順。`slot` は LLVM の型での位置。`record_slots` が `None` なら宣言の添字） |
| `reordered` | bool | record だけ。`record_slots` が `Some` |
| `layout` | string | union だけ。`enum`・`common`・`general`・`niche`・`node`（再帰する union のノード） |
| `tag_bytes` | int / null | union だけ。保存するタグの byte（`niche` は `null`） |
| `niche` | string / null | union だけ。`negative_length`・`null_pointer`（WASM には `null_pointer` の niche がない）・`null` |
| `cases` | array | union だけ。`[{"name","payload"}]`（`payload` は型の表示か `null`） |
| `drop_flag` | bool | 利用者の `Drop` の生存フラグを 1 byte 持つ |

### ライブラリ API

```rust
// src/kb/mod.rs
pub const SCHEMA_FACTS: &str = "tz-facts/1";
pub fn collect(project: &crate::driver::Project) -> Result<KbModel, DiagnosticSet>;
pub fn collect_sources(sources: &[(&str, &str)]) -> Result<KbModel, DiagnosticSet>; // ("Main.tz", text)。名前空間なし・std 付き
// src/kb/model.rs
impl KbModel {
    pub fn write_symbols(&self, out: &mut impl std::io::Write) -> std::io::Result<()>;
    pub fn write_graph(&self, out: &mut impl std::io::Write) -> std::io::Result<()>;     // Phase 2
    pub fn write_tests(&self, out: &mut impl std::io::Write) -> std::io::Result<()>;     // Phase 2
    pub fn fact_modules(&self) -> Vec<&str>;                                              // Phase 3。key の昇順。行のあるモジュールだけ
    pub fn write_facts(&self, module: &str, out: &mut impl std::io::Write) -> std::io::Result<()>;
    pub fn layout_modules(&self) -> Vec<&str>;                                            // Phase 6。同じ
    pub fn write_layout(&self, module: &str, out: &mut impl std::io::Write) -> std::io::Result<()>;
    pub fn fact_keys(&self) -> &'static [&'static str];  // この build が出す facts のキー（上の順）
}
```

`collect` は解析（`Parallelism::library()`）を 1 回だけ行い、型検査が通らなければ `Err`。未出荷の Phase のメソッドは足さない。

### 互換性

- `docs/stability.md` の「安定として保つもの」の表に、1 行足す（手順 6）: 「KB の schema（`tz-facts/1`）: キーを消さず・改名せず、行の意味を変えない。キーと、`kind`・`rel`・`event`・`conv`・`layout` の語彙の追加は互換（読み手は知らないキー・語彙を無視する）。互換のない変更は番号を上げる」。ファイルとして出るのは G27 から。
- edition（D-45・D-54）に作用しない。言語の意味・構文・診断・IR・ABI を変えない。新しい診断コード・フラグ・環境変数・予約語・crate を足さない。

## 設計

### 全体像（2 つの視点）

```
inputs ─► analyze_inputs_captured ─► check_modules_collect
            │  H1: indexing ||= capture.is_some()。索引の複製を Capture.semantic へ
            │  H2: infer_copy_all の直後・polymorph::specialize の前に kb_capture::collect_abstract
            ▼
          Capture（宣言の視点: 単相化前。symbols・edges・tests・recursion・loops・events・function_ids）
最終の CheckedModule（単相化後）─► kb::facts（実体の視点: effects・sites・specializations）・kb::layout
Capture ＋ 最終モジュール ＋ SpanTable ─► KbModel ─► write_* （JSON Lines）
```

- **宣言の視点**（`Capture`）: ジェネリックな関数も 1 つの宣言として扱う。呼び出し・型の使用・所有権の事象・再帰・ループ・署名（`conv`）を、抽象本体から取る（型変数は `own`、Copy の制約を要るものは Copy。D2・D10）。
- **実体の視点**（最終の `CheckedModule`）: 具体型が分かる。効果・複製・確保・トラップ・特殊化・レイアウトを、実体ごとに求め、宣言へ集める（付録 C の対応規則）。

### フック（`src/check.rs`。変更は読み取りだけ）

| # | 場所 | 変更 |
| --- | --- | --- |
| H1 | `check_modules_collect` の `let indexing = ...` と `if indexing { let index = semantic::collect(..) ..}` | `indexing` に `|| capture.is_some()` を足し、ブロックの中の `*requested = index;`（`index` を移動する）の**前**に `if let Some(capture) = capture.as_deref_mut() { capture.semantic = Some(index.clone()); }`。`semantic` に渡す分の挙動は変えない |
| H2 | `let copy_constraints = crate::ownership::infer_copy_all(..)` の `?` の直後 | `if let Some(capture) = capture.as_deref_mut() { kb_capture::collect_abstract(capture, &Abstract { modules, module: &module, names: &names, classes: &classes, declarations: &function_declarations, references: &references }); }`。`module` はまだ単相化前で、`polymorph::specialize(module, ..)` が消費する前 |
| 引数 | `check_modules_collect(.., semantic, parallelism)` | `capture: Option<&mut kb_capture::Capture>` を `parallelism` の前に足す。呼び出し 2 か所（`check.rs` の `check_modules_indexed_all`、`src/warnings.rs` のテスト）は `None` |
| 入口 | `check_modules_indexed_all` の隣 | `pub(crate) fn check_modules_captured(modules, capture: &mut kb_capture::Capture, parallelism) -> Result<CheckedModule, DiagnosticSet>`（本体は `check_modules_indexed_all` と同じで、`Some(capture)` を渡す） |
| `src/lib.rs` | `analyze_inputs_with` の本体 | 本体を `pub(crate) fn analyze_inputs_core(inputs, semantic, cache, parallelism, capture: Option<&mut check::kb_capture::Capture>)` に移し、`analyze_inputs_with` は `None` で呼ぶ。最後の `check::check_modules_indexed_all` の呼び出しを `match capture { Some(c) => check::check_modules_captured(&modules, c, parallelism), None => check::check_modules_indexed_all(&modules, semantic, parallelism) }` にする。`pub(crate) fn analyze_inputs_captured(inputs, capture, parallelism)` を足し、成功・失敗のあとで `capture.semantic` に `retain_spelled(&texts)` をかける（`analyze_inputs_semantic` と同じ） |
| `src/polymorph.rs` | `impl Classes`（すべて `pub(super)`） | `class_name(id) -> &str`、`class_is_builtin(id) -> bool`、`method_name(class, method) -> &str`、`method_signature(class, method) -> Option<&Signature>`、`instances() -> Vec<InstanceView>`（`InstanceView { class, head: Type, derived, span, methods: Vec<(usize, String)> }`。`methods` は `InstanceTemplate.methods` の関数 id と、`class.methods` の名前を zip したもの） |

### データ構造

```rust
// src/kb_capture.rs（check の子モジュール。`use super::*;`。Names・Classes を読める）
#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) semantic: Option<semantic::SemanticIndex>,
    pub(crate) modules: Vec<CapturedModule>,                  // user と dependency だけ（std は入れない）
    pub(crate) symbols: Vec<CapturedSymbol>,                  // Span 付き。位置の解決は kb 側
    pub(crate) edges: Vec<CapturedEdge>,                      // calls・implements・imports・instance_of・tested_by
    pub(crate) tests: Vec<CapturedTest>,
    pub(crate) function_ids: BTreeMap<(String, String), String>, // (CheckedFunction.module, 基底の名前) → symbol の ID
    pub(crate) recursion: BTreeMap<String, &'static str>,     // symbol の ID → none・self・self_tail・mutual（Phase 4）
    pub(crate) loops: BTreeMap<String, Vec<(&'static str, Span)>>, // Phase 4
    pub(crate) events: BTreeMap<String, Vec<CapturedEvent>>,  // Phase 5
}
pub(crate) struct CapturedModule { pub key: String, pub full: String, pub origin: String, pub source: usize, pub usings: Vec<(String, Span)> }
pub(crate) struct CapturedSymbol { pub id: String, pub kind: &'static str, pub module: String, pub name: String, pub signature: String,
    pub params: Vec<CapturedParam>, pub result: Option<String>, pub public: bool, pub export: bool, pub doc: Option<String>,
    pub deprecated: Option<String>, pub origin: String, pub span: Span, pub extent: Span }
pub(crate) struct CapturedParam { pub name: String, pub ty: String, pub conv: &'static str }
pub(crate) struct CapturedEdge { pub rel: &'static str, pub from: String, pub to: String, pub args: Option<Vec<&'static str>>, pub span: Span }
pub(crate) struct CapturedTest { pub id: String, pub kind: &'static str, pub module: String, pub name: String, pub index: usize, pub span: Span, pub function: usize }
pub(crate) struct CapturedEvent { pub var: String, pub event: &'static str, pub to: Option<String>, pub span: Span }
// H2 の時点の借用（`check_modules_collect` の局所変数）。`pub(crate) fn collect_abstract(capture: &mut Capture, input: &Abstract<'_>)`
pub(crate) struct Abstract<'a> { pub modules: &'a [ModuleInput<'a>], pub module: &'a CheckedModule, pub names: &'a Names, pub classes: &'a Classes,
    pub declarations: &'a [(String, FunctionDecl)], pub references: &'a [Vec<usize>] }
```

`extent` は宣言の範囲（名前の先頭から本体の終わり。付録 C の表 C1）で、`uses_type` の「最も内側の宣言」の決定に使う。`src/kb/model.rs` の公開の型（`KbModel`・`SymbolRow`・`GraphRow`・`TestRow`・`FactsRow`・`LayoutRow`・`Location`・`SpanTable`）は、`Location { path: String, start, end, line, column }` を持ち、`SpanTable::new(inputs: &[SourceInput<'_>])` が行頭の byte offset の表を作って `locate(span) -> Option<Location>` を答える。`FactsRow` のキーは Phase ごとに `Option<_>` で、`None` のキーは出さない。

### 事実の抽出元（型・解析・関数）

| 事実 | 視点 | 入力（型・関数） | 取り方 |
| --- | --- | --- | --- |
| symbols の宣言 | 宣言 | `ModuleInput.program`（`Program` の全 field を `..` なしで分解。新しい宣言の種類を足すと compile error になる。手本 `api_diff::add_items`） | 付録 B の表 B3 |
| `conv`・`params` の型 | 宣言 | 抽象の `CheckedFunction.signature.parameters`（`function_ids` の対応）、class のメソッドは `Classes::method_signature` | 付録 B の表 B4 |
| `calls` | 宣言 | 抽象本体の `TypedExprKind::Call(callee, arguments)`（`TypedExpr::children` で走査） | 付録 D の表 D1・D2 |
| `uses_type` | 宣言 | `Capture.semantic.references` と `definitions`（`Definition.kind`・`module`・`name`）、`CapturedSymbol.extent` | 付録 D の表 D4 |
| `implements`・`instance_of` | 宣言 | `Classes::instances()`、`RecordDecl.derives` | 付録 D の表 D5 |
| `tested_by`・`recursion` | 宣言 | `references: Vec<Vec<usize>>`（`recursion::check` の結果）、`CheckedModule.tests` | 付録 D の D3、本文の A3・A4 |
| `loops` | 宣言 | 抽象本体の `While`・`ForRange`・`ForEach` | 付録 E の表 E1 |
| `ownership` | 宣言 | `ownership.rs` の sink（`check_body` の `infer = true`） | 付録 G の表 G1 |
| `effects`・`effect_sites` | 実体 | 最終の `functions` の本体の走査と呼び出しグラフ。A20 があればその要約 | 本文の A6 |
| `copy_sites` | 実体 | `copies::sites(&module)` | `CopySite` の `function` を根へ。スパンで重複を除く |
| `trap_sites` | 実体 | 最終本体の走査＋ `ranges::analyze`・`index_in_bounds`・`enter_condition` | 付録 E の表 E3、本文の A5 |
| `alloc_sites` | 実体 | A20 の規則。無ければ `NewArray`・`NewList`・`NewLiteral` | 付録 E の表 E1 |
| `specializations` | 実体 | 最終の `functions` の名前の `.$mono.` | 根ごとの個数 |
| `layout` | 実体 | `llvm::layout::{layout_bound, record_slots, union_layout, tag_width, drop_flag}`（`target_scope(false)` の中） | 付録 G の表 G2 |

### アルゴリズム

**A1. 整列・重複除去・上限・衝突**（`src/kb/model.rs` の `finish`）: ① 行を第 1 キー→スパンで整列し、完全に同じ行を除く。② ID の衝突は、ソース位置順（`span.source`・`start`）の 2 件目以降に `#n` を付ける（`ids::Disambiguator`。`symbols` と、その ID を参照する `graph`・`tests`・`facts` の `id` を同時に書き換える）。③ 配列は整列後に 100 件で切る。

**A2. 最終の関数の根**（付録 C の表 C2）: 最終の関数 `f` について、`f.module` が `$lambda`・`$task` なら `f.origin.parent` の関数（持ち上げ元の名前つきの関数）を `f` とみなす。名前が `.$mono.` を含めば、その前までを基底の名前とする。`(module, 基底の名前)` で `Capture.function_ids` を引き、見つからない関数（std・生成された補助関数・`$export`・`$builtin`・`$case`・`$intrinsic`）は事実の対象外（効果の伝播では通す）。

**A3. `tested_by`**: 各 test の関数 id `t` から `references` を幅優先（キューは `VecDeque`、`visited: BTreeSet`）でたどる。深さ 0 は `t` 自身（`from` にしない）。深さ 1〜8 の関数のうち、`function_ids` の逆引きで user の宣言（kind が `def`・`method`・`extern`。test・bench・`$entry` を除く）になるものを、訪れた順に 200 件まで。1 件ごとに `tested_by(from = その関数, to = test)`。

**A4. `recursion`**: `references` を（Kosaraju。`recursion::check_graph` と同じ。**明示のスタック**）強連結成分に分ける。成分の大きさが 2 以上なら `mutual`。1 で自己辺があれば、抽象本体の直接の自己呼び出し（`Call` の callee が `Function(User(自分))`・`GenericFunction(自分, _)`、または `Binary(Pipe, 引数, callee)`）をすべて調べ、**全部**が「末尾の自己呼び出し」なら `self_tail`、1 つでも違う（自己の参照だけ・非末尾）なら `self`。自己辺がなければ `none`。末尾の自己呼び出し: 本体の末尾位置（本体・`If` の両枝・`Block` の `result`・`Match` の各腕の本体。`llvm.rs` の `emit_tail` と同じ）にあり、かつ `llvm.rs` の `is_self` と同じ条件（どの引数の型も `carries_loans` でない）。

**A5. `trap_sites`**（最終本体を、明示のスタックで評価順に走査する）: 関数ごとに `ranges::analyze(&module, function, false)` で `facts` を作る。`Block` の束縛の後に `facts.bind` は呼ばない（`intervals = false`）。`If` は条件を走査し、then の前に `enter_condition`、後に `leave_condition`、else は何もしない（`llvm.rs` と同じ）。各ノードは付録 E の表 E3 で site を出す（`Match` のパターンの射影の `Index`・`ListTail` は出さない。E3 の注）。`Index(array, index)` の `proven` は `facts.index_in_bounds(array, index)`。整数の `Divide`・`Remainder` の `proven` は、除数が `Int(n)` の定数で `n != 0`（ゼロ除算）、`n != -1`（オーバーフロー。符号なしは site を出さない）。`ForRange` の `proven` は step が `Int(n)` で `n != 0`。site は `(span, kind)` で重複を除く。

**A6. 効果（Level B。A20 が無いとき）**: 付録 F の写像で、最終本体を走査する。① 関数ごとに、直接の効果の集合と、効果ごとの最小のスパン（`(source, start)`）の式、参照先の関数 id（`Function(User(id))`・`Closure(id, _)` の参照と、そのスパン）を求める。② 呼び出しグラフの固定点（`BTreeSet` のワークリスト。関数は id 順、参照先はスパン順）で、`effects(f) = 直接 ∪ ⋃ effects(参照先)`。③ `effect_sites[e]` は、`f` が直接に持てばその式（`via = []`）、持たなければ、`e` を持つ参照先のうち参照のスパンが最小のもの `c` を選び、`span = その参照のスパン`、`via = (c に ID があれば [c の ID]、なければ []) ++ c の via`（ID があるのは、`function_ids` に在る user の関数と std の関数。持ち上げたラムダ・生成された補助関数は ID がないので連鎖に入れず、その先へ進む）。関数値の呼び出し（`Call` の callee が `Function`・`Closure` 以外）は辺にしない。④ `effects_complete = false`。**Level A**（A20 Phase 1 が done で `src/effects.rs` が在る）: 付録 F の F2（`rows` を根へ集約）。

**A7. 所有権の sink（Phase 5）**: 付録 G の表 G1 の 5 か所で `OwnershipEvent` を push する。`eval_call` は、引数ごとに `let mark = self.events.len()`、`self.eval(argument, ..)` のあと、`mark..` のうち `span == argument.span` の事象の `to` を、callee が既知の user の関数なら `User(id)`、組み込み関数なら `Builtin(..)`、class のメソッドなら `Method(..)` にする。終わりに `(span.start, span.end, event の順)` で整列し、重複を除く。`events_abstract(module)` は `copy_reads` と同じ形（関数ごとに `check_body(.., infer = true, .., events)`）で、`Vec<Vec<OwnershipEvent>>` を返す。ジェネリックな関数は抽象本体で 1 回だけ解析する。`copy` の事象は、Phase 4 の `copy_sites` を、事象の列にも足す（別の解析は走らせない）。

**A8. レイアウト（Phase 6）**: `layout::target_scope(false)` を開いて（ガードを保つ）から、最終の関数の署名・局所・式の `ty` を走査して具体的な `Type::Record`・`Type::Union`（user の宣言のもの）を `BTreeSet<Type>` に集め、型引数のない user の record・union を足す。型ごとに付録 G の表 G2 の行を作る。再帰する union（`types.recursive`）は `layout: "node"`、`size = [4, 8]`・`align = [4, 8]`（`layout_bound` の答え）。

## 前提とする他チケットのインターフェース

- **G22**（Phase 5 だけ）: Phase 2 が `ownership.rs` の `State` に「移動の起点の span」「loan の作成の span」を持たせる。本チケットの sink は同じ 2 か所（`read_places` の `if moving {`、`E::Borrow` の `loan(..)`）の直前・直後に push するだけで、`State` を変えない。G22 が先に入っているときは、その場所に G22 の記録が在るので、その隣に sink の push を足す（互いに診断を変えない）。G22 が未完でも Phase 5 は出荷できる。
- **A20**（Phase 3・4 の `effects`・`effect_sites`・`alloc_sites`）: 名前は A20 のチケットの 7.6 と付録 F から。`crate::check::effects`（`check.rs` の `#[path = "effects.rs"] pub mod effects;`）の `summarize(&CheckedModule, Options { scope: Scope::All, computed: EffectSet::ALL, sites: true }) -> Result<FunctionEffects, Diagnostic>`、`FunctionEffects::rows(&self, &CheckedModule) -> Vec<EffectRow>`（`EffectRow { id, effects, unknown, sites: Vec<EffectSite { effect, span, why, via }>, complete }`）、`FunctionEffects::sites(id) -> &[Site]`（`Site { effect, span, why, exempt }`）、`FunctionEffects.computed`。写し方は付録 F の F2。名前が違えば読み替える（意味は変えない）。意味が違えば停止条件 7。
- **A19**: `FunctionDecl.pure`（あれば `symbols` の `pure`）。無ければ `false`。
- **G27**: `KbModel` の `write_*`・`fact_modules`・`layout_modules`・`fact_keys` を呼んでファイルに書く（件数は G27 が書いた行数から作る）。`collect` の `Err` のとき、G27 が `manifest.json` に `complete: false` と `diag.jsonl` だけを書く（D15）。G27 は `check`・`build` の解析の中で KB を作るため、`analyze_inputs_core`（`pub(crate)`）と `KbModel::build(inputs, &module, capture)`（`pub(crate)`）を使う。
- **G30**: `calls` の `args`、`symbols` の `kind`・`signature`、`facts` の `ownership`・`layout` を、Mermaid の入力にする。キーの追加は互換。
- **E15**: `effects` に `extern` を、`calls` の `to` に std の ID を使う。

## 実装手順

各手順の最後で `cargo build --locked` と `cargo test --locked` が緑であること。テスト先行の手順で置くテストは `#[ignore = "手順 N で有効にする"]` を付け、`cargo test --locked --test kb -- --ignored` で**失敗する**ことを確かめてから、手順 N で `#[ignore]` を外して通す。

### Phase 1: 基盤と symbols

### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の `running N tests` の合計を控える。`mkdir -p target/tz-G26/before && cp target/release/tsuzuri target/tz-G26/tsuzuri-before`。`examples/point`・`examples/polymorphism` を `target/tz-G26/tsuzuri-before build <dir> --emit llvm -O0 -o target/tz-G26/before/<名前>.ll` で出す（手順 4 の IR 比較に使う）。付録 J のコマンドで `check` の時間を控える。
- 確認: すべて通る。失敗があれば、本チケットと無関係であることを記録する。

### 手順 2: テスト先行（fixture・golden・テスト）

- 変更: `tests/fixtures/kb_basic/`（新規。付録 A の 5 ファイル）、`tests/golden/kb/point/symbols.jsonl`・`tests/golden/kb/polymorphism/symbols.jsonl`（新規。付録 H の H1・H2 をそのまま貼る）、`tests/kb.rs`（新規）、`src/kb/mod.rs`（新規。仮の実装）、`src/lib.rs`（`pub mod kb;`）。
- 内容: fixture を置き、`tsuzuri check`（警告なし）・`fmt --check`・`test`（1 件通る）を確かめる。`src/kb/mod.rs` は、空の `pub struct KbModel;`、何も書かない `KbModel::write_symbols`、`E2000`（メッセージ `kb is not implemented yet`）の `Err` を返す `collect`・`collect_sources` だけ。`tests/kb.rs` に次を `#[ignore = "手順 5 で有効にする"]` で書く: `symbols_of_examples_point_match_the_golden_file`・`symbols_of_examples_polymorphism_match_the_golden_file`（`kb::collect_sources` に `examples/<名前>/` の全ファイルを `("<相対パス>", text)` の形で渡し、`write_symbols` の出力が golden と**完全一致**）、`kb_basic_symbols_cover_every_kind`（付録 H の H3 の kind 別の行数）、`symbols_are_sorted_unique_and_deterministic`（2 回の出力が同じ。入力の順序を逆にしても同じ。`id` が byte 順で一意）、`duplicate_test_names_get_suffixes`（`test "same"` 2 つと `bench "same"` が `test:Main.same`・`test:Main.same#2`・`bench:Main.same`）、`collect_fails_on_a_type_error`（`let x: i64 = "a"` の入力が `Err` で、最初の診断のコードが `E1003`）。名前空間つきの fixture は `Project::load(tests/fixtures/kb_basic)` で読む。
- 確認: `cargo test --locked` が手順 1 と同じ件数で通る（新規は ignored）。`cargo test --locked --test kb -- --ignored` が 6 件とも失敗する。

### 手順 3: JSON 書き手と ID

- 変更: `src/kb/json.rs`・`src/kb/ids.rs`（新規）、`src/kb/mod.rs`（`mod json; mod ids;`）。
- 内容: `json.rs` に `Object`（`new()`・`string(key, &str)`・`raw(key, &str)`・`uint(key, usize)`・`boolean(key, bool)`・`null(key)`・`strings(key, &[String])`・`finish() -> String`）を書く。エスケープは `diagnostic::json_string` を呼ぶ。`ids.rs` に `module_id`・`item_id`・`member_id`・`instance_id`・`test_id`・`bench_id`・`type_id_text`（付録 B の B2）・`Disambiguator`（A1 の ②）を書く。単体テスト: `escapes_like_diagnostic_json_string`（`"`・`\`・改行・U+0001・非 ASCII）、`writes_keys_in_the_given_order`、`composes_ids_per_the_decision_table`（本文の ID の表の全行）、`suffixes_duplicate_ids_in_source_order`、`type_id_text_covers_every_type_variant`（付録 B の B2 の全 variant）。
- 確認: `cargo test --locked kb::` が 5 件以上通る（`running N tests` の N を見る）。`cargo test --locked` が緑。

### 手順 4: capture の配管（挙動は変えない）

- 変更: `src/kb_capture.rs`（新規。`Capture` と空の `collect_abstract`）、`src/check.rs`（`pub(crate) mod kb_capture;`・`check_modules_collect` の引数・H1・H2・`check_modules_captured`）、`src/lib.rs`（`analyze_inputs_core`・`analyze_inputs_captured`）、`src/warnings.rs`（テストの呼び出し）、`src/kb/mod.rs`（単体テスト）。
- 内容: 「設計」のフックの表のとおり。`kb_capture` の宣言は `semantic` と同じ形（`#[path = "kb_capture.rs"]` の次の行に `pub(crate) mod kb_capture;`）で、`semantic` の隣に置く。H2 の `collect_abstract` はこの手順では何もしない。`src/kb/mod.rs` の `#[cfg(test)]` に `analysis_with_capture_keeps_the_ir_identical` を足す（`analyze_inputs_captured` は `pub(crate)` なので統合テストから呼べない）: `examples/point`・`examples/polymorphism`・`tests/fixtures/kb_basic` の各入力を、`analyze_modules`（capture なし）と `analyze_inputs_captured` の両方で解析し、`llvm::emit(.., Entry::Library)` の IR が文字列として同じで、`warnings` も同じ。
- 確認: `cargo test --locked` が緑で、件数が手順 1 ＋新規。手順 1 の `.ll` と、新しいコンパイラの `--emit llvm -O0` の出力が 1 byte も変わらない。

### 手順 5: symbols の抽出と書き出し

- 変更: `src/kb_capture.rs`・`src/polymorph.rs`（読み取り口）・`src/kb/model.rs`・`src/kb/symbols.rs`（新規）・`src/kb/mod.rs`（`collect`・`collect_sources`）・`tests/kb.rs`（`#[ignore]` を外す）。
- 内容: ① `polymorph.rs` に `Classes` の読み取り口を足す。② `collect_abstract` に、モジュール行・宣言の行（付録 B の B3 の全 kind。`$entry`・instance・instance のメソッドを含む）・`function_ids`・`tests` を作る処理を、付録 C の順で書く（`Program` は `..` なしで分解する）。③ `SpanTable`・`Location`・`SymbolRow`・`write_symbols`（キー順は本文の表）・`KbModel::build`（A1 の整列・重複除去・衝突）を書く。④ `collect_sources`・`collect` を書く（`analyze_inputs_captured` を 1 回呼ぶ）。
- 確認: 手順 2 の 6 件が `#[ignore]` なしで通る。`cargo test --locked` が緑。付録 H の H1・H2 と golden が 1 byte 違わない。

### 手順 6: 文書（Phase 1）

- 変更: `_tsuzuri/language-reference/compiler/kb-format.md`（新規）、`_tsuzuri/language-reference/index.md`、`docs/architecture.md`、`docs/stability.md`。
- 内容: 「ドキュメント」の節のとおり。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/kb-format.md` が通る。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が通る。

### Phase 2: graph と tests

### 手順 7: テスト先行（graph）

- 変更: `tests/golden/kb/point/graph.jsonl`（新規。付録 H の H1）、`tests/kb.rs`。
- 内容: `#[ignore = "手順 8・9 で有効にする"]` で次を書く: `graph_of_examples_point_matches_the_golden_file`、`kb_basic_graph_has_every_rel`（6 種類の `rel` が出て、`calls` は付録 H の H3 の件数）、`calls_args_conv_cases`（`own`・`ref`・`ref_mut`・`copy` と、値としての参照 `[]`。付録 D の D2 の例を `collect_sources` の入力にする）、`tested_by_depth_and_cap`（`f1` から `f9` まで 1 つずつ次を呼ぶ鎖を test が呼ぶと、`f1`〜`f8` だけが出る。250 の別々の関数を呼ぶ test は 200 件で止まる）、`tests_jsonl_lists_tests_and_benches`。
- 確認: `cargo test --locked` が緑（新規は ignored）。

### 手順 8: `calls`・`implements`・`instance_of`・`imports`・`uses_type`

- 変更: `src/kb_capture.rs`・`src/kb/graph.rs`（新規）・`src/kb/model.rs`・`src/kb/mod.rs`。
- 内容: 付録 D の表 D1〜D5 のとおり。`calls` は抽象本体を `children()` で走査して `Call` の callee と引数を見る（明示のスタック）。`uses_type` は `Capture.semantic.references` から作り、「最も内側の宣言」は `CapturedSymbol.extent` で決める。`write_graph` を書く。
- 確認: `graph_of_examples_point_matches_the_golden_file`・`calls_args_conv_cases` の `#[ignore]` を外して通る。

### 手順 9: `tests.jsonl` と `tested_by`

- 変更: `src/kb_capture.rs`・`src/kb/graph.rs`・`src/kb/model.rs`。
- 内容: A3 のとおり。`tests.jsonl`（`write_tests`）。
- 確認: 手順 7 の残りの `#[ignore]` を外して通る。`cargo test --locked` が緑。付録 H の H3 の件数と一致。

### Phase 3: facts の基盤と効果

### 手順 10: テスト先行（効果）

- 変更: `tests/kb.rs`。
- 内容: `#[ignore = "手順 11・12 で有効にする"]` で: `effects_debug_with_via`（`Main.traced` に `debug`、`effect_sites` が `debug` の 1 件で `via == ["Debug.print"]`）、`effects_are_transitive_through_user_functions`（関数 A が B を呼び B が `Debug.print` を呼ぶと、A の `via == ["B", "Debug.print"]`）、`effects_complete_is_false_without_a_20_summary`（`src/effects.rs` が無いビルドで `false`）、`facts_rows_exist_for_every_function_like_symbol`（`def`・`extern`・`test`・`bench`・`method`・`$entry` の行数）。
- 確認: `cargo test --locked` が緑。

### 手順 11: 最終の関数の根と `FactsRow`

- 変更: `src/kb/facts.rs`（新規）・`src/kb/model.rs`・`src/kb/mod.rs`。
- 内容: A2（付録 C の表 C2）を `attribute(function, module, capture) -> Option<String>` として書く。`FactsRow`（Phase 3 のキーだけ）・`write_facts`・`fact_modules`・`fact_keys` を書く。行は `symbols` の kind が `def`・`extern`・`test`・`bench`・`method`・`$entry` のものに 1 行（`id`・`effects: []`・`effect_sites: []`・`effects_complete: false`・`truncated: {}`）。
- 確認: `facts_rows_exist_for_every_function_like_symbol` の `#[ignore]` を外して通る。

### 手順 12: 効果（Level B。A20 があれば Level A）

- 変更: `src/kb/effects.rs`（新規）・`src/kb/facts.rs`・`tests/kb.rs`。
- 内容: A6 のとおり。付録 F の写像（89 個。`DynOf` は効果なし）を `match` に書き、`Builtin` の 246 項目すべてを名前で並べる（残りの 157 個は「効果なし」と明示）。ノードの上限（共通の規則 8）を守る。`src/effects.rs` が在る（A20 Phase 1 が done）なら Level A（付録 F の F2。`summarize` が `Err` のときは Level B）、無ければ Level B だけを実装する。
- 確認: 手順 10 の 3 件の `#[ignore]` を外して通る。`cargo test --locked` が緑。付録 H の H4 の `Main.traced` の行と一致。

### Phase 4: 箇所の事実

### 手順 13: `copy_sites`・`recursion`・`loops`

- 変更: `src/kb/sites.rs`（新規）・`src/kb_capture.rs`（`recursion`・`loops`）・`src/kb/facts.rs`・`tests/kb.rs`。
- 内容: `copy_sites` は `copies::sites(&module)` を根へ集め、スパンで重複を除く。`recursion` は A4、`loops` は付録 E の表 E1。先にテスト（`#[ignore]`）: `copy_sites_match_copies_sites`（`kb_basic` の `Main.copies` が 1 件で、`copies::sites` の件数と一致）、`recursion_classes`（`sum_to`＝`self_tail`・`fib`＝`self`・`is_even`・`is_odd`＝`mutual`・`total`＝`none`）、`loops_kinds`（`countdown`＝`while`、`total`＝`for_range`）。
- 確認: 3 件が通る。`cargo test --locked` が緑。

### 手順 14: `trap_sites`

- 変更: `src/kb/sites.rs`・`tests/kb.rs`。
- 内容: A5 のとおり。付録 E の表 E3 の全行（`kind`・`proven` の決まり）を実装する。先にテスト: `trap_sites_proven_and_unproven`（`total` の `bounds_check` が `proven: true`、`at` が `false`、`ratio` が `integer_division_by_zero` と `integer_division_overflow` の 2 件で `false`、`assert` が `assert`）。
- 確認: 通る。IR と同じ事実（`total` の IR に境界検査がない）であることを、手順 1 の `.ll` の読みで確かめる。

### 手順 15: `alloc_sites` と上限

- 変更: `src/kb/sites.rs`・`src/kb/model.rs`・`tests/kb.rs`。
- 内容: Level C は `NewArray`・`NewList`・`NewLiteral`（`alloc_complete = false`）。A20 Phase 3 が done（`computed` に `alloc`）なら Level A（付録 F の F2。`alloc_complete = true`）。テスト: `alloc_sites_new`（`Main.filled` が `new_array` を 47:18 に 1 件。`Main.copies` の配列リテラル `[n, n, n]` は `new` でないので 0 件）、`arrays_truncate_at_100`（150 回の `assert` を持つ関数で `trap_sites` が 100 件、`truncated.trap_sites == 50`）。
- 確認: `alloc_sites_new` と `arrays_truncate_at_100` が通る。`cargo test --locked` が緑。

### Phase 5: 所有権の出来事

### 手順 16: sink（`ownership.rs`。診断を変えない）

- 変更: `src/ownership.rs`（本体と単体テスト）。
- 内容: 付録 G の表 G1 の `OwnershipEvent`・`EventKind`・`EventTarget` と、`Checker` の `events: Option<&'run mut Vec<OwnershipEvent>>`、`check_body` の引数、`events_abstract(module)` を足す。呼び出し 3 か所（`check_functions`・`check_recovered`・`copy_reads`）は `None`。先にテスト（`src/ownership.rs` の `#[cfg(test)]`。`events_abstract` が `pub(crate)` のため）: `ownership_sink_does_not_change_diagnostics`（`tests/fixtures` と `examples` の全プロジェクトで、sink あり・なしの `check_all` の診断が同じ）。
- 確認: 既存の所有権のテスト（`cargo test --locked ownership`・`borrow`・`last_use`）が 1 件も変わらず通る。

### 手順 17: 抽象本体の事象と書き出し

- 変更: `src/kb_capture.rs`・`src/kb/events.rs`（新規）・`src/kb/facts.rs`・`tests/kb.rs`。
- 内容: `collect_abstract` から `events_abstract` を呼び、`CapturedEvent` に写す（`to` は付録 C の `function_ids` と `Classes` で ID にする）。`FactsRow.ownership` に、`copy` の事象（`copy_sites`）を足して書く。テスト: `ownership_events_for_kb_basic`（付録 H の H5）、`generic_function_events_use_the_abstract_body`（`Shapes.twice` の `ownership` が `[]`。型変数 `'a` を 2 回読んでも `move` が出ない）。
- 確認: 通る。付録 H の H5 と一致。

### Phase 6: レイアウトと特殊化

### 手順 18: 可視性と `layout`

- 変更: `src/llvm.rs`（`mod layout;` → `pub(crate) mod layout;`）、`src/kb/layout.rs`（新規）・`src/kb/model.rs`・`tests/kb.rs`。
- 内容: A8 と付録 G の表 G2。テスト: `layout_of_packed_reorders`（`Shapes.Packed` が `size [16,16]`・`align [8,8]`・`reordered: true`・`slot` が `a=1`・`n=0`・`b=2`）、`layout_union_kinds`（`Shapes.Shape` が `common`・`tag_bytes 1`・`size [16,16]`）。
- 確認: `cargo test --locked` が緑。IR が 1 byte も変わらない（手順 4 の比較を再実行）。

### 手順 19: `specializations` と測定・文書

- 変更: `src/kb/facts.rs`・`tests/kb.rs`・`docs/architecture.md`・`_tsuzuri/language-reference/compiler/kb-format.md`。
- 内容: `specializations` は最終の `functions` の名前の `.$mono.` を根ごとに数える。テスト `specializations_count`（`Shapes.twice` が 2、ほかが 0）。付録 J の手順で `check` と KB の時間を測り、結果を本チケットの末尾の「実装と検証」に書く（閾値なし）。文書に Phase 2〜6 を足す。
- 確認: `cargo test --locked` が緑。`node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/kb-format.md` が通る。`cargo test --locked --test kb -- --ignored dump` が `target/kb-dump/` に出力を書く（`#[ignore]` の測定ツール）。

## テスト計画

- **期待値の出どころ**: golden と件数の表は付録 H の手計算（Python でスパンを数えた。コンパイラの出力を写さない）。実装の出力と違うときは停止条件 6。
- Rust（`tests/kb.rs`）: 手順 2・7・10・13〜18 に名前を書いたもの。入力は `examples/point`・`examples/polymorphism`・`tests/fixtures/kb_basic`（新規）・インラインのソース（`collect_sources`）。拒否（`collect_fails_on_a_type_error`）は診断コード `E1003` を照合する。
- 単体（`src/kb/*.rs`。新規）: 手順 3 の 5 件。`src/kb/effects.rs` の `every_builtin_has_a_verdict`（`Builtin::ALL` の 246 項目すべてに判定がある。不純は付録 F の 90 個）、`src/kb/facts.rs` の `honors_the_exact_node_limit`（小さい `Limits` で、ちょうどの境界で `truncated.nodes == 1`）。
- 決定性: `symbols_are_sorted_unique_and_deterministic`（2 回の出力、入力の順序の逆転）。Phase 3 以降も同じ検査を `facts`・`layout` に広げる。
- 資源上限: `arrays_truncate_at_100`・`honors_the_exact_node_limit`。
- 測定ツール: `#[ignore]` の `dump_kb_for_shipped_projects`（`tests/fixtures`・`examples`・`benchmarks` の全プロジェクトで `collect`。成功件数・失敗の診断・`check` と KB の時間を表示し、`target/kb-dump/<名前>/` に書く。手本 `tests/copy_inventory.rs`）。
- 既存テストへの影響: **なし**。根拠: capture を使わない解析は仕事を足さず（手順 4 の IR・警告の一致）、`indexing` は警告を変えず、`ownership` の sink は診断を変えない（手順 16）。コンパイルだけ変わるのは `src/warnings.rs` のテストの呼び出し 1 か所。`tests/fixtures` を走査する `tests/formatter.rs`・`tests/llvm_handoff.rs` に `kb_basic` が加わるので、fixture は整形済みで、`Main.tz` にトップレベルのコードを持たない。
- native と WASM の `-O0`／`-O3`: コード生成に触れないので対象外（IR の不変は手順 4・18 の比較で確かめる）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/kb-format.md`（新規）: KB の各ファイルのキー・型・整列・省略規則、ID の規則、`kind`・`rel`・`event` の語彙、限界（付録 D の限界）。「ファイルへの書き出しと CLI は計画中（[G27](G27-build-time-kb.md)）」と書き、`tsuzuri::kb::collect` の使い方を示す。`index.md` の「コンパイラ」の一覧に足す。
- `docs/architecture.md`: ファイル表に `src/kb/`・`src/kb_capture.rs` の行、「Facts と KB の exporter」の節（2 つの視点・フック）。`docs/stability.md`: 「安定として保つもの」の表に schema の 1 行。`docs/language.md`・`README.md`: 更新なし（言語と CLI が変わらない。PR の説明に書く）。`_features/README.md`・GUIDE の台帳は統括者が更新する。
- 検証: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/kb-format.md _tsuzuri/language-reference/index.md`。

## 受け入れ条件

- [ ] Phase 1: `symbols.jsonl` が付録 H の H1・H2 と byte 単位で一致し、`kb_basic` の kind 別の行数が H3 と一致する。IR・診断・警告が変わらない（手順 4）。`cargo fmt`・`clippy -D warnings` が緑。
- [ ] Phase 2: `graph.jsonl` が H の表と一致し、6 種類の `rel` が出る。`tests.jsonl`・`tested_by` の深さと件数の上限が効く。
- [ ] Phase 3: `Main.traced` の `debug`・`via` が表のとおり。`effects_complete` が A20 の有無と一致。全 `Builtin` に判定がある。
- [ ] Phase 4: `copy_sites`（W1006 と同じ位置）・`trap_sites`（IR の境界検査の有無と一致）・`recursion`・`loops`・`alloc_sites` が表のとおり。100 件で切り、`truncated` に件数。
- [ ] Phase 5: `Main.ownership` の事象が H5 と一致。既存の所有権のテストと診断が変わらない。
- [ ] Phase 6: `Packed` が `reordered: true`、`specializations` が表のとおり。IR が変わらない。
- [ ] 全 Phase: 出力が決定的（2 回・入力順の逆転で同じ）。文書が更新され、`check-docs.mjs` が通る。測定の結果を「実装と検証」に書いた。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| 最終の `CheckedModule` に抽象のジェネリック関数がない。宣言の視点の事実を最終の本体から作ると、使われないジェネリックが消える | `kb_basic` の `Shapes.twice` が `specializations` 2 と、`symbols` と `ownership` の両方に出る |
| `FunctionOrigin.parent` は、持ち上げたラムダでは親の関数 id、`.$mono.` のインスタンスでは**自分自身**の id | A2 の対応表を付録 C の C2 どおりに書く。`attribute` の単体テスト |
| `copies::sites`・`ranges::analyze` は単相化後のモジュール用。抽象のモジュールに呼ぶと型変数で誤る | Phase 4 は最終のモジュールだけに呼ぶ（呼ぶ関数に抽象のモジュールを渡す経路を作らない） |
| `llvm::layout` を開く前に `layout_bound` を呼ぶと、メモが無く、WASM の niche の既定が変わる | `target_scope(false)` のガードを `layout.rs` の入口で保つ |
| `HashMap`・`HashSet` の反復順、`Vec::sort`（不安定な整列は同じキーの順を変える） | `BTreeMap`・`sort_by`（安定）。2 回の出力の比較テスト |
| Windows の `relative_path` は `\` 区切り | `path` は `/` に直す。`Location` の単体テスト |
| `Type::display` は名前空間を含まない。ID に使うと、依存パッケージの同名の型が衝突する | ID は `type_id_text` だけ。`display` は `type`・`signature` の表示だけ |
| fixture にトップレベルのコードや整形されていないソースを置くと、`llvm_handoff`・`formatter` のテストが落ちる | `tsuzuri fmt --check`・`check`（警告なし）を手順 2 で通す |
| `check_body` の `copies` と `events` を同時に渡すと、`copy` の事象が重複する | `copy` は `copy_sites` から作り、sink は `events` だけ（A7） |
| 明示のスタックにせず再帰すると、debug ビルドのテスト（2 MiB の stack）で溢れる | 走査はすべて明示のスタック。入れ子 128 の式を含む `tests/kb.rs` のテスト |

## 対象外

目的の「非目標」のほか: `@trusted`・権限（E15）。型エイリアスの `uses_type`（索引に使用が載らないため）。演算子のオーバーロードが呼ぶ instance のメソッドの `calls`（D7）。

## 決定事項

### D1: facts は型付き IR と所有権解析だけから作る

LLVM の IR・clang を使わない（`check` で動き、`-O` に依らない）。却下: `llvm_traps.rs` の IR の逆引きでトラップを求める案（IR がなく、`-O` に依る）。状態: 承認済み。

### D2: 2 つの視点（宣言と実体）

宣言の視点（単相化前）で `symbols`・`graph`・`ownership`・`recursion`・`loops`、実体の視点（単相化後）で `effects`・各 `*_sites`・`specializations`・`layout`。却下: 全部を最終モジュールで（使われないジェネリックが消え、事象が実体の数だけ増える）／全部を抽象で（型が分からず `trap`・`copy` を決められない）。状態: 承認済み。

### D3: capture は `check` の子モジュールで、`Option<&mut Capture>` を渡す

`Names`・`Classes` を読める。`semantic` の手本と同じ。却下: 解析を 2 回走らせる（費用が倍）、`Names` のフィールドを公開（可視性が広がる）、`CheckedModule` を複製（メモリ）。状態: 承認済み。

### D4: `symbols` は AST の宣言が駆動し、索引は補う

instance・test・bench・`$entry` は `SemanticIndex` の `definitions` にない。`Program` の全 field を `..` なしで分解する（宣言の種類を足すと compile error）。状態: 承認済み。

### D5: ID は完全名（名前空間つき）

key だけだと、依存パッケージの同名のモジュールが衝突する。std は `std::` を付けない（D-52 の `Array.map`）。却下: `Type::display` の名前（名前空間を含まない）。状態: 承認済み。

### D6: `uses_type` は `SemanticIndex.references` から

自前の名前解決を作らない。限界: 索引に載る使用（署名・instance の頭・構築式・注釈）だけ。型エイリアスの使用は載らない。状態: 承認済み。

### D7: 演算子のオーバーロードの呼び出しは `calls` に出さない

`classes.operation`・`intrinsic` の読み取り口が要り、費用に見合わない。`tested_by` の到達性は `references`（`recursion_targets` を含む）を使うので影響しない。拡張点: 読み取り口を足せば `calls` に足せる。状態: 承認済み。

### D8: 効果は 2 段階（A20 の有無）

Level A（A20 の `summarize`。`effects_complete` は A20 が 14 個すべてを計算するとき `true`）、Level B（本チケットの走査。A19 の分類の 10 個。`false`）。`effects_complete`・`alloc_complete` は D-52 の一覧への追加（足すだけ）。却下: A19 の `Scanned`（関数ごとに最初の 1 件しか持たない）の再利用。状態: 承認済み。

### D9: `proven` の意味

範囲の証明（`ranges.rs`）または定数の除数・step で、起きないと示せたもの。`proven: true` も列に残し、`trap` の効果には数えない。リソースの枯渇（`allocation_size`・`allocation_failure`・`string_concat_overflow`・`stack_overflow`）は出さない（D-53）。`unreachable` は `TrapKind` にないので名前をそのまま使う。状態: 承認済み。

### D10: 所有権の事象は抽象本体（`infer = true`）から

Copy の制約を要る型変数は Copy とみなし、事象にしない。`drop` は「そのブロックの出口で必ず落ちる」（移動されうるものは出さない）。`copy` は `copy_sites`。ラムダの本体の事象は `eval_lambda` が走査した分だけ（限界）。状態: 承認済み。

### D11: レイアウトのサイズは下限・上限で、実測ではない

`layout_bound` の `lower`。ネイティブ 64 bit の値。WASM は `null_pointer` の niche がない（`niche` に明記）。状態: 承認済み。

### D12: 決定性と上限

1 スレッド・整列・100 件・ノード上限。上限超過は `truncated` で、エラーにしない（facts は補助）。状態: 承認済み。

### D13: schema は `tz-facts/1` のまま、出荷した Phase のキーだけ出す

`fact_keys()` が出すキーを返す。読み手は欠けたキーを「不明」と扱う（空と扱わない）。状態: 承認済み。

### D14: G22 への依存は Phase 5 だけ

Phase 1〜4・6 は診断 v2 の型を使わない。D-50 の表（依存 G22）からの限定で、G22 が未完でも Phase 1 から着手できる。状態: 承認済み。

### D15: 失敗した解析の KB は作らない

`collect` は `Err(DiagnosticSet)`。G27 が `complete: false` と `diag.jsonl` を書く。却下: 構文だけの部分的な `symbols`（`conv` を決められない）。状態: 承認済み。

## 付録

[G26-appendix.md](G26-appendix.md): A fixture `kb_basic`／B ID と `symbols` の決定表／C 関数と宣言の対応／D `graph` の表／E `TypedExprKind` の分類と trap・alloc／F 効果の縮退版の写像／G 所有権の事象とレイアウトの表／H 想定出力（手計算）／J 実測と測定の手順。
