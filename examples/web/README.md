# Web ホスト

既存のPhysics/simulation例は通常のWASMで、SharedArrayBufferを必要としません。

`--wasm-feature threads`の実行ホストは現在Node.js用です。リポジトリの`src/runtime/wasm-threads.mjs`と`tests/wasm_threads.mjs`を参照してください。

Browserへ移植する場合の手順:

1. HTTPSまたはlocalhostで`Cross-Origin-Opener-Policy: same-origin`と`Cross-Origin-Embedder-Policy: require-corp`を配信し、`crossOriginIsolated`を検査する。
2. moduleがimportするenv.memoryの最大page数（既定256page、`--wasm-max-memory`で変更）のshared WebAssembly.Memoryと同じWebAssembly.Moduleを全Workerへ渡す。
3. 計算の呼び出し元もWorkerに置く。UI thread上ではatomic waitできない。
4. 各instanceの`__stack_pointer`を独立した256KiB領域の終端に、`tsuzuri_stack_base`・`tsuzuri_stack_top`をその領域の範囲に設定してから`tsuzuri_thread_entry(worker_id)`を呼ぶ。範囲を設定しないworkerは最初の呼び出しでトラップする。
5. Nodeホストと同じ初期化、spawn_workers/worker_ready import、失敗時の共有poisonと全wait通知、正常join後のWorker終了を実装する。

Browser本番glueは未実装です。COOP/COEPが使えない場合にthreads要求を黙って逐次実行へ置き換えないでください。
