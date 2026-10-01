# F11: WASM メモリ上限の設定と拡張

| 項目 | 内容 |
| --- | --- |
| ID | F11 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | F13, E13 |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D9（Phase 2: 2 GiB を超える上限と memory64）は 2026-10-01 に承認。stack の溢れの検出方法は実装者に一任された（D11）。Phase 1 は承認不要 |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: WASM の線形メモリが stack・data・heap 合計 16 MiB に固定され、Rust／C++ の wasm32（最大 4 GiB）より扱えるデータが小さい |
| 手本にする既存実装 | 値を取るオプションと重複の検出: `src/main.rs` の `parse_arguments` の `--target`・`--cpu` の分岐と `next_value`。組み合わせの検証: `src/driver.rs` の `BuildOptions::validate` の `wasm_simd`・`wasm_threads` の分岐（`E2000`）。runtime 断片の文字列置換: `src/llvm.rs` の `emit_program` が threads のときに `heap-wasm.ll` の `@tz.alloc(` などを `.unlocked` へ置換する箇所。heap だけを wasm にする検査: `tests/features.mjs` の `wasmReallocationChecks` |
| 主な影響ファイル | `src/main.rs`（`parse_arguments`・`run_test_action`・ヘルプ文・tests）, `src/driver.rs`（`BuildOptions`・`validate`・`build_complete` の wasm-ld 引数）, `src/test_runner.rs`（`TestOptions`・`run_tests`・`build_runner`）, `src/llvm.rs`（`Instrumentation`・`emit_program`・tests）, `src/runtime/wasm-threads.mjs`（`createThreadPool`）, `tests/wasm_memory.mjs`（新規）, `tests/fixtures/wasm_memory/Main.tz`（新規）, `tests/wasm_threads.mjs`, `tests/test_runner.rs`, `README.md`（E2E コマンドの一覧）, `docs/language.md`, `docs/architecture.md`, `_docs/guides/webassembly.md`, `_docs/guides/wasm-threads.md`, `_docs/tools/command-line.md`, `_docs/feature-status.md`, `_features/README.md`。変更しないことを確認するもの: `src/cache.rs`, `src/runtime/heap-wasm.ll`, `src/runtime/heap-wasm-threads.ll`, `src/runtime/task-wasm-threads.c`。Phase 2 で追加: `src/runtime/heap-wasm64.ll`（新規）, `.gitignore`, `src/package.rs`, `src/llvm_abi.rs`, `src/runtime/wasm-threads.mjs`（worker の stack の範囲）, `tests/wasm64.mjs`（新規）, `tests/features.mjs`, `examples/web/simulation.mjs`, `_docs/language-reference/modules-and-packages.md` |

## 目的

画像・音声・データ処理を WASM で行うときに、プログラムの必要に応じて線形メモリの上限と main stack の大きさを指定できるようにする。
既定値（上限 16 MiB、stack 1 MiB）と既定の出力（IR・`.wasm` の byte 列）は変えず、明示した場合だけ変える。

実装者は Phase 1（上限 2 GiB まで、wasm32 のまま）だけを実装する。Phase 2（2 GiB を超える上限、memory64、manifest の `[wasm]`）は
人間が求め、D9 が承認された場合だけ着手する。2026-10-01 に D9 が承認され、Phase 2 も実装した（末尾の「Phase 2 の実装と検証」）。

## 着手条件と停止条件

### 着手条件

- 依存はない。後続の F13（allocator の差し替え）と E13（host binding）は F11 の CLI と上限の渡し方を前提にする。
- PM05（WASM の小さな確保の高速化）と `src/runtime/heap-wasm.ll` を共有する。PM05 D5 は上限の定数 `16777216`・`16777184` を変えない
  と決めている。着手前に次を実行し、HEAD と同じ数（`16777184` が 2、`16777216` が 1）であることを確かめる。

  ```sh
  cd /Users/tmidorikawa/Documents/git/Tsuzuri
  grep -c "16777184" src/runtime/heap-wasm.ll
  grep -c "16777216" src/runtime/heap-wasm.ll
  grep -n "16777184\|16777216" src/runtime/heap-wasm-threads.ll
  ```

  期待: `2`、`1`、最後の grep は出力なし。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（既定の IR と `.wasm`）を保存していること。
- Node は `node --version` が v20 以上（HEAD の作業機は v20.19.6）。2 GiB の `WebAssembly.Memory` を作れる必要がある。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `heap-wasm.ll` の定数の数が上の期待と違う（PM05 などが heap を書き換えた）。置換の対象を推測で増やさない。
- Phase 1 で `heap-wasm.ll`・`heap-wasm-threads.ll` の命令（定数以外）を変えたくなった。2 GiB 以下では命令を変えずに正しい（D2）。
- オプションを指定しない build の IR か `.wasm` が、手順 1 のベースラインと byte 単位で一致しない。
- 既存テストの期待値（16 MiB の検査、診断コード、メッセージ）を変える必要がある。表明の追加だけは除く。
- wasm-ld が `--shared-memory` と 16 MiB 以外の `--max-memory` の組を拒否する、または Node が 2 GiB の shared memory を作れない。
  上限を黙って下げない。
- worker の stack（256 KiB、`tsuzuri_thread_stack_size`）を変えたくなった（D4 の範囲外）。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- link: `src/driver.rs` の `build_complete` は `options.emit == Emit::Wasm` のとき wasm-ld に `--no-entry --stack-first -z stack-size=1048576
  --max-memory=16777216` を渡す。`options.wasm_threads` なら `--shared-memory --import-memory --export-memory` と threads の export 群と
  threads object を足す。`--wasm-feature threads --emit object` は `wasm-ld -r` だけで、メモリの引数はない。
- test: `src/test_runner.rs` の `build_runner` は `test --target wasm32` の link に同じ 3 定数（`--stack-first`、`stack-size=1048576`、
  `--max-memory=16777216`）を直書きしている。`TestOptions` の field は `target`・`optimization`・`filter`・`indices` だけ。
- heap: `src/runtime/heap-wasm.ll` は `@__heap_base` を 16 に揃えた位置からの bump と、16 bytes のヘッダーを持つ address 順の空きリスト。
  `@tz.alloc` は `%fits = icmp ule i64 %size, 16777184` と、bump の `%end = add i32 %begin, %needed` の後の
  `%within = icmp ule i32 %end, 16777216` で上限を検査し、`@llvm.wasm.memory.size.i32`・`@llvm.wasm.memory.grow.i32` で必要な page 数
  ちょうどまで伸ばす。`@tz.realloc` は `%fits = icmp ule i64 %new_size, 16777184`。失敗はすべて `@llvm.trap()`（JS では
  `WebAssembly.RuntimeError`）。計算はすべて `i32`。
- 連結: `src/llvm.rs` の `emit_program` は、heap 関数を使う IR に `heap-wasm.ll`（wasm）か `heap-native.ll` を足す。
  `instrumentation.wasm_threads` なら先に `heap-wasm-threads.ll` を足し、`heap-wasm.ll` の `@tz.alloc(`・`@tz.free(`・`@tz.realloc(` を
  `.unlocked` 名へ置換して足す。`Instrumentation` の field は `traps`・`debug`・`cpu_dispatch`・`wasm_threads`（`#[derive(Default)]`）。
- threads: `heap-wasm-threads.ll` の `tsuzuri_thread_stack_alloc` は worker ごとに heap から 262144 bytes を取り、
  `src/runtime/task-wasm-threads.c` の `tsuzuri_thread_stack_size` は 262144 を返す。`src/runtime/wasm-threads.mjs` の
  `createThreadPool(bytes, { memory = new WebAssembly.Memory({ initial: 256, maximum: 256, shared: true }), ... })` は既定で 16 MiB の
  shared memory を最初から全部作る。そのため threads では `memory.grow` が起きない。
- cache: `src/cache.rs` の `build_key` は `format!("{options:?}")`（field 名 `options`）と、runtime を含む IR（`ir-and-embedded-runtime`）を
  hash する。`BuildOptions` に field を足せば key に自動で入る。
- CLI: `parse_arguments` にメモリのオプションはない。`--wasm-feature` は build だけ、`--target` は build と test だけ（run は native のみ）。
  `parse_arguments` の `Err(String)` は `main` が `E2000` で報告する。wasm-ld の失敗は `run_tool` が `E2002` にし、hint
  「install LLVM LLD or set TSUZURI_WASM_LD to the wasm-ld executable」を付ける。
- 16 MiB を表明するテスト: `tests/primitives.mjs`（`tz_allocation_limit` のトラップと `byteLength`）、`tests/strings.mjs`、`tests/e2e.mjs`、
  `tests/control.mjs`、`tests/computations.mjs`、`tests/tasks.mjs`、`tests/gpu.mjs`、`tests/host_imports.mjs`、`tests/display_parse.mjs`、
  `tests/features.mjs`（各 suite の `WASM stays within 16 MiB` と `wasmReallocationChecks` の `--max-memory=16777216`）、`tests/wasm_threads.mjs`、
  `tests/host_abi.rs`、`tests/trap_locations.rs`、`tests/math.mjs`。すべて既定の build なので F11 の後も成り立つ。
- 文書: `docs/language.md` の 3 か所（確保失敗の段落「WASM のヒープ上限は16 MiBのまま」、文字列の段落「合計して 16 MiB」、再帰とスタックの
  「stack-first 配置で 1 MiB のスタック、線形メモリ上限 16 MiB」）、`docs/architecture.md` の 16 MiB の記述、`_docs/guides/webassembly.md`、
  `_docs/guides/wasm-threads.md`（「同梱 Node ホストは全 256 page を最初に確保します」）。

### 再現（2026-09-29 に確認）

宣言された page 数は次の小さな parser で読む（`llvm-readobj` 21 の `MaxPages` は誤った値 280 を表示したので使わない）。

```javascript
// /tmp/tz-work-F11/limits.cjs: prints the memory section and memory import limits (pages)
const b = require("fs").readFileSync(process.argv[2]); let p = 8;
const leb = () => { let r = 0, s = 0, x; do { x = b[p++]; r += (x & 127) * 2 ** s; s += 7; } while (x & 128); return r; };
const name = () => { const n = leb(); const t = b.slice(p, p + n).toString(); p += n; return t; };
while (p < b.length) {
  const id = b[p++], n = leb(), end = p + n;
  if (id === 5) { for (let c = leb(); c--;) { const f = b[p++], mn = leb(), mx = f & 1 ? leb() : null; console.log("memory flags", f, "min", mn, "max", mx); } }
  if (id === 2) { for (let c = leb(); c--;) { const m = name(), f = name(), k = b[p++];
    if (k === 0) leb(); else if (k === 1) { p++; const lf = b[p++]; leb(); if (lf & 1) leb(); }
    else if (k === 2) { const lf = b[p++], mn = leb(), mx = lf & 1 ? leb() : null; console.log("import", m, f, "flags", lf, "min", mn, "max", mx); }
    else if (k === 3) p += 2; else { p++; leb(); } } }
  p = end;
}
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build tests/fixtures/tasks/Main.tz --target wasm32 -O3 --no-cache -o /tmp/tz-work-F11/plain.wasm
target/release/tsuzuri build tests/fixtures/tasks/Main.tz --target wasm32 --wasm-feature threads -O3 --no-cache -o /tmp/tz-work-F11/threads.wasm
node /tmp/tz-work-F11/limits.cjs /tmp/tz-work-F11/plain.wasm
node /tmp/tz-work-F11/limits.cjs /tmp/tz-work-F11/threads.wasm
target/release/tsuzuri build tests/fixtures/tasks/Main.tz --target wasm32 --wasm-max-memory 64MiB -o /tmp/tz-work-F11/x.wasm
```

結果（build は fixture の既存の `W1001` 警告を 2 件出す）:

```text
memory flags 1 min 17 max 256
import env memory flags 3 min 17 max 256
<command line>:1:1: error[E2000]: unknown option '--wasm-max-memory'; use --help
```

min 17 page は stack 1 MiB（16 page）と静的 data の 1 page。threads の import は flags 3（最大値あり・shared）。

## 仕様

### 前提とする他チケットのインターフェース

F11 は他チケットの完了を前提にしない。`heap-wasm.ll` と `--target` を共有するチケットとは、次の前提をそろえる。

- PM05: `heap-wasm.ll` の次の 3 行（前後の空白を含めて完全一致）が各 1 回だけ現れる。F11 はこの 3 行だけを置換する（D3）。PM05 が
  値の名前を変える場合は、F11 の unit test `wasm_heap_limit_lines_are_unique`（新規）が失敗するので、同じ変更で置換の対象を直す。
  heap の終端は常に上限以下で、上限が 2 GiB 以下なら `i32` の加算が折り返さない（D2 の証明）ことも保つ。

  ```llvm
    %fits = icmp ule i64 %size, 16777184
    %within = icmp ule i32 %end, 16777216
    %fits = icmp ule i64 %new_size, 16777184
  ```

- F13: D6 のとおり wasm32 は常に module 内の heap を使う。F11 の上限は `heap-wasm.ll` の定数と wasm-ld の `--max-memory` にだけ効く。
  F13 Phase 2 で WASM に host allocator を入れる場合も、`--wasm-max-memory` は wasm-ld の `--max-memory` を決める。
- G15: native の `--target <llvm-triple>` を足しても `wasm32` の値は残る。F11 の検査は `options.target == Target::Wasm32` だけを見る。
  G15 が `Target` に triple を持たせたら、この条件を「wasm32 か」の判定へ置き換える。Phase 2 の memory64 の target 名（`wasm64`）は G15 の
  命名に合わせる。
- E13・E05: host ABI の descriptor（offset 0 の `i32` pointer）は変えない。Phase 1 の上限 2 GiB では pointer が 2^31 未満なので、JS が
  受け取る `i32` の pointer は負にならない。

### CLI

新 API（実装後に有効。未検証）。値は次の文法で、次の引数として渡す（`--target wasm32` と同じ形。`=` 形式は受けない）。

```text
option = "--wasm-max-memory" size | "--wasm-stack-size" size
size   = digit { digit } [ "KiB" | "MiB" | "GiB" ]
```

値は `digits × 1024^k`（k は接尾辞なしで 0、`KiB` 1、`MiB` 2、`GiB` 3）。接尾辞は大文字小文字を区別し、空白・`_`・`+`・小数は受けない。
`u64` を超える値は書式の誤りとする。各オプションは一回だけ指定できる。

| action と出力 | `--wasm-max-memory` | `--wasm-stack-size` |
| --- | --- | --- |
| `build --target wasm32`（既定の `--emit wasm`） | 受理。wasm-ld の `--max-memory` と heap の定数 | 受理。wasm-ld の `-z stack-size` |
| `build --target wasm32 --emit object`・`--emit llvm` | 受理。heap の定数だけ（link は利用者が同じ値で行う） | `E2000` |
| `build --target wasm32 --emit header` | `E2000` | `E2000` |
| `build` の native（`exe`・`object`・`llvm`・`header`）と `--emit wgsl` | `E2000` | `E2000` |
| `test --target wasm32` | 受理 | 受理 |
| `test`（native）、`run`、`check`、`doc`、`fmt`、`lsp` | `E2000` | `E2000` |

`--wasm-feature threads` との組み合わせも同じ表に従う。threads の `--emit object` は `wasm-ld -r` だけなので heap の定数だけが効く。

### 値の範囲

- `--wasm-max-memory`: 65536 の倍数、2147483648（2 GiB、32768 page）以下、実効の stack の大きさ + 65536 以上。既定は 16777216。
- `--wasm-stack-size`: 16 の倍数、65536 以上。既定は 1048576。
- 実効値は「指定値、なければ既定値」。`--wasm-stack-size 16MiB` だけを指定すると、実効の上限 16 MiB が stack + 64 KiB より小さいので `E2000`。
- 既定値を明示した build（`--wasm-max-memory 16MiB --wasm-stack-size 1MiB`）の IR と `.wasm` は、指定しない build と byte 単位で一致する。

### 数値・トラップ・native と WASM の差

- heap の要求の上限は `max − 32`（`%fits` の定数。既定で 16777184）。これを超える要求、heap の終端が `max` を超える bump、`memory.grow` の失敗は
  すべて従来どおり `@llvm.trap()`（JS では `WebAssembly.RuntimeError`）。`--trap-info` の扱いも変えない。
- 線形メモリの初期 page 数は wasm-ld が stack と静的 data から決め（既定で 17 page）、非 threads では `memory.grow` で `max` まで伸びる。
- stack は stack-first のまま。stack の溢れは 0 より下へ折り返して 2^32 − frame 付近の address になり、上限が 2 GiB 以下なら 2 GiB 未満の frame で
  必ず範囲外アクセスのトラップになる（HEAD と同じ性質）。
- threads: `env.memory` の import は flags 3（shared）、min は wasm-ld の初期 page 数、max は `max / 65536`。同梱の Node ホストは宣言の max を
  初期値にも使う（D5）。threads では従来どおり `memory.grow` が起きない。worker の stack は 256 KiB 固定で heap から取る（D4）。
- native には効かない（`E2000`）。native の意味と出力は変わらない。

### 診断

`<command line>` は `parse_arguments` と `BuildOptions::validate` の診断が使う `Span::default()` の位置。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E2000` | 値の書式の誤り | `--wasm-max-memory must be a byte count or a number followed by KiB, MiB, or GiB, such as 67108864 or 64MiB`（stack は `--wasm-stack-size ... such as 4194304 or 4MiB`） | `<command line>` |
| `E2000` | 重複 | `WASM maximum memory specified more than once`、`WASM stack size specified more than once` | `<command line>` |
| `E2000` | build・test 以外の action | `--wasm-max-memory and --wasm-stack-size are only valid with build or test` | `<command line>` |
| `E2000` | test で `--target wasm32` がない | `--wasm-max-memory and --wasm-stack-size require --target wasm32` | `<command line>` |
| `E2000` | build で上限が使えない target・出力 | `--wasm-max-memory requires wasm32 object, LLVM IR, or WASM output` | `<command line>` |
| `E2000` | build で stack が使えない出力 | `--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size` | `<command line>` |
| `E2000` | 上限の範囲 | `--wasm-max-memory must be a multiple of 64 KiB and at most 2 GiB` | `<command line>` |
| `E2000` | stack の範囲 | `--wasm-stack-size must be a multiple of 16 bytes and at least 64 KiB` | `<command line>` |
| `E2000` | 上限 < stack + 64 KiB | `--wasm-max-memory must be at least the stack size plus 64 KiB; raise the memory limit or lower --wasm-stack-size` | `<command line>` |
| `E2002` | 静的 data が上限に収まらず wasm-ld が失敗 | wasm-ld の出力（例 `maximum memory too small, N bytes needed`）と既存の hint。どちらかのオプションを指定したときだけ hint の後に `; if the memory limit is too small, raise --wasm-max-memory or lower --wasm-stack-size` を足す | `<command line>` |

検査の順序: `parse_arguments` の検査（書式、重複、action、test の target）→ `BuildOptions::validate` の既存の検査すべて → 新しい検査（target・出力、
範囲、stack との関係）。新しい検査は `validate` の最後（`Cpu::Native` の分岐の後）に置き、既存の診断の優先順位を変えない。

### 資源上限

- Phase 1 の上限は 2 GiB。4 GiB 近くまでの拡張は Phase 2（D9）。
- worker 数の上限（`createThreadPool` の 31）と worker の stack（256 KiB）は変えない。
- 大きな上限でも、非 threads の初期 page 数は変わらない（必要な分だけ伸びる）。threads は宣言の max を最初に作る。

### 例

検証済み（HEAD の `target/release/tsuzuri`）: 次の `/tmp/tz-work-F11/a/Main.tz` は `check` が成功し、wasm32 `-O0` で `tz_bytes_length(1048576n)` が
`1048576n`（`byteLength` 2162688）、`tz_bytes_length(15728640n)` が `WebAssembly.RuntimeError`（16 MiB に収まらない）、import は空、
`tsuzuri test /tmp/tz-work-F11/a --target wasm32` が `1 passed; 0 failed; 0 ignored`。

```tsuzuri
export def bytes_length :: i64 -> i64 = \count -> (new [ubyte](count, index -> index as ubyte)).length

test "allocates the requested bytes" = assert (bytes_length 1024 == 1024)
```

新 API（実装後に有効。未検証）:

```sh
tsuzuri build app --target wasm32 --wasm-max-memory 64MiB -o app.wasm
tsuzuri build app --target wasm32 --wasm-max-memory 256MiB --wasm-stack-size 4MiB -o app.wasm
tsuzuri build app --target wasm32 --wasm-feature threads --wasm-max-memory 1GiB -o app.wasm
tsuzuri build app --target wasm32 --emit object --wasm-max-memory 64MiB -o app.o
wasm-ld app.o --no-entry --stack-first -z stack-size=1048576 --max-memory=67108864 --export-all -o app.wasm
tsuzuri test app --target wasm32 --wasm-max-memory 64MiB
```

上限 64 MiB の既定 stack では、上の fixture の `bytes_length 65011712`（62 MiB）が成功し、`bytes_length 66060288`（63 MiB）がトラップする。
計算: 要求 n の block は `(n + 31) & -16 = n + 16` bytes。heap の開始は 1 MiB（stack）+ 静的 data（64 KiB 以下。min 17 page から）以上なので、
63 MiB は 1 MiB + 63 MiB + 16 > 64 MiB で必ず失敗し、62 MiB は 1 MiB + 64 KiB + 62 MiB + 16 ≤ 64 MiB で必ず成功する。

拒否される例（すべて `E2000`）: `--wasm-max-memory 64MB`（書式）、`--wasm-max-memory 100000`（64 KiB の倍数でない）、`--wasm-max-memory 4GiB`
（2 GiB 超）、`--wasm-max-memory 1MiB`（stack 1 MiB + 64 KiB 未満）、`--wasm-stack-size 1000`（16 の倍数でない）、`run app --wasm-max-memory 64MiB`、
native の `build app --wasm-max-memory 64MiB`、`build app --target wasm32 --emit object --wasm-stack-size 2MiB`、
`test app --wasm-max-memory 64MiB`（`--target wasm32` なし）。

### Phase 2（設計方針）

- 2 GiB 超から 4 GiB − 64 KiB まで: heap の終端の計算を折り返さない形（`%room = sub i32 LIMIT, %begin` と比較、または `i64`）へ変える。
  上限を 4 GiB − 64 KiB にするのは、`%ceil = add i32 %end, 65535` が折り返さない最大だから。JS が受け取る `i32` の pointer（`tsuzuri_alloc`、
  `wasm-threads.mjs` の `base + tsuzuri_thread_stack_size()`、E05・E13 の glue）を `>>> 0` で扱うよう監査する。stack の溢れの折り返し先が
  範囲内になりうるので、64 KiB を超える frame の扱い（probe か拒否）を決める。
- memory64（`--target wasm64`）: pointer・`%tz.abi.buffer`・descriptor が 64-bit になり、host glue の ABI 版を分ける。Node の対応版を確かめる。
- manifest の `[wasm]`（`max-memory`・`stack-size`。root package だけ、CLI が優先）（D10）。

実装した形は D10–D12 と末尾の「Phase 2 の実装と検証」にある。heap は `%room` の形、frame の大きさは prologue 後の入口検査（D11）で
扱い、probe も拒否も要らなくなった。
## 設計

### データ構造

```rust
// src/driver.rs
pub const DEFAULT_WASM_MAX_MEMORY: u32 = 16 * 1024 * 1024; // （新規）
pub const DEFAULT_WASM_STACK_SIZE: u32 = 1024 * 1024; // （新規）

pub struct BuildOptions {
    // 既存の field はそのまま
    pub wasm_max_memory: Option<u64>, // （新規）None は既定値
    pub wasm_stack_size: Option<u64>, // （新規）None は既定値
    pub cache: bool,
}

/// Returns the effective (maximum memory, stack size) or an E2000 range diagnostic.
pub fn wasm_memory_limits(max: Option<u64>, stack: Option<u64>) -> Result<(u32, u32), Diagnostic>; // （新規）

// src/test_runner.rs の TestOptions に同じ 2 field（新規）。Default は None。
// src/llvm.rs
pub(crate) fn with_wasm_heap_limit(ir: String, limit: u32) -> String; // （新規）
// src/main.rs
fn parse_size(value: &OsStr, option: &str) -> Result<u64, String>; // （新規）
```

`Option<u64>` にする理由: 4 GiB のような `u32` を超える値を書式の誤りでなく範囲の誤りとして報告するため。`BuildOptions` は `Copy` のまま。
`Option` なので `#[derive(Default)]` 相当の既定（`None`）が「既定値」を意味し、0 と混同しない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `parse_arguments` | 2 オプションの分岐（`next_value` → `parse_size`、重複の検出）、action の検査、test の `--target wasm32` の検査、`BuildOptions` への格納 |
| CLI | `src/main.rs` | 使い方の文（`[--target native\|wasm32]` と `--target native\|wasm32  Target (default: native)` の行） | 2 オプションの行を足す |
| CLI | `src/main.rs` | `run_test_action` | `driver::TestOptions` に 2 field を渡す |
| driver | `src/driver.rs` | `BuildOptions`、`Default`、`validate` | field と既定、`validate` の最後に新しい検査（target・出力、`wasm_memory_limits`） |
| driver | `src/driver.rs` | `wasm_memory_limits`（新規）、定数 2 つ（新規） | 範囲と stack との関係の検査。実効値を返す |
| driver | `src/driver.rs` | `build_complete` | `options.target == Target::Wasm32 && options.emit != Emit::Header` の分岐の先頭で `text = llvm::with_wasm_heap_limit(text, max)`。wasm-ld の `stack-size=` と `--max-memory=` を実効値から作る。オプション指定時だけ link の hint を延ばす |
| test | `src/test_runner.rs` | `TestOptions`、`Default`、`run_with_timeout`、`build_runner` | field、先頭で `wasm_memory_limits`、wasm のとき IR の置換と link 引数 |
| llvm | `src/llvm.rs` | `with_wasm_heap_limit`（新規）、`mod tests` | 3 行の完全一致の置換。tests は D3 の 3 件 |
| cache | `src/cache.rs` | `build_key` | 変更なし（`options` の Debug と置換後の IR が key に入る。D7） |
| runtime | `src/runtime/heap-wasm.ll`、`heap-wasm-threads.ll`、`task-wasm-threads.c` | – | 変更なし |
| host | `src/runtime/wasm-threads.mjs` | `createThreadPool`、`memoryLimits`（新規） | `memory` を省略したら module の `env.memory` の宣言から作る（D5） |

### 生成 IR とランタイム

`--wasm-max-memory 64MiB` の IR では、heap の 3 行だけが次に変わる（`67108864 − 32 = 67108832`）。ほかの行、関数の順序、metadata は変わらない。

```llvm
  %fits = icmp ule i64 %size, 67108832
  %within = icmp ule i32 %end, 67108864
  %fits = icmp ule i64 %new_size, 67108832
```

wasm-ld の引数は `--no-entry --stack-first -z stack-size=<stack> --max-memory=<max>`（既定で HEAD と同じ文字列）。threads の `.wasm` の
import は `env.memory`（flags 3、min は wasm-ld が決める、max = `max / 65536`）。

命令を変えずに 2 GiB まで正しい理由（L = 上限、L は 65536 の倍数で L ≤ 2^31）:

- `@tz.alloc`: `%fits` で size ≤ L − 32。`%rounded = small + 31 ≤ L − 1`、`%needed ≤ L − 16`。`%begin` は L 以下（`@tz.heap.end` は検査を通った値だけを
  保存し、`__heap_base` を 16 に揃えた値も L 以下）。よって `%end = begin + needed ≤ 2L − 16 ≤ 2^32 − 16` で折り返さない。`%within` の後は
  `%end ≤ L` なので `%ceil = end + 65535 < 2^32`。
- `@tz.realloc`: `%wanted = block + capacity ≤ L`、`%combined = capacity + neighbor_size ≤ L`（どちらも heap の中の block）。
- 置換後も `%fits` の定数は `L − 32` で、既定では 16777184 のまま。

### アルゴリズム

```text
parse_size(value, option):
  text = value as UTF-8, else Err(syntax message)
  split text into digits prefix and suffix; suffix in {"", "KiB", "MiB", "GiB"} else Err
  digits must be non-empty ASCII digits; n = parse u64 else Err
  n.checked_mul(1024^k) else Err
wasm_memory_limits(max, stack):
  s = stack.unwrap_or(DEFAULT_WASM_STACK_SIZE); m = max.unwrap_or(DEFAULT_WASM_MAX_MEMORY)
  if s % 16 != 0 or s < 65536: E2000 (stack range)
  if m % 65536 != 0 or m > 2^31: E2000 (max range)
  if m < s + 65536: E2000 (relation)
  Ok((m as u32, s as u32))
with_wasm_heap_limit(ir, limit):
  if limit == DEFAULT_WASM_MAX_MEMORY: return ir
  for each line of ir (split_inclusive '\n'), compare line without trailing "\r\n"/"\n":
    "  %fits = icmp ule i64 %size, 16777184"     -> same text with limit - 32
    "  %within = icmp ule i32 %end, 16777216"    -> same text with limit
    "  %fits = icmp ule i64 %new_size, 16777184" -> same text with limit - 32
  keep the original line ending
```

行の完全一致にする理由: 利用者の文字列定数は `c"..."` として global の行の途中に出るので、部分文字列の置換では文字列の中身を書き換えうる。
LLVM の `c"..."` は改行を `\0A` に escape するので、行全体が一致するのは runtime の行だけ。

`memoryLimits(bytes)`（`wasm-threads.mjs`、新規）: 「再現」の parser と同じ手順で import section（id 2）を読み、module `env`・name `memory`・kind 2 の
flags・min・max を返す。見つからない、または flags に shared（2）と max（1）がなければ `TypeError("WASM threads module must import a shared
env.memory with a maximum; build it with --wasm-feature threads")`。`createThreadPool` の `memory` の既定値を削除し、省略時は次のとおり作る。

```javascript
// 新 API（実装後に有効。未検証）
if (memory === undefined) {
  const pages = bytes instanceof WebAssembly.Module ? 256 : memoryLimits(bytes).maximum;
  try {
    memory = new WebAssembly.Memory({ initial: pages, maximum: pages, shared: true });
  } catch (cause) {
    throw new RangeError(`cannot reserve ${pages * 64} KiB of shared WebAssembly.Memory; build with a smaller --wasm-max-memory`, { cause });
  }
}
```

`WebAssembly.Module` を渡され `memory` を省略した場合は HEAD と同じ 256 page（宣言を読めないため）。16 MiB 以外の module ではこの形を使えないことを
文書に書く。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、既定の出力を保存する。「着手条件」の定数の数を確かめる。
- 確認: 次がすべて成功し、6 ファイルができる（fixture の `W1001` 警告 2 件は既存）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-f11/before
for o in 0 3; do
  target/release/tsuzuri build tests/fixtures/tasks/Main.tz --target wasm32 -O$o --no-cache -o /tmp/tz-f11/before/plain-O$o.wasm
  target/release/tsuzuri build tests/fixtures/tasks/Main.tz --target wasm32 --wasm-feature threads -O$o --no-cache -o /tmp/tz-f11/before/threads-O$o.wasm
  target/release/tsuzuri build tests/fixtures/tasks/Main.tz --target wasm32 --emit llvm -O$o --no-cache -o /tmp/tz-f11/before/plain-O$o.ll
done
```

### 手順 2: heap の行の置換（呼び出し元なし）

- 変更: `src/llvm.rs` の `with_wasm_heap_limit`（新規）と `mod tests`。
- 内容: 「アルゴリズム」のとおり。`DEFAULT_WASM_MAX_MEMORY` はまだないので、この手順では `16 * 1024 * 1024` を関数内の定数にし、手順 3 で
  driver の定数へ置き換える。tests は「Rust テスト」の 3 件。
- 確認: `cargo test --locked --lib wasm_heap_limit` が `running 3 tests` で 3 passed。

### 手順 3: `BuildOptions` と検証

- 変更: `src/driver.rs` の定数 2 つ（新規）、`BuildOptions`・`Default`・`validate`、`wasm_memory_limits`（新規）、`mod tests`。
  `src/main.rs` の `BuildOptions { ... }`（`parse_arguments` の末尾の完全な literal）に `wasm_max_memory: None, wasm_stack_size: None`。
- 内容: `validate` の最後に、`wasm_max_memory` が `Some` で target が wasm32 でないか emit が `Object`・`Llvm`・`Wasm` 以外なら上限の `E2000`。
  `wasm_stack_size` が `Some` で target が wasm32 でないか emit が `Wasm` でなければ stack の `E2000`。その後 `wasm_memory_limits(...)?`。
- 確認: `cargo test --locked --lib wasm_memory` が `running 2 tests` で 2 passed。`cargo test --locked` が成功する。

### 手順 4: CLI

- 変更: `src/main.rs` の `parse_arguments`・`parse_size`（新規）・`HELP`・`run_test_action`・`mod tests`、`src/test_runner.rs` の `TestOptions` と
  `Default`、`tests/test_runner.rs` の `TestOptions` の完全な literal（`..options` を使わない箇所）に 2 field の `None`。
- 内容: 分岐は `--target` と同じ形（重複は先に検出）。action の検査は `--wasm-feature` の検査の隣に置く。`HELP` は build options に 2 行
  （`--wasm-max-memory SIZE  WASM linear memory limit (wasm32 build/test; default 16MiB, at most 2GiB)`、
  `--wasm-stack-size SIZE   WASM main stack size (wasm32 WASM output/test; default 1MiB)`）、test の usage 行に 2 オプションを足す。
- 確認: `cargo test --locked --bin tsuzuri parses_wasm_memory_sizes` と `cargo test --locked --bin tsuzuri rejects_ambiguous_or_unused_arguments`
  がそれぞれ 1 passed。`cargo test --locked` が成功する。

### 手順 5: build の link と IR

- 変更: `src/driver.rs` の `build_complete`。
- 内容: `let (max_memory, stack_size) = wasm_memory_limits(options.wasm_max_memory, options.wasm_stack_size)?;` を emission の前に置く。wasm32 の
  分岐（`options.target == Target::Wasm32 && options.emit != Emit::Header`）の先頭で `text = llvm::with_wasm_heap_limit(text, max_memory);`。
  wasm-ld の 2 引数を `format!("stack-size={stack_size}")`・`format!("--max-memory={max_memory}")` にする。どちらかのオプションが `Some` なら
  `run_tool` の hint に D8 の文を足す。
- 確認: `cargo build --release --locked` の後、手順 1 と同じ 6 ファイルを `/tmp/tz-f11/after/` へ作り `cmp` がすべて出力なし。
  `--wasm-max-memory 16MiB --wasm-stack-size 1MiB` を足した 6 ファイルも `cmp` が出力なし。`--wasm-max-memory 64MiB` の plain は
  `node /tmp/tz-work-F11/limits.cjs` が `memory flags 1 min 17 max 1024`、threads は `import env memory flags 3 min 17 max 1024`。

### 手順 6: test の link と IR

- 変更: `src/test_runner.rs` の `run_with_timeout`（`optimization` の検査の直後で `wasm_memory_limits`）と `build_runner`（wasm のとき
  `llvm::emit_test_runner` の結果を置換し、link 引数を実効値から作る）、`tests/test_runner.rs` の
  `wasm_memory_options_reach_the_wasm_test_link`（新規）。
- 確認: `cargo test --locked --test test_runner wasm_memory_options_reach_the_wasm_test_link` が 1 passed。

### 手順 7: threads のホスト

- 変更: `src/runtime/wasm-threads.mjs` の `createThreadPool`・`memoryLimits`（新規）、`tests/wasm_threads.mjs`。
- 内容: 「アルゴリズム」のとおり。`memoryLimits` を export する（`tests/wasm_threads.mjs` は使わず、独立した parser で照合する）。
- 確認: `node --check src/runtime/wasm-threads.mjs`、`node tests/wasm_threads.mjs target/release/tsuzuri` が成功し、新しい行を表示する。

### 手順 8: E2E `wasm_memory`

- 変更: `tests/fixtures/wasm_memory/Main.tz`（新規）、`tests/wasm_memory.mjs`（新規）、`README.md` の E2E のコマンド一覧
  （`node tests/wasm_threads.mjs target/release/tsuzuri` の行の後）。
- 内容: fixture は「例」の検証済みの 1 行と、1 件目と同じ形の `test "allocates 32 MiB" = assert (bytes_length 33554432 == 33554432)` を足した 3 行。
  script は「E2E」の 1〜8。
- 確認: `node --check tests/wasm_memory.mjs`、`node tests/wasm_memory.mjs target/release/tsuzuri` が `-O0`・`-O3` の行を表示して終了コード 0。

### 手順 9: 既存の WASM の検査

- 変更: なし。
- 内容: 既定の出力が不変であることを既存 suite で確かめる。Node 20 が V8 の `RepresentationChangerError` で落ちたら Node 24 で再実行する
  （`npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri`）。
- 確認: 次がすべて成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for s in e2e primitives strings control computations tasks host_imports display_parse math wasm_threads cache; do node tests/$s.mjs target/release/tsuzuri || echo "FAILED $s"; done
node tests/features.mjs target/release/tsuzuri
cargo test --locked --test trap_locations --test host_abi --test test_runner
```

### 手順 10: 文書と最終確認

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/guides/webassembly.md _docs/guides/wasm-threads.md _docs/tools/command-line.md` が成功する。
  `cargo test --locked` が成功する。GUIDE §10 の完了の定義のコマンドがすべて成功する。

## テスト計画

### Rust テスト

- `src/llvm.rs` の `mod tests`:
  - `wasm_heap_limit_lines_are_unique`（新規）: `include_str!("runtime/heap-wasm.ll")` の行のうち、3 つの対象行がそれぞれちょうど 1 回。
    `heap-wasm-threads.ll` には 0 回。
  - `wasm_heap_limit_default_is_identity`（新規）: `heap-wasm.ll` の本文と、その改行を `\r\n` にした本文で、`with_wasm_heap_limit(text, 16777216)` が
    入力と一致する。
  - `wasm_heap_limit_rewrites_whole_lines_only`（新規）: 上限 67108864 で 3 行が `67108832`・`67108864`・`67108832` になり、本文に `16777184` と
    `16777216` が残らない。行の途中に対象行の文字列を含む `@s = private constant [N x i8] c"  %within = icmp ule i32 %end, 16777216"` は変わらない。
    `\r\n` の本文でも行末が保たれる。
- `src/driver.rs` の `mod tests`:
  - `wasm_memory_limits_accept_boundaries_and_reject_invalid_values`（新規）: `(None, None)` → `(16777216, 1048576)`、`(Some(2147483648), None)`・
    `(Some(131072), Some(65536))` を受理。`Some(2147549184)`（2 GiB + 64 KiB）、`Some(100000)`、`Some(0)`、`Some(1048576)`（既定 stack と同じ）、
    stack `Some(1000)`・`Some(32768)` を `E2000`。メッセージは「診断」の文と一致する。
  - `validate_rejects_wasm_memory_options_outside_wasm_outputs`（新規）: native の exe・object に上限、wasm32 の header に上限、wasm32 の
    object・llvm に stack を `E2000`。wasm32 の object・llvm に上限、wasm の両方、threads の object に上限を受理。
- `src/main.rs` の `mod tests`:
  - `parses_wasm_memory_sizes`（新規）: `65536`・`4KiB`・`64MiB`・`1GiB` を 65536・4096・67108864・1073741824 に。`64MB`、`64 MiB`、空文字列、
    `0x10`、`1.5MiB`、`18446744073709551616`、`17179869184GiB` を拒否。build（wasm32）と `test --target wasm32` で両方を受理し、値が
    `options` に入る。
  - `rejects_ambiguous_or_unused_arguments`（拡張）: `run`・`check`・`doc`・`fmt` にどちらか、重複、`test` で `--target wasm32` なし、
    `--wasm-max-memory=64MiB`（`unknown option`）を足す。
- `tests/test_runner.rs` の `wasm_memory_options_reach_the_wasm_test_link`（新規）: fixture の本文で、wasm32 `-O0` の既定では
  `allocates 32 MiB` だけが failure、`wasm_max_memory: Some(67108864)` では failure なし。

### E2E

`tests/wasm_memory.mjs`（新規。`tests/wasm_threads.mjs` の `cli` と一時ディレクトリの形を写す）。fixture は各 case の一時 root へ複写する
（E03 の再帰探索）。page 数は「再現」の parser と同じ独立した実装で読む。`-O0` と `-O3` で次を行う。期待値はすべて「例」の計算から来る。

1. 既定: 省略と既定値の明示の `.wasm` が byte 単位で一致。`max 256`、`WebAssembly.Module.imports` が空。
2. 64 MiB: `max 1024`、min は 1 と同じ、import は空。呼び出しごとに新しい instance で、`tz_bytes_length(33554432n)` と
   `tz_bytes_length(65011712n)` が引数と同じ値、`tz_bytes_length(66060288n)` が `WebAssembly.RuntimeError`。`byteLength` は 64 MiB 以下。
3. stack: `--wasm-stack-size 4MiB --wasm-max-memory 64MiB` の min が 1 の min + 48（`(4 MiB − 1 MiB) / 64 KiB`）。
4. 2 GiB: `--wasm-max-memory 2GiB` の `max 32768`、`tz_bytes_length(33554432n)` が成功（GiB 単位の確保はしない）。
5. IR: `--emit llvm --wasm-max-memory 64MiB`、同じく `--trap-info`、`-g` の 3 つで、対象の 3 行（新しい値）がそれぞれ 1 回、古い 3 行が 0 回。
6. object: `--emit object --wasm-max-memory 64MiB` を `wasm-ld --no-entry --stack-first -z stack-size=1048576 --max-memory=67108864
   --export=tz_bytes_length` で link し、2 と同じ 3 呼び出しの結果。
7. heap の境界（`wasmReallocationChecks` の形）: `heap-wasm.ll` の 3 行を JS で上限 L へ置換し、L = 67108864 と 2147483648 で `--export=__heap_base
   --max-memory=L` で link する。B = `(__heap_base + 15) & -16` として、`allocate(L − B − 16)` が `B + 16` を返し `byteLength === L`、続く
   `allocate(1)` がトラップ。新しい instance で `allocate(L − 31)` と `allocate(L − 32)` がトラップ（前者は `%fits`、後者は `%within`）。
8. 拒否と test: 「例」の拒否の 9 件が終了コード非 0 で stderr に `error[E2000]` と表の文。`tsuzuri test <root> --target wasm32 -O0` は
   `1 passed; 1 failed`、`--wasm-max-memory 64MiB` を足すと `2 passed; 0 failed`。

`tests/wasm_threads.mjs` への追加: 既定の pool で `pool.memory.buffer.byteLength === 16 * 1024 * 1024`（D5 の不変）。tasks fixture を
`--wasm-max-memory 64MiB` の threads で build し、import が flags 3・`max 1024`、`createThreadPool(bytes, { workers: 2 })` の `byteLength` が
67108864、`tz_parallel_sum` の 0・1・64・1024 が `count * (count - 1) * (2 * count - 1) / 6`、`tsuzuri_thread_heap_live_bytes` が
`2 * (262144 + 16)`。非 threads の `.wasm` を渡すと `TypeError`。

### 既存テストへの影響

- 期待値の変更はない。16 MiB の表明はすべて既定の build なので成り立つ。
- compile のための変更: `src/main.rs` と `tests/test_runner.rs` の完全な literal に新しい field を足す（期待値ではない）。

### 性能

既定の出力は byte 単位で同じなので計測しない。指定時の差は比較の定数だけ。threads の大きな上限は宣言の max を最初に予約する（D5）。

## ドキュメント

- `docs/language.md`: 再帰とスタックの段落（「WASM は stack-first 配置で 1 MiB のスタック、線形メモリ上限 16 MiB を設定します」）を既定値と
  2 オプション・最大 2 GiB に書き換える。確保失敗の段落と文字列の段落の「16 MiB」を「上限（既定 16 MiB）」にする。
- `docs/architecture.md`: 16 MiB を述べる各段落を「上限（既定 16 MiB）」にし、WASM の link の段落に `with_wasm_heap_limit` の行置換と
  wasm-ld の 2 引数を一か所から作ることを書く。
- `_docs/guides/webassembly.md`: 上限の段落に 2 オプション、object を自分で link するときは同じ `--max-memory` を渡すこと。
- `_docs/guides/wasm-threads.md`: 「全 256 page を最初に確保します」を「module が宣言した最大 page 数を最初に確保します」にし、
  `WebAssembly.Module` を渡すときは `memory` を明示することを書く。
- `_docs/tools/command-line.md`: `## build と run の主なオプション` の表に 2 行、その下の制約の段落に使える action と出力。
- `README.md`: E2E のコマンド一覧に `node tests/wasm_memory.mjs target/release/tsuzuri`。
- `_docs/feature-status.md` と `_features/README.md` の F11 の状態。

## 受け入れ条件

- [ ] `--wasm-max-memory` と `--wasm-stack-size` を build（wasm32）と `test --target wasm32` で指定でき、heap が指定の上限まで使える。
- [ ] オプションを省略した build と既定値を明示した build の IR・`.wasm` が HEAD と byte 単位で一致する（手順 5）。
- [ ] 「診断」の表の `E2000` がすべて Rust テストか E2E で確かめられている。
- [ ] 上限 64 MiB・2 GiB で境界の確保が成功し、1 byte 超えでトラップする（E2E 2・7）。WASM の import は空のまま。
- [ ] threads の import の max と Node ホストの memory が指定の上限と一致し、既定では 16 MiB のまま。
- [ ] `heap-wasm.ll`・`heap-wasm-threads.ll`・`task-wasm-threads.c`・`src/cache.rs` は変更されていない。
- [ ] 「ドキュメント」の更新が終わり `node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 上限の不一致: heap の定数と wasm-ld の `--max-memory` を別々に決めると食い違う。`wasm_memory_limits` の戻り値だけを両方に使う。`build_runner`
  にも別の link があるので、`test --target wasm32` を忘れると 16 MiB のままになる（E2E 8 で検出）。
- 部分文字列の置換: `ir.replace("16777216", ...)` は `math.ll` の定数（`16777216` を含む行がある）や利用者の文字列定数を壊す。行の完全一致だけを置換する。
- CRLF: Windows の checkout で `.ll` が CRLF になりうる。行末を除いて比較し、元の行末を保つ（`wasm_heap_limit_default_is_identity`）。
- `llvm-readobj` 21 の `--sections` は wasm の `MaxPages` に誤った値（HEAD の既定 build で 280）を出す。page 数は byte の parser で読む。
- object 出力: 利用者が link で小さい `--max-memory` を渡すと `memory.grow` が先に失敗し、大きい値を渡すと heap が compile 時の上限で止まる。
  どちらもトラップで安全だが、文書に同じ値を渡すよう書く。
- JS の `i32`: 2^31 以上の pointer は JS で負になる。Phase 1 の上限 2 GiB はこれを避けるためで、上限を上げたくなったら停止条件に当たる。
- 2 GiB の検査: memory の中身に書き込むと実メモリを使う。E2E 7 は確保だけで payload に触れない。fixture の 2 GiB の case で GiB の配列を作らない。
- `--wasm-max-memory=64MiB` は `-` で始まる未知のオプションとして `unknown option` の `E2000` になる。`=` 形式は受けない（D1）。
- cache: `BuildOptions` の Debug が変わるので、更新後の最初の build は cache miss になる。`None` と既定値の明示は key が異なるが出力は同じで、正しさに影響しない。
- E03: 各 E2E の project は自分の一時 root に置く。fixture を直接 build すると親ディレクトリの `.tz` まで読む。
- `WebAssembly.Module` を `createThreadPool` に渡し `memory` を省略すると 256 page のまま。宣言の max が 256 より大きい module は 16 MiB で
  頭打ちになる（黙って成功する）。文書に書き、テストは bytes を渡す。

## 対象外

- 複数 memory、memory の縮小、GC 型との連携、worker の stack の大きさの指定。
- wasm64 の threads（shared な memory64）、wasm64 で 16 GiB を超える上限（Phase 2 の後も対象外。D12）。
- ブラウザー用のホスト（同梱のホストは Node だけ）。COOP・COEP の設定。

## 決定事項

### D1: CLI の名前と値の書式

- 決定: `--wasm-max-memory <size>` と `--wasm-stack-size <size>`。値は次の引数で、byte 数か `KiB`・`MiB`・`GiB` の接尾辞（大文字小文字を区別）。
  各一回。使えるのは build（wasm32）と `test --target wasm32` だけ（「CLI」の表）。
- 理由: 既存の `--wasm-feature` と同じ `--wasm-` 接頭辞で WASM 専用と分かる。`=` 形式は既存のどのオプションも受けない。run は native だけ。
- 状態: 既定案（実装者はこの案に従う）

### D2: 値の範囲と Phase 1 の上限 2 GiB

- 決定: 上限は 64 KiB の倍数で 2 GiB 以下、stack + 64 KiB 以上。stack は 16 の倍数で 64 KiB 以上。検査は `wasm_memory_limits` 一か所で、
  `BuildOptions::validate` と `run_with_timeout` から呼ぶ。
- 理由: 2 GiB 以下なら `heap-wasm.ll` の `i32` の計算が命令を変えずに折り返さず（「生成 IR とランタイム」の証明）、既定の出力を byte 単位で保てる。
  pointer が 2^31 未満なので JS の glue も変えずに済み、stack の溢れも必ず範囲外アクセスになる。stack の下限は `1024` のような単位の書き忘れを検出する。
- 状態: 既定案（実装者はこの案に従う）

### D3: heap への上限の渡し方

- 決定: `heap-wasm.ll` は変えない。`llvm::with_wasm_heap_limit` が生成後の IR の 3 行（完全一致）を置換する。build は `build_complete`、test は
  `build_runner` から呼ぶ。上限が既定なら何もしない。
- 理由: 定数 global（旧版の `@tz.heap.limit` 案）は `-O0` で load が増え既定の出力が変わる。wasm には最大 page 数を読む命令がなく、wasm-ld の
  `__heap_end` は初期メモリの終端で上限ではない。`emit_program` まで値を通すと 5 つの公開 emit 関数と `EmitOptions` の構築箇所すべてが変わる。
- 状態: 既定案（実装者はこの案に従う）

### D4: stack の指定

- 決定: `--wasm-stack-size` は main stack（wasm-ld の `-z stack-size`）だけを決める。worker の stack は 256 KiB 固定のまま。
- 理由: worker の stack は `heap-wasm-threads.ll` と `task-wasm-threads.c` の 2 か所の定数と Node ホストの計算にまたがり、別の設計が要る。
- 状態: 既定案（実装者はこの案に従う）

### D5: threads の shared memory と Node ホスト

- 決定: wasm-ld へ同じ `--max-memory` を渡し、import の max を上限にする。`createThreadPool` は `memory` の省略時に module の宣言の max を読み、
  `initial` と `maximum` の両方に使う。旧版の「最大値ぶんを最初に確保しない」は採らない。
- 理由: 既定では HEAD と同じ 256 page になり、ホストから見える挙動も変わらない。threads では memory が伸びないという HEAD の性質を保つので、
  import module が保持する view が短くならない。確保に失敗したら上限を下げるよう `RangeError` で知らせる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 既定値

- 決定: 既定の上限 16 MiB と stack 1 MiB は変えない。
- 理由: 旧版の未決事項の既定案どおり。既定を大きくするかは利用実績とブラウザーの制約を見て別に判断する。
- 状態: 既定案（実装者はこの案に従う）

### D7: cache key

- 決定: `src/cache.rs` は変えない。新しい field は `format!("{options:?}")` で key に入り、heap の定数は置換後の IR で key に入る。`FORMAT` は上げない。
- 理由: 既存の仕組みで網羅でき、link だけに効く stack の値も `options` に含まれる。test runner は cache を使わない。
- 状態: 既定案（実装者はこの案に従う）

### D8: 静的 data が上限に収まらない場合

- 決定: 事前に検査せず、wasm-ld の失敗を既存の `E2002` で報告する。どちらかのオプションを指定したときだけ hint に
  `; if the memory limit is too small, raise --wasm-max-memory or lower --wasm-stack-size` を足す。
- 理由: data の大きさは link まで分からない。既定の build の hint（既存のメッセージ）は変えない。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2（2 GiB 超と memory64）

- 決定: 2 GiB 超から 4 GiB − 64 KiB までの上限と memory64（`--target wasm64`）は Phase 2 とし、「Phase 2（設計方針）」の監査の後に着手する。
- 理由: heap の命令、JS の glue、stack の溢れの保証、E05・E13 の ABI を変える。memory64 は ABI の版を分ける。
- 状態: 承認済み（2026-10-01）。上限は wasm32 が 4 GiB − 64 KiB、wasm64 が 16 GiB（D12）。

### D10: manifest の `[wasm]`

- 決定: 旧版の `[wasm] max-memory`・`stack-size` は名前を保ったまま Phase 2 へ移す。Phase 1 は CLI だけ。
  Phase 2 の形: `[package]` の後に `[dependencies]` と `[wasm]` をそれぞれ一度まで、順序は問わない。値は CLI と同じ書式の引用符付き文字列（`"64MiB"`）。
  書式・未知の key・重複は manifest の `E0002`。root package の値だけを `Project::wasm` に持ち、依存 package の `[wasm]` は読まない。
  `main` が project の読み込み後に `BuildOptions::with_manifest_wasm` で CLI が未指定のものだけを補う（上限は WASM の object・llvm・wasmと test、
  stack は WASM の wasm 出力と test。native と header では無視）。補った後に `validate` し、範囲外は `Tsuzuri.toml` を示す `E2000` に
  `(after applying the root package's [wasm])` を足す。
- 理由: `src/package.rs` の `parse_manifest` は `[package]` と `[dependencies]` の順序と一回だけの出現を検査しており、節の追加は manifest の
  形式の変更になる。依存 package の値を読むと、library の都合で application の上限が黙って変わる。CLI が優先するのは一時的な上書きのため。
  範囲の検査は target に依存するので manifest の解析では書式だけを見て、範囲は CLI と同じ `wasm_memory_limits` に任せる。
- 状態: 既定案（Phase 2 で実装）

### D11: stack の溢れの検出

- 決定: wasm32 で上限が 2 GiB を超える build と threads の build（test runner も同じ条件）では、`llvm::with_stack_checks` が生成後の IR の
  全関数の entry block に `call void @tz.stack.check()` を入れる。entry block の static alloca は呼び出しの前へ集める。
  検査は `llvm.frameaddress(0)`（prologue 後の stack pointer）を `[base + 4096, top]` と符号なしの 1 比較で照合し、外れたら `llvm.trap`。
  範囲は単一 thread なら linker の `__stack_low`・`__stack_high`、threads は instance ごとの wasm global `tsuzuri_stack_base`・
  `tsuzuri_stack_top`（両方 0 は main の stack。`!invariant.load`）。同梱ホストは worker ごとに設定し、driver は threads の `--emit wasm` で export する。
  wasm32 で 2 GiB 以下の単一 thread と wasm64 には入れない（既定の出力は byte 単位で不変）。
- 理由: stack は尽きると 0 の下へ折り返る。2 GiB 以下の wasm32 と wasm64 ではその先は常に範囲外だが、4 GiB − 64 KiB の上限では 64 KiB を超える
  frame（frame 配列は 1 つ 64 KiB までで、複数持てる）や、使わない frame を積む再帰で heap の末尾へ届く。worker の stack は heap の block なので、
  溢れは隣の block を黙って壊す（Phase 2 の調査で `deep 64` が 259,419 bytes を上書きした HEAD からの不具合）。
  検査を prologue の後に置くので、その関数自身の frame（64 KiB 超も）を使う前に捕まえ、probe や frame の拒否を要しない。
  余白 4096 は検査しない `task-wasm-threads.c` と `wasm.ll` の frame の合計（O0 で 432 + 208 = 640 bytes。`tests/wasm_threads.mjs` が 4096 以下を表明）を含む。
  `llvm.stacksave` ではなく `llvm.frameaddress` と invariant な load を使うのは、inline 展開後に LLVM が重複した検査をまとめ、
  ループの外へ出せるため（stacksave 版は配列の初期化ループが threads で 2.4 倍遅かった）。alloca を集めないと inline 展開が entry block を分け、
  後ろの alloca が動的 alloca になってループごとに stack を消費した。範囲を設定しないホストの worker は main の範囲で検査され、最初の呼び出しで
  トラップする（黙って無検査にしない。そのための select 1 組は fib で約 5% の費用）。
- 状態: 実装者への一任に基づく決定（Phase 2 で実装）

### D12: wasm64 の範囲

- 決定: `--target wasm64` は build（wasm・object・llvm）と test。上限は 16 GiB（wasm-ld の上限）、threads は wasm32 だけ。
  heap は `heap-wasm64.ll`（`heap-wasm.ll` の i64 版。header は +0 の i64 capacity と +8 の i64 next）。`Instrumentation::memory64` が host ABI の
  `memory.size.i64`、scalar capture の幅（64）、DWARF の pointer 幅を選ぶ。pointer は JS で BigInt。test は PATH の Node.js が memory64 を
  検証できなければ `E2002`（Node.js 24 以降）。
- 理由: memory64 では stack の折り返り先が常に範囲外で、heap の計算も i64 で折り返らない。shared な memory64 と thread runtime の 64-bit 化は
  別の設計が要る。Node.js 20 は memory64 を持たず、24 で BigInt の pointer と `grow` を確認した。
- 状態: 実装者への一任に基づく決定（Phase 2 で実装）

## 実装と検証（2026-10-01、Phase 1）

Phase 1（手順 1–10）を実装した。D9（2 GiB を超える上限と memory64）と D10（manifest の `[wasm]`）は Phase 2 のため実装していない。
着手時の HEAD は `5d9c4a7`。

### 実装

- `src/driver.rs`: 定数 `DEFAULT_WASM_MAX_MEMORY`・`DEFAULT_WASM_STACK_SIZE`、`BuildOptions` の `wasm_max_memory`・`wasm_stack_size`
  （`Option<u64>`）、`validate` の最後の 2 検査、`wasm_memory_limits`。`build_complete` は wasm32 の分岐の先頭で `llvm::with_wasm_heap_limit` を呼び、
  wasm-ld の `-z stack-size=`・`--max-memory=` を実効値から作る。
- `src/llvm.rs`: `with_wasm_heap_limit`。3 行を行全体の一致で置換し、元の行末を保つ。上限が既定なら入力をそのまま返す。
- `src/main.rs`: 2 オプションの解析（`parse_size`）、重複・action・test の target の検査、`HELP`、`run_test_action`。
- `src/test_runner.rs`: `TestOptions` の 2 field、`run_with_timeout` の検査、`build_runner` の IR の置換と link 引数。
- `src/runtime/wasm-threads.mjs`: `memoryLimits`（新規 export）。`createThreadPool` は `memory` の省略時に env.memory の宣言の max から作る（D5）。
- テスト: `src/llvm.rs` 3 件、`src/driver.rs` 2 件、`src/main.rs` の `parses_wasm_memory_sizes`（新規）と `rejects_ambiguous_or_unused_arguments`（拡張）、
  `tests/test_runner.rs` の `wasm_memory_options_reach_the_wasm_test_link`、`tests/wasm_memory.mjs` と `tests/fixtures/wasm_memory/Main.tz`（新規）、
  `tests/wasm_threads.mjs`。
- 文書: 「ドキュメント」の各ファイルに加え、`_docs/tools/testing.md`、`examples/web/README.md`、`_perfs/README.md`（F11 へのリンク）を更新した。

### 決定事項への追記（チケットから外れた判断）

- D8 の hint は `tsuzuri test --target wasm32` の link（`build_runner`）にも同じ条件で足す（共通の `wasm_link_hint`）。
- `driver::run_tests` も、native の target とメモリのオプションの組を CLI と同じ文の `E2000` で拒否する。API から黙って無視しないため。
- `parse_size` の例示値（`67108864 or 64MiB`／`4194304 or 4MiB`）は option 名で選ぶ。署名はチケットどおり。
- link の失敗の `E2002` は、既存の driver の診断と同じく `Span::default()` で、表示は project の入力ファイルの `1:1` になる（「診断」の表の
  `<command line>` は `parse_arguments` と `validate` の `E2000` だけに当たる）。
- fixture の lambda は GUIDE §12 に合わせて `\index -> ...` にした（「例」は互換の形 `index -> ...`）。`HELP` の 2 行は桁をそろえた。
- E2E は fixture を一時 root へ一度だけ複写し、出力はすべて root の外に置く（root を汚さないので case ごとの複写は不要）。
  加えて、64 MiB の `.wasm` と IR を同じ入力で 2 回 build して一致すること、D8 の hint（70,000 単位の文字列定数を `-O0`、
  `--wasm-max-memory 128KiB --wasm-stack-size 64KiB` で build すると `E2002`。`-O3` は Clang が長さを畳み込み data が消えるので成功する）を確かめる。
- `memoryLimits` は wasm の magic を確かめ、import の走査を section の終端で止める。戻り値は `{ flags, minimum, maximum }`（page 数）。
  `tests/wasm_threads.mjs` はこれを使わず、独立した parser（`importedMemory`）で照合する。

### 確認（Apple M1 Max、macOS、Apple clang 21.0.0、Homebrew LLD 23.1.1、rustc 1.98.1、Node v20.17.0）

- 着手条件: `heap-wasm.ll` の `16777184` が 2 回、`16777216` が 1 回、`heap-wasm-threads.ll` は 0 回。HEAD の `cargo test --locked` は 506 passed。
  「再現」は HEAD で `memory flags 1 min 17 max 256`・`import env memory flags 3 min 17 max 256` を再現した。
- 手順 1・5: tasks fixture の plain／threads の `.wasm` と `--emit llvm`（それぞれ `-O0`・`-O3`）の 6 ファイルが、HEAD の出力と、省略・既定値の明示
  （`--wasm-max-memory 16MiB --wasm-stack-size 1MiB`、threads は `16777216`・`1024KiB`、IR は上限だけ）のどちらでも `cmp` で一致した。
  `--wasm-max-memory 64MiB` は plain が `memory flags 1 min 17 max 1024`、threads が `import env memory flags 3 min 17 max 1024`。
- Rust: `--lib wasm_heap_limit` 3 passed、`--lib wasm_memory` 2 passed、`--bin tsuzuri` の `parses_wasm_memory_sizes`・
  `rejects_ambiguous_or_unused_arguments` 各 1 passed、`--test test_runner wasm_memory_options_reach_the_wasm_test_link` 1 passed。
  `cargo test --locked` 全体は 513 passed・0 failed。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、§3.1 の回帰テスト 4 件、
  `sh scripts/check-runtime-includes.sh`（20 files、変更なし）が成功。
- `node tests/wasm_memory.mjs target/release/tsuzuri`: `-O0`・`-O3` で E2E 1〜8（既定の不変、64 MiB で 32・62 MiB 成功と 63 MiB のトラップ、
  stack 4 MiB の min が +48、2 GiB の max 32768、IR 3 種、object の手動 link、heap の境界 L = 64 MiB と 2 GiB、`tsuzuri test` が 1 failed から
  2 passed）、拒否 9 件の `E2000`、`E2002` の hint、決定性が成功。WASM の import は空。
- `node tests/wasm_threads.mjs target/release/tsuzuri`: `-O0`・`-O3` で既定の pool が 16 MiB、64 MiB の threads が import の max 1024・pool の
  67108864 bytes・`tz_parallel_sum` の 0／1／64／1024・live bytes `2 * (262144 + 16)`、非 threads の `.wasm` の `TypeError` が成功。
- 既存の suite（手順 9）: e2e（`-O0`・`-O3` の各 400 native/WASM 結果）、primitives、strings（3973 件）、control（270 件）、computations（70 件）、
  tasks（41 結果・4 trap）、host_imports、cache、features（4930 件。WASM realloc を含む）、display_parse（88063 表示・111282 解析）、
  math（Node 24.21.0、786051 参照）、examples、wasm_simd（`TSUZURI_OBJDUMP` は llvm@21）が成功。display_parse は作業機の `/usr/bin/python3`（3.9.6）に `sys.set_int_max_str_digits` がないため、
  `PATH` の先頭に Homebrew の Python 3.14 を置いて実行した（F11 と無関係の環境の問題）。
- 文書: `node scripts/check-docs.mjs` が対象ページ（5 pages）と全体（83 pages、740 links、136 examples、244 native runs、9 test projects）で成功。
  変更した Markdown の診断は増えていない（`_features/README.md` の MD060 は既存の表の区切り行）。
- 変更していないことの確認: `src/cache.rs`、`src/runtime/heap-wasm.ll`、`src/runtime/heap-wasm-threads.ll`、`src/runtime/task-wasm-threads.c`。
- 性能: 既定の出力が byte 単位で同じなので計測していない。性能は主張しない。

### 残作業（Phase 1 の時点）

- D9（2 GiB を超える上限・memory64）と D10（manifest の `[wasm]`）の Phase 2。D9 は承認が必要。（→ 下の Phase 2 で実装した。）
- ブラウザー用のホストは対象外のまま。コミットは作っていない。

## Phase 2 の実装と検証（2026-10-01）

D9 の承認（Phase 2 のすべてと、stack の溢れの扱いの一任）を受けて、2 GiB を超える wasm32 の上限、memory64（`--target wasm64`）、
manifest の `[wasm]`、stack の溢れの検出を実装した。基準は Phase 1 を適用した作業ツリー（HEAD `5d9c4a7`、未コミット）。

### 実装

- `src/driver.rs`: `Target::Wasm64` と `Target::is_wasm`。定数を `u64` にし、`MAX_WASM32_MEMORY`（4 GiB − 64 KiB）・`MAX_WASM64_MEMORY`（16 GiB）。
  `wasm_memory_limits(target, max, stack)` は target ごとの上限と文を持つ。`wasm_stack_checks`（D11 の条件）、`BuildOptions::with_manifest_wasm`、
  `Project::wasm`（root package の `[wasm]`）。`build_complete` は wasm64 と threads を `llvm::emit_wasm_build` で生成し、heap の置換の後に
  `llvm::with_stack_checks` を入れ、clang に `--target=wasm64-unknown-unknown`、wasm-ld に `-mwasm64` を渡す。検査付きの threads の
  `--emit wasm` は `tsuzuri_stack_base`・`tsuzuri_stack_top` を export する。
- `src/llvm.rs`: `with_wasm_heap_limit(ir, u64)` は 2 GiB を超えると `%within` を `%room` の 2 行に置き換え、wasm64 の `i64 %end` の行も扱う。
  `with_stack_checks`（D11）、`emit_wasm_build`（旧 `emit_wasm_threads_build` に memory64 を足したもの）、`emit_test_runner_for`、
  `Instrumentation::memory64`・`Globals::memory64`（heap の選択、`immediate_capture` の幅、DWARF の pointer 幅）。
- `src/llvm_abi.rs`: memory64 では host ABI の範囲検査に `llvm.wasm.memory.size.i64` を使う。
- `src/runtime/heap-wasm64.ll`（新規、`.gitignore` の例外を追加。runtime includes は 21 files）: `heap-wasm.ll` の i64 版。上限の 3 行は同じ形。
- `src/package.rs`: `WasmSettings`、`parse_size`（CLI と共有）、`[wasm]` の解析（D10）。
- `src/main.rs`: `--target wasm64`、test の target の検査（wasm32 か wasm64）、wasm64 の既定 emit、manifest の適用と再検証、`HELP`。
- `src/test_runner.rs`: wasm64 の生成と link、memory64 を検証できない Node の `E2002`、wasm32 の 2 GiB 超での stack 検査。
- `src/runtime/wasm-threads.mjs`: worker へ `base` も渡し、`tsuzuri_stack_base`・`tsuzuri_stack_top` を設定してから `tsuzuri_thread_entry` を呼ぶ。
  `tsuzuri_thread_stack_alloc` と `tsuzuri_threads_control` の pointer は `>>> 0`。
- `examples/web/simulation.mjs`: `tsuzuri_alloc` の pointer を `>>> 0`（「Phase 2（設計方針）」の glue の監査）。
- テスト: `src/llvm.rs`（heap の行の一意性に wasm64、`wasm_heap_limit_above_2_gib_compares_the_remaining_room`、
  `stack_checks_follow_static_allocas_in_every_definition`）、`src/driver.rs`（limits を target 別に、`stack_checks_cover_threads_and_wasm32_memory_above_2_gib`、
  `manifest_wasm_fills_only_unset_options_for_applicable_outputs`、validate の wasm64）、`src/package.rs`（`parses_wasm_sizes_in_any_section_order`、拒否 7 件）、
  `src/main.rs`（wasm64 の解析と拒否）、`tests/test_runner.rs`（4 GiB − 64 KiB と wasm64。Node が memory64 を持たなければ `E2002` を表明）、
  `tests/wasm_memory.mjs`、`tests/wasm_threads.mjs`、`tests/wasm64.mjs`（新規、Node 24）、`tests/features.mjs`（`TSUZURI_TEST_WASM_TARGET`）。
- 文書: README、`docs/language.md`、`docs/architecture.md`、`_docs/guides/webassembly.md`・`wasm-threads.md`、`_docs/tools/command-line.md`・`testing.md`、
  `_docs/language-reference/modules-and-packages.md`、`_docs/feature-status.md`、`_features/README.md`、GUIDE の §2.1 と D-30、`examples/web/README.md`。

### 決定事項への追記（チケットから外れた判断）

- 「Phase 2（設計方針）」の「64 KiB を超える frame の扱い（probe か拒否）」は、prologue の後の入口検査（D11）で不要になった。
- HEAD からの不具合として、threads の worker の stack の溢れが隣の heap block を黙って上書きしていた（調査の probe で `deep 64` が 259,419 bytes）。
  Phase 2 の stack 検査で直し、`tests/wasm_threads.mjs` で表明した。
- 範囲を設定しない独自ホストの worker は最初の呼び出しでトラップする（D11）。Phase 1 までの手順で worker を自前で起動するホストは、
  2 つの global の設定を足す必要がある。同梱ホストと文書の手順は更新した。
- wasm64 の `--emit llvm` の IR には target triple がない（既存の IR と同じ）。自分でコンパイルするときは `--target=wasm64` を渡す。
- manifest の書式の誤り（`E0002`）は既存の manifest の診断と同じく `Tsuzuri.toml:1:1` に出る。範囲外の値は `Tsuzuri.toml` を示す `E2000` に
  `(after applying the root package's [wasm])` が付く。
- VS Code 拡張（`vsc/src/core.ts`）は WASM の target として wasm32 だけを渡す。wasm64 の選択は拡張に足していない。

### 確認（Apple M1 Max、macOS、Apple clang 21.0.0、Homebrew LLD 23.1.1、rustc 1.98.1、Node v20.17.0 と v24.21.0）

- 既定の出力: tasks fixture の plain の `.wasm` と `--emit llvm`（`-O0`・`-O3`）の 4 ファイルが、F11 着手前のコンパイラの出力と `cmp` で一致した。
  threads の `.wasm` は D11 の検査が入るので変わる（意図どおり）。threads と wasm64 の build の繰り返しは byte 単位で一致した（E2E で表明）。
- Rust: `cargo test --locked` は 518 passed・0 failed（Phase 1 後の 513 に新規 5 件。§3.1 の回帰テスト 4 件を含む）。
  `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`（21 files）が成功。
  `tests/test_runner.rs` の wasm64 は Node 20 の PATH では `E2002`、Node 24 を PATH の先頭に置くと既定で 1 failed、64 MiB で 2 passed。
- 調査の probe（再現と修正の確認）: 256 KiB の worker stack で `deep 64`（1 段 8,000 bytes）は HEAD で隣の block を 259,419 bytes 上書きし、
  Phase 2 では `RuntimeError` で 0 bytes。4 GiB − 64 KiB まで grow した wasm32 の main stack の溢れはトラップし、memory の末尾 1 MiB は不変。
  O3 の逆アセンブルで検査が prologue（`i32.const 8000; i32.sub; global.set`）の後にあることを確かめた。wasm64 の host ABI は 4.36 GB の
  アドレスの buffer で `tz_sum`・`tz_copy_values` が動き、RSS は 48 MiB だった。
- E2E（Node v20.17.0。wasm64・math・features の wasm64 は v24.21.0）: e2e（`-O0`・`-O3` の各 400 native/WASM 結果、15 trap）、primitives、
  strings、tasks（41 結果・4 trap）、wasm_threads（worker の溢れのトラップ、`-fstack-usage` の検査なし frame の合計 4096 以下を含む）、
  wasm_memory（4 GiB − 64 KiB、room 形式の境界 4 種、stack 検査、manifest、拒否 10 件）、wasm64、gpu（783 参照）、cache、computations（70 結果・3 trap）、
  control（270 件）、numeric_casts（1585 件）、integer_intrinsics（263182 件）、display_parse（88063 表示・111282 解析。Homebrew の Python）、
  examples、features（wasm32 で 4930 件）、features の wasm64（`TSUZURI_TEST_WASM_TARGET=wasm64` で 4930 件）、lsp_sessions、docgen、
  wasm_simd・simd（`TSUZURI_OBJDUMP` は llvm@21、232 件）、cpu_dispatch、host_imports、io、debug_info、math（786051 参照）がすべて成功。
- 文書: `node scripts/check-docs.mjs` が 83 pages、740 links、136 examples、244 native runs、9 test projects で成功。他の suite と同時に既定の
  cache root で走らせた回だけ `build cache write failed`（W2001）が 2 回出たが、専用の `TSUZURI_CACHE_DIR` で単独に走らせた回は 0 回だった
  （cache の既存の競合で、F11 とは無関係）。変更した Markdown の診断は増えていない（`_features/README.md` の MD060 は既存の表）。

### 性能

目的は安全性（stack の溢れの検出）で、速度の向上は主張しない。費用は同じコンパイラで検査の有無だけを変えた build（一時的な局所スイッチで検査を外し、
計測後に削除）を、O3・Node v20.17.0 で比べた。各値は 3 回の warm-up の後の 9 回の中央値で、これを 3 回繰り返した範囲。
script・source・生の出力は `target/perf/F11-stack-checks/`（`bench-matched.mjs`、`Main.tz`、`results.txt`）にある。

| workload | wasm32 の 2 GiB 超 | threads（main instance） |
| --- | --- | --- |
| `fib 32`（frame のない再帰。最悪の場合） | 1.12〜1.22 倍 | 1.25〜1.40 倍 |
| `tree 27`（8 要素の frame 配列の再帰） | 0.86〜0.97 倍 | 0.89〜0.99 倍 |
| `sum 4M`（closure による配列の初期化と合計） | 0.98〜1.05 倍 | 1.00〜1.04 倍 |
| `text 200k`（f64 の文字列化） | 0.93〜0.99 倍 | 0.92〜0.97 倍 |

1 未満の値は配置などの揺れで、速くなったとは主張しない。O3 の逆アセンブルでは、検査は prologue の後に 1 回だけ残り、inline 展開された
重複は 1 つにまとまってループの外へ出た。`llvm.stacksave` を使った版は `sum` が 1.30 倍（2 GiB 超）・2.40 倍（threads）、
alloca を集めない版はループごとに stack を消費したため、D11 の形にした。

### 残作業

- wasm64 の threads（shared memory64 と thread runtime の 64-bit 化）、16 GiB を超える wasm64 の上限、ブラウザー用のホスト、
  VS Code 拡張の wasm64 の選択は対象外のまま。コミットは作っていない。
