# A26: 匿名共用体の型クラス（`Eq`・`Ord`・`Hash`・`Display`）と JSON（`Encode`・`Decode`）

| 項目 | 内容 |
| --- | --- |
| ID | A26 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | A24, A06, A07 |
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
