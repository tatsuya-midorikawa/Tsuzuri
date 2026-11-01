# A25: 匿名共用体どうしの変換（部分集合からの拡大と `:? (A | B)` の絞り込み）

| 項目 | 内容 |
| --- | --- |
| ID | A25 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | A24 |
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
