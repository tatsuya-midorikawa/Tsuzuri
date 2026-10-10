# コンパイラ オプション

`tsuzuri` のフラグは、サブコマンドごとに使えるものが違います。サポートされていないオプションの組み合わせを指定した場合は、コード生成へ進む前に `E2000` でエラーになります。同じソースでも、ターゲットや最適化を変えると成果物が変わるので、既定値を知っておくと再現しやすくなります。

このページの表は、`src/main.rs` のヘルプと引数解析、`BuildOptions::validate` で確認したものです。コマンドの流れ自体は [コンパイラの使い方](usage.md) を見てください。

## この記事のポイント

- 最適化の既定は `-O3` です。`test` と `repl` だけ既定が `-O0` です（`bench` は `-O3`）。fast-math は使いません。
- `--emit` の既定は、native なら `exe`、`wasm32` / `wasm64` なら `wasm` です。
- リンク入力は、ネイティブ実行ファイルの `build`、`run`、`test` と、共有ライブラリの `--emit shared` だけが受けます。
- フラグの重複や、サブコマンドとの不一致は終了コード 2 の `E2000` です。
- ツールの場所は、環境変数、配布物の `bin/`、PATH の順です。

## どのサブコマンドが受け付けるか

`lsp` は引数を取りません。`new` はディレクトリと `--namespace` だけ、`toolchain info` は 2 語ぴったりです。`bindgen` はヘッダー 1 つと、`-o`（必須、`.tz`）、`--include-dir DIR`、`--buffer FUNCTION:POINTER:LENGTH`、`--consume FUNCTION:PARAMETER`（この 3 つは繰り返し可）、`--json` だけを受け付けます。`repl` はパスを取らず、`-O0`〜`-O3`、`--cpu`、`--no-cache`、`--timeout SECONDS` だけを受け付けます。`script` は、ファイルより前に置いたものだけをオプションとして読み、ファイルより後ろはすべてプログラムの引数です。それ以外は、次の表のとおりです。`○` が受け付け、空欄は `E2000` です。

| オプション | check | build | run | test | bench | doc | fmt | repl | script |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `--json` | ○ | ○ | ○ | ○ | ○ | ○ | ○ | | ○ |
| `--deny-warnings` | ○ | ○ | ○ | ○ | ○ | ○ | | | ○ |
| `--warn implicit-copy` | ○ | ○ | ○ | ○ | ○ | ○ | | | ○ |
| `-o` / `--output` | | ○ | | ○（`-g` と） | | ○（必須） | | | |
| `--target` | | ○ | | ○ | ○（native だけ） | | | | |
| `--emit` | | ○ | | | | | | | |
| `-O0`〜`-O3` | | ○ | ○ | ○ | ○ | | | ○ | ○ |
| `--cpu` | | ○ | ○ | | | | | ○ | ○ |
| `--no-cache` | | ○ | ○ | | | | | ○ | ○ |
| `-g` / `--debug-info` | | ○ | ○ | ○（1 件の `--index` と `-o`） | | | | | |
| `--debug-output` | | ○ | ○ | | | | | | |
| `--trap-info` | | ○ | ○ | | | | | | |
| `--trap-mode` | | ○ | | | | | | | |
| `--allocator` | | ○ | | | | | | | |
| `--freestanding` | | ○ | | | | | | | |
| `--wasm-feature` | | ○ | | | | | | | |
| `--wasm-host` | | ○ | | | | | | | |
| `--wasm-max-memory` | | ○ | | ○ | | | | | |
| `--wasm-stack-size` | | ○ | | ○ | | | | | |
| `--link` / `-l` / `-L` | | ○ | ○ | ○ | ○ | | | | ○ |
| `--list` / `--filter` / `--index` | | | | ○ | ○ | | | | |
| `--coverage PATH` | | | | ○ | | | | | |
| `--seed N` | | | | ○ | | | | | |
| `--check` | | | | | | | ○ | | |
| `--samples N` | | | | | ○ | | | | |
| `--timeout` | | | | | | | | ○ | |

`--deny-warnings` と `--warn` は、ヘルプでは `check` / `build` / `run` が中心です。実装では警告の表示が `doc` と `test` の前にも走るので、この 2 つでも効きます。`fmt` は明示的に拒否します。

同じオプションを 2 回以上指定すると `E2000` になります。複数回の指定が許可されているのは `--link`、`-l`、`-L`、`--index`、および異なる機能名を指定した `--wasm-feature` のみです。`--link`、`-l`、`-L` の合計は最大 256 個までです。

未知のフラグは `unknown option '...'; use --help` です。値を取り忘れると `--output needs a value` のように、そのフラグの名前で止まります。

## 出力とターゲット

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `-o` / `--output PATH` | 入力の拡張子を差し替えたパス | 成果物、または `doc` のディレクトリ。親は作られます |
| `--target native\|wasm32\|wasm64` | `native` | `wasm64` は 64-bit の線形メモリです |
| `--emit exe\|object\|llvm\|header\|wasm\|wgsl\|wgsl-relaxed\|spirv\|spirv-relaxed\|shared\|bindings-js\|bindings-cs\|bindings-py\|bindings-cpp` | native は `exe`、WASM は `wasm` | 何を残すか |

拡張子は、native の実行ファイルが macOS / Linux で空、Windows で `.exe`、オブジェクトが `.o` または Windows の `.obj`、LLVM IR が `.ll`、ヘッダーが `.h`、WASM が `.wasm`、WGSL が `.wgsl`、SPIR-V が `.spv`、共有ライブラリが macOS で `.dylib`、Linux で `.so`、バインディングが JavaScript の `.mjs`、C# の `.cs`、Python の `.py`、C++ の `.hpp` です。

`--emit bindings-js` は `--target wasm32` だけで使え、`<name>.mjs` の隣に TypeScript 宣言 `<name>.d.mts` も書きます。`-o` は `.mjs` で終わる必要があります。`-O` は受け付けて無視し、`--trap-info`、`--debug-info`、`--debug-output`、`--wasm-feature simd128`、`--allocator` は `.wasm` のビルドに付けるよう `E2000` で求めます。`--wasm-feature threads` を付けると、Web Worker のスレッドプールを作るグルーになります。使い方は [WebAssembly への出力](webassembly.md#型付きのバインディングを生成する) にあります。

`--emit shared` は native の共有ライブラリです。Windows ではまだ使えません（`E2000`）。`--emit bindings-cs`、`bindings-py`、`bindings-cpp` は `--target native` だけで使え、`-o` の拡張子を除いたファイル名がライブラリの名前です。`-O` は受け付けて無視し、`--trap-info`、`--debug-info`、`--debug-output`、`--allocator`、`--freestanding` は共有ライブラリのビルドに付けるよう `E2000` で求めます。`--trap-mode return` は、ライブラリとバインディングの両方に付けます。使い方は [ネイティブ連携](native-interop.md#共有ライブラリと各言語のバインディング) にあります。

`--emit exe` は `--target native`、`--emit wasm` は `wasm32` か `wasm64` が必要です。逆にすると、次の 1 文で止まります。

```text
<command line>:1:1: error[E2000]: '--emit exe' requires '--target native'; '--emit wasm' requires '--target wasm32' or '--target wasm64'
```

`wgsl`、`wgsl-relaxed`、`spirv`、`spirv-relaxed` は実験的な GPU カーネル用です。`wgsl` は厳密な `i32`／`i32u` のカーネル、`wgsl-relaxed` は `f32` を含む緩いカーネルで、出力の 1 行目に `// tsuzuri-gpu float=relaxed …` を付けます（[Gpu](../built-in-types-and-modules/gpu.md#緩い-f32-カーネル)）。`spirv` は Vulkan 用の厳密なカーネルで、`i32`、`i32u`、`i64`、`i64u`、`f32` を受け、`spirv-relaxed` は緩い `f32`、`i32`、`i32u` のカーネルです。SPIR-V 1.3 のバイナリ（32 bit ワードのリトルエンディアン）を書き、同じソースから、同じバイト列ができます（[Gpu](../built-in-types-and-modules/gpu.md#spir-v-を出す)）。どれも `export` が 1 つの専用プロジェクトを受け、`--target`、`-O`、`--cpu`、デバッグ、WASM 機能とは一緒に使えません。SPIR-V の出力は、ビルドのキャッシュを使いません。

## 最適化と CPU

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `-O0` `-O1` `-O2` `-O3` | `build` / `run` / `script` / `bench` は `-O3`、`test` と `repl` は `-O0` | LLVM の最適化。fast-math や再結合は使いません |
| `--cpu generic\|native` | `generic` | native の実行ファイルとオブジェクト。`native` はこの機械の命令セットで、古い CPU では動きません |

`check` は最適化も CPU も見ません。型が通れば、どの `-O` でも同じ診断です。`doc` と `fmt` も最適化を受け付けません。`test` は `-O` を受け、`--cpu` は拒否します。

`--cpu native` は、native の `exe` と `object` だけです。`llvm`、`header`、WASM では `E2000` です。対応するホストは x86、x86_64、arm、aarch64 です。

整数の折り返し、NaN、符号付きゼロは、`-O3` でも変えません。次の関数は `-O0` でも `-O3` でも 42 です。

```tsuzuri run=42
def add :: i64 -> i64 -> i64 = \left right ->
    left + right

add 20 22
```

実行結果:

```text
42
```

## 診断とデバッグ

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `--json` | オフ | 診断を stderr の 1 行 1 JSON にする |
| `--deny-warnings` | オフ | 警告があれば、コード生成や実行の前に終了コード 1 |
| `--warn implicit-copy` | オフ | 配列とリストの暗黙コピー `W1006` を出す。生成コードは変わらない |
| `-g` / `--debug-info` | オフ | ソースレベルの DWARF。`header` では不可。Windows の MSVC のリンカーでは CodeView も出し、実行ファイルの隣に PDB を書く。表示とステップ実行は [デバッグ](debugging.md) |
| `--debug-output` | オフ | WASM の `Debug` 出力 import を有効にする。native は常に書く |
| `--trap-info` | `run` はオン、`build` はオフ | トラップ位置と `<output>.trap.json`。`header` では不可 |
| `--trap-mode return` | オフ | native の `object` / `llvm` / `header` / `shared` と C#・Python・C++ のバインディング。各 export に `tsuzuri_try_<name>` を足す |

`--trap-mode return` は、`object`、`llvm`、`shared` では `--trap-info` も有効にします。`header` では位置表を出さないので、`--trap-info` とは組み合わせません。戻り値は 0 が成功、1 がトラップ、2 が同じスレッドでの再入です。詳しくは [言語仕様のトラップ位置](../../../docs/language.md#トラップ位置) を見てください。

`--json` のフィールドは [診断メッセージとエラーコード](diagnostics.md) にあります。引数エラーも同じコードで、パスは `<command line>` です。

```text
{"severity":"error","code":"E2000","message":"missing .tz, .tt, or .tc input or project directory; use --help","path":"<command line>","span":{"start":0,"end":0,"line":1,"column":1,"end_line":1,"end_column":1}}
```

このときの終了コードは 2 でした。

## WebAssembly

| オプション | 既定 | 制約 |
| --- | --- | --- |
| `--wasm-max-memory SIZE` | 16 MiB | `wasm32` は 4 GiB − 64 KiB まで、`wasm64` は 16 GiB まで。64 KiB の倍数 |
| `--wasm-stack-size SIZE` | 1 MiB | WASM 出力だけ。16 の倍数で、64 KiB 以上。メモリ上限はスタック + 64 KiB 以上 |
| `--wasm-feature simd128` | オフ | `wasm32` / `wasm64` の `object`、`llvm`、`wasm` |
| `--wasm-feature threads` | オフ | `wasm32` の `object` か `wasm`。WASI や `--allocator host` とは排他 |
| `--wasm-feature jspi` | オフ | `wasm32` / `wasm64` の `object`、`llvm`、`wasm` と、wasm32 の `bindings-js`。`Async.block_on` 用。threads / WASI とは排他 |
| `--wasm-feature webgpu` | オフ | `wasm32` の `object`、`llvm`、`wasm`。`Gpu.request Gpu.WebGpu` 用。threads / WASI / `bindings-js` とは排他 |
| `--wasm-host wasi` | オフ | `wasm32` の `object` か `wasm`。既定の wasm32 は OS API を拒否します |

`SIZE` はバイト数か、`KiB` / `MiB` / `GiB` です。`64MB` のような 10 進の単位は受けません。`67108864` と `64MiB` は同じです。

`--wasm-stack-size` は、リンク済みの WASM にだけ埋めます。オブジェクトや LLVM IR を自分でリンクするときは、`wasm-ld -z stack-size` を使います。`test` でメモリやスタックを変えるときは、`--target wasm32` か `wasm64` が必要です。

`Async.block_on` を使う WASM では、`.wasm` と `--emit bindings-js` のどちらを作るときも `--wasm-feature jspi` が必要です。省略は生成前に `E2000` となり、同期型のグルーへ置き換えません。

`--wasm-feature webgpu` は、`Gpu.request Gpu.WebGpu` を使うプログラムのモジュールに、`tsuzuri_gpu.open` と `tsuzuri_gpu.run` の 2 つの import を足します。付けなければ import は増えず、`Gpu.request Gpu.WebGpu` は `Unavailable` です。ホスト側は `src/runtime/webgpu.mjs` の `createGpuImports` が作ります（[WebAssembly への出力](webassembly.md#webgpu-デバイス)）。

`Tsuzuri.toml` の `[wasm]` は、コマンドラインが省略した値の既定になります。コマンドラインが優先です。マニフェストの値だけが不正なときは、終了コード 1 で、メッセージの末尾に `(after applying the root package's [wasm])` が付きます。

呼び出し方とメモリの渡し方は [WebAssembly への出力](webassembly.md) にまとめています。

## リンクと allocator

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `--link PATH` | なし | ホストのオブジェクトか静的ライブラリ。繰り返せる |
| `-l NAME` | なし | システムライブラリ名。例は `-l sqlite3`。`lib` や拡張子は付けない |
| `-L DIR` | なし | `-l` の探索ディレクトリ |
| `--allocator system\|host\|counting` | `system` | `object`、`llvm`、`header`、WASM のヒープ。`shared` は `counting` まで |
| `--freestanding` | オフ | C ライブラリ不要の native `object` / `llvm` / `header`。`--allocator host` が必須 |
| `--no-cache` | キャッシュ有効 | `build`・`run`・`script` で、成果物のキャッシュと構文解析の結果のキャッシュを読まない、書かない。`repl` では成果物のキャッシュだけ（`repl` は構文解析の結果のキャッシュを使わない）。ほかのコマンドでは `E2000` |

`[native]` の `link`、`libraries`、`search` が先で、コマンドラインがそのあとに続きます。実行ファイルと `--emit shared` 以外、たとえば `--emit object` や WASM にリンク入力を付けると拒否されます。

```text
link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe
```

`--allocator host` は、ホストが定義する `tsuzuri_host_alloc`、`tsuzuri_host_free`、`tsuzuri_host_realloc` を呼びます。WASM ではモジュール `tsuzuri_heap` の `alloc`、`free`、`realloc` として import します。`counting` は `tsuzuri_alloc_stats` で回数を読めます。どちらも `--emit exe`、`wgsl`、`spirv`、`--trap-mode return` とは排他です。`host` は `--wasm-feature threads` とも排他です。

`--freestanding` は IO、OS API、タスク、`Debug`、プログラム引数、`Gpu.WebGpu`・`Gpu.Vulkan`・`Gpu.Auto` のデバイスを `E2000` で拒否します。`--trap-info` と `--debug-output` とも一緒には使えません。トラップは `llvm.trap` だけです。

## 組み合わせがエラーになる場合

引数解析の時点で検出される構文・指定誤りは終了コード 2、ビルド処理中にソースコードや設定を検証して検出されるものは終了コード 1（エラーコードはいずれも `E2000`）になります。遭遇しやすい主な組み合わせエラーを以下にまとめます。

| 条件 | メッセージの要点 |
| --- | --- |
| `--no-cache` を `check` などに付ける | `--no-cache is only valid with build, run, script, or repl` |
| `--wasm-feature` を `build` 以外に付ける | `--wasm-feature is only valid with build` |
| `threads` なのに wasm32 の object / wasm でない | `--wasm-feature threads requires wasm32 object or WASM output` |
| `jspi` なのに WASM の object / llvm / wasm / bindings-js でない | `--wasm-feature jspi requires wasm32 or wasm64 object, LLVM IR, WASM, or JavaScript bindings output` |
| `Async.block_on` に `jspi` がない | `Async.block_on on WebAssembly needs --wasm-feature jspi` |
| `jspi` と threads / WASI | `--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host` |
| `webgpu` なのに wasm32 の object / llvm / wasm でない | `--wasm-feature webgpu requires wasm32 object, LLVM IR, or WASM output; the JavaScript bindings do not provide the WebGPU imports, so instantiate the module with createGpuImports of src/runtime/webgpu.mjs` |
| `webgpu` と threads / WASI | `--wasm-feature webgpu cannot be combined with --wasm-feature threads or --wasm-host: its imports suspend the WebAssembly stack with JavaScript Promise Integration` |
| `simd128` が header や native | `--wasm-feature simd128 requires wasm32 or wasm64 object, LLVM IR, or WASM output` |
| WASI と threads | `--wasm-host wasi cannot be combined with --wasm-feature threads` |
| WASI が wasm32 の object / wasm でない | `--wasm-host wasi requires wasm32 object or WASM output` |
| メモリ上限が 64 KiB の倍数でない、または上限超え | `at most 4 GiB - 64 KiB on wasm32` / `at most 16 GiB on wasm64` |
| スタックがメモリに収まらない | `must be at least the stack size plus 64 KiB` |
| `--cpu native` が exe / object でない | `'--cpu native' requires native executable or object output` |
| `--emit bindings-js` が `--target wasm32` でない | `'--emit bindings-js' requires '--target wasm32'` |
| `--emit bindings-js` に `.wasm` 用のオプション | `--trap-info is not valid for bindings output; pass it when building the .wasm`（各オプション名で同じ形） |
| `--emit bindings-js` の `-o` が `.mjs` でない | `bindings output must end with '.mjs'; declarations are written next to it as '<name>.d.mts'` |
| `--emit shared` や `bindings-cs` などが `--target native` でない | `'--emit shared' requires '--target native'`（`bindings-cs` などは各名前で同じ形） |
| Windows で `--emit shared` | `'--emit shared' is not supported on Windows yet (G10); build an object with --emit object and link it into a DLL` |
| `--emit shared` と `--allocator host` | `--allocator host cannot be combined with --emit shared: ...` |
| `bindings-cs` などに共有ライブラリ用のオプション | `--trap-info is not valid for bindings output; pass it when building the shared library`（各オプション名で同じ形） |
| `bindings-cs` などの `-o` の拡張子が違う | `bindings output must end with '.cs'; its file name, without the extension, names the shared library`（`.py`、`.hpp` も同じ形） |
| `--freestanding` が exe | `--freestanding requires object, LLVM IR, or header output` |
| `--freestanding` なのに allocator が host でない | `--freestanding requires --allocator host` |
| `fmt` に `-O` や `--deny-warnings` | `fmt does not use optimization, CPU tuning, or compiler warning options` |
| `check` に `-O` | `check does not use an optimization level` |
| `test` に `--cpu` | `test does not use CPU tuning` |
| `doc` に `-o` が無い | `doc requires -o or --output with an output directory` |
| `--filter` を `test`・`bench` 以外に付ける | `--filter, --list, and --index are only valid with test or bench` |
| `test` に `-g` と `-o` の片方だけ | `debugging a test needs both -g and -o with the runner's path` |
| `test -g` の `--index` が 1 つでない、または `--list`・`--filter` と組み合わせる | `debugging a test needs exactly one --index, without --list or --filter` |
| `test -g` に WASM のターゲット | `debugging a test requires the native target` |
| `--samples` を `bench` 以外に付ける | `--samples is only valid with bench` |
| `--seed` を `test` 以外に付ける | `--seed is only valid with test` |
| `--seed` が 0〜18446744073709551615 の整数でない | `property seed must be an integer between 0 and 18446744073709551615` |
| `--samples` が 1〜1000 の整数でない | `bench samples must be an integer between 1 and 1000` |
| `bench` に `--target wasm32`／`wasm64` | `tsuzuri bench supports only the native target` |
| `bench` に `--cpu` | `bench does not use CPU tuning` |
| `--coverage` を `test` 以外に付ける | `--coverage is only valid with test` |
| `test --coverage` と `--target wasm32`／`wasm64` | `test coverage supports only the native target` |
| `test --coverage` と `--list` | `coverage cannot be combined with --list` |
| `test --coverage` と `-g` | `coverage cannot be combined with -g; a debug runner only builds` |
| 入力が 2 つ | `pass one .tz, .tt, or .tc file or project directory` |
| WASM 機能名が `relaxed-simd` など | `supported WASM features are 'simd128' and 'threads'; relaxed SIMD is not supported` |

ビルドがソースを読んでから出す `E2000` には、既定の wasm32 で `File` や `Env` などに到達した場合と、`--freestanding` で IO やタスクに到達した場合があります。前者は、native にするか、`--wasm-host wasi` にするか、ホスト不要の `Path` と `Random.Pcg` に限る、というメッセージです。

> [!WARNING]
> 終了コード 2 の `E2000` は、まだファイルを書いていません。終了コード 1 の `E2000` は、解析やリンクの途中です。既存の成果物は、コンパイルエラーでは置き換えません。

## 環境変数

ツールは次の順で決まります。空の環境変数も「設定済み」なので、そのときは配布物や PATH へ進みません。

```mermaid
flowchart TD
  ask["必要なツール"] --> env{"環境変数はあるか"}
  env -->|ある| useEnv["その値を使う。空でも次へ進まない"]
  env -->|ない| bundled{"配布物の bin に同梱名があるか"}
  bundled -->|ある| useBin["同梱の実行ファイル"]
  bundled -->|ない| path["PATH の既定名"]
```

| 変数 | 既定の名前 | 同梱の名前 | 用途 |
| --- | --- | --- | --- |
| `TSUZURI_CLANG` | `clang` | `tsuzuri-clang` | コンパイルとリンク、`bindgen` のヘッダー解析。LLVM 17 以降 |
| `TSUZURI_WASM_LD` | `wasm-ld` | `wasm-ld` | WebAssembly のリンク |
| `TSUZURI_LLVM_LINK` | `llvm-link` | `llvm-link` | macOS で、タスクを含むデバッグ用オブジェクト |
| `TSUZURI_DSYMUTIL` | `dsymutil` | `dsymutil` | macOS のデバッグ実行ファイル |
| `TSUZURI_CACHE_DIR` | OS ごとのキャッシュ | なし | ビルドキャッシュと構文解析の結果のキャッシュ（`frontend/`）のルート。空ならどちらも使わない |

`node` は PATH だけです。WASM テスト用の環境変数はありません。

`TSUZURI_CPU_FORCE` はコンパイル時のオプションではなく、生成されたネイティブバイナリが実行開始時に CPU 機能を自動判定する際に読み込む実行時環境変数です。指定可能な値は `baseline`、`sse4.2`、`avx2`、`avx512`、`sve`、`sve2` です。未知の名前や現在の CPU が対応していない機能レベルを指定した場合は、`Tsuzuri CPU runtime: requested variant is unavailable or unknown` を出力してプログラムを終了します。テスト目的で実行バージョンを固定したい場合を除き、通常は設定不要です。

`Gpu.request Gpu.WebGpu` を使う native のプログラムは、実行時に `TSUZURI_WEBGPU_LIBRARY`（読み込む wgpu-native のパス。設定すればそれだけを書いたとおりに試し、空なら WebGPU を無効にする。設定がなければ、システムの場所の絶対パスだけを探し、作業ディレクトリは探さない）と `TSUZURI_GPU_DEBUG`（空でなければ、`Unavailable` の理由とデバイスでの実行を標準エラーに出す）を読みます。コンパイル時のオプションでもキャッシュのキーでもありません。詳しくは [Gpu](../built-in-types-and-modules/gpu.md#native-wgpu-native-を実行時に読み込む) にあります。

`Gpu.Vulkan` か `Gpu.Auto` を使う native のプログラムは、実行時にさらに `TSUZURI_VULKAN_LIBRARY`（読み込む Vulkan のローダーのパス。設定すればそれだけを書いたとおりに試し、空なら Vulkan を無効にする。設定がなければ、システムの場所の絶対パスだけを探し、作業ディレクトリは探さない）と `TSUZURI_GPU_AUTO_MIN_WORK`（`Gpu.Auto` の規則を「lane 数 × カーネルの重みが指定の数以上」に置き換える。`0` は、デバイスが使えるなら常に Vulkan）を読みます。ローダーが見つけるドライバは、Vulkan のローダー自身の変数（`VK_DRIVER_FILES` など）に従います。どれも、コンパイル時のオプションでもキャッシュのキーでもありません。詳しくは [Gpu](../built-in-types-and-modules/gpu.md#native-vulkan-のローダーを実行時に読み込む) と [Gpu.Auto](../built-in-types-and-modules/gpu.md#gpuauto-で呼び出しごとに選ぶ) にあります。

キャッシュのキーには、上の 4 つのツール変数に加えて `PATH`、`SDKROOT`、`MACOSX_DEPLOYMENT_TARGET`、Windows の `INCLUDE` / `LIB`、`SOURCE_DATE_EPOCH`、`DEVELOPER_DIR` などが入ります。同じソースでも、ツールを差し替えるとキャッシュは別エントリになります。

## まとめ

- 使えるフラグはサブコマンドで決まります。迷ったら `tsuzuri --help` と、この表の空欄を見てください。
- 既定は native、`-O3`、generic CPU、system allocator、キャッシュ有効です。`test` だけ `-O0` です。
- WASM のメモリとスタックは、コマンドラインが `[wasm]` より優先します。
- 組み合わせの `E2000` は、引数なら終了コード 2、ソースを見たあとなら 1 です。
- ツールは環境変数、配布物、PATH の順です。空の環境変数も優先されます。

## 関連項目

- [コンパイラの使い方](usage.md)
- [コンパイラ ディレクティブ](directives.md)
- [診断メッセージとエラーコード](diagnostics.md)
- [WebAssembly への出力](webassembly.md)
- [ネイティブ連携 (C ABI)](native-interop.md)
- [言語仕様の公開 ABI](../../../docs/language.md#公開-abi)
- [言語リファレンスの目次](../index.md)
