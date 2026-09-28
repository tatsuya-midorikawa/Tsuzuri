# JSON ファイルを集計するコマンド

[実装例の一覧](README.md) · [ドキュメントのトップ](../README.md)

JSON ファイルから測定値を読み、件数・合計・最小・最大・平均を表示するコマンドを作ります。Node.js にファイル I/O と JSON 解析、Tsuzuri に数値計算を担当させる、実際のホスト連携例です。

## 完成時の動作

入力ファイルの内容:

```json
[12, 18, 6, 24]
```

コマンドの出力:

```json
{
  "count": 4,
  "sum": 60,
  "minimum": 6,
  "maximum": 24,
  "average": 15
}
```

数値配列以外、空配列、有限でない値、100,000 件を超える入力は拒否します。これはアプリ側の入力制限で、Tsuzuri 全体の配列件数制限ではありません。負の測定値と小数は受け付けます。

## 必要な環境と構成

ビルド済み Tsuzuri、Clang / wasm-ld、Node.js 20 以降が必要です。追加の npm パッケージは使いません。

```text
target/file-statistics/
    Stats.tz
    report.mjs
    readings.json
    stats.wasm       (ビルドで生成)
```

Stats はライブラリなので Main.tz は不要です。Node.js が export した関数を呼び出して実行します。

## Tsuzuri の計算モジュール

`Stats.tz` の全内容です。返却配列の順序を「合計、最小、最大、平均」と定めます。

```tsuzuri project=statistics file=Stats.tz
export def summarize :: ref [f64] -> [f64]
fn summarize values =
    assert (values.length > 0)
    let total = Array.sum_kahan values
    let minimum = Option.get (Array.min values)
    let maximum = Option.get (Array.max values)
    new [total, deref minimum, deref maximum, total / (values.length as f64)]

test "summarizes multiple readings" =
    let input = [12.0, 18.0, 6.0, 24.0]
    let actual = summarize ref input
    let expected = [60.0, 6.0, 24.0, 15.0]
    Test.equal (ref actual) (ref expected)

test "summarizes one reading" =
    let input = [7.5]
    let actual = summarize ref input
    let expected = [7.5, 7.5, 7.5, 7.5]
    Test.equal (ref actual) (ref expected)

test "accepts negative readings" =
    let input = [-10.0, -5.0, 0.0]
    let actual = summarize ref input
    let expected = [-15.0, -10.0, 0.0, -5.0]
    Test.equal (ref actual) (ref expected)
```

入力は共有借用し、結果の 4 要素だけを所有配列で返します。Array.min / max は要素への参照を返すので、deref で f64 の値を取り出します。空入力は先に拒否しているため、Option.get の失敗は起きません。

sum_kahan は Kahan-Babuska-Neumaier の補償和です。単純な左から右の加算より誤差を抑えられる場合がありますが、任意精度や overflow しない演算ではありません。

## Node.js のホスト

次が `report.mjs` の全内容です。コマンド引数のファイル名は現在の作業ディレクトリ、WASM はスクリプト自身の隣を基準に解決します。

```javascript
import { readFile } from "node:fs/promises";

try {
    if (process.argv.length !== 3) {
        throw new Error("Usage: node report.mjs <readings.json>");
    }

    const readings = JSON.parse(await readFile(process.argv[2], "utf8"));
    if (!Array.isArray(readings) || readings.length === 0 || readings.length > 100000) {
        throw new Error("Expected an array containing 1 to 100000 readings.");
    }
    if (!readings.every(value => typeof value === "number" && Number.isFinite(value))) {
        throw new Error("Every reading must be a finite number.");
    }

    const bytes = await readFile(new URL("./stats.wasm", import.meta.url));
    const { instance } = await WebAssembly.instantiate(bytes);
    const api = instance.exports;
    let input = 0;
    let output = 0;
    let result = 0;
    let summary;

    try {
        input = api.tsuzuri_alloc(BigInt(readings.length) * 8n);
        output = api.tsuzuri_alloc(16n);
        new Float64Array(api.memory.buffer, input, readings.length).set(readings);

        api.tz_summarize(output, input, BigInt(readings.length));

        const view = new DataView(api.memory.buffer);
        result = view.getUint32(output, true);
        if (view.getBigInt64(output + 8, true) !== 4n) {
            throw new Error("Unexpected result length from the calculation module.");
        }
        summary = new Float64Array(api.memory.buffer, result, 4).slice();
    } finally {
        api.tsuzuri_free(result);
        api.tsuzuri_free(output);
        api.tsuzuri_free(input);
    }

    if (!summary.every(Number.isFinite)) {
        throw new Error("The summary is not finite; reduce the magnitude of the readings.");
    }
    console.log(JSON.stringify({
        count: readings.length,
        sum: summary[0],
        minimum: summary[1],
        maximum: summary[2],
        average: summary[3],
    }, null, 2));
} catch (error) {
    console.error(error.message);
    process.exitCode = 1;
}
```

JSON の解析には Node.js 標準の JSON.parse を使います。独自のカンマ分割ではないので、文字列を誤って数値へ変換したり、JSON の構造を読み違えたりしません。

## ビルドと実行

各コードと入力ファイルを上記の構成で用意したプロジェクトに対して、リポジトリルートから実行します。

```sh
./target/release/tsuzuri check target/file-statistics/Stats.tz
./target/release/tsuzuri test target/file-statistics
./target/release/tsuzuri test target/file-statistics --target wasm32 -O3
./target/release/tsuzuri build target/file-statistics/Stats.tz --target wasm32 -o target/file-statistics/stats.wasm
node target/file-statistics/report.mjs target/file-statistics/readings.json
```

別のファイルを集計するときは最後の引数を変えます。入力エラー、読み込み失敗、JSON 構文エラー、計算の失敗は stderr に出し、終了コードを 1 にします。成功結果の JSON は stdout です。

## ABI とメモリの流れ

1. ホストが入力値の個数と型を検査する。
2. WASM の allocator で入力領域と結果 descriptor の領域を確保する。
3. Float64Array で入力値をコピーし、pointer と BigInt の要素数を渡す。
4. Tsuzuri は入力を借用し、所有する結果バッファを返す。
5. ホストが descriptor を読み、結果を JavaScript の独立した配列へコピーする。
6. 結果バッファ、descriptor、入力領域をそれぞれ一度だけ解放する。

返却値が配列なので、tz_summarize の先頭引数は out pointer です。descriptor は 16 byte で、offset 0 が i32 pointer、offset 8 が i64 length です。入力の length はバイト数ではなく f64 の要素数です。

allocator や計算関数が memory を拡張し得るため、view は呼び出し後の memory.buffer から作ります。結果は free する前に slice でコピーします。借用入力は Tsuzuri 側では解放せず、確保したホスト側が解放します。

## 失敗と数値の制限

有限な入力だけでも、合計の中間計算が overflow する可能性があります。ホストは結果の有限性も確認し、JSON.stringify が非有限値を null として出力する前に拒否します。

JSON の数値は最初に JavaScript の Number、すなわち binary64 として読みます。大きな整数の完全な桁保持、金額の十進計算、NaN や signed zero の表現保持を目的とする形式ではありません。金額には[整数円の見積もり](order-quote.md)のように別の契約を選びます。

このホストはコマンドごとに新しい WASM instance を作ります。finally は正常に管理できる確保領域を解放する処理であり、トラップ後の一般的な回復や instance の再利用を保証するものではありません。

読み込みはファイル全体です。100,000 件の検査は JSON 解析後なので、巨大なファイルの読み込み自体を制限する仕組みではありません。公開サービスに転用する場合は、ホストで入力バイト数やリクエストサイズも制限してください。

## 発展させるとき

複数列の入力はホスト側で形式を検査し、必要な数値列を Tsuzuri へ渡します。中央値や分位点を追加する場合はソートの費用と空入力の契約を決めます。並列集計へ変更する場合は加算順序が変わるため、逐次結果との完全一致を暗黙に要求しないでください。

## 関連項目

- [WASM とバッファ ABI](../guides/webassembly.md)
- [Math と順序付き集計](../library-reference/math.md)
- [Array の API](../library-reference/arrays-and-lists.md)
- [ホストの信頼境界](../guides/native-interop.md)
