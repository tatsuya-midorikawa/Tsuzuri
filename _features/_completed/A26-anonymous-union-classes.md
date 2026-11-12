# A26: 匿名共用体の型クラス（`Eq`・`Ord`・`Hash`・`Display`）と JSON（`Encode`・`Decode`）

| 項目 | 内容 |
| --- | --- |
| ID | A26 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | [A24](A24-anonymous-unions.md)（done）, A06, A07 |
| 後続 | – |
| 状態 | done（2026-11-11。Phase 1 と Phase 2。末尾の「実装と検証」。A25 を取り込み、A24 ← A25 ← A26 の順に積んだ（「A25 との統合」。Copilot のレビューの第1回の後に「A25 の再取り込み」）） |
| 起票 | 2026-11-02（A24 と同時。HEAD `2f556480`） |
| 承認 | `Ord` を正準順で持たせること（D2）と、`Decode` を JSON の種類が互いに素な場合に限ること（D5）は、2026-11-02 の利用者の回答「1~4 について承認します」で承認済み（GUIDE D-62 の決定 4・5）。ほかは既定案。Phase 2 は人間が求めた場合だけ着手する |
| 手本にする既存実装 | タプル・配列・リストの構造的なインスタンス: `Classes::structural`（`src/polymorph.rs:2556`）、制約の展開 `normalize`（`2382-2401`）、型付き IR への下ろし（`5230`、`5311-5399`）、LLVM の `structural_display`（`src/llvm_display.rs:548`）・`display_quoted`（`601`）・`structural_hash`（`src/llvm_hash.rs:17`）・`structural_compare`（`src/llvm_compare.rs:69`）。union の導出の意味: `src/derive.rs`（`Build::union`、`817`） |
| 主な影響ファイル | `src/polymorph.rs`, `src/anonymous_union.rs`, `src/ownership.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/E1025.md`, `tests/anonymous_unions.rs`, `tests/fixtures/anonymous_unions/Main.tz`, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `_features/GUIDE.md`, `_tsuzuri/language-reference/built-in-types-and-modules/anonymous-union.md`, `_tsuzuri/language-reference/built-in-types-and-modules/json.md`, `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`, `_tsuzuri/language-reference/compiler/diagnostics.md`（LLVM の表示・ハッシュ・比較の生成は D6 により変えていません。全体は末尾の「変えたファイル」） |

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
  - 型変数を持ち、すべての型引数についての `Decode` のインスタンスがない要素（第1回のレビューで追加。`JsonKinds::Unsettled`）: `no Decode instance for (i64 | Lib.Pair<'a>): the member Lib.Pair<'a> has no Decode instance for every type of 'a (a derived instance reads a JSON object; an instance for some types only is hand-written and not limited to one kind of JSON value), so the JSON value does not select one member; ...`。具体化した先で合うインスタンスは手書きのもの（種類が決まらない）しかないので、匿名共用体を書いた位置で拒否します。→ 第2回のレビューで取り消しました。基準（A24）が受理していた、呼び出さない総称の関数を拒否するためです。型変数を持つ匿名共用体の制約は、要素の制約がすべて満たせ、種類が決まって重ならないときだけ要素の制約に分け、そうでなければ匿名共用体のまま残して、型変数が決まる呼び出しの位置で調べます（「レビューの指摘への対応（第2回）」の 1）。
  - `Decode` のインスタンスがない要素があるときは、重なりを調べる前に要素の制約へ進み、`E1005` を先に出します（「インスタンスがない」のほうが直すべき点を正しく言うため）。
  - 調べる位置は `normalize`（制約を解く全経路: 関数の本体、導出したインスタンスの文脈、`dyn`、`holds`）と、`union_codec`（本体を作る直前。制約の検査を通らずに届いた場合の二重の検査で、同じ kind を返します。黙って要素を選ぶことも、内部エラーにすることもありません）。
- **`Decode.decode` の本体**: 引数 `ref Json.Value` のタグ（`UnionTag(Dereference(引数))`）で分岐する `If` の連鎖です。JSON の種類ごとに、その種類を読む要素 `k` の腕を置きます（`Bool`・`Number`・`Text`・`Items`・`Object` の case の番号の順。番号は `Json.Value` の宣言から名前で引きます）。
  - 腕 `k`: `match Decode.decode<要素 k> json with | Ok v -> Ok (v を要素 k として包む) | Error e -> Error e` を、検査器が同じソースに作るのと同じ形の `Match`（`UnionTag` の検査と `UnionPayload` の束縛）で作ります。要素の誤りはそのまま返します（`(i64 | string)` に `1.5` を読ませると、`i64` の `json: expected integer`）。
  - どの種類にも当たらないとき: `Result.Error (Json.Error { kind: ExpectedType "<要素の種類>", offset: -1 })`。種類の名前は `boolean`・`number`・`string`・`array`・`object` で、要素の正準順に ` or ` でつなぎます（`(i64 | string)` は `json: expected number or string`。`Map` の要素は `object or array`）。std の既存の書き方（`expected "boolean"`、`"object or array"`、`"string or object"`）に合わせています。
  - 再検討の 6 は `Result.map (CaseConstructor U k) (...)` を考えていましたが、関数値（`closures::lower` が作る `$case.$anonymous.Union$n...` という関数）を経由せず、直接の呼び出しと `Match` だけにしました。`-O0` でも間接呼び出しがなく、合成した名前の関数が増えません。std に関数は足しません。`Json.Value`・`Json.Error`・`Json.ErrorKind`・`Result` の形はメソッドのシグネチャの型から読み、case とフィールドは名前で引きます。見つからなければ既存の `E1004`（`std_item_missing`）です。
- **往復**: 種類が互いに素なので、`Encode` が書いた JSON の種類は、書いた要素だけが読みます（数 → number、文字列 → string、`bool` → boolean、配列・タプル・集合 → array、導出したレコード → object、`Map` → object か array）。値そのものの往復は要素の `Encode`・`Decode` の性質のままです（NaN は `Encode` が `NonFinite`、範囲外の数は `Decode` が `NumberRange`）。
  - 条件（第1回のレビューで補足）: これは、各要素の `Encode` が書く種類と `Decode` が読む種類が同じときの性質です。std のインスタンスと、`Encode`・`Decode` の両方を導出した型はそうです。コンパイラが調べるのは `Decode` の側（種類の表）だけで、`Encode` には制限がありません。`Decode` を導出して `Encode` を手書きしたレコードが object 以外を書くと、その JSON は、その種類を読む別の要素の値になるか、どの要素にも読まれません（そのレコードは単独でも往復しません）。言語リファレンスにこの条件を書き、`decode_reads_the_kind_whatever_encode_wrote` が振る舞いを固定します。
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

1. **本体は `Result.map` ではなく `Match`**（再検討の 6 からの変更。D7 に記録）: 腕 `k` は `Match { 値: Decode.decode<要素 k>(json), 腕: [Ok v -> Ok (v を要素 k として包む), Error e -> Error e] }` です。検査器が `match` に作る形（`PatternStep::Test(UnionTag(..) == Int(case))` と `UnionPayload` の束縛）をそのまま使いました。`-O0` の IR は、`Json.Value` のタグの比較と、要素の `Decode.decode`（`Json.$instance.N.decode`）の直接呼び出しだけで、関数値・間接呼び出し・`$case.$anonymous...` という関数はありません（新しい fixture の `-O0` の IR で確認）。所有権の検査は、この形をそのまま受理しました（非 Copy の要素、`Map`、導出したレコード、再帰するレコード）。
2. **std の形の読み方**: `Json.Value`・`Result`・`Json.Error`・`Json.ErrorKind` の id は、`Decode.decode` のシグネチャ（`ref Json.Value -> Result<'a, Json.Error>`）の型から読みます。case（`Bool`・`Number`・`Text`・`Items`・`Object`・`Ok`・`Error`・`ExpectedType`）とフィールド（`kind`・`offset`）は名前で引き、なければ `E1004`（`std_item_missing`）です。std の private な関数（`Json.expected`）には依存しません。
3. **`Missing` を先に**: `Decode` のインスタンスがない要素（`char`、導出していないレコード、5 要素以上のタプル）があるときは、重なりを調べずに要素の制約へ進みます。診断は `no instance for Decode<char>` です（`(char | i64 | f64)` でも、先に直すべき `char` を言います）。
4. **導出の文脈**: `record R { v: (i64 | f64) } deriving (Decode)` は、`deriving` の位置で `anonymous_union_decode_overlap` のままです（`derived_error` が付け替えるのは `E1005`・`E1027` だけなので、理由を言うメッセージと kind が残ります）。要素に `Decode` がない場合（`(i64 | Plain)`）は、従来どおり `derived_instance_invalid` です。
5. **総称の関数**: `Decode<'a> => ... -> Result<(['a] | string), Json.Error>` は受理します（種類は頭の型構築子で決まる）。`(['a] | Vec<'a>)` は定義の位置で `E1025` です。型変数を含むレコードで、一般の head の導出がなく特定の具体化にだけ手書きのインスタンスがあるような場合は、定義の位置では `Missing`（要素の制約が残る）になり、具体化の時点で `union_codec` が同じ検査をして `E1025` を返します（黙って要素を選ぶ本体は作りません）。→ 第1回のレビューで変更しました。この場合は定義の位置で `E1025`（`JsonKinds::Unsettled`）です（下の「レビューの指摘への対応（第1回）」の 3・5）。→ 第2回のレビューで再び変えました。定義は受理し、匿名共用体の制約のまま呼び出し元へ伝え、型変数が決まる呼び出しの位置で `E1025`・`E1005` です（「レビューの指摘への対応（第2回）」の 1）。
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

`src/polymorph.rs`、`src/anonymous_union.rs`、`src/ownership.rs`（第2回のレビューで。`E1012` の help）、`src/diagnostic_kinds.rs`、`src/explain/codes/E1025.md`、`tests/anonymous_unions.rs`、`tests/features.mjs`、`tests/fixtures/anonymous_unions/Main.tz`、`docs/language.md`、`docs/architecture.md`、`README.md`、`_features/GUIDE.md`、`_features/README.md`、`_tsuzuri/language-reference/index.md`、`_tsuzuri/language-reference/built-in-types-and-modules/`（`anonymous-union.md`・`json.md`・`hashmap.md`・`map.md`・`union.md`）、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/compiler/diagnostics.md`、`_tsuzuri/language-reference/pattern-matching/`（`match.md`・`pattern-matching.md`。第1回のレビューで）、このチケット。

### チケットの本文と違う点（まとめ）

- インスタンスの本体は LLVM の分岐ではなく、Specializer が型付き IR で作る（D6）。`src/llvm_display.rs`・`src/llvm_hash.rs`・`src/llvm_compare.rs` は変えていない。
- `dyn` への格納は拒否せず、タプルと同じく受理する（再検討の 4。`E1028` の規則は取り消し）。
- Phase 2 の種類の表は、HEAD に `Decode` がある型だけ（`char`・`unit`・`bigint`・固定長配列・5 要素以上のタプルを外した）。`Map`・`HashMap` は object と array の両方、`Set`・`HashSet` は array。種類が決まらない要素は `E1005` ではなく、理由を言う `E1025`（D7）。
- 型の不一致の誤りは `Json.DecodeError` ではなく、既存の `Json.Error { kind: ExpectedType "...", offset: -1 }`。
- `Decode` の腕は `Result.map` ではなく `Match`（D7、上の 1）。
- テスト: 本文と再検討の名前のテストに加えて、`dyn_encode_stores_anonymous_union`・`recursive_records_and_method_values_decode` と、suite の `sorted`・`digest`・`encoded`・`decoded` を足した。

### 残っていること・報告する事項

- `feat/a25-anonymous-union-conversions` の取り込み（A24 ← A25 ← A26 の順に積む、調整役の決定）は、この段ではしていませんでした。後で取り込み、T1・suite `anonymous_unions`・IR の比較をやり直しました（末尾の「A25 との統合（2026-11-11）」）。
- `driver::test_runner::tests::runner_rejects_malformed_indices_and_reaps_timed_out_children` は、機械が混んでいるときに 1 秒の時間切れで落ちることがあります（単独では通る。このブランチの変更とは関係がありません）。
- このチケットより前からの挙動（変えていません）: `pure def` の中のタプルの比較（`StructuralCompare`）は、要素のインスタンスの効果を辿りません（要素の `Eq` が `Debug.print` を呼んでも受理される）。匿名共用体のインスタンスは通常の関数なので辿られ、`E1029` になります。

### レビューの指摘への対応（第1回）

2026-11-11 のレビュー（9 件、すべて minor。記録は調整役の `target/scratch/wave1/review_A26_r1.json`）です。9 件とも、指摘のコミット `210b631a` のバイナリで再現しました。コードとテストはコミット `fc8ede30`、ドキュメントとこの節はその次のコミットです。「`210b631a` で失敗する」と書いたテストは、指摘の前のコードで落ち、いまのコードで通ります。

1. **広い匿名共用体の `<` が `E1017`（128 個の要件）で、`==` は通る**（指摘 1。指摘 6 も同じ原因）: 直しました。`Classes::normalize` は、匿名共用体が要素ごとに出す要件と、その要件の上位クラスの要件（`Ord<要素>` が要る `Eq<要素>`）を、128 個の予算に数えません（`pending` の 4 番目の欄）。要素は匿名共用体にならないので、数えない要件は、数える要件である共用体の要件 1 個につき高々 32 個で、解決は有界のままです。要素の中の型（タプルや配列の要素、導出したレコードのフィールドの型）は従来どおり数えるので、上限そのものは残ります。匿名共用体を解いていて上限を超えた `E1017` には、数え方と対処（型の種類を減らす、手書きのインスタンスを持つレコードで包む）を言う help を付けました。匿名共用体のない解決（タプルなど）の診断は変わりません。
   - テスト: `wide_unions_resolve_within_the_obligation_budget`（指摘の w2・w3・w4 の形: 3 つのレコードのタプル 16 要素、レコードの配列 32 要素、32 要素の匿名共用体 2 つのタプルと、指摘 6 の 21 種類の型の `[[T]]`・`[|[T]|]` 32 要素。`<`・`<=`・`>`・`>=`・`==`・`Hash`・`Display` の値を正準順序から手で書いた）、`distinct_parts_of_members_meet_the_obligation_limit`（境界: レコードのペア 31 要素の `<` は 128 個で受理、32 要素は 132 個で `E1017` と help の全文、同じ 32 要素の `==` は 66 個で受理、手書きのインスタンスを持つレコードで包めば受理、64 要素のタプルは従来どおり help のない `E1017`）。どちらも `210b631a` で失敗します。
   - 指摘 6 のテストのコメント（`wide_unions_have_instances` の「32 members, every one with the four classes」）は、要素が要素の型を共有していることと、異なる型の場合のテストの名前を書き足しました。
   - ドキュメント: `anonymous-union.md`（「持たないクラスとそのほかの規則」に上限の項、型の性質の表）、`diagnostics.md`（`E1017` の行）、`docs/language.md`（インスタンス解決の上限の段落）、`docs/architecture.md`（「組み込みのインスタンス（A26）」）。
2. **`Decode` を導出し `Encode` を手書きした要素は、書いた JSON が別の要素として読まれる**（指摘 2）: 健全性の問題ではなく（読んだ値は匿名共用体の正しい値です）、利用者の非対称なインスタンスの性質なので、文書化しました。`Encode` を見て「決まらない」にする案は採りませんでした: `Decode` の有無が `Encode` のインスタンスに依存することになり、`Encode` を持たない（読むだけの）型や、手書きの `Encode` が object を書く型の `Decode` まで拒否するためです。そのレコードは単独でも往復しません。D7 の「往復」に条件を書き足し、`json.md`・`anonymous-union.md`（例つき）・`docs/language.md` に「種類は `Decode` のインスタンスだけで決まる」と書きました。テスト `decode_reads_the_kind_whatever_encode_wrote`（指摘の d4 の形と、どの要素にも読まれない形、両方を導出した型の往復）が振る舞いを固定します。
3. **具体化の時点の `E1025` が、総称の関数の本体（別のモジュール）を指す**（指摘 3）と、5. **同じ検査が std の中（`std/Json.tz`）を指し、その経路のテストがない**（指摘 5）: 直しました。原因は、型変数を持つ要素に、型変数のままの型に合う `Decode` のインスタンスがないとき（`Pair<'a>` に手書きの `Decode<Pair<i64>>` だけがある）、定義の位置で `Missing` として要素の制約を残し、種類の検査を具体化まで延ばしていたことです。具体化した先で合うインスタンスは一部の型引数だけのもの、つまり手書きのもの（導出のインスタンスの head はすべての型引数について書かれる）で、必ず「種類が決まらない」になります。そこで `JsonKinds::Unsettled` を足し、匿名共用体を書いた位置（総称の関数の本体、導出したインスタンスの文脈）で `E1025`（`... the member Lib.Pair<'a> has no Decode instance for every type of 'a (a derived instance reads a JSON object; an instance for some types only is hand-written and not limited to one kind of JSON value), ...`）にします。推論中の型（`Type::Infer`）を含む要素は、従来どおり解けたときに調べます。`Decode` のインスタンスがない要素は、従来どおり先に `E1005` です。
   - 結果: 指摘 3 の `Lib.tz` は単独で拒否され、診断は `Lib.tz` の `Decode.decode` を指します（呼び出す `Main.tz` があっても同じ位置）。指摘 5 の形は、利用者のソースの `Json.deserialize` を指し、std は指しません。種類は代入で変わらないので、受理された総称の関数の具体化が種類の誤りになることはありません。
   - 具体化の位置へ診断を移す案（`Specializer` の系譜に要求の位置を持たせる）は試して、採りませんでした。std の経路は `instantiate` の `validate` から来ており、上の規則で両方の経路に届かなくなるためです。また、単独では通るのに、どの具体化でも誤りになるライブラリより、定義の位置で拒否するほうが契約として明確です。`union_codec` の調べ直しは防御として残しました（届くプログラムは知られていません。届いても、要素を黙って選ぶ本体は作りません）。
   - 振る舞いの変化: `def read :: Decode<Pair<'a>> => ... (Pair<'a> | i64) ...` のような関数は、一度も具体化されなくても定義の位置で `E1025` です（以前は具体化されるまで受理）。基準（A24）では匿名共用体に `Decode` がなく `E1005` だったので、基準が受理していたプログラムを拒否することはありません。署名の制約で呼び出し側のインスタンスに任せられないことを、制限として `anonymous-union.md`・`json.md`・`docs/language.md`・`diagnostics.md`（`E1025` の行）・`explain E1025`（1,469 文字）に書きました。
   - テスト: `generic_members_need_an_instance_for_every_argument`（指摘 3 の `Lib`・`Main` の 2 モジュールと `Lib` 単独、指摘 5 の std の経路、導出したインスタンスの文脈、型変数 2 つの要素、5 要素のタプル、`E1005` が先に出る例、すべての型引数についてのインスタンスがあれば受理して実行する例を edition 2026 と 2027 で）。`210b631a` で失敗します（`Lib` 単独が受理され、std の経路の診断が `std/Json.tz` にあった）。実装中の判断（Phase 2）の 5 に変更を注記しました。
4. **匿名共用体へのリテラルのパターンが、組み込みの `Eq` で新しく受理されるのに、テストも文書もない**（指摘 4）: 振る舞いは正しい（指摘のとおり）ので、テストと文書を足しました。テスト `literal_patterns_compare_by_eq`（整数・負の数・浮動小数点・文字・文字列のリテラル、OR パターン、タプルとレコードの中、型テストとの混在、関数ガード、ループの要素、NaN と `-0.0`、edition 2026 と 2027 の native と WASM の `-O0`・`-O3`。拒否: 2 つの要素になりうる `1` の `E1041`、要素でない `true` の `E1003`、`Eq` のない要素の `E1005`、網羅しない `missing: _` の `E1021`、`const def` のパターンの位置の `E1026`）。ドキュメント: `anonymous-union.md` に「リテラルのパターン」の節（`run=` の例）、`pattern-matching.md` の表、`match.md` の網羅性の規則、`docs/language.md`。`const def` の中の `E1026` のメッセージは比較演算子のもの（`an operator on a record, tuple, array, or union ...`）のままです。リテラルのパターンは `==` での比較で、位置はパターンを指すので、文言は変えず、その旨を文書に書きました。
5. （指摘 5 は上の 3 に含めました。）
6. **32 要素の匿名共用体の `<` が `E1017` で、参照とテストのコメントが言い過ぎ**（指摘 6）: 上の 1 で直しました（指摘の 32 要素の例は受理され、テストにあります）。
7. **`Format` のテストがない**（指摘 7）: `no_default_for_anonymous_unions` に、`$"{v:x}"` と `Format.format (ref v) "x"` の `E1005`（メッセージの末尾まで照合）と、幅と寄せだけの補間（`$"{text:>5}"` が `"   ab"` など）の実行を足しました。
8. **`anonymous-union.md` の「それ以外のクラスのインスタンスはありません」が実際と違う**（指摘 8）: 直しました。メソッドを持つほかの組み込みクラス（`Default`・`Format`・`Arbitrary`・演算子のクラス）のインスタンスはないこと、メソッドのない `Copy`・`Send`・`Sync`・`Capture` は要素から決まること、プログラムが宣言したクラスも匿名共用体のためのインスタンスは書けないが、すべての型に当てはまるインスタンス（`instance Size<'a>`）は匿名共用体にも当てはまること（確かめました）を書きました。
9. **`index.md` の項目とチケットの「主な影響ファイル」が古い**（指摘 9）: `index.md` の匿名共用体の項目に JSON（`Encode`・`Decode`）を足し、ヘッダの「主な影響ファイル」を実際に変えたファイルに直しました（LLVM の 3 ファイルを外し、`explain E1025`・`json.md`・`diagnostics.md`・`README.md`・`GUIDE.md` を足した）。`node scripts/repo-kb.mjs brief A26` の `affects:` が新しい行を示すことを確かめました。

検証（第1回の対応の後の木）:

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets --locked -- -D warnings` は警告なし。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3090 件がすべて passed、4 skipped です（記録は `target/scratch/a26/r1/t1-b.txt`）。
- `tests/anonymous_unions.rs` は 92 件から 97 件です（新しい 5 件: 上の 1 の 2 件、3 の 1 件、2 の 1 件、4 の 1 件。7 は既存のテストに足した）。
- E2E（dev バイナリ、native と WASM、`-O0` と `-O3`）: `node tests/features.mjs target/debug/tsuzuri <suite>` で `anonymous_unions` 96 件、`deriving` 209 件、`typeclasses` 39 件、`json` 165 件、`dyn_dispatch` 58 件、`hash_map` 199 件、`unions` 27 件が通り、`node tests/json.mjs target/debug/tsuzuri` は 297 入力・2,523 検査が通りました（記録は `target/scratch/a26/r1/suites.txt`）。
- ドキュメント: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変えた 6 ページ（`anonymous-union.md`・`json.md`・`index.md`・`match.md`・`pattern-matching.md`・`diagnostics.md`）に実行し、262 links、39 examples、native の実行 76 回（`-O0`・`-O3`）で通りました。`node scripts/gen-docs.mjs --check` は 19 blocks、0 stale、`node scripts/check-cli-docs.mjs` は 4 checks、0 failed、`node scripts/check-guide-lite.mjs` は 88 references、0 failed です。

### レビューの指摘への対応（第2回）

2026-11-11 の第2回のレビュー（5 件、すべて minor。再現の材料は `target/scratch/review-a26-ce-r2/`）です。5 件とも、指摘のコミット `d0782ed4` のバイナリで再現しました。

既存のテストの期待値の変更（指摘 1 の対応で、変更の前にここへ記録します）:

- `decode_overlap_is_rejected` の「総称の関数」の例 `def f :: Decode<'a> => ... Result<(['a] | Vec<'a>), Json.Error> = \json -> Decode.decode json`: 定義の位置の `E1025` から、定義は受理し、型引数を `i64` に決めて呼ぶ位置で同じ理由の `E1025`（`([i64] | Vec<i64>)`）に変えます。基準（A24）はこの定義を受理していたので、定義の位置で拒否するのは後退です（指摘 1）。
- `generic_members_need_an_instance_for_every_argument`（第1回で足したテスト）: 定義の位置の `E1025`（`JsonKinds::Unsettled`、`has no Decode instance for every type of 'a`）を確かめていた例を、定義は受理し、型が決まる位置（呼び出し・導出したインスタンスを使う位置）で診断する期待値に変えます。`Lib` だけのモジュールは受理、`Main` が `Lib.read` を `Lib.Pair<i64>` で呼ぶと、その呼び出しの位置（`Main.tz`）で `the member Lib.Pair<i64> is not limited to one kind of JSON value` の `E1025`、std の `Json.deserialize` を経由する例も利用者の呼び出しの位置です。導出したインスタンス・型変数 2 つの要素・5 要素のタプルの例と、「型変数のない `char` の要素が先に `E1005`」の例（`(Plain<'a> | char)`。定義の位置の `no instance for Decode<char>`）も、定義は受理し、使う位置で診断します（`char` の例は `Plain` に `Decode` を導出し、呼び出しの位置の `no instance for Decode<char>` にします）。`Unsettled` の文言はなくなります。

対応（コードは `6d6109b1` と `f1b8a75c`（外側の `settle` が予算を戻すことと、内側が残れば外側も残すこと）、テストは `f1b8a75c`、ドキュメントとこの節はその次のコミット）。「`d0782ed4` で失敗する」と書いたテストは、指摘のコミットのソースに新しい `tests/anonymous_unions.rs` を置いて別にビルドし（`target/scratch/a26/r2/prev`）、落ちることを確かめました。

1. **A24 が受理していた総称の関数が、定義の位置で拒否される**（指摘 1）: 直しました。原因は、A26 で匿名共用体のクラスを構造的にしたため、`normalize` が型変数を持つ匿名共用体の制約も要素の制約に展開し、型変数のない要素の制約が満たせない（`(['a] | (i64 -> i64))` の `Eq`）か、`Decode` の種類が重なる・決まらない（`Open`・`Unsettled`）と、定義の位置で誤りにしていたことです。基準（A24）は匿名共用体にインスタンスを持たせず、型変数を持つ制約を、満たすインスタンスがないものとして具体化まで残していました。
   - 変更: `Classes::normalize` の本体を `Classes::reduce`（開始の深さと `Part`、周りで解決中の要件、共有の `Budget`）に移し、型変数を持つ匿名共用体は新しい `Classes::settle` が扱います。`settle` は要素の制約を `reduce` で解き、すべて満たせ、`Decode` なら `decode_members` が `Ok(Some(..))` のときだけ、要素の残りの制約を返します。そうでなければ、匿名共用体の制約をそのまま残します（A24 と同じ形）。残った制約は `specialize` の不動点で呼び出し元へ呼び出しの位置ごと伝わり、型変数が決まった呼び出しで解かれます。内側の匿名共用体が残れば外側も残すので、残した制約を解き直しても同じ結果です（`entails` が、インスタンスの文脈とメソッドが要る制約を同じ形で比べられる）。外側の `settle` は予算の残りで要素を解いて予算を戻すので、その制約は A24 と同じく 1 個と数えます。`JsonKinds::Unsettled` とその文言は削りました。
   - 結果: 指摘の `un1`・`g3`・`g5_2`・`g5_3`・`g6_1`・`g6_2`・`g6_4` は、呼び出さなければ受理されます（基準と同じ）。呼び出すと、呼び出しの位置で診断します: `un1` を `Plain<i64>` で呼ぶと `no instance for Decode<Main.Plain<i64>>`（指摘 1(2) の誤った理由の文言はなくなり、D7 の「`Missing` を先に」のとおり `E1005`）、`g3` を `Pair<i64>` で呼ぶと `the member Main.Pair<i64> is not limited to one kind of JSON value` の `E1025`（指摘 1(3)）、`f1` は `no instance for Eq<i64 -> i64>`。総称の関数を経由しても（`outer` が `dec4` を呼ぶ）、型を決めた呼び出しを指します。
   - 第1回の指摘 3・5（診断が `Lib.tz` や `std/Json.tz` を指す）は、別の形で解決したままです: 残るのが要素の制約ではなく匿名共用体の制約なので、呼び出し元へ伝わって、`Main.tz` の `Lib.read` と、利用者の `read`（中で `Json.deserialize` を呼ぶ関数）の呼び出しを指します。`union_codec` の調べ直しは防御として残しました。
   - 同じ原因で、ほかに 2 つの基準が受理していた定義が拒否されていたので、テストに入れました: `record R<'a> { v: (['a] | (i64 -> i64)) } deriving (Eq)`（`d0782ed4` は `E1025`。使う位置の `a == b` で `E1005`）と、`instance Eq<(Box<['a]> | i64)> => Eq<Box<'a>>`（文脈が要素の中で育ち、`d0782ed4` は宣言の位置で `E1017`。使う位置では従来どおり `E1017`）。
   - タプルとの違い: タプルは、型変数のない部分が満たせなければ従来どおり定義の位置で `E1005` です（変えていません）。匿名共用体は A24 との互換を優先し、満たせないときは呼び出しまで残します。言語リファレンスと `docs/language.md` に書きました。
   - 第1回の節の 3 の「基準（A24）では匿名共用体に `Decode` がなく `E1005` だったので、基準が受理していたプログラムを拒否することはありません」は誤りでした（A24 の `E1005` は呼び出しの位置だけで、呼び出さない定義は受理していた）。この対応で、それらの定義は再び受理されます。
   - 残る違い（極端な場合だけ）: 型変数を持つ匿名共用体を要素の制約に分けると、関数の制約は要素の残りの制約の数だけになります（A24 では匿名共用体ごとに 1 個）。いくつかの匿名共用体の要素に、インスタンスのない型変数つきのレコードを合わせて 128 種類より多く並べると、1 つの関数の制約の上限（`polymorphic_constraint_limit`、128 個）に先に届きます。
   - テスト: `constraints_with_type_variables_wait_for_their_types`（受理する 9 つの定義、呼び出しの位置の 6 つの診断の全文と位置、インスタンスの文脈が要素の制約を満たして実行する例）。`d0782ed4` で失敗します。既存の 2 つのテストの期待値は、上に記録したとおりに変えました。
2. **`==` の被演算子にした非 Copy の変数が move され、help の `ref s` も `E1012`**（指摘 2）: 注入は値の使用で、非 Copy の値は move する、というのは利用者が承認した D-62 の決定 3（A24 の D10）なので、比較の意味は変えませんでした（被演算子を借用してタグとペイロードを直接比べる案は、この決定を変えるので採りませんでした）。直したのは help と文書です。`src/ownership.rs` の検査器が、匿名共用体の `Construct` で span がペイロードと同じもの（注入）のペイロードの span を集め（`Checker::injections`）、注入で move した値を後で使ったときの `E1012` の help を `an anonymous union holds its value, so injecting 's' moved it; use 's' before that, or inject a copy (clone_string ref s)` にします（`utf8string` は `Utf8String.clone ref s`、ほかの型は複製の書き方を言わない、フィールドは `injecting the field moved it out of 'r'`）。`let v: (i64 | string) = s` のようなほかの注入も同じ help です。`anonymous-union.md` の「比較と順序」に move されることと、複製を比べる・型テストで取り出してから比べる例（`run=`）を、「所有権」に比較の注入も同じだと書き、`docs/language.md` と `docs/architecture.md` にも書きました。テスト `injected_operands_move_with_their_own_help`（`string`・`utf8string`・`[string]`・`let` の注入・フィールドの help の全文、`ref s` の `E1012`、Copy の被演算子と回避の方法を native と WASM で実行）。`d0782ed4` で失敗します。
3. **結果式が匿名共用体のプログラムが、値を出力するようになった**（指摘 3）: 意図した変化として文書化しました。結果式は `Display` を持つ型の値を出力する規則（`src/check.rs` の `entry_result`、`modules.md` の「トップレベル式による形式」）で、A26 で匿名共用体が `Display` を持ったので、その規則どおりに出力します（A24 では `Display` がなく、値は捨てられていた）。コードは変えていません。`anonymous-union.md` の「表示」と `docs/language.md` に書き、テスト `top_level_union_result_is_shown`（`pick 0` が `-O0` と `-O3` で `none`）が振る舞いを固定します（振る舞いは変えていないので、このテストは `d0782ed4` でも通ります）。
4. **`E1017` の help が、匿名共用体が原因でなくても付く**（指摘 4）: 直しました。`reduce` は要件の出どころ（`Part::Outside`・`Member`・`Inside`）を持ち、要素の要件が導く要件（`Inside`）を `Budget::inside` に数えます。help は、深さの上限ではなく、数えた要件の半分より多くが `Inside` のときだけ付けます。指摘の `lim1`（63 個のレコードと `(i64 | string)` のタプル）と `pr1`（深さ）は help なし、第1回の 32 要素のレコードのペアは従来どおり help ありです。`distinct_parts_of_members_meet_the_obligation_limit` に 2 例を足しました（`d0782ed4` で失敗）。`anonymous-union.md`・`diagnostics.md`・`docs/language.md`・`docs/architecture.md` の条件を直しました。
5. **導出したインスタンスの文脈が満たせない要素で、`E1005` より先に重なりの `E1025`**（指摘 5）: 直しました。具体的な匿名共用体の `Decode` の重なりは、`reduce` の `overlaps` に（そのときの `pending` の長さと一緒に）積み、要素の制約とその先をすべて解いた後で返します。要素の誤りが先に出るので、`(Box<char> | Pt)` も `(Box<char> | i64)` と同じ `no instance for Decode<char>` です。`decode_overlap_is_rejected` の「`E1005` が先」に 2 例を足しました（`d0782ed4` は `(Box<char> | Pt)` で `E1025`）。

更新したドキュメント（第2回）: `anonymous-union.md`（「比較と順序」の move と `run=` の例、「表示」の結果式、「持たないクラスとそのほかの規則」の上限の help の条件と、型変数を持つ匿名共用体の制約の項、「Decode」の型変数を持つ要素、「所有権」）、`json.md`（制限の項）、`diagnostics.md`（`E1012`・`E1017`・`E1025` の行）、`docs/language.md`（インスタンス解決の上限、匿名共用体の `Decode`、型変数を持つ匿名共用体の制約・注入の move・結果式の段落）、`docs/architecture.md`（注入、組み込みのインスタンスの予算と `reduce`・`settle`、`Decode` の条件）、`src/explain/codes/E1025.md`（1,465 文字）。

検証（第2回の対応の後の木）:

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets --locked -- -D warnings` は警告なし。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3093 件がすべて passed、4 skipped です（記録は `target/scratch/a26/r2/t1.txt`）。ドキュメントを変えた後に、文書を読むテスト（`explain`・`property`・`std_examples`・`value_layout`・`diagnostic_kinds`・`diagnostic_v2`・`mcp`）の 140 件を再び実行し、すべて passed です。
- `tests/anonymous_unions.rs` は 97 件から 100 件です（新しい 3 件: 上の 1・2・3。4・5 は既存のテストに足し、1 の既存の 2 件は期待値を上に記録したとおりに変えた）。新しいテストと足した例のうち、指摘 3 の固定のテストを除く 5 件は `d0782ed4` で失敗します（`target/scratch/a26/r2/prev-tests.txt`）。
- E2E（dev バイナリ、native と WASM、`-O0` と `-O3`）: `node tests/features.mjs target/debug/tsuzuri <suite>` で `anonymous_unions` 96 件、`deriving` 209 件、`typeclasses` 39 件、`json` 165 件、`dyn_dispatch` 58 件、`hash_map` 199 件、`unions` 27 件が通り、`node tests/json.mjs target/debug/tsuzuri` は 297 入力・2,523 検査が通りました（記録は `target/scratch/a26/r2/suites.txt`）。
- 既存の IR: `tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を、`d0782ed4` のバイナリと今のバイナリで取り、1 byte も違いません（`tests/fixtures/anonymous_unions` を含む。`sh target/scratch/a26/baseline.sh` と `compare.sh`、記録は `target/scratch/a26/r2/ir-compare.txt`。IR は消しました）。制約の解き方を変えても、受理していたプログラムの生成コードは変わりません。
- ドキュメント: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変えた 3 ページ（`anonymous-union.md`・`json.md`・`diagnostics.md`）に実行し、96 links、29 examples、native の実行 56 回（`-O0`・`-O3`）で通りました。`node scripts/gen-docs.mjs --check` は 19 blocks、0 stale、`node scripts/check-cli-docs.mjs` は 4 checks、0 failed、`node scripts/check-guide-lite.mjs` は 88 references、0 failed です。

### A25 との統合（2026-11-11）

調整役の決定（A24 ← A25 ← A26 の順に積む）に従い、`feat/a25-anonymous-union-conversions`（先端 `8f14b16c`。A24 の第2回のレビューの対応 `09fdb0de` を含む）をこのブランチへ取り込みました（マージコミット `550d4715`）。履歴は書き換えていません。

衝突した 13 ファイルは、両方のチケットの振る舞いを残して解きました。

- `src/ownership.rs`（ムーブ後の使用の `E1012`）: A25 の「照合した一時値の 2 つの束縛」のメッセージと、拡大の help（`a widening moves 'w'; bind the wider value once ...`）を残し、その次に A26 の注入の help（`an anonymous union holds its value, so injecting 's' moved it; ...`）を調べます。拡大を先に調べるのは、拡大した値には拡大した値を 1 回だけ束縛する案内が当たり、「複製を注入する」は当たらないためです。1 つの被演算子 `v == (if c then w else s)` の中でも、拡大した `w` と注入した `s` はそれぞれの help です（下のテスト 1）。
- `src/anonymous_union.rs`: A26 の組み込みのインスタンスの部品（`tag_behind`・`member_call`・`by_member`）と JSON の種類（`JsonKind`・`JsonKinds`・`decode_body`）、A24 の第2回の `Fit` と `holds_array_literal` を、どちらも残しました。`src/check.rs`・`src/control.rs`・`src/polymorph.rs`・`src/diagnostic_kinds.rs` は自動で併合され、診断の kind は各コードの中のアルファベット順のままです（A25 の `anonymous_union_not_subset`（`E1003`）と `type_test_narrow_borrowed`（`E1041`）、A26 の `anonymous_union_decode_overlap`（`E1025`））。`src/explain/codes/E1041.md`（A25 が変えた）は 1,481 文字、`E1025.md`（A26 が変えた）は 1,465 文字で、上限の 1,500 文字に収まります。
- `tests/anonymous_unions.rs`: 共通の部分（両方の途中の変更は自動で併合）の後に、A26 が足したテスト、A25 と A24 の第2回が足したテストの順に置きました。69 + 31 + 40 = 140 件で、下の 3 件を足して 143 件です。
- `tests/features.mjs` とフィクスチャ: 両方の関数と JavaScript の参照値を残し、フィクスチャの冒頭の説明は A24・A25・A26 を名指しします（suite `anonymous_unions` は 96 + 56 = 152 件）。
- ドキュメント: `README.md` の機能の項目に拡大・絞り込みとインスタンスの両方を書き、テストの表の行は両方のファイルとテストを並べました。`_features/README.md` は A25 の行（A25 の版、`_completed/` へのリンク）と A26 の行（このブランチの版、`_completed/` へのリンク）を残し、A24 の行の「まだない」を「変換は A25 が、組み込みのインスタンスは A26 が足した」に直しました。`anonymous-union.md`（「所有権」の比較の注入と A25 の絞り込みのビュー、「注意点」は A26 の 2 項目と A25 の 2 項目。A25 が消した「要素の少ない共用体を渡せない」の項目と例は消したまま）、`diagnostics.md`（`E1012` の行は A25 の文に A26 の注入の help の文を足し、`E1013` の行は A25 の版）、`index.md`、`match.md`（A25 の絞り込みの数え方と A26 のリテラルのパターンの両方）、`docs/architecture.md`（モジュールの表の行は A24・A25・A26、注入の段落は A24 の第2回の版に A26 の `injections` の文を足した）、`docs/language.md`（`E1041` の行は A25 の版、`E1025` の行は A26 の版）。

監査が名指しした相互作用を確かめ、3 件のテストを足しました。期待値はどれも規則から手で求めたもので、ハッシュの 2 つの値は、`HASH_OF_1`・`HASH_OF_ONE` を再現する Python のスクリプト（`docs/language.md` の Canonical Stream から書いたもの）で計算しました。3 件とも、A25 のバイナリでは `E1005`（インスタンスがない）です。取り込む前の A26 には拡大がないので、拡大する位置が `E1003` です（拡大のない A24 のバイナリで確かめました）。

1. `comparisons_take_injected_and_widened_operands`: 比較の被演算子は拡大しません（A25 の `comparison_does_not_widen` はそのまま通ります。`v == w`・`w == v`・`v == (w)` は `E1003`）。右の被演算子は注入され（A24）、同じ型の 2 つの値は A26 の `Eq`・`Ord` が借用して比べるので、どちらも後で使えます。被演算子に書いた `if` の枝は拡大し（`v == (if positive 1 then w else 0)`）、拡大した値のタグは広い共用体の正準順序での位置です: `(i64 | string)` から拡大した `5` はどの `f64` の値よりも小さく（`wide < 0.5`）、`"abc"` は大きい。拡大した `w` を後で使うと拡大の help、同じ `if` で注入した `s` を後で使うと注入の help です。
2. `widened_and_narrowed_keys_hash_as_their_union`: 型注釈のある束縛で拡大したキーで `HashSet<(f64 | i64 | string)>` と `Map<(f64 | i64 | string), i64>` を作り、直接作ったキーで見つかること、`Map` の順が `0=3;2=2;0.5=9;s1=2;`（i64、f64、string の順）であること、拡大した `"s1"` のハッシュが要素 2 の値 `16237190552417262429`、もとの `(i64 | string)` の `"s1"` が要素 1 の値 `1096041482089778054` であることを確かめます。絞り込みは、所有する一時値から Copy でない要素を持つ `(i64 | string)` へ（`HashMap`）、借用した `HashSet` の要素から Copy の要素だけの `(i64 | bool)` へ（`HashSet`）絞ったキーが、直接作ったキーで見つかることを確かめます。総称の関数の引数は拡大しないので（`HashSet.insert w set` は `E1003`）、キーは束縛で拡大します。
3. `widened_and_narrowed_values_show_and_encode`: 拡大した値と絞った値の `Display`（単独・補間・配列の中・導出したレコードの中）と `Encode`（配列・導出したレコード）がもとの値と同じで、`Decode` で読み戻したレコードが `==` で等しいことを確かめます。

どれもコードの変更は要りませんでした（拡大と絞り込みはタグを付け替えてペイロードを移す型付きの `match` で、インスタンスはタグとペイロードを読むだけなので、互いに知る必要がありません）。ドキュメントに、相互作用の規則を足しました: `anonymous-union.md`（「比較と順序」に被演算子は拡大しないことと `if` の枝の拡大、拡大した値の順序。「表示」と「Encode」に拡大・絞った値はもとの値と同じ。「コレクションのキーと `deriving`」に総称の関数の引数は拡大しないこと、拡大・絞ったキーのハッシュと順序、`run=` の例）、`docs/language.md`（匿名共用体の節に 1 段落）。

検証（取り込みの後の木）:

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets --locked -- -D warnings` は警告なし。
- T1: マージコミット `550d4715` の木で `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` を実行し、3133 件がすべて passed、4 skipped です（記録は `target/scratch/a26/m/t1-merge.txt`）。その後に足した 3 件を含む `--test anonymous_unions` は 143 件が passed、ドキュメントを変えた後の文書を読むテスト（`explain`・`property`・`std_examples`・`value_layout`・`diagnostic_kinds`・`diagnostic_v2`・`mcp`・`guide`・`mcp_knowledge`）は 190 件が passed です。
- E2E（dev バイナリ、native と WASM、`-O0` と `-O3`）: `node tests/features.mjs target/debug/tsuzuri <suite>` で `anonymous_unions` 152 件、`deriving` 209 件、`typeclasses` 39 件、`json` 165 件、`dyn_dispatch` 58 件、`hash_map` 199 件、`unions` 27 件が通りました（記録は `target/scratch/a26/m/suites.txt`）。
- IR: `tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を、A25 の先端のバイナリと取り込んだ後のバイナリで取りました（`sh target/scratch/a26/baseline.sh` と `compare.sh`、記録は `target/scratch/a26/m/ir-compare.txt`）。違うのは `tests/fixtures/anonymous_unions` だけで、A25 のバイナリは A26 の関数を持つフィクスチャを `E1005` で拒否します。ほかの 185 単位は 1 byte も違いません。A25 の先端のフィクスチャ（`feat/a25-anonymous-union-conversions:tests/fixtures/anonymous_unions/Main.tz`）の native と wasm32 の IR も、2 つのバイナリで同じです。A26 は、匿名共用体のインスタンスを使わない A25 のプログラムの生成コードを変えません。IR は消しました。
- ドキュメント: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を、取り込みで変わった 7 ページ（`anonymous-union.md`・`diagnostics.md`・`index.md`・`match.md`・`pattern-matching.md`・`type-inference.md`・`types.md`）に実行し、318 links、42 examples、native の実行 82 回（`-O0`・`-O3`）で通りました（記録は `target/scratch/a26/m/check-docs.txt`）。`node scripts/gen-docs.mjs` は 19 blocks、0 updated、`--check` は 0 stale、`node scripts/check-cli-docs.mjs` は 4 checks、0 failed、`node scripts/check-guide-lite.mjs` は 88 references、0 failed、`node scripts/check-links.mjs` は 0 broken です。

### Copilot のレビューへの対応（第1回）

PR #104 に GitHub Copilot が付けたインラインの指摘 3 件です。3 件とも、指摘の時点の先端 `e8ac00b3` のバイナリで再現しました（再現の材料は `target/scratch/a26/copilot1/`）。新しいテストと足した例は、`src/` だけを `e8ac00b3` に戻した木で落ちることを確かめました（記録は `target/scratch/a26/copilot1/prev-tests.txt`）。既存のテストの期待値は変えていません。

1. **`(i64 | [i64])` の `Decode.decode` が、読んだ配列を要素ごとに複製する**（`src/anonymous_union.rs` の `decode_body`）: 直しました。原因は、合成した束縛（腕ごとの `$value` と `$error`）がすべて同じ span を持っていたことです。`LastUses::record_bindings` は束縛の span ごとに 1 つの move の判断を持ち、同じ span の判断を「かつ」でまとめるので、Copy でない `Json.Error` の束縛（判断は常に「複製」）が配列の束縛の move を消していました。`-O0` の IR で、配列の腕が新しい領域を `@tz.alloc` で取り、要素を 1 つずつ写し、読んだ配列を解放していました（`target/scratch/a26/copilot1/f1/Main.tz` を `--emit llvm -O0` にした IR）。利用者にも見える結果として、配列を直接読むより 1 回多く割り当て（`Json.deserialize` で `[1, 2, 3]` を読むと、`Alloc` で 13 回と 12 回）、`Decode.decode` を直接呼ぶ `@no_copy` の関数が `E1031` で拒否されていました。`Json.deserialize` だけを通して読む `@no_copy` の関数は、修正の前から拒否されていません: `copy` の効果は利用者のモジュールの複製だけを数え（`W1006` と同じ）、std の `Json.deserialize` だけが求めた `$intrinsic.Decode.decode.N` の本体は std の本体なので、その中の複製を数えませんでした。同じ型の `Decode.decode` を利用者の関数も直接呼ぶプログラム（テストの `numbers` と `whole`）では、2 つが 1 つの本体を共有し、それが利用者のモジュールの本体になるので、`Json.deserialize` を通す関数も `E1031` でした（定義の順序を入れ替えても同じです）。
   - 変更: `Checker::retag`（A25）と同じく、Copy で解放の要る型（配列や、配列を持つ Copy のレコード）の束縛だけが本体の span を持ち、ほかの束縛（`Json.Error`、数、文字列）は span の終わりの空の span を持ちます。判断を読むのは前者だけなので、配列の腕は要素の結果から move します。Copy で解放の要る要素が 2 つある `(Row | [f64] | string)` では、その 2 つが span を共有し、どちらも move です（どちらかが複製になっても、複製になるだけで安全です）。
   - テスト `decode_moves_array_members_out_of_the_result`（edition 2026 と 2027）: `@no_copy` の `Decode.decode`（`numbers`・`table`。`e8ac00b3` では `E1031`）と `Json.deserialize`（`whole` とそれを呼ぶ `through_union`。`e8ac00b3` では、`numbers` と本体を共有するときだけ `E1031`。`Json.deserialize` だけの経路は割り当ての回数が示します。`plain` と `direct` は配列を直接読む参照で、修正の前から受理）、生成した 2 つの `$intrinsic.Decode.decode.N` の本体に `@tz.alloc` がないこと、`(i64 | [i64])` に `[1, 2, 3]` を読む割り当ての回数が `[i64]` を読む回数と同じで、解放の回数と等しいこと（native と WASM、`-O0` と `-O3`。参照は配列を直接読む同じ形の関数）、読んだ値（`1 + 2 + 3`、空の和、数そのもの、`4 + 5`、2 つの数の 10 倍、`"abc"` の長さ、どの要素も読まない値の誤り）。
   - 要求する位置が違っても（導出したレコードのフィールド、関数値としての `Decode.decode`）、生成した本体に割り当てがないことを確かめました。
2. **入れ子の重なりの `E1025` が、ほかの要素の `E1005` より先に出る**（`src/polymorph.rs` の `Classes::reduce`）: 直しました。`Json.deserialize` で `(char | [(i64 | f64)])` を読むと、外側の匿名共用体は `char` が `Missing` なので要素の制約へ進みますが、`pending` は積んだ逆の順に取り出すので、正準順で後ろの `[(i64 | f64)]` が先に解かれ、その中の重なりが、要素の制約が終わった時点（積んだときの `pending` の長さ）で返り、`Decode<char>` はまだ解かれていませんでした。結果は要素の正準順に依っていました（`char`・`unit`・レコードは `E1025`、関数・5 要素のタプルは `E1005`）。D7 の「`Missing` を先に」に反します。
   - 変更: `reduce` は最初の重なりを `overlap` に取っておき、その解決のほかの要件をすべて解いた後で返します（`pending` の長さの印は削りました）。ほかの誤りが先に出るので、匿名共用体の中でも、隣の要件（タプルの要素）でも、要素の順序に関わらず `E1005` が先です。重なりだけがあれば、従来どおり同じ `E1025` です。`settle` の中の重なりは、どの誤りでも匿名共用体の制約を残すだけなので、順序は結果を変えません。
   - テスト: `decode_overlap_is_rejected` の「`E1005` が先」に 6 例（`(char | [(i64 | f64)])`・`(unit | [(i64 | f64)])`・`(Plain | [(i64 | f64)])`・`((i64 * i64 * i64 * i64 * i64) | [(i64 | f64)])`・`(char * (i64 | f64))`・`((i64 | f64) * char)`）、重なりの例に 2 例（`(string | [(i64 | f64)])`・`((i64 | f64) * string)` は `(i64 | f64)` の `E1025`）。`e8ac00b3` は最初の例で `E1025` です。5 要素のタプルの例は `e8ac00b3` でも `E1005` で、順序に依らないことを示すために置きました。関数の要素の例（`((i64 -> i64) | ...)`）は、`let` の型注釈に関数型を書くと匿名共用体がなくても先に `E1013`（`borrow requires a local place`）になるので、5 要素のタプルにしました。
3. **型変数を持つ匿名共用体の要素が、共通の要件を要素ごとに数え直す**（`src/polymorph.rs` の `Classes::settle`）: 直しました。`settle` は予算を要素の間で共有していましたが、要素ごとの `reduce` が解いた要件の集合（`completed`）と残す要件（`seen`・`residual`）を新しく作っていたので、いくつかの要素が要る同じ要件を要素の数だけ数えていました。指摘の 16 要素 `(Pk * [[[[['a]]]]])`（`Pk` は `i64` のフィールドに `Eq` を導出）は、要素ごとに `Eq<Pk>`・`Eq<i64>`（導出したインスタンスの文脈）・5 つの配列・`Eq<'a>` の 8 個で、1 + 16 × 8 = 129 > 128 のため匿名共用体の制約が残り、`instance Eq<'a> => Eq<Wrap<'a>>` が `E1027` でした（15 要素は 121 で受理、型変数のない同じ 16 要素も受理）。
   - 変更: `Resolution`（`completed`・`seen`・`residual`）を足し、`reduce` はそれに書き込みます。外側の `settle` は新しい `Resolution` を作り、その要素と、要素の中の `settle`（`Budget::settling`）はすべてそれを共有します。すべての要素が満たせたときだけ残す要件を呼び出し元の `Resolution` へ移し、そうでなければ捨てて、匿名共用体の制約を残します（従来どおり）。閉路の検査は変わりません: 要素はどれも同じ `ancestors` から始まり、解いた要件が仮定した解決中の要件は、満たせなければ誤りで全体を捨てるので、解いた要件は仮定がすべて満たせたときにだけ残ります（1 つの `reduce` の中で `completed` を共有するのと同じ理由）。外側の `Resolution` へは解いた要件を移さないので、失敗した `settle` が外側の要件を「解いた」ことにすることはありません。
   - テスト `members_with_type_variables_share_their_obligations`: 指摘の 16 要素の形（インスタンスと、署名に `Eq<'a>` を書いた総称の関数。`==`・`!=` の値）、異なる要件の境界（`(Pk * [[Qk<'a>]])` は要素ごとに 4 個で、`Eq<i64>` と `Eq<'a>` を共有する。31 要素は 1 + 4 × 31 + 2 = 127 で受理して実行、32 要素は 131 でインスタンスの `E1027`）、要素が `[Tree<'a>]` を共有する再帰する総称のレコード（`deriving (Eq, Ord)`。導出したインスタンスへの閉路。同じ要素の中の比較と、違う要素の値）。`e8ac00b3` は 16 要素で `E1027` です。数は `reduce` の数え方（要素そのものは数えず、要素の中の要件は 1 個ずつ）から手で求めました。
   - 総称の関数（`def`）は、要件が上限を超えても制約を匿名共用体のまま呼び出し元へ伝えるので、修正の前も後も受理します（32 要素で確かめました）。上限が見えるのはインスタンスの文脈の検査です。

更新したドキュメント: `anonymous-union.md`（「持たないクラスとそのほかの規則」に、型変数を持つ匿名共用体でも要素の中の型を 1 個と数えることと上限を超えたときの `E1027`、「Decode」に `E1005` が先であることの範囲と、値を複製せずに移すこと）、`json.md`（制限の `E1005` の項）、`docs/language.md`（匿名共用体の `Decode` の `E1005` の範囲と move、型変数を持つ匿名共用体の制約の段落）、`docs/architecture.md`（`reduce` と `Resolution`、`settle` の共有と閉路、`overlap`、`decode_body` の束縛の span）。

検証（この対応の後の木）:

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets --locked -- -D warnings` は警告なし。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3138 件がすべて passed、4 skipped です（記録は `target/scratch/a26/copilot1/t1.txt`）。ドキュメントを変えた後に、文書を読むテスト（`explain`・`property`・`std_examples`・`value_layout`・`diagnostic_kinds`・`diagnostic_v2`・`mcp`・`guide`・`mcp_knowledge`）と `anonymous_unions` の 335 件を再び実行し、すべて passed です。
- `tests/anonymous_unions.rs` は 143 件から 145 件です（新しい 2 件: 上の 1 と 3。2 は `decode_overlap_is_rejected` に 8 例を足した）。3 件とも `src/` を `e8ac00b3` に戻した木で失敗しました: 1 は `E1031`（`'Main.numbers' is marked '@no_copy' but this expression implicitly copies every element`）、2 は `(char | [(i64 | f64)])` が `E1025`、3 は 16 要素のインスタンスが `E1027` です。
- E2E（dev バイナリ、native と WASM、`-O0` と `-O3`、native は host の `live == 0`、WASM は import なし）: `node tests/features.mjs target/debug/tsuzuri anonymous_unions` は 152 件が通りました（記録は `target/scratch/a26/copilot1/suite.txt`）。
- 既存の IR: `tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を、`e8ac00b3` のソースから別にビルドしたバイナリと今のバイナリで取りました（`sh target/scratch/a26/baseline.sh` と `compare.sh`、記録は `target/scratch/a26/copilot1/ir-compare.txt`）。違うのは `tests/fixtures/anonymous_unions` の native と wasm32 の IR だけで、その中でも違う関数は、配列の要素を持つ匿名共用体の `$intrinsic.Decode.decode.N` の 1 つだけです（`@tz.alloc` が 1 から 0）。ほかの 185 単位は 1 byte も違わず、匿名共用体のないプログラムの出力は変わりません。IR とビルドした木は消しました。
- ドキュメント: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を変えた 2 ページ（`anonymous-union.md`・`json.md`）に実行し、54 links、33 examples、native の実行 64 回（`-O0`・`-O3`）で通りました（記録は `target/scratch/a26/copilot1/check-docs.txt`）。`node scripts/gen-docs.mjs --check` は 19 blocks、0 stale、`node scripts/check-cli-docs.mjs` は 4 checks、0 failed、`node scripts/check-guide-lite.mjs` は 88 references、0 failed、`node scripts/check-links.mjs` は 0 broken です。

検証者の指摘（記録の訂正）: 上の 1 の記録は、修正の前に `Json.deserialize` も `@no_copy` の関数で `E1031` になったと書いていました。`e8ac00b3` を `git archive` で取り出してビルドしたバイナリで確かめると（材料は `target/scratch/a26/copilot1/v1/`）、`Json.deserialize` だけを通して `(i64 | [i64])` を読む `@no_copy` の関数（`whole/Main.tz`）は `check` も `run` も通り、その `$intrinsic.Decode.decode.N` の `-O0` の IR には要素を写す `@tz.alloc` が 1 つありました（今のバイナリでは 0）。`E1031` になるのは `Decode.decode` を直接呼ぶ関数（`decode/Main.tz`）と、それと同じ本体を共有する関数です。テストのプログラムでは `numbers`・`table`・`whole`・`through_union` の 4 つが `E1031` で、`whole` を `numbers` の前に置いても同じでした（`order/`・`order2/`）。割り当ては、`numbers` のない `Json.deserialize` だけのプログラム（`alloc/Main.tz`、`--allocator counting`）で `-O0` と `-O3` とも 13 回と 12 回、今のバイナリで 12 回と 12 回です。1 の記録とテストの項目、テストのコメントをこの結果に合わせて直しました。ソースとテストの期待値は変えていません（テストは `numbers`・`table` の `E1031`、本体の `@tz.alloc`、割り当ての回数で、修正の前の木で落ちます）。利用者向けの文書の「`@no_copy` の関数から呼べます」は、修正の後のどちらの経路にも当てはまります。

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets --locked -- -D warnings` は警告なし。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3138 件がすべて passed、4 skipped です（記録は `target/scratch/a26/copilot1/v1/t1.txt`）。
- E2E: `node tests/features.mjs target/debug/tsuzuri anonymous_unions` は 152 件が通りました（native と WASM、`-O0` と `-O3`、記録は `target/scratch/a26/copilot1/v1/suite.txt`）。
- 言語リファレンスのページは変えていないので、`check-docs.mjs` は実行していません。

### A25 の再取り込み（Copilot のレビューの第1回の後、2026-11-12）

A24 と A25 が Copilot のレビューの第1回に対応したので、`feat/a25-anonymous-union-conversions` の先端 `941e45a7`（A25 の `c7f0b207`〜`fd622aa0` と、A24 の `907e5e8c`・`b9a31cce` を取り込んだもの）を、もう一度このブランチへ取り込みました（マージコミット `15f322e2`）。履歴は書き換えていません。前の取り込み（`8f14b16c`）から A25 の側で変わったのは `src/check.rs`・`src/control.rs`（Copy の要素への絞り込みのビューと手順の形）・`src/kg/find.rs`（`Subst::solve` の後戻りの探索）・`src/ownership_control.rs` で、A26 はこれらを変えていないので、コードは自動で併合されました。

- 衝突は `docs/architecture.md` の「ほかの段。」の段落の 1 か所です。A26 の側はこの段落を変えておらず（`8f14b16c` と同じ文）、A25 の側は `kg/find.rs` の説明を書き直していました。直前の A26 の「組み込みのインスタンス（A26）」の段落はそのまま残し、「ほかの段。」は A25 の版にしました。
- 自動で併合したもの: `tests/anonymous_unions.rs`（A25 の 9 件を足して 145 件から 154 件）、`tests/features.mjs` とフィクスチャ（A25 の `narrow_copy` と `narrow_generic`。suite `anonymous_unions` は 152 件から 169 件）、`anonymous-union.md`（「`:? (A | B)` で絞る」の複製の回数）、`kg.md`（問い合わせの `*` と `->` の結合）、`docs/language.md`（絞り込みの 2 項目）、A24 と A25 のチケット。
- この取り込みは `README.md`・`_features/README.md`・`src/diagnostic_kinds.rs`・`src/explain/codes/` を変えません。各チケットの行（`_completed/` へのリンク）と診断の kind の順はそのままで、`E1041.md` は 1,481 文字、`E1025.md` は 1,465 文字です（上限 1,500 文字）。

検証（取り込みの後の木）:

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked`）: 3154 件がすべて passed、4 skipped です（記録は `target/scratch/a26/copilot1/nextest-restack.log`）。
- E2E: `node tests/features.mjs target/debug/tsuzuri anonymous_unions` は 169 件が通りました（native と WASM、`-O0` と `-O3`、記録は `target/scratch/a26/copilot1/restack/suite.txt`）。
- IR: `tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を、`941e45a7` を `git archive` で取り出してビルドしたバイナリと取り込んだ後のバイナリで取りました（`sh target/scratch/a26/baseline.sh` と `compare.sh`、記録は `target/scratch/a26/copilot1/restack/ir-compare.txt`）。違うのは `tests/fixtures/anonymous_unions` だけで、A25 のバイナリは A26 の関数を持つフィクスチャを `E1005` で拒否します。ほかの 185 単位は 1 byte も違いません。`941e45a7` のフィクスチャの native と wasm32 の IR も、2 つのバイナリで同じです。IR とビルドした木は消しました。
- ドキュメント: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を取り込みで変わった 2 ページ（`anonymous-union.md`・`kg.md`）に実行し、45 links、21 examples、native の実行 40 回（`-O0`・`-O3`）で通りました（記録は `target/scratch/a26/copilot1/restack/check-docs.txt`）。`node scripts/gen-docs.mjs --check` は 19 blocks、0 stale、`node scripts/check-cli-docs.mjs` は 4 checks、0 failed、`node scripts/check-guide-lite.mjs` は 88 references、0 failed、`node scripts/check-links.mjs` は 0 broken です。

### A25 の再取り込み（Copilot のレビューの第2回の後、2026-11-12）

A24 と A25 が Copilot のレビューの第2回に対応したので、`feat/a25-anonymous-union-conversions` の先端 `e2135291`（A25 の `c732f5b0`・`26f041ff` と、A24 の `66e29dd2`・`1cf9565c` を取り込んだもの）を、もう一度このブランチへ取り込みました（マージコミット `2a7d0c32`）。履歴は書き換えていません。前の取り込み（`941e45a7`）から A25 の側で変わったコードは `src/api_diff.rs`（要素を並べ替える鍵を 1 回だけ作る）・`src/check.rs`（`@old (x)` の注入、`RETAGGED_PREFIX` と `Local::retags_payload`）・`src/control.rs`（拡大の束縛の名前に `RETAGGED_PREFIX`）・`src/last_use.rs`（Copy で解放の要る型でない拡大の束縛は move の判断に加わらない）・`src/parse_control.rs`（型テストを探す走査は、括弧の中の `->` で止まらない）で、A26 はこれらを変えていないので、コードは自動で併合されました。

- 衝突は 2 か所です。`docs/architecture.md` の「ほかの段。」の段落は、A26 の側が変えておらず（`941e45a7` と同じ文）、A25 の側が `api_diff.rs` の説明を書き直していたので、直前の A26 の「組み込みのインスタンス（A26）」の段落とその項目をそのまま残し、「ほかの段。」は A25 の版にしました。`tests/anonymous_unions.rs` の末尾は、両方がテストを足していました。A25 の版の順（A25 の最後のテスト `hover_on_a_narrowed_binder_shows_the_narrowed_union` の直後に A24 の `old_values_are_injected`・`arrows_in_annotations_do_not_hide_type_tests`）を残し、その後に A26 が最初の取り込みで足した 3 件を置きました。
- 自動で併合したもの: `tests/anonymous_unions.rs` のほかの 2 件（`api_diff_reorders_members_that_share_region_names`・`widening_beside_an_open_member_moves_array_members`）、`tests/features.mjs` とフィクスチャ（A25 の `widen_generic` の 9 件と、`widen_open` の具体化に `@tz.alloc` がないことの検査。suite `anonymous_unions` は 169 件から 178 件）、`anonymous-union.md`（拡大の段落の、型変数を含む要素と並ぶ配列の要素）、`docs/language.md`、A24 と A25 のチケット。
- この取り込みは `README.md`・`_features/README.md`・`src/diagnostic_kinds.rs`・`src/explain/codes/` を変えません。各チケットの行（`_completed/` へのリンク）と診断の kind の順はそのままで、`E1041.md` は 1,481 文字、`E1025.md` は 1,465 文字です（上限 1,500 文字）。

相互作用を確かめました。

- A24 の `@old (x)` の注入と A26 のインスタンス: テスト `old_values_compare_hash_and_show_through_the_instances` を足しました（コミット `8cd767b2`）。`@ensures` の比較の右の被演算子に書いた `@old (x)` は左の被演算子の匿名共用体へ注入され、A26 の `Eq`・`Ord` で比べます。型注釈のある `let` で注入した値は、要素の値としてハッシュし（同じ順に case を並べた名前付きの union の `I x` と同じ値）、数だけを表示します。期待値は規則から手で求めました: `grow 41`（結果 42）と `grow (-5)`（結果 -4）で、結果から 1 を引いた値は古い `x` に等しく、結果は同じ `i64` の要素で古い `x` より大きく（`>`・`!=` は真、`<=` は偽。`(string | i64)` も正準順序では `i64` が先）、`"a"` は `string` の要素なのでどの `i64` よりも大きく、等しくありません。`wrong 41` は 42 を古い 41 と比べるのでトラップします。native と WASM、`-O0` と `-O3` で通り、`src/check.rs` の `@old` の注入（A24 の `66e29dd2` の分岐）だけを戻した木では、A24 の `old_values_are_injected` と同じく `E1003`（`expected (i64 | string), found i64`）で失敗しました（記録は `target/scratch/a26/copilot2/prev-old.txt`）。コードの変更は要りませんでした。
- A25 の `LastUses` の変更（`Local::retags_payload`）: 名前が `$retag` で始まる拡大の束縛だけを見ます。A26 の `decode_body` の束縛は `$decoded`・`$value`・`$error` で、具体的な型の本体なので、その move の判断は変わりません。生成 IR の比較（下）でも、A26 の本体は A25 のバイナリと同じでした。

検証（取り込みの後の木）:

- T0: `cargo fmt --all -- --check` は差分なし、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: マージコミット `2a7d0c32` の木で 3163 件がすべて passed、4 skipped です（記録は `target/scratch/a26/copilot2/t1.txt`）。テストを足した後の `--test anonymous_unions` は 159 件がすべて passed です（154 件に A24・A25 の 4 件とこの 1 件。記録は `target/scratch/a26/copilot2/anonymous_unions.txt`）。
- E2E: `node tests/features.mjs target/debug/tsuzuri anonymous_unions` は 178 件が通りました（native と WASM、`-O0` と `-O3`、記録は `target/scratch/a26/copilot2/suite.txt`）。
- IR: `tests/fixtures/` と `examples/` の 186 単位の native と wasm32 の IR・`check --json` を、`e2135291` を `git archive` で取り出してビルドしたバイナリと取り込んだ後のバイナリで取りました（`sh target/scratch/a26/baseline.sh` と `compare.sh`、記録は `target/scratch/a26/copilot2/ir-compare.txt`）。違うのは `tests/fixtures/anonymous_unions` だけで、A25 のバイナリは A26 の関数を持つフィクスチャを `E1005` で拒否します。ほかの 185 単位は 1 byte も違いません。`e2135291` のフィクスチャの native と wasm32 の IR も、2 つのバイナリで同じです。IR とビルドした木は消しました。
- ドキュメント: 言語リファレンスで取り込みが変えたのは `anonymous-union.md` だけです。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` をそのページに実行し、37 links、21 examples、native の実行 40 回（`-O0`・`-O3`）で通りました（記録は `target/scratch/a26/copilot2/check-docs.txt`）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check` は 19 blocks、0 stale、`node scripts/check-cli-docs.mjs` は 4 checks、0 failed、`node scripts/check-guide-lite.mjs` は 88 references、0 failed、`node scripts/check-links.mjs` は 0 broken です。この取り込みとテストは利用者に見える振る舞いを変えないので、言語リファレンスのページは変えていません。

### A25 の再取り込み（Copilot のレビューの第3回の後、2026-11-12）

A24 と A25 が Copilot のレビューの第3回に対応したので、`feat/a25-anonymous-union-conversions` の先端 `c6ab27df`（A25 の `601f4c0b`・`41480d8d` と、A24 の `1ebf66a9`・`ebde884f` を取り込んだもの）を、もう一度このブランチへ取り込みました（マージコミット `9b739dd5`）。履歴は書き換えていません。前の取り込み（`e2135291`）から変わったコードは、A25 の `src/check.rs`・`src/anonymous_union.rs`（隣の被演算子の型を受け取る被演算子だけでなく、その値になる位置（`if` の枝、`match`・`select` の腕、ブロックの結果、`@checked` と計算式の中身）も拡大しない。`Checker::operand_results`）と、A24 の `src/api_diff.rs`（名前だけが違う要素の順）・`src/parser.rs`・`src/parse_control.rs`（型テストを探す走査が型注釈の型を構文解析と同じ文法で読む）です。A26 はこれらを変えていないので、コードは自動で併合されました。

- 衝突は 2 か所です。`docs/architecture.md` の「ほかの段。」の段落は、A26 の側が変えておらず（`e2135291` と同じ文）、A25 の側が `api_diff.rs` の説明を書き直していたので、直前の「組み込みのインスタンス（A26）」の段落とその項目をそのまま残し、「ほかの段。」は A25 の版にしました。`tests/anonymous_unions.rs` の末尾は両方がテストを足していたので、A26 の 4 件（最初の取り込みの 3 件と `old_values_compare_hash_and_show_through_the_instances`）の後に、A24 の `annotation_types_do_not_hide_type_tests` を置きました。
- 自動で併合したもの: `tests/anonymous_unions.rs` のほかの 4 件（A24 の `api_diff_orders_tied_members_by_the_whole_signature`・`api_diff_settles_independent_unions_one_by_one`、A25 の `operand_results_do_not_widen`・`operand_checked_first_widens_by_itself`。159 件から 164 件）、`anonymous-union.md`（拡大の注意点の被演算子の 2 項目）、`docs/language.md`（拡大の段落と API の差分の注意）、A24 と A25 のチケット。`tests/features.mjs`・フィクスチャ・`README.md`・`_features/README.md`・`src/diagnostic_kinds.rs`・`src/explain/codes/` は変わりません。

#### 取り込んだ木で落ちた 3 件のテストと、その判断

取り込んだ木で `cargo test --locked --test anonymous_unions` を実行すると、164 件のうち 161 件が通り、3 件が落ちました（記録は `target/scratch/a26/s5/test-merged.log`）。どれも、仕様に照らすとテストの期待値が古く、併合したコードが正しいと判断しました。期待値の変更をここに先に記録してから、テストを変えます。

1. `comparisons_take_injected_and_widened_operands`（A26 が最初の取り込みで足したテスト）: `assert (v == (if positive 1 then w else 0))`（`v: (f64 | i64 | string)`、`w: (i64 | string)`）が通ることと、`v == (if v > 1 then w else s)` の後の `w`・`s` の使用が `E1012` で、それぞれ拡大の help と注入の help であることを確かめていました。どちらの `w` も右の被演算子（左を先に検査し、その型を期待型として受け取る被演算子）の `then` の枝です。A25 の Copilot 第3回の規則では、その枝は被演算子の値になる位置なので拡大されず、`E1003`（`type_mismatch`、`expected (i64 | f64 | string), found (i64 | string)`）です。A24 の規則（右の被演算子は注入される）と A26 の規則（同じ型の 2 つの値は組み込みの `Eq`・`Ord` で比べる）は、拡大しない位置を決めません。テストのコメント「`if` の被演算子の腕は拡大される」が古い規則を述べていました。A25 のチケットの「修正の検証で見つかった問題への対応（Copilot 第3回）」の 2 も、同じ判断と書き直しの案を記録しています。
   - 書き直し（期待値は規則から手で求めます）: 拡大する枝を、先に検査する左の被演算子の `if` の後の枝に置きます。`let z: (f64 | i64 | string) = 0.5` を足して `assert ((if positive 0 then z else w) == v)` とします。左の被演算子は隣の型を受け取らずにひとりで検査され、`if` は `then` の `z` の型を `else` の期待型にする（D4 の並んだ式の規則）ので、`w` は `(f64 | i64 | string)` へ拡大されます。`positive 0` は偽なので値は拡大した `w`、つまり `string` の要素の `"abc"` で、`v` の `"abc"` と等しくなります。左が接尾辞のない数のリテラルのときは右の被演算子がひとりで検査されるので、`m: (i64 | string) = 5` の `assert (5 == (if positive 0 then z else m))` も通ります（拡大した `m` は `i64` の要素の 5、リテラルの `5` も `i64` の要素へ注入）。拡大した値の順序も確かめます: `m2: (i64 | string) = 5` の `assert (0.25 > (if positive 0 then z else m2))` は、`0.25` が `f64` の要素（正準順序の位置 1）、拡大した `m2` が `i64` の要素（位置 0）なので、タグの比較で真です。
   - 書き直し（A25 の規則を A26 の上で確かめる）: もとの `v == (if positive 1 then w else 0)` は、`w` の位置に `E1003`（上のメッセージ）の 1 件だけになることを確かめます。A25 では比較の `E1005` も出る形がありますが（`match`・`select`）、`Eq` のある A26 では `E1003` だけです。
   - 書き直し（help）: `let u: (f64 | i64 | string) = "abc"` を足し、`let same = (if v > 1 then u else if v > 2 then w else s) == v` にします。左の被演算子の外の `if` の `then` の `u` の型が、`else` の内の `if` の期待型になり、内の `if` はそれを両方の枝へ渡します。`w` は拡大され（先に検査する被演算子は記録されない）、`s` は `string` の要素へ注入されます。右の `v` は同じ型なので借用されるだけです。`let t = w` と `s.length` の help は、今の期待値と同じ文です（拡大の help と注入の help。`src/ownership.rs` の文から手で写したもの）。
2. `operand_results_do_not_widen`（A25）: 被演算子の中の呼び出しの引数・型注釈のある `let`・パイプの左辺は拡大する、という 4 つの形（`v == (if flag 1 then wide w else v)`・`v == (match 1 with | 0 -> wide w | _ -> v)`・`v == { let y: (f64 | i64 | string) = w; y }`・`v == (w |> wide)`）が、比較の `E1005`（匿名共用体に `Eq` がない）になることを確かめていました。A25 のテストのコメントとチケットが「A26 では通ります」と書いているとおり、A26 では被演算子が両方とも `(f64 | i64 | string)` なので、`Eq` の検査は通ります。
   - 新しい期待値: 後の 2 つの形は、誤りがありません。前の 2 つの形は、型の検査は通り、所有権の検査で `E1014` の 1 件です。比較は左の被演算子 `v` を借用したまま右の被演算子を評価し（`docs/language.md` の比較演算子の段落: 比較のあいだに同じ所有者を move すると `E1014`）、右の `if`・`match` の `else v`・`_ -> v` は `v` を新しい値へ move するためです。位置は、その内側の `v` です（A25 のプログラムの 17 行の 45 桁と 52 桁。文字列から手で数えました）。内側の `v` を `x` にしたプログラム（`x` は比較の外の値）は、4 つとも誤りがないことも確かめます。こうして、4 つの形の拡大が比較の `Eq` まで届いて通ることを確かめます。
   - 補助関数 `operand_mismatch` は、`E1003` の誤りか、比較の `E1005` を許していました（「A26 まで匿名共用体は `Eq`・`Ord` を持たない」）。A26 では `E1005` は出ないので、`E1003` だけを許すように強めます（A25 のチケットの「`Eq`・`Ord` のある A26 では `E1003` だけになります」）。
3. `operand_checked_first_widens_by_itself`（A25）: 先に検査する被演算子の中の拡大を、180 のプログラム（6 つの比較演算子、6 つの形、5 つの置き方）で確かめ、どれも誤りが比較の `E1005`（`Eq` か `Ord` がない）の 1 件だけであることを確かめていました。A26 では、両方の被演算子が `(f64 | i64 | string)` なので比較は通ります。
   - 新しい期待値: 174 のプログラムは誤りがありません。拡大されなければ `E1003` になるので、誤りがないことが拡大を示します。残りの 6 つ（ラムダの中の左の被演算子に `select` を置いた形、6 つの演算子）は、`E1005` の `cannot capture Channel<i64> in a reusable function` の 1 件です。`select` の腕 `recv _ from a` のチャネル `a` を、ラムダ（再利用できる関数値）が捕捉するためで、`Channel` は `Drop` を持つので捕捉できません（言語リファレンスの `lambda-expressions.md` の「捕捉」、`Capture` 制約）。A25 では、比較の `E1005` が先に出て、この誤りまで検査が進みませんでした。値の束縛（`v`・`w`・`u` のような `string` の要素を持つ共用体）を捕捉して本体で move するラムダは、捕捉で環境へ move され、関数値は呼び出しのたびに環境を複製するので、誤りになりません（同じページの「捕捉」）。

この 3 件の変更は、A25 のテストの期待値を弱めるものではありません。A25 の期待値が「A26 まで」と断っていた部分を、A26 の規則（D1・D2）と既存の所有権・捕捉の規則で手で求めた期待値に替え、どれも誤りの種類・数・位置を確かめます。
