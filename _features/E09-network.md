# E09: ネットワーク API

| 項目 | 内容 |
|---|---|
| ID | E09 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | E08, B08 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C#/F# 比: ネットワーク API がない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 主な影響ファイル | `std/Net.tz`（新規）, `src/runtime/`（新規 net ランタイム）, `src/driver.rs`, `docs/language.md`, `tests/net.mjs`（新規） |

## 目的

TCP／UDP ソケットを標準 API で扱い、将来の HTTP クライアントの土台を用意する。
.NET の `Socket`／`HttpClient`、Rust の `std::net` に相当する範囲を段階的に提供する。

## 現状

- 標準入出力以外の IO はない（E08 で OS API を計画）。非同期実行はない（B08 で計画）。
- extern で利用者がホストのネットワーク関数を呼ぶことはできるが、同期呼び出しだけで型付きのハンドルはない（E12 で計画）。

## 仕様

### Phase 1: 同期 TCP／UDP（native、実装対象）

- `Net.TcpStream`（B07 の Drop で close）: `connect`、`read`、`write`、`shutdown`、読み書きのタイムアウト。
- `Net.TcpListener`: `bind`、`accept`、`local_addr`。`Net.UdpSocket`: `bind`、`send_to`、`recv_from`。
- アドレスは `Net.Address`（IPv4/IPv6 とポート）。名前解決 `Net.resolve` は OS の resolver を使う。
- 全操作は `IO<Result<T, Os.Error>>`。WASM では Phase 1 は未対応で、到達すれば `E2000`（黙って失敗値にしない）。

### Phase 2 以降（設計方針）

- B08 の reactor と接続した非同期ソケット。
- HTTP/1.1 クライアントと TLS。TLS は OS の実装（Schannel／Security.framework）または外部ライブラリを使い、証明書検証を既定で有効にする。
- WASM は WASI preview2 の sockets、ブラウザーは E13 の glue で fetch へ接続する。

## 設計

- C ランタイムに BSD sockets／Winsock の薄い層を置き、E08 と同じ連結条件・エラー変換を使う。
- ソケットのハンドルは所有値で、並列 Task から IO を実行できない既存の制約を維持する。

## 実装手順

1. **アドレスと解決**: 確認: IPv4/IPv6 の解析と表示の往復。
2. **TCP**: ループバックでの echo。確認: `tests/net.mjs`（外部ネットワークに接続しない）。
3. **UDP**: ループバックでの送受信。
4. **Phase 2 の設計レビュー**: 非同期・TLS・HTTP の範囲と配置（std か公式パッケージか）。

## テスト計画

- E2E: native × `-O0`/`-O3`、ASan、タイムアウト、接続拒否、close の一回性。テストはループバックだけを使う。

## ドキュメント

- `docs/language.md`、`_docs/library-reference/`（新規ページ）、GUIDE D-07。

## 受け入れ条件

- [ ] native で TCP／UDP の同期通信ができ、エラーを Result で返す。
- [ ] WASM での未対応を明示的な診断で示す。

## 落とし穴

- `SIGPIPE` でプロセスが終了しないよう、送信時のフラグ・ソケットオプションを設定する。
- 受信バッファの上限を設けずに読み込むとメモリを使い切る。

## 対象外

- HTTP サーバーフレームワーク、WebSocket、QUIC。

## 未決事項

- **HTTP／TLS の配置**: 既定案は std を TCP/UDP までとし、HTTP/TLS は公式パッケージ（E10）にする。
