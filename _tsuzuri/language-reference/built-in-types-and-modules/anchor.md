# Anchor

`Anchor<T>` は、環境を捕捉できない静的コールバック（`FnPtr` や `extern` のコールバック引数）から、アプリケーションの状態 `T` を安全に読み書きするためのハンドルです。`Anchor.create` が状態をランタイムのスロット表に置き、C の userdata（`GWLP_USERDATA` や `void *`・`intptr_t`）に通せる非ゼロの `i64` トークンを発行します。

名前を書いたプログラムだけが読み込む標準モジュールです。`Anchor` という利用者のモジュール名は予約され、衝突は `E1011` です。

## この記事のポイント

- `Anchor.create value` は `value` をスロット表に格納し、その寿命を握る所有値 `Anchor<T>` を返します。`Anchor<T>` が `Drop` されるとスロットは即座に解放され、同じトークンは二度と有効になりません（世代番号つき）。
- `Anchor.token (ref anchor)` が返す `i64` は非ゼロで、C 側の整数・ポインタ幅 userdata にそのまま保存できます。トークン自体は型引数を持たない `i64` なので、`Anchor.read`・`Anchor.update` に渡す関数側で期待する状態の型（`\(s: ref AppState) -> ...` や名前付き関数）を示します。
- コールバック側では `Anchor.read (\state -> ...) token` で共有借用 `ref T` を読み、`Anchor.update (\state -> next_state) token` で値を置き換えます。
- 解放済みトークンは `Stale`、型の不一致は `WrongType`、作成した OS スレッド以外からの呼び出しは `WrongThread`、書き込み借用（`Anchor.update`）と競合する再入は `Busy` として、未定義動作ではなく `Result.Error` で返ります（共有借用の `Anchor.read` 同士の入れ子は成功します）。
- `Anchor<T>` 自身は作成した OS スレッドに結び付いており、`Copy`・`Capture`・`Send`・`Sync` のいずれも満たしません。一方、`Anchor.token` が返す `i64` はただの整数なので自由に渡せます。

## API

```text
record Anchor<'a>              // 不透明な所有ハンドル（Drop で解放）
union Error = Stale | WrongType | WrongThread | Busy

Anchor.create :: 'a -> Anchor<'a>
Anchor.token :: ref Anchor<'a> -> i64
Anchor.read :: (ref 'a -> 'b) -> i64 -> Result<'b, Anchor.Error>
Anchor.update :: ('a -> 'a) -> i64 -> Result<unit, Anchor.Error>
```

### 引数の順序

- `Anchor.read` と `Anchor.update` は、操作対象のトークン `i64` を**最後の引数**に取ります。コールバックが先なので、`token |> Anchor.read (\s -> s.count)` のようにパイプでつなげられます（[標準ライブラリの引数の順序](argument-order.md)）。
- `Anchor.token` は資源ハンドル `ref Anchor<'a>` を 1 つだけ受け取ります。

## 基本の書き方

状態を `Anchor.create` で預け、静的コールバックの側では `i64` トークンから状態を更新・参照します。

```tsuzuri run=count%3D2%20after_drop%3Dstale%20anchor%20token
record AppState { count: i64 }

def on_event :: i64 -> i64 -> Result<unit, Anchor.Error> = \delta tok ->
    tok |> Anchor.update (\(s: AppState) -> AppState { count: s.count + delta })

def run_scope :: unit -> (i64 * i64) = \() ->
    let app = Anchor.create (AppState { count: 0 })
    let key = Anchor.token (ref app)
    let _ = on_event 1 key
    let _ = on_event 1 key
    let current =
        match key |> Anchor.read (\(s: ref AppState) -> s.count) with
        | Result.Ok n -> n
        | Result.Error _ -> -1
    (current, key)

def main :: unit -> i32 = \() ->
    let (current, tok) = run_scope ()
    let after =
        match tok |> Anchor.read (\(s: ref AppState) -> s.count) with
        | Result.Ok n -> $"{n}"
        | Result.Error err -> $"{err}"
    do! IO.writeln $"count={current} after_drop={after}"
    0
```

実行結果:

```text
count=2 after_drop=stale anchor token
```

内側のブロックを抜けた時点で `app` が `Drop` されるため、その後に残った `tok` で `Anchor.read` を呼ぶと `Result.Error Anchor.Stale` になります。

## 借用と再入の規則

- `Anchor.read` のコールバックが受け取る `ref 'a` は、その `Anchor.read` が返るまでの借用です。コールバックの戻り値にその借用を含めることはできず、試みると所有権検査の `E1013`（`anchor_result_retains_borrow`）になります。
- `Anchor.create` と `Anchor.update` に渡す値は、外の変数の借用を保持していてはなりません（`E1013` `anchor_value_retains_borrow`）。
- 同じ生存中トークンに対する入れ子の `Anchor.read`（共有借用同士）は成功します。一方、`Anchor.update` の実行中に同じトークンへ `Anchor.read` または `Anchor.update` で再入したとき、および `Anchor.read` の実行中に同じトークンへ `Anchor.update` で再入したときは `Result.Error Anchor.Busy` を返します。
- `Anchor.create` を `tzroutine` の中から呼ぶと、ランタイムは自動的にその tzroutine を現在の OS スレッドに固定します（`Anchor<T>` が `Drop` されて解放が終わるまでその OS スレッドにとどまり、その間その OS スレッド上で他の tzroutine と協調的に切り替わります）。事前に `Routine.with_locked_thread` で囲む必要はなく、トラップもしません。

## エラーの種類

| ケース | `Display` の文字列 | 起きる場面 |
| --- | --- | --- |
| `Anchor.Stale` | `stale anchor token` | `0`・未割り当ての番号、または `Anchor<T>` がすでに `Drop` された後のトークン |
| `Anchor.WrongType` | `anchor token has a different value type` | `Anchor.create` 時の型 `T` と異なる型で `read` / `update` を呼んだとき |
| `Anchor.WrongThread` | `anchor token belongs to another thread` | `Anchor.create` を実行した OS スレッドとは別の OS スレッドから呼んだとき |
| `Anchor.Busy` | `anchor value is already borrowed` | `Anchor.update` の実行中に同じスロットへ `read` / `update` で再入したとき、または `Anchor.read` の実行中に同じスロットへ `update` で再入したとき |

## 関連項目

- [FnPtr](fnptr.md)
- [Routine](routine.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
