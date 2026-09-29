# F11: WASM メモリ上限の設定と拡張

| 項目 | 内容 |
|---|---|
| ID | F11 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | F13, E13 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: WASM の線形メモリが stack・data・heap 合計 16 MiB に固定され、Rust／C++ の wasm32（最大 4 GiB）より扱えるデータが小さい |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/package.rs`, `src/llvm.rs`, `src/runtime/heap-wasm.ll`, `src/runtime/heap-wasm-threads.ll`, `src/runtime/wasm-threads.mjs`, `docs/language.md`, `tests/e2e.mjs`, `tests/wasm_threads.mjs` |

## 目的

画像・音声・データ処理を WASM で行うときに、プログラムの必要に応じてメモリ上限とスタックの大きさを指定できるようにする。
既定値と既定の出力は変えず、明示した場合だけ上限を変える。

## 現状

- `src/driver.rs` の wasm-ld 引数は `--stack-first -z stack-size=1048576 --max-memory=16777216`。threads では `--shared-memory --import-memory` を追加し、Node ホストは全 256 page を最初に確保する。
- `src/runtime/heap-wasm.ll` は `@__heap_base` からの free-list で、`llvm.wasm.memory.grow` により拡張し、`icmp ule i32 %end, 16777216` で上限を固定している。
- 「WASM は stack-first 配置で 1 MiB のスタック、線形メモリ上限 16 MiB を設定します」（docs/language.md 再帰とスタック）。worker のスタックは 256 KiB。
- E2E は `memory.buffer.byteLength <= 16 MiB` を検査する（GUIDE §7.2）。

## 仕様

### Phase 1（実装対象）

- CLI `--wasm-max-memory <size>` と `--wasm-stack-size <size>`（`build`／`run`／`test`、WASM だけ）。manifest の `[wasm] max-memory`／`stack-size` でも指定できる。
  size は byte 数または `KiB`／`MiB`／`GiB` 接尾辞。max-memory は 64 KiB の倍数で 128 KiB 以上 4 GiB 以下、stack は 16 byte の倍数でメモリ上限より小さいこと。違反・native との併用は `E2000`。
- 既定値は 16 MiB／1 MiB のままで、指定しない場合の出力はバイト単位で変わらない。
- heap の上限はリンク時の値と一致させる。上限に達した確保は従来どおり `AllocationFailure` のトラップ。
- threads では shared memory の最大値も同じ値にし、Node ホストは初期 page 数と最大値を module の宣言から読む（最大値ぶんを最初に確保しない）。
- 公開 ABI の descriptor（offset 0 の i32 pointer）は wasm32 のまま変えない。

### Phase 2（設計方針）

- memory64（`--target wasm64`）: pointer が 64-bit になるため、descriptor と host glue（E13）の ABI 版を分ける。対応エンジン（Node の版）を確認してから進める。

## 設計

- heap の上限定数を IR 生成時に埋め込む（例: `@tz.heap.limit` の定数 global）。driver が同じ値を wasm-ld に渡し、両者の不一致を起こさない。
- 4 GiB 近くの境界で `i32` の加算が折り返さないよう、heap の終端計算を `i64` で行う。
- G11 の cache key に新しいオプションを含める。

## 実装手順

1. **オプションと検証**: CLI・manifest・`E2000`。確認: `src/main.rs` の引数テスト（`rejects_ambiguous_or_unused_arguments` を更新）。
2. **heap の上限**: 定数の埋め込みと `i64` の境界計算。確認: 既定値で IR・wasm が不変（バイト比較）。
3. **大きな確保**: 64 MiB 上限で 32 MiB の配列を作るケース、上限でのトラップ。確認: `tests/e2e.mjs`。
4. **threads**: shared memory の最大値と Node ホスト。確認: `tests/wasm_threads.mjs`。

## テスト計画

- E2E: WASM × `-O0`/`-O3` で既定・拡張の両方、import が空のまま、上限ちょうどの確保、境界の算術（4 GiB 近くは IR 単体テストで検査）。

## ドキュメント

- `docs/language.md` の再帰とスタック・配列、`_docs/guides/webassembly.md`、`_docs/guides/wasm-threads.md`、`_docs/tools/command-line.md`。

## 受け入れ条件

- [ ] WASM のメモリ上限とスタックを明示的に指定でき、heap がその上限まで使える。
- [ ] 既定の出力が変わらない。

## 落とし穴

- heap の上限定数と wasm-ld の `--max-memory` を別々に設定すると不一致になる。一か所から生成する。
- ブラウザーでは大きな shared memory の確保に失敗することがある。ホストのエラーを明示する。

## 対象外

- 複数 memory、memory の縮小、GC 型との連携。

## 未決事項

- **既定値の変更**: 既定案は変更しない。既定を大きくするかは利用実績とブラウザーの制約を見て判断する。
