# G15: 対応ターゲットの拡張とクロスコンパイル

| 項目 | 内容 |
|---|---|
| ID | G15 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | G10, G14, (F13) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: 対応プラットフォームの幅（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/llvm.rs`, `src/runtime/*.c`, `scripts/`（toolchain）, `.github/workflows/`, `README.md`, `_docs/tools/build-and-cache.md` |

## 目的

C/C++ や Rust と同じく、ビルド機と異なる target への出力（クロスコンパイル）と、より多くの OS・CPU・ABI への対応を段階的に広げる。
対応の程度を「CI でビルドとテスト」「CI でビルドだけ」「最善努力」の階層で明示する。

## 現状

- `--target native|wasm32`。native はビルド機の triple で、別 triple への出力はない。
- Windows x64（MSVC）は実装済みだが Windows 上の実行検証がなく `blocked`（G10）。README: 「PDB/ARM64/MinGW/DLL import library自動生成は対象外です」。
- VS Code 拡張のホストは darwin-arm64/x64、linux-x64/arm64、win32-x64/arm64。Linux の生成物は同梱 musl を使う。
- F05 の x86 経路（SSE4.2／AVX2）はクロスコンパイル済みだが実機未検証。

## 仕様

### Phase 1: クロスコンパイル（実装対象）

- `--target <llvm-triple>` で native 系の出力先を指定できる（例: `x86_64-unknown-linux-musl`、`aarch64-unknown-linux-gnu`、`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`）。
- sysroot と libc は G14 の同梱 SDK から解決する。見つからない target は `E2000`。ランタイム C（task.c・cpu.c・io.c など）は target ごとにコンパイルする。
- `run`／`test` は、実行できない target では明示的に拒否する（`E2000`）。qemu-user などの実行器は明示指定したときだけ使う。
- 対応階層（Tier）の文書: Tier 1 = CI でビルドとテストを実行、Tier 2 = CI でビルドだけ、Tier 3 = 最善努力。階層を実際の CI の内容と一致させる。

### Phase 2 以降（設計方針）

- Windows ARM64（G10 の完了後）、Android（NDK）と iOS の静的ライブラリ出力、MinGW の要否の判断。
- bare-metal（thumbv7em、riscv32 など）は F13 の freestanding 出力の後に検討する。

## 設計

- driver の `native_compile_args` を target triple から決める形に一般化する（G10 の Windows 分岐と同じ関数に集約する）。
- `--cpu native` はクロスコンパイル時に拒否する（ビルド機の ISA は無関係なため）。
- G11 の cache key に target triple を含める。

## 実装手順

1. **triple の指定と検証**: 確認: 引数テスト、未対応 triple の `E2000`。
2. **Linux musl／gnu 間のクロス**: 確認: macOS から Linux の実行ファイルを作り、Linux CI で実行する。
3. **macOS の arm64／x86_64 間**: 確認: 両方の実行ファイルを CI で実行する。
4. **Tier 文書と CI**: 確認: 文書の Tier と CI のジョブが一致する。
5. **Phase 2 の設計レビュー**。

## テスト計画

- 各 Tier 1 target で README の E2E（native `-O0`/`-O3`）を実行する。Tier 2 はビルドと `llvm-objdump` による形式確認。

## ドキュメント

- `README.md`、`_docs/tools/build-and-cache.md`、`_docs/tools/command-line.md`、`_docs/feature-status.md`。

## 受け入れ条件

- [ ] 別 triple への native 出力ができ、Tier 1 target は CI で実行検証されている。
- [ ] 実行検証していない target を「対応」と記載しない。

## 落とし穴

- F05 の CPU dispatch は target の ISA に依存する。クロス先の baseline を正しく選ぶ。
- Windows の object への runtime 埋め込みは G10 で `E2002` とした制約を引き継ぐ。

## 対象外

- 32-bit native target（i686、armv7）の優先対応、GPU 付き target の自動検出。

## 未決事項

- **Tier の CI 資源**: 既定案は GitHub Actions の標準 runner で賄える target を Tier 1 とする。
- **MinGW**: 既定案は需要が示されるまで対象外。
