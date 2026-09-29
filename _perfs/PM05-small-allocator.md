# PM05: 小さい確保のための size-class allocator

| 項目 | 内容 |
| --- | --- |
| ID | PM05 |
| 分類 | メモリ |
| 優先度 | P1 |
| 規模 | L |
| 依存 | PX01, (F13) |
| 関連 | F11, F13, PM04, A04, PX02 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D2（旧 Phase 1 のサイズ付き解放の取り下げ。Phase 2 の前）, D10（native の既定 allocator の切り替え。Phase 3 の前）。Phase 1 は承認不要 |
| 手本にする既存実装 | WASM heap: `src/runtime/heap-wasm.ll` の `@tz.alloc`（bump と `@llvm.wasm.memory.grow.i32`）・`@tz.free`（前後の結合）・`@tz.realloc`（隣の空きの吸収）。threads: `src/runtime/heap-wasm-threads.ll` の lock wrapper と `tsuzuri_thread_heap_live_bytes` の走査、`src/llvm.rs` の heap 連結（`.unlocked` への改名）。単体の heap 検査: `tests/features.mjs` の `wasmReallocationChecks`（`heap-wasm.ll` だけを wasm にして address を照合）。Phase 2: `src/runtime/generate.py`（C から target 中立の IR を生成）、`src/runtime/task.c`（pool と `atexit`）、`tests/features.mjs` の `sanitizerKind` |
| 主な影響ファイル | Phase 1: `src/runtime/heap-wasm.ll`, `src/runtime/heap-wasm-threads.ll`, `tests/features.mjs`（`wasmReallocationChecks` の拡張と `wasmHeapStress`（新規））, `benchmarks/run-heap.mjs`（新規）, `docs/architecture.md`, `docs/benchmarks.md`。確認のみ: `src/llvm.rs`, `src/llvm_traps.rs`, `tests/primitives.mjs`, `tests/wasm_threads.mjs`, `tests/tasks.mjs`。Phase 2: `src/runtime/heap-small.c`（新規）, `src/runtime/heap-small.ll`（新規、生成）, `src/runtime/heap-small-native.ll`（新規）, `src/runtime/generate.py`, `src/llvm.rs`, `src/main.rs`, `src/driver.rs`, `tests/heap_small_test.c`（新規）, `tests/small_allocator.mjs`（新規）, `benchmarks/run-control.mjs`, `docs/language.md`, `_docs/tools/command-line.md`。Phase 3: 確保追跡をする全 harness（「既存テストへの影響」） |
| 計測対象 | Phase 1: `benchmarks/control/Main.tz` の `list_sum`・`closure_churn`・`closure_capture`・`array_copy` の wasm32 `-O3`（時間と線形メモリの大きさ）。Phase 2: 同じ 4 種目の native `-O3`（`benchmarks/run-control.mjs`）と、PX02 が done なら `benchmarks/apps` の `binary_trees`（最大 RSS） |

## 目的

再帰 union のノード、小さな文字列、関数値の環境など、小さく多数の確保を、確保 1 回あたりのメモリ量と時間の両面で C の malloc・Rust の既定 allocator より効率化する。
所有権により確保と解放の対が生成コードで閉じる Tsuzuri の性質を使い、native ではヘッダーのない確保を実現する。

段階は 3 つ。実装者は Phase 1（WASM の size-class 化）だけを実装する。Phase 2（native の `--allocator small`）は人間が依頼し D2 が承認された場合だけ、
Phase 3（native の既定の切り替え）は D10 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PX01 が done であること。`_perfs/README.md` の一覧で PX01 の状態欄が `done`、`benchmarks/metrics.mjs` が `measureProcess`・`makeRecord`・`writeRecords`・
  `compareRuns` を export していること（HEAD `f8dc655` には `benchmarks/metrics.mjs` がない）。確認: `grep -n "PX01\|PM05" _perfs/README.md` と
  `ls benchmarks/metrics.mjs`。
- Phase 2 は F13 の Phase 1 が done であること（`grep -n "F13" _features/README.md` の状態欄）と、D2 の承認。Phase 3 は Phase 2 の完了と D10 の承認。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースラインを保存していること。
- `_perfs/README.md` の一覧で PM05 の状態を `doing` にしてから始める。

### 前提とする他チケットのインターフェース

- F13（並行して詳細化中。名前は F13 の本文に従い、違えば F13 に合わせて読み替える）:
  - 生成コードと runtime のすべての確保は `@tz.alloc(i64)`・`@tz.free(ptr)`・`@tz.realloc(ptr, i64 old_size, i64 new_size)` を通る。C runtime
    （`src/runtime/io.c`）と拡張 ABI は `tsuzuri_alloc`・`tsuzuri_free` を通り、それらは `@tz.alloc`・`@tz.free` を呼ぶ。
  - ビルドの選択肢 `--allocator <name>` があり、既定は `system`（HEAD の `heap-native.ll` と同じ IR）。選択は `src/llvm.rs` の heap 連結
    （`output.push_str(if wasm { heap-wasm.ll } else { heap-native.ll })` の箇所）で、どの runtime 断片が上の 3 関数を定義するかだけを変える。
    呼び出し側の IR は変えない。
  - PM05 は Phase 2 で値 `small`（新規）を足す。F13 が `@tz.free` にサイズ引数を足した場合も、PM05 の正しさはその値に依存しない（D2）。
- PX01: PX01 形式（schema 1）、`target/perf/<run_id>/<suite>.jsonl`、`measureProcess`（`/usr/bin/time` による `peak_rss`）、runner の `--metrics`、
  確保の計数 `alloc_calls`・`alloc_bytes`（tracked IR の `@malloc` などを `tracked_*` に置換）。
- PX02（任意）: `benchmarks/run-apps.mjs` の `binary_trees` と `--only`。done でなければ使わない。

### 停止条件

次の場合は即興で回避せず、作業を止めてコマンド・出力・該当箇所を添えて報告する（GUIDE §13）。

1. 既存テストの期待値を変える必要が生じた。特に `wasmReallocationChecks` の address、`tests/primitives.mjs` の「heap must reuse and coalesce freed blocks」
   （`tz_coalescing` 500 回で線形メモリの増加 ≤ 65536）、`tests/wasm_threads.mjs` の `reserved = 2n * (262144n + 16n)`。Phase 1 の設計はこれらを
   変えない前提で作ってある。
2. `@llvm.trap` を `@tz.alloc`・`@tz.realloc`（threads では改名後の `.unlocked`）以外の関数へ置きたくなった。`src/llvm_traps.rs` の
   `runtime_kind` は関数名で `TrapKind::AllocationFailure` を決めるため、補助関数は null を返し、トラップは既存の関数に残す。
3. WASM の上限 16 MiB（`16777216`・`16777184`）、`--max-memory`、stack-first の配置、WASM の import（既定は空）を変える必要が生じた。
4. 同じ入力の 2 回のビルドで IR か `.wasm` がバイト単位で異なる。
5. Phase 2: `unsafe`、新しい crate、生成 IR へのプラットフォーム固有 API（`pthread_*`・`mmap`・`VirtualAlloc`）が必要になった。TSan・ASan が報告した。
   Apple Clang 21（`/usr/bin/clang`）で `generate.py` を実行できない。
6. system の確保で `live != 0`、または `tests/heap_small_test.c`（新規）の統計の不一致が出た。
7. 性能の悪化を、意味を変える手段（未初期化領域の露出、上限の緩和、確保失敗の握りつぶし）でしか回避できない。

## 現状と計測（HEAD `f8dc655`）

### runtime の連結と契約（コードで確認）

- `src/llvm.rs` の IR 出力の末尾で、IR が `@tz.string.`・`@tz.utf8string.`・`@tz.free`・`@tz.alloc`・`@tz.realloc` を含むと `runtime/string.ll`・
  `runtime/utf8string.ll` と heap を連結する。`Instrumentation` の `wasm_threads` が真なら先に `heap-wasm-threads.ll` を足し、`heap-wasm.ll` の
  `@tz.alloc(`・`@tz.free(`・`@tz.realloc(` を `@tz.heap.alloc.unlocked(` などへ文字列置換して連結する。
- native（`src/runtime/heap-native.ll`）: `@malloc`・`@free`・`@realloc` の薄い wrapper。null は `@llvm.trap`。`@tz.realloc` は `new_size == 0` で
  free して null を返す。native の最小 alignment は libc の保証（対象の 64-bit 環境で 16）。
- WASM（`src/runtime/heap-wasm.ll`）: 大域 `@tz.heap.end`・`@tz.heap.free`（i32）。ブロックは 16 bytes のヘッダー（+0 に容量 i32、空きなら +4 に次の
  address）を持ち、容量は `(size + 31) & -16`（最小 32）。確保は address 順の単一リストの first fit（余りが 32 以上なら分割）、なければ
  `__heap_base` を 16 に揃えた位置からの bump で、`end ≤ 16777216` を検査し、`@llvm.wasm.memory.grow.i32` で必要なページ数ちょうどまで伸ばす。
  要求の上限は 16777184。解放は address 順の挿入（リスト長に比例）と前後の結合。`@tz.realloc` は容量内なら同じ address、次の隣が空きなら
  リストを探して吸収、どちらでもなければ確保・1 byte ずつの複写・解放。payload は 16 bytes 境界。
- threads（`src/runtime/heap-wasm-threads.ll`）: 3 関数を `tsuzuri_threads_heap_lock`／`unlock`（`src/runtime/task-wasm-threads.c` の
  `state.heap_mutex`）で囲む。`tsuzuri_thread_heap_live_bytes` は `(end − 揃えた base) − Σ 空きリストの容量`（ヘッダー込み）。
  `tsuzuri_thread_stack_alloc` は `@tz.alloc(i64 262144)` で worker の stack を取る。
- 公開 ABI（`src/llvm_abi.rs`）: weak な `tsuzuri_alloc`（負はトラップ、0 は 1 byte）と `tsuzuri_free`（`@tz.free`）。WASM は `src/driver.rs` で
  `--export=tsuzuri_alloc`・`--export=tsuzuri_free`。`src/runtime/io.c` の行読み込みは `tsuzuri_alloc` で 256 bytes から倍々に確保し、その領域の
  所有権を Tsuzuri へ渡す。ホストも `tsuzuri_alloc` した領域を Tsuzuri へ渡す（docs/language.md の拡張 ABI）。
- トラップ: `src/llvm_traps.rs` の `runtime_kind` が `@tz.alloc`・`@tz.realloc` の中の `@llvm.trap` を `TrapKind::AllocationFailure`
  （`src/trap.rs` の表示 `allocation failed`）に分類する。
- 値の alignment の要求は `src/llvm.rs` の `storage_layout` で最大 16（SIMD の `(16, 16)` と i128）。
- `@tz.free` はサイズを受け取らない。呼び出しは `src/llvm.rs`・`src/llvm_bulk.rs`・`src/llvm_display.rs`・`src/llvm_recursive.rs`・`src/llvm_task.rs`
  に約 20 か所ある。`@tz.realloc` だけが呼び出し側から旧サイズを受け取る。

### 確保追跡のテスト（コードで確認）

- native の E2E は `--emit llvm` の IR の文字列 `@malloc`・`@free`（多くは `@realloc` も）を `@tracked_*` に置換し、C の host で各呼び出しの後に
  `live == 0` を確かめる。該当: `tests/features.mjs`（`_Atomic` の `live`、magic `0x51a110ca7e`）、`tests/primitives.mjs`、`tests/control.mjs`、
  `tests/computations.mjs`、`tests/strings.mjs`、`tests/tasks.mjs`、`tests/io.mjs`、`tests/host_imports.mjs`、`tests/gpu.mjs`、`tests/e2e.mjs`、
  `tests/math.mjs`（`@math_test_malloc`）、`tests/host_abi.rs`（`ir.replace("@malloc", "@tracked_alloc")`）。
- WASM には置換による追跡がない。`tests/primitives.mjs` は線形メモリの増加（上の停止条件 1）と 16 MiB 超過のトラップ（`tz_allocation_limit`、
  `tz_list_length(1024n * 1024n)`）を、`tests/features.mjs` の `wasmReallocationChecks`（suite `vec` で実行）は分割・吸収・移動・null・0・結合の
  address を、`tests/wasm_threads.mjs` は `tsuzuri_thread_heap_live_bytes` を確かめる。
- 確保を貯める allocator を native の既定にすると、置換で数えた `live` に貯めた領域が残り、全 harness の `live == 0` が壊れる。これが PM05 の最大の
  落とし穴で、D9 で扱う。

### 計測済みの事実

- 2026-09-29 のフロントエンドの計測では、コンパイラ自身が時間の 57% を malloc／free に使っていた（PB02）。利用者のプログラムの確保の比率は未計測。
- hello の実行時の最大 RSS は 1.70 MB（C は 1.67 MB、`_perfs/README.md`）。確保の多い処理の RSS 比較はない。
- WASM の確保の時間と線形メモリの大きさは未計測。HEAD には `binary_trees` の種目がない（PX02 が `benchmarks/apps` に計画）。

### 再現（HEAD `f8dc655`）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -n "16777184\|16777216\|add i32 %small, 31" src/runtime/heap-wasm.ll
grep -n "@tz.free(" src/llvm*.rs
grep -n "AllocationFailure" src/llvm_traps.rs src/trap.rs
node tests/features.mjs target/release/tsuzuri vec
node tests/primitives.mjs target/release/tsuzuri
node tests/wasm_threads.mjs target/release/tsuzuri
```

最後の 3 つは成功し、`vec` は `WASM realloc: split, absorb, fallback, null, zero and coalescing passed at O0/O3` を出す。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する。

- G1（Phase 1）: WASM の小さな確保・解放を、空きブロックの数によらない一定の手数にする。4 種目の wasm32 `-O3` の時間を悪化させず、線形メモリを増やさない。
  Phase 1 はヘッダー（16 bytes）を残すため、確保 1 回あたりのメモリは変わらない。
- G2（Phase 2）: native の `--allocator small` で、確保の多い種目の時間と最大 RSS を system（libc の malloc）より小さくする。最終目標は PX02 の
  `binary_trees` で C（malloc）・Rust・Zig の既定 allocator より少ない最大 RSS と短い時間。
- G3: 大きな確保（512 bytes 超）と確保の少ない種目の時間を悪化させない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 確保 1 回のメモリ | bytes（固定値） | 要求 8・24・100・512 bytes | WASM は変化なし（32・48・128・528）。native small は 16・32・112・512（ヘッダーなし）。glibc は 8 bytes のヘッダーと最小 32 bytes の chunk（32・32・112・528）、macOS の tiny は 16 bytes 単位でヘッダーなし（16・32・112・512） |
| M2 時間 | ms（9 標本の中央値・最小・最大） | 計測対象の 4 種目。Phase 1 は wasm32、Phase 2 は native。`-O3` | Phase 1 は first fit の探索がなくなる分の短縮。Phase 2 は libc の malloc／free の呼び出しがスレッドキャッシュの pop／push に代わる分の短縮 |
| M3 WASM の線形メモリ | bytes（`wasm_memory_bytes`（新規）。決定的なので全標本が等しい） | Phase 1 の 4 種目の実行後の `memory.buffer.byteLength` | 増えない |
| M4 最大 RSS | bytes（`peak_rss`、9 標本の中央値） | Phase 2 の `benchmarks/control` の host 全体、PX02 が done なら `binary_trees` の `--only` | 16 bytes 以下の確保が多い種目で減る（glibc 比）。macOS では同等 |
| M5 確保の計数 | `alloc_calls`・`alloc_bytes`（PX01） | 4 種目 | 変化なし（allocator は生成コードの確保の呼び出しを変えない。tracked は system で作る、D9） |

断片化は M3（WASM）と M4（native）で見る。空き領域の比率そのものは記録しない（Phase 2 の `tests/heap_small_test.c` は検査のために内部の統計を読むが、
PX01 のレコードにはしない）。

## 変えてはいけない意味

- 確保失敗はトラップ（`TrapKind::AllocationFailure`、`allocation failed`）で、生成コードへ null を返さない。トラップは `@tz.alloc`・`@tz.realloc` の中に
  置く（停止条件 2）。WASM の要求の上限 16777184、`end ≤ 16777216`、`memory.grow` の失敗はトラップのまま。
- 大きさの計算は溢れない順序で行う。WASM は `size ≤ 16777184` を確かめてから i32 で `size + 31` を計算する（HEAD と同じ）。native は `size ≤ 512` を
  先に確かめてから区分を計算する。
- null でない結果の alignment は 16 以上（`storage_layout` の最大）。
- `@tz.realloc(old, old_size, new_size)`: `new_size == 0` なら解放して null、`old == null` なら新規の確保、それ以外は先頭 `min(old_size, new_size)` bytes を
  保つ。同じ address を返してよい。`old_size` は呼び出し側が渡す正しい値として信頼する（HEAD の WASM の複写と同じ）。`@tz.free(null)` は何もしない。
- 確保した領域の中身は未初期化で、生成コードは書く前に読まない（HEAD の規則）。allocator は 0 埋めを約束しない。
- 二重解放・他の address の解放は生成コードが起こさない前提で、検出を約束しない（HEAD と同じ）。検出は ASan とテストの役目。
- 公開 ABI の `tsuzuri_alloc`・`tsuzuri_free` の契約（E05。サイズなしの解放、`tsuzuri_alloc` の領域と Tsuzuri が所有を渡した領域のどちらも解放できる）を
  変えない。
- 別のスレッドで確保した領域を解放できる（`Task.parallel` の結果は worker で確保され main で解放される）。native の pool の worker は同時に確保・解放する。
  WASM threads では既存の heap lock の中で動く。
- 同じ入力から IR・`.wasm` がバイト単位で同じ。確保の address は言語の保証ではない。ただし Phase 1 の WASM は address を決定的に保つ（テストが依存する）。
- 既定の WASM に import を足さない。native の生成 IR が参照する外部シンボルは `malloc`・`free`・`realloc` のまま（Phase 2 の `memcpy` は LLVM の intrinsic）。
- 呼び出し側の IR（runtime 断片の外）は変えない。評価順序・所有権・借用は runtime の変更の影響を受けない。

## 設計

### Phase 分割

| Phase | 内容 | 既定の変化 | 着手条件 |
| --- | --- | --- | --- |
| 1 | WASM の `heap-wasm.ll` を区分ごとの空きリスト（boundary tag による即時の結合）へ。threads 版も同じ | WASM の既定の heap が変わる | PX01 done |
| 2 | native の `--allocator small`（新規）。ヘッダーなし・スレッドキャッシュ | なし（system のまま） | 人間の依頼、F13 Phase 1 done、D2 承認 |
| 3 | native の既定を `small` へ。確保追跡の harness は `--allocator system` を明示 | native の既定が変わる | Phase 2 done、D10 承認 |

### Phase 1: ブロックの形式（WASM）

```text
block (16 bytes 境界)                  payload = block + 16
+0  i32 size_flags  = capacity | FREE(1) | PREV_FREE(2)   capacity は 16 の倍数、32 以上
+4  i32 next        空きのときだけ。リストの次（0 は終端）
+8  i32 prev        空きのときだけ。リストの前（0 は先頭）
+12 i32 0           未使用
空きブロックの末尾 4 bytes（block + capacity - 4）に footer = capacity
```

- 容量の読み出しはすべて `and i32 %size_flags, -16`。HEAD のヘッダーは +0 に容量だけを持つので、flag を読まない既存の箇所（`tsuzuri_thread_heap_live_bytes`
  を含む）はすべて mask を足す。
- 不変条件 I1: 隣り合う 2 つの空きブロックはない（解放で必ず結合する）。I2: `@tz.heap.end` に接するブロックは空きではない（接すれば `end` を下げて返す）。
  I3: `PREV_FREE` は直前のブロックが空きのときだけ立つ。I4: 空きブロックはちょうど 1 つのリストにある。
- リスト: 容量 32〜528 は区分 `i = capacity / 16 - 2`（0〜31）の LIFO リスト `@tz.heap.small`（新規、`[32 x i32]`）。空でない区分は
  `@tz.heap.smallmask`（新規、i32）の bit i。容量 529 以上は既存の `@tz.heap.free` を先頭とする address 順のリスト（HEAD の方針のまま）。どちらも双方向で、
  外すのは定数手数。

### Phase 1: アルゴリズム（WASM）

```text
alloc(size):                                   # @tz.alloc
  if size > 16777184: trap
  c = (size + 31) & -16
  if c <= 528:
    m = smallmask & (-1 << (c/16 - 2))
    if m != 0: b = head of small[cttz(m)]; unlink(b); return take(b, c) + 16
  for b in large list (address 順): if cap(b) >= c: unlink(b); return take(b, c) + 16
  bump（HEAD の処理。新ブロックの size_flags = c。I2 により PREV_FREE は 0）

take(b, c):                                    # @tz.heap.take（新規）。b は空きリストから外した直後
  r = cap(b) - c
  if r >= 32: b.size_flags = c; insert(b + c, r)          # 余りは空き。I1 により b の PREV_FREE は 0
  else:       b.size_flags = cap(b); clear PREV_FREE of (b + cap(b))   # I2 により b + cap(b) < end
  return b

free(p):                                       # @tz.free
  if p == null: return
  b = p - 16; n = cap(b)
  if b.PREV_FREE: q = b - load(b - 4); unlink(q); n += cap(q); b = q
  next = b + n
  if next == end: end = b; return               # I2。top へ返す
  if next.FREE: unlink(next); n += cap(next)
  insert(b, n)

insert(b, n):                                  # @tz.heap.insert（新規）
  b.size_flags = n | FREE; store n at b + n - 4; set PREV_FREE of (b + n)
  n <= 528 ? push small[n/16 - 2] and set bit : address 順に large へ挿入

realloc(old, old_size, new_size):              # @tz.realloc
  new_size == 0: free(old); return null
  new_size > 16777184: trap;  old == null: return alloc(new_size)
  b = old - 16; c = needed(new_size); n = cap(b)
  if c <= n: return old
  next = b + n
  if next != end and next.FREE and n + cap(next) >= c:
    unlink(next); keep = b.size_flags & PREV_FREE; total = n + cap(next)
    if total - c >= 32: b.size_flags = c | keep; insert(b + c, total - c)
    else:               b.size_flags = total | keep; clear PREV_FREE of (b + total)
    return old
  fresh = alloc(new_size); 1 byte ずつ min(old_size, new_size) を複写（HEAD のループ）; free(old); return fresh
```

- `unlink(b)`（`@tz.heap.unlink`（新規））は容量から区分を決め、`prev == 0` なら先頭（`@tz.heap.small[i]` か `@tz.heap.free`）を、そうでなければ
  `prev.next` を書き換え、`next != 0` なら `next.prev` を書き換える。区分のリストが空になったら mask の bit を落とす。
- 補助関数（`take`・`insert`・`unlink`）は `@llvm.trap` を含まない（停止条件 2）。名前に `@tz.alloc(`・`@tz.free(`・`@tz.realloc(` を含めない
  （threads 版の文字列置換の対象になるため）。
- `memory.grow` は HEAD と同じく必要なページ数ちょうどまで伸ばす（D5）。複写に `@llvm.memcpy` を使わない（wasm32 で bulk memory を有効にしていないため
  `memcpy` の外部参照になり、import か未定義シンボルになる）。
- `wasmReallocationChecks` の 3 ケースの address は上のアルゴリズムでそのまま成り立つ（2026-09-29 に手計算で確認。例: 容量 48 の A・144 の B・32 の C で
  B を解放し A を 80 bytes へ広げると、A は 96 になり余り 96 が区分 4 へ入り、次の 64 bytes の確保（容量 80）は区分 4 から A + 96 を返す）。

### Phase 2: native の `--allocator small`

- 区分: 要求 0〜512 bytes を 16 bytes 刻みの 32 区分（`class = size == 0 ? 0 : (size + 15) / 16 - 1`、ブロックの大きさ `16 * (class + 1)`）。
  512 bytes 超は `malloc`・`realloc`・`free`（system と同じ）。
- region: 1 MiB 境界の 1 MiB。`malloc(2 MiB)` の結果を 1 MiB へ切り上げて使う（触れない残りは常駐しない）。先頭 16 KiB は region のヘッダー
  （`page_class[64]`、`raw`、`next_page`）、残り 63 ページ（各 16 KiB）を要求に応じて区分へ割り当て、ブロックへ切り分ける。region は返却しない。
- 区分の判定（ヘッダーなし・サイズなし）: 登録表 `tz_small_map`（新規）。`addr >> 48 == 0` のとき上位表 `[16384]`（`addr >> 34`）から葉（16384 bytes、
  region ごとに 1 byte）を引き、立っていれば `page_class[(addr >> 14) & 63]`。葉は初回に `malloc`＋0 埋めで作り、atomic の release で公開、acquire で読む。
  `malloc` が 2^48 以上の address を返した場合は region として使わず、以後の小さな確保も system の経路へ回す。
- スレッド安全: スレッドキャッシュ（`_Thread_local` のポインターが指す `tz_small_cache`（新規）。区分ごとの LIFO の先頭と個数）と、中央のリスト
  （区分ごとの先頭と個数）を 1 つの spinlock `tz_small_lock`（新規、C11 `atomic_int` の test-and-test-and-set）で守る。batch は
  `max(4, 4096 / ブロックの大きさ)` 個、キャッシュの上限は 2 batch。空のキャッシュは中央から 1 batch を取り、中央も空なら新しいページを切る。上限に
  達したキャッシュは 1 batch を中央へ返す。ブロックはスレッドに所有されないため、別スレッドの解放はそのスレッドのキャッシュへ積むだけでよく、
  リモート解放のキューは不要（D6）。
- `realloc`: 旧が小さく新しい区分が同じなら同じ address。それ以外は新しく確保して `min(old_size, new_size)` を `memcpy` し、旧を解放する。旧新とも
  512 bytes 超なら libc の `realloc`。
- 補助関数は失敗で null を返し、`heap-small-native.ll`（新規、手書き）の `@tz.alloc`・`@tz.realloc` が `@llvm.trap` する（`heap-native.ll` と同じ形）。
- 統計 `tz_small_stats`（新規）: lock の中で切り分けた bytes・キャッシュと中央にある bytes を返す。テスト専用で、生成コードからは呼ばない。
- 実装言語: C（`src/runtime/heap-small.c`（新規））を `generate.py` で target 中立の IR（`src/runtime/heap-small.ll`（新規、生成））にする（D8）。

### 段ごとの変更

| Phase | ファイル | 関数・大域 | 変更内容 |
| --- | --- | --- | --- |
| 1 | `src/runtime/heap-wasm.ll` | `@tz.alloc`, `@tz.free`, `@tz.realloc` | 上のアルゴリズム。bump・上限・`memory.grow`・複写のループは HEAD のまま |
| 1 | `src/runtime/heap-wasm.ll` | `@tz.heap.small`・`@tz.heap.smallmask`・`@tz.heap.take`・`@tz.heap.insert`・`@tz.heap.unlink`（新規） | 区分のリストと補助関数 |
| 1 | `src/runtime/heap-wasm.ll` | `@tz.heap.free` | 大きなブロックだけの address 順・双方向のリストの先頭（名前は変えない） |
| 1 | `src/runtime/heap-wasm-threads.ll` | `tsuzuri_thread_heap_live_bytes` | 大きなリストに加えて 32 区分を走査し、容量は `and -16` で読む |
| 1 | `src/llvm.rs` | heap 連結 | 変更なし（置換の対象が 3 関数名だけであることを確かめる） |
| 1 | `src/llvm_traps.rs` | `runtime_kind` | 変更なし |
| 1 | `tests/features.mjs` | `wasmReallocationChecks`, `wasmHeapStress`（新規） | 「テスト計画」の追加ケース |
| 1 | `benchmarks/run-heap.mjs`（新規）, `benchmarks/metrics.mjs` | `METRICS` | WASM の時間と `wasm_memory_bytes`（新規）の記録 |
| 2 | `src/runtime/heap-small.c`（新規）, `src/runtime/generate.py` | `tz_small_alloc`・`tz_small_free`・`tz_small_realloc`・`tz_small_stats`（新規） | 生成の対象に `heap-small.c` → `heap-small.ll` を足す。`numeric.ll` はバイト単位で不変 |
| 2 | `src/runtime/heap-small-native.ll`（新規） | `@tz.alloc`, `@tz.free`, `@tz.realloc` | `tz_small_*` を呼び、null（要求が 0 でない）でトラップ |
| 2 | `src/main.rs`, `src/driver.rs` | F13 の `--allocator` の解析と検証 | 値 `small`。wasm32 では E2000（D11） |
| 2 | `src/llvm.rs` | heap 連結、F13 の allocator の欄 | `Small`（新規）で `heap-small.ll`・`heap-small-native.ll` を連結 |
| 2 | `tests/heap_small_test.c`（新規）, `tests/small_allocator.mjs`（新規） | 単体の検査と E2E | 「テスト計画」 |
| 2 | `benchmarks/run-control.mjs` | 引数の解析、tracked の経路 | `--allocator <name>`（新規）を `tsuzuri build` へ渡す。tracked の IR は常に `--allocator system` で別に作る |
| 3 | `src/main.rs`（F13 の既定値）と確保追跡の全 harness | 既定値、`cli(["build", ...])` | 既定を `small` へ。harness は `--allocator system` を明示 |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。runtime の `.ll` は `include_str!` で compiler に入るため、E2E の前に必ず
`cargo build --release --locked` を行う（`wasmReallocationChecks` だけは `src/runtime/heap-wasm.ll` を直接読む）。以下の「WASM 3 点」は次のコマンドで、
すべて終了コード 0。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
node tests/features.mjs target/release/tsuzuri vec
node tests/primitives.mjs target/release/tsuzuri
node tests/wasm_threads.mjs target/release/tsuzuri
```

### 手順 1: ベースライン（Phase 1）

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、HEAD の compiler を `/tmp/tz-pm05/tsuzuri-before` へ複写する。IR を保存する:
  `target/release/tsuzuri build tests/fixtures/primitives/Main.tz --target wasm32 --emit llvm -o /tmp/tz-pm05/before-wasm.ll`。
- 確認: WASM 3 点が成功し、`vec` が `WASM realloc: split, absorb, fallback, null, zero and coalescing passed at O0/O3` を出す。

### 手順 2: 容量の mask と双方向リスト（address は不変）

- 変更: `src/runtime/heap-wasm.ll` の 3 関数、`src/runtime/heap-wasm-threads.ll` の `tsuzuri_thread_heap_live_bytes`。
- 内容: ヘッダーの +0 を読む箇所をすべて `and i32 %x, -16` にする。空きブロックの +8 に `prev` を書き、`@tz.heap.unlink`・`@tz.heap.insert`（新規）へ
  address 順の挿入と取り外しを移す。方針（単一リスト・first fit）は変えない。
- 確認: WASM 3 点が成功（address を照合する `vec` を含む）。

### 手順 3: boundary tag と即時の結合

- 変更: `src/runtime/heap-wasm.ll` の `@tz.alloc`・`@tz.free`、`@tz.heap.take`（新規）。
- 内容: `FREE`・`PREV_FREE`・footer と不変条件 I1〜I4。解放はリストを歩かずに前後を結合し、`end` に接すれば `end` を下げる。まだ区分のリストはなく、
  空きはすべて address 順の単一リストに入れる。
- 確認: WASM 3 点が成功。

### 手順 4: 区分のリスト

- 変更: `src/runtime/heap-wasm.ll` の `@tz.heap.small`・`@tz.heap.smallmask`（新規）、`@tz.heap.insert`・`@tz.heap.unlink`・`@tz.alloc`。
- 内容: 容量 528 以下は区分の LIFO、529 以上は `@tz.heap.free` の address 順。確保は `@llvm.cttz.i32` で mask から区分を選ぶ。
- 確認: WASM 3 点が成功。

### 手順 5: realloc の吸収を flag で行う

- 変更: `src/runtime/heap-wasm.ll` の `@tz.realloc`。
- 内容: 次のブロックを `block + capacity` の flag で判定し、リストの探索（HEAD の `search`・`before`・`advance`）を消す。`PREV_FREE` を保つ（設計の擬似コード）。
- 確認: WASM 3 点が成功。

### 手順 6: threads の live bytes

- 変更: `src/runtime/heap-wasm-threads.ll` の `tsuzuri_thread_heap_live_bytes`。
- 内容: 大きなリストと 32 区分を lock の中で走査し、容量の合計を引く。
- 確認: `node tests/wasm_threads.mjs target/release/tsuzuri` が成功（`reserved` は `2n * (262144n + 16n)` のまま）。

### 手順 7: heap の単体テスト

- 変更: `tests/features.mjs` の `wasmReallocationChecks` と `wasmHeapStress`（新規、`name === "vec"` のとき `wasmReallocationChecks` の後に呼ぶ）。
- 内容: 「テスト計画」の E2E の 1〜3。
- 確認: `node tests/features.mjs target/release/tsuzuri vec` が成功し、`WASM heap stress: size classes, coalescing, top trim and limits passed at O0/O3` を出す。

### 手順 8: 計測 runner

- 変更: `benchmarks/run-heap.mjs`（新規）、`benchmarks/metrics.mjs` の `METRICS`（`wasm_memory_bytes: "bytes"` を足す）。
- 内容: 「計測手順」の runner。PX01 の `makeRecord`・`writeRecords` を使い、suite `heap`、target `wasm32`、opt `O3`。
- 確認: `npx --yes --package=node@24 node benchmarks/run-heap.mjs target/release/tsuzuri --quick --baseline /tmp/tz-pm05/tsuzuri-before` が終了コード 0 で、
  4 種目の `result` が before と after で等しい。`node tests/benchmark_report.mjs` が成功。

### 手順 9: 全体の確認・計測・文書（Phase 1 の完了）

- 変更: `docs/architecture.md`、`docs/benchmarks.md`、`_perfs/README.md` の PM05 の状態欄（Phase 1 完了の注記）。
- 内容: README.md の検証コマンド一式、「生成コードの確認」、「計測手順」の本計測、「ドキュメント」の Phase 1 の行。
- 確認: `cargo test --locked` と `node tests/features.mjs target/release/tsuzuri`（全 suite）、`node tests/tasks.mjs target/release/tsuzuri`、
  `node tests/strings.mjs target/release/tsuzuri` が成功。`node scripts/check-docs.mjs docs/architecture.md docs/benchmarks.md` が成功。

### 手順 10: C の allocator と生成（Phase 2、依頼と D2 の承認の後）

- 変更: `src/runtime/heap-small.c`（新規）、`src/runtime/generate.py`、`src/runtime/heap-small.ll`（新規、生成）、`tests/heap_small_test.c`（新規）。
- 内容: 設計の Phase 2。`generate.py` は同じ clang の引数と後処理を `numeric.c` と `heap-small.c` の 2 つに適用する。C は `stdint.h`・`stdatomic.h`・
  `stddef.h` と `malloc`・`free`・`realloc` の宣言だけを使い、`memcpy` は `__builtin_memcpy`。
- 確認: `TSUZURI_CLANG=/usr/bin/clang python3 src/runtime/generate.py` の後、`git diff --exit-code src/runtime/numeric.ll` が成功。
  `node tests/small_allocator.mjs target/release/tsuzuri --unit` が成功（単体テストを通常・`-fsanitize=address`・`-fsanitize=thread` で実行）。

### 手順 11: `--allocator small`

- 変更: `src/runtime/heap-small-native.ll`（新規）、`src/llvm.rs` の heap 連結、`src/main.rs`・`src/driver.rs`（F13 の `--allocator`）。
- 内容: native で `heap-small.ll` と `heap-small-native.ll` を連結する。wasm32 では E2000（D11）。既定の IR は変えない。
- 確認: `cargo test --locked` が成功。`target/release/tsuzuri build examples/hello --emit llvm -o /tmp/tz-pm05/hello.ll` が HEAD の出力とバイト単位で同じ。

### 手順 12: small の E2E

- 変更: `tests/small_allocator.mjs`（新規）。
- 内容: 「テスト計画」の E2E の 4・5。
- 確認: `node tests/small_allocator.mjs target/release/tsuzuri` が成功し、`TSUZURI_TSAN=1` でも成功。

### 手順 13: native の計測

- 変更: `benchmarks/run-control.mjs`。
- 内容: `--allocator <name>`（新規）を `tsuzuri build` へ渡す。`--metrics` の tracked の IR は常に `--allocator system` で別に作る（D9）。`--metrics` のとき、
  計時用の host を `measureProcess(host, ["--scale", String(scale)])` で測り、workload `host` の `peak_rss` として記録する。
- 確認: `--allocator small --quick --metrics /tmp/tz-pm05/q-small` と `--allocator system --quick --metrics /tmp/tz-pm05/q-system` が終了コード 0 で、
  両者の `alloc_calls`・`alloc_bytes` が同じ。

### 手順 14: Phase 2 の文書と記録

- 変更: `docs/language.md`、`docs/architecture.md`、`docs/benchmarks.md`、`_docs/tools/command-line.md`。
- 内容: 「ドキュメント」の Phase 2 の行と「計測手順」の Phase 2。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/tools/command-line.md` が成功。

### 手順 15: 既定の切り替え（Phase 3、D10 の承認の後）

- 変更: F13 の既定値、「既存テストへの影響」の Phase 3 の harness すべて。
- 内容: 先に harness へ `--allocator system` を足して成功を確かめ、その後で既定を変える。`tests/small_allocator.mjs` の E2E の 5 を全 suite へ広げる。
- 確認: README.md の検証コマンド一式と `TSUZURI_TSAN=1 node tests/features.mjs target/release/tsuzuri` が成功。

## 計測手順

PX01 の「before／after（他チケットの共通手順）」に従う。Node は 24（`npx --yes --package=node@24 node`）。

### 環境と条件

- AC 電源、他の重い処理を止める。`captureRun` が CPU・OS・clang・rustc・node・commit を記録する。
- before は手順 1 の `/tmp/tz-pm05/tsuzuri-before`、after は `target/release/tsuzuri`。同じ計測機・同じ日に続けて測る。

### Phase 1（wasm32）

`benchmarks/run-heap.mjs`（新規）の仕様: `node benchmarks/run-heap.mjs <compiler> [--quick] [--baseline <compiler>] [--metrics <dir>]`。
`benchmarks/control/Main.tz` を `--target wasm32 -O3` で build し、`WebAssembly.Module.imports` が空であることを確かめる。種目と大きさは
`list_sum` 100000、`closure_churn` 1000000、`closure_capture` 1000000、`array_copy` 250000（`--quick` はすべて 1024）、seed は `42n`。
各種目で warm-up 1 回と 9 標本。標本ごとに新しい instance を作り、`tz_<name>` の 1 回の呼び出しを `process.hrtime.bigint()` で測り、呼び出し後の
`memory.buffer.byteLength` を読む。結果と memory は 9 標本で等しいことを確かめる（違えば throw）。`--baseline` の compiler も同じに測り、結果が
等しいことを確かめる。記録は `wall_time`（variant `default`、baseline は `before`）と `wasm_memory_bytes`（新規）。
before の build が 16 MiB の上限でトラップした種目は、大きさを半分にして全体をやり直し、その大きさを `docs/benchmarks.md` に書く。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
RUN=target/perf/$(date -u +%Y%m%dT%H%M%SZ)-pm05-p1
npx --yes --package=node@24 node benchmarks/run-heap.mjs target/release/tsuzuri --baseline /tmp/tz-pm05/tsuzuri-before --metrics "$RUN"
node benchmarks/metrics.mjs report "$RUN"
```

判定は PX01 の `compareRuns` の分類（`改善`・`差なし`・`悪化`）で、`before` と `default` の広がり（最小〜最大）が重なる差は `差なし` と書く。

### Phase 2（native）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
npx --yes --package=node@24 node benchmarks/run-control.mjs target/release/tsuzuri --allocator system --metrics target/perf/$STAMP-pm05-system
npx --yes --package=node@24 node benchmarks/run-control.mjs target/release/tsuzuri --allocator small --metrics target/perf/$STAMP-pm05-small
node benchmarks/metrics.mjs report target/perf/$STAMP-pm05-small --baseline target/perf/$STAMP-pm05-system
```

- 比べるのは計測対象の 4 種目の `wall_time` と workload `host` の `peak_rss`。PX02 が done なら、`run-apps.mjs` を同じ 2 つの `--allocator` で実行し
  `binary_trees` の `peak_rss` を加える（PX02 の runner が compiler の引数を渡せない場合は PX02 の完了後に別途対応し、ここでは測らない）。
- 生データは `target/perf/` に残し、リポジトリへ入れない（PX01 D2）。要約だけを `docs/benchmarks.md` に書く。

## 生成コードの確認

### IR（Phase 1）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build tests/fixtures/primitives/Main.tz --target wasm32 --emit llvm -o /tmp/tz-pm05/after-wasm.ll
target/release/tsuzuri build tests/fixtures/primitives/Main.tz --target wasm32 --emit llvm -o /tmp/tz-pm05/again-wasm.ll
cmp /tmp/tz-pm05/after-wasm.ll /tmp/tz-pm05/again-wasm.ll
for f in before after; do n=$(grep -n "^@__heap_base" /tmp/tz-pm05/$f-wasm.ll | cut -d: -f1); head -n $((n - 1)) /tmp/tz-pm05/$f-wasm.ll > /tmp/tz-pm05/$f-prefix.ll; done
diff /tmp/tz-pm05/before-prefix.ll /tmp/tz-pm05/after-prefix.ll
grep -c "call void @llvm.trap" /tmp/tz-pm05/after-wasm.ll
```

- `cmp` と `diff` は出力なし（決定性と、heap の断片より前の IR が不変）。`@__heap_base` は `heap-wasm.ll` の 1 行目。
- `@llvm.trap` は `define internal ptr @tz.alloc` と `define internal ptr @tz.realloc` の本体だけにある（`grep -n` の行番号を各 `define` の範囲と照らす）。

### 機械語（Phase 1）

```sh
target/release/tsuzuri build tests/fixtures/primitives/Main.tz --target wasm32 -O3 -o /tmp/tz-pm05/p.wasm
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d /tmp/tz-pm05/p.wasm | grep -c "i32.ctz"
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d /tmp/tz-pm05/p.wasm | grep -c "memory.grow"
```

- `i32.ctz` が 1 以上（区分の選択）。`memory.grow` が 1 以上。`node -e` で `WebAssembly.Module.imports` が空であることも確かめる。

### IR と機械語（Phase 2）

- `--allocator small --emit llvm` の IR に `thread_local global` がちょうど 1 つ、`declare ptr @malloc` があり、`@aligned_alloc`・`@posix_memalign`・`@mmap`・
  `@pthread_` がない（`grep -nE "@(aligned_alloc|posix_memalign|mmap|pthread_)"` が空）。
- `-O3` の object を `/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --disassemble-symbols=_tz_small_alloc` で読み、入口から最初の `ret` までの経路
  （キャッシュに在庫がある場合）に atomic の命令（arm64 の `ldaxr`・`stlxr`・`swp`・`cas`・`ldadd`、x86_64 の `lock`・`xchg`）がないことを確かめる。

## テスト計画

期待値は独立に計算する（address は設計の擬似コードから手で、結果は既存 harness の参照式から）。現在の compiler の出力から写さない。

### Rust テスト

- Phase 1: `src/llvm.rs` の tests に `heap_wasm_threads_rename_covers_all_entry_points`（新規）。`include_str!("runtime/heap-wasm.ll")` に heap 連結と同じ 3 つの
  置換をした文字列に `@tz.alloc(`・`@tz.free(`・`@tz.realloc(` が残らず、`define internal` の `@tz.heap.take`・`@tz.heap.insert`・`@tz.heap.unlink` が
  それぞれ 1 回だけあることを確かめる。確認: `cargo test --locked --lib heap_wasm_threads_rename_covers_all_entry_points` が `1 passed`。
- Phase 2: `src/main.rs` の tests に `rejects_small_allocator_for_wasm`（新規、`build Main.tz --target wasm32 --allocator small` が E2000 と D11 の
  メッセージ）。`src/llvm.rs` の tests に `small_allocator_links_generated_runtime`（新規、`Small` で `@tz_small_alloc` の定義と `thread_local` が 1 つずつあり、
  既定では IR に `tz_small` が現れない）。

### E2E

1. `wasmReallocationChecks` の追加（O0・O3、各ケース新しい instance）: `a = allocate(8)`・`allocate(600)` の後 `release(a)` で `allocate(16) == a`（同じ容量 32）。
   `a = allocate(100)`・`allocate(600)`・`release(a)` の後 `allocate(40) == a`、`allocate(40) == a + 64`（容量 128 を 64 と 64 に分割）。
   `x, y, z = allocate(32)` ×3・`allocate(16)`、`release(x)`・`release(z)`・`release(y)` の後 `allocate(128) == x`（容量 48 × 3 = 144 の結合）。
   `a = allocate(1000)`・`release(a)` の後 `allocate(1000) == a` かつ `memory.buffer.byteLength` が不変（top へ返す）。
   `l1 = allocate(1000)`・`allocate(16)`・`l2 = allocate(1000)`・`allocate(16)`、`release(l2)`・`release(l1)` の後 `allocate(1000) == l1`（大きな空きは address 順）。
2. `wasmHeapStress`（新規）: xorshift32（seed `0x9e3779b9`）の 20000 操作。生きたブロックは最大 512 個、大きさは 7/8 の確率で 0〜600、1/8 で 601〜70000。
   確保・解放・`resize` を 2:1:1 で選ぶ。各ブロックの要求 bytes を `id & 0xff` で埋め、解放・`resize` の前に模様を照合する（重なりの検出）。
   null でない結果はすべて `% 16 == 0`。最後に全部を解放し、`allocate(32)` がその instance の最初の確保と同じ address を返す（全ての空きが結合され
   top へ戻った証明）。`byteLength ≤ 16 MiB`。新しい instance で `allocate(16777184)` が `WebAssembly.RuntimeError`。
3. 既存の WASM の検査（変更なし）: `tests/primitives.mjs`（`tz_coalescing` の 500 回と線形メモリの上限、16 MiB のトラップ、`tz_ownership_stress`・
   `tz_array_churn`・`tz_list_churn`）、`tests/wasm_threads.mjs`（`reserved`、100 回の反復、`byteLength ≤ 16 MiB`）、`tests/features.mjs` の各 suite の WASM、
   `tests/tasks.mjs`。native は Phase 1 で変わらない。
4. Phase 2 の単体（`node tests/small_allocator.mjs <compiler> --unit`）: `tests/heap_small_test.c`（新規）を `src/runtime/heap-small.c` と通常・ASan・TSan で
   build して実行する。全 32 区分で 10000 個の確保（alignment 16、模様による重なりの検出）と全解放の後 `tz_small_stats` の切り分け bytes と在庫 bytes が
   等しい。大きさ 0 は 16 bytes の区分、512 は小さい経路、513 は `malloc`。realloc の同じ区分（同じ address）・区分をまたぐ・小→大・大→小・大→大・
   `new_size == 0`（null）・`tz_small_free(NULL)`。4 スレッドが各 100000 操作し、共有の queue で別スレッドへ渡して解放し、join の後に統計が等しい。
5. Phase 2 の E2E（`node tests/small_allocator.mjs <compiler>`）: `tests/fixtures/primitives/Main.tz` と `tests/fixtures/tasks/Main.tz` を `--allocator small` の
   `-O0`・`-O3` で build し（tasks は `src/runtime/task.c` と `-pthread`）、C の host で呼ぶ。期待値は既存 harness の式（`tz_ownership_stress(200000) == 512`、
   `tz_coalescing(n) == 16 * (3n + 3)`、`tz_curried_churn(40000) == 2160054`、`tz_parallel_sum(c) == c(c-1)(2c-1)/6`）。IR は置換しない。各呼び出しの後、
   `tz_small_stats` で小さなブロックがすべて在庫にあることを確かめる（大きな確保の leak は system の既存 harness が証明する、D9）。

### 既存テストへの影響

- Phase 1・Phase 2: なし。期待値を変える必要が出たら停止条件 1。
- Phase 3: 既定を `small` にすると、IR の `@malloc` を置換する harness は region を数えて `live != 0` になる。次の build 呼び出しへ `--allocator system` を
  足す（期待値は変えない）: `tests/features.mjs`, `tests/primitives.mjs`, `tests/control.mjs`, `tests/computations.mjs`, `tests/strings.mjs`, `tests/tasks.mjs`,
  `tests/io.mjs`, `tests/host_imports.mjs`, `tests/gpu.mjs`, `tests/e2e.mjs`, `tests/math.mjs`, `tests/host_abi.rs`、PX01 の runner の tracked の経路。

### 性能

「計測手順」の記録だけ。CI に速度・メモリの合否条件を足さない。

## ドキュメント

| Phase | ファイル | 節 | 内容 |
| --- | --- | --- | --- |
| 1 | `docs/architecture.md` | `## 性能設計の原則` の WASM threads の heap の段落と、`tsuzuri_alloc`・`free` の段落 | 区分のリスト・boundary tag・top へ返す規則・16 MiB の不変 |
| 1 | `docs/benchmarks.md` | `## 小さい確保（PM05）`（新規、`## 現実的な次の指標` の前） | 計測条件・種目と大きさ・before／after の表（中央値・最小・最大）・`wasm_memory_bytes`。未計測の項目は未計測と書く |
| 1 | `_perfs/README.md` | 一覧の PM05 の状態欄 | Phase 1 完了の注記（`done` は Phase 3 まで、または人間が範囲を閉じたとき） |
| 2 | `docs/language.md` | 拡張 ABI の allocator の段落 | `--allocator small`、alignment 16、`tsuzuri_free` の契約が不変であること |
| 2 | `_docs/tools/command-line.md` | F13 の `--allocator` の項 | 値 `small` と E2000 |
| 2 | `docs/architecture.md`, `docs/benchmarks.md` | Phase 1 と同じ節 | native の設計と計測 |

## 受け入れ条件

- [ ] Phase 1: 「実装手順」1〜9 の確認がすべて成功し、`wasmHeapStress` が O0・O3 で成功する。
- [ ] Phase 1: 既存テストの期待値を変えていない（`git diff --stat tests/` は `tests/features.mjs` の追加だけ）。
- [ ] Phase 1: 「生成コードの確認」の IR と機械語の条件を満たす（決定性、heap の断片より前の IR が不変、トラップの位置、import が空）。
- [ ] Phase 1: before／after を PX01 形式で記録し、`docs/benchmarks.md` に計測条件とともに要約した。計測していない改善を書いていない。
- [ ] Phase 2（依頼された場合）: 手順 10〜14 の確認、ASan・TSan の単体、既定の IR の不変、`alloc_calls` が system と同じ。
- [ ] Phase 3（承認された場合）: 手順 15 の確認。
- [ ] GUIDE §14 の性能チケットの共通手順と §10 の完了の定義を満たす。

## 落とし穴

- 容量の mask 漏れ: flag 付きの +0 をそのまま容量に使うと、大きさが 1〜3 bytes ずれる。`tsuzuri_thread_heap_live_bytes` の `reserved` が合わない、
  `wasmHeapStress` の模様が壊れる、で気づく。容量の読み出しを `grep -n "load i32, ptr" src/runtime/heap-wasm*.ll` で全部見直す。
- threads の文字列置換: `heap-wasm.ll` の中の `@tz.alloc(` などは `.unlocked` へ置換される（realloc の中の確保・解放は lock の中で正しい）。補助関数から
  lock 付きの公開名を呼ばない。Rust テスト `heap_wasm_threads_rename_covers_all_entry_points` で守る。
- wasm32 の `@llvm.memcpy`・`@llvm.memset` は bulk memory なしで `memcpy`・`memset` の外部参照になる。複写は HEAD のループを使う。
- top へ返した後の bump は新しいヘッダーの +0 を丸ごと書く（古い `PREV_FREE` を残さない）。最小容量 32 を下げると footer（+28）が `next`・`prev` と重なる。
- `wasmReallocationChecks` の単体 wasm は stack-first ではなく base が実際の module と違う。テストは相対 address だけを比べる。
- 補助関数に `@llvm.trap` を置くと `runtime_kind` が `AllocationFailure` に分類しない（停止条件 2）。null を返して呼び出し元でトラップする。
- Phase 2: `generate.py` は x86_64 の triple で生成し arm64・Windows でも使う。C では `long`・`va_arg`・inline asm・target 固有の builtin・
  `__attribute__((tls_model))` を使わない。後処理が `define hidden` を `define weak hidden` に変える。macOS の `clang -r` は weak hidden を局所化するため、
  既存の `-Wl,-keep_private_externs` を外さない。
- Phase 2: PX01 の `instrumentAllocations` を small の IR に使うと region の `malloc` を数え、`live != 0` になる。tracked は必ず system で作る（D9）。
- Phase 2: `tz_small_stats` は `Task.parallel` の実行中に呼ばない（TSan が競合を報告する）。
- Phase 2 の既知の上限（`heap-small.c` に `ponytail:` の注記を書く）: region を OS へ返さない（最大量の後も RSS は下がらない）。終了したスレッドの
  キャッシュは孤立する（pool のスレッドは `atexit` まで生きるので有界。スレッドを作り続けるホストでは増える）。中央の lock は 1 つ（区分ごとの lock は
  計測で競合が見えてから）。
- `cargo test --locked <filter>` は 0 件でも成功する。`running N tests` の N を必ず見る。Node 20 は重い suite で異常終了することがあるので Node 24 を使う。

## 対象外

- GC、世代別の回収、利用者が選ぶ allocator の API と host allocator（F13）、サイズ付きの解放（D2）。
- region・ページの OS への返却と trim、終了したスレッドのキャッシュの回収、区分ごとの lock。
- WASM のヘッダーなしの確保（ページ表が要る）、top のブロックの realloc のその場での伸長、`memory.grow` の先取り、16 MiB の上限の変更（F11）。

## 決定事項

### D1: Phase 分割と範囲

- 決定: Phase 1 は WASM の区分化（ヘッダーは残す）、Phase 2 は native の `--allocator small`（既定は system）、Phase 3 は native の既定の切り替え。
  実装者は Phase 1 だけを行う。
- 理由: WASM は heap 全体を所有するため F13 なしで閉じて出荷でき、既存テストの期待値を変えない。native の既定の変更は全 harness に及ぶ。
- 状態: 既定案（実装者はこの案に従う）

### D2: サイズ付きの解放の取り下げ

- 決定: 旧 Phase 1（`@tz.free(ptr, size)`）は行わない。native は address から区分を引く（D7）。WASM はヘッダーの容量を使う。
- 理由: `src/runtime/io.c` の行バッファとホストの `tsuzuri_alloc` の領域は、確保時の大きさ（容量）と Tsuzuri が知る長さが一致しないまま所有が移る。
  `tsuzuri_free` はサイズを持たない（E05 の契約）。誤ったサイズは黙ってメモリを壊す。address による判定なら ABI と呼び出し側の IR を変えない。
- 見直し提案: 起票時の Phase 1 を置き換える。サイズ付きの解放を残す場合は、ABI をまたぐ領域の容量の規則（E05 の契約の変更）が先に要る。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D3: サイズ区分

- 決定: 16 bytes 刻みの 32 区分。WASM はヘッダー込みの容量 32〜528（要求 512 bytes まで）、native はブロック 16〜512 bytes。
- 理由: 起票時の既定案（256 まで）を、関数値の環境と短い文字列を含めるため 512 まで広げた。32 区分は i32 の mask 1 つで空でない区分を探せる。
- 状態: 既定案（実装者はこの案に従う。計測の後の変更は別チケット）

### D4: WASM の空き管理

- 決定: 設計の boundary tag（`FREE`・`PREV_FREE`・footer）、容量 528 以下の LIFO の区分、529 以上の address 順の `@tz.heap.free`、`end` に接する空きは top へ返す。
- 理由: 解放を定数手数の結合にしつつ、`wasmReallocationChecks` の address、`tz_coalescing` の線形メモリ、`reserved` をそのまま保てる（設計の手計算）。
- 状態: 既定案（実装者はこの案に従う）

### D5: `memory.grow` と上限

- 決定: HEAD のまま必要なページ数ちょうどまで伸ばす。上限 16 MiB と要求の上限 16777184 は変えない。先取りはしない。
- 理由: 線形メモリを最小に保ち決定的。V8 の grow は安価で、回数は 64 KiB ごとに償却される。上限は F11 の範囲。
- 状態: 既定案（実装者はこの案に従う）

### D6: native のスレッド安全

- 決定: スレッドキャッシュと 1 つの spinlock で守る中央のリスト。ブロックはスレッドに所有されず、リモート解放のキューを作らない。region は返さない。
- 理由: 解放の速い経路に atomic 命令がない。起票時のリスクだったリモート解放は、所有をなくすことで消える。返却をしないことで region ごとの計数も要らない。
- 状態: 既定案（実装者はこの案に従う）

### D7: native の区分の判定

- 決定: 1 MiB 境界の region、16 KiB のページ、2 段の登録表 `tz_small_map`（新規）。2^48 以上の address は system の経路。
- 理由: ヘッダーもサイズもなしで、`malloc` の大きなブロックと安全に区別できる（登録された region の中しか読まない）。
- 状態: 既定案（実装者はこの案に従う）

### D8: native の実装言語

- 決定: C（`src/runtime/heap-small.c`）を `generate.py` で IR にし、手書きの `heap-small-native.ll` から呼ぶ。
- 理由: TLS と atomic を手書きの IR で書くより誤りにくく、同じ C を ASan・TSan の単体テストで直接検査できる。`numeric.c` の既存の経路を使う。
- 状態: 既定案（実装者はこの案に従う）

### D9: 確保追跡と leak の証明

- 決定: leak のなさは生成コードの性質なので、既存の harness が system の IR（置換で数える）で証明し続ける。allocator 自身の正しさは D8 の単体テストの統計で、
  small の結合は `tests/small_allocator.mjs` の結果と統計で確かめる。harness の IR に trim や flush を足さない。PX01 の tracked は常に system で作る。
- 理由: 確保を貯める allocator では置換による `live` が region を数えて壊れる。system の IR は HEAD と同じなので、既存の期待値を 1 つも変えずに済む。
- 状態: 既定案（実装者はこの案に従う）

### D10: native の既定の切り替え

- 決定: Phase 2 の計測で G2 を確かめた後、native の既定を `small` にし、確保追跡の harness に `--allocator system` を明示する。
- 理由: 利用者の全プログラムの確保の方式とメモリの挙動（最大量の後に RSS が下がらない）を変えるため。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D11: `--allocator small` の名前と WASM での扱い

- 決定: 値の名前は `small`。wasm32 で指定すると E2000、メッセージは
  `--allocator small is only available for native targets; remove it for wasm32, which always uses the built-in size-class heap`。
- 理由: WASM の heap は Phase 1 で常に区分化されており、別の選択肢は意味を持たない。既存の `--wasm-feature` の検証と同じ E2000 を使う。
- 状態: 既定案（実装者はこの案に従う。F13 が選択肢の名前の規則を決めていれば従う）

### D12: 計測

- 決定: 計測対象の 4 種目と `binary_trees`（PX02 が done のとき）。WASM は `benchmarks/run-heap.mjs`（新規）と新しい metric `wasm_memory_bytes`、
  native は `benchmarks/run-control.mjs --allocator` と workload `host` の `peak_rss`。9 標本の中央値・最小・最大で、合否の閾値は置かない。
- 理由: HEAD に WASM の確保の計測がなく、PX01 の `METRICS` に線形メモリがない。`*_bytes` の名前は PX01 の決定的な値の規則（全標本が等しい）に合う。
- 状態: 既定案（実装者はこの案に従う。PX01 の `METRICS` への追加は PX01 の担当に知らせる）
