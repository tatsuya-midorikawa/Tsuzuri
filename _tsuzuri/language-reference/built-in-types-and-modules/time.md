# Time

`Time` は、単調時計、UNIX 時刻、スリープを扱うモジュールです。いずれの操作も `IO` アクションとしてのみ提供され、純粋関数の中から暗黙のうちに現在時刻を読み取ることはできません。

## この記事のポイント

- `monotonic_ns` は単調増加する時計です。起点は規定されていないため、2 点間の差分のみが意味を持ちます。
- `unix_ns` は 1970-01-01 UTC からの経過ナノ秒です。`i64` の表現範囲に収まらない場合は `Other` エラーとなります。
- `sleep_ms 0` は待機せずに `Ok ()` を返します。負の値を渡すと `InvalidInput` エラーとなります。
- 時刻の数値そのものは実行のたびに変化するため、テストや照合では差分や大小関係を検証します。

## 経過を測る

```mermaid
flowchart LR
    before["monotonic_ns"] --> work["測りたい処理"]
    work --> after["もう一度 monotonic_ns"]
    after --> diff["後から前を引く"]
```

スリープを挟まなくても、2 回の読みの差は 0 以上です。時計が戻らない、という契約の確認になります。

```tsuzuri run=true%0Aslept-zero%0Atrue%0Ainvalid%20input
def main :: unit -> i32 = \() ->
    let! before = Time.monotonic_ns ()
    let! slept = Time.sleep_ms 0
    let! after = Time.monotonic_ns ()
    let! unix = Time.unix_ns ()
    let! negative = Time.sleep_ms (-1)
    do! IO.write_line (match (before, after) with
        | (Result.Ok first, Result.Ok second) -> to_string (second - first >= 0)
        | _ -> "clock-failed")
    do! IO.write_line (match slept with
        | Result.Ok _ -> "slept-zero"
        | Result.Error error -> Os.message (ref error))
    do! IO.write_line (match unix with
        | Result.Ok value -> to_string (value > 0)
        | Result.Error _ -> "unix-failed")
    do! IO.write_line (match negative with
        | Result.Ok _ -> "unexpected"
        | Result.Error error -> Os.message (ref error))
    0
```

実行結果:

```text
true
slept-zero
true
invalid input
```

`sleep_ms` に正の値を渡すと、少なくともそのミリ秒は現在のスレッドを止めます。ドキュメントの照合を遅らせないため、上の例では `0` だけを実行しています。`50` を渡せば、そのあいだは次の `let!` へ進みません。

`unix_ns` は壁時計です。利用者が時計を戻すと、値も戻ることがあります。経過時間には `monotonic_ns` を使ってください。ファイルの `modified_ns` も UNIX 時刻ですが、こちらは [File](./file.md) のメタデータです。範囲外は丸められ、`unix_ns` の範囲外は `Other` です。同じ「ナノ秒」でも、失敗の仕方が違います。

読み取りに失敗すると、`Os.Error { kind: Other, code: 0 }` になります。`Os.message` は `other` です。

差を取る 2 回の読みは、別々のアクションです。先に `let pending = Time.monotonic_ns ()` と置いただけでは、まだ時計を読んでいません。`let!` した時点の値です。待っている間に他の `IO` を実行すると、その時間も差に入ります。測りたい処理だけを 2 回の読みのあいだに置いてください。

## 公開 API

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Time.monotonic_ns` | `unit -> IO<Result<i64, Os.Error>>` | 戻らない時計のナノ秒。差だけを使う |
| `Time.unix_ns` | `unit -> IO<Result<i64, Os.Error>>` | 1970-01-01 UTC からのナノ秒。範囲外は `Other` |
| `Time.sleep_ms` | `i64 -> IO<Result<unit, Os.Error>>` | 少なくともそのミリ秒待つ。`0` は即 `Ok`。負は `InvalidInput` |

呼び出しは `Time.sleep_ms 0` や `Time.monotonic_ns ()` のように記述し、`unit` 引数には `()` を渡します。

## 所有権と対応環境

戻り値の `i64` は `Copy` です。アクション自体は実行するまで時計を読みません。同じ `IO` 値を 2 回実行すると、その都度時計を読み取ります。

既定の wasm32 でこれらの関数に到達すると、ビルドは `E2000` で失敗します。`--wasm-host wasi` では WASI の時計とポーリングに下がります。Windows のネイティブビルドは、OS API に到達すると `E2002` で失敗します。これらの環境では本ページの実行例をそのまま実行することはできません。

並行 `Task` の中から、これらのアクションを直接実行することはできません。測定はメインスレッド（エントリーポイント）で行います。

## 他の言語との比較

| | Tsuzuri | 近いもの |
| --- | --- | --- |
| 経過 | `monotonic_ns` の差。起点は未規定 | Rust の `Instant`、C# の `Stopwatch` |
| 壁時計 | `unix_ns`。戻り得る | `SystemTime`、`DateTimeOffset` の UTC |
| 待機 | `sleep_ms`。負はエラー、`0` は即戻る | `Thread.Sleep`。ここにはタイムアウト付きの待ち受けはない |

日時をカレンダーの年月日へ分ける関数は、標準ライブラリにはありません。ナノ秒のまま差を取るか、自分で割り算します。

## まとめ

- 経過時間は `monotonic_ns` の差です。起点には意味がありません。
- 日時を扱うなら `unix_ns` です。調整で戻ることがあります。
- `sleep_ms` の `0` は待たず、負は `InvalidInput` です。
- 数値そのものは毎回変わるので、比較や差で確認します。
- 時計の読み取りは `IO` の実行時だけです。

## 関連項目

- [IO](./io.md)
- [Os](./os.md)
- [File](./file.md)
- [言語仕様の環境・時刻・乱数・プロセス](../../../docs/language.md#環境時刻乱数プロセス)
- [言語リファレンスの目次](../index.md)
