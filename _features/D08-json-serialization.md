# D08: 構造化データの直列化（JSON）と Encode／Decode の導出

| 項目 | 内容 |
| --- | --- |
| ID | D08 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A07, D02, (C09) |
| 後続 | E13 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（std モジュール `Json` と組み込みクラス `Encode`／`Decode` の確定。GUIDE D-30 の仮割り当てを D-07 へ移す） |
| 改善する劣位 | C#/F# 比: 標準ライブラリの不足（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: `System.Text.Json`・serde に相当する直列化がない |
| 手本にする既存実装 | 導出の平らな束縛の連鎖: `src/derive.rs` の `Build::record`（`DeriveClass::Display` の `$text{index}` 束縛）と `Build::union`（`Build::pattern`・`Build::matched`・`Build::arm`）。std 型を使う組み込みクラスのシグネチャ: `src/polymorph.rs` の `Classes::collect` の `Parse`（`names.std_type("Maybe", "Maybe", ...)` と、std に無いときの `E1004`）。組み込みクラスの source instance: `tests/fixtures/display_parse/Main.tz` の `instance Display<Label>`。UTF-8 のバイト走査: `std/Utf8String.tz` の `find`・`matches_at`。std の登録: `src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES` と C06 の `std/Map.tz`。数値の変換: `Parse.parse`／`to_string`（`src/runtime/numeric.c` の `tz_soft_parse`・`tz_soft_format`） |
| 主な影響ファイル | `std/Json.tz`（新規）, `src/stdlib.rs`, `src/syntax.rs`, `src/parser.rs`, `src/derive.rs`, `src/polymorph.rs`, `tests/json.rs`（新規）, `tests/deriving.rs`, `tests/fixtures/json/Main.tz`（新規）, `tests/features.mjs`（suite `json`）, `tests/json.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `_docs/library-reference/json.md`（新規）, `_docs/library-reference/README.md`, `_docs/language-reference/deriving.md`, `_docs/feature-status.md`, `_features/README.md`, `_features/GUIDE.md`（D1 の承認後に D-07 の表と D-30 の行） |

## 目的

レコード・union・配列・`Vec`・`Maybe` を JSON と相互変換できるようにする。
ホスト ABI（E05）はスカラーとバッファ中心なので、Web ホストや設定ファイルとの間で複雑な値を受け渡す標準の経路として使う。
解析は RFC 8259 に厳密で、上限を超える入力を `Result` の Error で拒否し、スタック枯渇もトラップも起こさない。
出力は決定的で、native と WASM、`-O0` と `-O3` で同じバイト列になる。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- A07・D02 が `_features/README.md` の状態欄で done であること。確認: `grep -n "| A07 \|| D02 \|| C09 \|| D08 " _features/README.md`。
  C09（HashMap）は括弧付きの任意依存で、Phase 1 では使わない（D12）。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 組み込みクラスに `operation: None` のメソッドを登録すると、source instance の呼び出しが解決できない、または
  `Classes::intrinsic`・`Classes::structural` 以外の箇所で `Operation::Builtin` を前提にした `unwrap`・`unreachable!` に当たる。
- `std/Json.tz` の `instance Encode<i64>` などが `E1016`・`E1018`・`E1022` で拒否される（std が組み込みクラスの instance を持てない）。
- 導出したメソッドの本体の深さがフィールド数・case 数に比例する（D7 の形にできない）。
- 128 フィールドのレコードか 128 case の union の導出が `E1017` になる。
- std の汎用補助関数の具体化で `honors_the_exact_specialization_limit` が失敗する。
- 既存テストの期待値（IR・診断コード・メッセージ）を変える必要がある。`E1025` の導出一覧メッセージ（D6）だけは除く。
- stack-depth の 3 テスト（`tests/polymorphism.rs::bounds_type_growing_polymorphic_recursion`、
  `parser::tests::bounds_recursive_and_flat_expression_depth`、`tests/computations.rs::bounds_nested_builder_expansion_not_just_source_syntax`）が失敗する。
  または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、C ランタイムの追加・変更、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- JSON・直列化の API はない。`src/stdlib.rs` の `SOURCES` は `std/Array.tz` から `std/Vec.tz` までの 18 本で、`Json` はない。
  `RESERVED_MODULES` にも `Json` はない。
- 導出できるクラスは `src/syntax.rs` の `DeriveClass`（`Eq`・`Ord`・`Display`・`Hash`・`Default`）で、`src/parser.rs` の deriving 節の
  解析（`TokenKind::Deriving` の後）が名前を対応づける。それ以外は `E1025`。
- `src/derive.rs` の `instances` が record・union と deriving 指定ごとに `Build::record`／`Build::union` で通常の
  `Definition` を合成し、`Build::instance` が成分の型ごとに同じクラスの制約を付けた条件付き `InstanceDecl` を作る。
  `Build::definition` は `recursion: Some(name)` を設定するので、再帰型のメソッドは自分を呼べる。
  `Build::call` はクラスを生成名 `$class.{Class}` で参照し、`src/polymorph.rs` が `Provenance::Generated` のときだけ接頭辞を外して解決する。
- `Build::make` は式の深さと節点数を数え、深さ 128（`src/syntax.rs` の `MAX_NESTING`）か 4096 節点を超えると
  `E1017`（`deriving exceeds the expression depth or node limit`）。`instances` は導出が 1024 個を超えると `E1017`。
  `DeriveClass::Display` は `$text{index}` の束縛の連鎖で、フィールド数によらず深さを一定に保つ。
- 組み込みクラスは `src/polymorph.rs` の `BUILTIN_CLASSES` と `Classes::collect` で登録する。`Parse` のシグネチャは
  `names.std_type("Maybe", "Maybe", ...)` で std の型を引き、std に無ければ `E1004` のシグネチャ Error にする。
  `Classes::intrinsic` の `_ => false` により、一覧に無いクラスは組み込み instance を持たない。source instance は
  `class.builtin` かつ（marker・型変数・intrinsic・structural）のときだけ `E1016`（`built-in instances and marker classes cannot be overridden`）。
- 型クラスの宣言は `.tt` だけに書ける（`.tz` では `E1018`）。条件付き instance の頭部に tuple `('a * 'b)` と配列 `['a]` を書ける（下の再現）。
- 数値の表示は最短の往復表現で、指数表記の規則が JavaScript と異なる（`10000000.0` は `1e+7`、`-0.0` は `-0`。docs/language.md「表示と解析」）。
  `Parse.parse` は JSON より広い文法（`1.`・`.5`・`_`・`0x`・`inf`・`+1`）を受け、4096 UTF-16 単位を超える入力は `None`。
- utf8string は常に正しい UTF-8 で（`Utf8String.from_bytes` が検証し、不正なら `None`）、`text[index]` は `ubyte`。
  `Utf8String.from_string` は孤立サロゲートでトラップする。string は孤立サロゲートを保持できる。
- `Vec.to_array` は所有バッファを移し、要素を複製しない。`Array.sort` は `Ord<'a>` で並べた新しい配列を返す。
- 再帰 union の深い値の drop はスタックを使う（`tests/features.mjs` の `recursive_types` で `tz_deep_drop(1000000n)` が WASM でトラップする）。

### 再現（検証済み）

導出と `Json` の現状。各ケースを別ディレクトリに置く。

```tsuzuri
record Point { x: i64, y: i64 } deriving (Encode)

export def probe :: i64
fn probe = 1
```

```tsuzuri
export def probe :: i64
fn probe =
    let text = u8"[1]"
    let value = Json.parse (&text)
    1
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri check /tmp/tz-d08/derive  # error[E1025]: only Eq, Ord, Display, Hash and Default can be derived
target/release/tsuzuri check /tmp/tz-d08/parse   # error[E1002]: unknown value 'Json'
```

`Json.Value` の形は今の言語で書ける。ただし case 名と同じモジュールの型名は衝突する
（`Number of Number` は `E1001: duplicate or reserved name 'Number'`）。次は `check` が成功する。

```tsuzuri
record Numeral { text: utf8string }
union Value = Null | Bool of bool | Number of Numeral | Text of string | Array of [Value] | Object of [(string * Value)]

export def probe :: i64
fn probe =
    let items = [Array [Null, Bool true], Object [("a", Text "x")], Number (Numeral { text: u8"1.5" })]
    let index = 1
    let label = match index with
        | 0 -> 10
        | 1 -> 20
        | _ -> 30
    items.length + label
```

tuple と配列を頭部に持つ条件付き instance も書ける。`Shows.tt` の `class Show<'a> { def show :: ref 'a -> i64 }` に対し、`Main.tz` の
`instance (Shows.Show<'a>, Shows.Show<'b>) => Shows.Show<('a * 'b)> { fn show _p = 2 }` と
`instance Shows.Show<'a> => Shows.Show<['a]> { fn show _xs = 3 }` を `check` が受理した（`Shows.Show.show (&p)` の呼び出しを含む）。
一方、`String.length (&"abc")` は `E1013: borrow requires a local place` になるので、導出コードは文字列リテラルを借用せず値で渡す。

## 仕様

### 段階

- Phase 1（実装対象）: std `Json`（値の型、解析、出力、数値の変換）、組み込みクラス `Encode`／`Decode` と std の instance、
  `deriving (Encode, Decode)`。
- Phase 2（設計方針だけ）: 末尾の「Phase 2（設計方針）」。

### 前提とする他チケットのインターフェース

- A07（done）: `DeriveClass`、`Build`、`Build::instance` の条件付き instance 合成。
- D01（done）: `to_string` の最短往復表示と `Parse.parse`（全整数幅・f32・f64、一回丸め、範囲外は `None`）。
- D02（done）: `text[index]`（`ubyte`）、`Utf8String.from_string`、`to_string`（UTF-8 から UTF-16）。
- C02・C06（done）: `Vec.with_capacity`・`Vec.push`・`Vec.to_array`・`Array.sort`。C09 は使わない（D12）。
- E13（後続）への提供: 下の API をホストとの交換形式として使う。

### API

新 API（実装後に有効。未検証）。std のソースでは他の std モジュールを修飾して書く（GUIDE D-07）。

```tsuzuri
record Numeral { text: utf8string } deriving (Eq)
union Value = Null | Bool of bool | Number of Numeral | Text of string | Array of [Value] | Object of [(string * Value)] deriving (Eq)
union ErrorKind = Syntax | UnexpectedEnd | InvalidEscape | ControlCharacter | DuplicateKey of string | TooDeep | TooLarge | NonFinite | NumberRange | ExpectedType of string | MissingField of string | UnknownCase of string | LoneSurrogate deriving (Eq)
record Error { kind: ErrorKind, offset: i64 } deriving (Eq)

def parse :: ref utf8string -> Result<Value, Error>
def to_utf8string :: ref Value -> utf8string
def numeral :: ref utf8string -> Maybe<Numeral>
def to_i64 :: ref Numeral -> Result<i64, Error>
def to_f64 :: ref Numeral -> Result<f64, Error>
def encode :: Encode<'a> => ref 'a -> Result<Value, Error>
def decode :: Decode<'a> => ref Value -> Result<'a, Error>
def serialize :: Encode<'a> => ref 'a -> Result<utf8string, Error>
def deserialize :: Decode<'a> => ref utf8string -> Result<'a, Error>
```

組み込みクラス（コンパイラが登録する。source の宣言ではない）:

```text
Encode<'a> { encode :: ref 'a -> Result<Json.Value, Json.Error> }
Decode<'a> { decode :: ref Json.Value -> Result<'a, Json.Error> }
```

`Numeral.text` は常に RFC 8259 の number の字句である。`parse` は入力の字句をそのまま保持し（`1.0E+2` は `1.0E+2`）、
`numeral` は字句を検証して複製する。`Numeral { text: ... }` を直接書いた不正な字句は `to_utf8string` でトラップする（D14）。
`to_i64`・`to_f64` と数値の `Decode` は字句を string にして `Parse.parse` を一度だけ呼ぶ。整数型は `.`・`e`・`E` を含む字句を
`ExpectedType "integer"` で、範囲外と 4096 バイトを超える字句を `NumberRange` で拒否する。浮動小数点は最近接・偶数丸めを一度だけ行い、
無限大になる overflow を `NumberRange` にする。

### 型と JSON の対応

| Tsuzuri 型 | encode | decode が受理するもの |
| --- | --- | --- |
| `bool` | `true`／`false` | JSON の bool |
| `i8`–`i128`、`i8u`–`i128u` | `to_string` の十進 | 整数の字句で範囲内 |
| `f32`、`f64` | `to_string` の最短表現。NaN・無限大は `NonFinite` | 任意の number |
| `string` | JSON 文字列 | JSON 文字列 |
| `utf8string` | JSON 文字列 | JSON 文字列。孤立サロゲートを含めば `LoneSurrogate` |
| `Maybe<'a>` | `None` は `null`、`Some x` は x | `null` は `None`、それ以外は `Some` |
| `['a]`、リスト、`Vec<'a>` | array | array |
| 2–4 要素の tuple | 同じ長さの array | 同じ長さの array（違えば `ExpectedType "array of N"`） |
| `Json.Value` | 複製 | 複製 |
| 導出した record | object。キーはフィールド名で宣言順 | object。余分なキーは無視。キーが無ければ `null` として decode し、失敗なら `MissingField` |
| 導出した union | payload なしは `"Case"`、payload ありは `{"Case": payload}`（複数 payload は tuple なので array） | 同じ形だけ。未知の名前は `UnknownCase`、形が違えば `ExpectedType` |

`Maybe<Maybe<'a>>` の `Some None` は `null` になり、decode では `None` に戻る（serde と同じ。往復しない）。
f16・f128・decimal・char・utf8char・unit・関数・Task・参照・SIMD・`Map`・`Set` には instance を置かない（Phase 2）。

### 解析の規則

RFC 8259 に厳密に従う。空白は space・`\t`・`\n`・`\r` だけ。最上位は任意の値。先頭 BOM・コメント・末尾カンマ・
`NaN`・`Infinity`・先頭ゼロ（`01`）・`+1`・`.5`・`1.`・大文字の `TRUE` は `Syntax`。文字列中の 0x00–0x1F の生バイトは
`ControlCharacter`、`\"`・`\\`・`\/`・`\b`・`\f`・`\n`・`\r`・`\t`・`\uXXXX`（16 進は大小文字とも可）以外のエスケープは `InvalidEscape`。
`\uD83D\uDE00` の組は 1 文字に、組にならない `\uD800` は孤立サロゲートのコード単位としてそのまま `Text` に入れる（`JSON.parse` と同じ）。
同じ object のキーの重複は `DuplicateKey key`。値の後に空白以外があれば `Syntax`。入力の UTF-8 の正しさは utf8string が保証する。

`offset` は入力の先頭からのバイト位置で、入力を不正にした最初のバイトを指す。`UnexpectedEnd` は入力の長さ、`TooLarge` は 0、
`TooDeep` は深さ 129 を開く `[`／`{`、`DuplicateKey` は重複したキーの開き `"` を指す。encode・decode・数値変換の Error の `offset` は -1。

### 出力の規則

空白を出さず、キーは格納順、number は `Numeral.text` をそのまま出す。文字列は `"`→`\"`、`\`→`\\`、U+0008→`\b`、U+000C→`\f`、
U+000A→`\n`、U+000D→`\r`、U+0009→`\t`、その他の U+0000–U+001F は `\u00xx`、孤立サロゲートは `\udxxx`（16 進は小文字）にし、
それ以外（`/`、U+007F、U+2028 を含む）は UTF-8 のまま出す。これは ECMAScript 2019 以降の `JSON.stringify` の文字列出力と同じである。
利用者が組み立てた `Object` の重複キーは検査せずにそのまま出す（D3）。

### エラー

`instance Display<Json.Error>` は `json: <種類>` に、`offset >= 0` なら ` at byte <offset>` を付ける。

| 種類 | 条件 | 表示 |
| --- | --- | --- |
| `Syntax` | 文法違反、BOM、末尾の余分な値 | `syntax error` |
| `UnexpectedEnd` | 値の途中で入力が終わる、空入力 | `unexpected end of input` |
| `InvalidEscape` | 未定義のエスケープ、`\u` の後が 16 進 4 桁でない | `invalid escape` |
| `ControlCharacter` | 文字列中の 0x00–0x1F | `control character in string` |
| `DuplicateKey k` | 同じ object の重複キー | `duplicate key "k"`（k は `JSON.stringify` と同じ規則で引用） |
| `TooDeep` | 入れ子が 128 を超える | `nesting deeper than 128` |
| `TooLarge` | 入力が 64 MiB を超える | `input larger than 64 MiB` |
| `NonFinite` | NaN・無限大の encode | `non-finite number` |
| `NumberRange` | 型の範囲外、overflow、4096 バイト超の字句 | `number out of range` |
| `ExpectedType t` | JSON の種類が違う | `expected t` |
| `MissingField f` | 必須のフィールドがない | `missing field "f"` |
| `UnknownCase c` | union に無い case 名 | `unknown case "c"` |
| `LoneSurrogate` | utf8string へ decode する文字列に孤立サロゲート | `lone surrogate in string` |

### 評価順序・所有権・借用

- `parse`・`to_utf8string`・`encode`・`decode` は引数を共有借用し、新しい所有値を返す。文字列は複製する。入力を変えない。
- 導出した encode はフィールドを宣言順に encode し、最初の Error を返して残りを encode しない。導出した decode は全フィールドを
  宣言順に decode し、宣言順で最初の Error を返し、成功した他のフィールドを解放する。
- Error の経路でも途中の値をすべて解放する（`live == 0`）。トラップは D14 の不正な `Numeral` と確保失敗だけ。

### 数値・トラップ・native と WASM の差

- 数値の変換は `tz_soft_parse`・`tz_soft_format` だけを使い、native・WASM・`-O0`・`-O3` で同じ結果になる。fast-math は使わない。
- f32 は f32 の最短表現で出し（`0.1f32` は `0.1`）、decode は字句から f32 へ一度だけ丸める（f64 を経由しない）。
- JavaScript との差（テストの参照で扱う）: `-0.0` は `-0`（JS は `0`）、`1e+7` の形（JS は `10000000`）、NaN・無限大は Error（JS は `null`）、
  2^53 を超える整数を正確に保つ（JS は丸める）、重複キーは Error（JS は後勝ち）、深さ 129 は Error、`1e400` は `NumberRange`（JS は `Infinity`）。
  JS の `JSON.stringify` は整数形のキー（`"1"`）を先に並べ替えるが、Tsuzuri は格納順を保つ。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E1025` | deriving 節の未知のクラス名 | `only Eq, Ord, Display, Hash, Default, Encode and Decode can be derived` | クラス名 |
| `E1025` | 導出した instance の成分に `Encode`／`Decode` の instance がない | 既存の成分エラーのメッセージ（変更しない） | deriving 指定 |
| `E1005` | `Json.encode (&c)`（`c: char`）など、instance のない直接の使用 | `no instance for Encode<char>; define an instance or use a supported type`（既存） | 呼び出し |
| `E1016` | 利用者の `instance Encode<i64>` など std と重なる instance | `overlapping instance for Encode<i64>`（既存） | instance のクラス名 |
| `E1017` | 導出の深さ・節点数 | `deriving exceeds the expression depth or node limit`（既存） | deriving 指定 |
| `E1004` | std に `Json` が無い入力で `Encode`／`Decode` を使う | `Encode needs the standard module Json`／`Decode needs the standard module Json` | 使用箇所（`Parse` と同じ扱い） |

### 資源上限

- 入力は 64 MiB（67,108,864 バイト）まで。超えたら字句を読む前に `TooLarge`。
- 配列と object の入れ子は `MAX_NESTING`（128）まで。`[[1]]` の深さは 2。128 は受理、129 は `TooDeep`。
  解析は深さを数える再帰下降で、再帰の前に深さを検査するので、1,000,000 個の `[` でもスタックは 128 段で止まる。
- `Numeral` の変換は 4096 バイトまで（`Parse.parse` の上限）。
- 重複キーの検査は、32 メンバー以下なら線形、それを超えたらキーを複製して `Array.sort` し隣を比べる（O(n log n)）。
- 導出した decode のフィールド探索は線形（O(フィールド数 × メンバー数)）。
- `to_utf8string`、導出した encode・decode は値の構造の深さだけ再帰する。導出した `Display`・`Hash` と drop と同じ性質で、
  解析した値の深さは 128 以下になる。

### 例

新 API（実装後に有効。未検証）。

```tsuzuri
record Point { x: i64, y: f64 } deriving (Encode, Decode)
union Shape = Circle of f64 | Rect of f64 * f64 | Empty deriving (Encode, Decode)
record Scene { name: string, points: [Point], shapes: Vec<Shape>, note: Maybe<string> } deriving (Encode, Decode)
```

`Scene { name: "a", points: [Point { x: 1, y: 0.5 }], shapes: (Circle 2.0, Rect (1.0, 2.5), Empty の Vec), note: None }` の
`Json.serialize` は次のバイト列になる。

```json
{"name":"a","points":[{"x":1,"y":0.5}],"shapes":[{"Circle":2},{"Rect":[1,2.5]},"Empty"],"note":null}
```

| 入力 | 操作 | 結果 |
| --- | --- | --- |
| `{"x":1}` | `Point` へ decode | `MissingField "y"`、offset -1 |
| `{"x":1,"y":2,"z":3}` | `Point` へ decode | 成功（`z` は無視） |
| `{"x":1.5,"y":2}` | `Point` へ decode | `ExpectedType "integer"` |
| `{"Square":1}` | `Shape` へ decode | `UnknownCase "Square"` |
| `"Circle"` | `Shape` へ decode | `ExpectedType "object"` |
| `[1,2,]` | `parse` | `Syntax`、offset 5 |
| `{"a":1,"a":2}` | `parse` | `DuplicateKey "a"`、offset 7 |
| `01` | `parse` | `Syntax`、offset 1 |
| `[` を 129 個 | `parse` | `TooDeep`、offset 128 |
| `{"n":18446744073709551615}` | `parse` して `to_utf8string` | 入力と同じバイト列 |

### Phase 2（設計方針）

ストリーミング解析、借用した部分文字列によるゼロコピー、`Map`・`HashMap`（C09 の後）・f16・f128・decimal の instance、
フィールド名の変更属性、整形出力、反復の出力器、他形式（CBOR など）へ一般化した Serializer クラス。

## 設計

### データ構造

```rust
// src/syntax.rs
pub enum DeriveClass { Eq, Ord, Display, Hash, Default, Encode, Decode }
// DeriveClass::name: Self::Encode => "Encode", Self::Decode => "Decode"

// src/polymorph.rs: 末尾に足す（既存のクラス id と IR を変えない）
const BUILTIN_CLASSES: &[&str] = &[/* 既存 */ "Elementary", "Encode", "Decode"];
```

`Classes::collect` は `Hash` の登録の後で、`names.std_type("Json", "Value", ...)`・`names.std_type("Json", "Error", ...)`・
`names.std_type("Result", "Result", ...)` からシグネチャを作り、`operation: None` のメソッドを 1 つずつ登録する。
std に `Json` が無ければ `Parse` と同じくシグネチャを `Err(E1004)` にする。`Classes::intrinsic`・`Classes::structural` は変えない
（`_ => false` なので組み込み instance はなく、std の source instance だけが使われる）。

`std/Json.tz`（新規）の構成: 公開の型 4 つ、公開の関数 9 つ、instance（`Encode`・`Decode` を bool、整数 10 型、f32、f64、string、
utf8string、`Maybe<'a>`、`['a]`、リスト `[|'a|]`、`Vec<'a>`、2–4 要素の tuple、`Value` に。`Display<Error>`）、導出コード用の公開補助関数（新規）:

| 補助関数（新規） | 型 | 役割 |
| --- | --- | --- |
| `begin_object` | `i64 -> Result<Vec<(string * Value)>, Error>` | 容量付きの空の member 列 |
| `encode_field` | `Encode<'a> => Result<Vec<(string * Value)>, Error> -> string -> ref 'a -> Result<Vec<(string * Value)>, Error>` | Error なら何もせず返す。Ok なら値を encode して push |
| `end_object` | `Result<Vec<(string * Value)>, Error> -> Result<Value, Error>` | `Vec.to_array` して `Object` |
| `encode_case` | `Encode<'a> => string -> ref 'a -> Result<Value, Error>` | `{"Case": payload}` |
| `encode_tag` | `string -> Result<Value, Error>` | `"Case"` |
| `expect_object` | `ref Value -> Result<unit, Error>` | object でなければ `ExpectedType "object"` |
| `decode_field` | `Decode<'a> => ref Value -> string -> Result<'a, Error>` | 線形探索。無ければ `Null` を decode し、失敗なら `MissingField` |
| `keep_error` | `Maybe<Error> -> Result<'a, Error> -> Maybe<Error>` | 最初の Error を残し、残りを解放 |
| `case_index` | `ref Value -> [string] -> [bool] -> Result<i64, Error>` | case 名と payload の有無から index。形の検査 |
| `payload` | `ref Value -> ref Value` | 1 キーの object の値。`case_index` の成功後だけ呼ぶ（それ以外はトラップ） |

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/Json.tz`（新規） | 上の全部 | 解析・出力・数値・instance・補助関数 |
| std 登録 | `src/stdlib.rs` | `SOURCES`, `RESERVED_MODULES` | `("std/Json.tz", include_str!("../std/Json.tz"))` を名前順（`IO.tc` と `List.tz` の間）に、`"Json"` を予約に足す |
| 構文 | `src/syntax.rs` | `DeriveClass`, `DeriveClass::name` | `Encode`・`Decode` |
| 構文 | `src/parser.rs` | deriving 節（`TokenKind::Deriving` の後の名前の対応） | `"Encode"`・`"Decode"` と E1025 のメッセージ（D6） |
| 文書生成 | `src/docgen.rs` | `derives_text` | `DeriveClass::name` を使っていれば変更なし。match なら 2 arm |
| 型クラス | `src/polymorph.rs` | `BUILTIN_CLASSES`, `Classes::collect` | 2 クラスと 2 メソッド（上） |
| 導出 | `src/derive.rs` | `Build::record`, `Build::union` | `DeriveClass::Encode`・`Decode` の分岐を `Build::operators` の前に（アルゴリズム） |
| 導出 | `src/derive.rs` | `Build::make` | `ExprKind::Array(values)` の arm（深さ = 要素の最大 + 1）。`_ => unreachable!()` なので足さないと panic |
| 導出 | `src/derive.rs` | `Build::integer_pattern`（新規） | `PatternKind::Literal(Box<Expr>)`、depth 2 |
| 導出 | `src/derive.rs` | `instances` | 変更なし（Encode・Decode は全 payload を成分にする。`Default` だけが先頭 case） |
| 生成 | `src/llvm.rs` | なし | 変更なし（新しい `Builtin` を作らない） |

### 生成 IR とランタイム

新しい IR の形・runtime symbol・C コードはない。`std/Json.tz` の関数は他の std と同じく到達可能なものだけ出る（GUIDE D-07）。
`Json` を使わないプログラムの IR は byte 単位で変わらない（手順 1 のベースラインと比べる）。数値は既存の `@tz_soft_parse`・
`@tz_soft_format` を `Parse.parse`／`to_string` 経由で呼ぶ。

### アルゴリズム

導出コードは D7 の平らな形にする。`Main` モジュールの `Point`・`Shape`（例）に対して次を合成する（生成名なので未検証の形。
`&$value.x` は `Build::field`、クラス呼び出しは `Build::call` の `$class.Encode`）。

```text
fn encode $value =                              -- record
    let $object0 = Json.begin_object 2
    let $object1 = Json.encode_field $object0 "x" (&$value.x)
    let $object2 = Json.encode_field $object1 "y" (&$value.y)
    Json.end_object $object2

fn decode $json =                               -- record
    let $shape = Json.expect_object $json
    let $field0 = Json.decode_field $json "x"
    let $field1 = Json.decode_field $json "y"
    let $failed0 = Result.is_error (&$shape)
    let $failed1 = $failed0 || Result.is_error (&$field0)
    let $failed2 = $failed1 || Result.is_error (&$field1)
    if $failed2 then
        let $error0 = Json.keep_error Maybe.None $shape
        let $error1 = Json.keep_error $error0 $field0
        let $error2 = Json.keep_error $error1 $field1
        Result.Error (Maybe.get $error2)
    else Result.Ok (Main.Point { x: Result.get $field0, y: Result.get $field1 })

fn encode $value = match $value with             -- union
    | Main.Shape.Circle $payload -> Json.encode_case "Circle" $payload
    | Main.Shape.Rect $payload -> Json.encode_case "Rect" $payload
    | Main.Shape.Empty -> Json.encode_tag "Empty"

fn decode $json =                               -- union
    let $index = Json.case_index $json ["Circle", "Rect", "Empty"] [true, true, false]
    if Result.is_error (&$index) then Result.Error (Result.get_error $index) else
    match Result.get $index with
    | 0 ->
        let $payload = $class.Decode.decode (Json.payload $json)
        if Result.is_error (&$payload) then Result.Error (Result.get_error $payload)
        else Result.Ok (Main.Shape.Circle (Result.get $payload))
    | 1 -> （Rect も同じ形。payload は tuple 1 つ）
    | _ -> Result.Ok Main.Shape.Empty           -- 最後の case は wildcard（網羅性のため）
```

解析は `private` の再帰下降関数（`value`・`array`・`object`・`string`・`number`）で、位置と深さを引数に持つ。`array`・`object` は
深さ + 1 が 128 を超えたら再帰せずに `TooDeep` を返す。要素は `Vec` に積んで `Vec.to_array` で移す。string は UTF-8 を
UTF-16 へ組み立てる（`\u` の組は 1 コードポイントへ）。number は文法を検査した範囲を `Numeral` に複製する。
出力は `Vec<ubyte>` へ書き、最後に `Utf8String.from_bytes` で utf8string にする（出力は常に正しい UTF-8 なので `Maybe.get` で取り出す）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、`Json` を使わない fixture の IR を保存する。
- 確認: 次がすべて成功し、4 つのテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-d08
target/release/tsuzuri build tests/fixtures/deriving --emit llvm -o /tmp/tz-d08/before.ll
target/release/tsuzuri build tests/fixtures/deriving --emit llvm -O3 -o /tmp/tz-d08/before-O3.ll
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: std `Json` の骨格と数値

- 変更: `std/Json.tz`（新規）、`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`、`tests/json.rs`（新規）。
- 内容: 4 つの型、`numeral`・`to_i64`・`to_f64`、重複キー検査用の `private` 関数（`Array.sort` を呼ぶ）を書く。
  `tests/json.rs` に `tests/deriving.rs` の `accepts` を写し、`rejects(source, code)` を足す。
- 確認: `cargo test --locked --test json` が `2 passed`（`json_module_is_reserved_and_loaded`・`numeral_functions_type_check`）。
  `cargo test --locked --test stdlib` が成功する。`Array.sort` が case `Array` と衝突したら停止する（落とし穴）。

### 手順 3: 解析

- 変更: `std/Json.tz` の `parse` と `private` の再帰下降関数。
- 内容: 「解析の規則」「資源上限」どおり。深さの検査は再帰の前。途中で Error になったら作りかけの `Vec` を返さずに捨てる。
- 確認: `cargo test --locked --test json` が `3 passed`（`parse_signature_and_ir_are_deterministic` を追加）。

### 手順 4: 出力と `Display<Error>`

- 変更: `std/Json.tz` の `to_utf8string`、`instance Display<Error>`。
- 内容: 「出力の規則」どおり。`Numeral` の字句は `assert` で検証する（D14）。
- 確認: `cargo test --locked --test json` が `3 passed` のまま。`cargo test --locked` が成功する。

### 手順 5: 組み込みクラスとスカラーの instance

- 変更: `src/polymorph.rs` の `BUILTIN_CLASSES`・`Classes::collect`、`std/Json.tz` の bool・整数 10 型・f32・f64・string・utf8string の instance。
- 内容: 「データ構造」どおり。`operation: None` の組み込みメソッドで source instance が選ばれることを確かめる。
- 確認: `cargo test --locked --test json` が `6 passed`（`builtin_classes_have_json_signatures`・`scalar_instances_type_check`・
  `user_instances_cannot_overlap_std`）。停止条件の最初の 2 項に当たったら止める。

### 手順 6: コンテナの instance と公開関数

- 変更: `std/Json.tz` の `Maybe`・配列・リスト・`Vec`・tuple（2–4）・`Value` の instance、`encode`・`decode`・`serialize`・`deserialize`、補助関数。
- 確認: `cargo test --locked --test json` が `8 passed`（`container_instances_type_check`・`unsupported_types_report_e1005`）。

### 手順 7: deriving の名前

- 変更: `src/syntax.rs` の `DeriveClass`、`src/parser.rs` の deriving 節、`src/docgen.rs` の `derives_text`、`src/derive.rs` の `Build::operators`
  （未実装の Encode・Decode は既存の `this deriving implementation is not available yet` のまま）。
- 確認: `cargo test --locked --test deriving` が成功する。E1025 のメッセージを検査する既存テストがあれば D6 の文言へ直す
  （`grep -rn "only Eq, Ord, Display, Hash and Default" src tests docs _docs`）。

### 手順 8: record の導出

- 変更: `src/derive.rs` の `Build::record`、`Build::make`（`ExprKind::Array`）。
- 内容: 「アルゴリズム」の record の形。キーは `self.text(&field.name.text)` を値で渡す（借用しない）。
- 確認: `cargo test --locked --test json` が `10 passed`（`derives_records_with_constant_depth`・`derives_generic_and_recursive_records`）。

### 手順 9: union の導出

- 変更: `src/derive.rs` の `Build::union`、`Build::integer_pattern`（新規）。
- 確認: `cargo test --locked --test json` が `12 passed`（`derives_unions_with_constant_depth`・`derive_components_without_instances_report_e1025`）。
  手順 1 の stack-depth 3 テストと `honors_the_exact_specialization_limit` が成功する。

### 手順 10: E2E suite `json`

- 変更: `tests/fixtures/json/Main.tz`（新規）、`tests/features.mjs` に suite `json`（GUIDE §7.4）。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri json` が成功する。

### 手順 11: JavaScript を参照にした適合テスト

- 変更: `tests/json.mjs`（新規）。
- 確認: `npx --yes --package=node@24 node tests/json.mjs target/release/tsuzuri` が成功し、件数を表示する。

### 手順 12: 全体と IR の不変

- 確認: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked` が成功する。
  `node tests/features.mjs target/release/tsuzuri` と `node tests/display_parse.mjs target/release/tsuzuri` が成功する。
  手順 1 と同じコマンドで `/tmp/tz-d08/after.ll`・`after-O3.ll` を出し、`cmp` で `before` と一致する。

### 手順 13: 文書と最終確認

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/library-reference/json.md _docs/language-reference/deriving.md` が成功し、
  `git diff --check` が空。GUIDE §10 を満たす。

## テスト計画

### Rust テスト

`tests/json.rs`（新規）。受理は `accepts`（native と wasm の IR を 2 回出して一致）、拒否は `analyze(source).expect_err(source).code`。

| テスト | 内容 |
| --- | --- |
| `json_module_is_reserved_and_loaded` | 利用者の `Json.tz` は `E1011`。`Json.Null` を書くプログラムは受理 |
| `numeral_functions_type_check` | `Json.numeral`・`to_i64`・`to_f64` の呼び出し |
| `parse_signature_and_ir_are_deterministic` | `Json.parse`・`to_utf8string` を使う IR が 2 回で一致し、`Json` を使わないプログラムの IR に `Json` の関数がない |
| `builtin_classes_have_json_signatures` | `Encode.encode`・`Decode.decode` の型が「API」どおり。std に `Json` が無い custom std では `E1004`（`tests/deriving.rs::derives_preserve_scopes_limits_and_standard_independence` の custom std の作り方を写す） |
| `scalar_instances_type_check` | bool・10 整数型・f32・f64・string・utf8string を `Json.encode`／`Json.decode` |
| `user_instances_cannot_overlap_std` | `instance Encode<i64>` は `E1016`。利用者の record への手書き instance は受理 |
| `container_instances_type_check` | `Maybe`・配列・リスト・`Vec`・2–4 tuple・`Json.Value`、入れ子 |
| `unsupported_types_report_e1005` | char・f16・decimal64・関数・5 要素 tuple の `Json.encode` は `E1005` |
| `derives_records_with_constant_depth` | 128 フィールドの record と空の record を導出して受理 |
| `derives_generic_and_recursive_records` | `record Box<'a>`、`Maybe` で自己参照する record |
| `derives_unions_with_constant_depth` | 128 case の union、payload なし・1 つ・複数、1 case だけの union |
| `derive_components_without_instances_report_e1025` | `record R { f: i64 -> i64 } deriving (Encode)` と `char` のフィールドは `E1025`、`deriving (Copy)` は D6 のメッセージ |

### E2E

`tests/fixtures/json/Main.tz`（新規）と `tests/features.mjs` の suite `json`。native と WASM × `-O0`／`-O3`、全ケースの後に `live == 0`、
WASM の import なし（suite の既定の検査）。期待値は JavaScript で独立に計算する。

| export（新規） | 引数 | 期待値の出どころ |
| --- | --- | --- |
| `scene_hash` | なし | 「例」の Scene を `Json.serialize` した UTF-8 の FNV-1a 64。JS: `JSON.stringify` の同じ構造のバイト列の FNV-1a 64 |
| `scene_round_trip` | なし | `deserialize (serialize x) == x` で `true` |
| `nesting` | 深さ n | `[`×n + `]`×n の `parse`。成功は -1、失敗は offset。n = 1, 128 は -1、129 と 1,000,000 は 128 |
| `number_bits` | case 番号 | `to_f64` の bit 列。`0.1`・`-0`・`5e-324`・`1.7976931348623157e308`。JS: `Float64Array`／`BigUint64Array` |
| `integer_status` | case 番号 | `9223372036854775807` は成功、`9223372036854775808` は `NumberRange`、`1.0` は `ExpectedType`、`18446744073709551615` の `i64u` は成功。JS BigInt で範囲を計算 |
| `field_rules` | case 番号 | 「例」の表の decode 5 行。種類の番号と offset |
| `text_rules` | case 番号 | `"\ud800"` の出力は `"\ud800"`（JS の `JSON.stringify("\ud800")`）、utf8string への decode は `LoneSurrogate`、`NaN` の encode は `NonFinite` |

`too_large`（65 MiB の入力が `TooLarge`、offset 0）は WASM の 16 MiB の検査に当たるので `nativeCases` に置く。

`tests/json.mjs`（新規）は適合テストで、入力表から `Cases.tz` を一時ディレクトリに生成し、`status :: i64 -> i64`（成功は -1、失敗は
種類の番号 × 2^40 + offset）と `output_hash :: i64 -> i64`（`to_utf8string` の FNV-1a 64、失敗は 0）を export する。
native（`tests/features.mjs` の `run` の host.c と追跡確保を写す）と WASM を `-O0`／`-O3` で実行し、次と照合する。
入力は `u8"..."` に埋め込み、印字可能 ASCII 以外と `"`・`\` をエスケープする（生成物を `tsuzuri check` で確かめてから使う）。

- 受理と拒否: `JSON.parse` が成功することと `status == -1` が一致する。差（重複キー、深さ 129、BOM は両方拒否）は表に期待する種類を書く。
- 出力: 数値が JS の正準形（`String(Number(t)) === t`）で整数形のキーを含まない入力は、`JSON.stringify(JSON.parse(input))` のバイト列と一致する。
- 字句保持: `1.0E+2`・`-0`・`1e400`・`18446744073709551615` などは空白を除いた入力と一致する（手で書いた期待値）。
- 入力表: RFC 8259 §13 の 2 例、全エスケープ、サロゲートの組と孤立、0x00–0x1F の生バイト、先頭ゼロ・`+1`・`.5`・`1.`・`NaN`・
  `Infinity`・コメント・末尾カンマ・途中で切れた入力・空入力・最上位のスカラー・末尾の余分な値、深さ 128 と 129、
  seed 固定の LCG で JS が作る 200 個の値の `JSON.stringify`。

### 既存テストへの影響

- E1025 の導出一覧メッセージを検査するテストがあれば D6 の文言に変わる（正当）。
- std モジュールの一覧や予約名を列挙するテスト（`tests/stdlib.rs` など）があれば `Json` が増える（正当）。それ以外はなし。

### 性能

Phase 1 の合否条件にしない。完了後に docs/benchmarks.md の手順で、serde_json・System.Text.Json と同じ入力（1 MiB の object 配列）の
解析と出力の時間を記録してよい。速度の主張は計測した値だけにする。

## ドキュメント

- `docs/language.md`: `### Vec` の後に `### Json`（新規。API・型の対応・解析と出力の規則・上限・JS との差）。deriving の段落
  （`deriving (Eq, Ord, Display, Hash, Default)` の例の後）に `Encode`・`Decode` と符号化の形。
- `docs/architecture.md`: std の一覧と導出の段落（生成名と provenance の説明の近く）に `Encode`・`Decode` の合成形（D7）。
- `_docs/library-reference/json.md`（新規）と `_docs/library-reference/README.md` の「分野別リファレンス」へのリンク。
  `_docs/library-reference/api/` に std モジュールごとのページがあるので、既存ページの作り方（`grep -rn "library-reference/api" scripts tests src`）で `Json.md` を作る。
- `_docs/language-reference/deriving.md`: `## Encode と Decode`（新規）。
- `_docs/feature-status.md` と `_features/README.md` の D08 の状態。D1 の承認後に `_features/GUIDE.md` の D-07 の表へ `Json`、
  D-30 から `Encode`／`Decode`・`Json` の行を移す。

## 受け入れ条件

- [ ] D1 が承認されている。
- [ ] `Json.parse`・`to_utf8string`・`numeral`・`to_i64`・`to_f64`・`encode`・`decode`・`serialize`・`deserialize` が「API」どおりに動く。
- [ ] 「型と JSON の対応」の全行と `deriving (Encode, Decode)` が動き、導出の深さがフィールド数・case 数によらない。
- [ ] 上限を超える入力を Error で拒否し、1,000,000 個の `[` でもスタック枯渇・トラップを起こさない。
- [ ] Error の経路を含む全ケースで `live == 0`、WASM の import なし、native と WASM × `-O0`／`-O3` で同じ結果。
- [ ] `Json` を使わないプログラムの IR が手順 1 のベースラインと一致する。
- [ ] `tests/json.rs`・suite `json`・`tests/json.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `std/Json.tz` の case `Array` がモジュール `Array` と衝突して `Array.sort` を呼べないかもしれない。手順 2 で最初に確かめ、衝突したら停止して報告する。
- `Build::make` の match は `_ => unreachable!()`。導出で `ExprKind::Array` などを新しく使うなら arm を足さないと panic する。
- 文字列リテラルの借用は `E1013`。導出コードではキーを値で渡す（補助関数の引数は `string`）。
- 生成コードは std の名前を必ず修飾する（`Maybe.None`、`Result.Ok`、`Result.Error`、`Json.encode_field`）。無修飾だと利用者の同名の型・case に解決される（GUIDE D-07）。
  `std/Json.tz` の中では `Error` が record `Json.Error`、case は `Result.Error` と書き分ける。
- `Parse.parse` は JSON より広い文法を受ける。字句の検査を先にし、`Parse.parse` には検査済みの字句だけを渡す。入力は UTF-16 の string。
- `Utf8String.from_string` は孤立サロゲートでトラップする。`Decode<utf8string>` は変換の前に検査して `LoneSurrogate` を返す。
- 重複キーはエスケープを解いた後のコード単位で比べる（`"a"` と `"\u0061"` は重複）。offset は UTF-8 のバイト位置で、UTF-16 の位置ではない。
- 導出した decode を「効率のため」入れ子の match にしない。深さがフィールド数に比例して `E1017` と stack-depth の失敗を招く。
- JS の参照: `JSON.stringify` は整数形のキーを先に並べ、`-0` を `0` と書き、`1e21` 未満を指数にしない。これらを含む入力は字句保持の表へ回す。
- Node 20 は BigInt の多い suite で異常終了することがある。`npx --yes --package=node@24` を使う。
- WASM の suite は 16 MiB を超えると失敗する。64 MiB の検査は `nativeCases` だけ。

## 対象外

- スキーマ検証、JSON Pointer／Patch、YAML・TOML、JSON5・コメント付き JSON。
- 整形出力、ストリーミング、フィールド名の変更属性、`Map`・`HashMap` の instance（Phase 2）。
- Web ホストとの受け渡し例（E13 のホスト binding で扱う。E05 のバッファだけで書く glue をこのチケットで増やさない）。

## 決定事項

### D1: 名前と置き場所

- 決定: std モジュール `Json`（`std/Json.tz`）と、組み込みクラス `Encode`／`Decode`（`BUILTIN_CLASSES`）。導出名も `Encode`／`Decode`。
- 理由: GUIDE D-30 の仮割り当てどおり。クラスは `.tz` に宣言できず、組み込みにすると導出が既存の `$class.{Class}` 参照を使える。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: 値の表現

- 決定: 「API」の `Value`。数値は字句を保持する `Numeral`（case 名 `Number` と型名の衝突を避けて改名）。object は格納順の `(string * Value)` の配列。
- 理由: f64 固定では 2^53 を超える整数が失われる。配列は `Vec.to_array` で複製なしに作れ、キーの順序が決定的になる。
- 状態: 既定案（実装者はこの案に従う）

### D3: 解析の厳密さと重複キー

- 決定: RFC 8259 に厳密。BOM・コメント・末尾カンマは拒否、最上位のスカラーは受理、重複キーは `DuplicateKey`。出力は重複を検査しない。
- 理由: 重複の後勝ちは実装ごとに意味が変わる。出力での検査は O(n log n) の費用で、解析した値と導出した値では起きない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 入出力の文字列型

- 決定: 入力は `ref utf8string`、出力は `utf8string` だけ。`Text` の中身は string（UTF-16）。
- 理由: RFC 8259 の交換形式は UTF-8 で、utf8string は正しい UTF-8 を保証するので検証が要らない。string は `Utf8String.from_string` と `to_string` で変換できる。
- 状態: 既定案（実装者はこの案に従う）

### D5: Error の表現

- 決定: `Error { kind: ErrorKind, offset: i64 }`。offset は解析ではバイト位置、それ以外は -1。`Display<Error>` は「エラー」の表。
- 理由: 一つの型で `Result` を連結でき、位置は解析にしか意味がない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 導出の名前と E1025

- 決定: `deriving (Encode, Decode)`。未知の名前の `E1025` は `only Eq, Ord, Display, Hash, Default, Encode and Decode can be derived`。
- 理由: 既存の一覧メッセージに 2 名を足すだけで、コードは変えない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 導出コードの形

- 決定: 「アルゴリズム」の平らな形。record は束縛の連鎖、union は 1 段の match。std の公開補助関数を呼ぶ。
- 理由: `DeriveClass::Display` と同じく深さが一定で、`E1017` と stack-depth の制約を守る。補助関数に寄せると生成 AST が小さい。
- 状態: 既定案（実装者はこの案に従う）

### D8: 符号化の形

- 決定: record は宣言順の object、union は外部タグ（`"Case"`／`{"Case": payload}`、複数 payload は array）、`Maybe` は `null`、tuple は array。
- 理由: serde の既定と同じで Web ホストで扱いやすい。`Maybe<Maybe<'a>>` の曖昧さは serde と同じく受け入れる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 欠けたフィールドと余分なフィールド

- 決定: 余分なキーは無視。欠けたキーは `null` として decode し、失敗したら `MissingField`。
- 理由: serde・System.Text.Json の既定と同じ。`Maybe` の欠落を特別扱いせずに一つの規則で表せる。
- 状態: 既定案（実装者はこの案に従う）

### D10: 非有限値と数値の出力

- 決定: NaN・無限大の encode は `NonFinite`。数値は `to_string` の最短表現（`-0`・`1e+7` を含む）。
- 理由: JSON は非有限値を表せず、`null` への黙った変換は値を失う。最短表現は往復で元のビットに戻る。
- 状態: 既定案（実装者はこの案に従う）

### D11: 文字列のエスケープと孤立サロゲート

- 決定: 「出力の規則」（`JSON.stringify` と同じ）。孤立サロゲートは `\udxxx`。
- 理由: 旧案の既定どおり。JS と同じバイト列なので参照テストが直接比べられる。
- 状態: 既定案（実装者はこの案に従う）

### D12: C09 を使わない

- 決定: 重複キーは 32 以下で線形、超えたら `Array.sort`。decode のフィールド探索は線形。
- 理由: C09 を待たずに実装でき、最悪でも O(n log n) で解析の HashDoS がない。探索の索引は Phase 2 で計測してから足す。
- 状態: 既定案（実装者はこの案に従う）

### D13: 上限と解析の方式

- 決定: 入力 64 MiB、入れ子 128、字句 4096 バイト。解析は再帰の前に深さを検査する再帰下降。
- 理由: 旧案の「反復で行いスタックを深く使わない」の目的は、深さ 128 の上限で再帰を 128 段に抑えれば満たせ、明示スタックより短い。
- 状態: 既定案（実装者はこの案に従う）

### D14: `Numeral` の不変条件

- 決定: `to_utf8string` は字句を検証し、不正ならトラップする。`Numeral` は `Json.numeral` で作る。
- 理由: 出力の型を `Result` にせずに、不正な JSON を黙って出さない。検証は字句の長さに比例するだけ。
- 状態: 既定案（実装者はこの案に従う）

### D15: encode の戻り値

- 決定: `Encode.encode` は `Result<Json.Value, Json.Error>` を返す（旧案の `ref 'a -> Json.Value` を変更）。
- 理由: 旧案の未決事項の既定案どおり、非有限値を Error にするため。別 API（`Json.try_encode`）を作らずに済む。
- 状態: 既定案（実装者はこの案に従う）
