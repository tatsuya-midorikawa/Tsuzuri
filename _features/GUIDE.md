# 実装チケット共通ガイド

このガイドは `_features/` の全チケットに共通する前提・手順・設計決定をまとめたものです。
**チケットに着手する前に、このファイル全体と対象チケット、`AGENTS.md`、`docs/language.md`、
`docs/architecture.md` の関連節を必ず読んでください。** チケットとこのガイドが矛盾する場合は、
このガイドの「9. 設計決定台帳」を優先し、矛盾をチケットの「未決事項」に追記して人間に報告します。

調査時点のコミットは `19d8cdd`（2026-09-23）です。行番号は変わりやすいため、コード参照は
「ファイル + 関数名／型名」で書いています。見つからない場合は `grep` で関数名を探してください。

---

## 0. チケットの使い方（実装担当モデル向け）

1. 対象チケットの「依存」がすべて完了しているか `_features/README.md` の状態欄で確認する。
   未完了の依存があれば着手しない（先に依存チケットを実装する）。
2. 「現状」節に書かれたコードを実際に開き、記述が今のコードと一致するか確認する。
   一致しない場合は、チケットの意図を保ったまま現在のコードに合わせる（差分をチケット末尾に追記）。
   **P2／P3 のチケットは独立レビューを受けていないため、着手前にその時点のコードと台帳に対してレビューを行い、
   指摘を反映してから実装する**（README の「レビュー状況」参照）。
3. 「実装手順」を **上から一段ずつ** 実施し、各段の「確認」に書かれたテストを通してから次へ進む。
   複数段をまとめて書いてから一度にテストしない。
4. 仕様に書かれていない挙動を推測で追加しない。必要なら「未決事項」の既定案に従い、
   既定案がなければ最も保守的な挙動（コンパイルエラーで拒否）を選ぶ。
5. 完了したら「受け入れ条件」の全項目を確認し、`_features/README.md` の状態を `done` に更新する。

### チケットの共通構成

| 節 | 内容 |
|---|---|
| メタ情報 | ID、優先度（P0–P3）、規模（S/M/L/XL）、依存、後続、影響ファイル |
| 目的 | なぜ必要か、利用者にとっての価値 |
| 現状 | 現在の実装・仕様・制約（コード参照付き） |
| 仕様 | 構文（EBNF 風）、型規則、評価順序、所有権・借用、数値の意味、診断、native/WASM の差 |
| 設計 | データ構造の変更、各コンパイラ段の変更点、アルゴリズム、生成 IR の形 |
| 実装手順 | 小さく検証可能な段階。各段に「確認」方法 |
| テスト計画 | Rust テスト（受理／拒否と診断コード）、fixture、Node E2E（native/WASM × -O0/-O3）、解放追跡、性能測定 |
| ドキュメント | 更新すべき README／docs の節 |
| 受け入れ条件 | チェックリスト |
| 落とし穴 | 間違えやすい点 |
| 対象外 | このチケットでやらないこと |
| 未決事項 | 人間の判断が望ましい点と、その既定案 |

規模の目安: S = 1〜2 日、M = 3〜5 日、L = 1〜3 週、XL = 1 か月以上または複数チケットへの分割が前提。

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

---

## 2. 作業開始前の準備

### 2.1 ツールチェーン

- Rust 1.85 以降（edition 2024）、LLVM/Clang 17 以降、`wasm-ld`、Node.js 20 以降、Python 3.9 以降。
- macOS: `brew install llvm lld` の後 `export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"`。
- `TSUZURI_CLANG`／`TSUZURI_WASM_LD` で実行ファイルを指定できる。

### 2.2 既知の落とし穴: ランタイム IR が Git 管理外

`.gitignore` の `*.ll` により、`src/runtime/string.ll`、`heap-native.ll`、`heap-wasm.ll`、`console.ll`、
`numeric.ll` が **追跡されていません**（`git ls-files src/runtime` で確認できる）。`src/llvm.rs` は
これらを `include_str!` するため、新しい clone や worktree ではビルドできません。
チケット **G01** で修正します。G01 が未完了なら、既存の作業ディレクトリからこれらのファイルをコピーしてから作業してください。
新しいランタイム `.ll` を追加するチケットは、`.gitignore` の例外行（`!src/runtime/xxx.ll`）も追加します。

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
| `src/runtime/wasm.ll` | 128-bit 乗除算・シフトの補助（`__multi3` は `noinline optnone` 必須） | WASM で driver が常に追加 |

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

---

## 8. ドキュメント更新規約

- 文書は日本語、コード識別子と診断メッセージは英語。
- 利用者向けの仕様は `docs/language.md`、コンパイラ内部の不変条件は `docs/architecture.md`、
  概要と実行例は `README.md`、性能は `docs/benchmarks.md`。
- 「未実装」「未対応」と書かれている箇所（例: `docs/architecture.md`「初版の次に必要な設計」、
  `docs/language.md` の各節末尾の未対応一覧、README 冒頭の未実装一覧）を実装に合わせて更新する。
- 実装していない最適化や将来計画を、実装済みのように書かない。
- 例を追加したら `examples/` に置き、`tests/examples.mjs` で実行されるようにする。

---

## 9. 設計決定台帳（チケット横断）

複数のチケットにまたがる決定です。各チケットはこの決定に従って書かれています。
実装中に不可能・不適切と判明した場合は、**黙って変えず**、台帳の該当行に「見直し提案」を追記して人間に確認してください。

### D-01 命名規約
- 型・共用体のケース・モジュール・型クラス: `PascalCase`。関数・フィールド・ローカル: `snake_case`（既存の `clone_string`、`to_float`、`bit_and` と同じ）。
- 標準ライブラリの関数は常に `Module.function` で呼ぶ（F# の `List.map` と同様）。例: `Option.map`、`Array.sum`、`String.split`。

### D-02 型適用の構文
- 型パラメーターと型引数は **`<...>` 内のカンマ区切り**:
  `Option<i64>`、`Result<i64, string>`、`Pair<'a, 'b>`、`[Option<i64>]`、`Option<Pair<i64, i64>>`。
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
  `%"tz.record.Main.Pair[i64,string]"`、`%"tz.union.Option.Option[Main.Pair[i64,string]]"`。
  正規表記は構造的・単射な規則で生成する（修飾名、型引数は `[` `]` と `,` で区切る。配列 `[T]` は `array[T]`、
  リスト `list[T]`、タプル `tuple[T,U]`、関数 `fn[T,U->R]`、参照 `ref[T]`／`refmut[T]`、タスク `task[T]`）。
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
  union Option<'a> = None | Some of 'a
  ```
  最初の `|` は省略可。各ケースのペイロードは `of T` で **ちょうど 0 個か 1 個の型**（複数値はタプル型 `f64 * f64` やレコード）。
- ケース名は大文字始まり。同じモジュール内のケース名・レコード名・共用体名は互いに重複不可（`E1001`）。
- 構築: ペイロードなしのケースは値（`Empty`）、ペイロードありは一引数の関数値（`Circle 1.0`、`Rect (1.0, 2.0)`、`Option.map Some x`）。
- 名前解決: 無修飾（自モジュール → プロジェクト内で一意 → 標準ライブラリ）、`Module.Case`、`Module.Union.Case`。
  ローカル変数・関数・アクティブパターンとの優先順位はチケット A02 の表に従う。
- パターン: `Circle r`、`Rect (w, h)`、`Empty`、`Option.Some x`。ペイロードは既存のパターンで分解する。
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
  さらに std のソースでは、他の std モジュールの名前を常に修飾して書く（`Option.tc` の中では `Result.Result`、`Result.Ok`）。
  関数は従来どおり他モジュールからは修飾必須（`Option.map`）。
- LLVM を必要としない言語機能（ビット演算 intrinsic など）は、`Task.run` と同じ **修飾名付きの組み込み関数**
  （`Builtin` に `"Int.popcount"` のような名前で登録）として std のモジュール名前空間に置く。
  `Module.name` の解決は「ソース定義の関数 → 同名の組み込み」の順。ただし **組み込み関数として提供する API は
  `Builtin` だけに置き、同じ修飾名の `def` を std のソースに書かない**（書くと組み込みが隠れる）。
  衝突はコンパイラのテストで検出する（E02）。組み込みのシグネチャは文脈で解決するテンプレート
  `BuiltinType`／`BuiltinScheme`（std の型 `Option` や型族 `UnsignedOf`／`WidenOf` を表せる）で定義し、
  `FunctionRef::Builtin(BuiltinInstance)` を唯一の参照形式にする（詳細は E02）。
  引数なしの関数は `fn() -> T` の値なので、`Math.pi()` のように呼び出して使う。
- 未使用の標準ライブラリ関数は IR に出力しない（到達可能性で間引く）。型検査は常に行う。
- 補助関数は `private`（D-09）にする。
- 標準ライブラリのモジュール名と担当チケット（新しい std モジュールはこの表に追記してから作る）:

  | モジュール | 内容 | 担当 |
  |---|---|---|
  | `Option`（`Option.tc`） | `Option` 型・関数・ビルダー | B01 |
  | `Result`（`Result.tc`） | `Result` 型・関数・ビルダー | B01 |
  | `Array` | 配列の一括操作・関数的更新 | C04、C01 |
  | `List` | 連結リストの操作 | C04 |
  | `Vec` | 伸縮可能な配列 | C02 |
  | `String` | 文字列操作 | D02 |
  | `Char` | 文字の変換・分類 | A08 |
  | `Math` | 浮動小数点の数学関数、FMA | D03、D05 |
  | `Int` | 整数 intrinsic | D04 |
  | `Debug` | デバッグ出力 | E07 |
  | `Parallel` | データ並列 API | F02 |
  | `Simd` | SIMD ベクトル型の操作 | F04 |
  | `Map`／`Set` | 順序付きの連想コンテナ | C06 |
  | `Test` | テスト用の比較・報告 | G06 |
  | `Gpu` | GPU 実行 API | F07 |

  組み込みクラス（`Display`、`Parse`、`Hash`、`Default`、`Elementary` など）は std モジュールに属さない組み込み名として予約する。
  `Elementary` は超越関数（`Math.sin` など）用のメソッドなしマーカークラスで、D03 では f32／f64 だけが満たす。

### D-08 Option と Result
- `std/Option.tc`: `union Option<'a> = None | Some of 'a`、関数（`map`、`bind`、`default_value`、`is_some`、`is_none` など）、
  コンピュテーション式の操作（`Bind`、`Return`、`ReturnFrom`、`Zero` など）。`Option { let! x = ...; return x }` が使える。
- `std/Result.tc`: `union Result<'a, 'e> = Ok of 'a | Error of 'e`、関数（`map`、`map_error`、`bind` など）、同様の操作。
- `.tc` はビルダー 1 個分の実装であり、共用体・レコード・補助関数・インスタンスを含められる（A02 で union を許可）。

### D-09 可視性
- キーワード `private`（新規予約語）を `def`／`record`／`union`／`type` の前に付けると、同じモジュール内だけから参照できる。
  既定は従来どおり公開。`export` とは独立（`private export` はエラー）。違反は `E1022`。

### D-10 エラー処理の方針
- プログラムの誤り（範囲外アクセス、ゼロ除算、`assert` 失敗）は従来どおりトラップ。
- **A03 の実装後**、ソースに書いた `match` と関数ガードは静的に網羅的でなければならず、網羅されなければ `E1021`。
  生成コードの不一致時トラップ（`match_expression` の失敗ブロック）は多重防御として残す。
  `for`・`fx`・コンピュテーション式の分解パターンの不一致は従来どおり実行時トラップ（A03 参照）。
- 予期できる失敗（解析、検索、変換）は `Option`／`Result` を返す。例外・巻き戻しは導入しない。
- 早期脱出の `?` 演算子は導入しない。伝播は `Option { }`／`Result { }` ビルダーで書く（B02）。

### D-11 表示と解析
- 組み込みクラス `Display<'a> { def display :: &'a -> string }`。数値・bool・unit・string・char に組み込みインスタンス。
- 組み込み関数 `to_string :: Display<'a> => 'a -> string`（値を受け取り、内部で借用して `display` し、値を解放する）。
- 組み込みクラス `Parse<'a> { def parse :: &string -> Option<'a> }`。数値・bool に組み込みインスタンス。
- 数値の文字列表現は、`to_string` とコンソール出力（`console_main`、`tz_soft_format`）で **同じ実装・同じ形式** にする。
- **決定（2026-09-23 承認）:** 二進浮動小数点（f16／f32／f64／f128）は、同じ型へ解析し直すと元の値に戻る
  **最短の十進表現**で表示する（例: `0.1` は `0.1`。現在の `%.17g` 相当の `0.10000000000000001` はやめる）。
  コンソール出力も同じ形式に変える（既存のテスト・文書の期待値を D01 で更新する）。NaN はすべて `nan`、負のゼロは `-0`。
  字句の詳細・指数表記の条件・decimal の扱いは D01 に従う。
- 組み込みクラス `Hash`（A07）と `Default`（A07）は deriving のチケットで定義する。

### D-12 文字型
- 型名 `char`（Unicode スカラー値 U+0000–U+D7FF, U+E000–U+10FFFF）。LLVM では `i32`。リテラル `'a'`、`'\n'`、`'\u{1F600}'`。
- 字句解析: `'` の後に 1 文字（またはエスケープ）と `'` が続けば文字リテラル、それ以外は従来の型変数 `'a`。
- Copy・Eq・Ord・Display。算術なし。整数との変換は `Char` モジュールの関数（`Char.to_u32`、`Char.of_u32 : i32u -> Option<char>`）。

### D-13 コレクションの更新・伸縮・部分参照
- **消費する関数的更新**: `Array.set : ['a] -> i64 -> 'a -> ['a]` のように所有値を受け取り新しい値を返す。
  所有権により唯一の所有者であることが保証されるため、実装はバッファをその場で書き換えてよい（観測できない最適化）。
- 伸縮可能な配列は組み込み型 `Vec<'a>`（C02）。`[T]` の記述子 `{ ptr, i64 }` は変更しない。
- 部分参照（スライス）は **`&[T]` そのもの**（C03）。`&xs[a..b]` で作る。`&mut [T]` は従来どおり配列全体の置換用。

### D-14 決定性と数値演算の順序
- 一括演算は評価・集計の順序を API 契約として文書化する。浮動小数点の集計は既定で左から右の逐次。
  別の順序（pairwise、Kahan、並列チャンク）は名前で区別した別 API にする（`Array.sum` と `Array.sum_pairwise`）。
- 並列処理の分割境界は入力長だけで決め、スレッド数に依存させない（どの機械でも同じ結果）。
- 整数の折り返し加算のように結合的な演算だけは、順序を変えても結果が同じなので SIMD・並列化してよい。

### D-15 キーワード・演算子の追加一覧
新しい予約語: `union`（A02）、`type`（A05）、`private`（E01）、`break`／`continue`（B03）、`deriving`（A07）、
`const`（D06）、`test`（G06）、`extern`（E06）。`of` は union 宣言の中だけの文脈キーワード（A02。予約語にしない）。
文字リテラル `'x'`（A08）。範囲の部分参照 `xs[a..b]`（C03）。
レコード更新 `{ base with field = value }`（C05）。キーワード追加時は 6.1 を実施する。
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
| `E2006` | テストの失敗（`tsuzuri test`） | G06 |
| `W1001` | 未使用のローカル束縛 | G03 |
| `W1002` | 未使用の非公開関数・型 | G03 |
| `W1003` | 到達しない `match` 節 | A03／G03 |
| `W1004` | 同じスコープ内での紛らわしいシャドーイング（既定は無効） | G03 |

既存のコード（`E1001`–`E1020`、`E2000`–`E2005`、`W2001`）は意味を変えずに使う。

### D-17 ABI とシンボル
- 公開 ABI（`tz_<name>`）の型の範囲は E05 まで変更しない。内部シンボルは `@tz.*`、外部ランタイムは `tsuzuri_*`。
- 追加の公開エントリー（例: WASM の確保関数）は `tz_` の名前空間を避けて `tsuzuri_` を使い、文書化する。

### D-18 WASM とオプトイン機能
- WASM の既定はインポートなし・逐次・SIMD なし。SIMD128（F03）、threads（F06）、ホストのインポート（E06）、
  デバッグ出力（E07）は明示的なオプションで有効にし、未指定時の出力は変えない。

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
- 組み込みインスタンス（数値・bool・unit・string・char）の生成コードは変えない（`icmp`／`fcmp`／`@tz.string.equal` のまま、
  参照の受け渡しを生成しない）。ユーザー／条件付きインスタンスだけ `Call(method, [&left, &right])` に下げる。
- 算術・ビット演算のクラス（`Add` など）は従来どおり値を受け取る（文字列の `+` は両辺を消費する）。
- 実装はチケット **A11**。A06（条件付き `Eq<['a]>` など）、A07（deriving）、C04（`Array.sort`、`contains` など）、
  C06（Map のキー比較）は A11 を前提にし、比較のためだけに `Copy` を要求しない。

### D-21 発散する組み込み関数 `unreachable`
- `unreachable : unit -> 'a` を組み込み関数として追加する（B01 で実装。E02 の多相 builtin 機構を使う）。
  呼ぶと必ずトラップする。`Option.get` のような任意型を返す部分関数を、**網羅的な `match`** で書くために使う
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
      pub provenance: Provenance,        // G03: User | Generated(GeneratedKind)
      pub parent: Option<usize>,         // E02: 生成関数（$lambda/$task/$builtin/$export/union 構築子など）の持ち主の関数 id
      pub test: Option<usize>,           // G06: test 宣言の本体、またはその本体からだけ生成された関数なら test の番号
  }
  ```
- 各チケットは自分の担当フィールドを追加する: E02 が `FunctionOrigin` を導入して `module` と `parent` を持たせ、
  G03 が `provenance` を、G06 が `test` を追加する（先に実装されたチケットがなければ、その時点で構造体を導入する）。
  生成関数は持ち主の `module`・`test` を継承し、`provenance` は `Generated(kind)` にする。
- 出力する関数の集合（到達可能性）の計算は一か所（E02 の `reachable_functions`）にまとめ、G06 の通常ビルド／テストビルドの
  切り替えは、その計算の根（root）の選び方として実装する。
- ローカル束縛の由来（G03 の未使用警告）は `Local` に `provenance: Provenance` を追加して表す。

---

## 10. 完了の定義（全チケット共通）

- [ ] 仕様どおりに動作し、仕様外の入力は安定した診断コードで拒否される。
- [ ] `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked` が通る。
- [ ] コード生成に関わる変更では、関連する `node tests/*.mjs target/release/tsuzuri` がすべて通り、
      native/WASM × `-O0`/`-O3`、解放追跡（`live == 0`）、WASM インポートなしを確認した。
- [ ] 生成 IR が決定的（同じ入力で 2 回ビルドして一致）で、intrinsic 宣言が重複しない。
- [ ] 既存のテスト・例・ベンチマークの `--quick` が壊れていない。
- [ ] `README.md`／`docs/language.md`／`docs/architecture.md`（必要なら `docs/benchmarks.md`）を更新し、
      「未実装」一覧から実装した項目を外した。
- [ ] 性能を主張する場合は実測と生成コードの確認結果を `docs/benchmarks.md` に記録した。
- [ ] `_features/README.md` の状態欄を更新し、チケットの「未決事項」に実装時の判断を追記した。

## 11. 小さいモデルで実装するときの追加指示

- 一度に変更するのは 1 段（例: 構文だけ）。段ごとに `cargo test --locked` を実行する。
- 既存の似た機能（レコード、タプル、`Task`、`NewLiteral`）の実装を必ず読み、同じ形で書く。新しい抽象化を発明しない。
- `unreachable!()` を追加する前に、型検査でその状態が本当に排除されているかを確認する。
- エラーを握りつぶしたり、テストの期待値をコンパイラの現在の出力に合わせて書き換えたりしない。
- 迷ったら、チケットの「未決事項」の既定案 → この台帳 → 最も保守的な選択（拒否）の順に従う。
