# E17: `IO` の入出力関数名の統一（`readln`／`writeln`／`write_err`／`write_errln` と `try_` 版）

| 項目 | 内容 |
| --- | --- |
| ID | E17 |
| 優先度 | P1（標準入出力の基本 API の命名統一と重複別名の解消） |
| 規模 | M（`std/IO.tc` の改名と `E1040` の機械的移行は小さいが、`std/`・テスト・適合性スイート・KG golden・`guide/`・`vsc/`・言語リファレンスなど約 160 ファイルの呼び出し箇所と文書の一括更新がある） |
| 依存 | E02, G23, G24, G34, G35（すべて done） |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-20（利用者の依頼。HEAD `295ffcb4` で確認） |
| 承認 | 承認済み（2026-11-20 の利用者の依頼）。決定事項 D1〜D4 は既定案 |
| 改善する劣位 | 同じ動作に 2 つの名前（`IO.write_line` と別名 `IO.writeln`）があり、行単位の `_line` と `ln`、標準エラーの `_error` と `_err` の表記が揃っていない（D-50 の「同じ意味に複数の書き方がある」の解消） |
| 手本にする既存実装 | TZ03（`DataChannel` への改名と `E1040` + `tsuzuri fix` による機械的移行: `src/check.rs` の `removed_feature`・`removed_name`・`ExprKind::Field` の回復、`src/diagnostic_kinds.rs`、`src/explain/codes/E1040.md`、`tests/removed_features.rs`）、G34（`tests/std_examples.rs` の `FLOORS` と `tsuzuri test --doc std`）、G35（`scripts/gen-docs.mjs` と `scripts/check-cli-docs.mjs`） |
| 主な影響ファイル | `std/IO.tc`, `std/Array.tz`, `std/Debug.tz`, `std/Dir.tz`, `std/Env.tz`, `std/File.tz`, `std/Gpu.tz`, `std/List.tz`, `std/Net.tz`, `std/Process.tz`, `std/Random.tz`, `std/Time.tz`, `src/diagnostic_kinds.rs`, `src/check.rs`, `src/explain/codes/E1040.md`, `src/hints.rs`, `src/overflow.rs`, `src/stdlib.rs`, `src/repl.rs`, `src/llvm_split.rs`, `src/ownership.rs`, `src/kg/suggest.rs`, `tests/removed_features.rs`, `tests/std_examples.rs`, `tests/audit.rs`, `tests/kg_query.rs`, `tests/io.mjs`, `tests/fixtures/capabilities/std-table.txt`, `tests/fixtures/kg_shop/Report.tz`, `tests/fixtures/kg_shop_head/Cart.tz`, `tests/fixtures/kg_shop_head/Report.tz`, `tests/golden/kg/queries/show_io.txt`, `tests/golden/kg/queries/callees_main_3.txt`, `tests/golden/kg/shop/brief/L2/Acme.Shop.Report.print_total.md`, `tests/golden/kg/shop/edges.jsonl`, `tests/golden/kg/shop/facts/Main.jsonl`, `tests/golden/kg/shop/facts/Report.jsonl`, `tests/fixtures/mcp/tool-examples.json`, `guide/card.md`, `vsc/src/test/extension.test.ts`, `vsc/README.md`, `vsc/Development.md`, `examples/io/Main.tz`, `scripts/toolchain/smoke.mjs`, `_tsuzuri/language-reference/built-in-types-and-modules/io.md`, `_tsuzuri/language-reference/compiler/diagnostics.md`, `docs/language.md`, `docs/architecture.md`, `README.md`, `_features/GUIDE.md`, `_features/README.md` |

## 実装者向け早見

1. **範囲**: `std/IO.tc` の公開入出力関数名を「改行付きは `ln`（下線なし）、標準エラーは `_err`、`try_` 版とトラップ版は 1 対 1」に統一し、`write_line` と `writeln` の二重定義を `writeln` 1 つにまとめる。旧関数名 8 個（`try_read_line`・`read_line`・`try_write_line`・`try_write_error`・`try_write_error_line`・`write_line`・`write_error`・`write_error_line`）は `E1040`（kind `renamed_io_function`）で新しい名前を示し、`tsuzuri fix`（`certain`）で機械的に書き換えられるようにする。リポジトリ内の全呼び出し・テスト・golden・ガイド・VS Code 拡張・文書を新名称に揃える。
2. **触ってはいけない**: `IO` の意味・型・評価順序・所有権・フラッシュ、std 専用の内部組み込み `IO.__read_line`・`IO.__write`、ランタイム C 関数 `tsuzuri_io_read_line`・`tsuzuri_io_write`、WebAssembly のホスト import 名 `tsuzuri_io.read_line`・`tsuzuri_io.write`（`docs/stability.md` の公開 ABI。D3）、Rust 内部の関数名（`mcp::rpc::read_line`・`server::read_line_limited` など）。単純な全文一括置換（`read_line` → `readln`）はこれらを壊すので絶対に行わない（「落とし穴」参照）。
3. **最初に読む**: ① 本チケットの「仕様」1〜3 と「落とし穴」、② `std/IO.tc`（213 行）、③ `src/check.rs` の `removed_feature`・`removed_name`（6029〜6165 行）と `ExprKind::Field` の回復（13166〜13220 行）、④ `tests/removed_features.rs`（1〜185 行）、⑤ `tests/std_examples.rs`（`("IO", 14)` → `13`）。
4. **進め方**: 手順 1（`std/IO.tc` + `E1040` + `std/` の doc 例 + `removed_features.rs`・`std_examples.rs`）→ 手順 2（`src/` 単体テスト・Rust 統合テスト・fixture・適合性テスト・KG golden・`std-table.txt`）→ 手順 3（Node E2E・`examples/`・`benchmarks/`・`scripts/`・`guide/`・`vsc/`）→ 手順 4（言語リファレンス・`docs/`・`README.md`・生成表・未完了チケット）。1 チケット 1 ブランチ・1 PR。
5. **完了の見分け方**: `git grep -nE '\bIO\.(try_read_line|read_line|try_write_line|try_write_error_line|try_write_error|write_line|write_error_line|write_error)\b'` が、`E1040` の実装・テスト・説明ページ（`src/check.rs`・`src/explain/codes/E1040.md`・`tests/removed_features.rs`・`diagnostics.md`・`docs/language.md` の `E1040` 節・`io.md` の「以前の関数名」の表）と本チケット・過去の履歴（`_features/_completed/`・`_specs/_archive/`）以外で **0 件**になり、T1・関連 Node E2E・`tsuzuri test --doc std`・文書検査がすべて通る。

## 目的

現在の `std/IO.tc` には次の不統一がある。

1. `IO.write_line` とその別名 `IO.writeln` が併存しており、文書・例・テスト・AI ガイドの間で表記が割れている（`tsuzuri new` や `guide/` は `IO.writeln`、言語リファレンスや `std/` の doc 例の大半は `IO.write_line`）。
2. 行単位の操作が `writeln` だけ `ln` で、ほか（`read_line`・`try_read_line`・`try_write_line`・`write_error_line`・`try_write_error_line`）は `_line` になっている。
3. 標準エラー出力も `_error` と長く、`try_write_err`／`try_write_errln`／`write_err`／`write_errln` に揃えると、行の `ln`・標準エラーの `_err`・失敗を値で返す `try_` の 3 軸が直交した短い名前になる。

本チケットは `IO` の入出力関数をこの規則に統一して別名を廃し、旧名を書いた既存コードには `E1040` と `tsuzuri fix` による確かな自動書き換えを提供したうえで、リポジトリ全体のコード・テスト・文書を更新する。

## 着手条件と停止条件

### 着手条件

- 依存チケット（E02・G23・G24・G34・G35）はすべて `done`。
- 着手前に `gh run list --branch main --workflow ci.yml --limit 1` で `main` の基準を確認し、次のコマンドで改名前の参照件数を控える。
  ```sh
  git grep -c '\bIO\.\(try_read_line\|read_line\|try_write_line\|try_write_error_line\|try_write_error\|write_line\|write_error_line\|write_error\)\b'
  ```

### 停止条件

次のどれかに当てはまったら、即興で回避せず作業を止めて報告する（GUIDE §13.1）。

- **S1**: `IO` を使わないプログラムの生成 IR、または `tsuzuri_io.read_line`・`tsuzuri_io.write` のホスト ABI／WASM import 名が変わる。
- **S2**: 利用者が自分の名前空間に定義した `IO` モジュール（例: `namespace App` の `App/IO.tz`）に対する未定義メンバー参照が `E1002`（`unknown_member`）ではなく `E1040` になる。
- **S3**: `tsuzuri fix` を旧名のプログラムに適用したあと、1 ラウンドで全箇所が新名に置き換わらない、または再検査でエラーが残る。
- **S4**: 既存のテストの期待値を、関数名の変更とそれに伴う文字位置（span・列番号）・呼び出し経路（KG の `via`）以外の理由で変えないと通らない。
- **S5**: `node scripts/check-docs.mjs`・`node scripts/gen-docs.mjs --check`・`node scripts/check-cli-docs.mjs`・`node scripts/check-guide.mjs`・`node scripts/check-spec.mjs`・`node scripts/check-guide-lite.mjs` のいずれかが通らない。

## 現状（HEAD `295ffcb4` で確認）

- **`std/IO.tc`（213 行）**: 公開の小文字関数が 14 個ある: `pure`・`bind`・`map` と、入出力の 11 個（`try_read_line`・`read_line`・`try_write`・`try_write_line`・`try_write_error`・`try_write_error_line`・`write`・`write_line`・`writeln`・`write_error`・`write_error_line`）。`writeln` は `write_line` を呼ぶだけの別名（191〜192 行）。`tests/std_examples.rs` の `FLOORS` は `("IO", 14)`（44 行）。
- **内部組み込みとランタイム ABI**: `src/check.rs`（2526〜2527 行）の `Builtin::IOReadLine`（`"IO.__read_line"`）と `Builtin::IOWrite`（`"IO.__write"`）は std 専用の内部組み込みで、利用者コードからの参照は `E1022` になる。`src/llvm_io.rs`（40〜65 行）・`src/llvm.rs`（226 行）・`src/runtime/io.c`（95・118 行）・`src/runtime/os-wasi.c`（835・869 行）が C シンボル `tsuzuri_io_read_line`／`tsuzuri_io_write` と WASM import `tsuzuri_io.read_line`／`tsuzuri_io.write` を定義・参照している。`src/capabilities.rs`（434 行）は `"IO.__read_line" | "IO.__write" => Stdio` から std 関数の権限を導出しており、`tests/fixtures/capabilities/std-table.txt`（48〜58 行）が公開関数ごとの導出結果を固定している。
- **既存の `E1040` 基盤**: `src/check.rs` の `removed_feature`・`removed_name`（6029〜6165 行）と `ExprKind::Field` の分岐（13166〜13220 行）が、未解決の `Module.member` に対して `E1040` と `Applicability::Certain` の `Edit` を出し、`self.recovering` のときは置換後の式で型検査を続けて 1 回の解析ですべての旧名を報告する。ただし現在の `removed_feature` は `"Channel"`・`"Task"`・`"Async"`・`"Net"` を裸のモジュール名だけで照合しており、`std::IO.write_line` のような `std::` 修飾と、利用者の名前空間内の同名モジュール（`App/IO.tz` → `module_path` が `"App.IO"` を返す）の区別は `IO` 追加時に整える必要がある。
- **リポジトリ内の旧名参照**: `IO.(try_read_line|read_line|try_write_line|try_write_error_line|try_write_error|write_line|write_error_line|write_error)` は全体で 162 ファイル・1,022 箇所にある（`std/` 12 ファイル、`src/` 6 ファイル、`tests/` 87 ファイル、`_tsuzuri/language-reference/` 34 ファイル、`docs/` 3 ファイル、`README.md`、`examples/` 4 ファイル、`benchmarks/` 6 ファイル、`guide/card.md`、`vsc/` 3 ファイル、`scripts/toolchain/smoke.mjs`、未完了チケット `_features/` 2 ファイル・`_perfs/` 2 ファイル）。

## 変えてはいけない意味

- `IO<T>` 型、`IO.Error`（`ReadFailed`・`WriteFailed`・`InvalidEncoding`）、`IO.pure`・`IO.bind`・`IO.map`、コンピュテーション式ビルダー操作（`Return`・`ReturnFrom`・`Bind`・`Using`・`Zero`・`Delay`・`Combine`・`For`・`While`・`MergeSources`・`BindReturn`）、非公開の `IO.run`・`IO.try_output` の型と意味。
- 入出力の動作: LF／CRLF の扱い、空行と EOF の区別、孤立サロゲート・不正 UTF-8 の `InvalidEncoding`、途中の NUL 保持、書き込みごとのフラッシュ、`try_` なしの失敗時トラップ、引数の順序（D-59）、所有権と `Capture` 制約、`stdio` 権限の導出。
- 内部組み込み `IO.__read_line`・`IO.__write` と、公開 ABI（`tsuzuri_io_read_line`・`tsuzuri_io_write`、WebAssembly import `tsuzuri_io.read_line`・`tsuzuri_io.write`、`--wasm-host wasi` のリンク）。
- 利用者が自分の名前空間で定義した `IO` モジュール（`namespace App` の `App/IO.tz`）の名前解決と診断コード（未定義メンバーは従来どおり `E1002`）。

## 仕様

### 1. `std/IO.tc` の公開入出力関数

依頼の 8 行に加え、`std/IO.tc` 末尾（201〜212 行）にある標準エラー出力のトラップ版 2 関数（`write_error`・`write_error_line`）も同じ規則（`_error` → `_err`、`_line` → `ln`、`try_<name>` と `<name>` の 1 対 1 対応）で改名する（決定事項 D1）。`write_line` とその別名だった `writeln` は `writeln` 1 つに統合する。

| 現在 | 更新後 | シグネチャ（`std/IO.tc`、edition 2027） | 行き先・改行・失敗時 |
| --- | --- | --- | --- |
| `try_read_line` | `try_readln` | `own unit -> IO<Result<Maybe<string>, Error>>` | 標準入力から 1 行（LF／CRLF 除去）。失敗は `Error` |
| `read_line` | `readln` | `own unit -> IO<Maybe<string>>` | 標準入力から 1 行。失敗でトラップ |
| `try_write` | `try_write`（維持） | `(Display<'a>, Capture<'a>) => own 'a -> IO<Result<unit, Error>>` | 標準出力・改行なし。失敗は `Error` |
| `try_write_line` | `try_writeln` | `(Display<'a>, Capture<'a>) => own 'a -> IO<Result<unit, Error>>` | 標準出力・末尾 LF。失敗は `Error` |
| `try_write_error` | `try_write_err` | `(Display<'a>, Capture<'a>) => own 'a -> IO<Result<unit, Error>>` | 標準エラー・改行なし。失敗は `Error` |
| `try_write_error_line` | `try_write_errln` | `(Display<'a>, Capture<'a>) => own 'a -> IO<Result<unit, Error>>` | 標準エラー・末尾 LF。失敗は `Error` |
| `write` | `write`（維持） | `(Display<'a>, Capture<'a>) => own 'a -> IO<unit>` | 標準出力・改行なし。失敗でトラップ |
| `write_line` + 別名 `writeln` | `writeln`（統合） | `(Display<'a>, Capture<'a>) => own 'a -> IO<unit>` | 標準出力・末尾 LF。失敗でトラップ（本体は `map Result.get (try_writeln value)`） |
| `write_error` | `write_err`（D1） | `(Display<'a>, Capture<'a>) => own 'a -> IO<unit>` | 標準エラー・改行なし。失敗でトラップ（本体は `map Result.get (try_write_err value)`） |
| `write_error_line` | `write_errln`（D1） | `(Display<'a>, Capture<'a>) => own 'a -> IO<unit>` | 標準エラー・末尾 LF。失敗でトラップ（本体は `map Result.get (try_write_errln value)`） |

- `std/IO.tc` の公開小文字関数は 14 個 → **13 個**（`pure`・`bind`・`map` + 上記 10 個）になる。すべての公開小文字関数と `Bind` が実行可能な `///` の例を持つ状態を保つ。
- `writeln` の `///` コメントは、旧 `write_line` の説明（`An action that writes the display form of value and an LF to standard output; it traps where try_writeln gives an Error.`）を引き継ぎ、「`write_line` の別名」という記述をなくす。

### 2. 旧関数名に対する診断（`E1040`）と `tsuzuri fix`（D2）

標準ライブラリの `std::IO` モジュールに対して次の 8 つの旧関数名を書いたとき、`E1040`（新しい kind `renamed_io_function`）を報告し、`Applicability::Certain` の修正案を付ける。

| 書いた旧名（`std::IO.` 修飾も同様） | 新名 | メッセージ |
| --- | --- | --- |
| `IO.try_read_line` | `IO.try_readln` | `'IO.try_read_line' is now 'IO.try_readln'` |
| `IO.read_line` | `IO.readln` | `'IO.read_line' is now 'IO.readln'` |
| `IO.try_write_line` | `IO.try_writeln` | `'IO.try_write_line' is now 'IO.try_writeln'` |
| `IO.try_write_error` | `IO.try_write_err` | `'IO.try_write_error' is now 'IO.try_write_err'` |
| `IO.try_write_error_line` | `IO.try_write_errln` | `'IO.try_write_error_line' is now 'IO.try_write_errln'` |
| `IO.write_line` | `IO.writeln` | `'IO.write_line' is now 'IO.writeln'` |
| `IO.write_error` | `IO.write_err` | `'IO.write_error' is now 'IO.write_err'` |
| `IO.write_error_line` | `IO.write_errln` | `'IO.write_error_line' is now 'IO.write_errln'` |

- **`std::` 修飾と空白の保持**: `std::IO.write_line` と書いたときのメッセージは `'std::IO.write_line' is now 'std::IO.writeln'`、修正案の title は `write 'std::IO.writeln'`。診断の `span` は修飾名全体（`value.span.start..field.span.end`、既存の `removed_name` と同じ）とし、`Edit` は関数名の識別子だけ（`field.span`）を新しいメンバー名に置き換える（2026-11-21 の実装で確定。型検査器はソースの文字列を持たないので、起票時の案「`value.span.start..field.span.start` の元の綴りに新名を連結して修飾名全体を置き換える」と結果が同じで、間のコメントや改行も保つこの形にした）。これにより `IO.write_line`・`std::IO.write_line`・`IO . write_line`・`(IO.write_line)`・`"x" |> IO.write_line` のすべてで元の修飾と空白を保ったまま `tsuzuri fix` が 1 ラウンドで動く。
- **`help`**: `"the line and stderr functions of IO use 'ln' and 'err' (IO.readln, IO.writeln, IO.write_err, IO.write_errln, and their 'try_' forms)"`。
- **利用者定義モジュールとの分離**: `E1040` にするのは、`self.names.module_path(self.module, &spelled) == Some("IO")`（つまり参照先モジュールが標準ライブラリの `std::IO` に解決したとき）に限る。`namespace App` に `App/IO.tz` があるときの `IO.write_line` や `App::IO.write_line` は `module_path` が `Some("App.IO")` になるため `E1040` にせず、従来どおり `E1002`（`unknown_member`）にする。（2026-11-21 の実装で確認: `Tsuzuri.toml` の `namespace` が `App` のプロジェクトでは、`App/IO.tz` は既定の名前空間のモジュールなので `E1011`（`module name 'IO' is reserved`）になる。名前空間 `App` を宣言した `App/IO.tz` が成り立つのは既定の名前空間が別のときで、`tests/removed_features.rs` はその形を `analyze_modules` で検査する。利用者向けの文書の例は、どのプロジェクトでも成り立つ名前空間 `App::Tools` の `Tools/IO.tz` にした）
- **エラー回復**: `self.recovering` のときは、モジュール式 `(**value).clone()` を保ったままメンバー識別子だけを新名に差し替えた `ExprKind::Field` で型検査を続行し、1 回の `tsuzuri check` でファイル内のすべての旧名を報告する。

### 3. ヒント DB（`src/hints.rs`）

- `id: "print-macro"`（`println!("x")` に対するヒント）の `title`・`help`・`good` を `IO.write_line` から `IO.writeln` に更新する。

## 設計（書き換えの一覧）

### 1. 標準ライブラリ（`std/`）

| ファイル | 変更内容 |
| --- | --- |
| `std/IO.tc` | 仕様 1 のとおり 8 関数を改名し、`write_line` を削除して `writeln` に統合する。`pure`・`bind`・`map`・`Bind`・`try_write` を含むすべての `///` doc 例の旧名を新名に更新する |
| `std/{Array,Debug,Dir,Env,File,Gpu,List,Net,Process,Random,Time}.tz`（11 ファイル） | `///` doc 例の中の `IO.write_line` を `IO.writeln` に更新する |

### 2. コンパイラ（`src/`）

| ファイル | 変更内容 |
| --- | --- |
| `src/diagnostic_kinds.rs` | `since "0.1.0"` の `// E1040` 群に、`(code, id)` 順で `(RenamedIoFunction, "renamed_io_function", "E1040")` を追加（`RenamedToDataChannel` の直前） |
| `src/check.rs` | ① `IO` 用の表 `renamed_io_function`（8 つの旧名 → 新名。`removed_feature` には足さない。型の位置の `Names::type_head` は `removed_feature` を聞くので、そこで `IO.write_line` を型として報告しないため）。② `ExprKind::Field` で、`IO`／`std::IO` の旧名は `self.names.module_path(self.module, &spelled) == Some("IO")` のときだけ `E1040` を出す（`Checker::renamed_io_function`）。診断の `span` は修飾名全体、確かな修正案の `Edit` は関数名の識別子（`field.span`）だけ（仕様 2）。回復時は左辺のモジュールの式（`value.clone()`）を保って新しい名前で検査を続け、`std::IO` 修飾もそのまま通す |
| `src/explain/codes/E1040.md` | `What it covers`・`Fixes` に `renamed_io_function`（`IO.readln`・`IO.writeln`・`IO.write_err`・`IO.write_errln` と `try_` 版への改名、`tsuzuri fix` 対応）を追記。ページは `tests/explain.rs` の `CODE_PAGE_LIMIT`（1500 字）に既に近かった（1497 字）ので、任意の `See also`（kind の一覧）を外し、ほかの行を短くした（1496 字。`tsuzuri explain E1040` は kind の一覧を自分で出す。A24 の `E1041` と同じ扱い） |
| `src/hints.rs` | `"print-macro"`（435〜444 行）の `IO.write_line` を `IO.writeln` に更新 |
| `src/overflow.rs` | 251 行のコメント内の `IO.write_error "prefix"` を `IO.write_err "prefix"` に更新 |
| `src/{stdlib,repl,llvm_split,ownership}.rs`, `src/kg/suggest.rs` | モジュール内単体テストのソース文字列・候補文字列の `IO.read_line`／`IO.write_line` を `IO.readln`／`IO.writeln` に更新 |

### 3. テスト・golden・ツール・ベンチマーク・例

| ファイル群 | 変更内容 |
| --- | --- |
| `tests/std_examples.rs` | `FLOORS` の `("IO", 14)` を `("IO", 13)` に更新し、理由（`E17`: `write_line` と `writeln` の統合で公開小文字関数が 14 → 13）をコメントに添える |
| `tests/removed_features.rs` | `IO` の旧名 8 個すべて、`std::IO.write_line`、空白入り `IO . write_line`、括弧 `(IO.write_line)`、パイプ `"a" \|> IO.write_line`、1 ファイル内の複数箇所の一括報告、`tsuzuri fix` による自動書き換え、および `namespace App` の利用者モジュール `App/IO.tz` が `E1002` のままになるテストを追加 |
| `tests/fixtures/capabilities/std-table.txt`, `tests/capabilities.rs` | `IO.*` の 11 行を、新名 10 行（辞書順: `IO.readln`・`IO.try_readln`・`IO.try_write`・`IO.try_write_err`・`IO.try_write_errln`・`IO.try_writeln`・`IO.write`・`IO.write_err`・`IO.write_errln`・`IO.writeln`、すべて `: stdio`）に更新。表は 89 行 → 88 行になるので、`std_table_file_lists_the_89_functions` を `..._88_functions` にする |
| `tests/audit.rs` | テスト入力と期待出力（テキストと `--json`）の `IO.read_line`・`IO.write_line` を `IO.readln`・`IO.writeln` に更新 |
| `tests/kg_query.rs`, `tests/fixtures/kg_shop/Report.tz`, `tests/fixtures/kg_shop_head/{Cart,Report}.tz`, `tests/golden/kg/**` | `Report.tz` などの `IO.write_line` を `IO.writeln` に更新し、`kg_query.rs` のクエリと補完候補（`IO.write_lin` → `IO.writel`、`IO.writ_line` → `IO.writln` など）および `tests/golden/kg/` の 6 ファイル（`show_io.txt`・`callees_main_3.txt`・`Acme.Shop.Report.print_total.md`・`edges.jsonl`・`facts/Main.jsonl`・`facts/Report.jsonl`、3 文字短縮に伴う `span` と `column` と `via` の変化を含む）を更新 |
| `tests/io.mjs` | Tsuzuri ソース文字列内の `IO.read_line`・`IO.try_read_line`・`IO.write_line`・`IO.write_error_line` を新名に更新し、`IO.try_writeln`・`IO.try_write_err`・`IO.try_write_errln`・`IO.write_err` も網羅する。**WASM import 検査の `"tsuzuri_io.read_line"` と JS ホストオブジェクトの `read_line(out)` は変更しない** |
| その他の `tests/*.rs`（20 ファイル）・`tests/*.mjs`（15 ファイル）・`tests/fixtures/**`（12 ファイル）・`tests/conformance/**`（27 ファイル） | テスト用 Tsuzuri ソース内の旧名を新名に更新 |
| `guide/card.md`, `tests/fixtures/mcp/tool-examples.json` | `let! line = IO.read_line ()` を `let! line = IO.readln ()` に更新 |
| `vsc/src/test/extension.test.ts`, `vsc/{README,Development}.md` | 補完テストの期待ラベル `'write_line'` を `'writeln'` に、説明文の `IO.read_line` を `IO.readln` に更新 |
| `examples/{currying,functional,hello,io}/Main.tz`, `benchmarks/**`, `scripts/toolchain/smoke.mjs` | 旧名を新名に更新 |

### 4. 文書と台帳

| ファイル群 | 変更内容 |
| --- | --- |
| `_tsuzuri/language-reference/built-in-types-and-modules/io.md` | ポイント・本文・対応表・公開 API 表・例・まとめを新名 10 関数に更新（「`IO.write_line` の別名が `IO.writeln`」の記述を削除し、`write_err`／`write_errln`／`readln` と `try_` 版を記載。WASM 節の `tsuzuri_io.read_line`／`tsuzuri_io.write` とホスト側の `read_line` の説明はそのまま保つ） |
| `_tsuzuri/language-reference/` の他 33 ページ | コード例・説明文の `IO.write_line`・`IO.read_line`・`IO.write_error`・`IO.write_error_line` を新名に更新。`compiler/diagnostics.md` の `E1040` の説明に `renamed_io_function` を追記し、`node scripts/gen-docs.mjs` で `diagnostic-kinds` 生成ブロックを更新 |
| `docs/language.md`, `docs/architecture.md`, `README.md` | `IO` の仕様・例・`E1040` の表を更新（`docs/architecture.md` と `docs/language.md` の `IO.__read_line` および `tsuzuri_io.read_line` はそのまま保つ） |
| `_features/GUIDE.md` | D-55 の `E1040` の行に `E17`（旧 `IO` 関数名の改名）を追記 |
| 未完了チケット（`_features/A34-store-check-safe-rejections.md`, `_features/G39-stale-editor-diagnostics.md`, `_perfs/PB08-lsp-refresh-cost.md`, `_perfs/README.md`） | 未完了チケット内の再現コード・本文にある `IO.write_line`・`IO.read_line` を新名に揃える（`f64ca392` と同じ方針。`_features/_completed/` と `_specs/_archive/` は履歴なので変更しない） |

## 実装手順

### 手順 1: `std/IO.tc` の改名と `E1040`（`renamed_io_function`）の実装

- **変更**: `std/IO.tc`, `std/{Array,Debug,Dir,Env,File,Gpu,List,Net,Process,Random,Time}.tz`, `src/diagnostic_kinds.rs`, `src/check.rs`, `src/explain/codes/E1040.md`, `src/hints.rs`, `src/overflow.rs`, `tests/std_examples.rs`, `tests/removed_features.rs`
- **内容**:
  1. `std/IO.tc` の入出力関数と `///` 例を仕様 1 のとおり更新し、他 11 個の `std/*.tz` の `///` 例の `IO.write_line` を `IO.writeln` に直す。
  2. `src/diagnostic_kinds.rs` に `RenamedIoFunction`（`"renamed_io_function"`, `"E1040"`）を足し、`src/check.rs`・`src/explain/codes/E1040.md`・`src/hints.rs` を仕様 2・3 のとおり更新する。
  3. `tests/std_examples.rs` の `("IO", 14)` を `("IO", 13)` に更新し、`tests/removed_features.rs` に旧 `IO` 関数 8 個の診断・回復・`tsuzuri fix`・利用者モジュール分離のテストを足す。
- **確認**:
  ```sh
  cargo test --locked --test std_argument_order --test std_examples --test std_tags --test removed_features --test hints --test explain
  ```

### 手順 2: `src/` 単体テストと Rust 統合テスト・fixture・適合性スイート・KG golden の更新

- **変更**: `src/{stdlib,repl,llvm_split,ownership}.rs`, `src/kg/suggest.rs`, `tests/*.rs`（該当 22 ファイル）, `tests/fixtures/**`, `tests/conformance/**`, `tests/golden/kg/**`
- **内容**:
  1. `src/` 内の単体テストと `tests/` 配下の `.rs`・`.tz` の旧 `IO.*` 呼び出しを新名に更新する。
  2. `tests/fixtures/capabilities/std-table.txt`、`tests/audit.rs`、`tests/kg_query.rs` と `tests/golden/kg/` の 6 ファイルを更新する。
  3. `src/polymorph.rs` の `honors_the_exact_specialization_limit` も通ることを確認する。
- **確認**:
  ```sh
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  node scripts/slot.mjs -- cargo nextest run --locked
  ```

### 手順 3: Node E2E・doc-test・`examples/`・`benchmarks/`・`guide/`・`vsc/`・`scripts/` の更新

- **変更**: `tests/*.mjs`（`io.mjs` ほか該当 16 ファイル）, `examples/**`, `benchmarks/**`, `scripts/toolchain/smoke.mjs`, `guide/card.md`, `tests/fixtures/mcp/tool-examples.json`, `vsc/src/test/extension.test.ts`, `vsc/{README,Development}.md`
- **内容**:
  1. `tests/io.mjs` で新名 10 関数すべての動作（native／WASM × `-O0`／`-O3`、対話入力、エラー系、トラップ系）を検査し、`tsuzuri_io.read_line` import は変えない。
  2. 残りの `.mjs` スイート、`examples/`、`benchmarks/`、`scripts/toolchain/smoke.mjs`、`guide/card.md`、`tool-examples.json`、`vsc/` を更新する。
- **確認**:
  ```sh
  cargo build --locked
  target/debug/tsuzuri test --doc std
  node tests/io.mjs target/debug/tsuzuri
  node tests/os.mjs target/debug/tsuzuri
  node tests/repl.mjs target/debug/tsuzuri
  node tests/script.mjs target/debug/tsuzuri
  node tests/tzroutine.mjs target/debug/tsuzuri
  node scripts/conformance.mjs --bin target/debug/tsuzuri
  TSUZURI_BIN=target/debug/tsuzuri node scripts/check-guide.mjs
  ```

### 手順 4: 言語リファレンス・仕様書・生成ブロック・台帳の更新と全体検証

- **変更**: `_tsuzuri/language-reference/**`（`io.md`・`diagnostics.md` ほか全 34 ページ）, `docs/language.md`, `docs/architecture.md`, `README.md`, `_features/GUIDE.md`, `_features/{A34,G39}*.md`, `_perfs/{PB08*,README.md}`
- **内容**:
  1. 言語リファレンス・`docs/language.md`・`docs/architecture.md`・`README.md`・`_features/GUIDE.md`（D-55）・未完了チケットを更新する。
  2. `node scripts/gen-docs.mjs --bin target/debug/tsuzuri` を実行して `diagnostics.md` の `diagnostic-kinds` ブロックを再生成する。
  3. 旧名の残存検査（早見 5 の `git grep`）と、文書・仕様・KB の全チェッカーを通す。
- **確認**:
  ```sh
  node scripts/gen-docs.mjs --check --bin target/debug/tsuzuri
  node scripts/check-cli-docs.mjs --bin target/debug/tsuzuri
  node scripts/check-spec.mjs
  TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs
  node scripts/repo-kb.mjs build && node scripts/repo-kb.mjs verify
  node scripts/check-guide-lite.mjs
  ```

## テスト計画

- **単体・診断テスト（`tests/removed_features.rs`）**:
  - 旧名 8 個（`IO.try_read_line`・`IO.read_line`・`IO.try_write_line`・`IO.try_write_error`・`IO.try_write_error_line`・`IO.write_line`・`IO.write_error`・`IO.write_error_line`）がそれぞれ `E1040`（kind `renamed_io_function`）になり、正しいメッセージ・`help`・`Applicability::Certain` の置換を持つこと。
  - `std::IO.write_line`・`IO . write_line`・`(IO.write_line)`・`"x" |> IO.write_line` でも修飾と空白を保った置換になること。
  - 1 つの関数内に複数の旧名を並べたとき、エラー回復で全箇所が 1 回の解析で報告され、`tsuzuri fix` 1 回の実行でプロジェクト全体が新名に移行して `tsuzuri check` が通ること。
  - `namespace App` に `App/IO.tz` を置いたプロジェクトで `IO.write_line`／`App::IO.write_line` を呼んだとき、`E1040` ではなく `E1002` になること。
- **std 規約・doc-test**:
  - `tests/std_argument_order.rs`（`IO` の引数順序）、`tests/std_examples.rs`（`IO` の 13 関数 + `Bind`）、`tests/std_tags.rs`、`tsuzuri test --doc std`（`-O0`／`-O3`）。
- **E2E（`tests/io.mjs` ほか）**:
  - `readln`・`try_readln`・`write`・`try_write`・`writeln`・`try_writeln`・`write_err`・`try_write_err`・`write_errln`・`try_write_errln` の 10 関数について、native／WASM × `-O0`／`-O3` で標準入力（EOF・空行・CRLF・単独 CR・NUL・長大行・不正 UTF-8）、標準出力、標準エラー、書き込み失敗、孤立サロゲート、遅延評価（未使用アクションが副作用を起こさないこと）、WASM import 名 `tsuzuri_io.read_line`／`tsuzuri_io.write` を確認する。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/io.md`（主ページ）および旧名を含む他 33 ページ（`compiler/diagnostics.md`・`compiler/usage.md`・`compiler/kg.md`・`built-in-types-and-modules/{array,debug,dir,domain,env,file,list,net,os,owned,path,process,random,routine,time}.md`・`async-tasks-and-lazy/{lazy,tzroutine}.md`・`computation-expressions/computation-expressions.md`・`exception-handling/{assert,exception-handling,try-with-finally,use}.md`・`languages/how-about-tsuzuri.md`・`organizing-tsuzuri/{modules,packages}.md`・`ownership-and-memory/{drop,lifetimes}.md`・`types-and-type-inference/unit-type.md`・`values-and-functions/{attributes,lambda-expressions,tokens}.md`）。
- `docs/language.md`（「IO と標準入出力」節、`E1040` の行、各所のコード例）、`docs/architecture.md`（1019 行の `IO.write_line`）、`README.md`（クイックスタートと標準入出力の説明）。
- `guide/card.md`、`vsc/README.md`、`vsc/Development.md`、`_features/GUIDE.md`（D-55）。

## 受け入れ条件

- [ ] `std/IO.tc` の公開入出力関数が `try_readln`・`readln`・`try_write`・`try_writeln`・`try_write_err`・`try_write_errln`・`write`・`writeln`・`write_err`・`write_errln` の 10 個になり、`write_line` とその別名の重複が解消されている。
- [ ] 旧関数名 8 個が `E1040`（`renamed_io_function`）で報告され、`tsuzuri fix` で確実に新名へ書き換えられる。利用者の名前空間内の `IO` モジュールは影響を受けない。
- [ ] `IO.__read_line`・`IO.__write`、`tsuzuri_io_read_line`・`tsuzuri_io_write`、WebAssembly import `tsuzuri_io.read_line`・`tsuzuri_io.write` が変更されていない。
- [ ] `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo nextest run --locked` がすべて通る。
- [ ] `tsuzuri test --doc std`、`tests/io.mjs`・`tests/os.mjs`・`tests/repl.mjs`・`tests/script.mjs`・`tests/tzroutine.mjs`、`scripts/conformance.mjs`、`scripts/check-guide.mjs` が通る。
- [ ] `gen-docs.mjs --check`、`check-cli-docs.mjs`、`check-spec.mjs`、`check-docs.mjs`、`repo-kb.mjs verify`、`check-guide-lite.mjs` がすべて通る。
- [ ] VS Code 拡張のテスト対象（`vsc/src/test/extension.test.ts`）が新名 `'writeln'` に更新されている（PR に `ci:full` ラベルを付けるか `vscode` の検証を通す）。

## 落とし穴

1. **全文置換の禁止**: `read_line` や `write_line` を機械的に置換すると、以下を壊す。必ず `\bIO\.`（および `\bstd::IO\.`）を伴う Tsuzuri の識別子だけを対象にする。
   - WebAssembly import 名 `"tsuzuri_io"` / `"read_line"`（`src/llvm_io.rs`・`src/llvm.rs`・`tests/io.mjs` の `host` オブジェクト・`io.md` の WASM 節・`docs/language.md`・`docs/architecture.md`）。
   - std 内部組み込み `IO.__read_line` と C ランタイム関数 `tsuzuri_io_read_line`（`src/check.rs`・`src/capabilities.rs`・`src/runtime/io.c`・`src/runtime/os-wasi.c`）。
   - コンパイラ内部の Rust 関数 `mcp::rpc::{read_line, write_line}`、`server::read_line_limited`、`std::io::BufRead::read_line`、`kg::model::write_lines`。
   - `File`・`Net` などの他モジュールや利用者のテスト関数名（`def show_line` など）。
2. **`tests/std_examples.rs` の `FLOORS`**: `write_line` と `writeln` が統合されて公開小文字関数が 14 → 13 に減るため、`("IO", 14)` を `("IO", 13)` に下げないと `std_examples_meet_the_floors` が失敗する。
3. **KG golden の byte offset と `via`**: `tests/fixtures/kg_shop/Report.tz` の `IO.write_line`（13 文字）が `IO.writeln`（10 文字）になると、同じ行の後ろにある `(Cart.total lines)` の `start`・`end`・`column` が **3 小さく**なり、`facts/*.jsonl` の `via` も `["IO.writeln","IO.try_writeln","IO.try_output"]` に変わる。
4. **`std-table.txt` の辞書順**: `IO.try_write_err`・`IO.try_write_errln`・`IO.try_writeln` は旧名の並び（`try_write_error`・`try_write_error_line`・`try_write_line`）と同じく `BTreeMap` の辞書順で出力される。手で並べ間違えないよう `capabilities::analyze` の出力順と一致させる。
5. **`std::IO.write_line` の回復**: `src/check.rs` の `ExprKind::Field` の回復は現在 `ExprKind::Name(module)` を組み立てている（`Channel` → `DataChannel` のように裸の名前同士の置換だったため）。`std::IO.write_line` では左辺 `value` が `std::IO` のパス式なので、モジュールが変わらない改名では `(**value).clone()` をそのまま左辺に使わないと `std::IO` という単一識別子になって回復後の解決がずれる。

## 対象外

- `File`・`Net`・`Debug`・`Process` など、`IO` 以外の標準ライブラリモジュールの関数名の変更。
- `IO` への新しい関数の追加（`read_all`・書式付き出力など）やバッファリング方式の変更。
- 完了済みチケット（`_features/_completed/`）、アーカイブ済み仕様（`_specs/_archive/`）、過去のベンチマーク計測ログ（`docs/benchmarks-data/`）の書き換え。
- `vsc/images/completion.png`（静的スクリーンショット画像）の再撮影（拡張機能のリリース作業時に必要なら別途更新する）。

## 決定事項

| 番号 | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 依頼の表にある 8 関数に加え、`std/IO.tc` 末尾（201〜212 行、`writeln` の下）にある対のトラップ版 `write_error`・`write_error_line` も、同じ規則で `write_err`・`write_errln` に改名する | `IO` の入出力 API はすべての操作で `try_<name>`（`Result` 返却）と `<name>`（失敗時トラップ）が 1 対 1 に対になっており、`try_write_err`／`try_write_errln` に対してトラップ版だけ `write_error`／`write_error_line` のまま残すと命名規則が崩れ、`std/IO.tc` 内の呼び出し先（`try_write_error*`）とも食い違うため | 既定案 |
| D2 | 旧関数名 8 個は `@deprecated` による別名維持をせず `std/IO.tc` から削除し、`E1040`（kind `renamed_io_function`）と `Applicability::Certain` の修正案を出して `tsuzuri fix` で機械的に移行できるようにする | 今回の目的自体が `write_line`／`writeln` の二重定義の解消であり、0.x の改名方針（D-61・TZ03 の `DataChannel` 改名）と同じく、別名を残さず `E1040` + `tsuzuri fix` で即時・確実に移行するのが最も単純で誤用も防げるため | 既定案 |
| D3 | std 内部組み込み `IO.__read_line`・`IO.__write`、C ランタイム関数 `tsuzuri_io_read_line`・`tsuzuri_io_write`、WebAssembly import `tsuzuri_io.read_line`・`tsuzuri_io.write` は変更しない | 利用者から見える Tsuzuri の関数名ではなくホストとの公開 ABI（`docs/stability.md`）および内部プリミティブであり、変更しても利用者の利便性は変わらず既存の WASM ホスト実装だけを壊すため | 既定案 |
| D4 | `docs/benchmarks.md` 内のサンプルコード・表の説明に現れる `IO.write_line` は、再計測を伴わない範囲で新名（または計測時点の表記である旨が分かる形）に整え、`docs/benchmarks-data/*.json` の計測データは書き換えない | 関数名の変更は生成コードの性能を変えず、ベンチマーク数値の捏造や無意味な再計測を避けるため | 既定案 |

## 実装と検証（2026-11-21）

`main` `b6bbdcb5`（起票時の `295ffcb4` から 115 コミット後）で着手した。起票後の変化: G39・PB08 が `_completed/` へ移った（履歴なので触らない）、旧名を含むファイルに `benchmarks/lsp/workloads.mjs`・`vsc/scripts/test-extension.mjs`・`docs/benchmarks.md`（D4）が加わった。改名前の参照は履歴を除いて 162 ファイル・1,056 箇所だった。

### 仕様からの変更（実装で確定）

- 修正案の `Edit` は関数名の識別子だけを置き換える（仕様 2。型検査器はソースの文字列を持たないため。修飾・空白・間のコメントと改行を保つ）。
- 旧名の表は `removed_feature` ではなく `IO` 用の `renamed_io_function` にした（型の位置で `IO.write_line` を `E1040` にしないため）。
- `E1040.md` の `See also` を外した（`CODE_PAGE_LIMIT`）。`tests/capabilities.rs` の表の行数を 89 → 88 にした。
- 利用者向けの文書の「プロジェクトのモジュール `IO`」の例は `App::Tools` の `Tools/IO.tz`（仕様 2 の注記）。`io.md` の「以前の関数名」には、`tsuzuri fix` の 1 回の実行が最大 5 ラウンド（1 ラウンドはエラーの表示の上限 50 件まで）で、`///` の例の中は `tsuzuri test --doc` で見つけて手で直すことを書いた。

### 手順と結果

- 手順 1: `std/IO.tc` を改名し（公開の小文字関数 13 個、`FLOORS` の `("IO", 13)`）、`E1040`（`renamed_io_function`）・`hints.rs`・`overflow.rs`・`E1040.md` を更新した。`tests/removed_features.rs` に E17 のテスト 5 つ（8 つの旧名のメッセージ・help・確かな修正案と直した結果、`std::IO`・空白・括弧・パイプ・`std::IO . read_line`、1 回の解析で 9 か所を過不足なく報告、`tsuzuri fix --json` が 1 ラウンド・10 件・2 ファイルで直し `tsuzuri check` が通る、名前空間のモジュール `IO` が `E1002` のまま）を足した。
- 手順 2: `\bIO\.<旧名>\b` だけを対象にした置換で、履歴・本チケット・`src/check.rs` を除くリポジトリ全体を移行した。改名で文字位置が動いた期待値（`src/repl.rs` の `(22, 26)` → `(19, 23)`、KG の golden の `Report.tz:6:24` → `6:21` と `span`・`column` の -3、`manifest.json` のハッシュ（各ファイルの SHA-256 を `shasum` で照合）、`tests/kg_brief.rs` の source のハッシュ）と、`kg_query` の問い合わせ（`IO.write_lin` → `IO.writel`、`IO.writ_line` → `IO.writln`）を直した。`gen-docs` で `diagnostics.md` の生成ブロックを更新した。
- 手順 3: `tests/io.mjs` が 10 関数すべてを使うようにした（使われないアクションが動かないこと、`try_` 版の `InvalidEncoding`・`WriteFailed`・`Ok`、トラップ版の孤立サロゲートでのトラップ、閉じた標準出力での `write`・`writeln` のトラップ（`WriteFailed`）、native と WASM の `-O0`／`-O3`）。`tests/benchmark_report.mjs` の正規表現（`IO\.read_line`）、`vsc/src/test/extension.test.ts` の補完のラベル（`'writeln'`）、`benchmarks/lsp/workloads.mjs` のバイト数のコメントを直した。
- 手順 4: 言語リファレンス（`io.md` を書き直し、「以前の関数名」の節を足した。ほか 33 ページは置換と文の直し）、`diagnostics.md`・`docs/language.md`・`docs/architecture.md`・`README.md`・`_features/GUIDE.md`（D-55）を更新した。

検証（macOS arm64、dev ビルド）:

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（ホストと `x86_64-pc-windows-msvc`）が通った。
- `cargo nextest run --locked`: 3403 passed（改名前は 3398。E17 のテスト 5 つが増えた）。
- `tsuzuri test --doc std`: `-O0`・`-O3` とも 872 passed。
- Node: `io.mjs`・`os.mjs`・`repl.mjs`・`script.mjs`・`tzroutine.mjs`（163 runs）・`allocator.mjs`・`codegen_units.mjs`・`conformance_harness.mjs`・`integer_intrinsics.mjs`・`last_use_fuzz.mjs`・`local_recursion_fuzz.mjs`・`progfuzz.mjs`・`size.mjs`・`net.mjs`・`gpu_vulkan_language.mjs`（Vulkan の機器なしで 1 件 skip）・`benchmark_report.mjs`、`scripts/conformance.mjs`（241 ok）、`check-guide.mjs` が通った。`fast_backend.mjs` は `--features fast-backend` のビルドが要るので、その 2 つのプログラムを LLVM の経路で実行して期待値（`2` とトラップ位置、`4950`）を確かめた。
- 文書: `check-docs.mjs`（変えた 34 ページ、215 例・408 回のネイティブ実行）、`gen-docs.mjs --check`、`check-cli-docs.mjs`、`check-spec.mjs`、`check-guide-lite.mjs`、`repo-kb.mjs build`・`verify` が通った。
- 例と道具: `examples/{currying,functional,hello,io}` を `-O0`／`-O3` で実行し、WASM の import が `tsuzuri_io.read_line`・`tsuzuri_io.write` のままであることを確かめた。`vsc` は `npx tsc --noEmit` と `eslint` が通り、言語サーバーの補完が新名 10 個を出し旧名を出さないことを確かめた（拡張のホストのテストは `ci:full` の `vscode.yml`）。

### レビュー

独立のレビュー（コンパイラ・網羅と文書・テストの 3 つの観点と、指摘ごとの反証）で、この変更が入れた 6 件（`io.mjs` の閉じた標準出力の検査が必ず通っていたこと、`tsuzuri fix` のテストが 1 ラウンドを確かめていなかったこと、`io.md` の「1 回ですべて」の言い過ぎ、`docs/language.md` の「line 版」、`App/IO.tz` の例が `namespace = "App"` のプロジェクトで `E1011` になること、本チケットの設計の表の食い違い）を直した。E17 より前からある次の 2 つは、別のチケットにした。

- [E18](E18-removed-names-resolve-std-modules.md): `Channel`・`Net`・`Task`・`Async` の旧名の `E1040` が、書いたモジュール名の綴りだけで照合する（プロジェクトのモジュール `Channel` に無い `Channel.bounded` が `DataChannel.bounded` への確かな修正案を持つ `E1040` になり、`std::Channel.bounded` は `E1002` になる）。確かな改名の回復が関数を毒にして `finish` のエラーを隠し、`tsuzuri fix` が `E2008` で止まることがある点も、改善の候補として書いた。
- [G44](../G44-active-pattern-body-recovery.md): 複数ケースのアクティブパターンの本体が、最初のエラーしか報告しない（`infer_active_result` が回復しない）。

E17 より前からあり、この変更の範囲外のもの: `benchmarks/run-gpu-vulkan.mjs` の生成するプログラムと `tests/fixtures/mcp/tool-examples.json` の tz_guide の例が、edition 2027 の `E0004`（`&`・`1 .. n`）になる（どちらも名前の変更とは関係しない）。
