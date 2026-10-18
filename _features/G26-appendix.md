# G26 付録: Facts モデルの決定表・fixture・想定出力

[G26-facts-model.md](G26-facts-model.md) の付録。表の番号は本文が引く（B1・D2 のような番号）。**H の出力は手計算**（Python でスパンを数えた。コンパイラの出力を写していない）で、実装した出力と違うときは本文の停止条件 6。

## 付録 A: fixture `tests/fixtures/kb_basic/`（新規）

`tsuzuri check`（警告なし）・`fmt --check`・`test`（1 件通る）を確かめてある。`Main.tz` にトップレベルのコードを置かない（`tests/llvm_handoff.rs` が `"ok"` の entry を足すため）。`bench` は `tsuzuri test` では実行されない。

`Tsuzuri.toml`:

```toml
[package]
name = "kb-basic"
version = "0.1.0"
edition = "2026"
namespace = "Acme::Kb"
```

`Traits.tt`:

```tsuzuri
class Sized<'a> {
    def size :: ref 'a -> i64
}
```

`Geometry/Metrics.tz`:

```tsuzuri
/// The sum of the absolute coordinates.
export def norm1 :: f64 -> f64 -> f64
fn norm1 x y = abs x + abs y
```

`Shapes.tz`:

```tsuzuri
/// A point on the plane.
record Point { x: f64, y: f64 } deriving (Eq)

/// A counter between two flags.
record Packed { a: i8, n: i64, b: i8 }

/// A shape.
union Shape =
    | Circle of f64
    | Empty

/// A length in meters.
type Meters = f64

/// The unit radius.
const unit_radius: f64 = 1.0

extern type Handle

extern "kb_close" def close :: Handle -> i64

instance Traits.Sized<Point> {
    fn size _point = 2
}

/// The area of a shape.
def area :: ref Shape -> f64
fn area shape =
    match shape with
    | Circle r -> r * r * 3.0
    | Empty -> 0.0

def twice :: 'a -> 'a
fn twice value = value + value

def origin_point :: unit -> Point
fn origin_point _ = Point { x: 0.0, y: 0.0 }

def shift :: ref Point -> Meters -> Point
fn shift point dx = Point { x: point.x + dx, y: point.y }
```

`Main.tz`:

```tsuzuri
using Acme::Kb::Geometry

export def total :: ref [i64] -> i64
fn total values =
    let mut sum = 0
    for index in 0 .. values.length - 1 do
        sum = sum + values[index]
    sum

export def at :: ref [i64] -> i64 -> i64
fn at values index = values[index]

export def ratio :: i64 -> i64 -> i64
fn ratio left right = left / right

def rec sum_to :: i64 -> i64 -> i64
fn rec sum_to n acc =
    match n with
    | 0 -> acc
    | n -> sum_to (n - 1) (acc + n)

def rec fib :: i64 -> i64
fn rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)

def rec is_even :: i64 -> bool
def and is_odd :: i64 -> bool
fn rec is_even n = if n == 0 then true else is_odd (n - 1)
and is_odd n = if n == 0 then false else is_even (n - 1)

export def countdown :: i64 -> i64
fn countdown start =
    let mut n = start
    let mut steps = 0
    while n > 0 do
        n = n - 1
        steps = steps + 1
    steps

export def describe :: f64 -> f64
fn describe radius = Shapes.area (ref (Shapes.Circle radius)) + Metrics.norm1 radius 1.0

export def doubled :: i64 -> f64 -> f64
fn doubled n x = Shapes.twice x + (Shapes.twice n as f64)

export def filled :: i64 -> i64
fn filled n =
    let values = new [i64](n, i -> i * 2)
    total (ref values)

export def copies :: i64 -> i64
fn copies n =
    let values = [n, n, n]
    let first = total (ref values)
    let moved = values
    first + total (ref moved) + total (ref values)

private def add_to :: ref mut i64 -> i64 -> unit
fn add_to target amount = { deref target = deref target + amount; }

private def peek :: ref string -> i64
fn peek text = text.length

private def peek_twice :: ref string -> i64
fn peek_twice text = peek text + peek text

private def consume :: string -> i64
fn consume text = text.length

export def ownership :: i64 -> i64
fn ownership n =
    let mut total = n
    add_to (ref mut total) 1
    let label = "kb" + "!"
    let seen = peek_twice (ref label)
    let text = "own" + "ed"
    let owned = consume text
    total + seen + owned

export def traced :: i64 -> i64
fn traced n =
    Debug.print (ref n)
    n + 1

test "total adds" = assert (total (ref [1, 2, 3]) == 6)

bench "total of three" = \n -> n + total (ref [1, 2, 3])
```

## 付録 B: ID と `symbols` の決定表

### B1: ID の補足

- 完全名は `Names::module_display(key)`。manifest の `namespace` と、サブディレクトリのモジュールは名前空間に入る（`Geometry/Metrics.tz` → `Acme::Kb::Geometry::Metrics`）。manifest がなければ key の `.` を `::` にしたもの（`examples/point` は `Main`・`Point`）。
- std のモジュール（`ModuleOrigin::Std`）の完全名は `module_display` に `std::` が付くので、**付けずに key**（`Array`）を使う。`graph` の `to` に std の関数を書くときだけ。`symbols` に std の行は出さない。
- instance の ID の head は `type_id_text`（B2）。`Classes.Score` のように書かれた class 名は、解決した class の ID（`Classes.Score`、名前空間つきなら `Acme::Kb::Traits.Sized`）を使い、書かれたままの文字列を使わない。
- 衝突の例: `test "same"` が 2 つ → `test:Main.same`（先）・`test:Main.same#2`。`#n` を付けた ID を参照する行（`graph` の `from`・`to`、`tests` の `id`、`facts` の `id`）も同じ ID にする。

### B2: `type_id_text`（`Type` の全 30 variant）

| variant | 文字列 |
| --- | --- |
| `Record(id, args)`・`Union(id, args)` | `records[id].name`（`key.Name`）を最後の `.` で key と名前に分け、`module_display(key)` ＋ `.` ＋ 名前。`args` があれば `<` ＋ 各 `type_id_text` を `, ` で連結 ＋ `>` |
| `Handle(name)` | 同じ（`name` を最後の `.` で分ける） |
| `Variable(v)` | `'v`（`#N` は `N`） |
| `Integer`・`Binary`・`Decimal`・`Simd`・`Bool`・`Unit`・`Char`・`Utf8Char`・`String`・`Utf8String`・`Length`・`Dyn` | `Type::display(types)` |
| `Array(t)`・`ArrayView(t)`・`List(t)`・`Vec(t)`・`FixedArray(t, n)` | `[t]`・`[t..]`・`[\|t\|]`・`Vec<t>`・`[t; n]`（`n` は `Type::display`） |
| `Tuple(ts)` | `(a * b)`。`Function` の成分は括弧で囲む |
| `Task(t)`・`Shared(t, k)` | `Task<t>`・`k<t>`（`SharedKind::name`） |
| `Reference(t, m)` | `ref t`・`ref mut t`。`t` が `Function` なら括弧 |
| `Function(ps, r)` | `a -> b -> r`。成分が `Function` なら括弧。`ps` が空なら `fn() -> r` |
| `Error`・`Infer`・`Partial`・`Application` | `Type::display(types)`（ID には現れない） |

### B3: kind 別の行（`symbols`）

| kind | 元（`Program` の field） | `name` | `signature` | `params` / `result` | `public` / `export` | `span` |
| --- | --- | --- | --- | --- | --- | --- |
| `module` | `ModuleInput`（user・dependency。同じ key の 2 つ目以降のファイルは行を足さない） | 最後の `::` 以降 | `module <完全名>` | `[]` / `null` | `true` / `false` | 先頭のファイルの `0..0` |
| `def` | `functions`（`$` で始まる名前は除く。active pattern は `(\|A\|B\|)`） | 宣言の名前 | `docgen::signature(prefix, name, regions, constraints, 各引数の TypeExpr, result)`。`prefix` は `export `・`const `・`def`・` rec` の連結（`docgen::render_declarations` と同じ） | 引数名＝`declaration.parameters[i].name`（`$` で始まる・Generated は `arg{i+1}`）、型＝`type_text`、`conv`＝B4 / `type_text(result)` | `visibility == Public` / `exported` | 名前の識別子 |
| `const` | `constants` | 名前 | `const NAME: <type_text>` | `[]` / 型 | `visibility` / `false` | 名前 |
| `extern` | `externs` | 名前 | `docgen::signature("extern def", ..)` | 引数名は `arg{i+1}`、型は `type_text`、`conv` は B4 / 結果 | `visibility` / `false` | 名前 |
| `handle` | `extern_types` | 名前 | `extern type NAME` | `[]` / `null` | `visibility` / `false` | 名前 |
| `record` | `records`（Generated は除く） | 名前 | `record NAME` ＋ `parameters_text` ＋ `region_text` | `[]` / `null` | `visibility` / `false` | 名前 |
| `field` | `RecordDecl.fields` | フィールド名 | `NAME: <type_text>` | `[]` / 型 | 親の `visibility` / `false` | フィールド名 |
| `union` | `unions`（Generated は除く） | 名前 | `union NAME` ＋ `parameters_text` | `[]` / `null` | `visibility` / `false` | 名前 |
| `case` | `UnionDecl.cases` | case 名 | `NAME` または `NAME of <type_text>` | `[]` / payload の型か `null` | 親の `visibility` / `false` | case 名 |
| `alias` | `type_aliases` | 名前 | `type NAME` ＋ `parameters_text` ＋ ` = <type_text(target)>` | `[]` / target の型 | `visibility` / `false` | 名前 |
| `class` | `classes` | 名前 | `class <constraints_text(superclasses)>NAME<'v>`（kind が `*` でなければ `: <Kind::display>` を足す。`docgen` と同じ） | `[]` / `null` | `true` / `false` | 名前 |
| `method`（class） | `ClassDecl.methods` | メソッド名 | `docgen::signature("def", ..)` | 引数名は `arg{i+1}`、型は `type_text`、`conv` は `Classes::method_signature` の型で B4 / 結果 | メソッドの `visibility` / `false` | メソッド名 |
| `instance` | `instances`（`Classes::instances()` の `derived == false` のもの） | `format!("{}<{}>", 書かれた class 名, type_text(&instance.ty))`（`Classes.Score<Point>`） | `format!("instance {}{}<{}>", constraints_text(&instance.constraints), 書かれた class 名, type_text(&instance.ty))` | `[]` / `null` | `true` / `false` | class 名の識別子 |
| `method`（instance） | `InstanceView.methods` の関数 | メソッド名 | `def NAME :: <関数の signature の Type::display>` | 引数名＝`Definition.parameters` の名前、型＝`Type::display`、`conv` は B4 / 結果の `Type::display` | `true` / `false` | メソッド名の識別子 |
| `test` | `tests` | テストの名前 | `test "NAME"` | `[]` / `null` | `false` / `false` | 名前の文字列（`name_span`） |
| `bench` | `benches` | 名前 | `bench "NAME"` | `[]` / `null` | `false` / `false` | `name_span` |
| `def`（`$entry`） | `module.entry` の関数 | `$entry` | `entry :: <結果の Type::display>` | `[]` / 結果 | `false` / `false` | 関数の `span`（式の全体） |

どの kind も `pure: false`（A19 完了後は `FunctionDecl.pure`）、`doc`＝`Documentation.text`（record・union・alias・const・extern・class・メソッド・def にだけある。ほかは `null`）、`deprecated`＝`Documentation::deprecation()`、`origin`＝モジュールの `origin`。

### B4: `conv`

| 型（解決後の `Type`） | `conv` |
| --- | --- |
| `Reference(_, false)` | `ref` |
| `Reference(_, true)` | `ref_mut` |
| 上以外で `ty.is_copy(&types)` | `copy`（整数・浮動小数点・`bool`・関数値・全フィールドが Copy の record） |
| それ以外（`string`・配列・`Vec`・型変数・Drop を持つ型・`Task`） | `own` |

## 付録 C: 関数と宣言の対応

### C0: 抽象の関数 id → symbol の ID（`Capture.function_ids` の作り方）

`collect_abstract` は、モジュールごとに次の順で `function_ids.insert((function.module, function.name), ID)` する。関数 id は `names.local_function(key, name)`（`def`・`const`・`extern`）、`module.tests[i].function`（test）、`module.benches[i].function`（bench）、`module.entry`（`$entry`）、`Classes::instances()` の `methods` の関数 id（instance のメソッド。`name` は `$instance.<id>.<method>`）。`function_ids` に入らない関数（`derive`・`dyn`・active pattern の補助・`$export`・`$builtin`・`$case`・`$intrinsic`・`$lambda`）は、`calls` の `from` にも `to` にもならない。std の関数（`origin.module == Std`）は `to` にだけ使い、ID は `<function.module>.<function.name>`。

### C1: `extent`（宣言の範囲）

| 宣言 | `extent` |
| --- | --- |
| def・const | 名前の先頭から `body.span.end` |
| extern・class のメソッド | 名前の先頭から `result.span.end` |
| record | 名前の先頭から最後のフィールドの型の終わり（なければ名前の終わり）。field は名前の先頭からその型の終わり |
| union・case | 名前の先頭から最後の case（payload があればその型）の終わり。case は case 名から payload の終わり |
| alias | 名前の先頭から `target.span.end` |
| class | 名前の先頭から最後のメソッドの `result.span.end` |
| instance | class 名の先頭から最後のメソッドの `body.span.end`（メソッドがなければ `ty.span.end`）。instance のメソッドはメソッド名の先頭から `body.span.end` |
| test・bench | `test.span`・`bench.span` |
| `$entry` | 関数の `span` |

`uses_type` の `from` は、型名の出現を含む `extent` のうち**最も短いもの**。同じ長さなら ID の byte 順で最初。

### C2: 最終の関数 → 根（A2）

| 最終の関数 `f` | 根 |
| --- | --- |
| `f.module` が `$lambda`・`$task` | `f.origin.parent`（持ち上げ元の関数）を `f` として、もう一度この表を引く |
| `f.module` が `$` で始まる（`$export`・`$builtin`・`$case`・`$intrinsic`） | なし（事実の対象外。効果の伝播では辺として通す） |
| `f.origin.module == Std` | なし（同上） |
| それ以外 | `f.name` から `.$mono.` 以降を除いた名前 `n` で `function_ids[(f.module, n)]` を引く。なければなし |

## 付録 D: `graph` の表

### D1: `calls` の `to`（`Call` の callee）

| callee の `kind` | `to` |
| --- | --- |
| `Function(User(id))`・`GenericFunction(id, _)` | user: `function_ids` の ID。std: `<module>.<name>`。ID のない生成関数は辺を出さない |
| `Function(Builtin(instance))` | `instance.builtin.name()`（`sqrt`・`Math.sqrt`・`assert`） |
| `Method(class, method, _)` | `<class の ID>.<method>`（class の ID は組み込みなら名前、user なら key から `module_display`＋`.`＋名前） |
| `TypeFunction`・`Local`・`Field`・`Call`・そのほか（関数値） | 辺を出さない |

`span` は `callee.span`。`Call` でない位置の `Function(User)`・`GenericFunction`・`Method`（関数を値として渡す）も `calls` を 1 行（`args: []`）。同じ `(from, to, span)` は 1 行。

### D2: 実引数の `conv`

| 実引数の式 | `conv` |
| --- | --- |
| `Borrow(_, false)`・`BorrowOperand(_)` | `ref` |
| `Borrow(_, true)` | `ref_mut` |
| 上以外で `argument.ty.is_copy(&types)`（抽象本体では型変数は Copy でない） | `copy` |
| それ以外 | `own` |

例: `sqrt (point.x * point.x + ..)` → `["copy"]`、`add_to (ref mut total) 1` → `["ref_mut","copy"]`、`consume text`（`string`）→ `["own"]`、`total (ref values)` → `["ref"]`、`peek text`（`text: ref string`。checker が `Borrow(Dereference(text))` にする）→ `["ref"]`。

### D3: `tested_by`

本文の A3。到達の根は `CheckedModule.tests[i].function`（抽象の関数 id）。`references[f]` を辿る。`from` にする関数は、`function_ids` の逆引きで kind が `def`・`method`・`extern` のもの。

### D4: `uses_type`

`Capture.semantic` の各 `Reference`（`role` が `Read`）について、`d = definitions[r.definition]` が、record・union・class・`extern type`（`SymbolKind::Record` で、`detail` が `extern type ` で始まる）のとき `to` は `d` の ID（`module_id(d.module の key)` ＋ `.` ＋ `d.name`）。`from` は C1 の規則。`from == to` は出さない。型エイリアスの使用は索引にないので出ない。

### D5: `implements`・`instance_of`・`imports`

- `instance_of`: user の明示的な instance（`derived == false`）ごとに 1 行。`from` は instance の ID、`to` は class の ID、`span` は class 名。
- `implements`: 同じ instance ごとに 1 行（`from` は head の `type_id_text`）。加えて `RecordDecl.derives`・`UnionDecl.derives` の各 `(DeriveClass, Span)` に 1 行（`from` は record・union の ID、`to` は `DeriveClass::name()`、`span` はその class 名）。
- `imports`: `Program.usings` の各 `NamespaceDecl` に 1 行（`from` はモジュールの ID、`to` は `path.text`、`span` は宣言の `span`）。

## 付録 E: `TypedExprKind` の分類と trap の表

### E1: 全 64 variant（`match` に wildcard を置かない）

走査は `TypedExpr::children()` を明示のスタックで辿る。次の 23 variant だけが事実を出し、残りの 41 は走査するだけ（`match` には名前を並べ、新しい variant を足すと compile error になるようにする）。

| variant | loops | trap | alloc（Level C） | graph | effect（Level B） |
| --- | --- | --- | --- | --- | --- |
| `Function` | - | - | - | `User`: `calls`（`args: []`。callee なら `Call` の分） | `Builtin`: 付録 F |
| `GenericFunction` | - | - | - | `Function(User)` と同じ | - |
| `Method` | - | - | - | class のメソッドへ `calls` | - |
| `TypeFunction` | - | - | - | 辺なし | - |
| `Binary` | - | 整数の `Divide`・`Remainder`（E3） | - | - | - |
| `Call` | - | `assert`・`unreachable`（E3） | - | `calls`（D1・D2） | - |
| `HostCall` | - | - | - | - | `extern` |
| `Lambda` | - | - | - | 本体を走査（抽象本体だけに現れる） | - |
| `Closure` | - | - | - | - | 参照先の関数 id へ辺 |
| `TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel` | - | - | - | - | `task` |
| `If` | - | then の前後で `enter_condition`・`leave_condition` | - | - | - |
| `While` | `while` | - | - | - | - |
| `ForRange` | `for_range` | `range_step_zero`（E3） | - | - | - |
| `ForEach` | `for_each` | - | - | - | - |
| `NewArray`・`NewList`・`NewLiteral` | - | - | `new_array`・`new_list`・`new_literal` | - | - |
| `Index`・`Slice` | - | `bounds_check`（E3） | - | - | - |
| `Checked` | - | `overflow` | - | - | - |

走査するだけの 41 variant: `Error`・`Int`・`Float`・`String`・`Bool`・`Unit`・`Break`・`Continue`・`Local`・`Static`・`GenericInteger`・`GenericFloat`・`Unary`・`StructuralCompare`・`StructuralHash`・`StructuralDisplay`・`Interpolated`・`Match`・`Block`・`Record`・`RecordUpdate`・`Array`・`List`・`Tuple`・`ListTail`・`Field`・`Length`・`StringLength`・`Borrow`・`BorrowOperand`・`Dereference`・`Assign`・`Cast`・`CaseConstructor`・`Construct`・`UnionTag`・`UnionPayload`・`Try`・`RaisedException`・`Reraise`・`DynDispatch`。

### E2: 評価順

`Block` は束縛の式を順に、そのあと `result`。`If` は条件、then、else。`Call` は callee、引数を左から。`Match` は `value`、各腕の各 alternative の `steps` の式と `bindings` の式、ガード、本体。`ForRange` は `start`・`step`・`finish`、本体。`children()` の順に従えばよい（`llvm.rs` の評価順と同じ）。

### E3: `trap_sites`

| ノード | 条件 | `kind` | `proven` |
| --- | --- | --- | --- |
| `Call(Function(Builtin(Assert)), _)` | いつも | `assert` | `false` |
| `Call(Function(Builtin(Unreachable)), _)` | いつも | `unreachable`（`TrapKind` にない名前。D-53 の `trap` の範囲） | `false` |
| `Binary(Divide \| Remainder, l, r)` | `l.ty` が整数 | `integer_division_by_zero` | `r` が `Int(n)` で `n != 0` |
| 同上 | `l.ty` が符号つき整数（`Type::Integer(bits, true)`） | `integer_division_overflow` | `r` が `Int(n)` で `n` が `bits` 幅の全ビット 1（`-1`）でない |
| `Index(array, index)` | いつも（`array.ty` が `Array`・`List`・`Vec`・`FixedArray`・`String`・`Utf8String`） | `bounds_check` | `facts.index_in_bounds(array, index)`（`Array` 以外は `false`） |
| `Slice { .. }` | いつも | `bounds_check` | `false` |
| `ForRange { step, .. }` | いつも | `range_step_zero` | `step` が `Int(n)` で `n != 0` |
| `Checked(inner)` | いつも | `overflow` | `false` |
| 上以外 | | なし | |

`Match` の各腕では、`alternatives` の `steps`（`PatternStep::expression()`）と `bindings` の式が「パターンの射影」で、その中の `Index`・`ListTail` は site を出さない（`control.rs` の `PatternKind::Array`・`List`・`Cons` が、射影の前に `PatternStep::Test` で長さを検査する。IR には検査が出るが、起きない）。`guard` と `body` は通常どおり。

出さないもの（D9）: 資源のトラップ（`allocation_size`・`allocation_failure`・`string_concat_overflow`・`stack_overflow`）、`match_failure`（網羅性の検査で到達しない）・`pattern_mismatch`（`ListTail` の保護。直前の長さの検査 `PatternStep::Test` が先に通る。`control.rs` の `PatternKind::Cons`）、浮動小数点の演算、`NewArray` の長さ、std の関数の中のトラップ（その関数の facts に出る）。

## 付録 F: 効果（Level B の写像と Level A の写し方）

### F1: Level B の写像（A20 が無いとき）

`Builtin` の 246 項目を `match` に全部名前で並べ、wildcard を置かない（`src/kb/effects.rs`）。246 項目のうち 89 個が下の効果を持ち、残りの 157 個（A19 が純粋とした 156 個と、D-53 に対応する効果がない `DynOf`）は効果なし。分類は A19 の表（不純 90 個。すべて HEAD の `Builtin` に在ることを確かめた）に従い、効果の名前だけを D-53 の語彙へ写した。

| 効果 | `Builtin`（variant 名） |
| --- | --- |
| `debug` | `DebugPrintString` |
| `io` | `IOReadLine`・`IOWrite`・`AsyncResume`・`AsyncTake`・`AsyncPut`・`AsyncNextId`・`AsyncClock`・`AsyncWait`・`AsyncPosted`・`AsyncRetire` |
| `os` | `OsRead`・`OsArgs`・`OsWrite`・`OsRandom`・`OsClock`・`OsSleep`・`OsOpen`・`OsHandle`・`OsClose`・`OsSpawn`・`BenchNow`・`BenchConsume`・`GenSeed` |
| `net` | `NetResolve`・`NetOpen`・`NetAccept`・`NetRead`・`NetWrite`・`NetClose`・`NetClassify`・`NetWatch`・`NetUnwatch`・`NetConnect`・`NetNames`・`NetSend` |
| `task` | `TaskRun`・`TaskParallel`・`TaskParallelResults`・`TaskScope`・`ParallelInit`・`ParallelMap`・`ParallelMapRef`・`ParallelReduce`・`ParallelForEachChunk` |
| `atomic` | `AtomicCreate`・`AtomicLoad`・`AtomicStore`・`AtomicSwap`・`AtomicCompareExchange`・`AtomicFetchAdd`・`AtomicFetchSub`・`AtomicFetchAnd`・`AtomicFetchOr`・`AtomicFetchXor`・`AtomicIntoInner`・`MutexCreate`・`MutexWith`・`MutexIntoInner`・`ChannelBounded`・`ChannelSend`・`ChannelRecv`・`ChannelCloneSender`・`ChannelCloseSender`・`ChannelCloseReceiver`・`ArenaNextId` |
| `rc` | `RcNew`・`RcShare`・`RcGet`・`RcStrongCount`・`RcWeakCount`・`RcTryUnwrap`・`RcDowngrade`・`RcUpgrade`・`RcPtrEq`・`ArcNew`・`ArcShare`・`ArcGet`・`ArcStrongCount`・`ArcWeakCount`・`ArcTryUnwrap`・`ArcDowngrade`・`ArcUpgrade`・`ArcPtrEq` |
| `gpu` | `GpuOpen`・`GpuFeatures`・`GpuRun`・`GpuSelect`・`GpuLast` |

個数: 1 ＋ 10 ＋ 13 ＋ 12 ＋ 9 ＋ 21 ＋ 18 ＋ 5 ＝ 89。`DynOf` を足して A19 の 90 個。そのほかの規則: `HostCall` → `extern`。式の型 `ty` が `has_user_drop(&types)`（record・union。`Array`・`List`・`Vec`・`FixedArray`・`Tuple` は成分を再帰）→ `user_drop`。`TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel` の式 → `task`。`Function(Builtin)` は呼ばずに参照しただけでも効果（A19 と同じ）。関数値の呼び出しは辺にしない。`alloc`・`trap`・`copy`・`diverge` は Level B では `effects` に出さない（`effects_complete: false` の理由）。

### F2: Level A（A20 の `rows` と `sites` の写し方）

1. 呼び方: `let fe = effects::summarize(&module, Options { scope: Scope::All, computed: EffectSet::ALL, sites: true })`（`module` は最終の `CheckedModule`。`Err` なら F1 の Level B）。`let rows = fe.rows(&module)`。`EffectRow.id` は `module.functions` の添字。
2. 根への集約: 行の関数を付録 C の表 C2 で根にする（根がない行は捨てる）。根ごとに、`effects` は各行の `effects` の和集合（辞書順）。`effect_sites` は効果ごとに、その効果の `EffectSite` のうち `(span の source, start, end)` が最小のもの（同じなら `via` が短いもの）。`via` の関数 id は C0 の `function_ids` の逆引きで ID にし、ID のない関数は鎖から外す。`effects_complete` は根の全行の `complete` の論理積。行のない根（実体にならなかったジェネリック）は `effects: []`・`effect_sites: []`・`effects_complete` は `fe.computed == EffectSet::ALL`。
3. `unknown`（`dyn`・`unresolved`・`value_call`）は `tz-facts/1` に出さない。未知の呼び出しを通る効果が `effects` に入らないことを `kb-format.md` の限界に書く（キーの追加は互換）。
4. `alloc_sites`: `fe.computed` に `alloc` があるとき、根の各関数の `fe.sites(f)` のうち `effect == Alloc` かつ `exempt == false` を `{span, kind}` にする（`kind` は `why` の `-` を `_` にしたもの）。`(span, kind)` で重複を除き、`alloc_complete = true`。`alloc` がなければ Level C（`NewArray`・`NewList`・`NewLiteral`）。
5. `trap_sites`・`copy_sites` は Level に依らず、本チケットの走査（付録 E の E3）と `copies::sites`。

## 付録 G: 所有権の事象とレイアウトの表

### G1: 所有権の sink（`src/ownership.rs`）

型: `pub(crate) struct OwnershipEvent { pub var: String, pub kind: EventKind, pub to: Option<EventTarget>, pub span: Span }`、`pub(crate) enum EventKind { Move, Borrow, BorrowMut, Reborrow, Drop }`（出力の `event` は `snake_case`）、`pub(crate) enum EventTarget { User(usize), Builtin(Builtin), Method(usize, usize) }`。`Checker` に `events: Option<&'run mut Vec<OwnershipEvent>>` を足す（`copies` の隣）。

| # | 場所 | 条件 | 事象 | `span` |
| --- | --- | --- | --- | --- |
| 1 | `read_places` の `if moving {`（`self.state.moved.insert(place.clone());` の直後） | 非 Copy の消費。型が `copy_variables` の型変数なら出さない | `move` | `expression.span` |
| 2 | `eval_value` の `E::Borrow(value, mutable)`、各 `(place, via)` の `result.loans.insert(self.loan(..))` の直後 | `via` が空 | `borrow`（`mutable` なら `borrow_mut`） | `expression.span` |
| 2' | 同じ場所 | `via` が空でない | `reborrow`（共有・排他とも） | 同上 |
| 3 | `E::Slice`（共有）の `for (place, via)` | `via` が空／空でない | `borrow`／`reborrow` | `expression.span` |
| 4 | `exclusive_slice` の `loan(.., true, ..)` の直後 | `via` が空／空でない | `borrow_mut`／`reborrow` | 引数の `span` |
| 5 | `eval_block` の `for id in ids { self.state.locals.remove(&id); }` の直前。`check_body` の `checker.eval(body, ..)` の直後（引数） | ローカルの型が `needs_drop` で、`self.state.moved` に `place.root == id` がなく、宣言が user（`provenance == User`） | `drop` | `Block` の式の終わり（`start == end == expression.span.end`）。引数は本体の終わり |

`var`: `place.root` が引数の外部の root（`usize::MAX - id`）なら `self.declarations[&(usize::MAX - root)].name`、ローカルなら `self.declarations[&root].name`。静的データの root（`is_static_root`）は出さない。`to`: `eval_call` が、引数ごとの `self.eval(argument, ..)` の前後で `self.events.len()` を控え、`span == argument.span` の事象に callee を付ける（`Function(User(id))`・`GenericFunction(id, _)` → `User(id)`、`Function(Builtin(i))` → `Builtin(i.builtin)`、`Method(c, m, _)` → `Method(c, m)`）。出力の `to` は D1 と同じ ID。事象は `(span.start, span.end, 種類の順 move・borrow・borrow_mut・reborrow・drop・copy)` で整列し、同じ事象を除く。

### G2: `layout` の行

| 項目 | 取り方 |
| --- | --- |
| `id`・`type` | `type_id_text`・`Type::display` |
| `size`・`align` | `layout::layout_bound(&ty, module, true)`（下限）・`(…, false)`（上限）の `(size, align)` から `[min, max]` |
| record の `fields` | `module.types().record_fields(id, args)` と `records[id].fields` の名前を宣言順に zip。`slot` は `layout::record_slots(id, args, module)` が `Some(s)` なら `s[i]`、`None` なら `i` |
| `reordered` | `record_slots` が `Some` |
| union の `layout` | `UnionLayout::Enum`→`enum`、`Common`→`common`、`General`→`general`、`Niche`→`niche`。`module.types().recursive(&ty)` なら `node`（先に判定） |
| `tag_bytes` | `layout::tag_width(&ty, module)`。`niche` は `null` |
| `niche` | `UnionLayout::Niche { niche, .. }` の `Niche::NegativeLength`→`negative_length`、`NullPointer`→`null_pointer`。それ以外は `null` |
| `cases` | `module.types().union_payloads(id, args)` と `unions[id].cases` の名前。payload は `Type::display` か `null` |
| `drop_flag` | `layout::drop_flag(&ty, module).is_some()` |

## 付録 H: 想定出力（手計算）

どれも**想定出力**で、実装前に作った。`H1`・`H2` は golden にそのまま使う。`line`・`column` と、`kb_basic` の byte offset は Python で数えた。

### H1: `examples/point`

`examples/point` は `Tsuzuri.toml` がなく、モジュールは `Main`（トップレベルのコード）と `Point`（`record Point`・`distance`・`export def hypotenuse`）。`tests.jsonl` は 0 byte。

`tests/golden/kb/point/symbols.jsonl`（8 行）:

```jsonl
{"id":"Main","kind":"module","module":"Main","name":"Main","signature":"module Main","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":0,"end":0,"line":1,"column":1}}
{"id":"Main.$entry","kind":"def","module":"Main","name":"$entry","signature":"entry :: f64","params":[],"result":"f64","public":false,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":0,"end":61,"line":1,"column":1}}
{"id":"Point","kind":"module","module":"Point","name":"Point","signature":"module Point","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Point.tz","start":0,"end":0,"line":1,"column":1}}
{"id":"Point.Point","kind":"record","module":"Point","name":"Point","signature":"record Point","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Point.tz","start":7,"end":12,"line":1,"column":8}}
{"id":"Point.Point.x","kind":"field","module":"Point","name":"x","signature":"x: f64","params":[],"result":"f64","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Point.tz","start":15,"end":16,"line":1,"column":16}}
{"id":"Point.Point.y","kind":"field","module":"Point","name":"y","signature":"y: f64","params":[],"result":"f64","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Point.tz","start":23,"end":24,"line":1,"column":24}}
{"id":"Point.distance","kind":"def","module":"Point","name":"distance","signature":"def distance :: Point -> f64","params":[{"name":"point","type":"Point","conv":"copy"}],"result":"f64","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Point.tz","start":37,"end":45,"line":3,"column":5}}
{"id":"Point.hypotenuse","kind":"def","module":"Point","name":"hypotenuse","signature":"export def hypotenuse :: f64 -> f64 -> f64","params":[{"name":"x","type":"f64","conv":"copy"},{"name":"y","type":"f64","conv":"copy"}],"result":"f64","public":true,"export":true,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Point.tz","start":135,"end":145,"line":6,"column":12}}
```

`tests/golden/kb/point/graph.jsonl`（6 行。`uses_type` の 3 行は LSP の `references` で確かめた位置。`calls` の `span` は callee の式の全体で、`Main.tz` は修飾名 `Point.distance` 全体）:

```jsonl
{"rel":"calls","from":"Main.$entry","to":"Point.distance","args":["copy"],"span":{"path":"Main.tz","start":43,"end":57,"line":2,"column":9}}
{"rel":"calls","from":"Point.distance","to":"sqrt","args":["copy"],"span":{"path":"Point.tz","start":78,"end":82,"line":4,"column":5}}
{"rel":"calls","from":"Point.hypotenuse","to":"Point.distance","args":["copy"],"span":{"path":"Point.tz","start":181,"end":189,"line":7,"column":5}}
{"rel":"uses_type","from":"Main.$entry","to":"Point.Point","span":{"path":"Main.tz","start":8,"end":13,"line":1,"column":9}}
{"rel":"uses_type","from":"Point.distance","to":"Point.Point","span":{"path":"Point.tz","start":49,"end":54,"line":3,"column":17}}
{"rel":"uses_type","from":"Point.hypotenuse","to":"Point.Point","span":{"path":"Point.tz","start":191,"end":196,"line":7,"column":15}}
```

`facts`（Phase 3〜6 を全部出荷した build、A20 なし）: 両モジュールの関数は、`effects: []`・`effect_sites: []`・`effects_complete: false`・`copy_sites: []`・`alloc_sites: []`・`alloc_complete: false`・`trap_sites: []`・`ownership: []`・`recursion: "none"`・`loops: []`・`specializations: 0`・`truncated: {}`。`layout/Point.jsonl` は 1 行（`Point.Point`: `size [16,16]`・`align [8,8]`・`fields` は `x` が slot 0・`y` が slot 1・`reordered: false`）。

### H2: `examples/polymorphism`

`Classes.tt`（class `Score`・`Size`）と `Main.tz`（record `Point`、generic な `add`、`instance Add<Point>`・`Eq<Point>`・`Classes.Score<Point>`、`def score`、トップレベルのコード）。`tests/golden/kb/polymorphism/symbols.jsonl`（19 行）:

```jsonl
{"id":"Classes","kind":"module","module":"Classes","name":"Classes","signature":"module Classes","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Classes.tt","start":0,"end":0,"line":1,"column":1}}
{"id":"Classes.Score","kind":"class","module":"Classes","name":"Score","signature":"class Score<'a>","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Classes.tt","start":6,"end":11,"line":1,"column":7}}
{"id":"Classes.Score.score","kind":"method","module":"Classes","name":"score","signature":"def score :: ref 'a -> i32","params":[{"name":"arg1","type":"ref 'a","conv":"ref"}],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Classes.tt","start":26,"end":31,"line":2,"column":9}}
{"id":"Classes.Size","kind":"class","module":"Classes","name":"Size","signature":"class Size<'a>","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Classes.tt","start":58,"end":62,"line":5,"column":7}}
{"id":"Classes.Size.size","kind":"method","module":"Classes","name":"size","signature":"def size :: ref 'a -> i64","params":[{"name":"arg1","type":"ref 'a","conv":"ref"}],"result":"i64","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Classes.tt","start":77,"end":81,"line":6,"column":9}}
{"id":"Main","kind":"module","module":"Main","name":"Main","signature":"module Main","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":0,"end":0,"line":1,"column":1}}
{"id":"Main.$entry","kind":"def","module":"Main","name":"$entry","signature":"entry :: i32","params":[],"result":"i32","public":false,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":466,"end":569,"line":20,"column":1}}
{"id":"Main.Point","kind":"record","module":"Main","name":"Point","signature":"record Point","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":7,"end":12,"line":1,"column":8}}
{"id":"Main.Point.x","kind":"field","module":"Main","name":"x","signature":"x: i32","params":[],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":15,"end":16,"line":1,"column":16}}
{"id":"Main.Point.y","kind":"field","module":"Main","name":"y","signature":"y: i32","params":[],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":23,"end":24,"line":1,"column":24}}
{"id":"Main.add","kind":"def","module":"Main","name":"add","signature":"def add :: 'a -> 'a -> 'a","params":[{"name":"x","type":"'a","conv":"own"},{"name":"y","type":"'a","conv":"own"}],"result":"'a","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":37,"end":40,"line":3,"column":5}}
{"id":"Main.score","kind":"def","module":"Main","name":"score","signature":"def score :: Classes.Score<'a> => ref 'a -> i32","params":[{"name":"value","type":"ref 'a","conv":"ref"}],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":383,"end":388,"line":18,"column":5}}
{"id":"instance:Add<Main.Point>","kind":"instance","module":"Main","name":"Add<Point>","signature":"instance Add<Point>","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":85,"end":88,"line":5,"column":10}}
{"id":"instance:Add<Main.Point>.add","kind":"method","module":"Main","name":"add","signature":"def add :: Main.Point -> Main.Point -> Main.Point","params":[{"name":"left","type":"Main.Point","conv":"copy"},{"name":"right","type":"Main.Point","conv":"copy"}],"result":"Main.Point","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":105,"end":108,"line":6,"column":8}}
{"id":"instance:Classes.Score<Main.Point>","kind":"instance","module":"Main","name":"Classes.Score<Point>","signature":"instance Classes.Score<Point>","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":314,"end":327,"line":14,"column":10}}
{"id":"instance:Classes.Score<Main.Point>.score","kind":"method","module":"Main","name":"score","signature":"def score :: ref Main.Point -> i32","params":[{"name":"point","type":"ref Main.Point","conv":"ref"}],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":344,"end":349,"line":15,"column":8}}
{"id":"instance:Eq<Main.Point>","kind":"instance","module":"Main","name":"Eq<Point>","signature":"instance Eq<Point>","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":185,"end":187,"line":9,"column":10}}
{"id":"instance:Eq<Main.Point>.eq","kind":"method","module":"Main","name":"eq","signature":"def eq :: ref Main.Point -> ref Main.Point -> bool","params":[{"name":"left","type":"ref Main.Point","conv":"ref"},{"name":"right","type":"ref Main.Point","conv":"ref"}],"result":"bool","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":204,"end":206,"line":10,"column":8}}
{"id":"instance:Eq<Main.Point>.ne","kind":"method","module":"Main","name":"ne","signature":"def ne :: ref Main.Point -> ref Main.Point -> bool","params":[{"name":"left","type":"ref Main.Point","conv":"ref"},{"name":"right","type":"ref Main.Point","conv":"ref"}],"result":"bool","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":266,"end":268,"line":11,"column":8}}
```

`Eq`・`Add`・`Classes.Score` は instance の class で、`implements` は `Main.Point → Add`・`Main.Point → Eq`・`Main.Point → Classes.Score`、`instance_of` は 3 つの instance の ID から同じ 3 つの class（graph の golden は置かず、`kb_basic_graph_has_every_rel` の件数で確かめる）。

### H3: `kb_basic` の件数

| ファイル | 件数 |
| --- | --- |
| `symbols.jsonl` | 47 行: `module` 4・`def` 23（Metrics 1・Main 18・Shapes 4）・`const` 1・`record` 2・`field` 5・`union` 1・`case` 2・`class` 1・`method` 2（class の `size`、instance の `size`）・`instance` 1・`alias` 1・`test` 1・`bench` 1・`extern` 1・`handle` 1 |
| `tests.jsonl` | 2 行: `bench:Acme::Kb::Main.total of three`（index 0）・`test:Acme::Kb::Main.total adds`（index 0） |
| `graph.jsonl` | `calls` 24（`from` 別: `norm1` 2・`sum_to` 1・`fib` 2・`is_even` 1・`is_odd` 1・`describe` 2・`doubled` 2・`filled` 1・`copies` 3・`peek_twice` 2・`ownership` 3・`traced` 1・test 2・bench 1）、`implements` 2（`Acme::Kb::Shapes.Point` → `Eq`（deriving）、→ `Acme::Kb::Traits.Sized`）、`instance_of` 1、`imports` 1（`Acme::Kb::Main` → `Acme::Kb::Geometry`）、`tested_by` 1（`Acme::Kb::Main.total` → `test:Acme::Kb::Main.total adds`） |
| `facts/*.jsonl` | `Geometry.Metrics.jsonl` 1 行・`Main.jsonl` 20 行（`def` 18・test 1・bench 1）・`Shapes.jsonl` 6 行（`def` 4・`extern` 1・instance の `method` 1）。`Traits.jsonl` はない（class のメソッドは本体がない） |
| `layout/Shapes.jsonl` | 3 行: `Packed`・`Point`・`Shape`（H6） |

`calls` の `args` の例: `norm1` の `abs` は `["copy"]`、`describe` の `Shapes.area` は `["ref"]`・`Metrics.norm1` は `["copy","copy"]`、`ownership` の `add_to` は `["ref_mut","copy"]`・`peek_twice` は `["ref"]`・`consume` は `["own"]`、`copies` の `total` は 3 つとも `["ref"]`。

### H4: `facts`（Phase 3〜6 を全部出荷した build、A20 なし）

`Main.total`（`id` は `Acme::Kb::Main.total`）の行の全体（改行を除いて 1 行）:

```json
{"id":"Acme::Kb::Main.total","effects":[],"effect_sites":[],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[{"span":{"path":"Main.tz","start":164,"end":177,"line":7,"column":21},"kind":"bounds_check","proven":true}],"ownership":[],"recursion":"none","loops":[{"kind":"for_range","span":{"path":"Main.tz","start":105,"end":177,"line":6,"column":5}}],"specializations":0,"truncated":{}}
```

`Main.traced`（`id` は `Acme::Kb::Main.traced`）: `effects` が `["debug"]`、`effect_sites` が `[{"effect":"debug","span":{"path":"Main.tz","start":2065,"end":2076,"line":81,"column":5},"via":["Debug.print"]}]`、`ownership` が `[{"var":"n","event":"borrow","to":"Debug.print","span":{"path":"Main.tz","start":2078,"end":2083,"line":81,"column":18}}]`、ほかは空（A20 の Level A では `Debug.print` の表示で `alloc` が加わりうる）。

関数ごとの期待値（`line:column`。span の `start`・`end` は式の範囲で、テストは `kind`・`line`・`column` で照合する）:

| 関数（`Acme::Kb::` を省く） | 期待 |
| --- | --- |
| `Main.at` | `trap_sites`: `bounds_check`・`proven: false`・11:22 |
| `Main.ratio` | `integer_division_by_zero`・`integer_division_overflow`（ともに `proven: false`・14:23） |
| `Main.sum_to` | `recursion: "self_tail"`。`trap_sites: []` |
| `Main.fib` | `recursion: "self"` |
| `Main.is_even`・`Main.is_odd` | `recursion: "mutual"` |
| `Main.countdown` | `loops`: `while`・34:5 |
| `Main.filled` | `alloc_sites`: `new_array`・47:18 |
| `Main.copies` | `copy_sites`: `{"type":"[i64]","kind":"local","cost":"length"}`・54:17（`tsuzuri check --warn implicit-copy` の W1006 と同じ位置） |
| `Shapes.twice` | `specializations: 2`（`f64` と `i64`）。`ownership: []` |
| `Shapes.area` | 全部空。`recursion: "none"` |
| test `total adds` | `trap_sites`: `assert`・84:21。`recursion: "none"` |

### H5: `ownership`（Phase 5。想定）

`Acme::Kb::Main.ownership`（`path` は `Main.tz`）:

| `var` | `event` | `to` | 位置（line:column） |
| --- | --- | --- | --- |
| `total` | `borrow_mut` | `Acme::Kb::Main.add_to` | 72:13（`ref mut total`） |
| `label` | `borrow` | `Acme::Kb::Main.peek_twice` | 74:28（`ref label`） |
| `text` | `move` | `Acme::Kb::Main.consume` | 76:25 |
| `label` | `drop` | （なし） | 77:25（関数本体の終わり。`start == end`） |

`text` は `consume` に移動したので `drop` は出ない。`total`・`seen`・`owned`・`n` は Copy でドロップが要らない。`Acme::Kb::Main.peek_twice` は `reborrow` が 2 件（`to` は `Acme::Kb::Main.peek`、64:27 と 64:39。`text: ref string` を渡すと checker が `Borrow(Dereference(text))` にするため）。`Acme::Kb::Main.copies` の `copy` は `copy_sites` と同じ 54:17（`ownership` にも出す）。

### H6: `layout/Shapes.jsonl`（Phase 6）

```jsonl
{"id":"Acme::Kb::Shapes.Packed","kind":"record","type":"Shapes.Packed","size":[16,16],"align":[8,8],"fields":[{"name":"a","type":"i8","slot":1},{"name":"n","type":"i64","slot":0},{"name":"b","type":"i8","slot":2}],"reordered":true,"drop_flag":false}
{"id":"Acme::Kb::Shapes.Point","kind":"record","type":"Shapes.Point","size":[16,16],"align":[8,8],"fields":[{"name":"x","type":"f64","slot":0},{"name":"y","type":"f64","slot":1}],"reordered":false,"drop_flag":false}
{"id":"Acme::Kb::Shapes.Shape","kind":"union","type":"Shapes.Shape","size":[16,16],"align":[8,8],"layout":"common","tag_bytes":1,"niche":null,"cases":[{"name":"Circle","payload":"f64"},{"name":"Empty","payload":null}],"drop_flag":false}
```

根拠: IR は `%tz.record.Shapes.Packed = type { i64, i8, i8 }`（宣言は `a: i8, n: i64, b: i8`。`n` が先頭に動くので `a` の slot は 1、`n` は 0、`b` は 2。宣言順の大きさは 24、並べ替えると 16）、`%"tz.union.Shapes.Shape" = type { i8, double }`（タグ 1 byte＋ `f64`）。

## 付録 J: 実測と測定の手順

### J1: HEAD での実測（本チケットの根拠）

| 確認 | コマンド | 結果 |
| --- | --- | --- |
| 完全名 | `node target/tz-G26/lsp-hover.mjs target/tz-G26/p1 Geometry/Point.tz "norm ::"`（`textDocument/hover`） | `def Acme::Shapes::Geometry::Point.norm :: Geometry::Point -> f64` |
| 重複した test 名 | `tsuzuri test target/tz-G26/dup --list` | `0 Main.same` と `1 Main.same` |
| `$entry` のスパン | `examples/point` の先頭に `def main :: unit -> i32 = \() -> 0` を足して `check --json` | `E2004`・`"start":36,"end":97`（足した 36 byte を引くと 0..61） |
| 暗黙の複製 | `tsuzuri check target/tz-G26/kb_basic --warn implicit-copy` | `Main.tz:54:17: warning[W1006]`（1 件） |
| 境界検査・除算・末尾再帰・レイアウト | `tsuzuri build target/tz-G26/kb_basic --emit llvm -O0` | `total` に検査なし、`at` に `icmp ult`＋`llvm.trap`、`ratio` にゼロ・オーバーフローの検査、`sum_to` に自己呼び出しなし（`br label %loop` 2 つ）、`fib` に自己呼び出し 2、`Packed = { i64, i8, i8 }` |
| 名前の使用 | LSP `textDocument/references`（`examples/point` の `Point`） | `Main.tz` 8..13、`Point.tz` の宣言・3 行 17 列・7 行 15 列 |
| 時間 | `TSUZURI_TIME_PASSES=1 tsuzuri check examples/functional` | `load` 3.4・`cache` 24.8・`parse` 4.0・`check` 17.9・`specialize` 3.7・`closures` 0.04・`ownership` 0.1 ms、合計 55.0 ms |
| 型・分類の件数 | `check.rs` の enum の variant を数える | `TypedExprKind` 64、`Builtin` 246（`Builtin::ALL`）、`Type` 30 |
| A19 の不純な組み込み関数 | A19 のチケットの 90 個の名前を `Builtin` の variant と照合 | 90 個すべて在る |

`target/tz-G26/` の入力（`p1`・`dup`・`pt`・`lsp-hover.mjs`）は確認の後に消した。`kb_basic` は付録 A。`p1` は本文「現状」の完全名の入力（`namespace = "Acme::Shapes"` と `Geometry/Point.tz`）、`dup` は同名の `test "same"` を 2 つ書いた `Main.tz`、`pt` は `examples/point` の写しの先頭に `def main` を足したもの、`lsp-hover.mjs` は `tsuzuri lsp` に `initialize`・`textDocument/hover` を送る小さな Node スクリプト（必要なら書き直す。コマンドは再現の目安で、結果の数字が根拠）。

### J2: KB の費用の測り方（手順 19）

`tests/kb.rs` の `#[ignore]` の `dump_kb_for_shipped_projects` が、`tests/fixtures`・`examples`・`benchmarks` の各プロジェクトについて、`Project::analyze_all()` を 5 回（中央値＝`check_ms`）と `kb::collect` を 5 回（中央値＝`collect_ms`）測り、`kb_ms = collect_ms - check_ms`、`ratio = kb_ms / check_ms` を表示する（`cargo test --locked --release --test kb -- --ignored --nocapture dump`）。目標は `ratio <= 0.30` だが、閾値は置かず、結果（最大・中央値・`check` が最も遅い 5 プロジェクト）をチケットの「実装と検証」に記録する。
