# W44: 数のリテラルの制約が破れたときの `E1003` が、単一化した位置ではなくリテラルを指す

| 項目 | 内容 |
| --- | --- |
| ID | W44 |
| 優先度 | P3（誤りは誤りとして報告されるが、位置が原因の行から離れる。利用者は、正しい `let n = 0` の方を直そうとする） |
| 規模 | S |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。W01〜W41 の起票中に見つけ、利用者の依頼「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」に当たるので起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（コード `E1003`、kind `type_mismatch`、メッセージの文言は変えない。主の位置を、具体的な型どうしの不一致（「現状」の k5）と同じく、型を決めた単一化の位置にそろえる修正である。`docs/stability.md` は診断の「コードと位置の意味は保ちます」と定め、位置の意味（型の合わない式を指す）はこの修正でほかの `E1003` と同じになる。GUIDE-LITE §6「位置は問題の本体（名前・式）の `Span`」にも合う。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) と D-51） |
| 手本にする既存実装 | `src/polymorph.rs` の `Checker::finish` の `use_bindings` の枝（破れた制約を別の診断に作り直す）、`Inference::unifies` の `trial`（`unify` が解いた変数を記録して戻す）、`src/effects.rs` の `RelatedRole::RequiredBy` の注記（`required by this attribute`） |
| 主な影響ファイル | `src/polymorph.rs`（`Inference`・`Inference::unify`・新しい `Inference::integer_literal_default`・新しい `Inference::literal_site`・`Checker::finish`）、`src/check.rs`（整数リテラルの検査の `default_numeric` の呼び出し）、`tests/diagnostic_v2.rs`、`tests/parameter_conventions.rs`、`spec/rules/TYP.md`、`tests/conformance/TYP/`、`_tsuzuri/language-reference/types-and-type-inference/type-inference.md`、`values-and-functions/lambda-expressions.md`、`compiler/diagnostics.md`、`docs/language.md`、`docs/architecture.md` |

## 実装者向け早見

- 範囲: 接尾辞のない整数リテラル（`Checker::integer_literals` に位置を積むもの）の `Integer`・`SignedInteger` の制約が、`Checker::finish` で破れたときの `E1003`（`expected an integer, found T`）だけ。
- 原因: リテラルの型は未定の推論変数で、制約はリテラルの位置で記録される（`src/check.rs` の整数リテラルの検査の `self.require(…, span)` と `integer_literals.push(span)`）。後の単一化（`let u: unit = n`）は制約を見ずに変数を `unit` に解き、その位置はどこにも残らない。`Checker::finish` が最後に制約を検査し、`constraint.span`（リテラル）で報告する。
- 直し方: `Inference` が、整数リテラルの変数（と、それが単一化でつながった変数）を変数でない型に解いた `unify` の span を覚える。`finish` はその span を主の位置にし、リテラルを `related`（role `required_by`、文言 `required by this integer literal`）に回す。記録が無ければ、今と同じくリテラルを指す。
- 最初に読む: `Checker::finish` の制約の検査の loop → `Inference::unify` の `(Type::Infer(id), ty)` の腕（`self.solutions[*id] = Some(ty.clone())`）→ `Inference::unifies`（試しの単一化を戻す）→ `Inference::default_numeric`（整数と浮動小数点の両方のリテラルから呼ばれる）・`is_numeric_literal`。最初に書くテストは `tests/diagnostic_v2.rs` の `literal_constraint_points_at_the_deciding_use`。
- 触ってはいけない: 制約を検査する順序（どの誤りを報告するか）、メッセージの文言と kind、浮動小数点のリテラル（`Float` の `E1005`）、試しの単一化（`unifies`）の結果。
- 完了の見分け方: 次のプログラムが `3:19`（`let u: unit = n` の `n`）を指し、`note: required by this integer literal` が `2:13` の `0` を引く。

```tsuzuri
def main :: unit -> i32 = \() ->
    let n = 0
    let u: unit = n
    0
```

## 目的

型の不一致の `E1003` は、ふつう型が合わなかった式を指す。具体的な型どうしなら、`let u: unit = n` の `n` を指し、注釈を `expected_because` で引く（「現状」の k5）。ところが、`n` の型が接尾辞のない整数リテラルから来ていると、同じ誤りがリテラル（`let n = 0` の `0`）を指す。原因の行から離れ、`let` が離れていたり別の関数の呼び出しを経ていたりすると、利用者は何が型を決めたかを探せない。

リテラルの制約の誤りでも、主の位置を型を決めた単一化の位置にし、リテラルは関連する位置として示す。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W44`。「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。

### 停止条件

- **S1** 受け入れていたプログラムの IR・診断（警告を含む）が 1 byte でも変わる。
- **S2** `expected an integer, found …` 以外の診断の位置・文言・kind・順序が変わる（`tests/diagnostic_baseline` と全テスト）。
- **S3** 記録を整数リテラルの変数だけに限れず、すべての変数の解に span を持たせる必要が出る（`Inference` は `implicit_candidates` などで複製されるので、重くなる。設計を見直して報告する）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

`target/debug/tsuzuri.exe check`（`--json` も）で確かめた。作業は `target/scratch/w-w43w44/` で行い、確認後に消した。行は `/` で区切って書く。特に書かない限り `def main :: unit -> i32 = \() ->` の本体の中で、最後は `0` である。

| # | コード | 今の結果 |
| --- | --- | --- |
| n1 | `let n = 0` / `let u: unit = n`（早見のプログラム） | `2:13: error[E1003]: expected an integer, found unit`（`0` を指す。JSON に `related` は無い） |
| k8 | `let n = 0` / `n` | `2:13`（同じ文言） |
| k3 | `let n = 2` / `let y: f64 = n` | `2:13: … expected an integer, found f64` |
| k4 | `let n = 2` / `let y: bool = n` | `2:13: … expected an integer, found bool` |
| k2 | `let n = -1` / `let u: i32u = n` | `2:13: … expected an integer, found i32u`（文言は「対象外」） |
| k6 | `def takes :: string -> i32 = \s -> 0` の後、`main` の本体が `let n = 7` / `takes n`（`0` なし） | `4:13: … expected an integer, found string` |
| k7 | `let xs = [1, 2, 3]` / `let s: [string] = xs` | `2:15`（最初の要素 `1`） |
| d3 | トップレベルの `let xs = [1, "a"]` / `0` | `1:11`（`1`） |
| CTL-002 | `tests/conformance/CTL/CTL-002-all-branches-of-if.tz`（`if flag then 1 else "one"`） | `2:14`（`1`） |
| d1 | `type-inference.md` の「単相性のルール」の例（`let f = \x -> x` / `let a = f 42` / `let b = f "hello"`） | `3:15`（`42`）。文書の注記は 4 行目（`f "hello"`）に付いている |
| d4 | edition 2027 の `export def answer :: i64 = { let ys = [1, 2, 3] \|> Array.map (\(x: ref string) -> x.length); ys.length }` | `1:40`（最初の要素 `1`） |
| ab | `let n = -2` / `let m = abs n` | `2:13: … expected an integer, found f64` |
| k5 | `let n: i64 = 5` / `let u: unit = n`（具体的な型） | `3:19: error[E1003]: expected unit, found i64`、`note: the type annotation declares this type`（`3:12`）、JSON に `expected`・`found` |
| k1 | `let x = 1.5` / `let u: unit = x`（浮動小数点のリテラルは `f64` に決まっている） | `3:19`（k5 と同じ形） |
| e1 | トップレベルの `for i = 0 to 9.5 do` | `1:14: error[E1005]: no instance for Float<i32>`（「対象外」） |

### 原因（コード）

- `src/check.rs` の整数リテラルの検査（`expected.is_none_or(polymorph::is_unknown)` の枝）: 期待する型が未定なら、リテラルの型は推論変数である。`self.require("Integer" | "SignedInteger", ty, span)` で制約をリテラルの位置に積み、`default_numeric(&ty, Type::I32)` と `integer_literals.push(span)` を行う。
- `src/polymorph.rs` の `Inference::unify`: `(Type::Infer(id), ty)` の腕が `self.solutions[*id] = Some(ty.clone())` で変数を解く。クラスの制約は見ず、`span` も残さない。変数を解くのはこの腕と、`apply_defaults`（既定の型）と、`unifies` の戻し（解いた変数を `None` に戻す）だけである。
- `src/polymorph.rs` の `Checker::finish`: 制約を解決した型で `classes.validate` し、破れたら `integer_literals.contains(&constraint.span)` の枝で `expected an integer, found …` を `constraint.span` に作る。最初に破れた制約で戻る。
- 診断を検査した `tests/` の中で、この文言を引くのは `tests/anonymous_unions.rs` の `no_injection_without_expected_type`（文言だけ）と、`tests/parameter_conventions.rs` の `annotated_reference_callbacks_over_number_literals`（4 行の表で `error.span.start == source.find('[') + 1`、つまりリテラルの位置を固定する）だけである。`tests/diagnostic_baseline/baseline.jsonl` には無い。`check-error E1003 line=N` を書いた適合性テストを開発版で検査し、この文言になるのは CTL-002 だけだった（位置は同じ行のまま動く）。`src/hints.rs` の 40 個の `bad` の例にも、この経路に当たるものは無かった。

## 仕様

1. 接尾辞のない整数リテラルの `Integer`・`SignedInteger` の制約が `finish` で破れたとき、主の位置は、そのリテラルの型（の推論変数）を変数でない型に決めた単一化の位置にする。リテラルの位置は `related` に、role `required_by`、文言 `required by this integer literal` で置く。
2. コード `E1003`、kind `type_mismatch`、メッセージ `expected an integer, found T` は変えない。`expected`・`found` と `expected_because` は足さない。
3. 単一化の位置が分からないときは、今と同じくリテラルを指し、`related` を付けない。記録が無い、幅が 0、リテラルと別のソース、リテラルと同じ位置のときがこれに当たる。
4. 制約を検査する順序と、どの制約の誤りを報告するかは変えない。ほかの制約の誤り、浮動小数点のリテラル、整数リテラルの即時の誤り（期待する型が決まっていた場合）は変えない。
5. 新しい位置は、その型を決めた式である。n1 は k5 と同じ `3:19` の `n`、k8 は `3:5` の `n`、k6 は `takes n` の `n`、CTL-002 は `"one"`、d1 は `"hello"` になるはずである（単一化が値の式の span で行われるため。実装の手順 1 で確かめる。未確認）。

## 設計

`src/polymorph.rs` の `Inference` に 2 つの欄を足す。どちらも整数リテラルの変数の分だけ要素を持つ（S3）。

```rust
/// W44: the open types of number literals and the variables that they became (`unify` binds a
/// variable to another), so that `literal_sites` is kept for them alone.
literal_roots: BTreeSet<usize>,
/// W44: where a variable of `literal_roots` took a type that is no variable: the span of that `unify`.
literal_sites: BTreeMap<usize, Span>,
```

- `src/check.rs` の整数リテラルの検査の `self.inference.default_numeric(&ty, Type::I32)` を、新しい `Inference::integer_literal_default(&ty)` に置き換える。これは `default_numeric(ty, Type::I32)` を行い、その変数を解決した根（`resolve` した結果の `Type::Infer(root)`）を `literal_roots` に入れる。`default_numeric` そのものは浮動小数点のリテラル（`Type::F64`）からも呼ばれるので、その中では入れない（2 つの欄を整数リテラルの分だけにする。S3）。リテラルの型は、期待する型が未定なら `expected.cloned()` で、すでに別の変数へつながった変数のことがある（`polymorph::is_unknown` は解決せずに `Type::Infer` を見るだけ）。`unify` は解決した根だけを解くので、元の変数だけを入れると記録が残らない。
- `unify` の `(Type::Infer(id), ty)` の腕で、`solutions` に入れた直後に次を行う。試しの単一化（`self.trial.is_some()`）は戻されるので記録しない。

```rust
if self.trial.is_none() && self.literal_roots.contains(id) {
    match ty {
        Type::Infer(next) => { self.literal_roots.insert(*next); }
        _ => { self.literal_sites.insert(*id, span); }
    }
}
```

- 新しい `Inference::literal_site(&self, ty: &Type) -> Option<Span>`: `ty` から `solutions` の変数のつながりをたどり、各変数で `literal_sites` を引く。変数でない解に着いたら `None` を返す。
- `Checker::finish` の制約の loop: 解決する前の型を手元に残す（失敗したときだけ使い、成功する制約には複製を足さない）。`integer_literals` の枝で `literal_site` を引き、仕様 3 の条件を満たせば、`Diagnostic::of(DiagnosticKind::TypeMismatch, message, site).related(RelatedRole::RequiredBy, "required by this integer literal", constraint.span)` を作る。満たさなければ今と同じ形にする。

`finish` が返すのは最初に破れた制約の 1 件だけなので、重複除去の鍵（位置と文言）が変わっても、ほかの診断には影響しない。`related` の役割は D-51 の閉じた語彙の `RequiredBy` で、新しい kind・コードは足さない。

### 採らない案

- **単一化の時点で制約を検査し、その場で誤りにする**（`Checker::same` で、リテラルの変数が解けたら `Integer` を確かめる）: 位置は正しくなるが、誤りを出す時点が `finish` から検査の途中へ移る。そのため、複数の誤りがあるプログラムで、どの誤りが先に出るかと回復の結果が変わる（S2）。`implicit_candidates` のような複製した推論の試しにも影響しうる。
- **文言を `expected unit, found an integer` の向きに変える**: k5 と向きがそろうが、記録した `unify` には期待する側がどちらかの情報が無い。文言を変えるとテストと文書の書き換えが増える。文言は保証の対象外なので、要るなら別に直す。
- **すべての推論変数の解に span を持たせる**: `Inference` の複製が重くなる（S3）。

## 前提とする他チケットのインターフェース

- なし。

### W38・W43 との関係

- [W38](W38-top-level-plain-do-misplaced-e1003.md) の「対象外」の行（数値リテラルの制約の誤りがリテラルの位置に出ること）と、[W43](W43-plain-do-in-lambda-returns-unit.md) の q2（`3:17`）は、このチケットを指す。両チケットのテストは、リテラルの位置に出る誤りをコードだけで固定する。どちらが先に入ってもよく、変えるファイルも重ならない。

## 他チケットへ提供するインターフェース

- `Inference::literal_site`（整数リテラルの型を決めた単一化の位置）。ほかの診断（たとえば浮動小数点のリテラルの `Float` の誤り）を同じ形に直すときに使える。

## 実装手順

1. **変更**: ブランチ `fix/w44-literal-constraint-location`。基準の記録だけ。
   **内容**: 着手条件を確かめる。`tests/fixtures`・`examples` の `check --json` と、`tests/diagnostic_baseline` の今の結果を `target/scratch/w44/before/` へ保存する。
   **確認**: 保存した数を控える。
2. **変更**: テスト（テスト計画）を先に書く。
   **内容**: テスト計画の Rust のとおり（新しいテストと単体テスト。既存のテストの期待値は手順 3 で直す）。
   **確認**: `cargo test --locked --test diagnostic_v2 literal_constraint` が `running N tests`（N は 1 以上）で失敗する。
3. **変更**: `src/polymorph.rs`・`src/check.rs`（設計）、`tests/parameter_conventions.rs`。
   **内容**: 2 つの欄、`integer_literal_default`、`unify` の記録、`literal_site`、`finish` の主の位置と `related`。`parameter_conventions.rs` の 4 行はテスト計画のとおり直す。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test diagnostic_v2 --test parameter_conventions --test anonymous_unions --test hints` が、テスト計画のとおり直した期待値で通る。仕様 5 の各位置を、手で数えた列と比べる。手順 1 の結果と比べ、この文言の診断以外が変わらない（S1・S2）。
4. **変更**: 適合性テスト（`spec/rules/TYP.md`・`tests/conformance/TYP/`）と文書（下）。
   **内容**: テスト計画の適合性テストと「ドキュメント」のとおり。
   **確認**: `node scripts/conformance.mjs target/debug/tsuzuri --rule TYP-0NN` と CTL-002、変更したページの `node scripts/check-docs.mjs`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`。T1、T2。`target/scratch/w44/` を消す。

## テスト計画

### Rust

- `tests/diagnostic_v2.rs` の `literal_constraint_points_at_the_deciding_use`（新しい定数 `LIT1` などを足し、`at_line`・`text_at`・`line_of` を使う）:
  - n1: 主の位置が 3 行目の `n`、`related` が 1 件、role `RelatedRole::RequiredBy`、文言 `required by this integer literal`、位置の字句が `0`、2 行目。文言は今と同じ `expected an integer, found unit`。
  - k6（引数）・k8（式の文）・CTL-002 の形（`if` の兄弟の枝）: 主の位置の字句がそれぞれ `n`・`n`・`"one"`、`related` の字句がリテラル。
  - JSON（同じファイルの `check_cli` で `--json`）: n1 の行が `"related":[{"role":"required_by",…}]` を持ち、`expected`・`found` のキーを持たない。
- `src/polymorph.rs` の単体テスト `literal_site_follows_the_bound_variables`: リテラルの変数を別の変数へ、その変数を具体的な型へ解いたとき、後者の `unify` の span が返る。`integer_literal_default` の前にすでに別の未定の変数へつながっていた変数をリテラルの型にした場合も、根を具体的な型へ解いた span が返る。`unifies` の試しで解いた場合は `None` が返る。
- 直す既存のテスト: `tests/parameter_conventions.rs` の `annotated_reference_callbacks_over_number_literals` の 4 行。主の位置は、要素の型を参照の型に決めた単一化の位置になる。`annotated_reference_callbacks_over_nested_number_literals` と同じく、注釈したパターンのはずである（未確認。`pattern_offset(&source, "map (\\")` で表せるかを手順 3 で確かめる）。リテラルの位置（`source.find('[') + 1`）は `related` の位置として確かめる。直前のコメント（「the literal reports what it reported before」）も直す。期待値はソースの文字列の位置から書き、コンパイラの出力を写さない（GUIDE-LITE §2 規則 10）。
- 変わらないもの: `tests/anonymous_unions.rs` の `no_injection_without_expected_type`（文言だけを固定）、CTL-002（`line=2` のまま）、`tests/diagnostic_baseline`。

### 適合性テスト

- `spec/rules/TYP.md` に次の番号の規則を足す。文: 「When a later expression decides that the type of an unsuffixed integer literal is a type that is not an integer, the E1003 MUST point at that expression and name the literal as a related location.」（記録の無い場合の戻り（仕様 3）は、後の式が型を決めていない場合なので、規則と矛盾しない）
- `tests/conformance/TYP/TYP-0NN-literal-conflict-points-at-use.tz`（早見のプログラム。`// expect: check-error E1003 line=3`）。文書の規則の文の直前に `<!-- rule: TYP-0NN -->` を置く。

## ドキュメント

- `_tsuzuri/language-reference/types-and-type-inference/type-inference.md` の「数値リテラルの双方向推論と既定型」に段落を足す。後の使い方でリテラルの型が整数でない型に決まると `E1003` になり、位置はその型を決めた式、リテラルは関連する位置（`note: required by this integer literal`）である。早見のプログラムの `text` の出力例を付ける。「単相性のルール」の例は、注記の位置（`f "hello"` の行）が新しい位置と一致することを確かめ、要れば「リテラル `42` は関連する位置に出る」と足す。この例の関数名 `test` は予約語で、そのままでは `E0002` になる（起票時は `t` に変えて確かめた）ので、名前も直す。
- `values-and-functions/lambda-expressions.md` の「リテラルの位置に `E1003 expected an integer, found ref string` のように報告されます」（123 行目付近）を直す。新しい位置は要素の型を決めた位置（手順 3 で確かめた位置）で、リテラルは関連する位置になる。
- `compiler/diagnostics.md` の `E1003` の説明（337 行目付近）に 1 文足す。注釈のない整数リテラルの型が整数でない型に決まったときは、その型を決めた式を主の位置にし、リテラルを `required_by` に持つ。
- `docs/language.md` の接尾辞のない整数リテラルの段落（4497 行目付近）に同じ 1 文を足し、1796 行目付近の「リテラルの位置の `E1003 …` は変わりません」を直す。`docs/architecture.md` の 1624 行目付近の同じ記述も直す（書いてあることが誤りになるため）。

## 受け入れ条件

- [ ] n1・k8・k6・CTL-002 の形・d1 が、仕様 5 の位置を主の位置にし、リテラルを `required_by` の `related` に持つ。文言・コード・kind は今と同じ。
- [ ] 単一化の位置が分からない場合は今と同じ形になる（単体テスト）。
- [ ] この文言以外の診断と、受け入れていたプログラムの IR が変わらない（手順 1 との比較、`tests/diagnostic_baseline`）。
- [ ] `tests/diagnostic_v2.rs` の新しいテスト、直した `tests/parameter_conventions.rs`、T1 の全件が通る。
- [ ] 適合性テスト、`check-spec --require-markers`、変更した全ページの `check-docs`、T2 が通る。

## 落とし穴

- **試しの単一化を記録しない**: `Inference::unifies` は解いた変数を `None` に戻す。そこで記録すると、使われなかった試しの位置が残る。`trial` の間は `literal_roots` にも足さない。
- **変数どうしの単一化の向き**: リテラルの変数が別の変数に解かれることも、その逆もある。前者では行き先を `literal_roots` に足し、`literal_site` はつながりを最後までたどる。リテラルの型がはじめからつながった変数のこともあるので、`integer_literal_default` では根を入れる。浮動小数点のリテラルの変数は入れない。
- **解決する前の型を失わない**: `finish` は `constraint.ty = resolve(…)` で制約の型を上書きする。その前の変数から `literal_site` を引く。
- **ヒントの規則**: `src/hints.rs` は主の位置の字句で規則の当たりを決める。起票時の検査では当たる例は無かったが、`tests/hints.rs` を回して確かめる。
- **複製の費用**: 2 つの欄は整数リテラルの数だけ育つ。`implicit_candidates` は `Inference` を複製するので、すべての変数に広げない（S3）。性能の数字は書かない。

## 対象外

- **負のリテラルの文言**: k2 は `i32u` が整数なのに `expected an integer, found i32u` と出る（破れたのは `SignedInteger`）。文言は保証の対象外なので直せるが、位置の修正とは別の改善（別のチケットの候補）。
- **浮動小数点のリテラルの `Float` の誤り**: e1 の `E1005`（`no instance for Float<i32>`）もリテラルを指す。コードが `E1003` でなく文言の作り方も違うので、このチケットでは扱わない。`Inference::literal_site` で同じ形に直せる（別のチケットの候補）。
- `finish` で出す誤りに、注釈の `expected_because` や `expected`・`found` を足すこと。
- 型推論の規則と、どの制約の誤りを先に報告するか。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 主の位置を、リテラルの型を変数でない型に決めた単一化の位置にし、リテラルは `related`（`RequiredBy`、`required by this integer literal`）に回す | 具体的な型どうしの `E1003`（k5）と同じく型の合わない式を指し、利用者が原因の行を直せる。リテラルの情報も失わない | 既定案（実装者はこの案に従う） |
| D2 | 記録は `Inference` に、整数リテラルの変数とそのつながりの分だけ持ち、検査の時点は `finish` のままにする | どの誤りを先に出すかと回復の結果を変えない（S2）。複製の費用を抑える（S3） | 既定案（実装者はこの案に従う） |
| D3 | メッセージの文言・コード・kind は変えず、`expected`・`found` も足さない | 変更を位置だけにし、文言を固定したテストと文書の記述を保つ。向きの情報は記録に無い | 既定案（実装者はこの案に従う） |
| D4 | 位置が分からないときは今の形（リテラルを指す）に戻す | 生成した式や試しの単一化で span が無い場合にも、誤りの位置を失わない | 既定案（実装者はこの案に従う） |
| D5 | 規則 `TYP-0NN`（着手時の次の番号）と適合性テスト 1 つを足す | 位置の規則を CI（`conformance.mjs`）で捕まえるため（D-58 の形式） | 既定案（実装者はこの案に従う） |
