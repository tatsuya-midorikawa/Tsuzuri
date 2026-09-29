# D02: string / utf8string 文字列ライブラリ

> 現行の [文字列仕様](../../docs/language.md#string-と-utf8string) と [A08](A08-char-type.md) に合わせ、
> UTF-16 の string / char は String、UTF-8 の utf8string / utf8char は Utf8String で扱います。
> 既存の索引・列挙・文字列変換の契約を維持し、両モジュールの追加 API を実装済みです。

| 項目 | 内容 |
| --- | --- |
| ID | D02 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02, A08, A11, C03, B01, (C02) |
| 後続 | E07, G06, C06 |
| 状態 | done |
| 主な影響ファイル（実装時） | `std/String.tz`, `std/Utf8String.tz` または E02 の builtin 登録, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `src/runtime/string.ll`, `src/runtime/utf8string.ll`, `src/runtime/wasm.ll`, `tests/strings.rs`, `tests/strings.mjs`, `tests/string_library.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

`string` と `utf8string` の両方に、検索・分割・変換・構築の標準 API を提供する。
共通の操作名を `String`／`Utf8String` に揃えつつ、前者は孤立サロゲートも保持する UTF-16 コード単位、後者は妥当な UTF-8 を扱う。
`char` は1 UTF-16 コード単位、`utf8char` は1 Unicode スカラーであり、索引の単位・文字の取得・比較順序を混同しない。
新しい検索・切り出し・UTF-8 検証の失敗は D-10 に従って `Option` を返す。
既存の添字アクセスや `Utf8String.from_string` のトラップ契約は変更しない。
WASM は import なしなので、`memcmp`/`memchr`/`strlen` などの外部 libc symbol には依存しない。
連続メモリと型ごとの LLVM 操作を使い、符号化の実行時 dispatch や不要な中間変換を避ける。SIMD/GPU 加速は本チケットの実装済み機能として主張しない。

## 現状

現行の `docs/language.md`／`docs/architecture.md` に基づく前提:

| 項目 | string | utf8string |
| --- | --- | --- |
| 値モデル | 孤立サロゲートも保持する UTF-16 コード単位列 | 妥当な UTF-8 バイト列 |
| リテラル | `"..."` | `u8"..."` |
| `.length`／モジュールの `length` | UTF-16 コード単位数、O(1) | UTF-8 バイト数、O(1) |
| 索引・既存 `for in` の要素 | `i16u` | `ubyte` |
| 複製 | `clone_string ref text` | `Utf8String.clone ref text` |
| `<`／`<=`／`>`／`>=` | 符号なし16-bitコード単位の辞書順 | 符号なし UTF-8 バイトの辞書順 |
| LLVM descriptor | `%tz.string = { ptr, i64 }` | `%tz.utf8string = { ptr, i64 }` |
| 主な runtime | `src/runtime/string.ll` | `src/runtime/utf8string.ll` |

両型は別の非 Copy 型で、buffer を所有し、drop を要する。`+` は両辺を消費し、比較・長さ・索引・列挙は読み取り借用。
索引の負数・範囲外はトラップする。NUL 終端ではなく、埋め込み NUL も値として保持する。
`string` の上限は 2^53 - 1 コード単位で、両型ともターゲットのアドレス空間・heap 上限に従う。

`String.from_utf8`、`Utf8String.from_string`、`String.is_well_formed`、`String.to_well_formed` は既存 API。
UTF-16 から UTF-8 への変換・コンソール出力は孤立サロゲートでトラップし、置換は `String.to_well_formed` による明示的な操作だけが行う。
両型の std モジュールと長さ取得も既存であり、新規作成ではなく拡張する。
実装着手時には `Type`、`Builtin`、比較 lowering、runtime 連結、フレーム移送の現在のコードと依存チケットの状態を再確認する。

## 仕様

### 前提とする他チケットのインターフェース

- E02: `String`／`Utf8String` の修飾名を解決し、std モジュール名の衝突は `E1011`。builtin-backed API は `Builtin` entry だけに置き、std source に同名の `def` を置かない。
- A08: 更新後の `char`（全 UTF-16 コード単位、LLVM `i16`）と `utf8char`（Unicode スカラー、LLVM `i32`）を提供する。両型は Copy/Eq/Ord、整数への明示変換は `Char.to_u16`／`Utf8Char.to_u32`。D02 の core は Display に依存しない。
- A11: `Eq`／`Ord` method の借用 signature と、被演算子を消費しない比較を提供する。
- C03: `ref [T]`（`&[T]`）により、`[string]`／`[utf8string]` を配列や要素のコピーなしに借用する。
- B01: `Option<'a>` を提供する。
- C02: `Vec<'a>` は弱依存。内部構築に使ってよいが、未完了でも二 pass allocation で D02 の契約を満たす。

### 他チケットへの提供インターフェース

D01 の将来の文字列補間、E07 の trace、A07 の deriving Display は `String.concat`／`String.join` で UTF-16 string を組み立てられる。
Display の戻り値を utf8string に変更せず、UTF-8 が必要な箇所では既存の明示変換を使う。公開 Builder 型は追加しない。
G06 は `String.find`／`slice`／`replace` でコード単位を失わず比較差分を扱える。
C06 の Map/Set は既存の `Ord<string>` に加え、A11+D02 後の `Ord<utf8string>` を利用できる。両型の順序は同じとは限らない。

### 基本方針

| 契約 | String | Utf8String |
| --- | --- | --- |
| index／length／返却 offset | UTF-16 コード単位 | UTF-8 バイト |
| 検索・prefix・suffix | 符号なし16-bitコード単位列の完全一致 | バイト列の完全一致 |
| 切り出し境界 | 範囲内のすべてのコード単位境界。ペアの途中も可 | 範囲内のスカラー境界だけ |
| `decode_at` の要素 | char。surrogate も有効、次の offset は +1 | utf8char。次の offset は +1～4バイト |
| `char_count`／`chars` | コード単位数／`[char]`。ペアは2要素 | スカラー数／`[utf8char]`。補助平面も1要素 |
| `compare`／Ord | 符号なし16-bitコード単位の辞書順 | 符号なし UTF-8 バイトの辞書順（スカラー値順と一致） |
| buffer transfer | `[i16u]` とのコード単位移送 | `[ubyte]` とのバイト移送 |

`string` は well-formed UTF-16 である必要がない。検索・分割・切り出し・文字取得で surrogate を拒否・置換せず、サロゲートペアの分断も許す。
`utf8string` は常に妥当な UTF-8 を保ち、バイト索引とスカラー取得を分ける。いずれも正規化や locale による処理はしない。
既存の `.length`、`String.length`／`Utf8String.length`、`text[index]`、`for in` の型・単位は変えない。

両モジュール間、string と utf8string、char と utf8char の暗黙変換は導入しない。
読み取り API は共有借用、戻り文字列・配列は独立した所有値。所有権移送 API だけが入力を消費する。
すべての引数は左から右に一度だけ評価し、借用・move・drop の既存規則を保つ。

### API 一覧

`ref T` は共有借用で、`&T` と同じ。以下の関数名は各列のモジュールで修飾する。
モジュールをまたぐ引数の混在や、リテラルの型の自動切り替えは認めない。

| 関数 | String の型 | Utf8String の型 |
| --- | --- | --- |
| `length`（既存） | `ref string -> i64` | `ref utf8string -> i64` |
| `concat` | `ref [string] -> string` | `ref [utf8string] -> utf8string` |
| `join` | `ref string -> ref [string] -> string` | `ref utf8string -> ref [utf8string] -> utf8string` |
| `split` | `ref string -> ref string -> [string]` | `ref utf8string -> ref utf8string -> [utf8string]` |
| `find`／`rfind` | `ref string -> ref string -> Option<i64>` | `ref utf8string -> ref utf8string -> Option<i64>` |
| `contains`／`starts_with`／`ends_with` | `ref string -> ref string -> bool` | `ref utf8string -> ref utf8string -> bool` |
| `slice`／`sub` | `ref string -> i64 -> i64 -> Option<string>` | `ref utf8string -> i64 -> i64 -> Option<utf8string>` |
| `decode_at` | `ref string -> i64 -> (char * i64)` | `ref utf8string -> i64 -> (utf8char * i64)` |
| `char_count` | `ref string -> i64` | `ref utf8string -> i64` |
| `chars` | `ref string -> [char]` | `ref utf8string -> [utf8char]` |
| `trim`／`trim_start`／`trim_end` | `ref string -> string` | `ref utf8string -> utf8string` |
| `to_ascii_lower`／`to_ascii_upper` | `ref string -> string` | `ref utf8string -> utf8string` |
| `replace` | `ref string -> ref string -> ref string -> string` | `ref utf8string -> ref utf8string -> ref utf8string -> utf8string` |
| `repeat` | `ref string -> i64 -> string` | `ref utf8string -> i64 -> utf8string` |
| `compare` | `ref string -> ref string -> i64` | `ref utf8string -> ref utf8string -> i64` |

符号化ごとの buffer transfer API は名前も分ける:

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `String.to_code_units` | `string -> [i16u]` | 所有 buffer を UTF-16 コード単位配列へ移す |
| `String.from_code_units` | `[i16u] -> string` | 全コード単位をそのまま所有 string へ移す。surrogate も有効 |
| `Utf8String.to_bytes` | `utf8string -> [ubyte]` | 所有 buffer を UTF-8 バイト配列へ移す |
| `Utf8String.from_bytes` | `[ubyte] -> Option<utf8string>` | UTF-8 検証に成功すれば移す。失敗なら入力を解放して None |

旧案の `String.to_bytes`／`String.from_bytes` は `Utf8String` 側へ移す。UTF-16 の生メモリを UTF-8 とみなす API や、endian 未指定のバイト表現は追加しない。

### 連結・分割

`concat parts` は全要素を順に連結する。入力 slice と要素は借用で消費しない。
`join separator parts` は各要素の間に separator を挿入する。空 slice は各型の空文字列、要素1個なら独立した clone を返す。
非空の結果は buffer を1回だけ確保し、空の結果は既存の空 descriptor を使ってよい。
長さ合計・separator 分の乗算・確保バイト数・型とターゲットの上限を検査し、超過はトラップする。
複雑度は concat が O(n + total units)、join が O(n + total units + separator.length * max(n - 1, 0))。n は要素数、units は String がコード単位、Utf8String がバイト。

`split separator text` は重ならない一致を左から右に処理する。
非空 separator では先頭・末尾・連続する separator による空要素も保持し、空 text は空文字列1個の配列になる。一致なしなら text の clone 1個。
空 separator の場合は、String が1コード単位ずつ、Utf8String が1スカラーずつの所有文字列配列を返す。先頭・末尾の空要素は付けず、空 text なら空配列。
したがって `"😀"` の空 separator 分割は `"\uD83D"` と `"\uDE00"` の2要素、`u8"😀"` は1要素となる。

text と separator は消費しない。結果配列と各 substring は所有値であり、入力 buffer を共有しない。
C02 がなければ1 pass 目で個数を求め、2 pass 目で配列を初期化する。空 separator の個数は String なら `.length`、Utf8String なら scalar 計数で求める。
allocation は非空の結果配列1回と、非空の substring ごとに1回。空の値は既存の空表現を使ってよい。

### 検索

`find needle haystack` は最初の offset、`rfind needle haystack` は最後の offset を `Some` で返し、不一致は `None`。
offset は String がコード単位、Utf8String がバイト。空 needle は find が `Some 0`、rfind が `Some haystack.length`。
String では surrogate 単独の needle も有効で、ペアの high／low surrogate の位置にも一致できる。
Utf8String では妥当な UTF-8 同士のバイト検索なので、非空 needle の一致区間の両端は必ずスカラー境界になる。

`contains needle text` は find の有無を bool にする。
`starts_with prefix text`／`ends_with suffix text` は同じ型のコード単位列／バイト列の prefix／suffix 比較。空 prefix／suffix は true。
検索と比較は埋め込み NUL を終端として扱わない。

### 切り出し

`slice text start finish` は開始 inclusive、終了 exclusive で、`0 <= start <= finish <= text.length` が必要。
String はこの範囲条件だけで `Some substring` とし、サロゲートペアを分断する境界も許す。
Utf8String はさらに両端が UTF-8 スカラー境界であることを要求する。continuation byte の位置は、`start == finish` でも `None`。
`start == finish == text.length` は両型とも `Some` 空文字列。負数・逆順・範囲外は `None` で、JS の負数補正や範囲の丸めは導入しない。

`sub text start length` は同じ型の単位で長さを受ける。負の start／length、`start + length` の overflow は `None`。
それ以外は `slice text start (start + length)` と同じ。
検査はメモリアクセス前に行い、失敗時は確保しない。成功した非空 substring は独立した buffer を1回確保する。
境界検査は O(1)、コピーは O(substring.length)。

### 文字の取得

`String.decode_at text offset` はコード単位を1個読み、`(char, offset + 1)` を返す。
`0 <= offset < text.length` なら surrogate でも成功する。名前に decode を含むが、サロゲートペアをスカラーに合成する関数ではない。
`Utf8String.decode_at text offset` はバイト位置から1スカラーを復号し、`(utf8char, next_byte_offset)` を返す。
負数、終端を含む範囲外、continuation byte の位置、不正・途切れた UTF-8 はトラップ。妥当な utf8string のスカラー先頭なら成功する。
両関数とも O(1) であり、String は1コード単位、Utf8String は最大4バイトを読む。

`String.char_count` は char の個数、すなわち `.length` と同じコード単位数を O(1) で返す。
`Utf8String.char_count` は Unicode スカラー数を O(bytes) で数える。どちらも書記素クラスタ数ではない。
`String.chars` は全コード単位を `[char]` にコピーし、surrogate も保持する。`Utf8String.chars` は全スカラーを復号した `[utf8char]` を返す。
入力は借用、配列は独立した所有値。非空なら結果配列の確保は1回、要素ごとの確保はしない。
前者は O(code units)、後者は計数と復号の二 pass でも O(bytes)。配列バイト数の計算は要素幅を考慮し上限検査する。

これらの API は既存の索引や `for in` を文字型に変更するものではない。

### 空白除去・ASCII 変換

`trim text`／`trim_start text`／`trim_end text` は ASCII whitespace だけを対象にする。
対象は `0x09`, `0x0A`, `0x0B`, `0x0C`, `0x0D`, `0x20` のコード単位／バイト。Unicode whitespace は対象外。
`to_ascii_lower text`／`to_ascii_upper text` は ASCII A-Z／a-z だけを変換する。
String は非 ASCII コード単位（surrogate を含む）、Utf8String は非 ASCII バイトをそのまま保持する。
変化がなくても独立した clone を返し、非空結果の buffer 確保は1回。Utf8String の妥当性を壊さない。

### 置換・繰り返し・比較

`replace needle replacement text` は重ならない一致を左から右に置換する。replacement は検索・置換の対象に再投入せず、そのまま挿入する。
空 needle は先頭・各要素間・末尾に replacement を挿入する。要素は String がコード単位、Utf8String がスカラー。
String ではペアの途中にも挿入し、孤立サロゲートになっても保持する。Utf8String は UTF-8 の途中には挿入しない。空 text への挿入は1回。
1 pass 目で件数と結果長を求め、2 pass 目で確保済みの結果へ書き込む。非空結果の buffer 確保は1回。

`repeat text count` は count 回連結する。負の count は空 text に対してもトラップ。
`count == 0` または空 text なら各型の空文字列。長さの乗算・型の上限・確保可能量の超過はトラップする。
非空結果の buffer 確保は1回、複雑度は O(text.length * count)。

`compare left right` は辞書順を -1／0／1 で返す。
String は現行の符号なし16-bitコード単位順、Utf8String は符号なし UTF-8 バイト順。同じ prefix なら短い方を先とする。
後者だけがスカラー値順と一致する。例えば U+1F600 と U+E000 は String では前者が小さく、Utf8String では前者が大きい。
両者とも locale・大文字小文字・正規化による補正をしない。

### 配列との所有権移送

`String.to_code_units`／`String.from_code_units` は所有 buffer を `[i16u]`／string へ移す。
`[i16u]` の全要素が有効で、surrogate の検証は不要。from_code_units は Option を返さず、空配列は `""` になる。
string の長さ上限などの資源検査は必要で、超過はトラップする。

`Utf8String.to_bytes` は所有 buffer を `[ubyte]` へ移す。
`Utf8String.from_bytes` は配列を消費し、妥当な UTF-8 なら同じ buffer を移して `Some`、不正なら入力 buffer を解放して `None`。空配列は `Some u8""`。
入力の UTF-8 検証は O(bytes)、検証後の移送自体は O(1) であり、関数全体を O(1) と記載しない。不正位置や元の配列は返さない。

他の移送は資源検査を除き O(1) で、既に所有する heap buffer の追加コピー・確保はない。
返却値が唯一の所有者になり、その drop で buffer を解放する。元の値は move 済みで再利用できない。
フレーム・静的リテラルからの通常の heap 移送が必要な場合の扱いは「Buffer transfer」に従う。

### 既存の符号化変換との関係

| 既存 API | 維持する型と契約 |
| --- | --- |
| `String.from_utf8` | `ref utf8string -> string`。復号した独立の所有値 |
| `Utf8String.from_string` | `ref string -> utf8string`。符号化した独立の所有値。孤立サロゲートはトラップ |
| `String.is_well_formed` | `ref string -> bool`。孤立サロゲートの有無を検査 |
| `String.to_well_formed` | `ref string -> string`。孤立サロゲートを明示的に U+FFFD へ置換 |

D02 はこれらを再定義しない。符号化変換は buffer transfer とは別で、O(1) の再解釈ではない。
UTF-8 の `[ubyte]` から string が必要な場合は `Utf8String.from_bytes` の成功値を `String.from_utf8` へ渡す。
string を UTF-8 の `[ubyte]` にする場合は `Utf8String.from_string` の結果に `Utf8String.to_bytes` を使う。surrogate の暗黙置換はしない。

### Ord instance

`Ord<string>` は既存のコード単位順を保ち、`String.compare` と同じ比較経路を使う。
`Ord<utf8string>` を A11 の借用ベースの組み込み instance として追加し、`Utf8String.compare` と同じバイト辞書順を使う。
`Ord.lt`／`le`／`gt`／`ge` は型ごとの `ref T -> ref T -> bool` で、演算子も両辺を消費しない。
両型の既存の Eq は保持する。組み込み Ord の上書きは `E1016`、両文字列型の混在比較は型エラー。
演算子とモジュール関数で異なる符号化変換や性能経路を選ばない。

### トラップと Option

| 状況 | 結果 |
| --- | --- |
| 検索失敗 | `None` |
| slice／sub の負数・逆順・範囲外・加算 overflow | `None` |
| Utf8String.slice／sub のスカラー境界違反 | `None` |
| String の範囲内の surrogate 読み出し・切り出し | 成功。ペアの途中も有効 |
| Utf8String.from_bytes の不正 UTF-8 | 入力を解放して `None` |
| decode_at の負数・終端を含む範囲外 | トラップ |
| Utf8String.decode_at の境界違反・不正 UTF-8 | トラップ |
| repeat の負 count、結果長・確保サイズの overflow、資源上限・確保失敗 | トラップ |
| 既存の Utf8String.from_string／string コンソール出力の孤立サロゲート | 従来どおりトラップ |

既存の `text[index]` は範囲検査だけを行う。utf8string の continuation byte の索引も有効な `ubyte` 読み出しであり、decode_at とは違う。

## 設計

### std 実装と builtin の切り分け

小さく型だけで書ける helper は既存の `std/String.tz`／`std/Utf8String.tz` に追加する。
ただし builtin-backed API 関数は std source に同名 `def` を置かない。
同名 `def` は E02 の解決順で builtin を shadow するため禁止する。
コード単位／バイト走査、buffer transfer、UTF-8 検証、比較 lowering は適切な既存 builtin/runtime を再利用・拡張する。

| 操作 | 配置の方針 |
| --- | --- |
| concat／join／split／find／rfind／prefix／suffix／slice／chars／trim／ASCII 変換／replace／repeat | 両型の通常の std 関数。確保は正確な容量の Vec、既存の型付きアクセス・特殊化を再利用 |
| contains／sub | 各モジュールの std helper にできる。sub の overflow 検査を省かない |
| length／String.char_count | 既存 length と descriptor 取得を再利用。UTF-16 の scalar 計数処理は追加しない |
| decode_at | String は `i16` の範囲検査付き読み出し、Utf8String は UTF-8 decoder |
| compare／Ord | String は現行経路を維持、Utf8String はバイト比較経路を追加 |
| code unit transfer | `StringToCodeUnits`／`StringFromCodeUnits` |
| byte transfer | `Utf8StringToBytes`／`Utf8StringFromBytes` |

E02 の登録は `String.find`／`Utf8String.find` のように型ごとに行い、型検査時に呼び先を決める。
共有できる生成処理は共有してよいが、実行時の文字列型タグや汎用 encoding dispatcher は追加しない。
`Classes::intrinsic` は既存の string instance を保ち、`Ord<utf8string>` を追加する。Eq と比較演算子は対応するモジュール関数と同じ型付き経路へ下げる。

### runtime の拡張

runtime symbol は `define internal` とし、型に応じて `@tz.string.`／`@tz.utf8string.` prefix を使う。
`emit_target` の既存の条件付き連結を確認し、必要な runtime だけを重複なく連結する。
WASM import を増やさない。
外部 libc symbol の `@memcmp`, `@memchr`, `@strlen`, `@memcpy`, `@memmove` は使わない。
一方、LLVM intrinsic の `@llvm.memcpy.*` と `@llvm.memmove.*` は使用してよい。
これらは wasm32 bulk-memory で import なしに lower できることを object/link test で確認する。

| 処理 | String 側 | Utf8String 側 |
| --- | --- | --- |
| find／prefix／suffix | `i16` の一致。コード単位 offset を返す | `i8` の一致。バイト offset を返す |
| compare | 既存の符号なし `i16` 比較を再利用 | 符号なし `i8` 比較 |
| slice_copy | コード単位範囲検査と対応する `i16` 領域のコピー | バイト範囲・スカラー境界検査とコピー |
| decode_at の返却値 | `{ i16, i64 }` | `{ i32, i64 }` |
| char_count | descriptor の長さ | UTF-8 スカラー計数 |
| valid_utf8／is_boundary | 不要。well-formed 検査を String の一般操作へ入れない | 既存の検証・decoder を再利用・拡張 |

String の検索はコード単位境界に揃え、ホスト endian に依存する生バイト比較で順序を決めない。
符号なし比較、範囲内 load、要素幅と長さ単位を保持する。既存の最適化済み copy／比較を重複実装しない。
intrinsic を使う場合も byte count への換算を検査し、外部 `@memcpy` libcall を要求しない。

### UTF-8 decode

Utf8String 専用の検証・復号は既存の厳密な変換処理を再利用する。必要な規則:

- 1 byte: `0xxxxxxx`。
- 2 byte: `110xxxxx 10xxxxxx`、code point >= U+80。
- 3 byte: `1110xxxx 10xxxxxx 10xxxxxx`、code point >= U+800、surrogate ではない。
- 4 byte: `11110xxx 10xxxxxx 10xxxxxx 10xxxxxx`、U+10000..U+10FFFF。
- 読み取り前に必要バイト数を確認する。途切れた列、lone continuation、過長 encoding、その他の不正値は invalid。

`Utf8String.from_bytes` は invalid なら入力を解放して `None`、`Utf8String.decode_at` は trap。
妥当な utf8string の境界検査は、範囲確認後に `index == 0 || index == length || (byte & 0xC0) != 0x80` で判定できる。終端で byte をロードしない。
外部由来の配列は必ず全体を検証し、既存の validated 内部経路を無検証の入力に使わない。
この scalar 検査を char／String のコード単位取得には適用しない。

### Buffer transfer

`%tz.string`／`%tz.utf8string`／`%tz.array` の `{ ptr, i64 }` descriptor を、`extractvalue`／`insertvalue` で組み替える。
String と `[i16u]` は同じ `i16` 要素の個数、Utf8String と `[ubyte]` は同じ `i8` 要素の個数を保持する。文字列の長さをバイト数へ誤変換しない。
descriptor が同じ形という理由だけで移送せず、要素 stride・alignment・確保／解放規約が一致することを native/WASM で確認する。

所有値を消費する builtin として、`FunctionEmitter::call` 等の引数 move 規則に従う。
成功時は元 buffer を drop せず、新しい所有者だけが後で解放する。
`Utf8String.from_bytes` の invalid 分岐は buffer を1回解放して None。`i16u`／`ubyte` に要素 drop は不要。
String.from_code_units に surrogate の invalid 分岐は存在しない。

フレームや静的領域のリテラルでは、既存の所有値の heap 移送を先に適用する。静的領域や借用先の pointer を free 可能な所有配列へ渡してはいけない。
O(1)・追加確保なしの検査は既に所有する heap buffer を対象とし、フレーム由来の必要な移送とは区別する。
借用中の buffer の move、移送後の使用、二重解放、invalid 分岐のリークを検査する。

### Allocation 設計

両型とも連結・join・replace・repeat は結果長を先に計算し、非空結果を1回だけ allocate する。
長さの加算・乗算は checked arithmetic または事前の上限比較で検査し、折り返した値で確保・書き込みをしない。
String は 2^53 - 1 コード単位上限と2倍した確保バイト数、Utf8String はバイト数とターゲットの上限を検査する。
chars／split の結果配列も個数と要素サイズの積を検査する。空 needle の replace の挿入回数（要素数 + 1）にも overflow 検査が必要。

split は C02 なしの two-pass 経路で契約を満たす。C02 を使ってもよいが、API の所有権や空要素の規則を変えず、追加確保は測定上区別する。
substring は必ず独立した所有値。C03 は配列 slice であり、文字列の substring view や新しい寿命モデルを追加するものではない。

### 評価順序

両モジュールとも `replace needle replacement text` は needle、replacement、text の順に一度だけ評価される。
空 needle の特殊処理や複数 pass でも引数式を再評価しない。
split／replace は重ならない一致を左から右に処理する。rfind は最後の一致を返すが、引数の評価順は同じ。
処理中に読み取り借用の所有者を move・無効化する操作は、既存の所有権検査で拒否する。

## 実装手順

1. **依存確認**: E02、更新後の A08、A11、C03、B01 が `done` であることを確認する。C02 は未完了でもよい。A08 と GUIDE の旧文字型定義の相違を解消してから実装する。
2. **API 登録**: 既存の両 std モジュールに helper を追加し、両型の修飾 builtin を登録する。全 signature、モジュール名の予約、同名 std def による shadow がないことを確認する。
3. **検索・比較**: 型ごとの find／prefix／suffix／compare を実装し、`Ord<string>` を維持して `Ord<utf8string>` を追加する。surrogate の検索と、補助平面／BMP の順序差、演算子との一致を確認する。
4. **切り出し**: slice／sub を Option に下げる。String のペア途中は Some、Utf8String の continuation 位置は None、既存 indexing は数値読み出しのままであることを確認する。
5. **文字取得**: decode_at／chars／char_count を追加する。String は char・コード単位数、Utf8String は utf8char・スカラー数で、`Char.to_u16`／`Utf8Char.to_u32` から参照値を照合する。
6. **所有権移送**: コード単位／バイトの各 transfer を実装する。heap buffer の追加コピーなし、surrogate の往復、不正 UTF-8 の None と1回だけの解放、フレーム経路の安全性を確認する。
7. **構築・変換**: concat／join／split／trim／ASCII 変換／replace／repeat を追加する。空入力・空区切りの型別動作、確保回数、長さ単位・overflow・資源上限を確認する。
8. **runtime と E2E**: 両 runtime の必要時だけの連結、intrinsic 宣言の重複なし、IR 決定性、外部 libc symbol 不在、native/WASM × `-O0`/`-O3`、WASM imports 空を確認する。
9. **docs**: 実装完了後に仕様と README を更新する。既存の索引・列挙・符号化変換の契約が変わっていないことも既存テストで確認する。

## テスト計画

### Rust tests

既存の `tests/strings.rs` を拡張し、全 API の signature と型ごとの lowering を検査する。実行時の値は Node E2E でも照合する。

受理例:

```text
let needle = "ll"
let text = "hello"
let found: Option<i64> = String.find ref needle ref text
let characters: [char] = String.chars ref text

let utf8_needle = u8"ll"
let utf8_text = u8"hello"
let utf8_found: Option<i64> = Utf8String.find ref utf8_needle ref utf8_text
let utf8_characters: [utf8char] = Utf8String.chars ref utf8_text

let units: [i16u] = String.to_code_units "\0\uD800"
let restored: string = String.from_code_units units
let bytes: [ubyte] = Utf8String.to_bytes u8"hé"
let utf8_restored: Option<utf8string> = Utf8String.from_bytes bytes
```

| 境界ケース | String の期待値 | Utf8String の期待値 |
| --- | --- | --- |
| `"hé"`／`u8"hé"` の slice 1 2 | `Some "é"` | `None`（終端が continuation 位置） |
| `"😀"`／`u8"😀"` の slice 0 1 | `Some "\uD83D"` | `None` |
| 同じ text の decode_at 0 | `('\uD83D', 1)` | `(u8'\u{1F600}', 4)` |
| 同じ text の decode_at 1 | `('\uDE00', 2)` | トラップ |
| 同じ text の char_count | 2 | 1 |
| 空 text の slice 0 0 | `Some ""` | `Some u8""` |
| 空 needle の find／rfind | 0／コード単位長 | 0／バイト長 |

拒否・所有権:

- String の API へ utf8string、Utf8String の API へ string を渡す、両型を混在比較する: `E1003`。
- `String.from_code_units [1ubyte]`、`Utf8String.from_bytes [1i16u]`／`[1i8]`: `E1003`。整数配列の幅や符号を暗黙変換しない。
- 所有権移送 API に `ref string`／`ref utf8string`／借用配列を渡す: `E1003`。共有借用を必要とする通常 API は既存の自動借用も維持する。
- chars の要素に算術を適用する: A08 の Numeric 拒否（`E1005`）。char と utf8char を混在させない。
- transfer 後の元の所有値の再使用は `E1012`、借用が生きている間の transfer は `E1014`。
- 両型の組み込み Ord instance 上書きは `E1016`。公開 ABI に両文字列型や文字配列を追加しない。

IR 検査:

- String の decode_at／chars は `i16`、Utf8String は `i32` の文字値を生成する。UTF-16 の surrogate 拒否を一般操作に入れない。
- heap buffer transfer にコピー loop や追加確保がなく、from_bytes の invalid 分岐は1回の free と None になる。
- 比較演算子と各モジュールの compare が同じ型別の比較経路を使い、String を UTF-8 に変換しない。
- runtime／intrinsic の宣言は重複せず、IR は決定的。
- 外部 `@memcmp`／`@memchr`／`@strlen`／`@memcpy`／`@memmove` の参照がない。`@llvm.memcpy.*`／`@llvm.memmove.*` は許可し、native/WASM object/link と imports 空でも確認する。

### Node E2E

既存 `tests/features.mjs` の `string_library` suite と fixtures を使用する。native `-O0`／`-O3` は malloc/free tracking で各呼び出し後に `live == 0`、WASM `-O0`／`-O3` は imports 空を確認する。
両型の追加 API に加え、既存の `tests/strings.mjs` で長さ・索引・列挙・符号化変換を回帰検査する。

受理・境界・参照値:

- 両型で ASCII concat／join／split／find／rfind／contains／prefix／suffix／replace／repeat。空入力、空 needle、連続 separator、先頭・末尾の空要素、重なりのある needle、埋め込み NUL を含める。
- `"日本語😀"` は `.length == char_count == 5`、`u8"日本語😀"` は `.length == 13`、`char_count == 4`。chars の全要素を `Char.to_u16`／`Utf8Char.to_u32` で照合する。
- String はペアの両方と孤立した high／low surrogate を検索・切り出し・decode できる。Utf8String の slice／sub はスカラー境界だけ成功し、continuation 位置では空区間でも None。
- slice／sub の負数・逆順・範囲外・加算 overflow は両型とも None。offset 0／length、ASCII／2・3・4バイト境界を検査する。
- 空 separator の split は String がコード単位ごと、Utf8String がスカラーごと。元の separator で join するとコード単位／バイト列が正確に復元する。
- 空 needle の replace は String がペア途中にも挿入し、Utf8String はスカラー間だけに挿入する。空 text には1回だけ挿入する。
- trim は ASCII whitespace のみ。ASCII case mapping は `"Ä"`／`u8"Ä"`、補助平面、String の孤立サロゲートを変えない。
- コード単位 transfer は全65536値、NUL、ペア、孤立サロゲートを往復できる。バイト transfer は妥当な UTF-8 と NUL を往復し、既に所有する heap buffer の追加コピー・確保がない。
- from_bytes は lone continuation、途切れた列、overlong、surrogate、U+10FFFF 超過で None、リークなし。妥当値との境界を含める。
- compare は空、prefix、ASCII、BMP、補助平面を含め、String は surrogate も検査する。U+1F600 と U+E000 の順序差と、両型の比較演算子との一致を確認する。
- concat／join／replace／repeat の結果 buffer 確保は非空なら1回。chars／split の確保・解放、フレーム／静的リテラル由来の所有値、入力を保持したまま結果だけを解放する経路を検査する。
- String の孤立サロゲートを扱う結果は UTF-8 console に出さず、コード単位チェックサムで観測する。既存の明示的な符号化変換・well-formed 検査も維持する。

トラップ:

- 両型の decode_at の負数、空 text、`offset == length`、範囲外。Utf8String の continuation byte 位置。String の範囲内の surrogate はトラップしない。
- 両型の repeat の負 count（空 text も含む）、長さ／確保サイズの overflow、WASM の heap 上限超過。
- String の 2^53 - 1 コード単位上限超過。大きな書き込みの前にトラップすることを確認する。
- 既存の `Utf8String.from_string` と string console の孤立サロゲート。暗黙置換や None への変更をしない。

参照実装:

- String は JS の `.length`／`charCodeAt`／コード単位での検索・辞書順を使う。`for...of`／`Array.from(text)` はスカラー単位なので char 配列の参照には使わない。slice は JS の負数補正の前に本仕様の範囲検査を行う。
- Utf8String は妥当な JS 文字列を `TextEncoder` で符号化し、バイト offset と独立したスカラー計数で照合する。
- UTF-8 検証には `TextDecoder("utf-8", { fatal: true, ignoreBOM: true })` を使い、先頭の U+FEFF もデータとして保持する。不正列は手書きの byte array を使う。
- 孤立サロゲートを `TextEncoder` へ渡すと置換されるため、String の保存性や変換失敗の期待値には使わない。長さ・overflow の期待値は JS BigInt で計算する。

両文字列型、文字型、それらの配列・Option は公開 ABI で直接 export しない。
fixture は既存 ABI の bool／整数チェックサム、型ごとの長さ、コード単位／スカラー値などを返す helper で観測する。

### 実装完了時の検証コマンド

実装完了時に以下を検証済み。
GUIDE §3 に従い小さい範囲から確認し、各 Node E2E の直前に release build を実行する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test strings
cargo test --locked
cargo build --release --locked && node tests/features.mjs target/release/tsuzuri string_library
cargo build --release --locked && node tests/strings.mjs target/release/tsuzuri
cargo build --release --locked && node tests/primitives.mjs target/release/tsuzuri
```

`tests/features.mjs ... string_library` が両文字列型の native/WASM × `-O0`/`-O3` と WASM imports 空を検査する。確保回数は `tests/primitives.mjs` で確認する。

## ドキュメント

以下の現行仕様・設計台帳・実装済み一覧を同期済み。

- `docs/language.md`: 両モジュールの API 表、コード単位／バイト offset、char／utf8char、slice の境界・Option、transfer の所有権と検証コストを記載する。
- 同仕様の既存 `Ord<string>` と符号化変換を維持し、`Ord<utf8string>` と両型の非消費比較を記載する。索引・列挙の要素型が変わらないことを明記する。
- `README.md`: 両型の split／chars の例と実装済み API、実際に追加した検証コマンドを記載する。
- `docs/architecture.md`: 両 runtime の型別 helper、`i16`／`i8` の buffer、文字値の `i16`／`i32`、所有権移送、連結条件、libc 非依存を記載する。
- `_features/GUIDE.md` と関連チケット: 更新後の A08、String／Utf8String の API と順序の違いに同期する。D01／A07 の Display は UTF-16 string のまま。
- `docs/benchmarks.md`: 性能主張をしなければ更新不要。SIMD/vectorize を主張する場合は生成コードと測定条件を記録し、CI に速度閾値を置かない。

## 受け入れ条件

- [x] 両モジュールの全 API が仕様の signature で使え、文字列型・文字型の暗黙変換を導入しない。
- [x] String は全 UTF-16 コード単位を保持し、検索・切り出し・分割・文字取得で surrogate を拒否・置換しない。
- [x] Utf8String は妥当な UTF-8 を保ち、scalar boundary が必要な slice／sub は `None` で失敗する。
- [x] 両型の slice／sub は負数・逆順・範囲外・加算 overflow で `None`。String のペア途中は有効。
- [x] decode_at／chars／char_count が String は char とコード単位、Utf8String は utf8char とスカラーを扱う。
- [x] 既存の長さは両型とも `i64` で、string はコード単位数、utf8string はバイト数。索引・列挙の要素型はそれぞれ `i16u`／`ubyte` のまま。
- [x] 空 needle／separator、NUL、非 ASCII、ASCII whitespace、replacement の非再解釈が両型の契約どおり。
- [x] String のコード単位 transfer と Utf8String のバイト transfer が、所有 heap buffer の追加コピーなしで往復する。
- [x] Utf8String.from_bytes は O(bytes) で検証し、失敗時に入力を1回解放して `None`。String.from_code_units は surrogate を含め有効。
- [x] 既存の `Ord<string>` はコード単位順を維持し、追加する `Ord<utf8string>` は UTF-8 バイト順。比較は非消費で compare と一致する。
- [x] 長さの単位・要素幅・overflow・2^53 - 1 コード単位上限・ターゲットの資源上限を正しく検査する。
- [x] 既存の明示的な符号化変換と孤立サロゲートのトラップ方針を変更しない。
- [x] 両型とも native/WASM `-O0`/`-O3` で同じ結果。所有権移送・結果解放でリークや二重解放がない。
- [x] WASM imports は空。
- [x] IR は決定的で runtime／intrinsic の連結は重複なし。外部 libc string/memory call がなく、native/WASM object/link と imports 空で検証済み。
- [x] docs が string／utf8string、char／utf8char、コード単位／バイト／スカラーの違いを明記する。

## 落とし穴

- String の index を byte offset と書かない。Utf8String の byte offset をスカラー番号や utf8char と同一視しない。
- char は UTF-16 コード単位であり Unicode スカラーではない。String.chars／decode_at でペアを合成しない。
- String の一般操作に well-formed 検査を追加しない。孤立サロゲートの保持と UTF-8 変換時のトラップは別の契約。
- Utf8String の continuation byte は通常の索引では読めるが、slice では None、decode_at では trap。両 API を混同しない。
- String.compare を UTF-8 経由や生バイトの endian 依存順へ変更しない。`Ord<string>` は既に実装済み。
- transfer 後に元 buffer を drop しない。from_bytes の invalid 分岐は leak させず、String の surrogate に同じ invalid 分岐を適用しない。
- UTF-8 検証と所有権移送を合わせて O(1) と呼ばない。フレーム／静的領域を free 可能な heap とみなさない。
- String の2バイト幅、Utf8String の1バイト幅、utf8char 配列の4バイト幅を区別する。確保前に長さ・積の上限を検査する。
- ASCII 変換で非 ASCII コード単位／UTF-8 バイトを変更しない。NUL は終端ではなくデータ。
- 外部 libc symbol は追加せず、substring は借用 view ではなく独立の所有値にする。

## 対象外

- Unicode 正規化、locale aware case mapping、grapheme cluster、正規表現。
- 公開 string builder 型、substring view、mutable string、format string、I/O。
- 既存の索引・`for in` の要素型変更、string のスカラー列挙 API、文字型間のサロゲートペア合成・分解 API。
- UTF-32 変換、endian 付き UTF-16 byte serialization。既存の UTF-16／UTF-8 変換は維持するが、新しい変換方式は追加しない。
- `from_bytes` 失敗時に入力配列を返す API、暗黙変換、暗黙の U+FFFD 置換、公開 ABI の拡張。

## 未決事項

- **解決（2026-09-27）:** A08 と GUIDE を両文字型へ同期。通常の std ソースを優先し、buffer transfer・検証・比較だけを builtin/runtime に置く。検索は追加確保なし、最悪 O(text.length * needle.length) であり、線形時間や SIMD 加速は主張しない。
- `tests/features.mjs ... string_library` の 3,230 ケース、既存 strings の全 Unicode スカラー検査、storage の単一結果バッファ確保を native/WASM O0/O3 で確認。multiline else の解析と未使用 std の先行特殊化も回帰修正した。
- **既定案: 操作名は両モジュールで揃える。** split は `separator -> text`、replace は `needle -> replacement -> text`。型と単位は各モジュールの契約で固定する。
- **既定案: String.decode_at／char_count はコード単位を扱う。** Utf8String の同名 API はスカラーを扱う。String にスカラー走査を追加する場合は別名・別チケットとし、孤立サロゲートの失敗方針を明示する。
- **既定案: 空区切りは各文字型の1要素単位。** String は surrogate を含むコード単位、Utf8String はスカラー。ペアを分割しないために String 側の意味を暗黙に変えない。
- **既定案: UTF-16 配列移送は `to_code_units`／`from_code_units`。** `to_bytes`／`from_bytes` は Utf8String 専用とし、コード単位と符号化済みバイトを区別する。
- **既定案: trim は ASCII のみ、compare は i64 の -1／0／1。** Unicode whitespace や文字分類の拡張は後続チケットで扱う。
