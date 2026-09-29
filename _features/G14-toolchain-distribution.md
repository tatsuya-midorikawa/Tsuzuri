# G14: 自己完結ツールチェーンの配布

| 項目 | 内容 |
|---|---|
| ID | G14 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | (G10) |
| 後続 | G15, G16 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: VS Code 拡張以外では LLVM・Clang・LLD・Node.js を利用者が別途導入する必要がある（rustup や .NET SDK に相当する一括導入がない） |
| 主な影響ファイル | `vsc/scripts/toolchain.mjs`, `vsc/scripts/clang.rs`, `scripts/`（新規 toolchain スクリプト）, `src/driver.rs`, `src/main.rs`, `.github/workflows/`, `README.md`, `_docs/get-started.md` |

## 目的

コンパイラ・Clang・LLD・リンク用の SDK/libc を一つの配布物にまとめ、VS Code を使わない CI・サーバー・他のエディターでも同じツールチェーンを使えるようにする。

## 現状

- README の手順では、利用者が LLVM/Clang 17 以降・`wasm-ld`・（WASM の実行とテストに）Node.js を導入し、`TSUZURI_CLANG`／`TSUZURI_WASM_LD` で指定する。
- VS Code 拡張（`vsc/`）は `npm run toolchain`（`vsc/scripts/toolchain.mjs`）で LLVM 21・Zig 0.16 の SDK/libc・Node 24・コンパイラを OS/CPU 別に同梱し、再配置・署名・SHA-256 検証を行う。
  対象は darwin-arm64/x64、linux-x64/arm64、win32-x64/arm64。Linux の生成物は同梱 musl を使う。この配布は VSIX 専用。
- `vsc/scripts/clang.rs` は Zig の SDK を使う Clang の shim。
- G11 の cache key はツールの実行ファイルの digest と版を含む。

## 仕様

- ホストごとの配布物 `tsuzuri-<version>-<host>.tar.gz`（Windows は `.zip`）: `bin/tsuzuri`、`lib/tsuzuri/`（Clang・wasm-ld・shim・SDK/libc）、任意の Node.js コンポーネント、全ファイルの SHA-256 を記した manifest、ライセンス。
- ツールの探索順: 環境変数（`TSUZURI_CLANG` など）→ 配布物の同梱ツール（実行ファイルからの相対パス）→ `PATH`。選ばれたツールと版を `tsuzuri toolchain info` で表示する。
- インストール手順は、配布物のダウンロード → チェックサムの検証 → 展開 → `PATH` への追加。パイプで直接実行するスクリプトは推奨しない。
- VS Code 拡張は同じ配布物を取り込む形に移し、同梱処理を二重に保守しない。
- 生成物は配布物の版と無関係に決定的（同じ入力と同じツールで同じ IR・成果物）。

## 設計

- `vsc/scripts/toolchain.mjs` の取得・再配置・検証の処理を `scripts/toolchain/` に取り出して共有する。VSIX 固有の処理（拡張機能の manifest）だけを `vsc/` に残す。
- driver のツール探索（`tool("TSUZURI_CLANG", "clang")` など）に同梱ツールの段を追加する。探索結果は G11 の cache key に反映される。
- CI（`.github/workflows/`）でホストごとの配布物を作り、配布物だけを使うスモークテスト（native/WASM の build・run・test）を実行する。

## 実装手順

1. **処理の共通化**: 確認: 既存の VSIX 生成物が変わらない（manifest の比較）。
2. **配布物の生成**: 確認: 展開した配布物だけで、システムの LLVM がない環境（コンテナ）で build・run・test が通る。
3. **ツールの探索**: 確認: 環境変数・同梱・PATH の優先順位のテスト。
4. **CI とリリース手順**: 確認: ホストごとのスモークテスト。

## テスト計画

- 配布物の manifest の全ファイルのハッシュ検証、再配置後の実行（macOS の codesign、Linux の RPATH）。
- 既存の `npm run test:toolchain` と同等の検査を CLI 配布物に適用する。

## ドキュメント

- `README.md`、`_docs/get-started.md`、`_docs/tools/build-and-cache.md`、`vsc/README.md`、`vsc/Publishing.md`。

## 受け入れ条件

- [ ] システムに LLVM がない環境で、配布物だけを使って native/WASM のビルド・実行・テストができる。
- [ ] VSIX と CLI 配布物が同じ同梱処理を共有する。

## 落とし穴

- Windows（G10 が blocked）の配布物は実行検証ができるまで「未検証」と明記する。
- 同梱した上流ツールのライセンス表示を配布物に含める。

## 対象外

- 自動更新（`tsuzuri self update`）、OS のパッケージマネージャーへの登録。

## 未決事項

- **署名**: 既定案はチェックサムを必須とし、署名方式（minisign／sigstore など）は人間が判断する。
- **Node.js の同梱**: 既定案は任意コンポーネントとして別配布にする。
