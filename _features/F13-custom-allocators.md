# F13: アロケーターの差し替え・確保統計・freestanding 出力

| 項目 | 内容 |
|---|---|
| ID | F13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | (F11), (E12), (E14) |
| 後続 | G15 Phase 3 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: allocator を細かく制御できない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）／追加: libc のない環境へ出力できない |
| 主な影響ファイル | `src/runtime/heap-native.ll`, `src/runtime/heap-wasm.ll`, `src/llvm.rs`, `src/llvm_abi.rs`, `src/driver.rs`, `src/main.rs`, `docs/language.md`, `tests/host_abi.rs`, `tests/allocator.mjs`（新規） |

## 目的

C/C++ のカスタム allocator・メモリプール、Rust の `#[global_allocator]`・`no_std` に相当する制御を提供する。
組み込み機器・ゲーム・リアルタイム処理のホストが確保方式を決められるようにし、libc のない環境への出力の土台を作る。

## 現状

- native の `@tz.alloc`／`@tz.free`／`tz.realloc` は `src/runtime/heap-native.ll` で malloc／free／realloc を呼ぶ。
- WASM は `src/runtime/heap-wasm.ll` の free-list（再利用・結合）、threads は lock wrapper。確保失敗はトラップ。
- 拡張 ABI 用に `tsuzuri_alloc`／`tsuzuri_free` を weak な公開関数として出力する（E05）。
- arena・確保統計・freestanding（libc なし）の出力はない。native の trap reporter は `write` を使う。

## 仕様

### Phase 1: ホスト提供の allocator（実装対象）

- native の object 出力で `--allocator host` を指定すると、内部の確保を `tsuzuri_host_alloc(size, align)`／`tsuzuri_host_free(ptr, size, align)`／`tsuzuri_host_realloc(ptr, old_size, new_size, align)` の extern へ接続する。
  サイズ付きの free により、ホストはプールや slab を実装しやすくなる。既定（`--allocator system`）の出力は変えない。
- ホストの allocator は thread-safe でなければならない（`Task.parallel` の worker から呼ばれる）。null を返した確保は従来どおり `AllocationFailure` のトラップ。
- 生成 header に allocator の prototype と契約（alignment、サイズ 0 の扱い）を出す。

### Phase 2: 確保統計と WASM allocator（設計方針）

- `--allocator counting`（診断用）で確保回数・現在量・最大量を公開関数から取得できるようにする。E2E の確保追跡を置換ではなく正式な機能にする。
- WASM の size-class allocator を実装し、free-list と実測で比較してから切り替える。

### Phase 3: freestanding 出力（設計方針）

- `--freestanding`（native object だけ）: libc に依存しない出力。host allocator を必須にし、IO・Task・Debug・trap reporter の `write` は使えない（到達すれば `E2000`）。トラップは `llvm.trap` だけ。
- G15 の bare-metal target の前提になる。

## 設計

- allocator の選択は `emit_target` の runtime 連結で `heap-native.ll` の代わりに host 版の小さな IR を連結して行う。内部シンボル（`@tz.alloc` など）の呼び出し側は変えない。
- サイズ付き free のため、確保サイズを呼び出し側が知らない経路（`tsuzuri_free` による外部からの解放など）ではヘッダーにサイズを保存するか、その経路の API を分ける（未決事項）。

## 実装手順

1. **サイズ情報の調査**: 全 free 経路で解放サイズが分かるかを確認する。確認: 調査結果をチケットに追記。
2. **host allocator**: 連結・header・契約。確認: C ホストのプール allocator で全 E2E fixture を実行し、プールの残量で漏れがないことを確認。
3. **並列**: worker からの呼び出し。確認: TSan。
4. **Phase 2/3 の設計レビュー**。

## テスト計画

- E2E: native × `-O0`/`-O3` で system／host 両方の allocator、ASan、確保・解放の対応（サイズと alignment の一致）。
- 既定の出力が変わらないことを IR の比較で確認する。

## ドキュメント

- `docs/language.md` の公開 ABI・スタックとヒープ、`_docs/guides/native-interop.md`、`_docs/tools/command-line.md`。

## 受け入れ条件

- [ ] ホストが提供する allocator で全ての確保・解放が行われ、サイズと alignment が一致する。
- [ ] 既定の出力が変わらない。

## 落とし穴

- ランタイム C（task.c、io.c など）の内部確保も host allocator を通す必要がある。
- `tsuzuri_free` でホストが解放する返却 buffer と、host allocator の free の対応を混同しない。

## 対象外

- 利用者コード内での allocator の型パラメーター化、arena に確保した値の寿命検査（A12 以降の region 設計が必要）。

## 未決事項

- **サイズ付き free**: 既定案は全経路でサイズを渡し、外部解放の `tsuzuri_free` だけはヘッダーにサイズを保存する。
- **alignment**: 既定案は常に 16 byte を要求する。
