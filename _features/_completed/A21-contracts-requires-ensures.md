# A21: 契約 `@requires`／`@ensures`（off／check／test／prove）— 関数の約束を本体の先頭に宣言し、実行時検査・性質テスト・静的検証につなぐ

| 項目 | 内容 |
| --- | --- |
| ID | A21 |
| 優先度 | P2 |
| 規模 | XL（全体）。Phase 1: M ／ Phase 2: L ／ Phase 3: L ／ Phase 4: M ／ Phase 5: XL ／ Phase 6: M |
| 依存 | [A19](A19-pure-functions.md)（**Phase 1 の done が前提**。`pure def` と `src/pure_rules.rs` の効果の源の走査）。A20 Phase 1（任意。`src/effects.rs` があれば純粋性の走査にそれを使う）。G22（任意。`Diagnostic::with_kind`・`related`・`help` があれば使い、なければ付けない。「前提とする他チケットのインターフェース」）。F14（本チケットは機能として使わない。種の依存欄にあるが、使う箇所がない。逸脱として報告済み） |
| 後続 | G26（facts の `contracts` キー。任意）。ほかの第3期チケットで本チケットを前提にするものはない |
| 状態 | done（全 Phase。実装と検証は末尾の節） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D9 はすべて承認済み |
| 改善する劣位 | Eiffel・Ada/SPARK・Dafny・Kotlin（`require`／`ensure`）・Rust（`contracts` crate）は事前条件・事後条件を言語か標準の仕組みとして持ち、実行時検査・性質テスト・静的検証へつなぐ。Tsuzuri には `assert` だけがあり、関数の約束を署名のそばに宣言できず、AI が書いた関数の意図を機械が検査する足場がない（D-50 の調査結論 (2)「署名の契約として固定できない」） |
| 手本にする既存実装 | 属性: `Parser::checked_attribute`（`src/parser.rs`）・`checked_expression`・`body_expression`・`explicit_lambda`（`src/parse_control.rs`）。型検査: `Checker::spec_expression`（`src/exceptions.rs`）。組み込み関数: `Builtin::Assert` の全出現。トラップ: `TrapKind::RangeStepZero`（`src/trap.rs`）・`runtime_kind`（`src/llvm_traps.rs`）。導出: `src/derive.rs` の `DeriveClass::Default`、`BUILTIN_CLASSES`（`src/polymorph.rs`）、`OPT_IN`（`src/stdlib.rs`）。性質テスト: `std/Gen.tz`・`tests/property.rs`。整形: `canonicalize`（`src/formatter.rs`）。E2E: `tests/features.mjs` の `traps` |
| 主な影響ファイル | `src/contracts.rs`（新規。Phase 1〜6）・`src/prove.rs`（新規。Phase 5）・`src/prove_smt.rs`（新規。Phase 5）。Phase 1: `src/syntax.rs`・`src/parser.rs`・`src/parse_control.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/check.rs`・`src/pure_rules.rs`・`src/llvm.rs`・`src/lib.rs`・`src/driver.rs`・`src/main.rs`。Phase 2: `src/trap.rs`・`src/llvm_traps.rs`・`src/test_runner.rs`。Phase 3: `std/Gen.tz`・`src/polymorph.rs`・`src/stdlib.rs`・`src/derive.rs`。Phase 4: `src/docgen.rs`・`src/semantic.rs`・`src/api_diff.rs`・`src/warnings.rs`。テスト: `tests/contracts.rs`・`tests/contracts_property.rs`・`tests/prove.rs`・`tests/fixtures/contracts/Main.tz`（すべて新規）、`tests/features.mjs`・`tests/trap_locations.rs`。文書: `_tsuzuri/language-reference/`・`docs/language.md`・`docs/architecture.md`・`docs/stability.md`・`README.md` |

付録（実測ログ・`TrapKind` の全箇所・SMT-LIB の翻訳表・`Arbitrary`／`Gen` の骨格・生成する試験・診断の例・E2E・Phase 3〜6 の手順）は [A21-appendix.md](A21-appendix.md)。本文中の「付録 A〜H」はそのファイルの節。

## 実装者向け早見

- **最初に読む**: 本ファイルの「仕様」→「設計」→「実装手順」の Phase 1。参照するコード: `src/parse_control.rs` の `checked_expression`・`body_expression`・`explicit_lambda`、`src/exceptions.rs`、`Builtin::Assert` の 5 か所（`src/check.rs` の `enum Builtin`・`Builtin::ALL`・`name`・`signature` と `src/llvm.rs` の `emit_builtin`）。
- **最初に書くテスト**: 手順 3 の `tests/contracts.rs::parses_requires_and_ensures_on_lambda_and_fn_bodies`（失敗する形から始める）。
- **設計の背骨**（D1〜D5）: ① 契約は `FunctionDecl.contracts` に持つ（式の種類は増やさない）。② 型検査器（`Checker::contracted_body`）が本体を `Block`（`requires` の呼び出し → 本体 → `ensures` の呼び出し）に**織り込んだ型付き IR**を作る。③ モードは `check_modules` の入力。`off` は特殊化の**直前**に織り込みを剥がす（IR は契約なしと 1 byte も変わらない）。契約を含む `off` は先に `Check` の検査を 1 回行う。④ 実行時検査は組み込み関数 `$requires`・`$ensures` と `TrapKind` 2 つ。⑤ `prove` は型付き IR を SMT-LIB へ翻訳する。
- **触ってはいけないもの**: 型検査・所有権検査・特殊化・`closures::lower` の規則、`@checked` と `assert` の挙動、A19 の `pure_rules` の既存の判定（足すのは式の根を受け取る入口だけ）、契約を使わないプログラムの IR・診断・JSON（1 byte も変えない）。契約を最適化の前提にする（assume）経路を作らない（D-53）。
- **Phase の完了の見分け方**: 1 = `tsuzuri check` が契約を型検査・純粋性検査し、`build`・`run`・`test` の IR が契約なしと同一。2 = `--contracts check` の実行で `trap: contract requires failed at Main.tz:2:5`（native／WASM、`-O0`／`-O3`）。3 = `tsuzuri test --contracts test` が性質テストを作り、反例を縮小して出す。4 = `tsuzuri doc`・hover・`api-diff` が契約を出す。5 = `tsuzuri prove` が `proved`／反例を出す。6 = `@old (式)` が使える。
- **着手する Phase は人間の指示に従う**（指示がなければ Phase 1 だけ）。各 Phase は前の Phase の上に単独で出荷できる。

## 目的

関数の約束（呼ぶ側が守ること = 事前条件、関数が保証すること = 事後条件）を、本体の先頭に `bool` の式として書く。

```tsuzuri
def clamp :: i64 -> i64 -> i64 -> i64 = \lo hi x ->
    @requires lo <= hi
    @ensures \r -> lo <= r && r <= hi
    if x < lo then lo elif x > hi then hi else x
```

同じ宣言を、利用者が選ぶモードで 4 通りに使う（既定は `build` が `off`、`run`・`test` が `check`。`check`・`doc` は何も生成しないので、契約を常に型検査する）。

| モード | 何をするか | コスト |
| --- | --- | --- |
| `off` | 型検査・純粋性検査だけ。コードを出さない | 0（IR は契約なしと同一） |
| `check` | `@requires` を入口で、`@ensures` を出口で評価し、偽ならトラップ | 契約の式の評価 |
| `test` | `check` に加えて、`tsuzuri test` が契約から性質テスト（`Gen`）を自動生成して走らせ、反例を縮小して出す | テスト時のみ |
| `prove` | `check` に加えて、小さい純粋関数を SMT-LIB へ翻訳し、外部ソルバー（z3／cvc5）で検証する | ソルバーの時間 |

HEAD での測定（「現状」）: `@requires` は `E0002` で拒否される。代用の `assert` は、失敗しても `tsuzuri test` に位置が出ない。SMT-LIB のビットベクタ演算は、`i8`・`i8u`・`i32`・`i64u` × 13 演算 × 端の値の 3,312 件で、native の実行と 0 件不一致だった。

**非目標**（「対象外」も参照）: 契約を最適化の前提にするモード（assume）、契約の違反を例外にすること、呼び出し側の義務の静的検証、ループ不変式・停止性の証明、ソルバーの同梱。

## 着手条件と停止条件

### 着手条件

- **依存**: A19 Phase 1 が done（`git ls-files src/pure_rules.rs` が 1 行）。A20 は任意（`src/effects.rs` があれば純粋性の走査にそちらを使う。「前提とする他チケットのインターフェース」）。
- **ベースライン（GUIDE §2.3）**: `cargo build --release --locked && cargo test --locked` の件数を控え、`target/release/tsuzuri` を `/tmp/tz-A21/tsuzuri-before` に残し、`tests/fixtures` と `examples` の各プロジェクトを `--emit llvm -O0` で `/tmp/tz-A21/before/<名前>.ll` に保存する（手順 1。「IR が変わらない」確認に使う）。
- **読むもの**: GUIDE §6.2・§6.5・§6.7・§6.8・§6.9、D-51・D-53・D-55、[A19](A19-pure-functions.md) の「本体の規則: 効果の源」と「検証のアルゴリズム」。
- `z3` は Phase 5 の確認にだけ要る（調査は Z3 5.1.0）。なければ `z3` を使うテストは skip され、偽の solver のテストだけが走る。

### 停止条件

次のとき、即興で回避せず、作業を止めて報告する（GUIDE §13）。

1. **A19 が未実装**（`src/pure_rules.rs` がない）。選択肢は A（A19 を先に実装する。推奨）か B（`HostCall` と不純な組み込み関数の名前表だけの浅い検査で Phase 1 を先に出し、A19 の後で `pure_rules` に置き換える。呼び出しの先を見ないので保証が弱いことを文書に書く）。
2. 契約を使わないプログラムの受理・拒否・診断・生成 IR（`--emit llvm`）が 1 つでも変わった。**`@requires`・`@ensures` という名前の識別子・関数を使うプログラム**を含む（D4）。
3. `ContractMode::Off` で、契約付きの関数の IR が、契約の行を消した同じ関数の IR と 1 byte でも違う（手順 7 のテストが落ちる）。
4. 織り込みのために、型検査・所有権検査・特殊化・`closures::lower` の規則を変える必要が出た。織り込みは既存の型付き IR（`Block`・`Call`・`Local`）だけで組む。
5. A19／A20 の走査へ「契約の組み込み関数の引数へ降りない」規則を足すと、A19 の既存テストの期待値が変わる。
6. 既存テストの期待値を変えないと通らない（「既存テストへの影響」に書いたものを除く）。
7. 既存の `TrapKind` の番号（`repr(u32)`）が動く。2 つは**末尾**に足す（手順 9）。
8. 新しい予約語・crate・`unsafe`・既定の WASM の import・意味を変える最適化が要る。
9. SMT の翻訳が native の実行と食い違う（`tests/prove.rs::smt_semantics_match_native_execution` が 1 件でも落ちる）。選択肢は A（その演算を `unsupported` にする）か B（翻訳を直す。先に原因を報告する）。
10. `Gen` に足す API が `honors_the_exact_specialization_limit`（`src/polymorph.rs`）を落とす。
11. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

調査はリポジトリの外の `/tmp/tz-A21-*` で行った（このチケットの再現コマンドも同じ形。`target/release/tsuzuri` はビルド済み）。

### 現状 1: `@requires` は拒否される

```text
$ printf 'def clamp :: i64 -> i64 -> i64 -> i64 = \\lo hi x ->\n    @requires lo <= hi\n    if x < lo then lo elif x > hi then hi else x\n\nclamp 0 10 5\n' > /tmp/tz-A21-f/Main.tz
$ target/release/tsuzuri check /tmp/tz-A21-f
/tmp/tz-A21-f/Main.tz:2:5: error[E0002]: expected an expression; '@checked' is the only attribute of an expression
  2 |     @requires lo <= hi
    |     ^
```

トップレベルの `@requires 1 > 0` も、同じ行に書いた `\x -> @requires x > 0 x` も、同じメッセージの `E0002`。

### 現状 2: `@checked` の parser・型検査・lowering（手本）

| 段 | 場所 | 内容 |
| --- | --- | --- |
| 字句・構文 | `Parser::checked_attribute`（`src/parser.rs`） | 専用のトークンはない。`TokenKind::At` + `Ident("checked")` を食べて `bool` を返す。文の先頭で呼ぶ場所: `layout_block`（`src/parse_control.rs`）、`block_after_open`・`computation_statement`（`src/parser.rs`）。式の位置: `primary` の `TokenKind::At => return self.checked_expression(..)` |
| 構文 | `checked_expression`（`src/parse_control.rs`） | `@checked` でなければ `expected an expression; '@checked' is the only attribute of an expression`（現状 1 の出どころ）。`ExprKind::Checked(Box<Expr>)` を作る |
| 型検査 | `Checker::spec_expression`（`src/exceptions.rs`） | `self.checked_arithmetic = true` にして中を検査。型付き IR は `TypedExprKind::Checked` |
| 生成・走査 | `FunctionEmitter::checked`（`src/llvm_exception.rs`） | 溢れの検査つきの演算。`ExprKind::Checked` は 17 か所に出る（`syntax.rs`・`syntax_codec.rs`・`formatter.rs`・`check.rs`・`ownership.rs`・`llvm.rs` ほか） |

契約は**式ではなく関数の宣言の一部**なので、`ExprKind` の variant を足さない（17 か所の走査を増やさない。D1）。`@checked` の流儀（`@` + 識別子で読み分ける・ほかの `@名前` は `E0002`）は引き継ぐ。

### 現状 3: 数と形（`git grep` と簡単な script で数えた）

| 項目 | HEAD の値 | 確認コマンド |
| --- | --- | --- |
| `Builtin` の variant・`Builtin::ALL` | 246・246 | `src/check.rs` の `pub enum Builtin` と `Builtin::ALL` の要素を数える（手順 4 も同じ） |
| `TypedExprKind` の variant | 64 | 付録 C.3 が 64 個すべてを挙げる（C.2 は `Type` の 30 個） |
| `TrapKind` の variant（`TrapKind::ALL.len()`） | 16 | `src/trap.rs` |
| `DeriveClass` | 7（`Eq`・`Ord`・`Display`・`Hash`・`Default`・`Encode`・`Decode`） | `src/syntax_codec.rs` の `("DeriveClass", 7)` |
| `FRONTEND_FORMAT` | 4 | `src/frontend_cache.rs` |
| `FunctionDecl { .. }` を直接作る場所 | 7（`src/check.rs` 4・`src/parser.rs` 2・`src/polymorph.rs` 1）+ `src/syntax_codec.rs` の `put`・`get` | `git grep -n "FunctionDecl {" -- src` |
| `CheckedFunction { .. }` を直接作る場所 | 11（`src/check.rs` 3・`src/closures.rs` 4・`src/closures_rec.rs` 1・`src/polymorph.rs` 3） | `git grep -n "CheckedFunction {" -- src`（`impl`・型名だけの行を除く） |
| D-55 の割り当て | `E1033`・`E1034`・`W1009`。HEAD の `src/` に 0 件 | `git grep -n "E1033\|E1034\|W1009" -- src` が空 |

`@requires`・`@ensures` を表す名前は `src/` にない（`regions::contract` と `CallbackContract` は**名前付き寿命（A09）の「契約」**で別の概念。本チケットの型名は `ContractClause`・`ContractShape`・`ContractMode` とし、`regions::contract` に触れない）。

### 現状 4: 契約の形に手で織り込んだ関数の実測（`assert` で代用。ソースと結果は付録 A.2・A.3）

- 配列の引数を `requires` が読んでも複製しない（`@tz.alloc` が 0 件）。結果を `let r1 = 本体` に束縛して `let r2 = r1` で次の名前へ渡しても、複製ではなく移動（`store zeroinitializer`）。
- 値渡しの `string` を本体が移動し、`ensures` が同じ引数を読むと `E1012`。`ref mut` の引数の更新後の値は `ensures` から読める。
- 自己末尾再帰は、`requires` に当たる `assert` を前に置くだけなら 1,000 万段が `-O0` で通る（末尾呼び出しが保たれる）。`let r = 再帰呼び出し; assert (r >= 0); r`（`ensures` に当たる形）は `-O0` で 60,000 段まで通り、100,000 段で `trap: stack overflow`（`-O3` は 1 億段が通る）。**`@ensures` は末尾呼び出しを壊す**。
- 使われない関数を足しても IR は不変（到達可能な関数だけを出す）。ただし契約の式が先にジェネリックな関数を特殊化すると、本体の `$mono.N` の番号がずれる（付録 A.3。`strip` を特殊化の直前に置く理由）。

### 現状 5: トラップの報告

```text
$ target/release/tsuzuri run /tmp/tz-A21-e     (Main.tz の 2 行目に `assert (n > 0)`、n = -1)
trap: assertion failed at /tmp/tz-A21-e/Main.tz:2:5
/tmp/tz-A21-e/Main.tz:2:5: error[E2005]: program terminated with signal: 5 (SIGTRAP); integer division, indexing, assert, or allocation may have trapped
$ target/release/tsuzuri test /tmp/tz-A21-i
not ok 2 - Main traps
  failure: trapped or terminated by signal
```

`tsuzuri run` は既定で `--trap-info` が有効で、子プロセスの標準エラーの `trap: <説明> at <path>:<行>:<列>` を `E2005` の位置に写す（`src/driver.rs` の `run_captured`）。**`tsuzuri test` の runner は trap-info を入れないので、トラップした試験は位置を出さない**（手順 13 で、契約のあるときだけ入れる）。`.trap.json` の `kind` は `TrapKind::description()` の文字列。site の id は `1 + n * TrapKind::ALL.len() + kind`（`src/llvm_traps.rs`）。

### 現状 6: 性質テストと z3

- `Gen.for_all`・`Gen.check`・`tsuzuri test --seed N`（既定の seed `11400714819323198485`）がある。性質は `'a -> bool` で、**性質の中のトラップは縮小も報告もできずテストを終わらせる**（`_tsuzuri/language-reference/built-in-types-and-modules/gen.md`）。手書きの `Gen.for_all` は壊れた事後条件を `counterexample: (0, (0, 0))` まで縮める（付録 A.6）。
- z3 は `Z3 version 5.1.0`（`/opt/homebrew/bin/z3`）。`z3 -smt2 -t:10000 file.smt2` は `unsat`（証明）か `sat`+`(get-value ..)`（反例）。時間切れの 1 行目は `unknown`（付録 A.5）。`cvc5` は手元になく、同じ台本で書くが**未確認**（付録 C.10）。

## 変えてはいけない意味

- 契約の行を 1 つも書かないプログラムの受理・拒否・診断・JSON・生成 IR（`--emit llvm`・object・WASM）は 1 byte も変わらない。`@requires`・`@ensures` という名前の識別子・関数・フィールドは今までどおり使える（`@` の直後だけを属性として読む。`requires`・`ensures` は予約語にしない）。
- **`off` では、契約の行を足した関数の IR が、その行を消した関数の IR と 1 byte も変わらない**。型検査・純粋性検査・所有権検査は `off` でも同じ結果（`off` で通って `check` で落ちるプログラムを作らない。そのため契約を含むプロジェクトの `off` は検査を 1 回余分に行う）。
- `--contracts` を指定しない `tsuzuri build`・`check`・`doc`・`fmt`・`api-diff` の出力は、契約を使わないプログラムで変わらない。`run`・`test` の既定は `check` だが、契約がなければ生成物は同じ。
- 契約の式の評価は通常の式と同じ（左から右、短絡評価、整数の折り返し、トラップ）。**契約を最適化の前提にしない**。
- 既存の `TrapKind` の番号（`repr(u32)`）・`description()`・`.trap.json` の `kind` は変えない。足すのは末尾の 2 つだけ。
- 新しい予約語・crate・`unsafe`・既定の WASM の import を足さない。字句（lexer）にも足さない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 構文・codec・整形・織り込みの型検査・純粋性検査・`E1033`・`ContractMode`（`Off` の剥がし）。`build`／`run`／`test` は `off` 固定 | M | A19 Phase 1 | 契約を書かなければ何も変わらない。書いても `off` と同じ IR（検査だけ増える） |
| 2 | `--contracts off\|check`・`TrapKind` 2 つ・`E2005` の kind・test runner の trap 行・E2E | L | Phase 1 | `check` のときだけコードが増える。既定は `build` が `off`、`run`／`test` が `check` |
| 3 | `Gen.for_all_where`・`Arbitrary`・`deriving (Arbitrary)`・`--contracts test` の性質テスト生成 | L | Phase 2 | `--contracts test` のときだけ試験が増える |
| 4 | `W1009`・docgen・hover・`api-diff`・facts | M | Phase 1（facts は G26 次第） | 表示と警告だけで、生成物は変わらない |
| 5 | `tsuzuri prove`・`--contracts prove` | XL | Phase 1 | ソルバーがなければ `E1034`（`prove_solver_missing`）で止まるだけ |
| 6 | `@old (式)` | M | Phase 2・Phase 5 | `@old` を書かなければ何も変わらない |

## 仕様

### 構文

```text
body           ::= contract-line* statement+        // 契約の行は本体の最初の行から連続する
contract-line  ::= "@requires" expression EOL
                 | "@ensures" "\" ( identifier | "_" ) "->" expression EOL
```

- `@` の直後の識別子 `requires`／`ensures` だけが契約の行を始める（予約語ではない。`requires` という名前の変数・関数は今までどおり）。**各行は `->`／`=` の次の行から**、本体の文と同じインデントで書く。式は `let` の右辺と同じ規則で、次の行が演算子で始まり深くインデントされていれば続く。`tsuzuri fmt` は契約の行を本体の文と同じに整形する。
- 契約は `FunctionDecl.contracts: Vec<ContractClause>` に入る。本体 `FunctionDecl.body` には入らない。

| 書き方 | 結果 |
| --- | --- |
| `def f :: T = \x y ->`（入れ子のラムダ `\a -> \b ->` なら**最も内側**）、`fn f x y =`、`def f :: T =`（ラムダでない本体）の次の行から連続する契約の行 | 契約 |
| `@requires` と `@ensures` が混ざる | 契約。`@requires` は宣言順に入口で、`@ensures` は宣言順に出口で評価する（行の並びで評価時点は変わらない） |
| `->`・`=` と同じ行に `@requires` | `E1033` `contract_misplaced` |
| 本体の文のあとの契約の行 | `E1033` `contract_misplaced` |
| 本体が `{ ... }`、ガード付きの定義 `\| ..`、`let f = \x -> ..` の形のトップレベル関数 | `E1033` `contract_misplaced`（v1 は非対応） |
| 式の位置（引数・括弧の中）・入れ子のラムダの本体・ローカル関数・`test`・`bench`・クラスの既定メソッド・`instance` のメソッド・トップレベルの式 | `E1033` `contract_misplaced` |
| `const def` の契約 | `E1033` `contract_on_const` |
| `@ensures` の結果の型が参照を含む（`ref [i64]` を返す場合） | `E1033` `contract_ensures_reference_result`（`@requires` は書ける） |
| 契約の行だけで本体の文がない | `E0002`（既存の `expected a body expression`） |
| 1 つの関数の契約の行が 32 を超える | `E1017`（`a function takes at most 32 contract lines; split the function`） |
| `@cpu`・`export`・`private`・`rec`・`and` の関数、ジェネリックな関数 | 書ける |
| `@ensures` の結果の名前が `\(a, b) ->` などの模式・2 つ以上の引数・名前なし | `E1033` `contract_result_lambda` |

### 意味

| 時点 | 動き |
| --- | --- |
| 入口 | `@requires` の式を宣言順に評価。偽なら `TrapKind::ContractRequires` でトラップ（位置はその行の `@`）。引数は束縛として見える |
| 本体 | 通常どおり。自己末尾呼び出しの最適化は `@ensures` がなければ保たれる |
| 出口 | 本体の値に `@ensures` の名前を付け、宣言順に評価。偽なら `TrapKind::ContractEnsures`。**複数の `@ensures` が別の名前を使ってよい**（すべて同じ結果の値を指す） |
| 引数の見え方（出口） | `ref mut` と `mut` の引数は**本体が更新した後**の値。値渡しの引数を本体が移動していたら `E1012`（契約の式の位置）。配列・リストの引数を `@ensures` が読むと、`check` では本体の最後の使用が複製になる（O(n)。`off` では起きない） |
| 評価順・短絡・折り返し・トラップ | 通常の式と同じ。契約の式がトラップしたら、その種類のトラップ（契約の違反ではない） |
| 純粋性・型 | 契約の式は純粋で `bool`（A19 の効果の源の走査を式に適用する。`alloc`・`trap`・`copy`・`diverge` は許す）。違反は `E1033` `contract_impure`／`contract_not_bool` |
| 再帰・`@ensures` と末尾呼び出し | 再帰呼び出しのたびに入口・出口の検査が走る。`@ensures` を持つ関数の自己末尾呼び出しは末尾でなくなる（スタックが再帰の深さに比例。「現状 4」: `-O0` の `check` で約 10 万段で枯渇） |

### モードと CLI

`ContractMode`: `Off`・`Check`・`Test`・`Prove`（`--contracts off|check|test|prove`）。manifest の `[contracts]` は作らない。

| コマンド | 指定できる値 | 既定 |
| --- | --- | --- |
| `build` | `off`・`check`・`prove` | `off` |
| `run`・`script` | `off`・`check`・`prove` | `check` |
| `test` | `off`・`check`・`test`・`prove` | `check`（`test` で性質テストを生成） |
| `check` | `prove` | なし（何も生成しないので、契約は常に型検査される） |
| `repl` | （指定なし） | `check` |
| `doc`・`bench`・`fmt`・`api-diff`・`lsp` | 指定できない | `doc` は `check` と同じく型検査だけ。`bench` は `off` |

- `prove` は「検証を先に行い、そのあとは `check` として続ける」（生成物は `check` と同じ。**証明済みの検査を省かない**）。`--prove-timeout SECONDS`（1〜3600、既定 10）と `--function NAME`（`Module.name`。繰り返せる）は `prove` のときだけ使える。
- サブコマンド `tsuzuri prove [PATH] [--function NAME]... [--prove-timeout SECONDS] [--json]`（`--contracts prove` の `check` と同じ検証だけを行う）。
- 使えない組み合わせは `E2000`（終了コード 2。`--trap-info is only valid with build or run` と同じ形のメッセージ: `--contracts is only valid with build, run, test, or check`、`--contracts test is only valid with test`、`--prove-timeout and --function need --contracts prove or the prove command`）。`script` の選択肢の表（`script_arguments`・`script_json`・`SCRIPT_OPTIONS`）に `--contracts` を値つきの選択肢として足す。
- 環境変数 `TSUZURI_SMT`: ソルバーの実行ファイル（名前か経路）。なければ PATH の `z3`、なければ `cvc5`。見つからなければ `E1034`（kind `prove_solver_missing`）。

### 診断

メッセージは英語・小文字始まり・末尾にピリオドなし。`kind` は G22 が実装済みのときだけ付ける（未実装なら `Diagnostic::new` のまま）。位置は主スパン。例は付録 F。

| コード | kind | メッセージ | 主スパン |
| --- | --- | --- | --- |
| `E1033` | `contract_misplaced` | `a contract line must start a function body: write '@requires' and '@ensures' on their own lines right after '=' or '->', before the first statement` | `@` と属性名 |
| `E1033` | `contract_not_bool` | `a contract condition must be 'bool', but this is '{found}'; write a comparison such as 'x > 0'` | 条件の式 |
| `E1033` | `contract_impure` | `a contract must be pure: it {reason}{chain}; call only pure functions in a contract` | 効果の源の式。ほかの関数にあるときは、条件の中のその関数への最初の参照 |
| `E1033` | `contract_result_lambda` | `'@ensures' takes a lambda that names the result: write '@ensures \r -> condition'` | `@ensures` の次のトークン（なければ `@ensures`） |
| `E1033` | `contract_on_const` | `a 'const def' cannot have contract lines: the compiler evaluates it before the program runs; use a 'def' or remove the line` | 最初の契約の行 |
| `E1033` | `contract_ensures_reference_result` | `an '@ensures' needs the result as a value, but '{type}' holds a reference; remove the line or return an owned value` | その `@ensures` の行 |
| `E1017` | — | `a function takes at most 32 contract lines; split the function` | 33 行目 |
| `W1009` | `contract_always_true`・`contract_result_unused`・`contract_test_skipped` | 付録 F.3 | 条件（リテラル `true` のときだけ）／結果の名前／関数名 |
| `E1034` | `prove_solver_missing`・`prove_unsupported`・`prove_counterexample`・`prove_unknown`・`prove_solver_error` | 付録 F.2 のテンプレート | 最初の契約付き関数の名前／翻訳できない式／失敗する契約の行（トラップなら関数名）／関数名／関数名 |
| `E2005` | `contract_requires`・`contract_ensures` | 既存の `program terminated with ...`（一般の文言の `assert, or allocation` を `assert, a contract, or allocation` に直す。手順 12） | トラップの位置（契約の行） |

- `{reason}`・`{chain}`・`{found}`・`{type}` の書式は A19 の `E1029` と同じ（`{chain}` は ` (through 'A' -> 'B')`、最大 4 つ）。`{call}` は反例の呼び出し（`Main.clamp 3 (-1) 7`）、`{outcome}` は `returns 5`（関数の結果）か `traps: integer division by zero`。
- 1 つの関数につき `contract_impure` は最初の 1 件（ソース順）。`contract_not_bool` などは行ごとに 1 件。
- 各診断の `help`（G22 があるとき）: `contract_misplaced` = `move the line above the first statement of the function body`、`contract_not_bool` = `compare the value, for example 'x > 0'`、`prove_solver_missing` = `install z3 (https://github.com/Z3Prover/z3) or cvc5`。

### 出力の形

- **トラップ（`--trap-info`・`run`）**: `trap: contract requires failed at Main.tz:2:5` と `trap: contract ensures failed at Main.tz:3:5`（`<path>:<行>:<列>` は契約の行の `@` の位置）。`.trap.json` の `kind` は `contract requires failed`／`contract ensures failed`。
- **`tsuzuri prove`（text）**: 標準出力に関数ごと 1 行、順序は**モジュール名・宣言位置**。`proved Main.clamp`／`counterexample Main.bad`／`unsupported Main.sum_to (a 'while' loop)`／`unknown Main.hard`／`error Main.f (solver)`。診断（`E1034`）は標準エラー。終了コードは、選んだ関数がすべて `proved` なら 0、それ以外 1、引数の誤りは 2。
- **`tsuzuri prove --json`**: 標準出力が JSON Lines（関数ごと 1 行と最後に summary 1 行。キーの順序は固定で、時間を入れない＝決定的）。形は付録 C.9。診断は標準エラーに既存の JSON。

### 上限と決定性

契約の行 32／関数。条件の式の深さは既存の 128。`prove`: 呼び出しの展開の深さ 8、SMT の定義 50,000 個（超えたら `E1017`）、1 問あたりの時間 `--prove-timeout`（既定 10 秒。ソルバーの `-t` と、壁時計 +5 秒の強制終了）、ソルバーの出力 1 MiB。SMT の台本・名前・出力の順序は決定的（`BTreeMap`、宣言順）。

### edition・互換性

`@requires`／`@ensures` は今 `E0002` なので、変わるのは「エラーを受理にする」拡張だけで、すべての edition で同じ（`EDITION_KEYWORDS` は変えない）。`docs/stability.md` に沿う追加: 診断コード `E1033`・`E1034`・`W1009`、`--contracts`・`--prove-timeout`・`--function`・`prove`、`.trap.json` の `kind` の文字列（既存は不変）、`FRONTEND_FORMAT` の増加（キャッシュの再構築だけ）。IR・ABI・WASM の import: `off` は不変、`check` は検査コードが増えるだけで import は増えない。

## 設計

### データ構造

```rust
// src/syntax.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractKind { Requires, Ensures }

#[derive(Clone, Debug)]
pub struct ContractClause {
    pub kind: ContractKind,
    /// `@ensures \r -> ..`: the name of the result (`_` for `\_`). `None` for `@requires`.
    pub binder: Option<Ident>,
    pub condition: Expr,
    /// The condition's source text with line breaks and the indentation after them collapsed
    /// to one space; docgen, hover, api-diff, and facts print it. The formatter's AST
    /// fingerprint clears it.
    pub text: String,
    /// From the `@` to the end of the condition.
    pub span: Span,
}
// FunctionDecl gains `pub contracts: Vec<ContractClause>` after `body`.

// src/contracts.rs (new)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractMode { Off, Check, Test, Prove }

/// How a function's woven body is laid out (the counts of its contract lines).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContractShape { pub requires: u8, pub olds: u8, pub ensures: u8 }

// src/check.rs: CheckedFunction gains `pub(crate) contracts: contracts::ContractShape`
// (`Default` at the 10 other literal sites; the specializer's `template.clone()` carries it).
```

型付き IR の**織り込んだ形**（`ContractShape` が数を持つ）:

```text
Block {
  bindings: [ (_ , Call($requires, [cond_i]))  × requires      // 入口。宣言順
              (old_k, <typed e_k>)             × olds          // Phase 6
              (result, <typed original body>)  × 1 if ensures > 0
              (_ , Call($ensures, [cond_j]))   × ensures ],    // 出口。宣言順
  result: if ensures > 0 { Local(result) } else { <typed original body> }   // 本体は末尾位置のまま
}
```

`Builtin::ContractRequires`（名前 `"$requires"`）・`Builtin::ContractEnsures`（名前 `"$ensures"`）: 型 `bool -> unit`（`Assert` と同じ）。`$` は字句にならないので利用者は書けない。

### 新規ファイル・主な関数

| ファイル | 内容 |
| --- | --- |
| `src/contracts.rs`（新規。`src/check.rs` に `#[path = "contracts.rs"] pub mod contracts;`。`recursion` の隣） | `ContractMode`・`ContractShape`・`strip`・`validate_purity`・（Phase 3）`generate_tests`・（Phase 4）`warnings`、`impl Checker` の織り込み（`contracted_body`・`contract_condition`・`contract_call`）。`use super::*;` で `Checker`・`Names` を使う（`pure_rules` と同じ方式）。`main.rs` は `tsuzuri::check::contracts::ContractMode` で使う |
| `src/parse_control.rs` | `Parser::contract_ahead`・`contract_clauses`・`contract_clause` |
| `src/prove.rs`・`src/prove_smt.rs`（新規。Phase 5） | `prove::run`・SMT の台本の生成・ソルバーの起動・反例の復元（付録 C） |
| `tests/contracts.rs`・`tests/contracts_property.rs`・`tests/prove.rs`（新規）、`tests/fixtures/contracts/Main.tz`（新規） | 「テスト計画」 |

### 織り込み（`Checker::contracted_body`）

関数の本体の検査（`check_modules_collect` の `checker.expression(&function.body, Some(&signature.result))` の行。`extern` でない関数）を、`function.contracts` が空でなければ `checker.contracted_body(function, &signature.result)?` に替える。その前に、`computation::expand(&mut function.body, ..)` の直後で各 `clause.condition` にも `computation::expand` をかける。戻り値は `(TypedExpr, ContractShape)`。

1. `bindings = []`、`shape = default`。
2. `@requires` の行を**ソース順**に: `condition = contract_condition(clause)?`（下）→ `bindings.push(contract_call(ContractRequires, condition, clause.span)?)`、`shape.requires += 1`。
3. （Phase 6）`@old` の持ち上げ。
4. `body = self.expression(&function.body, Some(expected))?`。
5. `@ensures` がなければ、`Block { bindings, result: body }`（`ty = body.ty`、`span = body.span`。`bindings` が空なら `body` をそのまま返す）。
6. あれば、結果の型が参照を含む（`self.inference.resolve(expected).contains_reference()`）なら `contract_ensures_reference_result`。`result = self.bind(&Ident { text: "$contract.result", provenance: Generated, span: function.name.span }, body.ty.clone(), false)` を作り、`bindings.push((result.clone(), body))`。
7. `@ensures` の行をソース順に: `self.scopes.push(BTreeMap::new())`、`binder` が `_` でなければ `self.scopes.last_mut().unwrap().insert(binder.text.clone(), result.clone())`（**同じ `Local` への別名**。複数の `@ensures` が別の名前でよい理由）、`condition = contract_condition(clause)`、`self.scopes.pop()`（エラーでも pop する）、`bindings.push(contract_call(ContractEnsures, condition, clause.span)?)`、`shape.ensures += 1`。
8. `Block { bindings, result: Local(result) }` を返す（`ty = result.ty`、`span = function.body.span`）。

- `contract_condition(clause)`: `condition = self.expression(&clause.condition, None)?`、`self.same(&condition.ty, &Type::Bool, clause.condition.span)` が失敗したら `E1033` `contract_not_bool`（`{found}` は `self.inference.resolve(&condition.ty).display(&self.types)`）。
- `contract_call(builtin, condition, span)`: `callee = TypedExpr { kind: TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance { builtin, types: Vec::new() })), ty: Type::function(vec![Type::Bool], Type::Unit), span }`（`entry_result`（`src/check.rs`）が同じ形で組み込み関数の callee を作る）、`call = TypedExpr { kind: Call(Box::new(callee), vec![condition]), ty: Type::Unit, span }`、`local = self.bind(&Ident { text: "_", provenance: Generated, span }, Type::Unit, false)`。戻り値 `(local, call)`。

### モードと剥がし（`ContractMode`・`strip`）

`check_modules_indexed_all`・`check_modules_collect`・`analyze_inputs_with`・`Project::analyze_cached` に引数 `contracts: ContractMode` を足す（`check_modules_all`・`analyze_inputs_all`・`analyze_inputs_indexed_all`・`analyze_inputs_semantic` と `analyze*` の公開関数は `Check` を渡す。契約のないプログラムでは何も変わらない。契約を剥がした結果を見たいテストは新しい `analyze_modules_with_contracts(sources, mode)` を使う）。`main.rs` は `Check` の検査（`tsuzuri check`・`doc`・LSP）と、コマンドごとのモード（Phase 1 は `Off` を直書き）を渡す。

- **剥がす位置**: `check_modules_collect` の `polymorph::specialize` の**直前**（`infer_copy_all` の後）に、`contracts == Off` なら `for function in &mut module.functions { contracts::strip(function) }`。型検査・`finish`・網羅性・未使用の警告・`recursion::check`・`infer_copy_all` はすべて織り込んだ形で済んでいる。特殊化・`closures::lower`・所有権・A19／A20 の検査は**剥がした形**を見る。
- `strip(function)`: `shape` が空なら何もしない。`function.body.kind` が `Block { bindings, result }`（織り込みが作った形。違えば `unreachable!`。同じ pass が作った不変条件）。`ensures == 0` なら元の本体 = `*result`、そうでなければ `bindings[requires + olds].1`。`function.body = 元の本体`、`function.contracts = ContractShape::default()`。冪等。
- **理由**: 特殊化の番号 `$mono.N` は要求された順の通し番号で、契約の式が先にジェネリックな関数を呼ぶと本体の呼び出しの番号がずれる（実測: 本体の `Array.sum` を `[i64]` で呼ぶだけなら `$mono.2`、その前に `[i32]` の `Array.sum` を呼ぶと本体のほうが `$mono.3`。付録 A.3）。特殊化の後で剥がすと IR が 1 byte で一致しない。
- **一貫性のための 2 回の検査**: `analyze_inputs_with` は、`contracts == Off` で、いずれかのユーザーのモジュールに契約の行があるときだけ、先に `Check` で `check_modules_indexed_all` を呼び、`Err` ならそれを返し、`Ok` の module は捨てる（診断の再現のため、2 回目と同じ入力・同じ並列度）。そのあと `Off` で呼ぶ。特殊化の後に走る検査（所有権・A19／A20・契約の純粋性）を `off` でも通すため。契約を含まないプロジェクトの費用は増えない。
- **A20 との関係**: 効果の要約は `Check` の検査で織り込んだ形から求める。`@no_trap` の関数に契約の行があれば、`build`（`off`）でも A20 の `E1031`（上の 2 回目でなく 1 回目の検査で出る）。

### 純粋性（`contracts::validate_purity`）

`check_modules` で `pure_rules::validate` の直後に `diagnostics.extend(contracts::validate_purity(&module))`。`shape` が空でない関数ごとに、織り込んだ `Block` から条件の式を取り出し（`bindings[i].1` の `Call` の第 1 引数。位置は上の形）、`pure_rules` の走査を**式の根**に適用して最初の効果の源を求める。A19 の `pure_rules.rs` に次を足す（A19 の既存の判定は変えない）:

1. 局所の走査を `scan_expression(root: &TypedExpr, ..) -> Scanned` に切り出し、`scan_function` はそれを本体に適用する。
2. `pub(crate) fn first_effect_of(module: &CheckedModule, root: &TypedExpr) -> Result<Option<Violation>, Diagnostic>`（`Violation { span: Span, reason: String }`。`reason` は A19 の `{reason}{chain}`）。根の辺から幅優先で到達を探す（A19 の手順 3 と同じ。上限も同じ 4,096 関数・2,097,152 式、超えたら `E1017`）。
3. **契約の 2 つの組み込み関数は、走査が引数へ降りない**（`Call` の callee が `Function(Builtin(ContractRequires | ContractEnsures))` なら子を積まない）。`builtin_effect` では純粋（`trap` は不純ではない）。これで、不純な契約が `pure def` 自体の `E1029` と二重に報告されない。
4. 位置: 効果の源が条件の式（とその中のラムダ）にあればその式、ほかの関数にあれば条件の中でその関数へ最初に降りる式。

A20 の `src/effects.rs`（A20 で新規）が先に実装されているときは、`pure_rules` の代わりに A20 の `scan_expression`（A20 手順 4）を使う。`effects.rs` に `pub(crate) fn first_effect_in(module: &CheckedModule, effects: &FunctionEffects, root: &TypedExpr, forbidden: EffectSet) -> Option<(Span, Effect, Vec<(usize, Span)>)>` を足す。`root` の局所の走査で `forbidden` と交わる直接の源があればその span、なければ参照辺 `(callee, span)` を span 順に見て `effects.summary(callee).effects` が交わる最初の辺の span と `effects.chain(callee, e)` を返す。`validate_purity` は `effects::summarize(module, Options::new(Scope::All, PURE_FORBIDDEN))` を 1 回呼び（`Options` の構造体のリテラルは書かない。E15 が `trusted` を足す。E15 の Phase 4 のあとは `@trusted` の `pure` の性質を `pure def` と同じに契約でも許す）、`forbidden = PURE_FORBIDDEN`（`trap`・`alloc`・`copy`・`diverge` は許す）。契約の 2 つの組み込み関数は A20 の分類表（付録 B）に `trap` として足し（D-53）、走査は引数へ降りない。

### Phase 2 以降の設計（要点。詳細は付録）

- **test runner の trap 行**（手順 13）: `TestRunnerOptions` に `trap_sources: Option<&'a [TrapSource<'a>]>`、`TestOptions` に `trap_sources: Vec<(String, String)>`（path, text）。`emit_test_runner_with` は `Some` のとき `Instrumentation { traps: true, .. }` で出し、`traps::instrument(..)` を通して `EmitOutput.ir` を返す（`emit_with_trap_info` と同じ。`TestRunnerOptions.debug` はデバッガ用の別の項目）。native の runner だけ（WASM は従来どおり位置なし）。`run_test_action` は**契約付き関数があるとき**だけ `project.sources` から詰める。
- **Phase 3**（付録 D・E）: `Gen.for_all_where`・`Gen.check_where`、組み込みクラス `Arbitrary`（引数なしのメソッド `arbitrary`。呼び出しは `Arbitrary.arbitrary()`）、`DeriveClass::Arbitrary`、`contracts::generate_tests`。
- **Phase 5**（付録 C）: 織り込んだ型付き IR を、SSA 形の `QF_BV` の台本（値は `Bool` と `(_ BitVec n)` だけ）に翻訳する。問い 1 つ = 契約の行 1 つ。
- **Phase 6**（付録 H）: `@old (式)` は parser が `Call(Name("$old"), [式])` にし、織り込みが入口の束縛へ持ち上げる。

## 前提とする他チケットのインターフェース

| 相手 | 使う名前・形 | 未実装のとき |
| --- | --- | --- |
| A19（必須） | `src/pure_rules.rs`（A19 で新規）の `validate`・`Scanned`・`builtin_effect`・`Limits`、`E1029` の `{reason}{chain}` の書式、`pure def` | 停止条件 1 |
| A20 Phase 1（任意） | `src/effects.rs`（A20 で新規）の `Effect`・`EffectSet`・`summarize`・`Options`・`Scope`・`scan_expression`・分類表。契約の織り込み（`$requires`・`$ensures`）は D-53 のとおり `trap` 効果の源。**効果の要約は織り込んだ形（モードに依存しない）から求める**ので、`@no_trap` の関数に契約の行があれば、どのモードでも A20 の `E1031` になる（「落とし穴」） | `pure_rules` だけで実装（手順 6） |
| G22（任意） | `Diagnostic::with_kind(&'static str)`・`related(RelatedRole, impl Into<String>, Span)`・`with_help(impl Into<String>)`・`RelatedRole::RequiredBy`・`ViaCall`（D-51。名前・型は変えない）。`contract_impure` は、契約の行へ `related(RelatedRole::RequiredBy, "required by this contract line", ..)` を付ける | kind・help・related を付けない（`Diagnostic::new` のまま。テストは `code` だけ照合する） |
| G23（任意） | `Diagnostic::of(DiagnosticKind::.., ..)` と kind の登録簿 | G23 が実装済みなら、「診断」の表の kind（`E2005` の `contract_requires`・`contract_ensures` を含む）を G23 の「新しい kind を足す手順」（登録簿へ登録し、発行箇所を `Diagnostic::of` にする）で足す |
| G26（任意） | facts の行の `contracts` キー（追加のキーは読み手が無視できる。D-52） | Phase 4 の facts の手順を飛ばす |
| E14 | `--trap-mode return` の `tsuzuri_try_<name>` は契約のトラップを `trap.kind` 16・17 で返す。追加の作業なし | — |

## 実装手順

各手順の最後で `cargo build` と `cargo test --locked` が緑。**手順は番号順に 1 段ずつ**。`/tmp/tz-A21/...` は再現用の一時ディレクトリの例（リポジトリには置かない）。

### Phase 1: 構文と静的検査

### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`mkdir -p /tmp/tz-A21/before && cp target/release/tsuzuri /tmp/tz-A21/tsuzuri-before`。`tests/fixtures/*/` と `examples/*/` の各プロジェクトを `tsuzuri-before build <dir> --emit llvm -O0 -o /tmp/tz-A21/before/<名前>.ll` で出す（出せないものは一覧に残す）。「現状」の再現コマンドを作り直す。
- 確認: すべて通り、「現状」の出力と一致する。`git ls-files src/pure_rules.rs` が 1 行（A19 done）。`git grep -n "E1033\|E1034\|W1009" -- src` が空。

### 手順 2: 構文の型・codec・整形・`FRONTEND_FORMAT`

- 変更: `src/syntax.rs`・`src/parser.rs`・`src/check.rs`・`src/polymorph.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`。
- 内容: 「データ構造」の `ContractKind`・`ContractClause`・`FunctionDecl.contracts` を足す。コンパイラが指す `FunctionDecl` の直接構築 7 か所（`src/check.rs` 4・`src/parser.rs` 2・`src/polymorph.rs` 1）に `contracts: Vec::new()`。`src/syntax_codec.rs`: `unit_enum!(ContractKind { Requires, Ensures });`、`impl Wire for ContractClause`（`put`・`get`: `kind`・`binder`・`condition`・`text`・`span` の順）、`FunctionDecl` の `put` は `if out.full() { body.put(out)?; contracts.put(out)?; }`、`get` は `body` の次に `contracts: Vec::get(input)?`、単体テストの数の表に `("ContractKind", 2)`。`FRONTEND_FORMAT` を**現在の値 + 1**（HEAD の 4 → 5。別のチケットが先に上げていたらその値 + 1）。`src/formatter.rs` の `canonicalize` の関数のループに、clause ごとの `span = Span::default()`・`text.clear()`・`binder` の `canonical.ident`・`canonical.expression(&mut clause.condition)` を足す。
- 確認: `cargo test --locked` が手順 1 と同じ件数で通る。契約を書かない入力の IR は不変（手順 1 の `.ll` と `diff` が空。手順 7 でまとめて確認）。

### 手順 3: parser（先に失敗するテストを書く）

- 変更: `src/parser.rs`・`src/parse_control.rs`・`tests/contracts.rs`（新規）。
- 内容: 先に「テスト計画」の parser のテスト（`parses_requires_and_ensures_on_lambda_and_fn_bodies` ほか）を書いて落とす。実装: (a) `Parser` に `contract_site: Option<usize>`・`site_contracts: Vec<ContractClause>`。(b) `parse_control.rs` に `contract_ahead`（現在が `TokenKind::At`、次が `Ident("requires" | "ensures")`）・`contract_clause`（`@` と名前を食べ、`ensures` なら `Backslash`・識別子（`_` を含む）・`Arrow` を読み、条件を `self.enter()?; self.expression_inner(0, true, true)?; self.nesting -= 1` で読み、`text` は `self.source` の `condition.span` の範囲の空白の折り畳み、`span = at.through(condition.span)`）・`contract_clauses`（`contract_ahead` の間繰り返す。最初の行が `newline_before_current()` でなければ `contract_misplaced`、33 個目は `E1017`）。(c) `body_expression` の先頭: `if self.contract_site == Some(self.position) && self.contract_ahead() { self.contract_site = None; self.site_contracts = self.contract_clauses()?; }`。(d) `explicit_lambda`: 入口で `let at_site = self.contract_site == Some(self.position)`、`->` を食べた後・`body_expression` の前に `self.contract_site = at_site.then_some(self.position)`。(e) `Parser::definition`（`fn f x = ..`）の `self.take()`（`=`）の直後と、`program_all` の `def .. = <本体>` の本体を読む行の直前に `self.contract_site = Some(self.position)`。(f) `program_all` の宣言ループ: 各反復の先頭で `self.contract_site = None; self.site_contracts.clear();`、クロージャが返ったあとに `site_contracts` が空でなければ `definitions.last()` の名前で `by_function: BTreeMap<String, Vec<ContractClause>>`（ループの外）へ移す。定義と署名を組にした `Ok(function)` で `function.contracts = by_function.remove(&function.name.text).unwrap_or_default()`、`function.constant` で空でなければ `contract_on_const`（最初の行の位置）。(g) `checked_expression`: `@` の次が `requires`／`ensures` なら `E1033` `contract_misplaced`（それ以外の `@名前` は従来の `E0002`）。
- 確認: `cargo test --locked --test contracts` の parser のテストが通る。`cargo test --locked --lib parser` と `--test frontend --test diagnostics --test formatter` が従来どおり。`syntax_codec` の tag 網羅のテストに契約つきのソースを足して通る。`tsuzuri fmt` が契約の行を整形し、`--check` が冪等（`formats_contract_lines_and_round_trips`）。

### 手順 4: 組み込み関数 `$requires`・`$ensures`

- 変更: `src/check.rs`・`src/llvm.rs`・`src/pure_rules.rs`（A19。分類表に 2 行）・`tests/contracts.rs`。
- 内容: `git grep -n "Builtin::Assert" -- src` の 5 か所（`src/check.rs` の `enum Builtin`・`Builtin::ALL`・`name`・`signature`、`src/llvm.rs` の `emit_builtin`）の隣に `ContractRequires`・`ContractEnsures` を同じ形で足す（`name` は `"$requires"`・`"$ensures"`、`signature` は `(vec![Concrete(Type::Bool)], Concrete(Type::Unit), Vec::new())`、`emit_builtin` は `Assert` の arm と同じ本体）。`pure_rules::builtin_effect` に 2 つを純粋として足し、A19 が足す単体テスト `every_builtin_has_a_verdict` の期待を直す（純粋 156 → 158、不純 90 のまま、`Builtin::ALL` 246 → 248）。A20 が実装済みなら `effects.rs` の分類表（付録 B）に `trap` として足す。この段では `TrapKind` を足さない。
- 確認: `Builtin::ALL.len() == 248`。`describes_every_builtin_with_a_parameterized_scheme`・`std_definitions_never_shadow_builtin_functions`・`pure_rules` の単体テストが通る。`$requires` を利用者の関数名に使えない（字句にならない）ことは `tests/contracts.rs::builtin_names_cannot_be_written`（`$requires` は E0002 の字句エラー）。

### 手順 5: 織り込み（型検査）

- 変更: `src/check.rs`（`#[path = "contracts.rs"] pub mod contracts;`・`CheckedFunction.contracts`・直接構築 11 か所に `contracts: contracts::ContractShape::default()`、`check_modules_collect` の関数の検査の行）・`src/contracts.rs`（新規）・`tests/contracts.rs`。
- 内容: 「織り込み」の 8 手順のとおり `ContractShape`・`Checker::contracted_body`・`contract_condition`・`contract_call` を書く。関数の検査のループは `computation::expand` の後で各 `function.contracts[i].condition` を `computation::expand` し、`function.contracts` が空でなければ `contracted_body`、戻りの `ContractShape` を `CheckedFunction.contracts` に入れる（特殊化の `template.clone()` が引き継ぐ。`closures.rs`・`closures_rec.rs`・`polymorph.rs` の生成した補助関数は `default()`）。
- 確認: `tests/contracts.rs` の織り込みのテスト（`weaves_requires_before_and_ensures_after_the_body`・`ensures_names_alias_the_one_result`・`contract_not_bool_is_rejected`・`ownership_rules_apply_inside_contracts`・`generic_functions_carry_contracts_into_every_instance`）が通る。契約のないプログラムの `CheckedModule` は不変（手順 1 の IR と同一）。

### 手順 6: 純粋性（`contract_impure`）

- 変更: `src/pure_rules.rs`（A19。`scan_expression` の切り出し・`first_effect_of`・契約の 2 つの組み込み関数の規則）・`src/contracts.rs`（`validate_purity`）・`src/check.rs`（`pure_rules::validate` の直後の呼び出し）・`tests/contracts.rs`。A20 が実装済みなら同じ 3 点を `src/effects.rs` に足す。
- 内容: 「純粋性」の 1〜4 のとおり。A19 が足すテスト（A19 で新規の `tests/pure_functions.rs`）の期待値は変えない。
- 確認: `tests/contracts.rs` の `contract_impure_*`（`IO.write_line`・`extern`・`Atomic`・不純な呼び先・ラムダの中・`pure def` でない純粋な関数は許す）が `E1033` で通る。`cargo test --locked --test pure_functions` が従来どおり。**停止条件 5**: A19 のテストの期待値を変える必要が出たら止める。

### 手順 7: `ContractMode`・`strip`・配線（IR 不変）

- 変更: `src/contracts.rs`（`ContractMode`・`strip`）・`src/check.rs`（`check_modules_all`・`check_modules_indexed_all`・`check_modules_collect` の引数 `contracts`、`polymorph::specialize` の直前の剥がし）・`src/warnings.rs`（`check_modules_collect` を呼ぶ単体テスト 1 か所に `ContractMode::Check`）・`src/lib.rs`（`analyze_inputs_with_contracts`・`analyze_modules_with_contracts`）・`src/driver.rs`（`Project::analyze_cached` の引数）・`src/main.rs`（`analyze_cached` の呼び出し 1 か所）・`tests/contracts.rs`。
- 内容: 「モードと剥がし」のとおり。(a) `check_modules_collect` の引数 `contracts: ContractMode` を最後に足し、`ownership::infer_copy_all` の後・`polymorph::specialize` の前に `if contracts == ContractMode::Off { for function in &mut module.functions { contracts::strip(function); } }`。`check_modules_indexed_all` も同じ引数を最後に足し、`check_modules_all` は `ContractMode::Check` を渡す。(b) `lib.rs`: 今の `analyze_inputs_with` の本体を `analyze_inputs_with_contracts(inputs, semantic, cache, parallelism, contracts)` に移し、`analyze_inputs_with(..)` は `ContractMode::Check` でそれを呼ぶ 1 行にする（既存の呼び出し元は変えない）。本体の最後の `check::check_modules_indexed_all(&modules, semantic, parallelism)` の前に、`contracts == Off` かつ `programs` のどれかの `program.functions` に `!f.contracts.is_empty()` があるときだけ `check::check_modules_indexed_all(&modules, None, parallelism, ContractMode::Check)?;`（戻りの module は捨てる）。`pub fn analyze_modules_with_contracts(sources: &[(&str, &str)], mode: ContractMode) -> Result<CheckedModule, DiagnosticSet>`（`analyze_modules_with_std_all` と同じ入力の組み立てで、最後が `analyze_inputs_with_contracts(&inputs, None, None, Parallelism::library(), mode)`）。(c) `driver.rs`: `analyze_cached(&self, cache, kind, contracts)` が `analyze_inputs_with_contracts` を呼ぶ。`main.rs`: Phase 1 は `Action::Check`・`Action::Doc` が `Check`、ほかは `Off` を直書き（`--contracts` は手順 10 から）。
- 確認: `tests/contracts.rs::off_ir_equals_the_ir_without_contract_lines`: 契約つきの関数 20 個（引数が配列・文字列・レコード・ジェネリック・`ref mut`・自己再帰・`@cpu`・`export`・ラムダを含む本体）を、契約の行だけ消した版と並べ、`analyze_modules_with_contracts(.., Off)` → `llvm::emit(&module, Entry::Library)` の文字列が**完全一致**。`off_checks_the_woven_form_first`: 値渡しの `string` を `@ensures` が読む前に本体が移動する関数が、`Off` でも `E1012`。`tests/fixtures`・`examples` の全プロジェクトの `--emit llvm -O0` が手順 1 の `.ll` と `diff` で空。`cargo test --locked` が緑。

### 手順 8: 文書と全体の確認（Phase 1）

- 変更: 「ドキュメント」の Phase 1 の行。
- 内容: `attributes.md`・`functions.md`・`diagnostics.md`・`docs/language.md`・`docs/architecture.md`・`docs/stability.md`・`README.md` を更新（Phase 1 では `--contracts` はまだなく、契約は検査されるだけ、と書く）。`node scripts/check-docs.mjs` を変えたページに。
- 確認: GUIDE §3.1 の 4 つの回帰テスト、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`（件数 = 手順 1 + 新規）、`node scripts/check-docs.mjs <ページ>` が通る。

### Phase 2: 実行時検査（`off`／`check`）

### 手順 9: `TrapKind` を 2 つ足す

- 変更: `src/trap.rs`・`src/llvm_traps.rs`・`tests/trap_locations.rs`。
- 内容: 付録 B の表のとおり。`enum TrapKind` の**末尾**（`Overflow` の後）に `ContractRequires`・`ContractEnsures`、`TrapKind::ALL` を `[Self; 18]`、`description()` に `contract requires failed`・`contract ensures failed`、`TrapKind::diagnostic_kind(self) -> Option<&'static str>`（契約の 2 つだけ `Some("contract_requires")`・`Some("contract_ensures")`）。`llvm_traps::runtime_kind` の `builtin.assert` の分岐の前に `function.contains("builtin.$requires")` → `ContractRequires`、`function.contains("builtin.$ensures")` → `ContractEnsures`。`src/runtime/trap.c`・`trap-boundary.mjs`・`test-runner.mjs`・`bindings_native.rs` は変更なし（kind は不透明な `u32`、または `ALL` から自動）。
- 確認: `tests/trap_locations.rs::contract_traps_report_their_kind_and_the_clause_position`（native で標準エラーが `trap: contract requires failed at <path>:2:5`、`.trap.json` の `kind`、WASM で `site.kind`）が通る。`TrapKind::Overflow as u32 == 15`・`TrapKind::ContractEnsures as u32 == 17`。`git grep -n "urem i32 %offset" -- tests` が 0 件（site id の幅 16→18 に依存するテストはない）。既存の trap のテストが緑。

### 手順 10: `--contracts` の引数解析

- 変更: `src/main.rs`（`Arguments.contracts: Option<ContractMode>`・`parse_arguments`・`script_arguments`・`script_json`・`SCRIPT_OPTIONS`・`HELP`・`Arguments` の直接構築 2 か所）・`README.md`（CLI の表）。
- 内容: 仕様の「モードと CLI」の表と `E2000` の 3 つのメッセージ。重複指定は `contracts specified more than once`。Phase 2 は `off`・`check` だけ受理し、`test`・`prove` は `--contracts {mode} is not available yet`（Phase 3・5 で外す）。`--trap-info` の検査の隣に置く。
- 確認: `main.rs` の単体テスト `parses_contracts_option`（各コマンドの受理・拒否・重複）と `rejects_ambiguous_or_unused_arguments` への追加。

### 手順 11: モードの選択

- 変更: `src/main.rs`・`tests/contracts.rs`。
- 内容: `mode = arguments.contracts.unwrap_or(既定)`（`Build`・`Bench` は `Off`、`Run`・`Test` は `Check`、`Check`・`Doc` は常に `Check`）を手順 7 の `analyze_cached` の引数にする。`repl.rs`・LSP・`publish` は既存の `analyze_inputs_*`（`Check`）のままで変更なし。
- 確認: `tests/contracts.rs::default_modes_follow_the_command`（`build --emit llvm` の IR に `$requires` がなく、`--contracts check` ではある。`run` は契約違反で `E2005`、`run --contracts off` は通る）。

### 手順 12: `E2005` の kind と文言

- 変更: `src/driver.rs`（`run_captured`）・`tests/` と `docs/`・`_tsuzuri/` の該当文言。
- 内容: 一致した `TrapSite` の `span` と `kind` を保ち、G22 があれば `.with_kind(kind.diagnostic_kind()?)`。`E2005` の一般の文言 `integer division, indexing, assert, or allocation may have trapped` を `integer division, indexing, assert, a contract, or allocation may have trapped` に直す（`git grep -n "indexing, assert, or allocation" -- src tests docs _tsuzuri README.md` の全出現）。
- 確認: `tests/contracts.rs::run_reports_the_contract_line_as_e2005`（`--json` の `E2005` の `span` が契約の行）。文言の変更を `git grep` で 0 件にしてから `cargo test --locked`。

### 手順 13: test runner の trap 行

- 変更: `src/llvm.rs`（`TestRunnerOptions.trap_sources`・`emit_test_runner_with`）・`src/test_runner.rs`（`TestOptions.trap_sources`・`build_runner`）・`src/main.rs`（`run_test_action`）。
- 内容: 「Phase 2 以降の設計」の test runner の項。native の runner だけ。**契約付きの関数が 1 つでもあるときだけ**詰める（`module.functions.iter().any(|f| f.contracts != ContractShape::default())`。`off` では `strip` が `ContractShape` を空にするので何も変わらない）。`TestOptions` と `TestRunnerOptions` の直接構築（`Default` を含む）に新しい項目を足す。
- 確認: `tests/contracts.rs::test_runner_prints_the_contract_trap_line`（`test` の失敗の下に `trap: contract ensures failed at ..` が出る）。契約のない runner の IR が不変（手順 1 の fixture の `emit_test_runner_with` の出力が同一）。

### 手順 14: E2E と文書（Phase 2）

- 変更: `tests/features.mjs`・`tests/fixtures/contracts/Main.tz`（新規）・文書（「ドキュメント」の Phase 2 の行）。
- 内容: `run(name, suite)` の `cli(["build", ..])` のうち 5 か所（`--emit header`・`--emit llvm` の 2 つ・`--target` の WASM の通常と `--trap-info` の 2 つ）に `...(suite.options || [])` を足す（`recursive_types` 専用の 1 か所と `cli(["check", ..])` は除く）。suite `contracts` を足す（`options: ["--contracts", "check"]`、fixture と期待値は付録 G）。`traps` の第 3 要素の kind（`[trap, args, kind = suite.trapKind]`）は既存で、WASM の `--trap-info` の `.trap.json` だけが確かめる。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri contracts`（native と WASM の `-O0`／`-O3`、`live == 0`、IR の決定性、WASM の import なし）。`node scripts/check-docs.mjs <ページ>`。

### Phase 3〜6（手順 15〜34。各手順の「変更・内容・確認」は付録 H）

| 手順 | Phase | 変更の要点 | 確認の要点 |
| --- | --- | --- | --- |
| 15 | 3 | `std/Gen.tz`: `Gen.for_all_where`・`check_where` | `tests/property.rs` に `where` の縮小・捨てる比率 |
| 16 | 3 | 組み込みクラス `Arbitrary`（`polymorph.rs`・`stdlib.rs` の `OPT_IN`）と std の instance | `Arbitrary.arbitrary()` で `i64`・配列・組 |
| 17 | 3 | `deriving (Arbitrary)`（`syntax.rs`・`parser.rs`・`syntax_codec.rs`・`derive.rs`） | レコード・union の生成と縮小、再帰型は `E1025` |
| 18 | 3 | `contracts::generate_tests`（双子の関数と `TestDecl`）・`analyze_inputs_with` の引数 | 生成した試験の一覧と反例 |
| 19 | 3 | `--contracts test` の解禁と `W1009`（`contract_test_skipped`） | `tsuzuri test --contracts test` が反例を縮小して出す |
| 20 | 3 | 文書 | `check-docs` |
| 21〜24 | 4 | `W1009` の 2 種・docgen・hover・`api-diff`・facts | 各ツールの出力 |
| 25〜31 | 5 | `src/prove.rs`・`src/prove_smt.rs`・`Action::Prove`・差分テスト | z3 の `proved`・反例・時間切れ、偽の solver |
| 32〜34 | 6 | `@old (式)` | `old` の持ち上げと `E1033` の kind |

## テスト計画

期待値は独立な参照から作る（手計算・JavaScript の BigInt・native の実行・z3）。コンパイラの現在の出力を写さない。拒否のテストは診断コードを照合し、メッセージ全文は固定しない（決めたものを除く）。

**`tests/contracts.rs`（Phase 1・2。手本 `tests/constants.rs`、A19 で新規の `tests/pure_functions.rs`）**

| テスト | 入力 | 期待値 |
| --- | --- | --- |
| `parses_requires_and_ensures_on_lambda_and_fn_bodies` | 現状 1 の `clamp`、`fn clamp lo hi x =`、`\a -> \b ->`、引数なしの `def` | `contracts` が `[Requires, Ensures]`、`text == "lo <= hi"`、`binder == Some("r")` |
| `keeps_requires_as_an_identifier` | `let requires = 1`・`def ensures :: i64 -> i64` | 従来どおり受理して実行 |
| `rejects_misplaced_contract_lines` | 同じ行・本体の文の後・`{ }`・引数・`test`・`instance` のメソッド・トップレベルの `@requires` | `E1033`（`@checked` 以外の `@名前` は従来どおり `E0002`） |
| `rejects_contract_on_const_and_bad_result_lambdas` | `const def` の契約、`@ensures \(a, b) -> ..`・`@ensures r -> ..`・`@ensures` だけ | `E1033` |
| `formats_contract_lines_and_round_trips` | `@requires   lo<=hi` | `@requires lo <= hi`、`--check` が冪等 |
| `weaves_requires_before_and_ensures_after_the_body` | `clamp` | `Block` の `bindings` が `requires, result, ensures` の順。`requires` だけなら本体が末尾位置 |
| `ensures_names_alias_the_one_result` | `@ensures \r -> ..`・`@ensures \value -> ..` | 受理。両方が同じ値を見る |
| `contract_not_bool_is_rejected` / `ownership_rules_apply_inside_contracts` | `@requires x`（i64）／値渡しの `string` を移動して `@ensures` が読む | `E1033`／`E1012` |
| `generic_functions_carry_contracts_into_every_instance` | `def first :: ['a] -> 'a` に `@requires xs.length > 0`。`i64`・`string` で呼ぶ | 両方の特殊化が `ContractShape { requires: 1, .. }` |
| `contract_impure_*` | `IO.write_line`・`extern`・`Atomic`・不純な呼び先・ラムダの中／純粋な `def` | `E1033`／受理 |
| `off_ir_equals_the_ir_without_contract_lines` | 20 個の関数（配列・文字列・レコード・ジェネリック・`ref mut`・自己再帰・`@cpu`・`export`・ラムダ） | 契約の行を消した版と `analyze_modules_with_contracts(.., Off)` の IR（`llvm::emit(&module, Entry::Library)`）が**完全一致** |
| `off_checks_the_woven_form_first` | 本体が値渡しの `string` を移動し、`@ensures` が読む | `Off` でも `E1012` |
| `check_ir_has_the_contract_calls_and_is_deterministic` | 同じ入力を `Check` で 2 回 | IR に `@tz.builtin.$requires`。2 回が一致。intrinsic の宣言が重複しない |
| `default_modes_follow_the_command` | `build --emit llvm`／`--contracts check`／`run`／`run --contracts off` | `$requires` なし／あり／違反で `E2005`／通る |
| `run_reports_the_contract_line_as_e2005` | `run --json`、違反 | `E2005` の `span` が契約の行の `@` |
| `test_runner_prints_the_contract_trap_line` | 失敗する `@ensures` を呼ぶ `test` | 失敗の下に `trap: contract ensures failed at ..`。契約のない runner の IR は不変 |
| `requires_keeps_the_self_tail_call` | `requires` だけの 1,000 万段再帰（`-O0`） | 通る。`@ensures` を足すと再帰が `call`（`br label %loop` でない） |

**`tests/trap_locations.rs`**: `contract_traps_report_their_kind_and_the_clause_position`。**`tests/features.mjs` の suite `contracts`**（付録 G の 10 ケースと 3 つのトラップ）。**`main.rs` の単体テスト**: `parses_contracts_option`・`rejects_ambiguous_or_unused_arguments` への追加。

**Phase 3〜6**: `tests/contracts_property.rs`（`where` の縮小と捨てる比率・`Arbitrary` の instance・導出・生成した試験の反例・`W1009`・`--seed`）、`tests/prove.rs`（`smt_semantics_match_native_execution` = 3,312 件、証明・反例・`unsupported` の表・偽の solver・決定性・上限）。名前と入力・期待値は付録 H。

**既存テストへの影響**: `--trap-info` の site id の幅が 16 から 18 に変わる（id を `tsuzuri_trap_site()` で引くテストだけで、数値の直書きはない。確認: `git grep -n "urem i32 %offset" -- tests` が 0 件）。`--emit bindings-cs`／`py`／`cpp` の kind の配列の末尾に 2 つ増える（`git grep -n "TRAP_KINDS" -- tests` が 0 件）。`E2005` の一般の文言が変わる（手順 12 の `git grep` の全出現を直す）。`Builtin::ALL` が 246 → 248。それ以外は変わらない（契約のないプログラムの IR・診断・JSON は 1 byte も変わらない。根拠: 織り込みは `function.contracts` が空でなければ呼ばれず、`strip` は `ContractShape` が空の関数に何もせず、契約のないプログラムの `Off` は先の `Check` の検査を行わない）。

## ドキュメント

| 対象（`_tsuzuri/language-reference/` 配下） | 内容 |
| --- | --- |
| `values-and-functions/contracts.md`（新規）・`index.md` | 契約の構文・意味・モード・`E1033`・例（`run=`）・`prove` の対応する部分集合・`@old`・`Arbitrary` |
| `values-and-functions/attributes.md`・`functions.md`、`exception-handling/exception-handling.md`・`assert.md` | 属性の一覧に `@requires`・`@ensures`（HEAD は 5 つ。A20 が先なら 9 つ → 11）と「書ける位置とエラー」、契約へのリンク、トラップの表に 2 行、`assert` との使い分け（`assert` は本体の途中、契約は署名のそば） |
| `compiler/diagnostics.md`・`usage.md`・`option.md`・`debugging.md` | `E1033`・`E1034`・`W1009`、`--contracts`・`prove`・`TSUZURI_SMT`、トラップの kind |
| `built-in-types-and-modules/gen.md`・`record.md`・`union.md`、`types-and-type-inference/type-classes.md` | `Gen.for_all_where`・`Arbitrary`・`deriving (Arbitrary)` |
| `docs/language.md`・`docs/architecture.md`・`docs/stability.md`・`README.md` | 診断の表・CLI の表・属性・「トラップ位置」、ファイルと責務の表（`contracts.rs`・`prove.rs`）、互換性の追加項目、検証コマンド |

検証: `node scripts/check-docs.mjs <変えたページ>`。例は `tsuzuri run`（既定が `check`）で動き、違反の例はトラップの出力（`trap: contract ensures failed at Main.tz:3:5`）を示す。`_features/README.md`・GUIDE の台帳は統括者が更新する。

## 受け入れ条件

- [ ] **Phase 1**: 手順 2〜8 が緑。`off_ir_equals_the_ir_without_contract_lines` が通り、`tests/fixtures`・`examples` の IR が不変。`cargo fmt`・`clippy`・GUIDE §3.1 の回帰テスト。
- [ ] **Phase 2**: `trap: contract requires failed at Main.tz:2:5`（native）。`node tests/features.mjs ... contracts` が native／WASM × `-O0`／`-O3` で通る。
- [ ] **Phase 3**: `tsuzuri test --contracts test` が `contract <関数>` の試験を作り、壊れた契約を縮小した反例つきで落とす。`honors_the_exact_specialization_limit` が緑。
- [ ] **Phase 4**: `tsuzuri doc`・hover・`api-diff` が契約を出す。`W1009` の 3 つ。
- [ ] **Phase 5**: z3 で `proved`・反例（native で実行すると契約でトラップ）・`unsupported`。差分テスト 3,312／3,312。ソルバーなしで `E1034`。
- [ ] **Phase 6**: `@old (x)` が使え、`E1033` の kind が出る。
- [ ] 文書（上の表）が最新で `check-docs` が通る。完了報告（GUIDE §13.2）を末尾に書く。

## 落とし穴

| 間違えやすい点 | 検出 |
| --- | --- |
| `strip` は `polymorph::specialize` の**直前**。特殊化の後だと `$mono.N` の番号が契約の式のジェネリックな呼び出し分ずれ、`off` の IR が契約なしと一致しない（付録 A.3）。`off` と `check` で型・所有権・純粋性の結果を揃えるのは、契約を含む `Off` の先の `Check` の検査 | 手順 7 のテスト 2 つ |
| `FRONTEND_FORMAT` を上げ忘れると、古いキャッシュが `contracts` のない AST を返す | `node tests/frontend_cache.mjs target/release/tsuzuri` |
| `@ensures` は自己末尾呼び出しを壊す（`-O0` の `check` で約 10 万段で枯渇）。`@requires` だけなら壊さない | `requires_keeps_the_self_tail_call` |
| 配列・リストの引数を `@ensures` が読むと、`check` では本体の最後の使用が複製になる（`off` では起きない） | `--warn implicit-copy` の `W1006` |
| `@no_trap`（A20）の関数に契約の行を書くと、モードによらず A20 の `E1031`（契約の織り込みは `trap` 効果の源。D-53） | A20 の実装後のテスト |
| 契約の 2 つの組み込み関数の名前に `.` を入れない（`qualified_builtin` が `Module.name` として探す）。`Checker::name` は `.` のない名前を `Builtin::ALL` から見つける | `Builtin::name` が `"$requires"`・`"$ensures"` |
| `get-value` は `unsat`・`unknown` の後に `(error "..model is not available")` を出す。ソルバーの出力は**最初の行だけで判定**し、`sat` のときだけ残りを読む | `tests/prove.rs` の偽の solver |
| 契約の式は借用で読むだけにする。値を消費する関数を契約で呼ぶと、結果や引数が移動して後続が `E1012` になる | `ownership_rules_apply_inside_contracts` |

## 対象外

assume（契約を最適化の前提にする）モード。契約の継承・クラスの不変条件・メソッド（`instance`）への契約。ラムダ・ローカル関数・`{ }` の本体への契約。`extern` への契約（本体がないので書けない。`extern` の前の `@requires` は `contract_misplaced`）。std への契約の付与。ループ不変式・停止性の証明。呼び出し側の義務の静的検証（`prove` は 1 関数を本体ごと展開して検証する）。`old` に Copy でない式。cvc5 の動作保証（台本は共通だが未確認）。GPU の kernel。契約の違反を例外として捕捉すること。

## 決定事項

すべて承認済み（D-50）。実装者は蒸し返さない。

### D1: 契約は `FunctionDecl.contracts` に持つ

`ExprKind` に variant を足さない（`ExprKind::Checked` は 17 か所の走査に出る。契約は式でなく宣言の一部）。条件の原文 `text` も持つ（docgen・hover・`api-diff`・facts が式の整形器なしで出せる。整形の指紋は `text` を消す）。却下: `ExprKind::Contracted`、`Block` の特別な名前の束縛で符号化（整形・docgen が壊れる）。

### D2: 型検査器が型付き IR に織り込む

`Block`・`Call`・`Local` だけで組み、複数の `@ensures` の名前は同じ `Local` の別名にする。新しい `TypedExprKind` を足さず、全 pass が既存の規則で動く。結果を値として束縛するので、参照を含む結果の `@ensures` は `E1033`（`@requires` は可。参照のまま束縛する案は `ref` の算術がなく却下）。却下: AST の書き換え（引数が `ref` かどうか分からない）、契約を別関数に切り出す（ジェネリック・配列の複製が起きる）。

### D3: モードは `check_modules` の入力。`Off` は特殊化の直前に剥がす

契約を含むプログラムの `Off` は、先に `Check` で検査を 1 回行う。`off` の IR は契約なしと 1 byte 一致し、型・所有権・純粋性は `check` と同じ結果。既定は `build`・`bench` が `off`、`run`・`test`・`repl` が `check`（manifest の `[contracts]` は作らない。配布物を遅くせず、開発で気づける）。却下: 解析の後に剥がす `apply`（`$mono.N` がずれる。付録 A.3）、emitter で飛ばす（`let r = ..; r` の IR が元と一致しない）、`Off` で織り込まない（`off` で通って `check` で落ちる）。

### D4: 契約は `@requires`／`@ensures` で、`def` の本体の先頭の行だけに書ける

`@` の直後の `requires`／`ensures` だけを属性として読む（予約語を足さない。D-50）。ラムダ・`fn`・非ラムダの本体の先頭の行だけ。`{ }`・ガード付き・ラムダの中・クラス・`instance` は `E1033`。parser の変更を `body_expression`・`explicit_lambda` の 2 点に限る。

### D5: 実行時検査は組み込み関数 `$requires`・`$ensures` と 2 つの `TrapKind`

`ContractRequires`・`ContractEnsures`（末尾）。利用者が書けない名前。`requires` と `ensures` の失敗を区別でき、`E2005` の kind・`.trap.json` の説明が 1 対 1 になる。却下: `Builtin::Assert` の再利用・`ContractViolation` 1 つ（トラップの kind が同じで区別できない。種の記述からの逸脱として報告済み）、新しい `TypedExprKind`（走査が増える）。

### D6: 純粋性は A19 の走査を式の根へ適用する

契約の組み込み関数の引数へは降りない。二重報告を避け、保証を A19 に一本化する。A19 がないときだけ浅い版（停止条件 1 の B）。

### D7: 性質テストは `--contracts test` のときだけ、関数の双子を呼んで作る。`Arbitrary` は組み込みクラス

双子 `$contract.raw.<名前>`（契約なし）を呼ぶ。性質は `Maybe<bool>`（`None` = 捨てる）。捨てる比率の上限は 90%（1,000 回で有効な値が 100 個に届かなければ失敗）。検査つきの `f` を呼ぶと、`@ensures` の違反が性質の中のトラップになり、縮小も反例も出ない。`Arbitrary` は `Encode`／`Decode` と同じく `BUILTIN_CLASSES` に登録し、導出は再帰型を `E1025` で拒否する（停止しない生成器になる）。

### D8: `prove` は `QF_BV` の SSA の台本を外部ソルバーに渡す

値は `Bool` と `BitVec` だけで、固定幅整数の折り返しをそのまま表せる（3,312 件一致）。ソルバーは同梱しない（z3 は確認済み、cvc5 は同じ台本で未確認）。検証条件は `requires ⇒ (¬trap ∧ ensures_j)`、問いは契約の行ごと + `trap_free`。`proved` 以外はすべて `E1034`（厳格。選択は `--function`）。assume モードは作らず、証明済みの検査も省かない（定義されない動作を作らない。D-53。SMT の翻訳の誤りが実行時の安全性を壊さない）。

### D9: `@old (式)`（Phase 6）は `@ensures` の中だけ、Copy のみ

予約語・組み込みモジュール名を足さない。

## 実装と検証（2026-10-29）

- **範囲**: Phase 1〜6 をすべて実装した（構文・織り込み・純粋性・`ContractMode` と `off` の剥がし、`--contracts`・`TrapKind` 2 つ・`E2005` の kind・test runner のトラップ行・E2E、`Gen.for_all_where`・`Arbitrary`・`deriving (Arbitrary)`・`--contracts test` の性質テスト、`W1009`・docgen・hover・`api-diff`、`tsuzuri prove`・`--contracts prove`、`@old`）。基盤は A20 のブランチ（`feat/a20-effect-summary`）の上。
- **A20 との接続（チケットの経路）**: A20 が先に入ったので、条件の純粋性は `effects.rs` の走査を使う `effects::impurities`（条件の式を関数の文脈で走査し、参照する関数の要約を `summarize_from` で求める。`PURE_FORBIDDEN` と `dyn`・未解決・純粋な関数型でない関数値の呼び出しを不純とする）。`$requires`・`$ensures` は A20 の分類表で `trap`（D-53）。A19 の `pure_rules` の走査は契約の組み込み関数の引数へ降りない（`E1029` との二重の報告を避ける。A19 の既存テストは不変）。
- **チケットからの逸脱**: ① 性質テストの双子の名前は `$contract_raw_<名前>`（`.` を含む名前はモジュールの修飾と読まれるため）。② `--contracts test` は `Gen` を名前で読み込む opt-in なので、`Project::require_std("Gen")` で足す。③ 生成できる型から `Maybe` を外した（std の `Maybe` に `Display` の instance がなく、`Gen.for_all_where` の反例の表示が型検査に通らない）。④ facts の `contracts` キー（手順 24）は、手順のとおり `src/facts.rs` がない（G26 は `src/kg/` にある）ので飛ばした。⑤ `prove` は、束縛のある複数の選択肢を持つ `match` の腕を `a pattern with alternatives that bind` として非対応にした。`trap_free` の反例の位置は関数名。⑥ `--contracts` は `script` にも値つきの選択肢として足した。⑦ `tests/trap_locations.rs` の専用テストの代わりに、`tests/contracts.rs` の CLI のテスト（native の `trap:` 行と `E2005` の kind・位置）と E2E（WASM の `.trap.json` の kind）で確かめた。
- **数**: `Builtin` 252 → 254、`TrapKind` 16 → 18（`ContractRequires` = 16・`ContractEnsures` = 17。既存の番号は不変）、`DeriveClass` 7 → 8、`BUILTIN_CLASSES` 33 → 34、`FRONTEND_FORMAT` 10 → 12（契約の符号化と `deriving (Arbitrary)`）。`E2005` の一般の文言は `assert, a contract, or allocation`。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。`tests/contracts.rs` 20 件（`off` の IR が契約の行を消した版と完全一致する 10 関数の比較、`off` の先の検査、`check` の IR と決定性、CLI のモードと `E2005`、test runner のトラップ行、`@old`、1,000 万段の自己末尾再帰）、`tests/contracts_property.rs` 5 件、`tests/prove.rs` 6 件（z3 5.1 で `proved`・反例・`unsupported`、反例を native で実行すると契約でトラップ、偽のソルバーで `unknown`・`proved`・`error`・起動の失敗、**差分テスト 3,312／3,312 件が native の実行と一致**）、`api_diff_classifies_contract_lines`。`node tests/features.mjs target/debug/tsuzuri contracts` は 13 cases と 4 traps（native／WASM × O0／O3、WASM の kind）。A20 の掃引（`effects_alloc_conservative`）は契約の fixture を含む 126 プログラムで違反 0。
- **文書**: 新規 `values-and-functions/contracts.md`、`index.md`・`attributes.md`・`functions.md`・`exception-handling.md`・`assert.md`・`compiler/diagnostics.md`（`E1033`・`E1034`・`W1009` と kind の表）・`option.md`・`usage.md`・`gen.md`・`record.md`・`union.md`・`type-classes.md`、`docs/language.md`・`docs/architecture.md`・`docs/stability.md`・`README.md`・`_features/GUIDE.md`（§4）、explain の `E1033`・`E1034`・`W1009`。`check-docs` は 13 ページ・470 リンク・76 例・136 回の実行が成功。
- **IR の比較**: A20 の完了時の基準（`target/scratch/a20/base`。fixture と examples の 123 プロジェクトの `--emit llvm -O0`）と比べて、違うのは `tests/fixtures/map_set_chunks` の 1 つだけ。これは `Gen` を読み込むプログラムで、std の `Gen.tz` に足した `Arbitrary` の instance の分だけ、生成する instance の通し番号（`Main.$instance.897` → `900`）がずれる。命令は同じ。`Gen` を読み込まないプログラムは 1 byte も変わらない。全体の `cargo nextest run --locked` は 2,518 件が成功。node の `features`（`contracts`・`effect_attributes`）・`frontend_cache`・`docgen`・`lsp_sessions`・`script`・`e2e`、`check-guide-lite`・`repo_kb` も成功。
- **レビュー（#64 の 2 回目）での変更**: ① 性質テストは双子 `$contract_raw_<名前>` を呼ぶのをやめ、契約のない本体を、生成した引数をパターンの型注釈つきで束縛したところで評価する（`own` の引数を移動しないので、本体が借用するだけの引数を `@ensures` から読める）。結果は、`_` の行が結果の名前と同じ名前を読むときだけ `$contract_result` に置き、名前を付けた行の連なりごとにその名前で束縛する（連なりが 2 つ以上で結果が数値・`bool`・`char` でなければ `W1009`）。② `Expr::visit_mut` が `visit` と同じく、パターンと計算式の本体の式にも降りる。③ `@old` を引数の位置（`expression` を通らない `call_expression`）でも解決する（`Int.max (@old (x)) 0` が `E1002` だった）。④ `contract_writes` は条件の中の参照の別名（`let q = p` の `deref q = 7`）も根に辿る。⑤ `prove` は最後の `match` の腕の条件を保ち、どの腕にも合わない値を `pattern mismatch` の義務にする（ラムダの引数の `when` ガードは網羅性を検査しない）。⑥ `run` は、終了コードを返す入口でも、トラップの位置を stderr に見つけたらトラップとして報告する（Windows のトラップは終了コード `0xC000001D` で終わる）。
