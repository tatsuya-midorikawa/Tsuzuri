# OnceValue

`OnceValue<T>` は、関数で 1 回だけ値を作り、以後はその値を借りて返すセルです（Go の `sync.OnceValue`、TZ06）。共有の初期化（設定の読み込み、表の構築）を、ロックなしに読める形で書けます。

## この記事のポイント

- `OnceValue.create ()` で空のセルを作り、`OnceValue.get cell make` で値を借ります。最初の呼び出しが `make` で値を作り、同時に来たほかの呼び出しは、その終わりを待ちます。`make` は最大 1 回だけ呼ばれます。
- `get` の結果は `ref T` で、セルの借用と同じ寿命です（`Arc.get` と同じ規則。セルより長く持つと `E1013`）。`T` は `Send` かつ `Sync` です。
- 初期化の関数が返す値はセルに残るので、借用を持てる型（関数など）のときは、借用を持たないと示せる値だけを返せます。外の変数を借りる関数や、外の値を借りる初期化の関数は `E1013`（`mutex_result_retains_borrow`）です。関数を入れるときは、初期化の関数の中で自分の値から作ります（`\_unit -> { let step = 3; \x -> x + step }`）。
- 値は、書いてから状態を release で公開し、読む側は acquire で読むので、どの tzroutine も作られた値をそのまま見ます。
- 複数の tzroutine で共有するときは `Arc<OnceValue<T>>` にします。`OnceValue.into_inner` は作られていれば値を `Some` で返します。

## 基本の書き方

```tsuzuri run=7
let cell: OnceValue<i64> = OnceValue.create ()
let first = deref (OnceValue.get (ref cell) (\() -> 7))
let second = deref (OnceValue.get (ref cell) (\() -> 8))
first * 0 + second
```

実行結果:

```text
7
```

2 回目の `get` の関数は呼ばれず、1 回目の値 7 を借ります。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `OnceValue.create` | `unit -> OnceValue<'a>` | 空のセル |
| `OnceValue.get` | `(Send<'a>, Sync<'a>) => ref OnceValue<'a> -> (unit -> 'a) -> ref 'a` | 最初の 1 回だけ関数で値を作り、その値を借りる |
| `OnceValue.into_inner` | `OnceValue<'a> -> Maybe<'a>` | 作られていれば値 |

## 引数の順序

`OnceValue` は資源のハンドルの例外で、セルが先頭の引数です（`OnceValue.get cell make`）。[引数の順序](./argument-order.md)を見てください。

## 型とターゲット

- `OnceValue<T>` は非 Copy の不透明な値です（構築・分解は `E1022`）。セルは配列の中にあるので、ムーブしても動きません。関数値には捕捉できません（`E1005`）。
- ネイティブの macOS・Linux・Windows と既定の WebAssembly（1 本の実行）で使えます。

## 関連項目

- [Once](./once.md)
- [Rc と Arc](./rc.md)
- [言語リファレンスの目次](../index.md)
