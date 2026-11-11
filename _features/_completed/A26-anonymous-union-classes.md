# A26: 匿名共用体の型クラス（`Eq`・`Ord`・`Hash`・`Display`）と JSON（`Encode`・`Decode`）

| 項目 | 内容 |
| --- | --- |
| ID | A26 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | [A24](A24-anonymous-unions.md)（done）, A06, A07 |
| 後続 | – |
| 状態 | done（2026-11-11。Phase 1 と Phase 2。末尾の「実装と検証」） |
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

10. **検証（手順 1〜4 の時点。最終確認は手順 7）**:
    - 手順 1（基準）: `feat/a24-anonymous-unions` の先端（`bd625b4d`）の dev バイナリで、`features.mjs deriving` 209 件、`features.mjs anonymous_unions` 37 件、`cargo nextest run --locked --test deriving --test anonymous_unions` 77 件が通り、`tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を保存しました（`target/scratch/a26/before`）。T1 の基準は A24 の記録の 3062 passed、4 skipped です。
    - T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
    - T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 手順 2・3 と手順 4 のテストを入れた木で 3077 件のうち 3076 passed、4 skipped、1 件失敗（`driver::test_runner::tests::runner_rejects_malformed_indices_and_reaps_timed_out_children`。機械が混んでいるときの 1 秒の時間切れで、単独で回し直すと通る。このブランチの変更とは関係がありません）。`wide_unions_have_instances` を足した最終の木（手順 4 のコミット）では 3078 件すべて passed、4 skipped です（基準 3062 + 足した 16 件）。`tests/anonymous_unions.rs` は 85 件です。
    - 既存の出力（手順 6 の先取り）: 手順 2〜4 のバイナリで 186 単位を取り直し、`sh target/scratch/a26/compare.sh before after` は終了コード 0（native と wasm32 の IR・`check --json` が 1 byte も違わない）。
    - E2E（dev バイナリ）: `features.mjs deriving`（209 件）・`anonymous_unions`（37 件）・`dyn_dispatch`（58 件）・`hash_map`（199 件）が native と WASM、`-O0` と `-O3` で通りました。
    - 確保の追跡: 一時的な suite（コミットしていません。`target/scratch/a26/probe/Main.tz`）で、比較・`Hash.hash`・`Display.display`（単独とタプルの中）・`HashSet` のキーを 1 つの export にまとめ、手計算の値（`probe 5 = 210011011`、`probe -2 = 210031101`）と一致し、native の host の `live == 0` と WASM（import なし）を `-O0`・`-O3` で通りました。手順 5 で正式な suite に入れます。

### 実装中の判断（手順 5・6 と Phase 1 のドキュメント）

1. **suite の関数（手順 5）**: 本文の 4 関数 `equal`・`order`・`hashed`・`shown` に `sorted` と `digest` を足し、6 関数・44 件にしました（suite `anonymous_unions` は 37 件から 81 件）。どれも `key x`（偶数の `x` は `i64` の `x`、奇数の `x` は文字列 `s{x}`）の値を使います。
   - `equal x`: 10 個の比較を 10 進の桁にします。同じ要素の同じ値・同じ要素の違う値・別の要素の `==`、`!=` の 2 つ、注入した被演算子（`a == x`・`a == "s1"`）、`(f64 | string)` の NaN（`x / x`）と `-0.0`（`x * 0.0`）、メソッドの形 `Eq.eq` です。
   - `order x`: 別の要素との 4 つの演算子、同じ要素との 4 つの演算子、注入した `a < "s10"`・`a < 10` を 10 進の桁にします。
   - `sorted n`: `key (7 i mod n)` を `Set<(i64 | string)>` に入れ、`Ord` の順（`i64` を数の順に、次に文字列をコード単位の順に）に読んで表示をつないだ文字列の検査和と、要素数を返します。
   - `hashed n`: `HashSet` と `HashMap` のキーにして、異なるキーの数と、`key 0`・`"s3"` を入れた回数を数えます。
   - `digest x`: `Hash.hash (key x)` を返します。同じ順に case を並べた名前付きの union（`union Named = Number of i64 | Text of string deriving (Hash)`）と違えば 0 を返します。
   - `shown x`: 単独・文字列補間（引用符なし）と、タプル・配列・リスト・導出したレコード・導出した union の中（引用符付き）の表示を `|` でつなぎ、検査和と長さを返します。本文の「表示の長さ」より厳しく、文字の違いも検出します。
   - 参照の値は `tests/features.mjs` の JavaScript（BigInt）で、規則から書いています。比較は「型が違えば `i64` が先、同じなら値」（文字列は JavaScript の `<` がコード単位の順）、ハッシュは `docs/language.md` の Canonical Stream の定義から書いた FNV-1a（`i64` はタグ `0x13` と 8 バイト、文字列はタグ `0x53`・長さ・コード単位ごとに 2 バイト、共用体はタグ `0x55`・要素の位置・ペイロードのハッシュ）です。この JavaScript は、コンパイラを使わずに、監査の参照値 `15967365493956732001`（`1`）と `17017872639125386659`（`"one"`）を再現しました（`target/scratch/a26/try5/hash.mjs`）。手で求めた値（`equal 4 = 1110101001`、`equal 1 = 1111001001`、`equal 0 = 1100101001`、`order 4 = 1101010011`、`order 9 = 1101100`、`sorted 5` と `sorted 14` の文字列、`hashed 25 = 3031010`、`shown 4` と `shown -1` の文字列）を、参照の関数の `assert` として同じファイルに置きました。
   - `inspect`: IR に `hash_mix(i64 14695981039346656037, i64 85)` の呼び出しと、関数 `$intrinsic.Display.display_quoted.N` があることを確かめます。
   - 結果: `node tests/features.mjs target/debug/tsuzuri anonymous_unions` は 81 件が native（host の `live == 0`）と WASM（import なし）、`-O0` と `-O3` で通りました。
2. **既存の IR（手順 6）**: 手順 5 の後のバイナリで、`tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を取り直し、基準（`feat/a24-anonymous-unions` の先端 `bd625b4d` のバイナリ）と比べました。違うのは、手順 5 で fixture を変えた `tests/fixtures/anonymous_unions` の IR 2 つだけで、ほかの 185 単位は 1 byte も違いません。
   - 変える前の fixture（`bd625b4d` の `Main.tz`）は、基準と新しいバイナリで native・wasm32 とも同じ IR です。
   - 匿名共用体を使わず、タプル・配列・リスト・導出したレコードと union（再帰する union を含む）の `==`・`<`・`Hash.hash`・`Display.display`、`HashSet<Point>`、`Set<(i64 * string)>` を使うプログラム（`target/scratch/a26/step6/structural/Main.tz`）も、native（1,473,221 bytes）・wasm32（1,455,292 bytes）とも同じ IR です。
   - 新しい fixture の IR は、2 回の生成で同じです（決定的）。
3. **ドキュメントで確かめて書いた振る舞い**（どれもこのチケットで変えたものではなく、実装の結果です）:
   - 幅と寄せだけの補間 `$"{v:>5}"` は `Display` を使うので書けます。`$"{v:x}"` と `Format.format` は `Format` を要るので `E1005` です。
   - `"one" == b` は `E1003`（`expected string, found (i64 | string)`）です。左辺の型が右辺の期待される型になるためで、匿名共用体を左辺に書きます（接尾辞のない数のリテラルが左辺のときだけ、右辺を先に検査します）。
   - `HashSet.insert 1 empty` は通り、`HashSet.insert "one" set` は `'key = string` に決まって `E1003` です（数のリテラルだけ、型が後から決まります）。リファレンスの例は `key` 関数を使います。
   - `pure def` の中の匿名共用体の比較は、要素の `Eq` が `Debug.print` を呼ぶと `E1029` です（経路は `$intrinsic.Eq.eq.N` → 要素のインスタンス）。同じプログラムをタプルで書くと受理され、実行すると純粋でない `Eq` が呼ばれます（このチケットより前からの挙動で、変えていません。報告する事項です）。
4. **更新したページ**: `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`（「比較・順序・ハッシュ・表示」の節と `run=` の例 5 つ。「まだありません」の記述を実際の振る舞いに替えた）、`types-and-type-inference/type-classes.md`、`built-in-types-and-modules/hashmap.md`・`map.md`・`union.md`、`index.md`、`docs/language.md`（組み込みクラスの表の `Eq`・`Ord`・`Display`・`Hash` の行、Canonical Stream の union の行、「匿名共用体」の節）、`docs/architecture.md`（ファイルの表、構造的なインスタンスの節、「匿名共用体（A24）」の「組み込みのインスタンス（A26）」）、`README.md`（機能の一覧とテストの表）。`Encode`・`Decode` は Phase 2 で入れるので、この時点のページは「ない」と書いています。
5. **検証（手順 5・6 とドキュメントの時点）**:
   - `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs <変えた 6 ページ>`: 233 links、40 examples、native の実行 78 回（`-O0`・`-O3`）で通りました。`node scripts/gen-docs.mjs --check` は 19 blocks、0 stale（新しい kind はないので、生成するブロックは変わりません）。`node scripts/check-cli-docs.mjs` は 4 checks、`node scripts/check-guide-lite.mjs` は 88 references で通りました。
   - T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。この段では `src/` を変えていません。
   - T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3078 件すべて passed、4 skipped です（手順 4 の時点と同じ件数。この段は Rust のテストを足していません。記録は `target/scratch/a26/t1-stage2.txt`）。

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

### D7: `Encode`・`Decode` の本体と、`Decode` を持つ条件（Phase 2 の設計）

Phase 2 の着手時（2026-11-11）に `std/Json.tz`（`Value`・`Error`・`ErrorKind`、572〜1132 行のインスタンス、1194〜1308 行の導出用の関数）と `src/derive.rs`（`Build::record` 502〜、`Build::union` 819〜）を読み直して決めました。

- **現状**: `Encode`・`Decode` は組み込みクラスで、メソッド（`encode :: ref 'a -> Result<Json.Value, Json.Error>`、`decode :: ref Json.Value -> Result<'a, Json.Error>`）に `operation` がありません（`src/polymorph.rs` の `Classes::collect`）。インスタンスは `std/Json.tz` のソース（数・`bool`・`string`・`utf8string`・`Maybe`・配列・`Vec`・リスト・2〜4 要素のタプル・`Map`・`HashMap`・`Set`・`HashSet`・`Json.Value`）と、`deriving`・利用者のソースだけです。匿名共用体は head に書けない（`E1041`）ので、ソースのインスタンスは作れません。
- **判定**: `Classes::structural(class, ty)` を、匿名共用体の `Encode`・`Decode` でも真にします。`normalize` が要素ごとの同じクラスの制約に展開します（要素に `Encode`・`Decode` がなければ、要素の型を言う既存の `E1005`。導出の中なら既存の `E1025` `derived_instance_invalid`）。`Decode` だけ、展開の前に下の「種類の表」で要素を調べます。
- **`operation` のないメソッドの経路**: `Specializer::lower` の `Method` は、インスタンスが見つからなければ `intrinsic_function` に来ます。`intrinsic_function` は、`operation` が `None` で、型が匿名共用体で、`structural` が真のときだけ、新しい合成 `Specializer::union_codec` に回します（ほかの `operation` のないメソッドは従来どおり到達しません）。関数は `$intrinsic.Encode.encode.N`・`$intrinsic.Decode.decode.N` で、鍵は既存の `intrinsics`（クラス・メソッド・型）です。`dyn_of` の vtable は、`operation` があるときに加えて、匿名共用体で `structural` が真のときも `intrinsic_function` を使います（`dyn Encode` に格納できる）。`src/llvm*.rs` と `std/` は変えません（D6 のまま）。
- **`Encode.encode` の本体**: Phase 1 と同じ `If` の連鎖（`by_member`）で、腕 `k` は要素 `k` の `Encode.encode` を、ペイロードの借用に呼びます。タグも包みも書きません。結果は要素の `Encode` の結果そのもの（誤りも要素のもの。たとえば `f64` の NaN は `NonFinite`）です。
- **種類の表**（`Decode` の要素。HEAD で `Decode` を持つ型だけ。頭の型構築子で決まり、型引数の代入で変わりません）:

  | 要素 | JSON の種類 |
  | --- | --- |
  | `bool` | boolean |
  | 整数（`i8`〜`i128`、`i8u`〜`i128u`）・浮動小数点（`f16`〜`f128`）・`d32`〜`d128` | number |
  | `string`・`utf8string` | string |
  | `[T]`・`[|T|]`・`Vec<T>`・2〜4 要素のタプル・`Set<T>`・`HashSet<T>` | array |
  | `Map<K, V>`・`HashMap<K, V>` | object と array（キーが JSON の文字列にならないとき、`[key, value]` の配列を読むため） |
  | `Decode` を導出したレコード（そのレコードを head とする `Decode` のインスタンスが導出のもの） | object |
  | 上のどれでもなく、`Decode` のインスタンスに合う型（`Maybe<T>`・名前付きの union・`Json.Value`・手書きの `Decode` を持つ型） | 決まらない（どの要素とも重なる） |
  | 上のどれでもなく、`Decode` のインスタンスがない型（`char`・`utf8char`・`unit`・固定長配列・5 要素以上のタプル・関数など） | 表の対象外。要素の制約が既存の `E1005`（`no instance for Decode<char>`）になる |

  - 本文の表との違い: `char`・`utf8char`・`unit`・`bigint`・固定長配列・5 要素以上のタプルは、HEAD に `Decode` がないので外しました。`null` を読む要素は表にありません（`null` を読むのは `Maybe` と `Json.Value` で、どちらも「決まらない」です）。
  - `Maybe<T>` は `null` と `T` の種類、導出した名前付きの union は string と object、`Json.Value` はすべての種類を読みます。手書きの `Decode` は何を読むかをコンパイラが知りません。これらを細かく数えず、同じ「決まらない」にします（本文の仕様と再検討の 5 のとおり。最も保守的で、後から受理を広げても既存のプログラムは壊れません）。
- **重なりの拒否（`E1025`、kind `anonymous_union_decode_overlap`）**: 種類が決まる要素の間で同じ種類が 2 回現れるか、種類が決まらない要素が 1 つでもあれば、`Decode<U>` の制約は誤りです。既定の要素（最初の要素など）は選びません（D5）。メッセージは理由を言います。
  - 種類の重なり: `no Decode instance for (i64 | f64): the members i64 and f64 both read a JSON number, so the JSON value does not select one member; decode Json.Value and match on it, or use a union with named cases`（型と要素は正準順。種類の言い方は `a JSON boolean`・`a JSON number`・`a JSON string`・`a JSON array`・`a JSON object`）
  - 決まらない要素: `no Decode instance for (i64 | Maybe<i64>): the member Maybe<i64> is not limited to one kind of JSON value (a Maybe, a union with named cases, Json.Value, or a type with a hand-written Decode can read several), so the JSON value does not select one member; decode Json.Value and match on it, or use a union with named cases`
  - `Decode` のインスタンスがない要素があるときは、重なりを調べる前に要素の制約へ進み、`E1005` を先に出します（「インスタンスがない」のほうが直すべき点を正しく言うため）。
  - 調べる位置は `normalize`（制約を解く全経路: 関数の本体、導出したインスタンスの文脈、`dyn`、`holds`）と、`union_codec`（本体を作る直前。制約の検査を通らずに届いた場合の二重の検査で、同じ kind を返します。黙って要素を選ぶことも、内部エラーにすることもありません）。
- **`Decode.decode` の本体**: 引数 `ref Json.Value` のタグ（`UnionTag(Dereference(引数))`）で分岐する `If` の連鎖です。JSON の種類ごとに、その種類を読む要素 `k` の腕を置きます（`Bool`・`Number`・`Text`・`Items`・`Object` の case の番号の順。番号は `Json.Value` の宣言から名前で引きます）。
  - 腕 `k`: `match Decode.decode<要素 k> json with | Ok v -> Ok (v を要素 k として包む) | Error e -> Error e` を、検査器が同じソースに作るのと同じ形の `Match`（`UnionTag` の検査と `UnionPayload` の束縛）で作ります。要素の誤りはそのまま返します（`(i64 | string)` に `1.5` を読ませると、`i64` の `json: expected integer`）。
  - どの種類にも当たらないとき: `Result.Error (Json.Error { kind: ExpectedType "<要素の種類>", offset: -1 })`。種類の名前は `boolean`・`number`・`string`・`array`・`object` で、要素の正準順に ` or ` でつなぎます（`(i64 | string)` は `json: expected number or string`。`Map` の要素は `object or array`）。std の既存の書き方（`expected "boolean"`、`"object or array"`、`"string or object"`）に合わせています。
  - 再検討の 6 は `Result.map (CaseConstructor U k) (...)` を考えていましたが、関数値（`closures::lower` が作る `$case.$anonymous.Union$n...` という関数）を経由せず、直接の呼び出しと `Match` だけにしました。`-O0` でも間接呼び出しがなく、合成した名前の関数が増えません。std に関数は足しません。`Json.Value`・`Json.Error`・`Json.ErrorKind`・`Result` の形はメソッドのシグネチャの型から読み、case とフィールドは名前で引きます。見つからなければ既存の `E1004`（`std_item_missing`）です。
- **往復**: 種類が互いに素なので、`Encode` が書いた JSON の種類は、書いた要素だけが読みます（数 → number、文字列 → string、`bool` → boolean、配列・タプル・集合 → array、導出したレコード → object、`Map` → object か array）。値そのものの往復は要素の `Encode`・`Decode` の性質のままです（NaN は `Encode` が `NonFinite`、範囲外の数は `Decode` が `NumberRange`）。
- **`dyn`**: `dyn Encode` に格納できます（タプルと同じ）。`Decode` は受け手のないメソッドなので、もともと `dyn` にできません。
- **導出**: `deriving (Encode, Decode)` のレコード・union は、匿名共用体のフィールド・ペイロードを持てます（導出が文脈に書く `Decode<(i64 | string)>` がこの規則で解けるため）。重なる匿名共用体のフィールドは、同じ `anonymous_union_decode_overlap` です（導出のせいではなく、フィールドの型のせいなので、`derived_instance_invalid` に付け替えません）。導出したレコードの欠けたキーは、従来どおり `null` として decode され、匿名共用体は `null` を読まないので `MissingField` になります。
- **健全にできず拒否するもの**: 上の「重なり」と「決まらない要素」だけです。`Encode` には制限がありません。
- 理由: タグを書かない `Encode`（D5 の前提、TypeScript の union と同じ見え方）と往復できる範囲だけを `Decode` が受理し、既定の選び方を入れない（GUIDE §1 の規則 2）。本体は D6 の仕組み（型付き IR）で、LLVM と std を変えない。
- 状態: Phase 2 の着手時の設計（2026-11-11）。D5 の承認の範囲内。

## 実装と検証（2026-11-11）

ブランチ `feat/a26-anonymous-union-classes`（`feat/a24-anonymous-unions` の `bd625b4d` の上）。Phase 1 と Phase 2 の両方を実装しました。Phase 1 の判断と検証は「実装中の判断（手順 2〜4）」「実装中の判断（手順 5・6 と Phase 1 のドキュメント）」に、Phase 2 の設計は D7 にあります。この節は、Phase 2 の実装と、チケット全体の最終の検証をまとめます。

### 実装した範囲

| 段 | 内容 | コミット |
| --- | --- | --- |
| 再計画 | 「着手時の再検討」と D6 | `534fee42` |
| Phase 1 手順 2・3 | `Classes::anonymous`・`structural`・`structural_elements`、`Specializer::union_method`（`Eq`・`Ord`・`Hash`・`Display`）、`quoted_display_function` | `4fe0bf84` |
| Phase 1 手順 4 | 導出・コレクションのキー・注入した被演算子・`dyn`・`const def` のテスト | `ac7d2c47`、`b3c0c711` |
| Phase 1 手順 5 | suite `anonymous_unions` の `equal`・`order`・`sorted`・`hashed`・`digest`・`shown` | `9204c9d6` |
| Phase 1 手順 6・7 | 既存の IR の比較、ドキュメント | `2a769e9c`、`1808a3a1` |
| Phase 2 の設計 | D7 | `913779fa` |
| Phase 2 の実装 | `Encode`・`Decode`、kind `anonymous_union_decode_overlap`、`dyn Encode`、テスト、suite の `encoded`・`decoded` | `c9f2b2aa` |
| Phase 2 のドキュメント | `json.md` ほか、`explain E1025`、再帰するレコードとメソッド値のテスト | `8a30fe0f` |

### Phase 2 で作ったもの

- `src/diagnostic_kinds.rs`: `AnonymousUnionDecodeOverlap`（`anonymous_union_decode_overlap`、`E1025`。`E1025` の組の先頭）。`src/explain/codes/E1025.md` に説明と修正を足しました（1,399 文字。上限は 1,500 文字）。
- `src/anonymous_union.rs`: `JsonKind`（`Boolean`・`Number`・`Text`・`Array`・`Object`。`Json.Value` の case の順）、`JsonKinds`（`Fixed`・`Open`・`Missing`）、`expected_kinds`、`decode_body`（`Decode.decode` の本体）。
- `src/polymorph.rs`: `Classes::structural`（匿名共用体の `Encode`・`Decode` で真）、`Classes::decode_kinds`・`decode_members`（種類の表と重なりの判定）、`normalize`（`Decode` の構造的な展開の前に重なりを `E1025` にする）、`Specializer::dyn_of`（匿名共用体の構造的なインスタンスも `intrinsic_function` へ）、`intrinsic_function`（`operation` が `None` の匿名共用体を `union_codec` へ）、`Specializer::union_codec`。
- `src/llvm*.rs`・`std/`・`src/runtime/` は変えていません。新しい crate・`unsafe`・WASM の import・ランタイム関数はありません。合成した本体は、ほかの関数と同じ所有権の検査・純粋性の検査を通ります。

### 実装中の判断（Phase 2）

1. **本体は `Result.map` ではなく `Match`**（再検討の 6 からの変更。D7 に記録）: 腕 `k` は `Match { 値: Decode.decode<要素 k>(json), 腕: [Ok v -> Ok (v を要素 k として包む), Error e -> Error e] }` です。検査器が `match` に作る形（`PatternStep::Test(UnionTag(..) == Int(case))` と `UnionPayload` の束縛）をそのまま使いました。`-O0` の IR は、`Json.Value` のタグの比較と、要素の `Decode.decode`（`Json.$instance.N.decode`）の直接呼び出しだけで、関数値・間接呼び出し・`$case.$anonymous...` という関数はありません（`target/scratch/a26/p2/det1.ll` で確認）。所有権の検査は、この形をそのまま受理しました（非 Copy の要素、`Map`、導出したレコード、再帰するレコード）。
2. **std の形の読み方**: `Json.Value`・`Result`・`Json.Error`・`Json.ErrorKind` の id は、`Decode.decode` のシグネチャ（`ref Json.Value -> Result<'a, Json.Error>`）の型から読みます。case（`Bool`・`Number`・`Text`・`Items`・`Object`・`Ok`・`Error`・`ExpectedType`）とフィールド（`kind`・`offset`）は名前で引き、なければ `E1004`（`std_item_missing`）です。std の private な関数（`Json.expected`）には依存しません。
3. **`Missing` を先に**: `Decode` のインスタンスがない要素（`char`、導出していないレコード、5 要素以上のタプル）があるときは、重なりを調べずに要素の制約へ進みます。診断は `no instance for Decode<char>` です（`(char | i64 | f64)` でも、先に直すべき `char` を言います）。
4. **導出の文脈**: `record R { v: (i64 | f64) } deriving (Decode)` は、`deriving` の位置で `anonymous_union_decode_overlap` のままです（`derived_error` が付け替えるのは `E1005`・`E1027` だけなので、理由を言うメッセージと kind が残ります）。要素に `Decode` がない場合（`(i64 | Plain)`）は、従来どおり `derived_instance_invalid` です。
5. **総称の関数**: `Decode<'a> => ... -> Result<(['a] | string), Json.Error>` は受理します（種類は頭の型構築子で決まる）。`(['a] | Vec<'a>)` は定義の位置で `E1025` です。型変数を含むレコードで、一般の head の導出がなく特定の具体化にだけ手書きのインスタンスがあるような場合は、定義の位置では `Missing`（要素の制約が残る）になり、具体化の時点で `union_codec` が同じ検査をして `E1025` を返します（黙って要素を選ぶ本体は作りません）。
6. **署名の制約**: `def f :: Decode<(i64 | f64)> => ...` のように制約を明示しても、同じ `E1025` です。
7. **`pure def`・`const def`・`dyn`**: `pure def` の中の `Decode.decode`・`Encode.encode` は受理されます（std の `Json` のインスタンスが純粋なため）。`const def` の中では、ほかのインスタンスの呼び出しと同じ `E1026` です。`dyn Encode` には格納できます。
8. **デバッグ情報とカバレッジ**: 合成した局所変数（`$decoded`・`$value`・`$error`）と関数（`$intrinsic.Decode.decode.N`）は、DWARF の名前に現れません（`tsuzuri test --index 0 -g` の出力を `dwarfdump` で確認）。`--coverage` は利用者の行だけを数えます。
9. **テストの期待値を 1 件直しました**（実装の出力に合わせたのではなく、規則の読み違いの訂正です）: `(Point | Map<string, i64>)` の表示を `(Map<string, i64> | Main.Point)` と書いていましたが、レコードどうしは宣言の修飾名で並ぶ（言語リファレンスの「型の書き方」）ので、`Main.Point` が `Map.Map` より前です。期待値を `(Main.Point | Map<string, i64>)` に直しました。
10. **保守的に拒否したもの**: `Maybe<T>`・名前付きの union・`Json.Value`・手書きの `Decode` を持つ型を要素にした匿名共用体の `Decode`（D7 の「決まらない」）。`(Maybe<i64> | string)` は `null`・number と string で実際には重なりませんが、チケットの仕様どおり拒否しています。受理を広げる変更は、既存のプログラムを壊しません。`Encode` には制限がありません。

### テスト

- `tests/anonymous_unions.rs`: 85 件から 92 件（Phase 2 で 7 件）。期待値は、JSON の規則（空白なし、レコードは宣言順の object、タプルは配列、`None` は `null`）と `std/Json.tz` の誤りの文から手で書きました。実行するテストは native と WASM、`-O0` と `-O3`（`passes`）で通します。
  - `encode_writes_payload_without_tag`（edition 2026 と 2027。ペイロードだけ、`Decode` を持てない型の `Encode`、NaN の `NonFinite`、入れ物の中、IR の決定性、要素に `Encode` がないときの `E1005`）
  - `dyn_encode_stores_anonymous_union`
  - `decode_selects_member_by_kind`（edition 2026 と 2027。種類ごとの要素、表のほかの型、総称の関数、入れ子、メソッドの形）
  - `decode_overlap_is_rejected`（`(i64 | f64)`・`(Map<i64, string> | [i64])`・`(Maybe<i64> | string)` を含む 15 例のメッセージ全文、`E1005` が先に出る 3 例、受理する 3 例）
  - `decode_mismatch_names_member_kinds`（`json: expected number or string` など、`kind == Json.ExpectedType "number or string"` と `offset == -1`、選ばれた要素の誤り）
  - `derived_record_with_anonymous_field_round_trips`（edition 2026 と 2027。レコード・union・総称のレコード、欠けたキーの `MissingField`）
  - `recursive_records_and_method_values_decode`（`record Tree { value: (i64 | [Tree]) }`、`Decode.decode` を関数値として渡す）
- suite `anonymous_unions`（`tests/fixtures/anonymous_unions/Main.tz`、`tests/features.mjs`）: `encoded`（9 件）と `decoded`（6 件）を足し、81 件から 96 件。`doc x` は `x mod 5` で `i64`・`bool`・文字列・配列・導出したレコードのどれかです。
  - `encoded x`: 単独の値、5 つの値の配列、導出したレコードのフィールドの JSON テキストをつないだ文字列の検査和と長さ。参照は JavaScript で規則から書いた JSON の書き手（`jsonOf`）で、手で書いた 2 つの文字列（`x = 0`・`x = -3`）と `JSON.stringify` の出力（1 例）を `assert` で照合しています。
  - `decoded x`: 種類ごとの 6 つの JSON テキスト、レコードの 3 つ（余分なキー、欠けたキー）、どの要素も読まない・要素が拒否する 6 つ（`null`、`x.5`、`[1, "two"]`、範囲外の整数、フィールドの型違い、構文の誤り）、`(i64 | Map<string, i64>)` の 6 つ（object と `[key, value]` の配列の両方）、5 つの値と導出したレコードの往復。参照は `std/Json.tz` の規則から手で書いた文字列です。
  - `inspect`: IR に `$intrinsic.Encode.encode.N`・`$intrinsic.Decode.decode.N` の定義があること。

### 検証

- T0: `cargo fmt --all -- --check`（差分なし）、`cargo clippy --all-targets --locked -- -D warnings`（警告なし）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 手順 4 の時点の 3078 件に Phase 2 の 7 件を足した 3085 件がすべて passed、4 skipped です（最終の木。記録は `target/scratch/a26/t1-final.txt`）。Phase 2 の実装のコミットの時点（3084 件）では 3083 passed、1 件失敗（下の「残っていること」の `runner_rejects_malformed_indices_and_reaps_timed_out_children`。機械が混んでいるときの時間切れで、最終の実行では通っています）。
- 領域のテスト: `cargo nextest run --locked --test anonymous_unions --test explain --test diagnostic_kinds --test json --test deriving` は 140 件すべて passed。
- E2E（dev バイナリ、native と WASM、`-O0` と `-O3`、native は host の `live == 0`、WASM は import なし）: `node tests/features.mjs target/debug/tsuzuri <suite>` で `anonymous_unions` 96 件、`deriving` 209 件、`typeclasses` 39 件、`dyn_dispatch` 58 件、`hash_map` 199 件、`unions` 27 件、`json` 165 件、`cbor` 86 件が通りました。`node tests/json.mjs target/debug/tsuzuri` は 297 入力・2,523 検査が通りました（記録は `target/scratch/a26/suites-stage3.txt`）。
- 既存の IR（手順 6 のやり直し）: Phase 2 のバイナリで `tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を取り直し、基準（`bd625b4d` のバイナリ）と比べました。違うのは、fixture を変えた `tests/fixtures/anonymous_unions` の IR 2 つだけで、ほかの 185 単位は 1 byte も違いません。`sh target/scratch/a26/step6/run.sh old_fixture structural`（A24 の fixture と、タプル・配列・リスト・導出を使うプログラム）は終了コード 0（native 243,487・1,473,221 bytes、wasm32 224,480・1,455,292 bytes が同じ）。
- Phase 1 の IR: Phase 2 の前のコミット（`1808a3a1`）を `target/scratch/a26/stage2` に取り出して別にビルドし、その時点の fixture（Phase 1 の 6 関数を含む）を両方のバイナリで IR にしました。native（1,056,787 bytes）・wasm32（1,047,682 bytes）・`check --json` が同じです（`target/scratch/a26/stage2/compare.sh`。ビルドした木は消しました）。Phase 2 は、Phase 1 のインスタンスの IR も変えていません。
- 新しい fixture の IR は 2 回の生成で同じです（決定的）。
- ドキュメント: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs <変えた 4 ページ>` は 111 links、34 examples、native の実行 66 回（`-O0`・`-O3`）で通りました。`node scripts/gen-docs.mjs` は 1 ブロック（診断の kind の表）を更新し、`--check` は 19 blocks、0 stale。`node scripts/check-cli-docs.mjs` は 4 checks、`node scripts/check-guide-lite.mjs` は 88 references、`node scripts/check-links.mjs --into _features/README.md` は 11 links、0 broken で通りました。
- `src/runtime/` は変えていないので、`scripts/check-runtime-includes.sh` は対象外です。

### 更新したドキュメント（Phase 2）

- `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`: 「JSON（Encode と Decode）」の節（種類の表、`run=` の例 3 つ、`E1025` の例）、ポイント・型の性質の表・使い分け・まとめ・関連項目。「`Encode`・`Decode` はない」という記述を実際の振る舞いに替えました。
- `_tsuzuri/language-reference/built-in-types-and-modules/json.md`: 型の表の行、「匿名共用体」の節（種類の表、`run=` の例、制限）、ポイント・まとめ・関連項目、「インスタンスは Json と利用者のソースだけ」の文。
- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`: 組み込みクラスの表の `Encode`・`Decode` の行、構造的なインスタンスの段落。
- `_tsuzuri/language-reference/compiler/diagnostics.md`: `E1025` の行と、生成される kind の表。
- `docs/language.md`: 組み込みクラスの表、「匿名共用体」の節、`Json` の項、`E1025` の行。`docs/architecture.md`: ファイルの表（`std/Json.tz`・`src/anonymous_union.rs`）、構造的なインスタンスの節、「組み込みのインスタンス（A26）」。
- `README.md`（機能の一覧とテストの表）、`_features/GUIDE.md`（`E1025` の行、D-55 の `E1041` の行の注記、D-62 の型クラスの項）、`src/explain/codes/E1025.md`。

### 変えたファイル（チケット全体）

`src/polymorph.rs`、`src/anonymous_union.rs`、`src/diagnostic_kinds.rs`、`src/explain/codes/E1025.md`、`tests/anonymous_unions.rs`、`tests/features.mjs`、`tests/fixtures/anonymous_unions/Main.tz`、`docs/language.md`、`docs/architecture.md`、`README.md`、`_features/GUIDE.md`、`_features/README.md`、`_tsuzuri/language-reference/index.md`、`_tsuzuri/language-reference/built-in-types-and-modules/`（`anonymous-union.md`・`json.md`・`hashmap.md`・`map.md`・`union.md`）、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/compiler/diagnostics.md`、このチケット。

### チケットの本文と違う点（まとめ）

- インスタンスの本体は LLVM の分岐ではなく、Specializer が型付き IR で作る（D6）。`src/llvm_display.rs`・`src/llvm_hash.rs`・`src/llvm_compare.rs` は変えていない。
- `dyn` への格納は拒否せず、タプルと同じく受理する（再検討の 4。`E1028` の規則は取り消し）。
- Phase 2 の種類の表は、HEAD に `Decode` がある型だけ（`char`・`unit`・`bigint`・固定長配列・5 要素以上のタプルを外した）。`Map`・`HashMap` は object と array の両方、`Set`・`HashSet` は array。種類が決まらない要素は `E1005` ではなく、理由を言う `E1025`（D7）。
- 型の不一致の誤りは `Json.DecodeError` ではなく、既存の `Json.Error { kind: ExpectedType "...", offset: -1 }`。
- `Decode` の腕は `Result.map` ではなく `Match`（D7、上の 1）。
- テスト: 本文と再検討の名前のテストに加えて、`dyn_encode_stores_anonymous_union`・`recursive_records_and_method_values_decode` と、suite の `sorted`・`digest`・`encoded`・`decoded` を足した。

### 残っていること・報告する事項

- `feat/a25-anonymous-union-conversions` の取り込み（A24 ← A25 ← A26 の順に積む、調整役の決定）は、この段ではしていません。取り込んだ後に、T1・suite `anonymous_unions`・上の IR の比較をやり直します。
- `driver::test_runner::tests::runner_rejects_malformed_indices_and_reaps_timed_out_children` は、機械が混んでいるときに 1 秒の時間切れで落ちることがあります（単独では通る。このブランチの変更とは関係がありません）。
- このチケットより前からの挙動（変えていません）: `pure def` の中のタプルの比較（`StructuralCompare`）は、要素のインスタンスの効果を辿りません（要素の `Eq` が `Debug.print` を呼んでも受理される）。匿名共用体のインスタンスは通常の関数なので辿られ、`E1029` になります。
