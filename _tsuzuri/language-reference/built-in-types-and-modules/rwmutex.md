# RwMutex

`RwMutex<T>` は、値 `T` を入れ、多くの tzroutine が同時に `RwMutex.with_read` で読み、1 つずつ `RwMutex.with_write` で書くロックです（Go の `sync.RWMutex` と、それが守る値、TZ06）。読みが多く書きが少ない共有に使います。

## この記事のポイント

- `RwMutex.create value` で作り、`with_read` は共有借用 `ref T`、`with_write` は排他借用 `ref mut T` をコールバックに渡します。ロックはコールバックが終わると外れます。
- 読み手は何人でも同時に持てます。書き手は 1 人だけで、そのとき読み手はいません。書き手が待っている間、新しい読み手は待つので、読みが続いても書き手は飢えません（Go と同じ方針）。
- 同じ `RwMutex` を、それを持っている tzroutine がもう一度取ると（読みの中の読みも、書きの中の読みも）、`Tsuzuri runtime: RwMutex cannot be locked again by the tzroutine that holds it` を書いてトラップします。書き手が待っていると自分を待つからです（Go の文書の注意と同じ）。
- 結果は `Send` で、ロック中の値への借用を持てません（`Mutex.with_lock` と同じ `E1013`）。
- ロックを持ったまま、チャンネルや別のロックを待ってよい（待つのは tzroutine だけです）。

## 基本の書き方

```tsuzuri run=15
let lock = RwMutex.create 5i64
RwMutex.with_write (ref lock) (\value -> deref value = deref value * 3)
RwMutex.with_read (ref lock) (\value -> deref value)
```

実行結果:

```text
15
```

複数の tzroutine で共有するときは `Arc<RwMutex<T>>` にします（[Mutex](./mutex.md) と同じ）。`RwMutex<T>` を `Sync` にするには `T` も `Sync` でなければなりません。読み手が同時に `ref T` を持つからです（`Mutex<T>` は `T` が `Send` なら `Sync`）。

## API リファレンス

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `RwMutex.create` | `Send<'a> => 'a -> RwMutex<'a>` | 値を入れたロック |
| `RwMutex.with_read` | `Send<'b> => ref RwMutex<'a> -> (ref 'a -> 'b) -> 'b` | 読みのロック。読み手は同時に持てる |
| `RwMutex.with_write` | `Send<'b> => ref RwMutex<'a> -> (ref mut 'a -> 'b) -> 'b` | 書きのロック。1 人だけ |
| `RwMutex.into_inner` | `RwMutex<'a> -> 'a` | ロックを消費して値を返す |

## 引数の順序

`RwMutex` は資源のハンドルの例外で、ロックが先頭の引数です（`RwMutex.with_read lock callback`）。[引数の順序](./argument-order.md)を見てください。

## 表現とターゲット

- セルは、64 ビットのロックの語と値を持つ 1 要素の配列です。語の下位 62 ビットが読み手の数、ビット 62 が「書き手が待っている」、ビット 63 が「書き手が持っている」です。読みの解放で最後の読み手が書き手を、書きの解放が全員を起こします。
- ネイティブの macOS・Linux・Windows と既定の WebAssembly（1 本の実行。待つべきロックはトラップ）で使えます。

## 関連項目

- [Mutex](./mutex.md)
- [SyncMap](./syncmap.md)
- [言語リファレンスの目次](../index.md)
