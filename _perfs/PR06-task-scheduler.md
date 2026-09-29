# PR06: Task プールの低遅延化と work stealing

| 項目 | 内容 |
| --- | --- |
| ID | PR06 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | F01, F02, B06, F06, F10 |
| 状態 | todo |
| 起票 | 2026-09-29（未レビュー） |
| 主な影響ファイル | `src/runtime/task.c`, `src/runtime/task-wasm-threads.c`, `tests/task_runtime.c`, `tests/tasks.mjs`, `benchmarks/run-tasks.mjs` |

## 目的

`Task.parallel` と `Parallel.*` の起動・配布・完了待ちの遅延を減らし、短い仕事でも並列化の利益が出るようにする。
.NET のスレッドプールや Rust の rayon と同等以上の fork/join 性能を目指す。

## 現状と計測

- 2026-09-26 の `cpp/task_parallel`（50000 × 16）の相対性能は 0.768 で、最速は C# でした。逐次の `task_sequence`／`task_sequential` は Tsuzuri が最速でした。
- `src/runtime/task.c` は、プール全体の mutex と条件変数で仕事を管理します。グループの次の添字 `group->next` を mutex の中で 1 件ずつ取り出し、待機中の worker を `pthread_cond_broadcast` で起こします。
- worker 数は `min(CPU 数, 32) - 1` まで。呼び出し元も仕事を実行し、未割り当ての仕事がなくなれば追加の worker を起こしません（F01）。
- work stealing、worker ごとのキュー、待機前のスピンはありません（docs/benchmarks.md の注記）。

## 目標と指標

- 目標: `task_parallel` で C# の最速と同等以上。仕事の粒度（1 µs〜1 ms）と件数を変えた種目で、逐次版より遅くならない最小の粒度を記録する。
- 指標: 1 グループあたりの起動・完了待ちの遅延、スケーリング（1〜10 コア）、CPU 時間の合計（無駄なスピンの検出）。

## 施策

- 添字の取得を mutex から外し、原子的な `fetch_add` で取得する。仕事が多い場合は複数件をまとめて取得する（件数は入力長だけから決め、スレッド数に依存させない分割の規則 D-14 は `Parallel.*` の結果に影響しないことを確認する）。
- 起こす worker の数を残りの仕事の数に合わせ、全員を起こす broadcast を避ける。
- 短い待機ではスピンしてから条件変数（Linux は futex、macOS は `os_sync_wait_on_address` など）で待つ。スピンの長さは計測で決める。
- 入れ子のグループに worker ごとの deque と work stealing を導入し、呼び出し元が自分のグループを優先する現在の規則を保つ。
- WASM threads（F06）の共有メモリ版にも、同じ取得方式を `Atomics.wait`／`notify` で適用する。

## 意味・安全性の保持

- 結果の順序（入力の添字順）、全 worker の join、`parallel_results` の最小添字の Error と未開始の停止（B06）、トラップ時の扱いを変えない。
- データ競合を導入しない。TSan で検査する。
- `Parallel.*` の集計の分割と結合の順序は入力長だけで決まる現在の規則を保つ。

## 検証

- `tests/task_runtime.c`（条件変数による同時実行の確認）と `tests/tasks.mjs` を native・WASM threads × `-O0`/`-O3` で実行する。ASan/UBSan/TSan。
- 失敗注入（スレッド作成の失敗）の診断が変わらないこと。
- 粒度と件数を変えた計測を記録する。

## 受け入れ条件

- [ ] 仕事の取得と待機の改善後も、結果・失敗の契約と全 join が保たれる。
- [ ] `task_parallel` と粒度別の種目の改善を実測し、記録している。

## リスク

- スピンは CPU 時間と電力を増やす。CPU 時間の合計も記録し、既定のスピンを短く保つ。
- work stealing は実装の誤りが競合として現れやすい。モデル検査に近い網羅的な小規模テストを用意する。

## 対象外

- 非同期 I/O の実行器（B08）、GPU への自動移送。

## 未決事項

- **待機の API**: 既定案は OS ごとの軽量な待機を使い、使えない環境では条件変数に戻す。
