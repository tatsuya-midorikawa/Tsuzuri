# E09: ネットワーク API

| 項目 | 内容 |
| --- | --- |
| ID | E09 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | E08, B08 |
| 後続 | – |
| 状態 | done（Phase 1・2・3） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | main `96d7cbf` の上のブランチ `impl/e09-net`、2026-10-10。Phase 1 `de2766c`、Phase 2 `5ab4da9`、`resolve` のレビュー対応 `9efbf87`、Phase 3 `9ab610d`。Phase 2 のレビュー対応（CI の手順、Windows の UDP と `accept`）は、同ブランチのそのあとの 2 コミット |
| 承認 | 全フェーズと D1・D11 を含む判断への包括承認（依頼者の不在中に実装）。GUIDE の [D-44](../GUIDE.md#d-44-第2期の-4-チケットe09f09f10c11の確定) に記録した |
| 改善する劣位 | C#/F# 比: ネットワーク API がない |
| 利用者向け仕様 | [Net](../../_tsuzuri/language-reference/built-in-types-and-modules/net.md)、[WebAssembly への出力](../../_tsuzuri/language-reference/compiler/webassembly.md#net-のソケットnodejs)、[言語仕様](../../docs/language.md#ネットワークnet) |

## 目的と実装範囲

TCP／UDP ソケットを標準モジュール `Net` で扱えるようにし、将来の HTTP クライアント（E10 の公式パッケージ）の土台にする。
Rust の `std::net`、.NET の `TcpClient`／`TcpListener`／`UdpClient` に相当する。TLS・HTTP は作らない（D13）。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | macOS・Linux の blocking TCP client・server、UDP、アドレスの解析と表示、`resolve`、`close` と `with_*` の括弧 |
| Phase 2 | `Async.block_on` の中の `_async` の双子と 1 本の監視スレッド、Windows の Winsock |
| Phase 3 | wasm32 の opt-in `--wasm-feature net`（`--wasm-feature jspi` が必要）。生成した Node.js のグルーが `node:net`・`node:dgram`・`node:dns` で実装する |

旧計画は「着手前に再検討する」指示のもと、`96d7cbf` と照らして次の点を改めた。

- E08 の `File.Handle` は `Drop` 型ではなく、世代検査付きの runtime の表を指す `Copy` の添字として出荷された。std の型は `Drop` を持てず（E1016）、
  `Drop` 値は `let!` をまたげない（E1005）ためである（GUIDE D-32、[E08](E08-os-api.md) の D12）。旧計画の「操作が handle を返し直す `Drop` 型」は作れない。
- E08 は WASI（`--wasm-host wasi`）と Windows の E2002 を出荷し、test／bench の runtime は別に連結する。`net.c` も同じ配線にした。
- `Os.ErrorKind` は 7 case（`Interrupted` を含む）で、`Os.error_of_status` が `(kind << 32) | code` を読む。case は足さない。
- 新しい std は opt-in（D-40、`stdlib::OPT_IN`）。`Net` は予約（`RESERVED_MODULES` は 47 名。4 つのチケットを統合した後は 53 名）して opt-in にした。
- B08 の native reactor はタイマーと外部の完了（mailbox と `tsuzuri_async_post`）だけで、kqueue／epoll／IOCP を持たない（D-42・D-43）。
- G10 は blocked のままだが、B08 の reactor は Windows の実装を持ち、CI だけで検証されている（D-43）。

## 確定した API と意味

```text
parse_address :: ref string -> Maybe<Address>          parse_ip :: ref string -> i64 -> Maybe<Address>
address_text, ip_text :: Address -> string             port :: Address -> i64       is_ipv6 :: Address -> bool
resolve       :: string -> i64 -> IO<Result<[Address], Os.Error>>
connect       :: Address -> Maybe<i64> -> IO<Result<TcpStream, Os.Error>>
read          :: TcpStream -> i64 -> Maybe<i64> -> IO<Result<[ubyte], Os.Error>>
write         :: TcpStream -> [ubyte] -> Maybe<i64> -> IO<Result<unit, Os.Error>>
shutdown, close, stream_local_addr, peer_addr
bind, accept, local_addr, close_listener
bind_udp, send_to, recv_from, udp_local_addr, close_udp
with_connection, with_accepted, with_listener, with_udp        -- 本体の成否にかかわらず戻る前に閉じる括弧
error_kind    :: Os.Error -> ErrorKind     -- TimedOut | ConnectionRefused | ConnectionReset | AddressInUse | AddressNotAvailable | Unreachable | Unclassified
connect_async, accept_async, read_async, write_async, recv_from_async, send_to_async     -- Async<Result<...>>
bind_async, bind_udp_async, close_async, close_listener_async, close_udp_async, shutdown_async
```

- `TcpStream`・`TcpListener`・`UdpSocket` は `Copy` の不透明な record（`id` が runtime の表の添字）で、利用者は構築できない（E1022）。
  handle は `(generation << 32) | (slot + 1)`。世代は開くたびに増え（2^31 で 1 に戻る）、閉じた・古い・種類の違う handle は `InvalidInput`（`code` は `EBADF`）で、
  別のソケットに当たらない。`Drop` は持たない。閉じ忘れた descriptor はプロセスの終了まで残り、そのことを文書に書いた。
- `Address { v6, high, low, port }` は `Copy`・`Eq`・`Hash`・`Display`。`parse_address`・`parse_ip` は厳密で、`inet_aton` の形（`127.1`・`0x7f.0.0.1`・`010.0.0.1`）、
  ゾーン、先頭の 0、空白、ポートの省略を `None` にする。表示は RFC 5952。runtime とは 20 byte の record（family、0、big-endian の port、16 byte）で受け渡し、
  runtime はアドレスの文字列を読まない。アドレスの解析と表示は純粋で、どの target でも import なしで動く。
- 状態は `(kind << 32) | code`。kind は `Os.ErrorKind` の番号で、runtime が検出した timeout は `code` に `ETIMEDOUT` を持つ `Other`。
  `Net.error_kind` が runtime の `__classify` で errno を 6 種に分ける（OS ごとの番号の違いを runtime が吸収する）。
- 時間制限は `Maybe<i64>` のミリ秒（1〜2,147,483,647）で、呼び出し全体の期限。`poll`（Windows は `WSAPoll`）と単調時計（`CLOCK_MONOTONIC`、Windows は
  `QueryPerformanceCounter`）で測り、socket の状態や大域の既定値は使わない。範囲外は OS を呼ばずに `InvalidInput`（`code` 0）。
- 受信は必ず最大長（1〜16 MiB）を取る。`read` は 1 回の受信で得た分で、空の配列は相手の送信終了。`recv_from` は最大長より長い datagram を切り詰めず
  `InvalidInput`（`code` は `EMSGSIZE`）にして捨てる。`write` は全部を送る。
- socket の既定: 子プロセスへ継承しない（`FD_CLOEXEC`、Windows は継承しないハンドル）、SIGPIPE を起こさない（`MSG_NOSIGNAL`、macOS は `SO_NOSIGPIPE`）、
  IPv6 は `IPV6_V6ONLY`、listener は `SO_REUSEADDR`（Windows は `SO_EXCLUSIVEADDRUSE`）と `SOMAXCONN`。
- **`resolve` は、アドレスに読める host を OS に渡さない**（Phase 1 のレビューでの指摘）。`getaddrinfo` は `parse_ip` が拒む書き方（`127.1`・`0x7f000001`・
  `2130706433`・`010.0.0.1`・`1.2.3`・`0`・`fe80::1%lo0`）を数値のアドレスとして読み、読み方も OS で違う（`0177.0.0.1` は macOS が 177.0.0.1）ので、
  `parse_ip` で「名前」と分けてから `resolve` を呼ぶ許可リストの検査を抜けられた。host が `:` か `%`、空白か制御文字（0x20 以下と 0x7f）、ASCII 以外のバイトを含む、
  または最後のラベル（末尾の `.` が 1 つあれば除く）が数字だけか `0x`・`0X` で始まるときは、`parse_ip host port` だけで決める。厳密に読めればシステムを呼ばずに
  そのアドレス 1 つを返し、読めなければ `InvalidInput`（`code` 0）。名前だけが `getaddrinfo`（`AF_UNSPEC`、`SOCK_STREAM`、`AI_ADDRCONFIG` なし）へ進み、
  OS の順で、重複を除き 64 件まで。native の runtime は、さらに `AI_NUMERICHOST` の `getaddrinfo` を先に呼び、システムが数値と読む host を `InvalidInput` にする。
  分類が取りこぼした書き方があっても、システムのアドレス解析がアドレスを決めない。wasm32 の `dns.lookup` はこのフラグを受け取れないので、分類（と、グルーが持つ同じ規則）だけが守る。
  ASCII 以外を断るのは、Node.js の `dns.lookup` が host を IDNA で変換してから OS に渡し、全角の `１２７．０．０．１`・`127。0。0。1`・`127｡0｡0｡1`・後ろに U+00AD や U+200B が付いた
  `127.0.0.1` を 127.0.0.1 に引くため（2 回目のレビュー）。国際化ドメイン名は `xn--` の形で書く。
  `resolve` の非同期版は作らない（`resolve` は同期で、`block_on` の前に済ませる）。

### 非同期（Phase 2）

- `Async` の計算は `IO` を実行できないので、操作ごとに `_async` の双子を `Async.host` の上に作った。同期版は変えない。`Net` は `Async` を使うので、`OPT_IN` の `uses` に `Async` を置く。
- B08 の反応器は変えない（`Async.tc` は無変更、`async.c` はコメントだけ）。ソケットの準備は 1 本の分離スレッドが見る。最初の待ちで始まり、待ちが無くなると終わる。
  `poll`（Windows は `WSAPoll`）を、起床用の pipe（Windows は自分宛ての UDP ソケット）とすべての待ちに対して呼び、完了（0 か状態）を IR が渡す
  `tsuzuri_async_post` へ入れる。ロックの順序は監視の mutex、ソケット表の mutex。
- 操作は時間制限 0 で一度だけ試し（待つ必要があれば `Other`・`code` 0 の状態）、必要なら `Net.__watch`（接続は `Net.__connect`、取消は `Net.__unwatch`）で準備を頼み、再試行する。
  接続の途中のソケットだけが待ちの所有物で、取消と post の拒否で閉じる。ソケットを閉じるときは、記述子を閉じる前にそのソケットの待ちを外して起こす。
- 完了は何も所有しない。`Async.all_results` が捨てた、届いていない完了には `cancel` が呼ばれず、成功した接続や accept の結果は他の handle と同じく閉じ忘れとして漏れる。
- `Async.block_on` のない native のプログラムが非同期の操作に到達すると、完了を受け取る反応器が無いので `E2000`。

### Windows（Phase 2）

Winsock の実装を `net.c` に持つ。`WSAStartup` は `InitOnceExecuteOnce` で一度、`closesocket`・`WSAPoll`・`ioctlsocket`・`WSAGetLastError` を同じ分類へ写し、
`SOCKET` を `i64` の表に入れる。共有の翻訳単位の errno の macro とぶつからないよう、内部の名前は `TZ_NET_E_*`。`ws2_32` はドライバーが `-lws2_32` で、MSVC は
`#pragma comment` でもリンクする。`WSAPoll` は、失敗した非同期の接続を正しく報告する Windows 10 バージョン 2004 以降を前提にする。
Windows だけの 2 つの既定は POSIX に揃える。UDP のソケットは、閉じたポートへの送信の ICMP を次の受信の `WSAECONNRESET` で報告する既定（`SIO_UDP_CONNRESET`）を
`tz_net_create` が切り（切れなかったときのために受信も飛ばす）、`accept` は、相手が `accept` の前に RST した接続の `WSAECONNRESET` を `ECONNABORTED` と同じく飛ばして次の接続を返す
（同期版と `accept_async` は同じ `tsuzuri_net_accept`）。
**Windows の実行は CI だけで検証され、手元では実行していない。**

### wasm32（Phase 3）

- 既定の wasm32 は import を持たず、WASI preview1 には `connect`・`bind`・`listen` がなく（`sock_accept`・`sock_recv`・`sock_send`・`sock_shutdown` だけ）、
  preview 2 のソケットはコンポーネントモデルを要する（E13 は未提供）。そこで、ソケットを持つのは `--wasm-feature net` を明示した Node.js 向けのビルドだけにした。
  付けないと、ソケットに到達するビルドは従来どおり `E2000`。
- `--wasm-feature net` は wasm32 の `object`・`llvm`・`wasm`・`bindings-js` で使え、`--wasm-feature jspi` が必要（待つ呼び出しが WebAssembly のスタックを中断するため）。
  threads・`--wasm-host`・wasm64・native との併用は `E2000`。
- IR は native と同じ `tsuzuri_net_*` の宣言のままで、`llvm::with_net_imports` が、到達した宣言だけにモジュール `tsuzuri_net` の import の属性を付ける（`net.c` は連結しない）。
  14 個の import は `resolve`・`open`・`accept`・`read`・`write`・`try_accept`・`try_read`・`close`・`classify`・`names`・`send`・`watch`・`unwatch`・`connect`。
  `try_accept`・`try_read` は、時間制限 0 の `accept`・`read` の入口で、中断しない普通の import（`llvm_io.rs` の `waiting_call`。wasm のときだけ、時間制限を実行時に見て分ける）。
  待つ import は JSPI の `WebAssembly.Suspending` なので、`Async.start` の実行器（ホストが `promising` の外から進める）から呼ぶと、同期の値を返しても V8 が `SuspendError` にする。
- `--emit bindings-js` のグルーが `bindings-net.mjs` で実装する。`net.c` と同じ約束（世代検査付きの表、`(kind << 32) | code`、呼び出し全体の期限、時間制限 0 の「1 回だけ試す」）で、
  待つ 5 個は `WebAssembly.Suspending`、`watch`・`connect` は `tsuzuri_async_complete` で完了する。`src/bindings.rs` の `javascript_with_net` が `bindings-core.mjs` の 3 か所へ
  差し込み、一意性を assert する。ドライバーは、ソケットに到達するモジュール（`llvm::reaches_net`）にだけこのグルーを使い、到達しないモジュールは `--wasm-feature net` を付けても
  `jspi` だけのグルーになる（`.wasm` もその場合は alloc を export しない）。
- ブラウザーには生の TCP も UDP もないので、`node:` のモジュールを読めない環境では `load` が `Net sockets need Node.js ...` で失敗する。文書に書いた。
- import は Node.js の例外を呼び出しの状態にして返す（`guard`）。ソケットは、モジュールが待っているあいだだけプロセスを生かし、インスタンスが捨てられるとすべて閉じる。
  読まれないまま届く分は、UDP が 1 MiB・4096 個、listener が 128 接続まで。TCP は `allowHalfOpen`。非同期の UDP の送信は、結果を Node.js が次のターンに返すので、呼び出しが送信を 1 回だけ行って
  「受け取った」状態（`ACCEPTED` + 送信の番号）を返し、std の待ち（`__watch` の `events = 2 + 4 * 番号`）がその送信自身の結果で完了する（呼び出しをやり直さず、データグラムは送信の数だけ出る）。
  `resolve` は ASCII 以外の host を全ターゲットで `InvalidInput`（Node.js の IDNA の変換のため）。
- ネイティブとの違い: listener の `SO_REUSEADDR` は Node.js の既定、`shutdown` の `Read` は `SHUT_RD` を呼ばず以後の `read` を EOF にする、`write` の期限は積んだデータの送信を取り消さない、
  `write_async` の下の送信は 1 回に 64 KiB まで、`close` は受け取ったデータを送り終えてから閉じ（それまで Node.js は終わらない）、UDP のポート 0 宛ては `EINVAL`、
  `bind_async`・`bind_udp_async` は `Async.start` の実行器では使えない。

## 診断

| コード | 条件 |
| --- | --- |
| E1022 | std の `Net` 以外から `Net.__*` を参照、または `Address`・handle の record を構築・更新 |
| E1011 | 利用者の `Net.tz`（既存の検査） |
| E2000 | 既定の wasm32 と `--wasm-host wasi` でソケットに到達、`--wasm-feature net` の条件の不備（jspi なし、threads・WASI・wasm64・native との併用）、`Async.block_on` なしの native で非同期の操作に到達、`--freestanding` |
| E2002 | macOS・Linux・Windows 以外の native でソケットに到達 |

## 旧計画から外れた判断

| 旧判断 | 確定した判断と理由 |
| --- | --- |
| D4: 非 Copy の `instance Drop` の handle を操作が返し直す | `Copy` の不透明な添字と世代検査付きの表。std は `Drop` を持てず、`Drop` 値は `let!` をまたげない。操作は `IO<Result<T, Os.Error>>` だけを返し、`close` と `with_*`、プロセス終了時の close で資源を守る |
| T11（drop の検査）、手順 8、型規則と `is_noncopy_record` の登録 | T11 は「close は最終で、slot が再利用されても古い handle を拒否し、括弧は閉じる」。3 型は `Copy` なので登録しない。`Net.Address` は `Copy` のまま |
| D6: host を `getaddrinfo` へ渡す | アドレスに読める host は渡さない。`resolve` の節のとおり、レビューで指摘された許可リストの回避（SSRF の検査）を塞ぐ |
| D11: wasm32 は WASI preview2 か E13 のグルー | `--wasm-feature net`（`jspi` が必要）と生成した Node.js のグルー。preview1 に `connect`・`bind`・`listen` がなく、preview 2 はコンポーネントモデルが要る |
| D12: Windows は E2002 | Winsock を実装。ただし G10 は blocked で、実行は CI だけで検証する |
| Phase 2: 反応器に descriptor を登録（kqueue／epoll） | B08 の反応器は持たない。ソケットの準備を見る 1 本の監視スレッドが、既存の mailbox（`tsuzuri_async_post`）へ完了を入れる |
| Phase 2: `resolve` の非同期版 | 作らない。同期で、`block_on` の前に済ませる |
| `Net` は `Async` に依存しない | `Net` は `Async` を使う（`OPT_IN` の `uses`）。`IO` は `Async` の中で実行できないので `_async` の双子を持つ |
| 旧 `_docs/` と i64 既定の例 | 現在の日本語リファレンス、i32 既定、HEAD の `IO`・`File`・`Os`・`Process` に合わせた |

## 決定事項（確定）

| ID | 決定 |
| --- | --- |
| D1 | std モジュール `Net` を `RESERVED_MODULES` に載せ、opt-in にする。builtin は `Net.__*`（12 個） |
| D2 | Phase 1 は B08 に依存しない。Phase 2 が `Async` に接続する |
| D3 | 失敗は `Os.Error`。errno から `Os.ErrorKind` への対応と `Net.error_kind` は `docs/language.md` の表のとおり。`Os.ErrorKind` に case は足さない |
| D4 | 上の表のとおり、`Copy` の添字と世代検査付きの表 |
| D5 | `Address` は `Copy` の不透明な record。解析器は一つで、runtime は 20 byte の record を受け渡す |
| D6 | 上の `resolve` の規則 |
| D7 | 時間制限は操作ごとの `Maybe<i64>` で、呼び出し全体の期限 |
| D8 | builtin は `i64`・`ref`・`(i64 * [ubyte])` だけで表し、第 1 要素の符号で成否を表す。12 個 |
| D9 | `net.c` は IR が `@tsuzuri_net_` を宣言したときだけ native に連結し、`os.c` の直後に置く。`tsuzuri test`・`bench` の runner も同じ条件 |
| D10 | socket の既定は上のとおり |
| D11 | wasm32 は `--wasm-feature net`（上のとおり） |
| D12 | macOS・Linux・Windows。他の native は E2002 |
| D13 | std は TCP・UDP・アドレス・resolve まで。HTTP／TLS は公式パッケージ（E10）。通信は平文 |
| D14 | 受信の最大長は 1〜16 MiB。UDP の切り詰めは `InvalidInput`（`EMSGSIZE`） |

## 受け入れ条件

- [x] Phase 1 の全 API が macOS の native で仕様どおり動き、`tests/net.mjs` の address・resolve・sockets が `-O0`・`-O3` で成功する。Linux は警告なしでコンパイルできることまで（実行は CI）。
- [x] `tests/net_api.rs` が成功し、`cargo test --locked` が成功する。
- [x] Net を使わないプログラムの IR・ログが、変更前と byte 単位で一致する。
- [x] 既定の wasm32 は Net のソケットを `E2000` で拒否し、アドレスの関数だけなら import を増やさない。`--wasm-feature jspi --wasm-feature net` は Node.js 24 で動く。
- [x] 確保の追跡（ASan／UBSan）で `live == 0`、runtime の確保が 0、非同期の case は descriptor が開始時に戻る。`TSUZURI_TSAN=1` の ThreadSanitizer も成功。
- [x] どの case もプロセスが SIGPIPE で終了しない。close は 1 回だけ成功し、古い handle は拒否される。
- [x] `resolve` は、アドレスに読める host をシステムに読ませない（T9b、C の harness、`getaddrinfo` の呼び出しの計数）。
- [x] 言語リファレンスなどの更新と `node scripts/check-docs.mjs` の成功。

## 実装と検証（2026-10-10）

### 実装したファイル

- `std/Net.tz`。登録は `src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES`・`OPT_IN`）、builtin は `src/check.rs`・`src/polymorph.rs`（E1022）。
- `src/llvm.rs`・`src/llvm_io.rs`（`net_builtin`・`net_async_builtin`・`with_net_imports`）、`src/driver.rs`・`src/test_runner.rs`・`src/main.rs`（連結、E2000／E2002、`--wasm-feature net`）。
- `src/runtime/net.c`（POSIX と Winsock、表、監視スレッド）、`src/bindings.rs`・`src/runtime/bindings-net.mjs`・`src/runtime/bindings-net-host.mjs`（wasm32 のグルー）。
- テスト: `tests/net_api.rs`、`tests/net.mjs`（address・resolve・sockets・async・runtime・alloc）、`tests/net_runtime.c`、`tests/net_wasm.mjs`、`tests/windows.rs`、
  `tests/fixtures/net_address`・`net_resolve`・`net_sockets`・`net_async`・`net_wasm`。
- `.github/workflows/vscode.yml`（Net の手順を最後に追加。Windows の address・resolve・async、wasm32）、`vsc/src/core.ts`（予約名）。
- 文書: `net.md`、`webassembly.md`、`option.md`、`diagnostics.md`、`modules.md`、`file.md`、`os.md`、`async.md`、`strategy.md`、`docs/language.md`、`docs/architecture.md`、`README.md`、`index.md`。

### 検証結果

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`（38 ファイル）、`git diff --check` は成功。
- `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast` は **994 件成功**。`tests/net_api.rs` は 14 件（予約、E1022、全関数の型、IR の不変条件、到達した宣言だけ、決定性、
  `resolve` の CLI 検査）。`TSUZURI_WINDOWS_SDK` を付けた `tests/windows.rs` の Net の cross-link は、MSVC の x86_64 をリンク、aarch64 をコンパイルできた。
- `node tests/net.mjs` の全 block が `-O0`・`-O3` で成功: address（3,200 入力を Node と照合、native と wasm32）、resolve（880 個のアドレス文字列。手書きと seed 付きの生成で、
  基準は Node の `net.isIPv4`・`isIPv6` と URL）、sockets（T1–T12、A1、V1 を新しい handle の規則に合わせたもの）、async（取消、時間制限、待ちながらの close、並行する接続）、
  runtime（`tests/net_runtime.c`。plain と address-undefined、`TSUZURI_TSAN=1` で thread も）、alloc（12 回ずつ、ASan／UBSan。`live == 0`、runtime の確保 0、
  非同期は descriptor が開始時に戻る。`TSUZURI_TSAN=1` で async を ThreadSanitizer でも）。
- 確保の追跡の harness は `getaddrinfo` の呼び出しを数える。`resolve` の case は `localhost` の 2 回（数値かの確認と問い合わせ）だけで、アドレス文字列の corpus は 0 回、
  システムが数値と答えた回数は常に 0。変異検査: 分類を無効にすると harness が「システムが 36 個の host をアドレスとして読んだ」で失敗し、修正前の `net.c` では
  `tests/net_runtime.c` が最初の数値の host で失敗する（どちらも元に戻して成功を確認した）。
- `node tests/net_wasm.mjs`（Node.js 24.21、JSPI）が `-O0`・`-O3` で成功: ブロッキングと非同期の echo、サーバー、UDP、`resolve`（`dns.lookup` に届くのは名前だけ）、接続拒否、時間制限、
  取消、close、30 クライアントの並行、IPv6、切り詰め、3 MiB の読みと 4 MiB の書き。import は到達した 12 個だけ。既定の wasm32 は import なしと E2000、オプションの誤りも検査する。
- `node tests/io.mjs`・`os.mjs`・`async.mjs`（Node.js 20 と 24）・`features.mjs`（12,686 ケース）が成功。fixtures と examples の 1,038 個の IR とログ（native と wasm32、`-O0`・`-O3`）は、
  変更前（main の `96d7cbf`）とバイト単位で一致した。
- `node scripts/check-docs.mjs` は 10 ページ、236 links、30 examples、52 native runs が成功。
- Windows: MSVC（x86_64 はリンク、aarch64 はコンパイル）と MinGW（x86_64・aarch64 のコンパイル）で `net.c` が `-Wall -Wextra -Werror` で警告なし。実行は CI だけ。
- Linux: glibc と musl の x86_64・aarch64 へ、`net.c` と `tests/net_runtime.c` が `-std=c11 -Wall -Wextra -Werror` で警告なしにコンパイルできた（`zig cc`）。実行はしていない。
- レビューで見つかった 2 件を直した。(1) `resolve` が `inet_aton` の形をシステムに読ませた（上の `resolve` の規則、`9efbf87`）。(2) Phase 3 のホストが、Node.js の内部で積んだだけの
  データを「送れた」と数え、直後の `close` が捨てた（`write_async` の送信を 1 回 64 KiB に絞り、`close` を送り終えてからにした。Phase 3 のコミット）。
  どちらも負荷の下で再検証して見つかった。テストは sleep の後に検査する形をやめ、イベントを待つ形にした（`d83f458` と Phase 3 のコミット）。
- 統合担当のレビュー（Phase 2 のコミット）で見つかった 4 件を直した（このあとの 2 コミット）。(1) CI の手順は `vsc/` から `node ../tests/net.mjs` を呼ぶが、スクリプトは
  リポジトリの根からの相対パスを開いていた。`net.mjs` と `net_wasm.mjs` は、スクリプトの位置から根を求め、コンパイラとツールのパスを絶対にしてから根へ移る。手順は `vsc/` から実際に実行した
  （`vsc/toolchain/bin/tsuzuri` を介して、Homebrew の LLVM 21 と Node.js 24 で。macOS の分岐とサニタイザー付きの Linux の分岐）。(2) async の取消のテストは、Linux で
  `EADDRNOTAVAIL` になる 99 を「分類されない」コードに使っていた。どの OS にも無い 2147418113 に替えた（wasm の fixture も）。(3) Windows の UDP は、`SIO_UDP_CONNRESET` の既定のために、
  閉じたポートへ送った後の受信が `ConnectionReset` になる。(4) Windows の `accept` は、`accept` の前に RST した接続を `WSAECONNRESET` のエラーにして、サーバーのループを終わらせる。
  (3)(4) は上の Windows の節のとおりに直し、POSIX の契約を `tests/net_runtime.c`（`test_accept_reset`・`test_datagram_after_closed_port`）で固定し、同じ手順を
  Windows の CI でも実行する E2E（async の `accept_reset` と `udp` の `vanished`）を足した。
- CI の手順の見直しで、もう 1 件見つかった。macOS の CI は Homebrew の LLVM 21 で `net.mjs` の全 block を実行するが、B08 は、その LLVM でサニタイザー付きの実行ファイルが macOS で起動で止まることを
  確かめている（この環境の macOS でも、空の C プログラムで再現した）。`TSUZURI_NO_SANITIZERS=1` を足し、macOS の CI だけがこれを付ける（Linux は `libclang-rt-21-dev` で ASan／UBSan を使う）。
  1 つの手順の中の各コマンドは、1 つが失敗しても残りを実行する。
- 統合担当のレビュー（Phase 3 のコミット）で見つかった 11 件を直した（このあとのコミット）。(1) Node.js の `dns.lookup` は host を IDNA で変換するので、全角の `１２７．０．０．１` などが
  127.0.0.1 に引けた（`numeric_looking` に ASCII 以外のバイトを足し、グルーも同じ規則を持つ。全ターゲットで `InvalidInput`）。(2) 捨てられたインスタンスのソケットが残り、
  listener が bind されたままで、閉じ忘れたプログラムの後に Node.js が終わらなかった（`owner.dispose` と、待っているあいだだけ `ref`）。(3) Node.js の例外（ポート 0 の `dgram.send`）が
  「stack exhausted」でインスタンスを捨てた（すべての import を `guard` で包む）。(4) 相手の半クローズでこちらの書き込みが失敗した（`allowHalfOpen`）。(5) 失敗した非同期の UDP の送信が
  成功と報告され、以後の `recv_from` が古い失敗を返し続けた（送信の結果をその送信の呼び出しで返す）。(6) 読まれないうちの RST が EOF に見えた（エラーを 1 回返してから EOF）。
  (7) UDP と accept の待ち行列が無制限で、`error` の event が処理されなかった。(8) ソケットに到達しないモジュールに `--wasm-feature net` を付けると、読み込めないグルーになった
  （`llvm::reaches_net`）。(9) `Async.start` の実行器から `read_async`・`accept_async`・`recv_from_async` を呼ぶと `SuspendError`（時間制限 0 は中断しない `try_read`・`try_accept`）。
  (10) 2 GiB 以上のポインタが負の数でグルーに届いた（`>>> 0`）。(11) `bindings-core.mjs` の `memory64` の走査が import の名前を 1 バイト短く飛ばし、有効な wasm32 のモジュールを
  64 ビットと読んで読み込みに失敗した（既存の不具合。`tests/bindings.mjs` に回帰テスト）。それぞれ、修正前のコンパイラで失敗することを確かめたテストを `tests/net_wasm.mjs` などに足した。
- 統合担当のレビュー（上の修正のコミット）で、その修正に不具合が 1 件（高）と軽微なものが 2 件見つかった（このあとのコミット）。(1) 非同期の UDP の送信（上の (5) の直し）が、やり直しの
  呼び出しを「同じ配列のポインタ」で見分けていたが、std はやり直しのたびに配列を複製するのでポインタは毎回違い、1 回の送信でデータグラムが 3〜6 個出たり、何も出ずに成功したり、
  `Async.all` で数百〜数千個になって終わらなかったりした。呼び出しが 1 回だけ `socket.send` を呼んで「受け取った」状態（`ACCEPTED` + 送信の番号）を返し、std の待ち
  （`__watch` の `events = 2 + 4 * 番号`）がその送信自身の結果で完了する形に直した（結果は呼び出しごとで、データグラムは送信の数だけ出る。ソケットの `close` は終えていない送信を
  終えてから閉じる）。(2) 相手が `accept` の前に RST で切った接続の `peer_addr` が `0.0.0.0:0` だった（接続が届いた時点でアドレスを記録する）。(3) `EHOSTDOWN` が `Unreachable` に
  ならなかった（`os.constants.errno` に無い libuv の名前を、libuv の表から補う）。`tests/net_wasm.mjs` に足したテストは、修正前のコンパイラで、`udp_counts`（1 回、blocking、12 回連続、
  `Async.all` の 50 通、終端）、`udp_echo`（50 の要求に 50 の返事）、`udp_close_race`（送信の直後と、送信と重なった `close`）、`udp_lines`（native の `udp` のケースと行ごとに一致）、
  `peer_after_reset`、`error_names` のすべてが、-O0 と -O3 の両方で失敗することを確かめた。1 つの instance が詰まっても次の case が巻き込まれないよう、これらは instance を別にしている。
  `Async.all` の同時の送信の数が増えると、1 件あたりの時間が増える（-O3 で 250〜500 件は 1 件 0.1 ms、1000 件は 0.3 ms、4000 件は 3 ms。結果の数は常に送信の数と同じ）。CPU プロファイルでは
  時間のほとんどが wasm のアロケーター（first-fit の空きリストの探索）にあり、グルーの JavaScript ではない。待っている操作の数に比例して実行器が確保と解放を繰り返すためで、E09 の外にある。

### 確かめていないこと

- **Linux の実行は確かめていない。** glibc と musl の x86_64・aarch64 に `-std=c11 -Wall -Wextra -Werror` で警告なしにコンパイルできることだけを `zig cc` で確かめた。Docker は起動できなかった。
- **Windows の実行は CI だけで検証される。** MSVC は x86_64 でリンク、aarch64 でコンパイル、MinGW は x86_64・aarch64 でコンパイルできることまで。Windows だけの分岐（`SIO_UDP_CONNRESET`、
  `accept` で飛ばす `WSAECONNRESET`）も、コンパイルと、POSIX の契約の検査（`tests/net_runtime.c`）までで、実際に Windows が上のように報告するかは CI の async の block が確かめる。
  macOS の CI がサニタイザーなしで動くことは、Homebrew の LLVM 21 とこの環境の macOS で確かめたが、GitHub のランナーそのものは試していない。
- wasm32 の Node.js ホストは macOS の Node.js 24 だけで実行した。Linux・Windows の Node.js、ブラウザーは未検証。`Async.start` の実行器からは `connect_async`・`read_async`・`accept_async`・
  `recv_from_async` を確かめた（`bind_async`・`bind_udp_async` は使えない）。
- 速さの合否の閾値は置かず、SIMD・並列・GPU の主張もしていない（ソケットの I/O は 1 回の `poll`／システムコールの往復で、配列の一括演算ではない）。

### 後続

- UDP の `connect`、socket option（`TCP_NODELAY`・`SO_KEEPALIVE`・broadcast・multicast）、Unix domain socket、ゾーン付き IPv6、TLS・HTTP（E10）、`resolve` の非同期版は対象外のまま。
- 届いていない完了が `cancel` を受けない B08 の挙動（`Async.all_results` が捨てた完了）は、B08 側で変えれば、成功した接続や accept の結果の閉じ忘れを防げる。
