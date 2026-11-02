# TZ02: `tzroutine { }` 構文・`Routine` モジュール・終了の待ち

| 項目 | 内容 |
| --- | --- |
| ID | TZ02 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | TZ01 |
| 後続 | TZ03（`Channel`）、TZ04（`select`）、TZ05・TZ06・TZ07・TZ08・TZ09・TZ11 |
| 状態 | done（2026-11-02） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61）。決定事項 D1〜D8 は既定案 |
| 手本にする既存実装 | 構文: `src/parser.rs` の `task`（2916 行）と、文脈キーワード `try` の `try_clause_ahead`（3681 行）。型: `src/check.rs` の `ExprKind::Task` の扱い（10925 行の `self.lambda(&[], body, expected, span, LambdaKind::Task, &[])`）と `src/closures.rs` の `LambdaKind::Task`。生成: `src/llvm.rs` の `TypedExprKind::TaskRun`（5426 行。`closure_parts`）。組み込み: `Builtin::OwnedCall`（`src/check.rs` 3289 行、`src/llvm.rs` 7890 行）。ランタイムの連結: `src/driver.rs` の `task_runtime`（4119 行）と翻訳単位の組み立て（4673 行）。IR の後処理: `src/llvm.rs` の `with_stack_checks`（300 行） |
| 主な影響ファイル | `src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/parser.rs`・`src/check.rs`・`src/closures.rs`・`src/ownership.rs`・`src/llvm.rs`・`src/driver.rs`・`src/test_runner.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/kg_capture.rs`・`src/diagram/concurrency.rs`・`src/const_rules.rs`・`src/const_machine.rs`・`src/prove_smt.rs`・`src/capabilities.rs`・`src/stdlib.rs`・`src/formatter.rs`・`src/lsp.rs`・`src/runtime/tzroutine.c`・`tests/tzroutine.rs`（新規）・`tests/tzroutine.mjs`（新規）・`tests/fixtures/tzroutine/Main.tz`（新規）・`tests/features.mjs`・`vsc/syntaxes/tsuzuri.tmLanguage.json`・`vsc/src/core.ts`・`spec/rules/TSK.md`・`tests/conformance/TSK/`・文書 |

## 実装者向け早見

- 範囲: 利用者が tzroutine を書けるようにする。`tzroutine { 文 }`（文脈キーワード）、組み込みモジュール `Routine`（`go`・`yield`・`count`・`max_procs`・`cpu_count`）、`main`・`export`・`test`・`bench` の終了の待ち、ドライバーの連結と組み合わせの拒否。チャンネルは TZ03、`select` は TZ04。
- 最初に読む（この順）: ① D-61 ② 決定事項 D1〜D8 ③ `src/parser.rs` の `task`・`block`（2774〜2930 行）④ `src/closures.rs` 1〜200 行 ⑤ `src/llvm.rs` の `TypedExprKind::TaskRun` と `closure_parts`・`apply_value`（6486〜6510 行）⑥ TZ01 の仕様 1.2（呼ぶ C の関数）。
- 最初に書くテスト: `tests/tzroutine.rs` の `parses_tzroutine_as_a_contextual_keyword` → `spawns_and_waits_at_the_end_of_main`（E2E は `tests/tzroutine.mjs`）。
- 触ってはいけない: tzroutine を使わないプログラムの IR（byte 単位。`tests/llvm_handoff.rs` などの既存の IR の比較が守る）。`task { }` と `Task.*` の意味（TZ12 まで残る）。ランタイムの翻訳単位の既存の 9 つの部分の文字列（配布物の事前ビルドの照合が SHA-256 で一致を見る。手順 7）。
- 外から借りるもの: なし（TZ01 の `src/runtime/tzroutine.c`）。
- 完了の見分け方: `tsuzuri run` で次が `sum=4950` を出し、`live == 0`（E2E の確保の追跡）。最後の式の値は、終了の待ち（境界の待ち）より **前** に計算されるので、合計を読む前に子の完了を数えて待つ（このチケットには `WaitGroup` がない。TZ06 で `WaitGroup.wait` に置き換える）。

```tsuzuri
let total = Arc.new (Atomic.create 0i64)
let done = Arc.new (Atomic.create 0i64)
for i in 0 ..< 100 do
    let mine = Arc.share (ref total)
    let finished = Arc.share (ref done)
    tzroutine {
        let _sum = Atomic.fetch_add (Arc.get (ref mine)) (i as i64)
        let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64
    }
while Atomic.load (Arc.get (ref done)) < 100i64 do
    Routine.yield ()
$"sum={Atomic.load (Arc.get (ref total))}"
```

- 迷ったら: 決定事項 → D-61 → `task { }` の既存の挙動に合わせる → 最も保守的な選択（拒否）。

## 目的

Go の `go f(x)` に当たる `tzroutine { f x }` を言語に入れ、生成したプログラムを TZ01 のランタイムで走らせる。これで、待つ数がスレッド数に縛られない並行の処理（サーバーの接続ごとの処理、パイプライン、ファンアウト）を書ける土台ができる。チャンネル（TZ03）と `select`（TZ04）はこの上に載る。

非目標: チャンネル・`select`・同期・時間（TZ03〜TZ07）、ブロッキング呼び出しの引き継ぎ（TZ08。このチケットでは `extern` やファイルの I/O は P を持ったまま OS スレッドを止める）、WASM（TZ09。このチケットでは拒否）、Windows（TZ01 の Phase 2 が済むまで拒否）。

## 着手条件と停止条件

### 着手条件

- TZ01 の Phase 1 が done（`ls src/runtime/tzroutine.c` が成功し、`node tests/tzroutine_runtime.mjs` が通る）。
- 基準: `cargo nextest run --locked` の件数、`cargo build && node tests/features.mjs target/debug/tsuzuri` の結果、`node tests/tasks.mjs target/debug/tsuzuri`（既存の Task が変わらないことの基準）を控える。
- 共有の数を今の値から読む（GUIDE-LITE §11）: `grep -n "FRONTEND_FORMAT: u32" src/frontend_cache.rs`（起票時 15）、`grep -o 'out.tag("ExprKind", [0-9]*)' src/syntax_codec.rs | sort -t, -k2 -n | tail -1`（起票時 46）、`grep -n "Builtin::ALL.len()" src/effects.rs`（起票時 260）、`grep -n "RESERVED_MODULES.len()" src/stdlib.rs`（起票時 59）、`grep -n "CONTEXTUAL_KEYWORDS: \[" src/syntax.rs`（起票時 8）。

### 停止条件

- **S1** tzroutine を使わないプログラムの IR が 1 byte でも変わる（`cargo nextest run --locked` の IR の比較、`tests/llvm_handoff.rs`、`node tests/features.mjs` の既存の suite）。選択肢: 変更を `spawns(module)` が真のときだけに閉じ込め直す。
- **S2** `tzroutine { }` の本体の `do!`／`let!` が IO をその場で実行しない（`IO<unit>` の値になって `E1003` になる、またはビルダーを探して `E1018` になる）。本体を `Some(&Type::Unit)` で検査する経路（`direct_context`）が今の HEAD で別の形になっている。選択肢: A `src/computation.rs` の `direct_context`（1218 行）に `LambdaKind::Task` の本体を足す、B 止めて報告する。
- **S3** 配布物の事前ビルドのランタイム（`tests/prebuilt_runtime.mjs`）の照合が、tzroutine を使わないプログラムで外れる（翻訳単位の文字列が変わった）。
- **S4** `tests/tasks.mjs`・`tests/async.mjs`・`node tests/features.mjs` の `concurrency`・`channel` の suite が落ちる（既存の Task・Async を壊した）。
- **S5** 生成 IR が非決定的、`-O0` と `-O3` で結果が違う、新しい crate・`unsafe`・既定の WASM の import・予約語が要るように見える。
- **S6** 終了の待ちの後処理（手順 6）が、境界の関数の形（`entry:` と `ret` の行）を前提にできない IR を作る（複数の `ret` を含む関数のどこかを包み損ねる）。選択肢: 該当する関数の生成側で直接 `enter`／`leave` を出す。

## 現状（HEAD `9a76d8ac` で確認）

- `task` は予約語（`src/lexer.rs` 660 行の `"task" => TokenKind::Task`）で、`task { }` は `ExprKind::Task(Box<Expr>)`（`src/syntax.rs` 1153 行。codec の tag 17）。構文解析の `fn task`（`src/parser.rs` 2916 行）は `in_task = true` で `block()` を読み、本体の `let!`・`do!`・`return` を Task の bind として読む（`binding` 2994 行、`task_do` 2948 行、`task_return` 2927 行）。
- 文脈キーワードは字句では普通の識別子で、構文解析が文字列で見る（`try`: `primary` の 4379 行の `TokenKind::Ident(name) if name == "try" && self.try_clause_ahead()`）。一覧 `CONTEXTUAL_KEYWORDS: [&str; 8]`（`src/syntax.rs` 337 行。`finally`・`is`・`namespace`・`of`・`pure`・`try`・…）は、`@alias` の名前の検査（`src/parser.rs` 1426 行）と LSP の補完（`src/lsp.rs` 2357 行）が使う。
- **`tzroutine { ... }` は今、ビルダー名 `tzroutine` の計算式として読まれる**（`identifier_expression`、`src/parser.rs` 4596〜4609 行）。そのようなビルダーはないので型検査で誤りになる。
- 通常の関数・型注釈のあるラムダの本体では、`do!`／`let!` は IO をその場で実行する（`block()` の `implicit_computation_ahead` が `Computation("$implicit", …)` にし、型検査の `direct_context`（`src/computation.rs` 1218 行）が、期待する型にビルダーがなければ `IO.run` の直接実行に下ろす）。確認した例（HEAD のコンパイラ）:

```tsuzuri
let f: Owned.Function<unit, unit> = Owned.function (\() -> {
    do! IO.write_line "inside";
    ()
})
Owned.call (ref f) ()
42
```

  は `inside` と `42` を出す。期待する型がないと、本体は `IO<unit>` になり `Owned.call` で `E1003`（`expected unit, found IO<unit>`）。
- `task { }` の型検査は `self.lambda(&[], body, expected, span, LambdaKind::Task, &[])`（`src/check.rs` 10925 行）。`LambdaKind::Task` は、期待する型 `Task<r>` から結果の型を先に決め（`closures.rs` 36〜42 行）、捕捉に `Send` を要求し（124〜147 行）、型を `Type::Task(body.ty)` にする（152〜154 行）。下ろす段で、捕捉を引数にした関数として `"$task"` の module へ持ち上げる（`closures.rs` 361〜394 行、`is_task: true`）。
- 生成: Task の値は `%tz.closure = { ptr desc, ptr env }`。adapter `<ret> @tz.apply.$task.N.<count>(ptr %env)` が捕捉を読み、本体を呼び、`@tz.free(env)` する（1 回で消費する）。結果が `unit` の adapter は `void (ptr)`（`Type::Unit` の LLVM の型は `void`。`src/llvm.rs` 2958 行）。`Task.run` は `closure_parts` で `code` と `env` を取り出して `call {ty} {code}(ptr {env})`（5426 行）。
- ランタイムの連結: `RuntimeSymbols::collect(&text)` が IR の `@name` を集め、`task_runtime = native && contains("tsuzuri_task_parallel")`（`src/driver.rs` 4119 行）。翻訳単位は 9 つの部分を `"{}\n{}\n…"` で連結した文字列（4673 行）。`-pthread`（4733 行）、`-DTZ_STACK_GUARD`（4741 行。`stack_runtime` のとき）。`stack_runtime = native && exe && unix && has_recursion(text)`（4216 行）。
- 境界の関数: 実行ファイルの `@main`（`console_main`・`console_def_main`、`src/llvm.rs` 9141 行・9085 行）、ライブラリと IO の入口の `@tsuzuri_main`（`llvm_io::entry`、`library_def_main` 9108 行）、export の `@tz_<名前>`（`named_wrapper`、7350 行）、テストの `@tsuzuri_test_run`（1838 行）、ベンチの `@tsuzuri_bench_sample`（1817 行）。

## 変えてはいけない意味

- tzroutine を使わないプログラムの出力（IR・object・実行ファイル・import・ABI・ヘッダー）は 1 byte も変わらない。
- `task { }`・`Task.*`・`Async` の型・意味・生成コード（TZ12・TZ13 まで）と、旧 `Channel`（TZ03 で `DataChannel` に改名した後も意味は同じ）。
- 評価順序: `tzroutine { … }` を評価した時点で捕捉を移し（Copy の値は複製）、子を作ってから次の式へ進む。親の続きと子の本体の間に順序はない（Go と同じ）。
- 所有権: 捕捉は 1 回だけ使われる（子が 1 回だけ本体を走らせる）。本体が使わなかった捕捉は、本体の終わりに 1 回だけ drop される。

## 仕様

### 1. 構文

```text
tzroutine_expr := "tzroutine" block        (「tzroutine」の直後の字句が同じ行の「{」のときだけ)
```

- `tzroutine` は文脈キーワード（`CONTEXTUAL_KEYWORDS` に足す。予約語ではない）。直後が同じ行の `{` でなければ、普通の識別子（変数・関数の名前に使える）。
- `block` は通常のブロック（`{` 文 … `}`）。`let`・`let mut`・`let rec`・`use`・`while`・`for`・`match`・`if` を書ける。`let!`・`do!`・`use!` は IO をその場で実行する（関数の本体と同じ）。`return`・`return!` は `E0002`（既存の kind `return_outside_task_end`。今の文は `'return' is only allowed at the end of a task block` で、TZ12 で直す）。
- 式の位置ならどこにでも書ける。型は `unit`。

```tsuzuri
def greet :: i64 -> unit
fn greet n =
    tzroutine {
        do! IO.write_line $"hello {n}"
    }
```

- `@alias tzroutine` は `E0002` の既存の kind `builder_alias_is_contextual_keyword`（`'tzroutine' is a contextual keyword and cannot be a builder alias`）。

### 2. 型と所有権

| 規則 | 診断 |
| --- | --- |
| 本体の型は `unit`。ほかの型は `E1003`（`expected unit, found i64` など。位置は本体のブロック） | `E1003` |
| 捕捉する外の値は `Send`。`Rc`・Copy でない `dyn`・extern ハンドルの `Arc` などは `E1013`（既存の kind `task_requires_send`。文は今の `tasks require Send values; …`） | `E1013` |
| 捕捉は所有値のムーブか Copy の複製。借用（`ref`・借用した関数の環境）を捕捉すると `E1013`（`task_capture_retains_borrow`。`task captures cannot retain borrowed values, including borrowed function environments`） | `E1013` |
| 捕捉した値は、外では使えなくなる（ムーブ）。続けて使うと `E1012` | `E1012` |
| 外の `let mut` への代入は `E1014`（捕捉は不変） | `E1014` |
| 本体の中の `break`・`continue` で外のループへ出ることはできない（`E1023`） | `E1023` |
| `pure def` の中では書けない（A19 の `Category::Task`。既存の診断） | 既存 |
| `const` の初期化、GPU のカーネル、`Domain.run` の本体では書けない（それぞれ既存の Task と同じ診断） | 既存 |

- 結果は `unit` なので、`task { }` の「結果は所有値」の検査（`TaskResultIsReference`）は常に通る。
- 本体の中の `ref` の借用は、本体の中の値（捕捉して移した値を含む）に対してなら自由（`task { }` と同じ）。

### 3. 実行の意味

- `tzroutine { body }` を評価すると: 捕捉を環境へ移し、TZ01 の `tsuzuri_tz_go` で子を作り、`()` を返す。子の本体はどこかの M で走る。
- 子の本体の中のトラップは、ほかのトラップと同じくプロセス全体の失敗（`--trap-mode abort`）。報告は、トラップした tzroutine の OS スレッドから標準エラーへ（`--trap-info` の位置を含む）。
- **終了の待ち（D-61 の決定 1）**: 次の境界は、その中で生まれた tzroutine（孫を含む）がすべて終わるまで戻らない。待つ間は境界のスレッド（擬似 G）が眠る。

| 境界 | いつ待つか |
| --- | --- |
| 実行ファイルの `main`（トップレベルのコード、`def main`、IO の入口） | 入口の関数が戻った直後。トップレベルの最後の式の値を表示する前 |
| `export def` の 1 回の呼び出し（`tz_<名前>`） | 本体が戻った直後、ホストへ戻る前 |
| `test` の 1 件（`tsuzuri test`）、`bench` の 1 回の計測（`tsuzuri bench`） | その 1 件・1 回の本体が戻った直後 |

- 全員が待ちに入れば、TZ01 のデッドロックの判定（`Tsuzuri runtime: all tzroutines are asleep - deadlock! (…)`）でトラップする（境界の待ちの理由は `end of scope (waiting for tzroutines)`）。
- 実行時の失敗の文（ランタイムが標準エラーへ書く）: スタックが取れない `Tsuzuri runtime: cannot allocate a tzroutine stack (out of memory or address space)`。どちらも `abort()`。

### 4. `Routine` モジュール（組み込み。std のファイルはない）

| 関数 | 型 | 意味 | Go |
| --- | --- | --- | --- |
| `Routine.go` | `own Owned.Function<unit, unit> -> unit` | 所有する関数を新しい tzroutine で 1 回呼び、そのあと drop する。`tzroutine { }` の関数版（関数を値として受け取る高階の関数のため） | `go f()` |
| `Routine.yield` | `unit -> unit` | 実行を譲る | `runtime.Gosched` |
| `Routine.count` | `unit -> i64` | 生きている tzroutine の数 + 1（呼んだスレッドの分） | `runtime.NumGoroutine` |
| `Routine.max_procs` | `unit -> i64` | 同時に tzroutine を走らせる OS スレッドの数（`TSUZURI_MAXPROCS`、既定は CPU 数） | `runtime.GOMAXPROCS(0)` |
| `Routine.cpu_count` | `unit -> i64` | オンラインの CPU 数 | `runtime.NumCPU` |

- `Routine` は予約されたモジュール名（`RESERVED_MODULES` に足す）。`record Routine`、`Routine.tz` は `E1011`（既存の予約名の扱い）。
- `Routine.go` の `Owned.Function` は、捕捉を借りて呼ぶ（本体は捕捉をムーブできない。`Owned.function` の既存の規則 `MoveOutOfOwnedFunction`）。捕捉をムーブしたいなら `tzroutine { }` を使う。
- `yield` は予約語だが、`Routine.yield` のようにモジュールの後ろでは名前として書ける（既存の `Async.yield` と同じ。`src/parser.rs` 1690 行）。
- 関数値として渡せる（`Routine.yield` を `unit -> unit` の引数へ）。`Routine.go` を関数値にするのも可。

### 5. ターゲットと組み合わせ

| 構成 | 結果 |
| --- | --- |
| native の macOS・Linux（aarch64・x86-64） | 動く |
| native の Windows | TZ01 の Phase 2 が済むまで `E2002`（kind `toolchain_missing_feature`、`tzroutine is not supported on Windows targets yet`）。Phase 2 のあとはこの行を消す |
| wasm32・wasm64（`jspi` なし） | `E2000`（kind `option_requires_option`、`tzroutine on WebAssembly needs --wasm-feature jspi`） |
| wasm32 + `--wasm-feature jspi` | TZ09 まで `E2000`（kind `option_conflict`、`tzroutine with --wasm-feature jspi is not implemented yet`）。TZ09 がこの行を消す |
| `--freestanding` | `E2000`（既存の一覧に `("declare void @tsuzuri_tz_go(", "tzroutine")` を足す。文は既存の `--freestanding cannot use tzroutine: its runtime needs the C library; …`） |
| `--trap-mode return` | `E2000`（`option_conflict`、`--trap-mode return cannot be combined with tzroutine: a trap in one tzroutine cannot be returned from the call that spawned it`） |
| `--wasm-feature threads` | `E2000`（`option_conflict`。wasm の拒否の前に出る） |
| `--emit llvm`・`header` | 拒否しない（ホストがランタイムを連結する。`--emit llvm` の IR は `tsuzuri_tz_*` を宣言する） |
| `--backend fast` | `task.c` と同じ扱い（ランタイムの C は clang で別に作る）。fast の backend が C のランタイムを扱えないなら、既存の Task と同じ拒否を同じ kind で足す（手順 7 で確認） |

- **判定に使う条件**: ランタイムを連結するかは `tzroutine_runtime = runtime_symbols.prefix("tsuzuri_tz_")`（TZ03 以降のチャンネルだけを使うプログラムも、ランタイムが要る）。**拒否の判定**は、Windows・WASM・`--freestanding`・`--wasm-feature threads` は `tzroutine_runtime`（TZ03 の後は、tzroutine を生成しないプログラムの既定の WASM を除く）、`--trap-mode return` だけは「tzroutine を生成する」こと（IR が `@tsuzuri_tz_go(` を呼ぶ。TZ05・TZ06・TZ11 は、生成する C の関数 `tsuzuri_tz_timer_func`・`tsuzuri_tz_waitgroup_go`・`tsuzuri_tz_scope_run` をこの一覧に足す）。チャンネルや `Mutex` だけを使うプログラムは、`--trap-mode return` でも拒否しない（TZ12 で `Mutex` が全プログラムでこのランタイムを使うため）。`spawns = spawns(module)`（下の設計）は終了の待ちを入れるかどうかに使う。
- Task との共存（TZ12 まで）: 拒否しない。tzroutine の中で `Task.run`・`Task.parallel`・旧 `Channel`（TZ03 からは `DataChannel`）の待ち（TZ12 まで）を使うと、その M の OS スレッドを止める（P を持ったまま）。`task { }` の中で `tzroutine { }` は書ける（Task のワーカーのスレッドは擬似 G になる）。

## 設計

### 構文と AST

| ファイル | 変更 |
| --- | --- |
| `src/syntax.rs` | `ExprKind::Routine(Box<Expr>)`（`Task` の直後）。`visit_mut`（855 行付近）と `visit`（995 行付近）に `Task` と同じ腕。`CONTEXTUAL_KEYWORDS` に `"tzroutine"`（配列の長さを 1 増やす。並びはアルファベット順を保つ） |
| `src/syntax_codec.rs` | `ExprKind::Routine` の tag を「今の最大 + 1」（起票時 47）。put と get の両方。テストの `("ExprKind", N)` の件数（`seen` の数を比べるテスト）を 1 増やす |
| `src/frontend_cache.rs` | `FRONTEND_FORMAT` を 1 増やす（起票時 15 → 16） |
| `src/parser.rs` | `primary` の `TokenKind::Ident(name) if name == "try" …` の前に `TokenKind::Ident(name) if name == "tzroutine" && self.tzroutine_ahead() => return self.tzroutine(),`。`tzroutine_ahead`: 次の字句が `{` で、2 つの字句の間に改行がない（`try_clause_ahead` の改行の検査と同じ書き方）。`fn tzroutine`: `task` と同じ形で、`in_task` を **false** にして `block()` を読み、`ExprKind::Routine` を作る |
| `src/formatter.rs` | `expression`（983 行付近）に `Routine(body)` → `tzroutine ` + ブロック（`Task` の `task ` と同じ） |
| `src/warnings.rs`・`src/computation.rs`・`src/regions.rs`・`src/contracts.rs`・`src/api_diff.rs`・`src/semantic.rs` | `ExprKind::Task` の腕がある match（`grep -rn "ExprKind::Task\b" src/`）に、同じ扱いの `ExprKind::Routine` の腕 |

### 型検査と下ろし

| ファイル | 変更 |
| --- | --- |
| `src/check.rs` | `ExprKind::Routine(body)` を `composed_expression`（10741 行）へ送り、`Task` の腕（10925 行）の隣で: `let closure = self.lambda(&[], body, Some(&Type::Task(Box::new(Type::Unit))), span, LambdaKind::Task, &[])?;` → 期待する型があれば `self.same(&Type::Unit, expected, span)?` → `TypedExpr { ty: Type::Unit, kind: TypedExprKind::RoutineGo(Box::new(closure)), span }`。`TypedExprKind::RoutineGo(Box<TypedExpr>)` を足し、`children`（4336 行）・`try_for_each_child`（4438 行）・`try_for_each_child_mut`（4592 行）に `TaskRun` と同じ腕 |
| `src/check.rs`（組み込み） | `Builtin::{RoutineGo, RoutineYield, RoutineCount, RoutineMaxProcs, RoutineCpuCount}`。`Builtin::ALL`・`name`（`"Routine.go"` など）・`scheme`（`RoutineGo` は `[Std { module: "Owned", name: "Function", args: [Unit, Unit] }] -> Unit`、ほかは `[Unit] -> Unit` か `[Unit] -> Integer(64, true)`）。`Builtin::spawns(self) -> bool`（`RoutineGo` だけ真。TZ06・TZ07・TZ05 が足す） |
| `src/closures.rs` | 変更なし（`LambdaKind::Task` をそのまま使う。持ち上げた本体は `"$task"` の module、`is_task: true`） |
| `src/ownership.rs` | `eval_value` の `E::TaskRun …` の腕（3096 行）に `E::RoutineGo` を足す（値を消費する）。`closed()`（585 行）にも |
| `src/stdlib.rs` | `RESERVED_MODULES` に `"Routine"`。件数のテスト（662 行）を 1 増やす |
| `src/polymorph.rs` | `Checker::builtin`（組み込みの特殊化）に新しい組み込みの腕が要るなら `OwnedCall` と同じ所に足す（`grep -n "OwnedCall" src/polymorph.rs`） |

### 生成

- `TypedExprKind::RoutineGo(closure)`: `let closure = self.expression(closure); let (code, env) = self.closure_parts(&closure);` → `call void @tsuzuri_tz_go(ptr {code}, ptr {env})` → 値は `unit`。`code` は `void @tz.apply.$task.N.<count>(ptr %env)`（`unit` の結果は `void`）なので、TZ01 の `void (*)(void *)` と一致する。
- 宣言 `declare void @tsuzuri_tz_go(ptr, ptr)` などは、ほかのランタイムの宣言と同じく `intrinsics`（`BTreeSet`）へ入れる（重複しない）。
- `Builtin::RoutineGo`（関数版）: 1 つだけの補助関数を `intrinsics` へ入れる:

```llvm
define internal void @tz.tzroutine.owned(ptr %box) {
entry:
  %closure = load %tz.closure, ptr %box
  %code.desc = extractvalue %tz.closure %closure, 0
  %code = load ptr, ptr %code.desc
  %env = extractvalue %tz.closure %closure, 1
  call void %code(i8 0, ptr %env, i1 true)
  call void @tz.closure.drop(%tz.closure %closure)
  call void @tz.free(ptr %box)
  ret void
}
```

  呼び出し側は `%box = call ptr @tz.alloc(i64 16)` に `Owned.Function` の closure（`extractvalue {owned} %arg0, 0`）を store して `call void @tsuzuri_tz_go(ptr @tz.tzroutine.owned, ptr %box)`。`apply_value` の呼び方（`i8 0` の unit の引数、`ptr env`、`i1 borrowed`）は `Owned.call` の生成（`src/llvm.rs` 7890 行）と同じで、`apply_value` の今の形に合わせる（上の LLVM は起票時の形の例。`apply_value` が出す呼び出しの形をそのまま使う）。`@tz.closure.drop` の名前と形は `src/runtime/closure.ll` を確かめる。
- `RoutineYield`・`Count`・`MaxProcs`・`CpuCount`: `call void @tsuzuri_tz_yield()`、`call i64 @tsuzuri_tz_count()` など。
- **終了の待ち**: `spawns(module)`（`module` のすべての関数の本体に `TypedExprKind::RoutineGo` か `Builtin::spawns()` が真の組み込みの参照があるか。`try_for_each_child` で歩く。結果を `emit_with_options` の最初に 1 回求める）が真のときだけ:
  - `with_tzroutine_scopes(text, &boundaries) -> String`（新規。`with_stack_checks` と同じ IR の文字列の後処理）: `boundaries` の各関数（`define … @name(`で始まる定義）の `entry:` の直後に `%tz.scope = call ptr @tsuzuri_tz_scope_enter()` を、その関数の中の各 `ret` の行の直前に `call void @tsuzuri_tz_boundary_leave(ptr %tz.scope)` を入れる。`boundaries` は `tsuzuri_main`・`tsuzuri_test_run`・`tsuzuri_bench_sample`・すべての export の `tz_<名前>`。
  - `@main`（`console_main`・`console_def_main`）は生成側で直接出す（最後の式の値を表示する前に待つため）: 入口の関数の呼び出し（`%result = call … @tz.fn.<main>(…)`）の直後に `leave`。`@main` が `@tsuzuri_main` を呼ぶ形（IO の入口）では `@main` には入れない（`tsuzuri_main` が包む）。
  - `@tsuzuri_test_run` の `bad:` の `ret i32 2` の前にも入る（スコープは空なので即座に戻る）。
- ランタイムに 1 つ足す（`src/runtime/tzroutine.c`、TZ01 の仕様 1.2 の表にも足す）:

```c
/* The end of a boundary (main, an export call, a test, a bench sample): waits for the scope's
 * tzroutines. A deadlock is reported by tsuzuri_tz_scope_leave's detector; the boundary has no
 * source position of its own, so it traps here. */
TZR_API void tsuzuri_tz_boundary_leave(void *scope) {
    if (tsuzuri_tz_scope_leave(scope)) __builtin_trap();
}
```

  `tests/tzroutine_runtime.c` に、`tsuzuri_tz_boundary_leave` が空のスコープで即座に戻ることの検査を足す（`boundary` のケース）。

### ドライバー

| 箇所 | 変更 |
| --- | --- |
| `src/driver.rs` 4119 行付近 | `let tzroutine_runtime = options.target == Target::Native && runtime_symbols.prefix("tsuzuri_tz_");`。`native_runtime` に `\|\| tzroutine_runtime` |
| 4216 行（`stack_runtime`） | `&& (llvm::has_recursion(&text) \|\| tzroutine_runtime)`（tzroutine のスタックは再帰がなくても溢れうる。M が sigaltstack を持つため） |
| 翻訳単位（4673 行） | 既存の 9 つの部分の文字列は変えない。`tzroutine_runtime` のときだけ、末尾に `"\n"` と `include_str!("runtime/tzroutine.c")` を足す（**空の部分を足さない**。事前ビルドの照合のため。S3） |
| `-pthread`（4733 行） | 条件に `tzroutine_runtime` |
| 組み合わせの拒否 | 仕様 5 の表。`--freestanding` の一覧（4161 行）、`--trap-mode return`・`threads` は `BuildOptions::validate` ではなく IR を見たあと（Async の `host_entries` の検査、3784〜3805 行と同じ所）。Windows の `E2002` は `platform.windows()` |
| `src/test_runner.rs` | ネイティブのテストの実行器のランタイムの組み立て（`native_runtime_sources`、611 行・954 行付近）にも同じ条件で `tzroutine.c` を足し、`-pthread`。WASM のテストの実行器は tzroutine を `E2000`（`option_requires_option`、仕様 5 と同じ文） |
| `scripts/check-runtime-includes.sh` | `include_str!("runtime/tzroutine.c")` が数えられる（変更なしで通るはず。確認する） |

### 効果・事実・ツール

| ファイル | 変更 |
| --- | --- |
| `src/effects.rs` | `builtin_facts`: `RoutineGo` → `facts(Some(Task), alloc = true, callbacks = true)`（Owned.Function を呼ぶ）、`RoutineYield`・`Count`・`MaxProcs`・`CpuCount` → `facts(Some(Task), alloc = false)`。`node` の `K::RoutineGo` → `Task` + `Alloc("tzroutine")`（`K::TaskRun` の腕の隣）。件数の pin（`Builtin::ALL.len()`、`by(Effect::Task)`、不純の数）を今の値から +5 |
| `src/pure_rules.rs` | `builtin_effect` で `Routine*` → `Category::Task`。`scan_body` の `K::RoutineGo` → `None`（`K::TaskRun` と同じ）。件数の pin |
| `src/kg/effects.rs` | `builtin_effect` で `Routine*` → `"task"`。`K::RoutineGo` → `"task"`。`IMPURE_BUILTINS` を +5 |
| `src/kg_capture.rs` | `K::RoutineGo` → `calls` の辺を `"Routine.go"` へ（`TaskRun` の `"Task.run"` と同じ形。1070 行） |
| `src/diagram/concurrency.rs` | 並行の区間を作る呼び出し先（100〜108 行）に `"Routine.go"` を足す |
| `src/const_rules.rs`・`src/const_machine.rs`・`src/prove_smt.rs` | `K::TaskRun` を拒否する腕に `K::RoutineGo` を足す（同じ診断） |
| `src/capabilities.rs` | `Routine*` は権限なし（`PRIVATE_WITHOUT_CAPABILITY` ではなく、公開の組み込みの表。`tests/fixtures/capabilities/std-table.txt` の期待を直す必要があるか確認） |
| `src/lsp.rs` | `KEYWORDS` は変えない（`tzroutine` は `CONTEXTUAL_KEYWORDS` から補完に出る） |
| `vsc/syntaxes/tsuzuri.tmLanguage.json` | 文脈キーワードの規則（`try`・`pure` と同じ所）に `tzroutine` |
| `vsc/src/core.ts` | 予約モジュールの集合（133 行）に `Routine` |

## 前提とする他チケットのインターフェース

- TZ01 の仕様 1.2 の関数（`tsuzuri_tz_go`・`yield`・`count`・`max_procs`・`cpu_count`・`scope_enter`・`scope_leave`）。名前と型をそのまま使う。

## 他チケットへ提供するインターフェース

- `ExprKind::Routine`・`TypedExprKind::RoutineGo`・`Builtin::spawns()`・`spawns(module)`・`with_tzroutine_scopes`・`tzroutine_runtime`（ドライバーの条件）。TZ03 以降は、新しい組み込みが子を作るなら `Builtin::spawns` を真にする（TZ05 の `Time.after_func`、TZ06 の `WaitGroup.go`、TZ07 の `ErrGroup.go`）。
- `tests/tzroutine.mjs`（E2E の土台）。TZ03 以降は suite を足す。

## 実装手順

1. **変更**: ブランチ `feat/tz02-tzroutine-spawn`。基準を控える（着手条件）。**確認**: 件数のメモ。
2. **変更**: 構文（設計の「構文と AST」）。**内容**: `ExprKind::Routine`、codec、`FRONTEND_FORMAT`、`CONTEXTUAL_KEYWORDS`、`primary`、`fn tzroutine`、formatter、AST の歩き。型検査はまだ足さない（`ExprKind::Routine` を `composed_expression` で `unreachable!` にしない: 一時的に `Err(Diagnostic::of(DiagnosticKind::UnsupportedExpression…))` のような既存の拒否にする。既存の kind を `grep -n "Unsupported" src/diagnostic_kinds.rs` で選ぶ）。**確認**: `tests/tzroutine.rs` の `parses_tzroutine_as_a_contextual_keyword`（`tsuzuri::parse` 相当の関数で `tzroutine { }` が `ExprKind::Routine`、`let tzroutine = 1` と `tzroutine + 1` が識別子、`tzroutine\n{}` が識別子とブロック）と `formats_tzroutine`（`tsuzuri fmt` が `tzroutine {` の形を保つ）。`cargo nextest run --locked`（codec の件数のテストを含む）。
3. **変更**: 型検査（設計の「型検査と下ろし」の `ExprKind::Routine` の部分）。**内容**: 仕様 2 の表。**確認**: `tests/tzroutine.rs` に受理と拒否（`accepts`／`rejects` の型。GUIDE-LITE §5）: 受理 `tzroutine { () }`・本体の `do! IO.write_line "x"`・`let s = "a"; tzroutine { let _n = s.length }`・`Arc` の捕捉。拒否 `tzroutine { 1 }` → `E1003`、`let r = ref x; tzroutine { let _y = deref r }` → `E1013`、`Rc` の捕捉 → `E1013`、捕捉後の使用 → `E1012`、外の `let mut` への代入 → `E1014`、本体の `break` → `E1023`、`return` → `E0002`、`pure def` の中 → A19 の既存のコード。各拒否は診断コードを照合する。
4. **変更**: 組み込み `Routine.*` と `RESERVED_MODULES`（設計）。**内容**: 仕様 4。**確認**: 受理 `Routine.yield ()`・`let n = Routine.count ()`・`Routine.go (Owned.function (\() -> ()))`・関数値 `let f: unit -> unit = Routine.yield`。拒否 `record Routine { x: i64 }` → 既存の予約名のコード。`honors_the_exact_specialization_limit`（`src/polymorph.rs`。GUIDE-LITE §10）が通る。
5. **変更**: 生成（`RoutineGo`、組み込み、宣言）と `tsuzuri_tz_boundary_leave`（ランタイム）。**内容**: 設計の「生成」。終了の待ちはまだ入れない。**確認**: `tests/tzroutine.rs` の IR の検査: `tzroutine { }` の IR が `call void @tsuzuri_tz_go(ptr @tz.apply.$task.` を含み、2 回の `llvm::emit` が同じ（決定的）。tzroutine のない fixture（`tests/fixtures/tasks` など既存の 5 つ）の IR が手順 1 と byte 単位で同じ（S1）。`node tests/tzroutine_runtime.mjs` に足した `boundary` が通る。
6. **変更**: 終了の待ち（`spawns`・`with_tzroutine_scopes`・`console_main` の待ち）。**内容**: 仕様 3 の表。**確認**: IR の検査（`@main`・`@tsuzuri_main`・`@tz_f`・`@tsuzuri_test_run` のそれぞれに `@tsuzuri_tz_scope_enter` が 1 回、`ret` の数だけ `@tsuzuri_tz_boundary_leave`）。tzroutine のないプログラムには 1 つも入らない。
7. **変更**: ドライバーとテストの実行器（設計の「ドライバー」）。**内容**: 連結・拒否。**確認**: `cargo build` のあと、
   - `tests/tzroutine.mjs` を新規に作り（`tests/tasks.mjs` の形）、`tests/fixtures/tzroutine/Main.tz` を native の `-O0`／`-O3` で `tsuzuri run` し、期待の出力を照合する（fixture は下の「テスト計画」）。
   - 拒否: `--target wasm32` → `E2000`（kind `option_requires_option`）、`--wasm-feature jspi` → `E2000`（`option_conflict`）、`--freestanding --emit object` → `E2000`、`--trap-mode return --emit object` → `E2000`（`tsuzuri build --json` の kind を照合）。
   - `node tests/prebuilt_runtime.mjs target/debug/tsuzuri`（既存）が通る（S3）。
   - `sh scripts/check-runtime-includes.sh` が通る。
8. **変更**: 効果・事実・ツール（設計の表）。**内容**: 件数の pin を今の値から更新。**確認**: `cargo nextest run --locked`。`node tests/features.mjs target/debug/tsuzuri`（既存の全 suite。S4）、`node tests/tasks.mjs target/debug/tsuzuri`、`node tests/async.mjs target/debug/tsuzuri`。
9. **変更**: `tests/features.mjs` に suite `tzroutine`（`tests/fixtures/tzroutine` の export を native で。`live == 0` の検査は suite の既存の仕組み）。WASM の行は TZ09 まで飛ばす（suite の定義で native だけにする方法は、`features.mjs` の既存の native だけの suite を `grep -n "native" tests/features.mjs` で探して同じ形にする。なければ止めて報告）。**確認**: `node tests/features.mjs target/debug/tsuzuri tzroutine`。
10. **変更**: 規則 ID と適合性テスト、文書（「ドキュメント」）。**確認**: `node scripts/check-docs.mjs <変更したページ>`、`node scripts/check-spec.mjs --require-markers`、`node scripts/conformance.mjs`（`TSK` の領域）、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。
11. **変更**: T2 と CI。**確認**: `cargo fmt`・`clippy`・`nextest`、手順 7〜10 のコマンド、PR に `ci:full`（`src/runtime/` と `vsc/` を変えたため）。`gh run watch`。

## テスト計画

### Rust（`tests/tzroutine.rs`、新規）

| テスト | 内容 |
| --- | --- |
| `parses_tzroutine_as_a_contextual_keyword` | 手順 2 |
| `formats_tzroutine` | 手順 2 |
| `checks_the_body_and_the_captures` | 手順 3 の受理と拒否（コードを照合） |
| `tzroutine_module_functions` | 手順 4 |
| `emits_a_spawn_and_declares_the_runtime_once` | `@tsuzuri_tz_go` の宣言が 1 つ（`llvm::duplicate_declarations` が空）、IR が決定的 |
| `programs_without_tzroutine_keep_their_ir` | 手順 5 の S1 の検査（既存の fixture を 5 つ選び、`llvm::emit` が手順 1 の時点で保存した IR と一致。保存した IR を test の中に置かず、`git show <基準のコミット>` で作った文字列と比べるのではなく、**同じソースの 2 つの経路**（`spawns` を強制的に偽にした emit と通常の emit）を比べる） |
| `wraps_every_boundary` | 手順 6 |
| `rejects_unsupported_targets` | 手順 7 の拒否（CLI を `env!("CARGO_BIN_EXE_tsuzuri")` で呼ぶ。`tests/api_diff.rs` の形） |

### E2E（`tests/tzroutine.mjs`、新規。native の `-O0`・`-O3`）

`tests/fixtures/tzroutine/Main.tz` に置く関数（`export def`）と期待値:

| export | 内容 | 期待 |
| --- | --- | --- |
| `sum_atomic n` | 早見の例（`n` 個の tzroutine が `Atomic` に足し、完了の数を `Routine.yield` のループで待ってから読む） | `n(n-1)/2`（`n` = 0・1・100・10000） |
| `nested depth` | 深さ `depth` の 2 分木の tzroutine（各自が `Atomic` に 1。根が全体の数 `2^(depth+1) - 1` まで待つ） | `2^(depth+1) - 1`（`depth` = 0・10） |
| `moves_owned` | `string` と `[i64]` を捕捉して長さを `Atomic` へ | 所有値の長さの和、`live == 0` |
| `drops_unused_capture` | 使わない捕捉（`Drop` を実装した利用者の型。drop で `Atomic` を増やす） | drop が 1 回 |
| `yields count` | `count` 個が `Routine.yield` をはさんで足す | 合計 |
| `go_owned` | `Routine.go (Owned.function (…))` | 1 回だけ呼ばれ、drop される |
| `counts` | `Routine.count ()` が 1（自分だけ）、`max_procs ()` ≥ 1、`cpu_count ()` ≥ 1 | 1 |

- 実行ファイルの検査（`tsuzuri run`）: トップレベルが `tzroutine { do! IO.write_line "child" }` と最後の式 `42` → 出力は `child` の行のあとに `42`（待ってから表示する）。`TSUZURI_MAXPROCS=1` と `4` の両方。
- デッドロック: `tzroutine { … }` の子が `Mutex` で…（TZ06 まで同期がないので、このチケットでは「`Routine.yield` を続ける子」ではなく、**TZ03 で**チャンネルを使って足す。このチケットのデッドロックの E2E はない。TZ01 の C のテストが判定を検査済み）。
- トラップ: 子の中の `assert false` → プロセスが失敗し、標準エラーに `trap: assertion failed`（`--trap-info` の `tsuzuri run` なら位置）。
- 確保の追跡: 全ケースで `live == 0`（`features.mjs` の suite に入れる。手順 9）。export の値は境界の待ちの前に返すので、どの export も、値を読む前に子の完了を `Atomic` で数えて待つ（数えずに読むと結果が実行ごとに変わる）。

### 既存テストへの影響

- `src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/stdlib.rs` の件数の pin（今の値から +5、+1）。
- `tests/fixtures/capabilities/std-table.txt`（組み込みの表に行が増える場合）。
- syntax_codec の `("ExprKind", N)` の件数。

## ドキュメント

- 新規 `_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`（「tzroutine 式」）: この記事のポイント、基本の書き方（`run=` の例: 早見の `sum=4950`、`child` と `42`。最後の式の値は待ちの前に計算されることを、この 2 つの例の違いで説明する）、捕捉と所有権（`E1013`・`E1012`・`E1014` の例は `text` のブロック）、終了の待ち（D-61 の決定 1、Go との違いと理由、Go から来た人への注意: 終わらない tzroutine を残すと境界が待ち続ける。止め方の `Context` は TZ07 が書き足す。このチケットでは実装済みのものだけを書く）、スケジューラーの説明（P・`TSUZURI_MAXPROCS`・`TSUZURI_TZROUTINE_STACK_KIB`、1 MiB のスタックと Linux の mapping の上限）、デッドロックの判定の文、ターゲット（仕様 5 の表）、Task との違いの表（TZ12 まで）、他の言語との比較（Go の `go`、Rust の `std::thread::spawn`、Kotlin の `launch`、C# の `Task.Run`）、関連項目。
- 新規 `_tsuzuri/language-reference/built-in-types-and-modules/routine.md`（`Routine` モジュール。API の表、`yield` の例）。
- `_tsuzuri/language-reference/index.md`（目次の「非同期・タスク・遅延」に tzroutine 式、組み込みの型とモジュールに Routine）。
- `values-and-functions/keywords.md`（文脈キーワードの表に `tzroutine`）。
- `compiler/option.md`（`--freestanding`・`--trap-mode return`・`--wasm-feature` の拒否の行）、`compiler/platforms.md`（Windows・WASM の未対応）、`compiler/diagnostics.md`（`E2000`・`E2002` の行。生成の表は `node scripts/gen-docs.mjs`）。
- `docs/language.md`: 新しい節「tzroutine」（構文・型・終了の待ち）。`docs/architecture.md`: TZ01 の節に「生成コードとの接続（TZ02）」。
- `spec/rules/TSK.md`: 表の題を `# TSK: Tasks and tzroutines` にし、`TSK-011`〜`TSK-015` を足す（例: `TSK-011` `tzroutine { }` MUST return before its body finishes and the body MUST run exactly once.、`TSK-012` A boundary MUST NOT return before every tzroutine spawned in it has finished.、`TSK-013` A tzroutine MUST NOT capture a borrow (E1013).、`TSK-014` The body of a tzroutine MUST have type `unit` (E1003).、`TSK-015` `let!` and `do!` in a tzroutine body MUST run the IO action at that point.）。各規則の `<!-- rule: TSK-01x -->` を `tzroutine.md` に置き、`tests/conformance/TSK/TSK-01x-*.tz` を足す（TSK-001 の形。`// expect: stdout "…"` か `// expect: error E1013`）。
- `README.md`: 機能一覧（並行処理の行）と検証の表（`node tests/tzroutine.mjs`）。

## 受け入れ条件

- [ ] 仕様 1〜5 のとおりに動き、拒否は表のコードと kind で出る。
- [ ] tzroutine を使わないプログラムの IR・事前ビルドの照合が変わらない（S1・S3）。
- [ ] `tests/tzroutine.rs`・`tests/tzroutine.mjs`・`features.mjs` の `tzroutine` が通り、全ケースで `live == 0`。
- [ ] 既存の `tasks.mjs`・`async.mjs`・`features.mjs` が通る。
- [ ] 文書と規則 ID を更新し、`check-docs`・`check-spec`・`gen-docs --check`・`check-cli-docs` が通る。

## 落とし穴

- **本体を `Some(&Type::Unit)` で検査しないと、`do!` の本体が `IO<unit>` になる**（現状の節の例）。`self.lambda` に `Some(&Type::Task(Box::new(Type::Unit)))` を渡すと、`LambdaKind::Task` の分岐が結果を先に `unit` に決める（`closures.rs` 36〜42 行）。`expected` に呼び出し側の期待（`unit` かもしれない）を渡すと結果が決まらない。
- **`in_task` を false にする**: true のまま `block()` を読むと、`do!` が Task の bind（`TaskRun`）になる。
- **`tzroutine` の直後の改行**: `tzroutine` の識別子の次の行に `{` があるだけのコード（たとえば `let tzroutine = 1` の後の行がブロック）を壊さない。`tzroutine_ahead` は改行を見る。
- **adapter は env を解放する**: `@tz.apply.$task.N` は本体のあと `@tz.free(env)` する。`tsuzuri_tz_go` の後で生成コードが env を解放すると二重解放（`live` が負、ASan）。
- **`spawns` を IR の文字列で判定しない**: `@main` を作る時点ではほかの関数の IR がまだないことがある。型付きの module を歩く。
- **境界の `ret` が複数**: `tsuzuri_test_run` は `bad:` と各 `testN:` に `ret` がある。`with_tzroutine_scopes` は、その関数の `}` までのすべての `ret` の前に入れる。`%tz.scope` は `entry:` で定義するので、すべての block を支配する。
- **事前ビルドのランタイム**: 翻訳単位の 9 つの部分の書式（`"{}\n…"`）に 10 個目の `{}` を足すと、空でも改行が 1 つ増えて、全プログラムの照合が外れる（S3）。末尾に条件付きで足す。
- **Linux の mapping の上限**: 文書に書く（TZ01 の仕様 1.13）。テストで 10 万個を同時に生かさない（`sum_atomic 10000` は子がすぐ終わるので同時に生きる数は小さい）。

## 対象外

- チャンネル・`select`・同期・時間・取り消し（TZ03〜TZ07）。ブロッキング呼び出しの引き継ぎ（TZ08）。WASM（TZ09）。プリエンプション（TZ10）。`Parallel`・`Routine.scope`（TZ11）。Task の削除（TZ12）。
- `go f(x)` の「引数を呼び出し側で評価する」形（`tzroutine f x`）。`tzroutine { f x }` の `x` は捕捉なので、呼び出し側で移される（値は呼び出し時点のもの）。違いは `f` の引数の式の副作用が子で起きることだけ（D3）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `tzroutine` は「直後が同じ行の `{`」のときだけの文脈キーワード | 予約語を足さない（D-55）。`task { }` と同じ見た目 | 既定案 |
| D2 | 本体は型 `unit` の通常のブロック。`do!`／`let!` は IO をその場で実行する | サーバーの処理は I/O をする。`task { }` の bind の意味（Task の消費）は tzroutine に要らない | 既定案 |
| D3 | `go f(x)` の形は作らない。`tzroutine { f x }` だけ | 捕捉で値は生成時に決まる。構文を 1 つにする | 既定案 |
| D4 | 捕捉の規則と下ろしは `LambdaKind::Task` をそのまま使う（`"$task"` の module、`is_task`） | 検査済みの所有権の規則（ムーブ・`Send`・借用の拒否）を再利用する。TZ12 で Task を消しても内部の 1 回だけの関数として残す | 既定案 |
| D5 | `Routine.go` は `own Owned.Function<unit, unit>` を取る | 関数値で渡せる 1 回の仕事の型は `Owned.Function` だけ（普通の関数値は複製されうる） | 既定案 |
| D6 | 終了の待ちは境界の関数の IR を後処理して入れる（`@main` だけ生成側で入れる） | 境界の生成の箇所が多く、形が単純（`with_stack_checks` と同じ手法）。最後の式の表示の前に待つため `@main` は別 | 既定案 |
| D7 | 境界のデッドロックはランタイムでトラップ（`tsuzuri_tz_boundary_leave`） | 境界にはソースの位置がない。理由はランタイムが先に書いている | 既定案 |
| D8 | Task との共存を拒否しない（TZ12 まで） | 混ぜても正しく動く（遅くなるだけ）。拒否は TZ12 で消す仮の規則を増やすだけ | 既定案 |

## 実装と検証（2026-11-02）

- **範囲**: 仕様 1〜5 をすべて実装した（1 つの PR。TZ01 の PR #79 のブランチの上）。着手時に HEAD と照合し、次の「チケットから外れた判断」のとおり合わせた。共有の数は起票時と同じだった（`FRONTEND_FORMAT` 15 → 16、`ExprKind` の tag 46 → 47 は 47 番、`Builtin::ALL` 260 → 265、`RESERVED_MODULES` 59 → 60、`CONTEXTUAL_KEYWORDS` 8 → 9）。
- **構文**: `ExprKind::Routine`（`src/syntax.rs`、codec の tag 47、`FRONTEND_FORMAT` 16）、`CONTEXTUAL_KEYWORDS` に `tzroutine`。`src/parser.rs` の `primary` が `tzroutine_ahead`（次の字句が同じ行の `{`）のときだけ `fn tzroutine` を読む。本体は `in_task = false` の通常のブロック。formatter・`computation.rs`・`warnings.rs` の歩き。
- **型検査**: `composed_expression` の `ExprKind::Routine` が本体を `LambdaKind::Task` の `Task<unit>` として検査し、`TypedExprKind::RoutineGo` を作る（捕捉の規則は `task { }` と同じ）。組み込み `Builtin::{RoutineGo, RoutineYield, RoutineCount, RoutineMaxProcs, RoutineCpuCount}`、`is_routine`・`spawns`。`RESERVED_MODULES` に `Routine`。効果（`effects.rs`・`pure_rules.rs`・`kg/effects.rs`）、所有権（`ownership.rs` の消費）、const・SMT の拒否、KG の `calls` の辺（`Routine.go`）、図の並行の区間。
- **生成**: `routine_go`（`src/llvm.rs`）、`Routine.*` の本体（`routine_builtin`、`@tz.tzroutine.owned`）、`llvm::spawns`、`with_tzroutine_scopes`（`tsuzuri_main`・`tsuzuri_test_run`・`tsuzuri_bench_sample`・`tz_<名前>`）と `console_main`／`console_def_main` の待ち。ランタイムに `tsuzuri_tz_boundary_leave`（`tests/tzroutine_runtime.c` の `boundary`）。
- **ドライバー**: `tzroutine_symbols`・`tzroutine_runtime`、翻訳単位の末尾に `tzroutine_runtime_source_for(platform)`、`-pthread`、`stack_runtime` の条件、WebAssembly・`--wasm-feature threads`・`--freestanding`・`--trap-mode return`（spawn するとき）の `E2000`、aarch64・x86-64 以外の `E2002`。`src/test_runner.rs` のネイティブの実行器 2 つと WebAssembly の実行器の拒否。
- **チケットから外れた判断**:
  1. **Windows を拒否しない**: TZ01 の Phase 2 を同じ一連の作業で入れたので、仕様 5 の「Windows は `E2002`」の行は入れず（仕様 5 のとおり「Phase 2 のあとはこの行を消す」）、ドライバーは `tzroutine-windows.h` を展開した翻訳単位を作る。実行の確認は `windows.yml`。
  2. **unit の adapter は `i8` を返す**: 現状の節の「結果が unit の adapter は `void (ptr)`」は誤りで、`llvm_type(Type::Unit)` は `i8`（`c_type` が `void`）。TZ01 のランタイムは `void (*)(void *)` を呼ぶので、関数の型を合わせるため、場所ごとに adapter へ転送するだけの `void @tz.tzroutine.start.$task.N.<count>(ptr %env)` を作り、それを `tsuzuri_tz_go` に渡す（`Routine.go` の補助関数も `i8` を受けて捨てる）。
  3. **`let` は改行でも終わる**: 通常のブロックでは `let` の後に `;` が要るが、チケットの例（早見・TZ03・TZ04）は改行で区切っている。`task { }` と同じく、`tzroutine { }` の本体（`in_routine`）では改行でも `let` が終わるようにした。`let!`・`do!` は IO をその場で実行する（D2）。
  4. **`return` は関数の本体と同じ扱い**: 仕様 1 は `return` を `E0002` としたが、通常のブロックの `return` は暗黙のコンピュテーション式として読まれ、関数の本体でも `return ()` は通る。D2（本体は関数の本体と同じ通常のブロック）に合わせ、`return ()` は通り、`return 1` は `E1003` にした。
  5. **予約名は module のファイルで検査する**: 仕様 4 の「`record Routine` は `E1011`」は今の言語と違う（`record Mutex` なども通る。予約名はルートのファイル名とパスの先頭に使えない規則）。`Routine.tz` が `E1011` になることを検査した。
  6. **`Routine.go (\() -> ())` は `E1005`**: 所有する関数でない関数値を渡すと、既存の規則で `E1005`（`only a function value can be called`）。
  7. **`features.mjs` の native だけの suite**: 既存の native だけの suite がなかったので（手順 9 の「なければ止めて報告」）、suite の `nativeOnly: true` を足して WebAssembly の段を飛ばす（最小の変更。TZ09 で外す）。
  8. **stack probe**（TZ01 の PR #79 のレビューの 3 回目）: 局所の領域は `MAX_FRAME_BYTES` の外なので、1 つの frame が tzroutine のスタックのガード（128 KiB）より大きくなりうる（`[[i64; 1024]; 8]` の局所 3 つで `-O0` の frame は約 600 KB）。spawn するプログラムでは、`with_stack_probes` がすべての `define` に `"probe-stack"="inline-asm"` を付け、frame がガードに触れて `trap: stack overflow` と報告されるようにした（aarch64 の macOS・Linux と x86-64 の Linux で、probe の繰り返しが出ることをアセンブリで確かめた）。`--backend fast` は Cranelift の inline の probe をもとから使い、引用符の属性を読み飛ばす。spawn しないプログラムの IR は変わらない。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。`cargo nextest run --locked` 2,859 件すべて成功（4 件 skip。手順 1 の 2,851 件 + 新しいテスト 8 件。件数の pin は `Builtin::ALL` 265、不純 97、`Effect::Task` 15、`alloc` 60、`callbacks` 18、`IMPURE_BUILTINS` 96、`RESERVED_MODULES` 60）。その後の `tests/tzroutine.rs` は 9 件（stack probe の 1 件を足した）。`honors_the_exact_specialization_limit` 成功。E2E（dev のバイナリ）: `node tests/tzroutine.mjs` `27 runs passed`（`-O0`／`-O3` × P 1・4 の run 5 本、tzroutine の中のトラップ、`tsuzuri test`、ガードより大きな frame の溢れの報告、WebAssembly の `E2000`）、`node tests/features.mjs … tzroutine` `13 cases passed`（`live == 0`）、`node tests/tzroutine_runtime.mjs` 176 runs（`boundary` を含む）。既存の suite: `tasks.mjs`・`async.mjs`（Node 24）・`prebuilt_runtime.mjs` 成功。`features.mjs` は 70 の suite のうち 69 が成功し、`unicode` と `regex` は TZ01 だけの木（`main` と同じコンパイラ）でも同じ所で落ちる（手元の環境の既存の失敗。`tz_category_chunk`・`tz_property_hash` の照合）。**tzroutine を使わないプログラムの IR**: `tests/fixtures/*` と `examples/*` のすべての `Main.tz` を、TZ01 だけの木のコンパイラとこのコンパイラで `--emit llvm -O1`（native と wasm32）にして比べ、272 個すべてが byte 単位で一致した（S1。10 個は単独で build できない fixture）。文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（変更した 7 ページ）`333 links, 17 checked examples, 32 native runs`、`check-spec --require-markers` `233 active`、`conformance.mjs --area TSK` `15 ok`、`gen-docs --check` 0 stale、`check-cli-docs` `4 checks, 0 failed`。
- **文書**: 新規 `async-tasks-and-lazy/tzroutine.md`・`built-in-types-and-modules/routine.md`。`index.md`、`values-and-functions/keywords.md`（文脈キーワード）、`organizing-tsuzuri/modules.md`（予約名 60 個）、`compiler/option.md`（組み合わせの誤り 5 行）、`compiler/platforms.md`、`compiler/conformance.md`（生成）、`docs/language.md`（節「tzroutine」、文脈キーワード、予約名）、`docs/architecture.md`（TZ01 の節に「生成コードとの接続（TZ02）」）、`README.md`（主な特徴、タスクと並列処理の項、検証の表）、`spec/rules/TSK.md`（題と TSK-011〜015）、`tests/conformance/TSK/TSK-011`〜`015`、`vsc/syntaxes/tsuzuri.tmLanguage.json`、`vsc/src/core.ts`。
- **新しく予約した std モジュール名**: `Routine`（`Routine.tz` は `E1011`。release notes に載せる）。
- **変更したファイル**: `src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/parser.rs`・`src/parse_control.rs`・`src/formatter.rs`・`src/computation.rs`・`src/warnings.rs`・`src/check.rs`・`src/ownership.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/kg/sites.rs`・`src/kg_capture.rs`・`src/diagram/concurrency.rs`・`src/const_rules.rs`・`src/const_machine.rs`・`src/prove_smt.rs`・`src/stdlib.rs`・`src/llvm.rs`・`src/driver.rs`・`src/test_runner.rs`・`src/runtime/tzroutine.c`、`tests/tzroutine.rs`・`tests/tzroutine.mjs`・`tests/fixtures/tzroutine/Main.tz`（新規）・`tests/features.mjs`・`tests/tzroutine_runtime.c`・`tests/tzroutine_runtime.mjs`、上の文書。
