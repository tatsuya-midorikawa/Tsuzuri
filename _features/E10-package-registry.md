# E10: git／registry 依存・lockfile・版解決

| 項目 | 内容 |
|---|---|
| ID | E10 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | E04, G11 |
| 後続 | G19, E09 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | Rust 比: Cargo／crates.io に相当する依存管理がない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）、C#/F# 比: NuGet |
| 主な影響ファイル | `src/package.rs`, `src/driver.rs`, `src/main.rs`, `src/cache.rs`, `docs/language.md`, `tests/modules.rs`, `tests/packages.mjs`（新規） |

## 目的

別リポジトリのライブラリを再現可能に取得・固定し、共有できるようにする。
ビルド中にネットワークへ触れず、取得物の同一性を検証できる安全な依存管理を段階的に導入する。

## 現状

- `Tsuzuri.toml` の `[package]`（`name`、`version` は非空文字列で版解決に使わない）と `[dependencies] name = { path = "..." }` の限定 TOML（`src/package.rs`）。
- 「ネットワーク、git、lockfile、版解決、build script、言語内import宣言は導入しません」（docs/language.md ローカルパッケージ）。それ以外のキーは `E0002`。
- グラフの上限は package 1024・深さ 128・source 4096（`E1017`）。循環・名前空間衝突・symlink は `E1011`。
- `src/cache.rs` に SHA-256 の実装と NIST ベクトルの検証がある（G11）。

## 仕様

### Phase 1: git 依存と lockfile（実装対象）

- `name = { git = "https://example.org/repo.git", rev = "<40 桁の commit>" }`。`rev` は完全な commit ID だけを許す（branch・tag は Phase 2）。
- `tsuzuri fetch` が外部の `git` CLI で取得し、利用者のキャッシュ（`TSUZURI_CACHE_DIR` と同じ方針の場所）に commit ごとに展開する。
- `build`／`check`／`run`／`test` はネットワークに触れない。キャッシュにない依存は `E2007`（GUIDE D-30 の仮割り当て）で `tsuzuri fetch` を案内する。
- `Tsuzuri.lock`（決定的な書式）: 依存ごとに source、commit、正規化した内容の SHA-256（`.tz`／`.tt`／`.tc` と manifest を相対パス順に長さ付きで連結）。
  不一致は `E2007`。lockfile は `fetch` だけが書き、`build` は書き換えない。
- 安全性: `https://` 以外の URL を拒否、submodule・LFS・hooks を無効化、symlink と作業木外へのパスを拒否（E04 と同じ）、取得サイズの上限、build script なし。
- 同じ名前の異なる source・commit がグラフに現れた場合は `E1011`（一つの名前に一つの版）。

### Phase 2: 版と registry（設計方針）

- `version = "^1.2"` の semver 範囲と、lockfile による固定。
- 静的な HTTPS index とアーカイブのチェックサムによる registry、`tsuzuri publish`（G19 の API 差分検査を前提にする）。

## 設計

- `package.rs` の限定 TOML に `git`／`rev` を追加し、E04 の厳密な構文検査を維持する。
- 取得は `fetch` サブコマンドに限定し、driver の通常経路は既存の path 依存と同じ `Project` 読み込みに、キャッシュ内の root を渡すだけにする。
- 内容ハッシュは G11 の SHA-256 を再利用する。

## 実装手順

1. **manifest と lockfile の書式**: 解析・出力・決定性。確認: `tests/modules.rs` の受理・拒否（`E0002`）。
2. **fetch**: ローカルの bare repository を使うテスト（ネットワークなし）。確認: 取得・展開・lockfile。
3. **offline build**: キャッシュからの読み込み、欠落・改ざんの `E2007`。確認: 改ざんしたファイルでの拒否。
4. **安全性の検査**: `http://`・`file://`・symlink・submodule の拒否。
5. **Phase 2 の設計レビュー**: 版解決方式と registry の運用主体。

## テスト計画

- Rust: manifest・lockfile の解析、グラフ規則。
- E2E: 一時ディレクトリの bare repository を使う取得とビルド（native/WASM）、出力保護（依存ソースを出力で上書きしない）。

## ドキュメント

- `docs/language.md` のローカルパッケージ、`_docs/language-reference/modules-and-packages.md`、`_docs/tools/command-line.md`、README の未実装一覧。

## 受け入れ条件

- [ ] commit 固定の git 依存を取得し、lockfile で検証してオフラインでビルドできる。
- [ ] 改ざん・欠落・安全でない URL を明示的に拒否する。

## 落とし穴

- `git` の設定（利用者の global config の hooks・credential helper）が取得に影響しないよう、実行時の設定を固定する。
- lockfile をビルドのたびに書き換えると再現性と出力保護を壊す。

## 対象外

- build script、ネイティブライブラリの自動取得、プライベート registry の認証。

## 未決事項

- **HTTP の実装**: Phase 2 の registry 取得に外部 CLI を使うか Rust の依存 crate を追加するか（既定案: 人間が判断するまで Phase 1 は git CLI のみ）。
- **版解決方式**: 既定案は Cargo 互換（互換範囲の最大版 + lockfile 固定）。最小版選択（MVS）も候補。
