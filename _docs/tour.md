# Tsuzuri のツアー

[ドキュメントのトップ](README.md)

このツアーでは、小さなプログラムを通して値、関数、データ型、借用、失敗値、並列処理を確認します。各コードブロックは独立した Main.tz として実行できます。環境の準備は[入門](get-started.md)を参照してください。

## 値と関数

```tsuzuri run=42
def add :: i64 -> i64 -> i64 = \left right -> left + right

let add_twenty = add 20
22 |> add_twenty
```

`def name :: 型 = ラムダ式` で関数を定義します。関数はカリー化され、一部の引数だけを渡すと残りを受け取る関数値ができます。パイプは値を関数へ渡し、`f >> g` は二つの関数を合成します。最後の値 42 をコンソール用ホストが表示します。

ローカル let は不変です。接尾辞のない整数リテラルは使われ方から型が決まり（この例では `add` の引数なので i64）、決まらない場合は i32、小数は f64 です。`20l`（i64）や `1.5f`（f32）のように接尾辞でも選べます。異なる数値型は明示変換します。

## レコードでデータを表す

```tsuzuri run=42
record Position { horizontal: i64, vertical: i64 } deriving (Eq, Display)

let original = Position { horizontal: 20, vertical: 10 }
let updated = { original with vertical = 22 }
updated.horizontal + updated.vertical
```

レコードは不変の値です。更新構文は値を受け取って新しい値を返し、共有されたフィールドを書き換えません。型が Copy なら元値も使えます。独自の比較や表示は型クラスで、構造的なものは deriving で追加できます。

## 配列と借用

```tsuzuri run=42
let values = [10, 20, 22, 100]
let selected = ref values[1..3]
Array.sum selected
```

配列は `[T]`（`Array<T>` とも書けます）、リストは `[|T|]` です。スライスの終端は含みません。ref は値を複製せず貸し出します。参照が生きている間は所有者を移動・置換できません。

string や Vec は非 Copy の所有値です。値で渡すと move し、読み取りだけなら借用します。GC や手動の free を利用者コードに書く必要はありません。

## 文字列と失敗値

```tsuzuri run=42
let text = "42"
let parsed: Maybe<i64> = Parse.parse ref text
match parsed with
| Maybe.Some value -> value
| Maybe.None -> 0
```

Parse の失敗は None です。match はすべての入力を扱えるか検査します。理由付きの失敗には Result を使います。ゼロ除算や範囲外アクセスのトラップとは別の仕組みです。`@checked` を付けた整数演算のオーバーフローは、同じ関数の `try ... with` で Result として受け取れます（[例外処理](language-reference/error-handling.md)）。

文字列の既定型は UTF-16 の string です。utf8string は u8 接頭辞を使い、長さ・索引の単位も異なります。見た目の文字数と格納単位数を区別します。

## 計算を合成する

```tsuzuri run=42
let result = Maybe {
    let! left = Maybe.Some 20
    let! right = Maybe.Some 22
    return left + right
}
Maybe.default_value 0 result
```

Maybe の let! は成功値を取り出し、None なら続きへ進みません。独自のビルダーは .tc に通常の関数として定義できます。return は任意位置の早期 return ではなく、ビルダーの値を作る末尾の操作です。

## 明示的な並列処理

```tsuzuri run=14
let work = new [Task<i64>](4, index -> task { return index * index })
let result = Task.run (Task.parallel work)
Array.sum ref result
```

task は作成時には実行しません。Task.run が一回消費して実行します。native の Task.parallel は常駐プールを使い、WASM は既定で逐次ですが、どちらも結果を入力順に返します。

タスクの捕捉値・結果は所有値で、借用を外へ持ち出せません。UI をノンブロッキングにする非同期 I/O の仕組みではありません。

## ファイルを分ける

関数が増えたらモジュールへ分けます。Geometry/Point.tz は名前空間 Geometry の Point モジュールで、呼び出しは Geometry::Point.distance のように修飾します（名前空間とモジュールは `::`、メンバーは `.`）。型クラスは .tt、ビルダーは .tc です。モジュールを開く open 宣言はありません。

ホストから使う具体型の関数は export def、ホストを呼ぶ関数は extern def にします。WASM や C の境界には、対応するスカラー・バッファ・レコードだけを渡します。

## 次に読む

- [言語リファレンス](language-reference/README.md): 構文と所有権を機能別に調べる。
- [標準ライブラリ](library-reference/README.md): 各 API の引数順、借用、失敗条件を確認する。
- [F# からの移行](guides/from-fsharp.md): 似た記法の異なる意味を確認する。
- [対応状況](feature-status.md): 実装済みの段階と未対応範囲を確認する。
