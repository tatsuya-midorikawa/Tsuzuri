# 高階型と型コンストラクター

[ドキュメントのトップ](../README.md)

高階型 (HKT) は、`Maybe<i64>` のような完成した型だけでなく、`Maybe` のような型を作るものをパラメーターにする機能です。現在は明示 kind を使う rank-1 の型コンストラクターに対応します。

## kind

| kind | 意味 |
| --- | --- |
| `*` | i64 や string のような値型 |
| `* -> *` | 値型を一つ受け取る型コンストラクター |
| `* -> * -> *` | 値型を二つ受け取る型コンストラクター |

kind の矢印は右結合です。クラスの型パラメーターに kind を明示し、省略した場合は `*` です。`(* -> *) -> *` のようにコンストラクター自体を引数に取る kind は未対応です。

## コンテナーを抽象化する例

`Traits.tt`:

```tsuzuri project=functor file=Traits.tt
class Functor<'container: * -> *> {
    def map :: ('input -> 'output) -> 'container<'input> -> 'container<'output>
}
```

`Main.tz`:

```tsuzuri project=functor file=Main.tz run=42
instance Traits.Functor<Maybe> {
    fn map transform value = Maybe.map transform value
}

instance Traits.Functor<Result<string>> {
    fn map transform value = Result.map transform value
}

def fmap :: Traits.Functor<'container> => ('input -> 'output) -> 'container<'input> -> 'container<'output> = \transform value -> Traits.Functor.map transform value

let original: Result<i64, string> = Result.Ok 40
let result = fmap (value -> value + 2) original
match result with
| Result.Ok value -> value
| Result.Error _ -> 0
```

`'container<'input>` は型変数であるコンストラクターへの適用です。kind が値型でないクラスでは、メソッド固有の値型変数 `'input` / `'output` を使えます。

通常のジェネリック関数は `Functor<'container>` の制約から kind を得ます。未宣言のコンストラクター変数に対して kind を推測する機能ではありません。

## 末尾の型引数を固定する

上の `Result<string>` は、完成した一引数の Result 型ではありません。コンストラクター位置で末尾のエラー型を固定し、入力の型を受けて `Result<Input, string>` を作る `* -> *` です。

通常の値型では `Result<i64, string>` のように必要な型引数をすべて指定します。部分適用の記法を値の型として使うとエラーになります。

## 対応する構造

record / union の宣言の型引数数に応じたコンストラクター、および Array / List / Vec / Task に対応します。値型としての配列 `[T]`（`Array<T>` とも書けます）とリスト `[|T|]` の記法は変わりません。

デフォルトメソッド、条件付きインスタンス、重複検査は通常の型クラスと同じ仕組みを使います。具体化後のコードではコンストラクター適用を通常の型に解決し、boxing や実行時型情報を追加しません。

## 制限と診断

kind 不一致、型引数不足のコンストラクターを値型へ使用すること、過剰な適用は `E1015` です。汎用・具体インスタンスの重複は `E1016` です。

HKT 型別名、高階 kind 引数、kind 注釈の省略推論は未対応です。標準の `Functor` / `Applicative` / `Monad` が自動で導入されるわけでもありません。この例の `Functor` は利用者定義です。

Maybe / Result の通常の関数やコンピュテーション式を使うだけなら、HKT を定義する必要はありません。複数の型コンストラクターに同じ API を提供する必要がある場合に使います。

## 関連項目

- [型クラス](generics-and-typeclasses.md)
- [型と型推論](types.md)
- [高階型の正式な契約](../../docs/language.md#高階型hkt)
