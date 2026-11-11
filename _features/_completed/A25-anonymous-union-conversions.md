# A25: 匿名共用体どうしの変換（部分集合からの拡大と `:? (A | B)` の絞り込み）

| 項目 | 内容 |
| --- | --- |
| ID | A25 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | [A24](A24-anonymous-unions.md)（done） |
| 後続 | – |
| 状態 | done（2026-11-11。下の「実装と検証」） |
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

### 実装中の判断（第 1 段: 手順 1・2 と `E1003` の kind）

計画に書いていなかった点と、確かめて決めた点です。

1. **`last_use.rs` は変えませんでした（監査の案は採らない）。** 監査の案（`record_bindings` で、Copy かつ drop の要る型でない束縛を表に入れない）を試すと、匿名共用体を使わないプログラムの IR が変わりました。`for item in Seq.once row`（`row: [i64]`）の展開は `$sequence_next` と `$sequence_item` の束縛に同じ span を与えていて、今は前者（Copy でない `Seq`）が後者の move を打ち消しています。案を入れると `item` が複製から move に変わります（`target/scratch/a25/try/seq` の `base.ll` と `filter.ll` が 24 行目から違う）。「使わないプログラムの IR を 1 byte も変えない」を満たさないので、再検討に書いた代案にしました: 拡大の束縛の `Local.span` を型で 2 組に分けます。
   - Copy かつ drop の要る型（配列・リストなど）と、型変数・未定の推論変数を含む型の束縛は `value.span`。同じ `match` の束縛は同じ条件（対象が所有されていて後で使われない）で決まるので、互いに打ち消しません。複製の報告（`W1006`・`E1031`）は拡大した値の位置に出ます。
   - それ以外（`i64`・`string` など）の束縛は幅 0 の span（`value.span.end`）。この組の表の値はどの段も読みません（複製するかどうかを決めるのは Copy かつ drop の要る型だけ）。
   - 残る制限: 総称の本体で、型変数を含む要素（`Maybe<'a>`）と配列の要素が同じ拡大にあり、具体化で前者が Copy でない型（`Maybe<string>`）になると、前者が後者の move を打ち消し、配列が 1 回複製されます（結果は同じで、確保が 1 回増えるだけです）。
2. **確保と複製を測りました**（`target/scratch/a25/try/ir1`）。再帰しない匿名共用体について:
   - 所有値を 1 回だけ使う拡大（引数・束縛）は `@tz.alloc` がなく、`@no_copy` を満たします（テスト `widening_does_not_copy_owned_array_members`。タグの選択は `switch i32`）。
   - 後でもう一度使う値（Copy の匿名共用体）は、`let w = v` と同じく複製されます。位置は拡大した値です（`@no_copy` なら `E1031`）。
   - ループの本体の中の拡大は、配列・リストの要素を 1 回複製します。束縛から `Construct` へ読むところで複製されます（`single_use_locals` はループの中の局所を 1 回の使用と数えず、`LastUses` は `match` の束縛を最後の使用の対象から外しているため）。`W1006` は値の直後の幅 0 の位置に出ます。
   - パイプの左辺の拡大（`v |> f`）は、配列・リストの要素を 1 回複製します（`LastUses` はパイプの中を調べないため）。`W1006` は値の位置に出ます。
   - 後ろの 2 つは、名前付きの union を利用者が `match` で包み直した場合とまったく同じです。基準のバイナリで `union V = A of i64 | B of [i64]` を `match v with | A k -> X k | B xs -> Y xs` で包み直すと、同じ 2 か所（ループの本体とパイプ）で同じ数の `@tz.alloc` が出ます（`target/scratch/a25/try/hand`）。D2（検査器が作るのと同じ形の `match`）の帰結なので、このチケットでは変えません。文書（第 2 段）の「確保なし」は、ループとパイプの外の所有値についてだけ書きます。
3. **二項演算子の被演算子**は、`Checker` のフィールド `operand`（右の被演算子の span）で伝えます。`binary_expression` が右の被演算子を検査する直前に置き、`injected_expression` が取り出して、同じ span の式を拡大しません。括弧だけで包んだ式は同じ span なので拡大しません（`v == (w)` も `E1003`）。被演算子が `if`・`match`・ブロックなら、その腕は別の span の式なので拡大します。並んだ式（配列・リストの要素、`if` の枝、`match` の腕）は、A24 の注入と同じく、前の式の型を期待型として受けて拡大します（期待型のない `[v, w]` は `w` を拡大し、`[w, v]` は `E1003`）。
4. **`ref V`**（要素数の違う匿名共用体への参照）は `conform_union` の中の `borrowed_union` が `E1003`（kind `type_mismatch`）にします。既定で読み取りの引数なら文に `read_only_hint` を足し（help なし）、そうでなければ help `an anonymous union is widened from an owned value` を付けます。要素数の等しい `ref V` と、被演算子の位置は A24 のままです。
5. **総称の読み取りの引数**（`def f :: 'a -> (Maybe<'a> | i64) -> ...` の 2 つ目）は `finish_expression` が参照を外すので、拡大の `match` に入ります。本体の検査は通り、Copy でない型で具体化した呼び出しが `E1005`（`no instance for Copy<string>`）になります（総称の本体が Copy の制約を推論する既存の仕組みです）。
6. **対応の重複**（`V` の 2 つの要素が `U` の同じ要素に合う）は、正準化を通った型では起きませんが、起きた場合は `E1041`（`anonymous_union_ambiguous_member`、`the members {Vi} and {Vk} of {V} could both be {Uj} in {U}`）にします。
7. **`anonymous_union_not_subset` の help** は `the members are i64, f64, string; match the value with ':?' and pass on the members that (i64 | f64 | string) has` の形です（A24 の `members_help` に修正方法を足しました）。数のリテラルの推論変数は、A24 と同じく既定の型で表示します。
8. 拡大の束縛の名前は `$retag{start}.{元のケース}`、対象の局所は `match_value` と同じ `$match{start}` です（どちらも `$` で始まり、hover・補完・定義の一覧に出ません）。
9. テスト（`tests/anonymous_unions.rs`。69 件から 77 件）: 手順 2 の 5 件（`widens_subset_unions`・`rejects_non_subset_unions`・`widens_pipes_and_read_parameters`・`widening_moves_the_value`・`widening_maps_generic_members`）と、監査が足した 3 件（`widening_does_not_copy_owned_array_members`・`hover_on_a_widened_value_shows_the_value`・`comparison_does_not_widen`）。値の期待値は手計算です（テストのコメントに式を書きました）。実行するテストは native と WASM の `-O0`／`-O3` で、規約が関わるものは edition を明示しています。`members_commute_and_flatten` の 1 行は、再検討に書いたとおり受理に替え、逆向き（要素の多い値を少ない型へ）の拒否を足しました。

10. 検証（第 1 段）:
    - T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
    - T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3070 件のうち 3070 passed、4 skipped（A24 の先端は 3062 件。増えた 8 件は上の 9 のテスト）。`tests/anonymous_unions.rs` は 77 件です。
    - 匿名共用体を使わないプログラムの出力: A24 の先端のバイナリ（`target/wt/a24/target/debug/tsuzuri`）とこの段のバイナリで、`tests/fixtures/` と `examples/` の 186 単位の出力（native と wasm32 の IR、`check --json`、`doc`、`kg build`、`api-diff` の既定と `--effective`。9,577 ファイル）を取り、比べました（`target/scratch/a25/baseline.sh`・`compare.sh`、`before`・`after`）。出力先のディレクトリ名を含む `kg.out` をそろえると、186 単位すべてが 1 byte も違いません（匿名共用体を使う `tests/fixtures/anonymous_unions` も、まだ拡大を書いていないので同じです）。
    - 生成物: `node scripts/gen-docs.mjs`（`diagnostics.md` の kind の表に `anonymous_union_not_subset` が入る）、`--check` は 19 blocks、0 stale。`check-cli-docs.mjs` は 4 checks、0 failed。
    - E2E の下書き（第 2 段の手順 5 で確定します。`target/scratch/a25/e2e-draft.patch`）: fixture に `(i64 | string)` を 4 要素の読み取りの引数へ渡す関数と、文字列・配列を持つ値を束縛・ループの中の代入・読まずに捨てる値で拡大する関数を仮に足し、suite `anonymous_unions` を dev バイナリで実行しました。native と WASM、`-O0` と `-O3` の 47 件（既存の 37 件と仮の 10 件）が通り、native は `live == 0` です。fixture には A24 の非公開の補助関数 `widen` がすでにあるので、手順 5 の export `widen` は、その補助関数の名前を変えてから足します。

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

### 実装中の判断（第 2 段: 手順 3〜7 と `E1041` の kind）

再検討の「絞り込み（第 2 段）」の作り方を実装し、確かめながら次を決めました。

1. **形を決めるのは `match_value` です（再検討の案からの変更）。** 再検討は、ガードのある腕かどうかを `pattern_alternatives` へ伝える旗（`Checker` のフィールド）を置き、ガードのある腕では `type_test_pattern` が手順（`Bind`）を作る案でした。実装では、`narrowing_pattern`（`type_test_pattern` から分けた関数）はどの腕でも「対象を隠れた名前 `$narrow.x` に束縛し、`Narrowing` を記録する」だけにし、`match_value` が腕ごとに形を選びます（`stepped_narrowings`）。旗は要りません。
   - ブロックの形（既定）: 本体を `Block { [(x, retag(Local($narrow.x)))], body }` で包みます（`narrowed_body`）。
   - 手順の形: 隠れた束縛を外し、手順の末尾に `Bind($narrowed{n}, Construct { W, j, UnionPayload { 対象, k } })`、束縛を `(x, Local($narrowed{n}))` にします（`step_narrowing`。`active_pattern` と同じ形）。ガードのある腕と、下の 3 の場合に使います。`W` の要素がすべて Copy のときだけです。
2. **`W` の要素がすべて Copy で `U` が Copy でないとき、隠れた束縛をビューにします**（`Narrowing::view`。腕の `borrowed` に入れる）。再検討どおりに隠れた束縛へ値を move すると、`match v with | :? (i64 | f64) as n -> ...` が `v`（`(f64 | i64 | string)`）を move し、後で `v` を使えなくなります。要素ごとの腕（`:? i64 as n`・`:? f64 as n`）は `v` を move しないので、それに合わせました。ビューは A24 でも所有する対象の下に現れる形（配列のパターンの要素など）で、`views()`・所有権・LLVM・`match_use` は対象が所有されているかどうかを区別しません。
3. **ビューにする絞り込みと同じ代替に、対象から Copy でない値を move する束縛があるときは、手順の形にします。** `(:? (i64 | f64) as n) as whole` は、ビュー（`n` の隠れた束縛）と move（`whole`）が同じ場所で衝突し、ブロックの形では `E1014` になりました。要素ごとの `(:? i64 as n) as whole` は通るので、通る形にしました。判定は保守的で、「同じ代替のほかの束縛（ビューにする絞り込みの隠れた束縛を除く）のうち、型が Copy でなく、借用の先でないもの」が 1 つでもあれば手順の形です（場所が重ならない `(:? (i64 | f64) as n, s)` の `s: string` でも手順の形になります。結果は同じで、switch にならないだけです）。`W` に Copy でない要素があれば、隠れた束縛と `whole` の 2 回の move で `E1012` です（`(:? string as s) as whole` と同じ）。
4. **`W == U` の `as x`** は、参照を外した対象をそのまま束縛します（`whole` だけのパターンと同じ。`CoveragePat::Wildcard`、テストなし）。`as` のない `:? U` もテストを作りません。束縛の名前は、`PatternKind::As` と同じくつねに束縛です（case や recognizer の名前としては解決しません）。
5. **OR パターン**は、どの代替も同じ `x` を、同じ型の対象から、同じ `W` で絞るときだけ通します。片側だけが絞り込み（もう片側は同じ型の素の束縛）か、絞る前の型が違えば `E1020`（kind `or_pattern_binding_mismatch`。メッセージは `both sides of an OR pattern must narrow the same anonymous union with the same ':?' test to bind one name`）、`W` が違えば `E1003`（A24 の「OR の両側は同じ型」と同じ）です。1 つの代替で同じ名前を 2 回絞るのは `E1001`（`duplicate or reserved name 'n'`）です。
6. **診断の文**（`type_test_narrow_borrowed`。位置は `:?`）:
   - 借用の先: `cannot narrow a borrowed value into {W} because {Wi} is not Copy`、help `match each member with its own ':?' arm, or match an owned value`。
   - 所有する値のガードのある腕: `cannot narrow a value into {W} in an arm with a guard because {Wi} is not Copy`、help `match each member with its own ':?' arm, or test the condition in the arm's body`。借用の先でガードもある腕は、借用の文です。
   - `type_test_not_member` は、`W` の要素のうち正準順序で最初の、`U` にない型を名指しします（位置は `:?` の後の型全体）。
7. **span**: `retag` に span の引数を足しました（拡大は値の span、絞り込みはパターンの span）。隠れた束縛と `x` の `Local.span` は `x` の名前、`retag` の束縛は拡大と同じ 2 組（パターンの span か、その終わりの幅 0 の span）です。`LastUses` の表のキーが重ならないので、隠れた束縛の move を `retag` の束縛が打ち消しません。hover は、`x` の上で `x: W`、`:?` の上で `W` です。
8. **複製と確保**（`target/scratch/a25/try/ir2`・`cp1`〜`cp8` で測りました）:
   - 再帰しない匿名共用体の拡大・絞り込みは、数・文字列の要素では `@tz.alloc` がなく、`@no_copy` を満たし、タグは `switch i32` で選びます（テスト `narrowing_retags_without_allocating`・`widening_does_not_copy_owned_array_members`、suite の `inspect`）。
   - 絞り込みの配列・リストの要素は、ブロックの形で 1 回（`retag` が隠れた束縛から読むところ。`LastUses` が `match` の束縛を最後の使用の対象から外すため）、手順の形で 2 回（手順の `Construct` と束縛）複製されます。`W1006`・`E1031` の位置はパターン（と、手順の形では束縛の名前）です。
   - 第 1 段の判断 2 の「ループの本体の中の拡大」は、正確には「拡大がループの本体の中にあるか、同じ関数のその後ろにループがあるとき」です。`single_use_locals`（`src/call_specialization.rs`）が、ループに出会うとそれまでに数えた局所をすべて「2 回以上」にするためで、名前付きの union を `match` で包み直した場合も、基準のバイナリで同じ位置に同じ複製が出ます（`try/cp5`）。文書はこの条件で書きました。
9. **`type_test_union`** は出さなくなりました。登録簿には残し（D-51）、`tests/diagnostic_kinds.rs` の `RETIRED` に足しました。A24 のテスト `type_test_rejections` の 1 件（`:? (string | i64)` を `E1041` にする期待）は、受理の期待に替えました（どちらも、起票と再検討に書いた、許す期待値の変更です）。
10. **E2E**（suite `anonymous_unions`。fixture の A24 の非公開の補助関数 `widen` は `either_sign` に改名）: `widen`・`widen_owned`・`narrow`・`narrow_owned` に加えて `narrow_views`（配列の要素からの絞り込み、ガードが通る場合と落ちる場合、読み取りの引数）を足しました。`narrow_owned` は、一時値・局所・`Maybe` のペイロードからの move、読まずに捨てる絞り込み、`as whole` との併用（手順の形）を通します。参照値は JavaScript の BigInt で、fixture の定義から書きました。

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

## 実装と検証（2026-11-11）

ブランチ `feat/a25-anonymous-union-conversions`（基準は `feat/a24-anonymous-unions` の `bd625b4d`。レビューの第 1 回で、その先端 `09fdb0de` を取り込みました。下の「レビューの指摘への対応（第1回）」）で、Phase 1（このチケットの全体）の手順 1〜7 を実装しました。

### 実装した手順

| 手順 | 内容 | コミット |
| --- | --- | --- |
| 着手時の再検討 | 監査・調整役の決定・A24 が実際に作ったコードを合わせた節 | `74141875` |
| 1・2・4（`E1003`） | 拡大: `conform_union` の分岐と `widened`、`control.rs` の `match_subject`・`retag`・`widen_union`、kind `anonymous_union_not_subset` | `b83389c4` |
| 3・4（`E1041`） | 絞り込み: `type_test_pattern` から `narrowing_pattern`、`match_value` の `stepped_narrowings`・`step_narrowing`・`narrowed_body`、kind `type_test_narrow_borrowed`、`type_test_union` の引退 | 第 2 段のコミット（この節を含む） |
| 5・6 | suite `anonymous_unions` の `widen`・`widen_owned`・`narrow`・`narrow_owned`・`narrow_views` と `inspect` | 同上 |
| 7 | 文書（下の一覧）、`E1041.md`、`gen-docs` | 同上 |

### 受け入れ条件

- 「例」の表の 8 行は（2 行目は仕様の規則 1 に従い kind `type_mismatch` です。表の `anonymous_union_not_subset` は書き誤りで、下の「レビューの指摘への対応（第1回）」の 7）、`tests/anonymous_unions.rs` の `widens_subset_unions`・`rejects_non_subset_unions`・`widening_moves_the_value`・`narrows_to_sub_unions`・`narrowing_is_exhaustive_as_or`・`rejects_narrowing_borrowed_non_copy`・`rejects_narrowing_non_members` が確かめます。表示は正準順序です（表の `(f64 | i64 | string)` は `(i64 | f64 | string)` と表示されます）。
- suite `anonymous_unions` は native と WASM の `-O0`／`-O3` で通り、native は `live == 0` です（69 件。A24 の 37 件と、このチケットの 32 件）。
- 拡大に確保がないこと（手順 6）: 最適化前の IR（`build -O3 --emit llvm`）で、`widen` と `narrow` の本体に `@tz.alloc` がなく、タグを `switch i32` で選びます（suite の `inspect` が毎回確かめます）。`opt -O3` の後は、`tz_widen` に `number_or_text`・`weigh` が畳み込まれ、残る `malloc` は `$"odd {n}"` の文字列の 1 つだけ、`tz_narrow` も `classify` の文字列の 1 つだけです（`target/scratch/a25/try/ir2` で確かめました）。配列・リストの要素の複製の条件は「実装中の判断」（第 1 段の 2、第 2 段の 8）のとおりです。

### 実行したコマンドと結果

- T0: `cargo fmt --all -- --check`（差分なし）、`node scripts/slot.mjs -- cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`: 3081 件のうち 3081 passed、4 skipped（A24 の先端は 3062 件。増えた 19 件は `tests/anonymous_unions.rs` のテスト）。途中の実行（絞り込みを作り直した直後）では、3081 件のうち `llvm_handoff::pruning_keeps_the_ir_of_every_project_valid` の 1 件が `No such file or directory` で落ちました。落ちたのはディレクトリの読み取り（`tests/llvm_handoff.rs:470` の `read_dir`）で、IR の検査には達していません。このテストは `tests/fixtures/`・`examples/` の各単位の下を、無視されるキャッシュ（`target/tz/kg`。同じ T1 の実行のほかのテストが 154 個の単位に書きます。消してから T1 を回すと、実行の途中で作り直されました）も含めて歩くので、歩いている間にその中のファイルが入れ替わったものと見ています（このチケットの変更とは無関係です）。`--test llvm_handoff` だけを回すと 13 件すべて通り、最後の全体の実行でも通りました。`tests/anonymous_unions.rs` は 88 件（A24 の先端は 69 件。第 1 段で 8 件、第 2 段で 11 件）。GUIDE §3.1 の回帰テスト 4 件（`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・`bounds_nested_builder_expansion_not_just_source_syntax`・`honors_the_exact_specialization_limit`）は T1 の中で通りました。
- T2:
  - `node scripts/slot.mjs -- node tests/features.mjs target/debug/tsuzuri anonymous_unions`: `anonymous_unions: native/WASM at O0/O3 passed`、69 cases。
  - ほかの suite は回していません。下の「匿名共用体を使わないプログラムの出力」のとおり、ほかの suite の fixture（`tests/fixtures/` の 185 単位）は IR が 1 byte も変わらないので、結果は基準と同じです。
  - `node scripts/slot.mjs -- node tests/last_use_fuzz.mjs target/debug/tsuzuri 40 1 4`: `40 programs, 80 native O0/O3 executions, seed 1, depth 4; zero differences`。
  - `node scripts/check-docs.mjs <変更した 7 ページ>`: `7 pages, 292 links, 31 checked examples, 60 native runs (O0/O3), 0 test projects, 1 generated blocks`。
  - `node scripts/gen-docs.mjs --check`: 19 blocks、0 stale。`node scripts/check-cli-docs.mjs`: 4 checks、0 failed。
  - `node scripts/check-guide-lite.mjs`・`node scripts/check-links.mjs --into _features/README.md`: 88 references、0 failed と、11 links、0 broken。
- 匿名共用体を使わないプログラムの出力: A24 の先端のバイナリの出力（第 1 段で取った `target/scratch/a25/before`）と、このチケットの最後のバイナリの出力（`after`）を、`tests/fixtures/` と `examples/` の 186 単位（native と wasm32 の IR、`check --json`、`doc`、`kg build`、`api-diff` の既定と `--effective`）で比べました。`tests/fixtures/anonymous_unions`（この段で関数を足した fixture）を除く 185 単位は、すべてのファイルが 1 byte も違いません（`compare-final.txt`）。その fixture も、A24 の 15 関数（`pick`・`make`・`number`・`mixed`・`element`・`owned`・`measure`・`label`・`unwrap`・`nested`・`build`・`total`・`recursive`・`generic`・`wide`）の本体は、単相化とラムダの連番をそろえると native と wasm32 の両方で同じです。
- `src/runtime/` は変えていないので、`scripts/check-runtime-includes.sh` は対象外です。

### チケットから外れた判断

再検討の節と、2 つの「実装中の判断」に書いたとおりです。要点:

- 絞り込みの束縛は `Construct` ではなく、隠れた束縛と `retag` のブロック、または パターンの手順（`Bind`）で作ります（起票の作り方は、束縛が場所でなければならないので実現できません）。
- `W == U` の `as x` は値そのものを束縛します（作り直しません。借用の先でも拒否しません）。
- 要素がすべて Copy の絞り込みは、所有する値を move しません（要素ごとの腕と同じ）。
- 二項演算子の被演算子では拡大しません。`ref V` は拡大せず `E1003`（`type_mismatch`）です。
- `last_use.rs` は変えていません（監査の案は、匿名共用体を使わないプログラムの IR を変えるため）。
- E2E に `narrow_owned` と `narrow_views` を足しました（起票は 3 関数）。
- 変えた既存の期待値は、起票と再検討に書いた 3 つだけです: `members_commute_and_flatten` の 1 行（部分集合の受理）、`type_test_rejections` の 1 件（`:? (string | i64)` の受理）、`tests/diagnostic_kinds.rs` の `RETIRED`（`type_test_union`）。

### 更新した文書

- `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（「共用体どうしの受け渡し（拡大）」と「`:? (A | B)` で絞る」の節と `run=` の例 3 つ。「部分集合は渡せない」の注意を削除）
- `_tsuzuri/language-reference/pattern-matching/match.md`、`pattern-matching.md`
- `_tsuzuri/language-reference/types-and-type-inference/type-inference.md`、`types.md`
- `_tsuzuri/language-reference/compiler/diagnostics.md`（`E1003`・`E1020`・`E1041` の説明と、生成した kind の表）
- `_tsuzuri/language-reference/index.md`
- `docs/language.md`（匿名共用体の節の「拡大」「絞り込み」、パターンの表、`E1041` の行）、`docs/architecture.md`（「匿名共用体（A24・A25）」の「拡大」「絞り込み」）
- `README.md`（機能の一覧と「変更の種類別」の行）、`src/explain/codes/E1041.md`（1,481 文字）

### 変更したファイル

`src/anonymous_union.rs`、`src/check.rs`、`src/control.rs`、`src/diagnostic_kinds.rs`、`src/explain/codes/E1041.md`、`tests/anonymous_unions.rs`、`tests/diagnostic_kinds.rs`、`tests/features.mjs`、`tests/fixtures/anonymous_unions/Main.tz`、上の文書、`_features/README.md`、このチケット。

### レビューの指摘への対応（第1回）

レビュー（2026-11-11、11 件。うち major 2 件）の指摘をすべて再現し、次のとおり対応しました。修正した指摘の回帰テストは、修正前のコードで落ちることを確かめています（3 は取り込んだ A24 の修正によるもので、取り込む前の動作はレビューの再現のとおりです）。

1. **読み取りの引数への拡大が配列の要素を複製する（major）**: 再現しました。edition 2027 の読み取りの引数（`ref U`）へ渡す拡大は `BorrowOperand(Match)` で、`LastUses::visit` が `BorrowOperand` の中を不透明に読むため、拡大の束縛が move と判定されず、配列が複製されていました（`@no_copy` の `E1031`、`@tz.alloc`）。`last_use.rs` に `lent_union_match` を足し、値の型が匿名共用体の `match` を貸すときだけ、ほかの `match` と同じに読みます。`operand_borrow` はその `match` を一時値として評価して借用し、呼び出しの後で drop するので、所有する対象のペイロードを一時値へ move できます（所有権の検査は変えていません。対象が後でも使われるなら、従来どおり最初の呼び出しで複製します）。匿名共用体を使わないプログラムの読み方は変わりません。テスト `widening_into_a_read_parameter_moves_array_members`（局所・一時値・束縛からの拡大に `@tz.alloc` がなく `@no_copy` を満たすこと、2 回使う値は 1 回だけ複製すること、値は native と WASM の `-O0`／`-O3`）と、E2E `widen_read`（7 件。native は `live == 0`、`inspect` が本体に `@tz.alloc` がないことを確かめる）。名前付きの union を利用者が包み直して読み取りの引数へ渡す場合（基準のバイナリでも複製する）は変えていません。文書は「読み取りの引数へ渡すときも確保をしない」と書きました。
2. **すべて Copy の要素へ絞る腕で、照合した変数へ代入する（minor）・move する（minor）と `E1014`**: 再現しました。どちらも保守的な拒否で、健全性の問題ではありません。隠れた束縛 `$narrow.x` のビューの貸し出しが腕の間ずっと生きているためです（所有権の生存の集合は、`let` の値の中で使う局所も「後で使う」と数えるので、`match` が関数の結果の位置にあるときだけ通っていました）。`match_value` は本体を検査した後、ビューにする絞り込みのある腕の本体が、対象に届く局所（`reached_locals`: 場所の根の局所、参照を返す式ならその式のすべての局所）を読む・書く・借用する・捕捉するなら（`reaches_any`）、その絞り込みを手順の形（パターンがペイロードを複製する。ガードのある腕と同じ形）にします。本体が照合した変数に触れない腕は、従来どおりブロックの形で switch になります（`tests/fixtures` の `narrow` の `inspect` も同じ）。テスト `narrowing_arms_may_move_or_replace_the_matched_value`（腕の中の move・代入・`ref mut` の先への書き込みと、触れない腕の switch）。
3. **純粋かどうかだけが違う関数型の要素が、生の `type_mismatch` になる（minor）**: 下の 6 で A24 の第 2 回のレビュー対応を取り込み、`type_test_member` と `widened` が `agrees`（調整しない比較）で要素を探すようにしたので、拡大は `anonymous_union_not_subset`、絞り込みは `type_test_not_member` になりました。テスト `conversions_do_not_adapt_function_purity`。純粋な関数の値を持つ共用体は、純粋でない要素を持つ共用体へは拡大できません（拡大はペイロードを調整せずに移すため。要素ごとの注入では調整されます）。この制限を文書に書きました。
4. **括弧で囲んだ型テストの `type_test_narrow_borrowed` の位置（minor）**: 再現しました（A24 の `type_test_not_anonymous_union` も同じでした）。`grouped_pattern` が、型テストのパターンの span に括弧を含めないようにしました。型テストの診断（`type_test_narrow_borrowed` とガードの文、`type_test_not_anonymous_union`、`type_test_unknown_scrutinee`）は括弧の中でも `:?` を指し、絞り込みの複製の報告（`W1006`・`E1031`）は括弧を除いたパターンを指します。型テストを書かないプログラムの構文木は変わりません。`src/formatter.rs` のコメントと `docs/architecture.md` の注記を直しました。テスト `type_test_errors_point_at_the_marker_inside_parentheses`。
5. **`E1012` が生成した局所 `$match<offset>` を名指しする（minor）**: 再現しました。基準のバイナリでも、一時値を照合する `(:? string as s) as whole` と名前付きの union の `(A owner) as whole` で同じです。所有権の `access` は、ムーブ後の使用の根が生成した `$match` の局所なら、名前を出さない文 `use of moved or partially moved matched value: two bindings of the pattern take the same part of it` と help `bind the whole value with 'as' or bind its parts, not both, and match the bound value again in the arm` にします。絞り込みの隠れた束縛の射影の span を束縛の名前にしたので、注記 `value moved here`（とビューの `value borrowed here`）は `match` 全体ではなく先に move した束縛を指します。照合した値が変数なら、従来どおりその変数を名指しします。テスト `double_moves_out_of_a_matched_temporary_name_no_generated_local`。
6. **A25 が A24 の先端に基づいていない（major）**: `feat/a24-anonymous-unions` の `09fdb0de`（A24 のレビュー対応の第 2 回）を `eb59b098` で取り込み、7 ファイルの衝突を解きました。A25 が `type_test_pattern` から移した要素の探索（`type_test_member`。`:? T` と `:? (A | B)` が共有）と、`widened` の要素の探索は、第 2 回の `agrees` を使います（retag はペイロードを調整せずに移すため）。第 2 回の、関数の値が引数の規約のアダプターを通して要素に入る規則は注入だけのもので、拡大と絞り込みには入れていません。検証は、取り込んだ後の木で測り直しました（下の「再検証」）。
7. **「例」の表の 2 行目（minor）**: 指摘のとおりです。要素の数が同じ `(i64 | bool)` から `(i64 | string)` へは、仕様の「拡大」の規則 1（と再検討の 1）により `same` で単一化し、`E1003` の kind は `type_mismatch` です（`rejects_non_subset_unions` が確かめ、参照ページの表もそう書いています）。表の 2 行目の `anonymous_union_not_subset` は規則 1 と食い違う書き誤りで、規則 1 を正とします。上の「受け入れ条件」に注記しました。
8. **総称の読み取りの引数の拡大にテストと文書がない（minor）**: 動作は「実装中の判断（第 1 段）」の 5 のとおりで、共用体なしの `\_x v -> v`（`Maybe<'a>` を返す）でも基準のバイナリが同じ `E1005`（`no instance for Copy<string>`）を出します（総称の読み取りの引数から値を取り出す既存の規則）。テスト `widening_a_generic_read_parameter_needs_copy`（`i64` の具体化で通る、`string` で呼び出す関数の位置に `E1005`、`own` で通る）と、参照ページと `docs/language.md` の 1 文を足しました。
9. **`for`・レコード・配列・リストのパターンでの絞り込みと `E1017` にテストがない（minor）**: テスト `narrowing_in_loops_records_arrays_and_lists`（`for` の照合と不一致のトラップ、レコードのフィールドからの複製と move、配列の要素、リストの先頭、11 個の絞り込みの `pattern_alternatives_limit`）を足し、`narrows_to_sub_unions` の `for` を名乗るコメントを直しました。
10. **ループによる複製の報告の位置の書き方（minor）**: 参照ページと `docs/language.md` を「後でもう一度使う値とパイプの左辺では拡大した値の位置、ループによる複製では拡大した値の直後の幅 0 の位置」に直しました。span そのものは変えていません（`retag` の読み取りの幅 0 の span は、hover と定義が拡大の束縛を指さないためのものです）。

ほかに、`tests/anonymous_unions.rs` の `api_changes`（A24）が一時ディレクトリをプロセス番号だけで名付けていたため、`cargo test`（1 プロセスの複数スレッド）で `api_diff_treats_member_order_as_equal` と `effective_api_hides_synthetic_names` が同じディレクトリを使って落ちることがありました。呼び出しごとの番号を名前に足しました（期待値は変えていません。nextest はテストごとにプロセスが別なので影響がありませんでした）。

変えた既存の期待値はありません。追加したテストは `tests/anonymous_unions.rs` の 7 件（上の 1〜5・8・9 に 1 件ずつ）と E2E の `widen_read` の 7 件です。

#### 再検証（取り込んだ後の木）

上の「実行したコマンドと結果」は取り込む前の木（基準 `bd625b4d`）のものです。取り込んだ後の木（この対応の最後のコミット）で、次を測り直しました。

- T0: `cargo fmt --all -- --check`（差分なし）、`node scripts/slot.mjs -- cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`: 3095 件のうち 3095 passed、4 skipped（`target/scratch/a25/t1-review1.txt`）。`tests/anonymous_unions.rs` は 102 件（取り込んだ A24 の 76 件、A25 の 19 件、この対応の 7 件）。GUIDE §3.1 の回帰テストもこの中で通りました。
- T2: `node scripts/slot.mjs -- node tests/features.mjs target/debug/tsuzuri anonymous_unions`: `anonymous_unions: native/WASM at O0/O3 passed`、76 cases（`widen_read` の 7 件を足した）。`node tests/last_use_fuzz.mjs target/debug/tsuzuri 40 1 4`: `40 programs, 80 native O0/O3 executions, seed 1, depth 4; zero differences`。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`anonymous-union.md`・`compiler/diagnostics.md`）: `2 pages, 62 links, 10 checked examples, 18 native runs (O0/O3)`。`node scripts/gen-docs.mjs --check`: 19 blocks、0 stale。`node scripts/check-cli-docs.mjs`: 4 checks、0 failed。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。`node scripts/check-links.mjs --into _features/README.md`: 0 broken。
- 匿名共用体を使わないプログラムの出力: A24 の先端 `09fdb0de` のバイナリ（`target/wt/a24/target/debug/tsuzuri`）と、この対応の最後のバイナリで、`tests/fixtures/` と `examples/` の 186 単位の出力（native と wasm32 の IR、`check --json`、`doc`、`kg build`、`api-diff` の既定と `--effective`）を `target/scratch/a25/baseline.sh` で取り、`compare.sh` で比べました。違うのは `tests/fixtures/anonymous_unions` の 64 ファイルだけで（この fixture は拡大と絞り込みを使うので、基準のバイナリでは検査が通りません）、ほかの 185 単位はすべてのファイルが 1 byte も違いません（`compare-r1.txt`）。この対応の変更は、どれも匿名共用体の値か型テストのパターンを持つプログラムにだけ働きます（`lent_union_match` は匿名共用体の型、`reached_locals` は絞り込み、`grouped_pattern` は型テスト、`E1012` の文は失敗するプログラムだけ）。

更新した文書（第 1 回）: `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（拡大の節の読み取りの引数・総称の読み取りの引数・純粋かどうか・複製の位置、絞り込みの節の所有権の表・`E1012` の文・括弧で囲んだ型テスト・複製の回数）、`_tsuzuri/language-reference/compiler/diagnostics.md`（`E1012` の行）、`docs/language.md`（匿名共用体の節の拡大と絞り込み）、`docs/architecture.md`（型テストの span、拡大の `LastUses`、絞り込みの手順の形と隠れた束縛の span）。

変更したファイル（第 1 回。取り込みの `eb59b098` を除く）: `src/last_use.rs`、`src/control.rs`、`src/parse_control.rs`、`src/ownership.rs`、`src/formatter.rs`（コメント）、`tests/anonymous_unions.rs`、`tests/features.mjs`、`tests/fixtures/anonymous_unions/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第2回）

レビュー（2026-11-11、4 件。うち critical 1 件）の指摘をすべて再現し、次のとおり対応しました。どの回帰テストも、修正前のコードで落ちること（2 は終わらないこと）を確かめています。

1. **クロージャや計算式の継続が捕捉した値を拡大・絞り込むと二重解放（critical）**: 再現しました（`c09` は `-O0` で `E2005 SIGTRAP`、`-O3` で `300` と表示してヒープを壊す）。捕捉を消費しないクロージャの呼び出しは、捕捉を借用ワーカー（`@tz.specialized.N`、`call_specialization::can_borrow`）へ貸します。`LastUses` は関数ごとに 1 回の解析で、ワーカーの捕捉の引数を所有する局所として扱うので、拡大の `match`（`retag`）の束縛が配列のペイロードを環境から move していました。呼び出しの一時値と環境の両方がその配列を解放します。原因は A24 の基準のバイナリにもあり、捕捉した配列に利用者が書く `match w with | ys -> len_own ys` と、`let!` の後の明示的な包み直しも二重解放でした（A25 は `weigh w`・`let u: U = w`・`return w`・`:? (A | B) as x` から暗黙に届くようにしました）。`llvm_control.rs` の `match_expression` は、照合する局所を関数が借用しているとき（`borrowed_locals`: 借用ワーカーの捕捉と、所有する関数の捕捉）、その局所からの束縛を move せず複製します。局所の読み出しを `clones_on_take` が同じ条件で複製するのと同じ規則です。`LastUses` は変えていません（ワーカーでない呼び出しは環境を複製して捕捉を所有して渡すので、そこでは move が正しく、速い）。変わるのは借用ワーカーが捕捉から束縛を move していたプログラム、つまり二重解放していたプログラムだけです。テスト `widening_a_borrowed_capture_copies_its_payload`（1 回と 2 回の呼び出し、own と読み取りの引数、型注釈の `let`、絞り込み、`let!` の後の `return w`、A24 の明示的な `match` と包み直し。`Alloc` の窓で確保と解放の数が等しいこと。edition 2026 と 2027、native と WASM の `-O0`／`-O3`）と、E2E `widen_captured`（8 件。native は `live == 0`。修正前は host の `tracked_free` の表明で落ちる）。
2. **入れ子の `:? (A | B)` の腕で検査の時間が深さの指数になる（minor）**: 再現しました（基準のバイナリの OR パターン `:? i64 | :? f64` も同じでした）。所有権の検査（`ownership_control.rs` の `eval_match`）が、腕の本体を代替ごとに調べていたためです。直前にガードと本体を調べた代替と同じ局所を同じ場所へ束縛し（`same_bindings`）、手順の後の State と到達可能性が等しい（`State::same`）代替では、ガードと本体を調べ直しません。同じ State から同じコードを調べ直しても、失敗の State・出口の State・値・診断は同じで、違うのは新しく作る loan の番号だけです（イベントと複製の記録は整列・重複除去され、回復した診断は根ごとに 1 回）。等しさは実行時に比べるので、State が違う代替（束縛する場所が違う `Some x | Ok x` など）は従来どおり代替ごとに調べ、受け入れるプログラムは変わりません。絞り込みの代替と、何も束縛しない `:? A | :? B` が該当します。レビューの生成器のプログラムで、`big-narrow-14` は 13.72 秒から 0.38 秒、`big-notouch-16` は 13.28 秒から 0.36 秒、`big-or-14`（A24 の OR）は 2.17 秒から 0.38 秒、深さ 20 の束縛つきの絞り込みは 120 秒で終わらなかったのが 0.26 秒です（debug バイナリ、user 時間）。テスト `nested_narrowing_arms_check_each_body_once`（深さ 30 の束縛つきの絞り込み・束縛なしの絞り込み・A24 の OR を検査し、深さ 3 の値を native と WASM の `-O0`／`-O3` で確かめる）。修正前は終わらず、7 分半の CPU 時間で止めました。時間の閾値は置いていません（深さ 30 の 3^30 回の訪問が終わらないことで落ちます）。
3. **拡大の後の `E1012` が「値の一部」と存在しないフィールドの借用を案内する（minor）**: 再現しました。拡大の束縛がペイロードの場所を move するので、所有権からは部分的な move に見えていました。`access` は、move の位置が拡大の生成した束縛（`$retag..`）の射影の span なら（`widened_at`）、注記を `value moved here into a wider anonymous union`、help を `a widening moves 'w'; bind the wider value once with 'let wide: (i64 | f64 | string) = w' and borrow 'wide', or move it last` にします（`ref w` は拡大しないので案内しません。型は拡大した先の正準表示です）。名前付きの union のケースから取り出したときの文は変えていません。テスト `a_second_widening_says_that_the_value_moved`（own と読み取りの引数、型注釈の `let`、help のとおり束縛した値を 2 回借用すると通ること、名前付きの union の文が変わらないこと）。
4. **部分適用の読み取りの引数へ拡大する変数が「一時値を `let` で束縛せよ」の `E1013`（minor）**: 再現しました（A24 の注入 `three k` も同じ文でした）。部分適用の読み取りの引数は関数の値が生きている間借用するので、変換した一時値は借用できず、拒否は正しい制限です。所有権の `place` の失敗（`borrow_of_temporary`）は、借用する式が局所の変換（拡大の `match` か匿名共用体の `Construct`）なら、kind はそのまま（`borrow_of_temporary`）で、メッセージを `borrow requires a local place; 'w' converted into (i64 | f64 | [i64]) is a temporary value`、help を `bind the converted value first, as in 'let wide: (i64 | f64 | [i64]) = w', and pass 'wide'; or take the parameter with 'own'` にします。注入も同じ文にしました（A24 の診断の期待値を変えるテストはありません。文の改善だけで、受け入れるプログラムは変わりません）。利用者が書いた一時値の文は変えていません。この制限を参照ページの「注意点」に書きました。テスト `a_converted_local_borrowed_beyond_the_call_names_the_conversion`（`let g = three w`・`5 |> three w`・注入の `three k`、help の 2 つの書き方が通り値が正しいこと、書いた一時値の文）。

変えた既存の期待値はありません。追加したテストは `tests/anonymous_unions.rs` の 4 件（上の 1〜4 に 1 件ずつ）と E2E の `widen_captured` の 8 件です。

#### 再検証（第2回）

- T0: `cargo fmt --all -- --check`（差分なし）、`node scripts/slot.mjs -- cargo clippy --all-targets -- -D warnings`（警告なし）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`: 3099 件のうち 3099 passed、4 skipped（`target/scratch/a25/r2/t1-final.txt`）。`tests/anonymous_unions.rs` は 106 件（第 1 回の 102 件とこの対応の 4 件）。1 の修正だけの木でも 3096 件のうち 3096 passed、4 skipped でした（`t1-f1.txt`）。
- T2: `node scripts/slot.mjs -- node tests/features.mjs target/debug/tsuzuri anonymous_unions`: `anonymous_unions: native/WASM at O0/O3 passed`、84 cases（`widen_captured` の 8 件を足した）。`node tests/last_use_fuzz.mjs target/debug/tsuzuri 40 1 4`: `40 programs, 80 native O0/O3 executions, seed 1, depth 4; zero differences`。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`anonymous-union.md`・`compiler/diagnostics.md`）: `2 pages, 62 links, 10 checked examples, 18 native runs (O0/O3)`。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks、0 stale。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`: 4 checks、0 failed。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。`node scripts/check-links.mjs --into _features/README.md`: 0 broken。
- 匿名共用体を使わないプログラムの出力: A24 の先端 `09fdb0de` のバイナリと、この対応の最後のバイナリで、第 1 回と同じ 186 単位の出力を `baseline.sh` で取り、`compare.sh` で比べました。違うのは `tests/fixtures/anonymous_unions` だけで（67 ファイル。この fixture は拡大と絞り込みを使います）、ほかの 185 単位はすべてのファイルが 1 byte も違いません（`target/scratch/a25/r2/compare-r2.txt`）。1 の生成の変更は借用ワーカーが捕捉から束縛を move していたプログラム（二重解放していたもの）にだけ、2 は所有権の検査の手間にだけ、3 と 4 は失敗するプログラムの文にだけ働きます。

更新した文書（第 2 回）: `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（拡大の節の 2 回目の使用の診断と部分適用、絞り込みの節の入れ子の検査、注意点）、`_tsuzuri/language-reference/compiler/diagnostics.md`（`E1012`・`E1013` の行）、`docs/language.md`（匿名共用体の節の拡大）、`docs/architecture.md`（`LastUses` とワーカーの束縛、`eval_match` の代替）。

変更したファイル（第 2 回）: `src/llvm_control.rs`、`src/ownership.rs`、`src/ownership_control.rs`、`tests/anonymous_unions.rs`、`tests/features.mjs`、`tests/fixtures/anonymous_unions/Main.tz`、上の文書、このチケット。
