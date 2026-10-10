# WebAssembly への出力

`tsuzuri build --target wasm32` または `wasm64` は、ブラウザや Node.js から呼べる WebAssembly を出します。入出力（I/O）を行わない純粋な計算モジュールであれば、WASI は不要です。`--emit bindings-js` で生成する型付きのグルー（JavaScript と TypeScript 宣言）から呼ぶのが基本で、グルーを使わずに `WebAssembly.instantiate` で直接インスタンス化し、`tz_` 接頭辞の付いたエクスポート関数を呼び出すこともできます。

このページの JavaScript の実行例は、Node.js 20 環境で実際に実行して検証しています。なお、`tsuzuri test --target wasm64` のテスト実行には Node.js 24 以降が必要ですが、wasm64 向けにビルドしたモジュールのスカラー export 関数自体は Node.js 20 でも正常に `42n` を返します。

## この記事のポイント

- `--emit bindings-js` が `<name>.mjs` と `<name>.d.mts` を出します。型の検査、バッファの確保と解放、トラップの扱いはこのグルーが行います。
- `--wasm-feature threads` を足すと、ブラウザで Web Worker のスレッドプールを作るグルーになります。COOP / COEP の無いページでは `Error` で、逐次実行には切り替えません。
- [Async 式](../async-tasks-and-lazy/async.md) の `Async.start` は、グルーの `bindings.async` がイベントループで駆動します。`Async.block_on` は `--wasm-feature jspi` で JSPI を使い、export が `Promise` を返します。
- 公開名は C と同じ `tz_` 接頭辞です。`export def add` は `tz_add` になります。
- `i64` は JavaScript の `BigInt`、`f32` / `f64` は `Number`、`bool` は 0 か 1 の `Number` です。グルーは `bool` を `boolean` に直します。
- 線形メモリの既定上限は 16 MiB、メインスタックは 1 MiB です。
- ホスト関数は、モジュール名 `tsuzuri`、関数名 `Main.tax_rate` のように import します。
- トラップしたインスタンスは再利用しません。グルーは `TsuzuriTrap` を投げ、次の呼び出しで作り直します。位置は `<output>.trap.json` と照合します。

## ターゲットを選ぶ

```sh
tsuzuri build Main.tz --target wasm32 -o shop.wasm
tsuzuri build Main.tz --target wasm64 -o shop64.wasm
```

既定の `--emit` は `wasm` です。オブジェクトや LLVM IR が欲しいときは `--emit object` か `--emit llvm` を足します。`wasm32` のポインタは 32 bit、`wasm64` は 64 bit です。JavaScript から見ると、wasm64 のポインタは `BigInt` です。

次の関数は、ネイティブでも WASM でも同じ計算です。ネイティブの `run` は 42 を出します。

```tsuzuri run=42
export def add :: i64 -> i64 -> i64 = \left right ->
    left + right

add 20 22
```

実行結果:

```text
42
```

同じソースを `wasm32` と `wasm64` に出し、Node.js から `tz_add(20n, 22n)` を呼ぶと、どちらも `42n` でした。`tsuzuri test --target wasm64` は、Node.js 24 以降で空の memory64 モジュールを検査してからテストを始めます。コンパイラ本体が wasm64 を拒否するわけではありません。

既定の wasm32 はホスト import を持ちません。`File`、`Dir`、`Env`、`Time`、`Random`（`Random.Pcg` を除く）、`Process` に到達するコードは `E2000` です。OS の API が要るなら native にするか、後述の `--wasm-host wasi` を使います。`Path` はホスト不要です。

```mermaid
flowchart TD
  src["Main.tz の export def"] --> build["tsuzuri build --target wasm32"]
  build --> wasm["shop.wasm"]
  wasm --> inst["WebAssembly.instantiate"]
  inst --> invoke["exports.tz_add などを呼ぶ"]
  invoke --> trap{"RuntimeError か"}
  trap -->|いいえ| ok["戻り値を使う"]
  trap -->|はい| fresh["インスタンスを捨て、必要なら作り直す"]
```

## JavaScript との型

公開 ABI は C と揃えています。詳しい表は [言語仕様の公開 ABI](../../../docs/language.md#公開-abi) にあります。JavaScript から見るときの対応は次です。

| Tsuzuri | WASM | JavaScript |
| --- | --- | --- |
| `i8` / `i16` / `i32` と符号なし | i32 | `Number`。符号なしで読むときは `value >>> 0` |
| `i64` / `i64u` | i64 | `BigInt`。符号なしは `BigInt.asUintN(64, value)` |
| `f32` / `f64` | f32 / f64 | `Number` |
| `bool` | i32 | `Number`。0 が false、それ以外は true。戻り値は 0 か 1 |
| `unit` の戻り値 | なし | `undefined` |
| ポインタ（wasm32） | i32 | `Number`。2 GiB 以上は負になるので `>>> 0` |
| ポインタ（wasm64） | i64 | `BigInt` |

`i128`、`f16`、`f128`、decimal、文字型、共用体、タプル、リスト、関数値、タスクは、この ABI では出せません。`unit` は引数にはできません。

スカラーだけのモジュールは、`tsuzuri_alloc` と `tsuzuri_free` を export しません（`memory` はリンカーの既定で export されます）。配列、文字列、レコードを受け渡すと、それらが足されます。

## 型付きのバインディングを生成する

`--emit bindings-js` は、`.wasm` を読み込んで型付きの関数として呼ぶ JavaScript モジュール `<name>.mjs` と、その TypeScript 宣言 `<name>.d.mts` を出します。引数の検査、`tsuzuri_alloc` / `tsuzuri_free` による複製と解放、記述子の読み書き、BigInt と Number の変換、メモリが伸びたあとのビューの作り直しは、このグルーが行います。

```sh
tsuzuri build shop --target wasm32 --emit bindings-js -o shop.mjs
tsuzuri build shop --target wasm32 -o shop.wasm
```

グルーはソースだけから作り、LLVM を通しません。`-O` は受け付けて無視します。`.wasm` は同じソースから別にビルドし、`-O3` や `--trap-info` はそちらに付けます。`--target wasm32` が必須で、`-o` は `.mjs` で終わります。TypeScript は `.mjs` の宣言を `.d.ts` から読まず、`.d.mts` からだけ読むためです。`--trap-info`、`--debug-info`、`--debug-output`、`--wasm-feature simd128`、`--allocator` を付けると `E2000`（`... is not valid for bindings output; pass it when building the .wasm`）、`export def` が 1 つもないと `E2004` です。同じソースからは、バイト単位で同じグルーができます。

次のモジュールを例にします。

```tsuzuri
record Item { price: i64, count: i32 }

extern def tax_rate :: unit -> i64

export def total :: ref Item -> i64 = \item ->
    let subtotal = item.price * item.count as i64
    subtotal + subtotal * tax_rate () / 100

export def per_unit :: i64 -> i64 -> i64 = \price count ->
    price / count

export def shout :: ref string -> string = \text ->
    clone_string text + "!"

export def scale :: ref [f64] -> f64 -> [f64] = \values factor ->
    Array.map (\value -> value * factor) values
```

生成された `shop.d.mts` の主な部分です。export は名前のバイト順、引数名は C ヘッダーと同じ `arg0`、`arg1` です。レコードの interface 名は C ヘッダーの typedef 名と同じで、フィールド名は Tsuzuri の名前のままです。

```typescript
export interface tz_record_4Main_4Item { price: bigint; count: number }
export interface Exports {
  per_unit(arg0: bigint, arg1: bigint): bigint;
  scale(arg0: Float64Array | Borrowed<Float64Array>, arg1: number): Float64Array;
  shout(arg0: string): string;
  total(arg0: tz_record_4Main_4Item): bigint;
}
export interface Imports {
  "Main.tax_rate"(): bigint;
}
```

`load` は `.wasm` のバイト列か `WebAssembly.Module` を受け取り、`exports`、`withBorrowed`、`ready` を持つオブジェクトを返します。ホスト関数は、WASM の import 名をキーにして `imports` に渡します。

```javascript
import { readFile } from "node:fs/promises";
import { load, TsuzuriTrap } from "./shop.mjs";

const bytes = await readFile(new URL("./shop.wasm", import.meta.url));
const api = await load(bytes, { imports: { "Main.tax_rate": () => 10n } });
console.log(api.exports.total({ price: 200n, count: 3 }));
console.log(api.exports.shout("tea"));
console.log(api.exports.scale(Float64Array.of(1.5, 2), 2));
try {
  api.exports.per_unit(10n, 0n);
} catch (error) {
  if (!(error instanceof TsuzuriTrap)) throw error;
  console.log(error.message, error.trap.reason);
}
console.log(api.exports.per_unit(10n, 4n));
```

実行結果:

```text
660n
tea!
Float64Array(2) [ 3, 4 ]
trap (site 0) trap
2n
```

グルーは `node:` の import も `fetch` も使わないので、ブラウザでも同じファイルを `import` できます。ブラウザでは `fetch` で取ったバイト列か、`WebAssembly.compileStreaming` で作ったモジュールを `load` に渡します。`--wasm-feature threads` を付けたときのグルーは [スレッドのグルー](#スレッドのグルー) で、`--wasm-feature jspi` を付けたときのグルーと `bindings.async` は [非同期計算と JSPI](#非同期計算と-jspi) で説明します。

### 型の対応

| Tsuzuri | TypeScript | JavaScript から渡すときの検査 |
| --- | --- | --- |
| `i8` / `i16` / `i32` と符号なし | `number` | 整数で、型の範囲内。範囲外は `RangeError` |
| `i64` / `i64u` | `bigint` | `bigint` で、型の範囲内。`i64u` の戻り値は符号なしで返る |
| `f32` / `f64` | `number` | `number`。`f32` はエンジンが最近接へ丸める |
| `bool` | `boolean` | `boolean` だけ。`0` や `1` は `TypeError` |
| `unit`（戻り値） | `void` | — |
| `ref [i64]` / `ref [f64]` / `ref [ubyte]` | `BigInt64Array` / `Float64Array` / `Uint8Array` か `Borrowed<…>` | 型付き配列だけ。通常の配列は `TypeError` |
| `ref string` | `string` | UTF-16 のコード単位のまま。孤立サロゲートも通る |
| `ref utf8string` | `string` | `isWellFormed()` でない文字列は `TypeError`。先頭の U+FEFF も文字として渡り、結果でも保たれる |
| `[i64]` などの所有結果 | 型付き配列 | 複製して返し、すぐ `tsuzuri_free` する |
| `string` / `utf8string` の所有結果 | `string` | 同上 |
| スカラーレコードとその `ref` | `tz_record_…` の interface | 全フィールドを検査する。固定長配列のフィールドは長さが一致する配列 |
| `extern type` のハンドル | `number & { readonly __tsuzuri: "tz_handle_…" }` | 0 以上 2^32 未満の整数 |
| コールバック（import の引数） | 関数型 | import の呼び出し中だけ呼べる |

引数の数、型、範囲、レコードのフィールドは、インスタンスに触れる前に検査します。失敗は `TypeError`（`argument 0 of 'total' field 'price' must be a bigint`）か `RangeError`（`argument 0 of 'widen' is out of range for i8`）で、インスタンスは捨てません。

### 所有権と withBorrowed

グルーは wasm のポインタを JavaScript に渡しません。入力は呼び出しごとに線形メモリへ複製し、所有結果は JavaScript の値へ複製してすぐ解放します。呼び出しが成功すれば、確保の残りは 0 です。ビューは使う直前に `memory.buffer` から作り直します。

大きな入力を複製せずに渡すときは、`withBorrowed(kind, length, callback)` を使います。`kind` は `"i64"`、`"f64"`、`"ubyte"` です。wasm 側に `length` 要素の領域を確保し、`Borrowed` として callback に渡し、callback が返るか例外を投げたあとに解放します。`view()` は呼ぶたびに現在のメモリ上の型付き配列を返します。解放後やインスタンスが捨てられたあとの `Borrowed` と、別の `load` が作った `Borrowed` を使うと `TypeError` です。callback が Promise などを返すと、解放したうえで `TypeError` を投げます。

```javascript
const scaled = api.withBorrowed("f64", 3, (buffer) => {
  buffer.view().set([1, 2, 3]);
  return api.exports.scale(buffer, 10);
});
console.log(scaled);
```

実行結果:

```text
Float64Array(3) [ 10, 20, 30 ]
```

### import とトラップ

import の関数は、借用入力の複製を受け取ります。ホストが保持しても安全です。所有結果を返す import は、型付き配列か文字列を返せば、グルーが `tsuzuri_alloc` で確保して記述子を書きます。ホストの関数が投げた値は、記録したうえで同じ値のまま呼び出し元へ届きます。戻り値の型が違うときは `result of import 'Main.tax_rate' must be a bigint` のような `TypeError` です。

export の呼び出し中に例外が出たら、グルーはそのインスタンスを捨て、次の呼び出しで同じモジュールと import から同期的に作り直します。トラップは `TsuzuriTrap`（`Error` の派生、`name` は `"TsuzuriTrap"`、`cause` は元の例外）で、`trap` に `{ reason, site, kind, path, line, column }` を持ちます。`reason` は `"trap"` か、スタック枯渇の `"stack"` です。`--trap-info` 付きの `.wasm` と、その `.trap.json` の `sites` を `load` に渡すと、位置が付きます。

```javascript
const { sites } = JSON.parse(await readFile(new URL("./shop-sites.wasm.trap.json", import.meta.url), "utf8"));
const api = await load(await readFile(new URL("./shop-sites.wasm", import.meta.url)), { imports: { "Main.tax_rate": () => 10n }, sites });
try {
  api.exports.per_unit(10n, 0n);
} catch (error) {
  console.log(error.message);
}
```

実行結果:

```text
trap at shop/Main.tz:10:5 (integer division by zero)
```

Chrome のメインスレッドは、8 MB を超えるモジュールの同期の作り直しを拒みます。そのときの呼び出しは `Error`（`the WASM instance could not be recreated synchronously after a failure; await ready() to recreate it asynchronously, then call again`）で失敗するので、`await api.ready()` で非同期に作り直してから呼び直します。`ready()` は、インスタンスがあればすぐに解決します。9.4 MB のモジュールで、Chrome と Playwright の Chromium は同期の作り直しを拒み、`ready()` のあとの呼び出しは成功しました。WebKit は同期のまま作り直せました。

グルーは、モジュールが宣言した import だけを渡し、import を足しません。`--wasm-feature threads`（後述のスレッドのグルーを使います）、IO の入口、`--debug-output`、`--wasm-host wasi`、`--allocator host` で作った `.wasm` は、`load` が `Error` で拒否します。別のソースから作った `.wasm` は、`module does not match bindings: missing export 'tz_add'; regenerate the bindings from the same sources` のように拒否します。グルーのポインタは 32 bit なので、`--target wasm64` の `.wasm` も拒否します（`bindings support wasm32 modules only; build the .wasm with --target wasm32`）。

## Node.js から直接呼ぶ

グルーを使わずに、`WebAssembly.instantiate` で直接呼ぶこともできます。以下は、Node.js からエクスポート関数を呼び出す具体的な例です（`scale` は `f64`、`is_positive` は `bool` を扱う関数です）。

```javascript
import { readFileSync } from "node:fs";

const bytes = readFileSync(new URL("./quote.wasm", import.meta.url));
const api = (await WebAssembly.instantiate(bytes)).instance.exports;
console.log(api.tz_add(20n, 22n).toString());
console.log(api.tz_scale(1.5));
console.log(api.tz_is_positive(3n));
console.log(api.tz_is_positive(0n));
```

実行結果:

```text
42
3
1
0
```

ブラウザでも、`fetch` で取ったバイト列を同じ `WebAssembly.instantiate` に渡せます。import が空なら、第 2 引数は要りません。ファイルの配信に特別なヘッダーは要りません。スレッドを使うときだけ、後述の COOP / COEP が必要です。

## 線形メモリとスタック

既定の上限は 16 MiB、メインスタックは 1 MiB です。どちらも 64 KiB ページに揃ったメモリ上限の内側に収まる必要があり、上限はスタックより少なくとも 64 KiB 大きくします。

```sh
tsuzuri build Main.tz --target wasm32 --wasm-max-memory 64MiB --wasm-stack-size 1MiB -o shop.wasm
```

`wasm32` の上限は 4 GiB − 64 KiB、`wasm64` は 16 GiB です。`Tsuzuri.toml` の `[wasm]` は、コマンドラインが省略したときの既定です。

```toml
[wasm]
max-memory = "64MiB"
stack-size = "1MiB"
```

オブジェクトを自分で `wasm-ld` するときは、スタックを `-z stack-size` で渡します。`--wasm-stack-size` が効くのは、コンパイラが WASM までリンクするときだけです。

`memory` を export するモジュールでは、呼び出しのあと線形メモリが伸びることがあります。`DataView` や `TypedArray` は、呼び出しのたびに `api.memory.buffer` から取り直してください。伸びる前に握ったビューは無効です。

## バッファを渡す

この節は、グルーを使わずにホストを書くときの ABI です。`--emit bindings-js` のグルーは、同じ手順を自動で行います。

`ref [i64]` は、ポインタと `i64` の長さです。wasm32 ではポインタが `Number`、長さが `BigInt` です。ホストは、要素数ぶんの領域と自然な整列を保証します。長さ 0 と null は空のバッファとして通ります。呼び出しのあいだ、その領域を書き換えたり解放したりしてはいけません。

次は、`10, 20, 12` の和が 42 になった呼び出しです。

```javascript
const input = api.tsuzuri_alloc(24n);
new BigInt64Array(api.memory.buffer, Number(input), 3).set([10n, 20n, 12n]);
console.log(api.tz_sum(input, 3n).toString());
api.tsuzuri_free(input);
```

実行結果:

```text
42
```

所有して返る配列は、先頭の out ポインタへ `{ ptr, len }` を書きます。記述子は 16 バイトで、オフセット 0 がポインタ、オフセット 8 が i64 の長さです。wasm32 のポインタは i32 なので、残りは埋まります。返したバッファはホストの所有です。`tsuzuri_free` を 1 回だけ呼びます。

`export def make_tag :: i64 -> [ubyte]` を、長さ 3 で呼んだ結果は ASCII の `ABC` でした。

```javascript
const out = api.tsuzuri_alloc(16n) >>> 0;
api.tz_make_tag(out, 3n);
const view = new DataView(api.memory.buffer);
const pointer = view.getUint32(out, true);
const length = Number(view.getBigInt64(out + 8, true));
const text = new TextDecoder().decode(new Uint8Array(api.memory.buffer, pointer, length));
api.tsuzuri_free(pointer);
api.tsuzuri_free(out);
```

`>>> 0` は、wasm32 で 2 GiB 以上のアドレスが負の `Number` になるのを避けるためです。記述子と中身の両方を解放します。`utf8string` に不正な UTF-8 を渡すとトラップします。`string` は UTF-16 のコード単位で、孤立サロゲートもそのまま通ります。

## SIMD とスレッド

`--wasm-feature simd128` は、128-bit SIMD を出す許可です。`wasm32` と `wasm64` の object、LLVM IR、WASM で使えます。relaxed SIMD は受け付けません。スカラーだけのモジュールに付けても、追加の import は出ませんでした。

`--wasm-feature threads` は wasm32 の object か WASM だけです。スカラーだけのモジュールでも、共有メモリの `env.memory` と `tsuzuri_threads.worker_ready` を import しました。ワーカーを起動するランタイムは `tsuzuri_threads.spawn_workers` も宣言します。その呼び出しに到達しないモジュールでは、リンカーが import から外します。

ホストの実装は、リポジトリの `src/runtime/wasm-threads.mjs` が Node.js 向けの参照です。ブラウザ向けには、後述の [スレッドのグルー](#スレッドのグルー) を生成できます。自分で書くときの条件は [Web ホストの README](../../../examples/web/README.md) にあります。

- 共有の `WebAssembly.Memory` を、モジュールが要求する最大ページ数で作る。既定は 256 ページ、つまり 16 MiB です。
- `tsuzuri_threads_init` にワーカー数を渡してから計算を呼ぶ。
- 各ワーカーの `__stack_pointer`、`tsuzuri_stack_base`、`tsuzuri_stack_top` を、独立した 256 KiB の範囲に設定してから `tsuzuri_thread_entry` を呼ぶ。
- ブラウザでは、HTTPS か localhost で `Cross-Origin-Opener-Policy: same-origin` と `Cross-Origin-Embedder-Policy: require-corp` を配信する。UI スレッドでは atomic wait できないので、計算の呼び出し元も Worker に置きます。

COOP / COEP が使えないからといって、スレッド要求を黙って逐次実行へ置き換えないでください。WASI と threads、`--allocator host` と threads は、同時には指定できません。

共有状態と `Channel` の型は、既定の wasm32 では import を増やしません。`Atomic` は通常の命令、`Mutex` はロックを示す 1 つのフラグ、`Task.scope` の子どもは `index` の昇順の逐次実行で、`Channel` の操作はモジュールの中の IR です（満たされない待ちはその場でトラップします）。`--wasm-feature threads` では、`Atomic` は WASM の atomic 命令（`i64.atomic.rmw.add` など）になり、`Task.scope` の子どもは Workers で動きます。`Mutex` と `Channel` も使えます。

- ランタイムは `task-wasm-threads.c` の C で、`tsuzuri_mutex_lock`・`tsuzuri_mutex_unlock`・`tsuzuri_mutex_parallel_ok`・`tsuzuri_mutex_wait_ok` と `tsuzuri_channel_send`・`recv`・`clone_sender`・`close` を定義します。import は増えず、ホストのグルー（`wasm-threads.mjs`、`bindings-threads.mjs`）は変わりません。
- `Mutex` のロックの語は、セルの先頭にあります（0 解放、1 保持、2 保持して待ちがいる）。待つスレッドは、プール共有の `epoch` の語で `memory.atomic.wait32` します。ロックを解放して待ちがいたとき、`epoch` を進めて全員を起こします。ホストが失敗を伝えるときも、同じ語を進めて起こすので、ロックを待つスレッドも、失敗したプールでは待ち続けずにトラップします。
- スレッドごとの状態（いま持っているロック、チャンネルの待ちで手伝っている仕事の深さ）は、C の `address_space(1)` の変数、つまりインスタンスごとの WebAssembly のグローバルです。各 Worker は自分のインスタンスを作るので、グローバルはスレッドごとの変数になり、TLS（`__wasm_init_tls`）の領域を作って初期化する手順は要りません。
- `Channel` の待ちは、`epoch` で眠り、チャンネルが変わるたび（待っている者がいるとき）に全員を起こして、各自が条件を見直します。「全員が待っている」の判定は、`epoch` を進めた時点からの、待ちの登録の数で行います。眠るスレッドは、登録した `epoch` が動くまで眠り続けるので、遅れて届いた通知で二重に数えられることはありません。通知は、ランタイムの lock を放してから送ります。スレッド数は、プールを始めた後はメインとワーカーの数、始める前は 1 です。最後に登録したスレッドが、チャンネルの待ちを含む全員の待ちを見つけると、その待ちスレッド全員に判定を伝え、各自がトラップします。トラップは、プール全体を止めます。
- ブラウザの UI スレッドは atomic wait できないので、計算を呼ぶ側を Worker に置きます（上の「COOP / COEP」の節のとおりです）。`Mutex` と `Channel` の待ちも、同じ制約を受けます。
- 検証は `tests/wasm_threads.mjs`（本物の Worker の `createThreadPool` で、`Mutex` のロックの競合、生産者と消費者、3 段のパイプライン、複数の生産者、作業キュー、ピンポン、デッドロックのトラップを 1 から 4 スレッドで）と、`tests/bindings_threads.mjs`（グルーのコーディネーターとヘルパーで、ロックとパイプライン）です。

（[Mutex](../built-in-types-and-modules/mutex.md)、[Channel](../built-in-types-and-modules/channel.md)）

### スレッドのグルー

`--emit bindings-js` に `--wasm-feature threads` を足すと、ブラウザで Web Worker のスレッドプールを作るグルーを出します。`.wasm` も、同じソースから `--wasm-feature threads` でビルドします。

```sh
tsuzuri build shop --target wasm32 --wasm-feature threads --emit bindings-js -o shop.mjs
tsuzuri build shop --target wasm32 --wasm-feature threads -O3 -o shop.wasm
```

`load` は最初に、ページが cross-origin isolated か（`crossOriginIsolated` が true で、`SharedArrayBuffer` があるか）を確かめます。COOP / COEP の無いページでは、ワーカーを起動せずに `Error`（`WASM threads need a cross-origin isolated page: ...`）を投げます。逐次実行には切り替えません。

プールの手順は `src/runtime/wasm-threads.mjs` と同じです。共有の `WebAssembly.Memory` を `.wasm` の要求する最大ページ数で作り、`workers` 個の補助ワーカーと、export を実行する調整役のワーカーを 1 つ起動します。`WebAssembly.Module` を渡したときは上限を読めないので、既定の 256 ページです。`memory` で自分のメモリを渡すこともできます。ブラウザのメインスレッドは atomic wait できないので、export は調整役のワーカーで動き、すべて `Promise` を返します。`workers` は 0 から 31 で、既定は `navigator.hardwareConcurrency - 1` です。0 なら、調整役がすべてのタスクを自分で実行します。グルーは自分のファイル（`import.meta.url`）を module worker として起動するので、`.mjs` はバンドルせずに、そのまま同じオリジンから配信します。

ホスト関数は、ワーカーごとのインスタンスに渡します。関数の代わりに、`createImports({ workerId, data })` を export するモジュールの URL を `importsModule` に渡します。`workerId` は調整役が 0、補助ワーカーが 1 からです。`data` は `importData` の構造化複製で、`SharedArrayBuffer` は共有されます。import を持つモジュールでは、`importsModule` が必須です。

上の `shop` を、次のソースに置き換えた例です。1 万要素の `Parallel.map` は 3 つのチャンクに分かれ、プールのスレッドで並列に動きます。

```tsuzuri
extern def tax_rate :: unit -> i64

export def with_tax :: ref [i64] -> [i64] = \prices ->
    Parallel.map (\price -> price + price * tax_rate () / 100) prices

export def per_unit :: i64 -> i64 -> i64 = \price count ->
    price / count
```

`host.mjs` は、どのワーカーにも同じ税率を返します。

```javascript
export function createImports({ workerId }) {
  return { "Main.tax_rate": () => 10n };
}
```

ページが読み込む `app.mjs` です。

```javascript
import { load, TsuzuriTrap } from "./shop.mjs";

const bytes = await (await fetch("./shop.wasm")).arrayBuffer();
const api = await load(bytes, { workers: 2, importsModule: new URL("./host.mjs", import.meta.url) });
const prices = BigInt64Array.from({ length: 10000 }, (_, index) => BigInt(index));
const taxed = await api.exports.with_tax(prices);
console.log(taxed[200].toString(), taxed.length);
try {
  await api.exports.per_unit(10n, 0n);
} catch (error) {
  console.log(error instanceof TsuzuriTrap, error.message);
}
try {
  await api.exports.per_unit(10n, 4n);
} catch (error) {
  console.log(error.message);
}
await api.close();
```

COOP / COEP を付けて配信したページの、コンソールの出力です。

```text
220 10000
true trap (site 0)
the WASM thread pool stopped after a failure; load the module again
```

引数は、1 スレッドのグルーと同じ検査をページ側で行ってから送ります。`TypeError` や `RangeError` では、プールは止まりません。引数と結果は構造化複製で受け渡すので、`withBorrowed` はありません。トラップやホスト関数の例外は、どのワーカーで起きてもプール全体を止めます。トラップならその呼び出しは `TsuzuriTrap` で失敗し、補助ワーカーのトラップなら、そのワーカーのサイト ID が付きます。ホスト関数が投げた値と、グルーが import の結果を拒んだ `TypeError`・`RangeError` は、補助ワーカーで起きても、その値の構造化複製で呼び出しが失敗します。待っていた呼び出しとそれ以降の呼び出しも失敗します。作り直しは自動ではないので、`load` からやり直します。`close()` はワーカーを終了し、それ以降の呼び出しは `the WASM thread pool is closed` で失敗します。

この例とリポジトリの `tests/bindings_threads.mjs` は、Chrome（headless）と、Playwright の Chromium、WebKit で、COOP / COEP 付きのページでは 3 スレッドが同時に動くこと、ヘッダーの無いページでは `Error` になることを確かめました。Firefox は未確認です。Node.js から使うときは、このグルーではなく `src/runtime/wasm-threads.mjs` を使います。

## 非同期計算と JSPI

[Async 式](../async-tasks-and-lazy/async.md) の実行器に到達するプログラム（`Async.start` か `Async.block_on` を使うもの）は、`tsuzuri_async_poll`（`(now: i64) => i64`）と `tsuzuri_async_complete`（`(operation: i64, value: i64) => void`）を export します。実行器に到達しないプログラムの export と import は変わりません。

`Async.start` は import を足しません。ホストが `tsuzuri_async_poll` を回し、`Async.host` の操作の完了を `tsuzuri_async_complete` で渡します。`--emit bindings-js` のグルーは、これを自動で行います。export の呼び出しと `complete` のあと、またいちばん早いタイマーの時刻に、`performance.now()` のミリ秒で poll します。宣言には次が加わります。

```typescript
export interface AsyncHost {
  complete(operation: bigint, value: bigint): void;
  settled(): Promise<void>;
}
export interface Bindings { readonly async: AsyncHost }
```

`complete` は `bigint` の操作 ID と値を受け取ります。ほかの型は `TypeError`、`i64` の範囲外は `RangeError` です。`settled()` は、計算が残っていない状態になると解決します。実行器の失敗では拒否し、再作成まで失敗を保持します。待っていない ID を `complete` に渡すとトラップで、インスタンスを捨てます。待っていた計算も失われます。使い方の例は [Async 式](../async-tasks-and-lazy/async.md#ホストが駆動する実行asyncstart) にあります。

実行器のモジュールは `tsuzuri_async_set_epoch(i64)` も export します。グルーは、新しいインスタンスで操作を開始する前に、1 以上 2^24 未満の世代を設定します。同じ生成モジュール内では、複数の `load` とインスタンスの再作成を通じて世代を再利用しません。別のインスタンスや破棄したインスタンスの ID を `complete` に渡すと `TypeError` になり、新しい計算へ取り違えて渡しません。別々に生成したグルーや生の WASM の間では、ホストが操作 ID と実行器を対応付けます。生の WASM を再作成するホストも世代を変えます。操作の開始後に世代を変更するとトラップします。

`Async.block_on` は、待つ間に WebAssembly のスタックを中断します。そのために JavaScript Promise Integration（JSPI）を使います。`--wasm-feature jspi` を付けて、`.wasm` とグルーの両方をビルドします。

```sh
tsuzuri build app --target wasm32 --wasm-feature jspi --emit bindings-js -o app.mjs
tsuzuri build app --target wasm32 --wasm-feature jspi -O3 -o app.wasm
```

- モジュールは `tsuzuri_async.clock`（`() => i64`、ミリ秒）と `tsuzuri_async.wait`（`(deadline: i64) => void`）を import します。`wait` は `WebAssembly.Suspending` で包んだ関数で、期限か操作の完了で解決する `Promise` を返します。期限が `i64` の最大値なら、完了だけを待ちます。
- `Async.block_on` に到達する export は、`WebAssembly.promising` で包んで呼びます。JSPI のグルーは、すべての export を `Promise` を返す関数にします。宣言の戻り値も `Promise<T>` です。
- JSPI のグルーは export を呼び出し順に直列化し、待っている間に次の呼び出しで同じスタックや結果を使いません。入力は呼び出し時点のコピーで、`withBorrowed` はありません。生の `WebAssembly.promising` の呼び出しも、前の呼び出しを await してから次を始めます。
- export が待っている間も JavaScript は動きます。`bindings.async.complete` は、待っている export を起こします。
- `--wasm-feature jspi` は、`wasm32` と `wasm64` の `object`、`llvm`、`wasm` と、wasm32 の `bindings-js` で使えます。`--wasm-feature threads` や `--wasm-host wasi` とは同時に指定できません（`E2000`）。
- `--wasm-feature jspi` なしで `Async.block_on` に到達すると、`.wasm` のビルドだけでなく `--emit bindings-js` の生成も `E2000` です。`.wasm` とグルーの両方に同じオプションを付けます。JSPI を使う `.wasm` を JSPI なしのグルーで読むと、`load` が `module does not match bindings: it waits through JSPI; generate the bindings with --wasm-feature jspi as well` で失敗します。
- JSPI（`WebAssembly.Suspending` と `WebAssembly.promising`）を持つエンジンが要ります。リポジトリの `tests/async.mjs` で、Node.js 24 で動くことを確かめました。Node.js 20 にはありません。

`--wasm-feature threads` は、実行器に到達するプログラムを `E2000` で拒否します。現在の WASM worker は Async 実行器の TLS を設定しません。

## WASI

`--wasm-host wasi` は、標準入出力と `File`、`Dir`、`Env`、`Time`、`Random`、`Process` を WASI preview 1 の import へ下げます。wasm32 の object か WASM だけです。既定の wasm32 は、これらの API に到達した時点でビルドを拒否します。

WASI を付けない計算モジュールは、WASI もブラウザ固有の import も持ちません。ホストを用意できない環境へ計算だけを渡すときは、こちらを使います。

## ホスト関数を import する

`extern def tax_rate :: unit -> i64` は、WASM ではモジュール `tsuzuri`、名前 `Main.tax_rate` の import です。ファイル名がモジュール名になります。到達しない `extern` は import セクションに出ません。

```tsuzuri
extern def tax_rate :: unit -> i64

export def with_tax :: i64 -> i64 = \cents ->
    cents + cents * tax_rate () / 100
```

この断片は型検査できます。実行にはホストが要るので、`run=` は付けていません。200 セントに 10% を足すホストは、次のとおりです（グルーを使うときは `load` の `imports` に `"Main.tax_rate": () => 10n` を渡します）。実行結果は `220` でした。

```javascript
import { readFileSync } from "node:fs";

const bytes = readFileSync(new URL("./tax.wasm", import.meta.url));
const { instance } = await WebAssembly.instantiate(bytes, {
  tsuzuri: {
    "Main.tax_rate": () => 10n,
  },
});
console.log(instance.exports.tz_with_tax(200n).toString());
```

実行結果:

```text
220
```

`extern "env" "host_now" def host_now :: unit -> i64` のように文字列を 2 つ書くと、import のモジュール名と関数名を自分で指定します。文字列が 1 つの `extern "sqrt" def ...` は、モジュール名 `tsuzuri`、import 名はその文字列です。3 つ以上は `E0002` です。名前の制約は [言語仕様のリンク名](../../../docs/language.md#リンク名) を見てください。

所有して返すバッファは、ホストが `tsuzuri_alloc` で確保した領域の所有権を Tsuzuri へ渡します。借用入力や静的領域を、所有結果として返してはいけません。

## トラップと trap.json

`build` は既定では位置情報を入れません。`--trap-info` を付けると、成果物の隣に `<output>.trap.json` を書き、WASM は `tsuzuri_trap_site` を export します。正常な呼び出しの開始時に、この ID はリセットされません。直近のトラップです。

表は 1 つの JSON オブジェクトです。`version` は 1、`sites` の各要素は `id`、`kind`、`path`、`span` です。`span` の行と列は、診断 JSON と同じく 1 始まりです。`left / right` を含むオブジェクトを `--trap-info` で出すと、ゼロ除算と符号付きオーバーフローの 2 件が同じスパンに付きました。

```text
{"version":1,"sites":[{"id":2,"kind":"integer division by zero","path":"trap/Main.tz","span":{"start":60,"end":72,"line":2,"column":5,"end_line":2,"end_column":17}}]}
```

上は実ファイルの先頭要素だけを短くしたものです。完全なファイルには、同じ除算のオーバーフローなど、コンパイラが付けたサイトが続きます。

トラップすると `WebAssembly.RuntimeError` になります。Tsuzuri はスタックを巻き戻さないので、そのインスタンスのヒープとシャドウスタックは中途半端なままです。次の呼び出しでは、同じモジュールから新しいインスタンスを作ってください。古い `memory.buffer` のビューと、未解放の所有バッファは無効です。所有結果は、次の `call` の前に複製し、`tsuzuri_free` します。

リポジトリの `src/runtime/trap-boundary.mjs` は、1 回の呼び出しを境界で包みます。成功は `{ ok: true, value }`、トラップは `{ ok: false, trap: { reason, site, kind, path, line, column } }` です。`kind` 以下は、サイドテーブルに ID があるときだけ付きます。スタック枯渇は `reason: "stack"`、`site: 0` です。threads 付きモジュールは、この境界では拒否します。ワーカーごとではなく、プール全体を捨てます。

> [!WARNING]
> トラップ後のインスタンスで `tz_` 関数を続けると、壊れたヒープを読みます。境界がインスタンスを作り直すのは、そのためです。

## まとめ

- `--emit bindings-js` のグルーが、型の検査、複製と解放、import、トラップを受け持ちます。
- `export def` は `tz_` 名で出ます。`i64` は `BigInt`、浮動小数点と `bool` は `Number` です。
- 計算だけのモジュールは import なしで instantiate できます。OS API は WASI か native です。
- バッファはポインタと長さ、所有結果は 16 バイトの記述子です。呼び出しのあとビューを取り直します。
- `simd128` は許可、`threads` は共有メモリとワーカーのホストが必要です。ブラウザでは、`--emit bindings-js --wasm-feature threads` のグルーがプールを作ります。
- `Async.start` の実行器は `tsuzuri_async_poll` と `tsuzuri_async_complete` で駆動し、グルーでは `bindings.async` が受け持ちます。`Async.block_on` は `--wasm-feature jspi` が要ります。
- `--trap-info` の `.trap.json` とサイト ID で位置を引き、トラップしたインスタンスは捨てます。

## 関連項目

- [コンパイラ オプション](option.md)
- [Async 式](../async-tasks-and-lazy/async.md)
- [ネイティブ連携 (C ABI)](native-interop.md)
- [診断メッセージとエラーコード](diagnostics.md)
- [言語仕様の公開 ABI](../../../docs/language.md#公開-abi)
- [言語仕様のホスト関数](../../../docs/language.md#ホスト関数のインポート)
- [言語仕様のトラップ位置](../../../docs/language.md#トラップ位置)
- [言語リファレンスの目次](../index.md)
