# F10: 並行処理プリミティブ（Atomic・Mutex・Channel）

| 項目 | 内容 |
|---|---|
| ID | F10 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F01, B07, (A13), (C10) |
| 後続 | C10 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: C++ の `std::atomic`／`mutex`、Rust の `Mutex`／channel、C# の `Interlocked`／`Channel<T>` に相当する共有状態・パイプラインがない |
| 主な影響ファイル | `std/Atomic.tz`・`std/Mutex.tz`・`std/Channel.tz`（新規）, `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/runtime/task.c`, `src/runtime/task-wasm-threads.c`, `docs/language.md`, `tests/tasks.mjs` |

## 目的

構造化された fork/join（`Task.parallel`、`Parallel`）だけでは表せない、進捗カウンター・共有キャッシュ・生産者と消費者のパイプラインを、データ競合なしに書けるようにする。

## 現状

- 「`and!`、detach、スレッド ID、共有状態、外部キャンセルトークン、回復可能なタスク例外は提供しません」（docs/language.md 並列区間と寿命）。
- Task の捕捉値・結果は参照を保持できない（`Send` は参照を保持しない型）。`Parallel.map_ref` は入力の共有借用を fork/join の内側に閉じて渡す。
- native は常駐 pool（`src/runtime/task.c`）、WASM は既定で逐次、threads opt-in は共有メモリ Worker（F06）。

## 仕様

### Phase 1: fork/join 内の共有（実装対象）

- スコープ付き並列 `Task.scope`（名前は未決）: 呼び出し元の値を共有借用で子タスクへ渡し、全子タスクの join 後に戻る。借用はスコープの外へ出ない。
- `Atomic<'a>`（`'a` は i32/i64/u32/u64/bool）: `new`、`load`、`store`、`fetch_add`、`compare_exchange`。Phase 1 の memory order は SeqCst だけ。非 Copy、共有借用で操作する。
- `Mutex<'a>`: `Mutex.with (ref m) (\value -> ...)` のコールバック形式で排他参照を貸す。ガード値を外へ出さない。同じ Mutex の入れ子のロックは実行時トラップ。
- `Channel<'a>`（`'a` は Send）: 容量付き `Channel.bounded n` が `(Sender<'a>, Receiver<'a>)` を返す。`send`／`recv` はブロッキング。全 Sender の drop（B07）で close し、`recv` は None を返す。
- 型の条件: 共有借用を子タスクへ渡せる型を表す組み込みクラス（例: `Sync`）を追加する。Tsuzuri の値は共有中は不変なので、既存の値の大半は満たし、`Mutex`／`Atomic` は内部可変性を持つ例外として実装する。
- WASM の既定の逐次 backend: Channel の待ちで進めるタスクがなければ「deadlock」としてトラップする（黙って停止しない）。threads opt-in では `Atomics.wait` を使う。

### Phase 2（設計方針）

- acquire/release などの memory order、`RwLock`、`Arc` との組み合わせ（C10）、非同期（B08）との接続。

## 設計

- `Atomic` の操作は LLVM の `atomicrmw`／`cmpxchg`（seq_cst）へ直接下げる。Mutex・Channel は C ランタイム（pthread／Win32／WASM threads）に置き、既存の pool と同じ連結条件を使う。
- 所有権: スコープ付き並列は既存の `Parallel.map_ref` の借用規則を一般化する。子タスクの本体は `Sync` な共有借用だけを捕捉できる。
- 逐次 backend では子タスクを入力順に実行し、Channel の待ちでは他の未完了タスクへ切り替えるか、切り替え不能なら deadlock トラップにする。

## 実装手順

1. **スコープ付き並列**: 借用の捕捉と join。確認: 借用の持ち出し拒否（`E1013`）、TSan。
2. **Atomic**: 確認: 並列カウンターの結果、生成 IR の `atomicrmw`。
3. **Mutex**: 確認: 排他性、入れ子ロックのトラップ、TSan。
4. **Channel**: 確認: 生産者と消費者、close、deadlock トラップ（逐次 backend）。
5. **WASM threads**: 確認: `tests/wasm_threads.mjs` の拡張。

## テスト計画

- Rust: 型条件と借用の拒否。
- E2E: native/WASM（既定と threads）× `-O0`/`-O3`、TSan、確保追跡。時間を合否条件にしない（条件変数による同時実行の確認を手本にする）。

## ドキュメント

- `docs/language.md` のタスク、`_docs/language-reference/tasks.md`、`_docs/library-reference/parallel.md`、GUIDE D-07。

## 受け入れ条件

- [ ] スコープ付き並列で共有借用を子タスクへ安全に渡せる。
- [ ] Atomic・Mutex・Channel がデータ競合なしに動作し、TSan で検出がない。
- [ ] 逐次 backend での deadlock を明示的にトラップする。

## 落とし穴

- Mutex のコールバックから同じ Mutex を借用する経路（関数値経由）を型検査だけでは防げない。実行時の検出を残す。
- Channel の要素の drop 順序と close 時の未受信要素の解放。

## 対象外

- detach されたスレッド、スレッド ID、ロックフリーのデータ構造の公開。

## 未決事項

- **スコープ付き並列の API**: 既定案は `Task.scope`。`Task.parallel` の拡張にする案もある。
- **memory order**: 既定案は Phase 1 を SeqCst に限定する。
