# PX03: ビルド速度ベンチマーク

| 項目 | 内容 |
| --- | --- |
| ID | PX03 |
| 分類 | 計測基盤 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | PX01 |
| 関連 | PB01〜PB07, G17 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。Phase 2 は要承認: D9（Go・Zig の計測機への導入と他言語版の生成器） |
| 手本にする既存実装 | 合成プロジェクトの生成と cache の比較: `benchmarks/run-cache.mjs`（`mkdtempSync` の一時ディレクトリ、`TSUZURI_CACHE_DIR` の隔離、`spawnSync`、成果物の byte 一致の確認）。プロセスの計測と記録: PX01 が作る `benchmarks/metrics.mjs` の process suite（`measureProcess`・`parseTimePasses`・`captureRun`・`makeRecord`・`writeRecords`、warm-up 1 回と 9 標本、成果物の大きさの一致確認、WASM の skip 規則） |
| 主な影響ファイル | `benchmarks/build/generate.mjs`（新規）, `benchmarks/run-build.mjs`（新規）, `tests/benchmark_report.mjs`（生成器のテストを追加）, `docs/benchmarks.md`, `README.md`, `_perfs/README.md`（「ビルド速度」の表の注記と状態欄）。Rust のソースは変えない（段階別時間は PX01 の `TSUZURI_TIME_PASSES` を使う）。`benchmarks/run-cache.mjs` は変えない（D8） |
| 計測対象 | 合成プロジェクト 200・1,000 モジュール（quick は 10）と実プロジェクト 11 個（D5）。操作は `check`、`build --emit llvm`、実行ファイル `-O0`・`-O3`、WASM `-O0`・`-O3`、cache あり（変更なし・本体の変更・公開 API の変更）。指標は経過時間・CPU 時間・段階別時間・最大 RSS・成果物の大きさ |

## 目的

「Go を上回る超高速ビルド」を判断するため、規模と構造をそろえたプロジェクトで、Tsuzuri のビルド時間とコンパイラのメモリを再現可能に計測し、PX01 形式で記録する。
段階別の内訳（フロントエンドの各段、cache、Clang・wasm-ld などの外部ツール）を分けて記録し、PB01〜PB07 と G17 の優先順位と効果を同じ物差しで判断できるようにする。
Go・Zig・Rust・C/C++ との比較は Phase 2 とし、実装者は Phase 1 だけを実装する。Phase 2 は人間が求め、D9 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の一覧の状態欄で done であること。確認: `grep -n "PX01\|PX03" _perfs/README.md`。
  さらに `node -e 'import("./benchmarks/metrics.mjs").then((m) => console.log(Object.keys(m).sort().join(",")))'` が
  `captureRun`・`makeRecord`・`measureProcess`・`parseTimePasses`・`writeRecords` を含み、
  `TSUZURI_TIME_PASSES=1 target/release/tsuzuri check examples/hello` の stderr の最後の行が `{"time_passes":1,` で始まること。
- GUIDE §2.3 の基準コマンドが成功すること。このチケットは Rust を変えないが、計測するコンパイラは
  `cargo build --release --locked` で作った `target/release/tsuzuri` に限る。
- 計測機に Apple Clang 21（`clang --version`）、`wasm-ld`（`TSUZURI_WASM_LD` または PATH）、Node 24 があること（GUIDE §2.1）。
- 本計測（`mode: "benchmark"`）は電源接続・他の重い処理なしの計測機で行う（PX01 の「環境と条件」）。

### 前提とする他チケットのインターフェース

PX01 の「他チケットへ提供するインターフェース」をそのまま使う。PX03 は `benchmarks/metrics.mjs` を import するだけで、最後の 1 点以外は変えない。

- `captureRun(directory, { compiler, mode, clang, rustc })` は run の文脈（`schema`・`run_id`・`commit`・`dirty`・`date`・`mode`・`host`）を返す。
- `measureProcess(command, args, { cwd, env, runs, warmups, timeoutMs })` は `/usr/bin/time` 経由で `runs` 回測り、
  `wall_ms`・`user_ms`・`sys_ms`・`peak_rss_bytes` の配列と、各標本の stdout・stderr を返す（PX01 の process suite が各標本の stderr を
  `parseTimePasses` で読むため）。0 以外の終了は throw する。
- `parseTimePasses(stderr)` は `{ phases: [{ name, ms }], total_ms }` を返す。段の名前は `load`・`parse`・`check`・`specialize`・
  `closures`・`ownership`・`emit`・`cache`・`total`・`tool.<name>`（`tool.clang`・`tool.wasm-ld` など）。同じ名前は合算済み。
- `makeRecord(context, fields)` と `writeRecords(directory, suite, records)` は PX01 形式（schema 1）のレコードを作り、
  `target/perf/<run_id>/<suite>.jsonl` に書く。
- PX03 が PX01 に加える変更は、suite 名の許可に `build` を足すことだけ（`validateRecord` が suite 名を列挙で検査している場合）。
  metric・欄・`LANGUAGES` は変えない（`LANGUAGES` には `go`・`zig` がすでにある）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. PX01 の `measureProcess` が各標本の stderr を返さない、または `TSUZURI_TIME_PASSES=1` の段の名前が上の一覧と違う。
   PX03 側で別の計時の仕組み（`Instant` の追加、stderr の独自解析）を作らない。
2. PX01 形式に欄や metric を足さないと記録できない情報が出た。schema を上げず、行数・IR の大きさは manifest
   （設計「記録」の `projects.json`）に書く。それでも足りなければ報告する。
3. 生成したプロジェクトが `check`・`build`（native `-O0`・`-O3`）・WASM `-O3` のどれかで失敗する、または実行ファイルの出力が
   生成器の参照値（BigInt の計算）と一致しない。コンパイラの不具合の可能性があるので、生成規則を変えて隠さない。
4. 同じ入力の 9 回のビルドで成果物の大きさか内容が異なる（ビルドの非決定性）。
5. 計測のために `src/` を変える必要が出た（計測専用の CLI オプション、隠し環境変数など）。
6. 利用者の既定の cache（macOS は `~/Library/Caches/tsuzuri/build-cache`）に書き込む経路が見つかった。
7. 本計測 1 回（quick でない run）が計測機で 30 分を超える見込みになった。

## 現状と計測（HEAD `f8dc655`）

### 既存の計測手段（コードで確認）

- ビルド時間を測るのは `benchmarks/run-cache.mjs` だけ。一時ディレクトリに `Module<i>.tz`（`def value :: i64` と `fn value = <i>`）を
  200 個（`--quick` は 10 個）と、それらを足す `Main.tz` を書き、`TSUZURI_CACHE_DIR` を一時ディレクトリの `cache` にして、
  `build <project> -o <output>` の `--no-cache` あり（uncached）となし（warm）を交互に 7 回（quick は 2 回）測り、中央値の ms を
  1 行の JSON で出す。段階別時間・最大 RSS・`-O3`・WASM・`check`・編集後の再ビルドはない。関数の中身が定数なので、実際のコードの
  コンパイル時間を表さない。
- whole-build cache の鍵は `src/cache.rs` の `build_key` が作る。鍵には `ir-and-embedded-runtime`（生成 IR の全文）が入るので、
  cache hit でも読み込み・検査・IR 生成は毎回走り、省けるのは Clang・リンカーと成果物の生成だけ。1 ファイルの本体を変えると IR が
  変わり、必ず cache miss になる（段単位・モジュール単位の再利用はない。G17 の範囲）。
- cache の場所は `src/cache.rs` の `default_root`: `TSUZURI_CACHE_DIR` が空でなければそれ、macOS は
  `~/Library/Caches/tsuzuri/build-cache`。計測は必ず `TSUZURI_CACHE_DIR` を一時ディレクトリに向ける（停止条件 6）。
- `--no-cache` は `build`・`run` だけが受け付け、`check` に付けると CLI エラーになる（`src/main.rs` のテストの
  `parse(&["check", "Main.tz", "--no-cache"]).is_err()`）。`check` は cache を使わない。
- `--emit` は `exe`・`object`・`llvm`・`header`・`wasm`・`wgsl`、最適化は `-O0`〜`-O3`、WASM は `--target wasm32`（`src/main.rs`）。
  `--emit llvm` も cache を使う（`src/driver.rs` の `build_complete` の `let cache = if options.cache && options.emit != Emit::Header {`）ので、
  `--no-cache` を付けて測る。
- 外部ツールは `src/driver.rs` の `tool`（環境変数と既定名から実行ファイルを決める）と `run_tool` で起動する:
  `tool("TSUZURI_CLANG", "clang")`・`tool("TSUZURI_WASM_LD", "wasm-ld")`・`tool("TSUZURI_LLVM_LINK", "llvm-link")`・
  `tool("TSUZURI_DSYMUTIL", "dsymutil")`。PX01 の `TSUZURI_TIME_PASSES` はこれらを `tool.<name>` の段として、フロントエンドの段と分けて出す。
- 段階別時間は HEAD にはない（PX01 が `src/timings.rs`（PX01 で新規）で加える）。最大 RSS は `/usr/bin/time -l` の
  `maximum resident set size` で、子プロセスのうち最大の 1 つを含む（PX01 の `peak_rss` の定義）。

### 2026-09-29 の参考値（`--no-cache`、各 1 回、Apple M1 Max）

`_perfs/README.md` の「ビルド速度」と同じ値。合成プロジェクトの生成器はリポジトリになく、同じ入力を再現できない。
PX03 の生成器（設計）は関数の形が違うので、下の合成 2 行と PX03 の結果を直接比べない。

| 対象 | 行数 | check | IR 出力 | 実行ファイル `-O0` | 実行ファイル `-O3` | コンパイラの最大 RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `examples/hello` | 10 | 0.05 s | 0.02 s | 0.33 s | 0.53 s | – |
| 合成 200 モジュール | 24,803 | 0.39 s | 0.46 s | 1.26 s | 2.05 s | 237 MB |
| 合成 1,000 モジュール | 124,003 | 2.12 s | 2.58 s | 約 6.2 s | 約 10.1 s | 1.12 GB |

- hello の実行ファイルでは、数値ランタイム（`src/runtime/numeric.ll`、464 KB）全体を IR に含め、その Clang `-O3` だけで 0.43 s かかる（PB01 の対象）。
- 合成 1,000 モジュールでは Clang の処理が `-O0` で 3.70 s、`-O3` で 7.62 s。IR は 29 MB。フロントエンドは単一スレッドで約 5 万行/s（PB02・PB04 の対象）。
- 当時の合成コードは入力が定数で、`-O3` では計算全体が定数に畳み込まれた。`-O3` のビルド時間が実際のコードを表さない。
- Go と Zig は計測機に未導入。C の hello は Clang で 0.07 s。

hello の値の再現（PX01 の完了前でも動く。段階別時間はない）:

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-px03
/usr/bin/time -l target/release/tsuzuri check examples/hello
/usr/bin/time -l target/release/tsuzuri build examples/hello --emit llvm -O0 --no-cache -o /tmp/tz-px03/hello.ll
/usr/bin/time -l target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-px03/hello-O0
/usr/bin/time -l target/release/tsuzuri build examples/hello -O3 --no-cache -o /tmp/tz-px03/hello-O3
```

### 実プロジェクトの状態（2026-09-29 に確認）

`for d in examples/* std benchmarks/control benchmarks/cpp benchmarks/computations; do target/release/tsuzuri check $d; done` の結果:

- `check` が成功する（`Main.tz` を持つ）: `examples/computations`・`control`・`currying`・`functional`・`hello`・`io`・`point`・
  `polymorphism`・`tasks`、`benchmarks/control`・`benchmarks/computations`。どれも 0.1 s 未満。
- `Main.tz` のないディレクトリ（`examples/desktop`・`gpu`・`native`・`web`、`benchmarks/cpp`）は `E2001`（`cannot inspect source '<dir>/Main.tz'`）。
- `std` はプロジェクトとして検査できない（`check std/List.tz` は `E1011`: `module name 'Array' is reserved for the standard library; rename the file`）。
  標準ライブラリのコストは、それを使うプロジェクトの `load`・`check` などの段に含まれる（D5）。

### 生成コードの形の確認（2026-09-29 に確認）

設計の「関数の形」の 10 種類を 1 モジュールに書き、`Main.tz` から stdin の行の長さを種にして呼ぶプロジェクトを `/tmp/tz-work-PX03/syn/` で確認した。
`check` 成功、`build -O3` の実行ファイルは入力 `12345` で `1130048866073530410`、空行で `6527793999603744154`（入力で値が変わり、
定数に畳み込まれていない）。`--target wasm32 -O3` の build も成功した。使った構文は `private record`・`private union`・
`private def`・`def rec`・`Add<'a>` の多相関数・`while`・`for i in 0 .. n`・`for value in values`・`match`・closure・
`new [i64u](n, \i -> ...)`・`to_string`・`String.length`・`IO.read_line`。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で記録し、PB・G17 の before／after の判断材料にする。
すべて PX01 形式の `suite: "build"`（新規）のレコードで、統計は warm-up 1 回を除く 9 標本の中央値・最小・最大（大きさだけ 1 標本）。

| 指標 | metric（単位） | variant・opt | 使うチケット |
| --- | --- | --- | --- |
| B1 初回ビルド | `wall_time`（ms）、`cpu_time`・`user_time`・`sys_time`（ms） | `build` の `O0`・`O3`、`build-wasm` の `O0`・`O3`（すべて `--no-cache`） | PB01・PB03・PB05 |
| B2 IR まで | 同上 | `emit-llvm`（`O0`、`--no-cache`） | PB02・PB04 |
| B3 検査 | 同上。行/s は `projects.json` の `lines` ÷ 中央値（秒）で要約にだけ出す | `check` | PB02・PB04・G17 |
| B4 変更なしの再ビルド | 同上 | `build-warm`（`O0`、cache hit） | PB06・G17 |
| B5 本体だけの変更 | 同上 | `build-edit-body`（`O0`、合成だけ） | PB07・G17 |
| B6 公開 API の変更 | 同上 | `build-edit-signature`（`O0`、合成だけ） | G17 |
| B7 段階別時間 | `phase_time`（ms、`phase` 欄に段の名前） | 全 variant | PB01〜PB07 |
| B8 最大 RSS | `peak_rss`（bytes） | 全 variant。コンパイラ単体の値は `check` と `emit-llvm` | PB02 |
| B9 成果物 | `executable_bytes`・`wasm_bytes`（bytes、1 標本） | `build`・`build-wasm` | PM08 |

- コンパイラ本体と外部ツールの分け方（B7）: 標本ごとに `frontend`＝`load`・`parse`・`check`・`specialize`・`closures`・
  `ownership`・`emit` の和、`cache`＝`cache` の段（なければ 0）、`tools`＝`tool.` で始まる段の和、`other`＝`total` − 3 つの和
  （引数の解析、一時ディレクトリ、成果物の書き出し）を計算し、それぞれの中央値を要約に出す。レコードとして保存するのは PX01 の
  `phase_time` だけで、4 つの和は保存しない（`readRecords` から再計算できる）。
- 最大 RSS（B8）は子プロセスのうち最大の 1 つを含む（PX01 の定義）。`build` の値が Clang の最大 RSS になりうるので、
  コンパイラ単体の値は外部ツールを起動しない `check` と `emit-llvm` で読む（`emit-llvm` に `tool.` の段がないことを runner が確かめる）。
- 記録の目標: デバッグビルド（B1 の `O0`）の時間を同じ規模の Go と比べて差を毎回記録する。比較は Phase 2 で、合否は判定しない。

## 変えてはいけない意味

- コンパイラの成果物・stdout・stderr・終了コード・診断。PX03 は `src/` を変えず、通常の CLI だけを使う（停止条件 5）。
  `TSUZURI_TIME_PASSES=1` は stderr の最後に 1 行を足すだけ（PX01 の保証）。runner は 9 標本の成果物の SHA-256 が同じことを確かめる。
- リポジトリの内容。生成したプロジェクトは `mkdtempSync(join(tmpdir(), "tsuzuri-build-bench-"))` の下に置き、`finally` で消す。
  リポジトリへ書くのは `target/perf/<run_id>/build.jsonl` と `projects.json` だけ。実プロジェクトは読むだけで、一時ディレクトリへ複製しない。
- 利用者の cache。すべての実行で `TSUZURI_CACHE_DIR` を一時ディレクトリに向ける。
- 生成器の決定性。同じ引数から byte 単位で同じファイルを作る（`Date`・`Math.random`・環境・OS に依存しない。改行は LF、最後に改行 1 つ）。
- 生成コードの意味。整数は `i64u` だけを使い、加算・乗算は 2^64 を法とする wrap（トラップしない）。除数と法は 2 以上の定数で、
  0 除算・範囲外添字・`assert` はない。出力は生成器の BigInt の参照値と一致する（停止条件 3）。
- WASM の既定の import。合成プログラムは stdin を読むので `tsuzuri_io` を import するが、それはプログラムの性質で、既定は変えない。
  PX03 は WASM を build するだけで実行しない。
- CI に速度の閾値を入れない。`--quick` は正しさだけを確かめる。

## 設計

### 全体の構成

- `benchmarks/build/generate.mjs`（新規）: 合成プロジェクトの生成器。純粋関数と、ファイルに書く CLI。Tsuzuri のコンパイラを使わない。
- `benchmarks/run-build.mjs`（新規）: 生成・正しさの確認・計測・記録・要約。計測と記録は PX01 の `benchmarks/metrics.mjs` の関数を使う。
- `benchmarks/run-cache.mjs` はそのまま残す（D8）。PX01 の process suite とも別の suite で、互いのレコードを比べない。

### 生成器 `benchmarks/build/generate.mjs`（新規）

新 API（実装後に有効。未検証）:

```javascript
export const FUNCTIONS_PER_MODULE = 20;
export function moduleSource(index, { salt = 0, extra = null } = {}) {} // Module<index>.tz の全文
export function mainSource(modules) {}                                   // Main.tz の全文
export function generateProject(modules) {}  // [[fileName, text], ...]（fileName の昇順。Main.tz と Module0.tz〜）
export function writeProject(directory, modules) {} // ファイルを書き manifest { modules, files, lines, bytes } を返す
export function expectedOutput(modules, line) {}    // stdin の 1 行目が line のときの stdout（10 進 + "\n"）
```

- `modules` は 1 以上 5,000 以下の整数（それ以外は throw）。`writeProject` は `mkdirSync(directory)`（`recursive` なし）で、
  既存のディレクトリには書かない。`lines` は全ファイルの改行の数、`bytes` は UTF-8 の byte 数の和。
- CLI: `node benchmarks/build/generate.mjs --modules <N> --out <dir>` が `writeProject` を呼び、manifest を 1 行の JSON で stdout に出す。
- `Module<k>.tz` の構成（k は 0 始まり）: `private record Pair`、`private union Shape`、`private def twice`、`private def rec count`、
  関数 `f0`〜`f19`（すべて `private`、型は `i64u -> i64u`）、公開の `run`。関数 `fj` の形は `j % 10` の種類で、定数は
  `c = k * 20 + j + 3`（`f0` だけ `c + salt`）。`extra` が数 n のとき、末尾に公開の `def extra_<n> :: i64u -> i64u = \x -> x + <n>i64u` を足す。
- 同じ名前（`Pair`・`run` など）をすべてのモジュールで使う（モジュールが名前空間になる）。定数がモジュールごとに違うので、同じ本文の関数はない。

テンプレート（`{c}` などは生成器が置き換える。形は 2026-09-29 に別の定数で確認済み。「現状と計測」）:

```text
private record Pair { left: i64u, right: i64u }

private union Shape =
    | Dot of i64u
    | Span of i64u * i64u
    | Empty

private def twice :: Add<'a> -> 'a = \v -> v + v

private def rec count :: i64u -> i64u -> i64u = \n acc -> if n == 0 then acc else count (n - 1) (acc + n)

private def f{j} :: i64u -> i64u = \x ->          -- 種類 0（本文は下の表）
    let mut total = x
    let mut index = 0i64u
    while index < 8 do
        total = total * 6364136223846793005 + index + {c}
        index = index + 1
    total

def run :: i64u -> i64u = \seed ->
    let mut state = seed
    state = f0 state
    ...                                              -- f1〜f18 を同じ形で 1 行ずつ
    state = f19 state
    state
```

| 種類 | 本文（`x` は引数） | 参照値（BigInt、2^64 を法、`/`・`%` は符号なし） |
| --- | --- | --- |
| 0 `while` | 上のテンプレート | `t = x` から `i = 0..7` で `t = t * 6364136223846793005 + i + c` |
| 1 `for` | `let mut total = x`、`for i in 0 .. 7 do total = total * 31 + (i as i64u) + {c}`、`total` | `t = x` から `i = 0..7`（両端を含む 8 回）で `t = t * 31 + i + c` |
| 2 `match` | `match x % 4 with \| 0 -> x + {c} \| 1 -> x * 3 \| 2 -> x + {c+4} \| _ -> x / 2` | 同じ場合分け |
| 3 record | `let pair = Pair { left: x, right: x + {c} }`、`pair.left * pair.right` | `x * (x + c)` |
| 4 union | `x % 3` が 0 なら `Dot x`、1 なら `Span (x, x + {c})`、2 なら `Empty`。`match` で `v`・`a * b`・`x + {c+2}` | `[x, x * (x + c), x + c + 2][x % 3]` |
| 5 多相 | `twice x + twice {c}i64u` | `2x + 2c` |
| 6 closure | `let offset = x % {c}`、`let add = \v -> v + offset`、`add (add x)` | `x + 2 * (x % c)` |
| 7 文字列 | `let text = "m{k}-" + to_string x`、`x + (String.length text as i64u)` | `x + ("m" + k + "-" の長さ) + (x の 10 進の桁数)` |
| 8 配列 | `let values = new [i64u](8, \i -> (i as i64u) * x + {c})`、`for value in values do` で和 | `28x + 8c` |
| 9 再帰 | `count (x % 16) x` | `n = x % 16` として `x + n(n+1)/2` |

`Main.tz` は、`private def total :: i64u -> i64u`（`let mut sum = 0i64u` と `sum = sum + Module<k>.run seed` を k の昇順に 1 行ずつ、
最後に `sum`）と、`def main :: IO<unit>`（`IO.read_line ()`、`Option.default_value "" line`、`String.length text as i64u` を種にして
`IO.write_line (to_string (total seed))`）。`expectedOutput` は `seed` を `line` の UTF-16 の長さとして同じ計算をする。
式を深く入れ子にしない（1,000 項の `+` の連鎖は構文の深さの上限に当たりうる。未確認なので避ける）。1 モジュールは約 135 行で、1,000 モジュールは約 135K 行。

### 実プロジェクト

`REAL_PROJECTS`（新規、runner の定数）は「現状と計測」で `check` が成功した 11 個: `examples/computations`・`control`・`currying`・
`functional`・`hello`・`io`・`point`・`polymorphism`・`tasks`、`benchmarks/control`・`benchmarks/computations`。
`std` は単独で計測しない（D5）。実プロジェクトは読むだけで、編集の variant（B5・B6）は測らない。

### 操作（variant）

P は project、T は一時ディレクトリ。環境変数はすべて `{ ...process.env, TSUZURI_TIME_PASSES: "1", TSUZURI_CACHE_DIR: join(T, "cache") }`。

| variant | コマンド | target | opt | cache |
| --- | --- | --- | --- | --- |
| `check` | `tsuzuri check P` | native | none | 使わない |
| `emit-llvm` | `tsuzuri build P --emit llvm -O0 --cpu generic --no-cache -o T/app.ll` | native | O0 | 無効 |
| `build` | `tsuzuri build P -O0 --cpu generic --no-cache -o T/app-O0` | native | O0 | 無効 |
| `build` | `tsuzuri build P -O3 --cpu generic --no-cache -o T/app-O3` | native | O3 | 無効 |
| `build-wasm` | `tsuzuri build P --target wasm32 -O0 --no-cache -o T/app-O0.wasm` | wasm32 | O0 | 無効 |
| `build-wasm` | `tsuzuri build P --target wasm32 -O3 --no-cache -o T/app-O3.wasm` | wasm32 | O3 | 無効 |
| `build-warm` | `tsuzuri build P -O0 --cpu generic -o T/app-warm` | native | O0 | hit（warm-up が作る） |
| `build-edit-body` | 標本 s の前に `Module<m>.tz` を `moduleSource(m, { salt: s })` で書き直し、`build-warm` と同じコマンド | native | O0 | miss |
| `build-edit-signature` | 標本 s の前に `moduleSource(m, { extra: s })` で書き直し、同じコマンド | native | O0 | miss |

- m は `Math.floor(modules / 2)`。s は warm-up を 1、標本を 2〜10 とし、run の中で同じ内容を二度作らない（同じ内容に戻すと cache hit になる）。
  編集の variant の後は `moduleSource(m)` で元に戻す。
- `build-warm`・編集の variant の前に `rmSync(join(T, "cache"), { recursive: true, force: true })` で cache を空にする。
- 順序は表の順。各組で warm-up 1 回の後に 9 標本を続けて測る。標本ごとに処理を挟むため、`measureProcess` を
  `{ runs: 1, warmups: 0 }` で 10 回呼び、1 回目を捨てる（全 variant で同じ経路にする）。
- 標本ごとの確認: `--no-cache` の `build`・`build-wasm` は成果物の SHA-256 が 9 標本で同じ（停止条件 4）。`build-warm` の標本は
  `tool.` の段を含まず、warm-up は `tool.clang` を含む（hit と miss の確認）。編集の variant は全標本が `tool.clang` を含む。
  `emit-llvm` は `tool.` の段を含まない。
- 実プロジェクトの warm-up が 0 以外で終わった組は記録せず、stderr に `skip <P> <variant> <opt>: <stderr の 1 行目>` を出して続ける
  （例: `main` のない `benchmarks/control` の実行ファイル）。合成プロジェクトの失敗は throw（停止条件 3）。

### 正しさの確認（計測の前）

合成プロジェクトごとに `build -O0` と `build -O3`（`--no-cache`）を作り、stdin `12345\n` と空の stdin（`""`）で実行し、stdout が
`expectedOutput(modules, "12345")`・`expectedOutput(modules, "")` と一致することを `assert.equal` で確かめる。WASM は実行しない。

### 記録の形式

- レコードは PX01 の `makeRecord` で作り、`writeRecords(out, "build", records)` で `target/perf/<run_id>/build.jsonl` に書く。
  欄: `suite: "build"`、`workload` は合成なら `synthetic`、実プロジェクトならリポジトリ根からの相対パス、`size` は合成のモジュール数
  （実プロジェクトは `null`）、`language: "tsuzuri"`、`variant`・`target`・`opt` は上の表、`cpu_mode: "generic"`。
- metric は全 variant で `wall_time`・`cpu_time`（user + sys）・`user_time`・`sys_time`・`peak_rss`・`phase_time`（段ごとに 1 レコード、
  `total` を含む）。`build` は `executable_bytes`、`build-wasm` は `wasm_bytes` も記録する。9 標本で段の名前の集合が違えば throw。
- `projects.json`（新規、同じディレクトリ、`flag: "wx"`）: 合成の規模ごとに `writeProject` の manifest と `ir_bytes`（`emit-llvm` の出力の
  大きさ）、実プロジェクトごとに `.tz`・`.tc`・`.tt` の `files`・`lines`・`bytes` と `ir_bytes`。PX01 形式ではない補助の記録。
- 要約: 最後に stdout へ Markdown の表を 1 つ出す（workload・size・variant・target・opt・wall・frontend・cache・tools・other・peak RSS の中央値、
  `check` の行/s）。`report.txt` などのファイルは書かない（必要なら `> "$RUN/breakdown.md"` でリダイレクトする）。

runner の CLI（新 API、実装後に有効。未検証）:

```sh
node benchmarks/run-build.mjs [compiler] --out <dir> [--quick] [--sizes <n>[,<n>...]]
```

- `compiler` の既定は `target/release/tsuzuri`。`--out` は必須で、`captureRun(out, { compiler, mode, ... })` に渡す。
- `--sizes` の既定は `200,1000`、`--quick` の既定は `10`。`--quick` は `mode: "quick"`、warm-up 0 回・1 標本（`build-warm` だけは
  cache を作るビルドを warm-up として 1 回行う）、実プロジェクトは `examples/hello` だけ。
- 1 プロセスの timeout は 600,000 ms。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 生成 | `benchmarks/build/generate.mjs`（新規） | `FUNCTIONS_PER_MODULE`・`moduleSource`・`mainSource`・`generateProject`・`writeProject`・`expectedOutput`・CLI | 上の「生成器」 |
| 計測 | `benchmarks/run-build.mjs`（新規） | `REAL_PROJECTS`・`VARIANTS`・`measureVariant`・`verifySynthetic`・`breakdown`（すべて新規） | 上の「操作」「正しさの確認」「記録」。PX01 の関数を import する |
| 記録 | `benchmarks/metrics.mjs` | `validateRecord` | suite 名を列挙で検査している場合だけ `build` を足す。それ以外は変えない |
| テスト | `tests/benchmark_report.mjs` | 生成器のテスト（テスト計画） | Tsuzuri のコンパイラを使わないテストを足す |
| 文書 | `docs/benchmarks.md`・`README.md`・`_perfs/README.md` | 「ドキュメント」 | コマンドと読み方 |
| コンパイラ | `src/` | なし | 変えない |

### Phase 分割

- Phase 1（実装対象）: 上のすべて。Tsuzuri だけを計測する。
- Phase 2（人間が求め、D9 が承認された場合だけ。設計方針）: 生成器に言語の引数を足し、同じ 10 種類の関数・同じ参照値の C（モジュールごとの
  `.c` と `.h`、`clang -O0`／`-O3` と `make -j<cores>`）、Rust（1 crate と、モジュールごとの crate を持つ workspace の 2 通り。D10）、Go
  （モジュールごとの package、`go build` と `-gcflags=all=-N -l`）、Zig（`zig build-exe -O Debug`／`-O ReleaseFast`）のプロジェクトを作る。
  `language` 欄は PX01 の `LANGUAGES` の値を使い、ツールがない言語は `skip` を出して続ける。

## 実装手順

各手順の後で `node tests/benchmark_report.mjs` が成功する。Rust を変えないので `cargo test` は手順 1 と 12 だけで実行する。
Node は 24 を使う（`npx --yes --package=node@24 node ...`。以下の `node` はこれに読み替える）。

1. ベースライン
   - 変更: なし。
   - 内容: 着手条件を確かめる。GUIDE §2.3 の基準コマンドを実行する。
   - 確認: `node tests/benchmark_report.mjs` と `node benchmarks/run-cache.mjs target/release/tsuzuri --quick` が成功し、後者は
     `"modules":10` を含む 1 行の JSON を出す。`TSUZURI_TIME_PASSES=1 target/release/tsuzuri build examples/hello -O0 --no-cache -o /tmp/tz-px03/hello`
     の stderr の最後の行が `tool.clang` を含む。
2. 生成器の純粋関数
   - 変更: `benchmarks/build/generate.mjs`（新規）。
   - 内容: `FUNCTIONS_PER_MODULE`・`moduleSource`・`mainSource`・`generateProject`・`expectedOutput`（設計「生成器」）。
   - 確認: `node -e 'import("./benchmarks/build/generate.mjs").then((g) => console.log(g.generateProject(2).map(([n]) => n).join(",")))'` が
     `Main.tz,Module0.tz,Module1.tz`。
3. 生成器のテスト
   - 変更: `tests/benchmark_report.mjs`。
   - 内容: テスト計画の JavaScript テスト 1〜6。
   - 確認: `node tests/benchmark_report.mjs` が成功し、足したテストがそれぞれ 1 回ずつ実行される（テスト名の出力で確かめる）。
4. `writeProject` と CLI、3 モジュールの確認
   - 変更: `benchmarks/build/generate.mjs`。
   - 内容: 設計の CLI。
   - 確認: 次の 2 つの出力行が同じ数。`diff -r` が何も出さない。`check` が何も出さずに成功する（警告なし）。

     ```sh
     cd /Users/tmidorikawa/Documents/git/Tsuzuri
     mkdir -p /tmp/tz-px03 && rm -rf /tmp/tz-px03/g3 /tmp/tz-px03/g3b
     node benchmarks/build/generate.mjs --modules 3 --out /tmp/tz-px03/g3
     node benchmarks/build/generate.mjs --modules 3 --out /tmp/tz-px03/g3b
     diff -r /tmp/tz-px03/g3 /tmp/tz-px03/g3b
     target/release/tsuzuri check /tmp/tz-px03/g3
     target/release/tsuzuri build /tmp/tz-px03/g3 -O3 --no-cache -o /tmp/tz-px03/g3-O3
     printf '12345\n' | /tmp/tz-px03/g3-O3
     node -e 'import("./benchmarks/build/generate.mjs").then((g) => process.stdout.write(g.expectedOutput(3, "12345")))'
     ```

5. 1,000 モジュールの確認
   - 変更: なし。
   - 内容: 手順 4 を `--modules 1000` で行い、`-O0`・`-O3`・`--target wasm32 -O3` を build する。
   - 確認: 実行ファイル 2 つの出力が `expectedOutput(1000, "12345")` と同じ。WASM の build が成功する。manifest の `lines` が
     130,000〜140,000（設計の約 135 行 × 1,000）。外れたら生成器のテンプレートを見直す。どれかのビルドが失敗したら停止条件 3。
6. runner の骨組みと正しさの確認
   - 変更: `benchmarks/run-build.mjs`（新規）、必要なら `benchmarks/metrics.mjs` の `validateRecord`（suite `build` の許可）。
   - 内容: CLI の解析、`captureRun`、一時ディレクトリ、`verifySynthetic`（設計「正しさの確認」）。まだ計測しない。
   - 確認: `node benchmarks/run-build.mjs target/release/tsuzuri --quick --out /tmp/tz-px03/q6` が成功する。
     `expectedOutput` を一時的に 1 だけずらすと `assert.equal` で失敗する（確かめたら戻す）。
7. 合成プロジェクトの計測と記録
   - 変更: `benchmarks/run-build.mjs`。
   - 内容: `VARIANTS`・`measureVariant`、標本ごとの確認、`makeRecord`・`writeRecords`、`projects.json`。
   - 確認: `node benchmarks/run-build.mjs target/release/tsuzuri --quick --out /tmp/tz-px03/q7` が成功し、
     `node -e 'import("./benchmarks/metrics.mjs").then((m) => { const r = m.readRecords("/tmp/tz-px03/q7"); console.log(new Set(r.map((x) => x.variant + "/" + x.opt)).size) })'`
     が 9（`check/none`・`emit-llvm/O0`・`build/O0`・`build/O3`・`build-wasm/O0`・`build-wasm/O3`・`build-warm/O0`・`build-edit-body/O0`・`build-edit-signature/O0`）。
     `build-warm` の標本の `phase_time` に `tool.clang` の段がない。
8. 実プロジェクト
   - 変更: `benchmarks/run-build.mjs`。
   - 内容: `REAL_PROJECTS`、skip の規則。quick は `examples/hello` だけ。
   - 確認: quick の run に `workload: "examples/hello"` のレコードがある。`--sizes 10` の非 quick の run（手元の確認用、`mode: "benchmark"` だが
     公開しない）で `benchmarks/control` の `build` が `skip benchmarks/control build O0: ...` を出し、run 全体は成功する。
9. 要約
   - 変更: `benchmarks/run-build.mjs`。
   - 内容: `breakdown`（設計「記録」の要約）。
   - 確認: quick の stdout の最後が Markdown の表。`breakdown` は標本ごとに `frontend + cache + tools + other === total_ms` を assert し
     （`other` の定義から成り立つ）、`other` が負なら throw する（段が入れ子になっている。PX01 の規則違反として報告する）。
10. 生成コードの確認
    - 変更: なし。
    - 内容: 「生成コードの確認」を行う。
    - 確認: そこに書いた期待どおり。
11. 本計測と文書
    - 変更: `docs/benchmarks.md`・`README.md`・`_perfs/README.md`。
    - 内容: 「計測手順」の本計測を 1 回行い、「ドキュメント」を書く。
    - 確認: `node scripts/check-docs.mjs docs/benchmarks.md` が成功する（対象外のパスなら `git diff --check` だけ）。`git diff --stat` が
      主な影響ファイルだけを示す。
12. 最終確認
    - 変更: なし。
    - 確認: `node tests/benchmark_report.mjs`、`node benchmarks/run-build.mjs target/release/tsuzuri --quick --out /tmp/tz-px03/final`、
      `cargo test --locked`（Rust を変えていないことの確認）が成功する。`git status --porcelain` に `src/` の変更がない。

## 計測手順

### 環境と条件

PX01 の「環境と条件」に従う（電源接続、他の重い処理なし、`captureRun` が host・commit・dirty を記録）。`git status --porcelain` が空の
tree で、`cargo build --release --locked` の直後のコンパイラを測る。Clang・wasm-ld・Node の版は `host` に入る（wasm-ld の版は入らないので
`wasm-ld --version` を完了報告に書く）。

### 本計測（1 run）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
test -z "$(git status --porcelain)" && echo clean
RUN=target/perf/PX03-$(date -u +%Y%m%dT%H%M%SZ)
npx --yes --package=node@24 node benchmarks/run-build.mjs target/release/tsuzuri --out "$RUN" > /tmp/tz-px03/breakdown.md
mv /tmp/tz-px03/breakdown.md "$RUN/breakdown.md"
npx --yes --package=node@24 node benchmarks/metrics.mjs report "$RUN" > "$RUN/report.txt"
```

- 標本: 各組で warm-up 1 回と 9 標本。統計は中央値・最小・最大（PX01 の `summarize`）。
- 所要時間の見込み（2026-09-29 の参考値からの計算）: 1,000 モジュール約 8 分、200 モジュール約 2 分、実プロジェクト約 7 分。
  30 分を超えそうなら停止条件 7。
- 広がりの確認: 同じコンパイラで 2 回目の run を取り、`node benchmarks/metrics.mjs report "$RUN2" --baseline "$RUN"` で時間の metric に
  `改善`・`悪化` が出ないことを確かめる（出た metric は環境の揺れが大きく、判断に使わない。PX01 の「before／after」）。

### PB・G17 の before／after

PX01 の「before／after（他チケットの共通手順）」に従い、`--out target/perf/<ID>-before`・`<ID>-before2`・`<ID>-after` で 3 回測る。
変更前のコンパイラは別の作業ディレクトリで build し、`node benchmarks/run-build.mjs <baseline compiler> --out ...` で測る。
対象を絞るときは `--sizes 1000` を使い、実プロジェクトは減らさない（数分で終わる）。

### 記録

- 生データの置き場所と公開は PX01 D2 に従う（`target/perf/<run_id>/` の `build.jsonl`・`projects.json`・`breakdown.md`・`report.txt`）。
- `docs/benchmarks.md` の `## ビルド速度`（新規）に、公開する run の run_id・commit・日付・host と `breakdown.md` の表を貼る。

## 生成コードの確認

PX03 はコンパイラを変えないので、確かめるのは「合成コードが `-O3` で定数に畳み込まれていない」ことと「成果物が決定的」なこと。

1. 入力による出力の違い（`verifySynthetic` が毎回確かめる）: 手順 4 の `g3-O3` で、`printf '12345\n' | /tmp/tz-px03/g3-O3` と
   `printf '\n' | /tmp/tz-px03/g3-O3` の出力が異なり、それぞれ `expectedOutput(3, "12345")`・`expectedOutput(3, "")` と一致する。
2. 機械語: 乗算が残っていること。

   ```sh
   /opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn /tmp/tz-px03/g3-O3 | grep -cE '\s(mul|madd|umulh)\s'
   ```

   期待: 1 以上（arm64。種類 0・1・3・4 の乗算）。0 なら計算が畳み込まれているので停止して報告する。
3. 決定性: 同じ project の `-O3` を 2 回 build し、`shasum -a 256` が同じ（runner は 9 標本で確かめる。違えば停止条件 4）。

## テスト計画

### JavaScript テスト（`tests/benchmark_report.mjs` に追加。コンパイラ不要）

1. 決定性: `generateProject(3)` を 2 回呼んだ結果が `assert.deepEqual` で同じ。ファイル名が `Main.tz`・`Module0.tz`・`Module1.tz`・`Module2.tz`。
2. 構造: 各モジュールに `private def f<j> :: i64u -> i64u` が j = 0〜19 でちょうど 1 回ずつ、`def run :: i64u -> i64u` が 1 回。
   全モジュールの行数が同じ。`moduleSource(1)` が `"m1-"` を含み、タブと行末の空白を含まず、`\n` で終わる。
3. 編集: `moduleSource(1, { salt: 5 })` と `moduleSource(1)` の違いは 1 行だけ（`f0` の定数）。`moduleSource(1, { extra: 7 })` は
   `moduleSource(1)` の後ろに `def extra_7 :: i64u -> i64u = \x -> x + 7i64u\n` だけを足したもの（先頭の空行の有無も含めて固定する）。
4. Main: `mainSource(3)` が `Module0.run seed`・`Module1.run seed`・`Module2.run seed` をこの順で 1 行ずつ含み、`IO.read_line` を含む。
5. 範囲: `generateProject(0)`・`generateProject(5001)`・`generateProject(1.5)` が throw。`writeProject` は既存のディレクトリで throw し、
   新しいディレクトリでは manifest の `lines` が書いたファイルの `\n` の総数と同じ。
6. 参照値の性質: `expectedOutput(2, "")` と `expectedOutput(2, "12345")` が異なり、どちらも `^\d+\n$` に一致し、値が 2^64 未満。
   値そのものの正しさは、独立な実装（Tsuzuri のコンパイラ）との照合（E2E）で確かめる。

### E2E

- `node benchmarks/run-build.mjs target/release/tsuzuri --quick --out /tmp/tz-px03/quick`: 10 モジュールの `-O0`・`-O3` の実行ファイルの出力を、
  2 つの入力で BigInt の参照値と照合する（2 つの実装の照合）。全レコードが `readRecords` の検証を通る。
- 手順 5 の 1,000 モジュールの照合を実装時に 1 回行う（CI では行わない）。

### 既存テストへの影響

なし。`tests/benchmark_report.mjs` には追加だけで、PX01 のテストの期待値を変えない。`benchmarks/run-cache.mjs` の出力も変えない。

### 性能

閾値は置かない。本計測の結果は「計測手順」の記録として残し、CI では `--quick` の正しさだけを見る。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/benchmarks.md` | `## ビルド速度`（新規、`## 現実的な次の指標` の前） | コマンド、variant の表、`frontend`・`cache`・`tools`・`other` の読み方、最大 RSS は `check`・`emit-llvm` で読むこと、初回の本計測の表（run_id・commit・host）、2026-09-29 の参考値と生成規則が違うこと |
| `docs/benchmarks.md` | `## 表の読み方` | `run-cache.mjs` の行の後に `run-build.mjs` を 1 行 |
| `README.md` | ベンチマークのコマンドを並べた箇所（`grep -n "run-control.mjs" README.md` で探す） | `node benchmarks/run-build.mjs target/release/tsuzuri --out target/perf/<run_id>` を 1 行 |
| `_perfs/README.md` | `### ビルド速度`、一覧の PX03 の状態欄 | 表の上の「PX03 で正式に計測します」を、正式な値の場所（`docs/benchmarks.md` の `## ビルド速度`）に変える。状態を done |

## 受け入れ条件

- [ ] `benchmarks/build/generate.mjs` が決定的で、`check` を警告なしで通り、`-O0`・`-O3` の実行ファイルの出力が BigInt の参照値と一致する（10・1,000 モジュール）。
- [ ] `benchmarks/run-build.mjs` が合成 200・1,000 モジュールと実プロジェクト 11 個について、9 つの variant・opt の組を PX01 形式で記録する（skip は stderr に出る）。
- [ ] 段階別時間から `frontend`・`cache`・`tools`・`other` が要約に出て、コンパイラと Clang・wasm-ld の時間が分かれている。
- [ ] `build-warm` が cache hit（`tool.` の段なし）、編集の variant が cache miss であることを runner が確かめている。
- [ ] 本計測 1 回と、広がりの確認の 2 回目が `target/perf/` にあり、`docs/benchmarks.md` の `## ビルド速度` に公開されている。
- [ ] `src/` の変更がなく、`cargo test --locked` と `node tests/benchmark_report.mjs` と `--quick` が成功する。
- [ ] 速度の閾値を CI に入れていない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `check` に `--no-cache` を付けると CLI エラーで、計測が 0 ms 近くで「成功」したように見える。`measureProcess` が 0 以外の終了で throw することに頼り、`check` には付けない。
- `--emit llvm` は cache を使う。`--no-cache` を忘れると 2 回目以降が cache hit になり、B2 が小さく出る。
- cache hit でも IR 生成までは走る（鍵が IR の全文を含む）。B4 は「フロントエンド + 成果物のコピー」であり、インクリメンタルの速さではない。
- 編集の variant で同じ内容に戻すと cache hit になる。s を run の中で重複させない。変数を使い回すと warm-up と標本 1 が同じ s になりやすい。
- 最大 RSS は子プロセスのうち最大の 1 つを含む。`build` の値をコンパイラのメモリとして書かない。
- 標本ごとに出力ファイルが上書きされる。成果物の SHA-256 は標本ごとに計算してから次を走らせる。
- 符号付き `i64` の算術は overflow の規則（docs/language.md）に当たりうる。生成コードは wrap が参照値と一致すると確認済みの `i64u` だけを使う。
- `for i in 0 .. 7` は両端を含む 8 回。`String.length` は UTF-16 の長さ（`"m<k>-"` は ASCII なので JS の `length` と同じ）。
- `Main.tz` の `total` を 1 つの長い式にしない。run-cache と同じく文の列にする（深く入れ子になった式は構文の深さの上限に当たりうる）。
- Node 20 は BigInt の重いコードで V8 が落ちることがある（GUIDE §11.1 の既知の落とし穴）。Node 24 を使う。
- OS のファイルキャッシュは warm-up で温まる。`purge` などで冷やす計測はしない（sudo が要る。対象外）。
- `writeRecords` は `flag: "wx"` なので、同じ `--out` で再実行すると失敗する。run_id を毎回変える。
- `benchmarks/control` には `main` がなく、実行ファイルの build は失敗して skip になる。不具合ではない。
- 2026-09-29 の合成の参考値（124,003 行）と PX03 の合成の値を同じ表で比べない（関数の形と入力が違う）。

## 対象外

- Go・Zig・Rust・C/C++ との比較（Phase 2、D9）。
- OS のファイルキャッシュを冷やした計測、分散ビルド、リモートキャッシュ。
- 文書のサンプル（`scripts/check-docs.mjs`）のビルド時間。`std` 単独の計測（D5）。
- 段階別のメモリ（PX01 D6。プロセス全体の `peak_rss` で代える）。
- コンパイラの高速化そのもの（PB01〜PB07、G17）。

## 決定事項

### D1: 生成器の置き場所と生成規則

- 決定: `benchmarks/build/generate.mjs` に純粋関数と CLI を置く。1 モジュールは 10 種類の関数 × 2 の 20 個と `run`、整数は `i64u` だけ、定数は `k * 20 + j + 3`。
- 理由: 生成器をリポジトリに置かないと、2026-09-29 の値のように再現できない。10 種類は旧案の「ループ、match、レコード、union、ジェネリック関数、関数値、文字列」を覆い、形は HEAD で確認済み。
- 状態: 既定案（実装者はこの案に従う）

### D2: 実行時の入力

- 決定: `main :: IO<unit>` が stdin の 1 行目の UTF-16 の長さを種にする。
- 理由: HEAD にはコマンドライン引数の API がない（E08 の範囲）。stdin の長さなら文字列の解析なしで入力に依存させられ、`-O3` の定数畳み込みを防げる（確認済み）。
- 状態: 既定案（実装者はこの案に従う）

### D3: 規模

- 決定: 本計測は 200 と 1,000 モジュール、quick は 10 モジュール。1 モジュール 20 関数。
- 理由: 2026-09-29 の参考値と PB05 の目標（124K 行）に近い規模で、1 run が 30 分以内に収まる。
- 状態: 既定案（実装者はこの案に従う）

### D4: 操作と cache の状態

- 決定: 設計の 9 組。公開 API の変更は「公開関数を 1 つ足す」で表し、既存の関数の型は変えない。
- 理由: 既存の型を変えると呼び出し側の編集も要り、1 ファイルの変更にならない。whole-build cache では本体の変更と同じく miss になるが、G17 がモジュール単位の再利用を入れた後に差が出る。
- 状態: 既定案（実装者はこの案に従う）

### D5: 実プロジェクト

- 決定: HEAD で `check` が成功する 11 個（設計）。`std` は単独で測らない。
- 理由: `Main.tz` のないディレクトリは `E2001`、`std` は `E1011` で検査できない。標準ライブラリの処理は各プロジェクトの段に含まれる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 段階別時間と外部ツールの分離

- 決定: PX01 の `TSUZURI_TIME_PASSES=1` を使い、PX03 は Rust を変えない。段を増やす必要が出たら PX01 の変更として扱う（Rust の変更は PX01 が持つ）。
  `frontend`・`cache`・`tools`・`other` は要約で計算し、レコードには `phase_time` だけを保存する。
- 理由: PX01 D6 が G17 Phase 0 と PX03 の共通の仕組みと定めている。和を保存すると PX01 形式の metric が増える。
- 状態: 既定案（実装者はこの案に従う）

### D7: 標本数と順序

- 決定: 各組で warm-up 1 回と 9 標本を続けて測る。`measureProcess` を `{ runs: 1, warmups: 0 }` で 10 回呼ぶ。quick は warm-up 0 回・1 標本。
- 理由: PX01 の process suite と同じ統計。標本ごとに編集・SHA-256 の確認を挟むには 1 回ずつ呼ぶ必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D8: `benchmarks/run-cache.mjs`

- 決定: 変えずに残す。
- 理由: `docs/benchmarks.md` の `## 表の読み方` が参照し、warm build の成果物の byte 一致を短時間で確かめる役割がある。削除するかは PX03 の完了後に別に判断する。
- 状態: 既定案（実装者はこの案に従う）

### D9: 他言語との比較（Phase 2）

- 決定: 設計の Phase 2 の方針で、C・Rust・Go・Zig の同じ構造のプロジェクトを生成して測る。Go と Zig は計測機に導入する。
- 理由: 「Go を上回る」の判断に要る。ただし計測機へのツールの導入と 4 言語分の生成器・参照値の保守は費用が大きい。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D10: Rust の分割単位（Phase 2）

- 決定: 1 crate（モジュールごとの `mod`）と、モジュールごとの crate を持つ workspace の両方を測り、条件を表に明記する。
- 理由: Rust は crate が並列化と cache の単位で、どちらか一方では Tsuzuri のモジュールと対応しない。
- 状態: 既定案（実装者はこの案に従う。Phase 2 の承認後に適用）
