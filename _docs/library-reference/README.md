# 標準ライブラリと組み込み API

[ドキュメントのトップ](../README.md)

標準ライブラリはすべてのプロジェクトへ同梱され、import は不要です。ソースで実装した関数と、コンパイラの型付き組み込み関数を同じモジュール名で使います。公開 API の所有権と失敗条件は以下の解説で確認できます。

## 分野別リファレンス

| 記事 | 主な対象 |
| --- | --- |
| [Option / Result](option-result.md) | 不在、失敗値、map / bind、借用版、ビルダー |
| [Array / List](arrays-and-lists.md) | 不変コレクション、所有更新、共有スライス、検索と整列 |
| [Vec](vec.md) | 伸縮、容量、pop、バッファ移送 |
| [Map / Set](map-set.md) | キー順の検索・更新・集合演算 |
| [Seq](sequences.md) | 一回消費の遅延列、iter、借用反復 |
| [文字列と文字](text.md) | String / Utf8String / Char / Utf8Char |
| [Math](math.md) | Float、Elementary、FMA、固定順の集計 |
| [Int](integers.md) | checked、saturating、bit、rotate、widening |
| [表示と解析](formatting-and-parsing.md) | Display、Parse、to_string |
| [基本組み込み、Debug、Test](builtins.md) | 固定型の互換関数、assert、出力とテスト |
| [Parallel](parallel.md) | 配列の同期並列生成・変換・還元 |
| [Simd](simd.md) | 明示 128-bit vector と mask |
| [Task](../language-reference/tasks.md) | 遅延計算の組み合わせと並列実行 |
| [Gpu](../guides/gpu.md) | 実験的 CPU 参照、WGSL、WebGPU |

## 型と借用の読み方

T / U / State は説明用の型記号で、実際のシグネチャでは `'a` などの型変数を使います。`ref T` は共有借用、`T` の値引数は所有権を受け取ります。戻り値の ref は所有者の寿命を保持します。

Copy は複製可能という契約です。コレクションや捕捉環境の深い複製を伴う場合があります。読み取り API がコレクションを借用していても、値 callback に要素を渡す部分では Copy が必要になることがあります。

引数順は API ごとに確認します。例えば Array.map は配列が先、Parallel.map と Option.map は関数が先、Seq.map は列が先です。

## ソース宣言の API 一覧

以下は同梱 std のソースから tsuzuri doc で生成した参照資料です。宣言順、型パラメーター、明示制約を保ちます。

| モジュール | ソース宣言 | 契約の解説 |
| --- | --- | --- |
| Array | [Array](api/Array.md) | [Array / List](arrays-and-lists.md) |
| List | [List](api/List.md) | [Array / List](arrays-and-lists.md) |
| Vec | [Vec](api/Vec.md) | [Vec](vec.md) |
| Map | [Map](api/Map.md) | [Map / Set](map-set.md) |
| Set | [Set](api/Set.md) | [Map / Set](map-set.md) |
| Seq | [Seq](api/Seq.md) | [Seq](sequences.md) |
| Option | [Option](api/Option.md) | [Option / Result](option-result.md) |
| Result | [Result](api/Result.md) | [Option / Result](option-result.md) |
| String | [String](api/String.md) | [文字列](text.md) |
| Utf8String | [Utf8String](api/Utf8String.md) | [文字列](text.md) |
| Char | [Char](api/Char.md) | [文字型](../language-reference/strings-and-characters.md) |
| Utf8Char | [Utf8Char](api/Utf8Char.md) | [文字型](../language-reference/strings-and-characters.md) |
| Math | [Math](api/Math.md) | [Math](math.md) |
| Parallel | [Parallel](api/Parallel.md) | [Parallel](parallel.md) |
| Debug | [Debug](api/Debug.md) | [Debug](builtins.md) |
| Test | [Test](api/Test.md) | [Test](builtins.md) |
| Gpu | [Gpu](api/Gpu.md) | [GPU](../guides/gpu.md) |

**生成宣言だけでは全 API の一覧にはなりません。** Int、Simd、Task と、Vec / Math などの組み込み操作はコンパイラに実装され、上の手書き解説に含めています。また、本文で推論される Copy などの制約をすべてソース署名へ書き戻す生成器ではありません。

不透明な Map / Set / Seq / Gpu の内部フィールドがソース宣言として見えても、利用者による直接構築・分解を許可するものではありません。解説ページの所有権・可視性契約を優先します。

## 失敗と互換性

get / at / sub のような名前だけで失敗動作を推測しないでください。Array.get は None、Array.at / sub はトラップ、String.sub は None です。

std は .NET、WASI、OS / GUI / ファイル / ネットワークの標準ライブラリではありません。モジュール名が予約されていても、他言語の同名 API がすべて使えるわけではありません。

## 関連項目

- [言語リファレンス](../language-reference/README.md)
- [API 文書の生成](../tools/documentation.md)
- [対応状況](../feature-status.md)
