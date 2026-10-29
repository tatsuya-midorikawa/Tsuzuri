# 契約（`@requires`・`@ensures`）

契約は、関数の約束を本体の先頭に `bool` の式として書く行です。呼ぶ側が守ること（事前条件）を `@requires`、関数が保証すること（事後条件）を `@ensures` で書きます。同じ行を、コマンドが選ぶモードで、実行時の検査・性質テスト・SMT ソルバーによる証明に使います。

このページでは、契約の行の書き方、意味、4 つのモード、`@old`、性質テストの生成、`tsuzuri prove` が証明できる範囲、診断を説明します。

## この記事のポイント

- `@requires 条件` と `@ensures \r -> 条件` を、トップレベル関数の本体の最初の行から連続して書きます。条件は純粋な `bool` の式です。
- `@requires` は入口で、`@ensures` は出口で評価し、偽ならトラップします（`contract requires failed`・`contract ensures failed`）。
- モードは `off`・`check`・`test`・`prove` です。既定は `build` が `off`、`run`・`test` が `check` です。`off` の生成コードは契約の行がないのと 1 byte も変わりません。
- `tsuzuri test --contracts test` は契約から性質テストを作り、反例を縮小して出します。
- `tsuzuri prove` は整数・`bool`・組・レコードの範囲の関数を SMT ソルバー（z3 か cvc5）で証明し、反例を出します。
- 契約を最適化の前提にすることはありません。証明済みの検査も取り除きません。

## 書き方

```tsuzuri run=8
def clamp :: i64 -> i64 -> i64 -> i64 = \lo hi x ->
    @requires lo <= hi
    @ensures \r -> lo <= r && r <= hi
    if x < lo then lo elif x > hi then hi else x

clamp 0 8 20
```

実行結果:

```text
8
```

- 契約の行は、`=` や `->` の次の行から、本体の文と同じ字下げで連続して書きます。`def f :: T = \x y ->` の形（入れ子のラムダ `\a -> \b ->` なら最も内側）、`fn f x y =` の形、引数のない `def f :: T =` の形で書けます。
- `@ensures` は結果に名前を付けるラムダ `\r -> 条件` を取ります。引数についての行は `\_ -> 条件` と書きます。複数の `@ensures` が別の名前を使ってもよく、どれも同じ結果を指します。
- `@requires` は宣言順に入口で、`@ensures` は宣言順に出口で評価します。行の並びで評価の時点は変わりません。
- `export`・`private`・`rec`・`@cpu` の関数、ジェネリックな関数にも書けます。ジェネリックな関数は、型引数ごとのインスタンスが契約を持ちます。どこからも呼ばれないジェネリックな関数の契約の行も、純粋性と書き込みを検査します（class のメソッドの呼び出しのように型引数で決まるものは、インスタンスごとに検査します）。instance のメソッドと class の既定の実装には書けません（`E1033`）。
- `requires` と `ensures` は予約語ではありません。`@` の直後に書いたときだけ契約の行として読みます。

## 意味

| 時点 | 動き |
| --- | --- |
| 入口 | `@requires` の条件を評価し、偽なら `contract requires failed` でトラップします。位置は契約の行です |
| 本体 | 通常どおりです |
| 出口 | 本体の値に `@ensures` の名前を付けて条件を評価し、偽なら `contract ensures failed` でトラップします |

- 条件の評価は通常の式と同じです（左から右、短絡評価、整数の折り返し、トラップ）。条件そのものがトラップしたら、そのトラップになります。
- 条件は純粋でなければなりません（`pure def` と同じ効果を禁じます。確保とトラップは許します）。入出力・`Debug.print`・`extern`・共有状態・`dyn`・呼び出し元が選ぶ関数値の呼び出しは `E1033` です。引数のレコードの欄に入った関数のように、解析が追えない関数値を呼ぶ関数を呼ぶことも同じです。`pure (T -> U)` 型の関数値は、どこから来ても呼べます。`@old` の式も同じです。
- 条件は、関数が受け取った値と結果を変えられません。`check` と `off` で計算が変わるからです。引数や結果への代入、`ref mut` での借用、`ref mut` や `mut` の引数を取る関数へ渡すこと（`p |> poke` のパイプラインも）は `E1033`（`contract_writes`）です。条件の中の `let q = p` や `q = p` のように引数の参照を持った変数、`deref q = p` で参照の先に引数の参照を入れた変数、`poke (if b then p else p)` のように条件分岐・`match`・ブロックが結果にした参照を通した書き込みも同じです。`ref mut` の欄を持つレコードを `pure` でない関数へ渡すことも、値渡しでも `E1033` です（その関数は欄の参照先を書き換えられます）。`pure` 関数へ渡すことと、レコードへの共有参照（`ref Counter`）を渡すことはできます。どちらも欄の参照先を変えられません。可変の引数を読むこと、条件の中で作った値を変えることはできます。
- 出口では、`ref mut` の引数は本体が更新した後の値です。値渡しの引数を本体が移動していたら、`@ensures` からは読めません（`E1012`）。
- `@ensures` を持つ関数の自己末尾呼び出しは末尾でなくなり、再帰の深さに比例してスタックを使います。`@requires` だけなら末尾呼び出しは保たれます。
- `@ensures` の結果は値として束縛するので、参照を含む結果の関数には書けません（`E1033`）。

### `@old`

`@ensures` の中の `@old (式)` は、その式の入口での値です。コンパイラが入口で評価して保ちます。

```text
def bump :: ref mut [i64..] -> i64 -> unit = \xs v ->
    @ensures \_ -> xs[0] == v && @old (xs[0]) != v
    Array.write 0 v xs
```

`@old` は `@ensures` の中だけで書け（それ以外は `E1033`）、入れ子にできず、結果の名前を読めず、数値や `bool` のような平たい Copy の値だけを取ります。

## モード

| モード | 動き | コスト |
| --- | --- | --- |
| `off` | 型検査・純粋性検査だけをして、コードを出しません | 0（生成 IR は契約の行がないのと同じ） |
| `check` | 入口と出口で条件を評価し、偽ならトラップします | 条件の評価 |
| `test` | `check` に加えて、`tsuzuri test` が契約から性質テストを作ります | テストのときだけ |
| `prove` | 先に SMT ソルバーで証明し、そのあと `check` として続けます | ソルバーの時間 |

| コマンド | `--contracts` | 既定 |
| --- | --- | --- |
| `build` | `off`・`check`・`prove` | `off` |
| `run`・`script` | `off`・`check`・`prove` | `check` |
| `test` | `off`・`check`・`test`・`prove` | `check` |
| `check` | `prove` | 契約は常に型検査されます |
| `doc`・`bench`・`fmt`・`api-diff` | 指定できません | `bench` は `off` |

`off` でも、契約の行を型検査・所有権検査・純粋性検査します（`off` で通って `check` で落ちるプログラムを作りません）。`tsuzuri run` の違反は `E2005`（kind `contract_requires`・`contract_ensures`）で、位置は契約の行です。`tsuzuri test` の失敗の下には `trap: contract ensures failed at Main.tz:3:5` の行が出ます。

## 性質テスト（`--contracts test`）

`tsuzuri test --contracts test` は、契約を持つ関数ごとに試験 `contract <名前>` を作ります。引数を `Arbitrary.arbitrary()` で生成し、`@requires` を満たす値だけで本体を呼び、`@ensures` を確かめます（`Gen.for_all_where`）。違反は縮小した反例で報告します。

```text
not ok 3 - Main contract broken
  property failed after 8 tries (seed 11400714819323198485)
  counterexample: (0, (0, 0))
  shrunk 1 times from (-9223372036854775808, (-471411656769, -15014))
```

本体は、関数と同じ引数を持つ関数の中で、生成した引数を渡して評価します。引数は関数を呼ぶときと同じ規約で渡るので、`pure def` や edition 2027 で既定で借用する引数（`string` など）は、本体と契約の行から参照に見えます。`@ensures` からは `check` と同じものが見えます。本体が借用するだけの所有された引数（`own string` など）も読めます。結果の名前は、その名前を付けた行の中だけで結果を指します。`@ensures \_ -> ...` の行の同じ名前は、引数などの元の値を指します。`@ensures` の中の `@old (式)` は、本体を評価する前に式を評価した値です。

生成できる引数は、整数・`bool`・`f64`・`char`・`string`、それらの配列と 2〜4 個の組、`Arbitrary` と `Display` を導出した同じモジュールのレコード・共用体です。ジェネリックな関数・引数のない関数・生成できない型の引数を持つ関数は試験を作らず、`W1009`（`contract_test_skipped`）で知らせます。`@ensures` の行が結果に別々の名前を付けた関数も同じです。結果の名前と同じ名前の値を読む `_` の行を挟んで、その名前を付けた行が 2 か所以上にある関数も同じです。ただし、結果が数値・`bool`・`char` のときは作ります（結果の束縛が結果を移動しないためです）。`--seed` で再現できます。生成器は [Gen](../built-in-types-and-modules/gen.md) の `Arbitrary` と `deriving (Arbitrary)` です。

## 証明（`tsuzuri prove`・`--contracts prove`）

```text
tsuzuri prove [source.tz|directory] [--function NAME]... [--prove-timeout SECONDS] [--json]
```

関数ごとに、`@requires` が成り立つ入力で本体がトラップしないこと（`trap_free`）と、トラップしない実行で各 `@ensures` が成り立つことを、SMT-LIB の `QF_BV`（ビットベクトル）に翻訳してソルバーに問います。結果は 1 関数 1 行です。

```text
proved Main.clamp
counterexample Main.bad
unsupported Main.sum_to (a loop)
```

反例は `E1034` で、呼び出しと結果を示します（例: `'the body' of 'Main.half' can fail: Main.half (-2147483648) (-1) traps: integer division overflow`）。すべて `proved` なら終了コードは 0、それ以外は 1 です。

- ソルバーは `TSUZURI_SMT`、なければ `z3`、なければ `cvc5` です（同梱しません）。見つからなければ `E1034`（`prove_solver_missing`）です。`--prove-timeout` は問い 1 つあたりの秒数（1〜3600、既定 10）です。
- 証明できるのは、整数・`bool`・組・レコード・ペイロードのない共用体の値、`if`・`match`・`let`、ほかの関数の呼び出し（深さ 8 まで展開）、`Int.min`・`Int.max`・`Int.abs`・`assert` です。ラムダの引数の `when` ガード（`\x when x > 0 -> x`）は網羅性が検査されないので、どの節にも合わない入力は `traps: pattern mismatch` の反例です。ループ・文字列・配列・クロージャ・浮動小数点・再帰・契約の条件の中の除算は `unsupported` です。
- `@requires` が矛盾していれば、すべての問いが空虚に `proved` になります。
- `--function` に契約を持つ関数のどれでもない名前を渡すと、何も証明せずに `E2000`（`prove_unknown_function`）で終わります（ほかの名前と一緒でも同じです）。
- `--contracts prove` は証明が通ったときだけビルドや実行を続けます。証明しても実行時の検査は取り除きません。

## 診断

| コード | いつ出るか |
| --- | --- |
| `E1033` | 契約の行の置き場（instance のメソッドを含む）・型・純粋性・書き込み・結果のラムダ・`@old` の誤り、`const def` の契約 |
| `E1034` | `prove` の反例・非対応・時間切れ・ソルバーの失敗・ソルバーがない |
| `W1009` | 常に真の契約、使わない結果の名前、性質テストを作れない関数 |
| `E1017` | 1 つの関数の契約の行が 32 を超える、証明の台本が 50,000 定義を超える |

`tsuzuri doc` と言語サーバーの hover は契約の行を署名の下に表示します。`tsuzuri api-diff` は `@requires` を足す・`@ensures` を外すのを互換のない変更、`@requires` を外す・`@ensures` を足すのを互換の変更とします。

## まとめ

- 契約は本体の先頭の `@requires`・`@ensures` の行です。条件は純粋な `bool` です。
- `off` は何も生成せず、`check` はトラップで検査し、`test` は性質テストを作り、`prove` はソルバーで証明します。
- 契約は最適化の前提になりません。

## 関連項目

- [関数 / 高階関数 / 再帰関数](./functions.md)
- [属性](./attributes.md)
- [assert](../exception-handling/assert.md)
- [Gen](../built-in-types-and-modules/gen.md)
- [診断](../compiler/diagnostics.md)
- [言語リファレンスの目次](../index.md)
