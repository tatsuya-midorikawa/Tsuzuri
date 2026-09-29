# E11: C ヘッダーからの extern 生成

| 項目 | 内容 |
|---|---|
| ID | E11 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E12 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: 既存の C/C++ コードをそのまま取り込めない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 主な影響ファイル | `src/main.rs`, `src/bindgen.rs`（新規）, `src/driver.rs`, `docs/language.md`, `tests/bindgen.rs`（新規）, `tests/bindgen.mjs`（新規） |

## 目的

既存の C ライブラリのヘッダーから `extern` 宣言・型別名・レコードを生成し、手書きの宣言と shim を減らす。
Rust の bindgen、C# の ClangSharp／P/Invoke 生成、Zig の `@cImport` に相当する。

## 現状

- `extern def` は手書き。native のシンボル名は `tsuzuri_host_<module>_<name>` に固定され、既存の C 関数を直接呼べない（E12 でリンク名の指定を計画）。
- 引数はスカラー・unit・共有 buffer/string/utf8string・スカラーレコード、結果はスカラー・unit・所有 buffer・スカラーレコードに限られる（docs/language.md ホスト関数のインポート）。
- コンパイラは Clang を外部 CLI として使い、`serde_json` をすでに依存に持つ（G07）。

## 仕様

- `tsuzuri bindgen <header.h> --module Name -o Name.tz [-- <clang 引数>]`。
- 外部 Clang の `-Xclang -ast-dump=json -fsyntax-only` の出力を `serde_json` で読み、指定ヘッダー（と明示した include ディレクトリ）に属する宣言だけを変換する。
- 変換対象:
  - 関数宣言 → E12 のリンク名付き `extern`（C の名前をそのまま使い、Tsuzuri 名は snake_case の衝突しない名前）。
  - 整数・浮動小数点の typedef → 型別名。enum → `i32` の `const` 群。
  - スカラーだけの struct → E05 のスカラーレコード（フィールド順と型が一致する場合だけ）。
- 変換できない宣言（可変長引数、関数ポインター、C の union、bitfield、意味の分からないポインター引数、flexible array）は生成物に理由をコメントで残し、
  警告 `W2002`（GUIDE D-30 の仮割り当て）で件数と宣言名を報告する。
- ポインターと長さの組を buffer に変換するのは、利用者の注釈ファイル（`--map name:ptr,len` など）で明示した場合だけ。推測しない。
- 出力は決定的（ヘッダー内の宣言順）。生成物は通常の Tsuzuri ソースとして検査・整形できる。

## 設計

- 新しい `src/bindgen.rs` に JSON AST の読み取りと型の対応表を置く。Clang の JSON は版により差があるため、使うフィールドを最小限にし、未知の形は変換不可として報告する。
- `#define` の数値定数は AST に現れないため Phase 2 で `-dM -E` の出力を解析する。
- 生成物の ABI は E05/E12 の規則をそのまま使い、新しい ABI を作らない。

## 実装手順

1. **CLI と Clang 実行**: 出力保護（生成先がソースを上書きしない、G11 と同じ規則）。確認: `tests/bindgen.rs` の CLI テスト。
2. **関数と typedef**: 確認: 小さなヘッダーの golden 出力。
3. **struct と enum**: 確認: C のテストライブラリをリンクして E2E で呼び出す。
4. **変換不可の報告**: `W2002` とコメント。確認: 各種の非対応宣言。
5. **実ライブラリでの確認**: libm・zlib などのヘッダーで生成物を検査する（テストには同梱しない）。

## テスト計画

- Rust: JSON AST の固定入力からの変換（Clang に依存しない単体テスト）。
- E2E: native × `-O0`/`-O3` で生成した extern から C 関数を呼ぶ。Clang の版差は CI の固定版で確認する。

## ドキュメント

- `_docs/guides/native-interop.md`、`_docs/tools/command-line.md`、`docs/language.md` のホスト関数のインポート。

## 受け入れ条件

- [ ] C ヘッダーから E12 のリンク名付き extern を生成し、shim なしで呼び出せる。
- [ ] 変換できない宣言を黙って捨てず、理由とともに報告する。

## 落とし穴

- ヘッダーが include するシステムヘッダーの宣言まで生成すると出力が膨大になる。対象ファイルで絞る。
- C の `long` はプラットフォームで幅が異なる。生成時の target を記録し、別 target での再利用を警告する。

## 対象外

- C++ ヘッダー（`extern "C"` 以外）、マクロ関数、インライン関数の本体。

## 未決事項

- **Windows**: 既定案は G10 の完了後に MSVC の呼び出し規約と型幅を検証する。
