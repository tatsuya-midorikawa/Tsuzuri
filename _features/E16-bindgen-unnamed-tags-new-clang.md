# E16: 新しい Clang（23 で確認）の AST では、bindgen が `typedef struct { ... } S;` の無名の struct を tag の `struct S` と取り違える

| 項目 | 内容 |
| --- | --- |
| ID | E16 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | E11 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-05（TZ12 の作業中に `tests/bindgen.mjs` の `skipped` の golden の差として発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（文書が約束している振る舞いに戻す修正） |
| 改善する劣位 | 不具合: Clang 23.1.4 で `tsuzuri bindgen` を動かすと、`typedef struct { int small; } tagged;` を引数に取る C 関数が、別の型 `struct tagged { double big; }` の `ref Tagged` として生成される（ABI の合わない extern。呼ぶと C の側が違う配置の値を読む）。無名の enum の typedef を引数に取る関数は、Clang 21 なら `i32` で生成されるのに、Clang 23 では `W2002` で skip される |
| 手本にする既存実装 | `src/bindgen.rs` の `unnamed_tag`・`Tables::new` の typedef の `poisoned`・`anonymous`。テスト: `tests/bindgen.rs` の `does_not_read_unnamed_struct_typedefs_as_tags`（Clang 21 の AST の形を手で作る）、`tests/bindgen.mjs` の golden `skipped` |
| 主な影響ファイル | `src/bindgen.rs`, `tests/bindgen.rs`, `tests/fixtures/bindgen/skipped/skipped.h`, `tests/fixtures/bindgen/skipped/expected.tz`, `_features/README.md` |

## 目的

`tsuzuri bindgen` が、Clang の版によらず、無名の struct・union・enum を名付ける typedef を正しく見分けるようにする。
[ネイティブ連携](../_tsuzuri/language-reference/compiler/native-interop.md) の「`typedef struct { ... } S;` の無名の struct は、引数で `S` と書いても変換しません」という約束が、
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

- [ ] 「再現」の `m.h` が、Clang 21 と 23 で同じ本文（Clang 21 の今の出力）になる。
- [ ] `tests/bindgen.rs` の Clang 23 の形のケースが、修正前に失敗し、修正後に成功する。
- [ ] `tests/bindgen.mjs` が Clang 21 と 23 の両方で成功する。
- [ ] fmt・clippy・T1 が成功する。

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
