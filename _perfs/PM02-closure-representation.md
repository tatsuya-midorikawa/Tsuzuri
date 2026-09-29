# PM02: 関数値の表現の縮小

| 項目 | 内容 |
| --- | --- |
| ID | PM02 |
| 分類 | メモリ |
| 優先度 | P2 |
| 規模 | M |
| 依存 | PX01 |
| 関連 | PR03, PM07, B06 |
| 状態 | todo |
| 起票 | 2026-09-29（未レビュー） |
| 主な影響ファイル | `src/llvm.rs`, `src/runtime/closure.ll`, `src/llvm_task.rs`, `src/closures.rs`, `src/llvm_frame.rs`, `docs/architecture.md`, `tests/primitives.mjs`, `tests/tasks.mjs` |

## 目的

関数値（クロージャー）と Task の値を 4 ポインター（32 bytes）から 2 ポインター（16 bytes）に縮め、関数値を格納するデータ構造のメモリ量と、関数値の複製・受け渡しのコストを半分にする。

## 現状と計測

- `%tz.closure = type { ptr, ptr, ptr, ptr }`（コード、環境、clone 関数、drop 関数）で、関数値と Task は 32 bytes です（`src/llvm.rs` の `storage_layout`）。
- 捕捉のない関数値も 32 bytes です。捕捉が 1 個の小さなスカラーなら、環境の欄にビット列を直接置きます（immediate capture）。
- 2026-09-26 の初回の改善で、関数記述子の 4 ポインター表現・公開 ABI・Task ABI は変えずに adapter を改善した経緯があります（docs/benchmarks.md）。
- 関数値を多数格納する処理（イベントハンドラーの配列、関数値を持つレコード）のメモリ比較はありません。

## 目標と指標

- 目標: 関数値と Task の値を 16 bytes にする。捕捉のない関数値の生成・複製・解放で確保をしない（現状維持）。
- 指標: 関数値の配列・レコードのメモリ量、`control/closure_capture`・`closure_churn` と Task 系の種目の時間。

## 施策

- 表現を `{ コード, 環境 }` にし、clone／drop の関数ポインターは環境の先頭（ヘッダー）に置く。ヘッダーは静的な記述子（clone・drop・環境のサイズ）へのポインター 1 個とする。
- 捕捉のない関数値は環境を null にし、clone／drop を呼ばない。immediate capture は現在どおり環境の欄にビット列を置き、「ヘッダーを持たない」ことをコードポインター側の情報（adapter の種類）で区別する。
- 呼び出し規約（値・環境・借用の順）と Task の実行 ABI（環境だけを渡す）の関係を保つ。

## 意味・安全性の保持

- 関数値の Copy の意味（環境の独立した複製）と、解放の一回性を変えない。
- 公開 ABI（`tz_*`）は関数値を受け取らないため影響しない。extern のコールバック（E12 Phase 2）の設計と整合させる。

## 検証

- `tests/primitives.mjs`（immediate capture を含む）と `tests/tasks.mjs`、`tests/computations.mjs` を native/WASM × `-O0`/`-O3` で実行し、確保追跡 `live == 0`。
- 関数値の配列・Task の配列での確保量とメモリ量を記録する。

## 受け入れ条件

- [ ] 関数値と Task の値が 16 bytes になり、全テストが通る。
- [ ] 関数値を使う種目の時間とメモリの変化を記録している。

## リスク

- clone／drop の取得に間接参照が一段増える。複製の多い処理で遅くならないか計測する。
- `parallel_results` の未開始タスクの捕捉値の解放（B06）など、環境の所有権の経路が多い。全経路を洗い出してから変更する。

## 対象外

- 関数値の実行時の特殊化、関数値の比較・Hash。

## 未決事項

- **ヘッダーの形**: 既定案は静的な記述子へのポインター 1 個。clone／drop を直接 2 個置く案より環境あたり 8 bytes 小さい。
