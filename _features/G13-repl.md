# G13: REPL とスクリプト実行

| 項目 | 内容 |
| --- | --- |
| ID | G13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | G11, (G20), (G17) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D12（Phase 2 の `script` コマンドと shebang 行）, D13（Phase 3 の JIT）。Phase 1 は承認不要 |
| 改善する劣位 | C#/F# 比: 対話環境がない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | 位置引数を取らないサブコマンド: `src/main.rs` の `parse_arguments` の `lsp` 分岐。ディスクにないソースの解析: `src/lsp.rs` が呼ぶ `Project::load_with_overlays`。ビルドと実行・stderr の中継・`E2005`: `src/driver.rs` の `run_with_diagnostics` と `TemporaryDirectory`。診断の表示: `src/main.rs` の `print_with_severity`（`Diagnostic::render_with_severity`）。実プロセスの E2E: `tests/lsp_sessions.mjs`、`tests/io.mjs` の `execute` |
| 主な影響ファイル | `src/main.rs`, `src/repl.rs`（新規）, `src/lib.rs`, `src/driver.rs`, `tests/repl.mjs`（新規）, `README.md`, `docs/architecture.md`, `_docs/tools/command-line.md`, `_docs/get-started.md`, `_docs/guides/from-fsharp.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

F# Interactive（`dotnet fsi`）や C# の対話環境のように、宣言と式を一つずつ入力して、結果の値と型をすぐ確かめられる環境を用意する。
Phase 1 は JIT を使わず、入力ごとに既存のパイプライン（解析 → LLVM IR → Clang → native 実行）を通す再コンパイル型の `tsuzuri repl` を作る。
単一ファイルのスクリプト実行（Phase 2）と JIT（Phase 3）は方針だけを書く。

実装者は Phase 1 だけを実装する。Phase 2・Phase 3 は人間が承認して求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G11（whole-build cache）が `_features/README.md` の状態欄で done であること（HEAD で done）。G20・G17 は開始条件にしない。
  G20 は一度に出る診断の数を、G17 は解析の時間を改善するだけで、REPL の仕様を変えない。
  確認: `grep -nE "^\| G(11|17|20) " _features/README.md`。
- PB01（ランタイムの事前ビルド）と PB05（高速デバッグバックエンド）も開始条件にしない。REPL の応答時間はこの 2 件で短くなるが、
  REPL は `BuildOptions` を渡して既存のビルドを呼ぶだけなので、両チケットの完了後に REPL 側の変更は要らない（D1）。
- Phase 1 に承認は要らない。Phase 2 は D12、Phase 3 は D13 の承認前に着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 言語の規則を変えないと実現できない。例: 実行コードの後に宣言を書けるようにする、トップレベルの束縛を関数から参照できるようにする、`it` を予約語にする。
- 新しい crate（`rustyline`、`ctrlc`、`tempfile` など）、`unsafe`、シグナル処理やプロセスグループの操作が必要になった。
- `run_with_diagnostics` の共通化（手順 2）で `tsuzuri run` の挙動が変わる。stdin・stdout の継承、stderr の中継、`E2005` のメッセージと位置、
  `--json` のどれかが変わったら止める。確認は `tests/io.mjs`（対話 CLI の case を含む）と `tests/e2e.mjs`。
- semantic index からトップレベルの `let it` の型を得られない（D3 の前提が崩れる）。
- 通常の対話の規模で、生成する `Main.tz` が `MAX_SOURCE_BYTES`（1 MiB）以外の上限（特殊化 1,024 など）に達する。
- 既存テストの期待値を変える必要がある。`src/main.rs` の `HELP` への `repl` の行の追加と、それを照合するテストだけは除く。
- stack-depth の 3 テスト（GUIDE §11）が失敗する。REPL は parser と checker を変えないので、失敗したら設計からの逸脱を疑う。

## 現状（HEAD `f8dc655` で確認）

- CLI: `src/main.rs` の `parse_arguments` は先頭の `lsp` を特別扱いし、`check`／`doc`／`build`／`run`／`fmt`／`test` を `Action` の variant に対応させる。
  それ以外の先頭引数は入力パスとみなして `Action::Build` になるため、`tsuzuri repl` は `repl` という名前のファイルのビルドとして失敗する（下の再現）。
- 実行: `src/driver.rs` の `run(module, project, options)` と `run_with_diagnostics(module, project, options, json)` は、`Target::Native` かつ
  `Emit::Executable` 以外を `E2000`（`run requires native executable output`）で拒否する。`TemporaryDirectory::new(&env::temp_dir())` の中へ
  `build_complete(..., BuildOptions { trap_info: true, ..options }, "run")` で実行ファイルを作り、stdin と stdout を継承し、stderr を pipe で受けて中継する。
  異常終了は `E2005` で、`Project::with_trap_sources` と trap site からトラップ位置の span を探す。
- 読み込み: `Project { sources, manifests, root }`。`Project::load_with_overlays(input, overlays: &BTreeMap<PathBuf, String>)` は LSP の経路で、
  ディスクにない overlay のファイルも project に含める。`Project::analyze()` は最初の診断を、`Project::analyze_all()` は `DiagnosticSet` を返す。
  診断の表示は `src/main.rs` の `print_with_severity` が `Diagnostic::render_with_severity(severity, path, source)` を呼ぶ。
- 解析: `src/parser.rs` の `parse(source) -> Result<Program, Diagnostic>`。`Program` は宣言を種類別の `Vec`（`functions`、`records`、`unions`、
  `type_aliases`、`constants`、`externs`、`classes`、`instances`、`active_patterns`、`tests`）に、トップレベルの実行コードを `entry: Option<Expr>` に持つ。
  `src/lib.rs` の `analyze_modules_with_semantics(sources: &[(&str, &str)])` は単相化前の `SemanticIndex` を返し、`src/semantic.rs` は
  利用者の局所変数ごとに `detail: format!("{}: {}", local.name, local.ty.display(&types))` の `SemanticEntry` を作る。
- 入口の規則（docs/language.md「アプリケーションのエントリーポイント」）: `Main.tz` は宣言の後にトップレベルの `let` と最後の結果式を書く。
  宣言を実行コードの後に書くと `E0002`。結果式の自動表示は数値型・`bool`・`unit`・`string`・`utf8string`・`char`・`utf8char` だけで、
  `IO<T>` の結果式はアクションを一度実行して値を表示しない。トップレベルの `let` は IO を実行しない（`let!`／`do!` が実行する）。
- 出力先: 結果式の表示と `IO.write_line` は stdout、`Debug.print`／`Debug.trace` は native では stderr。
- WASM: 実行コードだけの `Main.tz` は wasm32 へビルドできない（`E2004`）。`IO<T>` の main か `export def` が要る。
- JIT はない。docs/architecture.md は LLVM の C API・Rust バインディングに結合しない方針を書いている。
- ビルド時間（PB01 の計測、M1 Max、`--no-cache`）: `examples/hello` の実行ファイルは `-O0` 0.33 s、`-O3` 0.53 s。このうち、埋め込みの
  数値ランタイム（`numeric.ll`）を含む IR の Clang `-O3` のコンパイルが 0.43 s（`-O0` では 0.06 s）。PB01 の完了まで、`-O3` では入力ごとにこの時間がかかる。
- 同じ種類の機械での目安（2026-09-30、各 1 回。ベンチマークではない）: 下の `p1` の `tsuzuri run --no-cache` は実行を含めて `-O0` 0.46 s、`-O3` 0.79 s。

### 再現（検証済み）

各ケースを `/tmp/tz-work-G13/<case>/Main.tz` に置き、`target/release/tsuzuri run <case> -O0 --no-cache` で確かめた。

`p1`: 式を `let it` に束縛し、`Display.display` で文字列にして結果式にする（D3 の包み方）。stdout は `36\n`。

```tsuzuri
def square :: i64 -> i64 = \x -> x * x
let base = 6
let it = square base
Display.display (ref it)
```

`p2`: `let it = "hi"` と結果式 `Display.display (ref it)` は `hi\n`（引用符なし）。

`p3`: `def square ...`、`let it = square`（関数値）、`Display.display (ref it)` の 3 行は、wrapper の行で
`p3/Main.tz:3:1: error[E1005]: no instance for Display<i64 -> i64>; define an instance or use a supported type`。

`p4`（拒否）: `let base = 6` の次の行に `def square ...` を書くと、2 行目で `E0002`（`expected the end of the file; declarations must precede the entry-point code`）。

`p5`: `do! IO.write_line "hello"` だけの `Main.tz` は stdout に `hello\n`。wasm32 へのビルドも成功する（IO の入口）。
`p6`: `let it = Debug.trace 5` と結果式 `it + 1` は stdout に `6\n`、stderr に `5\n`。
`p7`: `let action = IO.write_line "once"` と結果式 `action` は `once\n` を一度だけ出す。
`p8`: `let it = (`、`match 3 with`、`| 3 -> "three"`、`| _ -> "other"`、`)`、`Display.display (ref it)` の 6 行は `three\n`（括弧の中の複数行の式）。
`p10`: `let x = 10 / (5 - 5)` は stderr に `trap: integer division by zero at p10/Main.tz:1:9` と `p10/Main.tz:1:9: error[E2005]: ...` を出す。
`p12`: `let it = (IO.write_line "x")` に wrapper を付けると `E1005`（`no instance for Display<IO<unit>>; ...`）。IO の型の表示は `IO<unit>`。

```sh
cd /tmp/tz-work-G13
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri repl
# repl:1:1: error[E2001]: cannot inspect source 'repl': No such file or directory (os error 2)
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri build p1 --target wasm32 -o /tmp/tz-work-G13/p1.wasm
# p1/Main.tz:1:1: error[E2004]: a WebAssembly module needs an IO<T> main or at least one 'export def' entry point
```

## 仕様

### 前提とする他チケットのインターフェース

なし。Phase 1 は HEAD の API（`Project::load_with_overlays`、`analyze_inputs_indexed_all`、`build_complete`、`BuildOptions`）だけを使う。
PB01・PB05・G17 が入ると、同じ `BuildOptions` のまま応答が速くなる。REPL 側で経路を選び分けない（D1）。

### コマンドライン

新 API（実装後に有効。未検証）。

```text
tsuzuri repl [-O0|-O1|-O2|-O3] [--cpu generic|native] [--no-cache] [--timeout SECONDS]
```

- 既定は `-O0`、`--cpu generic`、cache 有効、`--timeout 10`。`--timeout 0` は無制限。値は 0 から 3600 の整数秒。
- パス、`--target`、`-o`、`--emit`、`--json`、`-g`、`--deny-warnings`、`--debug-output`、`--trap-info`、`--wasm-feature` は `E2000`（診断の表を参照）。
- 終了コード: `:quit` か stdin の EOF で 0。入力のエラーでは終了せず、終了コードも変えない。起動時の失敗（オプション、一時ディレクトリの作成）だけが 1。
- stdin が端末（`std::io::IsTerminal`）のときだけ、起動時に `Tsuzuri <version> REPL; enter :quit to exit` を、入力の 1 行目の前に `> ` を、
  継続行の前に `. ` を stdout へ出す。端末でなければ prompt も見出しも出さない（D9）。

### 入力の読み取りと複数行入力

stdin を 1 行ずつ UTF-8 で読み、行末の `\n` と直前の `\r` を除く。読んだ行を `\n` でつないだ文字列を「入力」と呼ぶ（D7）。

1. 新しい入力の 1 行目が空白だけなら無視する。`:` で始まる（先頭の空白は除く）ならコマンド（D6）。
2. それ以外は `parser::parse(text)` を試す。成功なら入力は完結している。
3. 失敗し、診断の `span.start >= text.trim_end().len()`（入力の末尾での失敗）なら継続する。空行（長さ 0 の行）か EOF まで行を足し、その全体を一つの入力にする。
4. 末尾以外での失敗は、その場で診断を出して入力を捨てる。
5. 継続中の `:` で始まる行はコマンドではなく入力の一部。継続中に入力が `MAX_SOURCE_BYTES` を超えたら、残りの行を空行まで読み捨てて `E0003` を出す。

したがって、複数行にしたい入力は 1 行目を未完の形（`(` や `{` を閉じない、`=` や `->` で終わる）にし、空行で終える。
1 行目だけで完結する入力（`let x = 40`）の次の行（`(x + 2)`）は、`Main.tz` のトップレベルと同じく別の入力になる。

### 入力の分類

入力の `Program`（`src/syntax.rs`）を次のように分ける（D2）。`Ident::provenance` が `Provenance::User` でない名前（parser が作る補助の宣言）は数えない。

| 部分 | 取り出し方 | key |
| --- | --- | --- |
| 宣言 | `functions`、`constants`、`active_patterns` | 値の名前 `Key::Value`（新規） |
| 宣言 | `records`、`unions`、`type_aliases`、`classes` | 型の名前 `Key::Type`（新規） |
| 宣言 | `instances` | `Key::Instance`（新規）: `class.text` と、`ty.span` の範囲の文字列から空白を除いたもの |
| 束縛 | `entry` が `ExprKind::Block { bindings, result }` のときの `bindings` | `Key::Binding`（新規）: `name.text` |
| 式 | `entry` が `Block` なら最後の束縛の後の残り、そうでなければ `entry` 全体 | なし |
| アクション | `entry` が `ExprKind::Computation` か `ExprKind::ComputationBoundary`（トップレベルの `let!`／`do!`／`match!` など） | なし |

- 宣言の文字列: 名前の span を含む行の行頭から、次の宣言の行頭（最後の宣言は `entry` の行頭か入力の末尾）の直前まで。同じ行に宣言が二つあれば `E2000`。
- 束縛の文字列: 前の境界（最初は `entry` の行頭、以後は直前の束縛の `value.span.end`）から `value.span.end` まで。先頭の空白と `;` を除く。
  式の文字列: 最後の束縛の `value.span.end` から末尾まで、両端の空白と `;` を除く。空なら式はない。
- 受け付けない宣言（D10）: `externs`、`tests`、名前が `main` の `functions`。どれかがあれば入力全体を `E2000` で拒否する。

### 評価と結果の表示

各入力で、セッション（受理済みの宣言と束縛）に入力の宣言と束縛を合わせた候補を作り、次の `Main.tz` を生成する（D2・D3）。

```text
<候補の宣言（セッションの順。置き換えは元の位置、新規は末尾）>
<候補の束縛（同じ規則）>
<入力の種類ごとの末尾>
```

| 入力 | 末尾 | 解析と実行 | stdout への表示 |
| --- | --- | --- | --- |
| 宣言だけ | なし | 解析だけ | なし |
| 束縛を含み式なし | なし | 解析し、実行する（束縛のトラップをその場で検出） | 入力の束縛ごとに `name: T` |
| 式 E | 検査用 `let it = (` 改行 E 改行 `)` | 解析して `it` の型 T を得る | 下の 3 行 |
| アクション | `entry` の文字列そのまま | 解析し、実行する | プログラムの stdout をそのまま |

式の 3 通り:

- T が `unit`: 検査用の `Main.tz` を実行する。表示なし。
- 表示用 `let it = (` 改行 E 改行 `)` 改行 `Display.display (ref it)` の解析が成功: 実行し、`it: T = <stdout から末尾の改行を 1 個除いた文字列>` を出す。
- 表示用の解析の診断がすべて wrapper の行の `E1005` のとき: 検査用を実行し、`it: T` だけを出す。`IO<unit>` などの IO の値もここに入り、実行されない。

T は `analyze_inputs_indexed_all` に渡した `SemanticIndex` の `entries` のうち、`span.start` が生成した `it` の位置で、`detail` が `it: ` で始まるものから取る。
束縛の `name: T` も同じく束縛名の位置の `detail` をそのまま出す。成功したら候補をセッションにする。失敗したらセッションを変えない（入力は原子的）。

### 再定義

同じ key の項目は置き換える（D4）。入力の項目を順に処理し、この入力より前からセッションにある同じ key の項目がそれを置き換える
（最初の位置へ置き、ほかの同じ key の項目は除く）。同じ入力の中の同じ key は後ろへ足す（入力内の shadowing を保つ）。
置き換えの後は生成した `Main.tz` 全体を解析するので、依存する宣言と束縛は必ず再検査される。どこかでエラーになれば入力全体を拒否し、
診断は `session:<行>:<列>`（`:list` の行）で示す。依存する側も直したい場合は、同じ入力にまとめるか `:reset` する。

### 入力間の状態と副作用

Phase 1 は実行中の状態を持ち越さない（D5）。評価のたびに新しいプロセスで、受理済みの束縛をソース順にすべて再実行してから入力を評価する。

- 束縛の値は毎回計算し直す。重い束縛は以後のすべての入力を遅くする。`it` はセッションに残さない（値を残すなら `let` にする）。
- トップレベルの `let` は IO を実行しないので、束縛の再実行で stdout への出力は重複しない（再現 `p7`）。IO はアクションの入力でだけ実行し、アクションはセッションに残さない。
- `Debug.print`／`Debug.trace` を含む束縛は、再実行のたびに stderr へ同じ出力を出す（表示だけで、結果は変わらない）。
- 評価するプログラムの stdin は `Stdio::null()`。`IO.read_line ()` は常に `None`（EOF）を返す。REPL 自身の入力を子プロセスが読むことはない。
- 束縛のトラップは、束縛を入力した時点の実行で検出して拒否する。以後の再実行で同じ束縛がトラップすることはない（決定的な言語なので同じ結果になる）。

### コマンド

| コマンド | 動作 | 出力 |
| --- | --- | --- |
| `:quit` | 終了する（終了コード 0） | なし |
| `:type E` | 式 E を検査用の `Main.tz` で解析だけする。実行しない。E は同じ行に書く | stdout に T |
| `:load PATH` | `.tz` ファイルを `driver::read_source` で読み、その全体を一つの入力として扱う。PATH は作業ディレクトリ基準。診断の path は PATH | 入力と同じ |
| `:list` | セッションの宣言と束縛を生成順に出す（`session:` の行番号の基準） | stdout にソース。空なら何も出さない |
| `:reset` | セッションを空にする | なし |

引数の過不足、未知のコマンド、`.tz` 以外の `:load` は `E2000`。

### 数値・トラップ・native と WASM の差

- 評価は `tsuzuri run` と同じ経路で、数値の意味（オーバーフロー、丸め、NaN、トラップ）は同じ。`-O0` と `-O3` で結果は同じで、時間だけが違う。
- トラップや異常終了は既存の `E2005` で、位置を入力の行と列へ写す。その入力は拒否し、セッションは変えない。
- Phase 1 は native だけ（D1）。WASM は対象外で、`--target` を受け付けない。

### 診断

REPL 独自のエラーは既存のコードを使う。メッセージは英語で、表の文字列と完全に一致させる。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E2000` | パスか未対応のオプション | `repl takes no paths; supported options are -O0 to -O3, --cpu, --no-cache, and --timeout` | なし（CLI のエラー） |
| `E2000` | `--target` | `repl supports only the native target; build WebAssembly with 'tsuzuri build --target wasm32'` | なし |
| `E2000` | `--timeout` の値 | `--timeout requires whole seconds from 0 to 3600` | なし |
| `E2000` | 未知のコマンド | `unknown command ':NAME'; commands are :type, :load, :list, :reset, and :quit` | `input:1:1` |
| `E2000` | 引数の過不足 | `:type requires an expression` ／ `:load requires one .tz file path` ／ `:list, :reset, and :quit take no arguments` | `input:1:1` |
| `E2000` | `extern` 宣言 | `extern declarations are not supported in the REPL; use a project with 'tsuzuri run'` | 名前 |
| `E2000` | `test` 宣言 | `test declarations are not supported in the REPL; use 'tsuzuri test'` | 名前 |
| `E2000` | `main` の定義 | `'main' cannot be defined in the REPL; enter its body as an input instead` | 名前 |
| `E2000` | 同じ行に二つの宣言 | `put each declaration on its own line in the REPL` | 二つ目の名前 |
| `E0003` | 入力か生成した `Main.tz` が 1 MiB を超える | 既存の lexer・loader のメッセージ | 既存 |
| `E2001` | `:load` の読み込みの失敗 | 既存の `read_source` のメッセージ | `input:1:1` |
| `E2005` | 評価の時間切れ | `evaluation exceeded the N-second limit and was stopped; use --timeout to change it` | 式の先頭 |
| `E2005` | stdout が 16 MiB を超えた | `program output exceeded 16 MiB and the program was stopped` | 式の先頭 |
| そのほか | 構文・型・所有権・トラップ | `tsuzuri check`／`run` と同じコードとメッセージ | 写した位置（D9） |

警告は表示しない（D11）。

### 資源上限

| 対象 | 上限 | 超えたとき |
| --- | --- | --- |
| 一つの入力 | `MAX_SOURCE_BYTES`（1 MiB） | `E0003`。入力を捨てる |
| 生成した `Main.tz` | 1 MiB（`Project::load_with_overlays` の既存の検査） | `E0003`。入力を拒否する |
| 評価の実行時間（Clang を除く子プロセスだけ） | `--timeout`、既定 10 s | 子プロセスを kill して `E2005` |
| 捕捉する stdout | 16 MiB（`MAX_CAPTURED_OUTPUT`（新規）） | 子プロセスを kill して `E2005` |

### 決定性

同じ stdin からは、時間・一時ディレクトリ名・cache の状態に関係なく、同じ stdout を byte 単位で出す（D9）。stdout には一時パスを出さない。
診断の path は `input`（`:load` ではそのパス）、`session`、std のモジュールのパスのどれか。例外は、実行時のトラップで子プロセスが
stderr に出す `trap: ... at <一時パス>/Main.tz:L:C` の 1 行で、続く `E2005` の診断が写した位置を示す。

### 例

新 API（実装後に有効。未検証）。各入力の Tsuzuri のコードは「再現」の `p1`・`p3`・`p8`（括弧の中の複数行の `match`）で確かめた形だけを使う。
stdin:

```text
def square :: i64 -> i64 = \x -> x * x
let base = 6
square base
:type square
def square :: i64 -> i64 = \x -> x + x
square base
"hi"
square
let word = (
    match base with
    | 6 -> "six"
    | _ -> "other"
)

word
let broken = 10 / (base - 6)
do! IO.write_line "hello"
:list
:quit
```

stdout（期待値。exit 0）:

```text
base: i64
it: i64 = 36
i64 -> i64
it: i64 = 12
it: string = hi
it: i64 -> i64
word: string
it: string = six
hello
def square :: i64 -> i64 = \x -> x + x
let base = 6
let word = (
    match base with
    | 6 -> "six"
    | _ -> "other"
)
```

stderr には `broken` の行の `input:1:14: error[E2005]: ...` が出て、`broken` はセッションに入らない。

### Phase 2: スクリプト（設計方針）

- `tsuzuri script FILE.tz [args...]`: 任意の名前の単一ファイルを `Main.tz` として overlay し、`run` と同じ経路で実行する。先頭の shebang 行
  （`#!/usr/bin/env tsuzuri script`）を無視する。shebang は lexer の規則の変更なので、承認（D12）の前に着手しない。

### Phase 3: JIT（研究）

- LLVM の JIT による再コンパイルなしの評価と、プロセスを跨がない状態の保持。LLVM の C API に結合しない方針（docs/architecture.md）と衝突するため、人間が判断する（D13）。

## 設計

### データ構造

```rust
// src/repl.rs（新規）
pub struct ReplOptions { pub build: BuildOptions, pub timeout: Option<Duration> }
#[derive(Clone, PartialEq, Eq)]
enum Key { Value(String), Type(String), Instance(String, String), Binding(String) }
#[derive(Clone)]
struct Item { key: Key, text: String, offset: usize } // offset: 入力の中の開始位置
#[derive(Clone, Default)]
struct Session { declarations: Vec<Item>, bindings: Vec<Item> }
enum Body { None, Expression(String, usize), Action(String, usize) }
struct Input { path: String, text: String, declarations: Vec<Item>, bindings: Vec<Item>, body: Body }
enum Origin { Session(usize), Input(usize), Wrapper } // listing の位置、入力の位置、生成部分
struct Generated { text: String, segments: Vec<(Range<usize>, Origin)>, it: Option<usize> }

// src/driver.rs
enum RunStdio { Inherit { json: bool }, Capture { timeout: Option<Duration>, limit: usize } } // （新規）
const MAX_CAPTURED_OUTPUT: usize = 16 * 1024 * 1024; // （新規）
pub fn run_captured(module: &CheckedModule, project: &Project, options: BuildOptions,
    timeout: Option<Duration>) -> Result<Vec<u8>, Diagnostic>; // （新規）stdout を返す
```

`Main.tz` はディスクへ書かない。空の一時ディレクトリ（`TemporaryDirectory::new(&env::temp_dir())`、REPL の終了時に `close`）を root にして、
`BTreeMap` に `<root>/Main.tz` → 生成した文字列を入れ、`Project::load_with_overlays(&root, &overlays)` で読む。作業ディレクトリの `.tz` は読まない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `HELP` | 「コマンドライン」の書式の `tsuzuri repl` の行を足す |
| CLI | `src/main.rs` | `Action`, `Arguments`, `parse_arguments` | `Action::Repl` と `Arguments::timeout`（新規）。先頭の `repl` を `lsp` と同じく早い分岐で読み、専用の小さなオプション解析をする。既定の `optimization` は 0 |
| CLI | `src/main.rs` | `main`、`run_action` | `Action::Repl` は project を読まずに `tsuzuri::repl::run`（新規）へ。`run_action` の match は `unreachable!` |
| lib | `src/lib.rs` | module 一覧 | `pub mod repl;` |
| 実行 | `src/driver.rs` | `run_with_diagnostics`, `run_process`（新規）, `run_captured`（新規） | 本体を `run_process(module, project, options, RunStdio)` へ移す。`Inherit` は HEAD と同じ動作 |
| 実行 | `src/driver.rs` | `read_source` | 変更なし（既に `pub`）。`:load` が使う |
| REPL | `src/repl.rs`（新規） | `run`, `read_input`, `classify`, `Session::merge`, `generate`, `analyze`, `evaluate`, `command`, `render` | 本チケットの本体 |
| 解析・検査・LLVM・cache | `src/parser.rs`, `src/check.rs`, `src/llvm.rs`, `src/cache.rs` | – | 変更なし |

### 生成 IR とランタイム

変更なし。生成した `Main.tz` は `tsuzuri run` と同じ経路を通るので、同じ文字列をディスクに置いた project の IR と一致する。
`run_captured` は `run_process` の `Capture` で、`build_complete` までは `Inherit` と共通。違いは子プロセスの stdio だけ:
stdin は `Stdio::null()`、stdout は pipe で読む thread が `MAX_CAPTURED_OUTPUT` まで集める、stderr は既存の中継の loop を別 thread で回す。
親 thread は `child.try_wait()` を 10 ms ごとに見て、期限か出力の超過で `child.kill()` と `wait()` をしてから `E2005` を返す。

### アルゴリズム

```text
run(options):
  root = TemporaryDirectory::new(temp_dir); session = Session::default()
  loop:
    input = read_input(stdin)            // EOF → root.close(); exit 0
    if command → command(...); continue
    program = parse(input.text)          // 失敗 → render; continue
    input = classify(program, input)     // E2000 → render; continue
    candidate = session.merge(&input)
    match input.body:
      None  → generated = generate(candidate, None)
              (module, index) = analyze(generated)?          // 失敗 → 写して render; continue
              if input.bindings が空でない: evaluate(module)?; 各束縛の detail を出す
      Expression(e) → check = generate(candidate, CheckIt(e)); (module, index) = analyze(check)?
              t = index の it の detail
              if t == "unit": evaluate(module)?
              elif show = generate(candidate, ShowIt(e)) の analyze が成功: out = evaluate(show)?; "it: t = out" を出す
              elif 表示用の診断がすべて Wrapper の E1005: evaluate(module)?; "it: t" を出す
              else: 表示用の診断を render; continue
      Action(a) → generated = generate(candidate, Action(a)); evaluate(analyze(generated)?)?; stdout をそのまま出す
    session = candidate
```

`render(diagnostic, generated)`: `project.source_for` が overlay の `Main.tz` なら、`span.start` を含む segment を探す。`Input(o)` は入力の文字列と
`o` からの相対位置で path `input`（または `:load` のパス）、`Session(o)` は `:list` の文字列で path `session`、`Wrapper` は式全体の span へ置き換える。
それ以外（std）はその source で render する。どれも `Diagnostic::render_with_severity("error", path, source)` を stderr へ出す。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の `p1`〜`p8`・`p10`・`p12` を `/tmp/tz-g13/<case>/Main.tz` に作り直し、
  `tsuzuri run <case> -O0` の出力を保存する。E2E の期待値の独立な根拠はこの出力と手計算で、REPL の出力ではない。
- 確認: 次がすべて成功し、`p1` の stdout が `36`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
target/release/tsuzuri run /tmp/tz-g13/p1 -O0
cargo test --locked --bin tsuzuri
node tests/io.mjs target/release/tsuzuri
node tests/e2e.mjs target/release/tsuzuri
node tests/cache.mjs target/release/tsuzuri
```

### 手順 2: `run_process` への共通化（挙動は不変）

- 変更: `src/driver.rs` の `run_with_diagnostics`、`RunStdio`（新規）、`run_process`（新規）。
- 内容: 本体を `run_process(module, project, options, RunStdio::Inherit { json })` へ移し、`run_with_diagnostics` はそれを呼ぶだけにする。
  戻り値は `(Vec<String>, Vec<u8>)`（`Inherit` の stdout は常に空）。`temporary.close()` と `E2005` の組み立ての順序を変えない。
- 確認: `cargo test --locked --lib driver::` が成功し、手順 1 の `tests/io.mjs`・`tests/e2e.mjs`・`tests/cache.mjs` が同じ結果で成功する。

### 手順 3: `run_captured`

- 変更: `src/driver.rs` の `RunStdio::Capture`、`run_captured`（新規）、`MAX_CAPTURED_OUTPUT`（新規）。
- 内容: 「生成 IR とランタイム」のとおり。stdout と stderr は別々の thread で読む。期限と超過の `E2005` は `Span::default()` で返し、REPL が式の先頭へ写す。
  polling の箇所に `// ponytail: 10 ms try_wait polling; use a blocking wait with timeout if std gains one` を付ける。
- 確認: `cargo build --locked` と `cargo clippy --all-targets -- -D warnings` が成功する。挙動は手順 9 の E2E（R10・R11）で確かめる。

### 手順 4: CLI

- 変更: `src/main.rs` の `HELP`、`Action::Repl`、`Arguments::timeout`、`parse_arguments`、`main`、`run_action`。`src/lib.rs` の `pub mod repl;`。
  `src/repl.rs`（新規）の `ReplOptions` と、`:quit` と EOF で 0 を返すだけの `run`。
- 内容: 先頭の `repl` を `lsp` と同じ位置の分岐で読み、専用のオプション解析をする。`BuildOptions::default()` は変えず、REPL の既定だけ `optimization: 0`。
- 確認: `cargo test --locked --bin tsuzuri repl` が `running 2 tests` で成功する。
  `cargo build --locked && printf ':quit\n' | target/debug/tsuzuri repl; echo $?` が `0` だけを出す。

### 手順 5: 入力の読み取りと分類

- 変更: `src/repl.rs` の `read_input`、`classify`、`Key`、`Item`、`Input`、`Body`。
- 内容: 「入力の読み取りと複数行入力」「入力の分類」のとおり。`read_input` は `impl BufRead` を受け、テストでは `std::io::Cursor` を渡す。
  アクションの判定と、式のない束縛列の `entry` の形は、先にテスト 4・5 で parser の実際の形を確かめてから書く。
- 確認: `cargo test --locked --lib repl::` が `running 7 tests`（Rust テストの 1〜7）で成功する。

### 手順 6: セッションの合成と生成

- 変更: `Session::merge`、`generate`、`Origin`、`Generated`。
- 内容: 「再定義」の規則と、宣言 → 束縛 → 末尾の順の生成。segment は生成と同時に記録する（後から文字列を探さない）。
- 確認: `cargo test --locked --lib repl::` が `running 10 tests` で成功する。

### 手順 7: overlay の解析と型の取り出し

- 変更: `src/repl.rs` の `analyze`。
- 内容: `Project::load_with_overlays` の後、`src/lsp.rs` の `refresh` と同じ形で `SourceInput` を作り、`crate::analyze_inputs_indexed_all(&inputs, Some(&mut index))` を呼ぶ。
  `project.input()` が overlay の `<root>/Main.tz` でなければ、生成した文字列を一時 root の `Main.tz` に書いて `Project::load` で読む（D2 の代替。挙動は同じ）。
- 確認: `cargo test --locked --lib repl::` が `running 12 tests` で成功する。

### 手順 8: 評価と表示

- 変更: `src/repl.rs` の `evaluate`（`driver::run_captured`）、`render`、`run` の本体。`tests/repl.mjs`（新規）の R1・R2。
- 確認: `cargo build --release --locked && node tests/repl.mjs target/release/tsuzuri` が 2 case で成功する。

### 手順 9: 再定義・状態・コマンド・上限

- 変更: `Session::merge` の置き換え、アクションの経路、`command`、`classify` の拒否。`tests/repl.mjs` の R3〜R12・R14。
- 確認: 同じコマンドが 13 case で成功する。R10 と R11 は経過時間を assert しない。

### 手順 10: 最適化レベルと全体の確認

- 変更: `tests/repl.mjs` の R13。
- 確認: 次がすべて成功し、`tests/repl.mjs` は 14 case。GUIDE §11 の stack-depth の 3 テストも成功する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/repl.mjs target/release/tsuzuri
node tests/io.mjs target/release/tsuzuri
node tests/e2e.mjs target/release/tsuzuri
node tests/cache.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
```

### 手順 11: ドキュメント

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/command-line.md _docs/get-started.md _docs/guides/from-fsharp.md _docs/feature-status.md` が成功する。

## テスト計画

### Rust テスト

`src/repl.rs` の `#[cfg(test)] mod tests`（`cargo test --locked --lib repl::`）。Clang は起動しない。

1. `continues_incomplete_input`: `let word = (`、`match` の 3 行、`)`、空行を一つの入力にする。`square base` は 1 行で完結する。
2. `reports_errors_before_the_end`: `let = 3` は継続せず `E0002`（列 5）。
3. `splits_declarations_bindings_and_expression`: 「例」の最初の 3 行を一つの入力にすると、宣言 1（`Key::Value("square")`、1 行目の文字列）、束縛 1（`let base = 6`）、式 `square base`。
4. `keeps_bindings_without_expression`: `let a = 1; let b = a` は束縛 2、`Body::None`。
5. `classifies_bang_statements_as_actions`: `do! IO.write_line "hello"` は `Body::Action`。
6. `keys_instances_by_class_and_type`: docs/language.md の instance の例を入力にし、`Key::Instance` がクラス名と空白を除いた型の文字列になる。
7. `rejects_extern_test_and_main`: docs/language.md の `extern` と `test` の例、`def main :: i64 = 1` がそれぞれ表の `E2000` のメッセージになる。
8. `replaces_in_place_and_keeps_input_shadowing`: 置き換えは元の位置、同じ入力の `let c = 1; let c = c + 1` は二つとも残る。
9. `generates_declarations_before_bindings`: 束縛の後に入力した宣言が、生成した `Main.tz` では束縛より前に来る（再現 `p4` の `E0002` を避ける）。
10. `maps_spans_to_input_and_session`: 入力の中のエラーは `input:1:<列>`、セッションの項目のエラーは `session:<行>:<列>`、wrapper は式の先頭。
11. `overlay_project_selects_main`: `project.input()` が overlay の `Main.tz` で、作業ディレクトリの `.tz` を読まない。
12. `reads_it_type_from_semantic_index`: `square base` → `i64`、`square` → `i64 -> i64`、`"hi"` → `string`、`IO.write_line "y"` → `IO<unit>`。

`src/main.rs` の tests（`cargo test --locked --bin tsuzuri repl`）: `parses_repl_options`（`repl` と `repl -O3 --cpu native --no-cache --timeout 0`）、
`rejects_repl_paths_and_build_options`（`repl Main.tz`、`repl --target wasm32`、`repl --json`、`repl --timeout 3601`、`repl --timeout x`）。

### E2E

`tests/repl.mjs`（新規。`tests/io.mjs` の `execute` の形）。各 case は新しい一時ディレクトリを `cwd` にし、
`spawnSync(compiler, ["repl", ...args], { input, encoding: "utf8", timeout: 120_000, cwd })` で実行する。stdout は完全一致、
stderr はコードと位置の部分一致、終了コードは完全一致。期待値は手計算と手順 1 の `tsuzuri run` の出力から作る。

| ID | stdin（行を `、` で区切る） | 期待する stdout | stderr・終了コード |
| --- | --- | --- | --- |
| R1 | 「例」の stdin | 「例」の stdout | `input:1:14: error[E2005]` を含む。0 |
| R2 | `let = 3`、`1 + 1` | `it: i64 = 2` | `input:1:5: error[E0002]` を含む。0 |
| R3 | `def square :: i64 -> i64 = \x -> x * x`、`def quad :: i64 -> i64 = \x -> square (square x)`、`quad 2`、`def square :: bool -> bool = \x -> x`、`quad 2` | `it: i64 = 16` を 2 行 | `session:2:` と `error[E1` を含む。0 |
| R4 | `let a = 2`、`let b = a * 10`、`b`、`let a = 5`、`b` | `a: i64`、`b: i64`、`it: i64 = 20`、`a: i64`、`it: i64 = 50` | 空。0 |
| R5 | `let c = 1; let c = c + 1`、`c` | `c: i64`、`c: i64`、`it: i64 = 2` | 空。0 |
| R6 | `let z = 0`、`:type 10 / z`、`IO.write_line "y"`、`do! IO.write_line "x"`、`do! IO.write_line "x"`、`1 + 1` | `z: i64`、`i64`、`it: IO<unit>`、`x`、`x`、`it: i64 = 2` | 空。0（`:type` は実行しない、IO の値は実行しない、アクションは再実行しない） |
| R7 | `:load Echo.tz`、`1 + 1`、`:list` | `it: i64 = 2` | 空。0（`Echo.tz` は docs/language.md の `let!`／`let!`／`do!` の 3 行。子が stdin を読まず、アクションは `:list` に出ない） |
| R8 | `:foo`、`:type`、`:load missing.tz`、`:load notes.txt`、`:list extra`、`1` | `it: i64 = 1` | 表の `E2000` 4 件と `E2001` 1 件。0 |
| R9 | `extern` の例、`test` の例、`def main :: i64 = 1`、`1` | `it: i64 = 1` | 表の `E2000` 3 件。0 |
| R10 | 引数 `--timeout 1`。終わらない `while` のアクション、`1 + 1` | `it: i64 = 2` | `evaluation exceeded the 1-second limit` を含む。0 |
| R11 | 17 MiB を書くアクション、`1 + 1` | `it: i64 = 2` | `program output exceeded 16 MiB` を含む。0 |
| R12 | 1,100,000 bytes の文字列リテラルの 1 行、`1 + 1` | `it: i64 = 2` | `error[E0003]` を含む。0 |
| R13 | 引数 `-O3` で R1 | R1 と同じ | R1 と同じ |
| R14 | 引数 `Main.tz`、`--target wasm32`、`--timeout 3601` の 3 回 | 空 | 表のメッセージ。1 |

R10・R11 の Tsuzuri のコード（`while` と繰り返しの `IO.write_line`）は docs/language.md の該当節の形から作り、`tests/repl.mjs` に入れる前に
`target/release/tsuzuri check` で確かめる。native だけなので WASM の case はない。生成コードを変えないので `live == 0` の確認は要らない。

### 既存テストへの影響

なし。`HELP` の文字列を照合するテストがあれば、`repl` の行の追加だけを反映する。

### 性能

閾値は設けない。完了報告に、`printf '1 + 1\n' | /usr/bin/time -p target/release/tsuzuri repl --no-cache` の `real` を `-O0` と `-O3` で 1 回ずつ記録する（ベンチマークではない）。

## ドキュメント

- `_docs/tools/command-line.md`: 「コマンド」の表に `repl`。新しい節「REPL」（書式、入力の規則、表示の書式、コマンド、状態を持ち越さないこと、上限）。セッションの例は `text` の fence にする。
- `_docs/get-started.md`: 「サンプルを試す」の後に短い節「REPL で試す」。
- `_docs/guides/from-fsharp.md`: 「提供しない F# / .NET 機能」に fsi との違い（`;;` がなく空行で終える、毎回の再実行、`it` を残さない、`#r` なし）。
- `docs/architecture.md`: `src/lsp.rs` の行の近くのモジュール表に `src/repl.rs`（再コンパイル型の REPL、overlay の `Main.tz`）。
- `README.md`: CLI の使い方と、E2E のコマンド一覧に `node tests/repl.mjs target/release/tsuzuri`。
- `_docs/feature-status.md` の G13 行と `_features/README.md` の状態欄: Phase 1 の完了を GUIDE §10 の書き方で記録し、Phase 2・3 は計画のままにする。

## 受け入れ条件

- [ ] `tsuzuri repl` が Phase 1 の仕様（分類、表示の書式、再定義、状態、コマンド、複数行、上限、診断）どおりに動く。
- [ ] Rust テスト 14 件と `tests/repl.mjs` の 14 case が成功し、R13 で `-O0` と `-O3` の stdout が一致する。
- [ ] IO の二重実行を起こさない（R6・R7）。
- [ ] 構文エラー・型エラー・トラップ・時間切れ・出力の超過の後もセッションが壊れない（R1〜R3・R10・R11）。
- [ ] `tsuzuri run` の挙動が変わらない（`tests/io.mjs`・`tests/e2e.mjs`・`tests/cache.mjs`）。
- [ ] 新しい crate、`unsafe`、parser・checker・LLVM・cache の変更がない。
- [ ] ドキュメントを更新し、`scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 子プロセスの stdin を継承すると、REPL の残りの入力を子が読んでしまう。`Stdio::null()` を必ず指定する。R7 が検出する。
- stdout と stderr を同じ thread で順に読むと、先に読まない側の pipe が満杯になって deadlock する。両方を別 thread で読む。
- 時間切れや超過で `kill()` した後は必ず `wait()` する（zombie と一時ディレクトリの削除失敗を防ぐ）。
- `run_with_diagnostics` の共通化で `--json` の stderr の扱い（中継しないで `E2005` に含める）を落としやすい。`tests/io.mjs` の `--json` の case で確かめる。
- `SemanticIndex` には同じ span に複数の entry がある。`detail` が `it: ` で始まるものを選び、優先度だけで選ばない。
- 型の表示の文字列で分岐しない。IO は `IO<unit>` と表示されるが、表示の有無は wrapper の `E1005` で決める（D3）。
- 生成した `Main.tz` を project の root にしないと、`Project::source_for` の既定の `self.root` が別のファイルを指し、位置の写像が壊れる（手順 7 のテスト 11）。
- 末尾での parse 失敗の判定は `trim_end()` した長さと比べる。改行の有無で継続の判定が変わらないようにする。CR は行ごとに除く。
- 行頭で区切るので、宣言の前の `///` の文書コメントは直前の項目の文字列に入る。置き換えで失われうるが、Phase 1 では直さない。
- REPL の既定 `-O0` を `BuildOptions::default()` に入れると `build`／`run` の既定（`-O3`）が変わる。`parse_arguments` の REPL の分岐だけで設定する。
- 入力ごとに whole-build cache の entry ができる。G11 の上限つき GC に任せ、REPL 専用の cache を作らない。
- トラップ時に子が stderr へ出す `trap: ... at <一時パス>` は環境ごとに違う。E2E はこの行を照合しない。
- R10 のように時間切れを試す case でも、経過時間を assert しない（共有 CI の速度に依存させない）。

## 対象外

- Notebook の統合（VS Code notebook は別途）、実行中のプロセスの状態保持、JIT（Phase 3）。
- 行編集・履歴・補完（新しい crate が要る）。Ctrl-C の個別処理（SIGINT は REPL と子プロセスをともに終了させる）。
- WASM での評価、作業ディレクトリのプロジェクトのモジュールの参照、`extern`・`test`・`main` の入力、警告の表示、`it` の保持。

## 決定事項

### D1: 実行方式と既定の target・最適化

- 決定: 入力ごとに生成した `Main.tz` を解析し、`run` と同じ経路で native 実行ファイルを作って実行する。既定は `-O0`。`--target` は受け付けない。
- 理由: JIT はない（LLVM の C API に結合しない方針）。wasm32 は実行コードだけの `Main.tz` をビルドできず（`E2004`）、Node の起動と `tsuzuri_io` の host も要る。
  PB01 の前は `-O3` だと埋め込みランタイムの Clang に毎回 0.43 s かかるが、`-O0` は 0.06 s。結果は最適化レベルに依存しない。PB01・PB05 はこの経路のまま速くなる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し提案: 旧版の `tsuzuri repl [--target native|wasm32]` から wasm32 を外した。WASM の REPL は IO の入口と Node の host を使う別チケットにする。

### D2: セッションの表現

- 決定: セッションは宣言と束縛の source 文字列の列（`Session`）。空の一時ディレクトリを root にし、生成した `Main.tz` を overlay で渡す。作業ディレクトリの `.tz` は読まない。
- 理由: `Main.tz` の既存の規則（宣言の後に束縛、最後に結果式）をそのまま使え、parser と checker を変えずに済む。E03 の再帰読み込みで無関係なファイルを拾わない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 式の包み方と結果の表示

- 決定: 検査用 `let it = (E)` で型を得て、表示用 `Display.display (ref it)` を結果式にし、`it: T = <表示>` を出す。unit は表示なし、Display がなければ `it: T`。
- 理由: 既存の入口の自動表示は数値・bool・文字列系だけで、Display はレコードなども扱える（再現 `p1`・`p2`）。型は LSP と同じ semantic index の文字列で、別の表示規則を作らない。
  括弧で包むと複数行の式も同じ規則で扱える（`p8`）。
- 状態: 既定案（実装者はこの案に従う）

### D4: 再定義と依存の再検査

- 決定: 同じ key の項目は元の位置で置き換え、生成した全体を解析する。依存する項目がエラーになれば入力全体を拒否する。
- 理由: 再コンパイル型ではソースが唯一の状態で、fsi のように古い定義を隠して残すには名前の書き換えが要る。拒否すれば、セッションは常に検査を通る状態に保たれる。
- 状態: 既定案（実装者はこの案に従う）

### D5: 入力間の状態と副作用

- 決定: 状態を持ち越さない。受理済みの束縛を毎回すべて再実行する。IO はアクション入力でだけ実行し、アクションはセッションに残さない。子の stdin は null。
- 理由: 旧版の「IO を伴う束縛は拒否」は、トップレベルの `let` が IO を実行しない（`p7`）ことで自然に満たされる。`let!` はアクション入力になる。`extern` は D10 で入力できない。
  残る重複は stderr への Debug 出力だけで、結果は変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D6: コマンド

- 決定: `:quit`、`:type E`、`:load PATH`、`:list`、`:reset` の 5 つ。`:help` は作らず、未知のコマンドの診断が一覧を示す。
- 理由: 旧版のコマンドをそのまま確定した。`:load` を一つの入力として扱うと、ファイルとキーボードで規則が一つになる。
- 状態: 既定案（実装者はこの案に従う）

### D7: 複数行入力

- 決定: 1 行目の parse が入力の末尾で失敗したときだけ継続し、空行か EOF で終える。
- 理由: 既存の parser だけで判定でき、fsi の `;;` のような区切りを足さない。完結した 1 行の後の行を別の入力にする規則は `Main.tz` のトップレベルと同じ。
- 状態: 既定案（実装者はこの案に従う）

### D8: 上限と時間制限

- 決定: 評価の実行時間は `--timeout`（既定 10 s、0 で無制限、最大 3600）、捕捉する stdout は 16 MiB、入力と生成した `Main.tz` は 1 MiB。
- 理由: Ctrl-C の処理（crate か `unsafe` が要る）なしで、終わらない評価から REPL を守る。10 s は対話で待てる長さの目安で、`--timeout` で変えられる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 決定性と診断の位置

- 決定: stdout は入力だけで決まる。prompt と見出しは stdin が端末のときだけ。診断の path は `input`（`:load` はそのパス）と `session`（`:list` の行）。
- 理由: 生成した `Main.tz` の行番号は利用者に見えず、一時パスは環境ごとに違う。scripted stdin の E2E を完全一致で書ける。
- 状態: 既定案（実装者はこの案に従う）

### D10: 受け付けない宣言

- 決定: `extern`、`test`、`main` の定義は `E2000` で拒否する。
- 理由: `extern` は外部 object のリンク手段がなく、再実行で副作用が重複する。`test` は `tsuzuri test` の役割。`main` はトップレベルの実行コードと併用できない（`E2004`）。
- 状態: 既定案（実装者はこの案に従う）

### D11: 警告と cache

- 決定: 警告は表示しない。cache は `run` と同じく既定で使い、`--no-cache` で止める。
- 理由: 束縛の未使用などの警告が入力のたびに繰り返される。cache は同じ入力の再評価で当たり、G11 の GC が大きさを抑える。
- 状態: 既定案（実装者はこの案に従う）

### D12: Phase 2 のスクリプト実行

- 決定: `tsuzuri script FILE.tz [args...]` は単一ファイルを `Main.tz` として overlay し、先頭の shebang 行を無視する。
- 理由: D2 の overlay をそのまま使える。shebang の無視は lexer の規則の変更で、CLI の新しいコマンド名も確定が要る。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D13: Phase 3 の JIT

- 決定: 採用しない。検討する場合は、LLVM の C API への結合と状態の保持を含めて人間が判断する。
- 理由: docs/architecture.md の方針と衝突し、配布物とビルドの前提が変わる。
- 状態: 要承認（承認前は Phase 3 に着手しない）
