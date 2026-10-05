# E09: ネットワーク API

| 項目 | 内容 |
| --- | --- |
| ID | E09 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | E08, B08 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（std モジュール名 `Net` の確定と予約。GUIDE D-30 の仮割り当て）, D11（wasm32 で Net を使う将来のホスト opt-in。Phase 1 の E2000 による拒否は承認を待たない） |
| 改善する劣位 | C#/F# 比: ネットワーク API がない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | E08（done 後）の全体: `std/Os.tz`（`Error`・`error_of_status`）、`std/File.tz` の IO の包み方と `File.Handle`（opaque な所有 handle、`instance Drop`、操作が handle を受け取って返す形）、`src/runtime/os.c`、`src/driver.rs` の `os_runtime`、`tests/os_api.rs`・`tests/os.mjs`。HEAD にある部品: `src/check.rs` の `Builtin::IOReadLine`／`Builtin::IOWrite`（`name`、型 scheme）、`src/polymorph.rs` の `Checker::builtin`（std 専用 builtin の E1022）、`src/llvm_io.rs` の `io_builtin`、`src/llvm_imports.rs` の `host_result_slot`・`read_host_result`、`src/runtime/io.c`（`TZ_IO_API`、EINTR の再試行）、`src/driver.rs` の `io_runtime`、`tests/io.mjs` の `execute` と確保を数える harness（`live == 0`） |
| 主な影響ファイル | `std/Net.tz`（新規）, `src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES`・`opaque_record`・`reserves_the_d07_table`）, `src/check.rs`（`Builtin`・`Type::is_noncopy_record`）, `src/polymorph.rs`（`Checker::builtin`）, `src/llvm.rs`（builtin の振り分け）, `src/llvm_io.rs`（`net_builtin`（新規））, `src/runtime/net.c`（新規）, `src/driver.rs`, `tests/net_api.rs`（新規）, `tests/net.mjs`（新規）, `tests/fixtures/net/`（新規）, `README.md`, `docs/language.md`, `docs/architecture.md`, `_docs/library-reference/net.md`（新規）, `_docs/library-reference/README.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

TCP／UDP ソケットを標準モジュール `Net` で扱えるようにし、将来の HTTP クライアント（E10 の公式パッケージ、D13）の土台を用意する。
Rust の `std::net`、.NET の `TcpClient`／`TcpListener`／`UdpClient` の同期 API に相当する。

| 段 | 内容 | 着手の条件 |
| --- | --- | --- |
| Phase 1 | native（macOS／Linux）の blocking TCP client・server、UDP、アドレスの解析と表示、名前解決。すべて `IO<'a>` で、失敗は `Result<'a, Os.Error>`（E08 の型） | E08 段 A・段 B が done、D1 の承認 |
| Phase 2 | B08 Phase 3 の reactor と接続した非同期ソケット、Windows（Winsock、G10） | B08 と G10 が done、人間の指示 |
| Phase 3 | wasm32 のホスト opt-in（WASI preview2 sockets または E13 の glue） | D11 の承認 |

実装者は Phase 1 だけを実装する。Phase 2・3 は「仕様」の設計方針にとどめ、人間が求めた場合だけ着手する。TLS・HTTP はこのチケットで作らない。

## 着手条件と停止条件

### 着手条件

- E08 の段 A と段 B が done であること。確認: `grep -n "^| E08 \|^| B07 " _features/README.md` の状態欄、`ls std/Os.tz std/File.tz` が両方あること、
  `grep -n '"File.Handle"' src/stdlib.rs` が一致すること。段 B は B07 を要するので B07 も done になっている。
- B08 は Phase 1 の開始条件にしない（D2）。metadata の依存 B08 は Phase 2 の条件として読む。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンド、`node tests/io.mjs target/release/tsuzuri`、`node tests/os.mjs target/release/tsuzuri`（E08 が追加）が成功すること。
- loopback が使えること。確認: `node -e "const s=require('node:net').createServer().listen(0,'127.0.0.1',()=>{console.log(s.address().port);s.close()})"`
  が 1–65535 の数を出す。`::1` は任意で、使えなければ E2E は IPv6 のケースを skip と表示する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- E08 段 B の `File.Handle` が「前提とする他チケットのインターフェース」の形（opaque、操作が handle を受け取って返す、`instance Drop` で close）と違う。
  特に、Drop 型を IO の closure に捕捉すると E1005 になる（B07 D5）のに E08 がそれを解消する仕組みを持たない。E09 は E08 と同じ規則に従う（D4）ので、
  独自の回避策（handle の大域表、Copy な handle など）を作らない。
- builtin の型 scheme（`BuiltinType`）が 6 引数や `bool`・`i64u` の引数を表せず、`src/check.rs` の builtin の枠組みを変える必要がある。
- `src/runtime/net.c` が `src/runtime/os.c` と異なる feature macro（`_POSIX_C_SOURCE` など）を要し、連結した runtime が compile できない。
- 既定の wasm32 出力に import が増える、`unsafe` や新しい crate が要る、`Net` を使わないプログラムの IR が手順 1 のベースラインと変わる。
- 既存テストの期待値（`tests/io.mjs` の import、`tests/os.mjs`、`reserves_the_d07_table` の件数以外の std テスト）を変える必要がある。
- E2E が外部ネットワーク、固定ポート、root 権限、時間の閾値（「50 ms 以内」など）なしでは書けない。
- macOS と Linux で同じケースの `Net.error_kind` が異なり、D3 の表で吸収できない。テストを「どちらでもよい」に緩めて通さない。
- stack-depth の 3 テスト（GUIDE §11.1 の一覧）か `honors_the_exact_specialization_limit` が失敗する。

## 現状（HEAD `f8dc655` で確認）

- `Net` は存在しない。`src/runtime/` にソケットを扱う C はない（`socket`・`getaddrinfo`・`SIGPIPE` の grep が空）。E08 の `std/Os.tz` などもまだない
  （`std/` は `Array.tz` … `Vec.tz` と `IO.tc`・`Option.tc`・`Result.tc`）。E08・B07・B08 は todo。
- `src/stdlib.rs` の `RESERVED_MODULES` は 20 名で `Net` を含まない。`reserves_the_d07_table` が件数 20 を検査する（E08 の段 A で 27 になる予定）。
  `opaque_record` は `"Map.Map"`・`"Seq.Seq"`・`"Gpu.Device"`・`"IO.IO"` などを列挙する。`Type::is_noncopy_record`（`src/check.rs`）は std の
  `"Seq.Seq" | "Gpu.Device" | "Gpu.Buffer"` を非 Copy にする。
- `std/IO.tc`: `record IO<'a> { work: unit -> 'a }`、`def pure :: Capture<'a> => 'a -> IO<'a>`、`def map :: ('a -> 'b) -> IO<'a> -> IO<'b>`、
  `def bind`、`union Error = ReadFailed | WriteFailed | InvalidEncoding`。IO の本体は関数値なので、捕捉する値は `Capture` を満たす必要がある。
  B07 の D5 は Drop 型を関数値に捕捉すると既存の E1005 にすると決めている（E08 段 B の設計との関係は「前提」と D4）。
- std 専用 builtin: `src/check.rs` の `Builtin::IOReadLine`（`IO.__read_line`）・`Builtin::IOWrite`（`IO.__write`）。`src/polymorph.rs` の
  `Checker::builtin` は `self.module == "IO"` かつ `ModuleOrigin::Std` 以外からの参照を E1022
  `IO primitives are private to the standard IO module; compose IO actions instead` にする。
- LLVM: `src/llvm_io.rs` の `io_builtin` が `declare i32 @tsuzuri_io_read_line(ptr)` などを出し、所有結果は `src/llvm_imports.rs` の
  `host_result_slot`・`read_host_result` で受け取る。
- 連結: `src/driver.rs` は `io_runtime = text.contains("declare i32 @tsuzuri_io_")` を計算し、native では `task_runtime_source()`・`runtime/cpu.c`・
  `runtime/io.c` を一つの `task.c` に `format!` で連結して `-std=c11` と `native_compile_args` で compile する。wasm32 で `io_runtime` のときは
  `--export=tsuzuri_alloc` などを linker に渡す。`src/runtime/io.c` の関数は `TZ_IO_API`（非 Windows で `weak, visibility("hidden")`）で、
  `tsuzuri_alloc`・`tsuzuri_free` を extern で使う。
- byte 列: `ubyte` は `i8u` の別名（docs/language.md の型の表）。`Utf8String.to_bytes :: utf8string -> [ubyte]`、
  `Utf8String.from_bytes :: [ubyte] -> Option<utf8string>`、`Array.init :: i64 -> (i64 -> 'a) -> ['a]`、`Array.get`、`Array.length` がある。
- extern（E06）で利用者がホストのソケット関数を呼ぶことはできるが、型付きの所有 handle と close の保証はない（E12 で計画）。

### 再現（検証済み）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri check /tmp/tz-work-E09/today   # Main.tz: IO.write_line (Net.address_text 1)
target/release/tsuzuri run /tmp/tz-work-E09/usermod   # Main.tz と利用者の Net.tz
```

1 行目は `error[E1002]: unknown value 'Net'`（位置は `Net`）で終了コード 1。2 行目は利用者の `Net.tz` を受け付けて `net` を出す。
D1 の予約後、2 行目は E1011 `module name 'Net' is reserved for the standard library; rename the file`（既存の文言）になる。

## 仕様

### 前提とする他チケットのインターフェース

- E08 段 A（done が着手条件）: `std/Os.tz` の `union ErrorKind = NotFound | PermissionDenied | AlreadyExists | InvalidInput | InvalidEncoding | Interrupted | Other`、
  `record Error { kind: ErrorKind, code: i32 }`、`def error_of_status :: i64 -> Error`（`(kind << 32) | (code & 0xffffffff)` の復号、kind は宣言順に 1..=7）、
  `def encode :: ref string -> Result<utf8string, Error>`。`src/runtime/os.c` の feature macro と、`src/driver.rs` の `os_runtime` の判定・wasm32 の E2000・
  Windows native の E2002。`Os.ErrorKind` への case 追加は edition（G19）でだけ行う（E08）ので、E09 は case を足さない（D3）。
- E08 段 B（done が着手条件）: `File.Handle` は opaque な所有値で、操作は handle を受け取って `IO<(Handle * Result<T, Os.Error>)>` で返し、
  `close :: Handle -> IO<Result<unit, Os.Error>>` が消費する。`instance Drop<Handle>` は close の失敗を無視する。
  B07 の D5 は Drop 型の関数値への捕捉を E1005 にするので、E08 段 B はこの点を解消した形で done になっているはずである。E09 は E08 が実際に採った
  仕組みを handle 3 型へそのまま写す（D4）。
- B07（E08 段 B を通じて done）: `class Drop<'a> { def drop :: ref mut 'a -> unit }`。
- B08: Phase 1 では使わない。Phase 2 は B08 Phase 3 の native reactor（kqueue／epoll）を前提にする。

### 構文

言語の構文は変えない。std モジュール `Net` と std 専用 builtin `Net.__*` を足すだけである。

### API（Phase 1）

新 API（実装後に有効。未検証）。IO を返す関数は実行時にだけ OS に触れ、構築では何もしない。アドレスの関数は純粋で、どの target でも import なしで動く。

```tsuzuri
// std/Net.tz
record Address { v6: bool, high: i64u, low: i64u, port: i64 } deriving (Eq, Hash)  // opaque、Copy
def parse_address :: ref string -> Option<Address>     // "127.0.0.1:80"、"[::1]:80"
def parse_ip :: ref string -> i64 -> Option<Address>   // "::1" と port 0..=65535
def address_text :: ref Address -> string                     // parse_address の逆
def ip_text :: ref Address -> string                          // "127.0.0.1"、"::1"
def port :: ref Address -> i64
def is_ipv6 :: ref Address -> bool

union ErrorKind = TimedOut | ConnectionRefused | ConnectionReset | AddressInUse | AddressNotAvailable | Unreachable | Unclassified
def error_kind :: ref Os.Error -> ErrorKind                   // D3 の分類。code 0 は Unclassified

def resolve :: string -> i64 -> IO<Result<[Address], Os.Error>>   // host、port。OS の順、重複なし、1 件以上

record TcpStream { descriptor: i64, local: Address, peer: Address }      // opaque、非 Copy、Drop
record TcpListener { descriptor: i64, local: Address }                   // 同上
record UdpSocket { descriptor: i64, local: Address }                     // 同上
union Shutdown = Read | Write | Both

def connect :: Address -> Option<i64> -> IO<Result<TcpStream, Os.Error>>
def read :: TcpStream -> i64 -> Option<i64> -> IO<(TcpStream * Result<[ubyte], Os.Error>)>   // 空の列は EOF
def write :: TcpStream -> [ubyte] -> Option<i64> -> IO<(TcpStream * Result<unit, Os.Error>)>  // 全部書く
def shutdown :: TcpStream -> Shutdown -> IO<(TcpStream * Result<unit, Os.Error>)>
def close :: TcpStream -> IO<Result<unit, Os.Error>>
def stream_local_addr :: ref TcpStream -> Address
def peer_addr :: ref TcpStream -> Address

def bind :: Address -> IO<Result<TcpListener, Os.Error>>         // SO_REUSEADDR、backlog SOMAXCONN
def accept :: TcpListener -> Option<i64> -> IO<(TcpListener * Result<TcpStream, Os.Error>)>
def local_addr :: ref TcpListener -> Address                            // port 0 で bind したときの実際の port
def close_listener :: TcpListener -> IO<Result<unit, Os.Error>>

def bind_udp :: Address -> IO<Result<UdpSocket, Os.Error>>
def send_to :: UdpSocket -> [ubyte] -> Address -> IO<(UdpSocket * Result<unit, Os.Error>)>
def recv_from :: UdpSocket -> i64 -> Option<i64> -> IO<(UdpSocket * Result<([ubyte] * Address), Os.Error>)>
def udp_local_addr :: ref UdpSocket -> Address
def close_udp :: UdpSocket -> IO<Result<unit, Os.Error>>
```

- timeout は `Option<i64>` のミリ秒（D7）。`None` は無期限、`Some ms` は 1..=2,147,483,647 で、呼び出し全体の期限（deadline）になる。範囲外は
  OS を呼ばずに `InvalidInput`（code 0）。socket option や大域の既定値は使わない。
- `read`／`recv_from` の最大長は 1..=16,777,216 byte（範囲外は `InvalidInput`、code 0）。`read` は 1 回の受信で得た分だけ返し、空の列は相手の送信終了（EOF）。
  `recv_from` は datagram が最大長を超えると切り詰めずに失敗する（`InvalidInput`、code `EMSGSIZE`。datagram は消費される）。
- `write` は全 byte を送るまで繰り返す。途中で失敗したら送信済みの量は返さない（Rust の `write_all` と同じ）。`send_to` は 1 datagram を送り、timeout を取らない。

### 型規則

- 新しい型規則はない。どの関数も generic にしない（特殊化の予算を使わない）。`Address` は scalar だけの record で Copy。`opaque_record` に載せ、
  利用者は `parse_address`・`parse_ip`・`resolve` と handle の関数からだけ得る（port 70000 のような値を作れない）。
- `TcpStream`・`TcpListener`・`UdpSocket` は opaque で非 Copy（`Type::is_noncopy_record` に載せる。Drop の有無によらない）。
- `Net.__*` builtin は std の `Net` モジュールからだけ参照できる。それ以外は E1022。

### 評価順序・所有権・借用

- 各 IO 関数は E08 と同じく IO の closure の中で引数の検査・builtin の呼び出し・結果の復号を行う。IO 値を 2 回実行すれば OS 呼び出しも 2 回起きる。
  実行順は `let!`／`do!` の順。宣言には `readnone`／`readonly` を付けない（`io_builtin` と同じ）。
- handle と送信データは所有値で IO に move する。操作は成功でも失敗でも handle を返すので、timeout の後に再試行できる。`close`・`close_listener`・`close_udp`
  は handle を消費し、OS の close を 1 回だけ呼ぶ。close が失敗しても descriptor は解放済みとして扱う（POSIX の close は EINTR でも fd を再利用可能にしうる）。
  Drop（D4）は close を呼び、失敗を無視する。
- IO は Task の中で実行できない（E08）。ソケット操作は入口のスレッドで順に起き、blocking の `accept`・`read` は プログラム全体を止める。並行な接続は Phase 2。
- runtime が返す buffer の所有権は Tsuzuri に移る（E06 の契約）。runtime 内の一時領域は runtime 内で全経路解放する。

### 数値・トラップ・native と WASM の差

- `Address` の port は 0..=65535、IPv4 は `low` の下位 32 bit（`high` は 0）、IPv6 は 16 byte を big-endian で `high`・`low` に分けた値。
- builtin の結果の第 1 要素は、0 以上が成功（descriptor または 0）、負が `-(status)`（E08 D2 の状態値）。kind が 1..=7 でない負の値と、
  長さが仕様（アドレスは 20 byte の倍数）に合わない buffer は `TrapKind::BoundsCheck` でトラップする（`io_builtin` の `icmp ult` と同じ方式）。
- 既定の wasm32: `Net.__*` に到達したビルドは E2000（D11）。`tsuzuri check` は IR を作らないので報告しない。アドレスの関数だけなら import は増えない。
- Windows native: `Net.__*` に到達したビルドは E2002（D12）。
- `Net.ErrorKind` への case 追加は網羅的な `match` を壊すため、edition（G19）でだけ行う。

### セキュリティ

- 暗号化しない。TLS は E10 の公式パッケージ（D13）で、平文の TCP で秘密を送らないよう文書に書く。受信データと送信元アドレスは認証されない。
- アドレスの文字列は `Net` の厳密な解析器だけが読み、runtime には binary で渡す。`inet_aton` が受ける `127.1`・`0x7f.0.0.1`・`010.0.0.1` を拒否し、
  利用者の許可リストと実際の接続先が食い違う（SSRF の検査の回避）余地をなくす。zone 付き IPv6（`fe80::1%en0`）は Phase 1 で受けない。
- 受信は必ず最大長を取り、「最後まで読む」API は作らない（相手がメモリを使い切らせることを防ぐ）。SIGPIPE を抑止し、相手の切断でプロセスが終了しない。
  すべての descriptor に `FD_CLOEXEC` を付け、将来の子プロセスへ漏らさない。
- `resolve` の host は `Os.encode` で UTF-8 にし、空・NUL を含む・253 byte 超は `InvalidInput`（code 0）。結果は OS の resolver に依存し、信頼しない。
- `bind` の例と文書は `127.0.0.1` を使い、`0.0.0.0` がネットワークへ公開することを明記する。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1022 | std の `Net` 以外から `Net.__open` などを参照 | `network primitives are private to the standard Net module; use the Net API instead` | 参照の span |
| E1011 | 利用者の `Net.tz`（既存の検査） | `module name 'Net' is reserved for the standard library; rename the file`（既存の文言） | 既存どおり |
| E2000 | 既定の wasm32 で `Net.__*` に到達 | `wasm32 output cannot use the Net socket API because the default wasm32 target has no host imports; build for the native target, or keep to Net address parsing, which needs no host` | なし |
| E2002 | Windows の native で `Net.__*` に到達 | `the Net socket API is not supported on Windows yet (G10); build on macOS or Linux` | なし |

### 資源上限

- 最大長 16 MiB、timeout 2,147,483,647 ms（`poll` の `int`）、`resolve` の結果は重複を除いた先頭 64 件、host 253 byte、listen の backlog は `SOMAXCONN`。
  descriptor 数は OS の上限に任せる（`EMFILE` は `Other`）。確保失敗は既存どおりトラップ。

### 例

新 API（実装後に有効。未検証）。アドレスの受理例（期待値は Python の `ipaddress` の `compressed` と Node の `new URL("http://[..]").hostname` で独立に確認する）:

| 入力（`parse_address`） | `address_text` の結果 |
| --- | --- |
| `127.0.0.1:80` | `127.0.0.1:80` |
| `0.0.0.0:0` | `0.0.0.0:0` |
| `[::1]:443` | `[::1]:443` |
| `[2001:DB8:0:0:0:0:0:1]:8080` | `[2001:db8::1]:8080` |
| `[2001:db8:0:0:1:0:0:1]:1` | `[2001:db8::1:0:0:1]:1`（同じ長さの 0 の並びは先頭を省略） |
| `[2001:db8:0:1:1:1:1:1]:1` | `[2001:db8:0:1:1:1:1:1]:1`（0 の group が 1 個なら省略しない） |
| `[::ffff:192.0.2.1]:53` | `[::ffff:c000:201]:53`（末尾の dotted 形式は入力だけ） |
| `[::]:65535` | `[::]:65535` |

`None` になる入力: `127.0.0.1`（port なし）、`127.1:80`、`010.0.0.1:80`、`256.0.0.1:80`、`1.2.3.4:65536`、`1.2.3.4:080`、` 1.2.3.4:80`（空白）、`::1:80`（括弧なし）、
`[fe80::1%en0]:80`、`[1:2:3:4:5:6:7:8:9]:1`、`[1::2::3]:1`、`[12345::]:1`、`[::ffff:1.2.3]:1`、`[1:2:3:4:5:6:7::8]:1`（`::` が 0 個の group を表す）。

client の形（新 API、未検証。fixture の構文は実装時に `check` で確かめる）:

```tsuzuri
def ping :: Net.Address -> IO<Result<[ubyte], Os.Error>>
fn ping address = IO {
    let! connected = Net.connect address (Option.Some 5000)
    match connected with
    | Result.Error error -> return Result.Error error
    | Result.Ok stream ->
        let! (stream, written) = Net.write stream (Utf8String.to_bytes (Utf8String.from_string (ref "ping"))) (Option.Some 5000)
        let! (stream, received) = Net.read stream 4096 (Option.Some 5000)
        let! _closed = Net.close stream
        return (match written with | Result.Error error -> Result.Error error | Result.Ok () -> received)
}
```

拒否される例: 利用者の `Net.__close 0i32 3` は E1022、`Net.connect` を使う project の `build --target wasm32` は E2000、利用者の `Net.tz` は E1011。

### Phase 2・3（設計方針）

- Phase 2: B08 Phase 3 の reactor（kqueue／epoll）に descriptor を登録し、`Async` を返す `connect_async` などを別名で足す（Phase 1 の関数は変えない）。
  Windows（G10）: runtime の最初の呼び出しで `InitOnceExecuteOnce` により `WSAStartup(MAKEWORD(2, 2))` を一度だけ行い、`closesocket`、`WSAPoll`、
  `WSAGetLastError` の値（`WSAECONNREFUSED` など）を D3 の分類へ写す。`SOCKET` は `UINT_PTR` なので descriptor の i64 に収まる。`ws2_32` を link する。
- Phase 3: wasm32 のホスト opt-in（D11）。WASI preview2 の `wasi:sockets`、またはブラウザー・Node 向けの E13 の glue。既定の wasm32 は import なしのまま。

## 設計

### データ構造

```rust
// src/check.rs
pub enum Builtin {
    // ...
    NetResolve,  // Net.__resolve :: ref utf8string -> i64 -> (i64 * [ubyte])
    NetOpen,     // Net.__open :: i32 -> i64 -> i64 -> i64 -> i64 -> (i64 * [ubyte])
    NetAccept,   // Net.__accept :: i64 -> i64 -> (i64 * [ubyte])
    NetRead,     // Net.__read :: i32 -> i64 -> i64 -> i64 -> (i64 * [ubyte])
    NetWrite,    // Net.__write :: i32 -> i64 -> ref [ubyte] -> i64 -> i64 -> i64 -> i64 -> i64
    NetClose,    // Net.__close :: i32 -> i64 -> i64
    NetClassify, // Net.__classify :: i32 -> i64（純粋。errno → ErrorKind の宣言順 0..=6）
}
```

引数の意味（`meta` は `(v6 ? 1 : 0) << 16 | port`、`high`・`low` はアドレスの 2 語を bit 保持の `as i64` で渡す、`timeout` は `None` を -1 にしたミリ秒）:

| builtin | op | 引数 | 成功時の結果 |
| --- | --- | --- | --- |
| `Net.__resolve` | – | host、port | 0 と、20 byte のアドレス記録の列 |
| `Net.__open` | 0／1／2 | op、meta、high、low、timeout | TCP connect／TCP bind + listen／UDP bind。descriptor と 40 byte（local、peer。listen・UDP の peer は 0） |
| `Net.__accept` | – | descriptor、timeout | 新しい descriptor と 40 byte（local、peer） |
| `Net.__read` | 0／1 | op、descriptor、max、timeout | TCP recv／UDP recvfrom。0 とデータ（op 1 は先頭 20 byte が送信元） |
| `Net.__write` | 0／1 | op、descriptor、data、meta、high、low、timeout | TCP の全送信／UDP sendto（op 0 は meta 以降が 0、op 1 は timeout が -1）。0 |
| `Net.__close` | 0／1／2／3 | op、descriptor | close／shutdown `SHUT_RD`／`SHUT_WR`／`SHUT_RDWR`。0 |

アドレス記録（20 byte）: byte 0 は family（4 か 6）、byte 1 は 0、byte 2–3 は port（big-endian）、byte 4–19 は 16 byte のアドレス（IPv4 は byte 16–19）。

### 段ごとの変更

GUIDE §6.5（組み込み関数）と §6.6（ランタイム関数）のチェックリストを E08 の実装に当てはめた一覧。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/Net.tz`（新規） | 仕様の全関数 | 「アルゴリズム」の解析・表示と、IO の包み方（E08 の `std/File.tz` と同じ）。失敗は `Os.error_of_status (0 - result)` |
| 登録 | `src/stdlib.rs` | `SOURCES`, `RESERVED_MODULES`, `opaque_record`, `reserves_the_d07_table` | `std/Net.tz` を名前順に、`"Net"` を予約に、`"Net.Address" \| "Net.TcpStream" \| "Net.TcpListener" \| "Net.UdpSocket"` を opaque に、件数を +1 |
| 性質 | `src/check.rs` | `Type::is_noncopy_record` | `"Net.TcpStream" \| "Net.TcpListener" \| "Net.UdpSocket"` を足す（`Address` は足さない） |
| 検査 | `src/check.rs` | `Builtin`、全 builtin の列挙（`Self::IOReadLine,` を含む）、`Builtin::name`、型 scheme の match（`Self::IOWrite => (` の隣） | 7 variant。scheme は `Concrete(Type::I64)`・`Concrete(Type::Integer(32, true))`・`Reference`・`Array`・`BuiltinType::Tuple` |
| 検査 | `src/polymorph.rs` | `Checker::builtin` | IO の判定の隣に `Net*` の判定。`self.module == "Net"` かつ `ModuleOrigin::Std` のときだけ許し、それ以外は E1022 |
| LLVM | `src/llvm.rs` | builtin の emission の match（`Builtin::IOReadLine \| Builtin::IOWrite => emit_typed_builtin(..)`）と `emit_typed_builtin` の振り分け | 同じ 2 か所に `Net*` を足し、`emitter.net_builtin`（新規）へ |
| LLVM | `src/llvm_io.rs` | `net_builtin`（新規） | 下の declare を `intrinsics` へ。所有結果は `host_result_slot`・`read_host_result`、`ref utf8string`／`ref [ubyte]` は E08 の `os_builtin` と同じく pointer と length に分ける。第 1 要素の検査と trap。wasm では import 属性を付けない |
| runtime | `src/runtime/net.c`（新規） | 「生成 IR とランタイム」の関数 | POSIX（macOS／Linux）だけ |
| driver | `src/driver.rs` | `io_runtime` を計算する箇所、`native_runtime`、runtime を連結する `format!`、E08 が `os_runtime` で返す E2000／E2002 の分岐 | `net_runtime = text.contains("declare i64 @tsuzuri_net_")`。native なら `native_runtime` に含め、`os.c` の直後（`task_runtime_source()` より前）に連結する。wasm32 は E2000、Windows native は E2002（E08 と同じ位置・同じ emit 条件） |

### 生成 IR とランタイム

```llvm
declare i64 @tsuzuri_net_resolve(ptr, ptr, i64, i64)
declare i64 @tsuzuri_net_open(ptr, i32, i64, i64, i64, i64)
declare i64 @tsuzuri_net_accept(ptr, i64, i64)
declare i64 @tsuzuri_net_read(ptr, i32, i64, i64, i64)
declare i64 @tsuzuri_net_write(i32, i64, ptr, i64, i64, i64, i64, i64)
declare i64 @tsuzuri_net_close(i32, i64)
declare i64 @tsuzuri_net_classify(i32)
```

- 先頭の `ptr` は `host_result_slot` の out descriptor（`{ data, length }`、長さ 0 は NULL）。到達した builtin の declare だけが出る。
- `src/runtime/net.c` は `TZ_NET_API`（非 Windows で `__attribute__((weak, visibility("hidden")))`）と `struct tz_net_buffer`（io.c の `struct tz_io_buffer` と
  同じ形だが別名。同じ翻訳単位に連結されるので名前を共有しない）を使い、`tsuzuri_alloc`・`tsuzuri_free` を extern で使う。先頭に E08 の `os.c` と
  同じ feature macro を `#ifndef` 付きで置き、`getaddrinfo`・`poll`・`MSG_NOSIGNAL`（Linux）・`SO_NOSIGPIPE`（macOS）が見えることを確かめる。
- 全 socket: `socket` の直後に `FD_CLOEXEC`、macOS は `SO_NOSIGPIPE`、IPv6 は `IPV6_V6ONLY = 1`。bind（op 1）は `SO_REUSEADDR` と `listen(fd, SOMAXCONN)`。
  失敗した経路は作った fd を close してから `-(status)` を返す。

### アルゴリズム

- IPv4 の解析: `.` で区切った 4 部分。各部分は 1–3 桁の 10 進、先頭の 0 は `"0"` だけ、値は 255 以下。
- IPv6 の解析: `::` は 0 か 1 回。`:` で区切った各 group は 1–4 桁の 16 進（大文字可）。最後の部分は IPv4 形式でもよく 2 group と数える。
  `::` がなければ group は 8 個ちょうど、あれば省略前後の合計が 7 個以下。`%` を含めば `None`。
- IPv6 の表示（RFC 5952）: 小文字、各 group の先頭の 0 を除く、長さ 2 以上の 0 の並びのうち最長（同長は先頭）を `::` にする。IPv4 埋め込みの特別扱いはしない。
- ソケットアドレス: IPv4 は `a.b.c.d:port`、IPv6 は `[...]:port`。port は 1–5 桁の 10 進で先頭の 0 は `"0"` だけ、65535 以下。前後の空白は受けない。
- 期限つきの待ち（runtime の `wait`）: timeout ≥ 0 なら `deadline = CLOCK_MONOTONIC + timeout`。`poll` を残り時間で呼び、`EINTR` は残り時間を計算し直して再試行、
  残り 0 以下または `poll` が 0 なら `ETIMEDOUT`。timeout = -1 は `poll(.., -1)`。
- connect: timeout があれば `O_NONBLOCK` にして `connect`、`EINPROGRESS` なら `wait(POLLOUT)` の後 `getsockopt(SO_ERROR)`、最後に `O_NONBLOCK` を戻す。
  timeout がなく `connect` が `EINTR` のときも同じく `wait(POLLOUT)` と `SO_ERROR` で完了を待つ（再度の `connect` は `EALREADY` になる）。
- recv: `wait(POLLIN)` の後 `recv(.., MSG_DONTWAIT)`。`EAGAIN`／`EWOULDBLOCK` は `wait` へ戻る（Linux の偽の起床に備える）。max ≤ 65,536 なら stack の
  64 KiB に受けて `tsuzuri_alloc(n)` へ複写、それより大きければ `tsuzuri_alloc(max)` に受け、`n < max` なら `tsuzuri_alloc(n)` へ複写して元を解放する。
  UDP は `recvmsg` で受け、`msg_flags & MSG_TRUNC` なら `EMSGSIZE`。
- 全送信: 残りがある間 `wait(POLLOUT)` と `send(.., MSG_DONTWAIT | MSG_NOSIGNAL)`（`MSG_NOSIGNAL` がない macOS は 0 と `SO_NOSIGPIPE`）。
  どちらもない platform は `#error`。
- resolve: `getaddrinfo(host, NULL, { AF_UNSPEC, SOCK_STREAM, AI_ADDRCONFIG なし }, ..)`。OS の順を保ち、同じアドレスの 2 回目以降を除き、64 件で打ち切る。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、Net を使わないプログラムの IR を保存する（`_docs/library-reference/io.md` の検証済みの例）。
- 確認: 次がすべて成功する。stack-depth の 3 テストと特殊化上限のテストは各 `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-e09/base
printf 'def main :: IO<unit> = IO {\n    do! IO.write_line 42\n}\n' > /tmp/tz-e09/base/Main.tz
for o in -O0 -O3; do
  target/release/tsuzuri build /tmp/tz-e09/base --emit llvm $o -o /tmp/tz-e09/native$o.ll
  target/release/tsuzuri build /tmp/tz-e09/base --target wasm32 --emit llvm $o -o /tmp/tz-e09/wasm$o.ll
done
node tests/io.mjs target/release/tsuzuri && node tests/os.mjs target/release/tsuzuri
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: `std/Net.tz` の登録と `Address`

- 変更: `std/Net.tz`（新規。`Address` と 6 つのアドレス関数の仮実装）、`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`opaque_record`・`reserves_the_d07_table`、
  `tests/net_api.rs`（新規）。
- 内容: `tests/os_api.rs`（E08）の `accepts`／`rejects` を写す。テスト 1・2（「Rust テスト」）を足す。
- 確認: `cargo test --locked --test net_api` が `2 passed`、`cargo test --locked --lib reserves_the_d07_table` が `1 passed`。

### 手順 3: アドレスの解析と表示

- 変更: `std/Net.tz`、`tests/fixtures/net/address/Main.tz`（新規）、`tests/net.mjs`（新規）。
- 内容: 「アルゴリズム」の 4 項目を実装する。fixture は「例」の全入力を順に解析し、`address_text`（または `none`）を 1 行ずつ出す。`tests/net.mjs` は
  `tests/io.mjs` の `execute` を写し、native と wasm32 × `-O0`・`-O3` で実行する。
- 確認: `cargo build --release --locked && node tests/net.mjs target/release/tsuzuri address` が成功し、wasm32 の import が `["tsuzuri_io.write"]` だけ。

### 手順 4: builtin と E1022

- 変更: `src/check.rs` の `Builtin` など（「段ごとの変更」の検査 2 行）、`src/polymorph.rs` の `Checker::builtin`、`std/Net.tz` の IO 関数と handle 型、`tests/net_api.rs`。
- 内容: 7 variant と型 scheme。std の IO 関数を E08 の `std/File.tz` の形で書く（引数の検査 → builtin → 復号）。テスト 3–6 を足す。
- 確認: `cargo test --locked --test net_api` が `6 passed`。`cargo test --locked` が成功する。

### 手順 5: LLVM の宣言と結果の検査

- 変更: `src/llvm.rs`（2 か所）、`src/llvm_io.rs` の `net_builtin`（新規）、`tests/net_api.rs`。
- 内容: declare・out slot・`ref` の分解・第 1 要素の範囲検査と `TrapKind::BoundsCheck`。テスト 7・8 を足す。
- 確認: `cargo test --locked --test net_api` が `8 passed`。手順 1 の 4 つの IR を作り直し、`cmp` で byte 単位に一致する。

### 手順 6: runtime と連結（TCP の client）

- 変更: `src/runtime/net.c`（新規。`open` の op 0、`read` の op 0、`write` の op 0、`close`、`classify`）、`src/driver.rs`、`tests/fixtures/net/sockets/Main.tz`（新規）、
  `tests/net.mjs`。
- 内容: `net_runtime`、連結位置、wasm32 の E2000、Windows native の E2002。fixture は標準入力の 1 行目で case 名、2 行目で Node の port を受ける。
- 確認: `cargo build --release --locked && node tests/net.mjs target/release/tsuzuri sockets` で E2E の T1・T3・T4・T5・T6・W1・V1 が成功する。

### 手順 7: TCP の server と UDP・resolve

- 変更: `src/runtime/net.c`（`open` の op 1・2、`accept`、`read` の op 1、`write` の op 1、`resolve`）、fixture、`tests/net.mjs`。
- 内容: 「アルゴリズム」の accept・recvmsg・getaddrinfo。
- 確認: 同じコマンドで T1–T10・T12・W1 が成功する（`::1` がない環境では T10 が skip と表示される）。

### 手順 8: handle の drop

- 変更: `std/Net.tz`（D4 の `instance Drop` 3 つ）、`tests/net_api.rs`、`tests/net.mjs`。
- 内容: drop は `Net.__close 0i32 descriptor` を呼び、結果を捨てる。テスト 9 と E2E の T11 を足す。
- 確認: `cargo test --locked --test net_api` が `9 passed`、`node tests/net.mjs target/release/tsuzuri sockets` で T11 が成功する。

### 手順 9: 確保と sanitizer

- 変更: `tests/net.mjs`。
- 内容: `tests/io.mjs` の形（`--emit llvm` の IR の `@malloc`／`@free`／`@realloc` を `@tracked_*` に置換し、runtime の C と確保を数える harness を
  `-fsanitize=address,undefined` で結合）を写し、T1・T2・T7 を `-O0`・`-O3` で実行して `live == 0` を確かめる。
- 確認: `node tests/net.mjs target/release/tsuzuri` の全体が成功し、最後に 1 行の要約を出す。

### 手順 10: 全体の確認

- 変更: なし。
- 内容: 手順 1 のコマンド一式、`cargo test --locked`、GUIDE §3.1 が std・runtime の変更に挙げる suite（`tests/io.mjs`、`tests/os.mjs`、`tests/features.mjs`）。
  Linux のマシンか CI で `node tests/net.mjs target/release/tsuzuri` を実行する。
- 確認: すべて成功し、手順 1 の IR と byte 単位で一致する。Linux で実行できなければ完了報告に「Linux 未確認」と書き、受け入れ条件を満たしたと扱わない。

### 手順 11: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs _docs/library-reference/net.md _docs/library-reference/README.md _docs/feature-status.md` が成功し、`git diff --check` が空。

## テスト計画

### Rust テスト

`tests/net_api.rs`（新規）。拒否はコードとメッセージの一部を検査する。受理は IR を 2 回出して一致を確かめる（`accepts` の既存の形）。

| # | 名前 | 内容 |
| --- | --- | --- |
| 1 | `reserves_net_module` | 利用者の `Net.tz` が E1011 `reserved for the standard library` |
| 2 | `address_is_copy_and_opaque` | `Address` の 2 回使用を受理。利用者の `Net.Address { .. }` の構築は `IO` を構築したときと同じ既存のコード |
| 3 | `net_primitives_are_private` | 利用者の `Net.__close 0i32 3` が E1022 `network primitives are private to the standard Net module` |
| 4 | `types_the_phase_one_api` | 仕様の全関数を型どおりに呼ぶプログラムを受理 |
| 5 | `handles_are_not_copy` | `TcpStream` を 2 回使うと E1012 `use of moved` |
| 6 | `error_kind_match_is_exhaustive` | `Net.ErrorKind` の match で `Unclassified` を欠くと E1021 |
| 7 | `declares_only_reached_net_builtins` | `parse_address` だけのプログラムの IR に `@tsuzuri_net_` がない。`Net.close` だけなら `declare i64 @tsuzuri_net_close(i32, i64)` があり `@tsuzuri_net_open` がない |
| 8 | `net_declarations_are_effectful` | `@tsuzuri_net_` の declare の行に `readnone`・`readonly`・`memory(` がない |
| 9 | `drop_closes_handles` | handle を close せずに捨てるプログラムの IR に `call i64 @tsuzuri_net_close(i32 0,` がある |

### E2E

`tests/net.mjs`（新規）。実行: `node tests/net.mjs target/release/tsuzuri [address|sockets]`（引数なしは全部）。native は `-O0`・`-O3` の実行ファイルを
一度ずつ `build` し、各 case を新しいプロセスで実行する。peer は同じ Node プロセスの `node:net`／`node:dgram` で、必ず `listen(0, "127.0.0.1")`（または
`bind(0, "127.0.0.1")`）の `listening` を待ってから port をプログラムの標準入力へ書く。外部ネットワーク・固定ポート・経過時間の検査は使わない。
子プロセスの 60 秒の kill は hang の検出用で、合否の閾値ではない。期待値は Node 側で独立に計算する。

| case | 内容 | 期待 |
| --- | --- | --- |
| A1 | 「例」のアドレス表（受理 8、拒否 14）。native と wasm32 × `-O0`・`-O3` | 各行が表どおり。wasm32 の import は `tsuzuri_io.write` だけ |
| T1 | Node の echo server へ `Array.init 1024 (\i -> (i % 256) as ubyte)` を `write`、1024 byte 集まるまで `read` | 出力 `1024 130560`（4 × 32640）。Node が受けた列が送った列と一致 |
| T2 | プログラムが `bind 127.0.0.1:0` して `port=<n>` を出し、`accept`。Node が `hello\n` を送る。プログラムは `HELLO\n` を返し `shutdown Write`、EOF まで読む | Node が `HELLO\n` と end を受ける。プログラムの `peer_addr` の port が Node の `localPort` と一致 |
| T3 | Node が `abc` を送って end | `abc` の後に `eof` |
| T4 | Node が listen して close した port へ `connect` | `ConnectionRefused Other` |
| T5 | Node は accept だけして送らない。`read .. (Option.Some 50)` の後、同じ handle で `x` を `write` | `TimedOut Other` の後、Node が `x` を受ける |
| T6 | Node が接続直後に `resetAndDestroy()`。プログラムは 64 KiB の `write` を最大 1000 回 | `ConnectionReset` を出して終了コード 0（SIGPIPE で死なない） |
| T7 | `bind_udp 127.0.0.1:0`、`recv_from 65536`、受けた byte 列を逆順にして送信元へ `send_to` | Node が逆順を受ける。送信元の表示が Node の socket と一致 |
| T8 | Node が 100 byte の datagram を送り、プログラムは `recv_from 10` | `InvalidInput` と code が Node の `os.constants.errno.EMSGSIZE` |
| T9 | `resolve "127.0.0.1" 8`、`resolve "localhost" 80`、`resolve "" 80`、port 70000 | `[127.0.0.1:8]`、`127.0.0.1:80` か `[::1]:80` を含む、`InvalidInput`、`InvalidInput` |
| T10 | T1 を `[::1]` で行う | T1 と同じ。Node が `::1` で listen できなければ `skip: ::1` |
| T11 | stream A を明示 `close`、B を開いて 1 byte 送り、B は drop に任せる | Node が A・B の `close` を 1 回ずつ観測し、B の 1 byte を受ける |
| T12 | Node が listen 中の port へ `bind` | `AddressInUse AlreadyExists` |
| W1 | sockets fixture の `build --target wasm32`（`-O0`・`-O3`）と `check` | build は E2000 と仕様の文言、`check` は成功 |
| V1 | `read` の最大長 0、timeout `Some 0`、`parse_ip "::1" 70000` | `InvalidInput 0`、`InvalidInput 0`、`none` |

T4・T6・T12 の errno は macOS と Linux で値が違うが、`Net.error_kind` と `Os.ErrorKind` の表示は同じになる（D3）。V1 は T1 と同じ実行ファイルの case として足す。

### 既存テストへの影響

- `reserves_the_d07_table` の件数だけが 1 増える（E08 後の 27 → 28）。それ以外の期待値（`tests/io.mjs` の import、`tests/os.mjs`）は変わらない。

### 性能

- 閾値は置かない。runtime の経路は syscall が支配的で、Phase 1 では計測値を主張しない。`read` の小さい受信で `max` 分を確保しない（「アルゴリズム」）ことだけを
  IR ではなく runtime のコードで確認する。

## ドキュメント

- `docs/language.md`: E08 が足す OS API の節の後に「ネットワーク（Net）」を足す（API、timeout、エラーと `Net.error_kind`、所有と close、wasm32・Windows の制限、
  セキュリティ）。「診断」の節の E1022・E2000・E2002 の説明に Net の条件を足す。
- `docs/architecture.md`: 「標準入出力」の節の runtime の一覧に `src/runtime/net.c` と連結条件（`declare i64 @tsuzuri_net_`）、builtin の形を足す。
- `_docs/library-reference/net.md`（新規）と `_docs/library-reference/README.md` の「分野別リファレンス」: 例は T1・T2 の fixture から取り、`127.0.0.1` だけを使う。
- `README.md`: 「ネイティブ・ホスト／デスクトップ」に Net の範囲、「検証と性能測定」に `node tests/net.mjs target/release/tsuzuri`。
- `_docs/feature-status.md` と `_features/README.md` の状態欄。GUIDE D-30 の `Net` 行を D-07 へ移すのは人間が行う（完了報告に書く）。

## 受け入れ条件

- [ ] Phase 1 の全 API が macOS と Linux の native で仕様どおり動き、`tests/net.mjs` の A1・T1–T12・W1・V1 が `-O0`・`-O3` で成功する。
- [ ] `tests/net_api.rs` の 9 テストが成功し、`cargo test --locked` が成功する。
- [ ] Net を使わないプログラムの IR が native・wasm32 × `-O0`・`-O3` で手順 1 と byte 単位に一致する。
- [ ] 既定の wasm32 は Net の socket API を E2000 で拒否し、アドレスの関数だけなら import を増やさない。
- [ ] T1・T2・T7 が ASan/UBSan の harness で `live == 0`。どの case もプロセスが SIGPIPE で終了しない。
- [ ] handle はすべての経路で 1 回だけ close される（T11）。
- [ ] 「ドキュメント」の更新と `node scripts/check-docs.mjs` の成功。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- runtime は `os.c`・`io.c`・task などと一つの翻訳単位に連結される。構造体・`static` 関数・macro は `tz_net_`／`TZ_NET_` で始め、`struct tz_io_buffer` を再定義しない。
  compile error `redefinition` が出たら名前の衝突を疑う。
- glibc は `-std=c11` で `getaddrinfo`・`MSG_NOSIGNAL` を隠し、macOS は `_POSIX_C_SOURCE` だけだと `SO_NOSIGPIPE` を隠す。feature macro は最初の `#include`
  より前でなければ効かない。macOS だけで compile を確かめて終えない。
- `SO_NOSIGPIPE` は accept で得た socket にも設定する。設定漏れは T6 がプロセスの signal 終了（`status == null`）で検出する。
- BSD 系では accept した socket が listener の `O_NONBLOCK` を継承し、Linux は継承しない。listener は常に `O_NONBLOCK` にして `EAGAIN` を `wait` へ戻し、
  accept した fd は `O_NONBLOCK` を明示的に外して `FD_CLOEXEC` を付ける。`accept` の `ECONNABORTED` は相手の早期切断なので `wait` へ戻して再試行する。
- `poll` のミリ秒は残りの ns を切り上げて渡す。切り捨てると残り 1 ms 未満で timeout 0 の空回りになる。
- close は `EINTR` でも再試行しない。再試行は別スレッドが得た同じ番号の fd を閉じる危険がある。
- `IPV6_V6ONLY` の既定は OS・設定で違う。明示しないと `[::]:p` の bind が IPv4 と衝突するかどうかが環境で変わる。
- Node の peer は `listening` と `connection` のイベントを待ってから次の行を書く。子プロセスの出力と Node のイベントの順序に依存する検査を書かない。
- `resolve "localhost"` の件数と順序は環境で違う。含まれるかどうかだけを検査する。`.invalid` などの存在しない名前は DNS へ問い合わせうるので使わない。
- `cargo test --locked <pattern>` は 0 件でも成功する。`running N tests` を見る。

## 対象外

- TLS、HTTP クライアント・サーバー、WebSocket、QUIC（D13）。非同期ソケット（Phase 2）、Windows（Phase 2）、wasm32（Phase 3）。
- `TCP_NODELAY`・`SO_KEEPALIVE`・broadcast・multicast などの socket option、UDP の `connect`、Unix domain socket、raw socket、zone 付き IPv6、
  部分書き込みの量を返す `write`、最後まで読む API、利用者に見える non-blocking・`poll` の API。

## 決定事項

### D1: std モジュール名 `Net`

- 決定: std モジュール `Net` を追加し、`RESERVED_MODULES` に載せる。builtin は `Net.__*`。
- 理由: GUIDE D-30 の仮割り当てどおり。予約は利用者の `Net.tz` を拒否する（「再現」参照）ので、確定には承認が要る。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: Phase 分割と依存

- 決定: Phase 1 は blocking の native TCP・UDP・アドレス・resolve だけ。開始条件は E08 段 A・段 B（B07 を含む）と D1。B08 は Phase 2 の条件にする。
- 理由: blocking の API は B08 の `Async` を使わない。metadata の依存 B08 は非同期ソケットのためで、Phase 1 を XL の B08 の完了まで待たせる理由がない。
- 状態: 既定案（実装者はこの案に従う）

### D3: エラー型と分類

- 決定: 失敗は E08 の `Os.Error`。errno から `Os.ErrorKind` への対応: `EACCES`・`EPERM` → PermissionDenied、`EADDRINUSE` → AlreadyExists、
  `EINVAL`・`EAFNOSUPPORT`・`EADDRNOTAVAIL`・`EMSGSIZE` → InvalidInput、`getaddrinfo` の `EAI_NONAME`（と定義があれば `EAI_NODATA`）→ NotFound（code 0）、
  `EAI_SYSTEM` は errno で同じ表、それ以外の `EAI_*` と残りの errno → Other。`EINTR` は runtime が再試行する。runtime が検出した timeout は
  code に `ETIMEDOUT` を入れる（E08 の「Tsuzuri が検出した失敗は 0」の例外。OS 自身の connect timeout と区別しないため）。
  `Net.error_kind` は code を `Net.__classify` で分類する: `ETIMEDOUT` → TimedOut、`ECONNREFUSED` → ConnectionRefused、`ECONNRESET`・`ECONNABORTED`・`EPIPE` →
  ConnectionReset、`EADDRINUSE` → AddressInUse、`EADDRNOTAVAIL` → AddressNotAvailable、`ENETUNREACH`・`EHOSTUNREACH`・`ENETDOWN`・`EHOSTDOWN` → Unreachable、
  code 0 とそれ以外 → Unclassified。
- 理由: `Os.ErrorKind` の case 追加は edition が要る（E08）。errno の値は OS ごとに違うので、分類は errno の定数を知る runtime に置く。
- 状態: 既定案（実装者はこの案に従う）

### D4: handle と drop

- 決定: handle は E08 の `File.Handle` と同じ規則に従う。E08 段 B が `instance Drop<Handle>` を持つなら Net の 3 型にも `instance Drop` を置き、
  close を呼んで失敗を無視する。E08 段 B が Drop なしで出荷されたなら Net も Drop を持たず、明示 close を必須と文書化する（drop 忘れは fd の漏れ）。
  どちらでも 3 型は `is_noncopy_record` で非 Copy。大域の handle 表や世代番号は作らない。
- 理由: 資源の規則を std で一つにする。B07 の D5 は Drop 型の関数値への捕捉を E1005 にするので、IO の closure で handle を運ぶ仕組みは E08 段 B が決める。
- 見直し提案: E08 段 B の `File.Handle`（handle を IO の closure に捕捉する形）は B07 の D5 と両立しない。E08 段 B の着手前に両チケットで解消する必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D5: アドレスの表現と文字列

- 決定: `Address { v6, high, low, port }`（Copy、opaque）。解析は「アルゴリズム」の厳密な規則で、表示は RFC 5952（IPv4 埋め込みの特別扱いなし）。
  zone 付き IPv6 は受けない。runtime とは 20 byte の binary 記録で受け渡し、runtime は文字列のアドレスを解析しない。
- 理由: Copy で比較・hash でき、wasm32 でも import なしで動く。解析器を一つにすると SSRF の検査と接続先が食い違わない（「セキュリティ」）。
- 状態: 既定案（実装者はこの案に従う）

### D6: 名前解決

- 決定: `getaddrinfo`（`AF_UNSPEC`、`SOCK_STREAM`、`AI_ADDRCONFIG` なし）。OS の順を保ち、重複を除き、64 件まで。host の検査は「セキュリティ」のとおり。
- 理由: OS の設定（`/etc/hosts`、resolver の優先順位）を尊重する。`AI_ADDRCONFIG` は loopback だけの環境で `localhost` を消すことがある。
- 状態: 既定案（実装者はこの案に従う）

### D7: timeout

- 決定: 操作ごとの `Option<i64>` のミリ秒で、呼び出し全体の deadline。`None` は無期限。`poll` と `CLOCK_MONOTONIC` で実装し、`SO_RCVTIMEO` などの
  socket の状態や大域の既定値は使わない。範囲外は `InvalidInput`（code 0）。
- 理由: 呼び出しを読めば待ち時間がわかり、隠れた状態がない。`poll` は macOS／Linux で同じ意味を持つ。0 を拒否するのは「待たない」と「無期限」の取り違えを防ぐため。
- 状態: 既定案（実装者はこの案に従う）

### D8: builtin の形

- 決定: 「データ構造」の 7 builtin と op 番号、20 byte のアドレス記録、第 1 要素の符号で成否を表す規則。
- 理由: E08 の builtin と同じ部品（`i64`・`ref`・`(i64 * [ubyte])`）だけで表せ、check・polymorph・llvm の箇所を増やさない。
- 状態: 既定案（実装者はこの案に従う）

### D9: runtime と連結条件

- 決定: `src/runtime/net.c` を IR に `declare i64 @tsuzuri_net_` があるときだけ native に連結し、`os.c` の直後に置く。
- 理由: 使わないプログラムのビルド・大きさ・cache の key を変えない。feature macro を task などの include より前に置くため。
- 状態: 既定案（実装者はこの案に従う）

### D10: socket の既定

- 決定: 全 socket に `FD_CLOEXEC`、macOS は `SO_NOSIGPIPE`、送信は `MSG_NOSIGNAL`（あれば）、IPv6 は `IPV6_V6ONLY = 1`、listener は `SO_REUSEADDR`・`SOMAXCONN`・
  `O_NONBLOCK`。`SO_REUSEPORT` と `TCP_NODELAY` は使わない。
- 理由: signal による終了と fd の漏れを防ぎ、OS 間の差をなくす。`SO_REUSEADDR` は再起動直後の `TIME_WAIT` による bind 失敗を避け、稼働中の listen との衝突は検出したまま。
- 状態: 既定案（実装者はこの案に従う）

### D11: wasm32

- 決定: Phase 1 の既定 wasm32 は `Net.__*` に到達したビルドを E2000 で拒否する（承認を待たず実装する）。wasm32 で Net を動かすホスト opt-in（WASI preview2 の
  `wasi:sockets`、または E13 の glue。CLI は E08 の `--wasm-host` の値として足すか別にするかを含む）は Phase 3 で設計する。
- 理由: D-18 の「既定は import なし」。黙って失敗値を返すと native との差が実行時まで見えない。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D12: Windows

- 決定: Phase 1 は macOS／Linux だけ。Windows の native で `Net.__*` に到達したビルドは E2002。Winsock は Phase 2 の設計方針どおり G10 と行う。
- 理由: G10 は blocked で Windows の実行検証ができず、検証できない C を完了と扱わない（E08 D11 と同じ）。
- 状態: 既定案（実装者はこの案に従う）

### D13: HTTP／TLS の配置

- 決定: std は TCP・UDP・アドレス・resolve までとし、HTTP／TLS は公式パッケージ（E10）にする。TLS は証明書検証を既定で有効にする。
- 理由: TLS は OS ごとの実装（Security.framework、OpenSSL、Schannel）か外部ライブラリが要り、更新の周期が言語と合わない。
- 状態: 既定案（実装者はこの案に従う）

### D14: 受信の最大長と UDP の切り詰め

- 決定: `read`／`recv_from` は 1..=16 MiB の最大長を必ず取る。UDP の datagram が最大長を超えたら `InvalidInput`（code `EMSGSIZE`）で、切り詰めた列を返さない。
- 理由: 相手がメモリを使い切らせることを防ぎ、データの欠落を黙らない。
- 状態: 既定案（実装者はこの案に従う）
