# 所有権、借用、スタックとヒープ

[ドキュメントのトップ](../README.md)

Tsuzuri は GC、参照カウント、言語内の手動解放を使いません。所有値には一つの所有者があり、値が不要になるとコンパイラが適切な解放処理を生成します。借用は所有権を渡さずに値へアクセスする仕組みです。

## move と Copy

`string` などの非 Copy 値を別の束縛、引数、返却値へ渡すと所有権が移動します。移動元を再使用すると `E1012` です。関数へ値で渡すと、その引数は呼び出し先が所有します。

| 値 | 複製の扱い |
| --- | --- |
| 数値、bool、unit、文字 | Copy |
| 共有参照 `ref T` | Copy。参照先を複製するわけではない |
| 関数値 | Copy。捕捉環境の深い複製を伴う場合がある |
| レコード、タプル、配列、リスト、非再帰 union | 全フィールド・要素・payload が Copy なら Copy |
| string、utf8string | 非 Copy |
| Vec、Map、Set、Seq、Task、再帰型 | 非 Copy |
| `Drop` を持つ record・union | 非 Copy。[利用者定義の解放](#利用者定義の解放drop)を参照 |
| 排他参照 `ref mut T` | 非 Copy |

Copy 配列やリストの複製は独立したバッファやノードを作ります。Copy は「後から元値を使える」という意味であり、O(1) や共有記憶域を意味しません。

レコードのフィールドは個別に move できます。残ったフィールドは使用・解放できますが、部分 move 後のレコード全体は使えません。非 Copy 要素を配列やリストの添字から move することはできません。

## 共有借用

```tsuzuri run=10
def length :: ref string -> i64 = \text -> text.length

let text = "hello"
let first = length ref text
let second = length text
first + second
```

`ref text` は読み取り専用の借用です。`&text` も使えます。上の二回目は引数型から借用を補っており、文字列を move も複製もしていません。

共有借用は複数存在できます。参照が生きている間、その元の値を move、置換、排他借用できません。生存期間は原則として最後の使用までで、必ずブロックの末尾まで継続するわけではありません。

## 排他借用と置換

```tsuzuri run=7
def replace :: ref mut string -> string -> unit = \target replacement -> { deref target = replacement; }

let mut text = "old"
let shared = ref text
let before = shared.length
replace (ref mut text) "next"
before + text.length
```

`ref mut text` は排他的な借用で、元の束縛には `mut` が必要です。`deref target = replacement` は参照先の値全体を置換し、古い所有値を解放します。排他参照自身を可変に束縛する必要はありません。

同時に使えるアクセスは一つだけです。上の例では `shared` の最後の使用が排他借用より前なので許可されます。`shared` を置換後にもう一度使う形は拒否します。

配列要素、リスト要素、文字列のコード単位、レコードのフィールドはこの構文で直接書き換えられません。コレクションやレコードの所有更新 API を使います。

## 再借用

`ref reference` / `ref mut reference` は、被演算子の静的型が参照なら一段貸し直します。元の排他参照は、再借用の最後の使用まで利用できません。共有参照から排他参照は作れません。

| 目的 | キーワード形式 | 記号形式 |
| --- | --- | --- |
| 所有値の共有借用 | `ref value` | `&value` |
| 所有値の排他借用 | `ref mut value` | `&mut value` |
| 参照先を読む | `deref reference` | `*reference` |
| 共有として再借用 | `ref reference` | `&*reference` |
| 排他として再借用 | `ref mut reference` | `&mut *reference` |

記号形式の `&reference` は参照自身の借用であり、キーワードの再借用と同じではありません。抽象的な型変数を扱う場合も、参照かどうかを推測して意味を変更しません。必要に応じて型注釈や明示的な記号形式を使います。

## 呼び出し時の補完

引数型が分かる場合、所有値から参照への借用、参照の再借用、値引数への参照外しを補います。これは関数値、組み込み関数、型クラスメソッド、部分適用にも共通です。

所有する非 Copy 値を参照から取り出すことは、補完があってもできません。未確定の型変数に対して借用を推測することもありません。代入や返却値の型合わせには、この呼び出し専用の補完を適用しません。

一時的な所有値の借用や寿命延長は行いません。例えば計算で作った文字列を借用したい場合は、先に `let` で束縛します。

## 記憶域と new

```tsuzuri run=42
let local = [20, 22]
let heap = new [20, 22]
assert (local == heap)
Array.sum ref local
```

`new` なしで束縛する配列・リストのリテラルは、原則として関数フレームに置きます。`new` を使うコレクションと、実行時長の生成はヒープです。文字列リテラルは静的な読み取り専用領域を参照できます。再帰 union のノードは暗黙のヒープ確保です。

`new` はレコードやタプルを個別にヒープへ置くための構文ではありません。型も記憶域によって変わらず、スタックとヒープの配列は同じ `[T]` です。

スタックにあるコレクションを戻り値、値引数、別の束縛、捕捉などへ移すと、その時点でヒープへ移送します。共有借用や読み取りは元の領域を使えます。最初から返却を予定しているコレクションには `new` が適する場合があります。

一つのリテラルの保守的なサイズが 64 KiB を超える場合は自動的にヒープへ置きます。これは深い再帰のスタック枯渇をすべて防ぐ保証ではありません。

## 借用を返す関数

借用結果は入力由来でなければなりません。関数ローカルな所有値の参照を返すことはできません。省略形では借用を含む全入力の寿命を保守的に保持し、直接の完全適用では名前付き region により返却元を指定できます。

借用を保持する配列、レコード、関数値を経由しても、元の所有者より長生きできません。排他参照をコレクションや再利用可能な関数値へ保存することも禁止します。

## 利用者定義の解放（Drop）

ファイルやホストのハンドルのようなメモリ以外の資源は、record・union に `Drop` のインスタンスを書くと、値が終わる時点で一度だけ解放できます。下の例は解放のたびに id を Debug 出力（native では stderr）へ書き、`2`、`1` の順に表示します。

```tsuzuri run=3
record Resource { id: i64, name: string }

instance Drop<Resource> {
    fn drop value =
        let id = value.id
        Debug.print id
}

def main :: i64 =
    let first = Resource { id: 1, name: "a" }
    let second = Resource { id: 2, name: "b" }
    first.id + second.id
```

`drop` は `ref mut` で値を受け取り、その後で field が宣言順に解放されます。scope の中の値は束縛の逆順です。scope の終わり、`break`・`continue`、代入で置き換わる古い値、捨てた一時値、コレクションの要素、未実行の Task の捕捉値でも同じで、move 済みの値では呼ばれません。トラップは巻き戻さないので、トラップ後の `drop` は走りません。

Drop を持つ型は field の型によらず Copy ではなく、通常の関数値に捕捉できません（`task` へは送れ、関数値には[資源を持つ関数値](#資源を持つ関数値)の方法で持たせます）。`drop` が完全な値を見られるように、次の書き方は拒否します。

```text
fn take_name value = value.name                // E1012: Drop 型から string の field を move
fn rename value = { value with name = "c" }    // E1012: Drop 型の更新
fn close value = Drop.drop (ref mut value)     // E1016: Drop.drop は自動で呼ばれる
instance Drop<i64> { fn drop _value = () }     // E1016: 利用者が宣言した record・union だけ
fn drop value = reset value                    // E1012: drop の中で値を ref mut で渡す（置き換えると drop がまた走る）
```

Copy の field の読み出し、`ref` による借用、値全体の move はできます。ホストのハンドルを閉じる例は[外部関数](../guides/native-interop.md#ハンドルを自動で閉じる)にあります。

### use 束縛と早期解放

`use` は `let` と同じ不変の束縛で、値の型が `Drop` を持つことを確かめます（持たなければ `E1005`）。解放の時点は `let` と同じ scope の終わりです。`Owned.drop` は値を消費して、その場で解放します。下の例は `3`、`2`、`1` の順に表示します。

```tsuzuri run=6
record Resource { id: i64 }

instance Drop<Resource> {
    fn drop value =
        let id = value.id
        Debug.print id
}

def main :: i64 =
    use first = Resource { id: 1 }
    use second = Resource { id: 2 }
    let third = Resource { id: 3 }
    Owned.drop third
    first.id + second.id + 3
```

計算式と `task` では `use! name = source` が `let!` と同じく値を取り出し、`use` で束縛します。計算式の `let!`・`use!` より後ろは継続の関数値になるので、その前に束縛した Drop 型の値を後ろで使うと捕捉になり `E1005` です。

### 資源を持つ関数値

Drop 型の値を捕捉する関数値は、`Owned.function` の引数にラムダを直接書いて作ります。`Owned.call (ref add) x` は捕捉した値を借用したまま何度でも呼べ、`add` の終わりに捕捉した値を一度だけ解放します。下の例は最後に `3` を表示します。

```tsuzuri run=36
record Resource { id: i64 }

instance Drop<Resource> {
    fn drop value =
        let id = value.id
        Debug.print id
}

def main :: i64 =
    let held = Resource { id: 3 }
    let add = Owned.function (\amount -> amount + held.id)
    Owned.call (ref add) 10 + Owned.call (ref add) 20
```

`Owned.Function<'a, 'b>` は Copy ではなく、通常の関数値にも捕捉できません。ラムダの引数は一つで、捕捉できるのは `task` と同じく所有値だけです。本体は捕捉した値を読むか `ref` で借用するだけで、move すると `E1012` です。変数やパイプを通したラムダは通常の関数値の規則に従います。

## 関連項目

- [名前付き lifetime と借用フィールド](lifetimes.md)
- [クロージャーの捕捉](functions.md)
- [レコードの更新](records.md)
