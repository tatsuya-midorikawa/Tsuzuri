# M11: Swift のバインディングの生成（`--emit bindings-swift`）

| 項目 | 内容 |
| --- | --- |
| ID | M11 |
| 優先度 | P3（Swift のホストは、`--emit header` の C ヘッダーを bridging header で読めば、今でも Tsuzuri のライブラリを呼べる。足りないのは型付きの包みと、UTF-16 の受け渡し・`tsuzuri_free`・トラップの扱いの自動化である） |
| 規模 | L（生成器 1 つ、`--emit` の種類 1 つ、Rust の検査、swiftc の E2E、文書） |
| 依存 | ([NS02](_completed/NS02-export-and-bindings-nullable.md)) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 5（SwiftUI など Swift 専用の API には届かない）」に当たる。Swift の ABI を直接呼ぶことは D-67 の「改めないもの」なので、Swift だけの API へはホストの構成（Swift のアプリが Tsuzuri のライブラリを呼ぶ）で届く。このチケットは、その構成のバインディングの生成を足す。HEAD `804b7970` で確認） |
| 承認 | 要承認: D-55（`--emit bindings-swift`）。決定事項 D1 が要承認、D2〜D9 は既定案。D-67 の分岐 1 のどの案（X′・Y′・Z′）でも実装するチケットである（分岐 1 は X′ で承認済み（2026-11-26）。[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22)） |
| 改善する劣位 | Rust（UniFFI・swift-bridge）比: Rust のライブラリは Swift の包みを生成できる。Tsuzuri は C#・Python・C++ の包みを生成する（E13 の Phase 2）が、Swift の包みはない。Swift のホストは C ヘッダーを読み、UTF-16 の配列への複写、所有の結果の `tsuzuri_free`、`--trap-mode return` の状態の検査を手で書く |
| 手本にする既存実装 | `src/bindings_native.rs` の `cpp`・`cpp_export`（C ヘッダーの上の包み、`tsuzuri_try_<name>` と例外、所有の結果の解放）、`Shape`・`Scalar`・`shape`・`identifier`・`trap_kinds`・`signature_text`、`csharp` のクラス名の作り方（PascalCase）、`python`（所有の結果を複製してすぐ解放する形）。検査は `tests/bindings.rs` の `native_bindings_are_deterministic_and_follow_the_c_abi`（行の照合）と `tests/host_bindings.mjs`（ツールがなければ飛ばす E2E）。[E13](_completed/E13-host-bindings.md) の Phase 2 |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/profile.rs`, `src/bindings.rs`, `src/bindings_native.rs`, `src/bindings_swift.rs`（新規）, `tests/bindings.rs`, `tests/bindings.mjs`, `tests/host_bindings.mjs`, `tests/host_bindings_test.swift`（新規）, `docs/language.md`, `docs/architecture.md`, `README.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/compiler/usage.md` |

## 実装者向け早見

- 範囲: `--emit bindings-swift` を足す。同じソースの `--emit header` の C ヘッダーを包む `.swift` を 1 つ出す。C の宣言は複製しない。整数・浮動小数点・`bool`・string・utf8string・借用のバッファ・所有の結果・record・ハンドルを Swift の型に写し、`--trap-mode return` では `tsuzuri_try_<name>` を呼んで `throws` にする。NS02 が先に出荷されていれば `Maybe` を Optional に写す。
- 最初に読む: ① 仕様 1〜4 ② `src/bindings_native.rs` の `cpp`・`cpp_export`（1687〜1914 行）③ `src/driver.rs` の `Emit` と `is_bindings`・`bindings`・`emit_needs_another_target`・生成の呼び分け（5883 行。`_` の腕は JavaScript を生成する）④ 「現状」の手で書いた Swift の包み（この形を生成する）。
- 最初に書くテスト: `tests/bindings.rs` の `swift_bindings_are_deterministic_and_follow_the_c_abi`（`tests/fixtures/bindings_native` の生成結果を行で照合する）→ `tests/host_bindings.mjs` の Swift の段（swiftc がなければ飛ばす）。
- 触ってはいけない: `bindings-js`・`bindings-cs`・`bindings-py`・`bindings-cpp` の出力、`--emit header` の C ヘッダー、IR と object。`--emit` の種類の誤りの文（`emit kind must be ...`）は、`bindings-swift` を足す分だけ変わる（変えてはいけない意味の例外）。
- 完了の見分け方: native-interop.md の `quote` の例で、次がビルドでき、`42 tea!` を出す。

```sh
tsuzuri build quote --emit header -o quote.h
tsuzuri build quote --emit bindings-swift -o quote.swift
tsuzuri build quote --emit object -o quote.o
swiftc -import-objc-header quote.h main.swift quote.swift quote.o -o quote_swift
./quote_swift
```

```swift
// main.swift
print(Quote.total([1, 2, 39]), Quote.label("tea"))
```

## 目的

Swift だけの API（SwiftUI、Swift の concurrency を前提にしたフレームワーク）は、Tsuzuri から直接呼べない。Swift の ABI を Tsuzuri から呼ぶことは、D-67 の非目標である。そうした API を使うアプリでは、Swift のホストが UI を持ち、Tsuzuri をライブラリとして呼ぶ構成になる（native-interop.md:632「GUI は言語の一部ではありません」）。

この構成は今でも動く（「現状」）。ただし、Swift の側の糊を手で書く必要がある。string は UTF-16 の配列に写して渡し、所有の結果は `tsuzuri_free` で解放し、`bool` は `int32_t` から戻し、`--trap-mode return` の状態を検査する。C#・Python・C++ には、同じ表から作る型付きの包みがある（E13 の Phase 2）。Swift にだけない。

このチケットは、C++ の包みと同じ形で Swift の包みを生成する。C ヘッダーはそのまま使い、Swift の側には呼び出しの変換だけを置く。Swift のホストの構成を、ほかのホスト言語と同じ手間にする（D-67 の分岐 1 で Y′ が選ばれていれば主な構成になったが、分岐 1 は X′ に決まった（2026-11-26））。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M11` を読む。
- D-55 の登録（`--emit bindings-swift`）と決定事項 D1 の承認が記録されている。承認が記録されるまで、手順 2 以降に着手しない（手順 1 の基準の保存だけはしてよい）。
- P3 のチケットなので、着手前にその時点のコードと台帳でこのチケットをレビューし、指摘を反映してから実装する（GUIDE §0 の 2）。
- NS02 の状態を `_features/README.md` で見る。done なら仕様 3 の Optional の写しをこのチケットで足す。todo なら足さない（D8）。
- `swiftc --version` を記録する。macOS の E2E に swiftc が要る。起票時に確かめたのは Apple Swift 6.4.2（`swift-driver 1.168.6`、target `arm64-apple-macosx27.2.0`）だけである。生成するファイルは型に付ける `nonisolated`（SE-0449）を使うので、Swift 6.1 以降のコンパイラが要る（D3・D7）。
- 「現状」の再現を、着手時の `main` で実行し直す。食い違えば、このチケットを直してから始める。

### 停止条件

- **S1** `bindings-swift` を使わない出力（ほかの 4 つのバインディング、C ヘッダー、IR、object）が、手順 1 の基準と 1 byte でも違う。
- **S2** 生成した `.swift` が、bridging header（`-import-objc-header`）と module map の読み方のどちらかで、Swift 6.1 以降のコンパイラの言語モード 5・6 のどちらかで、または `-default-isolation MainActor` の下で、警告なしにコンパイルできない。
- **S3** C ヘッダーの宣言（record の struct、ハンドルの typedef、バッファの記述子）を Swift の側で複製しないと書けない形が見つかる。D1 の前提が崩れるので、即興で複製しない。
- **S4** fixture のどれかの形（固定長配列の field、32 bit に正規化した field、utf8string など）が、Swift が読み込んだ C の型では受け渡せない。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### コード

- `--emit` の種類は `src/main.rs:1203-1225` で読む。バインディングは `bindings-js`・`bindings-cs`・`bindings-py`・`bindings-cpp` の 4 つ（1215〜1218 行）で、それ以外は 1222 行の文の `E2000`（`OptionInvalidValue`）になる。同じ文が `src/main.rs:6487` のテストと `tests/bindings.mjs:466` にもある。`--help` の文は `src/main.rs:234-253`。
- `src/driver.rs` の `Emit`（251 行から）は `BindingsJs`・`BindingsCs`・`BindingsPy`・`BindingsCpp` を持つ。`is_bindings`（301 行）、`bindings`（309 行。`--emit` の綴りと拡張子）、`emit_needs_another_target`（496 行。cs・py・cpp は native の target だけ）、`--trap-mode return` を許す種類の一覧（685〜702 行）、出力の拡張子（`output_path`、1180〜1193 行）、`bindings_output_error`（1200 行。拡張子が違えば `bindings output must end with '.{extension}'; its file name, without the extension, names the shared library`）、生成の呼び分け（5883〜5885 行）がある。`src/main.rs:6444-6449`、`src/profile.rs:791-804`、`src/driver.rs:7792-7794`・`8390-8392` のテストも種類を列挙する。
- このうち網羅の検査が効くのは `output_path` の `match` だけである。`is_bindings` と `--trap-mode return` の一覧は `matches!`、`bindings` は `_ => None`、`emit_needs_another_target` は `_ => false`、生成の呼び分けは `_ => crate::bindings::javascript_for(module, flavor)` で終わる。生成の呼び分けに腕を足し忘れると、`--emit bindings-swift` が JavaScript のグルーを `.swift` に書く。`bindings` に足し忘れると、`--backend fast` の文（`src/driver.rs:854` の `output_kind`）が `bindingsswift` になる。
- 生成器は `src/bindings.rs`（JavaScript と共通の表。`banner` 73 行、`exports` 81 行、`records` 178 行、`handles` 211 行）と、その子のモジュール `src/bindings_native.rs`（`#[path]` で `mod native`）にある。`bindings_native.rs` は C#（`csharp` 483 行）、Python（`python` 1139 行）、C++（`cpp` 1687 行）を持ち、`Shape`（158 行。`Scalar`・`Unit`・`Handle`・`Slice`・`Owned`・`Record`）と `Scalar`（17 行）で C の ABI の形を分類する。NS02 の対象も、この 3 言語だけである（NS02 の早見）。
- C++ の包み（`cpp_export`）は、`bool` を `? 1 : 0` で渡して `!= 0` で戻し、8・16 bit の整数を `static_cast` で戻し、ハンドルの NULL を引数では `std::invalid_argument`、結果では `std::runtime_error` にする。所有の結果は `tsuzuri_free` を呼ぶ RAII の型で返す。`--trap-mode return` では `tsuzuri_trap_info` と結果の領域を渡して `tsuzuri_try_<name>` を呼び、状態 1 を `trap_error`、0 でも 1 でもない状態を `std::logic_error` にする。
- バインディングは LLVM を通さない（`Emit::is_bindings` の注記「The host bindings that come from the sources alone, without LLVM」）。内容は target で変わらない（option.md:98）。
- `--trap-mode return` は、tzroutine を起動するモジュールでは `E2000` になる（`src/driver.rs:3897` の `--trap-mode return cannot be combined with tzroutine: a trap in one tzroutine cannot be returned from the call that spawned it`）。`--emit bindings-cpp --trap-mode return` も同じ誤りになる（開発版で確認）。
- export の呼び出しは、その中で起動した tzroutine がすべて終わるまで戻らない（native-interop.md:614）。スケジューラーは最初に要るときに 1 回だけ初期化する（`src/runtime/tzroutine.c:1450` の `tzr_start`。POSIX では 418 行の `tzr_run_once` が `pthread_once` を呼ぶ）。
- `grep -rli swift src/` は `src/bindgen.rs`・`src/backend/parse.rs`・`src/backend/module.rs` だけで、Swift のバインディングの生成はない。

### 再現

`target/scratch/m11/lib/Main.tz`（host import の `notify` を tzroutine の中から呼ぶ）。コマンドは `target/scratch/m11` で実行する:

```tsuzuri
record Point { x: f64, y: f64 }

extern def notify :: i64 -> unit

export def add :: own i64 -> own i64 -> i64
fn add a b = a + b

export def label :: ref string -> string
fn label name = clone_string name + "!"

export def norm2 :: ref Point -> f64
fn norm2 p = p.x * p.x + p.y * p.y

export def ready :: own i64 -> bool
fn ready n = n > 0

export def per_unit :: own i64 -> own i64 -> i64
fn per_unit price count = price / count

export def fan :: own i64 -> i64
fn fan n =
    let parts = Routine.scope (ref n) 4 (\size chunk ->
        notify chunk
        deref size * chunk)
    parts[0] + parts[1] + parts[2] + parts[3]
```

```sh
$ tsuzuri build lib --emit bindings-swift -o lib.swift; echo $?
<command line>:1:1: error[E2000]: emit kind must be exe, object, llvm, header, wasm, wgsl, wgsl-relaxed, spirv, spirv-relaxed, shared, bitcode, bindings-js, bindings-cs, bindings-py, or bindings-cpp
2
$ tsuzuri build lib --emit header -o lib.h
$ tsuzuri build lib --emit object -o lib.o
$ tsuzuri build lib --emit shared -o liblib.dylib
lib/Main.tz:1:1: error[E2002]: 'clang' failed (exit status: 1):
Undefined symbols for architecture arm64:
  "_tsuzuri_host_Main_notify", referenced from:
      _tz.parallel.chunk.0 in module.o
...
shared libraries require the Clang linker; unresolved extern symbols need --link, -l or -L
```

host import を持つモジュールは `--emit shared` にできない。Swift のホストが host import を実装するときは、`--emit object` を静的にリンクする。`lib.h` の宣言（抜粋）:

```c
typedef struct { uint16_t *ptr; int64_t len; } tsuzuri_string_buffer;
typedef struct {
    double x;
    double y;
} tz_record_4Main_5Point;
void tsuzuri_free(void *ptr);
void tsuzuri_host_Main_notify(int64_t arg0);
int64_t tz_add(int64_t arg0, int64_t arg1);
void tz_label(tsuzuri_string_buffer *out, const uint16_t *arg0_ptr, int64_t arg0_len);
double tz_norm2(const tz_record_4Main_5Point *arg0);
int32_t tz_ready(int64_t arg0);
int64_t tz_fan(int64_t arg0);
```

手で書いた Swift のホスト `main.swift`（このチケットが生成する糊を含む）:

```swift
import Foundation

final class Counter: @unchecked Sendable {
    private let lock = NSLock()
    private var value = 0
    func increment() { lock.lock(); value += 1; lock.unlock() }
    var current: Int { lock.lock(); defer { lock.unlock() }; return value }
}
let calls = Counter()

@_cdecl("tsuzuri_host_Main_notify")
func notify(_ chunk: Int64) { calls.increment() }

func label(_ text: String) -> String {
    var out = tsuzuri_string_buffer()
    Array(text.utf16).withUnsafeBufferPointer { tz_label(&out, $0.baseAddress, Int64($0.count)) }
    defer { tsuzuri_free(out.ptr) }
    return String(decoding: UnsafeBufferPointer(start: out.ptr, count: Int(out.len)), as: UTF16.self)
}

var point = tz_record_4Main_5Point(x: 3, y: 4)
print("add:", tz_add(40, 2), "ready:", tz_ready(1) != 0)
print("label:", label("tea"))
print("norm2:", tz_norm2(&point))
print("fan main:", tz_fan(10))
let sums = UnsafeMutableBufferPointer<Int64>.allocate(capacity: 64)
DispatchQueue.concurrentPerform(iterations: 64) { i in sums[i] = tz_fan(Int64(i)) }
print("gcd sum:", sums.reduce(0, +), "notify calls:", calls.current)
```

```sh
$ swiftc -O -import-objc-header lib.h main.swift lib.o -o host && ./host
add: 42 ready: true
label: tea!
norm2: 25.0
fan main: 60
gcd sum: 12096 notify calls: 260
```

メインスレッドからも、GCD の 64 並列の呼び出し（それぞれが中で tzroutine を 4 つ起動する）からも動く。host import は Swift の `@_cdecl` で実装できる。Swift 6.4.2 では SE-0495 の `@c(tsuzuri_host_Main_notify)` も通った。

`--trap-mode return` の状態も Swift から読める。tzroutine を起動しない `target/scratch/m11/tlib/Main.tz`（`site` の番号はソースの並びで変わる。この並びで 2 になる）:

```tsuzuri
export def per_unit :: own i64 -> own i64 -> i64
fn per_unit price count = price / count

export def label :: ref string -> string
fn label name = clone_string name + "!"
```

```sh
$ tsuzuri build tlib --emit header --trap-mode return -o libt.h
$ tsuzuri build tlib --emit object --trap-mode return -o libt.o
$ swiftc -import-objc-header libt.h tmain.swift libt.o -o thost && ./thost
```

`tmain.swift`:

```swift
var trap = tsuzuri_trap_info()
var value: Int64 = 0
let status = tsuzuri_try_per_unit(&trap, &value, 10, 0)
print("status:", status, "kind:", trap.kind, "site:", trap.site)
let again = tsuzuri_try_per_unit(&trap, &value, 10, 4)
print("status:", again, "value:", value)
```

```text
status: 1 kind: 1 site: 2
status: 0 value: 2
```

### 起票時の試作で確かめたこと

`tests/fixtures/bindings_native` の C ヘッダーに対して、仕様 2 の写し方の包みを手で書き（`Native` という caseless enum の static 関数）、`tests/host_bindings_host.c` とリンクして動かした。結果:

- `widen(-1, 65535, 4294967295)` は `4295032829`、`narrow(300)` は `44`、`negate(false)` は `true`。
- `copy_text("héllo 🍵")` と `copy_utf8("ü🍵")` は同じ文字列を返す。`make_bytes(4)` は `[0, 1, 2, 3]`。
- record（`update`）は C の struct のまま往復し、固定長配列の field（`tz_record_4Main_6Window` の `values`）は Swift のタプル `(5, 10, -5)` として読める。
- ハンドルの typedef（`typedef struct tz_handle_4Main_7Counter_s *tz_handle_4Main_7Counter;`）は Swift では `OpaquePointer` になる。結果は `OpaquePointer?` で、`guard let` で受けられる。どのハンドルの型も同じ `OpaquePointer` になるので、型の区別には包みの struct が要る（D4）。
- 空の配列（`withUnsafeBufferPointer` の `baseAddress` が NULL でない）、`tz_sum_float(nil, 0)`（`0.0`）、空の所有の結果（`ptr` が NULL、`tsuzuri_free(nil)` は何もしない）のどれも動いた。
- `String(decoding: [0x61, 0xD800, 0x62] as [UInt16], as: UTF16.self)` は `[97, 65533, 98]` になる。孤立したサロゲートは U+FFFD に置き換わる（D2）。
- `-swift-version 5 -warnings-as-errors`、`-swift-version 6` で警告なしにコンパイルできた。`-default-isolation MainActor` では、印のない static 関数を GCD のスレッドから呼ぶと `call to main actor-isolated static method 'per_unit' in a synchronous nonisolated context` の警告になり、関数に `nonisolated` を付けると消えた。
- レビューで、関数にだけ `nonisolated` を付けた包みに、ハンドルの struct と `--trap-mode return` の非公開の `check` を足して `-swift-version 6 -default-isolation MainActor` で型検査した。包みそのものが誤りになった（`call to main actor-isolated static method 'check' in a synchronous nonisolated context`、`main actor-isolated property 'raw' can not be referenced from a nonisolated context`、`call to main actor-isolated initializer 'init(raw:)' in a synchronous nonisolated context`）。利用者の側でも、ハンドルの struct の `Equatable` の適合が `main actor-isolated conformance of 'Native.Counter' to 'Equatable' cannot be used in nonisolated context` になった。名前空間の enum に `nonisolated` を付ける（`nonisolated public enum Native`）と、中の型と関数がすべて nonisolated になり、言語モード 5・6 と `-default-isolation MainActor` のどれでも `-warnings-as-errors` で通った。`#if compiler(>=6.1)` で enum だけを分け、中身を `extension` に置く形は、`extension` の中が MainActor に戻るので通らなかった（D3）。
- bridging header の代わりに `module.modulemap`（`module NativeC { header "native.h" export * }`）を `-I` で渡し、包みの先頭の `#if canImport(NativeC)` / `import NativeC` / `#endif` で読む形も、同じ包みのままでビルドと実行ができた（D1）。bridging header の経路では `canImport` が偽になり、宣言はそのまま見える。

## 変えてはいけない意味

- 既存の 4 つのバインディングの出力と、`--emit header`・`object`・`shared`・`llvm` の出力（1 byte も変えない）。
- `--trap-mode return` と tzroutine の組み合わせの `E2000`、`--trap-info` などのライブラリのビルド用のオプションを、バインディングの出力が拒むこと。
- export の意味（tzroutine を待つこと、トラップでプロセスが終わること、`--trap-mode return` の状態 0・1・2）。生成する包みは、それを Swift の型に写すだけで、意味を変えない。
- 例外: `--emit` の種類の誤りの文に `bindings-swift` が入る（`src/main.rs:1222` と、そのテスト 2 か所）。

## 仕様

### 1. CLI と出力

- `tsuzuri build <path> --emit bindings-swift -o <name>.swift`。native の target（`--target native` か native の triple）だけで使える（ほかは `cs`・`py`・`cpp` と同じ `E2000`）。`-o` の拡張子は `.swift` で、違えば今の `OutputExtensionInvalid` の文（`bindings output must end with '.swift'; ...`）。`-O` は受け付けて無視する。`--trap-info`・`--debug-info`・`--debug-output`・`--allocator`・`--freestanding` は、ほかのバインディングと同じ `E2000`。`--trap-mode return` は受け付ける（ライブラリと C ヘッダーにも付ける）。
- 出力は `<name>.swift` 1 つ。C の宣言は同じソースの `tsuzuri build --emit header -o <name>.h` に任せ、複製しない（D1）。内容は target とホストで変わらず、2 回の生成で byte 単位で同じ。
- 先頭は `banner("//")` の行（`// Generated by Tsuzuri <版>. Bindings ABI 1. Do not edit.`）、使い方の注記、export が tzroutine を待つことの注記（D3）、そして次の 3 行。`<Name>` は仕様 4 の名前空間の名前である。

```swift
#if canImport(<Name>C)
import <Name>C
#endif
```

- 本体は `nonisolated public enum <Name> { ... }`（caseless enum。Swift の名前空間の慣用）で、export ごとに `public static func` を 1 つ持つ。enum に付けた `nonisolated` は、中のハンドルの struct・`TrapError`・`NestedCallError`・非公開の `check`・関数のすべてに及ぶ（D3）。型に付ける `nonisolated` は SE-0449 の構文で、Swift 6.1 以降のコンパイラが要る。Foundation は import しない（Swift の標準ライブラリだけを使う）。

### 2. 型の写し方

| Tsuzuri の型（export の位置） | C ヘッダー | Swift の引数・結果 | 変換 |
| --- | --- | --- | --- |
| `i64`・`i64u`・`f32`・`f64` | `int64_t`・`uint64_t`・`float`・`double` | `Int64`・`UInt64`・`Float`・`Double` | そのまま |
| `i8`・`i16`・`i32`・`i8u`・`i16u`・`i32u` | 32 bit に正規化（`int32_t`・`uint32_t`） | `Int8`・`Int16`・`Int32`・`UInt8`・`UInt16`・`UInt32` | 引数は `Int32(x)`・`UInt32(x)`、結果は `T(truncatingIfNeeded:)`（C++ の `static_cast` と同じ） |
| `bool` | `int32_t` | `Bool` | 引数は `x ? 1 : 0`、結果は `!= 0` |
| `ref [i64]`・`ref [f64]`・`ref [ubyte]` | `const T *` と長さ | `[Int64]`・`[Double]`・`[UInt8]` | `withUnsafeBufferPointer` の `baseAddress` と `count` |
| `ref string` | `const uint16_t *` と長さ | `String` | `Array(x.utf16)` に複写して `withUnsafeBufferPointer` |
| `ref utf8string` | `const uint8_t *` と長さ | `String` | `var t = x; t.withUTF8 { ... }`（ネイティブの文字列は複写しない） |
| 結果 `[i64]`・`[f64]`・`[ubyte]` | 先頭の `tsuzuri_*_buffer *out` | `[Int64]`・`[Double]`・`[UInt8]` | `Array(UnsafeBufferPointer(start:count:))` に複写し、`defer { tsuzuri_free(out.ptr) }` |
| 結果 `string` | `tsuzuri_string_buffer *out` | `String` | `String(decoding:as: UTF16.self)` と `tsuzuri_free` |
| 結果 `utf8string` | `tsuzuri_utf8string_buffer *out` | `String` | `String(decoding:as: UTF8.self)` と `tsuzuri_free` |
| record（`R`・`ref R`。スカラーの record と `@c_layout` record） | `const tz_record_... *` | C の struct の型（`tz_record_...`）を値で受ける | `withUnsafePointer(to:)` |
| 結果 record | `tz_record_... *out` | C の struct の型 | `var out = tz_record_...()` に書かせて返す |
| `extern type` のハンドル（`H`・`ref H`） | `tz_handle_...`（Swift では `OpaquePointer`） | 仕様 4 の包みの struct | 引数は `.raw`、結果は `guard let` で包む |
| `unit` の結果 | `void` | `Void`（書かない） | – |

- 引数の名前は `_ arg0`、`_ arg1`、…（C++ の `arg{index}` と同じ。ラベルなしで呼ぶ）。
- 結果のハンドルが NULL なら `fatalError("result of '<name>' is a null handle")`。NS01 のとおり、`Maybe` でないハンドルの結果は Tsuzuri の側で非 NULL が保証されるので、これは防御の検査である（C++ の `std::runtime_error` に当たる）。引数のハンドルは非 Optional の `OpaquePointer` を持つ struct なので、NULL を渡せない。
- 不正な UTF-8 の検査は要らない。Swift の `String` は常に正しい Unicode である（C++ が `valid_utf8` で検査するのは、`std::string_view` が任意の byte を持てるからである）。
- 孤立したサロゲートを含む Tsuzuri の `string` の結果は、`String(decoding:as:)` が U+FFFD に置き換える（D2）。生成する doc コメントと文書に書く。

### 3. `--trap-mode return` と `Maybe`（NS02 の後）

- `--trap-mode return` の包みは `throws` になり、`tsuzuri_try_<name>(&trap, &value または &out, 引数…)` を呼ぶ。状態 1 は `<Name>.TrapError`（`site: UInt32`・`kind: UInt32`・`kindName: String`、`description` は `trap: <kindName> (site <site>)`）、0 でも 1 でもない状態は `<Name>.NestedCallError` を投げる（D6）。種類の名前の表は `trap_kinds()`（C++・Python と同じ）。
- `--trap-mode return` は tzroutine を起動するモジュールでは今どおり `E2000` である。`throws` の包みを作れるのは、tzroutine を起動しないモジュールだけである。
- NS02 が出荷されていれば、NS02 の仕様 1 の 4 形を次のように写す。引数 `Maybe<H>`・`Maybe<ref H>` は包みの struct の Optional（`nil` は NULL）、引数 `Maybe<ref R>` は `tz_record_...?`（値があるときだけ `withUnsafePointer`、なければ `nil`）、結果 `Maybe<H>` は包みの struct の Optional（NULL は `nil`）。ほかの `Maybe` は NS02 のとおり `E1008` で、包みの生成に届かない。

### 4. 名前

- 名前空間 `<Name>`: ファイル名の拡張子を除いた部分を `identifier`（`bindings_native.rs:233`）にかけ、`_` で区切って各部分の先頭を大文字にしてつなぐ（C# のクラス名と同じ）。空か数字で始まれば先頭に `Library` を足す。結果が Swift の予約語か、生成するコードが使う標準ライブラリの型の名前（`String`・`Array`・`Bool`・`Int8`〜`Int64`・`UInt8`〜`UInt64`・`Float`・`Double`・`Error`・`Never`・`Any`・`Self`・`Type`・`Protocol`・`OpaquePointer`・`UnsafeBufferPointer`・`UnsafeMutableBufferPointer`・`Void`）と同じなら、末尾に `Library` を足す。例: `quote.swift` は `Quote`、`my_lib.swift` は `MyLib`、`string.swift` は `StringLibrary`。
- C の module の名前は `<Name>C`（例 `QuoteC`）。module map の経路で使う（D1）。
- 関数の名前は Tsuzuri の名前のまま（`per_unit` を `perUnit` にしない）。Swift の予約語と同じなら backtick で囲む（`` `guard` ``。`export def guard` は Tsuzuri では正しい名前で、`tz_guard` になる）。fixture の `export def check` は、`--trap-mode return` の非公開の `check(_ status: Int32, _ trap: tsuzuri_trap_info) throws` と同じ名前になるが、引数の型が違う多重定義として共存し、`try check(tsuzuri_try_check(&trap, arg0), trap)` は非公開のほうを選ぶ（レビューで `-warnings-as-errors` の型検査を通した）。export の引数に `tsuzuri_trap_info` は来ないので、選び方が曖昧になることはない。
- record は C ヘッダーの struct の名前（`tz_record_4Main_5Point`）をそのまま使う。C++ と C# と同じ。
- ハンドルは、Tsuzuri の型の名前の最後の部分（`Counter`）の `public struct`（`public let raw: OpaquePointer`、`public init(raw: OpaquePointer)`、`Hashable`）を名前空間の中に置く。同じ名前の別のハンドル、名前空間の中のほかの名前（`TrapError`・`NestedCallError`・関数）と重なれば、`bindings_native.rs` の `unique` と同じく `_` を足す。並びはハンドルの C の名前の順（`handles` の `BTreeSet`）で決める。

### 5. 例（生成する形）

`quote` の例（`total`・`label`・`per_unit`）の `--emit bindings-swift` は、次の形になる（版の行と注記の文は実装で決める）。

```swift
// Generated by Tsuzuri <版>. Bindings ABI 1. Do not edit.
// Swift bindings over "quote.h": generate it from the same sources with tsuzuri build --emit header,
// and link the object of tsuzuri build --emit object (or the library of --emit shared).
// Read quote.h with -import-objc-header quote.h (an Xcode bridging header), or with a module map
// of the module QuoteC.
// A call returns after every tzroutine that it started has finished: call a long export from a
// background queue, not from the main thread.
#if canImport(QuoteC)
import QuoteC
#endif

nonisolated public enum Quote {
    /// tz_total: ref [i64] -> i64
    public static func total(_ arg0: [Int64]) -> Int64 {
        arg0.withUnsafeBufferPointer { tz_total($0.baseAddress, Int64($0.count)) }
    }

    /// tz_label: ref string -> string
    public static func label(_ arg0: String) -> String {
        var out = tsuzuri_string_buffer()
        Array(arg0.utf16).withUnsafeBufferPointer { tz_label(&out, $0.baseAddress, Int64($0.count)) }
        defer { tsuzuri_free(out.ptr) }
        return String(decoding: UnsafeBufferPointer(start: out.ptr, count: Int(out.len)), as: UTF16.self)
    }

    /// tz_per_unit: i64 -> i64 -> i64
    public static func per_unit(_ arg0: Int64, _ arg1: Int64) -> Int64 {
        tz_per_unit(arg0, arg1)
    }
}
```

レビューで、この形（注記の行を除く）を手で書き、`quote` の `--emit header`・`--emit object` と、早見の `main.swift` でビルドした。bridging header の経路で `-swift-version 5`・`-swift-version 6`・`-swift-version 6 -default-isolation MainActor`（どれも `-warnings-as-errors`）、module map の経路（`module QuoteC { header "../quote.h" export * }` を `-I` で渡す）で `-swift-version 6 -warnings-as-errors` のビルドが通り、どれも `42 tea!` を出した。

## 設計

### CLI と driver

- `src/main.rs`: `Some("bindings-swift") => Emit::BindingsSwift`、誤りの文と `--help` の一覧に `bindings-swift` を足す（`bindings-cs, bindings-py, bindings-cpp, or bindings-swift`）。`--help` の説明は「bindings-cs, bindings-py, bindings-cpp, and bindings-swift write C#, Python ctypes, C++20, and Swift bindings of the shared library NAME」。
- `src/driver.rs`: `Emit::BindingsSwift`（doc コメント「`--emit bindings-swift` (M11): Swift wrappers over the C header of the same sources」）。`is_bindings`、`bindings`（`("bindings-swift", "swift")`）、`emit_needs_another_target`（native だけ）、`--trap-mode return` を許す種類、出力の拡張子、生成の呼び分け（`crate::bindings::swift(module, stem, options.trap_return)`）に足す。足し忘れがコンパイルの誤りになるのは `output_path` だけである。ほかの 5 か所（`is_bindings`、`bindings`、`emit_needs_another_target`、`--trap-mode return` の一覧、生成の呼び分け）は `matches!` か `_` の腕で終わるので、どれもテストで確かめる（「現状」のコード）。

### 生成器（`src/bindings_swift.rs`、新規）

- `src/bindings_native.rs` の子のモジュールにする（`#[path = "bindings_swift.rs"] mod swift; pub use swift::swift;`）。Rust では子のモジュールが親の非公開の項目を使えるので、`Shape`・`Scalar`・`shape`・`identifier`・`unique`・`trap_kinds`・`signature_text` を公開に変えずに共有する。`src/bindings.rs` の `pub use native::{cpp, csharp, python}` に `swift` を足す。
- `pub fn swift(module: &CheckedModule, name: &str, trap_return: bool) -> String`。構成は `cpp` と同じ順: 名前の決定（仕様 4）、先頭の注記と `canImport`、`nonisolated public enum <Name> {`、`--trap-mode return` なら `TrapError`・`NestedCallError` と非公開の `check(_ status: Int32, _ trap: tsuzuri_trap_info) throws`、ハンドルの struct（`handles` の順）、export ごとの `swift_export`。
- `Scalar` に `swift`（`Int8`〜`UInt64`・`Float`・`Double`・`Bool`）と `swift_abi`（C ヘッダーの型を Swift が読み込んだ型。`Int32`・`UInt32`・`Int64`・`UInt64`・`Float`・`Double`）を足す。
- `swift_export` は `cpp_export` と同じく、引数ごとに「Swift の引数の型」「呼ぶ前の準備（配列の `withUnsafeBufferPointer` の入れ子、`withUTF8`、`withUnsafePointer(to:)`）」「C の引数の式」を作る。借用の引数が複数あるときは、クロージャーを引数の順に入れ子にする（例 `a.withUnsafeBufferPointer { a in b.withUnsafeBufferPointer { b in tz_f(a.baseAddress, Int64(a.count), b.baseAddress, Int64(b.count)) } }`）。入れ子の結果の値はクロージャーの戻り値で外へ出す。
- 決定的にする: 並びは `exports(module)`（名前の順）と `handles`（`BTreeSet`）だけで決め、`HashMap` を使わない。行末は LF。

### 採らない形

- C の宣言を `@_silgen_name` で Swift に書き直す形（D1 の退けた案）。
- 所有の結果を `deinit` で `tsuzuri_free` する class で返す形（D5 の退けた案）。

## 前提とする他チケットのインターフェース

- なし（単独で出荷できる）。
- [NS02](_completed/NS02-export-and-bindings-nullable.md) が先に出荷されていれば、その `Shape::NullableHandle`・`Shape::NullableRecord`（NS02 の設計）と C ヘッダーの形（NS02 の仕様 1）を使って、仕様 3 の Optional を写す。

## 他チケットへ提供するインターフェース

- `Emit::BindingsSwift` と `crate::bindings::swift`。
- NS02 がこのチケットより後に出荷されるときは、NS02 の PR が Swift の写し（仕様 3 の Optional）を足す。NS02 の仕様 2 の表に Swift の列を足し、`swift_export` の `match` に 2 つの形を足す。
- D-67 の分岐 1 は X′ に決まった（2026-11-26）ので、AppKit は Tsuzuri から M01・M02 の宣言で呼べる。この包みは、Swift だけの API（SwiftUI など）を Swift のホストに置き、Tsuzuri を呼ぶ構成で使う。選ばれなかった Y′（ObjC を言語に入れない）なら、AppKit も Swift のホストに置き、この包みで呼ぶ構成が主になるはずだった。

## 実装手順

### 手順 1: 基準

- 変更: なし（`target/scratch/m11/` の下書きだけ）。
- 内容: `node scripts/repo-kb.mjs brief M11`。`tests/fixtures/bindings_native` の `bindings-js`（`--target wasm32`）・`bindings-cs`・`bindings-py`・`bindings-cpp`（それぞれ `--trap-mode return` の有無）と `--emit header` の出力を `target/scratch/m11/base/` に保存する。「現状」の再現を実行し直す。
- 確認: 保存した件数と、`swiftc --version` を記録する。

### 手順 2: CLI と driver

- 変更: `src/main.rs`、`src/driver.rs`、`src/profile.rs`。
- 内容: 設計の「CLI と driver」。生成は空の文字列を返す仮の `swift` でよい。
- 確認: T0。`src/main.rs` と `src/driver.rs` のテストの一覧に `bindings-swift` を足し、`bindings_output_error` の `.swift` の検査、`--target wasm32` の拒否、`--trap-info` の拒否、`--trap-mode return` の受理を確かめる。`tests/bindings.mjs:466` の文を直す。

### 手順 3: 生成器

- 変更: `src/bindings_native.rs`、`src/bindings_swift.rs`、`src/bindings.rs`。
- 内容: 設計の「生成器」。仕様 2・4 の写しと、仕様 3 の `--trap-mode return`。NS02 が done なら仕様 3 の Optional も。
- 確認: `tests/bindings.rs` の `swift_bindings_are_deterministic_and_follow_the_c_abi`・`swift_names_avoid_keywords_and_collisions`。手順 1 の基準と、ほかの 4 つのバインディングと C ヘッダーが `cmp` で一致する。T1。

### 手順 4: E2E

- 変更: `tests/host_bindings.mjs`、`tests/host_bindings_test.swift`（新規）。
- 内容: 「テスト計画」の E2E。
- 確認: macOS で `node tests/host_bindings.mjs target/debug/tsuzuri` が Swift の段を通る。swiftc を隠した環境（`TSUZURI_SWIFTC=/nonexistent`）で `host bindings: Swift skipped (no swiftc)` を出して成功する。

### 手順 5: 文書

- 変更: 「ドキュメント」の節のページ。
- 内容: 例を `node scripts/check-docs.mjs` で検査する。Swift のコードの例は `run=` を付けない（Swift はページの検査の対象外）。手順 4 の E2E で同じ形を動かす。
- 確認: T2。`node scripts/check-cli-docs.mjs` が `bindings-swift` の抜けを報告しない。

## テスト計画

### Rust

- `tests/bindings.rs` の `swift_bindings_are_deterministic_and_follow_the_c_abi`: `native_fixture()` の `bindings::swift(&module, "native", trap)` が 2 回の生成で同じ（`trap` は偽と真）。`native_bindings_are_deterministic_and_follow_the_c_abi` と同じく、出力が `// {banner}` で始まり、次の行を含む（実装で文を決めたら、この一覧をその文で固定する）:
  - `#if canImport(NativeC)`・`import NativeC`・`nonisolated public enum Native {`
  - `public static func add(_ arg0: Int64, _ arg1: Int64) -> Int64 {`
  - `public static func narrow(_ arg0: Int64) -> Int8 {` と `Int8(truncatingIfNeeded: tz_narrow(arg0))`
  - `public static func negate(_ arg0: Bool) -> Bool {` と `tz_negate(arg0 ? 1 : 0) != 0`
  - `public static func copy_values(_ arg0: [Int64]) -> [Int64] {`
  - `public static func copy_text(_ arg0: String) -> String {`
  - `public static func update(_ arg0: tz_record_4Main_6Sample) -> tz_record_4Main_6Sample {`
  - `public struct Counter: Hashable {` と `fatalError("result of 'pass_through' is a null handle")`
  - `--trap-mode return` の出力: `public static func add(_ arg0: Int64, _ arg1: Int64) throws -> Int64 {` と `try check(tsuzuri_try_add(&trap, &value, arg0, arg1), trap)`。偽のときの出力は `TrapError` を含まない。
- `swift_names_avoid_keywords_and_collisions`: `export def guard :: own i64 -> i64` の関数が `` `guard` `` になる（C の名前は `tz_guard` のまま）。ファイル名 `string`・`1lib`・`library` から `StringLibrary`・`Library1lib`・`Library` になる。`extern type` の `Main.Counter` と `Other.Counter` が `Counter` と `Counter_` になる。
- `src/driver.rs` のテスト: `BuildOptions { emit: Emit::BindingsSwift, .. }` の `validate` が成功し、`--target wasm32` で `'--emit bindings-swift' requires '--target native'`、`--backend fast` で今の文。`src/profile.rs` の一覧に `Emit::BindingsSwift`。
- `src/main.rs` のテスト: `--emit bindings-swift` を読み、`--trap-mode return` と組み合わせても `trap_info` が偽。

### E2E（`tests/host_bindings.mjs`）

- どのホストでも: `bindings("swift", join(directories.swift, "native.swift"))` と `--trap-mode return` の版を 2 回ずつ生成して byte 単位で比べる（今の `bindings` 関数）。`--emit bindings-swift -o x.h` は `bindings output must end with '.swift'`、`--trap-info` は今の文で拒む。
- macOS で swiftc（`TSUZURI_SWIFTC` か `swiftc`）があるとき:
  - `tests/host_bindings_test.swift`（新規。`tests/host_bindings_test.cpp` と同じ呼び出しを Swift で書く）を、`swiftc -swift-version 6 -warnings-as-errors -import-objc-header native.h native.swift host_bindings_test.swift libnative.dylib -Xlinker -rpath -Xlinker <dir>` で `-Onone` と `-O` のそれぞれでビルドして動かす。Swift の 2 つ以上のファイルをまとめてビルドすると、トップレベルの文は `main.swift` という名前のファイルにしか書けない（ほかの名前では `expressions are not allowed at the top level`。レビューで確かめた）。テストは `@main` の型の `static func main()` に書く。ライブラリは今どおり `--allocator counting` と `--link host.o` で作り、C ヘッダーも今の C++ の段と同じく `--allocator counting` で作る（`tsuzuri_alloc_stats` の宣言が要る）。最後に `tsuzuri_alloc_stats` で `live_bytes == 0` と確保と解放の回数の一致を確かめる。
  - 同じ包みを `-swift-version 5 -warnings-as-errors` でもビルドする。swiftc が `-default-isolation MainActor` を受け付けるなら、それを付けて GCD のスレッドから呼ぶ版もビルドする（D3。ハンドルの struct と `--trap-mode return` の版を含める）。
  - module map の経路: `module NativeC { header "native.h" export * }` の `module.modulemap` を置いたディレクトリを `-I` で渡し、bridging header なしで同じテストをビルドして動かす。
  - トラップ: `--trap-mode return` を付けないライブラリでは、`trap` の引数で動かすとプロセスが 0 以外で終わる。`-D TRAP_MODE` の版は `native_trap.swift` と `libnative_trap.dylib` で、`try` が `Native.TrapError` を受け、次の呼び出しが成功する。
  - 出力の行 `host bindings: Swift passed (<swiftc の版>)`。swiftc がなければ `host bindings: Swift skipped (no swiftc)`。
- Linux の swiftc での実行は、初版では行わない（起票時に確かめていない）。生成の決定性の検査は Linux でも走る。

### 変えないことの確認

- 手順 1 の基準と、ほかの 4 つのバインディング（`--trap-mode return` の有無）と C ヘッダーが byte 単位で一致する（比べた件数を記録する）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md` の「共有ライブラリと各言語のバインディング」: `bindings-swift` を足し、バインディングの表に Swift の列を足す（呼び出し: `nonisolated` の caseless enum `<Name>` の `public static func`（Swift 6.1 以降）、整数・`bool`: `Int8`〜`UInt64`・`Bool`、借用入力: `[Int64]`・`[Double]`・`[UInt8]`・`String`、所有結果: 複写して `tsuzuri_free`、レコード: C ヘッダーの struct、ハンドル: `raw: OpaquePointer` の struct、トラップ: `--trap-mode return` なら `throws` の `TrapError`）。Swift の例（bridging header と module map の 2 つの読み方、`--emit object` の静的リンク、host import を `@_cdecl` か `@c` で実装する方法、孤立したサロゲートの置き換え）。
- 同じページの「ホストのイベントループと export」と「GUI は言語の一部ではありません」の段落: SwiftUI・AppKit は Swift のホストに置き、Tsuzuri はライブラリとしてこの包みで呼ぶ構成を書く。export は tzroutine を待つので、重い呼び出しはメインスレッドから呼ばず、`DispatchQueue` か Swift の `Task.detached` から呼ぶことを書く（D3）。
- `_tsuzuri/language-reference/compiler/option.md` の `--emit` の表と 98 行の段落、`_tsuzuri/language-reference/compiler/usage.md:499`。
- `docs/language.md:4969` の段落、`docs/architecture.md:225` の段落（Swift の包みも LLVM を通さず、C ヘッダーと同じ名前を使うこと、検証の方法）。
- `README.md` の 567 行（特徴の一覧）と 985 行（CLI の表）、1282 行（`host_bindings.mjs` の説明に Swift と swiftc を足す）。
- 変えたページに `node scripts/check-docs.mjs` を通す。

## 受け入れ条件

- [ ] `--emit bindings-swift` が仕様 1〜4 のとおりに `.swift` を出し、2 回の生成で byte 単位で同じ。
- [ ] macOS の swiftc で、bridging header と module map の両方の読み方で、`-Onone`・`-O`、言語モード 5・6 で警告なしにビルドでき、fixture の全 export の呼び出しが通り、`live_bytes == 0`。swiftc が受け付けるなら、`-default-isolation MainActor` の下でも警告なしにビルドでき、GCD のスレッドから呼べる。
- [ ] `--trap-mode return` の包みが `throws` で、トラップの後の次の呼び出しが成功する。付けないライブラリではトラップでプロセスが終わる。
- [ ] NS02 が done なら、仕様 3 の Optional の写しが動く。todo なら、他チケットへ提供するインターフェースのとおり NS02 に引き継ぐことを NS02 のチケットに書いた。
- [ ] ほかの 4 つのバインディングと C ヘッダー・IR・object が byte 単位で変わらない（比べた件数を記録した）。
- [ ] 文書を直し、T2 が成功した。

## 落とし穴

- `Emit` の 6 か所のうち、網羅の検査が効くのは `output_path` だけである。`Emit::is_bindings`・`--trap-mode return` を許す種類の一覧（`matches!`）、`bindings`・`emit_needs_another_target`・生成の呼び分け（`_` の腕）は、足し忘れてもコンパイルが通る。`is_bindings` を忘れると LLVM を通る経路に入り、生成の呼び分けを忘れると JavaScript のグルーを `.swift` に書く。テストで 5 つとも確かめる（生成の結果が `#if canImport(` を含み、`export` や `import` の JavaScript の行を含まないこと）。
- `String.utf16` の `withContiguousStorageIfAvailable` は、ネイティブの Swift の文字列（UTF-8 で持つ）では `nil` を返す。UTF-16 の連続した領域を得るには配列への複写が要る。複写を省こうとして `nil` の場合を落とさない。
- 借用のクロージャーの外へポインターを持ち出さない（`withUnsafeBufferPointer` の外で `baseAddress` を使うと未定義の動作になる）。複数の借用は入れ子にする。
- 所有の結果の `defer { tsuzuri_free(out.ptr) }` は、複写の後に走る位置に置く。`--trap-mode return` で状態が 0 でないとき、`out` には何も書かれていない（`ptr` は NULL のまま）。`check` が投げる前に `defer` を登録しても `tsuzuri_free(nil)` で害はないが、複写をしてはいけない。
- `nonisolated` は関数にだけ付けても足りない。`-default-isolation MainActor` の下では、印のないハンドルの struct（`raw`・`init(raw:)`・`Hashable` の適合）と非公開の `check` が MainActor に隔離され、nonisolated の関数から使えずに包みそのものが誤りになる（「現状」の試作）。名前空間の enum に付け、中の宣言には付けない。`extension` に中身を分けると、`extension` の中は既定の隔離に戻るので、生成する宣言は enum の本体の中に置く。型に付ける `nonisolated` は Swift 6.1 より前のコンパイラでは誤りになる（SE-0449）ので、生成するファイルは Swift 6.1 以降を要る（D7）。
- ハンドルの struct は Swift では複写できる。`own H` の引数に渡したハンドル（所有が Tsuzuri へ移る）を、その後も使わないのは利用者の責任である。C++ の typedef・C# の `record struct` と同じで、このチケットでは `~Copyable` にしない（`copy` でない `extern type` だけを複写できない型にすると、ほかの言語の包みと形が変わり、`extern type copy` と書き分ける規則も要る。決めるなら別のチケットにする）。
- record の C の struct は、Swift では全 field の引数を持つ初期化子と引数のない初期化子（0 埋め）を持つ。32 bit に正規化した field（`tiny: i8` は `int32_t tiny`）は Swift でも `Int32` に見える。包みは field を変換しない（C++ と同じ。C# だけが型付きのプロパティを持つ）。文書に書く。
- `-import-objc-header` は、Swift のコンパイラに渡す 1 つのヘッダーである。2 つの Tsuzuri のライブラリの `.h` を同じ翻訳単位で `#include` すると、今の C ヘッダーは `typedef redefinition with different types ('struct tsuzuri_string_slice' vs 'struct tsuzuri_string_slice')` で失敗する（起票時に `clang -fsyntax-only` で確かめた。`tsuzuri_trap_info` だけが `#ifndef TSUZURI_TRAP_INFO_DEFINED` で守られ、バッファの記述子の typedef は守られていない）。C・C++ のホストにも同じ制限がある。このチケットでは C ヘッダーを変えない（S1）。1 つの Swift のターゲットで 2 つの Tsuzuri のライブラリを使う形は、着手時に module map の経路で試し、結果を文書の既知の制限に書く。直すなら別のチケットで、`--emit header` の共通の typedef を守る。

## 対象外

- Swift の ABI を Tsuzuri から直接呼ぶこと（D-67 の非目標）。
- Swift の `async`・`await` への写し（export は同期の C の関数のままである）。
- Swift Package のひな型（`Package.swift`・C のターゲット・`module.modulemap`）の生成と配布。文書に手順を書くだけにする。
- host import を Swift で実装するためのひな型（`@_cdecl`・`@c` の関数の宣言）の生成。文書に書き方を書くだけにする。
- ホストから実行時に関数ポインターを渡すこと（fnptr_from_host）。
- WebAssembly の Swift（SwiftWasm）からの呼び出し。

## 決定事項

| ID | 決定 | 理由（採らない案を含む） | 状態 |
| --- | --- | --- | --- |
| D1 | 出力は `.swift` 1 つで、C の宣言は `--emit header` の `.h` に任せる。Swift から読む方法は、bridging header（`-import-objc-header`、Xcode の Objective-C Bridging Header）と module map（`<Name>C` の module）の両方を、同じ `.swift` のまま使えるようにする。そのために先頭に `#if canImport(<Name>C)` / `import <Name>C` / `#endif` を置く。module map は生成せず、3 行のひな型を文書に書く | C++ の包みが `#include "<name>.h"` で C ヘッダーを使うのと同じく、C の配置の正は 1 つにする。bridging header はアプリのターゲットの標準の方法で、module map は framework や Swift Package の C のターゲットの方法なので、両方が要る。`canImport` の形で両方が通ることを起票時に確かめた。採らない案: (a) bridging header だけを前提にする（Swift Package の C のターゲットで使えない）。(b) `module.modulemap` も生成する（ファイル名が固定なので、2 つのライブラリを同じディレクトリに置くと衝突し、Xcode と Swift Package は module map を置く場所がそれぞれ違う）。(c) C の宣言を `@_silgen_name` で Swift に書き直す（下線付きの属性で安定した Swift の機能ではなく、C の struct の配置を Swift の側で保証できない） | 要承認（承認前は手順 2 以降に着手しない） |
| D2 | string の引数は `Array(text.utf16)` に複写して `withUnsafeBufferPointer` で渡し、結果は `String(decoding:as: UTF16.self)` で読み、`defer { tsuzuri_free(out.ptr) }` で解放する。utf8string の引数は `withUTF8`、結果は `String(decoding:as: UTF8.self)` | Swift の `String` は UTF-8 で持つので、UTF-16 の連続した領域を得るには複写が要る（C++ の `std::u16string_view` のような複写のない入力はない）。孤立したサロゲートは U+FFFD に置き換わるが、Swift の `String` はそれを表せないので、`String` を返す限り避けられない。文書に書く。採らない案: 結果を `[UInt16]` で返す（正確だが、利用者が毎回 `String` に直すことになり、Swift の慣用に合わない） | 既定案（実装者はこの案に従う） |
| D3 | export が tzroutine を待つことを、生成するファイルの先頭の注記と文書に書く。名前空間の enum に `nonisolated` を付け（`nonisolated public enum <Name>`）、中の型・関数をすべて nonisolated にする | 重い export をメインスレッドから呼ぶと UI が止まる（native-interop.md:614）。Xcode の新しいアプリのターゲットは既定の隔離を MainActor にでき、そのとき印のない宣言は MainActor に隔離され、GCD のスレッドから呼ぶと警告になる（起票時に `-default-isolation MainActor` で確かめた）。採らない案: (a) 関数にだけ `nonisolated` を付ける（起票時の案。レビューで、ハンドルの struct と非公開の `check` が MainActor に残り、包みそのものが `-default-isolation MainActor` で誤りになると確かめた）。(b) `#if compiler(>=6.1)` で enum の宣言だけを分け、中身を `extension` に置く（`extension` の中は既定の隔離に戻るので通らなかった）。(c) 中の宣言ごとに `nonisolated` を付ける（struct の `Hashable` の適合は、struct の型に `nonisolated` を付けないと MainActor に残る。型に付けるなら本案と同じく Swift 6.1 以降が要り、行が増えるだけである）。代わりに Swift 6.0 以前のコンパイラは使えない（D7） | 既定案（実装者はこの案に従う） |
| D4 | 名前空間は caseless enum `<Name>`（C# と同じ PascalCase）。関数は Tsuzuri の名前のまま（予約語は backtick）、引数は `_ argN`。record は C の struct の名前をそのまま使う。ハンドルは Tsuzuri の型の名前の最後の部分の struct（`raw: OpaquePointer`）で包む | Swift は C の不完全な struct へのポインターをすべて `OpaquePointer` に読み込むので、包みがないと別のハンドルの型を取り違えても型の誤りにならない。record は C の struct が型を区別するので包まない。名前を camelCase に変えない理由は、`per_unit` と `perUnit` の衝突の扱いが要らず、C++・C#・Python と同じ名前で文書と検索が通るから | 既定案（実装者はこの案に従う） |
| D5 | 所有の結果は Swift の配列と `String` に複写し、すぐ `tsuzuri_free` で解放する（Python と同じ） | Swift の配列と `String` は値の型で、利用者は寿命を気にしなくてよい。複写の費用は結果の大きさに比例する（export が結果を作る費用と同じ次数）。性能の数字は測っていないので主張しない。採らない案: `deinit` で解放する class で返す（C++ の `buffer<T>` に当たる。複写はないが、参照カウントの class で、Swift の配列の操作には結局複写が要る） | 既定案（実装者はこの案に従う） |
| D6 | `--trap-mode return` の包みは型のない `throws` にし、状態 1 は `TrapError`、それ以外の 0 でない状態は `NestedCallError` を投げる | C++・C#・Python と同じ 2 つの区別。採らない案: 型付きの `throws(E)`（2 つの区別を 1 つの型にまとめる enum が要り、C++・C#・Python の 2 つの例外の形と揃わない。言語モードの制約ではない。Swift 6.4.2 の `-swift-version 5` でも型付きの `throws` が通ることをレビューで確かめた）。`TrapError` は `Error` と `CustomStringConvertible` の struct で、`site` は `.trap.json` の ID | 既定案（実装者はこの案に従う） |
| D7 | 生成するコードは Swift の標準ライブラリだけを使い（Foundation を import しない）、Swift 6.1 以降のコンパイラの言語モード 5・6 と `-default-isolation MainActor` の下で、警告なしにコンパイルできる形にする | Foundation がない環境（Linux の最小の Swift、組み込み）でも同じファイルが使える。6.1 は型に付ける `nonisolated`（SE-0449、D3）の要求である。`-default-isolation`（SE-0466）は Swift 6.2 からなので、それを使うコンパイラでは必ず満たされる。確かめたのは Swift 6.4.2 だけなので、6.1〜6.3 は文書で保証しない | 既定案（実装者はこの案に従う） |
| D8 | `Maybe` の Optional への写し（仕様 3）は、NS02 とこのチケットのうち後に出荷するほうが足す | NS02 は C ヘッダーの形を決めるチケットで、その前に Swift の写しを決めると二重の決定になる。依存を弱い依存にしたのは、Optional なしでも包みの大半（スカラー・バッファ・record・ハンドル・トラップ）が使えるから | 既定案（実装者はこの案に従う） |
| D9 | Swift の E2E は macOS で swiftc があるときだけ走らせ、なければ飛ばす。生成の決定性と行の照合は、どのホストでも走る | C++・C#・Python の E2E と同じく、ツールのない環境でも検査の残りが通る。Linux の swiftc の `-import-objc-header` は起票時に確かめていない | 既定案（実装者はこの案に従う） |
