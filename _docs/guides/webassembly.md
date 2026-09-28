# WebAssembly と JavaScript ホスト

[ドキュメントのトップ](../README.md)

WASM は計算モジュールとして生成します。入出力を使わないプログラムでは WASI、.NET、JavaScript ランタイムの import は不要です。標準入出力には [IO](../library-reference/io.md) を使い、tsuzuri_io のホスト関数へ接続します。DOM、イベント、ファイル、ネットワークはホスト側で実装します。

## モジュールを作る

独立した `target/wasm-demo/Kernel.tz` の例です。

```tsuzuri project=wasm file=Kernel.tz
export def add :: i64 -> i64 -> i64 = \left right -> left + right

export def make_bytes :: i64 -> [ubyte] = \count -> new [ubyte](count, index -> index as ubyte)
```

```sh
./target/release/tsuzuri build target/wasm-demo/Kernel.tz --target wasm32 -o target/kernel.wasm
```

Clang と wasm-ld が必要です。LLVM の開発ヘッダーや WASI SDK をアプリケーションへ同梱する方式ではありません。

## Node.js から呼ぶ

次のコードはリポジトリルートを作業ディレクトリとして実行します。

```javascript
import { readFile } from "node:fs/promises";

const { instance } = await WebAssembly.instantiate(await readFile("target/kernel.wasm"));
const api = instance.exports;
console.log(api.tz_add(20n, 22n).toString());

const out = api.tsuzuri_alloc(16n);
let pointer = 0;
try {
    api.tz_make_bytes(out, 4n);
    const view = new DataView(api.memory.buffer);
    pointer = view.getUint32(out, true);
    const length = Number(view.getBigInt64(out + 8, true));
    const bytes = new Uint8Array(api.memory.buffer, pointer, length).slice();
    console.log(Array.from(bytes));
} finally {
    api.tsuzuri_free(pointer);
    api.tsuzuri_free(out);
}
```

最初は `42`、次は 0、1、2、3 のバイト配列を表示します。ホストへ複製する前にバッファを free しないでください。トラップ後の回復やランタイムの再利用を一般に保証する例ではありません。

## scalar ABI

公開名は `tz_name` です。i32 と f32 / f64 は JavaScript の Number、i64 は BigInt です。i64 引数へ `42` ではなく `42n` を渡します。

WASM の整数の返却値は符号付きとして見えるため、符号なし値として扱うなら 32-bit は `value >>> 0`、64-bit は `BigInt.asUintN(64, value)` を使います。bool は 0 / 1、unit 結果は undefined です。

## buffer ABI

入力の借用バッファは pointer が Number、要素数が BigInt です。長さの単位は i64 / f64 配列なら要素、UTF-16 ならコード単位、UTF-8 ならバイトです。文字列の暗黙変換はありません。

所有結果は先頭引数の out pointer に次の descriptor を書きます。

| offset | フィールド |
| --- | --- |
| 0 | i32 pointer |
| 8 | i64 length |
| 全体 | 16 byte、alignment 8 |

allocator や公開関数の呼び出しで memory が grow し得ます。DataView / TypedArray はその呼び出し後に作り直します。古い buffer に結び付いた view を再利用しないでください。

借用入力は解放せず、所有結果は一度だけ tsuzuri_free します。out descriptor 自体を allocator で作った場合、その領域も別に解放します。scalar-only のモジュールには不要な allocator export は追加しません。

## extern import

`Clock.tz` の `extern def now :: unit -> i64` は、WASM では module が `tsuzuri`、name が `Clock.now` です。ホストの import object は次の形です。

```javascript
const imports = {
    tsuzuri: {
        "Clock.now": () => 40n,
    },
};
```

WebAssembly.instantiate の第二引数へ渡します。到達する extern だけを import し、未使用 extern のためにダミー実装を用意する必要はありません。同期呼び出しであり、Promise を返す非同期ホスト関数の自動待機はありません。

## SIMD128

```sh
./target/release/tsuzuri build target/wasm-demo/Kernel.tz --target wasm32 --wasm-feature simd128 -o target/kernel-simd.wasm
```

既定は SIMD 不要の成果物です。opt-in した成果物は SIMD 対応エンジンを要求し、未対応環境では validation が失敗します。relaxed-simd は受理せず、数値の順序、NaN、符号付きゼロ、トラップを緩めません。

未知・重複 feature や、native / check / run / header への指定は `E2000` です。threads と simd128 は別の opt-in 機能です。

## ブラウザーとメモリ

ブラウザーでは fetch で取得した bytes を instantiate できます。instantiateStreaming を使うならサーバーの MIME を application/wasm にします。通常の WASM サンプルは SharedArrayBuffer を要求しません。

通常の構成は 1 MiB の stack-first 領域と、16 MiB の線形メモリ上限を持ちます。文字列、配列、捕捉環境なども同じ上限を共有します。呼び出しスタックは WASM エンジンの制限も受けます。

run は native の実行コマンドです。WASM の言語内テストには `tsuzuri test --target wasm32`、アプリケーションの実行にはこのようなホストを使います。

## 関連項目

- [WASM threads](wasm-threads.md)
- [C ABI と信頼境界](native-interop.md)
- [既存の Web ホスト](../../examples/web/README.md)
- [バッファ移送の実装例](../../examples/web/simulation.mjs)
