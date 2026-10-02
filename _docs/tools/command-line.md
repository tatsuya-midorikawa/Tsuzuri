# コマンドラインリファレンス

[ドキュメントのトップ](../README.md)

以下の tsuzuri はビルド済み実行ファイルです。リポジトリでは `./target/release/tsuzuri` として起動できます。入力は原則として一つのファイルまたはディレクトリです。

## コマンド

| コマンド | 役割・入力 |
| --- | --- |
| `check INPUT` | 構文、型、所有権、公開 ABI などを検査。実行しない |
| `build INPUT` | 成果物を生成。build は省略可能 |
| `run INPUT` | Main を native へコンパイルして実行 |
| `test INPUT` | 言語内の test 宣言を分離実行 |
| `fmt INPUT` | 空白・インデントを保守的に整形 |
| `doc INPUT -o DIRECTORY` | 公開 API の Markdown を生成 |
| `lsp` | stdin / stdout の言語サーバー。パスやビルドオプションは付けない |
| `toolchain info` | 選ばれた外部ツール（環境変数・配布物・`PATH` のどれか）とその版、配布物の識別子を表示。ほかの引数は付けない |

```sh
./target/release/tsuzuri check examples/hello/Main.tz --json
./target/release/tsuzuri run examples/hello
./target/release/tsuzuri build examples/hello -o target/hello
```

check は LLVM を起動しません。ライブラリのファイルには main が不要です。ただし通常のディレクトリ入力は Main.tz を選ぶため、Main のないライブラリを check する場合は実際のソースファイルを指定します。doc / test は Main のないディレクトリも受理します。

## 入口

run と exe 出力には root 直下の Main.tz が必要です。宣言後のトップレベル実行コード、または引数なしの main を使い、両方は併用しません。

トップレベルの束縛は入口ローカルで、宣言済みの関数や別モジュールへ公開するグローバル値ではありません。最終結果は数値、bool、unit、両文字列型、両文字型です。ホスト wrapper が結果を表示し、unit は無出力です。

library / WASM 出力はトップレベルコードを自動実行しません。export された関数をホストから呼びます。WASM 出力には少なくとも一つの export def が必要です。

## 出力形式

| emit | 内容 |
| --- | --- |
| `exe` | native 実行ファイル |
| `object` | ホストでリンクする object。wasm32・wasm64 の未リンク object も可能 |
| `llvm` | ライブラリ用テキスト LLVM IR。コンソール wrapper は付けない |
| `header` | C / C++ 用ヘッダー |
| `wasm` | リンク済み WASM |
| `wgsl` | 実験的な単一 scalar export の GPU カーネル |

既定の target は native、既定 emit は native で exe、wasm32・wasm64 で wasm です。LLVM IR と header の生成に LLVM の実行環境は不要です。生の native タスク IR を直接リンクする場合はランタイム C も必要ですが、通常の object 出力には同梱します。

## build と run の主なオプション

| オプション | 契約 |
| --- | --- |
| `-o PATH`, `--output PATH` | build の出力先。親を作成し、成功した成果物を公開 |
| `--target native`, `--target wasm32`, `--target wasm64` | 生成ターゲット。run は native のみ。wasm64 は memory64 |
| `--emit KIND` | 出力形式 |
| `-O0` から `-O3` | build / run の既定は O3。test は O0 |
| `--cpu generic`, `--cpu native` | native exe / object / run の CPU 選択 |
| `--wasm-feature simd128` | WASM SIMD を明示要求 |
| `--wasm-feature threads` | WASM / object の Worker 対応を明示要求（wasm32 のみ） |
| `--wasm-max-memory SIZE` | WASM の線形メモリ上限。既定 16MiB、64 KiB の倍数で wasm32 は最大 4GiB-64KiB、wasm64 は最大 16GiB |
| `--wasm-stack-size SIZE` | WASM の main stack。既定 1MiB、16 の倍数で 64 KiB 以上 |
| `--no-cache` | build / run の成果物 cache の読み書きを無効化 |
| `--link PATH` | native の実行ファイルへホストの object / static library をリンク。繰り返し可 |
| `-l NAME`, `-L DIR` | system library を名前（`-l sqlite3`）でリンクし、`-L` で探索先を追加。繰り返し可 |
| `-g`, `--debug-info` | DWARF 情報を追加 |
| `--trap-info` | トラップ理由・位置と side table を追加。run は既定で有効 |
| `--trap-mode return` | native の `--emit object`・`llvm`・`header` で、各 export に `tsuzuri_try_<name>`（トラップを戻り値で返す）を追加。`--trap-info` を含む（build のみ） |
| `--debug-output` | WASM の Debug 出力をホスト import へ接続 |
| `--deny-warnings` | check / build / run を警告だけでも失敗させる |

native CPU 指定を WASM / LLVM IR / header に使うことはできません。WASM feature は check / run / native / header には指定できず、threads は LLVM テキスト出力にも指定できません。未知・重複 feature はエラーです。

`--link`・`-l`・`-L` は値を次の引数で渡し（`=` 形式は受けません）、合計 256 個までです。`-l` は `lib` 接頭辞と拡張子を付けない名前です。root package の `Tsuzuri.toml` の `[native]`（`link`・`libraries`・`search`）も同じ入力を指定し、コマンドラインより前に連結します。入力は build / run / test が native の実行ファイルを作るときだけ有効で、WASM、exe 以外の `--emit`、check / fmt への指定、同じ入力の重複、不正な `-l` 名、root package が `native = true` で許可していない依存 package の `[native]` は `E2000` です（manifest の `[native]` は前者では無視します）。root の `[dependencies]` に `lib1 = { path = "lib1", native = true }` と書いた直接の依存だけが `[native]` を持て、その入力は依存の root からの相対 path で、root の入力の後に連結します。入力がある間は成果物 cache を使いません。詳しくは [C ホスト連携](../guides/native-interop.md#ホストの-object-とライブラリをリンクする)を参照してください。

SIZE はバイト数か、`KiB`・`MiB`・`GiB` を付けた整数です（例 `67108864`、`64MiB`）。接尾辞は大文字小文字を区別し、値は次の引数で渡します（`=` 形式は受けません）。`--wasm-max-memory` は WASM の build（wasm・object・llvm 出力）と `test --target wasm32`／`wasm64`、`--wasm-stack-size` は WASM の wasm 出力と test だけで使えます。上限は stack + 64 KiB 以上が必要です。それ以外の action・出力、範囲外の値、重複は `E2000` で、静的データが上限に収まらないリンクは `E2002` です。object / llvm を自分でリンクするときは wasm-ld へ同じ `--max-memory` を渡します（wasm64 は `-mwasm64` も）。
root package の `Tsuzuri.toml` の `[wasm]` の `max-memory`・`stack-size` は、その指定が効く出力での既定値で、コマンドラインが優先します。詳しくは[WebAssembly ガイド](../guides/webassembly.md#ブラウザーとメモリ)を参照してください。

WGSL 出力は専用の制約を持ち、target / optimization / cpu / debug オプションを付けません。詳細は[GPU ガイド](../guides/gpu.md)を参照してください。

## その他のオプション

`--json` は診断を stderr の JSON lines にします。test の結果データは stdout であり、診断と分離します。`fmt --check` は変更なしの整形確認、`test --filter TEXT` はテスト名の部分一致です。

`--` の後はパスとして扱います。実行プログラムへ引数を転送する一般的な run 引数機能ではありません。ヘルプは `-h` / `--help`、バージョンは `--version` です。

## ツールと環境変数

| 変数 | 用途 |
| --- | --- |
| `TSUZURI_CLANG` | Clang の実行ファイル。配布物では `bin/tsuzuri-clang`、それ以外は clang |
| `TSUZURI_WASM_LD` | WASM linker。既定 wasm-ld |
| `TSUZURI_LLVM_LINK` | macOS の debug runtime object 等で使う対応版 llvm-link。既定 llvm-link |
| `TSUZURI_DSYMUTIL` | macOS の debug executable の DWARF 抽出。既定 dsymutil |
| `TSUZURI_CACHE_DIR` | 専用成果物 cache の保存先 |
| `TSUZURI_CPU_FORCE` | テスト用 baseline / sse4.2 / avx2 強制。初回呼び出し前だけ設定 |

4 つのツールは、環境変数（空文字列も設定済みとみなす）→ 配布物の `bin/` → `PATH` の順に決めます。配布物は、実行ファイルの実体（symlink を解決したパス）の 2 段上に `manifest.json` があるディレクトリです。開発用ビルド（`target/release/tsuzuri`）は配布物とみなしません。

```sh
./target/release/tsuzuri toolchain info
```

各ツールの行は `<変数>: <env|bundled|path> <解決したパス> (<--version の版の行>)` の形で、見つからないツールは `(not found)` です。WASM の言語内テストが起動する `node` は常に `PATH` から探します。

外部ツールはシェルを介さず起動します。ツールのパスと引数を一つの環境変数へ連結しないでください。CPU 強制は未対応・未知なら停止します。

## 出力保護と終了コード

出力省略時は選択した入力ファイルの拡張子を変更します。失敗時は既存成果物を維持し、ソースや manifest、その別名、symlink への出力を拒否します。リンク入力を上書きする出力も拒否し（`E2003`）、読めないリンク入力は `E2001` です。文書生成ディレクトリには別の marker 規則があります。

終了コードは成功 0、ソース・I/O・ツール・実行エラー 1、CLI 引数構成の誤り 2 です。拡張子や入力ファイル種別の拒否はソースエラーとして 1 になる場合があります。

## 関連項目

- [診断と警告](diagnostics.md)
- [テスト](testing.md)
- [整形と LSP](editor-tools.md)
- [開発環境](../get-started.md)
