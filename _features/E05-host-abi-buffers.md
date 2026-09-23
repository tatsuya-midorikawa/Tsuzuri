# E05: ホスト ABI の拡張（バッファ・スカラーレコード）
| 項目 | 内容 |
|---|---|
| ID | E05 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | C03 |
| 後続 | F07 |
| 状態 | todo |
| 主な影響ファイル | `src/check.rs`, `src/llvm.rs`, `src/driver.rs`, `src/runtime/string.ll`, `src/runtime/heap-native.ll`, `src/runtime/heap-wasm.ll`, `src/main.rs`, `tests/e2e.mjs`, `examples/native`, `examples/web`, `examples/desktop`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

`export def` のホスト ABI を、現行の scalar 限定から、借用入力バッファ、戻り値バッファ、スカラーだけのレコードへ拡張する。

現状では配列・文字列・レコードを C/JS host と直接やり取りできず、複雑なデータは scalar 関数に分解する必要がある。

C03 の slice により `&[T]` を言語内で表せるため、host から borrowed buffer を安全に渡す ABI を定義する。

戻り値の配列・文字列は Tsuzuri が所有する heap buffer を host へ移し、host が明示的に `tsuzuri_free` で解放する。

raw LLVM IR を直接生成しているため、C の by-value struct ABI classification に依存しない。

そのため scalar-only record は pointer 渡し/out pointer に統一する。

## 現状

`src/check.rs` の `Type::exportable` は 8/16/32/64-bit integer、f32、f64、bool だけを true にする。

`src/check.rs` の `check_modules` は `export def` の parameter がすべて `exportable`、result が `exportable` または `unit` でなければ `E1008`。

`src/llvm.rs` の `c_type` は scalar と `unit` だけを C type へ変換する。

`src/llvm.rs` の `abi_type` は i8/i16/bool を i32 に正規化し、i64/f32/f64/unit を直接扱う。

`src/llvm.rs` の `header` は `function.signature.parameters` を `c_type argN` にして `tz_<name>` prototype を出す。

`src/llvm.rs` の `export_wrapper` は scalar ABI parameter を内部 LLVM type へ変換し、`@tz.fn.Module.name` を呼ぶ。

`src/llvm.rs` の `export_wrapper` は bool result を 0/1 i32 に、i8/i16 result を i32 に正規化する。

`src/llvm.rs` の `llvm_type` は `string` を `%tz.string = { ptr, i64 }`、array を `%tz.array = { ptr, i64 }` にする。

`src/runtime/heap-native.ll` は internal `@tz.alloc` / `@tz.free` を malloc/free で実装する。

`src/runtime/heap-wasm.ll` は internal `@tz.alloc` / `@tz.free` と 16 MiB 上限付き free list を持つ。

`src/runtime/string.ll` は `@tz.string.new`, `@tz.string.allocate`, `@tz.string.copy`, `@tz.string.concat`, `@tz.string.equal` を持つ。

`src/driver.rs` は WASM link 時に exported functions だけを `--export=tz_<name>` する。

`tests/e2e.mjs` は scalar ABI の C host と WASM host を検査する。

`examples/native/main.c` は generated header の scalar `tz_next_position` / `tz_next_velocity` を呼ぶ。

`examples/web/simulation.mjs` は WASM export scalar functions だけを呼ぶ。

## 仕様

### ABI で扱う型

E05 で `export def` に許す追加型は次の通り。

入力 parameter:

```text
&[i64]
&[f64]
&[ubyte]
&string
scalar-only record by shared reference: &Record
scalar-only record by value: Record       // ABI では const pointer として受ける
```

戻り値:

```text
[i64]
[f64]
[ubyte]
string
scalar-only record
unit
existing scalar
```

`&mut` parameter は E05 では export 不可。

owned array/string parameter は E05 では export 不可。

`[i32]` など `i64`/`f64`/`ubyte` 以外の array は export 不可。

理由は C header と JS glue の最初の surface を小さく保ち、element ABI normalization を複雑にしないため。

`&[ubyte]` は binary buffer。

`&string` は UTF-8 buffer。

`&[i64]` と `&[f64]` は host memory を call 中だけ借用する。

host は call 中に buffer を変更してはならない。

host は call が戻った後、借用入力 buffer を自由に解放・変更してよい。

host から渡す buffer pointer は要素型に対して自然 alignment を満たし、`len` 要素分の読み取りに有効でなければならない。

Tsuzuri は借用入力を保持して戻り値へ直接参照させてはならない。所有権/借用検査がこれを保証する。

### C ABI shapes

Header は次の helper types を出す。

```c
typedef struct { const int64_t *ptr; int64_t len; } tsuzuri_i64_slice;
typedef struct { const double *ptr; int64_t len; } tsuzuri_f64_slice;
typedef struct { const uint8_t *ptr; int64_t len; } tsuzuri_ubyte_slice;
typedef struct { const uint8_t *ptr; int64_t len; } tsuzuri_string_slice;

typedef struct { int64_t *ptr; int64_t len; } tsuzuri_i64_buffer;
typedef struct { double *ptr; int64_t len; } tsuzuri_f64_buffer;
typedef struct { uint8_t *ptr; int64_t len; } tsuzuri_ubyte_buffer;
typedef struct { uint8_t *ptr; int64_t len; } tsuzuri_string_buffer;

void *tsuzuri_alloc(int64_t size);
void tsuzuri_free(void *ptr);
```

borrowed input は struct by value では渡さない。

C prototype では `(const T *ptr, int64_t len)` の pair に展開する。

例:

```text
export def sum :: &[i64] -> i64
```

Header:

```c
int64_t tz_sum(const int64_t *arg0_ptr, int64_t arg0_len);
```

戻り値 buffer は out pointer にする。

例:

```text
export def make_bytes :: i64 -> [ubyte]
```

Header:

```c
void tz_make_bytes(tsuzuri_ubyte_buffer *out, int64_t arg0);
```

`string` result:

```c
void tz_message(tsuzuri_string_buffer *out);
```

host は `out->ptr` を使い終わったら `tsuzuri_free(out->ptr)` を呼ぶ。

`len == 0` の buffer は `ptr == NULL` を返してよい。

host は `tsuzuri_free(NULL)` を呼んでよい。

### scalar-only records

scalar-only record は、すべての field が ABI scalar または scalar-only record で構成される record。

array/string/reference/function/task/list/tuple を含む record は export 不可。

ネスト record は Phase 1 では許すが、header は nested struct field として emit する。

i8/i16/bool field は ABI struct 内で i32 に正規化する。

これは internal LLVM record layout と C ABI record layout を一致させない設計である。

wrapper が ABI struct pointer から field を load し、bool は入力時 `!= 0`、出力時 0/1、i8/i16 は入力時 trunc、出力時 sign/zero extend して、内部 `%tz.record.Module.Name` を再帰的に組み立てる。

record parameter は by value ではなく const pointer。

```text
record Point { x: f64, y: f64 }
export def norm :: Point -> f64
```

Header:

```c
typedef struct { double x; double y; } tz_record_8Geometry_5Point;
double tz_norm(const tz_record_8Geometry_5Point *arg0);
```

record result は out pointer。

```text
export def origin :: Point
```

Header:

```c
void tz_origin(tz_record_8Geometry_5Point *out);
```

record parameter pointer must be non-null.

null record pointer traps.

record parameter and out pointers must be naturally aligned for the generated C struct and valid for one complete struct for the duration of the call.

### LLVM wrapper shapes

Borrowed input `&[i64]` wrapper parameter list:

```llvm
define i64 @tz_sum(ptr %arg0_ptr, i64 %arg0_len) nounwind {
entry:
  ; len >= 0
  %d0 = insertvalue %tz.array zeroinitializer, ptr %arg0_ptr, 0
  %descriptor = insertvalue %tz.array %d0, i64 %arg0_len, 1
  %result = call i64 @tz.fn.Main.sum(%tz.array %descriptor)
  ret i64 %result
}
```

negative length traps before descriptor construction.

`ptr == null && len == 0` is allowed.

`ptr == null && len > 0` traps.

C03 後、shared `&[T]` の internal LLVM type は `%tz.array` by value なので、borrowed buffer wrapper は descriptor SSA value を作って `%tz.array %descriptor` を直接 internal callee に渡す。

descriptor の alloca と pointer passing は `&string` と shared record references だけで使う。

`&string` wrapper validates UTF-8 before internal call.

host-provided invalid UTF-8 traps.

理由: Tsuzuri string invariant は UTF-8 であり、現行 ABI に error return channel がないため。

return array wrapper:

```llvm
define void @tz_make(ptr %out, i64 %n) nounwind {
entry:
  ; null out traps
  %result = call %tz.array @tz.fn.Main.make(i64 %n)
  %data = extractvalue %tz.array %result, 0
  %len = extractvalue %tz.array %result, 1
  ; store data/len into out struct
  ret void
}
```

wrapper must not drop/free returned buffer.

ownership transfers to host.

return string wrapper is analogous with `%tz.string`.

record wrapper builds internal record with `insertvalue` and writes result fields with `extractvalue`.

### exported allocator for WASM and native

E05 adds public external runtime functions with D-17 `tsuzuri_` prefix, not `tz_`.

```llvm
define ptr @tsuzuri_alloc(i64 %size) nounwind {
  %p = call ptr @tz.alloc(i64 %size)
  ret ptr %p
}

define void @tsuzuri_free(ptr %ptr) nounwind {
  call void @tz.free(ptr %ptr)
  ret void
}
```

E05 uses one allocator surface on both native and WASM: `tsuzuri_alloc(int64_t size)` and `tsuzuri_free(void *ptr)`.

Native hosts may allocate borrowed input buffers with ordinary host allocation as long as alignment/extent rules are met, but returned Tsuzuri-owned buffers must be released with `tsuzuri_free`.

Native object/exe exports both `tsuzuri_alloc` and `tsuzuri_free` whenever any E05 ABI is used, and the generated header declares both.

WASM exports `memory`, `tsuzuri_alloc`, and `tsuzuri_free` when any E05 buffer ABI is used.

WASM with only scalar exports may continue to omit allocator exports.

`tsuzuri_alloc(0)` returns a non-null 1-byte allocation or null? 既定案: internal `@tz.alloc` requires size passed; wrapper rounds `0` to `1` to avoid malloc(0) dependency.

Host must free only pointers returned by Tsuzuri allocator.

Host must not free borrowed input pointers through `tsuzuri_free`.

### WASM JS glue shape

Generated docs/examples should show manual glue.

Example borrowed input:

```js
const values = new BigInt64Array([1n, 2n, 3n]);
const bytes = values.byteLength;
const ptr = exports.tsuzuri_alloc(BigInt(bytes)); // i64 size parameter is BigInt
new BigInt64Array(exports.memory.buffer, Number(ptr), values.length).set(values);
const sum = exports.tz_sum(ptr, BigInt(values.length));
exports.tsuzuri_free(ptr);
```

Example returned bytes:

```js
const out = exports.tsuzuri_alloc(16n);
exports.tz_make_bytes(out, 4n);
const view = new DataView(exports.memory.buffer);
const dataPtr = view.getUint32(Number(out), true);
const len = view.getBigInt64(Number(out) + 8, true);
// Recreate views after calls that may grow memory.
const data = new Uint8Array(exports.memory.buffer, dataPtr, Number(len)).slice();
exports.tsuzuri_free(dataPtr);
exports.tsuzuri_free(out);
```

WASM pointer values are i32/Number on wasm32, while every i64 length/size parameter is BigInt.

Generated docs must show `Number(ptr)` for typed array offsets.

The wasm32 out-record layout for `{ ptr, i64 }` is fixed for E05 ABI as:

```text
offset 0: i32 ptr
offset 4: padding
offset 8: i64 len
size 16, alignment 8
```

Host JS must read pointer with `DataView.getUint32(out, true)` and length with `getBigInt64(out + 8, true)`.

Any call to `tsuzuri_alloc` or exported Tsuzuri code may grow memory, so JS must recreate `DataView` and typed array views after such calls before reading/writing.

### Evaluation order, traps, ownership

ABI wrappers evaluate arguments left to right in the same order as source parameters.

Wrapper validation happens before internal call, in parameter order.

If validation traps, internal function is not called.

Result out pointer null check happens before internal call for out-pointer ABI.

Borrowed input length validation happens before pointer null validation if written in parameter order? 既定案: for each buffer, validate len >= 0 then null/len relation.

Tsuzuri function body sees ordinary `&[T]` / `&string` borrowed values.

No hidden copy is made for borrowed input.

If the Tsuzuri function needs owned array/string, user code must clone/copy explicitly through std APIs.

Returned array/string ownership moves to host; Tsuzuri wrapper must not drop it.

Records contain only scalars, so no heap ownership crosses for records.

Trap behavior is existing `llvm.trap` / WASM RuntimeError / native abnormal termination.

No error-return ABI is introduced.

### 前提とする他チケットのインターフェース

C03 provides slice type `&[T]` and expression `&xs[a..b]`.

E05 assumes `Type::Reference(Box::new(Type::Array(T)), false)` can represent `&[T]`.

E02 std infrastructure is not required, but later helper docs may use std array functions.

### 他チケットへの提供インターフェース

F07 GPU backend can use E05 buffer ABI for host-owned contiguous data boundaries.

E06 host imports should reuse E05 `AbiType` classification for imported function signatures.

`llvm.rs` should expose helper classification:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
enum ExportAbi {
    Scalar(Type),
    BorrowedBuffer(BufferElement),
    BorrowedString,
    RecordPointer(usize),
    ReturnBuffer(BufferElement),
    ReturnString,
    ReturnRecord(usize),
    Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BufferElement {
    I64,
    F64,
    UByte,
}
```

`check.rs` should expose or share:

```rust
fn export_abi_for_parameter(ty: &Type, records: &[CheckedRecord]) -> Result<ExportAbi, Diagnostic>;
fn export_abi_for_result(ty: &Type, records: &[CheckedRecord]) -> Result<ExportAbi, Diagnostic>;
```

E06 imports use the same functions with direction-specific rules.

## 設計

### Type checking

Replace simple `Type::exportable` check for exported functions with ABI classifier.

Keep `Type::exportable` for scalar helper use.

`export_abi_for_parameter` accepts:

- scalar exportable type.
- `Type::Reference(Type::Array(element), false)` where element is i64/f64/ubyte.
- `Type::Reference(Type::String, false)`.
- scalar-only record by value.
- shared reference to scalar-only record.

`export_abi_for_result` accepts:

- scalar exportable type.
- `Type::Unit`.
- `Type::Array(i64/f64/ubyte)`.
- `Type::String`.
- scalar-only record.

Reject all others with existing `E1008`.

Message:

```text
exports support scalars, borrowed &[i64]/&[f64]/&[ubyte]/&string inputs, scalar-only records by pointer, and owned [i64]/[f64]/[ubyte]/string results
```

### Header generation

`header` must emit helper typedefs only when used.

Record typedef names are deterministic.

Use a collision-free length-prefixed escaped qualified-name encoding, not simple `_` replacement.

Example scheme:

```text
Geometry.Point.Point -> tz_record_8Geometry_5Point_5Point
A_B.C                -> tz_record_3A_B_1C
A.B_C                -> tz_record_1A_3B_C
```

Each segment is `<decimal byte length><escaped segment>`, joined with `_`; non `[A-Za-z0-9_]` bytes are escaped as `_xHH`.

Tsuzuri identifiers can be C keywords in field position after future features or generated names, so field names are sanitized against the full C keyword set including `restrict`, `_Atomic`, `_Bool`, `_Complex`, `_Generic`, `_Thread_local`, `alignas`, `alignof`, `nullptr`, and C++ keywords when header is inside `extern "C"`.

If two fields still collide after sanitization, emit `E1008` at the record declaration span and require renaming or wrapper records.

Emit record typedef before function prototypes.

Emit nested record typedef dependencies before outer record typedef.

Use `BTreeSet` to deduplicate helper typedefs and record typedefs.

For buffer result, first parameter is out pointer.

For record result, first parameter is out pointer.

For functions with both out result and normal parameters, out pointer comes first.

For unit result, no out pointer.

### LLVM ABI type helpers

Add C ABI LLVM type helper separate from internal `llvm_type`.

```rust
fn abi_parameter_types(ty: &Type, module: &CheckedModule) -> Vec<String>;
fn abi_result_type_or_out(ty: &Type, module: &CheckedModule) -> AbiResult;
```

For borrowed buffer parameter, ABI LLVM params are `ptr, i64`, but the internal call receives `%tz.array` by value after wrapper descriptor construction.

For record pointer parameter, ABI LLVM param is `ptr`.

For buffer/string/record result, ABI LLVM result is `void` and out pointer param is prepended.

For scalar result, current ABI result remains.

### UTF-8 validation

Add `@tz.string.validate_utf8(ptr, i64) -> i1`.

Implement in `src/runtime/string.ll` or new `src/runtime/utf8.ll`.

Validation must reject:

- overlong encodings.
- surrogate range.
- code points above U+10FFFF.
- truncated sequences.
- continuation byte without leader.

Validation must accept empty string with null ptr and len 0.

Wrapper traps if false.

Do not normalize or modify bytes.

### Runtime export conditions

`emit_target` should append public allocator wrappers when `ExportAbi` needs them.

Native object/exe with any E05 ABI needs `@tz.alloc`/`@tz.free` runtime and exported `@tsuzuri_alloc` / `@tsuzuri_free`.

WASM buffer input only still exports `tsuzuri_alloc/free` and memory for host convenience. Export allocator whenever any E05 buffer/string/record pointer ABI exists.

`--emit header` includes `void *tsuzuri_alloc(int64_t size);` and `void tsuzuri_free(void *ptr);` whenever any E05 ABI appears. It may include them unconditionally after E05 for simplicity, but the documented ABI is both functions together.

`wasm-ld` export list must add `--export=tsuzuri_alloc`, `--export=tsuzuri_free`, and `--export-memory` when required.

### JS memory alignment

`tsuzuri_alloc` returns at least 16-byte aligned memory because heap-wasm rounds allocations to 16.

JS glue may store i64/f64 arrays directly.

For `ubyte`, no alignment concern.

### Heap tracking tests

Native E2E can replace `@tz.alloc` / `@tz.free` or malloc/free tracking as existing primitives/storage tests do.

Each returned buffer must be freed by host and live bytes return to 0.

Borrowed input must not be freed by Tsuzuri.

## 実装手順

1. Add ABI classifier in `check.rs` and replace export validation.
   - 確認: scalar export tests still pass; `[i32]`, owned `[i64]` parameter, `&mut [i64]`, `&[i32]` reject with `E1008`.

2. Add header helper typedef emission.
   - 確認: header for borrowed `&[i64]` uses `const int64_t *arg0_ptr, int64_t arg0_len`; buffer result uses out pointer.

3. Add record scalar-only classifier and typedef emission.
   - 確認: record with string field rejects; nested scalar record emits deterministic length-prefixed typedefs; colliding sanitized field names reject.

4. Refactor `export_wrapper` to support expanded params and out result.
   - 確認: scalar wrappers produce same IR as before except harmless helper declarations.

5. Implement borrowed buffer descriptor construction.
   - 確認: wrapper constructs `%tz.array` SSA descriptor and calls internal callee with `%tz.array %descriptor`; negative len and null ptr with len > 0 trap in E2E child process/WASM RuntimeError.

6. Implement UTF-8 validation runtime and wrapper call.
   - 確認: invalid UTF-8 host string traps; valid multi-byte string length bytes preserved.

7. Implement returned array/string ownership transfer.
   - 確認: wrapper does not call `drop_value` on result; host frees with `tsuzuri_free`.

8. Add public `tsuzuri_alloc` / `tsuzuri_free` wrappers and WASM export logic.
   - 確認: native header declares both allocator functions; WASM module exports memory via `--export-memory` and allocator only for E05 ABI fixture; scalar fixture remains unchanged if chosen.

9. Update examples/native and examples/web with one buffer ABI example.
   - 確認: examples still run headless/browser tests.

10. Add tests and docs.
    - 確認: native/WASM × `-O0`/`-O3`, heap tracking live == 0.

実装時の検証は GUIDE §3 に従い、Rust は `cargo test --locked --test <file>` で対象 test file を指定する。

Node E2E の直前には必ず `cargo build --release --locked` を実行し、`node tests/*.mjs target/release/tsuzuri` は古い binary で走らせない。

## テスト計画

### Rust accepted

`export def sum :: &[i64] -> i64` accepted.

`export def mean :: &[f64] -> f64` accepted.

`export def checksum :: &[ubyte] -> i64` accepted.

`export def strlen :: &string -> i64` accepted.

`record Point { x: f64, y: f64 } export def norm :: Point -> f64` accepted with pointer ABI.

`export def origin :: Point` accepted with out pointer.

`export def bytes :: i64 -> [ubyte]` accepted.

`export def values :: i64 -> [i64]` accepted.

`export def message :: i64 -> string` accepted.

### Rust rejected

owned `[i64]` parameter is `E1008`.

`&mut [i64]` parameter is `E1008`.

`&[i32]` parameter is `E1008`.

`[i32]` result is `E1008`.

`&[string]` parameter is `E1008`.

record with `string` field in export parameter/result is `E1008`.

tuple export remains `E1008`.

list export remains `E1008`.

function/task export remains `E1008`.

### Header tests

Header contains `#include <stdint.h>`.

Header contains `const int64_t *arg0_ptr, int64_t arg0_len`.

Header contains `tsuzuri_ubyte_buffer`.

Header contains `void *tsuzuri_alloc(int64_t size);` and `void tsuzuri_free(void *ptr);`.

Record typedef names are deterministic, length-prefixed, and sorted.

Nested record fields recursively convert bool/narrow/scalar fields with the same rules as top-level records.

Fields named C keywords such as `restrict` are sanitized.

Distinct Tsuzuri names that would collide under `_` replacement, such as `A_B.C` and `A.B_C`, produce distinct typedef names.

Remaining sanitized field collisions are diagnosed.

No by-value record prototype appears.

### Native E2E

C host calls `tz_sum` with stack array and expects independent BigInt-computed sum.

C host calls `tz_make_values`, checks contents, then `tsuzuri_free`.

C host calls `tz_message`, validates bytes, then `tsuzuri_free`.

C host calls record pointer ABI and record out ABI.

Negative len and null pointer cases run in child process and trap.

Heap tracker shows live bytes 0 after each call.

`-O0` and `-O3`.

### WASM E2E

JS allocates input buffer with `tsuzuri_alloc`, fills memory, calls exported function.

JS passes BigInt for every i64 size/length argument and Number/i32 for pointers.

JS reads returned wasm32 out descriptor with pointer at offset 0 (`getUint32`) and length at offset 8 (`getBigInt64`) and copies data before free.

JS recreates `DataView` and typed array views after any call that may grow memory.

JS frees input, output payload, descriptor.

Invalid UTF-8 traps as `WebAssembly.RuntimeError`.

Memory stays within 16 MiB.

`-O0` and `-O3`.

### Deterministic IR

`--emit llvm` twice yields identical output.

Helper typedef/header order deterministic.

Intrinsic/runtime declarations not duplicated.

### LLVM assembly tests

For each ABI input kind, emit LLVM and assemble at least `--emit object` for native and wasm32 where applicable:

- borrowed `&[i64]`: wrapper calls internal function with `%tz.array` value, no descriptor alloca.
- borrowed `&[f64]`: same `%tz.array` value path.
- borrowed `&[ubyte]`: same `%tz.array` value path.
- `&string`: wrapper keeps pointer/reference path and UTF-8 validation.
- shared scalar record reference: wrapper keeps pointer/reference path.

Tests should grep IR for `%descriptor = insertvalue %tz.array` and `call ... @tz.fn...(%tz.array %descriptor)` for buffer inputs, and for absence of descriptor `alloca`.

## ドキュメント

`docs/language.md` の公開 ABI 表を拡張する。

borrowed buffer の aliasing rule と mutation 禁止を書く。

returned buffer の ownership transfer と `tsuzuri_free` を書く。

UTF-8 invalid input は trap と書く。

WASM allocator exports と JS glue を書く。

`docs/architecture.md` の Host invariant を更新し、by-value struct ABI に依存しない pointer ABI と明記する。

`README.md` の Native/Web host examples に buffer ABI の短例を追加する。

## 受け入れ条件

- [ ] scalar ABI の既存挙動が変わらない。
- [ ] borrowed `&[i64]` / `&[f64]` / `&[ubyte]` / `&string` input が C/WASM から使える。
- [ ] borrowed `&[T]` wrapper は C03 後の `%tz.array` by-value internal ABI に直接渡し、descriptor pointer alloca を作らない。
- [ ] `[i64]` / `[f64]` / `[ubyte]` / `string` result が out pointer + `tsuzuri_free` で使える。
- [ ] scalar-only record が pointer ABI で使える。
- [ ] header record names are collision-free and C keyword-safe; nested record conversion follows scalar ABI normalization.
- [ ] by-value C struct ABI classification に依存しない。
- [ ] invalid UTF-8, negative len, null invalid pointer が trap する。
- [ ] host borrowed pointers are documented as aligned and valid for `len` elements during the call.
- [ ] native/WASM × `-O0`/`-O3` が通る。
- [ ] heap tracking live == 0。
- [ ] native and WASM allocator exports are documented and present when needed.
- [ ] WASM uses `--export-memory`, pointer i32/Number, i64 BigInt, and defined out-record offsets.
- [ ] D-17 の `tz_` / `tsuzuri_` naming を守る。

## 落とし穴

C struct を by value で返すと platform ABI classification を LLVM IR 側で正確に再現する必要が出る。

i8/i16/bool fields を internal layout のまま C struct に出すと既存 scalar ABI normalization と矛盾する。

borrowed input を Tsuzuri が free してはいけない。

returned buffer を wrapper が drop すると host が dangling pointer を受け取る。

invalid UTF-8 を許すと Tsuzuri string invariant が壊れる。

WASM `memory` export を忘れると JS が returned buffer を読めない。

`tsuzuri_free` を `tz_free` にすると D-17 に反する。

C03 後の shared `&[T]` を pointer reference として渡すと internal callee ABI と不一致になる。

WASM out-record を 64-bit pointer として読むと offset 0 の i32 pointer/padding/i64 length layout を壊す。

JS typed array/DataView は `memory.grow` 後に detach/stale になるため、allocatorや exported call 後に作り直す。

`_` replacement for C names is not injective and C keywords such as `restrict` can break headers.

## 対象外

owned buffer parameter。

mutable host buffer parameter。

任意 element type の arrays。

lists, tuples, functions, tasks の ABI。

error-return ABI。

host callback/import は E06。

GPU pinned memory は F07。

## 未決事項

WASM allocator を scalar-only module でも常に export するかは未決。既定案は E05 ABI 使用時だけ export。

record nested fields を Phase 1 で許すかは未決。既定案は scalar-only record なら再帰的に許す。

UTF-8 validation runtime を `string.ll` に入れるか別 file にするかは未決。既定案は `string.ll`。

台帳の見直し提案はない。D-17 と D-18 に従う。
