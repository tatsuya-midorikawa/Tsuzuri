# D09: 正規表現と Unicode テキスト処理

| 項目 | 内容 |
| --- | --- |
| ID | D09 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | D02, A08 |
| 後続 | D07 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（std モジュール名 `Regex`・`Unicode` の予約の確定。GUIDE D-30 の仮割り当て。利用者の `Regex.tz`・`Unicode.tz` が E1011 になる）、D11（Phase 2 の着手と表の置き場所） |
| 改善する劣位 | C#/F# 比: 標準ライブラリの不足（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: 正規表現・Unicode 正規化・書記素処理がない |
| 手本にする既存実装 | std の不透明レコード: `std/Map.tz` の `Map` と `src/stdlib.rs` の `opaque_record`。文字列の走査と組み立て: `std/String.tz` の `find`・`matches_at`・`append_range`・`split`、`std/Utf8String.tz` の `boundary` と組み込みの `Utf8String.decode_at`。生成物を追跡する方式: `src/runtime/generate.py`（入力から決定的に生成し、先頭に `Do not edit.` を書く）。Rust テスト: `tests/map_set.rs` の `ordered_containers_are_opaque_noncopy_owned_values`。E2E: `tests/features.mjs` の `string_library` suite（参照値を JS で計算して `cases` へ push）と `map_set` suite（`inspect` で IR を検査） |
| 主な影響ファイル | `std/Regex.tz`（新規）, `std/Unicode.tz`（新規・生成物）, `scripts/generate-unicode.mjs`（新規）, `src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES`・`opaque_record`）, `tests/regex.rs`（新規）, `tests/regex-cases.mjs`（新規）, `tests/fixtures/regex/Main.tz`（新規）, `tests/fixtures/regex/Cases.tz`（新規・生成物）, `tests/features.mjs`, `docs/language.md`, `_docs/library-reference/regex.md`（新規）, `_docs/library-reference/README.md`, `_docs/library-reference/text.md`, `_docs/feature-status.md`, `_features/README.md`, `_features/GUIDE.md`（D-07 の表と D-30 の行の移動） |

## 目的

.NET の `Regex`、Rust の `regex`、C++ の `<regex>` に相当する検索・置換・分割を標準で提供する。入力の検証・抽出・整形という
Tsuzuri の主要用途で、ホスト実装に頼らずに済むようにする。エンジンは線形時間（Pike VM）とし、利用者が入力やパターンを
信頼できない場合でも ReDoS（指数時間の照合）が構造的に起きないようにする。

Unicode は Phase 1 で正規表現が必要とする表（一般カテゴリー、`Alphabetic`、`White_Space`、`Join_Control`、単純大文字小文字畳み込み）
だけを、版を固定した UCD から生成する。正規化・書記素クラスター・単語境界・完全な大文字小文字変換は Phase 2 とする。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求め、決定 D11 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D02・A08・B01 が `_features/README.md` の状態欄で done であること（HEAD で 3 件とも done）。確認:
  `grep -nE "^\| (D02|A08|B01) " _features/README.md`。
- D1 が承認済みであること。承認前はどの手順にも着手しない（手順 2 で予約語一覧を変えるため）。
- 参照用の Node が Unicode 17.0 を報告すること。確認: `node -p process.versions.unicode` が `17.0`。HEAD の環境
  （Node v20.19.6、ICU 78.1）はこの条件を満たす。版が異なる Node しかなければ停止して報告する（D6）。
- 生成に使う UCD 17.0.0 の 4 ファイルを `target/ucd/17.0.0/` に取得していること（手順 3 のコマンド）。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Phase 1 の実装に `src/stdlib.rs` 以外のコンパイラ変更（新しい `Builtin`、構文、`src/llvm.rs`、`src/runtime/` の C や `.ll`）が必要になった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。
- `Regex` の値に照合中の可変状態（DFA のキャッシュ、統計）を持たせたくなった（D3）。
- `Regex` を使わないプログラムの IR が手順 1 のベースラインと byte 単位で一致しない（D-07 の到達可能性による間引きが効いていない）。
- 生成した `std/Unicode.tz` が 128 KiB（131,072 bytes）を超える。または手順 1 の `check` 時間（3 回の中央値）が 20% 以上増えた（D7）。
- JS 参照と全体一致の範囲（group 0）が一致しない case が出た。捕捉 group の差は「落とし穴」の既知の差（反復内の group）だけを許す。
- V8（`\p{...}` と `iu` フラグ）から計算した表が、UCD から生成した表と一致しない。
- `honors_the_exact_specialization_limit`、または stack-depth の 3 テスト（手順 1）が失敗する。上限や stack サイズを上げたくなった。
- 既存テストの期待値（IR・診断コード・メッセージ）を変える必要がある。

## 現状（HEAD `f8dc655` で確認）

- 正規表現も Unicode の表もない（`std/` に `Regex.tz`・`Unicode.tz` がない）。
- std の埋め込み: `src/stdlib.rs` の `SOURCES`（`include_str!` の一覧）、`RESERVED_MODULES`（`Regex`・`Unicode` を含まない）、
  `opaque_record`（`"Map.Map" | "Map.Entry" | "Set.Set" | "Seq.Seq" | "Gpu.Device" | "Gpu.Buffer" | "IO.IO"`）。同じファイルのテスト
  `embedded_sources_use_reserved_flat_modules` は `SOURCES` の全モジュールが予約済みであることを、`std_definitions_never_shadow_builtin_functions`
  は std の `def` が組み込み関数を隠さないことを検査する。
- GUIDE D-07: std は常に読み込まれ型検査される。未使用の std 関数は IR に出ない（到達可能性で間引く）。したがって std のソースの大きさは
  全プログラムの `check` 時間に効き、生成コードの大きさには使った関数の分だけ効く。1 ファイルの上限は `src/syntax.rs` の
  `MAX_SOURCE_BYTES`（1 MiB）。
- 文字列 API は Tsuzuri で書かれている。`std/String.tz` の `find` は `matches_at` による素朴な照合で、`text[index]` は `i16u` の
  コード単位を返す（`append_range` が `Vec<i16u>` へ push する）。`String.from_code_units`・`Utf8String.decode_at`・`Vec.push`・
  `Vec.set`・`Vec.with_capacity`・`Vec.to_array`・`Array.set` は組み込み（`src/check.rs` の `Builtin` の名前表）。
- 文字列リテラルは `src/llvm.rs` の `string_constant` が `@tz.literal.N = private unnamed_addr constant [N x i16] [...]` を出し、
  `TypedExprKind::String` の評価ごとに `@tz.string.new(ptr, i64)` で確保して複製する。static データはない（D-28。チケット D11 の Phase 2 は要承認）。
  したがって std の中で表を文字列リテラルに埋め込むと、評価のたびに表の大きさの確保と複製が起きる。
- 生成物の前例: `src/runtime/generate.py` は `numeric.c` から `numeric.ll` を決定的に生成し、先頭に
  `; Generated from numeric.c by generate.py. Do not edit.` を書く。std のソース（`std/*.tz`）は `.gitignore` の対象外なので例外行は不要。
- コメントは `//`・`/* */`（入れ子可、`src/lexer.rs` の `comment`）・`///`（文書コメント）。
- 参照用の Node: v20.19.6、`process.versions.unicode` は `17.0`、`process.versions.icu` は `78.1`（2026-09-30 に確認）。
- `_docs/feature-status.md` の D09 行は「未着手（計画）: 線形時間の正規表現、Unicode 正規化と書記素」。

### 再現（検証済み）

`Regex` はまだ値として解決できない。

```tsuzuri
export def probe :: i64 -> i64
fn probe x =
    let r = Regex.compile "a"
    x
```

`target/release/tsuzuri check /tmp/tz-work-D09/repro` は `error[E1002]: unknown value 'Regex'`（3:13）で終了コード 1。利用者のモジュール
`Regex.tz`（`def answer :: i64 -> i64`）を `Regex.answer x` で呼ぶプロジェクトは今は成功し（終了コード 0）、D1 の後は E1011 になる。

## 仕様

Phase 1 は正規表現と、正規表現が使う Unicode の表だけを実装する。Phase 2（「Phase 2（設計方針）」）は実装しない。

### 前提とする他チケットのインターフェース

- D02（done）: `String`・`Utf8String` の添字規則（string は UTF-16 コード単位、utf8string は byte）、`String.from_code_units`、
  `Utf8String.decode_at :: ref utf8string -> i64 -> (utf8char * i64)`、`Utf8String.from_bytes`。
- A08（done）: `char` は UTF-16 コード単位、`utf8char` は Unicode スカラー（D-12）、`Utf8Char.to_u32`。
- B01（done）: `Result`（`Ok`・`Error`）と `Maybe`。
- D07 Phase 2 へ提供する書記素 API は Phase 2 の範囲で、Phase 1 は何も提供しない。

### 構文

パターンは `string`（UTF-16）で渡す。Phase 1 の構文は次のとおりで、JS の `u` フラグ付き正規表現の部分集合と同じ書き方にする（D4）。

```text
pattern     = [ "(?" flags ")" ] alternation ;          (* 旗の群はパターンの先頭だけ *)
alternation = sequence { "|" sequence } ;
sequence    = { piece } ;
piece       = atom [ quantifier ] ;
quantifier  = ( "*" | "+" | "?" | "{" n "}" | "{" n ",}" | "{" n "," n "}" ) [ "?" ] ;   (* n は 10 進 0..1000 *)
atom        = literal | "." | "^" | "$" | escape | class | group ;
group       = "(" alternation ")" | "(?:" alternation ")" | "(?" flags ":" alternation ")" ;
flags       = { flag } [ "-" flag { flag } ] ;           (* 少なくとも一つの flag *)
flag        = "i" | "m" | "s" | "u" ;
class       = "[" [ "^" ] item { item } "]" ;
item        = catom [ "-" catom ] | perl | property ;
catom       = any scalar except "\" "]" "[" | cescape ;  (* "-" は先頭か末尾だけ字義どおり *)
escape      = cescape | perl | property | "\b" | "\B" | "\A" | "\z" ;
cescape     = "\" syntax | "\-" | "\/" | "\n" | "\r" | "\t" | "\f" | "\v" | "\0"
            | "\x" hex hex | "\u" hex hex hex hex | "\u{" hex { hex } "}" ;
perl        = "\d" | "\D" | "\w" | "\W" | "\s" | "\S" ;
property    = ( "\p{" | "\P{" ) name "}" ;
syntax      = "^" | "$" | "\" | "." | "*" | "+" | "?" | "(" | ")" | "[" | "]" | "{" | "}" | "|" ;
literal     = any scalar except syntax ;
```

- 字義どおりの `{`・`}`・`]` は class の外では必ず escape する。class の中の `[` と `&&` は Unsupported（将来の集合演算のため）。
- `\uD83D\uDE00` のように隣り合う上位・下位サロゲートの escape は一つのスカラーになる（JS と同じ）。単独のサロゲートの escape は
  string の入力の孤立サロゲートにだけ一致する。`\u{...}` は `10FFFF` 以下。`\0` の後に数字、`\1`〜`\9` は Unsupported（後方参照）。
- 量指定子の連続（`a**`、`a{2}*`）、先頭の量指定子（`*a`）、表明への量指定子（`^*`、`\b+`）は Syntax。空の class（`[]`、`[^]`）は Syntax。
- flag: `i` は大文字小文字の区別なし（D9）、`m` は `^`・`$` が `\n` の前後にも一致、`s` は `.` が `\n` にも一致、`u` は既定で有効で、
  `-u` にすると `\d`・`\w`・`\s`・`\b` と `i` の畳み込みが ASCII だけになる。`.` は `-u` でもスカラー 1 個に一致する。
- `\d` は `Nd`、`\s` は `White_Space`、`\w` は `Alphabetic`・`M`・`Nd`・`Pc`・`Join_Control` の和（UTS #18 附属書 C）。`-u` では
  `[0-9]`、`[\t\n\v\f\r ]`、`[0-9A-Za-z_]`。`\b` は前後のスカラーの `\w` 判定が異なる位置（範囲外は非単語）。
- `\p{name}` の name は一般カテゴリーの短い名前 30 個（`Lu` … `Cn`）、群 `L`・`LC`・`M`・`N`・`P`・`S`・`Z`・`C`、
  `Alphabetic`・`White_Space`・`Join_Control`。それ以外（長い名前、`gc=`、`Script=`）は Unsupported。
- `^`・`$` は `m` なしでは入力の先頭・末尾だけ（`$` は末尾の `\n` の前に一致しない）。`\A`・`\z` は常に先頭・末尾。

### 型規則

新 API（実装後に有効。未検証）。`Regex.Regex` は不透明（`opaque_record`）で非 Copy、clone はない。全関数が `Regex` とテキストを
共有借用し、結果を所有値で返す。

```tsuzuri
union ErrorKind = Syntax | Unsupported | TooLarge
record Error { kind: ErrorKind, offset: i64, message: string }

def compile :: ref string -> Result<Regex, Error>
def escape :: ref string -> string
def group_count :: ref Regex -> i64
def is_match :: ref Regex -> ref string -> bool
def find :: ref Regex -> ref string -> Maybe<(i64 * i64)>
def find_at :: ref Regex -> ref string -> i64 -> Maybe<(i64 * i64)>
def find_all :: ref Regex -> ref string -> [(i64 * i64)]
def captures :: ref Regex -> ref string -> Maybe<[(i64 * i64)]>
def replace_all :: ref Regex -> ref string -> ref string -> string
def split :: ref Regex -> ref string -> [string]
def search_steps :: ref Regex -> ref string -> i64
```

- utf8string 版は同じ引数順で名前に `_utf8` を付ける: `is_match_utf8`、`find_utf8`、`find_at_utf8`、`find_all_utf8`、`captures_utf8`、
  `replace_all_utf8 :: ref Regex -> ref utf8string -> ref utf8string -> utf8string`、`split_utf8 :: ref Regex -> ref utf8string -> [utf8string]`。
  パターンは常に `ref string` で、一つの `Regex` を両方の符号化に使える（D8）。
- 範囲 `(start, end)` は半開区間で、string は UTF-16 コード単位、utf8string は byte。常にスカラーの境界にある。
- `captures` の要素 0 は全体一致、要素 k は k 番目の捕捉 group。一致に参加しなかった group は `(-1, -1)`。長さは `group_count re + 1`。
- `find_at re text start` は `start` から探索し、`^`・`\b` の判定には `start` より前の文字も使う。`start` が範囲外（負、長さ超）か
  スカラーの内部（サロゲート対の間、UTF-8 の継続 byte）なら `None`。
- `replace_all` の置換文字列は `${n}`（10 進。group n の文字列。参加しなかった group と `n > group_count` は空）と `$$`（`$`）だけを展開し、
  それ以外の `$` は字義どおり。`escape` は `syntax` の 14 文字の前に `\` を付ける。
- `search_steps` は `find` と同じ探索で実行した VM の手数（「アルゴリズム」の steps）を返す。試験用だが公開する（D12）。
- `Unicode` モジュールの新 API（実装後に有効。未検証）: `def version :: unit -> string`（`"17.0.0"`）、
  `def property_ranges :: ref string -> Maybe<[(i64 * i64)]>`（`\p{...}` と同じ名前の、昇順で連結済みの閉区間。未知の名前は `None`）、
  `def simple_case_folding :: unit -> [(i64 * i64)]`（`CaseFolding.txt` の状態 C と S の `(c, scf(c))`、`c != scf(c)`、`c` の昇順）。
  どれも呼び出しごとに表全体を復号して確保する。1 文字ごとの分類 API は Phase 2（D11）。

### 評価順序・所有権・借用

- 引数は通常どおり左から評価する。`Regex` は照合で変化しない（D3）ので、同じ値を共有借用で何度でも、複数の式から同時に使える。
- `Regex` は `Vec`・配列だけを持つので、task へ move できる。task 境界をまたぐ借用は既存どおり E1013。
- 照合の作業領域（thread 表、捕捉の slot）は各呼び出しの中で確保し、戻る前に解放する。native の host で各呼び出しの後 `live == 0`。

### 数値・トラップ・native と WASM の差

- 全関数は全域関数で、パターンにも入力にもトラップしない。失敗は `Regex.Error` か `None`。確保失敗だけが既存どおりトラップする。
- 結果（範囲、文字列、`search_steps`）は native と WASM、`-O0` と `-O3` で一致する。std の Tsuzuri コードだけで実装するので WASM の import は増えない。

### 診断

コンパイラの新しい診断はない。既存のコードが次の場合に出る。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1011 | 利用者のモジュール名が `Regex`・`Unicode` | 既存の予約モジュール名のメッセージ | 既存どおり |
| E1022 | `Regex` の field の読み書き・構築（std の外） | 既存の不透明レコードのメッセージ（`Map` と同じ） | 既存どおり |

実行時の `Regex.Error`（`message` は英語、小文字始まり、直し方を含む。`offset` はパターンの UTF-16 コード単位）:

| kind | 条件 | message | offset |
| --- | --- | --- | --- |
| Syntax | `(` が閉じない | `unclosed group; add ')'` | `(` |
| Syntax | 対応のない `)` | `unmatched ')'; escape it as '\)'` | `)` |
| Syntax | `[` が閉じない／空の class | `unclosed character class; add ']'`／`empty character class; escape ']' as '\]'` | `[` |
| Syntax | 字義どおりの `{`・`}`・`]` | `unescaped 'X'; escape it as '\X'` | その文字 |
| Syntax | 繰り返す対象がない | `repetition operator has nothing to repeat; add an expression before it or escape it` | 量指定子 |
| Syntax | `{` の形が不正、`n > m` | `invalid counted repetition; use {n}, {n,} or {n,m} with n <= m` | `{` |
| Syntax | class の範囲が逆順、端点が class | `invalid class range; use single characters with start <= end` | 範囲の始点 |
| Syntax | 未知の escape、末尾の `\` | `unknown escape '\X'; escape only syntax characters`／`pattern ends with '\'; escape a backslash as '\\'` | `\` |
| Syntax | `\x`・`\u` の値が不正 | `invalid code point escape; use \u{0} through \u{10FFFF}` | `\` |
| Syntax | 未知の flag | `unknown flag 'X'; use i, m, s or u` | その flag |
| Unsupported | `(?=`・`(?!`・`(?<=`・`(?<!`・`(?<name>`・`(?P<`・`(?>`・`(?#` | `unsupported group syntax; lookaround, named groups and atomic groups are not supported` | `(` |
| Unsupported | `\1`〜`\9`、`\k<`、`\0` の後の数字 | `backreferences are not supported; the engine guarantees linear time` | `\` |
| Unsupported | 先頭以外の `(?flags)` | `inline flags are allowed only at the start; use (?flags:...)` | `(` |
| Unsupported | 未対応の property | `unknown Unicode property 'X'; use a general category such as Lu, or Alphabetic, White_Space, Join_Control` | `\` |
| Unsupported | class 内の `[`・`&&`、所有的量指定子 `*+` など、`\G`・`\Z`・`\X`・`\R`・`\K` | `unsupported syntax 'X'` | その位置 |
| TooLarge | 資源上限を超えた | 「資源上限」の表 | 同表 |

### 資源上限

上限を超えたら `Result.Error { kind = TooLarge }` を返す。トラップしない（D10）。

| 上限 | 値 | message | offset |
| --- | --- | --- | --- |
| パターンの長さ | 65,536 コード単位 | `pattern is longer than 65536 code units` | 0 |
| 繰り返し回数 `n`・`m` | 1,000 | `repetition count exceeds 1000` | `{` |
| group の入れ子 | 64 | `groups are nested deeper than 64` | 65 段目の `(` |
| 命令数 | 10,000 | `compiled program exceeds 10000 instructions; simplify the pattern or reduce counted repetitions` | 0 |
| class の区間の合計 | 65,536 | `character classes exceed 65536 ranges in total` | 0 |
| 命令数 ×（2 ×（group 数 + 1）） | 262,144 | `too many capture groups for the pattern size; use (?:...) for groups that are not read` | 0 |

- 照合の時間は 1 回の探索（`is_match`・`find`・`find_at`・`captures`）で O(命令数 × 入力のスカラー数)。作業領域は
  `captures` で 2 × 命令数 × slot 数の `i64`（最大 4 MiB）、それ以外は O(命令数)。
- `find_all`・`replace_all`・`split` は一致ごとに探索を繰り返すので、最悪 O(命令数 × n²)（Rust の `regex` の反復と同じ）。文書に明記する。

### 例

新 API（実装後に有効。未検証）。

```tsuzuri
def digit_count :: ref string -> i64
fn digit_count text =
    match Regex.compile "\\d+" with
    | Result.Error _ -> -1
    | Result.Ok re ->
        match Regex.find (ref re) text with
        | Maybe.Some (start, stop) -> stop - start
        | Maybe.None -> 0
```

| パターン | 入力・操作 | 期待（参照は JS の `u` フラグ） |
| --- | --- | --- |
| `a\|ab` | `"ab"` の `find` | `(0, 1)`（leftmost-first。POSIX の最長一致ではない） |
| `.` | `"\u{1F600}"` の `find`／`find_utf8` | `(0, 2)`／`(0, 4)` |
| `.` | `"\uD800"` の `find` | `(0, 1)`（孤立サロゲートは 1 スカラー） |
| `(?i)ß` | `"\u{1E9E}"` の `is_match` | `true`（単純畳み込み） |
| `(?i)k` | `"\u{212A}"` の `is_match` | `true` |
| `\w`／`(?-u)\w` | `"é"` の `is_match` | `true`／`false` |
| `a*` | `"abc"` の `find_all` | `[(0,1); (1,1); (2,2); (3,3)]`（D5） |
| `,\s*` | `"a, b,c"` の `split` | `["a"; "b"; "c"]` |
| `(\w+)@(\w+)` | `"x@y"` を `"${2} at ${1}"` で `replace_all` | `"y at x"` |
| `(` ／ `a**` ／ `(?=a)` ／ `\1` | `compile` | Syntax 0 ／ Syntax 2 ／ Unsupported 0 ／ Unsupported 0 |
| `a{1001}` ／ `(?:a{1000}){1000}` | `compile` | TooLarge 1 ／ TooLarge 0 |

### Phase 2（設計方針）

`Unicode.normalize`（NFC・NFD・NFKC・NFKD）、UAX #29 の書記素クラスターと単語境界、`SpecialCasing.txt` による完全な大小変換、
1 スカラーごとの分類（`Unicode.category`）。表を評価ごとに複製しない置き場所が要るので、決定 D11 の承認まで着手しない。

## 設計

### データ構造

`std/Regex.tz`（新規）。新 API（実装後に有効。未検証）。命令は `i64` 3 個（op, a, b）の平坦な配列にし、thread の走査で union の分解や確保をしない。

```tsuzuri
record Regex {
    code: [i64],      // 命令 i: code[3i] = op, code[3i+1] = a, code[3i+2] = b
    classes: [i64],   // class k: classes[2k] = ranges の開始位置, classes[2k+1] = 区間の数
    ranges: [i64],    // 閉区間 lo, hi の列。class ごとに昇順・連結済み
    groups: i64,      // 捕捉 group の数（group 0 を除く）
    anchored: bool    // 最上位が選択でなく、最初の要素が \A か m なしの ^
}
```

| op | 名前 | a, b | 意味 |
| --- | --- | --- | --- |
| 0 | MATCH | - | 一致 |
| 1 | CHAR | a = スカラー | 1 スカラーを消費 |
| 2 | CLASS | a = class id | 区間の二分探索 |
| 3 | ANY／4 ANY_NO_NL | - | 任意／`\n` 以外（孤立サロゲートを含む） |
| 5 | SPLIT | a, b = 行き先 | a を優先 |
| 6 | JMP | a = 行き先 | |
| 7 | SAVE | a = slot | slot 2k が group k の開始、2k+1 が終了 |
| 8 | ASSERT | a = 種類、b = `\w` の class id | 0 `\A`、1 `\z`、2 行頭、3 行末、4 `\b`、5 `\B`（4・5 は b の class で判定） |

構文木は private の `union Node`（新規）を `Vec<Node>` に積み、子は別の `Vec<i64>` で参照する: `Empty`、`Literal of i64`、`Class of i64`、
`Any of bool`、`Assert of i64`、`Group of (i64 * i64)`（捕捉番号か -1、子）、`Concat of (i64 * i64)`・`Alternate of (i64 * i64)`（子の開始、数）、
`Repeat of (i64 * i64 * i64 * bool)`（子、min、max（-1 は無限）、greedy）。照合の作業領域は private の `record Threads`（新規）
`{ dense: Vec<i64>, sparse: Vec<i64>, count: i64, slots: Vec<i64> }` で、探索の開始時に大きさを確保し、照合の中は `Vec.set` だけを使う。

`std/Unicode.tz`（新規・生成物）の private の表は、UTF-16 の文字列リテラル（`\uXXXX` の列）に符号化する（D7）。

| 表 | 1 項目 | 内容 |
| --- | --- | --- |
| `general_category_table` | 2 単位: `(start >> 16) \| (cat << 5)`、`start & 0xFFFF` | 0 から `10FFFF` を隙間なく覆う区間の開始。cat は `Lu`=0, `Ll`, `Lt`, `Lm`, `Lo`, `Mn`, `Mc`, `Me`, `Nd`, `Nl`, `No`, `Pc`, `Pd`, `Ps`, `Pe`, `Pi`, `Pf`, `Po`, `Sm`, `Sc`, `Sk`, `So`, `Zs`, `Zl`, `Zp`, `Cc`, `Cf`, `Cs`, `Co`, `Cn`=29 |
| `alphabetic_table`・`white_space_table`・`join_control_table` | 4 単位: lo の上位・下位、hi の上位・下位 | 閉区間、昇順・連結済み |
| `case_folding_table` | 4 単位: c の上位・下位、f の上位・下位 | `(c, scf(c))`、c の昇順 |

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 生成 | `scripts/generate-unicode.mjs`（新規） | - | UCD 4 ファイルを読み `std/Unicode.tz` を書く（D7）。入力の SHA-256 を検査する |
| std | `std/Unicode.tz`（新規・生成物） | `version`, `property_ranges`, `simple_case_folding`、private の表 | 生成物。手で編集しない |
| std | `std/Regex.tz`（新規） | `Regex`, `Error`, `ErrorKind`, 公開 API、private の `parse`・`fold_class`・`emit`・`add_thread`・`step`・`search16`・`search8` | 本体 |
| std の登録 | `src/stdlib.rs` | `SOURCES` | `("std/Regex.tz", include_str!("../std/Regex.tz"))` を `Parallel` と `Result` の間、`("std/Unicode.tz", …)` を `Test` と `Utf8String` の間 |
| std の登録 | `src/stdlib.rs` | `RESERVED_MODULES` | `"Regex"`, `"Unicode"`（D1） |
| std の登録 | `src/stdlib.rs` | `opaque_record` | `"Regex.Regex"`（`Threads` は private なので不要） |
| その他 | `src/check.rs`, `src/llvm.rs`, 字句・構文・整形・LSP・`vsc/` | `opaque_record` の 2 呼び出し元ほか | 変更なし（予約語・構文を足さない）。変更が要れば停止条件 |

### 生成 IR とランタイム

- 新しい runtime 関数・`%tz.*` 型・header はない。`Regex.*`・`Unicode.*` は通常の std 関数として、到達したものだけが
  `@tz.fn.Regex.<name>`・`@tz.fn.Unicode.<name>` で出る。表は `Unicode.*` に到達したときだけ `@tz.literal.N` の定数になり、評価ごとに
  `@tz.string.new` で複製される（`compile` と `Unicode.*` の呼び出しあたり 1 回）。
- 照合の内側（`add_thread`・`step`）は確保しない。`Vec.push` は再確保の経路を IR に出すので使わず、事前に確保した `Vec` を `Vec.set` で更新する。
  E2E の `inspect` で `@tz.fn.Regex.add_thread`・`@tz.fn.Regex.step` の本体に `@tz.alloc`・`@tz.realloc` がないことを検査する。

### アルゴリズム

入力の復号（`search16`・`search8`。探索の本体は共通の `step`・`add_thread`）:

- string: 位置 p の単位 u が上位サロゲートで次が下位サロゲートなら対のスカラー（幅 2）、それ以外は u 自身（幅 1、孤立サロゲートを含む）。
  直前のスカラーは p-1 が下位で p-2 が上位なら対。utf8string: `Utf8String.decode_at` と `Utf8Char.to_u32`、直前は継続 byte を最大 3 個戻る。
- `text[index]`（`i16u`）は `as i64` で広げる。

```text
search(re, text, start, nslots, first_only) -> (matched, slots, steps)
  clist, nlist = Threads(命令数, nslots); best = [-1] * nslots; matched = false; pos = start
  loop
    if not matched and (pos == start or not re.anchored):
      add_thread(clist, 0, fresh(-1), pos)            # 開始 thread は最後（最低優先）に足す
    if clist.count == 0: break
    (c, width) = scalar_at(pos)                        # 末尾は c = -1
    for t in clist（優先度順）:
      steps += 1
      match op(t.pc):
        MATCH -> matched = true; best = t.slots; if first_only: return; break   # 低優先の thread を捨てる
        CHAR a -> if c == a: add_thread(nlist, t.pc + 1, t.slots, pos + width)
        CLASS k -> if c >= 0 and in_class(k, c): 同上
        ANY -> if c >= 0: 同上;  ANY_NO_NL -> if c >= 0 and c != 10: 同上
    if c < 0: break
    swap(clist, nlist); nlist.count = 0; pos = pos + width
  return (matched, best, steps)

add_thread(list, pc, slots, pos)                       # 再帰しない。stack は 2 × 命令数 + 1 で足りる
  push Explore(pc)
  while stack not empty:
    pop frame
    Restore(k, old) -> slots[k] = old
    Explore(pc) -> if list.contains(pc): continue
                   list.insert(pc); steps += 1
                   JMP a -> push Explore(a)
                   SPLIT a b -> push Explore(b); push Explore(a)
                   SAVE k -> if k < nslots: push Restore(k, slots[k]); slots[k] = pos
                             push Explore(pc + 1)
                   ASSERT k b -> if holds(k, prev(pos), next(pos)): push Explore(pc + 1)
                   それ以外 -> list.slots[pc * nslots ..] = slots
```

`is_match` は nslots = 0・first_only、`find`・`find_at` は 2、`captures` は 2 ×（group 数 + 1）。`find_all` は次の開始を
「空でない一致なら end、空の一致なら end の次のスカラー境界」にする（D5）。`split` は一致の間の部分文字列と最後の残り。

コンパイル（`emit`、構文木の深さは入れ子上限で抑える `def rec`）: 選択は `SPLIT L1, next; L1: c1; JMP end; …`。`x*` は
`L: SPLIT body, out; body: x; JMP L`（lazy は SPLIT の a と b を入れ替え）。`x{n,}` は x を n-1 回出してから `L: x; SPLIT L, out`
（n = 0 なら `x*`）。`x{n,m}` は x を n 回と、共通の out へ抜ける SPLIT 付きの x を m-n 回。命令を足すたびに 10,000 を検査する。
空に一致する本体の繰り返し（`(a*)*`）は `add_thread` の重複除去で止まる。

大文字小文字の畳み込み（`fold_class`、D9）: P = `Unicode.simple_case_folding()`（`-u` では A-Z と a-z の 26 対）。
(1) P の各 (c, f) について c か f が class に含まれれば f を targets に足す。(2) targets を整列・重複除去する。(3) P の各 (c, f) で f が targets に
あれば c を足し、targets 自身も足して正規化する。否定の class は畳み込んでから補集合を取る（`[0, 10FFFF]` に対して。サロゲートの範囲を含む）。
`i` の下の字義どおりの文字は、畳み込み後の区間が 2 個以上なら CLASS にする。`\w`・`\d`・`\s`・各 property の class は
（種類, 否定, i, u）ごとに一度だけ作り、id を使い回す（繰り返しで区間の合計が増えないように）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。Node の E2E の前には毎回 `cargo build --release --locked` を実行する。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、`Regex` を使わない fixture の IR と `check` 時間を保存する。
- 確認: 次がすべて成功し、4 つのテストはそれぞれ `1 passed`、Node は `17.0` を出す。`check` の 3 回の `real` を記録する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
node -p process.versions.unicode
mkdir -p /tmp/tz-d09 && cp -R tests/fixtures/strings /tmp/tz-d09/base
target/release/tsuzuri build /tmp/tz-d09/base --emit llvm -o /tmp/tz-d09/before.ll
target/release/tsuzuri build /tmp/tz-d09/base --emit llvm -O3 -o /tmp/tz-d09/before-O3.ll
for i in 1 2 3; do /usr/bin/time -p target/release/tsuzuri check /tmp/tz-d09/base; done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
```

### 手順 2: モジュール名の予約（D1）

- 変更: `src/stdlib.rs` の `RESERVED_MODULES`、`tests/regex.rs`（新規）。
- 内容: `"Regex"`・`"Unicode"` を足す。`tests/regex.rs` に `regex_and_unicode_are_reserved_std_modules` を作り、`tests/stdlib.rs` の
  `rejects(&sources, &[], "E1011", "reserved for the standard library")` の形で両名を検査する（補助関数は写す）。
- 確認: `cargo test --locked --test regex` が `1 passed`。`cargo test --locked --lib stdlib` と `cargo test --locked --test stdlib` が成功する。

### 手順 3: UCD の取得と生成器（D6・D7）

- 変更: `scripts/generate-unicode.mjs`（新規）、`std/Unicode.tz`（新規・生成物）、`src/stdlib.rs` の `SOURCES`。
- 内容: 生成器は引数の UCD ディレクトリから `extracted/DerivedGeneralCategory.txt`・`DerivedCoreProperties.txt`（`Alphabetic`）・
  `PropList.txt`（`White_Space`・`Join_Control`）・`CaseFolding.txt`（状態 C・S）を読む。検査: 入力の SHA-256 が `EXPECTED_SHA256`
  （初回の実行で表示した値を貼って commit する）と一致、一般カテゴリーが 0〜`10FFFF` を隙間なく覆う、各表が昇順・連結済み、
  `scf(scf(c)) == scf(c)`。出力は「データ構造」の符号化で、先頭行は
  `// Generated by scripts/generate-unicode.mjs from Unicode 17.0.0 UCD. Do not edit.`。公開 3 関数と private の復号関数の本文も生成器が書く。
- 確認: 2 回生成して `shasum` が一致し、`wc -c std/Unicode.tz` が 131,072 以下。`cargo build --release --locked` の後、手順 1 の
  `check` 時間の 3 回の中央値が 20% 未満の増加。`cargo test --locked --lib stdlib` が成功する。

```sh
mkdir -p target/ucd/17.0.0/extracted
for f in extracted/DerivedGeneralCategory.txt DerivedCoreProperties.txt PropList.txt CaseFolding.txt; do
  curl -fsSL -o "target/ucd/17.0.0/$f" "https://www.unicode.org/Public/17.0.0/ucd/$f"
done
node scripts/generate-unicode.mjs target/ucd/17.0.0 && shasum std/Unicode.tz
node scripts/generate-unicode.mjs target/ucd/17.0.0 && shasum std/Unicode.tz && wc -c std/Unicode.tz
```

### 手順 4: E2E の骨組みと Unicode の表の照合

- 変更: `tests/regex-cases.mjs`（新規）、`tests/fixtures/regex/Main.tz`（新規）、`tests/fixtures/regex/Cases.tz`（新規・生成物）、
  `tests/features.mjs` の `suites.regex`（新規）。
- 内容: `tests/regex-cases.mjs` が case 表の唯一の源で、`node tests/regex-cases.mjs --write` が `Cases.tz`（`pattern`・`input`・
  `replacement`・`property_name` を `match which with` で返す private でない def）を書く。`suites.regex` は再生成した文字列と
  commit 済みの `Cases.tz` を比べ、違えば `run node tests/regex-cases.mjs --write` で失敗する。この手順では `property_hash which` と
  `fold_orbit_hash` だけを足す（参照は「E2E」の V8 による計算）。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri regex` が `regex: native/WASM at O0/O3 passed`。

### 手順 5: パターンの解析と `Regex.Error`

- 変更: `std/Regex.tz`（新規）、`src/stdlib.rs` の `SOURCES`・`opaque_record`、`tests/regex.rs`、`tests/regex-cases.mjs`。
- 内容: `ErrorKind`・`Error`・`Regex`・`Node`、明示的な stack による解析（再帰しない）、flag、class の正規化、「診断」と「資源上限」の全 message。
  この時点の `compile` は解析だけで、命令を空にした `Regex` を返してよい。`tests/regex.rs` に `regex_values_are_opaque_noncopy_owned` を足す。
- 確認: `cargo test --locked --test regex` が `2 passed`。E2E の `compile_result` の全 case が成功する。

### 手順 6: 命令の生成と上限

- 変更: `std/Regex.tz` の `emit`。
- 内容: 「アルゴリズム」のコンパイル規則、命令数・区間の合計・捕捉の作業領域の上限、`anchored`、`group_count`。
- 確認: E2E の `compile_result`（`a{1001}`、`(?:a{1000}){1000}`、入れ子 65 段、`(){1000}` 系）が成功する。

### 手順 7: Pike VM（string、`is_match`・`find`・`find_at`・`search_steps`）

- 変更: `std/Regex.tz` の `Threads`・`add_thread`・`step`・`search16` と公開 4 関数。
- 内容: 「アルゴリズム」どおり。`add_thread` は明示的な stack、照合中は `Vec.set` だけ。
- 確認: E2E の `is_match16`・`find16`・`find_at16`・`linear` が成功する。`suites.regex.inspect` の無確保の検査が成功する。

### 手順 8: 捕捉・反復・置換・分割・escape

- 変更: `std/Regex.tz` の `captures`・`find_all`・`replace_all`・`split`・`escape`。
- 内容: D5 の規則。部分文字列は `String.slice` で作る（境界は常にスカラー境界なので `Some`）。
- 確認: E2E の `captures16`・`find_all16`・`replace16`・`split16`・`escape16` が成功する。

### 手順 9: 大文字小文字と Unicode の class

- 変更: `std/Regex.tz` の `fold_class` と class の memo。
- 内容: D9。`Unicode.property_ranges` と `Unicode.simple_case_folding` は `compile` ごとに高々一度ずつ呼ぶ。
- 確認: E2E の `i`・`\p{...}`・`\w`・`\b`・`-u` の case が成功する。

### 手順 10: utf8string 版

- 変更: `std/Regex.tz` の `search8` と `_utf8` の 7 関数。
- 内容: 復号だけが異なり、`step`・`add_thread` を共有する。
- 確認: E2E の `find8`・`find_all8`・`captures8`・`replace8`・`split8` が成功する。

### 手順 11: 型の検査と IR の不変

- 変更: `tests/regex.rs`。
- 内容: 「Rust テスト」の残り 4 テストを足す。手順 1 の IR と byte 単位で比べる。
- 確認: `cargo test --locked --test regex` が `6 passed`。次の `cmp` が何も出さない。手順 1 の 4 テストが成功する。

```sh
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-d09/base --emit llvm -o /tmp/tz-d09/after.ll && cmp /tmp/tz-d09/before.ll /tmp/tz-d09/after.ll
target/release/tsuzuri build /tmp/tz-d09/base --emit llvm -O3 -o /tmp/tz-d09/after-O3.ll && cmp /tmp/tz-d09/before-O3.ll /tmp/tz-d09/after-O3.ll
```

### 手順 12: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `./target/release/tsuzuri doc std -o _docs/library-reference/api` の後、`node scripts/check-docs.mjs _docs/library-reference/regex.md _docs/library-reference/README.md _docs/library-reference/text.md _docs/feature-status.md`
  が成功する。

### 手順 13: 最終確認

- 確認: `cargo test --locked` が成功し、`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri` の全 suite が
  `native/WASM at O0/O3 passed`。`target/release/tsuzuri fmt --check tests/fixtures/regex` が成功する。`git status` に予期しないファイルがない。

## テスト計画

### Rust テスト

`tests/regex.rs`（新規）。補助関数は `tests/polymorphism.rs` の `accepts`・`rejects` を写す。

| テスト | 検査すること |
| --- | --- |
| `regex_and_unicode_are_reserved_std_modules` | 利用者の `Regex.tz`・`Unicode.tz` が E1011 |
| `regex_values_are_opaque_noncopy_owned` | `re.code` の読み取りと `Regex { … }` の構築が E1022、move 後の再使用が E1012、`ref re` を 2 回渡すのは受理 |
| `public_api_signatures_type_check` | 「型規則」の全関数を宣言どおりの型で呼ぶ program が受理される。`Regex.find` に utf8string の借用を渡した code が、`String.find` に utf8string の借用を渡した code と等しい |
| `unused_regex_emits_nothing` | `Regex` を使わない program の IR に `@tz.fn.Regex.`・`@tz.fn.Unicode.` がない |
| `regex_ir_is_deterministic` | 全 API を使う program の IR を 2 回出して一致し、`declare` に新しい外部関数がない |
| `generated_unicode_source_is_within_budget` | `include_str!("../std/Unicode.tz")` が 131,072 bytes 以下で、先頭行が生成器の header |

### E2E

`tests/features.mjs` の `suites.regex`（新規。GUIDE §7.4）。fixture `tests/fixtures/regex/`。全 export は native・WASM × `-O0`・`-O3` で
呼ばれ、native は各呼び出しの後 `live == 0`、WASM は import なし（harness の既存検査）。`traps` は空。結果は
`mix h v = (h * 31 + v + 1) % 1000000007` の畳み込み（i64 で溢れない）か `start * 1000000 + end`（なし は -1）で返す。

| export | 参照（Node、`u` フラグ） |
| --- | --- |
| `compile_result which` | 仕様の表から手で書いた `kind * 1000000 + offset`（成功は -1）。Syntax の case は `new RegExp(js, "u")` も例外を投げることを確かめる |
| `is_match16`・`find16`・`find_at16 which start` | `RegExp.prototype.exec`（`lastIndex` と `y` なし）の `index` と `[0].length` |
| `captures16` | `d` フラグの `indices`。未定義は `(-1, -1)`。反復の内側の group を持つ case は含めない |
| `find_all16`・`split16`・`replace16` | `g` フラグの `matchAll`（空一致の後は 1 コードポイント進む＝D5）、分割は一致の範囲から切り出す、置換は関数 replacer で `${n}`・`$$` を展開 |
| `find8`・`find_all8`・`captures8`・`replace8`・`split8` | 同じ計算の範囲を `Buffer.byteLength(prefix, "utf8")` で byte に直す。孤立サロゲートを含む入力は使わない |
| `escape16` | `Regex.escape` の結果を `new RegExp(result, "u")` で `input` 全体に一致させた真偽 |
| `property_hash which` | 全スカラーを並べた文字列への `/\p{X}+/gu` の一致区間（サロゲート 2,048 個は 1 個ずつ `test`） |
| `fold_orbit_hash` | U（`toLowerCase`・`toUpperCase` が変える文字と表の全文字）の各 c について、U を並べた文字列に `[c]`（`giu`）が一致する集合を軌道とし、軌道の列を畳み込む |
| `linear which` | 期待は 1。fixture が `unit` を 1,000・2,000・4,000 回繰り返し `suffix` を付けた入力の `search_steps` s1・s2・s4 を求め、`s4 - s2 == 2 * (s2 - s1)` かつ `s1 > 0` なら 1 |
| `message_hash which` | 「診断」の表の英語 message をそのまま畳み込む |

case の最低限: 「例」の表の全行。`ab|a`・`(a|ab)(c|bcd)` を `"abcd"`（`(0, 4)`）、`^`・`$`・`\A`・`\z` と `m`（`\r` と U+2028 を含む入力）、
`s` の有無の `.`、lazy の 4 種、`{n}`・`{n,}`・`{n,m}` の境界（0、1、1000）、空パターン、`a|`、`()`、class の `-` の先頭・末尾、
`[^a]` と孤立サロゲート、`\u{10FFFF}`、`\uD83D\uDE00` の結合、U+1E9E・U+212A・U+03C2・U+0345 の `i`、`find_at` の範囲外とサロゲート対の間。
`linear` のパターン: `(a*)*b`、`(a|a)*b`、`(a|aa)*c`、`(x+x+)+y`、`(.*a){20}`、`^(a+)+$`（suffix `!`）、`(?i)(ß|ss)*x`。

JS の構文・意味の差は case 表の `js` 欄で手書きの同値なパターンに直す: `\d` は `\p{Nd}`、`\w` は `[\p{Alphabetic}\p{M}\p{Nd}\p{Pc}\p{Join_Control}]`、
`\s` は `\p{White_Space}`、`\b` はその class の lookbehind・lookahead、`.` は `[^\n]`、`m` の `^`・`$` は `(?<![^\n])`・`(?![^\n])`、
`\A`・`\z` は `(?<![\s\S])`・`(?![\s\S])`、`-u` は ASCII の class。`i` と否定（`[^…]` 以外の `\P`・`\W`・`\D`・`\S`・`\B`）の組は JS が
補集合を畳み込むので参照 case にしない。反復内の group の捕捉は手計算の期待を別表に置く: `(?:(a)|b)+` を `"ab"` に対して group 1 は `(0, 1)`
（Python の `re` と同じ。JS は未定義）、`(a*)*` を `"b"` に対して group 1 は `(-1, -1)`（「アルゴリズム」の重複除去から導く）。

### 既存テストへの影響

なし。`embedded_sources_use_reserved_flat_modules` と `std_definitions_never_shadow_builtin_functions` は期待値を変えずに新モジュールも検査する。
std のモジュール一覧を固定で比べる既存テストが見つかった場合は、その一覧に `Regex`・`Unicode` を足すことだけを許す。それ以外は停止条件。

### 性能

時間の閾値は置かない。線形性は `linear` の手数で検査する。完了報告に次を記録する（文書には書かない）: 手順 1 と手順 3 の `check` 時間、
`std/Unicode.tz` の bytes、`Regex.is_match` だけを使う fixture の native `-O3` と wasm32 の出力の大きさ。他言語の regex との速度比較は Phase 1 では主張しない。

## ドキュメント

- `docs/language.md`: `### Map / Set` の後に `### Regex`（新規）。構文の要約、leftmost-first、D5 の反復規則、捕捉、上限、線形時間と
  `find_all` の最悪、Unicode 17.0.0 と `\d`・`\w`・`\s` の定義、`Unicode` の 3 関数。`### string と utf8string` から参照を張る。
- `_docs/library-reference/regex.md`（新規）: 使い方、構文表、JS・Rust・.NET との差（`\w` の定義、`$` と改行、反復内の捕捉、非対応の構文）、
  上限と `Regex.Error` の表。
- `_docs/library-reference/README.md`: 「ソース宣言の API 一覧」の表に `| Regex | [Regex](api/Regex.md) | [正規表現](regex.md) |` と
  `| Unicode | [Unicode](api/Unicode.md) | [正規表現](regex.md) |`。`_docs/library-reference/api/` は `tsuzuri doc std` で再生成する。
- `_docs/library-reference/text.md` の `## API と関連項目` に regex.md への link。
- `_docs/feature-status.md` の D09 行: 「Phase 1 実装済み: 線形時間の正規表現（Pike VM）、Unicode 17.0.0 の一般カテゴリー・単純畳み込み。
  正規化と書記素は未着手（Phase 2）」。未対応の一覧の「一般的な Unicode 正規化」は残す。
- `_features/README.md` の D09 の状態、`_features/GUIDE.md` の D-07 の表に `Regex`／`Unicode`（D09）を足し、D-30 の表から該当行を消す（D1 の承認後）。

## 受け入れ条件

- [ ] 「型規則」の全 API が std にあり、`Regex` は不透明・非 Copy。
- [ ] 構文・意味・上限・`Regex.Error` の message が仕様の表どおりで、上限超過はトラップしない。
- [ ] `suites.regex` が native・WASM × `-O0`・`-O3` で成功し、`live == 0`、WASM の import なし、`linear` の全 case が 1。
- [ ] Unicode の表が V8 の `\p{...}`・`iu` と全スカラーで一致し、`std/Unicode.tz` が生成器から byte 単位で再現でき、131,072 bytes 以下。
- [ ] `Regex` を使わない program の IR が手順 1 と一致する。照合の内側の関数に確保がない。
- [ ] `tests/regex.rs` の 6 テストと stack-depth の 3 テスト、`honors_the_exact_specialization_limit` が成功する。
- [ ] 「ドキュメント」の更新が済み、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- JS との差（`\d`・`\w`・`\b` が ASCII、`m` の行末に `\r`・U+2028・U+2029、反復ごとに内側の group を消す）は「E2E」の `js` 欄と
  除外規則で扱う。直さないと Tsuzuri 側が正しくても失敗する。直した後も group 0 が違えば停止条件。
- 表を持つ文字列リテラルは評価のたびに確保・複製される。`Unicode.*` を照合の中や class ごとに呼ばない（`compile` ごとに一度、class は memo）。
- `add_thread` を再帰で書くと、JMP の長い鎖（`a{1000}` など）で 2 MiB の stack を超えうる。明示的な stack にする。
- 開始 thread を運んできた thread より先に足すと leftmost-first が壊れる（`ab|b` を `"ab"` に対して `(1, 2)` を返す。正しくは `(0, 2)`）。
- 空一致の後に 1 コード単位だけ進めると、string では孤立サロゲートの断片を作り、utf8string では `Utf8String.slice` が `None` になる。スカラー単位で進める。
- `\uD800` などの孤立サロゲートは string の入力にだけ現れる。utf8string 版の参照 case に入れない。`Cases.tz` の生成器は `"`・`\` と
  孤立サロゲートを escape する。
- std の private の補助関数を型変数で汎用にすると、利用者の特殊化の予算（1,024）を消費する。`Regex.tz` の関数は単相にする。
- `offset` の単位は、パターンの誤りは UTF-16 コード単位、照合結果は入力の型の単位。utf8string 版で混同しない。

## 対象外

- 後方参照、lookaround、名前付き group、atomic group・所有的量指定子、`\X`・`\R`、class の集合演算（JS の `v` フラグ）。
- 遅延 DFA、one-pass、リテラルの前置フィルタなどの高速化（Phase 1 は Pike VM だけ。D3）。
- 正規表現リテラルの専用構文、locale 依存の照合、置換の callback、`Seq` による遅延の反復、不正な UTF-8 の byte 照合。
- Phase 2 の全項目（正規化、書記素、単語境界、完全な大小変換、1 文字ごとの分類）。他言語との速度比較。

## 決定事項

### D1: std モジュール名 `Regex`・`Unicode` の予約

- 決定: `RESERVED_MODULES` に `Regex`・`Unicode` を足し、GUIDE D-30 の仮割り当てを D-07 へ移す。利用者の同名モジュールは E1011。
- 理由: std のモジュールは予約が必須（`embedded_sources_use_reserved_flat_modules`）。HEAD では利用者の `Regex.tz` が通るので、互換性に影響する。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: 実装の置き場所

- 決定: Phase 1 は全部を std の Tsuzuri（`std/Regex.tz`・`std/Unicode.tz`）で書く。C runtime・`.ll`・組み込み関数は足さない。
- 理由: 所有権と境界検査を compiler が保証し、生ポインターがない。IR は std のソースから決定的に出て、`generate.py` の Clang 版の制約を受けず、
  WASM の import も増えない。未使用なら D-07 の到達可能性で消える。既存案の「速度が不足する部分だけ C へ移す」は計測の後の別チケットにする。
- 状態: 既定案（実装者はこの案に従う）

### D3: エンジン

- 決定: Pike VM（捕捉付きの Thompson NFA の同時実行）だけ。遅延 DFA は作らない。`Regex` は compile 後に不変。
- 理由: 1 回の探索が O(命令数 × 入力長) で、ReDoS が構造的に起きない。遅延 DFA のキャッシュは共有借用の `Regex` を照合中に変更する必要があり、
  Tsuzuri には内部可変性がない。照合ごとに作ると線形時間の利点が消える。
- 状態: 既定案（実装者はこの案に従う）

### D4: 構文の範囲

- 決定: 「構文」の EBNF。JS の `u` フラグ付き正規表現と書き方をそろえ、曖昧な字義（`{`・`]`・未知の escape）は誤りにする。flag の群は先頭だけ。
- 理由: 参照を V8 で計算でき、利用者が既知の構文で書ける。誤りにした構文は後から意味を足しても互換性を壊さない。
- 状態: 既定案（実装者はこの案に従う）

### D5: 一致の意味

- 決定: leftmost-first（RE2・Rust の `regex`・Perl と同じ優先順位）。反復内の group は最後に参加した反復の値。`find_all` の次の開始は、
  空でない一致なら end、空の一致なら end の次のスカラー境界（JS・Python・.NET と同じ。Rust とは `a*` と `"abc"` で異なる）。`split` は一致の間。
- 理由: 優先順位は Pike VM で自然に得られ、JS の結果と group 0 が一致する。空一致の規則は主要 3 言語と同じで、参照を V8 で計算できる。
- 状態: 既定案（実装者はこの案に従う）

### D6: Unicode の版

- 決定: Unicode 17.0.0 に固定する。参照用の Node は `process.versions.unicode` が `17.0` のもの（HEAD の環境の v20.19.6）。版の更新は
  結果が変わる非互換変更なので、G19 の D8（std は全 edition で共有）に従い人間の承認を得て行い、生成器の `EXPECTED_SHA256` と
  `Unicode.version` を同時に変える。
- 理由: 表の照合の参照（V8 の ICU）と版が同じでないと全スカラーの比較ができない。HEAD の Node が 17.0 を報告する。
- 状態: 既定案（実装者はこの案に従う）

### D7: 表の表現・生成・大きさ

- 決定: `scripts/generate-unicode.mjs` が UCD 4 ファイルから `std/Unicode.tz` 全体を生成し、commit する。表は `\uXXXX` の UTF-16 文字列リテラル
  （「データ構造」の符号化）。上限 131,072 bytes（見積もり約 95 KB: 一般カテゴリー約 41 KB、`Alphabetic` 約 18 KB、畳み込み約 36 KB）。
  UCD のファイルは commit しない（`target/ucd/17.0.0/`）。
- 理由: static データがない HEAD で、文字列リテラルだけが 1 個の定数と 1 回の複製で済む表現である。`match` の arm は全プログラムの型検査を遅くする。
  入力の SHA-256 で再生成の再現性を保つ。
- 状態: 既定案（実装者はこの案に従う）

### D8: API の名前と符号化

- 決定: 「型規則」のとおり。string 版は無印、utf8string 版は `_utf8`。パターンは `ref string` だけ。反復は配列を返す。誤りは `Regex.Error`
  （`IO.Error` と同じく module 内の型名）。
- 理由: `String`・`Utf8String` の引数順（対象のテキストが最後）に合わせる。`Seq` はテキストと `Regex` の借用を閉包に持つ必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D9: 大文字小文字を区別しない照合

- 決定: Unicode の単純畳み込み（`CaseFolding.txt` の C と S）の同値類で比べる。class は畳み込んでから否定する（Rust と同じ）。`-u` では ASCII の 26 対。
- 理由: JS の `iu` と同じ同値関係で参照を計算できる。完全畳み込み（`ß` と `ss`）は長さが変わり、線形の VM の 1 スカラー 1 手を崩す。
- 状態: 既定案（実装者はこの案に従う）

### D10: 資源上限

- 決定: 「資源上限」の 6 項目。超過は `TooLarge` を返し、トラップしない。
- 理由: 信頼できないパターンを受ける用途で、プロセスを止めずに拒否できる。捕捉の作業領域の積の上限で、1 回の `captures` の確保を 4 MiB に抑える。
- 状態: 既定案（実装者はこの案に従う）

### D11: Phase 2 の着手と表の置き場所

- 決定: 正規化・書記素・単語境界・完全な大小変換・1 文字ごとの分類は Phase 2 とし、表の置き場所（チケット D11 の Phase 2 の static データか runtime の表）が
  決まるまで着手しない。
- 理由: 1 文字ごとの API を HEAD の文字列リテラルで作ると呼び出しごとに表を複製する。表の置き場所は D-28 の変更か、新しい runtime 基盤になる。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D12: 線形時間の試験

- 決定: `Regex.search_steps` を公開し、E2E で手数の等式 `s4 - s2 == 2 * (s2 - s1)` を検査する。時間は測らない。
- 理由: 共有 CI で時間の閾値は不安定（AGENTS.md）。手数は native と WASM で同じで、線形でない実装は等式を満たさない。
- 状態: 既定案（実装者はこの案に従う）
