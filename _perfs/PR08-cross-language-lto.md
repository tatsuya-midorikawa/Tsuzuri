# PR08: 言語間 LTO と bitcode 出力

| 項目 | 内容 |
| --- | --- |
| ID | PR08 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | (PB01), (E12) |
| 関連 | E05, E06, E13, PR07 |
| 状態 | todo |
| 起票 | 2026-09-29（未レビュー） |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/llvm.rs`, `src/llvm_abi.rs`, `docs/language.md`, `tests/host_abi.rs`, `tests/lto.mjs`（新規） |

## 目的

Tsuzuri の計算カーネルを C/C++/Rust のホストに組み込むとき、関数呼び出しの境界を越えてインライン展開・定数伝播ができるようにする。
細かい粒度でホストから呼ぶ場合の呼び出しコストをなくし、組み込み用途で C/C++ だけで書いた場合と同等以上の速度にする。

## 現状と計測

- 出力は native の実行ファイル・object、LLVM IR テキスト、C header、WASM です。LLVM bitcode の出力はありません。
- 公開関数は `tz_<name>` の wrapper で、ホストからの呼び出しは通常の関数呼び出しです。LTO は既定で使いません（docs/benchmarks.md の測定条件）。
- `--emit llvm` の IR は Clang で再コンパイルできますが、ランタイムの C コード（`task.c` など）は含まれません。
- [なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md) は、ホスト境界には引数変換・呼び出しなどのコストがあり、細かすぎる呼び出しは不利になり得ると説明しています。

## 目標と指標

- 目標: ホストのループから小さな `tz_*` 関数を呼ぶ種目で、LTO を使う場合の時間が C/C++ だけで書いた場合と同等。
- 指標: LTO の有無による時間、ホストのループ内での呼び出しの消失（生成アセンブリ）。

## 施策

- `--emit bitcode` を追加し、Tsuzuri のコードとランタイム（C ランタイムも Clang で bitcode にする）を一つの bitcode として出力する。
- ホストは Clang の `-flto`（C/C++）や Rust の linker-plugin LTO で同じ LLVM の版の bitcode と結合する。使える版の組み合わせを文書化し、版の不一致はホスト側のリンクで検出されることを明記する。
- 公開 wrapper の属性（PR01 の事実のうちホスト契約に依存しないもの）を LTO 後の最適化に使えるようにする。
- C header と同じ公開 ABI を保ち、bitcode でも `tz_*` 以外のシンボルを公開しない（内部シンボルは internal のまま）。

## 意味・安全性の保持

- LTO は呼び出しの境界を消すだけで、Tsuzuri 側の検査（借用バッファの範囲・UTF-8 の検証など）を省略しない。
- 内部の関数・ランタイムの名前がホストのシンボルと衝突しないよう、internal linkage と `tsuzuri_` 接頭辞の規則を保つ。

## 検証

- C と Rust のホストから LTO で結合し、`tests/host_abi.rs`・E2E と同じ結果になること。ASan。
- LLVM の版の組み合わせの確認手順を用意する。
- 呼び出しの消失と時間を記録する。

## 受け入れ条件

- [ ] bitcode 出力とホストとの LTO の手順があり、結果が通常のリンクと一致する。
- [ ] 小さな関数を細かく呼ぶ種目で効果を実測し、記録している。

## リスク

- LLVM の bitcode は版の互換性に制約がある。コンパイラが使う Clang の版を出力に記録する。

## 対象外

- WASM の LTO（wasm-ld の LTO は別途検討）、JIT。

## 未決事項

- **ランタイムの扱い**: 既定案は C ランタイムも bitcode に含め、ホストが一つの bitcode として結合できるようにする。
