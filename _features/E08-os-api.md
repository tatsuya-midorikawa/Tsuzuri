# E08: 標準 OS API（ファイル・環境・時刻・乱数・プロセス）

| 項目 | 内容 |
| --- | --- |
| ID | E08 |
| 優先度 | P1 |
| 規模 | XL |
| 依存 | B07, E06 |
| 後続 | E09, B08, G18 Phase 3 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（std モジュール名 `File`／`Dir`／`Path`／`Env`／`Time`／`Random`／`Os` の確定と予約。GUIDE D-30 の仮割り当て）, D9（`IO<i32>` 入口の値を終了コードにする。既存プログラムの意味の変更）, D10（`--wasm-host wasi` と WASI preview1 の import。段 C）, D12（段 B の `File.Handle` と B07 D5 の両立方法） |
| 改善する劣位 | C/C++ 比: OS との接続にホスト実装が必要（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）、C#/F# 比: ファイル API がない（[同](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | std 専用 builtin と E1022: `src/check.rs` の `Builtin::IOReadLine`／`Builtin::IOWrite`（`name`、型 scheme）と `src/polymorph.rs` の `Checker::builtin` の IO 判定。遅延アクションの包み方: `std/IO.tc` の `try_read_line`・`try_output`。宣言と所有結果の受け取り: `src/llvm_io.rs` の `io_builtin`、`src/llvm_imports.rs` の `host_result_slot`・`read_host_result`。C ランタイムと連結条件: `src/runtime/io.c`、`src/driver.rs` の `io_runtime`。入口: `src/llvm.rs` の `io_entry`・`validate_main`・`console_main`、`src/llvm_io.rs` の `entry`。E2E: `tests/io.mjs` |
| 主な影響ファイル | `std/Os.tz`・`std/File.tz`・`std/Dir.tz`・`std/Path.tz`・`std/Env.tz`・`std/Time.tz`・`std/Random.tz`（新規）, `src/stdlib.rs`（`SOURCES`, `RESERVED_MODULES`）, `src/check.rs`（`Builtin`）, `src/polymorph.rs`（`Checker::builtin`）, `src/llvm.rs`（builtin の振り分け、`io_entry`、`validate_main`、`console_main`）, `src/llvm_io.rs`, `src/runtime/os.c`（新規）, `src/driver.rs`, `src/main.rs`（段 C だけ）, `tests/os_api.rs`（新規）, `tests/os.mjs`（新規）, `tests/stdlib.rs`, `tests/io.mjs`（段 C だけ）, `README.md`, `docs/language.md`, `docs/architecture.md`, `_docs/library-reference/io.md`, `_docs/library-reference/os.md`（新規）, `_docs/guides/webassembly.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

ファイル・ディレクトリ・環境変数・コマンドライン引数・時刻・乱数・終了コードを標準 API で扱えるようにする。
C/C++ の標準ライブラリ、.NET の BCL、Rust の `std::fs`／`env`／`time` に相当し、小さなツールやバッチ処理をホスト実装なしで書けるようにする。
OS に触れる操作はすべて既存の `IO<'a>` の中の遅延アクションにし、IO の外で観測できる非決定性（時刻・乱数・ファイル内容）を増やさない。

Phase 1 は三つの段に分ける。実装者は段 A を実装し、段 B は B07 が done のときだけ、段 C は D10 の承認後だけ着手する。
Phase 2 は人間が求めた場合だけ着手する。

| 段 | 内容 | 着手の条件 |
| --- | --- | --- |
| A | `Os.Error`、`File`（ファイル全体の読み書き・削除）、`Dir`、`Path`、`Env`、`Time`、`Random`（OS 乱数と `Random.Pcg`）、`IO<i32>` の終了コード、既定 WASM での拒否。native の macOS／Linux だけ | D1 の承認。終了コードの手順だけ D9 の承認 |
| B | `File.Handle`（open／read／write／flush、B07 の `Drop` で close） | B07 が done |
| C | `--wasm-host wasi` で段 A・B と既存の標準入出力を WASI preview1 へ下げる | D10 の承認 |

## 着手条件と停止条件

### 着手条件

- E06 が done、B07 の状態を確認する。確認: `grep -n "^| B07 \|^| E06 \|^| E08 \|^| G10 " _features/README.md`。HEAD では E06 が done、
  B07 が todo、G10 が blocked。段 A は B07 に依存しない（D12）。
- D1 が承認済みであること。承認前はどの手順にも着手しない。D9 の承認前は手順 9 を飛ばし、D10 の承認前は段 C に着手しない。
- GUIDE §2.3 の基準コマンドが成功し、`node tests/io.mjs target/release/tsuzuri` が成功すること。
- macOS か Linux の開発機で作業する。Windows 実装は Phase 2（G10）で、段 A の Windows は E2002 で拒否する（D11）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 新しい std モジュール名の予約で、既存のテスト・例・文書のモジュール名が衝突した（HEAD の調査では該当なし）。
- `Builtin` の型 scheme で `(i64 * [ubyte])` のような tuple 結果を表せず、`BuiltinType` に新しい variant が要る。
- `host_result_slot`／`read_host_result` を `ubyte` 配列の所有結果に再利用できず、新しい受領経路が要る。
- 段 A で既定の wasm32 出力に import が一つでも増える。または既存の `tests/io.mjs` の import 期待値（`tsuzuri_io.read_line`／`tsuzuri_io.write`）が変わる。
- `console_main` の `@main` の形を、`Env.args`（`Os.__args`）を使わないプログラムでも変える必要がある（既存 IR の不変が崩れる）。
- ランタイムの C で `-D_GNU_SOURCE` 以外の feature macro、libc 以外のライブラリ、`system`／`popen` などの shell 経由の API が必要になった。
- `unsafe`、新しい Rust crate、既存テストの期待値（IR・診断コード・メッセージ）の変更が必要になった。`src/stdlib.rs` の `reserves_the_d07_table` の
  件数（20 → 27）だけは除く。
- stack-depth の 3 テスト（GUIDE §11.1 の一覧）か `honors_the_exact_specialization_limit` が失敗した。std の generic 関数を増やすと
  特殊化の予算を使うので、新しい std 関数は generic にしない。

## 現状（HEAD `f8dc655` で確認）

- `std/IO.tc`: `record IO<'a> { work: unit -> 'a }` と `union Error = ReadFailed | WriteFailed | InvalidEncoding`。
  `try_read_line` は `IO.__read_line ()` の `(状態, bytes)` を `Utf8String.from_bytes` と `String.from_utf8` で `string` にする。
  `try_output` は `String.is_well_formed` を確かめてから `Utf8String.from_string` で UTF-8 にし、`IO.__write` へ渡す。通常版は `Result.get` でトラップする。
- `src/stdlib.rs`: `SOURCES` に std の全ファイルを `include_str!` で並べる。`RESERVED_MODULES` は利用者が使えないモジュール名（`IO` を含み、
  `File` などは含まない）。`opaque_record` は `"IO.IO"` などの構築・field を定義モジュールに限る。
- `src/check.rs`: `Builtin::IOReadLine`（`IO.__read_line :: unit -> (i32 * [ubyte])`）と `Builtin::IOWrite`（`IO.__write :: i32 -> ref utf8string -> i32`）。
  `Builtin::name` と型 scheme の match、全 builtin の列挙に arm がある。
- `src/polymorph.rs` の `Checker::builtin`: 二つの IO builtin を `self.module == "IO"` かつ std 以外から参照すると
  `E1022`「IO primitives are private to the standard IO module; compose IO actions instead」。
- `src/llvm.rs`: builtin の emission は IO builtin を `emitter.io_builtin` へ振り分ける。`io_entry` は入口が std の `IO.IO` を返すかを判定し、
  `validate_main` は IO 以外の入口を scalar・unit・string に限る（E2004）。`console_main` は IO 入口に `define i32 @main()` から
  `@tsuzuri_main` を呼ぶだけの本体を出す。
- `src/llvm_io.rs`: `entry` は `@tsuzuri_main` で IO の closure を呼び、結果を drop して常に `ret i32 0`。`io_builtin` は
  `declare i32 @tsuzuri_io_<name>` を到達時だけ `intrinsics` に入れ、wasm では `"wasm-import-module"="tsuzuri_io"` を付ける。
  read_line の所有結果は `host_result_slot`／`read_host_result`（`src/llvm_imports.rs`）で受け、状態を `icmp ult i32 %s, 3` で検査する。
- `src/driver.rs`: IR に `declare i32 @tsuzuri_io_` があれば native で `src/runtime/io.c` を task／CPU ランタイムと一つの C ファイルに連結して
  clang でコンパイルする。Windows の COFF object に runtime を埋め込む場合は E2002。`run_with_diagnostics` は子プロセスが成功以外で
  終わると E2005「program terminated with …; integer division, indexing, assert, or allocation may have trapped」を返す。
- `src/main.rs`: `--wasm-feature <name>`（`simd128`／`threads`）はあるが、ホストの種類を選ぶ option はない。
- `src/runtime/io.c`: `tsuzuri_io_read_line`／`tsuzuri_io_write`。EINTR の再試行、Windows の `_setmode(…, _O_BINARY)`。
- 資源の解放はコンパイラ生成の drop だけ（B07 が `Drop` を導入予定、todo）。
- README の未実装一覧に「GUI/OSの標準ライブラリ」がある。docs/language.md の「IO と標準入出力」は「WASI や非同期イベントループは追加しません」と書く。

### 再現（検証済み）

`/tmp/tz-e08/<case>/Main.tz` に置いて `target/release/tsuzuri run` と `build` で確かめた結果。

```tsuzuri
def main :: IO<i32> =
    do! IO.write_line "hi"
    return 3i32
```

- 上の `IO<i32>` 入口は受理され、`hi` を出して終了コード 0 で終わる（値 3 は捨てられる）。`build` した実行ファイルも 0。
- `let! text = File.read_text "a.txt"` は `error[E1002]: unknown value 'File'`。
- 利用者の `Path.tz`（`def separator :: unit -> string`）と `Path.separator ()` の呼び出しは受理され、`/` を出す。D1 の予約後は
  `RESERVED_MODULES` の検査で拒否される（利用者への影響。ドキュメントに記す）。
- `IO.write_line` だけを使うプログラムの wasm32 出力の import は `[{"module":"tsuzuri_io","name":"write","kind":"function"}]` だけ。
- `Random.Pcg` に使う算術は既存 API で書ける。`(Int.widening_mul 1i64u 6364136223846793005i64u + 1442695040888963407i128u) as i64u` は
  `7806831264735756412`、その `>> 59` は `13`、`Int.rotate_right 1i32u 1` は `2147483648`（`String.split` は区切りが第 1 引数）。

## 仕様

### 前提とする他チケットのインターフェース

- E06（done）: 所有結果の ABI。runtime は `tsuzuri_alloc` で確保した buffer を descriptor（pointer、i64 length）に書き、Tsuzuri が所有して解放する。
  長さ 0 は NULL を許す。
- B07（段 B だけ）: `class Drop<'a> { def drop :: ref mut 'a -> unit }`。`File.Handle` の close に使う。
- G10（Phase 2）: Windows の toolchain と CI。段 A では Windows を拒否する（D11）。

### 構文

言語の構文は変えない。段 C だけ CLI option を足す。

```text
build-option = ... | "--wasm-host" host-name
host-name    = "wasi"
```

### API（段 A）

新 API（実装後に有効。未検証）。IO を返す関数は実行時にだけ OS に触れ、構築では何もしない。純粋な関数（`Path`、`Random.Pcg`、`Os.message`、
`Os.encode`、`Os.error_of_status`）はどの target でも import なしで動く。

```tsuzuri
// std/Os.tz
union ErrorKind = NotFound | PermissionDenied | AlreadyExists | InvalidInput | InvalidEncoding | Interrupted | Other
record Error { kind: ErrorKind, code: i32 }
def message :: ref Error -> string                          // "not found (os error 2)" の形
def encode :: ref string -> Result.Result<utf8string, Error>  // 孤立サロゲートは InvalidEncoding, code 0
def error_of_status :: i64 -> Error                          // D2 の状態値の復号

// std/File.tz
def read_bytes :: string -> IO<Result.Result<[ubyte], Os.Error>>
def read_text :: string -> IO<Result.Result<string, Os.Error>>        // UTF-8 厳密。BOM は残す
def write_bytes :: string -> [ubyte] -> IO<Result.Result<unit, Os.Error>>
def write_text :: string -> string -> IO<Result.Result<unit, Os.Error>>
def append_text :: string -> string -> IO<Result.Result<unit, Os.Error>>
def remove :: string -> IO<Result.Result<unit, Os.Error>>

// std/Dir.tz
def list :: string -> IO<Result.Result<[string], Os.Error>>          // D7 の順序。"." と ".." を含まない
def create :: string -> IO<Result.Result<unit, Os.Error>>            // 親は作らない
def remove :: string -> IO<Result.Result<unit, Os.Error>>            // 空ディレクトリだけ

// std/Path.tz（純粋。区切りは "/" だけ）
def join :: ref string -> ref string -> string
def parent :: ref string -> Option.Option<string>
def file_name :: ref string -> Option.Option<string>
def extension :: ref string -> Option.Option<string>

// std/Env.tz
def args :: unit -> IO<Result.Result<[string], Os.Error>>            // argv[0] を除く
def var :: string -> IO<Result.Result<Option.Option<string>, Os.Error>>
def current_dir :: unit -> IO<Result.Result<string, Os.Error>>

// std/Time.tz
def monotonic_ns :: unit -> IO<Result.Result<i64, Os.Error>>
def unix_ns :: unit -> IO<Result.Result<i64, Os.Error>>
def sleep_ms :: i64 -> IO<Result.Result<unit, Os.Error>>

// std/Random.tz
def bytes :: i64 -> IO<Result.Result<[ubyte], Os.Error>>
def next_u64 :: unit -> IO<Result.Result<i64u, Os.Error>>            // bytes 8 の little-endian
record Pcg { state: i64u, increment: i64u }                          // opaque（Random の中だけで構築）
def pcg :: i64u -> i64u -> Pcg                                       // seed、stream
def pcg_next_u32 :: Pcg -> (i32u * Pcg)
def pcg_next_u64 :: Pcg -> (i64u * Pcg)                             // 上位 32 bit が先の出力
```

### 型規則

- 新しい型規則はない。どの関数も generic にしない（特殊化の予算を使わない）。IO を返す関数の引数は所有値で、closure に move される。
- `Os.Error` と `Os.ErrorKind` は通常の record／union。`Random.Pcg` は `src/stdlib.rs` の `opaque_record` に加え、利用者が偶数の increment を作れないようにする。
- `Main` の入口 `IO<i32>` は値を終了コードにする（D9）。`IO<T>`（`T` が `i32` 以外）は従来どおり値を drop して 0 で終わる。
- `Os.__*` builtin（「データ構造」）は `File`／`Dir`／`Env`／`Time`／`Random`／`Os` の std モジュールからだけ参照できる。それ以外は E1022。

### 評価順序・所有権・借用

- 各 IO 関数は `IO.map (\() -> <即時の処理>) (IO.pure ())` の形で包む（`IO.IO` は opaque なので `IO { work }` を他の std から作れない）。
  path の UTF-8 化・OS 呼び出し・結果の復号は実行時に行い、IO 値を 2 回実行すれば OS 呼び出しも 2 回起きる。
- 実行順は `let!`／`do!` の順。宣言には `readnone`／`readonly` などを付けず、LLVM に削除・並べ替えをさせない（`io_builtin` と同じ）。
- runtime が返す buffer の所有権は Tsuzuri に移る。runtime の一時領域（NUL 終端の path など）は runtime 内の `malloc`／`free` で全経路解放する。
- IO は Task の中で実行できない（`IO.run` も Task への変換もない）。OS 呼び出しは常に入口のスレッドで起き、`sleep_ms` や大きな読み込みが
  task worker を塞ぐことはない。IO の途中で呼んだ `Task.parallel` は次の IO 段より前に完了する。

### 数値・トラップ・native と WASM の差

- `monotonic_ns` は `CLOCK_MONOTONIC`（起点は未規定、差だけが意味を持つ）、`unix_ns` は `CLOCK_REALTIME` の 1970 年からの ns。
  秒 × 10^9 + ns が i64 を超える場合は `Other`（code は `EOVERFLOW`）。
- `sleep_ms` は負で `InvalidInput`（code 0）、0 は OS を呼ばずに `Ok ()`。`nanosleep` を EINTR で残り時間から再開する。
- `Random.bytes` は負または 2^30 より大きい長さで `InvalidInput`（code 0）。`Random.next_u64` はその 8 byte を little-endian で組む。
- 状態値の範囲外（0 でも kind 1..=7 でもない）と descriptor の違反は、read_line と同じく `TrapKind::BoundsCheck` でトラップする。
- 既定の wasm32: `Os.__*` に到達したビルドは E2000（D10）。`tsuzuri check` は IR を作らないので報告しない。純粋な関数だけなら import は増えない。
- Windows native: `Os.__*` に到達したビルドは E2002（D11）。
- `Os.ErrorKind` への case 追加は網羅的な `match` を壊す非互換変更になる。G19 の D8（std は全 edition で共有し、union の case の
  追加は別チケット・要承認）に従い、Phase 1 の後に case を足すときは人間の承認を得る。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1022 | std の OS 系 6 モジュール以外から `Os.__read` などを参照 | `operating-system primitives are private to the standard File, Dir, Env, Time, Random, and Os modules; use those APIs instead` | 参照の span |
| E1011 | 利用者の `File.tz` などの予約名（既存の検査） | `module name 'File' is reserved for the standard library; rename the file`（既存の文言） | 既存どおり |
| E2000 | 既定の wasm32 で `Os.__*` に到達 | `wasm32 output cannot use the File, Dir, Env, Time, or Random operating-system APIs because the default wasm32 target has no host imports; build for the native target, or keep to Path and Random.Pcg, which need no host` | なし |
| E2002 | Windows の native で `Os.__*` に到達 | `the File, Dir, Env, Time, and Random operating-system APIs are not supported on Windows yet (G10); build on macOS or Linux` | なし |
| E2005 | `run` で `IO<i32>` 入口が非 0 で終了 | `program exited with code 3`（数値は実際の値） | なし |
| E2000 | 段 C: `--wasm-host` の値が不正、重複、wasm32 以外 | `unknown wasm host 'x'; the supported host is wasi`、`--wasm-host specified more than once`、`--wasm-host wasi requires wasm32 object, LLVM IR, or WASM output` | なし |

### 資源上限

- path・名前の長さは OS に任せる（`ENAMETOOLONG` は `InvalidInput`）。`Random.bytes` は 2^30 byte まで。`current_dir` の buffer は 1 MiB まで倍々に広げ、
  超えたら `Other`（code `ERANGE`）。ファイル全体の読み込みと `Dir.list` に件数・大きさの上限はなく、確保失敗は既存どおりトラップ。

### 例

新 API（実装後に有効。未検証）。受理される例:

```tsuzuri
def main :: IO<i32> =
    let! written = File.write_text "out.txt" "hello\n"
    let! text = File.read_text "out.txt"
    do! IO.write (Result.get text)
    return (match written with
        | Result.Ok _ -> 0i32
        | Result.Error error -> 1i32)
```

- 期待: `hello` を出し、終了コード 0。`File.read_text "missing"` は `Result.Error { kind: NotFound, code: 2 }`（macOS／Linux の `ENOENT`）。
- `Path.join (ref a) (ref b)`: `"a"`・`"b"` → `"a/b"`、`"a/"`・`"b"` → `"a/b"`、`""`・`"b"` → `"b"`、`"a"`・`"/etc"` → `"a//etc"`（置き換えない。D13）。
- `Path.parent`: `"a/b/c"` → `Some "a/b"`、`"/a"` → `Some "/"`、`"a"`・`"/"`・`""` → `None`。末尾の `/` は先に取り除く（`"a/b/"` → `Some "a"`）。
- `Path.file_name`: `"a/b.txt"` → `Some "b.txt"`、`"a/.."`・`"."`・`"/"` → `None`。`Path.extension`: `"a.tar.gz"` → `Some "gz"`、`".bashrc"` → `None`、`"a."` → `Some ""`。

拒否される例: 利用者の `Os.__read 0i32 (ref path)` は E1022、`File.read_text` を使う project の `build --target wasm32` は E2000、
利用者の `Time.tz` は E1011。

### Phase 1 段 B・段 C（設計方針）

- 段 B: `File.Handle`（opaque、`{ descriptor: i64 }`）と `union Mode = Read | Write | Append | CreateNew`。IO の closure は借用を捕捉できないので、
  handle は所有値として受け渡す: `open :: string -> Mode -> IO<Result.Result<Handle, Os.Error>>`、`read :: Handle -> i64 -> IO<(Handle * Result.Result<[ubyte], Os.Error>)>`、
  `write :: Handle -> [ubyte] -> IO<(Handle * Result.Result<unit, Os.Error>)>`、`flush`・`close :: Handle -> IO<Result.Result<unit, Os.Error>>`。
  `instance Drop<Handle>` は close の失敗を無視する（明示 `close` は結果を返す）。builtin は `Os.__open`／`Os.__handle`／`Os.__close`（新規）。
- 段 C: `--wasm-host wasi` のとき `Os.__*` と `IO.__*` を `src/runtime/os-wasi.c`（新規、libc なしの C11。`src/runtime/task-wasm-threads.c` と同じく clang で生成）へ下げ、
  その中から `wasi_snapshot_preview1` の `fd_read`／`fd_write`／`fd_close`／`path_open`／`path_create_directory`／`path_remove_directory`／`path_unlink_file`／
  `fd_readdir`／`fd_prestat_get`／`fd_prestat_dir_name`／`clock_time_get`／`poll_oneoff`／`random_get`／`args_sizes_get`／`args_get`／`environ_sizes_get`／`environ_get`／`proc_exit` を import する。
  path は preopen（fd 3 から）の名前と前方一致で解決し、相対 path は fd 3 からとする。到達しない import は出さない。

### Phase 2（設計方針）

- プロセスの起動（引数配列で渡し shell を介さない）、ファイルのメタデータ、ディレクトリの再帰走査。
- Windows（G10）: `string` の path を UTF-16 のまま `CreateFileW`／`FindFirstFileExW`／`CreateDirectoryW`／`RemoveDirectoryW`／`DeleteFileW`／`GetCurrentDirectoryW`／
  `GetEnvironmentVariableW`、引数は `CommandLineToArgvW(GetCommandLineW())`（`shell32` を link）、乱数は `BCryptGenRandom`（`bcrypt`）、時計は
  `QueryPerformanceCounter`／`GetSystemTimePreciseAsFileTime`。`GetLastError` の対応: 2・3 → NotFound、5・32 → PermissionDenied、80・183 → AlreadyExists、
  87・123・267 → InvalidInput、1113 → InvalidEncoding、それ以外（145 を含む）→ Other。`Dir.list` の UTF-16 名は UTF-8 に変換してから D7 で整列する。

## 設計

### データ構造

```rust
// src/check.rs
pub enum Builtin {
    // ...
    OsRead,   // Os.__read :: i32 -> ref utf8string -> (i64 * [ubyte])
    OsArgs,   // Os.__args :: unit -> (i64 * [ubyte])
    OsWrite,  // Os.__write :: i32 -> ref utf8string -> ref [ubyte] -> i64
    OsRandom, // Os.__random :: i64 -> (i64 * [ubyte])
    OsClock,  // Os.__clock :: i32 -> i64
    OsSleep,  // Os.__sleep :: i64 -> i64
}
// src/driver.rs（段 C）
pub enum WasmHost { Wasi }        // BuildOptions::wasm_host: Option<WasmHost>
```

| builtin | op | 意味 | 使う std 関数 |
| --- | --- | --- | --- |
| `Os.__read` | 0 | ファイル全体を読む | `File.read_bytes`・`read_text` |
| `Os.__read` | 1 | ディレクトリの名前（各名前の後に NUL、D7 の順） | `Dir.list` |
| `Os.__read` | 2 | 現在のディレクトリ（text は無視） | `Env.current_dir` |
| `Os.__read` | 3 | 環境変数（未設定は kind 1） | `Env.var`（kind 1 を `Ok None` に） |
| `Os.__write` | 0／1 | 作成して切り詰めて書く／作成して追記 | `File.write_*`・`append_text` |
| `Os.__write` | 2／3／4 | `mkdir`（0777）／`rmdir`／`unlink`（data は空） | `Dir.create`・`Dir.remove`・`File.remove` |
| `Os.__clock` | 0／1 | monotonic／unix の ns。`INT64_MIN` は失敗 | `Time.*`（失敗は `Other`, code 0） |

状態値（D2）: 成功は 0、失敗は `(kind << 32) | (code & 0xffffffff)`。kind は `ErrorKind` の宣言順に 1..=7。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/Os.tz`（新規） | `ErrorKind`, `Error`, `message`, `encode`, `error_of_status` | 仕様どおり。`error_of_status` は `status >> 32` を case に、`status as i32`（bit 保持）を code に |
| std | `std/File.tz`・`Dir.tz`・`Env.tz`・`Time.tz`・`Random.tz`・`Path.tz`（新規） | 仕様の全関数 | 「評価順序」の形で包む。文字列の結果は `Utf8String.from_bytes` → `String.from_utf8`、失敗は `InvalidEncoding`（code 0）。名前の列は `String.split (ref separator) (ref all)`（区切りが第 1 引数）で分け、末尾の空要素を除く |
| 登録 | `src/stdlib.rs` | `SOURCES`, `RESERVED_MODULES`, `opaque_record` | 7 ファイルを名前順に、7 名を予約に、`"Random.Pcg"` を opaque に（段 B で `"File.Handle"`） |
| 検査 | `src/check.rs` | `Builtin`、全 builtin の列挙（`Self::IOReadLine,` を含む）、`Builtin::name`、型 scheme の match | 6 variant。scheme は `BuiltinType::Tuple`・`Array`・`Reference`・`Concrete` で表す（`IOReadLine` と同じ部品） |
| 検査 | `src/polymorph.rs` | `Checker::builtin` | IO の判定の隣に `Os*` の判定。`self.module` が 6 モジュールのどれかで `ModuleOrigin::Std` のときだけ許す |
| LLVM | `src/llvm.rs` | builtin の emission の match（`Builtin::IOReadLine \| Builtin::IOWrite => emit_typed_builtin(..)`）と振り分け（`emitter.io_builtin`） | 同じ 2 か所に `Os*` を足し、`emitter.os_builtin`（新規）へ |
| LLVM | `src/llvm_io.rs` | `os_builtin`（新規） | `declare i64 @tsuzuri_os_<name>(…)` を `intrinsics` へ。所有結果は `host_result_slot`／`read_host_result`、`ref utf8string`／`ref [ubyte]` は `IOWrite` と同じく load して pointer と length を渡す。wasm では import 属性を付けない（driver が E2000 にする） |
| LLVM | `src/llvm_io.rs` | `entry` | payload が `Type::Integer(32, true)` なら drop せず `ret i32 %result`（D9） |
| LLVM | `src/llvm.rs` | `exit_code_entry`（新規）, `emit_target`, `console_main` | `exit_code_entry` は `io_entry` かつ payload が i32。`emit_target` は `intrinsics` を書き出す前に `declare i64 @tsuzuri_os_args(` の有無を調べ、`console_main(module, uses_args)` に渡す。true のとき `define i32 @main(i32 %argc, ptr %argv)` で先に `call void @tsuzuri_os_set_args(i32 %argc, ptr %argv)` |
| runtime | `src/runtime/os.c`（新規） | 「生成 IR とランタイム」の関数 | POSIX だけ |
| driver | `src/driver.rs` | `io_runtime` を計算する箇所、runtime を連結する `format!` | `os_runtime = text.contains("declare i64 @tsuzuri_os_")`。native なら `native_runtime` に含め、`os.c` を連結の先頭に置く。wasm32 は E2000、Windows native は E2002 |
| driver | `src/driver.rs` | `run_with_diagnostics` | `status.code()` が 0 以外の `Some(n)` で `llvm::exit_code_entry(module)` なら E2005 `program exited with code {n}` |
| CLI | `src/main.rs`（段 C） | `--wasm-feature` の解析の隣 | `--wasm-host wasi`、help 行、`parse` のテスト |

### 生成 IR とランタイム

```llvm
declare i64 @tsuzuri_os_read(ptr, i32, ptr, i64)
declare i64 @tsuzuri_os_args(ptr)
declare i64 @tsuzuri_os_write(i32, ptr, i64, ptr, i64)
declare i64 @tsuzuri_os_random(ptr, i64)
declare i64 @tsuzuri_os_clock(i32)
declare i64 @tsuzuri_os_sleep(i64)
declare void @tsuzuri_os_set_args(i32, ptr)   ; Env.args を使う場合だけ
```

- 所有結果を返す関数は第 1 引数が descriptor（`struct tz_os_buffer { unsigned char *data; int64_t length; }`（新規）。io.c の `tz_io_buffer` と
  同じ配置だが、同じ翻訳単位に連結されるので名前を分ける）。runtime は成功・失敗の全経路で descriptor を書く（失敗は NULL／0）。
- os.c の先頭で `#ifndef` を付けて `_DARWIN_C_SOURCE`（macOS）／`_GNU_SOURCE`（Linux）を定義する（`-std=c11` は POSIX 宣言を隠す）。
  このため driver は os.c を task／CPU／IO ランタイムより前に連結する。static 関数は `tz_os_` 接頭辞にする。
- すべての `open` に `O_CLOEXEC`。読み込みは `open` → `fstat`（ディレクトリなら `EISDIR`）→ EINTR を再試行する `read` の繰り返しで一時領域を倍々に広げ、
  最後に `tsuzuri_alloc` へ一度だけ複写する。書き込みは `O_WRONLY|O_CREAT|O_TRUNC`（追記は `O_APPEND`）、mode 0666、全量を書いて `close` の失敗も返す。
- `Dir.list` は `opendir`／`readdir`／`closedir`。`.` と `..` を除き、名前を byte の辞書順（`memcmp`、同じ接頭辞は短い方が先）で `qsort` する。
- 環境変数名が空・`=` を含む・NUL を含む場合、path が NUL を含む場合は `InvalidInput`（code 0）。`getenv` の NULL は kind 1。
- 乱数は `getentropy` を 256 byte ずつ。時計は `clock_gettime`。`tsuzuri_os_set_args` は `argc`／`argv` を static 変数に保存し、未設定（`tsuzuri test` の
  実行ファイルなど）なら `Env.args` は空配列を返す。

### アルゴリズム

errno の対応（D3）:

| errno | kind |
| --- | --- |
| `ENOENT`, `ENOTDIR` | NotFound (1) |
| `EACCES`, `EPERM`, `EROFS` | PermissionDenied (2) |
| `EEXIST` | AlreadyExists (3) |
| `EINVAL`, `ENAMETOOLONG`, `EISDIR` | InvalidInput (4) |
| `EILSEQ` | InvalidEncoding (5) |
| `EINTR`（再試行し尽くせない場合だけ。段 A では返らない） | Interrupted (6) |
| それ以外（`ENOTEMPTY`, `ELOOP`, `ENOSPC`, `EIO` を含む） | Other (7) |

`Random.Pcg` は PCG-XSH-RR 64/32（pcg-c-basic の `pcg32_srandom_r`／`pcg32_random_r` と同じ）。乗算は `Int.widening_mul` と `as i64u` で
2^64 を法として計算する（通常の `*` と `+` は overflow でトラップするため）。

```text
step(s)        = (s * 6364136223846793005 + increment) mod 2^64
pcg seed seq   : increment = (seq << 1) | 1; state = 0; state = step(state); state = state + seed (mod 2^64); state = step(state)
next_u32(p)    : old = p.state; p.state = step(old)
                 xorshifted = (((old >> 18) ^ old) >> 27) mod 2^32; rot = old >> 59
                 output = rotate_right_32(xorshifted, rot)
next_u64(p)    : hi = next_u32; lo = next_u32; output = (hi << 32) | lo
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る（GUIDE §3.1）。
手順 1–13 が段 A、手順 14 が段 B、手順 15 が段 C。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドと次を実行し、IO だけを使うプログラムの IR を保存する（手順 12 で比べる）。
- 確認: すべて成功する。stack-depth の 3 テストと特殊化上限のテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
node tests/io.mjs target/release/tsuzuri
mkdir -p /tmp/tz-e08/hello && printf 'def main :: IO<unit> =\n    do! IO.write_line "hi"\n' > /tmp/tz-e08/hello/Main.tz
for o in -O0 -O3; do
  target/release/tsuzuri build /tmp/tz-e08/hello --emit llvm $o -o /tmp/tz-e08/before$o.ll
  target/release/tsuzuri build /tmp/tz-e08/hello --emit llvm --target wasm32 $o -o /tmp/tz-e08/before-wasm$o.ll
done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: `std/Os.tz` とモジュール名の予約

- 変更: `std/Os.tz`（新規）、`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`reserves_the_d07_table`、`tests/os_api.rs`（新規）。
- 内容: 7 名をまとめて予約し（件数 27）、`Os` の 5 要素を書く。`tests/os_api.rs` に `tests/stdlib.rs` と同じ形の `accepts`／`rejects`
  （`tsuzuri::analyze_modules` を使い、同梱の std を読む）と IR を 2 回出して一致を見る `ir(sources, wasm)` を置き、`reserves_os_module_names` を足す。
- 確認: `cargo test --locked --lib stdlib` が成功、`cargo test --locked --test os_api` が `1 passed`。

### 手順 3: builtin と E1022

- 変更: `src/check.rs` の `Builtin` と全列挙・`name`・型 scheme、`src/polymorph.rs` の `Checker::builtin`。
- 内容: 「データ構造」の 6 variant。許可するモジュールは `["File", "Dir", "Env", "Time", "Random", "Os"]` と `ModuleOrigin::Std` の両方。
- 確認: `os_primitives_are_private_to_std` を足して `cargo test --locked --test os_api` が `2 passed`。`cargo test --locked --lib stdlib` が成功（std の定義が builtin 名を隠さない）。

### 手順 4: LLVM の `os_builtin` と `std/File.tz`

- 変更: `src/llvm_io.rs` の `os_builtin`（新規）、`src/llvm.rs` の builtin の 2 か所、`std/File.tz`（新規）、`SOURCES`。
- 内容: `IOReadLine` と同じく状態値を検査する（`icmp eq i64 %s, 0` または `ashr` した kind が 1..=7、違反は `TrapKind::BoundsCheck`）。
- 確認: `os_builtins_declare_runtime_only_when_reached`・`os_api_signatures_type_check` を足して `cargo test --locked --test os_api` が `4 passed`。

### 手順 5: `src/runtime/os.c` と driver

- 変更: `src/runtime/os.c`（新規。`tsuzuri_os_read` の op 0・`tsuzuri_os_write` の op 0/1/4・errno の対応）、`src/driver.rs`、`tests/os.mjs`（新規）。
- 内容: `os_runtime` の判定、連結の先頭への挿入、wasm32 の E2000、Windows native の E2002。`tests/os.mjs` は `tests/io.mjs` の `execute` と
  `mkdtempSync` を写し、ケースごとに別の project と別の作業ディレクトリを使う。
- 確認: `clang -std=c11 -Wall -Wextra -Werror -c src/runtime/os.c -o /tmp/tz-e08/os.o` が警告なし。
  `cargo build --release --locked && node tests/os.mjs target/release/tsuzuri` で E2E の 1–6 と 15 が成功。

### 手順 6: `Dir` と `Path`

- 変更: `std/Dir.tz`・`std/Path.tz`（新規）、`os.c` の op 1・`tsuzuri_os_write` の op 2/3。
- 確認: E2E の 7、8、13 が成功。

### 手順 7: `Env` と `@main` の引数

- 変更: `std/Env.tz`（新規）、`os.c` の op 2/3・`tsuzuri_os_args`・`tsuzuri_os_set_args`、`src/llvm.rs` の `emit_target`・`console_main`。
- 確認: `args_entry_receives_argv` と `io_only_programs_keep_their_main` を足して `cargo test --locked --test os_api` が `6 passed`。E2E の 9 が `--debug-info` ありでも成功。

### 手順 8: `Time` と `Random`

- 変更: `std/Time.tz`・`std/Random.tz`（新規）、`os.c` の `tsuzuri_os_clock`・`tsuzuri_os_sleep`・`tsuzuri_os_random`、`opaque_record` に `"Random.Pcg"`。
- 確認: `pcg_is_opaque` を足して `7 passed`。E2E の 10–12 が成功。

### 手順 9: `IO<i32>` の終了コード（D9 の承認後）

- 変更: `src/llvm_io.rs` の `entry`、`src/llvm.rs` の `exit_code_entry`（新規）、`src/driver.rs` の `run_with_diagnostics`。
- 内容: 先に `grep -rn "IO<i32>" tests examples _docs docs` で既存の `IO<i32>` 入口を探し、見つかれば停止する（終了コードが変わる）。
- 確認: `exit_code_entry_returns_the_value` を足して `8 passed`。E2E の 14 が成功。

### 手順 10: 確保追跡と ASan／UBSan

- 変更: `tests/os.mjs`。
- 内容: `tests/io.mjs` の形（`--emit llvm` の IR、runtime の C、確保を数える harness を `-fsanitize=address,undefined` で結合し `live == 0`）を写し、
  runtime の C は `src/runtime/os.c` と `src/runtime/io.c` を渡す。
- 確認: E2E の 16 が `-O0`・`-O3` で成功。

### 手順 11: WASM

- 変更: `tests/os.mjs`。
- 確認: E2E の 12、13（純粋な関数、import が空）と 15（E2000）が wasm32 の `-O0`・`-O3` で成功。

### 手順 12: 既存 IR の不変と全体の確認

- 確認: 手順 1 の 4 つの IR を作り直し、`cmp` で byte 単位に一致する。`cargo test --locked` と GUIDE §3.1 が IO／std の変更に挑げる suite（`tests/io.mjs`、
  `tests/features.mjs`）、手順 1 の stack-depth 3 テストと特殊化上限テストが成功する。

### 手順 13: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs _docs/library-reference/os.md _docs/library-reference/io.md _docs/guides/webassembly.md` が成功。

### 手順 14: 段 B `File.Handle`（B07 が done のときだけ）

- 内容: 「Phase 1 段 B・段 C」の形。`Os.__close` の呼び出し回数を E2E で数え、正常終了・早期の `return`・トラップの前の drop の各経路で
  一度だけ close されること、`/dev/fd`（macOS）／`/proc/self/fd`（Linux）の数が実行前後で等しいことを確かめる。

### 手順 15: 段 C `--wasm-host wasi`（D10 の承認後）

- 内容: `src/main.rs` の option、`BuildOptions::wasm_host`、`src/runtime/os-wasi.c`。`tests/os.mjs` の native ケースを Node の
  `new WASI({ version: "preview1", args, env, preopens })`（`node:wasi`）で実行する。
- 確認: `--wasm-host` なしの出力が手順 1 の wasm32 IR と一致し、`node tests/io.mjs target/release/tsuzuri` が変更なしで成功する。

## テスト計画

### Rust テスト

`tests/os_api.rs`（新規）。拒否はコードとメッセージの一部を検査する。

| テスト | 検査すること |
| --- | --- |
| `reserves_os_module_names` | 利用者の 7 モジュール名がそれぞれ E1011（`is reserved for the standard library`） |
| `os_primitives_are_private_to_std` | Main からの 6 builtin の参照がそれぞれ E1022（`operating-system primitives are private`） |
| `os_api_signatures_type_check` | 仕様の全関数を型注釈付きの `let` で受ける Main が受理される |
| `os_builtins_declare_runtime_only_when_reached` | `File.read_bytes` だけの IR に `declare i64 @tsuzuri_os_read(ptr, i32, ptr, i64)` があり、`@tsuzuri_os_write` がない。宣言に `readnone`／`readonly`／`memory(` がない。native・wasm とも 2 回の出力が一致 |
| `io_only_programs_keep_their_main` | `IO.write_line` だけの IR に `tsuzuri_os_` がなく、`define i32 @main()` のまま |
| `args_entry_receives_argv` | `Env.args` を使う IR に `define i32 @main(i32 %argc, ptr %argv)` と `call void @tsuzuri_os_set_args(i32 %argc, ptr %argv)` |
| `pcg_is_opaque` | Main の `Random.Pcg { state: 1i64u, increment: 2i64u }` が、同じテストで求めた `IO.IO { work: \() -> 42 }` の拒否と同じコード |
| `exit_code_entry_returns_the_value` | `IO<i32>` 入口の `@tsuzuri_main` が `ret i32 0` を含まない。`IO<i64>` 入口は `ret i32 0` のまま |

### E2E

`tests/os.mjs`（新規）。実行: `node tests/os.mjs target/release/tsuzuri`。native のケースは `-O0`・`-O3` の実行ファイルを `build` し、
`mkdtempSync(join(tmpdir(), "tsuzuri-os-"))` の下のケース専用ディレクトリを cwd にして直接起動する（`run` は引数を渡さない）。プログラムは結果を
`Os.message` などで 1 行ずつ出し、期待値は Node の `fs`・`os.constants.errno`・BigInt で独立に求める。終了時に一時ディレクトリを `rmSync` する。

1. `write_text "a.txt" "héllo\n"` → `read_bytes` が `Buffer.from("héllo\n")` と一致。`append_text` 後の `read_text`。ファイルの中身を Node でも読んで比べる。
2. 存在しないファイル → NotFound、code = `os.constants.errno.ENOENT`。`File.remove` 後の `read_bytes` も同じ。
3. `chmod 000` のファイル → PermissionDenied、`EACCES`（`process.getuid() === 0` なら skip を表示）。
4. Node が書いた `ff fe 41` の `read_text` → InvalidEncoding、code 0。`read_bytes` は 3 byte を返す。
5. ディレクトリの `read_bytes` → InvalidInput、`EISDIR`。
6. path `"a\0b"` → InvalidInput、code 0。path `"\uD800"` → InvalidEncoding、code 0。どちらもファイルが作られない。
7. `Dir.create` の 2 回目 → AlreadyExists、`EEXIST`。Node が作る `b`、`a`、`A`、`ab`、`é`、`\u{FF61}`、`\u{1F600}` の `Dir.list` が、`readdirSync` を
   UTF-8 の `Buffer.compare` で整列した列と一致（`\u{FF61}` が `\u{1F600}` より先。UTF-16 順とは逆）。空でない `Dir.remove` → Other、`ENOTEMPTY`。
   Linux だけ: 名前 `66 ff` のファイルがあると `Dir.list` が InvalidEncoding（macOS は作成できないので skip）。
8. symlink `link -> a.txt`: `read_text "link"` は対象の中身、`Dir.list` に `link` があり、`File.remove "link"` の後も `a.txt` が残る。
9. `Env.args` を引数 `["α", "", "b c"]` で起動 → 同じ 3 要素。`Env.var` は `{ TZ_E08: "värde" }` で Some、未設定で None、名前 `"A=B"`・`""` で InvalidInput。
   `Env.current_dir` は `realpathSync(cwd)`（macOS の `/tmp` は `/private/tmp`）。
10. `monotonic_ns` を `sleep_ms 50` の前後で読み、差 ≥ 50,000,000。`unix_ns` と `BigInt(Date.now()) * 1000000n` の差が 60 秒以内。`sleep_ms -1` → InvalidInput。
11. `Random.bytes 0` は空、`bytes 1000` は 1000 byte で 2 回の結果が異なる、`bytes -1` と `bytes 1073741825` は InvalidInput、`next_u64` は Ok。
12. `Random.pcg 42i64u 54i64u` の `pcg_next_u32` 6 回と `pcg_next_u64` 3 回が BigInt の参照実装と一致。参照実装は先に pcg-c-basic の
    demo 出力 `0xa15c02b7 0x7b47f409 0xba1d3330 0x83d2f293 0xbfa4784b 0xcbed606e` と照合し、一致しなければ停止する。native と wasm32。
13. 「例」の `Path` の全ケースと `Os.error_of_status`（`(2n << 32n) | 13n` → PermissionDenied、code 13、`message` が `permission denied (os error 13)`）。native と wasm32。
14. `IO<i32>` 入口が 3 を返す → 実行ファイルの status 3、`tsuzuri run` は E2005 `program exited with code 3`。返り値 0 と `IO<unit>` は status 0。
15. `File.read_text` を使う project の `build --target wasm32`（既定・`--emit llvm`・`--emit object`、`-O0`・`-O3`）が E2000 で失敗し、出力ファイルを作らない。
16. 1–9、11 を ASan／UBSan と確保追跡で実行し `live == 0`。

### 既存テストへの影響

- `src/stdlib.rs` の `reserves_the_d07_table` の件数が 20 から 27 になる（予約の追加による正当な変更）。それ以外はなし。
- `tests/io.mjs` の import の期待値は段 A・B では変わらない。段 C も `--wasm-host` なしの経路は変えない。

### 性能

計測しない。OS 呼び出しの費用が支配的で、op 番号の `switch` と `IO.map` の closure は誤差の範囲。速度の主張はしない。

## ドキュメント

- `docs/language.md`: 「IO と標準入出力」の後に「OS API」節（概要・`Os.Error`・WASM と Windows の扱い・symlink・決定性）。「IO と標準入出力」の
  「WASI や非同期イベントループは追加しません」を段 C までは「既定の wasm32 は OS API を E2000 で拒否する」に更新。`IO<i32>` の終了コード。「診断」の E2000／E2002／E2005。
- `docs/architecture.md`: 「標準入出力」節に `Os.__*` builtin、状態値、`os.c` の連結条件と連結順、`@main` の引数。
- `_docs/library-reference/os.md`（新規）と `_docs/library-reference/README.md` の一覧。`_docs/library-reference/io.md` の「WASM と C」。
- `_docs/guides/webassembly.md` の「extern import」の後に、OS API は既定の wasm32 で使えないことと `Path`／`Random.Pcg` は使えること。
- `README.md`: 未実装一覧から OS の標準ライブラリを外し、`node tests/io.mjs target/release/tsuzuri` の次に `node tests/os.mjs target/release/tsuzuri`。
- `_docs/feature-status.md` と `_features/README.md` の状態。D1 の承認後、GUIDE D-30 の仮割り当てを D-07 へ移すのは人間が行う。

## 受け入れ条件

- [ ] 段 A の全 API が macOS と Linux の native で仕様どおり動き、`tests/os.mjs` の 1–16 が `-O0`・`-O3` で成功する。
- [ ] `tests/os_api.rs` の 8 テストが成功し、`cargo test --locked` が成功する。
- [ ] 既定の wasm32 は OS API を E2000 で拒否し、`Path`／`Random.Pcg` だけの出力の import は空。IO だけのプログラムの IR は手順 1 と byte 単位で一致する。
- [ ] runtime の一時領域と所有結果に漏れがない（ASan、`live == 0`）。
- [ ] （段 B）handle がすべての経路で一度だけ close される。（段 C）WASI の import は `--wasm-host wasi` のときだけ。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `IO.IO` は opaque なので `File.tz` から `IO { work: … }` は書けない。`IO.map (\() -> …) (IO.pure ())` を使う。構築時に OS を呼ぶ形（`IO.pure (read_now path)`）は
  遅延の意味を壊す。E2E 1 で「IO 値を作るだけではファイルができない」ことも見る。
- driver は runtime を一つの C ファイルに連結する。`struct tz_io_buffer` や static 関数の名前が衝突すると IO と OS を両方使うプログラムだけが
  コンパイルできない。E2E 1 は `IO.write_line` も使う。feature macro は最初の `#include` より前でないと効かず、`-std=c11` で
  `nanosleep`／`getentropy` が暗黙の宣言になる。`grep -n "_POSIX_C_SOURCE\|_GNU_SOURCE\|_DARWIN_C_SOURCE" src/runtime/*.c` で既存の定義と揃える。
- `getentropy` の header は macOS が `<sys/random.h>`、glibc（≥ 2.25）と musl が `<unistd.h>`。1 回 256 byte を超えると `EIO`／`EINVAL`。
- `Dir.list` を `String` の比較で整列すると UTF-16 順になり、補助面の文字で D7 と違う。整列は C の byte 比較だけで行う。OS の返す順のままにすると非決定的。
- `String.split` は区切りが第 1 引数（HEAD で確認）。逆にすると全体が 1 要素になる。末尾の空要素の有無は手順 6 の前に scratch で確かめる。
  引数には空文字列があり得るので「空要素をすべて捨てる」実装は E2E 9 で失敗する。
- 通常の `*`／`+` は overflow でトラップする。PCG の計算は `Int.widening_mul` と i128u の加算、`as i64u`（bit 保持）で行う。
- `console_main` の出力は `debug::wrapper` を通る。`@main` に引数を足した後、`--debug-info` の build で IR が壊れていないかを E2E 9 で見る。
- macOS の `/tmp` は `/private/tmp` への symlink。`getcwd` は実パスを返すので期待値は `realpathSync` で作る。HFS+ は名前を NFD にするので、
  期待値は作成に使った文字列ではなく `readdirSync` の結果から作る。
- root で実行すると `chmod 000` でも読める。skip を表示し、黙って成功にしない。
- パスの正規化や `..` の扱いは OS に任せる。`Path.join` は絶対パスの第 2 引数も連結するだけで、sandbox にならない。アクセス制御を実装したと
  誤解させる文言を文書に書かない。`write_text` は切り詰めてから書くので、途中の失敗で中身が欠ける（原子的ではない）。
- wasm32 の拒否は build だけで起きる。`tsuzuri test --target wasm32` も `io_runtime` を計算する関数を通ることを確かめ、通らなければ停止して報告する。

## 対象外

- ネットワーク（E09）、非同期 IO（B08）、GUI、ファイル監視、ファイルロック、権限・umask の設定、環境変数の設定。
- プロセスの起動、メタデータ、再帰走査、byte 列の path API、Windows（Phase 2、G10）。`tsuzuri run` からの引数の受け渡し。WASI preview2／component model（E13）。

## 決定事項

### D1: std モジュール名

- 決定: `File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Os` を std モジュールとして追加し、`RESERVED_MODULES` に載せる。
- 理由: GUIDE D-30 の仮割り当てどおり。予約は利用者の `Path.tz` などを拒否するので（現状の再現参照）、確定には承認が要る。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: エラー型と状態値

- 決定: `Os.Error { kind: ErrorKind, code: i32 }`。code は errno（Phase 2 の Windows は `GetLastError`）、Tsuzuri が検出した失敗は 0。runtime は
  `(kind << 32) | (code & 0xffffffff)` の i64 を返し、`Os.error_of_status` が復号する。`Os.message` は kind を `not found`・`permission denied`・
  `already exists`・`invalid input`・`invalid encoding`・`interrupted`・`other` とし、code が 0 以外なら ` (os error <code>)` を付ける。
- 理由: `IO.Error` への case 追加は既存の網羅的な match を壊す。一つの scalar なら tuple の型 scheme を増やさず、隠れた errno 状態も持たない。
  `Os.encode`／`error_of_status` を公開するのは、他の std モジュールと E09 が共有するため（純粋なので安全）。
- 状態: 既定案（実装者はこの案に従う）

### D3: errno の対応

- 決定: 「アルゴリズム」の表。EINTR は runtime が再試行し、段 A の API は Interrupted を返さない。
- 理由: 利用者が分岐したい失敗だけを kind にし、残りは code で区別できる。kind の追加は非互換変更で承認が要る（G19 の D8）ので最小に保つ。
- 状態: 既定案（実装者はこの案に従う）

### D4: path の表現と欠損する場合

- 決定: API の path・名前・値は `string`。境界で UTF-8 にして POSIX の byte 列として渡す。孤立サロゲートは InvalidEncoding、NUL は InvalidInput。
  OS から得た名前・値・内容が UTF-8 でなければ呼び出し全体が InvalidEncoding で、置換文字は使わない。Unicode 正規化はしない。
- 理由: 利用者の文字列は `string` が標準で、`IO.read_line` も厳密変換。黙った置換は別のファイルを指す危険がある。byte 列の path は Phase 2。
- 状態: 既定案（実装者はこの案に従う）

### D5: builtin の形

- 決定: 「データ構造」の 6 builtin と op 番号。所有結果は `(i64 * [ubyte])`。`Env.args` だけ別 builtin。
- 理由: builtin ごとに check・polymorph・llvm の複数箇所が要るので数を抑える。op の `switch` は OS 呼び出しに比べて無視できる。
  `Os.__args` を分けるのは、到達したときだけ `@main` の形を変えるため。
- 状態: 既定案（実装者はこの案に従う）

### D6: runtime と連結条件

- 決定: `src/runtime/os.c` を IR に `declare i64 @tsuzuri_os_` があるときだけ native に連結する（`io.c` と同じ判定）。連結の先頭に置く。
- 理由: 使わないプログラムのビルド・大きさ・cache の key を変えない。feature macro を最初の include より前に置く必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D7: `Dir.list` の順序

- 決定: UTF-8 の byte 辞書順（Unicode スカラー値の順と同じ）。`.`・`..` を除く。locale と大文字小文字を考慮しない。
- 理由: OS・file system によらず決定的で、Phase 2 の Windows でも UTF-8 化して同じ順にできる。
- 状態: 既定案（実装者はこの案に従う）

### D8: 時刻と乱数の決定性

- 決定: 時刻と OS 乱数は IO の中でだけ読める（IO が能力の境界）。大域の seed・暫定の既定乱数源・`Random.seed` の設定は作らない。
  決定的な乱数は値として渡す `Random.Pcg`（PCG-XSH-RR 64/32、明示 seed と stream）。
- 理由: 同じ入力の純粋な計算は同じ結果になり、wasm32 でも import なしで動く。PCG32 は公開の参照出力があり独立に検証できる。
- 状態: 既定案（実装者はこの案に従う）

### D9: `IO<i32>` 入口の終了コード

- 決定: 入口が `IO<i32>` のとき、その値を `@main` の戻り値にする（POSIX の観測値は下位 8 bit）。途中終了の `Os.exit` は作らない。
  `tsuzuri run` は非 0 を E2005 `program exited with code <n>` で報告する。
- 理由: drop を飛ばす `exit` より安全。HEAD では `IO<i32>` の値を捨てて 0 で終わる（再現済み）ので、既存プログラムの意味が変わる。
- 状態: 要承認（承認前は手順 9 に着手しない）

### D10: WASM の拒否と `--wasm-host wasi`

- 決定: 段 A・B の既定 wasm32 は `Os.__*` に到達したビルドを E2000 で拒否する。段 C で `--wasm-host wasi` を足し、指定時だけ WASI preview1 の
  到達した import を出し、既存の標準入出力も同じ経路にする。preview2／component model は E13 と合わせて検討する。
- 理由: D-18 の「既定は import なし」。`--wasm-feature` は clang で有効にする WebAssembly の機能（`simd128`／`threads`）で、ホストの種類とは軸が違う。
  preview1 は Node（`node:wasi`）と主要ランタイムが実装済み。
- 状態: 要承認（承認前は段 C に着手しない。E2000 による拒否は承認を待たず段 A で実装する）

### D11: Windows

- 決定: 段 A・B は macOS／Linux だけ。Windows の native で `Os.__*` に到達したビルドは E2002。wide API の実装は Phase 2 の設計方針どおり G10 と行う。
- 理由: G10 は blocked で Windows の実行検証ができず、検証できない C を完了と扱わない。
- 状態: 既定案（実装者はこの案に従う）

### D12: `File.Handle` と B07

- 決定: handle は段 B。B07 が done になるまで着手せず、段 A は B07 を待たない。handle は所有値として操作に渡して返す。
- 理由: close を保証するには利用者定義の drop が要る。IO の closure は借用を捕捉できない。ファイル全体の API は資源を持ち越さない。
- 見直し提案: B07 の D5 は、関数値の複製が捕捉値を複製するため Drop 型を関数値に捕捉できない（E1005）と決めた。`IO<'a>` は
  `unit -> 'a` の closure なので、`read :: Handle -> i64 -> IO<...>` のように Drop 型の handle を IO の値へ渡す設計は、そのままでは
  B07 D5 と両立しない（E09 の D4 も同じ前提に立つ）。段 B の着手前に人間が次から選ぶ。推奨は (a)。
  (a) 複製できない（一度だけ実行する）関数値の型を別チケットで導入し、IO の closure をその型にする（B07 の対象外に挙がっている拡張）。
  (b) handle を Drop にせず、明示の `close` だけで閉じる（閉じ忘れは確保追跡の `live == 0` と同様の harness で検出する）。
- 状態: 要承認（承認前は段 B に着手しない。段 A は影響を受けない）

### D13: symlink と正規化

- 決定: Tsuzuri は path を正規化・解決しない。読み書きは symlink をたどり（`O_NOFOLLOW` なし）、`File.remove` は link 自体を消し、`Dir.list` は
  link を名前として返す。`Dir.remove` を directory への symlink に使うと `ENOTDIR` で NotFound。`Path.join` は絶対パスで置き換えない。
  存在確認 API は作らない（確認と使用の間の競合を排するため、操作の結果で判断する）。
- 理由: POSIX の既定と同じで驚きが少ない。正規化や連結の工夫は安全だと誤解させる。制限は利用者とホストの責任として文書化する。
- 状態: 既定案（実装者はこの案に従う）

### D14: コマンドライン引数

- 決定: `Env.args` は argv[0] を除く。`Os.__args` に到達したときだけ `@main(i32 %argc, ptr %argv)` と `tsuzuri_os_set_args` を出す。
  `tsuzuri test` の実行ファイルなど set されない場合は空配列。
- 理由: F# の `argv`・.NET の `Main(args)` と同じ。`_NSGetArgv` や `/proc/self/cmdline` のような OS 固有の手段に頼らず、他のプログラムの IR を変えない。
- 状態: 既定案（実装者はこの案に従う）
