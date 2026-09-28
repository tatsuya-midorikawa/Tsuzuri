# Option、Result、回復可能な失敗

[ドキュメントのトップ](../README.md)

値がないことは `Option<T>`、理由を持つ失敗は `Result<T, E>` で表します。どちらも標準ライブラリにある通常の union です。null、例外、暗黙のエラー変換を必要としません。

## 型と case

```text
Option<T> = None | Some of T
Result<T, E> = Ok of T | Error of E
```

case は `Option.Some` / `Result.Error` と修飾できます。無修飾名は利用者の同名 case に隠れる可能性があるので、公開例や複数モジュールでは修飾名が明確です。

```tsuzuri run=42
def parse_count :: ref string -> Result<i64, string>
fn parse_count text = Option.to_result "invalid count" (Parse.parse text)

let first = "20"
let second = "22"
let answer: Result<i64, string> = Result {
    let! left = parse_count ref first
    let! right = parse_count ref second
    return left + right
}
match answer with
| Result.Ok value -> value
| Result.Error _ -> 0
```

不正入力の解析は None になり、`Option.to_result` でエラー情報を与えています。Result の let! は最初の Error で継続を止めます。

## 調査と取り出し

| API | 入出力・失敗時 |
| --- | --- |
| `Option.is_some value`, `Option.is_none value` | 共有借用して bool |
| `Result.is_ok value`, `Result.is_error value` | 共有借用して bool |
| `Option.get value`, `Result.get value` | 所有値を消費して成功 payload。不一致ならトラップ |
| `Result.get_error value` | 所有値を消費して Error payload。Ok ならトラップ |
| `default_value fallback value` | 両モジュールに存在。成功値、なければ評価済みの fallback |
| `default_with fallback value` | fallback は `unit -> T`。不在・失敗のときだけ呼ぶ |

get は安全なエラー分岐の代わりにはなりません。不一致は Error や None を返さず、`unreachable ()` によるトラップです。信頼できない入力には match や default 系を使います。

## 変換と連結

| API | 契約 |
| --- | --- |
| `map transform value` | 成功 payload を消費して変換 |
| `bind value next` | 成功 payload を消費して、次の同種 union を返す継続へ渡す |
| `map_ref transform value` | union と成功 payload を共有借用し、所有する結果を作る |
| `bind_ref value next` | 借用した成功 payload から次の union を作る |
| `or_else value fallback` | 不在・失敗のときだけ `fallback ()` を実行 |
| `Option.filter predicate value` | payload を借用して判定し、成立なら元の Some を返す |
| `Result.map_error transform value` | Error payload だけを消費して変換 |

map は関数が先、bind は計算値が先です。不在・失敗の場合、成功用の callback は呼びません。Result の借用版は Error を結果へ複製するため `Copy<E>` が必要ですが、成功値 T の Copy は不要です。

```tsuzuri run=5
let value = Option.Some "hello"
let measured = Option.map_ref String.length (ref value)
assert (Option.is_some ref value)
Option.get measured
```

関数が先の API で匿名関数の型がまだ決まらない場合は、型が明確な関数を渡すか、ラムダ式の引数へ型注釈を付けます。後続引数だけで任意のフィールドアクセスが推論されるとは限りません。

## 型間の変換

| API | 結果 |
| --- | --- |
| `Option.to_result error value` | Some を Ok、None を指定した Error へ |
| `Result.of_option error value` | 同じ変換 |
| `Option.of_result value` | Ok を Some、Error を破棄して None へ |
| `Result.to_option value` | 同じ変換 |

error 引数も通常の厳格評価です。成功時に不要であっても引数式自体は評価されます。Option と Result の間で `?` などによる暗黙変換はありません。

## 所有権

所有版は union を消費し、成功または失敗 payload を move します。Copy の payload を繰り返し使う場合には通常の複製規則が適用されます。共有借用 payload の具体化も元所有者の寿命を引き継ぎ、排他参照は保持できません。

union の値そのものはホスト ABI に渡せません。ホスト境界では対応するスカラーやバッファへ変換します。

## ビルダー操作

両モジュールは `Bind`, `Return`, `ReturnFrom`, `Zero`, `Combine`, `Delay`, `Run`, `For`, `While`, `MergeSources`, `BindReturn`, `Bind2` を公開します。

Zero は成功した unit です。Combine と Bind は失敗時に続きを呼ばず、Result はエラー型を式全体で統一します。MergeSources は左右を評価した後で結合し、Result の両方が Error なら左を選びます。詳細は[コンピュテーション式](../language-reference/computation-expressions.md)を参照してください。

## トラップとの違い

assert の失敗、整数ゼロ除算、範囲外アクセス、確保失敗などは言語内で回復できる失敗値ではありません。Result の中に書いても自動捕捉しません。トラップ時の巻き戻しや解放は保証されません。

## API と関連項目

- [Option のソース宣言](api/Option.md)
- [Result のソース宣言](api/Result.md)
- [Task の結果付き並列実行](../language-reference/tasks.md)
