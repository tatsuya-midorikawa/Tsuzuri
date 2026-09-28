# デバッグ出力、トラップ位置、DWARF

[ドキュメントのトップ](../README.md)

値を見る Debug、失敗箇所を報告する trap-info、デバッガー用の DWARF は別の機能です。必要に応じて組み合わせます。

## 値を表示する

独立した `target/debug-demo/Main.tz` の例です。

```tsuzuri run=42
def main :: i64 =
    let answer = 40 + 2
    Debug.print ref answer
    answer
```

native では Debug の行を stderr、入口の結果を stdout に出します。どちらにも 42 が見えますが、同じ出力経路ではありません。

Debug.print は借用、Debug.trace は消費して表示後に同じ所有値を返します。表示には Display を使い、孤立サロゲートの UTF-8 出力や書き込み失敗はトラップです。並列 Task の行順・一行全体の不可分性は保証しません。

## WASM の Debug ホスト

WASM の既定は no-op ですが、引数・Display の評価と表示文字列の解放は行います。Display 内のトラップは消えません。

独立した `target/debug-wasm/Kernel.tz`:

```tsuzuri project=debug-wasm file=Kernel.tz
export def echo :: i64 -> i64 = \value -> Debug.trace value
```

```sh
./target/release/tsuzuri build target/debug-wasm/Kernel.tz --target wasm32 --debug-output -o target/debug.wasm
```

ホストでは到達可能な Debug 呼び出しに対して `tsuzuri_debug.write(pointer, length)` を提供します。

```javascript
import { readFile } from "node:fs/promises";

let instance;
const imports = {
    tsuzuri_debug: {
        write(pointer, length) {
            const bytes = new Uint8Array(instance.exports.memory.buffer, pointer, Number(length)).slice();
            console.error(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
        },
    },
};
instance = (await WebAssembly.instantiate(await readFile("target/debug.wasm"), imports)).instance;
console.log(instance.exports.tz_echo(42n).toString());
```

length は BigInt です。ホストは呼び出し中に UTF-8 bytes をコピーし、改行を一行分付けます。返却後に pointer を保持してはいけません。未使用の Debug のために import は増えません。

native の debug-output オプションは出力動作を変えません。check、fmt、test、header には指定できません。

## トラップの理由と位置

run は既定でトラップ位置を有効にします。build は既定で含めず、明示的に指定します。

```sh
./target/release/tsuzuri build examples/hello --trap-info -o target/hello-with-traps
```

成果物の隣に `<output>.trap.json` を出します。assert、整数ゼロ除算、符号付き除算 overflow、添字、確保、範囲のゼロ step、パターン不一致、Unicode 変換などの理由を区別します。

native は stderr の reporter、WASM は import 不要の `tsuzuri_trap_site()` を使います。WASM でトラップを受けたホストは、その ID と side table を照合します。ID は最後に記録した失敗であり、通常の呼び出し開始時にリセットされません。

run の異常終了は E2005 です。run --json では子プロセスの stderr を診断 message に含め、生の trap 行を別に出しません。正常時の stdout / stderr は通常どおり転送します。

trap-info は例外処理、巻き戻し、完全な stack trace を追加しません。ソース位置と理由を付ける機能です。後からリンクする補助関数の内部 trap は対象外の場合があります。

## ソースレベルのデバッグ

```sh
./target/release/tsuzuri build target/debug-demo/Main.tz -O0 -g -o target/debug-demo-app
lldb target/debug-demo-app
```

-g / --debug-info は関数、行、ローカル変数、型の DWARF を生成します。WASM は debug custom section に保持し、実際の表示はデバッガーの対応に依存します。

macOS の実行ファイルには隣接する `.dwarf` が必要です。LLDB での例:

```text
target symbols add target/debug-demo-app.dwarf
breakpoint set --file Main.tz --line 3
run
```

この行番号は上の Main の例に対応します。O3 では変数が最適化で消えたり、元のソースと実行位置が一対一にならなかったりします。変数を調べる初回のデバッグには O0 を選びます。

## 追加ツールと成果物

macOS の debug executable は dsymutil、Task 等の runtime を含む debug object は Clang と対応する llvm-link が必要です。TSUZURI_DSYMUTIL / TSUZURI_LLVM_LINK で実行ファイルを指定できます。

実行ファイルだけを移動して `.dwarf` や `.trap.json` を落とすと情報を参照できなくなります。build の出力保護は失敗時に既存成果物を維持しますが、複数ファイルを跨ぐ OS レベルのトランザクションを保証するものではありません。

## 関連項目

- [Debug API](../library-reference/builtins.md)
- [CLI オプション](command-line.md)
- [WASM のメモリ契約](../guides/webassembly.md)
