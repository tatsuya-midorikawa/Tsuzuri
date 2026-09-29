# PB05: デバッグビルド用の高速バックエンド

| 項目 | 内容 |
| --- | --- |
| ID | PB05 |
| 分類 | ビルド速度 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | PB01, PB02 |
| 関連 | PB03, PB07, G08, G04 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（第二のバックエンドとして Cranelift を採る。チケット全体）、D2（入力を Tsuzuri が出す `-O0` の LLVM IR テキストにする）、D3（Cranelift 系 crate の追加と cargo feature `fast-backend`）。D12（既定のバックエンドの変更）は本チケットでは行わない |
| 手本にする既存実装 | LLVM 以外の出力と対応外の早期拒否: `src/gpu.rs` の `extract_kernel`・`GpuKernel::wgsl`・`wgsl_type`。外部ツールの起動と一時ディレクトリ: `src/driver.rs` の `build_complete`・`tool`・`TemporaryDirectory`。CLI の値付きオプション: `src/main.rs` の `--cpu` の分岐と `BuildOptions::cpu`。cache の鍵: `src/cache.rs` の `build_key`。trap の E2E: `tests/e2e.mjs` の `traps`（SIGILL・SIGTRAP を `_Exit(86)` にする C harness） |
| 主な影響ファイル | `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/main.rs`, `src/driver.rs`, `src/cache.rs`, `src/backend/mod.rs`・`ir.rs`・`layout.rs`・`lower.rs`（新規）, `tests/fast_backend.rs`（新規）, `tests/fast_backend.mjs`（新規）, `docs/architecture.md`, `docs/language.md`, `docs/benchmarks.md`, `README.md`, `_perfs/README.md` |
| 計測対象 | PX03 の合成プロジェクト（`benchmarks/build/generate.mjs`、PX03 で新規）の native `-O0` の variant `build`・`build-edit-body`・`build-edit-signature`、`examples/hello`・`examples/computations` の `-O0` ビルド、生成コードの実行時間（`benchmarks/run-control.mjs` の `-O0` 相当） |

## 目的

デバッグビルド（native `-O0`）のコード生成で Clang を使わず、Cranelift で object を作って、編集→ビルド→実行の待ち時間を縮める。
Go や Zig の自前のバックエンド、Rust の Cranelift によるデバッグビルドと同じ考え方である。

- 入力は型付き IR ではなく、Tsuzuri が今も出している `-O0` の LLVM IR テキスト（連結されたランタイムの `.ll` を含む）とする（D2）。
  所有・解放・trap・評価順序の lowering は `src/llvm*.rs` の 1 か所に残し、新しいバックエンドは LLVM の命令を一つずつ Cranelift IR に写すだけにする。
  意味の一致を構成で保ち、差分テストは翻訳の誤りだけを探せばよい。
- Phase 1 は明示の `--backend fast`（新規）だけで選ぶ。自動では選ばず、既定も変えない（D12）。対応しない組み合わせと命令は
  E2000・E2002 で拒否し、黙って違う意味のコードを作らない。
- 実装者は Phase 1 だけを実装する。Phase 2（DWARF の行情報・`--trap-info`・`test` コマンド）と直接の WASM 生成（D13）は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D1・D2・D3 が承認済みであること。承認前はどの手順にも着手しない（crate を追加しない試作もしない）。
- PB01 が done であること。PB05 は PB01 の `numeric`・`native-c` の事前ビルド object を使う（下記インターフェース）。
  確認: `grep -n "PB01\|PB02\|PX03" _perfs/README.md` の状態欄と、`grep -n "external_numeric\|fn runtime_object" src/llvm.rs src/driver.rs` が両方を見つけること。
- PB02 が done であること（依存欄のとおり。PB02 は目標 G3 の合算にだけ関わり、コードの前提ではない）。
- PX03 が done であること。手順 8 の計測は PX03 の生成器・variant・段別時間を使う（依存欄にはないが、計測の前提）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 前提とする他チケットのインターフェース

- PB01: `llvm::EmitOptions::external_numeric: bool`（true なら `numeric.ll` を連結せず入口の宣言だけを出す）、
  `driver::runtime_object(cache, unit, source, input, args, directory, messages) -> Result<PathBuf, Diagnostic>`（unit は `"numeric"`・`"native-c"`）。
  PB05 の経路は常に `external_numeric = true` で IR を作り、この 2 つの object を link する（D7）。名前が違っていたら PB01 の実装に合わせ、意味が違えば止める。
- PX03: `TSUZURI_TIME_PASSES=1` で段別時間を出す仕組みと、`tool.` で始まる外部ツールの段名。PB05 は段 `backend.parse`・`backend.lower`・
  `backend.object`（新規）を足し、link の Clang は既存どおり `tool.` の段に入れる。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `rust-version = "1.85"` で build できる Cranelift の版がない（D3 の選び方で見つからない）。`rust-version` を上げたくなった。
2. 追加する crate の依存木に、C/C++ を compile する `build.rs`、ネットワークを使う `build.rs`、既定で有効な `std` 以外の OS 依存 feature がある。
3. 手順 2 の棚卸しで、D6 の対応表にない命令・intrinsic・型・属性が出た。とくに `musttail`・`invoke`・`landingpad`・`va_arg`・
   `llvm.fmuladd`・`half`・`fp128`・`x86_fp80`・128 bit を超える vector、宣言だけの関数の境界での `byval`・`sret`・`inreg`。
4. 宣言だけの関数（C ランタイム、libc、PB01 の object、compiler-rt）の呼び出しに、集約値・`i128`・vector の引数か戻り値があり、
   Clang と同じ ABI になることを手順 6 のテストで確かめられない。
5. `--backend fast` の結果が LLVM 経路と一つでも異なる（stdout、stderr、終了コード、trap、`live`）。期待値や比較の条件を緩めない。
6. `--backend fast` を指定しないビルドの IR・object・実行ファイルの byte 列が HEAD（PB01 適用後）から変わる。
7. 自分の crate に `unsafe` が要る。`unsafe_code = "forbid"` を緩めない。
8. Cranelift の object を Clang で link すると、linker の warning かエラーが出て、Cranelift・`object` の設定で消せない。
9. 手順 8 の計測で、合成 1,000 モジュールの `build-edit-body` の中央値が LLVM 経路以上になった。CI の合否ではなく、Phase 1 を続けるかの人間の判断に回す。
10. 既存テストの期待値を変える必要がある。

## 現状と計測（HEAD `f8dc655`）

### ビルドの経路（コードで確認）

- `src/driver.rs` の `build` → `build_complete` は、`llvm::emit_with_options`（`EmitOptions { entry, wasm, debug_output }`）で 1 本の IR 文字列を作り、
  `TemporaryDirectory` の `module.ll` に書いて `tool("TSUZURI_CLANG", "clang")` に `-x ir` で渡す。WASM の link は `tool("TSUZURI_WASM_LD", "wasm-ld")`、
  macOS の `-g` は `tool("TSUZURI_DSYMUTIL", "dsymutil")`。
- `BuildOptions` は `target`・`emit`・`optimization`・`cpu`・`debug_output`・`trap_info`・`debug_info`・`wasm_simd`・`wasm_threads`・`cache`。
  `src/main.rs` は `optimization` を `test` だけ 0、ほかは 3 を既定にする。
- CLI は `--target native|wasm32`、`--emit exe|object|llvm|header|wasm|wgsl`、`-O0`〜`-O3`、`--cpu generic|native`、`-g`/`--debug-info`、
  `--trap-info`、`--debug-output`、`--wasm-feature simd128|threads`、`--no-cache`。`run` は `--target wasm32` を拒否する
  （`src/main.rs` のテスト `parse(&["run", "Main.tz", "--target", "wasm32"]).is_err()`）。編集→ビルド→実行の繰り返しは native だけで起きる。
- ランタイム: `src/llvm.rs` は使う記号があるときだけ `include_str!("runtime/<name>.ll")` を IR の末尾に連結する（`string`・`heap-native`・
  `heap-wasm`・`heap-wasm-threads`・`closure`・`recursive`・`numeric`・`math`・`character`・`console`・`debug`・`display`・`utf8string`・`task-wasm`・`wasm`）。
  手書きの `.ll` は `internal`（例: `string.ll` の `define internal %tz.string @tz.string.new`）で、`%tz.*` の型定義を利用者 IR のヘッダーに頼るので、
  単独では事前 compile できない。`numeric.ll`（`define` 21 個）と `math.ll`（61 個）は Clang の生成物。C の `task.c`・`cpu.c`・`io.c` は
  `src/driver.rs` が別に Clang で compile する。
- LLVM 以外の出力は `src/gpu.rs`（530 行）の WGSL だけで、`wgsl_type` などが対応外の形を診断で拒否する。
- LLVM への lowering は `src/llvm*.rs` の合計 10,888 行（うち `src/llvm.rs` 5,375 行）。型付き IR から第二のバックエンドを作ると、
  この量の所有・解放・trap・評価順序の規則を二重に持つ（D2 の理由）。
- 依存 crate は `rustc_apfloat`・`serde_json`（Windows だけ `same-file`）。`[lints.rust] unsafe_code = "forbid"`、`rust-version = "1.85"`、
  `edition = "2024"`、release は `lto = "thin"`・`codegen-units = 1`。
- `src/cache.rs` の `build_key` は `BuildOptions` と IR から鍵を作る。バックエンドを区別する入力はまだない。
- `tests/e2e.mjs` の trap の C harness は SIGILL と SIGTRAP の両方を `_Exit(86)` にする。native の trap 命令の種類（`brk`・`ud2`・`udf`）には依存しない。

### 計測済みの事実

`_perfs/README.md`（2026-09-29、Apple M1 Max、`--no-cache`、各 1 回）より。

| プロジェクト | 行数 | `check` | `--emit llvm` | 実行ファイル `-O0` | 実行ファイル `-O3` | RSS |
| --- | --- | --- | --- | --- | --- | --- |
| 合成 1,000 モジュール | 124,003 | 2.12 s | 2.58 s | 約 6.2 s | 約 10.1 s | 1.12 GB |

- Clang の処理は `-O0` で 3.70 s、`-O3` で 7.62 s。IR は約 29 MB。`-O0` で PB05 が削れる上限は Clang の 3.70 s から、
  翻訳・Cranelift・link の時間を引いた値である（IR の生成 2.58 s は減らない）。
- 小さなプログラムは Clang の起動とランタイムの compile が支配的で、その大半は PB01 の対象。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PB05
target/release/tsuzuri build examples/hello --emit llvm -O0 -o /tmp/tz-work-PB05/hello.ll
wc -c /tmp/tz-work-PB05/hello.ll
grep -ohE '^ +(%[-A-Za-z0-9._]+ = )?[a-z]+' /tmp/tz-work-PB05/hello.ll | sed -E 's/^ +(%[^ ]+ = )?//' | sort | uniq -c | sort -rn
grep -ohE '@llvm\.[a-z0-9_.]+' /tmp/tz-work-PB05/hello.ll | sort -u
/usr/bin/time -p target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-work-PB05/hello-O0
```

結果: IR は 3,126 bytes。命令は 14 種（`ret` 13、`call` 8、`br` 8、`load` 5、`insertvalue` 5、`extractvalue` 4、`icmp` 3、`store` 2、`phi` 2、
`alloca` 2、`sub` 1、`ptrtoint` 1、`inttoptr` 1、`add` 1）、intrinsic は `@llvm.trap` だけ。ビルドは `real 0.12`（1 回）。
調査メモにあった「hello の IR が 464 KB」は `examples/hello` では再現しない（`numeric.ll` は浮動小数点の表示などを使うときだけ連結される）。
この 2 つの `grep` が手順 2 の棚卸しの原型である。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で、同じ commit・同じ binary（`--features fast-backend` で build）の `--backend llvm` と `--backend fast` を比べる。

- G1: native `-O0` の編集→ビルド→実行の待ち時間（PX03 の `build-edit-body`）を短くする。
- G2: native `-O0` のコード生成の時間を、Clang の 3.70 s（合成 1,000 モジュール）より短くする。
- G3（長期。Phase 1 の受け入れ条件ではない）: PB02・PB04 と合わせて 124K 行の `-O0` を 1 s 以下にし、同じ規模の Go のビルド（PX03 Phase 2）より短くする。
- G4: 生成コードの実行時間の悪化を記録する。デバッグビルドとして許容できるかは人間が判断する（上限は決めない）。
- G5: 追加の費用（`tsuzuri` の大きさ、`cargo build --release --locked` の時間、依存 crate の数）を記録する。

| 指標 | 単位・統計 | 対象 | 期待（未検証。計測で確かめる） |
| --- | --- | --- | --- |
| M1 ビルド全体 | s（9 標本の中央値・最小・最大） | 合成 200・1,000 モジュールの `build`（`--no-cache`）と `build-edit-body`・`build-edit-signature`（cache あり）、`examples/hello` の `build` | fast が短い。上限の計算値は Clang の compile 分（1,000 モジュールで最大 3.70 s）の削減 |
| M2 段別 | s（中央値） | M1 の各標本の `TSUZURI_TIME_PASSES` の段。LLVM は `tool.clang`、fast は `backend.parse`・`backend.lower`・`backend.object` と link の `tool.clang` | どの段が支配的かを記録する |
| M3 生成コードの実行時間 | ms（9 回の中央値・最小・最大） | `examples/computations` と合成 1,000 モジュールの実行ファイル（stdin `12345\n`）を `-O0` で両経路 | fast が遅くなりうる。比を記録するだけ |
| M4 大きさ | bytes（固定値） | 合成 1,000 モジュールの `module.o` と実行ファイル | 記録するだけ |
| M5 費用 | bytes・s・個（各 1 回） | `target/release/tsuzuri` の大きさ、`cargo build --release --locked` の時間、`cargo tree -e normal --prefix none` の行数。feature の有無で | 記録するだけ |

## 変えてはいけない意味

判定の基準は「同じ IR を Clang `-O0` で compile した結果と観測できる差がない」こと。LLVM の poison・UB を具体的な値や trap にするのは、
Tsuzuri の IR がその経路に到達しない場合に限って許す（D6 の各行に理由を書く）。

- 整数: 幅ごとの wrap、shift、比較、`i128` の演算。`i128` の除算・剰余は LLVM と同じ compiler-rt の `__divti3`・`__udivti3`・`__modti3`・`__umodti3` を呼ぶ。
- 浮動小数点: binary32・binary64 の IEEE 754 の丸め（最近接偶数）、NaN の payload、符号付きゼロ。NaN の正規化をしない
  （Cranelift の `enable_nan_canonicalization` は false のまま）。FMA は IR の `llvm.fma` だけで、`fmul` と `fadd` を融合しない。
  fast-math の flag（`fast`・`nnan`・`ninf`・`nsz`・`arcp`・`contract`・`afn`・`reassoc`）が IR に現れたら E2002 で止める。
- 型変換: 整数どうしはビットを保つ。浮動小数点→整数は `llvm.fptosi.sat`・`llvm.fptoui.sat` の飽和（NaN は 0）。
- trap: `@llvm.trap` と `unreachable` の位置と、プロセスが受けるシグナル（D6。aarch64 の LLVM は SIGTRAP、x86_64 は SIGILL）。
  `run` の E2005 の表示と終了コードも LLVM 経路と同じにする。
- 評価順序・所有権・借用・解放: IR の命令の順をそのまま写す。命令の並べ替え、削除（`llvm.assume` などの D6 の列を除く）、重複をしない。
  すべての E2E で `live == 0`。
- ABI: 宣言だけの関数（C ランタイム、libc、PB01 の object）の呼び出しと、外から呼ばれる定義（`main`、`tz_*`）の引数・戻り値は、Clang と同じ C ABI。
- 決定性: 同じ IR から byte 単位で同じ `module.o` を作る。記号・data の順は IR に現れた順で、`HashMap` の反復順を使わない。
- LLVM 経路と WASM: `--backend fast` を指定しないビルドの IR・object・実行ファイルは byte 単位で変わらない。WASM の既定の import は増えない。
- 資源: 既存の資源上限（E1017）、`MAX_VALUE_BYTES` と診断を変えない。stack の使い方は変わりうる（落とし穴）。

## 設計

### 方式の比較（D1・D2）

| 観点 | (A) Cranelift ← `-O0` の LLVM IR テキスト（採用） | (B) Cranelift ← 型付き IR | (C) 直接 WASM ← 型付き IR | (D) 直接 WASM ← LLVM IR テキスト | (E) LLVM のまま（PB01・PB03・PB07） |
| --- | --- | --- | --- | --- | --- |
| 対応 target | native（aarch64・x86_64。Cranelift の公開情報、未検証） | 同左 | wasm32 だけ | wasm32 だけ | すべて |
| `run` の繰り返しに効くか | 効く（`run` は native だけ。確認済み） | 効く | 効かない（`run` は wasm32 を拒否。確認済み） | 効かない | 効く |
| 新しい crate | cranelift 系（D3）。依存木と大きさは未計測（手順 3 で計測） | 同左 | なし | なし | なし |
| `unsafe` | 依存 crate の内部だけ。自分の crate の `forbid` は保つ | 同左 | なし | なし | なし |
| ランタイム | 手書きの `.ll`・`math.ll` は module と一緒に翻訳。`numeric`・C は PB01 の object | lowering をもう一組書く | `.ll` は `internal` で `%tz.*` に依存し、単独で wasm32 object にできない（確認済み） | 再配置可能な wasm object（`linking`・`reloc.*` section）の生成と、ランタイムの linkage の変更が要る | 変更なし |
| 意味の一致 | 所有・trap・順序の lowering は 1 か所。命令の翻訳の誤りだけが危険 | 10,888 行の lowering を二重に持つ | 同左 | (A) と同じ利点 | 変化なし |
| デバッグ情報 | Phase 1 はなし。Phase 2 で行情報（`gimli`、要承認） | 同左 | WASM の DWARF は別途 | 同左 | あり |
| 既知の時間 | Clang `-O0` の 3.70 s を置き換える（計測済みは置き換え前だけ）。Cranelift の速さは未検証 | 同左 | WASM の `-O0` は未計測 | 同左 | Clang 3.70 s。PB07 の並列化の効果は未計測 |

(E) は PB05 と排他ではない。手順 8 で (A) が (E) より速いことを実測してから Phase 1 を完了とする（停止条件 9）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | オプションの解析（`--cpu` の分岐の隣）、`BuildOptions` を作る箇所 | `--backend llvm\|fast`（新規）。重複・未知の値・組み合わせを E2000（診断の表）。`--backend fast` で `-O` 省略なら 0 |
| 駆動 | `src/driver.rs` | `Backend`（新規）、`BuildOptions::backend`（新規）、`Default` | 既定は `Backend::Llvm` |
| 駆動 | `src/driver.rs` | `build_complete` | `Backend::Fast` なら `external_numeric = true` で IR を作り、`clang -x ir` の代わりに `backend::compile` で `module.o` を作る。link は `dwarf_sidecar` の経路と同じ「object を作ってから `clang` で link」の形で、`-lm`・`runtime_object`・`-pthread` を同じ条件で付ける。`--emit object` は既存の `-r -nostdlib` の結合をそのまま使う |
| 駆動 | `src/driver.rs` | 環境の検査（新規の小関数 `fast_backend_host`） | Windows と aarch64・x86_64 以外の host は E2000。feature なしの build では E2000 |
| cache | `src/cache.rs` | `build_key` | 変更なし。`options` の `Debug` 表示に `backend` が入るので鍵は自動で分かれる（手順 7 の E2E で確かめる） |
| crate | `src/lib.rs` | module 宣言 | `#[cfg(feature = "fast-backend")] pub mod backend;` |
| 翻訳 | `src/backend/ir.rs`（新規） | `parse`、`Module`・`Function`・`Inst`・`Ty`・`Constant` | Tsuzuri と runtime の IR の部分集合の構文解析 |
| 翻訳 | `src/backend/layout.rs`（新規） | `size_align` | host の既定 DataLayout の大きさ・align・field の offset |
| 翻訳 | `src/backend/lower.rs`（新規） | `lower_function` | D6 の表のとおり Cranelift IR を作る |
| 翻訳 | `src/backend/mod.rs`（新規） | `compile` | 記号の宣言、data、関数の定義、object の書き出し、段別時間 |
| テスト | `tests/fast_backend.rs`・`tests/fast_backend.mjs`（新規） | — | テスト計画 |

### データ構造

```rust
// src/driver.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend { Llvm, Fast } // （新規）
pub struct BuildOptions { /* 既存の field */ pub backend: Backend } // （新規 field）

// src/backend/mod.rs（新規）。triple は target_lexicon::Triple::host()
pub fn compile(ir: &str, object: &Path) -> Result<(), Diagnostic>;

// src/backend/ir.rs（新規）。名前は IR の文字列をそのまま持つ（`@` と `%` は除く）
pub enum Ty { Void, Int(u32), F32, F64, Ptr, Named(Box<str>), Struct { fields: Vec<Ty>, packed: bool }, Array(u64, Box<Ty>), Vector(u32, Box<Ty>) }
pub struct Module { pub types: Vec<(Box<str>, Ty)>, pub globals: Vec<Global>, pub declarations: Vec<Declaration>, pub functions: Vec<Function> }
pub struct Function { pub name: Box<str>, pub linkage: Linkage, pub params: Vec<(Ty, Box<str>)>, pub ret: Ty, pub blocks: Vec<Block> }
pub struct Block { pub label: Box<str>, pub insts: Vec<Inst> }
pub enum Linkage { External, Internal, Private, Hidden } // これ以外は E2002
// Inst は D6 の表の命令ごとに 1 variant。巨大な field は Box にする
```

- 集約値（struct・array）は SSA の中で「Cranelift の値の列」に平らにする（D8）。`insertvalue`・`extractvalue` は列の置き換えと取り出しで、命令を出さない。
- 集約の `load`・`store` は `layout.rs` の offset で field ごとの load・store にする。`i1` は Cranelift の `i8`（0 か 1）で表し、メモリでも 1 byte。
- 内部の関数（定義が同じ module にあり外から呼ばれない）は平らにした列をそのまま引数・戻り値にする。外との境界は D8。

### 翻訳規則（D6 の要約）

| LLVM | Cranelift | 理由・条件 |
| --- | --- | --- |
| `add`・`sub`・`mul`（`nsw`・`nuw` は捨てる）、`and`・`or`・`xor` | `iadd`・`isub`・`imul`・`band`・`bor`・`bxor` | wrap。poison の flag を捨てるのは refinement |
| `sdiv`・`udiv`・`srem`・`urem`（8〜64 bit） | 同名の命令 | Cranelift は 0 除算と `MIN / -1` で trap。LLVM では UB で、Tsuzuri の IR は事前の分岐で除外する（手順 6 で確認） |
| 同上（`i128`） | `__divti3` など compiler-rt の呼び出し | LLVM と同じ関数 |
| `shl`・`lshr`・`ashr` | `ishl`・`ushr`・`sshr` | 幅以上の shift は LLVM で poison。Tsuzuri の IR が事前に検査することを手順 2 で確かめる |
| `icmp`・`fcmp` | `icmp`・`fcmp`（述語を一対一。`fcmp true`・`false` は定数） | `one`・`ueq`・`ord`・`uno` も一対一の条件がある |
| `fadd`・`fsub`・`fmul`・`fdiv`・`fneg` | 同名の命令 | fast-math の flag は E2002 |
| `frem` | `fmod`・`fmodf` の呼び出し | LLVM も libm を呼ぶ |
| `zext`・`sext`・`trunc`・`fpext`・`fptrunc`・`sitofp`・`uitofp` | `uextend`・`sextend`・`ireduce`・`fpromote`・`fdemote`・`fcvt_from_sint`・`fcvt_from_uint` | — |
| `fptosi`・`fptoui`・`llvm.fpto[su]i.sat.*` | `fcvt_to_sint_sat`・`fcvt_to_uint_sat` | 範囲内は同じ値。範囲外は LLVM で poison なので飽和でよい。`fcvt_to_sint`（trap する）は使わない |
| `bitcast`・`ptrtoint`・`inttoptr`・`freeze` | `bitcast`・値の受け渡し | `freeze` は Cranelift に poison がないので恒等 |
| `select`・`phi`・`br`・`switch`・`ret` | `select`・block の引数・`brif`/`jump`・`cranelift_frontend::Switch`・`return` | — |
| `alloca` | `StackSlotData`（大きさと align は `layout.rs`） | — |
| `load`・`store`・`getelementptr` | `load`・`store`・`iadd`/`imul_imm` | `volatile`・`atomic` の load/store は E2002（手順 2 で出なければ実装しない） |
| `call`・`tail call` | `call`・`call_indirect` | `tail` は hint なので捨てる。`musttail` は停止条件 3 |
| `@llvm.trap`・`unreachable` | aarch64: `debugtrap` の後に `trap`、x86_64: `trap` | シグナルを LLVM と合わせる（SIGTRAP・SIGILL）。手順 6 で両経路の終了シグナルを比べる |
| `llvm.memcpy`・`llvm.memset` | `call_memcpy`・`call_memset`（libc） | `volatile` 引数が true なら E2002 |
| `llvm.fabs`・`llvm.copysign`・`llvm.sqrt`・`llvm.fma`・`llvm.abs`・`llvm.smax`・`llvm.smin`・`llvm.ctlz`・`llvm.scmp`・`llvm.is.fpclass` | `fabs`・`fcopysign`・`sqrt`・`fma`・`iabs`・`smax`・`smin`・`clz`・比較と減算・ビット検査 | 棚卸しで現れた型と flag の組だけを実装する |
| `llvm.assume`・`llvm.lifetime.*`・`llvm.experimental.noalias.scope.decl`・`!llvm.loop` などの metadata | 捨てる | 最適化の hint だけ |
| 128 bit 以下の vector | Cranelift の vector 型（`I32X4` など） | 128 bit を超える vector は E2002 |
| それ以外 | E2002 | 停止条件 3 |

### 生成コードとランタイム

- `backend::compile` は `cranelift_object::ObjectModule` で host の object（Mach-O・ELF）を 1 つ書く。`opt_level = "none"`、
  `enable_verifier` は debug build だけ true。関数の linkage は `Internal`・`Private` → `Local`、`External` → `Export`、`Hidden` → `Hidden`、宣言 → `Import`。
- data は `private`・`internal` の定数（文字列、集約の初期値、`zeroinitializer`）を `DataDescription` で出す。定数式の `getelementptr`・`ptrtoint` は
  `layout.rs` で offset を計算し、記号への参照は再配置で書く。
- ランタイム（D7）: 手書きの `.ll` と `math.ll` は IR に連結されたまま一緒に翻訳する。`numeric` は PB01 の `external_numeric` で宣言だけにし、
  PB01 の `runtime_object("numeric", …)` を link する。C ランタイムは `runtime_object("native-c", …)`。link は既存と同じ `tool("TSUZURI_CLANG", "clang")`。
- 段別時間は PX01 の仕組みで `backend.parse`・`backend.lower`・`backend.object` を記録する。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E2000 | `--backend` の値が不正・重複 | `backend must be 'llvm' or 'fast'` / `backend specified more than once` | なし（CLI） |
| E2000 | `build`・`run` 以外に `--backend` | `--backend applies only to build and run` | なし |
| E2000 | `--backend fast` と `-O1`〜`-O3` | `--backend fast supports only -O0; remove -O{n} or use --backend llvm` | なし |
| E2000 | `--backend fast` と `--target wasm32`・`--wasm-feature` | `--backend fast supports only the native target; use --backend llvm for wasm32` | なし |
| E2000 | `--backend fast` と `--emit llvm\|header\|wasm\|wgsl` | `--backend fast produces only exe and object outputs; use --backend llvm for --emit {kind}` | なし |
| E2000 | `--backend fast` と `-g`・`--trap-info`・`--cpu native` | `--backend fast does not support {option} yet; remove it or use --backend llvm` | なし |
| E2000 | host が Windows か aarch64・x86_64 以外 | `--backend fast is not available on {os}-{arch}; use --backend llvm` | なし |
| E2000 | feature なしで build された `tsuzuri` | `this tsuzuri was built without the fast backend; rebuild with cargo build --release --features fast-backend or use --backend llvm` | なし |
| E2002 | 翻訳できない命令・型・属性・linkage | `the fast backend cannot translate {construct} in @{function}; build with --backend llvm and report this program` | なし |

### Phase 分割

- Phase 1（本チケットで実装）: macOS・Linux の aarch64・x86_64、`build --emit exe|object` と `run`、`-O0`、`-g`・`--trap-info`・`--cpu native` なし。
  cargo feature `fast-backend` の中だけ。差分 suite `tests/fast_backend.mjs` と計測まで。
- Phase 2（設計方針。人間が求めた場合だけ）: DWARF の行情報（Cranelift の source location と `gimli`、crate の追加は要承認）、`--trap-info`
  （trap 記号の表を Cranelift の trap 位置から作る）、`test` コマンド、Windows（G10 の後）。
- Phase 3（D13）: 直接の WASM 生成。WASM のデバッグビルドの待ち時間が問題になった場合だけ。
- 既定の選択（D12）: 本チケットの外。差分 suite が一定期間一致した記録を添えて、別の承認で判断する。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る（GUIDE §3.1）。
feature 付きのテストは `--features fast-backend` を付けないと 0 件になる。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、比較用の binary を保存する。「再現」のコマンドを実行して結果が同じことを確かめる。
- 確認: 次がすべて成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
mkdir -p target/perf/PB05 && cp target/release/tsuzuri target/perf/PB05/baseline-tsuzuri
{ git rev-parse HEAD; sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -n 1; rustc --version; node --version; } > target/perf/PB05/env.txt
```

### 手順 2: IR の棚卸し（crate なし）

- 変更: `tests/fast_backend.mjs`（新規）の `inventory` mode。
- 内容: `tests/fixtures/*/` と `examples/*/` を一時ディレクトリに写し（E03 の再帰探索の落とし穴）、`build <dir> --emit llvm -O0` で IR を出す。
  IR から `src/runtime/numeric.ll` の本文を文字列置換で除き（fast 経路は PB01 の object を使う）、命令（「再現」の正規表現）、`@llvm.*` の名前、
  型（`half`・`fp128`・`<N x T>`・奇数幅の整数）、fast-math の flag、宣言の属性（`signext`・`zeroext`・`sret`・`byval`・`inreg`）、linkage を数えて、
  名前順の表を `target/perf/PB05/inventory.txt` に書く。build に失敗した dir は名前と stderr の 1 行目を表に書く。
- 確認: `node tests/fast_backend.mjs target/release/tsuzuri inventory` が成功する。表のすべての行が D6 の表にある。ない行があれば停止条件 3。

### 手順 3: crate と feature

- 変更: `Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`src/backend/mod.rs`（新規、`compile` は常に E2002 を返す仮実装）。
- 内容: D3 のとおり optional な依存と `[features] fast-backend = [...]` を足す。既定の feature は空のまま。
- 確認: `cargo build --release --features fast-backend` で `Cargo.lock` を更新した後、`cargo build --release --locked`・`cargo build --release --locked --features fast-backend`・
  `cargo test --locked` が成功する。M5（大きさ、`time cargo build --release --locked --features fast-backend`、`cargo tree -e normal --prefix none --features fast-backend | wc -l`）を
  `target/perf/PB05/cost.txt` に書く。停止条件 1・2 を確かめる。

### 手順 4: CLI・`Backend`・E2000

- 変更: `src/main.rs`、`src/driver.rs` の `Backend`・`BuildOptions::backend`・`build_complete`・`fast_backend_host`（新規）、`tests/fast_backend.mjs` の既定 mode。
- 内容: 「診断」の E2000 をすべて実装する。`Backend::Fast` は `external_numeric = true` で IR を作り、`backend::compile` を呼ぶ（まだ E2002）。
  `src/main.rs` の `optimization` の既定は `action == Action::Test || backend == Backend::Fast` なら 0。
- 確認: `cargo test --locked --bin tsuzuri rejects_fast_backend_combinations` が `1 passed`。`cargo test --locked` が成功する。
  `target/release/tsuzuri build examples/hello --backend fast -O3` が E2000 で終わる。`--backend` なしの `examples/hello` の IR が手順 1 と byte 単位で同じ。

### 手順 5: 構文解析と layout

- 変更: `src/backend/ir.rs`・`layout.rs`（新規）、`tests/fast_backend.rs`（新規）。
- 内容: 手順 2 の表の構文をすべて読む。metadata（`!N = ...`、`, !llvm.loop !N`）、属性 group（`attributes #N`）、無視してよい属性
  （`noundef`・`nonnull`・`readonly`・`writeonly`・`noalias`・`local_unnamed_addr`・`unnamed_addr`・`nounwind`・`#N`）は読み飛ばす。
  意味を持つ属性（`signext`・`zeroext`・`sret`・`byval`・`inreg`・`volatile`・`atomic`）は保持する。浮動小数点定数は 16 進（`0x...`）と 10 進の両方を bit 単位で読む。
- 確認: `cargo test --locked --features fast-backend --test fast_backend` で `parses_emitted_ir_for_fixtures`・`layout_matches_clang` が成功する。

### 手順 6: 関数本体の翻訳

- 変更: `src/backend/lower.rs`（新規）、`src/backend/mod.rs`。
- 内容: D6 の表の順に、scalar → 制御 → memory・集約 → 呼び出し → intrinsic → trap を実装する。除算・shift・`fptosi` の各行について、
  Tsuzuri の IR が LLVM の UB に到達しないこと（検査の分岐が前にあること）を `src/llvm.rs` の生成箇所で確かめ、`lower.rs` の該当 arm に 1 行の注記を書く。
- 確認: `target/release/tsuzuri run examples/hello --backend fast` の stdout が `--backend llvm -O0` と一致する。`rejects_unsupported_constructs_with_e2002`・
  `object_is_deterministic` が成功する。trap の fixture（テスト計画）で両経路の終了シグナルと E2005 のメッセージが一致する。

### 手順 7: 境界の ABI・data・ランタイム

- 変更: `src/backend/mod.rs`・`lower.rs`、`src/driver.rs` の link。
- 内容: 宣言だけの関数と外から呼ばれる定義（`main`・`tz_*`）の signature を C ABI に合わせる（D8）。data と定数式、PB01 の 2 つの object の link、
  `--emit object` の `-r -nostdlib` の結合。
- 確認: `node tests/fast_backend.mjs target/release/tsuzuri` が成功する。`lowers_every_runtime_ll` が成功する。停止条件 4・8 を確かめる。

### 手順 8: 計測の関門

- 変更: なし。
- 内容: 「計測手順」の M1 を合成 1,000 モジュールの `build-edit-body` だけで測る。
- 確認: 中央値を `target/perf/PB05/gate.txt` に書く。fast が LLVM 以上なら停止条件 9 で止めて報告する。

### 手順 9: 差分 suite

- 変更: `tests/fast_backend.mjs` の `parity` mode。翻訳の誤りの修正。
- 内容: D11 の wrapper で「テスト計画」の suite を実行する。失敗は翻訳の誤りとして直す。期待値や suite は変えない（停止条件 5）。
- 確認: `npx --yes --package=node@24 node tests/fast_backend.mjs target/release/tsuzuri parity` が成功し、suite ごとに fast のビルドが 1 回以上あったと表示する。

### 手順 10: 生成コードの確認

- 変更: なし。
- 確認: 「生成コードの確認」のコマンドと pattern をすべて満たす。

### 手順 11: 計測・文書・最終確認

- 変更: `docs/benchmarks.md`、`docs/architecture.md`、`docs/language.md`、`README.md`、`_perfs/README.md`。
- 内容: 「計測手順」をすべて行い、「ドキュメント」を更新する。
- 確認: `cargo fmt --check`、`cargo clippy --all-targets --locked --features fast-backend -- -D warnings`、`cargo test --locked`、
  `cargo test --locked --features fast-backend`、README.md の全 E2E（`--features` なしの binary）、手順 9 が成功する。

## 計測手順

GUIDE §14 と PX03 の「計測手順」に従う。生データは PX01 形式の JSON Lines で `target/perf/PB05-<UTC 時刻>/`（コミットしない）に置く。

- binary: `cargo build --release --locked --features fast-backend` の 1 つ。LLVM 経路はそれを直接、fast 経路は D11 の wrapper
  （`node tests/fast_backend.mjs target/release/tsuzuri wrapper <dir>` が作る sh script）を compiler として PX03 の本計測に渡す。
  wrapper の起動は `/bin/sh` の `exec` だけで、Node を挟まない。
- 環境: 手順 1 の `env.txt` と同じ項目を run のディレクトリに書く。電源に接続し、ほかの重い処理を止める。
- M1・M2: PX03 の variant `build`（`--no-cache`、`-O0`）・`build-edit-body`・`build-edit-signature` を合成 200・1,000 モジュールで、warm-up 1 回と 9 標本。
  LLVM と fast を組ごとに交互に実行する。`examples/hello` の `build -O0 --no-cache` も同じ統計で測る。
- M3: 両経路の `-O0` の実行ファイルを 9 回ずつ交互に実行し、中央値・最小・最大を記録する（対象は計測対象の欄）。
- M4: `module.o` と実行ファイルの大きさ。M5: 手順 3 の `cost.txt`。
- 記録: docs/benchmarks.md に PB05 の節を足し、日付、計測機、commit、Clang・rustc・Node・Cranelift の版、M1〜M5 の表、生データの保存先、
  そろえられなかった条件（PB01 の cache の状態など）を書く。計測していない数値を書かない。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
T=/tmp/tz-work-PB05
target/release/tsuzuri build examples/hello -O0 --emit object --backend llvm -o $T/hello-llvm.o
target/release/tsuzuri build examples/hello -O0 --emit object --backend fast -o $T/hello-fast.o
target/release/tsuzuri build examples/hello -O0 --emit object --backend fast -o $T/hello-fast2.o
shasum -a 256 $T/hello-fast.o $T/hello-fast2.o
/opt/homebrew/opt/llvm@21/bin/llvm-nm -g --defined-only $T/hello-llvm.o > $T/defined-llvm.txt
/opt/homebrew/opt/llvm@21/bin/llvm-nm -g --defined-only $T/hello-fast.o > $T/defined-fast.txt
/opt/homebrew/opt/llvm@21/bin/llvm-nm -u $T/hello-fast.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d $T/hello-fast.o | grep -E 'brk|udf|ud2'
```

- 2 つの fast の object の SHA-256 が同じ。
- 外から見える定義の記号名の集合が LLVM 経路と同じ（アドレスの列は除いて比べる）。
- 未定義の記号は LLVM 経路の未定義の記号に、`memcpy`・`memset`・`fmod`・`fmodf`・`__divti3`・`__udivti3`・`__modti3`・`__umodti3` を足した集合に含まれる。
- trap は aarch64 で `brk` の直後に `udf`、x86_64 で `ud2`。LLVM 経路の aarch64 は `brk #0x1`（2026-09-30 に `clang -x ir -O0` で確認。終了コード 133 = SIGTRAP）。
- `tsuzuri build` の stderr が空（linker の warning がない。停止条件 8）。

## テスト計画

### Rust テスト

`tests/fast_backend.rs`（新規、先頭に `#![cfg(feature = "fast-backend")]`）。IR は `tests/control.rs` と同じく `llvm::emit` で作る。

- `parses_emitted_ir_for_fixtures`: 手順 2 で build できた fixture の IR を `ir::parse` が読み、関数・宣言・global の数が IR の `define`・`declare`・`@... =` の行数と一致する。
- `layout_matches_clang`: `i1`・`i8`・`i32`・`i64`・`i128`・`float`・`double`・`ptr`・`%tz.string` 相当の `{ ptr, i64 }`・`{ i32, [2 x i128] }`・`{ i8, <4 x i32> }`・
  `[3 x { i8, i64 }]` について、`@sN = constant i64 ptrtoint (ptr getelementptr (T, ptr null, i32 1) to i64)` と field の offset を並べた IR を
  `clang -x ir -O1 -S -emit-llvm -o -` で定数に畳み、`layout::size_align` と比べる（独立の参照は Clang）。
- `rejects_unsupported_constructs_with_e2002`: `va_arg`、`fadd fast`、`<8 x float>`、`musttail call`、`load volatile`、`linkonce_odr` を含む IR が E2002 になり、
  メッセージが構文と関数名を含む。
- `object_is_deterministic`: 同じ IR を 2 回 `compile` した object が byte 単位で同じ。
- `lowers_every_runtime_ll`: 文字列・関数値・表示・再帰型・Math・Task を使う IR（`string`・`closure`・`display`・`recursive`・`math`・`character`・`utf8string`・`heap-native` が連結される）を
  `compile` が E2002 なしで処理する。
- `src/main.rs` の tests: `rejects_fast_backend_combinations`（新規。診断の表の E2000 の各行と、`test` への `--backend`）、
  `accepts_fast_backend_at_o0`（新規。`build`・`run` と `-O` 省略が `optimization == 0` になる）。

### E2E

`tests/fast_backend.mjs`（新規）。既定 mode は次を確かめる。

- CLI: 診断の表の E2000 の各行が非 0 で終わり、stderr がメッセージを含む。
- `examples/hello`: `run --backend fast` と `run --backend llvm -O0` の stdout・終了コードが同じ。
- trap: `tests/fixtures/fast_backend/trap/Main.tz`（新規。`main` が 0 除算で trap する。式は `tests/e2e.mjs` の `divide` の fixture と同じ形）。
  先に LLVM 経路で `run` が E2005 になることを確かめ、fast の stderr（`program terminated with signal: ...` を含む）と終了コードが一致する。
- 決定性: `--no-cache` の fast の実行ファイルを 2 回作り、SHA-256 が同じ。
- cache: cache ありで llvm → fast → llvm の順に build し、1 回目と 3 回目の成果物が同じ、fast の成果物は異なる。
- 不変: `--backend llvm` と指定なしの `--no-cache -O0` の成果物が byte 単位で同じ。

`parity` mode は D11 の wrapper で次の suite を実行する: `e2e`・`primitives`・`strings`・`tasks`・`computations`・`control`・`numeric_casts`・
`integer_intrinsics`・`display_parse`・`features`・`math`・`io`・`host_imports`・`gpu`。各 suite で fast のビルドが 1 回以上あることを wrapper の log で確かめる。
0 回の suite は一覧から外し、理由を docs/benchmarks.md の PB05 の節に書く。除外: `simd`・`wasm_simd`・`cpu_dispatch`（機械語の pattern を検査する）、
`debug_info`（`-g`）、`cache`（鍵の検査は既定 mode）、`wasm_threads`・`windows`・`lsp_sessions`・`docgen`・`examples`（`-O0` の native ビルドがない、または build しない）。
期待値は各 suite の独立の参照（JavaScript の BigInt、C の harness）のままで、`live == 0`、trap の `_Exit(86)` を含む。

### 既存テストへの影響

なし。`BuildOptions` の `Debug` 表示が変わるので利用者の cache は一度 miss になる（`compiler-binary` の digest でも同じことが起きる）。

### 性能

「計測手順」だけ。CI に速さの合否を入れない。

## ドキュメント

- `docs/architecture.md`: pipeline に `--backend fast` の経路（`-O0` の IR → `src/backend/` → Cranelift の object → Clang で link）と module 表の `src/backend/` を足す。
  「LLVM の C API に結合しない」方針が保たれていることを書く。
- `docs/language.md`: ビルド専用オプション（E2000 の段落）に `--backend llvm|fast` と Phase 1 の制限を足す。
- `docs/benchmarks.md`: PB05 の節（計測手順の記録）。
- `README.md`: `cargo build --release --features fast-backend` と `node tests/fast_backend.mjs` の 2 mode を検証コマンドに足す。
- `_perfs/README.md`: PB05 の状態と計測値。

## 受け入れ条件

- [ ] D1・D2・D3 が承認されている。
- [ ] `--backend fast` が Phase 1 の範囲で動き、範囲外は診断の表のとおり E2000・E2002 になる。
- [ ] `--backend fast` を指定しないビルドの IR・成果物が byte 単位で変わらない。`--features` なしの build と全テストが成功する。
- [ ] `tests/fast_backend.rs` と `tests/fast_backend.mjs` の既定 mode・`parity` mode が成功する。
- [ ] 「生成コードの確認」をすべて満たす。
- [ ] M1〜M5 を計測して docs/benchmarks.md に記録し、計測していない数値を書いていない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- Cranelift の `sdiv`・`udiv`・`srem`・`urem` は 0 除算と `MIN / -1` で trap し、shift 量は幅で mask する。LLVM の UB の経路に Tsuzuri の IR が到達しないことを
  手順 6 で生成箇所ごとに確かめる。到達するなら翻訳で合わせず停止条件 5。
- `fptosi` を `fcvt_to_sint` にすると、Clang の生成した `math.ll` が範囲外の値を先に変換して `select` で捨てる箇所で trap する。必ず `_sat` を使う。
- Apple の aarch64 の C ABI は `i8`・`i16` の引数を呼び出し側で拡張する。`signext`・`zeroext` を `AbiParam::sext`・`uext` に写さないと、C ランタイムが上位 bit のごみを読む。
- Cranelift は 2 語を超える戻り値に暗黙の戻り値領域を使い、LLVM の first-class aggregate の戻り値と ABI が違う。module の中だけなら問題ないが、
  宣言だけの関数と `tz_*` では停止条件 4。
- `@llvm.trap` を Cranelift の `trap` だけにすると aarch64 で SIGILL になり、`run` の E2005 の表示（`signal: 5 (SIGTRAP)`）が変わる。
- IR に `target triple`・`target datalayout` はない（Clang が host の既定を使う）。`layout.rs` は host の既定と一致させ、`layout_matches_clang` で確かめる。
- `!llvm.loop` の metadata は `-g` なしでも `br` に付く（`llvm.loop.unroll.enable`）。読み飛ばしを忘れると構文解析が失敗する。
- PB01 の `external_numeric` の条件は `options.cache` を含むが、fast 経路は常に true にする。`--no-cache` では `runtime_object(None, …)` が毎回 Clang で
  `numeric` を compile するので、M1 の `build`（`--no-cache`）はその時間を含む。PX03 の `build-edit-body` と分けて読む。
- `--emit object` の `-r -nostdlib` の結合は macOS で `-Wl,-keep_private_externs` が要る（GUIDE §11.1 の既知の落とし穴）。Cranelift の `Hidden` の記号で確かめる。
- stack の使い方は Cranelift と LLVM で違う。深い再帰の E2E が fast だけで SIGSEGV になったら停止条件 5（stack の上限を上げない）。
- `Cargo.lock` の更新前は `--locked` が失敗する。feature なしの `cargo test --locked` では `tests/fast_backend.rs` が 0 件になる。
- 重い BigInt の suite は Node 20 で V8 が落ちることがある。`parity` は Node 24 で実行する。

## 対象外

- `-O1` 以上、リリースビルド、LLVM と同等の最適化。
- WASM（D13）、Windows、`-g` の DWARF・`--trap-info`・`test` コマンド（Phase 2）。
- 既定のバックエンドの変更（D12）。REPL の JIT（G13）。LLVM の C API・Rust バインディングへの結合。

## 決定事項

### D1: 方式

- 決定: 第二のバックエンドは Cranelift で native の object を作る。直接の WASM 生成（(C)・(D)）と自前の機械語生成は採らない。
- 理由: 計測済みの待ち時間は native `-O0` の Clang（3.70 s）で、`run` は native だけ。WASM の直接生成は `run` に効かず、ランタイムの `.ll` が
  `internal` で `%tz.*` に依存するため wasm32 object に事前 compile できない。自前の機械語生成は aarch64・x86_64 の命令選択とレジスタ割り当てを抱える。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: 入力

- 決定: Tsuzuri が出す `-O0` の LLVM IR テキスト（連結したランタイムを含む）を構文解析して翻訳する。`src/llvm*.rs` は変えない（PB01 の `external_numeric` を除く）。
- 理由: 10,888 行の lowering を二重に持たず、所有・trap・評価順序の一致を構成で保つ。IR の生成（2.58 s に含まれる）は残るが、Clang の compile 分を置き換える。
- 見直し提案: 起票時の目的は「型付き IR から直接」だった。IR テキストの生成と構文解析が支配的になったら、Phase 2 以降で `src/llvm.rs` の出力を構造化する案を別チケットで検討する。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D3: crate と feature

- 決定: optional の依存として `cranelift-codegen`・`cranelift-frontend`・`cranelift-module`・`cranelift-object`・`target-lexicon` を足し、
  cargo feature `fast-backend`（新規）だけで有効にする。cranelift の 4 crate は同じ版を `=` で固定し、`cargo info cranelift-codegen@<版>` の
  `rust-version` が 1.85 以下の最新版を選ぶ。`target-lexicon` は cranelift が要求する版に合わせる。
- 理由: 既定の build 時間・binary の大きさ・依存を変えずに評価できる。依存 crate の内部の `unsafe` は自分の crate の `forbid` に触れない。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D4: CLI

- 決定: `--backend llvm|fast`（新規）。既定は `llvm`。`build`・`run` だけが受ける。`--backend fast` で `-O` を省略すると `-O0`。
- 理由: 明示の選択だけにし、既存のコマンドの意味を変えない。
- 状態: 既定案（実装者はこの案に従う）

### D5: Phase 1 の範囲

- 決定: 「Phase 分割」の Phase 1 のとおり。範囲外の option は E2000、翻訳できない IR は E2002 で、LLVM 経路への黙った fallback をしない。
- 理由: 明示した backend が使えないときは報告する（AGENTS.md）。fallback は計測と差分テストを曖昧にする。
- 状態: 既定案（実装者はこの案に従う）

### D6: 翻訳規則

- 決定: 「翻訳規則」の表のとおり。表にない構文は E2002 で、手順 2 で見つかったものは停止条件 3。
- 理由: 観測できる差がない写し方だけを選んだ（`_sat`、trap のシグナル、compiler-rt の `i128` 除算、NaN 正規化なし）。
- 状態: 既定案（実装者はこの案に従う）

### D7: ランタイム

- 決定: 手書きの `.ll` と `math.ll` は IR と一緒に翻訳する。`numeric` と C ランタイムは PB01 の object を link する。
- 理由: `.ll` は `internal` で型を利用者 IR に頼るので分けられない。`numeric.ll` は `sret`・`byval`・`fastcc` を内部で使い（確認済み）、翻訳の対象を増やすだけで得がない。
- 状態: 既定案（実装者はこの案に従う）

### D8: 集約値と ABI

- 決定: SSA の集約値は scalar の列に平らにする。module 内の定義は列をそのまま引数・戻り値にする。宣言だけの関数と外から呼ばれる定義は、
  scalar・`ptr` と、2 語以下の集約の戻り値だけを受け、`signext`・`zeroext` を写す。それ以外は停止条件 4。
- 理由: 棚卸しの範囲では境界の signature は scalar が中心で、aggregate の C ABI を一般に実装する必要がない。
- 状態: 既定案（実装者はこの案に従う）

### D9: デバッグ情報

- 決定: Phase 1 は DWARF を出さず、`-g` を E2000 にする。Phase 2 で行情報を足す場合は `gimli` の追加として別に承認を得る。G16 のデバッガー体験は LLVM 経路のまま。
- 理由: 行情報なしで `-g` を受けると、利用者が期待するデバッグができないのに成功に見える。
- 状態: 既定案（実装者はこの案に従う）

### D10: cache

- 決定: `build_key` は変えず、`BuildOptions` の `Debug` 表示に入る `backend` で鍵を分ける。E2E で llvm と fast の成果物が混ざらないことを確かめる。
- 理由: 既存の鍵の作り方がすでに option 全体を含む。
- 状態: 既定案（実装者はこの案に従う）

### D11: 意味の一致の検証

- 決定: `tests/fast_backend.mjs` が生成する `/bin/sh` の wrapper を compiler として既存 suite に渡す。wrapper は第 1 引数が `build`・`run` で `-O0` を含み、
  `-g`・`--debug-info`・`--trap-info`・`--wasm-feature`・`--backend`・`wasm32`・`llvm`・`header`・`wasm`・`wgsl`・`native` のどれも含まないときだけ
  `--backend fast` を足して log に 1 行書き、`exec` する。suite の source は変えない。
- 理由: 既存の独立の参照と harness をそのまま使え、suite ごとの改修が要らない。除外の判定は保守的（`native` を含むと `--target native` も除く）。
- 状態: 既定案（実装者はこの案に従う）

### D12: 既定の選択

- 決定: 本チケットでは既定を変えず、自動選択もしない。差分 suite が一定期間一致した記録を添えて、別の承認で判断する。
- 理由: 意味の一致が長期に確認されるまで、利用者の成果物を変えない。
- 状態: 要承認（承認前は既定の変更に着手しない）

### D13: 直接の WASM 生成

- 決定: 本チケットでは行わない。将来行う場合は、再配置可能な wasm object（`linking`・`reloc.*` section）を書き、ランタイムの `.ll` の linkage を変えて
  wasm32 object に事前 compile し、`wasm-ld` で link する設計を別チケットで承認を得て進める。
- 理由: WASM のビルドは `run` の繰り返しになく、ランタイムの linkage の変更は LLVM 経路の IR を変える。
- 状態: 既定案（実装者はこの案に従う）
