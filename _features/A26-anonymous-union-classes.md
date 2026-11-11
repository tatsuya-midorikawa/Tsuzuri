# A26: 匿名共用体の型クラス（`Eq`・`Ord`・`Hash`・`Display`）と JSON（`Encode`・`Decode`）

| 項目 | 内容 |
| --- | --- |
| ID | A26 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | [A24](_completed/A24-anonymous-unions.md)（done）, A06, A07 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-02（A24 と同時。HEAD `2f556480`） |
| 承認 | `Ord` を正準順で持たせること（D2）と、`Decode` を JSON の種類が互いに素な場合に限ること（D5）は、2026-11-02 の利用者の回答「1~4 について承認します」で承認済み（GUIDE D-62 の決定 4・5）。ほかは既定案。Phase 2 は人間が求めた場合だけ着手する |
| 手本にする既存実装 | タプル・配列・リストの構造的なインスタンス: `Classes::structural`（`src/polymorph.rs:2556`）、制約の展開 `normalize`（`2382-2401`）、型付き IR への下ろし（`5230`、`5311-5399`）、LLVM の `structural_display`（`src/llvm_display.rs:548`）・`display_quoted`（`601`）・`structural_hash`（`src/llvm_hash.rs:17`）・`structural_compare`（`src/llvm_compare.rs:69`）。union の導出の意味: `src/derive.rs`（`Build::union`、`817`） |
| 主な影響ファイル | `src/polymorph.rs`, `src/llvm_display.rs`, `src/llvm_hash.rs`, `src/llvm_compare.rs`, `src/anonymous_union.rs`, `src/diagnostic_kinds.rs`, `tests/anonymous_unions.rs`, `tests/fixtures/anonymous_unions/Main.tz`, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`, `_tsuzuri/language-reference/types-and-type-inference/type-classes.md` |

## 実装者向け早見

- 範囲: 匿名共用体の組み込みの構造的なインスタンス `Eq`・`Ord`・`Hash`・`Display`（Phase 1）。`Encode`・`Decode` は Phase 2（人間が求めた場合だけ）。
- 最初に読む（この順）: ① 決定事項 D1〜D5 ② `src/polymorph.rs` の `structural`（2556 行）・`normalize`（2382〜2401 行）・下ろし（5230 行、5311〜5399 行）③ `src/llvm_display.rs` の `structural_display`（548 行）・`display_quoted`（601 行）④ `src/llvm_hash.rs:17`・`src/llvm_compare.rs:69` ⑤ `src/derive.rs` の `Build::union`（817 行。名前付きの union の `Hash` のストリーム）。
- 最初に書くテスト: `tests/anonymous_unions.rs` の `anonymous_unions_have_structural_classes` → `eq_compares_tag_then_payload` → `hash_matches_named_union_stream`。
- 触ってはいけない: タプル・配列・リスト・名前付きの union の比較・表示・ハッシュの生成 IR（byte 単位）。A24 の利用者インスタンスの拒否（D8）。
- 外から借りるもの: なし。
- 完了の見分け方: 目的の例が通り、`Display.display (a, b)` が `(1, "one")`、`a < b` が `true`。
- 迷ったら: 決定事項 → タプルの既存の挙動に合わせる → 名前付きの union の導出に合わせる → 最も保守的な選択（`E1005` のまま）。

## 目的

匿名共用体を比較・順序付け・ハッシュ・表示できるようにする。タプルと同じく、要素のインスタンスから構造的に決まる組み込みのインスタンスにする（宣言を書く場所がないので `deriving` は使えない）。

```text
let a: (i64 | string) = 1
let b: (i64 | string) = "one"
a == b                           // false（別の要素）
a < b                            // true（要素の順: i64 は string より前）
Display.display b                // one
Display.display (a, b)           // (1, "one")
let seen: HashSet<(i64 | string)> = HashSet.empty |> HashSet.insert a |> HashSet.insert b
```

これで `HashMap`・`HashSet`（`Eq`・`Hash`）、`Map`・`Set`（`Ord`）のキー、文字列補間（`Display`）、`deriving (Eq, Ord, Display, Hash)` のある型のフィールドに匿名共用体を使える。

- Phase 1: `Eq`・`Ord`・`Hash`・`Display`。
- Phase 2（人間が求めた場合だけ）: `Encode`・`Decode`（JSON）。

## 着手条件と停止条件

### 着手条件

- A24 が done であること（A25 は不要）。A06（条件付きインスタンス）と A07（導出）が done であること（HEAD で done）。
- A24 の suite `anonymous_unions` と `tests/anonymous_unions.rs` が HEAD で通ること。

### 停止条件

- `Classes::structural` に型の表（`TypeContext`）を渡さないと匿名共用体を判定できず、呼び出し元（6 か所）の広い変更が要る。その場合は `Classes` に匿名共用体の宣言の id の集合を持たせる案を報告する。
- `structural_display`・`structural_hash`・`structural_compare` が要素の数を型から静的に読む前提で、タグで分岐する形を足せない。
- タプル・配列・リストの比較・表示・ハッシュの生成 IR が 1 byte でも変わる。

## 現状（A24 の後、HEAD `2f556480` のコード）

- `Classes::structural(class, ty)`（`src/polymorph.rs:2556-2565`）: `Eq`・`Ord`・`Hash`・`Display` は `Array`・`List`・`Tuple`、`Default` は `Tuple` だけ。
- `normalize`（`2382-2401`）は、構造的な制約を要素ごとの同じクラスの制約に展開する。
- 下ろし（`5230` から）: `Display` は `TypedExprKind::StructuralDisplay([値, 要素ごとの DisplayQuoted の関数])`、`Hash` は `StructuralHash([値, 要素ごとの Hash.hash])`、比較演算子は `StructuralCompare(演算子, [左, 右, 要素ごとの ==（と Ord なら演算子）])`。LLVM は `src/llvm.rs:5417-5421` からそれぞれの関数へ。
- 入れ物の中の要素の表示は `DisplayQuoted`（文字列・文字は引用符付き。`src/llvm_display.rs:601`）。
- 利用者のインスタンス: A24 が匿名共用体の head を `E1041`（kind `anonymous_union_instance`）で拒否する。組み込みの構造的なインスタンスの上書きは `E1016`（kind `builtin_instance_override`、`src/polymorph.rs:1861-1871`）。
- 名前付きの union の導出（`src/derive.rs`）: `Eq` はタグ、次にペイロード。`Ord` は case の宣言順、次にペイロード。`Hash` は FNV-1a でタグ `0x55`、case の番号、ペイロードのハッシュ。`Display` は `Case payload`。

## 仕様

### Phase 1: `Eq`・`Ord`・`Hash`・`Display`

すべて「要素ごとの同じクラスの制約」を要る（タプルと同じ）。足りなければ既存の `E1005`（メッセージの型は要素の型。たとえば `(i64 | Point)` の `==` で `Point` に `Eq` がなければ `Eq<Point>`）。

| クラス | 意味 |
| --- | --- |
| `Eq` | 2 つの値が同じ要素で、ペイロードが等しい（要素の `==`）とき `true`。別の要素なら `false`（ペイロードは比べない）。NaN・符号付きゼロは要素の `Eq` のまま（`(f64 \| string)` の NaN どうしは等しくない） |
| `Ord` | 要素の正準順（A24 の「正準順序」。表示の順）で先に比べ、同じ要素ならペイロードを要素の `Ord` で比べる。`<`・`<=`・`>`・`>=`（`Ord.lt` など）が同じ順を使う。要素の比較が順序を決めない（NaN）なら、その演算子の結果は要素の演算子の結果 |
| `Hash` | 名前付きの union の導出と同じストリーム: タグ `0x55`、要素の番号（正準順の 0 始まり）、ペイロードのハッシュ。値が等しい（`Eq`）なら等しい |
| `Display` | ペイロードの表示だけ（タグを書かない）。単独の値は要素の `Display`（`string` は引用符なし）、入れ物・タプル・レコードの中では要素の `DisplayQuoted`（`"one"`）。`Display.display (a, b)` は `(1, "one")` |

- `Default`・`Format`・`Arbitrary` は持たない（`E1005`）。どの要素の既定値かを決められないため。
- 利用者のインスタンスの拒否（A24 の D8）は変えない。`Eq` などを独自に定義したい場合は、レコードで包む。
- `dyn Display` などへの格納: 組み込みの実装なので、既存の `E1028`（`cannot store {T} in dyn {C}: ... is built into the compiler`）。タプルと同じ。
- 比較演算子の右辺の注入（A24）: `v == 1`（`v: (i64 | string)`）は、`1` を `i64` の要素として包んでから比べる。`v` が `i64` の `1` なら `true`。言語リファレンスに書く。
- `deriving (Eq, Ord, Display, Hash)` のレコード・union が匿名共用体のフィールド・ペイロードを持てる（`src/derive.rs:461`・`466` は型を `polymorph::type_expression` で構文に戻して制約を作る。A24 がこの関数で匿名共用体を `(A | B)` の構文に戻すようにしたので、制約 `Eq<(i64 | string)>` は解決し直しても同じ型になり、そのまま通る）。

### Phase 2: `Encode`・`Decode`（人間が求めた場合だけ）

- `Encode`: ペイロードの JSON をそのまま書く（タグを書かない。F# の RFC の消去と同じ形で、TypeScript の union と同じ見え方）。要素ごとの `Encode` を要る。
- `Decode`: 要素の JSON の種類が互いに素なときだけ持つ。種類は `number`（整数・浮動小数点・`decimal`・`bigint`）、`string`（`string`・`utf8string`・`char`・`utf8char`）、`bool`、`null`（`unit`）、`array`（配列・リスト・`Vec`・タプル・固定長配列）、`object`（レコード・`Map`・`HashMap`）。名前付きの union と `Maybe` は導出の形しだいで複数の種類になりうるので、`Decode` を持たない要素として扱う。重なれば `E1025`（kind `anonymous_union_decode_overlap`。`(i64 | f64)` は両方 `number`）。
- 読むときは、JSON の値の種類で要素を 1 つ選び、その要素の `Decode` を呼ぶ。どれにも当たらなければ既存の `Json.DecodeError` の型の不一致。
- Phase 2 の詳細（`Encode`・`Decode` が std の型クラスで、組み込みの構造的なインスタンスの経路を通らない場合の作り方）は、着手時に `std/Json.tt`・`src/derive.rs` の `Encode`・`Decode` を読み、決定事項に追記してから実装する。

## 設計

### 判定

- `Classes::structural(class, ty)` が匿名共用体を判定するために、`Classes` に `anonymous: BTreeSet<usize>`（合成した宣言の id。A24 の `Names::anonymous_unions` から作る）を持たせる。`structural` は `Type::Union(id, _)` で `id` がこの集合にあれば `Eq`・`Ord`・`Hash`・`Display` について `true`。`Default` は `false` のまま。
- `normalize` の要素: `Type::Union(_, args)` なら `args`（要素）。

### 下ろし（`src/polymorph.rs:5311-5399`）

各分岐の `elements` を、匿名共用体なら `args` にする。値の並び（`values`）の形はタプルと同じ（`StructuralDisplay([値, 要素ごとの関数])` など）。
LLVM の側は、引数の型（`arguments[0].ty`。参照なら中身）が匿名共用体かどうかで分岐する。

| 関数 | 匿名共用体のときの生成 |
| --- | --- |
| `structural_display`（`src/llvm_display.rs:548`） | タグで `switch`。要素 `k` の腕で、ペイロードの参照を関数 `k`（単独の値なら要素の `Display.display`、入れ物の中なら `DisplayQuoted`）に渡し、その結果を返す。タプルのような `(`・`,`・`)` は書かない |
| `display_quoted`（`601`） | 匿名共用体の型なら、上と同じ `switch` で要素の `DisplayQuoted` を呼ぶ関数を作る |
| `structural_hash`（`src/llvm_hash.rs:17`） | 名前付きの union と同じストリーム（`0x55`、要素の番号、ペイロードのハッシュ）。名前付きの union の導出が作る列と同じ値になること（同じ要素・同じペイロードの名前付きの union と比べるテストを置く） |
| `structural_compare`（`src/llvm_compare.rs:69`） | まず両方のタグを読む。`==`: タグが違えば `false`、同じなら要素 `k` の `==`。`Ord` の演算子: タグが違えばタグの大小（`i32` の比較）で、同じなら要素 `k` の演算子 |

Display の「単独の値」と「入れ物の中」の区別は、タプルの要素が `DisplayQuoted` を使い、トップレベルの `Display.display` が `Display` を使う既存の区別のまま。下ろしで `Builtin::Display` の分岐（`5311`）の要素の関数を、匿名共用体なら `Display.display`（要素の `Method(class, 0, element)`）にする。

## 実装手順

1. ベースライン: `node tests/features.mjs target/debug/tsuzuri deriving`、`node tests/features.mjs target/debug/tsuzuri anonymous_unions`、`node scripts/slot.mjs -- cargo nextest run --locked --test deriving --test anonymous_unions`。タプル・配列の比較・表示・ハッシュを使う fixture の IR を保存する。
2. `Classes::anonymous` と `structural`・`normalize`。テスト: `anonymous_unions_have_structural_classes`、`missing_member_instance_names_the_member`、`no_default_for_anonymous_unions`。
3. 下ろしと LLVM。テスト（Rust、IR の決定性と結果）: `eq_compares_tag_then_payload`、`ord_follows_member_order`、`hash_matches_named_union_stream`、`display_shows_the_payload`、`display_quotes_inside_tuples`、`nan_members_follow_member_eq`。
4. `deriving` と標準のコレクション。テスト: `derived_record_with_anonymous_field`、`hashset_and_map_keys`、`comparison_injects_right_operand`（`v == 1`）、`dyn_rejects_builtin_instances`（`E1028`）。A24 の例の表の `v == 1` の行（`E1005`）を受理に替える。
5. suite `anonymous_unions` に 4 関数: `equal x`（同じ要素・別の要素・同じ値・違う値の 4 通りを 0/1 の和で返す）、`order x`、`hashed x`（`HashSet` に入れて数える）、`shown x`（表示の長さ）。native と WASM、`-O0` と `-O3`。
6. 手順 1 のタプル・配列の IR が 1 byte も変わらないこと。
7. ドキュメントと最終確認（A24 の手順 13・14 と同じ）。

## テスト計画

- Rust: `tests/anonymous_unions.rs` に手順 2〜4 の名前のテスト。
- E2E: 手順 5。
- 参照の値: `hash_matches_named_union_stream` は、同じ要素の順に case を並べた名前付きの union（`union R = A of i64 | B of string deriving (Hash)`）の `Hash.hash` と、匿名共用体の `Hash.hash` が一致することで確かめる（独立した参照）。
- 既存テストへの影響: A24 の `v == 1` の `E1005` のテストを受理に替えるだけ。

## ドキュメント

- `anonymous-union.md`: 「比較・順序・ハッシュ・表示」の節（表と `run=` の例）。A24 の「現在は `match` で取り出す」の注意を消す。
- `types-and-type-inference/type-classes.md`: 構造的なインスタンスを持つ型の一覧に匿名共用体。
- `built-in-types-and-modules/hashmap.md`・`map.md`: キーに匿名共用体を使える旨（必要なら 1 行）。
- `docs/language.md`: 「### 匿名共用体」に型クラスの段落。`docs/architecture.md`: 構造的なインスタンスの節。

## 受け入れ条件

- 手順 2〜4 のテストと suite が native と WASM、`-O0` と `-O3` で通る。
- タプル・配列・リスト・名前付きの union の生成 IR が変わらない。
- `hash_matches_named_union_stream` が通る。

## 落とし穴

- `Ord` の順は宣言順ではなく正準順（表示の順）。`(string | i64)` と書いても `i64` が先。言語リファレンスの例で必ず示す。
- `structural` を `true` にすると、`Classes::instances` の組み込みの上書きの拒否（`1861-1871`）が匿名共用体の head にも働くが、A24 の `anonymous_union_instance` が先に拒否する。順序を入れ替えない（メッセージが変わる）。
- 単独の `Display` で `DisplayQuoted` を使うと、`Display.display b` が `"one"` になる。下ろしの要素の関数を、`Display` の分岐と `DisplayQuoted` の型で使い分ける。

## 対象外

- `Default`・`Format`・`Arbitrary`。
- 利用者の独自インスタンス（A24 の D8 のまま）。
- Phase 2 を人間が求めない限り、`Encode`・`Decode`。

## 着手時の再検討（2026-11-11、A24 の先端）

A24 がレビュー第 1 回の修正まで入ったブランチ `feat/a24-anonymous-unions`（`bd625b4d`）を取り込んだ状態で、A24 の実際のコード（`src/anonymous_union.rs`、`src/check.rs` の注入の入口、`src/control.rs` の `union_case_pattern`・`type_test_pattern`、`src/polymorph.rs`）と、監査（HEAD `b3dd7f1c`。`target/scratch/audit/A26.json`・`A26-revision.md`）、調整役の決定（`target/scratch/audit/DECISIONS.md` と 2026-11-11 の追記）を照合し、計画を改めました。
この節は、この節より前の本文と食い違うところでは本文より優先します。本文の古い記述は、記録のため残します。
停止条件（「着手条件と停止条件」と GUIDE §13）は、このチケットでは次の規則に置き換えます（利用者の指示による）: 健全性を保ち、今受理されている安全なプログラムをすべて受理し続ける、最も保守的な選択を取り、その判断をこのチケットに記録して作業を続けます。テストの期待値を弱めて通すことと、安全でないプログラムを受理することはしません。
範囲は Phase 1 と Phase 2 の両方です（利用者が全 Phase の実装を求めたため。決定の 2026-11-11 の追記）。

### A24 が実際に作ったもの（このチケットが使う名前）

- 匿名共用体は `Type::Union(id, 要素)` で、`id` は合成した宣言 `$anonymous.Union$n`（n = 2〜32）です。要素は正準順に並び、ケース `k` のペイロードは要素 `k` です（A24 の D2・D3）。
- 判定: `Names::is_anonymous_union(id)`（`src/check.rs`。`Names::anonymous_unions: Vec<usize>` は連続した id）と、`TypeContext` から読む `CheckedUnion::is_anonymous()`。本文の `anonymous_union::is_anonymous` という関数はありません。
- 利用者のインスタンスの拒否（`anonymous_union_instance`、`E1041`）は、`Classes::instances` の組み込みの上書きの拒否（`builtin_instance_override`）より前にあります。このチケットは順序を変えません。
- `E1005`・`E1025` の文（`polymorph::no_instance`）: 匿名共用体なら `no instance for C<(i64 | string)>; instances cannot be declared for an anonymous union; match on its members, wrap it in a record, or declare a union with named cases`（A24 のレビュー第 1 回）。このチケットの後は、匿名共用体が持たないクラス（`Default` など）でだけ出ます。
- 型付き IR: case のパターンと型テストは、`UnionTag(対象) == Int(k)`（タグは `i32`）と `UnionPayload { value: 対象, case_id: k }` を作ります（`Checker::union_case_pattern`、`src/control.rs`）。参照の対象は `Checker::autoderef` が `Dereference` で外します。
- 比較演算子は、左辺の型を右辺の期待型に渡すので（`binary_expression` の `sibling_hint`）、`v == 1` の `1` は A24 の時点で注入されます。左辺が接尾辞のない数のリテラルなら右辺を先に検査するので、`1 == v` も注入されます。

### 位置（このブランチの先端）

- `src/polymorph.rs`: `Classes`（722）、`Classes::collect`（871。`names` と `types` を受け取る）、`anonymous_union_instance` の拒否（1933〜1946）と組み込みの上書きの拒否（1957〜1968）、`matching_instance`（2286）、`normalize`（2400。構造的な展開は 2481〜2501）、`structural`（2655）、`Specializer`（4902）、`Specializer::lower` の `Method`（5145〜5190）と `DisplayQuoted` の書き換え（5191〜5208）、`dyn_of` の vtable（5322〜5353）、`operator_call`（5358）、`intrinsic_function`（5408。`Display` 5452、`Hash` 5474、`Default` 5493、比較 5515）。
- `src/derive.rs`: `mix`・`hash_start`（286〜297）、`Build::union` の `Display`・`Hash`・`Eq`／`Ord`（906〜1057）。
- `src/llvm_display.rs`・`src/llvm_hash.rs`・`src/llvm_compare.rs` は、このチケットでは変えません（下の D6）。

### 現状と食い違う記述

1. **引用付きの表示**: 文字列・文字以外の `T` では、`DisplayQuoted<T>` を `Specializer::lower` が `T` の `Display.display` に置き換えます。`display_quoted`（`src/llvm_display.rs`）は文字列・文字専用で、匿名共用体には届きません。本文の設計のままでは `Display.display (a, b)` が `(1, one)` になります。
2. **`dyn` への格納**: タプル・配列は `dyn Display` に格納でき、`(1, "one")`・`[1, 2]` と表示されます。`built into the compiler` という `E1028` はありません。本文の仕様の `E1028` の行と、テスト `dyn_rejects_builtin_instances` は取り消します。
3. **ファイル名と仕組み（Phase 2）**: `std/Json.tt` は `std/Json.tz` です。`Encode`・`Decode` は組み込みクラスですが、メソッドに `operation` がなく、インスタンスは std のソースと導出だけにあります。
4. **誤りの型（Phase 2）**: `Json.DecodeError` はありません。誤りは `Json.Error { kind, offset }` で、型の不一致は `ExpectedType of string` です。
5. **JSON の種類の表（Phase 2）**: `char`・`utf8char`・`unit`・`bigint`・固定長配列・5 要素以上のタプルには `Encode`・`Decode` がありません。`Map`・`HashMap` は object と配列の両方になりえます。`Set`・`HashSet` は配列、`Json.Value` はすべての種類を受理します。
6. **目的の例**: `HashSet.empty` は `HashSet.empty()` です。`(i64 | string)` は Copy でないので、`Display.display (a, b)` の後で `a`・`b` を使うと `E1012` になります。リファレンスの例は順序を入れ替えるか、借用を使います。
7. **停止条件 1**: `Classes::collect` が `names` を受け取るので、`Classes` に匿名共用体の id の集合を持たせるだけで済み、`structural` の呼び出し元は変わりません。停止条件には当たりません。
8. **`!=`**: 本文は `!=` を書いていません。タプル・導出と同じく、`==` の否定です（タグが違えば `true`、同じなら要素の `==` の否定）。

### 採用する計画の変更

1. **D6（新規）: インスタンスの本体は、Specializer が通常の型付き IR として作ります。`src/llvm_display.rs`・`src/llvm_hash.rs`・`src/llvm_compare.rs`・`src/llvm.rs` は変えません。**
   - 形: 引数（`ref U`）を `Dereference` で外し、`UnionTag(..) == Int(k)` の `If` の連鎖にします（最後の要素は検査なしの `else`）。腕 `k` は、`Borrow(UnionPayload { value: Dereference(引数), case_id: k })`（型は `ref 要素 k`）を要素のメソッドに渡します。`union_case_pattern` が `ref` の対象に作る形と同じです。
   - `Eq.eq`: 両方のタグが違えば `false`、同じなら要素 `k` の `Eq.eq`。`Eq.ne` は、タグが違えば `true`、同じなら要素 `k` の `Eq.eq` の否定です（導出・タプルと同じ）。
   - `Ord` の 4 つの演算子: タグが違えば `Binary(演算子, 左のタグ, 右のタグ)`（`i32` の比較）、同じなら要素 `k` の同じ演算子（`Ord.lt` など）。導出した union と同じ形です。
   - `Hash.hash`: `$builtin.hash_mix` で `OFFSET`（14695981039346656037）→ `0x55` → `k` → 要素 `k` の `Hash.hash` の順に混ぜます。導出した union（`Build::union` の `hash_start(0x55, k)` と `mix`）と同じ列です。
   - `Display.display`: 要素 `k` の `Display.display`（引用符なし）。引用付きの版（入れ物・タプル・導出したレコードや union の中）は別のテンプレートで、要素 `k` の `DisplayQuoted` を呼びます。`Specializer::lower` の `DisplayQuoted` の書き換えから、専用のキャッシュ（`quoted_displays: BTreeMap<Type, usize>`）に作ります。`intrinsics` の鍵 `(Display, 0, U)` は引用符なしの版のものなので、共有しません。
   - 理由: Phase 2（`Encode`・`Decode`）は型付き IR でしか作れないので、仕組みを 1 つにします。LLVM の側を変えないので、タプル・配列・リスト・名前付きの union の生成 IR は構成上変わりません。再帰・`Common`・`General` のレイアウトは、既存の `UnionTag`・`UnionPayload` の生成が扱います。
   - 代案（所有権の検査 `ownership::check_all_with` が合成した本体を受理しない場合だけ）: 本文の LLVM の分岐（`llvm_layout.rs` の `load_tag`・`payload_pointer`・`case_switch`）で Phase 1 を作ります。最初にこれを確かめ、結果を「実装中の判断」に書きます。
2. **`Classes::anonymous`**: `Classes::collect` で `Names::anonymous_unions` から `BTreeSet<usize>` を作ります。`structural(class, ty)` は、`Type::Union(id, _)` で `id` がこの集合にあれば `Eq`・`Ord`・`Hash`・`Display`（Phase 2 で `Encode`・`Decode`）について真です。`Default` は偽のままです。
3. **要素を返す関数を 1 つにします**: `Array`・`List`・`Tuple`・匿名共用体の要素を返す `structural_elements(ty)` を作り、`normalize` と `intrinsic_function` の 3 か所の `match ... _ => unreachable!()` を置き換えます（タプル・配列・リストの結果は同じ）。
4. **`dyn`**: 匿名共用体は、タプルと同じく `dyn Display` などに格納できます。表示はペイロードだけで、引用符は付きません（単独の値と同じ）。テストは `dyn_stores_anonymous_union` にします。
5. **Phase 2 の制約**: `structural` は匿名共用体の `Encode`・`Decode` でも真にし、`normalize` が要素ごとの制約に展開します。`Decode` だけ、要素の JSON の種類が互いに素かを `normalize` で調べ、重なれば `E1025`（新しい kind `anonymous_union_decode_overlap`）です。
   - 種類は頭の型構築子で決めます（型変数を含む要素でも決まる）。number＝整数・浮動小数点・`decimal`、string＝`string`・`utf8string`、bool、array＝配列・リスト・`Vec`・2〜4 要素のタプル・`Set`・`HashSet`、object と array＝`Map`・`HashMap`、object＝`Decode` を導出したレコード。
   - 種類が決まらない要素（どの要素とも重なる）: `Maybe`・名前付きの union・`Json.Value`・手書きの `Decode` を持つ型。同じ kind で、その要素の JSON の種類が決まらないことをメッセージで示します。`Encode` はどの要素でも持てます。
6. **Phase 2 の本体**: `Encode`・`Decode` のメソッドには `operation` がないので、`Specializer::lower` の `Method` で、匿名共用体の `Encode`・`Decode` を専用の合成へ回します（`intrinsic_function` の `operation.expect` より前）。`dyn_of` の vtable の条件も直し、`dyn Encode` に格納できるようにします。
   - `Encode.encode`: 要素 `k` の `Encode.encode` を呼ぶ `If` の連鎖（タグを書かない）。
   - `Decode.decode`: `Json.Value` のタグで要素を選び、腕 `k` は `Result.map (CaseConstructor U k) (Decode.decode json)` です。どれにも当たらなければ `Result.Error (Json.Error { kind: ExpectedType "number or string", offset: -1 })`（要素の種類を ` or ` でつなぐ。導出した union の `string or object` と同じ書き方）。std に関数は足しません。
   - 詳細は Phase 2 の着手時に `std/Json.tz` と `src/derive.rs` を読み直し、決定事項に追記してから実装します。
7. **診断とドキュメント（Phase 2 を含む）**: 新しい kind は `E1025` の組に足し、`src/explain/codes/E1025.md` を広げ、`gen-docs` を実行します。更新するページに `json.md`（型の表と「インスタンスは Json と利用者のソースだけ」の文）、`docs/language.md` の組み込みクラスの表・Canonical Stream の union の行（匿名共用体は union の列で、番号は正準順の位置）・Json の項、`type-classes.md` の構造的な型の一覧、`docs/architecture.md` を足します。GUIDE D-55 の `E1041` の行に、`Decode` の重なりだけは D5 のとおり `E1025` であることを注記します。
8. **テスト**: 本文の手順 2〜4 のテストに、次を足し・替えます。
   - `hash_matches_named_union_stream` は、同じ順に case を並べた名前付きの union との一致に加えて、監査が `b3dd7f1c` の導出で得た参照値と照合します: `union R = A of i64 | B of string deriving (Hash)` の `Hash.hash (A 1)` = `15967365493956732001`、`Hash.hash (B "one")` = `17017872639125386659`（`-O0` と `-O3` で同じ）。実装の前に、FNV-1a の定義からの独立した計算（Python）でもこの値を確かめます。
   - `comparison_injects_right_operand` に `1 == v` を足します。
   - 再帰する型（`union Tree = Leaf | Node of (i64 | Tree)` に `deriving (Eq, Ord, Hash, Display)`）のテストを足します。
   - `dyn_rejects_builtin_instances` は `dyn_stores_anonymous_union` に替えます（上の 4）。
   - Phase 2: `encode_writes_payload_without_tag`、`decode_selects_member_by_kind`、`decode_overlap_is_rejected`（`(i64 | f64)`・`(Map<i64, string> | [i64])`・`(Maybe<i64> | string)`）、`decode_mismatch_names_member_kinds`、`derived_record_with_anonymous_field_round_trips`。suite `anonymous_unions` に `encoded x`・`decoded x` を足します。
9. **A24 の既存テストで期待を替えるもの**（このチケットの機能が、A24 の時点の「インスタンスがない」という前提を変えるため。替える前にここへ記録します）:
   - `comparisons_and_exports_keep_their_rules` の `v == 1`（`E1005`）は受理に替えます（本文の「既存テストへの影響」のとおり）。`E1008`（export）の部分は変えません。
   - `missing_instances_do_not_ask_for_an_instance`（A24 のレビュー第 1 回で足したテスト。本文を書いた後にできた）は、`Eq`・`Display` の `E1005` と `deriving (Eq)` の `E1025` を、匿名共用体に `no instance` と言う文が「インスタンスを定義する」ことを勧めないことの確認に使っています。このチケットで `Eq`・`Display` が入るので、同じ文を、匿名共用体が持たないままのクラス `Default`（`E1005`、`deriving (Default)` の `E1025`）で確かめる形に替えます。確かめる文（`...; instances cannot be declared for an anonymous union; ...`）と、名前付きの型の文の確認は変えません。
   - 言語リファレンスの「比較・表示・ハッシュはまだありません」の記述（`anonymous-union.md`）は、ドキュメントの手順で実際の振る舞いに替えます。
10. **手順の分け方**: 手順 2（判定と制約）と手順 3（本体）は、判定だけを先に入れると本体のない経路（`intrinsic_function` の `unreachable!()`）に届くので、同じ段で入れます。手順 6 の IR の比較の対象に、名前付きの union の導出（`deriving`）の fixture も入れます。

### 実装中の判断（手順 2〜4）

1. **所有権の検査（D6 の前提）**: `ownership::check_all_with` は `$intrinsic` の関数もほかの関数と同じに検査します（モジュール名での除外はありません）。合成した本体（`Borrow(UnionPayload { value: Dereference(引数), case_id })` を要素のメソッドへ渡す形）は、非 Copy の要素（`string`・`[i64]`・`[|string|]`）、再帰する型（`union Tree = Leaf | Node of (i64 | Tree)`）、全要素が同じ表現の型（`(i64 | i64u)`）、32 要素の型で、そのまま受理されました。本文の LLVM の分岐（代案）は使っていません。`src/llvm*.rs` は 1 行も変えていません。
2. **手順 2 と手順 3 は 1 つのコミット**にしました（再検討の 10）。`Classes::anonymous` は `Names::anonymous_unions` から作る `BTreeSet<usize>` です。`structural_elements` は `src/polymorph.rs` の自由関数で、`normalize` と `intrinsic_function` の 3 か所が使います。
3. **本体を作る位置**: 型付き IR の部品（`tag_behind`・`member_call`・`by_member`）は `src/anonymous_union.rs`、メソッドごとの組み立ては `Specializer::union_method`（`Eq`・`Ord`・`Hash`・`Display`）と `Specializer::quoted_display_function`（引用付きの表示。キャッシュは `quoted_displays`、関数名は `$intrinsic.Display.display_quoted.N`）です。演算子の形（`a == b`）もメソッドの形（`Eq.eq a b`）も、同じ `intrinsic_function` の同じ関数になります。
4. **`If` の連鎖**: 要素 0 から順に `tag == k` を調べ、最後の要素は検査なしの `else` です（タグは必ず要素の位置なので）。`-O0` の IR は、タグの読み取り・比較・ペイロードへの `getelementptr`・要素の関数の直接呼び出しだけで、値の複製はありません（`target/scratch/a26/try/ir1`）。
5. **ハッシュの定数**: `HASH_OFFSET`（14695981039346656037）と `UNION_HASH_TAG`（`0x55`）を `src/polymorph.rs` に置き、導出と同じく `$builtin.hash_mix` の呼び出しを 3 回並べます（コンパイル時に畳みません。導出した union と同じ式にするため）。参照値 `15967365493956732001`・`17017872639125386659` は、`docs/language.md` の Canonical Stream の定義から Python で計算し直した値とも一致しました（FNV-1a: 状態 `xor` バイト、`* 1099511628211`、語は下位バイトから 8 バイト。`i64` は `0x13`、文字列は `0x53`・長さ・UTF-16 の単位ごとに 2 バイト）。
6. **純粋性**: 本体が通常の関数なので、`pure def` の中で匿名共用体を比べると、要素のインスタンスの効果まで辿られます（要素の `Eq` が `Debug.print` を呼ぶと `E1029`。経路に `$intrinsic.Eq.eq.N` が現れます）。タプルの比較（`StructuralCompare`）は要素のインスタンスを辿らず、同じプログラムを受理します（このチケットより前からの挙動で、このチケットでは変えていません）。
7. **`const def`**: 匿名共用体の比較は、タプル・名前付きの union と同じ `E1026`（`const_aggregate_operator`、`an operator on a record, tuple, array, or union is not allowed in a const def; ...`）です。テスト `const_definitions_refuse_the_operators_as_for_tuples` で固定しました。
8. **テストを書くうえで分かった、A24 までの規則**（このチケットでは変えていません）:
   - 型変数そのものは要素にできない（`('a | string)` は `E1041`）ので、総称のレコードのテストは `(['a] | string)` を使います。
   - `Display<ref T>` はないので、参照のタプル `(ref a, ref b)` は表示できません（`Display.display (a, b)` は値を move します）。
   - 型引数の推論は要素から共用体へ広げない（A24）ので、`Set.empty() |> Set.insert "one"` は `'key = string` になります。キーは、型の付いた値（`def key :: own (i64 | string) -> (i64 | string)`）にしてから渡します。言語リファレンスの例でも同じにします。
   - `Array.sort` は `Copy` を要るので、並べ替えのテストは `(char | i64)`・`(f64 | i64)` を使います。
   - 導出した `Display` は、ペイロードを括弧で囲みません（`Node (Node 1)` は `Node Node 1`。名前付きの union だけでも同じ）。
   - `bool` には `Ord` がないので、`(i64 | bool)` は `Eq` だけを持ちます（`<` は `no instance for Ord<bool>`）。
9. **足したテスト**（`tests/anonymous_unions.rs`）: `anonymous_unions_have_structural_classes`・`missing_member_instance_names_the_member`・`no_default_for_anonymous_unions`（手順 2）、`eq_compares_tag_then_payload`・`ord_follows_member_order`・`hash_matches_named_union_stream`・`display_shows_the_payload`・`display_quotes_inside_tuples`・`nan_members_follow_member_eq`・`recursive_types_compare_through_named_unions`・`wide_unions_have_instances`（手順 3）、`derived_record_with_anonymous_field`・`hashset_and_map_keys`・`comparison_injects_right_operand`（`v == 1` と `1 == v`）・`dyn_stores_anonymous_union`・`const_definitions_refuse_the_operators_as_for_tuples`（手順 4）。実行するテストは native と WASM、`-O0` と `-O3` で通します（`passes`）。期待値は規則から手で求めた値です。
   - A24 のテストの変更は再検討の 9 のとおりです: `comparisons_and_exports_keep_their_rules` の `v == 1` を受理に、`missing_instances_do_not_ask_for_an_instance` の 3 例を `Add`・`Default`・`deriving (Default)` に替えました。

## 決定事項

### D1: 組み込みの構造的なインスタンス

- 決定: タプルと同じく、要素のインスタンスから構造的に決まる組み込みのインスタンスにする。
- 理由: 匿名共用体には宣言を書く場所がなく、`deriving` を付けられない。タプルの経路（`Classes::structural`）が同じ形の問題をすでに解いている。
- 状態: 既定案。

### D2: `Ord` を持たせる

- 決定: 正準順（表示の順）で要素を比べ、次にペイロードを比べる。
- 理由: F# の RFC は .NET の型どうしの比較ができないことを理由に比較を自動では与えないが、Tsuzuri の匿名共用体はタグを持つので、名前付きの union の導出と同じ規則で決定的に比べられる。`Map`・`Set` のキーに使える利点が大きい。順は表示と同じで、利用者が確かめられる。
- 状態: 承認済み（2026-11-02 の利用者の回答「1~4 について承認します」。起票時の確認事項 1: 順は書いた順ではなく正準順）。

### D3: `Display` はペイロードだけ

- 決定: タグを書かず、要素の表示をそのまま使う。
- 理由: 匿名共用体のケースには名前がなく、型名を書くと値の表示が冗長になる（TypeScript の union・F# の消去された値と同じ見え方）。
- 状態: 既定案。

### D4: `Hash` は名前付きの union と同じストリーム

- 決定: タグ `0x55`、正準順の要素の番号、ペイロードのハッシュ。
- 理由: 既存の規則（`docs/language.md` の Canonical Stream の表）に新しいタグを足さない。要素の番号は正準順で決まり、モジュールの追加で変わらない（A24 の D3）。
- 状態: 既定案。

### D5: `Decode` は JSON の種類が互いに素なときだけ（Phase 2）

- 決定: 要素の JSON の種類が重なれば `E1025`。
- 理由: 既定の選び方（最初の要素など）を入れない（GUIDE §1 の規則 2）。タグを書かない `Encode` と往復できる範囲だけを受理する。
- 状態: 承認済み（2026-11-02 の利用者の回答「1~4 について承認します」。起票時の確認事項 4）。承認は設計についてで、Phase 2 の着手は従来どおり人間が求めた場合だけ。

### D6: インスタンスの本体は Specializer が型付き IR で作る

- 決定: 匿名共用体の `Eq`・`Ord`・`Hash`・`Display`（引用付きの表示を含む）と、Phase 2 の `Encode`・`Decode` の本体は、単相化（`Specializer`）が通常の型付き IR の関数として作る。タグの `If` の連鎖と、`Borrow(UnionPayload { .. })` を要素のメソッドに渡す腕から成る。`src/llvm_display.rs`・`src/llvm_hash.rs`・`src/llvm_compare.rs` は変えない。
- 理由: Phase 2 は型付き IR でしか作れないので、仕組みを 1 つにする。LLVM の側を変えないので、タプル・配列・リスト・名前付きの union の生成 IR は構成上変わらない。再帰・`Common`・`General` のレイアウトは既存の `UnionTag`・`UnionPayload` の生成が扱う。`Hash` は導出した union と同じ式（`$builtin.hash_mix` の列）なので、同じ値になる。
- 状態: 着手時の再検討（2026-11-11）で採用（監査の提案を調整役が採用。`target/scratch/audit/DECISIONS.md`）。
