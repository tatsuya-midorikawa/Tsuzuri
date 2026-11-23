# NS03: tsuzuri bindgen の Nullability 対応（_Nullable・_Nonnull と既定 Maybe 化）

| 項目 | 内容 |
| --- | --- |
| ID | NS03 |
| 優先度 | P1（C ヘッダーのポインタは断りがない限り `NULL` になり得るのに、現行の `bindgen` はすべて非 `Maybe` として出力するため偽の非 Null 型が生成される） |
| 規模 | M（`src/bindgen.rs` の Nullability 解析・AST 属性読み取り・CLI `--nonnull` オプション、テストと文書） |
| 依存 | W15, NS01 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-22（[GUIDE D-64](../GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（`bindgen` の既定出力型の変更と CLI オプション `--nonnull` の追加（D-55）。決定事項 D1・D2 が要承認、D3〜D5 は既定案） |
| 手本にする既存実装 | `src/bindgen.rs` の `--consume FUNCTION:PARAMETER`・`--buffer FUNCTION:POINTER:LENGTH` の注釈解決（2130〜2290 行付近）、Clang 型字句解析 `qualifiers`（1035〜1070 行）、Clang JSON AST の属性走査 |
| 主な影響ファイル | `src/bindgen.rs`, `src/bindgen_driver.rs`, `src/main.rs`, `tests/bindgen.rs`, `tests/bindgen.mjs`, `docs/language.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/compiler/option.md`, `_tsuzuri/language-reference/compiler/usage.md`, `README.md` |

## 実装者向け早見

- 範囲: `tsuzuri bindgen` が C ヘッダーのポインタ（不透明 struct へのポインタ、および `const struct TAG *` のレコード入力）を変換するとき、Clang の Nullability 注釈（`_Nonnull`・`_Nullable`・`_Nullable_result`・`_Null_unspecified`）と関数属性（`__attribute__((nonnull))`・`__attribute__((returns_nonnull))`）、および新しい CLI 注釈 `--nonnull TARGET` を読んで型を決める。**非 Null の根拠がない C ポインタは既定で `Maybe<...>` に写し**、[NS01](NS01-nonnull-extern-handles.md) の境界トラップで落ちる偽の非 Null シグネチャを生成しないようにする。
- 最初に読む: ① 仕様 1〜3 ② `src/bindgen.rs` の 1035〜1070 行（`_Nonnull` / `_Nullable` を捨てている箇所）と 2295〜2360 行（`value_type`・`callback`）③ 2130〜2290 行（`--consume`・`--buffer` の検証）。
- 最初に書くテスト: `tests/bindgen.rs` の `unannotated_pointers_default_to_maybe` と `nonnull_qualifiers_and_attributes_emit_non_maybe`。
- 触ってはいけない: スカラー・enum・マクロ・`--buffer`（空スライスはもともと `NULL` と長さ `0` を許容）の変換規則。
- 完了の見分け方: `struct db; struct db *db_open(const char *path); void db_close(struct db * _Nonnull db);` から `db_open` の結果が `Maybe<Db>`、`db_close`（`--consume db_close:db`）の引数が `Db` として生成される。

## 目的

現行の `src/bindgen.rs`（1040〜1041 行、1063〜1064 行）は `_Nonnull`・`_Nullable`・`_Null_unspecified`・`_Nullable_result` をすべて無視し、不透明 struct ポインタを常に `ref H`（引数）・`H`（結果）、構造体ポインタを常に `ref Record` として出力する。

C の実コードでは、`fopen`・`sqlite3_db_handle`・`SSL_new` などの生成関数は失敗時に `NULL` を返すのが標準であり、ほとんどの C ヘッダーは `_Nullable` を書かない。このまま [NS01](NS01-nonnull-extern-handles.md)（非 `Maybe` の `H` で `NULL` が返るとトラップ）を入れると、`bindgen` が生成した宣言が失敗系でトラップしてしまう。Swift の C インポートや Rust bindgen の安全ラッパーと同様に、「非 Null と証明・指定された箇所だけ `H` / `ref H` / `ref R`、それ以外は `Maybe<...>`」とすることで、自動生成バインディングを最初から Null 安全にする。

## 着手条件と停止条件

### 着手条件

- [W15](W15-abi-nullable-handles.md) と [NS01](NS01-nonnull-extern-handles.md) が `done` であること。
- 決定事項 D1・D2 の承認があること。
- `node scripts/repo-kb.mjs brief NS03` を読み、Clang 21 と Clang 23 の JSON AST における `NonNullAttr`・`ReturnsNonNullAttr` と `_Nonnull` / `_Nullable` の `qualType` 表記を確認する。

### 停止条件

- **S1** Clang 21 と Clang 23 で同じヘッダーに対する生成結果が食い違う（`docs/language.md` 526 行の決定性保証に反する）。
- **S2** W15 が許していない位置（コールバックの結果のポインタや `Maybe<ref Record>` のコールバック引数など）に `Maybe` を出してしまう。W15 の許容位置の外は `W2002` で skip する。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- `src/bindgen.rs` 1040〜1041 行・1063〜1064 行: `"_Nonnull" | "_Nullable" | "_Null_unspecified" | "_Nullable_result" => {}` としてトークンを捨てている。2521 行の単体テスト `parsed("int * _Nonnull")` も修飾子が消えることを前提にしている。
- `src/bindgen.rs` 2311〜2336 行（`value_type`）: 不透明 struct ポインタは `Role::Parameter | Role::CallbackParameter` で `ref {handle}`、`Role::Result` で `{handle}`、定義済み struct の `const struct TAG *` は `Role::Parameter` で `ref {record}` を返す。
- `--buffer` で変換される `ref [T]` は `src/llvm_imports.rs` / `docs/language.md` 542 行のとおり、空配列で `NULL` と長さ `0` を扱えるため `Maybe` にする必要はない。

## 変えてはいけない意味

- スカラー型、enum 定数、整数リテラルマクロ、関数ポインタ引数（コールバック自体の関数ポインタが `_Nullable` の場合は下記仕様 2）、`--buffer` の変換と `W2002` のスキップ基準。
- 生成ファイルの先頭マーカー、SHA-256、`// options:` コメントの決定性。

## 仕様

### 1. ポインタごとの Nullability 判定

ポインタ型（外側の `*` に付く修飾子）と関数の属性・CLI 注釈から、各位置を `Nonnull` または `Nullable` に分類する。

| 優先順 | 条件 | 分類 |
| --- | --- | --- |
| 1 | ポインタ自身に `_Nullable` または `_Nullable_result` がある | `Nullable`（もし同じ位置に `--nonnull` が指定されていれば矛盾として `E2000`） |
| 2 | CLI で `--nonnull FUNCTION:PARAMETER`（引数は C の名前か 1 始まりの番号）または `--nonnull FUNCTION:return` が指定されている | `Nonnull` |
| 3 | ポインタ自身に `_Nonnull` がある、または関数の Clang AST に該当引数の `NonNullAttr` / 結果の `ReturnsNonNullAttr` がある（引数番号指定なしの `__attribute__((nonnull))` は全ポインタ引数に効く） | `Nonnull` |
| 4 | 上記以外（注釈なし、または `_Null_unspecified`） | `Nullable` |

### 2. Tsuzuri の型への写像

| C の型と位置 | `Nonnull` の出力 | `Nullable` の出力 |
| --- | --- | --- |
| 不透明 `struct TAG *` 引数（`--consume` なし） | `ref H` | `Maybe<ref H>` |
| 不透明 `struct TAG *` 引数（`--consume` あり） | `H` | `Maybe<H>` |
| 不透明 `struct TAG *` 結果（`const` なし） | `H` | `Maybe<H>` |
| 不透明 `struct TAG *` コールバック引数 | `ref H` | `Maybe<ref H>` |
| 定義済み `const struct TAG *` 引数 | `ref R` | `Maybe<ref R>` |
| 関数ポインタ引数（コールバック） | `(P1 -> ... -> R)` | `W2002` で関数を skip（C が `NULL` コールバックを取る API でも、Tsuzuri 側からは常に関数を渡せるが、逆に `_Nullable` 関数ポインタは Tsuzuri から `NULL` を渡す手段がまだないため D4 参照） |

- **コールバックの関数ポインタ引数の扱い（D4）**: `extern def` のコールバック引数では Tsuzuri が呼び出し側（常に非 Null のラッパー関数を渡す）なので、関数ポインタが `_Nullable` でも非 Null の関数を渡すことは C の契約上常に正当である。よって関数ポインタ自身の Nullability は問わず `(P1 -> ... -> R)` に変換する（`NULL` を渡したい利用者は W14 の複数宣言または手書き `extern` を使う）。

### 3. CLI オプション `--nonnull`

```sh
tsuzuri bindgen header.h -o Module.tz [--nonnull FUNCTION:TARGET]...
```

- `TARGET` は引数名、1 始まりの引数位置、または予約名 `return`（C では `return` が予約語のため引数名と絶対に衝突しない）。
- 存在しない関数・引数、ポインタ変換対象でない位置、`--buffer` と重なる位置、同じ位置の 2 回指定、`_Nullable` が明示された位置への指定は `E2000`（`--consume`・`--buffer` と同じ規則）。
- 出力ファイル先頭の `// options:` 行に `--nonnull` 指定も決定的な順序で記録する。

## 設計

- `Qualifiers`（`src/bindgen.rs` 900 行付近）に `nullability: Option<Nullability>`（`Nonnull`・`Nullable`）を追加し、`qualifiers` と型指定子内の `_Nonnull` / `_Nullable` / `_Nullable_result` を保持する（`_Null_unspecified` は `None`）。
- Clang JSON AST の `FunctionDecl.inner` から `NonNullAttr`（`args` が空なら全ポインタ引数、あれば該当 0-based index）と `ReturnsNonNullAttr` を読み取る。
- `value_type` に `Nullability`（解決後の `Nonnull` / `Nullable`）を渡し、`Nullable` なら `format!("Maybe<{inner}>")` で包む（ただし `Role::CallbackParameter` の `const struct TAG *` など元々変換しないものは `None` のまま）。
- `src/main.rs` の `bindgen` サブコマンド解析に `--nonnull` を追加し、`check-cli-docs.mjs` が検査する `--help`・`README.md`・`compiler/option.md` を更新する。

## 前提とする他チケットのインターフェース

- [W15](W15-abi-nullable-handles.md): `extern` 引数の `Maybe<H>`・`Maybe<ref H>`・`Maybe<ref R>`、結果の `Maybe<H>`、コールバック引数の `Maybe<H>`・`Maybe<ref H>`。
- [NS01](NS01-nonnull-extern-handles.md): 非 `Maybe` の `H` の非 Null 不変条件。

## 他チケットへ提供するインターフェース

- Null 安全な C ヘッダー自動取り込み。

## 実装手順

1. **変更**: なし（ブランチ `feat/ns03-bindgen-nullability`）。**内容**: `node scripts/repo-kb.mjs brief NS03`、Clang の JSON AST で `NonNullAttr`・`ReturnsNonNullAttr` のフィールド形式を実測確認。**確認**: 形式を控える。
2. **変更**: `src/bindgen.rs`, `src/bindgen_driver.rs`, `src/main.rs`。**内容**: `Nullability` の保持、AST 属性の読み取り、`--nonnull` オプション、`Maybe<...>` 出力。**確認**: T0、`cargo test --locked --test bindgen`、T1。
3. **変更**: `tests/bindgen.rs`, `tests/bindgen.mjs`。**内容**: 既存の `tests/bindgen.rs`・`tests/bindgen.mjs` の期待値（注釈なしポインタが `Maybe` になる分）を更新し、`_Nonnull`・`_Nullable`・`__attribute__((nonnull))`・`returns_nonnull`・`--nonnull`・`--consume` 併用の E2E テストを追加。生成した `.tz` を C ホストとリンクして `NULL` と非 `NULL` の両方で実行検証。**確認**: `node tests/bindgen.mjs target/debug/tsuzuri`。
4. **変更**: 文書。**内容**: `docs/language.md`・`native-interop.md`・`option.md`・`usage.md`・`README.md`。**確認**: T2、`node scripts/check-cli-docs.mjs`、`node scripts/check-docs.mjs <変更したページ>`。

## テスト計画

- `tests/bindgen.rs`:
  - `unannotated_pointers_default_to_maybe`: `struct ctx *open(const struct cfg *c)` → `Maybe<ref Cfg> -> Maybe<Ctx>`。
  - `nonnull_qualifiers_and_attributes_emit_non_maybe`: `_Nonnull`、`__attribute__((nonnull(1)))`、`__attribute__((returns_nonnull))`、`--nonnull`（名前・番号・`return`）で非 `Maybe` になること。
  - `consume_combines_with_nullability`: `--consume close:ctx` が注釈なしなら `Maybe<Ctx>`、`_Nonnull` または `--nonnull close:ctx` なら `Ctx` になること。
  - `invalid_nonnull_annotations_are_rejected`: 未知の関数・スカラー引数・`_Nullable` との衝突が `E2000` になること。
- `tests/bindgen.mjs`: 生成したモジュールで C 関数が `NULL` を返したときに `None` になり、`_Nonnull` な関数が有効ハンドルを返したときに `H` として使えることを native `-O0`・`-O3` で確認。

## ドキュメント

- `docs/language.md`: 「C ヘッダーからの生成」のポインタ変換規則と CLI シノプシスを更新（「NULL は区別できず」の文を廃止）。
- `_tsuzuri/language-reference/compiler/native-interop.md`、`_tsuzuri/language-reference/compiler/option.md`、`_tsuzuri/language-reference/compiler/usage.md`、`README.md`。

## 受け入れ条件

- [x] 注釈なし・`_Nullable` のハンドル／レコードポインタが `Maybe` になり、`_Nonnull`・`nonnull` 属性・`--nonnull` のみが非 `Maybe` になる。
- [x] Clang 21 と Clang 23 でバイト単位で同一の出力になる。
- [x] `node scripts/check-cli-docs.mjs` と `node scripts/check-docs.mjs` が通る。

## 落とし穴

- `struct ctx * const p`（ポインタ自身が `const`）と `const struct ctx *p`（指す先が `const`）のように、`_Nonnull` / `_Nullable` は `*` の後ろ（ポインタ自身）に付く。2 段以上のポインタ（元々 `W2002` で skip）と取り違えないこと。
- GCC/Clang の `__attribute__((nonnull))` を引数番号なしで関数に付けた場合、関数ポインタ引数にも属性が掛かることがあるため、対象はハンドル／レコードポインタ引数だけに限定する。
- 生成物に `Maybe` を含む `extern` が出るため、そのモジュールで到達した関数を WASM でビルドすると W15 の `abi_native_only`（`E1008`）になる（`bindgen` 自体がもともと LP64 native 専用なので整合する）。

## 対象外

- NUL 終端 `const char *` / `const wchar_t *` の自動変換（C の文字列ポインタはエンコーディングと所有権がヘッダーから決まらないため従来どおり `W2002`）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 注釈のない C ポインタ（不透明 struct と `const struct TAG *`）の既定出力を非 `Maybe` から `Maybe<...>` に変更する | C では注釈なしポインタは `NULL` になり得るのが原則であり、NS01 の境界非 Null トラップと組み合わせたときに安全な既定にするため | 要承認 |
| D2 | CLI オプション `--nonnull FUNCTION:TARGET`（`TARGET` は引数名・1 始まり番号・`return`）を追加する（D-55 登録） | `_Nonnull` を持たない既存の C ヘッダーでも、API 文書で非 Null が保証された箇所を手書き修正なしに非 `Maybe` に絞れるようにするため | 要承認 |
| D3 | `_Nonnull` と `__attribute__((nonnull))` / `returns_nonnull` は自動で `Nonnull` と判定する | ヘッダー自身が機械可読に表明している契約をそのまま型に反映するため | 既定案（実装者はこの案に従う） |
| D4 | コールバック引数（関数ポインタ）自身の `_Nullable` はスキップ理由にせず、通常のコールバック型 `(P1 -> ... -> R)` に変換する | 呼び出し側の Tsuzuri が常に有効な関数を渡すことは C 側の `_Nullable` 契約を満たすため | 既定案（実装者はこの案に従う） |
| D5 | 既存の `tests/bindgen.rs`・`tests/bindgen.mjs` の注釈なしポインタ期待値は D1 に合わせて `Maybe` へ更新する | 仕様変更そのものによる期待値更新（GUIDE-LITE §9 の明示例外） | 既定案（実装者はこの案に従う） |
