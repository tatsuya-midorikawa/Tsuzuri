# Debug

`Debug` は、開発中に値を見るための小さなモジュールです。標準エラーへ、`Display` の文字列と改行を出します。ログの枠組みではなく、標準出力の結果とは混ぜないための出口です。

## この記事のポイント

- `print` は共有借用して表示します。所有権は呼び出し元に残ります。
- `trace` は所有権を受け取り、表示したあと、同じ値を返します。
- ネイティブでは標準エラーです。エントリーポイントの表示や `IO.write_line` は標準出力です。
- WASM の既定は何も書きません。`--debug-output` で `tsuzuri_debug.write` をインポートします。
- `Debug.__print_string` はユーザーコードから呼べません。

## 関数

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Debug.print` | `Display<'a> => ref 'a -> unit` | 借りた値を表示する。値は消費しない |
| `Debug.trace` | `Display<'a> => 'a -> 'a` | 表示してから、受け取った所有値をそのまま返す |

どちらも `Display.display` で文字列にします。デバッグ専用の別フォーマットはありません。`Display` を実装していない型は渡せません。

表示そのものに失敗したとき、孤立サロゲートを UTF-8 にできないとき、書き込みが進まないときはトラップします。理由は `debug output failed` または `invalid Unicode encoding` です。`Display` の評価中に起きたトラップは、表示を省略しても消えません。

`Task` の中から複数のタスクが同時に出した場合、行の順序と、1 行が割り込まれないことは保証されません。

## 標準エラーと標準出力

`Debug.trace` は標準エラーへ出し、返した値は引き続き使えます。次の例の標準出力は、`IO.write_line` の 1 行だけです。

```tsuzuri run=150
def main :: unit -> i32 = \() ->
    let total = 120 + 30
    let traced = Debug.trace total
    do! IO.write_line traced
    0
```

実行結果:

```text
150
```

同じ実行の標準エラーにも、次の 1 行が出ます。`tsuzuri run` は子プロセスの標準エラーを、そのまま端末へ流します。

```text
150
```

`Debug.print` は参照を取るので、あとで同じ変数を読めます。出力先は同じく標準エラーです。

```tsuzuri run=stdout
def main :: unit -> i32 = \() ->
    let label = "tea"
    Debug.print (ref label)
    do! IO.write_line "stdout"
    0
```

実行結果:

```text
stdout
```

標準エラー:

```text
tea
```

エントリーポイントの最後の式を表示する経路も標準出力です。標準出力と標準エラーが分かれているため、標準出力をパイプで別コマンドに渡しても、デバッグ行が混ざることなく標準エラーに残ります。

## WASM のインポート

WASM では、既定の `Debug.print` と `Debug.trace` は書き込みをしません。引数と `Display` の評価、一時文字列の解放は実行されます。`Debug` を呼ぶプログラムを `--debug-output` なしで wasm32 へビルドすると、このためのインポートは付きません。

```sh
tsuzuri build Kernel.tz --target wasm32 --debug-output -o debug.wasm
```

到達する `Debug` の呼び出しがあるとき、このオプションは `tsuzuri_debug.write(pointer, length)` をインポートします。`memory` もエクスポートされます。`length` は wasm64 では 64 bit で、wasm32 の呼び出しでも JavaScript 側では `bigint` になることがあります。

ホストは、呼び出しが戻る前に、指定範囲の UTF-8 バイトをコピーします。返ってきたあとにポインタを保持してはいけません。ネイティブと違い、渡されるバイト列には改行が含まれません。1 行のログにするなら、ホストが改行を足します。`Debug.trace 42` のペイロードは `"42"` の 2 バイトです。

```javascript
let instance;
const imports = {
    tsuzuri_debug: {
        write(pointer, length) {
            const bytes = new Uint8Array(
                instance.exports.memory.buffer,
                pointer,
                Number(length)
            ).slice();
            console.error(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
        },
    },
};
instance = new WebAssembly.Instance(module, imports);
```

`Debug` を使わないプログラムでは、`--debug-output` の有無でバイナリは変わりません。ネイティブのビルドでは、このオプションを指定しても出力動作は変わらず、常に標準エラーへ出力されます。

`check`、`fmt`、`test`、ヘッダー出力に `--debug-output` を付けると `E2000` です。`build` と `run` だけで指定できます。

## 呼べない内部関数

`Debug.__print_string` は、標準ライブラリの `print` と `trace` が使う内部関数です。ユーザーコードから呼ぶと `E1022` です。

```text
Debug.__print_string "no"
```

```text
error[E1022]: Debug.__print_string is private to the standard Debug module; use Debug.print or Debug.trace
```

## 注意点

- 表示は副作用ですが、型の上では純粋な関数と区別されません。本番の記録には `IO.write_error_line` を使います。
- 値を消費したくないときは `print (ref value)`、パイプラインの途中で見たいときは `trace` です。
- 孤立サロゲートを含む `string` は、出力時の UTF-8 変換でトラップします。

## まとめ

- `print` は借用、`trace` は受け取った値を表示して返します。
- ネイティブの出力先は標準エラーで、改行が付きます。
- WASM は既定では無出力です。見るときは `--debug-output` と `tsuzuri_debug.write` が必要です。
- 内部の `__print_string` は呼ばないでください。

## 関連項目

- [例外処理](../exception-handling/exception-handling.md)
- [assert 式](../exception-handling/assert.md)
- [IO](io.md)
- [コンパイラ オプション](../compiler/option.md)
- [WebAssembly への出力](../compiler/webassembly.md)
- [言語仕様: デバッグ出力](../../../docs/language.md#デバッグ出力)
- [言語リファレンスの目次](../index.md)
