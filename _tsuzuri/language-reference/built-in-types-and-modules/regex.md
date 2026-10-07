# Regex

`Regex` は、`string`（UTF-16）と `utf8string`（UTF-8）を正規表現で検索・置換・分割する標準モジュールです。エンジンは Pike VM（捕捉付きの Thompson NFA を同時に走らせる方式）で、1 回の探索の時間は「命令数 × 入力のスカラー数」に比例します。後戻り（バックトラック）をしないので、パターンや入力を信頼できない場合でも、指数時間の照合（ReDoS）は構造的に起きません。

パターンは常に `string` で書き、`Regex.compile` で一度だけ `Regex` の値に変換します。同じ `Regex` を `string` にも `utf8string` にも使えます。

## この記事のポイント

- `Regex.compile (ref pattern)` は `Result<Regex, Regex.Error>` を返します。構文の誤り、未対応の構文、資源上限の超過はトラップせず `Result.Error` です。
- 一致は **leftmost-first**（Rust の `regex`、RE2、Perl と同じ優先順位）です。`a|ab` は `"ab"` の `(0, 1)` に一致し、POSIX の最長一致ではありません。
- 範囲 `(start, end)` は半開区間で、`string` では UTF-16 コード単位、`utf8string` では byte です。常にスカラーの境界にあります。
- `.`、文字、クラスは Unicode スカラー 1 個に一致します。サロゲートペアは 1 スカラー、`string` の孤立サロゲートも 1 スカラーです。
- `\d`、`\w`、`\s`、`\b` と大文字小文字の畳み込みは既定で Unicode（Unicode 17.0.0）です。`(?-u)` で ASCII だけにできます。
- `Regex` は不透明で非 Copy の値です。照合では変化しないので、共有借用で何度でも使えます。
- 後方参照、先読み・後読み、名前付き group、atomic group はありません（線形時間を保証するため）。

## コンパイルと検索

`compile` で得た `Regex` を共有借用で渡します。`find` は最初の一致の範囲を、`is_match` は一致の有無を返します。

```tsuzuri run=found%206..8
let pattern = "\\d+"
let text = "order 66 shipped"
match Regex.compile (ref pattern) with
| Result.Ok re ->
    match Regex.find (ref re) (ref text) with
    | Maybe.Some (start, stop) -> $"found {start}..{stop}"
    | Maybe.None -> "none"
| Result.Error error -> $"error {error.kind} at {error.offset}"
```

実行結果:

```text
found 6..8
```

`find_at re text start` は `start` から探します。`start` より前の文字も `^` と `\b` の判定に使うので、本文の途中から探しても単語境界の意味は変わりません。`start` が範囲外（負、長さ超）か、スカラーの内部（サロゲートペアの間、UTF-8 の継続 byte）なら `None` です。

## 捕捉 group と置換

`(...)` は捕捉 group、`(?:...)` は捕捉しない group です。`captures` は最初の一致について、要素 0 に全体の範囲、要素 k に k 番目の group の範囲を並べた配列を返します。一致に参加しなかった group は `(-1, -1)` です。配列の長さは `group_count re + 1` です。

`replace_all` の置換文字列では、`${n}`（10 進）が group n の文字列、`$$` が `$` 1 文字になります。参加しなかった group と、存在しない番号の group は空文字列です。それ以外の `$` はそのまま出力されます。

```tsuzuri run=replaced%3Dmail%20example%3Aalice%20and%20test%3Abob%20groups%3D%5B%285%2C%2018%29%2C%20%285%2C%2010%29%2C%20%2811%2C%2018%29%5D
let pattern = "(\\w+)@(\\w+)"
let re = Result.get (Regex.compile (ref pattern))
let text = "mail alice@example and bob@test"
let template = "${2}:${1}"
let replaced = Regex.replace_all (ref re) (ref text) (ref template)
let groups = Maybe.get (Regex.captures (ref re) (ref text))
$"replaced={replaced} groups={groups}"
```

実行結果:

```text
replaced=mail example:alice and test:bob groups=[(5, 18), (5, 10), (11, 18)]
```

繰り返しの中の group は、最後に参加した繰り返しの範囲を保ちます。`(?:(a)|b)+` を `"ab"` に照合すると group 1 は `(0, 1)` です（Python の `re` と同じで、繰り返しごとに捕捉を消す JavaScript とは異なります）。

## すべての一致と分割

`find_all` は重ならない一致を左から順に返します。次の探索は、空でない一致ならその終わりから、空の一致なら終わりの次のスカラーから始まります（JavaScript、Python、.NET と同じです。Rust の `regex` は空の一致の直後の空の一致を返さないので、`a*` と `"abc"` で結果が異なります）。`split` は一致と一致の間の部分文字列を、先頭と末尾も含めて返します。

```tsuzuri run=spans%3D%5B%280%2C%201%29%2C%20%281%2C%201%29%2C%20%282%2C%202%29%2C%20%283%2C%203%29%5D%20parts%3D%5B%22a%22%2C%20%22b%22%2C%20%22c%22%5D
let empty = Result.get (Regex.compile (ref "a*"))
let spans = Regex.find_all (ref empty) (ref "abc")
let comma = Result.get (Regex.compile (ref ",\\s*"))
let parts = Regex.split (ref comma) (ref "a, b,c")
$"spans={spans} parts={parts}"
```

実行結果:

```text
spans=[(0, 1), (1, 1), (2, 2), (3, 3)] parts=["a", "b", "c"]
```

## utf8string

`utf8string` には名前に `_utf8` を付けた関数を使います。引数の順番と意味は `string` 版と同じで、範囲は byte で数えます。パターンは `string` のままです。

```tsuzuri run=bytes%3D%284%2C%205%29%20units%3D%282%2C%203%29
let re = Result.get (Regex.compile (ref "[a-z]"))
let bytes = match Regex.find_utf8 (ref re) (ref u8"😀a") with
    | Maybe.Some (start, stop) -> $"({start}, {stop})"
    | Maybe.None -> "none"
let units = match Regex.find (ref re) (ref "😀a") with
    | Maybe.Some (start, stop) -> $"({start}, {stop})"
    | Maybe.None -> "none"
$"bytes={bytes} units={units}"
```

実行結果:

```text
bytes=(4, 5) units=(2, 3)
```

## 構文

構文は JavaScript の `u` フラグ付き正規表現の部分集合で、同じ書き方をします。曖昧な書き方（エスケープしない `{`・`}`・`]`、未知のエスケープ）は誤りです。

| 構文 | 意味 |
| --- | --- |
| `x` | 構文文字 `^ $ \ . * + ? ( ) [ ] { } \|` 以外のスカラーはそれ自身 |
| `.` | `\n` 以外の任意のスカラー（`s` フラグでは `\n` も） |
| `[abc]`、`[a-z]`、`[^a]` | 文字クラス。`-` は先頭か末尾だけ字義どおり。`[]` と `[^]` は誤り |
| `\d`、`\D` | `Nd`（10 進数字）とその補集合。`(?-u)` では `[0-9]` |
| `\w`、`\W` | `Alphabetic`・`M`・`Nd`・`Pc`・`Join_Control` の和（UTS #18 附属書 C）とその補集合。`(?-u)` では `[0-9A-Za-z_]` |
| `\s`、`\S` | `White_Space` とその補集合。`(?-u)` では `[\t\n\v\f\r ]` |
| `\p{Name}`、`\P{Name}` | 一般カテゴリーの短い名前（`Lu` … `Cn` の 30 個）、群 `L`・`LC`・`M`・`N`・`P`・`S`・`Z`・`C`、`Alphabetic`・`White_Space`・`Join_Control` とその補集合 |
| `^`、`$` | 入力の先頭と末尾（`$` は末尾の `\n` の前には一致しない）。`m` フラグでは行頭と行末（`\n` の後と前） |
| `\A`、`\z` | 常に入力の先頭と末尾 |
| `\b`、`\B` | 前後のスカラーの `\w` 判定が異なる位置と同じ位置（範囲外は単語以外） |
| `(x)`、`(?:x)` | 捕捉する group と捕捉しない group |
| `(?flags:x)`、`(?flags)` | 範囲を限ったフラグと、パターンの先頭だけに置けるフラグ。`i`・`m`・`s`・`u`、`-` の後ろは無効にするフラグ |
| `x*`、`x+`、`x?` | 0 回以上、1 回以上、0 回か 1 回 |
| `x{n}`、`x{n,}`、`x{n,m}` | 回数の指定（`n`・`m` は 0〜1000） |
| `x*?` など | 量指定子の後ろの `?` は最短一致（lazy） |
| `\n`、`\r`、`\t`、`\f`、`\v`、`\0` | 制御文字 |
| `\x41`、`\u0041`、`\u{1F600}` | 符号位置。隣り合う上位・下位サロゲートの `\uXXXX` は 1 スカラーにまとまる |
| `\.`、`\-`、`\/` など | 構文文字と `-`・`/` のエスケープ |

フラグは `i`（大文字小文字を区別しない）、`m`（`^`・`$` が行頭・行末にも一致）、`s`（`.` が `\n` にも一致）、`u`（既定で有効。無効にすると `\d`・`\w`・`\s`・`\b` と畳み込みが ASCII だけ）です。`(?i)` のようにフラグだけの group はパターンの先頭にしか書けません。途中では `(?i:...)` を使います。

`Regex.escape text` は、構文文字 14 個の前に `\` を付けて、`text` に字義どおり一致するパターンを作ります。

### 大文字小文字を区別しない照合

`i` フラグでは、Unicode の単純な大文字小文字の畳み込み（`CaseFolding.txt` の状態 C と S）で同じ文字に畳み込まれる文字どうしが一致します。`k`・`K`・`K`（ケルビン記号 U+212A）は互いに一致し、`ß` と `ẞ`（U+1E9E）も一致します。長さが変わる完全な畳み込み（`ß` と `ss`）は行いません。否定のクラスは、畳み込んでから補集合を取ります（`(?i)[^k]` は `K` にも U+212A にも一致しません）。`(?-u)` では ASCII の 26 組だけを畳み込みます。

## エラー

`Regex.Error` は `kind`（`Regex.ErrorKind` の `Syntax`・`Unsupported`・`TooLarge`）、`offset`（パターンの UTF-16 コード単位の位置）、`message`（英語。直し方を含む）を持つ record です。誤りが複数あるときは、パターンの前から読んで最初の誤りを返します。

| kind | 条件 | message | offset |
| --- | --- | --- | --- |
| `Syntax` | `(` が閉じない | `unclosed group; add ')'` | `(` |
| `Syntax` | 対応のない `)` | `unmatched ')'; escape it as '\)'` | `)` |
| `Syntax` | `[` が閉じない／空の class | `unclosed character class; add ']'`／`empty character class; escape ']' as '\]'` | `[` |
| `Syntax` | 字義どおりの `{`・`}`・`]` | `unescaped 'X'; escape it as '\X'` | その文字 |
| `Syntax` | 繰り返す対象がない（先頭、`(` や `\|` の直後、表明の後、量指定子の後） | `repetition operator has nothing to repeat; add an expression before it or escape it` | 量指定子 |
| `Syntax` | `{` の形が不正、`n > m` | `invalid counted repetition; use {n}, {n,} or {n,m} with n <= m` | `{` |
| `Syntax` | class の範囲が逆順、端点が class、途中の `-` | `invalid class range; use single characters with start <= end` | 範囲の始点 |
| `Syntax` | 未知のエスケープ（class の中の `\b` を含む）、末尾の `\` | `unknown escape '\X'; escape only syntax characters`／`pattern ends with '\'; escape a backslash as '\\'` | `\` |
| `Syntax` | `\x`・`\u` の値が不正 | `invalid code point escape; use \u{0} through \u{10FFFF}` | `\` |
| `Syntax` | 未知のフラグ（フラグが要る位置の別の文字を含む） | `unknown flag 'X'; use i, m, s or u` | その文字 |
| `Unsupported` | `(?=`・`(?!`・`(?<=`・`(?<!`・`(?<name>`・`(?P<`・`(?>`・`(?#` | `unsupported group syntax; lookaround, named groups and atomic groups are not supported` | `(` |
| `Unsupported` | `\1`〜`\9`、`\k<`、`\0` の後の数字 | `backreferences are not supported; the engine guarantees linear time` | `\` |
| `Unsupported` | 先頭以外の `(?flags)` | `inline flags are allowed only at the start; use (?flags:...)` | `(` |
| `Unsupported` | 未対応の property（`\p` の後に `{...}` がない形を含む） | `unknown Unicode property 'X'; use a general category such as Lu, or Alphabetic, White_Space, Join_Control` | `\` |
| `Unsupported` | class 内の `[`・`&&`、所有的量指定子 `*+` など、`\G`・`\Z`・`\X`・`\R`・`\K` | `unsupported syntax 'X'` | その位置（所有的量指定子は量指定子の先頭） |

```tsuzuri run=Syntax%200%3A%20unclosed%20group%3B%20add%20%27%29%27
let pattern = "(a"
match Regex.compile (ref pattern) with
| Result.Ok _ -> "ok"
| Result.Error error -> $"{error.kind} {error.offset}: {error.message}"
```

実行結果:

```text
Syntax 0: unclosed group; add ')'
```

### 資源上限

上限を超えたパターンは `TooLarge` です。トラップはしません。

| 上限 | 値 | message | offset |
| --- | --- | --- | --- |
| パターンの長さ | 65,536 コード単位 | `pattern is longer than 65536 code units` | 0 |
| 繰り返し回数 `n`・`m` | 1,000 | `repetition count exceeds 1000` | `{` |
| group の入れ子 | 64 | `groups are nested deeper than 64` | 65 段目の `(` |
| 命令数 | 10,000 | `compiled program exceeds 10000 instructions; simplify the pattern or reduce counted repetitions` | 0 |
| class の区間の合計 | 65,536 | `character classes exceed 65536 ranges in total` | 0 |
| 命令数 × 2 ×（group 数 + 1） | 262,144 | `too many capture groups for the pattern size; use (?:...) for groups that are not read` | 0 |

命令数は `x{n,m}` で `x` を複製した後の数です。`\d`・`\w`・`\s`・`\p{...}` の class は同じフラグの下で 1 つを共有するので、繰り返し書いても区間の合計は増えません。`[...]` の class は書いた数だけ増えます。パターンの長さ、繰り返し回数、入れ子、class の区間の上限は構文の誤りと同じく前から読んだ順に、命令数と捕捉の上限は解析が終わってから検査します。

## 計算量

| 操作 | 時間 | 作業領域 |
| --- | --- | --- |
| `compile` | O(パターン長 + 命令数 + class の区間 × log) | O(命令数 + 区間) |
| `is_match`、`find`、`find_at`、`captures` | O(命令数 × 入力のスカラー数) | `captures` は 2 × 命令数 × (group 数 + 1) 個の `i64`（最大 4 MiB）、それ以外は O(命令数) |
| `find_all`、`replace_all`、`split` | 一致ごとに探索を繰り返すので最悪 O(命令数 × n²)（Rust の `regex` の反復と同じ） | O(命令数) と結果 |

作業領域は呼び出しの中で確保し、戻る前に解放します。照合の内側（`add_thread` と VM の 1 歩）は確保しません。速度の比較（他言語の正規表現との比較）はまだ測っていません。

## 他の正規表現との違い

| 項目 | Tsuzuri | JavaScript（`u`） | Rust `regex` | .NET |
| --- | --- | --- | --- | --- |
| 一致の優先順位 | leftmost-first | 後戻りの順（多くの場合同じ結果） | leftmost-first | 後戻りの順 |
| `\d`・`\w`・`\b` | Unicode（`(?-u)` で ASCII） | ASCII | Unicode | Unicode |
| `.` | `\n` 以外 | `\n`・`\r`・U+2028・U+2029 以外 | `\n` 以外 | `\n` 以外 |
| `m` の行末 | `\n` だけ | `\n`・`\r`・U+2028・U+2029 | `\n` だけ | `\n` だけ |
| `$`（`m` なし） | 末尾だけ | 末尾だけ | 末尾だけ | 末尾と末尾の `\n` の前 |
| 繰り返しの中の捕捉 | 最後に参加した値 | 繰り返しごとに消す | 最後に参加した値 | 最後に参加した値 |
| 後方参照・先読み・後読み | なし | あり | なし | あり |
| 空の一致の後の次の探索 | 次のスカラーから | 次のスカラーから | 直後の空一致を飛ばす | 次の位置から |

JavaScript は、最小回数を超えた繰り返しの本体が空文字列に一致すると、その繰り返しを失敗させます。Tsuzuri の Pike VM はこの規則を持たず、空の繰り返しも優先順位どおりに扱います。そのため、空に一致しうる本体を lazy な部分で繰り返すパターン（`a(?:.{0,2}?(?:c{0,2}?|a)){0,2}.` を `"c baa c"` に照合すると Tsuzuri は `(3, 5)`、JavaScript は `(3, 7)`）では結果が異なります。

## API リファレンス

すべての関数は `std::Regex` モジュールに属しています。`_utf8` の関数は `string` を `utf8string` に、範囲の単位を byte に読み替えたものです。

| 関数 | シグネチャ | 説明 |
| --- | --- | --- |
| `compile` | `ref string -> Result<Regex, Regex.Error>` | パターンをコンパイルします |
| `escape` | `ref string -> string` | 構文文字の前に `\` を付けます |
| `group_count` | `ref Regex -> i64` | 捕捉 group の数（全体の一致を除く） |
| `is_match` / `is_match_utf8` | `ref Regex -> ref string -> bool` | どこかに一致するか |
| `find` / `find_utf8` | `ref Regex -> ref string -> Maybe<(i64 * i64)>` | 最初の一致の範囲 |
| `find_at` / `find_at_utf8` | `ref Regex -> ref string -> i64 -> Maybe<(i64 * i64)>` | `start` からの最初の一致の範囲 |
| `find_all` / `find_all_utf8` | `ref Regex -> ref string -> [(i64 * i64)]` | 重ならない一致の範囲をすべて |
| `captures` / `captures_utf8` | `ref Regex -> ref string -> Maybe<[(i64 * i64)]>` | 最初の一致の全体と各 group の範囲 |
| `replace_all` / `replace_all_utf8` | `ref Regex -> ref string -> ref string -> string` | すべての一致を置換文字列で置き換えます |
| `split` / `split_utf8` | `ref Regex -> ref string -> [string]` | 一致の間の部分文字列 |
| `search_steps` | `ref Regex -> ref string -> i64` | `find` と同じ探索の VM の手数。線形時間の試験のための関数です |

## 所有権と借用

- `Regex` は内部に命令と class の配列を持つ非 Copy の値で、clone はありません。`let other = re` の後の `re` の使用は `E1012`、フィールドの読み書きや構築は `E1022` です。
- 照合の関数は `Regex` とテキストを共有借用するので、同じ `Regex` を何度でも、複数の式から同時に使えます。`task` へは move できます。
- 結果の範囲、配列、文字列は呼び出し側が所有する新しい値です。

## まとめ

- `compile` で一度コンパイルし、`Regex` を共有借用して検索します。誤りと上限超過は `Result.Error` です。
- 照合は leftmost-first の Pike VM で、1 回の探索は入力の長さに線形です。
- 範囲は `string` ではコード単位、`utf8string` では byte の半開区間です。
- `\d`・`\w`・`\s`・`\b`・`i` は Unicode 17.0.0 に従います。表は [Unicode](./unicode.md) モジュールで確かめられます。

## 関連項目

- [String](./string.md) — 部分文字列の検索と分割
- [Utf8String](./utf8string.md)
- [Unicode](./unicode.md) — 一般カテゴリーと大文字小文字の表
- [Result](./result.md)
- [言語リファレンスの目次](../index.md)
