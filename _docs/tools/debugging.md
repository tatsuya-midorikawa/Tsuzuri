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

成果物の隣に `<output>.trap.json` を出します。assert、整数ゼロ除算、符号付き除算 overflow、添字、確保、範囲のゼロ step、パターン不一致、Unicode 変換、WASM の stack 溢れなどの理由を区別します。
stack 溢れは wasm32 の 2 GiB 超と threads の入口検査が検出し、溢れた関数の定義位置（std の内部ならそれを呼んだ位置）を報告します。

native は stderr の reporter、WASM は import 不要の `tsuzuri_trap_site()` を使います。WASM でトラップを受けたホストは、その ID と side table を照合します。ID は最後に記録した失敗であり、通常の呼び出し開始時にリセットされません。

run の異常終了は E2005 です。run --json では子プロセスの stderr を診断 message に含め、生の trap 行を別に出しません。正常時の stdout / stderr は通常どおり転送します。

トラップの位置が出ないまま SIGSEGV / SIGBUS で終わった場合、run は「スタック枯渇の可能性が高い」ことを E2005 で報告します。再帰が深すぎるときの典型的な終わり方で、再帰を浅くするかループにします。`tsuzuri test` も同じ signal の失敗理由に、スタック枯渇の可能性を書き添えます。これは子プロセスの終了状態からの推定で、メモリ破壊の確認ではありません。

再帰する（呼び出しが自分自身に戻る）プログラムの native 実行ファイルは、スタックが尽きると stderr に `trap: stack overflow` を書いてから `abort()` で終わります（macOS と Linux。main thread と `Task.parallel` の worker の両方）。run はこの行を見て、推定ではなく「stack overflow: the stack was exhausted by deep recursion」と E2005 で報告します。再帰しないプログラムは尽きないので、報告のための runtime を実行ファイルに足しません（ビルドを遅くしないためです）。末尾再帰は LLVM がループにするので数えません。スタックの外の SIGSEGV（`--link` したホスト関数の NULL 参照など）は、`stack overflow` と書かず従来どおりの動作で終わります。object と `--emit llvm` の出力、`tsuzuri test` の実行ファイルは signal の設定を変えません。

### native のホストへトラップを返す

ホストが native object を呼ぶとき、トラップはプロセスごと終了します。`--trap-mode return` を付けて build すると、各 export `tz_<name>` に対応する `tsuzuri_try_<name>` が増え、トラップは戻り値で返ります。

```sh
./target/release/tsuzuri build lib --emit header --trap-mode return -o target/lib.h
./target/release/tsuzuri build lib --emit object --trap-mode return -o target/lib.o
```

```c
#include "lib.h"

tsuzuri_trap_info trap;
int64_t result;
int32_t status = tsuzuri_try_div(&trap, &result, 7, 0);
if (status == 1) {
    /* trap.site は target/lib.o.trap.json の id、trap.kind は種類（1 は整数のゼロ除算） */
}
```

`tsuzuri_try_<name>(tsuzuri_trap_info *trap, <結果の置き場>, <tz_<name> と同じ引数>)` は 0（成功）、1（トラップ。`*trap` に記録）、2（同じ thread で別の `tsuzuri_try_*` の実行中。何も実行しない）を返します。結果の置き場は、値を返す export なら `T *result`、buffer や record を返す export なら `tz_<name>` の `out` です。トラップした呼び出しが確保した heap はすべて解放され（結果の buffer はホストの所有物のままです）、同じ thread で続けて呼べます。`Task.parallel` の worker のトラップも、グループを投入した呼び出しの status 1 になります（最も小さい index のトラップを返し、先に `Err` を返した item があっても、トラップした item が動いていればトラップを返します）。

`--trap-mode return` は native の `--emit object`・`--emit llvm`・`--emit header` だけで使え、`--trap-info` を含みます（header を除く）。object は `setjmp` を使う runtime（`src/runtime/trap.c`）を中に持つので Windows の COFF object は E2002 です。`--emit llvm` を使う場合は `trap.c` と `task.c`（`-DTZ_TRAP_BOUNDARY`）をホストが一緒にリンクします。extern のコールバック（E12）を使うプログラムは、トラップがホストのフレームを跨ぐので E2000 です。ホストが保持するリソース（`extern type` のハンドルなど）は追跡しないので、トラップした呼び出しが確保したハンドルはホストが解放してください。

WASM のホストは、同梱の `createBoundary` でトラップを値として受け取れます。`sites` に side table の `sites` を渡すと、トラップの種類と位置まで返します。

```sh
./target/release/tsuzuri build target/wasm-demo/Div.tz --target wasm32 --trap-info -o target/div.wasm
```

```javascript
import { readFileSync } from "node:fs";
import { createBoundary } from "./src/runtime/trap-boundary.mjs";

const module = new WebAssembly.Module(readFileSync("target/div.wasm"));
const { sites } = JSON.parse(readFileSync("target/div.wasm.trap.json", "utf8"));
const boundary = createBoundary(module, { sites });

console.log(boundary.call("tz_div", 7n, 2n));
// { ok: true, value: 3n }
console.log(boundary.call("tz_div", 7n, 0n));
// { ok: false, trap: { reason: "trap", site, kind: "integer division by zero", path, line, column } }
```

`reason` は `"trap"`（`WebAssembly.RuntimeError`）と `"stack"`（V8 の `RangeError`）です。`site` が 0 の `"trap"` は、`--trap-info` なしの build か、Tsuzuri の位置を持たないエンジンのトラップです。トラップは巻き戻さないため、失敗した instance の heap と shadow stack は途中の状態です。境界はその instance を捨て、次の `call` で同じ module から作り直します。ホストの import 関数が投げた例外は、同じ object のまま再送出します。

threads の module は `createBoundary` に渡せません。`createThreadPool` の pool が単位で、worker の失敗後は pool を再利用せず閉じてください。

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
