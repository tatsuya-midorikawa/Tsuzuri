# E15: 信頼境界と権限 — `@trusted`・`tsuzuri audit`・`[capabilities]`

| 項目 | 内容 |
| --- | --- |
| ID | E15 |
| 優先度 | P2（D-50 の表） |
| 規模 | 全体 L。Phase 1: M（3〜5 日）、Phase 2: L（1〜2 週）、Phase 3: M（3〜5 日）、Phase 4: S（1〜2 日。A20 が済んでから） |
| 依存 | Phase 1〜3 は**自己完結**（HEAD のコードだけで書ける）。[A20](A20-effect-summary-and-checked-attributes.md) は Phase 4 だけ（`@trusted` の性質を `pure`・`@no_*` の検査に効かせる）。[G26](G26-facts-model.md) は使わない（D-52 の `effects` の `extern` の名前だけ共有）。[G22](G22-diagnostic-v2.md) は弱い依存（未実装なら kind・related・help を出さず、コード・メッセージ・位置だけ。付録 C） |
| 後続 | なし。A20・A19・G26 は本チケットの `FunctionOrigin.trusted` を読める |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの `9dd0fbbc` との差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | Rust 比（`unsafe` と `cargo-audit`・`cargo-geiger`、依存の権限を持たない点）と Deno 比（権限フラグ）: Tsuzuri は `extern`（C の呼び出し）を人が確かめたかを書く場所がなく、依存パッケージがファイル・ネットワーク・環境・時計に触れるかをコンパイラが報告も制限もしない。D-50 の調査結論(1)（コンパイラが知っている事実を診断にもツールにも出さない）に当たる |
| 手本にする既存実装 | 属性: `src/parser.rs` の `cpu_attribute`・`program_all`・`extern_declaration`、`src/syntax.rs` の `CpuAttribute`、`cpu_attributes` の配線（`src/check.rs` の `cpu_levels`、`src/syntax_codec.rs`、`src/formatter.rs`、`src/api_diff.rs`、`src/frontend_cache.rs` の `FRONTEND_FORMAT`）。マニフェスト: `src/package.rs` の `parse_manifest`・`Line::string_array`・`path_dependency`・`git_dependency`・`registry_dependency`、`src/driver.rs` の `load_packages`・`LoadedPackage`・`Project::dependency_package`・`inputs`。CLI: `src/main.rs` の `api_diff_command`・`--warn implicit-copy`、`src/api_diff.rs` の `render_text`・`render_json`。パッケージを知る検査: `src/warnings.rs` の W1005（`ModuleInput.package`）。私的な組み込み関数: `src/polymorph.rs` の `Checker::builtin`（`E1022`）。`extern`: `src/check.rs` の `external_functions`・`TypedExprKind::HostCall`・`HostImport` |
| 主な影響ファイル | Phase 1: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/formatter.rs`・`src/api_diff.rs`・`src/frontend_cache.rs`・`src/check.rs`・`src/capabilities.rs`（新規）・`src/driver.rs`・`src/main.rs`・`src/bindgen.rs`、`tests/trusted.rs`（新規）。Phase 2: `src/package.rs`・`src/driver.rs`・`src/capabilities.rs`・`src/main.rs`、`tests/capabilities.rs`（新規）、`tests/fixtures/capabilities/`（新規）。Phase 3: `src/capabilities.rs`・`src/main.rs`、`tests/audit.rs`（新規）。文書は「ドキュメント」の表 |

## 実装者向け早見

- **最初に読むもの**（この順）: `src/parser.rs` の `cpu_attribute` と `program_all` の `extern` の分岐 → `src/package.rs` の `parse_manifest`（全体。約 240 行）→ `src/driver.rs` の `load_packages` と `Project::load_from_root`・`inputs` → `src/main.rs` の `api_diff_command` と `--warn` の解析 → 本チケットの「仕様」「設計」と [付録](E15-appendix.md)（A: std の決定表、B: マニフェスト、C: 診断、D: 出力例、E: Rust の骨格、F: 影響、G: テスト、H: 実測）。
- **着手前の基準**: `cargo build --release --locked && cargo test --locked`。基準の IR を手順 1 で保存する（Phase ごとに、`@trusted`・`[capabilities]` を使わないプロジェクトの出力が 1 byte も変わらないことを比べる）。
- **最初に書くテスト**: Phase 1 は `tests/trusted.rs::trusted_marks_extern_functions_and_types`。Phase 2 は `tests/capabilities.rs::std_probe_matches_the_expected_sites`。Phase 3 は `tests/audit.rs::audit_text_matches_the_examples`。
- **触ってはいけないもの**: `check_modules`・`SourceInput`・`ModuleInput` の形と型検査の結果（権限の検査は**型検査のあとの別の段**。決定事項 D2）、`@cpu` の意味、`E1022` の規則、`tsuzuri bindgen` の `MARKER`（1 行目）、`Tsuzuri.lock` の形（権限はマニフェストの宣言で、ロックに入れない）、IR・ABI・WASM の import（本チケットは生成コードを 1 byte も変えない）。
- **Phase の完了の見分け方**: Phase 1 = `@trusted pure extern "abs" def …` が受理され、`--warn untrusted-extern` が付録 C.3 の `W1010` を出し、5 つの bindgen の期待ファイルが新しい注意書きで通る。Phase 2 = 付録 A.7 のプローブが 14 地点・9 権限で期待どおり、付録 D.4 が `E1036` で終了コード 1、マニフェストに権限の記述がないプロジェクトの出力が HEAD と同じ。Phase 3 = 付録 D.1〜D.5 の text と JSON が一致する。
- **やってはいけない近道**: 権限を `os` 効果（A20）や `Path`・`Net` のモジュール名で決める（`Net.parse_address` は純粋）。ポリシーのないプロジェクトで `E1036` を出す。`@trusted` に検査を足す（印であって、ABI や挙動を確かめるものではない）。絶対パスや日時を `audit` の出力に入れる。

## 目的

Tsuzuri の外の世界への入口は 2 つある。(1) std の `File`・`Dir`・`Env`・`Time`・`Random`・`Process`・`Net`・`Gpu`・`IO`・`Debug`（実体は私的な組み込み関数。利用者は `E1022` で直接呼べない）、(2) `extern`（C の関数。コンパイラは署名しか知らない）。利用者は依存パッケージを追加するとき、そのパッケージが何に触れうるかを、ソースを読まない限り知らず、制限もできない。`extern` を人が確かめたかを書く場所もない。

HEAD で測った数字（付録 H）:

- std は 47 ファイル、`def` は 949 個（公開 559・非公開 390）で、`extern` は 0。世界に触れる私的な組み込み関数への参照は 12 ファイルの 61 か所だけで、権限を持つ公開関数は 85 個（付録 A）。
- `examples/` の 13 プロジェクトに `Tsuzuri.toml` も `extern` もない。マニフェストに `[capabilities]` を書くと `E0002`、`@trusted` を書くと `E0002`、`tsuzuri audit` は未知のサブコマンドで `E2000`。
- 依存パッケージの検査は W1005（非推奨の使用）の 1 つだけ。

本チケットが足すもの:

1. **Phase 1（M）`@trusted`**: `extern` の宣言に付ける**印**。意味は「人がこの外部関数の ABI・所有権・寿命の契約を確かめた」。コンパイラは確かめない。性質 `pure`・`no_alloc`・`no_copy`・`no_trap`・`terminates` を添えられる（A20 の語彙。Phase 4 で検査に効く）。誤りは `E1037`。印のない `extern` の使用は `W1010`（既定は無効。`--warn untrusted-extern` で有効）。`tsuzuri bindgen` の出力は `@trusted` を付けず、注意書きを 1 行足す。
2. **Phase 2（L）`[capabilities]`**: 権限 9 個（`env ffi fs gpu net process random stdio time`。D-57）。マニフェストでルートの `allow` と依存ごとの `capabilities` を宣言すると、`check`・`build`・`run`・`test`・`bench` がコンパイル後の**静的な使用**を調べ、許可の外なら `E1036`。宣言のないプロジェクトは何も変わらない。実行時のサンドボックスではない。
3. **Phase 3（M）`tsuzuri audit [PATH] [--json] [--deny-warnings]`**: パッケージごとの使用・許可・未使用・`extern` の信頼を、決定的なテキストと JSON Lines で出す。

非目標: 実行時の権限の強制（サンドボックス）、LSP での検査（LSP は検査しない。文書に書く）、`unsafe`・新しい予約語・新しい crate・生成コードの変更、`Tsuzuri.lock` への権限の記録、G26 の facts への権限の追加、`extern type` の `@trusted` の報告（印として受理するだけ）。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準（`cargo build --release --locked`・`cargo test --locked`）が成功する状態から始める。
- Phase の順に実装し、各 Phase は単独でマージできる。指示がなければ Phase 1 だけ。Phase 4 は A20 が `done` になってから（`grep -nE "^\| A20 " _features/README.md` で確かめる）。
- 読むもの: GUIDE の D-51（診断 v2）・D-53（効果の語彙）・D-55（名前の割り当て）・D-57（権限）、`docs/stability.md`、AGENTS.md。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

1. **ポリシーのないプロジェクトの出力が変わった**: `[capabilities]`・`capabilities`・`@trusted` を使わない `check`・`build`・`run`・`test` の IR・診断・終了コードが、手順 1 の基準と 1 byte でも違う。
2. **`E1036` が、マニフェストに権限の記述がない既存のプロジェクトで出た**（`tests/` の依存つきの fixture を含む）。有効化の規則（仕様 2）を見直す。上限を緩めて通さない。
3. **付録 A.6 の固定表が HEAD で成り立たない**（std の世界への参照が表と違う）。std が変わっている。表を直す前に報告する。
4. **`Os.__read`・`Os.__write` が `File`・`Dir`・`Env` 以外の std から参照されている**。A.2 の表に行を足す前に報告する。
5. **生成関数や特殊化された複本が、`origin.parent` の連鎖でも `span.source` でも属するソースに着かない**（std・パッケージの判定ができない）。選択肢: A（その関数を黙って飛ばす。手順 13 の仕様）、B（止めて報告する。飛ばした関数が権限を持つなら穴になる）。
6. **解析が `E1017` の上限（関数 65,536・式のノード 16,777,216）に、`examples/` や `tests/fixtures/` の実在のプロジェクトで当たった**。上限を上げず、報告する（選択肢: std の関数を参照されたものだけ遅延して走査する）。
7. **`SourceInput`・`ModuleInput`・`check_modules` への field の追加が必要に見える**。決定事項 D2 に反する。止めて報告する。
8. **bindgen の期待ファイル 5 つと `native-interop.md` 以外の既存テスト・文書の変更が 5 件を超えて必要になった**（付録 F のほかに）。
9. A20 が未実装なのに Phase 4 が必要に見える。Phase 4 は A20 の後。
10. 新しい予約語、`unsafe`、新しい crate、既定の WASM の import、新しい診断コード（`E1036`・`E1037`・`W1010` 以外）が必要に見える。
11. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

実機の出力は付録 H。要点:

- **マニフェスト**（`src/package.rs` の `parse_manifest`）: 行単位の手書きの解析。`[package]` を最初に、`[dependencies]`・`[wasm]`・`[native]`・`[registry]` を各 1 回。依存は 1 行のインラインテーブルで、`path`（`native = bool` を続けられる）・`git`+`rev`・`version` のどれか。`[capabilities]` と依存の `capabilities` は `E0002` で拒否される。
- **属性**: `@cpu` だけが宣言の属性で、`Parser::cpu_attribute` が `program_all` の先頭で読み、`Program.cpu_attributes: Vec<CpuAttribute>` に名前で記録し、`check.rs` が `cpu_levels` で関数の宣言に結び付け、`FunctionOrigin.cpu: u8` に持つ。`@trusted` は `E0002`（`'@checked' is the only attribute of an expression`）。
- **`extern`**: `extern "symbol" def name :: sig`・`extern def …`・`extern type Name`。`Program.externs`（`Signature`）と `Program.extern_types`。型検査は `external_functions` の関数を、本体が `TypedExprKind::HostCall(HostImport, …)` の `CheckedFunction` にする。std に `extern` はない。
- **世界への入口**: 私的な組み込み関数 46 個（付録 A.2）。`Checker::builtin`（`src/polymorph.rs`）の `E1022` が、`IO`・`Os`（`File`・`Dir`・`Env`・`Time`・`Random`・`Process`・`Os`）・`Net`・`Debug`・`Gen`・`Gpu`・`Async`・`Arena`・`Channel`・`Unicode` の私的な組み込み関数を、その std のモジュール以外から呼ぶことを拒否する。したがって利用者のコードが世界に触れる道は、std の関数の呼び出しと `extern` の呼び出しだけ。
- **パッケージ**: `ModuleInput.package`・`SourceInput.package`（`Option<&PackageId>`。ルートと std は `None`）。`Project::dependency_package` が同じ判定。W1005 が使う先例。`Span::source` は `Project::sources` の添字で、`SourceInput`・`ModuleInput` の列の添字と同じ。
- **CLI**: `--warn` は `implicit-copy` だけ。サブコマンドの先例は `api-diff`（`api_diff_command`。`parse_arguments` を通らない）。

## 変えてはいけない意味

| # | 不変にするもの | 確かめ方 |
| --- | --- | --- |
| 1 | マニフェストに `[capabilities]`・`capabilities` がなく、`@trusted` も使わないプロジェクトの、`check`・`build`・`run`・`test` の出力（IR・object・`.wasm`・診断・終了コード） | 手順 1 の基準（28 個の `.ll`）とのバイト比較、`tests/capabilities.rs::policy_off_costs_nothing_and_changes_nothing` |
| 2 | 型検査・特殊化・所有権検査の結果。`check_modules`・`SourceInput`・`ModuleInput` の形 | `cargo test --locked` が（付録 F の更新を除き）通る |
| 3 | `@cpu`・`extern` の構文と意味、`E1022`、`Tsuzuri.lock`、`tsuzuri bindgen` の `MARKER` | 既存の `tests/multiversion.rs`・`tests/packages.rs`・`tests/bindgen.mjs` |
| 4 | 診断の JSON の既存のキー（`docs/stability.md`）。新しい診断は既存の 5 キーの形で出せる | `tests/diagnostics.rs` |
| 5 | 生成 IR は決定的。WASM の import は既定で 0。本チケットは生成コードを変えない | IR を 2 回出して同じ。`WebAssembly.Module.imports` が `[]` |

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `@trusted`・`E1037`・`W1010`・`--warn untrusted-extern`・bindgen の注意書き。文書 | M | なし | 印と任意の警告だけで、既存のプログラムの意味・出力は変わらない |
| 2 | `[capabilities]`・依存の `capabilities`・実効の権限・使用の解析・`E1036`・std の決定表と固定テスト。文書 | L | Phase 1（`extern` の解析を共有） | ポリシーを宣言したプロジェクトだけが検査される。宣言がなければ何も変わらない |
| 3 | `tsuzuri audit`（text・JSON・終了コード）。文書 | M | Phase 2 | 新しいサブコマンドを足すだけ。既存の出力を変えない |
| 4 | `@trusted` の性質を A19 の `pure`・A20 の `@no_*` の検査に効かせる。audit の JSON に効果の件数を足す | S | A20 done（A19 の `pure` も） | A20 がなければ Phase 1〜3 のまま動く。Phase 4 はキーと緩和を足すだけ |

## 仕様

### 1. `@trusted`（Phase 1）

```text
trusted      = "@" "trusted" { property }           同じ行の識別子。順序は自由、重複は不可
property     = "pure" | "no_alloc" | "no_copy" | "no_trap" | "terminates"
declaration  = [ doc ] [ trusted ] [ visibility ] "extern" … 
```

- 置き場は `extern` の宣言（`extern "sym" def`・`extern "mod" "sym" def`・`extern def`・`extern type`）の**直前**。`@cpu` と同じ位置で、`program_all` の共通の属性の読み込み口が読む（D-53。A20 が先なら `attributes()` に `@trusted` を足し、E15 が先なら `cpu_attribute` の次に `trusted_attribute()` を置いて A20 が取り込む）。
- 意味は**印**: 検査も生成コードの変更もない。`FunctionOrigin.trusted: Option<u8>` に持つ（`None` = 印なし、`Some(bits)` = 印あり。ビット 0〜4 が上の性質の順）。性質は Phase 4 まで使われない。
- `extern type` に性質は付けられない（`@trusted` だけ）。型の印は受理するだけで、`audit` に出さない。
- 誤り（`E1037`。構文の段階。付録 C.2）: `extern` でない宣言への `@trusted`（`trusted_wrong_target`）、未知の性質、重複する性質、`extern type` の性質、1 つの宣言に `@trusted` が 2 つ。
- `tsuzuri fmt` は `@trusted` を保つ。`api-diff` は `@trusted` を API として扱わない。
- **`tsuzuri bindgen`**: 出力に `@trusted` を付けない。`// target:` の行の次に 1 行 `// Review each extern declaration and add '@trusted' once you have checked its contract; bindgen never adds it.` を足す（`// options:` の行があればその前）。`MARKER`（1 行目）は変えない。

**`W1010`**（付録 C.3）: `@trusted` のない `extern` の関数が、`extern` でない関数から参照されているとき、その宣言の名前に出す。`--warn untrusted-extern` はルートパッケージで宣言した `extern` だけ、`tsuzuri audit` は全パッケージの `extern` を常に出す。`--deny-warnings` は既存の規則で警告を失敗にする。

### 2. `[capabilities]`（Phase 2）

権限は 9 個で閉じている: `env`・`ffi`・`fs`・`gpu`・`net`・`process`・`random`・`stdio`・`time`（D-57。この辞書順がビットの順）。文法・エラー・例は付録 B。

- **ルート**: `[capabilities]` の `allow = [...]`。ルートのパッケージが使ってよい権限。**セクションがなければ全権限**。`allow = []` は何も使えない。
- **依存**: `[dependencies]` の項目の `capabilities = [...]`（path・git・registry のすべて）。**書かなければ空**。
- **ポリシーの有効化**: ルートのマニフェストに `[capabilities]` がある、またはルートの依存の項目のどれかに `capabilities` がある、とき**だけ**ポリシーが有効（`enforced`）。有効でなければ解析も検査も走らず、既存のプロジェクトは何も変わらない（決定事項 D1）。深い階層のマニフェストの `[capabilities]`・`capabilities` は、有効化には使われないが、有効なとき辺の許可として使われる（付録 E.3）。非ルートの `[capabilities]` は無視する。
- **実効の権限**: ルート = `allow`。依存 `C` = すべての入辺 `P → C` について `実効(P) ∩ 項目の capabilities` の**積**（`C` を使うどの親も許した権限だけ。決定事項 D5）。計算は付録 E.3。
- **何を検査するか**: コンパイルしたプログラムの各パッケージの関数の本体が**直接**参照する、権限を持つ std の関数と `extern`（付録 A の規則）。パッケージの境界は `Span::source` のソースの `package`。別のパッケージの関数への参照は、使用に数えない（そのパッケージが自分の本体で検査される）。単なる到達可能性（呼ばれるか）ではなく、**コンパイルしたパッケージの全関数**（呼ばれない関数も、インスタンス化されたジェネリック関数も）を見る（決定事項 D4）。インスタンス化されないジェネリック関数はプログラムに含まれず、見ない。
- **テストとベンチ**: `test`・`bench`（とその生成関数）の使用は `E1036` にしない。`audit` には `scope: "test"` で出す（決定事項 D6）。
- **LSP**: 検査しない。**権限は実行時のサンドボックスではない**: 検査はコンパイルした静的な使用に対するもので、`extern` の先の C のコードが何をするかは制限できない（`ffi` を許したパッケージは任意の C を呼べる）。文書に書く。

### 3. 権限の導出（付録 A）

権限を持つ源は、(a) 組み込みの私的関数と `Bench.now` の 33 個（A.2）、(b) `extern` の関数（`ffi`）、(c) `Gpu` の 5 個の明示（A.3）。std の関数の権限は、参照する源と std の関数の**推移閉包**。利用者の関数の使用は、参照する std の関数の権限と `extern` の `ffi`。結果の std の公開関数 85 個は付録 A.5 の表。**完全な決定表は付録 A**（47 ファイル、各モジュールの権限と、権限を持つ関数の全件）。

### 4. `E1036` と `E1037` と `W1010`（付録 C）

| コード | kind | 出す段 | 位置 | 終了コード |
| --- | --- | --- | --- | --- |
| `E1036` | `capability_denied` | 型検査のあと（`Project::check_capabilities`）。`check`・`build`・`run`・`test`・`bench`。`doc` では出さない | 使用の地点の参照の式 | 1（コード生成の前に失敗） |
| `E1037` | `trusted_wrong_target`・`trusted_unknown_property`・`trusted_duplicate_property`・`trusted_type_property`・`trusted_repeated` | 構文 | 付録 C.2 | 1 |
| `W1010` | `untrusted_extern` | `--warn untrusted-extern`・`audit` | `extern` の宣言の名前 | 既存の `--deny-warnings` の規則 |

`E1036` は同じ (source, start, end, 権限) で 1 件、(source, start, 権限) の昇順。メッセージは付録 C.1 の 3 つの形（ルート・依存で阻んだのがルートの `allow`・依存で阻んだのが親の項目）。型エラーがあるときは `E1036` を出さない（解析できない）。

### 5. `tsuzuri audit [PATH] [--json] [--deny-warnings]`（Phase 3）

`PATH` は省略すると `.`。`check` と同じに読み込み・型検査する（`Project::load`、`analyze_cached`。型エラーは通常の診断で終了コード 1、報告は出さない）。

| 終了コード | 条件 |
| --- | --- |
| 0 | 解析できて、`E1036` がなく、`--deny-warnings` でないか `W1010` がない |
| 1 | `E1036` が 1 件以上、`--deny-warnings` で `W1010` が 1 件以上、型エラー・マニフェストの誤り・`E1017` |
| 2 | 引数の誤り（`E2000`: `usage: tsuzuri audit [PATH] [--json] [--deny-warnings]`） |

**標準出力**に報告、**標準エラー**に診断（`E1036`・`W1010`。`--json` なら診断の JSON）。入力のディレクトリ名・絶対パス・日時・環境変数を報告に入れない。同じソースと同じコンパイラから同じバイト列。

**text**（1 行ずつ。例は付録 D）:

```text
audit: <ルート名>[ <version>]
policy: none | enforced
packages: <N>
（空行）
package <名前>[ <version>] (root | dependency)
  allowed: all | none | <権限を空白で並べる>
  granted: <親名>: <権限>[; <親名>: <権限>]          依存だけ。親名の昇順
  used: none | <権限を空白で並べる>
    <権限>: <N> site | <N> sites
      <path>:<line>:<column>  <callee>               1 権限あたり 5 件まで。超えたら "      ... and <M> more"
  denied: none | <権限>
  unused: none | <権限>                              ポリシーが有効なときだけ
  externs: none | <N> (<M> trusted)
    <name> -> "<symbol>"  <path>:<line>:<column>  trusted[ <性質>] | untrusted
（空行。パッケージの間）
summary: used=<権限を , で並べる | none> denied=<N> untrusted_externs=<N>
```

パッケージはルートが先頭、あとは名前の昇順。権限は `env ffi fs gpu net process random stdio time` の順。`path` はパッケージのルートからの相対（`/` 区切り）。ポリシーのないプロジェクトの `allowed` は `all`（全 9 個のとき）。

**JSON Lines**（1 行 1 オブジェクト。キーの順序は固定。文字列は `diagnostic::json_string`。整数は 10 進。配列が空なら `[]`。`null` は `version` だけ）:

| `type` | キー（この順） |
| --- | --- |
| `audit` | `type`・`schema`（`"tz-audit/1"`）・`root`（文字列）・`policy`（`"none"`・`"enforced"`）・`packages`（整数） |
| `package` | `type`・`name`・`version`（文字列か `null`）・`root`（真偽）・`allowed`（権限の配列）・`granted`（`{"by","capabilities"}` の配列。ルートは `[]`）・`used`（`{"capability","count","sites"[,"truncated"]}` の配列。権限の順）・`denied`（権限の配列）・`unused`（権限の配列。ポリシーが無効なら `[]`） |
| `extern` | `type`・`package`・`name`・`symbol`・`path`・`line`・`column`・`trusted`（真偽）・`properties`（文字列の配列）・`used`（整数） |
| `summary` | `type`・`packages`・`used`（権限の配列）・`denied`（整数）・`untrusted_externs`（整数）・`exit`（0 か 1） |

`sites` の要素は `{"path","line","column","callee","function","scope"}`（`scope` は `"program"`・`"test"`）。1 権限あたり 100 件まで（超えた件数は `truncated`。0 のときはキーを出さない）。行の順は `audit`・`package`（パッケージの順）・`extern`（パッケージ・行・列の順）・`summary`。キーは足すだけで互換（`tz-audit/1`）。Phase 4 は `package` に `effects` を足す（仕様 7）。

### 6. 上限と決定性

- 解析は関数 65,536・式のノード 16,777,216 まで（`capabilities::MAX_FUNCTIONS`・`MAX_NODES`）。超えたら `E1017`（付録 E.2）。依存のグラフは `load_packages` の既存の上限（1,024 パッケージ・深さ 128）。マニフェストの `capabilities` の配列は 9 個まで（重複は拒否するので自然に収まる）。
- 地点・パッケージ・`extern` の並びは入力だけで決まる（`BTreeMap`・整列。`HashMap` の反復順に依存しない）。計算量 O(式の総数 + 辺 × 9)。
- ポリシーが無効なら、解析は走らない（費用 0）。有効なとき、`check` の時間に走査が足される（手順 13 で `examples/` と std を読み込んだ状態で測って記録する。速度の閾値は CI に入れない）。

### 7. 互換性（`docs/stability.md`）

- マニフェストのキー・セクションの追加は互換（決定事項 D1 により、使わなければ何も変わらない）。新しい診断コード 3 つ（D-55 の割り当て）。`--warn untrusted-extern` と `audit`（D-55）の追加は互換。edition の差はない。
- `tz-audit/1` の JSON はキーを足すだけで互換（読み手は知らないキーを無視する）。互換のない変更は `tz-audit/2`。
- `@trusted` を足すのは互換、性質の語彙を足すのは互換、外すのは `W1010` が増えうる（警告）。
- A20 の `@no_*` と同じ位置・同じ語彙の属性を使う。名前は D-55 の `@trusted`。予約語は増えない（`trusted` は `@` の直後だけで意味を持つ）。
- **Phase 4（A20 後）**: `package` の行に `effects`（`{"declared","asserted":{"pure","no_alloc","no_copy","no_trap","terminates"}}`。各 `asserted` はそのパッケージで宣言した関数のうち、その属性を持つものの数）を足す。A20・A19 がなければキーを出さない。

## 設計

### 新規ファイルと型

| ファイル | 内容 |
| --- | --- |
| `src/capabilities.rs`（新規） | `Capability`・`CapabilitySet`・`Policy`・`Edge`・`PackageInfo`・`SourceInfo`・`Scope`・`Link`・`Site`・`ExternUse`・`Analysis`・`analyze`・`denied`・`untrusted_externs`・`Report`・`PackageReport`・`render_text`・`render_json`。型と関数の宣言は付録 E.1。`src/lib.rs` に `pub mod capabilities;`（`pub mod api_diff;` の隣） |
| `tests/trusted.rs`・`tests/capabilities.rs`・`tests/audit.rs`（新規） | 付録 G |
| `tests/fixtures/capabilities/`（新規） | `probe/Main.tz`（付録 A.7）、`std-table.txt`（付録 A.5 の 85 行） |

`Report`（Phase 3）: `Report { policy_enforced: bool, root: String, packages: Vec<PackageReport>, externs: Vec<ExternReport>, denied: usize, untrusted_externs: usize }`、`PackageReport { info: PackageInfo, allowed: CapabilitySet, granted: Vec<(String, CapabilitySet)>, used: Vec<UsedCapability>, denied: CapabilitySet, unused: CapabilitySet }`、`UsedCapability { capability: Capability, count: usize, sites: Vec<Site>, truncated: usize }`、`ExternReport { package: String, name: String, symbol: String, path: String, line: usize, column: usize, trusted: Option<u8>, used: usize }`。`render_text`・`render_json`・`exit_code(deny_warnings: bool) -> i32`。

### 段ごとの変更表

| Phase | ファイル | 場所 | 変更 |
| --- | --- | --- | --- |
| 1 | `src/syntax.rs` | `CpuAttribute` の隣・`Program` | `TRUSTED_PROPERTIES`・`TrustedAttribute`・`Program.trusted`（`cpu_attributes` の次） |
| 1 | `src/parser.rs` | `cpu_attribute` の隣・`program_all` | `trusted_attribute`（A20 が先なら `attributes()` に `@trusted` を足す。D-53）。`E1037` の 5 つ。`extern` の push で記録（付録 E.4） |
| 1 | `src/syntax_codec.rs` | `Program` の `put`/`get`・`impl Wire for TrustedAttribute` | `cpu_attributes`（A20 が先なら `effect_attributes`）の次に `trusted`。`GOLDEN` を更新。`src/frontend_cache.rs` の `FRONTEND_FORMAT` を 4 → 5 |
| 1 | `src/formatter.rs`・`src/api_diff.rs` | `canonicalize`・`Program` の分解 | `trusted` の `target` を正規化し span を消す／`trusted: _` |
| 1 | `src/check.rs` | `FunctionOrigin`・ユーザー関数の構築 | `trusted: Option<u8>`（`source()`・`generated()` は `None`）。`cpu_levels` の隣で結び付け |
| 1 | `src/driver.rs` | `Project` | `source_infos()`・`untrusted_extern_warnings()` |
| 1 | `src/main.rs` | `--warn`・`Arguments` | `warn_untrusted_extern`。`W1010` を `copies::warnings` の隣で足す |
| 1 | `src/bindgen.rs` | 2408 行付近のヘッダーの `format!` | 注意書きの 1 行 |
| 2 | `src/package.rs` | `Manifest`・`Dependency`・`parse_manifest`・`Line` | `capabilities` の field・セクション・依存の項目（付録 B） |
| 2 | `src/driver.rs` | `LoadedPackage`・`load_packages`・`load_from_root`・`Project`・`single_main` | `dependency_roots`、`Project.capabilities: Option<Policy>`（`single_main` は `None`）、`check_capabilities` |
| 2 | `src/capabilities.rs` | 全体 | `analyze`・`denied`・A.2 の表・A.3 の表 |
| 2 | `src/main.rs` | `analyze_cached` の直後 | `check_capabilities` を呼び、あれば出して `ExitCode::FAILURE`（`Doc` は除く） |
| 3 | `src/capabilities.rs`・`src/main.rs` | `Report`・`audit_command`・`main` の分岐（`api-diff` の隣） | 付録 D の書式 |

### `analyze` の要点

アルゴリズムは付録 E.2（番号つきの手順）、実効の権限は付録 E.3。要点: 家族の根をたどって属するソースを決める → 式木から参照を集める → std の関数の権限を不動点で求める → std でない関数の使用を地点にする。ポリシーが有効でないときは `Project::check_capabilities` が `analyze` を呼ばずに空を返す。`audit` は常に呼ぶ。

## 前提とする他チケットのインターフェース

| チケット | 使う名前・形 | 未実装のときの代替 |
| --- | --- | --- |
| [G22](G22-diagnostic-v2.md)（D-51） | `Diagnostic::with_kind(&'static str)`・`.related(RelatedRole::ViaCall \| DeclaredHere \| Note, impl Into<String>, Span)`・`.with_help(impl Into<String>)` | 呼ばない。メッセージ・コード・主スパンだけ（付録 C）。G22 が入ったら呼び出しを足すだけ |
| [A20](A20-effect-summary-and-checked-attributes.md)（D-53）Phase 4 | `effects::summarize`・`Options`（`Options::new(scope, computed)`・`checking`。E15 が `trusted` を足す）・`Effect::Extern`・`forbidden(EffectKind)`・`FunctionEffects`（HostCall の腕が `Extern` を源にする） | Phase 4 を実装しない。`@trusted` の性質は受理して持つだけ |
| [A19](_completed/A19-pure-functions.md) | `pure_rules::validate` が `extern` を不純にする | 同上（A20 の経路 A では A20 が置き換える） |
| [G26](G26-facts-model.md) | 使わない | — |
| D-57 | 権限の名前 9 個（閉じている）・既定（ルートは全権限、依存は空）・実効 = 積 | — |

## 実装手順

各手順の最後で `cargo build --release --locked` と `cargo test --locked` が緑であること。手順を飛ばさない。

### Phase 1

#### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`mkdir -p target/tz-E15/before && cp target/release/tsuzuri target/tz-E15/tsuzuri-before`。次の 14 個の入力を `target/tz-E15/tsuzuri-before build <入力> --emit llvm -O0 -o target/tz-E15/before/<名前>.ll` と、`--target wasm32` を付けた同じコマンド（`<名前>.wasm32.ll`）で出して保存する: `examples/` の `computations`・`control`・`currying`・`functional`・`hello`・`io`・`point`・`polymorphism`・`tasks`（`desktop`・`gpu`・`native`・`web` は `--emit llvm` で build できない）、`tests/fixtures/control`・`primitives`・`stdlib`・`tasks`、付録 A.7 のプローブを `target/tz-E15/probe/Main.tz` に保存したもの。
- 確認: すべて通る。基準の `.ll` が 28 個できている。

#### 手順 2: 失敗するテストを先に書く

- 変更: `tests/trusted.rs`（新規）。
- 内容: 付録 G の G9〜G13（`tests/trusted.rs` の 5 件。G14 は `tests/bindgen.mjs`）を書く。入力のソースは付録 C と D.5。`tests/packages.rs` の `Project` の作り方（`root(name, files)`）と `tests/editions.rs` の子プロセスの呼び方（`env!("CARGO_BIN_EXE_tsuzuri")`）を手本にする。
- 確認: `cargo test --locked --test trusted` は `running 5 tests` で 5 件失敗。

#### 手順 3: 構文・parser・符号化

- 変更: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/formatter.rs`・`src/api_diff.rs`・`src/frontend_cache.rs`。
- 内容: 付録 E.4 のとおり。`TrustedAttribute` を足し、`Program` に `trusted` を足す（`Program { … }` を全 field で書く箇所は `src/parser.rs` の `program_all` と、`grep -n "cpu_attributes" src` が示す箇所）。`Parser::trusted_attribute` と `E1037` の 5 つ。`syntax_codec` に `put`/`get`。`GOLDEN` はテストの失敗の出力の `hex` をコピーする。`FRONTEND_FORMAT` を 5 に。
- 確認: `cargo test --locked`（`pins_encoding_format` は新しい `GOLDEN` で通る）。`tests/trusted.rs` の G10（拒否）と G12（整形）が通る。

#### 手順 4: `FunctionOrigin.trusted`

- 変更: `src/check.rs`。
- 内容: 付録 E.4。`FunctionOrigin` に `trusted: Option<u8>`。`FunctionOrigin::source` と `generated` は `None`。`cpu_levels` の隣で `trusted_by_id` を作る。`FunctionOrigin { … }` を全 field で書く箇所（`grep -n "FunctionOrigin {" src`）に足す。
- 確認: `cargo test --locked`。G9 が通る。

#### 手順 5: `extern` の解析と `W1010`

- 変更: `src/capabilities.rs`（新規。型は付録 E.1 の `SourceInfo`・`ExternUse` と `untrusted_externs` だけ）、`src/lib.rs`、`src/driver.rs`。
- 内容: `ExternUse` を作る関数 `externs(module, sources) -> Result<Vec<ExternUse>, Diagnostic>`: 本体が `HostCall` の関数を `extern` とし、ほかの全関数の式木（`children()`）から `Function(User(id))`・`GenericFunction(id, _)`・`Closure(id, _)` で `extern` を参照する地点を `uses` に集める（`extern` でない関数から。`SourceInfo` は付録 E.2 の 2 と同じ規則）。`untrusted_externs` は付録 C.3。`Project::source_infos()`（`self.sources` の順。`SourceInfo.path` は、パッケージつきなら `source.path` から `id.root` を除いた `/` 区切り、なければ `relative_path`）と `Project::untrusted_extern_warnings(&self, module) -> Result<Vec<Diagnostic>, Diagnostic>`（ルートのパッケージで宣言した `extern` だけ）。
- 確認: `cargo test --locked`。

#### 手順 6: `--warn untrusted-extern`

- 変更: `src/main.rs`。
- 内容: `parse_arguments` の `--warn` の腕で `implicit-copy` のほかに `untrusted-extern` を受理する（名前の一覧は `implicit-copy, untrusted-extern`。メッセージは `--warn requires a warning name; supported: implicit-copy, untrusted-extern`・`unknown warning '{}' for --warn; supported: implicit-copy, untrusted-extern`・重複は `warn untrusted-extern specified more than once`）。`Arguments.warn_untrusted_extern`（`warn_implicit_copy` の隣）。`fmt` に指定したときの拒否の条件（`optimization.is_some() || … || warn_implicit_copy`）に足す。`main` の `copies` の隣で、`warn_untrusted_extern` のとき `project.untrusted_extern_warnings(&module)` を足す（`Err` は診断として出して失敗）。ヘルプの `--warn implicit-copy` の行の次に `--warn untrusted-extern   Report W1010 at extern functions that are used but not marked '@trusted'`。
- 確認: `src/main.rs` の `--warn` のテスト 3 か所（`parses_warn_implicit_copy` ほか）の期待メッセージを更新し、`untrusted-extern` の受理・重複・`fmt` での拒否を足す。`cargo test --locked --test trusted` が `running 5 tests` で 5 件通る。

#### 手順 7: bindgen の注意書き

- 変更: `src/bindgen.rs`、`tests/fixtures/bindgen/{basic,extended,include,names,skipped}/expected.tz`、`tests/bindgen.mjs`、`_tsuzuri/language-reference/compiler/native-interop.md`。
- 内容: ヘッダーの `format!("{MARKER}\n// header: …\n// clang: …\n// target: {}\n", …)` の末尾に、仕様 1 の注意書きの行と改行を足す（`// options:` の行はその次）。5 つの期待ファイルの `// target: <target>` の次に同じ行を足す。`tests/bindgen.mjs` に、`lines[4]` が注意書きであること、出力に `@trusted` がないことを足す（G14）。`native-interop.md` の生成物の例を同じに直す。
- 確認: `node tests/bindgen.mjs target/release/tsuzuri`（`cargo build --release --locked` の直後）が通る。`cargo test --locked`。

#### 手順 8: 文書（Phase 1）

- 変更: 「ドキュメント」の表の Phase 1 の行。
- 確認: 変更した各ページで `node scripts/check-docs.mjs <ページ>` が通る。`cargo test --locked`。手順 1 の基準の `.ll` と比べて 28 個とも 1 byte も変わらない。

### Phase 2

#### 手順 9: テスト先行

- 変更: `src/capabilities.rs`（付録 E.1 の型と `todo!()` の本体。コンパイルが通る最小）、`tests/capabilities.rs`（新規）、`tests/fixtures/capabilities/`（新規）、`src/package.rs` の単体テスト。
- 内容: 付録 G の G1〜G8・G15〜G26 を書く（G1〜G5 は `src/capabilities.rs` の単体テスト）。`tests/fixtures/capabilities/probe/Main.tz` は付録 A.7、`std-table.txt` は付録 A.5 の 85 行。G19〜G25 は付録 D.4 と C.1 のプロジェクトをテストの中で一時ディレクトリに作る。`analyze_with_limits(module, sources, max_functions, max_nodes)` を `pub(crate)` で足す（G26）。
- 確認: `cargo test --locked --test capabilities` は `running 12 tests` で全件失敗（`todo!()`）。`cargo test --locked` のほかのテストは通る。

#### 手順 10: `Capability`・`CapabilitySet`

- 変更: `src/capabilities.rs`。
- 内容: 付録 E.1 の `Capability`（`ALL`・`name`・`from_name`）と `CapabilitySet`（`u16`）を実装する。`NAMES` の文字列は `env, ffi, fs, gpu, net, process, random, stdio, time`（エラーメッセージで使う）。
- 確認: G1・G2 が通る。

#### 手順 11: マニフェスト

- 変更: `src/package.rs`、`tests/packages.rs`。
- 内容: 付録 B のとおり。`Manifest.capabilities: Option<(CapabilitySet, Span)>`（`registry` の隣）と `Dependency.capabilities: Option<CapabilitySet>`（`native` の隣）。`parse_manifest` の見出しの判定に `[capabilities]` を足す（`else { cursor.take("[native]")… }` の連鎖。エラーのメッセージは付録 B.2）。キー `allow` の解析（`Line::string_array`）。`Line` に `capability_names(&mut self, form: &str) -> Result<CapabilitySet, Diagnostic>`（未知の名前・重複を拒否）。`path_dependency` は `(DependencySource, bool, Option<CapabilitySet>)` を返し、`native` のあとに `, capabilities = [...]` を受理する。`git_dependency` は `rev` のあと、`registry_dependency` は `version` のあとに同じ項目を受理する。`Manifest { … }`・`Dependency { … }` の構築は `parse_manifest` の 2 か所だけ。既存の 2 つのテストの期待メッセージ（付録 F）を更新する。
- 確認: G6〜G8 が通る。`cargo test --locked`（`tests/packages.rs` のメッセージを含む）が通る。

#### 手順 12: ポリシー

- 変更: `src/driver.rs`、`src/capabilities.rs`。
- 内容: `LoadedPackage` に `pub(crate) dependency_roots: Vec<(String, PathBuf)>`。`load_packages` の `let mut dependencies = Vec::new();` の次に `let mut dependency_roots = Vec::new();` を置き、依存ごとに `dependency_root` が決まったところ（`dependencies.push((dependency_root, …))` の直前）で `dependency_roots.push((name.clone(), dependency_root.clone()));`、`loaded.insert` の `LoadedPackage { … }` に `dependency_roots` を渡す（`resolve` が `None` を返して `continue` した依存は辺にしない）。`Policy::build`（付録 E.3）。`Project` に `pub capabilities: Option<capabilities::Policy>`（`load_from_root` で `Policy::build(directory, &packages)`。`packages` を `manifests` の組み立てで消費する前に作る。`single_main` は `None`）。
- 確認: G4・G5 が通る。`cargo test --locked`。

#### 手順 13: `analyze`

- 変更: `src/capabilities.rs`。
- 内容: 付録 E.2 の手順 1〜7 と、付録 A.2 の表（`fn primitive(builtin: &str, source: &SourceInfo) -> Option<Capability>`。`Builtin::name()` の文字列で引く。表にない `__` の名前は `None`）・A.3 の表。`externs`（手順 5）を `analyze` の一部にする。ジェネリックな std の関数のインスタンス（名前が `….$mono.N`）は `origin.module == Std` で std と判定する。属するソースは家族の根（付録 E.2 の 2。`$mono` の複本は自分自身が根）の `span.source`。
- 確認: G3・G15・G16（std のソースの固定表）・G17・G23・G25・G26 が通る。`examples/` の全プロジェクトで `analyze` が `E1017` にならない（G15 のプローブで時間を測り、`check` の時間に対する割合を手順の報告に書く）。

#### 手順 14: `E1036`

- 変更: `src/capabilities.rs`、`src/driver.rs`、`src/main.rs`。
- 内容: `denied`（付録 C.1。阻んだ制限は `Policy::blocker`）。`Project::check_capabilities(&self, module) -> Result<Vec<Diagnostic>, Diagnostic>`: ポリシーが `None` か `enforced` でなければ `Ok(Vec::new())`、そうでなければ `analyze` して `denied`。`main` の `project.analyze_cached(…)` の `Ok(module)` の直後（`Action::Doc` を除く）で呼び、診断があれば `DiagnosticSet::from_diagnostics(診断, 0)` に包んで `print_diagnostics(&set, &project, arguments.json)` し、`ExitCode::FAILURE`。`Err` も診断として出す。
- 確認: G18〜G22・G24 が通る。`cargo test --locked`。手順 1 の基準の `.ll` と比べて 28 個とも変わらない（ポリシーのないプロジェクト）。

#### 手順 15: 文書（Phase 2）

- 変更: 「ドキュメント」の表の Phase 2 の行。
- 確認: 変更した各ページで `node scripts/check-docs.mjs <ページ>` が通る。`cargo test --locked`。

### Phase 3

#### 手順 16: `Report` と描画

- 変更: `src/capabilities.rs`。
- 内容: 仕様 5 の `Report`・`PackageReport`・`UsedCapability`・`ExternReport`。`Report::new(policy: Option<&Policy>, root_input: &Path, analysis: &Analysis, sources: &[SourceInfo]) -> Report`: パッケージの順・`granted`（`Policy.edges` の `child` が一致する辺を親名の昇順）・`allowed`（`Policy.effective`。ポリシーなしは `ALL`）・`denied`（`used` のうち `allowed` にないもの。`Scope::Program` の使用だけ）・`unused`（`enforced` のとき `allowed` − `used`）・`extern`・件数。ポリシーのない（マニフェストのない）プロジェクトのパッケージ名は、入力が存在するディレクトリならその最後の要素、ファイルなら拡張子を除いたファイル名。`render_text`・`render_json`・`exit_code`。
- 確認: 単体テスト（付録 D の期待値の文字列との一致）が通る。

#### 手順 17: `audit_command`

- 変更: `src/main.rs`、`src/driver.rs`。
- 内容: `api_diff_command` を手本に `audit_command(arguments: &[OsString]) -> ExitCode`（`main` の `api-diff` の分岐の隣に `raw.first() == "audit"`）。引数: `PATH`（0 または 1 個）・`--json`・`--deny-warnings`・`--`。それ以外・重複は `usage` の `E2000`（終了コード 2）。`Project::load(path)` → `analyze_cached(true, AnalysisKind::Program)`（`Err` は `print_diagnostics` して終了コード 1）→ `Project::audit(&module) -> Result<Report, Diagnostic>`（`analyze` と `Report::new`）→ 標準出力に `render_text`/`render_json` → `E1036`（ポリシーが有効なとき `denied`）と `W1010`（`untrusted_externs(&analysis, false)`）を標準エラーへ（`--json` なら JSON の診断）→ `exit_code`。ヘルプ（`help()`）に `tsuzuri audit [PATH] [--json] [--deny-warnings]` の行。
- 確認: G27〜G32 が通る（`tests/audit.rs`）。`cargo test --locked`。

#### 手順 18: 文書と最終確認（Phase 3）

- 変更: 「ドキュメント」の表の Phase 3 の行。
- 確認: 各ページで `node scripts/check-docs.mjs`。`cargo build --release --locked && cargo test --locked && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`。手順 1 の基準 28 個と 1 byte も違わない。`tsuzuri audit examples/io`・`examples/hello`・`examples/tasks` と付録 D.4・D.5 のプロジェクトの出力が付録 D と一致する。完了報告を書く（GUIDE §13.2）。

### Phase 4（A20 が済んでから）

#### 手順 19: `@trusted` の性質を検査に効かせる

- 変更: `src/effects.rs`（A20。`Options`・`summarize` の `HostCall` の腕）、A19 の `pure_rules`（A20 の経路 A なら不要）、`src/capabilities.rs`・`src/main.rs`（audit の `effects`）。
- 内容: `effects::Options` に `pub trusted: Option<u8>`（性質のビット 1 つ）を足す（`Options::new`・`checking` は `trusted: None` を入れる。構造体のリテラルを書いている箇所は `Options::new` に直す）。`summarize` が本体が `HostCall` の関数を走査するとき、`options.trusted` が `Some(bit)` で `function.origin.trusted` が `Some(bits)` かつ `bits & bit != 0` なら、`Extern` の源を足さない。`validate` は、属性ごとに（`pure` は A19 の検査が）対応する性質のビットを `Options.trusted` に入れて呼び、`@trusted` のない `extern` の呼び出しは従来どおり違反にする。**A21 の契約の純粋性（`validate_purity`）も `pure` の性質のビットを入れて呼ぶ**（`@trusted pure extern` を `pure def` と同じに契約でも許す）。`audit` の `package` の行に `effects` を足す（仕様 7。A20 がなければキーを出さない）。
- 確認: `tests/trusted.rs` に `trusted_pure_extern_may_be_called_from_a_pure_def`・`trusted_no_alloc_does_not_allow_pure`・`untrusted_extern_still_breaks_no_trap` を足して通る（ソースは付録 C と D.5 の `c_abs`）。A20 の既存のテストが期待を変えずに通る。

## テスト計画

名前・入力・期待値の全表は [付録 G](E15-appendix.md#付録-g-テストの一覧名前入力期待値)（G1〜G32）。

| Phase | ファイル | 件数 | 付録 G | 書く手順 |
| --- | --- | --- | --- | --- |
| 1 | `tests/trusted.rs`（新規）。G14 は `tests/bindgen.mjs` | 5＋1 | G9〜G14 | 手順 2（赤）・3〜7（緑） |
| 2 | `src/capabilities.rs` の単体 5、`src/package.rs` の単体 3、`tests/capabilities.rs`（新規） | 5＋3＋12 | G1〜G8・G15〜G26 | 手順 9（赤）・10〜14（緑） |
| 3 | `tests/audit.rs`（新規）、`src/main.rs` の引数のテスト | 5＋1 | G27〜G32 | 手順 16〜17 |
| 4 | `tests/trusted.rs` に 3 件 | 3 | 手順 19 | 手順 19 |

- **独立した参照**: 権限の期待値は付録 A.5 の表（std のソースを人が読んで作った。付録 A.6 の固定表が std の変更を検出する）と、付録 A.7 のプローブの位置（実ファイルの列を数えた値）。`audit` の期待値は付録 D（`examples/` と、付録 D.4・D.5 の全文のプロジェクトの実ファイルから位置を数えた）。
- **既存テストへの影響**: 付録 F。ポリシーのないプロジェクトの出力は変わらない（G18 と手順 1 の基準）。
- **決定性・資源上限**: G26（上限）、G30（出力が置き場に依らない）。IR を変えないので IR の決定性は既存のテストのまま。

## ドキュメント

手順 8・15・18 は、次の表の自分の Phase の行を更新する（`LR/` は `_tsuzuri/language-reference/`）。新しい例は `node scripts/check-docs.mjs <ページ>` を通る形（`extern` を呼ぶ例は `run=` を付けず `check` だけが通る形にする。`extern "abs"` は付録 H のとおり実行できる）。

| Phase | ページ | 更新 |
| --- | --- | --- |
| 1 | `LR/values-and-functions/attributes.md` | 属性の一覧を 5 つから 6 つに（`@trusted`）。置き場・性質・「印であって検査ではない」 |
| 1 | `LR/compiler/native-interop.md` | `@trusted` の節（`extern` の信頼の表明）。bindgen の出力例の注意書き。`W1010` |
| 1 | `LR/compiler/diagnostics.md`・`LR/compiler/usage.md` | `E1037`・`W1010`。`--warn untrusted-extern` |
| 1 | `docs/language.md`・`README.md`・`docs/architecture.md` | 属性の表・CLI の表・`capabilities.rs` の行 |
| 2 | `LR/organizing-tsuzuri/packages.md`・`LR/compiler/directives.md` | `[capabilities]` と依存の `capabilities`（文法は付録 B）。権限 9 個の表（付録 A の要約: どのモジュールが何を要るか）。ポリシーの有効化の規則。「サンドボックスではない」。LSP は検査しない |
| 2 | `LR/compiler/diagnostics.md` | `E1036` |
| 2 | `LR/built-in-types-and-modules/file.md` ほか（`dir`・`env`・`time`・`random`・`process`・`net`・`gpu`・`io`・`debug` の各ページ） | 各ページの先頭に「必要な権限: `fs`」の 1 行（付録 A.4）。ページがないモジュールは省く（`ls LR/built-in-types-and-modules` で確かめる） |
| 2 | `docs/language.md`・`docs/stability.md`・`README.md` | マニフェストの表・互換性（キーの追加）・用語 |
| 3 | `LR/compiler/usage.md` | `tsuzuri audit` の節（text・JSON のキー・終了コード。付録 D の例） |
| 3 | `README.md`・`docs/architecture.md`・`docs/stability.md` | CLI の表・`tz-audit/1`（キーを足すだけで互換） |
| 4 | `LR/compiler/native-interop.md`・`LR/values-and-functions/attributes.md` | `@trusted` の性質が `pure`・`@no_*` の検査に効く |
| — | `_features/README.md`・台帳 | 統括者が更新する（`--warn untrusted-extern` を D-55 に足す。付録に「追加が必要な名前」） |

## 受け入れ条件

### Phase 1

- [ ] `@trusted`・`@trusted pure no_trap` が `extern` の関数と型に受理され、`E1037` の 5 つの kind が付録 C.2 のとおり拒否される。
- [ ] `--warn untrusted-extern` が `W1010` を出し、`@trusted` で消える。既定では出ない。
- [ ] `tsuzuri fmt --check` が `@trusted` を保つ。bindgen の出力に注意書きがあり `@trusted` がない。5 つの期待ファイルが通る。
- [ ] 手順 1 の基準の `.ll` 28 個が 1 byte も変わらない。文書を更新し `check-docs` が通る。

### Phase 2

- [ ] 付録 A.7 のプローブが 14 地点・9 権限で期待どおり。付録 A.5 の 85 行と A.6 の固定表のテストが通る。
- [ ] 付録 D.4 が `E1036`（終了コード 1）。ルートの `allow`・依存の `capabilities`・積（G4・G21）が仕様どおり。テストとベンチは免除。
- [ ] ポリシーのないプロジェクトの出力が HEAD と同じ。`[capabilities]`・`capabilities` の不正が付録 B.2 のメッセージで拒否される。
- [ ] 文書を更新し `check-docs` が通る。

### Phase 3

- [ ] `tsuzuri audit` の text と JSON が付録 D.1〜D.5 と一致し、置き場に依らない。終了コードが仕様 5 の表のとおり。
- [ ] `cargo build --release --locked && cargo test --locked && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings` が緑。

### Phase 4

- [ ] `@trusted pure` の `extern` を `pure def` から呼べ、`@trusted no_alloc` は `pure` を許さず、印のない `extern` は従来どおり違反。`audit` の `effects` が A20 の要約と一致する。

## 落とし穴

1. **`fn rec`・`def and`・`def name { r } ::`**: std のソースを調べる補助の解析で、`fn rec f`・領域つきの署名（`def of_matrix { r } ::`）を取りこぼすと権限を落とす（付録 A の作成中に実際に起きた）。権限の導出は**型検査後の式木**で行い、ソースの文字列を使わない。検出: G16・G17。
2. **局所変数と関数の同名**: `let mut close = 1` と `Net.close` を取り違えない。式木の `Function(User(id))` の出現だけを辺にする。検出: プローブの `Net.parse_address` が 0 地点（G15）。
3. **生成関数の家族**: 属するパッケージは家族の根の `span.source` で決める。`$mono` の複本は**自分自身が根**（`polymorph::instantiate` が自分の添字を `parent` に入れる。`parent` をたどるときは自己参照と循環で止める）、`$builtin`・`$lambda` はそれを含む関数が根。検出: G23（ジェネリックの 2 つのインスタンス）。
4. **`Os.__read`・`__write` の多重化**: 呼ぶ関数の属する std のファイルで `fs`・`env` を決める。`File`・`Dir`・`Env` 以外から参照されていれば、表にない（無視してテストで落とす）。検出: G16。
5. **依存の宣言順**: マニフェストの依存の順や HashMap の順に依存しない。`BTreeMap`・整列を使う。検出: G30。
6. **ポリシーが有効かの判定**: ルートの `[capabilities]`、または**ルートの**依存の項目の `capabilities`。深い階層だけの記述では有効にならない。検出: G5・G18。
7. **テストとベンチの免除**: 生成関数の家族の根に `origin.test`・`origin.bench` があるかで決める。テストの `Gen.for_all` の使用を `E1036` にしない。検出: G24。
8. **`audit` の絶対パス**: `SourceInfo.path` はパッケージのルートからの相対。`source.path`（絶対）を出さない。診断の主の `path` は既存の表示（絶対）のままで、報告（stdout）だけ相対。検出: G30。
9. **`--warn` のメッセージ**: 一覧は D-55 の固定順（`implicit-copy, canonical, untrusted-extern`）で、実装済みの値だけを並べる（A22 が先に入っていれば `canonical` は `implicit-copy` の次、E15 だけなら `implicit-copy, untrusted-extern`）。3 か所の期待を同時に直す。検出: `cargo test --locked --bin tsuzuri`。
10. **bindgen の期待ファイル**: 注意書きの位置は `// target:` の次（`// options:` の前）。`tests/bindgen.mjs` は `lines[1]`〜`lines[3]` を置換してから全文を比べる。検出: `node tests/bindgen.mjs`。

## 対象外

- 実行時のサンドボックス・OS の権限の剥奪・`extern` の先の C のコードの制限。`unsafe`。
- LSP での `E1036`・`audit` の LSP 対応。`tsuzuri new` が `[capabilities]` を書くこと。
- `Tsuzuri.lock` への権限の記録。G26 の facts・KB への権限の追加。
- `extern type` の `@trusted` の報告・検査。`@trusted` が ABI を検査すること（`tsuzuri bindgen` が一致を確かめることとは別）。
- ネットワークのホスト名・ファイルのパスごとの細かい許可（権限は 9 個の粒度）。

## 決定事項

D1〜D15 はすべて**承認済み**。実装者は蒸し返さない。

| # | 決定 | 理由 | 却下した案 |
| --- | --- | --- | --- |
| D1 | ポリシーは**オプトイン**: ルートの `[capabilities]` か、ルートの依存の `capabilities` があるときだけ検査する | 依存の既定が「空」なので、常に検査すると既存の複数パッケージのプロジェクトがすべて `E1036` で壊れる。`docs/stability.md` は、受理されていたプログラムを新しい診断で拒否することを許さない | 常に検査する案（既存のプロジェクトが壊れる）。CLI のフラグや環境変数で有効にする案（ポリシーはソースと一緒に版管理するマニフェストのもの） |
| D2 | 検査は**型検査のあとの別の段**（`Project::check_capabilities`）。`check_modules`・`SourceInput`・`ModuleInput` を変えない。`audit` も同じ解析を使う | 変更が driver・main・新しい 1 ファイルに閉じる。11 か所の `SourceInput` の構築と cache を触らない。解析は特殊化・クロージャの持ち上げ後の式木で、判定が単純 | `check_modules_collect` の中で検査する案（`SourceInput`・`ModuleInput` に field が要り、LSP や cache まで影響する）。ソースの文字列を調べる案（局所名の取り違えで権限を落とす。落とし穴 1・2） |
| D3 | 権限は**私的な組み込み関数の表（A.2）＋ std の参照の推移閉包＋`extern`＋`Gpu` の 5 つの明示（A.3）**で決める | 世界に触れる源は 33 個の組み込み関数だけで（E1022 が利用者の直接の呼び出しを禁じる）、それ以外はすべて導出できる。新しい std の関数は自動で正しい権限を持つ | モジュール名で決める案（`Net.parse_address` や `Os` の純粋な関数まで権限を要る。D-57 の `Path` の方針と食い違う）。A20 の `os` 効果を使う案（`fs`・`env`・`time`・`random`・`process` を区別できない） |
| D4 | コンパイルしたパッケージの**全関数**を見る（到達可能性を調べない）。インスタンス化されないジェネリック関数は見ない | 呼ばれない経路に潜む使用こそ供給網の危険で、到達可能性の解析（動的な呼び出し・関数値で穴が空く）より単純で安全。プログラムに含まれないものは実行されない | 呼び出しグラフの到達可能な範囲だけ（種の案。ルートのない依存のライブラリでは根が決まらず、関数値・`dyn` の経路で穴が空く） |
| D5 | 実効の権限は、**すべての入辺**の `実効(親) ∩ 項目` の積 | 依存はビルドに 1 つしか入らないので、どの親も許していない権限を使えてはならない | 和（1 つの親が許せば使える）。直接の親だけの積（別の親が絞った許可を破る） |
| D6 | `test`・`bench` の使用は `E1036` にしない。`audit` には `scope: "test"` で出す | ポリシーは出荷するコードの制限。テストは `Debug.print`・`File` を使うのが普通で、`allow` を広げさせると意味が薄れる。依存のテストはコンパイルされない | テストも検査する案（`allow` にすべて足すことになる） |
| D7 | `@trusted` は**印**。検査も生成コードの変更もしない。性質は A20 の語彙（`pure`・`no_alloc`・`no_copy`・`no_trap`・`terminates`）で、Phase 4 まで保持するだけ | 人が確かめた事実の表明で、コンパイラが確かめられる性質ではない。性質を語彙として先に決めておくと A20 が依存できる | 検査する案（C の関数の挙動は検査できない）。性質なしの印だけ（A20 の `pure def` から呼べず、`extern` を使う関数を純粋にできない） |
| D8 | 印は `Program.trusted`（名前で結ぶ）と `FunctionOrigin.trusted: Option<u8>` に持つ。`Signature` に field を足さない | `@cpu` と同じ配線で、`Signature` の構築箇所を触らない。`Option` で「`@trusted` だけ」と「印なし」を区別する（W1010 の判定） | `Signature.trusted`（構築箇所が多い）。`u8` だけ（性質のない印が表せない） |
| D9 | `W1010` は**使われた**印のない `extern` に、宣言の名前で出す。`--warn untrusted-extern` はルートのパッケージの宣言だけ、`audit` は全パッケージで常に出す | 宣言に印を足すのが直し方で、直せるのは自分のパッケージだけ。使われない `extern` は危険でない。D-55 は「`audit` が既定で警告に格上げ」 | 使用の地点ごとに出す案（件数が多い）。依存の `extern` も `--warn` で出す案（利用者が直せない） |
| D10 | `tsuzuri bindgen` は `@trusted` を付けず、注意書きの 1 行を `// target:` の次に足す。`MARKER` は変えない | 一致を確かめたのは ABI で、挙動ではない。1 行目を変えると既存の生成ファイルの上書きの判定（`MARKER`）が壊れる | `@trusted` を自動で付ける案（確かめていない信頼を作る）。注意書きなし（種の要件に反する） |
| D11 | マニフェストの権限の記述の誤りは `E0002`（`Line::error`） | 同じファイルの値の誤り（`edition`・`native`・`rev`）がすべて `E0002`。`E1011` は重複・循環・名前空間の意味の誤り | 種の `E1011`（値の構文の誤りに別のコードを使うことになる） |
| D12 | `tsuzuri audit` は標準出力に報告、標準エラーに診断（`E1036`・`W1010`）。終了コードは違反か `--deny-warnings` の `W1010` で 1 | `api-diff` と同じ分け方。報告をパイプで読め、診断は既存の仕組みで出る | 診断も報告に混ぜる案（JSON の読み手が型を区別する必要がある） |
| D13 | `audit` の JSON は `tz-audit/1`（`type` で行を区別する JSON Lines）。キーは足すだけで互換 | `tsuzuri test --json` と同じ流儀。G26 の KB に依存しない | 1 つの JSON オブジェクト（長い出力を逐次に読めない）。G26 の facts に載せる案（D-52 の `symbols.jsonl` のキーを変えられない） |
| D14 | Phase 4 は `effects::Options.trusted` を足し、属性ごとに `summarize` を呼ぶ | A20 の走査を変えず、緩和が 1 か所に閉じる。`extern` がなければ追加の呼び出しは起きない | 要約に属性ごとの証拠を持たせる案（A20 の構造を変える）。`@trusted` を全属性に効かせる案（`pure` は `alloc` を許す意味ではない） |
| D15 | 新しい予約語・crate・`unsafe`・WASM の import・生成コードの変更を作らない。名前は D-55（`E1036`・`E1037`・`W1010`・`audit`・`@trusted`）に、`--warn untrusted-extern` を**足す**（追加が必要な名前） | D-50 の方針 | — |

## 付録

[E15-appendix.md](E15-appendix.md) にある。A: std の権限の決定表（導出の規則・組み込み関数の表・47 ファイルの結果・85 関数の一覧・固定表・プローブ）。B: マニフェスト（文法・エラー・例）。C: 診断（`E1036`・`E1037`・`W1010` の完全な仕様と例）。D: `tsuzuri audit` の出力例（text と JSON、`examples/` の実プロジェクトと依存のあるプロジェクト）。E: Rust の骨格とアルゴリズム。F: 既存のテスト・文書への影響。G: テストの一覧。H: 実機で確かめたこと。
