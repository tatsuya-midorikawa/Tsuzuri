# WASM threads と Worker ホスト

[ドキュメントのトップ](../README.md)

WASM の並列実行は明示的な opt-in です。既定の import-free な逐次バックエンドを変更せず、threads 成果物には共有メモリと Worker 用ホストを要求します。現在の同梱ホストは Node.js 用です。

## 計算を公開する

独立した `target/thread-demo/Kernel.tz`:

```tsuzuri project=threads file=Kernel.tz
export def sum_squares :: i64 -> i64 = \count ->
    let jobs = new [Task<i64>](count, index -> task { return index * index })
    let results = Task.run (Task.parallel jobs)
    Array.sum ref results
```

```sh
./target/release/tsuzuri build target/thread-demo/Kernel.tz --target wasm32 --wasm-feature threads -o target/threaded.wasm
```

threads は WASM / object 出力専用です。LLVM テキスト、header、native、check、run には指定できません。simd128 との併用は可能です。

## Node.js ホスト

`target/thread-demo/run.mjs` として実行する例です。

```javascript
import { readFile } from "node:fs/promises";
import { createThreadPool } from "../../src/runtime/wasm-threads.mjs";

const pool = await createThreadPool(await readFile("target/threaded.wasm"), { workers: 2 });
try {
    console.log(pool.call("tz_sum_squares", 4n).toString());
} finally {
    await pool.close();
}
```

リポジトリルートから `node target/thread-demo/run.mjs` を実行すると `14` を表示します。Node.js 20 以降の worker_threads、SharedArrayBuffer、shared WebAssembly.Memory が必要です。

## ホスト API

| 項目 | 契約 |
| --- | --- |
| `createThreadPool(bytes, options)` | bytes または WebAssembly.Module を受ける非同期の初期化 |
| `workers` | 追加 worker 数 0 から 31。既定は min(CPU 数, 32) - 1 |
| `memory` | shared な WebAssembly.Memory。既定は module が宣言した env.memory の最大 page 数を initial / maximum にしたもの |
| `importsModule`, `importData` | 各 instance 用のホスト import を生成するモジュールとデータ |
| `call(name, ...args)` | 同期実行。完了または失敗まで戻らない |
| `workerCount` | 起動済み worker 数 |
| `instance`, `memory` | ホストが保持する実体 |
| `close()` | 完了後に await し、Worker を終了する |

Worker は初回の並列グループで遅延起動し、以後再利用します。明示した worker 数で初期化できなかった場合、逐次成功へ置き換えません。失敗した pool や close 済み pool は再利用できません。

## import と stack

必要な import は env.memory と tsuzuri_threads の spawn_workers / worker_ready です。ホストが共通の Module と shared Memory を各 Worker へ渡し、各 instance の stack pointer と stack の範囲を設定します。

main の stack は既定 1 MiB、各追加 worker は 256 KiB です。stack、data、heap はすべて線形メモリの上限（既定 16 MiB、最大 4 GiB − 64 KiB）に含まれます。上限は build の `--wasm-max-memory`、main の stack は `--wasm-stack-size` で変更でき、値は env.memory の import の最大 page 数になります。

worker の stack は heap の block なので、各関数は frame を確保した後で stack pointer が自分の stack の範囲にあるかを検査し、溢れは隣の block を書く前にトラップします。
独自のホストは各 worker instance で `tsuzuri_thread_entry` を呼ぶ前に、次の exported global を設定します（`base` は `tsuzuri_thread_stack_alloc() >>> 0`、`top` はそれに `tsuzuri_thread_stack_size()` を足した値）。

```javascript
instance.exports.__stack_pointer.value = top;
instance.exports.tsuzuri_stack_base.value = base;
instance.exports.tsuzuri_stack_top.value = top;
```

両方 0 のままは main の stack を表すため、範囲を設定しない worker は最初の呼び出しでトラップします。`--emit object` を自分でリンクするときは、この 2 つの global も export します。

同梱 Node ホストは、module が宣言した最大 page 数（既定 256 page）を最初に確保します。threads では memory を伸ばしません。確保できない場合は RangeError で、より小さい上限で build し直します。bytes ではなく WebAssembly.Module を渡す場合は宣言を読めないため既定の 256 page になるので、16 MiB 以外の module では同じ大きさの `memory` を明示してください。threads でない module を渡すと TypeError です。

共有 allocator と atomic queue を使い、返却前に全 callback の完了と結果公開を待ちます。通常終了時の所有 heap は回収しますが、worker stack は pool の寿命に従います。

## extern と失敗

extern を使う場合、全 Worker に同じホスト定義が必要です。importsModule は `createImports({ memory, workerId, data })` を export し、instance ごとの import object を返します。

Worker の trap・起動失敗は共有状態を失敗にし、待機を解除します。以後は pool を再利用せず close します。トラップ後の完全な解放や unwind は保証しません。Task.parallel_results の Error という通常の失敗値とは区別します。

## ブラウザーへの移植条件

同梱 Node ホストをそのままブラウザーへ import できません。本番用のブラウザー glue は未実装です。必要な条件は次のとおりです。

1. HTTPS または localhost で COOP: same-origin と COEP: require-corp を配信する。
2. crossOriginIsolated と shared Memory を検査する。
3. 計算の呼び出し元も Worker に置き、UI thread で atomic wait しない。
4. 各 instance に独立した stack を割り当て、`tsuzuri_stack_base`・`tsuzuri_stack_top` を設定する。
5. 初期化、失敗状態、待機解除、正常完了後の終了を実装する。

要件を満たさない明示 threads 要求を、黙って逐次実行へ変更しないでください。

## 関連項目

- [Task と結果付き並列実行](../language-reference/tasks.md)
- [Parallel](../library-reference/parallel.md)
- [同梱 Worker ホスト](../../src/runtime/wasm-threads.mjs)
