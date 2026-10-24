# B10: トラップのバックトレースとクラッシュレポート、純粋関数の失敗の閉じ込め（`Domain`）

| 項目 | 内容 |
| --- | --- |
| ID | B10 |
| 優先度 | P2 |
| 規模 | XL。Phase 1（native のバックトレースとクラッシュレポート、WASM の関数名）L、Phase 2（`Domain`）XL、Phase 3（後始末の表と facts との対応）S |
| 依存 | [E14](_completed/E14-trap-boundary.md)（done。Phase 2 は E14 の `src/runtime/trap.c` の境界と追跡確保を土台にする）。Phase 2 はさらに [A19](_completed/A19-pure-functions.md) の Phase 1（`pure def` と `src/pure_rules.rs`。HEAD では todo）。Phase 1・3 は A19 に依存しない |
| 後続 | A20（`DomainRun` の効果の分類。「前提とする他チケットのインターフェース」）、G22（`Diagnostic` を `Box` にするときの `trap` の置き場）、G23（`E1038` の kind の登録と `explain`）、G26（`trap_sites` と `E2005` の `trap.kind` の対応。Phase 3） |
| 状態 | todo |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの `9dd0fbbc` との差は `docs/benchmarks.md` の 41 行の削除だけ: `git diff --stat a2313d19 9dd0fbbc`） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1… はすべて承認済み |
| 改善する劣位 | Rust・Go・Swift 比: トラップしても呼び出しの連鎖が出ない。Erlang・Rust（`catch_unwind`）・Go（`recover`）比: 計算 1 件の失敗を、プロセスを失わずに値として受けられない。D-50 の調査結論 (1)「内部で持つ事実を診断にもツールにも出さない」の、失敗の側の穴 |
| 手本にする既存実装 | Phase 1: `src/runtime/stack.c` と driver の `stack_runtime`（実行ファイルだけに連結する C）、`src/llvm_traps.rs` の `instrument`、`src/trap.rs` の `TrapSite`、`src/driver.rs` の `run_process`、`src/runtime/trap-boundary.mjs` の `createBoundary`、`tests/trap_locations.rs`・`tests/trap_boundary.mjs`・`tests/arguments.rs`（C を `clang` で組んで走らせる Rust のテスト）。Phase 2: `src/runtime/trap.c`、`src/llvm.rs` の `emit_trap_return`・`heap_native_tracked`・`emit_typed_builtin`（`Builtin::FixedArrayInit`・`Builtin::OwnedCall`）、`src/llvm_task.rs` の `parallel_result_tasks`、`src/llvm_sync.rs` の `result_cases`、`src/gpu.rs` の `validate_calls`、`std/Gpu.tz`、`src/stdlib.rs` の `OPT_IN`、`src/polymorph.rs` の `Checker::builtin`、`tests/trap_boundary_runtime.c`・`tests/trap_return.mjs`・`tests/trap_return_host.c` |
| 主な影響ファイル | Phase 1: `src/runtime/backtrace.c`（新規）・`tests/trap_backtrace.rs`（新規）・`tests/fixtures/trap_backtrace/Main.tz`（新規）・`tests/fixtures/trap_backtrace_deep/Main.tz`（新規）、`src/trap.rs`・`src/diagnostic.rs`・`src/docgen.rs`・`src/repl.rs`・`src/llvm_traps.rs`・`src/llvm.rs`・`src/driver.rs`・`src/runtime/trap-boundary.mjs`・`tests/trap_boundary.mjs`・`tests/stack_overflow.mjs`・文書。Phase 2: `std/Domain.tz`・`src/domain_rules.rs`・`src/llvm_domain.rs`・`tests/domain.rs`・`tests/domain.mjs`・`tests/domain_runtime.c`・`tests/domain_host.c`・`tests/fixtures/domain/Main.tz`・`tests/fixtures/domain_host/Main.tz`・`_tsuzuri/language-reference/built-in-types-and-modules/domain.md`（以上すべて（新規））、`src/runtime/trap.c`・`src/check.rs`・`src/polymorph.rs`・`src/stdlib.rs`・`src/trap.rs`・`src/llvm.rs`・`src/llvm_traps.rs`・`src/driver.rs`・`src/test_runner.rs`・`src/pure_rules.rs`（A19 の新規ファイル）・`vsc/src/core.ts`・`tests/trap_return.mjs`・文書。Phase 3: 文書だけ（`docs/language.md`・`_tsuzuri/language-reference/`） |

## 実装者向け早見

- 範囲: Phase 1（native のバックトレース）と Phase 3（後始末の表）は A19 なしで着手できる。Phase 2（`Domain`）は A19 Phase 1 が done（`ls src/pure_rules.rs` が成功）になってから。**指示がなければ Phase 1 だけ**を実装する。
- 最初に読む（この順）: ① 決定事項 D1・D5・D9 ② `src/runtime/trap.c`（全文） ③ `src/llvm_traps.rs` の `instrument` ④ `src/driver.rs` の `run_process`・`stack_runtime` ⑤ `docs/language.md`「トラップ位置」⑥ E14 チケット（`_completed/E14-trap-boundary.md`）。付録 [B10-appendix.md](B10-appendix.md) に実測と参照実装の C がある。
- 最初に書くテスト: Phase 1 は `src/trap.rs` の単体テスト `parses_a_report_line` → `tests/trap_backtrace.rs::report_file_lists_the_frames_of_a_trapping_chain`。Phase 2 は `tests/domain_runtime.c`（付録 B-2 の C をそのまま置く）。
- 触ってはいけない: `--trap-info` なしのビルドの IR・object・バイナリ（1 byte も変えない）、実行ファイル以外の `--trap-info` の IR（`--emit object|shared|llvm`・WASM）、`tsuzuri_try_<name>`・`tsuzuri_boundary_run` の意味（status 0・1・2）、`E2005` の既存 5 キーと既存メッセージ、WASM の import（空のまま）、`TrapKind` の並び（`tsuzuri_trap_info.kind` の値）、利用者の `Drop`・`finally` を走らせない規則（巻き戻しを入れない）。
- 外から借りるものは 2 つだけ: `llvm-symbolizer`（Phase 1 の関数名と行。なければ縮退し、そのテストは飛ばす）と、Phase 2 の A19。
- 完了の見分け方: **Phase 1** は `tsuzuri run tests/fixtures/trap_backtrace -O0` が `E2005` の下に `  at Main.level3`・`level2`・`level1`・`main` の 4 行を出し（想定出力）、`--json` が `trap` を足し、`--trap-info` なしの出力が 1 byte も変わらない。**Phase 2** は `Domain.run (\() -> 1 / x)` が `Result.Error` を返してプロセスが続き、成功・失敗を 10,000 回交互に実行しても `live == 0`。**Phase 3** は `check-docs.mjs` が通り、後始末の表が 2 か所（`docs/language.md`・言語リファレンス）にある。
- 迷ったら: 決定事項 → 停止条件 → 最も保守的な選択（報告を出さない・拒否する）の順に従う。**`Domain` の安全性は A19 の検査と「捕捉・結果は平たいデータ」の制限に依る**（付録 C）。その検査を緩める変更は、理由が何であれしない。

## 目的

トラップは今、種別と位置を 1 行で言って、プロセスを終える。どの呼び出しの連鎖でそこへ来たかは分からず、計算 1 件の失敗を、プロセスを失わずに受ける手段もない。

- **バックトレース（Phase 1）**: `tsuzuri run` がトラップを報告するとき、トラップした関数から `main` までの呼び出しの連鎖を `E2005` に付ける（人間には `  at Main.level2 (./Main.tz:5:15)` の行、機械には `--json` の `trap` オブジェクト）。クラッシュレポート（環境変数 `TSUZURI_TRAP_REPORT` が指すファイルの 1 行）は、配布した `--trap-info` の実行ファイルからも取れる。費用はトラップするまで 0。
- **失敗の閉じ込め（Phase 2）**: 純粋関数の計算を `Domain.run (\() -> …)` に包むと、中のトラップ（範囲外・ゼロ除算・`assert`・網羅されない `match`・確保失敗）が `Result.Error (failure)` になり、確保した値は一括で解放される。AI が生成した計算や外部の入力に対する計算を 1 件ずつ試せ、`tsuzuri test` の中でトラップを確かめることもできる。
- **後始末の表（Phase 3）**: トラップのとき何が走り何が走らないか（`Drop`・`finally`・ロック・ファイル・ヒープ・標準出力）を、実測した事実だけで 1 つの表にまとめる。

HEAD で測った数字（Apple M1 Max、macOS 27.0.1、2026-10-18。詳細は付録 A・B）:

| 測ったもの | 結果 |
| --- | --- |
| 呼び出し 4 段（`level3` → `level2` → `level1` → `main`）の `ゼロ除算` を `tsuzuri run` | 出力は `trap:` の 1 行と `E2005` の 1 件だけ。連鎖は出ない |
| 生成した実行ファイルで `backtrace()` 相当を呼ぶ（frame pointer の属性なし） | 3 フレーム（`tz_dump_backtrace`・`tz.trap.report`・`start`）。Tsuzuri の関数は 0（`level3`〜`main` の 4 つが落ちる） |
| 同（全関数に `"frame-pointer"="all"`、`-O0`） | 9 フレーム。上の 2 つに `tz.fn.Main.level3`・`level2`・`level1`・`main`・`tsuzuri_main`・C の `main`・`start`。`-O3` は 4 つの関数が `tsuzuri_main` へ inline され 4 フレーム |
| `llvm-symbolizer`（`--output-style=JSON`） | 関数名（`-g` なしは名前だけ、`-g` の `.dwarf` は `path:line:column` も） |
| ドメインの入口（空のドメイン 1 回）と追跡確保 | `setjmp` 約 370 ns・`sigsetjmp(env, 0)` 約 40 ns。追跡確保（境界の中）は素の `malloc`/`free`（約 20 ns）の約 2.8 倍（付録 A-9） |

非目標:

- 巻き戻し（`Drop`・`finally` を走らせるトラップ）、言語内の例外、`?` 演算子（D-10、GUIDE の「採用しない改善」）。
- スタック枯渇からの回復と、枯渇した stack の frame の報告（signal の文脈から fp を読む処理は OS・arch ごとに違い、macOS arm64 しか確認できない）。
- 無限ループ・時間切れの打ち切りと、ドメインの確保量の上限。WASM・Windows の `Domain`（`E1038`）。
- `tsuzuri test`・`bench` のバックトレース（テストの実行ファイルは計装を持たない。失敗したテストは `tsuzuri run` で再現する）。

## 着手条件と停止条件

### 着手条件

- E14 が `_features/README.md` の状態欄で done（`grep -nE "^\| E14 " _features/README.md`）。HEAD で done。
- GUIDE §2.3 の基準コマンドを実行して件数を控える: `cargo build --release --locked`、`cargo test --locked`、`node tests/trap_return.mjs target/release/tsuzuri`（付録 A の実測では exit 0）、`node tests/trap_boundary.mjs target/release/tsuzuri`、`node tests/stack_overflow.mjs target/release/tsuzuri`。テストの一時ディレクトリは `TMPDIR` で作業用の場所へ向けてよい。
- Phase 2 は A19 Phase 1 が done であること: `ls src/pure_rules.rs` が成功し、`grep -n "pub(super) fn own_violation" src/pure_rules.rs` が 1 件。A19 が未実装なら Phase 2 に着手せず、Phase 1・3 だけを進める（「前提とする他チケットのインターフェース」）。
- 外部ツールの確認（任意）: `llvm-symbolizer --version`（Homebrew の LLVM は `$(brew --prefix llvm)/bin`）。なくても Phase 1 は実装できる（symbolizer が要るテストは `llvm-symbolizer --version` が失敗したら飛ばす）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

- **S1**（Phase 1）`--trap-info` を付けない build の IR・object・バイナリ、または実行ファイル以外の `--trap-info` の IR が 1 byte でも変わる。選択肢: A 変更を実行ファイル向けの計装に閉じ込め直す、B その部分を諦める。
- **S2**（Phase 1）`-O0` の `tests/fixtures/trap_backtrace` で、報告ファイルの frame が 5 個未満（Tsuzuri の 4 関数 + `tsuzuri_main` が見えない）。Linux で frame pointer の鎖が辿れない場合を含む。選択肢: A walker を `uwtable` の unwind 表を使う実装へ変える、B その OS では報告を出さない（`trap.frames` は空）。
- **S3**（Phase 1）`tests/trap_locations.rs`・`tests/script.mjs`・`tests/repl.mjs` の既存の期待値を変えないと通らない（`E2005` の既存メッセージ・行・JSON の既存キーは変えない）。
- **S4**（Phase 1）`Diagnostic` へ field を足して、`Diagnostic { … }` のリテラルが `src/docgen.rs`・`src/repl.rs` の 2 箇所より多く壊れる、または GUIDE §3.1 の stack の回帰 4 コマンドが落ちる。選択肢: G22 の `Box` 化を先に入れる。
- **S5**（Phase 1）`llvm-symbolizer` の JSON が付録 A の形（配列の `Symbol` に `FunctionName`・`FileName`・`Line`・`Column`）でない版、または CI の Node の V8 の `error.stack` の wasm の行が付録 A の形（実測は Node 20.19.6）でない。選択肢: 版を限る、解析を足す。
- **S6**（Phase 2）`trap.c` の変更が Windows（`vsc/toolchain/zig/zig cc -target x86_64-windows-gnu -std=c11 -Wall -Wextra -c`）か Linux（`-target x86_64-linux-gnu`・`aarch64-linux-musl`）で警告・エラーになる、または Windows の CI（`windows.yml`）が赤になる。Windows には `sigsetjmp` がない（`TZ_SETJMP` の切り替えを消さない）。
- **S7**（Phase 2）A19 Phase 1 が done でない。**着手しない**。A19 の検査を真似た簡易版を B10 に作らない（安全性の根拠が A19 の検査だから。D6）。
- **S8**（Phase 2）`trap.c` の変更で、E14 の既存の check（`tests/trap_boundary_runtime.c`・`tests/trap_return.mjs`・`tests/trap_return_sync.mjs`・`tests/tasks.mjs`）が 1 件でも落ちる。
- **S9**（Phase 2）`Domain.run` の E2E で `live == 0` が成り立たない、または二重解放が出る（候補: 閉包の `drop_value` の位置、`apply_value` の `borrowed`、`tz_untrack_chain`）。直す試みが 2 回続けて失敗したら止める。
- **S10**（Phase 2）純粋と判定された本体から、ドメインの外の状態を書き換える経路が見つかった（付録 C の前提が崩れる）。選択肢: A 捕捉・結果の型の制限を狭める、B Phase 2 を「設計のみ・非実装」に戻す（D1 の第 3 案）。
- **S11**（全 Phase）生成 IR が非決定的になる、または `-O0` と `-O3` で結果が違う。新しい crate・`unsafe`・既定の WASM import・予約語が必要に見える。

## 現状（HEAD `a2313d19` で確認）

### トラップの報告（native）

- `--trap-info` の実行ファイルは、トラップの直前に `@tz.trap.report(i32 site)` を呼ぶ（`src/llvm_traps.rs` の `instrument`）。native では site の message `trap: <理由> at <path>:<line>:<column>` を stderr へ書いてから `@llvm.trap()`（arm64 は `brk`、SIGTRAP）。生成した関数は `internal` で、`instrument` の `managed` に入る関数には末尾の引数 `i32 %tz.context`（呼び出し地点の site の基点）が付く。
- `tsuzuri run`（`src/driver.rs` の `run_process`）は `trap_info: true` で build し、子の stderr を中継（`relay`）し、失敗したら stderr に `site.message(sources)` を含む `TrapSite` を探して `E2005` の主スパンにする。メッセージは `program terminated with signal: 5 (SIGTRAP); integer division, indexing, assert, or allocation may have trapped`（`--json` では stderr の本文を含み、診断は stderr に 1 行）。
- 再現（`tests/fixtures/trap_backtrace/Main.tz` に置く内容。HEAD の `check` が通ることを確認済み）:

```tsuzuri
def level3 :: i64 -> i64
fn level3 x = 100 / x

def level2 :: i64 -> i64
fn level2 x = level3 (x - 3) + 1

def level1 :: i64 -> i64
fn level1 x = level2 (x + 3) + 1

def main :: Array<string> -> i32
fn main args = if level1 args.length > 0 then 1 else 0
```

```sh
mkdir -p target/tz-b10/chain      # 上のソースを Main.tz として置く（target/ は git の管理外）
target/release/tsuzuri run target/tz-b10/chain          # 引数なし: args.length は 0。level3 の 100 / 0
target/release/tsuzuri run target/tz-b10/chain --json
```

```text
trap: integer division by zero at target/tz-b10/chain/Main.tz:2:15
target/tz-b10/chain/Main.tz:2:15: error[E2005]: program terminated with signal: 5 (SIGTRAP); integer division, indexing, assert, or allocation may have trapped
  2 | fn level3 x = 100 / x
    |               ^
{"severity":"error","code":"E2005","message":"program terminated with signal: 5 (SIGTRAP):\ntrap: integer division by zero at target/tz-b10/chain/Main.tz:2:15","path":"target/tz-b10/chain/Main.tz","span":{"start":39,"end":46,"line":2,"column":15,"end_line":2,"end_column":22}}
```

- 連鎖は出ない。`-O0` の実行ファイルの局所 symbol は `_tz.fn.Main.level3`・`level2`・`level1`・`main`（`nm -n` の `t`）で、`-O3` では 4 関数が `_main` へ inline されて symbol が残らない（付録 A）。
- 生成した関数は frame pointer を設定しない（`stp x29, x30` で保存するが `add x29, sp, …` がない）。そのため frame pointer の鎖を辿る walker は、`tz.trap.report` の次に `start` へ飛ぶ（付録 A の実測 3 フレーム）。`clang -fno-omit-frame-pointer`・module flag `!{i32 7, !"frame-pointer", i32 2}` は IR の入力に効かない。効くのは各 `define` の関数属性 `"frame-pointer"="all"` だけ（付録 A）。
- スタック枯渇は E14 Phase 3 の `src/runtime/stack.c`（再帰するプログラムの実行ファイルだけ）が `trap: stack overflow` を書いて `abort()` する。報告に frame はない。
- `tsuzuri test` のテスト実行ファイルは `--trap-info` を持たない（`--trap-info` は `build`・`run` だけ）。失敗は `trapped or exited with code N` のような 1 行。

### Phase 2・3 の土台

- 境界（`src/runtime/trap.c`）: `tsuzuri_boundary_run(thunk, argument, trap)` はスレッドごとの frame（`static _Thread_local struct tz_frame *tz_frame`）を作って `setjmp` し、**入れ子を拒否する**（status 2）。`tsuzuri_trap_raise(site, kind)` は frame があれば `longjmp`、なければ戻る。`tsuzuri_tracked_malloc/free/realloc` は frame の `owner`（`struct tz_boundary`: mutex・双方向 list・hash 表）に block を載せ、トラップなら全部 `free`、成功なら記録だけ捨てる。`tsuzuri_boundary_item`（`Task.parallel` の worker 用）は `frame.outer` を持つ frame を作り、`owner` は投入側のものを共有する。
- `--trap-mode return` の IR は `@tz.alloc` が `@tsuzuri_tracked_malloc` を呼び（`src/llvm.rs` の `heap_native_tracked`）、`@tz.trap.report` が先頭で `tsuzuri_trap_raise` を呼ぶ（`instrument` の `raise`）。`--emit exe` は拒否される（E14）。C から呼ばれる callback は `%tz.context` を持たない（`managed` が `@tz.task.item.`・`@tz.parallel.chunk.` を除く）。
- 純粋性: A19 は todo（`ls src/pure_rules.rs` は失敗、`CONTEXTUAL_KEYWORDS`（`src/syntax.rs`）は 7 語で `pure` がない）。関数値の捕捉は、Copy を複製・非 Copy を move で環境へ入れ、`ref mut` と `Task`・`Rc`・`Drop` 型は捕捉できない（`E1005`。`_tsuzuri/language-reference/values-and-functions/lambda-expressions.md`「捕捉」）。`tz.apply.*` の `i1 %borrow` は、`true` が環境を消費せず、`false` が環境を解放して捕捉した値を本体へ渡す（付録 A）。
- 後始末（測った）: 利用者の `Drop` はトラップで走らない（`Drop` を持つ束縛のあと `/ 0` で `drop` の出力が出ない）。`IO.write_line` は毎回 `fflush` するので、トラップの前の出力は失われない（stdout をファイルとパイプへ向けて確認）。`File` の書き込みは 1 回ごとに `write`。`try … finally` は字句的で、検査付き算術の `OverflowException` だけを扱い、トラップでは走らない。

## 変えてはいけない意味

- `--trap-info` なしのすべての build の生成 IR・object・実行ファイル・`.wasm`・header・`.trap.json` は 1 byte も変わらない（Phase 1・2・3 のどの手順の後でも。手順の「確認」で `cmp` する）。`Domain` を使わないプログラムの IR も 1 byte も変わらない。
- `--emit object|shared|llvm|header`・WASM の `--trap-info` の出力は、Phase 1 の前後で同じ。変わるのは native の `--emit exe` と `run` の `--trap-info` だけ（frame pointer の属性・`tz.trap.report` の呼び出し・C ランタイム 1 つの連結）。
- `E2005` の既存のメッセージ・既存の JSON のキー（`severity`・`code`・`message`・`path`・`span`）・終了コード（1）は変えない。足すのは `trap` キー 1 つだけ（値があるときだけ）。
- トラップの意味は変えない: 巻き戻さない、`Drop`・`finally` を走らせない、境界の外では終了する。`Domain.run` が閉じ込めるのは**そのクロージャの中**のトラップだけで、ドメインの外のトラップは今までどおりプロセスを終える。
- `tsuzuri_try_<name>`・`tsuzuri_boundary_run`・`tsuzuri_boundary_item` の戻り値と意味（status 0・1・2、入れ子は 2）。`trap.c` の既存の `tsuzuri_tracked_*` は、ドメインの外（frame が 1 つか 0）の挙動を変えない。
- `Domain` を足しても、予約語・既定の WASM の import・既存の std の署名は増減しない。`Domain` はソースに名前を書いたときだけ読み込まれる opt-in の std モジュールである（D-40）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | native のクラッシュレポート（frame pointer・`backtrace.c`・`TSUZURI_TRAP_REPORT`）、`E2005` の `trap`（`  at …` の行と JSON）、`llvm-symbolizer`、WASM の `frames` | L | E14 | `--trap-info` の実行ファイルと `run` の出力だけが変わる。symbolizer がなければ縮退する。A19 は不要 |
| 2 | `Domain.run`・`Failure`（std）、`E1038`、`trap.c` の入れ子の frame、`sigsetjmp`、native の exe・object・shared・test | XL | A19 Phase 1 | `Domain` を書かないプログラムの出力は変わらない（opt-in の std モジュール） |
| 3 | 後始末の表、`trap_sites`（G26）と `E2005` の `trap.kind` の対応 | S | Phase 1 | 文書だけ |

## 仕様

### Phase 1: バックトレースとクラッシュレポート

#### 1.1 報告ファイル（子の実行ファイル）

- 対象は、**`--trap-info` を付けて build した native の実行ファイル**（`build --emit exe`・`run`・`script`・REPL の実行）で、OS が unix（`Platform::unix()`）かつ `--freestanding` でないもの。object・shared・llvm・WASM・Windows の出力は変わらない。
- 環境変数 `TSUZURI_TRAP_REPORT` が空でないとき、トラップのたびに、そのパスのファイルへ **1 行を追記**する（`open(O_WRONLY|O_CREAT|O_APPEND, 0600)`）。`open` に失敗したら何もしない。未設定なら何もしない（stderr も今までと同じ）。
- 行の形式（`\n` で終わる ASCII。1,024 byte 以内）: `tz-trap/1 <site> <slide> <truncated> <address>…`

| 欄 | 形 | 意味 |
| --- | --- | --- |
| `tz-trap/1` | 固定 | 形式の版 |
| `<site>` | 10 進の `u32` | `@tz.trap.report` が受けた site（`.trap.json` の `id`） |
| `<slide>` | `0x` 付き 16 進 | 読み込み位置 − link 時の位置（macOS は `_dyld_get_image_vmaddr_slide(0)`、それ以外は `dl_iterate_phdr` の最初の `dlpi_addr`） |
| `<truncated>` | `0` か `1` | 32 個で打ち切ったら `1` |
| `<address>` | `0x` 付き 16 進、0〜32 個 | 戻りアドレス。最も内側が先。`tz.trap.report` への戻りアドレスは含めない |

- 例（付録 A の実測。macOS arm64、`-O0`）: `tz-trap/1 2 0x2188000 0 0x1021884e4 0x10218856c 0x1021885c0 0x10218861c 0x102188774 0x10218cd24 0x180d23e80`
- frame の鎖の止め方: 戻りアドレスが 0、frame record（16 byte）が読めない（`mincore` が失敗）、record が 8 byte 境界でない、次の frame pointer が現在の record 以下、32 個に達した、のどれか。
- 同時に複数のスレッドがトラップすると行は複数になる。親は最初の行を使う。

#### 1.2 `E2005` の `trap` オブジェクト

- 付く条件: `run`（`script`・`run_captured` を含む）の子が失敗し、トラップの site を特定できた（既存の探索）か、stderr が `trap: stack overflow` を含むとき。`Diagnostic.trap: Option<Box<TrapReport>>`。
- JSON は既存のオブジェクトの**末尾**に `"trap":{…}` を足す。キーの順序と省略の規則:

| キー | 型 | 規則 |
| --- | --- | --- |
| `kind` | string | `TrapKind::description()`。stack overflow は `"stack overflow"` |
| `site` | 数 | stack overflow は 0 |
| `frames` | 配列 | 0 個なら `[]`。最大 32 |
| `frames[].function` | string | 解決できたときだけ（1.3 の表のあとの名前） |
| `frames[].path`・`line`・`column` | string・数・数 | `-g` と symbolizer で行が分かったときだけ。`line`・`column` は 1 始まり |
| `frames[].address` | string `"0x…"` | 常にある。戻りアドレス − slide（link 時のアドレス） |
| `truncated` | `true` | 打ち切ったときだけ |

- 人間向け: 主の位置の引用のあとに、frame ごとに 1 行 `  at <function>` か `  at <function> (<path>:<line>:<column>)`。symbolizer が使えず 1 つも解決できなかったときは frame の行を出さず、次の 1 行だけを出す（frame が 1 つ以上あったときに限る）: `  note: frames are not resolved; install llvm-symbolizer or set TSUZURI_LLVM_SYMBOLIZER`。`truncated` なら最後に `  ... (more frames omitted)`。
- 出力例（想定。`tests/fixtures/trap_backtrace`、`-O0`）:

```text
trap: integer division by zero at tests/fixtures/trap_backtrace/Main.tz:2:15
tests/fixtures/trap_backtrace/Main.tz:2:15: error[E2005]: program terminated with signal: 5 (SIGTRAP); integer division, indexing, assert, or allocation may have trapped
  2 | fn level3 x = 100 / x
    |               ^
  at Main.level3
  at Main.level2
  at Main.level1
  at Main.main
```

`-O0 -g` では `  at Main.level2 (./Main.tz:5:15)`・`  at Main.level1 (./Main.tz:8:15)`・`  at Main.main (./Main.tz:11:19)` のように行が付く（付録 A の実測。最も内側は DWARF の行が 0 で、位置は主スパンが示すので付かない）。JSON（`-O0`。`address` は戻りアドレス − slide。付録 A の実測の値）: `…,"span":{…},"trap":{"kind":"integer division by zero","site":2,"frames":[{"function":"Main.level3","address":"0x1000004e4"},{"function":"Main.level2","address":"0x10000056c"},{"function":"Main.level1","address":"0x1000005c0"},{"function":"Main.main","address":"0x10000061c"}]}}`。

#### 1.3 frame の選び方と名前（決定表）

symbolizer が返した関数名（linkage 名）を次の順で扱う。

| 関数名 | 扱い |
| --- | --- |
| `tz.fn.<名前>` | 採用。`tz.fn.` を取り、末尾の `.$mono.<数>`・`.cold.<数>`・`.cpu.<名前>` を（何個でも）取る。`tz.fn.Main.twice.$mono.2` → `Main.twice`。ラムダは `$lambda.3` のまま |
| `tz.specialized.<数>` | 採用（名前のまま） |
| `tz_<名前>` | 採用。`tz_` を取り、上と同じ接尾辞を取る（export の入口。WASM の `tz_entry.cold.1` → `entry`） |
| `tz.apply.`・`tz.env.`・`tz.closure.` で始まる | 捨てる（呼び出しの継ぎ手） |
| 上以外（`tsuzuri_main`・`main`・`start`・`tz.trap.`・`tz.alloc`・`tz.string.`・C の runtime・空） | 捨てる |
| 連続する同名の 2 frame のうち、元の名前が `.cold.<数>` を持つ方 | 捨てる（同じ論理 frame の別の部分。もう一方が残る。両方とも持たない同名は再帰なので両方残す） |

- 1 つの戻りアドレスに `Symbol` が複数あるとき（inline された関数）は、内側から外側の順にすべてを frame にする。HEAD の native の `-O3 -g` では、`tsuzuri_main` の 1 つしか返らない（付録 A）ので、`-O3` の frame は空になる。
- 採用が 0 個で、symbolizer が動いたとき（`--strip` の実行ファイル）は、frame を出さない（`frames: []`）。symbolizer が動かなかったときは、生の `address` だけの frame を最大 32 個、JSON に出す。人間向けは上の note の 1 行。

#### 1.4 symbolizer の呼び出し

- `tool("TSUZURI_LLVM_SYMBOLIZER", "llvm-symbolizer") --obj=<ファイル> --output-style=JSON --inlining --functions=linkage <addr>…`。`<ファイル>` は macOS で `-g`（`dwarf_sidecar`）なら `<実行ファイル>.dwarf`、そのほかは実行ファイル。`<addr>` は `0x` 付き 16 進の `address − slide − 1`（戻りアドレスは呼び出しの次の命令なので 1 引く）。
- 出力は JSON の配列で、要素は `{"Address":…,"ModuleName":…,"Symbol":[{"FunctionName","FileName","Line","Column",…}]}`（付録 A）。`FileName` が空か `Line` が 0 の frame には `path`・`line`・`column` を付けない。
- 起動できない・終了コードが 0 でない・JSON が読めない・要素数が `<addr>` の数と違う、のいずれも**エラーにせず**、未解決として扱う。

#### 1.5 WASM（`createBoundary`）

- `createBoundary(module, { imports, sites, frames })` の `frames`（既定 `false`）が `true` のとき、`reason: "trap"` の結果に `trap.frames: [{ function?: string, index: number, offset: number }]` を足す。`false` の結果は今までと deep equal。`reason: "stack"` には付けない。
- 元は V8 の `error.stack` の `    at <名前> (wasm://wasm/<hash>:wasm-function[<index>]:0x<offset>)`（名前なしは `    at wasm://wasm/<hash>:wasm-function[<index>]:0x<offset>`）。名前は `-g` の name section のとき入り、V8 が module 名を前に付ける（`artifact.tz.fn.Main.level3`）。名前は `^[^.]*\.(tz[._].*)$` の第 1 群を取ってから 1.3 の表を通す（`tz_<export>` は採用）。wasm の行でない行（JS の frame）は捨てる。
- `call` の間だけ `Error.stackTraceLimit` を 33 以上にし、終わったら戻す。

#### 1.6 CLI・環境変数

新しいフラグはない。環境変数 `TSUZURI_TRAP_REPORT`（子が読み、`run` が設定する）と `TSUZURI_LLVM_SYMBOLIZER`（コンパイラが読む）を足す（D-55 の追補に割り当て済み）。終了コードは変えない。

### Phase 2: `Domain`

#### 2.1 API（`std/Domain.tz`。opt-in の std モジュール）

```tsuzuri
record Failure { site: i64, kind: i64, message: string }
def run :: (unit -> 'a) -> Result<'a, Failure>
```

- `Domain.run body` は `body ()` を**失敗の領域（domain）**の中で呼ぶ。値が返れば `Result.Ok value`。中（呼ぶ関数を含む）でトラップしたら、実行を捨てて `Result.Error failure`。領域の中で確保したものは解放され、外の値は変わらない。Drop・`finally` は走らない（走らせる巻き戻しは入れない。純粋な本体は Drop を持つ値を作れない）。
- `Failure.site` は `<output>.trap.json` の `id`。`kind` は `TrapKind` の値（C の `tsuzuri_trap_info.kind` と同じ。付録 E）。`message` は `<理由>`、場所が分かる build では `<理由> at <path>:<line>:<column>`（`tsuzuri test` の実行ファイルは対応表を持たないので理由だけ）。
- 閉じ込めるのはトラップ全般。**閉じ込めない**: スタック枯渇（プロセスが終わる）、停止しないこと、確保量の上限、領域の外のトラップ。

```tsuzuri
def average :: [i64] -> i64 = \xs -> Array.sum (ref xs) / xs.length

def describe :: Result<i64, Domain.Failure> -> string = \outcome ->
    match outcome with
    | Result.Ok value -> $"average {value}"
    | Result.Error failure -> $"failed (kind {failure.kind})"

def main :: unit -> i32 = \() ->
    let data = [4, 8, 15]
    let empty: [i64] = []
    do! IO.write_line (describe (Domain.run (\() -> average data)))
    do! IO.write_line (describe (Domain.run (\() -> average empty)))
    0
```

想定出力は `average 9` と `failed (kind 1)`（1 は `integer division by zero`）。`tsuzuri test` の中でトラップを確かめる例（`at` は添字の範囲外を作るための補助）:

```tsuzuri
def at :: [i64] -> i64 -> i64 = \xs i -> xs[i]

test "an out-of-range index is caught by the domain" =
    let numbers = [1, 2, 3]
    match Domain.run (\() -> at numbers 5) with
    | Result.Error failure -> Test.is_true (failure.kind == 3)
    | Result.Ok _ -> Test.is_true false
```

#### 2.2 `E1038`: 呼び出し地点の規則

`Domain.run` の呼び出し（std の `Domain.run` を呼ぶユーザーのコード）ごとに次を**この順に**調べ、最初の違反だけを報告する。検査は `src/gpu.rs` の `validate_calls` と同じ形（型検査・特殊化・`closures::lower` のあとの `CheckedModule` を読む）。

| # | 条件 | メッセージ | 主スパン |
| --- | --- | --- | --- |
| 0 | `Domain.run` を直接呼んでいない（引数が 1 つの呼び出しでなく、関数値として渡す・変数へ入れる） | `'Domain.run' must be called directly with its closure; write 'Domain.run (\() -> ...)'` | その式 |
| 1 | 実引数が、ラムダ（`TypedExprKind::Closure`、または捕捉のないラムダを下げた `Function(User(id))`）でも、`pure def` の名前（`origin.pure`）でもない | `the argument of 'Domain.run' must be a lambda or the name of a pure def; write 'Domain.run (\() -> ...)'` | 実引数 |
| 2 | 捕捉した変数の型（`ref` は 1 段だけ外す）が、A19 の `own_violation` で違反（平たいデータでない） | `the closure passed to 'Domain.run' captures '{name}', which holds {what}; capture plain data or compute the value inside the closure` | 実引数のラムダ |
| 3 | 本体が A19 の効果の源に到達する | `the closure passed to 'Domain.run' must be pure: it {reason}{chain}; move that work out of the closure` | A19 の位置の規則 |
| 4 | 結果型 `'a` が `own_violation` で違反 | `the result of 'Domain.run' must be plain data; '{type}' holds {what}` | 呼び出し式 |
| 5 | target・設定が領域を持てない（2.3） | `'Domain.run' needs a native POSIX build with the system allocator: {why}` | 最初の呼び出し |

`{what}`・`{reason}`・`{chain}` は A19 の診断と同じ語彙（`a reference`・`an 'Rc' or 'Arc'`・`a value with a user Drop`・`an extern handle`・`a function, 'dyn', or a task`・`a type that is not plain data`）。捕捉に関数値を許さないのは、外から入る関数値が A19 の「到達する本体を調べる」前提を破るから（D6）。**例 3 つ**:

```tsuzuri
let work = \() -> 1 / 0
let a = Domain.run work                                  // E1038 (1): 変数へ入れずにラムダを直接渡す
let b = Domain.run (\() -> { Debug.print "x"; 1 })       // E1038 (3): ... it prints debug output
let log = \text -> Debug.print text
let c = Domain.run (\() -> { log "x"; 1 })               // E1038 (2): captures 'log', which holds a function, 'dyn', or a task
```

`Domain.run` の本体の中の `Domain.run` は、A19 の分類で不純（`DomainRun` を不純にする。D8）なので (3) になる。

#### 2.3 target と設定

| 条件 | 結果 |
| --- | --- |
| native の macOS・Linux（`Platform::unix()`）で、system allocator、`build`（exe・object・shared・header）・`run`・`script`・`test`・`bench` | 動く |
| `--emit llvm` | `--trap-mode return` が要る（`E1038`。ホストが `src/runtime/trap.c` を提供する契約。E14） |
| `--target wasm32`・`wasm64`・bare-metal（`tsuzuri test` の WASM を含む）、`--freestanding`、`--allocator host`・`counting`・`small`、native の Windows | `E1038` |

`check` と LSP は target を見ないので `Domain.run` を受け入れる。

### Phase 3: 後始末の表と facts

付録 F の表を `docs/language.md`「トラップ位置」と `exception-handling.md` に置く（測った事実だけ）。G26 の `trap_sites[].kind`（snake_case の語彙）と、`E2005` の `trap.kind`・`.trap.json` の `kind`（`TrapKind::description()`）は**別の語彙**で、対応は次の表: `assert`↔`assertion failed`、`unreachable`↔`non-exhaustive match`（`TrapKind::MatchFailure`）、`integer_division_by_zero`↔`integer division by zero`、`integer_division_overflow`↔`integer division overflow`、`bounds_check`↔`index out of bounds`、`range_step_zero`↔`range step is zero`、`overflow`↔`unhandled OverflowException: arithmetic operation resulted in an overflow`。この表も同じ文書に置く（コードは変えない）。

## 設計

### Phase 1: 変更表

| ファイル | 変更 |
| --- | --- |
| `src/runtime/backtrace.c`（新規） | 付録 B-1 の C をそのまま置く。関数は `tsuzuri_trap_backtrace(uint32_t site)` の 1 つ。`stack.c` と同じく実行ファイルだけに連結する別の翻訳単位 |
| `src/trap.rs` | `MAX_TRAP_FRAMES`・`RawTrapReport`・`TrapFrame`・`TrapReport`（`json`・`lines`）・`parse_report_line`・`display_name`・`frames_from_symbolizer`（骨格は次の節） |
| `src/diagnostic.rs` | `Diagnostic.trap`・`Diagnostic::with_trap`。`render_with_severity` は引用の後ろへ `trap.lines()` を、`json_with_severity` は末尾へ `,"trap":…` を足す（`trap` が `None` の出力は 1 byte も変えない） |
| `src/docgen.rs`・`src/repl.rs` | `Diagnostic { … }` のリテラル（各 1 か所）に `trap: None` |
| `src/llvm_traps.rs` | `InstrumentOptions`・`instrument_with`・`with_frame_pointers`。`instrument` は `instrument_with` を呼ぶだけにする（5 つの呼び出し側は変えない） |
| `src/llvm.rs` | `emit_native` が `instrument_with(…, InstrumentOptions { wasm: false, raise: false, backtrace })` を呼ぶ。`backtrace = trap_info && executable_roots && crate::target::current_platform().unix()` |
| `src/driver.rs` | `backtrace_runtime`（flag・source・object・link の 4 点。手順 5）、`run_process`・`capture` の環境変数と報告の読み取り、`symbolize` |
| `src/runtime/trap-boundary.mjs`・`tests/trap_boundary.mjs` | `createBoundary` の `frames`（1.5） |

### Phase 1: Rust の骨格（名前・型・戻り値はこのとおり）

```rust
// src/trap.rs
pub const MAX_TRAP_FRAMES: usize = 32;

/// One line of the report file of `src/runtime/backtrace.c`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawTrapReport { pub site: u32, pub slide: u64, pub truncated: bool, pub addresses: Vec<u64> }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapFrame {
    pub function: Option<String>, pub path: Option<String>,
    pub line: Option<u32>, pub column: Option<u32>,
    pub address: u64,                 // the return address minus the slide
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapReport {
    pub kind: &'static str,           // TrapKind::description(), or "stack overflow"
    pub site: u32,                    // 0 for a stack overflow
    pub frames: Vec<TrapFrame>,
    pub resolved: bool,               // a symbolizer ran
    pub truncated: bool,
}
impl TrapReport {
    pub fn json(&self) -> String;     // the object of 1.2: {"kind":…,"site":…,"frames":[…]} + ,"truncated":true
    pub fn lines(&self) -> Vec<String>;   // the human lines of 1.2, without newlines
}
pub fn parse_report_line(line: &str) -> Option<RawTrapReport>;
pub fn display_name(linkage: &str) -> Option<String>;
pub fn frames_from_symbolizer(output: &str, raw: &RawTrapReport) -> Option<Vec<TrapFrame>>;
// src/diagnostic.rs: pub struct Diagnostic { …, pub trap: Option<Box<TrapReport>> }
//                    pub fn with_trap(self, report: TrapReport) -> Self
```

`json` は `json_string`（`src/diagnostic.rs`）でエスケープし、`frames[]` のキーを 1.2 の順（`function`・`path`・`line`・`column`・`address`。`None` は省く）で出す。`address` は `format!("0x{:x}", …)`。`lines` は、frame が 0 個なら空、`resolved == false` なら note の 1 行、そうでなければ frame ごとに `  at …` と、`truncated` なら末尾の `  ... (more frames omitted)`。

### Phase 1: アルゴリズム

**A1. 子（`tsuzuri_trap_backtrace`）**。付録 B-1 の C が実装そのもの（1.1 の形式。ヒープを使わない）。(1) 環境変数が空なら何もしない。(2) `__builtin_frame_address(0)` の record を、`mincore` で読めると確かめてから読む。(3) 最初の戻りアドレス（`tz.trap.report` の中）を捨てる。(4) 1 行を `open(O_APPEND)` と `write` 1 回で追記する。

**A2. IR（`instrument_with`）**。`backtrace == false` のとき、`wasm`・`raise` の組を含めて出力は今までと 1 byte も変わらない。`backtrace == true`（`wasm == false` が前提。`debug_assert!`）のとき次を足す。(a) `@tz.trap.report` の定義行を `define internal void @tz.trap.report(i32 %site) cold nounwind noinline {` にし、`done:` の直後（`ret void` の前）へ `  notail call void @tsuzuri_trap_backtrace(i32 %site)` を足す。(b) 出力の末尾に `declare void @tsuzuri_trap_backtrace(i32)`。(c) 最後に `with_frame_pointers(&output)` を 1 回通す: すべての `define ` 行について、` !dbg ` があればその直前、なければ最後の ` {` の直前に ` "frame-pointer"="all"` を挿入する（付録 A-5 の変換と同じ規則。この形だけが効く）。

**A3. 親（`run_process`）**。(1) `output` の隣に `let report = temporary.path.join("trap-report");`。(2) 子の `Command` に `.env("TSUZURI_TRAP_REPORT", &report)`（`RunStdio::Inherit` は `program_command(…)` の連鎖、`RunStdio::Capture` は `capture` に引数 `report: &Path` を足してその中）。(3) `child.wait()` のあと `temporary.close()?` の**前**に、`!status.success()` なら `let raw = fs::read_to_string(&report).ok().and_then(|text| text.lines().find_map(parse_report_line));` と、`raw` があれば `symbolize(&binary, &raw)`（`binary` は `platform.mach_o() && options.debug_info` なら `output` に `.dwarf` を付けたパス、そうでなければ `output`）。(4) `close()` のあと、既存の `site` の探索（stderr に `site.message(sources)` がある `TrapSite`）の結果を、`Span` だけでなく `TrapSite` ごと持つ（主スパンは今までと同じ）。(5) `trap` の中身を次の順で決める: ① 探索で `TrapSite` が見つかった → `kind = site.kind.description()`、`site = site.id` ② なければ `raw` がある → `site = raw.site`、`kind` は `sites` から id で引き、なければ `TrapKind::ALL[((site - 1) % TrapKind::ALL.len() as u32) as usize]` ③ なければ stderr が `STACK_OVERFLOW_REPORT` を含む → `kind = "stack overflow"`、`site = 0` ④ それ以外は `trap` なし。(6) `frames` は `raw` が `None` なら空、`symbolize` が `Some(frames)` なら `resolved: true` でそれ、`None` なら `resolved: false` で `raw.addresses` を `TrapFrame { address: pc − slide, function: None, … }` にしたもの。`truncated = raw.truncated`。(7) `Diagnostic::new("E2005", message, span).with_trap(report)` を返す（`E2005` のメッセージ・位置は今までと同じ）。

**A4. `symbolize`（`src/driver.rs`）**。`Command::new(tool("TSUZURI_LLVM_SYMBOLIZER", "llvm-symbolizer"))` に `--obj=<binary> --output-style=JSON --inlining --functions=linkage` と、`raw.addresses` の各 `format!("0x{:x}", a.wrapping_sub(raw.slide).wrapping_sub(1))` を渡す。stdin・stderr は null、終了コードが 0 でなければ `None`。標準出力を `trap::frames_from_symbolizer` へ。**1 回だけ起動**（アドレスは 32 個まで）。

**A5. `frames_from_symbolizer`**。`serde_json`（依存済み: `Cargo.toml` の `[dependencies]`）で配列を読む。要素数が `raw.addresses.len()` と違う、または配列でなければ `None`。要素 `i` の `Symbol[]` を順に、`display_name(FunctionName)` が `Some` のものだけ `TrapFrame` にする（`path = FileName`（空文字は `None`）、`line = Line`・`column = Column`（0 は `None`）、`address = raw.addresses[i] - raw.slide`）。各候補は `(名前, cold)`（`cold = FunctionName.contains(".cold.")`）を持ち、直前に採用した frame と名前が同じで、どちらかが `cold` なら、`cold` の方を捨てる（現在が `cold` なら採用しない。直前が `cold` なら直前を現在に置き換える）。`Some(frames)` を返す（0 個でもよい）。

**A6. `display_name`**。1.3 の表を上から: (1) `tz.fn.` で始まる → その後ろを `rest` とする。(2) `tz_` で始まり後ろが空でない → その後ろを `rest` とする。(1)(2) は `rest` の末尾が `.$mono.<数字>`・`.cold.<数字>`・`.cpu.<[a-z0-9_]+>` のどれかである限り、それを取る。結果が空なら `None`、そうでなければ `Some(rest)`。(3) `tz.specialized.<数字>` → そのまま `Some`。(4) それ以外（`tz.apply.`・`tz.env.`・`tz.closure.`・`tsuzuri_main`・`main`・C の runtime・空）→ `None`。

**A7. WASM（`createBoundary`）**。1.5 のとおり。JS は付録 B-3（`displayName`・`wasmFrames`。HEAD の V8 で確かめた）をそのまま置く。`frames: false` のときは今までの経路を 1 行も通らない（`stackTraceLimit` も触らない）。

### Phase 2: 変更するもの（行ごとの表は付録 H-1）

`src/runtime/trap.c`（付録 B-2 の差分）。`std/Domain.tz`（新規。下）。`src/stdlib.rs`・`vsc/src/core.ts`（`Domain` を予約名と opt-in に足す。件数の単体テスト 53 → 54）。`src/check.rs`（`Builtin::DomainRun`・`DomainMessage`、`domain_rules` の `mod` と `validate_cpu_functions` の直後の呼び出し）。`src/polymorph.rs`（`E1022`）。`src/domain_rules.rs`・`src/llvm_domain.rs`（新規）。`src/pure_rules.rs`（A19 の切り出し 3 つと `Category::Domain`）。`src/llvm.rs`・`src/llvm_traps.rs`・`src/trap.rs`・`src/driver.rs`・`src/test_runner.rs`（B4〜B6）。

**`std/Domain.tz` の全文**（`record` と 2 つの関数。`Gpu.tz` と同じ `def`/`fn` の 2 行組）:

```tsuzuri
/// The failure of a closure that trapped inside `Domain.run`: where (`site`, the `id` of a
/// `.trap.json` site), what (`kind`, the number of the trap kind; see docs/language.md) and a text.
record Failure { site: i64, kind: i64, message: string }

/// `Domain.run body` calls `body ()` inside a failure domain: a trap in the call, or in anything it
/// calls, ends it with `Result.Error`, and what the call allocated is freed. `body` is a lambda or a
/// `pure def`, it captures plain data only, and its result is plain data (E1038).
def run :: (unit -> 'a) -> Result<'a, Failure>
fn run body = match Domain.__run body with
    | Result.Ok value -> Result.Ok value
    | Result.Error (site, kind) -> Result.Error (Failure { site: site, kind: kind, message: Domain.__message site })
```

（この形は `Failure`・`run` を利用者のコードに置いて、`__run` を `Result.Ok (body ())` の関数に替えた版が、HEAD の `tsuzuri check`・`run` で通る（`ok 42`）ことを確かめてある。付録 E。）

### Phase 2: アルゴリズム

**B1. `domain_calls(module) -> Vec<Span>`（`src/domain_rules.rs`）**。`Domain.run` の id の集合 `runs` = `function.origin.module == ModuleOrigin::Std && function.module == "Domain" && function.name.split('.').next() == Some("run")` の `module.functions` の添字。空なら空の `Vec`。それ以外は、`origin.module != Std` の各関数の本体を、明示のスタック（再帰しない）で走査し、`Call(callee, arguments)` の `callee.kind` が `Function(FunctionRef::User(id))`、`id ∈ runs` のとき、その `Call` 式の `span` を集める。`(source, start)` の順に並べて返す。

**B2. `validate(module) -> Vec<Diagnostic>`**。`domain_calls` と同じ走査で `Call` を集め（`arguments.len() == 1` でない `Call`、および `Function(User(id ∈ runs))` が `Call` の callee 以外の位置にあるものは、`'Domain.run' must be called directly with its closure; write 'Domain.run (\() -> ...)'` の `E1038`、位置はその式）、各呼び出しに 2.2 の規則 1→4 を順に当てて、最初の違反だけを診断にする。規則ごとの判定:
1. `arguments[0].kind` が `Closure(id, _)` → 本体 `id`。`Function(User(id))` で `module.functions[id].origin.parent.is_some()`（捕捉のないラムダ）→ 本体 `id`。`Function(User(id))` で `origin.pure`（A19）→ 本体 `id`（検査済みなので規則 3 を飛ばす）。それ以外は違反。
2. `Closure(id, _)` のとき、`module.functions[id].parameters[..capture_count]`（捕捉は先頭の `capture_count` 個の引数）の各 `Local` について、`ty = local.ty`（`Type::Reference(inner, false)` なら `inner`）、`pure_rules::own_violation(&ty, &module.types())` が `Some(bad)` なら違反（`{name}` = `local.name`、`{what}` = A19 の分類。`ref mut` は言語が捕捉を許さない（`E1005`）ので来ない）。
3. 本体が `Closure`/捕捉のないラムダのとき、A19 の根つき解析 `impurity_of_roots(module, &[id])` が `Some(Impurity { reason, chain, span })` なら違反（位置は `span`）。
4. 呼び出し式の型 `Type::Union(_, arguments)` の `arguments[0]` が `own_violation` で違反（`{type}` = `arguments[0].display(&module.types())`）。
診断は `Diagnostic::new("E1038", message, span)`。複数の呼び出しは位置順に、それぞれ最大 1 件。

**B3. 組み込み関数 2 つ（`src/check.rs`）**。`Builtin::DomainRun` の `name()` は `"Domain.__run"`、`DomainMessage` は `"Domain.__message"`。`signature()`（`check.rs` の `Builtin` の型の表。`Self::TaskParallelResults` の隣の書き方）:
```rust
Self::DomainRun => (
    vec![BuiltinType::Function(vec![Concrete(Type::Unit)], Box::new(a()))],
    BuiltinType::Std { module: "Result", name: "Result",
        args: vec![a(), BuiltinType::Tuple(vec![Concrete(Type::I64), Concrete(Type::I64)])] },
    Vec::new(),
),
Self::DomainMessage => (vec![Concrete(Type::I64)], Concrete(Type::String), Vec::new()),
```
（`a()`・`Concrete` は同じ関数の中の既存の局所名。`grep -n "Self::TaskParallelResults => {" src/check.rs` の周りを見て合わせる）。`Builtin::ALL`（`&'static [Self]`。`src/check.rs` の `pub const ALL: &'static [Self] = &[`）に 2 つを足す。

**B4. `emit_domain_run`**（`src/llvm_domain.rs`。詳細は付録 G-1）。`emit_typed_builtin` が `Builtin::DomainRun` のとき呼ぶ。`parallel_result_tasks`（`src/llvm_task.rs`）と同じ型で補助関数 `@tz.domain.thunk.<型>(ptr %context)` を作り、`@tsuzuri_domain_run(thunk, context, info)` を呼んで `Ok`・`Error((site, kind))` を組み立てる。閉包は `apply_value(…, borrowed = true)` で呼び、**成功・失敗の両方で** `drop_value` する（付録 C）。

**B5. `emit_domain_message` と `describe`**（付録 G-2）。`Domain.__message` は `@tz.trap.text(i32 site, ptr length) -> ptr`（`instrument_with` が、IR に `call ptr @tz.trap.text(` を見つけたときだけ定義する、site → 本文の表）と `@tz.string.from_utf8` で `string` にする。`sources` が空でも `raise` なら `instrument_with` は通し（`E2000` にしない）、本文は `kind.description()` だけになる（`TrapSite::text`）。

**B6. 経路の切り替えと driver**（付録 G-3）。`domain = !crate::check::domain_calls(module).is_empty()` を `emit_native`・`emit_test_runner_with`・`emit_bench_runner_with`・`build_complete` の先頭で見る（WASM では見ない）。`emit_trap_return` の本体を `emit_trap_mode(…, TrapMode { cpu_dispatch, levels, wrappers, executable_roots, backtrace })` へ移し（`emit_trap_return` の出力は 1 byte も変えない）、`emit_native` は `wrappers: false` で呼ぶ。driver は `domain` のとき `runtime_objects = false`、`trap_runtime` を実行ファイル・object・shared へ広げ、`-pthread` を足し、`E1038` の target 検査（2.3）をする。`test_runner.rs` は `domain` のとき `trap.c` を足す。

### Phase 3: 変更表

文書だけ（付録 F の表を `docs/language.md` と `exception-handling.md` へ写す）。コードは変えない。

## 前提とする他チケットのインターフェース

| チケット | B10 が使うもの（名前・型・形） | 未実装・衝突のとき |
| --- | --- | --- |
| E14（done） | `src/runtime/trap.c` の ABI（`tsuzuri_trap_raise(site, kind)`・`tsuzuri_boundary_run`・`tsuzuri_boundary_item`・`tsuzuri_tracked_malloc/free/realloc`・`tsuzuri_trap_info { site, kind }`）、`Instrumentation.trap_return`、`emit_trap_return`、`host_abi::try_wrappers`、`STACK_OVERFLOW_REPORT`、`<出力>.trap.json` | 変えない（意味を変える変更は停止条件 S8） |
| A19（todo）Phase 1 | `FunctionOrigin.pure: bool`、`pure_rules::own_violation(ty: &Type, types: &TypeContext<'_>) -> Option<Type>`。B10 が使うのは D-53 の「純粋性の問い合わせの窓口」の `what`・`Impurity`・`impurity_of_roots`。**A19 だけの木（A20 の経路 A の前）では B10 が `src/pure_rules.rs` に足す**: ① `pub(super) fn what(ty: &Type, types: &TypeContext<'_>) -> &'static str`（A19 の診断の `{what}` を決める分類の切り出し）② `pub(super) struct Impurity { pub reason: String, pub chain: String, pub span: Span }` と `pub(super) fn impurity_of_roots(module: &CheckedModule, roots: &[usize]) -> BTreeMap<usize, Impurity>`（A19 の「根ごとの到達可能性」の結果を診断にする前の形で返す。`validate` はこれを呼んで `E1029` を作る形に直し、A19 の出力は 1 byte も変えない）③ `Category::Domain`（D-53 では `task`。`builtin_effect(DomainRun)` は不純で reason は `runs a failure domain ('Domain.run')`、`DomainMessage` は純粋。A19 の単体テストの個数は不純 90→91・純粋 156→157）。**A20 の経路 A のあと**は窓口が `pure_rules.rs` に残るので ①② は足さず、③ の代わりに `effects::builtin_facts` と reason の表へ `DomainRun`＝`task`＋`alloc`・`DomainMessage`＝純粋を足す（`Category` は使わない） | A19 Phase 1 が done でなければ Phase 2 に着手しない（S7）。A19 の関数名が違うときは、同じ役目の関数を探して使う（役目は上のとおり） |
| G22（todo。`_features/G22-diagnostic-v2.md`） | `Diagnostic(Box<DiagnosticData>)`・`.with_kind(&'static str)`、JSON の順序（既存 5 キー → kind → related → help → expected → found → candidates → scope → fixes → `trap`。D-51） | G22 が先に入ったら `trap: Option<Box<TrapReport>>` を `DiagnosticData` の独立した field にし（`Detail` に入れない）、`render_with`・`json_with` が `trap` を出す（`trap.lines()` は引用の直後・`related` の前）。B10 が先に入ったら `Diagnostic` に直接置き、G22 の実装者が移す（D-51 に書いた） |
| G23（todo） | `E1038` の kind（`snake_case`）: `domain_not_called_directly`・`domain_argument_not_closure`・`domain_capture_not_plain`・`domain_body_not_pure`・`domain_result_not_plain`・`domain_target_unsupported`（規則 0〜5 の順） | G23 があれば S1 の手順で登録して `of` で出す。G22 だけなら `with_kind(…)`。どちらもなければ付けない（D-51） |
| G26・A20（todo） | G26: `trap_sites[].kind` は snake_case の語彙で、`E2005` の `trap.kind`（`TrapKind::description()`）とは別（対応表は Phase 3）。A20: `Builtin::DomainRun` は `task`＋`alloc`（D-53。中のトラップは閉じ込めるので `trap` は持たない） | 文書と台帳だけ。コードは変えない |

## 実装手順

**共通確認（C）**: `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings` が警告 0。手順で名指しした `cargo test --locked …` の出力の `running N tests` が書いた N。`sh target/tz-b10/identity.sh target/tz-b10/after && diff -r target/tz-b10/base target/tz-b10/after` が差分なし（`cargo build --release --locked` のあとで。手順に書いた例外を除く）。各手順の最後でビルドとテストが緑。

### Phase 1

**手順 1: ベースライン（コードは変えない）**
- 内容: 「着手条件」の基準コマンドを実行し、`cargo test --locked` の合計件数を控える。付録 A-8 の `identity` 関数を `target/tz-b10/identity.sh`（`target/` は git の管理外）に置き、`sh target/tz-b10/identity.sh target/tz-b10/base` を実行する（約 10 秒、決定的。HEAD で 2 回実行して `diff -r` が空であることを確かめた）。
- 確認: `ls target/tz-b10/base | wc -l` が 39。`tests/fixtures/trap_backtrace/Main.tz`（「現状」のソース）を作り、`target/release/tsuzuri run tests/fixtures/trap_backtrace` が「現状」の出力と同じ（`E2005`・終了コード 1）。

**手順 2: `TrapReport` と `Diagnostic.trap`（テスト先行）**
- 変更: `src/trap.rs`、`src/diagnostic.rs`、`src/docgen.rs`、`src/repl.rs`。
- 内容: 先に `src/trap.rs` へ `#[cfg(test)] mod tests` の 4 つ（`parses_a_report_line`: 1.1 の例の行が `site 2`・`slide 0x2188000`・`addresses 7 個`、`tz-trap/2 …`・16 進でない欄・33 個以上・空行は `None`。`display_name_follows_the_table`: 1.3 の表の全行。`frames_from_symbolizer_reads_the_json_and_merges_cold_parts`: 付録 A-3 の形の JSON（`Symbol` が 1 つの要素が `tz.fn.Main.level3`・`tz.fn.Main.level2` の順に 2 つ。`raw.addresses` も 2 個）が 2 frame になり、要素数の不一致は `None`、`foo` と `foo.cold.1` が続くと 1 つ。`report_renders_json_and_lines`: 1.2 の JSON と human の例と byte 単位で一致、`resolved: false` は note 1 行、`truncated`、frame 0 個）と `src/diagnostic.rs` の 2 つ（`a_diagnostic_without_a_trap_keeps_its_json_and_text`: `trap: None` の JSON が現在の文字列と一致、`a_trap_report_follows_the_span_in_json_and_text`）を書いて失敗させ、「Phase 1: Rust の骨格」と A5・A6 を実装する。`Diagnostic` に `trap` を足し、リテラル 2 か所（`docgen.rs`・`repl.rs`）に `trap: None`。
- 確認: `cargo test --locked --lib trap::tests`（N = 4）、`cargo test --locked --lib diagnostic::tests`（N は以前の件数 + 2）。C。

**手順 3: IR（`instrument_with`・フレームポインタ・`backtrace` の呼び出し）**
- 変更: `src/llvm_traps.rs`、`src/llvm.rs`。
- 内容: 先に `tests/trap_backtrace.rs`（新規）へ `backtrace_instrumentation_is_only_for_trap_info_executables` を書く（`tests/trap_locations.rs` の `tsuzuri::analyze`・`EmitOptions` の書き方）: `Entry::Console` と `trap_info = true` の `llvm::emit_native_build` の IR は、`define ` で始まる**すべての行**が `"frame-pointer"="all"` を含み、`declare void @tsuzuri_trap_backtrace(i32)` を 1 つ持つ。`Entry::Library`、`trap_info = false`、`emit_with_trap_info`、`emit_trap_return` の IR はどちらも含まない。失敗させてから A2 を実装する（`InstrumentOptions`・`instrument_with`・`with_frame_pointers`。`instrument` は薄い関数に）。
- 確認: `cargo test --locked --test trap_backtrace`（N = 1）、`cargo test --locked --test trap_locations`（件数は手順 1 と同じ）、`cargo test --locked --lib llvm`。C（`exe --trap-info` はまだリンクできないので `identity` には含めていない）。

**手順 4: `backtrace.c` とリンク、報告ファイル**
- 変更: `src/runtime/backtrace.c`（新規）、`src/driver.rs`。
- 内容: 付録 B-1 を `src/runtime/backtrace.c` に**そのまま**置く。driver に `let backtrace_runtime = options.emit == Emit::Executable && text.contains("declare void @tsuzuri_trap_backtrace(");` を `stack_runtime` の隣に足し、`stack_runtime` の 4 点（source の書き出し、`-g` のときの別 object `compile_runtime_object("backtrace-debug", …)`、通常のリンクの `-x c -std=c11 <source>`（`-g` なら `-x none <object>`）、`dwarf_sidecar` のリンクの `<object>`）を `backtrace` で複製する（付録 G-4。`-pthread` は要らない）。`git add src/runtime/backtrace.c`。テストを先に `tests/trap_backtrace.rs` へ: `report_file_lists_the_frames_of_a_trapping_chain`（`build tests/fixtures/trap_backtrace --trap-info -O0 -o <dir>/chain` を実行し、`TSUZURI_TRAP_REPORT=<dir>/report` で走らせる。シグナルで終わる。報告は 1 行で `tz-trap/1 ` で始まり、アドレスが 5 個以上（S2）、2 つ目の欄が `0x`）、`an_unset_report_variable_changes_nothing`（環境変数なしで走らせるとファイルができず、stderr は `trap: …` の 1 行だけ）、`report_file_caps_a_deep_recursion_at_32_frames`（`tests/fixtures/trap_backtrace_deep/Main.tz`: `def rec depth :: i64 -> i64`・`fn rec depth n = if n == 0 then 100 / n else depth (n - 1) + 1`・`def main :: Array<string> -> i32`・`fn main args = if depth (100 + args.length) > 0 then 1 else 0`。報告の frame が 32 個で、`<truncated>` の欄が `1`）。いずれも `#[cfg(unix)]`。
- 確認: `cargo test --locked --test trap_backtrace`（N = 4）、`sh scripts/check-runtime-includes.sh`、`cargo test --locked --lib driver`（`stack_runtime_writes_the_report_the_driver_looks_for` を含み緑）、`node tests/stack_overflow.mjs target/release/tsuzuri`。C。

**手順 5: `run` が報告を読んで `E2005` に付ける**
- 変更: `src/driver.rs`（`run_process`・`capture`・`symbolize`）。
- 内容: A3・A4。テストを先に `tests/trap_backtrace.rs` へ: `run_reports_frames_with_a_symbolizer`（`llvm-symbolizer --version` が失敗したら `eprintln!("skipped: no llvm-symbolizer")` して戻る。`run tests/fixtures/trap_backtrace -O0 --json` の stderr（JSON 1 行）の `trap.kind` が `integer division by zero`、`trap.frames` の `function` が先頭から `Main.level3`・`Main.level2`・`Main.level1`・`Main.main`、`-g` を付けると `Main.level2` に `path`・`line`・`column`、`E2005` の既存 5 キーは変わらない）、`run_reports_raw_frames_without_a_symbolizer`（`TSUZURI_LLVM_SYMBOLIZER=/nonexistent` で、`trap.frames` は 5 個以上で `function` を持たず `address` だけ、human の stderr に note の 1 行があり frame の行はない）。`tests/stack_overflow.mjs` の `run` の確認の隣に、`--json` の `JSON.parse(stderr).trap` が `{ kind: "stack overflow", site: 0, frames: [] }` に deep equal する確認を足す。
- 確認: `cargo test --locked --test trap_backtrace`（N = 6）、`cargo test --locked --test trap_locations`、`node tests/stack_overflow.mjs target/release/tsuzuri`、`node tests/script.mjs target/release/tsuzuri`、`node tests/repl.mjs target/release/tsuzuri`（既存の期待値を変えない: S3）。C。

**手順 6: WASM の `frames`**
- 変更: `src/runtime/trap-boundary.mjs`、`tests/trap_boundary.mjs`。
- 内容: 付録 B-3 の JS（`displayName`・`wasmFrames`。HEAD の V8 で確かめた）を足し、`createBoundary(module, { imports = {}, sites = [], frames = false } = {})` の `call` の中で、`frames` のとき `Error.stackTraceLimit = Math.max(limit, 33)` にして `finally` で戻し、`WebAssembly.RuntimeError` の `trap` に `trap.frames = wasmFrames(error)` を足す（`reason: "stack"` には足さない）。テスト: `tests/trap_boundary.mjs` の `-O0`・`-O3` の各ループに、`--trap-info -g` の build を足し、`frames: true` の境界で `tz_div(7n, 0n)` の `trap.frames` の最後が `function: "div"`、`-O0` では先頭が `function: "Main.div"`（実測: `[{"function":"Main.div","index":0,"offset":219},{"function":"div","index":2,"offset":306}]`。`index`・`offset` は数なので型だけ照合する）、`-g` なしの build では `function` を持たず `index`・`offset` だけ、`frames` を渡さない既存の `deepEqual` が変わらないことを確かめる。
- 確認: `node tests/trap_boundary.mjs target/release/tsuzuri`（exit 0。既存の `cases` 件数 + 追加分が出力される）。C。

**手順 7: 文書（Phase 1）**
- 変更: 「ドキュメント」の Phase 1 の行。確認: `node scripts/check-docs.mjs <変更したページ>`。

**手順 8: Phase 1 の最終確認**
- 内容: GUIDE §3.1 の回帰 4 コマンドと `cargo test --locked`、`node tests/trap_return.mjs`・`trap_boundary.mjs`・`stack_overflow.mjs`・`e2e.mjs target/release/tsuzuri`、`sh scripts/check-runtime-includes.sh`。**最適化 `-O3`** でも `tests/fixtures/trap_backtrace` を `run --json` して、`trap.frames` が `[]`（4 つの関数が `tsuzuri_main` へ inline され、名前のある frame がない。付録 A）で、`E2005` の既存 5 キーと `trap.kind` が変わらないことを確かめる。確認: 受け入れ条件の Phase 1 がすべて ✓。

### Phase 2（A19 Phase 1 が done であること。S7。各手順の「変更」「内容」「確認」は付録 H-2）

| 手順 | 題名 | 変更 | 確認の要点 |
| --- | --- | --- | --- |
| 9 | `trap.c`（C）テスト先行 | `src/runtime/trap.c`、`tests/domain_runtime.c`（新規） | `domain runtime: 18041 checks passed`（`-O0`・`-O3`・ASan+UBSan・TSan）、E14 の C と `trap_return.mjs`・`tasks.mjs` が緑 |
| 10 | std `Domain` と組み込み関数 2 つ（検査まで） | `std/Domain.tz`（新規）、`src/stdlib.rs`、`vsc/src/core.ts`、`src/check.rs`、`src/polymorph.rs`、`src/llvm.rs`（`unreachable!`） | `RESERVED_MODULES.len()` が 54、`cd vsc && npm test`、`tests/fixtures/domain` が `check` を通る |
| 11 | `E1038`（テスト先行） | `src/domain_rules.rs`（新規）、`src/check.rs`、`src/pure_rules.rs`、`tests/domain.rs`（新規） | `cargo test --locked --test domain`（N = 2）、`--test pure_functions` が変わらない |
| 12 | `describe`・`TrapSite::text`・`emit_trap_mode`（出力不変） | `src/trap.rs`、`src/llvm_traps.rs`、`src/llvm.rs` | `identity` が差分なし（`.ret.ll`・`.ret.o` を含む）、`trap_return.mjs` |
| 13 | codegen と経路の切り替え | `src/llvm_domain.rs`（新規）、`src/llvm.rs` | `--test domain`（N = 3）、`identity` |
| 14 | driver・test runner・E2E | `src/driver.rs`、`src/test_runner.rs`、`tests/domain.mjs`・`tests/domain_host.c`・fixture 2 つ（新規） | `node tests/domain.mjs`、`--test domain`（N = 5）、`trap_return*.mjs`・`tasks.mjs`、`--test test_runner` |
| 15 | 文書（Phase 2） | 「ドキュメント」の Phase 2 の行 | `check-docs.mjs` |
| 16 | 最終確認 | — | GUIDE §3.1 の回帰 4 コマンド、`cargo test --locked`、`TSUZURI_TSAN=1 node tests/trap_return_sync.mjs`、`sh scripts/check-runtime-includes.sh` |

### Phase 3

**手順 17: 後始末の表と facts との対応（文書だけ）**
- 内容: 付録 F の表を `docs/language.md`「トラップ位置」と `exception-handling.md` に写し、`E2005` の `trap.kind` が `.trap.json` の `kind` と同じ文字列であることを `docs/language.md` に書く。確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/exception-handling/exception-handling.md`。

## テスト計画

期待値は付録 A・B の独立した実測（`nm`・`llvm-symbolizer` を別に走らせた結果、C の `domain_runtime.c`）から作る。拒否のテストは診断コードを照合する。

| ファイル::テスト | 入力 | 期待 |
| --- | --- | --- |
| `src/trap.rs::tests::{parses_a_report_line, display_name_follows_the_table, frames_from_symbolizer_reads_the_json_and_merges_cold_parts, report_renders_json_and_lines}` | 手順 2 | 手順 2 |
| `src/diagnostic.rs::tests::{a_diagnostic_without_a_trap_keeps_its_json_and_text, a_trap_report_follows_the_span_in_json_and_text}` | `trap: None` の診断、`trap` つきの診断 | 前者は現在の文字列と一致。後者は 1.2 の JSON・text と一致（キーは `span` の後ろ） |
| `tests/trap_backtrace.rs::backtrace_instrumentation_is_only_for_trap_info_executables` | 手順 3 | 手順 3。IR を 2 回出して一致（決定性） |
| 同 `report_file_lists_the_frames_of_a_trapping_chain` / `an_unset_report_variable_changes_nothing` | fixture の chain | 手順 4 |
| 同 `report_file_caps_a_deep_recursion_at_32_frames` | `tests/fixtures/trap_backtrace_deep`（`depth 100` から `100 / 0`）、`-O0` | 報告の frame が 32 個、`<truncated>` が `1` |
| 同 `run_reports_frames_with_a_symbolizer` / `run_reports_raw_frames_without_a_symbolizer` | 手順 5 | 手順 5 |
| `tests/trap_boundary.mjs`（追加）・`tests/stack_overflow.mjs`（追加） | 手順 5・6 | 手順 5・6。`frames` を渡さない既存の `deepEqual` は変えない |
| `tests/domain_runtime.c`（`tests/trap_return.mjs` が 4 つのフラグで組む） | 付録 B-2 | `domain runtime: 18041 checks passed`（`-O0`・`-O3`・ASan+UBSan・TSan） |
| `tests/domain.rs::{domain_accepts_lambdas_pure_defs_and_plain_captures, domain_rejects_each_rule_with_e1038}` | 2.2 の表の規則 0〜4 の各 1 つ以上と 3 つの例、受理は `Domain.run (\() -> 1)`・`pure def` の名前・`ref` を捕捉するラムダ | 拒否は `E1038` で、メッセージに規則ごとの語（`directly`・`lambda or the name`・`captures`・`must be pure`・`must be plain data`）。受理は診断 0 |
| 同 `domain_ir_is_deterministic_and_marks_the_runtime` / `domain_target_errors` / `programs_without_domain_keep_their_ir` | 手順 13・14 | `E1038`（WASM・`--freestanding`・`--allocator counting`・`--emit llvm`）。`Domain` のない IR に `tsuzuri_domain_run` がない |
| `tests/domain.mjs`（`-O0`・`-O3`） | 2.1 の 2 つの例（`run`・`build`）と `tsuzuri test`、`tests/domain_host.c` + `tests/fixtures/domain_host`（`--emit llvm --trap-mode return`）の 10,000 回の成功・失敗の交互 | `average 9`・`failed (kind 1)`、`kind == 3` のテストが通る、ホストの境界の中の入れ子、`live == 0`（ASan でも） |

**既存テストへの影響**: `src/stdlib.rs` の `RESERVED_MODULES.len()` の期待値 53 → 54（`Domain` を足すため）。A19 の `src/pure_rules.rs` の単体テストの個数 90/156 → 91/157（`DomainRun` が不純、`DomainMessage` が純粋）。`vsc/src/test/core.test.ts` は `libraryModules` に `Domain` を足せば通る。それ以外は**なし**: `--trap-info` なしと `Domain` なしの出力は 1 byte も変わらず（`identity` と `programs_without_domain_keep_their_ir`）、`E2005` の既存のメッセージ・キーも変わらない（手順 5 の `script.mjs`・`repl.mjs`）。資源上限: frame 32 個（上のテスト）、報告 1 行 1,024 byte、`Domain` の確保量には上限がない（対象外）。

## ドキュメント

更新する（verify: `node scripts/check-docs.mjs <変更したページ>`。新しい例は `run=` が通る形で書く）:
- `_tsuzuri/language-reference/built-in-types-and-modules/domain.md`（新規。2.1 の例と `run=` の出力、`Failure` と `kind` の番号の表（付録 E）、`E1038` の 3 例、閉じ込めないもの、性能、「確保した block を解放し、`Drop`・`finally` は走らない」）と、`_tsuzuri/language-reference/index.md` の `Mutex` の行の隣にリンク。
- `_tsuzuri/language-reference/compiler/debugging.md`（バックトレース、`TSUZURI_TRAP_REPORT`、`at` の行、`-g`、`-O3` で frame が空、symbolizer がないとき）、`diagnostics.md`（`E2005` の `trap` オブジェクトの全キー、`E1038` の行）、`option.md`（環境変数の表に `TSUZURI_LLVM_SYMBOLIZER`）、`webassembly.md`（`createBoundary` の `frames`）、`exception-handling/exception-handling.md`（「契約違反はトラップ」に `Domain` と、後始末の表（付録 F））。
- `docs/language.md`（「トラップ位置」の節: バックトレース、`Domain`、`kind` の番号の表（付録 E）、後始末の表）、`docs/architecture.md`（トラップの節に `backtrace.c`・`tsuzuri_domain_run`・`tz_untrack_chain`、ファイル表に `src/domain_rules.rs`・`src/llvm_domain.rs`）、`README.md`（「実行時トラップ報告」の段落と `--trap-info` の行）、`docs/stability.md`（診断の JSON のキーが増える例として `trap` を 1 行）。
- 台帳（統括者）: 「報告」に書く項目（環境変数 2 つ、`E1038` の kind、D-51 の `trap`、std モジュール名 `Domain`）。このチケットでは `_features/README.md`・`GUIDE.md` を変えない。

## 受け入れ条件

**Phase 1**
- [ ] `tsuzuri run tests/fixtures/trap_backtrace -O0` の出力と `--json` の `trap` が 1.2 と一致する（`-g` で `Main.level2 (./Main.tz:5:15)`）。
- [ ] `--trap-info` なし・実行ファイル以外の出力が 1 byte も変わらない（`identity` が差分なし）。`TSUZURI_TRAP_REPORT` なしの実行ファイルの stderr は今までと同じ。
- [ ] `llvm-symbolizer` がないとき、`trap.frames` は `address` だけで、human の出力は note の 1 行。`-O3` は frame が空。frame は 32 個まで（`truncated`）。
- [ ] `tests/trap_boundary.mjs` の既存の `deepEqual` が変わらず、`frames: true` で frame が付く。手順 8 がすべて緑（GUIDE §3.1 の回帰 4 コマンドを含む）。

**Phase 2**
- [ ] `Domain.run (\() -> 1 / x)` が `Result.Error`（`kind` 1）を返してプロセスが続く。2.1 の例の出力が `average 9`・`failed (kind 1)`。`tsuzuri test` の `Domain.run` が通る。
- [ ] `E1038` の規則 0〜5 がすべて 2.2 のメッセージで診断され、受理される形は診断 0。
- [ ] 成功・失敗を 10,000 回交互に実行して `live == 0`（`-O0`・`-O3`・ASan）。ホストの境界の中の入れ子が動く。`domain_runtime.c` が 4 つのビルドで通る。
- [ ] `Domain` のないプログラムの IR・object・実行ファイルが 1 byte も変わらず、E14 の既存の確認が緑。`trap.c` が `zig cc -target` の Windows・Linux（glibc・musl）で警告なしにコンパイルできる。

**Phase 3**
- [ ] 後始末の表が `docs/language.md` と `exception-handling.md` にあり、`node scripts/check-docs.mjs` が通る。

## 落とし穴

- `"frame-pointer"="all"` は各 `define` の属性だけが効く（module flag・`-fno-omit-frame-pointer` は IR の入力に効かない）。検出: `grep -c '"frame-pointer"="all"' <IR>` と `grep -c '^define ' <IR>` が同じ。
- `address` は戻りアドレス − slide。symbolizer へ渡すのは `− slide − 1`（呼び出しの前の命令）。1 を引き忘れると次の行の関数名になる。
- `-O3` では 4 つの関数が inline されて frame が空になる。テストは `-O0`。
- 新しい `include_str!("runtime/…")` の C は `git add` する。検出: `sh scripts/check-runtime-includes.sh`。
- `sigsetjmp` は Windows にない。`TZ_SETJMP` の `#if defined(_WIN32)` を消さない。検出: `zig cc -target x86_64-windows-gnu -std=c11 -c src/runtime/trap.c`。
- `apply_value` を `borrowed = false` にすると、閉包の環境（外の block）をドメインの中で解放し、トラップで外の block を失う。閉包の `drop_value` をどちらかの経路で忘れると漏れる。検出: `domain_host` の `live`。
- `domain` のとき `runtime_objects` を `false` にしないと `emit_program` が `E2000` にする。`RESERVED_MODULES` を足したら `vsc/src/core.ts` と `src/stdlib.rs` の件数の単体テストを一緒に直す。
- 孫プロセスが `TSUZURI_TRAP_REPORT` を引き継ぎ、先に報告を書くことがある（親は最初の行を使う。既知の制限）。`TrapSite::text` に絶対パスを入れない（`TrapSource.path` をそのまま使う。決定性）。

## 対象外

巻き戻し・例外・`?`（D-10）。スタック枯渇の回復と、枯渇した stack の frame。ドメインの確保量・時間の上限。WASM と Windows の `Domain`、Windows のバックトレース。`tsuzuri test`・`bench` のバックトレース。`toolchain info`・`--help` への `llvm-symbolizer` の追加（`tests/toolchain.rs` の行の期待値が変わる。VSIX の toolchain への同梱（`scripts/toolchain/bundle.mjs`・`vsc/src/toolchain.ts`）と一緒に別チケット）。`Domain.run` の入れ子。Linux の実機での測定（S2 で実装者が確かめる）。

## 決定事項

すべて承認済み（D-50）。

- **D1（Phase 2 の方式）**: 同一プロセスで `sigsetjmp`/`siglongjmp` による閉じ込め。安全性は A19 の純粋性と「捕捉・結果は平たいデータ」で担保する（付録 C）。理由: `trap.c`・`llvm_traps.rs`・`heap-*.ll`・`task.c` を読み、E14 の追跡確保と入れ子の frame で、試作が 18,041 の C の確認と 10,000 回の `live == 0` を通った（付録 B）。却下: 子プロセス方式（`fork`＋`exit`＋`wait` だけで 1 回約 2.7〜3.6 ms で、ドメインの入口の約 40 ns の 6 桁上。値の直列化も要る。「計算 1 件の失敗」には過大）、設計のみ（成立するので実装する）。
- **D2（確保）**: E14 の追跡 block を frame ごとの owner で持つ。bump arena は却下（解放のたびに再利用できず伸び続ける。結果を領域の外へ型ごとに複製する必要がある）。
- **D3（`sigsetjmp`）**: `trap.c` 全体を `sigsetjmp(env, 0)`/`siglongjmp` にする（macOS の `setjmp` は信号マスクを保存し、空のドメインで 370 ns → 40 ns）。Windows は `setjmp`（`TZ_SETJMP`）。却下: `_setjmp`（非標準）。
- **D4（名前の解決）**: 子は戻りアドレスだけを書き、親が `llvm-symbolizer` で名前と行にする。子の中の `dladdr`・`backtrace_symbols` は却下（glibc の `dladdr` は動的 symbol だけを引く既知の挙動。`-g` の行が引けない。子に解析を入れない）。`atos` の代替は macOS 限定で行だけなので採らない。
- **D5（フレームポインタ）**: 各 `define` の `"frame-pointer"="all"`（実行ファイルの `--trap-info` だけ）で鎖を辿る。却下: `_Unwind_Backtrace`・`backtrace()`（unwind 表のコードが増え、musl で使えず、どちらも frame pointer か表に依る）。
- **D6（検査の根拠）**: `Domain.run` の本体の純粋性は A19 の検査を使い、B10 は同じ検査を作らない。捕捉に関数値を許さない（A19 の到達する本体の前提を守る）。
- **D7（`Failure`）**: `site: i64`・`kind: i64`・`message: string`。`kind` は `TrapKind` の数（`tsuzuri_trap_info.kind` と同じ。並びは変えない）。却下: 文字列の `kind`（文言は保証外）、`TrapKind` を写した union（std が `TrapKind` と同時に変わる）。
- **D8（`E1038`）**: 1 コード・6 つの kind（G23）。`Domain.run` の中の `Domain.run` は不純（A19 の分類）で規則 3 になる。入れ子は v1 では拒否（C の入れ子は動くが、失敗の領域が二重になる意味を決めない）。
- **D9（報告の運び方）**: 環境変数 `TSUZURI_TRAP_REPORT` のファイル。stderr に frame を足すのは却下（`run`・`script` の既存の出力と、配布した実行ファイルの利用者の stderr が変わる）。
- **D10（Phase 3）**: 後始末の表には測った事実だけを書き、`Drop`・`finally` を走らせない（巻き戻しは採用しない改善）。

## 付録

[B10-appendix.md](B10-appendix.md): A 実測（chain、symbolizer の JSON、WASM の V8、frame pointer）、B-1 `backtrace.c`・B-2 `trap.c` の差分と `domain_runtime.c`・B-3 WASM の JS・B-4 ホストの C、C 安全性の議論、D `E1038` の例と診断の一覧、E `std/Domain.tz` と `TrapKind` の番号、F 後始末の表、G Phase 2 の codegen・経路・driver の詳細、H Phase 2 の変更表と手順 9〜17 の詳細。


