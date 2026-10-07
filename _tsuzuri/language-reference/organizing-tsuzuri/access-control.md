# アクセス制御

Tsuzuri におけるアクセス制御（可視性）は、モジュールの内部実装をカプセル化し、公開 API と内部ヘルパーを明確に分離するための機能です。

宣言は既定で公開（public）です。隠したいものにだけ `private` を付けます。`export` はホストへ出す指定であり、モジュール間の可視性とは別です。

## この記事のポイント

- Tsuzuri の宣言は既定で public です。非公開にする場合のみ `private` を明示します。
- `private` は `def`、`record`、`union`、`type`、`const`、`extern def` に付与できます。
- 関数の実装側（`fn`、`and`、`let`）には `private` を付けず、先行する `def` シグネチャの可視性を自動継承します。
- public な関数や型から同一モジュール内の private 型を漏洩させることは禁止されています（`E1022`）。
- `export` は C ABI や WASM ホスト環境に対する公開指定であり、モジュール間の可視性制御とは無関係です。
- 型クラス（`class`）とインスタンス（`instance`）は言語仕様上常に public です。

## 基本の書き方（既定 public と private）

```text
private def 関数名 :: 型 = 実装
private record 型名 { フィールド: 型 }
private union 型名 = ケース ...
private type 別名 = 既存型
private const 定数名: 型 = 式
```

非公開にしたい要素の直前に `private` キーワードを記述します。`private` を付与した要素は、宣言されたモジュール（ファイル）の内部からのみ参照可能になります。

次の例では、`Auth::Token` モジュール内で private なレコードとヘルパー関数を定義し、外部へは公開関数 `generate_id` のみを提供しています。

```tsuzuri project=access-basic file=Auth/Token.tz
namespace Auth

private record Token { secret: i64 }

private def create_token :: i64 -> Token = \id -> Token { secret: id * 1000 + 42 }

def generate_id :: i64 -> i64 = \raw ->
    let token = create_token raw
    token.secret
```

```tsuzuri project=access-basic file=Main.tz run=5042
let id = Auth::Token.generate_id 5
id
```

実行結果:

```text
5042
```

他のモジュールから `Auth::Token.create_token` や `Auth::Token.Token` を直接参照しようとすると、コンパイルエラー `E1022`（`only visible inside module 'Auth::Token'`）になります。

## 許可される宣言と禁止される組み合わせ

`private` を付けられる宣言と、付けられない組み合わせは次のとおりです。

| 宣言 | 可否 | 説明 / 違反時の挙動 |
| --- | --- | --- |
| `private def` | ○ | モジュール内限定の関数シグネチャ |
| `private record` | ○ | モジュール内限定のレコード型 |
| `private union` | ○ | モジュール内限定の共用体型（配下の case もすべて private） |
| `private type` | ○ | モジュール内限定の型エイリアス |
| `private const` | ○ | モジュール内限定の定数 |
| `private extern def` | ○ | モジュール内限定の外部ホスト関数 |
| `private fn` / `private let` | ✗ | エラー `E1022`。`def` の可視性を自動継承するため、実装側に重ねて書きません |
| `private class` | ✗ | エラー `E1022`。型クラスは常に public です |
| `private instance` | ✗ | エラー `E1022`。インスタンスはグローバル一貫性（coherence）のため常に public です |
| `private export` / `export private` | ✗ | エラー `E1022`。非公開とホスト公開は矛盾するため指定できません |
| ビルダーの基本操作 | ✗ | エラー `E1022`。`.tc` の `Bind` や `Return` は private にできません。補助関数は private にできます |

### シグネチャと実装の分離時の可視性

Tsuzuri では `def` による型シグネチャ宣言と、`fn` や `let` による実装を別々に記述できます。この分離形式を採用した場合、実装ブロックは対応する `def` の可視性を自動的に継承します。

```tsuzuri run=42
private def calculate :: i64 -> i64
fn calculate x = x * 2

def public_api :: i64 -> i64 = \n -> calculate n

public_api 21
```

実行結果:

```text
42
```

実装側の `fn calculate` に `private fn` と記述すると、構文解析時にエラー `E1022` になります。可視性の制御は常に `def` 側に記述してください。

相互再帰関数群（`def rec` と `and`）においても同様であり、各関数ごとに `private def` または `def` を指定します。

## private 型の漏洩禁止（E1022）

公開の関数やレコードの境界に private な型が出ると、外からその型を扱えません。コンパイラはこれを `E1022` にします。

次のパターンはすべてコンパイルエラーになります。

- public 関数の引数型または戻り値型に private 型を使用する
- public レコードのフィールド型に private 型を使用する
- public union のペイロード型に private 型を使用する
- public 関数の型クラス制約に private 型を含める

次は断片です。

```text
private record Secret { key: i64 }

// このコードはエラー E1022 になります（private 型が public 関数の戻り値から漏洩）
def get_secret :: unit -> Secret = \() -> Secret { key: 123 }
```

診断は、漏れた型の位置を指します。既定名前空間が無いときのメッセージは次のとおりです。行頭には `ファイル:行:列:` が付きます。

```text
error[E1022]: private type 'Sample.Secret' leaks from public function 'Sample.get_secret'; make the function private or expose a public type
```

パッケージの既定名前空間があると、型名にはそれが付きます（`App::Sample.Secret`）。関数名はモジュールキーのまま（`Sample.get_secret`）です。

private な関数の引数や戻り値、private なレコードのフィールドなら、private 型を使えます。アクティブパターンの可視性は、認識器の `def` に従います。

## private 宣言と名前解決

`private` 修飾子はコンパイル時の名前解決ルールに作用します。

1. **自モジュール内での解決**: 自モジュール内であれば、修飾・無修飾にかかわらず private な関数や型を直接利用できます。
2. **無修飾名の候補から外す**: 名前空間やモジュール名を付けない型名では、他モジュールの private 宣言は候補に入りません。
3. **名前の曖昧性判定への影響**: 複数のモジュールに同名の型が存在する場合、通常は曖昧性エラー `E1004` になりますが、他モジュールの同名型が private である場合は曖昧性判定の対象外となり、public な型のみが一意に解決されます。

## export とモジュール可視性の違い

`export` は、モジュールの公開と間違えやすいキーワードです。

**`export` は C 言語 ABI や WebAssembly ホスト環境に対する公開指定であり、Tsuzuri モジュール間の可視性とは無関係です。**

```text
// export が付いていない通常の def
def add :: i64 -> i64 -> i64 = \a b -> a + b

// C 言語や WASM ホストへシンボルを公開する def
export def c_add :: i64 -> i64 -> i64 = \a b -> a + b
```

- **モジュール間**: `export` の無い `def add` も、他のモジュールから修飾名で呼べます。レコードや配列、クロージャ、型クラスも、モジュール間では渡せます。`import` 宣言はありません。
- **ホスト境界**: `export def` だけが、ネイティブのシンボル（`tz_c_add`）や WASM の export になります。型引数は禁止で、引数と戻り値はスカラーや平坦なバッファに限られます。

`private` はコンパイル時の名前解決だけを変えます。評価順、所有権、生成される LLVM IR は変わりません。

Tsuzuri モジュール内だけで使う関数を他モジュールへ公開したい場合、`export` を付ける必要はありません（単に `def` と書くだけで public になります）。

## 他の言語との比較

| 項目 | Tsuzuri | Rust | F# | Go |
| --- | --- | --- | --- | --- |
| 既定の可視性 | **public** | **private** | **public** | **private**（小文字始まり） |
| 非公開の指定 | `private` | `pub` を付けない | `private` | 小文字で命名 |
| パッケージ内限定公開 | なし（モジュール単位） | `pub(crate)` | `internal` | なし（同一 package は可視） |
| 型クラス / インスタンス | 常に public | トレイトは `pub`、impl はスコープ依存 | インターフェースは public | インターフェース実装は暗黙 |
| ホスト公開 | `export def` | `#[no_mangle] pub extern "C"` | `[<DllExport>]` 等 | `//export` |

## まとめ

- すべての宣言は既定で public です。非公開にする要素にのみ `private` を明示します。
- `private` は `def`、`record`、`union`、`type`、`const`、`extern def` に付与できます。
- 関数の実装側（`fn` / `let`）は先行する `def` の可視性を自動継承するため、実装側に `private` は付けません。
- public なシグネチャから同一モジュールの private 型を漏洩させることは禁止されています（`E1022`）。
- 型クラスとインスタンスは言語仕様上常に public です。
- `export` は C 言語や WASM ホスト環境に対するエクスポート指定であり、モジュール間の可視性制御とは無関係です。

## 関連項目

- [モジュール](modules.md)
- [名前空間](namespaces.md)
- [using 宣言](using-declarations.md)
- [ネイティブ連携 (C ABI)](../compiler/native-interop.md)
- [WebAssembly への出力](../compiler/webassembly.md)
- [言語リファレンスの目次](../index.md)

