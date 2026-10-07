# E13: ホスト言語バインディングと Web glue の生成

| 項目 | 内容 |
| --- | --- |
| ID | E13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E05, E12, (F06), (F11) |
| 後続 | B08 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D11（Phase 2 の言語・共有ライブラリ出力・ブラウザー threads glue。承認前は Phase 2 に着手しない） |
| 改善する劣位 | C#/F# 比: .NET からの利用手段（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: WASM ホストの手書き glue と、ブラウザー向け threads glue がない |
| 手本にする既存実装 | ABI の型モデルと C header: `src/llvm.rs` の `header`、`src/llvm_abi.rs` の `header_types`・`c_parameters`・`record_name`・`record_layout`、`src/abi.rs` の `Buffer`・`parameter`・`result`・`out_result`。`--emit` の追加: `Emit::Header` の各 match（`src/main.rs` の `--emit` 解析、`src/driver.rs` の `BuildOptions::validate`・`BuildOptions::output_path`・`build_complete`）。副出力の公開: `trap_sidecar_path` と `publish_outputs`。同梱 JS の書き方: `src/runtime/wasm-threads.mjs` の `createThreadPool`。手書き glue の手順: `examples/web/simulation.mjs`。テスト: `tests/host_abi.rs` の `host_buffers_and_records_roundtrip_on_native_and_wasm`、`tests/host_imports.mjs` |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/lib.rs`, `src/llvm.rs`, `src/llvm_abi.rs`, `src/bindings.rs`（新規）, `src/runtime/bindings.mjs`（新規）, `tests/bindings.rs`（新規）, `tests/bindings.mjs`（新規）, `tests/bindings_consumer.mts`（新規）, `tests/fixtures/bindings/Main.tz`・`Geometry.tz`（新規）, `examples/web/README.md`, `docs/language.md`, `docs/architecture.md`, `README.md`, `_docs/guides/webassembly.md`, `_docs/guides/native-interop.md`, `_docs/feature-status.md`, `_features/README.md`。Phase 2 だけ: `src/runtime/wasm-threads.mjs`, `_docs/guides/wasm-threads.md` |

## 目的

Rust の wasm-bindgen、Emscripten の glue、C# の P/Invoke 生成に相当する型付きバインディングを生成し、ホスト側の手書き変換コード
（out descriptor の読み取り、`tsuzuri_alloc`／`tsuzuri_free`、BigInt／Number の変換、memory grow 後の view の作り直し）をなくす。

- Phase 1（実装対象）: WASM（`--target wasm32`）向けの JavaScript ESM glue `<name>.mjs` と TypeScript 宣言 `<name>.d.mts` を
  `tsuzuri build --target wasm32 --emit bindings-js`（新規）で生成する。native の C ホストは既存の `--emit header` を使い、変えない。
- Phase 2（設計方針のみ）: ブラウザー向け WASM threads glue、C#（`LibraryImport`）、Python（`ctypes`）、C++ の RAII ラッパー、
  native の共有ライブラリ出力。F06 で対象外としたブラウザーの本番 threads glue はここで扱う。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求め、D11 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- E05 が done であること（HEAD で done）。E12 が done であること。確認: `grep -n "^| E05\|^| E12\|^| E13\|^| E14" _features/README.md` の状態欄。
- E12 の段 C（`extern type` のハンドル）・段 D（静的コールバック）が承認されずに未実装のまま E12 が done になった場合、型対応表の
  該当行（ハンドル・コールバック）は実装しない。`src/bindings.rs` にもその arm を作らない。
- E14 は着手条件にしない。glue は E14 の D2–D4（捨てる・作り直す・分類）の規則を自前で実装し、`src/runtime/trap-boundary.mjs` を import しない（D7）。
- F06・F11 は着手条件にしない。threads の module は Phase 1 の glue が拒否し（D8）、pointer は常に `>>> 0` で扱う（F11 の 2 GiB 超に備える）。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（2 つの header）を保存していること。
- `.d.mts` の型検査に使う TypeScript は `vsc/` の devDependency（`vsc/package-lock.json` で 6.0.3）。`npm ci --prefix vsc` の後、
  `vsc/node_modules/.bin/tsc --version` が `Version 6.0.3` を出すこと。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `--emit header` の出力が 1 byte でも変わる。`tests/host_abi.rs` の header のテストの期待値を変えたくなった。
- `record_layout` から求めた field offset が WASM の wrapper の読み書きと合わず、record の往復テストが失敗する（offset を個別に補正しない）。
- `crate::abi::parameter`・`crate::abi::result`・E12 が、型対応表にない ABI 型（ハンドルを持つ record、buffer を受け取るコールバックなど）を許していた。
- WASM の export／import、`wasm-ld` の引数、生成 IR を変えないと glue を書けない。glue が使えるのは既存の `memory`・`tsuzuri_alloc`・
  `tsuzuri_free`・`tz_*`・`tsuzuri_trap_site` と、E12 が export する `__indirect_function_table` だけ。
- glue に `node:` の import、`fetch`、npm package、Rust crate の追加が必要になった。
- Node でスタック枯渇が `RangeError` 以外になった（E14 の停止条件と同じ）。
- 既存テストの期待値を変える必要がある。`--emit` の一覧を含む message と help 文字列（`src/main.rs`）の変更だけは除く。

## 現状（HEAD `f8dc655` で確認）

- `src/driver.rs` の `Emit` は `Executable`・`Object`・`Llvm`・`Header`・`Wasm`・`Wgsl`。`src/main.rs` は `--emit` の値 `exe`・`object`・
  `llvm`・`header`・`wasm`・`wgsl` を受け、それ以外は `emit kind must be exe, object, llvm, header, wasm, or wgsl`（E2000、終了コード 2）。
- `BuildOptions::validate` は header 出力で `--debug-info`・`--trap-info`・`--debug-output`・`--wasm-feature simd128`・`--cpu native` を
  E2000 で拒否する。`BuildOptions::output_path` は拡張子を `Emit` から決める（`Header => "h"`）。
- `build_complete` は `Emit::Header` のとき `llvm::header(module)` を本文にし、LLVM を通さない。cache は
  `options.cache && options.emit != Emit::Header` のときだけ使う。副出力は `trap_sidecar_path`（`<output>.trap.json`）を
  `protect_sources` で検査し、`publish_outputs` の `sidecars` で本体と一緒に公開する。
- `llvm::header` は注釈行、`#pragma once`、`host_abi::header_types`、`tsuzuri_main`（`io_entry` か `main_entry` のとき）、`imports::header`（extern の
  prototype）、`tz_<name>` の prototype を `module.functions` の順に出す。版は記録しない。header は target に依存しない
  （`--target wasm32 --emit header` と native の出力は同一。検証済み）。
- `src/abi.rs`: `Buffer`（`I64`・`F64`・`UByte`・`String`・`Utf8String`、`of`・`name`・`c_element`・`width`）、`scalar_record`、
  `parameter`、`result`、`out_result`、`field_name`（C/C++ の予約語へ `tz_` を付ける）。
- `src/llvm_abi.rs` は `src/llvm.rs` から `#[path = "llvm_abi.rs"] mod host_abi;` として読まれる。`record_name`（`pub(super)`。
  `tz_record_4Main_5Inner`、型引数は `_T<長さ>_<16 進>`）、`record_types`（非公開。ABI に現れる record を依存順に集める）、
  `record_layout`（非公開。(size, align)。64-bit スカラーは 8/8、それ以外のスカラーは正規化後の 4/4、空の record は size 1）、
  `uses_host_abi`（`pub(crate)`）。
- WASM の link（`src/driver.rs`）は `llvm::uses_host_abi(module) || io_runtime` のときだけ `memory`・`tsuzuri_alloc`・`tsuzuri_free` を export し、
  `--trap-info` で `tsuzuri_trap_site`、各 `export def` で `tz_<name>` を export する。scalar だけの module は memory を export しない。
- import の namespace: extern は module `tsuzuri`・name `<Module>.<name>`（`src/llvm_imports.rs`、docs/language.md「ホスト関数のインポート」）、
  IO は `tsuzuri_io`（`src/llvm_io.rs`）、`--debug-output` は `tsuzuri_debug` の `write`（`src/llvm.rs`）、threads は `tsuzuri_threads`。
- JavaScript 側は手書き: `examples/web/simulation.mjs` が確保・複製・descriptor の読み取り・解放を書き、docs/language.md「公開 ABI」が
  同じ手順を例示する。同梱 JS は `src/runtime/wasm-threads.mjs`（`createThreadPool`）、`src/runtime/webgpu.mjs`（`createWebGpu`）で、
  Rust からは参照しない。ブラウザーの本番 threads glue は F06 の対象外（`_features/README.md`）。
- TypeScript 6.0.3（`vsc/`）で `--module nodenext` のとき、`import "./a.mjs"` は `a.d.ts` を使わず TS7016 になり、`a.d.mts` なら成功する（検証済み）。
- C#・Python・C++ 向けの生成物と、native の共有ライブラリ出力（`.so`／`.dylib`／`.dll`）はない。

### 再現（検証済み）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-e13 && cp -R tests/fixtures/host_abi /tmp/tz-e13/host_abi
target/release/tsuzuri build /tmp/tz-e13/host_abi/Main.tz --target wasm32 -o /tmp/tz-e13/h.wasm
node -e 'const m=new WebAssembly.Module(require("fs").readFileSync("/tmp/tz-e13/h.wasm"));console.log(WebAssembly.Module.exports(m).map(e=>e.name).join(" "),WebAssembly.Module.imports(m).length)'
target/release/tsuzuri build /tmp/tz-e13/host_abi/Main.tz --target wasm32 --emit bindings-js -o /tmp/tz-e13/h.mjs
```

```text
memory tz_sum tz_sum_float tz_checksum tz_make_bytes tz_copy_values tz_copy_text tz_copy_utf8 tz_text_length tz_byte_length tz_update tz_owned_record tz_empty_record tsuzuri_alloc tsuzuri_free 0
<command line>:1:1: error[E2000]: emit kind must be exe, object, llvm, header, wasm, or wgsl
```

## 仕様

### 前提とする他チケットのインターフェース

- E05（done）: docs/language.md「公開 ABI」の型表、out descriptor（offset 0 に i32 pointer、offset 8 に i64 length、size 16、align 8）、
  `tsuzuri_alloc(i64) -> ptr`（0 は 1 byte、失敗はトラップ）と `tsuzuri_free(ptr)`（null は何もしない）。Phase 1 は形を変えない。
- E06（done）: extern の import は module `tsuzuri`、name は `HostImport::wasm_name`（`<Module>.<name>`）。引数はスカラー・借用 buffer・
  スカラー record、結果はスカラー・unit・所有 buffer（ホストが `tsuzuri_alloc` で確保し、先頭の out pointer へ descriptor を書く）・record。
- E12（着手条件）: `extern type H` は LLVM `ptr`（wasm32 では i32）、C 名は `handle_c_name`（E12 で新規）、使える位置は引数と結果だけ（E12 D5）。
  静的コールバックは extern の関数型引数に関数 table の index（i32）を渡し、`module.callbacks` が空でないときだけ `--export-table`
  （export 名 `__indirect_function_table`）。コールバックの引数・結果は ABI スカラーとハンドルだけと仮定する（違えば停止）。
- E14（任意）: D2–D4 の規則（例外が出た instance を捨て、次の呼び出しで作り直し、D3 の表で分類する）と、`TsuzuriTrap` の中身
  `{ reason: "trap" | "stack", site, kind?, path?, line?, column? }`。
- G04（done）: `--trap-info` の WASM は `tsuzuri_trap_site` を export し、`<output>.trap.json` は `crate::trap::side_table` の形
  `{"version":1,"sites":[{"id","kind","path","span":{"start","end","line","column","end_line","end_column"}}]}`。

### CLI

```text
tsuzuri build <path> --target wasm32 --emit bindings-js [-o <dir>/<name>.mjs] [-O0|-O1|-O2|-O3]
出力: <dir>/<name>.mjs（本体）と <dir>/<name>.d.mts（副出力）
既定の出力: 入力 file の拡張子を .mjs にした path（BuildOptions::output_path）
```

- glue は LLVM を通さず `CheckedModule` だけから作る。`-O` は受理して無視する。`.wasm` は同じソースから別に build する
  （`tsuzuri build <path> --target wasm32 [-O3] [--trap-info] -o <name>.wasm`）。`.d.mts` の path は出力 file 名の末尾 `.mjs` を `.d.mts` に替えたもの。

### 生成物の API

新 API（実装後に有効。未検証）。`tests/fixtures/bindings`（新規）の `.d.mts` の抜粋:

```typescript
// Generated by Tsuzuri 0.1.0. Bindings ABI 1. Do not edit.
export interface tz_record_8Geometry_5Point { x: number; y: number }
export interface Exports {
  add(arg0: bigint, arg1: bigint): bigint;
  area(arg0: tz_record_8Geometry_5Point): number;
  copy_values(arg0: BigInt64Array | Borrowed<BigInt64Array>): BigInt64Array;
  greet(arg0: string): string;
}
export interface Imports { "Main.now"(): bigint }
export interface TrapInfo { reason: "trap" | "stack"; site: number; kind?: string; path?: string; line?: number; column?: number }
export interface TrapSite { id: number; kind: string; path: string; span: { line: number; column: number } }
export declare class TsuzuriTrap extends Error { readonly trap: TrapInfo }
export interface Borrowed<T> { readonly length: number; view(): T }
export interface Bindings {
  readonly exports: Exports;
  withBorrowed<R>(kind: "i64", length: number, callback: (buffer: Borrowed<BigInt64Array>) => R): R;
  withBorrowed<R>(kind: "f64", length: number, callback: (buffer: Borrowed<Float64Array>) => R): R;
  withBorrowed<R>(kind: "ubyte", length: number, callback: (buffer: Borrowed<Uint8Array>) => R): R;
}
export declare function load(source: ArrayBuffer | ArrayBufferView | WebAssembly.Module, options: { imports: Imports; sites?: readonly TrapSite[] }): Promise<Bindings>;
```

- extern がなければ `Imports` は空の interface で、`load` の第 2 引数は `options?: { sites?: readonly TrapSite[] }`。
- 引数名は C header と同じ `arg0`、`arg1`、…。export の property 名は Tsuzuri の関数名（`tz_` なし）、record の interface 名は `record_name`、
  field 名は Tsuzuri の field 名（C 向けの `field_name` の `tz_` 付けはしない）。

### 型規則

型の対応（D3）。記述子は `.mjs` に埋め込む表の値（新規）。「JS → WASM」は export の引数と import の結果、「WASM → JS」は export の結果と import の引数に使う。

| Tsuzuri | 記述子 | TypeScript | JS → WASM（検査と変換） | WASM → JS |
| --- | --- | --- | --- | --- |
| `i8`／`i16`／`i32` | `"i8"` など | `number` | `Number.isInteger(v)` かつ符号付き範囲内。そのまま | そのまま |
| `i8u`／`i16u`／`i32u` | `"i8u"` など | `number` | 整数かつ `0 <= v < 2^n`。そのまま | `v >>> 0` |
| `i64` | `"i64"` | `bigint` | `typeof v === "bigint"` かつ `BigInt.asIntN(64, v) === v` | そのまま |
| `i64u` | `"i64u"` | `bigint` | `BigInt.asUintN(64, v) === v` | `BigInt.asUintN(64, v)` |
| `f32`／`f64` | `"f32"`／`"f64"` | `number` | `typeof v === "number"`（f32 は engine が最近接へ丸める） | そのまま |
| `bool` | `"bool"` | `boolean` | `typeof v === "boolean"`、`v ? 1 : 0` | `v !== 0` |
| `unit`（結果だけ） | `"unit"` | `void` | import の戻り値は捨てる | `undefined` |
| `ref [i64]`／`ref [f64]`／`ref [ubyte]` | `"slice:i64"` など | `BigInt64Array`／`Float64Array`／`Uint8Array` か `Borrowed<…>` | `Object.prototype.toString` で型を検査。長さ 0 は pointer 0、それ以外は `tsuzuri_alloc` へ複製。`Borrowed` は複製しない | `slice()` した複製 |
| `ref string` | `"slice:string"` | `string` | `charCodeAt` で UTF-16 code unit を `Uint16Array` へ（孤立 surrogate も保つ） | `String.fromCharCode` を 4096 単位で |
| `ref utf8string` | `"slice:utf8string"` | `string` | `v.isWellFormed()` でなければ `TypeError`。`TextEncoder` | `new TextDecoder("utf-8", { fatal: true })` |
| `[i64]`／`[f64]`／`[ubyte]`（結果） | `"buffer:i64"` など | 上の typed array | import: 型を検査して `tsuzuri_alloc` へ複製し、out へ descriptor を書く | descriptor を読んで `slice()`、`tsuzuri_free(ptr)` |
| `string`／`utf8string`（結果） | `"buffer:string"` など | `string` | import: 上の 2 行と同じ符号化で確保 | 上の 2 行と同じ復号、`tsuzuri_free(ptr)` |
| スカラー record とその `ref` | `"record:<record_name>"` | interface `<record_name>` | 各 field を検査し offset へ書く（pointer 渡し） | offset から読んだ新しい object |
| ハンドル（E12） | `"handle:<handle_c_name>"` | `number & { readonly __tsuzuri: "<handle_c_name>" }` | 整数かつ `0 <= v < 2^32` | `v >>> 0` |
| コールバック（E12、import の引数だけ） | `["callback", [引数], 結果]` | 関数型 | — | table の index から、上の変換を通す JS 関数（import の呼び出し中だけ有効） |

- Result／Maybe・union・タプル・関数値は E05 の ABI にない（`E1008`）ので対象外。ABI の許容範囲は `crate::abi::parameter`／`crate::abi::result` から変えない。
- pointer（out、確保、descriptor の ptr）は常に `>>> 0` してから `DataView`／typed array の offset に使う。

### 評価順序・所有権・借用

- 呼び出しの順序: (1) 引数の個数と全引数の型・範囲・record の field を検査する（失敗は `TypeError`／`RangeError` で、instance に触れず捨てない）。
  (2) instance がなければ作る。(3) 左から順に入力を確保・複製し、out 結果なら out 領域（descriptor は 16 bytes、record は `record_layout` の size）を確保する。
  (4) `tz_<name>` を呼ぶ。(5) 結果を読み、所有結果を複製して `tsuzuri_free(ptr)` する。(6) out、入力の順に逆順で解放する。
- (2) 以降で例外が出たら instance を捨て（D6）、解放しない（linear memory ごと捨てる。E14 D4）。
- JS は wasm の pointer を保持しない。所有結果は必ず JS の object へ複製してすぐ解放する。成功した呼び出しの後、確保の残り（`live`）は 0 になる。
- view は使う直前に `memory.buffer` から作る。確保・export・import のどの呼び出しの後でも古い view を使わない（memory grow）。
- `withBorrowed(kind, length, callback)`: `length` 要素の領域を確保し（0 なら確保せず pointer 0）、`Borrowed` を callback へ渡し、callback が返るか
  投げた後に解放する。`view()` は呼ぶたびに現在の memory 上の新しい typed array を返す。instance が捨てられた後や解放後の `Borrowed` は、
  `view()` や引数に使うと `TypeError` になる。callback が thenable を返したら解放後に `TypeError` を投げる（非同期の間は有効にできない）。
- import の wrapper: 借用入力は複製して渡す（ホストが保持しても安全）。ホストの例外は記録してそのまま投げ、export 側の分類で同じ object を再送出する。

### 数値・トラップ・native と WASM の差

export 呼び出しで (2) 以降に出た例外の分類（E14 D3 と同じ順序）。どの行でも instance を捨て、次の呼び出しで同じ module と imports から
`new WebAssembly.Instance` で同期に作り直す。

| 例外 | 条件 | glue の動作 |
| --- | --- | --- |
| import のホスト関数が投げた値 | 記録した値と同一（`===`） | そのまま再送出 |
| `WebAssembly.RuntimeError` | 上以外 | `TsuzuriTrap`。`trap = { reason: "trap", site }`、`site` は古い instance の `tsuzuri_trap_site?.() ?? 0`（1 回だけ呼ぶ）。`sites` に同じ `id` があれば `kind`・`path`・`span.line`・`span.column` を足す |
| `RangeError`、または `name === "InternalError"` | 上以外 | `TsuzuriTrap`。`trap = { reason: "stack", site: 0 }` |
| その他 | 上以外 | そのまま再送出 |

- `TsuzuriTrap` は `Error` の派生で、`name` は `"TsuzuriTrap"`、`cause` は元の例外。`message` は `trap at <path>:<line>:<column> (<kind>)`、
  site が表にないとき `trap (site <site>)`、スタック枯渇は `stack exhausted`。
- 最初の instance は `load` の中で `await WebAssembly.instantiate(module, imports)` で作る。作り直しは同期（Node と Worker が対象。ブラウザーの
  main thread で大きな module の同期 instantiate が拒まれる件は Phase 2）。
- native は既存の `--emit header` のまま変えない。glue の数値の意味は WASM の ABI と同じで、f32 の NaN の payload は保証しない。

### 診断

コンパイラの診断（終了コードは既存の規則: CLI の構成は 2、それ以外は 1）。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `--emit` の値が不明 | `emit kind must be exe, object, llvm, header, wasm, wgsl, or bindings-js` | コマンド行 |
| E2000 | `--emit bindings-js` で `--target wasm32` がない | `'--emit bindings-js' requires '--target wasm32'` | コマンド行 |
| E2000 | `--trap-info`・`--debug-info`・`--debug-output` を併用 | `--trap-info is not valid for bindings output; pass it when building the .wasm`（各 option 名で同形） | コマンド行 |
| E2000 | `--wasm-feature` を併用 | `--wasm-feature is not valid for bindings output; pass it when building the .wasm` | コマンド行 |
| E2000 | `-o` が `.mjs` で終わらない | `bindings output must end with '.mjs'; declarations are written next to it as '<name>.d.mts'` | コマンド行 |
| E2004 | `export def` が一つもない | `bindings need at least one 'export def' entry point` | コマンド行 |
| E2003 | 出力か `.d.mts` がソースと同じ・symlink | 既存の `protect_sources` の message | コマンド行 |

`--cpu native` は既存の `'--cpu native' requires native executable or object output` で拒否される。glue が投げる JavaScript の例外:

| 型 | 条件 | メッセージ |
| --- | --- | --- |
| `Error` | module が `tsuzuri_threads` を import する | `bindings do not support modules built with --wasm-feature threads; use createThreadPool` |
| `Error` | `tsuzuri` 以外の namespace を import する | `bindings do not support imports from '<namespace>'; build a library without IO main or --debug-output` |
| `Error` | 必要な export がない | `module does not match bindings: missing export '<name>'; regenerate the bindings from the same sources` |
| `TypeError` | import の関数がない | `missing import '<Module>.<name>'` |
| `TypeError` | 引数の個数・型の誤り | `'<name>' expects <n> arguments` ／ `argument <i> of '<name>' must be <description>` |
| `RangeError` | 整数の範囲外 | `argument <i> of '<name>' is out of range for <type>` |

### 資源上限

新しい上限はない。record の入れ子は `crate::syntax::MAX_NESTING` 未満（`scalar_record`）。長さ × 要素幅が wasm32 の memory を超える入力は
`tsuzuri_alloc` のトラップになり、`TsuzuriTrap` として報告される。

### 例

新 API（実装後に有効。未検証）。`examples/web/Physics.tz` の `next_positions`（`ref [f64] -> ref [f64] -> f64 -> f64 -> [f64]`）を呼ぶ。

```javascript
import { readFile } from "node:fs/promises";
import { load, TsuzuriTrap } from "./physics.mjs";

const api = await load(await readFile("physics.wasm"));
try {
  console.log(api.exports.next_positions(new Float64Array([0, 1]), new Float64Array([1, 1]), 1.0, 10.0));
} catch (error) {
  if (!(error instanceof TsuzuriTrap)) throw error;
  console.log(error.trap.reason);
}
```

拒否: `tsuzuri build Main.tz --emit bindings-js` は E2000（`--target wasm32` がない）、`-o physics.js` は E2000（`.mjs` でない）。

### Phase 2（設計方針）

- ブラウザーの threads glue: Worker 起動 script、COOP/COEP の検査（不備は明示的な例外にし、逐次へ黙って切り替えない）、共有 memory の初期化。
  `createThreadPool` と同じ protocol を使う。
- C#（`--emit bindings-cs`）: `LibraryImport` の宣言、所有 buffer の `SafeHandle`、借用入力の `ReadOnlySpan<T>`。native の共有ライブラリ出力
  （`--emit shared`）を同時に足し、G10 の Windows DLL 方針と揃える。
- Python（`--emit bindings-py`）: `ctypes` の宣言と、`tsuzuri_free` を呼ぶ変換。C++: C header の上の RAII ラッパー
  （`std::span`、`std::u16string_view`、`tsuzuri_free` を deleter にした `unique_ptr`）。
- どれも Phase 1 の記述子の表（`src/bindings.rs` の `table`）から生成する。

## 設計

### データ構造

```rust
// src/bindings.rs（新規）。src/lib.rs に `pub mod bindings;`
pub const ABI_VERSION: u32 = 1;
pub fn javascript(module: &CheckedModule) -> String;   // <name>.mjs の本文
pub fn declarations(module: &CheckedModule) -> String; // <name>.d.mts の本文
fn table(module: &CheckedModule) -> serde_json::Value; // {"abi","hostAbi","exports","imports","records"}
fn descriptor(ty: &Type, module: &CheckedModule) -> serde_json::Value;
fn typescript_type(ty: &Type, module: &CheckedModule, input: bool) -> String;
const DECLARATIONS: &str = "...";                     // TrapInfo, TrapSite, TsuzuriTrap, Borrowed, Bindings, load の固定部

// src/driver.rs
pub enum Emit { Executable, Object, Llvm, Header, Wasm, Wgsl, BindingsJs /* 新規 */ }
pub fn bindings_sidecar_path(output: &Path) -> PathBuf; // 新規。末尾の ".mjs" を ".d.mts" に替える
```

表の形（`serde_json::to_string` の compact 出力。`exports`・`imports` は名前の byte 順、`records` は `serde_json::Map` の既定の key 順）:

```json
{"abi":1,"hostAbi":true,
 "exports":[["add",["i64","i64"],"i64"],["area",["record:tz_record_8Geometry_5Point"],"f64"]],
 "imports":[["Main.now",[],"i64"]],
 "records":{"tz_record_8Geometry_5Point":{"size":16,"align":8,"fields":[["x",0,"f64"],["y",8,"f64"]]}}}
```

- `hostAbi` は `llvm_abi.rs` の `uses_host_abi(module)`。true なら glue は `memory`・`tsuzuri_alloc`・`tsuzuri_free` の export を要求する。
- `imports` は本体が `TypedExprKind::HostCall(import, _)` の関数ごとに `import.wasm_name`。unit 引数は除く（ABI から省略されるため）。
- `records` は `record_types`（ABI に現れる record）の各型。field の offset は下の「アルゴリズム」。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `--emit` の解析、help の `--emit KIND` 行 | `Some("bindings-js") => Emit::BindingsJs`。message と help の一覧に `bindings-js` を足す |
| driver | `src/driver.rs` | `Emit` | `BindingsJs` を足す |
| driver | `src/driver.rs` | `BuildOptions::validate` | 診断表の E2000 の 5 行。header の検査の直後に置く |
| driver | `src/driver.rs` | `BuildOptions::output_path` | `Emit::BindingsJs => "mjs"` |
| driver | `src/driver.rs` | `build_complete` | `Emit::Header` の分岐の隣に `Emit::BindingsJs`: E2004 の検査、本文 `crate::bindings::javascript(module)`、`.d.mts` を一時 directory へ書き `publish_outputs` の `sidecars` に足す。`windows_abi`・`runtime/wasm.ll` の追加・task runtime の検査・cache・text 出力の各条件で `Emit::Header` と同じ扱いにする |
| driver | `src/driver.rs` | `bindings_sidecar_path`（新規） | `trap_sidecar_path` の隣。`protect_sources` で検査する |
| ABI | `src/llvm.rs` | `mod host_abi` | `pub(crate) mod host_abi;` にする |
| ABI | `src/llvm_abi.rs` | `record_name`, `record_types`, `record_layout` | `pub(crate)` にする。中身は変えない（`uses_host_abi` は既に `pub(crate)`） |
| ABI | `src/llvm.rs` | `handle_c_name`（E12） | `pub(crate)` にする（E12 が done のとき） |
| 生成 | `src/bindings.rs`（新規） | `javascript`, `declarations`, `table`, `descriptor`, `typescript_type` | 表・宣言の生成。`Type` の match は ABI 型だけを列挙し、それ以外は `unreachable!("ABI type checked")` |
| 生成 | `src/runtime/bindings.mjs`（新規） | `load`, `TsuzuriTrap` | 固定の JS。`include_str!` で `.mjs` の末尾へ連結する |

### 生成 IR とランタイム

生成 IR・WASM・header は変えない。`.mjs` は 3 部からなる。

```text
// Generated by Tsuzuri <CARGO_PKG_VERSION>. Bindings ABI 1. Do not edit.
const TABLE = <table の JSON>;
<src/runtime/bindings.mjs の内容>
```

- `src/runtime/bindings.mjs` は `TABLE` を参照する ES module で、`node:` の import・`fetch`・大域状態を持たない。export は `load` と `TsuzuriTrap` だけ。
- `load` は表の記述子を変換関数の配列へ一度だけ解決する（呼び出しごとに記述子の文字列で分岐しない）。
- `.d.mts` は注釈行、record の interface（`record_name` の順）、ハンドルの型（名前順）、`Exports`、`Imports`、`DECLARATIONS` の順。

### アルゴリズム

```text
record offsets（record_layout と同じ規則）:
  offset = 0
  for (name, field) in record_fields(id, arguments):   # 宣言順
    (size, align) = record_layout(field)
    offset = ceil(offset / align) * align; emit [name, offset, descriptor(field)]; offset += size
  (size, align) = record_layout(record)

call(name, args):
  sig = signatures[name]; sig.check(args)            # TypeError/RangeError。instance に触れない
  state.instance ??= new WebAssembly.Instance(module, wrappedImports)
  try: lowered = lower(args); out = sig.out ? alloc(sig.outSize) : none
       value = lift(exports["tz_" + name](out?, ...lowered)); free(result, out, inputs 逆順); return value
  catch error: discard(); classify(error)             # 「数値・トラップ・native と WASM の差」の表
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、header を保存する。TypeScript を入れる。
- 確認: 次がすべて成功する。`host_abi` は `4 passed`、`tsc` は `Version 6.0.3`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-e13 && rm -rf /tmp/tz-e13/host_abi && cp -R tests/fixtures/host_abi /tmp/tz-e13/host_abi
target/release/tsuzuri build /tmp/tz-e13/host_abi/Main.tz --emit header -o /tmp/tz-e13/before.h
target/release/tsuzuri build tests/fixtures/host_imports --emit header -o /tmp/tz-e13/imports-before.h
cargo test --locked --test host_abi
npm ci --prefix vsc && vsc/node_modules/.bin/tsc --version
```

### 手順 2: `Emit::BindingsJs` と CLI

- 変更: `src/main.rs`、`src/driver.rs`（`Emit`、`BuildOptions::validate`、`BuildOptions::output_path`、`build_complete` の各条件、
  `bindings_sidecar_path`）、`src/lib.rs`、`src/bindings.rs`（新規。`javascript`・`declarations` は注釈行だけを返す）。
- 内容: 「段ごとの変更」の CLI と driver の行。`.d.mts` の path は `output.with_extension("d.mts")`（`.mjs` 終わりを検査済みなので正しい）。
  test を足す: `src/main.rs` の `selects_target_defaults_and_honors_path_separator` に `--emit bindings-js` の解析、
  `src/driver.rs` の `refuses_invalid_options_and_empty_wasm_modules` に診断表の E2000・E2004 の各行。
- 確認: `cargo test --locked --bin tsuzuri selects_target_defaults_and_honors_path_separator` と
  `cargo test --locked --lib refuses_invalid_options_and_empty_wasm_modules` がそれぞれ `1 passed`。`cargo test --locked` が成功する。

### 手順 3: ABI の型モデルを crate 内へ公開する（共有変更）

- 変更: `src/llvm.rs`（`pub(crate) mod host_abi;`）、`src/llvm_abi.rs`（`record_name`・`record_types`・`record_layout` を `pub(crate)`）。
- 内容: 可視性だけを変える。本体・呼び出し順は変えない。
- 確認: 手順 1 と同じ 2 つの header を `/tmp/tz-e13/after.h`・`imports-after.h` へ出し、`cmp` がどちらも差分なし。`cargo test --locked --test host_abi` が `4 passed`。

### 手順 4: 記述子の表

- 変更: `src/bindings.rs` の `table`・`descriptor`、`tests/bindings.rs`（新規）、`tests/fixtures/bindings/Main.tz`・`Geometry.tz`（新規）。
- 内容: 「データ構造」の表を作り、`javascript` は注釈行と `const TABLE = ...;` を返す。fixture は下の「E2E」の形。
- 確認: `cargo test --locked --test bindings` が `1 passed`（`bindings_table_lists_sorted_exports_imports_and_records`）。

### 手順 5: `.d.mts` の生成

- 変更: `src/bindings.rs` の `declarations`・`typescript_type`・`DECLARATIONS`、`tests/bindings.rs`。
- 内容: 「生成物の API」の形。ハンドルとコールバックの行は E12 の実装がある場合だけ。
- 確認: `cargo test --locked --test bindings` が `3 passed`（決定性と snapshot の 2 件を足す）。

### 手順 6: runtime の骨格・load の検査・スカラー

- 変更: `src/runtime/bindings.mjs`（新規）、`src/bindings.rs`（`include_str!` で連結）、`tests/bindings.mjs`（新規）。
- 内容: `load` の import・export 検査（JS の例外表の上 4 行）、スカラーの変換、instance の作成。
- 確認: `cargo build --release --locked && node tests/bindings.mjs target/release/tsuzuri` が E2E の 1–6・24–26 で `bindings: -O0 passed`、`bindings: -O3 passed`。

### 手順 7: buffer・文字列・record と所有権

- 変更: `src/runtime/bindings.mjs`、`tests/bindings.mjs`。
- 内容: 型対応表の buffer・文字列・record の行と、「評価順序・所有権・借用」の順序。
- 確認: 手順 6 と同じコマンドで E2E の 7–14・23 も成功する。

### 手順 8: 型付き import

- 変更: `src/runtime/bindings.mjs`、`tests/bindings.mjs`。
- 内容: import の wrapper（借用入力の複製、所有結果の確保と descriptor の書き込み、ホストの例外の記録）。E12 のコールバックの JS 関数化。
- 確認: E2E の 15–18 も成功する。

### 手順 9: トラップの分類と作り直し

- 変更: `src/runtime/bindings.mjs`（`TsuzuriTrap`、分類、`sites` の照合）、`tests/bindings.mjs`。
- 内容: 「数値・トラップ・native と WASM の差」の表。
- 確認: E2E の 19–21 も成功する。

### 手順 10: `withBorrowed`

- 変更: `src/runtime/bindings.mjs`、`tests/bindings.mjs`。
- 内容: 世代番号で無効化する `Borrowed`。
- 確認: E2E の 22 も成功する。

### 手順 11: TypeScript の型検査

- 変更: `tests/bindings_consumer.mts`（新規）、`tests/bindings.mjs`。
- 内容: 生成した `.d.mts` と同じ directory へ consumer を複写し、`vsc/node_modules/.bin/tsc --noEmit --strict --module nodenext
  --moduleResolution nodenext --target es2022` を実行する。consumer は正しい呼び出しと `// @ts-expect-error` を付けた誤り（`add(1, 2n)`、
  `copy_text(1)`、import の欠落）を持つ。tsc がなければ `bindings: tsc skipped (run npm ci --prefix vsc)` を出して続ける。
- 確認: `node tests/bindings.mjs target/release/tsuzuri` が `bindings: tsc passed` を出す。

### 手順 12: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md README.md _docs/guides/webassembly.md _docs/guides/native-interop.md _docs/feature-status.md` が成功する。

### 手順 13: 最終確認

- 確認: `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`、
  `node tests/bindings.mjs target/release/tsuzuri`、`node tests/examples.mjs target/release/tsuzuri`、`node tests/host_imports.mjs target/release/tsuzuri` が成功し、
  手順 1 と手順 13 の header が同一。

## テスト計画

### Rust テスト

`tests/bindings.rs`（新規）は `tests/host_abi.rs` と同じ方法で fixture を読み、`tsuzuri::bindings` を呼ぶ。

- `bindings_table_lists_sorted_exports_imports_and_records`: `tests/fixtures/bindings` の表。export は名前の byte 順（`add`、`add_u64`、`area`、…）、
  import は `Main.host_fail`・`Main.host_greeting`・`Main.host_scale`・`Main.now` の順で、`Main.now` の引数は空（unit を省く）。
  record は C の自然な配置から手で求めた値: `tz_record_4Main_5Flags` は size 12・align 4・`tiny` 0／`wide` 4／`flag` 8、
  `tz_record_4Main_6Sample` は size 24・align 8・`flags` 0／`amount` 16、`tz_record_8Geometry_5Point` は size 16・align 8・`x` 0／`y` 8。
- `bindings_are_deterministic`: `javascript` と `declarations` を 2 回ずつ生成して byte 単位で同一。先頭行が
  `// Generated by Tsuzuri {CARGO_PKG_VERSION}. Bindings ABI 1. Do not edit.`。
- `declarations_snapshot_for_small_module`: 一時 project（`record Point { x: f64, y: f64 }` と `export def add :: i64 -> i64 -> i64`）の `.d.mts` 全文を、
  手で書いた期待文字列と比べる（構造の snapshot。現在の出力から期待値を作らない）。
- `src/driver.rs`・`src/main.rs` の既存 test への追加は手順 2 のとおり。拒否の各行は code（E2000・E2004）と message の完全一致を見る。

### E2E

`tests/bindings.mjs`（新規）。fixture を `mkdtemp` の独立した root へ複写し（E03 の再帰探索）、glue を 1 回、WASM を `-O0`・`-O3`・
`-O3 --trap-info` で build し、`import()` した glue で下の case を各最適化で実行する。`node --check` で glue の構文も見る。

fixture の新しい部分（`/tmp/tz-work-E13/fx2` で `check` と wasm32 の build を確認済み。import は `tsuzuri/Main.now` など 4 つ）。
`copy_values`・`sum_float`・`checksum`・`make_bytes`・`copy_text`・`copy_utf8` の本体は `tests/fixtures/host_abi/Main.tz` と同じ。
`update` は `Sample { flags: Flags { … }, amount: value.amount + 1.0 }`、`Geometry.tz` は `record Point { x: f64, y: f64 }`。

```tsuzuri
record Flags { tiny: i8, wide: i16u, flag: bool }
record Sample { flags: Flags, amount: f64 }

extern def now :: unit -> i64
extern def host_scale :: ref [f64] -> f64 -> [f64]
extern def host_greeting :: ref utf8string -> utf8string
extern def host_fail :: i64 -> i64

def rec depth :: i64 -> i64
fn rec depth n = if n == 0 then 0 else depth (n - 1) * 3 + n

export def add_u64 :: i64u -> i64u -> i64u
fn add_u64 a b = a + b

export def widen :: i8 -> i16u -> i32u -> i64
fn widen a b c = a as i64 + b as i64 + c as i64

export def negate :: bool -> bool
fn negate value = !value

export def twice32 :: f32 -> f32
fn twice32 value = value + value

export def area :: Geometry.Point -> f64
fn area point = point.x * point.y

export def stamp :: i64 -> i64
fn stamp offset = now () + offset
```

`add`・`divide`・`deep`（`depth n`）・`scaled`（`host_scale values factor`）・`greeting`・`relay_fail` も同じ形で持つ。期待値は JS の BigInt・`Math.fround`・手計算から求める。

| # | 呼び出し | 期待 |
| --- | --- | --- |
| 1 | `add(40n, 2n)` | `42n` |
| 2 | `add(1, 2n)`、`add(2n ** 63n, 0n)` | `TypeError`、`RangeError`。直前に作った `Borrowed` が有効なまま（instance を捨てない） |
| 3 | `add_u64(2n ** 64n - 1n, 0n)`、`add_u64(-1n, 0n)` | `18446744073709551615n`、`RangeError` |
| 4 | `widen(-128, 65535, 4294967295)`、`widen(128, 0, 0)` | `4295032702n`、`RangeError` |
| 5 | `negate(true)`、`negate(1)` | `false`、`TypeError` |
| 6 | `twice32(0.1)` | `Math.fround(Math.fround(0.1) * 2)` |
| 7 | `copy_values(BigInt64Array.of(1n, -2n, 2n ** 63n - 1n))`、空の配列 | 同じ要素の新しい `BigInt64Array`、長さ 0 |
| 8 | `sum_float(Float64Array.of(0.5, 0.25))` | `0.75` |
| 9 | `checksum(Uint8Array.of(1, 2, 255))` | `258n` |
| 10 | `make_bytes(4n)`、`make_bytes(0n)` | `Uint8Array [0, 1, 2, 3]`、長さ 0 |
| 11 | `copy_text("a\ud800b😀")` | code unit 列が同一（孤立 surrogate を保つ） |
| 12 | `copy_utf8("é😀")`、`copy_utf8("\ud800")` | 同じ文字列、`TypeError` |
| 13 | `update({ flags: { tiny: -1, wide: 65535, flag: true }, amount: 1.5 })`、`amount` の欠落 | `amount: 2.5` で他は同じ、`TypeError` |
| 14 | `area({ x: 2, y: 3.5 })` | `7` |
| 15 | `stamp(5n)`（`"Main.now": () => 37n`） | `42n` |
| 16 | `scaled(Float64Array.of(1, 2), 3)`（ホストは受け取った配列を書き換えてから `map` する） | `Float64Array [3, 6]`（ホストの書き換えは入力に影響しない） |
| 17 | `greeting("世界")`（ホストは `"hello " + name`） | `"hello 世界"` |
| 18 | `relay_fail(1n)`（ホストが `marker` を throw） | `marker` と同一の object。次の `add(1n, 1n)` は `2n` |
| 19 | `divide(1n, 0n)`（`--trap-info` なし／あり + `sites`） | `TsuzuriTrap`、`reason: "trap"`、site 0／site > 0 で `path` が `Main.tz`、`line` は fixture 内の `a / b` の行（test が本文から探す） |
| 20 | 19 の後の `divide(-7n, 2n)` | `-3n`（BigInt `-7n / 2n`） |
| 21 | `deep(100000000n)`、続けて `deep(10n)` | `reason: "stack"`、`44281n`（JS BigInt の漸化式 `d(n) = d(n-1) * 3n + n`） |
| 22 | `withBorrowed("f64", 3, b => { b.view().set([1, 2, 3]); return sum_float(b); })` | `6`。callback の後の `b.view()` は `TypeError`、thenable を返すと `TypeError` |
| 23 | 1 MiB の文字列で `copy_text` を 64 回 | トラップしない（漏れれば固定 16 MiB の memory を使い切る） |
| 24 | `--wasm-feature threads` の `tests/fixtures/tasks/Main.tz` を load | threads の `Error`（JS の例外表） |
| 25 | `import` を省いて load、`host_abi` の wasm を bindings の glue で load | `missing import 'Main.now'`、`missing export` の `Error` |
| 26 | `examples/point/Point.tz` の glue と wasm（scalar だけ、memory の export なし） | `hypotenuse(3, 4)` が `5`、`WebAssembly.Module.imports` が空 |

手順 11 の tsc の検査もこの harness で行う。Node 20 で BigInt の多い case が V8 の内部エラーで落ちたら
`npx --yes --package=node@24 node tests/bindings.mjs target/release/tsuzuri` で再現を確かめる。

### 既存テストへの影響

`--emit` の不明値の message と help の一覧に `bindings-js` が加わる。これを文字列で比べる既存 test があれば、その期待値だけを直す。
header・IR・WASM の出力、`tests/host_abi.rs`・`tests/host_imports.mjs`・`tests/examples.mjs` の期待値は変わらない。

### 性能

glue は引数の検査と複製を足すだけで、生成コードは変えない。計測はせず、速度の主張もしない。`withBorrowed` が入力の複製を 1 回減らすことも計測していない。

## ドキュメント

- `docs/language.md`「公開 ABI」: `--emit bindings-js` と `.mjs`／`.d.mts`、型対応表（本チケットの表の要約）、所有権とトラップの規則。
- `docs/architecture.md`: module の表に `src/bindings.rs` と `src/runtime/bindings.mjs`。「不変条件」に「glue は module が宣言した import だけを渡し、import を増やさない。
  header は変えない」。「開発と検証」に `node tests/bindings.mjs target/release/tsuzuri`。
- `README.md`「CLI」の `--emit` の一覧、「Web／ゲーム」の最小例、「検証と性能測定」の test の一覧。
- `_docs/guides/webassembly.md`「Node.js から呼ぶ」を生成 glue の例にし、手書きの手順は「buffer ABI」に残す。「extern import」に型付き import。
- `_docs/guides/native-interop.md`「公開名と呼び出し」: native は従来どおり `--emit header` を使うと一文。`examples/web/README.md` に glue の生成手順。
- `_docs/feature-status.md` の E13 行を「Phase 1 実装済み（WASM 向け JavaScript／TypeScript）」に、`_features/README.md` の状態を更新する。

## 受け入れ条件

- [ ] `tsuzuri build --target wasm32 --emit bindings-js` が `<name>.mjs` と `<name>.d.mts` を出し、診断表の拒否がすべて期待どおり。
- [ ] E2E の 26 case が `-O0`・`-O3` で成功し、手書きの descriptor 操作なしに全 ABI 型・import・トラップを扱える。
- [ ] 生成物が同じ入力で byte 単位で同一で、`--emit header`・IR・WASM の出力が変わらない。
- [ ] 生成した `.d.mts` が TypeScript 6.0.3 の `--strict` で型検査に通り、誤った呼び出しを拒否する。
- [ ] glue は module が宣言した import だけを渡し、threads・IO・debug の module を明示的な例外で拒否する。
- [ ] 文書を更新し、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `.d.ts` は `.mjs` の宣言として使われない（TS7016、検証済み）。必ず `.d.mts` を出す。
- wasm32 の pointer は JS では符号付きの i32。2 GiB を超える memory（F11）で負になるので、offset に使う前に必ず `>>> 0`。
- `tsuzuri_alloc` の size と buffer の長さは i64 なので BigInt で渡す（E05 の例の `tsuzuri_alloc(16n)`）。
- 確保・export・import のどの呼び出しでも memory が grow し得る。view を変数に保持して使い回さない。import の wrapper の中で
  `tsuzuri_alloc` を呼んだ後も作り直す。
- `new TextDecoder("utf-16le")` は孤立 surrogate を U+FFFD に置き換える（検証済み）。`string` の復号に使わない。`TextEncoder` も黙って置き換えるので、
  `utf8string` の入力は先に `isWellFormed()` で検査する（Node 20 で使える。検証済み）。
- `String.fromCharCode(...codes)` は長い配列で引数の上限を超える。4096 単位に分ける。
- scalar だけの module は `memory`・`tsuzuri_alloc`・`tsuzuri_free` を export しない。`hostAbi` が false なら要求しない。
- record の field 名は Tsuzuri の名前を使う。C 用の `field_name`（`tz_` 付け）を JS に持ち込まない。`record_types` は import だけに現れる record も含む。
- トラップした instance には `tsuzuri_trap_site` の 1 回以外触れない。解放もしない。
- `serde_json` の `preserve_order` feature を有効にしない（key 順が変わり、決定性の test が意味を失う）。
- `cargo test --locked <pattern>` は 0 件でも成功する。`running N tests` を確かめる。

## 対象外

- Phase 2（D11）: ブラウザーの threads glue、C#・Python・C++ の生成、native の共有ライブラリ出力、ブラウザー main thread での非同期の作り直し。
- WebAssembly component model（WIT）、bundler の plugin、npm package の公開、CommonJS 出力、GUI フレームワークとの統合。
- Result／Maybe・union・タプル・関数値の ABI（E05 の範囲外）、非同期の import（B08 Phase 2）。

## 決定事項

### D1: CLI と出力ファイル

- 決定: `tsuzuri build --target wasm32 --emit bindings-js`。出力は `<name>.mjs` と、その隣の副出力 `<name>.d.mts`。`-o` は `.mjs` で終わる。
- 理由: 既存の `--emit` の値（生成物の種類）に揃い、Phase 2 で `bindings-cs`・`bindings-py` を足せる。本体は JavaScript なので旧案の `bindings-ts` より
  正確。`.d.ts` は `.mjs` の宣言にならない（TS 6.0.3 で TS7016、検証済み）。副出力は `.trap.json` と同じ `publish_outputs` で原子的に公開できる。
- 見直し提案: 起票時の案（`--emit bindings-ts`、`name.d.ts`）と依頼時の例（`<name>.d.ts`）を、上の検証結果により置き換えた。
- 状態: 既定案（実装者はこの案に従う）

### D2: Phase 1 の範囲

- 決定: WASM 向けの JavaScript ESM と TypeScript 宣言だけ。native の C ホストは既存の `--emit header` を使い、header は変えない。
- 理由: 両方とも既存の ABI の型モデルだけで作れ、新しい出力形式（共有ライブラリ）や toolchain を要しない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 型の対応

- 決定: 「型規則」の表。入力の配列は typed array だけ（通常の配列は受けない）。整数は範囲を検査し、符号なしの結果は `>>> 0`／`BigInt.asUintN`。
- 理由: 暗黙の切り詰め（ToInt32）を境界で防ぐ。typed array だけにすると検査と複製が一種類で済む。
- 状態: 既定案（実装者はこの案に従う）

### D4: 表と固定 runtime による生成

- 決定: Rust は記述子の表と宣言だけを生成し、変換は `src/runtime/bindings.mjs` の固定コードが `load` 時に関数の配列へ解決する。
- 理由: 生成する JS の量と Rust 側の文字列組み立てが最小になり、runtime を一か所で試験できる。呼び出しごとの文字列分岐を避ける。
- 状態: 既定案（実装者はこの案に従う）

### D5: 所有権と `withBorrowed`

- 決定: 入力は複製、所有結果は複製してすぐ `tsuzuri_free`。`withBorrowed(kind, length, callback)` は wasm 側の領域を callback の間だけ貸す。
- 理由: JS が wasm の pointer を持たなければ解放漏れと use-after-free が起きない。起票時の `withBorrowed(array, callback)` は配列を受け取るので結局
  複製が要る。領域を先に確保して JS から直接書く形にすると複製を 1 回減らせる。
- 状態: 既定案（実装者はこの案に従う）

### D6: トラップ

- 決定: トラップとスタック枯渇は常に `TsuzuriTrap`（`--trap-info` がなければ site 0）。例外が出た instance は捨て、次の呼び出しで作り直す。
- 理由: E14 D8 が E13 の glue にこの形を求める。起票時の「`WebAssembly.RuntimeError` をそのまま伝える」は、トラップした instance を
  使い続ける誤りを招く。
- 状態: 既定案（実装者はこの案に従う）

### D7: E14 との関係

- 決定: glue は E14 の D2–D4 と同じ規則を自前で実装し、`src/runtime/trap-boundary.mjs` を import しない。E14 は着手条件にしない。
- 理由: 生成物は利用者が配布する単独の file で、compiler の repository 内の path を参照できない。
- 状態: 既定案（実装者はこの案に従う）

### D8: import と namespace

- 決定: glue は module が宣言した import だけを `{ tsuzuri: {...} }` で渡し、import を足さない。`tsuzuri_threads`・`tsuzuri_io`・`tsuzuri_debug` の
  import を持つ module は load で拒否する。
- 理由: D-18（既定の WASM は import なし）を glue でも保つ。threads は `createThreadPool`、IO と Debug 出力は既存の runner の担当。
- 状態: 既定案（実装者はこの案に従う）

### D9: 決定性

- 決定: export・import は名前の byte 順、record は `record_name` の順。先頭行に compiler の版と Bindings ABI 1 を書き、時刻や path は書かない。
- 理由: 同じ入力から同じ byte 列を出す（GUIDE の決定的出力）。宣言順は module の読み込み順に依存しうる。
- 状態: 既定案（実装者はこの案に従う）

### D10: TypeScript の検査

- 決定: `vsc/` の lock 済み TypeScript 6.0.3 で consumer を `--strict` で検査し、未導入なら skip を表示する。snapshot は Rust test で常に行う。
- 理由: repository に既にある devDependency を使えば新しい依存が要らない。snapshot だけでは型の誤りを検出できない。
- 状態: 既定案（実装者はこの案に従う）

### D11: Phase 2 の言語と出力

- 決定: ブラウザーの threads glue、C#（`--emit bindings-cs` と `--emit shared`）、Python（`--emit bindings-py`）、C++ の RAII ラッパーを、この順で
  Phase 1 の表から生成する。共有ライブラリ出力は G10 の Windows DLL 方針と揃える。
- 理由: 共有ライブラリ出力は新しい link の形と配布の契約を持ち込み、ブラウザー glue は COOP/COEP の運用を前提にする。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D12: 診断コード

- 決定: 新しいコードを作らない。CLI の構成は E2000、export がないのは E2004、出力保護は E2003。glue の実行時の失敗は JS の例外で表す。
- 理由: どれも既存のコードの意味に収まる。
- 状態: 既定案（実装者はこの案に従う）
