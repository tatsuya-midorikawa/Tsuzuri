# G35 付録 A・B・C

[G35 本文](G35-generated-docs-single-source.md)の付録。A は実測（コマンドと出力）、B は std の初期の `@complexity`・`@see`、C は `_specs/` の項目ごとの決定表。

## A. 実測

HEAD `9dd0fbbc`（`a2313d19` の親）、`target/release/tsuzuri`（0.1.0）、macOS arm64、Node 24。測定のスクリプトは `target/` の下の作業用ディレクトリに置いた（リポジトリには入れず、チケットの完成後に消した）。コードは [G35-appendix-code.md](G35-appendix-code.md) の D に再掲した。

### A.1 `tsuzuri doc std` のページの形

````text
$ target/release/tsuzuri doc std -o <out>
$ head -n 21 <out>/Map.md
# Map

Namespace: `std`

## `Map`

```tsuzuri
record Map<'key, 'value> {
  entries: Vec<Entry<'key, 'value>>
}
```

## `empty`

```tsuzuri
def empty :: Map<'key, 'value>
```

## `singleton`
$ sed -n 53,60p <out>/Array.md          # 説明のある関数
## `copy`

```tsuzuri
def copy :: Copy<'a> => ref ['a] -> ['a]
```

Returns a new array with a copy of every element, the explicit form of an implicit array copy.
````

節は `## \`name\``、直後に署名の ```` ```tsuzuri ```` ブロック、その後ろが説明の本文（`///` の行が `\n` でつながったもの）。`record`・`union`・`type` の節は署名が `def` で始まらない。

### A.2 数え方と数

手順: `<out>/<Module>.md` を `\n## \`` で分け、最初の ```` ```tsuzuri ```` ブロックが `def`・`export def`・`const def` で始まる節を関数と数える。ブロックの後ろが空でなければ「説明あり」。

| モジュール | 公開 def | 説明あり | 説明のある関数 |
| --- | ---: | ---: | --- |
| Array | 39 | 6 | `copy`・`sum_pairwise`・`sum_kahan`・`dot`・`dot_fma`・`sort_in_place` |
| Vec | 1 | 0 | `iter` だけ（残りは組み込み） |
| Map | 14 | 0 | |
| Set | 13 | 0 | |
| HashMap | 24 | 6 | `sip13`・`with_seed`・`with_capacity_and_seed`・`try_randomized`・`randomized`・`longest_probe` |
| HashSet | 17 | 3 | `with_seed`・`try_randomized`・`longest_probe` |
| List | 5 | 1 | `copy` |
| String | 21 | 0 | |
| **計** | **134** | **16** | |

std 全体は 47 モジュール・559 個の公開 `def`（`def` を持つのは 42 モジュール）。組み込み操作は `Builtin::ALL`（`src/check.rs`）にあり、`Vec.push`・`Array.set`・`List.cons`・`String.from_utf8` は `std/*.tz` に `def` を持たない。

### A.3 HTML → JSON の往復検査（試作の結果）

`D.4` の解析で `docs/benchmarks.md` の 2 ブロックを JSON にし、旧 `renderTable`（HEAD の `benchmarks/report.mjs`）と旧の段落の式で**ブロック全体**を作り直して、元の文字列と比べた。

```text
summary block round trip: true
metrics block round trip: true
summary.json 6746 metrics.json 70695          # byte。JSON.stringify(value, null, 2) + "\n"
rows 38 runs 2 tables 36 body rows 314 body cells 2109
```

### A.4 新しい Markdown の大きさ

```text
summary block 5794 bytes (HTML 100585)         metrics block 26457 bytes (HTML 394852)
docs/benchmarks.md  952255 → 489211 bytes (-48.6%)    ブロックの外 456834 bytes は 1 byte も変わらない
headings identical: 94                          # check-docs.mjs の headings と同じ規則
```

### A.5 ドリフトの測定

```text
$ target/release/tsuzuri --help | wc -lc          221 行・15,688 byte。終了コード 0
$ target/release/tsuzuri --version                tsuzuri 0.1.0
cli:      flags 38, commands 16, variables 10
          README   lacks flags: --debug-output --sysroot --version --warn ; variables: TSUZURI_DSYMUTIL TSUZURI_LLVM_LINK ; command: toolchain
          option   lacks flags: --git --rev
          usage    lacks flags: --allocator --debug-info --debug-output --freestanding --output --sysroot --trap-mode --wasm-max-memory --wasm-stack-size ; variables: TSUZURI_LINKER TSUZURI_RANGE_FLAGS
codes:    emitted 47 (constructor calls 46 + the JSON literal of W2001) | compiler/diagnostics.md 47 | docs/language.md 46 (lacks W2001)
version:  22 mentions of "Tsuzuri X.Y.Z" in README, docs/ (not benchmarks.md) and _tsuzuri/, all 0.1.0
std:      559 public defs, 559 mentioned in their module's page (Async is checked against async-tasks-and-lazy/async.md)
```

`--sysroot` は `--help` の環境変数の文に出る C コンパイラの旗で、Tsuzuri の旗ではない（免除）。`E2026` は edition の定数（`Edition::E2026`）で診断コードではない。

### A.6 リンクの測定

```text
{"files":256,"links":2178,"withFragment":495,"anyInto":70,"intoBenchmarks":46,"broken":11}
unique anchors into benchmarks.md: 17     files linking into benchmarks.md with fragment: 17
```

壊れた 11 件（本チケットの対象外）: `_features/GUIDE.md`（`README.md#追加チケット2026-10-18`）、`_features/_completed/C09-hash-map.md`（`hash-map.md`）、`_features/_completed/D09-regex-unicode.md`（`../api/Regex.md`・`../regex.md` ×2・`../api/Unicode.md`）、`_perfs/_completed/PM06-allocation-elimination.md`（`#再帰とスタック`）、`docs/language.md`（`#利用者定義の解放` ×3、`...` 1）。`docs/benchmarks.md` 向けは 0 件。

## B. std の初期の `@complexity` と `@see`

**使い方**: 手順 2.4〜2.7 で、各 `def` の直前の `///` に次の値を「仕様 3.4」の順序で足す。値は std の実装を読んで決めた（コールバック・`Eq.eq`・`Ord.lt`・`Hash.hash` は `O(1)`。`n`・`m`・`k` の意味は「仕様 3.3」）。既存の手書きの表（53 行）と比べて一致しないのは `Array.sort_in_place`（旧 `O(n log n)`）だけ。`@see` の行は `Array` 8、`HashMap` 8、`HashSet` 4 の計 20 行（17 関数）。`@since` は足さない。

**説明に足す文**（英語。説明がある関数はその最後の文の後ろ、無ければ最初の文として。G34 が同じ関数に説明を足すときは、この文を 2 文目にする）:

- `Array.sort_in_place`: `Comparisons take O(n log n) and element moves O(n log^2 n).`
- `Map.insert`: `Replacing the value of an existing key takes O(log n); a new key shifts the entries after it.`
- `HashMap.longest_probe`: `Scans the whole table, whose size follows the largest number of entries the map has held.`
- `HashSet.longest_probe`: `Scans the whole table of the underlying map, whose size follows the largest number of entries it has held.`
- `List.iter`: `Collects a reference to every element first; each step of the returned `Seq` then takes O(1).`

**書き方の例**（説明のある関数。`Array.copy`）:

```tsuzuri
/// Returns a new array with a copy of every element, the explicit form of an implicit array copy.
///
/// @complexity O(n)
def copy :: Copy<'a> => ref ['a] -> ['a]
```

（説明のない関数。`Array.sum`。`@see` は 1 行に 1 つ）:

```tsuzuri
/// @complexity O(n)
/// @see Array.sum_pairwise
/// @see Array.sum_kahan
def sum :: Numeric<'a> => ref ['a] -> 'a
```

### B.1 Array（39 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `length` | `O(1)` | - |
| `is_empty` | `O(1)` | - |
| `iter` | `O(1)` | - |
| `get` | `O(1)` | - |
| `at` | `O(1)` | - |
| `init` | `O(k)` | - |
| `sub` | `O(k)` | - |
| `reverse` | `O(n)` | - |
| `copy` | `O(n)` | - |
| `append` | `O(n + m)` | - |
| `zip` | `O(n)` | - |
| `map` | `O(n)` | - |
| `mapi` | `O(n)` | - |
| `map_ref` | `O(n)` | - |
| `mapi_ref` | `O(n)` | - |
| `fold` | `O(n)` | - |
| `fold_ref` | `O(n)` | - |
| `fold_back` | `O(n)` | - |
| `fold_back_ref` | `O(n)` | - |
| `reduce` | `O(n)` | - |
| `sum` | `O(n)` | `Array.sum_pairwise`, `Array.sum_kahan` |
| `sum_pairwise` | `O(n)` | `Array.sum`, `Array.sum_kahan` |
| `sum_kahan` | `O(n)` | `Array.sum`, `Array.sum_pairwise` |
| `dot` | `O(n)` | `Array.dot_fma` |
| `dot_fma` | `O(n)` | `Array.dot` |
| `product` | `O(n)` | - |
| `min` | `O(n)` | - |
| `max` | `O(n)` | - |
| `any` | `O(n)` | - |
| `all` | `O(n)` | - |
| `count` | `O(n)` | - |
| `find` | `O(n)` | - |
| `index_of` | `O(n)` | - |
| `contains` | `O(n)` | - |
| `equal` | `O(n)` | - |
| `binary_search` | `O(log n)` | - |
| `sort` | `O(n log n)` | - |
| `sort_in_place` | `O(n log^2 n)` | - |
| `filter` | `O(n)` | - |

### B.2 Vec（1 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `iter` | `O(1)` | - |

### B.3 Map（14 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `empty` | `O(1)` | - |
| `singleton` | `O(1)` | - |
| `length` | `O(1)` | - |
| `is_empty` | `O(1)` | - |
| `contains_key` | `O(log n)` | - |
| `get` | `O(log n)` | - |
| `at` | `O(log n)` | - |
| `insert` | `O(n)` | - |
| `remove` | `O(n)` | - |
| `to_array` | `O(n)` | - |
| `keys` | `O(n)` | - |
| `values` | `O(n)` | - |
| `fold` | `O(n)` | - |
| `iter` | `O(1)` | - |

### B.4 Set（13 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `empty` | `O(1)` | - |
| `singleton` | `O(1)` | - |
| `length` | `O(1)` | - |
| `is_empty` | `O(1)` | - |
| `contains` | `O(log n)` | - |
| `insert` | `O(n)` | - |
| `remove` | `O(n)` | - |
| `to_array` | `O(n)` | - |
| `fold` | `O(n)` | - |
| `union` | `O(n + m)` | - |
| `intersect` | `O(n + m)` | - |
| `difference` | `O(n + m)` | - |
| `iter` | `O(1)` | - |

### B.5 HashMap（24 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `empty` | `O(1)` | - |
| `length` | `O(1)` | - |
| `is_empty` | `O(1)` | - |
| `sip13` | `O(1)` | - |
| `with_capacity` | `O(k)` | - |
| `with_seed` | `O(1)` | - |
| `with_capacity_and_seed` | `O(k)` | - |
| `try_randomized` | `O(1)` | - |
| `randomized` | `O(1)` | - |
| `contains_key_ref` | `expected O(1), worst O(n)` | `HashMap.contains_key` |
| `contains_key` | `expected O(1), worst O(n)` | `HashMap.contains_key_ref` |
| `get_ref` | `expected O(1), worst O(n)` | `HashMap.get` |
| `get` | `expected O(1), worst O(n)` | `HashMap.get_ref` |
| `at_ref` | `expected O(1), worst O(n)` | `HashMap.at` |
| `at` | `expected O(1), worst O(n)` | `HashMap.at_ref` |
| `insert` | `amortized O(1), worst O(n)` | - |
| `remove_ref` | `expected O(1), worst O(n)` | `HashMap.remove` |
| `remove` | `expected O(1), worst O(n)` | `HashMap.remove_ref` |
| `longest_probe` | `O(n)` | - |
| `to_array` | `O(n)` | - |
| `keys` | `O(n)` | - |
| `values` | `O(n)` | - |
| `fold` | `O(n)` | - |
| `iter` | `O(1)` | - |

### B.6 HashSet（17 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `empty` | `O(1)` | - |
| `with_capacity` | `O(k)` | - |
| `length` | `O(1)` | - |
| `is_empty` | `O(1)` | - |
| `with_seed` | `O(1)` | - |
| `with_capacity_and_seed` | `O(k)` | - |
| `try_randomized` | `O(1)` | - |
| `randomized` | `O(1)` | - |
| `insert` | `amortized O(1), worst O(n)` | - |
| `remove_ref` | `expected O(1), worst O(n)` | `HashSet.remove` |
| `remove` | `expected O(1), worst O(n)` | `HashSet.remove_ref` |
| `contains_ref` | `expected O(1), worst O(n)` | `HashSet.contains` |
| `contains` | `expected O(1), worst O(n)` | `HashSet.contains_ref` |
| `longest_probe` | `O(n)` | - |
| `to_array` | `O(n)` | - |
| `fold` | `O(n)` | - |
| `iter` | `O(1)` | - |

### B.7 List（5 関数）

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `length` | `O(1)` | - |
| `is_empty` | `O(1)` | - |
| `copy` | `O(n)` | - |
| `fold` | `O(n)` | - |
| `iter` | `O(n)` | - |

### B.8 String（21 関数）

`n` は `text`（主な文字列）の UTF-16 コード単位数、`m` は `needle`・`separator`・`prefix`・`suffix`、`k` は結果のコード単位数。`join`・`concat` の `n` は部品の数。

| 関数 | `@complexity` | `@see` |
| --- | --- | --- |
| `length` | `O(1)` | - |
| `find` | `O(n * m)` | - |
| `rfind` | `O(n * m)` | - |
| `contains` | `O(n * m)` | - |
| `starts_with` | `O(m)` | - |
| `ends_with` | `O(m)` | - |
| `slice` | `O(k)` | - |
| `sub` | `O(k)` | - |
| `decode_at` | `O(1)` | - |
| `char_count` | `O(1)` | - |
| `chars` | `O(n)` | - |
| `join` | `O(n + k)` | - |
| `concat` | `O(n + k)` | - |
| `split` | `O(n * m)` | - |
| `trim_start` | `O(n)` | - |
| `trim_end` | `O(n)` | - |
| `trim` | `O(n)` | - |
| `to_ascii_lower` | `O(n)` | - |
| `to_ascii_upper` | `O(n)` | - |
| `repeat` | `O(k)` | - |
| `replace` | `O(n * m + k)` | - |

値の分布（134）: `O(1)` 36、`O(n)` 55、`O(k)` 9、`O(log n)` 5、`O(n + m)` 4、`O(n * m)` 4、`O(m)` 2、`O(n + k)` 2、`O(n log n)` 1、`O(n log^2 n)` 1、`O(n * m + k)` 1、`expected O(1), worst O(n)` 12、`amortized O(1), worst O(n)` 2。

## C. `_specs/` の項目ごとの決定表

方法: 各ファイルのコードブロックを `target/release/tsuzuri check` と `run`（`run=` の出力のあるもの）にかけ、文は現行のページと突き合わせた。構文の雛形（`...` や `[when constant-list]` のような擬似構文）はプログラムではないので `check` の対象外。**決定はすべて「破棄（`_specs/_archive/` へ移すだけ）」**。取り込む項目は 0。「現行が異なる」の 6 項目は、現行のページが正で、その差を `_specs/README.md` に書かない（歴史的資料として残す）。

| ID | `_specs` の項目 | 現行のページ（`_tsuzuri/language-reference/` から） | 照合 |
| --- | --- | --- | --- |
| E1 | `try…with(…finally)` は例外を扱う式 | `exception-handling/try-with-finally.md` | 同じ形である |
| E2 | 関数の返り値の型が自動で `Result<'T,'TErr>` になる | 同 §注意点 | **現行が異なる**: `try` の型が `Result` で、関数の戻り値型は自動では変わらない。`def f :: i32 -> i32 -> i32 = … try …` は `E1003: expected i32, found Result<i32, an undetermined type>` |
| E3 | `'TErr` は `Err` を実装する | 同（`@'E : Err` は省略できる） | 同じ形である |
| E4 | `is` で特定の例外をキャッチする | 同 | 同じ形である（例を実行して確認） |
| E5 | `finally` 句 | 同 | 同じ形である |
| E6 | 構文の雛形（`try … with … finally`、`...`） | 同 §構文 | 雛形。`check` の対象外 |
| E7 | 例 1・2（`safe_add 2147483647 1` が `Overflow occurred: Arithmetic operation resulted in an overflow.`） | 同 | 同じ形である（`check` を通り、出力が一致） |
| L1 | 整数の接尾辞 `y uy s us u l ul L UL I` | `literals-and-strings/literals.md` | 同じ形である（現行は `86i8`・`86sbyte`・`86byte` を足した上位集合） |
| L2 | 浮動小数点 `hf f F` | 同 | 同じ形である |
| L3 | 10 進 `hm m M` | 同 | 同じ形である |
| L4 | `char`・`utf8char`・`string`・`utf8string` のリテラル | 同 | **現行が異なる**: `_specs` は `utf8string` を「UTF-16 string」と書くが、UTF-8 |
| L5 | `byte[]`・`utf8char[]`、`"test"B`・`u8"😊test"B` | 同 | **現行が異なる**: 型の表記は `[byte]`・`[utf8char]`（`byte[]` は `E0002`） |
| L6 | `bool`・`unit` | 同 | 同じ形である |
| L7 | `@literal` | `values-and-functions/attributes.md`・`literals.md` | 同じ形である（例は `W1001` の警告だけで `check` を通る） |
| N1 | `namespace [parent::]identifier` | `organizing-tsuzuri/namespaces.md` | 同じ形である |
| N2 | `Tsuzuri.toml` に既定 `namespace`、無ければ「プロジェクト名（ルートフォルダー名）」 | 同（既定値の節） | **現行が異なる**: 既定はパッケージ名の kebab-case を PascalCase にしたもの（`geometry-tools` → `GeometryTools`）。マニフェストが無ければ入力ディレクトリ名から同じ規則 |
| N3 | `tsuzuri new` と VS Code の拡張は `namespace` を必ず書く | `organizing-tsuzuri/packages.md`（`new`・`--namespace`） | 同じ形である |
| N4 | `namespace` はファイルの最初の宣言。所属はモジュール単位 | `namespaces.md` | 同じ形である |
| N5 | `using` は `namespace` の後 | `organizing-tsuzuri/using-declarations.md` | 同じ形である（複数ファイルの例を実行して確認） |
| N6 | `using` で `namespace` の修飾を省略 | 同 | 同じ形である |
| N7 | 1 ファイル 1 モジュール。`module` 宣言・入れ子・分割はできない | `organizing-tsuzuri/modules.md` | 同じ形である（`module Foo` は `E0002`） |
| N8 | モジュール名と同名の `record`・`union` の完全名は `namespace + 型名` | 同 §モジュールと同名の型 | 同じ形である（複数ファイルの例を実行して確認） |
| N9 | 同じ `namespace` なら修飾を省略でき、モジュール名は大文字始まり | `namespaces.md`・`modules.md`（`E1011`） | 同じ形である |
| O1 | 単項 `+`・`-` | `values-and-functions/op-and-expressions.md` | 同じ形である |
| O2 | 二項算術 `+ - * / % **`（`5 / 3` が 1、`5 % 3` が 2、`2**5` が 32） | 同 | 同じ形である（値を実行して確認） |
| O3 | `@checked` と `OverflowException` | 同・`attributes.md`・`try-with-finally.md` | 同じ形である |
| O4 | `@checked` の対象は `+ - * **` | `op-and-expressions.md`・`try-with-finally.md` §@checked が検査する演算 | 同じ形である |
| O5 | 比較 `== > < >= <= !=` | `op-and-expressions.md` | 同じ形である |
| O6 | bool の `not`・`\|\|`・`&&`（短絡） | 同 | 同じ形である（`_specs` の表は `\|\|` を escape せず壊れている） |
| O7 | ビット `&&& \|\|\| ^^^ ~~~ <<< >>>` | 同 | 同じ形である（値を実行して確認） |
| O8 | シフト量の型は `i32` | 同・`built-in-types-and-modules/int.md` | **現行が異なる**: 量は左辺と同じ整数型（`let x: i32 = 1`・`let n: i64 = 4` のとき `x <<< n` は `E1003: expected i32, found i64`）で、量は幅でマスクされる |
| O9 | `>>>` は符号なしで論理、符号付きで算術 | `op-and-expressions.md` | 同じ形である |
| F1 | `def name :: types constraint = lambda` | `values-and-functions/functions.md` | 雛形。具体例は同じ形である |
| F2 | ラムダ `\params [when constant-list] -> expr` | `values-and-functions/lambda-expressions.md` §when ガード | **現行が異なる**: `when` の後は任意の `bool` 式のガード |
| F3 | 複数行のラムダ | 同 | 同じ形である |
| F4 | `\x -> \y` と `\x y` は同じ（カリー化） | `functions.md`・`lambda-expressions.md` | 同じ形である |
| F5 | 最後の値が戻り値。`ignore` で `unit` | `functions.md` | 同じ形である |
| F6 | ジェネリクス `'T` と `@'T : Add` | `values-and-functions/generics-functions.md` | 同じ形である |
| F7 | `#value` 制約と `'T.value` | 同 | 同じ形である（例を実行して確認） |
| F8 | 部分適用 | `functions.md` | 同じ形である |
| F9 | パイプライン `\|>` | 同 | 同じ形である（例を実行して確認） |
| F10 | 合成 `>>` | 同（`<<` も） | 同じ形である |
| F11 | ラムダのパターン引数（`\(l, r)`・`\()`）・高階関数・`->` は右結合 | `lambda-expressions.md` | 同じ形である |
| F12 | キャプチャとメモリ（Copy は複製、非 Copy は move、`ref mut` は保存できない） | `lambda-expressions.md` §捕捉・§共有借用の寿命・§保存できない値 | 同じ形である |
| F13 | 再帰 `def rec` と相互再帰の `and` | `functions.md` | 同じ形である（例が `run=` の出力と一致） |
| F14 | 関数名は snake_case を推奨 | `functions.md` | 同じ形である |

内訳: 46 項目のうち、現行に同じ形である 40、現行が異なる 6（E2・L4・L5・N2・O8・F2）。コードブロックは 20 件（複数ファイルの例は 1 件と数える）で、`tsuzuri check` を通るのは 15 件（うち 4 件は未使用の `let` の `W1001` の警告だけ）。通らない 5 件は、構文の雛形 4 件（`try … with … finally` の雛形・`def function_name :: parameter-type-list`・`\parameter-list [when constant-list] -> expression`・`namespace [parent-namespaces::]identifier`）と、定義が次のブロックにあるパイプラインの断片 1 件（`100 |> function1 |> function2`）。
