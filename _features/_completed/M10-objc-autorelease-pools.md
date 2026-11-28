# M10: tzroutine と `main` の autorelease pool（`ObjC.with_autorelease_pool`）

| 項目 | 内容 |
| --- | --- |
| ID | M10 |
| 優先度 | P2 |
| 規模 | M（Phase 1・2） |
| 依存 | [M01](M01-objc-class-types.md), ([M02](M02-objc-typed-message-send.md)) |
| 後続 | – |
| 状態 | done（2026-11-29。Phase 1・2。組み込みの関数 `ObjC.with_autorelease_pool :: (unit -> 'a) -> 'a`（スケジューラーを使うプログラムでは push の前から pop の後までスレッドに固定する。固定する境界で、効果は `extern`、権限は `ffi`）、Objective-C を使うプログラムの実行ファイルの `@main` の pool（pop は tzroutine の境界の待ちの後で `@tz.main.done` の前）、Objective-C のイメージの tzroutine のランタイムだけが回す実行区間の pool（`-DTZR_OBJC_POOLS`。yield と終了では tzroutine が次へ進む前に pop する）。W14 の `i64` の結果の素の retain は、呼び出しと同じブロッキングの包みの中に入れた。Objective-C を使わないプログラムの IR とランタイムは変わらない。「実装と検証（2026-11-29）」） |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 4（プールのない tzroutine で Foundation を使うと永久に漏れる。200 万個で maxrss が 5.6 から 51.8 MiB。Foundation をリンクした -O3）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](../GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 1（X′）は承認済み（2026-11-26）。要承認（D-55（`ObjC.with_autorelease_pool`）。Phase 2（ランタイムが自動でプールを回す）は別に承認する）。決定事項の D3・D4 が要承認、ほかは既定案。D3・D4 は 2026-11-29 に D-67 の委任（利用者の指示「実装完了までに複数フェーズある場合には、すべてのフェーズを完了させること」「現在の最新仕様と齟齬がある内容がある場合には、実装前に方針を再検討して最新化して」「何か判断が必要なものがあれば、あなたが考える最高の選択肢で実装することを常に許可します」）で決め、着手前のレビューで D8〜D11 を足した。台帳は [GUIDE D-67](../GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の「委任による決定（2026-11-29、M10）」 |
| 改善する劣位 | Objective-C（`@autoreleasepool { }`）・Swift（`autoreleasepool { }`）比: tzroutine の中で Foundation の便利なコンストラクタや +0 の結果を受けると、プールのない OS スレッドで autorelease された物体が永久に漏れる（200 万個で maxrss が 5.6 MiB から 51.8 MiB）。プールを置く言語の手段がなく、SDK のヘッダーに宣言のない `objc_autoreleasePoolPush`・`Pop` を `extern` で宣言し、`Routine.with_locked_thread` の中で対にする手書きの回避策しかない。固定を忘れると、待った後に別のスレッドで pop して abort する |
| 手本にする既存実装 | `Routine.with_locked_thread` の下げ方（`src/llvm.rs:7371-7393` の `routine_builtin`、`tsuzuri_tz_lock_thread`・`unlock`）と効果の辺の除外（`src/effects.rs` の `pinning_argument_mask`、`src/pinned_rules.rs:33-35` の `Position::Pinning`。D-53）。生成した `@main` の `TZROUTINE_SCOPE_ENTER`・`LEAVE` を足す所（`src/llvm.rs:9994-10006`・`10017-10018`）。ランタイムの機能のマクロ（`src/driver.rs:4915-4923` の `-DTZ_TRAP_BOUNDARY`・`-DTZ_STACK_GUARD`・`-DTZ_ASYNC_PREEMPT`） |
| 主な影響ファイル | `src/check.rs`, `src/llvm.rs`, `src/llvm_objc.rs`（M01 が作る ObjC の生成）, `src/effects.rs`, `src/pure_rules.rs`, `src/kg/effects.rs`, `src/pinned_rules.rs`, `src/capabilities.rs`, `src/driver.rs`（Phase 2）, `src/runtime/tzroutine.c`（Phase 2）, `tests/objc_pools.rs`（新規）, `tests/objc_pools.mjs`（新規）, `tests/fixtures/objc_pools/Main.tz`（新規）, `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）, `_tsuzuri/language-reference/built-in-types-and-modules/routine.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `docs/language.md`, `docs/architecture.md` |

## 改訂（2026-11-29。着手前のレビュー、HEAD `a198b995`）

起票（HEAD `804b7970`）の後に M01・M02・M09 が入り、次の点が HEAD と食い違っていたので、実装の前に改めた。下の本文のうち、この節と食い違う記述はこの節が正である（決定事項の D8〜D11）。

- **早見の例は漏れの検査にならない**: `+[NSMutableArray array]` は +0 の結果だが、M01 の D6 の bundle（`clang.arc.attachedcall`）でハンドシェイクが効き、物体はプールを経ない。プールで包まずに `Routine.with_locked_thread` の中で 200 万回呼んでも、maxrss は 5.6 MiB で `MISSING POOLS` は 0 行だった（`-O3`、Foundation をリンク。実測）。早見の「+0 の結果なので、M02 が直後に retain しても物体はプールを経る」は、手書きの `extern` の送信（再現 3）にだけ当たる。漏れの検査は、ARC なしのホスト（`tests/objc_pools_host.m`）が確実に autorelease する物体で行う（D8）。
- **Phase 2 の pop は任意の `dealloc` を g0 の上で走らせる**: M のスレッドのスタックは 256 KiB（`tzr_startm`。「g0 runs only the scheduler」）で、G の既定（1 MiB）より小さい。`TZR_OBJC_POOLS` のランタイムでは M のスタックを 1 MiB にする（D9）。
- **pop の位置**: 「G から g0 に戻った直後」ではなく、yield と終了では `after` の前、待ちでは `after`（R1 の待ちのロックの解放）の後にする（D10）。待ちのロック（`tzr_lock`）を持ったまま `dealloc` を走らせない。yield と終了では `after` まで誰も G を走らせられないので、G が次へ進む前（終了では、境界へ終了が伝わる前）に解放が済み、振る舞いが決定的になる。
- **停止条件 S4 の決め方**: 「残りの窓を横取りの対象から外す」を採った（D11）。W14 の `i64` の結果の fallback の素の retain を、`host_call` が自分で出す `tsuzuri_tz_blocking_enter`・`_exit` の中に、呼び出しと一緒に入れる。包みの中では非同期の横取りが起きず（`syscall_depth > 0`）、`tsuzuri_tz_blocking_exit` は G を M から離さない（`tzr_exitsyscall` は P を待つだけ）。非同期の横取りは止めない。利用者が手で書いた `i64` の id は、文書の規律（`ObjC.with_autorelease_pool` の中に置くか、型付きの宣言に書き換える）で扱う。型付きの宣言だけのプログラムは、内部ポインター（`const char *` の結果など）を型で持てない（`ref ZUtf8String` は引数だけ）ので、区間の pop で壊れる値を持たない。
- **`emit_typed_builtin` の `Globals`**: 「`objc_used` を戻す処理を足す」ではなく、`objc` の表そのものを共有の `Globals` から持ち込んで戻す。selref と定数の NSString の番号が続き、release だけをする組み込みの本体（`ignore` など）の `declare` も残る。
- **行番号と文書の場所**: `src/` の行番号は M01・M02・M09 でずれている（`routine_builtin` は `src/llvm.rs` の 8155 行付近、`console_main` は 10895 行付近、`Builtin::ALL.len()` は 397 → 398）。「ドキュメント」の `_tsuzuri/language-reference/...` は別リポジトリ Tsuzuri-Reference の `docs/compiler/objective-c.md`・`docs/stdlib/sync/routine.md`・`docs/compiler/native-interop.md` に読み替える。

## 実装者向け早見

- **人間が求めない限り Phase 1 だけを実装する。** Phase 2 は別の承認（決定事項 D3）も要る。
- 範囲（Phase 1）: 組み込みの関数 `ObjC.with_autorelease_pool :: (unit -> 'a) -> 'a` を足す。tzroutine の中では `Routine.with_locked_thread` と同じくスレッドに固定してから `objc_autoreleasePoolPush` し、本体の後で `objc_autoreleasePoolPop` して固定を外す。ObjC を使うプログラムでは、生成した `@main` が `tz.fn.<Module>.main` の呼び出しをプールで包む（Xcode の `main` の `@autoreleasepool` と同じ）。
- 最初に読む: ① 「現状」の再現 1〜3（漏れ、固定しない pop の abort、+0 の結果もプールを経ること）② `routine_builtin`（`src/llvm.rs:7371-7393`）③ `console_def_main`・`console_main`（`src/llvm.rs:9968-10013`・`10055-10173`）④ W19 の `Position::Pinning` と `pinning_argument_mask`。
- 最初に書くテスト: `tests/objc_pools.rs` の `pool_builtin_pins_only_where_the_scheduler_runs`（スケジューラーを使うプログラムだけに `tsuzuri_tz_lock_thread` が出る）と `main_pool_only_in_programs_that_use_objc`（`llvm::emit(&module, Entry::Console)` の `@main`）。次に `tests/objc_pools.mjs` の `leak`。
- 触ってはいけない: ObjC を使わないプログラムの IR・object・header・診断・既定の `.wasm`（1 byte も変えない）。利用者が `extern "objc_autoreleasePoolPush"` を自分で宣言しただけのプログラムの `@main`。`Routine.with_locked_thread` の意味。tzroutine のランタイム（Phase 1 では変えない）。
- 完了の見分け方（Phase 1）: 次のプログラムを Foundation とリンクすると、`-O0`・`-O3` で maxrss 20 MiB 未満で終わり、`OBJC_DEBUG_MISSING_POOLS=YES` で `MISSING POOLS` を 1 行も出さない。`+[NSMutableArray array]` は +0 の結果なので、M02 が直後に retain しても物体はプールを経る。今は手書きの宣言で同じこと（「現状」の再現 3）をプールなしで動かすと maxrss は 113 MiB、手で push と pop を対にすると 5.6 MiB である。

```tsuzuri
@trusted
extern class NSMutableArray : ObjC.Object {
    class array :: unit -> Self
}

def churn :: i64 -> unit = \count ->
    for _ in 1 ..= count / 1000 do
        ObjC.with_autorelease_pool (\() ->
            for _ in 1 ..= 1000 do
                NSMutableArray.array () |> ignore)

def main :: unit -> i32 = \() ->
    tzroutine {
        churn 2000000
    }
    0
```

## 目的

Cocoa の物体の多くは autorelease pool を前提にしている。便利なコンストラクタ（`+[NSString stringWithFormat:]`、`+[NSMutableArray array]`）と +0 の結果は、物体を今のスレッドのプールに入れて返す。AppKit のメインスレッドは run loop がイベントごとにプールを回すが、tzroutine が走るワーカーのスレッドにはプールがない。プールのないスレッドで autorelease された物体は、libobjc が `MISSING POOLS ... just leaking` と言ってそのまま漏らす。ワーカーの M は終わらないので、スレッドの終わりにも回収されない。

M01 の `extern def` と M02 の型付きの送信は、+0 の物体の結果を M01 の D6 の形（呼び出しに `clang.arc.attachedcall` の bundle を付け、直後に `objc_retainAutoreleasedReturnValue` が走る）で retain する。ハンドシェイクが効けば、返された物体そのものはプールを経ない（M01 の「現状」8）。それでも、呼ばれた側の中で autorelease された物体（便利なコンストラクタの一時の物体、Foundation の内部の物体）はプールに入る。bundle を付けられない W14 の `i64` の結果の fallback（M01 の D6）と、今日の手書きの `extern` の送信と retain の組（「現状」の再現 3）では、返された物体もプールに入る。つまり、M01・M02 で ObjC の物体を tzroutine から扱うと、プールがなければ送信のたびに漏れうる。

このチケットは、プールを置く言語の手段 `ObjC.with_autorelease_pool` を足す。プールはスレッドごとの LIFO で、tzroutine は待つたびに別の OS スレッドで続くことがある（routine.md:130、`src/runtime/tzroutine.c:11-12` の R2）。そこで、`Routine.with_locked_thread` と同じく、本体の間はスレッドに固定する。あわせて、`main` の run loop より前の区間（OS のメインスレッドでもプールがない）を `@main` のプールで包む。Phase 2 は、ランタイムが tzroutine の実行の区間ごとに自動でプールを回す案で、別に承認する。

## 着手条件と停止条件

### 着手条件

- **人間が求めない限り Phase 1 だけを実装する。** **承認が記録されるまで、該当する Phase に着手しない。** D-67 の分岐 1（X′）は承認済み（2026-11-26）。Phase 1 は決定事項 D4（D-55 の登録）、Phase 2 は加えて D3 の承認が要る。
- [M01](M01-objc-class-types.md) が done（組み込みのモジュール `ObjC`、ObjC を使うプログラムの判定、`-lobjc`、`abi_apple_only`）。[M02](M02-objc-typed-message-send.md) は弱い依存で、Phase 1 と同じ時期に入れる（+0 の結果が tzroutine のプールで漏れるため）。Phase 2 は M02 の Phase 1 が done であること（安全の根拠が M02 の直後の retain だから）。確認: `grep -nE "^\| M0[12] " _features/README.md`。
- 「現状」の再現を着手時の HEAD で実行し、食い違えばこのチケットを直してから始める。
- ベースライン: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm`（native、`--target aarch64-apple-darwin`、`--target wasm32`）、`--emit header`、`check --json`、`--emit exe` の `@main`（Rust のテストで `llvm::emit(&module, Entry::Console)`）を `target/scratch/m10/base/` に保存する。M01 の fixture（ObjC を使う）も含める。

### 停止条件

- **S1** ObjC を使わないプログラム、または利用者が `extern "objc_autoreleasePoolPush"` を宣言しただけのプログラムの IR・object・header・診断・既定の `.wasm` が、ベースラインと 1 byte でも違う。
- **S2** 「ObjC を使うプログラム」の判定が、利用者が手で書いた `extern "objc_*"` の宣言だけで真になる（M01 の `uses_objc` はコンパイラだけが出すモジュールのフラグ `!"Objective-C Version"` で決めるので、その条件から外れたら止める）。
- **S3** `ObjC.with_autorelease_pool` を `Position::Pinning` にするのに、W19 の解析（`pinned_rules`・`pinning_argument_mask`）の形を変える必要がある。
- **S4**（Phase 2）TZ10 の非同期の横取り（`TZ_ASYNC_PREEMPT`）が送信とその直後の retain の間で G を止めうることを、横取りの規則を変えずに防げない（落とし穴）。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

確かめた環境は macOS 27.2.1（arm64）、Apple clang 21.0.0、SDK 27.2、`target/debug/tsuzuri` である。

- **プールの扱いはどこにもない**: `grep -rli objc src/runtime` は 0 件。`ObjC.with_autorelease_pool (\() -> 0)` は `E1002`（`unknown value 'ObjC'`）。
- **libobjc と SDK**: `objc_autoreleasePoolPush`・`objc_autoreleasePoolPop` は `/usr/lib/libobjc.A.dylib` が export する（`dyld_info -exports`）が、SDK の `usr/include` と Foundation・CoreFoundation のヘッダーに宣言はない（`grep -rl objc_autoreleasePoolPush` で 0 件）。clang の `@autoreleasepool` が使う ABI である。
- **スレッドの移動**: tzroutine は待つたびに別の OS スレッドで続くことがある（`_tsuzuri/language-reference/built-in-types-and-modules/routine.md:130`、`src/runtime/tzroutine.c:11-12` の R2）。プールはスレッドごとの LIFO である。
- **`Routine.with_locked_thread`**: 型は `(unit -> 'a) -> 'a`（routine.md:25、`src/check.rs:4091-4098`）。下げ方は `routine_builtin`（`src/llvm.rs:7371-7393`）で、native では `call void @tsuzuri_tz_lock_thread()`、本体の適用、`call void @tsuzuri_tz_unlock_thread()`、closure の drop の順。`'a` が `unit` のときに生成した IR は `define internal i8 @tz.builtin.Routine.with_locked_thread.unit(%tz.closure %arg0)` である。ランタイム（`src/runtime/tzroutine.c:2196-2208`）は、スケジューラーの M でなければ（擬似 G。`main` のスレッドとホストのスレッド）何もしない。固定された G は待つときも自分の M のスレッドで眠り（`tzr_park_locked`、`src/runtime/tzroutine.c:1561-1565`）、その間ほかの G はそのスレッドで走らない。
- **効果と固定の位置**: `builtin_facts` の `RoutineWithLockedThread` は効果 `task` と callbacks（`src/effects.rs:548-550`）。`pinning_argument_mask`（`src/effects.rs:2199` から。`RoutineWithLockedThread | FnPtrOf` は 2216 行と 2225 行）と `pinned_rules::builtin_position`（`src/pinned_rules.rs:33-35`）が `RoutineWithLockedThread | FnPtrOf` を固定する境界として扱い、引数の関数の `pinned` を呼び出し元に広げない。固定する境界の一覧は `_tsuzuri/language-reference/compiler/native-interop.md:224` にある。
- **ランタイムの連結**: driver は IR に `tsuzuri_tz_` の記号があるときだけ tzroutine のランタイムを連結する（`src/driver.rs:4246-4251`。D-61）。`with_locked_thread` は tzroutine を起動しないプログラムでも `tsuzuri_tz_lock_thread` を出すので、ランタイムを連結する。スケジューラーを使うかどうかは `src/llvm.rs:1645-1646` の `spawning`（`spawns(module) || (!wasm && scheduler_parallel(module))`）。
- **生成した `@main`**: `console_main`（`src/llvm.rs:10055-10173`、呼び出しは 2344-2351 行）は 3 つの形を作る。`def main` は `console_def_main`（9968-10013 行）で `define i32 @main(...) { entry: {enter}{body}  %code = call i32 @tz.fn.<main>(...) {leave}{done}  ret i32 %code }`（10006 行）。IO の入口は `%result = call i32 @tsuzuri_main()`（10069-10078 行。tzroutine の待ちは `tsuzuri_main` の中）、値を返す `main` は `%result = call <型> @tz.fn.<main>()`、`{leave}`、`{done}` の後に値を表示する（10097-10106 行）。`{enter}`・`{leave}` は tzroutine を起動するプログラムだけの `TZROUTINE_SCOPE_ENTER`・`LEAVE`（10017-10018 行）、`{done}` は `@tz.main.done` の store。`--emit llvm` は `Entry::Library` の形（`tsuzuri_main`）を出す（`src/driver.rs:4078-4082`）ので、`@main` は `--emit exe` の IR にしか現れない。

### 再現 1: プールのない tzroutine で autorelease すると漏れる

`target/scratch/m10/leak/Main.tz`:

```tsuzuri
@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def msg0 :: i64 -> i64 -> i64

@trusted
extern "objc_autoreleasePoolPush" def pool_push :: unit -> i64

@trusted
extern "objc_autoreleasePoolPop" def pool_pop :: i64 -> unit

// [[[NSObject alloc] init] autorelease] を count 回。
def churn :: i64 -> unit = \count ->
    let cls = objc_get_class (ref u8"NSObject\0")
    let alloc = sel (ref u8"alloc\0")
    let init = sel (ref u8"init\0")
    let autorelease = sel (ref u8"autorelease\0")
    for _ in 1 ..= count do
        let _ = msg0 (msg0 (msg0 cls alloc) init) autorelease
        ()

// 1000 個ごとに push と pop を対にする（スレッドに固定した中で）。
def churn_pooled :: i64 -> unit = \count ->
    Routine.with_locked_thread (\() ->
        for _ in 1 ..= count / 1000 do
            let pool = pool_push ()
            churn 1000
            pool_pop pool)

def main :: unit -> i32 = \() ->
    tzroutine {
        churn 2000000
    }
    0
```

```sh
target/debug/tsuzuri build target/scratch/m10/leak -O3 -l objc -o target/scratch/m10/leak/leak
/usr/bin/time -l target/scratch/m10/leak/leak 2>&1 | grep "maximum resident"
OBJC_DEBUG_MISSING_POOLS=YES target/scratch/m10/leak/leak 2>&1 | head -1
```

`main` の本体だけを変えた 4 つの版の maxrss（`/usr/bin/time -l`、バイト）:

| 版 | `-O3` | `-O3` と Foundation（`--link <SDK>/System/Library/Frameworks/Foundation.framework/Foundation.tbd`） | `-O0` |
| --- | --- | --- | --- |
| `tzroutine { churn 0 }`（基準） | 1,884,160（1.8 MiB） | 5,832,704（5.6 MiB） | 1,900,544 |
| `tzroutine { churn 2000000 }`（プールなし） | 50,380,800（48.0 MiB） | 54,280,192（51.8 MiB） | 50,380,800 |
| `tzroutine { churn_pooled 2000000 }` | 1,916,928（1.8 MiB） | 5,881,856（5.6 MiB） | 1,949,696 |
| `churn 2000000`（`main` のスレッド、プールなし） | 50,167,808（47.8 MiB） | – | – |

プールのない版は、`OBJC_DEBUG_MISSING_POOLS=YES` で物体ごとに `objc[96079]: MISSING POOLS: (0x16d607000) Object 0x10317d2a0 of class NSObject autoreleased with no pool in place - just leaking - break on objc_autoreleaseNoPool() to debug` を出す。`main` のスレッドの版も同じで、run loop の前の OS のメインスレッドにもプールはない。`churn_pooled` の版は 1 行も出さない。

### 再現 2: 固定せずに push と pop を待ちの前後に分けると abort する

`target/scratch/m10/migrate/Main.tz`（再現 1 の `pool_push`・`pool_pop` の宣言に加えて）:

```tsuzuri
@trusted
extern "pthread_self" def pthread_self :: unit -> i64

def worker :: i64 -> unit = \id ->
    for _ in 1 ..= 50 do
        let before = pthread_self ()
        let pool = pool_push ()
        Time.sleep (Time.millisecond())
        let after = pthread_self ()
        if before != after then
            do! IO.writeln $"tzroutine {id}: pushed on {before}, popping on {after}"
        pool_pop pool

def main :: unit -> i32 = \() ->
    for id in 1 ..= 8 do
        tzroutine {
            worker id
        }
    0
```

`target/debug/tsuzuri build target/scratch/m10/migrate -O3 -l objc -o target/scratch/m10/migrate/mig` の後、`TSUZURI_MAXPROCS=4 target/scratch/m10/migrate/mig` を 10 回動かすと、9 回は次のような出力で exit 134 だった（スレッドの番号は実行ごとに違う）。残りの 1 回は libobjc の文を出さずに SIGSEGV（exit 139）で終わった。どちらにしてもプロセスは終わる:

```text
tzroutine 1: pushed on 6104444928, popping on 6105640960
objc[5246]: Invalid or prematurely-freed autorelease pool 0x7880000058. Set a breakpoint on objc_autoreleasePoolInvalid to debug. 
tzroutine 2: pushed on 6104756224, popping on 6105640960
tzroutine 3: pushed on 6104444928, popping on 6104756224
objc[5246]: Invalid autorelease pools are a fatal error
```

ループの本体を `Routine.with_locked_thread (\() -> ...)` で包むと、スレッドは変わらず（出力 0 行）、3 回とも exit 0 だった。

### 再現 3: +0 の結果を直後に retain しても物体はプールを経る

`target/scratch/m10/rv/Main.tz`（再現 1 の `objc_get_class`・`sel`・`msg0` の宣言に加えて）:

```tsuzuri
@trusted
extern "objc_retainAutoreleasedReturnValue" def retain_autoreleased :: i64 -> i64

@trusted
extern "objc_release" def release :: i64 -> unit

// +[NSMutableArray array] は +0 の結果。受け取った直後に retain し、release で釣り合わせる。
def churn :: i64 -> unit = \count ->
    let cls = objc_get_class (ref u8"NSMutableArray\0")
    let array = sel (ref u8"array\0")
    for _ in 1 ..= count do
        let owned = retain_autoreleased (msg0 cls array)
        release owned

def main :: unit -> i32 = \() ->
    tzroutine {
        churn 1000
    }
    0
```

Foundation をリンクして `OBJC_DEBUG_MISSING_POOLS=YES` で動かすと、`-O0`・`-O3` のどちらも `MISSING POOLS` が 1000 行（呼び出しの数と同じ）出た。`--emit llvm` の IR では、送信と retain はそれぞれの `extern def` の包み（`tz.fn.Main.msg0`・`tz.fn.Main.retain_autoreleased`）の中にあり、tzroutine を起動するプログラムなのでどちらの `call` も `tsuzuri_tz_blocking_enter`・`_exit` で包まれている。clang の `"clang.arc.attachedcall"` が置く印（`mov x29, x29`）はないので、返す側の `objc_autoreleaseReturnValue` は速い経路を取らず、物体をプールに入れる。`churn 1000` を `churn 2000000` にすると maxrss は 118,652,928 バイト（113 MiB。`-O0` は 118,636,544）、再現 1 の `churn_pooled` と同じく `with_locked_thread` の中で 1000 回ごとに push と pop を対にすると 5,898,240 バイト（5.6 MiB）だった。

## 変えてはいけない意味

- ObjC を使わないプログラムの生成物（S1）。tzroutine のランタイムの振る舞い（Phase 1）。
- `Routine.with_locked_thread` の意味と、W19 の固定の規則（固定する境界の中で `pinned` の関数を呼べる、tzroutine へ送れない）。
- D-61 の `main` の境界の待ちと `Drop` の順。`@tz.main.done` の後にコールバックが来たらトラップする規則（`tz.callback.after_main`）。
- トラップの意味（巻き戻さない。プールの pop は飛ぶが、プロセスが終わる）。

## 仕様

### Phase の範囲

| Phase | 内容 | 前提 |
| --- | --- | --- |
| 1 | `ObjC.with_autorelease_pool`、ObjC を使うプログラムの `@main` のプール、漏れの回帰テスト、文書の規律 | M01、D-67 の分岐 1（承認済み）、D4 |
| 2 | ObjC を使うプログラムで、ランタイムが tzroutine の実行の区間ごとにプールを push・pop する | M02 の Phase 1、D3 |

### 1. `ObjC.with_autorelease_pool`（Phase 1）

- 型は `(unit -> 'a) -> 'a`。`f` を 1 回呼び、その結果を返す。
- スケジューラーを使うプログラム（`src/llvm.rs:1646` の `spawning` が真）では、`tsuzuri_tz_lock_thread` で固定してから push し、`f ()` と `f` の drop の後で pop し、`tsuzuri_tz_unlock_thread` で固定を外す。スケジューラーを使わないプログラムでは、すべての呼び出し元が擬似 G で、固定は何もしないので、push と pop だけを出す（tzroutine のランタイムを連結しない。決定事項 D1）。
- `f` の中で待っても、固定されているので、pop は push と同じスレッドで行われる。入れ子にしてよい（内側から pop する）。
- `f` の結果に入っている ObjC の物体は、M01 の所有の値（+1）なので pop で解放されない。手書きの `i64` の id を結果に返すと、pop の後には解放されていることがある（文書に書く）。
- `f` の中でトラップすると pop は飛ぶ。プロセスが終わるので問題ない。
- `f` の中では `pinned` の関数（M01 の既定の `local` のクラスに触れる関数を含む）を呼べる。W19 の固定する境界に足す（`Position::Pinning`、効果の辺の除外）。tzroutine を起動する本体を渡すことは、`with_locked_thread` と同じく妨げない。
- 引数の順序（D-59）: 1 引数（関数）なので例外は要らない。

### 2. `@main` のプール（Phase 1）

- ObjC を使うプログラム（M01 の `uses_objc` が真になるプログラム。M01 はコンパイラが ObjC の参照を出した IR にだけモジュールのフラグ `!"Objective-C Version"` を足し、それで `-lobjc` も決める）の console の `@main` は、3 つの形のどれでも、`main` の本体（`tz.fn.<main>` か `tsuzuri_main`）の呼び出しの直前に `%tz.pool = call ptr @objc_autoreleasePoolPush()` を置き、tzroutine の境界の待ち（`{leave}`）の後、`@tz.main.done` の store の前に `call void @objc_autoreleasePoolPop(ptr %tz.pool)` を置く。
- IO の入口の `@main` は `{leave}` を持たない。待ちは `tsuzuri_main` の中にある（`with_tzroutine_scopes` が `tzroutine_boundaries` の `tsuzuri_main` に足す。`src/llvm.rs:1689-1699`）。そこで pop は `call i32 @tsuzuri_main()` の直後、`{done}` の前に置く。
- 利用者が `extern "objc_autoreleasePoolPush"` を宣言しただけのプログラムは ObjC を使うプログラムではなく、`@main` は変わらない（S1・S2）。
- 包まないもの: ライブラリの入口（`tsuzuri_main`。`Entry::Library`）、`export def`、`test`・`bench` の runner の入口、コールバック。どれもホストかテストの本体の責務で、本体を `ObjC.with_autorelease_pool` で包む。
- `main` が AppKit の run loop（`[NSApp run]`）に入れば、AppKit がイベントごとに内側のプールを回す。`-[NSApp terminate:]` は `exit()` を呼ぶので pop は走らない（Xcode の `main` と同じ）。

### 3. target と診断（Phase 1）

- 宣言と型検査は target に依らない。wasm32・wasm64 で `ObjC.with_autorelease_pool` に到達したビルドは `E1008` `abi_native_only`、Apple でない native は `E1008` `abi_apple_only`（M01 の kind）。どちらも `build` のときに出す。
- 新しい診断コードと kind はない。

### 4. 効果・権限（Phase 1）

- `ObjC.with_autorelease_pool` は効果 `extern`（libobjc を呼ぶ）と callbacks、権限 `ffi`。組み込みの関数で利用者の宣言を持たないので `W1010` は出さない。`pure def` の中では使えない（`extern` が `pure` の禁じる効果だから）。

### 5. 文書の規律（Phase 1）

- tzroutine・`Parallel.*` の本体・ホストのスレッドで呼ばれるコールバックで Foundation や AppKit の物体を作るか +0 の結果を受けるときは、本体を `ObjC.with_autorelease_pool` で包む。長いループでは、反復ごとか一定の回数ごとに包む（再現 1 の 1000 回ごと）。`main` の本体と AppKit の run loop の中のコールバックは包まなくてよい。
- Phase 2 が入っても、明示のプールは使える（区間のプールの内側に入る）。

### 6. Phase 2: ランタイムが区間ごとにプールを回す（要承認）

- ObjC を使うプログラムだけ、tzroutine のランタイムを `-DTZR_OBJC_POOLS` を付けてコンパイルする（`-DTZ_STACK_GUARD` などと同じ並び。prebuilt のランタイムとの照合とキャッシュが、このマクロの有無を区別することを確かめる）。そのランタイムは、M が G に切り替える直前（`tzr_execute` の `TZR_TO_G(m, g)`、`src/runtime/tzroutine.c:1234`）に g0 の上で push し、G から g0 に戻った直後に pop する。
- 固定された G は待っても g0 に戻らない（自分のスレッドで眠る）ので、`ObjC.with_autorelease_pool` の明示のプールが区間の終わりで切られることはない。擬似 G（`main` のスレッド、ホストのスレッド）は `tzr_execute` を通らないので変わらない。
- 安全の根拠: M02 は +0 の物体の結果を受け取った直後に retain し、Tsuzuri が持つ ObjC の値はすべて +1 である。区間の pop は、Tsuzuri が所有しない物体だけを解放する。手書きの `i64` の id を待ちの前後で持ち越す古い書き方では、待ちの後に解放されていることがある（文書に書く）。
- 費用: 切り替えごとに push と pop が 1 回ずつ（数十 ns の見込み）。ObjC を使うプログラムでだけ払う。tzroutine の ping-pong の計測で、`TZR_OBJC_POOLS` の有無を比べて文書に書く（共有の CI に速度の閾値を置かない）。

## 設計

### Phase 1 の生成

- `Builtin::ObjCWithAutoreleasePool`（名前 `ObjC.with_autorelease_pool`）。`signature` は `with_locked_thread` と同じ形（`BuiltinType::Function([unit], Var("a")) -> Var("a")`）。`Builtin::ALL`・`name`・`signature`・`emit_builtin`、効果の表（`src/effects.rs` の `builtin_facts` は `facts(Some(Effect::Extern), false, false, false, true, false, None)`、`src/pure_rules.rs`・`src/kg/effects.rs`）、`src/pinned_rules.rs:33-35` の `Position::Pinning`、`src/effects.rs` の `pinning_argument_mask` の 2 か所（2216 行の `matches!` と 2225 行の `$builtin` の包みの `wrapper_builtin`）、権限の表（`src/capabilities.rs`）を更新する。
- 下げ方（スケジューラーを使うプログラム。使わないプログラムは lock と unlock の 2 行がない）:

```llvm
define internal <a> @tz.builtin.ObjC.with_autorelease_pool.<a>(%tz.closure %arg0) nounwind {
entry:
  call void @tsuzuri_tz_lock_thread()
  %pool = call ptr @objc_autoreleasePoolPush()
  ; f () を呼ぶ（with_locked_thread と同じ apply）
  call void @tz.closure.drop(%tz.closure %arg0)
  call void @objc_autoreleasePoolPop(ptr %pool)
  call void @tsuzuri_tz_unlock_thread()
  ret <a> %v
}
```

- `with_locked_thread` と違い、closure の drop を pop の前に置く（捕捉した値の drop で走る dealloc が autorelease する物体もプールに入れるため）。
- `ObjC.with_autorelease_pool` に到達した IR にも M01 のモジュールのフラグを出す。プールだけを使うプログラムでも `uses_objc` が真になり、`-lobjc` と `@main` のプールが同じ判定にそろう（フラグがないと `objc_autoreleasePoolPush` が未解決の `E2002` になる）。
- 「スケジューラーを使う」は `src/llvm.rs:1646` の `spawning` と同じ条件にする。`FunctionEmitter` からは `Globals` に値を足して読む（今の `Globals.spawns` は `spawns(module)` だけで、`Parallel.*` の `scheduler_parallel` を含まない）。`with_locked_thread` と同じく組み込みの本体は `emit_typed_builtin` の `local_globals`（`src/llvm.rs:8711-8722`）で作られうるので、足す値はそこで写す一覧にも足す。足さないと `..Globals::default()` の偽になり、固定が黙って消えて再現 2 の abort になる。[M05](../M05-objc-exception-boundary.md) のブロッキングの包みも同じ値を要る（先に入れたほうが足す）。
- 宣言 `declare ptr @objc_autoreleasePoolPush()`・`declare void @objc_autoreleasePoolPop(ptr)` は D-67 の共通の規則に従う。利用者の `extern` が同じ symbol を宣言していれば（再現 1 の `i64` の形）その行を残し、コンパイラは自分の関数型で `call` する（LLVM は型の違う 2 つ目の `declare` を拒み、debug ビルドでは driver の `debug_assert!` の `duplicate_declarations` の検査（`src/driver.rs:4239-4245`）も落ちる）。

### Phase 1 の `@main`

`console_def_main` と `console_main` に引数 `objc_pool: bool` を足す。値は M01 がモジュールのフラグを出す条件（M01 の `Globals` の `objc_used`）を読む。`console_main` を呼ぶのは、関数・組み込み・`intrinsics` を出した後である（`src/llvm.rs:2344`。`uses_args` を `intrinsics` から読むのと同じ時点）。だから値はそこで確定している。ただし、組み込みの関数の本体は `emit_typed_builtin`（`src/llvm.rs:8689`）で作り、`shared_globals.traps` が `None` なら一部の値だけを写した `local_globals`（8711-8722 行）を使う。`shared_globals` に戻すのは 3 つの表だけである（9002-9012 行）。`ObjC.with_autorelease_pool` の本体で立てた `objc_used` は、そこで戻す処理を足す。それでも確定しない経路があれば、`windows_abi` が `@main` の `entry:` の後に `tsuzuri_trap_install` を足す後工程の pass（`src/llvm.rs:309-313`）と同じ形で、`uses_objc` が真の IR の `@main` に行を足す。`def main` の形（tzroutine を起動し、`mark_main_done` のとき）は次のようになる:

```llvm
define i32 @main() {
entry:
  %tz.scope = call ptr @tsuzuri_tz_scope_enter()
  %tz.pool = call ptr @objc_autoreleasePoolPush()
  %code = call i32 @tz.fn.Main.main(i8 0)
  call void @tsuzuri_tz_boundary_leave(ptr %tz.scope)
  call void @objc_autoreleasePoolPop(ptr %tz.pool)
  store atomic i32 1, ptr @tz.main.done release, align 4
  ret i32 %code
}
```

- pop を境界の待ちの後にするのは、待っている間も `main` が autorelease した物体を生かしておくため（Xcode の `@autoreleasepool { return NSApplicationMain(...); }` と同じく、`main` の全体を包む）。`@tz.main.done` の前にするのは、pop で走る dealloc がコールバックを呼んでも `after_main` のトラップにならないため。
- `objc_pool` が偽なら、生成する文字列は今と 1 byte も違わない（S1）。

### Phase 2 のランタイム（`src/runtime/tzroutine.c`・`src/driver.rs`）

```c
#if defined(TZR_OBJC_POOLS)
void *objc_autoreleasePoolPush(void);
void objc_autoreleasePoolPop(void *pool);
#endif
/* tzr_execute の中 */
#if defined(TZR_OBJC_POOLS)
    void *pool = objc_autoreleasePoolPush(); /* on g0, on this M's thread */
#endif
    TZR_TO_G(m, g);
#if defined(TZR_OBJC_POOLS)
    objc_autoreleasePoolPop(pool);
#endif
```

- driver は、IR が M01 の判定で ObjC を使うときだけ `runtime.arg("-DTZR_OBJC_POOLS")` を足す（`src/driver.rs:4915-4923` の並び）。ObjC を使わないプログラムのランタイムは今と同じ object になる。
- `sh scripts/check-runtime-includes.sh` と `tests/tzroutine_runtime.c` に、マクロの有無の両方の build を足す。

## 前提とする他チケットのインターフェース

- [M01](M01-objc-class-types.md): 組み込みのモジュール `ObjC`、`HandleKind::ObjcLocal`・`ObjcSync`、`llvm::uses_objc(text)`（IR が `!"Objective-C Version"` を含むか）とモジュールのフラグを出す条件、`-lobjc` の自動の追加（5 つのリンク行）、`abi_apple_only`、`ObjC` の名前の登録の表。判定が利用者の手書きの宣言で真になるなら止めて報告する（S2）。
- [M02](M02-objc-typed-message-send.md)（弱い依存）: +0 の物体の結果の retain（M01 の D6 の `clang.arc.attachedcall` の bundle。M02 の D6 がこれに従う。Phase 2 の安全の根拠）。ハンドシェイクが効けば返された物体はプールを経ないが、呼ばれた側の内部の autorelease は残るので、Phase 1 の規律は変わらない。
- W19 の `Position::Pinning`・`pinning_argument_mask`・`E1013`（`pinned_code_spawned`）。TZ08 の `tsuzuri_tz_lock_thread`・`unlock`。D-61 の `TZROUTINE_SCOPE_ENTER`・`LEAVE`。

## 他チケットへ提供するインターフェース

- [M05](../M05-objc-exception-boundary.md): 例外の物体の多くは autorelease されてから投げられる。tzroutine で境界を繰り返し呼ぶ処理は `ObjC.with_autorelease_pool` で包む（M05 の文書が参照する）。
- [M04](../M04-objc-blocks.md)・[M09](M09-main-thread-dispatch-and-handles.md): GCD のスレッドで呼ばれる Block とコールバックは擬似 G で、固定は何もせず push と pop だけが走る。
- [M08](M08-macos-main-thread-docs-and-e1013.md)・W28: routine.md:130 と native-interop.md:630 の同じ段落に触れる。後に入るほうが文を合わせる。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/m10-objc-autorelease-pools`）。**内容**: 着手条件の確認、`node scripts/repo-kb.mjs brief M10`、ベースラインの保存。「現状」の再現 1〜3 を着手時の HEAD で動かす。**確認**: 保存した件数と、再現の maxrss と行数を控える。
2. **変更**: `src/check.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/pinned_rules.rs`・`src/capabilities.rs`。**内容**: 組み込みの関数の型、効果、固定の位置、権限。**確認**: `tests/objc_pools.rs` の型と固定の検査（下の表）。件数の pin（`src/effects.rs:3839` の `assert_eq!(Builtin::ALL.len(), 375)` などは現在の値 + 1）。`cargo nextest run --locked`。
3. **変更**: `src/llvm.rs`・`src/llvm_objc.rs`。**内容**: 仕様 1 の下げ方、`Globals` のスケジューラーの値、`declare` の規則。**確認**: スケジューラーを使うプログラム（`tzroutine { }` と `Parallel.map` の両方）と使わないプログラムの IR、利用者の `extern "objc_autoreleasePoolPush"` と並べた IR（`llvm::duplicate_declarations` が空）。組み込みの本体が `local_globals` で作られてもスケジューラーの値が写っていること。
4. **変更**: `src/llvm.rs`（`console_def_main`・`console_main`）。**内容**: 仕様 2。**確認**: `llvm::emit(&module, Entry::Console)` の `@main` の 3 つの形。ベースラインと `cmp`（S1）。
5. **変更**: `tests/objc_pools.mjs`・`tests/fixtures/objc_pools/Main.tz`。**内容**: E2E（下）。**確認**: macOS で `node tests/objc_pools.mjs target/debug/tsuzuri`。
6. **変更**: 文書。**内容**: 「ドキュメント」の節。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、PR に `ci:full`。

### Phase 2（人間が求め、D3 が承認されたとき）

1. 落とし穴の非同期の横取りの扱いを決めて記録する（S4）。
2. `src/runtime/tzroutine.c` の `TZR_OBJC_POOLS` と `src/driver.rs` のマクロ。**確認**: `sh scripts/check-runtime-includes.sh`、`tests/tzroutine_runtime.c` の両方の build、ObjC を使わないプログラムのランタイムの object がベースラインと同じ。
3. E2E の Phase 2 の case と計測（下）。文書。

## テスト計画

### Rust（`tests/objc_pools.rs`、新規。どのホストでも）

| 検査 | 期待 |
| --- | --- |
| 型 | `ObjC.with_autorelease_pool (\() -> 1)` が `i64`。引数が関数でなければ `E1003` |
| 固定 | `local` のハンドルに触れる関数を `ObjC.with_autorelease_pool` に渡すと通り、その呼び出し元に `pinned` が広がらない。同じ関数を `tzroutine { }` の中で直接呼ぶと `E1013` `pinned_code_spawned`。`with_autorelease_pool` を関数値として渡すのは `with_locked_thread` と同じ扱い |
| 効果 | `pure def` の中は `E1029`（効果 `extern`）。facts の効果が `extern` |
| IR（`--target aarch64-apple-darwin`） | スケジューラーを使うプログラムだけ `tsuzuri_tz_lock_thread` が出る。push・pop・closure の drop の順。利用者の `extern "objc_autoreleasePoolPush" def pool_push :: unit -> i64` と並べても `declare` が 1 行 |
| `@main` | ObjC を使うプログラムの 3 つの形（`def main`、IO の入口、値を返す `main`）で push と pop の位置。tzroutine を起動する形では `{leave}` の後、`@tz.main.done` の前。ObjC を使わないプログラムと、利用者の宣言だけのプログラムの `@main` はベースラインと同じ |
| target | `--target wasm32` で到達したビルドが `E1008` `abi_native_only`、`--target x86_64-unknown-linux-gnu` が `E1008` `abi_apple_only` |
| 決定性 | `emit` を 2 回して同じ |

### E2E（`tests/objc_pools.mjs`、macOS だけ。`-O0` と `-O3`）

| case | 内容 | 期待 |
| --- | --- | --- |
| `leak` | 早見の例（tzroutine の中で 200 万個を autorelease、1000 個ごとにプール） | maxrss（`/usr/bin/time -l`）が 20 MiB 未満。速度ではなく正しさの検査で、余裕を大きく取る（手で対にした版は 5.6 MiB、プールなしは 113 MiB。再現 3） |
| `missing_pools` | `leak` を `OBJC_DEBUG_MISSING_POOLS=YES` で | `MISSING POOLS` が 0 行 |
| `main_pool` | ObjC を使うプログラムの `main` の先頭で 1 つ autorelease する | `MISSING POOLS` が 0 行 |
| `migrate` | 再現 2 の形を `ObjC.with_autorelease_pool` で書く（`TSUZURI_MAXPROCS=4`、8 個の tzroutine が中で `Time.sleep`） | exit 0、pop の前後でスレッドが同じ |
| `nested` | 入れ子のプールと、本体の結果に M01 の所有の値を返す | 値が使え、`live == 0` |
| `parallel` | `Parallel.for_each_chunk` の各チャンクの関数の本体を包み、その中で autorelease する | `MISSING POOLS` が 0 行 |
| `stack` | Foundation を多く使う本体（`NSString` の書式、`NSDictionary` の構築を 1 万回）を既定の大きさの tzroutine で動かす | スタックのあふれがない。使った大きさを記録する（閾値にしない） |
| `phase2_auto`（Phase 2） | プールで包まない tzroutine で 200 万個を autorelease し、1000 個ごとに `Routine.yield ()` | maxrss が 20 MiB 未満 |
| `phase2_cost`（Phase 2） | channel の ping-pong を `TZR_OBJC_POOLS` の有無で計測 | 結果を文書に書く（合否にしない） |

## ドキュメント

- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作る）: 新しい節「autorelease pool」。`ObjC.with_autorelease_pool`、`@main` のプール、文書の規律（仕様 5）、固定の意味、手書きの `i64` の id の注意、wasm と Apple でない native の `E1008`。Phase 2 は出荷したときに書く。
- `_tsuzuri/language-reference/built-in-types-and-modules/routine.md`: `Routine.with_locked_thread` の節（128 行から）に、Cocoa の物体を扱うときは `ObjC.with_autorelease_pool` が固定とプールを兼ねるという注意とリンク。
- `_tsuzuri/language-reference/compiler/native-interop.md:224` の固定する境界の一覧に `ObjC.with_autorelease_pool` を足す。
- `docs/language.md`: tzroutine の節（2712 行から）と「ホスト関数のインポート」の節に同じ内容を短く。
- `docs/architecture.md`: 「`def main` の入口」（630 行から）に `@main` のプールの位置、Phase 2 では「tzroutine のランタイム（TZ01）」（1100 行から）に `TZR_OBJC_POOLS`。

## 受け入れ条件

- [ ] Phase 1 の仕様 1〜5 のとおりに動き、仕様外の入力は表の診断で拒否される。
- [ ] ObjC を使わないプログラムと、利用者の宣言だけのプログラムの IR・object・header・`check --json`・`@main`・既定の `.wasm` がベースラインと 1 byte も違わない。
- [ ] 生成 IR が決定的で、`declare` が重複しない。
- [ ] `tests/objc_pools.rs` と `node tests/objc_pools.mjs` が `-O0`・`-O3` で通る。`stack` の大きさを PR に書いた。
- [ ] D-55 の表に `ObjC.with_autorelease_pool` を記録した。PR に `ci:full` を付けた。
- [ ] T2 が通る。
- [ ] （Phase 2）ObjC を使わないプログラムのランタイムの object が変わらず、計測の結果を文書に書いた。

## 落とし穴

- **固定を外してから pop しない**: 順は lock → push → 本体 → drop → pop → unlock。unlock の後で pop すると、その間に待ちはないが、将来の変更で待ちが入ると再現 2 の abort になる。
- **closure の drop の位置**: `with_locked_thread` は本体の後に drop する。プールでは pop の前に drop する（捕捉した ObjC の値の dealloc が autorelease する物体もプールに入れる）。
- **利用者の宣言との `declare`**: 再現 1 のように `objc_autoreleasePoolPush` を `unit -> i64` で宣言したプログラムがある。`declare` を 2 行にすると LLVM が拒み、debug ビルドでは driver の `duplicate_declarations` の `debug_assert!` も落ちる。
- **組み込みの本体の `Globals`**: `emit_typed_builtin` は、`shared_globals.traps` が `None` のとき、一部の値だけを写した `local_globals` で組み込みの本体を作る（`src/llvm.rs:8711-8722`）。スケジューラーの値を写し忘れると固定が消え、`objc_used` を戻し忘れると `-lobjc` と `@main` のプールが消える（戻すのは 9002-9012 行の 3 つの表だけ）。
- **`@main` は `--emit llvm` に出ない**: `--emit llvm` は `Entry::Library` の形を出す。`@main` の検査は `llvm::emit(&module, Entry::Console)` か `--emit exe` の結果で行う。
- **判定を利用者の宣言で決めない**: `objc_` で始まる `declare` があるかで決めると、利用者が手で宣言しただけのプログラムの `@main` が変わる（S1・S2）。M01 のモジュールのフラグ（コンパイラだけが出す）で決める。
- **Phase 2 と非同期の横取り**: TZ10 の非同期の横取り（`TZ_ASYNC_PREEMPT`、`src/runtime/tzroutine.c:242-248`。tzroutine を起動する実行ファイルで driver が付ける。`src/driver.rs:4361`）は、生成コードの中なら任意の命令で G を止める。送信の `bl objc_msgSend` と直後の `bl objc_retainAutoreleasedReturnValue` の間で止まって g0 に戻ると、区間の pop が +0 の物体を解放し、再開した G が解放後の物体を retain する。素の retain を別の行に置く場合、正確な窓は、M02 の D12 の包みでは `tsuzuri_tz_blocking_exit` が戻ってから retain までの間である。横取りは `syscall_depth == 0` のときだけ起きる（`tzr_async_entry` の条件、`src/runtime/tzroutine.c:2616`）ので、包みの中（送信の間）は止まらない。M01 の D6 の bundle を付けた呼び出しでは、retain は呼び出しの命令の一部として `bl` の直後に走り、`with_blocking_externs`（`src/llvm.rs:1840-1915`）は行の単位で包むので、retain は包みの exit より前に済む。この窓が残るのは、bundle を付けられない W14 の `i64` の結果の fallback（素の retain が別の行）と、利用者が手で書いた `extern` の送信と retain の組である。Phase 2 は、ObjC を使うプログラムで非同期の横取りを止めるか、この残りの窓を横取りの対象から外す方法を決めてから着手する（S4）。固定された G の yield は横取りの yield も含めて `tzr_yield_locked`（1675 行。`tsuzuri_tz_yield` の 1707-1709 行）でその場で待ち、g0 に戻らない。
- **Phase 2 と古い書き方**: `i64` の id を待ちの前後で持ち越すと、区間の pop で解放される。文書に書き、M07 の生成物は型付きの宣言だけにする。

## 対象外

- ARC の最適化（retain と release の対の除去。+0 の結果の `clang.arc.attachedcall` の bundle は M01 の D6 で、このチケットは変えない）。
- `export def` の呼び出しごとのプール（ホストの責務）と、ライブラリの入口・`test`・`bench` の runner の入口のプール。
- main のスレッドの run loop のプール（AppKit が回す）。
- tzroutine のスタックの大きさの変更（Foundation を tzroutine で動かしたときの使用量は記録だけにする。W28 の macOS 版の測定も範囲の外）。
- `NSAutoreleasePool` のクラスを Tsuzuri から使うこと（M02 のメンバーとして宣言すれば送れるが、文書では勧めない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `ObjC.with_autorelease_pool` は、スケジューラーを使うプログラムでは固定してから push し、本体と closure の drop の後で pop して固定を外す。スケジューラーを使わないプログラムでは push と pop だけ。トラップでは pop が飛ぶ | 再現 2 のとおり、固定しない pop は別のスレッドのプールに対して行われて abort する。擬似 G では固定は何もしないので、lock を出すと tzroutine のランタイムを連結するだけになる。1 引数なので D-59 の例外は要らない。退けた案: (a) プールのトークンの型と push・pop の 2 つの関数（LIFO を守らない使い方を型で止められず、対を書き忘れる）。(b) `autoreleasepool { }` の構文（新しい語が要る。予約語を足さない方針）。(c) 今の手書きの回避策を文書にするだけ（SDK にない宣言を各自が書き、固定を忘れると abort する） | 既定案（実装者はこの案に従う） |
| D2 | `@main` のプールは、M01 の判定で ObjC を使うプログラムにだけ足す。push は本体の呼び出しの直前、pop は境界の待ちの後で `@tz.main.done` の前。宣言はコンパイラが生成し、`declare` の共通の規則に従う | ほかのプログラムの IR を変えない。Xcode の `main` と同じく本体の全体を包む。退けた案: macOS のすべてのプログラムを包む（全プログラムの IR が変わる）、`objc_` の `declare` の有無で判定する（利用者の手書きの宣言を拾う） | 既定案（実装者はこの案に従う） |
| D3 | Phase 2: ObjC を使うプログラムだけ、ランタイムを `TZR_OBJC_POOLS` でコンパイルし、`tzr_execute` の切り替えの前後で push と pop をする。費用は計測して書く。`i64` で id を持つ古い書き方の危険と、非同期の横取りの扱い（S4）を先に決める | プールを書き忘れても tzroutine の漏れが区間で止まる。マクロで分けるので ObjC を使わないプログラムのランタイムは変わらない。退けた案: すべてのランタイムで常に回す（全 tzroutine のプログラムの費用と object が変わる）、`@main` が関数の pointer を登録する（ランタイムの object と切り替えごとの分岐が全プログラムに入る） | 承認済み（D-67 の委任で決定、2026-11-29。D9〜D11 で形を改めた） |
| D4 | 名前は `ObjC.with_autorelease_pool`（D-55 の登録） | `Routine.with_locked_thread`・`Mutex.with_lock` と同じ `with_` の形で、関数を 1 つ取る。退けた案: Swift と同じ `ObjC.autoreleasepool`（Tsuzuri の std の名前の付け方と合わない） | 承認済み（D-67 の委任で決定、2026-11-29） |
| D5 | 固定の位置は W19 の `Position::Pinning` と効果の辺の除外に足す。効果は `extern`、権限は `ffi`、`W1010` は出さない | 本体は固定されたスレッドで走るので、`local` のクラスに触れる関数を呼べるのが正しい。組み込みの関数には利用者の宣言がない。退けた案: `Position::Inline`（`pinned` が呼び出し元に広がり、`local` のクラスに触れる本体を包んだ関数を tzroutine で呼べなくなる）、`W1010` を出す（利用者が書いた `extern` の宣言がなく、`@trusted` を付ける場所がない） | 既定案（実装者はこの案に従う） |
| D6 | wasm は `abi_native_only`、Apple でない native は `abi_apple_only`（M01 の kind）を build のときに出す | D-67 の共通の規則。退けた案: wasm と Apple でない native では黙って本体だけを呼ぶ（ObjC のない環境への移植の誤りを隠す） | 既定案（実装者はこの案に従う） |
| D7 | 漏れのテストは maxrss の閾値（20 MiB）と `MISSING POOLS` の有無で行い、速度の閾値は置かない | プールのある版（5.6 MiB）とない版（48〜113 MiB）の差が大きく、閾値に余裕がある。AGENTS.md は共有の CI に速度の合否を置かない。退けた案: 閾値を基準の数倍に狭く取る（CI の機械と SDK の版で基準の maxrss がぶれる）、dealloc の数で数える（ホストのクラスが要り、漏れそのものではなく解放の有無しか見ない） | 既定案（実装者はこの案に従う） |
| D8 | 漏れの E2E は、ARC なしのホスト（`tests/objc_pools_host.m`）が `[[[TzPooled alloc] init] autorelease]` で確実にプールへ入れる物体で行う。maxrss の対照は、同じ 200 万個を `main` のスレッドで 1 つのプール（`@main` のプール）に溜めた実行（30 MiB を超える）にする | 型付きの +0 の結果はハンドシェイクでプールを経ないので、早見の例では、プールがあってもなくても maxrss が変わらない（検査にならない）。ARC のホストの +0 の結果も同じ。退けた案: 早見の例のまま（何も確かめない検査になる）、手書きの `extern "objc_msgSend"` で autorelease を送る（Objective-C のイメージで手書きの id を使う形を fixture に残す） | 決定（D-67 の委任、2026-11-29） |
| D9 | `TZR_OBJC_POOLS` のランタイムでは、M のスレッドのスタック（g0）を 256 KiB から G の既定と同じ 1 MiB にする | 区間の pop は `dealloc` を g0 の上で走らせる。明示のプール（G のスタックの上で pop する）と同じ深さを持たせる。予約するのはアドレス空間だけで、触れたページしか使わない。退けた案: 256 KiB のまま（Foundation・AppKit の `dealloc` の連鎖でスケジューラーのスレッドのスタックがあふれうる）、pop を G のスタックの上で行う（待ちに入る G は R1 のロックを持っていて、その下で `dealloc` を走らせることになる） | 決定（D-67 の委任、2026-11-29） |
| D10 | 区間の pop は、yield（`tzr_after_yield`）と終了（`tzr_after_exit`）では `after` の前、待ち（`tzr_after_park`）では `after` の後に行う | 待ちの `after` は R1 のロックを外す。その前に pop すると、ランタイムのロック（spin）を持ったまま任意の `dealloc` が走る。yield と終了では、`after` まで G は走れず、ロックもないので、先に pop して「yield から戻ったら解放済み」「境界の待ちが終わったら解放済み」を決定的にする。待ちでは、起こし手が速ければ G は pop より先に再開する（文書に書く）。退けた案: いつも `after` の後（yield の直後の観測が競合する）、いつも `after` の前（ロックの下の `dealloc`） | 決定（D-67 の委任、2026-11-29） |
| D11 | S4: W14 の `i64` の結果の素の `objc_retainAutoreleasedReturnValue` を、`host_call` が出す `tsuzuri_tz_blocking_enter`・`_exit` の中に、呼び出し・`inttoptr` と一緒に入れる（`Globals::blocking_externs` のプログラムだけ）。非同期の横取りは止めない。手書きの `i64` の id は文書の規律で扱う | 包みの中では横取りが起きず、exit は G を M から離さないので、呼び出しから retain までに区間の pop は起きない。bundle 付きの呼び出しは retain が同じ行にあり、行の単位の包みで足りる。退けた案: ObjC を使うプログラムで非同期の横取りを止める（検査のないループの公平さを失い、協調的な検査の yield では結局 pop が起きるので、手書きの id は守れない）、手書きの `extern` の組も守る（コンパイラは利用者の 2 つの `extern` の呼び出しの間の関係を知らない） | 決定（D-67 の委任、2026-11-29） |

## 実装と検証（2026-11-29）

Phase 1・2 を 1 つのブランチ（`feat/m10-objc-autorelease-pools`）で実装した。D3・D4 は D-67 の委任で決め、着手前のレビューで D8〜D11 を足した（上の「改訂」と、GUIDE D-67 の「委任による決定（2026-11-29、M10）」）。

### 着手前の再現（HEAD `a198b995`、arm64 macOS 27.2、Apple clang 21）

- 再現 1（手書きの `extern`。プールのない tzroutine で 200 万個）: `-O3`・`-l objc` で maxrss 50,348,032 バイト（48.0 MiB）、`OBJC_DEBUG_MISSING_POOLS=YES` で `MISSING POOLS` の行が出た。起票時と同じ。
- `ObjC.with_autorelease_pool (\() -> 0)` は `E1002`（`module 'ObjC' has no function or union case 'with_autorelease_pool'`。M01 がモジュールを足したので、起票時の `unknown value 'ObjC'` から文が変わっていた）。
- 早見の例を、プールの代わりに `Routine.with_locked_thread` で包んで動かすと、maxrss 5,914,624 バイト、`MISSING POOLS` 0 行だった（漏れない。D8 の根拠）。

### 実装したもの

- **組み込みの関数**（`src/check.rs`）: `Builtin::ObjcWithAutoreleasePool`（名前 `ObjC.with_autorelease_pool`、型は `Routine.with_locked_thread` と同じ）。`is_objc`（直接の呼び出しだけで、その場で下げる組み込み）には入れず、普通の `$builtin` の包みを持つ（関数値にできる）。`Builtin::needs_objc_runtime` を足し、`validate_objc_target` が使う。
- **効果・固定・権限**: `effects::builtin_facts`（`extern` と callbacks）、`pure_rules`（`Outside`）、`kg/effects`（`extern`）、`pinned_rules::builtin_position`（`Position::Pinning`）、`effects::pinning_argument_mask`（直接の呼び出しと `$builtin` の包みの 2 か所）と `boundary_of`（`Boundary::Locked`: `pinned` は止まり、`main_thread` は進む）、`capabilities`（`ffi` の site。クラスを名指さないので `ObjC.Object` の下に置く）。`Builtin::ALL.len()` の pin は 397 → 398。
- **生成**（`src/llvm_objc.rs`・`src/llvm.rs`）: `objc_with_autorelease_pool` が lock → push → 適用 → 関数値の drop → pop → unlock を出す。lock と unlock は `Globals::scheduler`（native で、spawn するか `Parallel.*` を呼ぶ）のプログラムだけ。`ObjcGlobals::pool` が 2 つの libobjc の関数を `RUNTIME` の表から宣言し（利用者の `extern` の行があれば使い回す）、`used` を立てる。`emit_typed_builtin` は、組み込みの本体を別の `Globals` で作るとき、`objc` の表を共有の表から持ち込んで戻す。
- **`@main`**: `console_main`・`console_def_main` に `objc_pool` を足した（3 つの形）。判定は、`console_main` を呼ぶ時点の `Globals::objc` の `used`（後から変われば `debug_assert!` が落ちる）。
- **S4**（`src/llvm_imports.rs`）: `host_call` が、W14 の `i64` の結果の素の retain を、自分で出す `tsuzuri_tz_blocking_enter`・`_exit` の中に入れる（`bracket_retain`）。
- **ランタイム**（`src/runtime/tzroutine.c`）: `TZR_OBJC_POOLS` のとき、`tzr_execute` が g0 の上で push し、yield と終了では `after` の前、待ちでは `after` の後に pop する。M のスタック（`TZR_G0_STACK`）は、このマクロのときだけ 1 MiB。マクロがなければ、プリプロセスの後の本文は `256 * 1024` が `(256u * 1024u)` になるだけである。
- **driver**（`src/driver.rs`）: `objc_pools`（tzroutine のランタイムを連結する native の Mach-O で、`llvm::uses_objc(&text)`）のときだけ `-DTZR_OBJC_POOLS` を足す。tzroutine のランタイムを含む単位は事前ビルドの対象でない（`scripts/toolchain/runtime.mjs` は cpu.c・io.c の単位だけを作る）ので、事前ビルドの束は変わらない。キャッシュは引数の列で区別される。
- **テスト**: `tests/objc_pools.rs`（10 件）、`tests/objc_pools.mjs` と `tests/objc/pools/Main.tz`・`tests/objc/pingpong/Main.tz`・`tests/objc_pools_host.m`、`tests/tzroutine_runtime.c` の `objc_pools` と `tests/tzroutine_runtime.mjs` の macOS の build（`-DTZR_OBJC_POOLS`、`-lobjc`、`-O0`・`-O3` × P 1・4 で全 case）。`.github/workflows/targets.yml` の darwin-arm64 の `suites` の段に `objc_pools.mjs` を足し、README の「変更の種類 → テスト」の表に行を足した。

### 検証（arm64 macOS 27.2、Apple clang 21、開発版 `target/debug/tsuzuri`）

- **S1（変わらないこと）**: `main`（`a198b995`）の開発版を別の worktree で作り、`tests/fixtures`・`examples` の `Main.tz` を持つ 176 個のプロジェクトを、`--emit llvm`（native・`--target x86_64-unknown-linux-gnu`・`--target wasm32`）と `--emit object --no-cache`（native）、それぞれ `-O0`・`-O3` で比べた。1,408 個のうち、作れた 1,368 個がすべてバイト単位で一致し、39 個は両方が同じ終了コードと同じ文で拒否した。残りの 1 個（`tests/fixtures/net_wasm` の wasm32 `-O3`。両方とも `E2000` で拒否する）は、T1 と並行して走らせた回だけ標準エラーが食い違い、単独で 3 回やり直すと 3 回とも一致した。手書きの宣言だけのプログラムは、E2E の `legacy` の段（`nm -u` に `autoreleasePool` がなく、`MISSING POOLS` が今までどおり出る）と、Rust の `main_pool_only_in_programs_that_use_objc`（`@main` が宣言のないプログラムと同じ行）で確かめた（S2）。
- **Rust**: `cargo test --locked --test objc_pools` が `running 10 tests`、10 件通過（型、固定の境界と `main_thread`、効果と `E1029`・`E1031`、権限 `ffi`、スケジューラーの有無と lock、利用者の宣言との `declare` の共有、`@main` の 3 つの形と宣言だけのプログラム、release だけの組み込みの本体の `declare`、target の `E1008`、包みの中の retain）。`cargo nextest run --locked` は `3853 tests run: 3849 passed (21 slow), 4 failed, 4 skipped` で、落ちた 4 件は組み込みの数を固定する単体テスト（`effects` の 2 件、`pure_rules`、`kg::effects`。効果を持つ組み込み 183 → 184、callbacks 34 → 35、Level B 182 → 183）だった。数を直した後、`cargo test --locked --lib` が `795 passed; 0 failed; 1 ignored`、関連する 7 つのテストファイル（`objc_pools`・`objc_classes`・`objc_send`・`blocking`・`main_thread_handles`・`thread_bound_handles`・`capabilities`）が 88 件通過した。直したのはテストの期待値とコメントだけなので、全体はやり直していない（CI が走らせる）。
- **ランタイムの C**: `node tests/tzroutine_runtime.mjs` が `tzroutine runtime: 950 runs passed`（今までの build に加えて、`-DTZR_OBJC_POOLS`・`-lobjc` の `-O0`・`-O3` × P 1・4 で全 case と `objc_pools`）。`objc_pools` は、yield の後に 0 個、待ちの後に 0 個（待って数える）、境界の待ちが終わった時点で 0 個、固定された G が自分のプールを持ったまま yield と sleep をして 5 個のまま、固定を外した後の yield で 0 個、400 KiB を超えるスタックを使う `dealloc` が g0 の上で走ることを確かめる。
- **E2E**: `node tests/objc_pools.mjs target/debug/tsuzuri` が `objc_pools: ok (86 checks)`（同梱のツールチェーンの段を含む）。`-O0`・`-O3` × P 1・4 で 14 個の case の出力がホストの数と一致し、`main_pool`（`5`、終了時 `exit 0`）と `main_wait`（待ちの間は `1`、終了時 `exit 0`）、`OBJC_DEBUG_MISSING_POOLS=YES` で 6 つの case の `MISSING POOLS` が 0 行だった。
  - **maxrss**: 200 万個を 1 つのプール（`@main` のプール）に溜めると 51.8 MiB、1000 個ごとに `ObjC.with_autorelease_pool` で包むと 5.7 MiB、プールを書かずに 1000 個ごとに `Routine.yield ()` すると（区間のプール）5.7 MiB（`-O0`・`-O3` とも）。閾値は 20 MiB 未満と 30 MiB 超で、速度の閾値は置いていない（D7）。
  - **`stack`**: Foundation の書式（`+[NSString stringWithFormat:]` に辞書・配列・日付を渡すのを 1 万回）を既定の大きさ（1 MiB）の tzroutine で動かし、あふれなかった。Foundation が `-description` を呼び返した深さは、tzroutine の本体から 6,528 バイト（`-O3` は 6,512 バイト）下だった（その地点の深さで、Foundation の最も深いフレームではない。閾値にしていない）。
  - **`phase2_cost`**: 無バッファーのチャンネルの 20 万往復（プロセス全体の時間、5 回の中央値、`-O3`）は、区間のプールのあるプログラムで 1,528 ms、Objective-C を使わないプログラムで 1,536 ms（別の実行では 1,593 ms と 1,559 ms）。この機械は計測の間 load average が 20〜50 で、差は実行ごとのぶれの中にあった。切り替えごとの push と pop の費用は、この計測では見分けられなかった。静かな機械での計測はしていない。
- **既存の suite**（開発版）: `objc_classes.mjs`（33 checks）、`objc_send.mjs`（90 checks）、`main_thread.mjs`（53 checks）、`tzroutine.mjs`（179 runs）、`macos_bundle.mjs`、`macos_report.mjs`（60 cases）が通過。`weak_import.mjs` は同梱のツールチェーンの `minos 13.0` の検査で落ちたが、`main` の開発版でも同じ検査で落ちる（この機械の `target/toolchain` の状態によるもので、この変更とは関係がない）。
- **T0 と文書**: `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings` が通過。`node scripts/gen-docs.mjs --check`（11 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`sh scripts/check-runtime-includes.sh`（66 files）、`node scripts/check-guide-lite.mjs`（87 references、0 failed）。Tsuzuri-Reference は `node scripts/check-docs.mjs docs/compiler/objective-c.md docs/stdlib/sync/routine.md docs/compiler/native-interop.md` が `3 pages, 50 links, 14 checked examples, 14 native runs`。
- **確かめていないこと**: CI の `macos-15` のランナーでの `objc_pools.mjs`（PR に `ci:full` を付けて走らせる）。非同期の横取りが W14 の fallback の呼び出しと retain の間に入る実行そのもの（窓が包みの中にあることは、Rust の `a_plain_retain_is_inside_the_blocking_call` が IR の行の順で確かめる。実行での再現は試していない）。AppKit の run loop の中の tzroutine。

### チケットから外れた判断

- **D8〜D11**（上の「改訂」）: 漏れの検査の物体、g0 のスタック、pop の位置、S4 の決め方。
- **`is_objc` に入れない**: 仕様のとおり関数値にできる普通の組み込みにしたので、M01 の「直接の呼び出しだけ」の検査（`objc_builtin_not_called`）の対象ではない。target の検査は `needs_objc_runtime` で広げた。
- **`objc` の表の持ち回り**: 設計の「`objc_used` を戻す」の代わりに表ごと持ち回る。M01・M02 の組み込みの本体（`ignore` の release など）にも効く（今までは、同じ関数をプログラムのほかの所が使っていたので表に出ていなかった）。
- **E2E の case**: テスト計画の `leak`・`missing_pools`・`main_pool`・`migrate`・`nested`・`parallel`・`stack`・`phase2_auto`・`phase2_cost` に当たるものに加えて、`main_wait`（pop が境界の待ちの後であること）、`held_by_main`（maxrss の対照）、`auto_sleep`・`auto_exit`・`locked`・`owned`（区間のプールの境目）、`legacy`（手書きの宣言だけのプログラムが変わらないこと）、Objective-C を使わないプログラムの `nm -u` を足した。`parallel` は `Parallel.for_each_chunk` でなく `Parallel.map` で書いた（各要素の関数の本体を包む形は同じ）。
- **`tests/tzroutine_runtime.c` の case**: 設計は「マクロの有無の両方の build」だけを求める。libobjc の C の API でクラスを作って数える `objc_pools` を足した。
- **ほかのチケットへの追記**: M03（落とし穴）・M05・M23 に、区間のプールとの契約（retain までを包みの中に置く、区間のプールが呼ぶ `dealloc` は待たない）を 1 か所ずつ書いた。

### 更新した文書

- `docs/language.md`: 「Objective-C のクラスとメッセージ送信」に `ObjC.with_autorelease_pool` の行と段落「autorelease pool（M10）」（3 つの pool、所有しない物体だけを解放すること、手書きの `i64` の id、`dealloc` の走る場所、target）。固定する境界の一覧（`extern type local`・`main` の段落）と `Routine` の段落。制限の段落から、手書きの push・pop の手順と「解放されません」を除いた。
- `docs/architecture.md`: 「`def main` の入口」（`@main` のプールの位置）、「tzroutine のランタイム」（区間のプール、`TZR_OBJC_POOLS`、g0 のスタック、検証の build）、「Objective-C の連携」（組み込みの関数の生成と表、包みの中の retain）。
- `README.md`: 「変更の種類 → テスト」の表に行を足した。機能の一覧と CLI は変わらない。
- `_features/README.md`・`_features/GUIDE.md`（D-67 の「委任による決定（2026-11-29、M10）」）・M03・M05・M23。
- Tsuzuri-Reference（ブランチ `docs/m10-objc-autorelease-pools`）: `docs/compiler/objective-c.md`（要点、節「Autorelease Pool」、`ObjC` の表、効果と権限、運用上の注意点）、`docs/stdlib/sync/routine.md`、`docs/compiler/native-interop.md`。

### 変更したファイル

`src/check.rs`、`src/effects.rs`、`src/pure_rules.rs`、`src/kg/effects.rs`、`src/pinned_rules.rs`、`src/capabilities.rs`、`src/llvm.rs`、`src/llvm_objc.rs`、`src/llvm_imports.rs`、`src/driver.rs`、`src/runtime/tzroutine.c`、`tests/objc_pools.rs`（新規）、`tests/objc_pools.mjs`（新規）、`tests/objc_pools_host.m`（新規）、`tests/objc/pools/Main.tz`（新規）、`tests/objc/pingpong/Main.tz`（新規）、`tests/objc/classes/Main.tz`（コメント）、`tests/tzroutine_runtime.c`、`tests/tzroutine_runtime.mjs`、`.github/workflows/targets.yml`、`docs/language.md`、`docs/architecture.md`、`README.md`、`_features/README.md`、`_features/GUIDE.md`、`_features/M03-objc-classes-defined-in-tsuzuri.md`、`_features/M05-objc-exception-boundary.md`、`_features/M23-objc-out-parameters-and-weak-references.md`、`_features/_completed/M01-objc-class-types.md`・`M02-objc-typed-message-send.md`（リンク）、このチケット。

### Copilot のレビューへの対応（PR #156、2026-11-29）

指摘は 2 件で、どちらも直した。

- **`test`・`bench` のランナーが区間のプールを持たない**: `tzroutine.c` をコンパイルする所は driver のほかに `src/test_runner.rs` の 2 か所（`compile_native_runner` と `build_debug_runner`）があり、どちらも `-DTZR_OBJC_POOLS` を渡していなかった。判定を `driver::objc_pools_runtime`（Mach-O の target で `llvm::uses_objc`）とマクロの名前 `driver::OBJC_POOLS_DEFINE` にまとめ、3 か所が同じものを使う。fixture に `test` を 1 つ足し（yield する tzroutine、終わる tzroutine、固定された tzroutine）、E2E が `tsuzuri test` を `-O0`・`-O3` で通し、`test --index 0 -g -o` のランナーを実行して `nm -u` に `_objc_autoreleasePoolPush` があることを確かめる。直す前の形（Phase 1 だけ）では、同じ case が `1000 1000` を返していた。
- **`--trap-mode return` でトラップが pop を飛ばす**: 境界へ戻る `siglongjmp` は、pop と `tsuzuri_tz_unlock_thread` を飛ばし、プールがホストのスレッドに残る。`MainThread`・`Anchor` と同じ並びで、`llvm::uses_autorelease_pool` のプログラムを `E2000` `option_conflict` で拒否する（文: `--trap-mode return cannot be combined with ObjC.with_autorelease_pool: …`）。`Domain.run` の本体は純粋でなければならない（`domain_body_not_pure`）ので、効果 `extern` のこの関数は失敗の領域の中には書けない。Rust の `trap_mode_return_excludes_the_pool` が、直接の呼び出しと関数値の両方で確かめる。
- 対応の後: `cargo test --locked --test objc_pools` が 11 件、`node tests/objc_pools.mjs target/debug/tsuzuri` が `objc_pools: ok (89 checks)`、`main_thread`・`objc_classes`・`objc_send`・`test_runner`・`bench` の Rust のテストと `trap_boundary.mjs`（20 cases）が通過。`docs/language.md`・`docs/architecture.md` と Tsuzuri-Reference の `objective-c.md` に、ランナーの区間のプールと `--trap-mode return` の拒否を書いた。
