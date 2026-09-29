# PM08: 成果物サイズの最小化

| 項目 | 内容 |
| --- | --- |
| ID | PM08 |
| 分類 | 成果物サイズ |
| 優先度 | P2 |
| 規模 | M |
| 依存 | PB01 |
| 関連 | PB03, G08, F05 |
| 状態 | todo |
| 起票 | 2026-09-29（未レビュー） |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/llvm.rs`, `src/llvm_display.rs`, `src/runtime/numeric.c`, `docs/benchmarks.md`, `tests/e2e.mjs` |

## 目的

実行ファイル・object・WASM のサイズを、C・Zig の最小構成と同等にする。
組み込み機器、ブラウザーへの配信、起動時間、命令キャッシュの効率のいずれにもサイズが効きます。

## 現状と計測

2026-09-29、`examples/hello`（整数の結果を表示するプログラム）、macOS arm64:

- Tsuzuri の実行ファイル: 67,256 bytes（`-O3`）、100,824 bytes（`-O0`）。C の hello（Clang `-O3`）は 33,424 bytes。
- 結果の表示が `tz_soft_format` を参照するため、実行ファイルの IR に数値ランタイム（`numeric.ll`、`tz_soft_op`・`tz_soft_parse`・`tz_soft_math_*`・`tz_soft_hash_canonical` など）が含まれます。
  `-O3` では未使用の internal 関数が除かれますが、`-O0` ではすべて残ります。
- WASM の hello は 59 bytes（`-O3`）、208 bytes（`-O0`）です。
- 最適化の指定は `-O0`〜`-O3` だけで、サイズ優先（`-Os`／`-Oz`）はありません。合成 1,000 モジュールの `-O0` の実行ファイルは 7.1 MB でした。

## 目標と指標

- 目標: hello の実行ファイルを C の hello と同等（macOS arm64 で 34 KB 前後）にする。代表的な種目で、Rust（`opt-level = "z"`）・Zig（`ReleaseSmall`）と同等以下。
- 指標: 実行ファイル・object・WASM のサイズ（シンボル・デバッグ情報を除いたもの）。

## 施策

- 表示の分割: 整数・bool・文字列の表示が浮動小数点・decimal のランタイムを参照しないよう、`tz_soft_format` を型ごとの関数に分ける。PB01 の関数単位のランタイム object と合わせて、使う関数だけを連結する。
- `-Os`／`-Oz` の追加: Clang に同じ指定を渡し、G11 の cache key に含める。CPU dispatch（F05）の複数版はサイズ優先では baseline だけにする選択肢を設ける。
- 未使用コードの除去: native のリンクで `-dead_strip`（macOS）／`--gc-sections`（ELF）を使い、関数ごとの section を有効にする。
- シンボルの除去の指定（`--strip`）と、デバッグ情報を別ファイルに分ける既存の方式（G08）の整理。
- WASM: `-Oz` と wasm-ld の未使用コード除去の確認。名前 section の扱いを指定できるようにする。

## 意味・安全性の保持

- サイズ優先の最適化でも数値の意味・トラップ・評価順序は同じ。fast-math などは使わない。
- トラップ位置の表（G04）とデバッグ情報（G08）の対応を保つ。

## 検証

- 最適化の指定ごとに全 E2E を実行する（`-Os`／`-Oz` を native/WASM に追加）。
- 例とベンチマークの成果物サイズを記録し、C・Rust・Zig の同じ処理と比較する。

## 受け入れ条件

- [ ] hello の実行ファイルが浮動小数点のランタイムを含まない。
- [ ] `-Os`／`-Oz` が使え、全テストが通る。
- [ ] 成果物サイズの比較が記録されている。

## リスク

- 関数単位の section とリンカーの除去は、リンカーの版で動作が異なる。macOS（ld64／ld-prime）、lld、MSVC link（G10）で確認する。

## 対象外

- 実行ファイルの圧縮、動的ライブラリへの分割。

## 未決事項

- **既定の最適化**: 既定案は既定を `-O3` のまま変えず、サイズ優先は明示指定だけにする。
