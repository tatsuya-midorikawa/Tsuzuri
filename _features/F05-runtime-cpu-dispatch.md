# F05: 実行時の CPU 命令セット判定と関数の複数版
| 項目 | 内容 |
|---|---|
| ID | F05 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | C04 |
| 後続 | なし |
| 状態 | todo |
| 主な影響ファイル | `src/llvm.rs`, `src/driver.rs`, `src/main.rs`, `src/runtime/cpu.c`（新規）, `src/check.rs`, `tests/cpu_dispatch.rs`（新規）, `tests/cpu_dispatch.mjs`（新規）, `benchmarks/run-dispatch.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md` |

## 目的

- 配布用の `--cpu generic` 生成物でも、実行時に利用可能な ISA を検出し、選択済み標準カーネルだけを高速版に切り替える。
- すべてのユーザー関数を複数版にするのではなく、意味を保てる std / compiler-internal kernel に限定する。
- macOS で使えない GNU ifunc に依存せず、portable dispatcher を生成する。
- variant 間で bit-identical な結果を要求し、fast-math や未サポート命令実行を避ける。

## 現状

- `src/main.rs` は `--cpu generic|native` を CLI で受ける。
- `src/driver.rs::native_cpu_flag` は x86/x86_64 で `-march=native`、arm/aarch64 で `-mcpu=native` を返す。
- `src/driver.rs::BuildOptions::validate` は `--cpu native` を native executable/object に限定する。
- `docs/architecture.md` は「実行時 ISA 判定・複数版の選択は今後の実装」と書いている。
- `docs/benchmarks.md` は `--cpu native` の測定を generic と分けて記録している。
- `src/llvm.rs` は全関数を単一版の LLVM IR として出力している。
- `src/llvm.rs::emit_target` は intrinsic 宣言を `BTreeSet<String>` で deterministic に出力している。
- `src/driver.rs::build` は必要な場合だけ C runtime を一時ファイルに書き clang で object にしている。
- C04 の `Array` API は F05 の標準カーネル対象になる予定。

## 仕様

### 対象範囲

- 複数版にするのは compiler が所有する標準カーネルだけ。
- Phase 1 対象は C04 の `Array.map` / `Array.sum` / `Array.reduce` のうち、要素型と演算が固定された内部 worker。
- F02 の `Parallel.sum` の chunk 内 kernel は Phase 2 対象。
- F04 の `Simd` 明示 vector 関数は Phase 2 対象。
- ユーザー定義関数全般、再帰関数、任意 closure は対象外。
- variant selection はプログラムの意味に影響してはならない。

### ISA feature

- x86/x86_64 Phase 1 feature: `sse2` baseline、`sse4.2`, `avx2`。
- AArch64 Phase 1 feature: baseline NEON はアーキテクチャ標準として扱い、追加 feature は `asimd` を検出対象にしない。将来 `sve` / `sve2` を追加する。
- x86 の `avx2` variant は OSXSAVE / XCR0 による AVX state 有効性を確認してから選ぶ。
- feature がない場合は portable baseline variant。
- 検出不能 platform は baseline variant。

### 結果の同一性

- 全 variant は bit-identical な結果を返す。
- integer overflow は既存どおり wrapping。
- float は fast-math, reassociation, FMA contraction を使わない。
- `Array.sum` が左から右逐次なら、variant でも順序を変えない。
- SIMD / unroll で順序変更が必要な kernel は、D-14 に従う別 API だけ対象にする。
- trap の有無と位置は baseline と一致させる。
- bounds check と allocation failure の意味は変えない。

### CLI と `--cpu native`

- `--cpu generic` では baseline + runtime dispatch を使える。
- `--cpu native` はビルド機向けコードを生成する既存意味を維持する。
- `--cpu native` 指定時も runtime dispatch を完全に禁止しないが、baseline がビルド機 ISA を要求する可能性を docs に明記する。
- portable artifact を作りたい場合は `--cpu generic`。
- runtime dispatch の opt-out CLI は Phase 1 では追加しない。
- tests では環境変数または test hook で variant を強制できる。

### 前提とする他チケットのインターフェース

- C04: `Array` 一括操作 API が compiler-internal kernel に下がり、要素型・演算が静的に分かる。
- F04 は弱い利用者であり、明示 SIMD 型 variant は Phase 2。

### 他チケットへの提供インターフェース

- F02 は chunk 内の `Parallel.sum` を F05 kernel として登録できる。
- F04 は `Simd` std helper の fallback / ISA variant を F05 dispatcher に登録できる。
- F07 は CPU fallback の比較対象として、選択された CPU variant 名を benchmark JSON に記録できる。

## 設計

### runtime detection

- `src/runtime/cpu.c` を追加する。
- 外部入口は `__attribute__((weak, visibility("hidden"))) uint64_t tsuzuri_cpu_features(void)`。
- 返却値は bitset。
- bit 0: x86 `sse4.2`。
- bit 1: x86 `avx2`。
- bit 16: aarch64 `sve`。
- bit 17: aarch64 `sve2`。
- 未知 platform は 0。
- x86 は inline asm `cpuid` / `xgetbv` または compiler builtin を使う。
- compiler-rt の `__cpu_model` には依存しない。依存が必要なら docs と driver に明記するが、既定案は避ける。
- AArch64 Linux は `getauxval(AT_HWCAP)` / `AT_HWCAP2`。
- AArch64 macOS は `sysctlbyname("hw.optional.arm.FEAT_*")` 系を検討する。
- macOS の ifunc は使わない。
- `tsuzuri_cpu_features` は結果を static atomic に cache する。
- test hook として `TSUZURI_CPU_FORCE=baseline|sse4.2|avx2|sve|sve2` を許可するか、compile-time macro で差し替える。
- 環境変数 hook は runtime behavior なので docs では「テスト用・未保証」と明記する。

### IR variant

- `src/llvm.rs` に `KernelVariant` / `KernelFeatureSet` 相当の内部構造を追加する。
- baseline symbol: `@tz.kernel.<name>.baseline`。
- variant symbol: `@tz.kernel.<name>.avx2` など。
- dispatcher symbol: `@tz.kernel.<name>.dispatch`。
- call site は dispatcher から得た function pointer を call するか、初回に resolver を呼ぶ wrapper を call する。
- 推奨形:
  - `@tz.kernel.<name>.cached = internal global ptr null`
  - `define internal <ret> @tz.kernel.<name>(...)` が cached pointer を atomic load する。
  - null なら `@tz.kernel.<name>.resolve()` を呼ぶ。
  - 選択済み pointer を `cmpxchg` で cache する。
  - pointer call で variant を呼ぶ。
- benign race は許すが data race は atomic で避ける。
- `target-features` attribute は variant function にだけ付ける。
- x86 AVX2 variant には `"+avx2"` と必要に応じて `"+sse4.2"` を付ける。
- baseline には feature attribute を付けない。
- `target-cpu` は原則付けない。
- `--cpu native` の場合は driver clang flag が全体にかかるため、variant attribute との相互作用をテストする。

### driver

- `src/driver.rs::build` は IR に `@tsuzuri_cpu_features` が出る場合だけ `src/runtime/cpu.c` を compile/link する。
- task runtime と同じ一時 C source / object 方針にする。
- executable では `-pthread` は不要。task runtime と同時に必要なら task 側が付ける。
- object emit では module object と cpu runtime object を relocatable link する。
- wasm32 では runtime CPU dispatch を使わない。
- `--emit llvm` は runtime C を含まないため、docs に「直接 link する場合は `src/runtime/cpu.c` を追加」と書く。

### kernel marking

- user syntax で `#[target_feature]` のような属性は導入しない。
- compiler-internal `KernelId` を C04/F02/F04 lowering が要求する。
- 決定的な `BTreeSet<KernelId>` で必要 kernel を集める。
- variant 数は Phase 1 で baseline + 2 までに制限する。
- variant 生成が意味を変える可能性のある reducer は登録しない。

## 実装手順

1. **runtime detection を単体実装する。**
   - `src/runtime/cpu.c` を追加する。
   - x86 cpuid と fallback 0 を実装する。
   - 確認: 小さい C test で `tsuzuri_cpu_features()` が呼べる。

2. **driver 連結を追加する。**
   - `@tsuzuri_cpu_features` 出現時だけ C runtime を compile する。
   - object/exe の両方を task runtime と共存させる。
   - 確認: `--emit object` と executable link の smoke test。

3. **IR dispatcher を追加する。**
   - まだ variant は baseline と同じ body にして、dispatch / cache だけを検証する。
   - 確認: IR deterministic、atomic load/cmpxchg がある。

4. **C04 kernel を一つ登録する。**
   - 例: `Array.sum` の i64 wrapping sum。
   - baseline と avx2 variant を生成する。
   - variant は順序変更しない範囲で vectorization 可能な形に限定する。
   - 確認: force hook で baseline/avx2 をそれぞれ実行し同じ結果。

5. **テストと benchmark を拡張する。**
   - `tests/cpu_dispatch.mjs` で env force を使う。
   - `benchmarks/run-dispatch.mjs` で variant 名、feature bit、結果を JSON に出す。

6. **docs を更新する。**
   - 実装済み ISA と計画を分けて書く。
   - `--cpu native` との違いを README と architecture に書く。

## テスト計画

- Rust test: dispatcher IR が deterministic。
- Rust test: target-features attribute が variant にだけ付く。
- Rust test: baseline には unsupported feature が付かない。
- Node E2E: `TSUZURI_CPU_FORCE=baseline` と `TSUZURI_CPU_FORCE=avx2` で同じ checksum。
- Node E2E: force した feature が実機にない場合は、その variant を実行しない compile-time hook を使うか、unsupported force を error にする。
- Native E2E: `--cpu generic` と `--cpu native` の両方で結果一致。
- WASM E2E: runtime dispatch symbol が出ないことを確認する。
- Trap test: 範囲外 / allocation / division などが baseline と variant で同じ。
- Float kernel を入れる場合は NaN / signed zero / no FMA を検査する。
- benchmark: matched workload、same process、rotating order、no speed threshold。

## ドキュメント

- `docs/architecture.md` の性能設計に runtime dispatch の actual support を追加する。
- `docs/language.md` の backend 節に、言語意味ではなく内部実装選択であることを書く。
- `README.md` の `--cpu` 説明に runtime dispatch との違いを書く。
- `docs/benchmarks.md` に `run-dispatch.mjs` と variant force の記録方法を書く。

## 受け入れ条件

- [ ] `tsuzuri_cpu_features` は weak/hidden で、未知 platform は 0 を返す。
- [ ] ifunc に依存しない。
- [ ] driver は必要時だけ `src/runtime/cpu.c` を compile/link する。
- [ ] dispatcher は atomic cached function pointer を使う。
- [ ] variant function にだけ `target-features` attribute が付く。
- [ ] unsupported feature variant は自動選択されない。
- [ ] force hook で各 variant の正しさを検査できる。
- [ ] variant 間で bit-identical。
- [ ] wasm32 には影響しない。
- [ ] docs が actual と planned を分けている。

## 落とし穴

- macOS で ifunc を使うと portability を失う。
- AVX2 は CPUID だけでなく OSXSAVE / XCR0 が必要。
- `--cpu native` の artifact は portable ではないので、runtime dispatch の portable baseline と混同しない。
- float kernel に FMA や reassociation を入れると bit-identical でなくなる。
- feature detection failure を silent success の高速版選択にしてはならない。
- user function を勝手に multiversioning すると code size と semantics risk が大きい。

## 対象外

- user annotation による target_feature。
- 全関数の自動 multiversioning。
- Windows detection。
- GPU / WASM feature dispatch。
- SVE/SVE2 の本実装。
- speed threshold CI。

## 未決事項

- test hook を env var にするか compile-time macro にするか。
  - 既定案: env var は便利だが本番 API に見せず、tests だけで使う。
- x86 cpuid を inline asm にするか compiler builtin にするか。
  - 既定案: inline asm と builtin の小さい compatibility wrapper を `cpu.c` に閉じ込める。
- `--cpu native` 時に dispatch を有効にするか。
  - 既定案: 有効にしてよいが portable ではないことを docs に明記する。
- 台帳の見直し提案: なし。D-14, D-17 と整合する。
