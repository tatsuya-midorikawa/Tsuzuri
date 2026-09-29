# F08: 256／512-bit SIMD と関数単位の CPU 多版化

| 項目 | 内容 |
|---|---|
| ID | F08 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | F04, F05, (C08) |
| 後続 | C11, C09 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: 最適化の自由度（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）／追加: SIMD が 128-bit だけで、実行時 ISA 選択が同梱の `Array.sum<i64>` に限られる |
| 主な影響ファイル | `src/simd.rs`, `src/llvm_simd.rs`, `src/check.rs`, `src/llvm.rs`, `src/driver.rs`, `src/runtime/cpu.c`, `std/`（Simd API）, `docs/language.md`, `tests/simd.mjs`, `tests/cpu_dispatch.mjs` |

## 目的

C/C++ の intrinsics・`target_clones`、Rust の `#[target_feature]`・`std::arch` と同じく、AVX2／AVX-512／SVE の幅を明示的に使い、
利用者の関数を実行時に最適な命令セット版へ振り分けられるようにする。どの版も同じ結果を返し、速度だけが異なることを保証する。

## 現状

- SIMD 型は 128-bit だけ（`src/simd.rs` で lane 数 = 128 / 要素ビット幅、数値 10 型と mask 4 型）。
  「256-bit vector、gather/scatter、integer division、可変slice storeは後続段階です」（docs/language.md SIMD 値型）。`Simd.load` はあるが store はない。
- 実行時の CPU 選択は native の同梱 `Array.sum<i64>` だけ（`src/runtime/cpu.c`、SSE4.2／AVX2 を CPUID と XCR0 で判定、atomic な選択 cache）。
  「AArch64と未知環境はbaseline、SVE/SVE2・float dispatch・任意ユーザー関数の多重化は未実装です」（docs/architecture.md）。x86 の経路は実機未検証。
- `--cpu native` はビルド機の ISA 全体を有効にし、配布物には向かない。

## 仕様

### Phase 1: 256-bit の移植可能な型（実装対象）

- `i8x32`・`i16x16`・`i32x8`・`i64x4`・同じ符号なし型・`f32x8`・`f64x4` と対応する mask を追加する。演算・比較・`select`・順序付き `sum_lanes` の意味は 128-bit 型と同じ。
- AVX2 がない target では LLVM が 2 × 128-bit などへ分割する。結果は同じで、速度だけが異なる。WASM は simd128 指定時に 2 × v128、それ以外は scalar。
- store・gather は C08（排他スライス）の完了後に `Simd.store (ref mut dst) index vector` として追加する（境界検査は全 lane を先に検査）。

### Phase 2: 関数単位の多版化（設計方針、構文は要承認）

- 利用者の関数に対象 ISA の一覧を指定すると、同じ型付き IR から ISA ごとの版と選択関数を生成する。選択は F05 と同じ atomic cache を使う。
- 各版で fast-math・再結合・暗黙 FMA を使わないため、結果は全版で一致する。整数の結合的な演算だけが自動ベクトル化で順序を変えられる（D-14）。

### Phase 3: SVE／SVE2（研究）

- 可変長ベクトル（LLVM の `vscale`）の型と API の設計。固定長型との関係と、NEON との性能差を実測してから判断する。

## 設計

- `SimdType` の幅を 128 固定から 128/256（Phase 3 で scalable）へ一般化する。GUIDE §6.3 の全項目（レイアウト、mask の保存幅、ABI 不可）を更新する。
- 多版化は `emit_target` の関数出力を ISA 属性付きで複製し、名前を決定的にマングリングする。F05 のランタイム選択を一般化して共有する。
- 明示要求した ISA がない機械での強制選択（`TSUZURI_CPU_FORCE`）は F05 と同じく明示失敗にする。

## 実装手順

1. **256-bit 型**: 型・演算・比較・還元。確認: `tests/simd.mjs` を 256-bit へ拡張し、参照と照合（native/WASM × `-O0`/`-O3`、SIMD 有無）。
2. **生成コードの確認**: AVX2 target と AArch64 で命令を確認する（`llvm-objdump`）。
3. **store／gather**: C08 の完了後。確認: 境界トラップと部分書き込みがないこと。
4. **Phase 2 の設計レビュー**: 構文・ISA 名・選択の観測方法。
5. **x86 実機検証**: F05 の未検証経路とあわせて実機で結果と速度を記録する。

## テスト計画

- 全版の結果一致を `TSUZURI_CPU_FORCE` で版ごとに検査する（速度の閾値は CI に入れない）。
- NaN・符号付きゼロ・整数の折り返し・シフト量のマスクを lane ごとに照合する。

## ドキュメント

- `docs/language.md` の SIMD 値型、`docs/architecture.md` の性能設計の原則、`_docs/library-reference/simd.md`、`_docs/guides/performance.md`、`docs/benchmarks.md`。

## 受け入れ条件

- [ ] 256-bit 型が全 target で同じ結果を返し、AVX2 target では 256-bit 命令を生成する。
- [ ] 多版化の設計が承認され、版ごとの結果一致を検査できる。

## 落とし穴

- AVX-512 は周波数低下で遅くなる場合がある。使うかどうかは実測で決める。
- mask の保存幅（lane 数 bit）と 256-bit での ABI・フレーム配置を確認する。
- `--cpu generic` の配布物に未対応命令を無条件に入れない。

## 対象外

- 利用者が書く ISA 固有の intrinsics（`_mm256_*` の直接公開）、自動並列化。

## 未決事項

- **多版化の構文**: 既定案は宣言の修飾（例: `def name :: ... for cpu [avx2, avx512f]`）。代替案は manifest で関数名と ISA を列挙する方式。
- **512-bit 型**: 既定案は Phase 1 に含めず、AVX-512 と SVE の実測後に判断する。
