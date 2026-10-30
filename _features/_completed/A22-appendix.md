# A22 付録: 実測の記録・診断の例・コーパスの数・テストの表

本文は [A22-edition-2027-canonical-forms.md](A22-edition-2027-canonical-forms.md)。この付録は、本文から参照する長い表をまとめる（HEAD `a2313d19`。作業ツリーの HEAD は `9dd0fbbc`で、`git diff a2313d19 9dd0fbbc` は `docs/benchmarks.md` だけ）。

## 付録 A: 現状の実測（本文「現状」の全記録）

すべて `target/release/tsuzuri`（HEAD の release ビルド）で実行した結果。ソースは `/tmp/tz-A22/x/<名前>.tz`（1 ファイル 1 プログラム）に作り、`tsuzuri script <名前>.tz` で実行した。`check`・`build` はファイルを渡しても**同じ親ディレクトリの兄弟ファイルをすべて読む**ので、IR の比較は 1 プログラム 1 ディレクトリの `Main.tz` で行う。着手時に同じソースで再確認する。

### A.1 範囲（`for` とスライス）

| 名前 | ソース（要点） | 結果 |
| --- | --- | --- |
| r01 | `for i in 1 .. 3 do total = total + i` | `6`（両端を含む） |
| r02 | `1 .. 2 .. 9`／`10 .. -2 .. 4`／`3 .. 1`／`1 .. 3 + 1`／`(1 .. 3)` の和 | `a=25 b=28 c=0 d=10 e=6`（3 つ目は空。4 つ目は `1 .. (3 + 1)`。ステップは**真ん中**に書く） |
| r05 | `for i in 1 ..< 3 do` | `r05.tz:2:14: error[E0002]: expected an expression`（`<` の位置） |
| r06 | `for i in 1 ..= 3 do` | `r06.tz:2:14: error[E0002]: expected an expression`（`=` の位置） |
| r07b | `ref xs[1..3]`・`ref xs[2..]`・`ref xs[..2]`（`xs = [10, 20, 30, 40, 50]`）の和と長さ | `part=50 tail=120 head=30 lens=2,3,2`（半開） |
| r08 | `ref xs[1..=3]` | `r08.tz:2:22: error[E0002]: expected an expression` |
| r09 | `let r = 1 .. 3` | `error[E1005]: a range is an enumerable expression for 'for...in', not a stored value` |
| r11・r13 | `&xs[1..3]`、`&mut xs[1..3]` | どちらも受理（`2`、書き込みも通る） |
| r12 | `let a = xs[1..3]`（借用なし） | `error[E0002]: array slices must be borrows; write 'ref xs[start..end]' for a shared slice or 'ref mut xs[start..end]' for an exclusive slice` |
| r14 | `for i in 0l .. 3l`（`i64`）、`for i in 5 .. 7`、入れ子の `for y in x .. 3` | `a=6 b=18 c=25` |

`ExprKind::Range { start, step, finish, counted, descending }` は 1 つの AST で、`for i = a to b` / `downto`（`counted = true`）も同じ型を使う。スライスの借用は、`&` や `ref` を**スライスの式が飲み込み**（`ExprKind::Slice { value, start, end, mutable }`）、`ExprKind::Borrow` を作らない（`Notation` が残らない）。`Lexer::symbol` の最長一致の表には `..` があり `..<`・`..=` はない。

### A.2 接尾辞

`numeric::SHORT_SUFFIXES` は 15 個（`y`→`i8`、`uy`→`i8u`、`s`→`i16`、`us`→`i16u`、`u`→`i32u`、`l`→`i64`、`ul`→`i64u`、`L`→`i128`、`UL`→`i128u`、`hf`→`f16`、`f`→`f32`、`F`→`f128`、`hm`→`d32`、`m`→`d64`、`M`→`d128`）。`NUMERIC_NAMES` は 20 個（17 個の数値型名と `ubyte`・`sbyte`・`byte`）。`Lexer::number` は短い接尾辞を型名に直してから `Integer(text)`／`Float(text)` のトークンにする（`86uy` と `86i8u` は同じ `Integer("86i8u")`）ので、**綴りはトークンから失われる**（`Token.span` は元の綴りを覆う）。`86I` は `bigint` で型名の綴りがない。

18 通り（15 個の短い接尾辞と 3 個の別名）と、16 進・2 進・桁区切り・小数・指数・負の数との組み合わせを、1 つの `Main.tz`（`let a = 86y` … `$"{a} …"`）にして型名の綴り版と比べた。

| 比べたもの | 結果 |
| --- | --- |
| `tsuzuri run` の出力（短い綴り／型名の綴り） | どちらも `86 86 86 86 86 86 86 86 86 1 1 1 1 1 1 86 86 86 16 5 1000 1.5 2000 -128` |
| `tsuzuri build --emit llvm -O0`・`-O3` | どちらも 2 つの IR は 1 byte も違わない（`cmp` が一致） |

`export def f :: unit -> i8` は `E1008`（公開 ABI は狭い整数を結果にできない）なので、試験は `let` と補間で行う。

### A.3 借用と参照外し

| 名前 | ソース | 結果 |
| --- | --- | --- |
| b01 | `twice (&x)`／`twice (ref x)`／`twice &x`／`twice ref x`／`&mut y`／`ref mut y` | すべて受理。`42 42 42 42` |
| b02・b03 | `*r = *r + 1` と `deref r = deref r + 1`、`inc (&mut y)` と `inc (ref mut y)` | どちらも `7` |
| b06 | `def g :: &&i64 -> i64 = \r -> **r + 1`、`let rx = &x`、`g (&rx)` | `42`。`&&T` は型としては受理、**式の `&&x` は `E0002`**（b05: `expected an expression`） |
| b04 | `let rr = &r`（`r: ref i64`）で `**rr`、`let rr = ref r` で `deref rr` | どちらも `42`。**型が違う**（`&r` は `ref ref i64`、`ref r` は借り直した `ref i64`。`check.rs` の `reborrow_operand` の注釈: 「Keyword `ref r` on a reference `r` builds the same tree as the symbol reborrow `&*r`」） |
| b08 | `len2 (&[])`（`len2 :: ref [i64] -> i64`）、`len2 (&[1, 2, 3])`、`len2 (ref [1, 2, 3])` | `3 3 0`（記号の借用は期待する型を被演算子へ渡す） |
| b09 | `len2 (ref [])` | `b09.tz:3:19: error[E1004]: an empty array needs a type annotation, for example '[i64]'` |
| d01 | `*r + 1`／`deref r + 1`、`-*r`／`- deref r`、`*r as i64`／`deref r as i64`、`sq *r`／`sq deref r`、`get r *r`／`get r deref r`、`*r == 6`／`deref r == 6` | 各組が同じ値（`7 7 -6 -6 6 6 36 36 12 12 true true`） |
| d02 | `*id_ref r`（`id_ref :: ref i64 -> ref i64`）と `deref (id_ref r)` | `7 7`。`deref id_ref r` は `error[E0002]: 'deref' takes exactly one operand; write '(deref id_ref)' when more arguments follow, or parenthesize an application operand` |
| d05・d08 | `add *r *r`、`add (deref r) (deref r)`、`add deref r deref r` | 前 2 つは `12`。3 つ目は `E0002`（同じ文言）。**先頭の `*` の被演算子は application を含む（`PREFIX` = 15 < `APPLICATION` = 16）が、キーワードの `deref`・`ref` と引数位置の記号の接頭辞の被演算子は term（`TERM` = 17）で、後ろに引数が続くと `E0002`** |
| d04 | `**rr`、`deref deref rr`、`deref (deref rr)`、`* *rr` | すべて `5` |
| d06 | `total (ref xs[1..3])`、`total (&xs[1..3])`、`total &xs[1..3]` | すべて `2` |
| d07 | `total (ref (xs[1..3]))` | `error[E0002]: array slices must be borrows; write 'ref xs[start..end]' ...`（**スライスの式を括弧で包んではいけない**。借用ごと包む） |

`api-diff` は `export def f :: &i64 -> i64` と `export def f :: ref i64 -> i64` を同じ署名とみなす（`summary: 0 breaking, 0 non-breaking`）。

### A.4 旧ラムダ

| 名前 | ソース | 結果 |
| --- | --- | --- |
| l01 | `Array.map (x -> x + 1) xs`、`x -> y -> x + y` と `\x -> \y ->` と `\x y ->` を 1 2 に適用 | `[2, 3, 4] [2, 3, 4] 3 3 3` |
| l03 | `Array.map ((p, q) -> p + q) pairs` | `error[E0002]: expected the closing delimiter`（旧ラムダの引数は**識別子 1 つだけ**） |
| l04・l05 | `x -> match x with ...`、`x -> \y -> x + y` | 受理（後者は内側が明示のラムダでも 1 つに結合される） |

IR の比較（1 プログラム 1 ディレクトリ、`--emit llvm -O0`。`export def ra :: i64 -> i64 = \n -> let a = <ラムダ> in a n 1`）:

| `<ラムダ>` | 比較 |
| --- | --- |
| `x -> y -> x + y` と `\x y -> x + y` | **1 byte も違わない**（3,236 byte） |
| `x -> y -> x + y` と `\x -> \y -> x + y` | 違う（3,236 byte と 3,686 byte。結合した版は直接呼び出し `call i64 @tz.fn.$lambda.1(i64 %v6, i64 1)` 1 回、入れ子の版はクロージャを返す間接呼び出しと、その結果の間接呼び出しの 2 回） |
| `x -> (\y -> x + y)`（本体が括弧に包まれた明示のラムダ）と `\x y -> x + y` | **1 byte も違わない**（括弧があっても結合される） |
| トップレベルの `def a = x -> y -> ...`・`\x y ->`・`\x -> \y ->` | 3 つとも同じ（`def` の本体の入れ子のラムダは構文解析が 1 つにする） |

`lambda()`（`parser.rs`）は、本体が `ExprKind::Lambda` なら括弧に包まれていても引数を結合する。`explicit_lambda`（`\`）は結合しない。**したがって `x -> y -> e` を `\x -> \y -> e` に書き換えると IR が変わる（性能の経路が変わる）。正しい書き換えは `\x y -> e`**（D11）。`x -> e` は言語リファレンスの `lambda-expressions.md` に書かれていない（基本形は `\引数 -> 本体`）。

### A.5 警告・CLI・edition・診断

- `--warn` が受け取る名前は `implicit-copy` だけ。`tsuzuri script --warn canonical r02.tz` → `<command line>:1:1: error[E2000]: unknown warning 'canonical' for --warn; supported: implicit-copy`。`--warn implicit-copy` を 2 回 → `warn implicit-copy specified more than once`。`copies::warnings(&module)` は `main.rs` の `run` の経路で、フラグがあるときだけ呼ばれ、`module.warnings` に連ねて出る。
- 診断の JSON（現行の 5 キー）: `{"severity":"warning","code":"W1001","message":"unused local 'i'; prefix it with '_' to silence this warning","path":"r02.tz","span":{"start":124,"end":125,"line":8,"column":5,"end_line":8,"end_column":6}}`。
- `Edition::SUPPORTED` は `[E2026]` だけ。`tests/editions.rs` は `SUPPORTED == [E2026]`、`supported editions: 2026` の文言、`new_projects_declare_the_latest_edition`（`Edition::LATEST` を期待）を固定している。`src/package.rs` の単体テストにも `unsupported edition '2030'; supported editions: 2026` が 2 箇所ある。`tsuzuri new` は `Edition::LATEST` を `edition = "..."` に書く。
- `E0004`・`W1007` は HEAD のどこにもない（`grep -rn "E0004\|W1007" src tests docs _tsuzuri README.md` が 0 件）。
- `Parser::new(source, tokens)` は edition を持たない（edition は `lex` にだけ渡る）。構文解析の失敗は `Result<Program, Vec<Diagnostic>>`（`program_all`）で、**成功した構文解析だけ**が frontend cache に入る。構文解析は後戻りしない（誤りからの復帰で位置を進めるだけ）。補間の穴の式は主のトークン列の中にあり、スパンは実際のファイルの位置。

## 付録 B: 診断の例（本文の「例 1」以外）

**例 1 の `--json`**（1 行。D-51 のキーの順。G22 の前は `kind`・`help`・`fixes` を出さない）:

```text
{"severity":"warning","code":"W1007","message":"'..' is not canonical; write '..='","path":"Main.tz","span":{"start":29,"end":31,"line":2,"column":12,"end_line":2,"end_column":14},"kind":"bare_range_in_for","help":"write '..=' to include the end or '..<' to exclude it","fixes":[{"title":"Replace '..' with '..='","applicability":"certain","edits":[{"path":"Main.tz","span":{"start":29,"end":31,"line":2,"column":12,"end_line":2,"end_column":14},"text":"..="}]}]}
```

**例 6**（`E0004`。想定出力。`edition = "2027"` のプロジェクトで `for i in 1 .. 3 do` のある行と `&x` のある行。誤りは 1 つの列に並び、fix はない）:

```text
Main.tz:2:12: error[E0004]: '..' is not allowed in edition 2027; write '..='
  2 | for i in 1 .. 3 do
    |            ^
  help: write '..=' to include the end or '..<' to exclude it

Main.tz:7:8: error[E0004]: '&' is not allowed in edition 2027; write 'ref'
  7 | twice (&x)
    |        ^
  help: write 'ref x' to borrow x
```

**例 2**（想定出力。`Main.tz` の 1 行目が `let a = 86uy`）:

```text
Main.tz:1:9: warning[W1007]: '86uy' is not canonical; write '86i8u'
  1 | let a = 86uy
    |         ^
  help: write the type name after the number, as in '86i8u' or '1f32'
  fix (certain): Replace the suffix 'uy' with 'i8u'
```

fix の編集は接尾辞だけ（`start` 10、`end` 12、`text` `i8u`）。

**例 3**（想定出力。`Main.tz` = `def twice :: ref i64 -> i64 = \r -> deref r * 2` / `let x = 21` / `twice (&x)`）:

```text
Main.tz:3:8: warning[W1007]: '&' is not canonical; write 'ref'
  3 | twice (&x)
    |        ^
  help: write 'ref x' to borrow x
  fix (certain): Replace '&' with 'ref'
```

編集は `&`（`start` 66、`end` 67）を `ref ` に置換する 1 件。`twice &x` と書いてあれば、項の位置なので、`&`（`start` 65）を `(ref ` に置換し、`x` の直後（`start` = `end` = 67）に `)` を挿入する 2 件になる。

**例 4**（想定出力。連鎖の旧ラムダ `let m = Array.map (x -> y -> x + y) xs`）:

```text
Main.tz:1:20: warning[W1007]: 'x -> y ->' is not canonical; write '\x y ->'
  1 | let m = Array.map (x -> y -> x + y) xs
    |                    ^
  help: start the lambda with a backslash, as in '\x y -> body'
  fix (certain): Replace 'x -> y ->' with '\x y ->'
```

編集は `x` の始まりへの `\` の挿入と、`x` の終わりから `y` の始まりまで（` -> `）の 1 つの空白への置換の 2 件。

**例 5**（想定出力。参照の借用。`Main.tz` の 5 行目が `let rr = &r`、`r` は `let r = ref x`）:

```text
Main.tz:5:10: warning[W1007]: '&' is not canonical; write 'ref'
  5 | let rr = &r
    |          ^
  help: a reference to a reference cannot be written with 'ref'; 'ref r' reborrows r, so pass 'r' itself or restructure the code
```

fix はない（`reference_borrows` が捨てる）。

## 付録 C: コーパスの数（HEAD。本文の「目的」の根拠）

| 数えたもの | コマンド（リポジトリの根で） | 結果 |
| --- | --- | --- |
| `for` の素の `..` の行 | `grep -rEn '^\s*for [^=]+ in [^"]* \.\. [^"]* do' <dir> --include='*.tz' --include='*.tt' --include='*.tc'` | std 82、`tests/fixtures` 146、`examples` 0、`benchmarks` 57（文書: `_tsuzuri`・`docs`・README に 12） |
| 記号の借用（前置の `&`）の行 | `grep -rEn '(\(\|[[:space:]])&(mut[[:space:]]+)?[a-zA-Z_(]' <dir> --include='*.tz' --include='*.tt' --include='*.tc'` | std 16、`tests/fixtures` 204、`examples` 0、`benchmarks` 7（文書: `_tsuzuri`・`docs`・README に 50。`--include` なし） |
| 旧ラムダ（`(x ->` の行。型の `::` を除く） | `grep -rEn '\([a-z_][A-Za-z0-9_]* ->' <dir> \| grep -v '::'` | std 3、`tests/fixtures` 164、`examples` 0、`benchmarks` 3（文書 62） |
| 短い接尾辞（`byte`・`ubyte`・`sbyte` を除く 15 個）の数値リテラル | `grep -rEoh --include='*.tz' --include='*.tt' --include='*.tc' '\b[0-9][0-9_]*(\.[0-9]+)?([eE][+-]?[0-9]+)?(y\|uy\|s\|us\|u\|l\|ul\|L\|UL\|hf\|f\|F\|hm\|m\|M)\b' <dir>` | `tests/fixtures` に 35（`l` 10、`m` 5、`f` 5、`F` 4、そのほかの 11 種類は 1 ずつ）。std・examples・benchmarks は 0 |
| `byte`・`ubyte`・`sbyte` の接尾辞（正準として残す） | 同じ正規表現の選択肢を `byte\|ubyte\|sbyte` に替えて数える | std 197、`tests/fixtures` 14、`examples`・`benchmarks` 0（合計 211。ほとんどが `ubyte`） |
| `..<`・`..=` を含む Tsuzuri のソース | `grep -rEn '\.\.[<=]' std tests/fixtures examples benchmarks --include='*.tz' --include='*.tt' --include='*.tc'` | 0（文書の `for-to.md` の Rust との比較の表に 3 行ある） |
| コーパスのプロジェクト | `ls -d tests/fixtures/*/ \| wc -l`、`Main.tz` のあるもの | fixtures 100 件（`Main.tz` あり 93）と examples 13 件（あり 9）。`Main.tz` なしの fixture は 7 件（ライブラリ）と、トップレベルの `Semantics.tz` |
| `TypedExprKind::ForRange` の出現 | `grep -c ForRange src/*.rs` | 14 ファイル 24 行 |

手順 1 の IR の比較のループは、`Main.tz` のある 102 件（fixtures 93 + examples 9）すべてが `--emit llvm -O0` で成功する。出力は 1 件あたり約 0.75 MB（std を含む）で、合計は約 75 MB。fixture と example に同じ名前（`computations`・`control`・`currying`・`polymorphism`・`tasks`）があるので、出力名は `fixtures_`・`examples_` の接頭辞で分ける。

## 付録 D: 同じ意味の書き方のうち、本チケットが対象にしないもの（実機で受理を確認）

`o02.tz` が `8 8 14 14 6 6 -6 -6 3 3 2 2 2 2 16 16` を出し、次がすべて受理されることを確かめた。どれも意味が同じ 2 つの書き方で、D-54 の対象ではない。edition の差を増やさないため、本チケットでは正準を決めない（必要になったら別チケットで、同じ手順（`W1007` の kind を足す）で扱う）。

| 書き方 1 | 書き方 2 | 対象にしない理由 |
| --- | --- | --- |
| `a &&& b`、`a ||| b`、`a ^^^ b`、`~~~a` | `a & b`、`a | b`、`a ^ b`、`~a` | `2027` で前置の `&` が消えるので `&` は二項の ビット積だけになり、曖昧さが残らない。どちらも文書にある |
| `Array<i64>` | `[i64]` | どちらも型として文書にある（`main` の引数は `Array<string>`） |
| `fn(i64) -> i64` | `i64 -> i64` | `fn(...)` の型は文書で使われる |
| `else if` | `elif` | どちらも文書にある |
| `for i = a to b do` | `for i in a ..= b do` | `to` は別の構文（ループ変数は `i32` に固定） |
| `byte`・`ubyte`・`sbyte` の接尾辞 | `i8u`・`i8` | 型名である（D2） |

## 付録 E: アルゴリズムの詳細

**A1. 範囲の構文解析の疑似コード（本文「設計」の A1）。** `expression_inner` の `if minimum == 0 && !self.stop_at_slice_dotdot && self.eat(&TokenKind::DotDot)` の `eat` を、3 つのトークンのどれかを食べる形にして `range_expression` へ渡す。

```text
range_expression(first, op1 /* 食べた演算子のトークン */):
  next = expression_inner(1, ...)
  if op1 is DotDot and at(DotDot | DotDotEqual | DotDotLess):    // ステップ付き。最初の `..` はステップの印
      op2 = take(); finish = expression_inner(1, ...); step = Some(next)
  else: op2 = op1; finish = next; step = None
  if at(DotDot | DotDotEqual | DotDotLess) and (op2 is not DotDot or the next token is not DotDot):
      error E0002 "a range takes at most one step; write 'start .. step ..= finish'" at the token
  op = Bare / Closed / Open by op2
  if op == Bare: record_bare_range(op2.span)        // Phase 3
  Range { start: first, step, finish, counted: false, descending: false, op }
```

`(op2 is not DotDot or the next token is not DotDot)` は、素の `..` だけの連鎖（`a .. b .. c .. d`）を従来どおり構文解析で通す条件（HEAD では型検査が `E1005` にする。実測）。`computation.rs` の `expand` は `Range` を `..` で受けているので変更なし。

**A5. 定数評価器（`const_machine.rs`）。** `Loop::Range`・`range_start` を、`ForRange` の `exclusive` を読むようにする。`range_start` は `finish`・`step`・`start` を取り出して `step == 0` のトラップを調べたあと、`exclusive` なら、`step` の向き（符号なしは常に正）に従い、`start < finish`（正）／`start > finish`（負）で非空かを調べ、空なら `active = false`、非空なら `finish` を `finish - 1`／`finish + 1`（型の幅の中で巻き戻る）に置き換えてから既存の `within(start, step, finish, bits, signed)` を呼ぶ。

**A6. 範囲の解析（`ranges.rs`）。** `Scan::visit` が `loop_kind` へ `exclusive` を渡す。`loop_kind`（`local.ty == Type::I64` のときだけ）は `exclusive` のとき: `(start, step) == (Int(0), Int(1))` で `finish` が `length_root`（`array.length`）なら `Loop::BelowLength(array)`。`finish` と `start` が `SIGNED_LIMIT` 未満の `Int` で `step == Int(1)` かつ `start < finish` なら `Loop::Constant(start, finish - 1)`、`step == Int(MINUS_ONE)` かつ `start > finish` なら `Loop::Constant(finish + 1, start)`。それ以外は `None`（境界検査を残す。保守的）。`enter_counter`（PR02 の区間）は変えない（半開でも閉区間の式のまま。上端が 1 大きい上界で、健全）。

**A7. 接尾辞の検出（`Parser::literal`）。** `Integer(text)`・`Float(text)` のトークンのたびに、原文 `self.source[token.span.start..token.span.end]` へ `lexer::numeric_suffix` を呼ぶ。結果が `SHORT_SUFFIXES` のキーなら `record_short_suffix(span, suffix)`: `spelling` = 原文全体、`canonical` = 原文の接尾辞を `suffix_type(suffix)`（型名）に置換した文字列、主のスパン = リテラル全体、編集 = 接尾辞の部分（`start = span.end - suffix.len()`）を型名に置換（`Certain`）。`Parser::literal` は記号の `-` つきのパターン・補間の穴・属性の中でも同じ関数なので、1 か所で足りる。

**A8. 借用・参照外し・型の検出。** `prefix(allow_record, stop_at_newline, term)` が、`token.kind` が `Ampersand`・`Star`・`DoubleStar` のとき、被演算子 `value` を読んだあと（スライスの借用を返す早い `return` の前）に `record_symbolic_prefix(&token, mutable, term, &value)` を呼ぶ。`record_symbolic_prefix` は決定表のとおり `kind`・`spelling`・`canonical`・`level`・`edits` を決める（`&` は `is_place_path` で `Certain`/`Likely`、`*`・`**` は `Certain`）。`type_primary` が `Ampersand`・`AndAnd` を食べた直後に `record_symbolic_type(&token)`（`Certain`）。`ExprKind::Borrow(.., Notation::Symbol)` の型検査の分岐は、被演算子の型を `self.symbolic_borrows.push((value.ty.clone(), expression.span))` に積み、`Checker::finish` が推論の解決後に `Type::Reference` になったものの `span` を `reference_borrows` へ移す（`undecided_borrows` と同じ形）。

**A9. 旧ラムダの検出（`lambda`）。** `identifier_expression` が `Arrow` を食べた直後のトークンの範囲を `arrow` として `lambda(name, arrow, ..)` へ渡す。`lambda` は本体を読んだあと「旧ラムダの規則」の 4 つの場合に分けて `record_legacy_lambda` を呼ぶ。

**A11. 警告の運び。** `check_modules_collect` は、`modules` のうち `origin == ModuleOrigin::User && package.is_none()` のものの `program.noncanonical` を、`(source, start)` の昇順で `CheckedModule.noncanonical` に集める。その際 `Checker` の `reference_borrows`（関数ごとの worker の戻り値に載せる。`function_warnings` と同じ経路）に `(source, start)` が一致する `SymbolicBorrow` の記録は `level = Unavailable`・`edits = []` にする。`canonical::warnings(&module)` は各記録を `Diagnostic::warning("W1007", message, span)` にし、G22 があれば `with_kind`・`with_help`・`fix`（`level` が `Certain` → `Applicability::Certain`、`Likely` → `Likely`、`Unavailable` → なし）をつなぐ。`main.rs` は `--warn canonical` のとき `canonical::warnings(&module)` を `module.warnings` と `copies` の後ろに連ねる（G24 の `fix::canonical_warnings(project, module)` の本体も同じ呼び出し）。

**A12. `--edition`。** `Project::with_edition(self, edition: Edition) -> Result<Self, SourceError>`（新規。`driver.rs`）: `self.root_package.is_some()` なら `E2000`（パス `Tsuzuri.toml`）。そうでなければ `sources` のうち `origin == ModuleOrigin::User` の `edition` を `edition` にする。`main.rs` は `Project::load*` の直後に呼ぶ。frontend cache の parse key は各ソースの `edition` から作る（既存）ので、変更なし。

## 付録 F: 範囲とスライスの値の表

**`for` の範囲の値**（独立した参照は E2E の JavaScript の `referenceRange`。`MAX`・`MIN` は `i64` の最大・最小）:

| 型 | 範囲 | 値 |
| --- | --- | --- |
| `i32` | `0 ..< 3`／`0 ..= 3`／`3 ..< 3`／`3 ..= 3`／`5 ..< 2` | `[0,1,2]`／`[0,1,2,3]`／`[]`／`[3]`／`[]` |
| `i32` | `0 .. 2 ..< 7`／`0 .. 2 ..= 6`／`0 .. 2 ..< 6` | `[0,2,4,6]`／`[0,2,4,6]`／`[0,2,4]` |
| `i32` | `10 .. -3 ..< 0`／`10 .. -3 ..= 1`／`10 .. -3 ..< 1` | `[10,7,4,1]`／`[10,7,4,1]`／`[10,7,4]` |
| `i8` | `125 .. 2 ..= 127`／`125 .. 2 ..< 127`／`-128 ..< -126` | `[125,127]`／`[125]`／`[-128,-127]` |
| `i8u` | `250 ..< 255`／`250 ..= 255` | `[250,251,252,253,254]`／`[250,…,255]`（255 のあと値が範囲を外れるので止まる） |
| `i64` | `MAX-1 ..< MAX`／`MAX ..= MAX`／`MIN ..< MIN`／`MIN ..< MIN+1` | `[MAX-1]`／`[MAX]`／`[]`／`[MIN]` |
| `i32` | `0 .. 0 ..< 5`／`5 .. 0 ..< 5` | どちらも `range step is zero` のトラップ |

**スライスの値**（`xs = [10,20,30,40,50]`。`ref xs[..]`）:

| 書き方 | 結果 |
| --- | --- |
| `xs[1..<3]`／`xs[1..3]`／`xs[1..=3]`／`xs[..=1]`／`xs[..<2]` | `[20,30]`／`[20,30]`／`[20,30,40]`／`[10,20]`／`[10,20]` |
| `xs[0..=4]`／`xs[2..=1]`／`xs[0..=-1]`／`xs[..=-1]` | 5 要素／`[]`（`end = 2 = start`）／`[]`（`end = 0`）／`[]` |
| `xs[0..=5]`／`xs[2..=0]`／`xs[0..=-2]`／`xs[0..=MAX]` | `BoundsCheck` のトラップ（`end = 6 > 5`／`end = 1 < start`／`end = -1`（符号なしでは巨大）／`end = MIN`（符号なしで 2^63）） |

E2E の参照（JavaScript の BigInt）: `end = BigInt.asUintN(64, b + 1n)`、`start = BigInt.asUintN(64, a)` として、`start <= end && end <= 5n` なら `xs.slice(Number(start), Number(end))`、そうでなければトラップ。

## 付録 G: テストの表

### G.1 テストの一覧

| ファイル | テスト（手順） | 内容 |
| --- | --- | --- |
| `tests/editions.rs` | `edition_2027_is_supported_and_changes_no_generated_code`・`2027_is_a_supported_manifest_edition`・`cli_edition_reads_a_project_without_a_manifest`・`script_accepts_edition`（2〜3）、`new_projects_declare_edition_2027`・`new_project_edition_can_be_chosen`（17） | edition の足場、CLI、`tsuzuri new` |
| `tests/range_forms.rs`（新規） | `half_open_and_closed_ranges_are_accepted_in_every_edition`・`bare_and_closed_ranges_have_identical_ir`・`lexer_longest_match_for_range_operators`・`rejects_malformed_range_forms`（4〜5）、`slice_forms_lower_as_documented`（6）、`half_open_for_is_a_new_typed_loop`・`half_open_for_ir_is_deterministic_and_closed_ir_is_unchanged`・`half_open_const_loops_fold`（7） | 構文・IR・定数評価 |
| `tests/features.mjs` の suite `range_forms` | 8 | 値・トラップ・`live == 0`・WASM の import が空（native/wasm32 × `-O0`/`-O3`） |
| `tests/canonical_forms.rs`（新規） | `every_decision_row_is_reported`・`suffix_rules`・`borrow_and_deref_positions_parenthesize_exactly_when_needed`・`reference_operand_borrows_have_no_fix`・`legacy_lambda_chains_fold_into_one_lambda`・`std_and_dependencies_report_nothing`・`warn_canonical_is_off_by_default`・`deny_warnings_counts_w1007`（10〜11）、`json_has_kind_help_and_fixes`（12）、`corpus_certain_fixes_keep_the_ir`（13） | 検出・診断・fix の意味の保存 |
| `tests/edition_2027.rs`（新規） | `edition_2027_rejects_every_decision_row`・`edition_2027_reports_every_occurrence_in_one_run`・`edition_2027_accepts_the_canonical_forms`・`edition_2027_diagnostics_have_no_fixes`（14）、`edition_2027_does_not_reject_std_or_2026_dependencies`・`fmt_stops_on_noncanonical_forms_in_2027`・`a_2027_project_uses_the_frontend_cache_after_fixing`（15）、`corpus_migrates_without_changing_the_ir`（16） | `E0004`・std・依存・cache・移行 |
| `tests/edition_2027.mjs`（新規。G24 後） | 19 | `tsuzuri fix --edition 2027` の実行結果が移行前と同じ（native `-O0`、`live == 0`、2 回目は変更なし） |
| 単体テスト | `src/syntax.rs`: `rejects_noncanonical_starts_at_2027`（2）。`src/lexer.rs`: `lexes_range_operators`・`numeric_suffix_matches_the_lexer`。`src/ranges.rs`: `half_open_ranges_prove_bounds`。`src/canonical.rs`: `kinds_are_unique_and_short`・`messages_follow_the_templates`。`src/syntax_codec.rs`: 符号化の数の更新と往復。`src/main.rs`: `parses_warn_canonical`・`warn_names_are_each_allowed_once`・`parses_edition_option`・`rejects_bad_edition_options` | |

### G.2 `every_decision_row_is_reported` の行（構文解析だけ。型検査は不要）

「書き換え後」は、`Certain` と `Likely` の編集を当てた結果（手で書いた期待値）。`edition_2027_rejects_every_decision_row` は同じソースを `2027` で読み、記録 1 件につき `E0004` が 1 件（メッセージは `'{spelling}' is not allowed in edition 2027; write '{canonical}'`）出ることを確かめる。

| # | ソース | 記録（kind: `spelling` → `canonical`、level） | 書き換え後 |
| --- | --- | --- | --- |
| T1 | `for i in 1 .. 3 do ()` | `bare_range_in_for`: `..` → `..=`、Certain | `for i in 1 ..= 3 do ()` |
| T2 | `for i in 1 .. 2 .. 9 do ()` | 1 件（最後の `..`）、Certain | `for i in 1 .. 2 ..= 9 do ()` |
| T3 | `for i in 1 ..= 3 do ()`・`let a = ref xs[1..3]`・`for i = 1 to 3 do ()` | なし | 変わらない |
| T4 | `let a = 86uy` | `short_literal_suffix`: `86uy` → `86i8u`、Certain | `let a = 86i8u` |
| T5 | 15 個の短い接尾辞（`86y 86uy 86s 86us 86u 86l 86ul 86L 86UL 1hf 1f 1F 1hm 1m 1M`）と `1_000l`・`0x10l`・`0b101uy`・`1.5f`・`2e3f` | 各 1 件、Certain | 接尾辞だけが型名に（`86i8`・`86i8u`・`86i16`・`86i16u`・`86i32u`・`86i64`・`86i64u`・`86i128`・`86i128u`・`1f16`・`1f32`・`1f128`・`1d32`・`1d64`・`1d128`・`1_000i64`・`0x10i64`・`0b101i8u`・`1.5f32`・`2e3f32`） |
| T6 | `86i32u`・`86ubyte`・`86sbyte`・`86I`・`1f32`・`0xFFf`・`3` | なし | 変わらない |
| T7 | `let r = &x` | `symbolic_borrow`: `&` → `ref`、Certain | `let r = ref x` |
| T8 | `let r = &mut y` | `&mut` → `ref mut`、Certain | `let r = ref mut y` |
| T9 | `f &x` | `&` → `ref`、Certain（項の位置） | `f (ref x)` |
| T10 | `let r = &f x` | Likely（被演算子が application） | `let r = ref (f x)` |
| T11 | `f (&[1, 2, 3])` | Likely（被演算子が配列リテラル） | `f (ref [1, 2, 3])` |
| T12 | `f (&xs[1..3])`、`f &xs[1..3]` | Certain | `f (ref xs[1..3])`、`f (ref xs[1..3])` |
| T13 | `*r + 1` | `symbolic_deref`: `*` → `deref`、Certain | `deref r + 1` |
| T14 | `*f x` | Certain | `deref (f x)` |
| T15 | `add *r *r` | 2 件、Certain | `add (deref r) (deref r)` |
| T16 | `**rr`、`-*r` | `**` → `deref deref`、Certain | `deref deref rr`、`-deref r` |
| T17 | `def f :: &i64 -> i64 = \r -> deref r` | `symbolic_reference_type`: `&` → `ref`、Certain | `def f :: ref i64 -> i64 = ...` |
| T18 | `&mut i64`・`&&i64`・`&&mut i64` の型 | `&` → `ref`、`&&` → `ref ref`、Certain | `ref mut i64`・`ref ref i64`・`ref ref mut i64` |
| T19 | `let f = x -> x + 1` | `legacy_lambda`: `x ->` → `\x ->`、Certain | `let f = \x -> x + 1` |
| T20 | `let f = x -> y -> x + y` | 1 件: `x -> y ->` → `\x y ->`、Certain | `let f = \x y -> x + y` |
| T21 | `let f = x -> \y -> x + y` | 1 件、Certain | `let f = \x y -> x + y` |
| T22 | `let f = a -> b -> c -> a + b + c` | 1 件、Certain | `let f = \a b c -> a + b + c` |
| T23 | `let f = x -> (\y -> x + y)` | 1 件、Unavailable（編集なし） | 変わらない |
| T24 | `let f = \x -> x + 1`、`\x y -> x + y` | なし | 変わらない |
| T25 | `$"{&x}"`（補間の穴） | `&` → `ref`、Certain（スパンは実際のファイルの位置） | `$"{ref x}"` |
| T26 | `match n with \| 1l -> 0 \| _ -> 1` | `1l` → `1i64`、Certain | `match n with \| 1i64 -> 0 \| _ -> 1` |

別のテスト: `reference_operand_borrows_have_no_fix` は `let x = 41` / `let r = ref x` / `let rr = &r` を型検査まで通し、`module.noncanonical` の `SymbolicBorrow` が `Unavailable`・編集なしで、help が「例 5」のとおりであること。`std_and_dependencies_report_nothing` は、依存パッケージ（path 依存）と std の呼び出しを含む根で `--warn canonical` の報告が根のソースだけであること。

### G.3 E2E の suite `range_forms`（手順 8）

fixture `tests/fixtures/range_forms/Main.tz` の `export def`（引数と結果は `i64`）。本体は `let mut t: i64 = 0` / `for i in <範囲> do t = t * 31 + (i as i64)` / `t`。参照は JavaScript の `referenceRange(start, step, finish, exclusive)`（BigInt。`step > 0` なら `v <= finish`（半開は `<`）の間、`step < 0` なら `v >= finish`（半開は `>`）の間 `v += step`）と、`t = BigInt.asIntN(64, t * 31n + v)`。

| 関数 | `<範囲>` | 通る経路 | 引数のケース |
| --- | --- | --- | --- |
| `closed_i32`・`open_i32` | `(a as i32) ..= (b as i32)`／`..<` | 狭い整数・ステップ 1 | `(0,3)` `(3,3)` `(5,2)` `(-3,3)` `(2147483646,2147483647)` |
| `down_closed_i32`・`down_open_i32` | `(a as i32) .. -1 ..= (b as i32)`／`..<` | 狭い整数・ステップ -1 | `(3,0)` `(0,0)` `(0,3)` `(-2147483647,-2147483648)` |
| `closed_i64`・`open_i64` | `a ..= b`／`a ..< b` | 広い整数・ステップ 1 | `(0,3)` `(0,0)` `(5,2)` `(MAX-1,MAX)` `(MIN,MIN+1)` |
| `down_closed_i64`・`down_open_i64` | `a .. -1 ..= b`／`..<` | 広い整数・ステップ -1 | `(3,0)` `(MIN+1,MIN)` `(0,0)` |
| `stride_closed`・`stride_open` | `a .. s ..= b`／`..<`（`s` は変数） | 一般のステップ | `(0,2,7)` `(0,2,6)` `(10,-3,0)` `(10,-3,1)` `(5,1,5)` `(0,3,100)` |
| `u8_closed`・`u8_open` | `(a as i8u) ..= (b as i8u)`／`..<` | 符号なし・狭い | `(250,255)` `(0,0)` `(0,255)` `(255,255)` |
| `i8_stride_closed`・`i8_stride_open` | `(a as i8) .. (s as i8) ..= (b as i8)`／`..<` | 一般のステップ・狭い | `(125,2,127)` `(-128,50,127)` `(127,-1,126)` |
| `slice_closed`・`slice_open`・`slice_bare` | `ref xs[a..=b]`／`ref xs[a..<b]`／`ref xs[a..b]`（`xs = [10,20,30,40,50]`）の要素の和と長さを `len * 1000 + sum` に | スライス | 付録 F の表の全行（トラップ行は `traps`） |

`traps`: `stride_open [0n, 0n, 5n]`・`stride_open [5n, 0n, 5n]`・`stride_closed [0n, 0n, 5n]`（`range step is zero`）と、付録 F の `BoundsCheck` の行。`inspect(ir)`: 閉区間の関数の IR は手順 1 の形（`icmp sle`・`icmp eq`）を保つ。
