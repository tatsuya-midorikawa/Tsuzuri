# assert 式

`assert` は、成り立つはずの条件を実行時に確認する組み込み関数です。条件が偽なら、その位置でトラップします。最適化で消えることはありません。テストの比較には `Test.equal` などを使い、`assert` は不変条件に残すと読み分けやすくなります。

## この記事のポイント

- 型は `bool -> unit` です。偽なら `assertion failed` でトラップします。
- `-O0` でも `-O3` でも残ります。リリース用に無効化するスイッチはありません。
- 失敗は例外ではありません。`try` では捕まりません。
- テスト中の失敗も同じトラップです。差分メッセージは出ません。
- 比較の意図があるテストでは `Test.equal`、`Test.not_equal`、`Test.is_true` を使います。

## 基本の書き方

```text
assert 条件
```

条件は `bool` です。`assert` は関数なので、呼び出しの優先順位は `+` や `==` より高くなります。`assert 1 + 1 == 2` は `assert 1` と読まれ、`E1003`（`expected bool, found i32`）です。比較は括弧で囲みます。

```tsuzuri run=ok
def main :: unit -> i32 = \() ->
    let price = 120
    let tax = 30
    assert (price + tax == 150)
    do! IO.write_line "ok"
    0
```

実行結果:

```text
ok
```

戻り値は `unit` です。続く式を書く文として使えます。値を返す関数ではありません。

条件が偽のときは、標準エラーに次の形で出て、プロセスは止まります。`tsuzuri run` は同じ位置に `E2005` も出します。

```text
def main :: unit -> i32 = \() ->
    let ready = false
    assert ready
    0
```

```text
trap: assertion failed at Main.tz:4:5
```

この出力は `-O0` と `-O3` の両方で確認しています。定数の `assert false` も、最適化で削除されません。LLVM の `llvm.trap` として残り、条件が真だと分かった分岐だけが消えます。

> [!WARNING]
> `assert` はデバッグ専用のチェックではありません。本番のバイナリでも偽ならトラップします。利用者の入力ミスを止める場所には、`Result` や `Maybe` を返してください。

## テストでの報告

`test` 宣言の中で `assert` が失敗すると、そのテストは失敗です。テストランナーは各テストを別プロセスで実行し、標準エラーのトラップ文は捨てます。失敗理由は、信号で止まったか、0 以外の終了コードか、だけです。

```text
not ok 2 - Main stock is positive
  failure: trapped or terminated by signal
```

失敗したテストのあとも、ほかのテストは実行されます。1 件でも失敗すると、最初に失敗したテスト名の位置で `E2006` が出て、`tsuzuri test` の終了コードは 1 です。`-O3` を指定しても、偽の `assert` は同じく失敗します。

カスタムのメッセージや、期待値と実際の値の差分表示はありません。理由を残したいときは、テストの名前で何を確認しているかを書いてください。

## Test の関数との使い分け

`Test` の 3 関数は、中で `assert` を呼ぶだけです。失敗時の動きは `assert` と同じトラップです。

| 関数 | シグネチャ | いつ使うか |
| --- | --- | --- |
| `assert` | `bool -> unit` | その場の不変条件。計算の途中でも使える |
| `Test.equal` | `Eq<'a> => ref 'a -> ref 'a -> unit` | 期待値と実際の値が等しいこと |
| `Test.not_equal` | `Eq<'a> => ref 'a -> ref 'a -> unit` | 2 つの値が異なること |
| `Test.is_true` | `bool -> unit` | テスト本体で、条件が真であること |

`Test.equal` と `Test.not_equal` は引数を借用して比較するため、値を消費しません。変数や文字列リテラルはそのまま渡すだけで自動的に借用されます。型推論があいまいになりやすい数値リテラルや計算式を直接渡す場合は、`ref`（または型注釈）を明示して `Test.equal (ref 360) (ref (120 * 3))` のように記述します。

```tsuzuri
test "line total matches the price times quantity" =
    let price = 120
    let qty = 3
    let actual = price * qty
    let expected = 360
    Test.equal expected actual

test "names stay distinct" =
    Test.not_equal "tea" "coffee"

test "stock covers the order" =
    Test.is_true (5 >= 2)
```

`assert (actual == expected)` でもテストは書けます。等しいかどうかが主題なら `Test.equal` の方が、比較そのものがテストだと読めて、所有権も消費しません。`==` が無い型でも、`Eq` があれば `Test.equal` を使えます。

`Result` を返す API の失敗は、`assert` で潰さないでください。`Error` は回復できる値です。`match` でケースを確認します。`assert` が守るのは、そこまで来たら真であるはずの条件です。

## unreachable との違い

`unreachable :: unit -> 'a` もトラップします。条件を受け取りません。パターンや前の検査で、その分岐には来ないことを示すときに使います。報告される理由は `non-exhaustive match` です。

```text
def paid :: Result<i64, string> -> i64 = \result ->
    match result with
    | Ok amount -> amount
    | Error _ -> unreachable ()
```

`Error` でこの関数を呼ぶとトラップします。呼び出し元で回復したいなら、`unreachable` ではなく `match` で `Error` を扱います。`Result.get` と `Maybe.get` も、期待と違うケースでは同じトラップです。

## 注意点

- トラップなので、偽の `assert` のあとにある `Drop` は実行されません。
- テストの失敗ログには、どの値が違ったかは出ません。名前と、必要なら `Debug.trace` で足します。`Debug` の出力は標準エラーで、テストランナーはそれを失敗理由には使いません。
- `assert` を本番の入力検査に使うと、利用者のミスでプロセスが止まります。入力の拒否は `Result` で返します。

## まとめ

- `assert` は `bool -> unit` で、偽なら最適化レベルに関係なくトラップします。
- 比較は括弧で囲みます。`assert` は `==` より強く結合します。
- テストの等値比較は `Test.equal` に任せ、`assert` は不変条件に使います。
- 失敗は `try` では捕捉できず、テストでは信号停止として報告されます。
- 来ないはずの分岐は `unreachable ()` です。こちらも捕捉できません。

## 関連項目

- [例外処理](exception-handling.md)
- [Test](../built-in-types-and-modules/test.md)
- [Debug](../built-in-types-and-modules/debug.md)
- [Result](../built-in-types-and-modules/result.md)
- [言語仕様: 言語内テスト](../../../docs/language.md#言語内テスト)
- [言語仕様: トラップ位置](../../../docs/language.md#トラップ位置)
- [言語リファレンスの目次](../index.md)
