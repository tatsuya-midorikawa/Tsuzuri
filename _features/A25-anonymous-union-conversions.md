# A25: 匿名共用体どうしの変換（部分集合からの拡大と `:? (A | B)` の絞り込み）

| 項目 | 内容 |
| --- | --- |
| ID | A25 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | [A24](_completed/A24-anonymous-unions.md)（done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-02（A24 と同時。HEAD `2f556480`） |
| 承認 | 拡大は GUIDE D-62 の決定 1（期待型で自動的に包む）の範囲として扱う。ほかの決定事項は既定案 |
| 手本にする既存実装 | A24 の `conform_union`・`type_test_pattern`・`union_case_pattern`。型付き IR の `match`: `TypedExprKind::Match { local, value, arms }`（`src/check.rs:4096`）と `TypedMatchArm`・`PatternAlternative`・`PatternStep`（`3955-3995`）。OR パターンの代替と網羅性: `pattern_alternatives` の `PatternKind::Or` の腕（`src/control.rs:585`）と `CoveragePat::Or`（`src/exhaustiveness.rs:24`） |
| 主な影響ファイル | `src/anonymous_union.rs`, `src/check.rs`, `src/control.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/E1041.md`, `tests/anonymous_unions.rs`, `tests/fixtures/anonymous_unions/Main.tz`, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`, `_tsuzuri/language-reference/compiler/diagnostics.md` |

## 実装者向け早見

- 範囲: 期待型の位置での部分集合の匿名共用体からの拡大と、`:? (A | B) as x` の絞り込み。新しい `TypedExprKind` は足さない。
- 最初に読む（この順）: ① A24 の D1〜D9 と「値の作り方」② 決定事項 D1〜D3 ③ A24 で足した `conform_union`・`type_test_pattern`・`union_case_pattern` ④ `src/control.rs` の `match` の型付けと `PatternKind::Or` の腕（585 行）⑤ `src/ownership_control.rs` の `view`（192〜226 行）。
- 最初に書くテスト: `tests/anonymous_unions.rs` の `widens_subset_unions` → `narrows_to_sub_unions`（E2E は suite `anonymous_unions` の `widen`・`narrow`）。
- 触ってはいけない: A24 の診断・suite の期待値（`type_test_union` のテストを除く）。要素数の等しい匿名共用体の単一化（`same` のまま）。
- 外から借りるもの: なし。
- 完了の見分け方: `let a: (i64 | string) = 1` の後の `let b: (f64 | i64 | string) = a` が通り、`-O3` の IR に確保がない。
- 迷ったら: 決定事項 → A24 の決定事項 → D-62 → 最も保守的な選択（拒否）。

## 目的

A24 の匿名共用体どうしで値を受け渡せるようにする。

```text
def parse_size :: string -> (i64 | string) = \text -> ...
def font_size :: (f64 | i64 | string) -> string = \size -> ...

font_size (parse_size "12")        // (i64 | string) を (f64 | i64 | string) として渡す（拡大）

match size with
| :? (i64 | f64) as number -> scale number      // number: (i64 | f64)（絞り込み）
| :? string as text -> text
```

- **拡大**: 期待型が匿名共用体 `U` の位置で、式の型が別の匿名共用体 `V` で、`V` のすべての要素が `U` の要素なら、`V` の値を `U` の値にする（同じ要素のケースへタグを付け替え、ペイロードを移す）。A24 の注入の続き（D-62 の決定 1）。
- **絞り込み**: 型テストに匿名共用体を書けるようにし（`:? (A | B)`）、`as x` の `x` を `(A | B)` で束縛する。A24 の kind `type_test_union` は出さなくなる（D-51 のとおり登録簿からは消さない）。

## 着手条件と停止条件

### 着手条件

- A24 が done であること。確認: `grep -n "^| A24 " _features/README.md`。
- A24 の suite `anonymous_unions` と `tests/anonymous_unions.rs` が HEAD で通ること。

### 停止条件

- `TypedExprKind::Match` を構文を経ずに作ると、所有権（`src/ownership.rs`・`src/ownership_control.rs`）か最後の使用の解析（`src/last_use.rs`）が、検査器の作る `match` と違う扱いをする（たとえば `TypedMatchArm::borrowed` の計算を要求する）。
- 拡大のために新しい `TypedExprKind` が要る。
- A24 の診断・suite の期待値を変える必要がある（`type_test_union` を出すテストを除く）。

## 現状（A24 の後）

- A24 の `conform_union` は、式の型が匿名共用体なら包まずに返し、呼び出し側が `same` で `U` と単一化する。要素の集合が違えば `E1003`（kind `type_mismatch`）。
- A24 の `type_test_pattern` は、テストする型が匿名共用体なら `E1041`（kind `type_test_union`）。
- 型付き IR の `match` は `TypedExprKind::Match { local, value, arms }` で、`value` を局所 `local` に置き、各腕の `alternatives`（`PatternAlternative { steps, bindings }`）を順に試す。`PatternStep::Test` はタグの比較など、`bindings` は束縛する局所と式の組。

## 仕様

### 拡大

期待型 `U`（匿名共用体）と式の型 `V`（匿名共用体、`V ≠ U`）について、A24 の「値の作り方」の 3 の最初の分岐を次に替える。

1. `V` と `U` の要素数が等しい → 従来どおり `same(V, U)`（等しくなければ `E1003` kind `type_mismatch`）。
2. `V` の要素数が多い → `E1003`（kind `anonymous_union_not_subset`。下の表）。
3. `V` の要素数が少ない → `V` の各要素 `Vi` について、`U` の要素で `resolve` 後に等しいものを探し、なければ `may_member(Vi, Uj)` を満たす `Uj` がちょうど 1 つなら `same(Vi, Uj)` してそれを使う。
   - どの `Uj` とも合わない要素があれば `E1003`（kind `anonymous_union_not_subset`）。
   - 2 つ以上と合いうる要素があれば `E1041`（kind `anonymous_union_ambiguous_member`、A24 と同じ kind）。
   - 対応 `map[i] = j` が決まれば、値を「拡大の `match`」（下の「設計」）に包む。

- 拡大は値の使用で、`V` の値は 1 回だけ評価し、ペイロードを `U` の値へ move する（Copy なら複製）。確保はない（タグの付け替えとペイロードのコピー）。
- 読み取りの引数（期待型 `ref U`）へ `V` の場所を渡すと、注入と同じく `V` の値は新しい一時値へ move される。`ref V` の値は拡大しない（`E1003`。help `an anonymous union is widened from an owned value`）。
- 入れ物・関数の中（`[V]` から `[U]`）は拡大しない（A24 と同じ）。
- 単相化で `V` と `U` が等しくなる場合（generic な関数の中で、`'a` の具体化しだいで要素が重なる）はない。A24 が要素の型変数を拒否し、正準順序が「不明」の組を拒否しているので、`V ⊂ U` は検査の時点の型で決まる。

### 絞り込み

`:? T` と `:? T as x` の `T` に匿名共用体 `W` を書ける。対象の型 `U` について:

- `W` の各要素が `U` の要素であること。そうでない要素があれば `E1041`（kind `type_test_not_member`。メッセージは最初の要素を名指しする）。
- `W == U` も書ける（つねに一致する。束縛は `U` の値そのものではなく、同じ型の作り直しになるので、所有権の規則は下と同じ）。
- 照合: `W` の要素ごとの代替（OR）。代替 `j` は「タグが `U` での `Wj` の番号に等しい」を試し、`as x` があれば `x` に「`Wj` のケースとして作った `W` の値」（ペイロードは `U` の値の `UnionPayload`）を束縛する。
- 所有権: 所有する対象からはペイロードを move して `W` の値を作る。借用の先・コレクションの要素・ガードの中など、A24 でビューになる位置では、`W` の要素がすべて Copy なら複製して作り、Copy でない要素があれば `E1041`（kind `type_test_narrow_borrowed`。下の表）。`as` のない `:? W` は束縛を作らないのでこの制限はない。
- 網羅性: `:? A | :? B` と同じ（`CoveragePat::Or` で要素ごとの `Constructor::Union`）。witness は A24 のまま。

### 診断

| コード | kind | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- | --- |
| `E1003` | `anonymous_union_not_subset` | 拡大の元の要素が拡大先にない、または元の要素が多い | `expected {U}, found {V}; {Vi} is not a member of {U}` | 式 |
| `E1041` | `type_test_narrow_borrowed` | ビューになる位置で、Copy でない要素を持つ `W` に `as` で絞る | `cannot narrow a borrowed value into {W} because {Wi} is not Copy` と help `match each member with its own ':?' arm, or match an owned value` | `:?` |
| `E1041` | `type_test_not_member`（A24 の kind） | `W` の要素が `U` にない | `{Wi} is not a member of {U}` と help `the members are ...` | 型 |
| `E1041` | `anonymous_union_ambiguous_member`（A24 の kind） | 拡大の元の要素が複数の要素と合いうる | A24 と同じ | 式 |

`E1041` の kind `type_test_union` は出さなくなる。`src/explain/codes/E1041.md` からその説明を消し、絞り込みの説明に替える。

### 例

| ソース | 結果 |
| --- | --- |
| `let a: (i64 \| string) = 1` と `let b: (f64 \| i64 \| string) = a` | 通る。`b` は `i64` の `1` |
| `let a: (i64 \| bool) = true` と `let b: (i64 \| string) = a` | `E1003 anonymous_union_not_subset`（`bool`） |
| `let a: (i64 \| string \| bool) = 1` と `let b: (i64 \| string) = a` | `E1003 anonymous_union_not_subset` |
| `def f :: (f64 \| i64 \| string) -> i64 = ...` に `(i64 \| string)` の変数を渡す | 通る。変数は move される（`string` を含むので Copy でない） |
| `match v with \| :? (i64 \| f64) as n -> ... \| :? string as s -> ...`（`v: (f64 \| i64 \| string)`） | 通る。網羅的 |
| 腕が `:? (i64 \| f64)` だけ | `E1021 match is not exhaustive; missing: :? string` |
| `match ref v with \| :? (i64 \| string) as x -> ...`（`v: ([i64] \| i64 \| string)`） | `E1041 type_test_narrow_borrowed`（`string` は Copy でない） |
| `match v with \| :? (i64 \| bool) -> 0 \| _ -> 1`（`v: (i64 \| string)`） | `E1041 type_test_not_member`（`bool`） |

## 設計

### 拡大の `match`

`widen_union(value: TypedExpr, from: V, to: U, map: &[usize]) -> TypedExpr`（`src/control.rs` に足す。`Checker` のメソッド）は、次と同じ型付き IR を構文を経ずに作る。

```text
match value with
| :? V0 as x0 -> (U の map[0] 番目のケース) x0
| :? V1 as x1 -> (U の map[1] 番目のケース) x1
...
```

- `local`: `value` を置く新しい局所（検査器が `match` の対象に使う局所の作り方と同じ関数で作る。`control.rs` の `match` の型付けを読み、同じ関数を使う）。
- 腕 `i`: `alternatives = [PatternAlternative { steps: [Test(UnionTag(Local(local)) == Int(i))], bindings: [(xi, UnionPayload { value: Local(local), case_id: i })] }]`、`guard = None`、`body = Construct { union_id: U の id, case_id: map[i], payload: Some(Local(xi)) }`、`borrowed` は空（所有する一時値を照合するのでビューはない）。
- 型: 全体は `U`、`xi` は `Vi`。
- 網羅性の記録（`coverage`）には足さない（作り方から網羅的）。生成コードは、どの腕にも一致しないときの既存のトラップを含むが、到達しない。
- 呼び出し元: A24 の `conform_union` の「`found` が匿名共用体」の分岐と、`coerce_argument` の先頭の分岐（パイプの左辺）。

### 絞り込みの代替

`type_test_pattern` で `tested` が匿名共用体 `W` のとき:

```text
let alternatives = []
for (j, Wj) in W.members:
    let k = U での Wj の番号
    let test = UnionTag(matched) == Int(k)
    let bound = Construct { union_id: W の id, case_id: j, payload: Some(UnionPayload { value: matched, case_id: k }) }   // 型 W
    let (next, coverage_j) = if binder: self.pattern_alternatives(Binding(x), bound)  else: ([], Wildcard)
    alternatives += combine([test], next)
    coverage = Or(coverage, Constructor(Union { U の id, k }, [Wildcard]))
```

ビューの位置かどうか（`TypedMatchArm::borrowed` に入るか）は、A24 の case のペイロードの束縛と同じ判定で決まる。束縛の式が `Construct` なので、ビューにできない。ビューになる位置では、`W` の要素がすべて Copy ならペイロードを複製して作り（`TypedMatchArm::borrowed` に入れない）、そうでなければ `type_test_narrow_borrowed`。判定に使う関数（`ownership_control.rs` の `view`、`192-226`）を読み、同じ条件を検査器の側で先に調べる。

## 実装手順

1. ベースライン: `node scripts/slot.mjs -- cargo nextest run --locked --test anonymous_unions` と `node tests/features.mjs target/debug/tsuzuri anonymous_unions`。
2. `widen_union` と `conform_union` の分岐。テスト: `widens_subset_unions`、`rejects_non_subset_unions`、`widens_pipes_and_read_parameters`、`widening_moves_the_value`、`widening_maps_generic_members`（`(Maybe<'a> | i64)` を `(Maybe<'a> | i64 | bool)` へ）。
3. 絞り込み。テスト: `narrows_to_sub_unions`、`narrowing_is_exhaustive_as_or`、`narrowing_copies_copy_members_from_views`、`rejects_narrowing_borrowed_non_copy`、`rejects_narrowing_non_members`。A24 の `type_test_rejections` から `type_test_union` の行を外す。
4. `src/diagnostic_kinds.rs` に 2 kind（`anonymous_union_not_subset` は `E1003`、`type_test_narrow_borrowed` は `E1041`）。
5. suite `anonymous_unions` に 3 関数を足す: `widen n`（`(i64 | string)` を作って `(f64 | i64 | string)` の関数へ渡す。値）、`narrow n`（`(f64 | i64 | string)` を `:? (i64 | f64) as x` で絞って別の関数へ渡す。値）、`widen_owned n`（`string`・`[i64]` の要素を持つ値の拡大と drop。値と `live == 0`）。native と WASM、`-O0` と `-O3`。
6. 生成コード: `-O3` で `widen` の拡大がタグの対応表（`switch` か `select`）とペイロードのコピーになり、確保（`@tz.alloc`）がないことを確かめて PR に書く。
7. ドキュメント: 下の「ドキュメント」。最終確認は A24 の手順 14 と同じ。

## テスト計画

- Rust: `tests/anonymous_unions.rs` に手順 2・3 の名前のテスト。各 `rejects` は code と kind を確かめる。
- E2E: 手順 5。
- 既存テストへの影響: A24 の `type_test_union` のテストだけを、絞り込みの受理に替える。

## ドキュメント

- `anonymous-union.md`: 「共用体どうしの受け渡し（拡大）」と「`:? (A | B)` で絞る」の節と `run=` の例。A24 の「注意点」の「部分集合は渡せない」を消す。
- `compiler/diagnostics.md` と `docs/language.md`: `E1041` の説明に絞り込みの誤り、`E1003` の kind。
- `docs/architecture.md`: 拡大の `match` と絞り込みの代替。

## 受け入れ条件

- 「例」の表のすべての行が書いたとおりになる。
- suite `anonymous_unions` が native と WASM、`-O0` と `-O3` で通り、`live == 0`。
- 拡大に確保がない（手順 6）。

## 落とし穴

- 拡大の腕の `body` で `Construct` の `payload` を `UnionPayload` で直接作ると、`local` の値の一部を move したまま `local` 全体を drop する。必ず腕の束縛 `xi` を経由する（検査器の作る `match` と同じ形にし、所有権の解析が move を見えるようにする）。
- `V` と `U` の要素数が等しい場合を拡大の経路に入れない（等しい集合なら同じ型で、違えば部分集合ではない）。
- 絞り込みの代替の `test` は、対象の値の参照外し（`autoderef`）の後の値で作る（A24 の `union_case_pattern` と同じ）。

## 対象外

- 縮小（`(A | B | C)` から `(A | B)` への暗黙の変換）。`:?` で絞る。
- 入れ物・関数の中の拡大。
- 名前付きの union と匿名共用体の間の変換。

## 着手時の再検討（2026-11-11、A24 の先端）

A24 のレビュー対応（`bd625b4d`）を取り込んだ `feat/a24-anonymous-unions` の先端から着手します。この節は、監査（2026-11-07、HEAD `b3dd7f1c`。A24 の実装前）の再検討案と計画の変更（すべて採用）、調整役の決定、A24 が実際に作ったコードを合わせたものです。この節と上の本文が食い違うところは、この節が優先します。

- 停止条件は、調整役の決定により「健全性を保つ最も保守的な選択を取り、このチケットに記録して続ける」に置き換えます。通っている安全なプログラムは通したままにし、テストの期待値は弱めません。
- 基準は A24 の先端のバイナリです。匿名共用体を使わないプログラムの IR は、このチケットの前後で 1 byte も変えません（`tests/fixtures/` と `examples/` で確かめます）。
- 2 段に分けて進めます。第 1 段は手順 1・2 と、手順 4 のうち `E1003` の kind（拡大）です。第 2 段は絞り込み（手順 3 と `E1041` の kind）、E2E（手順 5）、生成コードの確認（手順 6）、文書（手順 7）です。

### A24 が実際に作ったもの（名前と位置）

起票と監査が使った名前のうち、A24 の実装で確かめたものです。行番号は A24 の先端のものです。

| 役割 | 実際の名前と位置 |
| --- | --- |
| 期待型が匿名共用体の式 | `Checker::expression` の最初の腕（`src/check.rs:12031`）が `injects` を満たす式を `injected_expression`（`src/anonymous_union.rs:556`）へ渡す。`if`・`match`・ブロックなど（`passes_expected_down`、514）は期待型を腕へ渡すだけ |
| 引数（`U`・`ref U`） | `Checker::argument` の最初の腕（`src/check.rs:13243`）が `union_argument`（`src/anonymous_union.rs:571`）へ渡す |
| 値の判定（規則 2・3） | `union_value`（595）が形で要素を選び（`preselect`）、選べなければ `conform_union`（709）。包むのは `wrap`（496。`TypedExprKind::Construct`） |
| パイプの左辺 | `coerce_argument` の先頭（`src/check.rs:13595`）が `conform_argument`（`src/anonymous_union.rs:956`）を呼ぶ。`apply_piped` の早期の戻り（`src/check.rs:13390`）は `piped_into_union`（`src/anonymous_union.rs:984`） |
| 要素の候補 | `may_member`（1011）。関数型は、共通の引数と残りを `unify` と同じ形で比べる（A24 のレビュー対応） |
| 型テスト | `pattern_alternatives` の `PatternKind::TypeTest` の腕（`src/control.rs:694`）から `type_test_pattern`（1032）。タグの比較とペイロードの射影は `union_case_pattern`（970）で、名前付きの union の `case_pattern`（922）と共通。`TypeTestUnion` を出すのは 1088〜1097 行 |
| `match` の型付け | `match_value`（`src/control.rs:500`）。対象の局所は 524〜536 行、腕の `borrowed` は 577〜579 行、`borrowed_place` は 480 行。OR は 711 行 |
| 型付き IR | `PatternAlternative` 4889、`PatternStep`（`Test`・`Bind`）4895、`TypedMatchArm` 4972（`views` 4987）、`TypedExprKind::Match` 5085、`Construct` 5133、`UnionTag`（`i32`）5139、`UnionPayload` 5141（すべて `src/check.rs`） |
| 後段 | `last_use.rs::record_bindings` 464〜495、`llvm_control.rs` の `switch_plan` 402 と `match_expression` 573、`llvm.rs` の `emit_place` の `unreachable!` 6330、`ownership_control.rs` の `alias_source` 165・`view` 192・`protect` 228・`eval_match` 450 |
| 診断 | `AnonymousUnionNotMember`（`src/diagnostic_kinds.rs:205`）、`E1041` の kind は 691〜700 行、`tests/diagnostic_kinds.rs` の `RETIRED` は 320 行 |

A24 が計画から変えていて、このチケットに関わる点:

- 合成した宣言は `names.unions` にも入れず、id の範囲（`Names::is_anonymous_union`）で見分けます。要素の型は `Type::Union(id, 要素)` の型引数そのものです（`types.union_payload(id, 要素, k)` が `k` 番目の要素）。
- 規則 3 は、匿名共用体でない型への参照 `ref T` を、参照を外した `T` として要素に数えます（値は共用体へ move され、非 Copy の読み取りの引数は既存の `E1012`）。匿名共用体への参照 `ref V` は包まずに返し、呼び出し側が単一化するか再借用します。
- 期待型を下へ渡す式に `Select`・`Checked`・`Hole`・`DynDispatch` があります。形での事前選択に、配列・リスト・タプル・`new` のリテラルがあります。
- 関数型の要素は、型引数の置き換えで順序と区別が変わらないものだけが書けます（レビュー対応）。`V` の 2 つの要素が `U` の同じ要素に合うことは、正準化を通った型では起きません。
- A24 のテスト `members_commute_and_flatten` は、`def f :: (i64 | string) -> (i64 | string | bool) = \v -> v`（edition 2026 の所有の引数）が `E1003`（kind `type_mismatch`）になることを固定しています。これはこのチケットの仕様（部分集合からの拡大）がそのまま受理に変えるプログラムです。**この 1 行の期待を受理に替えます**（起票の「既存テストへの影響」に足す、許す期待値の変更です）。同じテストの、要素数の等しい別の集合（`(i64 | string)` を `(i64 | bool)` へ）の拒否は変えません。

### 拡大（`widen_union`）

所有権・最後の使用・効果・LLVM・`const_machine`・単相化は、作り方によらず型付きの `match` を同じに扱います。型付き IR に `MatchOrigin` はなく、網羅性の記録（`coverage`）も要りません。

- 判定は `conform_union` に置きます（呼び出し元は `union_value` と `conform_argument` の 2 つで、`coerce_argument` からの呼び出しは後者を通ります）。式の型 `V` が匿名共用体のとき:
  1. 要素数が `U` と等しい → 従来どおり包まずに返し、呼び出し側の `same` に任せます（違えば `E1003` kind `type_mismatch`）。
  2. 要素数が `U` より多い → `E1003`（kind `anonymous_union_not_subset`）。`U` のどの要素とも合わない最初の要素を名指しします。
  3. 要素数が少ない → 要素ごとに、`resolve` 後に等しい `U` の要素、なければ `may_member` を満たすただ 1 つの要素を選び `same` で単一化します。合う要素がなければ `anonymous_union_not_subset`、2 つ以上なら `E1041`（kind `anonymous_union_ambiguous_member`）。2 つの要素が `U` の同じ要素に対応した場合（正準化を通った型では起きない）も `anonymous_union_ambiguous_member` にします。
- `conform_argument` と `piped_into_union` は、左辺・引数の型が要素数の違う匿名共用体（かその参照）のとき、`same` を先に呼ばずに `conform_union` へ進みます。
- 対象の局所: `match_value` の 524〜536 行を小さな関数（`match_subject`）に切り出し、`match_value` と拡大の両方から使います（scope を push してから `self.bind("$match{start}", Generated)`、借用の先なら `self.borrowed` に足す）。
- 拡大と絞り込み（第 2 段）で共通の `retag(value, to, cases)` を `src/control.rs` に置きます。`cases` は「元のケース、先のケース」の組で、拡大は `V` の全要素、絞り込みは `W` の要素だけです。`widen_union` は `retag` に全要素の対応を渡します。
- 腕 i の形:
  - steps は `[Test(Binary(Equal, UnionTag(Local(local)): i32, Int(i): i32))]` の 1 つだけにします（`switch_plan` が switch にする形です）。最後の腕にも Test を付けます（利用者が書く `match` と同じ形にします。到達しないトラップのブロックは残ります）。
  - 束縛は `(x_i, UnionPayload { Local(local), i })`、`guard` は `None`。
  - 本体は `Construct { U, map[i], Some(Local(x_i)) }`、`borrowed` は空。
- `borrowed` を空にする理由: `value` は場所でもかまいません。借用の先なら、Copy の要素は複製され、Copy でない要素は注入と同じ所有権の誤り（`E1012` など）になります。
- span: `Match`・射影・`Construct`・テスト・対象の局所の `Local.span` は `value.span` にします。`Local($match)`・`Local(x_i)` の読み取りの式は、幅 0 の span（`value.span.end`）にします。`value.span` のままだと、hover と定義への移動（`semantic.rs` の `at` と式の項目）で、`a` の上に `x_i` の型が出ます。
- `last_use.rs::record_bindings` は束縛の span をキーにして AND で合わせます。そのため全部の `x_i` が `value.span` を共有すると、`(i64 | [i64])` の配列が複製されます（`@tz.alloc`、`W1006`、`@no_copy` の誤り）。
  - 監査の案: Copy かつ drop の要る型でない束縛を表に入れない。**条件は、匿名共用体を使わないプログラムの IR が 1 byte も変わらないこと**です。`for x in seq` の展開（`sequence_loop`、`src/control.rs:353`）は `$sequence_next` と `$sequence_item` の束縛に同じ span を与えるので、この案は既存のプログラムの束縛の扱い（複製か move か）を変える見込みです。実装時に確かめます。
  - 変わる場合の代案（監査の「腕ごとに異なる span」を、衝突しない形にしたもの）: `last_use.rs` には触らず、拡大の束縛の `Local.span` を 2 組に分けます。Copy かつ drop の要る型になりうる要素（型変数・未定の推論変数を含む型も数える）の束縛は `value.span`、それ以外（`i64`・`string` など、表の値が生成コードに関わらない型）の束縛は幅 0 の span（`value.span.end`）です。前者どうしは同じ `match` の同じ条件で決まるので打ち消し合わず、後者は表の値を誰も読みません。複製の警告（`W1006`）は `value.span` に出ます。
  - どちらを取ったかと確かめた結果は、下の「実装中の判断」に書きます。

### 絞り込み（第 2 段）: 起票の作り方は実現できない

`x` を `Construct` に束縛する案は使えません。理由は次の 2 つです。

- LLVM は束縛を場所として読み、場所でなければ `unreachable!` になります。
- 所有権は束縛の射影を 2 回評価し、パターンの手順の中では move を許しません（保護の借用）。

次の作り方に替えます。

1. `as` なし: タグの比較の OR にします。網羅性は `CoveragePat::Or` を最初の要素から畳み込みます。
2. `W == U` の `as x`: `matched` をそのまま束縛します（拒否しません）。
3. `W ⊊ U` の `as x`:
   - ビューの位置（`self.borrowed_place(&matched)`）かガードのある腕で、`W` に Copy でない要素があれば `E1041 type_test_narrow_borrowed` にします。
   - ガードのある腕（`W` がすべて Copy）: `active_pattern` と同じ形にします。手順は `[Test(tag == k_j), Bind(tmp, Construct { W, j, UnionPayload { matched, k_j } })]`、束縛は `(x, Local(tmp))` です。
   - ガードのない腕:
     - 束縛 `($narrow.x, matched)` と「絞り込みの記録」を非公開の `Alternative` に足します。
     - `match_value` が `x: W` を bind し、本体を `Block { [(x, retag(Local($narrow.x)))], body }` で包みます。`retag` は拡大と共通です。
     - 所有値からは move、ビューからは Copy の複製になります。
   - OR の両側は、同じ `x` と同じ `W` の絞り込みにします（違えば既存の `E1020`）。

### 診断

- kind は `since "0.1.0"` の組に (code, id) 順で足します。
  - `anonymous_union_not_subset` は `E1003` で、`anonymous_union_not_member` の後です（第 1 段）。メッセージは `expected {U}, found {V}; {Vi} is not a member of {U}`、`expected`／`found` を付け、help は `the members of {U} are ...; match the value with ':?' to keep the members that {U} has` の形にします（A24 の `members_help` に、修正方法を足す）。
  - `type_test_narrow_borrowed` は `E1041` で、`type_test_not_anonymous_union` の前です（第 2 段）。ガードの場合はメッセージを分けます。
- `anonymous_union_ambiguous_member`（A24 の kind）の拡大での文は `the member {Vi} of {V} could be {Uj} or {Uk} in {U}`、help は A24 と同じ `annotate the value, for example with a typed let binding` です。
- `ref V`（要素数の違う匿名共用体への参照）は拡大せず、`E1003` の kind `type_mismatch`（`expected {U}, found ref {V}`、`expected`／`found` 付き）にします。式が既定で読み取りの引数（`default_reads`）なら `read_only_hint`（`write 'own T'`）を自分で付け、そうでなければ help `an anonymous union is widened from an owned value` を付けます。要素数の等しい `ref V` は従来どおり呼び出し側に任せます（`ref U` は再借用、ほかは `type_mismatch`）。
  - Copy で drop の要らない `V`（`(i64 | bool)`）の引数は edition 2027 でも値なので、ふつうに拡大します。総称の読み取りの引数（`read_value_parameters`。`finish_expression` が参照を外す）は拡大の `match` に入り、Copy でない要素は所有権が `E1012` にします。
- `type_test_union` は第 2 段で出さなくなります。`tests/diagnostic_kinds.rs` の `RETIRED` の、`implicit_task_needs_builder` の後に足します（許す期待値の変更）。
- `E1041.md` は 1,500 文字以内にし、`gen-docs.mjs` を回します。

### その他

- **二項演算子の被演算子では拡大しません。** `binary_expression`（`src/check.rs:12574`）は、右の被演算子を左の型を期待型として検査します。そこで拡大すると `v == w` が `w` を move し、`w == v` は通らず左右で結果が違います。D-62 の位置にも入っていません。A24 の注入（`v == 1` の `1`）はそのままです。右の被演算子そのもの（括弧だけで包んだものを含む）が別の匿名共用体なら、従来どおり `E1003`（kind `type_mismatch`）です。被演算子が `if`・`match`・ブロックのとき、その腕は D-62 の位置なので拡大します。
- 例・テストは edition 2027 の読み取りの引数（ビュー）を前提にします。`| :? string as text -> text` は `E1012` なので、`own` を付けるか値を変えます。Rust のテストは、規約が関わるところで edition を明示します（`analyze` は 2026）。
- 「確保なし」は、再帰しない匿名共用体の所有値についてだけ書きます。型の循環にある匿名共用体は、ノードを確保します。絞り込みの配列の要素は、入れ子の `match` と同じく 1 回複製されます。
- 追加するもの:
  - E2E: `narrow_owned`。
  - テスト: 配列の要素を複製しない拡大、hover、借用・読み取りの引数の拒否、所有値の絞り込みの move、ガードの拒否、`W == U`、OR、比較で拡大しないこと。
  - 文書: `pattern-matching/match.md`、`type-inference.md`、`docs/language.md` の匿名共用体の節。
- `--emit llvm` は最適化前の IR です。

## 決定事項

### D1: 拡大を注入と同じ位置で行う

- 決定: 期待型が匿名共用体の位置だけで、部分集合の匿名共用体を自動で拡大する。
- 理由: 利用者が選んだ「期待型で自動的に包む」（D-62 の決定 1）を、要素の値から要素の集合へ延ばしたもの。値を変えず、タグを付け替えるだけで、確保しない。拡大できなければ注入もできない値を、利用者が `match` で包み直す手間を省く。
- 状態: 既定案（D-62 の決定 1 の範囲）。

### D2: 拡大の IR は型付きの `match`

- 決定: 新しい `TypedExprKind` を足さず、検査器が作るのと同じ形の `match` を作る。
- 理由: 所有権・最後の使用・効果・LLVM の各段に腕を足さずに済む。
- 状態: 既定案。

### D3: 借用からの絞り込み

- 決定: Copy の要素だけなら複製して作り、そうでなければ拒否する。
- 理由: 絞り込みの束縛は新しい値で、借用の先のペイロードを move できない。暗黙の深い複製（A15）を増やさない。
- 状態: 既定案。
