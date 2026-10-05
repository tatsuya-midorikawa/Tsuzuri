# PX02: 実アプリ型ベンチマークと比較対象の拡充

| 項目 | 内容 |
| --- | --- |
| ID | PX02 |
| 分類 | 計測基盤 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | PM06, PR03, C09, C11, D08 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D7（Zig・Go の計測機への導入と Phase 3）。Phase 1・2 は承認不要 |
| 手本にする既存実装 | runner: `benchmarks/run-control.mjs`（`take` による旗の解析、`reference` の BigInt 参照、25 組の `checks` の照合、`--artifacts`）。host: `benchmarks/control/host.c`（`WORKLOAD`・`measure`・25 組の相互照合・JSON 出力）。参照実装: `benchmarks/control/reference.c`（`KERNEL`・`check_count`）、`benchmarks/computations/reference.rs`（`#[no_mangle] pub extern "C" fn`）。確保の計数: `benchmarks/run-computations.mjs` の tracked 実行ファイル。記録: PX01 の `captureRun`・`makeRecord`・`writeRecords`・`measureProcess` |
| 主な影響ファイル | Phase 1: `benchmarks/computations/Main.tz`, `benchmarks/computations/native.cpp`, `benchmarks/computations/reference.rs`, `benchmarks/run-computations.mjs`, `benchmarks/csharp/Program.cs`, `benchmarks/javascript.mjs`, `benchmarks/report.mjs`, `tests/benchmark_report.mjs`。Phase 2: `benchmarks/apps/Main.tz`・`reference.c`・`reference.cpp`・`reference.rs`・`host.c`（すべて新規）, `benchmarks/run-apps.mjs`（新規）。Phase 3: `benchmarks/apps/reference.zig`・`reference.go`（新規）, `benchmarks/run-apps.mjs`。文書: `docs/benchmarks.md`, `README.md`, `_perfs/README.md` |
| 計測対象 | suite `computations`（`std_option_owned`・`std_option_length`（新規））、`control`（`integer128_mix`）、`apps`（新規、10 種目）。metric は PX01 の `wall_time`・`cpu_time`・`peak_rss`・`alloc_calls`・`alloc_bytes` と、-O3 後の IR に残る確保の呼び出し数（文書の表だけ） |

## 目的

小さな計算カーネルだけでなく、確保・文字列・連想配列・再帰データ・関数値を含む実アプリに近い処理で、C／C++／Rust（承認後は Zig・Go も）と同じ仕事を比較できるようにする。
「超高速・超省メモリ」の主張を、偏りのない種目と一致した条件で判断できるようにする。

チケットを 3 つの Phase に分ける。実装者は Phase 1 と Phase 2 を実装する。Phase 3 は D7 の承認後だけ着手する。

| Phase | 内容 | 出荷の単位 |
| --- | --- | --- |
| 1 | 既存種目の条件の修正。`std_option_owned` の参照実装に Tsuzuri と同じ所有文字列の処理をさせ、定数長の版を `std_option_length`（新規）として残す。`integer128_mix` を同じ Clang・PX01 形式で再計測し、差が残れば配置の実験で原因を切り分ける | 単独で出荷できる |
| 2 | 実アプリ型の 10 種目を系列 `apps`（新規）として追加する。Tsuzuri・C・C++・Rust の native と Tsuzuri の WASM で checksum を照合し、PX01 形式で記録する | Phase 1 と独立に出荷できる |
| 3 | `apps` に Zig と Go の参照実装を加える。ツールがなければ理由を表示して飛ばし、失敗しない | D7 の承認後 |

## 着手条件と停止条件

### 着手条件

- PX01 が done であること。`_perfs/README.md` の一覧で PX01 の状態欄が `done` で、`benchmarks/metrics.mjs` が `SCHEMA`・`LANGUAGES`・`captureRun`・`makeRecord`・`writeRecords`・`readRecords`・`measureProcess`・`compareRuns` を export していること。HEAD `f8dc655` には `benchmarks/metrics.mjs` がない。確認:

  ```sh
  cd /Users/tmidorikawa/Documents/git/Tsuzuri
  grep -n "^| PX01" _perfs/README.md
  node -e 'import("./benchmarks/metrics.mjs").then((m) => console.log(Object.keys(m).sort().join(","), m.LANGUAGES.join(",")))'
  ```

- Phase 3 は、D7 が承認済みで、PX01 の `LANGUAGES` が `zig` と `go` を含み（PX01 の設計では含む）、計測機に `zig`・`go` が導入されていること。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。
- Node 24 を使う。PATH の Node は v20.19.6 で、`run-cpp.mjs`・`run-managed.mjs` は Node 24 未満を拒否し、Node 20 は BigInt の多い参照計算で V8 が停止することがある。以下のコマンドの `node24` は次の関数を指す（GUIDE §14）。

  ```sh
  node24() { npx --yes --package=node@24 node "$@"; }
  ```

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 種目を Tsuzuri で書くのに `src/`・`std/` の変更（std 関数の追加、コンパイラの修正）が必要になった。このチケットはベンチマークと文書だけを変える。
- Tsuzuri のコンパイルが `E1017`（資源上限）や特殊化上限で失敗する、または実行が stack overflow で止まる。上限や stack の大きさを上げない。
- 実装どうし、または runner の BigInt 参照と checksum が一致せず、「設計」の種目定義からどちらが外れたかを特定できない。参照の式を実装の出力に合わせて書き換えない。
- 参照実装に標準ライブラリ以外（Rust の crate、C++ の外部ライブラリ、Go の module）が必要になった（D6）。
- Rust の staticlib のリンクに、`rustc --print native-static-libs` が示す以外のライブラリや旗が必要になった。
- `std_option_owned`・`std_option_length` 以外の既存種目の名前・大きさ・checksum、または `run-managed.mjs` の出力の欄を変える必要が生じた。
- `tests/benchmark_report.mjs` の `std_option_owned` の assert 以外の期待値を変える必要が生じた。
- Tsuzuri 側の安全検査（境界検査・`assert`・trap）を外さないと比較できない、または参照実装に fast-math や境界検査の省略（Rust の `get_unchecked` など）が必要に見えた。
- `integer128_mix` の Tsuzuri と C の -O3 の内側のループが命令単位で同一でなかった（2026-09-29 の確認と矛盾）。配置の実験に進まず報告する。

## 現状と計測（HEAD `f8dc655`）

### 既存の種目と runner

- 36 種目 = `control` 15（`benchmarks/control/host.c` の `workloads`）、`cpp` 12、`computations` 9（`benchmarks/computations/native.cpp` の `workloads`）。比較対象は C（control だけ）・C++・Rust・C#・JavaScript。Zig と Go はない。
- 系列ごとに `benchmarks/<family>/` へ Tsuzuri のソース（`Main.tz` または `Kernels.tz`）、参照実装（`reference.c`・`reference.rs`・`native.cpp`・`native.rs`）、host（`host.c` または `native.cpp`）を置き、runner は `benchmarks/run-<family>.mjs`。C# は `benchmarks/csharp/Program.cs` の `Run(family, name, count, seed)`、JavaScript は `benchmarks/javascript.mjs` の `kernel(family, name, count, seed)` が系列名で分岐し、`benchmarks/run-managed.mjs` が `["control", "cpp", "computations"]` を順に実行して統合する。
- `run-control.mjs` の旗は `take` で読む `--cpu`・`--scale`・`--artifacts`・`--baseline` と `--quick`。host は大きさ `{0, 1, 2, 17, 257}` × seed 5 個の 25 組で全実装の結果を相互照合し、runner は `reference(name, size, seed)`（BigInt）で再照合する。
- Tsuzuri の object は driver が `TSUZURI_CLANG`（既定 `clang`）で作る（`src/driver.rs` の `tool("TSUZURI_CLANG", "clang")`）。runner は C／C++ にも `process.env.TSUZURI_CLANG ?? "clang"` を使うので、環境変数をそろえれば同じ Clang になる。Rust は rustc 同梱の LLVM を使う。
- 確保の計数は `run-computations.mjs` の tracked 実行ファイル（`allocations_at_size_64`）だけで、Tsuzuri の IR の `malloc`／`free` だけを数える。
- 詳細化時の計測機: `clang` は `/usr/bin/clang`（`Apple clang version 21.0.0 (clang-2100.3.34.2)`）、`rustc 1.98.1 (48a229cea 2026-09-01) (Homebrew)`、PATH の Node は v20.19.6、`dotnet` あり、`zig`・`go` なし（`command -v zig go` が何も出さない）。

### `std_option_owned` の不一致（コードで確認）

| 実装 | 場所 | `(state & 7) != 0` の反復の処理 |
| --- | --- | --- |
| Tsuzuri | `benchmarks/computations/Main.tz` の `option_owned_step`・`direct_option_owned_step` | `"hello" + " world"` で所有文字列を作り、`(text + "!").length` を取り、両方を解放する |
| C++ | `benchmarks/computations/native.cpp` の `cpp_loop<4>` | `const auto length = (state & 7) == 0 ? 0 : 12;`。確保しない |
| Rust | `benchmarks/computations/reference.rs` の `run_loop::<4>`（`rust_std_option_owned`） | `if state & 7 == 0 { 0 } else { 12 }`。確保しない |
| C# | `benchmarks/csharp/Program.cs` の `case "std_option_owned"` | 定数 `12UL` |
| JavaScript | `benchmarks/javascript.mjs` の `computation` の `case "std_option_owned"` | 定数 `12n` |

- 確保するのは Tsuzuri だけで、参照実装は長さを定数で求める。checksum は `mix(state, salt) ^ length` で全実装が一致する（`run-computations.mjs` の `reference`）。
- -O3 後の Tsuzuri の IR には、反復ごとに `@malloc(i64 22)`（`"hello world"` の 11 UTF-16 単位）1 回、`@llvm.memcpy` 2 回（10 bytes と 12 bytes）、`@free` 1 回が残る。`text + "!"` の確保は LLVM が除去し、長さの overflow 検査の分岐だけが残る。再現（検証済み）:

  ```sh
  cd /Users/tmidorikawa/Documents/git/Tsuzuri && mkdir -p /tmp/tz-px02
  target/release/tsuzuri build benchmarks/computations/Main.tz --emit llvm -o /tmp/tz-px02/comp.ll
  clang -O3 -fPIC -fno-fast-math -ffp-contract=off -Wno-override-module -S -emit-llvm /tmp/tz-px02/comp.ll -o /tmp/tz-px02/comp.opt.ll
  awk '/^define .*@tz_direct_std_option_owned\(/,/^}/' /tmp/tz-px02/comp.opt.ll \
    | grep -oE '@(malloc|free|llvm\.memcpy\.p0\.p0\.i64)\(' | sort | uniq -c
  ```

  期待: `1 @free(`、`2 @llvm.memcpy.p0.p0.i64(`、`1 @malloc(`。`tz_ce_std_option_owned` も `@free` 1・`@llvm.memcpy` 2。
- 2026-09-26 の実測（`--scale 0.1`、820 反復、ms/呼び出し）: Tsuzuri 0.017724、C++ 0.001539、Rust 0.001822、C# 0.002109、JavaScript 0.058372。`benchmarks/report.mjs` は `unrankedRows` でこの行を順位から外し、`tests/benchmark_report.mjs` がそれを assert する（`owned.workloads[0].name = "std_option_owned"`）。`docs/benchmarks.md` の `### 条件と読み方` と `## コンピュテーション式の比較` にも「長さを直接計算する最適化基準」と書かれている。

### `integer128_mix`（コードと記録で確認）

- Tsuzuri の `integer128_mix`（`benchmarks/control/Main.tz`）は `for remaining = count as i32 downto 1`、C の `KERNEL(integer128_mix)`（`benchmarks/control/reference.c`）は `int64_t remaining` で数える。どちらも `count` を 0〜100,000,000 に制限し、`i128u`／`__uint128_t` で同じ式を計算する。Rust は `rust_integer128_mix`（`benchmarks/control/reference.rs`）。host の大きさは `quick ? 1024 : 2000000`。
- 2026-09-26 の記録（Homebrew Clang 23.1.1、`--scale 0.1` で 200,000 反復、generic 3 回の中央値の中央値、ms/呼び出し）: Tsuzuri 0.572162、C 0.508878、C++ 0.508659、Rust 0.528105、C# 0.704955、JavaScript 12.225021。相対性能 0.891。
- 2026-09-29 に Apple Clang 21 で、内側のループが Tsuzuri と C で命令単位で同一と確認された（`_perfs/README.md`）。記録と確認で Clang の版が異なり、同じ条件での再計測はない。

### 欠けているもの

- 確保・文字列・連想配列・再帰データ・関数値の多い種目、最大 RSS、参照実装の確保数の比較がない。
- `std/Map.tz` の `Map` と `std/Set.tz` の `Set` は整列した `Vec` と二分探索（`lower_bound`）で、`insert` は末尾に足してから `Vec.swap` で位置まで移す。`Array.sort` は `Array.sort_by` を呼ぶ。HashMap（C09）、Arena（C10）、多次元配列（C11）、JSON（D08）は未実装（`_features/README.md` で todo）。

## 目標と指標

目標は比較の条件をそろえて記録することで、速度の値ではない。どの指標も CI の合否にしない（AGENTS.md）。

| 指標 | 単位・統計 | 対象 | 取り方 |
| --- | --- | --- | --- |
| `wall_time` | ms/呼び出し。warm-up を除く 9 標本以上の中央値・最小・最大 | `computations`・`control`・`apps` | host の単調時計（PX01 の in-process の定義） |
| `cpu_time` | ms/呼び出し。同上 | 同上 | host の `clock()` |
| `peak_rss` | bytes。9 標本の中央値・最小・最大 | `apps` | 種目 1 つ・実装 1 つだけを実行する host の起動（`--only`（新規））を PX01 の `measureProcess` で測る |
| `alloc_calls`・`alloc_bytes` | count・bytes。1 標本（決定的） | `computations`・`apps` の Tsuzuri だけ | PX01 の tracked 実行ファイル（PX01 D5） |
| 残る確保の呼び出し | -O3 後の IR の呼び出し箇所の数 | `std_option_owned`・`std_option_length` の全 native 実装 | 「生成コードの確認」の 1。`docs/benchmarks.md` の表だけに書き、PX01 の metric にしない |

Phase ごとの達成の定義:

- Phase 1: `std_option_owned` の全実装がソース上で同じ所有文字列の処理をし、`report.mjs` の順位の除外がなくなる。`std_option_length` が 37 種目目として全実装（C#・JavaScript を含む）で照合される。`integer128_mix` の 3 run の結果と、差の分類（「計測の揺れ」「配置」「未解明」のどれか）が `docs/benchmarks.md` にある。
- Phase 2: `apps` の 10 種目 × 4 実装（Tsuzuri・C・C++・Rust）の `wall_time`・`cpu_time`・`peak_rss`、Tsuzuri の `alloc_calls`・`alloc_bytes` が PX01 形式で `target/perf/<run_id>/apps.jsonl` にあり、Tsuzuri の WASM の checksum が native と一致する。
- Phase 3: 同じ 10 種目に `zig`・`go` の行が加わる。ツールがない計測機では runner が理由を表示して成功で終わる。

## 変えてはいけない意味

- `src/`・`std/` を変えない。コンパイラの生成 IR はこのチケットで変わらない。
- Tsuzuri のソースで安全検査（配列・Vec の境界検査、`assert`、除算の検査、trap）を外して速くしない。参照実装の検査の有無は D6 のとおりにそろえ、差は `docs/benchmarks.md` の表に書く。
- 整数はすべて 64 bit の折り返し（mod 2^64）。C／C++ は `uint64_t`、Rust は `wrapping_add`・`wrapping_mul`・`wrapping_sub`、Tsuzuri は `i64u`（`0 - value` を含め折り返すことを probe で確認済み）。符号付きの折り返しに依存しない。
- 浮動小数点は fast-math・FMA の縮約・再結合なし。C／C++ は既存の `-fno-fast-math -ffp-contract=off`、Rust は既定（縮約しない）、Tsuzuri は既定。和の順序は種目定義の添字の昇順に固定する。
- 既存種目の checksum を変えない。`std_option_owned` の長さは変更後も 12 で、`mix(state, salt) ^ length` の値は今と同じ。
- 全確保を解放してから戻る。Tsuzuri の tracked 実行で `live == 0`、WASM の module の import は空（`WebAssembly.Module.imports(module)` が `[]`、D-18）。
- checksum は仕様の式から独立に計算した BigInt 参照（runner の `reference`）と照合し、実装の出力から期待値を作らない。

## 設計

### Phase 1a: `std_option_owned` と `std_option_length`

D2 のとおり、各言語の標準の所有文字列でソース上の処理をそろえ、最適化による確保の除去は各コンパイラの結果として記録する（障壁を入れて確保を強制しない）。旧来の定数長の参照実装は `std_option_length`（新規）へ移す（D3）。

| 実装 | `std_option_owned`（変更後） | `std_option_length`（新規） |
| --- | --- | --- |
| Tsuzuri | 変更なし（`option_owned_step`・`direct_option_owned_step`） | `option_length_step`: `Maybe { let! length = if (state & 7) == 0 then None else Some 12; return length }` と `mix state salt ^ Maybe.default_value 0 result`。`direct_option_length_step`: 同じ Maybe を `match` で開く。export は `ce_std_option_length`・`direct_std_option_length`（すべて新規。`std_option_owned` の 4 定義を写す） |
| C++ | `cpp_loop<5>`（新規の分岐）: `std::optional<std::u16string> text;` に `std::u16string(hello) + world` を入れ、長さは `(*text + bang).size()`。`hello`・`world`・`bang` は関数の外の `static const char16_t` 配列 `u"hello"`・`u" world"`・`u"!"` | 現在の `cpp_loop<4>` |
| Rust | `run_loop::<5>`（新規）: `#![no_std]` のまま、`OwnedUtf16`（新規。`malloc` した `*mut u16` と長さを持ち、`Drop` で `free`）と `concat(&[u16], &[u16]) -> OwnedUtf16`（新規。`malloc`、`core::ptr::copy_nonoverlapping` 2 回）で 2 回連結する。`Option<OwnedUtf16>` を使い、長さは 2 回目の結果の長さ | 現在の `run_loop::<4>` |
| C# | `static readonly string Hello = "hello", World = " world", Bang = "!";`（`const` にしない。C# の定数畳み込みを避ける）。`string text = Hello + World; length = (text + Bang).Length;` | 現在の `12UL` の式 |
| JavaScript | モジュールの `const parts = ["hello", " world", "!"];`。`const text = parts[0] + parts[1]; length = (text + parts[2]).length;` | 現在の `12n` の式 |

- 大きさ: `std_option_owned` は今の `quick ? 128 : 8192` のまま。`std_option_length` は `std_option` と同じ `quick ? 1024 : 2000000`。
- `native.cpp` の `workloads` の順は既存 9 種目の後に `std_option_length` を足す（既存の添字を変えない）。
- probe（`/tmp/tz-work-PX02/optlen`、`benchmarks/computations` の写しに上の Tsuzuri の 4 定義を足したもの）で、WASM の `tz_ce_std_option_length`・`tz_direct_std_option_length` が大きさ 0・1・5・17、seed 42 で `tz_ce_std_option_owned` と同じ値（`2a`、`91778aed87ee5ebe`、`f23b2cc8de166f4a`、`e9155a66962728b6`）を返すことを確認済み。

### Phase 1b: `integer128_mix` の再計測

runner は変えない（D4）。手順:

1. `TSUZURI_CLANG=/usr/bin/clang` を export し、driver と runner の C／C++ を同じ Clang にする。`clang --version` の 1 行目は PX01 の `host.clang` に入る。
2. `run-control.mjs --metrics` を 3 回、別の run_id（`PX02-int128-a`・`-b`・`-c`）で実行する。host の標本は 1 run あたり実装ごとに 12（`VARIANTS * 3`）。
3. 判定: 3 run のすべてで Tsuzuri の `wall_time` の最小が C の最大より大きいときだけ「差が残る」とする。そうでなければ「計測の揺れ」と分類して終える。
4. 差が残る場合だけ、配置の実験をする。`--artifacts` の `control.ll` を C と同じ Clang の旗に `-falign-functions=64 -mllvm -align-loops=64` を足して object にし、C・C++（`reference.c`）も同じ旗で、Rust は `-C llvm-args=-align-all-functions=6 -C llvm-args=-align-loops=64` で作り直して `host.c` とリンクし、3 回実行する。差が消えれば「配置」、残れば「未解明」と分類し、「未解明」ならそれ以上調べずに artifacts と数値を報告する（停止条件ではなく完了報告に書く）。

### Phase 2: 系列 `apps` の構成

`control` と同じ形にする（D5）。kernel の ABI は `uint64_t <prefix><name>(int64_t size, uint64_t seed)`。

| ファイル | 内容 |
| --- | --- |
| `benchmarks/apps/Main.tz`（新規） | 10 種目の `export def <name> :: i64 -> i64u -> i64u`（C の記号は `tz_<name>`）。先頭で `assert (size >= 0 && size <= <上限>)` |
| `benchmarks/apps/reference.c`（新規） | C11。`c_<name>`。`check_size`（`control/reference.c` の `check_count` と同じ形で上限だけ種目ごと）、`malloc`／`realloc`／`free`、`qsort` は使わず手書きの merge |
| `benchmarks/apps/reference.cpp`（新規） | C++20。`extern "C"` の `cpp_<name>`。`std::vector`・`std::string`・`std::unique_ptr`・`std::function`・`std::lower_bound`・`std::to_chars`・`std::from_chars` |
| `benchmarks/apps/reference.rs`（新規） | Rust 2021、std あり。`#[no_mangle] pub extern "C" fn rust_<name>(size: i64, seed: u64) -> u64`。`--crate-type=staticlib` |
| `benchmarks/apps/host.c`（新規） | `control/host.c` を写す。`WORKLOAD(name, size)`、25 組の相互照合、`measure`。追加の起動形式 `host --only <name> <c\|cpp\|rust\|tsuzuri> [--quick\|--scale s]`（新規）は 1 回だけ呼び、checksum を 1 行出して終わる（`peak_rss` 用） |
| `benchmarks/run-apps.mjs`（新規） | `run-control.mjs` を写す。旗は `--quick`・`--scale`・`--cpu`・`--artifacts`・`--metrics`。`--baseline` は持たない（D5） |

共通の式（すべて mod 2^64、`>>` は論理シフト）:

```text
hash(k)     = let x = k * 6364136223846793005 + 1442695040888963407 in x ^ (x >> 29)
fold(t, v)  = t * 31 + v
```

### Phase 2: 種目の定義

`n` は host が渡す大きさ（`--scale` 適用後）。大きさ 0 の扱いも定義に含める。文字列の種目は、Tsuzuri が所有値を作る箇所（トークン・行・フィールド・数字列）で参照実装も所有値を作る（D6）。

| 種目 | 大きさ（通常／quick）・上限 | 入力と処理 | checksum |
| --- | --- | --- | --- |
| `tokenize` | 200,000／1,024・≤ 10,000,000 | 語 `w_i` = `["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"][hash(seed + i) >> 61]`、`text = join(w, " ")`、`tokens = split(text, " ")`（所有文字列の配列） | `n == 0` なら 0。`t = len(tokens)`、各 token で `t = fold(t, len(token))` |
| `format_join` | 100,000／1,024・≤ 10,000,000 | `v_i = hash(seed + i)`、各 `v_i` を 10 進の所有文字列にして `","` で連結、`split` で戻して各 field を u64 として解析し、折り返しで合計 | `n == 0` なら 0。`fold(len(text), sum)` |
| `csv_aggregate` | 100,000／1,024・≤ 10,000,000 | 行 `i`: `h = hash(seed + i)`、`category = h >> 60`、`amount = (h >> 20) & 65535`、行 = `str(category) + "," + str(amount)`、`text = join(行, "\n")`。`split(text, "\n")` の各行を `split(行, ",")` し、2 field を解析して `sums[category] += amount`、`counts[category] += 1`（16 要素） | `n == 0` なら 0。`t = fold(0, 行数)`、`c = 0..15` の順に `t = fold(fold(t, sums[c]), counts[c])` |
| `map_count` | 200,000／1,024・≤ 10,000,000 | `s = seed` から `s = s * 6364136223846793005 + 1442695040888963407` を n 回。各回 `key = (s >> 33) & 1023`、map[key] += 1（なければ 1）、set に `key & 255` を入れる。map・set は整列配列と二分探索（`std/Map.tz`・`std/Set.tz` と同じ。参照実装は見つかれば位置で更新、なければ末尾に足して隣との交換で位置まで移す） | `t = len(map) * 1000003 + len(set)`、`k = 0..1023` の順に `t = fold(t, map[k] または 0)` |
| `record_sort` | 200,000／1,024・≤ 10,000,000 | 記録 `{ key = hash(seed + i) >> 44, id = i }` の配列を key の昇順で安定に整列。アルゴリズムは `Array.sort_by` と同じ幅 1, 2, 4, … の bottom-up merge（左右の run に値がある間、`cmp(left, right) > 0` のときだけ右を取る） | 整列後の順に `t = fold(t, id)` |
| `tree_transform` | 256／2・≤ 100,000 | `i = 0..n-1` の各 `k = seed + i` で深さ 12 の `Expr`（`Lit of i64u \| Add \| Mul \| Neg`）を作り（下の擬似コード）、`simplify` して `eval` | `t = fold(t, eval(simplify(build(12, k))))` |
| `binary_trees` | 2,000／4・≤ 1,000,000 | 各 `i` で深さ `d = 4 + ((seed + i) & 7)` の完全二分木（`Leaf \| Node of Tree * Tree`、節ごとに確保）を作り、節の数を数えて解放 | `t = fold(t, count)` |
| `closure_pipeline` | 1,000,000／1,024・≤ 10,000,000 | 4 つの関数値 `f_k(x) = x * 6364136223846793005 + (seed ^ k)`（`seed` と `k` を捕捉）を配列に置く。`values_i = seed + i`、`mapped_i = f_{x & 3}(x)`（新しい配列）、偶数だけを畳み込む | `t = 0`、各 `x` で `x & 1 == 0` なら `t = fold(t, x)` |
| `graph_bfs` | 100,000／1,024・≤ 1,000,000 | 辺 `j = 0..4n-1`: `h = hash(seed + j)`、`from = (h >> 32) % n`、`to = (h & 0xffffffff) % n`。CSR（次数・排他的な累積和・辺の順に詰める）を作り、節 0 から幅優先探索（`dist` は -1 で初期化、隣接は CSR の順） | `n == 0` なら 0。`i = 0..n-1` の順に `t = fold(t, dist[i] as u64)`（未到達は 2^64 - 1） |
| `spectral_norm` | 500／16・≤ 5,000 | `a(i, j) = 1.0 / ((i + j) * (i + j + 1) / 2 + i + 1)`（整数で計算してから f64）。`u = [1.0; n]`、10 回 `v = AtA(u)`、`u = AtA(v)`。`AtA(x) = At(A(x))`、`A(x)_i = Σ_j a(i, j) x_j`、`At(x)_i = Σ_j a(j, i) x_j`（j の昇順）。`r = sqrt(Σ u_i v_i / Σ v_i v_i)` | `n == 0` なら 0。`(r * 1e15)` を u64 へ切り捨てて `^ seed` |

`tree_transform` の擬似コード（Tsuzuri の形は `/tmp/tz-work-PX02/tree2/Main.tz` で検証済み。WASM で深さ 0・1・5・12、seed 42 が `91778a`、`4d3e7f2b741c`、`bf4f7f34e53f2640`、`84cfa56ce891d040`）:

```text
build(depth, key): h = hash(key)
  depth == 0 -> Lit(h >> 40)
  h & 3 == 0 -> Neg(build(depth - 1, key * 2 + 1))
  h & 3 == 1 -> Mul(build(depth - 1, key * 2 + 1), build(depth - 1, key * 2 + 2))
  otherwise  -> Add(build(depth - 1, key * 2 + 1), build(depth - 1, key * 2 + 2))
simplify(Lit v) = Lit v
simplify(Neg e) = match simplify(e): Neg x -> x | Lit v -> Lit(0 - v) | other -> Neg other
simplify(Add(l, r)) = match (simplify(l), simplify(r)): (Lit a, Lit b) -> Lit(a + b) | (a, b) -> Add(a, b)
simplify(Mul(l, r)) = Mul(simplify(l), simplify(r))
eval: Lit v -> v | Neg e -> 0 - eval(e) | Add -> eval(l) + eval(r) | Mul -> eval(l) * eval(r)
```

- `simplify` は元の木を消費して新しい木を作る（参照実装も `simplify` の結果を新しい節として確保し、元の節を解放する。C++ は `std::unique_ptr`、Rust は `Box`）。
- 参照実装は `build` の左の子を先に作る（C は 1 つの式で 2 つの呼び出しを並べない）。`build` は純粋なので値は順序に依らないが、確保の順序をそろえる。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| Phase 1a | `benchmarks/computations/Main.tz` | `option_length_step`・`direct_option_length_step`・`ce_std_option_length`・`direct_std_option_length`（新規） | 上の表の定義を `std_option_owned` の定義の直後に置く |
| Phase 1a | `benchmarks/computations/native.cpp` | `cpp_loop`、`DECLARE_RUST`、`workloads` | `kind == 5` の分岐（`<optional>`・`<string>` を include）。`DECLARE_RUST(std_option_length)`。`WORKLOAD(std_option_owned, cpp_loop<5>, …)`、末尾に `WORKLOAD(std_option_length, cpp_loop<4>, quick ? 1024 : 2000000)` |
| Phase 1a | `benchmarks/computations/reference.rs` | `run_loop`、`OwnedUtf16`・`concat`（新規）、`loop_kernel!` | `5 =>` の分岐。`loop_kernel!(rust_std_option_owned, 5)`、`loop_kernel!(rust_std_option_length, 4)` |
| Phase 1a | `benchmarks/run-computations.mjs` | `reference` | `name === "std_option_owned" \|\| name === "std_option_length"` を同じ式にする |
| Phase 1a | `benchmarks/csharp/Program.cs` | `Computation` | `case "std_option_owned"` を連結に、`case "std_option_length"`（新規）に旧来の式 |
| Phase 1a | `benchmarks/javascript.mjs` | `computation` | 同上 |
| Phase 1a | `benchmarks/report.mjs` | `unrankedRows` を渡す行と注記の文 | 行を消し、`<code>std_option_owned</code> は確保条件が異なるため順位を付けません。` の文を消す。`renderTable` の `unrankedRows` の処理は他で使わなければ消す（`grep -n unrankedRows benchmarks/*.mjs tests/*.mjs`） |
| Phase 1a | `tests/benchmark_report.mjs` | `owned` の 2 つの assert | `std_option_owned` も他の行と同じく `data-fastest` と `data-tsuzuri-performance` を持つことの assert に変える |
| Phase 2 | `benchmarks/apps/*`・`benchmarks/run-apps.mjs` | 上の表 | 新規 |
| Phase 3 | `benchmarks/apps/reference.zig`・`reference.go`、`benchmarks/run-apps.mjs` | `zig_<name>`・`go_<name>`、ツールの検出 | D7 |

## 実装手順

このチケットは `src/` を変えないので、`cargo` は手順 1 のビルドだけでよい。各手順の後で、それまでの runner の quick と `node tests/benchmark_report.mjs` が成功する。以下のコマンドは `cd /Users/tmidorikawa/Documents/git/Tsuzuri`、`node24` の定義（着手条件）、`export TSUZURI_CLANG=/usr/bin/clang` の後で実行する。

### 手順 1: ベースライン

- 変更: なし。
- 内容: 着手条件を確かめ、変更前の quick の出力を保存する。
- 確認: 次がすべて終了コード 0 で、最後に `report-ok` が出る。

```sh
cargo build --release --locked && mkdir -p /tmp/tz-px02
for suite in control computations; do node24 benchmarks/run-$suite.mjs target/release/tsuzuri --quick > /tmp/tz-px02/before-$suite.json || break; done
node24 benchmarks/run-managed.mjs target/release/tsuzuri --quick --no-report > /tmp/tz-px02/before-managed.json
node tests/benchmark_report.mjs && echo report-ok
```

### 手順 2: Tsuzuri の `std_option_length`

- 変更: `benchmarks/computations/Main.tz`。
- 内容: 設計の 4 定義を足す。
- 確認: `target/release/tsuzuri check benchmarks/computations/Main.tz` が診断なし。次が 3 行とも `2a 91778aed87ee5ebe f23b2cc8de166f4a e9155a66962728b6`。

```sh
target/release/tsuzuri build benchmarks/computations/Main.tz --target wasm32 -O3 -o /tmp/tz-px02/comp.wasm
node -e 'const e = new WebAssembly.Instance(new WebAssembly.Module(require("fs").readFileSync("/tmp/tz-px02/comp.wasm"))).exports;
for (const k of ["tz_ce_std_option_length", "tz_direct_std_option_length", "tz_ce_std_option_owned"])
  console.log([0n, 1n, 5n, 17n].map((n) => BigInt.asUintN(64, e[k](n, 42n)).toString(16)).join(" "))'
```

### 手順 3: C++・Rust の参照実装と runner の参照

- 変更: `benchmarks/computations/native.cpp`、`benchmarks/computations/reference.rs`、`benchmarks/run-computations.mjs` の `reference`。
- 内容: 段ごとの変更の表の 3 行。
- 確認: `node24 benchmarks/run-computations.mjs target/release/tsuzuri --quick --artifacts /tmp/tz-px02/comp-art > /tmp/tz-px02/comp-quick.json` が終了コード 0。`node -e 'console.log(require("/tmp/tz-px02/comp-quick.json").workloads.map((w) => w.name).join(","))'` の末尾が `std_option_owned,std_option_length`。

### 手順 4: C#・JavaScript

- 変更: `benchmarks/csharp/Program.cs` の `Computation`、`benchmarks/javascript.mjs` の `computation`。
- 確認: `node24 benchmarks/run-managed.mjs target/release/tsuzuri --quick --no-report > /tmp/tz-px02/managed-quick.json` が終了コード 0 で、`workloads.length` が 37。

### 手順 5: 要約の順位の除外をやめる

- 変更: `benchmarks/report.mjs`、`tests/benchmark_report.mjs`。
- 確認: `node tests/benchmark_report.mjs` が終了コード 0。`grep -n "std_option_owned" benchmarks/report.mjs` が何も出さない。

### 手順 6: `integer128_mix` の再計測

- 変更: なし（結果は手順 11 で文書に書く）。
- 内容: 「計測手順」の Phase 1b。「生成コードの確認」の 2 を先に行い、ループが同一でなければ停止条件に従う。
- 確認: `target/perf/PX02-int128-{a,b,c}/control.jsonl` があり、分類が 1 つに決まる。

### 手順 7: `apps` の Tsuzuri 実装

- 変更: `benchmarks/apps/Main.tz`（新規）。
- 内容: 10 種目。検証済みの書き方は「落とし穴」の Tsuzuri の項にまとめる（再帰 union と `ref` の `match`、`Map.empty()`・`Map.get (ref m) key`・`Map.insert m key value`、`Set.insert`・`Set.length (ref s)`、`String.join (ref sep) (ref parts)`・`String.split (ref sep) (ref text)`、`Vec.with_capacity`・`Vec.push`・`Vec.set v i x`・`v[i]`・`Vec.get (ref v) i`、`Array.sort_by (left -> right -> …) (ref items)`、`Array.map`・`Array.fold`、関数値の配列 `new [i64u -> i64u](4, k -> (x -> …))`、`to_string`・`let parsed: Maybe<i64u> = Parse.parse ref field`・`Maybe.get`、`Math.sqrt`）。
- 確認: `target/release/tsuzuri check benchmarks/apps/Main.tz` が診断なし。`target/release/tsuzuri build benchmarks/apps/Main.tz --target wasm32 -O3 -o /tmp/tz-px02/apps.wasm` の後、`node -e 'console.log(WebAssembly.Module.imports(new WebAssembly.Module(require("fs").readFileSync("/tmp/tz-px02/apps.wasm"))).length)'` が `0`。

### 手順 8: C・C++・Rust の参照実装と host

- 変更: `benchmarks/apps/reference.c`・`reference.cpp`・`reference.rs`・`host.c`（新規）。
- 確認: 次が警告なしで成功し、最後に `native-static-libs:` を含む行が出る。

```sh
F="-O3 -fPIC -fno-fast-math -ffp-contract=off -Wall -Wextra -Werror"
clang -x c -std=c11 $F -c benchmarks/apps/reference.c -o /tmp/tz-px02/apps-c.o
clang --driver-mode=g++ -std=c++20 $F -c benchmarks/apps/reference.cpp -o /tmp/tz-px02/apps-cpp.o
rustc --edition=2021 --crate-type=staticlib --crate-name=apps_reference -C opt-level=3 -C panic=abort \
  -C codegen-units=1 -C relocation-model=pic --print native-static-libs \
  benchmarks/apps/reference.rs -o /tmp/tz-px02/libapps_reference.a 2>&1 | grep native-static-libs
```

### 手順 9: `run-apps.mjs`

- 変更: `benchmarks/run-apps.mjs`（新規）。
- 内容: `run-control.mjs` の構成を写す。`reference(name, size, seed)` に 10 種目の BigInt／Number の独立実装を書く（種目定義から直接。Tsuzuri の出力を写さない）。Rust は手順 8 のコマンドで作り、stderr の `native-static-libs: ` 以降を host のリンクに足す。WASM は `-O0` と `-O3` の 2 つを作り、import が空で、25 組の checksum が native と一致することを assert する。stdout の JSON は `run-control.mjs` と同じ欄（`environment`・`workloads[].name/size/checks/raw`・`c_cpp_flags`・`rust_flags`・`tsuzuri_flags`）に `rust_native_static_libs` を足す。
- 確認: `node24 benchmarks/run-apps.mjs target/release/tsuzuri --quick > /tmp/tz-px02/apps-quick.json` が終了コード 0。名前の一覧が `tokenize,format_join,csv_aggregate,map_count,record_sort,tree_transform,binary_trees,closure_pipeline,graph_bfs,spectral_norm`。

### 手順 10: `--metrics`（`peak_rss` と確保の計数を含む）

- 変更: `benchmarks/run-apps.mjs`、`benchmarks/apps/host.c`（`--only`、`TRACKING`）。
- 内容: PX01 の「runner の対応」と同じ形で `captureRun`・`makeRecord`・`writeRecords(dir, "apps", records)`。`peak_rss` は `measureProcess(host, ["--only", name, language, ...])`（既定の 9 run・warm-up 1）。Tsuzuri の確保は PX01 の `instrumentAllocations` と `benchmarks/tracked_alloc.c` で作る別の host を `-DTRACKING` で作り、種目ごとに 1 回呼んで `live == 0` を確かめる。
- 確認: `node24 benchmarks/run-apps.mjs target/release/tsuzuri --quick --metrics target/perf/PX02-apps-quick > target/perf/PX02-apps-quick/apps.json` が終了コード 0、`wc -l < target/perf/PX02-apps-quick/apps.jsonl` が `140`（`wall_time`・`cpu_time` 80、`peak_rss` 40、`alloc_calls`・`alloc_bytes` 20）、`node24 benchmarks/metrics.mjs report target/perf/PX02-apps-quick` が終了コード 0。`mkdir -p target/perf/PX02-apps-quick` を先に行う。

### 手順 11: 計測と文書

- 変更: 「ドキュメント」の表のファイル。
- 内容: 「計測手順」の本計測を行い、表と分類を書く。
- 確認: `git diff --check` が何も出さない。`node scripts/check-docs.mjs README.md` が成功（README の変更に対して）。

### 手順 12: 最終確認

- 確認: 手順 1 の 3 つの quick と `node24 benchmarks/run-apps.mjs target/release/tsuzuri --quick` が終了コード 0。`node tests/benchmark_report.mjs` が成功。`git diff --stat -- src std` が何も出さない。

## 計測手順

PX01 の「計測手順」に従う（生データは `target/perf/<run_id>/`、Git に加えない）。環境は `captureRun` が各レコードの `host` に記録する。suite は 1 つずつ実行し、並列にしない。計測中に他の重い処理を動かさない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node24() { npx --yes --package=node@24 node "$@"; }
export TSUZURI_CLANG=/usr/bin/clang
cargo build --release --locked
# Phase 1a
R=target/perf/PX02-computations-$(date -u +%Y%m%dT%H%M%SZ) && mkdir -p "$R"
node24 benchmarks/run-computations.mjs target/release/tsuzuri --metrics "$R" --artifacts /tmp/tz-px02/comp-art-full > "$R/computations.json"
# Phase 1b（3 run）
for tag in a b c; do
  R=target/perf/PX02-int128-$tag && mkdir -p "$R"
  node24 benchmarks/run-control.mjs target/release/tsuzuri --metrics "$R" --artifacts /tmp/tz-px02/int128-$tag > "$R/control.json" || break
done
for tag in a b c; do node -e 'for (const r of require("fs").readFileSync(process.argv[1], "utf8").trim().split("\n").map(JSON.parse))
  if (r.workload === "integer128_mix" && r.metric === "wall_time" && r.variant === "default") console.log(r.language, r.median, r.min, r.max)' target/perf/PX02-int128-$tag/control.jsonl; done
# Phase 2
R=target/perf/PX02-apps-$(date -u +%Y%m%dT%H%M%SZ) && mkdir -p "$R"
node24 benchmarks/run-apps.mjs target/release/tsuzuri --metrics "$R" --artifacts /tmp/tz-px02/apps-art > "$R/apps.json"
node24 benchmarks/metrics.mjs report "$R" > "$R/report.txt"
```

配置の実験（Phase 1b で「差が残る」ときだけ）。旗なしの版 `benchmark0` と旗ありの版 `benchmark64` を作り、それぞれ 3 回実行して `integer128_mix` の `tsuzuri_wall_ms` と `c_wall_ms` の中央値を比べる。Tsuzuri の object はどちらも driver ではなく同じ Clang で IR から作る（C と同じ backend の指定にそろえるため）。

```sh
A=/tmp/tz-px02/int128-a; X=/tmp/tz-px02/align; mkdir -p $X
for v in 0 64; do
  F="-O3 -fPIC -fno-fast-math -ffp-contract=off"; RF=""
  if [ $v = 64 ]; then F="$F -falign-functions=64 -mllvm -align-loops=64"; RF="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-loops=64"; fi
  clang ${=F} -Wno-override-module -c $A/control.ll -o $X/control$v.o
  clang -x c -std=c11 ${=F} -Wall -Wextra -Werror -DPREFIX=c_ -c benchmarks/control/reference.c -o $X/c$v.o
  clang -x c++ -std=c++20 ${=F} -Wall -Wextra -Werror -DPREFIX=cpp_ -c benchmarks/control/reference.c -o $X/cpp$v.o
  mkdir -p $X/rust$v && rustc --edition=2021 --crate-type=lib --crate-name=control_reference -C opt-level=3 -C panic=abort \
    -C force-unwind-tables=no -C relocation-model=pic -C codegen-units=1 ${=RF} --emit=obj benchmarks/control/reference.rs --out-dir $X/rust$v
  clang -std=c11 -O3 -fPIC -fno-fast-math -ffp-contract=off -Wall -Wextra -Werror benchmarks/control/host.c -I $A \
    $X/control$v.o $X/c$v.o $X/cpp$v.o $X/rust$v/control_reference.o -lm -o $X/benchmark$v
  for i in 1 2 3; do $X/benchmark$v --scale 1 > $X/run$v-$i.json; done
done
```

- `-mllvm -align-loops=64` を Clang が受け付けなければ、その旗を外して `-falign-functions=64` だけで行い、文書にその旨を書く。
- 生の JSON（`/tmp/tz-px02/align/run*.json`）は `target/perf/PX02-int128-align/` に写して残す。

## 生成コードの確認

### 1. `std_option_owned` の残る確保（手順 3 の artifacts）

```sh
A=/tmp/tz-px02/comp-art
for f in tz_direct_std_option_owned tz_ce_std_option_owned; do
  awk "/^define .*@$f\\(/,/^}/" $A/current.opt.ll | grep -oE '@(malloc|free)\(' | sort | uniq -c; done
awk '/^define .*cpp_loopILi5E/,/^}/' $A/cpp.opt.ll | grep -oE '@(_Znwm|_ZdlPv[A-Za-z0-9_]*|malloc|free)\(' | sort | uniq -c
awk '/^define .*(rust_std_option_owned|run_loop)/,/^}/' $A/computation_reference.ll | grep -oE '@(malloc|free)\(' | sort | uniq -c
```

- 期待: Tsuzuri は HEAD と同じく `malloc` 1・`free` 1（コンパイラを変えないため）。C++ と Rust は数を期待せず、得た数を `docs/benchmarks.md` の表にそのまま書く（0 でもよい。D2）。`std_option_length` は全実装で確保の呼び出しが 0。

### 2. `integer128_mix` の内側のループ（手順 6 の artifacts）

後方分岐の飛び先から分岐までの基本ブロックの命令名を比べる。

```sh
A=/tmp/tz-px02/int128-a
loop() { awk -v fn="$1" '$0 ~ "^"fn":" {on=1} on {print} on && /\.cfi_endproc/ {exit}' "$2" \
  | awk '/^L[A-Za-z0-9_]+:/ {label=$1; body=""} {body=body $0 "\n"} /^[ \t]+b\.[a-z]+[ \t]+L/ {if ($NF ":" == label) printf "%s", body}' \
  | awk 'NF && $1 !~ /:$/ {print $1}'; }
loop _tz_integer128_mix $A/control.s > /tmp/tz-px02/tz-loop.txt
loop _c_integer128_mix $A/c.s > /tmp/tz-px02/c-loop.txt
test -s /tmp/tz-px02/tz-loop.txt && diff /tmp/tz-px02/tz-loop.txt /tmp/tz-px02/c-loop.txt && echo loop-identical
```

- 期待: `loop-identical`。空（ループが 1 ブロックでない）か差があれば停止条件。配置の実験では `llvm-objdump -d --no-show-raw-insn` で両方のループ先頭のアドレスを出し、64 で割った余りを記録する（`/opt/homebrew/opt/llvm@21/bin/llvm-objdump`）。

### 3. `apps` の条件

- `grep -nE "unsafe|get_unchecked" benchmarks/apps/reference.rs` が何も出さない。
- `grep -nE "ffast-math|Ofast|ffp-contract=fast" benchmarks/run-apps.mjs benchmarks/apps/*` が何も出さない。
- run-apps の artifacts の Rust の IR で、`spectral_norm` の和に `fmul fast`・`contract` が現れない（`grep -c "fast\|contract" /tmp/tz-px02/apps-art/apps_reference.ll` の結果を記録し、0 でなければ該当行を確認する）。

## テスト計画

### JavaScript テスト

- `tests/benchmark_report.mjs`: `owned` の 2 つの assert を、`std_option_owned` の行も `data-fastest` と `data-tsuzuri-performance` を持つことの assert に変える。他の assert は変えない。
- `run-apps.mjs` の `reference` は起動時に自己検査する: 全種目で `reference(name, 0, seed) === 0n`（seed は 0、42、2^64 - 1）。`map_count` の大きさ 1 は、仕様から手で求めた `len(map) = 1`・`len(set) = 1` を使う式と一致する。

### E2E

- `run-computations.mjs --quick`: 10 種目の 25 組を BigInt 参照と照合し、WASM の `tz_ce_*`・`tz_direct_*` も照合する。
- `run-managed.mjs --quick --no-report`: 37 種目の C#・JavaScript の checksum を照合する。
- `run-apps.mjs --quick`: 10 種目 × 4 実装の 25 組、WASM `-O0`・`-O3` の 25 組、tracked の `live == 0`。大きさ 25 組は `{0, 1, 2, 17, 257}` × seed `{0, 42, 2^64 - 1, 2^63, 2^63 - 1}`（`control/host.c` と同じ）。
- 既存の quick を実行している CI があれば同じ job に `run-apps.mjs --quick` を足す（`grep -rn "run-managed.mjs" .github 2>/dev/null` で確認。なければ足さない）。速度の閾値は置かない。

### 既存テストへの影響

- `tests/benchmark_report.mjs` の `owned` の assert（順位の除外をやめるため）。それ以外はなし。
- `run-managed.mjs` の出力の種目数が 36 から 37 になる。欄は変わらない。

### 性能

- 閾値なし。`report --baseline` の比較は PX01 の判定をそのまま使う。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/benchmarks.md` | `### 条件と読み方` | `std_option_owned` の「長さを直接計算する最適化基準」の文を、全実装が同じ連結をすること、定数長の版は `std_option_length` であることに置き換える |
| `docs/benchmarks.md` | `## コンピュテーション式の比較` の種目の表と直後の段落 | `std_option_owned` の説明を更新し、`std_option_length` の行を足す。「生成コードの確認」の 1 の表（実装ごとの `malloc`・`free`・`operator new` の数） |
| `docs/benchmarks.md` | `## 実アプリ型の比較`（新規。`## 現実的な次の指標` の直前） | 10 種目の定義の要約、D6 の条件差の表（文字列の表現、境界検査、Map／Set の構造、書式・解析のライブラリ）、コマンド、本計測の `report` の表、`peak_rss`・確保の表 |
| `docs/benchmarks.md` | `### integer128_mix の再計測`（新規。`## 制御構文の比較` の末尾） | 3 run の中央値・最小・最大、分類、配置の実験の結果（行った場合） |
| `README.md` | ベンチマークのコマンドの一覧（`grep -n "run-control.mjs" README.md`） | `node benchmarks/run-apps.mjs target/release/tsuzuri` を 1 行 |
| `_perfs/README.md` | `## 一覧`、`### 実行速度` | PX02 の状態を `done`。`integer128_mix`・`std_option_owned` の行に再計測の分類と日付を足す（2026-09-26 の記録は消さない） |

## 受け入れ条件

- [ ] `std_option_owned` の C++ が `std::u16string` の連結、Rust が `OwnedUtf16` の連結、C# が `static readonly string` の連結、JavaScript が `parts` の連結をしている。
- [ ] `std_option_length` が Tsuzuri（ce・direct）・C++・Rust・C#・JavaScript にあり、quick で照合される。
- [ ] `benchmarks/report.mjs` に `std_option_owned` の特別扱いがなく、`node tests/benchmark_report.mjs` が成功する。
- [ ] `integer128_mix` の 3 run の PX01 形式の結果と分類が文書にあり、「差が残る」なら配置の実験の結果もある。
- [ ] `apps` の 10 種目が 4 実装と WASM `-O0`・`-O3` で照合され、本計測の `apps.jsonl` が 140 行ある。
- [ ] `docs/benchmarks.md`・`README.md`・`_perfs/README.md` が「ドキュメント」のとおり更新されている。
- [ ] `git diff --stat -- src std` が空。
- [ ] GUIDE §10 の完了の定義を満たす。
- [ ] Phase 3（D7 の承認後）: `zig`・`go` の 80 行（`wall_time`・`cpu_time`・`peak_rss`・checksum 照合）が加わり、ツールのない計測機では runner が理由を表示して終了コード 0 で終わる。

## 落とし穴

- Tsuzuri: `String.split` と `String.join` は区切りが先（`std/String.tz` の `split separator text`）。逆に書いても型が合い、probe では `tokenize` が大きさに関係なく `0x20` を返した。
- Tsuzuri: 文字列リテラルは直接借用できない（`ref " "` は `E1013 borrow requires a local place`）。`let separator = " "` で束縛する。
- Tsuzuri: 配列の要素は変更できない（`E1012 … array elements, and list elements are immutable`）。書き換える列は `Vec` にして `Vec.set v i x` で更新する。`Vec.at` は `ref` を返すので値として使うと `E1003`。値は `v[i]` か `Vec.get (ref v) i`。
- Tsuzuri: `Array.filter` の述語は `ref 'a` を受け、値の lambda は `E1003`。`closure_pipeline` は `Array.fold` の中で条件を書く。
- Tsuzuri: 複数行の `else if` の連鎖は `E0002`。`if … then … else` の後に字下げした `match` を置く（tree の probe の形）。
- C++: libc++ の `std::u16string` は 10 単位まで確保しない（short string）。`"hello world"`（11）と `"hello world!"`（12）は確保するので、リテラルを短くしない。
- C#: `const string` どうしの `+` はコンパイル時に畳み込まれる。`static readonly` にする。JIT が畳み込んだ場合も結果として記録する。JavaScript の文字列は rope になりうる（既存の `### 条件と読み方` の注意と同じ）。
- Rust: std の staticlib は `--print native-static-libs` の出力をリンクに足さないと未解決の記号になる。出力は stderr に出る。整数は `wrapping_*` を明示し、`overflow-checks` の既定に頼らない。
- C: `f64` から整数への変換は範囲外と NaN が未定義。`spectral_norm` は `n == 0` を先に 0 で返し、`r * 1e15 < 2^64` を前提にする（`r` は約 1.27）。C の関数引数の評価順序は未規定なので、`build` の 2 つの子は別の文で順に作る。
- `run-managed.mjs` は `--quick` でも `--no-report` でもないとき `docs/benchmarks.md` の要約を書き換える。計測だけのときは `--no-report` を付ける。
- PX01 の `writeRecords` は `wx` で書くので、同じ run_id に同じ suite を二度書けない。測り直すときは新しい run_id を使う。
- Node 20 では BigInt の多い参照で V8 が停止することがある。`node24` を使う。
- `graph_bfs` の `%` は u64 の剰余（Tsuzuri は `i64u`）。符号付きの剰余にしない。

## 対象外

- 言語の総合的な順位付け、ベンチマークゲームへの投稿。
- `apps` の C#・JavaScript の実装と `run-managed.mjs` への統合（D8）。
- 慣用的なビュー（`&str`・`std::string_view`）を使う別の版。必要なら種目名に `_view` を付けて別チケットで足す。
- C09（HashMap）・C10（Arena）・C11（多次元配列）・D08（JSON）に依存する種目（k-nucleotide、Arena のグラフ、行列積と畳み込み、JSON の解析と出力）。各チケットの完了後に `apps` へ行を足す。
- 差を縮めるコンパイラ・std の最適化（PM06、PR03、PM03、PM05 が扱う）。
- `apps` の WASM の時間計測、種目ごとの成果物サイズ（PM08）、Go の `GOGC` などの調整値の版。

## 決定事項

### D1: Phase の範囲

- 決定: 実装者は Phase 1 と Phase 2 を実装する。Phase 3 は D7 の承認後だけ。
- 理由: Phase 1・2 は計測機の既存のツールだけで検証できる。Zig・Go は導入されておらず、動かせないコードを入れない。
- 状態: 既定案（実装者はこの案に従う）

### D2: `std_option_owned` の「同じ仕事」

- 決定: 全実装がソース上で同じ操作（2 回の連結、長さ、解放）を各言語の標準の所有文字列で行う。最適化の障壁（`black_box`、空の `asm`）で確保を強制しない。-O3 後に残る確保の数を実装ごとに記録する。
- 理由: 旧来の参照実装は仕事そのものを省いていた。同じソースから確保を消せるのはそのコンパイラの成果であり、差は PM06 の対象として数で示す。
- 状態: 既定案（実装者はこの案に従う）

### D3: 定数長の版を残す

- 決定: 旧来の参照実装の式を `std_option_length`（新規）として全実装に置き、Tsuzuri も `Maybe` で定数長を返す。
- 理由: 元のチケットの決定（確保しない版は別の種目名で残す）を保ち、確保の費用と `Maybe` の費用を分けて読めるようにする。
- 状態: 既定案（実装者はこの案に従う）

### D4: `integer128_mix` の再計測

- 決定: runner を変えない。`TSUZURI_CLANG` で Clang をそろえて 3 run 測り、「計測の揺れ」「配置」「未解明」に分類する。配置の実験は「計測手順」の手作業のコマンドだけで行う。
- 理由: 一度きりの切り分けのために runner に旗を足さない。ループの同一性は確認済みで、残る候補は配置と揺れである。
- 状態: 既定案（実装者はこの案に従う）

### D5: 系列 `apps` の構成

- 決定: `benchmarks/apps/` と `benchmarks/run-apps.mjs` を `control` と同じ形で作る。`run-managed.mjs` には入れない。`--baseline` を持たず、WASM は `-O0`・`-O3` の checksum の照合だけで時間を測らない。
- 理由: 既存の要約の形を変えずに足せる。before／after は PX01 の `report --baseline` で比べられる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 参照実装の規則

- 決定: 標準ライブラリだけで書く。アルゴリズムとデータ構造は種目定義にそろえる（Map／Set は整列配列、整列は `Array.sort_by` と同じ bottom-up merge、グラフは CSR）。Tsuzuri が所有値を作る箇所は参照実装も所有値を作る。文字列は各言語の標準表現（Tsuzuri は UTF-16、C／C++／Rust は bytes）。境界検査は C／C++ なし、Rust は既定の検査、Tsuzuri は言語の検査。書式・解析は各標準ライブラリ（C は `snprintf`・`strtoull`、C++ は `std::to_chars`・`std::from_chars`、Rust は `to_string`・`parse::<u64>`）。これらの差は文書の表に書く。
- 理由: 元のチケットの既定案（種目ごとに最小の参照実装、依存ライブラリなし）を具体化した。表現の差は Tsuzuri の設計の費用として測る対象である（PM03 の根拠になる）。
- 状態: 既定案（実装者はこの案に従う）

### D7: Zig と Go（Phase 3）

- 決定: `TSUZURI_ZIG`（新規、既定 `zig`）・`TSUZURI_GO`（新規、既定 `go`）で指定する。既定名で見つからなければ stderr に `zig not found (set TSUZURI_ZIG to its executable); skipping Zig references` を出し、stdout の JSON の `skipped` に `{ "zig": "not found" }` を記録して続ける。環境変数で明示した実行ファイルが実行できなければエラーで終える（AGENTS.md の明示要求の規則）。Zig は `zig build-obj -O ReleaseSafe -fPIC` で `export fn zig_<name>(size: i64, seed: u64) u64`、確保は `std.heap.c_allocator`、演算は `+%`・`*%`。Go は `go build -buildmode=c-archive` で `//export go_<name>`、`GOGC`・`GOMAXPROCS` は既定のままで `go version` と共に記録する。
- 理由: 計測機への導入は人間の作業で、Go の GC の条件の扱いも判断が要る。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D8: `apps` の C#・JavaScript

- 決定: このチケットでは作らない。
- 理由: `_perfs/README.md` の目標で C#・JavaScript は参考であり、`run-managed.mjs` の要約の形を変えずに済む。
- 状態: 既定案（実装者はこの案に従う）

### D9: 記録の形式

- 決定: PX01 形式（schema 1）。suite `apps`、language は `tsuzuri`・`c`・`cpp`・`rust`（Phase 3 で `zig`・`go`）、variant `default`、target `native`、opt `O3`。`peak_rss` は host の `--only` と `measureProcess`、確保は Tsuzuri だけ（PX01 D5）。残る確保の呼び出し数は文書の表だけに書く。
- 理由: PX01 の schema を変えずに記録できる。IR の静的な数は実行時の metric と意味が異なる。
- 状態: 既定案（実装者はこの案に従う）
