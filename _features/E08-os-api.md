# E08: 標準 OS API（ファイル・環境・時刻・乱数・プロセス）

| 項目 | 内容 |
|---|---|
| ID | E08 |
| 優先度 | P1 |
| 規模 | XL |
| 依存 | B07, E06 |
| 後続 | E09, B08, G18 Phase 3 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C/C++ 比: OS との接続にホスト実装が必要（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）、C#/F# 比: ファイル API がない（[同](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 主な影響ファイル | `std/IO.tc`, `std/File.tz`・`std/Env.tz`・`std/Time.tz`・`std/Random.tz`（新規）, `src/llvm_io.rs`, `src/runtime/io.c`, `src/driver.rs`, `src/main.rs`, `docs/language.md`, `tests/io.mjs`, `tests/os.mjs`（新規） |

## 目的

ファイル・ディレクトリ・環境変数・コマンドライン引数・時刻・乱数・終了コードを標準 API で扱えるようにする。
C/C++ の標準ライブラリ、.NET の BCL、Rust の `std::fs`／`env`／`time` に相当し、小さなツールやバッチ処理をホスト実装なしで書けるようにする。

## 現状

- `std/IO.tc` の `IO<'a>` は不透明な遅延アクション。標準入力の `read_line`、標準出力・標準エラーの `write`／`write_line`／`write_error`／`write_error_line` と `try_` 版だけ。
  `IO.Error` は `ReadFailed | WriteFailed | InvalidEncoding`。
- native は `src/runtime/io.c`（C stdio）、WASM は到達する操作だけ `tsuzuri_io.read_line`／`write` を import する（`src/llvm_io.rs`）。「WASI や非同期イベントループは追加しません」（docs/language.md IO と標準入出力）。
- README: 「git・registry・版解決とGUI/OSの標準ライブラリは未実装です」。ファイル・環境・時刻・乱数・プロセスの API はない。
- 資源の解放はコンパイラ生成の drop だけ（B07 で `Drop` を導入予定）。

## 仕様

### Phase 1（実装対象）

- 全操作は `IO<Result<T, Os.Error>>` を返す遅延アクションで、既存の IO と同じく entry の IO 文脈でだけ実行される。
- `Os.Error` は `{ kind: Os.ErrorKind, code: i32 }`。`ErrorKind` は `NotFound | PermissionDenied | AlreadyExists | InvalidInput | InvalidEncoding | Interrupted | Other`。
- `File`: `read_bytes`／`read_text`（UTF-8 厳密）／`write_bytes`／`write_text`／`append_text`、`File.open` による `File.Handle`（B07 の Drop で close）と `read`／`write`／`flush`。
- `Dir`: `list`（名前順に整列して決定的に返す）、`create`、`remove`（空ディレクトリのみ）。`Path` は IO を伴わない文字列操作。
- `Env`: `args`、`var`、`current_dir`。`Time`: 単調時計 `monotonic_ns`、壁時計 `unix_ns`、`sleep_ms`。
- `Random`: OS の乱数 `Random.bytes`／`next_u64`（IO）と、IO を伴わない seed 付き決定的 PRNG `Random.Pcg`。
- 終了コード: `Main` の入口として `IO<i32>` も受け付け、値をプロセスの終了コードにする。
- WASM: 既定は import なしを維持する。`--wasm-host wasi` を指定した場合だけ WASI preview1 の import（`fd_read`／`fd_write`／`path_open`／`clock_time_get`／`random_get`／`args_get`／`environ_get`／`proc_exit` など）へ下げ、
  既存の標準入出力も同じ WASI 経路にする。指定なしで OS API に到達した WASM ビルドは `E2000` で理由を示して拒否する。
- 非 UTF-8 のパス・環境変数は `InvalidEncoding` を返す（黙って置換しない）。

### Phase 2（設計方針）

- プロセスの起動（引数配列で渡し shell を介さない）、ファイルのメタデータ、ディレクトリの再帰走査、Windows（G10）の実行検証。

## 設計

- native の実装は `io.c` と同じ C ランタイムに追加し、POSIX と Win32 を `#if` で分ける。driver の既存の runtime 連結条件（到達するシンボル）を使う。
- 各 builtin は std の `File` 等からだけ呼べる内部名にする（`IO.__read_line` と同じ方式、利用者の直接呼び出しは `E1022`）。
- 所有バッファは既存の `tsuzuri_alloc` 規約で受け取り、長さ・UTF-8 を受領時に検査する。
- `Os.ErrorKind` への case 追加は網羅的な `match` を壊すため、追加は edition（G19）でだけ行う。

## 実装手順

1. **エラー型と File の読み書き**: native。確認: `tests/os.mjs` で一時ディレクトリ内の読み書き・エラー種別。
2. **Handle と Drop**: open/read/write/close。確認: close 回数と fd の漏れがない（`lsof` 相当の検査はホスト側で）。
3. **Dir・Env・Time・Random**: native。確認: 決定的な整列、単調時計の単調性、乱数の長さ。
4. **終了コード**: `IO<i32>` の入口。確認: `tsuzuri run` の終了コードと `E2005` の区別。
5. **WASI**: `--wasm-host wasi` と Node の `node:wasi` で同じテストを実行。確認: 既定 WASM の import が空のまま。

## テスト計画

- E2E: native/WASM(WASI) × `-O0`/`-O3`、ASan/UBSan、エラー経路（存在しないファイル、権限なし）、確保追跡。
- テストはリポジトリ外の一時ディレクトリだけを使い、既存ファイルを変更しない。

## ドキュメント

- `docs/language.md` の IO、`_docs/library-reference/io.md`、`_docs/guides/webassembly.md`、`README.md` の未実装一覧、GUIDE D-07・D-18。

## 受け入れ条件

- [ ] native でファイル・環境・時刻・乱数・終了コードを扱える。
- [ ] WASM は明示した WASI 指定でだけ import を追加し、既定の出力が変わらない。
- [ ] ハンドルがすべての経路で一度だけ close される。

## 落とし穴

- 並列 Task から IO は実行できない（IO は entry 文脈だけ）。この制約を崩さない。
- パスの正規化や `..` の扱いはホスト OS に任せ、アクセス制御を実装したと誤解させない（パストラバーサル対策は利用者・ホストの責任として文書化する）。
- `Dir.list` の順序を OS の返す順にすると非決定的になる。

## 対象外

- ネットワーク（E09）、非同期 IO（B08）、GUI、ファイル監視。

## 未決事項

- **エラー型**: 既定案は `Os.Error` の新設。`IO.Error` への case 追加は既存の網羅的な match を壊すため採用しない。
- **WASI の版**: 既定案は preview1。preview2／component model は E13 と合わせて検討する。
