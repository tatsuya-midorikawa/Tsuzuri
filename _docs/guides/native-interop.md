# C ホストとの連携と外部関数

[ドキュメントのトップ](../README.md)

Tsuzuri の計算を既存の C / C++ アプリケーションから呼び出せます。OS、GUI、ファイル、ネットワークなどはホストに置き、公開 ABI を通じて必要な値を渡します。

## 計算関数を公開する

独立したディレクトリの `Kernel.tz`:

```tsuzuri project=native file=Kernel.tz
export def sum :: ref [i64] -> i64 = \values -> Array.sum values
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

native ではトラップがプロセスを終了させます。例外や巻き戻しはなく、呼び出しだけを隔離する境界もありません。隔離が必要な呼び出しは、`tsuzuri run` と同じように別プロセスで実行してください。スタック枯渇を理由付きで報告するのは、`tsuzuri run` と `tsuzuri test` が子プロセスの終了 signal から推定する場合だけです。WASM では、[トラップを値として受け取る境界](webassembly.md#nodejs-から呼ぶ)を使えます。

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
| `extern type` のハンドル | 不透明 pointer（`typedef struct tz_handle_… *`）。`ref` の引数もハンドルそのもの |
| extern の関数型の引数（コールバック） | C の関数 pointer。引数と結果はスカラー・unit・ハンドルだけ |

狭い整数は下位 bit へ切り詰め、結果を符号・ゼロ拡張して 32-bit にします。レコードの bool と狭い整数も 32-bit へ正規化します。C の by-value struct 分類には依存しません。

レコードの入れ子と具体化済みジェネリックレコードも、全フィールドが ABI 条件を満たせば使えます。typedef とフィールドの名前は生成ヘッダーを使い、内部のレイアウトを推測しないでください。

## 非対応の境界型

i128、f16 / f128、decimal、char / utf8char、union、タプル、List、Vec、Map / Set、Seq、関数値、Task、SIMD は直接 export できません。extern の引数に渡せる関数は、後述の静的コールバックだけです。

所有配列・文字列の入力、排他参照、借用結果、対応外の配列要素型も拒否します。export の unit 引数は未対応です。extern の unit 引数は後述のように ABI から省略します。

## バッファの責任

借用入力は同期呼び出し中だけ読み取り、コピー・解放・保持しません。ホストは自然 alignment と length 要素分の有効領域を保証し、呼び出し中に変更・解放してはいけません。

負の長さ、サイズ overflow、不正な null、alignment は検査します。ただし native の任意 pointer の実在性は一般には検証できません。null と長さ 0 は許可されます。UTF-8 は妥当性検査、UTF-16 は孤立サロゲートを保持します。

所有結果はホストへ移り、ホストが `tsuzuri_free(out.ptr)` で一度だけ解放します。借用入力を free してはいけません。対応するモジュールは `tsuzuri_alloc(int64_t size)` / `tsuzuri_free(void *ptr)` を公開します。alloc の 0 は最低 1 byte、free の null は何もしません。

## ホスト関数をインポートする

別の独立した例の `Clock.tz`:

```tsuzuri project=clock file=Clock.tz
extern def now :: unit -> i64

export def answer :: i64 = now () + 2
```

native ホストは `tsuzuri_host_Clock_now` を実装します。

```c
#include <stdint.h>

int64_t tsuzuri_host_Clock_now(void) {
    return 40;
}
```

生成ヘッダーにホスト関数の prototype も出力します。module 内のドットは underscore に変換し、衝突は拒否します。unit 引数はホスト ABI から省き、unit 結果は void です。

extern は同期的で副作用を持つ呼び出しです。引数は左から右に評価し、未使用の extern は最終的な import を増やしません。private extern def も使用できます。多相性、制約、named region、Task、排他参照などは境界に持ち込めません。関数を渡せるのは、後述の静的コールバックだけです。

未解決ホスト関数を含む exe / run はリンクエラーです。ホストの object やライブラリは `--link`・`-l`・`-L` で実行ファイルへ直接リンクできます（[ホストの object とライブラリをリンクする](#ホストの-object-とライブラリをリンクする)）。object / header を生成してホスト側でリンクすることもできます。

## 既存の C 関数を直接呼ぶ

`extern` の後に文字列を書くと、`tsuzuri_host_` で始まる名前の shim を書かずに、C の symbol をそのまま呼べます。文字列を二つ書くと、WASM の import module と symbol を別々に指定します。

```tsuzuri project=link_names
extern "sqrt" def c_sqrt :: f64 -> f64
extern "env" "host_now" def host_now :: unit -> i64

export def root :: f64 -> i64
fn root value = if c_sqrt value == 1.5 then host_now () else 0
```

native では `sqrt` と `host_now` という名前を呼びます。WASM では `c_sqrt` が module `tsuzuri`・name `sqrt` の import に、`host_now` が module `env`・name `host_now` の import になります。文字列が一つのときの WASM module は `tsuzuri` です。

symbol は 255 byte 以下の C 識別子です。`tz_`・`tsuzuri`・`__` で始まる名前と、`malloc`・`free`・`write` など生成物が自分で使う名前は使えません。同じ symbol を複数の extern で宣言してもよいですが、型と WASM module は一致させます。

リンク名を指定した extern の宣言は、ホストを実装する側（system header を含む）が持つので、生成ヘッダーには prototype を出しません。

## ホストの資源をハンドルで受け渡す

`extern type` は、ホストが所有する資源を指す不透明な型です。値は extern の結果か export の引数からだけ得られ、Copy ではありません。値で渡すと move され、`ref` は借用です。

```tsuzuri project=handles
extern type Counter
extern "counter_new" def counter_new :: i64 -> Counter
extern "counter_add" def counter_add :: ref Counter -> i64 -> i64
extern "counter_free" def counter_free :: Counter -> i64

export def total :: i64 -> i64
fn total start =
    let counter = counter_new start
    let first = counter_add (&counter) 5
    first + counter_free counter
```

ホスト側の実装の例です。

```c
#include <stdint.h>
#include <stdlib.h>

void *counter_new(int64_t start) {
    int64_t *value = malloc(sizeof *value);
    *value = start;
    return value;
}

int64_t counter_add(void *counter, int64_t amount) {
    return *(int64_t *)counter += amount;
}

int64_t counter_free(void *counter) {
    int64_t last = *(int64_t *)counter;
    free(counter);
    return last;
}
```

ハンドルは scope を抜けても何も呼ばれません。解放は `counter_free` のようなホスト関数へ値で渡して行い、move 済みの値は使えません。閉じ忘れはホスト資源の leak です。関数値には捕捉できず、`==` や表示もできません。

native ではポインター 1 つ幅、wasm32 では i32（JavaScript では number）で渡ります。生成ヘッダーは `typedef struct tz_handle_… *` をハンドルの型として一度だけ出力し、`export def` の引数・結果にも使えます。ABI で使える位置は extern・export・コールバックの引数と結果で、ABI のレコードのフィールドや配列の要素にはできません。

## ホストへ関数を渡す

extern の引数に関数型を書くと、ホストへ C の関数 pointer（コールバック）を渡せます。

```tsuzuri project=callbacks
extern "apply_twice" def apply_twice :: (i64 -> i64) -> i64 -> i64

private def triple :: i64 -> i64
fn triple value = value * 3

export def run :: i64 -> i64
fn run value = apply_twice triple value
```

ホスト側の実装の例です。

```c
#include <stdint.h>

int64_t apply_twice(int64_t (*callback)(int64_t), int64_t value) {
    return callback(callback(value));
}
```

渡せるのは、捕捉のないトップレベルの関数の名前だけです。ラムダ、局所変数、部分適用、型引数のある関数、標準ライブラリや組み込みの関数、extern は渡せません。コールバックを受け取る extern は、直接、全引数を付けて呼びます。値として持ち回したり部分適用したりはできません。

コールバックの引数はスカラー、ハンドル、ハンドルの `ref` で、結果はスカラーか unit です（unit は唯一の引数のときだけで、C では引数を省略します）。バッファ、レコード、入れ子の関数型は使えません。

ホストが受け取る pointer は寿命の契約を持たない静的な関数です。コールバックの中から extern を呼ぶ再入もできます。コールバックの中のトラップは通常のトラップと同じで、native ではプロセスが異常終了します。WASM では関数 table の index（number）として渡り、table が `__indirect_function_table` として export されます。JavaScript のホストは `instance.exports.__indirect_function_table.get(index)` で関数を取り、i64 には BigInt を渡します。トラップした instance は使い続けないでください。

```js
const callback = (index) => instance.exports.__indirect_function_table.get(index);
const imports = {
  tsuzuri: { apply_twice: (index, value) => callback(index)(callback(index)(value)) },
};
```

この検証は同じスレッドからの同期呼び出しと再入だけで行っています。ホストの別スレッドから呼ぶときは、ホスト側で同期を保ってください。

## ホストの object とライブラリをリンクする

`build`・`run`・`test` は、ホストの object や library を実行ファイルへ直接リンクできます。

```sh
clang -c host.c -o target/host.o
./target/release/tsuzuri run app --link target/host.o
./target/release/tsuzuri build app -L vendor -l sqlite3 --link target/host.o -o target/app
```

| オプション | 内容 |
| --- | --- |
| `--link PATH` | object または static library。path は存在する必要がある |
| `-l NAME` | system library。`libm` や `m.a` ではなく、`-l sqlite3` のように接頭辞と拡張子を付けない |
| `-L DIR` | `-l` の探索先 |

値は次の引数で渡し、各オプションは繰り返せます（合計 256 個）。入力は既存のリンク引数の後に、`-L`、`--link`、`-l` の順で並べます。static library は、それを参照する object より後に指定してください。

プロジェクトの `Tsuzuri.toml` の `[native]` にも同じ入力を書けます。path は package のルートからの相対です。CLI より前に連結します。

```toml
[package]
name = "app"
version = "0.1.0"

[native]
link = ["host.o"]
libraries = ["m"]
search = ["vendor"]
```

リンク入力は native の実行ファイルを作るときだけ有効です。wasm32・wasm64、exe 以外の `--emit`、`check`・`fmt` で指定すると `E2000` で、manifest の `[native]` はそれらでは無視します。依存 package の `[native]` は `E2000` です。読めない path は `E2001`、出力先が入力と同じ場合は `E2003` で、成果物 cache は入力がある間は使いません。

## ホストが所有結果を返す場合

ホストは Tsuzuri allocator で確保した独立領域を、全フィールド初期化済みの out descriptor として返し、所有権を渡します。static 領域や借用入力を所有結果として返してはいけません。

長さ、サイズ、alignment、UTF-8 などは受領時に検査しますが、native の確保元はホストの責任です。例外 unwind を Tsuzuri のフレーム越しに行わず、非同期に入力 pointer を保持しません。

並列タスクから呼ぶホスト関数は thread-safe にするか、そのような呼び出しを避けます。トラップや基盤の失敗は通常の Result へ自動変換されません。

## 関連項目

- [WASM のホスト連携](webassembly.md)
- [既存の C ホスト例](../../examples/native/main.c)
- [公開 ABI の正式な仕様](../../docs/language.md#公開-abi)
