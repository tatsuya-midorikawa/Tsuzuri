# G30: 図表の生成（Mermaid。モジュール・型・呼び出し・所有権・並行・レイアウト・状態遷移）

| 項目 | 内容 |
| --- | --- |
| ID | G30 |
| 優先度 | P2 |
| 規模 | L（Phase 1 M・Phase 2 M・Phase 3 M・Phase 4 M・Phase 5 M） |
| 依存 | [G26](_completed/G26-facts-model.md)（Phase 1〜2 の `symbols`・`graph`。Phase 2 の効果の色は Phase 3 の `facts`、Phase 3 は Phase 5 の `ownership`、Phase 5 のレイアウト表は Phase 6 の `layout`）、[G27](G27-build-time-kb.md)（Phase 2 の `KbData`・`open`・`finish`・`manifest.json`）。弱い依存: [G28](G28-kb-query-and-impact.md)（`kb` の引数の形と ID の解決。未完なら本チケットの規則）、[G22](_completed/G22-diagnostic-v2.md)（`related` の role 名だけを表に写す。なくても動く） |
| 後続 | [G29](G29-mcp-server.md)（`tz_diagram`）、[G35](G35-generated-docs-single-source.md)（`views/*.mmd` を文書に取り込む） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc` で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | プロジェクトの構造（モジュールの依存・型の関係・呼び出し・所有権の流れ・並行・メモリの配置・状態の遷移）は、AI も人間もソースを全部読んで頭の中で組み立てるしかない。KB（G26・G27）は JSONL の行で、関係を一目で見る形がない。文書の Mermaid 76 個は手書きで、1 つは Mermaid 12.1.0 で parse できない（「現状」4） |
| 手本にする既存実装 | JSON のエスケープ: `src/diagnostic.rs` の `json_string`。ハッシュ: `src/cache.rs` の `Sha256`（`field`）と `read_regular`。ソースの構文木: `src/parser.rs` の `parse_with_source_all`、`src/syntax.rs` の `Program`・`FunctionDecl`・`ExprKind`・`PatternKind`・`Edition`。KB の書き込み: G27 の `kb::build::finish`・`kb::write::publish`。`kb` の分岐: G27 の `src/kb/command.rs` |
| 主な影響ファイル | 新規: `src/diagram/{mod,input,resolve,escape,graph,mermaid,dot,validate,module,types,views}.rs`（Phase 1）、`calls.rs`（Phase 2）、`ownership.rs`（Phase 3）、`concurrency.rs`（Phase 4）、`layout.rs`・`transitions.rs`（Phase 5）、`tests/diagram.rs`・`tests/diagram_cli.rs`、`tests/fixtures/diagram/`・`tests/golden/diagram/`、`_tsuzuri/language-reference/compiler/diagrams.md`。変更: `src/lib.rs`（`pub mod diagram;`）、`src/kb/command.rs`（`diagram` の分岐）、`src/kb/build.rs`（`finish` に views）、`src/kb/store.rs`（Phase 5 で `KbData` に `layouts` を足す）、文書（「ドキュメント」）。**`Cargo.toml` は変えない** |

詳しい検証記録・参照実装・fixture・期待出力は 2 つの付録にある。付録 A〜D（Mermaid の制約の実験・fixture・期待出力・負の例）は [G30-appendix.md](G30-appendix.md)、付録 E〜F（参照実装と検証の手順）は [G30-appendix-reference.md](G30-appendix-reference.md)。**参照実装（付録 E の Python）の出力が期待出力の正本**で、本文の表と食い違ったら、参照実装に合わせて本文を直す。

## KG への更新（D-60、2026-10-28）

本チケットは [D-60](GUIDE.md#d-60-kb-から-kg知識グラフへの再設計g26g27g28g29g30) で KB から **KG（知識グラフ）** に改めた。コマンドは `tsuzuri kg diagram`、`Input::from_kg(&KgData)`（本文の `from_kb`）は `nodes.jsonl`・`edges.jsonl` の行を読む（キーは同じ）。

| 本文 | KG での実装 |
| --- | --- |
| `module` の循環（Tarjan） | `src/kg/algo.rs` の強連結成分（反復版）を使う |
| `types` のメンバーの所有者（ID の最後の `.` の前） | `contains` の辺の親（結果は同じ。ID を解析しない） |
| `calls` の幅優先 | `algo::bfs` と同じ順の規則（§`calls` の手順 2） |
| `views/*.mmd` | KG の `views/`（`manifest.json` の `files` に入る） |
| 付録 C の行・付録 E の参照実装 | 行のキーは同じ。ファイル名だけ `nodes`・`edges` に読み替える |

## 実装者向け早見

1. **作るもの**: `tsuzuri kb diagram KIND [ID]`（KIND は `module`・`types`・`calls`・`ownership`・`concurrency`・`layout`・`transitions` の 7 つ）、ライブラリ関数 `diagram::render`（G29 が呼ぶ）、KB の `views/modules.mmd`・`views/types.mmd`・`views/concurrency.mmd`。出力は Mermaid か DOT で、`ownership` と `layout` は Markdown（表つき）。
2. **最初に読む**: 付録 A（Mermaid の制約。実験で確かめた）→「仕様」→ 付録 E の `diagram_ref.py`（出力の正本）→ `src/diagnostic.rs` の `json_string`、`src/cache.rs` の `Sha256`。
3. **最初に書くテスト**: 手順 2 の `tests/diagram.rs::module_diagram_of_point_matches_the_golden`（先に赤にする）。
4. **触ってはいけない**: G26 の行の形、G27 の `manifest.json` の形（`files` に `views/*.mmd` が増えるだけ）、`check`・`build` の出力、`Cargo.toml`（crate を足さない。正規表現も使わない）。
5. **新しい名前**: オプション `--format`・`--max-nodes`・`--depth`・`--path`（`kb diagram` の中だけ。D-55 に足す）、`E2000` の kind 9 個（「診断」）。新しい診断コード・環境変数・予約語はない。
6. **設計の核**: フローチャート系の 4 種は `Graph` の 1 つのモデルから Mermaid と DOT を書く。ID は `n<連番>`、ラベルは必ず `"…"` で囲んでエスケープする。生成した Mermaid は、必ず構造検証器（`validate`）を通る。
7. **出力は決定的**: 同じ KB・同じ引数なら 1 byte も変わらない。整列は ID の byte 順。時刻・絶対パス・スレッド数・`HashMap` の反復順を入れない。
8. **ソースを読むのは `transitions` だけ**。ほかの 6 種は KB の行だけから作る（facts にない情報を足さない）。
9. **Phase の完了**: 1＝`module`・`types`・views・`kb diagram` の骨格。2＝`calls`。3＝`ownership`。4＝`concurrency`。5＝`layout`・`transitions`。各 Phase は期待出力（付録 D）と 1 byte も違わず、付録 F の harness（`mermaid` 12.1.0）で全 `.mmd` が parse でき、`dot -Tplain` が DOT を通す。
10. 各手順の最後で `cargo build --locked`・`cargo test --locked`・`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が緑。`cargo test <名前>` は 0 件でも成功するので `running N tests` の N を確かめる。

## 目的

**問題。** KB の `symbols.jsonl`・`graph.jsonl`・`facts/*.jsonl`・`layout/*.jsonl` は機械が読む行で、「どのモジュールがどのモジュールに依存するか」「この関数から先で何が呼ばれ、どれが入出力をするか」「この変数はいつ移動・借用されるか」「この型は何 byte か」を見るには、行を自分で結合して頭の中で絵にするしかない。文書の Mermaid 76 個（flowchart 73・sequenceDiagram 2・stateDiagram-v2 1）は手書きで、生成物ではない。

**価値。**

1. 7 種の図を、KB の行から決定的に作る。AI は MCP の `tz_diagram`（G29）で、人間は `tsuzuri kb diagram` で、ソースを全部読まずに構造を見る。
2. Mermaid は GitHub・VS Code の Markdown プレビューがそのまま描く。DOT は Graphviz に渡せる。
3. **生成した Mermaid が必ず parse できる**ことを、エスケープの表と構造検証器と、公式の `mermaid` 12.1.0 での検証（付録 F）で保証する。予約語（`end`・`call`）と特殊文字（`"`・`<`・`>`・`#`・`(`）を生成物に出さない。
4. `views/*.mmd` が KB の一部として、毎回の KB の組み立てで作り直される。

**HEAD で測った数字**: 文書の Mermaid は 76 個で、`mermaid` 12.1.0 の `parse` を通らないのは 1 個（`_tsuzuri/language-reference/built-in-types-and-modules/gpu.md` の 310 行目の `flowchart TD`。ノード ID `call` が `click … call` の予約語に当たる。**直さない**。報告だけ）。`mermaid` 12.1.0 の既定は、テキスト 50,000 文字（`maxTextSize`）、フローチャートの辺 500 本（`maxEdges`）。

**非目標**: `sequenceDiagram` を作ること（どの図も使わない。D11）、SVG・PNG の描画、`mmdc` を必須にすること、wasm32 のレイアウトの数値（G26 の `layout` が native だけ。D7）、`ownership` の変数間の関係、失敗するプログラムの図（KB が作られない。D6）、データフローに基づく並行の図（近似。D8）、図の対話的な操作、G28 の照会コマンド。

## 着手条件と停止条件

### 着手条件

- G26 の Phase 1〜2（`symbols`・`graph`）と G27 の Phase 2（`KbData`・`open`・`finish`・`kb` の分岐）が main にある。確認: `grep -n "pub fn finish" src/kb/build.rs` と `grep -n "pub fn from_parts" src/kb/store.rs` がそれぞれ 1 件。
- Phase 2 の効果の色は G26 Phase 3（`facts` の `effects`）、Phase 3 は G26 Phase 5（`ownership`）、Phase 5 の `layout` は G26 Phase 6 がある Phase だけ出す。**ない Phase は、そのコマンドを `E2000`（`diagram_unavailable`）にして先へ進む**（手順を飛ばす）。
- 付録 F の harness が動く: `node` 20 以上と `npm install mermaid@12.1.0 jsdom` ができる。ない環境では、期待出力の parse の確認だけを飛ばし、完了報告に書く。

### 停止条件

1. G26 の行のキー名・G27 の `Files`・`manifest.json` が本文と違う。`input.rs`（1 か所）だけを直す。直せない（情報が足りない）なら止める。
2. G27 の `kb verify` が `views/*.mmd` を「余分なファイル」として拒否する、または `files` に入れる場所がない。止める（G27 を直す提案を書く）。
3. 期待出力（付録 D）のどれかが、付録 F の harness で parse できない。止める（版の違いか、本文の誤り）。
4. views を足すと、`kb build` が 5% より遅くなる（手順 12 の測定）。止める。
5. G28 が `kb` の引数（位置引数・`--path`・`--no-refresh`）を本文と違う形に決めた。**G28 に合わせ**、本文との差を完了報告に書く（本文の規則は G28 がない場合の既定）。
6. `parse_with_source_all` の署名が変わり、`transitions` が構文木を取れない。Phase 5 の `transitions` だけ止める。

## 現状（HEAD `a2313d19`）

作業ツリーは `9dd0fbbc`（main）で、ソースは同じ。以下は `target/release/tsuzuri` と `npm` の `mermaid` 12.1.0（2026-10-18 に取得）での実測。

1. **`diagram` も `kb` もない。** `ls src | grep -i "diagram\|kb"` は 0 件。`tsuzuri kb` は `kb:1:1: error[E2001]: cannot inspect source 'kb': No such file or directory (os error 2)`（終了コード 1）、`tsuzuri kb diagram module` は `<command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively`（終了コード 2）。`src/lib.rs` の 10〜11 行目は `pub mod diagnostic;`・`pub mod docgen;`（`pub mod diagram;` はこの間に入る）。`Cargo.toml` の依存に `serde_json = "1.0"` はあり、`regex` はない。
2. **構文木の入口はある。** `src/parser.rs` の `pub fn parse_with_source_all(source: &str, source_id: usize, edition: Edition) -> Result<Program, Vec<Diagnostic>>`。`FunctionDecl { name, parameters, result, body, .. }` の `body` は、`define` が先頭の `ExprKind::Lambda` を剥がした後の式で、引数名は `parameters[i].name.text` にある。共用体の case の名前は、パターンでは `PatternKind::Binding(Ident)`（引数なし）か `PatternKind::Apply(Ident, Vec<Pattern>)`（引数あり）で、パーサーは「`.`・`:` で区切った最後の語が ASCII 大文字で始まる」ものを case として扱う（`src/parse_control.rs` の 1136 行付近）。式では `ExprKind::Name(Ident)`・`ExprKind::Call(Box<Expr>, Vec<Expr>)`。
3. **モジュールの循環は許される。** `A.tz`（`record A`・`def a_of :: B -> i32`）と `B.tz`（`record B`・`def b_of :: A -> i32`）は `tsuzuri check` が通り、`run` は `3` を出す（付録 B の `cycle`）。関数の相互再帰は別で、モジュールをまたぐと `E1019`。
4. **文書の Mermaid の検査**（付録 F の harness）: `grep -rl '^```mermaid' _tsuzuri docs README.md` は 73 ファイル、76 個。73 個の `flowchart`・2 個の `sequenceDiagram`・1 個の `stateDiagram-v2` のうち、`gpu.md` の 1 個（`call["…"]` と `call --> usable{…}`）だけが `Parse error … got 'CALLBACKNAME'`。ID を `call` から変えれば通る。
5. **Mermaid の制約**（公式の文書と `mermaid` 12.1.0 の `parse`・DB で確かめた。全文は付録 A）:

| 制約 | 根拠 | 生成物での扱い |
| --- | --- | --- |
| 小文字の `end` をノード ID にすると壊れる。`o`・`x` で始まるノード ID は `---` の直後で辺の記号に化ける | flowchart.md の警告。`end["b"]` を parse して失敗（付録 D の負の例） | ID は `n<連番>`・`s<連番>` だけ |
| `call` は `click … call` の予約語で、ノード ID にすると失敗 | `gpu.md` の実例 | 同上 |
| 特殊文字は `"…"` で囲めば書ける。`"` 自体は書けない | flowchart.md「Special characters that break syntax」。`say "hi" now` を parse して失敗 | ラベルは必ず `"…"`。中の `"` は `#quot;` |
| `#35;`（10 進）と `#quot;`（HTML 名）のエンティティコードで文字を書ける | flowchart.md「Entity codes」。`#35;`・`#quot;`・`#lt;`・`#gt;`・`#amp;`・`#40;` が DB で placeholder になる | 6 文字（`#`・`"`・`<`・`>`・`&`・`` ` ``）を必ずコードにする |
| 引用の中の `<b>x</b>` は HTML タグのまま残る（`Array<Point>` の `<Point>` が消える） | 実験（付録 A） | `<`・`>` は `#lt;`・`#gt;` |
| 引用の中の `(`・`)`・`[`・`]`・`{`・`}`・`\|`・`%%`・`;`・`\` は書ける | 実験 | そのまま書く |
| 引用の先頭の `` ` `` は Markdown 文字列になる | 公式の markdown strings | `` ` `` は `#96;` |
| classDiagram の中身（メンバー）に `(` があるとメソッド扱い | 実験（`+score(ref 'a) i32` が `methods` に入る） | フィールド・case のテキストの `(`・`)` は `#40;`・`#41;`、class のメソッドだけ括弧を使う |
| stateDiagram の `classDef` は「開始・終了状態と、複合状態の中・複合状態」に使えない。複合状態の内側から別の複合状態へ遷移できない | stateDiagram.md の「Styling with classDefs」「Composite states」 | 所有権は**変数ごとに 1 つの図**（複合状態を使わない） |
| 辺ラベル `-->\|"…"\|` は引用できる。classDiagram の関係のラベル `: text` は引用しない（引用符がタイトルに残る） | 実験 | 関係のラベルは `esc` した裸のテキスト |
| テキストは 50,000 文字、フローチャートの辺は 500 本まで | `config.md`（`maxTextSize`・`maxEdges`） | 45,000 文字・400 本を上限にする（D12） |

## 変えてはいけない意味

- G26 の行（`symbols`・`graph`・`facts`・`layout`）のキーと値。G30 は**読むだけ**で、読むキーは付録 C の表の列だけ（ほかのキーは無視する）。
- G27 の `manifest.json` の形と `kb verify` の意味。views が `files` に増えるだけで、既存のファイルの SHA-256・`counts`・`source_hash` は変わらない。
- `check`・`build`・`run`・`test` の出力と終了コード。`--kb` の既定の挙動。
- 診断コードの意味、IR、ABI、WASM の import、`Cargo.toml`。
- 既存の `kb` のサブコマンドの出力（`build`・`verify`・`clean`・`export` と G28 のもの）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 安全に出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 基盤（`escape`・`Graph`・Mermaid と DOT の書き手・構造検証器・`Input`・ID の解決）、`module`・`types`、`kb diagram` の骨格、`diagram::render`、views | M | G26 Phase 1〜2、G27 Phase 2 | 新しいコマンドと新しいファイル（`views/`）だけ。views は `files` に増えるだけ |
| 2 | `calls` | M | Phase 1（効果の色は G26 Phase 3） | `kind` を 1 つ足すだけ |
| 3 | `ownership` | M | Phase 1、G26 Phase 5 | 同上。`ownership` がなければ `E2000` |
| 4 | `concurrency`（views の `concurrency.mmd` も） | M | Phase 1 | 同上 |
| 5 | `layout`・`transitions` | M | Phase 1（`layout` は G26 Phase 6） | 同上 |

## 仕様

### CLI

```text
tsuzuri kb diagram KIND [ID] [--path PATH] [--kb-dir DIR] [--format mermaid|dot] [--max-nodes N] [--depth N] [--no-refresh] [--json]
```

| 引数 | 意味 |
| --- | --- |
| `KIND` | 最初の位置引数。下の表の 7 つ |
| `ID` | 2 つ目の位置引数。KIND ごとに必須か任意か決まる（下の表）。3 つ目の位置引数は `diagram_usage` |
| `--path PATH` | プロジェクト（G27 の `PATH` と同じ。既定 `.`） |
| `--kb-dir DIR` | G27 と同じ |
| `--format mermaid\|dot` | 既定 `mermaid`。KIND ごとの可否は下の表 |
| `--max-nodes N` | 5〜200 の整数。既定 60 |
| `--depth N` | `calls` だけ。1〜6 の整数。既定 2 |
| `--no-refresh` | KB を作り直さず、あるものを読む（G27 の `Refresh::Never`）。付けなければ `Refresh::Auto`（G27 §4 の鮮度の表のとおり） |
| `--json` | stdout に 1 行の JSON `{"kind":…,"format":…,"text":…,"nodes":n,"folded":b}`（キーの順はこのとおり。文字列は `diagnostic::json_string`）。付けなければ `text` だけを stdout に出す（末尾の改行は `text` のもの 1 つ） |

オプションは `ID` の前後どちらでもよい。同じオプションの繰り返し・未知のオプションは `E2000`。stdout には図だけを出し、通知は stderr に出す。終了コード: 0＝出力した、1＝`E2009`（KB の欠落・古い・壊れた。G27）、2＝`E2000`（引数の誤り・ID の誤り・図に要る事実がない・図が大きすぎる）。

| KIND | `ID` | 出力 `format` | `--format dot` | 図 |
| --- | --- | --- | --- | --- |
| `module` | 任意（モジュールの ID。そのモジュールと隣り合うモジュールだけ） | `mermaid` | 可 | `flowchart TD` |
| `types` | 任意（モジュールの ID。そのモジュールの型と、辺でつながる型） | `mermaid` | 可 | `classDiagram` |
| `calls` | 必須（`def`・`method`・`test`・`bench` の ID。`Main.$entry` も可） | `mermaid` | 可 | `flowchart TD` |
| `ownership` | 必須（同上） | `markdown` | **不可** | 変数ごとの `stateDiagram-v2` と台帳の表 |
| `concurrency` | 任意（モジュールの ID。その関数だけ） | `mermaid` | 可 | `flowchart LR` |
| `layout` | 必須（record・union の ID。`layout` の行の `id`） | `markdown` | **不可** | 表（図なし） |
| `transitions` | 必須（union の ID） | `mermaid` | 可 | `stateDiagram-v2` |

`--format mermaid` を `ownership`・`layout` に付けても（既定でも）出力は `markdown`（`DiagramOutput.format` が `markdown`）。`dot` を付けると `diagram_format_unsupported`。

### 共通の規則

1. **入力**: G30 は `Input`（`input.rs`）だけを読む。`Input::from_kb(&KbData)` が、`KbData` の `symbols`・`graph`・`facts`（`Fact.raw` の `ownership`）・`meta`（`edition`・`modules`）と、G30 が足す `layouts`（`layout/*.jsonl` の全行。「前提とする他チケットのインターフェース」）から作る。読むキーは付録 C の表の列だけで、ほかのキーは無視する。`Input::from_texts` は、同じ行を JSON Lines のテキストから作る（テスト用）。必須のキーがない行は `E2009`（kind `kb_broken`。`bad row in '{file}' line {n}: missing '{key}'`。`from_kb` では `{file}` が `symbols`・`graph`・`facts`・`layouts` のどれか）。`graph`・`facts`・`layouts` が `None`（ファイルがない）なら 0 行として扱う。
2. **ID の番号**: 図のノードの鍵（元の ID）を byte 順に整列して、1 から `n1`・`n2`… を振る（フローチャートと classDiagram）。状態は `s0`・`s1`…（所有権は変数ごとの図の中で、遷移の順）、`transitions` は case の宣言順に `s1`…。元の名前は**ラベル**に出す。
3. **エスケープ `esc(text, limit)`**（Mermaid の全部のラベル・メンバー・関係のラベルに使う）: ① `\t`・`\r`・`\n` を空白 1 つにする。② 文字数（Unicode scalar value）が `limit` を超えたら、先頭 `limit - 1` 文字に `…`（U+2026）を付ける（**切ってからエスケープする**）。③ 次の 6 文字を、文字ごとに 1 回だけ置き換える（置き換えた結果をもう一度置き換えない）: `#`→`#35;`、`"`→`#quot;`、`<`→`#lt;`、`>`→`#gt;`、`&`→`#amp;`、`` ` ``→`#96;`。`limit` は、ノードとメンバーのラベル 60、フローチャートの辺のラベル 40、classDiagram の関係のラベル 30、`stateDiagram-v2` の遷移のラベル（`ownership` は 60、`transitions` は 40）。
4. **DOT のエスケープ `dot_esc(text, limit)`**: ①②は同じ。③ `\`→`\\`、`"`→`\"`。DOT のラベルの中の改行は、`types` のメンバーだけが `\l`（左寄せの行末。リテラルの `\` と `l`）と `\n`（中央の改行）を**エスケープの後に**足す。
5. **テキストの形**: UTF-8、改行は `\n`、最後も `\n` で終わる。インデントは空白 4 つ（classDiagram のメンバーとアノテーションは 8）。空行・タブ・`\r` を出さない。
6. **上限**: ノード 200（`--max-nodes` の上限）、辺 400、テキスト 45,000 文字。辺が 400 を超えたら、整列した辺の先頭 400 本を残し、最後の行に `    %% omitted: {n} edges`（Mermaid のコメント）を足し、`folded` を `true`。テキストが 45,000 文字を超えたら、ラベルの `limit` を半分にして 1 回だけ作り直し、それでも超えたら `diagram_too_large`。
7. **`nodes`**: 図に出した要素の数。`module`・`calls`・`concurrency` は注記ノードを除くノード数、`types` は classDiagram のクラス数、`ownership` は図に出した変数の数、`layout` は表のフィールドか case の行数、`transitions` は case の数。`folded` は、畳んだか、辺か変数を切ったとき `true`。
8. **整列**: 辺は（始点の番号、終点の番号、ラベル（なければ空）、スタイル（`dashed` が `solid` より前））の順。関係（classDiagram）は（始点の番号、終点の番号、記号、ラベル）。クラス（`classDef` の順）は `pure`・`io`・`alloc`・`trap`・`ext`・`cycle`・`dep`・`region`・`cell`・`note`。

**Mermaid のフローチャートの行**（順序は、ヘッダー・ノード・辺・`classDef`・`class`・`style`・`linkStyle`）:

| 行 | 形 |
| --- | --- |
| ヘッダー | `flowchart TD` か `flowchart LR` |
| ノード | `    n1["L"]`（box）、`    n1("L")`（round）、`    n1(["L"])`（stadium）、`    n1[["L"]]`（subroutine）、`    n1{{"L"}}`（hexagon） |
| 辺 | `    n1 --> n2`、`    n1 -.-> n2`（dashed）。ラベル付きは `    n1 -->\|"L"\| n2`・`    n1 -.->\|"L"\| n2`（`\|` の前後に空白を入れない） |
| `classDef` | `    classDef {名前} {属性}`。使ったクラスだけ。属性は付録 E の `CLASS_DEFS` |
| `class` | `    class n1,n3 {名前}`（そのクラスのノードをノード順に） |
| `style` | `    style n1 stroke-width:3px`（起点のノードだけ） |
| `linkStyle` | `    linkStyle {辺の番号} stroke:#cf222e,stroke-width:2px`（赤い辺。番号は 0 から、整列した辺の順） |

**DOT の形**: `digraph tsuzuri {`、`    rankdir=TB;`（TD）か `LR;`、`    node [shape=box, fontname="monospace"];`、`    edge [fontname="monospace"];`、ノード `    n1 [label="…", shape=…, …];`、辺 `    n1 -> n2 [label="…", style=dashed, color="#cf222e"];`（属性があるものだけ）、`}`。形は box→`box`・round→`box`＋`style="rounded"`・stadium→`ellipse`・subroutine→`box`＋`peripheries=2`・hexagon→`hexagon`。クラスがあれば `style="filled"`（`ext`・`note` は `filled,dashed`）と `fillcolor`（付録 E の `DOT_FILL`）。起点は `penwidth=3`。`types` と `transitions` は付録 E の `types_dot`・`transitions_dot`。

### `module`

入力の行から、モジュールの依存を作る。**使う行**: `calls`・`uses_type`・`implements`・`instance_of` の `from`・`to` が、どちらも `symbols` にある ID で、`module` が違うもの。モジュールの組（`from` の module, `to` の module）ごとに行数 `n` を数える。

| 手順 | 規則 |
| --- | --- |
| 1 ノード | `kind` が `module` の全行。ID＝ノードの鍵、ラベル＝ID、形 box。`ID` を指定したら、そのモジュールと、組でつながるモジュールだけ |
| 2 辺（実線） | 組 `(a, b)`。ラベルは `n ≥ 2` のとき `×{n}`、`n = 1` はなし |
| 3 辺（破線） | `imports` の行（`from`＝モジュール、`to`＝`using` の名前空間）ごとに、`to + "::"` で始まり、その後ろに `::` を含まないモジュール `x`（`x` は `from` と違う）。組 `(from, x)` が手順 2 にないときだけ破線 `-.->`、ラベル `using` |
| 4 循環 | 手順 2 の辺（実線）だけで強連結成分（Tarjan）を求め、大きさ 2 以上の成分のモジュールはクラス `cycle`、その成分の中の実線の辺は赤（`linkStyle`） |
| 5 色 | `cycle` でなく `origin` が `project` でないモジュールはクラス `dep` |
| 6 畳み込み | ノードが `--max-nodes` を超えたら、次数（その組の数。実線も破線も、向きは問わない）の降順（同じなら ID）で `max - 1` 個を残し、残りを 1 つのノード `…N more modules`（クラス `dep`）にまとめる。辺は端点を写し、行数 `n` を足す（赤は付けない）。`folded` |

### `types`

| 手順 | 規則 |
| --- | --- |
| 1 ノード | `kind` が `record`・`union`・`class`・`instance` の行（`ID` を指定したら `module` が一致するものだけ）。ラベルは `name`。同じラベルが 2 つ以上あれば、その全部を ID にする |
| 2 組み込みの class | `instance_of` と、下の「derive」の `to` が、`symbols` にも図のノードにもない名前（`Eq`・`Add`）なら、その名前のノード（アノテーション `<<builtin>>`） |
| 3 メンバー | record は `field`（`+{name} {result}`）、union は `case`（`+{name}` に、`result` があれば ` {result}`）、class は `method`（`+{name}({param の type を ", " でつないだもの}) {result}`。`result` が `null` なら `unit`）。所有者は ID の最後の `.` の前。（`span.line`, `span.column`）の順に 12 個まで、超えたら `+… {n} more`。`instance` にはメンバーを出さない。メンバーの各テキスト（名前・型）は `member(text)` ＝ `esc(text)` の後ろで `(`→`#40;`、`)`→`#41;`（括弧が裸だと、フィールドがメソッドとして読まれる）。メソッドの外側の `(` `)` だけは、そのまま書く |
| 4 関係 | `instance_of`（`from` が instance）→ `from ..\|> to`（ラベルなし）。instance の ID が `instance:{class}<{head}>` で `head` が図のノードなら `from ..> head : for`。derive: `implements` の `from` がノードで、`instance:{to}<{from}>` というノードがないとき `from ..\|> to : derive`。合成: `uses_type` の `from` が `field`・`case` の行で、その所有者と `to` がどちらも図のノードで違うとき `所有者 --> to : {field か case の名前}` |
| 5 畳み込み | ノード（組み込みを含む）が `--max-nodes` を超えたら、モジュールごとに 1 つのクラス `{module} ({n})`（アノテーション `<<module>>`）、組み込みは `builtin ({n})`（`<<builtin>>`）にし、関係は異なるグループの組ごとに 1 本（2 本以上なら ` : ×{n}`）の `-->` |

### `calls`

| 手順 | 規則 |
| --- | --- |
| 1 起点 | `ID`（`def`・`method`・`test`・`bench`）。レベル 0 |
| 2 探索 | `calls` の行（`from`→`to`）を、レベル 1…`depth` まで幅優先でたどる。各レベルの前の層の ID を byte 順に、各ノードの行を（`span.line`, `span.column`, `to`）の順に見て、初めて現れた `to` をそのレベルに入れる |
| 3 ノード | たどった全部（`symbols` にない `to`＝組み込み・std は外部）。ラベルは ID。レベル `depth` のノードで、集合にない `to` への行があるものは、ラベルの末尾に ` …`（空白つき）。起点は太枠（`style`） |
| 4 形 | 外部＝stadium、`name` が `$entry`＝subroutine、`kind` が `method`＝round、`test`・`bench`＝hexagon、ほかは box |
| 5 クラス | 外部＝`ext`。`symbols` にある関数は、`facts` の行があれば次の最初の一致: `effects` が {`io`,`os`,`net`,`task`,`atomic`,`rc`,`extern`,`debug`,`user_drop`,`gpu`} と交わる→`io`、{`alloc`,`copy`}→`alloc`、{`trap`,`diverge`}→`trap`、`effects_complete` か `symbols` の `pure` が真→`pure`、どれでもなければクラスなし。`facts` の行がなければクラスなし |
| 6 辺 | 集合の中のノード間の `calls` の行を（`from`, `to`）ごとにまとめる（レベルをまたぐ辺・戻る辺・自己ループも含む）。ラベル: 各行の `args` を `,` でつないだ文字列のうち、空でなく重複しないものを、位置順に ` / ` でつなぐ。行が 2 行以上なら末尾に `×{行数}`（ラベルがあれば空白 1 つ挟む）。**辺の重み**は行数 |
| 7 畳み込み | ノードが `--max-nodes` を超えたら「畳み込み」の規則（起点を残す。グループ＝`module`、外部は `std:{最初の `.` の前}`、`.` がなければ `std:builtin`） |

### `ownership`

入力は、`ID` の `facts` の行の `ownership`（G26 Phase 5）。その行がない、または `ownership` のキーがなければ `diagram_unavailable`（`the KB has no ownership facts for '{id}'; build the KB with a compiler that records them`）。

| 手順 | 規則 |
| --- | --- |
| 1 出来事 | `var` が `$` で始まるものを除き、（`span.line`, `span.column`, `var`）の順に整列 |
| 2 変数 | 出来事の `var` を最初に現れた順に。`copy` 以外の出来事がある変数だけを図にする。12 を超えたら、`move`・`borrow`・`borrow_mut` のある変数だけにし、それでも 12 を超えたら先頭の 12。図にしなかった変数の数 `k` は、`k` があれば「`{k} variables are not drawn; the table lists every event.`」の 1 行 |
| 3 遷移 | 変数ごとに状態の列（下の表）。最初は `state "Owned" as s0` と `[*] --> s0`。出来事ごとに新しい状態 `s{連番}` を作る。**借用は、その変数の次の出来事まで続く**: 現在が `Borrowed`・`BorrowedMut` で、次が `borrow`・`borrow_mut`・`move`・`drop` なら、先に `Owned` への遷移（ラベル `release`）を入れる。最後に `{最後の状態} --> [*]` |
| 4 色 | `Moved`・`Dropped` の状態はクラス `moved`、`Borrowed`・`BorrowedMut` はクラス `borrowed`（`classDef` を図の末尾に。使ったものだけ） |
| 5 台帳 | 全出来事（200 行まで）の表 `\| # \| line \| var \| event \| to \| diagnostic role \|`。`diagnostic role` は、`move`→`moved_here`、`borrow`・`borrow_mut`→`borrowed_here`、ほかは `-`（G22 が `E1012`・`E1014` の `related` に付ける role 名。**同じ位置が診断で引用される出来事を示す**） |

| 出来事 | 新しい状態 | ラベル |
| --- | --- | --- |
| `borrow` | `Borrowed` | `L{line} borrow` ＋ `to` があれば ` → {to}` |
| `borrow_mut` | `BorrowedMut` | `L{line} borrow_mut` ＋ 同上 |
| `reborrow` | 現在が借用中ならそのまま（`Borrowed`・`BorrowedMut`）、そうでなければ `Borrowed` | `L{line} reborrow` ＋ 同上 |
| `move` | `Moved` | `L{line} move` ＋ 同上 |
| `drop` | `Dropped` | `L{line} drop` |
| `copy` | （図に出さない。台帳には出す） | — |

出力（`markdown`）: `# Ownership of {ID}`、空行、変数ごとに `## {var}`（引数なら `## {var} (parameter: {type})`）・空行・```` ```mermaid ````・図・```` ``` ````・空行、注記、台帳。出来事が 0 件なら `(no ownership events)` の 1 行と空の表。

### `concurrency`

| 手順 | 規則 |
| --- | --- |
| 1 証拠（署名） | `kind` が `def`・`method` の行の `params[].type` と `result` を調べる。規則: テキストに `Channel.Sender<`（直前が英数字・`_` でない）→ `send`、`Channel.Receiver<`→ `recv`、`Mutex<`→ `lock`、`Atomic<`→ `atomic`。`<` から対応する `>` までが型 `T`（入れ子の `<>` を数える）。**引数**にあれば（関数, その関係, 種類, `T`）、**結果**にあれば関係を `create` にして同じ |
| 2 証拠（呼び出し） | `calls` の `to` が `Task.parallel`・`Task.scope`・`Parallel.` で始まるもの→その `from` は「区画」。`to` が `Channel.bounded`・`Mutex.create`・`Atomic.create` → `from` は `create`。型 `T` は、その種類（`Channel`・`Mutex`・`Atomic`）の `T` が手順 1 でちょうど 1 種類見つかっていればそれ、そうでなければ `?` |
| 3 ノード | 証拠か区画のある関数（`ID` を指定したら、その `module` のものだけ）。区画は subroutine・クラス `region`・ラベル `{ID} · {区画の呼び先を ", " でつないだもの}`。ほかは box。セル（`Channel<T>`・`Mutex<T>`・`Atomic<T>`）は round・クラス `cell`、鍵 `cell:{種類}<{T}>`、ラベル `{種類}<{T}>` |
| 4 辺 | `send`・`lock`・`atomic`: 関数→セル（実線、ラベルはその語）。`recv`: セル→関数（実線、`recv`）。`create`: 関数→セル（破線、`create`）。区画の関数から、ノードにある別の関数への `calls` の行: 破線 `calls`。まったく同じ辺は 1 本 |
| 5 注記 | ノード `approximate: from signatures and call sites`（クラス `note`。辺なし）を必ず足す。`nodes` と `ID` の絞り込みに数えない |
| 6 畳み込み | ノード（注記を含む）が `--max-nodes` を超えたら「畳み込み」（残す＝セルと区画。グループ＝`module`。上限は `max - 1`。注記は畳んだ後に足す） |

### `layout`

入力は、`ID` に一致する `layout` の行（G26 Phase 6。native の下限・上限）。なければ `diagram_unknown_id`（候補は `layout` の `id` から）、`layout` のファイルが 1 つもなければ `diagram_unavailable`。出力（`markdown`）は、見出し `# Layout of {ID} (native, 64-bit)`、空行、表 `\| item \| value \|`（`kind`・`size (bytes)`・`align (bytes)`・record は `reordered`、union は `layout`・`tag bytes`（`null` は `-`）・`niche`（`null` は `none`）・`drop flag`）、空行、次の表、空行、注意の 1 行（付録 E の `WASM_NOTE`）。サイズと整列は、下限と上限が同じなら 1 つの数、違えば `{下限}..{上限}`。

- record: 表 `\| slot \| field \| type \| size \| align \| offset \|`。`fields` を `slot` の昇順に。型が `i8 u8 bool`（1/1）・`i16 u16`（2/2）・`i32 u32 f32`（4/4）・`i64 u64 f64`（8/8）のどれかで、**それまでの全フィールドの大きさが分かっている**ときだけ、大きさ・整列・`offset`（直前の終わりを整列に切り上げた値）を書く。そうでなければ以降は `?`。全部分かり、`size` の下限＝上限なら、最後に行 `\| - \| (tail padding) \| - \| {size - 最後の終わり} \| - \| {最後の終わり} \|`。
- union: 表 `\| case \| payload \|`（`payload` が `null` は `-`）。

### `transitions`

入力は、`ID` の共用体 `U` の `case` の行（`id` が `{U}.` で始まり、残りに `.` がないもの。（`span.line`, `span.column`）の順）と、**ソース**（`Sources`。KB の `manifest.json` の `modules[].path`・`source` で引く）。

| 手順 | 規則 |
| --- | --- |
| 1 候補 | `kind` が `def` の行で、`params` が 1 つ以上あり、`head(params[0].type)` と `head(result)` が `U` の `name`（または `U` の ID）に等しいもの。`head(t)`＝先頭の `ref `・`ref mut ` を除き、最初の `<` から後ろを切り、前後の空白を除いた文字列 |
| 2 構文木 | 候補の `module` の `manifest` の `path` を `Sources::read` で読み、SHA-256（`cache::Sha256`）が `source` と違えば `E2009`（kind `kb_stale`。`source '{path}' changed after the KB was built; run tsuzuri kb build`）、読めなければ `kb_broken`（`cannot read source '{path}'`）。`parse_with_source_all(text, 0, Edition::parse(edition))`（`Err` なら `kb_broken`）の `Program.functions` から `name.text` が候補の `name` に一致する宣言を取る |
| 3 match | `p` = `parameters[0].name.text`。本体を次で探して、最初に見つかった `Match { value: Name(p), arms, .. }` を使う: `Block { result }`→`result`、`If`→`then_branch` の後に `else_branch`、`Checked(e)`→`e`、`Match`（`value` が `Name(p)` でないもの）→探さない。なければその候補は飛ばす |
| 4 腕の始点 | 腕の順に、パターンから始点の case を取る: `Binding(id)`・`Apply(id, _)` で `id.text` を `.`・`:` で区切った最後の語が case の名前→その 1 つ。`Binding` で最後の語が小文字で始まる、または `Wildcard`→「それまでの腕が名指ししていない case の全部」。`Or(a, b)`→両方の和（どちらかがワイルドカードならワイルドカード）。`As(p, _)`・`Annotated(p, _)`→`p`。ほかは始点なし（その腕を飛ばす）。ガード `guard` は無視する |
| 5 腕の終点 | 本体の末尾の式を次で辿る: `Name(id)`・`Call(Name(id), _)` で最後の語が case の名前→その 1 つ。`If`→両方の枝、`Block { result }`→`result`、`Match`→全部の腕の本体、`Checked(e)`→`e`、ほかは終点なし |
| 6 辺 | 始点 × 終点の全部を（始点, 終点, 関数の `name`）とし、（始点, 終点）ごとに関数名を重複なく辞書順に ", " でつなぐ。辺がまったくなければ `diagram_unavailable`（`no function of type {U} -> … -> {U} with a 'match' over its first parameter was found`） |
| 7 図 | `stateDiagram-v2`・`direction LR`、case ごとに `state "{name}[ of {payload}]" as s{n}`（宣言順）、辺は（始点の番号, 終点の番号）の順に `s1 --> s2 : {関数名}`。ラベルは `esc(…, 40)` |

### 畳み込み（`--max-nodes`）

ノードの数が上限を超えるとき（`module` は上の専用の規則）、`fold(graph, group_of, keep, max_nodes)`: ① `keep` にないノードを `group_of` でグループにまとめる。② `room = max_nodes - |keep|`。グループを（次数の降順、同じならグループ名）に並べ、`room` 以下ならすべて残し、超えたら先頭 `room - 1` を残して、残りは 1 つのノード `…{k} more groups ({m})`（`k` は残りのグループ数、`m` はそのノード数）にまとめる。グループのノードのラベルは `{グループ名} ({ノード数})`。③ 辺は端点をグループに写し、同じ組を 1 本にして重み（元の辺の重みの和）を足す。重みが 2 以上ならラベルは `×{重み}`、元の辺が 1 本でラベルがあればそのラベルを残す。同じグループの内側の辺は捨てる（`keep` のノードの自己ループは残す）。破線は、まとめた全部の辺が破線のときだけ。次数＝そのグループ（か `keep` のノード）に接する辺の数（元の辺。向きは問わない）。

### 構造検証器（`validate`）

`pub fn validate_mermaid(text: &str) -> Result<(), Vec<String>>`（`Err` は行番号つきのメッセージの一覧）。**G30 が作る形だけを受理する厳しい検査**で、一般の Mermaid の検証ではない。参照実装は付録 E の `validate_ref.py`（同じ規則を正規表現で書いたもの。Rust では `strip_prefix`・`split` で書く）。

| 検査 | 規則 |
| --- | --- |
| 全体 | 末尾が `\n` 1 つ、`\r`・タブ・空行がない、45,000 文字以下 |
| ヘッダー | `flowchart TD`・`flowchart LR`・`classDiagram`・`stateDiagram-v2` のどれか（`sequenceDiagram` は**拒否**。D11） |
| ID | `n<数>`（フローチャート・classDiagram）、`s<数>`（`stateDiagram-v2`）。大文字小文字を区別せず、`end`・`graph`・`flowchart`・`subgraph`・`style`・`class`・`classdef`・`click`・`call`・`linkstyle`・`default`・`direction`・`state`・`note` と一致するものは拒否（参照実装の `RESERVED`） |
| ラベル | `"` で囲んだ中（classDiagram のメンバーと関係のラベルは行の該当部分）に、生の `"`・`<`・`>`・`&`・`` ` ``・`\`・制御文字がなく、`#` は直後が `[0-9]+;` か `[a-z]+;` のときだけ |
| 行の形 | 上の「Mermaid のフローチャートの行」の表、classDiagram（`    direction LR`、`    class n1["L"] {`、`        <<…>>`（`record`・`union`・`class`・`instance`・`builtin`・`module`）、`        +…`、`    }`、関係 `    n1 --> n2`・`..\|>`・`..>`（` : ラベル` つき可））、`stateDiagram-v2`（`    direction LR`、`    state "L" as s1`、`    s1 --> s2[ : ラベル]`、`    [*] --> s1`、`    s1 --> [*]`、`classDef moved\|borrowed`、`class s1,s2 moved\|borrowed`）の形のどれかに一致する。ほかはすべて拒否 |
| 整合 | ノードの ID は重複しない。辺・`class`・`style` が指す ID は、宣言済み。`linkStyle` の番号は辺の数より小さい。`classDef` した名前は `class` で使われ、使われた名前は `classDef` されている。辺は 400 本以下、ノードは 500 以下 |

`mmdc`（Mermaid CLI）が PATH にあれば、テスト `golden_diagrams_parse_with_mmdc_when_available` が、期待出力ファイルのそれぞれを `mmdc -i {file} -o {target 下の出力}` で通す（ない環境では何もしない。ある環境で失敗したらテストの失敗）。

### views（KB の `views/*.mmd`）

`diagram::views(&Input) -> Vec<(String, String)>`（パスと本文。パスの順）。**KB を組み立てるたびに作り直し**、G27 の `Files` に足す（`manifest.json` の `files` に入る）。

| ファイル | 作る条件 | 中身 |
| --- | --- | --- |
| `views/modules.mmd` | `kind` が `module` の行が 2 以上 | `module`（`--max-nodes 60`、`ID` なし）の Mermaid |
| `views/types.mmd` | 図のノードが 1 以上 | `types`（同上）の Mermaid |
| `views/concurrency.mmd` | 注記を除くノードが 1 以上 | `concurrency`（同上）の Mermaid |

`calls`・`ownership`・`layout`・`transitions` は ID が要るので views にしない。views の生成が `diagram_too_large` で失敗しても、KB の組み立ては失敗させず、そのファイルを作らない。

### 診断（このチケットが出すもの）

新しいコードはない。`E2000`（終了コード 2。`<command line>:1:1`）の kind を 9 個足す。メッセージは英語で固定。

| kind | いつ | メッセージ |
| --- | --- | --- |
| `diagram_usage` | KIND がない、位置引数が 3 つ以上 | `usage: tsuzuri kb diagram KIND [ID] [options]` |
| `diagram_unknown_kind` | KIND が 7 つのどれでもない | `unknown diagram kind 'X'; use module, types, calls, ownership, concurrency, layout, or transitions` |
| `diagram_needs_id` | `calls`・`ownership`・`layout`・`transitions` に `ID` がない | `diagram kind 'calls' needs an id` |
| `diagram_format_unsupported` | `ownership`・`layout` に `--format dot` | `format 'dot' is not available for kind 'layout'` |
| `diagram_bad_option` | オプションの誤り | `--format must be mermaid or dot`、`--max-nodes must be an integer from 5 to 200`、`--depth must be an integer from 1 to 6`、`--depth is only valid with the calls diagram`、`{option} specified more than once`、`{option} needs a value`、`unknown option 'X'; use --help` |
| `diagram_unknown_id` | `ID` が行にない | `unknown id 'X' for diagram kind 'calls'`。候補（許される種類の ID のうち、最後の `.` の後ろが同じか、入力を含むもの。byte 順で最大 5 つ）があれば `; did you mean: A, B` |
| `diagram_wrong_id_kind` | `ID` の `kind` が違う | `'X' is a record; diagram kind 'calls' needs a function id`（`a function id`・`a type id`・`a union id`・`a module id`） |
| `diagram_unavailable` | その図に要る事実がない | 各 KIND の節のメッセージ |
| `diagram_too_large` | 45,000 文字を超える | `the diagram is too large ({n} characters); lower --max-nodes` |

KB の欠落・古い・壊れた・不完全（`kb_missing`・`kb_stale`・`kb_broken`・`kb_not_ours`・`kb_incomplete`）は G27 の `E2009`（終了コード 1）。

### ライブラリ API（`src/diagram/mod.rs`。G29 が呼ぶ。名前と型を変えない）

```rust
pub enum Kind { Module, Types, Calls, Ownership, Concurrency, Layout, Transitions }
pub enum Format { Mermaid, Dot, Markdown }
pub struct DiagramRequest { pub kind: Kind, pub id: Option<String>, pub depth: usize, pub max_nodes: usize, pub format: Format }  // format は Mermaid か Dot。depth の既定 2、max_nodes の既定 60
pub struct DiagramOutput { pub format: Format, pub text: String, pub nodes: usize, pub folded: bool }
pub struct DiagramError { pub kind: &'static str, pub message: String }   // kind は上の表の 9 個か、E2009 の `kb_stale`・`kb_broken`（呼ぶ側が code を決める）
pub trait Sources { fn read(&self, relative_path: &str) -> Option<String>; }       // `transitions` だけが使う
pub struct DiskSources { pub root: std::path::PathBuf }                             // root.join(path) を cache::read_regular で読む。`..`・絶対パスは None
pub struct Input { /* 付録 C のキーの行 */ }
impl Input { pub fn from_kb(data: &KbData) -> Result<Input, DiagramError>; pub fn from_texts(symbols: &str, graph: &str, facts: &[&str], layouts: &[&str], manifest: &str) -> Result<Input, DiagramError>; }
pub fn render(input: &Input, sources: &dyn Sources, request: &DiagramRequest) -> Result<DiagramOutput, DiagramError>;
pub fn views(input: &Input) -> Vec<(String, String)>;
pub fn validate_mermaid(text: &str) -> Result<(), Vec<String>>;
```

## 設計

### 新規・変更するファイル

| ファイル | 内容 | Phase |
| --- | --- | --- |
| `src/diagram/mod.rs`（新規） | 公開 API（「ライブラリ API」）、`render` の分岐（KIND × 形式の表）、`views`、`DiagramError` | 1〜5 |
| `src/diagram/input.rs`（新規） | `Input`・行の構造体（`SymbolRow`・`EdgeRow`・`FactRow`・`LayoutRow`・`ModuleRow`）、`from_kb`・`from_texts`（`serde_json::Value` から付録 C のキーだけ取る。**G26・G27 の名前の違いはここだけで吸収する**） | 1 |
| `src/diagram/resolve.rs`（新規） | `ID` の解決（種類の検査・候補） | 1 |
| `src/diagram/escape.rs`（新規） | `esc`・`member`・`dot_esc`・`cut` | 1 |
| `src/diagram/graph.rs`（新規） | `Graph`（ノード・辺・番号付け・整列）、`fold`、強連結成分 | 1 |
| `src/diagram/mermaid.rs`・`dot.rs`（新規） | フローチャートの書き手、DOT の書き手 | 1 |
| `src/diagram/validate.rs`（新規） | `validate_mermaid` | 1 |
| `src/diagram/module.rs`・`types.rs`（新規） | `module`・`types`（`types` は classDiagram と DOT の書き手を含む） | 1 |
| `src/diagram/views.rs`（新規） | `views` | 1（`concurrency` は Phase 4） |
| `src/diagram/calls.rs` | `calls` | 2 |
| `src/diagram/ownership.rs` | `ownership` | 3 |
| `src/diagram/concurrency.rs` | `concurrency` | 4 |
| `src/diagram/layout.rs`・`transitions.rs` | `layout`・`transitions`（`Sources`・`DiskSources`） | 5 |
| `src/lib.rs` | `pub mod diagram;`（`diagnostic` の次） | 1 |
| `src/kb/command.rs`（G27） | `diagram` の分岐: 引数の解析、`open`、`Input::from_kb`、`render`、出力 | 1 |
| `src/kb/build.rs`（G27） | `finish` の `brief` の次で `diagram::views` を `Files` に足す（manifest の `files` を作る**前**） | 1 |
| `src/kb/store.rs`（G27） | Phase 5: `KbData` に `layouts: Option<Vec<serde_json::Value>>` を足し、`open` と `from_parts` が `layout/*.jsonl` の全行を（ファイル名順、行の順に）読む。ほかのフィールドに触れない | 5 |

### アルゴリズム（付録 E の Python との対応）

| Rust | Python（`diagram_ref.py`） |
| --- | --- |
| `escape::{esc, member, dot_esc}` | `cut`・`esc`・`member`・`dot_esc` |
| `graph::Graph`（`node`・`edge`・`numbered`） | `Graph` |
| `mermaid::flowchart`・`dot::dot` | `flowchart`・`dot`（`SHAPES`・`ARROWS`・`CLASS_DEFS`・`CLASS_ORDER`・`DOT_FILL`・`DOT_SHAPE`） |
| `graph::fold`・`graph::strongly_connected` | `fold`・`strongly_connected` |
| `module::graph` | `module_graph` |
| `types::mermaid`・`types::dot` | `types_diagram`・`types_dot`・`node_labels` |
| `calls::graph` | `calls_graph`・`effect_class`・`node_shape` |
| `ownership::markdown` | `variable_diagram`・`ownership_markdown` |
| `concurrency::graph` | `concurrency_graph`・`inner_type`・`has_opening` |
| `layout::markdown` | `layout_markdown`・`bound`・`PRIMITIVES`・`WASM_NOTE` |
| `transitions::{extract, mermaid, dot}` | `transitions_diagram`・`transitions_dot`（`extract` は「`transitions`」の手順 1〜6。Python にはない） |
| `views::views` | `views` |
| `validate::validate_mermaid` | `validate_ref.py` の `validate` |

`Graph` の中身（名前と型を変えない）:

```rust
pub struct Node { pub label: String, pub shape: Shape, pub class: Option<&'static str>, pub bold: bool }
pub enum Shape { Box, Round, Stadium, Subroutine, Hexagon }
pub struct Edge { pub src: String, pub dst: String, pub label: Option<String>, pub dashed: bool, pub red: bool, pub weight: usize }
pub struct Graph { pub direction: Direction, pub nodes: BTreeMap<String, Node>, pub edges: Vec<Edge> }   // Direction は TopDown か LeftRight
```

`BTreeMap` の鍵は元の ID（byte 順）で、`n{番号}` は `nodes` を昇順に数えた値。

## 前提とする他チケットのインターフェース

| 相手 | 使うもの | 着手時の確認 |
| --- | --- | --- |
| G26 | `symbols.jsonl`・`graph.jsonl`・`facts/<stem>.jsonl`・`layout/<stem>.jsonl` の行（付録 C の列）。`graph` の `to` は、user の ID、std の `<module>.<name>`（`Channel.send`）、組み込みの名前（`sqrt`）。`facts` の `effects` は D-53 の 14 語、`ownership` は `{var,event,to?,span}` | `grep -n "tz-facts/1" src/kb/*.rs`。行のキーが付録 C と違えば `input.rs` だけを直す |
| G27 | `KbData { meta, symbols, graph, tests, facts, diag }`（`Symbol`・`Edge { rel, from, to, args: Option<Vec<String>>, span }`・`Fact { effects, effects_complete, raw, .. }`・`Meta { edition, modules: Vec<ModuleEntry { name, key, origin, path, source }> }`）、`KbData::open`・`from_parts(meta, &Files)`、`Files`、`kb::open::open(path, kb_dir, refresh, json)`、`Refresh::{Auto, Never}`、`finish`、`E2009` の kind、`<stem>` の規則（`::`→`.`、`facts/<module の key>.jsonl`）。**`KbData` に `layout` がない**ので G30 が `layouts` を足す（手順 18） | `grep -n "pub struct KbData" src/kb/store.rs`、`grep -n "pub fn open\|pub enum Refresh" src/kb/*.rs` |
| G28 | `kb` の引数の解析と ID の解決（あれば）。`--no-refresh` | G28 がなければ本チケットの規則。あれば G28 に合わせる（停止条件 5） |
| G29 | `tz_diagram`: `Input::from_kb(&data)`・`DiskSources { root }`・`DiagramRequest`（`depth` の既定 2、`max_nodes` の既定 60）で `render` を呼び、`structured` を `{kind, format, text, nodes, truncated: folded}` にする。`format` は `markdown` もありうる | G29 の「前提とする他チケットのインターフェース」の `tz_diagram` の行 |
| G22 | `RelatedRole` の snake_case の名前 `moved_here`・`borrowed_here`（台帳の表に**文字列として**書くだけ。型は使わない） | なし |

## 実装手順

各手順の最後で `cargo build --locked`・`cargo test --locked`・`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が緑。

### Phase 1: 基盤・`module`・`types`・views・CLI

**手順 1: ベースライン。** 変更なし。`cargo build --release --locked && cargo test --locked` の件数を控える。`tsuzuri kb build tests/fixtures/diagram/…` はまだない（fixture は手順 2）ので、G27 の `tests/golden/kb/point/` の `manifest.json` の `files` の一覧を控える（手順 9 の比較用）。

**手順 2: テスト先行。** 変更: `tests/fixtures/diagram/kb/{point,cycle,shapes,flow,own,light,layout}/*.jsonl`（付録 C をそのまま。`facts.jsonl`・`layout.jsonl` は全モジュールを 1 ファイルにしたもので、`Input::from_texts` に渡す）、`tests/fixtures/diagram/src/{cycle,shapes,flow,own,light}/`（付録 B の小さなプロジェクトのソース。`tests/` の別のテストが `tsuzuri check` で緑を確かめる）、`tests/golden/diagram/*`（付録 D の期待出力。ファイル名は付録 D の見出し）、`tests/diagram.rs`（「テスト計画」の Phase 1 の群を先に書く。`diagram::*` がないので赤）。確認: `cargo test --test diagram` がコンパイルエラーで赤（`unresolved import`）。

**手順 3: `mod.rs` の骨格と `escape.rs`。** 変更: `src/lib.rs`（`pub mod diagram;` を `pub mod diagnostic;` の次）、`src/diagram/mod.rs`（「ライブラリ API」の型と、中身が `unimplemented` でない最小の `render`＝`diagram_unavailable` を返す）、`src/diagram/escape.rs`。内容: `cut`・`esc`・`member`・`dot_esc` を「共通の規則」3・4 のとおり。テスト: `esc_replaces_the_six_characters`（`a#b"c<d>e&f` と `` `x` `` → `a#35;b#quot;c#lt;d#gt;e#amp;f`・`#96;x#96;`）、`esc_cuts_before_escaping`（61 文字の `#` → 59 個の `#35;` ＋ `…`）、`member_escapes_parentheses`。

**手順 4: `graph.rs`・`mermaid.rs`・`dot.rs`。** 内容: `Graph`、番号付け（鍵の byte 順）、辺の整列、`flowchart`（「Mermaid のフローチャートの行」）と `dot`。`CLASS_DEFS`・`CLASS_ORDER`・`DOT_FILL` は付録 E の定数をそのまま。テスト: `node_numbers_follow_byte_order`、`edges_sort_by_endpoint_numbers_then_label`、`flowchart_prints_only_used_classes`、`dot_quotes_and_backslashes`。

**手順 5: `validate.rs`。** 内容: 「構造検証器」の表どおり（`strip_prefix`・`split`。正規表現を使わない）。テスト: `validator_accepts_every_golden`（`tests/golden/diagram/*.mmd` と、`.md` の ```` ```mermaid ```` ブロックの全部）、`validator_rejects_each_negative_case`（付録 D の負の例 7 つ。**それぞれが付録 D の表の「検証器」の列のとおり拒否される**）、`validator_rejects_sequence_diagram`。

**手順 6: `input.rs`・`resolve.rs`。** 内容: `Input`・`from_texts`（JSON Lines を `serde_json::from_str` で 1 行ずつ。付録 C の必須のキーがなければ `kb_broken`）、`from_kb`（`KbData` の型付きの行を `Input` の行に写す。`ownership` は `Fact.raw["ownership"]`。`layouts` は Phase 5 まで空）、`resolve`（「診断」の表の 3 つのエラー）。テスト: `input_ignores_unknown_keys`、`input_reports_the_missing_key`、`resolve_suggests_by_last_segment`、`input_from_kb_matches_input_from_texts`（`point` の行を `from_parts` で作った `KbData` と `from_texts` の結果が同じ）。

**手順 7: `module.rs`。** 内容: 「`module`」の手順 1〜6、`graph::strongly_connected`（再帰でよいが、モジュール数 1,000 で溢れない深さ。**Tarjan の再帰はスタックを使うので、反復で書く**）、`graph::fold` の `module` 用（専用の規則）。テスト: `module_diagram_of_point_matches_the_golden`・`module_diagram_marks_cycles`（`cycle`）・`module_diagram_dot_matches_the_golden`・`module_diagram_folds_at_max_nodes`（`max_nodes = 5` の合成入力）・`module_using_edges_are_dashed`（`imports` の行を足した合成入力）。

**手順 8: `types.rs`。** 内容: 「`types`」の手順 1〜5、`node_labels`、`types_dot`。テスト: `types_diagram_matches_the_golden`（`shapes`）・`types_diagram_dot_matches_the_golden`・`types_diagram_folds_by_module`（`max_nodes = 5`）・`types_member_limit_is_twelve`（合成）・`types_derive_and_instance_relations`。

**手順 9: `views.rs` と G27 の `finish`。** 内容: `views`（「views」の表）。`src/kb/build.rs` の `finish` で、`brief` を足した後・manifest の `files` を作る前に、`KbData::from_parts(meta, &files)` から `Input::from_kb` を作り、`diagram::views` の各ファイルを `files` に足す（失敗したら足さない）。テスト: `views_for_shapes_and_flow`（`shapes` は `modules`・`types`、`flow` は `modules`・`concurrency`＝Phase 4 まで `concurrency` は作らない）、`kb_build_writes_views_and_verify_passes`（`tests/diagram_cli.rs`。`kb verify` が `ok`）、`kb_build_is_deterministic_with_views`（2 回の `files` が同じ）。確認: 手順 1 の `manifest.json` の `files` と比べて、**増えたのが `views/` の行だけ**。

**手順 10: `kb diagram` の CLI。** 内容: 「CLI」の表どおり（引数の解析は G27 の `kb build` の `seen` の集合の形）、`open`、`Input::from_kb`、`render`、出力。`--json` は `json_string`。テスト（`tests/diagram_cli.rs`。子プロセス）: `kb_diagram_prints_the_diagram`（`examples/point` の `module` が付録 D の `point.module.mmd` と同じ）、`kb_diagram_json_line`、`kb_diagram_argument_errors`（「診断」の表の全メッセージ）、`kb_diagram_unknown_id_suggests`、`kb_diagram_no_refresh_without_a_kb_is_e2009`、`kb_diagram_dot_for_ownership_is_e2000`。

**手順 11: 文書（Phase 1）と測定。** 内容: 「ドキュメント」の Phase 1 の分。測定: `kb build` の時間（手順 1 の前後。`TSUZURI_TIME_PASSES=1` の `kb-views`＝`views` を測る）が、基準の 5% 以下（停止条件 4）。確認: `node scripts/check-docs.mjs <変更したページ>`。

### Phase 2: `calls`

**手順 12: `calls.rs`。** 内容: 「`calls`」の手順 1〜7、`effect_class`、`node_shape`、`graph::fold` の `calls` 用（グループ＝`module`・`std:…`）。テスト: `calls_diagram_of_hypotenuse_matches_the_golden`（`point`）・`calls_diagram_dot_matches_the_golden`・`calls_flow_matches_the_golden`（`flow` の `Main.$entry`、効果のクラスあり）・`calls_folds_by_module`（`max_nodes = 5`）・`calls_depth_marks_truncated_nodes`（`depth = 1`）・`effect_class_follows_the_table`（表の各行の合成）・`calls_merges_edges_and_counts_sites`（`Task.scope` が 2 回 → `ref,copy,copy ×2`）・`calls_needs_a_function_id`。

**手順 13: 文書（Phase 2）。** `diagrams.md` に `calls` の節と例（付録 D）。

### Phase 3: `ownership`

**手順 14: `ownership.rs`。** 内容: 「`ownership`」の手順 1〜5。テスト: `ownership_markdown_matches_the_golden`（`own`）・`ownership_release_precedes_the_next_event`（`borrow` の次が `move`）・`ownership_reborrow_keeps_the_borrow`・`ownership_limits_variables`（13 変数の合成: 先頭 12 と `k variables are not drawn`）・`ownership_without_facts_is_unavailable`・`ownership_ledger_roles`。**この Phase は G26 Phase 5 が main にあるときだけ**（なければ手順を飛ばし、`ownership` は `diagram_unavailable` のまま）。

**手順 15: 文書（Phase 3）。**

### Phase 4: `concurrency`

**手順 16: `concurrency.rs` と views。** 内容: 「`concurrency`」の手順 1〜6。`views` に `concurrency.mmd` を足す。テスト: `concurrency_matches_the_golden`・`concurrency_dot_matches_the_golden`・`concurrency_unique_element_type_attaches_creators`（`Channel<i64>` が 1 種類なら `Channel.bounded` の呼び出しがそこへ。2 種類なら `Channel<?>`）・`concurrency_folds`（`max_nodes = 6`）・`views_include_concurrency`。

**手順 17: 文書（Phase 4）。**

### Phase 5: `layout`・`transitions`

**手順 18: `store.rs` の `layouts` と `layout.rs`。** 変更: `src/kb/store.rs`（`KbData.layouts`。`open` と `from_parts` が `layout/*.jsonl` を読む。G27 のテスト `store` 系が緑のまま）、`src/diagram/input.rs`（`from_kb` が `layouts` を写す）、`src/diagram/layout.rs`。内容: 「`layout`」。テスト: `layout_tables_match_the_goldens`（`Packed`・`Shape`）・`layout_offsets_stop_at_the_first_unknown_type`・`layout_reports_unknown_id`・`kb_data_reads_layout_rows`（`tests/diagram_cli.rs`。`layout/*.jsonl` を持つ KB を `open` した `layouts` の行数）。G26 Phase 6 がなければ `layouts` は `None` のままで、`layout` は `diagram_unavailable`。

**手順 19: `transitions.rs`。** 内容: 「`transitions`」の手順 1〜7、`Sources`・`DiskSources`。テスト（合成の `Sources`＝`HashMap<String, String>`）: `transitions_match_the_golden`（`light`。`next` と `reset`）・`transitions_wildcard_means_the_remaining_cases`・`transitions_reject_a_changed_source`（SHA-256 違い → `kb_stale`）・`transitions_without_candidates_is_unavailable`・`transitions_ignore_guards`。**構文木の形は `tests/` の別のテスト `tests/diagram_syntax.rs`（`parse_with_source_all` で `light/Light.tz` を読み、`FunctionDecl.body` の形が「`transitions`」の手順 3〜5 の前提どおり）が先に確かめる**。

**手順 20: 文書と最終の測定（Phase 5）。** 全ページ・`index.md`・`README.md`・`docs/architecture.md`・`docs/stability.md`。`kb build` の時間を再測定（停止条件 4）。付録 F の harness で、全期待出力と、実装が出した全出力（`kb diagram` を fixture に実行して保存したもの）を検査する。

## テスト計画

期待値の出どころ: **付録 D の期待出力**（参照実装の出力。手で作った想定出力で、`mermaid` 12.1.0 と Graphviz 14.1.2 で構文を確かめた）と、付録 B の fixture のソースが `tsuzuri check` を通ること。実装の出力を期待値に写さない。

**`tests/diagram.rs`（ライブラリ。`Input::from_texts` に fixture の JSON Lines を渡す）**

| Phase | テスト |
| --- | --- |
| 1 | `esc_replaces_the_six_characters`・`esc_cuts_before_escaping`・`member_escapes_parentheses`・`node_numbers_follow_byte_order`・`edges_sort_by_endpoint_numbers_then_label`・`flowchart_prints_only_used_classes`・`dot_quotes_and_backslashes`・`validator_accepts_every_golden`・`validator_rejects_each_negative_case`・`validator_rejects_sequence_diagram`・`input_ignores_unknown_keys`・`input_reports_the_missing_key`・`input_from_kb_matches_input_from_texts`・`flowchart_truncates_edges`・`resolve_suggests_by_last_segment`・`module_diagram_of_point_matches_the_golden`・`module_diagram_marks_cycles`・`module_diagram_dot_matches_the_golden`・`module_diagram_folds_at_max_nodes`・`module_using_edges_are_dashed`・`types_diagram_matches_the_golden`・`types_diagram_dot_matches_the_golden`・`types_diagram_folds_by_module`・`types_member_limit_is_twelve`・`types_derive_and_instance_relations`・`views_for_shapes_and_flow`・`render_twice_gives_the_same_bytes`・`too_large_text_is_reported`・`every_golden_has_one_trailing_newline` |
| 2 | `calls_diagram_of_hypotenuse_matches_the_golden`・`calls_diagram_dot_matches_the_golden`・`calls_flow_matches_the_golden`・`calls_folds_by_module`・`calls_depth_marks_truncated_nodes`・`effect_class_follows_the_table`・`calls_merges_edges_and_counts_sites`・`calls_needs_a_function_id` |
| 3 | `ownership_markdown_matches_the_golden`・`ownership_release_precedes_the_next_event`・`ownership_reborrow_keeps_the_borrow`・`ownership_limits_variables`・`ownership_without_facts_is_unavailable`・`ownership_ledger_roles` |
| 4 | `concurrency_matches_the_golden`・`concurrency_dot_matches_the_golden`・`concurrency_unique_element_type_attaches_creators`・`concurrency_folds`・`views_include_concurrency` |
| 5 | `layout_tables_match_the_goldens`・`layout_offsets_stop_at_the_first_unknown_type`・`layout_reports_unknown_id`・`transitions_match_the_golden`・`transitions_dot_matches_the_golden`・`transitions_wildcard_means_the_remaining_cases`・`transitions_reject_a_changed_source`・`transitions_without_candidates_is_unavailable`・`transitions_ignore_guards` |

**`tests/diagram_cli.rs`（子プロセス。`Command::new(env!("CARGO_BIN_EXE_tsuzuri"))`。手本 `tests/api_diff.rs`）**: `kb_diagram_prints_the_diagram`・`kb_diagram_json_line`・`kb_diagram_argument_errors`（「診断」の表の全メッセージと終了コード 2）・`kb_diagram_unknown_id_suggests`・`kb_diagram_no_refresh_without_a_kb_is_e2009`・`kb_diagram_dot_for_ownership_is_e2000`・`kb_build_writes_views_and_verify_passes`・`kb_build_is_deterministic_with_views`・`golden_diagrams_parse_with_mmdc_when_available`（`mmdc` が PATH にあるときだけ）。**`tests/diagram_syntax.rs`**: `light_function_bodies_have_the_expected_shape`（`transitions` の前提の構文木の形）。

**外部の検証（付録 F）**: 期待出力の全 `.mmd` と `.md` の Mermaid ブロックを `mermaid` 12.1.0 の `parse` に通し、全 `.dot` を `dot -Tplain` に通す。**この検証は CI に入れない**（Node の依存と Graphviz を CI に足さない）。実装者が手元で実行し、結果（`failures=0`）を完了報告に書く。

**既存テストへの影響: なし。** `src/diagram/` は新しいモジュールで、`src/lib.rs` は 1 行、`src/kb/` は分岐と 1 呼び出しを足すだけ。`HELP` のテストは `contains` で見るだけ。**資源の上限のテスト**: `too_large_text_is_reported`（長いラベルの合成入力）、辺 400 本の切り詰め（`flowchart_truncates_edges`）。**決定性**: `render_twice_gives_the_same_bytes`。**native と WASM の差**: なし（コード生成に触れない）。**性能**: 速さのしきい値のテストは足さない（共有の CI で不安定）。手順 11・20 の測定を完了報告に書く。

## ドキュメント

更新するページ（手順 11・13・15・17・20）:

| ページ | 内容 |
| --- | --- |
| `_tsuzuri/language-reference/compiler/diagrams.md`（新規） | 7 種の図の目的と読み方、`kb diagram` の引数と形式の表、各図の例（実装後に実際に出した出力。` ```mermaid ` のブロック）、Mermaid の制約とエスケープ、DOT、`views/*.mmd`、上限（`--max-nodes`・辺 400・45,000 文字）、近似である図（`concurrency`・`transitions`）の限界、`layout` が native だけであること、診断（`E2000` の kind） |
| `_tsuzuri/language-reference/compiler/kb.md`（G27・G28 が作ったもの） | `kb diagram` への案内と、KB の `views/` の説明 |
| `_tsuzuri/language-reference/compiler/usage.md` | `kb` の節に `kb diagram` |
| `_tsuzuri/language-reference/compiler/option.md` | 「どのサブコマンドが受け付けるか」に `kb diagram` の `--format`・`--max-nodes`・`--depth`・`--path`・`--json`・`--no-refresh` |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | `E2000` の kind 9 個 |
| `_tsuzuri/language-reference/index.md` | コンパイラの節に `[図表の生成](./compiler/diagrams.md)` |
| `docs/language.md` | `kb` のサブコマンドの列挙に `diagram` |
| `README.md` | CLI の一覧に `tsuzuri kb diagram KIND [ID] …`、開発・AI 支援の説明に図表 |
| `docs/architecture.md` | ファイルの表に `src/diagram/*.rs`、KB の段落に `views/` |
| `docs/stability.md` | 「保証しないもの」に「図の本文（Mermaid・DOT・Markdown の字句・色・順序の細部）」、「保証するもの」に「`--format` の値・`kind` の名前・`DiagramOutput` のキー・views のファイル名」 |

検索: `grep -rnE "kb diagram|views/|\.mmd|mermaid" README.md docs _tsuzuri/language-reference | head -40`。検証: `node scripts/check-docs.mjs <変更したページ>`（`mermaid` のブロックは検査されない。付録 F の harness で別に確かめる）。**`gpu.md` の `call` の誤りは直さない**（完了報告に書く）。

## 受け入れ条件

### Phase 1

- [ ] `tsuzuri kb diagram module`・`types` が付録 D の期待出力と 1 byte も違わない。`--format dot` も同じ。
- [ ] 全期待出力が、`validate_mermaid`（`tests/diagram.rs`）と、付録 F の harness（`mermaid` 12.1.0 の `parse`・`failures=0`）と、`dot -Tplain` を通る。
- [ ] `kb build` が `views/modules.mmd`・`views/types.mmd` を作り、`kb verify` が `ok`。`files` の増加は `views/` の行だけ。2 回の `kb build` が byte 単位で同じ。
- [ ] 「診断」の表の全メッセージが出る。`kb build` の時間が基準の 5% 以下。文書（手順 11）。

### Phase 2〜5

- [ ] 各 Phase の KIND（`calls`・`ownership`・`concurrency`・`layout`・`transitions`）が、付録 D の期待出力と 1 byte も違わず、同じ 3 つの検証を通る。G26 の対応する Phase がないものは `diagram_unavailable`。
- [ ] `views/concurrency.mmd`（Phase 4）。`DiagramOutput` を G29 の `tz_diagram` が使える（G29 の `tool-examples.json` の `tz_diagram` の例が、実際の出力と同じ形）。
- [ ] 全ページが `check-docs.mjs` を通る。付録 F の harness の結果を完了報告に書く。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| 元の名前をノードの ID にする（`end`・`call`・`o…`・`x…`・`::`・`.`・`$`） | `validator_rejects_each_negative_case`。ID は `n<数>`・`s<数>` だけ |
| `esc` をエスケープした後に切る（`#35;` が途中で切れる） | `esc_cuts_before_escaping` |
| `<`・`>` を素通しにする（`Array<Point>` の `<Point>` が HTML タグとして消える） | 付録 D の負の例 `raw-angle`、`types_diagram_matches_the_golden`（`Traits.Sized#lt;Point#gt;`） |
| フィールドの型の `(`・`)` を裸で書く（メソッド扱いになる） | `member_escapes_parentheses` |
| stateDiagram の複合状態の中で `classDef` を使う（効かない） | 所有権は変数ごとの図（D6）。`ownership_markdown_matches_the_golden` |
| 強連結成分を再帰で書き、モジュールが多いとスタックが溢れる | 手順 7（反復で書く）。1,000 モジュールの合成入力 |
| `HashMap` の反復順で整列する | `render_twice_gives_the_same_bytes`。`BTreeMap`・`sort` だけ |
| views を manifest の `files` を作った後に足す（`kb verify` が `kb_broken`） | `kb_build_writes_views_and_verify_passes` |
| `transitions` で KB の古い位置を信じ、ソースの SHA-256 を検査しない | `transitions_reject_a_changed_source` |
| 出力の末尾の改行が 0 個・2 個 | `every_golden_has_one_trailing_newline` |
| stdout に通知を混ぜる | `kb_diagram_prints_the_diagram`（stdout が図と完全に一致） |
| テストが固定の `/tmp` に依存する | 一時ディレクトリは `std::env::temp_dir()` に一意の名前で作る（`tests/api_diff.rs`） |

## 対象外

`sequenceDiagram` の生成と検証（D11）、SVG・PNG の描画、`mmdc`・`dot` の実行を必須にすること、wasm32 のレイアウトの数値（D7）、データフローに基づく並行の図（D8）、失敗するプログラムの図（D6）、`alias`・`extern type`・`const` を `types` に出すこと、`types` を class・record の ID で絞ること、図の色・向きの設定、G28 の照会コマンド、`tsuzuri doc` との統合。

## 決定事項

すべて承認済み（D-50）。

- **D1: Mermaid が主で、DOT は 5 種（`module`・`types`・`calls`・`concurrency`・`transitions`）、Markdown は `ownership`・`layout`。** 理由: Mermaid は GitHub・VS Code がそのまま描く。所有権は変数ごとの図と台帳の表を 1 つの文書にしたく、レイアウトは表が本体。却下: 全部を Mermaid（表を図にする `block`・`packet` は版で揺れる）、SVG（描画の依存）。
- **D2: ノードの ID は `n<連番>`・`s<連番>`、ラベルは必ず `"…"` で囲んでエスケープ。** 理由: `end`・`call` の予約語（`gpu.md` の実例）と、ID に使えない文字を避ける。却下: 名前を ID に使い、危ないものだけ変える（検査が漏れる）、バックスラッシュのエスケープ（Mermaid にない）。
- **D3: フローチャート系の 4 種は `Graph` の 1 つのモデルから、Mermaid と DOT の 2 つの書き手で出す。** 理由: 規則を 2 度書かない。classDiagram・stateDiagram は構造が違うので専用の書き手。
- **D4: `transitions` だけはソースの構文木を読む。** 理由: KB に `match` の腕がない。KB に新しいファイルを足すと G26 の行の形（D-52）を変える。ソースの SHA-256 を `manifest.json` の `source` と照合して、古い KB の位置を信じない。却下: G26 に `transitions` の行を足す（G26 の変更）、型からの推論（情報が足りない）。
- **D5: 畳み込みはモジュール単位。** `--max-nodes` は 5〜200、既定 60。辺は 400、テキストは 45,000 文字まで。理由: Mermaid の既定（辺 500・テキスト 50,000）の内側に収め、Mermaid の `maxEdges`・`maxTextSize` で描画が失敗するのを防ぐ。
- **D6: `ownership` は変数ごとの図と台帳の表。** 「`E1012`・`E1014` の `related` の位置の強調」は、**KB が成功した解析からしか作られない**（G26 D15）ので、失敗するプログラムは図にできない。代わりに、診断が引用する出来事（`move`＝`moved_here`、`borrow`・`borrow_mut`＝`borrowed_here`）を、状態の色（`moved`・`borrowed`）と台帳の `diagnostic role` の列で示す。却下: 診断の JSON から図を作る（G27 の不完全な KB を読む経路が要り、G22 に依存する）、複合状態（`classDef` が効かない）。
- **D7: `layout` は native だけ。** 理由: G26 の `layout` の行が native（64 bit）の下限・上限だけで、wasm32 の値を持たない。wasm32 の列は、G26 が行に足してから別の変更で足す（表の下に注意書き）。オフセットは、幅の分かる基本型（`i8`〜`f64`・`bool`）だけで、分からない型があれば以降は `?`（推測しない）。
- **D8: `concurrency` は近似。** 理由: 値の流れは KB にない。署名の型（`Channel.Sender<T>`・`Channel.Receiver<T>`・`Mutex<T>`・`Atomic<T>`）と呼び出し（`Channel.bounded`・`Task.scope`・`Mutex.create`）から決定的に作り、注記のノードで近似であることを図に書く。1 種類の `T` しかなければ生成の呼び出しをそこへ結びつけ、2 種類以上なら `?`。
- **D9: 構造検証器は、G30 が作る形だけを受理する厳しい検査。** 理由: 外部ツールなしで全生成物を検査できる。実際の Mermaid での確認は付録 F の harness（手元）と、あれば `mmdc`（テスト）。却下: 一般の Mermaid の parser を書く（規模が違う）、CI に Node を足す。
- **D10: views は 3 つ（`modules`・`types`・`concurrency`）で、KB を組み立てるたびに作り直し、`manifest.json` の `files` に入る。** 理由: ID の要らない図だけが KB の一部になる。`kb verify` が見られる。views の失敗は KB の失敗にしない。
- **D11: `sequenceDiagram` は作らない。** どの図も使わず、使わない構文の検証器は書かない。足すときは `validate` に 1 つの形の群を足す。
- **D12: 上限は 45,000 文字・辺 400・ノード 200。** D5 のとおり、Mermaid の既定より下。
- **D13: CLI の形。** `kb diagram KIND [ID]` と `--path`・`--no-refresh`（G27・G28 と同じ意味）。`ID` が 2 つ目の位置引数で、`PATH` は `--path`（位置引数の曖昧さを避ける）。G28 が別の形に決めたら G28 に合わせる。
- **D14: `Sources` トレイト。** `render` はファイルを直接読まない。MCP（読み取り専用）とテスト（合成）が同じ関数を使える。`DiskSources` は `..`・絶対パス・シンボリックリンクでないものだけを読む。
- **D15: 参照実装（Python）が正本。** 期待出力はその出力で、手で作った想定出力。実装が出す出力と違えば、まず参照実装と本文の食い違いを疑い、本文を直す。

**D-55 に足す名前**（ledger）: `kb diagram` のオプション `--format`・`--max-nodes`・`--depth`・`--path`、`E2000` の kind 9 個（`diagram_usage`・`diagram_unknown_kind`・`diagram_needs_id`・`diagram_format_unsupported`・`diagram_bad_option`・`diagram_unknown_id`・`diagram_wrong_id_kind`・`diagram_unavailable`・`diagram_too_large`）。

## 付録

| 付録 | 内容 | ファイル |
| --- | --- | --- |
| A | Mermaid の制約（公式の文書の引用、`mermaid` 12.1.0 での実験、`config.md` の上限） | [G30-appendix.md](G30-appendix.md) |
| B | fixture のソース 5 つ（`cycle`・`shapes`・`flow`・`own`・`light`）と `tsuzuri check`・`run` の結果 | 同上 |
| C | fixture の KB の行（JSON Lines）と、G30 が読むキーの表 | 同上 |
| D | 期待出力（Mermaid・DOT・Markdown）と、負の例 | 同上 |
| E | 参照実装（`diagram_ref.py`・`validate_ref.py`・`fixtures.py`・`gen.py`） | [G30-appendix-reference.md](G30-appendix-reference.md) |
| F | 検証の手順（Node の harness・Graphviz）と測定 | 同上 |

