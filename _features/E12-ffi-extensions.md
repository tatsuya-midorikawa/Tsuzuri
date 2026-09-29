# E12: FFI の拡張（リンク名・ホストのリンク指定・不透明ハンドル・コールバック）

| 項目 | 内容 |
|---|---|
| ID | E12 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | E05, E06, (B07) |
| 後続 | E11, E13, E08, F13 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: 任意の外部 ABI・既存ライブラリとの接続の自由度が小さい（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 主な影響ファイル | `src/parser.rs`, `src/syntax.rs`, `src/check.rs`, `src/abi.rs`, `src/llvm_abi.rs`, `src/llvm_imports.rs`, `src/driver.rs`, `src/main.rs`, `src/package.rs`, `docs/language.md`, `tests/host_imports.rs`, `tests/host_imports.mjs` |

## 目的

既存の C ABI 関数を shim なしで呼び、ホストの資源を型安全なハンドルとして所有し、実行ファイルへホストのオブジェクトやライブラリを直接リンクできるようにする。
Phase 2 では Tsuzuri の関数をコールバックとして C へ渡せるようにする。

## 現状

- `extern def now :: unit -> i64` の native 名は `tsuzuri_host_<module>_<name>`、WASM は module `tsuzuri`・name `<Module>.<name>` に固定（`src/check.rs`、`src/llvm_imports.rs`）。
- 「未解決hostを含むexe/runはlink error E2002で、host object指定CLIはありません」（docs/language.md ホスト関数のインポート）。
- 「制約・named region・可変参照・callback/Task/list等は受け取りません」。公開 ABI も union・タプル・リスト・関数値・Task・i128・f16 などを拒否（`Type::exportable`、`src/abi.rs`）。
- 不透明ハンドル・ポインター型・コールバックはない。所有 buffer は `tsuzuri_alloc`／`tsuzuri_free` の規約で受け渡す（E05）。

## 仕様

### Phase 1（実装対象）

1. **リンク名**: `extern "sqrt" def c_sqrt :: f64 -> f64` のように C のシンボル名を指定する（native）。WASM は `extern "env" "now" def now :: unit -> i64` のように module と name を指定する。
   名前は C の識別子・WASM の名前の規則で検査し、同じシンボルへの異なる型の宣言は `E1008`。指定がない場合は従来の名前。
2. **ホストのリンク指定**: `build`／`run`／`test` に `--link <path>`（object・static library）、`-l <name>`、`-L <dir>` を追加し、manifest の `[native] link = [...]` でも指定できる。
   native exe でだけ有効で、WASM・object・header との併用は `E2000`。未解決シンボルは従来どおり `E2002`。
3. **不透明ハンドル**: `extern type FileHandle` で、中身を持たない非 Copy の所有型を宣言する。表現はポインター幅の値で、C header では `typedef struct tz_handle_FileHandle *` として現れる。
   解放は B07 の `Drop` instance で extern の close 関数を呼んで行う。ハンドルは extern の引数（値・共有参照）と結果にだけ使え、公開 ABI（`export def`）の引数・結果にも使える。
   WASM では `i32`（wasm64 では `i64`）の整数として渡し、externref は使わない。

### Phase 2（設計方針）

- コールバック: 関数値を C の `{ 関数ポインター, void *context }` の組として、呼び出し中だけ貸す（非 escaping）。ホストが保存しないことは信頼境界の契約とする。
- ABI 型の追加（固定長配列 A16 のフィールド、スカラー以外のフィールド、タプル）、共有ライブラリ出力（E13 と共同）。

## 設計

- `Program.externs` にリンク名と WASM の module/name を保持し、`llvm_imports.rs` の宣言生成で使う。宣言は既存どおり BTreeSet で重複なく決定的に出力する。
- `extern type` は `Type::Handle(id)` として GUIDE §6.3 を更新する。`is_copy` は false、`needs_drop` は Drop instance の有無に従う、`exportable` は true。
- driver のリンク指定は既存の clang 起動に引数を追加するだけにし、入力パスは出力保護の検査対象に含める。

## 実装手順

1. **リンク名**: 構文・検査・IR。確認: `tests/host_imports.rs`、`libm` の `sqrt` を直接呼ぶ E2E。
2. **リンク指定**: CLI と manifest。確認: C の static library をリンクした exe の実行、WASM との併用拒否。
3. **extern type**: 型・所有権・header。確認: ハンドルの作成・利用・Drop による close の回数。
4. **Phase 2 の設計レビュー**: コールバックの寿命契約とトラップ境界（E14）との関係。

## テスト計画

- Rust: 構文・`E1008`・`E2000`・所有権（ハンドルの move 後使用 `E1012`）。
- E2E: native/WASM × `-O0`/`-O3`、ASan、確保追跡、IR の決定性、WASM import の module/name の一致。

## ドキュメント

- `docs/language.md` のホスト関数のインポート・公開 ABI、`_docs/guides/native-interop.md`、`_docs/guides/webassembly.md`、`_docs/tools/command-line.md`。

## 受け入れ条件

- [ ] 既存の C 関数をリンク名で直接呼べ、`tsuzuri run` でホストのライブラリをリンクできる。
- [ ] 不透明ハンドルを所有値として扱い、Drop で確実に解放できる。
- [ ] リンク名を使わないプログラムの IR・import が変わらない。

## 落とし穴

- リンク名が `tz_`／`tsuzuri_` の予約名前空間や C ランタイムの名前と衝突しないか検査する。
- macOS の Mach-O は C シンボルに `_` を付ける。LLVM IR の名前は付けずに出力する（リンカーが処理する）。

## 対象外

- `unsafe` ブロック・生ポインター型、C++ ABI、可変長引数関数の呼び出し。

## 未決事項

- **構文**: 既定案は `extern "symbol" def` と `extern "module" "name" def`。manifest での一括指定も候補。
- **生ポインター**: 既定案は導入しない。低水準の操作はホスト側に置き、ハンドルで受け渡す。
