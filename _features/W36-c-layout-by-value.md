# W36: C レイアウトの record の値渡し（引数と結果。Windows x64・Arm64 から）

| 項目 | 内容 |
| --- | --- |
| ID | W36 |
| 優先度 | P2 |
| 規模 | L（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | W21 |
| 後続 | W25 |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W30 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E05 と W21 の値渡しの除外の改訂）。D1・D2 が要承認、ほかは既定案 |
| 改善する劣位 | C/C++・Rust（`#[repr(C)]`）・C#（P/Invoke）比: C の関数が構造体を値で取る・返す API（Win32 の `WindowFromPoint`・`PtInRect` の `POINT`、Direct2D の `D2D1_POINT_2F`、raylib の `Vector2`）を宣言できない。今の書き方（`ref` のない record）は診断なしにポインターを渡す |
| 手本にする既存実装 | `src/llvm_imports.rs` の `FunctionEmitter::host_call`・`host_result_slot`・`read_host_result`（record の一時領域）、[W21](W21-c-layout-records.md) の `llvm_abi::record_layout`・`c_layout_fields`・`read_host_record`・`write_host_record`（`@c_layout` の配置を 1 か所で求める）、`src/target.rs` の `current_platform`（IR の生成中の target）、`clang -S -emit-llvm`（独立した参照。GUIDE-LITE §2 規則 10） |
| 主な影響ファイル | `src/abi.rs`, `src/check.rs`, `src/llvm_imports.rs`, `src/llvm_byvalue.rs`（新規）, `src/llvm_abi.rs`, `src/llvm.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/E1008.md`, `tests/c_layout_by_value.rs`（新規）, `tests/c_layout_by_value.mjs`（新規）, `tests/c_layout_by_value_host.c`（新規）, `tests/fixtures/c_layout_by_value/Main.tz`（新規）, `tests/windows.mjs`, `.github/workflows/windows.yml`, `docs/language.md`, `docs/architecture.md`, 言語リファレンスの 4 ページ |

## 実装者向け早見

- 範囲: [W21](W21-c-layout-records.md) の `@c_layout` record を、`ref` なしで `extern def` の引数に書くと C の値渡し、結果に書くと C の値返しにする。Phase 1 は Windows x64（MSVC・MinGW）と Windows Arm64（MSVC・MinGW）、Phase 2 は System V x86-64（Linux・Intel の macOS）と AAPCS64（Linux・macOS）。Phase の外の native の target は `E1008`（kind 案 `abi_record_by_value_target`）、WASM は `E1008`（`abi_native_only`）。印のない record、`ref R`・`ref mut R`、`export def`、コールバックは今のまま。
- 最初に読む: 「現状」の再現（`PtInRect`）→ 仕様 2 の分類 → 「現状」の参照表 → W21 の仕様 3・4 と D4・D8。
- 最初に書くテスト: `tests/c_layout_by_value.rs` の `windows_x64_passes_by_size` と `windows_arm64_passes_hfa_in_float_registers`（`build --emit llvm --target <triple>` の `declare` の行を、参照表から手で書いた期待と比べる）→ `tests/c_layout_by_value.mjs` の C のホスト。
- 触ってはいけない: `@c_layout` の無い record の ABI（[E05](_completed/E05-host-abi-buffers.md)）、`ref R`（`const T *`）と `ref mut R`（`T *`）、`export def` の IR とヘッダー、WASM の出力、値渡しを使わないプログラムの IR・ヘッダー・診断（1 byte も変えない）。
- 完了の見分け方: 次のプログラムを `tsuzuri build <dir> -l user32 -o pt.exe` で作ると（`-l user32` が無いと `PtInRect` が未解決の `E2002`）、Windows x64 と Arm64（CI）で終了コード 1 を返し、IR に `declare i32 @PtInRect(ptr, i64)` がある（今は印のない record で同じことを書くと 0 を返す。「現状」）。

```tsuzuri
@c_layout
record Rect { left: i32, top: i32, right: i32, bottom: i32 }
@c_layout
record Point { x: i32, y: i32 }

@trusted
extern "PtInRect" def pt_in_rect :: ref Rect -> Point -> i32

def main :: unit -> i32 = \() ->
    let r = Rect { left: 0, top: 0, right: 10, bottom: 10 }
    pt_in_rect (ref r) (Point { x: 5, y: 5 })
```

## 目的

C の関数は構造体を値で取り、値で返せる。Win32 にも `WindowFromPoint(POINT)`・`MonitorFromPoint(POINT, DWORD)`・`PtInRect(const RECT *, POINT)` があり、Direct2D の `ID2D1RenderTarget::DrawLine` は `D2D1_POINT_2F` を値で取る。どのチケットもこれを担当していない: [E05](_completed/E05-host-abi-buffers.md) は「C の by-value struct ABI classification に依存しない」と決めて record をポインターでだけ渡し、[W21](W21-c-layout-records.md) D4 もそれを保ち、値渡しを対象外にして「`i64` に詰める」回避策を挙げた。その回避策は Windows Arm64 と System V で浮動小数点の構造体を誤る（`D2D1_POINT_2F` は Arm64 では浮動小数点レジスター 2 つ）。しかも今の書き方は、`ref` を付けない record を診断なしにポインターで渡し、C の側は黙って誤った値を読む。

LLVM IR は構造体の値渡しを C の ABI に下げない。Clang はフロントエンドで target ごとに構造体を整数・浮動小数点の値やポインターに変えてから IR を出す（参照表）。このチケットは、C の配置が明示された `@c_layout` record（W21）に限って、同じ分類をコンパイラが target ごとに行う。[W25](W25-extern-interface-com.md) の D10 が Phase 3 で引き受けていた値渡しの引数はこのチケットに移る（[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)）。

## 着手条件と停止条件

### 着手条件

- W21 の Phase 1 が done（`grep -n "fn c_layout_fields" src/llvm_abi.rs`、`grep -n "c_layout" src/check.rs`）。
- W21 が、`ref` の無い `@c_layout` record を `extern def` の引数と結果でどう受けて出荷したかを確かめる（`grep -rn "c_layout_by_value_pending" src/` と、W21 のテストの `positions`）。D2 のとおり `E1008` で綴りを空けていれば進める。W21 の仕様 4 の表のとおり `const T *` と先頭の out 引数で受けていれば S1。
- D1・D2 が承認されている。承認前は手順 1（参照表を作るだけ）まで。
- [W14](_completed/W14-extern-multiple-declarations.md) が done なら、その `abi::c_signature` に値渡しの形を足す（手順 4）。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` の `--emit llvm -O1`（native、`--target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`・`x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`・`wasm32`）と、`export` か `extern` を持つものの `--emit header` を `target/scratch/w36/base/` に保存する。

### 停止条件

- **S1** W21 が `ref` の無い `@c_layout` record を extern の引数・結果で受けて出荷している（`const T *`・out 引数）。このチケットの綴りはそれを値渡しに変えるので、既存のプログラムの C の ABI が変わる。選択肢: A 互換を破る変更として承認を受け、`docs/stability.md` に記録する。B 別の綴りにする（D1 の退けた案）。止めて報告する。
- **S2** このチケットの分類と、参照の Clang（手順 1）の `declare` が、参照集合のどれかの構造体と target で食い違う。規則を即興で直さず、構造体と 2 つの `declare` を記録して報告する。
- **S3** 値渡しを使わないプログラムの IR・ヘッダー・診断が、保存した基準と 1 byte でも違う。
- **S4** 分類に要る情報（葉の field の型とオフセット、大きさ、整列）を W21 の `c_layout_fields`・`record_layout` から得られず、W21 の配置の計算を変えたくなった。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 決定

- E05（`_features/_completed/E05-host-abi-buffers.md`）の「目的」: 「raw LLVM IR を直接生成しているため、C の by-value struct ABI classification に依存しない。そのため scalar-only record は pointer 渡し/out pointer に統一する」。「落とし穴」: 「C struct を by value で返すと platform ABI classification を LLVM IR 側で正確に再現する必要が出る」。
- W21 の仕様 4: `@c_layout` の「extern の引数 `R`・`ref R`」は `const T *`、「extern の結果 `R`」は先頭の out 引数 `T *out`。D4 は「C の値渡しの分類は再現しない」。「対象外」は値渡しを挙げ、`i64` に詰める回避策を文書に書くとする。
- W25 の仕様 9 と D10: 起票時は、Phase 3 で Windows の 2 つの target に限って値渡しの `@c_layout` record の引数を許し、ほかの target は `E1008`（kind 案 `abi_record_by_value_target`）としていた。同じ節は「`extern def` の record の結果は今の out ポインターの規則のまま」とも書いていた。このチケットの起票に合わせて、W25 の値渡しの部分はこのチケットへの参照に置き換え、`@c_layout` の `extern def` の結果はこのチケットの値返しとした（W25 の仕様 9・D10、Phase 3 の着手条件に W14 と W36）。メソッドの record の結果（MSVC のメンバー関数の規則）は W25 に残る。

### コード

- `src/abi.rs` の `parameter` は、`scalar_record(ty)`（`ref` なし）と `Reference(scalar_record)` の両方を受ける。`out_result` は record ならいつも真。
- `src/llvm_imports.rs` の `host_call` は、record の引数（`ref` の有無によらず）を `host_result_slot`（`alloca %tz_record_…, align 16` と `zeroinitializer` の `store`）に `write_host_record` で書き、`ptr` を渡す。record の結果は先頭の `ptr` で受けて `void` を返す `declare` にし、呼んだ後で `read_host_record` する。`header` は record の結果の関数を `void` で出す。
- 再現: 次の `Main.tz` を `target/debug/tsuzuri.exe build <dir> --emit llvm` にすると `declare i32 @take_point(ptr)`、`declare void @ret_point(ptr, i32)`、`define dllexport void @tz_echo(ptr %out, ptr %arg0)` で、`--emit header` は `void tz_echo(tz_record_4Main_5Point *out, const tz_record_4Main_5Point *arg0);`。

```tsuzuri
record Point { x: i32, y: i32 }
@trusted
extern "take_point" def take_point :: Point -> i32
@trusted
extern "ret_point" def ret_point :: i32 -> Point
export def use_point :: i32 -> i32 = \n ->
    let p = ret_point n
    take_point p
export def echo :: Point -> Point = \p -> p
```

- IR を生成している間の target は `crate::target::current_platform()`（`PlatformScope` が運ぶ）で、`windows()`・`aarch64()`・`mach_o()`・`linux()` を持つ。`check` は target を知らない（W21 の「現状」の「target の情報」）。
- `src/llvm.rs` の `direct_callee`（`with_blocking_externs` が使う）は、`call ` の後の最初の `@` から `(` までを名前にする。`call void @f(ptr sret(%T) %v)` と `call { float, float } @f(` でも名前は `f` になる（コードを読んで確かめた。テストはない）。

### 再現: 印のない record を値渡しの C の関数に渡すと黙って誤る

Git Bash で `T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain`、`target/scratch/w36/pt/Main.tz` に早見のプログラムから 2 つの `@c_layout` を外したものを置き、`TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build target/scratch/w36/pt -l user32 -o pt.exe && ./pt.exe; echo $?` → **0**（(5, 5) は矩形の中なので正しくは 1）。IR は `declare i32 @PtInRect(ptr, ptr)` と `call i32 @PtInRect(ptr %v4, ptr %v14)` で、`POINT` の場所に一時領域のアドレスが入る。引数を `i64` にして `5 * 4294967296 + 5` を渡す版（`pt2`）は **1**。同じ symbol を 2 つの型で宣言すると今は `E1008`（W14 の範囲）なので、2 つは別のプロジェクトにした。

### 値渡しの Win32 の API（Windows SDK 10.0.26100）

`um/WinUser.h`: `WindowFromPoint(POINT)`（9672 行）、`WindowFromPhysicalPoint(POINT)`（9679）、`ChildWindowFromPoint(HWND, POINT)`（9686）、`PtInRect(CONST RECT *, POINT)`（9932）、`DragDetect(HWND, POINT)`（8259）、`MenuItemFromPoint(HWND, HMENU, POINT)`（8163）、`MonitorFromPoint(POINT, DWORD)`（13717）。`um/d2d1.h`: `DrawLine(D2D1_POINT_2F, D2D1_POINT_2F, ID2D1Brush *, FLOAT, ID2D1StrokeStyle *)`（2523。COM のメソッド、W25）。`um/oleidl.h`: `IDropTarget::DragEnter(IDataObject *, DWORD, POINTL, DWORD *)`（3717。Tsuzuri が実装する側、[W26](W26-com-objects-in-tsuzuri.md)）。

### 参照表: Clang が下げた形

`C:\Program Files\LLVM\bin\clang.exe`（22.1.8）で、構造体を値で取る `int take(S)` と値で返す `S ret(void)` の C を `--target=<triple> -O1 -S -emit-llvm` にし、`declare` の型を読んだ（属性 `noundef`・`dead_on_return`・`dead_on_unwind writable` と `sret` の `align` は省いた）。`x86_64-pc-windows-gnu` は `-msvc` と、`aarch64-pc-windows-gnu` は `-msvc` と、全行で同じだった。結果の「構造体」は名前付きの struct 型（`{ float, float }` など、packed でない）。

| C の構造体（大きさ） | Win x64 引数 / 結果 | Win Arm64 引数 / 結果 | SysV x86-64（Linux）引数 / 結果 | AAPCS64 Linux 引数 | macOS Arm64 引数 |
| --- | --- | --- | --- | --- | --- |
| `{int x, y}`（8、`POINT`） | `i64` / `i64` | `i64` / `i64` | `i64` / `i64` | `i64` | `i64` |
| `{float x, y}`（8、`D2D1_POINT_2F`） | `i64` / `i64` | `[2 x float]` / 構造体 | `<2 x float>` / `<2 x float>` | `[2 x float] alignstack(8)` | `[2 x float]` |
| `{int l, t, r, b}`（16、`RECT`） | `ptr` / `sret` | `[2 x i64]` / `[2 x i64]` | `i64, i64` / `{ i64, i64 }` | `[2 x i64]` | `[2 x i64]` |
| `{unsigned char r, g, b}`（3） | `ptr` / `sret` | `i64` / `i24` | `i24` / `i24` | `i64` | `i64` |
| `{double x, y}`（16） | `ptr` / `sret` | `[2 x double]` / 構造体 | `double, double` / `{ double, double }` | `[2 x double] alignstack(8)` | `[2 x double]` |
| `{float x, y, z}`（12） | `ptr` / `sret` | `[3 x float]` / 構造体 | `<2 x float>, float` / `{ <2 x float>, float }` | `[3 x float] alignstack(8)` | `[3 x float]` |
| `{float f}`（4） | `i32` / `i32` | `[1 x float]` / 構造体 | `float` / `float` | `[1 x float] alignstack(8)` | `[1 x float]` |
| `{double d}`（8） | `i64` / `i64` | `[1 x double]` / 構造体 | `double` / `double` | `[1 x double] alignstack(8)` | `[1 x double]` |
| `{short s}`（2） | `i16` | `i64` | `i16` | `i64` | `i64` |
| `{double d; long long i}`（16） | `ptr` / `sret` | `[2 x i64]` / `[2 x i64]` | `double, i64` / `{ double, i64 }` | `[2 x i64]` | `[2 x i64]` |
| `{double d; float f}`（16） | `ptr` / `sret` | `[2 x i64]` / `[2 x i64]` | `double, float` / `{ double, float }` | `[2 x i64]` | `[2 x i64]` |
| `{unsigned char b[5]}`（5） | `ptr` / `sret` | `i64` / `i40` | `i40` / `i40` | `i64` | `i64` |
| `{unsigned short a, b, c}`（6） | `ptr` / `sret` | `i64` / `i48` | `i48` / `i48` | `i64` | `i64` |
| `#pragma pack(1)` の `{unsigned char tag; int value}`（5） | `ptr` / `sret` | `i64` / `i40` | `ptr byval align 8` / `sret` | `i64` | `i64` |
| `{float v[4]}`（16）、`pack(2)` の `{float a[3]; float b}` | `ptr` / `sret` | `[4 x float]` / 構造体 | `<2 x float>, <2 x float>` | `[4 x float] alignstack(8)` | `[4 x float]` |
| `pack(1)` の `{float a, b}`（8） | `i64` / `i64` | `[2 x float]` / 構造体 | `<2 x float>` | `[2 x float] alignstack(8)` | `[2 x float]` |
| `{ {float x, y} p; float z }`（12、入れ子） | `ptr` / `sret` | `[3 x float]` / 構造体 | `<2 x float>, float` | `[3 x float] alignstack(8)` | `[3 x float]` |
| `{float a, b, c, d, e}`（20） | `ptr` / `sret` | `ptr` / `sret` | `ptr byval align 8` / `sret` | `ptr` | `ptr` |
| `{long long a, b, c}`（24） | `ptr` / `sret` | `ptr` / `sret` | `ptr byval align 8` / `sret` | `ptr` | `ptr` |
| `{void *h; int x}`（16） | `ptr` / `sret` | `[2 x i64]` / `[2 x i64]` | `ptr, i32` / `{ ptr, i32 }` | `[2 x i64]` | `[2 x i64]` |

- 呼び出しの地点にも同じ属性（`alignstack(8)`、`byval(%struct.S) align 8`）が付く。
- System V で、整数レジスターの残りが足りない構造体はメモリーに回る: `five_then_rect(long ×5, Rect, long)` は `(i64 ×5, ptr byval(%struct.Rect) align 8, i64)`、`seven_doubles_then_p2d(double ×7, P2D, double)` は `(double ×7, ptr byval(%struct.P2D) align 8, double)`。後ろのスカラーはレジスターに入る。
- `sret` のレジスター: `declare void @with_sret(ptr sret(%R), i32)` と `declare void @with_out(ptr, i32)` の呼び出しを `clang -S` すると、`aarch64-pc-windows-msvc` と `aarch64-unknown-linux-gnu` は `sret` を `x8`・整数を `w0`、out ポインターを `x0`・整数を `w1` に置いた。`x86_64-pc-windows-msvc` はどちらも `rcx`・`edx`。したがって今の out ポインターの規則は、Arm64 では C の値返しと別物である（x64 では、メモリーで返す大きさの構造体に限って偶然一致する）。
- ポインターだけの構造体: `{void *h}` は Win x64 で `i64`、Arm64 と System V の引数で `ptr`。`{void *h; void *g}` は Arm64 の引数で `[2 x ptr]`、結果で `[2 x i64]`、System V で `ptr, ptr` と `{ ptr, ptr }`。`ptr` と `i64` は x86-64 と AArch64 で同じレジスターに入る（W14 D2 の `Word64` と同じ理由）ので、このチケットは整数の形（`i64`・`[2 x i64]`）で出し、参照との突き合わせでは同じと見なす。
- 参照に使った C は 4 つ（構造体 14・10・6・2 個の 4 つ）と、レジスターの数え方の 3 関数の C、`sret` のレジスターを見た IR 1 つ。手順 1 で同じ構造体と関数を `tests/c_layout_by_value_reference.c`（新規）に置く。

## 変えてはいけない意味

- `@c_layout` の無い record の受け渡し（E05: `const T *` と先頭の out 引数）、そのヘッダーと JS の糊。
- `ref R`（`const T *`）、`ref mut R`（[W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md)、`T *`）、`Maybe<ref R>`（[W15](_completed/W15-abi-nullable-handles.md)）。
- `export def` の `@c_layout` record の引数と結果（W21: `const T *` と先頭の out 引数）。コールバックの型の規則（record を取らない）。
- 引数の評価順（左から右）。トラップの意味。WASM の出力。

## 仕様

### 1. 綴りと位置

- `extern def` の引数の `@c_layout` record `R`（`ref` なし）は、C の `T` の値渡しになる。結果の `@c_layout` record `R` は、C の `T` の値返しになる。ほかの位置と型は「変えてはいけない意味」のとおり。
- W21 の Phase 2 の field（`FnPtr`・`ref ZString`）を持つ record は、W21 の仕様 4 の規則に従う: 値渡しの引数は入力なので可、値返しの結果は W21 の kind（`fnptr_from_host`・`c_layout_input_only`）で `E1008`。
- [W25](W25-extern-interface-com.md) の `extern interface` のメソッドの引数は、`host_call` を共有するので同じ規則になる。メソッドの record の結果は W25 の規則（MSVC の C++ のメンバー関数の規則）で、このチケットの対象外。
- 可変長引数の extern は無いので扱わない（Windows Arm64 は可変長の引数で HFA を使わない）。

### 2. Phase 1 の分類（Windows）

record の大きさ `s` と整列は W21 の `record_layout`、葉（入れ子の record と固定長配列を開いた数とハンドル）は `c_layout_fields` から求める。HFA は、葉が 1〜4 個で、すべて `f32` か、すべて `f64` の record（`pack` は関係しない。参照表）。

| target | 引数 | 結果 |
| --- | --- | --- |
| Windows x64（`x86_64-pc-windows-msvc`・`-gnu`） | `s` が 1・2・4・8 なら `i8`・`i16`・`i32`・`i64`。ほかは、呼び出しごとの新しい複写への `ptr` | `s` が 1・2・4・8 なら `i8`・`i16`・`i32`・`i64`。ほかは先頭の `ptr sret(%tz_record_…)` で `void` |
| Windows Arm64（`aarch64-pc-windows-msvc`・`-gnu`） | HFA は `[n x float]`・`[n x double]`。ほかは `s ≤ 8` なら `i64`、`s ≤ 16` なら `[2 x i64]`、それより大きければ複写への `ptr` | HFA は `{ float, … }`・`{ double, … }`（n 個）。ほかは `s ≤ 8` なら `i{8s}`（`i24`・`i40` など）、`s ≤ 16` なら `[2 x i64]`、大きければ `ptr sret(%tz_record_…)`（`x8`） |

- Windows x64 では HFA を使わない（`{float x, y}` は `i64` で整数レジスター）。
- 整列 16 以上の record は W21 では作れない（field の整列は 8 まで）。分類の関数は整列 16 以上を `unreachable!` にせず、`S4` と同じく報告の対象にする。

### 3. Phase 2 の分類（System V x86-64、AAPCS64）

- AAPCS64（Linux、macOS）: Windows Arm64 と同じ形で、Linux だけ HFA の引数の配列に `alignstack(8)` を付ける（宣言と呼び出しの地点の両方）。macOS は付けない。
- System V x86-64（Linux、Intel の macOS）: `s > 16` か、整列していない field（オフセットが自然な整列の倍数でない。`pack` で起きる）があればメモリー（引数は `ptr byval(%tz_record_…) align 8`、結果は `sret`）。ほかは 8 バイトごとに INTEGER か SSE に分け（両方なら INTEGER）、SSE の 8 バイトは `double`・`float`・`<2 x float>`、INTEGER は `i64`（最後の 8 バイトが足りなければ `i{8×残り}`。Clang はちょうどポインター 1 つの 8 バイトを `ptr` にするが、同じレジスターなので `i64` で出す）。引数は 1 つか 2 つの IR の引数に開き、結果は 1 つの値か `{ a, b }`。呼び出し全体で左から整数レジスター 6 個（`sret` が 1 個使う）と SSE 8 個を数え、構造体の全部が入らなければその構造体だけメモリーに回す（参照表の 2 関数）。
- 規則の正は参照の Clang の出力で、表にない形を足すときも手順 1 の参照表に行を足してから実装する。

### 4. target と診断

| 状況 | 結果 |
| --- | --- |
| Phase の外の native の target（Phase 1 の間は Linux と macOS） | `E1008`、kind 案 `abi_record_by_value_target`、位置は extern の名前。文案 `extern 'pt_in_rect' passes the '@c_layout' record 'Point' by value, which this compiler supports on Windows x64 and Arm64 only; declare the parameter 'ref Point' if the C function takes a pointer` |
| `--target wasm32`・`wasm64` | `E1008`、kind `abi_native_only`（W13・W15・W21 と共有） |
| `check`・LSP | 受け付ける（target を知らない）。誤りは `build` で出る |

- 誤りの出し方は、W21 が使う `abi_native_only` の共有の口（`host_call` が native だけの形に出会ったら extern の関数の番号を `Globals` の集合に記録し、`emit_program` が関数の生成の後で最も小さい番号の extern の名前の位置に返す）に、kind を 1 つ足す。
- `--emit header`: リンク名の無い extern（ヘッダーに出る import）の値渡しは `tz_record_… sym(tz_record_… arg0)` の形で出す。C のプロトタイプは target に依らないので、どの target でも出す。
- `--emit bindings-*`: W21 の `c_layout_bindings_pending`（`E2000`）のまま。

### 5. 評価と複写

- 引数は今と同じく左から評価し、値渡しの record は評価した直後に呼び出しごとの一時領域へ書く。メモリーで渡す引数は、呼び出しごと・引数ごとに新しい複写で、呼び出し先が書き換えてもよい（Clang が `dead_on_return` を付ける形）。Tsuzuri の値は変わらない。
- 宣言の正しさ（C の側の構造体と一致すること）は今の extern と同じく `@trusted` の責任。

## 設計

### 検査（`src/abi.rs`・`src/check.rs`）

- `abi::RecordPassing { Pointer, ByValue }` と `abi::extern_record_passing(ty, types) -> Option<RecordPassing>`（`@c_layout` で `ref` なしなら `ByValue`、印のない record と `ref` は `Pointer`、record でなければ `None`）を足す。`host_call`・`header` と W14 の `c_signature` はこれで分ける。`out_result` は export と印のない record のためにそのまま残し、extern の結果では `extern_record_passing` を先に見る。
- D2 の予約（W21 の `c_layout_by_value_pending`）を外す。kind は W21 が登録したものを消すか残すかを、`src/diagnostic_kinds.rs` の規則（GUIDE §6.7）に従って決める。

### 分類（`src/llvm_byvalue.rs`、新規）

```rust
/// W36: how a `@c_layout` record crosses a C call by value on the target being emitted.
pub(crate) enum ByValue {
    /// One integer: `iN` (Win x64 1/2/4/8 bytes, Arm64 `i64` arguments and `i{8s}` results).
    Integer(u32),
    /// Arm64, 9..=16 bytes that are not an HFA: `[2 x i64]`.
    IntegerPair,
    /// Arm64 HFA: `[n x float|double]` as an argument (`alignstack(8)` on Linux), `{ float, ... }` as a result.
    Floats { double: bool, count: u8, align_stack: bool },
    /// System V (Phase 2): one or two eightbytes, each `i64`, `iN`, `double`, `float` or `<2 x float>`.
    Eightbytes(Vec<Eightbyte>),
    /// A copy in memory: a `ptr` to a fresh copy (Windows, Arm64), `ptr byval(<T>) align 8`
    /// (System V), or as a result the first parameter `ptr sret(<T>)`.
    Memory,
}

pub(crate) fn classify(layout: &CLayoutLeaves, platform: Platform, result: bool) -> Result<ByValue, Unsupported>;
/// System V counts registers over the whole call; the other targets classify each position alone.
pub(crate) fn classify_call(parameters: &[Position], result: Option<&CLayoutLeaves>, platform: Platform) -> Result<Vec<ByValue>, Unsupported>;
```

- `CLayoutLeaves { size, align, leaves: Vec<(offset, Leaf)> }`、`Leaf = Int(bits) | F32 | F64 | Pointer`。W21 の `c_layout_fields` を再帰で開いて作る（固定長配列は要素ごと）。ここ以外で大きさや葉を数えない。
- `Unsupported` は Phase の外の target（`abi_record_by_value_target`）と WASM（`abi_native_only`）。
- 型の文字列（`declare` と呼び出しの地点）は `ByValue::argument_types`・`result_type` の 1 か所で作る。LLVM 17 が読めない属性（`dead_on_return` など）は出さない。`sret(<ty>)`・`byval(<ty>)`・`alignstack(8)` だけを使う。

### 生成（`src/llvm_imports.rs`）

- `FunctionEmitter::by_value_slot(record, class)`: `alloca [k x i8], align 16`（`k` は record の大きさと、分類した型の格納の大きさの大きい方。3 バイトの record を `i64` で読むと、W21 の一時領域だけでは `alloca` の外を読む）と `zeroinitializer` の `store`。
- `FunctionEmitter::by_value_to_abi(class, slot) -> Vec<(String, String)>`（メモリーの構造体から ABI の値へ）と `by_value_from_abi(class, values, slot)`（ABI の値からメモリーへ）を足す。呼び出し側は、引数で `write_host_record` → `by_value_to_abi`、結果で `by_value_from_abi` → `read_host_record` を使う（W26 の呼ばれる側は逆に使う）。
- `host_call`: 結果が `Memory` なら、引数より前に一時領域を作り `ptr sret(%tz_record_…) <slot>` を最初に置き、`void` を返す。引数の `Memory` は、Windows・Arm64 は `ptr <slot>`、System V は `ptr byval(%tz_record_…) align 8 <slot>`。ほかは `load` した値を渡す（`align` は record の整列）。宣言の行は `intrinsics`（宣言の行の文字列の集合）に入れ、属性も文字列に含めるので、同じ宣言は 1 つになる。
- `header`: 値渡しの結果を `void` にせず `tz_record_…` で出し、値渡しの引数を `tz_record_… argN` で出す（`host_abi::c_parameters` に `ByValue` の分岐）。

### W14 との関係

- W14 の `c_signature` に、`Integer(bits)` は W14 の `I32`（32 bit 以下）か `Word64`、`IntegerPair`・`Floats`・`Eightbytes` は新しい分類、`Memory` の引数は `Ptr`（Windows・Arm64）か新しい `ByvalMemory`（System V）、`Memory` の結果は先頭の新しい `Sret` を足す。`Sret` は W21 の out ポインター（先頭の `Ptr`）と別の分類にする（Arm64 でレジスターが違う）。

## 前提とする他チケットのインターフェース

- [W21](W21-c-layout-records.md): `CheckedRecord.c_layout`、`abi::c_layout_record`・`abi::host_record`、`llvm_abi::record_layout`・`c_layout_fields`、`@c_layout` の `read_host_record`・`write_host_record`（packed struct の境界の型、葉の整列）、Phase 2 の field の位置の規則と kind、`abi_native_only` の共有の口。D2 の予約（`c_layout_by_value_pending`。W21 の改訂の提案）。
- [W14](_completed/W14-extern-multiple-declarations.md)（done なら）: `abi::CType`・`AbiClass`・`c_signature`。

## 他チケットへ提供するインターフェース

- `llvm_byvalue::{ByValue, classify, classify_call}` と `FunctionEmitter::{by_value_slot, by_value_to_abi, by_value_from_abi}`。
- [W25](W25-extern-interface-com.md) Phase 3: メソッドの値渡しの引数（`D2D1_POINT_2F`）は `host_call` の共有でそのまま使える。W25 の仕様 9 と D10 の値渡しの部分は、このチケットへの参照に置き換えてあり、W25 Phase 3 の着手条件にこのチケットが入っている。
- [W26](W26-com-objects-in-tsuzuri.md) Phase 3: `IDropTarget::DragEnter` の `POINTL`（呼ばれる側）は、コールバックの wrapper で `classify` と `by_value_from_abi`（引数）・`by_value_to_abi`（結果）を使う。呼ばれる側の wrapper の変更は W26 の範囲。
- [W27](W27-win32-package-from-winmd.md): win32metadata のポインターでない構造体の引数・結果を `ref` の無い `@c_layout` record に写す。
- [W14](_completed/W14-extern-multiple-declarations.md): 上の分類。

## 実装手順

### Phase 1

1. **変更**: `tests/c_layout_by_value_reference.c`（新規）。**内容**: 「現状」の参照表の構造体と関数を C に書き、CI と同じ LLVM（21.1.8）の Clang で 9 つの triple の `-S -emit-llvm` を取り、`target/scratch/w36/reference/` に保存する。表と違う行があれば表を直してから進める（版の差の記録）。**確認**: Windows の 2 つの triple で、`-gnu` と `-msvc` が同じ。
2. **変更**: `src/abi.rs`・`src/check.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/E1008.md`。**内容**: `RecordPassing`、予約の解除、`abi_record_by_value_target`。**確認**: T0、`tests/c_layout_by_value.rs` の受理と kind。
3. **変更**: `src/llvm_byvalue.rs`（新規）・`src/llvm.rs`（`mod`）。**内容**: 分類と型の文字列。**確認**: 単体テスト（参照表の Windows の列を手で写した表。出力から写さない）。
4. **変更**: `src/llvm_imports.rs`・`src/llvm_abi.rs`（`c_parameters`）・W14 の `c_signature`（あれば）。**内容**: 生成とヘッダー。**確認**: `tests/c_layout_by_value.rs` の IR とヘッダーの検査。基準との比較（S3）。2 回作って同じ。
5. **変更**: `tests/c_layout_by_value.mjs`・`tests/c_layout_by_value_host.c`・`tests/fixtures/c_layout_by_value/Main.tz`・`tests/windows.mjs`・`.github/workflows/windows.yml`。**内容**: テスト計画の E2E と参照との突き合わせ。**確認**: Developer PowerShell と Git Bash（同梱 MinGW）で `node tests/c_layout_by_value.mjs target/debug/tsuzuri.exe`。
6. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full` を付け、`windows.yml` の x64・Arm64 で E2E が通る。

### Phase 2（人間が求めたとき）

7. **変更**: `src/llvm_byvalue.rs`・`src/llvm_imports.rs`。**内容**: System V（`classify_call` のレジスターの数え方、`byval`）と AAPCS64（`alignstack(8)`）。Phase の外の target が無くなるので、`abi_record_by_value_target` が届かなくなる（kind の扱いは手順 2 と同じ規則）。`--backend fast` が `sret`・`byval`・集成体の引数を翻訳できなければ、既存の `E2002`（`fast_backend_unsupported_construct`）になることを確かめる。**確認**: `targets.yml` の Linux・macOS で E2E と参照との突き合わせ。

## テスト計画

### Rust（`tests/c_layout_by_value.rs`、新規）

| テスト | 内容 | 期待（参照表と ABI の文書から手で書く） |
| --- | --- | --- |
| `windows_x64_passes_by_size` | 1・2・3・4・5・6・8・12・16・24 バイトの record（数・`f32`・`f64`・ハンドル） | 1・2・4・8 は `i8`〜`i64`、ほかは `ptr`。結果は同じ大きさなら `iN`、ほかは `ptr sret(…)` と `void` |
| `windows_arm64_passes_hfa_in_float_registers` | `{f32 ×1..5}`、`{f64 ×1..4}`、`{f32, f64}`、入れ子と配列の HFA、`pack = 1` の HFA | 4 個までの HFA は `[n x float]`・`[n x double]`、5 個と混合は整数の形かメモリー。結果は `{ float, … }` |
| `windows_arm64_small_results_are_exact` | 3・5・6・7 バイトの結果 | `i24`・`i40`・`i48`・`i56`（引数は `i64`） |
| `memory_copies_are_fresh` | 同じ値を 2 回、同じ呼び出しの 2 つの引数に渡す | 一時領域が別々、`sret` が先頭、`dead_on_return` を出さない |
| `slots_cover_the_coerced_size` | 3 バイトの record を Arm64 の `i64` で渡す | `alloca [8 x i8], align 16` 以上 |
| `other_targets_reject` | `--target x86_64-unknown-linux-gnu`・`aarch64-apple-darwin`（Phase 1）、`wasm32` | `E1008`（`abi_record_by_value_target`・`abi_native_only`）。`check` は 0 |
| `unchanged_forms` | 印のない record の `R`、`ref R`、`export def` の `@c_layout` | 基準と同じ `ptr`・out 引数・ヘッダー |
| `header_prints_values` | リンク名の無い extern の値渡し | `tz_record_… sym(tz_record_… arg0)` |
| `blocking_wrap_finds_the_callee` | spawn するプログラムで `sret` と集成体の結果の extern を呼ぶ | `with_blocking_externs` が両方の呼び出しを包む |

### E2E（`tests/c_layout_by_value.mjs`、新規。`-O0`・`-O3`）

- C のホスト `tests/c_layout_by_value_host.c` が、参照表の各構造体を値で取る関数（各 field の値を検査して、field から作った値を返す。メモリーで受けたものは書き換える）と値で返す関数を定義する。field の値は境界の値（`-128`・`255`・`-32768`・`65535`・`-0.0`・NaN の bit 列・ハンドルの 64 bit の値）を混ぜる。期待値は C の側で書く。
- Tsuzuri の側は fixture から呼び、結果の各 field と、メモリーで渡した record が呼び出しの後も変わらないことを検査する。
- レジスターの割り当て: 整数・浮動小数点・構造体を混ぜた 9 引数の関数（参照表の `many` の形。Windows x64 の 5 番目以降のスタックの引数、Arm64 の浮動小数点レジスターが尽きた HFA）。
- 参照との突き合わせ: fixture の extern と同じプロトタイプの C を、同じ target で `clang -S -emit-llvm` にして、`declare` の引数と結果の型を比べる（属性 `noundef` などと `dso_local` を除き、`ptr` と `i64`（`[2 x ptr]` と `[2 x i64]`）だけを同じと見なす。名前付きの struct 型は要素の並びに直す）。Clang の出力は独立した参照（GUIDE-LITE §2 規則 10）。
- Windows（`tests/windows.mjs`）: 早見の `PtInRect`（1 を返す）。
- 実行: Windows x64（`windows-2022`）と Arm64（`windows-11-arm`）で実行、ほかの Windows の triple はリンクまで。Phase 2 は Linux・macOS の `targets.yml`。

### 変えないことの確認

- 着手条件で保存した基準と、すべての `tests/fixtures`・`examples` の IR とヘッダーを byte 単位で比べる。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 「ホスト関数を import する」に値渡しの節（`R` は C の値渡し、`ref R` は `const T *`、対応する target、`PtInRect` の例、印のない record は今どおりポインターであること、Phase の外の `E1008`）。W21 が書いた `i64` に詰める回避策の文を消して、この節を指す。
- `_tsuzuri/language-reference/built-in-types-and-modules/record.md`: W21 の「C の配置（`@c_layout`）」の節に、extern での値渡し。
- `_tsuzuri/language-reference/compiler/diagnostics.md`（生成の部分は `node scripts/gen-docs.mjs`）と `src/explain/codes/E1008.md`。
- `_tsuzuri/language-reference/compiler/platforms.md`: target ごとの対応（Phase 1 は Windows だけ）。
- `docs/language.md` の「ホスト関数のインポート」（461 行付近の「レコード型の受け渡しには、正規化された一時構造体が使用されます」に `@c_layout` の値渡しの例外）。`docs/architecture.md` の「ホスト境界」（分類の 1 か所、参照の Clang との突き合わせ）。
- `README.md` の「変更の種類別」の host ABI の行に `tests/c_layout_by_value.rs`・`.mjs`。

## 受け入れ条件

- [ ] 仕様 1・2・4・5 のとおりに動き、E2E が Windows x64 と Arm64 の `-O0`・`-O3` で通る（Phase 2 は仕様 3 と Linux・macOS）。
- [ ] Windows の 4 つの triple で、fixture のすべての extern の `declare` が参照の Clang と一致する。
- [ ] 値渡しを使わないプログラムの IR・ヘッダー・診断が基準と byte 単位で一致する。宣言が重複せず、IR が決定的。
- [ ] 新しい診断コードを足さず、kind は `abi_record_by_value_target`（新しい）と `abi_native_only`（共有）。`diagnostics.md` と `docs/language.md` に載った。
- [ ] 文書を直し、`check-docs`・`gen-docs --check`・`check-cli-docs` が通る。PR に `ci:full` を付けた。

## 落とし穴

- **`sret` の属性を落とさない**。Arm64 の `sret` は `x8`、普通の先頭のポインターは `x0`（「現状」）。x64 の Windows はどちらも `rcx` なので、x64 だけのテストでは誤りが見えない。Arm64 の CI で必ず実行する。
- **一時領域の大きさ**。Arm64 は 3 バイトの record を `i64` で渡す。record の大きさの `alloca` から `i64` を `load` すると範囲外の読み出し（未定義動作）になる。
- **結果の整数の幅**。Arm64 の 8 バイト以下の結果は `i{8s}`（`i24`・`i40`）で、引数の `i64` と違う。
- **Windows x64 に HFA は無い**。`{float x, y}` は `i64` で整数レジスター。
- **メモリーで渡す引数は毎回の新しい複写**。呼び出し先は書き換えてよいので、Tsuzuri の値のアドレスや、ほかの引数・ほかの呼び出しと共有する一時領域を渡さない。
- **System V のレジスターの数え方**（Phase 2）。構造体ごとに分類するだけでは足りない。残りが足りない構造体は全部メモリーに回り、後ろのスカラーはレジスターに入る。`sret` は整数レジスターを 1 つ使う。整列していない field（`pack`）はメモリー。
- **Linux の AAPCS64 の `alignstack(8)`**（Phase 2）は宣言と呼び出しの地点の両方に付ける。macOS と Windows には付けない。
- **LLVM の版**。`dead_on_return`・`dead_on_unwind` は新しい LLVM の属性で、IR を読む Clang の版（LLVM 17 以上）によっては拒まれる。ABI に要らないので出さない。
- **W21 の out ポインターと混ぜない**。`export def` の `@c_layout` の結果と印のない record の extern の結果は今の先頭の out 引数で、`sret` ではない（Arm64 でレジスターが違う）。
- **C++ のメンバー関数**。MSVC は小さい構造体でも `this` の後の隠れたポインターで返す。W25 の規則で、このチケットの分類を使わない。

## 対象外

- `export def` の値渡し（ヘッダーが ABI を決めるので誤りが起きない。E13 のバインディングはポインターの形に立つ）。コールバック・`FnPtr` の引数と結果の値渡し（呼ばれる側。W26 Phase 3 がこのチケットの分類を使う）。
- MSVC の C++ のメンバー関数の record の結果（W25 Phase 3）。可変長引数。`__vectorcall`・`__m128` などのベクトルの型。
- 印のない record の値渡し（配置が C でないので値渡しの意味が無い。E05 のまま）。32-bit の target。WASM。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `extern def` の引数と結果の `ref` の無い `@c_layout` record を C の値渡し・値返しにする。`ref R` は `const T *` のまま。E05 の「by-value の分類を IR で再現しない」と W21 D4・仕様 4 を、`@c_layout` の extern の位置に限って改める | C の `T` と `const T *` の区別に、Tsuzuri の `R` と `ref R` がそのまま対応する（Rust の `#[repr(C)]`、C# の P/Invoke と同じ）。印のない record は配置が C でないので E05 のまま。W27 の生成器は「ポインターでない構造体は `R`」と写せる。退けた案: (a) 宣言の前の属性（`@by_value` など）は D-53 の改訂が要り、付け忘れると今と同じく黙ってポインターを渡す（「現状」の再現の形）。(b) 包む型 `ByValue<R>` は D-55 の登録が要り、すべての署名が長くなり、付けない `R` がポインターのまま残る。(c) `i64` に詰める回避策は Arm64 と System V の浮動小数点の構造体で誤る（参照表の `D2D1_POINT_2F`） | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | W21 の Phase 1 は、`ref` の無い `@c_layout` record を extern の引数と結果で `E1008`（kind 案 `c_layout_by_value_pending`、`ref R` か `ref mut R` を促す文）にして、このチケットのために綴りを空けておく | W21 の仕様 4 のまま出荷すると、このチケットが既存のプログラムの C の ABI を黙って変える（S1）。予約すれば、W21 の間に値渡しの C の関数を誤って宣言したプログラムも診断で止まる。W21 のチケットの改訂なので、承認が要る | 要承認（W21 の改訂。承認と W21 の実装の状態を着手条件で確かめる） |
| D3 | Phase 1 は Windows x64 と Arm64（MSVC・MinGW）、Phase 2 は System V x86-64 と AAPCS64。Phase の外の native は `E1008`（`abi_record_by_value_target`）、WASM は `E1008`（`abi_native_only`） | D-63 の Win32 が先。Windows の規則は位置ごとに決まり、レジスターを数えない。W25 Phase 3 は Windows だけを要る。WASM の C の ABI は扱わない（W01〜W30 の共通の規則） | 既定案（実装者はこの案に従う） |
| D4 | 分類は `src/llvm_byvalue.rs` の 1 か所で、W21 の配置の 1 か所（`c_layout_fields`・`record_layout`）と target から求める。target ごとの規則は、参照の Clang の `-S -emit-llvm` と突き合わせるテストで確かめる | LLVM IR は値渡しを下げないので、フロントエンドが Clang と同じ分類をするしかない。E05 が恐れた「再現の誤り」は、独立した参照との機械的な突き合わせで捕まえる | 既定案（実装者はこの案に従う） |
| D5 | メモリーで返す結果は `sret` の属性付きの先頭の引数にし、W21 の out ポインターの規則と混ぜない | Arm64 では `sret` が `x8`、out ポインターが `x0`（測定） | 既定案（実装者はこの案に従う） |
| D6 | `export def` とコールバックは変えない。呼ばれる側の下げ方（`by_value_from_abi`・`by_value_to_abi`）は提供し、使うのは W26 | `export def` はヘッダーが ABI を決めるので誤りが起きず、E13 のバインディングもポインターの形に立つ。呼ばれる側の値渡しの要る API（`IDropTarget`）は W26 の範囲 | 既定案（実装者はこの案に従う） |
| D7 | メモリーで渡す引数は呼び出しごと・引数ごとの新しい複写（`align 16`）。一時領域は分類した型を読み書きできる大きさにする | 呼び出し先は複写を書き換えてよい（Clang の `dead_on_return`）。Arm64 の 3 バイトの record の `i64` の読み出しが範囲外にならない | 既定案（実装者はこの案に従う） |
| D8 | 新しい診断コードを割り当てない。`E1008` の新しい kind `abi_record_by_value_target`（起票時の W25 D10 が案にした名前。W25 の Phase 3 のメソッドも同じ kind を使う）と共有の `abi_native_only` | D-51、E12 D12 の前例 | 既定案（実装者はこの案に従う） |
| D9 | リンク名の無い extern のヘッダーは値渡しを C の値で出す（target に依らない） | C のプロトタイプは target に依らず、ホストの C コンパイラが同じ分類をする | 既定案（実装者はこの案に従う） |
