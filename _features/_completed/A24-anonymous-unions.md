# A24: 匿名共用体 `(A | B)`（型で区別する共用体・期待型での注入・型テストのパターン `:?`）

| 項目 | 内容 |
| --- | --- |
| ID | A24 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | A02, A03, A05 |
| 後続 | A25, A26 |
| 状態 | done（2026-11-08。下の「実装と検証」） |
| 起票 | 2026-11-02（利用者の依頼「F# の Anonymous Type-tagged Unions と同様の機能の実装計画も追加して」。HEAD `2f556480`） |
| 承認 | 値の作り方（期待型で自動的に包む）は、2026-11-02 に利用者が選んだ（GUIDE D-62 の決定 1）。要素数の上限 32（D9）と、注入は値の使用で読み取りの引数には所有値が要ること（D10）は、2026-11-02 の利用者の回答「1~4 について承認します」で承認済み（D-62 の決定 2・3）。ほかの決定事項は既定案 |
| 手本にする既存実装 | 合成した union 宣言: `Parser::implicit_active_union`（`src/parse_control.rs:74-167`。`$` を含む名前と `Provenance::Generated`）。期待型で選ぶ構築: `Dyn.of`（`Checker::dyn_of`、`src/check.rs:13376`）と数のリテラル（`Checker::integer`、`src/check.rs:13111`）。case のパターン: `Checker::case_pattern`（`src/control.rs:796`） |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/anonymous_union.rs`（新規）, `src/check.rs`, `src/control.rs`, `src/polymorph.rs`, `src/exhaustiveness.rs`, `src/diagnostic_kinds.rs`, `src/explain.rs`, `src/explain/codes/E1041.md`（新規）, `src/docgen.rs`, `src/api_diff.rs`, `src/contracts.rs`, `src/warnings.rs`, `src/computation.rs`, `src/regions.rs`, `src/semantic.rs`, `src/semantic_api.rs`, `src/find.rs`, `src/prove.rs`, `src/llvm_debug.rs`, `src/kg_capture.rs`, `tests/anonymous_unions.rs`（新規）, `tests/fixtures/anonymous_unions/Main.tz`（新規）, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_tsuzuri/language-reference/`（下記「ドキュメント」）, `README.md` |

## 実装者向け早見

- 範囲: 型 `(A | B)`（正規化・正準順序・要素の規則）、期待型での注入、型テストのパターン `:? T [as x]`、網羅性の `missing: :? T`、名前の隠蔽、整形、`E1041`。共用体どうしの変換は A25、`Eq`・`Ord`・`Hash`・`Display` は A26。
- 最初に読む（この順）: ① GUIDE D-62 ② 決定事項 D1〜D9 ③ `src/check.rs` の `Checker::expression`・`finish_expression`（10702〜10900 行）と `argument`・`place_argument`・`temporary_argument`・`coerce_argument`（11834〜12100 行）④ `src/control.rs` の `case_pattern`（796〜880 行）⑤ `src/parse_control.rs` の `implicit_active_union`（74〜167 行）⑥ `src/check.rs` の `check_modules_collect` の `union_declarations`（6909〜6918 行）と `collect_unions`・`check_union`（8814〜9014 行）。
- 最初に書くテスト: `tests/anonymous_unions.rs` の `parses_anonymous_union_types` → `members_commute_and_flatten` → `injects_at_annotated_bindings` → `type_test_binds_member_type`（E2E は `tests/features.mjs` の suite `anonymous_unions`）。
- 触ってはいけない: 匿名共用体を書かないプログラムの IR・診断・`kg`・`doc`・`api-diff` の出力（byte 単位。手順 12）。既存の期待型の使い方（呼び出しの結果の先行単一化、リテラルの型付け）。`Type` の導出 `Ord`（要素の並べ替えに使わない）。
- 外から借りるもの: なし。
- 完了の見分け方: 次が `tsuzuri run` で `int 42` を出し、腕を 1 つ消すと `E1021 match is not exhaustive; missing: :? bool` になる。

```tsuzuri
def describe :: (i64 | string | bool) -> string = \value ->
    match value with
    | :? i64 as n -> $"int {n}"
    | :? string as s -> s
    | :? bool as b -> if b then "yes" else "no"

let first: (string | bool | i64) = 42
describe first
```

- 迷ったら: 決定事項 → D-62 → 名前付きの union の既存の挙動に合わせる → 最も保守的な選択（拒否）。

## 目的

F# の [FS-1092 Anonymous Type-tagged Unions](https://github.com/fsharp/fslang-design/blob/main/RFCs/FS-1092-anonymous-type-tagged-unions.md) に相当する型を入れる。
要素の型そのものがケースを表す共用体を、宣言なしで `(i64 | string)` と書き、期待される型が匿名共用体の位置では値をそのまま渡せるようにする。
取り出しは型テストのパターン `:? T as x` で行い、`match` の網羅性を検査する。

```text
def font_size :: (f64 | i64 | string) -> string = \size ->
    match size with
    | :? f64 as points -> $"{points}pt"
    | :? i64 as pixels -> $"{pixels}px"
    | :? string as text -> text

font_size 12.0        // "12pt"
font_size 10          // "10px"
font_size "small"     // "small"
```

F# の RFC が挙げる 2 つの用途（オーバーロードの代わりに「どれかの型」を受け取る API と、異種のデータの列 `[(string * (i64 | f64 | string))]`）を書けるようにする。
ラベルに意味があるデータ、再帰する構造、同じ型を 2 回持つケースは、従来どおり名前付きの `union` で表す（F# の RFC の Guidance と同じ。言語リファレンスにも書く）。

このチケットは型・構文・注入・型テスト・網羅性・表示までを入れる。匿名共用体どうしの変換（`(A | B)` を `(A | B | C)` へ渡す、`:? (A | B) as x` で絞る）は A25、`Eq`・`Ord`・`Hash`・`Display` は A26 で入れる。

## 着手条件と停止条件

### 着手条件

- A02（union）・A03（網羅性）・A05（型別名）が done であること（HEAD で done）。確認: `grep -n "^| A0[235] " _features/README.md`。
- GUIDE D-62 があること（このチケットと同時に起票済み）。決定 1（期待型で自動的に包む）は利用者の承認済み。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 合成した宣言（D2）を `collect_unions`・`check_union` に通すと、既存のテストの期待値（IR・診断・`kg`・`api-diff`・`doc` の出力）が 1 byte でも変わる。匿名共用体を書かないプログラムの出力は変えない。
- 擬似モジュール `$anonymous` を `Names::modules` に登録すると、モジュール一覧（`doc`・`kg`・`facts`・LSP のワークスペースシンボル）に現れ、`$` を含む名前の除外（手順 9）で消せない。
- `Type` の大きさ（4 語）、`TypeExpr`・`Pattern` の大きさを変えないと新しい variant を足せない（`src/syntax.rs:759` の注記と GUIDE §3.1 の stack の回帰テスト）。
- 注入（D4）を入れると、匿名共用体を使わないソースの型検査の結果（推論した型・診断のコードと位置）が変わる。
- `exhaustiveness.rs` の witness に型を通す変更（D7）で、既存の `missing:` の文言が変わる。
- `unsafe`、新しい crate、新しい WASM import、新しい予約語が必要になった。

## 現状（HEAD `2f556480` で確認）

- 型の構文 `TypeExprKind`（`src/syntax.rs:759-791`）に共用体の形はない。`type_primary` の `(` は括弧のまとめだけで（`src/parser.rs:2513-2516`。`type_expr` を読んで `)` を要求する）、`|` に出会うと `E0002 expected ')' after the type`（kind `expected_token`）。
- パターン `PatternKind`（`src/syntax.rs:1329-1344`）に型テストはない。`pattern_atom`（`src/parse_control.rs:1338-1378`）は `:` で始まるパターンを持たず `E0002 expected a pattern`（kind `expected_pattern`）。
- 字句: `?` は型付き穴のトークン `Question`（G25。`src/syntax.rs:163-164`、`src/lexer.rs:1101`）。`:?` は `Colon`・`Question` の 2 トークンになり、字句の誤りにはならない。`||` は `OrOr`、`|>` は `PipeForward`（`src/lexer.rs:1077`・`1080`）。
- パターンの型注釈 `(p: T)` は `:` の後を `type_product`（括弧の中では `type_expr`）で読み、その後も演算子の解析を続けるので、`(x: i64 | string)` は `Or(Annotated(x, i64), Binding(string))` になる（`src/parse_control.rs:1316-1324`）。
- 型の解決は `resolve_type_with_kinds`（`src/check.rs:9027-9263`）。型別名は構文のまま展開してから解決し直す（`expand_type_aliases`、`src/check.rs:9416`）。型別名の宣言の検査も右辺を `resolve_type` で解決する（`src/check.rs:7000-7030`）。
- union は `Type::Union(id, args)`（`src/check.rs:154`）。`id` は全モジュールの `program.unions` を順に並べた番号（`check_modules_collect` の `union_declarations`、`src/check.rs:6909-6918`）で、`collect_unions`（`8814`）が名前を、`check_union`（`8949`）が `CheckedUnion`（`3834-3846`）を作る。宣言は単相化しない（`TypeContext::union_payloads`・`union_payload` が型引数で置き換える。`src/check.rs:1170`・`1186`）。
- 合成した union の先例: 多ケースのアクティブパターンは、パーサーが `Active{関数}$result` という名前（`$` は識別子に書けない。`src/lexer.rs:628-636`）の `UnionDecl` を `program.unions` に足す（`src/parse_control.rs:74-167`）。
- 期待型: `Checker::expression(expression, expected)`（`src/check.rs:10702`）が入口で、`finish_expression`（`10832`）が最後に `same`（`10880`）で期待型と単一化する。束縛の注釈・関数の本体（署名の結果）・ラムダの本体・呼び出しの引数（`Checker::argument`、`11834`）・レコードのフィールド・タプルの要素（期待型が同じ長さのタプルのとき）・配列とリストの要素・`if` と `match` の腕・ブロックの結果が期待型を受ける。呼び出しは引数より先に結果を期待型と単一化する（`11116-11126`）。パイプの左辺は、新しい推論変数を期待型にして先に検査する（`self.argument(left, &open)`、`11226-11227`）。右辺が呼び出しなら左辺の型を `self.piped` に置き（`11228-11230`）、`call_expression` の `apply_piped`（呼び出しは `11131`、関数は `11970-11998`）が、残りの最初の引数の型（`ref U` なら `argument_hint` が取り出した `U`）と左辺の型を**引数の検査より前に** `same` で単一化する。その後で `coerce_argument`（`12052`）が引数の型へ合わせる（`11239`）。
- 読み取りの引数（A18）は期待型が `ref T` で、場所の式は `place_argument`（`11906`。期待型なしで検査してから `coerce_argument`）、一時値は `temporary_argument`（`12003`。中身の型を期待型にして検査してから借用）を通る。
- 数のリテラル: 接尾辞のない整数は、期待型が浮動小数点なら浮動小数点として読み（`let r: f64 = 2`）、`bigint` なら `bigint`、整数ならその型、未定なら `Integer` の制約付きの推論変数（既定 `i32`）になる（`Checker::integer`、`13111-13158`）。`Inference::is_numeric_literal`（`src/polymorph.rs:570`）が「まだ型の決まらない数のリテラル」を判定する。
- case のパターン: `case_pattern`（`src/control.rs:796-880`）は対象を自動で参照外しし（`autoderef`）、`Type::Union(id, 新しい推論変数)` と単一化して、タグの比較 `UnionTag == Int(case_id)` とペイロードの射影 `UnionPayload` を作り、網羅性へ `CoveragePat::Constructor(Constructor::Union { union_id, case_id }, ..)` を渡す。
- 網羅性の witness は型を持たない。`witness`（`src/exhaustiveness.rs:333-387`）は case 名を `case_name`（`404-411`。`names.case_path` で引けなければ `Module.Case`）で書き、修正候補 `| {witness} -> unreachable ()` の可否は `writable`（`315-331`）で決める。`MatchCoverage`（`87-94`）は対象の型を持たない。
- 型の表示 `Type::display`（`src/check.rs:441`。union は `479-494` で `type_display(&unions[id].name)` と `<args>`）。診断・hover・デバッグ情報の型名（`src/llvm_debug.rs:379`）はこれを使う。
- 利用者の instance の head は `Classes::instances`（`src/polymorph.rs:1734`）で解決し、組み込みクラスの組み込みインスタンスの上書きを `1861-1871` で拒否する（`E1016` kind `builtin_instance_override`）。
- 診断コードの最大は `E1039`（`src/diagnostic_kinds.rs:674`）。GUIDE D-55 では `E1040` が TZ03・TZ12・TZ13 に割り当て済みで、`E1041` 以降は予約（このチケットで `E1041` を割り当てる。D-62）。

### 再現（検証済み）

`cargo build --locked` した dev ビルドで、各ソースを `Main.tz` として `tsuzuri check` した結果。

| ソース | HEAD の結果 |
| --- | --- |
| `let v: (i64 \| string) = 1` | `E0002 expected ')' after the type`（1 行 13 列。`\|` の位置。kind `expected_token`） |
| `match v with` の腕 `\| :? i64 -> 0` | `E0002 expected a pattern`（`:` の位置。kind `expected_pattern`） |
| `let x: (i64 \|\| string) = 1` | `E0002 expected ')' after the type`（`\|\|` の位置） |
| `match v with` の腕 `\| (x: i64 \| string) -> 0` | `E1020 both sides of an OR pattern must bind the same names` |
| `type U = (i64)` と `let v: U = 1` | 通る（括弧は型をまとめるだけ） |
| `type X = (` 改行 `i64` 改行 `)` | 通る（型の括弧の中は改行してよい） |

## 仕様

### 構文: 型

```text
type-primary = ... | "(" type { "|" type }+ ")"
```

- `(` の後に型を読み、`|` が続けば匿名共用体、`)` が続けば従来どおり括弧のまとめ。要素は 2 個以上。括弧は省略できない（`i64 | string` は書けない。union 宣言・`match` の腕と区別するため）。
- 各要素は型全体（`type_expr`）。`->` と `*` は `|` より強く結ぶ: `(i64 -> i64 | string)` は `((i64 -> i64) | string)`、`(i64 * i64 | string)` は `((i64 * i64) | string)`。読みやすさのため、関数型の要素は括弧で囲むことを言語リファレンスで勧める（強制はしない）。
- 先頭の `|` は書けない（`(| i64 | string)` は既存の「型が要る」位置の `E0002`）。
- 要素の間の `||` は `E0002`（kind `anonymous_union_double_bar`、メッセージ `use a single '|' between the members of an anonymous union, as in (i64 | string)`、位置は `||`）。`|>` は既存の `E0002 expected ')' after the type` のまま。
- 括弧の中の改行は既存の `(T)` と同じ扱い（HEAD で通る）。複数行では `|` を行頭に揃えて書ける:

```text
type Value =
    ( i64
    | f64
    | string )
```

- 型を書ける位置ならどこにでも書ける（束縛・署名・引数・レコードのフィールド・union のペイロード・型引数 `Maybe<(i64 | string)>`・型別名・`dyn` 以外の型の中）。

### 構文: パターン

```text
pattern-atom = ... | ":?" type-product [ "as" binder ]
```

- `:?` は `:` と `?` を空白なしで続けて書く。`: ?` と離して書けば既存の `E0002 expected a pattern`（新しい kind は足さない）。
- `:?` の後の型は `type_product`（タプルは括弧なしで書けるが、関数型と匿名共用体は括弧が要る）。
- `as binder` は型テストのパターンの一部で、`binder` は小文字の名前か `_`。束縛の型はテストした要素の型（共用体ではない）。`(:? i64 as n) as whole` と書けば、`whole` は共用体の値全体を束縛する（既存の `pattern as name`）。
- 型テストは 1 つのパターンの原子なので、`| :? i64 | :? i32 -> ...` は 2 つの型テストの OR。`| :? i64 as n | :? i32 as n -> ...` は束縛の型が違うので、既存の OR パターンの検査で `E1003`（kind `type_mismatch`。HEAD で `| (x, _) | (_, x) -> 0` を `(i64 * string)` に書いたときと同じ）。
- 関数ガードでも書ける（`\v -> | :? i64 as n -> ... | :? string as s -> ...`）。`guard_predicate`（`src/parse_control.rs:1224`）は、`:` の直後に空白なしの `?` が続く腕をパターンとして扱う。
- `match` 以外のパターンの位置（`let`、ラムダの引数、`for`）にも書け、一致しなければ既存の規則どおり実行時にトラップする。

### 型の正規化と要素の規則

`(T1 | ... | Tn)` は次の順で 1 つの型にする。

1. 各要素を解決する（型別名は展開済み）。
2. 平坦化する: 要素が匿名共用体なら、その要素で置き換える（`(i64 | (string | bool))` は `(i64 | string | bool)`）。型別名を経た入れ子も同じ。
3. 要素の検査（違反は最初の 1 件を報告。位置は匿名共用体の型の式全体）:
   - `Type::Error` を含む → 結果を `Type::Error` にする（報告済み）。
   - `Type::contains_reference`（`src/check.rs:738`。名前付きの union のペイロードと同じ述語）→ `E1013`（kind `anonymous_union_borrowed_member`）。
   - 型変数そのもの（`Type::Variable`）、または頭が型変数の型（`Type::Application`・`Type::Partial`）→ `E1041`（kind `anonymous_union_type_variable`）。
4. 要素を下の「正準順序」で並べる。決められない組（「不明」）があれば `E1041`（kind `anonymous_union_indistinct_members`）、等しい組があれば `E1041`（kind `anonymous_union_duplicate_member`）。
5. 要素数が 32 を超えれば `E1017`（kind `anonymous_union_member_limit`）。
6. 結果は `Type::Union(合成した宣言の id(n), 並べた要素)`（D2）。

この規則から次が成り立つ（F# の RFC の「可換・結合・平坦化」と同じ）:

- `(A | B)` と `(B | A)` は同じ型。`(A | (B | C))` と `((A | B) | C)` も同じ型。どこで書いても同じ型で、モジュールをまたいでも等しい。
- `type Meters = f64` のとき `(Meters | f64)` は要素の重複で `E1041`。
- `('a | i64)` は `E1041`。`type OrInt<'a> = ('a | i64)` も宣言の検査で `E1041`（F# の `type StringOr<'a> = ('a | string)` の拒否と同じ）。
- `(Maybe<'a> | i64)` は書ける（頭の `Maybe` と `i64` で区別できる）。`(Maybe<'a> | Maybe<i64>)` は「不明」で `E1041`。`(Maybe<i64> | Maybe<string>)` は書ける。
- 単相化（型引数の置き換え）で要素の順序と区別は変わらない（「不明」を拒否したので、順序を決めた位置に型変数はない）。要素の型変数が匿名共用体に置き換わっても、要素そのものは型変数でないので平坦化は起きない。

### 正準順序

`order(a, b)` は `Less`・`Equal`・`Greater`・`Unknown` を返す。表示（`Type::display`）も、タグの番号も、この順。

1. `a` か `b` が `Variable`・`Infer`・`Partial`・`Application`・`Error` なら `Unknown`。
2. 頭の種類の順位を比べる。順位は `Type` の宣言順（`src/check.rs:133-177`）と同じで、`Integer` < `Binary` < `Decimal` < `Simd` < `Bool` < `Unit` < `Char` < `Utf8Char` < `String` < `Utf8String` < `Record` < `Union` < `Array` < `ArrayView` < `FixedArray` < `Length` < `List` < `Vec` < `Tuple` < `Task` < `Handle` < `Function` < `Reference` < `Dyn` < `Shared`。違えばそれで決まる。
3. 同じ種類なら次で比べる。複数の値は辞書式で、最初に `Equal` でない結果（`Unknown` を含む）で決まる。
   - `Integer(bits, signed)`: `(bits, signed)`（`false` < `true` なので `i8u` < `i8` < `i16u` < `i16` < ...。符号なしの型名は `i64u` など。`src/numeric.rs:7-10`）。`Binary(bits)`・`Decimal(bits)`: `bits`。`Simd(t)`: `SimdType` の導出 `Ord`。
   - `Bool`・`Unit`・`Char`・`Utf8Char`・`String`・`Utf8String`: `Equal`。
   - `Record(id, args)`・`Union(id, args)`: 宣言の修飾名（`Module.Name`。`Names` の新しい表、D2）の文字列順、次に `args`。匿名共用体どうしも同じ規則（修飾名は `$anonymous.Union$n`）。
   - `Array`・`ArrayView`・`List`・`Vec`・`Task`: 要素。`FixedArray(e, n)`: `e`、次に `n`。`Length(n)`: `n`。
   - `Tuple(xs)`: 要素数、次に要素。`Handle(name)`: 名前の文字列順。
   - `Function(params, result)`: 引数の数、各引数、結果、引数規約の列（`ParameterConvention` の導出 `Ord`）、`pure`（`false` < `true`）。
   - `Reference(t, mutable)`: `mutable`（`false` < `true`）、次に `t`。`Dyn(d)`: `DynType` の導出 `Ord`。`Shared(t, kind)`: `kind`、次に `t`。

`id` ではなく修飾名で比べるのは、モジュールを足したときに番号がずれても順序（表示とタグ）を変えないため（D3）。

### 合成する宣言

- プログラムのどれかのモジュールが匿名共用体の型か型テストのパターンを書いていれば（パーサーが `Program::uses_anonymous_unions` を立てる。D2）、要素数 n = 2〜32 の 31 個の union 宣言を、利用者の union の後ろに足す。書いていなければ何も足さない（出力は 1 byte も変わらない）。
- n 個の要素の宣言は、`visibility: Visibility::Public`（どのモジュールの型からも使う。`check_union` の `validate_public_type` は型変数だけのペイロードなので通る）、モジュール `$anonymous`、名前 `Union$n`、型引数 `$t0`〜`$t{n-1}`、ケース `Union$n$k`（`k` 番目のペイロードは `$t{k}`）。修飾名は `$anonymous.Union$n`。`$` は識別子に書けないので、利用者の名前と衝突せず、ソースから参照もできない。
- 擬似モジュール `$anonymous` は `Names::modules` に `ModuleOrigin::Std` で登録する（std のコードからも見える。`Drop`・`Format` の利用者インスタンスは既存の規則で拒否される）。

### 値の作り方（注入）

期待される型が匿名共用体 `U` の位置で、式の型が `U` の要素のどれか 1 つ `M` に決まるとき、式の値を `M` のケースとして包んだ `U` の値にする（型付き IR は既存の `TypedExprKind::Construct { union_id, case_id, payload }`。`Some x` と同じ構築）。
これは GUIDE §1 の規則 2（暗黙の変換をしない）の、D-62 で定めた唯一の例外。値は変えず、タグを付けるだけ。

期待型を受ける位置は既存のまま（上の「現状」）。判定は次の順。

1. **期待型を下へ渡す式**（`if`・`match`・ブロック・`ExprKind::ComputationBoundary`）は、従来どおり `U` を腕・結果へ渡す。腕ごとに下の 2 以降が働く。`if c then 1 else "x"` は `U = (i64 | string)` なら両方の腕が注入される（F# の例と同じ）。
2. **形で要素を先に選ぶ式**: 次の式は、選んだ要素 `M` を期待型にして検査してから包む。選べなければ 3 へ進む。
   - 接尾辞のない整数のリテラル（`ExprKind::Integer(_, None)`、または `ExprKind::Unary(UnaryOp::Negate, ..)` の中のそれ。`Checker::untyped_number`（`src/check.rs:13103-13109`）は浮動小数点のリテラルと、`-` 以外の単項演算子の中も真にするので、そのままは使わず、同じ形の小さな関数 `literal_class` を足して `Integer`・`Float`・なしを返す）: 整数の要素、なければ `bigint` の要素、なければ浮動小数点の要素（`Type::is_float`。`decimal` を含む）の順に、最初に空でない組を見る。1 つならそれ。2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_literal`。`(i32 | i64 | string)` の `16` は `i32` か `i64` か決まらない。help は接尾辞 `16i64` を示す）。どれもなければ 3 へ。
   - 接尾辞のない浮動小数点のリテラル（`literal_class` が `Float`）: 浮動小数点の要素が 1 つならそれ、2 つ以上なら同じ kind の `E1041`、なければ 3 へ。
   - ラムダ: 関数型の要素がちょうど 1 つならそれ（引数の型を注釈しなくてよい）。
   - `Dyn.of` の一引数の直接適用: `dyn` 型の要素がちょうど 1 つならそれ。
3. **それ以外の式**は、期待型なしで検査し、得た型 `T`（`resolve` 後）で決める。
   - `T` が匿名共用体 → 包まずに `U` と単一化する（等しくなければ `E1003`。A25 で部分集合からの拡大を足す）。
   - `T` が `Type::Error` → そのまま。
   - `T` が匿名共用体への参照（`ref V`）→ そのまま（引数の位置では `coerce_argument` が再借用し、ほかの位置では既存の `E1003`）。
   - `T` が数のリテラルでない未定の推論変数 → 包まずに `U` と単一化する（`let v: (i64 | string) = Parse.parse text` は `'a = (i64 | string)` になる。F# の「型の情報がない式は変換しない」と同じ）。
   - それ以外は、`may_member(T, M)`（D5。推論変数はどの型とも合いうる、ただし数のリテラルの推論変数は数の型とだけ）を満たす要素 `M` を数える。1 つなら `T` を `M` と単一化し（`finish_expression` を `M` を期待型にして通し、関数値の `pure` の調整なども行う）、包む。0 なら `E1003`（kind `anonymous_union_not_member`。メッセージ `expected {U}, found {T}; {T} is not a member of the anonymous union`、help に要素の一覧）。2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_member`。`(Maybe<i64> | Maybe<string>)` に `None` を渡したときなど。help は型の注釈を示す）。
4. 引数の位置では、期待型が `U` でも `ref U`（読み取りの引数）でも上の 1〜3 で `U` の値を作り、`coerce_argument` が借用する（一時値の借用。`f (Some x)` と同じ）。パイプの左辺（`x |> f`）は、`coerce_argument` が引数の型 `U`・`ref U` に合わせるときに 3 の判定をする。そのために `apply_piped` は、残りの最初の引数の型が `U` か `ref U` で、左辺の型が `U`・`ref U`・数のリテラルでない未定の推論変数のどれでもないとき、`same` を呼ばずに戻る（左辺の型と `U` の単一化は `coerce_argument` の判定に任せる）。

注入しないもの（すべて既存の `E1003` などのまま）:

- 入れ物・関数の中: `[i64]` を `[(i64 | string)]` へ、`i64 -> unit` を `(i64 | string) -> unit` へは変えない（共変・反変はない）。
- 借用: `ref i64` を `ref (i64 | string)` へは変えない（表現が違う）。
- 期待型のない位置: `let xs = [1, "a"]` は `E1003`（F# の注釈なしの例が無効なのと同じ）。
- 型引数の推論: `'a` を「要素の型」から「共用体」へ広げない。

比較演算子の右辺は、左辺の型を期待型として受ける（`binary_expression`、`src/check.rs:11240-11262`）ので、`v == 1`（`v: (i64 | string)`）の `1` は注入される。比較には A26 の `Eq` が要る（A24 では `E1005`）。

### 型テストのパターン

`:? T` と `:? T as x` は、対象の値が匿名共用体 `U` で、`T` が `U` の要素のとき、その要素のケースかどうかを調べる。

- 対象の型は、パターンを検査する時点で匿名共用体に決まっていなければならない（参照は既存の case のパターンと同じく自動で外す）。未定の推論変数なら `E1041`（kind `type_test_unknown_scrutinee`。help: 引数か束縛に型を注釈する）。匿名共用体でない型なら `E1041`（kind `type_test_not_anonymous_union`。help: 名前付きの union はケース名で照合する）。
- `T` は本体の型注釈と同じ規則で解決する（`Checker::annotation`、`src/polymorph.rs:3761`。署名にない型変数は既存の `E1015`）。`T` が匿名共用体なら `E1041`（kind `type_test_union`。A25 で受理に変える）。`T` が要素のどれとも等しくなければ `E1041`（kind `type_test_not_member`。help に要素の一覧）。
- 照合は case のパターンと同じ型付き IR（タグの比較と `UnionPayload` の射影）。`as x` はペイロードを `x` に束縛し、所有権の規則（所有する対象からは move、借用の先・コレクションの要素からは読み取り専用のビュー、ガードの中は別名）は case のペイロードの束縛と同じ。
- 網羅性: 要素ごとのケースとして数える。すべての要素を（ガードなしで）テストすれば網羅的。足りなければ `E1021 match is not exhaustive; missing: :? string` で、修正候補は `| :? string -> unreachable ()`（型の表記は `Type::display`。その位置で解決できない表記なら、`tsuzuri fix` の再検査（G24）が失敗して書かない）。同じ要素の 2 回目のテストは既存の `W1003`（到達不能）。

### 型の性質

匿名共用体は合成した generic な union の具体化なので、次はすべて名前付きの union の既存の規則がそのまま決める。

| 性質 | 規則 |
| --- | --- |
| Copy・Capture・Send・Sync・drop の要否 | 要素から構造的に（`Type::is_copy` などの union の腕） |
| レイアウト | 要素数のタグ（32 以下なので 1 byte）と最大の要素の領域（`llvm_layout.rs` の `work_out_union_layout`）。全要素の LLVM の型が同じなら `Common`（`(i64 \| i64u)`）。ペイロードのないケースがないので niche（タグなし）にはならない |
| 再帰 | 匿名共用体が名前付きの型の循環に入れば（`union Tree = Leaf \| Node of (i64 \| Tree)` のような形）、名前付きの union と同じくヒープのノードになる。ペイロードのないケースがないので、どの値もノードを確保する |
| 大きさの上限 | 既存の 64 KiB（`E1010`）を具体型ごとに |
| export・extern | 既存の `E1008`（メッセージの型は `(i64 \| string)` と表示） |
| GPU カーネル | 既存の `E1018` |
| 型クラス | A24 では組み込みのインスタンスを持たない（`==`・表示・ハッシュは `E1005`。A26 で足す）。利用者は匿名共用体を head とする `instance` を書けない（`E1041`、kind `anonymous_union_instance`。A26 で足す組み込みインスタンスと重ならないようにするため。D8） |
| `deriving` | 匿名共用体をフィールドに持つレコード・union の `deriving (Eq, ...)` は、匿名共用体の該当インスタンスがない（A24）ので既存の `E1005`／`E1025` |

### 評価順序・所有権・借用

- 注入する式は 1 回だけ評価し、その後で包む（`Some e` と同じ）。左から右の評価順は変わらない。
- 注入は値の使用（`Some x` と同じ）: Copy の値は複製、非 Copy の値は共用体の中へ move する。読み取りの引数に場所（`f name`、`name: string`）を渡すと、`name` は共用体へ move され、後で使えば既存の `E1012`（A17 の最後の使用なら何も起きない）。言語リファレンスの「注意点」に書く。
- 読み取りの引数（A18 の既定）の値は借用なので（D10。承認済み）、非 Copy の型の読み取りの引数を、そのまま匿名共用体の引数へ渡すことはできない（`finish_expression` が参照外しした値を共用体へ move しようとして、既存の所有権の誤りになる。`Some s` と同じ）。引数を `own` にするか、値を複製して渡す。言語リファレンスの「注意点」に書き、テスト `read_parameter_needs_owned_value` で固定する。
- 型テストの束縛は case のペイロードの束縛と同じ（上記）。

### native と WASM

型付き IR は既存の union の構築・照合だけなので、native と WASM、`-O0` と `-O3` で同じ結果。新しいランタイム関数・intrinsic・import はない。

### 診断

| コード | kind | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- | --- |
| `E0002` | `anonymous_union_double_bar` | 匿名共用体の要素の間に `\|\|` | `use a single '\|' between the members of an anonymous union, as in (i64 \| string)` | `\|\|` |
| `E1013` | `anonymous_union_borrowed_member` | 要素が借用を含む | `an anonymous union holds owned values; {member} is borrowed` | 型の式 |
| `E1017` | `anonymous_union_member_limit` | 平坦化後の要素が 33 個以上 | `an anonymous union has at most 32 members, but this one has {n}; declare a union with named cases instead` | 型の式 |
| `E1003` | `anonymous_union_not_member` | 注入する式の型がどの要素でもない | `expected {U}, found {T}; {T} is not a member of the anonymous union` と help `the members are {m1}, {m2}, ...` | 式 |
| `E1041` | `anonymous_union_type_variable` | 要素が型変数か、頭が型変数 | `a type variable cannot be a member of an anonymous union: {U}; members must differ without knowing type arguments` と help `use a union with named cases, such as union OrInt<'a> = Value of 'a \| Int of i64` | 型の式 |
| `E1041` | `anonymous_union_indistinct_members` | 2 つの要素の順序が「不明」 | `{A} and {B} may be the same type for some type arguments, so they cannot both be members of an anonymous union` | 型の式 |
| `E1041` | `anonymous_union_duplicate_member` | 2 つの要素が等しい | `{A} appears more than once in the anonymous union {U}` と help `a type alias names the same type as its definition` | 型の式 |
| `E1041` | `anonymous_union_ambiguous_literal` | リテラルが同じ組の 2 つ以上の要素になりうる | `the literal {text} could be {m1} or {m2} in {U}` と help `add a suffix, for example {text}{m1}` | リテラル |
| `E1041` | `anonymous_union_ambiguous_member` | 式の型が 2 つ以上の要素と合いうる | `this value of type {T} could be {m1} or {m2} in {U}` と help `annotate the value, for example with a typed let binding` | 式 |
| `E1041` | `anonymous_union_instance` | `instance` の head が匿名共用体 | `instances cannot be declared for the anonymous union {U}; wrap it in a record, or declare a union with named cases` | クラス名 |
| `E1041` | `type_test_unknown_scrutinee` | `:?` の対象の型が未定 | `the type of the matched value must be known to use ':?'` と help `annotate the parameter or binding with its anonymous union type` | `:?` |
| `E1041` | `type_test_not_anonymous_union` | `:?` の対象が匿名共用体でない | `':?' tests a member of an anonymous union, but the matched value has type {T}` と help `match the cases of a named union by name` | `:?` |
| `E1041` | `type_test_union` | `:?` の型が匿名共用体 | `':?' names one member of {U}; write one ':?' arm for each member` | 型 |
| `E1041` | `type_test_not_member` | `:?` の型が要素でない | `{T} is not a member of {U}` と help `the members are {m1}, {m2}, ...` | 型 |
| 既存 | — | move 後の使用、網羅性、OR の束縛、export、インスタンスなし、大きさ | 既存のコードとメッセージ（`E1012`・`E1021`・`E1020`／`E1003`・`E1008`・`E1005`・`E1010`） | 既存 |

`E1041` の説明（`tsuzuri explain E1041`、言語リファレンスの診断の表）は「匿名共用体の要素の規則・注入の曖昧さ・型テストの誤り」。

### 資源上限

- 要素は平坦化後に 32 個まで（`anonymous_union::MAX_MEMBERS`。超えると `E1017`）。合成する宣言は 31 個で、ケースは合計 527 個。
- 型の大きさの既存の上限（構築子 4,096・深さ 128。`polymorph::bounded_type`）は匿名共用体にもかかる。正準順序の比較はこの上限の中の型だけを辿る。
- OR 展開の上限 1,024 は変えない（型テストは 1 つの代替）。

### 例

```tsuzuri
def describe :: (i64 | string | bool) -> string = \value ->
    match value with
    | :? i64 as n -> $"int {n}"
    | :? string as s -> s
    | :? bool as b -> if b then "yes" else "no"

let items: [(i64 | string | bool)] = [1, "two", true]
let first: (string | bool | i64) = if items.length > 2 then 42 else "none"
describe first        // "int 42"（(string | bool | i64) と (i64 | string | bool) は同じ型）
```

| ソース | 結果 |
| --- | --- |
| `let v: (i64 \| string) = 42` | 通る。`v` の型は `(i64 \| string)` |
| `let v: (string \| i64) = 42` と `let w: (i64 \| string) = v` | 通る（同じ型） |
| `let v: (i32 \| i64 \| string) = 16` | `E1041 anonymous_union_ambiguous_literal` |
| `let v: (f64 \| string) = 2` | 通る（`2` は `f64`） |
| `let v: (i64 \| string) = true` | `E1003 anonymous_union_not_member` |
| `let xs = [1, "a"]` | 既存の `E1003`（期待型がない） |
| `let v: (i64 \| i64) = 1` | `E1041 anonymous_union_duplicate_member` |
| `def f :: ('a \| i64) -> i64 = ...` | `E1041 anonymous_union_type_variable` |
| `def f :: (Maybe<'a> \| Maybe<i64>) -> i64 = ...` | `E1041 anonymous_union_indistinct_members` |
| `let v: (ref string \| i64) = 1` | `E1013 anonymous_union_borrowed_member` |
| 腕が `:? i64` だけの `match v with`（`v: (i64 \| string)`） | `E1021 match is not exhaustive; missing: :? string` |
| `match 1 with \| :? i64 -> 0 \| _ -> 1` | `E1041 type_test_not_anonymous_union` |
| `match v with \| :? bool -> 0 \| _ -> 1`（`v: (i64 \| string)`） | `E1041 type_test_not_member` |
| `v == 1`（`v: (i64 \| string)`） | `E1005`（`Eq<(i64 \| string)>` がない。A26 で通る） |
| `export def f :: (i64 \| string) -> i64 = ...` | 既存の `E1008` |
| `instance Display<(i64 \| string)> { ... }` | `E1041 anonymous_union_instance` |

## 設計

### データ構造

- `src/syntax.rs`:
  - `TypeExprKind::AnonymousUnion(Box<[TypeExpr]>)`（書いた順の要素。`Box<[_]>` で `TypeExpr` を大きくしない）。
  - `PatternKind::TypeTest(Box<TypeExpr>, Option<Ident>)`（`as` の束縛。`_` は `None` にせず `Ident("_")` のまま持ち、検査器がワイルドカードとして扱う）。
  - `Program::uses_anonymous_unions: bool`（パーサーが、上の 2 つのどちらかを作ったときに立てる）。
- `src/anonymous_union.rs`（新規。`src/check.rs:12-32` の `exhaustiveness` などと同じく `#[path = "anonymous_union.rs"] mod anonymous_union;` で検査器の子モジュールにする）:
  - `pub const MODULE: &str = "$anonymous";`、`pub const MAX_MEMBERS: usize = 32;`
  - `pub fn declarations() -> Vec<UnionDecl>`: n = 2..=32 の宣言（上の「合成する宣言」。`Ident` は `Provenance::Generated`、span は `Span::default()` 相当の既存の値。`parse_control.rs:93-166` を手本にする）。
  - `pub fn declaration_name(n: usize) -> String`（`"Union$n"`）、`pub fn is_anonymous(union: &CheckedUnion) -> bool`（`union.module() == MODULE`）。
  - `pub enum Order { Less, Equal, Greater, Unknown }` と `pub fn order(a: &Type, b: &Type, names: &Names) -> Order`（上の「正準順序」）。
  - `pub fn canonicalize(members: Vec<Type>, names: &Names, span: Span) -> Result<Type, Diagnostic>`（「型の正規化」の 2〜6。挿入ソートで十分: 要素は 32 個まで）。
  - `pub fn display(members: &[Type], types: &TypeContext) -> String`（`(m1 | m2)`。要素が `Type::Function` なら `(...)` で囲む）。
- `Names`（`src/check.rs:4777`）に `union_names: Vec<String>` と `record_names: Vec<String>`（id → 修飾名）を足す。`collect_unions` の `union_arities.push`（`8858`）と、レコードの `record_arities.push`（`6873`）の隣で同じ順に push する。`anonymous_unions: Vec<usize>`（`n - 2` 番目が要素数 n の宣言の id。合成しなければ空）も足す。
- `MatchCoverage`（`src/exhaustiveness.rs:87`）に `scrutinee: Type` を足す（D7）。

### 段ごとの変更

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 構文 | `src/parser.rs:2513-2516`（`type_primary` の `(`） | 1 つ目の `type_expr` の後で、`Pipe` なら要素を読み続けて `AnonymousUnion` を返す。`OrOr` なら `anonymous_union_double_bar`。`uses_anonymous_unions` を立てる（パーサーの状態に持ち、`Program` を作るときに写す） |
| 構文 | `src/parse_control.rs:1338`（`pattern_atom`） | `Colon` の直後に `Question` があり、2 つの span が隣接（`colon.span.end == question.span.start`）なら、2 トークンを読み、`type_product`、続く `as` と名前を読んで `TypeTest`。旗を立てる |
| 構文 | `src/parse_control.rs:1224`（`guard_predicate`） | 先頭で、隣接した `Colon`・`Question` なら `false`（パターン） |
| 構文 | `src/syntax.rs:1349`・`1372`（`visit_expressions_mut`・`visit_expressions`） | `TypeTest` は式を持たない腕 |
| キャッシュ | `src/syntax_codec.rs` | `TypeExprKind::AnonymousUnion` を tag 18、`PatternKind::TypeTest` を tag 14 として `put`（`1368`・`2120`）と `get`（`1452`・`2189`）に足し、拒否の範囲を `19..=u8::MAX`（`1483`）・`15..=u8::MAX`（`2210`）にする。`Program` の codec に `uses_anonymous_unions` を足す。テスト `round_trips_every_variant`（`2533`。数の表 `2651`・`2654` と corpus）と `pins_encoding_format`（`2873`。`GOLDEN`）を直す |
| キャッシュ | `src/frontend_cache.rs` の `FRONTEND_FORMAT` | 1 増やす |
| 構文 | `src/api_diff.rs:639`・`src/kg_capture.rs:373`（`let Program { .. }` の分解。`..` なし） | `uses_anonymous_unions` のフィールドを足す（使わないので `uses_anonymous_unions: _`） |
| 整形 | `src/formatter.rs:803`（`Canonical::ty`）・`1071`（`Canonical::pattern`） | 新しい腕（子を辿る） |
| 整形 | `src/formatter.rs:653-740`（`spacing`） | 隣接した `Colon`・`Question` の間は空白なし。その `Colon` の前は、直前が `(` でなければ空白 1 つ（`\| :? i64`、`(:? i64 as n)`）。型の中の `\|` の前後は空白 1 つ（`(i64 \| string)`、`Maybe<(i64 \| string)>`） |
| 型の解決 | `src/check.rs:9027`（`resolve_type_with_kinds`） | `AnonymousUnion` の腕: 各要素を同じ関数で解決し、`anonymous_union::canonicalize` |
| 型の構文の走査 | `src/check.rs:6226`（`validate_public_type`）・`9450`（`TypeAliasExpansion::expand`。`_` の腕が `9544` にあるので**腕を足さないと入れ子の別名が展開されない**）・`9648`（`substitute`）・`9699`（`reject_expanded_field_constraints`）、`src/contracts.rs:778`、`src/warnings.rs:481`、`src/docgen.rs:69`（`type_text`。`(A \| B)` と書いた順で出す）、`src/api_diff.rs:454`・`761`・`1300`・`1376`、`src/regions.rs:8`、`src/semantic.rs:1344`、`src/polymorph.rs:1684` | 要素を子として辿る腕を足す |
| パターンの走査 | `src/warnings.rs:719`、`src/computation.rs:340`（そのまま返す）、`src/control.rs:149`・`893`、`src/diagram/transitions.rs:110`、`src/parse_control.rs:598` | 腕を足す（`_` の腕で正しく扱えるものはそのまま。確かめた結果を PR に書く） |
| 合成 | `src/check.rs:6909-6918`（`union_declarations`） | どれかのモジュールの `uses_anonymous_unions` が真なら、`anonymous_union::declarations()` をこの関数の局所変数に持ち、`(MODULE, &decl)` を後ろに足す。その前に `names.modules.insert(MODULE, ModuleOrigin::Std)`。`collect_unions` の後で `names.anonymous_unions` を埋める |
| 注入 | `src/check.rs:10702`（`Checker::expression`） | 期待型が匿名共用体で、式が「期待型を下へ渡す式」でなければ、`union_value` の結果を `finish_expression` に通して返す（`finish_recovery` は通す） |
| 注入 | `src/check.rs:11834`（`Checker::argument`） | 最初の分岐: 期待型が `U` か `ref U`（共有）なら、`union_value` の結果を `coerce_argument` に渡す |
| 注入 | `src/check.rs:12052`（`coerce_argument`） | 先頭: 期待型が `U` か `ref U` で、値の型が `U`・`Reference(U, _)`・数のリテラルでない未定の推論変数のどれでもなければ `conform_union`（数のリテラルの推論変数は `conform_union` に渡す。渡さないと `1 \|> f` で推論変数が `U` に束縛され、注入されない） |
| 注入 | `src/check.rs:11970-11998`（`apply_piped`） | 残りの最初の引数の型（`resolve` 後。`ref U` なら中身）が匿名共用体 `U` で、左辺の型が `U`・`Reference(U, _)`・数のリテラルでない未定の推論変数のどれでもなければ、`same` を呼ばずに `Ok(())` を返す |
| 型テスト | `src/control.rs:507`（`pattern_alternatives`。`PatternKind` の `match` は `528`） | `TypeTest` の腕から `type_test_pattern` |
| 型テスト | `src/control.rs:796`（`case_pattern`） | `note_case` とペイロードの数の検査の後を `union_case_pattern(span, union_id, case_id, payload: Option<&Pattern>, matched)` に切り出し、`case_pattern` と `type_test_pattern` から呼ぶ |
| 網羅性 | `src/exhaustiveness.rs` | `MatchCoverage` を作る唯一の位置（`src/control.rs:390`。`match_value`（`378`）が `MatchOrigin` で `match` と関数ガードの両方を扱う）で `scrutinee` を設定。`witness`・`missing_arms` に型を並べて渡す（D7） |
| インスタンス | `src/polymorph.rs:1861`（`Classes::instances`） | head が匿名共用体なら `anonymous_union_instance` |
| 表示 | `src/check.rs:479-494`（`Type::display`） | 匿名共用体なら `anonymous_union::display` |
| 名前の隠蔽 | 下の表 | `$anonymous` の名前を利用者に見せない |
| 診断 | `src/diagnostic_kinds.rs`、`src/explain.rs:353` の後、`src/explain/codes/E1041.md` | kind 14 個（`E0002` 1、`E1003` 1、`E1013` 1、`E1017` 1、`E1041` 10）を (code, id) 順に足す。`E1041` の説明を足す |

名前の隠蔽（`$anonymous` の宣言・ケースの名前が出うる位置）:

| 位置 | 変更 |
| --- | --- |
| `src/find.rs:457-479`（型付き穴の候補のケース）、`995-1090`（`value_pool` の case の別名 `1044-1048`・修飾した case `1049-1060`・モジュール `1062-1081`）、`1106-1110`（`member_pool` の case）、`1137-1145`（`function_pool` の case） | 既存の `hidden`（`src/find.rs:989-990`。`$` か `__` を含む名前）で除く。HEAD では関数（`1016`）と組み込み（`1033`）だけが `hidden` を通り、case とモジュールは通らない（`suggest`（`878`）自身は何も除かない）。除かないと `Anonymous` の綴り違いに `$anonymous` が候補に出る。テスト `suggestions_skip_synthetic_names` で確かめる |
| `src/semantic_api.rs:209-221`（`public_names`） | `$anonymous.` で始まる名前を除く |
| `src/polymorph.rs:2944`（`type_expression`。`Record`・`Union` の腕は `2994-3010`） | 匿名共用体なら `Apply(key_path("$anonymous.Union$n"), ..)` ではなく `TypeExprKind::AnonymousUnion(要素を type_expression で戻した列)` を返す（解決し直すと同じ正準の型になる）。使う側: `semantic_api.rs:26-39`（api-diff が読む署名と注釈）、`derive.rs:461`・`466`（導出の head と制約）。直さないと、api-diff の `public_name`（`api_diff.rs:424-429`）が `$anonymous.Union$2<…>` を出す |
| `src/semantic.rs:256-265`（`type_reference`）・`368-373`（`union_ids`）・`1305-1310`（`type_target`） | 匿名共用体は宣言への参照・定義への移動を作らない（`None`）。要素の型は従来どおり参照を作る |
| `src/exhaustiveness.rs:404-411`（`case_name`） | 匿名共用体では呼ばない（D7） |
| `src/prove.rs:363-366`（反例の表示） | 匿名共用体の値はペイロードだけを表示する |
| `src/check.rs:9840-9846`（`Layouts::union` の大きさの誤り。union の名前を出す唯一の診断） | 型名に `union.name` ではなく `Type::display` を使う（匿名共用体のとき） |
| `src/llvm_debug.rs:728-765`（`payload_union`）・`845-874`（`tag_type`） | 匿名共用体のメンバー名・列挙子名を要素の表示（`i64` など）にする |
| `src/kg/ids.rs:62`（関数）・`100`（union の宣言の id。`kg_capture.rs:800`・`1142` は `ids::type_id_text` を呼ぶだけ） | 匿名共用体は宣言の id を作らず、要素の型だけを辿る |

### 生成 IR

新しい命令の形はない。注入は `Construct`、型テストは `UnionTag` の比較と `UnionPayload` で、既存の union の emission（`llvm_layout.rs:566-617` の `construct`、`634-768` のタグ、`732` のペイロード）をそのまま通る。
LLVM の型名は既存の `canonical_type`（`src/llvm.rs:3821`。`unions[id].name` と `[args]`）で `%"tz.union.$anonymous.Union$2[...]"` のようになる。`mangled_type`（`src/llvm.rs:8014`）のエスケープ `$A`・`$L`・`$R`・`$C`・`$P`（A29 が足した dyn の印の `+`）と重ならないよう、合成する名前には `$` の直後に大文字の `A`・`L`・`R`・`C`・`P` を置かない（`$anonymous`・`Union$n`・`$t{k}` は満たす）。`mangled_type` の doc comment と `docs/architecture.md`（組み込み関数のシンボル）は、`canonical_type` に `$` が現れる場所を active pattern の union の名前だけとして単射を説明しているので、合成した名前を足すときはその説明も更新する。

### アルゴリズム

`union_value(expression, U)`（`Checker` のメソッド。`U` は `resolve` 済みの匿名共用体）:

```text
if expression is If | Match | Block | ComputationBoundary:
    return self.expression(expression, Some(U))           // 腕へ渡す。入口の判定は再帰しない
if let Some(k) = self.preselect(expression, U)?:          // リテラル・ラムダ・Dyn.of
    let value = self.expression(expression, Some(&members[k]))?
    return wrap(value, U, k)
let value = self.expression(expression, None)?
self.conform_union(value, U)
```

`conform_union(value, U)`:

```text
let found = resolve(value.ty)
if found is anonymous union or Error: return value     // 呼び出し側の finish_expression / coerce_argument が U と単一化する
if found is Reference(V, _) and V is anonymous union: return value   // 引数なら coerce_argument が再借用する
if found is Infer and not is_numeric_literal(found): return value
let candidates = [k for k, M in members(U) if self.may_member(found, M)]
match candidates:
    [k] => let value = self.finish_expression(value.kind, value.ty, Some(&members[k]), value.span)?
           wrap(value, U, k)
    []  => Err(E1003 anonymous_union_not_member)
    _   => Err(E1041 anonymous_union_ambiguous_member)
```

`wrap(value, U, k)` は `TypedExpr { kind: Construct { union_id: U の id, case_id: k, payload: Some(Box::new(value)) }, ty: U, span: value.span }`。

`may_member(found, member)`（D5）:

```text
let (a, b) = (resolve(found), resolve(member))
if a == b: true
(Error, _) | (_, Error): true
(Infer x, t) | (t, Infer x):
    if is_numeric_literal(Infer x): t is Integer | Binary | Decimal | Infer, or self.is_bigint(t)
    else: true
(Variable x, Variable y): x == y
同じ頭: 子を対にして all(may_member)。数・名前・id・可変性・kind・SharedKind・FixedArray の長さは等しいこと。関数は引数の数が等しいこと（規約と pure は見ない。finish_expression が調整する）
それ以外: false
```

`type_test_pattern(pattern, ty, binder, matched)`:

```text
let target = resolve(autoderef(matched).ty)
if target is Infer: Err(type_test_unknown_scrutinee)
let Type::Union(id, members) = target, is_anonymous(id) else Err(type_test_not_anonymous_union)
let tested = resolve(self.annotation(ty)?)
if tested is anonymous union: Err(type_test_union)
let k = members.position(|m| resolve(m) == tested)
     .or_else(|| unique position where may_member(m, tested), then same(m, tested))
     .ok_or(type_test_not_member)?
let payload = Pattern { kind: Binding(binder)（`_` なら Wildcard）, span: binder.span, depth: pattern.depth }
self.union_case_pattern(pattern.span, id, k, Some(&payload), matched)
```

## 実装手順

### 手順 1: ベースラインを取る

```sh
cargo build --locked
node scripts/slot.mjs -- cargo nextest run --locked > target/scratch/a24/baseline-tests.txt 2>&1
node tests/features.mjs target/debug/tsuzuri dyn_dispatch
node tests/features.mjs target/debug/tsuzuri deriving
```

`tests/fixtures/` の数本の `Main.tz` について `tsuzuri build --emit llvm` の IR を `target/scratch/a24/ir-before/` に保存する（手順 12 で、匿名共用体を書かないプログラムの IR が 1 byte も変わらないことを確かめる）。

### 手順 2: 構文と codec

`TypeExprKind::AnonymousUnion`・`PatternKind::TypeTest`・`Program::uses_anonymous_unions` を足し、パーサー・`guard_predicate`・codec・`FRONTEND_FORMAT`・`visit_expressions(_mut)` を直す。
コンパイルが通るまで、`TypeExprKind`・`PatternKind` を `_` なしで match する箇所（上の表）に腕を足す。
検査器の 2 つの腕（`resolve_type_with_kinds` と `pattern_alternatives`）の中身は手順 4 と手順 6 で書く。それまでは `todo!("A24 step 4")`・`todo!("A24 step 6")` を置き、別の意味の診断で代用しない（`todo!` は手順 6 までに消え、PR には残らない）。
`tests/anonymous_unions.rs` に構文のテスト（`parses_anonymous_union_types`・`parses_type_test_patterns`・`rejects_double_bar`）を足す。codec の往復は、`syntax_codec` と `frontend_cache` が `pub(crate)`（`src/lib.rs:35`・`63`）なので `tests/` からは呼べない。`src/syntax_codec.rs` の単体テスト `round_trips_every_variant` の corpus に `(i64 | string)` と `:? i64 as n` を足して確かめる。構文のテストは `tsuzuri::parser::parse`（`src/parser.rs:198`）で AST を見て、検査器を通さない。

### 手順 3: 合成する宣言と名前の表

`src/anonymous_union.rs` を作り、`declarations`・名前の定数・`Names::union_names`・`record_names`・`anonymous_unions` を足して、`check_modules_collect` で合成する。
テスト `synthesizes_declarations_only_when_used`（匿名共用体のないプログラムでは `types.unions.len()` が変わらない、あるプログラムでは 31 増える）。

### 手順 4: 型の正規化と表示

`order`・`canonicalize`・`display` と `resolve_type_with_kinds` の腕、`Type::display` の分岐、型の構文を辿る各関数の腕。
テスト: `members_commute_and_flatten`、`displays_in_canonical_order`、`rejects_type_variable_members`、`rejects_indistinct_members`、`rejects_duplicate_members_through_aliases`、`rejects_borrowed_members`、`limits_member_count`、`generic_members_stay_ordered_after_instantiation`（`def f :: (Maybe<'a> | i64) -> ...` を `'a = string` と `'a = (bool | f64)` で呼ぶ）。

### 手順 5: 注入

`union_value`・`preselect`・`conform_union`・`may_member` と 3 か所の入口（`expression`・`argument`・`coerce_argument`）。
テスト: `injects_at_annotated_bindings`、`injects_branches`、`injects_call_arguments_and_pipes`、`injects_read_parameters`、`injects_array_elements`、`injects_records_and_generic_constructors`（`None`・`Some 1`・`Pair { .. }`）、`literal_rules`（整数・浮動小数点・`bigint`・曖昧）、`lambda_takes_the_only_function_member`、`dyn_of_takes_the_only_dyn_member`、`no_injection_without_expected_type`、`no_injection_inside_containers`、`no_injection_through_references`、`injection_moves_the_value`（読み取りの引数へ渡した `string` の後の使用は `E1012`）、`read_parameter_needs_owned_value`、`references_pass_through`（`ref (i64 | string)` の値を読み取りの引数へ渡す）。この段の後で、手順 1 のベースラインの Rust テストがすべて通ることを確かめる（匿名共用体を使わないソースの推論は変わらない）。

### 手順 6: 型テストのパターン

`type_test_pattern`・`union_case_pattern` の切り出し。
テスト: `type_test_binds_member_type`、`type_test_views_through_references`、`type_test_rejections`（4 つの kind）、`type_test_in_function_guards`、`type_test_in_let_traps_at_run_time`。

### 手順 7: 網羅性の witness

`MatchCoverage::scrutinee` と、型を並べる `witness`・`missing_arms`（D7）。
テスト: `missing_member_is_named`（`missing: :? string`）、`missing_member_inside_maybe`（`missing: Some (:? string)`）、`missing_member_fix_is_written`（修正候補 `| :? string -> unreachable ()` を `tsuzuri fix` で適用して通る）、`duplicate_type_test_is_unreachable`（`W1003`）、`existing_missing_messages_unchanged`（`tests/` の既存の `E1021` の期待値が通る）。

### 手順 8: インスタンスの拒否

`Classes::instances` の分岐。テスト `rejects_instances_for_anonymous_unions`。

### 手順 9: 名前の隠蔽

上の表の各位置。テスト: `typed_holes_do_not_offer_synthetic_cases`、`suggestions_skip_synthetic_names`、`hover_shows_anonymous_union`（`semantic` の hover が `(i64 | string)`）、`effective_api_hides_synthetic_names`、`api_diff_treats_member_order_as_equal`（`(string | i64)` と `(i64 | string)` の版の差分がない）、`kg_has_no_synthetic_declarations`。

### 手順 10: 整形

`spacing` と `Canonical`。テスト（`tests/formatter.rs` に足す）: `formats_anonymous_unions`（`(i64|string)` → `(i64 | string)`、複数行の形を保つ）、`formats_type_tests`（`|:?i64 as n` → `| :? i64 as n`）、`format_is_idempotent_for_anonymous_unions`。

### 手順 11: E2E suite `anonymous_unions`

`tests/fixtures/anonymous_unions/Main.tz` と `tests/features.mjs` の suite `anonymous_unions`（`dyn_dispatch` の形。`export def` は `i64` の引数と結果）。関数と期待値:

| 関数 | 内容 | 期待値 |
| --- | --- | --- |
| `pick x` | `x` の符号で `(i64 \| string \| bool)` の 3 要素のどれかを作り、`match` で数に戻す（`i64` は値、`string` は長さ、`bool` は 0/1） | `pick 5 = 5`、`pick 0 = 4`（`"zero"`）、`pick -3 = 1`（`true`） |
| `mixed n` | `[(i64 \| string)]` に `n` 個の要素（偶数番目は `i64`、奇数番目は `string`）を入れ、`for` で合計 | `n = 0, 1, 4, 7` で手計算の値 |
| `owned n` | 非 Copy の要素（`string`・`[i64]`）を持つ共用体を move・drop する | 値と `live == 0` |
| `nested n` | `Maybe<(i64 \| string)>` と、匿名共用体を持つレコード | 値 |
| `recursive n` | `union Tree = Leaf \| Node of (i64 \| Tree)` の深さ `n` の木の和（ヒープのノード） | `n = 0, 3, 1000` |
| `generic n` | `def first_or :: 'a -> (Maybe<'a> \| i64) -> 'a` を `i64`・`string` で呼ぶ | 値 |
| `common n` | `(i64 \| i64u)`（`Common` のレイアウト） | 値 |
| `wide n` | 要素 32 個の共用体の最後の要素 | 値 |

native と WASM、`-O0` と `-O3`（`features.mjs` の既定）。

### 手順 12: 既存の出力の不変と生成コード

- 手順 1 の IR と、同じプログラムの今の IR が 1 byte も一致する。
- 匿名共用体を使う fixture を 2 回ビルドして IR が一致する（決定的）。
- `(i64 | string)` の IR に `%"tz.union.$anonymous.Union$2[i64,string]"` 相当の型が 1 回だけ定義され、`pick` の照合が `switch` か比較の連鎖になっている（`-O3`）ことを目で確かめて PR に書く。

### 手順 13: ドキュメント

下の「ドキュメント」。`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs`（診断の kind の表）、`node scripts/check-cli-docs.mjs`。

### 手順 14: 最終確認

GUIDE §10 と AGENTS.md の T2: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`node scripts/slot.mjs -- cargo nextest run --locked`、`node tests/features.mjs target/debug/tsuzuri anonymous_unions`、`dyn_dispatch`・`deriving` の suite、`node tests/control.mjs target/debug/tsuzuri`、`node tests/frontend_cache.mjs target/debug/tsuzuri`、`node tests/docgen.mjs target/debug/tsuzuri`。

## テスト計画

### Rust テスト

`tests/anonymous_unions.rs`（新規。`tests/dyn_dispatch.rs` の `accepts`・`rejects`・`rejects_containing` の形）に、手順 2〜10 の名前のテストを置く。各 `rejects` は code と kind（`error.kind`）の両方を確かめる。
`tests/formatter.rs` に手順 10 の 3 件。`src/syntax_codec.rs` の 2 件の既存テストを直す。`tests/diagnostic_kinds.rs` が新しい kind の登録の順序を検査する（既存のテスト）。

### E2E

手順 11 の suite `anonymous_unions`。言語リファレンスの `run=` の例は `check-docs.mjs` が native の `-O0`・`-O3` で動かす。

### 既存テストへの影響

なし（期待値を変えない）。`syntax_codec.rs` の 2 件のテストの数と `GOLDEN`、`FRONTEND_FORMAT` だけを直す。

### 性能

性能の主張はしない。匿名共用体を使わないプログラムの検査時間と IR は変わらない（手順 12）。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（新規。「匿名共用体」）: この記事のポイント、型の書き方（括弧・平坦化・順序）、値の作り方（注入の位置と規則、リテラル、注入しないもの）、`:?` での取り出しと網羅性、要素の規則（型変数・重複・借用・32 個）、名前付きの union との使い分け（F# の RFC の Guidance を Tsuzuri の言葉で）、所有権（注入は move）、他の言語との比較（F# の FS-1092、TypeScript の union、Rust の `enum`）、注意点（比較・表示がないので「現在は `match` で取り出す」と書く。要素の少ない共用体 `(i64 | string)` を `(i64 | string | bool)` へはそのまま渡せず、`match` で包み直す。どちらも計画中のチケットへのリンクは入れない）、関連項目。`run=` の例を 3 つ以上。
- `index.md`: 「組み込み型 / 組み込みモジュール」の `Union` の次に `- [匿名共用体](./built-in-types-and-modules/anonymous-union.md)`。
- `built-in-types-and-modules/union.md`: 「関連項目」と「他の言語との比較」に匿名共用体への言及。
- `pattern-matching/match.md`（と `pattern-matching.md` のパターンの一覧）: 型テストのパターンの行と例。
- `types-and-type-inference/types.md`: 型の構文の一覧に `(A | B)`。`type-inference.md`: 期待型での注入（推論は広げないこと）。
- `compiler/diagnostics.md`: `E1001`〜`E1039` の範囲の文を直し（`E1040` が先に入っていればその範囲）、`E1041` の行と kind の表（生成ブロックは `gen-docs.mjs`）。
- `docs/language.md`: 「### 共用体（union）」の後に「### 匿名共用体」を足し、「### match と分解」のパターンの表に型テストの行、「## 診断」の表に `E1041`。末尾の「実行時型テスト（`is` / `as`）は存在しません」（`docs/language.md:2926`）に「匿名共用体の `:?` は静的なタグの照合で、実行時の型情報は使いません」を足す。
- `docs/architecture.md`: 合成する宣言・正準順序・注入の位置・witness の型（D2〜D7）を union の節に。
- `README.md`: 機能一覧の型の行に匿名共用体。
- `_features/GUIDE.md` の §4.1「ファイル別の主な関数」の表に `src/anonymous_union.rs`（`declarations`・`order`・`canonicalize`・`display` | 匿名共用体の合成した宣言・正準順序・表示）の行。
- `vsc/` の文法は変えない（`:`・`?`・`|` は既存の平らな規則で色付けされる。`vsc/src/test/grammar.test.ts` も変えない）。

## 受け入れ条件

- 「例」の表のすべての行が書いたとおりになる（`tests/anonymous_unions.rs` に固定）。
- suite `anonymous_unions` が native と WASM、`-O0` と `-O3` で通り、`live == 0`。
- 匿名共用体を書かないプログラムの IR・診断・`kg`・`doc`・`api-diff` の出力が変わらない。
- 生成 IR が決定的。
- 言語リファレンスの変更したページが `check-docs.mjs` を通る。

## 落とし穴

- `TypeAliasExpansion::expand`（`src/check.rs:9450`）は `_` の腕で子を辿らない。腕を足さないと `(Meters | string)` の `Meters` が展開されず、`Meters` が未知の型になる。
- 呼び出しは引数より先に結果を期待型と単一化する（`src/check.rs:11116-11126`）。匿名共用体の期待型を `call_expression` に渡すと、注入の前に `E1003` になる。`union_value` は期待型なしで検査する（`Checker::argument` の `Call` の腕より前で分岐する）。
- パイプは `apply_piped`（`src/check.rs:11970-11998`）が、引数の検査より前に左辺の型と引数の型を単一化する。`coerce_argument` の分岐だけを足しても `x |> f` は `E1003` のまま。`apply_piped` の分岐（段ごとの変更の表）も足し、テスト `injects_call_arguments_and_pipes` に `"x" |> f`・`1 |> f`・`name |> f`（読み取りの引数）を入れる。
- `argument_hint` は `ref U` の期待型から `U` を取り出して呼び出しの結果と単一化する。`Checker::argument` の最初の分岐で `U`・`ref U` を扱えば通らない。
- 期待型を下へ渡す式に `union_value` を使うと、腕の値が 2 回包まれる（`if` の結果を包み、腕も包む）。`If`・`Match`・`Block`・`ComputationBoundary` は必ず `self.expression(expr, Some(U))` に渡す。
- 数のリテラルを期待型なしで検査すると `i32` の既定に流れる。リテラルは `preselect` で先に要素を選ぶ。
- `Type` の導出 `Ord` で要素を並べない（`id` の順になり、モジュールを足すと表示とタグが変わる）。`anonymous_union::order` を使う。
- witness は型を持たない。匿名共用体の case を `case_name` で書くと `$anonymous.Union$2$1` が診断と修正候補に出る。
- `(x: i64 | string)` は今も OR パターン。型注釈のパターンで匿名共用体を書くときは `(x: (i64 | string))`。言語リファレンスに書く。
- `Common` のレイアウト（全要素の LLVM の型が同じ）では、タグとペイロードの読み方が `General` と違う。`(i64 | i64u)` を E2E に入れる。

## 対象外

- 匿名共用体どうしの変換と部分集合の型テスト（A25）。
- `Eq`・`Ord`・`Hash`・`Display`・`Encode`・`Decode`（A26）。
- 値（リテラル）を要素にする型（F# の RFC の未解決事項 `(int | "auto")`）。
- 部分型・共変・反変、`ref T` から `ref (T | ...)` への変換、実行時の型情報。
- 型変数そのものを要素にすること（F# の RFC と同じく拒否）。
- C のヘッダーからの生成・WASM のバインディングでの匿名共用体（既存の `E1008`）。

## 着手時の再検討（2026-11-08、HEAD b3dd7f1c）

起票時（`2f556480`）以降に A27・A29・TZ02〜TZ14・G37・G38 などが入りました。着手の前に、構文・キャッシュ・整形、型検査器、周辺（名前の隠蔽・ツール・生成名・診断・文書・E2E）の 3 つの範囲を HEAD `b3dd7f1c` と照合し、計画を改めました。
この節は、この節より前の本文と食い違うところでは本文より優先します。本文の古い記述は、記録のため残します。
停止条件（「着手条件と停止条件」と GUIDE §13）は、このチケットでは次の規則に置き換えます（利用者の指示による）: 健全性を保ち、今受理されている安全なプログラムをすべて受理し続ける、最も保守的な選択を取り、その判断をこのチケットに記録して作業を続けます。テストの期待値を弱めて通すことと、安全でないプログラムを受理することはしません。

### 再現の確認

- 「再現」の表の 6 行は、`b3dd7f1c` の dev ビルドで書いたとおりに再現しました。
- 追加で確かめた結果:
  - `(| i64 | string)` は `E0002 expected an identifier`（kind `expected_identifier`。位置は `|`）です。「型が要る」位置の誤りではありません。
  - `(i64 |> string)` と `(i64 ||| string)`（`|||` は `TriplePipe` の 1 トークン）は、どちらも `E0002 expected ')' after the type` です。
  - `| : ? i64` と、関数ガードの `| :? i64 as n` は `E0002 expected a pattern` です。
  - `let (a, b) = (1, 2)` は `E0002 expected an identifier` です。`let` は名前しか取りません（`binding_rest`、`src/parser.rs:3021-3036`）。
  - 関数ガードの `| (x: Maybe<i64>) -> 1` と、`match` の `| ((x: Maybe<i64>), y) -> ...` は `E0002 expected the closing delimiter` です（既存の誤判定。下の 3）。
  - 名前付きの union を引数に取る `def describe :: V -> string` の `| S s -> s` は、edition 2027 で `E1012`（kind `move_out_of_reference_binding`）です。`own V` なら `int 42` を出します（下の 30）。

### 新しい位置（HEAD b3dd7f1c）

意味は変わらず、行番号だけが動いたものを、本文の行番号に代えて使います。

- `src/syntax.rs`: `Program` 286-323、`TypeExprKind` 760-792（`Apply` の大きさの注記 763-766）、`PatternKind` 1344-1359、`visit_expressions_mut` 1362・`visit_expressions` 1385、トークン `Question` 163、`Edition::LATEST`（E2027）33-35。
- `src/parser.rs`: `parse` 198・`parse_with_source` 202・`parse_with_source_all` 210、`Parser.edition` 246、`Program` の構築 467、`type_expr` 2371、`own_type_ahead` 2418、`type_primary` 2450（`(` の分岐 2514-2517）、`close_bracket_type` 2603・`dyn_type` 2662（`#[inline(never)]` の手本）、`binding_rest` 3021。
- `src/parse_control.rs`: `implicit_active_union` 74-167、`type_product` 479、`guard_predicate` 1341、`pattern` 1401、`pattern_suffix` 1422（型注釈 1430-1441）、`pattern_atom` 1455、`grouped_pattern` 1550、`pattern_argument_expression` 1601。
- `src/lexer.rs`: `EDITION_KEYWORDS` 10、識別子の `$` の除外 628-636、`|||` 1061、`||` 1076、`|>` 1079、`:` 1094、`?` 1100。
- `src/syntax_codec.rs`: `Program` の codec 547-631（`Mode::Interface` が本体・テスト・入口を省くのは 586-590）、`TypeExprKind` の put 1365・get 1446・拒否 1480（`18..=u8::MAX`。tag 11 は TZ12 の欠番）、`PatternKind` の put 2176・get 2246・拒否 2269（`14..=u8::MAX`）、`round_trips_every_variant` 2592（数の表 2718・2722、欠番の表 2739）、`pins_encoding_format` 2943（`GOLDEN` 2954）。`src/frontend_cache.rs`: `FRONTEND_FORMAT` 31（現在 18）、`interface_hash` 118。
- `src/formatter.rs`: `Hints` 323-336、`spacing` 653-741（`Colon` の規則 682-688、`Question` の規則 736）、`Canonical::ty` 791、`arm_owners` 883-886、`Canonical::pattern` 1084。
- `src/check.rs`（型と宣言）: `Type` 134-185（`Once` 170-172。`Task` はない）、`Type::function_with_conventions` 389-424、`Type::display` 445（record・union の腕 483-499）、`contains_reference` 783、`TypeContext::union_payloads` 1310・`union_payload` 1326、`CheckedUnion` 4714（`module()` 4730）、`Names` 5713、`Names::origin` 6168、`Names::member_function` 7218-7252。
- `src/check.rs`（収集と解決）: `validate_public_type` 7360、`check_modules_collect` 7502（モジュールの登録 7657-7661、`record_arities.push` 8012、`union_declarations` 8048-8058、型別名の検査 8133-8190、`check_union` の繰り返し 8298-8307）、`collect_unions` 9953（`union_arities.push` 9997）、`check_union` 10088、`resolve_type_with_kinds` 10166、`expand_type_aliases` 10554、`TypeAliasExpansion::expand` 10574（`_` の腕 10724）、`substitute` 10754、`reject_expanded_field_constraints` 10827、`size_error` 10878、`Layouts::union` 10955（再帰の誤り 10972-10980）。
- `src/check.rs`（式）: `Checker::expression` 11880、`finish_expression` 12011、`call_expression` 12255（A27 の `PendingHint` 12314-12333）、`binary_expression` 12442、`Hole` 12587、配列とリストのリテラル 12786-12830、`Checker::argument` 13104、`place_argument` 13176、`apply_piped` 13242、`temporary_argument` 13275、`argument_hint` 13291、`coerce_argument` 13444、`untyped_number` 14495、`Checker::integer` 14503、`dyn_of` 14772。
- `src/control.rs`: `match_value` 500（`MatchCoverage` を作る唯一の位置 512-520）、`pattern_alternatives` 629（`PatternKind` の match 650）、`case_pattern` 918-1005。`src/exhaustiveness.rs` は位置が変わっていません。`src/select.rs:214`、`src/exceptions.rs:26-31`。
- `src/polymorph.rs`: `Inference::unify` 401（`Application` の腕 432-454、関数 533-551）、`is_numeric_literal` 570、`expanded_inline_constraints` 1722-1774、`Classes::instances` 1777（組み込みの上書きの拒否 1904-1916、Drop・Format の検査 1917-1929）、`type_expression` 2988（record・union の腕 3036-3053）、`Checker::annotation` 3832。`src/higher_kinds.rs`: `Partial::display` 37、`decompose` 72-93。
- 周辺: `src/find.rs`（`hidden` 989、`build` の record 430・case 457-481、`value_pool` 997（case の別名 1041-1048・修飾した case 1046-1060・モジュール 1062-1087）、`member_pool` 1095、`function_pool` 1126、`pattern_pool` 1165-1182、`type_pool` 1205-1231）、`src/semantic.rs`（`type_reference` 246-268、`type_target` 1305-1311、`type_entry` 1313-1361）、`src/semantic_api.rs`（`public_names` 207-221）、`src/api_diff.rs`（`Renaming::ty` 445-512、`let Program` 638、既定の借用 753-775、`may_carry_functions` 1262-1318、`has_type_variable` 1374-1410）、`src/kg_capture.rs` 373・800・1158、`src/kg/ids.rs` 61（union の腕 100）、`src/kg/find.rs`（`Ty` 25-39、`parse` 287-302）、`src/llvm.rs`（`codegen_identity` 3752、`canonical_type` 3829、`mangled_type` 8022、単射のテスト 10580）、`src/llvm_debug.rs`（型名 384、`payload_union` 733-772、`tag_type` 850-880）、`scripts/lldb/tsuzuri_lldb.py`（`is_union` 489、`parts` 553-557）、`src/diagnostic_kinds.rs`（`E1040` 678-684）、`src/explain.rs`（`E1040` の行 362）。
- 文書: `docs/language.md`（union 1053、パターンの表 2766-2777、`is` / `as` の文 2877、診断の表の `E1013` 4982・`E1017` 4986・`E1040` 5006）、`docs/architecture.md`（ファイルの表 50 付近、生成名 560、デバッグ情報 667-690、union 1289-1304、`FRONTEND_FORMAT` 1533）、`compiler/diagnostics.md`（範囲の文 285、`E0002` 290・`E1013` 296・`E1017` 300・`E1003` 336）、`compiler/debugging.md` 101-102。

### 採用する計画の変更

#### 構文とキャッシュ

1. **`Program` の codec**: `uses_anonymous_unions` は本体からも立ち、合成する宣言はプログラム全体に及びます。そのため `Mode::Full` と `Mode::Interface` の両方で無条件に書きます。関数の本体だけに `(i64 | string)` を足すと `interface_hash` が変わることを、単体テストで確かめます。`FRONTEND_FORMAT` は現在の値 + 1（着手時は 18 → 19）にし、`docs/architecture.md:1533` の現在値の文も直します。
2. **tag**: `TypeExprKind::AnonymousUnion` は 18、`PatternKind::TypeTest` は 14 です。欠番の 11 は再利用しません。拒否の範囲は `19..=u8::MAX`・`15..=u8::MAX`、数の表は 19・15 です。`round_trips_every_variant` に 2 つの見本を明示して足し、`round_trips_deepest_accepted_nesting` に深い `(i64 | (i64 | ...))` を足します。
3. **`guard_predicate` と `pattern_argument_expression`**: どちらも `->`（と `when`）の前の `<` で式と判定するので、先頭だけを見ると `| (:? Maybe<i64> as m) -> ...`（関数ガード）と `((:? Maybe<i64> as m), y)`（`match`）が式として読まれます。両関数とも、走査中に**どの深さでも**隣接した `Colon`・`Question`（`colon.span.end == question.span.start`）に出会ったらパターンと判定します。受理を広げるだけの変更です。既存の `| (x: Maybe<i64>) -> 1` の誤判定は、このチケットでは直しません（型テストだけが対象）。
4. **要素の読み取りと資源上限**: 要素は `#[inline(never)]` の補助関数で読み、`type_primary` の再帰の frame を大きくしません。要素数は `MAX_NESTING` で打ち切り、`E0002`（新しい kind `too_many_anonymous_union_members`）にします（`too_many_tuple_elements` などと同じ形）。
5. **edition**: D-54 に従い、受理を広げる構文として 2026 と 2027 の両方で同じ意味で受理します。`Parser` の中で実装し、`self.edition` では分岐しません。新しい予約語はありません（`as` は既存のキーワード）。解析と整形のテストは両方の edition で書きます。
6. **`let` は型テストを書ける位置ではありません**: `let` は名前しか取らないので、仕様の「`match` 以外のパターンの位置」から `let` を外します。実行時のトラップはラムダの引数（`\(:? i64 as n) -> n`。原子が `as` まで読むので `\:? i64 as n -> n` も同じ）と `for` で確かめ、テスト名を `type_test_in_lambda_parameter_traps_at_run_time` にします。
7. **既存の誤りのまま残す形**: `(| i64 | string)` は `E0002 expected an identifier`（kind `expected_identifier`）、`|||` と `|>` は `E0002 expected ')' after the type`（kind `expected_token`）のままです。仕様の表とテストにこの文言を書きます。
8. **`TypeTest` の span** は `:` から始めます（整形の `Hints` と `arm_owners` が腕のパターンの開始位置を鍵にするため）。

#### 整形

9. HEAD の `spacing` は `Colon` の前を空白なしにするので、`| :? i64` は `|: ? i64` になり再解析できません。`?` の後の空白は G25 で意味を持つ（`?a` と `? a`）ので、トークンの種類だけの規則にはしません。`Hints` に `type_tests`（型テストのパターンの開始位置、つまり `:` の位置の集合。`Canonical::pattern` が span を消す前に埋める）を足し、次の 3 つをこの集合に結び付けます。
   - その `:` の前は、`(`・`[`・`[|` の直後を除いて空白 1 つ（汎用の `Colon` の規則より先に調べる）。
   - その `:` と `?` の間は空白なし。
   - その `?` の後は空白 1 つ。
10. **複数行の書き方**: `tsuzuri fmt` は `( i64` を `(i64` に、`string )` を `string)` に直します。言語リファレンスと `formats_anonymous_unions` には、整形の不動点を書きます:

```text
type Value =
    (i64
    | f64
    | string)
```

#### 型の正規化と正準順序

11. **`Once`**: `Type::Task` はなくなり、`Tuple` と `Handle` の間に `Once` があります。順位の列は `... < Vec < Tuple < Once < Handle < Function < Reference < Dyn < Shared` で、`Once(t)` は `t` で比べます。`Once`・`ArrayView`・`Length`・`Reference`・借用を持つ `Dyn` は要素になりませんが、順序を全順序に保つためだけに順位を置きます。
12. **関数型の比較**: `Inference::unify` は引数規約を見ず、純粋でない関数のカリー化の形（`i64 -> i64 -> i64` と `i64 -> (i64 -> i64)`）を同じ型とします（純粋でない関数型は構築時に平らになる。`Type::function_with_conventions`）。A29 の `codegen_identity` も規約を消します。そのため比較の鍵は「`pure`、純粋でなければカリー化を平らにした引数の列、最後の結果」とし、**規約は比べません**。この鍵で等しい 2 要素は `anonymous_union_duplicate_member` です。
13. **要素数の上限**: 32 個の上限（`E1017`）は平坦化の直後、要素の検査と並べ替えの前に調べます（敵対的な入力で O(n²) の並べ替えをしない）。
14. **`_` の腕を持つ走査は必須の変更**: 次はコンパイラが腕を求めないので、要素を子として辿る腕を明示して足し、テストで確かめます。`TypeAliasExpansion::expand`（`check.rs:10574`。足さないと入れ子の別名が展開されない）、`Classes::expanded_inline_constraints`（`polymorph.rs:1722`。足さないと `(Maybe<Add<'a>> | i64)` のような要素の中のインラインの制約を黙って落とす）、`semantic.rs` の `type_entry`（1313-1361）、`regions.rs` の `children`（7-26）、`contracts.rs` の `mentions_variable`（772-805）、`api_diff.rs` の `may_carry_functions`（要素を見る）と `has_type_variable`。`_` のままで正しいのは `api_diff.rs:761`、`contracts.rs:728`（`generated`。未知の型は性質テストを作らない既存の経路）、`control.rs:155`・`1015`、`diagram/transitions.rs:110`、`parse_control.rs:598`、`higher_kinds.rs:121` です。コンパイラが腕を求める箇所（`syntax_codec.rs`、`api_diff.rs:454`・`1375`、`check.rs` の `validate_public_type`・`resolve_type_with_kinds`・`substitute`・`reject_expanded_field_constraints`、`contracts.rs:778`、`docgen.rs:69`、`formatter.rs:803`・`1088`、`warnings.rs:480`・`723`、`syntax.rs:1364`・`1387`、`computation.rs:353`、`control.rs:650`、`Program` の `parser.rs:467`・`syntax_codec.rs:549`・`606`・`api_diff.rs:638`・`kg_capture.rs:373`）はすべて直します。

#### 合成と名前の隠蔽

15. **擬似モジュール**: `$anonymous` は `names.modules` にだけ `ModuleOrigin::Std` で登録し、`full_modules`・`module_namespaces` には入れません。`Std` なので、合成した宣言は `recursive.rs`（599-608。利用者の union だけを照会）にも入りません。`Names::modules` を読むモジュール一覧は `find.rs` の `value_pool` だけで、`doc`・`kg`・`kg export --facts`・LSP のワークスペースシンボル・`api-diff`・`guide` は読みません（停止条件 2 には当たりません）。
16. **名前の隠蔽は登録の時点で行います**: `collect_unions` は `MODULE` の宣言を `names.unions` と arity の表（`union_arities` など）にだけ登録し、`union_aliases`・`cases`・`case_aliases` には入れません。これで `find.rs` の `type_pool`（`Union$2` を出す）・`pattern_pool`（`Union$2$0` を出す）・`value_pool`・`function_pool`・型付き穴の索引 `build` の漏れを 1 か所で塞ぎます。二重の守りとして、`value_pool` のモジュールのループと `build` の case のループにも `hidden()` を通します。`suggestions_skip_synthetic_names` には型名の綴り違い `Union2` も足します。
17. `names.anonymous_unions` は `collect_unions` が割り当てた id から埋めます（`check_union` は成功したときだけ push するので、検査済みの列の位置は使いません）。
18. `src/anonymous_union.rs` は `check` の非公開の子モジュールなので、`llvm_debug.rs`・`kg/ids.rs` からは呼べません。`impl CheckedUnion { pub fn is_anonymous(&self) -> bool }` を足して使います。
19. **`Names::member_function`**（`check.rs:7218`）: 匿名共用体は宣言モジュールを持たない型として、record・union 以外の型と同じ `FunctionConstraintUnresolvable` の分岐へ回します（`module '$anonymous' has no function 'f'` を出さない）。
20. **高カインド**: `Inference::unify` の `Application` の腕（`polymorph.rs:432-454`）と `higher_kinds::decompose`（72-93）は、匿名共用体を型構築子として分解しません（`None`。既存の `ConstructorKindMismatch` で拒否）。分解すると `Partial` が `$anonymous.Union$2<string>` と表示され、`'f<'b>` が正準順でない、または要素が重複した union を作ります。
21. **union の名前を出すレイアウトの診断**は、大きさの誤り（`size_error`。型名を出さない）ではなく再帰の誤り（`InfiniteRecursiveLayout`、`check.rs:10972-10980`）です。匿名共用体のときだけ `Type::display` を使い、名前付きの union の文言は変えません。
22. **不要になった変更**: `prove.rs` は変えません（`prove_smt.rs:207-211` がペイロードを持つ union を拒否し、匿名共用体は必ずペイロードを持つ）。`semantic.rs` の `type_reference`・`union_ids` も変えません（合成した宣言はどの `program.unions` にもないので `links.unions[id]` が `None`）。`type_target` だけは `None` を返すように直します（`Span::default()` を返すと、`lsp.rs:637` の `target.source.unwrap_or(source_id)` で定義への移動が今のファイルの先頭へ飛ぶ）。
23. `polymorph::type_expression` は本文どおり、匿名共用体を `TypeExprKind::AnonymousUnion` に戻します（`--effective` の api-diff と derive が読む）。

#### 注入

24. **期待型を下へ渡す式**に `Select`（`select.rs:214`）、`Checked`（`exceptions.rs:26-31`）、`Hole`（`check.rs:12587`。型付き穴が `U` を報告するため。穴は包みません）を足します。
25. **形での事前選択**: 配列のリテラルは配列か固定長配列の要素がちょうど 1 つならそれ、リストのリテラルはリストの要素がちょうど 1 つならそれを選びます。選ばないと `[]` は `EmptyListNeedsAnnotation` になり、`[1, 2, 3]` を `[i64; 3]` の要素へ渡せません。0 個か 2 個以上なら規則 3 へ進みます。
26. **使わないプログラムの不変**: 注入の 4 つの入口（`Checker::expression`・`Checker::argument`・`coerce_argument`・`apply_piped`）は `self.names.anonymous_unions.is_empty()` なら何もしません。期待型は推論変数の鎖だけを辿って頭を見ます（型全体の `resolve` はしない）。`union_value`・`conform_union` などは `#[inline(never)]` の関数に置き、GUIDE §3.1 の stack の回帰テストの frame を変えません。
27. **`Checker::argument` の分岐の位置**: `U`・`ref U` の分岐は最初の腕（`ExprKind::Call` の腕より前）に置きます。`union_value` は呼び出しを期待型なしで検査するので、A27 の `PendingHint`・`defers_argument_hint` の経路に入りません。`apply_piped` の早期の戻りは、`ArrayView` の分岐と `argument_hint`・`same` より前に置きます。落とし穴の「呼び出しは引数より先に結果を期待型と単一化する」は、A27 で `ref T` の引数の未定の結果を後回しにする仕組みが入ったことを踏まえて読みます。
28. `E1003 anonymous_union_not_member` にも `.expected_found(U, T)` を付けます（`diagnostics.md` の `E1003` の契約に合わせる）。

#### 型テスト・網羅性・インスタンス

29. **D7**: 方針どおり実現できます（`MatchCoverage` を作るのは 1 か所で、型テストのペイロードは常にワイルドカード）。要素の型を決められないときは、メッセージだけに `:? _` を出し、`writable` を偽にして修正候補を書きません。
30. **D8**: 匿名共用体を head とする `instance` の拒否は、`resolve_constructor` と `bounded_type` の直後（`polymorph.rs:1830` 付近）、組み込みの上書き・マーカーのクラスの検査（`E1016`）と Drop・Format の検査より前に置きます。`instance Copy<(i64 | string)>`・`Drop`・`Format` も `E1041` になります。

#### ツール

31. **api-diff**: 既定の `tsuzuri api-diff` は構文だけで比べます（`api` → `api_with_resolution(.., None)`。`semantic_api` の型を使うのは `--effective` だけ）。`Renaming::ty` の比較用の形（正準モード）では、書いた直接の入れ子を平らにし、要素を `docgen::type_text` の順（型変数の改名より前の鍵）に並べます。表示用の形は書いた順のままです。テスト `api_diff_treats_member_order_as_equal` は既定と `--effective` の両方で確かめます。構文モードは型別名を解決しない（G19 の既存の限界）ので、`(Meters | string)` と `(f64 | string)` は構文モードでは違う型として報告されます。これを文書に書きます。
32. **デバッガー**: `scripts/lldb/tsuzuri_lldb.py` の `parts`（553-557）は型名が `(` で始まるペイロードをタプルとして展開するので、`is_union` でないときだけタプルとして扱うよう直します。`tests/debug_info.rs`（メンバー名・列挙子名が要素の表示）と `tests/debugger.mjs`（`i64(42)` の表示）に例を足し、`docs/architecture.md` の「デバッグ情報」の union の項と `compiler/debugging.md` の表（101-102）を直します。
33. **`tsuzuri find`**: `src/kg/find.rs` の型の構文解析（`Ty`・`Parser`）は `(A | B)` を読めず、匿名共用体を署名に持つ宣言は索引から落ち、問い合わせは `QueryTypeError` になります。小さく足せれば `Ty::Union`（要素は文字列の順）を足し、足さない場合は `compiler/usage.md` の find に制限として書きます。
34. **生成名**: `$anonymous.Union$n[…]` では `$` の後が小文字か数字なので、`mangled_type` のエスケープ（`$A`・`$L`・`$R`・`$C`・`$P`）と重ならず、単射は保たれます。`mangled_type` の doc comment と `docs/architecture.md:560` の説明を更新し、単射のテスト（`llvm.rs:10580`）に匿名共用体を足します。起票後に入った `$` の名前（関数の擬似モジュール `$lambda`・`$once`・`$builtin`・`$case`・`$instance`・`$export` と kg の `$once<…>`）を除く `starts_with('$')` は関数にだけ働くので、関数を持たない `$anonymous` とは衝突しません。
35. `guide/`（AI 向け成果物）は変えません（`guide/language.md` の Patterns の行には `:?` がありませんが、描画は 8,482/8,500 字で余地がない）。`kg/layout.rs` のレイアウトの行は、宣言のノードがない匿名共用体の具体化を出しません（生成した型と同じ扱い）。
36. `vsc/` の文法は変えません（`tsuzuri.tmLanguage.json:27-28`）。

#### 例と edition 2027

37. 手書きの manifest のない CLI と言語リファレンスは edition 2027 で読まれ、非 Copy の匿名共用体の引数は既定で読み取りになります。そのため本文の例の `| :? string as s -> s` は `E1012`（`move_out_of_reference_binding`）になります。例は `own` の引数で書きます。Rust テストの `tsuzuri::analyze` は E2026 で検査するので、引数規約に関わるテスト（所有権・型テストの束縛・`read_parameter_needs_owned_value`）は `analyze_with_edition(.., Edition::E2027)` で edition を明示します。
38. 改めた例（「実装者向け早見」の完了の見分け方、「目的」、「例」の代わりに使います）:

```tsuzuri
def describe :: own (i64 | string | bool) -> string = \value ->
    match value with
    | :? i64 as n -> $"int {n}"
    | :? string as s -> s
    | :? bool as b -> if b then "yes" else "no"

let first: (string | bool | i64) = 42
describe first        // "int 42"。腕を 1 つ消すと E1021 match is not exhaustive; missing: :? bool
```

```text
def font_size :: own (f64 | i64 | string) -> string = \size ->
    match size with
    | :? f64 as points -> $"{points}pt"
    | :? i64 as pixels -> $"{pixels}px"
    | :? string as text -> text
```

   `own` で通ることと、読み取りの引数のまま `| :? string as s -> s` と書くと `E1012` になること（名前付きの union と同じ）を、E2027 のテストで固定します。束縛したビューを読むだけの腕（`string` を外へ move しない腕）は `own` なしで書ける見込みで、これも同じテストで確かめます。

#### E2E と CI

39. suite `anonymous_unions` は、ほかの features の suite と並べて `.github/workflows/targets.yml`（release のバイナリ）に足します。`ci.yml` には足しません（時間の上限）。`inspect(ir)` で `%"tz.union.$anonymous.Union$2[i64,string]" = type` の定義が 1 回だけあることを固定します（正規表現では `$` をエスケープ）。`tests/anonymous_unions.rs` の `accepts` は `tests/dyn_dispatch.rs` の `assert_identifier_symbols` と同じ検査も行います。
40. 手順 1 のベースラインは、数本ではなく `tests/fixtures/` と `examples/` のすべてのプロジェクト（`Main.tz` を持つディレクトリと、`Main.tz` のない fixture のディレクトリ）について、`b3dd7f1c` のバイナリで `build --emit llvm --no-cache`（native と `--target wasm32`）・`check --json`・`doc`・`kg build`・`api-diff`（既定と `--effective`）の出力を `target/scratch/a24/before/` に保存します。手順 12 で同じ台本を新しいバイナリで回し、1 byte も変わらないことを確かめます（kg の公開の印 `.tsuzuri-kg` だけは実行ごとに変わるので比べません）。

#### 診断と文書

41. 診断の kind は 15 個です（`E0002` 2: `anonymous_union_double_bar`・`too_many_anonymous_union_members`、`E1003` 1、`E1013` 1、`E1017` 1、`E1041` 10）。登録表は `since "0.1.0"` の 1 群だけなので、`E1041` の群を `E1040` の群の後に置き、ほかは各コードの群に id の順で差し込みます。`every_kind_has_an_emission_site` があるので、kind はそれを発行する段で足します。`E1041` の説明は `src/explain.rs:362` の後に足し、`src/explain/codes/E1041.md` は `every_code_has_a_page` の形（見出し「What it covers」「Fixes」（2〜6 項目）と任意の「See also」（`E1041` の kind だけ）、1,500 字以内、コードブロックなし）にします。
42. `compiler/diagnostics.md` の範囲の文（285）は `E1001`〜`E1041` にし、`E0002`（290）・`E1013`（296）・`E1017`（300）の行と `E1003` の段落（336）に新しい kind を足します。`docs/language.md` の診断の表に `E1041` の行（`E1040` の後）を足し、`E1013`（4982）・`E1017`（4986）の行を直します。`check-cli-docs.mjs` は `E1041` が両方の表の行頭にあることを求めます。
43. `gen-docs.mjs`・`check-docs.mjs`・`check-cli-docs.mjs` は既定で `target/release/tsuzuri` を使うので、dev ビルドで確かめるときは `TSUZURI_BIN=target/debug/tsuzuri` を付けます。
44. 新しいファイルの行は GUIDE の §4.3（§4.1 は P1 時点の地図）、`docs/architecture.md` のファイルの表（50 行付近）、README の「変更の種類別」（1140 行）に足します。`docs/language.md` の「実行時型テスト（`is` / `as`）は存在しません」は 2877 行です。

### 改めたテスト一覧

本文の「実装手順」の各段のテストを、次のとおり改めます（足したもの・名前を変えたものを含む全体）。

- 手順 2（構文と codec）: `tests/anonymous_unions.rs` の `parses_anonymous_union_types`、`parses_type_test_patterns`（`match`、関数ガード、`| (:? Maybe<i64> as m) -> ...`、`((:? Maybe<i64> as m), y)`、ラムダの引数、`for`）、`rejects_double_bar`、`keeps_existing_errors_for_other_bars`（`(| i64 | string)` は `expected_identifier`、`|||`・`|>` は `expected_token`、`: ?` は `expected_pattern`）、`limits_parsed_member_count`（`too_many_anonymous_union_members`）、`parses_in_both_editions`。`src/syntax_codec.rs` の `round_trips_every_variant`（数の表と見本）・`round_trips_deepest_accepted_nesting`・`interface_hash_sees_body_only_anonymous_unions`（新規）・`pins_encoding_format`（`GOLDEN`）。
- 手順 3（合成と名前の表）: `synthesizes_declarations_only_when_used`（使わなければ `types.unions.len()` が変わらない、使えば 31 増える）、`synthetic_names_are_not_registered`（`union_aliases`・`cases`・`case_aliases` に `$anonymous` の名前がない）。
- 手順 4（正規化と表示）: `members_commute_and_flatten`、`displays_in_canonical_order`、`rejects_type_variable_members`、`rejects_indistinct_members`、`rejects_duplicate_members_through_aliases`、`function_members_ignore_conventions_and_currying`、`rejects_borrowed_members`、`limits_member_count`、`generic_members_stay_ordered_after_instantiation`、`aliases_inside_members_expand`、`inline_constraints_inside_members_are_kept`。
- 手順 5（注入）: 本文の 15 件に、`injects_through_select_checked_and_holes`（穴は `U` を報告する）、`collection_literals_pick_their_member`（`[]`、`[1, 2, 3]` を `[i64; 3]` の要素へ、リストのリテラル）を足します。`injection_moves_the_value`・`read_parameter_needs_owned_value` は E2027 で書きます。
- 手順 6（型テスト）: `type_test_binds_member_type`、`type_test_views_through_references`、`type_test_rejections`（4 つの kind）、`type_test_in_function_guards`、`type_test_in_lambda_parameter_traps_at_run_time`（`for` を含む。旧名 `type_test_in_let_traps_at_run_time`）、`read_parameter_type_test_moves_need_own`（E2027）。
- 手順 7（網羅性）: 本文の 5 件に、`unresolved_member_offers_no_fix`（`:? _` はメッセージだけ）を足します。
- 手順 8（インスタンス）: `rejects_instances_for_anonymous_unions`（`Display`・`Copy`・`Drop`・`Format` のどれも `E1041`）。
- 手順 9（名前の隠蔽）: 本文の 6 件（`suggestions_skip_synthetic_names` に `Union2`、`api_diff_treats_member_order_as_equal` は両方のモード）に、`member_function_constraint_has_no_module`、`higher_kinded_unification_refuses_anonymous_unions`、`recursive_layout_error_displays_members`、`definition_of_anonymous_union_has_no_target`、`find_reads_anonymous_unions`（足した場合）、`tests/debug_info.rs` と `tests/debugger.mjs` の例、`llvm.rs` の単射のテストへの追加を足します。
- 手順 10（整形）: `formats_anonymous_unions`（不動点の複数行の形）、`formats_type_tests`、`format_is_idempotent_for_anonymous_unions`、どれも E2026 と E2027 で。
- 手順 11（E2E）: 本文の表の関数は edition 2027 の fixture で、非 Copy の要素を取り出す引数は `own` にします。
- 手順 12: 上の 40 の台本での比較（`target/scratch/a24/baseline.sh`・`compare.sh`）。

### 実装中の判断（手順 5・6）

計画に書いていなかった点は、健全性を保つ最も保守的な選択を取り、次のとおり決めました。

1. **期待型を下へ渡す式**に `ExprKind::DynDispatch` も足しました。生成した `dyn` のインスタンスのメソッドの本体は期待型を結果の型にするので、期待型なしで検査すると `dyn_dispatch_result_type` になり、匿名共用体を返すクラスのメソッドを `dyn` で呼べなくなるためです。
2. **形での事前選択**に、同じ長さのタプルの要素がちょうど 1 つのときのタプルのリテラルと、`new` の配列・リストのリテラル（配列・リストの要素がちょうど 1 つ）を足しました。`(1, [])` を `((i64 * [string]) | bool)` へ渡せるようにするためで、選べなければ規則 3 へ進むことは配列のリテラルと同じです。レコードのリテラルと case の構築子は、名前の解決に副作用（改名の `E1040` の記録）があるので足していません。
3. **参照の値**: 規則 3 で、式の型が匿名共用体でない型への参照 `ref T` なら、参照を外した `T` で要素を数えます（要素は参照にならない）。値は共用体へ move されるので、読み取りの引数の非 Copy の値は既存の `E1012` になり（D10）、Copy の値は複製されます。
4. **未定の型**: 数のリテラルでない未定の推論変数は、期待型が `ref U`（読み取りの引数）でも `U` と単一化し、`coerce_argument` が借用します（`ref U` と単一化すると、`Parse.parse` のような総称の結果が参照になってしまうため）。パイプの左辺も同じです。
5. **パイプの左辺の数のリテラル**（`1 |> f`）は、引数の型が分かる前に検査されて数のリテラルの推論変数になります。`coerce_argument` は、それを規則 2 のリテラルの規則で選んだ要素として検査し直します（`3 |> half` は `(f64 | string)` の `f64` になり、`16 |> f` は `(i32 | i64)` で `anonymous_union_ambiguous_literal`）。元の推論変数は既定の型になり、何にも使われません。
6. **診断の文言**: `type_test_not_anonymous_union` の help（`match the cases of a named union by name`）は、対象が名前付きの union のときだけ付けます。対象と `anonymous_union_not_member` の `found` が数のリテラルなら、既定の型（`i32`・`f64`）で表示します（`match 1 with | :? i64` は `has type i32`）。`anonymous_union_ambiguous_member` の対象が数のリテラルなら、`this number could be i32 or i64 in ...` と書きます（`an undetermined type` と書かないため）。
7. **既知の制限**: 読み取りの引数（`ref U`）へ渡した `if`・`match`・ブロックは、腕へ `U` を渡します。腕が匿名共用体への参照（読み取りの引数など）だと既存の `E1003` になります（名前付きの union なら再借用されます）。値を束縛してから渡すか、引数を `own` にします。
8. `src/explain/codes/E1041.md` は 1,500 字の上限に収めるため、任意の「See also」を省き、「What it covers」に kind をまとめて書きました。
9. 手順 5 と手順 6 は、注入した値を読むのに型テストが要り、型テストの値を作るのに注入が要るので、実装を 1 つのコミットにし、型テストの専用のテストを次のコミットにしました。

### 実装中の判断（手順 7〜10 と周辺）

1. **witness の型（D7）**: `MatchCoverage::scrutinee` に対象の型（推論が終わる前の型）を持たせ、`check_coverage` で `resolve` します。witness は、参照を外したうえで union のペイロード（`union_payload`）・タプルの要素・レコードの置き換えたフィールド・リストのセル（要素とリスト）・配列の要素の型を辿ります。匿名共用体の case は `:? {要素}`（要素の表記は `anonymous_union::type_test_text`。`type_product` で読むので、純粋でない関数の要素は括弧で囲む）で、ペイロードは常にワイルドカードなので書きません。ペイロードの位置では括弧で囲みます（`Some (:? string)`）。レコードの witness（`{ value = :? string }`）は、従来どおり修正候補を書きません。
2. **`:? _` の経路**: 型テストは対象の型が検査の時点で匿名共用体に決まっていることを求めるので、今のコンパイラでは、ソースから `:? _` に至る経路はありません。守りとして残し、規則そのものを `src/anonymous_union.rs` の単体テスト `unresolved_member_offers_no_fix`（要素が分からない・推論変数・誤った型を含むときは `None`、つまりメッセージは `:? _` で修正候補なし）で固定しました。
3. **再帰のレイアウトの誤り**: `recursive.rs` が union を通るすべての循環をヒープのノードにするので、`Layouts::union` の `InfiniteRecursiveLayout` にはソースから至りません。文言を `recursive_union_layout` に切り出し、匿名共用体だけ `Type::display` で表示し（名前付きの union は従来どおり修飾名）、`src/check.rs` の単体テスト `recursive_layout_error_displays_members` で固定しました。
4. **高カインドの単一化**: `higher_kinds::decompose` は宣言を見られない自由関数で、呼び出し元は `Inference::unify` の `Application` の腕だけです。そこで、腕の条件に「`types.unions.get(id)` が匿名共用体でない」を足し、`decompose` 自体は変えていません。名前付きの union の分解（`Functor<Main.P<string>>`）は変わりません。
5. **型付き穴と候補**: `find.rs` の `build` の case のループの二重の守りは、`hidden(key)` ではなく `names.is_anonymous_union(id)` にしました（`__` を含む利用者の case 名や、`$` を含む能動パターンの暗黙の union の case の候補を変えないため）。`value_pool` のモジュールのループは計画どおり `hidden(key)` です（`names.modules` で `$` を含むのは `$anonymous` だけ）。
6. **整形**: 型テストの `:` の位置は、パターンの開始位置からは決められません（括弧で囲んだパターンは span が `(` から始まる）。`Hints::type_test_patterns` にパターンの開始位置と型の開始位置を記録し、`print_tokens` が、その間にある、`?` と隣接した `:` を `Hints::type_tests` に集めます。空白の規則は計画の 3 つに加えて、ラムダの `\` の直後も空白なしにしました（既存の `\` の規則と同じ。`\:? i64 as n -> n`）。
7. **api-diff**: 比較用の形で、書いた入れ子を平らにし、要素を「改名した型の表記の型変数を `'_` にした文字列」の順に並べ（安定な並べ替え）、その順で型変数を改名し直します。型変数の名前にも、最初に現れる位置にも順序が左右されません。構文モードは型別名を展開しないので、`(Meters | string)` と `(f64 | string)` は構文モードでは違う型、`--effective` では同じ型です（テストで固定。文書は手順 13）。
8. **`tsuzuri find`**: 小さく足せたので、`src/kg/find.rs` に `Ty::Union` を足しました。要素は平らにし、`member_key`（型変数を `_`、名前を最後の区切りで表した表記）の順に並べます。型別名は展開しません（検索のほかの部分と同じ）。テストは単体の `find_reads_anonymous_unions` と CLI の `find_searches_anonymous_unions_in_any_order` です。
9. **デバッグ情報**: `$tag` の列挙子と `$payload` のメンバーの名前を、匿名共用体では要素の `Type::display`（`i64`・`(i64 * string)`）にしました（`llvm_debug::case_names`。両者が同じ名前であることを `scripts/lldb` の `active_case` が使う）。`parts` は `is_union` でないときだけタプルとして展開します。修正前の台本では `Some(i64, {...})` と表示されることを、`tests/debugger.mjs` の新しい例が検出することを確かめました。
10. **kg の id**: `kg/ids.rs` の `type_id_text` は、匿名共用体を宣言の id ではなく `(A | B)`（関数の要素は括弧で囲む）で書きます。`kg/layout.rs` の行は、`$anonymous` が graph のモジュールにないので出ません（計画の 35 のとおり）。
11. **`warnings.rs` の `checked_type`**: 匿名共用体の具体化にも `Dependency::Type("$anonymous.Union$n")` を記録しますが、これは未使用の非公開の宣言の判定にだけ使い、`$anonymous` の宣言は利用者の宣言の一覧にないので、出力に現れません。変えていません。
12. 手順 7 と手順 8 は別のコミットにし、T1 は両方を含む状態で 1 回走らせました（3037 passed、4 skipped）。

### E2E と生成コードの確認（手順 11・12）

1. **suite `anonymous_unions`**（`tests/fixtures/anonymous_unions/Main.tz`、edition 2027、`tests/features.mjs`）: 本文の表の 8 関数を、期待値を fixture の定義から JavaScript の BigInt で書いた参照関数（`pick`・`mixed`・`owned`・`nested`・`recursive`・`generic`・`common`・`wide`）と比べます。37 件（`pick` 5・`mixed` 5・`owned` 5・`nested` 6・`recursive` 4・`generic` 5・`common` 4・`wide` 3）。手計算の値とも一致を確かめました（`pick 5 = 5`・`pick 0 = 4`・`pick -3 = 1`、`mixed 4 = 6`・`mixed 7 = 18`、`owned 0 = 3000005`・`owned 20 = 4210002`、`nested -3 = 3805`、`recursive 1000 = 1006`、`generic -3 = -592`）。`owned` は `string` と `[i64]` の要素の move・上書き・読まない値の drop で、host が `live == 0` を確かめます。`wide` は 32 要素（数の 17 型、`bool`・`unit`・`char`・`utf8char`・`string`・`utf8string`、`Maybe<i64>`、`[i64]`・`[i64; 2]`・`[|i64|]`、タプル 4 つ、`(i64 -> i64)`）で、正準順序の最後の関数型の要素（タグ 31）を閉包で作ります。`inspect(ir)` は、`%"tz.union.$anonymous.Union$2[i64,string]" = type` の定義が 1 回だけあること（`Labeled` のフィールド・`Maybe` の型引数・配列の要素で共有）、`(i64 | string | bool)` が `Union$3[i64,bool,string]`（正準順序）であること、`(i64 | i64u)` が `{ i8, i64 }`（`Common` のレイアウト）であること、`Tree` の要素がヒープのノード `tz.rec.node.$anonymous.Union$2[i64,Main.Tree]` であること、32 要素の型の最後が `fn[i64->i64]` であることを固定します。native と WASM、`-O0` と `-O3` で通ります（dev バイナリ）。`.github/workflows/targets.yml` に、release のバイナリで回す手順「Anonymous unions (A24)」を、Text class の手順の後に足しました。
2. **既存の出力の不変**: 手順 1 の台本（`target/scratch/a24/baseline.sh`）をこのブランチの dev バイナリで回し、`compare.sh` で比べました。`tests/fixtures/` と `examples/` の 185 単位すべてで、native と wasm32 の IR・`check --json`・`doc`・`kg build`・`api-diff`（既定と `--effective`）の出力と終了コードが、`b3dd7f1c` のバイナリの出力と 1 byte も違いません（kg の公開の印 `.tsuzuri-kg` を除く）。差分は新しい fixture `tests/fixtures/anonymous_unions` が増えたことだけです。その fixture の `doc`・`kg`・`api-diff`・`check --json` の出力には `$anonymous` も `Union$` も現れません。
3. **決定性**: suite が `--emit llvm` を 2 回比べるほか、`-O3` の native と wasm32 の IR もそれぞれ 2 回ビルドして一致しました。
4. **`pick` の生成コード**: 型は `%"tz.union.$anonymous.Union$3[i64,bool,string]" = type { i8, [2 x i64] }` が 1 回だけ定義されます。コンパイラの IR では、`number` の `match` はタグ（`i8`）を腕の順に `icmp eq`（0・2・1。`i64`・`string`・`bool`）で比べる連鎖です。LLVM の `-O3`（`opt -passes='default<O3>'`）は、関数が残る形（`number` に `noinline` を付けて確かめた）では、これを `switch i8 %tag, label %trap [0, 2, 1]`（既定の行き先は `llvm.trap`）にします。実際の fixture では `make` と `number` が `pick` に inline され、タグが経路ごとに定数になるので、照合は消えて `x` の符号の分岐と `phi` だけになります（`pick` に `switch` は残りません）。`common` と `wide` も `select` だけになります。

## 決定事項

### D1: 値の作り方（承認済み）

- 決定: 期待型が匿名共用体の位置で、式の型が要素の 1 つに決まるとき、自動で包む（F# と同じ）。明示の `Union.of` は作らない。
- 理由: 2026-11-02 に利用者が選んだ（GUIDE D-62 の決定 1）。F# の RFC の用途（オーバーロードの代わり、異種データ）は、明示の構築ではラベル付きの union との差が小さい。値を変えずタグを付けるだけで、確保も複製も増やさない（`dyn` の `Dyn.of` を明示にした理由「確保の位置を隠す」は当てはまらない）。
- 状態: 承認済み。

### D2: 内部表現（合成した generic な union）

- 決定: 要素数 n ごとに 1 つの generic な union 宣言（`$anonymous.Union$n<$t0, ..>`、ケース k のペイロードは `$t{k}`）を合成し、匿名共用体は `Type::Union(その id, 正準順の要素)`。合成は、どれかのモジュールが匿名共用体の構文を書いたとき（パーサーの旗）だけ、n = 2〜32 をまとめて行う。
- 理由: `Type` に variant を足すと、`Type` を match するすべての段（所有権・レイアウト・drop・clone・単相化・LLVM・事実の出力）に腕が要る。union の具体化にすれば、それらは既存の union の経路をそのまま通る。要素数を型の解決の前に知る方法がない（型別名の平坦化で数が変わる）ので、使われうる数をまとめて作る。旗で作らない場合を残し、使わないプログラムの出力を変えない。
- 状態: 既定案。

### D3: 正準順序

- 決定: 「正準順序」の表（`Type` の宣言順の順位、宣言は修飾名）。型変数に先に出会う組は「不明」として拒否する。
- 理由: 決定的で、モジュールの追加で変わらず、単相化で変わらない。F# の「型変数を独立に見て重なりうるなら拒否」より少し厳しいが、判定が局所的で実装が小さい。
- 状態: 既定案。

### D4: 注入の位置と判定

- 決定: 「値の作り方」の 1〜4。期待型を下へ渡す式は渡し、リテラル・ラムダ・`Dyn.of` は形で先に選び、それ以外は期待型なしで検査してから要素を数える。曖昧なら既定を選ばず `E1041`。
- 理由: GUIDE §1 の規則 2 の「既定値をしない」を保つ。期待型なしの検査は、既存の期待型の使い方（呼び出しの結果の先行単一化など）に触れない。
- 状態: 既定案。

### D5: `may_member` の緩さ

- 決定: 推論変数はどの型とも合いうる（数のリテラルの推論変数は数の型とだけ）。関数型の規約と `pure` は見ない。制約（型クラス）は見ない。
- 理由: 単一化を試して巻き戻す仕組みが `Inference` にない。構造だけの判定で、曖昧なら利用者に注釈を求める。
- 状態: 既定案。

### D6: 型テストの構文

- 決定: F# と同じ `:? T` と `:? T as x`（`as` は型テストの一部で、束縛の型は要素の型）。`:?` は空白なし。型は `type_product`。
- 理由: F# の RFC と同じ書き方。既存の `(p: T)` の意味（型の注釈）を変えない。`?` は型付き穴のトークンだが、パターンの先頭の `:` に続く位置には穴が来ない。
- 状態: 既定案。

### D7: 網羅性の witness に型を通す

- 決定: `MatchCoverage` に対象の型を持たせ、`witness`・`missing_arms` は witness と型を並べて辿る（union のペイロードは `union_payload`、タプルは要素、レコードは置き換えたフィールドの型、リストは要素とリスト、配列は要素）。匿名共用体の case は `:? {要素の表示}`。型が合わない（`Error`・推論変数）ときは `:? _`。既存の文言は変えない。
- 理由: 要素数ごとの宣言を全匿名共用体が共有するので、case の番号だけでは要素の型が分からない。
- 状態: 既定案。

### D8: 利用者のインスタンス

- 決定: A24 では匿名共用体を head とする `instance` を `E1041`（`anonymous_union_instance`）で拒否する。
- 理由: A26 が `Eq`・`Ord`・`Hash`・`Display` を組み込みの構造的なインスタンスとして足す。A24 で利用者のインスタンスを許すと、A26 が既存のプログラムを `E1016` で壊す。拒否を緩めるかは A26 で決める。
- 状態: 既定案。

### D9: 要素数の上限 32

- 決定: 32。超えると `E1017`。
- 理由: 合成する宣言のケースの合計が n² で増える（32 で 527）。F# の RFC の Guidance のとおり、要素が多い共用体は名前付きの union で表すべきもの。タグは 1 byte に収まる。
- 状態: 承認済み（2026-11-02 の利用者の回答「1~4 について承認します」。起票時の確認事項 2）。

### D10: 注入は値の使用（承認済み）

- 決定: 注入は `Some x` と同じ値の使用で、Copy の値は複製、非 Copy の値は共用体の中へ move する。読み取りの引数へ変数を渡せば、その変数は共用体へ move される。非 Copy の型の読み取りの引数を、そのまま匿名共用体の引数へ渡すことはできない（`own` にするか複製する）。
- 理由: 共用体の値はペイロードを所有する。借用からの暗黙の深い複製（A15）や、借用を持つ共用体の値は入れない。
- 状態: 承認済み（2026-11-02 の利用者の回答「1~4 について承認します」。起票時の確認事項 3）。

## 実装と検証（2026-11-08）

ブランチ `feat/a24-anonymous-unions`（`main` の `b3dd7f1c` から）。4 段に分けて、手順 1〜14 をすべて実装しました。未実装の手順はありません。

### 実装した手順

| 手順 | 内容 | コミット |
| --- | --- | --- |
| 着手時 | 3 つの監査と調整役の決定から「着手時の再検討」を書き、計画を改めた | `6c655467` |
| 1 | `b3dd7f1c` のバイナリで、`tests/fixtures/` と `examples/` の 185 単位の出力（native と wasm32 の IR、`check --json`、`doc`、`kg build`、`api-diff` の既定と `--effective`）を保存した。2 回の実行が一致した | （保存のみ。`target/scratch/a24/before/`） |
| 2 | `TypeExprKind::AnonymousUnion`・`PatternKind::TypeTest`・`Program::uses_anonymous_unions`、パーサー、`guard_predicate`・`pattern_argument_expression`、codec の tag 18・14、`FRONTEND_FORMAT` 19 | `a8a34295` |
| 3 | `src/anonymous_union.rs` と 31 個の合成した宣言。名前の表には登録しない | `18cd6c76` |
| 4 | 平坦化・要素の検査・正準順序・表示、`E1013`・`E1017`・`E1041` の kind、`E1041.md` | `abf7c109` |
| 5・6 | 4 つの入口での注入と、型テストのパターン | `931a36d5`・`435860a3` |
| 7 | 網羅性の witness に型を通し、`missing: :? T` と修正案 | `28b75945` |
| 8 | 利用者のインスタンスの拒否、高カインドの単一化・関数の制約・レイアウトの診断の守り | `0bc6d390` |
| 9 | 名前の隠蔽（find・型付き穴・hover・定義への移動・kg・api-diff・デバッグ情報）と `tsuzuri find` の `(A \| B)` | `5020128f` |
| 10 | 整形（`\| :? T`、`(A \| B)`、複数行の不動点） | `67814740` |
| 11 | E2E suite `anonymous_unions` と `targets.yml` の手順 | `865c22f0` |
| 12 | 既存の出力の不変・決定性・`pick` の生成コード（上の「E2E と生成コードの確認」） | `f4db8fad` |
| 13 | 言語リファレンス・`docs/language.md`・`docs/architecture.md`・README・GUIDE | `d2e77e81`・`4b9f1214` |
| 14 | T2 とこの節、チケットの移動 | この節のコミット |

### 実行したコマンドと結果

- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets -- -D warnings`（警告なし）。どの段でも通しました。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked`）: 手順 2 で 2988 passed、手順 3 で 2991、手順 4 で 3003、手順 5 で 3021、手順 6 で 3027、手順 7・8 で 3037、手順 9・10 で 3050 passed（どれも 4 skipped、失敗なし）。最後の段（手順 11〜13 は Rust のコードを変えていない）では 3050 件のうち 3049 passed、4 skipped で、`diagram_cli::kg_diagram_prints_the_diagram` が 1 件失敗しました。一時ディレクトリの名前 `tsuzuri-diagram-<pid>-0` を、同じ機械のほかの worktree の過去の実行（同じ pid）が残していたためで（`E2009 ... exists and is not a KG`）、`cargo nextest run --locked --test diagram_cli` を回し直すと 12 件すべて通りました。このブランチの変更とは関係がありません。
- GUIDE §3.1 の回帰テスト 4 件（`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・`bounds_nested_builder_expansion_not_just_source_syntax`・`honors_the_exact_specialization_limit`）は、T1 の中で通りました。
- `tests/anonymous_unions.rs` は 60 件、`tests/formatter.rs` に 3 件、`src/anonymous_union.rs` の単体テスト 3 件、`src/syntax_codec.rs`・`src/check.rs`・`src/kg/find.rs`・`src/llvm.rs` の単体テストの追加、`tests/debug_info.rs` に 1 件、`tests/debugger.mjs` に 1 例です。
- T2（dev バイナリ、`target/scratch/a24/t2.sh`）: `features.mjs anonymous_unions`（37 件、native と WASM の `-O0`／`-O3`）、`features.mjs dyn_dispatch`（58 件）、`features.mjs deriving`、`control.mjs`（701 件）、`frontend_cache.mjs`、`docgen.mjs`、`debugger.mjs`（LLDB）がすべて通りました。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変更した 13 ページに（11 ページで 416 links、43 examples、78 native runs。`tokens.md`・`keywords.md` で 116 links、9 examples、14 runs）、`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed。`E1041` は 68 codes に含まれる）、`node scripts/check-links.mjs`（既定と `--into` の `docs/language.md`・`docs/architecture.md`・`README.md`、0 broken）。
- 手順 12: `sh target/scratch/a24/compare.sh target/scratch/a24/before target/scratch/a24/after` は終了コード 0（185 単位が 1 byte も違わず、増えたのは新しい fixture だけ）。

### チケットと違う判断

- 停止条件は、調整役の決定（`target/scratch/audit/DECISIONS.md`）どおり「健全性を保つ最も保守的な選択を取り、記録して続ける」に置き換えました。各段の判断は「着手時の再検討」の「採用する計画の変更」（1〜44）、「実装中の判断（手順 5・6）」（9 項目）、「実装中の判断（手順 7〜10 と周辺）」（12 項目）、「E2E と生成コードの確認（手順 11・12）」に書きました。主なもの:
  - `let` は型テストを書けない（名前しか取らない）ので、実行時のトラップはラムダの引数と `for` で確かめました。
  - 正準順序の `Task` は `Once` に、関数型の要素は規約を比べない鍵にしました。
  - 合成した宣言は `names.unions` にも入れず、id で引く表にだけ登録しました（`semantic_api` の公開名に除外が要らない）。
  - 期待型を下へ渡す式に `Select`・`Checked`・`Hole`・`DynDispatch` を、形での事前選択に配列・リスト・タプル・`new` のリテラルを足しました。
  - `tsuzuri find` は `(A | B)` を読めるようにしました（制限として文書に書く代わりに）。
  - 既知の制限: 読み取りの引数（`ref U`）へ渡した `if`・`match`・ブロックの腕が匿名共用体への参照だと `E1003` です（言語リファレンスの「注意点」に書きました）。
- E2E の期待値は、fixture の定義から JavaScript の BigInt で書いた参照関数で、代表的な値は手計算とも照合しました。コンパイラの出力から期待値を取ったものはありません。
- `pick` の `-O3` の IR には `switch` が残りません（`make` と `number` が inline されてタグが定数になる）。`switch i8` になることは、`number` を out of line に保った場合で確かめました（手順 12 の 4）。

### 更新した文書

- 言語リファレンス: `built-in-types-and-modules/anonymous-union.md`（新規。`run=` の例 6 つ）、`index.md`、`built-in-types-and-modules/union.md`、`pattern-matching/pattern-matching.md`、`pattern-matching/match.md`、`types-and-type-inference/types.md`、`types-and-type-inference/type-inference.md`、`compiler/diagnostics.md`（範囲 `E1001`〜`E1041`、`E0002`・`E1003`・`E1013`・`E1017`・`E1021` の行、`E1041` の行。kind の表は手順 4〜8 で `gen-docs.mjs` が作り直した）、`compiler/debugging.md`、`compiler/usage.md`（find と api-diff）、`compiler/kg.md`、`values-and-functions/tokens.md`、`values-and-functions/keywords.md`。
- `src/explain/codes/E1041.md`（`tsuzuri explain E1041`）。
- `docs/language.md`（「### 匿名共用体」、パターンの表、網羅性、`is` / `as` の文、api-diff、診断の表の `E1013`・`E1017`・`E1041`）。
- `docs/architecture.md`（ファイルの表、「### 匿名共用体（A24）」で D2〜D7 と周辺、デバッグ情報の union の項、`mangled_type` の `$` の説明、`FRONTEND_FORMAT` 19）。
- `README.md`（設計と実装済みの範囲の型システムの行、匿名共用体の項、パターンマッチの項、「変更の種類別」の行）、`_features/GUIDE.md` の §4.3。
- `guide/`（AI 向けの成果物）と `vsc/` の文法は変えていません（計画の 35・36）。

### 変更したファイル

`src/anonymous_union.rs`（新規）、`src/syntax.rs`、`src/parser.rs`、`src/parse_control.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/formatter.rs`、`src/check.rs`、`src/control.rs`、`src/polymorph.rs`、`src/higher_kinds.rs`、`src/exhaustiveness.rs`、`src/diagnostic_kinds.rs`、`src/explain.rs`、`src/explain/codes/E1041.md`（新規）、`src/docgen.rs`、`src/api_diff.rs`、`src/contracts.rs`、`src/computation.rs`、`src/regions.rs`、`src/warnings.rs`、`src/semantic.rs`、`src/find.rs`、`src/kg_capture.rs`、`src/kg/ids.rs`、`src/kg/find.rs`、`src/llvm.rs`、`src/llvm_debug.rs`、`scripts/lldb/tsuzuri_lldb.py`、`tests/anonymous_unions.rs`（新規）、`tests/formatter.rs`、`tests/debug_info.rs`、`tests/debugger.mjs`、`tests/features.mjs`、`tests/fixtures/anonymous_unions/Main.tz`（新規）、`.github/workflows/targets.yml`、上の文書、`_features/A25-anonymous-union-conversions.md`・`_features/A26-anonymous-union-classes.md`（依存の A24 への移動先のリンク）、`_features/README.md`。

### レビューの指摘への対応（第1回）

2026-11-11。第 1 回のレビュー（適合性と反例の 2 件。`target/scratch/wave1/review_A24_conformance_r1.json`・`review_A24_counterexample_r1.json`）の指摘は 14 件（重大 2 件は同じ原因、軽微 12 件）で、重なりをまとめると 11 件です。すべて再現し、すべて直しました。実在しない指摘はありません。判断は調整役の決定（`target/scratch/audit/DECISIONS.md` の 2026-11-11 の追記）に従いました。

#### 指摘と対応

| 指摘 | 対応 | 回帰テスト |
| --- | --- | --- |
| 重大（2 件、同じ原因）: 関数型の要素の順序と区別が型引数の置き換えで変わる。`(i64 -> 'r)` と `(i64 -> i64 -> i64)` は引数の数で先に決まるが、`'r` が関数型になると引数が増え、同じ要素が 2 つになる（`E1021` の修正案が直らない）か、書いた型と順序が違って `E1003` になる | `order` の関数型の比較を改めました（下の判断 1）。結果が型変数・推論変数・`Partial`・`Application` の関数型は引数が増えうるので、そこまでの引数が等しく、より長い関数型との組は `Order::Unknown`（`anonymous_union_indistinct_members`）です。引数の数より先に引数そのものを比べるので、決まった順序は型引数で変わりません | `function_members_keep_their_order_under_substitution`（拒否 9 通り、受理 4 通り、`'r` を関数型にした具体化の実行、具体化と書いた型の表示の一致、関数型どうしの順序） |
| 軽微: 純粋かどうかだけが違う要素 `((i64 -> i64) \| pure (i64 -> i64))` は書けるが、どの値も注入できない | 型を書いた位置で `E1041`（`anonymous_union_indistinct_members`。`Order::Purity`）にしました。入れ子（`[pure (i64 -> i64)]` と `[(i64 -> i64)]`）も同じです（判断 2・3） | `members_that_differ_only_in_purity_are_rejected` |
| 軽微: 配列のリテラルが、固定長配列の要素も合うのに黙って動的配列の要素になる | 動的配列と、長さの合う固定長配列の両方が合えば `E1041`（`anonymous_union_ambiguous_member`、`this array literal could be [i64] or [i64; 3] in ...`）です（判断 4） | `array_literals_do_not_default_to_the_dynamic_array` |
| 軽微: `may_member` がカリー化した関数型を過小に数え、結果の決まらないラムダが黙って引数の少ない要素になる | `may_member` の関数型の腕を `unify` と同じ形（共通の引数、次に残り）にしました。結果が推論変数なら、引数の続く関数型とも合いえます | `open_function_results_may_be_curried_members` |
| 軽微（2 件）: ガードの述語やアクティブパターンの引数の中の `match`・ラムダに書いた `:?` が、腕をパターンにしてしまう | `type_test_ahead` が、それ自身の式（`match`・`with`・`\`・`for`・`fn`）の中の `:?` を数えないようにしました（判断 5）。`(:? Maybe<i64> as m)` の修正は保っています | `nested_type_tests_stay_in_their_expressions`（実行 12 通りと構文 3 通り）、既存の `parses_type_test_patterns` |
| 軽微: `tsuzuri find` が、末尾の名前が同じ要素（`Shapes.Point` と `Main.Point`）の順序を問わずには探せない | 匿名共用体の要素を、位置の組から順に、ほかの組み合わせでも単一化するようにしました（判断 6） | `kg::find::tests::find_pairs_union_members_in_any_order`、`find_pairs_members_of_the_same_name` |
| 軽微（2 件）: LSP の参照・定義・リネームが `:? T` の型を見ない | `semantic.rs` の `type_tests` が、`match`・ハンドラー・`for`・computation のパターンの型テストの型を、型注釈と同じ `type_entry` で登録します（hover と定義の対象にもなります）。匿名共用体を使わないプログラムの索引は変わりません | `type_tests_are_type_references`、`tests/lsp.rs` の `rename_and_references_reach_type_tests` |
| 軽微: 計画したテスト `synthetic_names_are_not_registered` がない | `src/check.rs` の単体テストとして足しました（`collect_unions` を直接呼び、id の表には 32 件、名前の表には利用者の union とその case だけがあることを確かめる）。これは欠けていたテストで、直す前のコードでも通ります | `check::tests::synthetic_names_are_not_registered` |
| 軽微: リファレンスが、並んだ式の期待型（`[a, 2]` と `[3, a]`）と、期待型を受けない構築子の引数（`Some [\|\|]`）を説明していない | `anonymous-union.md` の「値の作り方（注入）」と `type-inference.md`、`docs/language.md` に書きました | `check-docs.mjs`（文の中の例は `target/scratch/a24/r1/d1`〜`d5` で確かめた） |
| 軽微（2 件）: 匿名共用体の `E1005`・`E1025` が、書けないインスタンスの定義を勧める | 匿名共用体のときだけ `no instance for Eq<(i64 \| string)>; instances cannot be declared for an anonymous union; match on its members, wrap it in a record, or declare a union with named cases` にしました。ほかの型の文は 1 文字も変わりません | `missing_instances_do_not_ask_for_an_instance` |
| 軽微: タプルやペイロードの中の数のリテラルが `an undetermined type` と表示される | `literal_defaults` が、値の型の中の数のリテラルの推論変数を既定の型で示します（`found Maybe<i32>`） | `diagnostics_name_nested_number_literals` |

#### 判断

1. **関数型の正準順序を改めました（D3 と「正準順序」の節、再検討の 12 を上書き）。** 以前は「`pure`、平らにした引数の数、各引数、結果」でした。引数の数を先に比べると、`(i64 -> 'r)` と `(string -> 's)` のように引数で決まった組も、片方だけ引数が増えると順序が逆になります。そこで「純粋かどうかを除き、平らにした引数を先頭から順に、次に引数の少ないほうを先に、最後に結果」とし、決まった組で `pure` が違えば純粋でないものを先にします。`order` が `Less`・`Greater` を返す位置には型変数がなく、`Equal` は型変数のない型にだけ返すので、決まった順序と区別は型引数をどう置き換えても変わりません。関数型の要素を 2 つ以上持つ匿名共用体は、表示とタグの順が変わります（`((string -> i64) | (i64 -> i64 -> i64))` は `((i64 -> i64 -> i64) | (string -> i64))`）。匿名共用体はこのブランチで初めて入るので、既存のプログラムの出力は変わりません。
2. **純粋かどうかだけの違いは拒否します（決定どおり）。** `finish_expression` は純粋な関数の値を純粋でない関数型へ合わせ、D5 の `may_member` は純粋かどうかを見ないので、どの値も 2 つの要素に合います。純粋な関数型は、純粋でない関数型として読んで（`pure (i64 -> (i64 -> i64))` は `i64 -> i64 -> i64`）比べます。これに合わせて、既存のテスト `function_members_ignore_conventions_and_currying` の期待を 2 か所改めました: `((i64 -> i64) | pure (i64 -> i64))` の受理を新しいテストの拒否に置き換え、表示の順を確かめる型を `pure (i64 -> string)` を使うものにしました（決定が明示した変更です）。
3. **レビューにない、同じ種類の問題を 1 つ直しました。** `pure (i64 -> (i64 -> i64))` と `pure (i64 -> i64 -> i64)` は別の型（純粋な関数型は引数の数が合わなければ単一化しない）ですが、`Type::display` はどちらも `pure (i64 -> i64 -> i64)` と書きます。両方を要素にすると、網羅性の witness と修正案が片方を名指しできず、重大な指摘の (b) と同じく直らない `E1021` になります。純粋な関数型どうしも平らにして比べ、カリー化の形だけが違う組は `Order::Currying`（`anonymous_union_indistinct_members`、`... differ only in how a pure function type is curried, ...`）で拒否します。結果が型変数の `(pure (i64 -> 'r) | pure (i64 -> i64 -> i64))` も、同じ理由で「決められない組」です。`Type::display` の表示そのもの（匿名共用体のないプログラムの診断にも出る）は、既存の出力を変えないために変えていません。
4. **配列のリテラル。** 配列か固定長配列の要素がちょうど 1 つならそれ（以前と同じ。長さが違えば `E1003` の長さの誤りを出す）、なければ長さの合う要素がちょうど 1 つならそれを期待型にします（`([i64; 2] | [i64; 3])` に `[1, 2, 3]` が書けるようになりました）。それでも決まらなければ動的配列として検査し、型付きの値が配列のリテラル（`TypedExprKind::Array`）のとき、長さと要素の型の合う固定長配列の要素を数えます。それ 1 つだけなら同じ要素の固定長配列の値に作り直し（式を 2 回検査しない。`fixed_array_literal` と同じ `TypedExprKind::Tuple`）、ほかにも合う要素があれば `E1041` です。パイプの左辺のリテラル（`[1, 2, 3] |> f`）も同じ経路なので、同じ答えです。`new [..]` と、名前に束縛した配列は動的配列のままです。
5. **パーサー。** `type_test_ahead` は、括弧の中で `match`・`with`・`\`・`for`・`fn` に出会うと、それを囲む括弧が閉じるまで読み飛ばし、括弧の外なら「型テストなし」と答えて既存の判定に任せます。腕そのものの型テストは、その後ろにあっても見つけます（`(Holds (\x -> ...), :? string as s)`）。`{ x:? }`（`:?` の後に型が続かない、フィールドの型付き穴）も型テストとして数えません。`:?` を書かないプログラムでは、この走査は以前と同じく偽を返すだけです。
6. **`tsuzuri find`。** 決定は「修飾した後に完全な名前で並べ替える」でしたが、問い合わせの名前は修飾しないので（`(Point | Shapes.Point)`）、並べ替えだけでは、モジュール名の順によって一致しない組が残ります。そこで並べ替え（`member_key`）は最初の試行の順として残し、単一化が要素の組み合わせを探すようにしました（`Subst::unify_members`。1 つの匿名共用体につき 4,096 通りまで。失敗した組は束縛を残さない）。`_` や型変数の要素（`(_ | string)`）も、どの順序でも一致します。上限に達した比較は「一致しない」と答えます（`compiler/kg.md` に書きました）。
7. **LSP。** 型テストの型は hover の項目も作るので、定義への移動も働きます。パターンの型注釈 `(p: T)` の型は、以前から索引にありません（`main` も同じ）。決定が「匿名共用体を使わないプログラムの出力を変えない」としたので、このチケットでは変えていません。
8. **`may_member` と純粋さ。** 純粋でない値と純粋な要素の組は、単一化では必ず失敗しますが、`may_member` は今も合いうると数えます（過大に数えるだけで、選んだ後の単一化が `E1003` を出します）。

#### 検証

- 直す前のコードで失敗すること: `2c7ffa99`（直す前）のソースに新しいテストだけを足した写し（`target/scratch/a24/head-src`、別の `CARGO_TARGET_DIR`）で、新しい回帰テスト 11 件（`tests/anonymous_unions.rs` の 9 件、`tests/lsp.rs` の 1 件、`kg::find::tests::find_pairs_union_members_in_any_order`）を実行し、11 件すべてが失敗しました（`target/scratch/a24/prefix-tests.txt`）。`synthetic_names_are_not_registered` は欠けていたテストなので対象外です。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3062 件のうち 3062 passed、4 skipped（直す前は 3050 件。増えた 12 件は上の 11 件と `synthetic_names_are_not_registered`）。`tests/anonymous_unions.rs` は 69 件です。`tests/formatter.rs` の匿名共用体の 3 件も、この中で通りました。
- T2（dev バイナリ、`target/scratch/a24/t2-r1.sh`）: `features.mjs anonymous_unions`（37 件、native と WASM の `-O0`／`-O3`）、`features.mjs deriving`（209 件）、`control.mjs`（701 件）、`lsp_sessions.mjs`（utf-16 と utf-8）、`frontend_cache.mjs`、`docgen.mjs` がすべて通りました。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変更した 5 ページに（163 links、19 examples、30 native runs）、`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）。
- 匿名共用体を使わないプログラムの出力（パーサー・意味索引・`find`・`E1005` の文を触ったので、手順 12 の比較をやり直しました）: `b3dd7f1c` のバイナリと、この変更のバイナリで、`tests/fixtures/` と `examples/` の 186 単位の出力（native と wasm32 の IR、`check --json`、`doc`、`kg build`、`api-diff` の既定と `--effective`）を取り直し、`sh target/scratch/a24/compare.sh target/scratch/a24/before target/scratch/a24/after` で比べました。違いは、基準のバイナリが構文を読めない `tests/fixtures/anonymous_unions` の 1 単位だけ（50 行すべて）で、ほかの 185 単位は 1 byte も違いません（`target/scratch/a24/compare-r1.txt`）。
- 値の期待値は手計算です（`kind (mk add) == 42` は `(\_x -> add) 1` に `2` を渡した `2 + 40`、配列のリテラルの 10 通り、述語の 12 通り）。診断の文は、この節の表と判断に書いた文をテストが固定します。

#### 変更したファイル

`src/anonymous_union.rs`（`Order::Purity`・`Order::Currying`、`FunctionKey`・`functions`、`may_member` の関数型の腕、`takes_array_literal`・`array_literal_member`・`fixed_array_value`、`literal_defaults`）、`src/parse_control.rs`（`type_test_ahead`）、`src/kg/find.rs`（`Subst::unify_members`）、`src/semantic.rs`（`type_tests`）、`src/polymorph.rs`（`no_instance`）、`src/check.rs`（単体テスト）、`src/explain/codes/E1041.md`、`tests/anonymous_unions.rs`、`tests/lsp.rs`、言語リファレンスの `built-in-types-and-modules/anonymous-union.md`・`types-and-type-inference/type-inference.md`・`compiler/diagnostics.md`・`compiler/usage.md`・`compiler/kg.md`、`docs/language.md`、`docs/architecture.md`。

#### 残した制限

- `Type::display` は `pure (A -> (B -> C))` を `pure (A -> B -> C)` と書きます（このチケットより前から）。匿名共用体の要素としては上の判断 3 で拒否しますが、ほかの診断の表示は変えていません。
- パターンの型注釈 `(p: T)` の型は、LSP の参照・リネームの対象になっていません（このチケットより前から。判断 7）。
- `tsuzuri find` は、1 つの匿名共用体の中では要素のどの組み合わせも試しますが、ある引数の匿名共用体で選んだ組み合わせを、後ろの引数の不一致でやり直すことはしません。要素の組み合わせは 4,096 通りまでです。（「Copilot のレビューへの対応（第1回）」で解消。組み合わせは後ろの引数・結果・制約まで合うものを選び、上限は 1 回の比較につき 4,096 組）
- 期待型のない位置の並んだ式（`[3, a]`）と、構築子の引数（`Some [||]`）は、型を注釈した束縛が要ります（リファレンスに書きました）。

### レビューの指摘への対応（第2回）

2026-11-11。第 1 回の修正の後のレビュー（作業の再開後の最初の回。再現は `target/scratch/review-a24-ce/r2_*`）の指摘は軽微 7 件です。すべて再現し、すべて直しました。実在しない指摘はありません。ただし指摘 4 が「基準も受理する」とした `let h: ref string -> i64 = olen`（`olen :: own string -> i64`）は、基準（`b3dd7f1c`）でも `E1012` です（非 Copy の値を参照から move できない）。匿名共用体の要素に渡したときも同じ `E1012` になります（下の判断 4）。判断は `target/scratch/audit/DECISIONS.md` に従い、決定のない点は、健全で最も保守的な案か、型を注釈した位置の挙動に揃える案を選びました。

#### 指摘と対応

| 指摘 | 対応 | 回帰テスト |
| --- | --- | --- |
| 1. 匿名共用体を書かないプログラムの退行: 関数ガードの述語の中の、フィールドのコロンに続く名前付きの型付き穴 `R { x:?h }`（と `x:?h + 1`）が `E0002` になり、`tsuzuri fmt` も失敗する | `type_test_ahead` は、`:` の直前のトークンが名前（識別子）なら型テストとして数えません（判断 1）。基準と同じ `E1030`（`typed hole '?h' of type 'i64'`）に戻り、`fmt` は `x: ?h` に整形します | `named_holes_after_field_colons_stay_predicates`（述語 3 通り、アクティブパターンの引数、レコードのパターンの中の型テスト）、`tests/formatter.rs` の `formats_type_tests`（2 つの edition） |
| 2. 入れ子の配列のリテラルが、固定長配列の要素も合うのに黙って動的配列の要素になる（タプルの要素、構築子の引数も） | `conform_union` が `literal_fits` で「配列のリテラルを固定長配列として読めば合う要素」も数え、`may_member` より多ければ `E1041`（`anonymous_union_ambiguous_member`）にします（判断 2） | `nested_array_literals_pick_no_member_silently`（拒否 7 通り、実行 10 通り） |
| 3. `((i64 -> pure (i64 -> i64)) \| (i64 -> i64 -> i64))` は型として書けるが、どちらの型の値も `E1041` で注入できない | `may_member` は、値そのものの関数型の `pure` だけを調整して比べ、値の中の関数型（結果、タプルの要素など）は単一化と同じく `pure` も比べます（判断 3）。2 つは別の型（基準でも `let q: i64 -> i64 -> i64 = p` は `E1003`）なので、それぞれの値がそれぞれの要素になります | `function_results_keep_their_purity` |
| 4. 読み取りの引数を持つ関数（edition 2027 の `def len :: string -> i64`）を、型を注釈した位置（`let f: string -> i64 = len`）は受理するのに、`((string -> i64) \| i64)` は `E1003` にする | そのまま合う要素が 1 つもない関数の値を、`callback_adapter` と同じ条件（`read_conventions` のモジュール、引数の数が同じ、片方だけが共有参照の引数）で数え直します（`Fit::Borrowed`。判断 4） | `functions_reach_members_through_the_parameter_adapter`（edition 2027 の実行 9 通り、`E1012` 2 通り、edition 2026 の拒否 2 通り） |
| 5. ほかの位置でも違う要素 `((pure (i64 -> i64) * i64) \| ((i64 -> i64) * string))` を、「純粋かどうかだけが違う」という偽の文で拒否する | `Order::then`・`Order::all` が `Purity`・`Currying` を「ここまで等しい」として後ろの要素へ進みます。順序が循環しないよう、`pure` を順序の鍵から外しました（判断 5） | `purity_hides_no_later_difference`（実行 4 通り、拒否 1 通り、3 要素の 6 通りの並べ方が同じ型になること、関数型 4 つの順序） |
| 6. 値の中の数のリテラル: (a) 整数のリテラルを浮動小数点の要素にも数え、後の単一化が共用体に触れない逆向きの `E1003 expected an integer, found f64` を出す。(b) 浮動小数点のリテラルを整数の要素にも数え、偽の曖昧さになる。(c) 曖昧さの文が `this value of type X could be X or Y` と、決まっていない型を決まったように示す | `literal_takes` が、数のリテラルの推論変数に掛かっている制約（`Integer`・`SignedInteger`・`Float`）を相手の型で確かめます。(a) は `E1003`（`anonymous_union_not_member`。help が接尾辞か注釈を案内）、(b) は受理、(c) は型を示さない文になります（判断 6） | `number_literals_inside_values_keep_their_class`（拒否 8 通り、実行 4 通り）、`diagnostics_name_nested_number_literals`（文の期待を改めた） |
| 7. 要素と `pure` だけが違う関数型の型テスト（`pure (i64 -> i64)` の要素に `:? (i64 -> i64)`）が、`E1041 type_test_not_member` ではなく `E1003 type_mismatch` になる | 型テストの、等しい要素がないときの判定を `agrees`（`Fit::Same`。何も調整しない）にしました。要素でない型はすべて `type_test_not_member` で、help が要素を並べます | `type_tests_do_not_adapt_function_types`（4 通り） |

#### 判断

1. **パーサー（指摘 1）。** 型テスト `:? T` はパターンの先頭にだけ現れるので、その `:` の直前は `|`・`(`・`,`・`[`・`&`・`::`・フィールドの `:`／`=`・`\` などで、名前であることはありません（構築子の引数は括弧が要り、`x:? T` は型注釈としても誤りです）。式の中で `:` と `?` が隣り合うのは、フィールドの名前の後（`R { x:?h }`）だけです。そこで「`:` の直前が識別子なら型テストではない」としました。第 1 回の「`:?` の後に型が続かない」（`{ x:? }`）の判定も残しています。`:` と `?` が隣り合わないプログラムでは、この走査は以前と同じく偽を返すだけです。
2. **配列のリテラル（指摘 2。決定「`[T]` と `[T; N]` の両方が合えば E1041、黙って選ばない」）。** 型を注釈した位置では、期待される型がリテラルの中のリテラル、タプルの要素、構築子や総称の関数の引数（`let a: [i64; 2] = id [1, 2]` は基準でも通る）まで届くので、値のどこにある配列のリテラルも固定長配列になりえます。式を 2 回検査することはできない（推論の状態、持ち上げたラムダ、穴の報告が残る）ので、次のように数えます。値・リテラルの要素・タプルの要素が配列のリテラルなら、長さと要素を辿って比べます（`([[i64; 3]; 2] | [[i64]])` に `[[1, 2], [3, 4]]` は動的配列の要素に決まる）。それ以外の形の値は、部分式のどこかに配列のリテラルがあれば、値の型の動的配列を要素の固定長配列とも合いうるものとして数えます（長さは比べない過大な数え方で、曖昧さの誤りが増えるだけです）。型を注釈した束縛（`let a: [[i64; 2]; 2] = ...`）は配列のリテラルを含まない値なので、その型の要素に決まります。固定長配列として読んだときだけ合う要素が 1 つで、値がそのリテラル自身でないか、要素の型がそのままでは合わない場合（`(Maybe<[i64; 2]> | string)` に `Some [1, 2]`、`([[i64; 2]; 2] | [[string]])` に `[[1, 2], [3, 4]]`）は、以前と同じ `E1003`（`anonymous_union_not_member`）です（値を作り直さない。型を注釈した束縛が要る）。`array_literal_member` は `literal_fits` と `fixed_array_literal_member` に置き換えました。第 1 回のテスト `array_literals_do_not_default_to_the_dynamic_array` は変えずに通ります。
3. **`pure` の調整は値そのものだけ（指摘 3・7。D5 と第 1 回の判断 8 を改める）。** `finish_expression` が `pure` を調整するのは値の関数型そのもの（純粋な値を純粋でない型へ cast、閉じた関数を純粋な型へ）だけで、`unify` は値の中の関数型の `pure` を比べます。`may_member` はこれまで、どの深さでも `pure` を見ずに数えていたので、単一化が必ず失敗する要素まで数えていました。数え方を `member_fits(found, member, fit, literal)` 1 つにまとめ、`fit` で調整の範囲を表します: `Fit::Same`（`unify` と同じ。`agrees`）、`Fit::Purity`（値そのものの `pure`。`may_member`）、`Fit::Borrowed`（判断 4）。値の中の比較はいつも `Fit::Same` です。レビューが挙げた 2 つの治し方（型を拒否する、等しい要素を優先する）のうち、型を拒否する案は採りませんでした。2 つは単一化が区別する別の型で、第 1 回のテスト `function_members_ignore_conventions_and_currying` が受理を固定しており、数え方を正確にすれば値が 1 つの要素に決まるからです。値そのものの `pure` だけが違う要素（`((i64 -> i64) | pure (i64 -> i64))`）と、要素の中の関数型の `pure` だけが違う要素（`([pure (i64 -> i64)] | [(i64 -> i64)])`）の拒否は、決定と第 1 回のテストのとおり変えていません。
4. **借用する引数のアダプター（指摘 4）。** `finish_expression` は `read_conventions` のモジュール（edition 2027）で、関数型が期待される位置の関数の値を `callback_adapter` に通します。引数の数が同じで、片方だけが共有参照の引数を持てば、その違いを埋めるラムダで包みます。`may_member` で合う要素が 1 つもない関数の値にだけ、この条件を足して数え直します（2 段目）。そのまま合う要素があるときは 1 段目で決まるので、これまで受理していたプログラムの要素の選び方は変わりません（`((ref string -> i64) | (string -> i64))` に `len` を渡すと、今までどおり `ref string -> i64` の要素です。第 1 回のテストが受理を固定している型です）。選んだ後は型を注釈した位置と同じ経路なので、誤りも同じです: `own string -> i64` を `(ref string -> i64)` の要素へ渡すと、`let h: ref string -> i64 = olen` と同じ `E1012`（`move_out_of_reference_binding`）です（Copy の引数なら通る）。edition 2026 にはこのアダプターがないので、共用体があってもなくても `E1003` です。
5. **`pure` を順序の鍵から外しました（指摘 5。D3、第 1 回の判断 1 の「決まった組で `pure` が違えば純粋でないものを先にする」、決定の鍵 `(pure, 引数の列, 結果)` を改める）。** `Purity` を「ここまで等しい」として後ろの違いで決めるだけだと、`pure` を先頭の鍵にした順序は循環します。`X = pure (i64 -> i64)`、`Y = (string -> i64)`、`Z = (i64 -> i64)` のとき、`X` と `Z` は `pure` だけが違い、`Y < X`（`pure` の順）、`Z < Y`（引数の順）なので、要素 `(X * i64)`・`(Y * i64)`・`(Z * string)` は `(Y * i64) < (X * i64) < (Z * string) < (Y * i64)` になり、書いた順で型が変わります。そこで関数型は「平らにした引数、引数の数、結果」だけで並べ、`pure` は「それで等しい 2 つが区別できない」ことの判定にだけ使います。順序は、型を `pure` 抜きの鍵で読んだ辞書式の順序なので、推移的です。`pure` が違う 2 つの関数型の要素を持つ匿名共用体は、表示とタグの順が変わります（`(pure (string -> i64) | (i64 -> i64))` は `((i64 -> i64) | pure (string -> i64))`）。匿名共用体はこのブランチで初めて入るので、既存のプログラムの出力は変わりません。レビューが同じ形として挙げた `Unknown`（`(Maybe<'a> * i64)` と `(Maybe<i64> * string)` の拒否）は、仕様の「先に決められない位置があれば決められない」のままです。
6. **値の中の数のリテラル（指摘 6。D5 の「数のリテラルの推論変数は数の型とだけ」「制約は見ない」を、数のリテラルの制約に限って改める）。** 期待される型なしで検査した整数のリテラルは `Integer`（負なら `SignedInteger`）、浮動小数点のリテラルは `Float` の制約を持つ推論変数です。`literal_takes` は、その変数に掛かっている制約を相手の型で `Classes::validate` に通します（`validate` を `pub(super)` にした）。整数のリテラルは浮動小数点・`BigInt` の要素に、負のリテラルは符号なしの要素に、浮動小数点のリテラルは整数の要素に合いません。`let c: (f64 | string) = 1 + 2`、`let e: ([f64] | [string]) = [1, 2]`、`let m: (Maybe<f64> | string) = Some 1` は、型を注釈した位置（`let c: f64 = 1 + 2`）と違って拒否のままです（式を期待される型で検査し直せない）。文は `expected (f64 | string), found i32; i32 is not a member of the anonymous union` で、help の末尾に `a number literal inside the value takes no type from the union, so add a suffix to it or annotate the value` を足します。曖昧さの文は、値の型に決まっていない数のリテラルがあれば型を示さず（`this value could be (i32 * i32) or (i64 * i64) in ...`）、help は `its number literals have no type yet: add a suffix to them, or annotate the value, for example with a typed let binding` です。これに合わせて、第 1 回のテスト `diagnostics_name_nested_number_literals` の曖昧さの文の期待を、`this value of type (i32 * f64) could be ...` から `this value could be ...` に改めました（指摘 6 (c) が求めた変更です。`found Maybe<i32>` のほうの期待は変えていません）。

#### 検証

- 直す前のコードで失敗すること: ソースの変更だけを戻し（`git apply -R`。テストは新しいまま）、新しい回帰テスト 7 件と、期待を足した・改めた 2 件（`formats_type_tests`・`diagnostics_name_nested_number_literals`）を実行して、9 件すべてが失敗しました（`target/scratch/a24/prefix-tests-r2.txt`）。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked`）: 3069 件のうち 3069 passed、4 skipped（第 1 回の後は 3062 件。増えた 7 件は上の表の新しいテスト）。`tests/anonymous_unions.rs` は 76 件です（`target/scratch/a24/t1-r2.txt`）。
- T2（dev バイナリ、`target/scratch/a24/t2-r2.sh`。結果は `target/scratch/a24/t2-r2/`）: `features.mjs anonymous_unions`（37 件、native と WASM の `-O0`／`-O3`）、`features.mjs deriving`（209 件）、`control.mjs`（701 件）、`lsp_sessions.mjs`（utf-16 と utf-8）、`frontend_cache.mjs`、`docgen.mjs` がすべて通りました。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変更した 3 ページに（68 links、14 examples、26 native runs）、`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）。文の中の例（`1.0 + 2.0`、`let d: f64 = 1 + 2`、表示の順）は `target/scratch/a24/r2/d1`・`d2` で確かめました。
- 匿名共用体を使わないプログラムの出力（パーサーを触ったので、手順 12 の比較をやり直しました）: `b3dd7f1c` のバイナリと、この変更のバイナリで、`tests/fixtures/` と `examples/` の 186 単位の出力を取り直して比べました。違いは、基準のバイナリが構文を読めない `tests/fixtures/anonymous_unions` の 1 単位だけ（50 行すべて）で、ほかの 185 単位は 1 byte も違いません（`target/scratch/a24/compare-r2.txt`）。指摘 1 の再現（`target/scratch/review-a24-ce/r2_p1`・`r2_p2`）は、基準と同じ `E1030` と同じ整形結果になりました。
- 値の期待値は手計算です（`call olen == 103` は `"abc".length + 100`、`both olen == 1102` は `1000 + ("ab".length + 100)`、`f add == 203` は `200 + (1 + 2)`、`f padd == 103` は `100 + (2 + 1)`、`wide (Some (-7)) == -7`）。正準順序の期待（`([[i64; 2]] | [[i64]; 2])`、関数型 4 つの順、3 要素の順）は、種類の順位と「引数、引数の数、結果」の規則から手で並べました。診断の文は、この節の表と判断に書いた文をテストが固定します。

#### 変更したファイル

`src/anonymous_union.rs`（`Order::then`・`Order::all`、関数型の腕、`conform_union`、`literal_fits`・`fixed_array_literal_member`・`holds_array_literal`、`may_member`・`agrees`・`member_fits`・`Fit`、`literal_takes`、`holds_open_literal`。`array_literal_member` は削除）、`src/control.rs`（`type_test_pattern` の `agrees`）、`src/parse_control.rs`（`type_test_ahead`）、`src/polymorph.rs`（`Classes::validate` の可視性）、`tests/anonymous_unions.rs`、`tests/formatter.rs`、言語リファレンスの `built-in-types-and-modules/anonymous-union.md`・`types-and-type-inference/type-inference.md`・`compiler/diagnostics.md`、`docs/language.md`、`docs/architecture.md`、このチケット。診断の kind は増やしていません。

#### 残した制限

- 配列のリテラルを含む値の曖昧さは、リテラルの中のリテラルとタプルの要素のほかでは長さを比べません。`f :: [i64] -> [i64]` の `f [1, 2]` を `([i64; 2] | [i64])` へ渡すと、結果が動的配列に決まっていても `E1041` です（型を注釈した束縛で決まります）。
- 固定長配列として読んだときだけ合う要素が 1 つでも、値がそのリテラル自身でなければ（`Some [1, 2]`、入れ子のリテラル）作り直さず、`E1003` です。
- 値の中の数のリテラルは、期待される型なしで検査した型のままです（`1 + 2` は `f64` の要素にならない）。
- `(Maybe<'a> * i64)` と `(Maybe<i64> * string)` のように、決められない位置の後ろで区別できる要素は、仕様どおり拒否します。

### Copilot のレビューへの対応（第1回）

2026-11-12。PR #102 への GitHub Copilot のインラインの指摘は 2 件で、どちらも `tsuzuri find`／`kg find` の照合（`src/kg/find.rs`）です。2 件とも分岐のバイナリで再現し（`target/scratch/a24/copilot1/p1`）、直しました。実在しない指摘はありません。

#### 指摘と対応

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. `src/kg/find.rs:179`: `product()` が `*` の前で関数型を丸ごと読むので、`*` と `->` の結び付きがコンパイラと逆になる。`(bool \| i64 -> i64 * i64) -> unit` が `(i64 -> i64) * i64` を含む型になり、同じ署名の関数を見つけない | `def f :: (bool \| i64 -> i64 * i64) -> unit` に `tsuzuri find --project p --module Main '(bool \| i64 -> i64 * i64) -> unit'` は `no matches` で、括弧を足した `(bool \| i64 -> (i64 * i64)) -> unit` だけが `exact`。匿名共用体のない `(i64 -> i64 * i64) -> unit` も同じく見つけず（括弧の中の読み方は `b3dd7f1c` から同じ）、括弧のない `i64 * i64 -> i64` は読めずに `E2000` | パーサーをコンパイラの `type_expr`（`->` の間に `type_product`）と同じ形にしました: `function` が `product` を `->` でつなぎ、`product` が `atom` を `*` でつなぎます。括弧の中・匿名共用体の要素・`[..]`・`<..>` は `function` で読みます（判断 1） | `kg::find::tests::products_bind_more_tightly_than_arrows`、`tests/anonymous_unions.rs` の `find_reads_products_tighter_than_arrows` |
| 2. `src/kg/find.rs:469`: 要素の組み合わせを、それを囲むタプルや関数の単一化が終わる前に確定するので、`(Maybe<'a> \| Maybe<'b>) -> 'a -> i64` が `(Maybe<i64> \| Maybe<string>) -> string -> i64` に一致しない | 同じく `no matches`（位置の組で合う `... -> 'b -> i64` は `exact`）。第 1 回の対応の「残した制限」に書いた挙動です | 1 つの匿名共用体の中だけで後戻りしていた `Subst::unify_members` を、比較全体で後戻りする `Subst::solve` に置き換えました（判断 3〜5） | `kg::find::tests::member_pairings_stay_open_until_the_comparison_holds`・`solve_tries_other_pairings_until_the_check_holds`、`tests/anonymous_unions.rs` の `find_reconsiders_member_pairings_for_later_parameters` |

#### 判断

1. **結び付き（指摘 1）。** コンパイラの `type_expr` は `type_product` を `->` でつなぐので、`i64 -> i64 * i64` は `i64 -> (i64 * i64)`、`i64 * i64 -> i64` は `(i64 * i64) -> i64` で、匿名共用体の要素も `type_expr` です。検索のパーサーも同じ形にしました。知識グラフの署名の型（`docgen::type_text`）はタプルと関数型をいつも括弧で囲むので、エントリの読み方は変わりません。変わるのは括弧を省いた問い合わせで、匿名共用体を書かない問い合わせでは `(i64 -> i64 * i64) -> unit`（`i64 -> (i64 * i64)` を受け取る関数を探す。以前は `(i64 -> i64) * i64`）と `i64 * i64 -> i64`（以前は `E2000`）の形だけです。指摘の「検索のパーサーの全体で」に従いました。
2. **指摘にない、同じ種類の違いを 1 つ直しました。** 検索のパーサーは `fn() -> A -> B` を `(fn() -> A) -> B` と読んでいました（`b3dd7f1c` から）。コンパイラは `fn()` の後の結果を `type_expr` で読むので `fn() -> (A -> B)` で、署名をそのまま写した問い合わせ `(fn() -> i64 -> i64) -> unit` は `def z :: (fn() -> i64 -> i64) -> unit` を見つけませんでした（`target/scratch/a24/copilot1/p3`）。指摘 1 で同じ行（`fn()` の結果の読み方）を変えるので、結果を `function` で読むようにしました。括弧を書いた `(fn() -> i64) -> i64` は今までどおりです。
3. **比較全体での後戻り（指摘 2）。** `Subst::solve(goals, holds)` は、型の組の列（`tier_of` の `exact`・`borrow` では引数の組と結果の組）を順に単一化し、最後に `holds`（その宣言の制約の検査 `satisfied`）を確かめます。匿名共用体の組に出会うと、最初の要素の相手ごとに選択点を積み、後ろの組・後ろの引数・結果・制約のどれかが合わなければ、最も新しい選択点の次の相手へ戻ります。再帰ではなく目標と選択点のスタックを回すループなので、型の大きさでコールスタックは深くなりません（継続を渡す再帰にすると、深さが型の節の数に比例する）。要素の組は 1 回の `solve` につき 4,096 まで試し（以前は 1 つの匿名共用体の比較ごとに 4,096。後戻りを入れると、前の共用体の組ごとに後ろの共用体の 4,096 が掛け合わされるので、1 回の比較に 1 つの上限にしました）、上限に達した比較は以前と同じく「一致しない」です。失敗した比較は束縛を残しません。匿名共用体のない型では選択点が 1 つもできず、一階の単一化は順序によらず同じ答えを返すので、結果は以前と同じです。
4. **`partial` 段と `--takes`。** 引数の割り当ては、以前と同じ貪欲な選び方のままです（`assign`。問い合わせの引数ごとに、まだ使っていない宣言の引数のうち、それまでの比較がある組み合わせでまだ成り立つ最初のもの）。割り当てまで後戻りの対象にすると、匿名共用体のない問い合わせの `partial` の結果も変わるからです（割り当ての貪欲さは `b3dd7f1c` からの性質で、指摘の対象ではありません）。割り当てを決めた後、結果・選んだ組・制約を 1 回の `solve` で解くので、組み合わせは後ろの引数と制約まで合うものが選ばれます。
5. **制約の検査で組み合わせを選び直す経路**（`holds` が偽で次の組へ戻る）は、コンパイラが受理する宣言からは作れません。宣言の側で 2 つの要素が同じ問い合わせの要素に合うには `(Maybe<'a> | Maybe<'b>)` のような要素が要りますが、これは「決められない組」として `E1041` です。そこでこの経路は `Subst::solve` の単体テスト（`solve_tries_other_pairings_until_the_check_holds`。検査が見た `'a` が `f64`、`i64` の順であること）で確かめました。

#### 検証

- 直す前のコードで失敗すること: ソースを変える前に新しいテスト 4 件（`products_bind_more_tightly_than_arrows`・`member_pairings_stay_open_until_the_comparison_holds`・`find_reads_products_tighter_than_arrows`・`find_reconsiders_member_pairings_for_later_parameters`）を足して実行し、4 件すべてが失敗しました（`target/scratch/a24/copilot1/prefix-tests.txt`）。`solve_tries_other_pairings_until_the_check_holds` は新しい関数 `Subst::solve` のテストなので、直す前には書けません。判断 2 の `fn()` の問い合わせ（`find_reads_products_tighter_than_arrows` の最後の問い合わせ）は、直す前のソースのバイナリで `no matches`、直した後で `exact   Finder::Main.z :: (fn() -> (i64 -> i64)) -> unit` です。
- 匿名共用体を書かない問い合わせの結果: 直す前のソース（`git archive HEAD`、別の `CARGO_TARGET_DIR`）のバイナリと、直した後のバイナリで、std と `tests/fixtures/` の 4 つのプロジェクト（`kg_basic`・`kg_shop`・`conventions`・`canonical_2026`）に 27 の問い合わせと `--result`／`--takes` の 6 通り（計 141 回、テキストと `--json`）を実行して比べました（`target/scratch/a24/copilot1/cmp.sh`）。違ったのは、判断 1 の 2 つの形（`i64 * i64 -> i64` と `(i64 -> i64 * i64) -> unit`）の 10 回だけで、ほかの 131 回は 1 byte も違いません（作業ディレクトリの名前を除く。`target/scratch/a24/copilot1/cmp/diff.txt`）。`(i64 -> i64 * i64) -> unit` で増えたのは `partial Array.sort_in_place_by :: (Copy<'a>, Ord<'key>) => own ('a -> 'key) -> ref mut ['a..] -> unit  (+1)`（`'a = i64`、`'key = (i64 * i64)`）で、手で確かめた正しい一致です。std の検索の時間は、どちらも 0.06〜0.07 秒でした。
- 期待値は手で求めました: 指摘 2 の例は、位置の組（`'a = i64`）では後ろの `string` が合わず、もう一方の組（`'a = string`、`'b = i64`）で合う。`... -> 'a -> 'a` は `'a` が引数の `string` と結果の `i64` の両方になるので一致しない。2 つの共用体の例は、`'c = bool` から 2 つ目の共用体で `'b = i64`、1 つ目で `'a = string`（`'c = i64` なら `'b = bool` で、1 つ目の共用体の `{i64, string}` にない）。`peek`（`ref` の引数）は `exact` では `ref` と共用体が合わず、`borrow` で合う。`more` は引数が 1 つ多いので `partial (+1)`、`--takes` では `exact`。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし。単一化の入口 `Subst::unify` はテストだけが使うので `#[cfg(test)]` にしました）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3074 件のうち 3074 passed、4 skipped（第 2 回の後は 3069 件。増えた 5 件は上の表の新しいテスト）。`tests/anonymous_unions.rs` は 78 件です（`target/scratch/a24/copilot1/t1.txt`）。`tests/kg_query.rs`・`tests/find.rs`・`tests/mcp_knowledge.rs` の `find` のテストも、期待を変えずに通りました。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件、native と WASM の `-O0`／`-O3` がすべて通りました（`target/scratch/a24/copilot1/features.txt`。コード生成は変えていません）。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/kg.md`（8 links、0 failed）、`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`node scripts/check-links.mjs`（既定と `--into docs/architecture.md`、0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）。

#### 変更したファイル

`src/kg/find.rs`（`Parser::product`・`function`・`union`・`atom` の `(` と `fn()`、`Subst::solve`・`step`・`Goal`・`Choice`、`tier_of`・`tier_of_parts`・`assign`。`Subst::unify_members` は削除、`Subst::unify` はテスト専用）、`tests/anonymous_unions.rs`（`find_in_project` と 2 件）、言語リファレンスの `compiler/kg.md`（`*` と `->` の結び付き、組み合わせの選び直しと上限）、`docs/architecture.md`（「### 匿名共用体（A24）」の `kg/find.rs` の項）、このチケット（第 1 回の「残した制限」の find の項に、解消したことを書き足した）。

#### 対応の検証で見つかった問題（2026-11-12）

上の対応（`907e5e8c`）を検証して、問題が 2 つ見つかりました。どちらも、直す前のバイナリ（`09fdb0de`）と `907e5e8c` のバイナリで再現しました（`target/scratch/a24/copilot1-verify` の `pg`・`pb`）。2 つとも実在する問題なので直しました。

| 問題 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. [major] `partial` 段と `--takes` の割り当て（`assign`）が、「それまでの比較がある組み合わせで成り立つ」最初の引数を取る。そのため後ろの問い合わせの引数が要る引数を先に取ってしまい、直す前に見つかった一致を失う。判断 4 は「以前と同じ貪欲な選び方」と書いたが、選び方は変わっていた | `def e :: string -> i64 -> (Maybe<i64> \| Maybe<string>)` に対する `'a -> string -> (Maybe<'a> \| Maybe<'b>)` は、`09fdb0de` では `partial`、`907e5e8c` では `no matches`。`--takes "'a" --takes string --result "(Maybe<'a> \| Maybe<'b>)"` も `exact` から `no matches` に変わった（引数が 1 つ多い `e3` も同じ） | 割り当てを `Subst::solve` の選択点にしました（`Goal::Assign`）。問い合わせの引数ごとに、その時点の束縛で最初に単一化できる引数を取り、`Goal::Keep` で決めます。前の匿名共用体の組み合わせへ戻ると、割り当てもその組み合わせのもとでやり直します（判断 6）。この探索で一致しなければ、直す前の規則でもう一度照合します（判断 8） | `kg::find::tests::parameters_are_assigned_again_under_each_pairing`・`a_second_pass_keeps_the_first_pairings`、`tests/anonymous_unions.rs` の `find_assigns_parameters_under_each_pairing`・`find_keeps_the_matches_of_the_earlier_matcher` |
| 2. [minor] 上限 4,096 が照合全体で 1 つになったうえ、後戻りは最も新しい選択点から始まる。入れ子の匿名共用体はその選択点に当たるので、外側の共用体の次の組み合わせを試す前に入れ子の組み合わせをすべて試し、上限に達する。判断 3 の「上限に達した比較は以前と同じく『一致しない』」は、以前は上限に届かなかった比較が届くようになったことを書いていなかった | `def w :: (Result<(i64 \| string \| bool \| f64 \| char \| i8 \| i32), Maybe<i64>> \| Result<(…), Result<(…), Maybe<i64>>>) -> unit` に対する `(Result<(_\|_\|_\|_\|_\|_\|_), 'a> \| 'a) -> unit` は、`09fdb0de` では `exact`、`907e5e8c` では `no matches`。要素が 6 個なら `907e5e8c` でも `exact` | 答えを変えない組み合わせを試さない規則を 2 つ入れました。1 つ目: 変数がその後ろの目標にも制約にも現れない 2 つの匿名共用体は、最初に合った組み合わせで選択点を捨てます（`Goal::Cut`。判定は `Subst::survey`）。2 つ目: ほかに 1 回しか現れない変数だけが違う相手（`_` どうし、`Maybe<_>` どうし）は、最初の 1 つだけを試します（`Subst::twins`）（判断 7） | `kg::find::tests::pairings_that_nothing_else_reads_are_kept`・`alike_partners_are_tried_once`、`tests/anonymous_unions.rs` の `find_keeps_pairings_that_nothing_else_reads` |

#### 判断（続き）

6. **割り当て（問題 1）。** 直す前の `tier_of` は、結果の組み合わせを最初に合ったもので決め、その束縛のもとで、問い合わせの引数ごとに最初に単一化できる引数を取っていました。`907e5e8c` の `assign` は「ある組み合わせで成り立つ」最初の引数を取ったので、組み合わせを決めないまま引数を取りました。その結果、後ろの問い合わせの引数が要る引数をふさいでいました（例では、`'a = string` の組み合わせを理由に `'a` が `string` の引数を取った）。新しい `Goal::Assign` は、割り当てを組み合わせと同じ探索の中に置きます。問い合わせの引数は、その時点の束縛（それまでに選んだ組み合わせ）で最初に単一化できる引数を取ります。取った引数は `Goal::Keep` で決め、後ろが合わなくても別の引数は試しません。前の匿名共用体の次の組み合わせへ戻ったときは、その束縛のもとで割り当てを最初からやり直します。つまり、組み合わせごとの貪欲な割り当てです。匿名共用体のない比較では、選択点は割り当ての選択点だけで、どれも `Keep` で決まるので、直す前と同じ答えです。割り当てそのものを後戻りの対象にしないのは、判断 4 と同じく、匿名共用体のない問い合わせの結果を変えないためです。`907e5e8c` の規則が見つけた一致は、上限に達しない限り見つかります。`907e5e8c` が最後に解いた組み合わせまで来ると、そこまでの各問い合わせの引数は同じ引数を取るからです（それより前の引数は、どの組み合わせでも単一化できない）。
7. **答えを変えない組み合わせ（問題 2）。** `Goal::Union` を展開するときに、2 つの共用体の変数（束縛をたどった後のもの）を調べます。それが残りの目標（後ろの引数・結果・割り当て・外側の共用体の残りの要素）にも `watched` にも現れなければ、どの組み合わせを選んでも残りの目標の形は同じです。そこで、最初に合った組み合わせのところで選択点を捨てます（`Goal::Cut`）。`watched` は `holds` が読む型で、`tier_of` では制約の型引数です（`Entry::constrained`）。`solve` がこれを受け取るようにしたのは、`holds` が読む変数を持つ共用体の組み合わせを捨てないためです。もう 1 つの規則として、1 つの要素の相手の候補のうち、1 回しか現れない変数を 1 つの印に置き換えると同じ型になるものは、最初の候補だけを試します（`Subst::twins`）。それらの変数を入れ替えると、一方の候補の探索がもう一方の候補の探索にそのまま移るので、答えは同じです。問題 2 の例では、入れ子の `(_|…|_)` の `_` が後ろに現れないので、1 つ目の規則で入れ子の組み合わせが最初の 1 つに決まり、`'a` が合わないとすぐ外側の共用体の次の組み合わせへ戻ります（2 つ目の規則だけでも、相手の `_` を 1 つしか試さないので同じ結果になります）。どちらの規則も匿名共用体のない比較には関係しません。
8. **直す前の規則による 2 回目の照合。** 判断 6 と 7 だけでは、直す前の規則が見つけた一致を失う形が 2 つ残りました。どちらも、直す前の規則が「最初の組み合わせに決める」ことで見つけていた一致です。(a) 入れ子の共用体の変数を結果も使う場合です。たとえば `(Result<('p | … | 'v), 'a> | 'a) -> ('p | … | 'v)` を `(Result<V, Maybe<i64>> | Result<V, Result<V, Maybe<i64>>>) -> V`（`V` は 7 要素の共用体）と比べると、入れ子の組み合わせは後ろから読まれるので捨てられず、7! 通りを試すうちに上限に達して、`exact` が `partial` になります。(b) 1 回目の割り当ては「ある組み合わせで単一化できる」引数を取ります。直す前の規則（各共用体の最初の組み合わせで単一化できる引数）なら飛ばした引数を取ることがあり、それが後ろの問い合わせの引数の要る引数なら一致しません。そこで `solve` は 2 回目の探索（`commit`）を持ちます。2 回目はすべての共用体を最初に合った組み合わせで決め、目標の順序・組み合わせの順序・割り当ては直す前の `unify`・`unify_members`・`tier_of` と同じです。2 回目を行うのは、1 回目が一致せず、しかも組み合わせを終えた共用体の選択点へ戻っていた場合だけです（`Goal::Done` がその選択点に `done` を付け、そこへ戻ると `reopened`）。そうした選択点へ戻らなかった 1 回目は 2 回目と同じ手順を踏んでいるので、2 回目も一致しないからです。匿名共用体のない比較には選択点が割り当てのものしかないので、2 回目は行いません。最初は「後ろから読まれる共用体があれば 2 回目を行う」としましたが、`partial` 段の比較ではほとんど毎回 2 回目が走り、要素の多い共用体の差分ファジングで時間が `907e5e8c` の約 2 倍になったので、この条件に絞りました（答えは変わらず、時間は下の検証のとおり）。これで、直す前の規則が見つけた一致は、2 回目の探索が上限に達しない限りすべて見つかります。上限は探索 1 回につき 4,096 です。直す前（共用体の比較ごとに 4,096）とは数え方が違うので、非常に大きな比較では差が残りえます（「残した制限」）。
9. **判断 3・4 の訂正。** 判断 3 の「上限に達した比較は以前と同じく『一致しない』」は、上限に達する比較が増えたこと（問題 2）を書いていませんでした。判断 4 の「以前と同じ貪欲な選び方」は誤りで、`907e5e8c` の `assign` は選び方を変えていました（問題 1）。どちらも判断 6〜8 のとおりに直しました。上の判断 3・4 の文は、`907e5e8c` の記録として残します。

#### 検証（続き）

- 直す前に失敗すること: ソースを変える前（`907e5e8c` に新しいテストだけを足した状態）で新しいテスト 4 件を実行しました（`pairings_that_nothing_else_reads_are_kept`・`alike_partners_are_tried_once`・`find_assigns_parameters_under_each_pairing`・`find_keeps_pairings_that_nothing_else_reads`）。4 件すべてが、上の再現と同じ箇所で失敗しました（`target/scratch/a24/copilot1/verify/prefix-tests.txt`）。`parameters_are_assigned_again_under_each_pairing` と `a_second_pass_keeps_the_first_pairings` は新しい `Goal::Assign` を使うので、直す前には書けません。そこで同じ例を、`find.rs` の照合の部分を取り出して 3 版を並べたエミュレーター（`target/scratch/a24/copilot1/verify/emu`）で確かめました。判断 8 (b) の例は `09fdb0de` が `partial`、`907e5e8c` が `None`、直した後が `partial`。判断 8 (a) の例は順に `exact`・`partial`・`exact` です。`find_keeps_the_matches_of_the_earlier_matcher` の 2 つの問い合わせは、`09fdb0de` のバイナリでも、直した後のバイナリと同じく `exact`（`n`）と `partial`（`m`）です（`target/scratch/a24/copilot1-verify/pn`）。
- 3 版の差分ファジング（`emu/fuzz.rs`。検証で使われた生成器と同じく、エントリの署名を作り、変数・`_`・修飾のない名前・要素の並べ替えで問い合わせを作る。`tier_of` と `tier_of_parts` を半分ずつ）。件数は、検証と同じ種で 200,000 件、別の種で 300,000 件、要素 2〜6 個の共用体で 20,000 件、匿名共用体のない型で 200,000 件です。直した後が `09fdb0de` または `907e5e8c` より悪い答え（一致を失う、段が下がる）を返した件数は、どの組でも 0 でした。`09fdb0de` と比べると、一致が増えたのが 401・631・60 件、段が上がった（`partial` から `exact`）のが 61・83・4 件です。`907e5e8c` と比べると、増えたのが 51・91・38 件、上がったのが 1・3・1 件です。匿名共用体のない 200,000 件では 3 版とも同じ答えでした（`emu/final-*.txt`）。合計の時間は負荷の高い共用の機械での目安で、直した後・`907e5e8c`・`09fdb0de` の順に、200,000 件で 73 秒・121 秒・31 秒、要素の多い 20,000 件で 147 秒・111 秒・31 秒（1 件の最長は 197 ms・590 ms・166 ms）、匿名共用体のない 200,000 件で 4.8 秒・4.9 秒・3.7 秒です。`09fdb0de` が速いのは再帰の単一化で、目標のスタックを複製しないためです（`907e5e8c` から同じ）。
- 期待値は手で求めました。問題 1 の例: 結果の位置の組では `'a = i64`・`'b = string` で、`'a` は `string` の引数と単一化できず `i64` を取り、問い合わせの `string` は `string` を取る（引数が 2 つで `partial`、3 つで `partial (+1)`、`--takes` では `exact`）。`'a -> string -> unit` は、`'a` が `string` を取ると問い合わせの `string` に残る引数がない（`b3dd7f1c` からの結果）。問題 2 の例: 外側の位置の組では `'a = Maybe<i64>` になり、2 つ目の要素 `Result<…, Result<…>>` と合わない。もう一方の組では `'a = Result<(…), Maybe<i64>>` で、2 つ目の要素の入れ子の 7 要素は `_` と任意の対応で合う。`alike_partners_are_tried_once`: `'a` は `* last` の `last` になり、`last` が 8 つの要素のどれかなら一致し、`unit` なら一致しない。判断 8 (b) の例: `((Maybe<'x> | Maybe<'y>) * 'x)` は、位置の組（`'x = i64`）では 1 つ目の引数の `* string` と合わず、2 つ目の引数（`'x = bool`）と合う。2 つ目の問い合わせの引数は 1 つ目の引数と同じ型。
- 匿名共用体を書かない問い合わせ: 第 1 回と同じ 141 回（`target/scratch/a24/copilot1/cmp.sh`）を直した後のバイナリで実行し、`907e5e8c` の出力（`target/scratch/a24/copilot1/cmp/after`）と比べました。141 回のすべてで、出力は 1 byte も違いません（作業ディレクトリの名前を除く。`target/scratch/a24/copilot1/verify/cmp-result.txt`）。
- 検証で使われた匿名共用体の問い合わせ（`target/scratch/a24/copilot1-verify/ucmp.sh`。3 プロジェクト × 20 通り）。60 回すべてで、`09fdb0de` と `907e5e8c` が出した一致はすべて出ます。`09fdb0de` の `partial Finder::Main.g3` は、同じ名前の最もよい段の `exact` として出ます（`907e5e8c` と同じ）。`907e5e8c` より増えたのは、2 つの問題の例の 5 行です（`target/scratch/a24/copilot1/verify/fix-union.result.txt`）。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3081 件のうち 3081 passed、4 skipped です（`target/scratch/a24/copilot1/verify/t1.txt`）。上の対応の後は 3074 件で、増えた 7 件は表の新しいテストです。`tests/anonymous_unions.rs` は 81 件です。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`target/scratch/a24/copilot1/verify/features.txt`。コード生成は変えていません）。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/kg.md`（8 links、失敗なし）、`node scripts/check-links.mjs`（既定と `--into docs/architecture.md`、0 broken）、`gen-docs.mjs --check`（19 blocks、0 stale）、`check-cli-docs.mjs`（4 checks、0 failed）。

#### 変更したファイル（続き）

`src/kg/find.rs`（`Subst::solve`・`search`・`survey`・`twins`・`shallow`・`each_variable`・`shape`、`Goal::Union`・`Cut`・`Done`・`Assign`・`Keep`、`Goal::each_type`、`Choice::done`、`Pick`、`Entry::constrained`、`tier_of`・`tier_of_parts`。`assign` は削除）、`tests/anonymous_unions.rs`（3 件）、言語リファレンスの `compiler/kg.md`（`partial` 段と `--takes` の割り当て、答えを変えない組み合わせ、2 回目の照合、上限の数え方）、`docs/architecture.md`（「### 匿名共用体（A24）」の `kg/find.rs` の項）、このチケット。

#### 残した制限

- `partial` 段と `--takes` の引数の割り当ては貪欲です（判断 4・6）。問い合わせの引数は、その時点の束縛で最初に合う宣言の引数を取ります。後ろが合わなくても、別の引数への割り当てはやり直しません（匿名共用体と関係なく `b3dd7f1c` から）。割り当てをやり直すのは、匿名共用体の組み合わせを選び直したときと、2 回目の探索（判断 8）だけです。
- 要素の組は探索 1 回につき 4,096 までです（判断 7・8）。後ろから読まれる変数を持ち、区別できない要素の多い匿名共用体が入れ子になった比較では、1 回目の探索が上限に達することがあります。そのとき 1 回目が組み合わせを終えた共用体へ戻っていれば、2 回目の探索が直す前の規則で照合します（戻っていなければ、2 回目も同じ手順で上限に達します）。ただし 2 回目の上限も探索全体で 1 つです（直す前は共用体の比較ごとに 4,096）。そのため非常に大きな比較では、直す前に見つかった一致が見つからないことがありえます（差分ファジングでは起きていません）。

### Copilot のレビューへの対応（第2回）

2026-11-12。PR #102 への GitHub Copilot の 2 回目のインラインの指摘は 3 件です（`src/api_diff.rs`・`src/check.rs`・`src/parse_control.rs`）。3 件とも分岐のバイナリ（`b9a31cce`）で再現し（`target/scratch/a24/copilot2/p1`・`p2`・`p3`）、直しました。実在しない指摘はありません。

#### 指摘と対応

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. `src/api_diff.rs:563`: `Renaming::members` は、並べ替えの鍵を作るために各要素を `self.ty` で改名し、並べ替えた後にもう一度改名する。入れ物を通した入れ子（`([([i64] \| bool)] \| bool)`）は平らにならないので、段ごとに仕事が倍になり、`api-diff` が小さな署名で指数的に遅くなる | 配列を通して `d` 段入れ子にした `def f :: own ([…([i64] \| bool)…] \| bool) -> i64` と、要素の順を逆にした版の `api-diff` は、`d` = 16・18・20・22・24 で 0.11・0.39・1.53・5.97・24.05 秒（`--effective` の `d` = 24 は 29.4 秒）。構文の入れ子の上限（128）の中で書ける `d` = 63 は終わらない | 鍵を `blind` の改名で作るようにしました。`blind` の改名は型変数を `'_`、まだ名前のない region と長さパラメーターを `_` にし、名前の表を変えません。そのため入れ子の共用体の要素は 1 回ずつ読んで並べ、各要素は並べ替えた後に 1 回だけ改名します（判断 1）。`d` = 24 は 0.01 秒、`d` = 63 は 0.03 秒です | `api_diff::tests::nested_union_members_are_renamed_once`（`ty` の呼び出しの数を、手で数えた式と比べる）、`nested_members_take_one_order`、`members_order_ignores_where_regions_first_appear` |
| 2. `src/check.rs:12035`: 注入の分岐は `expression` の先頭の `old_value` の早い戻りより後ろにあるので、`@old` は注入を通らない。`@ensures` の中の `let v: (i64 \| bool) = @old (x)`（`x: i64`）は、`x` なら注入できるのに `E1003` になる | `def grow :: i64 -> i64 = \x ->` の `@ensures \r -> { let v: (i64 \| bool) = @old (x); count v + 1 == r }` は `E1003 expected (i64 \| bool), found i64`。`@old (x)` を `x` にすると受理し、`tsuzuri test` も通る。`if` の腕（`if r > 100 then @old (x) else true`）と配列のリテラルの要素（`total [@old (x), true, 1]`）も同じ `E1003`。引数（`count (@old (x))`）は `Checker::argument` の注入を通るので、以前から受理していた | `old_value` は、匿名共用体が期待されていれば値を `injected_expression` へ渡します（判断 2） | `old_values_are_injected`（型を注釈した `let`・`if` の腕・配列のリテラルの要素・引数。native と WASM の `-O0`／`-O3` で実行し、契約の行を実行時に確かめる。偽になる契約の行がトラップすること） |
| 3. `src/parse_control.rs:1428`: `type_test_ahead` が、腕の `->` だけでなく、入れ子の型注釈の中の `->` でも止まる。関数ガードの腕 `\| ((g: Maybe<i64> -> bool), :? i64 as n) -> n` は `:?` の前で走査が止まり、`guard_predicate` が `<` を比較と読んで、タプルのパターンを述語と取り違え、構文の誤りになる | `def pick :: (Maybe<i64> -> bool) -> own (i64 \| bool) -> i64 = \g v ->` の腕 `\| ((g: Maybe<i64> -> bool), :? i64 as n) -> ...` は `E0002 expected the closing delimiter` | 走査を止める `->`・`when` を、括弧の外（深さ 0）のものだけにしました（判断 3） | `arrows_in_annotations_do_not_hide_type_tests`（関数ガードの腕、`when` のある腕、`match` の腕を実行し、括弧の中に `->` のある述語が述語のままであること） |

#### 判断

1. **api-diff の並べ替えの鍵（指摘 1）。** 鍵の定義は以前と同じく「要素を改名した文字列の型変数を伏せたもの」で、それを名前の表を変えない改名（`Renaming::blind`）で作ります。以前は、鍵を作る改名が名前の表（型変数・region・長さパラメーター）を進め、鍵を作り終えてから元に戻していました。型変数は鍵の中で伏せていたので、鍵が名前の表に依存したのは、region と長さパラメーターが要素の中で初めて現れる場合だけです。`blind` の改名は、既に名前のあるものはその名前のまま、まだ名前のないものを `_` にします。record・union・型別名が宣言した長さパラメーター（`const N: i64`）は、`parameter` が型より先に名前を付けるので、鍵は以前と同じです。関数の署名の長さ（`[i64; N]`）は、既定の比較では書いた名前のまま（`length_argument`）で、`--effective` では `Variable("#N")` として最初に現れた順に名前が付きます。これが要素の中で初めて現れると、鍵のその位置は以前の `n0` などではなく `_` です。ただし鍵の順序がその位置で決まるのは、そこまでの文字列が等しい 2 つの要素（要素の型が等しい固定長配列の長さ、同じ型の長さの引数）だけで、それは検査器の正準順序でも決められない組（`E1041`）です。要素の中の region は検査器が拒否します（`((ref {a} string -> i64) | …)` は `E1013 named regions inside higher-order function types are not supported`、`(({r} ref {r} string -> i64) | …)` は `E1013 region-quantified function types are only supported as whole parameter types of a named function`）。そのため、検査器が受理するプログラムでは、並べ替えた順も、比較に使う文字列も以前と同じです（下の検証の差分の確認と、長さパラメーターを要素に持つ署名 `target/scratch/a24/copilot2/p1len` の両方の比較で、2 つのバイナリの出力は同じ）。違うのは、構文だけを読む既定の比較に検査器が拒否する署名を渡したときで、以前は先に書いた要素の region が `r0` になって鍵が書いた順に依存し、要素を並べ替えただけの 2 つの版が `breaking: changed def` でした。今は 2 つの版が同じ形（`((ref {r0} i64 -> i64) | (ref {r1} string -> i64))`）になり、変更になりません（`members_order_ignores_where_regions_first_appear`。要素の順を変えても変更にならない、という `members` の約束に沿う変更です）。仕事の量は、入れ子の深さ `d` の鎖で `ty` の呼び出しが `1 + 6d + 3d(d - 1) / 2` 回です（以前は `6 * 2^d - 5` 回。数え方はテストのコメント）。鍵の文字列は段ごとに作り直すので、文字数でも深さの 3 乗までで、深さは構文の上限で 63 段までです。鍵を記憶する表は作りませんでした（多項式で足り、表の鍵になる位置の同一性を持ち込まないため）。（検証で、まだ名前のないものをすべて `_` にすると、同じ region の名前を別の位置で使う要素の鍵が等しくなり、要素を並べ替えただけで変更になる署名があると分かりました。その後、名前のないものは要素の中で最初に現れた順の番号にしています。下の「検証者の指摘への対応」の判断 4）
2. **`@old`（指摘 2）。** `old_value` は `expression` の最初で `@old (e)` の束縛の値を返し、期待される型とは `same` で単一化していたので、`expression` の注入の分岐（`injects`）を通りませんでした。匿名共用体が期待されていれば、`injected_expression` へ渡します。`injected_expression` は `union_value` → `expression(e, None)` → `old_value`（期待される型なし）で束縛の値を得て、`conform_union` が要素として包むので、`x` を書いたときと同じ経路です。`call_expression` の入口の `old_value`（`expression` を通らない引数）も同じ関数なので、同じ答えです。匿名共用体を使わないプログラムでは `names.anonymous_unions` が空なので、以前と同じです。言語リファレンスの「値の作り方（注入）」は、期待される型が匿名共用体の位置のすべての式について書いており、`@old` を除いていません。直したのは実装がその記述と違っていた点なので、リファレンスは変えていません。
3. **パーサー（指摘 3）。** 関数ガードの腕の `->` と `when` は、腕の先頭から数えて括弧の外にあります。括弧の中の `->` は、パターンの型注釈の関数型（括弧の中のパターンは `pattern_type_arrow` で `->` を読む）か、読み飛ばす式（`\`・`match`・`for`・`fn`・`with`）の中のものです。そこで `type_test_ahead` は、括弧の外の `->`・`when` でだけ止まります。`type_test_ahead` が真を返すのは、隣接した `:`・`?` に出会ったときだけです。そのため、`:?` を書かないプログラムの判定は変わりません（`tests/fixtures/`・`examples/`・`std/` で `:?` を書くのは `tests/fixtures/anonymous_unions` だけ）。型テストのない腕 `| ((g: Maybe<i64> -> bool), n) -> ...` は、`b9a31cce` でも `E0002` です。`guard_predicate` の走査（A24 より前からのもの。括弧の深さによらず `->` で止まり、その前の `<` を比較と読む）が述語と判定するからです。`match` の腕でも、括弧がアクティブパターンの引数かどうかを決める `pattern_argument_expression` の走査が同じ `<` を比較と読み、同じ `E0002` です。匿名共用体を使わないプログラムの挙動を変えないため、このチケットでは変えていません（残した制限）。

#### 検証

- 直す前のコードで失敗すること: 新しいテストを足した後、ソースの変更だけを戻して（`src/check.rs`・`src/parse_control.rs` は `b9a31cce` のまま、`src/api_diff.rs` は `b9a31cce` に呼び出しを数える欄 `visits` と単体テストだけを足したもの）実行しました。`nested_union_members_are_renamed_once` は深さ 2 で `left: 19, right: 16`、`members_order_ignores_where_regions_first_appear` は `((ref {r0} string -> i64) | (ref {r1} i64 -> i64))`、`old_values_are_injected` は `E1003: expected (i64 | bool), found i64`、`arrows_in_annotations_do_not_hide_type_tests` は `E0002: expected the closing delimiter` で、4 件すべてが失敗しました（`target/scratch/a24/copilot2/prefix-tests.txt`・`prefix-tests-2.txt`）。`nested_members_take_one_order` は、並べ替えた順が以前と同じであることを確かめるテストなので、直す前も通ります。
- api-diff の差分の確認（`target/scratch/a24/copilot2/fuzz1/fuzz.mjs`）: 匿名共用体を入れ子にした無作為の署名（配列・リスト・`Maybe`・タプル・関数型の中の共用体、要素の並べ替え、型変数の付け替え、1 か所の型の変更）を 1 ファイル 40 個ずつ作り、`b9a31cce` のバイナリと直した後のバイナリの `api-diff` の出力（終了コード、標準出力、標準エラー）を比べました。既定の比較で 200 回（8,000 組。型変数あり）、`--effective` で 100 回（4,000 組。型変数なし）、どれも 1 byte も違いません。指摘の例の時間は、直した後に `d` = 24 で 0.01 秒、`d` = 63 で 0.03 秒です（負荷のある共用の機械では 0.07〜0.09 秒。`--effective` の `d` = 63 は 2.3 秒で、その大半は 2 つの版の型検査です。`check` だけで 0.7 秒）。
- 期待値は手で求めました。`ty` の呼び出しの数は判断 1 とテストのコメントの式（深さ 0・1・2・3・10・20・60 で 1・7・16・28・196・691・5,671）、正準形の文字列は ASCII の順（`'` < `M` < `[` < `b` < `s`）から、`old_values_are_injected` の値は `grow 41 == 42`（`count 41 + 1`）、`grow 200 == 201`（`w` は入口の 200 で `201 - 1`）、`total [41, true, false] == 41 + 1000 + 2000 == 42 + 2999`、偽になる行は `count 41 == 42`、`arrows_in_annotations_do_not_hide_type_tests` の値は `pick five 5 == 5`（`five (Some 5)` が真）、`pick five 7 == -7`、`pick five true == 100`、`pick five false == 200`、`guarded five 7 == 0`（ガードが偽で `_` の腕）、述語の `f 2 == 1`（`1 < 2`）と `f 0 == 0` です。
- 既存の出力の不変（手順 12 の比較）: `b9a31cce` のバイナリと直した後のバイナリで、`tests/fixtures/` と `examples/` の 186 単位の出力（native と wasm32 の IR、`check --json`、`doc`、`kg build`、`api-diff` の既定と `--effective`）を取り直し、`sh target/scratch/a24/compare.sh` で比べました。186 単位（9,577 ファイル）のすべてが 1 byte も違いません（`target/scratch/a24/copilot2/compare.txt`。匿名共用体を使う `tests/fixtures/anonymous_unions` も同じ）。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3085 件のうち 3085 passed、4 skipped です（`target/scratch/a24/copilot2/t1.txt`。第 1 回の対応の後は 3081 件で、増えたのは `api_diff::tests` の 2 件と `tests/anonymous_unions.rs` の 2 件。`tests/anonymous_unions.rs` は 83 件）。T1 の後に足した `api_diff::tests::members_order_ignores_where_regions_first_appear` は、`cargo nextest run --locked --lib api_diff::tests` で 3 件とも通りました。`tests/api_diff.rs` の既存のテストも、期待を変えずに通りました。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`target/scratch/a24/copilot2/features.txt`）。
- 文書: 言語リファレンスのページは変えていません（判断 2・3）。`docs/architecture.md` は `node scripts/check-links.mjs --into docs/architecture.md`（17 links、0 broken）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）。

#### 変更したファイル

`src/api_diff.rs`（`Renaming::blind`、`variable`・`region`・`length` の `blind` の腕、`members`、単体テストが数える `visits`（`#[cfg(test)]`）と単体テスト 3 件。`blind_variables` は削除）、`src/check.rs`（`old_value`）、`src/parse_control.rs`（`type_test_ahead`）、`tests/anonymous_unions.rs`（2 件）、`docs/architecture.md`（「### 匿名共用体（A24）」の `type_test_ahead` と `api_diff.rs` の項）、このチケット。言語リファレンスと `docs/language.md` は変えていません（判断 2・3。直したのは、リファレンスが書いているとおりに動かなかった点です）。

#### 残した制限

- 型テストを持たず、括弧の中の注釈に `<` を持つタプルのパターン（`((g: Maybe<i64> -> bool), n)`）は、関数ガードの腕では `guard_predicate`、`match` の腕では `pattern_argument_expression`（括弧がアクティブパターンの引数の式かどうかの判定）の走査が `<` を比較と読み、`E0002` です（判断 3。どちらも A24 より前からの走査で、`b9a31cce` でも同じ。`target/scratch/a24/copilot2/p3/plain`・`plainmatch`）。`:?` を書いた腕と括弧は、`type_test_ahead` が先にパターンと判定します。
- 既定の api-diff は、型変数の名前だけが違う要素（`(Maybe<'a> | Maybe<'b>)`）の鍵を区別できず、書いた順のまま名前を付けます（`b9a31cce` から同じ。検査器はこの組を `E1041` で拒否します）。

#### 検証者の指摘への対応

2026-11-12。上の対応（`66e29dd2`）を検証者が調べ、指摘 1 の直し方に 1 件の問題を見つけました（軽微）。`66e29dd2` のバイナリで再現し（`target/scratch/a24/copilot2-verify/p1q/a`・`b`）、直しました。

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| `blind` の鍵は、まだ名前のない region をすべて `_` にする。そのため、同じ名前の region を別の位置で使う 2 つの要素 `({a b} ref {a} i64 -> ref {b} i64 -> i64)` と `({a b} ref {b} i64 -> ref {a} i64 -> i64)` の鍵が等しく（`({_ _} ref {_} i64 -> ref {_} i64 -> i64)`）、安定な並べ替えは書いた順を残す。2 つの要素は同じ `r0`・`r1` を使うので、正準形は書いた順で変わる。`b9a31cce` の鍵は `ref {r0} … ref {r1}` と `ref {r1} … ref {r0}` で、前者がいつも先だった。上の判断 1 の「違うのは要素の順に依存しなくなったこと」は、名前の違う region の要素には正しいが、名前を共有する要素では逆に、変更でなかった並べ替えが変更になった（検査器が拒否する署名だけ） | 2 つの要素を逆の順に書いた 2 つの版の `def f :: own (… \| …) -> unit` の `api-diff` は、`b9a31cce` で `summary: 0 breaking`、`66e29dd2` で `breaking: changed def Lib.f`（`1 breaking`）。検査器はこの署名を拒否する（最初の診断は `E1041`。region を比べない要素の比較では同じ要素） | まだ名前のない region と長さパラメーターは、鍵の中で、その要素の中で最初に現れた順の番号（`_0`・`_1`…）にしました。番号は要素ごとに 0 から数えます。鍵の中にある共用体の各要素は、その共用体の前の番号から数え、その共用体を読み終えたら番号を戻します（判断 4）。上の 2 つの要素の鍵は `({_0 _1} ref {_0} i64 -> ref {_1} i64 -> i64)` と `({_0 _1} ref {_1} i64 -> ref {_0} i64 -> i64)` で、どちらの順に書いても前者が先です。直した後は `summary: 0 breaking` | `api_diff::tests::members_keep_which_region_is_which`、`api_diff::tests::nested_members_number_their_regions_afresh`、`tests/anonymous_unions.rs` の `api_diff_reorders_members_that_share_region_names` |

#### 判断（検証者の指摘）

4. **鍵の中の、名前のない region と長さパラメーターの番号。** 鍵に求めることは 2 つです。(1) ほかの要素にも書いた順にも依存しないこと（`66e29dd2` で直した点）。(2) 要素の中で、どの region がどの region と同じかを保つこと（`b9a31cce` の鍵が、名前の表を進めることで保っていた点）。要素の中で最初に現れた順の番号は両方を満たします。鍵は、その要素自身と、共用体より前に付いた名前だけで決まるからです。共用体より前に名前の付いたものは、その名前（`r0`・`n0` など）のままで、`_0` などの番号と重なりません。鍵の中にある共用体（`[(S | I)]`）の各要素も、その共用体の前の番号から数えます。番号を書いた順に続けて数えると、外の要素の鍵が中の要素の書いた順に依存し、外の要素の順まで変わりうるからです（`S = ({a} ref {a} string -> i64)`、`I = ({b} ref {b} i64 -> i64)` を書いた順に数えると、鍵は `[(({_0} ref {_0} string -> i64) | ({_1} …))]` で、`Z = [(({c} ref {c} i64 -> string) | bool)]` の鍵より後になります。数え直すと `[(({_0} ref {_0} i64 -> i64) | ({_0} ref {_0} string -> i64))]` で、どちらの順でも `Z` より前）。番号の表は `Vec` で、戻すのは長さを切り詰めるだけです。`ty` の呼び出しは増えません（`nested_union_members_are_renamed_once` の数は同じ）。

   検査器が受理するプログラムの出力は変わりません。region は、要素の中にあれば検査器が拒否するので（`E1013`、要素どうしが region だけ違えば `E1041`）、受理するプログラムでは共用体より前に名前が付いており、番号になりません。名前のない長さパラメーターが要素の中に現れるのは `--effective` だけです（パーサーが `#N` を書くのは宣言した長さパラメーターだけで、`parameter` が型より先に名前を付けます。`--effective` では関数の署名の長さが `Variable("#N")` になります）。`--effective` は先にプロジェクトを解析するので、受理するプログラムしか読みません。番号が `_` と違う順を生むのは、2 つの鍵がある長さの位置まで等しく、その位置の番号が違う場合だけです。そのとき 2 つの要素はその位置に違う長さパラメーターを持ちます（それより前がすべて同じなら、番号も同じだからです）。検査器はそのような組を `E1041` で拒否します（`(([i64; N] * [i64; M] * bool) | ([i64; N] * [i64; N] * string))`、`(([i64; N] * [i64; N] * [i64; M]) | ([i64; N] * [i64; M] * [i64; M]))`、`(([i64; N] * [string; M]) | ([i64; M] * [string; N]))` で確かめた。`target/scratch/a24/copilot2/v1len`）。長さパラメーターにも同じ規則を使うのは、`blind` の規則を 1 つにするためです（出力が変わらないことは下の検証）。型変数は以前どおりすべて `'_` で、この指摘の外です（型変数の名前だけが違う要素は、上の「残した制限」のとおり）。

#### 検証（検証者の指摘）

- 直す前のコードで失敗すること: 番号の代わりに `_` を返すように戻すと（`66e29dd2` の鍵と同じ）、`members_keep_which_region_is_which` は `(({r0 r1} ref {r1} i64 -> ref {r0} i64 -> i64) | ({r0 r1} ref {r0} i64 -> ref {r1} i64 -> i64))` で、`api_diff_reorders_members_that_share_region_names` は `breaking changed def Main.f` で失敗しました。鍵の中の共用体の後で番号を戻す処理だけを外すと、`nested_members_number_their_regions_afresh` が `([(({r0} ref {r0} i64 -> string) | bool)] | [(({r1} ref {r1} i64 -> i64) | ({r2} ref {r2} string -> i64))])` で失敗しました（判断 4 の手計算どおり、`Z` が先になる）。
- 期待値は手で求めました。鍵は判断 4 のとおりで、順序は ASCII の順（`0` < `1`、`i64` < `string`、`i64 -> i` < `i64 -> s`）、名前は並べ替えた順に最初に現れた順で付けました（テストのコメント）。
- region の差分の確認（`target/scratch/a24/copilot2/verify2/regions.py`・`run-regions.sh`）: region を持つ要素（`ref {x} T` の引数を持つ関数型、region を量化した関数型、`ref` のタプル、それらの共用体の配列）の無作為の署名と、その要素を並べ替え region の名前を一貫して付け替えた版、または 1 か所の region を変えた版の組を作りました。独立した参照として、各共用体の要素のすべての並べ方について region を最初に現れた順に改名した文字列の最小値を正準形とし、2 つの版はそれが等しいときに同じとしました。10 回 × 300 組（同じ 2,226 組、違う 774 組）で、同じ組を変更と報告した数は `b9a31cce` が 367、`66e29dd2` が 7、直した後が 2 です。違う組を見落とした数はどれも 0 です。直した後の 2 件は、どちらも `66e29dd2` の 7 件に含まれます。そのうち 1 件（seed 5 の `f95`）は `b9a31cce` では正しく、2 件とも下の残した制限の形（鍵が等しく、region の名前を鎖のように共有する要素）です。
- 長さパラメーターの差分の確認（`verify2/lengths.py`・`run-lengths.sh`）: 長さパラメーター（`N`・`M`・`K`）を持つ固定長配列を要素に持つ署名と、要素を並べ替え長さの名前を付け替えた版（3 割は 1 か所を変える）の組から、検査器が両方の版を受理する組だけを残しました。4 回で 521 組（変更と報告したのは 135 組）の `api-diff --effective` の出力（標準出力・標準エラー・終了コード）は、3 つのバイナリで 1 byte も違いません。
- 以前の確認の取り直し: 検証者の `gen.py` の 6 ファイル（300 組ずつ）は、3 つのバイナリで出力が同じです。第 2 回の `fuzz1/fuzz.mjs` は、`b9a31cce` と比べて既定の比較 200 回、`--effective` 100 回で、違いは 0 です。`tests/fixtures/` と `examples/` の 186 単位の `api-diff`（既定と `--effective`、`--json`）は、`66e29dd2` と直した後で同じです。`api_diff.rs` を使うのは `api-diff` のコマンドだけで、`members` を通るのは正準形だけなので、ほかの出力（IR、`check`、`doc`、`kg`）は変わりません。
- 検証の結果: T0 は `cargo fmt --all -- --check`（差分なし）と `cargo clippy --all-targets --locked -- -D warnings`（警告なし）です。T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）は 3089 件のうち 3089 passed、4 skipped です（`target/scratch/a24/copilot2/verify2/t1.txt`）。上の 3085 件から増えたのは、上の T1 の後に足した `members_order_ignores_where_regions_first_appear`、`api_diff::tests` の 2 件、`tests/anonymous_unions.rs` の 1 件（84 件になった）です。`features.mjs anonymous_unions`（dev バイナリ）は 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`verify2/features.txt`）。文書は `node scripts/check-links.mjs --into docs/architecture.md`（17 links、0 broken）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）です。

#### 変更したファイル（検証者の指摘）

`src/api_diff.rs`（`Renaming` の `unnamed_regions`・`unnamed_lengths`・`unnamed`、`region`・`length` の `blind` の腕、`members`、単体テスト 2 件、既存のテスト 1 件のコメント。期待は変えていない）、`tests/anonymous_unions.rs`（1 件）、`docs/architecture.md`（「### 匿名共用体（A24）」の `api_diff.rs` の項）、このチケット。言語リファレンスと `docs/language.md` は変えていません。リファレンスは「要素の順序の違いは差分にしません」と書いています。直したのは検査器が拒否する署名の比較で、残る差も検査器が拒否する署名だけだからです（公開できるパッケージには現れません）。

#### 残した制限（検証者の指摘）

- 既定の api-diff は、鍵が等しく、region の名前を鎖や輪のように共有する要素（`((ref {c} string -> ref {a} i64 -> bool) | (ref {a} string -> ref {b} i64 -> bool))`、`((ref {a} i64 * ref {b} i64 * ref {c} i64) | (ref {b} i64 * ref {c} i64 * ref {a} i64))`）を並べ替えると、変更と報告します。どちらを先に改名するかでもう一方の名前が変わるので、等しい鍵の要素のどの順も 1 つの形を与えないからです。`b9a31cce`・`66e29dd2`・直した後のどれでも `1 breaking` です（`verify2/lim`）。1 つの形に決めるには、等しい鍵の要素の並べ方を試し、最小のものを後ろの型まで持ち越す探索が要ります。この探索は等しい要素の数について指数的で、対象は検査器が拒否する署名だけなので（前者は `E1041`: region を除くと同じ要素、後者は `E1013`: 借用の要素）、していません。

### Copilot のレビューへの対応（第3回）

2026-11-12。PR #102 への GitHub Copilot の 3 回目のインラインの指摘は 2 件です（`src/api_diff.rs`・`src/parse_control.rs`）。2 件とも分岐のバイナリ（`1cf9565c`）で再現し（`target/scratch/a24/copilot3/p1`・`p2`）、直しました。実在しない指摘はありません。

#### 指摘と対応

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. `src/api_diff.rs:391`: 構文だけの `api-diff`（検査器が拒否する署名も読む）で、`(Maybe<'a> \| Maybe<'b>) -> 'a -> i64` の要素を入れ替えると偽の破壊的変更になる。2 つの要素の鍵（`blind`）が等しいので書いた順が変数の番号を決め、後ろの引数が `'t0` から `'t1` に変わる | `def f :: own (Maybe<'a> \| Maybe<'b>) -> 'a -> i64` と要素を入れ替えた版の `api-diff` は `breaking: changed def Main.f`（`1 breaking`）。検査器はこの署名を `E1041` で拒否する | 鍵の等しい要素を `Renaming::tied` が順に改名し、宣言全体での名前の現れ方で順を決めます。決まらない選択は `Renaming::least` の探索が試し、最も小さい描画を正準形にします（判断 1〜4）。直した後は `0 breaking` | `api_diff::tests::tied_members_take_one_order`（8 組。変数・region・長さパラメーター・入れ子の共用体・7 要素）、`api_diff::tests::interchangeable_tied_members_need_no_search`、`tests/anonymous_unions.rs` の `api_diff_orders_tied_members_by_the_whole_signature` |
| 2. `src/parse_control.rs:1435`: `fn` は関数型の始まりでもあるので、関数ガードの腕 `\| (g: Maybe<fn() -> i64>, :? i64 as n) -> n` で `type_test_ahead` が `fn` から括弧の終わりまでを式として読み飛ばし、型テストを見落とす。`guard_predicate` が `<` を比較と読み、構文の誤りになる | 関数ガードの腕と `when` のある腕は `E0002 expected the closing delimiter`。`match` の腕では `Some (x: ...)` は（`pattern_argument_expression` の 2 つ目の走査が `:` で止まるので）通るが、`Some (Some (x: Maybe<fn() -> i64>, :? i64 as n))` は同じ `E0002` | 走査がパターンの注釈の `:` に出会うと、その型をパーサーと同じ文法で読み飛ばします（`annotation_end`。括弧の中では `type_expr`、外では `type_product`）。`fn(` は `type_primary` と同じく型の始まりで、式として読み飛ばすのは局所関数の `fn name` だけです（判断 5〜7） | `annotation_types_do_not_hide_type_tests`（31 の型の形 × 10 の位置を 2026 と 2027 で構文解析し、10 の形を関数ガードと `match` の腕で native と WASM の `-O0`／`-O3` で実行、述語のままの腕 3 つ） |

#### 判断

1. **正準形の定義（指摘 1）。** 鍵の等しい要素は、変数・region・長さパラメーターの名前だけが違う要素です。どの順に並べるのが正しいかは、それらの名前が宣言のほかの場所（後ろの引数、ほかの要素、ほかのフィールド）のどこに現れるかで決まります。そこで正準形を「鍵の等しい要素のどの順で描いても得られる描画のうち、最も小さいもの」とします。名前の付け替えと要素の並べ替えだけが違う 2 つの宣言は同じ描画の集合を持つので同じ形になり、描画は宣言そのものなので、それ以外が違う宣言が同じ形になることはありません（変更を見落とさない）。探索の範囲は宣言 1 つです（`def` の署名、record のヘッダーとフィールド、union のケース、class のヘッダーとメソッド、instance。`least` で包んだ描画）。指摘の「署名全体の変数の関係で決める」に従い、record のフィールドや class のメソッドまで含めました。
2. **試さずに決める規則（`tied`・`refined`）。** すべての順を試すと要素の数の階乗になるので、次の要素を、試さずに決められる限り決めます。(a) それまでに付いた名前を使った鍵（`Blind::Refine`。名前の付いた変数はその名前、まだ名前のない変数・region・長さはその要素の中で最初に現れた順の番号 `~0`…。`~` は付いた名前より後ろに並ぶので、次の要素は既に改名した要素と名前を共有するものになる）。(b) まだ名前のない名前の「色」。色は、その名前が宣言全体で現れる場所（`place`）の並びの順位です。場所は、部分をその位置（引数の番号、タプルの要素の番号、型引数の番号、フィールドの名前、制約のクラス）で、匿名共用体の要素をその鍵で呼んだ道筋なので、名前にも書いた順にもよりません。平らにした矢印の鎖の位置で数えるので、`A -> (B -> C)` と `A -> B -> C` でも同じです。最初の描画で鍵の等しい要素に出会ったときだけ、場所を集める描画（census）を 1 回します。(a)・(b) で最も小さい要素が 1 つなら決まりです。(c) 同じに書いた要素は 1 つの候補です。(d) 共用体を含まず、まだ名前のない名前がすべてその要素の中にしかない要素どうしは、入れ替えても宣言が変わらないので最初の 1 つに決めます（鍵が名前を除いて要素を決めるので、名前と位置の入れ替えが宣言をそれ自身に写す）。どれも名前と書いた順によらない規則なので、同じ宣言のどの書き方でも探索の範囲は同じで、判断 1 の正準形は変わりません。
3. **探索（`least`・`choose`）。** (a)〜(d) で決まらない選択だけを探索します。描画を、前の描画の選択を再生して最後の選択の次の候補を試す形で繰り返し（再帰ではなく選択の列を回す）、最も小さい描画を採ります。描画は宣言 1 つにつき 4,096 回までで、それを超えると見つかった中で最も小さい描画を使います（残した制限）。鍵の等しい要素のない宣言は 1 回描くだけで、census も探索もしません。検査器は鍵の等しい要素を拒否するので（変数だけ違えば `E1041`、region は `E1013`、長さパラメーターも `E1041`）、検査器が受理するプログラム（`--effective` が読むもの）の出力は変わりません。
4. **第 2 回の残した制限の解消。** 第 2 回の「既定の api-diff は、型変数の名前だけが違う要素（`(Maybe<'a> | Maybe<'b>)`）の鍵を区別できず、書いた順のまま名前を付けます」と、検証者の指摘への対応の「鍵が等しく、region の名前を鎖や輪のように共有する要素を並べ替えると、変更と報告します」は、この対応で解消しました（`tied_members_take_one_order` の 4・5 組目と `api_diff_orders_tied_members_by_the_whole_signature`）。第 2 回の文は記録として残します。
5. **型テストの走査（指摘 2）。** 指摘は `fn(` だけですが、走査は注釈の型のどの部分も読み違えます（第 1 回の `<`、第 2 回の括弧の中の `->`、今回の `fn`）。そこで型の部分を字句ごとに判定するのをやめ、注釈の `:` の後ろをパーサーの `type_expr`／`type_product` そのもので読みます（パーサーの複製で読む。`own_type_ahead` と同じ方法）。読めた型の後ろから走査を続けるので、型の中の `fn`、`<`、`<...>`・`[...]`・`fn()` の後の `->`、`>>`・`>>>` の閉じ、region、`dyn (A, B)` のどれも走査に影響しません。括弧の中の注釈は `type_expr`、腕の括弧の外の注釈は `type_product` で、パーサーの `pattern_type_arrow` と同じです。
6. **フィールドの `:`。** レコードのパターン `{ x: p }` とレコードの式 `R { x: e }` の `:` の後ろは型ではありませんが、区別せずに型として読みます。型は `:` を含まないので、型として読める値（`{ x: n }`）を読み飛ばしても型テストを読み飛ばすことはなく、括弧は釣り合っています。型として読めない値（`{ x: :? i64 }`、`{ x: (a, :? i64) }`）は読めずに終わり、そのまま走査します。
7. **匿名共用体を使わないプログラム。** 走査が型テストを見つけるのは隣接した `:`・`?` だけなので、`:?` を書かないソースの判定は変わりません。さらにソースの最後の `:?` の位置を字句解析の後に 1 度だけ求め（`Parser::type_tests_end`）、それより後ろの走査はすぐに「型テストなし」と答えます。そのため型を読む複製は `:?` を書いたソースの、最後の `:?` より前でしか作りません。

#### 検証

- 直す前のコードで失敗すること: 新しいテストを足し、ソースの変更だけを戻した状態（`src/api_diff.rs`・`src/parse_control.rs`・`src/parser.rs` を `1cf9565c` に戻し、単体テストが数える `RUNS` は空の置き換え）で新しいテスト 4 件を実行し、4 件すべてが失敗しました（`target/scratch/a24/copilot3/prefix-tests-final.txt`）。`annotation_types_do_not_hide_type_tests` は腕の括弧の外の注釈 `| t: Maybe<fn() -> i64> * (i64 | bool) & (_, :? i64 as n) -> n` で `E0002: expected '->' after the pattern and optional guard`、`tied_members_take_one_order` は最初の組で `-> 't0 -> i64`（期待は `-> 't1 -> i64`）、`interchangeable_tied_members_need_no_search` は `'z` が書いた位置のまま `'t7`、`api_diff_orders_tied_members_by_the_whole_signature` は `breaking changed def Main.f` です。
- 型の形の表（`target/scratch/a24/copilot3/forms`。テストと同じ 31 の形 × 10 の位置 = 310 のソース）を `1cf9565c` のバイナリで構文解析すると、16 が `E0002` でした（関数ガードの腕と `when` のある腕で `Maybe<fn() -> i64>`・`Result<Maybe<i64>, Maybe<fn() -> Maybe<i64>>>`、2 段の `Some` の中の `match` の腕で同じ 2 つ、腕の括弧の外の注釈で `fn()` の 3 つ・`Maybe<fn() -> i64>`・`Result<...>`・矢印の 2 つ・region を量化した関数型）。直した後は 0 です。残りの 294 は以前も構文解析できていました（`match` の腕の 1 段の `Some` は `pattern_argument_expression` の 2 つ目の走査が `:` で止まるため、`fn() -> i64` だけの関数ガードの腕は `guard_predicate` の走査が `<` より前の `->` で止まるため）。
- 匿名共用体を使うプログラムの出力: `:?` を書くのは `tests/fixtures/anonymous_unions` だけで（`tests/fixtures`・`examples`・`std`）、その IR（native と wasm32）・`check --json`・`doc`・`kg build`・`api-diff`（既定と `--effective`）の出力は、`1cf9565c` と直した後のバイナリで 1 byte も違いません（`target/scratch/a24/copilot3/cmp`）。`:?` を書かないソースは `type_tests_end` で走査に入らないので、以前と同じです。
- api-diff の差分ファジング（`target/scratch/a24/copilot3/fuzz/fuzz.py`）: 1 つの形から名前だけを変えて要素を作った匿名共用体（変数・region を持つ関数型や `ref` のタプル・入れ子の共用体・タプル・`Result`）を持つ `def` と record の組を作ります。2 つ目の版は、6 割は名前を一貫して付け替えて全共用体の要素を並べ替え、入れ子を組み替えたもの、4 割は 1 か所の変数か region を変えたものです。独立した参照として、Python で全共用体の要素のすべての並べ方について最初に現れた順に改名した文字列の最小値を求め、等しい組を同じとしました。10 の種 × 600 組 = 6,000 組（参照で同じ 3,847、違う 2,153）で、直した後は見落とし 0・偽の変更 0、`1cf9565c` は見落とし 0・偽の変更 2,401 です。要素を 5〜8 個にした同じ版だけの 6,000 組（`STRESS=1`）では、直した後が 0、`1cf9565c` が 5,521 です。鍵の等しい要素を作らない 3,000 組（`NOTIE=1`）では、2 つのバイナリの出力が 50 回すべてで 1 byte も違わず、どちらも見落とし 0・偽の変更 0 です。
- 対称な構造（`target/scratch/a24/copilot3/graphs`）: 要素を辺 `Maybe<('x * 'y)>` とする無作為のグラフ（頂点 3〜10、辺 3〜16）の 750 組では、直した後が 0、`1cf9565c` が 736 です。無向の辺 `Maybe<(('x | 'y) * i64)>` で Petersen グラフ・立方体・`C4 + C4`・`C8`・`K4`・`K3,3`・三角柱を書いた 70 組（`regular.py`）は、直した後が 0（7.0 秒）、`1cf9565c` が 70 です。判断 2 (a) の「付いた名前を前に並べる」を入れる前は、同じ 70 組に 158.6 秒かかっていました（答えは同じ）。`K5`・`K4,4`・4 次元の立方体（32 要素）・3×3 のルークのグラフの 20 組（`regular2.py`）も 0 で、4 次元の立方体は 1 組に約 13 秒かかります（dev ビルド、load average 約 68 の共用の機械）。輪 `Maybe<('a_i * 'a_{i+1})>`（3〜9 要素）と、後ろのタプルの位置だけで区別する 7・8 要素（`limit`。場所の色を入れる前は探索の上限に達して `1 breaking`）も、並べ替えと付け替えで変更になりません。
- 時間: 鍵の等しい要素のない宣言は 1 回描くだけです。第 2 回の深さ 63 の入れ子の鎖（`target/scratch/a24/copilot3/deep`）は、2 つのバイナリとも 0.1 秒前後です（負荷による揺れの範囲）。
- 期待値は手で求めました（テストのコメント。`tied_members_take_one_order` の 8 組の場所と色、型の形の実行値 `answer()` = 40、`mul 3 4` = 12 など）。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3093 件のうち 3093 passed、4 skipped です（`target/scratch/a24/copilot3/t1-final.txt`）。第 2 回の後の 3089 件から増えた 4 件は、上の表の新しいテストです（`tests/anonymous_unions.rs` は 86 件）。`tests/api_diff.rs` の既存のテストも、期待を変えずに通りました。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`target/scratch/a24/copilot3/features.txt`）。
- 文書: 言語リファレンスのページは変えていません。`node scripts/check-links.mjs --into docs/architecture.md`（17 links、0 broken）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）、`gen-docs.mjs --check`（19 blocks、0 stale）、`check-cli-docs.mjs`（4 checks、0 failed。どちらも dev バイナリ）。

#### 変更したファイル

`src/api_diff.rs`（`Blind`・`NameKind`・`Census`・`Colors`・`Ties`・`MAX_TIE_RUNS`、`Renaming` の `census`・`place`・`ties`・`unnamed_variables`、`least`・`replay`・`choose`・`counted`・`placed`・`named`・`tied`・`written`・`refined`、`ty` と `constraint` の場所の名札、`members`、`Written`・`Refined`・`has_union`、`function_text` と `add_items` の `least`、単体テスト 2 件）、`src/parse_control.rs`（`type_test_ahead`・`annotation_end`）、`src/parser.rs`（`Parser::type_tests_end`）、`tests/anonymous_unions.rs`（2 件と `assert_type_test_in`）、`docs/architecture.md`（「### 匿名共用体（A24）」の `type_test_ahead` と `api_diff.rs` の項）、このチケット。言語リファレンスと `docs/language.md` は変えていません。型テストはリファレンスが書いているとおりに関数ガードと `match` の腕のどこにでも書けるようになり、`api-diff` の「要素の順序の違いは差分にしません」は検査器が拒否する署名でもそのとおりになりました。

#### 残した制限

- 既定の api-diff の探索は、宣言 1 つにつき 4,096 回の描画までです（`MAX_TIE_RUNS`）。それを超えると、見つかった中で最も小さい描画を使うので、形が書いた順に依存することがありえます。起きうるのは、検査器が拒否する署名のうち、鍵の等しい要素が多く、鍵と色で区別できず、入れ替えもきかない場合だけです。上のファジングと対称な構造のどの組でも形は変わりませんでしたが、4 次元の立方体のように対称な 32 要素は上限まで描き、1 宣言に数秒かかります。
- 第 2 回の残した制限の 1 つ目（型テストを持たず、括弧の中の注釈に `<` を持つタプルのパターンが、`guard_predicate`・`pattern_argument_expression` の走査で述語と読まれる）は、そのままです（どちらも A24 より前からの走査で、匿名共用体を使わないプログラムの挙動を変えないため）。同じく A24 より前からの `guard_predicate` の走査は最初の `->` で止まるので、`(match ... with | Some h -> h | None -> 0) > n` のように括弧の中の `match` から始まる述語の腕はパターンと読まれ、`E0002 expected a pattern` です（`1cf9565c` でも同じ。`target/scratch/a24/copilot3/predmatch`）。テストの述語は関数を呼ぶ形（`value_of (Box { ... }) > n`）にしました。

#### 検証者の指摘への対応（第3回）

2026-11-12。上の対応（`1ebf66a9`）を検証者が調べ、3 件の問題を見つけました（どれも軽微）。3 件とも `1ebf66a9` のバイナリで再現し（`target/scratch/a24/copilot3-verify`、検証者の `bin/tsuzuri-new`）、直しました。指摘 1 のうち、組の外の部分を通じてだけ互いに独立な組は、下の残した制限のとおり残ります。

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. 既定の api-diff の探索は宣言の鍵の等しい要素のすべての選択を 1 つの積として試し、`MAX_TIE_RUNS` で止まる。選択の列は辞書順に試すので、最初の 4,096 回は最初の組が選択 0（先に書いた要素）のままで、名前を共有しない 13 個の 2 択（2^13 = 8,192 通り）では形が最初の組の書いた順に依存する。第 3 回の指摘 1 と同じ偽の破壊的変更が、大きさを変えて残っていた（`1cf9565c` でも同じで、第 3 回の後退ではない。検査器は `E1041` で拒否する） | 各引数が `(('aK * 'bK * 'cK) \| ('bK * 'cK * 'aK))` の 13 個の輪で、最初の輪の 2 つの要素だけを入れ替えた版の `api-diff` は `breaking: changed def Main.f`（`1 breaking`）。12 個（`vcap/n12`）は `0 breaking`。region の輪（`cap/n13`）も同じ | 鍵の等しい要素の組が、まだ名前のない名前のすべての出現を持つとき（宣言の census の出現の数と、組の中の数が等しい。`Renaming::confined`）、その組の順は組の外の何も変えないので、出会ったところで組だけを探索し、組の描画の最も小さい順を採ります（`Renaming::local`）。その選択は宣言の探索の選択肢になりません（判断 8）。13 個の輪は 1 つずつ 2 回描くだけになり、`0 breaking` | `api_diff::tests::independent_ties_are_searched_one_by_one`（14 個の輪 × 変数と region × 3 つの書き方: 正準形、`render` 3 回、局所の描画 28 回）、`tests/anonymous_unions.rs` の `api_diff_settles_independent_unions_one_by_one` |
| 2. 探索の費用。共用体を含む要素は入れ替えがきくとされないので、どの順も同じに描かれる場合でも上限まで探索し、1 回ごとに候補の数の 2 乗の `refined` を作る。class はヘッダーの探索の各描画で各メソッドの探索（`function_text` の `least`）を丸ごと走らせるので、費用が掛け合わさる（最大 4,096 × 4,096 回） | 検証者の数（dev ビルド）: `([('v0 \| i64)] \| ... \| [('v31 \| i64)])` とその逆順（`wide/k32`）で `1cf9565c` 0.03 秒、`1ebf66a9` 10.35 秒（64 要素 37.4 秒、4 引数 42.9 秒）。4 つの上位クラス制約 `Show<(('a * 'b * 'c) \| ('b * 'c * 'a))>` と 12 個の region の輪を持つメソッドの class（`cls/o4`）で 0.11 秒と 57.8 秒 | (a) 候補のすべてが private（まだ名前のない名前をその要素の中にしか持たない）なら、共用体を含む要素も、それぞれを今の名前から単独で描いた最も小さい描画（`local`）を比べて選び、試しません。private で共用体を含む要素の中の選択も `local` で決めます（判断 9）。(b) class のメソッドの文字列を、メソッドが届く名前に付いた名前と付いた名前の数で覚え、class の探索の描画ごとに作り直しません（判断 10）。(c) 上限を探索ごとから宣言ごとにし、入れ子の探索も合わせて 4,096 回までにしました（`Renaming::spend`。判断 11） | `api_diff::tests` の `private_members_with_unions_need_no_search`（32 要素 × 4 引数、局所の描画 4 × 529 回）、`class_searches_do_not_multiply`（`render` 8 回、局所 32 回）、`class_methods_are_searched_once`（class の探索の 6 回の描画がメソッドの文字列を再利用: `render` 13 回）、`searches_share_one_budget`（7! 通りの探索が 1 + 4,096 回、class の中でも 3 + 4,096 回で止まる）、上の `api_diff_settles_independent_unions_one_by_one` |
| 3. 型テストの走査が、型テストでない注釈の `:` ごとに `self.clone()` でパーサー全体を複製する。複製は `dyn_types`・`noncanonical`・`active_patterns`・`site_contracts` を写すので、`dyn` の型と注釈付きのパターンが多く、最後の近くに型テストのあるファイルの構文解析は (注釈の数) × (`dyn` の型の数) になる | 検証者の数: `def fK :: (i64 * dyn Format) -> i64 = \t -> match t with \| (x: i64, d: dyn Format) -> x` を 8,000 個と、最後に `\| :? i64 as n -> n` の関数（`q2/quad2`）。`fmt --check` は `1cf9565c` 0.56 秒、`1ebf66a9` 23.96 秒。`dyn` の型を `bool` にすると 2.91 秒と 3.01 秒 | 投機的な読みは `Parser::probe` で作る写しで読みます。`probe` は位置と文法を決める旗だけを写し、ソースとともに増える記録（`noncanonical`・`dyn_types`・`active_patterns`・`site_contracts`・`pending_statement`）を空で始めます。`Parser` から `Clone` を外し、`annotation_end` と `own_type_ahead` の両方が `probe` を使います（判断 12） | `parser::tests::probes_carry_no_records`（40 個の腕と 80 個の `dyn` の型の後の型テスト: 121 回以上の probe が運んだ記録は 0、構文解析の結果は同じ） |

#### 判断（検証者の指摘、第3回）

8. **局所の探索（指摘 1、`confined`・`local`）。** 鍵の等しい要素の組の、まだ名前のない名前が組の外に現れなければ、組のどの順も組の外の描画を変えません。組の後で付く名前の番号は組の中の名前の数だけ進み（順によらない）、後ろの鍵・色・選択はその名前を読まないからです。そこで組の順を「組だけの描画の最も小さいもの」で決めます。この規則は名前と書いた順によらない（同じ組のどの書き方も同じ描画の集合を持つ）ので、判断 1 の議論はそのまま成り立ちます。名前の付け替えと要素の並べ替えだけが違う宣言は同じ形になり、形は宣言の描画なので、それ以外が違う宣言が同じ形になることはありません。正準形の定義は判断 1 の「宣言の描画の最も小さいもの」から、「`confined` な組と private な要素は、その部分の描画の最も小さいもので決め、残る選択について宣言の描画の最も小さいもの」に変わります。正準形の文字列は等しさを比べるだけで、報告には `tsuzuri doc` の形を書くので（`Change` の `old`・`new`）、どの代表を採るかは出力に現れません。第 3 回の単体テストの期待値（8 組、`interchangeable_tied_members_need_no_search` の 3 回）は変わりません。census がない最初の描画では `confined` を判定せず（`false`）、名前のない名前を持たない組も `local` にしません（その鍵が書き方の違う要素をすべて区別するので、選択がない）。census にない名前は組の外にあるものとして扱います（起きないが、外す側に倒す）。
9. **private な要素（指摘 2、`ordered`）。** 第 3 回の判断 2 (d) は、共用体を含まない要素だけを入れ替えがきくとしました。共用体を含む要素では、`Refine` の鍵が中の共用体の要素と外の名前の関係を落とすので、鍵が等しくても同じ形とは限らないからです（`([Maybe<'a>] | [Maybe<'b>]) * 'a` と `... * 'c` は同じ鍵）。今回は、候補のすべてが private なら、共用体を含む要素はそれぞれを今の名前の表から単独で描いた最も小さい描画を比べ、最も小さいものを採ります。描画が等しい 2 つの要素は、名前の付け替えと中の要素の並べ替えだけが違い（描画は要素そのもの）、その名前がその要素の中にしかないので、入れ替えても宣言は変わりません。描画の違う要素の間の順も描画で決めます（名前と書いた順によらない規則）。private の判定は選ぶときの名前の表で行い（第 3 回と同じ）、名前のない名前を持たない要素は private にしません。private で共用体を含む要素は、候補が 1 つのときや探索で選んだときも `local` で描くので、その中の選択も宣言の探索の選択肢になりません。単独の描画は残る候補ごとに作るので、1 つの組で候補の数の 2 乗回です（64 要素で 2,080 回、各 1 回の描画）。
10. **class のメソッドの文字列（指摘 2、`add_items`）。** `function_text` の結果は、class の改名のうち、メソッドが届く名前に付いた名前と、付いた変数・region・長さパラメーターの数だけで決まります。ほかの名前を読まず、新しい名前の番号は数で決まり、探索の状態は `function_text` の `least` が作り直すからです。そこでメソッドだけの census で届く名前を書いたとおりに集め、(メソッドの位置, 届く名前に付いた名前, 数) で文字列を覚えます（`MethodKey`）。census の描画では覚えず、毎回描きます（名前の出現を数えるため）。匿名共用体を書かない class と `--effective` は探索しないので（`--effective` が読むのは検査器が受理するプログラムで、鍵の等しい要素はない）、census も覚えることもしません。
11. **宣言ごとの予算（指摘 2、`spend`）。** 判断 8〜10 で、名前を共有しない部分の探索とメソッドの探索は掛け合わさらなくなりました。それでも、組の外にも名前が現れ、色で区別できない組がいくつもあれば宣言の探索で掛け合わさり（残した制限）、局所の探索の中の探索や、class の名前に依存するメソッドの探索は、外の探索の描画ごとに走ります。そこで上限を探索ごとから宣言ごとに変え、宣言の探索のすべて（`least` の census と 2 回目以降の描画、メソッドの探索、`local` の 2 回目以降）で合わせて 4,096 回までとしました（`Rc<Cell<usize>>` を改名の写しが共有する）。各探索の最初の描画は数えませんが、予算を使い切った後の探索は最初の 1 回で終わるので、宣言 1 つの仕事は 4,096 回と宣言の大きさの積で抑えられます。予算の範囲で終わる探索の結果は以前と同じです。以前は別々に 4,096 回まで試せた複数の探索（class の別々のメソッドなど）は、合わせて 4,096 回を超えると以前より早く打ち切られます（どれも検査器が拒否する署名だけ）。
12. **`Parser::probe`（指摘 3）。** 型を読むパーサーの関数は、増える記録を読みません（`noncanonical` は上限の数と古いラムダの書き換え、`active_patterns` はパターン、`site_contracts` と `pending_statement` は本体でだけ読む）。probe の追加は probe とともに捨てます。そのため写しが記録を持たなくても読みの結果は同じで、probe 1 回の費用はそれより前に読んだ量によりません。`Parser` の `Clone` を外したので、ほかの場所が丸ごとの複製を作ることもありません。`own_type_ahead` も同じ複製で `own` の型ごとに同じ費用を払っていました（A24 より前から: `def fK :: own (i64 * dyn Format) -> i64 = \t -> 0` を 8,000 個並べたファイルで、下の表のとおり）。同じ `probe` を使うようにしました。

#### 検証（検証者の指摘、第3回）

- 直す前のコードで失敗すること: 新しいテスト 7 件を、ソースの変更だけを戻した状態で実行しました（`src/api_diff.rs` のテスト以外を `1ebf66a9` に戻し、テストが数える `LOCAL_RUNS` は空の置き換え。`Parser::probe` は複製と同じく記録を写す版）。7 件すべてが失敗しました（`target/scratch/a24/copilot3/verify-fix/prefix/prefix-tests-final.txt`）。`independent_ties_are_searched_one_by_one` と `private_members_with_unions_need_no_search` は `render` が 4,098 回（期待は 3）、`class_searches_do_not_multiply` は 69,685 回（期待は 8）、`class_methods_are_searched_once` は 28,695 回（期待は 13）、`searches_share_one_budget` は 4,098 回（期待は 4,097）、`probes_carry_no_records` は probe が運んだ記録が 4,961（期待は 0）、`api_diff_settles_independent_unions_one_by_one` は `breaking changed def Main.f` です。メソッドの文字列を覚えること（判断 10）だけを外すと、`class_methods_are_searched_once` は 30 回、`class_searches_do_not_multiply` は 10 回で失敗します（どちらも手で求めた数）。
- 期待値は手で求めました（テストのコメント。輪の 2 つの順の描画と、`t9`・`t10` のように桁の変わる名前を含む輪がないこと、制約の文字列の並び（`'t1 ` < `'t10` < `'t4`）、`render` と `local` の回数の内訳）。`probes_carry_no_records` の probe の数は、手で求めた 80 + 41 と一致しました。
- 検証者の再現の取り直し（dev ビルド、load average 30〜50 の共用の機械。`verify-fix/times.sh`・`times.txt`）:

  | 再現 | `1cf9565c` | `1ebf66a9` | 直した後 |
  | --- | --- | --- | --- |
  | `vcap/n13`（変数の輪 13 個） | `1 breaking` | `1 breaking`、0.81 秒 | `0 breaking`、0.00 秒 |
  | `cap/n13`（region の輪 13 個） | `1 breaking` | `1 breaking`、1.56 秒 | `0 breaking`、0.00 秒 |
  | `wide/k64`（`[('vN \| i64)]` 64 要素） | 0.01 秒 | 13.96 秒 | 0.05 秒 |
  | `wide/k32p4`（32 要素 × 4 引数） | 0.30 秒 | 15.86 秒 | 0.04 秒 |
  | `cls/o4`（上位クラス制約の輪 4 個とメソッドの region の輪 12 個） | `1 breaking`、0.01 秒 | 24.35 秒 | `0 breaking`、0.02 秒 |
  | `q2/quad2` の `fmt --check` | 0.34 秒 | 16.67 秒 | 0.34 秒 |
  | `q2/quad2` の `check`（キャッシュなしの写し） | 0.17 秒 | 8.57 秒 | 0.14 秒 |
  | `own/n8000` の `fmt --check`（`own_type_ahead`、判断 12） | 4.28 秒 | 4.69 秒 | 0.19 秒 |

  `1cf9565c` の `1 breaking` は、第 3 回の指摘 1 の偽の変更です（`vcap/n12`・`cls/o2` も同じ）。
- 第 3 回の差分ファジングと構造の取り直し（`verify-fix/run.sh`、直した後のバイナリ）: 6,000 組（同じ 3,847、違う 2,153）で見落とし 0・偽の変更 0、要素を 5〜8 個にした 6,000 組（`STRESS=1`）で 0、鍵の等しい要素を作らない 3,000 組（`NOTIE=1`）で 0、無作為のグラフ 750 組で 0、Petersen グラフなどの 70 組（`regular.py`、3.99 秒）で 0、`K5`・`K4,4`・4 次元の立方体・ルークのグラフの 20 組（`regular2.py`、41.47 秒）で 0 です。この 250 回のファジング（`NOTIE` の 50 回を含む）の `api-diff` の出力は、`1ebf66a9` と直した後で 1 byte も違いません（報告は `tsuzuri doc` の形を引くので、正準形の代表が変わっても出力は変わらない）。
- 匿名共用体を使わないプログラムの出力: `tests/fixtures` と `examples` の全単位の IR（native と wasm32）・`check --json`・`doc`・`kg build`・`api-diff`（既定と `--effective`）を `1ebf66a9` と直した後のバイナリで比べました（`verify-fix/compare.txt`）。186 単位の 9,577 個のファイル（`tests/fixtures/anonymous_unions` を含む）が 1 byte も違いません。
- 残した制限の形（`verify-fix/gen/later.py`、`later/n13`）: 13 個の輪にそれぞれ後ろの引数 `('aK | 'bK | 'cK)` を足すと、最初の輪の要素の入れ替えは `1cf9565c`・`1ebf66a9`・直した後のどれでも `1 breaking` です（12 個までは直した後で 0）。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3100 件のうち 3100 passed、4 skipped です（`verify-fix/t1.txt`）。第 3 回の 3093 件から増えた 7 件は上の表の新しいテストです（`tests/anonymous_unions.rs` は 87 件）。既存のテストの期待は変えていません。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`verify-fix/features.txt`）。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（1 ページ、12 links、7 examples、native の 12 回の実行）、`node scripts/check-links.mjs --into docs/architecture.md`（17 links、0 broken）、`node scripts/check-links.mjs`（0 broken）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）、`gen-docs.mjs --check`（19 blocks、0 stale）、`check-cli-docs.mjs`（4 checks、0 failed。どちらも dev バイナリ）。

#### 変更したファイル（検証者の指摘、第3回）

`src/api_diff.rs`（`MAX_TIE_RUNS` を宣言ごとの予算に、`advance`、`Renaming` の `budget`・`spend`・`given`・`local`・`confined`・`ordered`、`least` の予算、`tied` の局所の探索、`refined` の `private`、`Refined::private`（`interchangeable` に代わる）、class の `text_of`・`reached`・`known`（`MethodKey`）、単体テスト 5 件と `LOCAL_RUNS`）、`src/parser.rs`（`Parser::probe`、`Clone` を外す、`own_type_ahead`、単体テスト 1 件と `PROBES`）、`src/parse_control.rs`（`annotation_end` が `probe` を使う）、`tests/anonymous_unions.rs`（1 件）、`docs/architecture.md`（「### 匿名共用体（A24）」の `annotation_end` と `api_diff.rs` の項）、`_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md` と `docs/language.md`（api-diff の要素の順序の項に、下の残した制限を足した）、このチケット。

言語リファレンスを変えたのは、第 2 回の検証者の指摘への対応で変えなかったのと同じ文（「要素の順序の違いは差分にしません」）が、既定の比較が読む検査器の拒否する署名について、今も残る例外（下の 1 つ目）を書いていなかったからです。既定の比較はそのような署名も読むと書き、探索の上限と、例外が起きうる署名の形を足しました。

#### 残した制限（検証者の指摘、第3回）

- 組の外（ほかの引数など）にも名前が現れ、色で区別できない鍵の等しい要素の組は、宣言の探索の選択肢のままです。そのような組どうしは、名前を共有しなくても選択が掛け合わさり、4,096 回の予算を超えると形が書いた順に依存します（上の `later/n13`）。1 つの形に決めるには、名前と出現の位置で互いに独立な選択を見分け、それぞれの最小を組み合わせる比較（名前の番号の桁が変わっても位置がずれない、名前を番号として比べる描画）が要ります。一般には要素の並べ替えのもとでのグラフの正準形と同じ問題で、どの方法でも最悪の場合は指数的です。対象は検査器が拒否する署名だけなので、予算の範囲の探索にとどめ、リファレンスにこの例外を書きました。第 3 回の残した制限の 1 つ目（探索は「宣言 1 つにつき」4,096 回）は、実装が探索ごとの上限だったので、今回の判断 11 で書いたとおりの意味になりました。
- 予算の範囲でも、色で区別できない要素の多い組の探索は、描画ごとに候補の数の 2 乗の鍵を作ります。4 次元の立方体（32 要素）は予算を使い切り、1 組に約 10 秒かかります（dev ビルド、`regular2.py` の 20 組で 41 秒）。宣言 1 つの仕事は予算と宣言の大きさで抑えられますが、小さくはありません。

### Copilot のレビューへの対応（第4回）

2026-11-12。PR #102 への GitHub Copilot の 4 回目のインラインの指摘は 2 件です（`src/check.rs`・`src/anonymous_union.rs`）。2 件とも分岐のバイナリ（`ebde884f`）で再現しました（`target/scratch/a24/copilot4/p1`・`p2`・`leak`）。指摘 2 は実在し、直しました。指摘 1 は健全性の穴ではなく、チケットの規則（名前付きの union のペイロードと同じ述語）どおりの挙動です。ただし言語リファレンスなどが規則を実際より広く書いていたので文を改め、借用の追跡を固定するテストを足しました。

#### 指摘と対応

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. `src/check.rs:10507`: 正規化は `contains_reference()` で調べ、レコードのフィールドを見ないので、`record Stored { value: Maybe<ref i64> }` の `(i64 \| Stored)` が必要な `E1013` にならない。総称の具体化でも共有借用が入る。`Validation::check` で具体化した要素を `contains_stored_reference` で調べるべき | `(i64 \| Stored)` は受理（`p1/a1`〜`a3`・`a6`）。名前付きの `union U = A of Stored \| B` も同じく受理（`p1/a4`・`a5`。`check_union` も `contains_reference`）。借用を外へ持ち出す 18 通り（判断 1）はすべて `E1013 borrow_outlives_block` で、名前付きの対照と同じ。借用元が生きている間の使用は実行でき、`g5a`・`c3a`・`x3a` は 9（名前付きの `g5n` も 9）、`d4a` は 7 | 挙動は変えません（判断 1・2）。言語リファレンス・`docs/language.md`・`docs/architecture.md` の「借用を含む要素は `E1013`」を、実際の規則（要素の型に書いた借用。レコードと関数型の中は見ない。具体化で入る共有借用は借用検査が追う。排他借用は具体化の検査で拒否）に改めました | `members_that_hold_borrows_keep_their_loans`（受理 2、持ち出し 14、排他 2、型別名 1、実行 3） |
| 2. `src/anonymous_union.rs:1167`: 未定の推論変数の 2 回目以降の出現を、互いに独立な何でも合う型として数える。`def pair :: unit -> (Maybe<'a> * Maybe<'a>)` の `pair ()` を `((Maybe<i64> * Maybe<i64>) \| (Maybe<i64> * Maybe<string>))` へ注入すると両方の要素を数えて `E1041` になるが、2 つ目の要素には同じ変数が `i64` と `string` の両方でなければならない | `which (pair ())` は `E1041: this value of type (Maybe<an undetermined type> * Maybe<an undetermined type>) could be (Maybe<i64> * Maybe<i64>) or (Maybe<i64> * Maybe<string>) in ...`。配列の要素とタプルの要素（`boxes ()`）、総称の関数の値（`same`）、タプルのリテラル（`(n, n)`）も同じ | 1 回の比較の中で、推論変数が最初に出会った型を覚え（`Ties`）、2 回目以降はその型と比べます。推論の状態には書きません（判断 3〜6） | `repeated_inference_variables_fit_one_member`（実行 8、拒否 4） |

#### 判断

1. **指摘 1 は健全性の穴ではありません。** 匿名共用体は合成した generic な union の具体化で（D2）、借用検査（`ownership.rs`・`regions.rs`）は匿名共用体を特別に扱いません（どちらも匿名共用体を判定する箇所を持たない）。union の値が持つ借用は `carries_loans` がペイロード（レコードのフィールドを含む）を辿って決めるので、`(i64 | Stored)` の値は `Maybe<Stored>` の値と同じ借用を持ちます。調整役の指示にある経路をすべて試しました（`target/scratch/a24/copilot4/leak`）: フィールドに借用を持つレコード（region なし `r1a`、あり `r2a`）、タプルの中のレコード（`r3a`）、名前付きの union の中のレコード（`r4a`）、型引数に参照を渡したレコード（`g2a`）、借用を捕捉した関数の要素（`c1a`）、借用した引数を捕捉したクロージャを返す関数の結果（`c2a`）、借用したレコードを読む関数の結果（`f1a`、region の関数 `x2a`）、総称の関数の型引数を共有借用で具体化した要素（`Maybe<Maybe<ref string>>` の `g4a`、`Box<Maybe<ref string>>` の `g7a`、`dyn Display {r}` の `d3a`）。持ち出しの 18 通り（名前付きの対照 `r1n`・`c1n`・`c2n`・`f1n`・`g4n`・`d3n` を含む）はすべて `E1013`（`borrow_outlives_block`）です。排他借用の具体化（`g8a`・`g8n`）は `Validation::check` の `union_payload_exclusive_borrow`、型別名の引数に書いた借用（`t1a`、`type OrMaybe<'a> = (Maybe<'a> | i64)` の `OrMaybe<ref string>`）は正規化の `anonymous_union_borrowed_member`、借用を持つ `dyn` を借用を持たない `dyn` の要素へ注入すること（`d5a`）は `E1003` です。
2. **規則は変えません。** チケットの仕様（「型の正規化と要素の規則」の 3）は、要素の検査を「名前付きの union のペイロードと同じ述語」の `Type::contains_reference` と定めています（Copilot の言う「必要な `E1013`」は仕様にありません）。`check_union` も同じ述語で、`union U = A of Stored | B` を受理し、型引数としての共有借用（`Maybe<ref i64>`）を持てます。`Validation::check` は具体化したペイロードの排他借用だけを拒否します。Copilot の案（`Validation::check` で `contains_stored_reference`）は、名前付きの union なら通る安全なプログラム（`g5n` と同じ形の `g5a`）を匿名共用体でだけ、それも具体化の後に拒否し、総称の関数が型引数によって失敗するようにします。D2 の「型の性質は名前付きの union の既存の規則がそのまま決める」にも反します。健全性の穴がないので、規則を厳しくする理由がなく、名前付きの union と同じ規則のままとしました。直したのは、規則を実際より広く書いていた文書です（リファレンスの「借用を含む要素は `E1013`」は、借用を持つレコードも拒否するように読めた）。
3. **推論変数の一貫性（指摘 2、`Ties`）。** `member_fits` と `literal_fits` は、1 つの要素との比較の中で、未定の推論変数が最初に出会った型を `Ties` に覚え、変数に出会うたびに覚えた型を辿って（`tied_head`）比べます（`member_fits_tied`・`literal_fits_tied`。edition 2027 の借用する引数のアダプター（`Fit::Borrowed`）が引数の `ref` を見る所も覚えた型を辿る。辿らなければ、覚えた参照を見落として数えが減りうる）。単一化が変数をその型に解くのと同じ対応で、推論の状態（`Inference`）には書かないので、比較の後に何も残りません。要素ごとの比較は空の `Ties` から始め、`literal_fits` が配列のリテラルを固定長配列として読み直すときは、最初の読みで覚えた分を戻します（`mark`・`undo`）。`literal_fits` も同じ `Ties` を受け渡すので、タプルのリテラルの要素（`(n, n)`）も一貫して数え、「`literal_fits` は `may_member` の数える要素をすべて数える」性質（`conform_union` の `literal.len() > candidates.len()` の前提）が保たれます。`literal_fits` の要素ごとの比較に空の `Ties` を渡した版（`target/scratch/a24/copilot4/tsuzuri-untied`）では、`(n, n)` は `may_member` が 1、`literal_fits` が 2 で、`this value, which holds an array literal, could be (Maybe<i64> * Maybe<i64>) or (Maybe<i64> * Maybe<string>) in ...` という偽の `E1041` になりました。
4. **数えすぎる側に倒す（DECISIONS 2026-11-11 の「曖昧さを黙って選ばない」）。** 覚えるのは、単一化がその変数を必ずその型に解く対応だけです。そのため `Ties` によって「合わない」となる要素は、単一化でも必ず失敗し、数え方は単一化が受け入れる要素を落としません（曖昧さが黙った選択になることはない）。単一化が変数を解かない相手（排他スライスの対象 `ArrayView`。`unify` はこの組を何もせずに通す）、変数自身を含む型（`unify` は infinite type として拒否する）、`Application`・`Partial`・`Error` との比較では何も覚えず、変数は以後もどの型とも合いうるものとして数えます。未定の変数どうしが出会えば、数のリテラルの変数を根に残し（`literal_takes` の制約が後でも効く）、もう一方をそれに結びます。制約の違う 2 つの数のリテラルの変数が出会えば、結んだ側の制約は以後見ません（数えすぎるだけです）。
5. **診断の変化。** 以前は 2 つ以上の要素に数えた値のうち、どの要素にも一貫しては合わないもの（`((Maybe<i64> * Maybe<string>) | (Maybe<string> * Maybe<i64>))` への `pair ()`、`((i64 -> string) | (string -> i64))` への `same`）は、`E1041`（`anonymous_union_ambiguous_member`）から `E1003`（`anonymous_union_not_member`）になります。型テストも `agrees` で同じ比較をするので、要素が推論変数を含む `mk ()`（`((Maybe<?a> * Maybe<?a>) | i64)`）に `:? (Maybe<i64> * Maybe<string>)` を書くと、単一化の `E1003 expected Maybe<string>, found Maybe<i64>` ではなく `E1041`（`type_test_not_member`）です（第 2 回の指摘 7 の「要素でない型はすべて `type_test_not_member`」と同じ）。どれも拒否から拒否への変化です。両方の要素に一貫して合う値（`((Maybe<i64> * Maybe<i64>) | (Maybe<string> * Maybe<string>))` への `pair ()`）は以前どおり `E1041` です。
6. **D5 との関係。** D5 の「推論変数はどの型とも合いうる」は、1 回の比較の中で変数が最初に型に出会うまでのことになります。D5 の理由（単一化を試して巻き戻す仕組みが `Inference` にない）は変わらず、`Ties` は `Inference` に触れない、比較だけの対応表です。匿名共用体を使わないプログラムはこの比較に入りません（注入の入口は `names.anonymous_unions` が空なら何もしない）。

#### 検証

- 直す前のコードで失敗すること: 新しいテスト 2 件を足し、`src/anonymous_union.rs` だけを `ebde884f` に戻した状態で実行しました（`target/scratch/a24/copilot4/prefix-tests.txt`）。`repeated_inference_variables_fit_one_member` は最初の `which (pair ())` で `E1041: this value of type (Maybe<an undetermined type> * Maybe<an undetermined type>) could be (Maybe<i64> * Maybe<i64>) or (Maybe<i64> * Maybe<string>) in ...` を出して失敗しました。`members_that_hold_borrows_keep_their_loans` は通りました。指摘 1 は挙動を変えないので、直す前も後も通るテストで、持ち出しの拒否と、具体化した借用を読む実行を固定します（`dyn` の 2 通りはこの実行の後に足し、2 つのバイナリで同じ `E1013` になることを CLI で確かめました）。
- 部分ごとの比較（`ebde884f` のバイナリ `tsuzuri-before` と直した後の `tsuzuri-after`、`target/scratch/a24/copilot4/p2`）:

  | ソース | 直す前 | 直した後 |
  | --- | --- | --- |
  | `which (pair ())`、注釈した束縛、パイプ | `E1041`（`anonymous_union_ambiguous_member`） | 1 |
  | `kind (boxes ())` | `E1041` | 1 |
  | `call same` | `E1041`（`this value of type an undetermined type -> an undetermined type could be ...`） | 41 |
  | `let n = none ()` の後の `which (n, n)` | `E1041` | 1 |
  | `which (Some 1, Some "x")`、`mk ()` の型テスト（対照） | 2、10 | 2、10 |
  | `((Maybe<i64> * Maybe<i64>) \| (Maybe<string> * Maybe<string>))` への `pair ()` | `E1041` | `E1041`（変わらない） |
  | `((Maybe<i64> * Maybe<string>) \| (Maybe<string> * Maybe<i64>))` への `pair ()`、`((i64 -> string) \| (string -> i64))` への `same` | `E1041` | `E1003`（`anonymous_union_not_member`） |
  | `mk ()` への `:? (Maybe<i64> * Maybe<string>)` | `E1003 expected Maybe<string>, found Maybe<i64>` | `E1041`（`type_test_not_member`） |

  値の期待値は手で求めました: 2 つ目の要素は同じ変数に `i64` と `string` を求めるので最初の要素（1）、`(Some 1, Some "x")` は 2 つ目の要素だけに合う（2）、`same` は `i64 -> i64` の要素だけなので `same 41`（41）、`mk ()` は `i64` の要素の 0 なので 0 + 10。指摘 1 の再現の 52 のソース（`p1`・`leak`・`doc`。持ち出し、実行、型別名、排他借用、文書の文の確かめ）は、2 つのバイナリの `run` の出力が 1 byte も違いません。`"123456789"` は 9 文字です。
- `literal_fits` に `Ties` を渡すことの確かめ: 要素ごとの比較に空の `Ties` を渡した一時的な版（`tsuzuri-untied`）では、`which (n, n)` が `E1041: this value, which holds an array literal, could be (Maybe<i64> * Maybe<i64>) or (Maybe<i64> * Maybe<string>) in ...` になりました（判断 3）。
- 匿名共用体を使わないプログラムの出力: `tests/fixtures` と `examples` の全単位の IR（native と wasm32）・`check --json`・`doc`・`kg build`・`api-diff`（既定と `--effective`）を、`ebde884f` のバイナリと直した後のバイナリで取り直して比べました（`target/scratch/a24/baseline.sh`・`compare.sh`、`target/scratch/a24/copilot4/base-before`・`base-after`）。186 単位の 9,391 個のファイル（`tests/fixtures/anonymous_unions` を含む）が 1 byte も違いません（`compare.sh` は終了コード 0、差分の行なし）。変えた比較は注入と型テストの入口からしか呼ばれず、その入口は匿名共用体を使わないプログラムでは何もしません。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3102 件のうち 3102 passed、4 skipped です（`target/scratch/a24/copilot4/t1.txt`）。第 3 回の検証者の指摘への対応の後の 3100 件から増えた 2 件は、上の表の新しいテストです（`tests/anonymous_unions.rs` は 89 件）。既存のテストの期待は変えていません。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`target/scratch/a24/copilot4/features.txt`）。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変更した 2 ページ（`anonymous-union.md`・`type-inference.md`）に（23 links、14 examples、native の 26 回の実行）、`node scripts/check-links.mjs`（0 broken）、`--into docs/architecture.md`（17 links、0 broken）、`--into docs/language.md`（324 links、0 broken）、`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）。文の中の規則（共用体の型引数の借用は拒否、レコードの型引数の借用と関数型の引数の借用は受理、`dyn C {r}` と配列の中の借用は拒否）は `target/scratch/a24/copilot4/doc` の 7 つのソースで、名前付きの union の同じ形と比べて確かめました。

#### 変更したファイル

`src/anonymous_union.rs`（`Ties`、`member_fits_tied`・`literal_fits_tied`・`tied_head`・`tie`・`tied_mentions`。`member_fits` と `literal_fits` は空の `Ties` で始める入口になり、`Fit::Borrowed` の引数の比較も `tied_head` を使う）、`tests/anonymous_unions.rs`（2 件）、言語リファレンスの `built-in-types-and-modules/anonymous-union.md`（要点、注入の「まだ決まっていない型」、要素の規則の表と借用の規則の段落、まとめ）・`types-and-type-inference/type-inference.md`（注入の「それ以外の式」）、`docs/language.md`（匿名共用体の型の解決と注入）、`docs/architecture.md`（ファイルの表、「### 匿名共用体（A24）」の正規化と注入）、このチケット。診断の kind は増やしていません。

#### 残した制限

- 要素の借用の検査は、名前付きの union のペイロードと同じく書いた型だけを見ます。借用を持つレコードを要素にでき、借用は借用検査が追います（`(i64 | Stored)` の値は `Maybe<Stored>` の値と同じ借用を持つ）。名前付きの union の `A of Stored` と同じ規則で、匿名共用体だけを厳しくすることはしていません（判断 2）。
- `Ties` は単一化が必ず行う対応だけを覚えます。`Application`・`Partial` と推論変数でない型を比べた部分は中を辿らないので、その中の変数は何にも結ばれず、排他スライスの対象や変数自身を含む型と出会った変数も結びません。どれも以後はどの型とも合いうるものとして数えます（推論変数が `Application` そのものと出会えば、`unify` と同じくそれに結びます）。制約の違う数のリテラルの変数どうしが出会えば、片方の制約は以後見ません。どれも数えすぎる側（曖昧さの `E1041`）で、黙って要素を選ぶことはありません。

#### 検証者の指摘への対応（第4回）

2026-11-12。上の対応（`1c5b6a11`）を検証者が調べ、1 件の問題を見つけました（軽微）。`1c5b6a11` のバイナリで再現し（`target/scratch/a24/copilot4-verify/exp`。検証者の `bin/after` は `1c5b6a11`、`bin/before` は `ebde884f`）、直しました。調べる中で、同じ原因（`Ties` が覚えた型を辿る仕事に上限がない）の別の 2 つの形を見つけ（下の表の 2・3。`target/scratch/a24/copilot4/genrev.mjs`・`gendeep.mjs`）、合わせて直しました。

| 指摘 | 再現 | 対応 | 回帰テスト |
| --- | --- | --- | --- |
| 1. `Checker::tie` の出現検査 `tied_mentions` は `polymorph::map_type` で型を写し、覚えた変数に出会うたびにその型を覚えなしに辿り直す。互いを結び合う変数では覚えた型が結ぶたびに 2 倍になるので、指数の時間がかかる。`unify` は大きすぎる解を `bounded_type`（`E1017`）で拒否するが、`tie` には上限がない。`literal_fits_tied` からも同じ経路に入る | 総称の関数の結果 `(Maybe<'f1> * (Maybe<('f1 * 'f1)> * (Maybe<'f2> * ...)))` を、要素 `(Maybe<('m0 * 'm0)> * (Maybe<'m1> * (Maybe<('m1 * 'm1)> * ...)))` を持つ匿名共用体の引数へ渡す（検証者の `genexp.mjs`）。最後の成分が値は `string`、要素は `i64` なので、どちらのバイナリも `E1003` です。`1c5b6a11` の `check` は k=12 で 21.35 秒（検証者の数は 11.0 秒、k=13 で 44.5 秒）、k=14 と k=40 は 120 秒で終わりません。`ebde884f` は k=12 で 0.96 秒、k=14 で 0.11 秒。最後の成分を `i64` にした形（`ok12`）はどちらも `E1017` です | `tie` の出現検査を `solvable` に替えました。解いた変数と覚えた変数を辿りながら、型を写さずに `type_size` と同じく構成子を数え、変数自身が現れるか、`bounded_type` の上限（入れ子 `MAX_NESTING`、構成子 `polymorph::MAX_TYPE_SIZE` = 4,096）を超えれば結びません（判断 7） | `tied_inference_variables_stay_within_limits` の doubling（k=40。`E1003` と、最後の成分を `i64` にした `E1017`） |
| 2. （調べる中で見つけた形）覚えた型が結ぶ時にはどれも小さくても、後から結ばれる変数を通じて入れ子になると、`member_fits_tied` が 2 つの成分を比べる歩みそのものが 4^k 段になる（`f_i := (m_i * m_i)`、`m_i := (f_{i+1} * f_{i+1})` の後に `f1` と `m1` を比べる） | `genrev.mjs` の最後の成分が合わない形: `1c5b6a11` は k=11 で 3.93 秒、k=12 で 14.68 秒、k=30（テストの形）は 120 秒で終わりません。`ebde884f` は k=12 で 1.37 秒（どれも `E1003`） | 1 回の比較で、覚えた型を辿る回数と `solvable` が数える構成子を合わせて `TIE_STEPS`（65,536）手までとし、使い切った後は覚えた型を辿らず、新たにも覚えません。残りは第 4 回の前と同じく、変数を何にでも合うものとして数えます（判断 8） | 同じテストの lockstep（k=30、`E1003`） |
| 3. （同上）覚えた型を辿る比較の再帰の深さが、書いた型の入れ子でなく変数の数で決まる（`f_i := [|m_i|]`、`m_i := [|f_{i+1}|]` の後に `f1` と `m0` を比べると 2N 段） | `gendeep.mjs` の N=780（約 1,560 段、最後の成分が合わない形）: `ulimit -s 512` と `256`（KB）で、`1c5b6a11` は stack を使い切って終了コード 134、`ebde884f` と直した後は `E1003` です。`ulimit -s 1024` 以上では 3 つとも `E1003` | 比較が `TIE_DEPTH`（`2 * MAX_NESTING` = 256）段より深い所では、覚えた型を辿らず新たにも覚えません（判断 9） | 同じテストの deep（N=300、約 600 段、`E1003`） |

#### 判断（検証者の指摘、第4回）

7. **覚える型の上限（指摘 1、`solvable`）。** 「覚えるのは単一化が必ず行う対応だけ」（判断 4）に、単一化の上限を足しました。`unify` は解く型を `bounded_type` に通し、上限を超えれば `E1017` で、変数自身を含めば `E1015` で拒否します。`solvable` は同じ数え方（`type_size` と同じく構成子ごとに 1、入れ子は `MAX_NESTING` まで）を、解いた変数と覚えた変数をその場で辿りながら行い、上限で止まります。1 回の確かめは 4,097 個までの構成子しか見ず、型を写しません。上限を超える型に結ばない変数は以後も何にでも合うので、数え方は数えすぎる側にだけ動きます。結ぶ型が上限を超える要素は、値がその要素になれば値の型も上限を超えます（覚えた対応は単一化の解なので、結ぶ型は単一化した値の型の一部）。値の型は関数の検査の終わりに `bounded_type` に通されるので（`polymorph.rs` の型の確定）、そのような要素への注入は、数えても数えなくても `E1017` で終わります。
8. **比較の手数（指摘 2、`TIE_STEPS`）。** 判断 7 は結ぶ時の型を抑えますが、結ぶ時に小さい型も後から入れ子になるので、比較の歩みは抑えられません。そこで 1 回の比較（読み直しを含む）で、覚えた型を辿る回数（`Ties::follow`）と `solvable` が数える構成子（`Ties::step`）を合わせて `TIE_STEPS` 手までとしました。推論変数を結ばない比較は手を使いません。使い切った後は `tied_head` が覚えた型を辿らず、`tie` が結ばないので、残りの比較は第 4 回の前（`ebde884f`）と同じく変数を何にでも合うものとして数えます。そのため数え方は第 4 回の前より多く数えることはなく、要素を落とすこともありません。1 回の比較の仕事は、手数と、辿った型の書いた大きさで抑えられます（覚えた型を `Rc` で持ち、辿るたびに写さない）。65,536 手は、普通の比較が使う手（結ぶ変数の出現ごとに 1 と、結ぶ型の構成子の数）より十分大きく、使い切る比較も dev ビルドで約 3.4 ミリ秒です（下の検証）。`undo` は手を戻しません。
9. **比較の深さ（指摘 3、`TIE_DEPTH`）。** 比較の 1 段は、2 つの型の少なくとも片方へ 1 段降ります（関数型の残りの引数を比べる段は片方だけ降りる）。そのため、入れ子が `MAX_NESTING` 以下の 2 つの型の比較は `2 * MAX_NESTING` 段の中で終わります。`TIE_DEPTH` より深い所に届く比較の要素は、値がその要素になれば値の型の入れ子が `MAX_NESTING` を超えるので（判断 7 と同じく）`E1017` で終わります。その所の変数を何にでも合うものとして数えすぎるのは、そのような要素だけです（残した制限）。再帰の深さは、`TIE_DEPTH` に、覚えた型をそれ以上辿らずに比べる型の入れ子を足したものまでで、第 4 回の前の比較と同じ程度です。
10. **テストの期待値と時間。** 期待値は手で求めました（テストのコメント）: どの値も最後の成分の `string` が要素の `i64` でなく、`bool` はタプルでないので、どの要素でもありません（`E1003`）。最後の成分を `i64` にした doubling は、タプルの要素だけに合い、単一化は `'f1` を `('m0 * 'm0)`（構成子 3 個）、`'m1` を `('f1 * 'f1)`（7 個）に解き、以後は前の 2 倍に 1 を足すので、`'m6` は 2^13 - 1 = 8,191 個で `bounded_type` の上限（4,096）を超えます（`E1017`）。テストは時間を測らず、診断だけを確かめます。直す前のコードでは終わらないので、CI では nextest の打ち切り（15 分）で失敗します。

#### 検証（検証者の指摘、第4回）

- 直す前のコードで失敗すること: 新しいテストの 4 つのソースを同じ生成で書き出し（`target/scratch/a24/copilot4/gentest.mjs`、`testsrc`）、`check` を 3 つのバイナリで 120 秒まで走らせました（`tmo.py`）。`1c5b6a11` は doubling・lockstep・`i64` の doubling の 3 つが 120 秒で終わらず、deep は `E1003` です（判断 9 の深さは 1 MB 以上の stack では足りるので、上の表の `ulimit -s` で確かめた）。直した後はどれも 0.08〜0.41 秒で、テストの期待どおりの `E1003`・`E1017` です。`ebde884f` も同じ診断です（0.08〜0.36 秒）。
- 再現の取り直し（直した後の dev ビルド `tsuzuri-fixed`、load average の高い共用の機械）:

  | 再現 | `ebde884f` | `1c5b6a11` | 直した後 |
  | --- | --- | --- | --- |
  | `genexp.mjs` k=12（`E1003`） | 0.96 秒 | 21.35 秒 | 0.26 秒 |
  | `genexp.mjs` k=14（`E1003`） | 0.11 秒 | 120 秒で終わらない（検証者） | 0.08 秒 |
  | `genexp.mjs` k=40（`E1003`）、`ok40`（`E1017`） | | | 0.09 秒、0.10 秒 |
  | `ok12`（最後の成分を `i64`、`E1017`） | 0.24 秒（検証者） | 7.46 秒（検証者） | 0.22 秒 |
  | `genexp4.mjs` 4 項の k=6（`q6`）、k=12（`E1003`） | 0.07 秒（検証者） | 3.1 秒（検証者） | 0.22 秒、0.09 秒 |
  | `genrev.mjs` k=12、k=30（`E1003`） | 1.37 秒 | 14.68 秒、120 秒で終わらない | 0.11 秒、0.09 秒 |
  | `gendeep.mjs` N=780、`ulimit -s 512`（`E1003`） | `E1003` | 終了コード 134 | `E1003`、0.13 秒 |

- 手数を使い切る比較の費用（`genmany.mjs`。31 個の要素がどれも lockstep の形で、最初の成分で区別する。最後の成分が合わないので、どの比較も手を使い切ってから要素でないと分かる）: 呼び出し 1 回で 0.20 秒（`ebde884f` は 0.10 秒）、50 回（`E1003` を 50 個）で 5.23 秒（同 0.25 秒）。1 回の呼び出しの 1 要素あたり約 3.4 ミリ秒です。
- 匿名共用体を使わないプログラムの出力: 変えたのは注入と型テストの入口からしか呼ばれない比較（`src/anonymous_union.rs`）と、`type_size` の上限の 4096 を定数 `MAX_TYPE_SIZE` に置いたこと（値は同じ）だけです。`tests/fixtures` と `examples` の全単位の IR（native と wasm32）・`check --json`・`doc`・`kg build`・`api-diff`（既定と `--effective`）を直した後のバイナリで取り直し、上の検証の `1c5b6a11` の出力（`base-after`）と比べました（`compare-verify.txt`。取り直した出力 `base-fixed` は比べた後に消しました）。186 単位の 9,391 個のファイル（`tests/fixtures/anonymous_unions` を含む）が 1 byte も違いません（`compare.sh` は終了コード 0、差分の行なし）。
- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3103 件のうち 3103 passed、4 skipped です（`target/scratch/a24/copilot4/t1-verify.txt`）。上の 3102 件から増えた 1 件は新しいテストです（`tests/anonymous_unions.rs` は 90 件）。既存のテストの期待は変えていません。
- `features.mjs anonymous_unions`（dev バイナリ）: 37 件で、native と WASM の `-O0`／`-O3` がすべて通りました（`features-verify.txt`）。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（1 ページ、13 links、7 examples、native の 12 回の実行）、`node scripts/check-links.mjs`（0 broken）、`--into docs/architecture.md`（17 links、0 broken）、`--into docs/language.md`（324 links、0 broken）、`node scripts/gen-docs.mjs --check`（19 blocks、0 stale）、`node scripts/check-cli-docs.mjs`（4 checks、0 failed）、`node scripts/check-guide-lite.mjs`（88 references、0 failed）。

#### 変更したファイル（検証者の指摘、第4回）

`src/anonymous_union.rs`（`tied_mentions` に代わる `solvable`、`tie`・`tied_head` の `depth`、`member_fits_tied`・`literal_fits_tied` の `depth`、`TIE_DEPTH`・`TIE_STEPS`、`Head`、`Ties` の `steps`・`follow`・`step` と `Rc` で持つ型）、`src/polymorph.rs`（`MAX_TYPE_SIZE`）、`tests/anonymous_unions.rs`（1 件と `tuple`）、言語リファレンスの `built-in-types-and-modules/anonymous-union.md`（注入の「まだ決まっていない型」に上限を超える比較の数え方）、`docs/language.md`（匿名共用体の注入の同じ文）、`docs/architecture.md`（「### 匿名共用体（A24）」の注入）、このチケット。診断の kind は増やしていません。

#### 残した制限（検証者の指摘、第4回）

- 上限を超える比較（結ぶ型が `bounded_type` の上限を超える、比較が `TIE_DEPTH` 段より深い、手が `TIE_STEPS` に達する）は、その先の変数を何にでも合うものとして数えます。第 4 回の前と同じ数えすぎで、要素を落とすことはありませんが、別の要素にだけ合う値が、その要素でも数えられて `E1041` になることがあります。前の 2 つは、値がその要素になれば `E1017` になる要素の比較だけです（判断 7・9）。手の上限は、型が上限の中でも、大きな型に結ぶ変数が入れ子に多くある比較で届きえます（`solvable` の手は、結ぶ型の大きさの和）。
- 手の上限は 1 回の比較の仕事を抑えるだけなので、仕事は（呼び出し）×（要素）× `TIE_STEPS` まで増えます（上の `genmany.mjs` の 50 回で 5.23 秒）。どの形も、ファイルの大きさに比例する時間で終わります。
- 単一化そのものの指数の時間は残ります。lockstep の形で最後の成分も合う値は、要素を 1 つ選んだ後の `unify` が、解いた変数をすべて写して展開するので（`resolve`。上限は解く型ごとの `bounded_type` だけ）、`ebde884f`・`1c5b6a11`・直した後のどれでも k=11 で 8〜13 秒かかります（`rok11`、`E1015`。順に 9.70、12.46、8.49 秒）。匿名共用体を使わない同じ形（引数の型を共用体でなくそのタプルにした `plain11`）も `ebde884f` で 11.02 秒、直した後で 6.73 秒かかるので、匿名共用体と第 4 回より前からある `unify` の挙動で、このチケットでは変えていません。
