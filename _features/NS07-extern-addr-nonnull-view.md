# NS07: 非ゼロ外部アドレス型 ExternAddr と extern view の静的非 Null 化

| 項目 | 内容 |
| --- | --- |
| ID | NS07 |
| 優先度 | P2（[W24](_completed/W24-extern-view.md) の `extern view` が生の `i64` を直接受けると、アドレス `0`（NULL）の検査漏れがコンパイル時ではなく実行時トラップになる） |
| 規模 | S（opt-in の `std/ExternAddr.tz` と `W24` の `extern view` 引数型の `ExternAddr` 化、テストと文書） |
| 依存 | W24 |
| 後続 | –（弱い後続: [M06](M06-extern-data-symbols-and-zutf8string.md)。データシンボルの `address` の結果に `ExternAddr` を許す） |
| 状態 | todo |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（opt-in モジュール `ExternAddr` の登録（D-55）と [W24](_completed/W24-extern-view.md) の署名型の改訂。決定事項 D1・D2 が要承認、D3〜D5 は既定案） |
| 手本にする既存実装 | [W20](_completed/W20-zstring.md) の `std/ZString.tz`（opt-in の不透明な std record で不変条件を入口関数 `of` で保証し、ABI・`extern` で内部の 1 スカラーを取り出す形）、`std/Timer.tz`、`src/stdlib.rs` の `opaque_record`・`OPT_IN`・`RESERVED_MODULES` |
| 主な影響ファイル | `std/ExternAddr.tz`（新規）, `src/stdlib.rs`, `src/abi.rs`, `src/check.rs`, `src/llvm_imports.rs`, `src/llvm_layout.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/extern_view.rs`, `tests/extern_view.mjs`, `vsc/src/core.ts`, `docs/language.md`, `_tsuzuri/language-reference/compiler/native-interop.md`, `_tsuzuri/language-reference/built-in-types-and-modules/` |

## 実装者向け早見

- 範囲: opt-in の標準モジュール `ExternAddr`（不透明な Copy レコード `record ExternAddr { raw: i64 }`、不変条件 `raw != 0`）を追加し、`ExternAddr.of_i64 :: i64 -> Maybe<ExternAddr>`・`ExternAddr.to_i64 :: ExternAddr -> i64`・`ExternAddr.offset :: i64 -> ExternAddr -> Maybe<ExternAddr>` を提供する。その上で [W24](_completed/W24-extern-view.md) の `extern view` のアドレス引数を生の `i64` から `ExternAddr` に改め、ホストから届いた `lParam` 等の整数アドレスに対して `0`（NULL）でないことの分岐確認を型システム上で強制する（生ポインタ型やポインタ演算演算子・`unsafe` を入れない [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の範囲はそのまま守る）。
- 最初に読む: ① 仕様 1〜3 ② [W24](_completed/W24-extern-view.md) の仕様 1〜3 ③ [W20](_completed/W20-zstring.md) の不透明 std record の作り方 ④ `src/stdlib.rs` の `OPT_IN`・`RESERVED_MODULES`・`opaque_record`。
- 最初に書くテスト: `tests/extern_view.rs` の `extern_addr_rejects_zero_and_view_requires_extern_addr`。
- 触ってはいけない: `ExternAddr` を名指さないプログラムの IR・診断・内部番号。
- 完了の見分け方: `lparam`（`i64`）をそのまま `read_nmhdr lparam` に渡すと `E1003` になり、`match ExternAddr.of_i64 lparam with | Some addr -> ... | None -> ...` を通したときだけ `read_nmhdr addr` が呼べる。

## 目的

[GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) と [W24](_completed/W24-extern-view.md) は、生ポインタ型 `Ptr<T>` や `*p` 演算子・`unsafe` を入れず、構造体への型付き複写を `extern view` 宣言に閉じ込めた。しかし W24 のアドレス引数が素の `i64` のままだと、単なる件数や座標の `i64` とアドレスの取り違えも、ホストが `0`（NULL）を渡したケース（`WM_CREATE` 以外のメッセージや省略された `lParam`）の事前分岐漏れも型検査で見つけられず、実行時トラップで初めて発覚する。

不透明な単一フィールド値型 `ExternAddr`（`of_i64` が `0` を `None` に弾く）を 1 枚挟むことで、ランタイム表現（`i64` 1 語・Copy・確保ゼロ）を一切重くせずに、アドレスの非 Null 確認をコンパイル時に強制する。

## 着手条件と停止条件

### 着手条件

- [W24](_completed/W24-extern-view.md) Phase 1 が `done` であること（または人間が W24 と同時に実装することを指示した場合）。
- 決定事項 D1・D2 の承認があること。
- `node scripts/repo-kb.mjs brief NS07` を読み、`RESERVED_MODULES` の現在数を控える。

### 停止条件

- **S1** `ExternAddr` を参照しないプログラムの IR・診断が 1 byte でも変わる。
- **S2** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- [W24](_completed/W24-extern-view.md)（状態: `todo`）は `extern view def read_nmhdr :: i64 -> NmHdr` / `extern view def write_minmax :: i64 -> ref MinMaxInfo -> unit` として `i64` を受け取り、呼び出し地点で `address == 0` とアライメント違反を `TrapKind::BoundsCheck` でトラップする設計になっている。
- `src/stdlib.rs` には `ExternAddr` はなく、利用者の `ExternAddr.tz` も `tests/`・`examples/` に存在しない。

## 変えてはいけない意味

- [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) の境界（任意の型 `T` に対する汎用ポインタ型 `Ptr<T>`、自由な逆参照演算子、`unsafe` ブロックは導入しない）。
- `extern view` 呼び出し地点におけるアライメント検査（および多重防御としての非ゼロ検査）。

## 仕様

### 1. モジュール `ExternAddr`（opt-in）

`std/ExternAddr.tz` に不透明レコード `record ExternAddr { raw: i64 } deriving (Eq, Ord, Hash)` を定義する（`Default` は `0` を作ってしまうため導出しない！）。Copy・Send・Sync はフィールド `i64` から自動で真になる。

| 関数 | 型（D-59 準拠：対象 `addr` が最後） | 動作 |
| --- | --- | --- |
| `ExternAddr.of_i64` | `i64 -> Maybe<ExternAddr>` | `0` なら `None`、それ以外なら `Some (ExternAddr { raw })` |
| `ExternAddr.of_i64u` | `i64u -> Maybe<ExternAddr>` | `0i64u` なら `None`、それ以外ならビットを保って `Some` |
| `ExternAddr.to_i64` | `ExternAddr -> i64` | 内部の非ゼロ `i64` を返す |
| `ExternAddr.to_i64u` | `ExternAddr -> i64u` | 内部の非ゼロアドレスを `i64u` で返す |
| `ExternAddr.offset` | `i64 -> ExternAddr -> Maybe<ExternAddr>` | 符号なし 64-bit アドレス空間で `delta` バイトずらし、オーバーフロー・アンダーフローまたは結果が `0` になった場合は `None` を返す |

### 2. `extern view` の署名

- [W24](_completed/W24-extern-view.md) の `extern view` の第 1 引数を `ExternAddr` とする:
  - 読む形: `extern view def read_x :: ExternAddr -> R`
  - 書く形: `extern view def write_x :: ExternAddr -> ref R -> unit`
- 生の `i64` を第 1 引数に書いた場合は `E1008`（`extern_view_signature`）とし、メッセージで `ExternAddr` と `ExternAddr.of_i64` を案内する。
- 生成コードでは `ExternAddr` の単一フィールド `raw`（`i64`）を取り出し、アライメント検査（および多重防御の非ゼロ検査）を通して W24 と同一のロード・ストア命令列を出す。

### 3. `Maybe<ExternAddr>` の niche（任意・設計）

- `ExternAddr` は不透明レコードのため、通常の 2 ケース union としては 16 バイト（`{ i8, { i64 } }`）でも十分に小さくヒープ確保もない（D4: 内部レイアウトの特別扱いは不要）。

## 設計

- `std/ExternAddr.tz`: 純粋な Tsuzuri コードだけで実装できる（C ランタイムも新しい組み込み関数も不要！）。`offset` は `let base = addr.raw as i64u` に対して `delta >= 0` なら `base + (delta as i64u) >= base`、`delta < 0` なら符号なし減算でアンダーフローと結果 `!= 0` を検査する。全関数が `pure`・`@no_alloc`・`@no_trap`・`@terminates` を満たす。
- `src/stdlib.rs`: `SOURCES`・`OPT_IN`・`RESERVED_MODULES`（+1）・`opaque_record` に `"ExternAddr.ExternAddr"` を登録。`extern view` を解析するソースでは `ExternAddr` 名が現れるため自動で opt-in 読み込みされる。
- `src/abi.rs`・`src/llvm_imports.rs`: W24 の `extern view` の第 1 引数判定を `Type::Integer(64, true)` から `abi::extern_addr(ty, types)`（`"ExternAddr.ExternAddr"` の不透明 record）へ変更し、コード生成で field 0 を `extractvalue` して `inttoptr` に渡す。

## 前提とする他チケットのインターフェース

- [W24](_completed/W24-extern-view.md): `extern view def` の構文・検査・コード生成と `DiagnosticKind::ExternViewSignature`。

## 他チケットへ提供するインターフェース

- `ExternAddr`（[W26](W26-com-objects-in-tsuzuri.md) や [W27](W27-win32-package-from-winmd.md) が `lParam` 等を包む補助関数を生成する場合にもそのまま利用できる）。

## 実装手順

1. **変更**: なし（ブランチ `feat/ns07-extern-addr`）。**内容**: `node scripts/repo-kb.mjs brief NS07`、`RESERVED_MODULES` などの現在値を確認。
2. **変更**: `std/ExternAddr.tz`, `src/stdlib.rs`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `vsc/src/core.ts`。**内容**: `ExternAddr` モジュールの追加と単体・doc-test 検証。**確認**: T0、`tsuzuri test --doc std`、T1。
3. **変更**: `src/abi.rs`, `src/check.rs`, `src/llvm_imports.rs`, `tests/extern_view.rs`, `tests/extern_view.mjs`。**内容**: `extern view` の第 1 引数を `ExternAddr` に切り替え、既存の `extern_view` テストを更新。**確認**: T1、`node tests/extern_view.mjs target/debug/tsuzuri`。
4. **変更**: 文書。**内容**: `docs/language.md`・`native-interop.md`・`extern-addr.md`（新規リファレンスページ）と `index.md`。**確認**: `node scripts/check-cli-docs.mjs`、`node scripts/check-docs.mjs <変更したページ>`、T2。

## テスト計画

- `std/ExternAddr.tz` doc-test と `tests/extern_view.rs`:
  - `ExternAddr.of_i64 0` が `None`、`ExternAddr.of_i64 4096` が `Some` になること。
  - `ExternAddr.offset` が正・負のオフセットで正しく動き、`0` への到達や 64-bit 境界折り返しで `None` を返すこと。
  - `Default<ExternAddr>` インスタンスが存在せず `Default.default () : ExternAddr` が `E1005` になること（`0` アドレスを作れないこと！）。
  - `extern view def read_p :: i64 -> Point` が `E1008` になり、`ExternAddr -> Point` が通ること。

## ドキュメント

- `docs/language.md`、`native-interop.md`、`_tsuzuri/language-reference/built-in-types-and-modules/extern-addr.md`、`_tsuzuri/language-reference/index.md`。

## 受け入れ条件

- [ ] `ExternAddr` の構築手段が `of_i64` / `of_i64u` / `offset` だけに閉じ、`0` を持つ `ExternAddr` が絶対に作れない（`Default` も持たない）。
- [ ] `extern view` が `ExternAddr` を要求し、アライメント違反時は従来どおりトラップする。
- [ ] `ExternAddr` を使わないプログラムの IR が 1 byte も変わらない。

## 落とし穴

- `ExternAddr` に決して `deriving (Default)` を付けてはならない（`Default` を付けると `raw: 0` が作れてしまい不変条件が崩れる）。テストで `Default<ExternAddr>` が `E1005` になることを必ず固定する。
- `tests/std_argument_order.rs` の `MODULES` に `("ExternAddr", subject(&["ExternAddr"]))` を追加し、`offset` の引数順が `delta` → `addr`（対象が最後、D-59）であることを守る。

## 対象外

- 型引数付きの `ExternAddr<'T>`（それは事実上の生ポインタ型 `Ptr<T>` になり D-63 に反するため導入しない。型付けは `extern view` の宣言側が担う）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | opt-in の不透明値型モジュール `ExternAddr` を追加する（D-55 登録） | 生ポインタ型や `unsafe` を入れずに「`0` でない検証済みアドレス」を `i64` から型で区別するため | 要承認 |
| D2 | [W24](_completed/W24-extern-view.md) の `extern view` のアドレス引数を `i64` から `ExternAddr` に改める | `extern view` を呼ぶすべての箇所で NULL アドレスの分岐処理を静的に強制するため | 要承認 |
| D3 | `ExternAddr.offset` はラップアラウンドさせず、境界超えと `0` 到達を `None` にする | 負オフセットや巨大オフセットで偶然 `0` やラップした番地を作るのを防ぐため | 既定案（実装者はこの案に従う） |
| D4 | `Maybe<ExternAddr>` の専用 LLVM niche は足さず通常の共用体レイアウトとする | `{ i8, { i64 } }` はヒープ確保なしの値であり、`UnionLayout` に std 固有の特例を増やさない方が単純で堅牢なため | 既定案（実装者はこの案に従う） |
| D5 | 呼び出し地点の `0` ガードもアライメント検査と一緒に残す | 多重防御（1 命令の `icmp` でコストは無視でき、万一の ABI 経由破損も止める） | 既定案（実装者はこの案に従う） |
