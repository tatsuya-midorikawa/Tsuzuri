# IO と標準入出力

[ドキュメントのトップ](../README.md)

`IO<T>` は、実行すると T を返す入出力アクションです。Haskell の IO と同じく、アクションを作ることと実行することを分離します。合成には既存の F# 風コンピュテーション式を使い、世界状態の引数やモナド演算子を手書きする必要はありません。

## 最初の出力

```tsuzuri run=42
IO {
    do! IO.write_line 42
}
```

この例は独立した Main.tz です。入口が返す `IO<T>` を実行環境が一度実行し、結果を解放します。通常の値を返す入口と異なり、追加の結果表示や改行はありません。

引数なしの `def main :: IO<unit> = IO { ... }` でも同じ動作です。IO を作るだけでは実行しません。例えば `let _unused = IO.write_line "unused"` は何も出力しません。

## 読み書きする

```tsuzuri
def main :: IO<unit> = IO {
    do! IO.write "Name: "
    let! line = IO.read_line ()
    let name = Option.default_value "world" line
    do! IO.write_line ("Hello, " + name + "!")
}
```

[同梱サンプル](../../examples/io/Main.tz)をリポジトリルートから実行できます。

```sh
./target/release/tsuzuri run examples/io
printf 'Tsuzuri\n' | ./target/release/tsuzuri run examples/io
```

`let!` はアクションの結果を束縛し、`do!` は `IO<unit>` を実行して次へ進みます。通常の let、式、関数呼び出しには `!` を付けません。`return value` は値を IO に包み、`return! action` は別の IO を続けます。文は改行で区切れます。

IO ブロックを省略し、Option の短絡も同じ本体で使えます。

```tsuzuri
def main :: IO<Option<unit>> =
    let! line = IO.read_line ()
    let! value = line
    do! IO.write_line value
```

EOF なら二つ目の let! で残りの文を中断します。結果の型は `IO<Option<unit>>` です。
Result や独自ビルダーの失敗値も保持します。入口は None／Error を表示せず終了コード 0 で終えるため、
失敗を報告する場合は名前付き関数から IO の結果を受け取り、match で処理してください。

## API

| API | 結果 | 動作 |
| --- | --- | --- |
| `IO.read_line ()` | `IO<Option<string>>` | stdin から一行読む。EOF は None |
| `IO.write value` | `IO<unit>` | stdout へ改行なしで表示 |
| `IO.write_line value` | `IO<unit>` | stdout へ表示して LF を追加 |
| `IO.write_error value` | `IO<unit>` | stderr へ改行なしで表示 |
| `IO.write_error_line value` | `IO<unit>` | stderr へ表示して LF を追加 |
| `IO.try_read_line ()` | `IO<Result<Option<string>, IO.Error>>` | 読み取り失敗を値として返す |
| `IO.try_write value` / `IO.try_write_line value` | `IO<Result<unit, IO.Error>>` | stdout の失敗を値として返す |
| `IO.try_write_error value` / `IO.try_write_error_line value` | `IO<Result<unit, IO.Error>>` | stderr の失敗を値として返す |
| `IO.pure value` | `IO<T>` | 入出力せず値を返す |
| `IO.bind action next` | `IO<U>` | T を返す action の後に `T -> IO<U>` を実行 |
| `IO.map transform action` | `IO<U>` | action の結果を `T -> U` で変換 |

出力は `Display<T>` と `Capture<T>` を要求します。文字列だけでなく整数・浮動小数点数・bool・Display インスタンスを持つ型を表示できます。値はアクションへ所有権を渡すので、後でも必要な非 Copy 値は明示的に複製します。表示への変換自体も実行時まで遅延します。

`IO.Error` のケースは `ReadFailed`、`WriteFailed`、`InvalidEncoding` です。try のない API はエラー時にトラップします。EOF はエラーではありません。資源枯渇、ホスト ABI 違反、プロセスへのシグナルは Result に変換しません。

```tsuzuri run=invalid%20encoding
IO {
    let! result = IO.try_write "\uD800"
    let message = match result with
        | Result.Error IO.InvalidEncoding -> "invalid encoding"
        | Result.Error _ -> "write failed"
        | Result.Ok _ -> "written"
    do! IO.write_line message
}
```

## 文字と順序

native の入出力は UTF-8 バイト列です。読み取った行を UTF-16 の string に変換し、不正 UTF-8 は InvalidEncoding にします。出力時の孤立サロゲートも InvalidEncoding で、暗黙の置換は行いません。

LF と CRLF は行末から除きます。空行は `Some ""`、読み取り開始直後の EOF は None、改行のない最終行は Some です。LF の直前ではない CR、埋め込み NUL、Unicode の内容はそのまま保持します。

IO 内ではソース順に同期実行します。native は C 標準入出力のバッファを使い、出力操作ごとに flush するため、改行なしのプロンプトも入力前に表示されます。失敗前に書き込めたバイトは取り消しません。複数スレッドや複数ホスト呼び出しを跨ぐ行全体の不可分性は保証しません。

`run` は stdin を子プロセスへ渡し、stdout を即時表示します。通常モードの stderr も即時表示します。`run --json` だけは異常終了時の診断を守るため stderr を終了まで保持するので、対話プロンプトには stdout を使います。

## 合成と制約

IO は不透明型です。内部フィールドへのアクセス・構築・更新、低水準の `IO.__read_line` / `IO.__write` は拒否されます。言語内に `IO.run` や `IO<T> -> T` の取り出し関数はなく、Task.run で IO を実行することもできません。IO の生成・map・bind・破棄では入出力しません。

IO アクションは通常の関数値の捕捉・複製・借用規則に従います。共有借用は所有者の寿命を越えられず、排他参照や一回実行 Task の捕捉は Capture 制約で拒否します。

`for` は配列を順番に処理します。`and!` は左から右の逐次実行であり、並列実行ではありません。while も使えますが、通常の計算式と同じく、継続を跨いだ外側の可変ローカルの更新はできません。詳細は[計算式](../language-reference/computation-expressions.md)を参照してください。

Debug と extern は従来の副作用付き API として残ります。IO の追加は言語全体の effect system、非同期イベントループ、ファイル・ネットワーク API の導入ではありません。

## WASM と C

IO を返す Main は WASM でも使えます。

```sh
./target/release/tsuzuri build examples/io --target wasm32 -o target/io.wasm
```

ホストは `instance.exports.tsuzuri_main()` を呼びます。インスタンス化だけでは実行せず、export def も不要です。戻り値は正常完了時の 0 です。`--emit object` / `--emit header` でも `int32_t tsuzuri_main(void)` を公開し、C ホストから明示実行できます。

Windows のランタイム同梱 COFF オブジェクト出力は未対応で、実行ファイルか LLVM 出力を使います。Windows 実機での IO 動作は未検証です。

WASM は到達する操作だけを tsuzuri_io モジュールから import します。標準入出力を使わないプログラムには import を追加しません。ホスト未提供時に no-op として成功することはありません。

| import | 同期ホストの契約 |
| --- | --- |
| `read_line(out) -> i32` | 0 = 一行、1 = EOF、2 = 読み取り失敗。out の offset 0 に i32 pointer、offset 8 に i64 byte length を全経路で設定 |
| `write(destination, pointer, length) -> i32` | destination は 1 = stdout、2 = stderr。長さは i64 byte 数。0 = 全量書き込みと flush 成功、非 0 = 失敗 |

read_line の成功バッファは `tsuzuri_alloc` で確保し、改行を除いた UTF-8 を格納して所有権を Tsuzuri に渡します。空行・EOF・失敗は pointer 0 / length 0 にできます。状態値、負の長さ、NULL と正の長さ、不正な WASM メモリ範囲を検査します。確保後は memory.buffer の view を作り直してください。

write のバッファは同期呼び出し中だけの借用です。解放・変更・返却後の保持をしてはいけません。write_line では既に LF を含むので、ホストは改行を追加しません。ホストは ABI を守る信頼境界であり、Promise は返せません。Node.js / ブラウザー等に応じた接続例の基礎は[WASM ガイド](../guides/webassembly.md)、実行可能な参照ホストは[IO テスト](../../tests/io.mjs)にあります。
