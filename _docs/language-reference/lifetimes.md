# 名前付き lifetime と借用フィールド

[ドキュメントのトップ](../README.md)

借用には、元の所有者が有効である期間という制約があります。通常はコンパイラが追跡しますが、複数の借用入力のうちどれを返すかを API で指定したい場合は、名前付き region を使います。

## 返却元を指定する

```tsuzuri run=5
def first {r s} :: ref {r} string -> ref {s} string -> ref {r} string
fn first left right =
    assert (right.length > 0)
    left

let left = "hello"
let mut right = "temporary"
let selected = first (ref left) (ref right)
right = "changed"
selected.length
```

`{r s}` は region の宣言です。返却値の `{r}` は最初の入力を借用元に指定し、二番目の `{s}` と独立させています。したがって直接の完全適用後に `right` を置換しても、`selected` は有効です。

region 名は小文字で始まる ASCII 識別子で、型変数とは別の名前空間です。宣言の区切りにはカンマも使えます。複数の入力に同じ region を付けた場合は、それらすべての寿命を保持します。

本体が指定した入力以外の借用を返した場合は `E1013` です。region はコンパイラへの無検査の約束ではなく、検査される契約です。

## 借用を格納するレコード

```tsuzuri run=42
record View<'value> {r} { value: ref {r} 'value }

def view {r} :: ref {r} i64 -> View<i64> {r}
fn view value = View { value: value }

let owner = 42
let borrowed = view ref owner
deref borrowed.value
```

共有借用フィールドは元の値を所有しません。レコードをコピー・移動した場合、部分 move、入れ子のレコード、クロージャー捕捉を経由した場合も、借用の依存関係を保持します。

所有者より長生きする返却、タスクへの送信、排他借用フィールドは拒否します。借用レコードの存在によって共有可変状態を作れるわけではありません。

## 関数値を経由する場合

名前付き契約が入力を絞り込むのは直接の完全適用です。関数値や部分適用に契約を持ち上げることはなく、それらを経由すると通常どおり借用を含む全入力の寿命を保持します。

「直接呼び出すと通るのに、高階関数へ渡すと借用が長く残る」という場合は、この制限を確認してください。契約を破って参照を短命にするのではなく、呼び出しを直接にするか、所有値を返す API を検討します。

## 現在の制限

| 項目 | 現在の対応 |
| --- | --- |
| 一つの引数・結果 | 一つの共有 region |
| 一つのレコード | 一つの共有 region |
| レコードの独立した複数 region | 未対応 |
| 関数値型に region を保持 | 未対応 |
| 排他借用フィールド | 未対応 |
| static lifetime | なし |
| alias、union、const、ローカル型注釈への region 指定 | 未対応 |
| 高階関数型の内部への region 指定 | 未対応 |
| 参照経由の借用 aggregate の置換 | 未対応 |

未宣言・未使用の region や、スカラー型への region 指定も `E1013` です。region を省略した API は従来の寿命推論を使います。Rust の全 lifetime 機能と同等ではありません。

## 関連項目

- [所有権と借用](ownership.md)
- [レコード](records.md)
- [正式な region 契約](../../docs/language.md#名前付き-region)
