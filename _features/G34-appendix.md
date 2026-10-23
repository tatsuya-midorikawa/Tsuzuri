# G34 付録

[G34-doc-tests.md](G34-doc-tests.md) の付録。A 検証済みの例、B fixture と想定出力、C std の作業表、D 数え方。コマンドは、リポジトリの外の作業ディレクトリで `T=<repo>/target/release/tsuzuri` として実行する。

## 付録 A: 検証済みの例 22 件

HEAD の `tsuzuri` で、例の本文を `Main.tz` にして `run`（`-O0` と `-O3`）・`test`（同）・`check`（`-O0` のみ）を実行し、期待が一致した（2026-10-18）。期待値は、関数の署名と本体から手で作った値で、コンパイラの出力を写したものではない。`doc` コメントに入れるときは、各行に `/// ` を付ける。

````text
/// The sum of the elements, added from left to right.
///
/// ```tsuzuri run=6
/// Array.sum [1, 2, 3]
/// ```
def sum :: Numeric<'a> => ref ['a] -> 'a
````

隠し行の使い方（Phase 2 の後）:

````text
/// ```tsuzuri run=3
/// # let numbers = [1, 2, 3]
/// Array.length numbers
/// ```
````

| 関数 | 種別 | 本文 | 出力 |
| --- | --- | --- | --- |
| `Array.sum` | `run=6` | `Array.sum [1, 2, 3]` | `6` |
| `Array.map` | `run=[2,%204,%206]` | `Array.map (\x -> x * 2) [1, 2, 3]` | `[2, 4, 6]` |
| `Array.filter` | `run=[2,%203]` | `Array.filter (\x -> deref x > 1) [1, 2, 3]` | `[2, 3]` |
| `Array.reverse` | `run=[3,%202,%201]` | `Array.reverse [1, 2, 3]` | `[3, 2, 1]` |
| `Array.sort` | `run=[1,%202,%203]` | `Array.sort [3, 1, 2]` | `[1, 2, 3]` |
| `Array.contains` | `run=true` | `Array.contains [1, 2, 3] (ref 2)` | `true` |
| `Array.get` | `run=20` | `match Array.get 1 [10, 20, 30] with` ／ `\| Maybe.Some value -> value` ／ `\| Maybe.None -> 0`（3 行） | `20` |
| `String.split` | `run=["a",%20"b",%20"c"]` | `String.split "," "a,b,c"`（区切りが先） | `["a", "b", "c"]` |
| `String.contains` | `run=true` | `String.contains "ell" "hello"`（部分文字列が先） | `true` |
| `String.trim` | `run=hi` | `String.trim "  hi  "` | `hi` |
| `Maybe.default_value` | `run=0` | `Maybe.default_value 0 Maybe.None` | `0` |
| `Result.default_value` | `run=0` | `let failed: Result<i32, string> = Result.Error "bad"` ／ `Result.default_value 0 failed`（2 行） | `0` |
| `Map.length` | `run=1` | `let map = Map.insert (Map.empty()) 1 "one"` ／ `Map.length (ref map)`（2 行） | `1` |
| `Set.to_array` | `run=[1,%202]` | `let set = Set.insert (Set.insert (Set.empty()) 2) 1` ／ `Set.to_array (ref set)`（2 行） | `[1, 2]` |
| `Path.join` | `run=a/b` | `Path.join "a" "b"` | `a/b` |
| `Char.is_ascii_digit` | `run=true` | `Char.is_ascii_digit '7'` | `true` |
| `Maybe.Bind`（ビルダー） | `run=3` | `let total = Maybe {` ／ `    let! first = Maybe.Some 1` ／ `    let! second = Maybe.Some 2` ／ `    return first + second` ／ `}` ／ `Maybe.default_value 0 total`（6 行） | `3` |
| `IO.write_line` | `run=hello` | `IO.write_line "hello"` | `hello` |
| `Test.equal` | `test` | `test "sum" = Test.equal (ref (Array.sum [1, 2, 3])) (ref 6)` | 通る |
| `Gen.range` | `test` | `test "range stays in bounds" = Gen.for_all (Gen.range 0 10) (\n -> n >= 0 && n <= 10)` | 通る |
| `File.read_text` | （検査だけ） | `let content = File.read_text "notes.txt"` | 型が通る（`W1001` の警告は無視される） |
| `Array.sum` の拒否 | `fails=E1005` | `Array.sum ["a"]` | `E1005: no instance for Numeric<string>; ...` |

**書いてはいけない形**（実測）: `Maybe.Some 3`（末尾に置くと出力が空）、`Array.contains [1, 2, 3] 2`（`E1003`。`(ref 2)` が要る）、`String.contains "hello" "ell"`（型は通り `false`。引数の順序が逆）、`Result.Ok 3`（`E1015`: 型が決まらない）、`(Result.Ok 3: Result<i32, string>)`（`E0002`: 型注釈つきの式は書けない）、`Array.zip [1, 2] ["a", "b"]`（`E1005`: `Copy<string>` がない）、`Map.empty ()`（`E1006`。`Map.empty()` と書く）。

## 付録 B: fixture の全文と想定出力

置き場は `tests/fixtures/doctest/`。パスは、リポジトリの根を作業ディレクトリにして実行したときの表示。`duration_ms` は実行ごとに違う（テストは検査しない）。

### `pass/Counter.tz`（行番号つき。開きの行は 3・9・14・22・26・30）

```text
 1 /// Adds one to `n`.
 2 ///
 3 /// ```tsuzuri run=3
 4 /// Counter.add_one 2
 5 /// ```
 6 ///
 7 /// A hidden line sets up a value that the reader does not see.
 8 ///
 9 /// ```tsuzuri run=11
10 /// # let base = 10
11 /// Counter.add_one base
12 /// ```
13 ///
14 /// ```tsuzuri fails=E1003
15 /// Counter.add_one "two"
16 /// ```
17 def add_one :: i64 -> i64
18 fn add_one n = n + 1
19
20 /// Doubles `n`.
21 ///
22 /// ```tsuzuri test
23 /// test "doubles" = Test.equal (ref (Counter.double 4)) (ref 8)
24 /// ```
25 ///
26 /// ```tsuzuri
27 /// let _answer = Counter.double 21
28 /// ```
29 ///
30 /// ```tsuzuri ignore
31 /// Counter.double
32 /// ```
33 def double :: i64 -> i64
34 fn double n = n * 2
```

`tsuzuri test --doc tests/fixtures/doctest/pass --list`:

```text
0 Counter.add_one#1 [run]
1 Counter.add_one#2 [run]
2 Counter.add_one#3 [fails]
3 Counter.double#1 [test]
4 Counter.double#2 [check]
5 Counter.double#3 [ignore]
```

`--list --json` の 1 行目: `{"index":0,"kind":"run","line":3,"module":"Counter","name":"add_one#1","path":"tests/fixtures/doctest/pass/Counter.tz","type":"doctest"}`。

`tsuzuri test --doc tests/fixtures/doctest/pass`（標準出力。終了コード 0、標準エラーは空）:

```text
ok 1 - Counter add_one#1
ok 2 - Counter add_one#2
ok 3 - Counter add_one#3
ok 4 - Counter double#1
ok 5 - Counter double#2

5 passed; 0 failed; 1 ignored
```

`--json` の 1 行目と最後の行: `{"type":"doctest","index":0,"module":"Counter","name":"add_one#1","status":"passed","path":"tests/fixtures/doctest/pass/Counter.tz","line":3,"duration_ms":230}`、`{"type":"summary","passed":5,"failed":0,"ignored":1,"duration_ms":1100}`。

Phase 2 の後、`tsuzuri doc tests/fixtures/doctest/pass -o <dir>` の `Counter.md` の `add_one` の節の説明部分（`# ` の行がなく、`run=` と `fails=` が併記される）:

````text
Adds one to `n`.

```tsuzuri
Counter.add_one 2
```

Output:

```text
3
```

A hidden line sets up a value that the reader does not see.

```tsuzuri
Counter.add_one base
```

Output:

```text
11
```

```tsuzuri
Counter.add_one "two"
```

Compile error: `E1003`.
````

### `fail/Counter.tz`（開きの行は 3・7・11・15・19・23・27）

```text
 1 /// Adds one to `n`.
 2 ///
 3 /// ```tsuzuri run=4
 4 /// Counter.add_one 2
 5 /// ```
 6 ///
 7 /// ```tsuzuri
 8 /// let wrong: bool = Counter.add_one 1
 9 /// ```
10 ///
11 /// ```tsuzuri fails=E1003
12 /// let _fine = Counter.add_one 1
13 /// ```
14 ///
15 /// ```tsuzuri fails=E1003
16 /// Array.sum ["a"]
17 /// ```
18 ///
19 /// ```tsuzuri test
20 /// test "fails on purpose" = Test.equal (ref (Counter.add_one 1)) (ref 3)
21 /// ```
22 ///
23 /// ```tsuzuri test
24 /// let _nothing = Counter.add_one 1
25 /// ```
26 ///
27 /// ```tsuzuri run=
28 /// Array.sub 2 5 [1, 2, 3]
29 /// ```
30 def add_one :: i64 -> i64
31 fn add_one n = n + 1
```

`tsuzuri test --doc tests/fixtures/doctest/fail`（標準出力。終了コード 1）:

```text
not ok 1 - Counter add_one#1
  failure: expected output "4", found "3"
  at: tests/fixtures/doctest/fail/Counter.tz:3
not ok 2 - Counter add_one#2
  failure: E1003: expected bool, found i64 (example line 1, column 19)
  at: tests/fixtures/doctest/fail/Counter.tz:7
not ok 3 - Counter add_one#3
  failure: expected E1003, but the example was accepted
  at: tests/fixtures/doctest/fail/Counter.tz:11
not ok 4 - Counter add_one#4
  failure: expected E1003, found E1005: no instance for Numeric<string>; define an instance or use a supported type (example line 1, column 1)
  at: tests/fixtures/doctest/fail/Counter.tz:15
not ok 5 - Counter add_one#5
  failure: test "fails on purpose" failed: trapped or terminated by signal
  at: tests/fixtures/doctest/fail/Counter.tz:19
not ok 6 - Counter add_one#6
  failure: the example declares no test
  at: tests/fixtures/doctest/fail/Counter.tz:23
not ok 7 - Counter add_one#7
  failure: E2005: program terminated with signal: 5 (SIGTRAP); integer division, indexing, assert, or allocation may have trapped (example line 1, column 1)
  at: tests/fixtures/doctest/fail/Counter.tz:27

0 passed; 7 failed; 0 ignored
```

標準エラーの最後: `tests/fixtures/doctest/fail/Counter.tz:3:1: error[E2006]: 7 doc tests failed`（その行の引用つき）。5 件目と 7 件目の文言は OS で変わる（Windows のトラップの文言は `tests/bench.rs` の `TRAPPED` を参照）。

### `notation/Counter.tz`

```text
 1 /// Notation errors, one per example: `tsuzuri test --doc` reports all six and runs none.
 2 ///
 3 /// ```tsuzuri sloppy
 4 /// Counter.id 1
 5 /// ```
 6 ///
 7 /// ```tsuzuri run=%zz
 8 /// Counter.id 1
 9 /// ```
10 ///
11 /// ```tsuzuri run=1 test
12 /// Counter.id 1
13 /// ```
14 ///
15 ///  ```tsuzuri
16 ///  Counter.id 1
17 ///  ```
18 ///
19 /// ```tsuzuri
20 /// ```
21 ///
22 /// ```tsuzuri
23 /// Counter.id 1
24 def id :: i64 -> i64
25 fn id n = n
```

`tsuzuri test --doc tests/fixtures/doctest/notation --json`: 標準出力は空、標準エラーに `E2011` が 6 行、`span.line` は 3・7・11・15・19・22、メッセージは順に `unknown option 'sloppy' in a tsuzuri code fence; ...`、`'%' in run= must be followed by two hexadecimal digits; ...`、`a tsuzuri code fence takes one of run=, test, ignore, and fails=; found 'run=' and 'test'`、`a tsuzuri code fence must start at column 1 of the comment; remove the indentation`、`a tsuzuri example needs at least one line of code`、`the tsuzuri code fence is not closed; end it with a line of three backticks`。終了コード 1。

### `main/Main.tz` と `main/Lib.tz`

```text
Main.tz
 1 /// The value that this program prints. Its example is not run: it would replace this `Main.tz`.
 2 ///
 3 /// ```tsuzuri run=1
 4 /// 1
 5 /// ```
 6 def one :: i64
 7 fn one = 1
 8
 9 one()

Lib.tz
 1 /// Adds one to `n`.
 2 ///
 3 /// ```tsuzuri run=3
 4 /// Lib.add_one 2
 5 /// ```
 6 def add_one :: i64 -> i64
 7 fn add_one n = n + 1
```

`--list`: `0 Lib.add_one#1 [run]`、`1 Main.one#1 [run]`。`tsuzuri test --doc tests/fixtures/doctest/main`: 標準出力は `ok 1 - Lib add_one#1`、空行、`1 passed; 0 failed; 1 ignored`。標準エラーは `1 doc example in Main.tz not run; move the documented declaration to another module`。終了コード 0。

## 付録 C: std の作業表

測定: `tsuzuri doc std -o <dir>` の 47 ページ（HEAD `9dd0fbbc` の release）。「builder」は `.tc` の大文字で始まる `def`。「免除」は `Json` の derive 用の 10 個（`begin_object`・`encode_field`・`end_object`・`encode_case`・`encode_tag`・`expect_object`・`decode_field`・`keep_error`・`case_index`・`decode_payload`）。「目標」＝ 公開 `def` − builder − 免除。種別: `run` は決定的な関数に `run=`、`test` は `test` の例（`Gen`・`Test`）、`check` は検査だけ（OS・時間・ネットワーク・GPU・標準入力に触れる）。`run` のモジュールでも、OS の乱数や標準入力に触れる関数（`HashMap`・`HashSet` の `randomized`・`try_randomized`、`Random.bytes`・`Random.next_u64`、`IO.read_line`・`try_read_line`）は検査だけにする。

| モジュール | ファイル | 公開 `def` | 説明あり | builder | 免除 | 目標 | 種別 | 手順 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `Arena` | `std/Arena.tz` | 10 | 10 | 0 | 0 | 10 | run | 3.8 |
| `Array` | `std/Array.tz` | 39 | 6 | 0 | 0 | 39 | run | 3.1・3.2 |
| `Async` | `std/Async.tc` | 16 | 9 | 7 | 0 | 9 | check | 3.19 |
| `Atomic` | `std/Atomic.tz` | 0 | 0 | 0 | 0 | 0 | — | — |
| `Bench` | `std/Bench.tz` | 2 | 2 | 0 | 0 | 2 | check | 3.18 |
| `BigInt` | `std/BigInt.tz` | 4 | 4 | 0 | 0 | 4 | run | 3.12 |
| `Cbor` | `std/Cbor.tz` | 4 | 4 | 0 | 0 | 4 | run | 3.12 |
| `Channel` | `std/Channel.tz` | 0 | 0 | 0 | 0 | 0 | — | — |
| `Char` | `std/Char.tz` | 6 | 0 | 0 | 0 | 6 | run | 3.3 |
| `Debug` | `std/Debug.tz` | 2 | 0 | 0 | 0 | 2 | check | 3.18 |
| `Dir` | `std/Dir.tz` | 4 | 4 | 0 | 0 | 4 | check | 3.20 |
| `Env` | `std/Env.tz` | 3 | 3 | 0 | 0 | 3 | check | 3.20 |
| `Exception` | `std/Exception.tz` | 0 | 0 | 0 | 0 | 0 | — | — |
| `File` | `std/File.tz` | 14 | 14 | 0 | 0 | 14 | check | 3.20 |
| `Format` | `std/Format.tz` | 2 | 2 | 0 | 0 | 2 | run | 3.12 |
| `Gen` | `std/Gen.tz` | 22 | 22 | 0 | 0 | 22 | test | 3.14 |
| `Gpu` | `std/Gpu.tz` | 9 | 1 | 0 | 0 | 9 | check | 3.23 |
| `HashMap` | `std/HashMap.tz` | 24 | 6 | 0 | 0 | 24 | run | 3.7 |
| `HashSet` | `std/HashSet.tz` | 17 | 3 | 0 | 0 | 17 | run | 3.8 |
| `IO` | `std/IO.tc` | 25 | 1 | 11 | 0 | 14 | run | 3.18 |
| `Json` | `std/Json.tz` | 38 | 38 | 0 | 10 | 28 | run | 3.10・3.11 |
| `List` | `std/List.tz` | 5 | 1 | 0 | 0 | 5 | run | 3.5 |
| `Map` | `std/Map.tz` | 14 | 0 | 0 | 0 | 14 | run | 3.6 |
| `Math` | `std/Math.tz` | 1 | 0 | 0 | 0 | 1 | run | 3.5 |
| `Matrix` | `std/Matrix.tz` | 18 | 18 | 0 | 0 | 18 | run | 3.15 |
| `MatrixView` | `std/MatrixView.tz` | 29 | 29 | 0 | 0 | 29 | run | 3.16 |
| `Maybe` | `std/Maybe.tc` | 25 | 0 | 12 | 0 | 13 | run | 3.4 |
| `Mutex` | `std/Mutex.tz` | 0 | 0 | 0 | 0 | 0 | — | — |
| `Net` | `std/Net.tz` | 40 | 40 | 0 | 0 | 40 | check | 3.21・3.22 |
| `Os` | `std/Os.tz` | 6 | 6 | 0 | 0 | 6 | run | 3.19 |
| `Owned` | `std/Owned.tz` | 0 | 0 | 0 | 0 | 0 | — | — |
| `Parallel` | `std/Parallel.tz` | 1 | 0 | 0 | 0 | 1 | run | 3.5 |
| `Path` | `std/Path.tz` | 4 | 4 | 0 | 0 | 4 | run | 3.13 |
| `Process` | `std/Process.tz` | 1 | 1 | 0 | 0 | 1 | check | 3.19 |
| `Random` | `std/Random.tz` | 5 | 5 | 0 | 0 | 5 | run | 3.14 |
| `Regex` | `std/Regex.tz` | 18 | 18 | 0 | 0 | 18 | run | 3.12 |
| `Result` | `std/Result.tc` | 26 | 0 | 12 | 0 | 14 | run | 3.4 |
| `Seq` | `std/Seq.tz` | 7 | 0 | 0 | 0 | 7 | run | 3.5 |
| `Set` | `std/Set.tz` | 13 | 0 | 0 | 0 | 13 | run | 3.6 |
| `String` | `std/String.tz` | 21 | 0 | 0 | 0 | 21 | run | 3.3 |
| `Tensor` | `std/Tensor.tz` | 27 | 27 | 0 | 0 | 27 | run | 3.17 |
| `Test` | `std/Test.tz` | 3 | 0 | 0 | 0 | 3 | test | 3.5 |
| `Time` | `std/Time.tz` | 3 | 3 | 0 | 0 | 3 | check | 3.19 |
| `Unicode` | `std/Unicode.tz` | 24 | 24 | 0 | 0 | 24 | run | 3.13 |
| `Utf8Char` | `std/Utf8Char.tz` | 6 | 0 | 0 | 0 | 6 | run | 3.5 |
| `Utf8String` | `std/Utf8String.tz` | 20 | 0 | 0 | 0 | 20 | run | 3.9 |
| `Vec` | `std/Vec.tz` | 1 | 0 | 0 | 0 | 1 | run | 3.5 |
| **合計** | 47 ファイル | **559** | **305** | **42** | **10** | **507** | | |

ビルダーの例（各 `.tc` の `Bind` に 1 つ）: `Maybe`（3.4）・`Result`（3.4）・`IO`（3.18）・`Async`（3.19）。

### 手順ごとの関数

番号は、各モジュールの「目標」の関数（builder と免除を除く）を `tsuzuri doc std` の出力の順に数えたもの。`*` は、今の説明（doc コメント）が無い関数で、説明の 1 文も足す。

#### 手順 3.1（20 関数）

- `Array` の 1〜20 番目: length* is_empty* iter* get* at* init* sub* reverse* copy append* zip* map* mapi* map_ref* mapi_ref* fold* fold_ref* fold_back* fold_back_ref* reduce*

#### 手順 3.2（19 関数）

- `Array` の 21〜39 番目: sum* sum_pairwise sum_kahan dot dot_fma product* min* max* any* all* count* find* index_of* contains* equal* binary_search* sort* sort_in_place filter*

#### 手順 3.3（27 関数）

- `String` の 1〜21 番目: length* find* rfind* contains* starts_with* ends_with* slice* sub* decode_at* char_count* chars* join* concat* split* trim_start* trim_end* trim* to_ascii_lower* to_ascii_upper* repeat* replace*
- `Char` の 1〜6 番目: is_ascii_digit* is_ascii_lower* is_ascii_upper* is_ascii_alphabetic* to_ascii_upper* to_ascii_lower*

#### 手順 3.4（27 関数と builder 2）

- `Maybe` の 1〜13 番目: is_some* is_none* get* default_value* default_with* map* map_ref* bind* bind_ref* filter* or_else* to_result* of_result*
- `Result` の 1〜14 番目: is_ok* is_error* get* get_error* default_value* default_with* map* map_ref* map_error* bind* bind_ref* or_else* to_maybe* of_maybe*
- builder: `Maybe.Bind`・`Result.Bind` に 1 例ずつ（`BUILDER_FLOORS` を 1 に）

#### 手順 3.5（24 関数）

- `List` の 1〜5 番目: length* is_empty* copy fold* iter*
- `Seq` の 1〜7 番目: empty* once* defer* unfold* map* filter* to_array*
- `Vec` の 1 番目: iter*
- `Math` の 1 番目: zero*
- `Test` の 1〜3 番目: equal* not_equal* is_true*
- `Parallel` の 1 番目: sum*
- `Utf8Char` の 1〜6 番目: is_ascii_digit* is_ascii_lower* is_ascii_upper* is_ascii_alphabetic* to_ascii_upper* to_ascii_lower*

#### 手順 3.6（27 関数）

- `Map` の 1〜14 番目: empty* singleton* length* is_empty* contains_key* get* at* insert* remove* to_array* keys* values* fold* iter*
- `Set` の 1〜13 番目: empty* singleton* length* is_empty* contains* insert* remove* to_array* fold* union* intersect* difference* iter*

#### 手順 3.7（24 関数）

- `HashMap` の 1〜24 番目: empty* length* is_empty* sip13 with_capacity* with_seed with_capacity_and_seed try_randomized randomized contains_key_ref* contains_key* get_ref* get* at_ref* at* insert* remove_ref* remove* longest_probe to_array* keys* values* fold* iter*

#### 手順 3.8（27 関数）

- `HashSet` の 1〜17 番目: empty* with_capacity* length* is_empty* with_seed with_capacity_and_seed* try_randomized randomized* insert* remove_ref* remove* contains_ref* contains* longest_probe to_array* fold* iter*
- `Arena` の 1〜10 番目: empty with_capacity length contains get at insert remove update iter

#### 手順 3.9（20 関数）

- `Utf8String` の 1〜20 番目: length* find* rfind* contains* starts_with* ends_with* slice* sub* char_count* chars* join* concat* split* trim_start* trim_end* trim* to_ascii_lower* to_ascii_upper* repeat* replace*

#### 手順 3.10（14 関数）

- `Json` の 1〜14 番目: parse numeral to_i64 to_f64 to_utf8string to_utf8string_pretty encode decode serialize serialize_pretty deserialize reader next token_text

#### 手順 3.11（14 関数）

- `Json` の 15〜28 番目: token_matches token_numeral writer write_null write_bool write_number write_text write_json write_key open_array close_array open_object close_object finish

#### 手順 3.12（28 関数）

- `Regex` の 1〜18 番目: compile escape group_count is_match find find_at find_all captures replace_all split search_steps is_match_utf8 find_utf8 find_at_utf8 find_all_utf8 captures_utf8 replace_all_utf8 split_utf8
- `Format` の 1〜2 番目: parse pad
- `BigInt` の 1〜4 番目: of_i64 to_i64 of_string compare
- `Cbor` の 1〜4 番目: encode decode serialize deserialize

#### 手順 3.13（28 関数）

- `Unicode` の 1〜24 番目: version property_ranges simple_case_folding category normalize normalize_utf8 is_normalized is_normalized_utf8 grapheme_boundaries grapheme_boundaries_utf8 word_boundaries word_boundaries_utf8 graphemes graphemes_utf8 words words_utf8 to_lower to_lower_utf8 to_upper to_upper_utf8 to_title to_title_utf8 case_fold case_fold_utf8
- `Path` の 1〜4 番目: join parent file_name extension

#### 手順 3.14（27 関数）

- `Gen` の 1〜22 番目: range i64 i32 bool f64 char unicode_char array_up_to array string_of string maybe result constant map bind pair one_of element check for_all for_all_cases
- `Random` の 1〜5 番目: bytes next_u64 pcg pcg_next_u32 pcg_next_u64

#### 手順 3.15（18 関数）

- `Matrix` の 1〜18 番目: of_array init rows cols at get row as_array to_array set map fold transpose add mul mul_fma mul_parallel mul_fma_parallel

#### 手順 3.16（29 関数）

- `MatrixView` の 1〜29 番目: of_matrix of_array strided rows cols offset row_stride col_stride data at get transpose sub row col is_contiguous to_matrix fold map mul of_array_mut freeze write row_mut sub_mut transpose_mut fill copy_from map_in_place

#### 手順 3.17（27 関数）

- `Tensor` の 1〜27 番目: of_array init of_matrix to_matrix as_array to_array reshape view borrow rank shape count strides offset data at get permute index_axis narrow is_contiguous reshape_view fold to_tensor map as_matrix_view of_matrix_view

#### 手順 3.18（18 関数と builder 1）

- `IO` の 1〜14 番目: pure* bind* map* try_read_line* read_line* try_write* try_write_line* try_write_error* try_write_error_line* write* write_line* writeln write_error* write_error_line*
- `Debug` の 1〜2 番目: print* trace*
- `Bench` の 1〜2 番目: with_input of
- builder: `IO.Bind` に 1 例（`BUILDER_FLOORS` の `IO` を 1 に）

#### 手順 3.19（19 関数と builder 1）

- `Async` の 1〜9 番目: yield sleep now run all all_results host start block_on
- `Os` の 1〜6 番目: message encode error_of_status split_names decode decode_i64
- `Process` の 1 番目: run
- `Time` の 1〜3 番目: monotonic_ns unix_ns sleep_ms
- builder: `Async.Bind` に 1 例（`BUILDER_FLOORS` の `Async` を 1 に）

#### 手順 3.20（21 関数）

- `File` の 1〜14 番目: read_bytes read_text write_bytes write_text append_text remove open read write flush close with_open metadata link_metadata
- `Dir` の 1〜4 番目: list create remove walk
- `Env` の 1〜3 番目: args var current_dir

#### 手順 3.21（20 関数）

- `Net` の 1〜20 番目: parse_address parse_ip ip_text address_text port is_ipv6 error_kind resolve connect read write shutdown close stream_local_addr peer_addr bind accept local_addr close_listener bind_udp

#### 手順 3.22（20 関数）

- `Net` の 21〜40 番目: send_to recv_from udp_local_addr close_udp with_connection with_accepted with_listener with_udp connect_async accept_async read_async write_async recv_from_async send_to_async bind_async bind_udp_async close_async close_listener_async close_udp_async shutdown_async

#### 手順 3.23（9 関数）

- `Gpu` の 1〜9 番目: request* backend* init* map* init_relaxed* map_relaxed* last_backend from_array* to_array*

合計 507 関数（20+19+27+27+24+27+24+27+20+14+14+28+28+27+18+29+27+18+19+21+20+20+9）と builder 4 例。

### `tests/std_examples.rs` の骨子（手順 3.0）

```rust
//! Which std functions carry a doc example (G34 Phase 3). The floors only go up.
use std::collections::{BTreeMap, BTreeSet};
use tsuzuri::doctest::{self, Expectation, OwnerKind};
use tsuzuri::parser;
use tsuzuri::stdlib::SOURCES;
use tsuzuri::syntax::{Edition, Visibility};

/// (module, public non-builder functions that must have an example): 47 entries, all 0 at step 3.0, in the order of
/// the table above. A step raises the entries of its modules to the table's 目標 (a split module: to the count so far).
const FLOORS: &[(&str, usize)] = &[("Arena", 0), ("Array", 0), /* ... */ ("Vec", 0)];
/// Public functions that are exempt, with the reason in a comment each.
const EXEMPT: &[(&str, &[&str])] = &[("Json", &["begin_object", "encode_field", "end_object", "encode_case",
    "encode_tag", "expect_object", "decode_field", "keep_error", "case_index", "decode_payload"])]; // derive protocol
/// The `.tc` modules whose `Bind` must carry an example.
const BUILDER_FLOORS: &[(&str, usize)] = &[("Async", 0), ("IO", 0), ("Maybe", 0), ("Result", 0)];

struct Tally { functions: Vec<String>, with_example: BTreeSet<String>, bind_examples: usize }

fn tally() -> BTreeMap<String, Tally> {
    let mut modules = BTreeMap::new();
    for (path, text) in SOURCES {
        let module = path.trim_start_matches("std/").split('.').next().unwrap().to_owned();
        let program = parser::parse_with_source(text, 0, Edition::LATEST).unwrap();
        let exempt = EXEMPT.iter().find(|(name, _)| *name == module).map_or(&[][..], |(_, names)| *names);
        let functions = program.functions.iter()
            .filter(|function| function.visibility == Visibility::Public)
            .map(|function| function.name.text.clone())
            .filter(|name| !name.starts_with(|c: char| c.is_ascii_uppercase()) && !exempt.contains(&name.as_str()))
            .collect();
        let examples = doctest::examples_in_source(text, Edition::LATEST, 0, &module)
            .unwrap_or_else(|errors| panic!("{path}: {errors:?}"));
        let live = |example: &&doctest::Example| example.expectation != Expectation::Ignore;
        let with_example = examples.iter().filter(live)
            .filter(|example| example.owner_kind == OwnerKind::Function)
            .map(|example| example.owner.clone()).collect();
        let bind_examples = examples.iter().filter(live).filter(|example| example.owner == "Bind").count();
        modules.insert(module, Tally { functions, with_example, bind_examples });
    }
    modules
}
```

テスト 4 件（手順 3.0）: `std_examples_meet_the_floors`（`FLOORS` のキーが `SOURCES` の全モジュールと一致することも検査し、足りないモジュール名・余るモジュール名を出して失敗する。G34 の後に std のモジュールを足すチケットは、そのモジュールを `FLOORS` に足す。`module: functions / with_example` の表を `println!` する）、`exemptions_stay_below_one_fifth_of_the_public_functions`（`EXEMPT` の数と builder の数の和 × 5 < 公開 `def` の総数）、`every_std_example_has_valid_notation`（`tally()` が panic しない）、`builder_modules_show_a_computation_expression`。手順 3.24 の 5 件目: `every_public_std_function_has_an_example`（`tally()` の `functions` が `with_example` に全部含まれる）。

## 付録 D: 数え方（再現）

```sh
$T doc <repo>/std -o fx/stdapi
python3 count_std.py fx/stdapi        # 559 305 0（公開 def・説明あり・例のあるもの）
```

```python
import os
import re
import sys

root = sys.argv[1]  # the output directory of `tsuzuri doc std -o <dir>`
total = documented = examples = 0
for name in sorted(os.listdir(root)):
    if name == 'index.md':
        continue
    text = open(os.path.join(root, name), encoding='utf8').read()
    parts = re.split(r'^## `([^`]+)`\n', text, flags=re.M)
    for i in range(1, len(parts), 2):
        m = re.match(r'\n```tsuzuri\n(.*?)\n```\n(.*)', parts[i + 1], flags=re.S)
        if re.match(r'(export )?(const )?def\b', m.group(1).split('\n', 1)[0]):
            total += 1
            documented += bool(m.group(2).strip())
            examples += '```' in m.group(2)
print(total, documented, examples)
```

Phase 3 の後は、`cargo test --locked --test std_examples -- --nocapture` が同じ数を（例のあるものを含めて）出す。
