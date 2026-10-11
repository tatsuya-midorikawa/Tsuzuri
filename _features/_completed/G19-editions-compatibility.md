# G19: 言語版（edition）と互換性・非推奨の管理

| 項目 | 内容 |
| --- | --- |
| ID | G19 |
| 優先度 | P3 |
| 規模 | M |
| 依存 | E04, G09, (E10) |
| 後続 | E08（エラー種別の追加）, D09（UCD の更新） |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | `053bced8`、2026-10-11 |
| 承認 | 全フェーズと必要な判断への包括承認（2026-10-11。旧 D1 の manifest の `edition`、旧 D4 の `@deprecated` と `W1005`、旧 D9 の `tsuzuri api-diff` を含む）。`W1005` は GUIDE D-30 の仮割り当てから D-16 の確定した割り当てへ移る |
| 利用者向け仕様 | [安定性と互換性の方針](../../docs/stability.md)、[パッケージ](../../_tsuzuri/language-reference/organizing-tsuzuri/packages.md#言語版edition)、[ドキュメント コメント](../../_tsuzuri/language-reference/organizing-tsuzuri/documentation-comment.md#非推奨の印)、[コンパイラの使い方（api-diff）](../../_tsuzuri/language-reference/compiler/usage.md#api-diff)、[診断](../../_tsuzuri/language-reference/compiler/diagnostics.md)、[言語仕様](../../docs/language.md#公開-api-の差分api-diff) |

## 目的と実装範囲

言語と標準ライブラリの変更で既存のコードが壊れないことを、方針と検査の仕組みで支える。実績そのものは時間でしか得られないので、今できる互換性の枠組みを作った。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | manifest の `[package] edition = "2026"` と、ソースごとの edition による予約語表の切り替え（`EDITION_KEYWORDS`。現在は空で、差はない）。doc comment の `@deprecated` と、別のパッケージ・std の非推奨の宣言の使用を知らせる警告 `W1005`。`docs/stability.md`。`tsuzuri new` の雛形と `tsuzuri fmt` の edition |
| Phase 2 | `tsuzuri api-diff <old> <new> [--json]`。2 つの版の公開 API を構文解析だけで比べ、breaking と non-breaking の変更を 1 行ずつ出す。互換性のない変更があれば終了コード 1 |

## 確定した仕様

### edition

- `Edition(u16)`（`src/syntax.rs`）。`E2026`、`LATEST`（manifest のない入力と std と、パッケージを知らない入口）、`MANIFEST_DEFAULT`（`edition` のない manifest。将来も `2026`）、`SUPPORTED`。`Edition::parse` は `SUPPORTED` の名前との完全一致だけを受ける。
- manifest の `[package]` のキー `edition`（省略可、順序は自由）。未対応の値は E0002 `unsupported edition '<value>'; supported editions: 2026`。未知のキーのメッセージは `unknown package key; expected name, version, namespace, or edition`。`tsuzuri new` の `Tsuzuri.toml` は `edition = "2026"` を持つ。
- ソースの edition は、`Manifest::edition` → `SourceFile::edition`（`src/driver.rs`）→ `SourceInput::edition`（`src/lib.rs`）→ `ModuleInput::edition`（`src/check.rs`）と運ばれ、lexer・parser の入口（`lex`、`lex_all`、`lex_with_trivia`、`parse_with_source`、`parse_with_source_all`、`parse_source`）の必須の引数になる。path・git・registry の依存は自分の manifest の edition で読む。`tsuzuri fmt` は入力のディレクトリ（ファイルなら親）の `Tsuzuri.toml` を読み、上位は探さない。
- lexer は、`Lexer::identifier` のキーワードの `match` より前に `reserved_in` で `EDITION_KEYWORDS` を引く。モジュール名の予約語の検査（E1011）も、そのモジュールの edition で行う。
- edition は字句と警告だけに作用する。型検査と IR は edition を見ないので、IR は edition の値と `@deprecated` の有無で byte 単位で同じ。
- frontend cache（G17）の parse key は、ソースの byte 列と compiler の同一性に edition を加えたもの（`FRONTEND_FORMAT` 2 → 3）。ビルドキャッシュのキーは manifest の本文を含むので変更不要。

### `@deprecated` と W1005

- `Documentation::deprecation`: 説明の各行の先頭の空白を除いて `@deprecated` で始まり、直後が行末か空白（スペース、タブ）である最初の 1 行が印。残りの前後の空白を除いたものが案内（空でもよい）。`@deprecatedX` は印ではなく、2 つ目以降は通常の説明。新しい構文はない。
- 対象は `def`（`extern def` を除く）、`record`、`union` の公開宣言。`const`・`type`・`class`・メソッド・`extern def` の印は表示されるだけ。
- W1005 は、利用者のモジュールにある `SemanticIndex::references` のうち、関数、record、union、union の case（印は union のもの。`Union.Case` は case の位置に 1 件）が、別のパッケージか std の印を持つ宣言を指すもの。メッセージは `'<Qualified.name>' is deprecated: <note>`（note は 200 文字で `...` で切る）、note が空なら `'<Qualified.name>' is deprecated; see its documentation for a replacement`。位置は参照の最後の名前で、同じ span は 1 回。既定で有効で、`--deny-warnings` で失敗になる。`check`・`build`・`run`・`test`・`bench`・`doc`・LSP が同じ経路で出す。
- 同じパッケージの中の使用には出さない。`SourceInput::package`・`ModuleInput::package` は依存パッケージの `PackageId` で、root パッケージ、manifest のないソース、std は `None`。印を使える宣言は std と依存パッケージのものだけで、`semantic::collect` は、使える印があるか `SemanticIndex` を求められたときだけ、1 回走る。
- 名前は完全修飾名（`'LegacyLib::Old.old'`、`'std::Legacy.Pair'`）。

### `tsuzuri api-diff`

- `Project::load_for_api`（`src/driver.rs`）で 2 つの入力を読む。git・registry の依存を取得せず、`Tsuzuri.lock` も読まず、`Project::root_package` のソースだけを `parse_with_source` で構文解析する。型検査はしない。
- 公開宣言は、`private` でない `def`、`extern def`、`extern type`、`record`、`union`、`type`、`const`、すべての `class`、`instance`、ビルダーの別名（`@alias`）、とそのメンバー（field、case、メソッド）、`deriving`、`export`。名前は `名前空間::モジュール.宣言[.メンバー]`（`instance` は `Class<Type>`）。
- 署名は `src/docgen.rs` の描画を共有する。比較には、型変数と region を最初に出てきた順の連番に置き換え、制約を並べ替え、定義の結果型の矢印を平らにした正規形を使う（`fn() -> T` の thunk と、引数のない定義の関数の結果は、parser と同じく平らにしない）。メッセージには各版で書かれたままの署名を出す。
- breaking: 公開宣言の削除（`private` 化を含む）、署名の変更（型・制約・region・型パラメーター・`export` の取り消し）、record の field の追加・削除・変更・並べ替え、union の case の追加・削除・変更・並べ替え、既定のない class メソッドの追加、メソッドの削除・署名の変更・既定の削除、superclass の変更、`deriving`・`instance` の削除と変更。non-breaking: 宣言・`export`・`instance`・`deriving` の追加、既定のあるメソッドの追加、説明の変更、`@deprecated` の追加と削除。
- 出力: テキストは `breaking: ` か `non-breaking: ` で始まる 1 変更 1 行、`summary: N breaking, M non-breaking`、`note:` の行（省略した件数、版の注記）。`--json` は 1 行 1 オブジェクトの JSON Lines（`type` が `change` の行と最後の `summary`）。breaking が先で、それぞれ名前順（決定的）。1000 行まで、超えた分は `omitted` に数える。
- 終了コード: 互換性のない変更がなければ 0、あれば 1。入力の読み込み・構文の失敗は診断を出して 1（標準出力は空）。引数の誤りは E2000 で 2。入力の保護（シンボリックリンク E1011、ソースの大きさ E0003、数 E1017）はほかのコマンドと共通。
- 両方のマニフェストの `version` が `MAJOR.MINOR.PATCH` で、新しい版が古い版の互換の範囲（1.0.0 以降は同じ MAJOR、それより前は同じ `0.MINOR`）に入っているのに breaking な変更があるときは、その注記を出す。

### 安定性の方針

`docs/stability.md`（新規）に、安定なもの（edition ごとの構文と意味、std の公開 API、公開 ABI、診断のコード・重要度・JSON のキー、api-diff の出力形式）と安定でないもの（IR の形、内部のシンボル、キャッシュの形式、診断の文言、警告の有無。`--deny-warnings` は保証の外）、edition の表と決め方、edition の追加の手順、非推奨の手順（`@deprecated` → 取り除くのは構文なら edition でだけ、パッケージの API は互換の範囲を変える版へ）、既知の例外（std へのモジュールの追加は同名の利用者モジュールを E1011 にする。std の union への case の追加は網羅的な `match` を壊す）、現時点の制限を書いた。

## 旧計画から外れた判断

| 旧判断 | 確定した判断と理由 |
| --- | --- |
| W1005 は `semantic::collect` の `entries` と、union の case の構築に足す `Construct` の target から出す（手順 8・9） | `SemanticIndex::references` と `definitions` から出す。`Construct` の target だけでは、payload を持つ case（`CaseConstructor`）、pattern、型の注釈を落とす。そのため pattern の中の case と record も警告になり、例の graph の警告は 3 件でなく **5 件**（pattern 2 件を含む）。pattern の中の型注釈（`| q: T ->`、`\(p: T) ->`、`for`）は record・union の参照として索引されないので警告しない（既知の限界） |
| pattern の中の参照は対象外（旧 D5） | 上のとおり、索引が記録する constructor の pattern は警告にする。使う側が移行で書き換える場所なので |
| `@deprecated` を含む project だけが `semantic::collect` の費用を払う | 使える印（std と依存パッケージの宣言）がある project だけ。root パッケージの印は、どのパッケージもそれに依存しないので使えない。コードレビューの指摘で、`SourceInput::package` を「依存パッケージ、root は `None`」に改めた（旧実装は 2 パッケージ以上のとき root の印でも走っていた） |
| 同じ package の判定は名前の文字列 | `Option<&PackageId>`（名前と root のパス）。manifest のない project は `None` |
| lexer・parser の入口の edition を必須にする。`format_source` にも足す | 足した。ただし `formatter::format_source` は 3 引数のまま（`LATEST`、manifest のない入口）で、`format_source_with_edition` を足した。約 58 か所のテストの呼び出しの変更を避けるため。`tsuzuri fmt` と LSP は後者（manifest の edition）を使う |
| `Edition::LATEST` の使用箇所は旧計画の一覧 | 一覧に加えて、`bindgen`（`plain_identifier`）、`repl`、`folder_namespace`、`create_project`（雛形）、`formatter::format_source`（manifest のない入口）。manifest のない入口と std だけに限る方針は変えていない |
| 例の graph の名前は `LegacyLib.Old.old` | 現在の名前空間の規則（D-35・D-37）で `LegacyLib::Old.old`。診断も完全修飾名で出す |
| `deprecation_note` の疑似コード（`?` を `Option` の外で使う） | `find_map` で書き直した。タブも空白として受ける。案内の長さの上限（200 文字）は追加 |
| 診断の表の未知キーのメッセージは `expected name, version, or edition` | HEAD の manifest は `namespace` も受けるので `expected name, version, namespace, or edition` |
| Phase 2 は `Project::load_for_docs` で 2 つの入力を読む | `Project::load_for_api`。`load_for_docs` は型検査を伴い、git・registry の依存と `Tsuzuri.lock` を要る。api-diff は構文解析だけで、依存を取得せず、型エラーのある版も比べられる |
| `tsuzuri publish` は api-diff を通す（E10 の計画） | 通さない。`publish` は index を読まず、前の版のソースを取得する経路がない（index の参照と 2 回目のネットワーク取得と store への書き込みが要る）。api-diff は署名の比較で、本体の挙動は見つけられない助言なので、公開を止める条件にしない。手順は `git worktree` で前の版を取り出して `tsuzuri api-diff <前の版> .` と書いた。E10 の完了記録の「`publish` は検査しない」という注記と整合する |
| `_docs/...` の文書、`_docs/feature-status.md` | `_tsuzuri/language-reference/` の該当ページ（`packages.md`、`documentation-comment.md`、`diagnostics.md`、`usage.md`、`option.md`、`keywords.md`、`index.md`、`languages/strategy.md` の表）。`feature-status.md` は存在しない |
| `tsuzuri new` の変更は計画にない | 雛形の `Tsuzuri.toml` に `edition = "2026"` を足した（新しいプロジェクトが edition を明示する）。VS Code 拡張の README の表も直した |

## 受け入れ条件

- manifest の `edition = "2026"` を受け、省略時は 2026、manifest のない入力と std は `Edition::LATEST`。未知・未対応の値は E0002（`tests/editions.rs`、`src/package.rs` の単体テスト）。
- 全ソースが自分の edition で字句解析され、`EDITION_KEYWORDS` の語は予約を始める edition より古いソースで識別子になる（`src/lexer.rs` の合成表による単体テストと `tests/editions.rs`）。同じ byte 列でも edition が違えば frontend cache の別の entry（`src/frontend_cache.rs`）。
- `grep -n "Edition::LATEST" src/*.rs` の製品コードの使用箇所は、`src/syntax.rs` の定義、`parser::parse`、`src/lib.rs` の manifest のない入口、`src/driver.rs`（std・manifest のない入力・`folder_namespace`）、`src/check.rs` の単一モジュールの入口、`bindgen`・`repl`・`formatter::format_source`・`create_project` だけ（残りは各ファイルのテスト）。
- 別 package と std の非推奨の `def`・`record`・`union` の使用が W1005 になり、同じ package の使用はならない。`--deny-warnings` で失敗する。cache の有無で出力は同じ（`tests/warnings.rs`、`tests/lsp.rs`）。
- 使える印のない project は `semantic::collect` を追加で呼ばない（`src/warnings.rs` の単体テストが呼び出しを数える）。
- 例の graph の IR が native・wasm32 × `-O0`・`-O3` でベースライン（`053bced8`）と byte 単位で一致し、object も一致する。WASM の import は増えない。
- `docs/stability.md` があり、`tsuzuri api-diff` の出力と判定の規則は `tests/api_diff.rs` が検査する。
- GUIDE §10 の完了の定義（下の検証結果）。

## 実装と検証（2026-10-11）

### 実装したファイル

- `src/syntax.rs`（`Edition`、`Documentation::deprecation`）、`src/lexer.rs`（`EDITION_KEYWORDS`、`reserved_in`）、`src/parser.rs`、`src/package.rs`（`Manifest::edition`、`valid_namespace`、雛形）、`src/driver.rs`（`SourceFile::edition`、`read_manifest`、`format_edition`、`Project::root_package`・`load_for_api`・`dependency_package`）、`src/lib.rs`（`SourceInput`）、`src/check.rs`（`ModuleInput`、W1005 の呼び出し）、`src/warnings.rs`（`deprecations`、`deprecated_uses`）、`src/semantic.rs`（テストの計数器）、`src/frontend_cache.rs`（parse key、`FRONTEND_FORMAT` 3）、`src/formatter.rs`、`src/lsp.rs`、`src/docgen.rs`（描画関数の公開）、`src/bindgen.rs`、`src/repl.rs`、`src/stdlib.rs`、`src/syntax_codec.rs`（テストの呼び出し）、`src/main.rs`（`api-diff`）。
- `src/api_diff.rs`（新規）。
- `tests/editions.rs`（新規）、`tests/api_diff.rs`（新規）、`tests/warnings.rs`、`tests/lsp.rs`（W1005 の publishDiagnostics）、機械的な更新（`tests/chars.rs`・`diagnostics.rs`・`lists.rs`・`strings.rs`・`string_interpolation.rs`・`formatter.rs`・`generic_records.rs`）。
- `docs/stability.md`（新規）、`docs/language.md`、`docs/architecture.md`、`README.md`、`_tsuzuri/language-reference/` の `organizing-tsuzuri/packages.md`・`documentation-comment.md`、`compiler/usage.md`・`diagnostics.md`・`option.md`、`values-and-functions/keywords.md`、`built-in-types-and-modules/net.md`、`languages/strategy.md`、`index.md`、`vsc/README.md`。

### 検証結果

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（rustc 1.98.1）、`sh scripts/check-runtime-includes.sh`（43 ファイル）が成功。`cargo check --locked --all-targets --target x86_64-pc-windows-msvc` と `aarch64-pc-windows-msvc`（rustup の stable 1.96.1）が成功（Windows では実行していない）。同じ toolchain の clippy は、この変更が触れていない `src/lsp.rs:820` と `src/parser.rs:2147` に `nonminimal_bool` を 3 件出す（既存のコードで、clippy 1.96 の lint）。
- `cargo test --locked --no-fail-fast -- --skip bounds_computation_syntax_and_expansion` は、95 の結果（lib、統合テスト、doc-test）で **1,119 件成功**、失敗 0、無視 0。`RUST_MIN_STACK=2200000 cargo test --locked --test computations bounds_computation_syntax_and_expansion` が成功し、この test が通る最小の `RUST_MIN_STACK` は、ベースライン（`053bced8` を `git archive` したもの）も本ブランチも約 2,132,016（4,000 バイトの刻みの二分探索）で、parser・checker の stack の使用は変わらない。GUIDE §3.1 の 4 つの回帰テストは各 1 件（`running 1 test`）で成功。
- 新しいテスト: `tests/editions.rs` 7 件、`tests/api_diff.rs` 20 件、`tests/warnings.rs` は 7 件から 14 件、`tests/lsp.rs` に W1005 の publishDiagnostics、`src/lexer.rs`・`src/package.rs`・`src/frontend_cache.rs`・`src/warnings.rs` の単体テスト。`src/warnings.rs` の単体テストは `semantic::collect` の呼び出し回数を数え、使える印のない project（`@deprecated` なし、同じパッケージだけの印、root パッケージの印）では増えず、std か依存パッケージの印があるときだけ 1 回増えることを確かめる。
- Node の E2E は `cargo build --release --locked` の直後に 16 suite（`e2e`、`examples`、`cache`、`frontend_cache`、`packages`、`lsp_sessions`、`docgen`、`repl`、`script`、`control`、`computations`、`strings`、`io`、`json`、`primitives`、`features`（13,647 ケース））がすべて成功（native と WASM の `-O0`・`-O3` を含む suite）。
- IR の一致（ベースラインは `053bced8` の release）: 例の graph（`@deprecated` を持つ）の `--emit llvm` は native・wasm32 × `-O0`・`-O3` の 4 構成で、`--emit object` は `-O0`・`-O3` で byte 単位で一致。`examples/` と `tests/fixtures/` の 98 プロジェクト × native・wasm32 × `-O0`・`-O3` の 392 構成は、358 構成の IR が一致し、34 構成は同じ診断で失敗し、差は 0。`export def` を持つ fixture の wasm32 `--emit wasm` 168 ビルドも byte 単位で一致（WASM の import は、fixture 自身のホスト関数と IO のものだけで、ベースラインと同じ）。`edition = "2026"` の有無で IR が変わらないことは `the_edition_key_changes_no_generated_code`（native・wasm32）が検査する。
- 文書: 変更した 9 ページで `node scripts/check-docs.mjs` が成功（307 リンク、19 例、30 回の native 実行、2 つのテストプロジェクト）。`docs/` と `README.md` の相対リンクと見出しも、同じ規則で確かめた（既存の `#利用者定義の解放` の 3 件を除く）。

### レビュー後の修正

独立したコードレビューの 4 件を 1 つのコミットで直した。

- api-diff の `flatten` が、引数のない関数型（`fn() -> T`）の結果をすべて引数へ取り込み、`i64 -> i64` と `i64 -> fn() -> i64`、単項の `def` と `fn() -> (i64 -> i64)` を同じ署名にしていた（互換性のない変更を 0 件と報告）。parser と同じ規則（引数のある定義の、引数のある関数型の結果だけを取り込む）に直し、`a_thunk_is_not_an_extra_parameter`（定義と class メソッド）を足した。
- `Union.Case` が型名と case の 2 参照として 2 件の W1005 になっていた。case の位置の 1 件にした（`a_union_named_before_its_case_warns_once_at_the_case`）。
- 使えない印（root パッケージの `@deprecated`）が `semantic::collect` を走らせていた（停止条件の趣旨）。`SourceInput::package` を依存パッケージの `PackageId` に改め、root の印は集めない（単体テストの計数と `a_tag_in_the_root_package_never_warns`）。
- pattern の中の型注釈が警告にならない指摘は、索引が記録しない種類として文書に書いた（旧 D5 の「pattern の中は対象外」の範囲）。

### 文書と計測

更新したページは「実装したファイル」の文書のとおり。`@deprecated`・W1005・api-diff の出力の例は、実際に実行した結果（例の graph の W1005 5 件、`api-diff` のテキストと JSON）を載せた。`docs/benchmarks.md` は更新していない（性能の主張を足していない）。

計測（`check` の費用）: `@deprecated` を含まない 20,000 関数・41 ファイルの project（生成物は ignored の scratch。キャッシュなし）を、ベースラインと本ブランチの release で 9 回ずつ交互に `check` した中央値は、user+sys が 0.955 秒（本ブランチ）と 0.957 秒（ベースライン）、wall が 1.027 秒と 1.000 秒で、差は測れない。条件は Apple M1 Max、macOS 27、共有の機械で load average は 16〜40。W1005 の追加の解析は、使える印がある project だけが払う構造なので、この結果は構造の確認であり、速さの主張ではない。

## 既知の限界

- W1005 は、pattern の中の型注釈（`match` の腕の `| q: T ->`、ラムダの `\(p: T)`、`for` のパターン）に書いた非推奨の型を警告しない。型エイリアスを経由した使用は、エイリアスの定義の 1 件だけ。
- api-diff は宣言の署名だけを比べる。関数の本体の挙動、性能、`const` の値の変更は見つけられず、型エイリアスの展開は解決しない（エイリアスの置き換えは変更として出る）。モジュールの改名は削除と追加で、改名は検出しない。`publish` には組み込んでいない。
- edition は `2026` だけで、差がまだない。`EDITION_KEYWORDS` は空で、予約語を識別子として書く構文（raw identifier）はない。LSP の補完のキーワードと VS Code 拡張の構文ハイライトは edition を区別しない。`tsuzuri fmt` は上位のディレクトリの manifest を探さない。
- マニフェストの診断の行・列は、既存どおり `1:1`（`span` の `start`・`end` は正しい）。
- Windows は `cargo check --all-targets --target x86_64-pc-windows-msvc` と `aarch64-pc-windows-msvc` の型検査だけで、実行していない。シンボリックリンクの api-diff のテストは Unix だけ。
