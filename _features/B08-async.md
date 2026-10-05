# B08: 非同期計算（Async）とホスト駆動の実行

| 項目 | 内容 |
| --- | --- |
| ID | B08 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | B05, B07, (E08), (E13) |
| 後続 | E09 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（std モジュール名 `Async` と API 面。GUIDE D-30 の仮割り当て）, D10（Phase 2 のホスト再開: opt-in フラグ・runtime シンボル・WASM import） |
| 改善する劣位 | C#/F# 比: `async`／`await` がない、Rust 比: 非同期 I/O と `Task` の違い（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | closure で表す std ビルダー: `std/IO.tc` の `record IO<'a>` と `Bind`・`Delay`・`Combine`・`For`・`While`。std 登録: `src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`opaque_record`。非 Copy の std レコード: `src/check.rs` の `Type::is_noncopy_record`（`Seq.Seq`）。借用を持つ環境の拒否: `src/ownership.rs` の `eval_value` 内 `E::Closure` 節（`Type::Task` の closure だけ loan を拒否）。Rust テスト: `tests/computations.rs` の `accepts`・`rejects`。E2E: `tests/features.mjs` の suite `computation_extensions` |
| 主な影響ファイル | Phase 1: `std/Async.tc`（新規）, `src/stdlib.rs`, `src/check.rs`, `src/ownership.rs`, `tests/async.rs`（新規）, `tests/fixtures/async/Main.tz`（新規）, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/computation-expressions.md`, `_docs/language-reference/tasks.md`, `_docs/library-reference/async.md`（新規）, `_docs/library-reference/README.md`, `_docs/feature-status.md`, `_features/README.md`, `README.md`。Phase 2（承認後）: `src/runtime/`, `src/main.rs`, `src/driver.rs`, `src/llvm.rs` |

## 目的

I/O 待ちでスレッドをブロックせずに計算を合成できるようにする。C# の `async`／`await`、F# の `async`、Rust の `Future` に相当する。
最終的には、ブラウザーの Promise、UI スレッド、ホストのイベントループと Tsuzuri の計算をつなぐ。

Phase 1 はホスト I/O を使わない土台だけを作る。std の `Async<'a>` とビルダー `Async { ... }`、中断点 `yield_now`・`sleep`（仮想時刻）、
決定的な協調スケジューリング（`Async.all`・`Async.all_results`）、実行器 `Async.run`、中断を跨ぐ借用の禁止を提供する。
Phase 2（ホストによる再開）と Phase 3（native reactor）は設計方針だけを書く。実装者は Phase 1 だけを実装し、Phase 2 以降は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- B05 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認: `grep -n "| B05 \|| B07 \|| B08 " _features/README.md`。
- B07（todo）は Phase 1 の着手条件にしない。Phase 1 の取り消しは closure の通常の drop で足りる（D8）。B07 は Phase 2 のホストへの取り消し通知で要る。
  E08・E13 は Phase 3・Phase 2 の条件で、Phase 1 には関係しない。依存欄の値は変えない。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `std/Async.tc` を既存の構文・所有権規則だけで書けない（例: `For` の反復で所有値を move できず E1012 になる、`Capture` 制約を満たせない）。
  Phase 1 では `src/computation.rs`・`src/llvm*.rs`・`src/runtime/` を変更しない（D2）。変更が要ると分かった時点で止める。
- 本体が同期的に完了する反復（`For`・`While`）を再帰なしで書けず、100 万回の反復が native か WASM の `-O0` で stack 枯渇する。
- D6 の検査を `Async` 型の式以外に広げないと実装できない、または既存の E1013 のメッセージ・位置を変える必要がある。
- 既存テストの期待値（IR・診断コード・メッセージ）を変える必要がある。`Async` を使わないプログラムの IR が 1 byte でも変わる。
- 暗黙本体（ビルダー名を省略した本体）が `Async` の結果型で選ばれない（`check_implicit` の変更が要る）。
- stack-depth の 3 テスト（GUIDE §3.1）か `honors_the_exact_specialization_limit` が失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import、`--wasm-feature` の新しい値が必要になった（Phase 2 は D10 の承認が前提）。

## 現状（HEAD `f8dc655` で確認）

- `Task<T>` は cold・一回実行の同期計算で、`Task.run` は完了までブロックする。
  「ホストの非同期I/O／イベントループとの連携はこの機能には含みません」「`Task.run` は…UI スレッドをノンブロッキングにする API ではありません」（docs/language.md の「タスク」）。
- `extern` は同期呼び出しだけで、ホストは非同期に保持・再開しない（docs/language.md の「ホスト関数のインポート」）。
  `--wasm-feature` の値は `simd128` と `threads` だけ（`src/main.rs`）。GUIDE D-18 により WASM の既定は import なし。
- 計算式の展開は `src/computation.rs`。`collect_all` が `.tc` の公開操作を集め（`OPERATIONS` に `Bind`・`Return`・`ReturnFrom`・`Yield`・`YieldFrom`・`Zero`・
  `Combine`・`Delay`・`Run`・`For`・`While`・`MergeSources`・`BindReturn`・`Bind2`・`Using`）、`lower` と `Lowering::block` が `let! x = s; rest` を
  `Builder.Bind(s, \x -> rest)` に、`do!` を unit 引数の継続に変える。継続は `Lowering::continuation` が作る `ExprKind::Lambda`、本体は
  `Lowering::delay` が `Delay(thunk)` で包み、`Run` があれば呼ぶ。未定義の操作は `Lowering::call` が E1018
  `computation builder '{}' does not define '{operation}'` を返す。ビルダー名は `.tc` のファイル名。
- `std/IO.tc` は `record IO<'a> { work: unit -> 'a }` の closure で遅延計算を表すビルダーで、`IO.IO` は `stdlib::opaque_record` にあり、
  利用者はフィールドを使えない（`src/check.rs` の `record_storage` が E1022 `the representation of '{}' is opaque; use its module API` を返す）。
- 匿名関数は外側の値を作成時に捕捉する。Copy 値はコピー、非 Copy 値は move。関数値は Copy で、複製は環境の独立したスナップショットを作る
  （docs/language.md の「匿名関数と捕捉」）。`Type::is_copy` は `Type::Function` を fallback で true にし、関数だけを持つレコードも Copy になる。
  例外は `Type::is_noncopy_record` の `Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`。
- 所有権検査（`src/ownership.rs`）では、関数値の loan は `eval_value` の `E::Closure` 節で結果へ伝わる。loan を拒否するのは closure の型が
  `Type::Task` のときだけ（E1013 `task captures cannot retain borrowed values, including borrowed function environments`）。
- std は `src/stdlib.rs` の `SOURCES` に `include_str!` で埋め込まれる。`RESERVED_MODULES` に `Async` はない。予約名の利用者ファイルは E1011
  `module name 'IO' is reserved for the standard library; rename the file`。`tests/` に `Async` という名前のモジュール・fixture はない。
- 使わない std 関数は IR に出ない（`export def add :: i64 -> i64` だけのプログラムの IR は `@tz.fn.Main.add`・`@tz.apply.Main.add.0`・`@tz_add` だけ）。
- `yield` は予約語（`src/lexer.rs` の `TokenKind::Yield`）。中断の primitive は `yield_now` と名付ける。
- `std/Async.tc`・`Async` の文書・テストはない。

### 再現（検証済み）

利用者の `.tc` として書いた試作ビルダーが、HEAD の release compiler で compile・実行できる（`/tmp/tz-work-B08/proto`・`runp`）。
これが D2（既存の計算式展開と closure で中断を表す）の根拠である。`Async` は HEAD では予約名ではないので利用者モジュールとして書ける。

```tsuzuri
union Step<'a> =
    | Done of 'a
    | Yield of (unit -> Step<'a>)
    | Sleep of i64 * (unit -> Step<'a>)
    | Now of (i64 -> Step<'a>)

record Async<'a> { start: unit -> Step<'a> }

private def rec bind_step :: Step<'a> -> ('a -> Step<'b>) -> Step<'b>
fn rec bind_step step next =
    match step with
    | Done value -> next value
    | Yield resume -> Yield (\() -> bind_step (resume ()) next)
    | Sleep (wake, resume) -> Sleep (wake, \() -> bind_step (resume ()) next)
    | Now resume -> Now (tick -> bind_step (resume tick) next)

def Bind :: Async<'a> -> ('a -> Async<'b>) -> Async<'b>
fn Bind source next = Async { start: \() -> bind_step (source.start ()) (value -> (next value).start ()) }

def run :: Async<'a> -> 'a
fn run computation =
    let mut current = Option.Some (computation.start ())
    let mut output = Option.None
    let mut clock = 0
    while Option.is_some (ref current) do
        match Option.get current with
        | Done value -> { output = Option.Some value; current = Option.None }
        | Yield resume -> current = Option.Some (resume ())
        | Sleep (wake, resume) -> { clock = if wake > clock then wake else clock; current = Option.Some (resume ()) }
        | Now resume -> current = Option.Some (resume clock)
    Option.get output
```

試作の残りは `Return`（`Capture<'a> =>`）、`ReturnFrom`、`Delay`、`Zero`、`Combine`（`bind_step (first.start ()) (\() -> rest.start ())`）、
`yield_now`、`now`、`sleep` で、`Async { let! a = Async.Return 4; do! Async.sleep 3; let! t = Async.now (); return a * 10 + t }` を
`Async.run` すると `43` を出力した。同じ試作で次も確かめた。

| 試したこと | HEAD の結果 |
| --- | --- |
| `for` を含む `Async { }` | E1018 `computation builder 'Async' does not define 'For'`（`For` 未定義のため） |
| 外側の配列を `let view = ref values` で借用し、`do!` の後で使う | E1013 `cannot return a reference to a local value`（`Async {` の位置） |
| `ref [i64]` を受け取り、それを `do!` の後で使う `Async` を返す関数 | 受理（loan が引数に結び付き、呼び出し側の寿命内で実行される） |
| ブロック内の所有値を `do!` の後で `ref` で使う、外側の所有値を `let owned = values` で move する | 受理 |
| `Step` を `private union` にする | E1022 `private type 'Async.Step' leaks from public record 'Async'; make the record private or expose a public type` |
| 関数だけを持つ `Async` レコードを 2 回使う | 受理（Copy。D5 で非 Copy にする） |

## 仕様

### 前提とする他チケットのインターフェース

- B05（done）: 「現状」の展開規則。`match!` は `Bind` だけで動く。`and!` は `MergeSources`（融合時は `Bind2`）を要求する。
- B06（done）: 取り消しは協調的で、先取りしない。未開始の計算は drop で解放する。`Task.parallel_results` は最小 index の失敗を返す。
  `Async.all_results` は同じ考え方を中断点単位に適用する（D8）。
- B07（todo）: Phase 2 でホスト操作の取り消し通知に使う。Phase 1 は使わない。
- E14（todo）: トラップ境界は外側の公開関数呼び出しに掛かる。`Async` は独自のトラップ処理を持たない（D13）。
- E09（todo）: 非同期ソケットは Phase 2 のホスト再開を使う。E09 Phase 1（同期 TCP／UDP）は B08 を必要としない。
- F10（todo）: 関係しない。`Async` は一つのスレッドの中で進む。

### API（Phase 1）

新 API（実装後に有効。未検証）。`std/Async.tc`（新規）の公開宣言の全体。

```tsuzuri
union Step<'a> =
    | Done of 'a
    | Yield of (unit -> Step<'a>)
    | Sleep of i64 * (unit -> Step<'a>)
    | Now of (i64 -> Step<'a>)

record Async<'a> { start: unit -> Step<'a> }

def Return :: Capture<'a> => 'a -> Async<'a>
def ReturnFrom :: Async<'a> -> Async<'a>
def Bind :: Async<'a> -> ('a -> Async<'b>) -> Async<'b>
def Delay :: (unit -> Async<'a>) -> Async<'a>
def Zero :: Async<unit>
def Combine :: Async<unit> -> Async<'a> -> Async<'a>
def For :: (Copy<'a>, Capture<'a>) => ['a] -> ('a -> Async<unit>) -> Async<unit>

def yield_now :: unit -> Async<unit>
def sleep :: i64 -> Async<unit>
def now :: unit -> Async<i64>
def all :: (Copy<'a>, Capture<'a>) => [Async<'a>] -> Async<['a]>
def all_results :: (Copy<'a>, Copy<'e>, Capture<'a>, Capture<'e>) => [Async<Result<'a, 'e>>] -> Async<Result<['a], 'e>>
def run :: Async<'a> -> 'a
```

- `Async<'a>` は cold。作っただけでは何も実行しない。`run` が最初の step を始める。
- `run` は完了まで同期的に駆動して結果を返す。仮想時刻は `run` の呼び出しごとに 0 から始まる。step の中で別の `run` を呼ぶと、
  その内側の計算を独立した時刻で完了まで実行する（外側から見れば普通の関数呼び出し）。
- `yield_now ()` は中断点を一つ作る。時刻は進まない。
- `sleep n` は `n <= 0` なら `yield_now ()` と同じ。`n > 0` なら時刻が `now + n` 以上になるまで中断する（`now + n` は `9223372036854775807` で飽和）。
- `now ()` は現在の仮想時刻を返す。中断点ではない。
- 時刻が進むのは、`run` の中のすべての計算が `sleep` 中のときだけで、最小の起床時刻まで一度に進む（離散事象の模擬。D4）。
  `yield_now` を繰り返す計算がある間、`sleep` 中の計算は起きない。
- `all xs` は子を index 順に開始し、以後は round ごとに実行可能な子を index 順に 1 step ずつ進める。結果は入力の index 順。
  空配列は中断せずに `[]` を返す。
- `all_results xs` は `all` と同じ順序で進め、`Error e` で完了した最初の子（round 順、同じ round では index 順）で直ちに `Error e` を返す。
  残りの子は現在の中断点のまま drop し、それ以上実行しない。すべて `Ok` なら `Ok` の配列を index 順で返す。
- `Step` は `Async` の内部表現で、公開は E1022 の漏れ規則のため（D3）。利用者は `Step` を構築・網羅 match しない。

### 構文

構文の追加はない。`Async { ... }` は既存の計算式で、ビルダー名 `Async` は std の `.tc` から来る。

```text
async-block  = "Async" "{" statement* "}"
statement    = "let!" pattern "=" expr | "do!" expr | "return" expr | "return!" expr
             | "match!" expr "with" arms | "let" binding | "if" expr "then" block ["else" block]
             | "for" pattern "in" expr "do" block | expr
unsupported  = "and!" | "yield" | "yield!" | "while" | "use"      (* E1018 または B05 の既存の拒否 *)
```

結果型が `Async<T>` と宣言された関数は、既存の暗黙本体（`check_implicit`）で `Async { }` を省略できる。

### 型規則

- `Async.Async<'a>` は不透明（`stdlib::opaque_record`）で Copy ではない（`Type::is_noncopy_record`）。std の外で `start` を読む、
  `Async { start: ... }` を書くと E1022。同じ値を 2 回使うと E1012。
- `Async.Step<'a>` は `'a` が Copy なら Copy（関数値は Copy）。`all`・`all_results` はこの性質を使うので結果型に `Copy` を要求する（D7）。
- `Async` の値は loan を持たない（D6）。したがって既存の `Capture`・`Task` の捕捉規則は、関数だけを持つ普通のレコードとして判定する。
  `Task` の中で `Async.run` を呼べる。`Async` 専用の `Send` 規則や実行スレッドは作らない（D9）。

### 評価順序・所有権・借用

- step は次の中断点（`yield_now`、`sleep`）か完了まで同期的に進む。`let!` の右辺は step がその文に達したときに評価する。
- D6: 型が `Async<T>` の式の値が loan を持てば E1013。どの関数（利用者・std・生成コード）でも同じ。次をすべて拒否する。
  - `let r = ref x` の後に `let!`／`do!` があり、その後で `r` を使う（継続が `r` を捕捉する）。
  - `Async { }` の中で外側の値を借用する（cold な開始も中断点とみなす）。
  - 借用引数を使う `Async` を返す関数（HEAD では受理される。「再現」の表）。
  - `Async.Return (ref x)` のように借用を値として渡す。
- 受理する: 次の中断点より前に終わる借用（`let n = Array.length (ref values)` の後の `do!`）、継続への所有値の move、Copy 値。
- 未完了の `Async`・`Step` を drop すると、継続 closure の環境と中の所有値を解放する（`all_results` の取り消し、使わずに捨てた `Async`）。
- closure の捕捉はスナップショットなので、継続を呼ぶたびに環境の複製が起こりうる（D11、落とし穴）。

### 数値・トラップ・native と WASM の差

- 仮想時刻は `i64`、0 から単調に増え、加算は飽和する。整数の折り返しやトラップは起こさない。
- step の中のトラップは従来どおりプログラム全体のトラップになる。巻き戻し・隔離はしない（GUIDE D-10、D13）。新しい `TrapKind` はない。
- stack: 一回の再開が使う stack は、実行中の非末尾 `let!` の入れ子の深さ（async 関数の再帰の深さ）に比例する。同期の非末尾再帰と同じ扱いで、
  docs/language.md の「再帰とスタック」に従う。`run`・`all`・`all_results`・`For` の反復は loop で書き、反復回数で stack を使わない。
- native と WASM で結果・順序は同じ。WASM の import は増えない。runtime シンボル・スレッドを使わない。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1013（新メッセージ） | 型が `Async<T>` の式の値が loan を持つ | `async computations cannot keep borrowed values across 'let!', 'do!', or the start of the computation; move or clone the value into the async block instead` | その式の span。内側から評価するので、多くは `let!`／`do!` の文、`Async { ... }` 全体、または借用を受け取る std 関数の呼び出し |
| E1012（既存） | `Async` の値を move 後に使う | `use of moved or partially moved value '{name}'` | 2 回目の使用 |
| E1022（既存） | std の外で `Async` のフィールド・構築を使う | `the representation of 'Async' is opaque; use its module API` | フィールド・レコード式 |
| E1018（既存） | `and!`・`yield`・`while` など未定義の操作 | `computation builder 'Async' does not define '<operation>'` | 文 |
| E1011（既存） | 利用者のモジュール名が `Async` | `module name 'Async' is reserved for the standard library; rename the file` | ファイル先頭 |

### 資源上限

コンパイラの上限は増やさない。計算式の展開は既存の `bounded_depth` に従う。実行時の上限はない。`all` は子の数に比例して確保する。
仮想時刻は飽和する。

### 例

新 API（実装後に有効。未検証）。二つの worker を `all` で並べる。独立に計算した期待値は `123024`。

```tsuzuri
def rec worker :: i64 -> i64 -> i64 -> Async<i64>
fn rec worker delay count trace = Async {
    do! Async.sleep delay
    let! tick = Async.now ()
    if count == 1 then return trace * 10 + tick
    else return! worker delay (count - 1) (trace * 10 + tick)
}

export def timeline :: i64 -> i64
fn timeline _unused =
    let results = Async.run (Async.all [worker 1 3 0, worker 2 2 0])
    results[0] * 1000 + results[1]
```

時刻は 0 → 1 → 2 → 3 → 4 と進む。worker 1 は時刻 1・2・3 で起き、worker 2 は 2・4 で起きる。記録は 123 と 24 で、`123 * 1000 + 24 = 123024`。
配列リテラルはカンマ区切り（`[1; 2]` は HEAD で E0002）。

新 API（実装後に有効。未検証）。`do!` を跨ぐ借用は E1013。

```tsuzuri
export def borrowed :: i64 -> i64
fn borrowed n =
    let values = Array.init 3 (index -> index)
    Async.run (Async {
        let view = ref values
        do! Async.yield_now ()
        return Array.length view + n
    })
```

### Phase 2 以降（設計方針。D10 の承認まで着手しない）

- Phase 2（ホスト再開）: `Step` に `Host of i64 * (i64 -> Step<'a>)` を足し、std 専用の `Async.host :: i64 -> Async<i64>` で操作を始める。
  `Async.start :: Async<unit> -> IO<unit>` は最初の中断点まで進めて戻り、中断中の継続は runtime の表（操作 ID → 継続）が所有する。
  ホストは `tsuzuri_async_complete(operation, value)` と `tsuzuri_async_poll()` で再開する。二重完了・未知 ID はトラップ。
  再入的な `poll` はトラップ。未完了の drop は B07 の `Drop` でホストへ取り消しを通知する。
  すべて明示的な opt-in（CLI の値は `E2xxx`／フラグ名とも要割り当て）で、未指定時の IR と WASM import は変えない（GUIDE D-18）。
  `run` が `Host` に出会ったらトラップする。
- Phase 3: タイマー・ソケット（E08／E09）用の native reactor（kqueue／epoll／IOCP）、WASM の JSPI、E13 の生成 glue での Promise 化。

## 設計

### データ構造

Phase 1 はコンパイラの型・構文・IR を増やさない。変更は std の登録 3 か所、型の性質 1 か所、所有権の検査 1 か所。

```rust
// src/stdlib.rs
pub const SOURCES: &[(&str, &str)] = &[
    ("std/Array.tz", include_str!("../std/Array.tz")),
    ("std/Async.tc", include_str!("../std/Async.tc")), // 新規。名前順を保つ
    // ...
];
pub const RESERVED_MODULES: &[&str] = &[/* ... */ "IO", "Async"]; // "Async" を追加
// opaque_record の matches! に "Async.Async" を追加

// src/check.rs（impl Type）
pub(crate) fn is_async(&self, types: &TypeContext<'_>) -> bool { // 新規
    matches!(self, Self::Record(id, _)
        if types.records[*id].origin == ModuleOrigin::Std && types.records[*id].name == "Async.Async")
}
// is_noncopy_record の名前の列に "Async.Async" を追加

// src/ownership.rs（impl Checker）
const ASYNC_BORROW: &str = "async computations cannot keep borrowed values across 'let!', 'do!', or the start of the computation; move or clone the value into the async block instead"; // 新規
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/Async.tc`（新規） | `Step`, `Async`, 公開関数、非公開の `bind_step`・`settle`・`for_from`・`round` | 「API」と「アルゴリズム」。操作は `Return`・`ReturnFrom`・`Bind`・`Delay`・`Zero`・`Combine`・`For` だけ（D12） |
| std 登録 | `src/stdlib.rs` | `SOURCES` | `("std/Async.tc", include_str!("../std/Async.tc"))` を `std/Array.tz` の次に置く |
| std 登録 | `src/stdlib.rs` | `RESERVED_MODULES` | `"Async"` を追加 |
| std 登録 | `src/stdlib.rs` | `opaque_record` | `"Async.Async"` を追加 |
| std 登録 | `src/stdlib.rs` | `tests::embedded_sources_use_reserved_flat_modules` | 変更なし（新しい source も予約名であることをこの既存テストが確かめる） |
| 検査 | `src/check.rs` | `Type::is_noncopy_record` | 名前の列に `"Async.Async"` |
| 検査 | `src/check.rs` | `Type::is_async`（新規） | 上の定義 |
| 所有権 | `src/ownership.rs` | `Checker::eval` | 振り分けの結果を変数に受け、`!value.loans.is_empty() && expression.ty.is_async(&self.module.types())` なら `error("E1013", ASYNC_BORROW, expression.span)`。他の式は変えない |
| 展開 | `src/computation.rs` | `OPERATIONS`, `Lowering` | 変更なし |
| 型付け | `src/check.rs` | `check_implicit` の呼び出し元 | 変更なし（暗黙本体は既存の規則で選ばれる。選ばれなければ停止） |
| LLVM・runtime | `src/llvm*.rs`, `src/runtime/` | なし | 変更なし（Phase 1） |
| 整形・LSP・文書生成 | `src/formatter.rs`, `src/semantic.rs`, `src/docgen.rs` | なし | 変更なし（新しい構文がない） |

### 生成 IR とランタイム

新しい IR 形・`%tz.*` 型・runtime シンボルはない。`Async` は既存の union・レコード・closure の lowering（`make_closure` など）で下がる。
`Async` を使わないプログラムは std の未使用関数を出さないので IR が変わらない（手順 3 で byte 比較する）。WASM import は空のまま。

### アルゴリズム

`std/Async.tc` の非公開 helper。`settle` 以外は試作（「再現」）と同じ形で書く。反復はすべて `while` と `let mut` で書き、再帰しない。

```text
bind_step(step, next) -> Step<'b>:                      # 試作と同じ。def rec
    Done v          -> next v
    Yield k         -> Yield(\() -> bind_step(k (), next))
    Sleep(w, k)     -> Sleep(w, \() -> bind_step(k (), next))
    Now k           -> Now(\t -> bind_step(k t, next))

settle(step, tick) -> Step:                             # Now を即答する。loop
    while step is Now k: step = k tick
    return step                                         # Done・Yield・Sleep のどれか

run(a):                                                 # 試作の run と同じ
    clock = 0; step = a.start ()
    loop: Done v -> return v | Now k -> step = k clock | Yield k -> step = k ()
          Sleep(w, k) -> clock = max(clock, w); step = k ()

sleep(n) = Async{ \() -> Now(\t -> if n <= 0 then Yield(\() -> Done ())
                                   else Sleep(saturating(t + n), \() -> Done ())) }

for_from(values, index, body) -> Step<unit>:            # def rec。同期完了の反復は loop
    i = index
    while i < values.length:
        s = (body values[i]).start ()
        if s is Done (): i = i + 1
        else: return bind_step(s, \() -> for_from(values, i + 1, body))
    return Done ()

all(children) = Async{ \() -> Now(\t -> round(first = true, children の start, t)) }
round(states, tick):                                    # states: [Step<'a>]（'a は Copy）
    for i in 0 .. n-1 (index 順):
        first round: states[i] = settle(children[i].start (), tick)
        Yield k                -> states[i] = settle(k (), tick)
        Sleep(w, k), w <= tick -> states[i] = settle(k (), tick)
        それ以外               -> そのまま
        (all_results: states[i] が Done (Error e) なら直ちに Done (Error e) を返す。states は drop)
    すべて Done  -> Done(値の配列。index 順)
    Yield がある -> Yield(\() -> Now(\t -> round(states, t)))
    それ以外     -> Sleep(Sleep 中の最小の w, \() -> Now(\t -> round(states, t)))
```

`states` の更新は `Array.set`（消費する更新。HEAD で受理を確認済み）か `new [Step<'a>](n, index -> ...)` で行う。
非 Copy の要素は配列から move で取り出せない（HEAD の E1012 `cannot move a non-Copy element out of an array or list; borrow the element instead`）ため、
`all` の結果型に `Copy` を要求する（D7）。子の `start` はフィールド呼び出しなので `children` を借用するだけでよい。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。作業用のプロジェクトは `/tmp/tz-b08/` の下に一つずつ置く。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 を実行する。`/tmp/tz-b08/plain/Main.tz` に `export def add :: i64 -> i64` と `fn add n = n + 1` を置き、IR を保存する。
- 確認: 次がすべて成功し、4 つのテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
for o in 0 3; do
  target/release/tsuzuri build /tmp/tz-b08/plain --emit llvm -O$o -o /tmp/tz-b08/plain-before-O$o.ll
  target/release/tsuzuri build /tmp/tz-b08/plain --target wasm32 --emit llvm -O$o -o /tmp/tz-b08/plain-wasm-before-O$o.ll
  target/release/tsuzuri build tests/fixtures/computations --emit llvm -O$o -o /tmp/tz-b08/comp-before-O$o.ll
done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: `std/Async.tc` の骨格と登録

- 変更: `std/Async.tc`（新規）, `src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`opaque_record`, `tests/async.rs`（新規）。
- 内容: `Step`、`Async`、`bind_step`、`settle`、`Return`、`ReturnFrom`、`Bind`、`Delay`、`Zero`、`Combine`、`yield_now`、`now`、`sleep`、`run` を
  「再現」の試作と「アルゴリズム」のとおりに書く。公開関数には `///` の説明を付ける。テストファイルには `tests/computations.rs` の
  `accepts`（IR を 2 回出して一致、`llvm::emit_target(.., true)` も成功）と `rejects` を、`analyze_modules(&[("Main.tz", source)])` で写す。
  最初の 2 テストは `reserves_the_async_module_name` と `lowers_async_blocks_deterministically_for_both_targets`。
- 確認: `cargo test --locked --test async` が `2 passed`。`cargo test --locked --lib stdlib` が成功。`cargo test --locked` が成功。

### 手順 3: 既存の IR が変わらないこと

- 変更: なし（検査だけ）。
- 内容: release を作り直し、手順 1 と同じ 6 個の IR を `-after` の名前で出して比べる。
- 確認: `cargo build --release --locked` の後、`cmp /tmp/tz-b08/plain-before-O0.ll /tmp/tz-b08/plain-after-O0.ll` など 6 組すべてが出力なしで成功する。
  差があれば停止条件。

### 手順 4: 不透明で非 Copy

- 変更: `src/check.rs` の `Type::is_noncopy_record`、`tests/async.rs`。
- 内容: 名前の列に `"Async.Async"` を足す。テスト `async_values_are_opaque_and_not_copy` を足す。
- 確認: `cargo test --locked --test async` が `3 passed`。`cargo test --locked` が成功。

### 手順 5: 中断を跨ぐ借用の拒否（D6）

- 変更: `src/check.rs` の `Type::is_async`（新規）、`src/ownership.rs` の `Checker::eval` と `ASYNC_BORROW`（新規）、`tests/async.rs`。
- 内容: 「段ごとの変更」の所有権の行。`eval_value` の `E::Closure` 節や他の検査は変えない。テスト `rejects_borrows_across_suspension_points` と
  `accepts_borrows_that_end_before_suspension_and_moved_values` を足す。
- 確認: `cargo test --locked --test async` が `5 passed`。`cargo test --locked --test tasks` と `cargo test --locked --test computations` が成功し、
  既存の E1013 のメッセージは変わらない。`cargo test --locked` が成功。

### 手順 6: `For`

- 変更: `std/Async.tc` の `For` と非公開の `for_from`、`tests/async.rs`。
- 内容: 「アルゴリズム」の `for_from`。同期に完了する反復は `while` で回し、中断したときだけ継続を作る。
  `ponytail:` コメントで「中断ごとに配列の複製が起こる。要素を move できる API が入ったら見直す」と書く。テスト `for_loops_accept_arrays_of_copy_values` を足す。
- 確認: `cargo test --locked --test async` が `6 passed`。`cargo test --locked` が成功。

### 手順 7: `all` と `all_results`

- 変更: `std/Async.tc` の `all`・`all_results` と非公開の `round`（結果の種類ごとに分けてよい）、`tests/async.rs`。
- 内容: 「アルゴリズム」の `round`。`all_results` は `Error` を見た時点で残りの `states` を `settle` せずに手放す。テスト `all_requires_copy_results` を足す。
- 確認: `cargo test --locked --test async` が `7 passed`。`cargo test --locked` が成功。

### 手順 8: 未対応の操作と暗黙本体

- 変更: `tests/async.rs`。
- 内容: テスト `unsupported_operations_report_missing_builder_operations` と `implicit_async_bodies_select_the_async_builder` を足す。
  暗黙本体が受理されなければ停止条件（`check_implicit` を変えない）。
- 確認: `cargo test --locked --test async` が `9 passed`。

### 手順 9: E2E suite `async`

- 変更: `tests/fixtures/async/Main.tz`（新規）, `tests/features.mjs` の `suites`。
- 内容: 「E2E」の表の公開関数を書き、suite `async` に `cases` と `traps` を登録する（GUIDE §7.4）。期待値は表の手計算を写す。
- 確認: 次が終了コード 0。native と WASM の `-O0`・`-O3`、各呼び出し後の `live == 0`、WASM import が空、トラップを harness が確かめる。

```sh
cargo build --release --locked && node tests/features.mjs target/release/tsuzuri async
TSUZURI_ASAN=1 node tests/features.mjs target/release/tsuzuri async
```

### 手順 10: 中断点のコストを記録する（D11）

- 変更: なし（記録は手順 11 の文書へ）。
- 内容: fixture を `-O3` で IR にし、`bind_step` を含む関数（`grep -n "define .*bind_step"`）の本体にある `@tz.alloc` の呼び出しを数える。
  `tail_loop 1000000` を `main` から呼ぶ別プロジェクト `/tmp/tz-b08/perf` を `target/release/tsuzuri build /tmp/tz-b08/perf -O3 -o /tmp/tz-b08/perf.bin` で作り、
  `/usr/bin/time -l /tmp/tz-b08/perf.bin` を 9 回測って中央値と最大 RSS を記録する。
- 確認: 数値が記録されている。合否の閾値は設けない。計測していない数値を文書に書かない。

### 手順 11: 文書

- 変更: 「ドキュメント」のファイル。
- 内容: 「ドキュメント」のとおり。API 参照は `target/release/tsuzuri doc std -o /tmp/tz-b08/std-api` で生成した `Async.md` を写す。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/language-reference/computation-expressions.md _docs/language-reference/tasks.md _docs/library-reference/async.md _docs/library-reference/README.md _docs/library-reference/api/Async.md _docs/feature-status.md` が成功。

### 手順 12: 最終確認

- 変更: なし。
- 内容: GUIDE §10 の完了の定義に従う。
- 確認: `cargo test --locked` が成功。手順 1 の stack-depth 3 テストと `honors_the_exact_specialization_limit` が成功。
  `node tests/features.mjs target/release/tsuzuri`（全 suite）と `node tests/computations.mjs target/release/tsuzuri`、`node tests/tasks.mjs target/release/tsuzuri` が成功。
  `git diff --check` が出力なし。

## テスト計画

### Rust テスト

`tests/async.rs`（新規。Cargo は `async` という名前のテスト target を受け付けることを小さな crate で確認済み）。拒否はコードとメッセージ全文を assert する。

| テスト | 内容 |
| --- | --- |
| `reserves_the_async_module_name` | 利用者の `Async.tz` が E1011 `module name 'Async' is reserved for the standard library; rename the file`。`Async.run (Async { return 1 })` は受理 |
| `lowers_async_blocks_deterministically_for_both_targets` | `let!`・`do!`・`return`・`return!`・`match!`・`if`（else なしを含む）・`for`・`all`・`all_results` を使う源を `accepts`（手順 6・7 で源を増やす） |
| `async_values_are_opaque_and_not_copy` | `let a = Async.yield_now (); let b = a; Async.run a` が E1012。`(Async.now ()).start ()` と `Async { start: ... }` が E1022 の opaque メッセージ |
| `rejects_borrows_across_suspension_points` | 次の 4 つが E1013 と `ASYNC_BORROW`: 「例」の `borrowed`、外側の値をブロック内で `ref` するだけの `Async { return Array.length (ref values) }`、`ref [i64]` 引数を `do!` の後で使う `Async` を返す関数、`Async.Return (ref values)` |
| `accepts_borrows_that_end_before_suspension_and_moved_values` | 「再現」の受理 3 例（`moved`・`within`・`outer_moved`） |
| `for_loops_accept_arrays_of_copy_values` | `for i in values do do! Async.yield_now ()` を受理。`string` の配列は E1005 `no instance for Copy<string>; define an instance or use a supported type` |
| `all_requires_copy_results` | `Async.all [Async.Return "a"]` が E1005 の同じ形のメッセージ |
| `unsupported_operations_report_missing_builder_operations` | `and!`・`while`・`yield` が E1018。メッセージは `computation builder 'Async' does not define '` で始まる |
| `implicit_async_bodies_select_the_async_builder` | `def worker :: i64 -> Async<i64>` と、`Async { }` を書かない本体（`do! Async.yield_now ()` と `return n`）を受理 |

### E2E

`tests/fixtures/async/Main.tz`（新規）と `tests/features.mjs` の suite `async`。すべて `i64 -> i64` の公開関数で、結果の配列や `Result` は
表の式で 1 つの整数にする（`Error e` は `-e`）。期待値は仮想時刻の規則から手で計算したもので、実装の出力から作らない。

| 公開関数 | 引数 | 期待値 | 導出 |
| --- | --- | --- | --- |
| `basic` | 0, 4, -5 | 3, 43, -47 | `let! a = Return n; do! yield_now (); do! sleep 3; let! t = now ()` → `a * 10 + t` |
| `sleep_zero` | 0 | 0 | `sleep 0` と `sleep -5` の後の `now ()` |
| `saturated` | 0 | 9223372036854775807 | `sleep 9223372036854775807` を 2 回。2 回目の起床時刻は飽和 |
| `timeline` | 0 | 123024 | 「例」 |
| `starvation` | 0 | 1 | `all [spin, sleeper]`。spin は `yield_now` 3 回の後 `now` = 0、sleeper は `sleep 1` の後 1。`0 * 10 + 1` |
| `nested_all` | 0 | 120302 | `all [all [worker 1 2 0, worker 3 1 0], all [worker 2 1 0]]` = `[[12, 3], [2]]`。`12 * 10000 + 3 * 100 + 2` |
| `all_empty` | 0 | 0 | 型注釈付きの空配列を `all` した結果の長さ |
| `results_ok` | 0 | 12 | `all_results [sleep 1 の後 Ok 1, Ok 2]` = `Ok [1, 2]`。`1 * 10 + 2` |
| `results_first_error` | 0 | -1 | index 0 と 1 が時刻 2 で `Error 1`・`Error 2`、index 2 は `sleep 9`。同じ round では index 順で 0 が勝つ |
| `results_time_order` | 0 | -2 | index 0 は `sleep 3` の後 `Error 1`、index 1 は `sleep 1` の後 `Error 2`。時刻 1 で index 1 が先 |
| `cancel_frees` | 0 | -5 | `Array.init 100` を所有して `sleep 10` する子を、時刻 1 の `Error 5` で取り消す。`live == 0` が解放を証明する |
| `for_suspending` | 0, 1, 1000 | 0, 1, 1000 | 要素ごとに `yield_now`。ループ後に要素数を返す |
| `for_sync` | 100000 | 100000 | 本体は `do! Async.Return ()`。再帰すると `-O0` で stack が尽きる規模 |
| `tail_loop` | 100000 | 100000 | `yield_now` して `return! loop (i + 1) n`。stack が増えない |
| `deep` | 1000 | 1000 | `let! r = deep (n - 1); do! yield_now (); return r + 1`（非末尾） |
| `nested_run` | 0 | 25 | 外側で `sleep 5`、step 内の `Async.run` は `sleep 2` の後 `now` = 2、外側の `now` = 5。`2 * 10 + 5` |
| `unused` | 7 | 7 | `Async.sleep 5` を作って使わずに捨てる |
| `divide_after_yield` | 2 | 5 | `yield_now` の後 `10 / n`。`traps` に `["divide_after_yield", [0n]]` |

### 既存テストへの影響

なし。`src/stdlib.rs` の既存テストは新しい source も満たす。`Async` を使わないプログラムの IR は手順 3 で byte 単位に同じ。

### 性能

手順 10 の記録だけ。閾値は設けない。最適化（継続の複製の省略など）は PM02・PM07 の範囲で、このチケットでは行わない。

## ドキュメント

- `docs/language.md`: `## タスク` の後に `## 非同期計算（Async）` を足す（API、仮想時刻、順序、取り消し、借用規則、Task との違い、Phase 2 が未実装であること）。
  `### 操作と展開規則` に標準ビルダー `Async` を追記。`## 診断` の E1013 に新メッセージの条件を追記。
- `docs/architecture.md`: `## 不変条件` に「`Async` 型の値は loan を持たない（`Checker::eval`）」。手順 10 の記録を `## 性能設計の原則` に計測条件と一緒に書く。
- `_docs/language-reference/computation-expressions.md`: `## 標準 Result ビルダー` の後に `## 標準 Async ビルダー`、`## 所有権と制限` に D6。
- `_docs/language-reference/tasks.md`: `## 作成と実行` に Task（並列・同期）と Async（協調・中断）の違い、`## 関連項目` にリンク。
- `_docs/library-reference/async.md`（新規）、`_docs/library-reference/api/Async.md`（生成）、`_docs/library-reference/README.md` の二つの表に行を足す。
- `_docs/feature-status.md` の B08 行を「Phase 1 実装済み（純粋な Async・仮想時刻・協調スケジューリング）。ホスト再開は計画」にする。
  `_features/README.md` の状態欄は Phase 2・3 が残るので `todo` のままにし、変えない。
- `README.md` の `## タスクと並列処理` に Async への一文とリンク。

## 受け入れ条件

- [ ] D1 が承認されている。
- [ ] `std/Async.tc` が「API」の公開宣言だけを持ち、`src/computation.rs`・`src/llvm*.rs`・`src/runtime/` に変更がない。
- [ ] `cargo test --locked --test async` が `9 passed`。拒否は診断表のコードとメッセージ全文で確かめている。
- [ ] suite `async` が native・WASM × `-O0`・`-O3` で成功し、`live == 0`、WASM import が空、`TSUZURI_ASAN=1` でも成功する。
- [ ] `Async` を使わないプログラムの IR（native・wasm32、`-O0`・`-O3`）が手順 1 と byte 単位で同じ。
- [ ] stack-depth の 3 テストと `honors_the_exact_specialization_limit` が成功し、上限を変えていない。
- [ ] 手順 10 の計測が記録され、計測していない性能を主張していない。
- [ ] 「ドキュメント」の更新があり、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `yield` は予約語。`Async.yield` と書くと字句・構文エラーになる。名前は `yield_now`。
- `Step` を `private` にすると E1022 の漏れエラー（「再現」）。公開のままにし、`Async` を opaque にして隠す。
- 同期に完了する反復（`For` の本体が `Done` を返す、`all` の round、`run`、`settle`）を再帰で書くと、`-O0` では末尾呼び出しが最適化されず
  stack が尽きる。`for_sync 100000` が検出する。`while` と `let mut` で書く。
- `Yield` で時刻を進めると `starvation` が 11 などになる。時刻を進めるのは実行器が `Sleep` を受けたときだけ。
- `round` が `Yield` の後に `Now` を要求しないと、外側で進んだ時刻を見落として `nested_all` が変わる。再開のたびに `Now` で時刻を取り直す。
- `all_results` が `Error` の後も round を続けると、取り消した子の副作用が起き、`results_first_error` の順序が崩れる。
- D6 の検査を `E::Closure` に置くと普通の関数値まで拒否する。`Checker::eval` で式の型が `Async` のときだけ見る。
  借用引数（`self.external` に入る root）の loan が `Value::loans` に出ない場合は、`rejects_borrows_across_suspension_points` の 3 例目が落ちる。
  そのときは検査を広げずに停止する。
- D6 は std のコードにも適用される。`std/Async.tc` の closure が引数を借用したまま `Async` を作らないよう、値は move かフィールド呼び出し（`body.start ()`）で扱う。
- closure の捕捉はスナップショットなので、継続の中で捕捉値を値として渡すと呼び出しごとに複製される。`For` で大きい配列の各要素で中断すると
  配列の複製が要素数の 2 乗になる。fixture の規模を表どおりに保ち（WASM の 16 MiB の検査がある）、複製を減らそうとしてコンパイラを変えない。
- 配列リテラルは `[a, b]`。空配列は型を注釈する（`let empty: [Async<i64>] = []`）。
- `cargo test --locked async` のような部分一致は他のテスト名にも当たる。`--test async` を使う。
- Node 20 で V8 の `RepresentationChangerError` が出たら `npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri async` で実行する。

## 対象外

- ホスト I/O・ホストによる再開・実時間のタイマー（Phase 2・3）。`IO` と `Async` の相互変換。
- stackful coroutine、OS スレッドを使う async executor、自動的な並列実行、先取り。
- `and!`、`while`、`use`、取り消し token、`Async` 対応の channel（F10）。
- 非 Copy の結果を持つ `all`・`all_results`（D7）。中断点の確保・複製の最適化（D11）。

## 決定事項

### D1: 名前と API 面

- 決定: std モジュール `Async`（`std/Async.tc`、予約モジュール名に追加）、ビルダー `Async { }`、型 `Async<'a>`。公開 API は「API」のとおり
  （`yield_now`・`sleep`・`now`・`all`・`all_results`・`run` とビルダー操作）。
- 理由: GUIDE D-30 の仮割り当てどおり。`yield` は予約語なので `yield_now`。名前は F# の `Async` と Rust の `yield_now` に合わせる。
- 状態: 要承認（承認前は Phase 1 のどの手順にも着手しない）

### D2: 表現

- 決定: 既存の計算式展開が作る継続 closure と、要求を表す union `Step` で表す。中断は継続を持った `Step` を実行器へ返すことで、
  stack の切り替えはしない。コンパイラ生成の状態機械は作らない。
- 理由: 試作が HEAD で動く（「再現」）。`src/computation.rs`・`src/llvm*.rs`・runtime を変えず、IR は決定的で WASM import も増えない。
  状態機械は新しい typed IR・frame 配置・LLVM lowering を要し、借用検査も作り直しになる。代償は中断ごとの確保と複製（D11）。
- 状態: 既定案（実装者はこの案に従う）

### D3: `Step` の公開と安定性

- 決定: `Step` は公開だが内部表現として文書化し、安定 API に含めない。Phase 1 の case は `Done`・`Yield`・`Sleep`・`Now`。Phase 2 で `Host` を足してよい。
- 理由: 公開レコードのフィールド型に private 型を使うと E1022。`Async` が opaque なので、利用者が `Step` を得る手段はない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 実行器と仮想時刻

- 決定: `run` は純粋な関数で、呼び出しごとに 0 から始まる仮想時刻を持つ。時刻は全計算が `sleep` 中のときだけ最小の起床時刻へ飛ぶ。
  `sleep n` は `n <= 0` で `yield_now` と同じ、加算は飽和。`now` は中断しない。
- 理由: ホストなしで決定的にテストできる。実時間はホストの時計が要り、D-18 の opt-in になる（Phase 2・E08）。
- 状態: 既定案（実装者はこの案に従う）

### D5: 不透明・非 Copy

- 決定: `Async.Async` を `stdlib::opaque_record` と `Type::is_noncopy_record` に加える。
- 理由: 元の設計の「不透明・非 Copy・一回消費」をコンパイラの型を増やさずに実現する。前例は `Seq`・`IO`。
  関数だけのレコードは HEAD では Copy で、暗黙の複製が継続の環境全体を複製する。
- 状態: 既定案（実装者はこの案に従う）

### D6: 中断を跨ぐ借用

- 決定: 型が `Async<T>` の式の値が loan を持ったら E1013（診断表のメッセージ）。`Checker::eval` の一か所で検査する。
  cold な開始も中断点とみなすので、外側の値の借用も拒否する。
- 理由: Phase 2 では継続が字句的な寿命を超えて runtime の表に残る。Phase 1 から同じ規則にしておけば後方互換を壊さない。
  `Task` の捕捉規則（E1013）と同じ考え方で、所有値の move と一 step 内の借用は許すので実用上の制約は小さい（「再現」の受理 3 例）。
- 状態: 既定案（実装者はこの案に従う）

### D7: スケジューリングの順序と `Copy` 制約

- 決定: `all`・`all_results` は子を index 順に開始し、round ごとに index 順で 1 step ずつ進める。結果は index 順。結果型に `Copy` を要求する。
- 理由: 順序が入力だけで決まる（GUIDE D-14 の決定性）。HEAD では配列から非 Copy の要素を move できないので、状態の配列を更新するには
  `Step<'a>` が Copy（`'a` が Copy）である必要がある。要素を move できる API が入ったら制約を外す（型の緩和なので互換）。
- 状態: 既定案（実装者はこの案に従う）

### D8: 取り消し

- 決定: 取り消しは drop だけ。`all_results` はスケジュール順（仮想時刻、同じ round では index）で最初の `Error` を返し、残りの子は
  現在の中断点で drop する。取り消し token は作らない。Phase 1 は B07 を使わない。
- 理由: B06 と同じく協調的で先取りしない。B06 と違い中断中の子は止められるので、最小 index を待たずに最初の失敗で止める。
  解放するのは closure の環境だけで、既存の drop で足りる。見直し提案: 依存欄の B07 は Phase 2 だけの依存である。
- 状態: 既定案（実装者はこの案に従う）

### D9: Task・スレッドとの関係

- 決定: `Async` 専用の `Send`・`Capture` 規則は作らない。`Async` は loan を持たないので、既存の規則で `Task` へ move でき、
  `Task` の中で `Async.run` できる。`Async` 自身はスレッドを使わず、複数スレッドで共有しない。
- 理由: 共有可変状態がなく、所有の move だけなので既存の送信検査で安全性を証明できる。証明できない共有は F10 の範囲。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2 のホスト再開

- 決定: 「Phase 2 以降」の形（`Step.Host`、`Async.host`、`Async.start`、runtime の継続表、`tsuzuri_async_complete`・`tsuzuri_async_poll`、
  B07 による取り消し通知）。すべて明示的な opt-in で、フラグ名・WASM import 名・診断は要割り当て。
- 理由: 新しい runtime・公開シンボル・WASM import は GUIDE D-17・D-18 の台帳に関わる。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D11: 中断点のコスト

- 決定: 中断ごとの closure の確保と、スナップショットによる環境の複製を許す。手順 10 で記録するだけで最適化しない。
- 理由: 計測前に表現を複雑にしない（AGENTS.md）。複製の省略は PM07、関数値の表現は PM02 の課題。
- 状態: 既定案（実装者はこの案に従う）

### D12: ビルダー操作の範囲

- 決定: `Return`・`ReturnFrom`・`Bind`・`Delay`・`Zero`・`Combine`・`For` だけを定義する。`While` は定義しない（捕捉した束縛は不変で、
  Phase 1 では条件を変える手段がない）。`MergeSources`・`Bind2` は定義しない（逐次か並行かが曖昧。並行は `all`）。`BindReturn` は最適化専用なので
  計測後に検討する。`Yield`・`YieldFrom`・`Using`・`Run` も定義しない（`Delay` が `Async` を返すので `Run` は不要）。
- 理由: 必要なものだけを公開し、追加は互換を壊さない。未定義の操作は既存の E1018 が利用者に伝える。
- 状態: 既定案（実装者はこの案に従う）

### D13: トラップ

- 決定: step 内のトラップは従来どおり全体を止める。隔離・新しい `TrapKind`・巻き戻しはない。境界は E14 が外側の呼び出しに設ける。
- 理由: GUIDE D-10。Phase 1 の実行器は普通の関数呼び出しなので、既存のトラップ経路がそのまま正しい。
- 状態: 既定案（実装者はこの案に従う）
