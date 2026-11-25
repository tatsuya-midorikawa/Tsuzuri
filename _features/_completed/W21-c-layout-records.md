# W21: C レイアウトの record（`@c_layout`）

| 項目 | 内容 |
| --- | --- |
| ID | W21 |
| 優先度 | P1 |
| 規模 | XL（Phase 1〜3。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | W13, W15, W18（Phase 2: W20, W22） |
| 後続 | W24, W25, W27, W36 |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E05 の正規化の例外、E12 D5・D11、D-53）。決定事項 D1・D3・D7 が要承認で、ほかは既定案 |
| 手本にする既存実装 | `src/llvm_abi.rs` の `record_layout`・`type_definitions`・`header_types`・`read_host_record`・`write_host_record`（境界の一時構造体）、`src/abi.rs` の `scalar_record`（field の検査）、`src/parser.rs` の `attributes`・`cpu_attribute`（宣言の前の属性。D-53）と `program.trusted`（宣言の名前で引く脇のリスト。E15）、`src/llvm_layout.rs` の `record_slots`（内部の並べ替え。変えない） |
| 主な影響ファイル | `src/parser.rs`・`src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT`）・`src/formatter.rs`・`src/docgen.rs`・`src/api_diff.rs`・`src/check.rs`（`CheckedRecord`）・`src/abi.rs`・`src/llvm_abi.rs`・`src/llvm.rs`（ヘッダーの冒頭、WASM の検査）・`src/llvm_imports.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/E1008.md`・`src/driver.rs`（Phase 1 のバインディングの拒否）・`src/bindings.rs`・`src/runtime/bindings-core.mjs`・`src/bindings_native.rs`（Phase 3）・`tests/c_layout.rs`（新規）・`tests/c_layout.mjs`（新規）・`tests/c_layout_host.c`（新規）・`tests/fixtures/c_layout/`（新規）・`tests/windows.mjs`・文書 |

## 実装者向け早見

- 範囲: record の宣言の前に `@c_layout`（または `@c_layout(pack = N)`）を書くと、その record がホスト境界を越えるときの C の構造体を、正確な幅（8・16 bit を 32 bit に広げない）、宣言の順、C の整列と詰め物で作る。Phase 1 の field は、数・数の固定長配列・入れ子の `@c_layout` record・`extern type copy` のハンドル（[W18](_completed/W18-copy-extern-type.md)）・`Maybe<H>`（[W15](_completed/W15-abi-nullable-handles.md)）。Phase 2 で `FnPtr<F>`（[W22](_completed/W22-fnptr-values.md)）と `ref ZString`・`Maybe<ref ZString>`（[W20](_completed/W20-zstring.md)）、Phase 3 で [E13](_completed/E13-host-bindings.md) の生成器（C#・Python・C++）と JS の糊。`ref mut R` の copy-in/copy-out は [W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md) の経路をそのまま使う。共通の決定は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)。
- 最初に読む: ① 仕様 2〜4 ② `src/llvm_abi.rs` の `record_layout`（641 行付近）と `read_host_record`・`write_host_record`（728〜810 行付近）③ `src/abi.rs` の `scalar_record` ④ 「現状」の再現（LogFont が 180 バイトになる）。
- 最初に書くテスト: `tests/c_layout.rs` の `dlg_template_packs_to_18_bytes`（`--emit header` の文字列と、IR の境界の型 `<{ i32, i32, i16, i16, i16, i16, i16 }>`）→ `tests/c_layout.mjs` の C ホスト。
- 触ってはいけない: `@c_layout` のない record の内部の配置・境界の構造体・IR・ヘッダー（1 byte も変えない）。`abi_type`・`c_type`（引数とスカラーの結果の 32 bit への正規化は C の呼び出し規約で無害なので、そのまま）。`record_slots`（内部の並べ替え）。`ABI_VERSION`（1 のまま）。
- 完了の見分け方: 次のプログラムの `--emit header` を `windows.h` と一緒にコンパイルした C で、`sizeof(tz_record_4Main_7LogFont) == sizeof(LOGFONTW)`（92）と `sizeof(tz_record_4Main_11DlgTemplate) == sizeof(DLGTEMPLATE)`（18）が成り立ち、ヘッダー自身の静的表明も通る。

```tsuzuri
@c_layout
record LogFont {
    height: i32, width: i32, escapement: i32, orientation: i32, weight: i32,
    italic: i8u, underline: i8u, strike_out: i8u, char_set: i8u,
    out_precision: i8u, clip_precision: i8u, quality: i8u, pitch_and_family: i8u,
    face_name: [i16u; 32]
}

@c_layout(pack = 2)
record DlgTemplate { style: i32u, extended_style: i32u, cdit: i16u, x: i16, y: i16, cx: i16, cy: i16 }

@trusted
extern "CreateFontIndirectW" def create_font :: ref LogFont -> i64

// DLGTEMPLATE は可変長の部分が後ろに続くので、extern の一時構造体だけを渡す API はない。ヘッダーに出すために export で使う。
export def dialog_items :: ref DlgTemplate -> i16u = \template -> template.cdit
```

## 目的

Win32 の構造体の多くは `BYTE`・`WORD`・`WCHAR`・`BOOL` とハンドルを混ぜて持つ（`LOGFONTW`・`DLGTEMPLATE`・`WNDCLASSEXW`・`MSG`・`GUID`）。今の record の ABI は、[E05](_completed/E05-host-abi-buffers.md) の決定で 8・16 bit と `bool` の field を 32 bit に広げ、ハンドルの field を拒否する（[E12](_completed/E12-ffi-extensions.md) D5）ので、これらを Tsuzuri だけで宣言できない。既定の配置を変えると既存の C・JS・C#・Python・C++ のホストが黙って壊れる。そこで、record ごとの明示の印で C の配置を選べるようにし、印のない record は 1 byte も変えない。D-63 の `extern view`（[W24](W24-extern-view.md) Phase 2）と `extern interface`（[W25](W25-extern-interface-com.md)）は、この C レイアウトの record の上に立ち、[W27](W27-win32-package-from-winmd.md) の生成器は win32metadata の構造体をこの形に写す。

## 着手条件と停止条件

### 着手条件

- Phase 1: W13・W15・W18 が done（`ref mut R` の copy-in/copy-out、`Maybe<H>` の境界の符号化、`extern type copy`）。決定事項 D1・D3・D7 の承認。
- Phase 2: Phase 1 が done、W20・W22 が done。
- Phase 3: Phase 2 が done（Phase 1 の後でも、Phase 2 の field を生成器の対象外にすれば始められる。始める前に人間に確かめる）。
- 共通: `node scripts/repo-kb.mjs brief W21`。着手時の `FRONTEND_FORMAT`（`src/frontend_cache.rs`、起票時 20）、`ABI_VERSION`（`src/bindings.rs`、起票時 1）、`DiagnosticKind` の個数を控える（GUIDE-LITE §11）。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` のすべてについて、`main` のコンパイラで `--emit llvm -O1`（native と `--target wasm32`）と、`export` か `extern` を持つものの `--emit header` を `target/scratch/w21/base/` に保存する（TZ05 の比べ方）。

### 停止条件

- **S1** `@c_layout` を使わないプログラムの IR・ヘッダー・診断が、保存した基準と 1 byte でも違う。
- **S2** C コンパイラ（MinGW の clang と MSVC 環境の clang）の `sizeof`・`offsetof` と、このチケットの配置の規則（仕様 3）が食い違う構造体が見つかった。規則を即興で直さず、構造体と 2 つの値を記録して報告する。
- **S3** W13・W15・W18（Phase 2 は W20・W22）の実装が「前提とする他チケットのインターフェース」と違い、意図を保って合わせる方法が 1 つに決まらない。
- **S4** 内部の配置（`record_slots`、`llvm_type` の record の型）を変えないと実装できないように見える。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 境界の構造体は 8・16 bit と `bool` を 32 bit に広げる

- `src/llvm.rs` の `abi_type` は `Integer(8 | 16 | 32, _)` と `Bool` を `i32` に、`c_type` は整数を `int{max(bits, 32)}_t` に、`bool` を `int32_t` にする。どちらも `module` を受け取らない。
- `src/llvm_abi.rs` の `record_layout` は、64-bit の数でないスカラーをすべて `(4, 4)` にする（固定長配列は要素の大きさ × 長さ）。`record_field_type`・`type_definitions`（境界の LLVM の型 `%tz_record_*`）・`header_types`（C の `typedef struct`）は field ごとに `abi_type`・`c_type` を使う。`read_host_record`・`write_host_record` は field ごとに `decode_host_scalar`・`encode_host_scalar`（`trunc`／`sext`・`zext`、`bool` は `icmp ne`／`zext`）で広げ縮める。
- E05（`_features/_completed/E05-host-abi-buffers.md` の「scalar-only records」）は「i8/i16/bool field は ABI struct 内で i32 に正規化する。これは internal LLVM record layout と C ABI record layout を一致させない設計である」と決め、「C struct を by value で返すと platform ABI classification を LLVM IR 側で正確に再現する必要が出る」ので record をポインターでだけ渡す。
- 再現 1（LogFont が 180 バイトになる）: 早見の `LogFont` から `@c_layout` を外し、`export def font_height :: ref LogFont -> i32 = \f -> f.height` と `def main :: unit -> i32 = \() -> 0` を足した `Main.tz` を `target/scratch/w21/logfont/` に置き、Git Bash で次を実行すると `180 92` を出した（ヘッダーは `uint32_t italic; ... uint32_t face_name[32];`）。

```sh
T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain
D=target/scratch/w21/logfont
target/debug/tsuzuri.exe build $D --emit header -o $D/out.h
cat > $D/check.c <<'EOF'
#include <windows.h>
#include <stdio.h>
#include "out.h"
int main(void) { printf("%zu %zu\n", sizeof(tz_record_4Main_7LogFont), sizeof(LOGFONTW)); return 0; }
EOF
$T/bin/tsuzuri-clang.exe $D/check.c -o $D/check.exe && $D/check.exe
```

- 再現 2（境界の型と内部の型）: `record Font { height: i32, italic: i8u, weight: i16, flag: bool, face_name: [i16u; 4] }` を `export def font_height :: ref Font -> i32` で使い `target/debug/tsuzuri.exe build <dir> --emit llvm -o out.ll` とすると、境界の型が `%tz_record_4Main_4Font = type { i32, i32, i32, i32, [4 x i32] }`、内部の型が `%tz.record.Main.Font = type { i32, i16, [4 x i16], i8, i1 }` だった（内部は並べ替え済み）。

### 内部の並べ替えは境界から切り離されている

- `src/llvm_layout.rs` の `record_slots` は利用者の record の field をアライメントの大きい順に並べ替え（小さくなるときだけ）、すべての読み書きが `field_slot` を通る。`read_host_record`・`write_host_record` は宣言順の境界の型の各 field を `field_slot` で内部の位置に写す。したがって境界の配置だけを変えれば、内部の並べ替えはそのまま使える。

### ハンドルの field は拒否される

- `src/abi.rs` の `scalar_record` は、field が `exportable`（8〜64 bit の整数・`f32`・`f64`・`bool`）か、その固定長配列（長さ 1 以上）か、入れ子のスカラーの record（`Drop` を持たない）であることを求める。E12 D5 は「ABI の record の field・buffer の要素・`ref mut H` は不可。理由: record の layout（native 8 bytes、wasm32 4 bytes）と JS glue を変える必要がある」、D11 は「ハンドルを含む record は従来どおり E1008」と決めている。
- 再現: `extern type Hwnd` と `record Msg { hwnd: Hwnd, message: i32u }` を `@trusted extern "GetMessageW" def get_message :: ref Msg -> i32` に使った `Main.tz` を `target/debug/tsuzuri.exe check <dir>` にかけると `error[E1008]: extern signatures support scalar/unit, shared ABI buffers and scalar records, ...`（`src/check.rs` の `UnsupportedExternSignature`）。
- `src/llvm_abi.rs` の `handle_typedefs` は関数の署名だけを見て record の field に降りず、明示のリンク名の extern を除く。一方 `record_types` は明示のリンク名の extern の record も含む（ヘッダーに `typedef` が出る）。

### record の前の属性はない

- `src/parser.rs` の `attributes` が読むのは `@cpu`・`@no_alloc` などの効果の属性・`@trusted` だけ。再現: `@c_layout` を `record Point { x: i32, y: i32 }` の前に書いた `Main.tz` を `target/debug/tsuzuri.exe check <dir>` にかけると `1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression`。`RecordDecl`（`src/syntax.rs`）は `derives` を持つが属性を持たない。

### 生成器と JS の糊は広げた配置を前提にする

- `src/bindings.rs` の `fields` は `record_layout` で field のオフセットを求め、`ABI_VERSION = 1`。`src/runtime/bindings-core.mjs` の `scalar` は 8・16 bit の型を `getInt32`／`getUint32` で読み書きする。`src/bindings_native.rs` の C# の record は「bool and 8/16-bit integers are stored as 32-bit values」と書き、`csharp_abi`・`ctypes` は 8・16 bit を 32 bit の型にする。

### 32/64-bit の field だけの record は、今でも C と同じ配置

- `NMHDR`（ハンドルを `i64`）・`MINMAXINFO`（`POINT` 5 つ）・`CREATESTRUCTW`（ポインターを `i64`）の形の record をヘッダーに出し、`windows.h` の構造体と `sizeof`・`offsetof` を `_Static_assert` で比べた C が、MinGW と MSVC 環境の clang の両方でコンパイルできた（24・40・80 バイト）。手順は W24 の「現状」の再現 3。W24 の Phase 1 はこれに立つ。

### 確かめた C の配置（x86-64。MinGW の `tsuzuri-clang.exe` と MSVC 環境の `C:\Program Files\LLVM\bin\clang.exe` で同じ値）

`<windows.h>`・`<commdlg.h>`・`<stddef.h>` を `#include` し、各構造体の `sizeof`・`_Alignof` と下の `offsetof` を `printf` する C を、Git Bash で `$T/bin/tsuzuri-clang.exe sizes.c -o sizes.exe`（MinGW）、PowerShell で `Enter-VsDevShell`（MSVC の開発者シェル。`-DevCmdArguments "-arch=x64 -host_arch=x64"`）の後に `& "C:\Program Files\LLVM\bin\clang.exe" sizes.c -o sizes_msvc.exe`（MSVC）としてコンパイルし、実行した。

| 構造体 | 大きさ | 整列 | メモ |
| --- | --- | --- | --- |
| `LOGFONTW` | 92 | 4 | `lfFaceName` は 28 |
| `DLGTEMPLATE` | 18 | 2 | `winuser.h` が `#pragma pack(2)` で宣言。`cdit` は 8、`x` は 10 |
| `WNDCLASSEXW` | 80 | 8 | ハンドル 5 つ、関数ポインター、文字列ポインター 2 つ |
| `OPENFILENAMEW` | 152 | 8 | |
| `GUID` | 16 | 4 | `{u32, u16, u16, u8[8]}` |
| `MSG`・`PAINTSTRUCT`・`RECT` | 48・72・16 | | |

- aarch64 の Windows と Linux・macOS の配置は確かめていない（未確認。数とポインターだけの構造体は同じ規則のはずで、テスト計画で確かめる）。

### target の情報

- `check` は target を取らない（`tsuzuri check --target wasm32 <dir>` は `E2000`）。target に依る検査は `build` の生成（`src/llvm.rs` の `emit_with_options` は `options.wasm` で `Diagnostic` を返せる）か、`src/llvm_layout.rs` の `WEBASSEMBLY`（スレッドに置いた target の印）で行う。ネイティブの target はすべて 64-bit（`src/target.rs` の `unknown_target_message` が 32-bit を拒否）。

## 変えてはいけない意味

- `@c_layout` のない record の内部と境界の配置、IR、ヘッダー、バインディングの表と生成物、診断。
- 引数とスカラーの結果の C の型（`abi_type`・`c_type` の 32 bit への正規化）。record をポインターで渡すこと（E05）。
- record の評価順・drop の順・`deriving`・表示・デバッガーに見える順（宣言順）。

## 仕様

### 1. 構文

```text
c_layout_attribute := "@c_layout" | "@c_layout" "(" "pack" "=" ("1" | "2" | "4" | "8" | "16") ")"
```

- `record`（`private record` を含む）の宣言の前にだけ書ける。D-53 の宣言の前の属性として、ほかの属性と任意の順に読む（record に付けられるほかの属性はないので、実際は単独）。
- `@c_layout` は Tsuzuri の型としては普通の record である。構築・field の読み出し・更新・パターン・`deriving` は今と同じ。違いはホスト境界で作る C の構造体だけ。
- どの edition（2026・2027）でも受ける。今はどの edition でも `E0002` になる書き方なので、意味の変わる既存のプログラムはない（GUIDE-LITE §7。`tests/editions.rs` で両方の edition を確かめる）。

### 2. field の型

| Phase | 許す field の型 | C の型 |
| --- | --- | --- |
| 1 | `i8`・`i8u`・`i16`・`i16u`・`i32`・`i32u`・`i64`・`i64u`・`f32`・`f64` | `int8_t`〜`uint64_t`・`float`・`double`（広げない） |
| 1 | 上の数の固定長配列 `[T; N]`（N ≥ 1） | `T name[N]` |
| 1 | 入れ子の `@c_layout` record | その `typedef` |
| 1 | `extern type copy` のハンドル `H`（W18） | `tz_handle_...`（ポインター） |
| 1 | `Maybe<H>`（H は extern type。W15 の共通契約のとおり） | 同じハンドルの型。`None` ⇔ `NULL` |
| 2 | `FnPtr<F>`（W22） | `R (*name)(A...)` |
| 2 | `ref ZString`・`Maybe<ref ZString>`（W20） | `const uint16_t *`（`None` ⇔ `NULL`） |

- `bool` は `E1008`（`c_layout_bool_field`）。Win32 の `BOOL` は `i32`、C の `_Bool` は `i8u` と書く（4 バイトか 1 バイトかを黙って選ばない。GUIDE §1 規則 2）。
- 裸のハンドルの field は `extern type copy` だけで、所有するハンドル（`copy` でない `extern type`）は `E1008`（`c_layout_owned_handle_field`）。`Maybe<H>` の field は、W15 の共通契約（`@c_layout` の field に `Maybe<H>`、H は extern type）のとおり所有するハンドルも受ける。そのような record は Copy でなく、copy-out（W13）と結果の読み出しはホストのメモリから所有するハンドルを作る（W15 の `Maybe<H>` の結果と同じ扱い。ハンドルの値を Tsuzuri が逆参照しないのでメモリ安全性は崩れないが、copy-out の上書きはハンドルを漏らしうる。決定事項 D7）。
- `char`・`f16`・`i128`・`decimal`・`string`・配列・リスト・タプル・ほかの共用体・関数・借用（Phase 1）・SIMD は `E1008`（`c_layout_field_type`）。`@c_layout` でない record の field は `E1008`（`c_layout_nested_record`）。
- 型引数・region を持つ record、field のない record、`instance Drop` を持つ record は `@c_layout` にできない（`c_layout_generic`・`c_layout_empty`・`c_layout_drop`）。
- 逆に、印のない record の field に `@c_layout` の record を置いたものは、境界（extern・export の署名）に届くと `E1008`（`c_layout_in_plain_record`。外の record にも `@c_layout` を付けるよう促す）。境界の外の普通の Tsuzuri の値としては今どおり使える。
- 検査は宣言の時点で行い、使われていない `@c_layout` record も検査する。

### 3. 配置の規則

宣言順の各 field について、自然な大きさ `s` と整列 `a` を決め、`pack` があれば `a = min(a, pack)` とし、オフセットを `a` の倍数に切り上げて置く。record の整列は field の整列の最大、大きさはそれの倍数に切り上げる。

- 自然な大きさと整列: 8 bit は 1・1、16 bit は 2・2、32 bit と `f32` は 4・4、64 bit と `f64` は 8・8、ハンドル・`Maybe<H>`・`FnPtr`・`ref ZString` は 8・8（ネイティブ）、固定長配列は要素の大きさ × N と要素の整列、入れ子の record はその大きさと整列（入れ子自身の `pack` で求めたもの。外の `pack` でさらに上限をかける）。
- これは MSVC と GCC・Clang の `#pragma pack(N)` と同じ規則で、ビットフィールドと `__declspec(align)` のない構造体について、x86-64・AArch64 の C の配置と一致する（停止条件 S2 で確かめる）。

### 4. 使える位置

| 位置 | Phase 1 の field だけの record | Phase 2 の field を持つ record |
| --- | --- | --- |
| extern の引数 `R`・`ref R` | 可（`const T *`。今と同じ一時構造体） | 可（入力） |
| extern の引数 `ref mut R`（W13） | 可（`T *`。copy-in、呼び出し、copy-out） | `E1008`（`c_layout_input_only`） |
| extern の結果 `R` | 可（先頭の out 引数 `T *out`） | `E1008` |
| `export def` の引数 `R`・`ref R`、結果 `R` | 可 | 引数・結果とも `E1008` |
| コールバックの引数・結果 | 今と同じく record は不可 | 同左 |
| `extern view`（W24 Phase 2） | 可 | `E1008`（W24 の決定） |

- 渡し方は今の record と同じポインターで、C の値渡し（8 バイト以下の構造体をレジスターで渡す分類）は再現しない（E05）。値渡しは [W36](W36-c-layout-by-value.md) が、`ref` の無い `@c_layout` record の extern の引数・結果の意味として足す。
- 未決（W36 の D2、要承認）: W36 は、このチケットの Phase 1 が上の表の「extern の引数 `R`」（`ref` なし）と「extern の結果 `R`」を `E1008`（kind 案 `c_layout_by_value_pending`、`ref R` か `ref mut R` を促す文）にして綴りを空けておくことを求めている。表のまま出荷すると、W36 が既存のプログラムの C の ABI を黙って変えるためである（W36 の S1）。W36 の D2 が承認されたら、この 2 行をそれに改めてから実装する。承認前は表のまま。

### 5. ヘッダー

- `@c_layout` record の `typedef struct` は、正確な C の型の field を宣言順に持つ。`pack` があれば `#pragma pack(push, N)` と `#pragma pack(pop)` で囲む。
- その後に、大きさと各 field のオフセットの静的表明を置く。C89・C++ のどちらでも通るように、長さが負になる配列の `typedef` を使う（`typedef char tz_record_4Main_11DlgTemplate__size[(sizeof(tz_record_4Main_11DlgTemplate) == 18) ? 1 : -1];`、field ごとに `typedef char tz_record_4Main_11DlgTemplate__f_cdit[(offsetof(tz_record_4Main_11DlgTemplate, cdit) == 8) ? 1 : -1];`）。名前は `record_name` に `__size` と `__f_<field_name>` を付ける。`record_name` の区切りは `_` と数字で始まるので `__` の後ろとは衝突せず、`size` という field があっても `__size` と `__f_size` は別の名前になる。`@c_layout` record が 1 つでもあるときだけ `#include <stddef.h>` と、冒頭の注記の次の行に `/* Records marked @c_layout use exact C field types, alignment and padding. */` を足す。
- `handle_typedefs` は、ヘッダーに `typedef` を出す record の field（入れ子を含む）のハンドルも集める（明示のリンク名の extern だけが使う record でも、ハンドルの `typedef` が要る）。

### 6. WASM

- 数・数の固定長配列・入れ子だけの `@c_layout` record は wasm32・wasm64 でも使え、配置は同じ規則（wasm32 の C の配置とも一致する）。
- ポインター幅の field（ハンドル・`Maybe<H>`・`FnPtr`・`ref ZString`）を持つ `@c_layout` record が境界に届いたプログラムは、`build --target wasm32|wasm64` で `E1008`（W15 の kind `abi_native_only`。位置はその record を使う extern か export の宣言）。`check` は target を取らないので、この誤りは `build` で出る。

### 7. 診断

| コード | kind（案） | 位置 | メッセージ（案） |
| --- | --- | --- | --- |
| `E0002` | `misplaced_c_layout_attribute` | `@c_layout` | `'@c_layout' applies to a record declaration` |
| `E0002` | `c_layout_repeated` | 2 つ目の `@c_layout` | `'@c_layout' is written twice; remove one` |
| `E0002` | `c_layout_argument` | 引数 | `'@c_layout' takes 'pack = 1', 'pack = 2', 'pack = 4', 'pack = 8' or 'pack = 16'` |
| `E1008` | `c_layout_bool_field` | field の型 | `field 'flag' of '@c_layout' record 'Font' is bool, whose C width is 4 bytes as BOOL and 1 byte as _Bool; declare it i32 or i8u` |
| `E1008` | `c_layout_owned_handle_field` | field の型 | `field 'font' of '@c_layout' record 'X' holds the owning handle 'Font'; declare the handle 'extern type copy' if it does not own a resource, or keep the owning handle outside the C struct` |
| `E1008` | `c_layout_field_type` | field の型 | `field 'name' of '@c_layout' record 'X' has type 'string', which has no C layout; use an exact-width number, a fixed-length array of numbers, another '@c_layout' record, an 'extern type copy' handle or 'Maybe' of an extern type` |
| `E1008` | `c_layout_nested_record` | field の型 | `field 'origin' of '@c_layout' record 'X' is the record 'Point', which has no '@c_layout'; add '@c_layout' to 'Point'` |
| `E1008` | `c_layout_generic`・`c_layout_empty`・`c_layout_drop` | record の名前 | 型引数・region、field なし、`Drop` を外すよう促す |
| `E1008` | `c_layout_in_plain_record` | extern・export の宣言 | `record 'Outer' has the '@c_layout' record 'Inner' as field 'inner'; add '@c_layout' to 'Outer' to pass it to the host` |
| `E1008` | `c_layout_input_only`（Phase 2） | 宣言の名前 | `'@c_layout' record 'WndClass' holds a borrowed ZString, which the host can only read; pass it as an extern argument 'WndClass' or 'ref WndClass'` |
| `E1008` | `fnptr_from_host`（W22。Phase 2） | 宣言の名前 | `FnPtr` の field を持つ record を、ホストから受け取る位置（結果・copy-out・`export` の引数）に置いた。文は W22 のもの |
| `E1008` | `abi_native_only`（W15） | extern・export の宣言 | ポインター幅の field を WASM で使えない旨 |
| `E2000` | `c_layout_bindings_pending`（Phase 1・2 だけ） | なし | `--emit bindings-js|cs|py|cpp does not support '@c_layout' records yet; use --emit header` |

## 設計

### 構文と AST

- `src/parser.rs` の `Attributes` に `c_layout: Option<CLayoutMark { pack: Option<u8>, span }>` を足し、`attributes` で読む（`cpu_attribute` と同じ形の `c_layout_attribute`）。`program_all` は、`@c_layout` の後が `record`（`private` を含む）でなければ `misplaced_c_layout_attribute`、`@trusted` と同じく宣言を読んだ後に脇のリスト `Program.c_layouts: Vec<CLayoutAttribute { target: Ident, pack: Option<u8>, span }>` へ積む（E15 の `program.trusted` の前例。`RecordDecl` は変えない）。
- `src/syntax_codec.rs` の `Program` の符号化に `c_layouts` を足し（`trusted` の後）、`FRONTEND_FORMAT` を着手時の値から 1 上げる。`src/formatter.rs` は `program.trusted` と同じく `target` を正規化し、整形の出力で属性を record の前に保つ。`src/docgen.rs` は record の見出しに `@c_layout` と `pack` を出す。`src/api_diff.rs` は、公開の record の `@c_layout`・`pack` の追加・削除・変更を互換のない変更として報告する（その record を使う export の C の ABI が変わる）。

### 検査（`src/check.rs`・`src/abi.rs`）

- `CheckedRecord` に `pub c_layout: Option<CLayout>`（`CLayout { pack: Option<u8> }`）を足す。record の field の型を解いた後、`@c_layout` の record ごとに仕様 2 の検査をする。ハンドルが `copy` かは W18 が真にする `Type::is_copy`、`Maybe<H>` の見分けは W15 の `src/abi.rs` の述語を使う。
- `src/abi.rs` に `c_layout_record(ty, types) -> bool`（`@c_layout` で、宣言の検査を通った record）と `host_record(ty, types) = scalar_record(ty, types) || c_layout_record(ty, types)` を足し、`parameter`・`result`、W13 の `extern_parameter`（`ref mut R` を含む）、`out_result` の呼び手が `host_record` を使う。`scalar_record` は、入れ子に `@c_layout` の record を持つ印のない record を偽にする（`c_layout_in_plain_record` を出す。今のプログラムには `@c_layout` がないので、今の判定は変わらない）ほかは意味を変えない（`@c_layout` で 32/64-bit の field だけの record は両方に当たるが、境界の型は `@c_layout` の規則で作る）。
- Phase 2 の field を持つ record の位置の検査（仕様 4 の右の列）は `UnsupportedExternSignature` の隣で行い、`FnPtr` の field を持つ record には W22 の `fnptr_from_host`、借用の `ZString` の field だけを持つ record には `c_layout_input_only` を出す（両方を持つなら `fnptr_from_host`）。`export def` の検査（`UnsupportedExportSignature` の所）も同じ。

### 配置の計算

- `src/llvm_abi.rs` の `record_layout(ty, module)` を、`@c_layout` の record では仕様 3 の規則で求めるように広げ、field ごとのオフセットも返す `c_layout_fields(ty, module) -> Vec<CField { offset, size, align }>` を足す。大きさ・整列・オフセットはこの 2 つだけが求め、LLVM の型・ヘッダーの静的表明・`bindings.rs` の表（Phase 3）・W24 の `host_pointer` の整列がすべてここから読む。ポインター幅は、ネイティブでは 8。WASM ではポインター幅の field を持つ record が届かない（仕様 6）。
- `abi_type`・`c_type` は変えない。field の側の型は、`module` を受け取る `record_field_type` と `header_types` の中で `@c_layout` の record のときだけ新しい関数 `c_layout_llvm_field`・`c_layout_c_field`（正確な幅、ハンドルは `ptr`／`tz_handle_...`）を使う。これで「印のない record は変えない」が構造上成り立ち、`module` を持たない呼び手の署名も変えない。

### 生成（`src/llvm_abi.rs`）

- 境界の LLVM の型は、詰め物を明示した packed struct にする: `%tz_record_4Main_4Pair = type <{ i8, [3 x i8], i32 }>`（`{ a: i8, b: i32 }`）、`DlgTemplate` は `<{ i32, i32, i16, i16, i16, i16, i16 }>`。末尾の詰め物も `[k x i8]` で置き、LLVM の型の大きさを C の大きさに一致させる。field の番号から struct の要素の番号への写像（詰め物を飛ばす）を `c_layout_fields` と一緒に求める。
- `read_host_record`・`write_host_record` は `@c_layout` の record のとき、写像した番号で GEP し、すべての `load`・`store` に、その葉が保証される整列を `align` で書く（packed struct の要素は既定で整列 1 と見なされるため）。保証される整列は、葉の実効の整列（`pack` を反映した `a`）と、外側のすべての record が与える上限の最小値である。再帰では引数 `cap`（最上位は record の整列）を渡し、入れ子の record の field に降りるとき `cap = min(cap, その field の実効の整列)` にする。例: `@c_layout(pack = 1) record Outer { tag: i8u, inner: Inner }` の `Inner` が `i32` の field を持つと、`inner` はオフセット 1 に置かれるので、その `i32` の `load` は `align 1`。数は幅が内部と同じなので変換しない（`decode_host_scalar`・`encode_host_scalar` を通さない）。固定長配列は内部も同じ要素型なので、配列ごと 1 回の `load`／`store` で写す。ハンドルは `ptr` をそのまま、`Maybe<H>` は W15 の符号化（`None` ⇔ `null`）で写す。
- 一時構造体（`host_result_slot`）は今と同じく `align 16` の `alloca` と `zeroinitializer` の `store` なので、詰め物は 0 になる。copy-out（W13）と結果の読み出しは詰め物を読まない。
- `type_definitions` の出し方の条件（`uses_host_abi`）は変えない。

### ヘッダー

- `header_types` は `@c_layout` の record について仕様 5 の形で出す。`#pragma pack` は `pack` のある record だけ。静的表明の名前は仕様 5 のとおり `<record_name>__size` と `<record_name>__f_<field_name>`（`field_name` は C の予約語を避けた名前）。
- `c_parameters` の `ref mut R` の `T *argN`（`const` なし）は W13 が足す。

### Phase 1・2 のバインディング

- `src/driver.rs` で `--emit bindings-js|bindings-cs|bindings-py|bindings-cpp` のとき、生成器が記述する署名（`src/bindings.rs` の `signatures` が集める export と import）に `@c_layout` の record が届けば `E2000`（`c_layout_bindings_pending`）。`--emit header`・`llvm`・`object`・`wasm`・`exe`・`shared` は使える。Phase 3 でこの拒否を外す。

### Phase 2（W20・W22 の後）

- `FnPtr<F>` の field: 内部の値（W22 の表現。`ptr @tz.callback.<name>` を持つ）を `ptr` として書く。ヘッダーは `R (*name)(A...)`（W22 の関数ポインターの C の型の書き方）。
- `ref ZString`・`Maybe<ref ZString>` の field: W20 が `ref ZString` の引数に使う「NUL 終端の UTF-16 の先頭のポインター」を書く。`None` は `null`。ポインターは呼び出しの間だけ有効で、借用は呼び出しの間生きている（record の借用 field と同じ規則）。
- これらの field を持つ record は入力の位置だけ（仕様 4）。copy-out・結果・`export` の引数・`extern view` の読み出しで、ホストが書いたポインターから `FnPtr`（W22 は外部の関数ポインターを受け取らない）や借用を作らないため。
- テストの例: `WNDCLASSEXW` を `lpfn_wnd_proc: FnPtr<(Hwnd -> i32u -> i64u -> i64 -> i64)>`、`class_name: ref ZString`、`menu_name: Maybe<ref ZString>`、ハンドルを `Maybe<Hicon>` などにして `RegisterClassExW` に渡す。

### Phase 3（E13 の生成器と JS の糊）

- `src/bindings.rs`: `@c_layout` の record の表の項目に `"c": true` と（あれば）`"pack": N` を足し、`fields` のオフセットを `c_layout_fields` から取る。ほかの record の項目は変えない。`ABI_VERSION` は上げない（表と糊は同時に生成され、印のない record の出力を変えないため）。
- `src/runtime/bindings-core.mjs`: `"c": true` の record の field は、`i8`・`i8u`・`i16`・`i16u` を `getInt8`・`getUint8`・`getInt16`・`getUint16`（書き込みも同じ幅）で読み書きする。印のない record の経路と生成される糊の文字列は変えない（`@c_layout` の record があるときだけ足す部分を `splice` で入れる）。
- `src/bindings_native.rs`: C# は `[StructLayout(LayoutKind.Sequential, Pack = N)]` と正確な型（`sbyte`・`byte`・`short`・`ushort`、固定長配列は正確な要素型の `InlineArray`）、ハンドルは既存のハンドルの `record struct`（`Maybe<H>` は `Value == 0` を `None`）。Python は `_pack_ = N` と `c_int8` などの正確な ctypes、ハンドルは `c_void_p`（`None` ⇔ `NULL`）。C++ は C ヘッダーの構造体をそのまま使う。Phase 2 の field の扱い（C# の関数ポインター、文字列の一時領域）はこの Phase で決め、決められないものは `E2000` のまま残して報告する。

## 前提とする他チケットのインターフェース

- W13（[W13](_completed/W13-extern-ref-mut-and-mutable-buffers.md) の「他チケットへ提供するインターフェース」）: extern の引数の判定 `abi::extern_parameter`（`ref mut R` を受ける）と、`host_call` の copy-out の表（一時構造体に `write_host_record` して呼び、呼んだ後 `read_host_record` して借用元へ書き戻す）。ヘッダーは `T *argN`。W21 は `extern_parameter` の record の判定を `host_record` にし、`@c_layout` の record を同じ copy-out の表に載せる。`read_host_record`・`write_host_record` を `@c_layout` に対応させれば、copy-out の経路には手を足さない。WASM の `abi_native_only` の共有の口（`host_call`・`wrapper` が WASM の生成で native だけの形に出会ったら extern の関数の番号を `Globals` の集合に記録し、`emit_program` が関数の生成の後で最も小さい番号の extern の名前の位置に `E1008` を返す。先に入ったチケットが作る）。
- W15（[W15](_completed/W15-abi-nullable-handles.md) の「他チケットへ提供するインターフェース」）: `abi::nullable(ty, types)`（`Maybe<H>` の見分け）と、`FunctionEmitter::nullable_to_c(ty, value)`（`Maybe` の値から C の `ptr`）・`nullable_from_c(ty, pointer)`（C の `ptr` から `Maybe<H>` の値）。W21 は `@c_layout` の `Maybe<H>` の field の読み書きにこの 2 つを呼ぶ。kind `abi_native_only` と上の共有の口（W21 はポインター幅の field を持つ `@c_layout` の record に出会ったら記録する）。
- W18（[W18](_completed/W18-copy-extern-type.md)）: `Type::Handle(name, HandleKind::Copy)` の判定（裸のハンドルの field はこれだけを受ける）と、それに対する `Type::is_copy` が真であること。値の表現は今の `extern type` と同じ `ptr`。
- W20（Phase 2。[W20](_completed/W20-zstring.md)）: `abi::zstring` と、`ref ZString` の下げ方（`units` の先頭のポインター 1 つ）。W21 の Phase 2 の `ref ZString` の field はこのポインターを書く。
- W22（Phase 2。[W22](_completed/W22-fnptr-values.md)）: `Type::FnPtr(Box<Type>)`、`abi::fnptr`、`FnPtr` の `c_type`（関数ポインターの宣言子）と `abi_type`（`ptr`）、大きさと整列（8）。向きの規則（`FnPtr` を含む型は Tsuzuri からホストへの向きだけ）と、その拒否の kind `fnptr_from_host`。

## 他チケットへ提供するインターフェース

- `CheckedRecord.c_layout`、`abi::c_layout_record`・`abi::host_record`。
- `llvm_abi::record_layout`（`@c_layout` 対応）と `c_layout_fields`（オフセット・大きさ・整列）。W24 Phase 2 は `host_pointer` の大きさ・整列と、`read_host_record`・`write_host_record` をそのまま使う。
- W25: vtable のメソッドの引数・結果の構造体（`[out]` の構造体を含む）に `@c_layout` の record を使う。
- [W36](W36-c-layout-by-value.md): `record_layout`・`c_layout_fields`（葉・オフセット・大きさ・整列）と `read_host_record`・`write_host_record` を使い、`ref` の無い `@c_layout` record を extern の引数・結果で C の値渡し・値返しにする。分類のために配置を数え直さない（W36 の S4）。
- W27: win32metadata の構造体を `@c_layout`（`PackingSize` は `pack`）に写す。

## 実装手順

### Phase 1

1. **変更**: なし（下書きは `target/scratch/w21/`）。**内容**: 着手条件の基準の保存。C の配置の参照表（`LOGFONTW`・`DLGTEMPLATE`・`GUID`・`MSG`・`PAINTSTRUCT`・`RECT`・`NMHDR`、詰め物のある自作の構造体、`#pragma pack(1)`・`(2)` の自作の構造体）を、`windows.h` の有無に分けて C で `printf` させて記録する。**確認**: MinGW と MSVC 環境の値が一致する（違えば S2）。
2. **変更**: `src/parser.rs`・`src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/diagnostic_kinds.rs`。**内容**: 属性の構文と脇のリスト、`E0002` の 3 つの kind。**確認**: T0、`tests/c_layout.rs` の構文のテスト、`tests/formatter.rs`、`src/syntax_codec.rs` の test module、`node tests/frontend_cache.mjs target/debug/tsuzuri`。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/E1008.md`。**内容**: `CheckedRecord.c_layout`、仕様 2 の検査、`host_record`。**確認**: `tests/c_layout.rs` の受理・拒否（kind を照合）。T1。
4. **変更**: `src/llvm_abi.rs`。**内容**: 配置の計算、packed struct の境界の型、`read_host_record`・`write_host_record`、`handle_typedefs`、`header_types`。**確認**: `tests/c_layout.rs` の IR とヘッダーの文字列（期待値は C コンパイラが出した手順 1 の表から書く。今の出力を写さない。GUIDE-LITE §2 規則 10）。IR が 2 回の生成で同じ。基準との比較（S1）。
5. **変更**: `src/llvm.rs`（WASM の `abi_native_only` を W15 の場所で `@c_layout` の record にも出す）、`src/driver.rs`（`c_layout_bindings_pending`）。**確認**: `tests/c_layout.rs` の WASM の拒否と、バインディングの拒否（`std::process::Command` で CLI を呼ぶ）。
6. **変更**: `tests/c_layout.mjs`・`tests/c_layout_host.c`・`tests/fixtures/c_layout/Main.tz`。**内容**: テスト計画の E2E。**確認**: `node tests/c_layout.mjs target/debug/tsuzuri`（native と wasm32、`-O0`・`-O3`、`live == 0`）。
7. **変更**: `tests/windows.mjs`。**内容**: `windows.h` との静的表明の段。**確認**: Windows で `node tests/windows.mjs target/debug/tsuzuri.exe`。
8. **変更**: 文書。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full`。

### Phase 2

9. **変更**: `src/check.rs`・`src/abi.rs`・`src/llvm_abi.rs`。**内容**: `FnPtr`・`ref ZString`・`Maybe<ref ZString>` の field、`c_layout_input_only`、ヘッダーの関数ポインターと `const uint16_t *`。**確認**: `tests/c_layout.rs`、`tests/c_layout.mjs` の Phase 2 の段（C ホストが文字列ポインターを読み、関数ポインターを呼ぶ）、W06 が done なら `tests/windows.mjs` の `RegisterClassExW`・`UnregisterClassW` の段。

### Phase 3

10. **変更**: `src/bindings.rs`・`src/runtime/bindings-core.mjs`・`src/bindings_native.rs`・`src/driver.rs`（拒否を外す）。**確認**: `cargo test --locked --test bindings`、`node tests/bindings.mjs target/debug/tsuzuri`・`node tests/host_bindings.mjs target/debug/tsuzuri`（C# は .NET SDK があるとき）。印のない record の生成物が基準と同じ。

## テスト計画

### Rust テスト（`tests/c_layout.rs`、新規）

| テスト | 内容 | 期待 |
| --- | --- | --- |
| `attribute_syntax` | `@c_layout`・`@c_layout(pack = 1)`、`private record`、`def` の前（`misplaced_c_layout_attribute`）、2 回（`c_layout_repeated`）、`pack = 3`・`align = 4`（`c_layout_argument`）。2026 と 2027 の両方の edition で同じ結果（`tests/editions.rs` にも 1 件足す） | 受理と `E0002` の kind |
| `field_types` | 仕様 2 の表の各型の受理と、`Maybe<所有するハンドル>` の受理と、`bool`・裸の所有するハンドル・`string`・`char`・`(i32, i32)`・印のない入れ子・型引数・field なし・`Drop` の拒否 | `E1008` の kind |
| `dlg_template_packs_to_18_bytes` | `@c_layout(pack = 2)` の `DlgTemplate` | 境界の型 `<{ i32, i32, i16, i16, i16, i16, i16 }>`、ヘッダーの `#pragma pack(push, 2)` と `== 18` の表明 |
| `log_font_is_92_bytes` | `LogFont` | ヘッダーの `uint8_t italic;`・`uint16_t face_name[32];`、`== 92`、`face_name` の `offsetof` 28 |
| `padding_is_explicit` | `{ a: i8, b: i32, c: i8 }`、`{ a: i16, b: i64 }`、入れ子の record の後の詰め物 | `<{ i8, [3 x i8], i32, i8, [3 x i8] }>` など（手で数えた値） |
| `pack_caps_nested_alignment` | `pack = 1` の外の record の中の、整列 4 と 8 の入れ子（`i32`・`i64`・`[i32; 2]` の field） | 入れ子のオフセットが詰められ（C の参照値）、入れ子の中の葉の `load`・`store` の `align` が 1 |
| `header_assert_names` | `size`・`f_x` という名前の field を持つ record | 静的表明の `typedef` の名前が重ならず、ヘッダーが `clang -fsyntax-only` と `clang -x c++ -fsyntax-only` で通る |
| `copy_handles_and_nulls` | `extern type copy Hwnd`、`hwnd: Hwnd`・`parent: Maybe<Hwnd>` | ヘッダーにハンドルの `typedef`（明示のリンク名の extern だけが使う record でも）、境界の型の `ptr` |
| `positions` | 仕様 4 の各位置の受理（`ref mut R` を含む）。印のない record の中の `@c_layout` を extern に渡す | 受理、`E1008`（`c_layout_in_plain_record`） |
| `unmarked_records_do_not_change` | 印のない同じ record の IR とヘッダー | 印のある版と境界の型だけが違い、印のない版は基準のコンパイラの出力と同じ形（`{ i32, i32, ... }`） |
| `wasm_rejects_pointer_fields` | ハンドルの field の record を使う extern を wasm32 で生成 | `E1008`（`abi_native_only`）。数だけの record は wasm32 で生成できる |
| `bindings_wait_for_phase_3` | `--emit bindings-js`・`bindings-cs` | `E2000`（`c_layout_bindings_pending`） |
| `layout_changes_are_api_breaks` | `tsuzuri api-diff` で `@c_layout` の追加・`pack` の変更 | 互換のない変更 |

### E2E（`tests/c_layout.mjs`、新規。`tests/host_imports.mjs` の形）

- フィクスチャ: `LogFont` の縮小版（境界値 `-128`・`255`・`-32768`・`65535` の field、`f32`・`f64`、`[i16u; 4]`）、`pack = 1` と `pack = 2` の record、詰め物のある入れ子、`pack = 1` の外の record の中の自然な整列の入れ子（入れ子の `i32` が奇数のオフセットに来る）、`copy` ハンドルと `Maybe<H>` の field（native だけ。`None` と `Some`）。
- C ホスト（`tests/c_layout_host.c`）は生成したヘッダーを `#include` し、`export def` に構造体を渡して field ごとの値を返させ、`extern def` の実装で構造体を受け取って値を検査し、結果の構造体を書いて返し、`ref mut` の構造体の各 field を書き換える。詰め物に 0 以外の値を置いて、Tsuzuri が詰め物を読まないことも確かめる。期待値は C の側で書く（GUIDE-LITE §2 規則 10）。
- native の `-O0`・`-O3`。wasm32 の `-O0`・`-O3` では数だけのフィクスチャを、`DataView` でヘッダーのオフセットどおりに読み書きする JS のホストで同じ検査をする。`live == 0`。

### Windows（`tests/windows.mjs` の段）

- `LogFont`・`DlgTemplate`・`Guid`・`Msg`（`hwnd: Hwnd`（`copy`）、`message: i32u`、`w_param: i64u`、`l_param: i64`、`time: i32u`、`pt: Point`、`l_private: i32u`。`private` は予約語）、Phase 1 の形の `WndClassEx`（関数ポインターと文字列ポインターを `i64` にしたもの）のヘッダーを、`windows.h` と一緒にコンパイルし、`sizeof(tz_record_...) == sizeof(LOGFONTW)` と主な field の `offsetof` を `_Static_assert` で比べる。`-O0`・`-O3` は関係しない（コンパイルだけ）。
- `CreateFontIndirectW` に `ref LogFont` を渡し、0 でないハンドルが返ること（`DeleteObject` で解放）。

### 変えないことの確認

- 着手条件で保存した基準と、すべての `tests/fixtures`・`examples` の `--emit llvm -O1`（native と wasm32）と `--emit header` を比べ、byte 単位で一致する（`c_layout` のフィクスチャを除く）。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/record.md`: 新しい節「C の配置（`@c_layout`）」（構文・field の型・配置の規則・`pack`・`bool` の扱い・例）。「メモリ上の大きさ」の「`export def` で見える C の構造体は、宣言の順のまま」に、印のない record は 32 bit に正規化した構造体、`@c_layout` は正確な配置と足す。「借用フィールド」の最後の段落（「レイアウトを C の構造体そのものとは見なさないでください」）を、印のない record に限る文にして新しい節へリンクする。
- `_tsuzuri/language-reference/values-and-functions/attributes.md`: 「属性の一覧と適用対象」の表に `@c_layout`（対象は record）、新しい節「@c_layout (C の構造体の配置)」と「書ける位置とエラー」（`E0002` の 3 つの kind）。
- `_tsuzuri/language-reference/compiler/native-interop.md`: 「公開できる型」と「ホスト関数を import する」に `@c_layout` の record、ヘッダーの形（`#pragma pack`・静的表明）、ハンドルの field、WASM の制限、Phase 1 の間のバインディングの `E2000`。Win32 の例（`LOGFONTW`・`DLGTEMPLATE`）。
- `_tsuzuri/language-reference/compiler/diagnostics.md`（生成の部分は `node scripts/gen-docs.mjs`）と `docs/language.md` の診断の表。
- `docs/language.md`: 「ホスト関数のインポート」の「`bool` および狭幅整数は 32-bit 幅へと正規化されて受け渡されます」と「レコード型の受け渡しには、正規化された一時構造体が使用されます」に `@c_layout` の例外を足し、「不透明ハンドル」の「ABI 境界のレコードのフィールド型 ... は E1008」を `@c_layout` の `copy` ハンドルの例外付きに直す。
- `docs/architecture.md` の「ホスト境界」の節: 配置の計算の 1 か所（`record_layout`・`c_layout_fields`）、packed struct の境界の型、内部の並べ替えとの関係。
- `README.md` の「変更の種類別」の「host ABI・import」の行に `tests/c_layout.rs`・`c_layout.mjs` を足す。C ABI の節の記述が誤りになるなら直す。
- Phase 3: native-interop.md の「共有ライブラリと各言語のバインディング」の生成器の対応。

## 受け入れ条件

- [ ] 仕様 1〜7 のとおりに動く（Phase 1。Phase 2・3 は着手したとき）。
- [ ] `@c_layout` を使わないプログラムの IR・ヘッダー・診断・バインディングが、すべての `tests/fixtures`・`examples` で基準と byte 単位で一致する（native と wasm32）。
- [ ] `LOGFONTW`・`DLGTEMPLATE`・`GUID`・`MSG` と自作の詰め物・`pack` の構造体の配置が、MinGW と MSVC 環境の C コンパイラの `sizeof`・`offsetof` と一致する（`tests/windows.mjs`、`tests/c_layout.mjs`）。
- [ ] ヘッダーの静的表明が C と C++ のどちらのコンパイルでも通る。
- [ ] E2E が native と wasm32 の `-O0`・`-O3` で通り、`live == 0`。
- [ ] 新しい診断は既存のコード（`E0002`・`E1008`・`E2000`）の kind で、`diagnostics.md` と `docs/language.md` に載った。
- [ ] `FRONTEND_FORMAT` を 1 上げ、`src/syntax_codec.rs` の test module と `frontend_cache.mjs` が通る。
- [ ] 文書を更新し、`check-docs`・`gen-docs --check`・`check-cli-docs` が通る。PR に `ci:full` を付けた。

## 落とし穴

- **packed struct の要素の整列**: `<{ ... }>` の要素への `load`・`store` に `align` を書かないと、LLVM は整列 1 と見なして遅い命令を出すか、逆に `align` を自然な値で書くと `pack = 1` の構造体で誤った前提（未定義動作）になる。葉の実効の整列と外側の上限 `cap` の最小値を書く。入れ子の record を再帰で読み書きするとき、入れ子自身の整列だけを見ると、`pack` の外の record の中で誤る。
- **入れ子の `pack`**: 外の `pack` は入れ子の record の置き場所の整列に上限をかけるが、入れ子の中の配置は入れ子自身の `pack` で決まる（MSVC・Clang と同じ）。入れ子の LLVM の型も packed で、自分の詰め物を持つ。
- **ヘッダーのハンドルの `typedef`**: 明示のリンク名の extern（Win32 の API はすべてそう）だけが使う record も `typedef` は出るので、`handle_typedefs` が record の field に降りないとヘッダーがコンパイルできない。
- **`decode_host_scalar` を通さない**: `@c_layout` の 8・16 bit の field を既存の経路に通すと `trunc i32` になり型が合わない。field の型で分岐する。
- **`Maybe<H>` の内部の表現**: 内部はタグ付きの共用体で、ハンドルにニッチはない（`src/llvm_layout.rs` の `niche_layout`）。境界で W15 の符号化を通す。内部の配置を C に合わせようとしない（S4）。
- **match の `_ =>`**: `CheckedRecord` に field を足したら `grep -rn "CheckedRecord {" src/` で構築する所をすべて直す。
- **`#pragma pack` の対**: `push` と `pop` を必ず対にし、ほかの record の配置に漏らさない。
- **印のない record の中の `@c_layout`**: 印のない record の境界の型は LLVM の普通の struct で、packed struct の要素の整列は 1 なので、整列 8 の `@c_layout` の record を入れると LLVM のオフセットが C（と `record_layout`）より前にずれる。だから境界では拒否する（`c_layout_in_plain_record`）。`scalar_record` が `@c_layout` の入れ子を偽にするのが手早い。
- **ARM64 の Windows**: 配置は同じ規則のはずだが確かめていない。CI の Windows の aarch64 の段があればそこでも静的表明を通す。

## 対象外

- `align(N)`（`__declspec(align)`・`alignas`）、ビットフィールド、C の共用体、可変長の末尾の配列（`DLGTEMPLATE` の後に続く可変部は、利用者がバイト列で組む）。
- 構造体の値渡し（`POINT` を値で取る `WindowFromPoint` など。Win64 の 8 バイト以下の構造体の分類）。[W36](W36-c-layout-by-value.md) が扱う。W36 が入るまでの回避策として `i64` に詰めて渡す方法を文書に書くなら、整数の field だけで大きさが 1・2・4・8 バイトの構造体（`POINT`）に限る。浮動小数点の field を持つ構造体は、Windows Arm64 と System V で浮動小数点レジスターで渡るので誤る（W36 の参照表の `D2D1_POINT_2F`）。3・5・6・7 バイトの構造体は、Windows x64 では複写へのポインターで渡るので誤る（同じ表）。W36 の文書はこの回避策の文を消して値渡しの節を指す。
- 構造体の配列の field（`[R; N]`）と、構造体の配列のバッファ（`ref [R]`）。
- 呼び出し先が書く可変バッファの field（`OPENFILENAMEW.lpstrFile` の `ref mut [i16u]`）。W25 の `IFileOpenDialog` が代わりの経路になる。
- 内部の配置を C の配置と同じにしてコピーなしで渡すこと（D5）。
- 32-bit のネイティブ target（コンパイラが対応しない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 属性 `@c_layout` と `@c_layout(pack = N)`（N は 1・2・4・8・16）を `record` の宣言の前に書く。D-53 の閉じた属性の集合に足し、脇のリスト `Program.c_layouts` に持つ | record ごとの明示の opt-in なら、既存のホストと IR を変えずに済む。`pack` は `windows.h` の `#pragma pack(2)`（`DLGTEMPLATE`）などに要る。`extern record` という新しい宣言の形は、record の機能（構築・パターン・`deriving`）を二重に持つことになるので採らない | 要承認（承認前は Phase 1 の手順 2 以降に着手しない） |
| D2 | 印のない record の内部・境界の配置、IR、ヘッダー、`ABI_VERSION` は変えない | 既定を変えると E05 の正規化を前提にした C・JS・C#・Python・C++ のホストが黙って壊れる | 既定案（実装者はこの案に従う） |
| D3 | `@c_layout` の record だけ、境界の構造体の field を正確な幅にする（E05 の「i8/i16/bool field は ABI struct 内で i32 に正規化する」の例外） | Win32 の構造体の `BYTE`・`WORD`・`WCHAR` を表せない（`LOGFONTW` は 92 バイトのところ 180 になる）。引数とスカラーの結果の正規化は、呼び出し先が下位のビットだけを読むので残す | 要承認（承認前は Phase 1 の手順 3 以降に着手しない） |
| D4 | 渡し方は今と同じポインター（引数は `const T *`、`ref mut` は `T *`、結果は out ポインター）。C の値渡しの分類は再現しない | E05 の「platform ABI classification を IR で再現しない」を保つ。値渡しの API は少ない（対象外。値渡しは [W36](W36-c-layout-by-value.md)） | 既定案（実装者はこの案に従う） |
| D5 | 内部の配置（並べ替え・`i1` の bool・タグ付きの `Maybe`）は変えず、境界で field ごとに複写する | 内部の並べ替えはすでに `field_slot` で境界から切り離されている。Win32 の構造体は 16〜152 バイトで、field ごとの複写の数は field の数に比例するだけ（速さは計測していない）。内部を C に合わせる案は、ハンドルの `Maybe` の内部表現や並べ替えと衝突し、コピーなしの利点の代わりに内部の規則を 2 つにする | 既定案（実装者はこの案に従う） |
| D6 | `bool` の field は `E1008`。`BOOL` は `i32`、`_Bool` は `i8u` と書く | 4 バイトか 1 バイトかを黙って選ぶと、片方の C の型で配置がずれる（GUIDE §1 規則 2） | 既定案（実装者はこの案に従う） |
| D7 | 裸のハンドルの field は `extern type copy`（W18）だけ、`Maybe<H>` の field は W15 の共通契約のとおり任意の extern type を許す（どちらもネイティブだけ）。裸の所有するハンドルは `E1008`。E12 D5（ABI の record の field にハンドルを置かない）と D11（ハンドルを含む record は `E1008`）を `@c_layout` について改める | `WNDCLASSEXW`・`MSG`・`PAINTSTRUCT` はハンドルを持つ。D5 の理由（wasm32 で 4 バイト、JS の糊）は、WASM で拒否し配置を target ごとに 1 か所で決めることで答える。裸の所有するハンドルを許すと、copy-out・結果・`extern view` の読み出しがホストのメモリから所有権を作り、上書きで漏らす。`Maybe<H>` の所有するハンドルは[共通の綴り](README.md#共通の綴り起票時に決めた契約)に従って受けるが、同じ懸念がある（承認のときに `copy` に限るかを決める） | 要承認（承認前はハンドルの field の部分に着手しない。数だけの field は D1・D3 の承認で進めてよい） |
| D8 | 配置は `record_layout` と `c_layout_fields` の 1 か所で求め、境界の LLVM の型は詰め物を明示した packed struct、各 `load`・`store` に葉の保証される整列（葉の実効の整列と外側の record の上限の最小値）を書く | LLVM の data layout と Rust の計算が `pack` で食い違う余地をなくす。field の番号で GEP する既存の形を保ち、IR の型を読めば C の配置が分かる。バイトのオフセットで GEP する案も正しいが、既存の `read_host_record` の形から離れ、IR の型が配置を表さなくなる | 既定案（実装者はこの案に従う） |
| D9 | ヘッダーに `#pragma pack` と、大きさ・オフセットの静的表明（負の長さの配列の `typedef`）を出す | ホストの C コンパイラが Tsuzuri の配置の前提を確かめられる。`_Static_assert` は MSVC の既定の C モードで使えるか確かめていない（未確認）ので、C89・C++ のどちらでも通る形にする | 既定案（実装者はこの案に従う） |
| D10 | WASM では、ポインター幅の field を持つ `@c_layout` の record を `E1008`（`abi_native_only`）にし、数だけの record は使える | Win32 の用途はネイティブだけで、WASM の既存の出力を変えない。数だけの record は wasm32 の C の配置とも一致する | 既定案（実装者はこの案に従う） |
| D11 | Phase 1・2 の間、`--emit bindings-js|cs|py|cpp` は `@c_layout` の record を `E2000` で拒否し、Phase 3 で対応する。`ABI_VERSION` は上げない | 今の糊と生成器は 8・16 bit を 32 bit として読むので、黙って誤ったオフセットになる。版を上げるとすべての既存の生成物の冒頭が変わる | 既定案（実装者はこの案に従う） |
| D12 | 型引数・region・`Drop`・field なしの record は `@c_layout` にできない | C に型引数と空の構造体はない（ISO C）。`Drop` の record は隠れた生存フラグを持つ資源で、データではない（`scalar_record` と同じ） | 既定案（実装者はこの案に従う） |
| D13 | Phase 2 の field（`FnPtr`・`ref ZString`・`Maybe<ref ZString>`）を持つ record は入力の位置だけ。`FnPtr` の field は W22 の `fnptr_from_host`、借用の `ZString` の field は `c_layout_input_only` で拒む | ホストが書いたポインターから `FnPtr`（W22 は外部の関数ポインターを受け取らない）や借用を作らない。`FnPtr` の向きの規則と kind は W22 が持つので、それに合わせる | 既定案（実装者はこの案に従う） |
| D14 | 診断は新しいコードを割り当てず、`E0002`・`E1008`・`E2000` の新しい kind で表す。属性の置き場の誤りは `@cpu`（`misplaced_cpu_attribute`）と同じ `E0002` | D-51、E12 D12 の前例 | 既定案（実装者はこの案に従う） |
