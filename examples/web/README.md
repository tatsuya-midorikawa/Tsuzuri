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

`next_positions` の入力は呼び出しごとに線形メモリへ複製され、結果は `Float64Array` に複製されてすぐ解放されます。長さの違う配列を渡して `assert` が失敗すると `TsuzuriTrap` になり、次の呼び出しは新しいインスタンスで動きます。Chrome のメインスレッドは 8 MB を超えるモジュールを同期で作り直せないので、そのときは呼び出しが `Error` になります。`await api.ready()` で非同期に作り直してから呼び直してください。

## WASM threads

ブラウザでは、`--emit bindings-js --wasm-feature threads` が生成するグルーがWeb Workerのスレッドプールを作ります。`.wasm`も同じソースから`--wasm-feature threads`でビルドします。次の`app`は、[スレッドのグルー](../../_tsuzuri/language-reference/compiler/webassembly.md#スレッドのグルー)の例と同じ`with_tax`と`host.mjs`を持つプロジェクトです。

```sh
tsuzuri build app --target wasm32 --wasm-feature threads --emit bindings-js -o app.mjs
tsuzuri build app --target wasm32 --wasm-feature threads -o app.wasm
```

```javascript
import { load } from "./app.mjs";

const api = await load(await (await fetch("./app.wasm")).arrayBuffer(), { workers: 2, importsModule: new URL("./host.mjs", import.meta.url) });
console.log(await api.exports.with_tax(BigInt64Array.of(100n, 200n))); // BigInt64Array [110n, 220n]
await api.close();
```

ページと`app.mjs`、`app.wasm`、`host.mjs`は、HTTPSかlocalhostから`Cross-Origin-Opener-Policy: same-origin`と`Cross-Origin-Embedder-Policy: require-corp`付きで配信します。満たさないページでは、`load`がワーカーを起動せずに`Error`を投げます。exportはWorkerで動いて`Promise`を返し、ホスト関数は各Workerで`host.mjs`の`createImports({ workerId, data })`から作ります。グルーは自分自身をmodule workerとして起動するので、`app.mjs`はバンドルせずに配信します。詳しくは[WebAssembly への出力](../../_tsuzuri/language-reference/compiler/webassembly.md#スレッドのグルー)を見てください。

Node.jsの実行ホストは`src/runtime/wasm-threads.mjs`で、`tests/wasm_threads.mjs`が使い方の例です。

ホストを自分で書く場合の手順:

1. HTTPSまたはlocalhostで`Cross-Origin-Opener-Policy: same-origin`と`Cross-Origin-Embedder-Policy: require-corp`を配信し、`crossOriginIsolated`を検査する。
2. moduleがimportするenv.memoryの最大page数（既定256page、`--wasm-max-memory`で変更）のshared WebAssembly.Memoryと同じWebAssembly.Moduleを全Workerへ渡す。
3. 計算の呼び出し元もWorkerに置く。UI thread上ではatomic waitできない。
4. 各instanceの`__stack_pointer`を独立した256KiB領域の終端に、`tsuzuri_stack_base`・`tsuzuri_stack_top`をその領域の範囲に設定してから`tsuzuri_thread_entry(worker_id)`を呼ぶ。範囲を設定しないworkerは最初の呼び出しでトラップする。
5. Nodeホストと同じ初期化、spawn_workers/worker_ready import、失敗時の共有poisonと全wait通知、正常join後のWorker終了を実装する。

COOP/COEPが使えない場合にthreads要求を黙って逐次実行へ置き換えないでください。
