# A21 付録: 実測・`TrapKind` の一覧・SMT-LIB の翻訳・`Gen`／`Arbitrary`・診断の例・E2E・Phase 3〜6 の手順

本体: [A21-contracts-requires-ensures.md](A21-contracts-requires-ensures.md)。調査は HEAD `a2313d19` のソースと `target/release/tsuzuri`、Z3 5.1.0 で行った。`/tmp/tz-A21-*` のパスは再現用の一時ディレクトリの例（ソースから作り直す。リポジトリには置かない）。

## 付録 A: 実測ログ

### A.1 現状の拒否

本体「現状 1」。トップレベルの `@requires 1 > 0`（1:1）と、同じ行の `\x -> @requires x > 0 x`（1:29）も同じ `E0002`。

### A.2 契約の形に手で織り込んだ関数（`assert` で代用）

| 実験 | ソース（抜粋） | 結果 |
| --- | --- | --- |
| (a) 配列の引数 | `def first_pos_c :: [i64] -> i64 = \xs ->` ⏎ `assert (xs.length > 0)` ⏎ `let r = Array.sum xs` ⏎ `assert (r >= 0)` ⏎ `r` | 実行 `10`。`--emit llvm -O0` の `first_pos_c` に `@tz.alloc` がなく、`xs` を読む `extractvalue %tz.array %v1, 1` だけ。`Array.sum` の呼び出しは契約なしの版と同じ形 |
| (b) 結果の名前の連鎖 | `let r1: [i64] = Array.sort xs` ⏎ `assert (r1.length == xs.length)` ⏎ `let r2: [i64] = r1` ⏎ `assert (r2.length >= 0)` ⏎ `r2` | `[1, 2, 3] [1, 2, 3]`。IR は `store %tz.array zeroinitializer, ptr %v3`（移動）で複製なし |
| (c) 値渡しの引数を本体が移動 | `let r: i64 = { let t = s; t.length }` ⏎ `assert (r == s.length)` | `error[E1012]: use of moved or partially moved value 's'`（`3:18`） |
| (d) `ref mut` の事後の値 | `def bump :: ref mut [i64..] -> i64 -> unit = \xs v ->` ⏎ `let r: unit = Array.write 0 v xs` ⏎ `assert (xs[0] == v)` ⏎ `r` | `bump (ref mut a) 9` のあと `a` は `[9, 2, 3]` |
| (e) 末尾呼び出し | `def rec plain :: i64 -> i64 -> i64 = \n acc ->` ⏎ `assert (n >= 0)` ⏎ `if n == 0 then acc else plain (n - 1) (acc + n)` | `plain 10000000 0` が `-O0` で `50000005000000`（末尾呼び出しが保たれる） |
| (f) `ensures` に当たる形 | `let r: i64 = if n == 0 then acc else woven (n - 1) (acc + n)` ⏎ `assert (r >= 0)` ⏎ `r` | `-O0`: 10,000／30,000／60,000 段は通り、100,000 段以上は `trap: stack overflow`。`-O3`: 1 億段が通る |

### A.3 使われない関数・特殊化の番号と IR

`private def unused_helper :: i64 -> i64 = \x -> x * 3`（と、それがジェネリックな `unused_generic` を呼ぶ形）を足したプログラムの `--emit llvm -O0` は、足す前と `diff` が空。`W1002` の警告だけが増える。IR は到達可能な関数だけを出す。

**特殊化の番号は契約でずれる**（本体「設計」の `strip` を特殊化の直前に置く理由）。`def total :: [i64] -> i64 = \xs ->` ⏎ `Array.sum xs`（`total [1, 2, 3]` で呼ぶ）の IR の特殊化は `Array.sum.$mono.2` の 1 つ。本体の前に `let ys: [i32] = [1, 2]` ⏎ `assert (Array.sum ys > 0)`（契約の式で別のジェネリックを呼ぶ形）を足すと、`Array.sum.$mono.2`（`[i32]`）と `Array.sum.$mono.3`（`[i64]`）になり、本体の呼び出しの名前が変わる。特殊化の**後**に契約の式を消すと、`$mono.3` が残って IR が契約なしの版と一致しない。

### A.4 トラップの報告

本体「現状 5」。`tsuzuri test` の runner は trap-info を入れないので、`not ok 2 - Main traps` の下は `failure: trapped or terminated by signal` だけ。`E2005` の `--json`: `{"severity":"error","code":"E2005","message":"program terminated with signal: 5 (SIGTRAP):\ntrap: assertion failed at /tmp/tz-A21-e/Main.tz:2:5","path":"/tmp/tz-A21-e/Main.tz","span":{"start":45,"end":59,"line":2,"column":5,"end_line":2,"end_column":19}}`。

### A.5 SMT-LIB と z3

- **差分テスト（本体の停止条件 9 の元）**: 型 `i8`・`i8u`・`i32`・`i64u`、演算 `+ - * / % < >= == &&& ||| ^^^ <<< >>>`（13 個）、値は符号付きが `[最小, 最小+1, -7, -1, 0, 1, 7, 最大-1, 最大]`、符号なしが `[0, 1, 7, 2^(w-1)-1, 2^(w-1), 最大-1, 最大]` の全組（ゼロ除算と `最小 / -1` は除く）。Tsuzuri のプログラム（`let v0: i8 = (-128)` … `Debug.print (ref (to_string (v0 + v1)))`。`tsuzuri run -O0`）を実行した結果 3,312 件（i8 1,033・i8u 623・i32 1,033・i64u 623）を、z3 の `(simplify (bvadd #x80 #xff))` の形（`z3 -in`）の結果と比べて **0 件不一致**。z3 の Python API（`A / B` = `bvsdiv`、`A >> B` = `bvashr`、`A < B` = `bvslt`）でも 0 件不一致。
- 一緒に確かめた値: `i8` の `-128 / -1` と `-128 % -1` は `trap: integer division overflow`、`5 % 0` は `trap: integer division by zero`。`0 - a`・`-a`・`Int.abs a`（`a = -128`）は `-128`、`Int.min a 5` は `-128`、`Int.max a 5` は `5`、`~~~a` は `127`。`(-5: i32) as i64` は `-5`、`as i8` は `-5`、`as i32u` は `4294967291`。`(200: i8u) as i32` は `200`（ゼロ拡張）、`as i8` は `-56`。`3 ** 4` は `81`。シフトの量は `count & (bit_width - 1)`。`-9223372036854775808`（`i64` の最小）は 10 進のリテラルで書ける。
- **証明と反例**（`clamp`）: 台本（付録 C.8）に `ensures` が `lo <= r && r <= hi` なら `z3 -smt2 -t:10000` が `unsat`（0.10 秒）。`r < hi` に壊すと `sat` と `((lo #x8000000000000000) (hi #x6ffffffffffffffe) (x #x7000000004000400))`（0.27 秒）。
- **時間切れ**: `z3 -smt2 -t:20 factor.smt2`（64 bit の乗算で素因数分解）は `unknown` のあと `(error "line 10 column 16: model is not available")`。`-t:100` では `sat` と `((a #x00000000ffffffef) (b #x00000000fffffffb))`。`-T:秒` はプロセス全体の時間切れ。
- `QF_BV` の台本で `define-fun` の名前つきの項、`ite`、`let` は使える。データ型（`declare-datatypes`）も z3 は受理するが、本チケットは使わない（成分に分ける）。

### A.6 性質テスト

```tsuzuri
test "clamp broken postcondition" =
    Gen.for_all (Gen.pair (Gen.i64()) (Gen.pair (Gen.i64()) (Gen.i64()))) (\args ->
        match args with
        | (lo, (hi, x)) ->
            if lo <= hi then
                let r = clamp lo hi x
                r < hi
            else true)
```

`tsuzuri test` の出力: `property failed at case 8 of 100 (seed 11400714819323198485)`・`counterexample: (0, (0, 0))`・`shrunk 1 times from (-9223372036854775808, (-471411656769, -15014))`。同じ形で正しい事後条件（`lo <= r && r <= hi`）は `ok`。複数行の引数のラムダは `(\args ->` を前の引数と**同じ行**に書く（改行して `(` で始めると別の文になって `E1003`）。

## 付録 B: `TrapKind` を 1 つ足すときに触る全箇所

`git grep -n "TrapKind" -- src tests` と `git grep -n "kind" -- src/runtime/trap.c` で確かめた（HEAD）。

| # | ファイル | 場所 | 変更 |
| --- | --- | --- | --- |
| 1 | `src/trap.rs` | `enum TrapKind`（`#[repr(u32)]`）| **末尾**（`Overflow` の後）に 2 つ足す。既存の番号が動かない（`Overflow` = 15、`ContractRequires` = 16、`ContractEnsures` = 17） |
| 2 | `src/trap.rs` | `TrapKind::ALL` | `[Self; 16]` → `[Self; 18]`、末尾に 2 つ |
| 3 | `src/trap.rs` | `TrapKind::description` | `ContractRequires => "contract requires failed"`、`ContractEnsures => "contract ensures failed"`（`.trap.json` の `kind` と `trap:` 行の説明になる） |
| 4 | `src/trap.rs` | `TrapKind::diagnostic_kind`（新規） | `E2005` の kind（契約の 2 つだけ `Some`） |
| 5 | `src/llvm_traps.rs` | `runtime_kind(function: &str)` | `builtin.assert` の分岐の**前**に 2 つ（`function.contains("builtin.$requires")`・`"builtin.$ensures"`）。ビルトインの定義名 `@tz.builtin.$requires` とラッパー `@tz.fn.$builtin.$requires` の両方に当たる |
| 6 | `src/llvm_traps.rs` | `Sites::base` の `TrapKind::ALL.len()`、`instrument` の `urem i32 %offset, {}` | **変更なし**（`ALL.len()` から自動。site の id = `1 + n * 18 + kind`、`--trap-mode return` の `kind = (site - 1) % 18`） |
| 7 | `src/bindings_native.rs` | `trap_kinds()`（C#・Python・C++ の kind の配列） | 変更なし（`ALL` から自動。末尾に 2 つ増える。既存の番号は不変） |
| 8 | `src/runtime/trap.c` | `kind` は不透明な `uint32_t` | 変更なし（kind の一覧を持たない） |
| 9 | `src/runtime/trap-boundary.mjs`・`test-runner.mjs`・`bindings*.mjs` | `.trap.json` の `kind`（説明の文字列）を引く | 変更なし |
| 10 | `src/llvm.rs` | `tsuzuri_trap_info` の C の宣言のコメント | 変更なし（kind の一覧を書いていない） |
| 11 | `tests/trap_locations.rs` | 新しいテスト `contract_traps_report_their_kind_and_the_clause_position` | native の標準エラー・`.trap.json` の `kind`・WASM の `site.kind` |
| 12 | `tests/features.mjs` | suite `contracts` の `traps` の第 3 要素 | kind の文字列 |
| 13 | 文書 | `exception-handling.md` の「契約違反はトラップ」の表に 2 行。`docs/language.md` の「トラップ位置」 | 本体「ドキュメント」 |

トラップを出す側は、組み込み関数 `$requires`・`$ensures` の定義（`emit_builtin`。`Assert` の arm と同じ本体）の `call void @llvm.trap()` が `runtime_kind` で kind を決め、呼び出し側の `append_context` が呼び出し式の `Span`（契約の行）を位置にする（`Assert` と同じ経路）。`guard(&valid, kind)`（`src/llvm.rs`）を使う経路は使わない（`Assert` の手本は関数の定義側で trap する）。

## 付録 C: `prove`（Phase 5）の設計

### C.1 入口・対象・値の表現

- `src/prove.rs`: `pub(crate) struct ProveOptions { pub timeout_seconds: u64, pub functions: Vec<String>, pub solver: Option<String> }`、`pub(crate) fn run(module: &CheckedModule, sources: &[(String, String)], options: &ProveOptions) -> Vec<Report>`（`Report { function: String, span: Span, clauses: Vec<ClauseResult>, diagnostics: Vec<Diagnostic> }`）。`src/prove_smt.rs`: 翻訳器 `Translator` と台本 `Script`（`String` を作るだけ。ソルバーを知らない）。
- **対象**: `module.functions` の `function.contracts != ContractShape::default()` で、`origin.module == ModuleOrigin::User`・`origin.provenance == Provenance::User`・`origin.parent.is_none()`・`origin.test.is_none()` の関数（`prove` は `contracts == Prove` の解析結果＝織り込み済み・特殊化済み・`closures::lower` 済みの `CheckedModule` を読む）。`--function NAME` は `qualified_name()` から末尾の `.$mono.N` を落とした名前に一致するもの。順序は (`module`, `span` の `(source, start)`)。0 件のとき、標準出力は空で終了コード 0。
- **織り込んだ形の読み方**: `body.kind == Block { bindings, result }`。`bindings[..requires]` の各 `Call(_, [条件])` が `@requires`、ensures が 0 なら `*result` が本体、1 以上なら `bindings[requires + olds]` が結果の束縛（`.1` が本体）で、その後ろの `ensures` 個の `Call(_, [条件])` が `@ensures`（条件の中で結果は `Local(result)`）。
- **値**: `enum Value { Unit, Bool(String), Bits(String, u16), Tag(String), Tuple(Vec<Value>), Record(Vec<Value>) }`（`String` は SMT-LIB の項。`Bits` は幅つき。`Tag` は 32 bit）。非自明な項は `define-fun t<N> () <sort> <項>` に束縛し、名前だけを持ち回る（決定的な通し番号）。引数は成分ごとに `declare-const p<i>`（成分は `p<i>_<k>`）。ペイロードなしの union の引数は 32 bit で、「`p<i>` < 場合の数」を前提に足す。
- **トラップは値に混ぜない**。式の翻訳は `tr(expr, path) -> Result<Value, Unsupported>` で、`path`（その式が評価される条件、SMT の `Bool` 項）を受け取り、トラップしうる場所を `obligations: Vec<Obligation { name: "o<k>", cond: String, what: &'static str, span: Span }>` に積む（`cond` = `path` かつ トラップする条件）。`If` は then に `path ∧ c`、else に `path ∧ ¬c`、`&&` の右辺は `path ∧ 左`、`||` の右辺は `path ∧ ¬左`、`Match` の腕は前の腕がすべて外れて自分の条件が成り立つとき。全体の `trap = (or o0 o1 ..)`（なければ `false`）。

### C.2 型の分類（`Type` の 30 variant）

| `Type` | 扱い |
| --- | --- |
| `Integer(w, signed)` | 対応（`(_ BitVec w)`。w は 8〜128） |
| `Bool` | 対応（`Bool`） |
| `Unit` | 対応（`Value::Unit`。項なし） |
| `Tuple(ts)` | 対応（成分がすべて対応のとき） |
| `Record(id, args)` | 対応（非再帰・全 field が対応のとき。`types.record_fields`） |
| `Union(id, args)` | 対応はペイロードなしの場合だけ（`types.union_payloads` がすべて `None`。再帰型は不可）。それ以外は `a union with a payload` |
| `Reference(t, false)` | 対応（`t` と同じ値。借用は透過） |
| `Reference(_, true)` | 不可: `a mutable reference` |
| `Binary`・`Decimal`・`Simd` | 不可: `a floating-point value`（`Binary` は f16〜f128、`Decimal` は d32〜d128、`Simd` はベクトル） |
| `Char`・`Utf8Char`・`String`・`Utf8String` | 不可: `a character` ／ `a string` |
| `Array`・`ArrayView`・`FixedArray`・`List`・`Vec` | 不可: `an array or list` |
| `Function`・`Task`・`Dyn`・`Shared`・`Handle` | 不可: `a closure`／`a task`／`a dyn value`／`a shared pointer`／`an extern handle` |
| `Length`・`Application`・`Partial`・`Variable`・`Infer`・`Error` | 不可: `an unresolved type`（特殊化の後には出ない） |

### C.3 式の分類（`TypedExprKind` の 64 variant）

「対応」は v1 で翻訳する形。「不可」は `Unsupported(reason)` を返し、その関数は `E1034` `prove_unsupported`（`{reason}` は右の英語）になる。

| variant | 扱い |
| --- | --- |
| `Int(n)` | 対応: `(_ bv<n mod 2^w> <w>)`（型の幅 w） |
| `Bool`・`Unit` | 対応 |
| `Local(id)` | 対応: 環境 `BTreeMap<usize, Value>` から引く（引数・`Block`・`Match` の束縛） |
| `Unary(op, e)` | 対応: `Negate` = `bvneg`、`Not` = `not`、`BitNot` = `bvnot`、`Plus` = そのまま |
| `Binary(op, a, b)` | 対応（C.4）。`Power` は `a power operator`、`Pipe` は `an unresolved pipe` |
| `Call(callee, args)` | 対応: callee が `Function(User(id))` なら本体を**インライン展開**（引数を環境に束縛。深さ 8 まで、超えたら `a call deeper than 8`。呼び先が再帰（自分または相互）なら `a recursive call`）、`Function(Builtin(b))` は `Assert`・`Unreachable`・`Not`・`IntMin`・`IntMax`・`IntAbs`・`ContractRequires`・`ContractEnsures` だけ（C.5）。ほかの builtin は `a call of '<name>'`。`Closure`・`Local` の callee は `a closure call` |
| `Function(_)` | callee の位置だけ（上）。値としては `a function value` |
| `If` | 対応: `ite`（`path` の分け方は C.1） |
| `Match` | 対応: `local` に `value` の値を束縛してから、腕を順に、`alternatives` の各 `steps` を前から（`Test(e)` は条件の `∧`、`Bind(l, e)` は環境に束縛）、alternative の `∨`、`guard` があれば `∧`。値は腕の `ite` の連鎖で、最後の腕は条件なし（網羅はチェッカーが保証）。`alternatives[i].bindings` も束縛する |
| `Block` | 対応: `bindings` を順に評価して環境に束縛し、`result` を評価 |
| `Record(fields)`・`RecordUpdate`・`Tuple`・`Field(e, i)` | 対応（成分の `Value` を組む／取り出す。`Record` は field の添字順） |
| `Borrow(e, false)`・`BorrowOperand(e)`・`Dereference(e)` | 対応: `e` の値（借用は透過） |
| `Borrow(_, true)` | 不可: `a mutable reference` |
| `Cast(e)` | 対応は整数から整数だけ: 幅が減る = `(_ extract w2-1 0)`、同じ = そのまま、増える = 元が符号付きなら `(_ sign_extend n)`、符号なしなら `(_ zero_extend n)`。それ以外は `a cast of '<type>'` |
| `Construct { case_id, payload: None }` | 対応: `Tag` = `(_ bv<case_id> 32)`（パターンの `UnionTag(v) == Int(case_id)` と同じ番号。`src/control.rs` の `pattern_alternatives`） |
| `Construct { payload: Some(_) }`・`UnionPayload` | 不可: `a union with a payload` |
| `UnionTag(e)` | 対応: `e` の `Tag` |
| `StructuralCompare(Equal \| NotEqual, [a, b])` | 対応: 成分ごとの `=` の `and`（`NotEqual` はその `not`）。ほかの演算は `a structural comparison` |
| `Error`・`GenericFunction`・`Method`・`TypeFunction`・`GenericInteger`・`GenericFloat`・`CaseConstructor` | 不可: `an unresolved expression`（特殊化の後には出ない） |
| `Float`・`String`・`StructuralHash`・`StructuralDisplay`・`Interpolated` | 不可: `a floating-point value`／`a string`／`a hash`／`a text conversion` |
| `Break`・`Continue`・`While`・`ForRange`・`ForEach` | 不可: `a loop` |
| `Lambda`・`Closure`・`TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel` | 不可: `a closure`／`a task` |
| `HostCall`・`Static` | 不可: `an extern call`／`a static value` |
| `Array`・`List`・`ListTail`・`NewArray`・`NewList`・`NewLiteral`・`Index`・`Slice`・`Length`・`StringLength` | 不可: `an array or list`／`a string` |
| `Assign` | 不可: `an assignment` |
| `Checked` | 不可: `a '@checked' operation` |
| `Try`・`RaisedException`・`Reraise` | 不可: `an exception` |
| `DynDispatch` | 不可: `a dyn call` |

### C.4 演算子の対応（幅 w、符号 s は**左オペランド**の `Type::Integer(w, s)`。右オペランドは同じ型）

| 演算 | 翻訳 | トラップの問い |
| --- | --- | --- |
| `+ - *` | `bvadd bvsub bvmul`（折り返す） | なし |
| `/` `%` | 符号付き `bvsdiv bvsrem`、符号なし `bvudiv bvurem` | `path ∧ (= b 0)`（`integer division by zero`）。符号付きは加えて `path ∧ (= a MIN) ∧ (= b -1)`（`integer division overflow`。`MIN` = `#x8000..`、`-1` = `#xffff..`）。SMT-LIB は 0 での除算を全ビット 1／被除数と定義するが、使わない（問いで除く） |
| `< <= > >=` | 符号付き `bvslt bvsle bvsgt bvsge`、符号なし `bvult bvule bvugt bvuge` | なし |
| `==` `!=` | `=`、`(not (= ..))`（`Bool` も同じ） | なし |
| `&&` `\|\|` | `and` `or`（右辺は C.1 の path で短絡） | 右辺の問いは短絡の path つき |
| `&&&` `\|\|\|` `^^^` | `bvand bvor bvxor`（整数だけ。`bool` に `Bits` の instance はない） | なし |
| `<<<` | `(bvshl a (bvand n (_ bv<w-1> w)))`（量は値と**同じ型**。違う型は型エラー E1003） | なし |
| `>>>` | 符号付き `(bvashr a (bvand n (_ bv<w-1> w)))`、符号なし `bvlshr` | なし |
| 単項 `-` | `bvneg`（最小値は最小値のまま） | なし |

### C.5 組み込み関数の翻訳

| builtin | 翻訳 |
| --- | --- |
| `Assert(c)`・`ContractRequires(c)`・`ContractEnsures(c)` | 値 `Unit`。問い `path ∧ (not c)`（`assertion failed`／`contract requires failed`／`contract ensures failed`。`span` は呼び出し式） |
| `Unreachable(..)` | 値は呼び先の型に応じた自由な項（`Unit` のみ対応。それ以外は `a call of 'unreachable'`）。問い `path`（`unreachable code reached`） |
| `Not(b)` | `(not b)` |
| `IntMin(a, b)`・`IntMax(a, b)` | 符号付き `(ite (bvsle a b) a b)`・`(ite (bvsge a b) a b)`、符号なしは `bvule`・`bvuge` |
| `IntAbs(a)` | 符号付きだけ: `(ite (bvslt a (_ bv0 w)) (bvneg a) a)`（最小値は最小値のまま）。符号なしは `a call of 'Int.abs'` |
| `IntClamp` | 不可（`a call of 'Int.clamp'`。端の値の意味を未確認のため） |

インライン展開した呼び先の中の `$requires`／`$ensures` も同じ問いになる（呼び先の契約違反は呼び元の `trap_free` の反例になる）。`$builtin` の生成関数（`Function(User(id))` で本体が builtin の呼び出し）は、ふつうのインライン展開で届く。

### C.6 検証条件と台本の形

問いは 1 関数につき `trap_free` 1 つと `@ensures` の行ごと 1 つ。**問い 1 つ = 台本 1 つ = ソルバーのプロセス 1 つ**（決定的で、時間切れが問いごとに分かる）。

| 問い | 台本に足す `assert` | `unsat` の意味 |
| --- | --- | --- |
| `trap_free` | 前提（すべての `@requires` と union の範囲）、`(or o0 o1 ..)` | 前提を満たす入力で本体はトラップしない |
| `ensures_j` | 前提、`(not (or o0 o1 ..))`、`(not e_j)` | トラップしない実行で `e_j` は成り立つ |

`obligations` が空なら `trap_free` は台本を作らず `proved`（ソルバーを呼ばない）。`@ensures` が 0 個で `trap_free` だけの関数も同じ。`@requires` が矛盾していれば全部 `unsat`＝`proved`（空虚な証明。文書に書く）。台本は次の順（決定的）:

```text
(set-logic QF_BV)
(set-option :produce-models true)
(declare-const p<i>[_<k>] <sort>)      ; 引数。宣言順
(define-fun t<N> () <sort> <項>)       ; 出現順の通し番号
(define-fun o<k> () Bool <cond>)       ; トラップの問い（C.1）
(assert <前提>) ... (assert <この問いの assert>)
(check-sat)
(get-value (<引数の成分> <結果の成分> o<k> ..))   ; sat のときだけ読む
```

### C.7 ソルバーの起動と出力の読み方

- **解決**: `driver::resolve_tool("TSUZURI_SMT", "z3")`（`pub`。環境変数 → 配布物の `bin/` → `PATH`）。環境変数が未設定で `z3` の起動が `ErrorKind::NotFound` なら `cvc5` を試す。どちらもなければ `E1034` `prove_solver_missing`（このとき `prove` は何も検証せず、終了コード 1）。
- **起動**: `std::process::Command` を直接使う（`driver::run_tool` は標準入力を渡せず非公開）。z3: `-smt2 -in -t:<ミリ秒>`。cvc5（**未確認**）: `--lang=smt2 --produce-models --tlimit=<ミリ秒>`。`Stdio::piped()` の 3 本、台本は別スレッドで標準入力へ書いて閉じ、標準出力は別スレッドで最大 1 MiB まで読む。本スレッドは 10 ミリ秒おきに `try_wait`、`--prove-timeout` + 5 秒（壁時計）で `kill`。
- **読み方**: 標準出力の**最初の行だけ**で判定する。`unsat` → `proved`。`sat` → 残りを S 式として読み反例にする。`unknown` → `E1034` `prove_unknown`。それ以外の文字列・起動の失敗・強制終了・出力の超過 → `E1034` `prove_solver_error`。`unsat`・`unknown` の後ろの `(error "... model is not available")` は無視する（実測。付録 A.5）。モデルの値は `#x<16進>`・`#b<2進>`・`(_ bv<10進> <幅>)`・`true`・`false` のどれでも読めるようにする。
- 実測（z3 5.1.0）: `-t:20` で素因数分解の台本は `unknown`、`-t:100` では `sat`（付録 A.5）。ソフトな `-t:` は `unknown` を返す（プロセスは終わる）。

### C.8 `clamp` の台本（実測）

`def clamp :: i64 -> i64 -> i64 -> i64` に `@requires lo <= hi`・`@ensures \r -> lo <= r && r <= hi`。引数は `p0`（`lo`）・`p1`（`hi`）・`p2`（`x`）。`ensures_0` の台本（`t3` が結果、`t4` が前提、`t7` が `e_0`）:

```text
(set-logic QF_BV)
(set-option :produce-models true)
(declare-const p0 (_ BitVec 64))
(declare-const p1 (_ BitVec 64))
(declare-const p2 (_ BitVec 64))
(define-fun t0 () Bool (bvslt p2 p0))
(define-fun t1 () Bool (bvsgt p2 p1))
(define-fun t2 () (_ BitVec 64) (ite t1 p1 p2))
(define-fun t3 () (_ BitVec 64) (ite t0 p0 t2))
(define-fun t4 () Bool (bvsle p0 p1))
(define-fun t5 () Bool (bvsle p0 t3))
(define-fun t6 () Bool (bvsle t3 p1))
(define-fun t7 () Bool (and t5 t6))
(assert t4)
(assert (not t7))
(check-sat)
(get-value (p0 p1 p2 t3))
```

`z3 -smt2 -t:10000` の出力は `unsat`（0.18 秒）のあとに `(error "line 17 column 24: model is not available")`。事後条件を `r < hi` に壊した版（`t5` を `(bvslt t3 p1)`、`(assert (not t5))`）は `sat` と `((p0 #x8000000000000000) (p1 #x6ffffffffffffffe) (p2 #x7000000004000400) (t3 #x6ffffffffffffffe))`（0.04 秒）。前者は `proved`、後者の `E1034` は `'@ensures \r -> r < hi' of 'Main.clamp' can fail: Main.clamp (-9223372036854775808) 8070450532247928830 8070450532315038720 returns 8070450532247928830`。**反例の数値は solver の版で変わりうる**ので、テストは数値を固定せず「反例を native で実行すると契約でトラップする」ことを確かめる。

除算の `trap_free`（`def half :: i32 -> i32 -> i32` に `@requires b != 0`、本体 `a / b`）の台本は、`(declare-const p0 (_ BitVec 32))`・`p1`、`(define-fun t0 () Bool (distinct p1 #x00000000))`（前提）、`(define-fun o0 () Bool (= p1 #x00000000))`、`(define-fun o1 () Bool (and (= p0 #x80000000) (= p1 #xffffffff)))`、`(assert t0)`、`(assert (or o0 o1))`、`(check-sat)`、`(get-value (p0 p1 o0 o1))`。z3 は `sat` と `p0 = #x80000000`・`p1 = #xffffffff`・`o0 = false`・`o1 = true`。反例の文面は `Main.half (-2147483648) (-1) traps: integer division overflow`（最初に `true` の `o<k>` の `what`）。

### C.9 反例の復元・文面・報告

- `{call}` = `<module>.<name>` と引数を 10 進で並べる（負数は `(-5)`。`Bool` は `true`/`false`、`Unit` は `()`、組は `(a, b)`、record は `Name { f: v }`、ペイロードなしの union は `Module.Case`）。引数の成分はモデルの `p<i>_<k>` から型に従って組み直す（`Bits` は符号付きなら 2 の補数）。
- `{outcome}`: `ensures_j` は `returns <結果>`（結果の成分のモデルから）。`trap_free` は `traps: <what>`（最初に `true` の `o<k>`。`what` は C.4・C.5 の文言）。
- `{clause}`: `ensures_j` は `@ensures \r -> <text>`（`ContractClause.text`）、`trap_free` は `the body`。
- 診断の位置は C ではなく本体「診断」の表のとおり。
- **`--json`**（標準出力。JSON Lines）: 関数ごと `{"type":"prove","function":"Main.clamp","result":"proved","clauses":[{"kind":"trap_free","result":"proved"},{"kind":"ensures","index":0,"result":"proved"}]}`、最後に `{"type":"summary","proved":N,"failed":M}`。`result` は `proved`・`counterexample`・`unknown`・`unsupported`・`error`、`clauses` の `kind` は `trap_free`・`ensures`（`index` は `@ensures` の番号）。キーの順序は固定、時間を入れない（決定的）。

### C.10 上限・限界・未確認

- 台本の定義（`t<N>`・`o<k>`）が 50,000 個を超えたら `E1017`（`a proof script is limited to 50000 definitions; split the function or its callees`）。インラインの深さ 8。出力 1 MiB。
- 未確認: cvc5（台本は標準の `QF_BV`＋`get-value`。出力の読み方は 3 通りの値の形を読む）。`QF_BV` の幅 128 の `bvmul` の速さ（差分テストは 64 bit まで）。
- 限界: 契約の式の除算（`@requires`／`@ensures` の条件が除算・剰余を含むと、条件自体がトラップしうる。v1 は `prove_unsupported`（`a division in a contract`）にする）。`Float`・文字列・配列・ループ・クロージャ・`ref mut` は対象外（C.2・C.3）。再帰関数は展開できない（`a recursive call`）。
- `prove` は証明済みの検査を取り除かない（D8）。

### C.11 差分テスト `smt_semantics_match_native_execution`（3,312 件）

型 `i8`・`i8u`・`i32`・`i64u`、演算 `+ - * / % < >= == &&& ||| ^^^ <<< >>>`（13 個）、値は符号付きが `[最小, 最小+1, -7, -1, 0, 1, 7, 最大-1, 最大]`、符号なしが `[0, 1, 7, 2^(w-1)-1, 2^(w-1), 最大-1, 最大]` の全組（ゼロ除算と符号付きの `最小 / -1` は除く。件数は i8 1,033・i8u 623・i32 1,033・i64u 623）。**参照は native の実行**: 型ごとに 1 つの Tsuzuri プログラム（`let v0: i8 = (-128)`・…・`Debug.print (ref (to_string (v0 + v1)))`）を作り、`tsuzuri run -O0` の標準出力を期待値にする。**被験は SMT-LIB の翻訳**: 同じ式を `Translator` で台本にし、`z3 -in` に `(simplify <項>)` を流して `#x..` を 10 進に直す。両者の行を比べる（z3 がなければ skip し、標準出力に `skipped: z3 not found`）。1 件でも違えば停止条件 9。

## 付録 D: `Gen`・`Arbitrary`・`deriving (Arbitrary)`（Phase 3）

### D.1 `std/Gen.tz` に足す関数（`Gen.of_fn` は作らない。導出も `Gen.map`・`Gen.pair`・`Gen.one_of` だけで書ける）

`for_all` の次（ファイルの末尾）に足す。**手元の試作**（`Gen.__seed()` を定数に置いたコピーで `tsuzuri test` を実行）の結果: 正しい `clamp` は `ok`、事後条件を壊した `clamp` は `property failed after 7 tries (seed 11400714819323198485)`・`counterexample: (0, (0, 0))`・`shrunk 1 times from (-9223372036854775808, (-471411656769, -15014))`、常に `None` の性質は `property gave up: 0 of 100 values were valid after 1000 tries`。

```tsuzuri
/// Runs `generator` on `source`, then `property` on its value: its verdict (`None` discards the
/// value), and the choices that the run made.
private def attempt_where :: Gen<'a> -> ('a -> Maybe<bool>) -> Source -> (Maybe<bool> * [i64u])
fn attempt_where generator property source =
	match generator.run source with
	| (value, (_, _, _, choices)) -> (property value, Vec.to_array choices)

/// Checks `property` on values of `generator` until `cases` of them are valid (the property
/// is not `None`), and traps on the first value where it is `Some false`, as `check` does.
/// More than nine discarded values per valid one end the test with a failure.
def check_where :: (ref 'a -> string) -> i64 -> Gen<'a> -> ('a -> Maybe<bool>) -> unit
fn check_where show cases generator property =
	assert (cases > 0)
	let seed = Gen.__seed()
	let limit = cases * 10
	let holds = \value ->
		match property value with
		| Some false -> false
		| _ -> true
	let mut tried = 0
	let mut valid = 0
	let mut failing: [i64u] = []
	let mut failed = false
	while !failed && valid < cases && tried < limit do
		match attempt_where generator property (search seed (tried as i64u)) with
		| (Some false, choices) ->
			failing = choices
			failed = true
		| (Some true, _) ->
			valid = valid + 1
			tried = tried + 1
		| (None, _) -> tried = tried + 1
	if failed then
		match shrink generator holds failing with
		| (smallest, shrinks) ->
			let header = "property failed after " + to_string tried + " tries (seed " + to_string seed + ")"
			let found = "counterexample: " + describe show generator smallest
			let original = "shrunk " + to_string shrinks + " times from " + describe show generator failing
			Debug.print (ref header)
			Debug.print (ref found)
			Debug.print (ref original)
			assert false
	else if valid < cases then
		Debug.print (ref ("property gave up: " + to_string valid + " of " + to_string cases + " values were valid after " + to_string tried + " tries (seed " + to_string seed + ")"))
		assert false

/// Checks `property` on 100 valid values of `generator`, as `check_where` with `Display.display`.
def for_all_where :: Display<'a> => Gen<'a> -> ('a -> Maybe<bool>) -> unit
fn for_all_where generator property = check_where (\value -> Display.display value) (default_cases()) generator property
```

捨てた値は縮小で「成り立つ」側に数える（`holds`）ので、縮小が捨てる値へ逃げない。`Gen.__seed()` は std の `Gen` の中だけで書ける（利用者のモジュールでは `E1022`）。

### D.2 組み込みクラス `Arbitrary`（`Encode`／`Decode` と同じ登録）

| 場所 | 変更 |
| --- | --- |
| `src/polymorph.rs` の `BUILTIN_CLASSES` | `[&str; 33]` → `[&str; 34]`、`"Decode"` の次に `"Arbitrary"` |
| `src/polymorph.rs` の `Classes::collect`（`Encode`／`Decode` の登録の隣、`classes.declarations[classes.names[class]].methods.push(..)`） | クラス `Arbitrary` のメソッド `arbitrary`: `Signature { parameters: Vec::new(), result: names.std_type("Gen", "Gen", vec![Type::Variable("a".into())].into(), Span::default()) }`（失敗したら `E1004` `Arbitrary needs the standard module Gen`）、`operation: None`、`default: None` |
| `src/stdlib.rs` の `OPT_IN` の `module: "Gen"` | `names: &["Gen"]` → `&["Gen", "Arbitrary"]`（`Arbitrary` や `deriving (Arbitrary)` を書くと `Gen` が読み込まれる） |
| `std/Gen.tz` | 下の instance（`instance Encode<i64> { fn rec encode value = .. }` と同じ書き方。`Arbitrary` のメソッドは引数なし） |

```tsuzuri
instance Arbitrary<i64> { fn arbitrary = Gen.i64() }
instance Arbitrary<i32> { fn arbitrary = Gen.i32() }
instance Arbitrary<i16> { fn arbitrary = Gen.map (\value -> value as i16) (Gen.i64()) }
instance Arbitrary<i8> { fn arbitrary = Gen.map (\value -> value as i8) (Gen.i64()) }
instance Arbitrary<i64u> { fn arbitrary = Gen.map (\value -> value as i64u) (Gen.i64()) }
instance Arbitrary<i32u> { fn arbitrary = Gen.map (\value -> value as i32u) (Gen.i64()) }
instance Arbitrary<i16u> { fn arbitrary = Gen.map (\value -> value as i16u) (Gen.i64()) }
instance Arbitrary<i8u> { fn arbitrary = Gen.map (\value -> value as i8u) (Gen.i64()) }
instance Arbitrary<bool> { fn arbitrary = Gen.bool() }
instance Arbitrary<f64> { fn arbitrary = Gen.f64() }
instance Arbitrary<char> { fn arbitrary = Gen.char() }
instance Arbitrary<string> { fn arbitrary = Gen.string() }
instance Arbitrary<'a> => Arbitrary<['a]> { fn arbitrary = Gen.array (Arbitrary.arbitrary()) }
instance Arbitrary<'a> => Arbitrary<Maybe<'a>> { fn arbitrary = Gen.maybe (Arbitrary.arbitrary()) }
instance (Arbitrary<'a>, Arbitrary<'b>) => Arbitrary<('a * 'b)> {
	fn arbitrary = Gen.pair (Arbitrary.arbitrary()) (Arbitrary.arbitrary())
}
instance (Arbitrary<'a>, Arbitrary<'b>, Arbitrary<'c>) => Arbitrary<('a * 'b * 'c)> {
	fn arbitrary = Gen.map (\t -> match t with | (a, (b, c)) -> (a, b, c)) (Gen.pair (Arbitrary.arbitrary()) (Gen.pair (Arbitrary.arbitrary()) (Arbitrary.arbitrary())))
}
instance (Arbitrary<'a>, Arbitrary<'b>, Arbitrary<'c>, Arbitrary<'d>) => Arbitrary<('a * 'b * 'c * 'd)> {
	fn arbitrary = Gen.map (\t -> match t with | (a, (b, (c, d))) -> (a, b, c, d)) (Gen.pair (Arbitrary.arbitrary()) (Gen.pair (Arbitrary.arbitrary()) (Gen.pair (Arbitrary.arbitrary()) (Arbitrary.arbitrary()))))
}
```

手順 16 の最初のテストで、`Arbitrary.arbitrary()` の型が文脈（`let g: Gen<i64> = Arbitrary.arbitrary()`、`Gen.pair` の引数）から決まることを確かめる（`Default.default()` は同じ形で決まる）。決まらなければ、メソッドを `Arbitrary.arbitrary ()` ではなく `Arbitrary.gen()` などに変えず、**停止して報告する**。

### D.3 `deriving (Arbitrary)`

| ファイル | 変更 |
| --- | --- |
| `src/syntax.rs` | `enum DeriveClass` の `Decode` の次に `Arbitrary`、`name()` の `Self::Arbitrary => "Arbitrary"` |
| `src/parser.rs`（`derives`） | `"Arbitrary" => DeriveClass::Arbitrary`。`E1025` の文言を `only Eq, Ord, Display, Hash, Default, Encode, Decode and Arbitrary can be derived` に（`git grep -n "can be derived" -- src tests docs _tsuzuri` の全出現を直す） |
| `src/syntax_codec.rs` | `unit_enum!(DeriveClass { .. })` に `Arbitrary`、単体テストの表 `("DeriveClass", 7)` → `8`。`FRONTEND_FORMAT` + 1（手順 17） |
| `src/derive.rs`（`Build::make`） | 深さの `match` に `Lambda(_, body) => body.depth` を足し、`Unit` を深さ 0 の arm（`Name(_) \| QualifiedFunction(_) \| ..`）に加える（今は `_ => unreachable!()` で、どちらもない） |
| `src/derive.rs`（新しい補助） | `fn lambda(&self, parameters: &[&str], body: Expr) -> Result<Expr, Diagnostic>` = `ExprKind::Lambda(parameters.iter().map(\|name\| (self.ident(*name), false)).collect(), Box::new(body))` |
| `src/derive.rs`（`instances`） | レコード・union の両方で、`class == DeriveClass::Arbitrary && types.recursive(&ty)` なら `E1025` `deriving (Arbitrary) cannot build a recursive type; write the instance by hand`（位置は `deriving` の `span`） |
| `src/derive.rs`（`Build::record`・`Build::union`） | `Arbitrary` の枝（下） |

`Build::record` の `Arbitrary` の枝（`Default` の枝の隣）。フィールドが 0 個なら `E1025`（`deriving (Arbitrary) needs at least one field`）。n 個のとき、生成器 `g_i = self.call("Arbitrary", "arbitrary", Vec::new())`（`$class.Arbitrary.arbitrary()`）を右から組にして `pairs = pair(g_0, pair(g_1, .. g_{n-1}))`（`self.apply("Gen.pair", vec![a, b])`）。パターンも同じ形（`PatternKind::Tuple(vec![Binding($f_0), Tuple(vec![Binding($f_1), ..])])`）。本体は `self.make(ExprKind::Record { name, fields: [(field_i, name($f_i))] })`（`Default` の枝と同じ `name`）。n = 1 のときは組もパターンも作らず `Gen.map (\$f0 -> Record{..}) g_0`。それ以外: `definition("arbitrary", &[], self.apply("Gen.map", vec![self.lambda(&["$t"], self.matched("$t", vec![self.arm(pattern, body)])?)?, pairs])?)`。

`Build::union` の `Arbitrary` の枝。場合ごとに 1 つの生成器 `case_gen`: ペイロードあり = `Gen.map (\$p -> Owner.Case $p) (Arbitrary.arbitrary())`（`self.lambda(&["$p"], self.construct(owner, case, self.name("$p")?)?)`）、ペイロードなし = `Gen.map (\_ -> Owner.Case) (Gen.constant ())`（`self.lambda(&["_"], case_value)`、`self.apply("Gen.constant", vec![unit])`。`unit` は `ExprKind::Unit` 相当の式）。全体 = `Gen.one_of [case_gen..]`（`self.make(ExprKind::Array(cases))`）。`instance(..)` の `components` は全場合のペイロード型（`Default` だけが先頭の場合に絞る `count` の条件は変えない）。`Pattern` は `integer_pattern` と同じく `Pattern { kind, span: self.span, depth }`（`depth` は子の最大 + 1）で作る。

**手書きでの確認（実測）**: 次の手書きの展開は HEAD の `tsuzuri test` で通る（record・入れ子の record・payload のある場合とない場合の union）。導出の出力はこの形と同じ意味にする。

```tsuzuri
record Pt { x: i64, y: i64 }
union Shape =
    | Circle of i64
    | Rect of (i64 * i64)
    | Empty
def gen_pt :: Gen<Pt> = Gen.map (\t -> match t with | (a, b) -> Pt { x: a, y: b }) (Gen.pair (Gen.i64()) (Gen.i64()))
def gen_shape :: Gen<Shape> =
    Gen.one_of [Gen.map (\p -> Shape.Circle p) (Gen.i64()), Gen.map (\p -> Shape.Rect p) (Gen.pair (Gen.i64()) (Gen.i64())), Gen.map (\_ -> Shape.Empty) (Gen.constant ())]
```

使うときは `(gen_pt())`（引数なしの `def` は `()` で呼ぶ）。`Gen.constant` は `Copy<'a>` を要求するので、ペイロードなしの場合は `()` に対して使う。

## 付録 E: 契約から作る性質テスト（Phase 3）

### E.1 入口

`ContractMode::Test` のとき、`lib.rs` の `analyze_inputs_with_contracts` が、パース後・`modules` を作る前に、**利用者のモジュール**（`input.origin == ModuleOrigin::User`）の `Program` ごとに `check::contracts::generate_tests(program: &mut syntax::Program) -> Vec<Diagnostic>` を呼ぶ（`programs.iter_mut().zip(inputs)`）。戻りの診断（`W1009` `contract_test_skipped`）は、`check_modules_indexed_all` が `Ok(mut module)` を返したあとに `module.warnings.extend(..)`（関数の宣言順）。frontend キャッシュはパース結果だけを持つので、生成は何も変えない。

### E.2 作る条件（関数ごと。上から順に判定し、最初に当たった理由で `W1009` を出す）

| 条件 | 満たさないとき（`{reason}`） |
| --- | --- |
| `function.contracts` が空でない | （何もしない） |
| ジェネリックでない（引数・結果の `TypeExpr` に `Variable` がなく、`constraints` が空） | `it is generic` |
| 引数が 1 つ以上 | `it has no parameters` |
| 各引数の型が下の「生成できる型」 | `parameter '{name}' has type '{type}', which has no generator` |
| `@ensures` の結果の名前（`_` を除く）が高々 1 種類 | `its '@ensures' lines name the result differently` |

**生成できる型**（`TypeExprKind` の構造で判定。引数の `mutable`・`Reference` は不可）: `Named` が `i8 i16 i32 i64 i8u i16u i32u i64u bool f64 char string` のどれか。`Array(T)`・`Tuple(2〜4 個)`・`Apply(Maybe, [T])` で成分が生成できる型。**同じモジュールで宣言された**ジェネリックでない `record`／`union` で `deriving` に `Arbitrary` と `Display` の両方があるもの（`Named(名前)`）。それ以外（別のモジュールの型・`ref`・関数・`f32`・`i128`）は不可。`{type}` は `TypeExpr` を `docgen` の型の整形（`src/docgen.rs`）で書いた文字列。

### E.3 作るもの（Tsuzuri のソースで書いた同じ意味。実装は `Build` と同じく AST を組む）

```tsuzuri
private def $contract.raw.clamp :: i64 -> i64 -> i64 -> i64 = \lo hi x ->
    if x < lo then lo elif x > hi then hi else x     // 元の本体の clone。契約の行はなし

test "contract clamp" =
    Gen.for_all_where (Gen.pair (Arbitrary.arbitrary()) (Gen.pair (Arbitrary.arbitrary()) (Arbitrary.arbitrary()))) (\$args ->
        match $args with
        | (lo, (hi, x)) ->
            if lo <= hi then
                let r = $contract.raw.clamp lo hi x
                Maybe.Some (lo <= r && r <= hi)
            else Maybe.None)
```

- **双子** `$contract.raw.<名前>`: `FunctionDecl` の clone（`name` を変え、`contracts = Vec::new()`、`visibility = Private`、`exported = false`、`recursion = None`、`doc = None`）。自己再帰の中の呼び出しは元の関数（契約つき）を呼ぶままにする。`program.functions` の末尾へ足す。
- **試験** `TestDecl { name: "contract <名前>", name_span, body, span }` を `program.tests` の末尾へ。`span` は関数の名前の `span`。
- **生成器**: 引数ごとに `Arbitrary.arbitrary()`（`self.call("Arbitrary", "arbitrary", vec![])`）を `Gen.pair` で右へ入れ子にする（引数が 1 つなら生成器 1 つで、パターンは引数名そのもの）。パターンは同じ形の入れ子の `PatternKind::Tuple`。
- **性質**: `@requires` の条件（clone）を `&&` でつないだものが偽なら `Maybe.None`（捨てる）。真なら `let <結果の名前> = $contract.raw.<名前> 引数..` を束縛し、`@ensures` の条件（clone）を `&&` でつないで `Maybe.Some (..)`。`@requires` が 0 個なら `if` を作らない。`@ensures` が 0 個なら結果を `_` に束縛して `Maybe.Some true`（トラップしないことだけを確かめる）。結果の名前は、`@ensures` の（`_` 以外の）名前。
- 条件は引数と同じ名前の束縛で読む（パターンが関数の引数名を使う）。反例の表示は `Gen.for_all_where` の `Display` による。
- `W1009` の位置は関数の名前。メッセージは `no property test for 'Main.clamp': {reason}`。

### E.4 実行と報告

`tsuzuri test --contracts test` の出力に `ok 3 - Main contract clamp`（生成した試験）が並ぶ。違反は、`for_all_where` の出力（`property failed after N tries (seed S)`・`counterexample: (lo, (hi, x))`・`shrunk K times from ..`）が試験の失敗の下に出る。性質の中でトラップ（ゼロ除算・範囲外の添字・本体の中の `assert`）が起きると、その試験は縮小なしで落ちる（`Gen.check` と同じ。「現状 6」）。**`--seed` で再現できる**。

## 付録 F: 診断の例（期待される出力）

E1033・E1034・W1009 は HEAD にないので、次は仕様の文言から作った期待値（位置は 1 始まりの行:列）。

### F.1 `E1033`

| ソース | 期待 |
| --- | --- |
| `def f :: i64 -> i64 = \x -> @requires x > 0`（1 行目。次の行に `    x`） | `1:29: error[E1033]: a contract line must start a function body: write '@requires' and '@ensures' on their own lines right after '=' or '->', before the first statement`（kind `contract_misplaced`） |
| 3 行目 `    @requires x > 0`（本体の文 `let y = x` の後） | `3:5` 同上 |
| `    @requires x`（`x: i64`） | `2:15: error[E1033]: a contract condition must be 'bool', but this is 'i64'; write a comparison such as 'x > 0'`（`contract_not_bool`） |
| `def noisy :: i64 -> bool = \x ->`⏎`    Debug.print (ref "checking")`⏎`    x > 0`、`def f ..` に `    @requires noisy x` | `2:15: error[E1033]: a contract must be pure: it prints debug output (through 'Main.noisy'); call only pure functions in a contract`（`contract_impure`。位置は条件の中の `noisy` への参照） |
| `    @ensures x > 0` | `2:14: error[E1033]: '@ensures' takes a lambda that names the result: write '@ensures \r -> condition'`（`contract_result_lambda`） |
| `    @ensures \(a, b) -> a > 0` | `2:14` 同上 |
| `const def k :: i64 -> i64 = \n ->`⏎`    @requires n >= 0`⏎`    n` | `2:5: error[E1033]: a 'const def' cannot have contract lines: the compiler evaluates it before the program runs; use a 'def' or remove the line`（`contract_on_const`） |
| `def g :: i64 -> ref i64 = \x ->`（結果が参照）に `@ensures \r -> true` | `error[E1033]: an '@ensures' needs the result as a value, but 'ref i64' holds a reference; remove the line or return an owned value`（`contract_ensures_reference_result`） |

### F.2 `E1034`（`tsuzuri prove`。標準エラーの診断と標準出力の行）

テンプレート（`{..}` を埋める。`kind` は本文「診断」の表）: `prove_solver_missing` = `no SMT solver found; install z3 or cvc5, or set TSUZURI_SMT to its path`、`prove_unsupported` = `cannot prove '{function}': {reason}; see 'Prove' in the language reference for the supported subset`、`prove_counterexample` = `'{clause}' of '{function}' can fail: {call} {outcome}`、`prove_unknown` = `the solver could not decide '{function}' within {seconds} seconds; raise --prove-timeout or simplify the contract`、`prove_solver_error` = `the solver '{name}' failed: {detail}`。`{reason}` は付録 C.2・C.3 の英語、`{call}`・`{outcome}`・`{clause}` は C.9。

| 場合 | 期待 |
| --- | --- |
| `clamp` の `@ensures` を `r < hi` に壊す | 標準出力 `counterexample Main.clamp`、標準エラー `Main.tz:3:5: error[E1034]: '@ensures \r -> r < hi' of 'Main.clamp' can fail: Main.clamp (-9223372036854775808) 8070450532247928830 8070450532315038720 returns 8070450532247928830`（数値は solver の版で変わりうる）。終了コード 1 |
| 本体に `while` | 標準出力 `unsupported Main.sum_to (a loop)`、`error[E1034]: cannot prove 'Main.sum_to': a loop; see 'Prove' in the language reference for the supported subset` |
| `z3` も `cvc5` もない（`TSUZURI_SMT` 未設定） | `error[E1034]: no SMT solver found; install z3 or cvc5, or set TSUZURI_SMT to its path`。終了コード 1 |
| 時間切れ（`--prove-timeout 1` で難しい式） | `unknown Main.hard`、`error[E1034]: the solver could not decide 'Main.hard' within 1 seconds; raise --prove-timeout or simplify the contract` |
| `TSUZURI_SMT` が実行できない | `error[E1034]: the solver 'nope' failed: cannot execute` |

### F.3 `W1009`

| ソース | 期待 |
| --- | --- |
| `    @requires true` | `warning[W1009]: this contract is always true; remove it or state a real condition`（`contract_always_true`。位置は `true`） |
| `    @ensures \r -> x > 0`（`r` を使わない） | `warning[W1009]: the result name 'r' is not used; write '@ensures \_ -> ...' to say that the line is about the arguments`（`contract_result_unused`。位置は `r`） |
| `def apply :: (i64 -> i64) -> i64 -> i64` に契約があり、`--contracts test` | `warning[W1009]: no property test for 'Main.apply': parameter 'f' has type 'i64 -> i64', which has no generator`（`contract_test_skipped`。位置は `apply`） |

## 付録 G: E2E の fixture と suite（手順 14）

`tests/fixtures/contracts/Main.tz`（新規）。すべて `export def`（ハーネスは `tz_<名前>` を C から呼ぶ）。

```tsuzuri
export def clamp :: i64 -> i64 -> i64 -> i64 = \lo hi x ->
    @requires lo <= hi
    @ensures \r -> lo <= r && r <= hi
    if x < lo then lo elif x > hi then hi else x

export def safe_div :: i64 -> i64 -> i64 = \a b ->
    @requires b != 0
    @ensures \r -> r * b + a % b == a
    a / b

def rec sum_acc :: i64 -> i64 -> i64 = \n acc ->
    if n == 0 then acc else sum_acc (n - 1) (acc + n)

export def sum_to :: i64 -> i64 = \n ->
    @requires n >= 0
    sum_acc n 0

export def at_least :: i64 -> i64 -> i64 = \a b ->
    @ensures \r -> r >= a
    @ensures \value -> value >= b
    Int.max a b

export def wrap_add :: i8u -> i8u -> i8u = \a b ->
    @ensures \r -> r == a + b
    a + b

export def abs_diff :: i64 -> i64 -> i64 = \a b ->
    @requires a >= b
    @ensures \r -> r >= 0
    a - b
```

`tests/features.mjs` に足す suite（期待値は JavaScript の BigInt で作る。コンパイラの出力を写さない）:

```js
contracts: {
  options: ["--contracts", "check"],
  cases: [
    ["clamp", [0n, 10n, -5n], 0n], ["clamp", [0n, 10n, 5n], 5n], ["clamp", [0n, 10n, 50n], 10n],
    ["clamp", [min, max, 0n], 0n],
    ["safe_div", [7n, 2n], 3n], ["safe_div", [-7n, 2n], -3n],
    ["sum_to", [10n], 55n], ["sum_to", [1000000n], 500000500000n],
    ["at_least", [3n, 5n], 5n], ["wrap_add", [200n, 100n], 44n],
  ],
  traps: [
    ["clamp", [5n, 3n, 0n], "contract requires failed"], ["sum_to", [-1n], "contract requires failed"],
    ["abs_diff", [max, -1n], "contract ensures failed"],
  ],
  trapKind: "contract requires failed",
  inspect(ir) { assert.match(ir, /@tz\.builtin\.\$requires/); assert.match(ir, /@tz\.builtin\.\$ensures/); },
},
```

`wrap_add` の `200 + 100` は `i8u` で 300 mod 256 = 44、`abs_diff` の `最大 - (-1)` は折り返して最小（負）になるので `@ensures` が偽（トラップは `contract ensures failed`）。`safe_div (-7) 2` は 0 方向への切り捨てで `-3`、`-3 * 2 + (-7 % 2) = -7`。`min`・`max` は `tests/features.mjs` が既に持つ `i64` の端（`bounds_checks` の suite が使っている）。**native の runner は kind を確かめない**（0 以外の終了か signal だけ）ので、kind は WASM の `--trap-info` の `.trap.json` で確かめる（既存の仕組み。`traps` の第 3 要素）。`--contracts check` の IR に `@tz.builtin.$requires` があること（`inspect`）で、`off` の IR と区別する。**`wrap_add` は意図した折り返し**なので、A23（`Int.wrapping_add` と、`tsuzuri test`・`--overflow-report` での折り返しの報告）が済んでいる木（`rg -n "wrapping_add" src/check.rs`）では、`@ensures \r -> r == Int.wrapping_add a b` と本体 `Int.wrapping_add a b` で書く（期待値 44 は同じ）。A23 が先でなければ上のまま書き、A23 の手順 17 が書き換える（A23 付録 C.3）。

## 付録 H: Phase 3〜6 の実装手順（手順 15〜34）

Phase 1・2（手順 1〜14）は本文。Phase 1 で作った（新規の）`src/contracts.rs`・`tests/contracts.rs` に足していく。各手順の最後で `cargo build` と `cargo test --locked` が緑。

### Phase 3: 性質テスト（`--contracts test`）

### 手順 15: `Gen.for_all_where`・`Gen.check_where`

- 変更: `std/Gen.tz`・`tests/property.rs`。
- 内容: 付録 D.1 のコードを `for_all_cases` の次に足す。先に `tests/property.rs` へ失敗するテストを書く（手本は同じファイルの `for_all` のテスト）: (a) 正しい `clamp` の `for_all_where`（`lo <= hi` でないとき `None`）が `ok`、(b) 事後条件を壊した版が `property failed after`・`counterexample: (0, (0, 0))`・`shrunk 1 times from` を出して落ちる（`Gen.__seed()` の既定のまま。付録 A.6 と D.1 の実測と同じ値）、(c) 常に `None` が `property gave up: 0 of 100 values were valid after 1000 tries` で落ちる、(d) `cases = 0` が `assert` で落ちる。
- 確認: `cargo test --locked --test property` と `cargo test --locked honors_the_exact_specialization_limit`（`running N tests` の N が 0 でないこと）が緑。`Gen` の既存関数の出力が不変。

### 手順 16: 組み込みクラス `Arbitrary` と std の instance

- 変更: `src/polymorph.rs`・`src/stdlib.rs`・`std/Gen.tz`・`tests/contracts_property.rs`（新規）。
- 内容: 付録 D.2 の 3 つの変更と instance 群。`BUILTIN_CLASSES` の長さの定数 `33` が書かれた箇所（`git grep -n "BUILTIN_CLASSES\|; 33\]" -- src`）を `34` に。テスト: `let g: Gen<i64> = Arbitrary.arbitrary()`・`Gen.check .. (Arbitrary.arbitrary() : Gen<(i64 * bool)>)`・`Gen.for_all (Arbitrary.arbitrary() : Gen<[string]>)`・`Arbitrary<Maybe<i8>>` が型検査と実行を通る。`Arbitrary` を書かないプログラムでは `Gen` が読み込まれない（`OPT_IN` の `names` に `Arbitrary` を足しただけ）。
- 確認: `cargo test --locked --test contracts_property`・`--lib stdlib`・`--lib polymorph` が緑。`Arbitrary.arbitrary()` の型が文脈から決まらないときは停止（付録 D.2）。

### 手順 17: `deriving (Arbitrary)`

- 変更: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`（`FRONTEND_FORMAT` + 1）・`src/derive.rs`・`tests/contracts_property.rs`。
- 内容: 付録 D.3 の表のとおり。テスト: (a) `record Pt { x: i64, y: i64 } deriving (Arbitrary, Display)` の `Gen.for_all (Arbitrary.arbitrary())` が実行でき、壊れた性質を `counterexample: Pt { x: 0, y: 0 }` まで縮める、(b) payload のある場合とない場合の union、(c) 入れ子の record、(d) 再帰型（`union List = | Nil | Cons of (i64 * List)`）は `E1025`、(e) フィールドなしの record は `E1025`、(f) `deriving` の誤字の `E1025` の新しい文言。
- 確認: `cargo test --locked` が緑（`syntax_codec` の tag 網羅・`frontend` のテストを含む）。`node tests/frontend_cache.mjs target/release/tsuzuri`。

### 手順 18: 試験の生成 `generate_tests`

- 変更: `src/contracts.rs`（`generate_tests`・生成できる型の判定・`W1009` `contract_test_skipped`）・`src/lib.rs`（`analyze_inputs_with_contracts` の呼び出し）・`tests/contracts_property.rs`。
- 内容: 付録 E のとおり。`analyze_modules_with_contracts(.., ContractMode::Test)` で `module.tests` に `contract clamp` が増えること、双子 `$contract.raw.clamp` に契約がないこと（`contracts == ContractShape::default()`）、生成できない関数が `W1009` になることをテストする。
- 確認: `contract_tests_cover_every_eligible_function`（20 個の関数の表: 生成する／しない理由 5 種）・`generated_tests_find_a_broken_ensures`（壊れた `clamp` を `tsuzuri test --contracts test` 相当で走らせると失敗し、出力に `counterexample:` がある）・`generated_tests_do_not_change_other_modes`（`Check`・`Off` の `module.tests` は増えない）。

### 手順 19: `--contracts test` を解禁する

- 変更: `src/main.rs`（手順 10 の `--contracts test is not available yet` を外す）・`README.md`・`tests/contracts_property.rs`。
- 内容: `test` コマンドだけが `--contracts test` を受理（ほかは `E2000` `--contracts test is only valid with test`）。`run_test_action` が `ContractMode::Test` を `analyze_cached` に渡す（`test` の既定は `check` のまま）。
- 確認: `main.rs` の単体テスト `parses_contracts_option` に `test` を足す。`cargo test --locked --test contracts_property` の CLI のテスト（`tsuzuri test --contracts test --seed 1` の出力が 2 回で一致）。

### 手順 20: 文書（Phase 3）

- 変更: 「ドキュメント」の Phase 3 の行（`gen.md`・`type-classes.md`・`record.md`・`union.md`・`contracts.md`・`usage.md`・`option.md`）。
- 内容: `Gen.for_all_where`・`Arbitrary`・`deriving (Arbitrary)`・`--contracts test`・`W1009` `contract_test_skipped`。例は `run=` つき（`for_all_where` は `tsuzuri test` を使う例のため、ページの規則に従い `test` つきの例にする）。
- 確認: `node scripts/check-docs.mjs <変えたページ>`。

### Phase 4: 表示と警告

### 手順 21: `W1009`（`contract_always_true`・`contract_result_unused`）

- 変更: `src/contracts.rs`（`Checker::contracted_body` の中）・`src/check.rs`（`Checker.contract_warnings: Vec<Diagnostic>`、関数の検査のループの `function_warnings.append(&mut checker.contract_warnings)`、`checker.shadowing_warnings` の隣）・`tests/contracts.rs`。
- 内容: 織り込みの中で、(a) 条件が `ExprKind::Bool(true)` なら `contract_always_true`（位置は条件）、(b) `@ensures` の結果の名前が `_` でなく、条件の中に `Name(名前)` が 1 つもなければ `contract_result_unused`（位置は名前。`Expr::visit` で探す）。メッセージは本文「診断」の表。ユーザーのモジュールの関数だけ（`function.origin.module == ModuleOrigin::User`）。
- 確認: 付録 F.3 の 2 つが出る。`@ensures \_ -> true` と `@requires true` の組、結果を使う `@ensures` は出ない。`--deny-warnings` で終了コード 1。

### 手順 22: `tsuzuri doc` と hover

- 変更: `src/docgen.rs`（`render_declarations`）・`src/semantic.rs`（関数の `detail`）・`tests/`（docgen・lsp の既存テストの隣）。
- 内容: 公開関数の署名の次の行に `@requires <text>`・`@ensures \<名前> -> <text>` を 4 つの空白で字下げして足す（`signature(..)` が制約に `\n    @..` を足す形と同じ）。hover の `detail` の末尾にも同じ行（`semantic.rs` の `callables` の組に契約の行の `Vec<String>` を足す）。`text` は `ContractClause.text`。
- 確認: `tsuzuri doc` の出力に契約の行があり、契約なしの関数の出力は不変。LSP の hover のテストで `detail` に `@requires`。

### 手順 23: `api-diff`

- 変更: `src/api_diff.rs`・`tests/`（既存の `api_diff` のテストの隣）。
- 内容: `Item` に `contracts: BTreeSet<String>`（`@requires <text>`・`@ensures \<名前> -> <text>`）。`compare_items` の `derive` の変更の隣に、`kind: "contract"`・`name: "{item}.{line}"`・`note: ""` の `Change` を足す: `old.contracts` にだけある行 = `Action::Removed`、`breaking` は `@ensures` のとき真（保証が減る）。`new.contracts` にだけある行 = `Action::Added`、`breaking` は `@requires` のとき真（呼ぶ側の義務が増える）。`header`（署名）の比較は変えない（`--canonical` の同一性に契約を入れない）。
- 確認: 付録の表: `@requires` の追加＝breaking、削除＝non-breaking、`@ensures` の追加＝non-breaking、削除＝breaking、文言の変更＝1 件の削除と 1 件の追加。契約のない 2 つの版の出力は不変。

### 手順 24: facts（G26 があるとき）と文書（Phase 4）

- 変更: G26 の facts 生成（G26 で新規の `src/facts.rs` があるとき。`git ls-files src/facts.rs` が 1 行）・文書。
- 内容: facts の関数の行に `contracts: [{"kind":"requires"|"ensures","text":"..","line":N}]` を足す（追加のキーは読み手が無視できる。D-52）。`src/facts.rs` がなければ何もせず「G26 待ち」と報告して次へ進む。文書は `contracts.md`・`diagnostics.md`（`W1009` の 3 つ）・`usage.md`（`doc`・`api-diff`）。
- 確認: facts のテスト（あるとき）。`node scripts/check-docs.mjs <変えたページ>`。

### Phase 5: `prove`

### 手順 25: `prove` のコマンドと `--contracts prove`

- 変更: `src/main.rs`（`Action::Prove`・`parse_arguments`・`HELP`・`--prove-timeout`・`--function`・`run_prove_action`）・`src/driver.rs`・`README.md`・`tests/prove.rs`（新規）・`src/prove.rs`（新規。空の `run`）。
- 内容: 本文「モードと CLI」の表のとおり。`Action` の `match` の全箇所（`git grep -n "Action::Check" -- src/main.rs`）に `Prove` を足す。`prove` は `ContractMode::Prove` で解析し、`prove::run` の結果を出力する。`--contracts prove` は `build`／`run`／`test`／`check` で `prove::run` を先に走らせてから通常の処理を続ける（検証に失敗したら診断を出して終了コード 1）。`--prove-timeout` は 1〜3600（範囲外は `E2000`）、`--function` は繰り返せる。
- 確認: `main.rs` の単体テスト（受理・拒否の組み合わせ）。`tests/prove.rs::prove_requires_a_solver`（`TSUZURI_SMT=/nonexistent` かつ `PATH` に `z3` なし → `E1034` `prove_solver_missing`、終了コード 1。`PATH` を空にした子プロセスで確かめる）。

### 手順 26: 翻訳器（型・整数・`Bool`・演算）

- 変更: `src/prove_smt.rs`（新規）・`src/prove.rs`。
- 内容: 付録 C.1・C.2・C.4 のとおり。`Value`・`Script`・`Obligation`・`Translator::tr` の `Int`・`Bool`・`Unit`・`Local`・`Unary`・`Binary`・`Cast`・`If`・`Block`（付録 C.3 の「対応」の行から順に）と、`Unsupported` の理由。台本は `String`（決定的）。
- 確認: 単体テストで `clamp` の `ensures_0` の台本が付録 C.8 の文字列と**完全一致**。型の表（C.2）の 30 行・式の表（C.3）の 64 行を 1 件ずつ、対応／不可の判定と理由の文字列がテストされる（`Type`・`TypedExprKind` の variant を `match` で全部書き、足されたら落ちる）。

### 手順 27: 翻訳器（`Match`・record・tuple・union・呼び出し）

- 変更: `src/prove_smt.rs`。
- 内容: 付録 C.3 の `Match`・`Record`・`RecordUpdate`・`Tuple`・`Field`・`Construct`・`UnionTag`・`StructuralCompare`・`Call`（インライン展開。深さ 8・再帰の検出）と付録 C.5 の組み込み関数。呼び先の契約（`$requires`・`$ensures`）は問いにする。
- 確認: 単体テストで、`enum` の `match`（`Shape` の 3 場合）・タプルの引数・record の更新・インライン展開（深さ 9 で `a call deeper than 8`）・再帰（`a recursive call`）・`Assert` の問い。`i32` の `half`（付録 C.8）の `trap_free` の台本が文字列で一致。

### 手順 28: ソルバーの起動と出力の読み方

- 変更: `src/prove.rs`。
- 内容: 付録 C.7 のとおり。`run_solver(script: &str, options) -> Outcome { Unsat, Sat(Vec<(String, String)>), Unknown, Error(String) }`。モデルの値の 3 通りの形（`#x..`・`#b..`・`(_ bv.. ..)`）を読む関数と、2 の補数への変換。
- 確認: 偽の solver（Unix のテストだけ。テストが一時ディレクトリに `#!/bin/sh` の台本を作り、`TSUZURI_SMT` に渡す）: `unsat` → `proved`、`unsat` の後の `(error ..)` を無視、`sat` + モデル → 反例、`unknown` → `prove_unknown`、不正な出力・終了コード 1・出力の 1 MiB 超過 → `prove_solver_error`、`--prove-timeout 1` で寝続ける solver が 6 秒以内に kill されて `prove_solver_error`（`timed out`）。z3 があるときは本物の z3 で `clamp` が `proved`（`unsat`）と壊した版の `sat`。

### 手順 29: 問いと報告

- 変更: `src/prove.rs`・`src/main.rs`。
- 内容: 付録 C.6 の問い（`trap_free`・`ensures_j`）、反例の復元（C.9）、`E1034` の 5 つの kind（本文「診断」の表。位置は `ClauseResult` の `span`）、text と `--json` の出力（付録 C.9 の JSON Lines）、終了コード（0 = 選んだ関数がすべて `proved`、1 = それ以外、2 = 引数の誤り）、順序（モジュール名・宣言位置）。`--function` で選んだ名前がなければ `E2000` `no function named '{name}' has a contract`。
- 確認: `tests/prove.rs`: 付録 F.2 の 5 つの出力（z3 があるとき。なければ偽の solver で同じ診断）、`--json` の行が決定的（2 回の出力が一致）、`proved` の関数だけの出力で終了コード 0。反例の呼び出しを native で実行して契約違反になる（`prove_counterexamples_fail_when_run`: 壊れた `clamp` の反例の引数を `Main.clamp ..` の呼び出しとして `run`（`check` モード）に渡し、`E2005` の kind が `contract_ensures`）。

### 手順 30: 差分テスト

- 変更: `tests/prove.rs`。
- 内容: 付録 C.11 の `smt_semantics_match_native_execution`（3,312 件）。z3 がなければ skip（標準出力に `skipped: z3 not found`）。
- 確認: z3 のある環境で 3,312／3,312 件一致。1 件でも違えば停止条件 9。

### 手順 31: 文書と全体の確認（Phase 5）

- 変更: 「ドキュメント」の Phase 5 の行（`contracts.md` の `prove` の節に対応する部分集合の表（C.2・C.3 の対応）と例、`diagnostics.md`（`E1034` の 5 つ）、`usage.md`・`option.md`（`prove`・`--prove-timeout`・`--function`・`TSUZURI_SMT`）、`docs/language.md`・`docs/architecture.md`（`prove.rs`・`prove_smt.rs`）・`README.md`）。
- 内容: 「Phase 5 は z3（検証済み）または cvc5（未確認）が必要」と書く。`prove` が何を保証し何を保証しないか（証明済みの検査は省かない・対象の部分集合・空虚な証明）を書く。例の `proved` は `run=` ではなく ` ```text ` で示す（ソルバーに依存するため `check-docs` が実行しない）。
- 確認: GUIDE §3.1 の回帰テスト、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`、`node scripts/check-docs.mjs <ページ>`。

### Phase 6: `@old`

### 手順 32: `@old (式)` の構文

- 変更: `src/parse_control.rs`・`src/parser.rs`・`tests/contracts.rs`。
- 内容: `Parser` に `in_ensures: bool`。`contract_clause` が `@ensures` の条件を読む間だけ真にする。`checked_expression` の `@` の分岐で、次が `Ident("old")` なら、`in_ensures` のとき `old` を食べ、括弧つきの式を 1 つ読んで `ExprKind::Call(Box::new(Name($old)), vec![式])`（`Ident { text: "$old", provenance: Generated, .. }`）を返す。`in_ensures` でなければ `E1033` `contract_old_misplaced`（`'@old' is only valid inside an '@ensures' line`）。`@old` を括弧なしで書いたら `E0002`（`expected '(' after '@old'`）。`formatter.rs` の `canonicalize` は `$old` の呼び出しを素通しする（`@old (x)` は `Call` のトークンとして整形されるので、整形のテストで往復を確かめる）。
- 確認: `parses_old_inside_ensures`（`Call(Name($old), [式])`）・`rejects_old_outside_ensures`（`@requires`・本体・`test` の中）。`@old` を使わないプログラムの AST は不変。

### 手順 33: 持ち上げと型検査

- 変更: `src/contracts.rs`（`Checker::contracted_body`）・`src/check.rs`（`Checker.old_locals: Vec<(Span, Local)>`）・`tests/contracts.rs`。
- 内容: 織り込みの手順 3 と 4 の間（`requires` の後、本体の前）で、`@ensures` の条件を宣言順・式の出現順に走査し、`Call(Name($old), [e])` ごとに: (a) `e` の中にさらに `$old` があれば `E1033` `contract_old_nested`、(b) `e` が `@ensures` の結果の名前を参照すれば `contract_old_operand`（結果は入口に存在しない）、(c) `e` を型検査して束縛 `old_k`（`Local`、名前 `$old.k`、provenance `Generated`）にし、型が `Copy` でなければ `contract_old_not_copy`（`ref mut` の引数の中身を読む式は値が Copy のときだけ）。束縛は `Block` の `bindings` の `requires` の後ろ、結果の束縛の前に、`olds` 個（`ContractShape.olds`）。条件を型検査するとき、`Checker::call` が callee `Name($old)` のとき `old_locals` から（`Call` の `span` で）`Local` を返す。
- 確認: `old_is_hoisted_before_the_body`（`def bump :: ref mut [i64..] -> i64 -> unit` の `@ensures \_ -> xs[0] == v && @old (xs[0]) != v` を `[1, 2, 3]` と `v = 9` で `check` 実行すると通り、`v = 1` では `contract ensures failed`。`off` の IR は契約なしと一致）・`old_requires_copy`（`@old (xs)` が `contract_old_not_copy`）・`old_cannot_nest`・`old_cannot_read_the_result`。`strip` が `olds` を含めて元の本体を取り出す（手順 7 のテストに `@old` つきを足す）。

### 手順 34: `prove` の対応と文書（Phase 6）

- 変更: `src/prove_smt.rs`（変更なしのはず。`old_k` は `Block` の束縛なので C.3 の `Block` で届く）・`tests/prove.rs`・文書（`contracts.md` の `@old` の節、`diagnostics.md` の `E1033` の 4 つの kind）。
- 内容: `@old` を使う関数の `prove` の台本は、`old_k` を `define-fun` として同じ引数の項へ束縛する。`ref mut` の引数は C.2 で不可のため、`@old` の `prove` は値渡しと共有参照の引数だけ。
- 確認: `prove` の例（`def incr :: i64 -> i64 = \n ->` に `@ensures \r -> r == @old (n) + 1`、本体 `n + 1` が `proved`）。`node scripts/check-docs.mjs <ページ>`、GUIDE §3.1 の回帰テスト、全体の `cargo test --locked`。完了報告（GUIDE §13.2）を本体の末尾に書く。
