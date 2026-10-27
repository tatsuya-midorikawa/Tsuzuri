# 実装チケット共通ガイド

このガイドは `_features/` の全チケット（完了済みは `_features/_completed/`）と `_perfs/` の性能チケットに共通する前提・手順・設計決定をまとめたものです。
**着手前に読むのは、[GUIDE-LITE.md](GUIDE-LITE.md)（約 16 KB の抜粋）、対象チケット、`AGENTS.md` です。** このファイル（約 280 KB）は参照用で、全文は読みません。
読むのは、チケットが指す節と台帳の D-n、§0・§1・§13、変更の種類に合う §6 のチェックリストだけです（§9 は約 1,250 行の台帳なので、
引かれた D-n だけを読みます）。`docs/language.md`（約 700 KB）と `docs/architecture.md`（約 500 KB）も全文は読まず、
`grep -n '^#' <ファイル>` で見出しを出して、関連する節だけを読みます。チケットとこのガイドが矛盾する場合は、
このガイドの「9. 設計決定台帳」を優先し、矛盾をチケットの「決定事項」（旧いチケットでは「未決事項」）に追記して人間に報告します。

**第3期のチケット（[D-50](#d-50-第3期計画ai堅牢性性能27-チケットの起票)。G21–G36・A20–A23・B10・C12・D13–D14・E15・F14–F15）は、GUIDE-LITE.md とそのチケットだけで着手できるように書いてあります。**
第1・2期のチケットも、まず GUIDE-LITE.md とチケットを読み、チケットが指す GUIDE の節だけを足します。小さいモデルに渡す文脈は、G32 の `ticket-brief`（チケットごとの文脈パックの自動生成）が作ります。

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
8. **資源上限を守る。** 構文・式の深さ 128（`syntax::MAX_NESTING`）、特殊化 65,536（型が育つ多相再帰は先に止める。D-41）、制約 128、
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

着手前に全テストは回しません。基準は `main` の最新の `ci.yml` の結果です（`gh run list --branch main --workflow ci.yml --limit 1`）。それが緑なら、`main` が通る検査を自分の変更で落としたときは自分の変更の影響です。赤のときだけ、赤い検査を手元で再現し、自分の変更と無関係であることを記録してから着手します。

---

## 3. 検証コマンド

回す範囲は [README の「検証と性能測定」](../README.md#検証と性能測定)の T0〜T3 に従います。T0 は編集のたび（fmt・clippy・該当するテスト）、T1 は手順の 1 段ごと（全 Rust テスト）、T2 は完了の報告の前（T1・該当する E2E・変更した `_tsuzuri/` のページの `check-docs`）で、T3 は CI です。

```sh
# T0: 形式・lint と、変更した Rust テストだけ（--test <ファイル名> を使う）
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test <ファイル名から .rs を除いたもの>
cargo test --locked <テスト関数名の一部>

# T1: 全 Rust テスト（LLVM 不要）。cargo-nextest があれば速い
cargo nextest run --locked    # 無ければ cargo test --locked

# T2: E2E（LLVM/Clang/LLD/Node が必要）。内側のループは dev バイナリで足りる
cargo build
node tests/e2e.mjs target/debug/tsuzuri
node tests/primitives.mjs target/debug/tsuzuri
node tests/tasks.mjs target/debug/tsuzuri
node tests/computations.mjs target/debug/tsuzuri
node tests/control.mjs target/debug/tsuzuri
node tests/numeric_casts.mjs target/debug/tsuzuri
node tests/examples.mjs target/debug/tsuzuri

# 追加のメモリ検査（ASan が使える環境）
TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/control.mjs target/debug/tsuzuri
TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/computations.mjs target/debug/tsuzuri
```

> **注意:** 名前フィルターだけの `cargo test --locked <pattern>` は、一致するテストが 0 件でも成功します。
> 出力の `running N tests` の N が 0 でないことを必ず確認してください。
> Node の E2E には、`cargo build` が作る `target/debug/tsuzuri`（dev profile は opt-level 1）か、`cargo build --release --locked` が作る
> `target/release/tsuzuri` を渡します。**どちらでも、Node テストの直前にそのビルドを実行し、古いバイナリで検証しないでください。**
> 性能や大きさを見る検査と、チケットが release と指定した検査は release で回します。

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

「変更の種類 → 回す Rust テストと E2E」の表と、どの変更でも通す回帰テストは、[README の「変更の種類別」](../README.md#変更の種類別)にあります（旧版はこの節に表を持っていました）。`cargo test --locked <pattern>` は一致するテストが 0 件でも成功する。**出力の `running N tests` の N が 0 でなく、期待した数であることを毎回見る。** 回帰テストの 4 件は T1（全 Rust テスト）に含まれる。上限や stack の大きさを上げて通してはいけない（§11.1）。

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
       closures_rec.rs          ローカルな let rec（グループ）のリフト（B09）
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
| `src/closures_rec.rs` | `lower_block`、`lower_group`、`lower_body`、`rewrite`、`Captured`、`Frame`、`State` | ローカルな `let rec`（`and` のグループ）のリフト。捕捉の値渡しと借用の分類、本体ごとのフレーム（どの `let rec` 関数を直接呼べるか）、直接呼び出しへの書き換え（B09、D-46） |
| `src/computation.rs` | `OPERATIONS`、`collect_all`（`@alias` の収集を含む）、`expand`、`resolved_builder`、`Lowering::{block, continuation, delay, call}` | ビルダー式の展開と、別名のビルダーへの書き換え（D-43） |
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
| `src/constants.rs`、`src/const_machine.rs`、`src/const_rules.rs`、`src/const_static.rs` | 定数の評価と型付きリテラルへの展開（D06）。`const def` の明示スタックの評価器（`Machine`）、署名と本体の規則（`Type`／`TypedExprKind` を `_ =>` なしで振り分ける）、平たいデータの定数の静的データ（D11） |
| `src/pure_rules.rs` | A19 の署名・平たい所有データ・純粋性の検査。閉じた根からの到達可能性と D-53 の問い合わせ口 |
| `src/derive.rs` | `deriving` の instance を通常の AST として生成（A07） |
| `src/docgen.rs` | API 文書の生成（G09） |
| `src/exhaustiveness.rs` | match の網羅性検査（A03） |
| `src/formatter.rs` | `tsuzuri fmt`（`format_source`、`SourceKind`） |
| `src/gpu.rs` | GPU kernel の抽出と WGSL 生成（F07）。`--emit wgsl-relaxed` と `f16`・`f32` の relaxed kernel（F09） |
| `src/gpu/spirv.rs` | Vulkan 向けの SPIR-V の emitter。依存なしで決定的（F09 Phase 3） |
| `src/gpu_devices.rs` | 非 CPU の `Gpu.Backend` を作るプログラムの kernel の表（`CheckedModule.gpu`）と呼び出しの付け替え（F09 Phase 2） |
| `src/llvm_gpu.rs` | kernel の記述子の表と GPU runtime の境界（`tsuzuri_gpu_open`・`tsuzuri_gpu_run`・`tsuzuri_gpu_select`）の IR（F09） |
| `src/llvm_sync.rs` | `Atomic`・`Mutex`・`Channel` の IR（F10） |
| `src/higher_kinds.rs` | 高カインド型の正規化（A10） |
| `src/llvm_bulk.rs` | 配列の一括 API（C04） |
| `src/llvm_compare.rs`、`src/llvm_hash.rs`、`src/llvm_display.rs` | 構造比較・構造 Hash・表示（A06・A07・A11・D01） |
| `src/llvm_debug.rs` | DWARF（G08） |
| `src/llvm_imports.rs` | host import（E06） |
| `src/llvm_io.rs` | IO の入口（`std/IO.tc`） |
| `src/llvm_math.rs` | 数学関数と FMA（D03・D05）。f16 の `half` 命令の経路 `half_binary`・`half_cast`・`half_sqrt`（D10） |
| `src/llvm_parallel.rs`、`src/llvm_task.rs` | データ並列と `Task.parallel_results`（F02・B06） |
| `src/llvm_recursive.rs`、`src/recursive.rs` | 再帰型の判定・ノード・反復的な drop／clone（A04） |
| `src/llvm_simd.rs`、`src/simd.rs` | SIMD 型（F03・F04） |
| `src/llvm_traps.rs`、`src/trap.rs` | trap の種類・位置・計装（G04） |
| `src/lsp.rs`、`src/semantic.rs` | LSP サーバーと `SemanticIndex`（G07） |
| `src/package.rs` | manifest と package graph（E04）。`[package]` の `edition`（G19） |
| `src/target.rs` | `--target TRIPLE` が名指しできる 22 個の native の triple の表、ビルド先の性質を答える `Platform`（`cfg!` の代わり）、実行器 `TSUZURI_RUNNER`（G15） |
| `src/api_diff.rs` | `tsuzuri api-diff`。2 つの版の公開 API を構文解析だけで比べる（G19） |
| `src/regions.rs` | 名前付き region の検査（A09。複数 region の record と region で量化した引数は A12） |
| `src/stdlib.rs` | std ソースの埋め込み（`SOURCES`）、予約モジュール、opaque record |
| `src/test_runner.rs` | `tsuzuri test`（G06） |
| `src/warnings.rs` | 警告 W1001–W1005（G03・G19。W1005 は `SemanticIndex::references` から出す） |
| `src/copies.rs` | 暗黙の複製の一覧と W1006（A15） |
| `src/bindgen.rs`、`src/bindgen_driver.rs` | `tsuzuri bindgen`。Clang の JSON AST と `-E -dD`／`-dM` から extern を生成（E11） |
| `src/bindings.rs`、`src/bindings_native.rs`、`src/llvm_shared.rs` | `--emit bindings-js`／`bindings-cs`／`bindings-py`／`bindings-cpp` と `--emit shared`（E13） |
| `src/fetch.rs` | `tsuzuri fetch`・`tsuzuri publish`。git・registry 依存の取得と最小版選択（E10） |

実行時ランタイムも増えています（`character.ll`、`display.ll`、`debug.ll`、`recursive.ll`、`utf8string.ll`、`math.ll`、
`heap-wasm-threads.ll`、`task-wasm-threads.c`、`task-windows.h`、`cpu.c`、`io.c`、`test-runner.c`、`unicode.ll`（D09、`scripts/generate-unicode.mjs` の生成物）、
`bindings.mjs`・`bindings-core.mjs`・`bindings-threads.mjs`（E13 の glue。コンパイラは連結せず生成物へ埋め込む）、
`net.c`（E09。`Net` の socket と名前解決。IR が `@tsuzuri_net_` を宣言したときだけ連結）、`bindings-net.mjs`・`bindings-net-host.mjs`（E09 Phase 3 の `--wasm-feature net` の glue）、
`gpu.c`・`gpu-vulkan.c`（F09。WebGPU と Vulkan のローダーを実行時に読み込む runtime。IR が `tsuzuri_gpu_`・`; tsuzuri-gpu: vulkan` を宣言したときだけ連結）、
`sync.ll`・`sync-wasm.ll`・`channel-wasm.ll`（F10））。連結条件は `src/llvm.rs` と
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
- LLVM 表現: `%tz.string = { ptr, i64 }`、`%tz.array = { ptr, i64 }`、`%tz.list = { ptr, i64 }`（先頭の chunk・長さ。空は `zeroinitializer`）、
  リストの chunk `{ ptr next, i64 rest, i64 capacity, [0 x T] slots }`（1 回の確保。header は 24 bytes で、要素は連続して並ぶ。一度に作るリストは 1 chunk、`List.cons` は chunk を足す。PM04、`src/llvm_list.rs`）、`%tz.closure = { ptr, ptr }`（静的記述子 `@tz.desc.*`（`%tz.closure.desc = { code, clone, drop }`）/environment。PM02）、
  レコード `%tz.record.<Module.Name> = type { fields... }`（利用者のレコードは、フィールドをアライメントの降順に並べると小さくなるときだけその順に格納する（PM01）。
  評価・drop・`deriving`・DWARF のメンバー・公開 ABI の C の構造体は宣言順のまま。フィールドの読み書きは `field_slot` を通す）、タプルはリテラル構造体（宣言順）、`bool` は `i1`、`unit` は `i8`、
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
- **G19 以後（D-45）は、予約語をソースの edition で決める。** 新しい予約語は `Lexer::identifier` の `match` に足したうえで、予約を始める edition と一緒に
  `EDITION_KEYWORDS`（`src/lexer.rs`）へ登録する。それより古い edition のソースでは識別子として読める。新しい edition は人間の承認を得て一つの変更で作る
  （`Edition` の定数・`SUPPORTED`・`LATEST`・`EDITION_KEYWORDS`・`docs/stability.md` の表・`tests/editions.rs` を同時に更新し、`MANIFEST_DEFAULT` は変えない）。
  lexer と parser の入口は edition を必須の引数にし、既定値を持つ入口を足さない。

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
- 引数は D-59 の順（操作する値を最後）にする。`Array`・`List`・`Vec`・`String`・`Utf8String` の組み込みは、
  `tests/std_argument_order.rs` が型を検査する。資源のハンドルを先に取る組み込み（`Atomic`・`Mutex`・`Channel` など）は例外で、理由を同じファイルの表に書く。

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
  `driver.rs` の `BuildOptions`（`validate` で不正な組み合わせを `E2000` にする）、README の CLI 表を更新する。
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

機能を追加・変更・修正・削除したら、**同じプルリクエストで**（動作が固まったところで一度）`_tsuzuri/language-reference/`（目次は `index.md`）の
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
  概要と実行例は `README.md`、性能は `docs/benchmarks.md`。`docs/language.md` は言語リファレンスと同じ PR で食い違いを残さない。
  README は、書いてあること（クイックスタート・特徴の一覧・CLI の概要・検証手順）が変更で誤りになるときだけ直す。
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
- LLVM 表現（PM01 で改めた。規則の詳細は `docs/architecture.md` と `src/llvm_layout.rs` の `UnionLayout`）:
  - タグは case の数に応じた幅（256 個までは `i8`、65,536 個までは `i16`、それを超えると `i32`）で格納し、読む側が `zext` で `i32` に広げる
    （`TypedExprKind::UnionTag` と `SwitchPlan.selector` は `i32` のまま。再帰 union のノードのタグは `i32`）。
    タグと record のフィールドの読み書きは `src/llvm_layout.rs` の helper（`tag_value`・`load_tag`・`construct_value`・`tag_type`・`field_slot`）だけを通す。
  - 全 case がペイロードなしなら値はタグだけ（`Enum`）。全ペイロードが同じ LLVM 型なら `{ iT, T }`（`Common`）。型が異なるなら共有領域 `{ iT, [N x iU] }`（`General`。
    `U` は最大ペイロードのアラインメント（16 まで）の整数、`N` は最大ペイロードを覆う個数。ケースごとの別フィールドにはしない。ペイロードはメモリ経由で読み書きする）。
  - case がちょうど 2 つで、1 つがペイロードなし、もう 1 つのペイロードが `string`・`utf8string`・配列なら、タグを持たずペイロードそのもの（型定義なし。`NegativeLength`:
    長さが負の `{ ptr null, i64 -1 }` が `None`）。native のターゲットだけ、スライスでない `ref` のペイロードも同じ（`NullPointer`: null ポインターが `None`）。
    WebAssembly は `--stack-first` でアドレス 0 も有効なオブジェクトの位置になりうるので `NullPointer` を使わない（`Maybe<ref T>` は `{ i8, ptr }`）。
    再帰 union と `Drop` を実装した union は対象外。ゼロ埋めした値は `None` ではない（`%tz.string` のゼロは `Some ""`）ので、`None` は必ず明示的に作る（`Seq.next` の空の `Seq` と `None` など）。
  - サイズの見積もり `layout_bound` は上限・下限とも実際のレイアウトを挟む（独立なデータレイアウトの模型でテスト）。E1010 の見積もり（`check.rs` の `Layouts`）と
    `llvm_frame::stack_size` は保守的な上限のまま変えない。
  再帰的な共用体は D-24（所有ノード）。

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
- 未使用の標準ライブラリ関数は IR に出力しない（到達可能性で間引く）。型検査は常に行う（opt-in std モジュールは名前を書いたプログラムだけが読み込む。D-40）。
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
  | `Gpu` | GPU 実行 API（`Gpu.Auto` と Vulkan・WebGPU の実行時接続を含む） | F07、F09 |
  | `Owned` | 早期解放（`Owned.drop`）と Drop 型を捕捉できる関数値（`Owned.Function`） | B07 |
  | `HashMap`／`HashSet` | 不透明なハッシュ表と集合（挿入順の反復、seed 付きハッシュ、借用キーの検索） | C09 |
  | `File`／`Dir`／`Path`／`Env`／`Time`／`Random`／`Os`／`Process` | OS API（`IO` の遅延アクション。`Path`・`Os` の純粋な部分と `Random.Pcg` は wasm32 でも使える） | E08 |
  | `Format` | 書式指定の部品（`Spec`・`parse`・`pad`）。`Format` 型クラスの instance を書くための補助 | D07 |
  | `FixedArray` | 固定長配列の構築（組み込み `FixedArray.init`。std のソースはなく、名前を予約する） | A16 |
  | `Dyn` | dyn 値の構築とアップキャスト（組み込み `Dyn.of`。std のソースはなく、名前を予約する） | A14 |
  | `Arena` | 世代付きハンドルで要素を指す arena（循環するグラフ）。opt-in std モジュール（D-40） | C10 |
  | `Rc`／`Arc` | 参照カウントの共有所有と弱参照（組み込みの型と関数。std のソースはなく、名前と型名を予約する） | C10 |
  | `Json`／`Cbor` | JSON の値・解析・出力・ストリーミング、`Encode`／`Decode` の std インスタンス、CBOR。opt-in std モジュール（D-40） | D08 |
  | `Regex`／`Unicode` | 線形時間の正規表現、Unicode 17.0.0 の表による分類・正規化・境界・大小変換。opt-in std モジュール（D-40） | D09 |
  | `Bench` | `bench` 宣言の本体を作る `Bench.of`・`Bench.with_input` と組み込み `Bench.now`（`tsuzuri bench` の実行器の中だけ）・`Bench.consume`。opt-in std モジュール（D-41） | G18 |
  | `Gen` | プロパティテストの生成器・組み合わせ・縮小と `Gen.for_all`。std 専用の組み込み `Gen.__seed`。opt-in std モジュール（D-41） | G18 |
  | `Async`（`Async.tc`） | 協調的な非同期計算・仮想時計・ホストの再開・native reactor・WASM JSPI。opt-in std モジュール（D-42） | B08 |
  | `Atomic`／`Mutex`／`Channel` | 整数・bool の不可分操作、排他制御（`Mutex.with_lock`）、有界の多対多チャネル。opt-in std モジュール（D-44） | F10 |
  | `Matrix`／`MatrixView`／`Tensor` | 行優先の行列と順序を固定した積、借用する窓（`MatrixView`・`MatrixView.Mut`）、N 次元の `Tensor`。opt-in std モジュール（D-44） | C11 |
  | `Net` | アドレスの解析と表示、名前解決、TCP・UDP（`IO<Result<_, Os.Error>>` と `_async` の双子）。opt-in std モジュール（D-44） | E09 |

  組み込みクラス（`Display`、`Parse`、`Hash`、`Default`、`Elementary` など）は std モジュールに属さない組み込み名として予約する。
  `Elementary` は超越関数（`Math.sin` など）用のメソッドなしマーカークラスで、D03 では f32／f64 だけが満たす。
  `UnsignedInteger`（D04）は符号なし整数だけが満たす組み込みマーカークラスとして予約する。
  `Drop`（B07）は利用者が宣言した record・union だけが instance を持つ組み込みクラスとして予約する（D-31）。
  `Format`（D07）も利用者が宣言した record・union だけが instance を持つ組み込みクラスとして予約する（D-32）。
  `Encode`／`Decode`（D08）は組み込みクラスで、組み込みの instance を持たず、std の `Json` の instance と利用者の instance・導出を使う（D-40）。
  `Sync`／`AtomicValue`（F10）はメソッドのない組み込みのマーカークラスで、`Sync` は複数の task が共有借用で同時に使ってよい型の印（`Task.scope` が共有する値と
  `Arc<T>` の `Send` の判定に使う）、`AtomicValue` は `Atomic` に入れられる整数 8 種と `bool` が満たす。利用者の instance は `E1016`（D-44）。

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
- **消費する関数的更新**: `Array.update_at : i64 -> 'a -> ['a] -> ['a]` のように所有値を受け取り新しい値を返す。
  所有権により唯一の所有者であることが保証されるため、実装はバッファをその場で書き換えてよい（観測できない最適化）。
- 伸縮可能な配列は組み込み型 `Vec<'a>`（C02）。`[T]` の記述子 `{ ptr, i64 }` は変更しない。
- 部分参照（スライス）は共有が `&[T]`（C03）、排他が `&mut [T..]`（C08）。`&xs[a..b]`／`&mut xs[a..b]` で作る。
  `&mut [T]` は配列全体の置換用のままで、呼び出しの引数では `&mut [T..]` へ変換できる。
  コレクションは共有されている間は不変で、排他スライスを通してだけ `Array.write`／`Array.swap_in`／`Array.sort_in_place` で要素をその場で置換できる。
  要素単位の排他借用（`&mut xs[i]`）は導入しない。
  **A18 Phase 4（2026-10-23 に全 Phase を承認）の例外:** edition 2027 では可変な所有者または
  排他参照を根とする record field と配列・リスト・Vec・固定長配列の要素への代入構文を許す。
  共有 loan、移動済みの射影、範囲検査、格納する参照の lifetime 契約は従来どおり検査する。
  2026 の受理・拒否は変えず、文字列の要素と要素単位の排他借用は不変のままである。
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
`const`（D06）、`test`（G06）、`extern`（E06）、`dyn`（A14。D-39）、`bench`（G18。D-41）。`of` は union 宣言の中だけの文脈キーワード（A02。予約語にしない）。
文字リテラル `'x'`／`u8'x'`（A08）。範囲の部分参照 `xs[a..b]`（C03）。
レコード更新 `{ base with field = value }`（C05）。キーワード追加時は 6.1 を実施する。
D-34 の演算子 `**`・単項 `+`・`&&&`・`|||`・`^^^`・`~~~`・`<<<`・`>>>`、関数合成 `>>`／`<<`（従来のシフトから意味を変更）、
文脈キーワード `try`／`finally`／`is`（予約語にしない）、属性 `@checked`／`@literal`。
D-35 の文脈キーワード `namespace`／`using`（ファイル先頭の宣言だけ。予約語にしない）。D-37 の名前空間のパス `A::B::Module`（新しい記号はなく、空白なしの `::` を lexer が `PathSep` にする）。
D-39 の固定長配列の型 `[T; N]` と長さパラメーター `const N: i64`（A16。既存の予約語 `const` の組み合わせで、新しい予約語はない）、排他スライスの型 `ref mut [T..]`（C08）、
dyn 型 `dyn C`・`dyn (C, D, Copy, Send)`・`dyn C {r}`（A14）、関数の属性 `@cpu ["avx2", ...]`（F08。`def` シグネチャの前だけ。`cpu` は予約語にしない）。
D-41 のトップレベル宣言 `bench "name" = body`（G18）と、どのソースでも先頭（byte 0）の `#!` 行を行コメントとして読む shebang の規則（G13 Phase 2）。
D-45 の関数の修飾 `const def`（D11。既存の予約語 `const` と `def` の組み合わせで、新しい予約語はない）、manifest の `[package]` のキー `edition`、文書コメントの行 `@deprecated`（G19。字句と構文は変えない）。
D-48 の関数の修飾 `pure def` と純粋な関数型 `pure (T -> U)`（A19。`pure` はこれらの位置だけの文脈キーワードで、予約語にしない）、引数の型の前置き `own T`（A18・A19。通常と pure の `def` の引数で受理するが、結果・field・型引数・矢印型の中には導入しない）。`own` も文脈キーワードで、いずれも `EDITION_KEYWORDS` は空のまま。
G19 以後に予約語を足すときは、`Lexer::identifier` の `match` に足すほかに、予約を始める edition と一緒に `EDITION_KEYWORDS`（`src/lexer.rs`）へ登録する（§6.1。D-45）。
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
| `E1029` | `pure def` の署名・本体の違反（効果の源の使用、`own` の型の制限、`ref mut`・`mut` の引数など。メッセージで区別する。D-48） | A19 |
| `E2006` | テストの失敗（`tsuzuri test`） | G06 |
| `E2007` | 依存の取得・検証の失敗（`Tsuzuri.lock` の欠落・不一致、store の欠落、git の失敗、registry の index の不整合。D-40） | E10 |
| `W1001` | 未使用のローカル束縛 | G03 |
| `W1002` | 未使用の非公開関数・型 | G03 |
| `W1003` | 到達しない `match` 節 | A03／G03 |
| `W1004` | 同じスコープ内での紛らわしいシャドーイング（既定は無効） | G03 |
| `W1005` | 別のパッケージ・std の `@deprecated` な `def`・`record`・`union`（と case）の使用（既定で有効。同じパッケージの中では出さない。D-45） | G19 |
| `W1006` | 長さに比例する配列・リストの暗黙の複製（既定は無効。`--warn implicit-copy` で有効。D-33） | A15 |
| `W2002` | `tsuzuri bindgen` が ABI の一致を確かめられない C の宣言を省いた（D-40） | E11 |

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
- 新しい解析・展開（網羅性検査、deriving の生成、const 評価、到達可能性など）は、既存の上限（深さ 128、組み合わせの予算 1,024 など）
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
- 定数の直接借用は単一段階の完全適用・loanを持たない結果だけ。保存・返却・段階適用では明示let束縛を要求し、static lifetimeを導入しない。**D-45 で、平たいデータの定数に限り改めた**（D11 Phase 2。その借用は返す・格納する・渡すことができ、バイナリの読み取り専用の静的データ `@tz.static.N` を指す。`string` など平たくない定数は従来どおり）。
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
- manifestも読み込み・出力保護対象とする。全graphでpackage1024・深さ128・source4096、名前空間衝突・循環・symlinkを拒否する。build scriptは導入しない（git 依存・lockfile・registry は E10 で導入した。D-40）。
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
- 既存の予約語の組み合わせで表せる構文（`extern type`、`extern "symbol" def`、`const def`、`const N: i64`）を優先し、新しい予約語を増やさない。G19 の edition を導入したので（D-45）、新しい予約語は新しい edition でだけ予約する（`EDITION_KEYWORDS`。新しい edition を作るのは人間の承認を得た一つの変更）。
- 言語の意味や既存の決定を変える提案は承認まで着手しない。C08（D-13 の変更）と A16（`[T; N]` の再導入）は D-39、C10 Phase 2（参照カウントの導入）は D-40、D11 Phase 2（static データ。D-28 の変更）は D-45 で承認済み。
- 新しいホスト機能（WASI、乱数 seed の設定、非同期の再開、GPU runtime）は D-18 に従い明示的な opt-in とし、既定の WASM に import を追加しない。

診断コードの仮割り当て（`W1005`、G19）は D-16 へ移して表が空になった。

2026-09-29 の詳細化で、各チケットが次の名前を仮に決めた（衝突を避けるための台帳。確定は各チケットの決定事項と承認に従う）。
`要承認` の欄は、そのチケットで承認が必要な名前であることを示す。

| 種別 | 仮の名前 | チケット | 要承認 |
| --- | --- | --- | --- |
| 構文 | `extern "symbol" def`・`extern "module" "symbol" def` | E12 | いいえ（承認済み・実装済み） |
| 構文 | 文書コメントの `@deprecated` タグ、manifest の `edition` キー | G19 | いいえ（承認済み・実装済み。D-45） |
| 構文 | `const def` | D11 | いいえ（承認済み・実装済み。D-45） |
| サブコマンド | `tsuzuri api-diff <old> <new> [--json]` | G19 | いいえ（承認済み・実装済み。D-45） |
| CLI | `--target TRIPLE`（22 個の native の triple。`run --target` は実行器が要る） | G15 | いいえ（承認済み・実装済み。D-45） |
| std | `Bench.now`・`Bench.consume`・`Bench.with_input`・`Bench.of`（`with` は予約語なので `Bench.with` は `Bench.with_input`） | G18 | いいえ（承認済み・実装済み。D-41） |
| サブコマンド | `tsuzuri watch`・`tsuzuri serve` | PB06 | いいえ（承認済み・実装済み。`serve` は PB06 の D2） |
| サブコマンド | `tsuzuri repl`・`tsuzuri script`（CLI の `--timeout` は repl だけ） | G13 | いいえ（承認済み・実装済み。D-41） |
| サブコマンド | `tsuzuri bench` | G18 | いいえ（承認済み・実装済み。D-41） |
| サブコマンド | `tsuzuri toolchain info` | G14 | いいえ |
| CLI | `--allocator system\|host\|counting\|small`・`--freestanding`（F13・PM05、実装済み。`small` は native の opt-in で、既定にする段は PM05 D10 の条件を満たしたときだけ） | F13・PM05 | `small` を既定にする段だけ |
| CLI | `--wasm-max-memory`・`--wasm-stack-size`、manifest の `[wasm]`（`max-memory`・`stack-size`） | F11 | いいえ（Phase 2 承認済み・実装済み） |
| CLI | `--target wasm64` | F11 | いいえ（Phase 2 承認済み・実装済み） |
| CLI | `--emit bitcode` | PR08 | いいえ |
| CLI | `--wasm-feature tail-call` | PM09 | はい |
| CLI | `--trap-mode return` | E14 | いいえ（Phase 2 承認済み・実装済み） |
| CLI | `--link`・`-l`・`-L`、manifest の `[native]` | E12 | いいえ（承認済み・実装済み） |
| CLI | `--profile-generate`・`--profile-use` | PR07 | いいえ |
| CLI | `-Os`・`-Oz`・`--strip` | PM08 | いいえ |
| CLI | `--backend llvm\|fast` | PB05 | はい |
| CLI | `--samples`（bench）・`--coverage`・`--seed`（test）、`test --index N -g -o PATH`（G16） | G18・G16 | いいえ（承認済み・実装済み。D-41） |
| CLI | `--no-server`（check・build）・`--idle-timeout`（serve） | PB06 | いいえ（承認済み・実装済み） |
| 環境変数 | `TSUZURI_THREADS` | PB04 | いいえ |
| 環境変数 | `TSUZURI_CODEGEN_UNITS` | PB07 | はい（承認済み・実装済み。既定は `single` のままで、`auto`・`split` が opt-in） |
| 環境変数 | `TSUZURI_SERVER_DIR` | PB06 | いいえ（承認済み・実装済み） |
| 環境変数 | `TSUZURI_SYSROOT`・`TSUZURI_RUNNER`、`TSUZURI_CROSS_LINK`・`TSUZURI_CROSS_CLANG`・`TSUZURI_CROSS_EXECUTE`（テスト用） | G15 | いいえ（承認済み・実装済み。D-45） |
| 環境変数 | `TSUZURI_F16_EXPECT`（テスト用。`tests/f16_hardware.mjs` だけが読み、コンパイラは読まない） | D10 | いいえ（実装済み。D-45） |
| 環境変数 | `TSUZURI_LLVM_PROFDATA` | PR07 | いいえ |
| 環境変数 | `TSUZURI_TIME_PASSES` | PX01 | いいえ |
| 環境変数 | `TSUZURI_ZIG`・`TSUZURI_GO` | PX02 | 計測機への導入だけ |
| 環境変数 | `TSUZURI_TEST_ALLOCATOR`（テスト用） | F13 | いいえ |
| 環境変数 | `TSUZURI_LLDB`（テスト用） | G16 | いいえ（実装済み） |
| 環境変数 | `TSUZURI_BASELINE`（テスト用） | PM03 | はい（PM03 全体） |
| 環境変数 | `TSUZURI_STRINGS=utf16\|compact`（`string` の内部表現。既定は `utf16`） | PM03 | いいえ（承認済み・実装済み。`compact` は opt-in で、既定にする段は PM03 D11 の条件を満たしたときだけ） |
| 環境変数 | `TSUZURI_TEST_WASM_TARGET`（テスト用。`tests/features.mjs` の WASM を `wasm64` で実行） | F11 | いいえ |
| 公開記号 | `tsuzuri_host_alloc`・`tsuzuri_host_free`・`tsuzuri_host_realloc`（WASM は `tsuzuri_heap` の `alloc`・`free`・`realloc`）、`tsuzuri_alloc_stats` と `tsuzuri_allocation_stats` | F13 | いいえ（実装済み） |
| 公開記号 | `tsuzuri_cpu_<op>_<type>`（`tz_cpu_level`・`TZ_CPU_PICK`・`CPU_KERNELS`）、`@cpu` 関数の stub が呼ぶ `tsuzuri_cpu_pick` | F08・PR05 | いいえ（F08 は実装済み） |
| 公開記号 | `tsuzuri_try_<name>`・`tsuzuri_trap_info`（`tsuzuri_boundary_run`・`tsuzuri_trap_raise`・`tsuzuri_tracked_*` は runtime の内部） | E14 | いいえ（Phase 2 承認済み・実装済み） |
| 公開記号 | wasm global `tsuzuri_stack_base`・`tsuzuri_stack_top`（threads の worker の stack の範囲） | F11 | いいえ（Phase 2 実装済み） |
| ランタイム | `string_scalar.ll`・`string_v128.ll`（PR05）、`string_latin1.ll`（PM03）、`integer.ll`（PM08）、`heap-host.ll`・`heap-counting.ll`（F13、実装済み）、`net.c`（E09、実装済み）、`heap-wasm64.ll`（F11、実装済み）。`format.ll`（D07）・`os.c`・`os-wasi.c`（E08）は D-32 で確定 | 各チケット | 各チケットの承認に従う |

新しい `.ll` を足すときは §2.2 の `.gitignore` の例外行と `scripts/check-runtime-includes.sh` を忘れない。
Phase 2 以降の仮の名前（B08 の opt-in フラグ・WASM import、E14 の `Trap`・`TrapInfo` など）は、承認のときに割り当てる。

### D-31 利用者定義の解放（B07）

- 2026-10-02、利用者の「B07 の実装を完遂して」という依頼を D1・D2・D4 の承認として扱い、Phase 1 を実装した。組み込みクラス `Drop` は D-07 へ移した。
- LLVM は move 済みの領域を `zeroinitializer` で埋め、その解放を無処理にする規則で動く。この規則を変えずに利用者の `drop` を一度だけ呼ぶため、
  非再帰の Drop 型は値の末尾に `i8` の生存フラグを持ち、構築で 1 にする。drop glue はフラグが 0 の値で `drop` を呼ばない。
  payload のない Drop union は素のタグではなく `General { unit: 1, units: 0 }`（`{ iT, [0 x i8], i8 }`）の配置にする。record の生存フラグは並べ替えても最後で、Drop の union は niche にしない。
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

### D-40 第2期の 6 チケット（C10・D08・D09・E11・E13・E10）の確定

- 2026-10-07、利用者の「C10、D08、D09、E11、E13、E10 の実装をすべて完遂して。…すべてのフェーズを完了させること。
  …判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」を、6 チケットの `要承認` の決定事項すべての承認として扱った。
  詳細は各チケットの「実装と検証」にある。
- opt-in std モジュール（D-07 の改訂）: `Arena`・`Regex`・`Unicode`・`Json`・`Cbor` の宣言は、利用者のコードからは修飾した名前（`Json.Value`、`Arena.Handle`）でだけ見え、
  無修飾の型・case・型クラスの解決の候補にならない。`build`・`check`・`run`・`test`・`doc` は、利用者のソースがモジュール名（`Json` は `Encode`／`Decode` も）を
  識別子として含むときだけそれらを読み込む（`stdlib::OPT_IN`・`sources_for`）。言語サーバーは補完のため常に全部を読み込む。名前を書かないプログラムの型検査の時間と
  IR は追加前と同じで、無修飾の `ErrorKind`・`Handle` は従来どおり `Os.ErrorKind`・`File.Handle` を指す。
- C10 Phase 1: std `Arena`。`Arena.Handle<'a>` の phantom 型引数は opaque な std record にだけ許す（利用者の record は従来どおり `E1024`）。arena ID は std 専用の
  組み込み `Arena.__next_id`（`@tz.arena.next_id` の `atomicrmw add monotonic`）。空き slot の世代は `-1 - g` で表し、ハンドルは使用中の slot にだけ一致する。
- C10 D13・D14（Phase 2）: 参照カウントを導入した。`Rc<'a>`・`Rc.Weak<'a>`・`Arc<'a>`・`Arc.Weak<'a>` は `Type::Shared(Box<Type>, SharedKind)` の組み込みの型で、
  std のソースはない（`Rc`・`Arc` は予約モジュール名で、`Vec` と同じく予約の型名）。ヒープの `{strong, weak, T}` を指し、強参照全体で弱参照を 1 つ持つ。
  `Arc` の増加は monotonic、減少は release と `fence acquire`。計数が i64 を超えるとトラップ。再帰する型を含む値は再帰 union の反復 drop で解放する。
  `Rc` は Send にならず、関数値に捕捉できない（`E1013`・`E1005`。関数値は task へ送れるため）。`Arc<T>` は T が Send で、extern ハンドル・Copy でない dyn 値・
  `Owned.Function`・`Rc` を含まないときだけ Send と捕捉ができる（F10 の `Sync` まで）。内部可変性がないので参照の循環は作れず、サイクルコレクターはない。
  型の循環は `Rc`／`Arc` を通れば union を経なくてよい。`.` の後の `new` は名前として読む（`Rc.new`）。
- D08 D1: std `Json`・`Cbor`、組み込みクラス `Encode`／`Decode`（`BUILTIN_CLASSES` の末尾）、`deriving (Encode, Decode)`、record の field と union の case の
  属性 `@json "name"`。`Json.Value` の配列の case は `Items`（`Array` は予約の case 名）。instance の重なりの検査の予算（1,024 組）は、外側の型構築子が同じ頭部の組だけを数える。
  Phase 2 は f16・f128・decimal・`Map`・`Set`・`HashMap`・`HashSet` の instance（マップはキーがすべて JSON 文字列なら object、そうでなければ `[key, value]` の配列）、
  字下げ出力、プル型の `Json.Reader` と `Json.Writer`、CBOR（RFC 8949。serde 風の Serializer クラスではなく共通の `Json.Value` を使う）。新しい診断コードはない。
- D09 D1・D11: std `Regex`・`Unicode`。表は `scripts/generate-unicode.mjs` が UCD 17.0.0 から生成するランタイム `src/runtime/unicode.ll` に置き、std 専用の組み込み
  `Unicode.__table_length`／`__table_entry`（std の `Unicode`・`Regex` 以外からは `E1022`）で読む。チケットの D2・D7（文字列リテラルの表を std に生成する）を改めた。
  std の全ソースの型検査を遅くせず、Phase 2 の 1 文字ごとの API で呼び出しごとの複製を避けるため。Phase 2 は `Unicode.category`、NFC／NFD／NFKC／NFKD、
  UAX #29 の書記素と単語の境界、SpecialCasing と Final_Sigma による大小変換（locale による調整と辞書による分割はない）。
- E11 D1: サブコマンド `tsuzuri bindgen` と警告 `W2002`（D-16）。Phase 2 は整数の `#define`（`clang -E -dD` の行の印で所属を決め、`-dM` の最終値と照合）、
  定義のない struct の不透明ハンドル（E12 の `extern type`）、E12 のコールバック、注釈付きのバッファー（`--buffer FUNC:PTR:LEN`）、引数の移動（`--consume FUNC:PARAM`）。
  ABI の一致を確かめられない宣言（未知の属性、無名 struct の typedef、`typeof`、`push_macro` など）は推測せず `W2002` で省く。対象は LP64（Linux・macOS）だけ。
- E13 D11（Phase 2）: `--emit bindings-js`（wasm32 だけ。wasm64 は `E2000`）、`--wasm-feature threads` の module のブラウザー向け Worker glue（cross-origin isolated で
  なければ例外。逐次実行へ切り替えない）、`--emit shared`（macOS・Linux。Windows は G10 まで `E2000`）、`--emit bindings-cs`／`bindings-py`／`bindings-cpp`。
  新しい診断コードはない。E12 の不具合（スカラーだけの export の `ref` ハンドル引数がハンドルの格納先を渡していた）を直した。
- E10 D1・D10: `tsuzuri fetch`・`tsuzuri publish`、診断 `E2007`（D-16）、`Tsuzuri.lock`（format 1 と、registry 依存があるときの format 2）。D-29 の E04 の記録
  「git、lockfile を導入しない」を改めた。git 依存は commit 固定で、取得は外部の `git`（2.32 以降）が作業木を作らずに行い、ビルドの経路は git もネットワークも使わない。
  registry 依存は root の `[registry]` に書いた git の index（`index/<name>.json`）から、名前と互換の範囲ごとの最小版選択で選ぶ（選ばれなかった版だけが述べる要求は
  衝突にしない）。Tsuzuri は公開の registry を運営しない。`publish` は index の項目を出力するだけで、G19 の公開 API の差分検査はまだない。

### D-41 第2期の 4 チケット（G16・G18・G17・G13）の確定

- 2026-10-08、利用者の「G16、G18、G17、G13 の実装をすべて完遂して。もし現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して。
  …すべてのフェーズを完了させること。…判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」を、4 チケットの `要承認` の
  決定事項すべての承認として扱った。詳細は各チケットの「実装と検証」にある。
- G16（デバッガー）: DWARF の型名は `Type::display`。`bool` 以外のスカラーは同名の typedef、`char`・`utf8char` は `DW_ATE_UTF`。payload を持つ union は
  `$tag`（enumeration。大きさは格納するタグの幅）と `$payload`（case 名を member にする union）、タグを持たない union（niche。PM01）は `$tag` の代わりに `$payload` に重なる
  列挙型 `$niche`（`None` は長さの `-1`、ポインターの `0`）を持つ。record の member は宣言順で、offset は格納の位置。再帰する union はノードの struct への pointer、リストは先頭の chunk の struct（`next`・`rest`・`capacity`・要素型の長さ 0 の配列 `slots`。PM04）への pointer で、
  offset は生成 IR の格納位置から
  計算する。`DISubprogram` は `linkageName` を出さず、単相化の実体は元の名前、ラムダ・task・テストは `<親>.lambda@行:列` などの名前にする。束縛の `store` は
  宣言の位置、引数の束縛は prologue（行 0）。C のランタイムはデバッグ情報なしでコンパイルする（G08 を改めた。step-in がランタイムに入らず、DWARF 5 の
  ランタイムの compile unit による `-O3` の `llvm-dwarfdump --verify` の失敗もなくなる）。LLDB の formatter `scripts/lldb/tsuzuri_lldb.py` を VS Code が
  `initCommands` で読み込み、配布物は `share/lldb/` に置く。Phase 2 は `tsuzuri test ROOT --index N -g -o PATH`（1 件のテストのデバッグ用ランナーを作り、
  実行しない）と Testing ビューの Debug。Phase 3 は MSVC のリンカーを使う Windows の `-g` で CodeView を足して PDB を作り、natvis
  （`src/runtime/tsuzuri.natvis`、配布物の `share/natvis/`）を埋め込む。DWARF も image に残す。配布物の `tsuzuri-clang`（MinGW）の経路は DWARF だけ。
- G18（ベンチマーク・カバレッジ・プロパティテスト）: 予約語 `bench`（D-15）と `bench "name" = body`（本体は `i64 -> i64`）、opt-in std `Bench`
  （`with` が予約語なので `Bench.with` は `Bench.with_input`）、組み込み `Bench.now`（`tsuzuri bench` の実行器の外の IR が参照すれば `E1018`）と
  `Bench.consume`（stack slot と memory clobber の `asm sideeffect`）、`tsuzuri bench`（ネイティブだけ、`--samples`、閾値なし）。Phase 2 は
  `tsuzuri test --coverage PATH`（コンパイラ自身の計装。関数・`if`・`match`・ループ・`try` の region ごとの atomic なカウンター、lcov、成功したテストだけを
  合算）。カウンターのファイルは実行器が環境変数 `TSUZURI_COVERAGE_FILE` で子へ渡す（内部用）。Phase 3 は opt-in std `Gen`（選択の列による統合縮小、
  固定の既定 seed、`tsuzuri test --seed N`、std 専用の組み込み `Gen.__seed`）。`tsuzuri test` は失敗したテストの標準エラーの末尾 64 KiB を表示する。
  ランタイム `src/runtime/bench-runner.c` と実行器だけの記号 `tsuzuri_bench_*`・`tsuzuri_coverage_*` を足した。WASM のテストモジュールは `Gen` を使う
  プログラムのときだけ `tsuzuri_debug.write` を import する（D-18 の opt-in）。
- G17（増分コンパイル）: Phase 1 の frontend cache は `<cache root>/frontend/` にプロジェクトとコマンドの種類ごとのパックと manifest を置く（D4 の見直し。
  ファイルを開く費用のため）。D5 により `check`・`test`・`doc`（と `bench`・`script`）も既定で使う。`--no-cache` は `build`・`run`・`script` で両方の cache を、
  `repl` で成果物の cache を止め（`repl` は frontend cache を使わない）、`check`・`test`・`bench`・`doc` では従来どおり `E2000`。それらの無効化は `TSUZURI_CACHE_DIR=`。キャッシュの有無で IR・診断・終了コードは変わらない。Phase 2（D10、関数本体の検査結果の再利用）は、本体の検査が
  現実的な 1,000 モジュールの `check` の 18.2% で判断の基準 25% に届かないため実測で見送った（F12 の Phase 2 と同じ扱い）。Phase 3（D11）はソースの
  上限 `MAX_SOURCE_BYTES` を 4 MiB（`E0003`。`Tsuzuri.toml`・`Tsuzuri.lock`・registry の index は `MAX_PACKAGE_FILE_BYTES` の 1 MiB のまま）、特殊化の
  上限を 65,536（`E1017`）にした。型が育つ多相再帰は、自分の型引数を包んだ型で自分を直接呼ぶ関数を最初の特殊化で、ほかの関数を通る成長を
  1 つの呼び出しの系列に同じ関数が 32 回現れた時点で `E1017` にする（分岐する成長は全体の上限が止める）。D-19 の上限の例と §1.8・§11.1 を改めた。
  frontend cache のコンパイラの同一性は、実行ファイルの大きさ・更新時刻に加え、Unix では device・inode・ctime、ほかでは path と作成時刻を使う。
- G13（REPL とスクリプト）: `tsuzuri repl`（入力ごとに `Main.tz` を作り直して検査・実行する再コンパイル型。状態は持ち越さず、受理した束縛を毎回実行し直す。
  `--timeout`）と、D12 の `tsuzuri script FILE [args...]`。shebang は、どのソースでも byte 0 の `#!` 行を行コメントとして読む（`#` で始まる正しい
  プログラムはないので、既存のプログラムの意味は変わらない。D-15）。D13（JIT）は再検討しても採用しない。LLVM の C API への結合・新しい crate・
  `unsafe`・配布物の前提の変更が要り、docs/architecture.md の方針と衝突する。代わりに 1 入力の待ち時間を段ごとに測り（docs/benchmarks.md）、表示できる
  式の解析を 1 回にした。残る費用は PB01・PB05・PB06 の対象。

### D-42 B08 の全フェーズと現行仕様の確定

- 2026-10-09、利用者の B08 全フェーズの完遂、最新仕様との再検討、`yield_now` を `yield` とする依頼、および必要な判断への包括承認を、
  B08 D1・D10 と Phase 3 の承認として扱った。着手時点は `ce7e8a1`。B07・E08・E13・E14 は完了済みで、旧 HEAD `f8dc655` の計画より
  D-34〜D-41 の整数既定 i32、`def ... :: ... = \...`、名前空間、opt-in std、現在の文書配置を優先した。
- 利用者向けの中断点は `Async.yield ()`。`yield` は `union` と同じく関数宣言名とドットの後で名前として読めるが、単独では計算式の文。
  `Async` の builder operation `Yield` は実装せず、`yield_now` の別名も作らない。formatter・LSP・VS Code の文法も同じ規則にする。
- `Async<'a>` は不透明・非 Copy の cold 計算。既存の計算式の展開を使い、状態機械や stackful coroutine は足さない。
  通常の Copy な closure が継続環境を再帰的に複製する問題を避けるため、不透明・非 Copy の `Next<'x,'y>` と std 専用の consuming
  builtin `Async.__resume` を使う（旧 D2 の LLVM 変更禁止を改めた）。Async を使わない 6 組の IR はバイト比較で維持する。
- `Async<T>` の式は実際の loan を保持できない（E1013）。抽象的な関数引数の placeholder loan だけを呼び出し側で具体化して検査し、
  std 本体の検査は免除しない。所有した捕捉値を中断点の内側で借用することはできる。継続・集約状態を `Vec.pop` で move するため、
  `all` / `all_results` の結果に旧 D7 の Copy 制約は要らない。開始・round は入力順、Error の短絡はスケジュール順。
- Phase 2 は `Async.Operation { start, cancel }`、`Async.host`、`Async.start`、`tsuzuri_async_poll(now)`、
  `tsuzuri_async_complete(operation,value)`。start は cold な計算を投入して戻り、最初の poll が開始する。未完了のホスト操作を破棄すると
  cancel を 1 回呼ぶ。poll の再入と未知・二重・取消済みの raw complete は trap。`run` は独立した仮想時計で、ホスト操作を拒否する。
- Phase 3 は `Async.block_on :: Async<'a> -> IO<'a>`。native の状態は TLS、操作 ID の bits 39–62 は再利用しないスレッド index。
  POSIX の単調時計・条件変数・登録操作の mailbox で待ち、他スレッドの `int32_t tsuzuri_async_post` は受理時 1、未知・二重・退役時 0 を返す。
  完了・取消時には競合した完了を除去し、最後の操作の退役時に Tsuzuri allocator で確保した mailbox を解放する。
  kqueue／epoll／IOCP を先に足す旧案は再検討し、B08 に必要なタイマーと外部完了だけを実装した。ソケット API は E09 に残す。
- WASM の block_on は明示的な `--wasm-feature jspi`。既定の純粋な Async の WASM import は増やさない。raw WASM のホストは操作開始前に
  `tsuzuri_async_set_epoch` を設定し、再作成前と異なる世代を使う。生成グルーは同じ生成モジュール内で世代を再利用せず、
  `bindings.async.complete` と `settled()` で自動駆動する。JSPI の export はすべて Promise で直列化し、typed array は要求時にコピーする。
  `withBorrowed` は提供しない。失敗はインスタンスを再作成するまで保持し、未観測のバックグラウンド失敗は診断する。
- native の test/debug-test/bench も reactor をリンクする。未対応の native Windows reactor は E2002（D-43 で Windows 対応に置き換えた）、
  WASM の host executor を使うテスト実行器は E2000。WASM threads の executor TLS と `--trap-mode return` の状態復旧は未実装なので
  明示的に拒否する。新しい Rust crate、unsafe Rust、既定の host import、stack・特殊化の上限引き上げはない。

### D-43 native reactor の Windows 対応とビルダーの別名（`@alias`）

- 2026-10-09、利用者の「native 実時間リアクターの Windows 対応」と「コンピュテーション式の `@alias computation-alias` 構文」の依頼、
  および必要な判断への包括承認を、B08 の後続として扱った。着手時点は `2cc2b21`（PR #19 のマージ）。
- **Windows の reactor** は `src/runtime/async.c` の薄いプラットフォーム層（`tz_async_ready/acquire/release/notify/wait_signal/
  wait_timeout`）で足す。POSIX は pthread の mutex・条件変数・`CLOCK_MONOTONIC`、Windows は `SRWLOCK`・`CONDITION_VARIABLE`
  （`SleepConditionVariableSRW`）・`QueryPerformanceCounter`。mailbox・操作 ID・`tsuzuri_async_post` の規則は変えない。
  IOCP は足さない（D-42 と同じく、B08 に必要なタイマーと外部完了だけ）。待ち時間は 1 日で頭打ちにし、起きるたびに単調時計を読み直すので、
  Windows の約 15.6 ms の粒度は遅れとしてだけ現れ、早く起きない。`timeBeginPeriod` はプロセス全体の電源設定を変えるので使わない。
  runtime を埋め込む Windows の COFF `--emit object` は task・CPU・IO と同じく非対応のまま（LLVM を出して runtime を 1 回リンクするか、実行ファイルを作る）。
  対応の判定は driver と test_runner が共有する `Platform::reactor`（macOS・Linux・Windows。G15 で `ASYNC_NATIVE_SUPPORTED` から移した）で、それ以外の host は E2002。
- Windows の実行は CI だけで確かめる（ローカルに Windows の実行環境がない）。同梱の MinGW toolchain（x64 は Zig、ARM64 は llvm-mingw）と
  x64 の LLVM MSVC で `tests/async.mjs` を実行し、toolchain の smoke にも `Async.block_on` の実行ファイルとテストを足した。同梱 toolchain は
  `wasm32-unknown-unknown` だけを受けるので、その手順は `TSUZURI_WASM64=0` で wasm64 の JSPI を除く（既存の制限で、Async の制限ではない）。
  x64 の同梱 toolchain で最適化した host テストが `0xC000001D` で落ちた原因は reactor ではなかった。`zig cc` は `-O1` 以上で `NDEBUG` を定義し、
  テストの C host が `assert` の中で executor を呼んでいたため、呼び出し自体が消えて操作が始まらないまま完了を送っていた（`-O0` だけが通った）。
  host に `#undef NDEBUG` を足して実際に確かめ直した。この教訓は AGENTS.md の CI の項に入れた。
- **`@alias name`** は `.tc` のトップレベルに 1 行で書く。「コンピュテーション式に設定した alias」という依頼に合わせて、宣言はビルダー側の
  `.tc` に置き、利用側の `.tz` に別名の宣言を増やさない。複数宣言でき、`.tz`・`.tt` に書くと E1018。
  - 名前は同じ行の小文字 ASCII で始まる予約語でない識別子（`finally`・`is`・`namespace`・`of`・`try`・`using`・`where` も不可）。
    ビルダー名（`.tc` のファイル名）は常に大文字始まりなので、小文字なら衝突せず、大文字始まりを許すとモジュール名や型名と区別できないため。
    レコードは小文字でも宣言できるので、`name { field: value }` はレコードリテラルのまま、本体が空の `name {}` は別名を先に選ぶ
    （`Name.tc` と空レコード `Name` と同じ規則）。ファイル内の重複とドキュメントコメントの前置も E0002。
  - 別名はビルダー名のもう 1 つの綴り。`computation::expand` が別名をビルダーのモジュールの鍵へ書き換えるので、展開以降は同一で、
    構文木・型付き IR・ランタイムに別名のための構造はない（`async {}` と `Async {}` の IR はバイト単位で一致する）。
  - 解決は `Names::builder_name`。まずビルダーのモジュール名（名前空間付きを含む）、次に別名。別名は利用者 → std の層ごとに、
    ビルダー自身の名前がその参照元から見える候補だけを数える（`Names::builder_in_sight`。2 つの `using` が同名のビルダーを持ち、
    名前自体が曖昧なときも、そのどちらも候補に数える）。1 つなら採用し、複数なら E1004 でビルダーの名前を書くよう案内する。
    本体が空の `name {}` は、別名があれば別名、なければその名前のレコードがない小文字の名前だけを宣言のないビルダーとして E1018 にする
    （`Names::empty_block_is_computation`。小文字のレコードは宣言できるので、その空リテラルは残す）。
    名前空間を前置した別名（`Ns::alias`）は作らない。レキサーは `::` の後に小文字の識別子をつなげず、利用者の混乱の元になる。
  - opt-in std（D-40）の `Async` は `OptIn::aliases` に `async` を持ち、ソースに `async` の語があれば読み込む。`Names::choose` の
    「opt-in の宣言を裸の名前から隠す」規則は別名に適用しない。そうしないと `async { ... }` が使えない。語の走査は小文字では別名と一致したものだけ数え、
    ほかのプログラムの型検査の時間と IR は変わらない。`asynchronous` のような別の語の一部は数えない。
  - 診断は新しいコードを足さず、E0002（宣言の誤り）、E1018（`.tc` 以外の `@alias`、宣言のない小文字のビルダー）、E1004（曖昧）を使う。
  - tooling は formatter（正規化と AST の指紋）、syntax codec（interface を含む。`Program` の符号化が変わるので `FRONTEND_FORMAT` を 2 に上げ、
    golden を更新した）、`tsuzuri doc`（`Builder alias:` の行）、VS Code の文法。
    LSP はビルダー名のトークンを索引しないので、別名専用の処理は足さない。

### D-44 第2期の 4 チケット（E09・F09・F10・C11）の確定

- 2026-10-10、利用者の「E09、F09、F10、C11 の実装をすべて完遂して。もし現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して。
  複数フェーズある場合には、すべてのフェーズを完了させること。…何か判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」を、
  4 チケットの `要承認` の決定事項すべての承認として扱った。着手時点は `96d7cbf`。旧 HEAD `f8dc655` の計画は、D-34〜D-43 の整数既定 `i32`、opt-in std（D-40）、
  `_tsuzuri/language-reference/` への文書の移動、`File.Handle`（世代検査付きの表を指す Copy の添字）、B08 の reactor がタイマーと mailbox だけであること（D-42・D-43）、
  E14 の `--trap-mode return`、C10 の `Rc`・`Arc` と照合して見直した。詳細は各チケットの「実装と検証」にある。
- 共通: 新しい std モジュール 7 個（`Net`・`Atomic`・`Mutex`・`Channel`・`Matrix`・`MatrixView`・`Tensor`）はすべて opt-in（`stdlib::OPT_IN`。D-40）で、
  予約名は 46 個から 53 個、組み込みクラスは 31 個から 33 個（`Sync`・`AtomicValue`）。`MatrixView` は `Matrix`、`Tensor` は `Matrix` と `MatrixView`、`Net` は `Async` を
  `uses` で連れてくる。`opaque_record` と `vsc/src/core.ts` の `libraryModules`（予約名と一致させる）も同じ名前で更新した。名前を書かないプログラムの IR はバイト単位で変わらない
  （チケットごとに基準のコンパイラと 274〜342 組を比較した）。例外は F09 で、std の `Gpu.tz` に関数が増えたため、単相化の `$intrinsic.N`／`$instance.N` の番号が
  基準 144 組のうち 24 組で振り直される（番号を正規化すると差は 0）。新しい診断コード、crate、unsafe Rust、fast-math、既定の host import はない。
  すべてのフェーズの diff を独立した code review にかけ、指摘を再現してからテスト付きで直した。各項の「review で直したこと」にその一覧がある。
- E09（`Net`）: ticket の D4（`Drop` で閉じる非 Copy の handle）は std の型が `Drop` を持てず（E1016）、`Drop` が `let!` をまたげない（E1005）ので実現できない。
  代わりに `File.Handle` と同じ `Copy` の不透明 record にし、`(generation << 32) | (slot + 1)` で世代検査付きの表を指す（閉じた・古い・種類違いの handle は
  `InvalidInput`／`EBADF`）。`Net.Address`・`Net.TcpStream`・`Net.TcpListener`・`Net.UdpSocket` は `Copy`、閉じ忘れは終了時まで漏れる（括弧の `with_connection`・`with_accepted`・
  `with_listener`・`with_udp` を用意した）。API は `IO<Result<T, Os.Error>>`: `resolve`・`connect`・`read`・`write`・`shutdown`・`close`・`bind`・`accept`・`close_listener`・
  `bind_udp`・`send_to`・`recv_from`・`close_udp`・`error_kind`（`Net.ErrorKind` 7 case）と、純粋なアドレスの `parse_address`・`parse_ip`・`address_text`・`ip_text`・`port`・`is_ipv6`。
  アドレスの解析は厳密（`inet_aton` の形・ゾーン・先頭 0・空白は `None`）で、表示は RFC 5952。時間制限は `Maybe<i64>` の呼び出し全体の期限、受信は 1〜16 MiB、UDP は切り詰めずに `EMSGSIZE`。
  socket は `FD_CLOEXEC`・SIGPIPE 抑止・`IPV6_V6ONLY`・listener の `SO_REUSEADDR`（Windows は `SO_EXCLUSIVEADDRUSE`）。
  - `resolve` は、アドレスに読める host（`:`／`%`、空白か制御文字、ASCII 以外のバイト、最後のラベルが数字だけか `0x`）を OS に渡さず `parse_ip` だけで決め（一致すれば OS を呼ばない 1 件、
    そうでなければ `InvalidInput`／code 0）、ランタイムは `getaddrinfo(AI_NUMERICHOST)` が数値と読む host を断る。システムの `inet_aton` 系が `0x7f.1` や全角の数字を数値と読み、
    利用者の SSRF の検査をすり抜けるため。国際化ドメインは `xn--` で書く（native でも `éxample.com` は `InvalidInput`）。wasm32 は `dns.lookup` に `AI_NUMERICHOST` を渡せないので分類だけが守る。
  - 非同期は `_async` の双子 12 個（`Async.host` の上。`IO` は `Async` の中で動かせないため）。B08 の reactor には登録せず、1 本の分離した監視スレッド（`poll`／`WSAPoll` と起床用 pipe）が
    `tsuzuri_async_post` へ完了を入れる。`resolve_async` はない（`getaddrinfo` は取り消せない）。`Async.block_on` のない native で非同期の操作に到達すると E2000。時間制限 0 は「1 回だけ試す」。
  - runtime は `src/runtime/net.c`（IR が `@tsuzuri_net_` を宣言したときだけ連結。`-pthread`、Windows は `-lws2_32`）。macOS・Linux・Windows 以外は E2002。Windows は Winsock で、UDP は `SIO_UDP_CONNRESET` を切り、
    `accept` は接続前に RST された `WSAECONNRESET` を飛ばす（POSIX に揃える）。builtin は std 専用の `Net.__*`（`E1022`）。既定の wasm32 と `--wasm-host wasi` は import を足さず E2000。
  - `--wasm-feature net`（`jspi` が必要。wasm32 の `object`・`llvm`・`wasm`・`bindings-js` だけで、threads・WASI・wasm64・native とは排他。違反は E2000）は、`tsuzuri_net` の import 14 個（JSPI で中断する
    ものと、時間制限 0 の `try_read`／`try_accept`）を足し、生成グルーが Node.js 24 の `node:net`・`node:dgram`・`node:dns` で実装する。ブラウザーは生の TCP／UDP がないので `load` で失敗する。
    グルーは `reaches_net`（ソケットに到達するモジュールだけ）で選ぶ。ホストの例外は呼び出しの status に直し（モジュール自身の trap だけが通る）、破棄したインスタンスの socket は閉じ、
    UDP の受信は 1 MiB・4096 件、listener は 128 接続で頭打ちにし、TCP は `allowHalfOpen`、ポインタは `>>> 0`。`bind_async`・`bind_udp_async` は `Async.block_on` が要る（Node の listen/bind に同期の道がない）。
    UDP の送信は datagram を 1 回だけ Node に渡し、その送信自身の結果を token で返す（`Net.__send` が `ACCEPTED` と token を返し、std の `send_loop` が `Net.__watch` で待つ。再試行はなく、native の status は変わらない）。
    `close_udp` は送信中の datagram を待ってから Node の socket を閉じる。接続の相手の名前は接続が届いたときに記録する。errno の表は `os.constants.errno` に libuv の名前（`EHOSTDOWN` など）を足す。
  - review で直したこと: Phase 1 の `resolve` の数値 host の差（上記）、Phase 2 の CI（`vsc/` から動かないテスト、errno 99 の仮定、macOS の Homebrew LLVM の ASan が起動で止まる件は
    `TSUZURI_NO_SANITIZERS=1`）と Windows の UDP／accept、Phase 3 の 11 件（Unicode の host、破棄したインスタンスの socket、JS 例外が「stack exhausted」になる件、半閉じ、UDP の送信失敗が受信を汚す件、
    アイドル中の reset、無制限の queue、ソケットのないモジュールのグルー、`Async.start` の `SuspendError`、2 GiB 以上のポインタ、`memory64` の import 名の走査）、その修正の review で見つかった UDP の再試行が datagram を重複・欠落させる件。
  - 限界: Linux は実行未確認（glibc・musl の x86_64・aarch64 はコンパイルのみ）、Windows の実行は CI だけ、wasm32 は macOS の Node.js 24 だけ、TLS・HTTP はない（E10）。wasm32 で同時のホスト操作が数千になると、
    allocator の空き領域の走査で 1 操作あたりの時間が伸びる（4000 件で 1 件 3.1 ms。`-O3`）。
- F09（`Gpu`）: Phase 1・2・3 を実装した。厳密な `Gpu.init`／`Gpu.map` は厳密のまま、浮動小数点の GPU 実行は名前で選ぶ（D-14）。
  - Phase 1: `Gpu.init_relaxed`／`Gpu.map_relaxed`（`f16`・`f32`・`i32`・`i32u`。CPU の参照は厳密に評価する）と `--emit wgsl-relaxed`（1 行目 `// tsuzuri-gpu float=relaxed input=<t> output=<t>`、
    `f16` を使うときは `enable f16;`、f32／f16 のリテラルはビットパターン。浮動小数点から整数への変換・f64・`**`・bool・64 bit は E1018）。
  - Phase 2: `Gpu.request Gpu.WebGpu` は、runtime が WebGPU の device を開き、プログラムの kernel が要る機能（`shader-f16`）をすべて持つときだけ `Ok`、そうでなければ `Error Gpu.Unavailable`（CPU への黙った代替はない）。
    WebGPU の device を作る関数を利用者が書いたプログラムだけが kernel を埋め込み（`CheckedModule.gpu`、`@tz.gpu.kernels`）、`Gpu.request`／`init`／`map` を `request_on`／`init_on`／`map_on` へ付け替える
    （`src/gpu_devices.rs`）。ほかのプログラムは GPU runtime も import も持たない。device の関数を指す `Gpu.__open`／`__features`／`__run`（と `request_on` など）は std 専用（`E1022`）。
    native は `src/runtime/gpu.c` が wgpu-native 29 を `dlopen`／`LoadLibrary` する（`TSUZURI_WEBGPU_LIBRARY` で指名、空なら無効。リンク時の依存なし。`wgpuGetVersion` が 0 のビルドは指名したときだけ受理）。
    WASM は `--wasm-feature webgpu`（wasm32 の object・llvm・wasm。threads・`--wasm-host`・`bindings-js` とは排他）が `tsuzuri_gpu.open`／`run` の import を足し、`createGpuImports`（JSPI）が実装する。
    厳密な呼び出しの device kernel は `i32`／`i32u` の lane と厳密な WGSL の callback だけ（CPU 参照とビットが一致）。ほかの厳密な呼び出しは理由を stderr に書いて trap する。
    1 呼び出しごとに host 配列をコピー・upload・dispatch・待ち・read back し、GPU 常駐の buffer はない。1 device／1 プロセス、呼び出しは直列。
  - Phase 3: 自前の SPIR-V emitter（`src/gpu/spirv.rs`。LLVM の SPIR-V target は `NoContraction` を出さず、Vulkan は付けないと乗加算の融合を許すため使わない）、`Gpu.request Gpu.Vulkan`（native。
    ローダーを `dlopen`／`LoadLibrary`、`TSUZURI_VULKAN_LIBRARY`、portability 列挙、`shaderInt64`、float control。WASM は常に `Unavailable`）、`Gpu.Auto`（常に `Ok`。呼び出しごとに CPU 参照か Vulkan を選ぶ）、
    公開の `Gpu.last_backend`、`--emit spirv|spirv-relaxed`（`.spv`。ビルド cache には入れない）、64 bit 整数の kernel（`shaderInt64`）。厳密な `f32` は `+ - *`・単項 `-`・比較・整数との変換だけで、
    `/` は E1018（`OpFDiv` は 2.5 ULP）。f32 から整数への変換は範囲比較と `OpIsNan` で Tsuzuri の飽和・NaN を再現する。整数の `/ % **`・`f64`・`f16`・`bool` は厳密では E1018。
    **厳密な `f32` は device の float control の性質に加えて 27 lane・9 演算の conformance probe に通ったときだけ使う**（性質は申告であって証明ではない。MoltenVK は SignedZeroInfNanPreserve を申告しながら
    `-(x*0.0)` が `-0.0` にならず、SwiftShader は 2^31 以上の u32 から f32 への丸めが違った）。probe は 1 回だけ、厳密な `f32` を要する最初の要求で走り、失敗するとそのプロセスでは厳密な `f32` を断る。
    probe は標本であって証明ではない。`Gpu.request Gpu.Vulkan` は全 kernel の機能の OR を、`Gpu.Auto` は kernel ごとの機能を確かめる。明示した backend は CPU へ落ちない。
  - `Gpu.Auto` の規則: 候補は CPU と Vulkan だけ（WebGPU には測った規則がない）。Vulkan は統合 GPU でメモリを CPU と共有するときだけ。使える判定（module・lane 数・機能と probe・limit・pipeline）は device が報告する内容で
    **device を作る前**に行い、合う device がなければ論理 device を作らない → 費用の見積もり（CPU `n·w·0.03 ns`、Vulkan `270 µs + n·b·0.13 ns + n·w·0.001 ns`。`w` は 1 lane が実行する演算数の**下限**で、`if` は条件と安い腕、
    `&&`／`||` は左の項だけを数え、呼ぶ関数は同じ規則で数える。上限は `i32::MAX`（記述子の欄の幅）。`b` は 1 lane の入出力のバイト数）→ 最初の使用の費用（device を開く 30 ms、pipeline の 7 ms）をスキーレンタルの credit で払う。準備の失敗は CPU、
    実行中の失敗は明示した device と同じ trap。判定が CPU になる呼び出しは実行中の GPU を待たない（runtime の state lock と run lock を分け、棄却する判定は lock を取らず、厳密な `f32` の probe が要る判定も run lock を try-lock して、
    取れなければ verdict を記録せず CPU に回して次の判定でやり直す。明示的な要求は待ってよい）。プロセスの論理 device は 1 つで、
    最初の要求が決める（離散と統合の両方がある機械で、`Auto` が先なら統合 GPU を開いてあとの明示的な `Gpu.Vulkan` も使い、明示的な要求が先なら離散 GPU を開いて `Auto` は以後 CPU。device を 2 つ持つには device ごとの状態と
    呼び出しの種別の区別が要るので、まだない）。`TSUZURI_GPU_AUTO_MIN_WORK=<n>` は規則を `lanes×weight >= n` に置き換えるが、機能・class・limit の検査は緩めない。ticket の「閾値」は、1 回の閾値では
    少しずつ得する呼び出しの繰り返しで device を開かないため、費用の規則に改めた。定数は 1 台の機械（M1 Max）の経験値で（負荷の高いときと静かなときの 2 回の測定で同じ範囲に入った）、CI に速度の閾値は入れない。
  - 環境変数: `TSUZURI_WEBGPU_LIBRARY`・`TSUZURI_VULKAN_LIBRARY`（名指しのパス）、`TSUZURI_GPU_DEBUG`（空でなければ有効。理由と dispatch の経過）、`TSUZURI_GPU_AUTO_MIN_WORK`。ライブラリの既定の探索は system の場所だけ
    （macOS は絶対パスだけ。dyld がカレントディレクトリの同名ファイルを先に読むため。Windows は `LOAD_LIBRARY_SEARCH_SYSTEM32`。Linux は動的リンカーが作業ディレクトリを探さないので名前のまま）。
    Vulkan のローダーが読み込めても `vkGetInstanceProcAddr` を持たなければ読み飛ばして探索を続ける。runtime は `VK_*` の変数を読み書きしない。
  - 記述子は `{flags, lanes, features, wgsl, wgsl_len, spirv, spirv_len, weight}`（features の bit1 が i64、bit2 が厳密な f32、lane の種別 4 が 64 bit 整数）。境界は `tsuzuri_gpu_open`／`run`／`select`。IR の印
    `; tsuzuri-gpu: vulkan` が `src/runtime/gpu-vulkan.c` を連結する。runtime の `#undef NDEBUG`（`zig cc` の `-O1` 以上で `assert` が消える件。D-43）。
  - review で直したこと: WGSL の `mut` 引数と文の入れ子（else-if 63 個以上は E1017）、利用者が呼べた `request_on`／`init_on`／`map_on`（kernel の添字を偽造できた。非公開にし、記述子の lane 種別を `Gpu.__run` が確かめる）、
    device の limit を adapter でなく開いた device から、失敗した object の cache、2 GiB 以上のポインタ、カレントディレクトリからのライブラリ読み込み（WebGPU・Vulkan とも）、スタックに置いた待ち状態、
    SPIR-V の probe の抜け（符号なしの変換が切り捨てと最近接偶数丸めを区別できなかった）、`Gpu.Auto` の判定が実行中の GPU を待つ件（厳密な `f32` の probe の経路も）、分岐の両腕を足していた weight と `i32` を超える weight、
    統合 GPU と離散 GPU の両方がある機械、テストの変異生存。
  - 限界: 確かめたのは Apple M1 Max 1 台（Metal 上の wgpu-native と Dawn、MoltenVK）とソフトウェア実装の SwiftShader だけ。厳密な `f32` を通す device は未確認（MoltenVK は probe で断る）、Linux・Windows・x86・CUDA・Metal 直接は未実行、
    `Gpu.Auto` は WebGPU を選ばず、WASM の `Auto` は常に CPU。速度の優位は主張しない（測定と限界は docs/benchmarks.md）。
- F10（`Atomic`・`Mutex`・`Channel`・`Task.scope`・`Sync`）: Phase 1・2 を実装した。
  - `Atomic<'a>`（`i8`〜`i64u` と `bool`。組み込みクラス `AtomicValue`）: `create`・`load`・`store`・`swap`・`compare_exchange`・`fetch_add`／`sub`／`and`／`or`／`xor`（整数だけ）・`into_inner`。seq_cst。
    `new` と `with` は予約語なので、構築は `create`、ロックは `Mutex.with_lock`（`Bench.with_input` と同じ）。`Mutex<'a>`: `create`・`with_lock`・`into_inner`。poison はなく、ロック中の入れ子・
    並列の開始・`Channel` の操作は trap（`tsuzuri_mutex_parallel_ok`・`tsuzuri_mutex_wait_ok`）。`--trap-mode return` の境界は弱い hook `tsuzuri_sync_hooks.abandon` でロックを解放する。
  - 組み込み `Task.scope :: (Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]`（共有借用を子へ渡す fork／join。子 1 つにつき結果 1 つ）と組み込みクラス `Sync`／`AtomicValue`
    （`BUILTIN_CLASSES`。利用者の instance は E1016）。`Function` は型の上では `Sync` で、安全は「共有する関数の環境は所有したものだけ」（E1013）で保つ。`Arc<T>` は `T` が `Send` かつ `Sync` のとき `Send`
    （D-40 の「`T` が `Send`」を改めた）。**型変数は `Send` でも `Sync` でもない**（型変数に引数を適用した高カインドの型 `'f<i64>` と部分適用も「まだ分からない」として同じに扱う）: `Send<'a>`・`Sync<'a>`（`Channel.bounded`・`Mutex.create`・`Parallel.*`・`Task.scope`・`Dyn.of` の `dyn (C, Send)`・利用者の制約）は
    ジェネリック関数の制約として残り、使う型ごとに検査する（既定メソッドと instance メソッドが要るときは宣言する。E1027）。F10 以前から `Rc` を運ぶ `Parallel.init` などが通っていた穴も塞いだ。
    `can_send`・`sync_here` は `Type` の全種類を網羅する match で、新しい型の種類を足すときはここで分類しないとコンパイルが通らない。
  - `Mutex.with_lock` の結果が借用を持ち得る型なら、callback の結果が借用した環境を持たないことを証明する（E1013）。`Channel.send` は、要素の型が借用を持ち得る（関数など）とき、要素が loan を持たない
    （E1013）。直接の 2 引数の呼び出しだけで、部分適用・関数値・pipe は拒否する。`Channel.recv` の結果は loan を持たない。関数型の引数を `Channel.send` へ渡すジェネリック関数は、使う型で拒否する
    （関数は送る場所で作る）。
  - `Channel`（有界・多対多）: `bounded`・`send :: ... -> Result<unit,'a>`（受信側がいないとき要素を失わない）・`recv`・`clone_sender`。`Sender`・`Receiver` は `{ block: i64 }` の不透明 record で、std の `Drop` の instance が閉じる
    （std のモジュールは自分で宣言した型に `Drop` を書ける。利用者が std の型に書くのは従来どおり E1016）。`Channel.__close_sender`／`__close_receiver` は std 専用（E1022）。閉じ忘れた `Sender` は待ちの規則が見つける。
  - 待ち: channel を待つ thread は、idle な worker がなく未開始の item があるときだけ、最も古い group の item を自分のスタックで手伝い（深さ 16）、そうでなければ止まる。join を待つ thread（`Task.scope`・`Task.parallel` の終わり）は
    自分の group の item だけを動かす。最後に止まる thread が `Tsuzuri runtime: deadlock: every task is waiting on a channel`（Assert の trap）の判定をし、全員を起こす。逐次の WASM と 1 CPU の native は、満たされない待ちで trap する。
    完了は thread 数に依る（受け入れ済み）。1 つの pool lock が全 channel を守る（限界）。WASM threads の pool も native と同じ規則で、group は末尾に足して古いものから取り、`submit` は group の最初の item を公開する lock の中で取る。
  - WASM threads でも `Mutex`・`Channel` が動く。worker ごとに module instance があるので wasm の global が thread-local になり（`__wasm_init_tls` なし、`ThreadState` も変えない）、共有の epoch word で待つ。
    epoch が動くのは新しい仕事・group の最後の item・誰かが待つ channel の変化・競合したロックの解放だけで、起こすのは lock を放してから。sleeper は登録した epoch が動くまで眠り、眠る前に失敗の印（host の `fail()`）を確かめる。
    `--freestanding` は両方とも E2000。
  - review で直したこと: `Mutex.with_lock` の結果が借用を運ぶ、`Task.scope`／`Parallel.*` の一時の関数が借用した環境を共有する、再帰型の `can_capture` が `Arc` の中を見る、`Channel.send` が借用する関数を渡せる、
    `Send` がジェネリックを通ると検査されない（型変数に引数を適用した高カインドの型 `'f<i64>` も。`Rc` を運ぶ holder で 2 つの task が非原子的に数える競合になった）、WASM threads の pool が Channel を使わないプログラムまで遅くなる
    （100 万 child・7 worker で 360 ms → 2,216 ms → 233 ms）、3 stage の pipeline と、入れ子の `Task.parallel` の後に送る 2 task のプログラムが偽の deadlock 判定になる（join を待つ thread が別の group の item を取っていた）、
    host の失敗と眠りの競合。
  - 限界: lock は 1 つの pool lock、`Arc` の循環（`Mutex` を通す）は回収しない、`with_lock` を包むジェネリック関数は借用を持ち得る結果を拒否する、native の Windows・Linux は実行確認なし（CI）。
- C11（`Matrix`・`MatrixView`・`Tensor`）: Phase 1・2 を実装した。std のソースだけで書かれ、`Builtin`・`Type` の variant・ランタイム関数・WASM の import はない。`opaque_record` は `Matrix.Matrix`・
  `MatrixView.MatrixView`・`MatrixView.Mut`・`Tensor.Tensor`・`Tensor.View`、`Type::is_noncopy_record` は `Matrix.Matrix`・`Tensor.Tensor`・`Tensor.View`。
  - `Matrix.mul` の各出力要素は `+0` から `k` の昇順に別々に丸めた和（D-14）。実装は出力の行ごとの更新（i-k-j。`left` の行を先にスライスしないと out-of-line で約半分の速さ）だが、ビットは変わらない。順序の違う積は別名にした:
    `mul_fma`（`Math.fma` の順）、チャンクが形だけで決まる `mul_parallel`・`mul_fma_parallel`（約 2^20 回の積和ごと）。`Math.fma` がハードウェアの命令になるのは AArch64 の native（ビルド機が AArch64 のときの `--target native`、または AArch64 の triple。G15 で、コンパイラ自身の arch から target に改めた）だけで、
    それ以外の native と wasm32 では `tz_soft_fma`（1 積和あたり約 1.6 µs。128³ の `mul_fma` が約 3.4 s で `mul` の約 4,000 倍。ビットは同じ）。
  - 窓は region 付きの借用 record: 局所の所有者の窓は返せない（E1013）、所有者の move・置換と `Mut` が生きている間の元の配列の使用は E1014、フィールド参照などは E1022。`MatrixView.Mut` は行優先かその転置の配置だけで、
    同じ slot を指す要素がない。`Tensor` は軸 16 本まで、長さ 0 の軸を除いた積が `i64` に収まる。窓の不変条件（`stride >= 0`、全要素が `data` の中）は `strided` が桁あふれなしで確かめる。
  - GPU との連携は `Matrix.as_array`／`of_array` と `Gpu.from_array`／`map`／`to_array` の組み合わせだけで、行列積の GPU kernel はない（kernel の部分集合に 2 次元の添字も総和もない）。SIMD・`@cpu`・builtin は足していない。
    benchmark は `benchmarks/run-matrix.mjs`（C の i-j-k・i-k-j・`fma` と、チェックサムをビット単位で比べてから測る）。
  - review で直したこと: FMA のテストが host の CPU に依っていた件、FMA の文書の言い過ぎ、`extent` の `+ 1` の桁あふれ、`MatrixView.mul` の順序、空の窓のループ。
  - 限界: x86-64・SVE／AVX-512・実機 GPU・ブラウザの WASM threads は未検証、速度は M1 Max 1 台の測定。

### D-45 第2期の最後の 4 チケット（D10・D11・G15・G19）の確定

- 2026-10-11、利用者の「D10、D11、G15、G19 の実装をすべて完遂して。もし現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して。
  複数フェーズある場合には、すべてのフェーズを完了させること。…何か判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」を、
  4 チケットの `要承認` の決定事項すべての承認として扱った（D11 の旧 D10＝Phase 2 の静的データ、G15 の旧 D8＝CI の `targets.yml`、G19 の旧 D1・D4・D9）。
  着手時点は `053bced8`（PR #21 のマージ）。旧 HEAD `f8dc655` の計画は、D-34〜D-44 の整数既定 `i32`、`def name :: type`、名前空間 `A::B::Module`、opt-in std、
  文書の移動（`_docs/` → `_tsuzuri/language-reference/`）、frontend cache と syntax codec（G17）、`--warn`・`W1006`（A15）、E10 の manifest と git／registry 依存、
  B08・E09・F09・E13・E14 などが足した host 依存の経路（reactor、`Net`、GPU の `dlopen`、`--emit shared`、trap return、bench runner）と照合して見直した。
  詳細は各チケットの「実装と検証」（`_features/_completed/`）にある。実装は 4 つの worktree で並行し、統合のあとに独立した code review をかけた。
- 共通: 新しい予約語・crate・`unsafe`・既定の WASM の import・std モジュールはない。新しい診断コードは `W1005` だけ（D-16 へ移した）。新しい CLI は `tsuzuri api-diff` と
  `--target TRIPLE`（22 個）、新しい環境変数は `TSUZURI_SYSROOT`・`TSUZURI_RUNNER`（コンパイラが読む）と、テスト用の `TSUZURI_CROSS_LINK`・`TSUZURI_CROSS_CLANG`・
  `TSUZURI_CROSS_EXECUTE`・`TSUZURI_F16_EXPECT`。`FRONTEND_FORMAT` は D11（`const` の印）と G19（edition を parse の key に入れる）で 2 から 3 になった。
  新しい機能を使わないプログラムの IR・object・`.wasm` は、各チケットが基準のコンパイラ（`053bced8`）とバイト単位で比べて同じだった。例外は意図したもので、D10 は
  `f16` の演算を使う native の IR と object（`half` 命令になる）、D11 は定数を `Array.length (&Values)` のように借用して読む形（静的データを指す）。
- D10（`f16` の `half` 命令）: 経路は `llvm::F16Path`（`Soft`・`Conversions`・`Hardware`）で、`driver::f16_path(wasm, os, arch, cpu, CpuFeatures)` が target とその CPU から 1 回だけ選ぶ
  純関数。実行時の dispatch はなく、`llvm::emit`・`emit_target` と WASM は常に `Soft`。AArch64 は OS によらず `Hardware`（FEAT_FP16 がなければ LLVM が演算ごとに `f32` を 1 回挟む。
  この形は libcall がなく 1 回の丸めと同じ結果で、機械語を AArch64 の Linux・musl・Windows・macOS で確かめた。広げすぎなら `AARCH64_HALF_WITHOUT_FP16` の 1 行で狭められる）。
  x86-64 は `--cpu native` のときだけ、AVX512-FP16 があれば `Hardware`、F16C があれば `Conversions`（f16 から f32／f64、f32 から f16 だけ。f64 から f16 は直接の命令がなく、f32 を経由すると
  2 回丸めになるのでソフトウェアのまま）。`--cpu native` の x86-64 は clang へ `-mf16c`・`-mavx512fp16` も渡す。`@cpu` の版は、その版が `half` を使うときだけ target feature を足す。
  結果が NaN のときは、符号とペイロードをソフトウェア実装（`numeric.c`）と同じビットに置き換える。費用は C の `_Float16` 比 1.4〜4.5 倍で、NaN のビットを仕様から外せば消える見込みだが、
  WASM と native で NaN のビットが変わるので旧 D3 を残した。M1 Max の測定は、hardware が soft の約 330〜1,480 倍速く、C の `_Float16` の 1.4〜4.5 倍遅い（docs/benchmarks.md）。
  `tests/f16_hardware.mjs` は 2,707,485 件（f16 の単項は全 65,536 パターン）を JavaScript の厳密な参照と NaN のビットまで比べ、`TSUZURI_F16_EXPECT` でホストが通る経路を固定できる
  （`.github/workflows/targets.yml` が runner ごとに設定する）。G15 と統合したとき、経路を **target** の OS・arch から選ぶように `platform_f16_path(platform, target, cpu)` と
  `Platform::os_name` を足した（cross では host の CPU 機能を使わない）。確かめた範囲: 結果のビットは `targets.yml` の `f16 paths` が macOS arm64・Intel Mac・Linux x86-64・Linux arm64 の runner で
  参照と照合した（2,707,485 件。PR #22 の CI で、AArch64 は `hardware`、x86-64 は `generic=soft,native=conversions`）。限界: AVX512-FP16 の算術は標準の runner に CPU がなく、Windows は job がなく、
  どちらも機械語の確認のまま。速度の主張は macOS arm64 だけ。
- D11（`const def` と静的データ）: `const` は `def` の署名だけに付け、`fn`・`and`・`let` は継承する。評価器は Rust の再帰を使わない明示スタックの `Machine`
  （`src/const_machine.rs`・`const_rules.rs`・`const_static.rs`）で、D06 の予算（`remaining` 1<<20）と `MAX_NESTING`（128）を共有し、どの上限も上げていない。`Type` と `TypedExprKind` は
  `_ =>` を使わずに全 variant を振り分け、固定長配列 `[T; N]`・`string`・`utf8string` の走査を足した。`@checked`・`|>`・集成体への演算子・`@cpu`・`@literal` は拒否する。
  **Phase 2（D-28 の変更）**: 平たいデータ（整数・浮動小数点・decimal・`bool`・`char`・`unit`・それらだけのタプル／record／固定長配列・payload が同じ型の union・それらの `[T]`）の定数 `C` は、
  読み出しと借用（`ref C`、`C[i]`、`C.length`、`C.field`、`ref C[a..b]`、`for x in C`、`ref` 引数）でバイナリの読み取り専用の静的データ `@tz.static.N` を 1 つだけ指し、
  借用は返す・格納する・渡すことができる（所有権検査は静的な根を持つ）。静的データは書き換え・move・排他借用ができず（`E1012`・`E1013`・`E1014`）、drop・free・realloc は到達しない
  （借用から所有値を作る経路はすべて確保する）。値としての使い方は従来どおり複製で、`string` などは従来のまま。合計 4 MiB を超えると `E1026`。WASM ではデータがヒープの下に入り、
  ヒープの上限（絶対 16 MiB）を狭める。CRC-32 の表を 4 MiB に回す測定で、静的データは以前の 5.0 倍速く C の `static const` と同じ（docs/benchmarks.md。1 つの workload・M1 Max 1 台）。
  限界: 値として使う展開の予算（1,048,576 ノード）は従来どおり関数ごとに数える（大きな表を多くの関数で値として使うと `check` のメモリが膨らむので、静的データの読み出しと借用を使う）。
- G15（対応ターゲットとクロスコンパイル）: `--target native|wasm32|wasm64|TRIPLE`。native の triple は 6 個の計画から 22 個に広げた（`src/target.rs` の閉じた表。aarch64・x86-64 の apple-darwin、linux-gnu・musl、
  pc-windows-msvc・gnu（MinGW）、freebsd、android、riscv64 の linux-gnu・musl、iOS（端末とシミュレーター）、bare-metal の `*-unknown-none`・`riscv64gc-unknown-none-elf`）。32-bit の native は
  IR が 64-bit のポインターを前提にするので表に入れない。ビルド先の性質は `cfg!` でなく `Platform`（ビルド機の `--target native` は従来どおり）が答え、IR にも target に依る所が 3 か所あった
  （COFF の allocator、`@cpu` の level、`Math.fma`）ので、スレッドローカルの `PlatformScope` でそれらを target に従わせた（旧 D5「IR は target に依存しない」は偽だった。`EmitOptions` に field を足すと
  24 のテストが壊れるため）。リンクは同梱ツールチェーン（`scripts/toolchain/clang.rs` が Clang の target を Zig の target に写す）か、`TSUZURI_SYSROOT`（Apple は `-isysroot`。Apple の Clang は
  `--sysroot` を黙って無視するため）。ビルド機で動かせない target の実行は、利用者が与える実行器 `TSUZURI_RUNNER`（qemu-user・Wine・エミュレーターの前置コマンド）だけで、何も取得せず、
  ないときは `E2000`。`run --target` はこのときだけ使える。OS API は Android（Bionic は API 28 から）・iOS・Windows で、`Net` と `Async.block_on` は FreeBSD・Android・iOS で、`E2002` で拒否する。
  bare-metal は `--freestanding --allocator host` の object・llvm・header だけ（x86-64 は全関数に `noredzone`）。tier の記録は `_tsuzuri/language-reference/compiler/platforms.md` の表 1 か所で、
  `.github/workflows/targets.yml` の job が確かめるものだけを書く（tier 1 はホストの 4 triple、ほかは tier 2 以下）。CI の最初の実行の結果（PR #22）: 4 つの job が自分の triple を実行し、Linux の 2 つの job が
  静的な musl の 3 つを（直接と qemu-user で）、macOS arm64 の job が `x86_64-apple-darwin` を Rosetta で実行した。ほかの target は link か object まで（x86-64 の Mach-O・ELF・COFF の別の CPU、
  Windows・FreeBSD・Android・iOS・RISC-V。Wine と、Windows のホストでの名前付きの Windows target の実行は未確認）。`TSUZURI_RUNNER` は偽の実行器・qemu-user・Rosetta で確かめた。
- G19（edition・互換性・非推奨）: `Edition(u16)` は lexer と parser の入口の必須の引数で、manifest の `[package] edition`（省略は将来も `2026`）→ `SourceFile` → `SourceInput` → `ModuleInput` と運ばれ、
  manifest のない入力と std は `Edition::LATEST`。`EDITION_KEYWORDS` は空（差はまだない）。edition は字句と警告だけに作用し、IR は edition の値によらずバイト単位で同じ。frontend cache の parse key に edition を入れた。
  文書コメントの `@deprecated [note]`（最初の該当行だけが有効）を、別のパッケージ・std の `def`・`record`・`union`（と case）の使用の警告 `W1005`（既定で有効、同じパッケージの中では出さない）にした。
  チケットの `Construct` の target 追加ではなく `SemanticIndex::references` から出すので、構築式だけでなく pattern の case の参照にも出る（チケットの例は 3 件、実装は 5 件）。
  `tsuzuri api-diff <old> <new> [--json]` は 2 つの版の公開 API を型検査なしの構文解析だけで比べる（git・registry の依存を取得せず、`Tsuzuri.lock` も読まない）。型変数と region の改名は変更に数えず、
  breaking があれば終了コード 1。`tsuzuri publish` は `api-diff` を実行しない（index の項目を出すだけで、前の版を取得しないため。手順は文書に書いた）。`docs/stability.md` に保つものと非推奨の手順を書いた。
  限界: pattern の中の型注釈の使用と、型別名の解決・改名の検出は `api-diff`・W1005 の対象外。
- 統合後の独立した code review（2 回）で直したこと: G15 は、bare-metal の失敗時の案内が LLD と sysroot を求めていた件と、MSVC・bare-metal の `TSUZURI_SYSROOT` の拒否が freestanding の object のビルドで
  起きなかった件（`validate` の段で `E2000`）。G19 × D11 は、`api-diff` が `const def` を見ず `const` の取り消しを報告しなかった件（breaking にした）。D11 は、`f16`・`f128` の定数の NaN の符号が
  実行時（`tz_soft_op`）と違った件（実行時と同じ規則に揃えた。`f32`・`f64` は機械が決めるので NaN であることだけを決める）と、定数ごとに `const def` の本体を歩き直して 2 乗に遅くなった依存の走査
  （関数ごとに 1 度だけ求める）。値として使う展開の予算が関数ごとなので、大きな非 plain の定数を多数の関数が値として使うとメモリが関数の数に比例する点は、従来の設計のまま文書に書いた。
- `targets.yml` の最初の実行で見つかったこと（PR #22）。macOS の 2 つの job の iOS のリンクが、Xcode 16.4 の `/usr/bin/clang`（生成した IR の `icmp samesign` を読めない）で失敗した。
  `tests/targets.mjs` は、システムの Clang が IR を読めるときだけそれを iOS に使い、読めなければ与えられた Clang（CI は Homebrew の LLVM 21）でリンクする。README などの「LLVM 17+」は、この IR には
  足りない（既存の記述で、この変更では触れていない）。Intel Mac の job は約 80 分（Rust のテストだけで 62 分、Linux は 21〜40 分、macOS arm64 は 25〜28 分）かかったので、制限時間を 150 分にし、
  Rust のテストを最後の step にして、ほかの step の結果が長い step に隠れないようにした。`pull_request` の `paths` は、最後の push でなく PR 全体の差分で判定するので、文書だけの push も `targets.yml` を
  やり直す。
- 既知の事前の不具合（このブランチの変更ではない）: debug build の深さの上限のテストは、テストスレッドの 2 MiB（2,097,152 bytes）の stack にほとんど余裕がない。arm64 macOS（rustc 1.98.1）で必要な stack は
  `parser::tests::bounds_recursive_and_flat_expression_depth` が 2,081,456 bytes、`computations::bounds_nested_builder_expansion_not_just_source_syntax` が 2,032,274、
  `computations::bounds_computation_syntax_and_expansion` が 2,130,638 で、最後のテストは溢れて binary ごと abort する。基準（`053bced8`）と 4 チケットの変更後で、3 つとも 1 KiB の刻みで同じ値だった。
  linux-arm64 の runner（rustc 1.99.0）では、基準の `main` でも 1 つ目と 3 つ目が既定の stack で溢れ、2.2 MB で通る（linux-x64 では 5 つとも既定で通る）。
  全体は `RUST_MIN_STACK=4194304` を付けて流す（これまでの PR と同じ値。`targets.yml` もこれ）。§3.1 の 4 つの単独の実行は既定の stack のままで、linux-arm64 では 1 つ目が通らない。
  上限や stack の大きさのコードは変えていない。フレームを小さくするか、テストが stack を決める修正は別の変更にする。

### D-46 追加チケット（D12・B09）の確定

- 2026-10-12、利用者の「D12, B09 の実装をすべて完遂して。もし現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して。複数フェーズある場合には、すべてのフェーズを完了させること。…
  何か判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」を、B09 の D9（Phase 2 の相互再帰・解放処理を持つ値の捕捉・ほかの `let rec` 関数の参照）の承認として扱った。
  着手時点は `aecdf75e`（PR #22 のマージ）。詳細は各チケットの「実装と検証」（`_features/_completed/`）にある。
- 共通: 新しい予約語（`rec`・`and` は既存）・診断コード・crate・`unsafe`・CLI・環境変数・std モジュール・既定の WASM の import はない。`FRONTEND_FORMAT` は B09（`Binding.recursion`）で 3 から 4 になった。
  `let rec` を書かないプログラムの IR は、D12 を実装した基準のコンパイラとバイト単位で比べて同じだった（`examples/` と `tests/fixtures/` の 105 ディレクトリ × native・wasm32 × `-O0`・`-O3` の 420 構成のうち
  366 が一致、50 は両者が同じ診断で失敗、差は 0。残りの 4 は新しい fixture を基準が読めない構成）。D12 自身の IR の差は、重複した `declare` を落とすことだけである。
- D12（intrinsic 宣言の重複）: 連結後のランタイム IR の `declare` を、シンボル名ごとに最初の 1 つだけ残す（`llvm::unique_declarations`、`emit_program` の最後。重複が無ければ入力をそのまま返す）。
  テストの一意性の検査は行全体の比較をやめ、`llvm::duplicate_declarations`（JavaScript は `@` から `(` までの名前）で行う。driver の `debug_assert!` は、後段の変換が足す重複を debug build で検出する。
- B09（ローカルな `let rec`）: `Binding.recursion`（`None`・`Rec`・`And`）、`TypedExprKind::Lambda.recursive`（group の members）。型検査は group の名前を先に束縛する（`Checker::let_rec_group`）。リフトは `src/closures_rec.rs` で、
  group の捕捉を各メンバーの捕捉と、本体が参照する前の・外側の `let rec` 関数のうち直接呼べるものの捕捉の和にし、リフト先の id を本体を下ろす前に予約して、そのメンバーの本体が直接呼ぶ名前の使用を直接呼び出しに書き換える。
  直接呼べるかは本体ごとの `Frame` で決める（リフトしたメンバーの本体と通常のラムダ式の本体が 1 つずつ）。捕捉は、グループを束縛した本体のローカルと、グループのメンバーの引数としてしか存在しないので、
  前の関数の捕捉を引き継げるのは、その関数が `Frame.direct` にあるか、同じフレームで束縛されていて値渡しの捕捉が代入・可変借用されていない（`State.mutated`）ときだけである。それ以外は関数値として捕捉し、閉包を通して呼ぶ。
  **D4 の置き換え**: 値で渡せない捕捉（`!(is_copy && !needs_drop)`）は、特殊化の後の具象型で分類して共有借用にする（閉包は `Borrow(Local)`、リフトした関数は `ref T`）。
  借用した捕捉の移動は `E1012`、借用を持つ閉包を持ち主から返すのは `E1013`、型変数を含む型の捕捉は `E1005`（チケットの対象外のまま）。通常のラムダが捕捉できない値（`Drop`・`Rc`）は従来どおり `E1005`。
  末尾呼び出しは、借用した捕捉をそのまま渡し直すときもループにする（`llvm::FunctionEmitter::is_self` が引数を見る。`capture_count` が 0 の関数、つまり `def rec` は従来どおり借用を持つ引数でループにならない）。
  相互再帰の末尾呼び出しと関数値を介した再帰はループにならない。
- **教訓（debug build の stack）**: 再帰する関数のフレームは、debug build では全 arm の局所変数と一時値の和になり、`?` 1 つにつき `Diagnostic` を持つ `Result` の一時値が 3 つ付く。B09 の最初の版は
  `composed_expression`・`lower_expression`・`eval_lambda`・`collect_locals` と parser の再帰する関数へ分岐を足したので、§3.1 の `bounds_nested_builder_expansion_not_just_source_syntax` の必要 stack が
  2,033,203 から約 2,098,000 bytes になり、既定の 2 MiB（2,097,152）で溢れた。新しい処理は専用の関数（`block_bindings`・`implicit_prefix`・`capture_loans`・`collect_block`・`closures_rec::lower` など）へ出し、
  再帰する関数からは `return helper(..)` でそのまま返し、診断は `rec_error<T>` のような関数が作る。結果は 1,802,734 bytes（Block の束縛ループを出した `composed_expression` が 6,368 から 3,680 bytes になったため、
  基準より小さい）。レビューの修正（ラムダ式の本体ごとの `lower_body` が再帰の経路に 1 つ増える）のあとは、1,816,000 bytes で溢れ、1,824,000 bytes で通る（それでも基準より小さい）。測り方は `RUST_MIN_STACK` の二分探索で、macOS arm64 は 16 KiB 刻みになる。`bounds_computation_syntax_and_expansion` は基準と同じく既定の stack で溢れる（最終は 2,130,000 bytes で溢れ、2,150,000 bytes で通る。基準は 2,130,638 bytes）。
  再帰する関数へ分岐を足したあとは、§3.1 の 4 つのテストを既定の stack で必ず実行する。
- **教訓（レビューで見つかった B09 の誤り）**: PR のレビュー（独立したコードレビューと Copilot）が 9 件を見つけ、テストの不足も 1 件挙げた。うち 3 件は、リフトした関数が「捕捉を引数として持つ」前提で、前の・外側の `let rec` 関数を
  どこからでも直接呼んでいたことによる（入れ子のグループが外側の本体から呼ばれる、ラムダ式・継続の中のグループが前の関数の捕捉を持つ、代入された値渡しの捕捉を引き継ぐ）。誤りは値の違いか、emitter の panic として出た。
  残りは `match` の対象を常にムーブとして扱っていたこと、E1012 の位置が捕捉した値の宣言を指していたこと、`let rec` の関数型の捕捉が `E1005` になっていたこと、REPL が `and` のメンバーの 2 つ目以降の名前を登録しなかったこと、
  コンピュテーション式の `@checked` を付けた文が、前の文を見ずに `and` をグループのメンバーとして読んでいたこと、暗黙の task の本体が `let rec ... and ...` のグループを 1 文ずつに分けていたことである。
  構文の新しい 1 つの形（ここではグループ）は、文を 1 つずつ展開する経路（`lower_implicit_task_sequence`）と、状態を引数で渡す経路（`after_rec`）を、すべて洗い出してから入れる。
  手書きのテストは書いた本人の想定に沿うので、同じ盲点を共有する。`tests/local_recursion_fuzz.mjs`（乱数で作った入れ子のグループ・クロージャ・代入される捕捉・借用した文字列と配列のプログラムを、
  仕様から書いた独立した JS の評価器と照合する差分テスト）は、修正前のコンパイラで 150 件中 8 件の差を見つけ、最終のコンパイラは 1,399 件（深さ 5 の 400 件と、深さ 6 の 999 件。1 件は大きさで除外）がすべて一致した。リフトや捕捉の分類を変えたときは、これを回す。
  **共有の解析を直すと、機能を使わないプログラムの IR が変わる**: `match` の誤りを `TypedExpr::consuming_use` の中で直したところ、`let rec` を書かない `tests/fixtures/maybe_result` の IR が 4 構成すべてで変わった
  （`call_specialization::can_borrow` が同じ関数を使い、環境を複製せずに読める閉包が増えた）。テストはすべて通っていたので、気づいたのは基準との IR の比較だけである。直しは `let rec` 専用の `moving_use` に分けた。
  修正のあとも、手順 9 の IR の比較（基準との byte 一致）をやり直す。

### D-47 追加チケット（A17・A18）の起票

- 2026-10-13、利用者の相談（GC や `new`/`delete` は性能とリークのリスクで受け入れられない。所有権モデルで `ref`・`ref mut` が必要になることを、よりよいメモリ管理で減らせないか）への検討と、
  「『次の一手』を進めて、`_features` に開発チケットを作成して」の依頼から起票した。結論は、所有権（アフィン型）を保ち、(1) Copy のコレクションの暗黙の複製を除き（A17）、(2) 読むだけの引数の既定を読み取りにする（A18）。
  GC・既定の参照カウント・世代つき参照・リージョン推論は採らない（理由と出典は A18 の付録）。
- 着手時点は `2282fd96`。コンパイラ本体は変えていない。足したのは、棚卸しの道具 `tests/copy_inventory.rs`（`#[ignore]`。長さに比例するコストの位置を、利用者 120・std 138 以上と数える）と、
  A17 の意味の基準 `tests/fixtures/last_use` ＋ `tests/features.mjs` の suite `last_use`（HEAD で 102 件通る）だけである。試作は捨てた。試作の数字と、移動してはいけない形（segfault 2 件・黙って結果が変わる 1 件）は A17 の「現状と計測」にある。
- A17（最後の使用の移動。PM07 の Phase 2 を置き換える）: 言語の意味・構文・診断を変えないので承認は不要。Phase 2（`match` の束縛の移動）は人間が求めたときだけ着手する。新しい予約語・診断コード・CLI・環境変数はない。
  **D-19 の例外**: 解析の予算（訪問 200,000 回）を超えたら `E1017` にせず「移動なし」へ戻す（解析は最適化で、結果が常に正しいため、コンパイルを止めない）。
  B09（D-46）の教訓（手書きのテストは書いた本人の盲点を共有する）に従い、差分テスト `tests/last_use_fuzz.mjs` を実装手順 1 で作る。
- A18（引数規約。`own T` と edition `2027`）: **2026-10-23 に D1〜D4 と全 Phase の実装を承認**。`own` は文脈キーワード（予約語にしない）。Phase 1 では既定版を保ち、Phase 2 の checked migration と std・fixture・文書の移行後に `LATEST` と std を `2027` にする。`MANIFEST_DEFAULT` は `2026` に固定する。
  **2026-10-24 に Phase 1〜4 の実装・受入検証を完了し、[チケット](_completed/A18-parameter-conventions.md)をアーカイブした。**
  `docs/stability.md` の「edition は型検査に影響しない」を改める必要がある（承認の対象に含む）。診断は強めるだけ（`E1003`・`E1012` の注記）で、新しいコードは作らない。
  起票後の修正（D-48 の検討中に見つけた）: 補完が `const def`・エントリーの `main`・`extern` の署名の検査（`E1026`・`E2004`・`E1008`）に当たるので、この 3 つは補完しない（A18 の D10。HEAD で確認）。`extern` は `function_declarations` に積まれて同じループを通る。さらに、`own` の符号化（`src/syntax_codec.rs` の tag 15 と `FRONTEND_FORMAT`）が手順 3 から漏れていたので足した。A18 が `own` の共有構文を実装し、A19 は純粋性に必要な位置と型の制約を追加する。

### D-48 追加チケット（A19）の起票

- 2026-10-13、利用者の相談「`def pure` のような純粋関数を保証する構文を前提にすると、A17・A18 の方針は変わるか」への検討と、指示「`pure` は純粋関数を維持するために `ref` のみを受け取るようにすること。なお、定義するときは `ref` を省略でき、引数はすべて `ref` 扱いにすること」、続く「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」から起票した。着手時点は `cef6c67f`。コンパイラ本体は変えていない。
- 検討の結論: A17 は変わらない（判定は「もう読まない」と「別名がない」で、副作用に依らない。`ref mut` を禁じる `pure` は、A17 によるループの更新の高速化を前提にする）。A18 の中身も変わらず、`pure def` の引数は A19 の規則で補完し、A18 の補完の対象にしない（A18 の D11、A19 の D13）。
- A19（`pure def`）: D1・D2・D4 は人間の指示で承認済み。D4 は構文 `pure def`（2026-10-13「構文を `pure def` にしてよいです」。予約語にしない文脈キーワードで、`CONTEXTUAL_KEYWORDS` に登録する。既存の修飾子がすべて `def` の前にあることに揃えた。代案は人間の例示の `def pure`）。D1 は、引数が共有借用（無印か `ref`。省略でき既定）か `own` で、`ref mut` と `mut` は不可。`own` の許可は「保証されるのであれば」の条件つきで、保証は A19 の D15（`own` の型は平たいデータ）と付録 E（HEAD の実測）で確かめた。
- `own` の保証（A19 の「`own` と純粋性」・D15）: HEAD で、(1) 排他借用（`ref mut` の field）を持つ record を値渡しで受けた関数は、呼び出し側の変数を書き換えられる（`ref` で受けると `E1014`）、(2) `Rc` を値渡しで消費して捨てると呼び出し側の `Rc.strong_count` が 2 から 1 に変わる、(3) 利用者の `Drop` は呼び出し先の中で走る。破棄と書き換えが観測できるので、`own` の型は、許す型を列挙した「平たいデータ」（数値・文字・文字列・配列・リスト・`Vec`・固定長配列・tuple・利用者の `Drop` を持たない record／union）に限る。参照・`Rc`・`Arc`・利用者の `Drop` を持つ型・`extern` のハンドル・`Task`・`dyn`・関数は `E1029`。Copy の配列を `own` で受けて書き換えても呼び出し側の値は変わらず、消費した非 Copy の値は読めない（`E1012`）。起票時の `own` の前置き（`TypeExprKind::Own`）は `pure def` の引数だけの案だった（A19 の D16）。通常の `def` へ広げる A18 の D2 は 2026-10-23 に承認され、共有構文は A18 が実装済み。A19 は純粋性の制約を追加する。共有の符号化は `src/syntax_codec.rs` と `FRONTEND_FORMAT` に記録する。
- 割り当て: 診断コード **`E1029`**（`pure def` の署名と本体の違反。`E1026` と同じ作りで、メッセージで区別する）。`FRONTEND_FORMAT` は 4 から 6（手順 2 の `pure`、手順 3 の `own`）。新しい予約語・CLI・環境変数・std モジュール・既定の WASM の import・crate・`unsafe` はない。
  解析は `src/pure_rules.rs`（`closures::lower` の後、`gpu::validate_calls` の隣。到達可能性の幅優先。上限は `E1017`。到達する本体で作られた関数値だけが現れるので、関数値の呼び出しは許し、外から関数値が入る引数を禁じる）。`Builtin` の 246 個を 90（不純）と 156（純粋）に分け、wildcard なしの `match` で保つ。
- 型の上の表し方（A19 の D3）: Copy で drop が要らない型は値のまま、ほかの無印は `ref T`、`own T` は `T`。HEAD で `ref i64` が算術にも戻り値にも使えず `E1003` になるため。
- 蓄積引数の再帰（A19 の D14）: `own` で書ける。HEAD では二乗（n = 40,000 で 15〜20 秒）で、A17 の後に線形になる見込み（未測定）。

- **2026-10-23、全 Phase の実装とローカル受入を完了。** A18 の通常 `def` の `own` を維持し、generic、純粋関数型と呼び出し段階、class／instance・CPU variant、遅延 IO の構築、閉じた明示並行処理、Char・PCG の契約、`api-diff --effective`、GPU との効果問い合わせの共有に対応した。旧 Phase 1 の制約と format 4→6 の記録を現行仕様とはしない。現行 frontend format は **9**。
  最適化は独立に `memory(none)` を証明した整数呼び出しの同一基本ブロック内の再利用だけで、最初の呼び出しを残す。自動 offload・所有結果の共有・永続 memo cache は導入せず、速度向上は未計測。
  A19 単独で Rust **1,816 passed / 2 skipped**＋targets **12**、features **20,012 cases**（native／WASM × O0／O3）、旧 **114** プログラムの IR byte 一致、付録 E **36** 検査、リファレンス **12 ページ／75 例／134 native runs**が成功。全 Phase の記録は [A19](_completed/A19-pure-functions.md)、最終 head の CI・Copilot レビューは PR で追跡する。

### D-49 G10（Windows ネイティブ対応）の確定

- 2026-10-13、利用者の「G10 の実装をすべて完遂して。もし現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して。複数フェーズある場合には、すべてのフェーズを完了させること。…
  何か判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します。すべての実装・検証が完了したら、コミット・プッシュをしたあとに、GitHub 上でプルリクを作成して。Copilot のレビューをさせたあと、何か指摘事項があれば、それにも対応をすること」を、
  G10 の D3（push と workflow の実行は人間が行う）の例外承認として扱った。着手時点は `2282fd9`（PR #23 のマージ）。旧 HEAD `f8dc655` の計画は、文書の移動（`_docs/` → `_tsuzuri/language-reference/`）、
  `targets.yml`（G15）と `RUST_MIN_STACK=4194304`（D-45）、`tests/windows.rs` が 4 件になったこと、action の版（`checkout@v7`・`setup-node@v7`）、G16 の PDB（対象外の一覧から外れた）、
  E08・E09・E13・E14・E11・E10 が「G10 の後」とした Windows の作業と照合して見直した。詳細は [G10](_completed/G10-windows.md) の「旧計画から外れた判断」にある。
- 共通: 新しい予約語・診断コード・crate・`unsafe`・CLI・環境変数・std モジュール・既定の WASM の import はない。IR と ABI は変えていない。コードの変更は `src/runtime/task-windows.h`（マクロの guard）と
  `src/fetch.rs` の `cfg!(windows)` の分岐と、`\\?\` で始まる path にだけ効く `cache::real_path`（共有の `\\?\UNC\...` も外す）だけで、Unix と WASM の挙動は変わらない。
- 実行ゲート: `.github/workflows/windows.yml`（workflow `Windows MSVC`、job `windows`、`windows-2022`）を復元した。W1〜W8 が緑になったのは、LLVM 21.1.8、Visual Studio 2022 の MSVC 14.44、Windows SDK 10.0.26100、
  rustc 1.99.0、Node.js 24.21.0 の組。MSVC の toolset と SDK は `ilammy/msvc-dev-cmd` の `toolset`・`sdk` で指定し、`Show tools` が版と `link.exe` の場所を検査する（image の更新でこの組が変わると job が失敗するので、文書の版はすべての run の性質になる）。W3（`cargo test --locked --no-fail-fast`）は `RUST_MIN_STACK=4194304` を付けて 40〜50 分かかるので最後に置く。step は前の失敗に左右されず走るが、W5〜W8 と `numeric_casts.mjs` は W4 の `tsuzuri.exe` を使うので、W4 が失敗したときは飛ばす。LLVM の installer は `scripts/toolchain/bundle.mjs` の `download()`（SHA-256 の検証・30 秒無応答の skip・3 回の pass）で取得し、`actions/cache` の保存を 2 つ目の source にした。
  最初の run は W5 が 8 秒で止まり、W3 が 17 件失敗した。runner の log は run が終わるまで読めず、W3 を含む run は 40 分以上かかるので、速い step だけの throwaway branch（約 10 分）で直して戻した（AGENTS.md の CI の項）。
- 見つけた不具合（8 種）: 製品の側は 2 件。`git` へ verbatim パス（`\\?\`）を渡していたため、Windows の git／registry の取得がすべて `E2007` だった（E10 が「Windows での git は未検証」と記していたもの。`src/fetch.rs` の `cfg!(windows)` の分岐で直した）。
  `task-windows.h` は、`-D_CRT_SECURE_NO_WARNINGS` と `-Werror` の組で再定義になった（未定義のときだけ定義する形にした）。残りは harness と Rust のテストの POSIX の前提で、Unix の期待値は変えずに Windows の側を足した
  （`-lm`、compiler-rt のない 128-bit の除算、トラップが signal でなく 0xC000001D で終わること、パスの区切り、CRLF、stderr の先頭の警告の JSON 行）。
- tier: windows.yml の job が、名前を付けた `x86_64-pc-windows-msvc` を `-O0`・`-O3` で `build`・`run`・`test` する検査を足し（`tests/windows.mjs`）、`platforms.md` の tier を 2 から 1 にした。tier 1 は 5 つになった。
  `aarch64-pc-windows-msvc` と MinGW は Windows のホストで実行していないので 2 のまま。
- 後続の作業は G10 のチケットの末尾に一覧した（Windows の OS API、DLL、トラップの報告の改善、LLP64 の bindgen、COFF の object のランタイム、ARM64 の実行）。診断のメッセージにある `(G10)` は、その作業を指す印として残し、
  メッセージと既存のテストは変えていない。

### D-50 第3期計画（AI・堅牢性・性能。27 チケット）の起票

- 2026-10-18、「AI と人間が、少ない暗黙ルールで堅牢かつ高パフォーマンスなプログラムを書ける」という言語の目標に対する、現状の不足と妨げの洗い出し（調査時点 HEAD `a2313d19`）と、
  指示「すべての改善案の設計を詰めて。複数フェーズある場合にはすべてのフェーズを完了させること。判断が必要なものは最高の選択肢を採択してよい。設計は `_features` にチケットとして出力し、
  あとから小さいモデルが実装しても大きいモデルと遜色ない結果になる内容にすること」から起票した。コンパイラ本体は変えていない（チケットと台帳だけ）。
- 調査の結論（根拠は各チケットの「現状」）:
  (1) コンパイラは移動・借用・コピー・確保・トラップ・整数の範囲の事実を内部で持つが、診断にもツールにも出さない（診断は主スパン 1 個で、移動元も借用元も示さない。KB・MCP・図・`explain`・`fix` がない）。
  (2) 署名の契約として固定できない（効果の型付けは `IO`・`Async`・`Task` だけ。`pure def` は A19 で計画中。確保・トラップ・複製・停止性を表す手段がない）。
  (3) 同じ意味に複数の書き方があり、同じ記号の意味が文脈で違う（`..` は `for` で両端を含みスライスで半開。接尾辞は `l`=i64・`L`=i128 と大小で幅が変わる。`&mut r` と `ref mut r` は別の意味）。
  (4) 文書が 1,000 万字を超え、規則 ID も規則とテストの対応もなく、食い違いを機械で検出できない（README の `Map` の説明は平衡木だが実装はソート済み `Vec`、`_specs` の `try` の説明は現行の仕様と違う）。
- 骨子は 3 つの動詞: **引く**（Knowledge: G21・G26〜G32）、**縛る**（Contracts: A20・A21・A23・E15・F14）、**直す**（Diagnostics: G22〜G25）。
  加えて、edition 2027 の正準形（A22）、性能の構造的な保証（C12・D13・D14・F14・F15）、仕様の単一情報源（G33〜G36）、失敗の可観測性（B10）。
- 承認: 人間の包括許可（2026-10-18）で、各チケットの決定事項（D1…）はすべて承認済みとして記録した。
  **A18 の D1〜D4（無印の引数を読み取りにする。言語の意味の変更）は、この承認に含めない**（A18 は独立に判断する。A22 は A18 に依存せず、edition `2027` の足場だけを共有する。D-54）。
- 守る方針: 「採用しない改善」（GC、例外の巻き戻しと `?`、`unsafe`、境界検査の無効化、暗黙の fast-math）は変えない。AGENTS.md の規則（暗黙の変換・既定値を入れない、native と WASM の `-O0`／`-O3` を保つ、
  共有 CI に速度の合否の閾値を置かない）に従う。新しい crate・`unsafe`・既定の WASM の import・予約語は足さない（使ってよい crate は既存の `serde_json` と標準ライブラリだけ）。
- 実装の単位: 全 Phase の設計を各チケットに書いた。各 Phase は独立に実装・検証・マージできる。**着手する Phase は人間が指示する（指示がなければ Phase 1 だけ）。**
  小さいモデルは [GUIDE-LITE.md](GUIDE-LITE.md) とそのチケットだけを読めば着手できる（GUIDE のほかの節は、チケットが指定したときだけ読む）。
- 生成物の置き場: チケットは `_features/<ID>-<slug>.md`（`todo` の間。完了したら `_completed/`）。README の一覧は [追加チケット（2026-10-18）](README.md#追加チケット2026-10-18) にある。

| ID | 題名 | 優先度 | 規模 | 依存 |
| --- | --- | --- | --- | --- |
| G32 | 開発リポジトリの KB と `ticket-brief`（小さいモデルへ渡す文脈の自動生成） | P0 | M | なし |
| G22 | 診断 v2 の基盤（関連位置・help・fix・expected/found。所有権の移動元・借用元） | P0 | L | G20（done） |
| G21 | AI ガイドパック（`tsuzuri guide`、`tsuzuri new` の `AGENTS.md`・`llms.txt`） | P1 | L | なし（Phase 4 は G23 のヒント DB があれば使う） |
| G23 | 診断の kind・`tsuzuri explain`・ヒント DB | P1 | L | G22 |
| G24 | `tsuzuri fix`（機械的な修正の適用と edition の移行基盤） | P1 | L | G22 Phase 1 |
| G26 | Facts モデル（コンパイラの事実の機械可読な定義と exporter） | P1 | XL | Phase 1〜4・6 は なし（A15・PM01 は done）。Phase 5 だけ G22。Phase 3 の `effects` を完全にするのは A20（なくても縮退版で出荷できる） |
| G27 | ビルド時の KB（`target/tz/kb`、brief カード、増分、鮮度） | P1 | L | G26 Phase 1（graph・facts・layout・tests の書き手は G26 の後続の Phase があれば使う） |
| G28 | `tsuzuri kb` の照会と `impact`（facts の差分） | P1 | L | G27 Phase 2、G26 |
| G31 | AI 作業コスト指標（入力量と往復数を決定的に測る） | P1 | M（L 寄り） | なし（G22・G24・G27 があれば、その方式も測る） |
| A20 | 効果の要約と検査付き関数属性（`@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`） | P1 | XL | A19（未実装なら経路 B で本チケットが `pure` の基盤を先に作る）、G22（弱い） |
| A22 | edition 2027 の正準形（`..<`／`..=`・接尾辞・借用の表記・旧ラムダ）と移行 | P1 | L | なし（Phase 3 の fix の付与は G22 Phase 1、統合試験は G24 Phase 3 が要る） |
| C12 | [`Map`／`Set` の更新を対数時間に近づける（チャンク化したソート配列）](_completed/C12-ordered-map-set-chunked.md)。Phase 1・2 完了 | P1 | M | なし |
| D13 | 補間の穴の一時値（`{s.length}` の `E1013` の解消） | P1 | S | なし |
| [F14](_completed/F14-structural-performance-assertions.md) | 構造的な性能表明（確保数のテスト用 API と `bench` の確保数）。Phase 1・2・3 完了 | P1 | L | F13（done）、G18（done） |
| G25 | 型つき穴 `?` と `tsuzuri find`（型でのシグネチャ検索）、未解決名の候補 | P2 | L | G22 |
| G29 | `tsuzuri mcp`（MCP サーバー） | P2 | L | Phase 1 は なし。Phase 2 以降のツールごとに G21・G23・G26〜G28（あるものだけ公開する） |
| G30 | 図表の生成（Mermaid。モジュール・型・呼び出し・所有権・並行・レイアウト・状態遷移） | P2 | L | G26、G27 |
| G33 | 仕様の規則 ID と適合性スイート | P2 | L | なし |
| G34 | doc-test（`///` の例を実行する）と std の例 | P2 | M | なし |
| G35 | 生成物による文書の単一情報源化（診断コード表・std 索引・benchmarks の JSON 化） | P2 | L | なし（G23 があれば診断コード表の検査が `explain --catalog` を使う） |
| G36 | 言語全体の差分ファズ（プログラム生成型。コンパイル時評価を参照実装に） | P2 | L | D11（done） |
| A21 | 契約 `@requires`／`@ensures`（off／check／test／prove） | P2 | XL | A19 Phase 1（done が前提）。A20・G22 は任意（F14 は使わない） |
| A23 | 整数の折り返しの意図（`Int.wrapping_*`・オーバーフロー報告・lint） | P2 | L | PR02（done）。G22 は弱い |
| B10 | トラップのバックトレースとクラッシュレポート、純粋関数の失敗の閉じ込め（`Domain`） | P2 | XL | E14（done）。Phase 2 だけ A19 Phase 1 |
| E15 | 信頼境界と権限（`@trusted`・`tsuzuri audit`・`[capabilities]`） | P2 | L | Phase 1〜3 は なし。Phase 4 だけ A20（G26 は使わない） |
| D14 | `Text` クラス（`String`／`Utf8String` の API の統一） | P2 | L | なし |
| F15 | 逐次と同一の結果を保証するデータ並列（`Monoid` の還元・純粋性・提案だけの自動化） | P3 | L | Phase 1・2 は なし。Phase 3 は A19 Phase 1。Phase 4 は G26 Phase 1 |

実装の推奨順（依存を満たす範囲で並行してよい）: ① G32・D13・C12・F14・G31（基準値）→ ② G22 → G23・G24・G21 → ③ G26 → G27 → G28 → G30・G29 → ④ A19（既存）→ A20 → A21・E15、A22、A23 → ⑤ G25・G33〜G36・D14・B10・F15。

### D-51 診断 v2 の共通仕様（G22・G23・G24・G25 と、診断を出すすべてのチケット）

- **互換性**: [docs/stability.md](../docs/stability.md) の「診断のコード・重要度・JSON のキー」に従う。`--json` の既存の 5 キー（`severity`・`code`・`message`・`path`・`span`）は消さず・改名せず、
  キーを**足すだけ**にする。足したキーは、値があるときだけ出す。**足すキーを一つも持たない診断の JSON は 1 byte も変わらない**（既存のテストと文書の例が壊れない）。
  メッセージの文言は保証の対象外なので直してよいが、コードと位置の意味は保つ。
- **Rust の型**（`src/diagnostic.rs`。名前と型を変えない）:
  `Diagnostic { severity, code: &'static str, message: String, span: Span, kind: Option<&'static str>, related: Vec<Related>, help: Option<String>, fixes: Vec<Fix>, detail: Detail }`、
  `Related { role: RelatedRole, message: String, span: Span }`、`Fix { title: String, applicability: Applicability, edits: Vec<Edit> }`、`Edit { span: Span, text: String }`、
  `Detail { expected: Option<String>, found: Option<String>, candidates: Vec<Candidate>, scope: Vec<ScopeEntry> }`（`Default`）、`Candidate { name: String, signature: String, how: String }`、`ScopeEntry { name: String, ty: String }`。
  `Applicability` は `Certain`・`Likely`・`Maybe`。`RelatedRole` の語彙は閉じている: `MovedHere`・`BorrowedHere`・`BorrowUsedHere`・`DeclaredHere`・`ExpectedBecause`・`FirstDefinedHere`・`ViaCall`・`RequiredBy`・`Note`。
  構築は `Diagnostic::new`／`warning`（既存。拡張フィールドは空）に、ビルダー `.with_kind(&'static str)`・`.related(RelatedRole, impl Into<String>, Span)`・`.with_help(impl Into<String>)`・`.fix(title, Applicability, Vec<Edit>)`・`.expected_found(expected, found)`・`.with_candidates(Vec<Candidate>)`・`.with_scope(Vec<ScopeEntry>)` をつなぐ形。
  重複除去のキーは変えない（source・start・end・severity・code・message）。`related`・`fixes`・`detail` は同じキーの診断の間で先に入った方が残る。
- **G22 での具体化**（G22 の D1・D2・D4・D14。台帳の型は読み手の見え方で、置き場所だけ変わる）: `Diagnostic` は `pub struct Diagnostic(Box<DiagnosticData>)`（8 bytes。264 bytes にすると GUIDE §3.1 の stack の回帰テストがすべて溢れたため）で、
  上のフィールドはすべて `DiagnosticData` にあり、`Deref`・`DerefMut` で `d.code`・`d.span = ..` のように読み書きする。フィールドを move するときは `into_data()`、`Diagnostic { span, ..e }` の更新構文は使えない。
  別ファイルの `related` と edit は `SourceLookup<'a> = dyn Fn(usize) -> Option<(&'a str, &'a str)> + 'a`（source の番号 → (path, text)）で引き、解決できない `related` と、edit が 1 つでも解決できない fix は出さない。
  `apply_edits(text: &str, edits: &[&Edit]) -> Option<String>`（`src/diagnostic.rs`）は LSP と G24 が共有する（`(start, end)` の安定整列で、範囲外・文字境界でない・重なりなら `None`）。
  JSON の `role` の値は variant 名の `snake_case`（`moved_here`・`borrowed_here`・`borrow_used_here`・`declared_here`・`expected_because`・`first_defined_here`・`via_call`・`required_by`・`note`。キーも値も安定として扱う）。
  fix の `title` は先頭大文字の命令形の英語（`Add the arm '..'`。LSP の既存の `Prefix 'spare' with '_'` と同じ）。メッセージと `help` は小文字始まり。
- **JSON のキーの順序**（値があるものだけ）: 既存の 5 キー → `kind` → `related` → `help` → `expected` → `found` → `candidates` → `scope` → `fixes` → `trap`（`E2005` だけ。B10）。
  `related` の要素は `{"role","message","path","span"}`（`span` は主スパンと同じ 6 キーの形）。`candidates` の要素は `{"name","signature","how"}`（`how` は `call`・`value`・`type`・`module`）。`scope` の要素は `{"name","type"}`。
  `fixes` の要素は `{"title","applicability","edits":[{"path","span","text"}]}`。`applicability` は `certain`・`likely`・`maybe`。`edits` の `span` は `start`・`end` の UTF-8 バイトオフセット（`end` は排他）を必ず含め、行・列も付ける。
- **applicability**: `certain` は意味を保つ機械的な書き換えで、`tsuzuri fix` が既定で適用する。`likely` はほぼ正しいが人が確かめるもので、`--applicability likely` のときだけ適用する。`maybe` は提案で、自動では適用しない。
- **kind**: 診断の原因を一意に表す安定した識別子（ASCII の `snake_case`、48 文字以内、コンパイラ全体で一意、別の意味に再利用しない）。`code` はそのまま（粗い分類）で、1 つの kind は 1 つの `code` に属す。
  kind の登録簿と移行は G23。G23 が移行するまで `kind` を持たない診断がある（その間は `kind` キーを出さない）。
  G23 のあとは、kind は enum `DiagnosticKind`（登録簿。HEAD では 47 コード・653 kind。マクロで定義）で、診断は `Diagnostic::of(DiagnosticKind, ..)` でしか作れず（旧 `new`／`warning` は削除）、`kind` は**常に `Some`**（型は D-51 のまま `Option<&'static str>`。G23 の D1・D5・D7）。
  **新しい診断の kind は、その診断を足すチケット自身が付ける**（G23 が後のチケットの kind を代わりに足すことはない）。木の状態で 3 通り: ① G23 がある（`Diagnostic::of` がある）: 出す前に、自分の kind を G23 の S1「新しい kind を足す手順」で登録簿へ足し、`of` で出す。② G22 Phase 1 だけある（`with_kind` がある）: `with_kind("…")` で直接付ける。G23 があとで登録簿へ移す（名前は変えない）。③ どちらもない: kind を付けない（コード・位置・メッセージは同じにし、あとで G22／G23 が付ける）。kind の文字列はチケットの表のとおりで、どの場合も同じ。
  `E2005` だけが持つ JSON の `trap`（B10）は、`fixes` の**後ろ**の最後のキーで、型は `Option<Box<TrapReport>>`。G22 のあとは `DiagnosticData.trap`（`Detail` に入れない）にし、`render_with`・`json_with` が出す（人間向けは引用の直後、`related` の前に `trap.lines()`）。B10 が G22 より先に入るなら `Diagnostic` に直接足し（`trap: None` のリテラルは `docgen.rs`・`repl.rs`）、G22 の実装者が `DiagnosticData` へ移して `render_with`・`json_with` に通す。`trap` が `None` の出力は 1 byte も変わらない。
- **メッセージ**: 英語、小文字始まり、末尾にピリオドなし、修正方法を含める（§6.7）。`help` は命令形の 1 文（例: `borrow the value with 'ref text' or move it last`）。`related` の `message` は名詞句か短い文（例: `value moved here`）。
- **人間向けの表示**: 主の位置の引用に続けて、`related` を 1 件ずつ `  note: <message>` と `    --> path:line:column`、その行の引用の順に出し、`  help: <help>`、`  fix (certain): <title>` を出す。別ファイルの `related` はそのファイルのソースを引く。
- **LSP**: `related` は `relatedInformation`、`fixes` は `quickfix` の code action（`certain` は `isPreferred: true`）にする。`kind` は `data.kind`。
- **上限**: 診断 1 件の `related` は 16 件、`fixes` は 8 件、`edits` は 1 つの fix で 64 件、`candidates` は 10 件、`scope` は 32 件。超えた分は黙って切り捨て、メッセージには影響させない。
  **ただし 1 つの fix の `edits` が 0 件または 65 件以上なら、fix を丸ごと捨てる**（途中で切ると半分だけ適用される fix になる。G22 の D3）。

### D-52 Facts と KB の共通仕様（G26・G27・G28・G29・G30・G31・E15・A20）

**2026-10-28 に [D-60](#d-60-kb-から-kg知識グラフへの再設計g26g27g28g29g30) で KG（知識グラフ）に改めた。** 以下の KB の名前（`target/tz/kb`・`symbols.jsonl`・`graph.jsonl`・`--kb` など）は D-60 の対応表で読み替える。行のキー・ID・整列・上限・カードの規則は変わらない。

- **用語**: *facts* は、コンパイラが解析で得た事実（シンボル・呼び出し・効果・トラップ箇所・確保箇所・複製箇所・所有権の出来事・型のレイアウト）の機械可読な表。*KB* は、facts と、そこから決定的に作る要約（brief カード）と図（Mermaid）を置いた
  ディレクトリ。**KB に LLM が書いた文章・日時・絶対パスは入れない。** 同じソースと同じコンパイラからは byte 単位で同じ内容ができる。
- **schema の名前**: KB は `tz-kb/1`（`manifest.json` の `schema`）、facts の各行は `tz-facts/1`。互換のない変更は番号を上げ、足すだけの変更は番号を保つ（読み手は知らないキーを無視する）。
- **置き場**: プロジェクト（`Tsuzuri.toml` か `Main.tz` のあるディレクトリ）の `target/tz/kb/`。環境変数 `TSUZURI_KB_DIR` か `--kb-dir DIR` で変える。プロジェクトでない単独のファイルは、`check`・`build`・`run`・`test` では既定で作らない（`--kb` で作る）。
  `doc`・`fmt`・`bench`・`script`・`repl`・`lsp` は KB を作らず、`--kb`・`--no-kb`・`--kb-dir` も受け付けない（単独のファイルや `script` の入力は `tsuzuri kb build FILE` で作る。G27）。`tsuzuri mcp` は環境変数を読まない（G29）ので、`TSUZURI_KB_DIR` を使う環境では `mcp --kb-dir DIR` を明示する（KB が見つからなければ MCP はメモリ上に作る）。
- **既定**: `check`・`build`・`run`・`test` は、プロジェクトの入力なら KB を作る（`--no-kb` か環境変数 `TSUZURI_KB=0` で作らない）。書き込めない・失敗した場合は、警告（`W2001` と同じ扱いのスパンのない通知）を 1 回出すだけで、終了コードと診断を変えない。
  入力（`.tz`・`.tt`・`.tc`・`Tsuzuri.toml`）の SHA-256・schema・`manifest.json` の `compiler`（`tsuzuri <版>`。機械によらず同じ文字列）が前回と同じで、マーカーの stamp（機械ごとのコンパイラの識別。`frontend_cache::compiler_identity()` に KB の版を足した値。G27 D3）も同じなら何も書き換えない。stamp は決定性のため `manifest.json` に入れない。
- **置換**: 一時ディレクトリへ書いてから rename する。マーカーファイル `.tsuzuri-kb`（`tsuzuri doc` の `.tsuzuri-docs` と同じ作法）のないディレクトリは上書きしない（`E2009`）。
- **ファイル**（JSON Lines は 1 行 1 オブジェクト、キーの順序は固定、数値は整数、文字列は `diagnostic::json_string` と同じエスケープ、行は ID・位置の順に整列）:
  `manifest.json`・`symbols.jsonl`・`graph.jsonl`・`facts/<Module>.jsonl`・`layout/<Module>.jsonl`・`diag.jsonl`（診断 v2 の JSON 行。`path` は根からの相対パスで、`--warn implicit-copy` の `W1006` は入れない。同じソースから同じ byte にするため。G27）・`brief/L0.md`・`brief/L1/<Module>.md`・`brief/L2/<Module>.<name>.md`・`views/*.mmd`・`tests.jsonl`。
  名前が `.` で始まるファイル（`.tsuzuri-kb`＝マーカーと stamp、`.tsuzuri-reuse.json`＝増分の鍵）は機械ごとのもので、`manifest.json` の `files` に入れず、上の byte の同一性の対象にしない。
  プロジェクトの KB に std は含めない（std の KB はコンパイラの識別で決まるキャッシュに 1 つ持ち、`kb` の照会が両方を合わせて見る。G27 の Phase 4）。
- **ID**: 関数・定数・型・クラス・テストの ID は、名前空間とモジュールを `::` でつなぎ、項目の前だけ `.` にする（`MyCompany::MyProject::Main.sum_to`、std は `Array.map`）。
  フィールドは `Main.Point.x`、union の case は `Main.Shape.Circle`、型クラスのメソッドは `Main.Show.show`、instance は `instance:Main.Show<Main.Point>`、テストは `test:Main.<名前>`（名前はテスト宣言の文字列）。
  ID は位置に依存せず、同じ宣言なら編集で行が動いても変わらない。位置は別のキーに持つ。
- **`symbols.jsonl` の行のキー（順序）**: `id`・`kind`・`module`・`name`・`signature`・`params`・`result`・`public`・`export`・`pure`・`doc`・`deprecated`・`origin`・`span`。
  `kind` は `module`・`def`・`const`・`record`・`union`・`case`・`field`・`class`・`method`・`instance`・`alias`・`test`・`bench`・`extern`・`handle`。
  `params` は `[{"name","type","conv"}]`、`conv` は `own`（所有権を受け取る）・`ref`・`ref_mut`・`copy`（Copy の値渡し）。`origin` は `project`・`dependency:<name>`。`span` は `{"path","start","end","line","column"}`（`path` はプロジェクトの根からの相対で `/` 区切り）。
- **`graph.jsonl` の行のキー**: `rel`・`from`・`to`・`args`（`calls` だけ。各実引数の `conv`）・`span`。`rel` は `calls`・`uses_type`・`implements`・`imports`・`tested_by`・`instance_of`。
- **`facts/<Module>.jsonl` の行のキー**: `id`・`effects`・`effect_sites`・`effects_complete`・`copy_sites`・`alloc_sites`・`alloc_complete`・`trap_sites`・`ownership`・`recursion`・`loops`・`specializations`・`truncated`
  （`effects_complete`・`alloc_complete` は G26 が足した真偽値。A20 が 14 個の効果をすべて計算するまでは `false`。キーは G26 の Phase が出荷したものだけを出し、出すキーは常に出す（値がなければ `[]`・`null`・`false`・`{}`）。F15 の Phase 4 は `parallel`（`[]` か要素の配列）を `specializations` の次・`truncated` の直前に足す）。
  `effects` は D-53 の語彙の名前を辞書順に並べた配列（推論した要約）。`ownership` の要素は `{"var","event","span"}`（`event` は `move`・`borrow`・`borrow_mut`・`reborrow`・`drop`・`copy`。`move`・`borrow` は `to`（呼び先の ID）を持てる）。
  1 つのキーの配列が 100 件を超えたら切り捨てて `truncated` に切り捨てた件数を足す。詳細な定義は G26。
- **brief カードの上限**（文字数。超えたら優先度の低い項目から `…N more` に置き換える）: L0（プロジェクト全体）2,400、L1（モジュール）1,600、L2（関数・型）700。上限の検査はテストで強制する。
- **freshness**: `tsuzuri kb verify` は入力の SHA-256 を計算し直して `manifest.json` と比べ、違えば `E2009`（`stale`）。`kb` の照会コマンドは、古ければ自動で作り直す（`--no-refresh` で止める）。

### D-53 効果の語彙と検査付き属性の共通仕様（A19・A20・A21・A23・E15・F15・G26・B10）

2026-10-23 の A19 後続範囲の実装判断: 明示した Task／Parallel は、入力に不透明な関数・Task・dyn がなく、
全コールバックを閉じた到達グラフで検査できる根に限って許可する。問い合わせもこの根の条件を検査し、
不明なら純粋としない。`builtin_impure` は Task のプリミティブ単体を引き続き不純とする（単体には閉じた根の証明がない）。
自動並列化やトラップを消す最適化には使わず、既存の明示した並行 API の失敗・待機の意味は変えない。

- **効果の名前**（`effects` の値。小文字の ASCII。この 14 個で閉じている）: `io`（コンソール・`IO`・`Async`）、`os`（`File`・`Dir`・`Env`・`Time`・`Random`・`Process`・`Os.__*`）、`net`、`task`（`Task`・`Parallel`、および失敗の領域を作る `Domain.run`（B10）。`Domain.run` は `alloc` も持つ）、`atomic`（`Atomic`・`Mutex`・`Channel`）、
  `rc`（`Rc`・`Arc` の参照カウントの観測）、`extern`、`debug`（`Debug.print`・`Debug.trace`）、`user_drop`（利用者の `Drop` を持つ型の解放）、`gpu`、`alloc`（ヒープの確保をしうる）、`trap`（プログラムの論理によるトラップをしうる）、`copy`（長さに比例する暗黙の複製をしうる）、`diverge`（停止しないかもしれない）。
  最初の 10 個は A19 の効果の源（`pure` が禁じる集合）と同じ分類を使い、`alloc`・`trap`・`copy`・`diverge` を A20 が足す。
- **要約**: 関数ごとの `EffectSet`（上の 14 個のビット集合）を、型付き IR の効果の源の走査と、呼び出しグラフの強連結成分の不動点で求める。組み込み関数は表（`match` に wildcard を置かず、新しい組み込み関数に分類を強制する）、`extern` は `extern` を持つ。
  通常の関数値の呼び出しは、呼び先が分からなければ「未知の呼び出し」で、検査付き属性の違反になる（A20 の Phase 4 が関数型の引数を条件付きにする）。A19 の `pure` 関数型は、検証済みの純粋な呼び出し契約を持つ。
  **純粋性の問い合わせの窓口**（B10・F15 が使う。`check::pure_rules`、`pub(crate)`）: `what(ty, types) -> &'static str`、`Impurity { reason: String, chain: String, span: Span }` と `impurity_of_roots(module, roots: &[usize]) -> BTreeMap<usize, Impurity>`、`reachable_effect(module, root: usize) -> Result<Option<Effect>, LimitExceeded>`、`builtin_impure(builtin: Builtin) -> bool`（組み込み関数を関数値として渡したときの判定）。A19 の有界な閉じた到達検査から作る（B10・F15 の手順が足りない分を足す）。**A20 の経路 A のあとも名前と形を変えず**、`pure_rules.rs` の薄い関数として残し、中身は `effects::summarize`（`Options::new(Scope::Reachable, PURE_FORBIDDEN)`）の `Witness` から作る（`Effect::Builtin` の名前は `Builtin::name()`）。B10・F15 は `builtin_effect`・`Category` を直接は呼ばない。
- **属性**（`@cpu` と同じ位置の、トップレベル `def` の直前の属性。ユーザー定義の属性はない）: `@no_alloc` は `alloc` を、`@no_copy` は `copy` を、`@no_trap` は `trap` を、`@terminates` は `diverge` を要約に持たないことを表明する。`pure`（A19）は A19 の集合を禁じる。
  **宣言の前の属性の文法**（A19・A20・E15 の共通。読み込み口は `parser.rs` の `program_all` の 1 か所）: `@cpu(..)`・`@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`・`@trusted(..)` を**任意の順序で**書ける（同じ属性の重複は `E1032`。`@trusted` は `E1037`）。`@no_*` と `pure def` は併用できる（`@no_alloc pure def`）。`@cpu` と `pure def` は併用でき、各 CPU 版の純粋性を検査する（2026-10-23 の全 Phase 承認で旧 A19 D12 を更新）。`@trusted` は `extern` だけ（それ以外は `E1037`）、効果の属性は `def` だけ（`extern` の前は `E1032`）。
  **読み込み口は先に入ったチケットが作り、後のチケットはその口に属性を足す**（置き換えない）: A20 が先なら `attributes()`（`@cpu` と 4 属性）に E15 が `@trusted` を足す。E15 が先なら `cpu_attribute` の次の `trusted_attribute()` を、A20 が `cpu_attribute()` を `attributes()` にするときに取り込む。符号化は `Program` の `cpu_attributes` → `effect_attributes` → `trusted` の順。
  **A20 の効果属性は表明であり、意味も生成 IR も変えない**（検査だけ。違反は診断 `E1031`、属性の置き場の誤りは `E1032`）。属性は要約の推論を置き換えず、推論した要約が表明を満たすかを検査する。要約は属性がなくても全関数について facts に出る。
- **`trap` の範囲**: 論理によるトラップ（`assert`、範囲外の添字・スライス、ゼロ除算、符号付きの除算の溢れ、`unreachable`、`range` のステップ 0、負の長さ・負の指数、`Checked`・`Reraise`、契約違反。A20 の T1〜T13 が正）。網羅されない `match` は `E1021` でコンパイル時に拒否されるので、`Match` は `trap` の源ではない。メモリの確保失敗とスタックの枯渇（資源の枯渇）は含めない。
  範囲の証明（`ranges.rs` と F12 の境界検査の省略）で除けるものは除く。
- **`alloc` の範囲**: 最適化の水準に依らない言語の規則で決める（`-O0` と `-O3` で表明の結果を変えない）。`new`、確保をする std・組み込み関数、環境を持つクロージャの生成、
  関数の外へ出る（返す・`own` の引数へ渡す・捕捉される・ヒープへ格納される）`new` のない集成値（昇格しうるため）を `alloc` とする。精密な表は A20。
- **api-diff**: 属性を足すのは互換、外すのは互換のない変更として報告する（A20）。
- **A20 での具体化**（A20 の D1・D3・D6・D8・D10・D18・D19）: 属性は `Program.effect_attributes`（`@cpu` と同じ脇のリスト。`FunctionDecl` には足さない）に持つ。要約は、属性か `pure` があるとき、または消費者（facts など）が呼ぶときだけ計算する（常には計算しない）。
  `Match` は `trap` の源ではない（`Checked`・`Reraise` は `trap`）。`for`・`ForEach` は有界で `diverge` ではなく、`while`・再帰・ブロッキングの組み込み関数が `diverge`。`extern` は `@no_alloc`・`@no_trap`・`@terminates` を破り（E15 の `@trusted` が緩める）、未知の呼び出しは全属性を破る。
  `E1031` は kind で 6 種に分け（属性ごとにコードを作らない）、要約が計算できない属性（`SUPPORTED` に含まれない効果）は `E1032` の `effect_attribute_unavailable` で拒否する（Phase を単独で出荷しても、検査されない保証を受理しない）。
- **契約（A21）**: `@requires`・`@ensures` は本体の先頭の文としての属性。検査の方式は `off`・`check`・`test`・`prove`。**契約を最適化の前提（assume）に使うモードは作らない**（未定義動作を作らないため）。
  契約は `FunctionDecl.contracts` に持ち、型検査器が `Block`・`Call`・`Local` だけで織り込む。実行時検査は組み込み関数 `$requires`・`$ensures` と末尾に足す 2 つの `TrapKind`（`ContractRequires`・`ContractEnsures`）。`off` は特殊化の直前に契約を剥がし、IR は契約なしと 1 byte 一致する。
  既定は `build`・`bench` が `off`、`run`・`test`・`repl` が `check`（`check`・`doc` は何も生成せず、契約を常に型検査する）。`prove` は `QF_BV` の SSA の台本を外部ソルバー（環境変数 `TSUZURI_SMT`。同梱しない）に渡し、`proved` 以外は `E1034`。

### D-54 edition 2027 の正準形の方針（A22・G24・A18）

- 方針は [docs/stability.md](../docs/stability.md) の範囲に収める。edition の差として許されるのは「予約語を増やす」「前の edition で非推奨にした構文を削除する」「警告の既定を変える」の 3 つだけで、両方の edition が受理するプログラムの意味は同じ。
  **2026-10-23 の A18 全 Phase 承認による例外**: 通常の `def` の引数の既定を 2027 で共有借用にし、
  `mut T`・射影への代入・Array／List の読み取り callback 入口・`Vec.push` の排他更新と unit 戻り値を許す。
  上記の「意味は同じ」はこの引数規約と版別 API を除く。2026 の規約と、明示した借用・数値・評価順・トラップは変えない。
  A22 の正準形（`..<`・`..=`、型名の接尾辞、`ref`／`ref mut`／`deref`）は**全 edition が受理する**（受理を増やす拡張は常に許される）。A18 は A22 の表記の受理・拒否を実装しない。
- A18 Phase 2 は `LATEST = 2027` にし、std・manifest なしの CLI・script・REPL・新規パッケージも 2027 で読む。
  std は `own` と `_owned` 入口で旧契約を保存する単一コピーである。`MANIFEST_DEFAULT = 2026` は変えない。
  `tsuzuri fix --edition 2027 [--from 2026] PATH` は旧署名・API を保存して対象版で全体を再検査し、
  ソースと manifest を atomic に移行する。G24 の一般の warning fix ではなく、誤りが残る場合は何も書かない。
- **A22・G24 の未実装計画:** 全版で正準形を受理し、旧版の非正準形を `W1007` と `certain` fix で案内し、
  新しい edition で `E0004` にする。起票時の仮称は 2027 だが、A18 のリリースで凍結した版へ拒否を追加してはならず、
  版の割り当てを改めて承認する。`check`・`build`・`run`・`test`・`script`・`new` の `--edition`、
  `NEW_PROJECT`・`rejects_noncanonical`・`src/canonical.rs` は A18 では実装しない。
- A22 の対象（起票時に存在を確かめたものだけ）: `for` の範囲の素の `..`（`..<` と `..=` に）、
  短い数値接尾辞（型名の接尾辞に）、記号の借用（`&x`・`&mut x`・`*r`・`&T`・`&mut T`）、旧ラムダ `x -> e`。
  **A22 は `..` の意味を edition で変えない**。A18 はこれらの旧表記を拒否しない。

### D-55 識別子の割り当て（診断コード・サブコマンド・フラグ・環境変数・std）

| 種別 | 名前 | チケット | 用途 |
| --- | --- | --- | --- |
| 診断コード | `E0004` | A22 | edition で拒否する表記（kind で区別） |
| 診断コード | `E1030` | G25 | 型つき穴 |
| 診断コード | `E1031`・`E1032` | A20 | 効果の表明の違反・属性の置き場の誤り（A21 の `@no_*` に似た誤りは `E1033`） |
| 診断コード | `E1033`・`E1034` | A21 | 契約の節の誤り・検証の失敗（`prove`） |
| 診断コード | `E1036`・`E1037` | E15 | 権限の違反・`@trusted` の誤り |
| 診断コード | `E1038` | B10 | `Domain` の使い方の誤り |
| 診断コード | `E1039` | F15 | `Monoid` の還元・並列 API の使い方の誤り |
| 診断コード | `E2008` | G24 | `tsuzuri fix` の中止（新しい誤りが出る、競合、書き込めない） |
| 診断コード | `E2009` | G27・G28 | KG の欠落・古い・壊れた・上書きできない（D-60） |
| 診断コード | `E2010` | G29 | MCP の致命的な誤り |
| 診断コード | `E2011` | G34 | `tsuzuri test --doc` の例の記法の誤り（閉じのない・字下げした・空の `tsuzuri` のフェンス） |
| 警告 | `W1007` | A22 | 非正準な表記（既定は無効） |
| 警告 | `W1008` | A20 | 効果の属性が冗長・効かない |
| 警告 | `W1009` | A21 | 契約が常に真・使わない束縛など |
| 警告 | `W1010` | E15 | 信頼を表明していない `extern` の使用（`audit` が既定で警告に格上げ） |
| 予約（未割り当て） | `E1035`・`E1040`〜`E1049`・`E2012`〜`E2015`・`W1011`〜`W1015` | — | 実装中に必要になったら、この表に追記してから使う |
| サブコマンド | `guide` | G21 | AI 向けの案内の出力 |
| サブコマンド | `explain` | G23 | 診断コード・kind の説明 |
| サブコマンド | `fix` | G24 | 機械的な修正の適用 |
| サブコマンド | `find` | G25 | 型での検索（`kg find` も同じ実装。D-60） |
| サブコマンド | `kg`（`build`・`verify`・`clean`・`export`・`brief`・`show`・`callers`・`callees`・`uses`・`search`・`find`・`tests-for`・`impact`・`path`・`diagram`） | G27・G28・G30 | 知識グラフの作成と照会（D-60。旧名 `kb`） |
| サブコマンド | `mcp` | G29 | stdio の MCP サーバー |
| サブコマンド | `audit` | E15 | 信頼境界と権限の報告 |
| サブコマンド | `prove` | A21 | 契約の静的検証（`--contracts prove` と同じ） |
| CLI | `--kg`・`--no-kg`・`--kg-dir DIR` | G27 | KG の作成の制御（D-60。旧名 `--kb`・`--no-kb`・`--kb-dir`） |
| CLI | `--warn canonical` | A22 | `W1007` を有効にする（既存の `--warn implicit-copy` と同じ形） |
| CLI | `--edition 2026\|2027` | A22 | マニフェストのない入力の edition の指定（`check`・`build`・`run`・`test`・`script`） |
| CLI | `--applicability certain\|likely\|maybe`・`--dry-run` | G24 | `fix` の適用範囲と差分の表示 |
| CLI | `--contracts off\|check\|test\|prove` | A21 | 契約の扱い（既定は `build` が `off`、`run`・`test`・`repl` が `check`。`check`・`doc` は何も生成せず常に型検査だけ。`--prove-timeout`・`--function` は `prove` のときだけ） |
| CLI | `--overflow-report` | A23 | 折り返しの報告の計装（`test` は既定で有効） |
| CLI | `--allocator counting` の `build`・`run`・`test` への対応 | F14 | 確保数の計装（`build`・`run` は明示、`test` は `Alloc` に到達すると自動） |
| 環境変数 | `TSUZURI_KG`（`0` で作らない）・`TSUZURI_KG_DIR` | G27 | KG（D-60。旧名 `TSUZURI_KB`・`TSUZURI_KB_DIR`） |
| 環境変数 | `TSUZURI_GUIDE_EXAMPLES`（テスト用） | G21 | ガイドの例の検証の対象の切り替え |
| std | `Text`（型クラスとモジュール） | D14 | 文字列の API の統一 |
| std | `Alloc` | F14 | 確保数の測定（テストでは自動、通常の `build`・`run` は counting を明示） |
| std | `Domain` | B10 | 純粋関数の失敗の閉じ込め |
| std | `Monoid`（クラス）と `Parallel.fold` など | F15 | 還元の保証 |
| 属性 | `@no_alloc`・`@no_copy`・`@no_trap`・`@terminates` | A20 | D-53 |
| 属性 | `@requires`・`@ensures`（本体の先頭の文） | A21 | 契約 |
| 属性 | `@trusted` | E15 | `extern` の信頼の表明 |
| 予約語 | なし | — | 第3期は予約語を足さない（文脈キーワードと属性だけ） |

**D-55 の追補（各チケットの確定。2026-10-18）**: 次は、チケットを書く間に決まった名前。**診断コードの kind・`E2000` の kind はチケットが G23 の登録簿へ足す**（コード自体は上の表だけ）。同じサブコマンドのフラグの名前が衝突しないように、ここに集める。

| 種別 | 名前 | チケット | 用途 |
| --- | --- | --- | --- |
| CLI（`guide`・`new`） | `--level`・`--topic`・`--list`・`--agents`・`--llms-txt`・`--edition`（`guide` の中だけ）・`--no-agent-files`（`new` の中だけ） | G21 | AI 向けの案内。JSON の schema は `tz-guide/1` |
| CLI（`find`・`kb find`） | `--project`・`--result`・`--takes`・`--module`・`--limit` | G25 | 型でのシグネチャ検索 |
| CLI（`kg build`・`kg clean`、`kg export`） | `--std`・`--force`、`--nodes`・`--edges`・`--facts` | G27 | std の KG・古くなくても作り直す・書き出すファイル（D-60） |
| CLI（`kg` の照会・`impact`・`path`） | `--path`・`--depth`・`--budget`・`--no-refresh`・`--git`・`--from`・`--to`・`--affected`・`--rel` | G28 | 照会の範囲・文字数の上限・差分の指定・最短路の関係（`--rel` は D-60） |
| CLI（`kg diagram`） | `--format mermaid\|dot`・`--max-nodes N`（5〜200）・`--depth N`・`--path`（この 4 つは `kg diagram` の中だけ。`--kg-dir`・`--no-refresh` は G28 と同じ意味） | G30 | 図の出力形式と大きさの上限。誤りは `E2000` の kind `diagram_usage`・`diagram_unknown_kind`・`diagram_needs_id`・`diagram_format_unsupported`・`diagram_bad_option`・`diagram_unknown_id`・`diagram_wrong_id_kind`・`diagram_unavailable`・`diagram_too_large`（すべて終了コード 2） |
| CLI（`mcp`） | `--root DIR`・`--kg-dir DIR` | G29 | stdio の MCP サーバー（プロトコルは 2025-06-18 だけ。**環境変数を読まない**ので `TSUZURI_KG_DIR` は効かず、置き場は `--kg-dir` か `<root>/target/tz/kg`） |
| CLI（`fix`） | `--applicability`・`--dry-run`・`--edition`（移行先の edition） | G24 | `--applicability maybe` は `--dry-run` のときだけ受理する。`--yes` は作らない（提案の自動適用をしない） |
| CLI（`build`・`run`・`test`） | `--overflow-report`（`build`・`run`・`test`）・`--allow-overflow`（`test` だけ） | A23 | 折り返しの報告。`test` は既定で報告し、折り返したテストを失敗にする（`--allow-overflow` で警告のまま通す）。`--no-overflow-report` は作らない |
| CLI（`--contracts prove`・`prove`） | `--prove-timeout SECONDS`（1〜3600）・`--function NAME` | A21 | 契約の静的検証（`prove` のときだけ使える） |
| CLI（`--warn` の値） | `untrusted-extern` | E15 | `W1010` を有効にする。**`--warn` の値の一覧とエラー文の `supported:` は、実装済みの値だけを固定の順序 `implicit-copy, canonical, untrusted-extern` で並べる**（`main.rs` の 1 つの定数から解析・`--help`・エラー文を作る。先に入ったチケットが作り、後のチケットは順序の位置に足す） |
| CLI（`test`） | `--doc`（`--list`・`--filter`・`--index`・`--json` と併用できる。`test` 以外・`--coverage`・`--seed`・`-g`・`-o` との併用は `E2000`） | G34 | `///` の例の検査。`E2011` を出す |
| 終了コード | `E2000` | 全チケット | コマンドラインの語だけで決まる誤りは 2。入力（マニフェスト・ソース）を読んだあとに分かる誤り（A22 の `--edition` とマニフェストの衝突など）は 1（既存の読み込みの誤りと同じ） |
| CLI（`bench`） | `--allocs` | F14 | bench ごとの確保数 |
| CLI（`check`） | `--report parallel`（`check` だけ。ほかのサブコマンドは引数エラー。`--json` と併用できる。報告は stdout、提案だけで自動では適用しない） | F15 | 並列化の提案と、既存の `Parallel.*` の呼び出しの純粋さ。JSON の schema は `tz-report-parallel/1`。付けない `check` の出力は 1 byte も変わらない |
| 診断コード | `E2011` | G34 | 上の表。予約は `E2012`〜`E2015` に縮む |
| 環境変数 | `TSUZURI_SMT` | A21 | `prove` のソルバー（名前か経路。なければ PATH の `z3`、なければ `cvc5`。同梱しない） |
| 環境変数 | `TSUZURI_TRAP_REPORT`・`TSUZURI_LLVM_SYMBOLIZER` | B10 | クラッシュレポートの出力先・`llvm-symbolizer` の場所 |
| 環境変数 | `TSUZURI_PROGFUZZ_*`（`KIND`・`PATHS`・`BUDGET`・`DIR`・`JOBS`・`KEEP`・`MINIMIZE`・`MUTANT`・`INJECT_LEAK`） | G36 | 差分ファズ（`tests/`・`scripts/` だけが読む。コンパイラは読まない） |
| std | `Arbitrary`（組み込みクラス）・`Gen.for_all_where`・`Gen.check_where` | A21 | 契約の性質テスト |
| std | `Int.wrapping_*` | A23 | 折り返しの意図（`Int` は組み込みモジュール） |
| std | `Text`（常に読み込む型クラス）・`RegexText` | D14 | 文字列の API の統一。`RESERVED_MODULES` の数は現在値から増やす |
| std | `Monoid`（型クラス。常に読み込む）・`Monoids`（opt-in の newtype 群。`Monoids.Sum` のように修飾して書く）・`Array.fold_monoid`・`Parallel.fold` | F15 | 還元の保証。浮動小数点を含む型の `Monoid` は `E1039`（kind `monoid_float_instance`）。`RESERVED_MODULES` へ `Monoid`・`Monoids`（現在値から 2 増やす） |
| std | `Map.chunk_count`・`Map.invariants_hold`・`Set.chunk_count`・`Set.invariants_hold` | C12 | 診断・テスト用の公開関数（チャンクの数と不変条件の検査）。`Map`／`Set` の計算量の表は C12 の仕様 §5 が正（G35 が `@complexity` を生成するときに合わせる） |
| 生成物 | `target/tz/kg/`・`target/progfuzz/`・`target/ai-cost/` | G27・G36・G31 | すべて `target/` の下（コミットしない） |

### D-56 AI 向け成果物（ガイド・AGENTS.md・llms.txt・brief カード）の言語とサイズ

- 言語は**英語**にする（トークン数が少なく、モデルの習熟度が高い。診断のメッセージも英語）。利用者向けの文書（言語リファレンス・README）は日本語のままで、この規則の対象外。ここでいう brief カードは G27 の KB のカード。**G32 の `repo-kb brief`（開発リポジトリの文脈パック）は別物**: 見出しと案内は英語、取り込んだ本文（チケット・GUIDE-LITE の抜粋）は原文のまま（日本語）で、サイズの上限（16,000〜24,000 字）は G32 が決める。
- サイズの上限（文字数。テストで強制する）: ガイド L0 6,000、L1 20,000、std の 1 モジュールのページ 6,000、std 全体の 1 行索引 45,000、`AGENTS.md` のひな形 3,500、`llms.txt` 1,500。
- 内容は、コンパイラの表・std のソース・検証済みの例から生成するか、検証済みの例（`tsuzuri check`／`run` を通る）だけを置く。**実行を通らない例を置かない。** 落とし穴の表は 1 か所（G23 のヒント DB）に置き、ガイドはそこから生成する。
- ガイドと brief カードは互換性の保証の対象外（文言は直してよい）。`tsuzuri guide --json` と KB のキーは D-52 に従う。

### D-57 権限（capabilities）の語彙（E15・G26）

- 名前（閉じている）: `stdio`（`IO` のコンソール・`Debug`）、`fs`（`File`・`Dir`）、`env`（`Env`）、`time`（`Time`）、`random`（`Random`）、`process`（`Process`）、`net`（`Net`）、`gpu`（`Gpu`）、`ffi`（`extern`）。
  `Path` は文字列の操作だけなら権限を要らない（OS に触れる関数があれば `fs`。E15 が確定する）。`Task`・`Parallel`・`Atomic`・`Mutex`・`Channel` は CPU の資源であって権限ではない。
- 既定: ルートのパッケージは全権限を持つ。依存パッケージは `Tsuzuri.toml` の `[dependencies]` の `capabilities = [...]` で許可された範囲だけ（既定は空）。実効の権限は、ルートが許した範囲と、各依存が自分の依存へ許した範囲の積。

### D-58 仕様の規則 ID の形式（G33・G35）

- 形式は `AREA-NNN`（大文字 3 字・ハイフン・数字 3 桁）。AREA は `LEX`（字句）・`SYN`（構文）・`TYP`（型）・`OWN`（所有権と借用）・`EVL`（評価順序）・`NUM`（数値）・`STR`（文字列）・`CTL`（制御とパターン）・`MOD`（モジュールとパッケージ）・`TSK`（タスクと並行）・`DIA`（診断）・`CLI`・`ABI`・`STD`・`EDN`（edition）。
- 文書の規則の文の直前に HTML コメント `<!-- rule: OWN-012 -->` を置く（表示に出ない）。規則の本体は `spec/rules/<AREA>.md` の表（ID・規則の 1 文・文書の節へのリンク・適合性テストの一覧）。ID は再利用しない。廃止は `retired` にする。

### D-59 標準ライブラリの引数の順序（対象は最後）

- **規則**: 関数が読む・変換する・更新する値（対象。コレクション、文字列、`Maybe`・`Result`・`IO`、行列・窓・テンソル、アリーナ、`Json` の書き込み器など）を最後の引数にする。キー・添字・個数・コールバック・区切りなどは、F# または .NET の順のまま、その前に置く。これで `x |> M.f a` と `M.f a >> M.g b`（`>>` は `\v -> g (f v)` に展開される）がどのモジュールでも通る。F# の `List`・`Map`・`Set`・`Option`・`Result` の関数と同じ並びで、F# が配列やリストを先に取る関数（`Array.get`・`set`・`sub`・`fill`・`blit`・`fold_back` 系と `List` の 4 関数）だけを、対象が最後になるよう移した。
- 対象が複数ある関数（`append`・`zip`・`union`・`mul`）は F# の順で、最後の対象が最後。一方だけを書き換える関数は、書き換えられる方が最後（`MatrixView.copy_from source target`）。対象のない関数（`empty`・`init`・`of_array`・`parse`）はこの規則の外。借用版の双子（`get_ref`・`foldr`）は元の関数と同じ順。
- 評価順: 対象は最後に移動・借用されるので、前の引数の式は対象を読める（`Array.write 0 (v[1] * 10) v`）。パイプ（`v |> Array.write 0 (v[1] * 10)`）は対象を先に借りるので `E1014`。
- **例外は 4 種類だけ**（`std/` のファイルがある例外は、理由を `tests/std_argument_order.rs` の `MODULES` の行に書く。組み込みだけのモジュールにはファイルがないので、この一覧が記録）: (1) 資源のハンドル（`File`・`Net`・`Process`・`Atomic`・`Mutex`・`Channel`・`Rc`・`Arc`・`Owned`・`Task`）。ハンドルが先で、.NET のメソッドの受け手と同じ。(2) 数値の関数（`Int`・`Math`・`Simd`・`Char`）。数式の被演算子の順。(3) コンパイラが呼ぶプロトコル。計算式ビルダーの大文字の操作（`Bind` など）と、`derive` が呼ぶ `Json.decode_field`・`Json.case_index`。(4) テストとベンチマークの入口（`Gen.for_all`・`Test.*`・`Bench.*`）。理由のない例外と、純粋な変換の例外は認めない。資源のモジュールの順序を変えるときは、`Atomic`・`Mutex`・`Channel`・`Rc`・`Arc`・`Task` の組み込みの生成（`src/llvm_*.rs` が引数の位置で読む）と、ネットワーク・非同期のテストも一緒に直す別のチケットにする。
- コンパイラが名前や位置で見ている std 関数は、順序を変えるときにフックも同時に直す。`String.slice`（`src/llvm_strings.rs` の `compact_body`）、組み込みの `Utf8String.decode_at`、組み込みの `Array.__set`・`Array.__sort_with`・`Array.update`・`Array.swap`・`Array.write`・`Array.swap_in`・`Array.split_at_mut`（`src/check.rs` の `Builtin::name` で見つかる）。`src/derive.rs` が呼ぶ `Json`・`Result`・`Maybe` の関数は順序を変えない。`src/` で名前を検索して確かめる。
- 検査は `tests/std_argument_order.rs`。`std/` の全ファイルを `MODULES` に分類しないと赤になるので、新しいモジュールは必ずここに当たる。`def` 行の型で「対象を取る関数は対象が最後」を調べ、同じ型が 2 つ並ぶ `String`・`Utf8String`・`Regex` は `fn` 行の最後の仮引数名（`text`・`parts`）で調べる。コンパイラ組み込みの `Array`・`List`・`Vec`・`String`・`Utf8String` の型にも同じ規則を当て、旧順の呼び出しが型エラーになること、パイプと合成が通ることも確かめる。
- 文書は、利用者向けの `_tsuzuri/language-reference/built-in-types-and-modules/argument-order.md` と各モジュールのページの「引数の順序」の節、仕様の `docs/language.md`（標準ライブラリの引数の順序）、AI 向けの `AGENTS.md`（Standard library API conventions）。
- 経緯: `Array`・`List`・`Vec` を先に揃え（`Vec` は .NET の `List<T>` との互換と同時）、その後に `Map`・`Set`・`HashMap`・`HashSet`・`Arena`・`String`・`Utf8String`・`Matrix`・`MatrixView`・`Tensor`・`Maybe`・`Result`・`IO`・`Regex`・`Json` を同じ規則に揃えた。

### D-60 KB から KG（知識グラフ）への再設計（G26・G27・G28・G29・G30）

- **経緯**: 2026-10-28、利用者の指示で、D-52 の KB（ファイルの集まりとしての知識ベース）を、グラフ理論に基づく **KG（知識グラフ）** に改めた。G26〜G30 のチケットの本文は書いた時点（KB）のまま残し、各チケットの先頭の「KG への更新（D-60）」の節と、この決定を正とする。D-52 の行のキー・ID の規則・整列・上限・カードの上限・鮮度の規則は変えない。
- **モデル**: KG は型付きの有向多重グラフ（property graph）*G* = (*V*, *E*)。
  - **ノード** *V*: `nodes.jsonl`（旧 `symbols.jsonl`）の 1 行が 1 つのノードで、`id` が主キー、`kind` がラベル（`module`・`def`・`const`・`record`・`union`・`case`・`field`・`class`・`method`・`instance`・`alias`・`test`・`bench`・`extern`・`handle`）。ノードの性質は、その行のキーと、性質の表 `tests.jsonl`（テストのノードの `index`）・`facts/<key>.jsonl`（関数的なノードの事実）・`layout/<key>.jsonl`（型の実体のレイアウト。キーは `type_id_text`）の、同じ ID の行。
  - **辺** *E*: `edges.jsonl`（旧 `graph.jsonl`）の 1 行が 1 本の有向辺 `from` → `to`。ラベルは `rel`、性質は `args`・`span`。同じ端点の間に、出現の位置ごとに別の辺がありうる（多重グラフ）。
  - **構造の辺 `contains`（新規）**: module → 最上位の宣言（`$entry`・instance・test・bench を含む）、record → field、union → case、class → method、instance → method。`span` は子の `span`。**不変条件**: `contains` は森で、根はちょうど `module` のノード、`module` 以外のノードは `contains` の入次数がちょうど 1（`kg verify` とテストが検査する）。親を ID の文字列から推測しない（`contains` を辿る）。
  - **端点**: `from` はつねにこの KG のノード（`implements` の `from` だけは型の `type_id_text` で、利用者の型でなければ外部）。`to` は、この KG のノードか、**外部の端点**（std の KG のノード、組み込み関数 `sqrt`・組み込みのクラス `Eq`、`imports` の名前空間）。読み手は外部の端点を「この KG にない葉」として扱い、std の KG（G27 Phase 4）があれば、そのノードに結ぶ。
- **索引とアルゴリズム**: 読み手（`KgData`）は読み込みのときに 1 回だけ、ノードの ID を 0..*n* の添字にし、関係ごとに前向き・後ろ向きの隣接（CSR。辺は `(相手, span.path, span.start)` の順）を作る（`KgIndex`）。グラフのアルゴリズムは `src/kg/algo.rs` の 1 か所にまとめ、どれも明示のスタック・キューで書く（再帰しない）。
  | アルゴリズム | 使う所 |
  | --- | --- |
  | 有界の幅優先（*k* 近傍、親つき） | `kg callers`・`callees`（深さ 1〜8）、`tested_by`（深さ 8・200 件）、`kg diagram calls` |
  | 強連結成分（Tarjan の反復版）と縮約 | facts の `recursion`、`kg diagram module` の循環、`impact` の変更モジュール |
  | 逆向きの到達可能性（深さつき。仮想の辺つき） | `kg impact` と `test --affected` の影響を受けるテスト（クラスのメソッドの呼び出しは、`instance_of` でそのクラスの全 instance のメソッドへ届く辺を足す） |
  | 最短路（重みなしの幅優先。同じ長さなら ID の byte 順で最初） | 新しい照会 `kg path`、`impact` の JSON の根拠の鎖（`via`） |
- **新しい照会** `tsuzuri kg path FROM TO [--rel R[,R…]] [--depth N]`: `FROM` から `TO` への最短路を、指定した関係（既定 `calls`。`contains`・`calls`・`uses_type`・`implements`・`instance_of`・`imports`・`tested_by` の組み合わせ）の辺だけで求める（`--depth` は 1〜16、既定 8）。1 行 1 段 `{ID} --{rel}--> {ID}  {path}:{line}:{column}`、最後に `summary: {n} steps`（見つからなければ `summary: no path within depth {N}`。どちらも終了コード 0）。`--json` は段ごとの `{"type":"step",…}` と `{"type":"summary",…}`。MCP は `tz_path`（G29）。
- **検証**: `kg verify` は、ファイルの SHA-256 に加えて、グラフの不変条件（`contains` の森、辺の `from` がノードであること、ID の一意性）を検査し、違えば `E2009` `kg_broken`。
- **名前の対応**（D-52・D-55 と各チケットの本文の名前は、この表で読み替える。行のキー・語彙・ID は変えない）:
  | 旧（KB） | 新（KG） |
  | --- | --- |
  | `target/tz/kb/`、マーカー `.tsuzuri-kb`（1 行目 `generated by tsuzuri kb`） | `target/tz/kg/`、`.tsuzuri-kg`（`generated by tsuzuri kg`） |
  | `manifest.json` の `schema` `tz-kb/1`（facts の行の `tz-facts/1` は変えない） | `tz-kg/1` |
  | `symbols.jsonl`・`graph.jsonl`（`manifest.json` の `counts` の `symbols`・`public_symbols`） | `nodes.jsonl`・`edges.jsonl`（`nodes`・`public_nodes`） |
  | `tsuzuri kb …`（`build`・`verify`・`clean`・`export`・`brief`・`show`・`callers`・`callees`・`uses`・`tests-for`・`search`・`find`・`impact`・`diagram`） | `tsuzuri kg …`（同じ名前と `path`） |
  | `kb export --symbols`・`--graph`・`--facts` | `kg export --nodes`・`--edges`・`--facts` |
  | `--kb`・`--no-kb`・`--kb-dir`、`TSUZURI_KB`・`TSUZURI_KB_DIR` | `--kg`・`--no-kg`・`--kg-dir`、`TSUZURI_KG`・`TSUZURI_KG_DIR` |
  | `E2009` の kind `kb_missing`・`kb_stale`・`kb_broken`・`kb_not_ours`・`kb_incomplete` | `kg_missing`・`kg_stale`・`kg_broken`・`kg_not_ours`・`kg_incomplete` |
  | `src/kb/`・`src/kb_capture.rs`・`KbModel`・`KbData`・`KbView`・`KbFlags` | `src/kg/`・`src/kg_capture.rs`・`KgModel`・`KgData`・`KgView`・`KgFlags` |
  | `tests/kb*.rs`、fixture `kb_basic`・`kb_shop`・`kb_shop_head`、`tests/golden/kb/` | `tests/kg*.rs`、`kg_basic`・`kg_shop`・`kg_shop_head`、`tests/golden/kg/` |
  | 文書 `compiler/kb-format.md`・`compiler/kb.md` | `compiler/kg-format.md`・`compiler/kg.md` |
  | MCP の `--kb-dir`、資源 `tz://kb/brief/…`、能力 `kb` | `--kg-dir`、`tz://kg/brief/…`、`kg` |
  | brief カードの L0 の行 `tz-kb/1 \| … \| symbols S \| …`、`next:` の `symbols.jsonl, graph.jsonl` | `tz-kg/1 \| … \| nodes N \| edges E \| …`（`edges` は `edges.jsonl` があるとき）、`nodes.jsonl, edges.jsonl` |
- **G25 との関係**: G28 Phase 2 の `kg find` は、G25 の完了を待たず、KG の関数のノード（プロジェクトと std の KG）の署名を型で照合する検索として実装する（グラフの問い合わせの 1 つ）。G25 の `tsuzuri find` は、出荷するときにこの実装を呼ぶ（D-55 の「同じ実装」のまま）。G29 の `tz_find` はこの実装を使う。
- **変えないもの**: `check`・`build`・`run`・`test` の出力・終了コード・IR、診断コード `E2009`・`E2010`・`W2001` の扱い、facts の規則（G26 の付録）、カードの文字数の上限（L0 2,400・L1 1,600・L2 700）、決定性（同じソースと同じコンパイラから byte 単位で同じ）。

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

- **stack の深さ。** テストのスレッドは既定で 2 MiB の stack で動く（dev profile は opt-level 1 で frame が小さくなったが、再帰の深さに注意する点は変わらない。CI は 4 MiB を付ける）。`syntax` の enum の variant を小さく保ち（大きい field は `Box`）、
  `Parser::primary` の分岐を小さく保ち、引数の検査の入口は小さな振り分けにして重い処理は再帰しない補助関数へ出す。
  parser の再帰する補助関数は入れ子の数を必ず増減する（減らし忘れて深さ 128 の前に溢れた例がある）。
  上限や stack の大きさを上げてテストを通してはいけない。回帰テストは §3.1 の 3 つ（`bounds_type_growing_polymorphic_recursion`、
  `bounds_recursive_and_flat_expression_depth`、`bounds_nested_builder_expansion_not_just_source_syntax`）。
- **`_ =>` の fallback。** 新しい variant を黙って無視する match が多い。§6.9 の手順で全出現を確かめる。
- **std の generic 関数と特殊化の予算。** 単相化は利用者の非 generic 関数からだけ始める。std の generic 関数を先に特殊化すると、
  利用者の特殊化の予算を消費する（上限は 65,536。D-41）。`honors_the_exact_specialization_limit` は `src/polymorph.rs` の単体テストで、
  テスト用の小さい上限で予算の数え方を確かめる。std に API を足したら必ずこのテストを実行する。
- **LLVM の型の出力順。** enum の別名（`= type i8` など）は、どの record／union の struct 定義よりも前に出す（LLVM は非 struct の別名を前方参照できない）。niche の union は型定義を持たない。
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
- 関数やブロックの中の再帰は `let rec go = \n acc -> ...`（相互再帰は `and`）。右辺はラムダ式に限る。捕捉は値か共有借用で、末尾呼び出しはループになる（B09、D-46）。
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
| `let new = 1` | `E0002`（`new` は予約語） | 別の名前（例: `create`） |
| `Atomic.new`（`.` の後の `new`） | 名前として読む。存在しなければ `E1002`（C10 の `Rc.new`・`Arc.new` のため。D-40） | 定義された関数を呼ぶ |
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
- 変更したファイルの一覧。コミットは自分のブランチに作ってよい（1 チケット 1 ブランチ・1 PR。`main` にはコミットも push もしない。`AGENTS.md`）。

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
