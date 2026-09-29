# B08: 非同期計算（Async）とホスト駆動の実行

| 項目 | 内容 |
|---|---|
| ID | B08 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | B05, B07, (E08), (E13) |
| 後続 | E09 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C#/F# 比: `async`／`await` がない、Rust 比: 非同期 I/O と `Task` の違い（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 主な影響ファイル | `std/Async.tc`（新規）, `src/computation.rs`, `src/closures.rs`, `src/llvm.rs`, `src/runtime/`（新規 async runtime）, `docs/language.md`, `tests/async.mjs`（新規） |

## 目的

I/O 待ちでスレッドをブロックせずに計算を合成できるようにする。C# の `async`／`await`、F# の `async`／`task`、Rust の `Future` に相当する。
ブラウザーの Promise、UI スレッド、ホストのイベントループと Tsuzuri の計算をつなぐ入口を用意する。

## 現状

- `Task<T>` は cold・一回実行の同期計算で、`Task.run` は完了までブロックする。
  「ホストの非同期I/O／イベントループとの連携はこの機能には含みません」「`Task.run` は…UI スレッドをノンブロッキングにする API ではありません」（docs/language.md タスク）。
- 計算式は `.tc` の利用者ビルダーで定義でき、B05 で `match!`・`and!`・`BindReturn`・`Bind2` がある。`IO<T>` は std の `unit -> T` 遅延関数（`std/IO.tc`）。
- `extern` は同期呼び出しだけで、ホストは非同期に保持・再開しない（docs/language.md ホスト関数のインポート）。

## 仕様

### Phase 1: Async 値とホスト駆動の実行器（実装対象）

- std に不透明・非 Copy・一回消費の `Async<'a>` と `Async { let! x = ...; return ... }` ビルダーを追加する。
- 表現は stackless な継続渡し: 各 `let!` の続きを所有 closure として保持する。既存の計算式展開と closure lowering で表せる範囲を先に確認する。
- 中断点はホスト操作だけ: `Async.host :: i64 -> Async<i64>` のような opt-in の低水準 primitive（操作 ID と結果の受け渡し）を std 専用に置き、
  ホストは `tsuzuri_async_complete(operation, value)` と `tsuzuri_async_poll()` で再開する。
- 実行: `Async.start :: Async<unit> -> unit` は最初の中断点まで同期実行して戻る。完了・再開はホストの呼び出しで進む。
- キャンセル: 未完了の `Async` の drop は継続を解放し、ホストへ取り消しを通知する（B07 の drop を使う）。
- WASM の import は `Async` を使うプログラムだけに追加し、D-18 の opt-in 規則に従う。native はホストのイベントループから呼び出す。
- CPU 並列は従来どおり `Task`／`Parallel` を使い、`Async` と混同しない。

### Phase 2 以降（設計方針）

- タイマー・ソケット（E08/E09）用の native reactor（kqueue/epoll/IOCP）。
- WASM の JavaScript Promise Integration（JSPI）による Promise との直接接続、E13 の生成 glue での Promise 化。

## 設計

- `std/Async.tc` のビルダーは `Bind`/`Return`/`ReturnFrom`/`Delay`/`Run` を実装し、継続は `Capture` を満たす所有 closure とする。
- 中断中の継続は runtime の表（操作 ID → 継続）に所有される。表の操作は決定的な ID 採番で行う。
- 捕捉値の寿命: 中断を跨いで借用を保持できない（`Send` と同様に参照を拒否する）。
- 例外・巻き戻しは使わない。失敗は `Async<Result<...>>` で表す（D-10）。

## 実装手順

1. **純粋なビルダー**: 中断点なしの `Async` を既存 closure で実装。確認: `tests/computations.rs` 相当のビルダー展開テスト。
2. **ホスト primitive と表**: 中断・再開・二重完了の拒否。確認: Node ホストで Promise から再開する E2E。
3. **キャンセル**: drop による継続解放と取り消し通知。確認: 確保追跡 `live == 0`。
4. **native ホスト例**: C のイベントループ例。確認: ASan。

## テスト計画

- Rust: ビルダー展開、捕捉規則（借用の拒否 `E1013`）、`Async` の Copy 拒否。
- E2E: native/WASM × `-O0`/`-O3`、再開順序の決定性、二重完了・未知 ID のトラップ、確保追跡。
- 性能: 中断点あたりの確保回数と時間を計測してから主張する。

## ドキュメント

- `docs/language.md` のタスク・計算式、`_docs/language-reference/tasks.md`、`_docs/guides/webassembly.md`、`_docs/guides/from-fsharp.md`。

## 受け入れ条件

- [ ] `Async` をホストの完了通知で再開でき、キャンセルで資源を回収できる。
- [ ] `Async` を使わないプログラムの IR・import が変わらない。

## 落とし穴

- 継続の closure が中断を跨いで大きな環境を複製しないよう、所有 move を徹底する（A15 の警告と併用）。
- ホストが再入的に `poll` を呼ぶ場合の再入禁止・トラップ条件を定義する。

## 対象外

- stackful coroutine、OS スレッドを使う async executor、自動的な並列実行。

## 未決事項

- **継続の方式**: 既定案は stackless。stackful（別スタック）は WASM と所有権の検査が難しい。
- **中断点ごとの確保**: 既定案は closure 1 個の確保を許容し、実測後に最適化する。
