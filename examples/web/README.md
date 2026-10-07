# Web ホスト

既存のPhysics/simulation例は通常のWASMで、SharedArrayBufferを必要としません。

## 生成グルーで呼ぶ

`simulation.mjs` は、確保・複製・記述子の読み取り・解放を手で書いた例です。同じ呼び出しは、`--emit bindings-js` が生成する型付きのグルーで書けます。

```sh
tsuzuri build examples/web/Physics.tz --target wasm32 -o examples/web/physics.wasm
tsuzuri build examples/web/Physics.tz --target wasm32 --emit bindings-js -o examples/web/physics.mjs
```

`physics.mjs` の隣に TypeScript 宣言 `physics.d.mts` もできます。グルーは `node:` の import も `fetch` も使わないので、ブラウザでもそのまま `import` できます。

```javascript
import { load, TsuzuriTrap } from "./physics.mjs";

const api = await load(await (await fetch(new URL("./physics.wasm", import.meta.url))).arrayBuffer());
try {
  console.log(api.exports.next_positions(Float64Array.of(9, 1), Float64Array.of(3, -3), 1, 10)); // Float64Array [8, 2]
} catch (error) {
  if (!(error instanceof TsuzuriTrap)) throw error;
  console.log(error.trap.reason);
}
```

`next_positions` の入力は呼び出しごとに線形メモリへ複製され、結果は `Float64Array` に複製されてすぐ解放されます。長さの違う配列を渡して `assert` が失敗すると `TsuzuriTrap` になり、次の呼び出しは新しいインスタンスで動きます。

## WASM threads

`--wasm-feature threads`の実行ホストは現在Node.js用です。リポジトリの`src/runtime/wasm-threads.mjs`と`tests/wasm_threads.mjs`を参照してください。

Browserへ移植する場合の手順:

1. HTTPSまたはlocalhostで`Cross-Origin-Opener-Policy: same-origin`と`Cross-Origin-Embedder-Policy: require-corp`を配信し、`crossOriginIsolated`を検査する。
2. moduleがimportするenv.memoryの最大page数（既定256page、`--wasm-max-memory`で変更）のshared WebAssembly.Memoryと同じWebAssembly.Moduleを全Workerへ渡す。
3. 計算の呼び出し元もWorkerに置く。UI thread上ではatomic waitできない。
4. 各instanceの`__stack_pointer`を独立した256KiB領域の終端に、`tsuzuri_stack_base`・`tsuzuri_stack_top`をその領域の範囲に設定してから`tsuzuri_thread_entry(worker_id)`を呼ぶ。範囲を設定しないworkerは最初の呼び出しでトラップする。
5. Nodeホストと同じ初期化、spawn_workers/worker_ready import、失敗時の共有poisonと全wait通知、正常join後のWorker終了を実装する。

Browser本番glueは未実装です。COOP/COEPが使えない場合にthreads要求を黙って逐次実行へ置き換えないでください。
