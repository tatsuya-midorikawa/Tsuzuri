# FnPtr

`FnPtr<F>` は、環境を捕捉しないトップレベル関数の C ABI ラッパーを指す関数ポインタの値です。Tsuzuri 側のデータ構造に関数ポインタを保持したり、ネイティブ `extern` の引数として C 側へ渡したりするときに使います。

`FnPtr` という利用者のモジュール名は予約され（`E1011`）、同名の型宣言は組み込み型との重複（`E1001`）になります。

## この記事のポイント

- `FnPtr.of f` は、型パラメーターも捕捉も持たないトップレベルのユーザー関数 `f` から `FnPtr<F>` を作ります。ラムダ・部分適用・ローカル関数・標準ライブラリ関数・`extern` は `E1008` です。
- 型引数の `F` には C の静的コールバックとして成立する関数型を書きます。`pure` や引数規約（`own`）は型の上では正規化され、同じ引数・戻り値列の `FnPtr` は同じ型になります。
- `FnPtr<F>` は 1 ポインタの平たい値で、`Copy`・`Capture`・`Send`・`Sync` を満たします。レコードのフィールドや配列の要素にも置けます。ただし Tsuzuri の関数値 `(A -> B)` とは別の型で、Tsuzuri 側から直接 `p x` のように呼ぶことはできません。
- ネイティブターゲット専用です。WebAssembly で到達すると `E1008`（`abi_native_only`）になります。
- 生成される C ABI ラッパーは [静的コールバック](../compiler/native-interop.md#静的コールバック) と共有され、同じ寿命の契約（`main` が返る前はどの OS スレッドからでも呼べ、`main` が返った後はトラップ）に従います。

## API

```text
FnPtr<'f>
FnPtr.of :: 'f -> FnPtr<'f>
```

### 引数の順序

`FnPtr.of` の引数は対象の関数名 1 つだけなので、順序の問題は起こりません（[標準ライブラリの引数の順序](argument-order.md)）。

## 基本の書き方

トップレベルの関数を `FnPtr.of` で関数ポインタにし、Tsuzuri のレコードや変数に格納できます。

```tsuzuri run=stored
record ClassSpec {
    style: i32u,
    proc_ptr: FnPtr<(i64 -> i32u -> i64u -> i64 -> i64)>,
    extra: i32,
}

def wnd_proc :: i64 -> i32u -> i64u -> i64 -> i64 = \_hwnd msg _wparam _lparam ->
    if msg == 2i32u then 0 else 1

let spec = ClassSpec {
    style: 3i32u,
    proc_ptr: FnPtr.of wnd_proc,
    extra: 0i32,
}
if spec.style == 3i32u then "stored" else "bad"
```

実行結果:

```text
stored
```

同じ関数を `extern` のコールバック引数と `FnPtr.of` の両方に渡しても、出力される C ABI ラッパー関数は 1 つに共有されます。

## 型の正規化と検査

`FnPtr<F>` に書ける `F` は、静的コールバックと同じく、引数が C ABI 互換のスカラー・不透明ハンドル（ネイティブでは `Maybe<H>`・`Maybe<ref H>` も可）、戻り値がスカラーまたは `unit` の関数型です。カリー化された多引数関数型 `A -> B -> C` は C の多引数関数ポインタ `C (*)(A, B)` として正規化されます。

```tsuzuri
extern type copy Hwnd

type WndProc = FnPtr<(Hwnd -> i32u -> i64u -> i64 -> i64)>

@trusted
extern "DefWindowProcW" def def_window_proc :: Hwnd -> i32u -> i64u -> i64 -> i64

def my_proc :: Hwnd -> i32u -> i64u -> i64 -> i64 = \hwnd msg wp lp ->
    def_window_proc hwnd msg wp lp

let _ptr: WndProc = FnPtr.of my_proc
```

`extern type local` のハンドルを触る関数（`pinned` 効果を持つ関数）も `FnPtr.of` に渡せます。C 側から呼ばれたときは、その呼び出しを行った OS スレッドの上で実行されます（[ネイティブ連携](../compiler/native-interop.md#スレッド拘束ハンドルextern-type-local)）。

## ホスト境界と C ヘッダーでの表現

`FnPtr<F>` は Tsuzuri からネイティブ C 関数への一方向（`extern` の引数）にだけ渡せます。ホストから任意アドレスを受け取る経路（`extern` の戻り値や `export def` の引数・戻り値、およびホスト境界を越えるスカラーレコードのフィールド）は `E1008`（`fnptr_from_host` など）で拒否されます。リンク名のないネイティブ `extern def` の引数に `FnPtr<F>` を使った場合、`--emit header` では対応する C の関数ポインタ宣言子（たとえば `int64_t (*arg0)(int32_t)`）として出力されます。NULL の関数ポインタを作る手段は Tsuzuri 側にはなく、`FnPtr.of` が返す値は常に有効なラッパー関数を指します。

## 関連項目

- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [Anchor](anchor.md)
- [ZString](zstring.md)
