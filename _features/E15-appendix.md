# E15 付録: 決定表・マニフェスト・診断・出力例・骨格・テスト

本文は [E15-trust-boundary-and-capabilities.md](E15-trust-boundary-and-capabilities.md)。HEAD `a2313d19`（作業ツリーは同じコードの `9dd0fbbc`）で確認した。出力に「（想定出力）」とあるものは、未実装の機能の出力で、実機の出力ではない。

## 付録 A: std の権限の決定表（完全）

### A.1 導出の規則

1. 世界に触れる**源**は 3 種類だけ: (a) 組み込みの私的関数と `Bench.now`（A.2。`Bench.now()` を除き、利用者は `E1022` で直接呼べない。`Bench.now()` は `bench` の中だけ許される（`E1018`））、(b) `extern` の関数（`ffi`）、(c) A.3 の明示の 5 つ（`Gpu`）。
2. std の関数 `f` の権限 = `f` の本体が直接参照する源の権限 ∪ `f` が参照する std の関数の権限（推移閉包）。参照は式木の `Function(FunctionRef::User(id))`・`GenericFunction(id, _)`・`Closure(id, _)`・`Function(FunctionRef::Builtin(_))` の**出現のすべて**（呼び出しでなくてもよい。`closures::lower` と特殊化が作る生成関数（モジュール名が `$` で始まるもの）は `origin.parent` の連鎖で元の関数に属する）。
3. 利用者（ルート・依存パッケージ）の関数の**使用** = 本体が参照する std の関数の権限（参照ごとに 1 地点）と、参照する `extern` の関数の `ffi`。利用者の関数どうしの参照は使用ではない（各パッケージは自分の本体だけを検査される）。
4. `Os.__read` と `Os.__write` は操作番号で多重化されているので、**参照する関数が属する std のファイル**（`SourceInfo.name`）で決める（A.2）。

### A.2 組み込み関数の表（`Builtin::name()` の 244 個のうち、`__` を含むか `Bench.` で始まる 46 個）

世界に触れるもの（33 個）:

| `Builtin` の名前 | 権限 |
| --- | --- |
| `Debug.__print_string` | `stdio` |
| `IO.__read_line`・`IO.__write` | `stdio` |
| `Os.__read` | `File`・`Dir` の関数から参照されたら `fs`、`Env` の関数から参照されたら `env`（ほかのファイルからの参照は表にない。本文の手順 15 のテストで 0 件を確かめる） |
| `Os.__write` | `fs`（`File`・`Dir` の関数から） |
| `Os.__open`・`Os.__handle`・`Os.__close` | `fs` |
| `Os.__args` | `env` |
| `Os.__random` | `random` |
| `Os.__clock`・`Os.__sleep` | `time` |
| `Os.__spawn` | `process` |
| `Net.__resolve`・`__open`・`__accept`・`__read`・`__write`・`__close`・`__watch`・`__unwatch`・`__connect`・`__names`・`__send`（11 個） | `net` |
| `Gpu.__open`・`__features`・`__run`・`__select`・`__last`（5 個） | `gpu` |
| `Async.__clock`・`Async.__wait`・`Async.__posted` | `time`（`Async.block_on` の実時計と待機） |
| `Bench.now` | `time` |

権限なしの私的な組み込み関数（13 個）: `Net.__classify`（errno の分類）、`Unicode.__table_length`・`__table_entry`、`Arena.__next_id`、`Async.__resume`・`__take`・`__put`・`__next_id`・`__retire`（実行器の内部状態）、`Channel.__close_sender`・`__close_receiver`、`Gen.__seed`（`tsuzuri test --seed` か既定の定数。実行時に環境を読まない）、`Bench.consume`（最適化の壁）。

そのほかの組み込み関数（198 個。244 個から上の 46 個を除いた残りのすべて）は CPU とメモリだけに触れる。`Task`・`Parallel`・`Atomic`・`Mutex`・`Channel` は CPU の資源であって権限ではない（D-57）。

### A.3 明示の表（導出できない 5 つ）

`gpu_devices::retarget_calls` は、非 CPU のバックエンドを名指すプログラムだけ、呼び出しを私的な `*_on` へ付け替える（`check_modules_collect` の `polymorph::specialize` より前）。付け替えのないプログラムでは公開関数の本体が源を参照しないので、次の 5 つは**常に `gpu`** とする（バックエンドに依らず、意図で決める）。

| 関数 | 権限 |
| --- | --- |
| `Gpu.request`・`Gpu.init`・`Gpu.map`・`Gpu.init_relaxed`・`Gpu.map_relaxed` | `gpu` |

`Gpu.last_backend` は `Gpu.__last` を参照するので導出で `gpu`。`Gpu.backend`・`Gpu.from_array`・`Gpu.to_array` は権限なし。

### A.4 std の 47 ファイルの結果

`ls std/*.tz std/*.tc | wc -l` が 47。`def` は 949 個（`grep -c '^def ' std/*.tz std/*.tc | awk -F: '{s+=$2} END {print s}'` が公開 559、`'^private def '` が非公開 390）。`std` に `extern` はない（`grep -c extern std/*.tz std/*.tc` がすべて 0）。列: 公開関数の数・非公開関数の数・権限を持つ公開関数の数。権限を持つ関数は A.5 に全件ある。

| モジュール（ファイル） | 公開 | 非公開 | 権限 | 権限を持つ公開関数 |
| --- | --- | --- | --- | --- |
| Arena | 10 | 3 | なし | 0 |
| Array | 39 | 6 | なし | 0 |
| Async（`.tc`） | 16 | 40 | `time` | 1（`block_on`） |
| Atomic | 0（record だけ） | 0 | なし | 0 |
| Bench | 2 | 0 | `time` | 2 |
| BigInt | 4 | 12 | なし | 0 |
| Cbor | 4 | 21 | なし | 0 |
| Channel | 0（record と `Drop`） | 0 | なし | 0 |
| Char | 6 | 0 | なし | 0 |
| Debug | 2 | 0 | `stdio` | 2 |
| Dir | 4 | 4 | `fs` | 4 |
| Env | 3 | 3 | `env` | 3 |
| Exception | 0 | 1 | なし | 0 |
| File | 14 | 8 | `fs` | 14 |
| Format | 2 | 10 | なし | 0 |
| Gen | 22 | 16 | `stdio`（失敗の報告が `Debug.print`） | 3 |
| Gpu | 9 | 4 | `gpu` | 6 |
| HashMap | 24 | 12 | `random` | 2 |
| HashSet | 17 | 0 | `random` | 2 |
| IO（`.tc`） | 25 | 2 | `stdio` | 11 |
| Json | 38 | 69 | なし | 0 |
| List | 5 | 0 | なし | 0 |
| Map | 14 | 3 | なし | 0 |
| Math | 1 | 1 | なし | 0 |
| Matrix | 18 | 4 | なし | 0 |
| MatrixView | 29 | 2 | なし | 0 |
| Maybe（`.tc`） | 25 | 0 | なし | 0 |
| Mutex | 0（record だけ） | 0 | なし | 0 |
| Net | 40 | 45 | `net` | 29 |
| Os | 6 | 0 | なし（エラー値の変換だけ） | 0 |
| Owned | 0（record だけ） | 0 | なし | 0 |
| Parallel | 1 | 0 | なし | 0 |
| Path | 4 | 2 | なし（文字列の操作だけ。D-57 の確定） | 0 |
| Process | 1 | 3 | `process` | 1 |
| Random | 5 | 4 | `random`（`pcg`・`pcg_next_u32`・`pcg_next_u64` は純粋） | 2 |
| Regex | 18 | 63 | なし | 0 |
| Result（`.tc`） | 26 | 0 | なし | 0 |
| Seq | 7 | 0 | なし | 0 |
| Set | 13 | 5 | なし | 0 |
| String | 21 | 4 | なし | 0 |
| Tensor | 27 | 6 | なし | 0 |
| Test | 3 | 0 | なし | 0 |
| Time | 3 | 2 | `time` | 3 |
| Unicode | 24 | 29 | なし | 0 |
| Utf8Char | 6 | 0 | なし | 0 |
| Utf8String | 20 | 5 | なし | 0 |
| Vec | 1 | 1 | なし | 0 |

合計: 公開 559（うち権限を持つもの 85）。`Net` の権限を持たない 11 個は `parse_address`・`parse_ip`・`ip_text`・`address_text`・`port`・`is_ipv6`・`error_kind`・`stream_local_addr`・`peer_addr`・`local_addr`・`udp_local_addr`（アドレスの解析と取り出し）。`Dir` の 4 個、`File` の 14 個、`Env` の 3 個はすべて権限を持つ。

### A.5 権限を持つ公開関数 85 個（`tests/fixtures/capabilities/std-table.txt` の内容。1 行 1 関数、この順）

```text
Async.block_on: time
Bench.with_input: time
Bench.of: time
Debug.print: stdio
Debug.trace: stdio
Dir.list: fs
Dir.create: fs
Dir.remove: fs
Dir.walk: fs
Env.args: env
Env.var: env
Env.current_dir: env
File.read_bytes: fs
File.read_text: fs
File.write_bytes: fs
File.write_text: fs
File.append_text: fs
File.remove: fs
File.open: fs
File.read: fs
File.write: fs
File.flush: fs
File.close: fs
File.with_open: fs
File.metadata: fs
File.link_metadata: fs
Gen.check: stdio
Gen.for_all: stdio
Gen.for_all_cases: stdio
Gpu.request: gpu
Gpu.init: gpu
Gpu.map: gpu
Gpu.init_relaxed: gpu
Gpu.map_relaxed: gpu
Gpu.last_backend: gpu
HashMap.try_randomized: random
HashMap.randomized: random
HashSet.try_randomized: random
HashSet.randomized: random
IO.try_read_line: stdio
IO.read_line: stdio
IO.try_write: stdio
IO.try_write_line: stdio
IO.try_write_error: stdio
IO.try_write_error_line: stdio
IO.write: stdio
IO.write_line: stdio
IO.writeln: stdio
IO.write_error: stdio
IO.write_error_line: stdio
Net.resolve: net
Net.connect: net
Net.read: net
Net.write: net
Net.shutdown: net
Net.close: net
Net.bind: net
Net.accept: net
Net.close_listener: net
Net.bind_udp: net
Net.send_to: net
Net.recv_from: net
Net.close_udp: net
Net.with_connection: net
Net.with_accepted: net
Net.with_listener: net
Net.with_udp: net
Net.connect_async: net
Net.accept_async: net
Net.read_async: net
Net.write_async: net
Net.recv_from_async: net
Net.send_to_async: net
Net.bind_async: net
Net.bind_udp_async: net
Net.close_async: net
Net.close_listener_async: net
Net.close_udp_async: net
Net.shutdown_async: net
Process.run: process
Random.bytes: random
Random.next_u64: random
Time.monotonic_ns: time
Time.unix_ns: time
Time.sleep_ms: time
```

### A.6 std のファイルごとの世界への参照の固定表（`std_world_primitives_are_referenced_only_by_the_known_files` の期待値）

std の各ソースから `//` のコメントを除き、A.2 の 46 個の名前（`Os.__*`・`Net.__*`・`Gpu.__*`・`Debug.__*`・`IO.__*`・`Bench.now`・`Async.__clock|wait|posted` を含む）の出現を数える。次の表のファイルだけが出現を持ち、数も同じ（合計 61）。ほかのファイルは 0。新しい std の関数が世界に触れるようになったら、このテストが落ちる。作者は A.2・A.5・この表を更新する。

| ファイル | 出現 | 数 |
| --- | --- | --- |
| `Async.tc` | `Async.__clock`・`Async.__posted`・`Async.__wait` | 3 |
| `Bench.tz` | `Bench.now` ×4 | 4 |
| `Debug.tz` | `Debug.__print_string` | 1 |
| `Dir.tz` | `Os.__read` ×2・`Os.__write` | 3 |
| `Env.tz` | `Os.__args`・`Os.__read` ×2 | 3 |
| `File.tz` | `Os.__close`・`Os.__handle`・`Os.__open`・`Os.__read` ×2・`Os.__write` | 6 |
| `Gpu.tz` | `Gpu.__features` ×2・`Gpu.__last`・`Gpu.__open` ×2・`Gpu.__run` ×2・`Gpu.__select` ×2 | 9 |
| `IO.tc` | `IO.__read_line`・`IO.__write` | 2 |
| `Net.tz` | `Net.__accept` ×2・`__classify`・`__close` ×8・`__connect`・`__names`・`__open`・`__read` ×4・`__resolve`・`__send` ×2・`__unwatch` ×2・`__watch`・`__write` ×2 | 26 |
| `Process.tz` | `Os.__spawn` | 1 |
| `Random.tz` | `Os.__random` | 1 |
| `Time.tz` | `Os.__clock`・`Os.__sleep` | 2 |

### A.7 std のプローブ（`tests/fixtures/capabilities/probe/Main.tz`。全文。HEAD の `tsuzuri check` が通る）

```tsuzuri
extern "abs" def c_abs :: i32 -> i32

def probe_file :: unit -> IO<Result<string, Os.Error>> = \() -> File.read_text "a.txt"
def probe_dir :: unit -> IO<Result<[string], Os.Error>> = \() -> Dir.walk "."
def probe_env :: unit -> IO<Result<Maybe<string>, Os.Error>> = \() -> Env.var "HOME"
def probe_time :: unit -> IO<Result<i64, Os.Error>> = \() -> Time.monotonic_ns ()
def probe_random :: unit -> IO<Result<i64u, Os.Error>> = \() -> Random.next_u64 ()
def probe_stdio :: unit -> IO<unit> = \() -> IO.write_line "x"
def probe_debug :: unit -> unit = \() -> Debug.print (ref "x")
def probe_net :: Net.Address -> IO<Result<Net.TcpStream, Os.Error>> = \address -> Net.connect address Maybe.None
def probe_net_async :: Net.TcpStream -> Async<Result<[ubyte], Os.Error>> = \stream -> Net.read_async stream 16 Maybe.None
def probe_hash :: unit -> IO<HashMap<string, i64>> = \() -> HashMap.randomized ()
def probe_async :: Async<i64> -> IO<i64> = \job -> Async.block_on job
def probe_gpu :: unit -> Result<Gpu.Device, Gpu.Error> = \() -> Gpu.request Gpu.CpuReference
def probe_process :: unit -> IO<Result<Process.Output, Os.Error>> = \() -> Process.run "true" [] []
def probe_pure_net :: string -> Maybe<Net.Address> = \text -> Net.parse_address (ref text)
def probe_pure_path :: string -> string = \text -> Path.join (ref text) (ref text)
def probe_ffi :: i32 -> i32 = \value -> c_abs value

def main :: unit -> i32 = \() -> 0
```

期待する `analyze` の地点（ルートパッケージ。`line:column` は参照の先頭。14 地点・9 権限。`Net.parse_address`（16:63）と `Path.join`（17:52）は地点にならない）:

| 位置 | 参照 | 権限 | 位置 | 参照 | 権限 |
| --- | --- | --- | --- | --- | --- |
| 3:65 | `File.read_text` | `fs` | 10:83 | `Net.connect` | `net` |
| 4:66 | `Dir.walk` | `fs` | 11:87 | `Net.read_async` | `net` |
| 5:71 | `Env.var` | `env` | 12:61 | `HashMap.randomized` | `random` |
| 6:62 | `Time.monotonic_ns` | `time` | 13:52 | `Async.block_on` | `time` |
| 7:65 | `Random.next_u64` | `random` | 14:65 | `Gpu.request` | `gpu` |
| 8:46 | `IO.write_line` | `stdio` | 15:76 | `Process.run` | `process` |
| 9:42 | `Debug.print` | `stdio` | 18:41 | `c_abs` | `ffi` |

`extern` の関数: `c_abs`（宣言 1:18、symbol `abs`、`@trusted` なし、参照 1 件）。

## 付録 B: マニフェスト

### B.1 文法（`src/package.rs` の手書きの行単位の解析に足す部分）

```text
section      = "[package]" … | "[dependencies]" | "[wasm]" | "[native]" | "[registry]" | "[capabilities]"   各 1 回、[package] のあとに順不同
capabilities = "[capabilities]" NL  "allow" "=" names NL            allow は必須。重複は不可
dependency   = key "=" "{" source [ "," "native" "=" bool ] [ "," "capabilities" "=" names ] "}"
source       = "path" "=" string | "git" "=" string "," "rev" "=" string | "version" "=" string
names        = "[" [ string { "," string } ] "]"                    1 行に収める。名前は env ffi fs gpu net process random stdio time
```

`native` は path 依存だけ。キーの順序は固定（`native` のあとに `capabilities`）。重複と順序違いは `expected '}'`（既存の `native = true, native = true` の期待値のまま）。

### B.2 エラー（すべて `E0002`、主スパンは `Line::error` と同じ「その行」）

| 入力 | メッセージ |
| --- | --- |
| `[capabilities]` を 2 回、または `[package]` より前 | `expected [package] followed by optional [dependencies], [wasm], [native], [registry] and [capabilities], each once` |
| `[capabilities]` に `allow` がない | `[capabilities] requires allow = ["fs", ...]`（主スパンはヘッダーの行。`[registry]` の `index` と同じ作法） |
| `[capabilities]` の未知のキー | `unknown capabilities key; expected allow` |
| `allow` が 2 回 | `capabilities keys must occur at most once` |
| `allow` が配列でない（`allow = "fs"`・`allow = [1]`） | `allow must be an array of capability names on one line, such as ["fs", "time"]` |
| 依存の `capabilities = "fs"` | `capabilities must be an array of capability names on one line, such as ["fs", "time"]` |
| 未知の名前 `"files"` | `unknown capability 'files'; expected one of env, ffi, fs, gpu, net, process, random, stdio, time` |
| 同じ名前が 2 回 | `capability 'fs' is listed twice` |
| path 依存の未知のキー（`link = true`） | `only the path, native, and capabilities keys are supported in a dependency`（HEAD は `only the path and native keys are supported in a dependency`） |
| git 依存・registry 依存の未知のキー | 既存の `GIT_FORM`・`VERSION_FORM` のまま |

実機で確かめた HEAD の挙動（付録 H）: `[capabilities]` は `expected [package] followed by optional [dependencies], [wasm], [native] and [registry], each once`（`E0002`）、依存の `capabilities` は `only the path and native keys are supported in a dependency`（`E0002`）で拒否される。

### B.3 受理される例

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
fscache = { path = "../fscache", capabilities = ["fs"] }
host-lib = { path = "../host", native = true, capabilities = ["ffi"] }
remote = { git = "https://example.org/remote.git", rev = "0123456789abcdef0123456789abcdef01234567", capabilities = [] }
registered = { version = "1.0.0", capabilities = ["net", "time"] }

[capabilities]
allow = ["fs", "net", "stdio", "time"]
```

## 付録 C: 診断

D-51 の型（`Diagnostic`・`Related`・`RelatedRole`）に従う。G22 が未実装のうちは、`.with_kind`・`.related`・`.with_help` を呼ばず、メッセージとコードと主スパンだけを出す（kind・related・help の列は G22 が入ったら足す。呼び出しの追加だけで他は変わらない）。

### C.1 `E1036`（kind `capability_denied`。severity は error）

| 項目 | 内容 |
| --- | --- |
| 主スパン | 使用の地点: 参照の式（`File.read_text` や `c_abs` の識別子）。ルート・依存のどのソースか |
| メッセージ（ルート） | `the root package may not use capability '{cap}' ('{callee}'); add "{cap}" to allow in [capabilities]` |
| メッセージ（依存・阻んだのがルートの `allow`） | `package '{name}' may not use capability '{cap}' ('{callee}'); the root [capabilities] allow does not list "{cap}"` |
| メッセージ（依存・阻んだのが親 `{parent}` の依存の項目） | `package '{name}' may not use capability '{cap}' ('{callee}'); grant it with capabilities = ["{cap}"] in the entry of '{name}' in '{parent}'` |
| `related` | 通った std の関数の連鎖を `ViaCall`（`'{from}' calls '{to}'`、スパンは std 内の呼び出し地点）に最大 8 件、最後に `Note`（`'{last}' reaches the host through '{builtin}'`、スパンは源の参照）。`ffi` は `DeclaredHere`（`extern function declared here`、スパンは宣言の名前） |
| `help` | `grant "{cap}" to package '{name}' in Tsuzuri.toml`（ルートは `add "{cap}" to allow in [capabilities]`） |
| `fixes` | なし（権限は人が決める） |
| 阻んだもの | 順に、ルートの `allow`、そのパッケージへの入辺（親の名前の昇順）のうち、`{cap}` を含まない最初のもの |

例（`app` の `allow = ["fs", "stdio"]`、依存 `fscache` に `capabilities = ["fs"]`。付録 D.4 のプロジェクト。位置は `{root}` 以下の実ファイルの絶対パス）:

1. 依存が許可の外を使う。ルートの `allow` に `time` がない（想定出力）:
   ```text
   {root}/fscache/Cache.tz:3:57: error[E1036]: package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); the root [capabilities] allow does not list "time"
     3 | def stamp :: unit -> IO<Result<i64, Os.Error>> = \() -> Time.monotonic_ns ()
       |                                                         ^
   ```
   同じプロジェクトで `allow = ["fs", "stdio", "time"]` にすると、阻むのは `app` の項目（`capabilities = ["fs"]`）になり、メッセージは `package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); grant it with capabilities = ["time"] in the entry of 'fscache' in 'app'`（G20・G21 が両方を確かめる）。
2. ルートが `allow` の外を使う（`app` の `allow = ["stdio"]` にして `Fscache` を呼ばず、`File.read_text` を使った場合。想定出力）:
   ```text
   {root}/app/Main.tz:2:11: error[E1036]: the root package may not use capability 'fs' ('File.read_text'); add "fs" to allow in [capabilities]
     2 |     let! _t = File.read_text "notes.txt"
       |               ^
   ```
3. `ffi`（`allow = ["stdio"]` のルートが `extern` を呼ぶ。想定出力）:
   ```text
   {root}/ffi/Main.tz:3:34: error[E1036]: the root package may not use capability 'ffi' ('c_abs'); add "ffi" to allow in [capabilities]
     3 | def main :: unit -> i32 = \() -> c_abs (0 - 5)
       |                                  ^
   ```

JSON（`tsuzuri check --json` の 1 行。例 1。G22 前の既存 5 キー。想定出力）:

```text
{"severity":"error","code":"E1036","message":"package 'fscache' may not use capability 'time' ('Time.monotonic_ns'); the root [capabilities] allow does not list \"time\"","path":"/work/fscache/Cache.tz","span":{"start":139,"end":156,"line":3,"column":57,"end_line":3,"end_column":74}}
```

### C.2 `E1037`（`@trusted` の誤り。構文の段階で出す。severity は error）

| kind | 条件 | メッセージ | 主スパン |
| --- | --- | --- | --- |
| `trusted_wrong_target` | `@trusted` の次の宣言が `extern` でない（`extern` 以外のすべての宣言） | `'@trusted' applies to an extern declaration; remove it or declare the function with 'extern'` | `@trusted` と性質の全体 |
| `trusted_unknown_property` | 性質が `pure`・`no_alloc`・`no_copy`・`no_trap`・`terminates` のどれでもない | `unknown '@trusted' property '{name}'; use pure, no_alloc, no_copy, no_trap, or terminates` | その性質 |
| `trusted_duplicate_property` | 同じ性質が 2 回 | `'@trusted' lists '{name}' twice; remove one` | 2 回目 |
| `trusted_type_property` | `extern type` に性質がある | `'@trusted' on an extern type takes no properties; write '@trusted' alone` | 最初の性質 |
| `trusted_repeated` | 1 つの宣言に `@trusted` が 2 つ | `'@trusted' appears twice on one declaration; remove one` | 2 つ目の `@` |

`help`（G22 後）は各メッセージの `;` の後ろと同じ命令形の 1 文。`fixes` はなし。例（想定出力）:

```text
Main.tz:1:1: error[E1037]: '@trusted' applies to an extern declaration; remove it or declare the function with 'extern'
  1 | @trusted
    | ^^^^^^^^
Main.tz:1:10: error[E1037]: unknown '@trusted' property 'fast'; use pure, no_alloc, no_copy, no_trap, or terminates
  1 | @trusted fast
    |          ^^^^
Main.tz:1:10: error[E1037]: '@trusted' on an extern type takes no properties; write '@trusted' alone
  1 | @trusted pure
    |          ^^^^
```

HEAD の実機: `@trusted` は `error[E0002]: expected an expression; '@checked' is the only attribute of an expression`（付録 H）。

### C.3 `W1010`（kind `untrusted_extern`。severity は warning。既定は無効）

| 項目 | 内容 |
| --- | --- |
| 条件 | `@trusted` のない `extern` の関数が、`extern` でない関数から 1 回以上参照されている（コンパイルしたプログラムで） |
| 主スパン | 宣言の名前（`extern "abs" def c_abs` なら `c_abs`）。宣言のあるソース |
| メッセージ | `extern function '{name}' is used but not marked '@trusted'; review its contract and add '@trusted' above its declaration` |
| `related` | `Note`: `first used here`（参照のうち (source, start) が最小のもの） |
| `help` | `add '@trusted' above the extern declaration once its contract is checked` |
| `fixes` | なし |
| 有効にする | `--warn untrusted-extern`（`check`・`build`・`run`・`test`。ルートパッケージで宣言した `extern` だけ）、`tsuzuri audit`（全パッケージの `extern`。常に） |
| 無効にする | `@trusted`（性質があってもなくても） |

例（想定出力。付録 D.5 のプロジェクト）:

```text
Main.tz:1:18: warning[W1010]: extern function 'c_abs' is used but not marked '@trusted'; review its contract and add '@trusted' above its declaration
  1 | extern "abs" def c_abs :: i32 -> i32
    |                  ^^^^^
```

`@trusted pure` を足すと警告は消え、`tsuzuri audit` の `extern` の行が `"trusted":true,"properties":["pure"]` になる（付録 D.5）。`--deny-warnings` と合わせると、`check` は警告で終了コード 1（既存の規則）。

## 付録 D: `tsuzuri audit` の出力例（想定出力）

書式は本文の仕様 4。`examples/` のプロジェクトにマニフェストはないので、パッケージ名は入力のディレクトリ名、`version` は `null`。パスはパッケージのルートからの相対で `/` 区切り。標準出力に報告、標準エラーに診断。

### D.1 `tsuzuri audit examples/io`（ポリシーなし。終了コード 0）

`examples/io/Main.tz` は `IO.write`（2:9）・`IO.read_line`（3:17）・`IO.write_line`（5:9）を使う（実ファイルの列を数えた値）。

```text
audit: io
policy: none
packages: 1

package io (root)
  allowed: all
  used: stdio
    stdio: 3 sites
      Main.tz:2:9  IO.write
      Main.tz:3:17  IO.read_line
      Main.tz:5:9  IO.write_line
  denied: none
  externs: none

summary: used=stdio denied=0 untrusted_externs=0
```

`tsuzuri audit examples/io --json`:

```text
{"type":"audit","schema":"tz-audit/1","root":"io","policy":"none","packages":1}
{"type":"package","name":"io","version":null,"root":true,"allowed":["env","ffi","fs","gpu","net","process","random","stdio","time"],"granted":[],"used":[{"capability":"stdio","count":3,"sites":[{"path":"Main.tz","line":2,"column":9,"callee":"IO.write","function":"Main.main","scope":"program"},{"path":"Main.tz","line":3,"column":17,"callee":"IO.read_line","function":"Main.main","scope":"program"},{"path":"Main.tz","line":5,"column":9,"callee":"IO.write_line","function":"Main.main","scope":"program"}]}],"denied":[],"unused":[]}
{"type":"summary","packages":1,"used":["stdio"],"denied":0,"untrusted_externs":0,"exit":0}
```

### D.2 `tsuzuri audit examples/hello`（`IO.write_line` が 11:9 に 1 件）

```text
audit: hello
policy: none
packages: 1

package hello (root)
  allowed: all
  used: stdio
    stdio: 1 site
      Main.tz:11:9  IO.write_line
  denied: none
  externs: none

summary: used=stdio denied=0 untrusted_externs=0
```

### D.3 `tsuzuri audit examples/tasks`（`Task.run`・`Task.parallel` は CPU の資源で権限ではない）

```text
audit: tasks
policy: none
packages: 1

package tasks (root)
  allowed: all
  used: none
  denied: none
  externs: none

summary: used=none denied=0 untrusted_externs=0
```

```text
{"type":"audit","schema":"tz-audit/1","root":"tasks","policy":"none","packages":1}
{"type":"package","name":"tasks","version":null,"root":true,"allowed":["env","ffi","fs","gpu","net","process","random","stdio","time"],"granted":[],"used":[],"denied":[],"unused":[]}
{"type":"summary","packages":1,"used":[],"denied":0,"untrusted_externs":0,"exit":0}
```

### D.4 ルートと依存のあるプロジェクト（ポリシーあり。執筆時に、権限のキーを除いたマニフェストで `tsuzuri check` が通ることを実機で確かめた内容。終了コード 1）

`app/Tsuzuri.toml`:

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
fscache = { path = "../fscache", capabilities = ["fs"] }

[capabilities]
allow = ["fs", "stdio"]
```

`app/Main.tz`（実機で `tsuzuri check` が通る内容。`capabilities` のキーを除いたマニフェストで確認した）:

```tsuzuri
def main :: unit -> i32 = \() ->
    let! _text = Fscache::Cache.load "notes.txt"
    do! IO.write_line "loaded"
    0
```

`fscache/Tsuzuri.toml` は `[package] name = "fscache" version = "0.2.0"`、`fscache/Cache.tz`:

```tsuzuri
def load :: string -> IO<Result<string, Os.Error>> = \path -> File.read_text path

def stamp :: unit -> IO<Result<i64, Os.Error>> = \() -> Time.monotonic_ns ()
```

`tsuzuri audit app` の標準出力:

```text
audit: app 0.1.0
policy: enforced
packages: 2

package app 0.1.0 (root)
  allowed: fs stdio
  used: stdio
    stdio: 1 site
      Main.tz:3:9  IO.write_line
  denied: none
  unused: fs
  externs: none

package fscache 0.2.0 (dependency)
  allowed: fs
  granted: app: fs
  used: fs time
    fs: 1 site
      Cache.tz:1:63  File.read_text
    time: 1 site
      Cache.tz:3:57  Time.monotonic_ns
  denied: time
  unused: none
  externs: none

summary: used=fs,stdio,time denied=1 untrusted_externs=0
```

標準エラー（付録 C.1 の例 1 と同じ `E1036`）。`--json` の標準出力:

```text
{"type":"audit","schema":"tz-audit/1","root":"app","policy":"enforced","packages":2}
{"type":"package","name":"app","version":"0.1.0","root":true,"allowed":["fs","stdio"],"granted":[],"used":[{"capability":"stdio","count":1,"sites":[{"path":"Main.tz","line":3,"column":9,"callee":"IO.write_line","function":"Main.main","scope":"program"}]}],"denied":[],"unused":["fs"]}
{"type":"package","name":"fscache","version":"0.2.0","root":false,"allowed":["fs"],"granted":[{"by":"app","capabilities":["fs"]}],"used":[{"capability":"fs","count":1,"sites":[{"path":"Cache.tz","line":1,"column":63,"callee":"File.read_text","function":"Fscache.Cache.load","scope":"program"}]},{"capability":"time","count":1,"sites":[{"path":"Cache.tz","line":3,"column":57,"callee":"Time.monotonic_ns","function":"Fscache.Cache.stamp","scope":"program"}]}],"denied":["time"],"unused":[]}
{"type":"summary","packages":2,"used":["fs","stdio","time"],"denied":1,"untrusted_externs":0,"exit":1}
```

（`function` は宣言した関数の `CheckedFunction::qualified_name()`（`{module}.{name}`）。コンパイラの内部では名前空間もモジュール名も `.` でつなぐので、依存のモジュールは `Fscache.Cache.load` の綴りで出る。実装時に `qualified_name()` の実際の綴りを確かめ、違えばテストの期待値をその綴りに合わせる。）

### D.5 `extern` のあるプロジェクト（マニフェストなし。執筆時に `tsuzuri check`・`run` が通ることを実機で確かめた。終了コード 0）

`Main.tz`:

```tsuzuri
extern "abs" def c_abs :: i32 -> i32

def main :: unit -> i32 = \() -> c_abs (0 - 5)
```

`tsuzuri audit ffi`（標準出力。標準エラーに付録 C.3 の `W1010` が 1 件）:

```text
audit: ffi
policy: none
packages: 1

package ffi (root)
  allowed: all
  used: ffi
    ffi: 1 site
      Main.tz:3:34  c_abs
  denied: none
  externs: 1 (0 trusted)
    c_abs -> "abs"  Main.tz:1:18  untrusted

summary: used=ffi denied=0 untrusted_externs=1
```

```text
{"type":"audit","schema":"tz-audit/1","root":"ffi","policy":"none","packages":1}
{"type":"package","name":"ffi","version":null,"root":true,"allowed":["env","ffi","fs","gpu","net","process","random","stdio","time"],"granted":[],"used":[{"capability":"ffi","count":1,"sites":[{"path":"Main.tz","line":3,"column":34,"callee":"c_abs","function":"Main.main","scope":"program"}]}],"denied":[],"unused":[]}
{"type":"extern","package":"ffi","name":"c_abs","symbol":"abs","path":"Main.tz","line":1,"column":18,"trusted":false,"properties":[],"used":1}
{"type":"summary","packages":1,"used":["ffi"],"denied":0,"untrusted_externs":1,"exit":0}
```

`@trusted pure` を 1 行目の前に足すと、`extern` の行は `"trusted":true,"properties":["pure"]`、text は `c_abs -> "abs"  Main.tz:2:18  trusted pure`（宣言が 1 行下がる）、`untrusted_externs` は 0、`W1010` は出ない。`tsuzuri audit ffi --deny-warnings`（`@trusted` なし）は終了コード 1。

## 付録 E: Rust の骨格（名前と型はこのとおり）

### E.1 `src/capabilities.rs`（新規。`src/lib.rs` に `pub mod capabilities;`）

```rust
use std::collections::BTreeMap;
use crate::check::{CheckedModule, ModuleOrigin};
use crate::diagnostic::{Diagnostic, Span};
use crate::package::PackageId;

pub const MAX_FUNCTIONS: usize = 65_536;
pub const MAX_NODES: usize = 16_777_216;
pub const MAX_SITES_SHOWN: usize = 100;          // audit --json の sites の上限（超えた件数は "truncated"）
pub const MAX_SITES_TEXT: usize = 5;             // audit の text の 1 権限あたりの行数

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability { Env, Ffi, Fs, Gpu, Net, Process, Random, Stdio, Time }   // 名前の辞書順 = ビットの順

impl Capability {
    pub const ALL: [Capability; 9] = [/* 上の順 */];
    pub fn name(self) -> &'static str;                       // "env" "ffi" "fs" "gpu" "net" "process" "random" "stdio" "time"
    pub fn from_name(name: &str) -> Option<Self>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilitySet(u16);                                // ビット i が Capability::ALL[i]
impl CapabilitySet {
    pub const EMPTY: Self; pub const ALL: Self;
    pub fn single(capability: Capability) -> Self;
    pub fn contains(self, capability: Capability) -> bool;
    pub fn insert(&mut self, capability: Capability);
    pub fn union(self, other: Self) -> Self;
    pub fn intersection(self, other: Self) -> Self;
    pub fn difference(self, other: Self) -> Self;
    pub fn is_empty(self) -> bool;
    pub fn iter(self) -> impl Iterator<Item = Capability>;    // ALL の順
}

/// A package of the graph, for reports. `owner` is `None` for the root package, with or without a manifest,
/// as in `Project::dependency_package`.
#[derive(Clone, Debug)]
pub struct PackageInfo { pub owner: Option<PackageId>, pub name: String, pub version: Option<String>, pub root: bool }

/// `parent` (`None`: the root) lets `child` use `granted` (the dependency entry's `capabilities`, empty when absent).
#[derive(Clone, Debug)]
pub struct Edge { pub parent: Option<PackageId>, pub child: PackageId, pub granted: CapabilitySet }

#[derive(Clone, Debug)]
pub struct Policy {
    pub enforced: bool,                                       // ルートに [capabilities] か、ルートの依存に capabilities がある
    pub packages: Vec<PackageInfo>,                           // ルートが先頭、あとは名前の昇順
    pub edges: Vec<Edge>,                                     // (child 名, parent 名) の昇順
    pub effective: BTreeMap<Option<PackageId>, CapabilitySet>, // ルートは None（Project::dependency_package と同じ）。enforced でなければ全部 ALL
}

impl Policy {
    /// 付録 E.3 のアルゴリズム。`packages` は `driver::load_packages` の結果、`root` はルートのディレクトリ。
    pub(crate) fn build(root: &std::path::Path, packages: &[crate::driver::LoadedPackage]) -> Option<Policy>;
    /// The first limit that leaves `capability` out of `package`: the root's allow, or an incoming edge.
    pub fn blocker(&self, package: &Option<PackageId>, capability: Capability) -> Option<Blocker>;
}
pub enum Blocker { RootAllow, Edge { parent: String } }

/// A source file, by the index that `Span::source` uses (the order of `Project::sources`).
#[derive(Clone, Debug)]
pub struct SourceInfo { pub name: String /* モジュール名（ファイルの語幹。std は `File`・`Dir`・`Env` のように） */, pub std: bool, pub package: Option<PackageId>, pub path: String /* パッケージのルートからの相対、`/` 区切り */ }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope { Program, Test }

#[derive(Clone, Debug)]
pub struct Link { pub from: String, pub to: String, pub span: Span }

#[derive(Clone, Debug)]
pub struct Site {
    pub capability: Capability, pub span: Span,
    pub callee: String,                  // 参照した std の関数 `File.read_text`、extern の名前
    pub function: String,                // 使った側の宣言の関数（生成関数は origin.parent をたどる）の qualified_name
    pub scope: Scope,
    pub via: Vec<Link>,                  // callee から源までの std の連鎖（最大 8。ffi は空）
    pub source_span: Option<Span>,       // 源（組み込み関数の参照、extern の宣言）の位置
}

#[derive(Clone, Debug)]
pub struct ExternUse {
    pub function: usize, pub name: String, pub symbol: String,
    pub package: Option<PackageId>, pub span: Span /* 宣言の名前 */,
    pub trusted: Option<u8> /* None: 印なし。Some(bits): @trusted と性質 */,
    pub uses: Vec<Span>,                 // extern でない関数からの参照。(source, start) の昇順。重複なし
}

#[derive(Clone, Debug, Default)]
pub struct Analysis { pub sites: Vec<(Option<PackageId>, Site)>, pub externs: Vec<ExternUse> }
// sites は (パッケージ、source、start、capability) の昇順。同じ (source, start, end, capability) は 1 件。

pub fn analyze(module: &CheckedModule, sources: &[SourceInfo]) -> Result<Analysis, Diagnostic>;   // E1017
pub fn denied(analysis: &Analysis, policy: &Policy, sources: &[SourceInfo]) -> Vec<Diagnostic>;     // E1036（Scope::Program だけ）
pub fn untrusted_externs(analysis: &Analysis, root_only: bool) -> Vec<Diagnostic>;                  // W1010
```

### E.2 `analyze` のアルゴリズム

1. `module.functions.len() > MAX_FUNCTIONS` なら `E1017`（`capability analysis supports at most 65536 functions; split the program`、スパンなし）。
2. 各関数 `f`（添字 `i`）の**家族の根** `root(i)` = `origin.parent` を、`None`・自分自身・すでに通った関数のどれかに着くまでたどった最後の関数（最大 `module.functions.len()` 歩）。ジェネリックなインスタンス（名前が `….$mono.N`）は `polymorph::instantiate` の `origin.generated(instance)` で**自分自身**を `parent` に持つので、根は自分になる（`module` は元のモジュールのまま、`span` は定義のもの）。クロージャの持ち上げと組み込み関数の包み（`$lambda`・`$builtin`）の `parent` は、それを含む関数の添字。`SourceInfo` は `functions[root(i)].span.source` で引く（`None` なら `i` を黙って飛ばす）。`std(i)` = `functions[i].origin.module == ModuleOrigin::Std`（`generated()` は `module` を引き継ぐ）。`scope(i)` = 根に `origin.test` か `origin.bench` があれば `Test`、なければ `Program`。
3. 各関数の本体を、明示のスタックで `TypedExpr::children()` をたどって走査する（ノード数を数え、`MAX_NODES` を超えたら `E1017`）。見つけるもの: `Function(User(id))`・`GenericFunction(id, _)`・`Closure(id, _)` → 辺 `(i → id, span)`。`Function(Builtin(b))` → A.2 の表を引き、権限があれば `direct[i]` に `(権限, span)`（`Os.__read`・`Os.__write` は `SourceInfo.name` が `File`・`Dir`・`Env` のものを引く。ほかなら無視して `os_read_outside_the_known_files` の内部の計数器を 1 足す。テストが 0 を確かめる）。本体が `HostCall` の関数は `extern` として `ExternUse` を作る。
4. std の関数の権限 `caps[i]`: `direct[i]` の権限 ∪ A.3 の明示（`qualified_name()` が `Gpu.request`・`Gpu.init`・`Gpu.map`・`Gpu.init_relaxed`・`Gpu.map_relaxed`）。辺 `i → g` で `std(g)` なら `caps[g]` を足す。**不動点**: 変化がなくなるまで、添字の昇順に全辺を回す（最大 9 回の増加 × 関数の数。収束する）。各 `(i, 権限)` について、その権限を最初に与えた辺（添字の最小の `g`）を `parent[i][権限]` に覚える（連鎖と `related` に使う）。
5. std でない関数 `i`（`root` のソースの `package` に属す）の使用: 辺 `i → g` のそれぞれについて、`g` が `extern` なら `ffi` の地点（`callee` = `g` の名前、`source_span` = `g` の宣言）、`std(g)` なら `caps[g]` の各権限の地点（`callee` = `g` の表示名。`$` で始まるモジュールの生成関数は `origin.parent` の根の `qualified_name()`、名前に `.$mono.` を含む複本は `.$mono.` 以降を除いた名前、`via` は `parent[g][権限]` をたどった `g → … → 源を直接参照する関数` の連鎖、`source_span` = その源の参照）。`g` が std でない `extern` でもない関数なら使用ではない。`i` の本体が直接 `Function(Builtin(b))` を持つ場合も同じ表で地点にする（利用者が直接書ける私的な組み込み関数は `Bench.now()` だけで、`bench` の中に限る（`E1018`）ので、地点は `scope == Test`。生成関数は家族の根に属す）。
6. `extern` の `uses` は、`extern` でない関数から `extern` への辺の `span`。`trusted` は `origin.trusted`。
7. 結果を整列して返す。計算量: O(式の総数 + 辺 × 9)。資源上限は 1 と 3。

### E.3 実効の権限（`Policy::build`）

```text
入力: root（ディレクトリ）、packages（LoadedPackage の列。manifest と dependency_roots を持つ）
1. ルートのパッケージ R = packages のうち id.root == root のもの。なければ None を返す（マニフェストのないプロジェクト）。
2. enforced = R.manifest.capabilities.is_some() || R.manifest.dependencies の値のどれかに capabilities.is_some()
3. 辺の集合: 各パッケージ P について、P.dependency_roots の (名前, 子のルート) ごとに
      granted = P.manifest.dependencies[名前].capabilities.unwrap_or(EMPTY)
      Edge { parent: owner(P), child: 子の id, granted }          owner(R) = None、ほかは Some(id)（Project::dependency_package と同じ）
4. effective[R] = enforced ? R.manifest.capabilities.map(|c| c.allow).unwrap_or(ALL) : ALL
5. R から幅優先（親が先に確定する順。依存の循環は load_packages が E1011 で拒否済みで、DAG）:
      各辺 (P → C): candidate = effective[P] ∩ (enforced ? granted : ALL)
      effective[C] = 初めて見たなら candidate、2 回目以降は effective[C] ∩ candidate
   親がすべて確定してから子を処理する（入次数を数える Kahn 法）。
6. enforced でないとき、effective はすべて ALL。非ルートの [capabilities] は無視する。
7. packages: ルートが先頭、あとは名前の昇順。edges: (child 名, parent 名) の昇順。
```

計算量 O(パッケージ + 辺)。上限は `load_packages` の 1,024 パッケージ・深さ 128。

### E.4 そのほかの変更の骨格

```rust
// src/syntax.rs（CpuAttribute の隣）
pub const TRUSTED_PROPERTIES: [&str; 5] = ["pure", "no_alloc", "no_copy", "no_trap", "terminates"];   // ビット 0..4
#[derive(Clone, Debug)]
pub struct TrustedAttribute { pub target: Ident, pub properties: u8, pub span: Span /* '@trusted' から最後の性質まで */ }
// Program に `pub trusted: Vec<TrustedAttribute>`（cpu_attributes の次）

// src/parser.rs: fn trusted_attribute(&mut self) -> Result<Option<(u8, Span)>, Diagnostic>   // cpu_attribute の隣。
//   `@` と `Ident("trusted")` で始まれば消費し、同じ行の Ident を性質として読む（語彙外・重複は E1037）。
// program_all: `let cpu = self.cpu_attribute()?;` の直後に `let trusted = self.trusted_attribute()?;`。
//   `trusted` があって、visibility のあとが `extern` でなければ E1037 trusted_wrong_target。`extern type` で性質があれば trusted_type_property。
//   `ExternDeclaration::Function(signature)` と `Type(declaration)` の push のとき `program.trusted.push(TrustedAttribute { target: 名前, .. })`。
//   同じ宣言に `@trusted` が 2 つなら trusted_repeated（trusted_attribute のあとにもう一度 `@trusted` が続く）。

// src/check.rs: FunctionOrigin に `pub trusted: Option<u8>`（cpu の隣。source() は None、generated() も None）。
//   `cpu_levels` の隣で `trusted_by_id: BTreeMap<usize, u8>` を作り（`program.trusted` の target を function_declarations の (module, 名前) で引く）、
//   ユーザー関数の構築（`cpu: cpu_levels...` の隣）で `trusted: trusted_by_id.get(&id).copied()`。extern でない関数への印は存在しない（構文で拒否済み）。
```

driver・main の変更は本文の設計の表と手順にある。

## 付録 F: 既存のテスト・文書への影響

| Phase | 対象 | 変わること |
| --- | --- | --- |
| 1 | `src/syntax_codec.rs` の `pins_encoding_format` の `GOLDEN` | `Program` に `trusted` が足されるので変わる。テストの失敗の出力の `hex` をコピーする。`src/frontend_cache.rs` の `FRONTEND_FORMAT` を 4 から 5 に上げる |
| 1 | `src/main.rs` の `parses_warn_implicit_copy` と、`grep -n "supported: implicit-copy" src/main.rs` が示す 3 か所 | `supported: implicit-copy, untrusted-extern`（A22 が先に入っていれば `canonical, implicit-copy, untrusted-extern`） |
| 1 | `tests/fixtures/bindgen/*/expected.tz` の 5 つ（`basic`・`extended`・`include`・`names`・`skipped`）と `tests/bindgen.mjs`・`_tsuzuri/language-reference/compiler/native-interop.md`（生成物のヘッダーの例） | `// target:` の次に 1 行（本文の仕様 1）。`tests/bindgen.mjs` は `lines[1]`〜`lines[3]` だけを置換するので、期待ファイルに 1 行足すだけ |
| 2 | `src/package.rs` の単体テスト（セクション順のメッセージ。`grep -n "followed by optional" src/package.rs`）と `tests/packages.rs`（同じメッセージの 1 件） | `…[registry] and [capabilities], each once` |
| 2 | `src/package.rs` の `parses_the_native_opt_in_of_a_dependency` の `only the path and native keys…` | `only the path, native, and capabilities keys are supported in a dependency` |
| 2 | `Manifest`・`Dependency` を組み立てる箇所 | `src/package.rs` の 2 か所（`Manifest {` と `Dependency {`）。`LoadedPackage {` は `src/driver.rs` の 1 か所。`Project` の組み立ては `load_from_root` と `single_main` の 2 か所 |
| 3 | `README.md` の CLI の表・`docs/architecture.md` の表・`docs/stability.md` | 行を足す |
| 1〜3 | 既存のテストの期待値 | 上のほかは**変わらない**。マニフェストに `[capabilities]` も `capabilities` もないプロジェクトは `Policy.enforced == false` で、`check`・`build`・`run` の出力と終了コードは 1 byte も変わらない |

## 付録 G: テストの一覧（名前・入力・期待値）

| # | ファイル::関数 | 入力 | 期待値 |
| --- | --- | --- | --- |
| G1 | `src/capabilities.rs::capability_names_are_sorted_and_round_trip` | `Capability::ALL` | 名前の辞書順。`from_name(name())` が一致。`from_name("Fs")` は `None` |
| G2 | `…::capability_set_operations` | 和・積・差・`iter` | `ALL` の順、`EMPTY.is_empty()`、`ALL` の要素数 9 |
| G3 | `…::every_private_builtin_is_classified` | `Builtin::ALL` の名前のうち `__` を含むか `Bench.` で始まるもの | 46 個がすべて A.2 の表（権限あり 33・なし 13）にある。表にない名前があれば落ちる |
| G4 | `…::effective_grants_intersect_along_every_path` | root→A（`fs,net`）、root→B（`fs`）、A→C（`fs,net`）、B→C（`net`）、`allow = ["fs","net"]` | `effective[C] == {}`（`fs` は B→C で、`net` は B の許可で落ちる） |
| G5 | `…::policy_is_off_without_any_capabilities_key` | 既存形式のマニフェスト | `enforced == false`、すべて `ALL` |
| G6 | `src/package.rs::parses_the_capabilities_section` | B.3 の例 | `allow` と依存ごとの集合が付録のとおり。`[capabilities]` なしは `None` |
| G7 | `…::rejects_malformed_capabilities` | B.2 の各行 | 各メッセージ（`E0002`、`error.span.source == Some(5)`） |
| G8 | `…::parses_dependency_capabilities_for_every_source_kind` | path・git・registry の各形 | 受理。`native = true, capabilities = [...]` の順 |
| G9 | `tests/trusted.rs::trusted_marks_extern_functions_and_types` | `@trusted` ／ `@trusted pure no_trap` を `extern "abs" def` と `extern type` に | 受理。`FunctionOrigin.trusted` が `Some(0)`／`Some(0b01001)`（`pure` = ビット 0、`no_trap` = ビット 3） |
| G10 | `…::trusted_rejects_wrong_targets_unknown_and_duplicate_properties` | C.2 の 5 つの kind | `E1037`。主スパンは表のとおり |
| G11 | `…::trusted_survives_the_frontend_cache_round_trip` | `TSUZURI_CACHE_DIR` を一時ディレクトリにして、同じプロジェクトを `Project::analyze_cached(true, AnalysisKind::Program)` で 2 回 | 2 回目も `FunctionOrigin.trusted` が残り、診断が同じ |
| G12 | `…::formatter_keeps_trusted_attributes` | `tsuzuri fmt --check` | 整形済みなら終了コード 0、`@trusted` が消えない |
| G13 | `…::untrusted_extern_warning_is_off_by_default_and_on_with_the_flag` | `check ffi`／`check ffi --warn untrusted-extern`／`@trusted` つき | 警告 0 件／`W1010` 1 件（付録 C.3 のメッセージ）／0 件 |
| G14 | `tests/bindgen.mjs`（JavaScript。既存の suite に足す） | `tsuzuri bindgen` の出力（`lines[4]`） | 注意書き（本文の仕様 1 の文）が `lines[4]`、出力に `@trusted` を含まない |
| G15 | `tests/capabilities.rs::std_probe_matches_the_expected_sites` | 付録 A.7 のプローブ | 14 地点（表のとおり）、`Net.parse_address`・`Path.join` は 0 |
| G16 | `…::std_world_primitives_are_referenced_only_by_the_known_files` | std の全ソース | 付録 A.6 の表と一致（合計 61） |
| G17 | `…::std_table_file_lists_the_85_functions` | `tests/fixtures/capabilities/std-table.txt` | 85 行、A.5 と一致、各行 `<モジュール>.<関数>: <権限>` |
| G18 | `…::policy_off_costs_nothing_and_changes_nothing` | マニフェストのあるプロジェクトを `check` | 出力・終了コードが HEAD と同じ。`capabilities::analyze` は呼ばれない（`Project::check_capabilities` が空を返す） |
| G19 | `…::root_allow_denies_the_root_itself` | C.1 の例 2 | `E1036`（ルートのメッセージ）、終了コード 1 |
| G20 | `…::dependency_without_a_grant_gets_the_empty_set` | `capabilities` のない依存が `IO.write_line` を使う | `E1036` |
| G21 | `…::dependency_grant_is_cut_by_the_root_allow` | 依存に `["fs"]`、ルートの `allow = ["stdio"]` | `E1036`（`the root [capabilities] allow does not list "fs"`） |
| G22 | `…::denial_names_the_std_chain` | `Gen.for_all`（`stdio`）を依存が使う | `E1036` の `related` に `Gen.for_all` → `Debug.print` の連鎖（G22 後） |
| G23 | `…::generic_std_instances_keep_their_capability` | `HashMap.randomized` を `i64`・`string` の 2 通りで使う | どちらも `random` の地点 |
| G24 | `…::tests_and_benches_are_exempt_from_the_check` | `allow = []` のルートに `test "t" = Debug.print (ref "x")`。`Project::load_for_tests` と `analyze_cached(false, AnalysisKind::Tests)` | `E1036` なし（`check_capabilities` が空）。`analyze` の地点は `scope == Test` |
| G25 | `…::extern_needs_ffi_and_trusted_does_not_change_it` | `@trusted pure` の extern を呼ぶ | `ffi` の地点が出る（印は権限を変えない） |
| G26 | `…::limits_report_e1017` | 関数 65,537 個（単体テストは `MAX_FUNCTIONS` の差し替え不可なので、`analyze_with_limits(module, sources, 2, 100)` を `pub(crate)` で足す） | `E1017` |
| G27 | `tests/audit.rs::audit_text_matches_the_examples` | 付録 D.1〜D.5 の各プロジェクト | 標準出力が付録と一致 |
| G28 | `…::audit_json_matches_the_examples` | 同上 `--json` | 行ごとに一致。キーの順序が固定 |
| G29 | `…::audit_exit_codes` | D.4（1）、D.1（0）、D.5（0）、D.5 `--deny-warnings`（1）、型エラーのあるプロジェクト（1）、不正な引数（2） | 終了コード |
| G30 | `…::audit_is_deterministic_and_path_free` | 同じ入力を 2 回、別の置き場にコピーして 1 回 | 標準出力が 3 つとも同じ（絶対パスを含まない） |
| G31 | `…::audit_reports_untrusted_externs_of_dependencies` | 依存が `extern` を宣言して使う | `extern` の行 `package` が依存の名前、標準エラーに `W1010` |
| G32 | `src/main.rs::parses_audit_arguments` | `audit`・`audit PATH`・`--json`・`--deny-warnings`・不正 | 受理と拒否（`E2000`） |

## 付録 H: 実機で確かめたこと（HEAD。`target/release/tsuzuri`。入力は執筆時の作業ディレクトリの小さなプロジェクトで、内容は本文と付録に全文がある）

| コマンド | 出力（要約） |
| --- | --- |
| `tsuzuri check cur1`（`[capabilities] allow = ["fs"]` を足したマニフェスト） | `Tsuzuri.toml:1:1: error[E0002]: expected [package] followed by optional [dependencies], [wasm], [native] and [registry], each once`、終了 1 |
| `tsuzuri check cur2`（依存に `capabilities = ["fs"]`） | `error[E0002]: only the path and native keys are supported in a dependency`、終了 1 |
| `tsuzuri check tr1`（`@trusted` の次に `extern "abs" def …`） | `Main.tz:1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression`、終了 1 |
| `tsuzuri audit ffi` | `<command line>:1:1: error[E2000]: pass one .tz, .tt, or .tc file or project directory; modules are loaded recursively`、終了 2（`audit` は未知のサブコマンドで、入力のパスとして扱われる） |
| `tsuzuri check ffi --warn untrusted-extern` | `error[E2000]: unknown warning 'untrusted-extern' for --warn; supported: implicit-copy`、終了 2 |
| `tsuzuri check probe`（付録 A.7） | 診断なし、終了 0 |
| `tsuzuri check app`（付録 D.4 の内容から `capabilities` のキーを除いたもの） | 診断なし、終了 0 |
| `tsuzuri run ffi` | `error[E2005]: program exited with code 5`（`abs(-5)` が `main` の戻り値） |
| `ls examples` | 13 項目。`Tsuzuri.toml` を持つものも `extern` を使うものもない |
| `grep -c '^def ' std/*.tz std/*.tc \| awk -F: '{s+=$2} END {print s}'` | 559（`'^private def '` は 390）。47 ファイル |
| `awk '/pub fn name\(self\)/,/^    }$/' src/check.rs \| grep -o '=> "[A-Za-z0-9_.]*"' \| wc -l` | 244（そのうち `__` を含むか `Bench.` で始まるものは 46） |
