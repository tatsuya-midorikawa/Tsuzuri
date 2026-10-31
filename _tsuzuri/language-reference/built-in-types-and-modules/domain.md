# Domain

`Domain.run` は、純粋な計算を**失敗の領域（failure domain）**の中で呼びます。中でトラップしても（添字の範囲外、ゼロ除算、`assert`、網羅されない `match`、確保の失敗など）プロセスは終わらず、`Result.Error` が返ります。領域の中で確保したものは解放され、外の値は変わりません。AI が生成した計算や外部の入力に対する計算を 1 件ずつ試したり、`tsuzuri test` の中でトラップを確かめたりできます。

名前を書いたプログラムだけが読み込む標準モジュールです。`Domain` という利用者のモジュール名は予約され、衝突は `E1011` です。

## この記事のポイント

- `Domain.run (\() -> 計算)` は、値が返れば `Result.Ok 値`、トラップしたら `Result.Error failure` です。
- `failure.kind` はトラップの種類の番号（1 はゼロ除算、3 は添字の範囲外）、`failure.site` は `.trap.json` の位置の番号、`failure.message` は理由と位置の文です。
- 引数はその場で書いたラムダか `pure def` の名前で、捕捉する値も結果も平たいデータ（関数・`dyn`・タスク・`Rc`・`Arc`・extern のハンドル・利用者の `Drop` を持たない値）です。本体は純粋でなければなりません。違反は `E1038` です。
- 閉じ込めないもの: スタック枯渇（プロセスが終わる）、終わらない計算、確保量の上限、領域の外のトラップ。
- macOS と Linux のネイティブのビルドだけです。WebAssembly・Windows・`--freestanding`・システム以外の allocator は `E1038` です。

## API

```text
record Failure { site: i64, kind: i64, message: string }
Domain.run :: (unit -> 'a) -> Result<'a, Domain.Failure>
```

- `run body` は `body ()` を領域の中で一度呼びます。中で呼ぶ関数のトラップも含めて閉じ込めます。
- 成功すると、結果が持つ確保は呼び出し元の物になります。トラップすると、領域の中で確保した block をすべて解放します。利用者の `Drop` と `try … finally` は走りません（巻き戻しはしません。純粋な本体は `Drop` を持つ値を作れません）。

### 引数の順序

`run` の引数は本体の関数 1 つだけなので、順序の問題は起こりません（[標準ライブラリの引数の順序](argument-order.md)）。

## 基本の書き方

平均を求める計算を、空でない配列と空の配列で試します。空の配列は長さ 0 で割るので、2 つ目はゼロ除算（`kind` 1）になります。

```tsuzuri run=average%209%0Afailed%20(kind%201)
def average :: [i64] -> i64 = \xs -> Array.sum (ref xs) / xs.length

def describe :: Result<i64, Domain.Failure> -> string = \outcome ->
    match outcome with
    | Result.Ok value -> $"average {value}"
    | Result.Error failure -> $"failed (kind {failure.kind})"

def main :: unit -> i32 = \() ->
    let data = [4, 8, 15]
    let empty: [i64] = []
    do! IO.write_line (describe (Domain.run (\() -> average data)))
    do! IO.write_line (describe (Domain.run (\() -> average empty)))
    0
```

実行結果:

```text
average 9
failed (kind 1)
```

`message` は `integer division by zero at ./Main.tz:1:38` のように理由と位置です（パスはビルドに渡したソースのパス）。位置の表を持たない実行ファイル（`bench` と、契約の行のないプログラムの `tsuzuri test`）では理由だけです。

## テストの中でトラップを確かめる

```tsuzuri
def at :: [i64] -> i64 -> i64 = \xs i -> xs[i]

test "an out-of-range index is caught by the domain" =
    let numbers = [1, 2, 3]
    match Domain.run (\() -> at numbers 5) with
    | Result.Error failure -> Test.is_true (failure.kind == 3)
    | Result.Ok _ -> Test.is_true false
```

`at` は添字の範囲外を起こすための補助です。トラップがテストの失敗にならず、`Result.Error` として調べられます。

## `Failure`

| フィールド | 意味 |
| --- | --- |
| `site` | トラップした位置の番号。`--trap-info` の `<出力>.trap.json` の `id` と同じ |
| `kind` | トラップの種類の番号（下の表） |
| `message` | `<理由> at <パス>:<行>:<列>`。位置の表がなければ（`bench` と、契約の行のないプログラムの `tsuzuri test`）`<理由>` |

`kind` の番号は、ホストの `tsuzuri_trap_info.kind`（`--trap-mode return`）と同じです。

| `kind` | 理由（`.trap.json` の `kind`） | `Domain.run` の中で起きる例 |
| --- | --- | --- |
| 0 | assertion failed | `assert` が偽 |
| 1 | integer division by zero | `x / 0` |
| 2 | integer division overflow | `Int.min / -1` |
| 3 | index out of bounds | 長さ 3 の配列の `xs[5]` |
| 4 | allocation size overflow | 巨大な長さの配列 |
| 5 | allocation failed | 確保の失敗 |
| 6 | range step is zero | 刻みが 0 の範囲 |
| 7 | non-exhaustive match | 網羅されない `match`、`unreachable` |
| 8 | pattern mismatch | 失敗するパターン束縛 |
| 9 | string length overflow | 文字列の連結の長さの溢れ |
| 10 | numeric runtime trap | 数値の補助関数のトラップ |
| 11 | wasm runtime trap | WebAssembly だけなので起きない |
| 12 | invalid Unicode encoding | 不正な符号化の復号 |
| 13 | debug output failed | 純粋な本体では起きない |
| 14 | stack overflow | 閉じ込めない（プロセスが終わる）ので現れない |
| 15 | unhandled OverflowException: arithmetic operation resulted in an overflow | `try` の外の `@checked` の算術の溢れ |
| 16 | contract requires failed | 呼んだ関数の `@requires` が偽 |
| 17 | contract ensures failed | `@ensures` が偽 |

## E1038

`Domain.run` の呼び出しごとに、次の規則を順に調べ、最初の違反だけを報告します。

| 規則 | kind | メッセージ |
| --- | --- | --- |
| 直接呼んでいない（関数値として渡す・変数に入れる） | `domain_not_called_directly` | `'Domain.run' must be called directly with its closure; write 'Domain.run (\() -> ...)'` |
| 引数がラムダでも `pure def` の名前でもない | `domain_argument_not_closure` | `the argument of 'Domain.run' must be a lambda or the name of a pure def; ...` |
| 平たいデータでない値を捕捉する（`ref` は 1 段外して調べる） | `domain_capture_not_plain` | `the closure passed to 'Domain.run' captures '{名前}', which holds {何}; ...` |
| 本体が純粋でない（呼ぶ関数を含む。中の `Domain.run` も） | `domain_body_not_pure` | `the closure passed to 'Domain.run' must be pure: it {理由}; ...` |
| 結果が平たいデータでない | `domain_result_not_plain` | `the result of 'Domain.run' must be plain data; '{型}' holds {何}` |
| ビルドが領域を持てない | `domain_target_unsupported` | `'Domain.run' needs a native POSIX build with the system allocator: {理由}` |

```text
let work = \() -> 1 / 0
let a = Domain.run work                                  // E1038: 変数ではなくラムダを直接渡す
let b = Domain.run (\() -> { Debug.print "x"; 1 })       // E1038: ... it prints debug output
let log = \(text: string) -> Debug.print text
let c = Domain.run (\() -> { log "x"; 1 })               // E1038: captures 'log', which holds a function, 'dyn', or a task
```

捕捉に関数値を許さないのは、外から入る関数値の本体を、純粋性の検査（[純粋関数](../values-and-functions/functions.md#純粋関数pure-def)と同じ検査）が調べられないからです。`check` と LSP はターゲットを見ないので、最後の規則は `build`・`run`・`test` が調べます。

## 使えるビルド

| ビルド | 結果 |
| --- | --- |
| macOS・Linux のネイティブで、システムの allocator の `build`（実行ファイル・`object`・`shared`・`bitcode`・`header`）・`run`・`script`・`test`・`bench` | 動く |
| `--emit llvm` | `--trap-mode return` が要る。IR を自分でリンクするホストが `src/runtime/trap.c` を用意する（[トラップ位置](../../../docs/language.md#トラップ位置)） |
| `--target wasm32`・`wasm64`（`tsuzuri test` の WebAssembly を含む）、`--freestanding`、`--allocator host`・`counting`・`small`、`bench --allocs`、Windows のネイティブ | `E1038` |

`--trap-mode return` の `tsuzuri_try_<name>` の境界の中でも動きます。領域は境界の中に入れ子になり、領域のトラップは境界の確保に触れません。

## 後始末と性能

`Domain.run` を使うプログラムは、`--trap-mode return` と同じ追跡付きのヒープで動き、どのトラップもまず最も内側の領域へ戻ります。領域の外のトラップは今までどおりプロセスを終えます（位置の行 `trap: <理由> at <位置>` が標準エラーに出ます）。

| 事象 | 領域の中でトラップしたとき |
| --- | --- |
| 領域の中で確保した block | すべて解放する |
| 捕捉した値（関数値の環境） | 外の物なので変わらない。呼び出しごとに写しを領域の中に作る |
| 利用者の `Drop`・`try … finally` | 走らない（本体が持てない） |
| 標準出力・ファイル・ロック | 本体が触れない（純粋でない） |
| stack | 捨てる（領域の入口へ jump する） |

macOS arm64（Apple M1 Max）で測った費用は、空の領域 1 回が約 40 ns、領域の中の確保 1 回ごとに追跡の約 35 ns、成功したときの結果の block の引き継ぎです。大きな配列を捕捉すると呼び出しごとに写しを作るので、`ref` で捕捉するか、領域の中で作るほうが速くなります。`Domain` を使わないプログラムの生成コードは変わりません。

## まとめ

- `Domain.run (\() -> 計算)` は、トラップを `Result.Error failure` に変えてプロセスを続けます。
- `failure.kind` は種類の番号、`failure.site` は位置の番号、`failure.message` は理由と位置です。
- 本体は純粋で、捕捉と結果は平たいデータです（`E1038`）。
- macOS と Linux のネイティブだけで、スタック枯渇は閉じ込めません。

## 関連項目

- [例外処理](../exception-handling/exception-handling.md)
- [純粋関数](../values-and-functions/functions.md#純粋関数pure-def)
- [テスト](test.md)
- [診断メッセージとエラーコード](../compiler/diagnostics.md)
- [言語リファレンスの目次](../index.md)
