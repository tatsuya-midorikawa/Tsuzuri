# PB07: 関数単位の増分コード生成と増分リンク

| 項目 | 内容 |
| --- | --- |
| ID | PB07 |
| 分類 | ビルド速度 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | PB06, G17 |
| 関連 | PB05, G11 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（`-O0` の native 実行ファイルの既定経路を codegen unit に分ける。成果物の byte 列が変わり、利用者の cache に unit の項目が増える）、D10（Phase 2） |
| 手本にする既存実装 | IR テキストの行単位の後処理: `src/llvm.rs` の `emit_with_export_style`（`define internal i32 @tz.console.write(` を探して行を書き換える）と、実行時 IR を連結する箇所の `declare` の重複除去（`BTreeSet` の `declarations`）。linkage の書き換え: `src/llvm_abi.rs` の `.replacen("define internal ", "define ", 1)`。内容で決まる鍵と保護付きの保存: `src/cache.rs` の `build_key`・`Sha256`・`BuildCache::open`・`load`・`store`・`evict`。外部ツールの起動: `src/driver.rs` の `tool`・`run_tool`。Clang の呼び出し回数を数える E2E: `tests/cache.mjs` の `clang-wrapper.mjs` |
| 主な影響ファイル | `src/llvm_split.rs`（新規）, `src/llvm.rs`（`mod` 宣言、実行時 IR の定義名の一覧）, `src/driver.rs`（`build_complete` の分岐、`compile_units`・`link_units`（新規））, `src/cache.rs`（`unit_key`・`tool_fields`（新規）、`build_key` の環境変数の一覧）, `tests/codegen_units.mjs`（新規）, `tests/fixtures/codegen_units/`（新規）, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`（環境変数の一覧）, `_perfs/README.md`（状態）。旧版にあった `src/server.rs`（PB06）と `src/backend/`（PB05）は HEAD になく、Phase 1 では触らない |
| 計測対象 | PX03 の合成 1,000 モジュール（約 135K 行。旧版の「124K 行」に相当）と 200 モジュール。variant は `build-edit-body`（B5）、`build` の `O0`・`O3`（B1）、`build-warm`（B4）。加えて unit cache の大きさ（bytes） |

## 目的

`-O0` のビルドで、変更のない部分のコード生成（Clang）をやり直さない。生成済みの IR を決定的な codegen unit に分け、
unit ごとの object を内容の hash で cache し、変わった unit だけを並列にコンパイルして、全 object をリンクし直す。
1 関数の本体の変更で Clang 全体（合成 1,000 モジュールで `-O0` 3.70 s）を払わないことが狙いで、PB06（常駐）と G17（解析の cache）と
合わせて、旧版の目標「124K 行で 1 関数の変更後の `-O0` 再ビルドを 100 ms 級」に近づける。

分担（G17・PB06・PB07 の間の決定）: G17 はディスク上のモジュール単位のフロントエンド cache、PB06 は cache をメモリに保つ常駐サーバーと監視、
PB07 は IR 以降（unit への分割、unit の object cache、並列コンパイル、リンク）。PB07 は `CheckedModule` と IR テキストより前の段を変えない。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D1 が承認済みであること。承認前はどの手順にも着手しない。
- 台帳上の依存 PB06・G17 が `_perfs/README.md`・`_features/README.md` の状態欄で done であること
  （確認: `grep -n "PB06\|PB07\|G17" _perfs/README.md _features/README.md`）。Phase 1 のコードは両者の interface を使わない
  （IR テキストだけを入力にする）。人間が先行着手を明示的に許可した場合は、この条件を外してよい。
- 計測（手順 11）には PX01・PX03 が done であること（`benchmarks/metrics.mjs`、`benchmarks/build/generate.mjs`、`benchmarks/run-build.mjs`）。
  未完了なら手順 10 まで進め、手順 11 の前で止めて報告する。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR の形の棚卸しと、`-O0`・`-O3` の IR の保存）を取っていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. 手順 1 の棚卸しで、設計「IR の最上位の行」の表にない形の行が見つかった。分割器で推測して扱わない。
2. 定数でない global（`global`）の名前に通し番号が入っている、または `thread_local`・`appending`・`linkonce`・`weak` の global がある。
3. 分割したビルドと `TSUZURI_CODEGEN_UNITS=single`（新規）のビルドで、E2E のどれかの stdout・終了コード・トラップが異なる。
4. 無関係なモジュールの編集で unit のテキストが変わり（手順 7 の安定性テストの失敗）、原因が emitter の出力（関数本体に入る
   プログラム全体の通し番号など）にある。emitter を変えないと直らない場合は報告する。
5. `-O1`〜`-O3`、`--emit llvm`、`--emit object`、WASM、`--debug-info`、`--trap-info` の IR か成果物が HEAD から変わる。
6. 新しい crate、`unsafe`、既定の WASM import、Clang・リンカーの新しい必須の版が必要になった。
7. リンクが引数の長さ（`ARG_MAX`）以外の理由で、プラットフォーム固有の新しいフラグを要する。
8. 手順 11 の計測で、`build -O0 --no-cache`（B1）の中央値が before の広がりを超えて遅くなった。並列度や unit 数を推測で調整しない。
9. PB03・PB05・G17 のどれかが IR の受け渡し（テキスト以外の形、LLVM 以外の `-O0` バックエンド、モジュールごとの emit）を変えていて、
   IR テキストの分割が前提として成り立たない。

## 現状と計測（HEAD `f8dc655`）

### ビルドの経路（コードで確認）

- `src/driver.rs` の `build_complete` は IR テキスト全体を作った後、whole-build cache を引く。鍵は `src/cache.rs` の `build_key` で、
  `ir-and-embedded-runtime`（IR の全文）、全ソースと manifest、`format!("{options:?}")`、環境変数の一覧（`TSUZURI_CLANG` など 17 個）、
  ツールの実行ファイルの digest と `--version` の出力を含む。1 文字の編集でも鍵が変わり、miss になる。
- miss のとき、IR を一時ディレクトリの `module.ll` に書き、native の実行時 C（`task_runtime_source()`・`runtime/cpu.c`・`runtime/io.c` の
  連結）を `task.c` から `task.o` へ毎回 `clang -std=c11 -c` でコンパイルする。その後 Clang 1 回（`-x ir -Wno-override-module -O<n>`、
  `native_compile_args`、`--cpu native` なら `native_cpu_flag`）で IR のコンパイルとリンク（`-lm`、`task.o`、`-pthread`）を行う。
- driver にスレッドはない（`std::thread` の使用なし）。Clang は常に 1 プロセスで、全関数を一つの LLVM モジュールとして処理する。
- `--trap-info` は `run` で常に有効（`src/main.rs` の `trap_info: trap_info || action == Action::Run`）。`build` の既定は無効。
- docs/architecture.md の「不変条件」は「parse/check/IR の cache は作りません」と定める（PB07 は IR の cache を作らない。object だけ）。

### IR の形（2026-09-29 に `examples/computations` で確認）

- 関数はすべて `define internal`（`--emit llvm` の 71 個）。`linkonce`・`weak` は 0 個。
- 利用者関数の名前は `@tz.fn.<qualified_name>`（`src/check.rs` の `CheckedFunction::qualified_name` は `"{module}.{name}"`）で、
  モジュール名と関数名で決まる。例: `@tz.fn.Main.divide`。
- 生成関数の名前は通し番号を含み、他の関数の追加・削除でずれる: `@tz.fn.$lambda.10`、`@tz.fn.Result.map.$mono.7`
  （`src/polymorph.rs` の `format!("{}.$mono.{instance}", function.name)`、`src/closures.rs` の `.$mono.{id}`）、`@tz.specialized.{id}`
  （`src/llvm.rs`）。補助関数は持ち主の名前と番号: `@tz.apply.Main.divide.0`、`@tz.apply.Result.Run.$mono.2.0`。
- 型の名前は内容で決まる（`src/llvm.rs` の `canonical_type`）。例: `%"tz.union.Result.Result[i64,i64]"`。
- 実行時 IR は emitter の末尾で、使われるときだけ連結する（`runtime/recursive.ll`・`debug.ll`・`display.ll`・`numeric.ll`・`math.ll`・
  `closure.ll`・`character.ll`・`string.ll`・`utf8string.ll`・`heap-native.ll` など）。`define internal` の数/全 `define`: `numeric.ll` 12/21、
  `string.ll` 16/16、`heap-native.ll` 3/3、`closure.ll` 4/4、`console.ll` 1/1。
- `src/llvm.rs` の `Globals` は `definitions`・`next_metadata`・`traps`・`debug` などを持つ。`next_metadata` は `FIRST_METADATA`
  （`numeric.ll`・`math.ll` の metadata 番号の最大 + 1）から始まる。`-g` では `distinct !DICompileUnit` が 1 個
  （`src/llvm_debug.rs`）で、`examples/computations` の生成 metadata は `!267` から始まる。

### 計測済みの事実（2026-09-29、`--no-cache`、各 1 回、Apple M1 Max。`_perfs/README.md` と同じ値）

| 対象 | 行数 | IR 出力 | 実行ファイル `-O0` | 実行ファイル `-O3` | Clang（`-O0`／`-O3`） | IR |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 合成 1,000 モジュール | 124,003 | 2.58 s | 約 6.2 s | 約 10.1 s | 3.70 s／7.62 s | 29 MB |

1 関数の変更でも同じ時間がかかる。当時の生成器はリポジトリになく、PX03 の生成器（1 モジュール約 135 行）とは直接比べない。

### 再現（2026-09-29 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-PB07
target/release/tsuzuri build examples/computations --emit llvm -o /tmp/tz-work-PB07/computations.ll
grep -cE '^define' /tmp/tz-work-PB07/computations.ll                 # 71
grep -E '^define' /tmp/tz-work-PB07/computations.ll | grep -vc 'define internal'   # 0
grep -oE '@tz\.fn\.[A-Za-z0-9_.$]+' /tmp/tz-work-PB07/computations.ll | sort -u   # $lambda.8〜10、$mono.2〜7
grep -cE 'linkonce|weak' /tmp/tz-work-PB07/computations.ll           # 0
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before（HEAD または着手直前の commit）と after を PX03 の手順で記録して判断する。

- G1: PB07 単独（HEAD のフロントエンド）で、B5（1 モジュールの 1 関数の本体の変更）の `tool.clang` 段を、B1 `O0` の `tool.clang` の
  15% 以下にする（見込み: 変わる unit は編集したモジュールの unit だけ。リンクは全 object をやり直す）。
- G2: B5 の wall time を before より短くする。PB06・G17 の完了後は、常駐サーバー経由の B5 が 100 ms 級（旧版の目標）かを別に記録する。
- G3: B1 `O0 --no-cache` を悪化させない（unit の並列コンパイルで短くなる見込み。停止条件 8）。
- G4: B1 `O3` と B4（変更なしの再ビルド）は変化なし（単一 unit の経路と whole-build cache は HEAD のまま）。
- G5: unit cache の大きさを記録する（目標値なし）。

| 指標 | 単位・統計 | 対象 | 期待 |
| --- | --- | --- | --- |
| M1 本体の変更の再ビルド | ms（`wall_time`・`cpu_time`、warm-up 1 回を除く 9 標本の中央値・最小・最大） | PX03 `build-edit-body`（`O0`）、合成 200・1,000 モジュール | before より短い（G2） |
| M2 段階別時間 | ms（`phase_time`、段 `split`・`units`（新規）と `tool.clang`、中央値） | M1 と B1 `O0` | B5 の `tool.clang` ≤ B1 `O0` の 15%（G1） |
| M3 コンパイルした unit 数 | 個（1 標本、`tests/codegen_units.mjs` の wrapper で数える） | 合成 1,000 モジュールの B5 相当の編集 1 回 | 1（編集したモジュールの unit）。0 や全 unit は不具合 |
| M4 初回ビルド | ms（M1 と同じ統計） | PX03 `build` の `O0`・`O3`（`--no-cache`） | `O0` は短いか同等、`O3` は変化なし |
| M5 変更なしの再ビルド | ms（同上） | PX03 `build-warm` | 変化なし |
| M6 unit cache の大きさ | bytes（1 標本。`du -sk` × 1024） | 合成 1,000 モジュールの `build -O0` 1 回の後の `TSUZURI_CACHE_DIR` | 記録のみ |

`split` は IR の分割・安定名・unit テキストの組み立てと鍵の hash、`units` は unit cache の読み書きと object の書き出し（Clang を除く）。
PX01 の `TSUZURI_TIME_PASSES` の段として足す（PX01 の段名の規則に従う。PX01 が未完了なら段は足さず、手順 11 で止める）。

## 変えてはいけない意味

- プログラムの動作。PB07 は関数本体の命令を変えない。変えるのは記号の名前（安定名）、linkage（`internal` → `hidden`）、定数 global と
  metadata・属性の番号（unit 内の振り直し）と、unit への配置だけ。overflow・丸め・NaN・符号付きゼロ・評価順序・トラップ・所有権・借用は
  構成上保たれる。分割ありと `TSUZURI_CODEGEN_UNITS=single` の両方で既存の E2E が同じ結果になることで確かめる。
- 分割しない経路の IR と成果物。`-O1`〜`-O3`、`--emit llvm`・`object`・`header`・`wgsl`、WASM、`--debug-info`、`--trap-info`（`run` を含む）、
  Windows は HEAD と byte 単位で同じ IR を同じ Clang の呼び出しに渡す（停止条件 5）。
- 決定性。同じ入力から同じ unit の集合・unit テキスト・鍵・リンク順を作る。分割ありのビルド 2 回、および unit cache から組み立てたビルドと
  cache なしのビルドは byte 単位で同じ実行ファイルになる。分割ありと単一 unit の実行ファイルは byte 列が異なってよい（D5）。
- 公開される記号。実行ファイルの外部から見える記号（`nm -gU`）は単一 unit のビルドと同じ集合。追加する linkage は `hidden` だけ。
- cache の保護。unit の項目は whole-build と同じ `BuildCache`（marker、Unix の `0o700`、一時ディレクトリからの原子的な rename、
  SHA-256 の照合、`evict` の上限と 30 日）を使う。unit の項目にソースの本文やパスを入れない。
- WASM の既定の import（D-18）。WASM は Phase 1 で分割しない。
- CI に速度の閾値を入れない。

## 設計

### 全体の流れ

1. emitter は HEAD のまま IR テキストを 1 つ作る（`emit_native_build`）。whole-build cache の参照も HEAD のまま。hit なら終わり。
2. miss かつ `split_eligible`（新規）が真なら、`llvm::split::split`（新規）で IR を unit に分ける。分割に失敗したら単一 unit の経路に戻る（D3）。
3. unit ごとに `unit_key`（新規）で鍵を作り、`BuildCache::load` を引く。hit は object を一時ディレクトリへ書く。miss は束ねて並列に
   Clang でコンパイルし、`BuildCache::store` へ入れる。native の実行時 C（`task.c`）も unit `c.runtime`（新規）として同じ扱いにする。
4. 全 object を unit 順に response file へ並べ、Clang（ドライバー）で 1 回リンクする。増分リンクはしない（D8）。
5. 成果物を HEAD と同じく whole-build cache へ入れ、公開する。

### 適用条件（D2・D3）

`split_eligible` は次がすべて真のときだけ真。一つでも偽なら HEAD の単一 unit の経路を通る。

| 条件 | 理由 |
| --- | --- |
| `options.optimization == 0` | `-O1` 以上は unit 間のインライン展開を失う（D2） |
| `options.target == Target::Native` かつ `options.emit == Emit::Executable` | WASM・`--emit object` は Phase 2 |
| `!options.debug_info` かつ `!options.trap_info` | DWARF の compile unit とトラップ地点の通し番号は Phase 2 |
| `!options.wasm_threads` かつ `!cfg!(windows)` | Windows のリンカーは Phase 2 |
| `TSUZURI_CODEGEN_UNITS`（新規）が未設定・空・`auto` | `single` で分割を止める（差分テスト用）。他の値は `E2000` |

`TSUZURI_CODEGEN_UNITS` の値が不正なら `E2000`、メッセージ
`invalid TSUZURI_CODEGEN_UNITS value '<value>'; use 'auto' or 'single'`（位置なし）。この変数は `build_key` の環境変数の一覧に足す。

### IR の最上位の行

emitter が出す行は次の形に限る（手順 1 で全 fixture と examples の IR を走査して確かめる。表にない形は停止条件 1）。

| 形 | 扱い |
| --- | --- |
| `source_filename = ...`、`target datalayout = ...`、`target triple = ...` | 全 unit に複写 |
| `%name = type ...`（`%"..."` を含む） | unit から推移的に参照される型を、元の順序のまま複写（enum の別名が struct より前に来る順序を保つ） |
| `@name = private\|internal [unnamed_addr] constant ...` | 参照する unit ごとに複写し、unit 内で出現順に `@tz.unit.<k>`（新規の予約接頭辞）へ改名。linkage は `private` |
| `@name = [internal] global ...`（定数でない） | 持ち主の unit に 1 つ置き、`internal` を `hidden` にする。参照する他の unit は `@name = external hidden global <型>` |
| `declare ...` | 参照する unit に複写（記号名で重複除去） |
| `define ... @name(...) ... {` から行頭の `}` まで | 「unit の割り当て」の規則。`define internal` は `define hidden` にする |
| `attributes #N = { ... }` | 参照する unit に複写し、unit 内で出現順に `#0` から振り直す |
| `!N = ...`、`!name = !{...}` | 番号付きは参照から推移的に複写し、出現順に `!0` から振り直す。名前付き（`!llvm.module.flags` など）は全 unit に複写 |
| `;` で始まる行、空行 | 捨てる |

### unit の割り当て（D4）

`SplitInput`（新規）は IR テキストと、`CheckedModule::functions` のうち `name` に `$` を含まない関数の `qualified_name → module` の表
（安定な関数）を持つ。定義 `@S` は上から順に最初に当てはまる規則で unit を決める。

1. `S` が `tz.fn.<Q>` で `Q` が安定な関数 → unit `m.<module(Q)>`。名前は変えない。
2. `S` が `tz.<kind>.<Q>.<数字>` で `Q` が安定な関数（例: `tz.apply.Main.divide.0`）→ 同じ `m.<module(Q)>`。名前は変えない。
3. `S` が `$` を含むか、`.` で区切った成分に数字だけのものがある → 安定名へ改名し（D6）、unit `g.<x>`。
4. それ以外（実行時 IR、型の名前から作る補助関数、`@main` など）→ 名前は変えず、unit `g.<x>`。

`x` は最終的な名前（`@` を除く、引用符を外した byte 列）の SHA-256 の先頭 byte の下位 4 bit を 16 進 1 桁にしたもの（`g.0`〜`g.f`）。
定数でない global の持ち主も同じ `g.<x>`。unit の順序は `g.0`〜`g.f`、`m.<モジュール名>`（byte 順）、`c.runtime`。空の unit は作らない。
一時ファイル名は順序の番号で `u0000.ll`（モジュール名を file 名に使わない。大文字小文字を区別しない file system の衝突を避ける）。

### 安定名（D6）

通し番号を含む名前は、内容の hash に置き換える。依存する改名が先に決まるよう、規則 3 の定義どうしの参照グラフの強連結成分（SCC）を
呼ばれる側から順に処理する。

```text
for scc in tarjan(unstable definitions, edges = references among them), callees first:
    order members by original text position
    for each member m at index i:
        text_i = render([m]) where
                 m's own name        -> @tz.self
                 other members of scc -> @tz.scc.<index>
                 already renamed     -> their stable names
    digest = sha256(text_0 || 0x00 || text_1 || ...)
    for member at index i:
        prefix = original name with every all-digit "." component removed
        name   = prefix + ".h" + hex(digest)[0..16] + (".<i>" if scc has more than one member)
names equal to an earlier one (identical content, same prefix) get ".2", ".3", ... in original order
```

`render` は unit テキストの組み立てと同じ関数で、定数 global・属性・metadata を振り直した自己完結のテキストを返す。同じ内容の関数を
一つにまとめない（関数の同一性を変えない。D6）。

### unit テキストの組み立て（`render_unit`（新規））

1. unit の定義を元の順序で並べ、改名と `define internal` → `define hidden` を適用する。
2. 本文の `@name` を走査する。同じ unit の定義はそのまま。他の unit の関数は `declaration_of`（新規）で `define` の見出しから
   `declare hidden <戻り値> @name(<引数の型と ABI 属性>)` を作る（引数名 `%x` と `!dbg` を除き、`sret`・`byval`・`signext`・`zeroext`・
   `noundef` は残す）。定数 global は複写と改名、定数でない global は外部宣言、`declare` は複写。
3. 型・属性・metadata を上の表のとおり推移的に集めて振り直す。
4. 出力順は、見出しの行、型、global、`declare`（記号名順）、`define`、`attributes`、metadata。末尾に改行 1 つ。

分割の結果の例（形を示す。名前と hash は説明用）:

```llvm
source_filename = "tsuzuri"
%tz.string = type { ptr, i64 }
declare hidden i64 @tz.fn.$lambda.h1f0c2a9b3d4e5f60(ptr, i64)
define hidden i64 @tz.fn.Module7.f3(i64 %x) nounwind {
entry:
  ...
}
```

### unit の鍵と cache（D7）

- `tool_fields`（新規）: `build_key` の後半（ツールのパス・digest・`--version`・native CPU の特徴）を関数に切り出す。`build_key` の
  hash の入力と順序は変えない（whole-build の鍵は `TSUZURI_CODEGEN_UNITS` の追加以外 HEAD と同じ）。
- `unit_key(prefix: &[u8; 32], text: &str) -> String`（新規）: `prefix` は build ごとに 1 回だけ作る SHA-256 で、`FORMAT`、
  `"codegen-unit v1"`、compiler の binary digest、host の OS・arch、`target`・`optimization`・`cpu`・`wasm_simd` の値、環境変数の一覧
  （`build_key` と同じ）、`tool_fields` の Clang 分を入れる。鍵は `prefix` と unit テキストの SHA-256 の 16 進。ソース・パス・`action` は入れない。
- 保存は `BuildCache::store(key, paths, messages)` で、`paths` は `{"artifact": <object>}`。読み出しは `BuildCache::load(key)` で、
  `files` の鍵が `artifact` だけのときに使う（whole-build と同じ照合）。`evict` は build の最後に 1 回だけ呼ぶ（unit ごとに呼ばない）。
- `c.runtime` の鍵は `prefix` と `task.c` の本文と Clang の引数の列（`-std=c11 -c -fPIC -O0` など）の SHA-256。

### 並列コンパイルとリンク（D8）

- miss の unit を unit 順に並べ、`jobs = min(std::thread::available_parallelism, miss の数)` 個の束へ順に配る（i 番目は `i % jobs`）。
- 束ごとに Clang 1 プロセス: `clang -x ir -Wno-override-module -O0 -fPIC [native_cpu_flag] -c u0003.ll u0019.ll ...`、作業ディレクトリは
  一時ディレクトリで、出力は `u0003.o` など。`std::thread::scope` で束を並行に `run_tool` し、messages は束の順に集める。
- リンク: `objects.rsp`（新規の一時ファイル。1 行 1 path、`"` と `\` を escape して `"..."` で囲む）を作り、
  `clang @objects.rsp -lm [-pthread] -o artifact`。`-pthread` の条件は HEAD と同じ（`task_runtime && !cfg!(windows)`）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 分割 | `src/llvm_split.rs`（新規） | `SplitInput`・`Unit { name, text }`・`SplitError`・`split`・`parse_entities`・`assign_units`・`stable_names`・`render_unit`・`declaration_of`（すべて新規） | 上の「IR の最上位の行」から「unit テキストの組み立て」まで。`std` だけを使う |
| 宣言 | `src/llvm.rs` | 先頭の `#[path = "..."] mod` 群 | `#[path = "llvm_split.rs"] pub(crate) mod split;` |
| 鍵 | `src/cache.rs` | `build_key`・`tool_fields`・`unit_key` | 切り出しと新規。環境変数の一覧に `TSUZURI_CODEGEN_UNITS` |
| 駆動 | `src/driver.rs` | `build_complete`・`split_eligible`・`codegen_units_mode`・`compile_units`・`link_units` | miss の分岐。単一 unit の経路の行は動かさない |
| 計時 | PX01 の `src/timings.rs` | 段 `split`・`units` | PX01 が done のときだけ |
| 試験 | `tests/codegen_units.mjs`・`tests/fixtures/codegen_units/`（新規） | テスト計画 | 差分・安定性・cache・決定性 |

### Phase 分割

- Phase 1（この実装）: native（macOS・Linux）、`-O0`、実行ファイル、デバッグ情報とトラップ情報なし。
- Phase 2（設計方針。人間が求めた場合だけ。D10）: トラップ情報（unit 内の地点番号と unit ごとの基点表）、デバッグ情報（unit ごとの
  `DICompileUnit`、`dsymutil` が読む object を一時ディレクトリに残す）、WASM（`wasm-ld` に複数 object）、`--emit object`（macOS は
  `-Wl,-keep_private_externs` を付けた relocatable link）、Windows（`link.exe` の `/INCREMENTAL` の評価）、`$lambda` をモジュールの unit へ置く規則。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が 0 でないことを必ず見る（GUIDE §3.1）。一時ファイルは `/tmp/tz-pb07/` の下に置く。

### 手順 1: ベースラインと IR の棚卸し

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。実行ファイル用の IR（`emit_native_build` の出力）は `--emit llvm` では出ないので、
  Clang の wrapper で `.ll` の入力を写す。`-O0`・`-O3` の `--emit llvm` も保存する（手順 10 の byte 比較に使う）。
- 確認: 最後の `grep` が何も出さない（すべての最上位の行が設計の表の形）。出たら停止条件 1。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-pb07/base /tmp/tz-pb07/exe-ir /tmp/tz-pb07/exe
printf '#!/bin/sh\nfor a in "$@"; do case "$a" in *.ll) cp "$a" "/tmp/tz-pb07/exe-ir/$$.ll";; esac; done\nexec clang "$@"\n' > /tmp/tz-pb07/clang-copy.sh
chmod +x /tmp/tz-pb07/clang-copy.sh
for d in examples/*/ benchmarks/control benchmarks/computations tests/fixtures/*/; do
  [ -f "$d/Main.tz" ] || continue; n=$(echo "$d" | tr '/' '_')
  target/release/tsuzuri build "$d" --emit llvm -O0 --no-cache -o "/tmp/tz-pb07/base/$n-O0.ll"
  target/release/tsuzuri build "$d" --emit llvm -O3 --no-cache -o "/tmp/tz-pb07/base/$n-O3.ll"
  TSUZURI_CLANG=/tmp/tz-pb07/clang-copy.sh target/release/tsuzuri build "$d" -O0 --no-cache -o "/tmp/tz-pb07/exe/$n"
done
cat /tmp/tz-pb07/base/*.ll /tmp/tz-pb07/exe-ir/*.ll | grep -vE '^( |$|define |declare |attributes #|!|%|@|source_filename|target |;|[A-Za-z_.$0-9-]+:|\})' | sort | uniq -c
```

`E2004`（`Main.tz` でない fixture）や `main` のない project の失敗は無視してよい。`grep -hE '^@' ... | grep -v constant` で
定数でない global の名前も見て、通し番号があれば停止条件 2。

### 手順 2: `tool_fields` の切り出しと環境変数

- 変更: `src/cache.rs` の `build_key`、`tool_fields`（新規）。
- 内容: ツールの hash 入力を関数へ移すだけで、`build_key` の入力の順序は変えない。環境変数の一覧の末尾に `TSUZURI_CODEGEN_UNITS` を足す。
- 確認: `cargo test --locked --lib cache` が既存の `cache_checks_content_partial_entries_races_and_eviction` を含めて成功（N ≥ 1）。
  `cargo build --release --locked && node tests/cache.mjs target/release/tsuzuri` が成功。

### 手順 3: 最上位の行の解析

- 変更: `src/llvm_split.rs`（新規）の `parse_entities`・`SplitError`、`src/llvm.rs` の `#[path = "llvm_split.rs"] pub(crate) mod split;`。
- 内容: 設計の表の形だけを受ける。字句の走査は `"..."`・`c"..."`・`!"..."` の中を飛ばし、引用符付きの名前（`@"..."`・`%"..."`）を一つの名前として読む。
- 確認: `cargo test --locked --lib split::tests` が `parses_every_top_level_form`・`rejects_unknown_top_level_line`・
  `skips_quoted_strings_when_scanning` の 3 件で成功。

### 手順 4: unit の割り当てと安定名

- 変更: `assign_units`・`stable_names`（新規）。
- 内容: 設計「unit の割り当て」の規則 1〜4 と「安定名」の SCC の処理。SCC は Tarjan を再帰なしで書く（明示的な stack。関数数は数万になりうる）。
- 確認: `cargo test --locked --lib split::tests` が 8 件で成功（追加: `assigns_stable_functions_to_module_units`・
  `renames_counter_names_by_content`・`stable_names_ignore_unrelated_counters`・`names_recursive_components_deterministically`・
  `identical_bodies_keep_distinct_names`）。

### 手順 5: unit テキストの組み立て

- 変更: `render_unit`・`declaration_of`・`split`（新規）。
- 内容: 設計「unit テキストの組み立て」。`stable_names` の hash も `render_unit` の一要素版を使う（同じ関数を二つ書かない）。
- 確認: `cargo test --locked --lib split::tests` が 12 件で成功（追加: `declaration_keeps_abi_attributes`・
  `renumbers_constants_attributes_and_metadata`・`keeps_type_definition_order`・`split_is_deterministic_and_self_contained`）。

### 手順 6: 実 IR での安定性

- 変更: `src/llvm_split.rs` の `mod tests`。
- 内容: `src/llvm.rs` の `mod tests` が IR を作るのと同じ helper（`grep -n "fn .*source: &str" src/llvm.rs` で確かめる）で、2 モジュールの
  プロジェクトの IR を作る。版 B は版 A の `Main` に lambda と多相関数の呼び出しを先頭に足したもの（構文は既存の fixture から取り、
  `target/release/tsuzuri check` で確かめる）。`Helper` の unit と、変わらない生成関数を含む `g.<x>` のテキストが byte 単位で同じことを確かめる。
- 確認: `cargo test --locked --lib split::tests` が 13 件（`unrelated_edit_keeps_other_unit_texts`）で成功。失敗の原因が emitter なら停止条件 4。

### 手順 7: 鍵と駆動

- 変更: `src/cache.rs` の `unit_key`（新規）、`src/driver.rs` の `codegen_units_mode`・`split_eligible`・`compile_units`・`link_units`（新規）と
  `build_complete` の miss の分岐。
- 内容: 設計「全体の流れ」から「並列コンパイルとリンク」まで。`--no-cache` でも分割と並列コンパイルは行い、unit cache を読み書きしない。
  unit の `load` の失敗は whole-build と同じ文言 `build cache read failed; rebuilding: <error>` を messages に足してコンパイルする。
- 確認: `cargo test --locked --lib driver` が `codegen_units_mode_accepts_auto_single_and_rejects_others`・`split_eligible_matches_the_table`
  を含めて成功。`cargo build --release --locked && target/release/tsuzuri run examples/computations` が HEAD と同じ出力。

### 手順 8: E2E `tests/codegen_units.mjs`

- 変更: `tests/codegen_units.mjs`・`tests/fixtures/codegen_units/`（新規）。
- 内容: テスト計画の C1〜C8。`tests/cache.mjs` の形（`mkdtempSync`、`TSUZURI_CACHE_DIR`、`clang-wrapper.mjs`）を写す。
- 確認: `node tests/codegen_units.mjs target/release/tsuzuri` が成功し、最後に `codegen units: 8 cases passed` を出す。

### 手順 9: 既存の E2E を両方の経路で

- 変更: なし（失敗したら分割器を直す。期待値は変えない。停止条件 3）。
- 確認: 次の各 suite が、既定と `TSUZURI_CODEGEN_UNITS=single` の両方で成功する。BigInt の重い suite は Node 24。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for mode in auto single; do
  for s in e2e features examples control primitives strings tasks io computations numeric_casts math integer_intrinsics cache debug_info cpu_dispatch; do
    TSUZURI_CODEGEN_UNITS=$mode npx --yes --package=node@24 node tests/$s.mjs target/release/tsuzuri || echo "FAIL $mode $s"
  done
done
cargo test --locked
```

### 手順 10: 生成コードの確認

- 変更: なし。
- 確認: 「生成コードの確認」の 1〜4 がすべて期待どおり。

### 手順 11: 計測

- 変更: なし（PX01 が done なら `src/timings.rs` に段 `split`・`units` を足す。PX01 の段の規則に従う）。
- 確認: 「計測手順」の記録が `target/perf/<run_id>/` にそろう。停止条件 8 に当たらない。

### 手順 12: 文書と最終確認

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs docs/architecture.md docs/benchmarks.md README.md` が成功。GUIDE §3 の全体確認
  （`cargo build --release --locked`、`cargo test --locked`、手順 8・9）が成功。

## 計測手順

GUIDE §14 と PX03 の「PB・G17 の before／after」に従う。before は着手直前の commit の release compiler、after は手順 10 の後の release compiler。

1. 環境の記録: PX01 の `captureRun`（CPU、OS、`clang --version`、`rustc --version`、`node --version`、commit）。電源接続、他の重い処理なし。
2. 本計測: PX03 の「本計測（1 run）」のコマンドを before と after で 1 回ずつ実行する。M1・M2・M4・M5 は PX03 の `build-edit-body`・`build`・
   `build-warm` の記録（warm-up 1 回、9 標本、中央値・最小・最大）から読む。結果は `target/perf/<run_id>/build.jsonl`。
3. M3（時間を測らない別 run）:

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tz-pb07/g1000
export TSUZURI_CACHE_DIR=/tmp/tz-pb07/cache1000
target/release/tsuzuri build /tmp/tz-pb07/g1000 -O0 --cpu generic -o /tmp/tz-pb07/g1000.out
du -sk /tmp/tz-pb07/cache1000                     # M6（KiB。× 1024 で bytes）
node --input-type=module -e 'import { writeFileSync } from "node:fs"; import { moduleSource } from "./benchmarks/build/generate.mjs"; writeFileSync("/tmp/tz-pb07/g1000/Module500.tz", moduleSource(500, { salt: 1 }));'
TSUZURI_CLANG=/tmp/tz-pb07/clang-count.mjs target/release/tsuzuri build /tmp/tz-pb07/g1000 -O0 --cpu generic -o /tmp/tz-pb07/g1000.out
```

`/tmp/tz-pb07/clang-count.mjs` は `tests/codegen_units.mjs` の wrapper と同じもの（`.ll` の入力の数を 1 行ずつ log に書く）。
4. 記録: before／after の中央値と広がりを `docs/benchmarks.md` の表へ。G1 の比は after の B5 と B1 `O0` の `tool.clang` の中央値で計算する。
   計測値でない数（見込み）を表に入れない。

## 生成コードの確認

1. 分割しない経路の IR が不変: 手順 1 の `*-O0.ll`・`*-O3.ll` を after の compiler で作り直し、`cmp` がすべて一致する。
2. unit の object の記号: 手順 8 の cache の unit の項目 `<cache>/<key>/artifact` に
   `/opt/homebrew/opt/llvm@21/bin/llvm-nm -m <artifact>` を実行する。`_tz.fn.<Module>.<name>` が `private external`（hidden）、
   `_tz.unit.<k>` が `non-external` であること。`weak` と `external`（hidden でない）の `_tz.` 記号がないこと。
3. 公開される記号: 分割あり・`single` の実行ファイルで `nm -gU <exe> | awk '{print $3}' | sort` が一致する（Linux は `nm -g --defined-only`）。
4. 命令の同一性: `examples/computations` を両方の経路で `-O0` の実行ファイルにし、
   `/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn --disassemble-symbols=_tz.fn.Main.divide <exe>` の命令列が、
   分岐先・呼び出し先の番地を除いて一致する。

## テスト計画

### Rust テスト

- `src/llvm_split.rs` の `mod tests`（13 件）: 手順 3〜6 の名前。主な期待値:
  `rejects_unknown_top_level_line` は `module asm "x"` を `SplitError::UnknownLine`（新規）で拒否する。
  `renames_counter_names_by_content` は本文が同じで番号だけ違う 2 つの IR（`$mono.3` と `$mono.9`）から同じ安定名を作る。
  `identical_bodies_keep_distinct_names` は本文が同じ 2 つの lambda に `.h<hash>` と `.h<hash>.2` を与え、一つにまとめない。
  `declaration_keeps_abi_attributes` は `define internal void @f(ptr sret(%T) %out, i8 signext %x) nounwind {` から
  `declare hidden void @f(ptr sret(%T), i8 signext)` を作る。
  `split_is_deterministic_and_self_contained` は同じ入力の 2 回の `split` が同じで、各 unit の参照する記号・型・`#N`・`!N` がすべて unit 内で
  定義か宣言されている。
- `src/driver.rs` の `mod tests`: `codegen_units_mode_accepts_auto_single_and_rejects_others`（未設定・空・`auto`・`single` と、`x` の `E2000`）、
  `split_eligible_matches_the_table`（適用条件の表の各行を一つずつ偽にして false）。
- `src/cache.rs` の `mod tests`: `unit_key_depends_only_on_prefix_and_text`（本文 1 byte の差で鍵が変わり、同じ入力で同じ鍵）。

### E2E（`tests/codegen_units.mjs`、native のみ）

fixture は `Main.tz` と `Mod0.tz`〜`Mod3.tz`（record、union、closure、多相関数、文字列、stdin の値を種にした `i64u` の wrap 演算）。期待値は
テスト内の JS BigInt で独立に計算する（PX03 の生成器の参照値の方式）。各ケースは独立した一時ディレクトリ（E03 の再帰探索を避ける）。

- C1 差分: 既定と `single` の `-O0` の実行ファイルの stdout と終了コードが、入力 `12345` と空行で BigInt の参照値と一致する。
- C2 決定性: `--no-cache` の分割ありのビルド 2 回が byte 単位で同じ。
- C3 組み立て: cache ありでビルドし、`Mod1.tz` の関数本体の定数を変えてビルドした成果物が、同じ内容の `--no-cache` のビルドと byte 単位で同じ。
- C4 再コンパイル数: wrapper の log で、初回は全 unit、本体の定数の変更後は `.ll` 1 個、変更なしは Clang の起動なし（whole-build hit）、
  lambda の本体の変更後は 2 個以上かつ全 unit の半分未満。
- C5 破損: unit の項目の `artifact` を 1 byte 書き換えると、ビルドは成功し、出力は C1 と同じ。
- C6 適用条件: `-O3`・`--debug-info`・`--trap-info`・`run`・`--target wasm32` は Clang への `.ll` の入力が 1 個。
  `TSUZURI_CODEGEN_UNITS=x` は終了コード 1 と `E2000`。
- C7 公開記号: 分割ありと `single` の `nm -gU` が同じ。
- C8 競合: 同じ cache で 4 つの分割ありのビルドを同時に走らせ、すべて成功し、成果物が byte 単位で同じ。

### 既存テストへの影響

なし。既存の期待値（IR、診断、成果物の byte 比較）は分割しない経路か、分割ありの経路の中での比較（`tests/cache.mjs` の byte 一致）で、
どちらも成り立つ。成り立たないテストが出たら停止条件 3 か 5。

### 性能

M1〜M6 を PX03 で記録するだけで、CI に閾値を入れない。

## ドキュメント

- `docs/architecture.md`「不変条件」: Whole-build cache の段落の後に「codegen unit cache」の段落（適用条件、unit の規則、安定名、鍵の入力、
  `hidden` の linkage、`TSUZURI_CODEGEN_UNITS`）。「parse/check/IR の cache は作りません」は保つ（object だけを cache する）。
- `docs/benchmarks.md`: M1〜M6 の before／after と環境。
- `README.md`: `TSUZURI_CACHE_DIR` を説明している箇所（`grep -n TSUZURI_CACHE_DIR README.md`）に `TSUZURI_CODEGEN_UNITS` を 1 行。
- `_perfs/README.md`: PB07 の状態。Phase 1 の完了で `done`（Phase 2 は未着手と書く）。

## 受け入れ条件

- [ ] `-O0` の native 実行ファイルのビルドが適用条件の表どおりに分割され、それ以外の IR と Clang の呼び出しが HEAD と同じ（生成コードの確認 1）。
- [ ] 1 モジュールの関数本体の変更で、Clang がコンパイルする unit が 1 個（C4、M3）。
- [ ] 分割ありと `single` で既存の E2E がすべて成功し、C1〜C8 が成功する。
- [ ] unit テキスト・鍵・実行ファイルが決定的（C2・C3、`split_is_deterministic_and_self_contained`）。
- [ ] 公開される記号の集合が変わらない（C7）。
- [ ] M1〜M6 を before／after で記録し、実測と見込みを分けて `docs/benchmarks.md` に書いた。
- [ ] 新しい crate・`unsafe`・既定の WASM import がない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- Clang に入力を複数渡して `-o` を付けると「cannot specify -o when generating multiple output files」で失敗する。束のコンパイルでは `-o` を付けず、
  作業ディレクトリを一時ディレクトリにする。
- `build_key` のツールの `--version` を unit ごとに呼ぶと、1,000 unit で数秒かかる。`prefix` は build ごとに 1 回だけ作る。`evict` も最後に 1 回。
- 字句の走査で文字列定数を飛ばさないと、`c"...!12..."` の中の `!12` や `@x` を参照と誤認して番号を書き換える。`skips_quoted_strings_when_scanning` で守る。
- loop metadata は自己参照（`!5 = distinct !{!5, !6}`）を持つ。振り直しは参照の出現順で番号を決めてから定義を書き換える（2 パス）。
- 安定名の hash に、改名前の呼び出し先の名前を入れると、無関係な番号のずれで名前が変わる。SCC を呼ばれる側から処理し、改名後の名前で hash する。
- 型の定義は元の順序を保つ。enum の別名（`= type i32`）が struct より後に来ると LLVM が拒否する。
- `u` の一時ファイル名は 5 桁の 0 詰め（`u00000.ll`）。モジュール名を file 名に使うと、大文字小文字を区別しない file system で衝突する。
- 並列の束の messages を完了順に集めると、出力が非決定的になる。束の番号順に並べる。
- `std::thread::available_parallelism` は失敗しうる。失敗したら 1。
- 環境変数の一覧に `TSUZURI_CODEGEN_UNITS` を足すと、既存の whole-build の項目はすべて miss になる（一度だけ。想定どおり）。
- Mach-O の記号は `_` が前に付く（`_tz.fn.Main.divide`）。`llvm-nm`・`llvm-objdump` の確認で付け忘れない。
- `cargo test --locked split::tests` の N が 0 のまま成功することがある。N を必ず見る。

## 対象外

- 実行中のプロセスへのコードの差し替え（ホットリロード）。
- 増分リンク（実行ファイルの一部の書き換え）。ld64・GNU ld・LLD に増分リンクの機能がない（D8）。Windows の `/INCREMENTAL` は Phase 2 で評価する。
- `-O1` 以上の分割と ThinLTO。関数単位の unit と、インライン展開の依存グラフ（`-O0` では unit 間のインライン展開がないので不要）。
- emitter が最初から安定名を出す変更（Phase 2 以降の候補）。フロントエンドの cache（G17）、常駐サーバー（PB06）、PB05 のバックエンド。

## 決定事項

### D1: 分割の方式と既定での有効化

- 決定: emitter は変えず、生成済みの IR テキストを `src/llvm_split.rs`（新規）で unit に分ける。適用条件を満たすビルドでは既定で有効にする。
- 理由: emitter の多数の経路を触らずに、分割しない経路の IR を byte 単位で保てる。分割の誤りは `single` との差分テストで検出できる。
  既定で有効にしないと、利用者の `-O0` の再ビルドが速くならない。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: `-O1` 以上は単一 unit

- 決定: 分割は `-O0` だけ。`-O1`〜`-O3` は HEAD の単一 unit の経路。
- 理由: unit 間のインライン展開を失うと生成コードが遅くなる（AGENTS.md の性能方針）。ThinLTO はリンカーの LTO 対応が要り、Phase 1 の範囲を超える。
- 状態: 既定案（実装者はこの案に従う）

### D3: 適用条件と失敗時の扱い

- 決定: 設計の表の条件。`split` が `SplitError` を返したら、そのビルドは単一 unit の経路で続ける（エラーにしない）。
- 理由: 単一 unit の経路は常に正しい。分割器の誤りは Rust テストと E2E（全 fixture の分割）で CI が検出する。
- 状態: 既定案（実装者はこの案に従う）

### D4: 分割の単位と生成関数の置き場所

- 決定: 安定な利用者関数とその補助関数はモジュールごとの unit `m.<module>`。単相化の instance・lambda・known-call worker・実行時 IR・その他は、
  名前の hash による 16 個の unit `g.0`〜`g.f` に 1 つだけ置く（持ち主は 1 つ）。`linkonce_odr` は使わない。
- 理由: `linkonce_odr` で使う unit ごとに複写すると、よく使う instance を unit の数だけコンパイルし、初回ビルドが遅くなる。定義元のモジュールを
  持ち主にすると、他のモジュールが instance を足すたびに定義元の unit が変わる。hash の bucket なら、instance の追加は 1 bucket だけを変える。
  関数単位の unit は object と cache の項目が 2 万個規模になり、リンクと cache の費用が増える。
- 状態: 既定案（実装者はこの案に従う）

### D5: 一致の基準

- 決定: 分割ありと単一 unit は動作の一致だけを求める（byte 列は異なってよい）。同じ経路の中では unit テキスト・鍵・実行ファイルを byte 単位で
  決定的にする。`-O1` 以上は常に単一 unit で、HEAD と同じ決定性を保つ。
- 理由: 関数の配置と記号表は unit の分け方で変わる。旧版の未決事項の既定案と同じ。
- 状態: 既定案（実装者はこの案に従う）

### D6: 安定名

- 決定: 通し番号を含む名前を、SCC ごとの内容の SHA-256 の先頭 16 桁で置き換える。本文が同じ関数も一つにまとめず、`.2` などで区別する。
- 理由: 通し番号は無関係な編集でずれ、参照する全 unit を変える。まとめると関数の同一性が変わりうる（関数値の比較や記号の数え方）。
- 状態: 既定案（実装者はこの案に従う）

### D7: unit cache

- 決定: whole-build と同じ `BuildCache`（同じ root、同じ保護と `evict`）に、鍵 `unit_key` で object を 1 つずつ入れる。鍵にソースとパスを入れない。
  whole-build cache を先に引き、miss のときだけ unit を使う。`--no-cache` は unit cache も使わない。
- 理由: G11 の保護をそのまま使える。ソースを鍵に入れると、どの編集でも全 unit が miss になる。
- 状態: 既定案（実装者はこの案に従う）

### D8: コンパイルとリンク

- 決定: miss の unit を `available_parallelism` 個の束に分け、束ごとに Clang 1 プロセスで `-c`。リンクは Clang ドライバーで全 object を
  response file から毎回やり直す。増分リンクはしない。
- 理由: プロセスの起動を束で償却できる。response file で `ARG_MAX` を避ける。native のリンカーに増分リンクがない。新しい crate が要らない（`std::thread::scope`）。
- 状態: 既定案（実装者はこの案に従う）

### D9: 可視性

- 決定: 分割した unit の `define internal` と定数でない `internal global` は `hidden` にする。定数 global は unit ごとの `private` の複写。
- 理由: 参照される側の unit のテキストが、参照する側の編集で変わらない（参照の有無で linkage を選ぶと変わる）。`hidden` は実行ファイルの外へ出ない。
- 状態: 既定案（実装者はこの案に従う）

### D10: Phase 2

- 決定: トラップ情報・デバッグ情報・WASM・`--emit object`・Windows・`$lambda` のモジュール unit への配置は Phase 2（設計「Phase 分割」）。
- 理由: それぞれプログラム全体の番号（トラップ地点、`DICompileUnit`）か、プラットフォーム固有のリンク（`-keep_private_externs`、`link.exe`）が要る。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D11: 切り替えの手段と段の名前

- 決定: 環境変数 `TSUZURI_CODEGEN_UNITS`（`auto`・`single`）だけを足し、CLI のオプションは足さない。段階別時間の段は `split`・`units`。
- 理由: 差分テストと不具合の切り分けに必要な最小の手段で、公開の CLI を増やさない。`build_key` に入れるので、経路の違う成果物を cache で取り違えない。
- 状態: 既定案（実装者はこの案に従う）
