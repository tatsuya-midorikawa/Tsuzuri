# ビルド環境、成果物キャッシュ、プラットフォーム

[ドキュメントのトップ](../README.md)

コンパイラは Rust でビルドし、コード生成はテキスト LLVM IR と Clang / LLD のコマンド境界を使います。LLVM の C API、開発ヘッダー、CMake を Cargo ビルドへ要求する構成ではありません。

## コンパイラをビルドする

```sh
cargo build --release --locked
sh scripts/check-runtime-includes.sh
```

Rust 1.85 以降が必要です。ランタイム IR はリポジトリに同梱しており、新しい clone での通常ビルド時に生成し直す必要はありません。

check-runtime-includes は埋め込む runtime IR が存在し、Git に追跡され、ignore されていないことを確認します。ランタイムを変更する開発作業では、生成された IR も配布できる状態に保ちます。

native と WASM の成果物には LLVM / Clang 17 以降、WASM には wasm-ld が必要です。数学 runtime の再生成は別の開発工程で、対応する Clang / llvm-link の版をそろえます。現在の検証済み再生成環境と総合検証は[README](../../README.md#検証と性能測定)を参照してください。

## プラットフォーム

| 環境 | 条件・現在の状態 |
| --- | --- |
| macOS / Linux native | 通常の LLVM とシステム開発環境。POSIX Task は pthread |
| 配布物（darwin / linux の arm64・x64） | コンパイラ・Clang・LLD・Zig の SDK/libc を同梱し、LLVM の別途導入が不要。darwin-arm64 は実機で検証。ほかは CI の同じ検査で確認する。導入手順は[はじめに](../get-started.md#配布物を使う) |
| wasm32 | Clang / LLD と対応 WASM engine。既定は SIMD / threads 不要 |
| x86_64 Windows MSVC | 実装・専用 CI・実 SDK による cross-link は存在。Windows 上の実行ゲートは未確認 |

Windows は LLVM clang、Windows SDK、Visual Studio Build Tools の MSVC / CRT を Developer PowerShell から使う構成です。clang-cl 専用引数は対象外です。コンパイラは MSVC target を選び、POSIX の fPIC / pthread / libm フラグを渡しません。

Windows の Task は Win32 の常駐プールです。runtime を同梱する COFF object の結合は E2002 で、exe へ直接生成するか LLVM と runtime を明示的に一度だけリンクします。

PDB、ARM64 Windows、MinGW、DLL import library の自動生成は未対応です。出力は UTF-8 bytes でコードページを変更しないため、対応するコンソールを使います。実装があることを、Windows での全実行検証が完了した保証とはしません。

## キャッシュの対象

build / run の成果物 cache は既定で有効です。現在は parse / check / IR 生成の後に外部ツールの仕事を省く **whole-build cache** です。ソースファイルごとの増分型検査や IR cache ではありません。

```sh
./target/release/tsuzuri build examples/hello --no-cache -o target/hello-without-cache
```

--no-cache は読み取りと書き込みの両方を無効化します。check / header は対象外です。専用ディレクトリを指定する例:

```sh
TSUZURI_CACHE_DIR="$PWD/target/tsuzuri-build-cache" ./target/release/tsuzuri build examples/hello -o target/hello
```

## 保存先

| OS | 既定 |
| --- | --- |
| macOS | `$HOME/Library/Caches/tsuzuri/build-cache` |
| Linux | `$XDG_CACHE_HOME/tsuzuri/build-cache`。未設定なら `$HOME/.cache/tsuzuri/build-cache` |
| Windows | `%LOCALAPPDATA%\Tsuzuri\Cache\build-cache` |

既存の任意のディレクトリを cache として転用しません。専用 marker を要求し、symlink を拒否します。手動削除する場合もこの専用 cache だけを対象にし、プロジェクトや出力先と混同しないでください。

## キーと検証

コンパイラ実行ファイル、外部ツール本体と版、ソースと manifest、設定、生成 IR、関連環境を SHA-256 で識別します。同じ Git commit でも異なる開発版 compiler は区別します。native CPU 設定や、macOS debug executable の出力先依存も考慮します。

外部ツールは環境変数 → 配布物の `bin/` → `PATH` の順に決め（[CLI](command-line.md#ツールと環境変数)）、key には解決後のパス・digest・`--version` を含めます。同梱のツールと `PATH` のツールでは key が変わり、成果物を取り違えません。

hit でも各ファイルのサイズ・digest・権限を検査し、通常の出力保護を通して公開します。trap table や DWARF などの sidecar も対象です。破損、欠落、未知形式は miss として再生成します。

## 障害と容量管理

cache の I/O 失敗は警告として扱い、元のビルドを維持します。ツールの版を確認できなければ cache を無効化する場合があります。cache hit ではないことと、ビルド失敗は別です。

同時保存には非待機の lock を使い、競合時は保存を省いて他のビルドを待たせません。全ファイル完成後にディレクトリを公開します。

2 GiB / 30 日を目安とし、一回最大 128 件を回収します。走査にも上限があるため容量は soft limit です。共有 CI の性能閾値や分散 cache の整合性を提供するものではありません。

cache の信頼境界は同じ OS ユーザーの private な保存先です。他者が自由に書き込めるディレクトリを使用することは想定していません。

## 関連項目

- [入門の環境構築](../get-started.md)
- [デバッグ成果物](debugging.md)
- [性能と CPU 選択](../guides/performance.md)
