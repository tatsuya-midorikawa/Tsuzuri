# W24: `extern view`（ホストが渡したアドレスと C レイアウトの record の型付きの複写）

| 項目 | 内容 |
| --- | --- |
| ID | W24 |
| 優先度 | P1 |
| 規模 | M（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | W15（Phase 2: W21） |
| 後続 | W25, W27 |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 承認済み（D-63）。細目は要承認（決定事項 D1 の綴りと文脈キーワードの登録。ほかは既定案） |
| 手本にする既存実装 | `src/llvm_abi.rs` の `wrapper`（record の引数を `host_pointer` で null と整列を検査してから `read_host_record` で読む）、`src/llvm.rs` のコールバックを取る extern の呼び出し地点の `host_call`（`import.callbacks` の分岐。関数本体を出さない）、`src/check.rs` の `validate_callbacks`（直接・全引数の呼び出しの検査）、`src/parser.rs` の `extern_declaration`（`extern type` の分岐） |
| 主な影響ファイル | `src/parser.rs`・`src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT`）・`src/formatter.rs`・`src/docgen.rs`・`src/check.rs`（`HostImport`）・`src/abi.rs`・`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/bindings.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/capabilities.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/`・`tests/extern_view.rs`（新規）・`tests/extern_view.mjs`（新規）・`tests/extern_view_host.c`（新規）・`tests/fixtures/extern_view/`（新規）・`tests/windows.mjs`・文書（D-63 の文言の改訂を含む） |

## 実装者向け早見

- 範囲: `extern view def read_nmhdr :: i64 -> NmHdr` でホストが渡したアドレス（`i64`）から record を読み、`extern view def write_minmax :: i64 -> ref MinMaxInfo -> unit` で書く。アドレスが 0 か整列違反なら、呼び出し地点でトラップする。範囲と型の正しさは検査しない（`@trusted` と `ffi` の監査の責任）。Phase 1 は 32/64-bit の数の field だけの record（今の境界の配置が C と一致する）、Phase 2 は `@c_layout` の record（[W21](W21-c-layout-records.md)）。WASM の `E1008` の kind は [W15](W15-abi-nullable-handles.md) と共有する。共通の決定は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)。
- 最初に読む: ① 仕様 1〜5 ② `src/llvm_abi.rs` の `wrapper` の record の引数の腕と `host_pointer` ③ `src/llvm.rs` の `import.callbacks` の 2 か所（呼び出し地点の `host_call` と、本体を出さない分岐）④ `src/check.rs` の `validate_callbacks`。
- 最初に書くテスト: `tests/extern_view.rs` の `read_and_write_lower_at_the_call_site`（IR に `inttoptr`・null と整列のガード・field ごとの `load`／`store` があり、`declare` と `call` がない）。
- 触ってはいけない: view を使わないプログラムの IR・ヘッダー・`tsuzuri audit` の出力・診断。`TrapKind` の列（増やさない）。普通の extern の意味と生成。
- 完了の見分け方: Windows で次が終了コード 42 になり、`read_point 0` を足して `--trap-info` で build すると `trap: index out of bounds at <path>/Main.tz:<その呼び出しの行>:<列>` で止まる。

```tsuzuri
record Point { x: i32, y: i32 }

@trusted
extern "GetProcessHeap" def process_heap :: unit -> i64
@trusted
extern "HeapAlloc" def heap_alloc :: i64 -> i32u -> i64 -> i64
@trusted
extern "HeapFree" def heap_free :: i64 -> i32u -> i64 -> i32
@trusted
extern view def read_point :: i64 -> Point
@trusted
extern view def write_point :: i64 -> ref Point -> unit

def main :: unit -> i32 = \() ->
    let heap = process_heap ()
    let address = heap_alloc heap 8 8
    let point = Point { x: 40, y: 2 }
    write_point address (ref point)
    let back = read_point address
    let _ = heap_free heap 0 address
    back.x + back.y
```

## 目的

Win32 のウィンドウプロシージャは、`lParam` で構造体のアドレスを受け取る（`WM_NOTIFY` の `NMHDR`、`WM_GETMINMAXINFO` の書き換える `MINMAXINFO`、`WM_CREATE` の `CREATESTRUCTW`）。今の Tsuzuri には、そのアドレスの先を読む契約内の方法がない。D-63 で利用者は、生ポインター型・ポインター演算の構文・`unsafe` を入れずに、`extern` の宣言として書き `@trusted` と `ffi` の監査の対象になる型付きの複写 `extern view` を足すと決めた。このチケットはその本体で、出荷する PR で strategy.md・README の「採用しない改善」・[E12](_completed/E12-ffi-extensions.md) D13 の文言を改める。監査は [E15](_completed/E15-trust-boundary-and-capabilities.md) の `@trusted`・`W1010`・`tsuzuri audit` にそのまま乗せる。COM の [W25](W25-extern-interface-com.md) と生成器の [W27](W27-win32-package-from-winmd.md) が後に続く。

## 着手条件と停止条件

### 着手条件

- Phase 1: W15 が done か、W15 を待たずに進めることを人間が認めた（「前提とする他チケットのインターフェース」）。決定事項 D1 の承認。
- Phase 2: Phase 1 と W21 の Phase 1 が done。
- 共通: `node scripts/repo-kb.mjs brief W24`。`FRONTEND_FORMAT`（起票時 20）と `DiagnosticKind` の個数を控える。`tests/fixtures/*/Main.tz`・`examples/*/Main.tz` の `--emit llvm -O1`（native と wasm32）と、`tests/fixtures` の extern を持つプロジェクトの `tsuzuri audit --json` を `main` のコンパイラで `target/scratch/w24/base/` に保存する。

### 停止条件

- **S1** view を使わないプログラムの IR・ヘッダー・audit の出力・診断が基準と違う。
- **S2** `HostCall` を view に使うと、ある経路（所有権・効果・権限・KG・定数評価・被覆）の結果が普通の extern と違ってしまい、`view` の分岐を足すだけでは直らない。
- **S3** トラップの位置を呼び出し地点にするには、直接の呼び出しの規則（決定事項 D4）より広い変更が要る。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

再現は Git Bash で、`T=/c/Users/tmidorikawa/.vscode/extensions/tmidorikawa.tsuzuri-0.1.6-win32-x64/toolchain` とし、各 `Main.tz` を `target/scratch/w24/<名前>/` に置いて行った（開発版 `target/debug/tsuzuri.exe`、MinGW）。

- **構文がない**（再現 1）: `record NmHdr { hwnd_from: i64, id_from: i64, code: i32u }` と `@trusted extern view def read_nmhdr :: i64 -> NmHdr` と `def main :: unit -> i32 = \() -> 0` の `Main.tz` を `target/debug/tsuzuri.exe check target/scratch/w24/view` にかけると、`view` の位置に `error[E0002]: expected 'def' after extern`（`src/parser.rs` の `extern_declaration` は、リンク名の文字列の後に `type` か `def` だけを受ける）。
- **任意のアドレスはすでに読める**（D-63 の事実 1）: `record Word { value: i32 }` と `extern "RtlMoveMemory" def peek :: i64 -> i64 -> Word`（record の結果は先頭の out 引数になるので、`RtlMoveMemory(out, base, 2)` になる）と `extern "GetModuleHandleW" def module_base :: i64 -> i64` で、`main` を `let base = module_base 0`・`let word = peek base 2`・`word.value % 256` とした `Main.tz` を `TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build target/scratch/w24/peek -o target/scratch/w24/peek/peek.exe` で作って実行すると、終了コード 77（exe の先頭の `M`）だった（再現 2）。早見の例の view を `extern "RtlMoveMemory" def peek :: i64 -> i64 -> Point` に置き換えた版も、`HeapAlloc` の領域（0 で埋めた 8 バイト）を読んで終了コード 42（`0 + 0 + 42`）だった。安全性の実体はすでに「extern の宣言が正しいこと」にあり、view はそれに型と監査の形を与える。
- **ホストのポインターの検査はある**: `src/llvm_abi.rs` の `wrapper`（`export def` と静的コールバックの C の入口）は、record の引数を `host_pointer(ptr, size, align, false)` で検査してから `read_host_record` で読む。`host_pointer` は null と `ptrtoint` の下位ビットの整列をガードし（`TrapKind::BoundsCheck`）、WASM ではさらに線形メモリの大きさを見る。`BoundsCheck` の文は `src/trap.rs` で `index out of bounds`。本体を `let xs = [1, 2, 3]` と `xs[args.length + 5] as i32` の 2 行にした `def main :: Array<string> -> i32 = \args ->` の `Main.tz`（添字は 3 行目の 5 列目から）を `TSUZURI_CLANG=$T/bin/tsuzuri-clang.exe target/debug/tsuzuri.exe build <dir> --trap-info -o t.exe` で作って実行すると、標準エラーに `trap: index out of bounds at <dir>\Main.tz:3:5` と出て、Git Bash で終了コード 132（`Illegal instruction`）だった。同じものを `--trap-info` なしで build すると、この Windows の開発版では標準エラーに何も出なかった（`build --help` は `--trap-info` を「Report trap locations ... disabled by default for build」と書く。ほかの OS の `--trap-info` なしの出力は確かめていない）。テストは `--trap-info` を付けて位置を見る。
- **32/64-bit の field だけの record は C と同じ配置**（再現 3）: `record NmHdr { hwnd_from: i64, id_from: i64u, code: i32u }`、`record Point { x: i32, y: i32 }` を 5 つ持つ `MinMaxInfo`、ポインターを `i64` にした `CreateStruct`（`create_params`・`module_instance`・`menu`・`parent` が `i64`、`cy`・`cx`・`y`・`x`・`style` が `i32`、`name`・`class_name` が `i64`、`ex_style` が `i32u`）を `export def` に使い、`target/debug/tsuzuri.exe build <dir> --emit header -o out.h` とした。`#include <windows.h>`・`<stddef.h>`・`"out.h"` の後に `_Static_assert(sizeof(tz_record_4Main_5NmHdr) == sizeof(NMHDR), "")`、`offsetof(tz_record_4Main_5NmHdr, code) == offsetof(NMHDR, code)`、`MINMAXINFO` の大きさと `ptMaxTrackSize`、`CREATESTRUCTW` の大きさと `lpszName`・`dwExStyle` のオフセットを比べる C を、`$T/bin/tsuzuri-clang.exe -c check.c` と、PowerShell の `Enter-VsDevShell`（MSVC の開発者シェル。`-DevCmdArguments "-arch=x64 -host_arch=x64"`）の後の `& "C:\Program Files\LLVM\bin\clang.exe" -c check.c` の両方でコンパイルでき（24・40・80 バイト）、表明はすべて成り立った。`src/llvm_abi.rs` の `record_layout` は 64-bit の数を `(8, 8)`、ほかのスカラーを `(4, 4)` にし、`abi_type` は 32/64-bit の数をその幅のまま使う。8・16 bit と `bool` は 32 bit に広がるので C と一致しない（W21 の「現状」）。
- **extern の扱いは `HostCall` に集まる**: `src/check.rs` は extern の関数本体を `TypedExprKind::HostCall(HostImport, 引数)` にする（`HostImport` は `native_symbol`・`wasm_module`・`wasm_name`・`explicit`・`callbacks`）。`HostCall` は `src/` の 18 のファイルが見る。`src/capabilities.rs` は `HostCall` の関数への参照を権限 `ffi` の場所にし（`is_extern`）、`untrusted_externs` が `W1010` を、`Report::render_text` が `名前 -> "symbol"  path:line:col  trusted` を、`render_json` が `"symbol"` を出す。`src/effects.rs` の `HostCall` の腕は、`@trusted` の性質で保証されない限り `extern` の効果の源にし、`trap` を除外する（`exempt(Effect::Trap, .., "host-call")`）。`src/llvm.rs` の `extern_symbols` は extern の C の symbol を集め、tzroutine のプログラムでそれらの呼び出しを P を手放す包み（TZ08）に入れる。
- **直接の呼び出しの前例**: コールバックを取る extern は、`validate_callbacks` が直接・全引数の呼び出しだけを許し（`ExternCallbackCallForm`、`E1008`）、`src/llvm.rs` は呼び出し地点で `host_call` を出して関数本体を出さない（E12 D9）。
- **target**: ネイティブの target はすべて 64-bit（`src/target.rs` の `unknown_target_message`）なので、`i64` はポインターの幅と同じ。`check` は target を取らない（`tsuzuri check --target wasm32` は `E2000`）。
- **文書**: `_tsuzuri/language-reference/languages/strategy.md` の「採らないもの」に「`unsafe` と生ポインター型。低水準の処理はホストに置き、不透明ハンドルで受け渡します。」、`languages/why-tsuzuri.md` の表に「生ポインタ」、`_features/README.md` の「採用しない改善」に D-63 への注記付きの行、`_features/_completed/E12-ffi-extensions.md` の D13「導入しない」。

## 仕様

### 1. 構文

```text
extern_view := ["private"] "extern" "view" "def" name "::" "i64" "->" R                 // 読む
             | ["private"] "extern" "view" "def" name "::" "i64" "->" "ref" R "->" "unit" // 書く
```

- `view` は `extern` と `def` の間でだけの文脈キーワードで、ほかの所では識別子として使える（D-55 の登録）。
- どの edition（2026・2027）でも受ける。今はどの edition でも `E0002` になる書き方なので、意味の変わる既存のプログラムはない（GUIDE-LITE §7。`tests/editions.rs` で両方の edition を確かめる）。
- リンク名の文字列は書けない（`extern "x" view def` は `E0002`）。`@trusted` は性質なしでだけ書ける（性質を書くと `E1037`）。効果の属性（`@no_alloc` など）は今の extern と同じく `E1032`。

### 2. 型

- 読む形は `i64 -> R`、書く形は `i64 -> ref R -> unit` だけ。`R` を値で書く（`i64 -> R -> unit`）、引数の数・順が違う、アドレスが `i64` でない、などは `E1008`（`extern_view_signature`）。
- Phase 1 の `R`: field のない record でなく、すべての葉が `i32`・`i32u`・`i64`・`i64u`・`f32`・`f64` である record（その固定長配列と、同じ条件の入れ子の record を含む）。`Drop` の record・型引数を具体化した record は今の `scalar_record` の規則に従う。`bool`・8・16 bit の field は `E1008`（`extern_view_record`。今の境界の配置が C と一致しないため）。
- Phase 2 の `R`: 上の record か、`@c_layout` の record（W21 の Phase 1 の field: 正確な幅の数・その配列・入れ子・`extern type copy`・`Maybe<H>`）。W21 の Phase 2 の field（`FnPtr`・`ref ZString`・`Maybe<ref ZString>`）を持つ record は、読む形でも書く形でも `E1008`（`extern_view_record`）。

### 3. 意味

- 引数を左から右に評価した後、アドレスが 0 ならトラップ、`R` の整列（Phase 1 は 4 か 8。`record_layout` の値）の倍数でなければトラップする。どちらもメモリに触れる前で、トラップの位置はその view の呼び出し式（`--trap-info` の報告）。種類は `TrapKind::BoundsCheck`（`index out of bounds`。export の入口の検査と同じ）。
- 読む形は、アドレスから C の配置（宣言の順、C の整列と詰め物）で field ごとに読み、新しい `R` の値を返す。書く形は、借用した `R` の field をその配置でアドレスへ書く。詰め物のバイトは読まず、書かない。
- 検査しないもの（`@trusted` の責任）: アドレスの先に `R` の大きさだけの読み書きできるメモリがあるか、そこが本当にその構造体か、ホストが同時に書き換えていないか。誤れば、ほかの誤った extern の宣言と同じく、プロセスの異常終了やメモリの破壊になりうる。field の読み書きは 1 つずつの普通の `load`・`store` で、全体として原子的ではない。
- アドレスの計算は普通の `i64` の演算（`lparam + 16` など）で、新しい演算子や型はない。
- view は直接・全引数の呼び出しでだけ使える。関数値としての使用・部分適用は `E1008`（`extern_view_call_form`）。

### 4. 効果・権限・監査

- 効果: `extern`（`pure def` から呼ぶと `E1029`）と `trap`（`@no_trap` の関数から呼ぶと `E1031`）の源。確保はせず、停止する（`@no_alloc`・`@no_copy`・`@terminates` の関数から呼べる）。
- 権限: 呼び出しは `ffi` の場所（普通の extern と同じ）。`tsuzuri audit` のテキストは `read_point -> view read  path:line:col  trusted`、JSON の extern の行は `"symbol":null,"view":"read"`（view の行だけに `view` の鍵を足す）。`@trusted` のない view は `W1010` の対象。
- C ヘッダー・バインディング: 何も出さない（view はホストの関数ではない）。

### 5. target

- ネイティブだけ。wasm32・wasm64 で view が届いたプログラムは、`build` で `E1008`（W15 の kind `abi_native_only`。位置は view の宣言）。`check` は target を取らないので、この誤りは `build` で出る。

### 6. 診断

| コード | kind（案） | 位置 | メッセージ（案） |
| --- | --- | --- | --- |
| `E0002` | `extern_view_link_name` | 文字列 | `an extern view copies memory and links no symbol; remove the string` |
| `E0002` | 既存の `expected 'def'` | `view` の後 | `expected 'def' after 'extern view'` |
| `E1008` | `extern_view_signature` | 宣言の名前 | `an extern view is 'i64 -> R' to read or 'i64 -> ref R -> unit' to write, with R a record` |
| `E1008` | `extern_view_record` | 宣言の名前 | `extern view 'read_font' copies the record 'Font', whose field 'italic' is i8u; the record ABI widens it, so its layout differs from C; use 32-bit or 64-bit fields` （Phase 2 では `or declare 'Font' with '@c_layout'` を足す） |
| `E1008` | `extern_view_call_form` | 式 | `extern view 'read_nmhdr' must be called directly with all its arguments` |
| `E1008` | `abi_native_only`（W15） | 宣言の名前 | view が WASM で使えない旨 |
| `E1037` | `trusted_view_property` | 最初の性質 | `'@trusted' on an extern view takes no properties; the compiler knows its effects` |

## 設計

### AST と構文

- `src/syntax.rs` の `SignatureDecl` に `view: bool`（`link` と同じく extern だけが使う）を足す。`src/parser.rs` の `extern_declaration` は、`link_name()` の後で、現在の字句が識別子 `view` で次が `def` なら view として読む。リンク名があれば `extern_view_link_name`。
- `src/syntax_codec.rs` の `SignatureDecl` の符号化に `view` を足し、`FRONTEND_FORMAT` を着手時の値から 1 上げる。`src/formatter.rs` は `extern view def` と書く。`src/docgen.rs` は view を `extern view` として出す。

### 検査（`src/check.rs`・`src/abi.rs`）

- `HostImport` に `pub view: Option<ViewDirection>`（`Read`・`Write`）を足す。view の `HostImport` は `native_symbol`・`wasm_module`・`wasm_name` が空、`explicit` と `callbacks` が偽。生成した symbol の重複検査（`external_symbols`）と、明示のリンク名の署名の一致の検査（`explicit_symbols`）に入れない。
- 署名の検査は、`abi::parameter`・`abi::result` の代わりに仕様 2 の形を見る。`R` の条件は `src/abi.rs` の新しい `view_record(ty, types) -> Result<(), ViewRecordError>`（Phase 1 は `scalar_record` かつ葉が 32/64-bit、Phase 2 は W21 の `c_layout_record` も）。
- `validate_callbacks` の「直接・全引数」の検査を、コールバックを取る extern と view の両方に当てる（kind は view なら `extern_view_call_form`）。
- `@trusted` の性質の検査（`program.trusted` を関数に結び付ける所）で、view に性質があれば `trusted_view_property`。

### 生成（`src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`）

- 呼び出し地点: `src/llvm.rs` の、`import.callbacks` の extern を呼び出し地点で `host_call` に下げる分岐に、`import.view.is_some()` の extern を `extern_view` に下げる分岐を足す。関数本体を出さない分岐（`import.callbacks` の `continue`）も view に当てる。
- `src/llvm_imports.rs` の `extern_view(direction, arguments, result)`: アドレスを評価して `inttoptr i64 %a to ptr`。`R` の `record_layout` の大きさと整列で `host_pointer(ptr, size, align, false)`（ネイティブなので null と整列だけ）。読むなら `read_host_record(R, ptr)`、書くなら借用から `R` を `load` して `write_host_record(R, value, ptr)`。`declare`・`call`・ヘッダーの宣言は出さない。
- `src/llvm.rs` の `extern_symbols` は view を集めない（P を手放す包みに入らない。メモリの複写だけで、待たない）。`src/llvm_imports.rs` の `header` と、`src/bindings.rs` の `imports` は view を飛ばす。
- `src/llvm_abi.rs`: `host_function`・`uses_host_abi`・`handle_typedefs`・ヘッダーの `record_types` は view を数えない（ヘッダーに `typedef` も `tsuzuri_alloc` も出さない）。一方 `read_host_record` は境界の LLVM の型 `%tz_record_*` で GEP するので、`type_definitions` は view が使う record の型を、`uses_host_abi` が偽でも出す。
- WASM: `extern_view` は `self.globals.wasm` のとき view の関数の番号を `abi_native_only` の共有の口の集合に記録し（生成はそのまま続けてよい）、`emit_program` の最後にその宣言の名前の位置で `E1008` になる。生成しない（届かない）view は拒否しない。

### 効果・権限・監査

- `src/effects.rs` の `HostCall` の腕: view なら、`extern` の源は今と同じ判定のうち「`@trusted` の性質による保証」を、`no_alloc`・`no_copy`・`terminates` の検査ではコンパイラが保証したものとして扱い（源にしない）、`pure`・`no_trap` の検査では源にする。`trap` は除外せず、源（理由 `extern-view-address`）にする。文字列の複写（`string-marshalling`）はない。
- `src/pure_rules.rs` の `Effect::Host(import.native_symbol)` は、view では `view <関数名>` を出す（理由の文が空の symbol にならないように）。
- `src/capabilities.rs`: `ffi` の場所の数え方は変えない（`HostCall` のまま）。`ExternUse`・`ExternReport` に `view: Option<ViewDirection>` を足し、`render_text` は `-> view read`、`render_json` は view の行だけ `"symbol":null,"view":"read"` にする。
- 残りの `HostCall` を見るファイル（`src/ownership.rs`・`src/kg/effects.rs`・`src/kg/sites.rs`・`src/coverage.rs`・`src/warnings.rs`・`src/const_machine.rs`・`src/const_rules.rs`・`src/prove_smt.rs`・`src/call_specialization.rs`・`src/llvm_overflow.rs`）は、`grep -rn "HostCall\|native_symbol" src/` で 1 つずつ開き、view が普通の extern と同じでよい（借用の引数、外から見えない呼び出し、定数評価の拒否）ことを確かめる。違うなら S2。

### Phase 2（W21 の後）

- `view_record` が `@c_layout` の record を受ける。`host_pointer` の大きさと整列、`read_host_record`・`write_host_record` は W21 が `@c_layout` に対応させたものをそのまま使う。読む形で `copy` のハンドルと `Maybe<H>` の field を作るのは、W21 の copy-out と同じ写し方（`Maybe<H>` は `null` ⇔ `None`）。

## 前提とする他チケットのインターフェース

- W15（[W15](W15-abi-nullable-handles.md)・[W13](W13-extern-ref-mut-and-mutable-buffers.md) と共有）: WASM の `E1008` の kind `abi_native_only` と、その共有の口（WASM の生成で native だけの形に出会った extern の関数の番号を `Globals` の集合に記録し、`emit_program` が関数の生成の後で最も小さい番号の extern の名前の位置に `E1008` を返す）。W24 の `extern_view` は、`self.globals.wasm` のとき view の関数の番号を同じ集合に記録する。W13・W15 のどちらも先に入っていなければ、このチケットが同じ名前の kind と口を作る（W13・W15 の設計にある「先に入ったチケットが作る」）。Phase 1 が W15 から使うのはこれだけ。
- W21（Phase 2。[W21](W21-c-layout-records.md)）: `abi::c_layout_record`、`@c_layout` に対応した `record_layout`・`c_layout_fields`・`read_host_record`・`write_host_record`（`Maybe<H>` の field は W15 の `nullable_from_c`・`nullable_to_c` で写す）。

## 他チケットへ提供するインターフェース

- `HostImport.view` と `ViewDirection`、`abi::view_record`、`FunctionEmitter::extern_view`。
- W25: `extern interface` の vtable の読み出しは、コンパイラが生成する型付きの複写として、同じ「0・整列の検査 → 型付きの load」の規則と、監査の表示（`HostImport` の種類の分岐）を使う。
- W27: win32metadata のメッセージの `lParam` の構造体に、`extern view` の宣言を生成できる。

## 実装手順

### Phase 1

1. **変更**: なし（下書きは `target/scratch/w24/`）。**内容**: 着手条件の基準の保存。`grep -rn "HostCall\|native_symbol" src/` の結果を控え、設計の各所に印を付ける。**確認**: 一覧のすべてに「変える」「変えない（理由）」が書けた。
2. **変更**: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/docgen.rs`・`src/diagnostic_kinds.rs`。**内容**: 構文と `E0002` の kind。**確認**: T0、`tests/extern_view.rs` の構文のテスト、`tests/formatter.rs`、`src/syntax_codec.rs` の test module。
3. **変更**: `src/check.rs`・`src/abi.rs`・`src/diagnostic_kinds.rs`・`src/explain/codes/E1008.md`・`E1037.md`。**内容**: `HostImport.view`、署名と record の検査、直接の呼び出しの検査、`@trusted` の性質。**確認**: `tests/extern_view.rs` の受理・拒否（kind を照合）。T1。
4. **変更**: `src/llvm.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`・`src/bindings.rs`。**内容**: 呼び出し地点の生成と、symbol・ヘッダー・包みからの除外、WASM の `abi_native_only`。**確認**: `tests/extern_view.rs` の IR のテスト（2 回の生成で同じ）。基準との比較（S1）。
5. **変更**: `src/effects.rs`・`src/pure_rules.rs`・`src/capabilities.rs`。**内容**: 効果・権限・監査。**確認**: `tests/extern_view.rs` の効果・監査のテスト、`cargo test --locked --test audit`・`--test trusted`・`--test capabilities`。
6. **変更**: `tests/extern_view.mjs`・`tests/extern_view_host.c`・`tests/fixtures/extern_view/Main.tz`、`tests/windows.mjs`。**確認**: `node tests/extern_view.mjs target/debug/tsuzuri`（native の `-O0`・`-O3`）、Windows で `node tests/windows.mjs target/debug/tsuzuri.exe`。
7. **変更**: 文書（D-63 の文言の改訂を含む）。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。PR に `ci:full`。

### Phase 2

8. **変更**: `src/abi.rs`（`view_record` が `@c_layout` を受ける）と診断の文。**確認**: `tests/extern_view.rs` の `@c_layout` のケース、`tests/extern_view.mjs` の Phase 2 の段（8・16 bit の field、`pack = 2`、`copy` ハンドルと `Maybe<H>`）。

## テスト計画

### Rust テスト（`tests/extern_view.rs`、新規）

| テスト | 内容 | 期待 |
| --- | --- | --- |
| `syntax` | 読む形・書く形・`private extern view def`、`view` を識別子に使う（`let view = 1`）、`extern "x" view def`、`extern view type`。2026 と 2027 の両方の edition で同じ結果（`tests/editions.rs` にも 1 件足す） | 受理、`E0002`（`extern_view_link_name` など） |
| `signatures` | `i32 -> R`、`i64 -> i64`、`i64 -> R -> unit`、`i64 -> ref R -> i32`、`unit -> R` | `E1008`（`extern_view_signature`） |
| `records` | 32/64-bit・`f32`・`f64`・固定長配列・入れ子の受理、`bool`・`i8u`・`i16` の field、field のない record | `E1008`（`extern_view_record`。メッセージに field の名前） |
| `call_form` | 関数値として束縛・`List` に渡す・部分適用 | `E1008`（`extern_view_call_form`） |
| `read_and_write_lower_at_the_call_site` | IR | 呼び出し側の関数に `inttoptr`、`icmp ne ptr`、`and i64` の整列のガード、field の `load`／`store`。`declare` と view の名前の `define` がない |
| `trusted_takes_no_properties` | `@trusted no_trap extern view def` | `E1037`（`trusted_view_property`） |
| `effects` | `pure def` から呼ぶ、`@no_trap` から呼ぶ、`@no_alloc`・`@terminates` から呼ぶ | `E1029`、`E1031`、受理 |
| `audit` | `tsuzuri audit` のテキストと `--json`、`--warn untrusted-extern` | `view read`／`view write`、`"symbol":null,"view":"read"`、`@trusted` のない view に `W1010` |
| `wasm_is_native_only` | `--target wasm32` で生成 | `E1008`（`abi_native_only`） |
| `programs_without_views_do_not_change` | view のない同じプログラムの IR | 基準のコンパイラの出力と同じ |

### E2E（`tests/extern_view.mjs`、新規。`tests/ffi_extensions.mjs` の形）

- C ホスト（`tests/extern_view_host.c`）は、`NMHDR` の形（`int64_t`・`uint64_t`・`uint32_t` と末尾の詰め物）、`MINMAXINFO` の形（入れ子 5 つ）、`CREATESTRUCTW` の形（途中の詰め物）、`f32`・`f64`・`int32_t[4]` を持つ構造体を置き、`(int64_t)(intptr_t)&s` を `export def` に渡す。Tsuzuri は読んだ値を返し（期待値は C の側で書く）、書く形で `MINMAXINFO` の 2 つの field を書き換え、C が全 field と詰め物（0 以外の値を置いておき、変わっていないこと）を検査する。extern が返したアドレス（`extern def host_block :: unit -> i64`）も読む。
- トラップ: アドレス 0 と、整列 8 の構造体の `+4`・整列 4 の構造体の `+2` を、`--trap-info` の build の子プロセスで呼び、0 でない終了と、標準エラーの `index out of bounds at` と view の呼び出し式の行・列を確かめる。
- native の `-O0`・`-O3`（どちらも同じ結果）。

### Windows（`tests/windows.mjs` の段）

- 早見の例が 42 で終わる（`-O0`・`-O3`）。`read_point 0` の版は `0xC000001D` で終わり、報告に呼び出しの位置が出る。
- `NmHdr`・`MinMaxInfo`・`CreateStruct` の record を export でヘッダーに出し、`windows.h` の構造体と `sizeof`・`offsetof` を `_Static_assert` で比べる。
- [W06](W06-windows-gui-test-harness.md) と [W12](W12-static-callback-lifetime.md) が done なら、ウィンドウプロシージャで `WM_GETMINMAXINFO` を書く形と `WM_NOTIFY` を読む形の段を足す（TZ05 の「TZ04 が done なら」と同じ扱い）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 新しい節「ホストのメモリの読み書き（extern view）」（構文、意味、トラップ、検査しないこと、効果と監査、Win32 のウィンドウプロシージャの `WM_NOTIFY`・`WM_GETMINMAXINFO` の例、Phase 1 の record の条件、WASM で使えないこと）。「信頼の印（@trusted）」に view は性質を取らないこと。
- `_tsuzuri/language-reference/values-and-functions/attributes.md` の `@trusted` の「書ける位置とエラー」に `trusted_view_property`。
- `docs/language.md`: 「ホスト関数のインポート」に小節「extern view」、診断の表、`tsuzuri audit` の出力の説明。`_tsuzuri/language-reference/compiler/diagnostics.md`（生成の部分は `node scripts/gen-docs.mjs`）。`tsuzuri audit` の JSON の説明がある `compiler/usage.md` と `organizing-tsuzuri/packages.md` の「権限（capabilities）」。
- D-63 の文言の改訂（この PR で行う）: `languages/strategy.md` の「採らないもの」の項を「`unsafe` と生ポインター型。低水準の処理はホストに置きます。ホストが渡したアドレスとの型付きの複写（`extern view`）だけを、`@trusted` と権限 `ffi` の監査の対象として許します。」の趣旨に、`languages/why-tsuzuri.md` の表の「生ポインタ」の行を同じ趣旨に、`_features/README.md` の「採用しない改善」の行から注記を外して出荷した形に、`_features/_completed/E12-ffi-extensions.md` の D13 に「2026-11-22 の D-63 と W24 で、`extern view` に限って改めた」の注記を足す。
- `docs/architecture.md` の「ホスト境界」の節: view の生成（呼び出し地点、`host_pointer`、P を手放さない）と `HostImport.view`。
- `README.md` の「変更の種類別」の「host ABI・import」の行に `tests/extern_view.rs`・`extern_view.mjs`。

## 受け入れ条件

- [ ] 仕様 1〜6 のとおりに動く（Phase 1。Phase 2 は着手したとき）。
- [ ] view を使わないプログラムの IR（native と wasm32）・ヘッダー・`tsuzuri audit` の出力・診断が基準と byte 単位で一致する。
- [ ] E2E が native の `-O0`・`-O3` で通る。0 と整列違反のトラップの位置が view の呼び出し式。
- [ ] Windows の段（早見の例、`windows.h` との静的表明）が通る。PR に `ci:full` を付けた。
- [ ] `TrapKind` の列と生成したバインディングのトラップの種類の表が変わっていない。
- [ ] 新しい診断は既存のコード（`E0002`・`E1008`・`E1037`）の kind で、文書に載った。`FRONTEND_FORMAT` を 1 上げた。
- [ ] strategy.md・why-tsuzuri.md・`_features/README.md`・E12 D13 の文言を出荷した形に改めた。`check-docs`・`gen-docs --check`・`check-cli-docs` が通る。

## 落とし穴

- **関数本体を作らない**: view の `HostCall` の関数本体を普通の extern と同じく出すと、空の symbol の `declare` と呼び出しができる。本体を出さない分岐を必ず通す。
- **トラップの位置**: 呼び出し地点で生成するので、`current_span` は呼び出し式。本体の中で生成すると位置が宣言になる。
- **`uses_host_abi` を真にしない**: view を host の関数と数えると、view だけのプログラムのヘッダーに `tsuzuri_alloc`・`typedef` が出て、IR の公開の symbol も変わる。一方で `%tz_record_*` の型の定義は要る。
- **tzroutine の包み**: `extern_symbols` に入れると、view の呼び出しを P を手放す包みで囲もうとする（直接の `call` がないので包みは付かないが、集める対象から外して意図を明確にする）。
- **`load` の整列**: Phase 1 の `%tz_record_*` は普通の struct なので、`read_host_record` の `load` は型の自然な整列を前提にする。`host_pointer` の整列の検査が、その前提を満たすことを保証する（検査を省かない）。
- **`HostCall` の `_ =>`**: `HostImport` に field を足したら `grep -rn "HostImport {" src/` で構築する所をすべて直す。
- **WASM の `host_pointer`**: ネイティブだけなので、線形メモリの大きさの検査の分岐は通らない。WASM を許す拡張は対象外。

## 対象外

- 生ポインター型・ポインター演算の構文・`unsafe`（D-63 で入れないと決めた）。
- スカラー（`i32` 1 つなど）や配列そのものの view（record に包む）。可変長の構造体（`DLGTEMPLATE` の後ろなど）。
- アドレスの範囲の検査（ホストの領域の大きさを知らない）、`IsBadReadPtr` のような検査、例外（SEH）で不正なアドレスから回復すること。
- 原子的な読み書き・volatile。
- WASM の線形メモリへの view（JS のホストには E13 の糊がある）。
- `extern interface`（W25）、関数ポインターの読み出し（[W22](W22-fnptr-values.md) は外部の関数ポインターを受け取らない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 綴りは `extern view def name :: i64 -> R`（読む）と `extern view def name :: i64 -> ref R -> unit`（書く）。`view` を `extern` と `def` の間の文脈キーワードとして D-55 に登録する | D-63 の共通契約の綴り。`extern` の宣言として書くので、`@trusted`・`ffi`・`audit`・`W1010` の既存の監査にそのまま乗る。新しい予約語を足さない（GUIDE-LITE §7） | 要承認（承認前は Phase 1 の手順 2 以降に着手しない） |
| D2 | アドレスの型は `i64` で、ポインターの型を足さない | D-63（利用者が承認済み）。ネイティブの target はすべて 64-bit | 既定案（実装者はこの案に従う） |
| D3 | 0 と整列違反は呼び出し地点でトラップし、種類は既存の `TrapKind::BoundsCheck` | export の入口の `host_pointer` と同じ検査と種類にそろう。`TrapKind` を増やすと、番号を使う `--trap-mode return` と、種類の名前の表を埋め込む C#・Python のバインディングの生成物が、view を使わないプログラムでも変わる。文が `index out of bounds` なのは文書で説明する | 既定案（実装者はこの案に従う） |
| D4 | view は直接・全引数の呼び出しでだけ使え、関数本体を出さない | トラップの位置を呼び出し式にでき、関数値のための本体が要らない。コールバックを取る extern（E12 D9）の前例がある。関数値として渡す用途は見当たらない | 既定案（実装者はこの案に従う） |
| D5 | Phase 1 の record は、葉が 32/64-bit の数だけのもの。`bool` と 8・16 bit は `E1008` | 今の境界の配置が C と一致する範囲だけを許す（`NMHDR`・`MINMAXINFO`・`CREATESTRUCTW` で確認）。8・16 bit と `bool` を許すと 32 bit に広がった配置で読み、黙って誤る。W21 の `@c_layout` の `bool` の扱いともそろう | 既定案（実装者はこの案に従う） |
| D6 | 表現は `HostCall` と `HostImport.view` | `ffi` の権限・`W1010`・`audit`・所有権・効果・定数評価の拒否が、普通の extern と同じ経路で働く。新しい `TypedExprKind` は 18 のファイルの `match` に足す必要があり、`_ =>` が黙って見落とす所で権限や効果が抜ける。symbol に依る所（`declare`・ヘッダー・包み・バインディング）は数が少なく、誤れば見える失敗になる | 既定案（実装者はこの案に従う） |
| D7 | 効果は `extern` と `trap` の源で、確保せず停止する。`@trusted` に性質を書くと `E1037`（`trusted_view_property`） | コンパイラが複写の効果を正確に知っている。人の書く性質は冗長か誤り（`no_trap` は 0 でトラップするので偽）になる。`extern type` の性質を拒む `trusted_type_property` の前例 | 既定案（実装者はこの案に従う） |
| D8 | `@trusted` は必須にせず、ない view は普通の extern と同じく `W1010` | E15 は信頼の印を警告にして段階的に入れられるようにした。`RtlMoveMemory` の偽の宣言と同じ危険の view だけを必須にすると、扱いが食い違う | 既定案（実装者はこの案に従う） |
| D9 | ネイティブだけで、WASM は `E1008`（`abi_native_only`） | 用途は Win32 のウィンドウプロシージャで、WASM の既存の出力を変えない。JS のホストとの受け渡しには E13 の糊がある | 既定案（実装者はこの案に従う） |
| D10 | ヘッダー・バインディングに何も出さず、tzroutine の P を手放す包みにも入れない | view はホストの関数を呼ばず、メモリを複写するだけで待たない | 既定案（実装者はこの案に従う） |
| D11 | strategy.md・why-tsuzuri.md・`_features/README.md` の「採用しない改善」・E12 D13 の文言は、Phase 1 を出荷する PR で出荷した形に改める | D-63 の「文言の改訂の時期」。言語リファレンスは今のコンパイラがすることだけを書く | 既定案（実装者はこの案に従う） |
| D12 | Phase 2 で `@c_layout`（W21 の Phase 1 の field）を受け、W21 の Phase 2 の field（`FnPtr`・`ref ZString`）を持つ record は両方の形で `E1008` | view には呼び出しの間という寿命がなく、書いた借用のポインターはすぐに宙に浮く。読んだ関数ポインターから `FnPtr` を作ると W22 の「外部の関数ポインターを受け取らない」を破る | 既定案（実装者はこの案に従う） |
