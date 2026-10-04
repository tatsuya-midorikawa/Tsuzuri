# 標準ライブラリと組み込み API

[ドキュメントのトップ](../README.md)

標準ライブラリはすべてのプロジェクトへ同梱され、import は不要です。ソースで実装した関数と、コンパイラの型付き組み込み関数を同じモジュール名で使います。公開 API の所有権と失敗条件は以下の解説で確認できます。

## 分野別リファレンス

| 記事 | 主な対象 |
| --- | --- |
| [Option / Result](option-result.md) | 不在、失敗値、map / bind、借用版、ビルダー、try と Exception |
| [Array / List](arrays-and-lists.md) | 不変コレクション、所有更新、共有スライス、検索と整列 |
| [Vec](vec.md) | 伸縮、容量、pop、バッファ移送 |
| [Map / Set](map-set.md) | キー順の検索・更新・集合演算 |
| [HashMap / HashSet](hash-map.md) | ハッシュ表、挿入順の反復、seed 付きハッシュ、借用キーの検索 |
| [Seq](sequences.md) | 一回消費の遅延列、iter、借用反復 |
| [文字列と文字](text.md) | String / Utf8String / Char / Utf8Char |
| [Math](math.md) | Float、Elementary、FMA、固定順の集計 |
| [Int](integers.md) | シフト、累乗、@checked、checked、saturating、bit、rotate、widening |
| [BigInt](bigint.md) | 任意精度整数 bigint、`I` リテラル、演算と変換 |
| [表示と解析](formatting-and-parsing.md) | Display、Parse、to_string、Format クラスと書式指定 |
| [IO と標準入出力](io.md) | IO モナド、stdin / stdout / stderr、EOF と失敗 |
| [OS API](os.md) | File / Dir / Path / Env / Time / Random / Os / Process、終了コード、WASI |
| [基本組み込み、Debug、Owned、Test](builtins.md) | 固定型の互換関数、assert、出力とテスト、Owned の早期解放と資源を持つ関数値 |
| [Parallel](parallel.md) | 配列の同期並列生成・変換・還元 |
| [Simd](simd.md) | 明示 128-bit vector と mask |
| [Task](../language-reference/tasks.md) | 遅延計算の組み合わせと並列実行 |
| [Gpu](../guides/gpu.md) | 実験的 CPU 参照、WGSL、WebGPU |

## 型と借用の読み方

T / U / State は説明用の型記号で、実際のシグネチャでは `'a` などの型変数を使います。`ref T` は共有借用、`T` の値引数は所有権を受け取ります。戻り値の ref は所有者の寿命を保持します。

Copy は複製可能という契約です。コレクションや捕捉環境の深い複製を伴う場合があります。読み取り API がコレクションを借用していても、値 callback に要素を渡す部分では Copy が必要になることがあります。

高階関数は F# と同じく関数を先、対象の配列・リスト・列・コレクションを最後に受け取ります。例えば `Array.map f values`、`Seq.filter p sequence`、`Map.fold f initial map`、`Parallel.map f values`、`Option.map f value` です。そのため `values |> Array.map f |> Array.sum` のように `|>` で繋げられます。Option / Result の bind と bind_ref は計算値が先、fold_back は F# と同じく初期値が最後です。

## ソース宣言の API 一覧

以下は同梱 std のソースから tsuzuri doc で生成した参照資料です。宣言順、型パラメーター、明示制約を保ちます。

| モジュール | ソース宣言 | 契約の解説 |
| --- | --- | --- |
| Array | [Array](api/Array.md) | [Array / List](arrays-and-lists.md) |
| List | [List](api/List.md) | [Array / List](arrays-and-lists.md) |
| Vec | [Vec](api/Vec.md) | [Vec](vec.md) |
| Map | [Map](api/Map.md) | [Map / Set](map-set.md) |
| Set | [Set](api/Set.md) | [Map / Set](map-set.md) |
| HashMap | [HashMap](api/HashMap.md) | [HashMap / HashSet](hash-map.md) |
| HashSet | [HashSet](api/HashSet.md) | [HashMap / HashSet](hash-map.md) |
| Seq | [Seq](api/Seq.md) | [Seq](sequences.md) |
| Option | [Option](api/Option.md) | [Option / Result](option-result.md) |
| Result | [Result](api/Result.md) | [Option / Result](option-result.md) |
| Exception | [Exception](api/Exception.md) | [例外と Result](option-result.md#例外と-result) |
| IO | [IO](api/IO.md) | [IO と標準入出力](io.md) |
| File | [File](api/File.md) | [OS API](os.md) |
| Dir | [Dir](api/Dir.md) | [OS API](os.md) |
| Path | [Path](api/Path.md) | [OS API](os.md) |
| Env | [Env](api/Env.md) | [OS API](os.md) |
| Time | [Time](api/Time.md) | [OS API](os.md) |
| Random | [Random](api/Random.md) | [OS API](os.md) |
| Os | [Os](api/Os.md) | [OS API](os.md) |
| Process | [Process](api/Process.md) | [OS API](os.md) |
| String | [String](api/String.md) | [文字列](text.md) |
| Utf8String | [Utf8String](api/Utf8String.md) | [文字列](text.md) |
| Char | [Char](api/Char.md) | [文字型](../language-reference/strings-and-characters.md) |
| Utf8Char | [Utf8Char](api/Utf8Char.md) | [文字型](../language-reference/strings-and-characters.md) |
| Format | [Format](api/Format.md) | [表示と解析](formatting-and-parsing.md) |
| Math | [Math](api/Math.md) | [Math](math.md) |
| BigInt | [BigInt](api/BigInt.md) | [BigInt](bigint.md) |
| Parallel | [Parallel](api/Parallel.md) | [Parallel](parallel.md) |
| Debug | [Debug](api/Debug.md) | [Debug](builtins.md) |
| Owned | [Owned](api/Owned.md) | [Owned](builtins.md#owned) |
| Test | [Test](api/Test.md) | [Test](builtins.md) |
| Gpu | [Gpu](api/Gpu.md) | [GPU](../guides/gpu.md) |

**生成宣言だけでは全 API の一覧にはなりません。** Int、Simd、Task、Owned の関数と、Vec / Math などの組み込み操作はコンパイラに実装され、上の手書き解説に含めています。また、本文で推論される Copy などの制約をすべてソース署名へ書き戻す生成器ではありません。

不透明な Map / Set / HashMap / HashSet / Seq / Gpu / IO / Owned / BigInt の内部フィールドがソース宣言として見えても、利用者による直接構築・分解を許可するものではありません。解説ページの所有権・可視性契約を優先します。

## 失敗と互換性

get / at / sub のような名前だけで失敗動作を推測しないでください。Array.get は None、Array.at / sub はトラップ、String.sub は None です。

std は .NET の標準ライブラリではなく、GUI とネットワークの API はありません。[OS API](os.md) はファイル・ディレクトリ・環境・時刻・乱数・プロセスに限られ、Windows を除く native と `--wasm-host wasi` で使えます。既定の WASM（`E2000`）と Windows の native（`E2002`）では使えません。モジュール名が予約されていても、他言語の同名 API がすべて使えるわけではありません。

## 関連項目

- [言語リファレンス](../language-reference/README.md)
- [API 文書の生成](../tools/documentation.md)
- [対応状況](../feature-status.md)
