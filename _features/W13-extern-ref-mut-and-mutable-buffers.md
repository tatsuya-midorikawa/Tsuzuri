# W13: `extern` の `ref mut`（copy-in/copy-out）と、呼び出し先が書く可変バッファ・`@count`

| 項目 | 内容 |
| --- | --- |
| ID | W13 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | [W20](W20-zstring.md)（`ZString`）、[W21](W21-c-layout-records.md)（`@c_layout` の record の `ref mut`）、[W27](W27-win32-package-from-winmd.md)（`[Out]` → `ref mut`、NativeArrayInfo → `@count`） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E05・E06 の除外の解除、D-53）。決定事項 D1・D2 が要承認、ほかは既定案。方針は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | `src/llvm_imports.rs` の `host_call`（record の引数を `host_result_slot` へ `write_host_record` で写す経路、compact 文字列の写しを全引数の評価後に作る `widen`）と `read_host_result`、`src/llvm_abi.rs` の `read_host_record`・`write_host_record`・`c_parameters`、`src/abi.rs` の `parameter`・`Buffer`、`src/parser.rs` の `attributes`・`trusted_attribute` と `Program.trusted`（名前と位置で宣言に結ぶ脇の表。D-53） |
| 主な影響ファイル | `src/abi.rs`、`src/check.rs`、`src/parser.rs`、`src/syntax.rs`、`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT`）、`src/formatter.rs`、`src/llvm_imports.rs`、`src/llvm_abi.rs`、`src/llvm.rs`、`src/effects.rs`、`src/bindings.rs`、`src/diagnostic_kinds.rs`、`src/explain/codes/E1008.md`、`tests/extern_ref_mut.rs`（新規）、`tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`、`spec/rules/ABI.md`、文書 |

## 実装者向け早見

- 範囲: native の `extern` の引数に、(1) `ref mut R`（スカラーの record）と `ref mut T`（スカラー。W18 の後は `copy` ハンドルも）を呼び出しごとの一時領域で copy-in/copy-out し、(2) `ref mut [T]`・`ref mut [T..]`（T は `i64`・`f64`・`ubyte`・`i16u`・`i32`・`i32u`）を写さずに `T *ptr, 長さ` で渡す。(3) 宣言の前の属性 `@count(位置, i32 | i32u | i64 | i64u | none)` でバッファの長さの C の型を選ぶ。wasm32・wasm64 は `E1008`。
- 最初に読む: 「現状」→ 仕様 1〜4 → `src/llvm_imports.rs` の `host_call`（全体で 150 行）→ `src/llvm_abi.rs` の `write_host_record`・`read_host_record`。
- 最初に書くテスト: `tests/extern_ref_mut.rs` の `ref_mut_record_is_copied_in_and_out`（IR に copy-in → `call` → copy-out の順がある）→ E2E の `ref_mut_rect`。
- 触ってはいけない: `export def` とコールバックの引数の規則（`abi::parameter`・`abi::callback` は変えない）、今の共有バッファ・record の引数の IR、WASM の出力。
- 完了の見分け方: 次のプログラム（今は `E1008`）が Windows のメッセージ専用の窓で `GetClientRect` と `GetWindowTextW` の結果を受け取る。

```tsuzuri
record Rect { left: i32, top: i32, right: i32, bottom: i32 }
extern type Hwnd

@trusted
extern "GetClientRect" def get_client_rect :: ref Hwnd -> ref mut Rect -> i32

@trusted
@count(2, i32)
extern "GetWindowTextW" def get_window_text :: ref Hwnd -> ref mut [i16u..] -> i32

def title :: ref Hwnd -> string = \hwnd ->
    let mut units: [i16u] = new [i16u](256, \_ -> 0)
    let n = get_window_text hwnd (ref mut units)
    String.from_code_units (Array.truncate (n as i64) (ref units))
```

## 目的

Win32 の多くの関数は、呼び出し側の構造体やバッファへ結果を書く（`GetMessageW(&msg)`、`BeginPaint(&ps)`、`GetClientRect(&rc)`、`GetWindowTextW(hwnd, buf, n)`）。今の `extern` は値と共有借用しか渡せず、record は正規化した一時領域へ写して渡すので、呼び出し先の書き込みは Tsuzuri から見えない。D-63 の範囲（生ポインターの型・ポインター演算・`unsafe` を足さず、`extern` の宣言として書いて `@trusted` と `ffi` で監査する形）で、最も安く最も広い範囲の API を契約の中に入れるのがこのチケットである。排他借用は呼び出しの間だけ貸し、書き戻しはコンパイラが生成する。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認。承認前は手順 2 以降に着手しない（手順 1 の基準取りはしてよい）。
- `grep -n "fn host_call\|fn host_result_slot\|fn read_host_result" src/llvm_imports.rs` と `grep -n "pub fn parameter\|pub enum Buffer" src/abi.rs` が「現状」と同じ形を示す。
- W14 が done かを確かめる（done なら、W14 の `abi::c_signature` にこのチケットの形を足す。「前提とする他チケットのインターフェース」）。W18 が done なら `ref mut H`（`copy` ハンドル）を手順 2 に含める。

### 停止条件

- **S1** `ref mut` の実引数を、所有権の検査（`src/ownership.rs` の `E::HostCall` の腕）が通常の関数呼び出しの排他借用と同じに扱わず、検査を変えないと同じ場所の別名（`f (ref mut r) (ref r)`）を `E1014` にできない。
- **S2** 新しい機能を使わないプログラム（`tests/fixtures`・`examples` の全 `Main.tz`）の `--emit llvm -O1`（native と `--target wasm32`）か `check --json` が 1 byte でも変わる（`unsupported_extern_signature` の文の変更だけは除く）。
- **S3** `ref mut` のバッファの要素の格納が、C から `T *` として書いてよい連続した書き込み可能な領域でない場合が見つかる（読み取り専用の静的データを指しうる、要素の幅が C の型と違う、など）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

- `src/abi.rs` の `parameter` は `abi_scalar`・`scalar_record`・`Reference(inner, false)`（共有借用の `Buffer` かスカラーの record）だけを真にし、可変借用 `Reference(_, true)` は常に偽。`src/check.rs` の `check_modules_collect` は、`export def` の判定（`UnsupportedExportSignature`）と `extern` の判定（`UnsupportedExternSignature`）の両方でこの関数を使う。
- `src/abi.rs` の `Buffer` は `I64`・`F64`・`UByte`・`String`・`Utf8String` だけで、`[i16u]`・`[i32]`・`[i32u]` の要素の種類はない。
- 再現（`target/debug/tsuzuri.exe check`）: `extern` の引数の `ref mut Rect`・`ref mut i32`・`ref mut [i64]`・`ref mut [i64..]`・`ref mut [i16u]`・共有の `ref [i16u]` は、どれも `E1008`（kind `unsupported_extern_signature`、文 `extern signatures support scalar/unit, shared ABI buffers and scalar records, and owned ABI buffer or scalar record results`）。
- `src/llvm_imports.rs` の `host_call` は、record の引数（値でも `ref` でも）を `host_result_slot` の一時領域（関数の入口の `alloca`、align 16、ゼロで初期化）へ `write_host_record` で写して `ptr` を渡し、呼び出しの後は何も読み戻さない。バッファは `extractvalue` した `ptr` と `i64` の 2 つ。compact 文字列の UTF-16 の写しは全引数の評価後に作り、呼び出しの後に `free_copies` で解放する。record・バッファの結果は先頭の out ポインターから `read_host_result` で読む。`--emit llvm` で確かめると、`ref [i64]` と `ref Rect` を取る `extern "fill"` は `declare i32 @fill(ptr, i64, ptr)`。
- `src/llvm_abi.rs` の `c_parameters` はヘッダーの引数を `const T *argN`（record）・`const T *argN_ptr, int64_t argN_len`（バッファ）と書き、`record_types`・`header_types` は `Reference(inner, false)` だけを開く。`src/bindings.rs` の `descriptor` は `Reference(_, true)` を `unreachable!` にする。
- 宣言の前に `@count(2, i32)` を書くと `E0002`（`expected_expression_after_attribute`）、`@trusted` の次の行に書くと `@trusted` が `E1037`（`trusted_wrong_target`）になる。宣言の前の属性の読み込み口は `src/parser.rs` の `attributes`（`@cpu`・効果の属性・`@trusted`）。
- 所有権: `def g :: ref mut Rect -> ref Rect -> i32` を `g (ref mut r) (ref r)` と呼ぶと `E1014`（`access conflicts with a live borrow`）。`def h :: ref mut [i16u..] -> i32` には `ref mut buf[0..2]` も `ref mut buf`（配列全体の排他スライスへの変換）も渡せる。
- 決定の記録: E05 の「対象外」に「mutable host buffer parameter」、本文に「`&mut` parameter は E05 では export 不可」。E06 は import の型に「`&mut` は不可」。E12 D5 は `ref mut H` を不可とする。docs/language.md「ホスト関数のインポート」（453 行付近）は「可変参照（`ref mut`）…を引数や戻り値に指定することはできず」と書く。
- 配置（このチケットの「安い中間段」の根拠。開発版の `--emit header` と MinGW の `windows.h` を `_Static_assert` で照合した）: ハンドルを `i64` の field にした `record Msg { hwnd: i64, message: i32u, wparam: i64u, lparam: i64, time: i32u, pt: Point, lprivate: i32u }` は `sizeof(MSG) == 48` と各 field の `offsetof` が一致する。`record PaintStruct { hdc: i64, erase: i32, paint: Rect, restore: i32, inc_update: i32, reserved: [i32u; 8] }`（`rgbReserved` を `[i32u; 8]`）は `PAINTSTRUCT` と、`Rect` は `RECT` と一致する。今の正規化は 32/64-bit の field を広げないので、`ref mut` だけで `GetMessageW`・`PeekMessageW`・`BeginPaint`・`GetClientRect` が契約に入る（NULL の `HWND` は引数を `i64` にして 0 を渡す。`Maybe<H>` は W15）。

## 仕様

### 1. 書ける形（native の `extern` の引数だけ）

| 引数の型 | 条件 | C の引数（ヘッダー） | 渡し方 |
| --- | --- | --- | --- |
| `ref mut R` | R が `abi::scalar_record`（値・共有借用の record と同じ条件） | `tz_record_<…> *argN`（`const` なし） | copy-in → 呼び出し → copy-out（仕様 2） |
| `ref mut T` | T が `i8`〜`i64`・`i8u`〜`i64u`・`f32`・`f64`・`bool` | 正確な幅の `int8_t *`・`uint16_t *`・`int64_t *`・`float *`・`double *` など。`bool` は `int32_t *`（Win32 の `BOOL`） | 同上（スカラーの一時領域） |
| `ref mut H` | H が W18 の `extern type copy`（W18 の後だけ） | `tz_handle_<…> *argN` | 同上 |
| `ref mut [T]`・`ref mut [T..]` | T が `i64`・`f64`・`ubyte`・`i16u`・`i32`・`i32u` | `T *argN_ptr, <長さ> argN_len` | 写さない。要素の領域をそのまま渡す |
| `ref [T]`（共有） | T に `i16u`・`i32`・`i32u` を足す | `const T *argN_ptr, <長さ> argN_len` | 今のバッファと同じ |

- 要素の C の型: `i64` → `int64_t`、`f64` → `double`、`ubyte` → `uint8_t`、`i16u` → `uint16_t`、`i32` → `int32_t`、`i32u` → `uint32_t`。長さは仕様 3（既定は `int64_t`）。
- 書けない形（`E1008`、kind `unsupported_extern_signature`）: `ref mut string`・`ref mut utf8string`（文字列は不変で、UTF-8 の妥当性を壊せる）、`ref mut [bool]`・`ref mut [char]` とそのほかの要素（不正なビットの並びを持つか、C の型が決まらない）、非 `copy` のハンドルの `ref mut H`（E12 D5 のまま。所有するハンドルを書き換えると前の値が黙って失われる）、スカラーの record でない record の `ref mut`、結果の位置の可変借用。スカラーの共有借用 `ref T` も今どおり `E1008`（値で渡せばよく、C の `const T *` の API は少ない）。
- `export def` の引数とコールバック（`extern` の関数型の引数）の `ref mut` と新しい要素の種類は、今どおり `E1008`（決定事項 D9）。

### 2. copy-in/copy-out の意味

- 引数は今どおり左から右へ評価する。`ref mut R`・`ref mut T` の実引数は、その位置で借用先の値を読み、呼び出しごとの一時領域へ C の配置で書く（copy-in）。全引数の評価と `@count` の範囲の検査（仕様 3）の後に呼び、戻った直後に一時領域を読んで借用先へ書く（copy-out）。copy-out は結果の変換・`free_copies`・out の結果の読み出しより前で、宣言の順に行う。
- ホストは呼び出しの間だけ一時領域を読み書きしてよく、戻った後に保持・書き込みしてはならない（共有借用の今の契約と同じ）。
- ホストが書かなかった field は copy-in の値のまま戻る（in/out）。`bool` は 0 か非 0 を copy-out で正規化する（`decode_host_scalar`）。record の 8/16-bit の field は今の正規化（32-bit で渡し、copy-out で下位のビットを取る）。
- 実引数は排他借用の場所（`ref mut x`、`ref mut x.field`、`ref mut xs[a..b]`）。同じ呼び出しの別の実引数が同じ場所を借りれば、通常の呼び出しと同じく `E1014`。コールバックは捕捉を持たないので、呼び出しの間に借用先へ届く経路はない。
- `ref mut [T]`・`ref mut [T..]` は要素の領域の先頭と要素の数を渡す。ホストは呼び出しの間 `0 ≦ i < 長さ` の要素を読み書きしてよく、長さは変えられない。長さ 0 の配列は NULL と 0 で渡ることがある（今の共有バッファと同じ）。要素の型に不正な値はないので、戻った後の検査はない。

### 3. `@count`

```tsuzuri
@trusted
@count(4, none)
extern "SendMessageW" def send_get_text :: ref Hwnd -> i32u -> i64u -> ref mut [i16u..] -> i64
```

- 文法: `extern` の宣言の前の属性 `@count(<位置>, <長さの型>)`。位置は Tsuzuri の引数の 1 始まりの番号（`unit` の引数も数える）。長さの型は `i32`・`i32u`・`i64`・`i64u`・`none`。D-53 の宣言の前の属性として `@trusted` と任意の順に書け、違う位置へなら複数書ける（`MultiByteToWideChar` のように 2 つのバッファを持つ関数）。両 edition で使える（今は `E0002` なので意味の変わるプログラムはない）。
- 意味: その位置のバッファ（`ref [T]`・`ref mut [T]`・`ref mut [T..]`・`ref string`・`ref utf8string`）の長さの C の型を選ぶ。属性がなければ今どおり `int64_t`。`none` は長さの引数を渡さない（ポインターだけ）。長さは要素の数（`string` は UTF-16 の単位、`utf8string`・`[ubyte]` はバイト）。
- 範囲: `i32` は長さが 2^31 − 1 を、`i32u` は 2^32 − 1 を超えると、呼び出しの前にトラップする（`TrapKind::BoundsCheck`。飽和も切り捨てもしない）。検査は全引数の評価の後、compact 文字列の写しを作る前に行い、トラップが写しを残さないようにする。`i64`・`i64u` は検査しない（長さは 0 以上の `i64`）。
- `none` の安全性: ホストは長さを受け取らないので、書いてよい範囲は別の引数（呼び出し側が渡す）か API の約束で決まる。その正しさは宣言の `@trusted` の責任で、コンパイラは確かめない（D-63 の背景 1: 安全性の実体はすでに extern の宣言の正しさにある）。文書は、`none` の extern を長さを検査する Tsuzuri の関数で包む書き方を示す。
- 誤り: 位置が引数の数の外は `E1008`（kind 案 `count_position_out_of_range`）、その位置がバッファでない（W20 の `ref ZString` を含む）は `E1008`（`count_not_buffer`）、同じ位置への 2 つ目は `E1008`（`count_repeated`）、`extern` の関数でない宣言（`def`・`export def`・`extern type`）の前は `E0002`（`misplaced_count_attribute`。`@cpu` の `misplaced_cpu_attribute`・W21 の `@c_layout` と同じく属性の置き場の誤りは構文の `E0002`。`E1032` は D-53 の効果の属性の番号なので使わない）、括弧・数・型の名前の書き損じは `E0002`（`count_attribute_syntax`）。

### 4. ターゲットとヘッダー

- native（Windows・Linux・macOS）だけ。wasm32・wasm64 で生成する `extern` が `ref mut` の引数、新しい要素の種類のバッファ（共有を含む）、`@count` のどれかを持てば `E1008`（kind 案 `abi_native_only`。位置は extern の名前）。生成しない（届かない）extern は拒否しない（未使用の extern は import を作らない今の規則に合わせる）。
- `--emit header` の、リンク名のない extern（`tsuzuri_host_*`）のプロトタイプは仕様 1・3 の C の型。リンク名を明示した extern はプロトタイプを出さない（今どおり）。

## 設計

### 検査（`src/abi.rs`・`src/check.rs`・構文）

- `abi::extern_parameter(ty, types) -> bool` を足す（`parameter(ty, types)` か仕様 1 の形）。W15 も同じ名前の関数に `Maybe<ref R>` などを足すので、先に入ったほうが作り、後のほうが形を足す（W15 の設計と同じ取り決め）。`check_modules_collect` の `UnsupportedExternSignature` の判定だけをこれに替え、`UnsupportedExportSignature` は `parameter` のまま。
- `Buffer` に `I16U`・`I32`・`I32U` を足す（`name`・`c_element`・`width` は `i16u`/`uint16_t`/2、`i32`/`int32_t`/4、`i32u`/`uint32_t`/4）。`Buffer::of`（結果・export・`header_types` が使う）は今の 5 つだけを返し、extern の引数は新しい `Buffer::of_extern(ty)`（8 つ）で決める。可変のバッファは `Reference(Array(T), true)` と `Reference(ArrayView(T), true)` の両方。
- `@count`: `src/parser.rs` の `attributes` に `count_attribute` を足し、`Program.extern_counts: Vec<CountAttribute { target: Ident, position: u32, length: CountLength, span: Span }>` を `trusted` と同じく脇の表に持つ（宣言とは名前と位置で結ぶ）。`src/syntax_codec.rs` の符号化に足して `FRONTEND_FORMAT` を 1 上げ、`src/formatter.rs` は `@trusted` と同じ所に出力する。`check_modules_collect` は `HostImport` に `counts: Vec<(usize, CountLength)>`（引数の添字の順。属性がなければ空）を足し、仕様 3 の誤りを出す。
- `UnsupportedExternSignature` の文を `extern parameters support scalars, unit, extern handles, callbacks, scalar records by value or by shared or mutable borrow, mutable scalars, and shared or mutable buffers of i64, f64, ubyte, i16u, i32 or i32u (plus string and utf8string inputs); results support scalars, unit, handles, owned buffers and scalar records` に改める（受け入れ条件の唯一の例外）。

### 生成（`src/llvm_imports.rs`・`src/llvm_abi.rs`）

- `host_call` の引数の分岐に足す:
  - `Reference(R, true)`（record）: `self.expression` は借用先の `ptr`。`load` して `host_result_slot(R)` へ `write_host_record` し、`ptr {slot}` を渡して `(slot, 借用先, R)` を copy-out の表に積む。
  - `Reference(T, true)`（スカラー・`copy` ハンドル）: 新しい `host_scalar_slot(T)`（`allocas` に C の型の `alloca`。`bool` は `i32`、8/16-bit は正確な幅）へ copy-in（`bool` は `zext`）。
  - `Reference(Array(T), true)`: 借用先の `%tz.array` を `load` して `extractvalue` 0・1。`Reference(ArrayView(T), true)`: 値がそのまま `%tz.array`。
  - 長さ: `HostImport.counts` に従い、`i32`・`i32u` は `icmp ule i64 %len, <上限>` と `self.guard(…, TrapKind::BoundsCheck)` の後に `trunc`。`none` は長さを積まない。検査は `widen` の写しの前。
  - 呼び出しの直後、copy-out の表を積んだ順に一時領域を読み（record は `read_host_record`、スカラーは `load` と `decode_host_scalar`）、借用先へ `store` する。その後に今の `free_copies` と結果の変換。
- `c_parameters`・`record_types`・`header_types`・`handle_typedefs` は `Reference(inner, true)` も開く。可変には `const` を付けず、長さの型は `counts` に従う（`c_parameters` は `HostImport` を読む）。
- `src/bindings.rs`: JS の記述子（`descriptor`）は WASM の import だけに使われ、WASM は仕様 4 で拒否されるので変えない。拒否が記述子の生成より前に効くことをテストで固定する。native の `--emit bindings-cs`・`py`・`cpp` は import の記述子を使わない（`handles`・`records` は `Reference(inner, _)` を開く）。
- 効果: `@count` の `i32`・`i32u` を持つ extern の呼び出しは `trap` を持つ（`src/effects.rs` の `HostCall` の分類に足す。`@trusted no_trap` は C の関数の性質で、生成する検査には効かない）。copy-in/copy-out はスタックの一時領域だけで `alloc` を足さない。

### WASM の拒否（`src/llvm.rs`）

- W15 の設計と同じ共有の口を使う（W15 の「設計」の wasm の拒否）: `host_call` は `self.globals.wasm` のとき仕様 4 の形の引数に出会ったら、その extern の関数の番号を `Globals` の集合（`BTreeSet<usize>`）に記録する。`emit_program` は関数の生成の後、記録があれば最も小さい番号の extern の名前の位置で `E1008` `abi_native_only` を返す。生成した extern だけが記録されるので、届かない extern は拒否されず、新しい到達の規則は要らない。W15 が先に入っていればその口に条件を足し、W13 が先ならこのチケットが口と kind を作る（W15・W16・W20・W21・W22・W24・W25 が後から条件を足す）。

## 前提とする他チケットのインターフェース

- W14（任意。依存ではない）: `abi::c_signature`（下げた C の引数と結果の並び）。W14 が先に done なら、仕様 1・3 の形（`ref mut` は `ptr` 1 つ、バッファは `ptr` と `@count` の長さの型、`none` は `ptr` だけ）をそこに足す。W13 が先なら W14 が足す。
- W18（任意）: `extern type copy` かどうかの判定 `Type::Handle(_, HandleKind::Copy)`（W18 の D3: 種類は型の中に持ち、別表は作らない）。W18 が done でなければ `ref mut H` は今どおり `E1008`。

## 他チケットへ提供するインターフェース

- `abi::extern_parameter`、`Buffer::of_extern` と `Buffer::{I16U, I32, I32U}`、`HostImport.counts`、`host_call` の copy-out の表（W21 が `@c_layout` の record を同じ表に載せる）。
- `E1008` の kind `abi_native_only` と、wasm の生成で記録して最後に拒否する共有の口（W15 と同じ設計。W15・W16・W20・W21・W22・W24・W25 が条件を足す。先に入ったチケットが作る）。
- `@count`（W27 の生成器が NativeArrayInfo の CountParamIndex から作る。表せない形は「対象外」）。

## 実装手順

1. **変更**: なし（下書きは `target/scratch/w13/`）。**内容**: `node scripts/repo-kb.mjs brief W13`、着手条件の `grep`。`tests/fixtures`・`examples` の全 `Main.tz` について `--emit llvm -O1`（native と `--target wasm32`）と `check --json` を保存する（TZ05 の「実装と検証」の比較と同じ手順）。**確認**: 保存した入力の数と、単独で build できない入力を記録する。
2. **変更**: `src/abi.rs`・`src/check.rs`・`src/llvm_imports.rs`・`src/llvm_abi.rs`。**内容**: `ref mut R`・`ref mut T`（W18 が done なら `ref mut H`）の検査・生成・ヘッダーと文の変更。**確認**: T0、`cargo test --locked --test extern_ref_mut`（record とスカラーのテスト）、`cargo test --locked --test ffi_extensions --test host_abi`、T1。
3. **変更**: 同じファイル。**内容**: `Buffer` の新しい種類、`ref mut [T]`・`ref mut [T..]`・共有の新しい要素。**確認**: `mutable_buffer_passes_storage`・`header_prototypes`、T1。
4. **変更**: `src/parser.rs`・`src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/check.rs`・`src/llvm_imports.rs`・`src/effects.rs`・`src/diagnostic_kinds.rs`。**内容**: `@count`（G23 の手順で kind を登録する）。**確認**: `count_types`・`rejections`、`cargo test --locked --test formatter`（`@count` の往復）、T1。
5. **変更**: `src/llvm.rs`・`src/llvm_imports.rs`。**内容**: WASM の `abi_native_only`（共有の口。W15 が先ならそこへ条件を足す）。**確認**: `wasm_rejects_native_only_forms`、T1。
6. **変更**: `tests/ffi_extensions_host.c`・`tests/ffi_extensions.mjs`（W06 が done なら `tests/windows.mjs` も）。**内容**: テスト計画の E2E。**確認**: `cargo build && node tests/ffi_extensions.mjs target/debug/tsuzuri` を `-O0`・`-O3` と `--backend fast` で。
7. **変更**: 文書・`spec/rules/ABI.md`・`src/explain/codes/`。**内容**: 「ドキュメント」の節。手順 1 の保存物と比べる。**確認**: T2、`node scripts/gen-docs.mjs --check`、`node scripts/check-docs.mjs <変更したページ>`。PR に `ci:full` を付ける。

## テスト計画

| Rust（`tests/extern_ref_mut.rs`） | 内容 |
| --- | --- |
| `ref_mut_record_is_copied_in_and_out` | IR に、一時領域への store → `call` → 一時領域の load → 借用先への store がこの順にある。`declare` の引数は `ptr` |
| `ref_mut_scalar_widths` | `ref mut i16u` は `alloca i16`、`ref mut bool` は `alloca i32`、`ref mut f64` は `alloca double` |
| `mutable_buffer_passes_storage` | `ref mut [i16u..]` と `ref mut [i32]` の `call` の引数が配列の `ptr` と長さで、`alloca` の写しも `@tz.alloc` もない |
| `count_types` | `@count(2, i32)` の `declare` が `(ptr, ptr, i32)` で `icmp ule i64 …, 2147483647` の guard がある。`i32u` は 4294967295、`none` は長さなし、`i64` は属性なしと同じ IR |
| `rejections` | 仕様 1・3 の誤りのコードと kind。`f (ref mut r) (ref r)` が `E1014`。export の `ref mut Rect` とコールバックの `ref mut` が今と同じ `E1008` の kind |
| `wasm_rejects_native_only_forms` | `llvm::emit_target(…, true)` が `abi_native_only`。届かない extern だけなら成功する |
| `header_prototypes` | リンク名のない extern のヘッダーが `tz_record_… *arg0`・`uint16_t *arg1_ptr, int32_t arg1_len` |
| 決定性 | 各テストで 2 回の `emit` が一致する |

E2E（`tests/ffi_extensions.mjs` と `tests/ffi_extensions_host.c`、native、`-O0`・`-O3`、`--backend fast`）。期待値は C の側で独立に求める（GUIDE-LITE §2 規則 10）:

- `ref_mut_rect`: C が初期値を読んで記録し（copy-in の確認）、各 field に別の値を書く。Tsuzuri が読む値を C の期待値と比べる。
- `ref_mut_narrow`: `i8`・`i16u`・`bool` の field と、`ref mut i16u`・`ref mut bool`（C が 2 を書くと `true`）。
- `ref_mut_untouched`: C が何も書かないと値が保たれる。`let mut a = <定数>` の `ref mut a` も書ける（S3）。
- `fill_window`: `ref mut buf[2..5]` に C が書くと窓の外は変わらない。長さ 0 の配列（NULL と 0）。
- `count_lengths`: C が `int32_t`・`uint32_t` の長さを受け取る。`none` の extern は C が別の引数の長さを使う。範囲外のトラップは 2^31 個の配列が要るので E2E にせず、IR の guard を Rust で固定する。
- Windows（W06 が done なら `tests/windows.mjs`、まだなら C の補助）: メッセージ専用の窓の `GetClientRect`、`SetWindowTextW` の後の `GetWindowTextW`、`PeekMessageW` の `ref mut Msg`。

## ドキュメント

- docs/language.md「ホスト関数のインポート」: 引数の型の列挙、可変参照の禁止の文の置き換え、copy-in/copy-out とバッファの契約、`@count`、WASM の `E1008`。「不透明ハンドル」の `ref mut H` の文（W18 の後の `copy`）。
- `_tsuzuri/language-reference/compiler/native-interop.md`「ホスト関数を import する」に可変借用と `@count` の節と C の型の表。`values-and-functions/attributes.md` に `@count` の節と、冒頭の属性の一覧（今は「10 個」。着手時の数から 1 増やす）。`compiler/diagnostics.md` の kind の表（`node scripts/gen-docs.mjs`）。`src/explain/codes/E1008.md`・`E0002.md`。
- `spec/rules/ABI.md`（番号は着手時の最大の次）: 「An extern `ref mut` record or scalar argument MUST be copied in before the call and copied back after it returns.」「An extern `ref mut [T]` argument MUST pass the element storage and its length without a copy.」「A buffer length that does not fit its `@count` type MUST trap before the call.」と適合性テスト。
- README は記述が誤りにならないので変えない。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動く（native の `-O0`・`-O3`、`--backend fast`）。
- [ ] 新しい機能を使わないプログラムの IR・object・header・診断が、手順 1 の保存物と 1 byte も変わらない（例外は `unsupported_extern_signature` の文だけ）。WASM の既存の出力も変わらない。
- [ ] `E1008`・`E0002` の新しい kind を登録し、文書・`spec/rules/ABI.md`・explain を更新した。
- [ ] T2 が通る。PR に `ci:full` を付けた。

## 落とし穴

- `abi::parameter` を広げると `export def` も広がる（export の wrapper は可変を書き戻さない）。extern だけの判定を分ける。
- 排他スライス `ref mut [T..]` の値は `%tz.array` そのもの、`ref mut [T]` は `%tz.array` の場所への `ptr`。取り違えると長さに `ptr` を読む。
- `host_result_slot` は入口の `allocas` に積むので、ループの中の呼び出しでも領域は 1 つで、呼び出しごとの copy-in が上書きする（今の record の引数と同じ）。copy-out を積む表は呼び出しごとに作る。
- コールバックを取る extern（`import.callbacks`）は呼び出し側で `host_call` を出す（`src/llvm.rs` の `Call` の分岐）。実引数が `Borrow(place, true)` の式のまま来るこの経路もテストする。
- tzroutine のプログラムは `with_blocking_externs` が `call` の前後に `tsuzuri_tz_blocking_enter/exit` を挟む。copy-out はその後になるが、スタックの一時領域なので問題ない。
- `ref mut i16u` の C の型は `uint16_t *`（ABI-010 の「値の `i16u` は `uint32_t`」とは別。ポインターの先は C の型の幅で書かれる）。

## 対象外

- WASM（JS の糊の書き戻し）、`export def` とコールバックの `ref mut`。
- `@c_layout` の record の `ref mut`（W21 の Phase 1 が同じ経路に足す）、`ref mut Maybe<H>`（W15・W21）、ハンドルを出力する引数（`void **`。W25）。
- 長さが前にある・バイト数で数える・`DWORD *` で出入りする長さ（`@count(…, none)` と明示の整数の引数で書く）、値渡しの struct。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | native の extern の引数についてだけ、E05 の対象外「mutable host buffer parameter」・本文の「`&mut` parameter は不可」、E06 の「`&mut` は不可」、E12 D5 の「`ref mut H` は不可」を解除する（`copy` ハンドルだけ） | 書き戻しはコンパイラが生成し、借用は呼び出しの間だけ。安全性の前提（宣言の正しさ）は D-63 の背景 1 のとおり変わらない | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | `@count` を D-53 の宣言の前の閉じた属性の集合に足す（D-55 の「属性」の行にも登録） | Win32 の長さは `int`・`DWORD` で、`int64_t` では渡せない。名前と位置で結ぶ脇の表は `@trusted` と同じ形 | 要承認（承認前は手順 4 に着手しない） |
| D3 | record とスカラーは呼び出しごとの一時領域で copy-in/copy-out する（直接のアドレスは渡さない） | 内部の配置（並べ替え・`i1` の `bool`）と C の配置を独立に保てる。記録の型でも経路が 1 つで、ホストが契約を破って後から書いても Tsuzuri の値を壊さない | 既定案（実装者はこの案に従う） |
| D4 | 可変バッファは写さずに要素の領域を渡す | 要素の型に不正な値がなく、排他借用で別名がないので写しは不要。大きいバッファの費用を増やさない | 既定案（実装者はこの案に従う） |
| D5 | 要素の種類は `i64`・`f64`・`ubyte`・`i16u`・`i32`・`i32u`（共有の `ref [T]` にも新しい 3 つを足す） | Win32 の `WCHAR`・`INT`・`DWORD` のバッファを覆う。共有と可変で書ける型を揃える。`bool`・`char` は不正な値を持つので除く | 既定案（実装者はこの案に従う） |
| D6 | `ref mut T` の 8/16-bit は正確な幅、`bool` は `int32_t`（`BOOL`） | ポインターの先は C の型の幅で書かれる（`WORD *`）。`bool` は E05 の正規化と Win32 の `BOOL` に合わせる | 既定案（実装者はこの案に従う） |
| D7 | `@count` の範囲外は呼び出しの前に `TrapKind::BoundsCheck` でトラップ | 共通契約（飽和しない）。`TrapKind` を増やさない。ホストのポインターの検査（`host_pointer`）と同じ種類 | 既定案（実装者はこの案に従う） |
| D8 | native だけ。WASM は届いた extern を `E1008` `abi_native_only` | W チケットの共通規則（新しい ABI の種類は native だけ、WASM の出力は変えない）。JS の糊の書き戻しは別の設計が要る | 既定案（実装者はこの案に従う） |
| D9 | `export def` とコールバックの引数の `ref mut` は今どおり `E1008` | ホストから Tsuzuri へ可変のアドレスを渡す逆向きは、wrapper の書き戻しと E13 の生成器の変更が要り、Win32 の要求にない | 既定案（実装者はこの案に従う） |
| D10 | `ref mut [T]`（配列への可変参照）と `ref mut [T..]`（排他スライス）の両方を同じ C の形で受ける。文書は部分を渡せる `ref mut [T..]` を勧める | 共通契約の綴りは `ref mut [T]`。言語では部分の窓は排他スライスで、`ref mut [T]` に排他スライスを渡すと `E1005` になる（docs/language.md 3114 行付近） | 既定案（実装者はこの案に従う） |
| D11 | 診断は既存のコードの新しい kind（`E1008` の `abi_native_only`・`count_position_out_of_range`・`count_not_buffer`・`count_repeated`、`E0002` の `misplaced_count_attribute`・`count_attribute_syntax`） | D-51・E12 D12 の前例（新しいコードを割り当てない） | 既定案（実装者はこの案に従う） |
