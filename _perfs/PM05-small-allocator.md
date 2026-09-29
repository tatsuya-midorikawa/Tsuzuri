# PM05: 小さい確保のための size-class allocator

| 項目 | 内容 |
| --- | --- |
| ID | PM05 |
| 分類 | メモリ |
| 優先度 | P1 |
| 規模 | L |
| 依存 | PX01, (F13) |
| 関連 | F11, F13, PM04, A04 |
| 状態 | todo |
| 起票 | 2026-09-29（未レビュー） |
| 主な影響ファイル | `src/runtime/heap-native.ll`, `src/runtime/heap-wasm.ll`, `src/runtime/heap-wasm-threads.ll`, `src/runtime/`（新規 allocator）, `src/llvm.rs`, `tests/primitives.mjs`, `tests/wasm_threads.mjs` |

## 目的

再帰 union のノード、小さな文字列、関数値の環境など、小さく多数の確保を、確保 1 回あたりのメモリ量と時間の両面で C の malloc・Rust の既定 allocator より効率化する。
所有権により解放の時点とサイズがコンパイル時に分かる Tsuzuri の性質を使い、ヘッダーのない確保を実現する。

## 現状と計測

- native の `@tz.alloc`／`@tz.free` は OS の malloc／free です（`src/runtime/heap-native.ll`）。
- WASM は `src/runtime/heap-wasm.ll` の空きリストで、要求サイズに 16 bytes のヘッダーを加えて 16 bytes 単位に丸め（`(size + 31) & -16`）、先頭から線形に探索（first fit）します。
  8 bytes の確保でも 32 bytes を使い、空きブロックが多いと確保のたびに探索が長くなります。
- `@tz.free` はサイズを受け取りません。
- 2026-09-29 のフロントエンドの計測では、コンパイラ自身も時間の 57% を malloc／free に使っていました（PB02）。利用者のプログラムの確保の比率は計測していません。

## 目標と指標

- 目標: binary-trees などの確保の多い種目（PX02）で、C（malloc）・Rust・Zig の既定 allocator より少ない最大 RSS と短い時間。
- 指標: 確保 1 回あたりのメモリ、最大 RSS、確保・解放の時間、断片化（空き領域の比率）。

## 施策

### Phase 1: サイズ付きの解放

- 生成コードの全ての解放に確保時のサイズを渡す（`@tz.free(ptr, size)`）。文字列・配列は長さと要素サイズから、ノード・環境は型から決まる。
  ホストが解放する返却バッファ（`tsuzuri_free`）は外部の経路として別に扱う（F13 と共通の調査）。

### Phase 2: size-class allocator

- 16〜256 bytes 程度の小さな確保を、サイズ区分ごとのページ（slab）から割り当てる。ブロックにヘッダーを持たず、区分はサイズ付きの解放で決まる。
- native はスレッドごとのキャッシュを持ち、`Task.parallel` の worker からの確保・解放で共有の lock を避ける。大きな確保は OS の allocator に任せる。
- WASM は単一スレッド版を既定とし、threads 版（F06）では既存の heap lock の方式と整合させる。

## 意味・安全性の保持

- 確保失敗は従来どおりトラップ。確保サイズの overflow の検査を保つ。
- 解放の一回性と、解放済み領域の再利用によるデータの漏洩がないこと（未初期化の領域を利用者に見せない現在の規則を保つ）。
- 公開 ABI の `tsuzuri_alloc`／`tsuzuri_free` の契約（F13、E05）を変えない。

## 検証

- 全 E2E を native/WASM × `-O0`/`-O3` で実行し、確保追跡 `live == 0`。ASan（native）と独自の二重解放検出（WASM）。
- TSan で worker の確保・解放。
- 確保の多い種目の最大 RSS と時間、断片化の指標を記録する。

## 受け入れ条件

- [ ] 全ての解放がサイズを持ち、size-class allocator で全テストが通る。
- [ ] 確保の多い種目でメモリと時間の改善を実測し、記録している。

## リスク

- スレッドキャッシュは、他スレッドで確保された領域の解放の扱いが難しい。Phase 2 の設計でリモート解放のキューを定義する。
- 大きなページの予約は小さなプログラムの最大 RSS を増やす。最初のページは小さくする。

## 対象外

- GC、世代別の回収、利用者が選ぶ allocator（F13）。

## 未決事項

- **サイズ区分**: 既定案は 16 bytes 刻みで 256 bytes まで。計測で調整する。
