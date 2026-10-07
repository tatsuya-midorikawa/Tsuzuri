# 実装チケット共通ガイド

このガイドは `_features/` の全チケット（完了済みは `_features/_completed/`）と `_perfs/` の性能チケットに共通する前提・手順・設計決定をまとめたものです。
**チケットに着手する前に、このファイル全体と対象チケット、`AGENTS.md`、`docs/language.md`、
`docs/architecture.md` の関連節を必ず読んでください。** チケットとこのガイドが矛盾する場合は、
このガイドの「9. 設計決定台帳」を優先し、矛盾をチケットの「決定事項」（旧いチケットでは「未決事項」）に追記して人間に報告します。

調査時点のコミットは `19d8cdd`（2026-09-23）です。行番号は変わりやすいため、コード参照は
「ファイル + 関数名／型名」で書いています。見つからない場合は `grep` で関数名を探してください。
第2期（A12–G20）と `_perfs/` のチケットは 2026-09-29 に HEAD `f8dc655` で実装担当者向けに詳細化し、このガイドに
§3.1（テスト選択）・§4.3（P2 以降のモジュール）・§6.9・§7.4（suite の追加）・§11.1（既知の落とし穴）・§12（構文）・
§13（停止条件と完了報告）・§14（性能チケットの共通手順）を足しました。

---

## 0. チケットの使い方（実装担当モデル向け）

1. 対象チケットの「依存」がすべて完了しているか `_features/README.md`（性能チケットは `_perfs/README.md`）の状態欄で確認する。
   未完了の依存があれば着手しない（先に依存チケットを実装する）。メタ情報の「承認」行と「決定事項」で `要承認` の項目は、
   人間の承認が記録されるまで該当する Phase／段に着手しない。承認済みか分からなければ人間に尋ねる。
2. 「現状」節に書かれたコードを実際に開き、記述が今のコードと一致するか確認する。
   一致しない場合は、チケットの意図を保ったまま現在のコードに合わせる（差分をチケット末尾に追記）。
   **P2／P3 のチケットは独立レビューを受けていないため、着手前にその時点のコードと台帳に対してレビューを行い、
   指摘を反映してから実装する**（README の「レビュー状況」参照）。
3. 「実装手順」を **上から一段ずつ** 実施し、各段の「確認」に書かれたテストを通してから次へ進む。
   複数段をまとめて書いてから一度にテストしない。
4. 仕様に書かれていない挙動を推測で追加しない。必要なら「決定事項」（旧いチケットでは「未決事項」の既定案）に従い、
   該当する決定がなければ最も保守的な挙動（コンパイルエラーで拒否）を選ぶ。チケットの「停止条件」と §13 に
   当てはまったら、即興で回避せず作業を止めて報告する。
5. 完了したら「受け入れ条件」の全項目を確認し、`_features/README.md` の状態を `done` に更新する。
   チケットは `_features/_completed/` へ移動し、README のリンクを更新する（性能チケットは `_perfs/_completed/` と `_perfs/README.md`）。
   `_tsuzuri/language-reference/` は §8 のとおり、機能を変えた PR ごとに最新にする。完了時には、このチケットを指す「計画中」の注記や
   リンクが残っていないかも確かめる。最後に §13 の書式で完了報告を書く。
6. チケット中の `/tmp/tz-*` のパスは再現用の一時ディレクトリの例である。記載のソースから作り直して使い、リポジトリには置かない。
   「検証済み」と書かれたサンプルも、着手時の HEAD で `tsuzuri check` し直してから使う。
7. チケット中の `_docs/...` のパス（更新する文書や `check-docs.mjs` の引数）は旧構成である。§8.1 の表で
   `_tsuzuri/language-reference/` のページに読み替える。

### チケットの共通構成

| 節 | 内容 |
| --- | --- |
| メタ情報 | ID、優先度（P0–P3）、規模（S/M/L/XL）、依存、後続、状態、承認（`不要` か `要承認: D<n>…`）、手本にする既存実装、影響ファイル |
| 目的 | なぜ必要か、利用者にとっての価値 |
| 着手条件と停止条件 | 依存・承認・基準コマンド（§2.3）と、即興で回避せず止めて報告する条件（§13 に加えるチケット固有の条件） |
| 現状 | 現在の実装・仕様・制約（コード参照付き、再現コマンド付き） |
| 仕様 | 構文（EBNF 風）、型規則、評価順序、所有権・借用、数値の意味、診断（コード・条件・英語メッセージ・位置）、native/WASM の差、資源上限、例 |
| 設計 | データ構造の変更、段ごとの変更表（ファイル・関数・変更内容）、アルゴリズム、生成 IR の形 |
| 実装手順 | 小さく検証可能な段階。各段に「変更」「内容」「確認」（正確なコマンドと期待結果） |
| テスト計画 | Rust テスト（受理／拒否と診断コード）、fixture、Node E2E（native/WASM × -O0/-O3）、解放追跡、既存テストへの影響、性能測定 |
| ドキュメント | 更新すべき `_tsuzuri/language-reference/` のページと、README／docs の節（§8） |
| 受け入れ条件 | チェックリスト |
| 落とし穴 | 間違えやすい点と検出方法 |
| 対象外 | このチケットでやらないこと |
| 決定事項 | 判断の結果（`D<n>`: 決定・理由・状態）。状態は「既定案（実装者はこの案に従う）」か「要承認（承認前は該当する Phase に着手しない）」。完了済みの古いチケットでは「未決事項」 |

性能チケット（`_perfs/`）は「現状と計測」「目標と指標」「変えてはいけない意味」「計測手順」「生成コードの確認」を加えた構成で、
共通の計測手順は §14 にまとめています。

規模の目安: S = 1〜2 日、M = 3〜5 日、L = 1〜3 週、XL = 1 か月以上または複数チケットへの分割が前提。
XL と大きい L のチケットは Phase に分かれています。**人間が求めない限り Phase 1 だけを実装します。**

---

## 1. 絶対に守る規則

`AGENTS.md` と `docs/architecture.md`「性能設計の原則」の要約です。違反する実装は受け入れません。

1. **言語の意味を変える最適化をしない。** 整数の折り返し、飽和、最近接・偶数丸め、NaN、符号付きゼロ、
   評価順序（左から右）、短絡評価、トラップ、所有権、借用を保つ。fast-math、再結合、暗黙の FMA、
   二重丸め、未サポート命令の無条件使用を禁止する。
2. **暗黙の変換・既定値・読み飛ばしをしない。** 未対応の構文や型は、別の意味に置き換えずコンパイルエラーにする。
3. **型付き LLVM 命令・intrinsic を優先する。** 同じ意味の組み込み関数と演算子が別の性能経路を選ばないようにする
   （例: `Add.add` と `+`、`to_int` と `as i64`）。
4. **移植可能な正しい経路を常に残す。** ハードウェア依存の経路は能力確認と文書化されたフォールバックを持つ。
   明示的に要求されたバックエンドが使えない場合はエラーにする（黙って別経路にしない）。
5. **生成 IR は決定的にする。** 名前集合は `BTreeMap`／`BTreeSet`。intrinsic 宣言は重複させない
   （`FunctionEmitter` の `intrinsics: BTreeSet<String>` に登録する）。
6. **WASM は既定でインポートなし。** インポートが必要な機能は明示的なオプトインにし、文書化する。
7. **性能主張には実測と生成コード確認を伴う。** 共有 CI に速度の合否閾値を入れない。
   「実装済み」と「計画」を区別して文書に書く。
8. **資源上限を守る。** 構文・式の深さ 128（`syntax::MAX_NESTING`）、特殊化 1,024、制約 128、
   OR 展開 1,024 など既存上限に揃え、新しい解析にも上限と `E1017` を設ける。
9. **型付き IR の不変条件は型検査で保証する。** コード生成時のキャストや `unreachable!` で辻褄を合わせない。
10. `unsafe` は `Cargo.toml` の lint で禁止（`unsafe_code = "forbid"`）。Rust の依存 crate 追加は、
    チケットに明記された場合だけ行う。
11. **言語リファレンスを同じ変更で最新にする。** 機能を追加・変更・修正・削除したら、同じ PR で
    `_tsuzuri/language-reference/` の関連ページを直し、`node scripts/check-docs.mjs <変更したページ>` を通す（§8）。

---

## 2. 作業開始前の準備

### 2.1 ツールチェーン

- Rust 1.85 以降（edition 2024）、LLVM/Clang 17 以降、`wasm-ld`、Node.js 20 以降、Python 3.9 以降。
- macOS: `brew install llvm lld` の後 `export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"`。
- `TSUZURI_CLANG`／`TSUZURI_WASM_LD` で実行ファイルを指定できる。
- 2026-09-29 の詳細化で使った作業機: Apple M1 Max（10 コア）、macOS、Apple clang 21.0.0、rustc 1.98.1、Node v20.19.6。
  LLVM 21 の単体ツールは `/opt/homebrew/opt/llvm@21/bin/`（`llvm-objdump`・`llvm-link`・`llvm-dwarfdump`・`llvm-profdata` など）。
  Go・Zig は入っていない。x86_64 の実行ファイルは起動できない（Rosetta 2 なし。x86 は cross-compile だけ確認できる）。
- テストが読む主な環境変数: `TSUZURI_OBJDUMP`（`llvm-objdump`）、`TSUZURI_LLVM_LINK`・`TSUZURI_DWARFDUMP`（`tests/debug_info.mjs`）、
  `TSUZURI_MATH_PYTHON`（`tests/math.mjs`）、`TSUZURI_ASAN`・`TSUZURI_TSAN`（sanitizer 付きの harness）、`TSUZURI_WEBGPU=1`（実 GPU）、
  `TSUZURI_TEST_WASM_TARGET=wasm64`（`tests/features.mjs` の WASM を memory64 で実行。Node 24 が必要）。
- BigInt を多用する suite は Node 20 の V8 で異常終了することがある（§11.1）。その場合は Node 24 で実行する:
  `npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri`。

### 2.2 ランタイムの追跡と生成

`src/runtime/` の埋め込みファイルはすべて Git で追跡している（G01 で解決済み）。ただし `.gitignore` は `*.ll` を無視し、
追跡する `.ll` だけを `!src/runtime/<name>.ll` の例外行で戻している。

- 新しいランタイム `.ll` を足すチケットは、`.gitignore` に例外行を足し、`sh scripts/check-runtime-includes.sh` で
  「runtime includes are tracked: N files」の N が増えたことを確かめる（`include_str!` した全ファイルの存在・追跡・非無視を検査する）。
- `numeric.ll` と `math.ll` は生成物で、**手で直さない**。`numeric.c` を変えたら両方をこの順で作り直す（metadata の番号の範囲が
  連動するため）:
  `TSUZURI_CLANG=/usr/bin/clang python3 src/runtime/generate.py` の後に
  `TSUZURI_CLANG=/usr/bin/clang TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link python3 src/runtime/generate_math.py`。
  Apple Clang 21 と llvm-link 21 の組だけを使う（LLVM 22／23 は lifetime の署名と浮動小数点の定数表記が変わり、生成 IR が大きく変わる）。
- `Globals::FIRST_METADATA` は両ランタイムの metadata の範囲より上に保つ（重なると trap-info が math の metadata を消す）。

### 2.3 ベースラインの確認

```sh
cargo build --release --locked
cargo test --locked
```

変更前にテストが通ることを確認し、失敗があれば自分の変更と無関係であることを記録してから着手します。

---

## 3. 検証コマンド

小さい範囲から実行し、最後に全体を実行します。

```sh
# 最小: 変更した Rust テストだけ（--test <ファイル名> を使う）
cargo test --locked --test <ファイル名から .rs を除いたもの>
cargo test --locked <テスト関数名の一部>
```

> **注意:** 名前フィルターだけの `cargo test --locked <pattern>` は、一致するテストが 0 件でも成功します。
> 出力の `running N tests` の N が 0 でないことを必ず確認してください。
> Node の E2E は `target/release/tsuzuri` を使うため、**Node テストの直前に毎回 `cargo build --release --locked`** を実行し、
> 古いバイナリで検証しないでください。

```sh
# 形式・lint
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# 全 Rust テスト（LLVM 不要）
cargo test --locked

# E2E（LLVM/Clang/LLD/Node が必要。release ビルドを使う）
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/tasks.mjs target/release/tsuzuri
node tests/computations.mjs target/release/tsuzuri
node tests/control.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri

# 追加のメモリ検査（ASan が使える環境）
TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/control.mjs target/release/tsuzuri
TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/computations.mjs target/release/tsuzuri
```

性能測定（合否条件にしない）: `node benchmarks/run.mjs`、`run-cpp.mjs`、`run-computations.mjs`、`run-control.mjs`
（いずれも第 1 引数に `target/release/tsuzuri`）。手順と限界は `docs/benchmarks.md`。

手元での動作確認は、リポジトリ外の一時ディレクトリで行います。

```sh
mkdir -p /tmp/tz-try && cat > /tmp/tz-try/Main.tz <<'EOF'
let x = 40
x + 2
EOF
target/release/tsuzuri run /tmp/tz-try
target/release/tsuzuri build /tmp/tz-try --emit llvm -O0 -o /tmp/tz-try/Main.ll
```

### 3.1 テスト選択表と必ず実行する回帰テスト

`cargo test --locked <pattern>` は一致するテストが 0 件でも成功する。**出力の `running N tests` の N が 0 でなく、期待した数であることを毎回見る。**
Node の E2E は `target/release/tsuzuri` を使うので、直前に `cargo build --release --locked` を実行する。

どの変更でも、最後に次の回帰テストを実行する。上限や stack の大きさを上げて通してはいけない（§11.1）。

```sh
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

| 変更の種類 | Rust テスト（手本のファイル） | E2E（`node tests/<名前>.mjs target/release/tsuzuri`） |
| --- | --- | --- |
| 字句・構文・予約語 | `tests/frontend.rs`、`tests/diagnostics.rs`、`src/parser.rs` の test module、`tests/formatter.rs` | `examples.mjs`、`features.mjs`（関係する suite） |
| 型・型クラス・多相 | `tests/polymorphism.rs`、`tests/higher_kinds.rs`、`tests/deriving.rs`、`tests/union_types.rs` | `features.mjs typeclasses`・`deriving`・`higher_kinds` |
| 所有権・借用・region | `tests/types_ownership.rs`、`tests/borrowed_records.rs`、`tests/slices.rs` | `features.mjs borrowed_records`、`control.mjs` |
| 制御構文・パターン | `tests/control.rs`、`tests/match_exhaustiveness.rs` | `control.mjs`、`features.mjs` |
| 計算式・IO | `tests/computations.rs` | `computations.mjs`、`io.mjs` |
| コレクション・std | `tests/stdlib.rs`、`tests/map_set.rs`、`tests/vec.rs`、`tests/iteration_protocol.rs` | `features.mjs map_set`・`iteration_protocol`・`vec` |
| 文字列・文字・表示と解析 | `tests/strings.rs`、`tests/chars.rs`、`tests/display_parse.rs` | `strings.mjs`、`display_parse.mjs` |
| 数値・数学 | `tests/integers.rs`、`tests/math.rs` | `numeric_casts.mjs`、`integer_intrinsics.mjs`、`math.mjs`（重い。Node 24） |
| 生成 IR・ランタイム・閉包 | 変更した機能の Rust テスト、IR の決定性の確認 | `primitives.mjs`、`e2e.mjs`、`features.mjs`（全 suite） |
| Task・並列・WASM threads | `tests/tasks.rs`、`tests/parallel.rs` | `tasks.mjs`、`wasm_threads.mjs`、`TSUZURI_TSAN=1` の実行 |
| SIMD・CPU dispatch・GPU | `tests/simd.rs`、`tests/cpu_dispatch.rs`、`tests/gpu.rs` | `simd.mjs`、`wasm_simd.mjs`、`cpu_dispatch.mjs`、`gpu.mjs` |
| host ABI・import | `tests/host_abi.rs`、`tests/host_imports.rs` | `host_imports.mjs` |
| driver・CLI・cache | `src/main.rs` の test module、`tests/modules.rs` | `cache.mjs`、`examples.mjs` |
| 診断・警告 | `tests/diagnostics.rs`、`tests/warnings.rs` | — |
| LSP・docgen・debug | `tests/lsp.rs`、`tests/docs.rs`、`tests/debug_info.rs` | `lsp_sessions.mjs`、`docgen.mjs`、`debug_info.mjs` |
| 言語内テスト | `tests/test_runner.rs` | `tsuzuri test` を使う E2E |
| `_tsuzuri/` の文書 | — | `node scripts/check-docs.mjs <変更したページ>`（引数なしで全体） |

表にない組み合わせは、変更した関数名を `grep -rn "<関数名>" tests/` して、それを使うテストを全部実行する。
最終確認は §3 の全体（fmt・clippy・`cargo test --locked`・関係する E2E）と `sh scripts/check-runtime-includes.sh`。

---

## 4. パイプラインとコードマップ

```text
driver (src/driver.rs)        同じディレクトリの .tz/.tt/.tc をファイル名順に読み込み、Main.tz を選ぶ
  -> lexer (src/lexer.rs)      Token 列。キーワード表は Lexer::identifier
  -> parser (src/parser.rs, src/parse_control.rs)   Program（syntax.rs の AST）
  -> check (src/check.rs)      check_modules: 名前収集 → レコード → クラス/インスタンス → シグネチャ → 本体の型検査
       computation.rs           型検査前に .tc ビルダー式を通常の呼び出しへ展開（computation::expand）
       control.rs               for/while/match/パターン → 型付き IR（PatternStep）
       polymorph.rs             Inference（単一化）、Classes（型クラス）、specialize（単相化）
       recursion.rs             rec 指定の検査（参照グラフの SCC）
       closures.rs              lambda の型検査、捕捉、lambda lifting（closures::lower）
  -> ownership (src/ownership.rs, src/ownership_control.rs)   move/loan/寿命の検査（単相化の前後で実行）
  -> llvm (src/llvm.rs)        FunctionEmitter による決定的な LLVM IR
       llvm_control.rs          ループ・match・switch・定数表
       llvm_frame.rs            new を使わないリテラルのスタック確保（Frame）と relocate
       call_specialization.rs   既知の非 escaping 関数引数の worker 特殊化
       src/runtime/*.ll, *.c     文字列・ヒープ・クロージャ・数値・タスクのランタイム（条件付きで連結）
  -> driver: clang -O0..3 → native 実行ファイル／オブジェクト、または wasm32 オブジェクト → wasm-ld
```

入口: `src/lib.rs` の `analyze(source)`（単一ファイル、モジュール名 `Main`）と
`analyze_modules(&[("Name.tz", source), ...])`（拡張子でソース種別を決める）。
`check::check_modules` の最後で `polymorph::specialize` → `closures::lower` → `ownership::check` を実行します。
`src/main.rs` は CLI（`check`／`build`／`run`、`--json` など）、`src/driver.rs` は Clang/LLD の起動と出力保護です。

### 4.1 ファイル別の主な関数

| ファイル | 主な型・関数 | 役割 |
|---|---|---|
| `src/syntax.rs` | `TokenKind`、`Program`、`RecordDecl`、`FunctionDecl`、`SignatureDecl`、`ClassDecl`、`InstanceDecl`、`TypeExpr`/`TypeExprKind`、`Expr`/`ExprKind`、`Pattern`/`PatternKind`、`MatchArm`、`Binding`、`ComputationBlock` | 構文木。`MAX_NESTING = 128`、`MAX_SOURCE_BYTES = 1 MiB` |
| `src/lexer.rs` | `lex`、`Lexer::identifier`（キーワード表）、`number`、`string`、`symbol`、`comment` | 字句解析。コメントは捨てる |
| `src/parser.rs` | `Parser::program`（トップレベル宣言のループ）、`signature`、`definition`、`type_expr`、`type_product`、`type_atom`、`expression_inner`（Pratt）、`application`、`postfix`、`primary`、`record`、`new_collection`、`block`、`task`、`computation*`、`binary`（優先順位表） | 構文解析 |
| `src/parse_control.rs` | `for_source`、`make_match`、`match_arms`、`pattern_atom`、`named_pattern`、`record_pattern`、`collection_pattern` | 制御構文・パターン |
| `src/check.rs` | `Type`（`is_copy`、`needs_drop`、`contains_reference`、`carries_loans`、`can_capture`、`can_send`、`exportable`、`display`）、`Builtin`（`ALL`、`name`、`signature`）、`CheckedModule`、`CheckedRecord`、`CheckedFunction`、`TypedExpr`/`TypedExprKind`（`children`、`children_mut`）、`Names`（`record`）、`check_modules`、`resolve_type`、`record_size`、`layout_size`、`validate_size`、`Checker`（`expression`、`composed_expression`、`value_expression`、`name`、`binary_type`、`integer`） | 名前解決・型検査の中心 |
| `src/polymorph.rs` | `Constraint`、`Scheme`、`Inference`（`fresh`、`resolve`、`unify`、`default_numeric`）、`Classes`（`collect`、`resolve`、`inline_constraints`、`instances`、`method`、`intrinsic`、`validate`）、`map_type`、`substitute`、`bounded_type`、`require_concrete`、`type_expression`、`Checker::{builtin, function, method, require, annotation, call_signature, finish}`、`specialize`、`Specializer::{request, instantiate, lower, operator_call, intrinsic_function}` | 型推論・型クラス・単相化 |
| `src/control.rs` | `Checker::control_expression`、`match_value`、`pattern_alternatives`、`active_pattern`、`length_test`、`combine` | 制御構文とパターンの型付け。パターンは `PatternStep::Test`（bool 式）と `Bind` の列へ展開 |
| `src/closures.rs` | `Checker::lambda`、`free_locals`、`lower`、`lower_expression` | 匿名関数と lambda lifting |
| `src/computation.rs` | `OPERATIONS`、`collect`、`expand`、`Lowering::{block, continuation, delay, call}` | ビルダー式の展開 |
| `src/recursion.rs` | `check`、`references` | `rec` 検査 |
| `src/ownership.rs` | `check`、`infer_copy`、`check_body`、`closed_returns`、`Checker::{access, place, read_place(s), eval, eval_composed, eval_value, merge}`、`count_uses`、`uses` | 所有権と借用 |
| `src/ownership_control.rs` | `eval_control`、`eval_match`、`check_loop`、`protect`、`alias_source` | ループの固定点、ガードの別名 |
| `src/llvm.rs` | `emit_target`（モジュール全体の出力とランタイムの条件付き連結）、`header`、`llvm_type`、`c_type`、`abi_type`、`validate_main`、`FunctionEmitter::{emit, expression, expression_mode, place, read_place, drop_value, clone_value, allocate_array, call, apply_value, make_closure, binary, cast, short_circuit, tail, tail_arguments, bind, slot, drop_scope}`、`export_wrapper`、`emit_builtin`、`console_main` | LLVM IR 生成 |
| `src/llvm_control.rs` | `while_loop`、`range_loop`、`for_each`、`match_expression`、`switch_cases`、`lookup_match` | 制御構文の IR |
| `src/llvm_frame.rs` | `Frame`、`stack_size`、`frame_bytes`、`frame_value`、`relocate`、`heap_copy`、`drop_framed` | スタック上のリテラル |
| `src/call_specialization.rs` | `Specializations`、`target`、`transparent`、`single_use_locals`、`may_mutate`、`non_escaping`、`read_only`、`all_children` | 継続の特殊化 |
| `src/numeric.rs` | `primitive`（型名 → `Type`）、接尾辞、binary/decimal リテラルの丸め | 数値 |
| `src/driver.rs` | `Project::load`、`build`、`run`、`native_cpu_flag`、`protect_sources` | ツール起動 |
| `src/main.rs` | CLI の解析と診断表示 | CLI |
| `src/diagnostic.rs` | `Span`（`new`、`in_source`、`through`）、`Diagnostic::new(code, message, span)`、`render`、`json` | 診断 |

### 4.2 ランタイム

| ファイル | 内容 | 連結条件（`emit_target`） |
|---|---|---|
| `src/runtime/string.ll` | `@tz.string.copy/allocate/new/concat/equal` | IR に `@tz.string.`／`@tz.free`／`@tz.alloc` が現れる |
| `src/runtime/heap-native.ll` / `heap-wasm.ll` | `@tz.alloc`／`@tz.free`（native は malloc/free、WASM は空き領域リスト、上限 16 MiB） | 同上 |
| `src/runtime/closure.ll` | `@tz.closure.clone/drop` | `@tz.closure.` が現れる |
| `src/runtime/numeric.c` → `numeric.ll` | f16/f128/decimal/i128 変換のソフトウェア実装、`tz_soft_op/cmp/cast/format` | `@tz_soft_` が現れる。**`numeric.ll` は手編集禁止。`python3 src/runtime/generate.py` で再生成** |
| `src/runtime/console.ll` | `@tz.console.write`（putchar） | ネイティブのコンソール入口だけ |
| `src/runtime/task.c` / `task-wasm.ll` | `tsuzuri_task_parallel`（pthread の bounded fork/join／WASM 逐次） | `@tsuzuri_task_parallel(` が現れる。native は driver が task.c をコンパイル |
| `src/runtime/trap.c` | `--trap-mode return` の境界（`tsuzuri_boundary_run/item/resume`、thread ごとの setjmp frame）、`tsuzuri_trap_raise`、確保の追跡（`tsuzuri_tracked_malloc/free/realloc`） | `--trap-mode return` の native object。driver が trap.c をコンパイルし、`heap-native.ll` の確保名を `tsuzuri_tracked_*` に置き換える |
| `src/runtime/stack.c` | スタック溢れの signal handler（`sigaltstack`、`trap: stack overflow` を書いて `abort()`） | native の実行ファイルで、再帰するプログラム（`llvm::has_recursion`）だけ。`-g` でなければ driver が同じ clang 呼び出しに `-x c` で足す |
| `src/runtime/wasm.ll` | 128-bit 乗除算・シフトの補助（`__multi3` は `noinline optnone` 必須） | WASM で driver が常に追加 |

### 4.3 P2 以降に追加されたモジュール

4.1 と 4.2 は P1 の時点の地図です。HEAD `f8dc655` の `src/` には次のモジュールもあります（括弧は導入したチケット）。
詳細化した第2期・性能チケットの「段ごとの変更」表は、関数名を HEAD で grep 確認しています。着手時にもう一度 grep してください。

| ファイル | 役割 |
| --- | --- |
| `src/abi.rs`、`src/llvm_abi.rs` | host ABI の型の対応、公開 wrapper と C header（E05） |
| `src/cache.rs` | 全ビルドの cache。SHA-256、metadata、writer lock、GC（G11） |
| `src/constants.rs` | 定数の評価と型付きリテラルへの展開（D06） |
| `src/derive.rs` | `deriving` の instance を通常の AST として生成（A07） |
| `src/docgen.rs` | API 文書の生成（G09） |
| `src/exhaustiveness.rs` | match の網羅性検査（A03） |
| `src/formatter.rs` | `tsuzuri fmt`（`format_source`、`SourceKind`） |
| `src/gpu.rs` | GPU kernel の抽出と WGSL 生成（F07） |
| `src/higher_kinds.rs` | 高カインド型の正規化（A10） |
| `src/llvm_bulk.rs` | 配列の一括 API（C04） |
| `src/llvm_compare.rs`、`src/llvm_hash.rs`、`src/llvm_display.rs` | 構造比較・構造 Hash・表示（A06・A07・A11・D01） |
| `src/llvm_debug.rs` | DWARF（G08） |
| `src/llvm_imports.rs` | host import（E06） |
| `src/llvm_io.rs` | IO の入口（`std/IO.tc`） |
| `src/llvm_math.rs` | 数学関数と FMA（D03・D05） |
| `src/llvm_parallel.rs`、`src/llvm_task.rs` | データ並列と `Task.parallel_results`（F02・B06） |
| `src/llvm_recursive.rs`、`src/recursive.rs` | 再帰型の判定・ノード・反復的な drop／clone（A04） |
| `src/llvm_simd.rs`、`src/simd.rs` | SIMD 型（F03・F04） |
| `src/llvm_traps.rs`、`src/trap.rs` | trap の種類・位置・計装（G04） |
| `src/lsp.rs`、`src/semantic.rs` | LSP サーバーと `SemanticIndex`（G07） |
| `src/package.rs` | manifest と package graph（E04） |
| `src/regions.rs` | 名前付き region の検査（A09。複数 region の record と region で量化した引数は A12） |
| `src/stdlib.rs` | std ソースの埋め込み（`SOURCES`）、予約モジュール、opaque record |
| `src/test_runner.rs` | `tsuzuri test`（G06） |
| `src/warnings.rs` | 警告 W1001–W1004（G03） |
| `src/copies.rs` | 暗黙の複製の一覧と W1006（A15） |

実行時ランタイムも増えています（`character.ll`、`display.ll`、`debug.ll`、`recursive.ll`、`utf8string.ll`、`math.ll`、
`heap-wasm-threads.ll`、`task-wasm-threads.c`、`task-windows.h`、`cpu.c`、`io.c`、`test-runner.c`）。連結条件は `src/llvm.rs` と
`src/driver.rs` の `include_str!` の周辺を読んで確認します（§2.2）。

---

## 5. 主要データ構造（要点）

- `Type`（`check.rs`）: `Variable(String)`（シグネチャの `'a`、rigid）、`Infer(usize)`（推論変数）、
  `Integer(bits, signed)`、`Binary(bits)`、`Decimal(bits)`、`Bool`、`Unit`、`String`、`Record(usize)`、
  `Array(Box<Type>)`、`List(Box<Type>)`、`Tuple(Vec<Type>)`、`Task(Box<Type>)`、
  `Function(Vec<Type>, Box<Type>)`（矢印列を正規化した表現。非カリー化ではない）、`Reference(Box<Type>, bool)`。
  `Record(id)` の id は `CheckedModule.records` の添字。**レコードは現在ジェネリックではない。**
- `TypedExprKind`（`check.rs`）: 型付き IR。子の列挙は `TypedExpr::children`／`children_mut`。
  `_ => Vec::new()` のフォールバックがあるため、**新しいバリアントを追加したら children 系に必ず追加**する。
- `CheckedModule { records, functions, entry }`。関数 ID は `functions` の添字。インスタンスのメソッドは
  `$instance.<id>.<method>` という名前の通常関数として `function_declarations` に追加される（`Classes::instances`）。
- `Names`（`check.rs`、非公開）: `modules`、`builders`、`records`（`"Module.Name"` → id）、`record_aliases`
  （無修飾名 → 修飾名の一覧）、`functions`（`"Module.name"` → id）、`active_patterns`。
  無修飾レコード名の解決は `Names::record`: 自モジュール → 全体で一意 → 曖昧なら `E1004`。
- `Scheme { signature, variables, constraints }`: 関数の型スキーム。`Checker::function` が呼び出しごとに
  `Infer` で具体化し、ジェネリックなら `TypedExprKind::GenericFunction(id, types)` を作る。
- `Classes`: 組み込みクラス（`Add` … `Send`）とユーザークラス。`implementations: BTreeMap<(class, Type), Vec<関数 id>>`。
  組み込みインスタンスの判定は `Classes::intrinsic`。制約の検査は `Classes::validate`。
- パターン: `control.rs` の `pattern_alternatives` が `Alternative { steps: Vec<PatternStep>, bindings }` を返す。
  `PatternStep::Test(bool 式)` は順に評価され、false なら次の節へ。OR は alternatives を増やす。
- LLVM 表現: `%tz.string = { ptr, i64 }`、`%tz.array = { ptr, i64 }`、`%tz.list = { ptr, i64 }`（先頭ノード・長さ）、
  リストのノード `{ ptr next, T }`、`%tz.closure = { ptr, ptr, ptr, ptr }`（code/environment/clone/drop）、
  レコード `%tz.record.<Module.Name> = type { fields... }`、タプルはリテラル構造体、`bool` は `i1`、`unit` は `i8`、
  参照は `ptr`。値は SSA の集約値（`extractvalue`／`insertvalue`）、借用可能な場所は entry block の `alloca`。
- 公開 ABI: `export def` だけが `tz_<name>` として出力され、8/16/32/64-bit 整数・f32・f64・bool・unit 結果のみ
  （`Type::exportable`、`abi_type`、`c_type`、`export_wrapper`）。

---

## 6. 変更パターン別チェックリスト

既存コードは `match` の `_ =>` フォールバックが多く、**追加漏れがコンパイルエラーにならない**場所があります。
新しいバリアントを追加したら、下の一覧に加え、似た既存バリアントの名前で全体を `grep` して漏れを確認してください。
直近の実例: コミット `19d8cdd` の `NewLiteral`（新しい式）、`Type::List` の出現箇所（新しい型）。

### 6.1 新しいキーワード・トークン

- `syntax.rs` の `TokenKind` にバリアントを追加し、`lexer.rs` の `Lexer::identifier` のキーワード表に登録する。
- **キーワード化はその名前を識別子として使う既存コードを壊す。** `docs/language.md` の「ソースと宣言」に予約語を追記し、
  `tests/frontend.rs` などに「以前は識別子だった名前がエラーになる」ことのテストを追加する。
- 記号は `Lexer::symbol` で最長一致。既存の `|`、`[|`、`|]`、`||`、`|>` の区別を壊さないよう
  `tests/lists.rs` の `list_delimiters_do_not_conflict_with_operators` を参考にテストする。
- モジュール名・レコード名の検査（`check_modules` 冒頭の字句チェック、`E1011`）はキーワードを拒否するので、
  キーワード追加で既存モジュール名が不正になることがある。

### 6.2 新しい式（`ExprKind` → `TypedExprKind`）

| 段 | 変更箇所 |
|---|---|
| 構文 | `syntax.rs` の `ExprKind`、`parser.rs`（`primary`／`prefix`／`postfix`／`application`／`expression_inner` のどこで読むか）、深さ計算 `Parser::make` |
| ビルダー展開 | `computation.rs` の `expand`（全 `ExprKind` を再帰的に辿る。漏れると内部のビルダー式が展開されない） |
| 型検査 | `Checker::expression` の振り分け（大きなスタックフレームを避けるため、呼び出し・ブロック系は `composed_expression`、値系は `value_expression`、制御系は `control_expression`）、新しい `TypedExprKind` |
| IR 走査 | `TypedExpr::children`／`children_mut`、`PatternStep`、`polymorph.rs` の `walk`／`expression_types`／`Specializer::lower`、`closures.rs` の `free_locals`／`lower_expression`、`recursion.rs` の `references` |
| 所有権 | `ownership.rs` の `eval_value`／`eval_composed`（move・loan・評価順序）、`closed_returns` の `closed`、`count_uses`／`uses`、`ownership_control.rs`（制御構文の場合） |
| 特殊化 | `call_specialization.rs` の `single_use_locals`、`may_mutate`、`non_escaping`、`read_only`、`all_children` |
| 生成 | `llvm.rs` の `expression_mode`（値の生成。`take` の意味に注意）、必要なら `place`／`is_place`、`llvm_frame.rs` の `frame_bytes`／`frame_inner`、末尾位置なら `tail` |

### 6.3 新しい型（`Type` のバリアント、または `Type::Record` の拡張）

| 段 | 変更箇所 |
|---|---|
| 解決 | `check.rs` の `resolve_type`、`polymorph.rs` の `type_expression`（`Type` → `TypeExpr` の逆変換。インスタンス生成で使用） |
| 性質 | `Type::{display, is_copy, needs_drop, contains_reference, contains_mutable_reference, carries_loans, can_capture, can_send, exportable}` |
| レイアウト | `check.rs` の `layout_size`、`validate_size`、`record_size`（再帰検出）、`llvm_frame.rs` の `stack_size` |
| 推論 | `polymorph.rs` の `map_type`、`substitute`、`bounded_type`（`visit`）、`Inference::resolve`、`Inference::unify`、`variables` |
| 型クラス | `Classes::intrinsic`（組み込みインスタンス）、`Classes::validate` |
| 所有権 | `ownership.rs` の `Checker::is_copy`、`require_copy`（ジェネリックの Copy 推論）、`closed_returns::owned`、`place`／`is_place` |
| 特殊化 | `call_specialization.rs` の `may_mutate::mutable_reference` |
| 生成 | `llvm.rs` の `llvm_type`、`drop_value`、`clone_value`、必要なら `emit_target` の型定義出力、`llvm_frame.rs` の `field_types`、`heap_copy`、`drop_frame_contents` |
| ABI | 公開しないなら `exportable` は false のまま。`c_type`／`abi_type` は `unreachable!` の前提を壊さない |

**関数値との違いに注意:** `is_copy` と `needs_drop` は同義ではありません（関数値は Copy だが drop が必要）。
配列・リストは要素がスカラーでも `needs_drop` は true です。

### 6.4 新しいトップレベル宣言

- `syntax.rs` の `Program` にフィールドを追加し、`Parser::program` のループに分岐を足す。
  トップレベルの分岐は先頭キーワードで決まり、最後の `else` は `Main.tz` のエントリーコードとして解釈される。
- ソース種別ごとの許可（`.tz`／`.tt`／`.tc`）は `computation.rs` の `collect` 周辺と `docs/language.md` の表で管理している。
  新しい宣言をどの種別で許可するか必ず決め、違反を `E1018` で拒否する。
- 名前は `check_modules` の冒頭で全モジュール分を先に収集する（宣言順・ファイル順に依存させない）。
  重複・予約名は `duplicate()`（`E1001`）。

### 6.5 新しい組み込み関数

- `check.rs` の `Builtin` に追加し、`Builtin::ALL`（配列長も更新）、`name`、`signature` を更新する。
  現在の `signature` は一引数だけを想定しているので、複数引数・制約付きが必要なら拡張する（E02 参照）。
- `Checker::name` は無修飾名、`Task.run` のような修飾名は `value_expression` の `ExprKind::Field` 分岐で解決される。
- 生成は `emit_builtin`（`define internal ... @tz.builtin.<name>`）。LLVM intrinsic は `intrinsics` に
  宣言文字列を登録し重複させない。**演算子と組み込み関数で同じ intrinsic を使う**（`binary`／`cast` と揃える）。
- 組み込み関数名はユーザーのトップレベル関数名として再定義できない（`check_modules` で拒否）。
  名前を追加すると既存ユーザーコードを壊す可能性があるため、新しい汎用名は std の修飾名（`Math.sin` など）を優先する。

### 6.6 新しいランタイム関数

- 小さい IR は `src/runtime/*.ll`、数値の正確なアルゴリズムは `numeric.c`（生成器 `generate.py` で `numeric.ll` を更新）、
  OS 依存は C（`task.c` の例: driver がコンパイル・リンク）。
- `emit_target` の末尾で、生成 IR に特定のシンボルが含まれるときだけランタイムを連結する方式に揃える。
- 内部シンボルは `@tz.<area>.<name>`（`define internal`）、C から見える外部シンボルは `tsuzuri_<name>`（`tz_` は公開 ABI 専用）。
- WASM ではインポートを増やさない。`tests/*.mjs` の `WebAssembly.Module.imports(module)` が空であることを検査する。
- 新しい `.ll` は `.gitignore` に `!src/runtime/<name>.ll` を追加して追跡する。

### 6.7 新しい診断コード

- 既存コードで表せる場合は既存コードを使う（`docs/language.md`「診断」の表）。新しいコードは
  **9 章の台帳で事前割り当てされたもの**を使う。未割り当ての新コードが必要になったら、台帳に追記してから使う。
- メッセージは英語、小文字始まり、修正方法を含める（既存の例: `"an empty array needs a type annotation, for example '[i64]'"`）。
- 位置は問題の本体（名前・式）の `Span`。別ファイルのモジュールでも元ファイルの位置を保つ（`Span::in_source`）。
- `docs/language.md` の診断表を更新する。

### 6.8 新しい CLI オプション

- `src/main.rs` の引数解析（`--target` などの分岐と、`rejects_ambiguous_or_unused_arguments` テスト）、
  `driver.rs` の `Options`（`validate` で不正な組み合わせを `E2000` にする）、README の CLI 表を更新する。
- 使えない組み合わせは黙って無視せずエラーにする（`--cpu native` と WASM の組み合わせの扱いが手本）。

### 6.9 P2 以降に増えた走査箇所

6.2〜6.4 の表は P1 の時点のものです。P2 以降のモジュール（§4.3）にも `ExprKind`・`TypedExprKind`・`Type` を辿る match があり、
`_ =>` の fallback で新しい variant を黙って無視するものがあります。新しい variant を足したら、次の手順で漏れを探します。

1. 性質の近い既存 variant の名前で全体を grep し、出現する関数を全部列挙する。
   例: 式なら `grep -rn "ExprKind::RecordUpdate\|ExprKind::NewLiteral" src/`、型付き IR なら
   `grep -rn "TypedExprKind::TaskParallelResults\|TypedExprKind::StructuralHash" src/`、型なら `grep -rn "Type::Char\|Type::Simd" src/`。
2. 列挙した各 match で、新しい variant が fallback に落ちて正しいかを一つずつ判断し、正しくなければ arm を足す。
   判断の結果（変更なし、を含む）はチケットの「段ごとの変更」表と照合する。
3. 特に確認するもの: `src/formatter.rs`（整形）、`src/semantic.rs`（LSP の索引）、`src/docgen.rs`（署名の描画）、
   `src/computation.rs` の `expand`、`src/constants.rs`、`src/derive.rs`、`src/warnings.rs`、`src/regions.rs`、`src/exhaustiveness.rs`、
   `src/llvm_debug.rs`、`src/llvm_compare.rs`・`src/llvm_hash.rs`・`src/llvm_display.rs`、`src/abi.rs`・`src/llvm_abi.rs`、`src/gpu.rs`。
4. 新しい variant を使う最小のソースで `tsuzuri fmt --check`・`tsuzuri doc`・LSP の hover（`tests/lsp.rs` の手本）も通ることを確かめる。

---

## 7. テストの書き方

### 7.1 Rust テスト（LLVM 不要）

`tests/*.rs` は `tsuzuri::{analyze, analyze_modules, llvm}` を使います。典型的な補助関数（`tests/lists.rs`）:

```rust
use tsuzuri::{analyze, analyze_modules, llvm};

fn accepts(source: &str) {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    // 決定的な IR であること
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}
```

- 複数ファイル: `analyze_modules(&[("Main.tz", main), ("Shapes.tz", shapes), ("Traits.tt", classes)])`。
- WASM 向け IR: `llvm::emit_target(&module, llvm::Entry::Library, true)`。
- 新機能ごとに `tests/<機能>.rs` を作り、受理例・拒否例（**診断コードを必ず照合**）・IR の不変条件
  （例: `assert!(ir.contains("switch i32"))`、intrinsic 宣言の重複なし）を網羅する。
- 所有権は「正常な move／借用」「move 後の使用 `E1012`」「寿命切れ `E1013`」「借用競合 `E1014`」を必ず含める。
- 多相は「抽象本体の検査」「具体化後の再検査」「特殊化上限」を含める。

### 7.2 E2E（Node、実ツールチェーン）

`tests/control.mjs` が標準的な手本です。

1. `tests/fixtures/<機能>/` に `.tz` の fixture を置き、`export def` で検査用の関数を公開する
   （公開 ABI は 64-bit 以下の整数・f32・f64・bool のみ。複雑な値は内部で計算して整数のチェックサムを返す）。
2. `cli(["build", fixture, "--emit", "llvm", "-o", ir])` を 2 回実行して IR が同一であることを確認する。
3. IR 中の `@malloc`／`@free` を `@tracked_alloc`／`@tracked_free` に置換し、C ホストで確保量を追跡して
   **各呼び出し後に `live == 0`**（解放漏れ・二重解放なし）を検査する。
4. `-O0` と `-O3` の両方で native をビルドして全ケースを実行する。トラップするケースは子プロセスで実行し、
   異常終了を確認する。
5. 同じ fixture を `--target wasm32 -O0/-O3` でビルドし、`WebAssembly.Module.imports(module)` が空であること、
   全ケースの結果、トラップが `WebAssembly.RuntimeError` になること、`memory.buffer.byteLength <= 16 MiB` を検査する。
6. 期待値は JavaScript の BigInt などの独立した参照実装で計算する（コンパイラの出力を期待値にしない）。
7. 新しい `tests/<機能>.mjs` を作ったら README と `docs/architecture.md` の検証コマンド一覧に追加する。

数値の意味や共有 lowering を変える場合は、native と WASM の `-O0`／`-O3` をすべて確認します（AGENTS.md）。
並列実行の検査で時間を合否条件にしないでください（`tests/task_runtime.c` の条件変数による同時実行確認が手本）。

### 7.3 性能

性能を主張するチケットは、`benchmarks/` に同条件の C/C++（必要なら Rust）比較と、生成 IR／アセンブリの確認手順を追加し、
`docs/benchmarks.md` に日付・環境・コミット・中央値・生データの保存先を記録します。速度閾値を CI に入れません。
性能チケットの共通手順は §14 です。

### 7.4 `tests/features.mjs` への suite の追加

多くのチケットは、新しい E2E を独立した `.mjs` ではなく `tests/features.mjs` の suite として足します。

1. fixture を `tests/fixtures/<suite 名>/` に置く。入口は `Main.tz`（必要なら他のモジュール・`.tt`・`.tc`）。
   検査したい関数は `export def` で公開する（公開 ABI の型だけ。複雑な値は内部で計算して整数のチェックサムにする）。
   置いたら `target/release/tsuzuri check tests/fixtures/<suite 名>` が通ることを確かめる。
2. `tests/features.mjs` の `const suites = { ... }` に `<suite 名>: { cases: [...], traps: [...], inspect(ir) { ... } }` を足す。
   - `cases` の各要素は `["<export 名>", [引数...], 期待値]`。引数と期待値は BigInt（`42n`）で書き、期待値は JavaScript の
     独立した計算で作る（`BigInt.asIntN(64, ...)` などで折り返しを表す）。**コンパイラの出力を期待値に写さない。**
   - `traps` は `["<export 名>", [引数...]]`。trap することだけを確かめる。
   - `inspect(ir)` は省略できる。生成 IR の不変条件（確保がない、intrinsic が一度だけ宣言される等）を `assert` で書く。
   既存の suite（`map_set`・`higher_kinds`・`simd` など）の書き方をそのまま真似る。
3. harness の共通部分（suite のループ `for (const [name, suite] of Object.entries(suites))` から呼ばれる処理）が、
   `@malloc`／`@free`／`@realloc` を追跡関数に置き換えた native の実行、wasm32 の `-O0`／`-O3` の build と
   `WebAssembly.Module.imports(module)` が空であることの検査を行う。足す前にこのループと suite の関数を読み、何が自動で検査されるかを確かめる。
4. 一つの suite だけを実行する: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri <suite 名>`。
   全 suite は第 2 引数なしで実行する。
5. suite 名は `_features/README.md` の完了記録と `README.md` の検証コマンドの説明に合わせて書く（件数を記録する）。

---

## 8. ドキュメント更新規約

機能を追加・変更・修正・削除したら、**同じコミットかプルリクエストで** `_tsuzuri/language-reference/`（目次は `index.md`）の
関連ページを最新にする。チケットを Phase ごとの PR に分けるときは PR ごとに行い、チケットのない修正にも適用する
（`AGENTS.md` の「Language reference」と同じ規則）。対象は、利用者から見えるすべての振る舞い（構文・意味・型・組み込み関数・
std の API・診断コードとメッセージ・CLI オプション・ターゲット・ホスト連携・文書に書いた性能特性・既知の制限と不具合）である。

1. 関連ページをすべて探す。機能の名前・キーワード・診断コード・CLI オプションでリファレンス全体を検索し
   （例: `grep -rnE "Int\.min|E1005|--emit" _tsuzuri/language-reference`）、いちばん目立つページだけで終わらせない。
2. 本文、例と `run=` の期待値、表（シグネチャ・計算量・他言語との比較）、mermaid 図、`index.md` を直す。新しい機能は、読者が探す場所に
   節かページを足す。削除した機能は説明と例を消し、代わりの書き方を示す。
3. 計画中の機能を実装したとき、既知の不具合を直したときは、「計画中」の注記や既知の不具合の警告と、`_features/`・`_perfs/` のチケットへの
   リンクを、実際の振る舞いの説明に置き換える。
4. 今のコンパイラの振る舞いだけを書く。主張は実装とコンパイラの実行で確かめ、計画中の対応は分けて書く（チケットへリンクする）。
5. `node scripts/check-docs.mjs <変更したページ>` を通す。リンクと見出しへのリンクを検査し、すべての例を `tsuzuri check` し、
   `run=` の例を native の `-O0`／`-O3` で実行し、テストを含む例は `tsuzuri test` する。
6. 更新したページを PR の説明（チケットでは §13.2 の完了報告）に書く。更新が要らなければ、その理由を書く。

そのほかの規約:

- 文書は日本語、コード識別子と診断メッセージは英語。言語リファレンスは、周りのページと同じ文体（です・ます）と構成で書く。
- 利用者向けの仕様は `docs/language.md`、コンパイラ内部の不変条件は `docs/architecture.md`、
  概要と実行例は `README.md`、性能は `docs/benchmarks.md`。言語リファレンスと同じ変更で食い違いを残さない。
- 「未実装」「未対応」と書かれている箇所（例: `docs/architecture.md`「初版の次に必要な設計」、
  `docs/language.md` の各節末尾の未対応一覧、README 冒頭の未実装一覧）を実装に合わせて更新する。
- 実装していない最適化や将来計画を、実装済みのように書かない。
- サンプルプロジェクトを追加したら `examples/` に置き、`tests/examples.mjs` で実行されるようにする（言語リファレンスの中の例は
  ページに直接書き、上の手順 5 で検証する）。

### 8.1 旧 `_docs/` のパスの読み替え

`_docs/` は `_tsuzuri/language-reference/`（以下 `LR/`）へ再構成した。2026-10-07 より前に書いたチケットや性能計画にある
`_docs/...` のパスと `check-docs.mjs` の引数は、次の表で読み替える。旧ページの内容は
[c82c13e の `_docs/`](https://github.com/tatsuya-midorikawa/Tsuzuri/tree/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs) で読める。

| 旧パス（`_docs/`） | 読み替え先（`LR/`） |
| --- | --- |
| `language-reference/active-patterns.md`・`patterns.md` | `pattern-matching/` の各ページ |
| `language-reference/computation-expressions.md` | `computation-expressions/computation-expressions.md` |
| `language-reference/control-flow.md` | `loops-and-conditionals/` の各ページ |
| `language-reference/error-handling.md` | `exception-handling/` の各ページ |
| `language-reference/expressions-and-operators.md` | `values-and-functions/op-and-expressions.md` |
| `language-reference/functions.md` | `values-and-functions/functions.md`・`lambda-expressions.md` |
| `language-reference/generics-and-typeclasses.md`・`higher-kinds.md`・`deriving.md` | `types-and-type-inference/generics.md`・`type-classes.md`・`constraints.md`、`values-and-functions/generics-functions.md` |
| `language-reference/lexical-and-layout.md` | `values-and-functions/keywords.md`・`tokens.md`・`statements.md` |
| `language-reference/modules-and-packages.md` | `organizing-tsuzuri/` の各ページ |
| `language-reference/numbers.md`・`types.md` | `types-and-type-inference/` の各ページ、`literals-and-strings/literals.md` |
| `language-reference/ownership.md`・`lifetimes.md` | `ownership-and-memory/` の各ページ |
| `language-reference/records.md`・`unions.md` | `built-in-types-and-modules/record.md`・`union.md` |
| `language-reference/strings-and-characters.md` | `literals-and-strings/` の各ページ |
| `language-reference/tasks.md` | `async-tasks-and-lazy/task.md` |
| `language-reference/values-and-constants.md` | `values-and-functions/values.md` |
| `library-reference/*.md`・`library-reference/api/*.md` | `built-in-types-and-modules/` の該当モジュールのページ（生成 API の snapshot は置かない） |
| `guides/webassembly.md`・`wasm-threads.md`・`native-interop.md` | `compiler/webassembly.md`・`native-interop.md` |
| `guides/gpu.md`・`performance.md` | `built-in-types-and-modules/gpu.md`・`parallel.md`・`simd.md`、`languages/strategy.md` |
| `tools/*.md` | `compiler/` の各ページ、`organizing-tsuzuri/documentation-comment.md`、`built-in-types-and-modules/test.md` |
| `learn/*.md`・`tour.md`・`get-started.md` | `languages/` の各ページ、`compiler/usage.md` |
| `feature-status.md` | なし（機能の状態は `_features/README.md` の一覧で管理する） |

---

## 9. 設計決定台帳（チケット横断）

複数のチケットにまたがる決定です。各チケットはこの決定に従って書かれています。
実装中に不可能・不適切と判明した場合は、**黙って変えず**、台帳の該当行に「見直し提案」を追記して人間に確認してください。

### D-01 命名規約
- 型・共用体のケース・モジュール・型クラス: `PascalCase`。関数・フィールド・ローカル: `snake_case`（既存の `clone_string`、`to_float`、`bit_and` と同じ）。
- 標準ライブラリの関数は常に `Module.function` で呼ぶ（F# の `List.map` と同様）。例: `Maybe.map`、`Array.sum`、`String.split`。

### D-02 型適用の構文
- 型パラメーターと型引数は **`<...>` 内のカンマ区切り**:
  `Maybe<i64>`、`Result<i64, string>`、`Pair<'a, 'b>`、`[Maybe<i64>]`、`Maybe<Pair<i64, i64>>`。
  レコード・union・型クラスの宣言、制約・インスタンス、`Task<T>`、将来のジェネリック型も同じ形式にする。
  名前と `<` は隣接させ、空のリストは拒否、末尾のカンマは許す。旧来の空白区切りは受理しない。
- 構文木は `TypeExprKind::Apply(Box<Ident>, Box<[TypeExpr]>)`。
  `type_primary` は修飾名の後の `<...>` を読み、各型引数は完全な `type_expr` として解析する。
  入れ子の `>>`・`>>>` と `>=` の先頭の `>` は型の文脈でだけ分割し、式の演算子を変えない。
- 既存の `Add<'a>`（制約付き型変数）も同じ `Apply` として構文解析し、`resolve_type` の段階で分類する:
  名前が型クラスなら制約、ジェネリック型なら型適用。同じ名前が両方に解決できる場合は `E1004`（曖昧）。
  型クラス名と型名の衝突は宣言時にも `E1001` で拒否する（新しい曖昧さを作らない）。

### D-03 ジェネリックな名前付き型の内部表現
- `Type::Record(usize)` を `Type::Record(usize, Vec<Type>)` に変更する（id は宣言、`Vec<Type>` は型引数。非ジェネリックは空）。
  共用体は `Type::Union(usize, Vec<Type>)` を新設する。フィールド／ペイロードの型は宣言の型パラメーターを
  型引数で置換して得る（補助関数 `CheckedModule`／`Checker` 側に `record_field_types(id, args)` などを設ける）。
- 単一化は id の一致と型引数の要素ごとの単一化。表示は `Pair<i64, string>`（`Type::display`）。
- 単相化後の LLVM 型名は決定的にマングリングする。型引数には **LLVM 型の文字列ではなく Tsuzuri の型の正規表記**を使う
  （`llvm_type` の結果には引用符付き識別子が含まれ、入れ子にすると無効な IR になるため）:
  `%"tz.record.Main.Pair[i64,string]"`、`%"tz.union.Maybe.Maybe[Main.Pair[i64,string]]"`。
  正規表記は構造的・単射な規則で生成する（修飾名、型引数は `[` `]` と `,` で区切る。配列 `[T]` は `array[T]`、
  リスト `list[T]`、タプル `tuple[T,U]`、関数 `fn[T,U->R]`、参照 `ref[T]`／`refmut[T]`、タスク `task[T]`、固定長配列 `[T; N]` は `fixed[N,T]`（A16。型引数の位置の長さは十進の数字））。
  `"` と `\` は生じないが、生じ得る実装にする場合は LLVM の `\xx` エスケープを使う。レコードと共用体で同じ関数を共有する。
  全具体インスタンスは **実際に出力する関数・ラッパーの型から** BTreeSet で集めて IR 冒頭に定義する（未使用の std の型を出さない）。
  入れ子（レコード in レコード、共用体 in レコード、タプル・関数・参照を引数に持つもの）を Clang で native／wasm32 とも
  アセンブルできることをテストする。
- レイアウト上限（64 KiB）・再帰検出は具体化した型ごとに再検査する。

### D-04 ジェネリックなレコード宣言
- `record Pair<'a, 'b> { first: 'a, second: 'b }`。型パラメーターは名前の後の `<...>` にカンマ区切りで並べ、フィールドは宣言した変数だけを使う。
- リテラル `Pair { first: 1, second: "x" }` の型引数は推論する。型注釈は `Pair<i64, string>`。
- A01 はライフタイム機能ではない。型引数を代入した後のフィールドが共有・排他のどちらの参照を含んでも `E1013`
  （現在のレコードと同じ規則。入れ子のレコード・コレクション経由も含む）。リテラル・注釈・シグネチャ・単相化後の全具体型で検査する。
  借用フィールドは A09 で扱う。

### D-05 判別共用体（union）
- キーワード `union`（新規予約語）。
  ```text
  union Shape =
      | Circle of f64
      | Rect of f64 * f64
      | Empty
  union Maybe<'a> = None | Some of 'a
  ```
  最初の `|` は省略可。各ケースのペイロードは `of T` で **ちょうど 0 個か 1 個の型**（複数値はタプル型 `f64 * f64` やレコード）。
- ケース名は大文字始まり。同じモジュール内のケース名・レコード名・共用体名は互いに重複不可（`E1001`）。
- 構築: ペイロードなしのケースは値（`Empty`）、ペイロードありは一引数の関数値（`Circle 1.0`、`Rect (1.0, 2.0)`、`Maybe.map Some x`）。
- 名前解決: 無修飾（自モジュール → プロジェクト内で一意 → 標準ライブラリ）、`Module.Case`、`Module.Union.Case`。
  ローカル変数・関数・アクティブパターンとの優先順位はチケット A02 の表に従う。
- パターン: `Circle r`、`Rect (w, h)`、`Empty`、`Maybe.Some x`。ペイロードは既存のパターンで分解する。
- 値の性質: 全ペイロードが Copy なら Copy、いずれかが drop を要すれば `needs_drop`。`==` は組み込みにしない（レコードと同じ。A07 の deriving で提供）。
- LLVM 表現: タグ `i32` と、全ケースで共有するペイロード領域（ケースごとの別フィールドにしない）。
  詳細なレイアウト規則はチケット A02。再帰的な共用体はチケット A04 まで `E1010` で拒否する。

### D-06 型別名
- `type Meters = f64`、`type Pair2<'a> = Pair<'a, 'a>`。**透過的**（別名と元の型は同一の型）。キーワード `type`（新規予約語）。
- 区別される新しい型（newtype）は単一ケースの共用体で表す: `union UserId = UserId of i64`。

### D-07 標準ライブラリの配置と解決
- リポジトリ直下の `std/` に Tsuzuri のソース（`.tz`／`.tt`／`.tc`）を置き、`include_str!` でコンパイラに埋め込む。
  driver とメモリ上の `analyze_modules` の両方で、利用者のモジュールに加えて常に読み込む。
- 標準ライブラリのモジュール名は予約し、利用者のファイル名と衝突したら `E1011`。
- 無修飾の **型・ケース・レコード・クラス** の解決順（参照元が利用者のモジュールの場合）:
  自モジュール → 利用者のモジュールで一意 → 標準ライブラリ。
  **参照元が標準ライブラリの場合は利用者の宣言を一切探さない**（自モジュール → 標準ライブラリ）。
  利用者が `Result` や `Ok` という名前を宣言しても std の型検査結果が変わらないようにするためである。
  さらに std のソースでは、他の std モジュールの名前を常に修飾して書く（`Maybe.tc` の中では `Result`、`Result.Ok`）。
  関数は従来どおり他モジュールからは修飾必須（`Maybe.map`）。
- LLVM を必要としない言語機能（ビット演算 intrinsic など）は、`Task.run` と同じ **修飾名付きの組み込み関数**
  （`Builtin` に `"Int.popcount"` のような名前で登録）として std のモジュール名前空間に置く。
  `Module.name` の解決は「ソース定義の関数 → 同名の組み込み」の順。ただし **組み込み関数として提供する API は
  `Builtin` だけに置き、同じ修飾名の `def` を std のソースに書かない**（書くと組み込みが隠れる）。
  衝突はコンパイラのテストで検出する（E02）。組み込みのシグネチャは文脈で解決するテンプレート
  `BuiltinType`／`BuiltinScheme`（std の型 `Maybe` や型族 `UnsignedOf`／`WidenOf` を表せる）で定義し、
  `FunctionRef::Builtin(BuiltinInstance)` を唯一の参照形式にする（詳細は E02）。
  引数なしの関数は `fn() -> T` の値なので、`Math.pi()` のように呼び出して使う。
- 未使用の標準ライブラリ関数は IR に出力しない（到達可能性で間引く）。型検査は常に行う。
- 補助関数は `private`（D-09）にする。
- 標準ライブラリのモジュール名と担当チケット（新しい std モジュールはこの表に追記してから作る）:

  | モジュール | 内容 | 担当 |
  |---|---|---|
  | `Maybe`（`Maybe.tc`） | `Maybe` 型・関数・ビルダー | B01 |
  | `Result`（`Result.tc`） | `Result` 型・関数・ビルダー | B01 |
  | `Array` | 配列の一括操作・関数的更新 | C04、C01 |
  | `List` | 連結リストの操作 | C04 |
  | `Vec` | 伸縮可能な配列 | C02 |
  | `String`／`Utf8String` | UTF-16／UTF-8 文字列操作 | D02 |
  | `Char`／`Utf8Char` | UTF-16 コード単位／Unicode スカラーの変換・分類 | A08 |
  | `Math` | 浮動小数点の数学関数、FMA | D03、D05 |
  | `Int` | 整数 intrinsic | D04 |
  | `Debug` | デバッグ出力 | E07 |
  | `Parallel` | データ並列 API | F02 |
  | `Simd` | SIMD ベクトル型の操作 | F04 |
  | `Map`／`Set` | 順序付きの連想コンテナ | C06 |
  | `Seq` | 明示的な一回消費の遅延反復 | C07 |
  | `Test` | テスト用の比較・報告 | G06 |
  | `Gpu` | GPU 実行 API | F07 |
  | `Owned` | 早期解放（`Owned.drop`）と Drop 型を捕捉できる関数値（`Owned.Function`） | B07 |
  | `HashMap`／`HashSet` | 不透明なハッシュ表と集合（挿入順の反復、seed 付きハッシュ、借用キーの検索） | C09 |
  | `File`／`Dir`／`Path`／`Env`／`Time`／`Random`／`Os`／`Process` | OS API（`IO` の遅延アクション。`Path`・`Os` の純粋な部分と `Random.Pcg` は wasm32 でも使える） | E08 |
  | `Format` | 書式指定の部品（`Spec`・`parse`・`pad`）。`Format` 型クラスの instance を書くための補助 | D07 |
  | `FixedArray` | 固定長配列の構築（組み込み `FixedArray.init`。std のソースはなく、名前を予約する） | A16 |
  | `Dyn` | dyn 値の構築とアップキャスト（組み込み `Dyn.of`。std のソースはなく、名前を予約する） | A14 |

  組み込みクラス（`Display`、`Parse`、`Hash`、`Default`、`Elementary` など）は std モジュールに属さない組み込み名として予約する。
  `Elementary` は超越関数（`Math.sin` など）用のメソッドなしマーカークラスで、D03 では f32／f64 だけが満たす。
  `UnsignedInteger`（D04）は符号なし整数だけが満たす組み込みマーカークラスとして予約する。
  `Drop`（B07）は利用者が宣言した record・union だけが instance を持つ組み込みクラスとして予約する（D-31）。
  `Format`（D07）も利用者が宣言した record・union だけが instance を持つ組み込みクラスとして予約する（D-32）。

### D-08 Maybe と Result
- `std/Maybe.tc`: `union Maybe<'a> = None | Some of 'a`、関数（`map`、`bind`、`default_value`、`is_some`、`is_none` など）、
  コンピュテーション式の操作（`Bind`、`Return`、`ReturnFrom`、`Zero` など）。`Maybe { let! x = ...; return x }` が使える。
- `std/Result.tc`: `union Result<'a, 'e> = Ok of 'a | Error of 'e`、関数（`map`、`map_error`、`bind` など）、同様の操作。
- `.tc` はビルダー 1 個分の実装であり、共用体・レコード・補助関数・インスタンスを含められる（A02 で union を許可）。

### D-09 可視性
- キーワード `private`（新規予約語）を `def`／`record`／`union`／`type` の前に付けると、同じモジュール内だけから参照できる。
  既定は従来どおり公開。`export` とは独立（`private export` はエラー）。違反は `E1022`。

### D-10 エラー処理の方針
- プログラムの誤り（範囲外アクセス、ゼロ除算、`assert` 失敗）は従来どおりトラップ。
- **A03 の実装後**、ソースに書いた `match` と関数ガードは静的に網羅的でなければならず、網羅されなければ `E1021`。
  生成コードの不一致時トラップ（`match_expression` の失敗ブロック）は多重防御として残す。
  `for`・ラムダ式・コンピュテーション式の分解パターンの不一致は従来どおり実行時トラップ（A03 参照）。
- 予期できる失敗（解析、検索、変換）は `Maybe`／`Result` を返す。例外・巻き戻しは導入しない。
  **D-34 で改訂:** `@checked` の整数 overflow だけが例外を送出し、`try ... with ... finally` が同じ関数本体の中で捕まえる（巻き戻しは導入しない）。
- 早期脱出の `?` 演算子は導入しない。伝播は `Maybe { }`／`Result { }` ビルダーで書く（B02）。

### D-11 表示と解析
- 組み込みクラス `Display<'a> { def display :: &'a -> string }`。数値・bool・unit・string・utf8string・char・utf8char に組み込みインスタンス。
- 組み込み関数 `to_string :: Display<'a> => 'a -> string`（値を受け取り、内部で借用して `display` し、値を解放する）。
- 組み込みクラス `Parse<'a> { def parse :: &string -> Maybe<'a> }`。数値・bool・char・utf8char に組み込みインスタンス。
- 数値の文字列表現は、`to_string` とコンソール出力（`console_main`、`tz_soft_format`）で **同じ実装・同じ形式** にする。
- **決定（2026-09-23 承認）:** 二進浮動小数点（f16／f32／f64／f128）は、同じ型へ解析し直すと元の値に戻る
  **最短の十進表現**で表示する（例: `0.1` は `0.1`。現在の `%.17g` 相当の `0.10000000000000001` はやめる）。
  コンソール出力も同じ形式に変える（既存のテスト・文書の期待値を D01 で更新する）。NaN はすべて `nan`、負のゼロは `-0`。
  字句の詳細・指数表記の条件・decimal の扱いは D01 に従う。
- 組み込みクラス `Hash`（A07）と `Default`（A07）は deriving のチケットで定義する。

### D-12 文字型
- **決定（2026-09-27）:** `char` は全 UTF-16 コード単位（LLVM `i16`）、`utf8char` は Unicode スカラー（LLVM `i32`）とする。
- リテラルは `'a'`／`u8'a'`。char は surrogate を許し補助平面を拒否、utf8char は補助平面を許し surrogate を拒否する。型変数 `'a` は維持する。
- Copy・Eq・Ord・Display・Parse、算術と暗黙変換なし。`Char.to_u16`／`of_u16`、`Utf8Char.to_u32`／`of_u32`／`of_u32_unchecked` で明示変換する。
- string／utf8string の既存の索引・列挙は i16u／ubyte のまま。char の孤立 surrogate は Display/Parse で保持し、UTF-8 コンソール出力ではトラップする。

### D-13 コレクションの更新・伸縮・部分参照
- **消費する関数的更新**: `Array.set : ['a] -> i64 -> 'a -> ['a]` のように所有値を受け取り新しい値を返す。
  所有権により唯一の所有者であることが保証されるため、実装はバッファをその場で書き換えてよい（観測できない最適化）。
- 伸縮可能な配列は組み込み型 `Vec<'a>`（C02）。`[T]` の記述子 `{ ptr, i64 }` は変更しない。
- 部分参照（スライス）は共有が `&[T]`（C03）、排他が `&mut [T..]`（C08）。`&xs[a..b]`／`&mut xs[a..b]` で作る。
  `&mut [T]` は配列全体の置換用のままで、呼び出しの引数では `&mut [T..]` へ変換できる。
  コレクションは共有されている間は不変で、排他スライスを通してだけ `Array.write`／`Array.swap_in`／`Array.sort_in_place` で要素をその場で置換できる。
  要素単位の排他借用（`&mut xs[i]`）と代入構文（`xs[i] = v`）は導入しない。
  固定長配列 `[T; N]`（A16）は名前付きの値から `&xs[a..b]` で共有スライスを作れ、引数では `&[T]` へ変換できる。固定長配列の排他スライスは作らない。
- **決定（2026-09-27、推奨仕様での実装を承認）:** ジェネリック union の共有借用ペイロードを許可し、
  C04 の `Maybe<&T>` を提供する。コンテナ借用は格納値の loan を親として引き継ぐ。
  返却値は本体の実 loan を検査し、呼び出し側では借用を持つ全入力の寿命に制限する。
  `None` のように loan を持たない値は借用入力なしでも返せる。直接の借用 payload 宣言・排他参照・レコードの借用フィールドは引き続き拒否する。
- 以降の機能についても、利用者が推奨仕様での実装を承認済み（2026-09-27）。判断は各チケットと本台帳へ記録し、未実装を実装済み扱いにしない。

### D-14 決定性と数値演算の順序
- 一括演算は評価・集計の順序を API 契約として文書化する。浮動小数点の集計は既定で左から右の逐次。
  別の順序（pairwise、Kahan、並列チャンク）は名前で区別した別 API にする（`Array.sum` と `Array.sum_pairwise`）。
- 並列処理の分割境界は入力長だけで決め、スレッド数に依存させない（どの機械でも同じ結果）。
- 整数の折り返し加算のように結合的な演算だけは、順序を変えても結果が同じなので SIMD・並列化してよい。
- F02 の初版は init/map/map_ref/reduce の直接完全適用で所有権境界を保持する。演算自体の関数値化は未対応だが、callback は所有環境を証明できる関数値を受ける。
  チャンク数は4096要素目標・最大1024、各チャンクと最終結合の両方を identity から始める。Array の逐次集計とは別の順序である。

### D-15 キーワード・演算子の追加一覧
新しい予約語: `union`（A02）、`type`（A05）、`private`（E01）、`break`／`continue`（B03）、`deriving`（A07）、
`const`（D06）、`test`（G06）、`extern`（E06）、`dyn`（A14。D-39）。`of` は union 宣言の中だけの文脈キーワード（A02。予約語にしない）。
文字リテラル `'x'`／`u8'x'`（A08）。範囲の部分参照 `xs[a..b]`（C03）。
レコード更新 `{ base with field = value }`（C05）。キーワード追加時は 6.1 を実施する。
D-34 の演算子 `**`・単項 `+`・`&&&`・`|||`・`^^^`・`~~~`・`<<<`・`>>>`、関数合成 `>>`／`<<`（従来のシフトから意味を変更）、
文脈キーワード `try`／`finally`／`is`（予約語にしない）、属性 `@checked`／`@literal`。
D-35 の文脈キーワード `namespace`／`using`（ファイル先頭の宣言だけ。予約語にしない）。D-37 の名前空間のパス `A::B::Module`（新しい記号はなく、空白なしの `::` を lexer が `PathSep` にする）。
D-39 の固定長配列の型 `[T; N]` と長さパラメーター `const N: i64`（A16。既存の予約語 `const` の組み合わせで、新しい予約語はない）、排他スライスの型 `ref mut [T..]`（C08）、
dyn 型 `dyn C`・`dyn (C, D, Copy, Send)`・`dyn C {r}`（A14）、関数の属性 `@cpu ["avx2", ...]`（F08。`def` シグネチャの前だけ。`cpu` は予約語にしない）。
- **並行作業の注意（2026-09-23 時点、未コミット）:** 作業ツリーで、借用・参照外しの別表記 `ref x`／`ref mut x`／`deref r`
  （予約語 `ref`／`deref`、`ExprKind::Borrow`／`Dereference` に `Notation` を追加。`&`／`*` も残る）が開発中。
  取り込まれた後に着手するチケットは、借用・参照外しを扱う箇所（C03 の `&xs[a..b]` に対する `ref xs[a..b]`、A11 の比較用の
  内部借用、G05 のフォーマッター、G02 の復帰位置の判定、A08 の字句解析など）で両方の表記を扱うこと。

### D-16 診断コードの事前割り当て

| コード | 用途 | チケット |
|---|---|---|
| `E1021` | 網羅されていない `match`／関数ガード | A03 |
| `E1022` | `private` な名前の参照、可視性の不正な指定 | E01 |
| `E1023` | ループ外・関数境界を越える `break`／`continue` | B03 |
| `E1024` | 型宣言（union、ジェネリックなレコード、型別名）の不正（未使用・未宣言の型パラメーター、ケースの重複など） | A01／A02／A05 |
| `E1025` | `deriving` できない型・クラス | A07 |
| `E1026` | コンパイル時定数の評価失敗（トラップ、上限超過） | D06 |
| `E1027` | 条件付きインスタンス・スーパークラスの不整合 | A06 |
| `E1028` | dyn 互換でない型クラス（理由をメッセージに示す。D-39） | A14 |
| `E2006` | テストの失敗（`tsuzuri test`） | G06 |
| `W1001` | 未使用のローカル束縛 | G03 |
| `W1002` | 未使用の非公開関数・型 | G03 |
| `W1003` | 到達しない `match` 節 | A03／G03 |
| `W1004` | 同じスコープ内での紛らわしいシャドーイング（既定は無効） | G03 |
| `W1006` | 長さに比例する配列・リストの暗黙の複製（既定は無効。`--warn implicit-copy` で有効。D-33） | A15 |

既存のコード（`E1001`–`E1020`、`E2000`–`E2005`、`W2001`）は意味を変えずに使う。

### D-17 ABI とシンボル
- 公開 ABI（`tz_<name>`）の型の範囲は E05 まで変更しない。内部シンボルは `@tz.*`、外部ランタイムは `tsuzuri_*`。
- 追加の公開エントリー（例: WASM の確保関数）は `tz_` の名前空間を避けて `tsuzuri_` を使い、文書化する。
- E05 は現在の文字列モデルに合わせ、string の ABI を UTF-16/i16u のコード単位、utf8string を妥当な UTF-8/ubyte のバイト列とする。
  借用時の暗黙変換・コピーは行わず、返却 buffer の所有権だけをホストへ移す。レコードは scalar field を正規化した pointer/out pointer ABI にする。

### D-18 WASM とオプトイン機能
- WASM の既定はインポートなし・逐次・SIMD なし。SIMD128（F03）、threads（F06）、ホストのインポート（E06）、
  デバッグ出力（E07）は明示的なオプションで有効にし、未指定時の出力は変えない。
  OS API（E08）の WASI への lowering `--wasm-host wasi` も同じ扱いで、既定の wasm32 は OS API に到達するビルドを `E2000` で拒否する。

### D-19 解析の資源上限
- 新しい解析・展開（網羅性検査、deriving の生成、const 評価、到達可能性など）は、既存の上限（深さ 128、1,024 など）
  に揃えた上限を持ち、超えたら `E1017`。無限ループやスタック枯渇でコンパイラを落とさない。

### D-20 比較は非消費（`Eq`／`Ord` は借用を受け取る）
- **決定（2026-09-23 承認）。** 言語仕様の変更として実施する（`docs/language.md` の該当記述は A11 で更新する）。
- 組み込みクラスのメソッドを `Eq.eq`／`Eq.ne :: &'a -> &'a -> bool`、`Ord.lt`／`le`／`gt`／`ge :: &'a -> &'a -> bool` に変更する。
  ユーザーのインスタンスもこのシグネチャで実装する（本体のフィールドアクセスは自動参照外しされるため、
  `fn eq a b = a.x == b.x` のような既存の書き方はそのまま通る）。
- 演算子 `==`、`!=`、`<`、`<=`、`>`、`>=` は **全ての型で被演算子を消費しない**（現在の文字列 `==` の規則を一般化）。
  被演算子は左から右に評価し、場所（ローカル・フィールドなど）なら借用、一時値なら内部の一時スロットに置いて借用し、
  比較後に解放する（利用者が書く `&` の一時値借用が未対応であることとは別の、コンパイラ内部の処理）。
  所有権検査では被演算子を `Use::Read` として扱う（`ownership.rs` の `E::Binary` 分岐の文字列特例を一般化）。
- 組み込みインスタンス（数値・bool・unit・string・utf8string・char・utf8char）の直接 lowering を保つ（`icmp`／`fcmp`／型別文字列比較のまま、
  参照の受け渡しを生成しない）。ユーザー／条件付きインスタンスだけ `Call(method, [&left, &right])` に下げる。
- 算術・ビット演算のクラス（`Add` など）は従来どおり値を受け取る（文字列の `+` は両辺を消費する）。
- 実装はチケット **A11**。A06（条件付き `Eq<['a]>` など）、A07（deriving）、C04（`Array.sort`、`contains` など）、
  C06（Map のキー比較）は A11 を前提にし、比較のためだけに `Copy` を要求しない。

### D-21 発散する組み込み関数 `unreachable`
- `unreachable : unit -> 'a` を組み込み関数として追加する（B01 で実装。E02 の多相 builtin 機構を使う）。
  呼ぶと必ずトラップする。`Maybe.get` のような任意型を返す部分関数を、**網羅的な `match`** で書くために使う
  （A03 の網羅性検査と両立させる。bottom 型・`panic : string -> 'a`・std 限定の非網羅許可は採用しない）。
- 生成は型ごとの `define internal <T> @tz.builtin.unreachable.<mangled-T>(i8 %unit)`（`unit` は `i8` に下がるため、
  通常の呼び出し規約どおり引数を 1 個受け取り、使わない）。本体は `call void @llvm.trap()` と `unreachable` だけ。
  呼び出し側の制御フロー・所有権解析・関数値としての持ち上げは通常の一引数関数のまま変えない。

### D-22 関数・束縛の由来（origin）は一つの構造で表す
- 複数のチケットが関数に由来情報を付けるため、`CheckedFunction` のフィールドは **`origin: FunctionOrigin` の一つだけ**にする
  （別名のフィールドを並立させない）。

  ```rust
  pub struct FunctionOrigin {
      pub module: ModuleOrigin,          // E02: User | Std
      pub provenance: Provenance,        // G03: User | Generated
      pub parent: Option<usize>,         // E02: 生成関数（$lambda/$task/$builtin/$export/union 構築子など）の持ち主の関数 id
      pub test: Option<usize>,           // G06: test 宣言の本体、またはその本体からだけ生成された関数なら test の番号
  }
  ```
- 各チケットは自分の担当フィールドを追加する: E02 が `FunctionOrigin` を導入して `module` と `parent` を持たせ、
  G03 が `provenance` を、G06 が `test` を追加する（先に実装されたチケットがなければ、その時点で構造体を導入する）。
  生成関数は持ち主の `module`・`test` を継承し、`provenance` は `Generated` にする。
- 出力する関数の集合（到達可能性）の計算は一か所（E02 の `reachable_functions`）にまとめ、G06 の通常ビルド／テストビルドの
  切り替えは、その計算の根（root）の選び方として実装する。
- ローカル束縛の由来（G03 の未使用警告）は `Local` に `provenance: Provenance` を追加して表す。

---

### D-23 アクティブパターンの再評価

- **決定（2026-09-27）:** B04 の複数ケース認識器は既存 union と宣言順で対応させ、隠し union を作らない。
- 認識器と追加引数は節ごとに再評価する。副作用で結果が変わり得るため、全 active case の列挙だけで網羅的とはみなさず、現段階ではフォールバックを要求する。
- case のメタデータは維持する。将来の網羅性拡張では、同一入力・同一追加引数と再評価の安全性を証明するか、明示的な一回評価の契約を別に設計する。

### D-24 再帰型は具体的な union の所有ノードで表す

- A04は当初案の `Type::Boxed`／box/unbox typed kindを追加しない。宣言とcanonical nameを保ち、具体型のSCC内にあるunionだけを所有ポインターへ下げる。レコードはinline、非再帰の具体化は従来表現のまま。
- array/list/Vecも再帰の格納グラフへ含める。空collectionを有限値の基点とし、全循環はunionを通る必要がある。function/task/referenceは境界。
- payloadを一度だけ左から右へ評価し、その後でノードを確保する。最初のnullary caseだけはnullで表す。
- 解放予定ノード／複製先ノードを待ちリストに使い、collection要素も一つの反復走査へ登録する。dropに追加確保はない。sourceの型・所有権・constructor signatureは変えない。
- nativeは100万ノード正常、WASMは16 MiB内正常と100万ノードの確保トラップを検証する。A04の旧手順よりこの決定とチケット冒頭の実装仕様を優先する。

### D-25 型クラスの条件付き解決と構造比較

- A06は単一のinstance template集合と既存Specializerのキャッシュを使う。signatureとdefault本体は既存AST列に保持して名前で関連付ける。
- OrdのEq前提、defaultの無条件検査、headだけでのoverlap拒否、深さ64・要件128・overlap1024組を採用する。
- 配列・リスト・タプルは借用要素メソッドを持つ合成関数へ下げる。内部のStructuralCompareはlistのlockstep単一走査用で、公開構文・辞書・boxingを増やさない。
- 型が縮むinstance展開は実行時の再帰とは区別し、具体化後も共有SCC検査でrec指定を検証する。

### D-26 導出の文字モデルとcanonical Hash

- A07の旧scalar文字モデルをA08に合わせる。string/charはUTF-16 unit、utf8string/utf8charはUTF-8 byte/scalar。引用はUTF-16の4桁escape、UTF-8型のu8接頭辞・braced escapeで孤立surrogateを保持する。
- Hashのタグはstring/utf8stringが0x53/0x73、char/utf8charが0x43/0x63。文字列長はunit/byte数、unitは2byte、scalarは4byte、すべてlittle-endian。
- 数値Hashのwidth codeはlog2(width/8)。±0とNaNを正規化し、decimalは係数末尾0を除いたBIDへ正規化する。等しいdecimal cohortが異なるHashになる旧案は採用しない。
- 通常ASTのinstance合成を維持し、コレクションHash/Displayの単一走査だけLLVM内部helperへ下げる。構造Displayの引用builtinはD02ソースに依存しない。

### D-27 数学ランタイムの再利用と生成環境

- D03の基本Mathは全float形式、Elementaryはf32/f64のみ。既存numericランタイムでsoft sqrt／丸めを正確に実装する。
- 超越関数は固定musl 1.2.5の必要ソースをライセンス込みで同梱する。通常f64 atan2は1ulp契約を守る補償演算で補強し、速度の改善は未主張。
- Clangとllvm-linkは対応版を揃える。検証済みはApple Clang 21とllvm-link 21。22/23はlifetime署名・浮動小数点定数表記が変わるため使用しない。
- numeric生成後にmathも生成し、metadata／attribute範囲を分離する。source markerは両ランタイムの後に配置する。保存前のIRコンパイル検証を必須にする。

### D-28 定数とP2の実装判断

- 2026-09-27、P2全件と必要な判断を利用者が承認。安全性・数値意味・既存契約を優先して判断し、実装内容を各チケットに記録する。
- D06は独立した型検査器を作らず、内部宣言で通常の型検査を行ってから型付きリテラルへ評価・展開する。参照依存は明示スタック、binary演算はAPFloat。decimal演算は拒否する。
- aggregateは既存literal/frame/relocateを再利用し、専用global templateやConst型付き命令を増やさない。`private const`も通常の可視性に揃える。
- 定数の直接借用は単一段階の完全適用・loanを持たない結果だけ。保存・返却・段階適用では明示let束縛を要求し、static lifetimeを導入しない。
- 定数の依存深さ128・探索1024件・評価／展開1048576ノードの上限超過はD06の専用診断E1026を使う。
- G07のJSONにはチケットで検討可能としたserde_jsonを採用する。独自JSONパーサーを作らず、深さ128とメッセージ16 MiBの上限を保つ。
- G08は既存Span/TrapSourceとmetadata採番を共有する。decimalはBID保存ビットをunsignedとして表示。macOSのdebug task objectは対応版llvm-linkでIR結合し、executableはフラットなoutput.dwarfを保存する。O3の公開wrapperにもscopeを保持する。
- E03はrelative_pathを保存して全ソースを再帰探索。file入力の親とdirectory入力を明示rootにし、上位を推測しない。std予約は先頭namespace、LSPは明示workspace rootを使用する。fmtの対象は既存どおり直下のみ。
- C06はcompiler登録のopaque標準recordとVecで表し、構築・field・pattern・updateを定義モジュールに限定する。型と所有権の走査を重複させず、既存の確保・移動・clone/dropを共有する。
- C06のSet.unionと予約語の衝突を解決するため、unionだけはmodule関数の宣言名とdot後のmember名でも許可する。変数・型・moduleの名前としては引き続き予約する。
- C06のsingletonは無制約で任意の1要素を保持。検索・更新・集合演算の比較時はEqの反射性を確認してNaNをトラップし、Ordの一貫性は利用者のinstance契約とする。Set.unionは線形の出力領域を一つ確保し、左の代表値を保持する。
- C07はopaqueなSeqを常にnon-Copyにし、Maybeの単一要素と通常closureの遅延stepで表す。明示的なSeq.defer/unfoldをユーザー反復の構築口とする。
- C07のnextは所有closureを直接呼ぶbuiltin。forは次状態を先に復元する既存while/matchへ展開し、filterは借用述語へ修正する。List.iterは参照Vecの準備O(n)、所有状態の移送による反復O(n)を採用する。
- A09の共有record fieldはC04の交差寿命とValue.loansを継承する。古い「借用入力は必ず1個」という制限へ戻さず、複数入力の場合は全入力の寿命を保持する。排他参照fieldは region 付きの直接フィールドと、その record を入れ子にしたフィールドに限り許可する（A13。D-38 で改めた）。
- A09の名前付きregionは値全体に一つとし、defの返却元を本体loanで検証する。直接完全適用のみ指定入力へ寿命を縮小し、関数値は保守的に全入力を保持する。独立複数region・高階region型は後続段階として拒否する。
- F05は同梱`Array.sum<i64>`のnative exe/objectだけをC11のCPU runtimeへ下げる。既存LLVM API/IR出力は独立経路を維持。検出・atomic cache・target属性はCに集約し、floatやuser関数を多重化しない。
- E06は通常の型付き関数内のHostCall wrapperで表現し、関数値/部分適用を再利用する。E05の共有buffer/recordと所有結果ABIを共用し、ホストはborrow保持禁止・所有結果のallocator契約を守る。未使用externを出力rootにしない。
- B05はand!右辺をすべて順序付き生成letへ評価してからMergeSources/Bind2へ渡す。Bind2が選べなければMergeSourcesを要求し、ネストBindへ置換しない。optional operationの意味はビルダーが定義する。
- D05はMath.fmaだけに単一丸めを許す。AArch64 native f32/f64は保証されたFMA命令、それ以外は同梱整数ランタイム。大きな指数差は符号付きstickyで縮約し、decimalも一度だけpackする。Arrayの集計木/左順を固定し、sum_kahanの算法はNeumaierと明記する。
- G09はdocでもMainなしdirectoryをライブラリとして扱う。署名ASTから公開APIを描画し、constの値・instance・privateを出さない。既存出力は専用markerで所有権を確認して全体置換する。stdの完全なファイル構成を入力可能とし、builtin一覧とdoc-testは別責務とする。
- E03の再帰探索では独立したfixture/benchmark projectを同じrootに混ぜない。旧来の直下単独ファイルSemantics/Mixはハーネスが一時rootへコピーして実行し、言語側の探索契約を弱めない。

### D-29 P3 の実装判断

- 2026-09-28、利用者がP3全件と必要な設計判断を承認。各チケットの対象段階を実装し、未検証のプラットフォームや後続段階を完了と混同しない。
- E04は指定どおりlocal pathとstrict manifestのみ。PackageIdはSourceFile.packageに保持し、コンパイラのUser/Std分類は変更しない。依存の名前空間をrelative_pathへ付けて既存の解析・可視性・所有権を再利用する。
- manifestも読み込み・出力保護対象とする。全graphでpackage1024・深さ128・source4096、名前空間衝突・循環・symlinkを拒否する。git、lockfile、build scriptは導入しない。
- F06は手書きLLVM queueではなくfreestanding C11 runtimeをClangで生成する。既存callback ABIとallocatorを共有し、threadsだけshared/import-memory、atomic、heap lock wrapperを有効にする。
- Nodeホストは明示countを一回初期化し、初回groupのspawn_workersで同数を起動する。各instanceの__stack_pointerを256KiB sliceへ設定する。失敗はpoisonで全waitを解除し、再利用しない。browser本番glueは対象外。
- F07のPhase 1は型付きkernel抽出・CPU参照・WGSL生成。WGSL仕様には具体的i64/f64がなく、floatのfusion/reassociation/subnormal差を許すため、strict shaderはi32/i32uだけとする。CPU参照では元の64-bit/floatを保持し、明示GPU要求をCPU成功へ変換しない。動的除算・剰余もtrap契約が異なるのでshaderでは拒否する。
- Gpu.Deviceは各操作へ共有借用で渡すnon-Copy型。初版のGpu.requestは明示CpuReferenceだけに成功し、GPU backendはUnavailable。WebGPU host試作は別の実device所有経路として実行検証する。initのcallback indexはi32、count上限はi32::MAX。--emit wgslは単一exportの専用projectを受ける。
- A10は既存Apply(Ident,args)のheadに`'f`を保持し、全AST移行を避ける。class.kindと制約からのkind環境、Partial/Applicationのcompile-time型、既存Inference/Specializerを使う。型の四wordサイズと全解析上限を維持する。
- A10の部分適用は末尾引数を固定する。Result<'a,'e>の既存順序とチケットの「エラー型固定」を両立させ、Functor<Result<string>>はResult<'a,string>を表す。通常の完全適用の意味は変更しない。
- A10初版は値型を引数に取るrank-1 constructor、HKT method固有の値型変数、default/条件付きinstance、generic関数制約を対象とする。higher-order kind引数、HKT aliases、標準Functor導入は対象外。値型位置の未適用record/unionはE1015、対象外aliasの診断は従来E1024。
- B06は既存native/threads poolの同一groupでcallback種別と最小failureを管理する。queue lock内で未配布範囲を切り、開始済みをjoinする。通常Task.parallelの軽いcompletion経路は維持。startedはi8で、一時Result領域はraw freeのみとして移動済みpayloadの再dropを避ける。
- G10のMetadataExt::file_index/volume_serial_numberはstableでwindows_by_handle不安定APIだったため、Windows限定same-file 1.0.6のsafe APIを採用する。unsafe禁止・stable Rust・hardlink出力保護を維持する。COFFのtask/CPU runtime埋め込みobjectは保守的にE2002、exeと明示LLVM+runtimeリンクを提供する。
- G11は既存serde_jsonでmetadataを保存し、SHA-256はチケット指定どおり内部実装してNIST検証する。compiler実行file全体のdigestを使い、git commit/mtimeだけの開発版識別は採用しない。
- G11は毎回解析/IRを生成してから生成物cacheを照合する。全source/manifest/origin、IR/runtime、options/action、tool binary/version/env、nativeCPU macroを長さ付きhashへ含める。macOS debug exeだけoutput pathを含めてDWARF参照を保つ。
- G11は専用marker、digest/size検証、非待機writer lock、directory atomic rename、既存publish_outputsを使う。部分/破損entryはmiss、cache障害はW2001。GCは2GiB/30日のsoft上限で4096走査/128削除まで。
- A10のinstance head変数とmethod固有変数は別binderとして扱い、同名ならinstance関数生成時にmethod側を内部名へalpha分離する。Result<'a>の'aがmapの入力型'aを捕捉しない。

### D-30 第2期計画の仮割り当て（未承認）

- 2026-09-29、C/C++・Rust・C#/F# との比較で見える劣位を改善する第2期のチケット（A12–A16、B07–B08、C08–C11、D07–D11、E08–E14、F08–F13、G12–G20）を起票した。
  調査時点はコミット `9012e92`。一覧と対応表は [README の第2期](README.md#第2期-他言語比較で見える劣位の改善計画) にある。
- 以下は計画上の仮割り当てで、人間の承認と各チケットの着手前レビューを経て確定する。確定したら該当行を D-07（std）・D-15（予約語）・D-16（診断コード）へ移し、この表から削除する。
- 既存の予約語の組み合わせで表せる構文（`extern type`、`extern "symbol" def`、`const def`、`const N: i64`）を優先し、新しい予約語を増やさない。G19 の edition を導入した後は、新しい予約語を新しい edition でだけ予約する。
- 言語の意味や既存の決定を変える提案は承認まで着手しない: C10 Phase 2（参照カウントの導入）、D11 Phase 2（static データ。D-28 の変更）。C08（D-13 の変更）と A16（`[T; N]` の再導入）は D-39 で承認済み。
- 新しいホスト機能（WASI、乱数 seed の設定、非同期の再開、GPU runtime）は D-18 に従い明示的な opt-in とし、既定の WASM に import を追加しない。

| 種別 | 仮割り当て | チケット |
|---|---|---|
| 予約語 | `bench` | G18 |
| 診断 | `E2007` 依存の取得・検証の失敗（lockfile の不一致、キャッシュの欠落） | E10 |
| 警告 | `W1005` 非推奨の宣言の使用 | G19 |
| 警告 | `W2002` bindgen で変換できない C 宣言の省略 | E11 |
| 組み込みクラス | `Encode`／`Decode` | D08 |
| 組み込みクラス | `Sync`（仮称） | F10 |
| std | `Arena` | C10 |
| std | `Matrix` | C11 |
| std | `Json` | D08 |
| std | `Regex`／`Unicode` | D09 |
| std | `Net` | E09 |
| std | `Async` | B08 |
| std | `Atomic`／`Mutex`／`Channel` | F10 |
| std | `Bench` | G18 |

2026-09-29 の詳細化で、各チケットが次の名前を仮に決めた（衝突を避けるための台帳。確定は各チケットの決定事項と承認に従う）。
`要承認` の欄は、そのチケットで承認が必要な名前であることを示す。

| 種別 | 仮の名前 | チケット | 要承認 |
| --- | --- | --- | --- |
| 構文 | `extern "symbol" def`・`extern "module" "symbol" def` | E12 | いいえ（承認済み・実装済み） |
| 構文 | 文書コメントの `@deprecated` タグ、manifest の `edition` キー | G19 | はい |
| 構文 | `const def` | D11 | いいえ（Phase 1） |
| 組み込みクラス・builtin | `AtomicValue`、`Task.scope`、構築関数 `create`（`new` は予約語） | F10 | はい |
| std | `Gpu.map_relaxed`・`Gpu.init_relaxed` | F09 | はい |
| std | `Bench.now`・`Bench.consume`・`Bench.with`・`Bench.of` | G18 | はい（`bench` と共に） |
| std | `Json.Numeral`（`Json` の数値の record） | D08 | はい（`Json` と共に） |
| 予約モジュール | `Regex`・`Unicode` | D09 | はい |
| サブコマンド | `tsuzuri watch`・`tsuzuri serve` | PB06 | `serve` だけ |
| サブコマンド | `tsuzuri bindgen` | E11 | はい |
| サブコマンド | `tsuzuri fetch` | E10 | はい |
| サブコマンド | `tsuzuri repl`（Phase 2 の `tsuzuri script` は要承認） | G13 | Phase 2 だけ |
| サブコマンド | `tsuzuri bench` | G18 | はい |
| サブコマンド | `tsuzuri toolchain info` | G14 | いいえ |
| CLI | `--allocator system\|host\|counting`・`--freestanding`（F13、実装済み）、値 `small`（PM05） | F13・PM05 | `small` を既定にする段だけ |
| CLI | `--wasm-max-memory`・`--wasm-stack-size`、manifest の `[wasm]`（`max-memory`・`stack-size`） | F11 | いいえ（Phase 2 承認済み・実装済み） |
| CLI | `--target wasm64` | F11 | いいえ（Phase 2 承認済み・実装済み） |
| CLI | `--emit bitcode` | PR08 | いいえ |
| CLI | `--emit bindings-js`（出力 `<name>.mjs`・`<name>.d.mts`） | E13 | いいえ |
| CLI | `--emit wgsl-relaxed`・`--wasm-feature webgpu` | F09 | はい |
| CLI | `--wasm-feature tail-call` | PM09 | はい |
| CLI | `--trap-mode return` | E14 | いいえ（Phase 2 承認済み・実装済み） |
| CLI | `--link`・`-l`・`-L`、manifest の `[native]` | E12 | いいえ（承認済み・実装済み） |
| CLI | `--profile-generate`・`--profile-use` | PR07 | いいえ |
| CLI | `-Os`・`-Oz`・`--strip` | PM08 | いいえ |
| CLI | `--backend llvm\|fast` | PB05 | はい |
| CLI | `--samples`・`--coverage` | G18 | `bench` と共に |
| CLI | `--no-server`・`--idle-timeout` | PB06 | はい（Phase 2） |
| 環境変数 | `TSUZURI_THREADS` | PB04 | いいえ |
| 環境変数 | `TSUZURI_CODEGEN_UNITS` | PB07 | はい（既定の変更） |
| 環境変数 | `TSUZURI_SERVER_DIR` | PB06 | はい（Phase 2） |
| 環境変数 | `TSUZURI_SYSROOT`・`TSUZURI_CROSS_LINK`（テスト用） | G15 | いいえ |
| 環境変数 | `TSUZURI_LLVM_PROFDATA` | PR07 | いいえ |
| 環境変数 | `TSUZURI_TIME_PASSES` | PX01 | いいえ |
| 環境変数 | `TSUZURI_ZIG`・`TSUZURI_GO` | PX02 | 計測機への導入だけ |
| 環境変数 | `TSUZURI_TEST_ALLOCATOR`（テスト用） | F13 | いいえ |
| 環境変数 | `TSUZURI_LLDB`（テスト用） | G16 | いいえ |
| 環境変数 | `TSUZURI_BASELINE`（テスト用） | PM03 | はい（PM03 全体） |
| 環境変数 | `TSUZURI_TEST_WASM_TARGET`（テスト用。`tests/features.mjs` の WASM を `wasm64` で実行） | F11 | いいえ |
| 公開記号 | `tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc`（WASM は `tsuzuri_heap` の `alloc`・`free`・`realloc`）、`tsuzuri_alloc_stats` と `tsuzuri_allocation_stats` | F13 | いいえ（実装済み） |
| 公開記号 | `tsuzuri_cpu_<op>_<type>`（`tz_cpu_level`・`TZ_CPU_PICK`・`CPU_KERNELS`）、`@cpu` 関数の stub が呼ぶ `tsuzuri_cpu_pick` | F08・PR05 | いいえ（F08 は実装済み） |
| 公開記号 | `tsuzuri_try_<name>`・`tsuzuri_trap_info`（`tsuzuri_boundary_run`・`tsuzuri_trap_raise`・`tsuzuri_tracked_*` は runtime の内部） | E14 | いいえ（Phase 2 承認済み・実装済み） |
| 公開記号 | wasm global `tsuzuri_stack_base`・`tsuzuri_stack_top`（threads の worker の stack の範囲） | F11 | いいえ（Phase 2 実装済み） |
| ランタイム | `string_scalar.ll`・`string_v128.ll`（PR05）、`string_latin1.ll`（PM03）、`integer.ll`（PM08）、`heap-host.ll`・`heap-counting.ll`（F13、実装済み）、`net.c`（E09）、`heap-wasm64.ll`（F11、実装済み）。`format.ll`（D07）・`os.c`・`os-wasi.c`（E08）は D-32 で確定 | 各チケット | 各チケットの承認に従う |

新しい `.ll` を足すときは §2.2 の `.gitignore` の例外行と `scripts/check-runtime-includes.sh` を忘れない。
Phase 2 以降の仮の名前（B08 の opt-in フラグ・WASM import、E14 の `Trap`・`TrapInfo` など）は、承認のときに割り当てる。

### D-31 利用者定義の解放（B07）

- 2026-10-02、利用者の「B07 の実装を完遂して」という依頼を D1・D2・D4 の承認として扱い、Phase 1 を実装した。組み込みクラス `Drop` は D-07 へ移した。
- LLVM は move 済みの領域を `zeroinitializer` で埋め、その解放を無処理にする規則で動く。この規則を変えずに利用者の `drop` を一度だけ呼ぶため、
  非再帰の Drop 型は値の末尾に `i8` の生存フラグを持ち、構築で 1 にする。drop glue はフラグが 0 の値で `drop` を呼ばない。
  全 case が nullary の Drop union は素の `i32` tag ではなく `General(0)` の配置にする。
- 再帰する Drop union は先頭の nullary case もノードに置く（null は move 済みだけ）。型ごとの drop helper がノードごとに `drop` を呼ぶ。
- Drop 型は ABI の scalar record にしない。一時値から Copy の field・payload を読むときは、値を壊さず全体を解放する（非 Copy の取り出しは `E1012`）。
- record の field は元々代入できないので、`drop` 本体での引数全体の置換の診断は field への代入ではなく読み出し・借用を勧める。
- Drop の印は instance の収集直後に立て、instance の superclass 検査（`Classes::check_superclasses`）はその後に行う。
- 2026-10-02、利用者の「Phase 2 以降もすべて実装を完了させて」を受けて Phase 2 を実装した。
  - `use`／`use!` は予約語にしない（`use`・`use!` の後に名前と `=`／`:`、または `mut` が続くときだけ束縛）。意味は `let` と同じ lexical drop で、型に `Drop` を要求する。
    計算式の `use` はビルダーの `Using` を呼ばない（既存の `Using` は別のビルダーへの接続に使う）。
  - 新しい std モジュール `Owned`（D-07）。`Owned.drop`・`Owned.function`・`Owned.call` は組み込み関数で、`std/Owned.tz` は opaque な
    non-Copy の `Owned.Function<'a, 'b>` だけを宣言する。std の関数を足すと生成関数の番号がずれ、Drop を使わないプログラムの IR が変わるため。
  - 関数値への Drop 型の捕捉（D5）は `Owned.function` の引数に直接書いた一引数のラムダだけに許す。捕捉は `Send`（参照なし）、
    本体は Copy でない捕捉値（drop glue のないハンドルを含む）を move できない（`E1012`）。環境は複製しないので `Owned.Function` は Copy でも `Capture` でもない。
  - `extern type` への直接の `Drop` は入れない。ハンドルの表現に生存フラグを足すと ABI が変わり、null／0 を move 済みの印にすると
    整数のハンドル 0 を閉じられない。record で包む方法（E12 D3）を維持する。
- 2026-10-03、PR #6 のレビューを受けて、`drop` 本体で引数を `ref mut` で渡すこと（値全体の排他的な再借用と、引数の参照そのものの move）も
  `E1012` にした。呼び出し先が値を置き換えると同じ `drop` が再び走る。所有の関数値の本体で move を禁じる捕捉値は、
  解放の要る値から Copy でない値へ広げた（ハンドルを呼び出しごとに閉じられないようにする）。

### D-32 第2期の先頭 3 チケット（E08・C09・D07）の確定

- 2026-10-03、利用者の「E08 / C09 / D07 の実装を完遂して。複数フェーズある場合にはすべてのフェーズを完了させること」と
  「なんらかの判断が必要な場合には、あなたが考えられる最高の選択をすることを常に許可します」を、3 チケットの `要承認` の決定事項すべての承認として扱い、
  Phase 1 と Phase 2（依存先が todo／blocked のものを除く）を実装した。確定した内容は次のとおり。詳細は各チケットの「実装と検証」にある。
- std モジュール（D-07 の表へ移した）: C09 の `HashMap`／`HashSet`、E08 の `File`／`Dir`／`Path`／`Env`／`Time`／`Random`／`Os`／`Process`、D07 の `Format`。
  予約モジュールは 32。利用者の同名のモジュールは `E1011` になる（既存のプログラムへの影響として文書に書いた）。
- 構文（D07 D1）: 文字列補間 `$"..."`・`u8$"..."` と穴 `{expr}`・`{expr:spec}`、`{{`・`}}`。予約語は増やさない（`$` は変更前は字句エラーだった）。
  書式指定は `[[fill]align][+][width][.precision][type]`（D07 D6）。
- 組み込みクラス（D07 D12）: `Format<'a>`（`format :: ref 'a -> ref string -> string`）。`BUILTIN_CLASSES` の末尾に足し、instance は利用者の record・union だけが持つ。
  record・union の穴に書式があるとき、instance があるか書式が符号・精度・型を含めば `Format` へ渡し（検証済みの正規の綴りを借用した静的な文字列で渡し、幅の処理は instance）、そうでなければ従来どおり `Display` の文字列にコンパイラが幅を適用する。
- ランタイム（D07 D9、E08 D6）: `format.ll`、`numeric.c` の `tz_soft_format_spec`（`numeric.ll`・`math.ll` を再生成）、`os.c`・`os-wasi.c`。
  `os.c` は IR が `@tsuzuri_os_` を宣言したときだけ連結し、`os-wasi.c` は `--wasm-host wasi` のときだけ連結する。`.gitignore` の例外行と `scripts/check-runtime-includes.sh` の対象に入っている。
- 診断コード: 新しいコードは足していない。`E2000`（既定の wasm32 が OS API に到達）・`E2002`（Windows の OS API）・`E2005`（`IO<i32>` 入口の非 0 の終了コード）の使う場面を広げた。
- CLI（D-18）: `--wasm-host wasi`（E08 D10）。WASI preview1 の到達した import だけを出し、既定の wasm32 の出力と import は変えない。
- `IO<i32>` の入口は値をプロセスの終了コードにする（E08 D9。`Os.exit` は作らない）。他の `IO<'a>` は従来どおり値を捨てて 0 で終わる。
- `File.Handle` は `Drop` にしない（E08 D12 の選択肢 (b)）。Copy の opaque な添字で、runtime の世代検査付きの表を指す。明示の `close` と `File.with_open` で閉じる。
  std の型は `Drop` の instance を持てず（`E1016`）、`Drop` の値は `let!` の継続をまたげない（`E1005`）ため。
- Windows の OS API は `E2002` のまま（E08 D11）。G10 が blocked で実行を検証できない。WASI preview2／component model は E13。
- C09 D11: seed 付きの `HashMap`／`HashSet`（SipHash-1-3 の鍵付きの仕上げ。`with_seed`・`randomized`）を足した。OS の seed は E08 の `Random.next_u64` で、取得失敗は報告かトラップで、固定 seed へ置き換えない。
  耐性は部分的（64 bit の `Hash.hash` の digest が衝突するキーは seed でも衝突する）。F08 の SIMD によるグループ探索は F08 の後。
- ベースラインの注意: 変更前の HEAD でも、`cargo test --locked` の全体実行では `bounds_nested_builder_expansion_not_just_source_syntax` が既定の 2 MiB の stack で溢れる。
  全体は `RUST_MIN_STACK=4194304` を付けて実行する。単独の実行（§3.1）は既定のままで通る。上限や stack の大きさのコードは変えていない。
- std の union の case 名は利用者のモジュールから無修飾で見え、利用者の active pattern の名前と `E1004` で衝突しうる。新しい std の union には `Left`・`Right`・`Plain` のような一般的な case 名を避け、型名の接頭辞を付ける（`Format.AlignLeft` など。D07）。
- 既存の不具合の修正: `Classes::matching_instance` が、型に未解決の変数が残る呼び出しで panic していた（未定義の名前と instance が同じプログラムにあると再現）。未解決の変数があるときは instance なしとして扱う。
  名前だけの穴（union の case・ゼロ引数の関数）が `E1013` になる不具合も直した（D07）。

### D-33 暗黙の複製の可視化（A15）と複数 region（A12）の確定

- 2026-10-03、利用者の「A15 / A12 の実装を完遂して。複数フェーズある場合には、すべてのフェーズを完了させること」を、A15 D3 と A12 D10・D13 の承認として扱い、
  両チケットの Phase 1 と Phase 2 を実装した。詳細は各チケットの「実装と検証」にある。
- 警告 `W1006`（D-16 へ移した）と CLI `--warn implicit-copy`（check・build・run・test）。`--warn` が受ける名前は `implicit-copy` だけで、
  既定では無効。有効でも生成コードは変わらない。複製の一覧 `copies::sites` は具体化後の module で所有権検査器を収集モードで再実行して作り、
  debug build は生成した暗黙の複製がすべて一覧にあることを検査する（PM07 はこの一覧を使う）。
- std: `Array.copy`・`List.copy`（新しい std モジュールはない）。LSP は `textDocument/inlayHint` で W1006 と同じ位置に複製の種類を返す。
  LSP は設定を受けないので、既定無効の警告は診断にしない。
- A12 D10: 関数あたりの region の上限は、parser の入れ子の上限と同じ 128 を正式な上限にした（64 へ下げない）。record あたりは 16（`u16` の mask）。
- A12 D13: region で量化した関数型 `({s} ref {s} T -> ref {s} T)` は名前付き関数の引数の型全体にだけ書ける。
  `Type::Function` には量化を入れず（region は型・単相化・IR に入らない。A12 D11）、契約は `CheckedFunction.callback_contracts` に置く。
  その関数は直接の完全適用でだけ使え、呼び出し側で渡す関数が契約を守ることを検査する。量化のない関数値は D-28 のとおり全入力の寿命を保持する。
- 新しい診断コード・予約語・ランタイムはない。寿命と region の誤りは `E1013`、上限は `E1017`。

### D-34 `_specs` の言語仕様への追従

- 2026-10-04、利用者の「`_specs` フォルダーに記載の仕様となるように、仕様変更や仕様追加を行なって」を、`_specs/` の
  literals・operators・functions・error-handling の承認として扱い、D-10 と D-15 をこの項のとおり改めた。
- 例外: `try ... with ... finally` は `Result<'T, 'E>` の式で、`'E` は組み込みクラス `Err` の instance（std の `Exception`）。
  送出するのは `@checked` の整数 overflow（`OverflowException`）だけで、同じ関数本体の最も内側の `try` への字句的な脱出として実装した。
  関数・ラムダの境界は越えず、捕まらなければトラップする。`finally` を持つ `try` から `break`／`continue` で出るのは `E1023`。
- 演算子: `>>`／`<<` を関数合成に変え、シフトは `<<<`／`>>>`（`>>>` は符号付きで算術、符号なしで論理）。
  ビット演算の旧表記 `&`・`|`・`^`・`~` は別名として残した。`not`・`ignore` は修飾なしの組み込み関数。
- リテラルと型: 接尾辞のない整数リテラルの既定を `i64` から `i32` に、`byte` を `i8` から `i8u` に変え、`sbyte` と `bigint`（std の `BigInt`）を足した。
- std の高階関数は F# と同じく関数を先に受け取る（`Array.map f xs`、`Array.fold f state xs`）。
- トップレベルの値で `Display` を持たないものは `E2004` にせず捨てる。
- 新しい診断コード・予約語はない（整数リテラルの型の誤りは `E1003`）。トラップの種類 `Overflow` を末尾に足した。

### D-35 名前空間（`namespace`／`using`）

- 2026-10-04、`_specs/namespace-and-module.md` の承認として扱った。従来の「`namespace` 宣言なし・`open` なし」を改める。
  以下の `.` 区切りのパスは D-37 で `::` に改めた。
- `namespace A.B` はファイルの最初の宣言、`using A.B` はその後で他の宣言の前。どちらも 1 行で、キーワードの後やパスの途中の改行は `E0002`。
  どちらも文脈キーワードで、識別子としても使える（予約語は増やさない）。
- モジュールの完全名は名前空間 + ファイル名。宣言がなければパッケージの既定名前空間（manifest の新しい任意キー `namespace`、
  なければ package 名の PascalCase、manifest がなければ root フォルダー名）+ ディレクトリ。std はグローバル名前空間。
- コンパイラ内部の key（型付き IR・LLVM シンボルの修飾名）は、宣言のないファイルでは従来の相対パス名のまま。
  既定名前空間の内側を宣言したファイルは既定名前空間を除いた名前にし、IR を変えない。入口は key ではなく root の `Main.tz`。
- 解決順は C# と同じく参照元の名前空間、`using`（その名前空間の直下のモジュール名だけ）、外側の名前空間、グローバル、最後に key。
  モジュールのパスは同名の record／union も表す（`Sample.Point { ... }`）。`Ns.Module.Type` も受理する。
  修飾しない型名もこの順で利用者のモジュールの同名の型を探し（利用者の型と同じ順位）、なければ従来の一意な名前の検索。
- モジュール名は英大文字始まり（`E1011`）。同じ完全名の重複・不正な `using`（存在しない・モジュール・重複）は `E1011`、
  `using` の曖昧さは `E1004`（修飾しない型名も、ファイル自身がその型・クラスを宣言しない限り同じ）、位置の誤りは `E0002`。新しい診断コードはない。
- `def f :: T = \() -> body` は引数なしの定義。`tsuzuri new <dir> [--namespace NAME]` を足し、VS Code の New Project も使う。

### D-36 `def` の型注釈は `::` だけ

- 2026-10-05、利用者の「`def` の型指定は必ず `def name :: type`」を受け、D-34 で `_specs` の例に合わせて受理していた単一の `:`（`def name : T`）をやめた。
- `def`・`export def`・`private def`・`@literal def`・`extern def`・型付きの `and`・クラスのメソッドは、名前と型の間に `::` だけを書く。
  単一の `:` は `E0002`（`use '::' between a 'def' name and its type`）。`_specs` の literals・operators・error-handling の例も `::` に直した。
- `let`・`const`・フィールド・引数の型注釈と、制約の `@'T : Class` は従来どおり `:`。新しい診断コード・予約語はない。

### D-37 名前空間のパスは `::` でつなぐ

- 2026-10-05、利用者の「完全修飾では namespace と module 名、入れ子の namespace を `::` でつなぐ」と更新された `_specs/namespace-and-module.md` を受け、D-35 の `.` 区切りを改めた。
- `namespace Sample::Features`、`using Sample::Features`、`Sample::Features::Shape.area`、`Sample::Point { ... }`。モジュールとそのメンバー（関数・型・case・クラス）、値のフィールドは従来どおり `.`。
  manifest の `namespace = "Acme::Tools"` と `tsuzuri new --namespace Acme::Tools`、ディレクトリのモジュール（`Geometry::Point.distance`）も同じ。
- 名前空間を `.` でつなぐ従来の書き方は受理しない。宣言は `E0002`、参照は `E1002`／`E1004`、manifest は `E1011`、`new` は `E2000` で、診断は `::` の書き方を示す。
- `::` は `def` の型注釈とリストの cons にも使うため、lexer が空白なしの `Ident::Ident` の連鎖のうち、最後の要素が英大文字始まりのものと
  `namespace`／`using` のパスだけを `PathSep` にする。`def`／`rec`／`and` の宣言名の直後と `x::xs` は従来の `::`。
- 内部の key・完全名・IR シンボルは `.` 区切りのままで IR は変わらない。コンパイラが key で組み立てる修飾名は先頭の `::` で利用者のパスと区別する。
  診断・LSP・`tsuzuri doc` の見出し・型の表示（`expected Geometry::Point`）・`tsuzuri test` の一覧と結果と filter のモジュール名は、名前空間を `::` で表示する。
- key としての検索はモジュール名だけになった。新しい診断コード・予約語はない。

### D-38 排他借用フィールド（A13）の確定

- 2026-10-06、利用者の「A13 の実装を完遂して。…複数フェーズある場合には、すべてのフェーズを完了させること」を、A13 D1・D5 と Phase 2 の承認として扱い、
  D-28 の「排他参照fieldは禁止する」を改めた。詳細はチケットの「実装と検証」にある。
- record のフィールドに region 付きの排他参照 `ref mut {r} T` と、それを持つ record（region の適用は必須）を置ける。排他の region は
  record 内の一つのフィールドの一つの位置だけが使う。配列・リスト・Vec・タプル・union payload・共有参照の内側と、型変数の具体化は引き続き拒否する。
  排他借用を持つ record は既存の定義どおり非 Copy・Capture 不可・Send 不可。
- `ref mut c.f` と呼び出し時の補完は一段の貸し直しで、`c` が `let` の束縛でもよい（D5）。貸し直しの生存中は同じ参照先への経路と `c` の move だけを拒否する。
  共有参照を経由した変更・排他貸し直しは `E1014`。ガード中のパターン束縛は既存の alias で健全に追跡されるので、チケットの M7 は実装しない。
- 引数の外部 loan の可変性は A12 の slot ごとに決める（排他 region の slot と `ref mut` 引数）。書き込みの可否はその場所に重なる経路の loan だけで判定する。
  複数 region の record の `let mut` 束縛は A12 のとおり全 region をまとめて扱う（同じ式での共有フィールドの読み出しと排他フィールドの貸し直しは `E1014` になりうる）。
- Phase 2: 参照先の region を参照の後ろに書く（`ref mut {r} T {s}`。`s` は `r` と別名。引数・戻り値・フィールド）。参照先から読んだ借用は `s` を持ち、
  参照を経由した借用集約型の置換は、ローカルの所有者なら新旧の借用を合わせて保持し、引数の参照先なら `s` を持つ入力の借用だけを受け付ける。
  参照先へ入力を格納しうる関数は直接の完全適用だけで、呼び出し側が格納される入力の loan を参照先の所有者へ加える。
  region 間の outlives 制約は表さず、包含関係は既存の loan と NLL で保つ。参照先は排他借用を持てない。
- region は型・単相化・生成 IR に入らない（既存の fixture の IR は変わらない）。新しい診断コード・予約語・ランタイムはない（`E1013`・`E1014`）。

### D-39 第2期の 5 チケット（C08・A16・A14・F13・F08）の確定

- 2026-10-06、利用者の「C08、A14、A16、F13、F08 の実装をすべて完遂して。…複数フェーズある場合には、すべてのフェーズを完了させること。
  …判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」を、5 チケットの `要承認` の決定事項すべての承認として扱った。
  詳細は各チケットの「実装と検証」にある。
- C08 D1: D-13 を改めた。排他スライス `ref mut [T..]` は型検査器の中で `Reference(ArrayView(T), true)` で、`ArrayView` は排他参照の参照先にだけ現れる。
  表現は共有スライスと同じ `%tz.array`。新しい診断コード・予約語・ランタイム関数・`TrapKind` はない。
  チケットの D5（変換は引数だけ）は緩め、明示の `ref mut xs` は排他スライスを期待する位置（`let` の注釈を含む）で配列全体のスライスになる。
- C08 D11（Phase 2）: `Parallel.for_each_chunk :: Send<'a> => i64 -> (i64 -> ref mut ['a..] -> unit) -> ref mut ['a..] -> unit`。
  排他スライス自体は Send にしない（task への捕捉は従来どおり `E1013`）。互いに素なチャンクを fork/join の中でだけ貸す組み込みで、F10 の同期規則には触れない。
- A16 D1: 固定長配列 `[T; N]`（N は 0–1024）を導入した（D-03 の `fixed[N,T]`、D-07 の std `FixedArray`、D-13 のスライス化、D-15 の型構文）。
  型は `Type::FixedArray(Box<Type>, Box<Type>)` で、長さは `Type::Length(n)` か長さパラメーター `Type::Variable("#N")`。LLVM では `[N x T]` の値で、ヒープを使わない。
  新しい予約語・診断コード・ランタイム関数・`TrapKind` はない（添字は既存の `BoundsCheck`）。配列パターン・`for` による列挙・構造的インスタンス・排他スライスは持たない。
- A16 D10（Phase 2）: record・union・型エイリアスの長さパラメーターは `const N: i64` で宣言し、関数の型注釈の未宣言の長さは `'a` と同じく暗黙のパラメーターにする。
  D-02 の「型引数は完全な型」の例外として、型引数の位置に長さ（十進の整数リテラル・`i64` 定数・長さパラメーター）を書ける。長さにできる定数は初期化式が整数リテラルの `i64` 定数だけ。
  長さの算術はない。E12 の公開 ABI では、スカラー型の固定長配列をレコードのフィールド（C の `T name[N]`）に限って許す。
- A14 D1: 予約語 `dyn`（D-15）、型 `dyn C`、構築 `Dyn.of`（D-07 の予約モジュール `Dyn`）、診断 `E1028`（D-16）を確定し、D-30 の仮割り当てから外した。
  型は葉の `Type::Dyn(Box<DynType>)`（クラス名の列と印 `copy`・`send`・`borrowed`）で、値は `%tz.dyn = { data, vtable }`。vtable は `(クラスの列, 格納型)` ごとの定数で、
  D-03 の正規名を使う `@"tz.vtable.{クラス}[{型}]"`。dyn 型を書かないプログラムの IR は変わらない。新しいランタイム・`TrapKind`・WASM import はない。
- A14 Phase 2: 複数クラス `dyn (C, D)`、印 `Copy`（clone slot）と `Send`、region 付きの `dyn C {r}`（借用を格納し、Send にならない）、
  `Dyn.of` による vtable の差し替えだけのアップキャスト、組み込み実装の slot（組み込みラッパー関数）。`Copy`・`Send` は型クラスではなく値の性質の印として並べる。
  downcast・実行時の型情報・dyn 値の比較・host ABI での受け渡しは持たない。
- F13 D9: Phase 2・Phase 3 を承認として扱った。`--allocator system|host|counting` は build 専用で、host と counting は object・LLVM IR・header・WASM 出力だけ。
  heap runtime は一つの `.ll` を `emit_program` が選ぶ（`heap-host.ll`、`heap-counting.ll` は対象の heap を `@tz.alloc.base` などへ改名して包む）。
  host と counting は各ブロックに 16 バイトのサイズのヘッダーを置き、align は 16。WASM の host allocator は D-18 の明示の opt-in として import モジュール `tsuzuri_heap` を使い、
  threads と `--trap-mode return` とは併用しない。`--freestanding` は `--allocator host` を必須にし、C ライブラリを要する runtime を使うプログラムを `E2000` にする。
  WASM の size-class allocator は PM05 へ移した（F13 D9 の見直し提案）。新しい診断コード・予約語・`TrapKind` はない。
- F08 D7（Phase 2）: `SimdType` に `width`（128・256）を足し、256-bit の 14 型（`i8x32`〜`i64ux4`、`f32x8`、`f64x4`、`mask8x32`〜`mask64x4`）を同じ型族の lane 数違いにした。
  `Simd.of_lanes32` と `Simd.store :: ref mut [lane..] -> i64 -> v -> unit`（全 lane の境界を書き込み前に検査）を足した。256-bit 型の格納先は 16 バイト境界なので、
  256-bit ベクトルを含む型の load／store は `align 16` を明示する。512-bit 型は足さない。
- F08 D8（Phase 3）: 利用者関数の多版化の構文は、既存の `@literal`・`@checked` と同じ属性の形 `@cpu ["avx2", "sve"]` を `def` シグネチャの前に置く形にした
  （起票時の既定案 `for cpu [avx2]` は型の後ろに新しい修飾を足すため採らない）。名前は `TSUZURI_CPU_FORCE` と同じ `"sse4.2"`・`"avx2"`・`"avx512"`・`"sve"`・`"sve2"` の文字列で、
  build 先で選べない名前は無視する。版は trap 計装後の IR を複製して `"target-features"` を付け、stub が `tsuzuri_cpu_pick` で選ぶ。版ごとに 256-bit ベクトルの
  受け渡しレジスタが異なるため、シグネチャと関数値の呼び出しに 256-bit ベクトルを通すことを `E1005` で拒否し、直接呼ぶ関数は同じ level の版を作る。
  新しい予約語・診断コード・`TrapKind` はない。
- F08 D9: AVX-512（F・BW・CD・DQ・VL と XCR0 の opmask・ZMM 状態）を feature bit2、SVE／SVE2 を Linux の `getauxval` で bit16／bit17 にした。
  実機で測れないため、同梱 kernel の自動選択は AVX2 までで、AVX-512 と SVE の kernel は `TSUZURI_CPU_FORCE` の指定時だけ使う。`@cpu` で明示した版は自動でも選ぶ。

## 10. 完了の定義（全チケット共通）

- [ ] 仕様どおりに動作し、仕様外の入力は安定した診断コードで拒否される。
- [ ] `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked` が通る。
- [ ] コード生成に関わる変更では、関連する `node tests/*.mjs target/release/tsuzuri` がすべて通り、
      native/WASM × `-O0`/`-O3`、解放追跡（`live == 0`）、WASM インポートなしを確認した。
- [ ] 生成 IR が決定的（同じ入力で 2 回ビルドして一致）で、intrinsic 宣言が重複しない。
- [ ] 既存のテスト・例・ベンチマークの `--quick` が壊れていない。
- [ ] `README.md`／`docs/language.md`／`docs/architecture.md`（必要なら `docs/benchmarks.md`）を更新し、
      「未実装」一覧から実装した項目を外した。
- [ ] `_tsuzuri/language-reference/` の関連ページを §8 の手順で最新にし、`node scripts/check-docs.mjs <変更したページ>` が通る
      （更新が不要なら、完了報告にその理由を書いた）。
- [ ] 性能を主張する場合は実測と生成コードの確認結果を `docs/benchmarks.md` に記録した。
- [ ] `_features/README.md` の状態欄を更新し、チケットの「未決事項」に実装時の判断を追記した。
- [ ] 完了したチケットを `_features/_completed/` へ移動し、参照リンクを更新した。
- [ ] `要承認` の項目に承認なしで着手していない。§3.1 の回帰テストが通る。
- [ ] §13.2 の書式で完了報告を書いた（チケット末尾の `## 実装と検証（YYYY-MM-DD）` と人間への報告）。

## 11. 小さいモデルで実装するときの追加指示

- 一度に変更するのは 1 段（例: 構文だけ）。段ごとに `cargo test --locked` を実行する。
- 既存の似た機能（レコード、タプル、`Task`、`NewLiteral`）の実装を必ず読み、同じ形で書く。新しい抽象化を発明しない。
- `unreachable!()` を追加する前に、型検査でその状態が本当に排除されているかを確認する。
- エラーを握りつぶしたり、テストの期待値をコンパイラの現在の出力に合わせて書き換えたりしない。
- 迷ったら、チケットの「未決事項」の既定案 → この台帳 → 最も保守的な選択（拒否）の順に従う。
  詳細化済みのチケットでは「決定事項」の既定案に従い、`要承認` の項目は承認まで着手しない。
- チケットの手順は「変更 → 内容 → 確認」の順に一つずつ進め、確認のコマンドの結果（通過したテストの数を含む）を控えておく。
  控えは §13 の完了報告にそのまま使う。

### 11.1 既知の落とし穴（過去の実装から）

- **stack の深さ。** debug ビルドのテストは 2 MiB の stack で動く。`syntax` の enum の variant を小さく保ち（大きい field は `Box`）、
  `Parser::primary` の分岐を小さく保ち、引数の検査の入口は小さな振り分けにして重い処理は再帰しない補助関数へ出す。
  parser の再帰する補助関数は入れ子の数を必ず増減する（減らし忘れて深さ 128 の前に溢れた例がある）。
  上限や stack の大きさを上げてテストを通してはいけない。回帰テストは §3.1 の 3 つ（`bounds_type_growing_polymorphic_recursion`、
  `bounds_recursive_and_flat_expression_depth`、`bounds_nested_builder_expansion_not_just_source_syntax`）。
- **`_ =>` の fallback。** 新しい variant を黙って無視する match が多い。§6.9 の手順で全出現を確かめる。
- **std の generic 関数と特殊化の予算。** 単相化は利用者の非 generic 関数からだけ始める。std の generic 関数を先に特殊化すると、
  利用者の 1,024 件の予算を消費する（`honors_the_exact_specialization_limit`）。std に API を足したら必ずこのテストを実行する。
- **LLVM の型の出力順。** enum の別名（`= type i32`）は、どの record／union の struct 定義よりも前に出す（LLVM は非 struct の別名を前方参照できない）。
- **生成ランタイム。** `numeric.ll`・`math.ll` は手で直さず、§2.2 の手順で両方を作り直す。`Globals::FIRST_METADATA` の範囲を保つ。
  数値ランタイムに静的な表を足した形が Clang 23 の wasm32 `-O0` で不正なコードになった例がある。新しいランタイムの形は
  native と WASM の `-O0`／`-O3` で必ず確かめる。
- **Node のバージョン。** Node 20.19.6 は BigInt の重い suite で V8 の `RepresentationChangerError` により異常終了することがある。Node 24 を使う（§2.1）。
- **macOS の再配置可能リンク。** `clang -r -nostdlib` は weak／hidden の記号を局所化するので、`-Wl,-keep_private_externs` を保つ。
- **閉包の immediate capture。** 生成・stack 上の閉包・adapter・借用読み出し・clone／drop のすべてが同じ規則に従う必要がある
  （`tests/primitives.mjs`・`tests/tasks.mjs` で検査）。
- **複製の省略。** `clones_on_take` が Copy の複製を省くのは、一度だけ使う局所変数の全体だけ（`Field` の場所は対象外）。
- **整数の cast。** 整数から整数への cast はビットを保つ（幅が変わるときは切り捨て／符号拡張）。飽和するのは浮動小数点から整数だけ。
- **再帰的なソース探索。** E03 以降、ディレクトリ内のソースは再帰的に読み込まれる。テストのプロジェクトは一つずつ別の一時 root に置く
  （既存の harness は fixture を一時ディレクトリへ写してから使う）。
- **期待値の源。** テストの期待値は独立した参照（JavaScript の BigInt、Python、C）から作り、コンパイラの現在の出力を写さない。
- **テストの件数。** `cargo test --locked <pattern>` は 0 件でも成功する（§3.1）。
- **文書。** 機能を変えたら、同じ PR で `_tsuzuri/language-reference/` の関連ページを直す（§8）。変えたページは
  `node scripts/check-docs.mjs <ページ>` で検証する。

## 12. Tsuzuri 構文の早見表と落とし穴

fixture・例・再現用のソースを書くときの早見表です。仕様は `docs/language.md`、実行可能な例は `_tsuzuri/language-reference/**/*.md`
（`scripts/check-docs.mjs` で検証済み）と `tests/fixtures/**` にあります。**書いたソースは必ず `tsuzuri check` か `run` で確かめます。**

次のプログラムは 2026-09-29 に release コンパイラで `target/release/tsuzuri run <dir>` を実行し、`33` を出すことを確かめました。

```tsuzuri
union Reading = Missing | Value of i64

record Point { x: i64, y: i64 }

def twice :: Add<'a> -> 'a = \v -> v + v

def describe :: Reading -> i64 = \reading ->
    match reading with
    | Missing -> 0
    | Value v -> v

def first :: (i64 * i64) -> i64 = \pair ->
    match pair with
    | (left, _) -> left

def classify :: i64 -> i64 = \n ->
    if n < 0 then
        -1
    elif n == 0 then
        0
    else
        1

def sum_to :: i64 -> i64 = \count ->
    let mut total = 0
    for i in 0i64 .. (count - 1) do
        total = total + i
    total

export def answer :: i64 -> i64 = \seed -> seed + 1

let point: Point = Point { x: 3, y: 4 }
let text = to_string (twice 21)
let shown = Display.display point.x
let bytes = u8"abc"
let magnitude = Int.abs (-5i64) + (Math.abs (-2.0) |> to_int)
describe (Value 10) + first (5, 6) + classify (-3) + sum_to 4 + text.length + shown.length + bytes.length + magnitude
```

要点:

- 定義は `def name :: Type = \args -> body`。再帰は `def rec` と `and name :: Type = ...`。引数のない定義は `def name :: Type = expression`。
- 制約付きの generic は制約を型の位置に書く（`Add<'a> -> 'a`）。
- 値の型注釈は `let name: Type = value`。可変な局所変数は `let mut` と `=` の代入。
- `for x in a .. b` は **両端を含む**。長さ `n` の配列の添字は `0i64 .. (n - 1)`。`for name = a to b` は i32 だけ。
- 複数行の条件の連鎖は `if` ／ `elif` ／ `else`。同じ行の `else if` は使えるが、複数行の `else` の中の `if` は連鎖ではなくブロックになる。
- 文字列への変換は `to_string value`（消費）か `Display.display value`（借用）。UTF-8 の文字列リテラルは `u8"..."`。
- 絶対値: `abs` は f64 だけ、`Math.abs` は Float、符号付き整数は `Int.abs`（MIN は MIN のまま）。
- `export def` の引数・結果は 8／16／32／64-bit の整数、f32、f64、bool（結果は unit も可）だけ。

| 誤りの例 | 結果（2026-09-29 に確認） | 正しい書き方 |
| --- | --- | --- |
| `pair.0` | `E0002` expected an identifier | `match pair with \| (first, _) -> first` |
| `(5: i64)` | `E0002` expected the closing delimiter | `let x: i64 = 5` |
| `Display.to_string 5` | `E1002` type class 'Display' has no method 'to_string' | `to_string 5` か `Display.display 5` |
| `let new = 1`（`Atomic.new` なども） | `E0002`（`new` は予約語） | 別の名前（例: `create`） |
| `abs n`（`n` は整数。`let n = -2` など） | `E1003` expected an integer, found f64（2026-10-04 に確認。リテラルの `abs (-2)` は D-34 から f64 として受理される） | `Int.abs n` |
| task の中の `if c then return v` | `E0002` | `if c { return v } else { return w }` |
| 対の `def` で `fn name () = ...` | 古い署名として解釈される | `fn name _unit = ...` |
| union を record の波括弧で書く | 構文エラー | `union Reading = Missing \| Value of i64` |

- 通常の明示ブロックは文の区切りに `;` が要る。`and!` は次の行の `let!` に続けて書き、前に `;` を置かない。
  通常の `match` の節は `do!` を直接受けない（値を選んでから `do!` するか `match!` を使う）。
- 無名関数は `\x -> body`。`x -> body` は互換のための形なので新しいコードでは使わない。

## 13. 停止条件と完了報告

### 13.1 止めて報告する条件（全チケット共通）

次のどれかに当てはまったら、即興で回避せず作業を止め、状況・原因の候補・選択肢を人間に報告します。チケット固有の停止条件はチケットの
「着手条件と停止条件」にあります。

- `要承認` の決定事項が承認されていないのに、その Phase／段の変更が必要になった。
- チケットの記述と HEAD のコードが食い違い、チケットの意図を保ったまま合わせる方法が一つに決まらない。
- `unsafe`、新しい crate、既定の WASM import、fast-math や意味を変える最適化が必要に見える。
- 既存のテストの期待値を変えないと通らない（チケットの「既存テストへの影響」に書かれたものを除く）。
- §3.1 の回帰テストが失敗する、または上限・stack の大きさを上げたくなった。
- 生成 IR が非決定的になった、または native と WASM、`-O0` と `-O3` で結果が違う。
- 同じ失敗を直す試みが 2 回続けて失敗した。

### 13.2 完了報告の書式

作業の終わりに、チケットの末尾へ `## 実装と検証（YYYY-MM-DD）` の節を足し、同じ内容を人間にも報告します。

- 実装した Phase と手順、実装しなかったもの（対象外・未承認）。
- 実行した確認コマンドと結果（通過したテストの件数、E2E の suite 名と件数、native／WASM × `-O0`／`-O3`、`live == 0`）。
- 計測した場合は計測条件と結果の保存先（§14）。性能を主張しない場合はそう書く。
- チケットから外れた判断（決定事項への追記）と、見つけた問題・残作業。
- 更新した `_tsuzuri/language-reference/` のページ（§8）。更新が不要なら、その理由。
- 変更したファイルの一覧。コミットは人間の指示があるまで作らない。

## 14. 性能チケットの共通手順

`_perfs/` のチケットと、性能に触れる機能チケットに共通する手順です。個々の指標・workload・コマンドは各チケットと PX01・PX03 にあります。

1. **before を保存する。** 着手直前の release コンパイラを `target/perf/<チケット ID>/tsuzuri-before` に写し、以後の比較に使う。
   生データは PX01 の形式（JSON Lines）で `target/perf/<run_id>/` に置く。run_id は `<チケット ID>-before`・`<チケット ID>-after` の形にする。
   `target/` はコミットしない。
2. **環境を記録する。** CPU・コア数・OS・Clang・rustc・Node の版、コミット、未コミットの変更の有無（PX01 の record の `host` 欄）。
3. **標本。** 一つの組につき 9 回以上測り、中央値と最小・最大を記録する。warm-up は各 runner の既定に従う。
   計測中は他の重い処理を動かさない。時間の比較は同じ機械・同じ条件の before と after だけで行う。
4. **意味を先に確かめる。** 計測の前に、チケットの「テスト計画」の suite と §3.1 の回帰テストを通す。結果が変わる最適化は計測しない。
5. **生成コードを確かめる。** 速くなった理由を IR か機械語で示す:
   `target/release/tsuzuri build <dir> --emit llvm -O3 -o /tmp/<name>.ll` と
   `/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn <実行ファイル>`。チケットの「生成コードの確認」の形を探す。
6. **プロファイル。** 必要なら strip しない release を別の target dir に作る:
   `CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir /tmp/tzperf/target`、macOS では `sample <pid>`。
7. **記録と主張。** `docs/benchmarks.md` に日付・環境・コミット・中央値・生データの保存先を書く。計測していない効果を書かない。
   CI に速度の合否の閾値を入れない。実装済みと計画を区別する。
8. **fast-math・再結合・暗黙の FMA・二重丸め・未対応命令の無条件使用で速くしない**（AGENTS.md）。
