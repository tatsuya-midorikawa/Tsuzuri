# Random

Tsuzuri には 2 種類の乱数機構があります。OS が提供する暗号論的乱数バイト列は `IO` アクションとしてのみ取得できます。一方、`Random.Pcg` は純粋な値であり、同一のシードからはネイティブでも WASM でも決定論的に同一の乱数列が得られます。暗黙のうちに参照される大域的な乱数シードは存在しません。

## この記事のポイント

- `Random.bytes` と `Random.next_u64` は OS カーネルが提供する暗号論的乱数です。実行のたびに値が変わります。
- 要求バイト数が負、または 2^30 を超えると `InvalidInput` エラーとなります。
- `Random.pcg` が構築する `Pcg` は不透明な値です。呼び出しても元の生成器の状態は変化しません。
- 次の乱数を取得する際は、関数が返却した新しい `Pcg` を次の呼び出しへ引き渡します。
- 擬似乱数は wasm32 環境でも追加のホストインポートなしで動作します。OS の乱数は `--wasm-host wasi` が必要です。

## OS の乱数

値そのものを照合すると、実行のたびに失敗します。長さと、失敗の種類だけを見ます。

```tsuzuri run=empty:0%0Asample:4%0Ainvalid%20input%0Aword
def main :: unit -> i32 = \() ->
    let! empty = Random.bytes 0
    do! IO.write_line (match empty with
        | Result.Ok bytes -> "empty:" + to_string bytes.length
        | Result.Error error -> Os.message (ref error))
    let! sample = Random.bytes 4
    do! IO.write_line (match sample with
        | Result.Ok bytes -> "sample:" + to_string bytes.length
        | Result.Error error -> Os.message (ref error))
    let! negative = Random.bytes (-1)
    do! IO.write_line (match negative with
        | Result.Ok _ -> "unexpected"
        | Result.Error error -> Os.message (ref error))
    let! word = Random.next_u64 ()
    do! IO.write_line (match word with
        | Result.Ok _ -> "word"
        | Result.Error error -> Os.message (ref error))
    0
```

実行結果:

```text
empty:0
sample:4
invalid input
word
```

`bytes 0` は空の配列で成功します。`next_u64` は 8 バイトをリトルエンディアンの `i64u` として読みます。暗号用途のシードやセッショントークンなどには OS の乱数を使い、テストで再現したい乱数列には後述の `Pcg` を使用します。

`[ubyte]` は `Copy` です。乱数の配列を関数へ渡しても、元の変数は残ります。大きな配列はコピーします。上限の 2^30 バイトは 1 GiB です。

## 種から再現する

`Random.pcg seed sequence` は PCG-XSH-RR 64/32 の生成器です。`sequence` から奇数の増分を作るので、利用者が偶数の増分を渡して列を壊すことはありません。フィールドは `E1022` で読めません。

同じ値をもう一度渡すと、同じ次の値が出ます。状態は呼び出し先で書き換わらないからです。

```tsuzuri run=2707161783%202068313097%0Atrue%0A11627171325034361865
def main :: unit -> i32 = \() ->
    let generator = Random.pcg 42i64u 54i64u
    do! IO.write_line (match Random.pcg_next_u32 generator with
        | (first, next) ->
            match Random.pcg_next_u32 next with
            | (second, _) -> to_string first + " " + to_string second)
    let same = Random.pcg 42i64u 54i64u
    let repeated = match Random.pcg_next_u32 same with
        | (value, _) -> value
    let again = match Random.pcg_next_u32 same with
        | (value, _) -> value
    do! IO.write_line (to_string (repeated == again))
    let wide = Random.pcg 42i64u 54i64u
    do! IO.write_line (match Random.pcg_next_u64 wide with
        | (value, _) -> to_string value)
    0
```

実行結果:

```text
2707161783 2068313097
true
11627171325034361865
```

`42` と `54` は、pcg-c-basic の公開例と同じ種です。最初の 2 つの 32 ビットが `2707161783` と `2068313097` になります。`pcg_next_u64` は、32 ビットを 2 回出して、先の方を上位 32 ビットにします。上の 64 ビット値は、その結合です。

```mermaid
flowchart LR
    seed["pcg seed sequence"] --> gen["Pcg"]
    gen --> step["pcg_next_u32"]
    step --> pair["値と次の Pcg"]
    pair --> again["次の Pcg を次の呼び出しへ"]
```

列を進めるときは、返った `Pcg` を次に渡します。元の変数を使い続けると、同じ値が繰り返されます。`let mut` で返った値を入れ直すと、ループの中でも書けます。

```text
let mut state = Random.pcg 42i64u 54i64u
match Random.pcg_next_u32 state with
| (value, next) -> state = next
```

これは断片です。`state = next` は、古い生成器を新しい生成器で置き換えます。

## 公開 API

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Random.bytes` | `i64 -> IO<Result<[ubyte], Os.Error>>` | OS の乱数を `count` バイト。範囲外は `InvalidInput` |
| `Random.next_u64` | `unit -> IO<Result<i64u, Os.Error>>` | OS の 8 バイトをリトルエンディアンの `i64u` にする |
| `Random.pcg` | `i64u -> i64u -> Random.Pcg` | 種とストリーム番号から生成器を作る |
| `Random.pcg_next_u32` | `Random.Pcg -> (i32u * Random.Pcg)` | 次の 32 ビットと、進んだ生成器 |
| `Random.pcg_next_u64` | `Random.Pcg -> (i64u * Random.Pcg)` | 32 ビットを 2 回結合した 64 ビットと、進んだ生成器 |

`Pcg` は `Copy` です。関数へ渡しても元は残ります。残ることと、状態が進むことは別です。進むのは戻り値の方です。

## 対応環境

`Random.pcg` と `pcg_next_*` は OS を呼ばないので、既定の wasm32 でも同じ列になります。`bytes` と `next_u64` に到達するコードは、既定の wasm32 では `E2000` でビルドに失敗します。`--wasm-host wasi` では WASI の乱数に下がります。

`HashMap.randomized` のように OS の種を使う API も、同じ理由で既定の wasm32 では拒否されます。種を自分で渡す版は拒否されません。

Windows のネイティブビルドは、OS の乱数 API に到達すると `E2002` で失敗します。一方、`Pcg` のみを使用するプログラムはこの制限を受けません。

## まとめ

- OS の乱数は `IO` です。個数の範囲は 0 以上 2^30 以下です。
- 再現したい列は `Pcg` です。種とストリームが同じなら列も同じです。
- 生成器は不変です。次の値は、返ってきた新しい `Pcg` から取ります。
- フィールドは読めません。奇数の増分は `pcg` が作ります。
- 擬似乱数は WASM でも追加のホスト関数なしで動きます。

## 関連項目

- [IO](./io.md)
- [Os](./os.md)
- [HashMap](./hashmap.md)
- [Gen](./gen.md)（`Random.pcg` で値を選ぶプロパティテスト）
- [言語仕様の環境・時刻・乱数・プロセス](../../../docs/language.md#環境時刻乱数プロセス)
- [言語リファレンスの目次](../index.md)
