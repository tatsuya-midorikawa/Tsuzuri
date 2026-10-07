# 名前付き lifetime と借用フィールド

[ドキュメントのトップ](../README.md)

借用には、元の所有者が有効である期間という制約があります。通常はコンパイラが追跡しますが、複数の借用入力のうちどれを返すかを API で指定したい場合は、名前付き region を使います。

## 返却元を指定する

```tsuzuri run=5
def first {r s} :: ref {r} string -> ref {s} string -> ref {r} string = \left right ->
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

def view {r} :: ref {r} i64 -> View<i64> {r} = \value -> View { value: value }

let owner = 42
let borrowed = view ref owner
deref borrowed.value
```

共有借用フィールドは元の値を所有しません。レコードをコピー・移動した場合、部分 move、入れ子のレコード、クロージャー捕捉を経由した場合も、借用の依存関係を保持します。

所有者より長生きする返却とタスクへの送信は拒否します。借用レコードの存在によって共有可変状態を作れるわけではありません。排他借用のフィールドは次の節のとおり、region を付けて宣言します。

## 排他借用フィールド

```tsuzuri run=22
record Counter {r} { value: ref mut {r} i64, step: i64 }

def add_to :: ref mut i64 -> i64 -> unit = \target amount -> { deref target = deref target + amount; }

let mut total = 1
let counter = Counter { value: ref mut total, step: 20 }
add_to counter.value counter.step
let again = ref mut counter.value
deref again = deref again + 1
total
```

`ref mut {r} T` のフィールドは、可変の作業領域への排他借用を状態と一緒に持ち運ぶ view です（Rust の `struct Counter<'a> { value: &'a mut i64 }` に当たります）。レコードが生きている間は `total` を直接読めず、最後の使用の後に借用が終わります。

- フィールドには region を名前で書きます（`ref mut i64` だけのフィールドは `E1013`）。排他の region は一つのフィールドの一つの位置だけが使い、共有借用のフィールドには別の region を付けます（`record Mixed {r s} { value: ref mut {r} i64, name: ref {s} string }`）。排他借用を持つレコードを入れ子にするフィールドも region を適用します（`counter: Counter {r}`）。
- `counter.value` を `ref mut i64` の引数へ渡すと一段の再借用になり（Rust の `&mut *counter.value`）、`counter` は `let mut` でなくてかまいません。再借用が生きている間は、同じ参照先への別の経路と `counter` の move を拒否します。別のフィールドの読み出しはできます。
- `let taken = counter.value` は再借用ではなく部分 move です。以後 `counter` 全体と `counter.value` は使えません。
- 共有参照（`ref Counter`）を通して排他フィールドを変更・排他再借用すると `E1014` です。読み出しはできます。
- 排他借用を持つレコードは Copy でなく、再利用する関数値への捕捉、タスクへの送信、配列・リスト・Vec の要素、union の payload、`export` の境界には使えません。ローカル・引数・戻り値・タプルの成分にはできます。
- 配列・タプル・union・共有参照の内側に排他参照を置くフィールドや、借用を持つ型を指す排他参照のフィールドは `E1013` です（参照先の借用は次の節のとおり region を分けて書きます）。

## 参照先の region と参照経由の置換

```tsuzuri run=65
record Note {r} { text: ref {r} string }

def swap_text {r s} :: ref mut {r} Note {s} -> ref {s} string -> ref {s} string = \target text ->
    let old = (deref target).text
    deref target = Note { text: text }
    old

let first = "alpha"
let second = "beta!!"
let mut note = Note { text: ref first }
let old = swap_text (ref mut note) (ref second)
note.text.length * 10 + old.length
```

`ref mut {r} Note {s}` の `{r}` は参照自体の region、`{s}` は参照先の `Note` が持つ借用の region です。参照先から読んだ借用は `s` を持つので、`old` は `note` ではなく `first` だけを借用します。参照を経由して借用を持つ値を置換できるのは、置換する値の借用がすべて参照先と同じ region（ここでは `s`）を持つ入力に由来する場合です。呼び出し側では、`s` を持つ入力（`second`）の借用を `note` が保持するものとして扱います。したがって `second` が `note` より先に終わると `E1013` です。

- ローカルの所有者を指す参照を経由した置換は、region を書かなくても、新しい値の借用を所有者が古い借用と合わせて保持します。
- 共有参照にも書けます（`ref {r} Note {s}`）。参照先から読んだ借用は `s` の入力だけを借用するので、参照や `Note` より長く使えます。
- レコードのフィールドにも書けます（`record Editor {r s} { note: ref mut {r} Note {s} }`）。引数の `Editor {r s}` を経由した置換も、`s` を持つ入力の借用だけを受け付けます。
- 参照先へ入力を格納しうる関数（`s` を持つ入力がほかにある関数）は、すべての引数を渡す直接呼び出しだけができます（関数値・部分適用は `E1013`）。
- 参照先の region は参照の region と別の名前にします（`ref mut {r} Note {r}` は従来どおり一つの region）。参照先は排他借用を持てません。region 付きの関数型の中には書けません。

## 独立した複数の region を持つレコード

```tsuzuri run=7
record Pair {r s} { left: ref {r} string, right: ref {s} string }

def make_pair {r s} :: ref {r} string -> ref {s} string -> Pair {r s} = \left right -> Pair { left: left, right: right }
def left_of {r s} :: Pair {r s} -> ref {r} string = \pair -> pair.left

let long = "abcdefg"
let kept = { let short = "xy"; left_of (make_pair (ref long) (ref short)) }
kept.length
```

レコードは 16 個までの region を宣言できます。使う側では宣言の順に同じ数の名前を書き（`Pair {r s}`）、一番目の名前が宣言の一番目の region に対応します。`left_of` の結果は `r` の slot、つまり `long` の借用だけを持つので、`short` のブロックの外へ返せます。`pair.right` を返すと `short` の寿命なので `E1013` です。

複数の region を持つレコードでは、借用を格納するフィールドは型に region を一つ書きます（`ref {r} T`）。一つのフィールド・引数・結果に異なる region を混ぜられるのは、型全体が複数 region のレコードの直接適用（`Pair {r s}`、入れ子なら `pair: Pair {b a}`）の場合だけです。`ref Pair {r s}` や `(ref {r} T, ref {s} U)` は `E1013` です。

コンパイラが region ごとに借用を分けるのは、レコードの作成と更新、フィールドの読み出し、不変の束縛と引数、`if`／`match` の合流、名前付き関数の直接の完全適用です。`let mut` の束縛、ループで合流する値、関数値・部分適用、クロージャーの捕捉、配列・リスト・タプル・union への格納を経由すると、すべての region の借用を保持します（安全側の扱いです）。

## 関数値を経由する場合

名前付き契約が入力を絞り込むのは直接の完全適用です。関数値や部分適用に契約を持ち上げることはなく、それらを経由すると通常どおり借用を含む全入力の寿命を保持します。

「直接呼び出すと通るのに、高階関数へ渡すと借用が長く残る」という場合は、この制限を確認してください。契約を破って参照を短命にするのではなく、呼び出しを直接にするか、所有値を返す API を検討します。

### region で量化した関数型の引数

名前付き関数の引数では、関数型の前に region の並びを書いて、渡される関数の契約を型にできます。

```tsuzuri run=7
def keep {r} :: ({s t} ref {s} string -> ref {t} string -> ref {s} string) -> ref {r} string -> ref string -> ref {r} string = \f kept other -> f kept other
def first {s t} :: ref {s} string -> ref {t} string -> ref {s} string = \left _right -> left

let long = "abcdefg"
let mut other = "xy"
let kept = keep first (ref long) (ref other)
other = "changed"
kept.length
```

`({s t} ref {s} string -> ref {t} string -> ref {s} string)` は「一番目の入力だけを借用して返す関数」です。`keep` の本体で `f` を全引数で呼んだ結果は `kept` の借用だけを持つので、`keep` は結果の region を `kept` の `r` と宣言できます。呼び出し後に `other` を置換しても `kept` は有効です。

渡せる関数は、named region の契約がこの型を満たす名前付き関数（先頭の引数を部分適用したものを含む）、本体が契約を満たすラムダ、同じ量化型を持つ引数です。ラムダの捕捉は入力ではないので、捕捉した借用を返すラムダや、名前のない入力から返す関数を渡すと `E1013` です。量化型の引数を持つ関数は、すべての引数を渡す直接呼び出しだけができます（関数値・部分適用は `E1013`）。

量化した region は関数自身の region と別の名前にします。量化型は引数の型全体にだけ書け、戻り値・ローカルの型注釈・フィールド・入れ子の型には書けません。その引数は `mut` にできません。

## 現在の制限

| 項目 | 現在の対応 |
| --- | --- |
| 一つの引数・結果 | 一つの region。型全体が複数 region のレコードの直接適用なら、その全 region |
| 一つのレコード | 16 個までの region |
| レコードの独立した複数 region | 対応。可変の束縛・ループ・関数値・コレクションを経由すると全 region を保持 |
| 関数値型に region を保持 | 名前付き関数の引数の型全体だけ（region で量化した関数型） |
| 排他借用フィールド | 対応（region は必須。参照先の借用は別の region に書く。配列・タプル・union・共有参照の内側には置けない。`ref mut {r} [T..]` の排他スライスも置ける） |
| 排他スライスの分割 | `Array.split_at_mut` の二つの半分を一つの借用として扱う。片方を貸し直している間はもう片方も使えない（別々の `let` で使い終えてから貸し直す） |
| static lifetime | なし |
| alias、union、const、ローカル型注釈への region 指定 | 未対応 |
| 高階関数型の内部への region 指定 | 引数の型全体の量化だけ |
| 参照経由の借用 aggregate の置換 | 対応（ローカルの所有者、または参照先の region を名前で書いた引数） |
| 複数 region のレコードの `let mut` 束縛 | 全 region をまとめて扱う。共有フィールドの読み出しと排他フィールドの再借用を同じ式で行うと `E1014` になることがある（先に `let` で読み出す） |

未宣言・未使用の region や、スカラー型への region 指定も `E1013` です。region を省略した API は従来の寿命推論を使います。Rust の全 lifetime 機能と同等ではありません。

## 関連項目

- [所有権と借用](ownership.md)
- [レコード](records.md)
- [正式な region 契約](../../docs/language.md#名前付き-region)
