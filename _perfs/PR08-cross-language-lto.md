# PR08: 言語間 LTO と bitcode 出力

| 項目 | 内容 |
| --- | --- |
| ID | PR08 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | (PB01), (E12) |
| 関連 | E05, E06, E13, PR07 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 2 の WASM は人間が求めた場合だけ着手する） |
| 手本にする既存実装 | C ランタイムの IR 化と結合: `src/driver.rs` の `build_complete` の `merge_debug_ir` 経路（`task.c` などを `-S -emit-llvm` で IR にし、`tool("TSUZURI_LLVM_LINK", "llvm-link")` で `module.ll` と結合する）。emit の追加: `Emit::Wgsl`（`src/driver.rs` の `Emit`・`BuildOptions::validate`・`BuildOptions::output_path`、`src/main.rs` の `--emit` の match）。ホストとの往復テスト: `tests/host_abi.rs` の `host_buffers_and_records_roundtrip_on_native_and_wasm`（IR の `@malloc`／`@free` を `@tracked_alloc`／`@tracked_free` へ置換して `live == 0` を確かめる） |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/cache.rs`（変更しないことの確認だけ）, `tests/lto.rs`（新規。ソースはテスト内に書き、fixture は作らない）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_docs/guides/native-interop.md` |
| 計測対象 | ホストの C ループから小さな公開関数 `tz_add` を呼ぶ種目 `lto/small_call`（新規）。通常リンク（`--emit object`）・言語間 ThinLTO（`--emit bitcode`）・C だけの実装の 3 通りで、時間・ループ内の呼び出し命令の数・実行ファイルの大きさ |

## 目的

Tsuzuri の計算カーネルを C/C++/Rust のホストに組み込むとき、関数呼び出しの境界を越えてインライン展開・定数伝播ができるようにする。
細かい粒度でホストから呼ぶ場合の呼び出しコストをなくし、組み込み用途で C/C++ だけで書いた場合と同等の速度にする。

手段は新しい出力 `--emit bitcode`（新規）で、プログラムと必要なランタイムを一つの ThinLTO 用 LLVM bitcode として出す。
ホストは同じ LLVM の版の Clang（または linker plugin）で `-flto=thin` の LTO リンクを行う。
実装者は Phase 1（native）だけを実装する。Phase 2（wasm32 と `wasm-ld` の LTO）は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- 依存の PB01・E12 は括弧付き（任意）で、開始条件にしない。PR08 はランタイムの C を今と同じくビルドのたびに Clang で
  コンパイルする。PB01 が先に done になっていても、その object キャッシュは bitcode に使えない（object は LTO に参加しない）ので流用しない。
  状態の確認: `grep -n "PB01\|PR08" _perfs/README.md` と `grep -n "E12" _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR・object の記号・手作業の LTO 実験）を保存していること。
- 道具: `clang --version` の 1 行目が `Apple clang version 21.0.0 (clang-2100.3.34.2)`（2026-09-30 に確認）、
  `/opt/homebrew/opt/llvm@21/bin/` に `llvm-link`・`llvm-objdump`・`llvm-bcanalyzer` があること。Apple の Xcode には `llvm-link` がないので、
  ランタイム C を含む bitcode は `TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link` で作る（README の macOS debug object と同じ組）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 属性の付与（D3）で、1 行で終わらない `define`（行末が `{` でない）や、`#<n>` と `!dbg` 以外の後置要素を持つ `define` が見つかった。
- `--emit llvm`・`--emit object`・`--emit exe` の生成 IR が 1 byte でも変わった（bitcode 以外の経路は変えない。手順 1 の保存物と比べる）。
- `llvm-link --internalize` の後で `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` が internal になった、またはランタイム C の記号が外部に残った。
- 属性を付けても手順 7 の検査で `tz_add` がホストのループに inline されない（remark に `conflicting attributes` 以外の理由が出る場合を含む）。
- 既存テストの期待値（IR・診断メッセージ・`--emit` の一覧以外の CLI 出力）を変える必要がある。`emit kind must be ...` の一覧だけは変えてよい。
- `unsafe`、新しい crate、LLVM の C API、既定の WASM import が必要になった。
- Rust との組み合わせが失敗した。これは停止ではなく、失敗の出力を文書の「未検証・非対応」の表に記録して先へ進む。

## 現状と計測（HEAD `f8dc655`）

### 出力とシンボル

- `src/driver.rs` の `Emit` は `Executable`・`Object`・`Llvm`・`Header`・`Wasm`・`Wgsl` の 6 つ。`src/main.rs` の `--emit` は
  `exe`・`object`・`llvm`・`header`・`wasm`・`wgsl` を受け、それ以外は `error[E2000]: emit kind must be exe, object, llvm, header, wasm, or wgsl`（終了コード 2）。
  値の名前は小文字の 1 語で、拡張子は `BuildOptions::output_path` が決める（`o`・`ll`・`h`・`wasm`・`wgsl`）。
- `--emit llvm` の IR（`src/llvm.rs` の `emit_target`）は `source_filename = "tsuzuri"` だけで、`target triple`・`target datalayout` を持たない。
  関数の属性は `nounwind` などの直書きだけで、`"target-cpu"`・`"target-features"` を持たない。
- 公開 wrapper は `src/llvm_abi.rs` の `wrapper` が `.replacen("define internal ", "define ", 1)` で作る `define i64 @tz_add(...)`（外部・既定の可視性）。
  内部関数は `define internal`。拡張 ABI の `tsuzuri_alloc`・`tsuzuri_free` は `define weak`（同ファイルの定数文字列）。
- 埋め込みランタイムは `src/llvm.rs` が必要に応じて IR へ連結する `.ll`（`string.ll`・`utf8string.ll`・`heap-native.ll`・`closure.ll`・`numeric.ll`・
  `math.ll` など）。`numeric.ll`・`math.ll` は `x86_64-unknown-linux-gnu` 向けに `generate.py`・`generate_math.py` で生成し、属性グループに
  `"target-cpu"` を持たない（`numeric.ll` の `attributes #0` は `"tune-cpu"="generic"` を持つ）。
- C のランタイム `task.c`・`cpu.c`・`io.c` は IR に含まれない。`build_complete` は IR の `declare void @tsuzuri_task_parallel(`・
  `declare i64 @tsuzuri_cpu_sum_i64(`・`declare i32 @tsuzuri_io_` を検索し（`task_runtime`・`cpu_runtime`・`io_runtime`）、exe・object のときだけ Clang で
  コンパイルして結合する。記号は `TZ_TASK_API`・`TZ_CPU_API`・`TZ_IO_API`（`__attribute__((weak, visibility("hidden")))`）。object は
  `clang -r -nostdlib`（macOS は `-Wl,-keep_private_externs`）で一つにする。
- ビルドキャッシュ（`crate::cache::build_key`）は `Emit::Llvm | Emit::Header | Emit::Wgsl` 以外で Clang を key に含める。

### 再現（2026-09-30 に確認）

`/tmp/tz-work-PR08/lib/Main.tz` を次の 2 行とし、C のホストから 1 億回呼ぶ。

```tsuzuri
export def add :: i64 -> i64 -> i64
fn add x y = x + y
```

```c
#include <stdint.h>
#include <stdio.h>

int64_t tz_add(int64_t x, int64_t y);

int main(void) {
    int64_t total = 0;
    for (int64_t i = 0; i < 100000000; i++) total = tz_add(total, i);
    printf("%lld\n", (long long)total);
    return 0;
}
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
W=/tmp/tz-work-PR08
target/release/tsuzuri build $W/lib --emit llvm -O3 -o $W/lib.ll
clang -O3 -flto=thin -x ir -Wno-override-module -c $W/lib.ll -o $W/lib.o
clang -O3 -flto=thin -c $W/host.c -o $W/host.o
clang -O3 -flto=thin $W/host.o $W/lib.o -o $W/host-lto && $W/host-lto
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn $W/host-lto | grep -c tz_add
```

| 実験 | 結果 |
| --- | --- |
| 上のとおり（属性なし、ThinLTO） | `4999999950000000`。`bl <_tz_add>` がループに残る（`grep -c` が 2） |
| 同じ IR で full LTO（`-flto`）、リンクに `-Wl,-mllvm,-pass-remarks-missed=inline` | inline されない。remark: `'tz_add' not inlined into 'main' because it should never be inlined (cost=never): conflicting attributes` |
| IR の全 `define` にホストの C と同じ `"target-cpu"="apple-m1"`・`"target-features"="+aes,..."` を付けて full LTO | inline され、ループ全体が定数 `0x11c379e5348f80`（= 4999999950000000）に畳まれる。`grep -c` が 0 |
| 同じく属性付きで ThinLTO | inline される（`grep -c` が 0） |
| ホストが full LTO（`-flto`）、Tsuzuri 側が ThinLTO | inline されない（`grep -c` が 2）。ThinLTO の module と通常 LTO の module の間では import しない |
| リンク行に `.bc` の名前で渡す | Clang が `.bc` を入力として一度コンパイルし直す（`-###` で `"-cc1"` が 1 回）が、inline はされる |
| `-fuse-ld=lld`（Homebrew の lld） | `LLVM ERROR: Unsupported stack probing method` で異常終了。macOS では Apple の `ld` を使う |
| 生成 bitcode の producer | `llvm-bcanalyzer -dump` の IDENTIFICATION が `APPLE_1_2100.3.34.2_0`。ThinLTO 版は `GLOBALVAL_SUMMARY_BLOCK` を 1 つ持ち、full LTO 版は持たない |

原因: IR 入力の関数に `"target-cpu"` がないと、LTO の target machine の既定 CPU（Apple の `ld` では C の `apple-m1` と異なる）で扱われ、
AArch64 の inline 互換検査（callee の機能ビットが caller の部分集合）に失敗する。Clang は IR 入力の関数へ属性を補わない（上の bitcode を
`llvm-dis` すると `tz_add` の属性は `mustprogress nofree norecurse nosync nounwind willreturn memory(none)` だけ）。

### 既存の計測

- `docs/benchmarks.md` の「測定条件」「C++20 とのネイティブ比較」などは、ネイティブの公開関数を外部 ABI で呼び、LTO を使わない。
  LTO の有無による時間はまだ測っていない。
- [なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md) は、ホスト境界には引数変換・呼び出しなどのコストがあり、細かすぎる呼び出しは不利になり得ると説明している。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する。

- G1: `--emit bitcode` の出力と C のホストを ThinLTO でリンクすると、ホストのループから `tz_add` の呼び出し命令が消える。
- G2: 種目 `lto/small_call` の時間が、同じ関数を C で書いた場合と同等（差が広がりの範囲内）。
- G3: `--emit bitcode` 以外の出力（exe・object・llvm・header・wasm・wgsl）の生成 IR と結果を変えない。
- G4: 同じ入力・同じ Clang からの bitcode は byte 単位で一致する。

| 指標 | 単位・統計 | 対象 | 期待（計測で確かめる） |
| --- | --- | --- | --- |
| M1 呼び出し命令 | 個（固定値） | `lto/small_call` の実行ファイルの `_main` 内の `tz_add` への `bl`／`call` | object 1 → bitcode 0 |
| M2 時間 | ms（9 回の中央値、最小、最大） | `lto/small_call`、規模 `n = 1000000000` | bitcode と C だけの実装の差が広がりの範囲内。object はそれより遅い |
| M3 実行ファイルの大きさ | bytes（固定値） | M2 の 3 通りの実行ファイル | 記録する（未使用のランタイムが LTO で消えるので bitcode ≤ object を見込む） |

## 変えてはいけない意味

- LTO は呼び出しの境界を消すだけで、Tsuzuri 側の検査（借用バッファの長さ・null・alignment、UTF-8 の検証、`tsuzuri_alloc` の負の size の
  トラップ）を省略しない。検査は `tz_*` wrapper の本体にあり、inline 後も残る。
- 整数の折り返し（`add i64` に `nsw`/`nuw` を付けない）、浮動小数点の丸め・NaN・符号付きゼロ・fast-math なし、評価順序、トラップ
  （`llvm.trap`）を保つ。属性の付与（D3）は `"target-cpu"`・`"target-features"`・`"tune-cpu"` の 3 つだけを写す。`"unsafe-fp-math"`・
  `"no-nans-fp-math"`・`"no-trapping-math"`・`"frame-pointer"` などは写さない。
- 所有権の契約（結果バッファはホストが `tsuzuri_free` で一度だけ解放）と確保の釣り合い（`live == 0`）を変えない。
- 公開する記号は通常の object と同じく `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ。内部関数・ランタイムの名前は internal か hidden のまま。
  `tsuzuri_` 接頭辞の規則（docs/language.md「公開 ABI」）を保ち、新しい外部記号を足さない。
- 決定性: 同じ入力・同じ Clang・同じ `--cpu` なら bitcode は byte 単位で一致する（G4）。
- WASM の既定の import なし（D-18）は Phase 1 では触らない。

## 設計

### 出力の形

- `--emit bitcode`（新規）は `--target native` だけで使え、拡張子は `bc`。中身は ThinLTO の要約（`GLOBALVAL_SUMMARY_BLOCK`）を持つ一つの
  bitcode で、生成 IR（埋め込みの `.ll` ランタイムを含む）と、必要なときだけ C のランタイム（`task.c`・`cpu.c`・`io.c`）の IR を含む。
- IR は object と同じ `llvm::emit_native_build` で作る。このため `--trap-info`（`<output>.trap.json`）・`--debug-info`（`-g`）・`--cpu native` は
  object と同じ意味で使える。エントリーは `Entry::Library`（`main` を出さない）。
- `-O<n>` は bitcode を作る前の最適化（ThinLTO の pre-link）の段を決める。リンク時の最適化の段はホストのリンク行の `-O<n>` が決める。
- ビルドキャッシュは使わない（D7）。

### LLVM の版

- bitcode を書くのは設定された Clang（`TSUZURI_CLANG`、なければ `clang`）。版は bitcode の IDENTIFICATION に自動で入る
  （このマシンでは `APPLE_1_2100.3.34.2_0`）。確認は `clang --version` と `llvm-bcanalyzer -dump x.bc | grep -m1 "record string"`。
- 規則: LTO を行うリンカー（linker plugin）の LLVM は、すべての bitcode の producer と同じか新しくなければならない。コンパイラは版を検査しない（D6）。
  不一致はホストのリンクで LLVM の bitcode reader のエラーとして出る（文言は版ごとに違う。`Unknown attribute kind`・`Invalid record` や
  `Producer: '...' Reader: '...'` を含むのが典型。未検証）。
- 組み合わせの確認状況（2026-09-30）:

| ホスト側 | Tsuzuri 側 | リンク | 状態 |
| --- | --- | --- | --- |
| Apple clang 21 の C、`-flto=thin` | Apple clang 21、属性付き（手作業） | `clang -flto=thin`（Apple の `ld`） | 確認済み（inline される） |
| Apple clang 21 の C、`-flto`（full） | Apple clang 21、ThinLTO | 同上 | 結合はできるが inline されない（D2） |
| Apple clang 21 の C | Apple clang 21 | `-fuse-ld=lld`（Homebrew） | 異常終了（非対応） |
| rustc 1.98.1（LLVM 22.1.8）の `--emit=llvm-bc`、C のホスト | なし | Apple の `ld` | 1 例で結合・inline された（保証しない） |
| Rust の `-C linker-plugin-lto` | `--emit bitcode` | Apple の `ld` または `lld` | 未検証 |
| Linux の Clang の C | `--emit bitcode` | `clang -flto=thin -fuse-ld=lld` | 未検証（手順 9 のテストが Linux の CI で通れば確認済みに変える） |
| Windows | `--emit bitcode` | `lld-link` | 未検証（拒否はしない） |

### データ構造

```rust
pub enum Emit {
    Executable,
    Object,
    Llvm,
    Header,
    Wasm,
    Wgsl,
    Bitcode, // 新規
}

/// Returns `"target-cpu"=... "target-features"=... ["tune-cpu"=...]` that `clang` gives a C function with `flags`.
pub fn probe_target_attributes(clang: &OsStr, flags: &[&str], directory: &Path) -> Result<String, Diagnostic> // 新規
/// Gives every `define` in `ir` the target attributes, replacing those in the attribute groups the definitions use.
pub fn stamp_target_attributes(ir: &str, attributes: &str) -> String // 新規
```

2 関数は `src/driver.rs` に置く（新しい module は作らない）。`tests/lto.rs` から使うので `pub`。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `--emit` の match、使い方の `--emit KIND` の行 | `Some("bitcode") => Emit::Bitcode`。エラーは `emit kind must be exe, object, llvm, header, wasm, wgsl, or bitcode`、使い方は `exe, object, llvm, header, wasm, wgsl, or bitcode` |
| driver | `src/driver.rs` | `Emit` | `Bitcode` |
| driver | `src/driver.rs` | `BuildOptions::validate` | `emit == Emit::Bitcode && target != Target::Native` を E2000（診断の表）。`--cpu native` の検査は変えない（`Emit::Llvm`・`Emit::Header` だけを拒否するので bitcode は通る） |
| driver | `src/driver.rs` | `BuildOptions::output_path` | `Emit::Bitcode => "bc"`（網羅 match なので arm がないと compile error） |
| driver | `src/driver.rs` | `build_complete` の `matches!(options.emit, Emit::Executable \| Emit::Object)`（`llvm::emit_native_build` を選ぶ条件） | `Emit::Bitcode` を加える |
| driver | `src/driver.rs` | `build_complete` の `let cache = if options.cache && options.emit != Emit::Header` | `&& options.emit != Emit::Bitcode` を加える（D7） |
| driver | `src/driver.rs` | `build_complete` の object 経路（`module.ll` を書く `else` 節） | `module.ll` を書く前に bitcode なら probe と属性の付与。ランタイム C のコンパイルに `-S -emit-llvm` を足す条件を `merge_debug_ir \|\| bitcode` に広げ、`llvm-link` の結合も同じ条件にする。bitcode のときだけ `--internalize` を足し、hint を bitcode 用にする。最後の `clang` に bitcode のときだけ `-flto=thin` を足す（`-c` と出力先 `artifact` は既存の条件で決まる） |
| driver | `src/driver.rs` | `probe_target_attributes`・`stamp_target_attributes` | 新規（アルゴリズム） |
| driver | `src/driver.rs` | そのほかの `Emit::` の比較（`-lm`、exe へのランタイム object、relocatable link、`wasm-ld`、`run_with_diagnostics`、Windows COFF の拒否） | 変更なし（どれも `Executable`・`Object`・`Wasm` だけを見るので bitcode は正しく外れる） |
| cache | `src/cache.rs` | `build_key` | 変更なし（bitcode はキャッシュの前に外れる） |

### 生成 IR とランタイム

```text
text = llvm::emit_native_build(...).ir            // object と同じ IR（Windows は llvm::windows_abi も同じ）
attributes = probe_target_attributes(clang, [native_compile_args..., native_cpu_flag?], temporary)
text = stamp_target_attributes(text, attributes)
write temporary/module.ll
if native_runtime:
    clang -std=c11 -c <native_compile_args> -O<n> [-pthread] [-g] -S -emit-llvm [-mcpu=native|-march=native] task.c -o task.o
    llvm-link module.ll task.o --internalize -S -o combined.ll      // TSUZURI_LLVM_LINK
clang -x ir -Wno-override-module -O<n> [-g] <native_compile_args> [cpu] -c -flto=thin <ir> -o artifact
```

| 記号 | bitcode での linkage | 備考 |
| --- | --- | --- |
| `tz_*` | 外部・既定の可視性 | ホストが呼ぶ。実行ファイルへの LTO では inline 後に消えてよい |
| `tsuzuri_alloc`, `tsuzuri_free` | `weak` | 拡張 ABI を使うときだけ（現状のまま） |
| `tz.*`、埋め込み `.ll` の関数 | `internal` | 現状のまま |
| `tz_soft_op`（`numeric.ll`） | `weak hidden` | 現状のまま |
| `tsuzuri_task_*`, `tsuzuri_cpu_*`, `tsuzuri_io_*` | `internal`（`--internalize`） | object では hidden。bitcode ではリンク後に外へ出ない |
| `malloc`, `free`, `write`, pthread など | `declare` | ホストのリンクで解決する。task runtime を含むときホストは `-pthread` を付ける |

`llvm-link --internalize` は最初のファイル（`module.ll`）以外から結合した記号だけを internal にする（`module.ll` の `tz_*` は外部のまま）。
`module.ll` は `target triple` を持たないので、結合で C 側の triple と datalayout が入る（既存の `merge_debug_ir` 経路と同じ。警告は無視してよい）。

### アルゴリズム

`probe_target_attributes`:

1. `directory/probe.c` に `void tsuzuri_probe(void) {}` を書き、`clang -std=c11 -S -emit-llvm <flags> probe.c -o probe.ll` を `run_tool` で実行する
   （hint は `bitcode output requires Clang that emits LLVM IR; set TSUZURI_CLANG`）。
2. `define` で始まり `@tsuzuri_probe(` を含む行から `#<n>` を取り、`attributes #<n> = {` の行を探す。
3. その行から `"target-cpu"="…"`・`"target-features"="…"`・`"tune-cpu"="…"` をこの順で取り出し、空白 1 つで連結して返す。
   `"target-cpu"` がなければ E2002 `cannot read target attributes from Clang output; set TSUZURI_CLANG to a Clang that emits LLVM IR`。

`stamp_target_attributes`（行単位。正規表現の crate は使わない）:

1. `attributes #<n> = {` の行の最大の `n` を求め、`k = n + 1`（なければ 0）とする。
2. `define ` で始まる各行: 行末は `{` でなければならない（違えば停止条件）。` #<数字>` を含むなら、その番号を集合に入れる。含まないなら
   ` !dbg ` の直前、なければ行末の ` {` の直前に ` #k` を挿入する。
3. 集合に入った番号の `attributes #<n> = { ... }` の行から 3 つの鍵（`"key"="` から次の `"` まで）とその前の空白を取り除き、`}` の直前に
   `attributes` を足す。
4. 末尾に `attributes #k = { <attributes> }` を 1 行足す（ステップ 2 で挿入した行がなければ足さない）。

`declare` の行と呼び出し位置の属性グループは変えない。入力が同じなら出力も同じ（決定的）。

### Phase 分割

- Phase 1（このチケット）: native の `--emit bitcode`、C ホストとの ThinLTO の手順・テスト・計測、Rust の手順（未検証と明記）。
- Phase 2（設計方針。人間が求めた場合だけ）: `--target wasm32 --emit bitcode`。IR に `wasm.ll` を含め、probe は
  `--target=wasm32-unknown-unknown -mbulk-memory` と `-msimd128`／`-mno-simd128` で行う。ホストは
  `clang --target=wasm32-unknown-unknown -flto=thin -nostdlib -Wl,--no-entry -Wl,--export=tz_<name>` で `wasm-ld` の LTO を行い、
  `--emit wasm` と同じ export（`memory`、拡張 ABI の `tsuzuri_alloc`・`tsuzuri_free`）を出す。import が空であることを node で確かめる。
  `--wasm-feature threads`（`task-wasm-threads.c`）は Phase 2 でも対象外。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `--emit bitcode` と `--target wasm32` | `'--emit bitcode' requires '--target native'; wasm32 bitcode is not supported yet` | `<command line>` |
| E2000 | 不明な emit の種類 | `emit kind must be exe, object, llvm, header, wasm, wgsl, or bitcode`（既存の文の一覧を更新） | `<command line>` |
| E2002 | probe の出力に `"target-cpu"` がない | `cannot read target attributes from Clang output; set TSUZURI_CLANG to a Clang that emits LLVM IR` | `<command line>` |
| E2002 | `llvm-link` を実行できない・失敗した | 既存の `run_tool` の形（`cannot execute '<name>': <error>; <hint>` または `'<name>' failed (<status>):` と出力と hint）。hint は `bitcode with the task, CPU, or IO runtime requires llvm-link matching Clang; set TSUZURI_LLVM_LINK` | `<command line>` |
| E2002 | probe・bitcode 化の Clang の失敗 | 同じ `run_tool` の形。hint は probe が上の文、bitcode 化は既存の `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` | `<command line>` |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。作業用の場所は `/tmp/tz-pr08/`。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。`/tmp/tz-pr08/fix/Main.tz` を次の内容で作り（2026-09-30 に `--emit llvm`・`--emit object` で確認済み）、
  他の出力の hash を保存する。「再現」の手作業の実験も一度行い、`grep -c tz_add` が 2 であることを見る。
- 確認: 次がすべて成功し、`before.sha` が 4 行になる。`cargo test --locked --test host_abi` の `running N tests` の N を記録する。

```tsuzuri
export def add :: i64 -> i64 -> i64
fn add x y = x + y
export def sum :: ref [i64] -> i64
fn sum values = Array.sum values
export def bytes :: i64 -> [ubyte]
fn bytes count = new [ubyte](count, index -> index as ubyte)
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
T=target/release/tsuzuri; W=/tmp/tz-pr08
$T build $W/fix --emit llvm -O3 -o $W/fix.ll
$T build $W/fix --emit header -o $W/fix.h
for o in 0 3; do $T build $W/fix --emit object -O$o -o $W/fix-O$o.o; done
shasum -a 256 $W/fix.ll $W/fix.h $W/fix-O0.o $W/fix-O3.o > $W/before.sha && wc -l $W/before.sha
nm -g $W/fix-O3.o | grep -v ' U '
cargo test --locked --test host_abi
```

`nm -g` は `_tsuzuri_alloc`・`_tsuzuri_cpu_features`・
`_tsuzuri_cpu_sum_i64`・`_tsuzuri_cpu_variant`・`_tsuzuri_free`・`_tz_add`・`_tz_bytes`・`_tz_sum` を出す（2026-09-30 に確認）。

### 手順 2: 属性の付与（純粋関数）

- 変更: `src/driver.rs` の `stamp_target_attributes`（新規）、`tests/lto.rs`（新規）。
- 内容: 「アルゴリズム」のとおりに実装する。テスト `stamps_every_definition_and_existing_groups` を足す（テスト計画）。まだどこからも呼ばない。
- 確認: `cargo test --locked --test lto` が `1 passed`。

### 手順 3: 属性の probe

- 変更: `src/driver.rs` の `probe_target_attributes`（新規）、`tests/lto.rs`。
- 内容: 「アルゴリズム」のとおり。`run_tool` と `driver_error` を使う。テスト `probes_clang_target_attributes` を足す。
- 確認: `cargo test --locked --test lto` が `2 passed`。

### 手順 4: `Emit::Bitcode` と CLI

- 変更: `src/driver.rs` の `Emit`・`BuildOptions::validate`・`BuildOptions::output_path`、`src/main.rs` の `--emit` の match と使い方、
  `src/main.rs` の tests。
- 内容: 「段ごとの変更」の CLI と driver の行。この手順では `build_complete` をまだ変えないので、`build` は `Emit::Bitcode` を object と同じ経路で
  処理してしまう。手順 5 まで CLI から試さない。`src/main.rs` の tests に `parse(&["build", "Main.tz", "--emit", "bitcode"])` が `Emit::Bitcode` になる
  assert と、`--target wasm32` との組み合わせが `validate` で E2000 になる assert を足す（既存の `--emit wgsl` の assert の隣）。
- 確認: `cargo test --locked --bin tsuzuri` が成功し、N が 1 以上増える。`cargo test --locked --lib validates_native_cpu_tuning_and_selects_architecture_flags` が `1 passed`。

### 手順 5: `build_complete` の配線

- 変更: `src/driver.rs` の `build_complete`。
- 内容: 「段ごとの変更」の `build_complete` の 3 行。probe の flags はランタイム C のコンパイルと同じ `native_compile_args(cfg!(windows), env::consts::ARCH)` と、
  `options.cpu == Cpu::Native` のときの `native_cpu_flag(env::consts::ARCH)?`。`merge_debug_ir` の既存の意味（macOS・debug・object）は変えず、
  `let merge_runtime_ir = merge_debug_ir || options.emit == Emit::Bitcode;`（新規の局所変数）で分岐を広げる。
- 確認: 次が成功する。`file` は `LLVM bitcode`（macOS は `LLVM bitcode, wrapper`）を表示し、`cmp` は何も出さない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
T=target/release/tsuzuri; W=/tmp/tz-pr08; B=/opt/homebrew/opt/llvm@21/bin
mkdir -p $W/lib && printf 'export def add :: i64 -> i64 -> i64\nfn add x y = x + y\n' > $W/lib/Main.tz
$T build $W/lib --emit bitcode -o $W/add.bc && file $W/add.bc
$T build $W/lib --emit bitcode -o $W/add2.bc && cmp $W/add.bc $W/add2.bc
TSUZURI_LLVM_LINK=$B/llvm-link $T build $W/fix --emit bitcode -o $W/fix.bc && file $W/fix.bc
$T build $W/lib --emit bitcode --target wasm32 -o $W/x.bc; echo "exit=$?"
```

最後の行は `error[E2000]: '--emit bitcode' requires '--target native'; wasm32 bitcode is not supported yet` と `exit=2`。

### 手順 6: 生成物の検査

- 変更: なし。
- 内容: 「生成コードの確認」の bitcode の項（要約・producer・属性・記号）を `add.bc` と `fix.bc` に行う。
- 確認: すべての期待どおり。とくに `fix.bc` の外部定義が `tz_add`・`tz_bytes`・`tz_sum`・`tsuzuri_alloc`・`tsuzuri_free` だけで、
  `tsuzuri_cpu_` で始まる記号が外部に出ないこと。

### 手順 7: C ホストとの LTO と inline

- 変更: なし。
- 内容: 「再現」のホスト `host.c` を `$W/host.c` に置き、`--emit bitcode` の出力で LTO リンクする。
- 確認: 出力が `4999999950000000`、`grep -c` が 0。

```sh
W=/tmp/tz-pr08; B=/opt/homebrew/opt/llvm@21/bin
clang -O3 -flto=thin -c $W/host.c -o $W/host.o
clang -O3 -flto=thin $W/host.o $W/add.bc -o $W/host-lto && $W/host-lto
$B/llvm-objdump -d --no-show-raw-insn $W/host-lto | grep -c tz_add
```

### 手順 8: テスト

- 変更: `tests/lto.rs`。
- 内容: テスト計画の残り 3 テストを足す。
- 確認: `TSUZURI_LLVM_LINK=/opt/homebrew/opt/llvm@21/bin/llvm-link cargo test --locked --test lto` が `5 passed`。`TSUZURI_LLVM_LINK` なしでも
  `5 passed`（runtime のテストは skip の行を出して成功する）。

### 手順 9: 既存の出力が変わらないこと

- 変更: なし。
- 内容: 手順 1 のコマンドを再実行して hash を比べる。全体のテストを流す。
- 確認: `shasum -a 256 -c /tmp/tz-pr08/before.sha` がすべて `OK`。`cargo test --locked` が成功する。
  `cargo test --locked --test host_abi` が手順 1 と同じ N で成功する。

### 手順 10: 計測

- 変更: なし。
- 内容: 「計測手順」のとおり。
- 確認: `target/perf/PR08-after/` に 3 通りの時間・大きさの記録があり、3 通りとも出力が `499351563280`。

### 手順 11: 文書

- 変更: 「ドキュメント」の各ファイル。
- 内容: 手順・版の規則・確認状況の表・計測結果を書く。
- 確認: `node scripts/check-docs.mjs _docs/guides/native-interop.md` が成功する。`git diff --check` が何も出さない。

## 計測手順

### 環境

専用の計測機で、ほかの重い処理を止めて行う。次を記録の先頭に残す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
P=target/perf/PR08-after; mkdir -p $P
{ git rev-parse HEAD; git status --short | wc -l; sw_vers; sysctl -n machdep.cpu.brand_string; clang --version | head -1;
  rustc -vV | grep LLVM; shasum -a 256 target/release/tsuzuri; } > $P/env.txt
```

### 種目 `lto/small_call`

`$P/small_call.c` を次の内容で作る。`values` は実行時に作り、`n` は引数なので、ループ全体の定数化は起きない。`c_add` は Tsuzuri の
`i64` の `+`（`add i64`、折り返し）と同じ意味を未定義動作なしで書いたもの。

```c
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int64_t tz_add(int64_t x, int64_t y);
static int64_t c_add(int64_t x, int64_t y) { return (int64_t)((uint64_t)x + (uint64_t)y); }
#ifndef ADD
#define ADD tz_add
#endif

int main(int argc, char **argv) {
    int64_t n = argc > 1 ? atoll(argv[1]) : 1000;
    int64_t values[1024];
    for (int i = 0; i < 1024; i++) values[i] = (int64_t)(i * 2654435761u % 1000u);
    int64_t total = 0;
    for (int64_t i = 0; i < n; i++) total = ADD(total, values[i & 1023]);
    printf("%lld\n", (long long)total);
    return 0;
}
```

独立の参照（Python、2026-09-30 に計算）: `n = 1000` で `498932`、`n = 1000000000` で `499351563280`。

```sh
P=target/perf/PR08-after; T=target/release/tsuzuri
mkdir -p $P/lib && printf 'export def add :: i64 -> i64 -> i64\nfn add x y = x + y\n' > $P/lib/Main.tz
$T build $P/lib --emit object -O3 -o $P/add.o
$T build $P/lib --emit bitcode -O3 -o $P/add.bc
clang -O3 -c $P/small_call.c -o $P/host.o && clang -O3 $P/host.o $P/add.o -o $P/object
clang -O3 -flto=thin -c $P/small_call.c -o $P/host-lto.o && clang -O3 -flto=thin $P/host-lto.o $P/add.bc -o $P/bitcode
clang -O3 -flto=thin -DADD=c_add $P/small_call.c -o $P/c_only
for v in object bitcode c_only; do echo "$v $($P/$v 1000) $($P/$v 1000000000)"; done
```

3 行とも `498932 499351563280` でなければ計測しない（正しさが先）。

### 時間（M2）

ウォームアップ 1 回の後、3 通りを交互に 9 回ずつ実行する。

```sh
P=target/perf/PR08-after
for v in object bitcode c_only; do $P/$v 1000000000 > /dev/null; done
for r in 1 2 3 4 5 6 7 8 9; do for v in object bitcode c_only; do
  /usr/bin/time -p $P/$v 1000000000 2>> $P/$v.time > /dev/null; done; done
for v in object bitcode c_only; do echo "$v $(awk '/^real/ {print $2 * 1000}' $P/$v.time | sort -n | tr '\n' ' ')"; done
```

各行の 5 番目が中央値、1 番目と 9 番目が最小と最大（ms）。

### 大きさ（M3）と呼び出し命令（M1）

```sh
P=target/perf/PR08-after; B=/opt/homebrew/opt/llvm@21/bin
for v in object bitcode c_only; do echo "$v $(stat -f %z $P/$v) $($B/llvm-objdump -d --no-show-raw-insn $P/$v | sed -n '/<_main>:/,/^$/p' | grep -c tz_add)"; done
```

Linux では `stat -c %s` を使い、`<_main>` を `<main>` に読み替える。

### 記録

- 生データ（`*.time`・`env.txt`・上の出力）は `target/perf/PR08-after/` に残す（Git には入れない）。
- PX01 が done（`benchmarks/metrics.mjs` がある）なら、同じ値を PX01 形式（suite `lto`、workload `small_call`、variant `object`・`bitcode`・`c_only`、
  `samples` は 9 個の ms）で `target/perf/PR08-after/lto.jsonl` に書く。PX01 が todo なら生データだけでよい。
- 中央値・最小・最大・大きさ・M1 と環境を `docs/benchmarks.md` に表で書く。差が広がりの範囲内なら「差なし」と書き、速いと主張しない。

## 生成コードの確認

### bitcode

```sh
W=/tmp/tz-pr08; B=/opt/homebrew/opt/llvm@21/bin
$B/llvm-bcanalyzer -dump $W/add.bc | grep -c '<GLOBALVAL_SUMMARY_BLOCK'
$B/llvm-bcanalyzer -dump $W/add.bc | grep -m1 'record string'
$B/llvm-dis $W/add.bc -o - | grep -E '^define|^attributes'
$B/llvm-nm --defined-only --extern-only $W/fix.bc
```

- 1 行目は `1`（ThinLTO の要約がある）。2 行目は `record string = 'APPLE_1_2100.3.34.2_0'`（このマシン）。
- 3 行目: すべての `define` が属性グループ `#<n>` を持ち、それらのグループがすべて `"target-cpu"="apple-m1"` と `"target-features"="+aes,` で
  始まる機能の列を含む（`--cpu native` なら probe の CPU 名）。`"unsafe-fp-math"`・`"no-nans-fp-math"` が現れない。
- 4 行目: 名前が `tsuzuri_alloc`・`tsuzuri_free`・`tz_add`・`tz_bytes`・`tz_sum` だけ。

### 機械語（native `-O3`）

- 手順 7 と M1: bitcode の実行ファイルの `_main` に `tz_add` への `bl` がない。object の実行ファイルにはある。
- inline の理由を見るときは、リンク行に `-Wl,-mllvm,-pass-remarks=inline -Wl,-mllvm,-pass-remarks-missed=inline`（macOS の `ld`）を足す。
  `'tz_add' inlined into 'main'` を含む行が出て、`conflicting attributes` が出ないこと。Linux の `lld` では `-Wl,-plugin-opt=-pass-remarks=inline`（未検証）。

### ほかの出力

- 手順 9 の `shasum -a 256 -c` で、llvm・header・object（`-O0`・`-O3`）が HEAD と byte 単位で同じであること。

## テスト計画

### Rust テスト（`tests/lto.rs`、新規）

補助関数: 一時ディレクトリ（`tests/host_abi.rs` と同じく pid と時刻の名前）、`TSUZURI_CLANG` か `clang`、ソース文字列を自分専用の一時 root の
`Main.tz` に書いて `tsuzuri::driver::Project::load` する `project(source)`（E03 の再帰探索のため、テストごとに root を分ける）。
期待値は Tsuzuri の出力ではなく手計算・C の意味から決める。

| テスト | 内容と期待 |
| --- | --- |
| `stamps_every_definition_and_existing_groups` | 入力は `declare void @llvm.trap()`、`define internal i64 @f(i64 %x) nounwind {`、`define i64 @tz_f(i64 %x) nounwind !dbg !7 {`、`define internal void @g(ptr %p) local_unnamed_addr #0 {`（本体つき）と `attributes #0 = { nounwind "no-trapping-math"="true" "tune-cpu"="generic" }`。属性 `"target-cpu"="cpu-x" "target-features"="+a,+b"` で、`@f` の行は `nounwind #1 {`、`@tz_f` の行は `nounwind #1 !dbg !7 {`、`@g` の行と `declare` は不変、`#0` は `{ nounwind "no-trapping-math"="true" "target-cpu"="cpu-x" "target-features"="+a,+b" }`、末尾に `attributes #1 = { "target-cpu"="cpu-x" "target-features"="+a,+b" }`。期待する全文を文字列で書いて `assert_eq!`。2 回呼んで同じ結果。`define` のない入力は不変 |
| `probes_clang_target_attributes` | `#[cfg(unix)]`。`probe_target_attributes(&clang, &["-fPIC"], &root)` が `"target-cpu"="` で始まり ` "target-features"="` を含む。2 回の結果が同じ |
| `bitcode_inlines_small_exports_into_c_host` | `add` だけのソース（「再現」）。`-O0`・`-O3` で `Emit::Bitcode` を `tsuzuri::driver::build` し、2 回の出力が byte 一致、先頭 4 byte が `BC\xC0\xDE` か `\xDE\xC0\x17\x0B`（Darwin の wrapper）。「再現」の `host.c` と `clang -O3 -flto=thin host.c <out.bc> -o <exe>` でリンクして実行し、stdout が `4999999950000000\n`（`n(n-1)/2`、`n = 10^8`）。`nm <exe>` の出力に `tz_add` がない。`--target wasm32` の `BuildOptions::validate` が E2000 |
| `bitcode_keeps_owned_results_balanced` | `add` と `bytes`（手順 1 の 2 関数）。`tsuzuri::llvm::header` で `api.h`、`tsuzuri::llvm::emit_target(&module, Entry::Library, false)` の IR の `@malloc`／`@free` を `@tracked_alloc`／`@tracked_free` に置換し（`tests/host_abi.rs` と同じ）、`probe_target_attributes` と `stamp_target_attributes` を通して `clang -x ir -Wno-override-module -O3 -flto=thin -c` で `module.o` にする。ホストは `tests/host_abi.rs` の `tracked_alloc`・`tracked_free` を写し、`tz_bytes(&out, 5)` の中身が `0 1 2 3 4`、`tsuzuri_free(out.ptr)` と `tsuzuri_free(tsuzuri_alloc(0))` の後に `live == 0`、`tz_add(40, 2) == 42` を assert する。リンクは `-O0` と `-O3`（ともに `-flto=thin`）。同じホストを置換済みの `.ll` と LTO なしでリンクした結果と stdout が一致する。バッファ型の名前は生成 header のもの（`tsuzuri_ubyte_buffer` の想定。header で確かめる） |
| `bitcode_merges_c_runtime_and_internalizes_it` | `sum`（`Array.sum`。native では `tsuzuri_cpu_sum_i64` を使う）。`TSUZURI_LLVM_LINK` がなければ `skipped: set TSUZURI_LLVM_LINK to llvm-link matching Clang` を出して成功する（D9）。あれば bitcode を作り、`int64_t v[] = {1, 2, 3}` の `tz_sum(v, 3)` が `6`。`llvm-link` と同じディレクトリに `llvm-nm` があれば、`llvm-nm --defined-only --extern-only` の名前が `tz_sum`・`tsuzuri_alloc`・`tsuzuri_free` の部分集合 |

`src/main.rs` の tests: `--emit bitcode` の parse と、`emit kind must be ...` を assert している箇所があれば一覧の更新（手順 4）。

### E2E

- 他の出力の生成 IR を変えないので、既存の E2E suite は変わらない。新しい `tests/*.mjs` は作らない（C のホストが要るので Rust テストで行う）。
- native の `-O0`・`-O3` は上の 3・4 番目のテストで確かめる。WASM は Phase 1 で触らない（`tests/host_abi.rs` の WASM 部分が不変のまま通る）。
- トラップ: `tests/host_abi.rs` が既存の経路で確かめる。bitcode でも同じ wrapper なので新しいトラップのテストは足さない。

### 既存テストへの影響

`emit kind must be ...` の文の一覧だけ（あれば）。それ以外はなし。

### 性能

「計測手順」のとおり。合否の閾値は作らない。

## ドキュメント

- `docs/language.md`「公開 ABI」: 末尾の `sh` の例に `tsuzuri build Kernel.tz --emit bitcode -o kernel.bc` を足し、ThinLTO（ホストは `-flto=thin`）・
  LLVM の版の規則・公開される記号（D5）を 3〜5 行で書く。
- `_docs/guides/native-interop.md`: 節「言語間 LTO（bitcode）」（新規）。C のリンク手順（手順 7 のコマンド）、Rust の手順（下の例。未検証と明記）、
  版の確認方法、「LLVM の版」の組み合わせの表、`-fuse-ld=lld`（macOS）の非対応。
- `README.md`「CLI」の emit の一覧と、「ビルド」の `TSUZURI_LLVM_LINK` の説明（bitcode でランタイム C を含むときにも要る）。
- `docs/architecture.md`: driver の段の説明に bitcode の経路（probe・属性の付与・`llvm-link --internalize`・`-flto=thin`）を 2〜4 行。
- `docs/benchmarks.md`: 節「言語間 LTO（PR08）」（新規）に環境・3 通りの中央値・最小・最大・大きさ・M1。「測定条件」の「LTO は行いません」は
  他の比較には引き続き当てはまることを残す。
- `_perfs/README.md` の PR08 の状態（GUIDE §10 の手順で更新）。

Rust の例（新 API（実装後に有効。未検証））。`tz_add` は `extern "C" { fn tz_add(x: i64, y: i64) -> i64; }` で宣言する。

```sh
tsuzuri build Kernel.tz --emit bitcode -o target/tz/tzkernel.o
ar crs target/tz/libtzkernel.a target/tz/tzkernel.o
RUSTFLAGS="-C linker-plugin-lto -C linker=clang -L native=target/tz -l static=tzkernel" cargo build --release
# Linux: RUSTFLAGS に -C link-arg=-fuse-ld=lld を足す。rustc -vV の LLVM の版がリンカーの LLVM 以下であること
```

## 受け入れ条件

- [ ] `tsuzuri build <dir> --emit bitcode` が native で ThinLTO の要約を持つ bitcode を出し、`--target wasm32` は診断の表の E2000 になる。
- [ ] bitcode のすべての `define` が probe した target 属性を持ち、外部に定義される記号が `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ（手順 6）。
- [ ] 同じ入力・同じ Clang で bitcode が byte 単位で一致する。
- [ ] 手順 7 で `tz_add` がホストのループに inline され（呼び出し命令 0）、出力が `4999999950000000`。
- [ ] `TSUZURI_LLVM_LINK` ありで `cargo test --locked --test lto` が `5 passed`、`cargo test --locked` が成功する。
- [ ] 手順 9 の `shasum -a 256 -c` がすべて `OK`（他の出力が不変）。
- [ ] `lto/small_call` の 3 通りを計測し、環境とともに `docs/benchmarks.md` に記録している。測っていない速さを主張していない。
- [ ] 「ドキュメント」の各ファイルを更新し、未検証の組み合わせ（Rust・Linux・Windows）をそう書いている。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 属性がないと inline されない: remark `conflicting attributes` が出る。`alwaysinline` を付けたり inline の閾値を変えたりして回避しない（D3）。
- ホストが `-flto`（full）だと、Tsuzuri の ThinLTO module から import せず inline されない（「再現」で確認）。文書と例は必ず `-flto=thin`。
- `.bc` をリンク行に渡すと Clang が一度コンパイルし直す。結果は同じだが、Rust の static library には `.o` の名前で入れる。
- macOS で Homebrew の `lld` を使うと Apple clang の bitcode で `LLVM ERROR: Unsupported stack probing method` になる。Apple の `ld` を使う。
- Xcode には `llvm-link` がない。`/opt/homebrew/opt/llvm@21/bin/llvm-link` を使う。llvm@22・@23 の `llvm-link` の `-S` 出力は Apple clang 21 が
  読めない構文を含み得る（generated `.ll` の既知の差）。版を混ぜない。
- `--internalize` は 2 番目以降のファイルだけに効く。`module.ll` を必ず先頭に置く（逆にすると `tz_*` が internal になり停止条件）。
- `numeric.ll` のグループの `"tune-cpu"="generic"` は置き換わる。意図どおり（通常の object でも Clang の既定 CPU で codegen される）。
  それ以外の鍵（`"no-builtins"`・`memory(...)` など）は触らない。
- `-O0` の bitcode では C ランタイムの関数に Clang が `optnone noinline` を付けるので、リンク時にも inline されない。仕様どおり。
- テスト 4 は driver ではなく `emit_target` の IR を使う（`@malloc` の置換のため）。driver の経路に揃えようとしない。driver の経路はテスト 3・5 が見る。
- 実行ファイルへの LTO では inline 後に `tz_add` の記号が消える。共有ライブラリ（`-dynamiclib`・`-shared`）では `tz_*` は残るのが正しい。
- macOS の remark は `ld: warning: LTO remark:` の行として stderr に出る。警告ではない。
- Windows は `llvm::windows_abi` が足す `define internal i64 @write(...) {` も属性付与の対象になる。未検証なので主張しない。

## 対象外

- WASM の bitcode と `wasm-ld` の LTO（Phase 2）、JIT。
- full LTO 用の bitcode を選ぶ option、コンパイラによる LLVM の版の検査、ビルドキャッシュ（D6・D7）。
- Tsuzuri の bitcode の sanitizer 計装。確保の検査は `live == 0` の追跡 allocator で行う。
- PB01 の事前ビルド済みランタイム（object は LTO に参加しない）、E12 のリンク指定の CLI、`run`・`test` での bitcode。
- PR01 の属性の追加。PR01 が IR に付けた属性は bitcode にもそのまま入るので、このチケットで追加の作業はない。

## 決定事項

### D1: 出力の名前と対象

- 決定: `--emit bitcode`、拡張子 `bc`、`--target native` だけ。
- 理由: 既存の値は小文字 1 語（`llvm` がテキスト IR）で、`llvm-bc`（rustc）の形は既存の命名と合わない。WASM は export・import の検査が別に要る。
- 状態: 既定案（実装者はこの案に従う）

### D2: ThinLTO

- 決定: bitcode は `-flto=thin` で作り、ThinLTO の要約を持たせる。ホストにも `-flto=thin` を求める。
- 理由: full LTO のホストと ThinLTO の module の組では inline されないことを確認した。Rust の linker-plugin LTO も ThinLTO の module を出す。
- 状態: 既定案（実装者はこの案に従う）

### D3: target 属性の付与

- 決定: 設定された Clang で空の C 関数を IR にし、その `"target-cpu"`・`"target-features"`・`"tune-cpu"` を bitcode の全 `define` に付ける（アルゴリズム）。
- 理由: 属性がないことが inline されない原因だと確認し、付けると inline された。値は同じ Clang・同じ flags で C に付く値と同じなので、C のホストと
  互換で、通常の object の codegen が使う既定 CPU とも一致する。新しい crate や LLVM の API が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D4: ランタイムの扱い

- 決定: 埋め込みの `.ll` は IR に含まれたまま。C のランタイムは必要なときだけ `-S -emit-llvm` で IR にし、`llvm-link --internalize`
  （`TSUZURI_LLVM_LINK`）で一つにしてから bitcode にする。
- 理由: 旧「未決事項」の既定案（一つの bitcode）を保つ。既存の `merge_debug_ir` の経路を再利用でき、ランタイムの記号が外へ出ない。
- 状態: 既定案（実装者はこの案に従う）

### D5: 公開する記号

- 決定: 「生成 IR とランタイム」の表のとおり。外部の定義は `tz_*`・`tsuzuri_alloc`・`tsuzuri_free` だけ。新しい外部記号を足さない。
- 理由: C header・object と同じ公開 ABI を保つ。
- 状態: 既定案（実装者はこの案に従う）

### D6: LLVM の版

- 決定: コンパイラは版を検査しない。版は bitcode の IDENTIFICATION に入り、文書で確認方法と規則（リンカーの LLVM ≥ 全 producer）を示す。
- 理由: 読めるかどうかを決めるのはホストのリンカーで、コンパイラはそれを知らない。rustc の LLVM 22.1.8 の bitcode を Apple の `ld` が読めた例のように、
  版の数字だけでは判定できない。
- 状態: 既定案（実装者はこの案に従う）

### D7: キャッシュ

- 決定: bitcode はビルドキャッシュを使わない。
- 理由: key に `llvm-link` と probe の結果を足す必要があり、bitcode は頻繁に作る出力ではない。
- 状態: 既定案（実装者はこの案に従う）

### D8: option の意味

- 決定: `-O<n>` は pre-link の最適化の段。`--cpu native`・`--debug-info`・`--trap-info` は object と同じ意味で使える。`--cpu native` の既存の
  診断文は変えない。
- 理由: IR を object と同じ `llvm::emit_native_build` で作るので、意味を分ける理由がない。既存の文を変えるとテストの期待値が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D9: `llvm-link` のないテスト環境

- 決定: `bitcode_merges_c_runtime_and_internalizes_it` は `TSUZURI_LLVM_LINK` がなければ skip の行を出して成功する。他の 4 テストは Clang だけで動く。
- 理由: Xcode に `llvm-link` がなく、既存のテストも `llvm-link` を要求していない。実装手順の確認コマンドは必ず `TSUZURI_LLVM_LINK` を付けて実行する。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2（WASM）

- 決定: 「Phase 分割」の設計方針で、人間が求めた場合だけ着手する。
- 理由: `wasm-ld` の LTO と export の組み合わせ、node での import の検査が別に要り、Phase 1 の目的（C/C++/Rust のホスト）には不要。
- 状態: 既定案（実装者はこの案に従う）
