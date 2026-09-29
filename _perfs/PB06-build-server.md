# PB06: 常駐ビルドサーバーと watch モード

| 項目 | 内容 |
| --- | --- |
| ID | PB06 |
| 分類 | ビルド速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PB02, (G17) |
| 関連 | G07, G12, G13, PB07 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D2（Phase 2 の着手。ローカル IPC の常駐サーバー） |
| 手本にする既存実装 | 入力スレッド・`mpsc::sync_channel`・`recv_timeout` の主ループ: `src/lsp.rs` の `serve`。遅延実行: `Session::schedule`（200 ms）。上限つきの読み込み: `read_message`。私用ディレクトリ: `src/driver.rs` の `TemporaryDirectory::new`（`DirBuilderExt::mode(0o700)`）と `src/cache.rs` の `BuildCache::open`（`PermissionsExt`）。環境変数による置き場所の上書き: `cache::default_root`（`TSUZURI_CACHE_DIR`）。hash: `cache::Sha256`・`cache::file_digest`。オプションの検査: `src/main.rs` の `parse_arguments` の `--no-cache`。E2E: `tests/cache.mjs`（`cli`・`concurrent`）、`tests/lsp_sessions.mjs`（常駐プロセスとの対話） |
| 主な影響ファイル | `src/main.rs`, `src/lib.rs`, `src/watch.rs`（新規。Phase 1）, `src/server.rs`（新規。Phase 2）, `src/driver.rs`（Phase 2。常駐状態の受け渡し）, `src/cache.rs`（変更なし。`Sha256` を使う）, `src/lsp.rs`（変更なし。手本）, `tests/watch.mjs`（新規）, `tests/server.mjs`（新規）, `benchmarks/run-build.mjs`（PX03 が新規に作る。variant を足す）, `README.md`, `_docs/tools/command-line.md`, `_docs/tools/build-and-cache.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md` |
| 計測対象 | 1 モジュールの本体を変えた後の `check` と `build -O0` の時間（server 経由と `--no-server`）、watch の変更検出から完了までの時間、server の RSS と常駐 bytes |

## 目的

コンパイラの状態をプロセスをまたいで保持し、ファイルを保存してから結果が出るまでを短くする。目標は 1 ファイルの本体を変えた後の
`check` で 100 ms 級である（「目標と指標」）。

分担（調整役の決定）: G17 はディスク上のモジュール単位の前段 cache（parse 結果とモジュールの公開要約。content hash が key）を作る。
PB06 はそれと PB01 の事前ビルド済みランタイム object を常駐プロセスのメモリに保つ `tsuzuri serve` と、ファイルの変更で再実行する
`tsuzuri watch` を作る。PB07 はその上に関数単位の増分コード生成とリンクを作る。

結果（診断・stdout・stderr・終了コード・成果物）は常駐の有無によらずバイト単位で同じにする。server に何か問題があれば、CLI は黙って
プロセス内のビルドに戻る。LSP（G12）と REPL（G13）が server を使うかは各チケットで決める（対象外）。

実装者は Phase 1（`tsuzuri watch`）だけを実装する。Phase 2（`tsuzuri serve` と CLI の接続）は D2 の承認後、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PB02 が `_perfs/README.md` の状態欄で done であること（メタデータの依存）。PX03 も done であること（PB02 の依存。計測は PX03 の
  `benchmarks/run-build.mjs` に variant を足して行う）。確認: `grep -n "PX03\|PB02\|PB06" _perfs/README.md`。
- Phase 2 だけ: D2 が承認済みであること。G17 のモジュール単位の前段 cache（G17 の「Phase 2: モジュール単位の解析キャッシュ」に当たる段）が
  `_features/README.md` の状態欄で done であること。確認: `grep -n "| G17 " _features/README.md`。PB01 Phase 1 は任意（done なら常駐状態に含める）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 前提とする他チケットのインターフェース

- PX03（計測）: `benchmarks/run-build.mjs` の `VARIANTS`・`measureVariant`、`benchmarks/build/generate.mjs` の `moduleSource(m, { salt })`、
  PX01 の `measureProcess(command, args, { cwd, env, runs, warmups, timeoutMs })`（`wall_ms`・`peak_rss_bytes` を返す）と
  `writeRecords(directory, suite, records)`。名前が違えば PX03 の実装に合わせて読み替える。
- G17（Phase 2）: 以下 `FrontendCache` と呼ぶ値（G17 の実際の名前に読み替える）。(1) プロセスで一度作り、解析の関数へ `&mut` で渡せる。
  (2) 項目の key がソースの内容の hash・コンパイラの同一性・解析に効く指定を含み、PB06 が無効化しなくても使い回しが安全である。
  (3) メモリ上の大きさを bytes で返す関数がある。(4) 項目がない・壊れているときは計算し直す。ディスクの層は G17 のものを使う。
- PB01（Phase 2、任意）: 事前ビルド済み object を返す値（以下 `RuntimeObjects`）が (1)(2)(4) と同じ性質を持つ。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `unsafe` か新しい crate（`notify`・`libc`・`interprocess`・`windows-sys` など）が必要になった。std の `std::os::unix::net` と
   `std::os::unix::fs` だけで作る（D4・D9）。
2. server 経由と `--no-server` で、診断・stdout・stderr・終了コード・成果物のどれかが 1 byte でも違う。ただし 2 回の cold build 同士でも
   違うもの（既存の非決定性）は、その事実を報告して比較から外すかを人間に確認する。
3. check または build の経路で、lib が `eprintln!`・`io::stderr()` へ直接書く箇所が「現状」の 2 か所のほかに見つかった（server の応答に
   載らない出力になる）。
4. 解析を main thread 以外で動かす必要が出た（D8）。stack サイズを上げて回避しない。
5. G17 の cache がプロセス全体の `static` を使う、または 1 回の解析ごとに作り直す前提で、常駐プロセスで使い回せない。
6. 既存テストの期待値（`tests/cache.mjs`、`tests/lsp_sessions.mjs`、`src/main.rs` の `parse` のテスト）を変える必要がある。
7. socket の既定のパスが 100 bytes を超える環境が CI にある（`sun_path` は macOS 104 bytes、Linux 108 bytes）。
8. 計測のために `src/` に計測専用のオプションや隠し環境変数を足したくなった。

## 現状と計測（HEAD `f8dc655`）

### CLI の流れ

- `src/main.rs` の `main` は `parse_arguments` の後、`Project::load`（`test` は `Project::load_for_tests`、`doc` は `Project::load_for_docs`）→
  `Project::analyze_all` → `print_diagnostics`（警告）→ `run_action`（`driver::build`・`driver::run_with_diagnostics`・`driver::document`）
  または `run_test_action` を 1 回だけ実行して終わる。状態は残らない。
- 出力は `print_diagnostic`・`print_diagnostics`・`print_with_severity` と `main` の message のループが `eprintln!`・`println!` で直接書く
  （`src/main.rs` に計 20 か所）。書き先を差し替える引数はない。
- `Action` は `Lsp`・`Check`・`Doc`・`Build`・`Run`・`Fmt`・`Test`。先頭の語がどれでもなければ `build` として読む（HELP の
  `tsuzuri [build] source`）。`watch`・`serve` は今は入力パスとして読まれる（再現）。
- `--no-cache` は build と run だけで受け、ほかは `--no-cache is only valid with build or run`（`E2000`、終了コード 2）。
- `Project::load` は入力が symlink なら `E1011`、ディレクトリなら `Main.tz` を足し、その親をルートとして `load_from_root` を呼ぶ。
  `SourceFile` は `path`・`relative_path`・`name`・`text`・`origin`（`ModuleOrigin::User`・`ModuleOrigin::Std`）・`package` を持ち、std は
  仮想パス `std/<Name>.tz` でディスクにない。

### LSP（手本）

- `src/lsp.rs` の `serve` は入力を読むスレッドを `std::thread::spawn` で 1 本作り、`mpsc::sync_channel(8)` で main thread へ渡す。解析
  （`Session::refresh`）は `serve` を呼んだ thread で動く。`Session::schedule` は 200 ms 後に refresh を予約し、主ループは `recv_timeout` で待つ。
- 変更のたびにプロジェクト全体を再解析し、増分の状態は持たない（G07）。

### スレッドと stack

- `src/` に `std::thread::Builder`・`stack_size` の使用はない。`std::thread::spawn` は `src/lsp.rs` の入力スレッドだけ、`std::thread::scope` は
  `src/driver.rs` の `mod test_runner`（テストの並列実行）と `src/cache.rs` のテストだけにある。解析は CLI・LSP とも main thread
  （macOS・Linux の既定 8 MiB）で動く。debug build の Rust テストは 2 MiB の thread で動く。

### プロセス全体の状態と直接の出力

- 可変の `static` は `src/driver.rs` の `TEMP_COUNTER: AtomicU64`（一時ディレクトリ名 `.tsuzuri-<pid>-<counter>`）だけ。`src/llvm.rs` の
  `FIRST_METADATA: OnceLock<usize>` は埋め込みランタイムから決まる定数。`process::exit` の呼び出しはない。
- lib が標準エラーへ直接書くのは `TemporaryDirectory` の `Drop`（`warning: cannot remove ...`）と、`run` の子プロセスの出力の中継
  （`io::stderr()`）の 2 か所だけ。

### cache と環境

- `cache::build_key` は compiler の SHA-256（`file_digest(&std::env::current_exe()?)`）、`BuildOptions` の `Debug` 表現、IR、全ソースと
  manifest、環境変数 17 個（`TSUZURI_CLANG`・`TSUZURI_WASM_LD`・`TSUZURI_LLVM_LINK`・`TSUZURI_DSYMUTIL`・`PATH`・`PATHEXT`・`SDKROOT`・
  `MACOSX_DEPLOYMENT_TARGET`・`INCLUDE`・`LIB`・`LIBPATH`・`CPATH`・`C_INCLUDE_PATH`・`CPLUS_INCLUDE_PATH`・`LIBRARY_PATH`・
  `SOURCE_DATE_EPOCH`・`DEVELOPER_DIR`）、ツールの digest と `--version` の出力を hash する。起動するツールは環境全体を継承する。
- `cache::default_root` は `TSUZURI_CACHE_DIR`、なければ macOS `$HOME/Library/Caches/tsuzuri/build-cache`、Linux `$XDG_CACHE_HOME` か
  `$HOME/.cache` の下の `tsuzuri/build-cache`、Windows `%LOCALAPPDATA%\Tsuzuri\Cache\build-cache`。
- whole-build cache（G11）は解析と IR 生成の後に照合するので、hit しても解析の時間は残る。parse・check・IR の cache はない
  （docs/architecture.md の「不変条件」）。

### 依存と制約

- `Cargo.toml` の依存は `rustc_apfloat`・`serde_json`（Windows だけ `same-file`）。`[lints.rust]` は `unsafe_code = "forbid"`。
  edition 2024 では `std::env::set_var` が `unsafe` なので、要求ごとに環境変数を差し替える設計は取れない（D5）。
- std で使えるもの: `std::os::unix::net::{UnixListener, UnixStream}`、`std::os::unix::fs::{DirBuilderExt, PermissionsExt, MetadataExt}`、
  `std::fs::File::set_modified`（Rust 1.75。`rust-version` は 1.85）。Windows の named pipe を std だけで作る API はない（D4）。

### 計測済みの事実

- hello の `check` は 0.05 s（2026-09-29 の起票時）。2026-09-30 にこの計測機で 3 回測ると `real` 0.02〜0.03 s。規模別の時間と RSS は
  `_perfs/README.md` の「ビルド速度」の表（合成 200・1,000 モジュール）を基準にする。
- release の compiler は 4,509,744 bytes。`shasum -a 256` 1 回が `real` 0.04 s（プロセスの起動を含む）。要求のたびに compiler を
  hash しない理由（D6）。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PB06/hello /tmp/tz-work-PB06/bad
printf '42\n' > /tmp/tz-work-PB06/hello/Main.tz
printf 'missing\n' > /tmp/tz-work-PB06/bad/Main.tz
target/release/tsuzuri run /tmp/tz-work-PB06/hello
for i in 1 2 3; do /usr/bin/time -p target/release/tsuzuri check /tmp/tz-work-PB06/hello; done
cd /tmp/tz-work-PB06 && /Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri check bad
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri watch check hello
```

`run` は `42`。`check bad` は終了コード 1 で次を出す（`--json` では `"code":"E1002"` と `"path":"bad/Main.tz"` の 1 行）。`watch check hello` と
`serve hello` は終了コード 2 で `<command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively`。

```text
bad/Main.tz:1:1: error[E1002]: unknown value 'missing'
  1 | missing
    | ^
```

## 目標と指標

目標は CI の合否条件にしない。計測機で before と after を記録して判断する。Phase 1 だけでは解析を使い回さないので速くならない
（watch の利便と、Phase 2 の土台）。短縮は Phase 2 と G17 の cache の組み合わせで初めて生じる。

- G1（Phase 2 + G17）: 1,000 モジュールの合成プロジェクトで、1 モジュールの本体を変えた後の server 経由の `check` を 100 ms 以下にする。
  `build -O0` は PB05 と合わせて 200 ms 以下（起票時の目標。PB05 は依存ではない）。
- G2（Phase 1）: watch は変更から再実行の開始まで 1 周期（100 ms）以内。追加・削除したファイルは 1 s 以内（D9）。
- G3: server を起動していない CLI を遅くしない（client の確認は socket の `symlink_metadata` 1 回）。
- G4: 常駐 bytes は上限（1 GiB、D6）を超えない。RSS は記録する。

| 指標 | 単位・統計 | 対象 | 期待 |
| --- | --- | --- | --- |
| M1 編集後の `check` | ms（9 標本の中央値・最小・最大） | PX03 の合成 200・1,000 モジュール。標本の前に `Module<m>.tz` を `moduleSource(m, { salt: s })` で書き直す | server 経由が `--no-server` より短い（Phase 2 + G17） |
| M2 編集後の `build -O0` | 同上 | 同上。`--cpu generic`、cache は有効（編集で miss） | 同上 |
| M3 watch の遅延 | ms（9 標本） | 同じ編集。書き込みの直前から `watch: run N finished` の行を読むまで | `--no-server` の M1 + 100 ms 以内（Phase 1） |
| M4 server の RSS | bytes（M1・M2 の後に 1 回） | `ps -o rss= -p <pid>` の値 × 1024 | 記録のみ |
| M5 常駐 bytes | bytes | `status` 応答の `resident_bytes` | 1 GiB 以下 |
| M6 server なしの追加費用 | ms（9 標本） | hello の `check`（socket なし）、before と after | 差が広がりの範囲内 |

## 変えてはいけない意味

- 出力のバイト列: 診断（順序・本文・`--json` の各行・省略の note）、stdout、stderr、終了コード（0・1・2）、成果物（exe・object・llvm・wasm・
  header・wgsl・`.trap.json`・DWARF の sidecar）、`W2001` の message。server 経由・watch・通常の CLI で同じにする。
- コード生成には触れない。overflow・丸め・NaN・符号付きゼロ・trap・評価順序・所有権・借用・IR の決定性・WASM の既定の import なし
  （D-18）は、既存の `driver::build` の経路をそのまま使うことで保つ。
- cache: `cache::build_key` の入力と `--no-cache` の意味を変えない。server は環境と作業ディレクトリが同じ要求だけを受ける（D5）ので、
  key も同じになる。
- 出力の保護: `protect_sources` と `E2003` の検査は `driver::build` の中で同じく動く。server は要求のパスを別の規則で書き換えない。
- 信頼の境界: 同じ OS 利用者だけ。TCP などネットワークの listener を開かない。
- 既存コマンド: server がなければ今と同じ。増えるのは先頭の語 `watch`・`serve` と、check・build の `--no-server` だけ。
- LSP（`tsuzuri lsp`）の挙動は変えない。

## 設計

### 全体の構成

```text
Phase 1: tsuzuri watch <check|build|test> ARGS
  main thread: loop { S = watch::snapshot; code = cli(ARGS); 状態行; watch::wait_for_change(S) }

Phase 2: tsuzuri serve DIR [--idle-timeout SECONDS]
  accept thread: accept → hello 行を書く → sync_channel(QUEUE) へ（満杯なら busy を書いて閉じる）
  main thread:   recv_timeout(idle) → 要求 1 行 → 検査 → cli(ARGS, &mut resident) → 応答 1 行 → ログ 1 行
Phase 2: tsuzuri check|build ARGS（--no-server でなく、socket がある）
  server::request(...) が Some(応答) → stdout・stderr を書いて応答の終了コードで終わる
                      None       → 今と同じくプロセス内で cli(ARGS)
```

### Phase 分割

- Phase 1（実装対象）: `src/main.rs` の出力先の差し替え（`cli`）、`tsuzuri watch`、`src/watch.rs`、`tests/watch.mjs`、文書。
  server・socket・`--no-server` は作らない。
- Phase 2（D2 の承認後、人間が求めた場合だけ）: `src/server.rs`、`tsuzuri serve`、`--no-server`、client、`ResidentState`（新規）、
  `tests/server.mjs`、PX03 の variant、文書。
- Phase 3（設計方針。対象外）: LSP・REPL との共有、Windows の named pipe、`build_key` のツール問い合わせの記憶。

### 出力先の差し替え（Phase 1、`src/main.rs`）

- `fn cli(raw: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8`（新規）に、今の `main` の `let json = ...` 以降を移す。
  Phase 2 で引数 `resident: &mut ResidentState` を足す。
- `print_diagnostic`・`print_diagnostics`・`print_with_severity` と、`main` の message のループ（`W2001` の JSON 行を含む）は
  `err: &mut dyn Write` を受け、`eprintln!` を `writeln!(err, ...)` にする。書き込みの失敗は `expect("failed printing to stderr")`
  （`eprintln!` と同じく panic）。文字列は 1 byte も変えない。
- `run_formatter` と `run_test_action` は戻り値を `ExitCode` から `u8`（`SUCCESS` は 0、`FAILURE` は 1）に変えるだけで、中の `println!`・
  `eprintln!` は変えない（server はこの 2 つを実行しない。watch は同じプロセスの stdout・stderr へ書くので差がない）。
- `main` は `--help`・`--version`・`lsp` の扱いを今のまま残し、先頭が `watch` なら watch へ、それ以外は
  `ExitCode::from(cli(&raw, &mut io::stdout().lock(), &mut io::stderr().lock()))`。

### watch（Phase 1）

新 API（実装後に有効。未検証）:

```text
tsuzuri watch check <source|directory> [check の options]
tsuzuri watch build <source|directory> [build の options]
tsuzuri watch test <source|directory> [test の options]
```

- 引数: `parse_watch_arguments(arguments)`（新規）が `parse_arguments(&arguments[1..])` を呼び、action が `Check`・`Build`・`Test` でなければ
  `E2000` `watch runs check, build, or test; use tsuzuri watch check <path>`（終了コード 2）。`tsuzuri watch` だけのときも同じ。
- 1 回ごとに `cli(&raw[1..], ...)` を実行し、stderr に状態行を書いて flush する。text: `watch: run <n> finished with exit code <code>`。
  `--json`: `{"type":"watch","run":<n>,"exit":<code>}`（`test --json` の `"type"` 行と同じ形）。
- 失敗しても終わらない。Ctrl-C（SIGINT）で終わる。後片付けは不要（socket も一時ファイルも作らない）。
- `run` は受けない（D10）。

### ファイル監視（Phase 1、`src/watch.rs`（新規））

```rust
pub const POLL: std::time::Duration = std::time::Duration::from_millis(100); // （新規）
pub const FULL_EVERY: u32 = 10; // （新規）10 周期（1 s）ごとに全体を読み直す

#[derive(PartialEq)]
pub struct Snapshot { // （新規）
    files: Vec<(PathBuf, u64, Option<SystemTime>)>, // 長さと更新時刻。std（ModuleOrigin::Std）は除く
    texts: Vec<(PathBuf, String)>,                  // path と内容。sources と manifests の順
}

pub fn snapshot(input: &Path, tests: bool) -> Snapshot; // （新規）Project::load（tests なら load_for_tests）。失敗なら空
pub fn wait_for_change(input: &Path, tests: bool, before: Snapshot); // （新規）内容が変わるまで戻らない
```

```text
wait_for_change(input, tests, before):
  polls = 0
  loop:
    sleep(POLL); polls += 1
    stat_changed = before.files のどれかの (len, modified) が違う、または読めない
    if stat_changed or polls % FULL_EVERY == 0:
      next = snapshot(input, tests)
      if next.texts != before.texts: return      # 内容・ファイルの集合が変わった
      before = next                              # 時刻だけの変化（touch）は吸収して再実行しない
```

- snapshot は `cli` の前に取る。実行中の保存は、次の周期で「前の snapshot との差」として見つかり、もう一度実行する（取りこぼさない）。
- 対象は `Project` の `sources` と `manifests` のうち `origin == ModuleOrigin::User` のもの（package のソースを含む）。発見の規則は
  `Project::load` と同じなので、除外（隠しファイル、上限）も一致する。成果物（exe・`.ll` など）は拡張子が違うので対象にならない。
- 1 周期は stat だけ。全体の読み直しは 1 s ごとと、stat が変わったときだけ。`// ponytail: 1 s ごとの全体読み直し。PX03 で CPU 5% を
  超えたらディレクトリの mtime で追加を検出する` を書く。

### server（Phase 2、`src/server.rs`（新規））

```rust
pub const PROTOCOL: u64 = 1; // （新規）
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024; // （新規）
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024; // （新規）LSP の 1 message の上限と同じ
pub const QUEUE: usize = 16; // （新規）
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(2); // （新規）
pub const IGNORED_ENV: [&str; 3] = ["_", "OLDPWD", "SHLVL"]; // （新規）シェルの記録だけの変数

pub struct Request { pub cwd: PathBuf, pub compiler: PathBuf, pub env: Vec<(String, String)>, pub args: Vec<String> } // （新規）
pub struct Response { pub exit: u8, pub stdout: String, pub stderr: String } // （新規）
pub enum Refusal { Protocol, Busy, Root, Cwd, Env, Compiler, CompilerChanged, Unsupported, TooLarge, Internal } // （新規）

pub fn server_directory() -> Option<PathBuf>; // （新規）D5
pub fn project_root(input: &Path) -> Option<PathBuf>; // （新規）Project::load と同じ規則の canonical なルート
pub fn socket_path(directory: &Path, root: &Path) -> PathBuf; // （新規）<dir>/<SHA-256(root) の先頭 32 桁>.sock
pub fn serve(root: &Path, idle: Duration, handle: impl FnMut(&Request) -> Result<Response, Refusal>) -> Result<(), Diagnostic>; // （新規）
pub fn request(root: &Path, args: &[String]) -> Option<Response>; // （新規）None は「プロセス内で実行する」
```

- 非 Unix（`#[cfg(not(unix))]`）: `serve` は `E2000` `the build server needs unix domain sockets; use tsuzuri watch or build without a server`、
  `request` は常に `None`。
- protocol v1: UTF-8 の JSON を 1 行 1 message（改行区切り）。`serde_json` で読み書きする。要求は 1 MiB、応答は 16 MiB まで。

```json
{"protocol":1,"server":"tsuzuri","version":"0.1.0","pid":4242}
{"protocol":1,"op":"run","cwd":"/work","compiler":"/work/target/release/tsuzuri","env":[["HOME","/Users/me"],["PATH","/usr/bin:/bin"]],"args":["check","app"]}
{"protocol":1,"ok":true,"exit":1,"stdout":"","stderr":"app/Main.tz:1:1: error[E1002]: unknown value 'missing'\n  1 | missing\n    | ^\n"}
{"protocol":1,"ok":false,"error":"env","message":"PATH differs from the server environment"}
```

  1 行目は accept 直後の hello、2 行目が要求（`op` は `run`・`status`・`shutdown`）、3・4 行目が応答。`status` の応答は
  `{"protocol":1,"ok":true,"root":...,"requests":n,"served":n,"refused":n,"resident_bytes":n,"compiler_sha256":"..."}`、`shutdown` は
  `{"protocol":1,"ok":true}` を返してから socket を消して終了する。`error` の値は `Refusal` の順に `protocol`・`busy`・`root`・`cwd`・`env`・
  `compiler`・`compiler-changed`・`unsupported`・`too-large`・`internal`。
- 要求の検査（main thread、この順）: 行の長さと JSON と `protocol == 1`・`op`（`protocol`）→ compiler の自己点検（起動時の `current_exe` の
  canonical パスの `len`・`modified`・`ino` と今の値。違えば `compiler-changed` を返して終了）→ `compiler` が同じ canonical パス（`compiler`）→
  `cwd` が server の起動時の `current_dir` と同じ（`cwd`）→ 環境が起動時の `vars_os` と `IGNORED_ENV` を除いて完全一致（`env`。server か要求に
  UTF-8 でない値があれば常に `env`）→ `TSUZURI_TIME_PASSES` がない（`unsupported`。PX01 の段別時間は lib が直接 stderr に書く）→
  `handle`（引数の解釈、action が `Check`・`Build` か（`unsupported`）、`project_root` が server のルートと同じか（`root`））。
- client（`request`）: 引数がすべて UTF-8、環境がすべて UTF-8、`server_directory` がある、socket が `FileTypeExt::is_socket`、ディレクトリが
  私用（D5）の順に確かめ、どれかが偽なら接続せずに `None`。接続後は hello を `HELLO_TIMEOUT` で待ち、要求を書いて書き込み側を閉じ、
  応答は時間制限なしで待つ（前の要求の実行を待つことがある）。I/O の失敗・EOF・壊れた JSON・`protocol` の不一致・`ok:false` はすべて `None`。
  client は fallback の理由を表示しない（出力を cold と同じにするため。理由は server のログに出る）。
- `main`（Phase 2）: `parse_arguments` が成功し、action が `Check`・`Build`、`--no-server` なしのときだけ `request` を試す。`Some` なら
  stdout・stderr をそのまま書いて `ExitCode::from(exit)`。`None` なら `cli`。
- server のログ（server の stderr、1 要求 1 行）: `serve: listening on <socket> for <root>`、`serve: request <n> served (exit <code>)`、
  `serve: request <n> refused: <error> (<message>)`、`serve: idle for <s> s; stopping`、
  `serve: the compiler binary changed; stopping (start tsuzuri serve again)`。

### 常駐状態（Phase 2、`src/driver.rs`）

```rust
#[derive(Default)]
pub struct ResidentState { // （新規）
    frontend: FrontendCache,          // G17 の値（名前は G17 に合わせる）
    runtime: Option<RuntimeObjects>,  // PB01 Phase 1 が done のときだけ
}
impl ResidentState {
    pub fn bytes(&self) -> u64; // （新規）G17・PB01 の大きさの和
}
pub const MAX_RESIDENT_BYTES: u64 = 1 << 30; // （新規）
```

- CLI の 1 回の実行は `ResidentState::default()` を作って `cli` に渡す（今と同じ仕事量）。watch と server は 1 つを使い回す。
- 要求の後に `bytes() > MAX_RESIDENT_BYTES` なら `ResidentState::default()` に置き換え、ログに `serve: resident state dropped (<bytes> bytes)`。
  `// ponytail: 上限を超えたら全部捨てる。M4 で頻発するなら LRU` を書く。
- `CheckedModule`・IR・成果物は常駐させない（D11）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI（P1） | `src/main.rs` | `cli`（新規）、`main` | `main` の本体を `cli` へ移す。`main` は `watch` の分岐と `ExitCode::from(cli(...))` |
| CLI（P1） | `src/main.rs` | `print_diagnostic`、`print_diagnostics`、`print_with_severity` | 引数 `err: &mut dyn Write`。`writeln!` に置き換える |
| CLI（P1） | `src/main.rs` | `run_formatter`、`run_test_action` | 戻り値を `u8` に |
| CLI（P1） | `src/main.rs` | `parse_watch_arguments`（新規）、`watch_main`（新規） | 引数の検査、実行と状態行と `wait_for_change` のループ |
| CLI（P1） | `src/main.rs` | `HELP` | `tsuzuri watch <action> ...`（action は check・build・test）の 1 行。P2 で `tsuzuri serve directory [--idle-timeout SECONDS]` と `--no-server` |
| 監視（P1） | `src/watch.rs`（新規）、`src/lib.rs` | `POLL`、`FULL_EVERY`、`Snapshot`、`snapshot`、`wait_for_change` | 上の「ファイル監視」。`pub mod watch;` |
| server（P2） | `src/server.rs`（新規）、`src/lib.rs` | 上の「server」の定数・型・関数 | `pub mod server;` |
| CLI（P2） | `src/main.rs` | `Arguments::no_server`（新規）、`parse_arguments` | `--no-server` は check・build だけ。ほかは `--no-server is only valid with check or build`。2 回目は重複の E2000（`--no-cache` と同じ） |
| CLI（P2） | `src/main.rs` | `parse_serve_arguments`（新規）、`serve_main`（新規） | `serve` の引数と、`handle` の closure（`cli` を `Vec<u8>` に書く） |
| 常駐（P2） | `src/driver.rs` | `ResidentState`（新規）、`MAX_RESIDENT_BYTES`（新規）、G17 の解析の入口 | G17 の値を解析へ渡す |
| 計測（P2） | `benchmarks/run-build.mjs` | `VARIANTS` | `check-edit-body-server`・`build-edit-body-server`・`watch-edit-body`（計測手順） |
| テスト | `tests/watch.mjs`（新規）、`tests/server.mjs`（新規）、`src/watch.rs`・`src/server.rs`・`src/main.rs` の `mod tests` | テスト計画 | |
| 変更なし | `src/lsp.rs`、`src/cache.rs`、`src/llvm.rs`、`src/check.rs` | | コード生成・解析・cache の key は変えない |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests`
の N を必ず見る（GUIDE §3.1）。手順 1〜7 が Phase 1。手順 8〜13 は D2 の承認後だけ行う。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、基準のコンパイラを保存する。「再現」の `hello`・`bad` を作る。
- 確認: 次がすべて成功する。`cargo test` の N を記録する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p target/perf/PB06 && cp target/release/tsuzuri target/perf/PB06/baseline-tsuzuri
cargo test --locked --bin tsuzuri
node tests/cache.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
grep -rn '"watch"\|"serve"' tests src/main.rs
```

最後の grep が空であること（既存テストが `watch`・`serve` を入力パスとして使っていない）。

### 手順 2: 出力先の差し替え（出力は変えない）

- 変更: `src/main.rs` の `cli`（新規）、`main`、`print_diagnostic`・`print_diagnostics`・`print_with_severity`、`run_formatter`・`run_test_action`。
- 内容: 設計「出力先の差し替え」。`watch` はまだ足さない。
- 確認: `cargo test --locked --bin tsuzuri` が手順 1 と同じ N。`cargo build --release --locked` の後、次の diff が空。`node tests/e2e.mjs target/release/tsuzuri`
  と `node tests/cache.mjs target/release/tsuzuri` が成功する。

```sh
cd /tmp/tz-work-PB06
for c in baseline-tsuzuri tsuzuri; do
  bin=/Users/tmidorikawa/Documents/git/Tsuzuri/target/perf/PB06/baseline-tsuzuri
  [ $c = tsuzuri ] && bin=/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri
  { $bin check bad; echo "exit=$?"; $bin check bad --json; echo "exit=$?"; $bin check hello --no-cache; echo "exit=$?"; } > out-$c.txt 2>&1
done
diff out-baseline-tsuzuri.txt out-tsuzuri.txt
```

### 手順 3: `src/watch.rs`

- 変更: `src/watch.rs`（新規）、`src/lib.rs`（`pub mod watch;`）。
- 内容: 設計「ファイル監視」。1 周期の判定を `fn changed(input: &Path, tests: bool, before: &mut Snapshot, polls: u32) -> bool`（新規）に分け、
  `wait_for_change` は `sleep(POLL)` と `changed` のループだけにする（テストは `changed` を直接呼び、待たない）。
- 確認: `cargo test --locked --lib watch::` が `running 3 tests` で 3 passed（テスト計画）。

### 手順 4: `tsuzuri watch`

- 変更: `src/main.rs` の `parse_watch_arguments`（新規）・`watch_main`（新規）・`main`・`HELP`・`mod tests`。
- 内容: 設計「watch」。`main` の `lsp` の分岐の直後で `raw[0] == "watch"` を見る。
- 確認: `cargo test --locked --bin tsuzuri watch_arguments` が 1 passed。`cargo build --release --locked` の後、
  `target/release/tsuzuri watch run /tmp/tz-work-PB06/hello` が終了コード 2 で
  `error[E2000]: watch runs check, build, or test; use tsuzuri watch check <path>`。

### 手順 5: E2E `tests/watch.mjs`

- 変更: `tests/watch.mjs`（新規）。
- 内容: テスト計画の E2E の 1〜9。
- 確認: `node tests/watch.mjs target/release/tsuzuri` が終了コード 0。

### 手順 6: 文書（Phase 1）

- 変更: 「ドキュメント」の Phase 1 の行。
- 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md` が成功する。

### 手順 7: Phase 1 の最終確認と報告

- 確認: GUIDE §2.3 の基準コマンド、`cargo test --locked`、`node tests/watch.mjs target/release/tsuzuri`、`node tests/cache.mjs target/release/tsuzuri`、
  `node tests/lsp_sessions.mjs target/release/tsuzuri`、「生成コードの確認」がすべて成功する。GUIDE §13 の形で報告して止まる。

### 手順 8: protocol と私用ディレクトリ（Phase 2。まだ使わない）

- 変更: `src/server.rs`（新規）の定数・型・`server_directory`・`project_root`・`socket_path`、`ensure_private_directory`（新規）、
  `read_line_limited`（新規）・`decode_request`（新規）・`encode_response`（新規）、`src/lib.rs`（`pub mod server;`）。
- 確認: `cargo test --locked --lib server::` が `running 5 tests` で 5 passed。

### 手順 9: `tsuzuri serve`

- 変更: `src/server.rs` の `serve`、`src/main.rs` の `parse_serve_arguments`（新規）・`serve_main`（新規）・`HELP`。
- 内容: accept thread、主ループ、`status`・`shutdown`、寿命（D6）、ログ。`handle` は `cli` を `Vec<u8>` へ書き、`String::from_utf8` が
  失敗すれば `Refusal::Internal`。
- 確認: `cargo test --locked --bin tsuzuri serve_arguments` が 1 passed。次で stderr に `serve: listening on /tmp/tz-work-PB06/srv/<32 桁>.sock for
  /private/tmp/tz-work-PB06/hello` が出て、5 s 後に `serve: idle for 5 s; stopping`、終了コード 0、socket が消える。

```sh
cd /tmp/tz-work-PB06 && TSUZURI_SERVER_DIR=/tmp/tz-work-PB06/srv /Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri serve hello --idle-timeout 5; ls srv
```

### 手順 10: client と `--no-server`

- 変更: `src/server.rs` の `request`、`src/main.rs` の `Arguments::no_server`・`parse_arguments`・`main`。
- 確認: `cargo test --locked --bin tsuzuri no_server` が 1 passed。手順 1 の `cargo test --locked --bin tsuzuri` の既存分が全部成功する。

### 手順 11: E2E `tests/server.mjs`

- 変更: `tests/server.mjs`（新規）。
- 内容: テスト計画の E2E の 1〜10。この時点の server は常駐状態を持たないので、一致の検査の土台になる。
- 確認: `node tests/server.mjs target/release/tsuzuri` が終了コード 0。

### 手順 12: `ResidentState` と G17・PB01

- 変更: `src/driver.rs` の `ResidentState`・`MAX_RESIDENT_BYTES`、`cli` と `watch_main` と `serve_main` の受け渡し。
- 内容: 設計「常駐状態」。G17 の値を解析へ渡す方法は G17 の API に従う。
- 確認: `cargo test --locked` が成功。`node tests/server.mjs target/release/tsuzuri` と `node tests/watch.mjs target/release/tsuzuri` が成功
  （2 回目以降の要求が G17 の cache を使い、なお cold と同じ出力になる）。`status` の `resident_bytes` が 0 より大きい。

### 手順 13: 計測と文書（Phase 2）

- 変更: `benchmarks/run-build.mjs` の `VARIANTS`、「ドキュメント」の Phase 2 の行。
- 確認: 「計測手順」を行い、`docs/benchmarks.md` に記録する。`node scripts/check-docs.mjs _docs/tools/command-line.md _docs/tools/build-and-cache.md`
  が成功する。GUIDE §10 の完了の定義を確かめる。

## 計測手順

GUIDE §14 と PX03 の「計測手順」に従う。PX01 形式の記録は `target/perf/<run_id>/build.jsonl`（`writeRecords`）、PB06 固有の記録は
`target/perf/PB06/`（どちらもコミットしない）。計測中はほかの重い処理を止め、電源に接続する。

### 環境

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
{ git rev-parse HEAD; git diff --stat; sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -n 1; rustc --version; node --version; } > target/perf/PB06/env.txt
```

### variant（Phase 2。`benchmarks/run-build.mjs` に足す）

P・T・m・s は PX03 の「操作」と同じ。env は PX03 の env から `TSUZURI_TIME_PASSES` を除き、`TSUZURI_SERVER_DIR: join(T, "server")` を足したもの。
server・client・watch はすべて同じ env と `cwd: T` で起動する（D5 の一致の検査）。

| variant | 準備 | 標本 |
| --- | --- | --- |
| `check-edit-body-server` | 組の前に `tsuzuri serve P --idle-timeout 600` を起動し `serve: listening` を待つ。warm-up 1 回 | 標本の前に本体を編集し、`tsuzuri check P` |
| `check-edit-body-no-server` | なし | 同じ編集の後に `tsuzuri check P --no-server` |
| `build-edit-body-server` | server は上と同じ | `tsuzuri build P -O0 --cpu generic -o T/app-server` |
| `build-edit-body-no-server` | なし | `tsuzuri build P -O0 --cpu generic --no-server -o T/app-no-server` |
| `watch-edit-body` | `tsuzuri watch check P` を起動し run 1 の状態行を待つ | 書き込みの直前の `performance.now()` から次の状態行まで（`measureProcess` は使わない） |

- 各組は warm-up 1 回と 9 標本。server の標本ごとに `status` の `served` が 1 増えることと、同じ s の `--no-server` と終了コード・stdout・stderr
  が同じことを確かめる（違えば停止条件 2）。
- server の組の終わりに `status`（M5）と `ps -o rss= -p <pid>`（M4）を `target/perf/PB06/server-<P>.json` に書き、`shutdown` を送る。M4・M5 は
  PX01 形式に入れない（PX03 停止条件 2）。
- M6: `for i in 1 2 3 4 5 6 7 8 9; do /usr/bin/time -p target/perf/PB06/baseline-tsuzuri check /tmp/tz-work-PB06/hello; /usr/bin/time -p target/release/tsuzuri check /tmp/tz-work-PB06/hello; done 2> target/perf/PB06/m6.txt`。

### 記録

`docs/benchmarks.md` に「常駐ビルドサーバーと watch（PB06）」の節を足し、日付、計測機、before と after のコミット、ツールの版、M1〜M6 の表
（中央値・最小・最大）、生データの場所、G17・PB01 の状態（done か）、そろえられなかった条件を書く。

## 生成コードの確認

PB06 はコード生成を変えない。IR が before と同じことを確かめる。Phase 2 では server 経由の成果物も比べる。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/perf/PB06/baseline-tsuzuri build /tmp/tz-work-PB06/hello --emit llvm -O3 --no-cache -o /tmp/tz-work-PB06/before.ll
target/release/tsuzuri build /tmp/tz-work-PB06/hello --emit llvm -O3 --no-cache -o /tmp/tz-work-PB06/after.ll
cmp /tmp/tz-work-PB06/before.ll /tmp/tz-work-PB06/after.ll
```

`cmp` が何も出さない。Phase 2 では server を起動した状態で `tsuzuri build hello -O3 -o /tmp/tz-work-PB06/s` と
`tsuzuri build hello -O3 --no-server -o /tmp/tz-work-PB06/s2` を `cd /tmp/tz-work-PB06` から実行し、`cmp` が一致を示すこと。

## テスト計画

### Rust テスト

- `src/main.rs` の `mod tests`（`parse` と同じ形）:
  - `watch_arguments_accept_check_build_test_only`: 受理 `watch check Main.tz`、`watch build Main.tz -O0 -o out`、`watch test Specs.tz --filter x`。
    拒否（E2000）`watch`、`watch run Main.tz`、`watch fmt Main.tz`、`watch lsp`、`watch watch check Main.tz`、`watch check Main.tz --no-cache`。
  - `serve_arguments_need_one_directory_and_valid_timeout`（P2）: 受理 `serve app`、`serve app --idle-timeout 1`、`--idle-timeout 86400`。
    拒否 `serve`、`serve a b`、`--idle-timeout 0`・`86401`・`x`・重複、`serve app -O0`。
  - `no_server_is_only_valid_with_check_or_build`（P2）: 受理 `check Main.tz --no-server`、`build Main.tz --no-server`。拒否 `run`・`test` との組と重複。
- `src/watch.rs`（それぞれ一時ディレクトリを `std::env::temp_dir()` の下に作る）:
  - `snapshot_ignores_std_and_follows_project_order`: `ModuleOrigin::Std` がなく、`texts` の順が `Project::load` と同じ。
  - `changed_detects_same_length_edit_with_same_mtime`: 同じ長さに書き換え、`File::set_modified` で時刻を戻す。`polls = 1` では false、
    `polls = FULL_EVERY` では true。
  - `changed_ignores_touch_and_detects_addition_and_removal`: 時刻だけ変えると false。ファイルの追加・削除は `polls = FULL_EVERY` で true。
- `src/server.rs`（P2）:
  - `protocol_roundtrips_and_rejects_invalid_messages`: 要求と応答の往復。拒否（`protocol`）: 1 MiB を超える行、壊れた JSON、object でない、
    `protocol: 2`、`args` がない・文字列でない、未知の `op`、`env` の組が 2 文字列でない。
  - `socket_path_is_short_stable_and_root_specific`: 同じルートは同じ名前、違うルートは違う名前、名前は 32 桁の小文字 hex と `.sock`。
  - `private_directory_rejects_shared_or_linked_directories`: 0o755 と symlink は拒否、新規作成は 0o700、既存の 0o700 は受理。
  - `environment_comparison_ignores_only_shell_bookkeeping`: `IGNORED_ENV` の違いは受理。`PATH` の違い、変数の追加・欠落は `env`。
  - `project_root_matches_project_load`: ディレクトリはそれ自身、ファイルは親、親のない相対ファイルは作業ディレクトリ（canonical）、symlink は `None`。

### E2E

`tests/watch.mjs`（新規、Phase 1）。`tests/cache.mjs` の形（自分の一時ルート、`TSUZURI_CACHE_DIR`、`spawnSync`・`spawn`）。watch の子プロセスの
stderr を貯め、状態行を正規表現で待つ関数を作る（60 s で失敗。性能の閾値ではなく hang の検出）。各 run の stderr（前の状態行から次の状態行まで）が、
同じ内容で実行した cold の `tsuzuri check P`（または build）の stderr とバイト単位で同じことを毎回確かめる。`finally` で子を `SIGTERM` する。

1. `Main.tz` が `42` → `watch: run 1 finished with exit code 0`。
2. `missing` に書き換え → run 2、終了コード 1、`error[E1002]: unknown value 'missing'`。
3. 同じ長さの `mixxing` に書き換え、`utimesSync` で時刻を 2 の値に戻す → run 3（1 s の全体読み直しで見つかる）。
4. `utimesSync` で時刻だけ進め、次に `43` を書く → 次の状態行が run 4（touch で run が増えない）、終了コード 0。
5. 構文エラーの `Extra.tz` を足す → run 5、終了コード 1。
6. `Extra.tz` を消す → run 6、終了コード 0。
7. `watch build P -O0 --cpu generic -o T/app` → run 1 の後 `T/app` の stdout が `43`。`44` に書き換え → run 2 の後 `44`。
8. `watch check P --json` → 状態行が JSON として `{"type":"watch","run":1,"exit":0}`。
9. `watch`、`watch run P` → 終了コード 2、stderr に `watch runs check, build, or test`。

`tests/server.mjs`（新規、Phase 2）。env に `TSUZURI_CACHE_DIR: T/cache` と `TSUZURI_SERVER_DIR: T/srv`、server と client は同じ env と `cwd: T`。
socket は `T/srv/` + `sha256(realpathSync(P))` の先頭 32 桁 + `.sock`。protocol は `node:net` の `createConnection` で直接話す。

1. 一致: 状態（`42`、`43`、`missing`、構文エラーの `Extra.tz` を追加、削除、`Main.tz` の改名で `Main.tz` がない）× コマンド（`check P`、
   `check P --json`、`build P -O0 --cpu generic -o T/a`、`build P -O3 -o T/a`、`build P --target wasm32 -O0 -o T/a.wasm`、
   `build P --emit llvm -O0 -o T/a.ll`、`build P --no-cache -O0 -o T/a`）。server 経由の後に `--no-server` を同じ `-o` で実行し、終了コード・stdout・
   stderr・成果物のバイト列を比べる。server 経由の実行ごとに `status` の `served` が 1 増える。
2. 拒否と fallback: env に `EXTRA=1` を足す（`env`）、`cwd: P`（`cwd`）。どちらも出力は `--no-server` と同じで、server の stderr に
   `refused: env`・`refused: cwd`。`tsuzuri run P` と `tsuzuri test P` の後で `requests` が増えない。
3. protocol: 1 MiB + 1 byte の行、`"protocol":2`、壊れた JSON → `protocol`。別ディレクトリの `args` → `root`。別パスの `compiler` → `compiler`。
4. 偽の server（`shutdown` の後、同じパスに `net.createServer`）: 接続直後に閉じる、hello が壊れている、hello の後 `ok:false`、hello の
   `protocol` が 2、hello を送らない。どれも `check P` の出力が `--no-server` と同じ。
5. 古い socket: server を `SIGKILL` → CLI の出力が `--no-server` と同じ。新しい `serve` が起動できる。
6. 二重起動: 2 つ目の `serve P` が終了コード 1、`a build server is already running for`。
7. 権限: `T/srv` を 0o755 で先に作る → `serve` が終了コード 1 の `E2001`、CLI は fallback。正常時は `T/srv` が 0o700、socket が 0o600。
8. 寿命: `--idle-timeout 1` の server が自分で終了コード 0 で終わり socket が消える。compiler を `T/bin/tsuzuri` に複写して起動し、
   複写し直して時刻を進める → 次の要求で `compiler-changed`、server が終わり、CLI の出力は `--no-server` と同じ。
9. 並行: `build P -O0 -o T/p<i>` を 6 並列（`tests/cache.mjs` の `concurrent`）→ 全部 0、成果物が同一、`served` が 6 増える。
10. 常駐（手順 12 の後）: 1 の一致が常駐状態ありでも成り立ち、`resident_bytes` が 0 より大きい。

コード生成を変えないので、native・WASM × `-O0`・`-O3` の意味の suite と `live == 0` は既存の suite（`tests/e2e.mjs` など）が変わらず成功すること
で確かめる。

### 既存テストへの影響

なし。`tsuzuri watch`・`tsuzuri serve` が入力パスとして読まれなくなるが、手順 1 の grep で既存テストが使っていないことを確かめる。

### 性能

「計測手順」のとおり。CI に時間の閾値を置かない。

## ドキュメント

- `README.md` の「## ビルド」: Phase 1 で `tsuzuri watch`、Phase 2 で `tsuzuri serve`・`--no-server`・`TSUZURI_SERVER_DIR`。`src/main.rs` の `HELP` も同じ。
- `_docs/tools/command-line.md` の「## コマンド」（watch・serve）と「## build と run の主なオプション」（`--no-server`）。
- `_docs/tools/build-and-cache.md`（Phase 2）: 節「常駐ビルドサーバー」を足す（置き場所、権限、fallback、idle、compiler の変更、Windows 非対応）。
- `docs/architecture.md` の「## 不変条件」: watch の周期と検出の規則、server の信頼の境界、出力の一致、環境と作業ディレクトリの一致、main thread での解析。
- `docs/benchmarks.md`: 「計測手順」の記録。`_perfs/README.md`: PB06 の状態欄。

## 受け入れ条件

- [ ] Phase 1: `tsuzuri watch` が check・build・test を受け、run・引数なしを `E2000` で拒否する。
- [ ] Phase 1: 各 run の出力が cold の CLI とバイト単位で同じ（`tests/watch.mjs`）。同じ長さ・同じ時刻の編集、追加、削除を検出し、touch では再実行しない。
- [ ] Phase 1: `cargo test --locked` と既存の suite が成功し、新しい crate と `unsafe` がない。「生成コードの確認」の `cmp` が一致する。
- [ ] Phase 2: `tests/server.mjs` の一致・拒否・fallback・protocol・権限・寿命・並行のテストが成功する。
- [ ] Phase 2: M1〜M6 を計測し、生データの場所とともに `docs/benchmarks.md` に記録している。
- [ ] 「ドキュメント」の該当部分を更新している。GUIDE §10 の完了の定義を満たす。

## 落とし穴

- snapshot を `cli` の後に取ると、実行中の保存を取りこぼして古い結果のまま止まる。必ず前に取る（`changed` のテストでは防げないので E2E の 2〜4 で見る）。
- 更新時刻の粒度（HFS+ は 1 s）や同じ長さの書き換えは stat で見えない。1 s ごとの内容の比較が唯一の救済なので、`FULL_EVERY` を消さない。
- macOS の `/tmp` は `/private/tmp` の symlink。ルートは canonical で比べ、テストの socket 名は `realpathSync` から作る。
- `sun_path` は 104 bytes（macOS）。bind の前に socket のパスが 100 bytes 以下かを確かめ、超えれば `E2000`
  `the build server socket path is too long; set TSUZURI_SERVER_DIR to a shorter directory`。
- edition 2024 の `std::env::set_var` は `unsafe`。client の環境を server に取り込もうとせず、一致しなければ断る。
- 一時ディレクトリ名は pid と `TEMP_COUNTER` を含む。成果物に漏れると server と cold が違う。まず cold 2 回同士を比べる（停止条件 2）。
- `TemporaryDirectory` の `Drop` の警告は server の stderr に出る（まれ。仕様として文書に書く）。新しい直接出力を足さない（停止条件 3）。
- client は fallback の理由を表示しない。調べるときは server の stderr のログを見る。client 用の冗長表示の環境変数を足さない。
- accept thread で解析しない（2 MiB の stack で落ちる）。Rust テストで `serve` のループを動かさない（test thread も 2 MiB）。ループは E2E で試す。
- 待ち行列の client は時間制限なしで待つ。`-O3` の大きなビルドの後ろでは正常に長く待つ。時間制限は hello の 2 s だけ。
- 終了時の socket の削除は、bind した socket と `dev`・`ino` が同じときだけ。古い socket を消して起動した別の server の socket を消さない。
- `watch`・`serve` という名前のディレクトリは `tsuzuri build watch` か `tsuzuri ./watch` で扱う（文書に書く）。
- テストは必ず自分の `TSUZURI_SERVER_DIR` を使い、`finally` で `shutdown` か kill する。開発者の既定の置き場所を使わない。

## 対象外

- 複数の利用者で共有するビルドサーバー、リモートのビルド、TCP。
- LSP（G12）・REPL（G13）との状態の共有。server の protocol は公開 API として固定しない。
- Windows の server（named pipe は crate か `unsafe` が要る）。Windows でも `tsuzuri watch` は動く。
- CLI による server の自動起動（D3）。`run`・`test`・`doc`・`fmt` の server 経由の実行（D10）。
- `cache::build_key` のツールの digest と `--version` の問い合わせの記憶（M2 で大きいと分かったら別チケット）。
- OS のファイル変更 API（FSEvents・inotify）と `notify` crate（D9）。

## 決定事項

### D1: Phase 分割

- 決定: Phase 1 は `tsuzuri watch`（プロセス内。server なし）、Phase 2 は `tsuzuri serve`・client・常駐状態。実装者は Phase 1 だけを行う。
- 理由: watch は socket も権限の面も持たずに単独で出せ、出力先の差し替え（`cli`）は Phase 2 の土台になる。速さは G17 待ちである。
- 状態: 既定案（実装者はこの案に従う）

### D2: Phase 2 の着手

- 決定: 常駐サーバー（ローカル IPC、常駐プロセス、要求の受理）を作るかを人間が決める。
- 理由: 同じ利用者に限っても新しい攻撃面と運用（古い socket、二重起動、寿命）を持ち込む。効果は G17 の cache が done になるまで出ない。
- 状態: 要承認（承認前は Phase 2（手順 8〜13）に着手しない）

### D3: 起動方式

- 決定: `tsuzuri serve <directory> [--idle-timeout SECONDS]` で明示的に前面で起動する。CLI は server を起動しない。check・build は socket が
  あれば接続し、`--no-server` で避けられる。起動の失敗は終了コード 1、引数の誤りは 2。
- 理由: 起票時の既定案（自動起動しない）を保つ。std だけでは daemon 化（fork）ができず、前面なら寿命とログが利用者に見える。
- 状態: 既定案（実装者はこの案に従う）

### D4: 通信路と protocol

- 決定: Unix domain socket（`std::os::unix::net`）、改行区切りの JSON（`serde_json`）、`PROTOCOL = 1`、要求 1 MiB・応答 16 MiB、1 接続 1 要求。
  非 Unix では `serve` が `E2000`、client は使わない。
- 理由: 新しい crate と `unsafe` が要らない。1 接続 1 要求は状態機械を持たない。上限は LSP の `read_message` にそろえる。
- 状態: 既定案（実装者はこの案に従う）

### D5: 置き場所と安全性

- 決定: ディレクトリは `TSUZURI_SERVER_DIR`（空でなければ）、なければ macOS `$HOME/Library/Caches/tsuzuri/server`、Linux `$XDG_RUNTIME_DIR/tsuzuri`
  か `$XDG_CACHE_HOME`・`$HOME/.cache` の下の `tsuzuri/server`。`DirBuilderExt::mode(0o700)` で作り、symlink でない・`mode & 0o077 == 0`・
  中に `create_new` で probe ファイルを作って消せる、の 3 つで私用と判定する（client は作らず判定だけ）。socket は 0o600。要求は
  canonical なルート・`cwd`・compiler のパス・環境（`IGNORED_ENV` を除く）が server と同じときだけ受ける。
- 理由: libc なしで自分の uid を得られないが、0o700 で書き込めることは所有者である証拠になる。環境と作業ディレクトリをそろえれば
  `build_key`・ツールの起動・DWARF の `PWD` まで cold と同じになる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 寿命

- 決定: idle は既定 1800 s（`--idle-timeout` は 1〜86400）。compiler は起動時の canonical パスと `len`・`modified`・`ino` を要求ごとに比べ、違えば
  `compiler-changed` を返して終わる（SHA-256 は起動時に 1 回だけ計算し `status` に出す）。常駐 bytes が 1 GiB を超えたら全部捨てる。
- 理由: 4.5 MB の hash を要求ごとに計算すると目標の 100 ms を食う。LRU は計測で必要と分かるまで作らない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 正しさと fallback

- 決定: server の問題はすべて client の黙った fallback（プロセス内の実行）にする。起票時の「定期的に全体を検査するモード」は作らない。
- 理由: 常駐状態は G17 の content key の cache だけで、cold と比べるには G17 のディスク層も止める必要がある。一致は `tests/server.mjs` と
  `--no-server` で確かめる。
- 見直し提案: 起票時の検査モードを削った。必要なら G17 の API が固まった後に `serve --verify` として足す。
- 状態: 既定案（実装者はこの案に従う）

### D8: 並行性と stack

- 決定: 要求は main thread が 1 つずつ実行する（プロジェクトごとに同時に 1 ビルド）。accept thread は hello を書いて `sync_channel(QUEUE)` に
  入れるだけ。`stack_size` は使わない。
- 理由: HEAD の解析は CLI・LSP とも main thread で動き（8 MiB）、同じ条件を保てる。LSP の `serve` と同じ形で idle も `recv_timeout` で書ける。
- 状態: 既定案（実装者はこの案に従う）

### D9: ファイル監視

- 決定: std だけの polling。100 ms ごとに stat、1 s ごとと stat の変化時に `Project::load` で内容を比べる。
- 理由: 発見の規則を `Project::load` と共有でき、新しい crate が要らない。`notify` などの crate を使う場合は要承認（新しい crate）。
- 状態: 既定案（実装者はこの案に従う）

### D10: watch の CLI と server の対象

- 決定: `tsuzuri watch check|build|test ...`。起票時の `tsuzuri build --watch` は採らない。server が実行するのは check と build だけ。
- 理由: 一つの前置きで 3 つの action を扱え、各 action のオプション検査に手を入れずに済む。`run` と `test` は子プロセスの stdin・stdout・
  終了コードを client 側で扱う必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D11: 常駐状態の中身

- 決定: G17 の前段 cache の値と PB01 の object の値だけを保つ。`CheckedModule`・IR・成果物は保たない。
- 理由: どちらも content key で自分を検証するので、PB06 が無効化の規則を持たずに済む（起票時の「G11 と同じ入力の規則」は各 cache の key が担う）。
- 状態: 既定案（実装者はこの案に従う）

### D12: 計測

- 決定: PX03 の `benchmarks/run-build.mjs` に 5 variant を足す。M4・M5 は `target/perf/PB06/` に置き、PX01 形式を広げない。
- 理由: 編集の手順・標本数・記録を PX03 と共有でき、PX01 の schema を変えない。
- 状態: 既定案（実装者はこの案に従う）
