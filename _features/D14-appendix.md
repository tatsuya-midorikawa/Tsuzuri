# D14 付録: `Text` クラスの表・実測・検証済みの骨格

本文は [D14-text-class.md](D14-text-class.md)。この付録は、本文が引く長い表（A）、実測（B）、試作で型検査と実行を通した std・Rust・テスト・文書の全文（C〜H）を置く。
すべて HEAD `a2313d19`（作業ツリーは `main` の `9dd0fbbc`。`src/`・`std/`・`tests/` は `a2313d19` と同一）で確かめた。コマンドと出力は実際に実行したもの。

| 節 | 内容 |
| --- | --- |
| A | 署名の突き合わせ表（`String`／`Utf8String`／`Char`／`Utf8Char`／`Regex`／`Json`、参考に `Unicode`） |
| B | 実測（E1013、特殊化の予算、IR とオブジェクトコード、既存プログラムへの影響、E2E の結果） |
| C | std の骨格（`std/Text.tt`、instance、`std/RegexText.tt`、`Regex.tz` の追記）。型検査済みの全文 |
| D | Rust などの変更（`src/constants.rs`、`src/stdlib.rs`、既存テストの期待値、`vsc/src/core.ts`、予算のテスト、CI の step） |
| E・F | テスト（fixture、`tests/features.mjs`、`tests/text_class.mjs`、`tests/text_class.rs`、`regex_text`）は [D14-appendix-tests.md](D14-appendix-tests.md) |
| G・H | 文書の草稿（`text.md` の全文、`regex.md` の差分、他のページ、`docs/benchmarks.md`）と IR を比べる手順は [D14-appendix-docs.md](D14-appendix-docs.md) |

## A. 署名の突き合わせ表

### A.0 件数（コマンドと出力）

```sh
for m in String Utf8String Char Utf8Char Regex Json Unicode; do
  printf "%-10s public def: %3s  private def: %3s  def rec: %3s  _utf8 twins: %3s\n" $m \
    "$(grep -cE '^(export )?def (rec )?[a-z_0-9]+ ::' std/$m.tz)" "$(grep -cE '^private def ' std/$m.tz)" \
    "$(grep -cE '^(export )?def rec ' std/$m.tz)" "$(grep -E '^def [a-z_0-9]+_utf8 ::' std/$m.tz | wc -l | tr -d ' ')"
done
```

```text
String     public def:  21  private def:   4  def rec:   0  _utf8 twins:   0
Utf8String public def:  20  private def:   5  def rec:   0  _utf8 twins:   0
Char       public def:   6  private def:   0  def rec:   0  _utf8 twins:   0
Utf8Char   public def:   6  private def:   0  def rec:   0  _utf8 twins:   0
Regex      public def:  18  private def:  63  def rec:   0  _utf8 twins:   7
Json       public def:  38  private def:  69  def rec:   9  _utf8 twins:   0
Unicode    public def:  24  private def:  29  def rec:   0  _utf8 twins:  10
```

```sh
# String と Utf8String の共通の名前（20）、片方だけ（String に decode_at。Utf8String の decode_at は組み込み）
comm -12 <(grep -oE "^def [a-z_0-9]+" std/String.tz | sort) <(grep -oE "^def [a-z_0-9]+" std/Utf8String.tz | sort) | wc -l   # 20
comm -23 <(grep -oE "^def [a-z_0-9]+" std/String.tz | sort) <(grep -oE "^def [a-z_0-9]+" std/Utf8String.tz | sort)           # def decode_at
comm -13 <(grep -oE "^def [a-z_0-9]+" std/String.tz | sort) <(grep -oE "^def [a-z_0-9]+" std/Utf8String.tz | sort)           # （空）
comm -12 <(grep -oE "^def [a-z_0-9]+" std/Char.tz | sort) <(grep -oE "^def [a-z_0-9]+" std/Utf8Char.tz | sort) | wc -l       # 6
# 名前空間 String／Utf8String／Char／Utf8Char の組み込み関数（clone_string を含む）
grep -cE 'Self::[A-Za-z0-9]+ => "(String|Utf8String|Char|Utf8Char)\.[a-z_0-9]+"|Self::CloneString => "clone_string"' src/check.rs   # 18
```

（`private def` は `std/` の内部の補助で、公開しない。`Json` の `def rec` 9 件は `encode`・`decode`・`serialize`・`serialize_pretty`・`deserialize`・`encode_field`・`encode_case`・`decode_field`・`decode_payload`。）

### A.1 `String` と `Utf8String` の `def`（21 と 20）

`S` は `string`（`String`）または `utf8string`（`Utf8String`）。分類: 「同じ」= 引数と結果の意味が同じ、「単位」= 位置・長さの単位だけが違う（コード単位とバイト）、「違う」= 結果の意味や上限が違う、「入れない」= `Text` のメソッドにしない。

| # | 名前 | 署名（`S` を置き換える） | 分類 | `Text` | HEAD の違い（`😀` = U+1F600 で測定） |
| --- | --- | --- | --- | --- | --- |
| 1 | `length` | `ref S -> i64` | 単位 | メソッド | 2 と 4（コード単位とバイト） |
| 2 | `find` | `ref S -> ref S -> Maybe<i64>` | 単位 | メソッド | 位置の単位。パターンが第 1 引数 |
| 3 | `rfind` | `ref S -> ref S -> Maybe<i64>` | 単位 | メソッド | 同上 |
| 4 | `contains` | `ref S -> ref S -> bool` | 同じ | メソッド | なし。空のパターンは真 |
| 5 | `starts_with` | `ref S -> ref S -> bool` | 同じ | メソッド | なし |
| 6 | `ends_with` | `ref S -> ref S -> bool` | 同じ | メソッド | なし |
| 7 | `slice` | `ref S -> i64 -> i64 -> Maybe<S>` | 違う | メソッド | `string` はサロゲートの途中で切れる（`slice 1 2` は長さ 1 の `Some`）。`utf8string` はスカラーの境界でなければ、空の範囲でも `None`（`slice 1 2` は `None`） |
| 8 | `sub` | `ref S -> i64 -> i64 -> Maybe<S>` | 違う | メソッド | 同上（`sub text first count`） |
| 9 | `decode_at` | `String`: `ref string -> i64 -> (char * i64)`（`def`）。`Utf8String`: `ref utf8string -> i64 -> (utf8char * i64)`（組み込み） | 違う | 入れない | 戻りの型が違う。`char` は 1 コード単位でサロゲートを結合せず、次の位置は +1。`utf8char` はスカラーで、次の位置は +1〜+4 |
| 10 | `char_count` | `ref S -> i64` | 違う | 入れない | `String` は `length` と同じ値（O(1)。`😀` で 2）、`Utf8String` はスカラーの数（O(n)。`😀` で 1）。同じ名前で意味が違う |
| 11 | `chars` | `ref string -> [char]`／`ref utf8string -> [utf8char]` | 違う | 入れない | 要素の型が違う |
| 12 | `join` | `ref S -> ref [S] -> S` | 違う | メソッド | 結果の上限: `string` は 2^53 − 1 コード単位、`utf8string` は `i64` の範囲（超えるとトラップ） |
| 13 | `concat` | `ref [S] -> S` | 違う | メソッド | 同上 |
| 14 | `split` | `ref S -> ref S -> [S]` | 違う | メソッド | 空の区切り: `string` はコード単位ごと（`😀` で 2 個）、`utf8string` はスカラーごと（1 個） |
| 15 | `trim_start` | `ref S -> S` | 同じ | メソッド | なし。ASCII の空白（9〜13 と 32）だけ |
| 16 | `trim_end` | `ref S -> S` | 同じ | メソッド | なし |
| 17 | `trim` | `ref S -> S` | 同じ | メソッド | なし |
| 18 | `to_ascii_lower` | `ref S -> S` | 同じ | メソッド | なし。`A`〜`Z` だけ |
| 19 | `to_ascii_upper` | `ref S -> S` | 同じ | メソッド | なし |
| 20 | `repeat` | `ref S -> i64 -> S` | 違う | メソッド | 負の回数はトラップ。結果の上限は `join` と同じ |
| 21 | `replace` | `ref S -> ref S -> ref S -> S` | 違う | メソッド | 空のパターン: `string` はコード単位のあいだ（`😀` で 5 単位）、`utf8string` はスカラーの前後（6 バイト） |

`Text` に入る `def` は 18 個（#1〜8 と #12〜21）、入れない `def` は 3 個（#9〜11 の `decode_at`・`char_count`・`chars`）。

### A.2 組み込み関数 18 個

`Builtin::name`（`src/check.rs`）が返す名前のうち、名前空間が `String`・`Utf8String`・`Char`・`Utf8Char` のものと `clone_string`。署名は同じファイルの `Builtin::scheme` から読んだ。`i16u`・`i8u`・`i32u` は符号なしの 16・8・32 ビット整数。

| # | 組み込み | 署名 | 相手 | `Text` |
| --- | --- | --- | --- | --- |
| 1 | `String.compare` | `ref string -> ref string -> i64` | `Utf8String.compare` | メソッド `compare`。順序が違う: UTF-16 のコード単位の順（`"\u{FF5E}"` と `"😀"` で 1） |
| 2 | `Utf8String.compare` | `ref utf8string -> ref utf8string -> i64` | `String.compare` | メソッド `compare`。バイトの順（同じ組で −1） |
| 3 | `clone_string` | `ref string -> string` | `Utf8String.clone` | メソッド `clone`（名前が `String.clone` でない） |
| 4 | `Utf8String.clone` | `ref utf8string -> utf8string` | `clone_string` | メソッド `clone` |
| 5 | `String.from_utf8` | `ref utf8string -> string` | `Utf8String.from_string` | 入れない（型の変換） |
| 6 | `Utf8String.from_string` | `ref string -> utf8string` | `String.from_utf8` | 入れない（孤立サロゲートはトラップ） |
| 7 | `String.is_well_formed` | `ref string -> bool` | なし | 入れない（`string` だけ） |
| 8 | `String.to_well_formed` | `ref string -> string` | なし | 入れない（`string` だけ） |
| 9 | `String.to_code_units` | `string -> [i16u]` | `Utf8String.to_bytes` | 入れない（要素の型が違う。所有を移す） |
| 10 | `String.from_code_units` | `[i16u] -> string` | `Utf8String.from_bytes` | 入れない |
| 11 | `Utf8String.to_bytes` | `utf8string -> [i8u]` | `String.to_code_units` | 入れない |
| 12 | `Utf8String.from_bytes` | `[i8u] -> Maybe<utf8string>` | `String.from_code_units` | 入れない（検査して `Maybe` を返す） |
| 13 | `Utf8String.decode_at` | `ref utf8string -> i64 -> (utf8char * i64)` | `String.decode_at` | 入れない（A.1 の #9） |
| 14 | `Char.of_u16` | `i16u -> char` | なし | 入れない（A.3） |
| 15 | `Char.to_u16` | `char -> i16u` | なし | 入れない |
| 16 | `Utf8Char.of_u32` | `i32u -> Maybe<utf8char>` | なし | 入れない |
| 17 | `Utf8Char.of_u32_unchecked` | `i32u -> utf8char` | なし | 入れない |
| 18 | `Utf8Char.to_u32` | `utf8char -> i32u` | なし | 入れない |

`Text` の 21 メソッドの出どころ: `def` 18 個（A.1）＋ 組み込み 2 個（`compare`・`clone`。#1〜4 の 4 関数が 2 メソッド）＋ 新しい 1 個（`is_empty`。既定の実装）。

### A.3 `Char` と `Utf8Char`（6 と 6、名前は 6 つとも共通）

| # | 名前 | `Char` | `Utf8Char` | 分類 | `Text` |
| --- | --- | --- | --- | --- | --- |
| 1 | `is_ascii_digit` | `char -> bool` | `utf8char -> bool` | 同じ | クラスなし |
| 2 | `is_ascii_lower` | `char -> bool` | `utf8char -> bool` | 同じ | クラスなし |
| 3 | `is_ascii_upper` | `char -> bool` | `utf8char -> bool` | 同じ | クラスなし |
| 4 | `is_ascii_alphabetic` | `char -> bool` | `utf8char -> bool` | 同じ | クラスなし |
| 5 | `to_ascii_upper` | `char -> char` | `utf8char -> utf8char` | 同じ | クラスなし |
| 6 | `to_ascii_lower` | `char -> char` | `utf8char -> utf8char` | 同じ | クラスなし |

理由（決定事項 D6）: `Text<'s>` から文字を取り出す手段（`chars`・`decode_at`）は入れていない（A.1）ので、`'s` から文字の型 `'c` を決められない。文字の型クラスは、呼び出し側が `char` か `utf8char` を既に持っているときだけ使え、`Char.is_ascii_digit c` と書けば足りる。

### A.4 `Regex`（公開 18）

| # | 名前 | 署名 | `_utf8` の相手 | Phase 2 |
| --- | --- | --- | --- | --- |
| 1 | `compile` | `ref string -> Result<Regex, Error>` | なし（パターンは常に `string`） | 対象外 |
| 2 | `escape` | `ref string -> string` | なし | 対象外 |
| 3 | `group_count` | `ref Regex -> i64` | なし（テキストを取らない） | 対象外 |
| 4 | `is_match` | `ref Regex -> ref string -> bool` | #12 | `is_match_text` |
| 5 | `find` | `ref Regex -> ref string -> Maybe<(i64 * i64)>` | #13 | `find_text` |
| 6 | `find_at` | `ref Regex -> ref string -> i64 -> Maybe<(i64 * i64)>` | #14 | `find_at_text` |
| 7 | `find_all` | `ref Regex -> ref string -> [(i64 * i64)]` | #15 | `find_all_text` |
| 8 | `captures` | `ref Regex -> ref string -> Maybe<[(i64 * i64)]>` | #16 | `captures_text` |
| 9 | `replace_all` | `ref Regex -> ref string -> ref string -> string` | #17 | `replace_all_text` |
| 10 | `split` | `ref Regex -> ref string -> [string]` | #18 | `split_text` |
| 11 | `search_steps` | `ref Regex -> ref string -> i64` | なし（線形時間の試験用） | 対象外 |
| 12 | `is_match_utf8` | `ref Regex -> ref utf8string -> bool` | #4 | 呼び出される側 |
| 13 | `find_utf8` | `ref Regex -> ref utf8string -> Maybe<(i64 * i64)>` | #5 | 〃 |
| 14 | `find_at_utf8` | `ref Regex -> ref utf8string -> i64 -> Maybe<(i64 * i64)>` | #6 | 〃 |
| 15 | `find_all_utf8` | `ref Regex -> ref utf8string -> [(i64 * i64)]` | #7 | 〃 |
| 16 | `captures_utf8` | `ref Regex -> ref utf8string -> Maybe<[(i64 * i64)]>` | #8 | 〃 |
| 17 | `replace_all_utf8` | `ref Regex -> ref utf8string -> ref utf8string -> utf8string` | #9 | 〃 |
| 18 | `split_utf8` | `ref Regex -> ref utf8string -> [utf8string]` | #10 | 〃 |

18 = 対象外 4（#1〜3、11）＋ 組 7（#4〜10 と #12〜18）。Phase 2 は組ごとに `*_text` を 1 つ足す（7 関数の追加。既存 18 関数は変えない）。範囲の単位は `string` がコード単位、`utf8string` がバイト。

### A.5 `Json`（公開 38）

テキストの型の欄: 「文書」= `utf8string`（JSON の文書。入出力）、「内容」= `string`（キー・文字列値・タグ名）、「混在」= 両方、「なし」= どちらも取らない。`_utf8` の相手は 0 件。

| # | 名前 | 署名 | テキスト |
| --- | --- | --- | --- |
| 1 | `parse` | `ref utf8string -> Result<Value, Error>` | 文書 |
| 2 | `numeral` | `ref utf8string -> Maybe<Numeral>` | 文書 |
| 3 | `to_i64` | `ref Numeral -> Result<i64, Error>` | なし |
| 4 | `to_f64` | `ref Numeral -> Result<f64, Error>` | なし |
| 5 | `to_utf8string` | `ref Value -> utf8string` | 文書 |
| 6 | `to_utf8string_pretty` | `ref Value -> i64 -> utf8string` | 文書 |
| 7 | `encode` | `Encode<'a> => ref 'a -> Result<Value, Error>` | なし |
| 8 | `decode` | `Decode<'a> => ref Value -> Result<'a, Error>` | なし |
| 9 | `serialize` | `Encode<'a> => ref 'a -> Result<utf8string, Error>` | 文書 |
| 10 | `serialize_pretty` | `Encode<'a> => ref 'a -> i64 -> Result<utf8string, Error>` | 文書 |
| 11 | `deserialize` | `Decode<'a> => ref utf8string -> Result<'a, Error>` | 文書 |
| 12 | `begin_object` | `i64 -> Result<Vec<(string * Value)>, Error>` | 内容 |
| 13 | `encode_field` | `Encode<'a> => Result<Vec<(string * Value)>, Error> -> string -> ref 'a -> Result<Vec<(string * Value)>, Error>` | 内容 |
| 14 | `end_object` | `Result<Vec<(string * Value)>, Error> -> Result<Value, Error>` | 内容 |
| 15 | `encode_case` | `Encode<'a> => string -> ref 'a -> Result<Value, Error>` | 内容 |
| 16 | `encode_tag` | `string -> Result<Value, Error>` | 内容 |
| 17 | `expect_object` | `ref Value -> Result<unit, Error>` | なし |
| 18 | `decode_field` | `Decode<'a> => ref Value -> string -> Result<'a, Error>` | 内容 |
| 19 | `keep_error` | `Maybe<Error> -> Result<'a, Error> -> Maybe<Error>` | なし |
| 20 | `case_index` | `ref Value -> ref [string] -> ref [bool] -> Result<i64, Error>` | 内容 |
| 21 | `decode_payload` | `Decode<'a> => ref Value -> Result<'a, Error>` | なし |
| 22 | `reader` | `ref utf8string -> Result<Reader, Error>` | 文書 |
| 23 | `next` | `ref utf8string -> Reader -> Result<(Event * Reader), Error>` | 文書 |
| 24 | `token_text` | `ref utf8string -> ref Lexeme -> Result<string, Error>` | 混在 |
| 25 | `token_matches` | `ref utf8string -> ref Lexeme -> ref string -> bool` | 混在 |
| 26 | `token_numeral` | `ref utf8string -> ref Lexeme -> Result<Numeral, Error>` | 文書 |
| 27 | `writer` | `unit -> Writer` | なし |
| 28 | `write_null` | `Writer -> Result<Writer, Error>` | なし |
| 29 | `write_bool` | `Writer -> bool -> Result<Writer, Error>` | なし |
| 30 | `write_number` | `Writer -> ref Numeral -> Result<Writer, Error>` | なし |
| 31 | `write_text` | `Writer -> ref string -> Result<Writer, Error>` | 内容 |
| 32 | `write_json` | `Writer -> ref Value -> Result<Writer, Error>` | なし |
| 33 | `write_key` | `Writer -> ref string -> Result<Writer, Error>` | 内容 |
| 34 | `open_array` | `Writer -> Result<Writer, Error>` | なし |
| 35 | `close_array` | `Writer -> Result<Writer, Error>` | なし |
| 36 | `open_object` | `Writer -> Result<Writer, Error>` | なし |
| 37 | `close_object` | `Writer -> Result<Writer, Error>` | なし |
| 38 | `finish` | `Writer -> Result<utf8string, Error>` | 文書 |

内訳: 文書 11（#1・2・5・6・9・10・11・22・23・26・38）＋ 内容 9（#12〜16・18・20・31・33）＋ 混在 2（#24・25）＋ なし 16 = 38。文書は `utf8string`、内容は `string` に固定されており、同じ操作の 2 つの版がない。したがって `Json` に `Text` は適用しない（決定事項 D5）。

### A.6 参考: `Unicode`（公開 24。このチケットの対象外）

組 10（20 関数）と単独 4: `normalize`／`normalize_utf8`、`is_normalized`／`is_normalized_utf8`、`grapheme_boundaries`／`grapheme_boundaries_utf8`、`word_boundaries`／`word_boundaries_utf8`、`graphemes`／`graphemes_utf8`、`words`／`words_utf8`、`to_lower`／`to_lower_utf8`、`to_upper`／`to_upper_utf8`、`to_title`／`to_title_utf8`、`case_fold`／`case_fold_utf8`。単独: `version`、`property_ranges`、`simple_case_folding`、`category`。将来、同じ「フック用の型クラス＋ `*_text` の薄い関数」の型（D4）で包める。

### A.7 数のまとめ

| モジュール | 公開 `def` | 組み込み | `Text` のメソッドへ | `Text` に入れない |
| --- | --- | --- | --- | --- |
| `String` | 21 | 6（＋ `clone_string`） | 18 `def` ＋ `compare`・`clone` | `decode_at`・`char_count`・`chars`、`from_utf8`・`is_well_formed`・`to_well_formed`・`to_code_units`・`from_code_units`（計 8） |
| `Utf8String` | 20 | 6 | 18 `def` ＋ `compare`・`clone` | `char_count`・`chars`、`decode_at`・`from_bytes`・`from_string`・`to_bytes`（計 6） |
| `Char` | 6 | 2 | なし | 8 |
| `Utf8Char` | 6 | 3 | なし | 9 |
| `Regex` | 18 | 0 | `RegexText` の 7 メソッド（`*_text` の 7 関数が使う） | `compile`・`escape`・`group_count`・`search_steps` |
| `Json` | 38 | 0 | なし | 38 |

### A.8 意味の違いの実測（HEAD。`😀` = U+1F600）

```sh
tsuzuri run target/d14-s2     # Main.tz は下
```

```tsuzuri
def facts :: unit -> string
fn facts _ =
    let wide = "\u{1F600}"
    let bytes = u8"\u{1F600}"
    let a = String.char_count (ref wide)
    let b = Utf8String.char_count (ref bytes)
    let c = String.length (ref wide)
    let d = Utf8String.length (ref bytes)
    let e = match String.slice (ref wide) 1 2 with | Some part -> part.length | None -> -1
    let f = match Utf8String.slice (ref bytes) 1 2 with | Some part -> part.length | None -> -1
    let g = (String.split (ref "") (ref wide)).length
    let h = (Utf8String.split (ref u8"") (ref bytes)).length
    let i = (String.replace (ref "") (ref "-") (ref wide)).length
    let j = (Utf8String.replace (ref u8"") (ref u8"-") (ref bytes)).length
    let wave = "\u{FF5E}"
    let wave8 = u8"\u{FF5E}"
    let k = String.compare (ref wave) (ref wide)
    let l = Utf8String.compare (ref wave8) (ref bytes)
    $"{a} {b} {c} {d} {e} {f} {g} {h} {i} {j} {k} {l}"

facts ()
```

出力（HEAD のバイナリ）: `2 1 2 4 1 -1 2 1 5 6 1 -1`。読み方: `char_count` 2／1、`length` 2／4、`slice 1 2` 1／−1（`None`）、`split ""` 2／1 個、`replace "" "-"` 5／6 単位、`compare` 1／−1。

## B. 実測

### B.1 E1013: クラスのメソッドへ一時値を借用で渡せない（HEAD の不具合。決定事項 D7 で直す）

ユーザーのクラスで再現できる（std の `Text` は HEAD にない）。`Pairing.tt` と `Main.tz`:

```tsuzuri
class Pairing<'s> {
    def one :: ref 's -> i64
    def two :: ref 's -> ref 's -> i64
}
```

```tsuzuri
instance Pairing<string> {
    fn one text = text.length
    fn two left right = left.length + right.length
}

let a = Pairing.one (ref "ab")
let b = Pairing.two (ref "a") (ref "b")
let c = Pairing.two (ref "a") (ref "b") + a
$"{a} {b} {c}"
```

```text
$ target/release/tsuzuri check target/d14-r1          # HEAD。target/d14-r1 に上の 2 つのファイルを置く
target/d14-r1/Main.tz:7:26: error[E1013]: borrow requires a local place; bind the temporary with 'let' first
  7 | let b = Pairing.two (ref "a") (ref "b")
    |                          ^
$ <試作のコンパイラ> run target/d14-r1   # 試作（D.1 の腕を足したもの）
2 2 4
```

1 引数の呼び出し（`Pairing.one (ref "ab")`）は通り、2 引数以上が落ちる。1 つの本体で報告される E1013 は最初の 1 件だけ。`Text` と同じ 21 メソッドのクラスをユーザーが書いて HEAD で測ると（`Text.m (ref "…") …` の形。1 行ずつ `Main.tz` の `let m = …` と最後の `0` にして `check`。付録 2 の E.5 の `LITERAL_LINES` と同じ）:

| 結果 | メソッド |
| --- | --- |
| 通る 9 | `length`・`is_empty`・`clone`・`concat`・`trim`・`trim_start`・`trim_end`・`to_ascii_lower`・`to_ascii_upper`（引数 1 個） |
| `E1013` 12 | `compare`・`find`・`rfind`・`contains`・`starts_with`・`ends_with`・`slice`・`sub`・`join`・`split`・`repeat`・`replace`（引数 2 個以上） |

対照: `String.find (ref "a") (ref "ab")` などの直接の関数は HEAD で通る（6 行すべて）。

原因: `src/constants.rs` の `Checker::temporary_borrows` は、借用する一時値（`ref "a"` や、自動で借用されるリテラル）を持つ呼び出しが「引数をすべて一度に適用する」ときだけ `BorrowOperand` に直す。`single_stage` の判定は、呼ばれるものが `Function`・`GenericFunction`・組み込みのときに引数の数と仮引数の数を比べ、それ以外は `arguments.len() == 1` と決める。クラスのメソッドの呼び出しは `TypedExprKind::Method(クラス, メソッド, 型)`（`src/polymorph.rs` の `method` が作る）で `_` の腕に落ちるので、2 引数以上で `single_stage` が偽になる。

試作の直し（D.1）: `TypedExprKind::Method(..)` の腕を足し、`callee.ty` が `Type::Function(parameters, _)` で `parameters.len() == arguments.len()` のときだけ真にする。試作での結果:

| 入力 | 結果 |
| --- | --- |
| `Text.m (ref "…") …` の 21 メソッド＋直接の 6 行＋ `u8` の 3 行（付録 2 の E.5 の `LITERAL_LINES` の 30 行） | 30 行すべて通る |
| 自動借用のリテラル（`Text.find "a" "ab"` を含む 12 行。`Text.contains (ref "a") "ab"` のように片方だけ `ref` を付けた行もある） | 12 行すべて通る |
| 戻り値が借用を持つメソッド（`def keep :: ref 'a -> ref 'a` に当たる形） | 従来どおり `E1013`（`loan_free` が偽） |
| 部分適用（`let f = Text.find "a"`） | 従来どおり `E1013` |

### B.2 特殊化の予算

上限は 65,536（`src/polymorph.rs` の `specialization_limits`）。`src/polymorph.rs` の `reaches_the_specialization_limit` と同じ形（`def id :: 'a -> 'a` と、`record R<i> { value: i8 }` と `fn f<i>(x: R<i>) -> R<i> { id x }` を N 組）の Main に式を足して `check` した。

| 足す式 | N | HEAD | 試作 |
| --- | --- | --- | --- |
| `0` | 65,536 | 通る | 通る |
| `0` | 65,537 | `E1017: more than 65536 specializations` | 同じ |
| `let a = String.length "abc"` ＋ `a` | 65,535 | — | 通る（1 つ使う） |
| 同上 | 65,536 | — | `E1017` |
| `let a = Text.length "abc"` ＋ `a` | 65,535 | — | `E1017`（2 つ使う） |
| 同上 | 65,534 | — | 通る |
| `Text.length "abc"` と `Text.length "abcd"`（同じ型 2 回） | 65,534 | — | 通る（型と名前が同じなら 2 つのまま） |
| `Text.length "abc"` と `Text.length u8"abcd"`（2 つの型） | 65,533 | — | `E1017` |
| 同上 | 65,532 | — | 通る（4 つ使う） |
| `String.length "abc"` と `Utf8String.length u8"abcd"` | 65,534 | — | 通る（2 つ使う） |

結論: 読み込んだだけで使わない `Text` は 0 個。`Text.m` を型 T に使うと、(メソッド, 型) の組ごとに、instance のメソッドと呼ばれる `String.m`／`Utf8String.m` の 2 個を使う（直接の呼び出しは 1 個）。

### B.3 生成される IR とオブジェクトコード

試作（Phase 1 の std）で、`Text.repeat`・`Text.contains`・`Text.starts_with`・`Text.length` を `string` と `utf8string` に使う次のプログラム（`Text` の版）と、`Text.` を `String.`（`utf8string` の関数は `Utf8String.`）に置き換えた同じ形のプログラム（直接の版）を比べた。字下げはタブ 1 個。

```tsuzuri
export def score :: i64 -> i64
fn score n =
	let base = "ab"
	let text = Text.repeat (ref base) n
	let needle = "ba"
	let a = if Text.contains (ref needle) (ref text) then 1 else 0
	let b = if Text.starts_with (ref needle) (ref text) then 10 else 0
	a + b + Text.length (ref text)

export def score8 :: i64 -> i64
fn score8 n =
	let base = u8"ab"
	let text = Text.repeat (ref base) n
	let needle = u8"ba"
	let a = if Text.contains (ref needle) (ref text) then 1 else 0
	let b = if Text.starts_with (ref needle) (ref text) then 10 else 0
	a + b + Text.length (ref text)
```

`tsuzuri build <dir> --emit llvm -o out.ll`（`--emit llvm` は `-O` を無視し、`-O0` と `-O3` で同じバイト列）。`Text` の版の instance のメソッドは、本体が 1 回の呼び出しだけの関数:

```text
define internal i64 @tz.fn.String.$instance.428.length(ptr noalias nocapture noundef nonnull readonly align 8 dereferenceable(16) %arg0) nounwind {
entry:
  %v0 = alloca ptr, align 16
  br label %loop
loop:
  %p0 = phi ptr [ %arg0, %entry ]
  store ptr %p0, ptr %v0
  %v1 = load ptr, ptr %v0
  %v2 = call i64 @tz.fn.String.length(ptr %v1)
  ret i64 %v2
}
```

呼び出し側（`Main.score` の `call` の行）。`Text` の版は instance のメソッドを直接呼び、直接の版は関数を直接呼ぶ。間接呼び出し・辞書・vtable はない（`grep -c "tz.dyn\|%tz.hkt\|dictionary" out.ll` は 0）:

```text
Text の版:   %v5 = call %tz.string @tz.fn.String.$instance.446.repeat(ptr %v3, i64 %v4)
             %v10 = call i1 @tz.fn.String.$instance.433.contains(ptr %v9, ptr %v6)
             %v13 = call i1 @tz.fn.String.$instance.434.starts_with(ptr %v9, ptr %v6)
             %v19 = call i64 @tz.fn.String.$instance.428.length(ptr %v6)
直接の版:    %v5 = call %tz.string @tz.fn.String.repeat(ptr %v3, i64 %v4)
             %v10 = call i1 @tz.fn.String.contains(ptr %v9, ptr %v6)
             %v13 = call i1 @tz.fn.String.starts_with(ptr %v9, ptr %v6)
             %v19 = call i64 @tz.fn.String.length(ptr %v6)
```

オブジェクトコードの比較（`--emit object`／`--target wasm32` の出力を `cmp`）:

| プログラム | 構成 | 結果 |
| --- | --- | --- |
| 小（4 メソッド） | native `-O1`・`-O2`・`-O3`・`-Os`・`-Oz`（1,688／1,704／1,704／1,704／1,360 バイト） | すべてバイト一致 |
| 小 | wasm32 `-O1`・`-O2`・`-O3`（4,028／4,084／4,404 バイト） | すべてバイト一致 |
| 小 | native `-O0`（34,896 と 39,504 バイト） | 一致しない（instance のメソッドの関数が残り、1 呼び出し増える） |
| 大（20 メソッドを `string` と `utf8string` に使う `text_class_calls`） | native `-O2`・`-O3`・`-Os`・`-Oz` | 一致（10,912／11,752／10,912／10,800 バイト） |
| 大 | native `-O1` | 一致しない（10,560 バイトで同じ長さ。数命令の違い） |
| 大 | wasm32 `-O1`・`-O2` | 一致（22,146／20,217 バイト） |
| 大 | wasm32 `-O3` | 一致しない（21,020 と 21,264 バイト。`tz_score` 3,341 → 3,395 命令、`tz_score8` 3,353 → 3,431 命令。補助の関数は一致。全体で +244 バイト、約 1.2%） |

測定の再現は `node tests/text_class.mjs <コンパイラ> --objects`（E.4）。この表は**測定であって表明ではない**（LLVM の組み込みの判断で、大きいプログラムでは数命令が動く）。表明にするのは、IR の構造（instance のメソッドが 1 回の呼び出しで、呼ぶ先が同名の関数であること。`tests/text_class.mjs` の既定の検査）だけ。

`Regex.*_text`（Phase 2）: `string` と `utf8string` に使った版と、`_utf8` を直接呼んだ版のオブジェクトの関数の集合は同じ（28 個のシンボル。ラッパーは残らない）。ただし 5 つの関数が数命令違う（例: `Regex.search` が 764 と 769 命令、サイズ 83,392 と 83,376 バイト）ので、バイト一致は主張しない。

### B.4 既存のプログラムと文書への影響

- **IR の差**: `examples/*`（13）と `tests/fixtures/*`（100）を `tsuzuri build <dir> --emit llvm -O0 -o <out>` で、HEAD のバイナリと試作（Phase 1 と Phase 2 の std、`constants.rs` の腕）の両方で作り、HEAD で作れた 102 個を比べた（H の手順）。バイト一致 50、生成シンボルの番号（`$intrinsic.Display.display.428` → `469` や `$instance.N`）を `tests/bench.rs` の `renumbered` と同じ規則で正規化すると一致 51、1 件（`tests/fixtures/computations`）は `Text.tc` が予約名になり E1011 で作れない（`Text.tc` を `Phrase.tc` に改名したコピーは `check`・`build` が通り、IR の大きさも同じ）。**番号が動く理由**: 常に読む std が 41 個の関数 ID を足すため。`-g` では `Text.tt` の `!DIFile` が 1 つ増え、メタデータの番号が動く。
- **ユーザーの `Text` と名前の衝突**（`check` の出力が HEAD と試作でバイト一致。8 件）: 既存のテストが使う `union Token = Text of string | Number of i64` と `Text "a"`・パターン `Text _`（`tests/json.rs`）、`const Text: string = "hello"` と `String.length (ref Text)`・`Text + Text`・`&Text` の誤り（`tests/constants.rs`、E1013 の文言も同じ）、`Json.Text "a"`。`type Text = string`（`tests/type_aliases.rs`）も同じ診断（E1012）。依存パッケージの `Acme::Tools::Text`（`tests/modules.rs` の `tools/Text.tz`）を `using Acme::Tools` で `Text.width 1` と呼ぶ構成も、HEAD と試作で `check` が通り、`run` の終了コードが 37 で一致。
- **ユーザーが `Text` の名前を使うと**（試作）: `class Text<'a>`・`record Text`・`union Text`・`type Text = …`・`const Text` は宣言できる。無修飾の `Text` は常にユーザーの宣言を指し、std のクラスは `Text.Text`（メソッドは `Text.Text.length`）と書く。`union Text = Plain | Rich` と `Text.Text.length "abc"` は `1 2 3 4`（`label Plain` `label Rich` と 2 つの長さ）を返し、`Text.length "abc"` は `E1002: union 'Main.Text' has no case 'length'`。
- **予約名にしたことで新しく E1011／E2000 になるもの**（試作。HEAD ではどれも通る。`Text` を `RegexText` に読み替えても同じ）:

| 入力 | 診断 |
| --- | --- |
| ルート直下の `Text.tz`・`Text.tt`・`Text.tc` | `E1011: module name 'Text' is reserved for the standard library; rename the file` |
| ファイルの `namespace Text` と `namespace Text::Tools` | `E1011: namespace 'Text' puts module 'Main' under 'Text', a name reserved for the standard library; choose another namespace` |
| `Tsuzuri.toml` の `namespace = "Text"` | `E1011: package namespace must be identifiers joined by '::' such as "Acme::Tools", at most 16 segments and 255 bytes, that do not start with 'std' or a standard library module name` |
| `tsuzuri new text`（`regex` と同じ） | `E2000: cannot derive a namespace from the folder name 'text'; pass --namespace` |
| 下位のディレクトリの `Utils/Text.tz`（`Utils::Text`） | 通る（予約されるのはルートの名前と名前空間の先頭だけ） |

- **ユーザーが `Text` のインスタンスを書く**（試作）: `record Name { value: string } deriving (Eq, Ord)` に `instance Text<Name>` を書くと、`is_empty` 以外の 20 メソッドを書けば通る。`dyn Text` は `E1028`、`Text.length (ref 5)` は `E1005: no instance for Text.Text<i32>`、`Text.length` に `string` と `utf8string` を混ぜると `E1003`、存在しないメソッドは `E1002: type class 'Text' has no method 'reverse'`。

### B.5 試作での検証の結果

| 検証 | 結果 |
| --- | --- |
| `node tests/features.mjs <試作> text_class`（`text_class` の fixture と参照値、E.1〜E.2） | `text_class: native/WASM at O0/O3 passed` ／ `features: 4595 cases passed`（約 35 秒、Node 20）。HEAD のバイナリでは `E1016: unknown type class 'Text'`。期待値を 22608n に変えると失敗する |
| `node tests/features.mjs <試作> regex_text`（F） | `regex_text: native/WASM at O0/O3 passed` ／ `features: 66 cases passed`。HEAD のバイナリでは `E1002: module 'Regex' has no function or union case 'find_all_text'`。参照値を 1 か所変えると失敗する |
| `node tests/text_class.mjs <試作> --objects`（E.4） | `text class: 40 instance methods, each one call; no indirect call` と B.3 の表 |
| `node tests/e2e.mjs <試作>` | 終了コード 0（`Polymorphism` の特殊化の上限 65,536 の検査を含む） |
| 関連する 20 の suite（`node tests/features.mjs <試作> <suite>`） | すべて `native/WASM at O0/O3 passed`。件数: `typeclasses` 39、`higher_kinds` 17、`deriving` 209、`string_interpolation` 82、`constants` 9、`const_functions` 19、`const_statics` 52、`string_library` 3,230、`regex` 2,273、`unicode` 4,341、`chars` 19、`json` 165、`cbor` 86、`display_parse` 10、`stdlib` 3、`maybe_result` 26、`visibility` 7、`dyn_dispatch` 55、`borrowed_comparisons` 10、`generic_records` 10 |
| `node scripts/check-docs.mjs …/text.md …/regex.md`（`TSUZURI_BIN=<試作>`） | `Docs: 2 pages, 17 links, 10 checked examples, 20 native runs (O0/O3), 0 test projects.` |

## C. std の骨格（試作で `tsuzuri check`・`run`・E2E を通した全文）

そのまま使う。メソッドの宣言の順序は `Text` の仕様の表（本文の「仕様」）と同じ。`.tz` の instance の本体はタブ 1 個で字下げする（`std/String.tz` の既存の書き方）。`.tt` はスペース 4 個。

### C.1 `std/Text.tt`（新規。Phase 1）

```tsuzuri
/// The operations that `string` (UTF-16) and `utf8string` (UTF-8) have in common. A method means exactly what the
/// function of the same name in `String` or `Utf8String` means, and `Text.f` on a `string` or a `utf8string` is that
/// call: the instances only delegate. A unit is a UTF-16 code unit for a `string` and a byte for a `utf8string`; every
/// length, offset, and range counts units, and every text argument is borrowed. The two types differ in these places,
/// and the instances keep each difference: `slice` and `sub` of a `utf8string` need offsets on a scalar boundary,
/// `split` and `replace` with an empty pattern work between the units of a `string` and between the scalars of a
/// `utf8string`, and `compare` and `<` order by UTF-16 code unit or by byte.
class Ord<'s> => Text<'s> {
    /// The number of units; the same as `.length`.
    def length :: ref 's -> i64

    /// True when the text has no units.
    def is_empty :: ref 's -> bool = \text -> Text.length text == 0

    /// -1, 0, or 1 by the order of the units.
    def compare :: ref 's -> ref 's -> i64

    /// A copy of the text.
    def clone :: ref 's -> 's

    /// The offset in units of the first match of the pattern (the first argument), or `None`.
    def find :: ref 's -> ref 's -> Maybe<i64>

    /// The offset in units of the last match of the pattern (the first argument), or `None`.
    def rfind :: ref 's -> ref 's -> Maybe<i64>

    /// True when the pattern (the first argument) occurs in the text.
    def contains :: ref 's -> ref 's -> bool

    /// True when the text starts with the prefix (the first argument).
    def starts_with :: ref 's -> ref 's -> bool

    /// True when the text ends with the suffix (the first argument).
    def ends_with :: ref 's -> ref 's -> bool

    /// The units from `first` up to, not including, `last`, or `None` for a bad range.
    def slice :: ref 's -> i64 -> i64 -> Maybe<'s>

    /// `count` units from `first`, or `None` for a bad range.
    def sub :: ref 's -> i64 -> i64 -> Maybe<'s>

    /// The parts joined by the separator (the first argument).
    def join :: ref 's -> ref ['s] -> 's

    /// The parts joined with no separator.
    def concat :: ref ['s] -> 's

    /// The pieces of the text between the matches of the separator (the first argument).
    def split :: ref 's -> ref 's -> ['s]

    /// The text without leading and trailing ASCII white space.
    def trim :: ref 's -> 's

    /// The text without leading ASCII white space.
    def trim_start :: ref 's -> 's

    /// The text without trailing ASCII white space.
    def trim_end :: ref 's -> 's

    /// The text with `A` to `Z` changed to `a` to `z`.
    def to_ascii_lower :: ref 's -> 's

    /// The text with `a` to `z` changed to `A` to `Z`.
    def to_ascii_upper :: ref 's -> 's

    /// The text repeated `count` times.
    def repeat :: ref 's -> i64 -> 's

    /// The text with every match of the pattern (the first argument) replaced by the replacement (the second).
    def replace :: ref 's -> ref 's -> ref 's -> 's
}
```

### C.2 `std/String.tz` の末尾に追記（Phase 1）

ファイルの最後の `String.from_code_units (Vec.to_array output)` の行のあとに、空行 1 つを置いて足す。

```tsuzuri
instance Text<string> {
	fn length text = String.length text
	fn compare left right = String.compare left right
	fn clone text = clone_string text
	fn find needle text = String.find needle text
	fn rfind needle text = String.rfind needle text
	fn contains needle text = String.contains needle text
	fn starts_with prefix text = String.starts_with prefix text
	fn ends_with suffix text = String.ends_with suffix text
	fn slice text first last = String.slice text first last
	fn sub text first count = String.sub text first count
	fn join separator parts = String.join separator parts
	fn concat parts = String.concat parts
	fn split separator text = String.split separator text
	fn trim text = String.trim text
	fn trim_start text = String.trim_start text
	fn trim_end text = String.trim_end text
	fn to_ascii_lower text = String.to_ascii_lower text
	fn to_ascii_upper text = String.to_ascii_upper text
	fn repeat text count = String.repeat text count
	fn replace needle replacement text = String.replace needle replacement text
}
```

### C.3 `std/Utf8String.tz` の末尾に追記（Phase 1）

ファイルの最後の `Maybe.get (Utf8String.from_bytes (Vec.to_array output))` の行のあとに、空行 1 つを置いて足す。

```tsuzuri
instance Text<utf8string> {
	fn length text = Utf8String.length text
	fn compare left right = Utf8String.compare left right
	fn clone text = Utf8String.clone text
	fn find needle text = Utf8String.find needle text
	fn rfind needle text = Utf8String.rfind needle text
	fn contains needle text = Utf8String.contains needle text
	fn starts_with prefix text = Utf8String.starts_with prefix text
	fn ends_with suffix text = Utf8String.ends_with suffix text
	fn slice text first last = Utf8String.slice text first last
	fn sub text first count = Utf8String.sub text first count
	fn join separator parts = Utf8String.join separator parts
	fn concat parts = Utf8String.concat parts
	fn split separator text = Utf8String.split separator text
	fn trim text = Utf8String.trim text
	fn trim_start text = Utf8String.trim_start text
	fn trim_end text = Utf8String.trim_end text
	fn to_ascii_lower text = Utf8String.to_ascii_lower text
	fn to_ascii_upper text = Utf8String.to_ascii_upper text
	fn repeat text count = Utf8String.repeat text count
	fn replace needle replacement text = Utf8String.replace needle replacement text
}
```

instance のメソッドの名前（`fn length text = …`）はクラスの宣言に書いたメソッド名と同じ。instance の頭は**裸のクラス名**（`instance Text<string>`）。std のコードは、ユーザーのクラスを見ない（std 用の名前解決は std のクラスだけを見る）ので、修飾（`Text.Text`）は要らない。

### C.4 `std/RegexText.tt`（新規。Phase 2）

```tsuzuri
/// The text types that a `Regex` searches: `string` (UTF-16) and `utf8string` (UTF-8). A method is the function of the
/// same name in `Regex` for a `string` and the function with the suffix `_utf8` for a `utf8string`; the instances only
/// delegate. `Regex.is_match_text` and its six siblings call these methods, so write those. Ranges are in code units of
/// a `string` and in bytes of a `utf8string`.
class RegexText<'s> {
    /// `Regex.is_match` or `Regex.is_match_utf8`.
    def is_match :: ref Regex -> ref 's -> bool

    /// `Regex.find` or `Regex.find_utf8`.
    def find :: ref Regex -> ref 's -> Maybe<(i64 * i64)>

    /// `Regex.find_at` or `Regex.find_at_utf8`.
    def find_at :: ref Regex -> ref 's -> i64 -> Maybe<(i64 * i64)>

    /// `Regex.find_all` or `Regex.find_all_utf8`.
    def find_all :: ref Regex -> ref 's -> [(i64 * i64)]

    /// `Regex.captures` or `Regex.captures_utf8`.
    def captures :: ref Regex -> ref 's -> Maybe<[(i64 * i64)]>

    /// `Regex.replace_all` or `Regex.replace_all_utf8`; the replacement has the type of the text.
    def replace_all :: ref Regex -> ref 's -> ref 's -> 's

    /// `Regex.split` or `Regex.split_utf8`.
    def split :: ref Regex -> ref 's -> ['s]
}
```

### C.5 `std/Regex.tz` の末尾に追記（Phase 2）

ファイルの最後（`split_utf8` の本体の `Vec.to_array output` の行）のあとに、空行 1 つを置いて足す。

```tsuzuri
instance RegexText<string> {
	fn is_match re text = Regex.is_match re text
	fn find re text = Regex.find re text
	fn find_at re text start = Regex.find_at re text start
	fn find_all re text = Regex.find_all re text
	fn captures re text = Regex.captures re text
	fn replace_all re text replacement = Regex.replace_all re text replacement
	fn split re text = Regex.split re text
}

instance RegexText<utf8string> {
	fn is_match re text = Regex.is_match_utf8 re text
	fn find re text = Regex.find_utf8 re text
	fn find_at re text start = Regex.find_at_utf8 re text start
	fn find_all re text = Regex.find_all_utf8 re text
	fn captures re text = Regex.captures_utf8 re text
	fn replace_all re text replacement = Regex.replace_all_utf8 re text replacement
	fn split re text = Regex.split_utf8 re text
}

/// `is_match` for a `string` or `is_match_utf8` for a `utf8string`, chosen by the type of `text`.
def is_match_text :: RegexText<'s> => ref Regex -> ref 's -> bool
fn is_match_text re text = RegexText.is_match re text

/// `find` for a `string` or `find_utf8` for a `utf8string`.
def find_text :: RegexText<'s> => ref Regex -> ref 's -> Maybe<(i64 * i64)>
fn find_text re text = RegexText.find re text

/// `find_at` for a `string` or `find_at_utf8` for a `utf8string`.
def find_at_text :: RegexText<'s> => ref Regex -> ref 's -> i64 -> Maybe<(i64 * i64)>
fn find_at_text re text start = RegexText.find_at re text start

/// `find_all` for a `string` or `find_all_utf8` for a `utf8string`.
def find_all_text :: RegexText<'s> => ref Regex -> ref 's -> [(i64 * i64)]
fn find_all_text re text = RegexText.find_all re text

/// `captures` for a `string` or `captures_utf8` for a `utf8string`.
def captures_text :: RegexText<'s> => ref Regex -> ref 's -> Maybe<[(i64 * i64)]>
fn captures_text re text = RegexText.captures re text

/// `replace_all` for a `string` or `replace_all_utf8` for a `utf8string`.
def replace_all_text :: RegexText<'s> => ref Regex -> ref 's -> ref 's -> 's
fn replace_all_text re text replacement = RegexText.replace_all re text replacement

/// `split` for a `string` or `split_utf8` for a `utf8string`.
def split_text :: RegexText<'s> => ref Regex -> ref 's -> ['s]
fn split_text re text = RegexText.split re text
```

`std/Regex.tz` の中では `RegexText<'s>`・`RegexText.is_match` を裸で書ける（std の中の名前解決は、opt-in の std モジュールも候補にする）。ユーザーのコードからは `RegexText.RegexText<'s>` と修飾する（本文の「仕様」）。

## D. Rust などの変更

### D.1 `src/constants.rs`（Phase 1。コンパイラの変更はここだけ）

`Checker::temporary_borrows` の `let single_stage = match &callee.kind { … }` に、`_ => arguments.len() == 1,` の直前の腕を 1 つ足す（試作で `diff -u` した結果。`Type` はこのファイルが既に使っている）。

```diff
             TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                 instance.builtin.scheme().parameters.len() == arguments.len()
             }
+            TypedExprKind::Method(..) => matches!(
+                &callee.ty,
+                Type::Function(parameters, _) if parameters.len() == arguments.len()
+            ),
             _ => arguments.len() == 1,
         };
```

関数の doc comment（「呼び出しは仮引数をすべて一度に適用し、借用を持たない値を返す」）は、そのまま正しい。ほかの腕・`loan_free`・`BorrowOperand` への書き換えは変えない。

### D.2 `src/stdlib.rs`（Phase 1）

```diff
     ("std/Tensor.tz", include_str!("../std/Tensor.tz")),
+    ("std/Text.tt", include_str!("../std/Text.tt")),
 ];
```

```diff
     "Tensor",
     "Channel",
+    "Text",
 ];
```

（`SOURCES` の末尾に足す。先頭や途中に足すと、既存のソースの添字が動く。`RESERVED_MODULES` も末尾。）`OPT_IN` は変えない（`Text` は常に読む）。

### D.3 `src/stdlib.rs`（Phase 2）

```diff
     ("std/Text.tt", include_str!("../std/Text.tt")),
+    ("std/RegexText.tt", include_str!("../std/RegexText.tt")),
 ];
```

```diff
     "Text",
+    "RegexText",
 ];
```

```diff
     OptIn {
         module: "Regex",
-        names: &["Regex"],
+        names: &["Regex", "RegexText"],
         aliases: &[],
-        uses: &["Unicode"],
+        uses: &["Unicode", "RegexText"],
     },
```

`OPT_IN` の配列の最後（`Channel` の `OptIn { … },` の次）に足す:

```rust
    OptIn {
        module: "RegexText",
        names: &["RegexText"],
        aliases: &[],
        uses: &["Regex"],
    },
```

理由（`src/stdlib.rs` の単体テスト `opt_in_modules_are_reached_only_through_their_names` の規則。読んで確かめた）: ① `std/Regex.tz` の instance は型 `string`・`utf8string` が Regex のものでないので、クラス名 `RegexText` を `Regex` の `names` に入れる。② `Regex.tz` が `RegexText` を、`RegexText.tt` が `Regex` を名指すので、互いを `uses` に入れる（テストは、名指している組と `uses` が一致することを要求する）。③ 常に読む `Text.tt`・`String.tz`・`Utf8String.tz` は opt-in のモジュール名（`OPT_IN` の `module` に並ぶ名前。たとえば `Regex`・`RegexText`）を名指さない（同じテストの前半）。`.tt` の `///` の中の語は名指しに数えない。

### D.4 既存のテストと型の期待値の変更（これで全部。ほかの既存のテストは変えない）

| ファイル | 場所 | 変更 | Phase |
| --- | --- | --- | --- |
| `src/stdlib.rs` | `mod tests` の `reserves_the_d07_table` | `assert_eq!(RESERVED_MODULES.len(), 53);` を `54` に。`assert!(is_reserved_module("Text"));` を `Tensor` の行の次に足す | 1 |
| 同上 | 同上 | `54` を `55` に。`assert!(is_reserved_module("RegexText"));` を足す | 2 |
| `src/stdlib.rs` | `sources_for_loads_named_opt_in_modules_and_their_uses` | `assert_eq!(loaded("Regex.compile (ref text)"), ["Regex", "Unicode"]);` と `assert_eq!(loaded("let x = 1Regex"), ["Regex", "Unicode"]);` の右辺を `["Regex", "Unicode", "RegexText"]` に。`assert_eq!(loaded("RegexText.RegexText.find_all"), ["Regex", "Unicode", "RegexText"]);` を足す（読み込み順は `SOURCES` の順） | 2 |
| `tests/stdlib.rs` | `rejects_reserved_modules_and_invalid_std_sources` の最初の配列 | `("Text.tz", ""), ("Text.tt", ""), ("Text.tc", ""),` を足す（Phase 2 は `("RegexText.tz", ""), ("RegexText.tt", ""), ("RegexText.tc", ""),` も）。空の `Text.tt` でも `E1011` になる（確認済み） | 1, 2 |
| `tests/computations.rs` | 7 行目 `const TEXT: &str = include_str!("fixtures/computations/Text.tc");` | `const PHRASE: &str = include_str!("fixtures/computations/Phrase.tc");` | 1 |
| 同上 | `("Text.tc", TEXT),`（530 行目付近） | `("Phrase.tc", PHRASE),` | 1 |
| 同上 | 文字列の中の `Text { let! prefix = text; return prefix + \" world\" }`・`Text { return text }`・`Text { yield \"a\"; yield! \"b\" }`（663・676・1023 行目付近の 3 か所） | `Text` を `Phrase` に | 1 |
| `tests/fixtures/computations/Main.tz` | `Text {` の 3 か所（150・169・225 行目付近） | `Phrase {` | 1 |
| `tests/fixtures/computations/Text.tc` | ファイル | `git mv tests/fixtures/computations/Text.tc tests/fixtures/computations/Phrase.tc`（中身は変えない。ビルダー名はファイル名から決まる） | 1 |
| `vsc/src/core.ts` | `export const libraryModules = new Set('… Tensor Channel'.split(' '));` | 文字列の末尾に ` Text`（Phase 2 は ` Text RegexText`）を足す。`vsc/src/test/core.test.ts` が `src/stdlib.rs` の `RESERVED_MODULES` と同じ集合かを検査する（`cd vsc && npm run test:unit`） | 1, 2 |

期待値が**変わらない**ことを確かめたもの: `tests/modules.rs`（`tools/Text.tz`。依存パッケージの `Acme::Tools::Text` で、B.4 の再現で `check` が通り `run` の終了コードが 37 のまま）、`tests/json.rs`・`tests/constants.rs`・`tests/type_aliases.rs`（ユーザーの `Text` の名前。B.4 の 8 件）、`src/polymorph.rs` の `honors_the_exact_specialization_limit`・`hash_containers_leave_the_specialization_budget_to_users`（使わない `Text` は予算を使わない。B.2）、`tests/e2e.mjs`（B.5）。IR の番号が動くことで壊れる追跡済みのテストはない（生成シンボルの番号を決め打ちしているのは、git の管理外の `tests/fixtures/net_sockets/Main.ll` だけ）。

### D.5 `src/polymorph.rs` の単体テスト（予算。Phase 1）

`mod tests` の `hash_containers_leave_the_specialization_budget_to_users` の次に足す（`SMALL`・`analyze_within` は同じ `mod tests` の既存の補助。`SMALL.total` は 64）。B.2 の測定の縮小版:

```rust
    #[test]
    fn a_text_method_costs_one_more_specialization_than_the_direct_call() {
        use std::fmt::Write;
        let program = |records: usize, call: &str| {
            let mut source = String::from("def id :: 'a -> 'a\nfn id x = x\n");
            for index in 0..records {
                writeln!(
                    source,
                    "record R{index} {{ value: i8 }}\nfn f{index}(x: R{index}) -> R{index} {{ id x }}"
                )
                .unwrap();
            }
            writeln!(source, "let a = {call}\na").unwrap();
            source
        };
        let total = SMALL.total;
        let direct = program(total - 1, "String.length \"abc\"");
        assert_eq!(analyze_within(&[("Main", &direct)]), Ok(()));
        let through_text = program(total - 2, "Text.length \"abc\"");
        assert_eq!(analyze_within(&[("Main", &through_text)]), Ok(()));
        let one_too_many = program(total - 1, "Text.length \"abc\"");
        let (code, _) = analyze_within(&[("Main", &one_too_many)]).unwrap_err();
        assert_eq!(code, "E1017");
    }
```

（試作の実際の上限 65,536 では、この 3 つが B.2 の表の 65,535 の 2 行と 65,534 の 1 行に当たる。単体テストは `SMALL` の上限 64 で動く。コンパイルしていないので、`cargo test --locked a_text_method_costs` が `running 1 test` で通ることを確かめる。）

### D.6 CI（`.github/workflows/targets.yml`。Phase 1・2）

`Text` の値と IR の構造を CI でも検査する。`End-to-end compiler tests` の step の次（`Memory and LLVM fact semantics` の前）に、次の step を足す。`windows.yml` には足さない（`string_library` など同じ種類の suite も Windows の job にない）。実行の時間は、`text_class` が約 35 秒、`regex_text` が約 20 秒、`text_class.mjs` が数秒（`--objects` なし）。速度の閾値はない。

```yaml
      - name: Text class (D14) - values against JavaScript references, instance methods are single direct calls
        if: ${{ !cancelled() && steps.bundle.outcome == 'success' }}
        run: |
          export TSUZURI_CLANG="$LLVM_PREFIX/bin/clang"
          export TSUZURI_WASM_LD="${TSUZURI_WASM_LD:-$LLVM_PREFIX/bin/wasm-ld}"
          node tests/features.mjs target/release/tsuzuri text_class
          node tests/text_class.mjs target/release/tsuzuri
```

Phase 2 は、同じ `run` の `text_class` の行の次に `node tests/features.mjs target/release/tsuzuri regex_text` を足す。YAML は試作で実行していない。PR の CI が緑になることで確かめる（`gh run list` で前の run と比べ、既存の失敗と新しい失敗を分ける）。



