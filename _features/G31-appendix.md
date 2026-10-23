# G31 付録: タスクの一覧・JSON・コーパス・検証の記録

本文は [G31-ai-cost-metrics.md](G31-ai-cost-metrics.md)。この付録の B・C は、実装者が `benchmarks/ai/` に**そのまま置く**ファイルの全文（HEAD `a2313d19` 相当の `target/release/tsuzuri` と試作の実行器で検証済み）。A はタスクの一覧、D は検証の記録（golden の数値）、E は出力と検査の書式、F は Phase 2・3 の戦略の擬似コード。

## 付録 A: タスクの一覧

37 件（型 6 つ、各 6〜7 件。Phase 1 は A・B・D・E・F の 30 件、Phase 2 が C の 7 件）。

型の記号と名前: A = `definition`（定義と署名）、B = `callers`（呼び出し元）、C = `diagnostic`（診断の原因を直す）、D = `type-search`（型から std の関数を探す）、E = `spec`（仕様の規則を引く）、F = `impact`（変更の影響を受けるテスト）。コーパスの `shop` は `benchmarks/ai/corpus/shop`、`diag/cNN` は `benchmarks/ai/corpus/diag/cNN`。問いは JSON の `prompt`（英語）の要約。

| ID | 問い | 正解の根拠（`verify` が確かめる） | コーパス |
| --- | --- | --- | --- |
| A-01 | `Array.fold` の署名 | `std/Array.tz` の `def fold ::` の 1 行 | `std` |
| A-02 | `Maybe.default_value` の署名 | `std/Maybe.tc` の `def default_value ::`。言語リファレンス `maybe.md` の表にも載る（`baseline-docs` あり） | `std` |
| A-03 | `Map.insert` の署名 | `std/Map.tz` の `def insert ::`（`HashMap`・`Set` にも同名があり、grep は複数ヒット） | `std` |
| A-04 | `String.split` の署名 | `std/String.tz` の `def split ::`（`Utf8String`・`Regex` にも同名） | `std` |
| A-05 | record `Item` のフィールドと型 | `shop/Catalog.tz` の `record Item {` の 1 行 | `shop` |
| A-06 | union `Shape` の case | `tests/fixtures/unions/Main.tz` の `union Shape =` から 4 行 | `tests/fixtures/unions` |
| B-01 | `Utf8String.from_string` を呼ぶ std の関数 | ルールで導出した 7 件（`Cbor.numeral_of`・`Cbor.text_item`・`IO.try_output`・`Json.encode_number`・`Net.ascii_bytes`・`Net.resolve_with`・`Os.encode`）。呼び出し元は別モジュールで、名前は修飾して現れる | `std` |
| B-02 | `Char.to_u16` を呼ぶ std の関数 | 6 件（`BigInt.of_string`・`Char.to_ascii_lower`・`Char.to_ascii_upper`・`Gen.string_of`・`String.to_ascii_lower`・`String.to_ascii_upper`） | `std` |
| B-03 | `Utf8String.decode_at` を呼ぶ std の関数 | 6 件（`Regex.scalar8`・`Unicode.decode`・`Utf8String.char_count`・`Utf8String.chars`・`Utf8String.replace`・`Utf8String.split`。同じモジュールの呼び出しは修飾なし） | `std` |
| B-04 | `Array.swap_in` を呼ぶ std の関数 | 4 件（`Array.sort_in_place_insertion`・`_merge`・`_rotate`、`Json.by_key`） | `std` |
| B-05 | `Orders.order_total` を呼ぶ関数 | 2 件（`Orders.total_with_discount`・`Report.summary`） | `shop` |
| B-06 | `Money.percent_of` を呼ぶ関数 | 2 件（`Money.with_discount`・`Money.with_tax`。どちらも 1 行の定義で、grep の出力だけで足りる） | `shop` |
| C-01 | E1012（移動後の使用）を直す | 壊れたソースで `check` の先頭が `E1012`・37 行目。`fix` で `check` が空になる（原因は 6 行目の `heading` が `string` を値で取ること） | `diag/c01` |
| C-02 | E1014（生きている `ref mut` との競合）を直す | `E1014`・28 行目。`fix` は `watcher` の作成を `snapshot` の後ろへ移す | `diag/c02` |
| C-03 | E1003（型の不一致）を直す | `E1003`・38 行目（2 件）。`fix` は引数の順序を入れ替える。`first_try`（`to_string` で包む）は誤り | `diag/c03` |
| C-04 | E0002（構文）を直す | `E0002`・12 行目（`pair.0`）。`fix` は `match` に書き換える。`first_try`（`pair[0]`）は誤り | `diag/c04` |
| C-05 | E1013（寿命）を直す | `E1013`・31 行目（ローカルへの `ref` を返す）。`fix` は値を返す。`first_try`（リテラルへの `ref`）は誤り | `diag/c05` |
| C-06 | E1021（網羅されない `match`）を直す | `E1021`・41 行目。`fix` は `Square _` の腕を足す | `diag/c06` |
| C-07 | E1005（`Eq` の instance がない）を直す | `E1005`・27 行目。`fix` は `record Point` に `deriving (Eq)` | `diag/c07` |
| D-01 | 型 `Ord<'a> => ref ['a] -> ['a]` の関数 | `std/Array.tz` の `def sort ::`（`Array.sort`） | `std` |
| D-02 | 型 `Copy<'a> => i64 -> ref ['a] -> Maybe<'a>` の関数 | `std/Array.tz` の `def get ::`（`Array.get`）。grep は型の断片だけで探す | `std` |
| D-03 | 型 `ref string -> ref string -> [string]` の関数 | `std/String.tz` の `def split ::`（`String.split`） | `std` |
| D-04 | 型 `'e -> Maybe<'a> -> Result<'a, 'e>` の関数 | `std/Maybe.tc` の `def to_result ::`（`Maybe.to_result`）。grep は断片 | `std` |
| D-05 | 型 `string -> IO<Result<string, Os.Error>>` の関数 | `std/File.tz` の `def read_text ::`（`File.read_text`）。grep は断片 | `std` |
| D-06 | 型 `'key -> ref HashMap<'key, 'value> -> bool` の関数 | `std/HashMap.tz` の `def contains_key ::` | `std` |
| E-01 | `-7 / 2` と `-7 % 2` の値 | `values-and-functions/op-and-expressions.md` の「整数の除算と剰余」の 1 行（`-3`・`-1`） | `_tsuzuri/language-reference`・`docs/language.md` |
| E-02 | `for i in a .. b` は `b` を含むか | `loops-and-conditionals/for-in.md` の「配列の添字は両端を含むので」の行 | 同上 |
| E-03 | シフト量がビット幅以上のとき | `op-and-expressions.md` の「シフト量はビット幅でマスクされます」の行（`count & 31`） | 同上 |
| E-04 | 接尾辞のない整数リテラルの既定の型 | `types-and-type-inference/basic-types.md` の表の `i32` の行（「整数リテラルの既定型」） | 同上 |
| E-05 | `NaN as i64` の値 | `types-and-type-inference/cast.md` の「`NaN` の扱い」の行（`0`） | 同上 |
| E-06 | `==` を record で使うには | `built-in-types-and-modules/record.md` の `deriving (Eq, Ord, Display, Hash, Default)` の行 | 同上 |
| F-01 | `Money.pad2` を変えたとき影響を受けるテスト | ルールで導出した 3 件 | `shop` |
| F-02 | `Money.percent_of` | 10 件 | `shop` |
| F-03 | `Catalog.position` | 7 件 | `shop` |
| F-04 | `Stock.available` | 4 件 | `shop` |
| F-05 | `Stock.take` | 1 件 | `shop` |
| F-06 | `Orders.line_total` | 5 件 | `shop` |

## 付録 B: タスクの JSON

各ファイルは `{"schema":"tz-ai-task/1","type":"<型名>","tasks":[ … ]}`。1 行 1 タスク。キーの意味は本文の「タスクの JSON」。`needs` は `[パス, 一致させる部分文字列, 前の行数, 後ろの行数]`。`kb`・`find` は Phase 3 が使う（Phase 1・2 は読まない）。

`benchmarks/ai/tasks/a-definition.json`:

```json
{"schema":"tz-ai-task/1","type":"definition","tasks":[
  {"id":"A-01","type":"definition","prompt":"What is the exact signature of Array.fold? Reply with the def line.","corpus":["std"],"grep":["def fold ::"],"needs":[["std/Array.tz","def fold ::",0,0]],"answer":["def fold :: Copy<'a> => ('state -> 'a -> 'state) -> 'state -> ref ['a] -> 'state"],"kb":{"id":"Array.fold"}},
  {"id":"A-02","type":"definition","prompt":"What is the exact signature of Maybe.default_value? Reply with the def line.","corpus":["std"],"grep":["def default_value ::"],"needs":[["std/Maybe.tc","def default_value ::",0,0]],"answer":["def default_value :: 'a -> Maybe<'a> -> 'a"],"docs":{"needs":[["_tsuzuri/language-reference/built-in-types-and-modules/maybe.md","| `Maybe.default_value` |",0,0]]},"kb":{"id":"Maybe.default_value"}},
  {"id":"A-03","type":"definition","prompt":"What is the exact signature of Map.insert? Reply with the def line.","corpus":["std"],"grep":["def insert ::"],"needs":[["std/Map.tz","def insert ::",0,0]],"answer":["def insert :: Ord<'key> => 'key -> 'value -> Map<'key, 'value> -> Map<'key, 'value>"],"kb":{"id":"Map.insert"}},
  {"id":"A-04","type":"definition","prompt":"What is the exact signature of String.split? Reply with the def line.","corpus":["std"],"grep":["def split ::"],"needs":[["std/String.tz","def split ::",0,0]],"answer":["def split :: ref string -> ref string -> [string]"],"kb":{"id":"String.split"}},
  {"id":"A-05","type":"definition","prompt":"Which fields does the record Item have, and what are their types?","corpus":["benchmarks/ai/corpus/shop"],"grep":["record Item"],"needs":[["benchmarks/ai/corpus/shop/Catalog.tz","record Item {",0,0]],"answer":["record Item { id: i64, name: string, price: i64, tax_percent: i64 }"],"kb":{"id":"Catalog.Item"}},
  {"id":"A-06","type":"definition","prompt":"Which cases does the union Shape have in the unions fixture, and what do they carry?","corpus":["tests/fixtures/unions"],"grep":["union Shape"],"needs":[["tests/fixtures/unions/Main.tz","union Shape =",0,3]],"answer":["| Circle of f64","| Rect of f64 * f64","| Empty"],"kb":{"id":"Main.Shape"}}
]}
```

`benchmarks/ai/tasks/b-callers.json`（`grep`・`needs` は書かない。`symbol` から導出する）:

```json
{"schema":"tz-ai-task/1","type":"callers","tasks":[
  {"id":"B-01","type":"callers","prompt":"Which std functions call Utf8String.from_string? Reply with Module.function names.","corpus":["std"],"symbol":"Utf8String.from_string","expect":["Cbor.numeral_of","Cbor.text_item","IO.try_output","Json.encode_number","Net.ascii_bytes","Net.resolve_with","Os.encode"]},
  {"id":"B-02","type":"callers","prompt":"Which std functions call Char.to_u16? Reply with Module.function names.","corpus":["std"],"symbol":"Char.to_u16","expect":["BigInt.of_string","Char.to_ascii_lower","Char.to_ascii_upper","Gen.string_of","String.to_ascii_lower","String.to_ascii_upper"]},
  {"id":"B-03","type":"callers","prompt":"Which std functions call Utf8String.decode_at? Reply with Module.function names.","corpus":["std"],"symbol":"Utf8String.decode_at","expect":["Regex.scalar8","Unicode.decode","Utf8String.char_count","Utf8String.chars","Utf8String.replace","Utf8String.split"]},
  {"id":"B-04","type":"callers","prompt":"Which std functions call Array.swap_in? Reply with Module.function names.","corpus":["std"],"symbol":"Array.swap_in","expect":["Array.sort_in_place_insertion","Array.sort_in_place_merge","Array.sort_in_place_rotate","Json.by_key"]},
  {"id":"B-05","type":"callers","prompt":"Which functions of the shop project call Orders.order_total? Reply with Module.function names.","corpus":["benchmarks/ai/corpus/shop"],"symbol":"Orders.order_total","expect":["Orders.total_with_discount","Report.summary"]},
  {"id":"B-06","type":"callers","prompt":"Which functions of the shop project call Money.percent_of? Reply with Module.function names.","corpus":["benchmarks/ai/corpus/shop"],"symbol":"Money.percent_of","expect":["Money.with_discount","Money.with_tax"]}
]}
```

`benchmarks/ai/tasks/c-diagnostic.json`（Phase 2。`project` は `benchmarks/ai/corpus/diag/cNN`。`fix`・`first_try` は `{path, find, replace}` の列で、`find` は元のファイルにちょうど 1 回現れる）:

```json
{"schema":"tz-ai-task/1","type":"diagnostic","tasks":[
  {"id":"C-01","type":"diagnostic","prompt":"`tsuzuri check` reports E1012. Fix the program so that it type-checks.","project":"benchmarks/ai/corpus/diag/c01","expect":{"code":"E1012","line":37},"needs":[["Main.tz","def heading :: string -> i64 -> string",0,0],["Main.tz","let title = heading entry.name rank",0,0],["Main.tz","by {entry.name}",0,0]],"first_try":[{"path":"Main.tz","find":"heading entry.name rank","replace":"heading (ref entry.name) rank"}],"fix":[{"path":"Main.tz","find":"def heading :: string -> i64 -> string","replace":"def heading :: ref string -> i64 -> string"}]},
  {"id":"C-02","type":"diagnostic","prompt":"`tsuzuri check` reports E1014. Fix the program so that it type-checks and keeps its meaning.","project":"benchmarks/ai/corpus/diag/c02","expect":{"code":"E1014","line":28},"needs":[["Main.tz","let watcher = ref mut hits",0,5]],"fix":[{"path":"Main.tz","find":"    let watcher = ref mut hits\n    misses = misses + 3\n    let snapshot = hits\n","replace":"    misses = misses + 3\n    let snapshot = hits\n    let watcher = ref mut hits\n"}]},
  {"id":"C-03","type":"diagnostic","prompt":"`tsuzuri check` reports E1003 twice at the same call. Fix the program so that it type-checks.","project":"benchmarks/ai/corpus/diag/c03","expect":{"code":"E1003","line":38},"needs":[["Main.tz","def format_row :: string -> i64 -> string",0,0],["Main.tz","format_row (double 21) \"answer\"",0,0]],"first_try":[{"path":"Main.tz","find":"format_row (double 21) \"answer\"","replace":"format_row (to_string (double 21)) \"answer\""}],"fix":[{"path":"Main.tz","find":"format_row (double 21) \"answer\"","replace":"format_row \"answer\" (double 21)"}]},
  {"id":"C-04","type":"diagnostic","prompt":"`tsuzuri check` reports E0002. Fix the program so that it type-checks.","project":"benchmarks/ai/corpus/diag/c04","expect":{"code":"E0002","line":12},"needs":[["Main.tz","def bounds ::",0,0]],"first_try":[{"path":"Main.tz","find":"start: pair.0, stop: pair.1","replace":"start: pair[0], stop: pair[1]"}],"fix":[{"path":"Main.tz","find":"def bounds :: (i64 * i64) -> Span = \\pair -> Span { start: pair.0, stop: pair.1 }","replace":"def bounds :: (i64 * i64) -> Span = \\pair ->\n    match pair with\n    | (low, high) -> Span { start: low, stop: high }"}]},
  {"id":"C-05","type":"diagnostic","prompt":"`tsuzuri check` reports E1013. Fix the program so that it type-checks.","project":"benchmarks/ai/corpus/diag/c05","expect":{"code":"E1013","line":31},"needs":[["Main.tz","def placeholder :: unit -> ref string",0,3]],"first_try":[{"path":"Main.tz","find":"    ref text","replace":"    ref \"untitled\""}],"fix":[{"path":"Main.tz","find":"def placeholder :: unit -> ref string = \\() ->\n    let text = \"untitled\"\n    ref text","replace":"def placeholder :: unit -> string = \\() -> \"untitled\""}]},
  {"id":"C-06","type":"diagnostic","prompt":"`tsuzuri check` reports E1021. Fix the program so that it type-checks.","project":"benchmarks/ai/corpus/diag/c06","expect":{"code":"E1021","line":41},"needs":[["Main.tz","def sides :: Shape -> i64",0,6]],"fix":[{"path":"Main.tz","find":"    | Rect _ -> 4\n    | Triangle _ -> 3","replace":"    | Rect _ -> 4\n    | Triangle _ -> 3\n    | Square _ -> 4"}]},
  {"id":"C-07","type":"diagnostic","prompt":"`tsuzuri check` reports E1005. Fix the program so that it type-checks.","project":"benchmarks/ai/corpus/diag/c07","expect":{"code":"E1005","line":27},"needs":[["Main.tz","record Point { x: i64, y: i64 }",0,0],["Main.tz","if points[i] == target then count = count + 1",0,0]],"fix":[{"path":"Main.tz","find":"record Point { x: i64, y: i64 }\n","replace":"record Point { x: i64, y: i64 } deriving (Eq)\n"}]}
]}
```

`benchmarks/ai/tasks/d-type-search.json`（`find` は Phase 3 の `kb find` の型の問い合わせ）:

```json
{"schema":"tz-ai-task/1","type":"type-search","tasks":[
  {"id":"D-01","type":"type-search","prompt":"Which std function has the type `Ord<'a> => ref ['a] -> ['a]`? Reply with Module.function.","corpus":["std"],"grep":["Ord<'a> => ref ['a] -> ['a]"],"needs":[["std/Array.tz","def sort :: Ord<'a> => ref ['a] -> ['a]",0,0]],"answer":["def sort :: Ord<'a> => ref ['a] -> ['a]"],"find":"ref ['a] -> ['a]"},
  {"id":"D-02","type":"type-search","prompt":"Which std function has the type `Copy<'a> => i64 -> ref ['a] -> Maybe<'a>`? Reply with Module.function.","corpus":["std"],"grep":["i64 -> ref ['a] -> Maybe"],"needs":[["std/Array.tz","def get :: Copy<'a> => i64 -> ref ['a] -> Maybe<'a>",0,0]],"answer":["def get :: Copy<'a> => i64 -> ref ['a] -> Maybe<'a>"],"find":"i64 -> ref ['a] -> Maybe<'a>"},
  {"id":"D-03","type":"type-search","prompt":"Which std function has the type `ref string -> ref string -> [string]`? Reply with Module.function.","corpus":["std"],"grep":["ref string -> ref string -> [string]"],"needs":[["std/String.tz","def split :: ref string -> ref string -> [string]",0,0]],"answer":["def split :: ref string -> ref string -> [string]"],"find":"ref string -> ref string -> [string]"},
  {"id":"D-04","type":"type-search","prompt":"Which std function has the type `'e -> Maybe<'a> -> Result<'a, 'e>`? Reply with Module.function.","corpus":["std"],"grep":["Maybe<'a> -> Result"],"needs":[["std/Maybe.tc","def to_result :: 'e -> Maybe<'a> -> Result<'a, 'e>",0,0]],"answer":["def to_result :: 'e -> Maybe<'a> -> Result<'a, 'e>"],"find":"'e -> Maybe<'a> -> Result<'a, 'e>"},
  {"id":"D-05","type":"type-search","prompt":"Which std function has the type `string -> IO<Result<string, Os.Error>>`? Reply with Module.function.","corpus":["std"],"grep":["IO<Result<string"],"needs":[["std/File.tz","def read_text :: string -> IO<Result<string, Os.Error>>",0,0]],"answer":["def read_text :: string -> IO<Result<string, Os.Error>>"],"find":"string -> IO<Result<string, Os.Error>>"},
  {"id":"D-06","type":"type-search","prompt":"Which std function has the type `'key -> ref HashMap<'key, 'value> -> bool` (with Hash and Eq constraints)? Reply with Module.function.","corpus":["std"],"grep":["'key -> ref HashMap<'key, 'value> -> bool"],"needs":[["std/HashMap.tz","def contains_key :: (Hash<'key>, Eq<'key>) => 'key -> ref HashMap<'key, 'value> -> bool",0,0]],"answer":["def contains_key :: (Hash<'key>, Eq<'key>) => 'key -> ref HashMap<'key, 'value> -> bool"],"find":"'key -> ref HashMap<'key, 'value> -> bool"}
]}
```

`benchmarks/ai/tasks/e-spec.json`（コーパスと答えは日本語の文書。`grep` も日本語の固定文字列）:

```json
{"schema":"tz-ai-task/1","type":"spec","tasks":[
  {"id":"E-01","type":"spec","prompt":"What does the integer expression -7 / 2 evaluate to, and what is -7 % 2?","corpus":["_tsuzuri/language-reference","docs/language.md"],"grep":["ゼロ方向へ切り捨て"],"needs":[["_tsuzuri/language-reference/values-and-functions/op-and-expressions.md","**整数の除算と剰余**",0,0]],"answer":["`-7 / 2` は `-3`","`-7 % 2` は `-1`"],"docs":{"needs":[["_tsuzuri/language-reference/values-and-functions/op-and-expressions.md","**整数の除算と剰余**",0,0]]}},
  {"id":"E-02","type":"spec","prompt":"Does `for i in a .. b do` include b, and what is the right loop bound for an array index?","corpus":["_tsuzuri/language-reference","docs/language.md"],"grep":["両端を含"],"needs":[["_tsuzuri/language-reference/loops-and-conditionals/for-in.md","配列の添字は両端を含むので",0,0]],"answer":["0 .. (words.length - 1)"],"docs":{"needs":[["_tsuzuri/language-reference/loops-and-conditionals/for-in.md","配列の添字は両端を含むので",0,0]]}},
  {"id":"E-03","type":"spec","prompt":"What happens when a shift count is not smaller than the bit width of the integer type?","corpus":["_tsuzuri/language-reference","docs/language.md"],"grep":["シフト量"],"needs":[["_tsuzuri/language-reference/values-and-functions/op-and-expressions.md","シフト量はビット幅でマスクされます",0,0]],"answer":["count & 31"],"docs":{"needs":[["_tsuzuri/language-reference/values-and-functions/op-and-expressions.md","シフト量はビット幅でマスクされます",0,0]]}},
  {"id":"E-04","type":"spec","prompt":"What is the type of an integer literal without a suffix when the context does not fix the type?","corpus":["_tsuzuri/language-reference","docs/language.md"],"grep":["整数リテラルの既定型"],"needs":[["_tsuzuri/language-reference/types-and-type-inference/basic-types.md","整数リテラルの既定型",0,0]],"answer":["整数リテラルの既定型"],"docs":{"needs":[["_tsuzuri/language-reference/types-and-type-inference/basic-types.md","整数リテラルの既定型",0,0]]}},
  {"id":"E-05","type":"spec","prompt":"What integer does `NaN as i64` produce?","corpus":["_tsuzuri/language-reference","docs/language.md"],"grep":["NaN"],"needs":[["_tsuzuri/language-reference/types-and-type-inference/cast.md","`NaN` の扱い",0,0]],"answer":["必ず **`0`** に変換されます"],"docs":{"needs":[["_tsuzuri/language-reference/types-and-type-inference/cast.md","`NaN` の扱い",0,0]]}},
  {"id":"E-06","type":"spec","prompt":"How do you make == work on a record type?","corpus":["_tsuzuri/language-reference","docs/language.md"],"grep":["deriving (Eq"],"needs":[["_tsuzuri/language-reference/built-in-types-and-modules/record.md","で比較、表示、ハッシュ、既定値を",0,0]],"answer":["deriving (Eq, Ord, Display, Hash, Default)"],"docs":{"needs":[["_tsuzuri/language-reference/built-in-types-and-modules/record.md","で比較、表示、ハッシュ、既定値を",0,0]]}}
]}
```

`benchmarks/ai/tasks/f-impact.json`（`grep`・`needs` は `change` から導出する）:

```json
{"schema":"tz-ai-task/1","type":"impact","tasks":[
  {"id":"F-01","type":"impact","prompt":"Which tests of the shop project can be affected by a change to Money.pad2? Reply with Module.test names.","corpus":["benchmarks/ai/corpus/shop"],"change":"Money.pad2","expect":["Money.show pads the cents","Report.discounted shows the reduced total","Report.summary shows the order total"]},
  {"id":"F-02","type":"impact","prompt":"Which tests of the shop project can be affected by a change to Money.percent_of? Reply with Module.test names.","corpus":["benchmarks/ai/corpus/shop"],"change":"Money.percent_of","expect":["Catalog.gross includes tax","Catalog.price_of an unknown id is zero","Money.percent_of rounds toward zero","Money.with_discount subtracts the percentage","Money.with_tax adds the percentage","Orders.line_total multiplies the gross price","Orders.order_total adds the lines","Orders.total_with_discount takes the percentage off","Report.discounted shows the reduced total","Report.summary shows the order total"]},
  {"id":"F-03","type":"impact","prompt":"Which tests of the shop project can be affected by a change to Catalog.position? Reply with Module.test names.","corpus":["benchmarks/ai/corpus/shop"],"change":"Catalog.position","expect":["Catalog.position finds the item","Catalog.price_of an unknown id is zero","Orders.line_total multiplies the gross price","Orders.order_total adds the lines","Orders.total_with_discount takes the percentage off","Report.discounted shows the reduced total","Report.summary shows the order total"]},
  {"id":"F-04","type":"impact","prompt":"Which tests of the shop project can be affected by a change to Stock.available? Reply with Module.test names.","corpus":["benchmarks/ai/corpus/shop"],"change":"Stock.available","expect":["Orders.fillable needs every line in stock","Stock.available sums the levels of one id","Stock.can_fill compares with the available count","Stock.take reduces the count"]},
  {"id":"F-05","type":"impact","prompt":"Which tests of the shop project can be affected by a change to Stock.take? Reply with Module.test names.","corpus":["benchmarks/ai/corpus/shop"],"change":"Stock.take","expect":["Stock.take reduces the count"]},
  {"id":"F-06","type":"impact","prompt":"Which tests of the shop project can be affected by a change to Orders.line_total? Reply with Module.test names.","corpus":["benchmarks/ai/corpus/shop"],"change":"Orders.line_total","expect":["Orders.line_total multiplies the gross price","Orders.order_total adds the lines","Orders.total_with_discount takes the percentage off","Report.discounted shows the reduced total","Report.summary shows the order total"]}
]}
```

## 付録 C: コーパスの全文

### C.1 `benchmarks/ai/corpus/shop/`（6 ファイル。`tsuzuri check`・`tsuzuri test`・`tsuzuri run` が通る。テスト 16 件）

HEAD で `tsuzuri test` は `16 passed; 0 failed; 0 ignored`、`tsuzuri run` は次の 3 行を出した:

```text
total 11.88
after 10%: 10.70
backorder
```

`Money.tz`:

```tsuzuri
/// Applies a percentage to an amount in cents, rounding toward zero.
def percent_of :: i64 -> i64 -> i64 = \cents percent -> cents * percent / 100

def with_tax :: i64 -> i64 -> i64 = \cents percent -> cents + percent_of cents percent

def with_discount :: i64 -> i64 -> i64 = \cents percent -> cents - percent_of cents percent

def pad2 :: i64 -> string = \n -> if n < 10 then $"0{n}" else to_string n

def show :: i64 -> string = \cents -> $"{cents / 100}.{pad2 (cents % 100)}"

test "percent_of rounds toward zero" = assert (percent_of 999 10 == 99)
test "with_tax adds the percentage" = assert (with_tax 1000 8 == 1080)
test "with_discount subtracts the percentage" = assert (with_discount 1000 25 == 750)
test "show pads the cents" = assert (show 1205 == "12.05")
```

`Catalog.tz`:

```tsuzuri
record Item { id: i64, name: string, price: i64, tax_percent: i64 }

def make :: i64 -> string -> i64 -> Item = \id name price ->
    Item { id: id, name: name, price: price, tax_percent: 8 }

/// The price of one item with its tax, in cents.
def gross :: ref Item -> i64 = \item -> Money.with_tax item.price item.tax_percent

def position :: ref [Item] -> i64 -> i64 = \items id ->
    let mut found = -1
    for i in 0 .. items.length - 1 do
        if items[i].id == id then found = i
    found

def price_of :: ref [Item] -> i64 -> i64 = \items id ->
    let at = position items id
    if at < 0 then 0 else gross (ref items[at])

test "gross includes tax" = assert (gross (ref (make 1 "pen" 200)) == 216)
test "position finds the item" =
    let items = [make 1 "pen" 200, make 2 "ink" 500]
    assert (position items 2 == 1)
test "price_of an unknown id is zero" =
    let items = [make 1 "pen" 200]
    assert (price_of items 9 == 0)
```

`Stock.tz`:

```tsuzuri
record Level { id: i64, count: i64 }

def available :: ref [Level] -> i64 -> i64 = \levels id ->
    let mut total = 0
    for level in levels do
        if level.id == id then total = total + level.count
    total

def can_fill :: ref [Level] -> i64 -> i64 -> bool = \levels id qty -> available levels id >= qty

def take :: [Level] -> i64 -> i64 -> [Level] = \levels id qty ->
    Array.map (\level -> if level.id == id then Level { id: level.id, count: level.count - qty } else level) levels

test "available sums the levels of one id" =
    let levels = [Level { id: 1, count: 4 }, Level { id: 2, count: 1 }, Level { id: 1, count: 3 }]
    assert (available levels 1 == 7)
test "can_fill compares with the available count" =
    let levels = [Level { id: 1, count: 4 }]
    assert (can_fill levels 1 4 && !(can_fill levels 1 5))
test "take reduces the count" =
    let levels = take [Level { id: 1, count: 4 }] 1 3
    assert (available levels 1 == 1)
```

`Orders.tz`:

```tsuzuri
record Line { id: i64, qty: i64 }

def line_total :: ref [Catalog.Item] -> Line -> i64 = \items line ->
    Catalog.price_of items line.id * line.qty

def order_total :: ref [Catalog.Item] -> ref [Line] -> i64 = \items lines ->
    let mut sum = 0
    for line in lines do
        sum = sum + line_total items line
    sum

def total_with_discount :: ref [Catalog.Item] -> ref [Line] -> i64 -> i64 = \items lines percent ->
    Money.with_discount (order_total items lines) percent

def fillable :: ref [Stock.Level] -> ref [Line] -> bool = \levels lines ->
    let mut ok = true
    for line in lines do
        if !(Stock.can_fill levels line.id line.qty) then ok = false
    ok

test "line_total multiplies the gross price" =
    let items = [Catalog.make 1 "pen" 200]
    assert (line_total items (Line { id: 1, qty: 3 }) == 648)
test "order_total adds the lines" =
    let items = [Catalog.make 1 "pen" 200, Catalog.make 2 "ink" 500]
    assert (order_total items [Line { id: 1, qty: 1 }, Line { id: 2, qty: 2 }] == 1296)
test "total_with_discount takes the percentage off" =
    let items = [Catalog.make 1 "pen" 200]
    assert (total_with_discount items [Line { id: 1, qty: 5 }] 50 == 540)
test "fillable needs every line in stock" =
    let levels = [Stock.Level { id: 1, count: 2 }]
    assert (fillable levels [Line { id: 1, qty: 2 }] && !(fillable levels [Line { id: 1, qty: 3 }]))
```

`Report.tz`:

```tsuzuri
def summary :: ref [Catalog.Item] -> ref [Orders.Line] -> string = \items lines ->
    $"total {Money.show (Orders.order_total items lines)}"

def discounted :: ref [Catalog.Item] -> ref [Orders.Line] -> i64 -> string = \items lines percent ->
    $"after {percent}%: {Money.show (Orders.total_with_discount items lines percent)}"

test "summary shows the order total" =
    let items = [Catalog.make 1 "pen" 200]
    assert (summary items [Orders.Line { id: 1, qty: 2 }] == "total 4.32")
test "discounted shows the reduced total" =
    let items = [Catalog.make 1 "pen" 200]
    assert (discounted items [Orders.Line { id: 1, qty: 5 }] 50 == "after 50%: 5.40")
```

`Main.tz`:

```tsuzuri
def main :: unit -> i32 = \() ->
    let items = [Catalog.make 1 "pen" 200, Catalog.make 2 "ink" 500]
    let lines = [Orders.Line { id: 1, qty: 3 }, Orders.Line { id: 2, qty: 1 }]
    let levels = [Stock.Level { id: 1, count: 10 }, Stock.Level { id: 2, count: 0 }]
    do! IO.writeln (Report.summary items lines)
    do! IO.writeln (Report.discounted items lines 10)
    do! IO.writeln (if Orders.fillable levels lines then "in stock" else "backorder")
    0
```

### C.2 `benchmarks/ai/corpus/diag/cNN/Main.tz`（7 ファイル。各 1 つの意図した誤りを持つ）

HEAD の `tsuzuri check . --json`（各ディレクトリで実行）の先頭の診断は、`c01` が `E1012` 37 行、`c02` が `E1014` 28 行、`c03` が `E1003` 38 行（2 件）、`c04` が `E0002` 12 行、`c05` が `E1013` 31 行、`c06` が `E1021` 41 行、`c07` が `E1005` 27 行。付録 B の `fix` を当てると 7 件とも出力が空になる（終了コード 0）。

`c01/Main.tz`:

```tsuzuri
record Entry { name: string, score: i64 }

def make :: string -> i64 -> Entry = \name score -> Entry { name: name, score: score }

/// Builds the heading line of an entry. The name is consumed.
def heading :: string -> i64 -> string = \name rank -> $"{rank}. {name}"

def grade :: i64 -> string = \score ->
    if score >= 90 then "A"
    elif score >= 70 then "B"
    else "C"

def total :: ref [Entry] -> i64 = \entries ->
    let mut sum = 0
    for i in 0 .. entries.length - 1 do
        sum = sum + entries[i].score
    sum

def average :: ref [Entry] -> i64 = \entries ->
    if entries.length == 0 then 0 else total entries / entries.length

def best :: ref [Entry] -> i64 = \entries ->
    let mut top = 0
    for i in 0 .. entries.length - 1 do
        if entries[i].score > top then top = entries[i].score
    top

def passed :: ref [Entry] -> i64 -> i64 = \entries limit ->
    let mut count = 0
    for i in 0 .. entries.length - 1 do
        if entries[i].score >= limit then count = count + 1
    count

def describe :: Entry -> i64 -> string = \entry rank ->
    let title = heading entry.name rank
    let letter = grade entry.score
    $"{title} ({letter}) by {entry.name}"

def main :: unit -> i32 = \() ->
    let entries = [make "ada" 95, make "bob" 72, make "cy" 40]
    do! IO.writeln $"avg {average entries} best {best entries} passed {passed entries 70}"
    do! IO.writeln (describe (make "ada" 95) 1)
    0
```

`c02/Main.tz`:

```tsuzuri
def record_hit :: ref mut i64 -> unit = \counter ->
    deref counter = deref counter + 1

def clamp :: i64 -> i64 -> i64 -> i64 = \low high value ->
    if value < low then low
    elif value > high then high
    else value

def percent :: i64 -> i64 -> i64 = \part whole ->
    if whole == 0 then 0 else part * 100 / whole

def bar :: i64 -> string = \percent_value ->
    let filled = clamp 0 10 (percent_value / 10)
    let mut text = ""
    for i in 1 .. 10 do
        text = text + (if i <= filled then "#" else ".")
    text

def summarize :: i64 -> i64 -> string = \hits misses ->
    let rate = percent hits (hits + misses)
    $"{bar rate} {rate}%"

def run :: unit -> string = \() ->
    let mut hits = 0
    let mut misses = 0
    let watcher = ref mut hits
    misses = misses + 3
    let snapshot = hits
    record_hit watcher
    record_hit watcher
    summarize snapshot misses

def main :: unit -> i32 = \() ->
    do! IO.writeln (run ())
    0
```

`c03/Main.tz`:

```tsuzuri
record Row { label: string, value: i64 }

def make_row :: string -> i64 -> Row = \label value -> Row { label: label, value: value }

def pad_left :: string -> i64 -> string = \text width ->
    let mut out = text
    while out.length < width do
        out = " " + out
    out

def format_row :: string -> i64 -> string = \label value ->
    $"{pad_left label 8}: {value}"

def render :: ref Row -> string = \row -> format_row (clone_string row.label) row.value

def double :: i64 -> i64 = \n -> n * 2

def triple :: i64 -> i64 = \n -> n * 3

def scale_all :: [i64] -> i64 -> [i64] = \values factor ->
    Array.map (\v -> v * factor) values

def checksum :: ref [i64] -> i64 = \values ->
    let mut acc = 0
    for i in 0 .. values.length - 1 do
        acc = (acc * 31 + values[i]) % 1000003
    acc

def report :: ref [Row] -> string = \rows ->
    let mut lines = ""
    for i in 0 .. rows.length - 1 do
        lines = lines + render (ref rows[i]) + "\n"
    lines

def main :: unit -> i32 = \() ->
    let rows = [make_row "alpha" 3, make_row "beta" 14]
    do! IO.write (report rows)
    do! IO.writeln (format_row (double 21) "answer")
    do! IO.writeln (checksum (scale_all [1, 2, 3] (triple 2)))
    0
```

`c04/Main.tz`:

```tsuzuri
record Span { start: i64, stop: i64 }

def length_of :: Span -> i64 = \span -> span.stop - span.start

def overlaps :: Span -> Span -> bool = \a b -> a.start < b.stop && b.start < a.stop

def merge :: Span -> Span -> Span = \a b ->
    Span { start: Int.min a.start b.start, stop: Int.max a.stop b.stop }

def contains :: Span -> i64 -> bool = \span point -> point >= span.start && point < span.stop

def bounds :: (i64 * i64) -> Span = \pair -> Span { start: pair.0, stop: pair.1 }

def widest :: ref [Span] -> i64 = \spans ->
    let mut best = 0
    for i in 0 .. spans.length - 1 do
        if length_of spans[i] > best then best = length_of spans[i]
    best

def count_covering :: ref [Span] -> i64 -> i64 = \spans point ->
    let mut count = 0
    for i in 0 .. spans.length - 1 do
        if contains spans[i] point then count = count + 1
    count

def main :: unit -> i32 = \() ->
    let a = bounds (2, 9)
    let b = Span { start: 5, stop: 12 }
    let spans = [a, b, merge a b]
    do! IO.writeln $"{widest spans} {count_covering spans 6} {overlaps a b}"
    0
```

`c05/Main.tz`:

```tsuzuri
record Note { title: string, text: string, stars: i64 }

def make_note :: string -> string -> Note = \title text ->
    Note { title: title, text: text, stars: 0 }

def star :: Note -> Note = \note ->
    let next = note.stars + 1
    { note with stars = next }

def short :: ref Note -> bool = \note -> note.text.length < 20

def score :: ref Note -> i64 = \note -> note.stars * 10 + note.title.length

def best_index :: ref [Note] -> i64 = \notes ->
    let mut best = 0
    for i in 1 .. notes.length - 1 do
        if score (ref notes[i]) > score (ref notes[best]) then best = i
    best

def title_at :: ref [Note] -> i64 -> ref string = \notes index ->
    ref notes[index].title

def shortest :: ref [Note] -> i64 = \notes ->
    let mut count = 0
    for i in 0 .. notes.length - 1 do
        if short (ref notes[i]) then count = count + 1
    count

def placeholder :: unit -> ref string = \() ->
    let text = "untitled"
    ref text

def main :: unit -> i32 = \() ->
    let notes = [star (make_note "todo" "buy milk"), make_note "ideas" "a long note about many things"]
    do! IO.writeln (Display.display (title_at notes (best_index notes)))
    do! IO.writeln $"{shortest notes} short notes"
    do! IO.writeln (placeholder ())
    0
```

`c06/Main.tz`:

```tsuzuri
union Shape =
    | Circle of i64
    | Rect of i64 * i64
    | Triangle of i64 * i64
    | Square of i64

def area :: Shape -> i64 = \shape ->
    match shape with
    | Circle r -> 3 * r * r
    | Rect (w, h) -> w * h
    | Triangle (b, h) -> b * h / 2
    | Square s -> s * s

def perimeter :: Shape -> i64 = \shape ->
    match shape with
    | Circle r -> 6 * r
    | Rect (w, h) -> 2 * (w + h)
    | Triangle (b, h) -> b + h + Int.max b h
    | Square s -> 4 * s

def name :: Shape -> string = \shape ->
    match shape with
    | Circle _ -> "circle"
    | Rect _ -> "rect"
    | Triangle _ -> "triangle"
    | Square _ -> "square"

def scale :: Shape -> i64 -> Shape = \shape k ->
    match shape with
    | Circle r -> Circle (r * k)
    | Rect (w, h) -> Rect (w * k, h * k)
    | Triangle (b, h) -> Triangle (b * k, h * k)
    | Square s -> Square (s * k)

def is_round :: Shape -> bool = \shape ->
    match shape with
    | Circle _ -> true
    | _ -> false

def sides :: Shape -> i64 = \shape ->
    match shape with
    | Circle _ -> 0
    | Rect _ -> 4
    | Triangle _ -> 3

def main :: unit -> i32 = \() ->
    let shape = scale (Rect (2, 3)) 2
    do! IO.writeln $"{name shape} {area shape} {perimeter shape} {sides shape} {is_round shape}"
    0
```

`c07/Main.tz`:

```tsuzuri
record Point { x: i64, y: i64 }

record Segment { from: Point, to_point: Point }

def origin :: Point = Point { x: 0, y: 0 }

def add :: Point -> Point -> Point = \a b -> Point { x: a.x + b.x, y: a.y + b.y }

def neg :: Point -> Point = \p -> Point { x: 0 - p.x, y: 0 - p.y }

def manhattan :: Point -> i64 = \p -> Int.abs p.x + Int.abs p.y

def length_of :: Segment -> i64 = \s ->
    manhattan (add s.to_point (neg s.from))

def midpoint :: Segment -> Point = \s ->
    Point { x: (s.from.x + s.to_point.x) / 2, y: (s.from.y + s.to_point.y) / 2 }

def shift :: Segment -> Point -> Segment = \s by ->
    Segment { from: add s.from by, to_point: add s.to_point by }

def is_degenerate :: Segment -> bool = \s -> length_of s == 0

def count_at :: ref [Point] -> Point -> i64 = \points target ->
    let mut count = 0
    for i in 0 .. points.length - 1 do
        if points[i] == target then count = count + 1
    count

def main :: unit -> i32 = \() ->
    let points = [Point { x: 1, y: 2 }, origin(), Point { x: 1, y: 2 }]
    let seg = Segment { from: origin(), to_point: Point { x: 4, y: 6 } }
    do! IO.writeln $"{count_at points (Point { x: 1, y: 2 })} {length_of seg} {is_degenerate (shift seg origin())}"
    0
```

## 付録 D: 検証の記録

試作の実行器（付録の仕様どおりに書いた約 300 行の Node のスクリプト。チケットには全文を載せない）で、次を確かめた。

| 確認 | 結果 |
| --- | --- |
| 付録 B の 37 タスクの `needs`・`answer` | すべて一意に解決し、`answer` の文字列が `needs` の本文に含まれる |
| `callers`・`impact` の導出（12 タスク） | 導出した一覧が `expect` と一致（`expect B-01 true` … `expect F-06 true`） |
| `diag-v1` の 7 タスク | 壊れたソースの先頭の診断が `expect` と一致。`fix` のあとの `check` の出力が空。`first_try` が付くタスク（C-01・C-03・C-04・C-05）は `first_try` のあとの出力が空でない |
| 行の数 | `run` の出力 104 行（A: 6×3＋docs 1、B: 6×3、D: 6×3、E: 6×3＋docs 6、F: 6×3、C: 7。C を除く Phase 1 だけなら 97 行） |

`diag-v1` の記録（`calls` と `chars` は `check` の出力と読んだ本文の合計。`rounds` は `check` を走らせた回数）:

| タスク | `check` の出力の字数 | 読んだ範囲の字数 | `rounds` | `calls` | `chars` | `est_tokens` |
| --- | --- | --- | --- | --- | --- | --- |
| C-01 | 198 | 1,434（ファイル全体） | 2 | 6 | 1,824 | 456 |
| C-02 | 276 | 962 | 1 | 4 | 1,241 | 311 |
| C-03 | 358 | 1,214 | 2 | 6 | 1,757 | 440 |
| C-04 | 173 | 1,075 | 2 | 6 | 1,666 | 417 |
| C-05 | 210 | 1,249 | 2 | 6 | 1,681 | 421 |
| C-06 | 194 | 1,265 | 1 | 4 | 1,462 | 366 |
| C-07 | 225 | 1,221 | 1 | 4 | 1,449 | 363 |

（`chars` = `check` の出力 ＋ 読んだ範囲 ＋ `edit` の `ok\n`（3 字）× 試行 ＋ 試行ごとの `check` の出力。7 つのファイルがすべて 80 行未満なので、±40 行の窓は常にファイル全体になる。）

**独立した参照（golden）**: コーパスが `benchmarks/ai/corpus/shop` のタスクの値を、実行器を使わずに、シェルの `grep -rnF <語> <dir> | sed（パスの接頭辞を論理パスにする） | LC_ALL=C sort -t: -k1,1 -k2,2n | wc -m` と `wc -m <ファイル>`（`LC_ALL=en_US.UTF-8`）の足し算で再計算し、すべて一致した。shop の 5 ファイルは、`Catalog.tz` 941・`Main.tz` 460・`Money.tz` 770・`Orders.tz` 1,394・`Report.tz` 662・`Stock.tz` 956 字。

`baseline-grep-chunk` と `baseline-grep-window`（shop では同じ値。どのファイルも 80 行未満で、±40 行の窓がファイル全体になるため）:

| タスク | `chars` | `calls` | `est_tokens` | 内訳（独立した計算） |
| --- | --- | --- | --- | --- |
| A-05 | 107 | 1 | 27 | `grep record Item` の出力 107 字（必要な行が出力に含まれるので読み足さない） |
| B-05 | 2,567 | 3 | 642 | grep `order_total` 511 ＋ `Orders.tz` 1,394 ＋ `Report.tz` 662 |
| B-06 | 478 | 1 | 120 | grep `percent_of` 478（定義が 1 行なので、出力だけで足りる） |
| F-01 | 2,381 | 6 | 596 | grep `pad2` 225・`show` 593・`discounted` 437・`summary` 395・`main` 69 ＋ `Report.tz` 662 |
| F-02 | 7,652 | 14 | 1,913 | 11 回の grep（`percent_of` 478 ほか。合計 4,655）＋ `Catalog.tz` 941・`Orders.tz` 1,394・`Report.tz` 662 |
| F-03 | 5,902 | 11 | 1,476 | 8 回の grep（合計 2,905）＋ 同じ 3 ファイル |
| F-04 | 3,860 | 6 | 965 | grep `available` 553・`can_fill` 429・`fillable` 459・`main` 69 ＋ `Orders.tz` 1,394・`Stock.tz` 956 |
| F-05 | 358 | 1 | 90 | grep `take` 358 |
| F-06 | 4,312 | 8 | 1,078 | 6 回の grep（合計 2,256）＋ `Orders.tz` 1,394・`Report.tz` 662 |

`baseline-grep`（素朴。grep にヒットしたファイルを全文読む。`calls` = grep の回数 ＋ ヒットしたファイルの数）:

| タスク | `chars` | `calls` | `est_tokens` | 内訳（独立した計算） |
| --- | --- | --- | --- | --- |
| A-05 | 1,048 | 2 | 262 | grep 107 ＋ `Catalog.tz` 941 |
| B-05 | 2,567 | 3 | 642 | 上と同じ（ヒットしたのが `Orders.tz`・`Report.tz`） |
| B-06 | 1,248 | 2 | 312 | grep 478 ＋ `Money.tz` 770 |
| F-01 | 3,611 | 8 | 903 | 5 回の grep（合計 1,719）＋ `Main.tz` 460・`Money.tz` 770・`Report.tz` 662 |
| F-02 | 8,882 | 16 | 2,221 | 11 回の grep（4,655）＋ 5 ファイル（`Catalog.tz`・`Main.tz`・`Money.tz`・`Orders.tz`・`Report.tz` で 4,227） |
| F-03 | 6,362 | 12 | 1,591 | 8 回の grep（2,905）＋ 4 ファイル（`Catalog.tz`・`Main.tz`・`Orders.tz`・`Report.tz` で 3,457） |
| F-04 | 4,320 | 7 | 1,080 | 4 回の grep（1,510）＋ `Main.tz` 460・`Orders.tz` 1,394・`Stock.tz` 956 |
| F-05 | 2,708 | 3 | 677 | grep 358 ＋ `Orders.tz` 1,394・`Stock.tz` 956 |
| F-06 | 4,772 | 9 | 1,193 | 6 回の grep（2,256）＋ `Main.tz` 460・`Orders.tz` 1,394・`Report.tz` 662 |

`std`・文書のコーパスのタスクは、std・文書が変わると値が変わる（`docs/ai-cost.md` に値を書き、テストの期待値にはしない）。HEAD の試作の値（参考。chars / calls）: `baseline-grep-chunk` は A-01 910 / 1、A-06 5,435 / 2、B-01 93,025 / 7、B-02 27,390 / 3、B-03 54,722 / 4、B-04 55,595 / 4、E-05 18,475 / 1。`baseline-grep-window` は A-06 1,094 / 2、B-01 22,493 / 7、B-02 7,777 / 3、B-03 14,043 / 4、B-04 9,250 / 3。`baseline-grep`（素朴）は A-01 118,418 / 10、A-02 7,743 / 3、A-04 72,056 / 4、B-01 136,669 / 6、B-03 169,072 / 6、E-01 375,348 / 5、E-05 672,806 / 26。`baseline-docs` は A-02 14,037 / 2、E-01 15,183 / 2。

**型ごとの下位の中央値**（試作の 104 行。`report` の「型ごとの値」の表と同じ形。`complete` は全行 `true`）:

| 型 | 戦略 | 件数 | chars | calls | est_tokens |
| --- | --- | --- | --- | --- | --- |
| definition | baseline-grep | 6 | 7743 | 3 | 1936 |
| definition | baseline-grep-chunk | 6 | 218 | 1 | 55 |
| definition | baseline-grep-window | 6 | 218 | 1 | 55 |
| definition | baseline-docs | 1 | 14037 | 2 | 6421 |
| callers | baseline-grep | 6 | 34454 | 3 | 8614 |
| callers | baseline-grep-chunk | 6 | 27390 | 3 | 6848 |
| callers | baseline-grep-window | 6 | 7777 | 3 | 1945 |
| diagnostic | diag-v1 | 7 | 1666 | 6 | 417 |
| type-search | baseline-grep | 6 | 9448 | 2 | 2362 |
| type-search | baseline-grep-chunk | 6 | 68 | 1 | 17 |
| type-search | baseline-grep-window | 6 | 68 | 1 | 17 |
| spec | baseline-grep | 6 | 375348 | 5 | 303902 |
| spec | baseline-grep-chunk | 6 | 799 | 1 | 470 |
| spec | baseline-grep-window | 6 | 799 | 1 | 470 |
| spec | baseline-docs | 6 | 15183 | 2 | 7762 |
| impact | baseline-grep | 6 | 4320 | 8 | 1080 |
| impact | baseline-grep-chunk | 6 | 3860 | 6 | 965 |
| impact | baseline-grep-window | 6 | 3860 | 6 | 965 |

**診断の `check --json` の実出力**（各ディレクトリで `tsuzuri check . --json`。stderr。先頭の 1 件）:

```json
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'entry'","path":"./Main.tz","span":{"start":1179,"end":1189,"line":37,"column":30,"end_line":37,"end_column":40}}
{"severity":"error","code":"E1014","message":"access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively","path":"./Main.tz","span":{"start":813,"end":817,"line":28,"column":20,"end_line":28,"end_column":24}}
{"severity":"error","code":"E1003","message":"expected string, found i64","path":"./Main.tz","span":{"start":1123,"end":1134,"line":38,"column":32,"end_line":38,"end_column":43}}
{"severity":"error","code":"E0002","message":"expected an identifier","path":"./Main.tz","span":{"start":465,"end":466,"line":12,"column":65,"end_line":12,"end_column":66}}
{"severity":"error","code":"E1013","message":"borrowed value does not live long enough to leave this block","path":"./Main.tz","span":{"start":936,"end":944,"line":31,"column":5,"end_line":31,"end_column":13}}
{"severity":"error","code":"E1021","message":"match is not exhaustive; missing: Square _","path":"./Main.tz","span":{"start":1013,"end":1089,"line":41,"column":5,"end_line":44,"end_column":22}}
{"severity":"error","code":"E1005","message":"no instance for Eq<Main.Point>; define an instance or use a supported type","path":"./Main.tz","span":{"start":867,"end":886,"line":27,"column":12,"end_line":27,"end_column":31}}
```

（C-03 は同じ呼び出しに 2 件目の `E1003`（`expected i64, found string`、44〜52 列）が続く。）診断の JSON は G22 が `related`・`help`・`fixes` を足すと長くなり、`diag-v2` の値が変わる（それが測りたいこと）。

## 付録 E: 出力と検査の書式

### E.1 `report` の出力（`docs/ai-cost.md` の生成領域と同一）

Markdown。数値は 10 進の整数で桁区切りなし。表は下の順で、データがなければ節ごと省く。節の間は空行 1 つ。最後の行の後ろに改行 1 つ。

```markdown
測定: tsuzuri 0.1.0、コミット 9dd0fbbcf029（未コミットの変更あり）、タスク 37 件、行 104

### 型ごとの値（下位の中央値）

| 型 | 戦略 | 件数 | chars | calls | est_tokens | complete |
| --- | --- | --- | --- | --- | --- | --- |
| definition | baseline-grep | 6 | 7743 | 3 | 1936 | 6/6 |
| definition | baseline-grep-chunk | 6 | 218 | 1 | 55 | 6/6 |

### 診断の修復（C）のタスクごとの値

| タスク | コード | 戦略 | chars | calls | rounds | est_tokens |
| --- | --- | --- | --- | --- | --- | --- |
| C-01 | E1012 | diag-v1 | 1824 | 6 | 2 | 456 |

### 静的な成果物の大きさ

| 成果物 | chars | est_tokens | 上限 |
| --- | --- | --- | --- |
| guide-l0 | 5429 | 1358 | 6000 |

### 外部エージェントの実測（下位の中央値）

| agent | variant | 型 | 件数 | 成功 | tokens_in | tokens_out | turns |
| --- | --- | --- | --- | --- | --- | --- | --- |
| claude-haiku | plain | callers | 3 | 2/3 | 12345 | 321 | 4 |

### 測らなかったもの

- 戦略 `diag-v2`: check --json has no related spans
- タスク E-03: need _tsuzuri/language-reference/values-and-functions/op-and-expressions.md "シフト量はビット幅でマスクされます": 0 matching lines
```

規則: 1 行目の `（未コミットの変更あり）` は `meta.json` の `dirty` が真のときだけ。`タスク N 件` は `loadTasks` の件数、`行 M` は `results.jsonl` の行数。「型ごとの値」の行は、型の記号順（A〜F）→ 戦略の並び順。`件数` はその型・戦略の行の数。`complete` は `真の数/件数`。「診断の修復」は C の行をタスク順 → 戦略順。`上限` は `limit` が `null` なら `-`。外部の表は `agent`・`variant`・型の記号順（`sort()`）で、`件数` は結果の行数、`成功` は `真の数/件数`。「測らなかったもの」は `skipped` と `skipped_tasks` を `戦略 \`{strategy}\`: {reason}`・`タスク {task}: {reason}` で並べる（戦略 → タスクの順）。

### E.2 外部の結果（`tz-ai-external/1`）のキーと検証メッセージ

| キー | 型 | 規則 |
| --- | --- | --- |
| `schema` | 文字列 | `"tz-ai-external/1"` |
| `task` | 文字列 | 読み込んだタスクの `id` |
| `agent` | 文字列 | `^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$` |
| `variant` | 文字列 | 条件の名前。`^[a-z0-9][a-z0-9._-]{0,31}$`（例: `plain`・`guide`・`kb`・`mcp`・`diag-v2`） |
| `tokens_in`・`tokens_out`・`turns` | 整数 | 0 以上の安全な整数 |
| `success` | 真偽 | |

知らないキーは無視する（足すだけの互換）。空行は読み飛ばす。不正は `report` が終了コード 1 で出す（`error: ` の後ろ）:

| 不正 | メッセージ（`{f}` は `external/` からの相対のファイル名、`{n}` は行番号） |
| --- | --- |
| JSON として読めない | `{f}:{n}: not valid JSON` |
| `schema` が違う | `{f}:{n}: schema must be "tz-ai-external/1"` |
| 必須のキーがない | `{f}:{n}: missing key '{key}'` |
| `task` が読み込んだタスクにない | `{f}:{n}: unknown task '{id}'` |
| `agent`・`variant` が形に合わない | `{f}:{n}: agent must match ^[A-Za-z0-9][A-Za-z0-9._-]{0,47}$`（`variant` は `^[a-z0-9][a-z0-9._-]{0,31}$`） |
| `tokens_in`・`tokens_out`・`turns` が 0 以上の整数でない | `{f}:{n}: tokens_in must be a non-negative integer`（キー名は該当のもの） |
| `success` が真偽でない | `{f}:{n}: success must be a boolean` |

### E.3 `verify` の検査

各タスクについて（失敗の見出しは `task {id}: …`）。**安定なタスクの失敗は失敗、非安定なタスクの失敗は drift**（`--strict` なら失敗）。

| # | 検査 | メッセージ |
| --- | --- | --- |
| 1 | タスクのファイルが本文「タスクの JSON」の形（キー・型・`id` の一意と記号） | 本文のメッセージ |
| 2 | `grep` のパターンが `corpus` で 1 行以上に一致する（A・D・E） | `task {id}: grep pattern "{p}" matches nothing` |
| 3 | すべての `needs`・`docs.needs` が一意に解決でき、`needs` のパスは `corpus`（C は `project`）の中、`docs.needs` のパスは `_tsuzuri/language-reference` の中にある | `need {path} "{match}": {n} matching lines`、`task {id}: need path {path} is outside the corpus` |
| 4 | `answer` の各文字列が、`needs` の本文（行を `"\n"` で連結）に含まれる | `task {id}: answer "{a}" is not in the needed lines` |
| 5 | B・F: 導出した答えが `expect` と一致する | `task {id}: derived {callers\|tests} differ from expect: {導出の JSON} vs {expect の JSON}` |
| 6 | タスクの `corpus` に現れる安定なコーパス（`shop`）で、`parseItems` のテスト項目の `id` の集合が、`tsuzuri test {dir} --list` の各行（先頭の索引と空白を除く）の集合と一致する | `corpus {dir}: test ids differ from tsuzuri test --list: {差}` |
| 7 | 同じコーパス（`shop`）が `tsuzuri check` を通る（stderr が空）。C の `project` は壊れているので対象外 | `corpus {dir}: check printed diagnostics` |
| 8 | （Phase 2）C: 壊れたソースの先頭の `error` の `code`・`span.line` が `expect` と一致。`fix` を当てた複製の `check` に `error` がない。`first_try` があれば、当てた複製の `check` に `error` がある。各 `find` が元のファイルにちょうど 1 回現れる | `task {id}: expected first diagnostic {code} at line {line}, got {code} at line {line}`、`task {id}: fix leaves errors`、`task {id}: first_try does not fail`、`task {id}: find "{先頭 40 字}" must occur exactly once` |

最後の行は `verified {n} tasks: {f} failures, {d} drift`（各件数は 10 進）。失敗が 1 件でもあれば終了コード 1。`--strict` なら drift も失敗に数える。

### E.4 CLI のメッセージ

| 入力 | メッセージ（`error: ` の後ろ。使い方の誤りは続けて `usage: node benchmarks/ai/run.mjs run\|verify\|report\|task ID [options]`） | 終了コード |
| --- | --- | --- |
| 引数なし・知らないコマンド | `unknown command '{x}'; use run, verify, report, or task` | 2 |
| 知らないオプション | `unknown option '{x}'` | 2 |
| `--types` に A〜F 以外 | `--types needs letters from A to F, comma separated` | 2 |
| 知らない戦略名 | `unknown strategy '{x}'; known: {並び順の全部をコンマ区切り}` | 2 |
| 明示した戦略が使えない | `strategy '{x}' is not available: {reason}` | 1 |
| `task` の ID がない | `unknown task '{x}'` | 2 |
| `report` の `results.jsonl` がない | `{path} not found; run 'run' first` | 1 |

## 付録 F: 後続 Phase の戦略

### F.1 `diag-v1`・`diag-v2`・`diag-fix`（Phase 2）

```js
// diag-v1。task.project を作業ディレクトリ <out>/work/<id>/ へ cpSync で複製してから始める（project はその複製、tree = new Tree(project)）
function diagV1(task, project, tree, compiler):
  agent = new Agent(tree, compiler);  first = agent.check(project)                                     // 1 往復
  d = firstError(first)          // stderr を行ごとに JSON.parse して、最初の severity == "error" の行。なければ例外
  path = d.path.replace(/^\.\//, "");  L = d.span.line;  [s, e] = itemRange(tree.lines(path), L)
  agent.read(path, min(L - 40, s), max(L + 40, e))                                                    // 1 往復。±40 行と、それを含む最上位の項目全体
  for (need of resolveNeeds(task.needs, tree)) if (!agent.covers(need)) agent.read(need.path, need.from - 40, need.to + 40)
  return tryAttempts(agent, task, project, tree)

function tryAttempts(agent, task, project, tree):
  attempts = task.first_try ? [task.first_try, task.fix] : [task.fix];  rounds = 0
  for (edits of attempts):
    trial = <out>/work/<id>-trial を project の複製から作り直す;  applyEdits(trial, edits)           // 元のソースに独立に当てる
    agent.edit();  out = agent.check(trial);  rounds++
    if (!hasError(out)) return { agent, rounds, complete: true }
  return { agent, rounds, complete: false }

itemRange(lines, L):    // KEYWORD は本文「導出規則」の正規表現
  s = L;  while (s > 1 && !KEYWORD.test(lines[s - 1])) s--          // L 以前で最も近い項目の先頭行
  if (!KEYWORD.test(lines[s - 1])) return [L, L]
  e = L;  while (e < lines.length && !KEYWORD.test(lines[e])) e++    // 次の項目の先頭の 1 行前
  while (e > s && lines[e - 1].trim() == "") e--                      // 末尾の空行を除く
  return [s, e]
hasError(text) = text の行のどれかが JSON として読め、severity == "error"
applyEdits(dir, edits):  各 {path, find, replace} について、ファイルに `find` がちょうど 1 回現れなければ例外 `edit {path}: "{find の先頭 40 字}" must occur exactly once`。`indexOf` と `slice` で置換する（`String.replace` の `$&` の解釈を避ける）
```

`calls = 1 + 1 + (読み足した必要の数) + 2 * rounds`。例（C-02。`rounds = 1`）: `check` 276 字 + 読み 962 字 + `edit` 3 字 + 直後の `check` 0 字 = 1,241 字・4 往復（付録 D の表）。

```js
// diag-v2（G22 の関連位置が入っているとき）: 読む範囲を、主スパンと related の周辺だけにする。DIAG_V2_WINDOW = 5
function diagV2(task, project, tree, compiler):
  agent = new Agent(tree, compiler);  first = agent.check(project)                                     // 1 往復
  return diagV2From(agent, first, task, project, tree)

function diagV2From(agent, first, task, project, tree):
  d = firstError(first)
  spans = [d, ...(d.related ?? [])].map(x => ({ path: x.path.replace(/^\.\//, ""), from: x.span.line, to: x.span.end_line }))
  for ([path, list] of spans を path で群にして path の sort() の順に回す):
    N = tree.lines(path).length
    for ([from, to] of mergeWindows(list.map(s => [max(1, s.from - 5), min(N, s.to + 5)]))) agent.read(path, from, to)
  for (need of resolveNeeds(task.needs, tree)) if (!agent.covers(need)) agent.read(need.path, need.from - 5, need.to + 5)
  return tryAttempts(agent, task, project, tree)                                                       // v1 と同じ

// diag-fix（G24 が入っているとき）: fix は複製の上で走らせ、元のソースは触らない
function diagFix(task, project, tree, compiler):
  agent = new Agent(tree, compiler);  first = agent.check(project)                                     // 1 往復
  result = agent.fix(<out>/work/<id>-fix に project を複製したもの)                                     // 1 往復（`tsuzuri fix .`。--json を付けない）
  if (result.status == 0 && result.stderr に `error[E` を含む行がない) return { agent, rounds: 0, complete: true }   // 2 往復で完了
  return diagV2From(agent, first, task, project, tree)       // 直らなかったら v2 の流れを続ける。2 往復の字数は数えたまま。元のソースから始めるので、やり直し（revert）は数えない（下限）
```

C の行の `code` は、どの戦略でも `firstError(first).code`。

### F.2 能力の検出

`probe(args)`: `tsuzuri {args}` を、`args` だけで（追加の引数なし）・stdin を閉じて走らせ、**終了コードが 0**、または**終了コードが 2 で stderr に `usage:` を含む**とき真。HEAD では `guide`・`fix`・`kb`・`mcp` は終了コード 1（`E2001`）、`kb callers` は終了コード 2 だが `usage:` を含まない（`E2000 pass one .tz…`）ので、すべて偽。

| 能力 | 検出 | 使えないときの `reason` |
| --- | --- | --- |
| `diag-v2` | `diag/c01` の複製の `check . --json` の先頭の行が `"related":` を含む | `check --json has no related spans` |
| `diag-fix` | `probe(["fix"])` | `tsuzuri fix is not available` |
| `kb`・`kb-oriented` | 型ごとに、使うコマンドの `probe(["kb", "show"])`（A）・`["kb","callers"]`（B）・`["kb","find"]`（D）・`["kb","tests-for"]`（F）。`kb-oriented` はさらに `["kb","brief"]` も真であること | 使えない型ごとに `skipped` へ `{"strategy":"kb","reason":"tsuzuri kb {sub} is not available ({型の名前})"}` を入れ、その型の行だけを飛ばす（ほかの型は動く）。**4 つの型がすべて使えない**ときだけ、戦略ごと使えない扱い（`--strategies kb` の明示はエラー。`reason` は 4 つの理由を `; ` でつないだもの）。`kb-oriented` も同じ規則 |
| MCP の `tools/list` | 実際に走らせて 10 秒以内に答えが返る | `tsuzuri mcp did not answer tools/list` |
| 静的な成果物 | 実際にコマンドを走らせて終了コード 0 | その成果物の行を出さない（`skipped` には入れない。表に出ないだけ） |

### F.3 `kb`・`kb-oriented`（Phase 3。A・B・D・F）

```js
// cwd = kbProject(task): corpus[0] が "std" なら "benchmarks/ai/corpus/shop"（std の ID は std の KB が引く）、そうでなければ corpus[0]
KB_COMMANDS = {                                     // 型 → tsuzuri への引数。id(x) は ID の解決
  definition:    (t) => ["kb", "show", id(t.kb.id)],
  callers:       (t) => ["kb", "callers", id(t.symbol)],
  "type-search": (t) => ["kb", "find", t.find],
  impact:        (t) => ["kb", "tests-for", id(t.change)],
}
KB_ORIENT = ["kb", "brief", "--project"]
function kb(task, oriented):
  agent = new Agent(tree, compiler)
  if (oriented) agent.command(KB_ORIENT, cwd)                                                     // 1 往復
  out = agent.command(KB_COMMANDS[task.type](task), cwd)                                          // 1 往復
  return { agent, complete: kbSucceeded(task, out) }
```

- `id(short)`: `task.corpus[0]` が `std` なら `short` のまま（std の ID は `Array.fold` の形）。そうでなければ `tsuzuri kb export --symbols`（G27。cwd = `kbProject`）の各行の `id` のうち `id === short` または `id.endsWith("::" + short)` のものがちょうど 1 つ（0 個・2 個以上は例外 `kb id '{short}': {n} matches`）。この呼び出しは**数えない**（ID の書式の差を吸収するだけで、エージェントの手間ではない）。
- 数えずに事前に `tsuzuri kb build`（cwd = `kbProject`。タスクごとに 1 回ではなく、`kbProject` ごとに 1 回）と、std のタスクがあれば `tsuzuri kb build --std` を 1 回走らせる。失敗は無視する（`complete` が偽になるだけ）。
- `kbSucceeded(task, out)`: A・D は `needles = task.kb?.contains ?? [signatureOf(task.answer[0])]` のすべてが `out` に含まれる（`signatureOf(a)` は `a` の最初の `" :: "` より後ろ。なければ `a` 全体）。B・F は `task.expect` のすべてが `out` に含まれる（ID に名前空間の接頭辞があっても部分文字列で一致する）。文字列の一致による近似であり、G28 の書式に合わず偽になるときは `kb.contains` を足して直す（`false` を `true` に書き換えることではなく、書式の差を表すこと）。
- 戦略の対象外の型（C・E）には行を出さない。

### F.4 静的な成果物（Phase 3。`static.jsonl`。各行は `tz-ai-static/1`）

`STATIC_ARTIFACTS`（この順）。「コマンド」は `tsuzuri` への引数で、`chars` は stdout のコードポイント数。「ファイル」は `tsuzuri new <out>/work/demo` が書いたファイルの字数（`new` が `AGENTS.md`・`llms.txt` を書かなければ行を出さない）。KB の 3 つは `cwd = benchmarks/ai/corpus/shop`。

| `artifact` | 取り方 | `limit`（出典） |
| --- | --- | --- |
| `guide-l0` | コマンド `guide` | 6000（D-56） |
| `guide-l1` | コマンド `guide --level 1` | 20000（D-56） |
| `guide-std-array` | コマンド `guide std Array` | 6000（D-56） |
| `guide-std-all` | コマンド `guide --topic std-all` | 45000（D-56） |
| `agents-md` | ファイル `AGENTS.md` | 3500（D-56） |
| `llms-txt` | ファイル `llms.txt` | 1500（D-56） |
| `kb-brief-l0` | コマンド `kb brief --project` | 2400（D-52） |
| `kb-brief-l1` | コマンド `kb brief --module Orders` | 1600（D-52） |
| `kb-brief-l2` | コマンド `kb brief {id("Orders.order_total")}` | 700（D-52） |
| `mcp-tools-list` | F.5 | `null` |

### F.5 MCP の `tools/list`（Phase 3。G29 が入っているとき）

G29 のツールの `text` は対応する CLI の text と同じ（G29「ツール」の表）なので、タスクごとの戦略は作らず、**セッションごとに 1 回エージェントの文脈へ入る固定費**だけを測る。

```js
async function mcpToolsList(compiler, cwd):          // 改行区切りの JSON-RPC 2.0（G29。protocolVersion は 2025-06-18）
  child = spawn(compiler, ["mcp"], { cwd, stdio: ["pipe", "pipe", "ignore"] })     // cwd = shop
  send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "tz-ai-cost", version: "1" } } })
  await response with id 1 (10 秒);  send({ jsonrpc: "2.0", method: "notifications/initialized" })
  send({ jsonrpc: "2.0", id: 2, method: "tools/list" });  response = await response with id 2 (10 秒)
  child.stdin.end();  child を終わらせる（終わらなければ kill）
  text = JSON.stringify(response.result.tools)       // 再直列化して、サーバーの空白に依らない
  return { chars: countText(text).chars, est_tokens, tools: response.result.tools.length }
```
応答が 10 秒以内に来ない・JSON でない・`result.tools` がない場合は、行を出さず `skipped` に `{"strategy":"mcp-tools-list","reason":"tsuzuri mcp did not answer tools/list"}` を入れる。

## 付録 G: テスト計画の表

`tests/ai_cost.mjs`（本文「テスト計画」）の全テスト。番号は本文の「実装手順」が引く。期待値の独立な参照: 数式は手計算の表、golden は付録 D のシェルの計算、導出は `tsuzuri test --list` とシェルの `grep`。一時ファイルは `target/ai-cost-test-{pid}/` の下に作り、`finally` で消す。

| # | 関数（`tests/ai_cost.mjs::`） | 入力 → 期待 |
| --- | --- | --- |
| 1 | `estimate_tokens_matches_the_formula` | `(ascii, other)`: (0,0)→0、(1,0)→1、(4,0)→1、(5,0)→2、(0,1)→2、(1,1)→2、(0,10)→13、(3999,0)→1000、(4001,0)→1001。`a`・`n` を 0〜40 で全部回し `Math.ceil((10 * a + 52 * n) / 40)` と一致 |
| 2 | `chars_count_code_points_not_utf16_units` | `countText("a😀あ")` が `{chars: 3, ascii: 1, other: 2}`、`estimateTokens` が 3。`""` は全部 0 |
| 3 | `lower_median_picks_the_lower_middle` | `[5,1,3]`→3、`[4,1,3,2]`→2、`[7]`→7、`[]` は例外 |
| 4 | `files_are_sorted_and_filtered` | 一時のツリー（`b.tz`・`a.md`・`.hidden.tz`・`target/x.tz`・`node_modules/y.tz`・`c.txt`・`sub/d.tc`）の `files(".")` が `["a.md","b.tz","sub/d.tc"]`。ファイルの `root` は 1 要素。存在しない `root` は `corpus root 'nope' does not exist` |
| 5 | `lines_drop_one_trailing_empty_element` | `"a\nb\n"`→2 行、`"a\nb"`→2 行、`"a\n\n"`→`["a",""]`、`""`→0 行 |
| 6 | `grep_formats_orders_and_caps` | shop の `percent_of` が 4 行・478 字（`benchmarks/ai/corpus/shop/Money.tz:2:def percent_of :: …`、4・6・12 行目。付録 D）。150 行が一致する一時ファイルで出力が 100 行、`seen` も 100 行だけ。一致なしは `""` で `calls` が 1 |
| 7 | `read_clamps_and_marks_lines` | `read(p, -5, 3)` が 1〜3 行目、`to` が行数を超えると行数に丸める、`chunk(p, 2)` が 501〜1000 行目。出した行が `seen` に入る |
| 8 | `needs_must_match_exactly_one_line` | 2 行に一致 → `need … 2 matching lines`、0 行 → `0 matching lines`、`before` が先頭を越えれば 1 に丸める |
| 9 | `task_files_are_valid` | `loadTasks` が 30 件（Phase 2 後は 37）。各型 5 件以上、`id` が昇順で一意、記号と `type` が対応 |
| 10 | `unknown_and_missing_task_keys_are_rejected` | 未知のキー → `task A-01: unknown key 'foo'`、`grep` なし → `task A-01: missing key 'grep'` |
| 11 | `items_follow_the_keyword_rule` | 一時のコーパス: `private def rec f ::`・`fn rec f x =`・`and g y =`・`test "a // b" =`・コメント・インデントした `def`・`record R` を `parseItems` に通して、項目の `kind`・`id`・`from`・`to` が手で書いた表と一致（`//` は文字列の外だけ取り除く） |
| 12 | `callers_and_impact_match_expect_on_shop` | B-05・B-06・F-01〜F-06 の導出が `expect` と一致。F-02 の `grep` が `percent_of,with_discount,with_tax,total_with_discount,gross,discounted,price_of,main,line_total,order_total,summary` |
| 13 | `compiler_test_list_matches_parsed_tests` | `tsuzuri test shop --list` の各行から先頭の索引を除いた集合が、`parseItems` のテスト項目の `id` の集合（16 件）と一致 |
| 14 | `shop_goldens_for_the_grep_baselines` | 付録 D の 2 つの表のとおり（A-05・B-05・B-06・F-01〜F-06 × `baseline-grep`・`-chunk`・`-window` の `chars`・`calls`・`est_tokens`。`-chunk` と `-window` は同じ）。`complete` が真 |
| 15 | `chunks_are_read_until_the_needs_are_covered` | 1,200 行の一時ファイルで、必要が 100 行目なら `chunk` 1 回、1,100 行目なら 3 回（1〜500・501〜1000・1001〜1200） |
| 16 | `windows_merge_adjacent_ranges` | `mergeWindows([[1,10],[11,20],[30,40],[35,50]])` が `[[1,20],[30,50]]`、`[[5,5]]` はそのまま |
| 17 | `docs_strategy_reads_the_index_first` | A-02 の `baseline-docs` で、最初の結果の字数が `index.md` の字数（`wc -m` で 5914。文書が変わればその値）、`calls` が 2 |
| 18 | `run_is_deterministic` | `run` を 2 つの `--out` に走らせ、`results.jsonl` が 1 byte も違わない。行数が `meta.json` の `rows` と一致 |
| 19 | `result_rows_have_the_documented_keys` | 先頭の行のキーが `schema,task,type,strategy,chars,calls,est_tokens,complete`、値の型が整数・真偽 |
| 20 | `cli_usage_errors_exit_two` | 付録 E.4 の入力が終了コード 2（明示した使えない戦略は 1）で、メッセージが一致 |
| 21 | `task_command_hides_the_answer` | `task A-01` の JSON のキーが `id,type,prompt,corpus`。`task Z-99` は終了コード 2 |
| 22 | `verify_passes_on_the_repository_tasks` | 終了コード 0、最後の行が `verified 30 tasks: 0 failures, {d} drift`（Phase 2 後は 37）。安定なタスクで失敗がない |
| 23 | `verify_fails_on_a_broken_stable_task` | B-05 の `expect` を 1 つ消した一時のタスクのディレクトリ（`--tasks`）で、終了コード 1、`task B-05: derived callers differ from expect` |
| 24 | `report_tables_are_exact` | 手で書いた 4 行の `rows` と `meta` から、付録 E.1 の書式の文字列と完全に一致（`complete` の `k/n`・下位の中央値を含む） |
| 25 | `report_replaces_only_the_marked_region` | マーカーの外が 1 byte も変わらず、2 回目が冪等。マーカーなし・2 組・逆順は例外 |
| 26 | `report_publish_updates_the_document` | `docs/ai-cost.md` の複製に対して `report --publish --doc 複製` を走らせ、マーカーの間が `report` の標準出力と一致 |
| 27 | `diag_corpus_reports_the_expected_first_diagnostic`（Phase 2） | C の 7 つで、先頭の `error` の `code`・`span.line` が `expect`（付録 D） |
| 28 | `diag_patches_behave` | `fix` の複製の `check` に `error` がない、`first_try` の複製には `error` がある、各 `find` が 1 回だけ現れる |
| 29 | `diag_v1_counts_calls_and_rounds` | `calls`/`rounds` が C-01 6/2、C-02 4/1、C-03 6/2、C-04 6/2、C-05 6/2、C-06 4/1、C-07 4/1。各行で `calls = 2 + 読み足した数 + 2 * rounds`。`chars` はコンパイラの文言に依るので固定しない（付録 D は参考） |
| 30 | `diag_rows_have_rounds_and_code` | C の行のキーが `…,est_tokens,rounds,code,complete`、`complete` が真 |
| 31 | `diag_v2_is_skipped_or_reads_related` | `check --json` に `related` がなければ `skip`。あれば `diag-v2` の C-01 の行が `complete` で、最初の読みが `diag-v1`（ファイル全体）より短い |
| 32 | `diag_fix_is_skipped_or_finishes_in_two_calls` | `tsuzuri fix` がなければ `skip`。あれば `fix` で直るタスクが `calls` 2・`rounds` 0 |
| 33 | `unavailable_strategy_is_an_error` | `diag-fix`・`diag-v2`・`kb` のうち使えないものを `--strategies` に明示すると終了コード 1、`strategy '{x}' is not available: {reason}`（すべて使えるなら `skip`） |
| 34 | `external_frames_are_validated`（Phase 3） | 正しい 1 行が通り、付録 E.2 の 7 つの不正がそれぞれのメッセージで終了コード 1 |
| 35 | `external_table_uses_lower_medians` | 同じ `agent`・`variant`・型の 4 行から、`件数` 4、`成功` `3/4`、下位の中央値の `tokens_in`・`tokens_out`・`turns` |
| 36 | `static_artifact_limits` | 測れた静的な成果物（`guide`・`kb brief`・`new` が書くファイル）の `chars` が、すべて `limit` 以下。測れないものは `skip` に理由を出す。0 件でも通る（HEAD） |
| 37 | `static_rows_are_deterministic` | 2 回測って同じ |
| 38 | `kb_strategies_are_skipped_or_use_one_call` | `kb` がなければ `skip`。あれば B-05 の `kb` が `calls` 1、`kb-oriented` が 2 |
| 39 | `kb_success_uses_substring_checks` | `kbSucceeded` を文字列だけで: A は `answer[0]` の `" :: "` の後ろ、B・F は `expect` の全部、`kb.contains` があればそれ |
| 40 | `mcp_tools_list_row_or_skip` | `tsuzuri mcp` がなければ `skip`。あれば `mcp-tools-list` の `chars` が 0 より大きい |

## 付録 H: `docs/ai-cost.md` の骨格

手順 9 で新規に書く文書の最初の形（日本語。表の数値は `report --publish` が入れる）。見出しとマーカー（2 行。間は空）はこのとおりにする。本文は事実に合わせて直してよいが、**「限界」の 6 項目は消さない**。

````markdown
# AI 作業コスト（入力量と往復数）

Tsuzuri の AI 向けの機能（ガイド・診断 v2・`fix`・KB・MCP）が、AI エージェントの作業を軽くするかを、LLM を呼ばずに**決定的に**数えて確かめるためのページ。実行速度の記録は [benchmarks.md](benchmarks.md)。ここで扱うのは、エージェントが読む文字数と、ツールを呼ぶ往復の数である。定義と設計は [G31](../_features/G31-ai-cost-metrics.md)。

## 測る量

| 名前 | 意味 |
| --- | --- |
| `chars` | 模擬エージェントが受け取ったツールの結果の文字数（Unicode のコードポイント）の合計 |
| `calls` | ツールを呼んだ回数 |
| `est_tokens` | 推定トークン `ceil(ASCII の字数 / 4 + 非 ASCII の字数 × 1.3)`。実トークナイザではない（日本語は ASCII の 5.2 倍に数える） |
| `rounds` | 診断を直す試行（`edit` と `check`）の回数 |
| `complete` | 戦略が答えに必要な行を得たか |

数えないもの: タスクの問い、エージェントの出力・思考、ツールの引数。

## タスクと戦略

タスクは 6 型 37 件（定義・呼び出し元・診断・型で探す・仕様・影響範囲）で、`node benchmarks/ai/run.mjs task ID` が問いを出す。模擬エージェントの戦略は次のとおり。

| 戦略 | 模すもの |
| --- | --- |
| `baseline-grep` | `grep` → ヒットしたファイルを全文読む（素朴な上限） |
| `baseline-grep-chunk` | `grep` → 必要な行が見えるまで 500 行ずつ読む |
| `baseline-grep-window` | `grep` → 必要な行の ±40 行だけ読む（最も厳しい下限） |
| `baseline-docs` | 言語リファレンスの目次 → ページを 500 行ずつ読む |
| `diag-v1`・`diag-v2`・`diag-fix` | `check` → 周辺を読む → 直す。v2 は関連位置、fix は `tsuzuri fix` |
| `kb`・`kb-oriented` | `tsuzuri kb` の 1 コマンド（先に `kb brief`） |

**効果の主張は、最も厳しい下限 `baseline-grep-window` との比で書く。**

## 結果

<!-- generated:ai-cost -->
<!-- /generated:ai-cost -->

## 再現手順

```sh
node benchmarks/ai/run.mjs run
node benchmarks/ai/run.mjs report --publish
node tests/ai_cost.mjs target/release/tsuzuri
```

外部のエージェントの実測は、`node benchmarks/ai/run.mjs task ID` が出すタスクを渡し、結果を `target/ai-cost/external/*.jsonl`（`tz-ai-external/1`）に置くと、「外部エージェントの実測」の表に入る。

## 限界

- 模擬エージェントは「経路を知っている」下限と素朴な上限の範囲であり、実エージェントの再現ではない。
- 呼び出し元・影響範囲の導出は近似（桁 0 のキーワード行を項目とする）。
- 診断のプログラムは 31〜49 行で、±40 行の窓がファイル全体になる（`diag-v1` の読みが小さく出る）。
- `std`・文書のタスクの値は、それらの編集で動く。
- KB 方式の `complete` は文字列の一致による近似。
- LLM の回答の質・応答時間・料金は測らない。
````
