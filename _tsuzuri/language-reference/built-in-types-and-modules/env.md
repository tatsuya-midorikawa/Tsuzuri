# Env

`Env` は、コマンドライン引数、環境変数、作業ディレクトリを読むモジュールです。3 つとも `IO<Result<...>>` で、実行するまで OS は見ません。値を書き換える関数はありません。

## この記事のポイント

- `Env.args ()` はプログラム名を除いた引数です。不正な UTF-8 は置換せず、`InvalidEncoding` です。
- `def main :: Array<string> -> i32` の引数は、不正な UTF-8 を `U+FFFD` に置き換えます。
- 未設定の環境変数はエラーではなく `Ok None` です。
- 空の名前、`=` を含む名前、NUL を含む名前は `InvalidInput` です。
- `tsuzuri run` は追加の引数を渡さないので、このコマンドで実行すると `args` は空です。

## 引数と main

エントリーポイントが `def main :: Array<string> -> i32` の場合、起動時のコマンドライン引数がその配列に渡されます。`Array<string>` は `[string]` と同じです。プログラム名（`argv[0]`）は含まれません。引数が渡されなかった場合は空配列となります。

`Env.args ()` も、プログラム名を除いた同じ並びを返します。ただし、以下の 2 点で挙動が異なります。

- `main` の引数は、不正な UTF-8 シーケンスを Unicode 置換文字（`U+FFFD`）に自動置換して受け取ります。一方、`Env.args` は不正な UTF-8 を検出した時点で `InvalidEncoding` エラーを返し、中途半端な配列は返しません。
- `main` の引数は関数のパラメーターとして渡されます。一方、`Env.args` はアクションを実行したタイミングで OS から取得します。ライブラリ出力、`tsuzuri test`、および追加引数を渡さない `tsuzuri run` では空配列となります。

ビルドした実行ファイルに引数を渡すと、両方に同じ個数が入ります。POSIX では、起動元が分けた引数をそのまま受け取ります。Windows ではプロセスが 1 本のコマンドラインを受け取るので、実行ファイル側が空白と引用符で分けます。バックスラッシュに特別なエスケープはありません。

`tsuzuri run` では追加引数を渡せません。下の例の `0` は、そのためです。ビルドしたバイナリで同じコードを動かすと、渡した引数の個数になります。

```tsuzuri run=0%0Anone%0Ainvalid%20input%0Ainvalid%20input%0Atrue
def main :: unit -> i32 = \() ->
    let! args = Env.args ()
    do! IO.write_line (match args with
        | Result.Ok names -> to_string names.length
        | Result.Error error -> Os.message (ref error))
    let! unset = Env.var "TSUZURI_DOC_UNSET_VAR"
    do! IO.write_line (match unset with
        | Result.Ok (Maybe.Some _) -> "some"
        | Result.Ok Maybe.None -> "none"
        | Result.Error error -> Os.message (ref error))
    let! equals = Env.var "A=B"
    do! IO.write_line (match equals with
        | Result.Ok _ -> "unexpected"
        | Result.Error error -> Os.message (ref error))
    let! empty = Env.var ""
    do! IO.write_line (match empty with
        | Result.Ok _ -> "unexpected"
        | Result.Error error -> Os.message (ref error))
    let! directory = Env.current_dir ()
    do! IO.write_line (match directory with
        | Result.Ok text -> to_string (text.length > 0)
        | Result.Error error -> Os.message (ref error))
    0
```

実行結果:

```text
0
none
invalid input
invalid input
true
```

`TSUZURI_DOC_UNSET_VAR` は、この例のために選んだ未設定の名前です。設定されている環境では `some` になります。空の値は未設定ではなく `Some ""` です。

## 環境変数と作業ディレクトリ

`Env.var` は名前を UTF-8 にしてから OS に聞きます。孤立サロゲートは、問い合わせの前に `InvalidEncoding`（`code` は 0）です。値が UTF-8 でないときも `InvalidEncoding` で、置換しません。

空の名前、`=` を含む名前、NUL を含む名前は `InvalidInput`（`code` は 0）です。上の 3 行目と 4 行目がそれです。

`Env.current_dir ()` は、OS が報告した作業ディレクトリです。相対パスの [File](./file.md) や [Dir](./dir.md) は、ここを起点に OS が解決します。表示そのものは実行のたびに違うので、例では空でないことだけを見ています。

環境変数を設定・変更する関数は提供されていません。子プロセスへ渡される環境変数は、親プロセスのものがそのまま継承されます。詳しくは [Process](./process.md) を参照してください。

## 公開 API

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `Env.args` | `unit -> IO<Result<[string], Os.Error>>` | プログラム名を除いた引数。不正な UTF-8 は `InvalidEncoding` |
| `Env.var` | `string -> IO<Result<Maybe<string>, Os.Error>>` | 環境変数。未設定は `Ok None` |
| `Env.current_dir` | `unit -> IO<Result<string, Os.Error>>` | OS が報告した作業ディレクトリ |

返った `string` は所有値です。配列の要素は `Copy` ではないので、`names[0]` をムーブせずに使うには借用するか複製します。

## 対応環境

既定の wasm32 でこれらの関数に到達すると、ビルドは `E2000` で失敗します。`--wasm-host wasi` では、ホストが渡した引数を受け取ります。`current_dir` は最初の preopen ディレクトリの名前です。引数を渡さない WASM 出力では、`main` の引数も `Env.args` も空です。

Windows のネイティブビルドは、OS API に到達すると `E2002` で失敗します。これらの環境では本ページの実行例をそのまま実行することはできません。

## まとめ

- `Env.args` と `main` の配列は、どちらもプログラム名を含みません。
- 不正な UTF-8 を置換するのは `main` の引数だけです。`Env.args` は失敗します。
- 未設定の変数は `Ok None` です。空、`=`、NUL の名前は `InvalidInput` です。
- `tsuzuri run` では引数は空です。引数を試すにはビルドした実行ファイルを使います。
- 作業ディレクトリの文字列は OS の報告そのものです。

## 関連項目

- [IO](./io.md)
- [Os](./os.md)
- [File](./file.md)
- [Process](./process.md)
- [言語仕様のエントリーポイント](../../../docs/language.md#アプリケーションのエントリーポイント)
- [言語リファレンスの目次](../index.md)
