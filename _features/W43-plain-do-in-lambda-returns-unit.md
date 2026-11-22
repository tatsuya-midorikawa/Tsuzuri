# W43: 結果の型の注釈がないラムダ・ブロックに `!` のない `do` があると、最後の式ではなく `unit` を返す型になる

| 項目 | 内容 |
| --- | --- |
| ID | W43 |
| 優先度 | P2（回避策はある: 裸の `do` を外して `Debug.print "x"` と書くか、ラムダの型を注釈する（`let f: unit -> i64 = \() -> …`）。ただ、ラムダの途中に `do Debug.print …` を 1 行足すだけで、正しいプログラムが関係の無い位置の `E1005`・`E1003` で拒否される） |
| 規模 | S |
| 依存 | W38 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W41 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（文書の規則「裸の `do expr` は `unit` の通常式です」（`computation-expressions.md` の「所有権」の末尾）に実装を合わせる修正。今拒否しているプログラムの一部を受け入れるだけで、受け入れていたプログラムの意味・IR・診断は変えない。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | [W38](W38-top-level-plain-do-misplaced-e1003.md) の `Checker::entry_value`（`$entry` の最後の式を値にする補助）、`src/computation.rs` の `Checker::direct_context`・`Checker::direct_root`、`Checker::implicit_root` の `let Some(builder) = builder else` の match |
| 主な影響ファイル | `src/computation.rs`（`Checker::implicit_root`・`Checker::direct_context`、W38 の `entry_value` を一般にした `Checker::last_value`）、`src/check.rs`（`Checker` の欄 `last_value_hints` とその初期化）、`tests/computations.rs`、`tests/computations.mjs`、`spec/rules/CTL.md`、`tests/conformance/CTL/`、`_tsuzuri/language-reference/computation-expressions/computation-expressions.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 結果の型が決まらない文脈（期待する型が無いか、未定の推論変数）の暗黙の計算（素の `$implicit`）のうち、`let!`・`do!` などの束縛が無く、最後の文が式のもの。結果の型の注釈がないラムダの本体、`let` の値の `{ }` ブロック、期待する型の無い `if` の枝と `match` の腕、`Array.map` などへ渡すラムダが当たる。文の頭の裸の `do` がその本体を計算にする（`src/parser.rs` の `implicit_computation_ahead`。`Parser::lambda`・`Parser::block`・`src/parse_control.rs` の `layout_block` から）。
- 原因: `Checker::implicit_root` は期待する型からビルダーを決められない。`Checker::implicit_prefix` が最後の式まで `_: unit` の束縛として検査し、`[] => Unit` の腕が本体の値を `()` にする。そのためラムダは `unit` を返す型になる。W38 が直すのはトップレベル（`$entry`）だけである。
- 直し方: W38 の `entry_value` を `last_value` に改める。`$entry` と同じ条件（束縛が無く、最後の文が式）なら、素の `$implicit` にも使う。最後の式は、新しいヒントの推論変数を期待する型にして検査する。`direct_context` は、このヒントを期待する入れ子の暗黙の本体を直接実行にする。今その本体は `unit` を期待して直接実行に進むので、同じ経路を保つ。これで `unit` で終わる本体の IR を変えない。
- 最初に読む: W38 が入れた `implicit_root` の `if builder.is_none()` の枝と `entry_value` → `Checker::direct_context` → `src/closures.rs` の `Checker::lambda`（注釈が無ければ本体を新しい推論変数 `Some(&result)` で検査する）→ `src/check.rs` の `ExprKind::If` の検査（`then` に期待する型、`else` に `sibling_hint`）。最初に書くテストは `tests/computations.rs` の `last_value_after_bare_do_in_lambdas_and_blocks`。
- 触ってはいけない: `unit` で終わる本体の型付きの木と IR（「現状」の IR の比較）、ビルダーで組み立てる計算、結果の型が決まる文脈（`direct_root`）。
- 完了の見分け方: 次のプログラム（「現状」の q2）が、標準出力に `5`、標準エラーに `x` を出して終了コード 0 で終わる（native の `-O0`・`-O3`）。

```tsuzuri
def main :: unit -> i32 = \() ->
    let f = \() ->
        let n = 5
        do Debug.print "x"
        n
    do! IO.writeln (to_string (f ()))
    0
```

## 目的

文書は、裸の `do expr` を「`unit` の通常式」と定め、`do!` とは別だとしている。したがって、上の `f` は `do` の無い同じラムダ（「現状」の l3）と同じく `5` を返すはずである。ところが実装は、裸の `do` があるだけで本体を `unit` の文の列として扱う。`f` の型が `unit -> unit` になり、関係の無い位置（`f () as i32` の `E1005`、`let n = 5` の `5` の `E1003`）で拒否する。

W38 はトップレベルのコードについて同じ食い違いを直す。このチケットは、その規則をラムダとブロックに広げる。結果の型の注釈の有無で、裸の `do` の後の最後の式の扱いが変わらないようにする。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W43`。「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。
- [W38](W38-top-level-plain-do-misplaced-e1003.md) が `main` に入っている（依存）。入っていなければ始めない。[W02](W02-break-in-do-function.md) と [W31](W31-top-level-loop-before-do.md) が入っているかも確かめ、入っていればその上に作る（「前提とする他チケットのインターフェース」）。

### 停止条件

- **S1** 修正の後、今受け入れているプログラムのどれかで IR か診断が 1 byte でも変わる。対象は `tests/fixtures`・`examples`・`tests/conformance`・std の例と、「現状」の u1・u2・n5・t1・t2・h1・l2・l3 である。とくに、最後の式を `unit` でなくヒントで検査したために、型付きの木が変わる場合。
- **S2** ビルダーで組み立てる計算の診断の文・kind・位置が変わる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

`target/debug/tsuzuri.exe check` で確かめ、実行は Git Bash で同梱の MinGW の `TSUZURI_CLANG` を使って `run` した。作業は `target/scratch/w-w43w44/` で行い、確認後に消した。行は `/` で区切って書く。特に書かない限り `def main :: unit -> i32 = \() ->` の本体の中である。

| # | コード | 今の結果 |
| --- | --- | --- |
| l1 | `let f = \() ->` / `let n = 5` / `do Debug.print "x"` / `n` の後に `f () as i32` | `6:5: error[E1005]: no instance for Numeric<unit>` |
| q2 | 同じ `f` の後に `do! IO.writeln (to_string (f ()))` / `0`（早見のプログラム） | `3:17: error[E1003]: expected an integer, found unit`（`5` を指す） |
| l3 | l1 から `do` を外した形 | 受け入れる。`run` は標準エラーに `x`、終了コード 5 |
| l2 | l1 を `let f: unit -> i64 = \() ->` にした形 | 受け入れる（期待する型が `i64` なので `direct_root`）。終了コード 5 |
| g3 | `let f = \(x: i32) ->` / `do Debug.print "x"` / `x` の後に `f 5` | `4:9: error[E1003]: expected unit, found i32` と `5:5` の `E1003` |
| g2 | g3 の引数の注釈を外した `\x ->` | `5:5: error[E1003]: expected i32, found unit`（`x` が `unit` に決まる） |
| b1 | `let v = {`、改行で区切った `let n = 5` / `do Debug.print "x"` / `n`、`}` の後に `v as i32` | `7:5` の `E1005`（`Numeric<unit>`） |
| i1・m1 | i1: `let v =` の値が `if c then`（`do Debug.print "x"` / `1`）`else`（`0`）、その後に `v`。m1: `match` の腕が同じ形で、ほかの腕が `0` | i1 は `6:13` と `8:13` の `E1003`（`expected unit, found i32`）、m1 は `7:13`・`8:16`・`9:5` の `E1003` |
| a1 | `Array.map (\x ->`（`do Debug.print "x"` / `x + 1`）`) xs` の後に `ys[0] as i32` | `5:13` の `E1003` と `6:5` の `E1005` |
| t3 | `let f = \(c: Maybe<i64>) ->` / `do Debug.print "x"` / `match c with` で、腕 `Some v` が `do Debug.print "some"` / `v`、腕 `None` が `0`。その後に `f (Maybe.Some 3) as i32` | `7:13`・`8:25` の `E1003` と `9:5` の `E1005` |
| u1 | `let f = \() ->` / `let n = 5` / `do Debug.print "x"`（`unit` で終わる） | 受け入れる（`W1001` だけ） |
| n5 | `let f = \(c: bool) ->` / `do Debug.print "a"` / `match c with`（2 つの腕が `Debug.print …`） | 受け入れる |
| t2 | t3 の腕を `unit` にした形（腕 `Some v` は `do Debug.print "some"` / `Debug.print (to_string v)`、腕 `None` は `Debug.print "none"`） | 受け入れる。`run` は標準エラーに `x`・`some`・`3` |
| h1 | `let f = \(c: bool) ->` / `do Debug.print "a"` / `match c with` で、腕 `true` が `do! IO.writeln "t"`、腕 `false` が `()` | 受け入れる。`run` は標準出力に `t`（腕の `do!` をその場で実行する） |
| h2 | h1 から `do Debug.print "a"` を外した形 | `6:20` と `7:5` の `E1003`（腕が `IO<unit>` の値になる） |
| j2 | h1 の腕 `true` を `do! IO.writeln "t"` / `1`、腕 `false` を `0` にし、最後を `f true` にした形 | `7:13`・`8:20`・`9:5` の `E1003`（`expected unit, found i32` など） |
| j1 | h1 の腕 `true` を `let! x = Maybe.Some 1` / `Debug.print (to_string x)` にした形 | `6:22: error[E1003]: expected IO<an undetermined type>, found Maybe<an undetermined type>`（腕を直接実行するので `IO.run` の引数になる） |
| t1 | トップレベルの `let c = Maybe.Some 3` / `do Debug.print "x"` / `match c with`（t2 と同じ腕） | 受け入れる |
| n3 | `let f = \c ->` / `do Debug.print "a"` / `if c then 1 else 2` | `3:9: error[E1018]: cannot infer the computation builder; …`（「対象外」） |

IR の比較（`build --emit llvm`。native の `-O0`・`-O1` と `--target wasm32 -O1`。どれも違いが出た）:

- u2（`let f = \() ->` / `let _n = 5` / `do Debug.print "x"` / `Debug.print "y"`）と、`do` を外した u3 は違う。u2 は `alloca i8` に最後の値を `store` して `ret i8 0` を返し、u3 は `ret i8 %v17` を返す。したがって、`unit` で終わる本体は今の型付きの木（`_: unit` の束縛と `()`）を保たなければ、IR が変わる。
- s1（`let r: unit = match c with …`。腕の 1 つが裸の `do` を含む本体）と s2（同じ式の `let r = match c with …`）は違う（s2 に `alloca i8` と `store` が 1 つずつ多い）。腕の暗黙の本体は、期待する型が `unit` なら `direct_root`（s1）、未定なら素の `$implicit` の経路（s2）に進み、形が違う。t2・h1・t1 の最後の `match` は今 `unit` を期待して検査されるので、腕は `direct_root` に進んでいる。最後の式を `unit` なしで検査すると、t2・t1 の IR が変わり（s1 と s2 の差）、h1 は h2 と同じ誤りになる。

### 原因（コード）

- `src/computation.rs` の `check_implicit` は、素の `$implicit` を `implicit_root(None, …)` に渡す（`strip_prefix("$implicit.root.")` が `None`）。ラムダは `src/closures.rs` の `Checker::lambda` が、注釈が無ければ本体を新しい推論変数 `Some(&result)` で検査する。`let` の値のブロックは `None` で、`if` の `then` の枝は `if` の期待する型で検査される（`src/check.rs` の `ExprKind::If`）。
- `Checker::implicit_root`: `direct_context` は期待する型が未定の推論変数なら偽である。`implicit_candidates(expected, true)` からも 1 つのビルダーは決まらない。束縛が無いので `implicit_prefix(body, …)` が最後の文まで取り、`Expression` を `annotation: unit` の `_` の束縛として検査する。残りは空なので、`[] => Unit` の腕が本体の値を `()` にし、期待する型（ラムダの結果の変数）を `unit` に決める。
- 位置が `5` になるもの（q2）は、数値リテラルの型の変数が `unit` に解かれ、`src/polymorph.rs` の `Checker::finish` がリテラルの位置で制約の誤りを出すためである（[W44](W44-literal-constraint-error-location.md)）。

## 仕様

1. 結果の型が決まらない文脈の素の暗黙の本体が、次の 3 つを満たすとき、最後の式を本体の値にする。束縛（`let!`・`do!`・`match!`・`return!`・`and!`）が無い。最後の文より前の文がすべて `implicit_prefix` の取る文（`let`・`let rec` の組・式の文。裸の `do` を含む）である。最後の文が式である。これは `do` の無い同じ本体と同じ扱いで、`$entry`（W38）と同じ規則である。
2. ただし、最後の式の型が `unit`、`Type::Error`、数値リテラルでない未定の型のときは、今と同じく `unit` の文として扱い、本体の値は `()` にする（W38 の D2）。g2 のように、型が未定の引数をそのまま最後の式にした形は、今と同じく `unit` を返す（引数か結果を注釈すれば値になる。g3）。
3. 最後の式の値の位置（`match` の腕、`if` の枝、ブロックの結果など、最後の式の期待する型をそのまま受け取る位置）にある入れ子の暗黙の本体は、今と同じく直接実行（`direct_root`）にする。t2・h1・t1 の意味と IR は変わらない。最後の式が値になる形（t3）でも同じ規則で、腕の `do!` はその場で実行する。
4. 次は今と同じ: 結果の型が決まる文脈（注釈や `def` のシグネチャで `direct_root` に進む本体）、束縛のある本体（ビルダーで組み立てる計算。末尾の通常式は今どおり `unit` に限る）、最後の文が `if`・`while`・`for` の文の本体（n3 の `E1018`）、評価の順序（文は書いた順、最後の式はその後）。
5. 結果: l1・q2・b1・i1・m1・a1・t3・g3・j2 を受け入れる。q2 は `5` を表示し、標準エラーに `x` を出す。j2 は腕の `do!` をその場で実行して `1` を返す。j1 は今と同じ `E1003` のままである。

## 設計

`Checker`（`src/check.rs`）に欄 `last_value_hints: Vec<Type>`（`last_value` が検査中の最後の式のヒントの変数）を足し、`Vec::new()` で初期化する。`src/computation.rs` の `Checker::direct_context` では、`resolve` した期待する型が `Type::Infer` なら、`last_value_hints` のどれかを `resolve` した型と等しいかを返す（今は常に偽）。W38 の `entry_value` は、次の `last_value` に置き換え、`$entry` からも呼ぶ。

```rust
/// The value of an implicit body that binds nothing and ends in an expression (W38, W43): bare
/// `do`s are unit statements, so the last expression is the value, unless it is unit, an error or
/// open (no number literal), which keeps the unit statement and `()` of before (same IR).
#[inline(never)]
fn last_value(&mut self, mut prefix: Vec<(Local, TypedExpr)>, value: &Expr, statement: Span,
              body: Span, expected: Option<&Type>) -> Result<TypedExpr, Diagnostic> {
    let hint = self.inference.fresh();
    self.last_value_hints.push(hint.clone());
    let checked = self.expression(value, Some(&hint));
    self.last_value_hints.pop();
    let checked = checked?;
    let ty = self.inference.resolve(&checked.ty);
    let unit = matches!(ty, Type::Unit | Type::Error)
        || (matches!(ty, Type::Infer(_)) && !self.inference.is_numeric_literal(&ty));
    if !unit {
        if let Some(expected) = expected {
            self.same(&checked.ty, expected, value.span)?;
        }
        return Ok(typed_block(prefix, checked, body));
    }
    self.same(&checked.ty, &Type::Unit, statement)?;
    let local = self.bind(&ident("_", statement), checked.ty.clone(), false);
    prefix.push((local, checked));
    let result = self.expression(&make(ExprKind::Unit, body, 1)?, expected)?;
    Ok(typed_block(prefix, result, body))
}
```

`Checker::implicit_root` では、W38 の `if entry` の条件を `tail` に広げる。`tail` は、`$entry` を除いた後の `forced` が `None`（素の `$implicit` か `$entry`）で、最後の文が `ComputationStatementKind::Expression` であることを表す。`if builder.is_none()` の枝では、すでに複製した `remaining` から最後の文を外して `implicit_prefix(&remaining, …)` に渡し、取った数を `drain` した後に戻す。`body.clone()` は増やさない。`let Some(builder) = builder else` の match の腕は、W38 の `if entry` を `if tail` にして `last_value` を呼ぶ。

- 束縛があれば `implicit_prefix` は束縛の手前で止まり、取る数は今と同じなので、ビルダーの経路は変わらない（W38 と同じ議論）。
- ヒントが `unit` と同じ経路を保つ理由: `unit` と未定の変数で答えが変わる判定は `direct_context` だけである（`src/check.rs` に期待する型が `unit` かどうかで枝を選ぶ所は無い。起票時に `expected.*Type::Unit` で探した）。`if`・`match`・ブロックや `(unit -> 'a) -> 'a` の呼び出しは期待する型を下へ渡すので、今 `unit` を受け取る入れ子の本体は、代わりにヒントを受け取って同じく直接実行に進む。最後の式が `unit` なら型付きの木は今と同じで、違いは `annotation(unit)`・`require_use`（`_` には何もしない）を呼ばないことと推論変数が 1 つ増えることだけである。IR が同じことは手順 1 の比較で確かめる（S1）。

### 採らない案

- **W38 の `entry_value` のまま（最後の式を `None` で検査して）広げる**: t2・t1 の腕が素の `$implicit` の経路に進み、IR が変わる（s1・s2）。h1 は h2 と同じ誤りになる。
- **結果の型の無い本体をすべて `direct_root` に送る**: u1・u2 の IR が変わる（u2・u3）。
- **最後の式を型によらず値にする**: g2 や `unreachable ()` で終わる本体の型付きの木が変わり、使われないラムダは `E1015` になりうる（W38 の採らない案と同じ）。

## 前提とする他チケットのインターフェース

- [W38](W38-top-level-plain-do-misplaced-e1003.md) の `Checker::entry_value(prefix, value, statement, body, expected)` と、`implicit_root` の `$entry` の扱い（最後の文を除いて `implicit_prefix` に渡す）。W38 が別の形で入ったら、その形に、素の `$implicit` への拡張とヒントの 2 点だけを足す。
- **W38**: このチケットは `entry_value` を `last_value` に置き換え、`$entry` もヒントで検査する。W38 の設計のとおり最後の式を `None` で検査すると、t1（トップレベルの最後の `match` の腕に裸の `do` を含む本体）の腕が `direct_root` でなく素の `$implicit` の経路に進み、IR が変わりうる（s1・s2）。W38 の S1 がそれを捕まえて W38 自身がヒントを入れていれば、それを使う。そうでなければ、このチケットが `$entry` も直す（t1 をテストに入れる）。
- **W02**: `runs_directly` が `check_implicit` と `implicit_root` の両方で `direct_context` を呼ぶ。このチケットは `direct_context` の中身を変えるので、ヒントは両方に効き、`break`・`continue` の境界の判定と下げ方が食い違わない。変える行は重ならない。
- **W31**: `direct_controls` は `$entry` だけで、このチケットは広げない（n3 は「対象外」）。W31 と W38 が変えた `if builder.is_none()` の枝をこのチケットも変えるので、テキストは衝突する。推奨の順は W02 → W31 → W38 → W43 である。

## 他チケットへ提供するインターフェース

- なし。

## 実装手順

1. **変更**: ブランチ `fix/w43-last-value-after-bare-do`。基準の記録だけ。
   **内容**: 着手条件を確かめる。HEAD のコンパイラで、`tests/fixtures`・`examples` の `Main.tz` と「現状」の u1・u2・n5・t1・t2・h1・l2・l3 を `build --emit llvm -O1`（native と `--target wasm32`）にする。それを `check --json` の結果と一緒に `target/scratch/w43/before/` へ保存する。
   **確認**: 保存した数を控える。
2. **変更**: `tests/computations.rs` に新しいテスト（テスト計画）を先に書く。
   **内容**: テスト計画の Rust のとおり。受け入れる形は今の HEAD で失敗し、今と同じ形は通ることを確かめる。
   **確認**: `cargo test --locked --test computations last_value_after_bare_do` が `running 1 test` で失敗する。
3. **変更**: `src/check.rs` の欄、`src/computation.rs`（設計）。
   **内容**: 欄 `last_value_hints`、`direct_context` のヒントの判定、`entry_value` を置き換える `last_value`、`implicit_root` の `tail` の条件。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test computations` と `--test specs` の全件が通る。手順 1 の IR と `check --json` が `cmp` で一致する（S1）。
4. **変更**: E2E（`tests/computations.mjs`）と適合性テスト（`spec/rules/CTL.md`・`tests/conformance/CTL/`）。
   **内容**: テスト計画の E2E と適合性テストのとおり。
   **確認**: `node tests/computations.mjs target/debug/tsuzuri`。`node scripts/conformance.mjs target/debug/tsuzuri --rule CTL-0NN`（番号は着手時の次の番号）。
5. **変更**: 文書（下）。
   **内容**: 「ドキュメント」のとおり。
   **確認**: 変更したページの `node scripts/check-docs.mjs`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`。T1、T2。`target/scratch/w43/` を消す。

## テスト計画

### Rust（`tests/computations.rs`）

`last_value_after_bare_do_in_lambdas_and_blocks`: 既存の `accepts`（native と wasm32 の IR を作って決定的であることを見る）と `rejects`。

- 受け入れる: l1・q2・b1・i1・m1・a1・t3・g3・j2。
- 今と同じに受け入れる: u1・n5・t2・h1・t1・l2・l3。
- 拒否する（今と同じ）: g2 は `E1003`、h2 は `E1003`、j1 は `E1003`、n3 は `E1018`。数値リテラルの位置に出る誤りは、コードだけを固定する（位置は W44 が動かす）。

### E2E（`tests/computations.mjs`）

一時プロジェクトを `tsuzuri run` で `-O0` と `-O3` の両方で動かす。期待値は手で書く（GUIDE-LITE §2 規則 10）。

| 内容 | 期待 |
| --- | --- |
| q2 | 標準出力 `5\n`、標準エラーに `x`、終了コード 0 |
| t3 の最後を `do! IO.writeln (to_string (f (Maybe.Some 3)))` / `0` にした形 | 標準出力 `3\n` |
| i1 の最後を `do! IO.writeln (to_string v)` / `0` にした形 | 標準出力 `1\n` |
| j2 の最後を `do! IO.writeln (to_string (f true))` / `0` にした形 | 標準出力 `t\n1\n`（腕の `do!` を先に実行する） |

### 適合性テスト

- `spec/rules/CTL.md` に次の番号の規則を足す。文: 「In a lambda or block body without a result type that binds nothing and ends in an expression whose type is concrete or a number literal where it is written, bare `do expr` statements MUST NOT change the value of the body.」（g2 と n3 は規則の外。仕様 2 と「対象外」）
- `tests/conformance/CTL/CTL-0NN-bare-do-keeps-lambda-value.tz`（早見のプログラムで `let n = 41` / … / `n + 1` とした形。`// expect: stdout "42\n"`）。文書の規則の文の直前に `<!-- rule: CTL-0NN -->` を置く。

## ドキュメント

- `_tsuzuri/language-reference/computation-expressions/computation-expressions.md` の「裸の `do expr` は `unit` の通常式です」（386 行目付近。W38 がトップレベルの 1 文を足す）に、ラムダ・ブロック・`if` の枝・`match` の腕でも同じだと書く。`let!`・`do!` が無ければ、裸の `do` の後の最後の式が本体の値になる。最後の式の型が決まっていない（型の未定の引数など）ときは `unit` になるので、引数か結果を注釈する。`run=42` の例（適合性テストと同じプログラム）を 1 つ足す。
- `docs/language.md` の「ビルダーによるモナド合成を必要とせず…」（2499 行目付近）に同じ規則を 1 文足す。README・`compiler/diagnostics.md` は変えない（診断を足さない）。

## 受け入れ条件

- [ ] l1・q2・b1・i1・m1・a1・t3・g3・j2 が受け入れられ、E2E の表のとおり動く（`-O0`・`-O3`）。
- [ ] u1・n5・t2・h1・t1・l2・l3 を今と同じに受け入れ、g2・h2・j1・n3 は今と同じコードで拒否する。
- [ ] 今受け入れているプログラム（手順 1 の集合）の IR と `check --json` が、native と wasm32 で 1 byte も変わらない。
- [ ] `tests/computations.rs` の新しいテストと既存の全件、`tests/specs.rs`、`node tests/computations.mjs target/debug/tsuzuri` が通る。
- [ ] 適合性テスト、`check-spec --require-markers`、変更した全ページの `check-docs`、T2 が通る。

## 落とし穴

- **ヒントは必ず外す**: `last_value_hints.pop()` は `?` より前に置く（誤りで戻るときも外す）。入れ子のラムダの `last_value` は自分のヒントを積むので、`Vec` で重ねる。
- **推論変数が 1 つ増える**: 型は `finish` で解決するので IR には出ないはずだが、S1 の比較で確かめる。差が出たら、期待する型が未定の推論変数のときはそれをヒントにし、新しい変数を作らない形に変える。
- **開いた型は `unit`**: g2 の `x` のように、検査の時点で型が未定の最後の式は値にしない。値にすると、`unit` で使われていた受け入れ済みのラムダの型付きの木が変わる（S1）。

## 対象外

- **最後の文が `if`・`while`・`for` の文の本体**: n3 は、裸の `do` の後の `if c then 1 else 2` が `ComputationStatementKind::If` の文になり、`implicit_prefix` がそこで止まって `E1018` になる（`do` の無い同じラムダは受け入れる）。W31 の `direct_controls` は `$entry` だけで、ラムダに広げるとビルダーの `While` の規則と関わる（W31 の採らない案）。別のチケットの候補。
- **型が未定の最後の式**（g2）: 仕様 2 のとおり、今と同じく `unit` を返す。
- **`do` の無い同じ本体との、入れ子の束縛の違い**: 最後の式の値の位置にある入れ子の本体は直接実行なので（仕様 3）、j2 の腕の `do!` はその場で実行され、j1 の腕の `let! … Maybe` は `E1003` になる。`do` の無い同じラムダでは、腕は期待する型からビルダーを選ぶ計算になる（h2 は `IO<unit>` の値）。今の `unit` の形と同じ規則を保つために残す（テストで固定する）。
- **数値リテラルの制約の誤りがリテラルを指すこと**（q2 の `3:17`）: [W44](W44-literal-constraint-error-location.md) が直す。
- ビルダーで組み立てる計算の末尾の通常式に、暗黙の `return` を入れること（文書の表が否定している）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 束縛が無く式で終わる素の暗黙の本体では、最後の式を本体の値にする（W38 の `$entry` の規則を広げる） | 文書は裸の `do` を通常の `unit` 式と定める。`do` の無い同じ本体（l3）と注釈のある同じラムダ（l2）は値を返す。注釈の有無で意味が変わらないようにする | 既定案（実装者はこの案に従う） |
| D2 | 最後の式はヒントの推論変数を期待する型にして検査し、`direct_context` はそのヒントを期待する入れ子の暗黙の本体を直接実行にする。W38 の `entry_value` も `last_value` に置き換え、`$entry` も同じヒントで検査する | `unit` で検査していたときと同じ経路を保ち、t2・h1・t1 の意味と IR を変えない（`None` で検査すると s1・s2 の差が出て、h1 は誤りになる）。規則を 1 か所にする | 既定案（実装者はこの案に従う） |
| D3 | 最後の式の型が `unit`・`Type::Error`・数値リテラルでない未定の型なら、今と同じ `unit` の文と `()` にする | u1・u2 の IR を変えない（u2・u3 の比較）。g2 の形は残るが、受け入れていたプログラムを壊さない（W38 の D2 と同じ） | 既定案（実装者はこの案に従う） |
| D4 | 規則 `CTL-0NN`（着手時の次の番号）と、受け入れる形の適合性テスト 1 つを足す | 裸の `do` の意味は文書の規則で、外れたことを CI（`conformance.mjs`）で捕まえるため（D-58 の形式。W38 の D4 と同じ） | 既定案（実装者はこの案に従う） |
| D5 | n3（最後の文が `if` などの文）と g2 はこのチケットでは直さない | 原因が別（`implicit_prefix` が制御の文で止まる）か、受け入れていたプログラムを変えずには直せない。小さな修正の範囲を保つ | 既定案（実装者はこの案に従う） |
