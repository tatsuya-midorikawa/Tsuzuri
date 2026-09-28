# C ホストとの連携と外部関数

[ドキュメントのトップ](../README.md)

Tsuzuri の計算を既存の C / C++ アプリケーションから呼び出せます。OS、GUI、ファイル、ネットワークなどはホストに置き、公開 ABI を通じて必要な値を渡します。

## 計算関数を公開する

独立したディレクトリの `Kernel.tz`:

```tsuzuri project=native file=Kernel.tz
export def sum :: ref [i64] -> i64
fn sum values = Array.sum values
```

次の例ではそのディレクトリを `target/host-demo` とします。

```sh
./target/release/tsuzuri build target/host-demo/Kernel.tz --emit object -o target/kernel.o
./target/release/tsuzuri build target/host-demo/Kernel.tz --emit header -o target/kernel.h
```

同じ作業ディレクトリの `main.c`:

```c
#include "kernel.h"
#include <inttypes.h>
#include <stdio.h>

int main(void) {
    const int64_t values[] = {20, 22};
    printf("%" PRId64 "\n", tz_sum(values, 2));
    return 0;
}
```

POSIX ホストでのリンク例です。

```sh
clang -std=c11 target/host-demo/main.c target/kernel.o -I target -pthread -lm -o target/host-demo-app
./target/host-demo-app
```

出力は `42` です。pthread を使うランタイムを含む object には `-pthread` を付けます。Windows MSVC 向けは別のツールチェーンと制限があり、この POSIX コマンドをそのまま使いません。

## 公開名と呼び出し

export def の公開名は `tz_name` です。モジュール名は付かないため、プロジェクト全体で export 名を一意にします。言語内でカリー化されていても、ホスト側は全引数を一度に渡します。

ジェネリック関数は直接 export せず、具体型のラッパーを作ります。private と export は併用できません。内部 LLVM 型や内部シンボルは安定 ABI ではありません。

## 対応型

| Tsuzuri の型 | ホスト側 |
| --- | --- |
| i8 / i16 / i32 | int32_t |
| i8u / i16u / i32u | uint32_t |
| i64 / i64u | int64_t / uint64_t |
| f32 / f64 | float / double |
| bool | int32_t。入力 0 は false、非 0 は true、出力は 0 / 1 |
| unit の結果 | void |
| ref [i64] / ref [f64] / ref [ubyte] | const 要素ポインターと int64_t の長さ |
| ref string / ref utf8string | const uint16_t / uint8_t ポインターと長さ |
| 対応配列・文字列の所有結果 | 先頭の out pointer へバッファ記述子を書き、void を返す |
| ABI スカラーだけのレコード | 正規化した struct の入力 pointer / 先頭 out pointer |

狭い整数は下位 bit へ切り詰め、結果を符号・ゼロ拡張して 32-bit にします。レコードの bool と狭い整数も 32-bit へ正規化します。C の by-value struct 分類には依存しません。

レコードの入れ子と具体化済みジェネリックレコードも、全フィールドが ABI 条件を満たせば使えます。typedef とフィールドの名前は生成ヘッダーを使い、内部のレイアウトを推測しないでください。

## 非対応の境界型

i128、f16 / f128、decimal、char / utf8char、union、タプル、List、Vec、Map / Set、Seq、関数値、Task、SIMD は直接 export できません。

所有配列・文字列の入力、排他参照、借用結果、対応外の配列要素型も拒否します。export の unit 引数は未対応です。extern の unit 引数は後述のように ABI から省略します。

## バッファの責任

借用入力は同期呼び出し中だけ読み取り、コピー・解放・保持しません。ホストは自然 alignment と length 要素分の有効領域を保証し、呼び出し中に変更・解放してはいけません。

負の長さ、サイズ overflow、不正な null、alignment は検査します。ただし native の任意 pointer の実在性は一般には検証できません。null と長さ 0 は許可されます。UTF-8 は妥当性検査、UTF-16 は孤立サロゲートを保持します。

所有結果はホストへ移り、ホストが `tsuzuri_free(out.ptr)` で一度だけ解放します。借用入力を free してはいけません。対応するモジュールは `tsuzuri_alloc(int64_t size)` / `tsuzuri_free(void *ptr)` を公開します。alloc の 0 は最低 1 byte、free の null は何もしません。

## ホスト関数をインポートする

別の独立した例の `Clock.tz`:

```tsuzuri project=clock file=Clock.tz
extern def now :: unit -> i64

export def answer :: i64
fn answer = now () + 2
```

native ホストは `tsuzuri_host_Clock_now` を実装します。

```c
#include <stdint.h>

int64_t tsuzuri_host_Clock_now(void) {
    return 40;
}
```

生成ヘッダーにホスト関数の prototype も出力します。module 内のドットは underscore に変換し、衝突は拒否します。unit 引数はホスト ABI から省き、unit 結果は void です。

extern は同期的で副作用を持つ呼び出しです。引数は左から右に評価し、未使用の extern は最終的な import を増やしません。private extern def も使用できます。多相性、制約、named region、callback、Task、排他参照などは境界に持ち込めません。

未解決ホスト関数を含む exe / run はリンクエラーです。ホスト object を追加指定する CLI はないので、object / header を生成してホスト側でリンクします。

## ホストが所有結果を返す場合

ホストは Tsuzuri allocator で確保した独立領域を、全フィールド初期化済みの out descriptor として返し、所有権を渡します。static 領域や借用入力を所有結果として返してはいけません。

長さ、サイズ、alignment、UTF-8 などは受領時に検査しますが、native の確保元はホストの責任です。例外 unwind を Tsuzuri のフレーム越しに行わず、非同期に入力 pointer を保持しません。

並列タスクから呼ぶホスト関数は thread-safe にするか、そのような呼び出しを避けます。トラップや基盤の失敗は通常の Result へ自動変換されません。

## 関連項目

- [WASM のホスト連携](webassembly.md)
- [既存の C ホスト例](../../examples/native/main.c)
- [公開 ABI の正式な仕様](../../docs/language.md#公開-abi)
