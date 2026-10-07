# Utf8String

`Utf8String` は、妥当な UTF-8 である `utf8string` の検索・切り出し・連結です。長さと添字の単位はバイトです。`String` と同名の関数がありますが、空の区切りとスライスの境界だけはスカラー単位になります。

型の選び方は [文字列](../literals-and-strings/strings.md)、UTF-16 側の契約は [String](string.md) を参照してください。

## この記事のポイント

- 引数の並びは `String` と同じです。パターンや区切りが第 1 引数です。
- `slice` と `sub` は、始点と終点がスカラー境界でなければ、空の区間でも `None` です。
- 空の区切りによる `split` と `replace` は、バイトではなくスカラーのあいだで切ります。
- `from_bytes` は不正な UTF-8 を `None` にします。値になった `utf8string` は常に妥当です。
- 複製は `Utf8String.clone` です。`clone_string` ではありません。

> [!WARNING]
> `Utf8String.find u8"zu" title` です。本文を先に書きません。`split` も区切りが先です。

## 長さと走査

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `length` | `ref utf8string -> i64` | バイト数。`.length` と同じで O(1) |
| `char_count` | `ref utf8string -> i64` | スカラー数。バイト数ではない。O(n) |
| `chars` | `ref utf8string -> [utf8char]` | スカラーを順に集めた新しい配列。O(n) |
| `decode_at` | `ref utf8string -> i64 -> (utf8char * i64)` | その位置のスカラーと、次のバイト位置 |

`decode_at` は共有借用です。index が範囲外、負、スカラーの途中だとトラップします。`utf8string` 自体は妥当な UTF-8 なので、先頭から順に呼べば途中バイトには当たりません。

```tsuzuri run=mid%3Dnone%20parts%3D%5Bu8%22%E3%81%82%22%5D%20decoded%3D12354%403%20count%3D1
let bytes = u8"あ"
let mid = Utf8String.slice (ref bytes) 1 3
let mid_text = match mid with
    | Some _ -> "some"
    | None -> "none"
let parts = Utf8String.split u8"" bytes
let decoded = match Utf8String.decode_at (ref bytes) 0 with
    | (value, next) -> to_string (Utf8Char.to_u32 value) + "@" + to_string next
$"mid={mid_text} parts={parts} decoded={decoded} count={Utf8String.char_count bytes}"
```

実行結果:

```text
mid=none parts=[u8"あ"] decoded=12354@3 count=1
```

`あ` は UTF-8 で 3 バイト、スカラーとしては 1 個です。途中のバイト 1 から切ろうとすると `None` です。空の区切りで割っても、バイト 3 つには分かれません。

`for byte in bytes` が返すのは `utf8char` ではなく `ubyte` です。

## 検索と比較

空のパターンの扱いは `String` と同じです。`find` の空パターンは `Some 0`、`rfind` は `Some text.length`、`contains` / `starts_with` / `ends_with` の空パターンは真です。すべて共有借用です。

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `find` | `ref utf8string -> ref utf8string -> Maybe<i64>` | 無ければ `None`。位置はバイト | 最悪 O(本文 × パターン) |
| `rfind` | `ref utf8string -> ref utf8string -> Maybe<i64>` | 無ければ `None` | 最悪 O(本文 × パターン) |
| `contains` | `ref utf8string -> ref utf8string -> bool` | トラップしない | `find` と同じ |
| `starts_with` | `ref utf8string -> ref utf8string -> bool` | トラップしない | O(パターン) |
| `ends_with` | `ref utf8string -> ref utf8string -> bool` | トラップしない | O(パターン) |
| `compare` | `ref utf8string -> ref utf8string -> i64` | `-1` / `0` / `1`。バイト順 | O(短いほう) |

バイト順は Unicode スカラーの順と一致します。UTF-16 のコード単位順とは、補助平面でずれることがあります。

## 切り出し

| 関数 | シグネチャ | 失敗 | 計算量 |
| --- | --- | --- | --- |
| `slice` | `ref utf8string -> i64 -> i64 -> Maybe<utf8string>` | 負の位置、`last < first`、範囲外、またはスカラー境界でないなら `None` | O(結果のバイト数) |
| `sub` | `ref utf8string -> i64 -> i64 -> Maybe<utf8string>` | 負の位置、加算オーバーフロー、範囲外、またはスカラー境界でないなら `None` | O(結果のバイト数) |

`slice text first last` の `last` は結果に含まれません。`sub text first count` は `first` から `count` バイトを切り出します。負の位置、逆順（`last < first`）、範囲外アクセス、および加算オーバーフローが発生した場合は `None` を返します。

それに加えて、始点と終点が妥当な Unicode スカラー値の境界でなければ `None` を返します。長さ 0 の空区間であっても、後続バイト（continuation byte）の途中に位置する場合は `None` です。`String.slice` がサロゲートペアの途中インデックスをそのまま切り出すのと、この点が大きく異なります。

切り出しに成功した場合は、そのバイト列をコピーした新しい所有値 `utf8string` を返します。計算量は結果のバイト数に比例します。

## 組み立て

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `concat` | `ref [utf8string] -> utf8string` | 区切りなし。空配列は `u8""` |
| `join` | `ref utf8string -> ref [utf8string] -> utf8string` | 区切りが第 1 引数。空配列は `u8""` |
| `split` | `ref utf8string -> ref utf8string -> [utf8string]` | 区切りが第 1 引数 |
| `replace` | `ref utf8string -> ref utf8string -> ref utf8string -> utf8string` | パターン、置換、本文の順 |
| `repeat` | `ref utf8string -> i64 -> utf8string` | 負の回数はトラップ。0 回は `u8""` |

空でない区切りは、バイト列として一致した位置で切ります。連続や端の一致は空要素です。空文字列を `u8","` で割ると `[u8""]` です。

空の区切りはスカラーのあいだで切ります。`u8"あ"` は 1 要素のままです。`replace` の空パターンも、スカラー境界と両端に挿入し、空文字列には 1 回だけ挿入します。置換後の再検索はしません。

連結と置換は、結果の長さを先に検査してから 1 回だけ確保します。積や和が溢れるとトラップします。`string` と違い、上限の検査は `i64` の範囲です。

## 空白と ASCII の大小

| 関数 | シグネチャ | 説明 | 計算量 |
| --- | --- | --- | --- |
| `trim` | `ref utf8string -> utf8string` | 先頭と末尾の ASCII 空白を除去 | O(n) |
| `trim_start` | `ref utf8string -> utf8string` | 先頭の ASCII 空白を除去 | O(n) |
| `trim_end` | `ref utf8string -> utf8string` | 末尾の ASCII 空白を除去 | O(n) |
| `to_ascii_lower` | `ref utf8string -> utf8string` | ASCII 英大文字（`A`〜`Z`）を小文字に変換 | O(n) |
| `to_ascii_upper` | `ref utf8string -> utf8string` | ASCII 英小文字（`a`〜`z`）を大文字に変換 | O(n) |

`trim`、`trim_start`、`trim_end` が取り除くのは、バイト値 9〜13（タブ、改行、垂直タブ、フォームフィード、復帰）と 32（空白）のみです。それ以外の空白文字は除去しません。また、ASCII の英文字のみを大文字・小文字に変換するのが `to_ascii_lower` と `to_ascii_upper` です。いずれも新しい所有値を返し、失敗することはありません。計算量は O(n) です。

## バイト列との変換

| 関数 | シグネチャ | 所有権 | 失敗 |
| --- | --- | --- | --- |
| `to_bytes` | `utf8string -> [ubyte]` | 入力を消費。ヒープならポインタを移す | しない |
| `from_bytes` | `[ubyte] -> Maybe<utf8string>` | 入力を消費。成功ならそのバッファを使う | 不正なら解放して `None`。O(n) |
| `clone` | `ref utf8string -> utf8string` | 入力は残る。O(n) のコピー | しない |
| `from_string` | `ref string -> utf8string` | 入力は消費しない | 孤立サロゲートでトラップ |

`from_bytes` は、妥当なら新しいコピーを作りません。不正なときは入力バッファを解放してから `None` を返します。呼び出し側で配列を再利用はできません。

```tsuzuri run=bad%3Dnone%20back%3DA%20bytes%3D%5B65%5D
let bad = Utf8String.from_bytes [255ubyte]
let bad_text = match bad with
    | Some _ -> "some"
    | None -> "none"
let raw = Utf8String.to_bytes u8"A"
let back = Utf8String.from_bytes raw
let back_text = match back with
    | Some value -> to_string value
    | None -> "none"
$"bad={bad_text} back={back_text} bytes={Utf8String.to_bytes u8"A"}"
```

実行結果:

```text
bad=none back=A bytes=[65]
```

`to_bytes` の結果を表示すると、バイト値の配列になります。`A` は 65 です。

UTF-16 から来る文字列は、先に `String.is_well_formed` を見るか、`String.to_well_formed` で置換してから `from_string` してください。置換せずに孤立サロゲートを渡すとトラップします。

## 注意点

> [!NOTE]
> 添字 `bytes[index]` は `ubyte` です。スカラーが欲しいときは `decode_at` か `chars` を使います。途中バイトを `decode_at` に渡すとトラップし、`slice` に渡すと `None` です。

## まとめ

- 長さはバイト、`char_count` と `chars` はスカラーです。
- スライスはスカラー境界が必須です。途中なら空でも `None` です。
- 空の区切りはスカラー境界、空でない区切りはバイト列の一致です。
- `from_bytes` は検査に失敗すると `None`、`from_string` は孤立サロゲートでトラップします。
- 複製は `Utf8String.clone` です。

## 関連項目

- [Regex](regex.md) — 正規表現による検索・置換・分割（`_utf8` の関数）
- [文字列](../literals-and-strings/strings.md)
- [String](string.md)
- [Utf8Char](utf8char.md)
- [Format](format.md)
- [言語リファレンスの目次](../index.md)
