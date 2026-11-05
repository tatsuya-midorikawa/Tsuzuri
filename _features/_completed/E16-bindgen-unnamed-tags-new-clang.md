# E16: 新しい Clang（23 で確認）の AST では、bindgen が `typedef struct { ... } S;` の無名の struct を tag の `struct S` と取り違える

| 項目 | 内容 |
| --- | --- |
| ID | E16 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | E11 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-05（TZ12 の作業中に `tests/bindgen.mjs` の `skipped` の golden の差として発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（文書が約束している振る舞いに戻す修正） |
| 改善する劣位 | 不具合: Clang 23.1.4 で `tsuzuri bindgen` を動かすと、`typedef struct { int small; } tagged;` を引数に取る C 関数が、別の型 `struct tagged { double big; }` の `ref Tagged` として生成される（ABI の合わない extern。呼ぶと C の側が違う配置の値を読む）。無名の enum の typedef を引数に取る関数は、Clang 21 なら `i32` で生成されるのに、Clang 23 では `W2002` で skip される |
| 手本にする既存実装 | `src/bindgen.rs` の `unnamed_tag`・`Tables::new` の typedef の `poisoned`・`anonymous`。テスト: `tests/bindgen.rs` の `does_not_read_unnamed_struct_typedefs_as_tags`（Clang 21 の AST の形を手で作る）、`tests/bindgen.mjs` の golden `skipped` |
| 主な影響ファイル | `src/bindgen.rs`, `tests/bindgen.rs`, `tests/fixtures/bindgen/skipped/skipped.h`, `tests/fixtures/bindgen/skipped/expected.tz`, `_features/README.md` |

## 目的

`tsuzuri bindgen` が、Clang の版によらず、無名の struct・union・enum を名付ける typedef を正しく見分けるようにする。
[ネイティブ連携](../../_tsuzuri/language-reference/compiler/native-interop.md) の「`typedef struct { ... } S;` の無名の struct は、引数で `S` と書いても変換しません」という約束が、
新しい Clang では破れている。bindgen の目的は「ABI の一致を確かめられる C の宣言だけを生成し、推測で型を当てはめない」ことなので、
合わない extern を黙って出すのは最も重い種類の誤りである。

## 着手条件と停止条件

### 着手条件

- E11 が `_features/README.md` で done であること（HEAD で done）。
- Clang 21（CI と同じ）と Clang 23 以降の両方を用意できること。macOS なら Homebrew の `llvm@21` と `llvm`。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の再現を確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Clang 21 で、修正の前後で `tests/bindgen.mjs` のどれかの golden の出力が変わる。
- Clang 23 の AST の別の変化（このチケットの無名の tag 以外）で、`tests/bindgen.mjs` の golden がまだ一致しない。見つけた変化ごとに報告する
  （このチケットで直すか、別のチケットにするかを決める）。
- 無名の tag を見分ける手がかりが、AST の JSON の `ownedTagDecl` と `decl` のどちらにも無い Clang の版が見つかった。

## 現状（HEAD `5ee6b0f2` で確認）

- `src/bindgen.rs:1342` の `Typedef` の doc comment: Clang は `typedef struct { ... } S;` を `struct S` と綴るので、無名の struct を名付ける typedef は
  `poisoned` にして展開しない（展開すると、無関係な tag の `struct S` と読んでしまう）。
- `src/bindgen.rs:1381` の `unnamed_tag` は、typedef の型の節点の中から、`ownedTagDecl` の `name` が空のものだけを探す。
- `src/bindgen.rs:1432〜1446` の `Tables::new` は、`unnamed_tag` が見つけた tag が `RecordDecl` なら typedef を `poisoned` にし、tag の `id` から
  typedef の名前と位置への表 `anonymous` を作る。`anonymous` は、無名の enum の key（`enum_key`、1474 行）と、無名の struct・union の
  skip の報告（`record`、1822 行。「An unnamed struct or union is reported at the typedef that names it」）が使う。
- Clang の AST の JSON（`clang -x c -fsyntax-only -Xclang -ast-dump=json`）の `typedef struct { int small; } tagged;`:
  - Clang 21.1.8（Homebrew `llvm@21`）: `TypedefDecl` の下に `ElaboratedType`（`ownedTagDecl: { kind: RecordDecl, name: "" }`）、その下に `RecordType`（`decl`）。
  - Clang 23.1.4（Homebrew `llvm`）: `TypedefDecl` の下に `RecordType`（`type.qualType: "struct tagged"`、`decl: { kind: RecordDecl, name: "" }`）だけで、
    `ElaboratedType` と `ownedTagDecl` が無い。どの版から変わったかは確かめていない（22 は未確認）。
- そのため Clang 23 では `unnamed_tag` が何も見つけず、typedef は `poisoned` にならず、`anonymous` も空になる。
- `src/bindgen_driver.rs:48` は `tool("TSUZURI_CLANG", "clang")` で Clang を選び、`CLANG_HINT` は「LLVM/Clang 17+」と案内する。版の上限は無い。
- CI は LLVM 21 だけを使う（`.github/workflows/ci.yml` の `TSUZURI_CLANG: /usr/lib/llvm-21/bin/clang`）ので、この差を見ない。
- `tests/bindgen.rs:1683` の `does_not_read_unnamed_struct_typedefs_as_tags` と 421 行のテストは、Clang 21 の形（`ElaboratedType` と `ownedTagDecl`）だけを手で作る。

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64）

`m.h`:

```c
typedef enum { MODE_A, MODE_B } mode;
int pick(mode value);
struct tagged { double big; };
typedef struct { int small; } tagged;
double use_tagged(const tagged *value);
```

```sh
TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang target/debug/tsuzuri bindgen m.h -o M.tz
TSUZURI_CLANG=/opt/homebrew/opt/llvm/bin/clang target/debug/tsuzuri bindgen m.h -o M.tz
```

Clang 21.1.8 の本文（見出しの 5 行を除く）:

```text
const MODE_A: i32 = 0
const MODE_B: i32 = 1
extern "pick" def pick :: i32 -> i32
record Tagged { big: f64 }
// skipped tagged: anonymous struct; give the struct a tag
// skipped use_tagged: parameter 1 has unsupported type 'const tagged *'
```

Clang 23.1.4 の本文:

```text
const MODE_A: i32 = 0
const MODE_B: i32 = 1
// skipped pick: parameter 1 has unsupported type 'mode'
record Tagged { big: f64 }
// skipped (anonymous struct): anonymous struct; give the struct a tag
extern "use_tagged" def use_tagged :: ref Tagged -> f64
```

`node tests/bindgen.mjs target/debug/tsuzuri` は、Clang 23 では golden `skipped` の同じ 3 行の差で失敗する
（`skipped untagged` が `skipped (anonymous struct)` に、`skipped tagged` と `skipped use_tagged` が `skipped (anonymous struct)` と `extern "use_tagged" … ref Tagged` に）。

## 仕様

`tsuzuri bindgen` の出力を、Clang の AST の形によらず次のとおりにする（Clang 21 の今の出力と同じ）。

- 無名の struct・union を名付ける typedef（`typedef struct { ... } S;`、`typedef struct { ... } *P;` など、typedef の型の中に無名の tag の定義を持つもの）は展開しない。
  その typedef を使う引数・結果・フィールドは `unsupported type` で skip する。
- 無名の struct・union の skip は、名付ける typedef の名前と位置で報告する（`// skipped tagged: anonymous struct; give the struct a tag`）。
- 無名の enum を名付ける typedef は、今と同じく `enum <typedef 名>` の key で、`int` の enum なら `i32` にする（同じ名前の enum の tag があれば、どちらも変換しない）。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/bindgen.rs` | `Tables::new` | 翻訳単位の全節点から、`name` が空の `RecordDecl`・`EnumDecl` の `id` の集合 `unnamed_ids` を先に作る |
| `src/bindgen.rs` | `unnamed_tag` | `ownedTagDecl`（Clang 21）に加えて、`decl` の `id` が `unnamed_ids` にある節点（Clang 23）も、typedef が名付ける無名の tag として返す（D1） |
| `src/bindgen.rs` | `Typedef` と `unnamed_tag` の doc comment | 2 つの AST の形を書く |
| `tests/bindgen.rs` | `does_not_read_unnamed_struct_typedefs_as_tags` ほか | Clang 23 の形の AST でも同じ出力になるテスト |
| `tests/fixtures/bindgen/skipped/` | `skipped.h`・`expected.tz` | 無名の enum の typedef を引数に取る関数を足す |

### アルゴリズム

1. `Tables::new` の最初の走査（`complete`・`enum_tags` を集める `visit_all`）で、`kind` が `RecordDecl` か `EnumDecl` で `name` が空の節点の `id` を
   `unnamed_ids: BTreeSet<&str>` に入れる。
2. `unnamed_tag(node, unnamed_ids)` は、typedef の型の節点を今と同じ順（深さ `MAX_TYPE_NESTING` まで）でたどり、次のどちらかに当たった最初の
   節点の tag を返す。
   - `ownedTagDecl` の `name` が空（今の判定）。
   - `decl` の `id` が `unnamed_ids` にある。このとき返す値は `decl`（`id`・`kind`・`name` を持つ）。
3. `anonymous` の表は今と同じく、tag の `id` ごとに最初の typedef を残す（`or_insert`）。

`decl` は Clang 21 の `RecordType` にもあるが、同じ typedef の中では `ElaboratedType` の `ownedTagDecl` が先に（外側で）見つかり、どちらも同じ `id` を指すので、
Clang 21 の結果は変わらない。

## 実装手順

### 手順 1: ベースラインと再現

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンド。「再現」を両方の Clang で確かめる。Clang 21 で全 golden の出力を保存する。
- 確認: `TSUZURI_CLANG=<clang 21> node tests/bindgen.mjs target/debug/tsuzuri` が成功し、`TSUZURI_CLANG=<clang 23> …` が `skipped` で失敗する。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/bindgen.rs`。
- 内容: `does_not_read_unnamed_struct_typedefs_as_tags` の AST の作り方を、Clang 21 の形と Clang 23 の形（`RecordType` の `decl` だけ）の 2 通りにし、
  両方で同じ行を期待する。無名の enum の typedef（`typedef enum { A, B } mode; int pick(mode);`）も 2 通りで、`extern "pick" def pick :: i32 -> i32` を期待する
  テスト `reads_unnamed_enum_typedefs_from_either_ast_shape` を足す。421 行のテストの `anon_t` も 2 通りにする。
- 確認: `cargo test --locked --test bindgen` で、Clang 23 の形のケースだけが失敗する。

### 手順 3: `unnamed_tag` を直す

- 変更: `src/bindgen.rs`。
- 内容: 「アルゴリズム」のとおり。
- 確認: `cargo test --locked --test bindgen` と `cargo test --locked --lib bindgen` が全件成功する。

### 手順 4: golden と両方の Clang

- 変更: `tests/fixtures/bindgen/skipped/skipped.h`・`expected.tz`。
- 内容: `typedef enum { PICK_A, PICK_B } pick_mode; int pick_mode_value(pick_mode value);` を足し、`expected.tz` に Clang 21 の出力
  （`const PICK_A: i32 = 0`・`const PICK_B: i32 = 1`・`extern "pick_mode_value" def pick_mode_value :: i32 -> i32`）を書く。名前が既存の宣言と衝突しないことを確かめる。
- 確認: `TSUZURI_CLANG=<clang 21> node tests/bindgen.mjs target/debug/tsuzuri` と `TSUZURI_CLANG=<clang 23> …` が、どちらも全 golden で成功する。
  Clang 23 で別の差が残ったら停止条件の 2 つ目。

### 手順 5: 最終確認

- fmt・clippy・T1、`node tests/bindgen.mjs` を両方の Clang で。

## テスト計画

### Rust テスト

- `tests/bindgen.rs`: `does_not_read_unnamed_struct_typedefs_as_tags`（2 つの AST の形）、`reads_unnamed_enum_typedefs_from_either_ast_shape`（新規）、
  421 行のテスト（`anon_t` を 2 つの形で）。CI の Clang 21 では作れない Clang 23 の形を、Rust のテストで確かめる。

### E2E

- `tests/bindgen.mjs`（golden 5 つ）を Clang 21 と 23 で。CI は Clang 21 だけで動く。

### 既存テストへの影響

- `tests/fixtures/bindgen/skipped/` に 1 つの enum と 1 つの関数を足す。ほかの期待値は変わらない。

### 性能

- 影響なし（AST の走査が 1 回の集合の作成を足すだけ）。

## ドキュメント

- `_tsuzuri/language-reference/compiler/native-interop.md`: 変更不要。658 行の約束（無名の struct の typedef は変換しない）が、新しい Clang でも成り立つようになる。
  確かめた Clang の版を書く節（`grep -n "Clang" _tsuzuri/language-reference/compiler/native-interop.md`）があれば、21 と 23 を足す。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [x] 「再現」の `m.h` が、Clang 21 と 23 で同じ本文（Clang 21 の今の出力）になる。
- [x] `tests/bindgen.rs` の Clang 23 の形のケースが、修正前に失敗し、修正後に成功する。
- [x] `tests/bindgen.mjs` が Clang 21 と 23 の両方で成功する。
- [x] fmt・clippy・T1 が成功する。

## 落とし穴

- `decl` は無名の tag への参照すべてに付く（Clang 23）。`unnamed_ids` で「名前の無い tag」に絞らずに `decl` を見ると、`typedef struct S T;` の `T` まで
  `poisoned` になり、`S` が定義されていれば変換できる typedef を失う。
- 無名の struct をフィールドに持つ struct（`struct outer { struct { int a; } inner; };`）の `RecordDecl` も `name` が空になる。これは typedef が名付けないので
  `anonymous` には入らず、今と同じく `(anonymous struct)` で報告される。手順 2 で 1 通り確かめる。
- Clang の版によって `qualType` の綴り（`struct (unnamed struct at …)` と `(anonymous struct at …)`）も違う。`src/bindgen.rs:1239` の `scrub` は両方を扱う。新しい綴りが
  出たら停止条件の 2 つ目として報告する。

## 対象外

- 無名の struct・union の変換（tag を付けるよう案内する今の方針を保つ）。
- CI に Clang 23 の job を足すこと（Rust のテストで AST の形を確かめる）。

## 決定事項

### D1: 無名の tag は `id` の集合で見分ける

- 決定: 翻訳単位の名前の無い `RecordDecl`・`EnumDecl` の `id` を集め、typedef の型の節点の `ownedTagDecl` か `decl` がその `id` を指すとき、typedef が無名の tag を名付けるとみなす。
- 理由: Clang 21 の形（`ownedTagDecl`）と Clang 23 の形（`decl`）の両方を、同じ規則で読める。版の番号で分岐する案は、22 などの未確認の版で誤る。
- 状態: 既定案（実装者はこの案に従う）。
- 実装での形（2026-11-06 のレビューの後に書き足し）: `ownedTagDecl` は、アルゴリズムの 2 のとおり今までどおり `name` が空であることで判定し、`id` の集合は `decl` にだけ使う。`ownedTagDecl` は typedef 自身が宣言する tag なので、名前が空ならそれが無名の tag で、実際の Clang 21 の AST ではその `id` も集合にある（結果は同じ）。どちらか一方が見つかれば名付けるとみなす。

## 実装と検証（2026-11-06）

Clang 21.1.8（Homebrew `llvm@21`）と Clang 23.1.4（Homebrew `llvm`）、macOS arm64 で、手順 1〜5 をすべて行いました。

### 手順と結果

- 手順 1: 「再現」の `m.h` が、Clang 21 では上の本文、Clang 23 では `skipped pick`・`skipped (anonymous struct)`・`extern "use_tagged" … ref Tagged` になることを確かめました。
  `TSUZURI_CLANG=<clang 21> node tests/bindgen.mjs target/debug/tsuzuri` は成功、Clang 23 は golden `skipped` の 3 行の差で失敗しました。5 つの golden の Clang 21 の出力を保存しました。
- 手順 2: `tests/bindgen.rs` に AST の形 `Shape`（`Elaborated` が Clang 21、`Direct` が Clang 23）と、型のノードを作る `tag_ref`・`tag_type`・`typedef_over` を足し、
  `generates_scalar_records_and_ref_parameters`（`anon_t`）、`does_not_read_unnamed_struct_typedefs_as_tags`、新しい `reads_unnamed_enum_typedefs_from_either_ast_shape` を 2 つの形で回すようにしました。
  修正前の `cargo test --locked --test bindgen` は `running 27 tests`、24 passed・3 failed で、3 件とも `Direct` の形だけが失敗しました（`Elaborated` は先に回って成功）。`Direct` の失敗には、`extern "use_typedef" def use_typedef :: ref S -> f64` という ABI の合わない extern が出ていました。
- 手順 3: `Tables::new` の最初の走査で、名前のない `RecordDecl`・`EnumDecl` の `id` の集合を作り、`unnamed_tag` が、名前のない `ownedTagDecl`（今までの判定。集合は見ません）に加えて、`decl` の `id` がその集合にある型のノード（Clang 23 の形）も返すようにしました（D1）。
  `Typedef` と `unnamed_tag` の doc comment に 2 つの形を書きました。`cargo test --locked --test bindgen` は 27 passed、`cargo test --locked --lib bindgen` は 5 passed です。
  落とし穴の 1 つ目は、`id` の集合で絞る条件を外す変更（`decl` の `id` があれば常に名付けるとみなす）で `does_not_read_unnamed_struct_typedefs_as_tags` が失敗する（`typedef struct S S_named;` を展開しなくなる）ことを確かめてから、元に戻しました。
- 手順 4: `skipped.h` に `typedef enum { PICK_A, PICK_B } pick_mode; int pick_mode_value(pick_mode value);` を足し、`expected.tz` に `const PICK_A: i32 = 0`・`const PICK_B: i32 = 1`・`extern "pick_mode_value" def pick_mode_value :: i32 -> i32` を足しました（名前は既存の宣言と衝突しません）。
  修正の前後で、Clang 21 の 5 つの golden と `m.h` の出力は（足した行を除いて）1 byte も変わりません。修正後は Clang 21 と 23 の出力が 5 つの golden と `m.h` のすべてで一致します。
- 手順 5: `cargo fmt --all -- --check` と `cargo clippy --locked --all-targets -- -D warnings` は成功。`cargo nextest run --locked` は 2935 tests run: 2935 passed、4 skipped。
  `cargo build` の後、`TSUZURI_CLANG=/opt/homebrew/opt/llvm@21/bin/clang node tests/bindgen.mjs target/debug/tsuzuri` と `TSUZURI_CLANG=/opt/homebrew/opt/llvm/bin/clang …` は、どちらも golden・決定性・`check`／`fmt --check`・W2002、native の往復（`-O0`／`-O3`）、CLI のすべてが成功しました。
  `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/native-interop.md` は成功（1 ページ、36 リンク、8 例、4 回の native 実行）。

### チケットを補った判断

1. **Clang 23 の無名の tag の綴りも直しました（落とし穴の 3 つ目）。** Clang 23 は `qualType` で無名の tag を `struct (unnamed at …)` と綴り、Clang 21 の `struct (unnamed struct at …)` から種類の語が抜けます。
   golden には出ませんが、無名の struct をフィールドの型に持つ struct の skip が `'struct (unnamed)'`（Clang 23）と `'struct (unnamed struct)'`（Clang 21）で食い違いました。
   停止条件の 2 つ目は「golden がまだ一致しない」場合で、これには当たりません。仕様の「Clang の AST の形によらず、Clang 21 の今の出力と同じ」に合わせるため、`scrub` が `(unnamed at …)` の直前の `struct`・`union`・`enum` を補って `(unnamed struct)` などにするようにしました（数行の変更で、Clang 21 の出力は変わりません）。
   `src/bindgen.rs` の単体テスト `splits_function_types` に Clang 23 の綴りを足しました。
2. **golden `skipped` に、無名の struct をフィールドに持つ struct を 1 つ足しました。** `struct nested_unnamed { struct { int a; } inner; };`（`// skipped nested_unnamed: field 'inner' has unsupported type 'struct (unnamed struct)'`）で、1 の綴りを実際の Clang 23 でも golden で確かめるためです。
   skip が 1 つ増えるので、`tests/bindgen.mjs` の `--json` の W2002 の件数を 37 から 38 にしました（ほかの期待値は変わりません）。
3. **Rust のテストに、落とし穴を確かめるケースを足しました。** `does_not_read_unnamed_struct_typedefs_as_tags` に、名前のある tag の typedef `typedef struct S S_named;`（展開して `ref S` になる。落とし穴の 1 つ目）、
   `typedef struct { ... } *P;`（仕様の 1 つ目の例）、無名の struct をフィールドに持つ struct と、どの typedef も名付けない無名の struct（`(anonymous struct)` で報告。落とし穴の 2 つ目）を足しました。
   `Elaborated` の形は、Clang 21 が実際に書くとおり `ElaboratedType` の下に `decl` を持つ `RecordType`・`EnumType` を置くので、新しい `decl` の判定が Clang 21 の形で結果を変えないことも同じテストで確かめています。ただしこの形では `decl` の判定だけでも同じ結果になり、`ownedTagDecl` の判定を確かめていませんでした（「レビューの指摘への対応」で、それだけを確かめるテストを足しました）。
   `reads_unnamed_enum_typedefs_from_either_ast_shape` は、定数名を `MODE_A`・`MODE_B` にし、typedef の別名（`typedef mode mode_alias;`）を引数に取る関数も足しました。
4. Clang 23 は型のノードに `isTagOwned` も書きますが、D1 のとおり `id` の集合で判定し、`isTagOwned` は読みません。どちらの形も同じ規則で読めて、未確認の版（22）にも版の番号で分岐しないからです。

### Clang 23 の AST のほかの違い

5 つの golden は、修正後に Clang 21 と 23 で一致します（停止条件の 2 つ目には当たりません）。golden のほかに、次の形のヘッダーで両方の Clang の出力が一致し、Clang 21 の出力が修正の前後で変わらないことを確かめました:
`typedef struct { ... } A, *PA;`、`typedef A B;`、`typedef __typeof__(A) C;`、`const A *` を取る関数ポインターの typedef、`typedef struct named { ... } named_t;` と `typedef struct named named2_t;`、
`typedef enum { ... } E, *PE;` と `typedef E E2;`、無名の union の typedef、`typedef const struct { ... } *CP;`、struct の中の無名の struct・union・enum のフィールド、無名のメンバー（`struct { int a; };`）、無名の struct の大域変数。
Clang 22 は手元に無く、確かめていません（D1 は版によらない規則です）。CI は今までどおり Clang 21 だけで、Clang 23 の形は Rust のテストが確かめます。

### 更新した文書

- `_tsuzuri/language-reference/compiler/native-interop.md`: 無名の struct の typedef の項に、判定と `// skipped` 行の無名の tag の表記が Clang 21 と 23 で同じになることを書きました（版を書く節は無かったので、この項に足しました）。
- `docs/language.md`: bindgen の無名の tag の段落に、同じことを 1 文で足しました。
- `docs/architecture.md`: 無名の tag を `id` の集合と `ownedTagDecl`／`decl` で見分ける規則と、Clang 23 の無名の tag の綴りを `scrub` がそろえることを書きました。
- `_features/README.md`: 状態とリンクを更新しました。

### 変更したファイル

`src/bindgen.rs`、`tests/bindgen.rs`、`tests/bindgen.mjs`、`tests/fixtures/bindgen/skipped/skipped.h`、`tests/fixtures/bindgen/skipped/expected.tz`、
`_tsuzuri/language-reference/compiler/native-interop.md`、`docs/language.md`、`docs/architecture.md`、`_features/README.md`、このチケット（`_features/_completed/` へ移動）。
