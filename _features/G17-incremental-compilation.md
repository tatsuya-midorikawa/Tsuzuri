# G17: モジュール単位の増分コンパイルと規模上限の見直し

| 項目 | 内容 |
| --- | --- |
| ID | G17 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | G11, E03, (G20) |
| 後続 | G12, G13 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 承認済み（2026-10-08、D-41）: D5（`check`・`test`・`doc` でも既定で frontend cache を読み書きする）、D10（Phase 2: 本体の検査結果の再利用）、D11（Phase 3: 上限の新しい値） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: 毎回の全体コンパイル・単一 LLVM モジュールと、プログラム全体の特殊化上限 1,024 などの固定上限が大規模開発の障壁になる |
| 手本にする既存実装 | cache root・marker・一時名と rename・GC: `src/cache.rs` の `default_root`・`BuildCache::open`・`BuildCache::store`・`BuildCache::evict`・`read_regular`・`Sha256::field`。span と文書を除いた AST の正準形: `src/formatter.rs` の `ast_fingerprint`・`canonicalize`。parse の唯一の入口: `src/lib.rs` の `analyze_inputs_indexed_all`。一時 cache を使う実 CLI の E2E: `tests/cache.mjs`（`cli`・`concurrent`・`TSUZURI_CACHE_DIR`）。合成プロジェクト: `benchmarks/run-cache.mjs` |
| 主な影響ファイル | `src/syntax_codec.rs`（新規）, `src/frontend_cache.rs`（新規）, `src/lib.rs`, `src/driver.rs`, `src/main.rs`, `src/cache.rs`（`read_regular` の可視性だけ）, `tests/frontend_cache.mjs`（新規）, `README.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_docs/tools/build-and-cache.md`, `_docs/tools/command-line.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

変更していないモジュールの字句解析・構文解析を毎回やり直さないようにし、Rust の増分コンパイルや C# の Roslyn の増分解析に近い
フロントエンドの再利用の土台を作る。モジュールごとに parse 結果と interface summary（公開シグネチャ・record・union・class・
instance などの宣言の頭部）を内容の hash で鍵付けしてディスクに保存し、どのモジュールの interface が変わったかを後続
（PB06 の常駐サーバー、PB07 の関数単位の増分コード生成、G12 の LSP）が同じ規則で判定できるようにする。
あわせて、規模に関する固定上限を実測に基づいて見直す（Phase 3）。

cache の有無で出力は 1 byte も変えない。特殊化、`closures::lower`、特殊化後の所有権検査、IR 生成などプログラム全体の段は
Phase 1 でも毎回すべて実行する。

実装者は Phase 1 だけを実装する。Phase 2・3 は人間が求め、該当する決定事項が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G11 と E03 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認:
  `grep -nE "^\| (G11|E03|G20) " _features/README.md`。
- G20 は Phase 1 の着手条件にしない（D1）。Phase 1 は `check_modules_collect` を変えない。Phase 2 は G20 が done であることを条件にする。
- 手順 8（`check`・`test`・`doc` への接続）は D5 の承認後に行う。手順 1〜7 と、承認前でも実行できる手順 9 以降の項目は先に進めてよい。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR と診断）を保存していること。
- PX01（`TSUZURI_TIME_PASSES`）と PX03（`benchmarks/run-build.mjs`）は条件にしない。done なら手順 11 で使い、未完なら手順 11 の
  代替手順（`/usr/bin/time -l`）で計測する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. 新しい crate（serde の derive、bincode など）か `unsafe` が必要になった。
2. cache の有無・cold と warm で、`--emit llvm` の IR、`--json` の診断（コード・メッセージ・位置・順序）、警告、終了コードのどれかが
   1 byte でも違う。
3. parse 直後の `Program` に、`Span::source` が `None` でも自分の source index でもない span がある（D3 の付け替えが成り立たない）。
4. encode → decode した `Program` の `{:?}` が parse 直後と一致しないソースが、std・`tests/fixtures`・`examples` にある。
5. 受理されるソースの AST の深さが decoder の上限（D4 の 512）を超える。上限を上げて通さない。
6. 関数・定数・extern の型が本体から推論される構文が見つかった（HEAD の `FunctionDecl::result` は `TypeExpr` で省略できない）。
   D6 の interface の定義が成り立たない。
7. 既存テストの期待値を変える必要がある。特に `src/main.rs` のテストの `assert!(parse(&["check", "Main.tz", "--no-cache"]).is_err())` と
   `tests/cache.mjs` の `check --no-cache` の `E2000` は D5 でも変えない。
8. `BuildCache::evict` が `frontend/` を消す、または G11 の entry・marker の扱いを変えないと共存できない。
9. 手順 11 の計測で、warm（cache あり）の解析が cache なしより遅い project がある。既定を変えずに数値を添えて報告する。
10. Windows での置き換え rename の扱いが、HEAD の `BuildCache::store` と同じ扱い（既存なら勝者を残して成功とみなす）を超える変更を要する。

## 現状（HEAD `f8dc655` で確認）

> 見直し（2026-10-08、HEAD `1182045`）: `SourceInput` は `namespace` を持ち、モジュールの名前（key）・名前空間・入口かどうかは構文解析の後に
> `program.namespace` から `module_identity` で決まる（D-35・D-37）。std は 37 ファイル（約 290 KB）で、そのうち `Arena`・`Regex`・`Unicode`・
> `Json`・`Cbor` は名前を書いたプログラムだけが読み込む（D-40、`stdlib::OPT_IN`・`sources_for`）ので、opt-in を使わないプログラムの std は 32 個。
> 構文木には名前空間・`using`・属性・固定長配列と長さ引数・`dyn`・補間・`try` などが増えた。G20（エラー回復）は done で、
> `analyze_inputs_indexed_all` の parse ループと診断の順序は変えない。言語サーバーの解析（`analyze_inputs_semantic`）は cache を使わない（D9）。
> 規模の数値（下の 2.12 s など）は当時のもので、2026-10-08 の計測は `docs/benchmarks.md` の「コンパイル時間と規模の上限（G17）」にある。

- 解析の入口は `src/lib.rs` の `analyze_inputs_indexed_all`。`SourceInput`（`path`・`text`・`origin`）を順に
  `parser::parse_with_source_all(input.text, id)` で parse し、拡張子から `program.source_kind` を設定する。`Diagnostics::is_full` で
  打ち切り、parse の診断が一つでもあれば検査へ進まない。その後 `check::check_modules_indexed_all` に
  `ModuleInput { name, program, origin }` を渡す。`analyze_inputs_all` と LSP（`src/lsp.rs` の再解析で
  `analyze_inputs_indexed_all(&inputs, Some(&mut semantic))`）がこの関数を使う。
- `Project::analyze_all`（`src/driver.rs`）は `SourceFile`（`path`・`relative_path`・`name`・`text`・`origin`・`package`）から
  `SourceInput` を作って `analyze_inputs_all` を呼ぶ。`src/main.rs` は check・build・run・test・doc のすべてで 1 か所だけ
  `project.analyze_all()` を呼ぶ（test は `Project::load_for_tests`、doc は `Project::load_for_docs` で読む）。
- 検査の `check_modules_collect`（`src/check.rs`）は全モジュールの宣言を一つの `function_declarations` に平らに並べ、その位置を
  関数 ID にする。モジュール名の検査（E1011・E1017）、`computation::collect_all`、定数と extern の関数化の後、最後に
  `ownership::infer_copy_all`、`polymorph::specialize`、`closures::lower`、`ownership::check_all`、`gpu::validate_calls` を全体に行う。
  警告は `(span.source, span.start)` で並べ替える。モジュール単位の検査の境界はない。
- `Span { start, end, source: Option<usize> }`（`src/diagnostic.rs`）。parse は source index（`inputs` の位置）を span に埋める。
  ファイルを足す・消すと、内容が同じファイルの index も変わる。
- `Program`（`src/syntax.rs`）は `#[derive(Debug)]` だけで、`ClassDecl`・`InstanceDecl` も Clone を持たない。`Cargo.toml` の依存は
  `rustc_apfloat`・`serde_json`（Windows だけ `same-file`）で、serde の derive はない。`unsafe_code = "forbid"`。
- `FunctionDecl` の `parameters: Vec<Parameter>`（`ty: TypeExpr`）と `result: TypeExpr` は必須で、関数の型は本体から推論しない。
- `src/formatter.rs` の `ast_fingerprint(program: Program) -> String` は `canonicalize` で span と文書 comment を消した正準形を返す
  （`fmt` の AST 保存の確認用）。`Program` を消費する。
- `src/cache.rs`: `default_root` は `TSUZURI_CACHE_DIR` が空でなければそれを、空なら `None` を返す（macOS の既定は
  `~/Library/Caches/tsuzuri/build-cache`）。`BuildCache::open` は marker `.tsuzuri-cache`（`MARKER`）を検証・作成し、Unix では `0o700`、
  上限は 2 GiB・30 日。`BuildCache::evict` は 64 桁 hex・`.lock-<key>`・`.tsuzuri-*` の名前だけを扱い、ほかの名前は飛ばす。
  `build_key` は `Sha256::field` で `format`・`compiler-binary`（`file_digest(current_exe)`）・options・IR・全ソースなどを hash する。
  `Sha256` は `new`・`update`・`field`・`hex` を持つ。`read_regular` は symlink と上限超えを拒否する（private）。
- G11 の cache は build と run だけで使う（`src/main.rs` の `"--no-cache is only valid with build or run"`）。build は解析と IR 生成の後に
  `build_key` を計算するので、whole-build cache に当たっても解析は毎回走る。cache を開けないと `build cache disabled: ...` を
  W2001 として出す（`src/driver.rs`）。
- 埋め込み std（`stdlib::SOURCES`）は 18 ファイル・50,853 bytes で、毎回 parse する。
- 規模（`_perfs/README.md`、2026-09-29）: 合成 1,000 モジュール（124,003 行）で `check` 2.12 s、IR 出力 2.58 s、compiler RSS 1.12 GB。
  フロントエンドの sample の 57% が malloc／free／memmove。parse と検査の内訳は未計測（PX01 の段階別時間で測る）。
- 上限: `MAX_SOURCE_BYTES`（1 MiB、E0003）と `MAX_NESTING`（128）は `src/syntax.rs`、`MAX_SPECIALIZATIONS`（1,024、E1017）は
  `src/polymorph.rs` と `src/call_specialization.rs`、`MAX_VALUE_BYTES`（64 KiB）は `src/check.rs`。モジュール名は 16 segment・255 bytes
  （`check_modules_collect` の E1017）。ソース数・package 数は E03・E04 の上限。
- LSP は変更ごとに全体を parse・検査する（G07）。旧 G17 の Phase 0（段階別時間）は PX01、合成プロジェクトの計測は PX03 が持つ。

### 再現（2026-09-29 に確認）

`check` は cache を開かず、build は marker と entry を一つ作る。release の compiler は 4,509,744 bytes。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
rm -rf /tmp/tz-work-G17 && mkdir -p /tmp/tz-work-G17
TSUZURI_CACHE_DIR=/tmp/tz-work-G17/cache target/release/tsuzuri check examples/hello
ls -A /tmp/tz-work-G17/cache        # No such file or directory
TSUZURI_CACHE_DIR=/tmp/tz-work-G17/cache target/release/tsuzuri build examples/hello --emit llvm -o /tmp/tz-work-G17/hello.ll
ls -A /tmp/tz-work-G17/cache        # .tsuzuri-cache と 64 桁 hex の entry 一つ
```

## 仕様

### Phase 分割

| Phase | 内容 | 状態 |
| --- | --- | --- |
| 1 | 構文木の codec、ディスクの parse cache、interface summary と hash、project manifest と `FrontendDelta`、build・run への接続（D5 の承認後は check・test・doc） | 実装対象 |
| 2 | 本体の検査結果の再利用。source と使う interface が変わらないモジュールの関数本体の型検査を省く | 設計方針。D10 の承認と G20 の完了が条件 |
| 3 | 上限の見直し（特殊化上限の二段化、ソース上限など） | 設計方針。D11 の承認が条件 |

旧 Phase 0（段階別時間・合成プロジェクト）は PX01・PX03 へ、旧 Phase 1（IR の分割と並列 Clang）は PB07 へ移した（D1、D12）。

### 前提とする他チケットのインターフェース

- G11（done）: `cache::default_root`、`BuildCache::open(root) -> io::Result<BuildCache>`、`Sha256::{new, update, field, hex}`、
  `read_regular(path, limit)`。G17 は `read_regular` を `pub(crate)` にするだけで、ほかは変えずに使う。
- PX01（todo、任意）: `TSUZURI_TIME_PASSES=1` で stderr の最後に `{"time_passes":1,` で始まる 1 行。手順 11 の計測だけで使い、G17 は段を足さない。
- PX03（todo、任意）: `node benchmarks/run-build.mjs <compiler> --out <dir> [--sizes 1000]` と「PB・G17 の before／after」の手順。
- G20（todo）: Phase 2 だけの条件。Phase 1 は検査の診断経路に触れない。

### 後続に提供するインターフェース

PB06・PB07・G12 は次の名前と規則だけを使い、同じ内容を別の形で再計算しない。Phase 1 で実装し、単体テストで固定する。

| 名前 | 形 | 保証 |
| --- | --- | --- |
| `frontend_cache::FRONTEND_FORMAT`（新規） | `u32`（初期値 1） | codec・entry・manifest・hash の規則を変えたら必ず上げる |
| `frontend_cache::compiler_identity()`（新規） | `Option<[u8; 32]>` | D2。同じ compiler の実行ファイルなら同じ値。得られなければ `None`（cache を使わない） |
| `frontend_cache::parse_key(identity, text)`（新規） | `[u8; 32]` | ソースの byte 列だけで決まる。path・拡張子・source index を含まない |
| `frontend_cache::interface_hash(identity, program, source)`（新規） | `[u8; 32]` | span・文書 comment・関数本体・test・入口式・instance method の本体に依存しない（D6） |
| `syntax_codec::encode`・`decode`（新規） | `Vec<u8>`・`Program` | `format!("{:?}", decode(encode(p, s, Full), s))` が `format!("{:?}", p)` と同一 |
| `FrontendCache::parse(text, source)`（新規） | `Result<Parsed, Vec<Diagnostic>>` | `Parsed::program` は `parser::parse_with_source_all(text, source)` と `{:?}` で同一。診断も同一 |
| `FrontendCache::finish(project_key, modules)`（新規） | `FrontendDelta` | D7 の比較規則 |
| `FrontendDelta::must_recheck(module)`（新規） | `bool` | D7 の再検査規則（Phase 1 は全 interface を依存とみなす） |

PB06 は `Program` を `(parse_key, source index)` の鍵でメモリに持ち、ディスクには同じ形式でだけ書く。PB07 は `FrontendDelta` と
manifest の interface hash を関数単位の鍵の入力に使う。G17 は関数単位の hash を提供しない。

> 見直し（2026-10-08）: パック（D4 の見直し）に合わせ、`FrontendCache::open(root, project)` が `ProjectKey { hash, kind }`（新規）を受け取って
> そのプロジェクトのパックを読み、`finish(modules)` は project を引数に取らない。`interface_hash` は `Result<[u8; 32], Invalid>`（符号化できない
> program は `Invalid`）。`analyze_inputs_with` の第 3 引数は `Option<&mut FrontendCache>`。PB06 はディスクに同じパックの形式で書く。

### CLI と環境変数

- 新しい option・環境変数は足さない。frontend cache は whole-build cache と同じ root（`TSUZURI_CACHE_DIR`）の下の `frontend/` に置く。
- build・run: `BuildOptions::cache` と同じ条件で使う。`--no-cache` は whole-build cache と frontend cache の両方の読み書きをやめる。
- check・test・doc: D5 の承認前は使わない。承認後は root が得られれば使う。`--no-cache` は今までどおり build・run だけで受ける
  （E2000 のまま）。無効にするには `TSUZURI_CACHE_DIR=`（空）にする。
- root が得られない・開けない・`frontend/` を作れない場合は、診断を出さずに cache なしで解析する（D8）。build・run の
  `build cache disabled: ...`（W2001）は今までどおり G11 だけが出す。

### 保存形式

```text
<root>/                          cache::default_root()
  .tsuzuri-cache                 G11 の marker（BuildCache::open が検証・作成）
  <64 桁 hex>/                   G11 の entry（変更なし）
  frontend/                      G17（新規）。BuildCache::evict は名前が合わないので扱わない
    p-<parse key の hex>.tzp     parse entry（ソース 1 個）
    m-<project key の hex>.json  project manifest（project root と種類ごとに 1 個）
    .tmp-<pid>-<counter>         書き込み途中。rename で置き換える
```

parse entry（整数は little endian）:

| 位置 | 大きさ | 内容 |
| --- | --- | --- |
| 0 | 8 | magic `b"TZFRONT\0"` |
| 8 | 4 | `FRONTEND_FORMAT` |
| 12 | 32 | parse key（ファイル名の hex と一致すること） |
| 44 | 32 | interface hash |
| 76 | 8 | payload の長さ `n` |
| 84 | n | `encode(program, source, Mode::Full)`（source index を含まない） |
| 84 + n | 32 | 位置 0 から 84 + n までの SHA-256 |

manifest は `serde_json` で読み書きする。`modules` は `name` の昇順で、std のモジュールも `"origin":"std"` で載せる。
`kind` は `program`（`Project::load`）・`tests`（`load_for_tests`）・`docs`（`load_for_docs`）。

```json
{"format":1,"compiler":"<identity hex>","kind":"program","modules":[{"name":"Main","path":"Main.tz","origin":"user","kind":"tz","source":"<parse key hex>","interface":"<interface hash hex>"}]}
```

### キーと hash

いずれも `Sha256::field(tag, bytes)` で作る。tag の文字列も形式の一部で、変えたら `FRONTEND_FORMAT` を上げる。

| 値 | field（この順） |
| --- | --- |
| `compiler_identity()` | `frontend-format`（`FRONTEND_FORMAT` の LE）、`compiler-version`（`CARGO_PKG_VERSION`）、`compiler-length`（`current_exe` の大きさ、LE u64）、`compiler-modified`（更新時刻の UNIX ns、LE u128） |
| `parse_key` | `identity`、`source`（ソースの byte 列） |
| `interface_hash` | `identity`、`interface`（`encode(program, source, Mode::Interface)`） |
| project key | `frontend-format`、`project-root`（`fs::canonicalize` した project root の byte 列）、`kind` |

### 無効化規則

| 変更 | parse entry | interface hash | `FrontendDelta` |
| --- | --- | --- | --- |
| 関数本体・comment・空白・文書 comment だけ | 新しい鍵（miss） | 変わらない | `changed_sources` にだけ入る |
| 宣言の頭部（引数・結果・制約・region・可視性・`export`・`rec`）、record・union・型別名・class（既定 method の本体を含む）・instance の頭部・定数（値を含む）・extern・active pattern、宣言の追加・削除・並べ替え | 新しい鍵 | 変わる | `changed_sources` と `changed_interfaces` |
| ファイル名の変更（モジュール名が変わる） | 内容が同じなら hit | 変わらない | 旧名が `removed`、新名が両方に入る |
| 拡張子の変更（`.tz` と `.tt`・`.tc`） | 内容が同じなら hit | 変わらない | manifest の `kind` が変わるので `changed_interfaces` |
| compiler の再 build・版・`FRONTEND_FORMAT` の変更 | 全部 miss | 全部変わる | `previous == false` |
| ファイルの追加・削除で source index がずれる | hit（index を付け替える） | 変わらない | 追加・削除したモジュールだけ |

再検査の規則（Phase 2 と PB06・PB07 が使う）: `must_recheck(m)` は `!previous`、`changed_interfaces` か `removed` が空でない、`m` が
`changed_sources` にある、のいずれかで true。qualified 名でどのモジュールからでも参照でき、instance は大域なので、Phase 1 は
全モジュールが全 interface に依存するとみなす。依存先の絞り込みは Phase 2（D10）。

### 診断

新しい診断はない。既存の診断の条件・メッセージ・位置・順序を変えない。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `W2001` | build・run で whole-build cache を開けない（今までどおり） | `build cache disabled: ...` | 位置なし |
| `E2000` | `check`・`test`・`doc` の `--no-cache`（今までどおり） | `--no-cache is only valid with build or run` | `<command line>` |

frontend cache の失敗（root・`frontend/` を開けない、entry の破損・形式違い・大きさ超え・symlink、書き込み失敗）は診断にせず miss とする。
parse に失敗したソースは保存しない。

### 資源上限

- parse entry 1 個 64 MiB（`MAX_ENTRY_BYTES`、新規）、manifest 16 MiB（`MAX_MANIFEST_BYTES`、新規）。超えたら保存せず、読むときは miss。
- decoder の入れ子の深さ 512（`MAX_DEPTH` = `4 * MAX_NESTING`、新規）。長さの接頭辞は残りの byte 数以下でなければ破損とする（先に確保しない）。
- `frontend/` の合計 1 GiB（`MAX_TOTAL_BYTES`、新規）、更新時刻から 30 日（`MAX_AGE_MS`、新規）。GC は 1 回に 4,096 件を調べ、最大 128 件を
  消す（`BuildCache::evict` と同じ数）。1 時間より古い `.tmp-*` も消す。

### 例

言語の構文は変えない。`benchmarks/run-cache.mjs` と同じ合成プロジェクト（`Module<i>.tz` が `def value :: i64` と `fn value = <i>`、
`Main.tz` が合計を返す）で次の結果になる。

| 操作 | parse entry | manifest の変化 | 出力 |
| --- | --- | --- | --- |
| 1 回目の build | 全ソース（利用者と std 18 個）が miss・保存 | `previous == false` | `--no-cache` の build と IR が同一 |
| 変更なしで 2 回目 | 全部 hit | 変化なし | 同一 |
| `Module3.tz` の `fn value = 3` を `fn value = 30` に | `Module3` だけ miss | `changed_sources == ["Module3"]`、`changed_interfaces` は空 | `--no-cache` の build と IR が同一 |
| `Module3.tz` の `def value :: i64` を `def value :: i32` に | `Module3` だけ miss | `changed_interfaces == ["Module3"]` | 型の診断が `--no-cache` と同一 |

## 設計

### データ構造

```rust
// src/syntax_codec.rs（新規）
pub(crate) const MAX_DEPTH: u32 = 4 * MAX_NESTING as u32;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode { Full, Interface }
pub(crate) struct Invalid; // 破損・未対応。呼び出し側は miss（保存しない）として扱う
pub(crate) struct Writer { bytes: Vec<u8>, mode: Mode, source: usize }
pub(crate) struct Reader<'a> { bytes: &'a [u8], at: usize, source: usize, depth: u32 }
pub(crate) trait Wire: Sized {
    fn put(&self, out: &mut Writer) -> Result<(), Invalid>;
    fn get(input: &mut Reader<'_>) -> Result<Self, Invalid>;
}
pub(crate) fn encode(program: &Program, source: usize, mode: Mode) -> Result<Vec<u8>, Invalid>;
pub(crate) fn decode(bytes: &[u8], source: usize) -> Result<Program, Invalid>;

// src/frontend_cache.rs（新規）
pub(crate) const FRONTEND_FORMAT: u32 = 1;
pub(crate) struct FrontendCache {
    directory: PathBuf, // <root>/frontend（canonicalize 済み）
    identity: [u8; 32],
    stored: usize,      // この process で保存した数。1 以上なら finish で evict
    pub(crate) stats: FrontendStats,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FrontendStats { pub(crate) hits: usize, pub(crate) misses: usize, pub(crate) rejected: usize }
pub(crate) struct Parsed { pub(crate) program: Program, pub(crate) key: [u8; 32], pub(crate) interface: [u8; 32] }
pub(crate) struct ModuleRecord<'a> {
    pub(crate) name: &'a str, pub(crate) path: &'a str, pub(crate) origin: ModuleOrigin,
    pub(crate) kind: Option<SourceKind>, pub(crate) source: [u8; 32], pub(crate) interface: [u8; 32],
}
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FrontendDelta {
    pub(crate) previous: bool,
    pub(crate) changed_sources: Vec<String>,    // 名前の昇順
    pub(crate) changed_interfaces: Vec<String>, // 名前の昇順
    pub(crate) removed: Vec<String>,            // 名前の昇順
}

// src/driver.rs（新規の公開 API。src/main.rs は別 crate なので pub(crate) を呼べない）
pub enum AnalysisKind { Program, Tests, Docs }
impl Project {
    pub fn analyze_cached(&self, cache: bool, kind: AnalysisKind) -> Result<CheckedModule, DiagnosticSet>;
}
```

codec の規則: enum の tag は宣言順の番号の `u8`、整数は LEB128 の `u64`（`usize` へ変換できなければ `Invalid`）、`bool` は 0・1 だけ、
`String` は長さと UTF-8（`String::from_utf8` で検証）、`Vec`・`Box<[T]>` は長さと要素、`Option` は 0・1 と値。`Span` は `Mode::Full` で
`start`・`end`・source の有無（0 は `None`、1 は自分の source）を書き、`get` は 1 を `Some(reader.source)` に戻す。自分以外の source を
持つ span は `Invalid`。`Mode::Interface` は `Span`、`Option<Documentation>`、`FunctionDecl::body`、`Program::tests`、`Program::entry`、
`InstanceDecl::methods` の各 `Definition::body` を書かない。ほかはすべて source の順に書く。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 構文 | `src/syntax.rs` | `Program` から届くすべての型（`TypeExpr`・`TypeExprKind`・`Expr`・`ExprKind`・`PatternKind`・各 `*Decl`・`Ident`・`Provenance`・`StringLiteral`・`Kind`・`DeriveClass`・`SourceKind` など） | 変更なし。derive も足さない |
| 直列化 | `src/syntax_codec.rs`（新規） | 上の各型の `Wire`、`encode`、`decode` | 上の規則。`match` に `_ =>` を使わず全 variant を書き、variant を足すと compile error になるようにする |
| cache | `src/frontend_cache.rs`（新規） | `compiler_identity`、`parse_key`、`interface_hash`、`FrontendCache::open`・`parse`・`finish`・`evict`、`compare`（新規） | 「アルゴリズム」のとおり |
| cache | `src/cache.rs` | `read_regular` | `pub(crate)` にする。ほかは変えない |
| 公開 | `src/lib.rs` | module 宣言 | `pub(crate) mod syntax_codec;`・`pub(crate) mod frontend_cache;` |
| 解析 | `src/lib.rs` | `analyze_inputs_indexed_all` | 本体を `analyze_inputs_with(inputs, semantic, cache: Option<(&mut FrontendCache, [u8; 32])>)`（新規、第 2 要素は project key）へ移し、`parser::parse_with_source_all` の呼び出しだけを `cache.parse` に替える（cache なしは今と同じ呼び出し）。`source_kind` の設定、打ち切り、エラーの順は変えない。parse がすべて成功したら `ModuleRecord` を作り、検査の前に `finish` を呼ぶ |
| driver | `src/driver.rs` | `AnalysisKind`・`Project::analyze_cached`（新規） | `analyze_all` と同じ `SourceInput` を作る。`cache` が true で `default_root()` と `FrontendCache::open` が成功すれば cache 付きで、そうでなければ `analyze_all` と同じ経路で解析する。project key は `self.sources[self.root]` の親ディレクトリから作る |
| CLI | `src/main.rs` | `project.analyze_all()` の呼び出し | `project.analyze_cached(<build・run で !no_cache>, <action から AnalysisKind>)` に替える。手順 8 で check・test・doc も true にする（D5） |
| 整形 | `src/formatter.rs` | `ast_fingerprint` | 変更なし（テストの独立した参照） |
| LSP | `src/lsp.rs` | 再解析 | 変更なし（D9） |
| 検査 | `src/check.rs` | `check_modules_collect` | Phase 1 は変更なし |

### アルゴリズム

```text
FrontendCache::open(root):
  BuildCache::open(root) が失敗 → None
  dir = root/frontend。create_dir_all。symlink_metadata が実ディレクトリでなければ None。Unix は 0o700
  Some(FrontendCache { directory: canonicalize(dir), identity: compiler_identity(), .. })

FrontendCache::parse(text, source):
  key = parse_key(identity, text); path = directory/"p-{hex(key)}.tzp"
  read_regular(path, MAX_ENTRY_BYTES) が読め、magic・format・key・長さ・末尾の SHA-256 が合い、decode(payload, source) が成功
    → hits += 1、Parsed を返す
  読めたが不正 → rejected += 1
  misses += 1
  program = parser::parse_with_source_all(text, source)?      // 診断は保存しない
  full = encode(program, source, Full); face = encode(program, source, Interface)
  どちらかが Invalid → Parsed { interface: key, .. } を返して保存しない（source が変われば interface も変わる保守的な扱い）
  interface = interface_hash の規則で face から計算
  directory/".tmp-{pid}-{counter}" へ書いて path へ rename。失敗しても Parsed は返す。成功したら stored += 1

FrontendCache::finish(project_key, records):
  current = manifest(records)
  previous = read_regular(m-{hex}.json, MAX_MANIFEST_BYTES)。format と compiler が合うときだけ使う
  delta = compare(previous, current)
  前回と byte 列が違えば一時名へ書いて rename
  stored > 0 なら evict()（失敗は無視）
  delta

compare(previous, current):
  previous なし → FrontendDelta { previous: false, changed_sources: 全名, changed_interfaces: 全名, removed: [] }
  各 current の m: 前回になし → 両方へ。source が違う → changed_sources。(origin, kind, interface) が違う → changed_interfaces
  前回だけにある名前 → removed
```

一時名は `TemporaryDirectory::new`（`src/driver.rs`）と同じ pid と counter の方式にし、`src/frontend_cache.rs` に専用の `AtomicU64` を置く。
rename は置き換えである。Windows で宛先が既にあって失敗した場合は、内容で鍵付けしているので成功とみなして一時ファイルを消す。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、変更前の compiler と IR を保存する。
- 確認: 次がすべて成功し、`/tmp/tz-g17/before/` に `.ll` が 4 個できる。stack-depth のテストは `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-g17/before && cp target/release/tsuzuri /tmp/tz-g17/baseline-tsuzuri
for p in examples/hello tests/fixtures/hierarchical; do for o in -O0 -O3; do
  target/release/tsuzuri build $p --emit llvm $o --no-cache -o /tmp/tz-g17/before/$(basename $p)$o.ll
done; done
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
```

### 手順 2: codec の土台

- 変更: `src/syntax_codec.rs`（新規）、`src/lib.rs`（`pub(crate) mod syntax_codec;`）。
- 内容: `Writer`・`Reader`・`Invalid`・`Wire`、LEB128、`bool`・`String`・`Vec`・`Box<[T]>`・`Option`、`Span`・`Ident`・`Provenance`・
  `TypeExpr`・`TypeExprKind`・`Kind`。再帰する型の `get` は入口で `depth` を増やし、出口で必ず減らす（`MAX_DEPTH` 超えは `Invalid`）。
- 確認: `cargo test --locked --lib syntax_codec` が `running 2 tests` で成功（「Rust テスト」の `round_trips_primitives` と
  `round_trips_spans_into_another_source`）。

### 手順 3: codec の完成と round-trip

- 変更: `src/syntax_codec.rs`。
- 内容: `Program` から届く残りの型と `encode`・`decode`。`match` に `_ =>` を書かない。
- 確認: `cargo test --locked --lib syntax_codec` で round-trip・破損・深さ・golden のテストが成功する。停止条件 3〜5 に当たらない。

### 手順 4: interface の mode と hash

- 変更: `src/syntax_codec.rs`（`Mode::Interface`）、`src/frontend_cache.rs`（新規。`FRONTEND_FORMAT`・`compiler_identity`・`parse_key`・
  `interface_hash`）、`src/lib.rs`（`pub(crate) mod frontend_cache;`）。
- 内容: D6 の省略規則。`compiler_identity` は `current_exe` か更新時刻を得られなければ `None` を返し、呼び出し側は cache を使わない。
- 確認: `cargo test --locked --lib interface_` が `running 2 tests` で成功。

### 手順 5: parse entry の保存と読み込み

- 変更: `src/frontend_cache.rs`（`FrontendCache::open`・`parse`）、`src/cache.rs`（`read_regular` を `pub(crate)`）。
- 内容: 「アルゴリズム」の `open` と `parse`。テストは `std::env::temp_dir()` の下に pid と counter の名前の root を作り、`default_root` を使わない。
- 確認: `cargo test --locked --lib frontend_cache` が成功し、`stores_then_hits` など手順 5 の 6 テストを含む。

### 手順 6: manifest・`FrontendDelta`・GC

- 変更: `src/frontend_cache.rs`（`finish`・`compare`・`FrontendDelta::must_recheck`・`evict`）。
- 内容: GC の本体は `evict_with(max_total, max_age_ms, now_ms)`（新規、private）にし、テストは小さい値で呼ぶ。
- 確認: `cargo test --locked --lib frontend_cache` が手順 6 の 3 テストを加えて成功。

### 手順 7: 解析と build・run への接続

- 変更: `src/lib.rs`（`analyze_inputs_with`）、`src/driver.rs`（`AnalysisKind`・`Project::analyze_cached`）、`src/main.rs`（`analyze_all` の呼び出し）。
- 内容: 段ごとの変更の表のとおり。`analyze_inputs_all`・`analyze_inputs_indexed_all` は `analyze_inputs_with(.., None)` を呼ぶだけにする。
- 確認: `cargo test --locked` が成功。`cargo build --release --locked` の後、次の `cmp` が何も出さず、cache に `frontend/` ができる。
  `node tests/cache.mjs target/release/tsuzuri` が成功する。

```sh
export TSUZURI_CACHE_DIR=/tmp/tz-g17/cache
for run in cold warm; do for o in -O0 -O3; do
  target/release/tsuzuri build tests/fixtures/hierarchical --emit llvm $o -o /tmp/tz-g17/$run$o.ll
  cmp /tmp/tz-g17/before/hierarchical$o.ll /tmp/tz-g17/$run$o.ll
done; done
ls /tmp/tz-g17/cache/frontend | grep -c '^p-'    # 利用者のソース数 + 18
unset TSUZURI_CACHE_DIR
```

### 手順 8: check・test・doc への接続（D5 の承認後）

- 変更: `src/main.rs`。
- 内容: `analyze_cached` の第 1 引数を、すべての action で `BuildOptions::cache` の値にする（check・test・doc では `--no-cache` を受けないので true）。
- 確認: `cargo test --locked` と `node tests/cache.mjs target/release/tsuzuri`（`check --no-cache` の E2000 を含む）が成功。手順 1 の
  「再現」を実行すると、`check` の後に `frontend/` ができている。

### 手順 9: E2E `tests/frontend_cache.mjs`

- 変更: `tests/frontend_cache.mjs`（新規）。`tests/cache.mjs` の `cli`・`concurrent` の形を写し、`mkdtempSync` の一時 root と
  `TSUZURI_CACHE_DIR` で隔離する。
- 内容: 「E2E」の 1〜9。D5 の承認前は 10 を skip し、その旨を 1 行出す。
- 確認: `node tests/frontend_cache.mjs target/release/tsuzuri` が終了コード 0 で `frontend cache e2e: ok` を出す。

### 手順 10: 診断の順序の回帰（リスクの確認）

- 変更: `tests/frontend_cache.mjs` の E2E 8。
- 内容: `analyze_inputs_with` は parse の順・`is_full` の打ち切り・エラーの連結を変えないはずである。これを複数ファイルの parse エラーと
  複数モジュールの型エラー・警告で確かめる。
- 確認: 手順 9 と同じコマンドで成功する。違いが出たら停止条件 2。

### 手順 11: 計測

- 変更: `docs/benchmarks.md`。
- 内容: PX03 が done なら、その「PB・G17 の before／after」に従い `--sizes 1000` を測る。未完なら次の代替手順で、
  1,000 モジュールの project（`benchmarks/run-cache.mjs` と同じ生成規則）を基準の compiler の warm（G11 だけ）、新しい compiler の cold、
  warm の 3 系列で 9 回ずつ測り、中央値・最小・最大を記録する。速度の合否条件は作らない。
- 確認: 3 系列の生データが `target/perf/G17/` にあり、`docs/benchmarks.md` に表がある。warm が基準より遅ければ停止条件 9。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p target/perf/G17 && P=/tmp/tz-g17/p1000 && export TSUZURI_CACHE_DIR=/tmp/tz-g17/bench-cache
node -e 'const fs=require("fs"),d=process.argv[1];fs.mkdirSync(d,{recursive:true});const m=["let mut total = 0"];for(let i=0;i<1000;i++){fs.writeFileSync(`${d}/Module${i}.tz`,`def value :: i64\nfn value = ${i}\n`);m.push(`total = total + Module${i}.value()`)}m.push("total");fs.writeFileSync(`${d}/Main.tz`,m.join("\n")+"\n")' $P
/tmp/tz-g17/baseline-tsuzuri build $P --emit llvm -O0 -o /tmp/tz-g17/x.ll && target/release/tsuzuri build $P --emit llvm -O0 -o /tmp/tz-g17/x.ll
for i in 1 2 3 4 5 6 7 8 9; do
  /usr/bin/time -l /tmp/tz-g17/baseline-tsuzuri build $P --emit llvm -O0 -o /tmp/tz-g17/x.ll 2>> target/perf/G17/base-warm.txt
  rm -rf $TSUZURI_CACHE_DIR/frontend
  /usr/bin/time -l target/release/tsuzuri build $P --emit llvm -O0 -o /tmp/tz-g17/x.ll 2>> target/perf/G17/new-cold.txt
  /usr/bin/time -l target/release/tsuzuri build $P --emit llvm -O0 -o /tmp/tz-g17/x.ll 2>> target/perf/G17/new-warm.txt
done
unset TSUZURI_CACHE_DIR
```

### 手順 12: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/build-and-cache.md _docs/tools/command-line.md _docs/feature-status.md` が成功。

### 手順 13: 最終確認

- 確認: 次がすべて成功し、手順 1 の stack-depth テストと `honors_the_exact_specialization_limit` も成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo fmt --check && cargo test --locked && cargo build --release --locked
node tests/cache.mjs target/release/tsuzuri
node tests/frontend_cache.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
node tests/lsp_sessions.mjs target/release/tsuzuri
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
```

## テスト計画

### Rust テスト

`src/syntax_codec.rs` の `#[cfg(test)] mod tests`:

- `round_trips_primitives`: LEB128 の 0・127・128・`u64::MAX`、空と非 ASCII の `String`、`Option`・`bool`。`bool` の 2 と長さ超えの `Vec` は `Invalid`。
- `round_trips_spans_into_another_source`: source 3 で encode し、source 3 と 5 で decode する。`Span::source` がそれぞれ `Some(3)`・`Some(5)`、
  `None` は `None` のまま。
- `round_trips_every_std_source`: `stdlib::SOURCES` の全ソースで、`parse_with_source_all(text, 7)` の `{:?}` と decode 結果の `{:?}` が一致。
- `round_trips_fixture_and_example_sources`: `tests/fixtures` と `examples` の `.tz`・`.tt`・`.tc` を再帰的に読み、parse できるものすべてで同じ比較。
  件数が 0 でないことも assert する。
- `round_trips_deepest_accepted_nesting`: `parser::tests::bounds_recursive_and_flat_expression_depth` が受理する最深の入力で round-trip する。
- `rejects_every_truncation` と `rejects_flipped_bytes`: 小さい program の encode 結果の全接頭辞と各 byte の反転で decode が panic せず、
  接頭辞は必ず `Invalid`。
- `rejects_excessive_depth`: 手で作った 600 段の `TypeExprKind::Array` は `Invalid`。
- `rejects_foreign_source_spans`: source 2 の span を持つ program を source 1 で encode すると `Invalid`。
- `pins_encoding_format`: 固定の小さい program の encode 結果が golden の hex と一致する。失敗したら `FRONTEND_FORMAT` を上げて golden を更新する旨の
  comment を 1 行置く。
- `interface_ignores_bodies_docs_and_positions`: 関数本体、文書 comment、先頭への空行の追加、test、入口式、instance method の本体だけを変えた 6 組で
  `interface_hash` が同じ。
- `interface_changes_with_headers`: 引数の型、結果の型、可視性、record の field、union の case、class の既定 method の本体、定数の値、
  宣言の順の 8 組で `interface_hash` が違う。

`src/frontend_cache.rs` の `#[cfg(test)] mod tests`（一時 root を使う）:

- `stores_then_hits`: 1 回目 `misses == 1`、2 回目 `hits == 1`、両方の `Program` の `{:?}` が同じ。
- `corrupt_entries_are_misses_and_replaced`: 末尾の 1 byte を反転すると `rejected == 1` で parse し直し、次は hit。
- `parse_errors_are_not_stored`: parse エラーのソースは診断が `parse_with_source_all` と同じで、`p-*.tzp` ができない。
- `other_format_or_identity_is_a_miss`: header の format を 2 にした entry と、別の identity の entry は miss。
- `symlinked_entry_is_a_miss`（Unix だけ）: `p-<key>.tzp` が symlink なら読まずに miss。
- `shares_root_with_build_cache`: `BuildCache::open` と `FrontendCache::open` を同じ root で開き、`BuildCache::evict` の後も `frontend/` が残る。
- `delta_classifies_changes`: 「無効化規則」の 6 行を manifest の組で再現し、`FrontendDelta` の 4 欄を assert する。
- `must_recheck_follows_phase_one_rule`: body だけの変更では変えたモジュールだけ true、interface の変更・削除・前回なしでは全部 true。
- `evicts_oldest_over_budget_and_stale_temporaries`: 小さい上限で古い順に消え、1 時間より古い `.tmp-*` が消え、新しい `.tmp-*` は残る。

### E2E

`tests/frontend_cache.mjs`（新規）。参照は同じ compiler の `--no-cache` の出力で、frontend cache の経路から独立している。合成 project は
`benchmarks/run-cache.mjs` と同じ生成規則の 20 モジュールで、実行結果は独立に計算した `20 * 19 / 2 = 190`。

1. IR の同一: native と `--target wasm32` のそれぞれ `-O0`・`-O3` で、`--no-cache`・cold・warm の `--emit llvm` が byte 単位で同一。
2. 実行: `run` の標準出力が `190`、cold と warm で同じ。
3. entry: cold の後の `p-*.tzp` の数が利用者 21 + std 18、`m-*.json` が 1 個。warm では `p-*.tzp` の数と更新時刻が変わらない。
4. 本体だけの変更: `Module3.tz` の値を変えると、manifest の `Module3` の `source` だけが変わり `interface` は同じ。IR は変更後の `--no-cache` と同一、
   実行結果は `190 - 3 + 30 = 217`。
5. 頭部の変更: `def value :: i64` を `i32` にすると `interface` が変わり、診断の JSON が `--no-cache` と同一。
6. 破損: 全 `p-*.tzp` の末尾を反転、長さ 0 に切り詰め、symlink に置き換えの 3 通りで build が成功し IR が同一。次の実行で entry が正しく書き直される。
7. 同時実行: 空の cache で 6 個の build を同時に起動し、すべて成功・IR 同一・`.tmp-*` が残らない。
8. 診断の順序（リスク）: 2 ファイルの parse エラーの project と、3 モジュールの型エラーと W1001 を持つ project で、`--json` の診断列・終了コードが
   `--no-cache`・cold・warm で同一。
9. 無効化: `TSUZURI_CACHE_DIR=`（空）で build が今までどおり W2001 を出し、`frontend/` を作らない。無関係な空でないディレクトリを指すと
   G11 の `cache disabled` だけが出る。
10. D5 の承認後: `check`・`test` の cold と warm で stdout・stderr・終了コードが同一。`check --no-cache` は E2000 のまま。

### 既存テストへの影響

なし。`tests/cache.mjs` の `entries()` は 64 桁 hex の名前だけを数えるので、`frontend/` は数に入らない。`src/main.rs` の `--no-cache` の
parse テストも変えない。

### 性能

手順 11 のとおり。速度の合否条件は CI に入れない。std だけの小さい project（`examples/hello`）は parse の割合が小さく、改善を主張しない。

## ドキュメント

- `docs/architecture.md`: 「Whole-build cache」の段落の「parse/check/IRのcacheは作りません」を更新し、「Frontend cache」の段落（保存形式、鍵、
  無効化規則、失敗は miss、出力は同一、GC）を足す。
- `README.md`: 「build/runの成果物cacheは既定で有効です…check/headerは対象外です」の文を D5 の結果に合わせて直し、テストの一覧に
  `node tests/frontend_cache.mjs target/release/tsuzuri` を足す。
- `_docs/tools/build-and-cache.md`: `## キャッシュの対象`、`## 保存先`（`frontend/`）、`## キーと検証`、`## 障害と容量管理`。
- `_docs/tools/command-line.md`: `## ツールと環境変数` の `TSUZURI_CACHE_DIR` の行に frontend cache を足す。
- `docs/benchmarks.md`: 手順 11 の結果（PX03 の `## ビルド速度` があればその中、なければ G17 の節）。
- `_docs/feature-status.md` と `_features/README.md` の状態欄（GUIDE §10）。

## 受け入れ条件

- [ ] 「Rust テスト」の全テストと E2E 1〜9（D5 の承認後は 10 も）が成功する。
- [ ] cache なし・cold・warm で IR（native・wasm32 × `-O0`・`-O3`）、診断、終了コードが byte 単位で同一。
- [ ] std・fixtures・examples の全ソースで round-trip が一致し、破損した entry で panic しない。
- [ ] 後続に提供するインターフェースの表の名前が実装され、`FRONTEND_FORMAT` と golden のテストがある。
- [ ] 新しい crate・`unsafe`・診断・CLI option がなく、既存テストの期待値を変えていない。
- [ ] 手順 11 の 3 系列の計測が `docs/benchmarks.md` にある。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- span は source index を持つ。index を保存すると、ファイルを足した後の hit で別ファイルの位置を指す診断になる。`Span` の `put` は index を書かず、
  `round_trips_spans_into_another_source` で確かめる。
- interface に span を入れると、上の関数の本体を 1 行変えただけで下の全宣言の interface が変わる。`Mode::Interface` は span を書かない。
- codec の `match` に `_ =>` を置くと、variant の追加が黙って壊れる。全 variant を書き、`pins_encoding_format` の失敗で `FRONTEND_FORMAT` を上げる。
- 破損した長さの接頭辞で `Vec::with_capacity(len)` すると巨大な確保で落ちる。残りの byte 数と比べてから読む。
- decoder の再帰は破損した入力で深くなる。`depth` の増減を全経路で対にし、debug build（2 MiB の stack）の round-trip テストで確かめる。
- `Program` に Clone を足して回避しない。`FrontendCache::parse` は所有する `Program` を返す。
- テストから利用者の cache を汚さない。単体テストは root を引数で渡し、子プロセスには `.env()` で `TSUZURI_CACHE_DIR` を渡す。
  `std::env::set_var` は使わない（edition 2024 では `unsafe`）。
- compiler を build し直すと更新時刻が変わり、すべて cold になる（D2 の意図どおり）。計測の warm-up を忘れない。
- manifest は検査の前に書くので、型エラーの build でも更新される（interface は parse だけで決まる）。
- 同時実行で同じ entry を書いても内容は同じ。rename の失敗を error にしない。
- `analyze_inputs_with` の parse ループで早期 return を足すと、診断の順序と `is_full` の打ち切りが変わる。今のループの形を変えない。

## 対象外

- Phase 2（本体の検査結果の再利用）と Phase 3（上限の変更）。人間が求め、D10・D11 が承認された場合だけ行う。
- IR の分割・並列 Clang・`-O3` の分割（PB07、D12）、常駐サーバー・メモリ上の cache・watch（PB06）、LSP での利用（G12・PB06）。
- 段階別時間と合成プロジェクトの計測基盤（PX01・PX03）、フロントエンドの並列化（PB04）。
- 型付き IR・LLVM IR・object の cache、共有・遠隔 cache、分散ビルド。

## 決定事項

### D1: 分担と Phase

- 決定: G17 はディスクのモジュール単位の frontend cache（parse 結果・interface summary・manifest・`FrontendDelta`）を持つ。PB06 はそれをメモリに
  持つ常駐サーバーと watch、PB07 は関数単位の増分コード生成とリンク、旧 Phase 1 の IR 分割と並列 Clang を持つ。旧 Phase 0 は PX01・PX03 が持つ。
  G20 は Phase 1 の着手条件にしない。
- 理由: 調整役の分担の決定。Phase 1 は検査の内部に触れないので G20 と独立に出荷できる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し提案: 分担では「依存するモジュールは使う interface が変わったときだけ再検査する」を G17 に置いた。`check_modules_collect` は全モジュールの
  関数を一つの配列の位置で番号付けするため、検査結果の再利用はモジュール局所の ID を要する大きな変更になる。Phase 1 は規則と `FrontendDelta`
  までとし、再利用は Phase 2（D10）に分けた。

### D2: compiler の同一性

- 決定: `compiler_identity` は `FRONTEND_FORMAT`・`CARGO_PKG_VERSION`・`current_exe` の大きさと更新時刻（ns）から作る。得られなければ cache を使わない。
- 理由: G11 の `file_digest(current_exe)` は 4.5 MB の全体を毎回 hash し、0.05 s の `check` に対して無視できない。再 build は更新時刻を変え、
  entry は末尾の SHA-256 と decoder の検証で守られる。
- 状態: 既定案（実装者はこの案に従う）

### D3: 構文木の直列化

- 決定: `src/syntax_codec.rs` に手書きの codec を置き、crate を足さない。source index は保存せず decode で付け直す。`{:?}` の一致を round-trip の基準にする。
- 理由: serde の derive は新しい crate で要承認になる。`Program` は `Debug` を持ち、`{:?}` は span を含む全 field を出すので独立した比較になる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: 規則どおり実装し、現在の構文型に合わせて次を決めた。`ExprKind::Integer` の `u128` は 128 bit までの LEB128、`char` は
  `u32` で書き `char::from_u32` で検証する。`ComputationStatementKind::Operation` の `&'static str` は builder の操作名の固定表の番号。
  tag を読む `match` は `_ =>` の代わりに `N..=u8::MAX => Err(Invalid)` と書き、variant の数を超える tag を拒否する。`MAX_DEPTH` は式・パターン・型・
  計算式のブロック・kind の入れ子を数え、encode も同じ深さで打ち切る（復号できない entry を保存しない）。parser が作らない variant（`DynDispatch`、
  `QualifiedFunction`、`ComputationBoundary`、`ShiftRightUnsigned`、`ComputationDestructuring`、`Provenance::Generated`）を含め、全 variant が往復することを
  `round_trips_every_variant`（言語リファレンスの例、std、fixtures、examples と手で作った program）で確かめる。

### D4: 保存場所・形式・版・GC

- 決定: 「保存形式」「資源上限」のとおり。`<root>/frontend/` に置き、`BuildCache::open` で root を検証する。版は `FRONTEND_FORMAT` を header と鍵に入れる。
  GC は mtime で数え、hit でも mtime を更新しない（30 日ごとに一度 parse し直すだけ）。
- 理由: G11 の marker・権限・symlink 拒否をそのまま使え、`BuildCache::evict` は `frontend` という名前を扱わない。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08、停止条件 9 への対応）: ソース 1 個ごとの entry ファイル（`p-<parse key>.tzp`）で実装して測ると、warm の `check` が cache なしより遅かった
  （現実的な 1,000 モジュールで中央値 3.86 s 対 2.44 s）。`sample` では時間の大半が `open` で、この機械ではコンパイラのプロセスがファイルを 1 個開くのに
  0.3〜1.5 ms かかる（on-access scan。同じ 1,034 ファイルを Node は 35 ms、新しく build した C の実行ファイルは 240〜380 ms で読む）。モジュールの構文解析
  （1 個 0.2 ms）より開く費用が大きいので、プロジェクトのルートと種類ごとに 1 個のパック `p-<project key>.tzp` に entry をまとめた。パックの header は
  magic `b"TZPACK\0\0"`・`FRONTEND_FORMAT`・compiler の同一性（32）・project key（32）・entry 数（u64）の 84 byte で、entry は「保存形式」の表の形のまま
  （magic・形式・parse key・interface hash・長さ・payload・SHA-256）並ぶ。warm の解析が読むのはパックと manifest の 2 ファイルになる。パックは今回の解析で
  使った entry だけを入力順に持ち（同じ内容のソースは 1 個）、前回のパックと同じなら書かない。ファイルを開く費用のない環境でも、ソースごとのファイルより
  読み書きが少ない。代わりに、別のプロジェクトや別の種類（`check` と `test`）の間で entry は共有しない（std の entry はプロジェクトごとに約 230 KB 重複する）。
  パックの上限は 512 MiB（`MAX_PACK_BYTES`、新規）。Windows で rename が置き換えに失敗した場合は、パックは内容で鍵付けしていないので成功とみなさず、
  前回のパックを残して次の実行で書き直す。GC の対象は `p-<64 桁>.tzp`・`m-<64 桁>.json`・`.tmp-*` で、規則と数は変えない。

### D5: check・test・doc での既定の利用

- 決定: 手順 8 で check・test・doc も既定で frontend cache を読み書きする。`--no-cache` は build・run だけのまま。無効化は `TSUZURI_CACHE_DIR=`。
- 理由: 解析だけの `check` が最も恩恵を受ける。ただし check はこれまでディスクに書かず（`README.md` の「check/headerは対象外」）、利用者から見える
  挙動が変わる。承認されない場合は build・run だけで使う。
- 状態: 承認済み（2026-10-08、D-41）。手順 8 を実施した。`check`・`test`・`doc` は既定で読み書きし、`--no-cache` は今までどおり build・run だけ
  （それ以外は `E2000`）、`TSUZURI_CACHE_DIR=`（空）で無効になる。`publish` の検査（`Project::load_for_tests` と `analyze_all`）は対象外のまま cache を使わない。

### D6: interface summary の中身

- 決定: `Mode::Interface` の encode 結果を interface とする。span・文書 comment・関数本体・test・入口式・instance method の本体を除き、ほか
  （頭部、record・union・型別名、class の既定 method の本体、定数の値、extern、active pattern、宣言の順）は含める。
- 理由: 関数の型は注釈で決まり本体から推論しない。既定 method と定数の値は他モジュールの検査に効き得るので保守的に含める。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: `Program` は宣言を種類ごとの列に持つので、「宣言の順」は同じ種類の中の順である（record と union の入れ替えは `Program` に
  現れず、interface も変わらない）。`dyn_types`（本体に書いた `dyn` 型も含む）・`cpu_attributes`・名前空間・`using` も interface に入る。

### D7: 無効化規則と `FrontendDelta`

- 決定: 「無効化規則」の表と `must_recheck` の規則。Phase 1 は全モジュールが全 interface に依存するとみなす。
- 理由: qualified 名はどのモジュールも参照でき、instance は大域である。絞り込みは誤ると誤った再利用になるので、使用関係を記録する Phase 2 まで行わない。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: manifest の `name` は `module_identity` が返すモジュールの key で、各モジュールに `namespace` と `entry`（`Main` のファイルか）を
  足した。名前空間は `Tsuzuri.toml` の変更だけでも変わり、`entry` は宣言した名前空間のファイルを移すと名前を変えずに変わるので、どちらも
  `changed_interfaces` の比較（`origin`・`kind`・`namespace`・`entry`・`interface`）に入れた。パスだけの変更は検査に効かないので変更として数えない
  （拡張子の変更は表のとおり `kind` が変わるので `changed_interfaces` だけに入る）。manifest の byte 列が前回と同じときは解析せずに「変化なし」とする。
  `FrontendDelta::must_recheck` は Phase 2 を見送った（D10）ので、本体では使わない（PB06・PB07 が使う規則として単体テストで固定した）。

### D8: 失敗の扱い

- 決定: frontend cache の失敗はすべて miss で、診断・警告を出さない。W2001 は G11 の経路だけが出す。
- 理由: 最適化の失敗で `check` の出力を変えない。build・run の root の失敗は G11 が既に知らせる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し（2026-10-08）: 構文解析が一つでも失敗した解析ではパックも manifest も書かない（成功したソースの新しい entry も次の成功まで保存しない）。

### D9: LSP と常駐

- 決定: Phase 1 は `src/lsp.rs` を変えない。編集中の overlay をディスクへ書かない。メモリの層は PB06 が同じ鍵で作る。
- 理由: 打鍵ごとのディスク書き込みは無駄で、LSP の索引（`SemanticIndex`）の扱いは G12・PB06 の範囲である。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2（本体の検査結果の再利用）

- 決定: 方針だけ決める。モジュール局所の関数 ID、型付き本体と警告・semantic 索引のモジュール単位の保存、`must_recheck` が false のモジュールの本体検査の
  省略、診断を `(source, start)` で並べる現在の順序の維持、使用関係による依存の絞り込み。E2E 8 の比較を全 suite の診断に広げて確かめる。
- 理由: 検査の構造と診断の順序を変える変更で、G20 の回復の方式と衝突し得る。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D11: Phase 3（上限の新しい値）

- 決定: 上限と `E1017` は維持する。新しい値（特殊化上限を全体の上限と型が成長する多相再帰の検出の二段にする、ソース上限の引き上げなど）は PX03 の計測の後に人間が判断する。
- 理由: 上限は資源と診断の約束で、計測なしに変えない。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D12: `-O3` の分割と並列コード生成

- 決定: IR の分割と並列 Clang は PB07 へ移す。`-O3` で ThinLTO を使うか一つのモジュールのままにするかは、PB07 で実測して決める。
- 理由: 関数単位の増分コード生成と同じ分割の単位を共有する。旧案の「実測で決める」は変えない。
- 状態: 既定案（実装者はこの案に従う）
