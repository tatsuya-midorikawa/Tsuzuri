# D02: 文字列ライブラリ
| 項目 | 内容 |
|---|---|
| ID | D02 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02, A08, A11, C03, B01, (C02) |
| 後続 | E07, G06, C06 |
| 状態 | todo |
| 主な影響ファイル | `std/String.tz`, `std/String.tt` または E02 の builtin 登録, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/runtime/string.ll`, `src/runtime/wasm.ll`, `tests/strings.rs`, `tests/string_library.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

`string` を単なる UTF-8 byte buffer から、標準ライブラリとして検索・分割・変換・構築できる実用的な型へ拡張する。
既存仕様では `string.length` と `text[index]` は byte 単位で、`+` と `==` だけが組み込みである。
D02 はこの byte 単位仕様を保ちつつ、UTF-8 の妥当性を壊さない関数群を `String` モジュールに集約する。
失敗しうる変換や境界検査は D-10 に従って `Option` を返し、プログラム誤りの添字アクセスだけをトラップに残す。
WASM は import なしなので、`memcmp`/`memchr`/`strlen` などの外部 libc symbol には依存しない。
連続メモリを保つ実装にし、LLVM の vectorize と将来の SIMD/GPU backend が使いやすい IR 形状にする。

## 現状

`docs/language.md` の UTF-8 文字列仕様では、`string.length` は byte 数、`text[index]` は `ubyte`、範囲外はトラップである。
文字列は所有する不変 UTF-8 buffer で、`+` は両辺を消費して連結し、`==`/`!=` は所有権を消費しない。
コピーは `clone_string(&text)` で行う。

`src/check.rs` の `Type::String` は `is_copy` が false、`needs_drop` が true、`exportable` は false。
`Type::String` は `layout_size`/`validate_size` で 16 bytes の descriptor として扱われる。
`TypedExprKind::StringLength` と `TypedExprKind::Index` は string 専用 lowering を持つ。
`Checker::value_expression` の `ExprKind::Field` は `field.text == "length"` の場合だけ string length を許す。
`ExprKind::Index` は string なら要素型を `Type::Integer(8, false)` にする。

`src/llvm.rs` の `llvm_type(Type::String)` は `%tz.string = { ptr, i64 }`。
`FunctionEmitter::binary` は string 連結を `@tz.string.concat`、等価比較を `@tz.string.equal` へ下げる。
`FunctionEmitter::expression_mode` の string literal は `@tz.string.new` で所有 buffer を作るが、`llvm_frame.rs` の frame lowering では literal の静的領域を使う経路がある。
`drop_value(Type::String)` は descriptor の pointer を `@tz.free` へ渡す。
`clone_value(Type::String)` は `@tz.string.new(ptr, length)`。

`src/runtime/string.ll` は次だけを持つ。
`@tz.string.copy(ptr %to, ptr %from, i64 %length)`。
`@tz.string.allocate(i64 %length)`。
`@tz.string.new(ptr %source, i64 %length)`。
`@tz.string.concat(%tz.string %left, %tz.string %right)`。
`@tz.string.equal(%tz.string %left, %tz.string %right)`。
これらは libc を使わない byte loop である。

`src/runtime/wasm.ll` は i128 補助だけで、文字列用の追加 helper はない。
`emit_target` は IR に `@tz.string.`、`@tz.free`、`@tz.alloc` が現れると `string.ll` と heap runtime を連結する。
新しい runtime symbol はこの検出に乗るよう `@tz.string.` prefix を使う。

`src/polymorph.rs` の `Classes::intrinsic` は `Add` と `Eq` に string を含める。
`Ord` は string に対して組み込み instance を持たない。
D02 の `Ord<string>` は A11 の「比較は非消費（Eq/Ord は借用を受け取る）」を hard dependency として実装する。
A11 完了後、`Classes::intrinsic` と `FunctionEmitter::binary` の比較 lowering を拡張する。

## 仕様

### 前提とする他チケットのインターフェース

E02 は `std/String.tz` を同梱し、ユーザーは `String.find` のように修飾名で呼ぶ。
E02 は `String` モジュール名を予約し、ユーザーの `String.tz` と衝突したら `E1011`。
E02 は複数引数・制約付き・修飾名 builtin を提供する。
E02 の builtin-backed API は `Builtin` entry としてだけ存在し、std source に同名 `def` を置かない。
std source には `String.contains` のような Tsuzuri で実装する helper だけを書く。
A08 は `char` 型を Unicode scalar value として提供し、LLVM では `i32`、Copy/Eq/Ord/Display である。
A11 は `Eq`/`Ord` method が `&'a -> &'a -> bool` になり、比較演算子が被演算子を消費しないことを提供する。
C03 は slice 型 `&[T]` を提供する。
`&[string]` は配列全体をコピーせずに `ptr + length` で借用できる。
B01 は `Option<'a>` を提供する。
C02 の `Vec<'a>` は弱依存であり、実装済みなら split/join/building の内部に使ってよい。
C02 が未完了でも D02 は二 pass allocation で実装する。

### 他チケットへの提供インターフェース

D01 の string interpolation フェーズ 2 は `String.concat`/`String.Builder` 相当を使ってよい。
E07 は trace の文字列組み立てに `String.concat`/`String.join` を使える。
G06 は failure message の比較差分に `String.find`, `String.slice`, `String.replace` を使える。
C06 の Map/Set は A11+D02 後の `Ord<string>` を前提にしてよい。
A07 の deriving Display は `String.concat` を使って field 表示を組み立ててもよい。

### 基本方針

すべての index は byte offset。
返す index も byte offset。
UTF-8 の妥当性は `string` の不変条件として常に保つ。
`String.from_bytes` だけが byte array から string を作る入口で、失敗時は `None`。
byte 境界ではなく char boundary が必要な関数は、境界不正を `Option.None` にする。
既存 `text[index]` と `String.decode_at` はプログラム誤りとして範囲外・境界不正をトラップする。
検索失敗は `Option.None`。
空 needle の検索は成功として扱う。
`String.compare` は Unicode scalar の code point order と一致する UTF-8 byte lexicographic order を使う。
`Ord<string>` は A11 後にこの比較を借用ベースの組み込み instance として使う。
ASCII 変換関数は ASCII 以外の byte を変更しない。
すべての関数引数は左から右に一度だけ評価する。

### API 一覧

`String.concat : &[string] -> string`
全要素を順に連結する。
入力 slice と要素 string は借用であり消費しない。
戻り値は新しい所有 string。
空 slice は `""`。
allocation は結果 buffer 1 回。
長さ合計 overflow はトラップ。
複雑度 O(total bytes)。

`String.join : &string -> &[string] -> string`
separator を各要素の間に挿入して連結する。
separator と slice は借用。
空 slice は `""`。
要素 1 個なら要素を clone する。
allocation は結果 buffer 1 回。
複雑度 O(total bytes + separator.length * max(n - 1, 0))。

`String.split : &string -> &string -> [string]`
`String.split separator text` の順にする。
separator が空なら UTF-8 scalar ごとの string 配列を返す。
separator が見つかるたびに前区間を新しい string としてコピーする。
結果配列と各 substring は所有値。
text/separator は消費しない。
C02 がない場合、1 pass 目で個数を数え、2 pass 目で配列を初期化する。
separator が空の場合は `String.chars` 相当の decode pass で個数を数える。
不正 UTF-8 は string 不変条件違反なので想定しない。
allocation は結果配列 1 回 + substring 個数分。

`String.find : &string -> &string -> Option<i64>`
`String.find needle haystack`。
最初の byte offset を返す。
needle が空なら `Some 0`。
見つからなければ `None`。
byte pattern 検索であり、char boundary は要求しない。
ただし haystack と needle は妥当 UTF-8 なので、見つかった offset は必ず char boundary になる。

`String.rfind : &string -> &string -> Option<i64>`
最後の byte offset を返す。
needle が空なら `Some haystack.length`。

`String.contains : &string -> &string -> bool`
`find` の `Option` を bool にする。

`String.starts_with : &string -> &string -> bool`
prefix と text を byte prefix で比較する。

`String.ends_with : &string -> &string -> bool`
suffix と text を byte suffix で比較する。

`String.slice : &string -> i64 -> i64 -> Option<string>`
`String.slice text start finish`。
`start` は inclusive、`finish` は exclusive の byte offset。
`0 <= start <= finish <= text.length` かつ start/finish が UTF-8 char boundary の場合だけ `Some substring`。
失敗は `None`。
戻り substring は所有 string。
allocation は substring length が 0 なら 0 回または空 descriptor、非空なら 1 回。
複雑度 O(length + boundary check cost)。

`String.sub : &string -> i64 -> i64 -> Option<string>`
`String.sub text start length`。
length は byte 長。
`length < 0` は `None`。
`start + length` overflow は `None`。
それ以外は `slice text start (start + length)` と同じ。

`String.decode_at : &string -> i64 -> (char, i64)`
byte offset の UTF-8 scalar を decode し、`(char, next_byte_offset)` を返す。
offset が範囲外、continuation byte、過長 encoding、不正 scalar の場合はトラップ。
妥当な `string` では offset が char boundary かつ `< length` なら成功する。
複雑度 O(1)。

`String.char_count : &string -> i64`
UTF-8 scalar 数を返す。
複雑度 O(bytes)。

`String.chars : &string -> [char]`
全 scalar を decode した所有配列を返す。
allocation は配列 1 回。
複雑度 O(bytes)。

`String.trim : &string -> string`
`String.trim_start : &string -> string`
`String.trim_end : &string -> string`
ASCII whitespace だけを対象にする。
対象 byte は `0x09`, `0x0A`, `0x0B`, `0x0C`, `0x0D`, `0x20`。
Unicode whitespace は対象外。
戻り値は所有 string。
入力が変わらなくても clone する。
allocation は非空結果 1 回。

`String.to_ascii_lower : &string -> string`
`String.to_ascii_upper : &string -> string`
ASCII A-Z/a-z だけ変換する。
UTF-8 の非 ASCII byte はそのままコピーし、妥当性は壊れない。
allocation は text.length が 0 なら 0 回または空 descriptor、非空なら 1 回。

`String.replace : &string -> &string -> &string -> string`
`String.replace needle replacement text`。
needle が空なら、先頭・各 scalar 間・末尾に replacement を挿入する。
それ以外は重ならない左から右の occurrence を置換する。
検索は byte pattern。
戻り値は所有 string。
C02 がある場合は builder、ない場合は 1 pass 目で count と結果長を求め、2 pass 目で copy。

`String.repeat : &string -> i64 -> string`
`count < 0` はトラップ。
`text.length * count` overflow はトラップ。
allocation は結果 1 回。
`count == 0` または空 text は `""`。
複雑度 O(text.length * count)。

`String.compare : &string -> &string -> i64`
byte lexicographic compare。
戻り値は `<0`, `0`, `>0`。
具体値は -1/0/1 に固定する。
`Ord<string>` の `< <= > >=` はこの比較を使う。

`String.to_bytes : string -> [ubyte]`
string を消費し、同じ buffer を `[ubyte]` の所有配列へ O(1) で移す。
コピーしない。
返った配列の drop が buffer を解放する。
元の string は move 済み。
空 string は空配列。

`String.from_bytes : [ubyte] -> Option<string>`
byte array を消費する。
UTF-8 として妥当なら O(1) で同じ buffer を string へ移し、`Some string`。
不正なら `None` を返し、入力配列 buffer は解放する。
不正 byte の位置は返さない。
空配列は `Some ""`。

### Ord instance

`Ord<string>` を A11 後の組み込み instance に追加する。
`Ord.lt`/`le`/`gt`/`ge` は A11 の借用 signature `&string -> &string -> bool` で、`<`, `<=`, `>`, `>=` は被演算子を消費しない。
結果は `String.compare` と同じ byte lexicographic order。
UTF-8 の仕様により、これは code point order と一致する。
locale・大文字小文字・正規化は考慮しない。
`Eq<string>` は既存 `@tz.string.equal` と同じ。

### トラップと Option

検索失敗は `Option.None`。
parse/変換失敗は `Option.None`。
char boundary を要求する `slice`/`sub` の失敗は `Option.None`。
`decode_at` の invalid offset は既存 indexing と同じプログラム誤りなのでトラップ。
`repeat` の負 count と overflow はプログラム誤りなのでトラップ。
メモリ確保失敗は既存 heap runtime と同じくトラップ。

## 設計

### std 実装と builtin の切り分け

小さく型だけで書ける helper は `std/String.tz` に置く。
ただし builtin-backed API 関数（下の `Builtin` variants と同じ修飾名）は std source に同名 `def` を置かない。
同名 `def` は E02 の解決順で builtin を shadow するため禁止する。
高速 byte loop、O(1) buffer transfer、UTF-8 validation、Ord lowering は builtin/runtime に置く。
推奨する builtin variants:

```rust
pub enum Builtin {
    // existing...
    StringConcatMany,
    StringJoin,
    StringSplit,
    StringFind,
    StringRFind,
    StringStartsWith,
    StringEndsWith,
    StringSlice,
    StringDecodeAt,
    StringCharCount,
    StringChars,
    StringTrim,
    StringTrimStart,
    StringTrimEnd,
    StringAsciiLower,
    StringAsciiUpper,
    StringReplace,
    StringRepeat,
    StringCompare,
    StringToBytes,
    StringFromBytes,
}
```

E02 の修飾 builtin により、名前は `String.concat`, `String.join` などで登録する。
`String.contains` は builtin にせず、std で `Option.is_some (String.find needle text)` としてよい。
その場合 `Builtin` に `StringContains` を追加しない。
ただし `Option` の import は E02/B01 の std 常時読み込みに依存する。

`Classes::intrinsic` は A11 完了後に `Ord` へ `Type::String` を追加する。
`FunctionEmitter::binary` は A11 の比較 lowering に合わせ、`left.ty == Type::String` で比較 operator を扱う。
`Equal`/`NotEqual` は既存 `@tz.string.equal`。
`Less`/`LessEqual`/`Greater`/`GreaterEqual` は `@tz.string.compare` の戻り `i64` と 0 の比較。

### runtime/string.ll 追加関数

runtime symbol はすべて `define internal` で `@tz.string.` prefix。
WASM import を増やさない。
外部 libc symbol の `@memcmp`, `@memchr`, `@strlen`, `@memcpy`, `@memmove` は使わない。
一方、LLVM intrinsic の `@llvm.memcpy.*` と `@llvm.memmove.*` は使用してよい。
これらは wasm32 bulk-memory で import なしに lower できることを object/link test で確認する。
候補:

```llvm
define internal i64 @tz.string.compare(%tz.string %a, %tz.string %b) nounwind
define internal i64 @tz.string.find(%tz.string %needle, %tz.string %haystack, i1 %reverse) nounwind
define internal i1 @tz.string.starts_with(%tz.string %prefix, %tz.string %text) nounwind
define internal i1 @tz.string.ends_with(%tz.string %suffix, %tz.string %text) nounwind
define internal i1 @tz.string.is_boundary(%tz.string %text, i64 %index) nounwind
define internal { i32, i64 } @tz.string.decode_at(%tz.string %text, i64 %index) nounwind
define internal i64 @tz.string.char_count(%tz.string %text) nounwind
define internal i1 @tz.string.valid_utf8(ptr %data, i64 %length) nounwind
define internal %tz.string @tz.string.slice_copy(%tz.string %text, i64 %start, i64 %finish) nounwind
```

上の一覧は本文なしの署名説明であり、実ファイルでは通常の LLVM body を持つ `define internal` にする。
`valid_utf8` は array からの transfer 前に使う。
`decode_at` は invalid の場合 `llvm.trap`。
`is_boundary` は `index == 0 || index == length || (byte & 0xC0) != 0x80` を基本にし、範囲外は false。
妥当 UTF-8 の string では continuation 判定だけで境界が分かる。

`copy` loop は既存 `@tz.string.copy` を使うか、`@llvm.memcpy.*` intrinsic を使う。
どちらの場合も IR に外部 `@memcpy` libcall を要求してはならない。

### UTF-8 decode

decode は次の規則を実装する。
1 byte: `0xxxxxxx`。
2 byte: `110xxxxx 10xxxxxx` かつ code point >= U+80。
3 byte: `1110xxxx 10xxxxxx 10xxxxxx` かつ code point >= U+800、surrogate でない。
4 byte: `11110xxx 10xxxxxx 10xxxxxx 10xxxxxx` かつ U+10000..U+10FFFF。
それ以外は invalid。
`String.from_bytes` は invalid なら `None`。
`String.decode_at` は invalid なら trap。
既存 string は lexer と runtime construction が妥当性を保証するため、通常 invalid にならない。

### Buffer transfer

`String.to_bytes` は `%tz.string` と `%tz.array` がどちらも `{ ptr, i64 }` であることを利用する。
IR は descriptor を bitcast せず、`extractvalue`/`insertvalue` で `%tz.array` を作る。
入力 string の pointer を drop しないよう、builtin が所有値を消費してそのまま返す。
呼び出し側の通常 drop が入力 string を二重解放しないよう、`FunctionEmitter::call` の引数 move 規則に従う。

`String.from_bytes` は array descriptor を受け取る。
valid なら `%tz.string` descriptor を作って `Some`。
invalid なら `@tz.free(ptr data)` して `None`。
`[ubyte]` の element は drop 不要なので buffer free だけでよい。
将来 `[byte]` は負値があり UTF-8 byte として不適切なので対象外。

### Allocation 設計

連結・join・replace・repeat は結果長を先に計算し、1 回だけ allocate する。
長さ計算は `icmp ule` による overflow 検査を行い、overflow は trap。
`String.split` は C02 がなければ two-pass。
C02 があれば `Vec.push` で substring を追加し、最後に `Vec.to_array`。
ただし C02 は弱依存なので、D02 の受け入れ条件は two-pass 経路で満たす。

各 substring は新しい owning string。
入力 text の byte buffer を共有しない。
これは現在の所有権モデルに substring view/lifetime がないため。
C03 slice は byte/string slice ではなく `[T]` slice なので、string substring view は対象外。

### 評価順序

curried 関数なので `String.replace needle replacement text` は needle、replacement、text の順に評価される。
builtin lowering でもこの順序を保つ。
内部の検索 loop は left-to-right。
`replace` は重ならない occurrence を左から右に処理する。
needle が空の特殊ケースでも、replacement/text の評価順序は同じ。

## 実装手順

1. 依存確認。
   確認: E02, A08, A11, C03, B01 が `done`、C02 は未完了でもよい。
2. `std/String.tz` を追加し、source 実装 helper だけを置く。
   確認: `String.contains` を std で実装する場合はユーザー source なしに解決し、builtin-backed API と同名の `def` が存在しないことを E02 テストで確認する。
3. `Builtin` に `String.*` variants を追加する。
   確認: `Builtin::ALL` と duplicate name 検査で `fn concat` ではなく `String.concat` だけが予約される。
4. `Builtin::signature`/E02 signature へ各関数の型を正確に登録する。
   確認: `tests/strings.rs` で各関数の型注釈代入が通る。
5. `runtime/string.ll` に compare/find/prefix/suffix/boundary/decode/validate/copy helpers を追加する。
   確認: `llvm::emit_target(..., wasm=true)` に外部 `@memcmp`/`@memchr`/`@strlen`/`@memcpy`/`@memmove` がない。`@llvm.memcpy.*`/`@llvm.memmove.*` は許可し、wasm object/link と imports 空で検証する。
6. `FunctionEmitter::emit_builtin` 相当に string builtins を実装する。
   確認: `let needle = "ll"; let text = "hello"; String.find (&needle) (&text)` の IR が `@tz.string.find` を呼ぶ。
7. `String.slice`/`sub` を Option 結果へ下げる。
   確認: boundary invalid が `None`、既存 `text[index]` invalid は trap のまま。
8. `String.chars` と `String.char_count` を実装する。
   確認: emoji、4 byte scalar、ASCII 混在で char 数と code point を A08 `Char.to_u32` で照合する。
9. `String.to_bytes`/`from_bytes` の O(1) transfer を実装する。
   確認: native heap tracking で valid round-trip に追加 copy allocation がないことを検査する。
10. A11 後に `Ord<string>` を `Classes::intrinsic` と `FunctionEmitter::binary` に追加する。
    確認: `"a" < "b"`、`"ä" > "z"` 等を code point order で照合する。
11. split/join/replace/repeat の allocation/overflow/trap を実装する。
    確認: large length overflow は native child process trap と WASM RuntimeError。
12. docs と README を更新する。
    確認: string length が byte のままで、char APIs は明確に別名である。

## テスト計画

### Rust tests

`tests/strings.rs` を追加する。
受理:
`let needle = "ll"; let text = "hello"; String.find (&needle) (&text) : Option<i64>`。
`let needle = "l"; let text = "hello"; String.rfind (&needle) (&text)`。
`let needle = "he"; let text = "hello"; String.contains (&needle) (&text)`。
`let prefix = "he"; let text = "hello"; String.starts_with (&prefix) (&text)`。
`let suffix = "lo"; let text = "hello"; String.ends_with (&suffix) (&text)`。
`let text = "hé"; String.slice (&text) 0 1` は `Some "h"`。
`let text = "hé"; String.slice (&text) 1 2` は `None`。
`let text = "😀"; String.decode_at (&text) 0` は char と next offset 4。
`String.to_bytes "abc"` は `[ubyte]`。
`String.from_bytes (new [ubyte](...))` は `Option<string>`。
`"a" < "b"` が受理される。

拒否:
`String.from_bytes [1i8]` は `E1003`。
`let text = "x"; String.to_bytes (&text)` は `E1003`。
`String.slice "x" 0 1` は `E1003`（第一引数は `&string`）。
`let text = "x"; String.chars (&text)[0] + 1` は char に算術がないため A08 の診断。
`instance Ord<string> { ... }` は組み込み instance 上書きで `E1016`。

IR:
WASM target IR に外部 `declare.*@memcmp`, `declare.*@memchr`, `declare.*@strlen`, `declare.*@memcpy`, `declare.*@memmove` がない。
`declare.*@llvm.memcpy.*` と `declare.*@llvm.memmove.*` は許可し、wasm32 object/link と imports 空で確認する。
`String.to_bytes` IR に byte copy loop がない。
`String.from_bytes` invalid branch が `@tz.free` を呼ぶ。
`Ord<string>` は `@tz.string.compare` を呼ぶ。

### Node E2E

`tests/string_library.mjs` を追加する。
native `-O0`/`-O3` は malloc/free tracking で `live == 0`。
WASM `-O0`/`-O3` は imports 空。
IR determinism を検査。

accepted programs:
ASCII concat/join/split/find/rfind/replace/repeat。
UTF-8 text `"日本語😀"` の length が byte 数、char_count が scalar 数。
slice valid boundaries: `0`, ASCII boundary, multi-byte boundary, length。
slice invalid continuation offsets returns None。
trim ASCII whitespace only。
ASCII case mapping leaves `"Ä"` unchanged。
to_bytes/from_bytes round-trip for NUL を含む string。
from_bytes invalid sequences: lone continuation, overlong slash, surrogate, >U+10FFFF returns None and leaks 0。
compare cases: `""`, `"a"`, `"aa"`, `"b"`, `"ä"`, `"😀"`。
replace empty needle: replacement inserted at scalar boundaries。
split empty separator: one string per scalar。
`string`, `[string]`, `[char]`, `Option<string>` は公開 ABI で直接 export できないため、E2E fixture は Tsuzuri 内部で bool/i64 checksum、byte length、code point の合計、または 64-bit 以下の分割値を返す exported helper で観測する。
公開 ABI を広げない。

trap cases:
`let text = ""; String.decode_at (&text) 0`。
`let text = "é"; String.decode_at (&text) 1`。
`let text = "x"; String.repeat (&text) (-1)`。
`let text = "x"; String.repeat (&text) INT64_MAX`。
allocation limit in WASM for huge repeat。

reference implementations:
JS `TextEncoder`/`TextDecoder` with fatal mode for UTF-8 validation。
JS BigInt for byte lengths and overflow expectations。
Manual byte arrays for invalid UTF-8 because JS string cannot hold invalid UTF-8 bytes.

### 実装完了時の検証コマンド

GUIDE §3 に従い、小さい範囲から次を実行する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test strings
cargo test --locked
cargo build --release --locked
node tests/string_library.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
```

`tests/string_library.mjs` は native/WASM × `-O0`/`-O3`、WASM imports 空、外部 libc string/memory symbol 不在、`@llvm.memcpy.*`/`@llvm.memmove.*` 使用時の wasm32 object/link 成功を必ず含める。

## ドキュメント

`docs/language.md` の UTF-8 文字列節に `String` モジュール関数表を追加する。
「index は byte offset」「char は A08 の Unicode scalar」「slice は Option」を明記する。
`Ord<string>` を型クラス表へ追加し、A11 により比較が非消費であることを書く。
`README.md` に簡単な `String.split` と `String.chars` の例を追加する。
`docs/architecture.md` の runtime 表に `string.ll` の追加 helper と libc 非依存を書く。
`docs/benchmarks.md` は性能主張をしない限り更新不要。
もし search の SIMD/vectorize を主張するなら、生成 IR/assembly と測定条件を記録し、CI 速度閾値は置かない。

## 受け入れ条件

- [ ] 全 API が仕様の signature で `String.` 修飾名から使える。
- [ ] string の UTF-8 不変条件を破る関数がない。
- [ ] char boundary が必要な slice/sub は `Option` で失敗する。
- [ ] decode_at と既存 indexing の trap 方針が一致する。
- [ ] to_bytes/from_bytes は valid round-trip で O(1) transfer する。
- [ ] invalid from_bytes は入力 buffer を解放して `None` を返す。
- [ ] A11 後の Ord<string> は byte lexicographic/code point order で、比較が string を消費しない。
- [ ] native/WASM `-O0`/`-O3` で同じ結果。
- [ ] WASM imports は空。
- [ ] IR に外部 libc string/memory call がない。`@llvm.memcpy.*`/`@llvm.memmove.*` intrinsic は wasm32 link/imports 空で検証済み。
- [ ] docs が byte index と Unicode scalar の違いを明記している。

## 落とし穴

byte index を char index と誤記しない。
UTF-8 continuation byte への slice を trap にせず `None` にする。
`String.decode_at` は `Option` ではなく trap であり、slice と混同しない。
`String.to_bytes` 後に元 string を drop すると二重解放になる。
`String.from_bytes` invalid path で array buffer を leak しない。
empty needle の find/rfind/replace/split は仕様を固定してテストする。
ASCII case mapping で UTF-8 multi-byte の内部 byte を変換しない。
外部 `@memcmp` を IR に出すと WASM で import/libcall 問題になる。
substring view を作ると lifetime/ownership が未設計なので、このチケットでは必ず owning copy にする。

## 対象外

Unicode normalization。
locale aware case mapping。
grapheme cluster。
正規表現。
string builder の公開型。
substring view。
UTF-16/UTF-32 変換。
format string。
mutable string。
I/O。

## 未決事項

`String.split` の引数順は `separator -> text` を既定案にする。
理由は `String.replace needle replacement text` と同じく「操作パラメータを先、対象を最後」に揃え、pipeline で `text |> String.split sep` と書けるため。
trim の whitespace 範囲は ASCII のみを既定案にする。
Unicode whitespace は後続チケットで `Char.is_whitespace` と一緒に検討する。
`String.compare` の戻り型は i64、値は -1/0/1 を既定案にする。
`String.from_bytes` invalid 時に入力 array を返す API は対象外。

台帳の見直し提案: なし。
