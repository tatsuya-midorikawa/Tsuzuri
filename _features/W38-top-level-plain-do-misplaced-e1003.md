# W38: トップレベルの `do`（`!` なし）を含むコードで、最後の式に場違いな `E1003` が出る

| 項目 | 内容 |
| --- | --- |
| ID | W38 |
| 優先度 | P3（回避策はある: 裸の `do` を外して `Debug.print "x"` と書く。スクリプトの形で、途中の表示に `do Debug.print ...` を書き、最後に値を置くと当たる） |
| 規模 | S |
| 依存 | – |
| 後続 | [W43](W43-plain-do-in-lambda-returns-unit.md)（このチケットの `entry_value` を `last_value` に改め、結果の型の注釈がないラムダ・ブロックに同じ規則を広げる） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W37 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（文書の規則「裸の `do expr` は `unit` の通常式です」（`computation-expressions.md` の「所有権」の末尾）と「最後の式が表示されます」（`io.md` の「終了コードとトップレベルの表示」）に実装を合わせる修正。今 `E1003` で拒否しているプログラムの一部を受け入れるだけで、受け入れていたプログラムの意味・IR・診断は変えない。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `src/computation.rs` の `Checker::implicit_root` の `let Some(builder) = builder else` の `[Return]` の腕（ビルダーの無い本体の最後の値を `expected` で検査する）、`Checker::direct_root`・`Checker::implicit_prefix`（`#[inline(never)]` で `implicit_root` のフレームから出す補助） |
| 主な影響ファイル | `src/computation.rs`（`Checker::implicit_root`、新しい `Checker::entry_value`）、`tests/computations.rs`、`tests/computations.mjs`、`spec/rules/CTL.md`、`tests/conformance/CTL/`、`_tsuzuri/language-reference/computation-expressions/computation-expressions.md`、`_tsuzuri/language-reference/built-in-types-and-modules/io.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 式で終わるトップレベルのコード（`$implicit.root.$entry`）のうち、計算の文が裸の `do` だけで、`let!`・`do!` などの束縛が無いもの。0 桁目の裸の `do` がトップレベル全体を計算にする（`src/parser.rs` の `implicit_computation_ahead`）。
- 原因: `Checker::implicit_prefix` は、束縛の無い本体の文をすべて取り、最後の式の文も `_: unit` の束縛として検査する。`implicit_root` の `[] => ()` の腕が、入口の値を `()` にする。最後の式が `unit` でなければ `E1003` になる。整数リテラルの型なら、位置は `let` の右辺のリテラルになる。
- 直し方: `$entry` では、最後の文を `implicit_prefix` に渡さない。束縛が見つからなければ、新しい `Checker::entry_value` で最後の式を期待する型なしで検査する。型が `unit`・`Type::Error`・数値リテラルでない未定の型なら、今と同じ「`_: unit` の束縛と `()`」の形にする。それ以外なら、その値を入口の値にする。
- 最初に読む: `Checker::implicit_root` の `if builder.is_none()` の枝と `let Some(builder) = builder else` の match → `Checker::implicit_prefix` → `direct_entry`。最初に書くテストは `tests/computations.rs` の `entry_value_after_bare_do_statements`。
- 触ってはいけない: `unit` で終わる形の IR（「現状」の IR の比較）、ビルダーで組み立てる計算、`implicit_prefix` の今の呼び出し（`$entry` 以外）。
- 完了の見分け方: 次のプログラムの `tsuzuri run` が、標準出力に `0`、標準エラーに `x` を出して終了コード 0 で終わる（native の `-O0`・`-O3`）。

```tsuzuri
let mut n = 0
do Debug.print "x"
n
```

## 目的

文書は、裸の `do expr` を「`unit` の通常式」と定め、`do!` とは別だとしている。`main` の無いトップレベルのコードでは「最後の式が表示されます」。したがって、上のプログラムは `do` の無い同じコード（`Debug.print "x"` の行）と同じく `0` を表示するはずである。ところが実装は、裸の `do` があるだけで最後の式を `unit` の文として検査する。そして、関係の無い位置（`let mut n = 0` の `0`）に `E1003` を出す。

式で終わるトップレベルのコードで、裸の `do` の有無が最後の式の扱いを変えないようにする。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W38`。「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。
- [W31](_completed/W31-top-level-loop-before-do.md) と [W02](_completed/W02-break-in-do-function.md) が `main` に入っているかを確かめる（「W31・W02 との関係」）。入っていれば、その上に作る。

### 停止条件

- **S1** 修正の後、今受け入れているプログラムのどれかで IR か診断が 1 byte でも変わる。対象は `tests/fixtures`・`examples`・`tests/conformance`・std の例と、「現状」の c・s・p・p2、[W43](W43-plain-do-in-lambda-returns-unit.md) の「現状」の t1 である。とくに、最後の式を期待する型なしで検査することで、`unit` で終わる形の型付きの木が変わる場合。
- **S2** ビルダーで組み立てる計算（「現状」の b・j）の診断の文・kind・位置が変わる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

`target/debug/tsuzuri.exe check`（実行は同梱の MinGW の `TSUZURI_CLANG` で `run`）で確かめた。作業は `target/scratch/w-w38w40/` で行い、確認後に消した。行は `/` で区切って書く。

| # | トップレベルのコード | 今の結果 |
| --- | --- | --- |
| a | `let mut n = 0` / `do Debug.print "x"` / `n` | `1:13: error[E1003]: expected an integer, found unit`（`0` を指す） |
| e | `let n = 5` / `do Debug.print "x"` / `n` | `1:9` の `E1003`（`5` を指す） |
| f | `let n: i64 = 5` / `do Debug.print "x"` / `n` | `3:1: error[E1003]: expected unit, found i64` |
| g | `do Debug.print "x"` / `"s"` | `2:1` の `E1003`（`expected unit, found string`） |
| r | `do Debug.print "x"` / `IO.writeln "y"` | `2:1` の `E1003`（`expected unit, found IO<unit>`） |
| d | `let mut n = 0` / `Debug.print "x"` / `n`（`do` なし） | 受け入れる。`run` は標準出力 `0`、標準エラー `x` |
| c・p2 | c: `let mut n = 0` / `do Debug.print "x"`、p2: `do Debug.print "x"` / `do Debug.print "y"` | 受け入れる（c は `W1001` だけ） |
| s | `let xs = [1, 2]` / `do Debug.print "x"` / `Debug.print "y"` | 受け入れる |
| p | `do Debug.print "x"` / `unreachable ()` | 受け入れる（最後を `unit` として検査する）。`do` の無い同じコードは `2:1` の `E1015`（ambiguous polymorphic use） |
| k | `let mut n = 0` / `do! IO.writeln "x"` / `n` | 受け入れる（ビルダーが `IO` の直接実行） |
| b | `let mut n = 0` / `let! s = Maybe.Some 0` / `let! x = Maybe.Some n` / `x` | `1:13` の `E1003`（`expected an integer, found unit`） |
| j | `let n: i64 = 5` / `let! x = Maybe.Some n` / `x` | `3:1: error[E1003]: expected unit, found i64` |

- `do` の無い形（d・`Debug.print "x"` / `"s"`・`Debug.print "x"` / `IO.writeln "y"`・`let n = 5` / `Debug.print "x"` / `n`）を `run` すると、それぞれ `0`・`s`・`y`・`5` を標準出力に出し、終了コード 0。IR の比較（`build --emit llvm -O1`、native と `--target wasm32`）: c と、`do` を外した `let mut n = 0` / `Debug.print "x"` は違う。`do` のある形は `alloca i8` に最後の値を置いて `ret i8 0` を返し、無い形は `ret i8 %v4` を返す。s と p2 も、`do` を外した形と同じように違う。したがって、`unit` で終わる形は今の型付きの木（`_: unit` の束縛と `()`）を保たなければ、IR が変わる。

### 原因（コード）

- 構文と入口: `src/parser.rs` の `computation_statement` は、裸の `do expr` を `ComputationStatementKind::Expression(expr)` にする（3680 行目付近。`do` の無い式の文と区別がない）。0 桁目の `do` は `implicit_computation_ahead` を真にし、トップレベル全体を `$implicit` の計算にする。`src/check.rs` の `check_modules_collect` では、`computation::direct_entry` が、最後の文が式の計算を `$implicit.root.$entry` に変え（9534 行目付近）、`checker.expression(&expression, None)` の `body.ty` を入口の結果型にする。
- `src/computation.rs` の `Checker::implicit_root`: `$entry` は期待する型を持たず、`builder` は `None`。そのため `implicit_prefix(body, ..)` に進む。`implicit_prefix` は `Let(_, false)`・`Expression`・`let rec` の組を取り、`Expression` は `annotation: unit` の `_` の束縛として検査する。束縛の無い本体では最後の文まで取る。残りが空になるので、`let Some(builder) = builder else` の `[] => Unit` の腕が入口の値を `()` にする。
- 位置が `0` になる理由: `let mut n = 0` の `n` は数値リテラルの開いた型を持つ。`_: unit` との単一化は成功し、`Integer` の制約が残る。`src/polymorph.rs` の `Checker::finish` が最後に制約を検査し、`integer_literals` の枝がリテラルの位置で `expected an integer, found unit` を出す。f のように型が具体なら、位置は最後の式になる。
- b・j（ビルダーで組み立てる計算が式で終わる形）の拒否は文書どおりである（`computation-expressions.md` の表「末尾の通常式 … 通常式は `unit` に限り … 暗黙の `return` はない」）。b の位置がリテラルになる理由は上と同じ制約の検査で、通常のコードでも起きる（`def main` の中の `let n = 0` / `n` / `0` も `2:13` の `E1003`）。このチケットの原因（裸の `do` が最後の式を `unit` にすること）とは別である（「対象外」）。

## 仕様

1. 式で終わるトップレベルのコードで、束縛（`let!`・`do!`・`match!`・`return!`・`and!`）が無く、最後の文より前の文がすべて `implicit_prefix` の取る文（`let`・`let rec` の組・式の文。裸の `do` を含む）のとき、最後の式を入口の値にする。表示と終了コードは、`do` の無い同じコードと同じ規則（`io.md` の「終了コードとトップレベルの表示」）に従う。
2. ただし、最後の式の型が `unit`、`Type::Error`、または数値リテラルでない未定の型のときは、今と同じく `unit` の文として扱い、入口の値は `()` にする。これで c・s・p2 の IR と、p の受け入れが変わらない。p だけは `do` の無い同じコード（`E1015`）と結果が違うまま残る（D2）。
3. 次は今と同じ: 束縛のあるトップレベルのコード（k・b・j）、`do!`・`return` で終わるトップレベルのコード、評価の順序（文は書いた順、最後の式はその後）、受け入れていたプログラムの意味・IR・診断。ビルダーで組み立てる計算の末尾の通常式は、今どおり `unit` に限る（暗黙の `return` はない）。
4. 結果: a・e・f・g・r を受け入れ、それぞれ `0`・`5`・`5`・`s` を表示し、r は `IO.writeln "y"` を 1 回実行する。標準エラーには `x` が出る。

## 設計

`src/computation.rs` の `Checker::implicit_root` の `if builder.is_none()` の枝:

```rust
// W38: the last statement of top-level code that ends in an expression is the entry's value.
let count = if entry {
    let mut head = body.clone();
    head.statements.pop();
    self.implicit_prefix(&head, &mut prefix)?
} else {
    self.implicit_prefix(body, &mut prefix)?
};
remaining.statements.drain(..count);
```

`let Some(builder) = builder else` の match の `[Return]` の腕の後に、`[ComputationStatement { kind: ComputationStatementKind::Expression(value), span }] if entry => return self.entry_value(prefix, value, *span, body.span, expected)` の腕を足す。

```rust
/// The value of top-level code that ends in an expression and binds nothing (W38): bare `do`s are unit
/// statements, so the last expression is the value, unless it is unit or open (kept as before: same IR).
#[inline(never)]
fn entry_value(&mut self, mut prefix: Vec<(Local, TypedExpr)>, value: &Expr, statement: Span,
               body: Span, expected: Option<&Type>) -> Result<TypedExpr, Diagnostic> {
    let checked = self.expression(value, None)?;
    let ty = self.inference.resolve(&checked.ty);
    let unit = matches!(ty, Type::Unit | Type::Error)
        || (matches!(ty, Type::Infer(_)) && !self.inference.is_numeric_literal(&ty));
    if !unit {
        return Ok(typed_block(prefix, checked, body));
    }
    self.same(&checked.ty, &Type::Unit, statement)?;
    let local = self.bind(&ident("_", statement), checked.ty.clone(), false);
    prefix.push((local, checked));
    let result = self.expression(&make(ExprKind::Unit, body, 1)?, expected)?;
    Ok(typed_block(prefix, result, body))
}
```

- `implicit_prefix` の署名は変えない（W31 と重ねないため。`head` の複製は `$entry` のときだけ）。束縛があれば `implicit_prefix` は最後の文より前で止まるので、`head` でも今と同じ数を返す。新しい腕に届くのは残りが最後の式の文 1 つのときだけで、仕様 1 の形と一致する。
- `unit` の形の手順（式の検査 → `_` の束縛 → `()` の検査）は、今の `implicit_prefix` の最後の 1 回と `[]` の腕と同じ順である。違いは 2 つだけ。(a) 式を `Some(&Type::Unit)` ではなく `None` で検査し、後で `same` で `unit` にそろえる。(b) `annotation(unit)` を呼ばない（`unit` の注釈は制約も局所の番号も作らない）。IR が同じことは、手順 1 の比較で確かめる（S1）。

### 採らない案

- **裸の `do` では計算にしない（`implicit_computation_ahead` で `do!` だけを数える）**: c・s・p2 が通常のブロックになり、IR が変わる（「現状」の IR の比較）。ほかのブロックの裸の `do` の扱いも変わる。
- **最後の式を常に入口の値にする、または `direct_root` に送る**（W31 の D4 と同じ経路。`direct_block` は最後の式をブロックの値にする）: c・s・p2 の IR が変わり、前者は p を `E1015` にする（受け入れていたプログラムを壊す）。
- **位置だけを最後の式に直す**: 文書の規則（裸の `do` は通常の `unit` 式、最後の式は表示される）と食い違いが残る。

## 前提とする他チケットのインターフェース

- なし。

### W31・W02 との関係

- [W02](_completed/W02-break-in-do-function.md) が変えるのは、`check_implicit` と `implicit_root` の先頭の判定（`runs_directly`）である。このチケットの行とは重ならず、意味も独立している（`$entry` の `normal_loop_depth` は 0 のまま）。
- [W31](_completed/W31-top-level-loop-before-do.md) は、`implicit_prefix` に `direct_controls` を足し、`if builder.is_none()` の枝に 2 回目の呼び出しと `assumed_direct` を足す。このチケットは同じ枝の 1 回目の呼び出しを変えるので、テキストは衝突する。
- 意味の上の関係: W31 の `direct_controls` の規則 (b)「最後の文は取らない」と、このチケットの `head` は、同じ規則（`$entry` の最後の文は入口の値）を別の呼び出しに当てる。両方が入った後は、`while` などの制御の文の後に裸の `do` と最後の式がある形は W31 の D4（`direct_root`）、制御の文の無い形はこのチケットの `entry_value` に進む。
- 推奨の順: W02 → W31 → W38（優先度の順）。後から入る方の PR が、組み合わせの形（`let mut n = 0` / `while n < 3 do n = n + 1` / `do Debug.print "x"` / `n` は W31 の経路、a はこのチケットの経路）をテストに足す。W31 の「対象外」の行（「素の `do` を含み束縛の無いトップレベルの計算の、最後の式」）と落とし穴の HEAD の注記は、このチケットを指す。

## 他チケットへ提供するインターフェース

- なし。W06・W12 などのスクリプトの形の例は、`do Debug.print ...` の後に値を置いてよい。

## 実装手順

1. **変更**: ブランチ `fix/w38-entry-value-after-bare-do`。基準の記録だけ。
   **内容**: 着手条件を確かめる。HEAD のコンパイラで、`tests/fixtures`・`examples` の `Main.tz` と、「現状」の c・s・p・p2、W43 の t1（落とし穴）を `--emit llvm -O1`（native と `--target wasm32`）にする。その結果を `check --json` の結果と一緒に `target/scratch/w38/before/` へ保存する。テスト計画の `do!` で終わる形の `check` の結果も記録する。
   **確認**: 保存した数を控える。
2. **変更**: `tests/computations.rs` に新しいテスト（テスト計画）を先に書く。
   **確認**: `cargo test --locked --test computations entry_value_after_bare_do` が `running 1 test` で失敗する。
3. **変更**: `src/computation.rs`（設計）。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test computations` と `--test specs` の全件。手順 1 の IR と `check --json` が `cmp` で一致する（S1）。
4. **変更**: E2E（`tests/computations.mjs`）と適合性テスト（`spec/rules/CTL.md`・`tests/conformance/CTL/`）。
   **確認**: `node tests/computations.mjs target/debug/tsuzuri`。`node scripts/conformance.mjs target/debug/tsuzuri --rule CTL-0NN`（番号は着手時の次の番号）。
5. **変更**: 文書（下）。
   **確認**: 変更したページの `node scripts/check-docs.mjs`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`。T1、T2。`target/scratch/w38/` を消す。

## テスト計画

### Rust（`tests/computations.rs`）

`entry_value_after_bare_do_statements`: 既存の `accepts`（利用者のビルダーの `.tc` を置いたプロジェクトで解析し、native と wasm32 の IR を作って決定的であることを見る）と `rejects`。

- 受け入れる: a・e・f・g・r。`let rec even … and odd …` の組・`do Debug.print "x"`・`even 4` の順の形（今は `4:1` の `expected unit, found bool`）。
- 今と同じに受け入れる: c・s・p2・p・d・k と、`let n = 0` / `do if n == 0 then Debug.print "zero"` / `do Debug.print "x"`。c と p2 は、入口の結果型が `unit` であることも確かめる（`analyze_modules` の結果の `functions[entry]` の `signature.result`）。
- 拒否する（今と同じ）: b と j は `E1003`（コードだけを固定する。b の位置は「対象外」）。`do!` で終わるトップレベルのコード（`let mut n = 0` / `do Debug.print "x"` / `do! IO.writeln "y"`）は今と同じ結果（起票時には測っていないので、HEAD の結果を手順 1 で記録して固定する）。

### E2E（`tests/computations.mjs`）

一時プロジェクトを `tsuzuri run` で `-O0` と `-O3` の両方で動かす。期待値は手で書く（GUIDE-LITE §2 規則 10）。

| 内容 | 期待 |
| --- | --- |
| a | 標準出力 `0\n`、標準エラーに `x`、終了コード 0 |
| g | 標準出力 `s\n` |
| r | 標準出力 `y\n`（`IO` を 1 回実行） |

### 適合性テスト

- `spec/rules/CTL.md` に次の番号の規則を足す。文: 「In top-level code that ends in an expression, bare `do expr` statements MUST NOT change the value that the code prints.」
- `tests/conformance/CTL/CTL-0NN-bare-do-keeps-entry-value.tz`（`let n = 41` / `do Debug.print "x"` / `n + 1`。`// expect: stdout "42\n"`）。文書の規則の文（`computation-expressions.md` の裸の `do` の文）の直前に `<!-- rule: CTL-0NN -->` を置く。

## ドキュメント

- `_tsuzuri/language-reference/computation-expressions/computation-expressions.md` の「裸の `do expr` は `unit` の通常式です」（386 行目付近）に 1 文足す: トップレベルのコードに裸の `do` があっても、`let!`・`do!` が無ければ、最後の式は `do` が無いときと同じく表示される。`run=42` の例（適合性テストと同じプログラム）を 1 つ足す。
- `_tsuzuri/language-reference/built-in-types-and-modules/io.md` の「終了コードとトップレベルの表示」（229 行目付近）の箇条に同じ 1 文。
- `docs/language.md` の「ビルダーによるモナド合成を必要とせず…」（2499 行目付近）に、トップレベルの裸の `do` の後の最後の式の扱いを 1 文足す。README と `compiler/diagnostics.md`（`E1003` の kind を足さない）は変えない。

## 受け入れ条件

- [ ] a・e・f・g・r が受け入れられ、仕様 4 のとおり表示する（`-O0`・`-O3`）。
- [ ] c・s・p2・p・d・k を今と同じに受け入れ、b・j は今と同じ `E1003`。
- [ ] 今受け入れているプログラム（`tests/fixtures`・`examples` のすべての `Main.tz` と c・s・p・p2・t1）の IR と `check --json` が、native と wasm32 で 1 byte も変わらない（手順 1 との比較）。
- [ ] `tests/computations.rs` の新しいテストと、`tests/computations.rs`・`tests/specs.rs` の既存の全件、`node tests/computations.mjs target/debug/tsuzuri` が通る。
- [ ] 適合性テスト、`check-spec --require-markers`、変更した全ページの `check-docs`、T2 が通る。

## 落とし穴

- **`unit` の形と開いた型**: 型が `unit` なら必ず `_` の束縛と `()` にする（ブロックの値にすると c・s・p2 の IR が変わる）。`unreachable ()` のような開いた型を値にすると p が `E1015` になるが、数値リテラル（`is_numeric_literal`）は値にしないと a・e が直らない。
- **2 回検査しない**: 最後の式は 1 回だけ検査する（`Checker` は複製できない）。型を見てから形を選ぶ。
- **最後の `match` の腕の裸の `do`**: トップレベルの `let c = Maybe.Some 3` / `do Debug.print "x"` / `match c with`（腕が裸の `do` を含む `unit` の本体。[W43](W43-plain-do-in-lambda-returns-unit.md) の「現状」の t1）は今受け入れられ、`match` を `unit` で検査するので腕は `direct_root` に進む。最後の式を期待する型なしで検査すると腕が素の `$implicit` の経路に進み、IR が変わりうる（W43 の s1・s2 の比較）。手順 1 の集合と S1 の対象に t1 を入れてあるので、変わるなら止めて報告する（W43 の D2 のヒントの推論変数が直し方の案）。
- **`$entry` だけ**: `head` は `$entry` のときだけ使う。結果型の無いラムダやブロックの暗黙の計算に広げない（「対象外」。[W43](W43-plain-do-in-lambda-returns-unit.md) が広げる）。

## 対象外

- **結果型の無いラムダ・ブロックの、同じ形**: `def main` の中の `let f = \() ->` の本体が `let n = 5` / `do Debug.print "x"` / `n` の形だと、`f` は `unit` を返す。そのため `f () as i32` は `6:5: error[E1005]: no instance for Numeric<unit>` になる。原因は同じ `implicit_root` の枝だが、入口の表示の規則が無く、ラムダの型推論を変えるので、別に決める（[W43](W43-plain-do-in-lambda-returns-unit.md) が直す。W43 は `entry_value` を `last_value` に改め、`$entry` も同じヒントの推論変数で検査する）。
- **数値リテラルの制約の誤りが、リテラルの位置に出ること**: b の位置（`let mut n = 0` の `0`）は、`Checker::finish` が `Integer` の制約をリテラルの位置で報告するためである。通常のコードでも同じことが起きる（`def main` の中の `let n = 0` / `n` / `0`、`let n = 0` / `let u: unit = n` / `0` は、どちらも `0` を指す `E1003`）。[W44](W44-literal-constraint-error-location.md) が、主の位置を型を決めた単一化の位置にし、リテラルを `related` に回す。W44 の起票時の調べでは、位置の期待値が変わる既存のテストは `tests/parameter_conventions.rs` の 1 件で、CTL-002 は同じ行のままである。このチケットのテストは、b の誤りをコードだけで固定する。
- ビルダーで組み立てる計算の末尾の通常式に、暗黙の `return` を入れること（文書の表が否定している）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 束縛の無い `$entry` では、最後の式を入口の値にする（a・e・f・g・r を受け入れる）。位置を直すだけにはしない | 文書は裸の `do` を通常の `unit` 式とし、トップレベルの最後の式を表示すると定める。`do` の無い同じコード（d）は受け入れて `0` を表示する。位置と文面だけを直すのが正しいのは、拒否が文書どおりの b の形だが、b の位置の原因は別（数値リテラルの制約）なので「対象外」に送った | 既定案（実装者はこの案に従う） |
| D2 | 最後の式の型が `unit`・`Type::Error`・数値リテラルでない未定の型なら、今と同じ `unit` の文と `()` にする | c・s・p2 の IR を 1 byte も変えず、p（`do` の無い同じコードは `E1015`）を受け入れたままにする。p の不一致は、受け入れていたプログラムを壊さないために残す | 既定案（実装者はこの案に従う） |
| D3 | `implicit_prefix` の署名は変えず、`$entry` では最後の文を除いた複製を渡す | W31 が同じ関数の署名を変えるので、衝突を減らす。束縛があるときの結果は今と同じ | 既定案（実装者はこの案に従う） |
| D4 | 規則 `CTL-0NN`（着手時の次の番号）と、受け入れる形の適合性テスト 1 つを足す | 裸の `do` の意味は文書の規則で、外れたことを CI（`conformance.mjs`）で捕まえるため（D-58 の形式。W31 の D5 と同じ） | 既定案（実装者はこの案に従う） |
| D5 | b の位置（数値リテラルの制約）とラムダ・ブロックの同じ形は、このチケットでは直さない | 原因が別か、規則を別に決める必要がある（「対象外」。前者は [W44](W44-literal-constraint-error-location.md)、後者は [W43](W43-plain-do-in-lambda-returns-unit.md)）。小さな修正の範囲を保つ | 既定案（実装者はこの案に従う） |
