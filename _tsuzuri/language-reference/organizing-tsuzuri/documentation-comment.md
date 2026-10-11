# ドキュメント コメント

`///` は、直後の宣言へ説明を付ける構文です。言語サーバー（LSP）のホバーと、`tsuzuri doc` が生成する Markdown がこの説明を使います。

型検査や生成されるバイナリには影響しません。ただし、`@deprecated` で始まる行は、別のパッケージからの使用を警告 `W1005` にします（[非推奨の印](#非推奨の印)）。

## この記事のポイント

- 直後の宣言に対して `///` でドキュメントコメント（doc comment）を記述します。
- スラッシュ直後の空白 1 文字が自動トリミングされ、複数行は改行で連結されます。
- `def`、`record`、`union`、`type`、`const`、`class`、型クラス内のメソッド `def` に付与できます。
- `fn` や `let` 実装、`instance`、フィールド、ローカル束縛への配置は構文エラー `E0002` です。
- 説明の行を `@deprecated` で始めると宣言が非推奨になり、別のパッケージ（標準ライブラリを含む）からの使用が警告 `W1005` になります。
- `tsuzuri doc <input> -o <outdir>` コマンドで公開 API の Markdown ドキュメントを出力します。
- 出力先ディレクトリは `.tsuzuri-docs` マーカーファイルによる上書き保護機構を備えています。

## 基本の書き方

```text
/// 関数の概要を記述します。
/// Markdown 記法やコードブロックを含められます。
def 関数名 :: 型 = 実装
```

`///` を付与した直後の宣言がドキュメント化の対象になります。コメント行は連続して複数行記述でき、改行で連結されます。

```tsuzuri project=docs-example file=Arithmetic/Calculator.tz
/// 2 つの数値を加算します。
///
/// 整数演算のオーバーフロー検査が行われます。
def add :: i64 -> i64 -> i64 = \a b -> a + b

/// 計算精度の既定値です。
const DefaultPrecision: i64 = 64
```

```tsuzuri project=docs-example file=Main.tz run=30
let sum = Arithmetic::Calculator.add 10 20
sum
```

実行結果:

```text
30
```

### 整形のルール

- **先頭空白のトリミング**: `///` の直後にある半角スペース 1 文字は自動的に除去されます（`/// テキスト` は `テキスト` として解釈されます）。
- **4 連スラッシュ**: `////` のようにスラッシュを 4 つ以上重ねた場合、トリミング後の本文は `/` で始まります。
- **通常のコメントとの違い**: 通常の行コメント `//` やブロックコメント `/* ... */` はドキュメント情報として保持されず、生成ドキュメントや LSP ホバーには現れません。
- **Markdown の保持**: コメント内に記述された見出し、箇条書き、コードフェンス（```` ```tsuzuri ```` など）、HTML タグはそのまま維持されます。

## コメントを付与できる宣言と禁止される場所

付けられるのは、次の宣言の直前だけです。`private` にも付けられますが、生成される文書には出ません。

| 対象の宣言 | 可否 | 説明 / 違反時の挙動 |
| --- | --- | --- |
| `def`（通常 / export / private / extern） | ○ | 関数シグネチャの説明 |
| `record` | ○ | レコード型自体の説明 |
| `union` | ○ | 共用体型自体の説明 |
| `type` | ○ | 型エイリアスの説明 |
| `const` | ○ | 定数の説明（生成文書では型のみ表示） |
| `class` | ○ | 型クラスの説明 |
| 型クラス内のメソッド `def` | ○ | 型クラスメソッドの説明 |
| `fn` / `let` 実装ブロック | ✗ | 構文エラー `E0002`。先行する `def` に付けます |
| `instance` | ✗ | 構文エラー `E0002`。インスタンス実装は文書化されません |
| レコードのフィールド定義 | ✗ | 構文エラー `E0002` |
| ローカル束縛（関数内の `let` 等） | ✗ | 構文エラー `E0002` |
| `test` ブロック | ✗ | 構文エラー `E0002` |
| コンピュテーション式の文 | ✗ | 構文エラー `E0002` |
| エントリーポイントの式・ファイル末尾 | ✗ | 構文エラー `E0002` |

次は断片です。

```text
def calculate :: i64 -> i64
/// 誤った位置: 実装の fn の前に置いている
fn calculate x = x * 2
```

上記のように `fn` の直前に `///` を配置すると、構文解析時にエラー `E0002` になります。ドキュメントコメントは必ず `def` の直前に記述してください。

## 非推奨の印

説明の行を `@deprecated` で始めると、その宣言は非推奨になります。残りの文が、使う側への案内（移行先など）です。新しい構文ではなく、説明の本文の約束です。

```text
/// @deprecated use LegacyLib::Old.next instead
def old :: i64 -> i64 = \x -> x + 1
```

- **印の行**: 各行の先頭の空白を除いて `@deprecated` で始まり、その直後が行末か空白（スペースかタブ）である最初の 1 行です。`@deprecatedX` は印ではありません。2 つ目以降の `@deprecated` の行は、通常の説明です。コードフェンスの中の行も区別しません。
- **案内**: 印の行の残りから前後の空白を除いたものです。空でもかまいません。
- **表示**: `tsuzuri doc` と LSP のホバーは、この行も説明として出します。
- **対象**: `def`（`extern def` を除く）、`record`、`union` です。`const`、`type`、`class`、メソッド、`extern def` の印は、説明として表示されるだけで、警告にはなりません。

### 警告 W1005

別のパッケージ、または標準ライブラリが宣言した非推奨の宣言を、自分のソースが使うと `W1005` が出ます。同じパッケージの中の使用には出ません。`Tsuzuri.toml` のないプロジェクトは、全体が 1 つのパッケージです。非推奨にした側が、移行の期間に自分の `--deny-warnings` のビルドを壊さずに済みます。

パッケージ `legacy-lib` の `Old.tz` と、それに依存する `app` の `Main.tz` です。

```text
legacy-lib/Old.tz
    /// @deprecated use LegacyLib::Old.next instead
    def old :: i64 -> i64 = \x -> x + 1
    def next :: i64 -> i64 = \x -> x + 1
    /// @deprecated use a tuple
    record Pair { left: i64, right: i64 }
    /// @deprecated use bool
    union Light = Red | Green
    def inside :: i64 -> i64 = \x -> old x

app/Main.tz
    let pair = LegacyLib::Old.Pair { left: 1, right: 2 }
    let light = LegacyLib::Old.Red
    let bonus = match light with
        | LegacyLib::Old.Red -> 0
        | LegacyLib::Old.Green -> 1
    LegacyLib::Old.old 39 + pair.right + bonus
```

`tsuzuri check app` は、名前を書いた 5 か所を警告します。終了コードは 0 です。

```text
app/Main.tz:1:27: warning[W1005]: 'LegacyLib::Old.Pair' is deprecated: use a tuple
  1 | let pair = LegacyLib::Old.Pair { left: 1, right: 2 }
    |                           ^

app/Main.tz:2:28: warning[W1005]: 'LegacyLib::Old.Light' is deprecated: use bool
  2 | let light = LegacyLib::Old.Red
    |                            ^

app/Main.tz:4:22: warning[W1005]: 'LegacyLib::Old.Light' is deprecated: use bool
  4 |     | LegacyLib::Old.Red -> 0
    |                      ^

app/Main.tz:5:22: warning[W1005]: 'LegacyLib::Old.Light' is deprecated: use bool
  5 |     | LegacyLib::Old.Green -> 1
    |                      ^

app/Main.tz:6:16: warning[W1005]: 'LegacyLib::Old.old' is deprecated: use LegacyLib::Old.next instead
  6 | LegacyLib::Old.old 39 + pair.right + bonus
    |                ^
```

`legacy-lib` の `inside` が `old` を呼ぶ行は、同じパッケージなので警告しません。

- **警告する場所**: 宣言の名前を書いた場所です。関数の参照、`record` の構築、型の注釈、`union` の case の構築とパターンです。`union` の case を使うと、`union` の印が使われます。型エイリアスを経由した使用は、エイリアスの定義の 1 件だけです。
- **メッセージ**: `'<完全修飾名>' is deprecated: <案内>` です。案内が空なら `'<完全修飾名>' is deprecated; see its documentation for a replacement` です。案内は 200 文字までで、超えると `...` で切ります。
- **位置**: 参照の最後の名前（上の例の `Pair`、`old`）です。同じ位置は 1 回だけです。
- **コマンド**: `check`、`build`、`run`、`test`、`bench`、`doc`、言語サーバーが、ほかの警告と同じ形で出します。既定で有効で、`--deny-warnings` を付けると終了コード 1 です。この例の `run app --deny-warnings` は何も実行しません。
- **コード生成**: 生成されるコードは、`@deprecated` の有無で変わりません。別のパッケージが使える非推奨の宣言がないプロジェクトでは、警告のための追加の解析も行いません。

非推奨にした宣言を取り除く手順と、警告が互換性の保証の対象外であることは、[安定性と互換性の方針](../../../docs/stability.md#非推奨の手順) にあります。

## tsuzuri doc によるドキュメント生成

`tsuzuri doc` は、公開（public）宣言だけを Markdown の API リファレンスにします。

```sh
tsuzuri doc src -o docs/api
```

### 生成の仕様

- **型検査のあと**: 入力したソースの型検査が終わってから出力します。型エラーがあると生成せず、既存の出力も書き換えません。
- **入口は不要**: ライブラリ用なので、入力ディレクトリに `Main.tz` が無くても実行できます。
- **ファイル名**:
  - 一覧は `index.md` です。
  - 階層モジュールは `Geometry.Point.md` です。見出しと一覧では `Geometry::Point` と書きます。
  - ファイルに `namespace` があると、見出しの下に Namespace 行が出ます（例: Namespace: `Physics`）。ディレクトリから決めた名前空間だけでは、この行は出ません。
- **並び**: ページは名前順です。ページ内の宣言は、ソースに出てきた順です。型変数、制約、region、アクティブパターンの宣言名はそのまま残します。
- **中身**:
  - `const` は型だけです。初期化式は出しません。
  - 型クラスはメソッドを含めて出します。`instance` は出しません。
  - `private` は出しません。`///` を付けてあっても同じです。
  - コンパイラ内部の組み込み関数は一覧にしません。ソースにある標準ライブラリは、`std` ディレクトリを入力すれば生成できます。`Maybe.tc` のように型とビルダー操作が同じファイルにあれば、1 ページに並びます。

```sh
tsuzuri doc std -o build/std-api
```

## 出力ディレクトリの保護機構（.tsuzuri-docs）

出力先を間違えて既存ファイルを消さないよう、`tsuzuri doc` は次の規則で守ります。

1. **新規ディレクトリ**: 出力先が無ければ、親も含めて作ります。
2. **マーカー**: 既存ディレクトリを指定するときは、直下に通常ファイル `.tsuzuri-docs` が要ります。4096 バイト以内で、中に `generated by tsuzuri doc` が含まれている必要があります。
3. **置換**: いったん一時ディレクトリへ全部書き、成功したら既存出力と入れ替えます。公開に失敗したときは旧出力の復元を試みます。復元できないときは、退避先のパスを診断に出します。
4. **拒否（`E2003`）**: 次のときは何も書き換えません。
   - マーカーの無い既存ディレクトリ
   - 出力先やマーカーがシンボリックリンク
   - ソース（`.tz`、`.tt`、`.tc`）を含むディレクトリ
   - ページ名が `index.md` と衝突する（モジュール `Index` など）
   - 大文字小文字だけが違うページ名が衝突する

> [!WARNING]
> 正しいマーカーがあるディレクトリは、ツールが管理する出力先です。全体が置き換わるので、手書きのメモを混ぜないでください。

## オプションと制限

- `--json`: コンパイル診断メッセージを標準エラー出力へ JSON lines 形式で出力します。ドキュメント本文を JSON 化するオプションではありません。
- **ビルド専用オプションの拒否（`E2000`）**: `--target`、`--emit`、`-O`、`--cpu`、`-g` などのコード生成関連フラグを `tsuzuri doc` に渡すと引数エラー `E2000` になります。
- **doc-test は無い**: `tsuzuri doc` はコメント内のコード例を実行しません。例の確認は `tsuzuri check` や `tsuzuri test` で行います。
- **サニタイズ**: コメント内の Markdown や HTML はそのまま出力されます。第三者のドキュメントをブラウザ等でレンダリングする場合は、表示側で適切なサニタイズを実施してください。

## LSP ホバーとの関係

Tsuzuri の組み込み言語サーバー（`tsuzuri lsp`）は、ソースコード内の宣言に付与された `///` ドキュメントコメントを自動的に解析します。

LSP 対応のエディタで名前へカーソルを合わせると、定義位置の `///` が型シグネチャとともにホバーに出ます。

## 他の言語との比較

| 項目 | Tsuzuri | Rust | F# | C# |
| --- | --- | --- | --- | --- |
| コメント構文 | `///` | `///` または `//!` | `///`（XML ドキュメント） | `///`（XML ドキュメント） |
| ドキュメント生成ツール | `tsuzuri doc` | `cargo doc` | `fsdocs` | `dotnet build` / DocFX |
| 書式形式 | Markdown | Markdown | XML / Markdown | XML |
| 誤配置のエラー | コンパイルエラー `E0002` | 警告（unused doc comment） | 無視または警告 | 警告（CS1587 等） |
| 出力先保護 | `.tsuzuri-docs` マーカー | `target/doc` に上書き | 指定フォルダに出力 | 指定フォルダに出力 |

## まとめ

- `///` は直後の宣言へ説明を付けます。型検査やバイナリには影響しません。
- 先頭の空白 1 文字は除かれ、複数行は改行でつながります。Markdown もそのまま出せます。
- `def`、`record`、`union`、`type`、`const`、`class` に付けられます。実装や式へ付けると `E0002` です。
- 説明の行を `@deprecated` で始めると、別のパッケージや標準ライブラリからの使用が `W1005` の警告になります。同じパッケージの中では出ません。
- `tsuzuri doc` は公開宣言だけを Markdown にします。`private` は出しません。
- 出力先は `.tsuzuri-docs` マーカーで保護されます。`Index` というモジュール名は `index.md` と衝突して `E2003` です。
- LSP のホバーは、定義に付いた `///` を表示します。

## 関連項目

- [アクセス制御](access-control.md)
- [モジュール](modules.md)
- [コンパイラの使い方](../compiler/usage.md)
- [診断メッセージとエラーコード](../compiler/diagnostics.md)
- [安定性と互換性の方針](../../../docs/stability.md)
- [言語リファレンスの目次](../index.md)

