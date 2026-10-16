# PB04: フロントエンドの並列化

本文の起票時の設計（Phase 1 は構文解析だけ、`analyze_inputs_threaded` など）と手順は調査履歴です。現行の方針・決定事項・目標・停止条件・テストコマンドは
「着手時の再検討（2026-10-16）」と末尾の「実装と検証」を優先します。

| 項目 | 内容 |
| --- | --- |
| ID | PB04 |
| 分類 | ビルド速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PB02 |
| 関連 | G17, PB06 |
| 状態 | doing |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-16 に HEAD `6d15b437` で再検討（「着手時の再検討」の節） |
| 承認 | Phase 1 は不要（新しい crate を使わない。D1）。D8（Phase 2: 本体の型検査と所有権検査の並列化）は 2026-10-16 の全フェーズ実装依頼で承認済み |
| 手本にする既存実装 | 並列実行と入力順への復元: `src/test_runner.rs` のテストの並列実行（`std::thread::available_parallelism().map_or(1, usize::from).min(32)`、`std::thread::scope`、`AtomicUsize` の cursor を `fetch_add` で進める、`results.sort_by_key(\|(index, _)\| *index)`）。環境変数の読み取り: `src/cache.rs` の `TSUZURI_CACHE_DIR`（`std::env::var_os`）。診断の集約: `src/diagnostic.rs` の `Diagnostics`（`is_full`・`push`・`extend`・`finish`） |
| 主な影響ファイル | 起票時の一覧（改訂後は「着手時の再検討」と末尾の「実装と検証」。新規の `src/parallel.rs`・`src/check_functions.rs` と `src/frontend_cache.rs`・`src/driver.rs` の `Project::analyze_cached`・`src/main.rs` の usage が加わる）: Phase 1: `src/lib.rs`（`analyze_inputs_indexed_all` の分割、`analyze_inputs_threaded`・`parse_input`・`parse_inputs`・`thread_setting`・`frontend_threads`・`MAX_FRONTEND_THREADS`・`WORKER_STACK_BYTES`・`PARALLEL_PARSE_MIN_BYTES`・`#[cfg(test)] mod tests`（すべて新規））, `src/main.rs`（usage の環境変数の一覧）, `tests/parallel_frontend.mjs`（新規）, `README.md`（`## CLI` の環境変数）, `docs/architecture.md`（`## 不変条件`・`## 性能設計の原則`）, `docs/benchmarks.md`（PB04 の節）, `_perfs/README.md`（状態）。Phase 2（D8 の承認後だけ）: `src/recursive.rs`（`Cache`）, `src/check.rs`（`CheckedRecord`・`CheckedUnion`・`check_modules_collect`）, `src/ownership.rs`（`check_functions`）。`src/lexer.rs`・`src/parser.rs`・`src/llvm.rs` は変えない |
| 計測対象 | PX03 の合成 200・1,000 モジュール（PX03 の生成器で約 27K・135K 行）と 1 ファイルの hello。variant は `check`（B3）と `emit-llvm`（B2）、段 `parse`・`check`・`total`（B7）、最大 RSS（B8）。`TSUZURI_THREADS`（新規）= 1・2・4・8・10 |

## 目的

フロントエンドを複数のコアで実行し、CPU 数に応じて `check`・`build` を速くする。出力（診断、`--emit llvm` の IR、成果物）は
スレッド数によらず byte 単位で同じにする。

- Phase 1（この実装）: ファイルごとの字句解析と構文解析を並列に行う。共有する可変状態がなく、入力と出力の対応が 1 対 1 だからである。
- Phase 2（設計方針だけ。D8）: 関数ごとの所有権検査、次に関数本体の型検査。共有データの `Sync` 化が前提になる。
- 単相化・closure lowering・IR 生成は並列化しない（D9）。LLVM 側の並列コード生成は PB07（codegen unit）と G17 の担当である。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求め、D8 が承認された場合だけ着手する。

## 着手時の再検討（2026-10-16）

2026-10-16 の全フェーズ実装の依頼で、D8 を含む `要承認` はすべて承認済みです。HEAD は `6d15b437`（PB02・PX01〜PX03・G17 の frontend cache・G19 の edition が実装済み）。
起票時の「実装者は Phase 1 だけを実装する」は適用せず、意味の保持（診断・IR・pack の byte 同一）と検証の条件は維持します。
調査と試作は repo の外の scratch で行いました（Apple M1 Max 10 コア。他の agent が同時に動く共有機で load average 9〜80、Defender の影響あり）。
時間の値は best-of-N の参考値で、正式な前後比較は静かな機械で取り直します。

### 現行コードとの照合

| # | 起票時の記述 | 現行 | 扱い |
| ---: | --- | --- | --- |
| 1 | 構文解析のループは `analyze_inputs_indexed_all` | 実体は `analyze_inputs_with(inputs, semantic, cache)`（G17）で、前者は委譲だけ。ループは `module_identity`（E1011）・frontend cache・`ModuleRecord`・`cache.finish` を含む | `parse_input` に identity を含め、cache の `&mut` を分離する |
| 2 | `parse_with_source_all(source, id)` | 第 3 引数 `edition`（G19）。純関数性は保たれる | `input.edition` を渡す |
| 3 | std は 18 file・50,853 bytes、hello は 19 file | 47 file・417,738 bytes。CLI は opt-in を名指しされた分だけ読むので、hello は std 32 file（113,318 bytes）と Main | 256 KiB の閾値と `parse_inputs_skips_small_projects`（`stdlib::SOURCES` は 417 KB で超える）は成り立たない。閾値は段ごとの仕事量で決める |
| 4 | `MAX_SOURCE_BYTES` 1 MiB、特殊化の上限 1,024 | 4 MiB（G17 Phase 3）、65,536（D-41） | D6 の負荷分散の前提が弱まる。大きい file から取る |
| 5 | frontend cache への言及なし | `check`・`build`・`run`・`test`・`bench`・`doc`・`script` は既定で frontend cache を使う（hit は SHA-256 と復号、miss は parse・符号化・interface hash） | `FrontendCache::parse(&mut self)` を `&self` の lookup と逐次の commit に分ける |
| 6 | parse の割合は未計測。停止条件 6（5% 未満） | `check` の 1,000 module で 9.6%、`emit-llvm` で 5.6%、cache が warm で 6.2% | 停止条件には当たらないが、Phase 1 単独の効果は約 −7% が上限。Phase 2 と合わせて評価する |
| 7 | Phase 2 の障害は `recursive::Cache`・`derive.rs` の `Cell`・`exhaustiveness.rs` の `Rc` | 後の 2 つは関数内の局所値で障害ではない。実際の障害は `Cache` と `Names.symbols`（`Interner`）の `Rc<str>`（PB02 P2-2）の 2 つだけ | 2 箇所の修正で `CheckedModule: Sync`・`TypeContext: Send + Sync`・`Names: Sync`・`Checker: Send` になる（compile 時の監査 28 項目で確認） |
| 8 | `solve_members` と `finish` の loop は逐次のまま | `solve_members` は 1 ms で逐次でよい。finish の loop は 1,000 module で 147 ms（7.8%）あり並列にできる | finish も並列にする |
| 9 | 所有権検査 2 回を同じ段として扱う | `infer_copy_all` は `check` 段（162 ms）の中、`ownership` 段は `check_all` だけ（154 ms）。PR01 の `ownership_free` で scalar の関数は検査を省く | 指標は `check` と `ownership` を分けて読む |
| 10 | `computation::expand` は `chunks_mut` で分ける | HEAD は `expand(&mut body, &names, module)`。1 関数の出力は `(recovered, Result, Checker)`。`active_results`・`external_functions`・`classes.derived_error` が絡む | `Vec<Mutex<Option<&mut T>>>` の slot と動的な取り出しで足りる（試作で確認、`unsafe` なし） |
| 11 | 並列にしない段は const fold・recursion など | check の後半（const 29・recursion 77・`unused_private` 146・validate 34 ms）が 284 ms で、本体検査（254 ms）より大きい | PB04 の対象外。逐次の最適化として別に起票する候補 |
| 12 | 段の名前の前提 | 一致。ただし `tests/time_passes.rs` が段名の許可表と「段の和 ≤ total」を assert する | 段名を足さず、worker の中では `Phase` を使わない（CPU 時間の和になる） |
| 13 | 計測値: check 2.12 s、RSS 1.12 GB、IR 2.58 s（124,003 行の旧生成器） | 現行の生成器（139,010 行）: check 1.87〜1.98 s、RSS 650 MB（PB02 後）、emit-llvm 3.2〜3.5 s（emit が 1.2 s）。200 module は 0.37〜0.42 s、hello は 40〜45 ms | G1 の基準が変わる |
| 14 | G1: `check` を 1/5 | 並列にできる段だけなら上限 1.9 倍。check の後半と load も含めて無限に速くしても 4.0 倍、現実的には約 2.2 倍 | G1 を改める（下の「目標」） |
| 15 | 手順 1 の 5 テスト | `honors_the_exact_specialization_limit` は `src/polymorph.rs` の単体テスト（`--lib`）。`bounds_recursive_and_flat_expression_depth` は `src/parser.rs`。`bounds_computation_syntax_and_expansion` は CI 標準の `RUST_MIN_STACK=4194304` が要る | コマンドを直す（下の「テストコマンドの訂正」）。`--test polymorphism honors…` は 0 件で成功してしまう |
| 16 | `thread::scope` は cache.rs と test_runner.rs だけ | `fetch.rs` に 2 箇所（pipe）、`driver.rs`・`bindgen_driver.rs` に `thread::spawn`。stack の指定は無く、並列 map の先例は test_runner だけ | 結論は同じ。`Builder::stack_size` は新規 |
| 17 | 呼び出し側の thread も作業する（D4） | Windows の main thread の stack は 1 MiB、worker は 8 MiB。同じ入力が thread によって受理も拒否もされうる | worker が 2 つ以上のとき、呼び出し側は join だけをする |
| 18 | library が環境変数を読み、不正値は `E2000` | LSP・REPL・Rust テストが不正値で全滅する。`cargo test` の oversubscription もある | 環境変数は library も読むが既定は 1。自動は CLI だけ。不正値の `E2000` は CLI の起動時に検証する |
| 19 | `TSUZURI_THREADS` は `build_key` に入れない | `build_key` は `format!("{options:?}")`（`BuildOptions` の Debug）を hash する | thread 数を `BuildOptions` や `TestOptions` に足さない。テストで証明する |
| 20 | 文書は README・architecture・benchmarks | `_tsuzuri/language-reference/compiler/{option,usage}.md`・`languages/strategy.md`・README の「CLI リファレンス」・`architecture.md` の「段階別時間」が抜けている（AGENTS.md） | 文書の表に足す |
| 21 | 計測は PX03 の `run-build.mjs` | `benchmarks/run-frontend.mjs`（PB02。hello・200・1,000 の `check`・`emit-llvm`、9 標本、cache 無効、環境変数を継承、IR と診断の同一性を assert）が適する。`run-build.mjs` は Clang まで走る | 計測手順を差し替える |

### 測定した事実（HEAD、cache 無効、`check`、ms。best-of-N）

| 段 | hello | 200 module | 1,000 module（割合） |
| --- | ---: | ---: | ---: |
| total | 40.2 | 369 | 1,873（100%） |
| load（逐次） | 0.3 | 9 | 226（12.1%） |
| parse（lex と syntax は約 4 : 6） | 5.4 | 41 | 180（9.6%） |
| check: 名前・署名の収集（逐次） | 4.7 | 20 | 79（4.2%） |
| check: 関数本体の検査 | 11.0 | 61 | 254（13.6%） |
| check: finish の loop | 3.8 | 32 | 147（7.8%） |
| check: 後半（const fold・recursion・`unused_private`・validate） | 3.4 | 59 | 284（15.2%） |
| check: `infer_copy_all`（所有権・Copy の推論） | 7.1 | 38 | 162（8.7%） |
| specialize ／ closures（逐次、D9） | 3.3 ／ 0 | 50 ／ 8 | 221 ／ 38（11.8% ／ 2.0%） |
| ownership（`check_all`） | 0.1 | 30 | 154（8.2%） |
| 段に入らない残り（終了時の drop など） | 約 1 | 約 20 | 約 125（6.7%） |

- `emit-llvm`（1,000 module）: total 3,247 のうち emit が 1,221（37.6%）。emit は D9 の理由（`Globals` の定義列・metadata の番号・trap 点・statics の初出順）で逐次のまま。
- 既定の frontend cache での `parse`（1,000 module）: cold 297〜313、warm 118〜140、cache 無効 180。
- 試作（scratch。parse・本体・finish・所有権を並列化）: 1,000 module の `check` が 1 thread 1,887 ms → 8 thread 1,323 ms（1.43 倍）、10 thread 1,295 ms。
  200 module は 378 → 240 ms（1.58 倍）。hello は負荷の下で 1 thread の中央値 57 ms → 4 thread 38 ms。`emit-llvm`（1,000）は 3,239 → 2,546 ms（1.27 倍）。
  user CPU は 1.56〜1.60 s → 2.17〜2.22 s（+36〜39%）、最大 RSS は 658 → 688〜696 MB（+5〜6%）。診断・IR・型付き構造の `{:?}` は HEAD と byte 一致でした。
- 並列にする段だけなら Amdahl の上限は 1.9 倍（47.9%）。逐次のまま残る約 464 ms（名前・署名 79、specialize 221、closures 38、終了時の残り 125）が床です。

### 改訂した方針

- 段の順は効果とリスクで決める: Sync 化 → helper と設定 → 所有権 → finish → 本体検査 → frontend cache の分割 → parse。commit は 10 個に分け、それぞれ単独で検証する。
- 共有の可変状態は再帰型の memo（`recursive::Cache`）だけで、値は key の純関数。それ以外は read-only の借用か、関数・file ごとの出力にし、
  入力の順に HEAD と同じ accumulator へ replay する（`Diagnostics::is_full` の打ち切りと先勝ちの重複排除、`warnings` と `name_uses` の順を含む）。
- 小さい入力は段ごとの仕事量（関数 256 件、parse は 16 file かつ 64 KiB が worker 1 つ分）で逐次にする。値は hello と 200 module の計測で 1 回だけ調整してよい。

### 決定事項の更新（本文の D1〜D9 に対する）

- D1 維持。D3 維持して Phase 2 にも適用する（index の順に replay、`is_full` の打ち切り）。D7 維持（段名を足さず、壁時計は main thread だけで測る）。D9 維持（load・名前と署名の収集・`solve_members`・specialize・closures・emit は逐次）。
- D2 変更: 環境変数 `TSUZURI_THREADS` だけ。未設定または空は自動。自動の既定は CLI が `min(available_parallelism, 8)`、library・LSP・REPL・Rust テストは 1。`1`〜`32` は worker 数の上限で、`1` は HEAD と同じ経路（逐次・遅延 parse）。
  それ以外（`0`・`33` 以上・数字以外・UTF-8 でない値）は `E2000`（CLI は起動時に検証）。CLI の flag は足さない。thread 数はどの cache の鍵にも入れない（出力が同じなので）。
- D4 変更: worker の stack は 8 MiB（明示）のまま。worker が 2 つ以上のとき、呼び出し側は作業せず join だけをする（Windows の main thread の 1 MiB との非対称を避ける）。
- D5 変更: 入力のバイト数の閾値をやめ、段ごとの仕事量で worker の数を決める（`min(設定, 項目数 ÷ 項目あたりの下限)`。1 以下なら呼び出し側で逐次）。
- D6 変更: 動的な cursor は維持する。関数の段は 8 件ずつ、parse は大きい file から取る（1 file は最大 4 MiB）。結果は index の順に戻す。
- D8 承認済み。順序は「Sync 化 → 所有権 → finish → 本体検査」（finish も並列にする）。
- D10（新）`FrontendCache` を `&self` の `lookup` と `&mut self` の `commit` に分ける（pack の内容と `stats` は入力の順に再生して従来と同じにする）。
- D11（新）`recursive::Cache` は `Mutex` を包む newtype（`Clone` は中身を複製、`Debug` は従来と同じ文字列）。1,000 module の解析で参照は約 7.4 万回で、競合は測定上なし。
- D12（新）`Interner` の `Rc<str>` は `Arc<str>` にする（`lookup` は参照数を触らないので速度は変わらない）。
- D13（新）CLI で既定を自動にするのは、同値性の検証が通り、hello と 200 module が遅くならず、最大 RSS が +10% 以内で、小さな入力が逐次のままのときだけ。満たさなければ opt-in で出荷して理由を記録する。
- D14（新）目標の改訂（下）。

### 目標（G1〜G4 の改訂。CI の合否条件にはしない）

- G1: 8 thread で、合成 1,000 module の `check` を 1 thread の 1.4 倍以上に、`emit-llvm` を 1.2 倍以上にする。hello は悪化させない。（起票時の「1/5」は上のとおり到達できない。）
- G2: 合成 1,000 module の `parse` 段の壁時計時間を 10 thread で 1 thread の 1/4 以下にする（試作は 171 → 39 ms）。
- G3: hello と合成 200 module の `check` は悪化させない（9 標本の最小〜最大の範囲内、またはより短い）。
- G4: 合成 1,000 module の `check` の最大 RSS の増加を 10% 以内にする。
- G5（記録のみ）: user + sys の CPU 時間の増加（試作は +36〜39%）を M4 として表に出す。

### 停止条件の改訂

- 条件 1（`Program`・`Diagnostic` が `Send` でない）は満たされている（確認済み）。代わりに「`Checker`・`CheckedModule` の `Send`／`Sync` に `unsafe impl` や `Arc<Mutex<..>>` での包み直しが要る」を止める条件にする。
- 条件 6（parse の割合 5% 未満）は Phase 1 単独でなく全体の効果で評価する（parse は `check` の 9.6%）。
- 条件 2・3・4・5・7 は維持する（`unsafe`・新 crate・`set_var`、既存テストの期待値の変更、stack と上限の回帰、thread 数ごとの不一致、hello の悪化）。

### テストコマンドの訂正（手順 1・5）

```sh
cargo test --locked --lib bounds_recursive_and_flat_expression_depth     # src/parser.rs
cargo test --locked --lib honors_the_exact_specialization_limit          # src/polymorph.rs（--test polymorphism では 0 件）
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test tasks bounds_nested_task_syntax_and_types
RUST_MIN_STACK=4194304 cargo test --locked --test computations bounds_computation_syntax_and_expansion   # CI 標準の 4 MiB
```

`RUST_MIN_STACK` を付けた `cargo test --locked` を基準にする（README の「テストスイートの実行」と同じ）。並列を確かめるテストは、環境変数ではなく API に thread 数を渡す
（`std::env::set_var` は使わない）。全テストを並列で通す確認だけは、`TSUZURI_THREADS=8` を `cargo` のプロセスへ渡す。

### 文書の追加先

`_tsuzuri/language-reference/compiler/option.md`（環境変数の表と「段階別時間」の注記）、`compiler/usage.md`、`languages/strategy.md`、`README.md`（環境変数の欄と test の一覧）、`src/main.rs` の usage、
`docs/architecture.md`（不変条件・性能設計の原則・段階別時間）、`docs/benchmarks.md`（PB04 の節）。`_perfs/README.md` は統合担当が更新する。

## 着手条件と停止条件

### 着手条件

- PB02 が `_perfs/README.md` の状態欄で done であること。確認: `grep -n "PB02\|PB04" _perfs/README.md`。PB02 は構文木と
  token の確保を変えるので、並列化はその後の単一スレッドの値を基準にする。
- PX01・PX03 は実装の着手条件にしない。ただし手順 9（計測）は PX03 が done になってから行う。PX03 がなければ手順 8 までで止め、
  計測が未了であることを報告する（代わりの計測基盤を作らない）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースラインを保存していること。

### 前提とする他チケットのインターフェース

- PB02: `parser::parse_with_source_all(source: &str, source_id: usize) -> Result<Program, Vec<Diagnostic>>` は、入力の文字列と
  source id だけで結果が決まる純粋な関数のままである。ファイルをまたぐ共有の interner や arena を持たない。`Program` と `Diagnostic` は
  `Send` である（HEAD では `src/syntax.rs`・`src/parser.rs`・`src/lexer.rs` に `Rc`・`Cell`・`RefCell`・`static mut`・`thread_local` がない）。
- PX01: `TSUZURI_TIME_PASSES=1` で stderr に段の時間を出し、段の名前は `load`・`parse`・`check`・`specialize`・`closures`・
  `ownership`・`emit`・`total` など（PX03 の「前提とする他チケットのインターフェース」と同じ）。PB04 は段の名前を足さない。並列化した
  `parse` の値は壁時計の時間（全 worker の合計ではない）とする。
- PX03: 生成器 `benchmarks/build/generate.mjs` と runner が、`check`・`emit-llvm` の variant を環境変数付きで実行できる。
  環境変数を渡す手段がなければ、PX03 の runner の `env` 引数（PX01 の `measureProcess` の `env`）を使う。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. `Program` または `Diagnostic` が `Send` でなく、`std::thread::scope` の spawn が compile できない（PB02 が `Rc` の interner を
   入れた場合など）。`Arc`・`Mutex` で包み直す、`unsafe impl Send` を書く、といった回避をしない。
2. `unsafe`、新しい crate（`rayon`・`crossbeam` など）、テストでの `std::env::set_var`（edition 2024 では `unsafe`。`unsafe_code = "forbid"`）が
   必要になった。
3. 既存テストの期待値（診断のコード・メッセージ・順序、IR）を変える必要がある。
4. 手順 1 の stack-depth の 5 テストのどれかが失敗する。または worker の stack を D4 の値より大きくしたくなった、`MAX_NESTING`
   （`src/syntax.rs`、128）を変えたくなった。
5. スレッド数 1・2・8 で診断・IR・成果物のどれかが一致しない（手順 6・7 のテスト）。並べ替えの規則を今と変えて一致させない。
6. 手順 9 の計測で、PX03 の合成 1,000 モジュールの `parse` 段が `total` の 5% 未満だった。Phase 1 の効果が計測の広がりに埋もれるので、
   数値を添えて報告し、以後の手順（文書の性能の記述）を止める。
7. hello（1 ファイル）の `check` が `TSUZURI_THREADS=1` より遅くなり、D5 の閾値の調整（`PARALLEL_PARSE_MIN_BYTES` の 1 回の見直し）で
   戻らない。

## 現状と計測（HEAD `f8dc655`）

### 処理の流れ（コードで確認）

- 読み込み: `src/driver.rs` の `Project::load_with_overlays` → `load_from_root` がファイルを 1 つずつ読み、1 MiB（`MAX_SOURCE_BYTES`）を
  超えると `E0003`。source の並びは `sort_by_cached_key`（相対 path の文字列、`\` を `/` に置換）で決まる。読み込みは Phase 1 でも逐次のまま（D9）。
- 解析の入口: `Project::analyze_all` が `crate::SourceInput` の列を作り、`crate::analyze_inputs_all` → `src/lib.rs` の
  `analyze_inputs_indexed_all(inputs, semantic)` を呼ぶ。LSP の `analyze_modules_with_semantics` と Rust テストの `analyze`・
  `analyze_modules*` も同じ関数を通る（コメント「The single parse entry point」）。
- `analyze_inputs_indexed_all` の構文解析は逐次のループである。各 input で `diagnostics.is_full()` なら break、モジュール名を
  `module_name_from_relative`（User）か `stdlib::module_name`（Std、失敗は `E1011`）で決め、`parser::parse_with_source_all(input.text, id)` を
  呼び、成功なら `program.source_kind` を拡張子から設定して `programs` に積み、失敗なら `diagnostics.extend(errors)`。一つでも診断があれば
  `Err(diagnostics.finish())`。なければ `ModuleInput` を組んで `check::check_modules_indexed_all` を呼ぶ。
- 入力には埋め込みの標準ライブラリ（`stdlib::SOURCES`、`std/` の 18 ファイル、合計 50,853 bytes）が毎回含まれる。hello でも 19 ファイルを解析する。
- 型検査以降（`src/check.rs` の `check_modules_collect`）: モジュール名 → `computation::collect_all` → record・union → シグネチャ →
  関数本体のループ（関数ごとに `Checker::new(module, &names, types, &signatures, &classes)`。`Checker` は `Inference` を所有し、`&Names`・
  `TypeContext`・`&[Scheme]`・`&Classes` を借りる）→ entry → `polymorph::solve_members(&mut functions, &mut pending)`（関数をまたぐ）→
  `finish` のループ → `semantic::collect` → `constants::fold` → `recursion::check` → 警告を `(span.source, span.start)` で並べ替え →
  `ownership::infer_copy_all` → `polymorph::specialize` → `closures::lower` → `ownership::check_all` → `gpu::validate_calls`。
- `src/ownership.rs` の `check_functions(module, infer)` は `module.functions` を順に `check_body` へ渡す。`Checker` は関数ごとに作られ、
  `module: &CheckedModule` と `closed: &[bool]` を読むだけである。

### 診断の順序（コードで確認）

- `src/diagnostic.rs` の `Diagnostics` は `BTreeMap<DiagnosticKey, Diagnostic>` で、鍵は `(source, start, end, severity, code, message)`。
  表示の順は push の順によらず鍵の順である。
- ただし `MAX_UNIQUE_DIAGNOSTICS`（1000）に達すると `push` は捨て、各ループは `is_full` で break する。どの診断が入るかは push の順で
  決まるので、並列化しても push は入力順に再生する（D3）。`finish` は先頭の `MAX_REPORTED_ERRORS`（50）件を返す。

### スレッドと stack（コードで確認）

- `src/` に `thread::Builder`・`stack_size` の呼び出しはない（`src/llvm_frame.rs` の `stack_size` は生成コードの frame 計算で無関係）。
  `Cargo.toml` と `src/main.rs` にも stack の設定はない。CLI の解析はプロセスの main thread で動く。main thread の stack は OS の既定で、
  macOS は 8 MiB、Linux は `ulimit -s`（通常 8 MiB）、Windows（MSVC の既定）は 1 MiB である。
- Rust テストは test harness の thread（既定 2 MiB。`RUST_MIN_STACK` で変わる）で動く。debug build の stack-depth テストはこの 2 MiB で通る。
- 既存の thread の利用: `src/test_runner.rs`（上の手本）、`src/cache.rs` のテスト `cache_checks_content_partial_entries_races_and_eviction`
  （`std::thread::scope`）、`src/lsp.rs` の `serve`（入力の読み取りに `std::thread::spawn`）。
- `Sync` でないデータ: `src/check.rs` の `CheckedRecord`・`CheckedUnion` の `recursive: recursive::Cache`（`src/recursive.rs` で
  `std::cell::RefCell<BTreeMap<..>>`）。このため `CheckedModule` と `TypeContext` は `Sync` でなく、関数単位の並列化（Phase 2）には
  この cache の置き換えが要る。`src/derive.rs` の `nodes: Cell<usize>` と `src/exhaustiveness.rs` の `Rc`（`type Pat = Rc<Node>`）もある。
- 依存 crate は `rustc_apfloat`・`serde_json`・`same-file`（Windows）だけで、`[lints.rust] unsafe_code = "forbid"`。
- スレッド数を指定する CLI オプション・環境変数はない（`src/` の `TSUZURI_` は `CACHE_DIR`・`CLANG`・`WASM_LD`・`LLVM_LINK`・`DSYMUTIL` だけ）。

### 計測済みの事実（2026-09-29、`_perfs/README.md` と同じ値、Apple M1 Max 10 コア、各 1 回）

- 旧生成の合成 200 モジュール（24,803 行）: `check` 0.39 s、IR 出力 0.46 s、最大 RSS 237 MB。
- 旧生成の合成 1,000 モジュール（124,003 行）: `check` 2.12 s、IR 出力 2.58 s、最大 RSS 1.12 GB。IR 出力の間、main thread 以外に処理はなかった（サンプリング）。
- `parse` 段の割合は未計測（PB02 のサンプリングは確保関数の分類で、段別ではない）。手順 1 で PX01 の段別時間から記録する。
- 目標 G1（下記）に必要な並列部分: 10 コアで 1/5 にするには、逐次部分の割合 $s$ が $s + (1 - s)/10 \le 0.2$、すなわち $s \le 1/9 \approx 11\%$ でなければならない。
  Phase 1 は構文解析だけなので G1 に届かないことは起票時点で分かっている（G1 は Phase 2 までの目標）。

### 再現（2026-09-29 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -rn "thread::Builder\|stack_size(" src/ Cargo.toml | grep -v llvm_frame   # 0 件
grep -n "thread::scope\|available_parallelism" src/*.rs                         # cache.rs・test_runner.rs だけ
wc -c std/* | tail -n 1                                                         # 50853 total
grep -n "RefCell\|Cell<\|Rc<" src/recursive.rs src/derive.rs src/exhaustiveness.rs
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before（`TSUZURI_THREADS=1`）と after（スレッド数別）を記録して判断する。

- G1（Phase 2 までの目標。起票時の目標を保つ）: 10 コアで、PX03 の合成 1,000 モジュールの `check` を `TSUZURI_THREADS=1` の 1/5 以下にする。
  基準は PB02 の完了後の値。Phase 1 だけでは届かない（「計測済みの事実」の $s \le 11\%$）。
- G2（Phase 1）: 合成 1,000 モジュールの `parse` 段の壁時計時間を、10 スレッドで 1 スレッドの 1/4 以下にする。
- G3（Phase 1）: hello と合成 200 モジュールの `check` の時間を悪化させない（差が 9 標本の最小〜最大の範囲内）。
- G4（Phase 1）: 合成 1,000 モジュールの `check` の最大 RSS の増加を 10% 以内にする。

| 指標 | 単位・統計 | 対象 | 期待（計測で確かめる） |
| --- | --- | --- | --- |
| M1 構文解析の時間 | ms（PX01 の段 `parse`、9 標本の中央値・最小・最大） | 合成 200・1,000 モジュールの `check`、`TSUZURI_THREADS` = 1・2・4・8・10 | スレッド数とともに短くなる。1,000 モジュールで 10 スレッドが 1 スレッドの 1/4 以下（G2） |
| M2 全体の時間 | ms（`wall_time` と段 `total`、同上） | `check`（B3）と `emit-llvm`（B2）、同上 | `parse` の短縮分だけ短くなる。`check` 以降の段は変わらない |
| M3 スケーリング | 比（M1 の 1 スレッドの中央値 ÷ n スレッドの中央値） | M1 から計算。要約にだけ出す | 記録だけ。2 で 1.6 以上、10 で 4 以上を期待値とする |
| M4 CPU 時間 | ms（`user_time` + `sys_time`、中央値） | M2 と同じ | 増加は spawn と確保器の競合の分。記録して並列化の効率を見る |
| M5 最大 RSS | bytes（`peak_rss`、中央値） | `check` と `emit-llvm`、1・10 スレッド | 増加 10% 以内（G4） |
| M6 hello | ms（`wall_time`、中央値） | 1 ファイルの hello の `check`、1・10 スレッド | 変化なし（D5 の閾値で逐次の経路を通る。G3） |

## 変えてはいけない意味

- 診断: 集合・内容・順序・省略の注記（`DiagnosticSet` の `omitted` と `omitted_is_lower_bound`）を、スレッド数によらず同じにする。
  一意の診断が `MAX_UNIQUE_DIAGNOSTICS` に達する場合も同じにする（D3 の再生の規則）。`Span::source` は今と同じく `inputs` の添字である。
- 出力: 型付き IR、`--emit llvm` の IR、成果物、`header` の出力を byte 単位で同じにする。`TSUZURI_THREADS` は出力を変えないので、
  `src/cache.rs` の `build_key` の環境変数の一覧に入れない（入れると同じ成果物の cache が分かれる）。
- `TSUZURI_THREADS=1` は HEAD と同じ経路である。入力ごとに遅延して解析し、`is_full` で break した後の入力は解析しない（D2）。
- 上限: `MAX_NESTING`（128）、`MAX_SOURCE_BYTES`（1 MiB）、`E0001`〜`E0003` の条件を変えない。今の main thread で受理・拒否される
  入力は、worker でも stack overflow せず同じ結果になる（D4）。
- panic: 構文解析の panic（コンパイラの不具合）は、worker でも同じ payload で呼び出し側に伝わる（`std::panic::resume_unwind`）。
- 安全性と依存: `unsafe` を使わない。新しい crate を足さない（D1）。
- 生成コード: 生成する LLVM IR と実行時は変えない。WASM の import、数値の意味、評価順序、所有権の規則はこのチケットの対象外で、変わらない。

## 設計

### データ構造

`src/lib.rs` に置く。すべて新規で、公開 API は変えない（`pub(crate)` か private）。

```rust
/// Upper bound for explicit and default frontend threads.
const MAX_FRONTEND_THREADS: usize = 32;
/// Matches the default main-thread stack on macOS and Linux.
const WORKER_STACK_BYTES: usize = 8 << 20;
// ponytail: fixed threshold, measure per-file parse cost if tiny projects regress.
const PARALLEL_PARSE_MIN_BYTES: usize = 256 << 10;

type Parsed = Result<(String, syntax::Program), Vec<Diagnostic>>;

fn thread_setting(value: Option<&std::ffi::OsStr>, available: usize) -> Result<usize, Diagnostic>;
fn frontend_threads() -> Result<usize, Diagnostic>;
fn parse_input(id: usize, input: &SourceInput<'_>) -> Parsed;
fn parse_inputs(inputs: &[SourceInput<'_>], threads: usize) -> Vec<Parsed>;
pub(crate) fn analyze_inputs_threaded(
    inputs: &[SourceInput<'_>],
    semantic: Option<&mut check::semantic::SemanticIndex>,
    threads: usize,
) -> Result<check::CheckedModule, DiagnosticSet>;
```

- `parse_input` は HEAD の `analyze_inputs_indexed_all` の中の closure（モジュール名の決定、`parser::parse_with_source_all`、
  `program.source_kind` の設定）をそのまま移した関数である。本文は変えない。
- `parse_inputs` は並列の経路を選んだときだけ、全入力の結果を `id` の順に返す。逐次の経路では空の `Vec` を返す（D2）。
- `analyze_inputs_indexed_all(inputs, semantic)` は `frontend_threads()` の結果で `analyze_inputs_threaded` を呼ぶだけになる。
  Rust の単体テストは `analyze_inputs_threaded` にスレッド数を直接渡す（環境変数を書き換えない）。

### アルゴリズム

```rust
fn analyze_inputs_threaded(inputs, semantic, threads) {
    let mut parsed = parse_inputs(inputs, threads).into_iter();
    let mut programs = Vec::new();
    let mut diagnostics = Diagnostics::new(0);
    for (id, input) in inputs.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        // Parallel results arrive in id order; the sequential path parses lazily.
        match parsed.next().unwrap_or_else(|| parse_input(id, input)) {
            Ok(program) => programs.push(program),
            Err(errors) => diagnostics.extend(errors),
        }
    }
    // Remainder is HEAD's code unchanged: finish on errors, build ModuleInput, check_modules_indexed_all.
}

fn parse_inputs(inputs, threads) -> Vec<Parsed> {
    let bytes: usize = inputs.iter().map(|input| input.text.len()).sum();
    let workers = threads.min(inputs.len());
    if workers <= 1 || bytes < PARALLEL_PARSE_MIN_BYTES {
        return Vec::new();
    }
    let cursor = AtomicUsize::new(0);
    let work = || {
        let mut done = Vec::new();
        loop {
            let id = cursor.fetch_add(1, Ordering::Relaxed);
            let Some(input) = inputs.get(id) else { break };
            done.push((id, parse_input(id, input)));
        }
        done
    };
    let mut results = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..workers)
            .map_while(|_| std::thread::Builder::new()
                .stack_size(WORKER_STACK_BYTES)
                .spawn_scoped(scope, &work)
                .ok())
            .collect();
        let mut results = work();          // the caller also works
        for handle in handles {
            match handle.join() {
                Ok(done) => results.extend(done),
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
        results
    });
    results.sort_unstable_by_key(|(id, _)| *id);
    results.into_iter().map(|(_, parsed)| parsed).collect()
}
```

- 呼び出し側の thread も同じ loop で働く。spawn が失敗した（`io::Error`）ときは、それ以降の worker を作らず、残りを呼び出し側が処理する。
  結果は `id` で並べ替えるので、どの thread が何を処理しても出力は同じである。
- `id` は `fetch_add` の戻り値である。`Ordering::Relaxed` で足りる（結果の受け渡しは `join` が同期する）。
- 並列の経路では、`is_full` で break した後の入力も解析済みになる。結果は捨てるだけで、診断は同じ（D3）。

### スレッド数の決定（D2）

判定は純粋な関数 `thread_setting`（新規）に置き、`frontend_threads()` は
`thread_setting(std::env::var_os("TSUZURI_THREADS").as_deref(), available)` を返すだけにする（`available` は
`std::thread::available_parallelism().map_or(1, usize::from)`）。テストは `thread_setting` を直接呼ぶ。

| 値 | 結果 |
| --- | --- |
| 未設定、または空文字列 | `available.min(MAX_FRONTEND_THREADS)` |
| `1`〜`32` の 10 進整数（前後の空白は `trim` で除く） | その値 |
| それ以外（`0`、`33` 以上、数字以外、UTF-8 でない値） | `E2000`: `TSUZURI_THREADS must be an integer from 1 to 32; unset it to use the available cores`。span は `Span::default()` |

`analyze_inputs_indexed_all` は `frontend_threads().map_err(DiagnosticSet::from)?` で呼ぶ（`impl From<Diagnostic> for DiagnosticSet` は既存）。
実際に使う thread の数は `min(threads, inputs.len())` で、閾値（D5）未満なら 1 である。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 入口 | `src/lib.rs` | `analyze_inputs_indexed_all` | `frontend_threads` を呼び、`analyze_inputs_threaded`（新規）へ委ねる |
| 入口 | `src/lib.rs` | `analyze_inputs_threaded`（新規） | HEAD の本体。構文解析のループだけ「アルゴリズム」の形にする |
| 構文解析 | `src/lib.rs` | `parse_input`・`parse_inputs`・`Parsed`・定数 3 つ（新規） | 上のとおり。`use std::sync::atomic::{AtomicUsize, Ordering};` を足す |
| 設定 | `src/lib.rs` | `thread_setting`・`frontend_threads`（新規） | D2 の表 |
| CLI の説明 | `src/main.rs` | usage の環境変数の行（`TSUZURI_CLANG          Clang executable ...` の並び） | `TSUZURI_THREADS        Frontend threads (1-32; default: available cores, max 32)` を足す |
| 試験 | `src/lib.rs` | `#[cfg(test)] mod tests`（新規） | テスト計画の Rust テスト |
| 試験 | `tests/parallel_frontend.mjs`（新規） | — | テスト計画の E2E |
| cache | `src/cache.rs` | `build_key` の環境変数の一覧 | 変更なし（出力を変えない設定は鍵に入れない） |
| 読み込み | `src/driver.rs` | `Project::load_from_root`・`Project::analyze_all` | 変更なし |
| 字句・構文 | `src/lexer.rs`・`src/parser.rs` | — | 変更なし |
| 検査以降 | `src/check.rs`・`src/ownership.rs`・`src/polymorph.rs`・`src/closures.rs`・`src/llvm.rs` | — | Phase 1 では変更なし |
| LSP・test | `src/lsp.rs`・`src/test_runner.rs` | — | 変更なし（同じ入口を通る。`test_runner` の worker は解析の後に動くので入れ子にならない） |

### Phase 分割

- Phase 1（この実装）: 上の表のとおり。字句解析と構文解析だけを並列にする。
- Phase 2（設計方針。D8 の承認後だけ）。次の順に、1 段ずつ出力の一致を確かめながら進める。
  1. `Sync` 化: `src/recursive.rs` の `Cache` を `std::sync::Mutex<BTreeMap<..>>` にする。`CheckedRecord`・`CheckedUnion` の
     `#[derive(Clone)]` は `Mutex` を複製できないので、中身を複製する `Clone` を手で書く。cache は結果の memo なので、
     計算の順が変わっても値は同じである。
  2. 所有権検査: `ownership::check_functions` の関数ごとの `check_body` を、`module.functions` の添字を cursor にして並列に実行し、
     結果（`Result<BTreeSet<String>, Diagnostic>`）を添字の順に並べてから、HEAD と同じ loop（`is_full` の break を含む）で再生する。
     `infer_copy_all` と `check_all` の両方に効く。
  3. 本体の型検査: `check_modules_collect` の本体のループ。`Checker`（`pending` に残す）が `Send` であること、`Names`・`Classes`・
     `[Scheme]`・`TypeContext` が `Sync` であることが前提である（`src/exhaustiveness.rs` の `Rc`、`src/derive.rs` の `Cell` の扱いを先に調べる）。
     `computation::expand(&mut function.body, &names)` は宣言ごとの可変借用なので、`function_declarations` を `chunks_mut` で分けて渡す。
     結果は宣言の添字の順に再生する。`solve_members` と `finish` の loop は逐次のまま。
- 並列化しない段（D9）: 読み込み、名前・シグネチャの収集、`solve_members`、`constants::fold`、`recursion::check`、`polymorph::specialize`
  （worklist と 1,024 の上限の厳密な判定が順序に依存する）、`closures::lower`、`llvm::emit`（文字列定数・metadata・トラップ地点の番号が
  出力順で決まる。LLVM 側の並列化は PB07・G17）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: 基準のコンパイラを保存し、stack と上限の 5 テスト、例の IR を記録する。
- 確認: 5 テストがそれぞれ `1 passed`。`cargo test --locked` が成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p target/perf/PB04/before-ir && cp target/release/tsuzuri target/perf/PB04/baseline-tsuzuri
for d in examples/*/; do n=$(basename "$d"); target/perf/PB04/baseline-tsuzuri build "$d" --emit llvm -o "target/perf/PB04/before-ir/$n.ll" --no-cache; done
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test tasks bounds_nested_task_syntax_and_types
```

### 手順 2: `parse_input` の切り出し（挙動は不変）

- 変更: `src/lib.rs` の `analyze_inputs_indexed_all`、`parse_input`・`Parsed`（新規）。
- 内容: HEAD の closure を本文を変えずに `parse_input(id, input)` へ移し、ループから呼ぶ。
- 確認: `cargo test --locked` が成功する。手順 1 と同じ loop で `/tmp/tz-pb04/ir/` に出した IR が `cmp` で `before-ir` と一致する。

### 手順 3: スレッド数の設定と逐次の経路

- 変更: `src/lib.rs` の `thread_setting`・`frontend_threads`・`analyze_inputs_threaded`・定数 3 つ（新規）、`#[cfg(test)] mod tests`（新規）。
- 内容: D2 の表を実装する。`parse_inputs` はまだ常に空の `Vec` を返す。テスト `thread_setting_accepts_1_to_32_and_rejects_others` を足す。
- 確認: `cargo test --locked --lib thread_setting` が `1 passed`。`TSUZURI_THREADS=0 target/release/tsuzuri check examples/hello`
  （`cargo build --release --locked` の後）が失敗し、stderr に `error[E2000]: TSUZURI_THREADS must be an integer from 1 to 32` を含む。

### 手順 4: 並列の経路

- 変更: `src/lib.rs` の `parse_inputs`。
- 内容: 「アルゴリズム」のとおり。テスト計画の Rust テストの残り 4 つを足す。
- 確認: `cargo test --locked --lib parallel_` が `3 passed`、`cargo test --locked --lib parse_inputs_skips_small_projects` が `1 passed`。

### 手順 5: 全体と両方の経路

- 変更: なし。
- 内容: 既定、1、8 スレッドで全テストを実行する。環境変数は `cargo` のプロセスに渡すだけで、テストの中では書き換えない。
- 確認: 次の 3 つが成功し、手順 1 の 5 テストも成功する。

```sh
cargo test --locked
TSUZURI_THREADS=1 cargo test --locked
TSUZURI_THREADS=8 cargo test --locked
```

### 手順 6: CLI の説明

- 変更: `src/main.rs` の usage。
- 内容: `TSUZURI_WASM_LD        WebAssembly linker (default: wasm-ld)` の次に `TSUZURI_THREADS` の行を足す（桁をそろえる）。
- 確認: `grep -n "TSUZURI_THREADS" src/main.rs` が 1 件。`cargo test --locked --bin tsuzuri` が成功する。

### 手順 7: E2E `tests/parallel_frontend.mjs`

- 変更: `tests/parallel_frontend.mjs`（新規）。
- 内容: テスト計画の E2E。生成するソースの形は、書く前に `/tmp/tz-pb04/probe/` の小さなプロジェクトで
  `target/release/tsuzuri check`・`run` して確かめる（落とし穴の 1 番目）。
- 確認: `cargo build --release --locked && node tests/parallel_frontend.mjs target/release/tsuzuri` が成功し、最後に
  `parallel frontend: 4 projects x 4 thread settings identical` を出す。

### 手順 8: 既存の E2E を両方の経路で

- 変更: なし。
- 確認: 次がすべて成功する。WASM を含む suite で Node 20 が落ちる場合は Node 24（`npx --yes --package=node@24 node ...`）で実行する。

```sh
TSUZURI_THREADS=1 node tests/features.mjs target/release/tsuzuri
TSUZURI_THREADS=8 node tests/features.mjs target/release/tsuzuri
TSUZURI_THREADS=8 node tests/cache.mjs target/release/tsuzuri
for d in examples/*/; do n=$(basename "$d"); TSUZURI_THREADS=8 target/release/tsuzuri build "$d" --emit llvm -o "/tmp/tz-pb04/ir/$n.ll" --no-cache; cmp "/tmp/tz-pb04/ir/$n.ll" "target/perf/PB04/before-ir/$n.ll"; done
```

### 手順 9: 計測（PX03 が done のときだけ）

- 内容: 「計測手順」のとおり。停止条件 6・7 をここで判定する。
- 確認: `target/perf/<run_id>/` に `t1`〜`t10` と `baseline` の `build.jsonl` があり、`benchmarks/metrics.mjs report` の表が出る。

### 手順 10: 生成コードと文書

- 内容: 「生成コードの確認」と「ドキュメント」のとおり。
- 確認: `node scripts/check-docs.mjs README.md docs/architecture.md docs/benchmarks.md` が成功する。

### 手順 11: 最終確認

- 確認: `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`、手順 1 の 5 テスト、
  `node tests/parallel_frontend.mjs target/release/tsuzuri` がすべて成功する。`cargo tree --locked --depth 1` の出力が HEAD と同じ（crate が増えていない）。

## 計測手順

GUIDE §14 と PX03 の「計測手順」に従う。PX01 形式（JSON Lines、`target/perf/<run_id>/build.jsonl`）で、各 variant は warm-up 1 回を除く
9 標本の中央値・最小・最大を使う。計測中はほかの重い処理を止め、電源に接続する。環境（CPU、OS、Clang・rustc・Node の版、commit）は
PX01 の `captureRun` が記録する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
RUN=target/perf/$(date +%Y%m%d-%H%M%S)-PB04
TSUZURI_THREADS=1 npx --yes --package=node@24 node benchmarks/run-build.mjs target/perf/PB04/baseline-tsuzuri --out "$RUN/baseline" --sizes 200,1000
for n in 1 2 4 8 10; do
  TSUZURI_THREADS=$n npx --yes --package=node@24 node benchmarks/run-build.mjs target/release/tsuzuri --out "$RUN/t$n" --sizes 200,1000
done
npx --yes --package=node@24 node benchmarks/metrics.mjs report "$RUN/t10" --baseline "$RUN/t1" > "$RUN/report.txt"
npx --yes --package=node@24 node benchmarks/metrics.mjs report "$RUN/t1" --baseline "$RUN/baseline" > "$RUN/report-baseline.txt"
```

- runner は `process.env` を子プロセスへ渡す（PX03 の「操作（variant）」）。`TSUZURI_THREADS` の値は run ごとに固定する。
- M1 は各標本の段 `parse`、M2 は `wall_time` と段 `total`、M4 は `user_time` + `sys_time`、M5 は `check`・`emit-llvm` の `peak_rss`。
  M3 は M1 の中央値の比で、`report.txt` の横に手で表にする。
- M6（hello）: runner が `examples/hello` を測らない場合は、次で 9 回測り `$RUN/hello-t1.txt`・`$RUN/hello-t10.txt` に保存し、
  要約にだけ使う（PX01 形式ではない補助の値と明記する）。

```sh
for n in 1 10; do for i in 1 2 3 4 5 6 7 8 9; do TSUZURI_THREADS=$n /usr/bin/time -l target/release/tsuzuri check examples/hello 2>&1 | grep -E "real|maximum resident"; done > "$RUN/hello-t$n.txt"; done
```

- `t1` と `baseline` の差が広がりの範囲内であること（逐次の経路が HEAD と同じ速さ）を先に確かめる。外れたら原因を調べ、報告する。
- 記録: `docs/benchmarks.md` の `## ビルド速度`（PX03 で新規）に PB04 の小節を足し、run_id・commit・日付・host、M1〜M6 の表、
  そろえられなかった条件を書く。

## 生成コードの確認

PB04 は生成コードを変えない。確認するのは「出力が同じこと」と「実際に複数の thread で解析していること」である。

- 出力: 手順 8 の `cmp` と、PX03 の生成器で作った 1,000 モジュールの `--emit llvm`（`-O0`・`-O3`）を `TSUZURI_THREADS=1` と `10` で出し、
  `cmp` で一致すること。

```sh
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tz-pb04/g1000
for n in 1 10; do for o in -O0 -O3; do TSUZURI_THREADS=$n target/release/tsuzuri build /tmp/tz-pb04/g1000 --emit llvm $o --no-cache -o /tmp/tz-pb04/g1000-t$n$o.ll; done; done
cmp /tmp/tz-pb04/g1000-t1-O0.ll /tmp/tz-pb04/g1000-t10-O0.ll && cmp /tmp/tz-pb04/g1000-t1-O3.ll /tmp/tz-pb04/g1000-t10-O3.ll
```

- 並列の実行: symbol 付きで build し（`CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked`）、`TSUZURI_THREADS=10` の `check` を
  `sample <pid> 1 -f /tmp/tz-pb04/sample.txt` で採る。`parse_input` を含む stack が main thread 以外の 2 つ以上の thread に現れること。
  `TSUZURI_THREADS=1` では main thread だけに現れること。
- 依存: `cargo tree --locked --depth 1` に新しい crate がないこと。`grep -n "TSUZURI_THREADS" src/cache.rs` が 0 件であること。

## テスト計画

### Rust テスト（`src/lib.rs` の `#[cfg(test)] mod tests`、新規）

ソースは `format!` で生成し、`SourceInput`（User）の列に `stdlib::SOURCES`（Std）を続けて `analyze_inputs_threaded` に渡す
（`analyze_modules_with_std_all` と同じ並び）。IR は `tests/polymorphism.rs` の `accepts` と同じ呼び方で `llvm::emit` から得る。期待値は
「スレッド数 1 の結果との一致」と、生成規則から数えた値（件数・source の添字）である。

- `thread_setting_accepts_1_to_32_and_rejects_others`: `available` = 10 で `None` → 10、`Some("")` → 10、`available` = 64 で `None` → 32、
  `"1"` → 1、`" 8 "` → 8、`"32"` → 32。`"0"`・`"33"`・`"-1"`・`"abc"`・`"1.5"` は `Err` で、`code == "E2000"` と D2 のメッセージに一致。
- `parallel_parse_matches_sequential_outputs`: 48 個のモジュール（各 8 KiB 以上、合計が `PARALLEL_PARSE_MIN_BYTES` を超える）と、それらを
  呼ぶ `Main`。`parse_inputs(&inputs, 8).len() == inputs.len()`（並列の経路）と `parse_inputs(&inputs, 1).is_empty()` を確かめ、
  スレッド数 1・2・8・32 の IR が一致して空でないこと。
- `parallel_parse_replays_the_diagnostic_cap`: 1,100 個のモジュールの各末尾に構文エラーを一つ置く（合計が閾値を超えるよう各ファイルを
  埋める）。スレッド数 1・2・8 の `DiagnosticSet` が `==` で一致し、`omitted_is_lower_bound` が true、`diagnostics.len() == 50`、
  全件の code が `E0002`、`diagnostics[0].span.source == Some(0)`。
- `parallel_parse_keeps_depth_results`: 40 個の埋め草のモジュールに、`(` を 100 重にした式を持つモジュール（受理）と 200 重（拒否）を
  加えた 2 つの入力で、スレッド数 1 と 8 の結果（IR か `DiagnosticSet`）が一致する。debug build の test thread（2 MiB）で実行される。
- `parse_inputs_skips_small_projects`: `Main` 1 つと std だけの入力で `parse_inputs(&inputs, 8).is_empty()`（D5）。

### E2E（`tests/parallel_frontend.mjs`、新規）

`node tests/parallel_frontend.mjs <compiler>`。`tests/cache.mjs` と同じく `mkdtempSync(join(tmpdir(), "tsuzuri-parallel-frontend-"))` の
一時 root を作り、プロジェクトごとに別のディレクトリにする。生成器はこのファイルの中に書く（PX03 に依存しない）。

| プロジェクト | 内容 | 期待（独立に決まる値） |
| --- | --- | --- |
| `ok` | 300 モジュール × 30 関数（合計 256 KiB 超）と、いくつかの関数の結果を出力する `Main` | `run` の出力が生成器の JavaScript（`BigInt`）で計算した値と一致 |
| `syntax` | 1,100 モジュールの各末尾に構文エラー | 終了 code が 0 でない。stderr が `E0002` を含み、省略の注記（`more errors not shown`）がある |
| `types` | 200 モジュールのうち 10 個おきに型エラー | 終了 code が 0 でない |
| `small` | `Main` だけ | `run` の出力が生成器の値と一致 |

- 各プロジェクトで `TSUZURI_THREADS` を `1`・`2`・`8`・未設定の 4 通りにし、`check` の stdout・stderr・終了 code が 4 通りで一致する。
- 成功するプロジェクトでは、`build --emit llvm` の `-O0`・`-O3` と、`tests/features.mjs` と同じ指定の wasm32 の `--emit llvm` `-O0`・`-O3` の
  IR が 4 通りで byte 単位で一致し、native `-O0` の実行ファイルの出力が期待値と一致する。すべて `--no-cache`。
- 不正な値: `0`・`33`・`abc` で `check` が失敗し、stderr が
  `TSUZURI_THREADS must be an integer from 1 to 32; unset it to use the available cores` を含む。`" 4 "` は成功する。
- 最後に `parallel frontend: 4 projects x 4 thread settings identical` を出す。

### 既存テストへの影響

なし。std と小さな入力は閾値（D5）未満なので逐次の経路を通る。大きな入力を生成するテストは並列の経路を通りうるが、出力は同じである。

### 性能

「計測手順」のとおり。CI に時間の合否条件を足さない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `README.md` | `## CLI`（`TSUZURI_CACHE_DIR` を説明する段落の後） | `TSUZURI_THREADS` の意味、既定、範囲、`1` で逐次、出力は変わらないこと |
| `src/main.rs` | usage の環境変数 | 手順 6 |
| `docs/architecture.md` | `## 不変条件` | 構文解析はファイル単位で並列に実行し、入力順に結合する。診断・IR はスレッド数によらず同一 |
| `docs/architecture.md` | `## 性能設計の原則` | worker の stack（8 MiB）、閾値、並列化しない段と理由（D9） |
| `docs/benchmarks.md` | `## ビルド速度` の PB04 の小節 | 計測手順の記録 |
| `_perfs/README.md` | 一覧の PB04 の状態欄 | Phase 1 の完了時に done（Phase 2 は別に起票するか、このチケットの D8 を更新する） |

## 受け入れ条件

- [ ] 手順 1〜8 と 10〜11 の確認がすべて成功した。
- [ ] スレッド数 1・2・8・未設定で、診断（上限に達する場合を含む）・IR（native と wasm32、`-O0`・`-O3`）・実行結果が一致する（Rust テストと E2E）。
- [ ] `TSUZURI_THREADS=1` が HEAD と同じ経路で、`t1` の計測値が基準のコンパイラと広がりの範囲内で一致する。
- [ ] 不正な `TSUZURI_THREADS` が `E2000` になり、`src/cache.rs` の鍵に入っていない。
- [ ] stack と上限の 5 テストが成功し、`MAX_NESTING`・stack・上限の値を変えていない。
- [ ] `unsafe` と新しい crate を使っていない。
- [ ] PX03 が done なら、M1〜M6 を記録し、G2〜G4 の達成・未達を実測値で書いた（未達も記録する）。PX03 が未了なら、その旨を報告した。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 生成するソースの構文: 起票時の probe で、`pub fn f(...)` は `E0002`（`expected the end of the file; declarations must precede the entry-point code`）、
  関数本体の `let y = ...` の後に `;` がないと `E0002`（`expected ';' after the let binding`）になった。既存の fixture か `_docs/` の例の形を写し、
  `check` で確かめてから生成器に入れる。
- `std::thread::scope` の `scope.spawn` は stack の大きさを指定できない（既定 2 MiB）。必ず `std::thread::Builder::new().stack_size(WORKER_STACK_BYTES).spawn_scoped(scope, ..)` を使う。
- テストで `std::env::set_var` を使わない（edition 2024 では `unsafe`、しかも並列のテストが競合する）。`thread_setting` と `analyze_inputs_threaded` を直接呼ぶ。
- `is_full` の break を並列の結合で落とすと、1,000 件を超える場合だけ診断が変わる。`parallel_parse_replays_the_diagnostic_cap` で必ず確かめる。
- 呼び出し側の thread も解析するので、その stack は今と同じ（CLI は main thread、テストは 2 MiB）。worker だけに重い入力が回る前提を置かない。
- worker の panic を `join().unwrap()` で包むと payload が変わる。`resume_unwind` で元の payload を伝える。
- Rust のテストは並列に動くので、大きな入力のテストが同時に 32 thread ずつ作ることがある。遅くなるだけで結果は同じ。共有の thread pool を作らない（対象外）。
- `TSUZURI_THREADS` を `build_key` に入れると、スレッド数ごとに cache が分かれる。入れない。
- 最大 RSS: 8 MiB の stack は予約だけで、触った page だけが RSS に入る。M5 の増加が大きいときは確保器の thread ごとの領域を疑い、数値を添えて報告する。
- E2E の各プロジェクトは別の一時 root に置く（E03 の再帰的なソース探索）。
- `cargo test --locked <filter>` は 0 件でも成功する。`running N tests` を見る。

## 対象外

- LLVM 側の並列コード生成（PB07・G17）、複数マシンでの分散ビルド、常駐サーバー（PB06）。
- ファイルの読み込みの並列化、`--jobs` の CLI オプション（D2）、`test_runner` と共有する thread pool。
- 単相化・closure lowering・IR 生成の並列化（D9）。Phase 2（D8）は承認までこのチケットの実装に含めない。

## 決定事項

### D1: 並列化の手段

- 決定: `std::thread::scope` と `std::thread::Builder::spawn_scoped` だけを使う。`rayon` などの crate は使わない。
- 理由: Phase 1 の仕事はファイル数個〜千個の独立な処理で、`AtomicUsize` の cursor で十分に均せる。crate の追加は承認が要る（GUIDE §1 の規則 10）。
  手本の `src/test_runner.rs` と同じ形になる。
- 状態: 既定案（実装者はこの案に従う）

### D2: スレッド数の指定と既定

- 決定: 環境変数 `TSUZURI_THREADS`（新規）だけで指定する。未設定・空は `available_parallelism` と 32 の小さい方、`1`〜`32` はその値、
  それ以外は `E2000`。`1` は HEAD と同じ遅延・逐次の経路。`--jobs N` は Phase 1 では足さない。
- 理由: 出力を変えない調整なので、計測とテストに要るのは環境変数だけである。CLI オプションはすべての subcommand の引数解析と usage に
  触れる。上限 32 は `src/test_runner.rs` と同じで、8 MiB の stack の予約を抑える。
- 見直し提案: 旧版の未決事項は「`--jobs N` と環境変数」だった。`--jobs` が必要になったら、同じ `thread_setting` を CLI から呼ぶ形で足す。
- 状態: 2026-10-16 に改訂（自動の既定は CLI だけ `min(コア数, 8)`、library は 1。「着手時の再検討」の D2）

### D3: 決定性

- 決定: 結果は source の添字で並べ替え、HEAD と同じループ（`is_full` の break を含む）で `Diagnostics` に push し直す。
  診断の並びは今と同じく `Diagnostics` の鍵の順である。
- 理由: どの診断が上限内に入るかは push の順で決まるので、添字順の再生だけが HEAD と同じ集合を保証する。
- 状態: 既定案（実装者はこの案に従う）

### D4: worker の stack

- 決定: `WORKER_STACK_BYTES` = 8 MiB（`8 << 20`）。呼び出し側の thread も同じ loop で働き、spawn の失敗時は残りを引き受ける。
- 理由: CLI の解析が今動く main thread（macOS・Linux の既定 8 MiB）と同じ大きさで、stack-depth テストが通る 2 MiB の 4 倍ある。
  予約だけで RSS はほぼ増えない。Windows の main thread（1 MiB）より大きいので後退しない。
- 状態: 2026-10-16 に改訂（worker が 2 つ以上のとき呼び出し側は作業しない。「着手時の再検討」の D4）

### D5: 並列にする閾値

- 決定: 入力が 2 つ以上で、入力の合計が `PARALLEL_PARSE_MIN_BYTES` = 256 KiB 以上のときだけ並列にする。手順 9 の結果で 1 回だけ見直してよい
  （値を変えたら理由と計測値を `docs/benchmarks.md` に書く）。
- 理由: std（50,853 bytes）と小さなプロジェクトでは thread の生成の固定費が上回る見込みで、hello を悪化させない（G3）。
- 状態: 2026-10-16 に改訂（段ごとの仕事量で worker 数を決める。「着手時の再検討」の D5）

### D6: 仕事の配り方

- 決定: source の添字の順に `AtomicUsize` の cursor で 1 ファイルずつ取る。大きさ順の並べ替えはしない。
- 理由: 1 ファイルは 1 MiB 以下（`MAX_SOURCE_BYTES`）で、動的な配布で偏りが残るのは最後の 1 ファイル分だけである。
- 状態: 2026-10-16 に改訂（関数は 8 件ずつ、parse は大きい file から。「着手時の再検討」の D6）

### D7: 計測での段の扱い

- 決定: PX01 の段 `parse` は並列化後も壁時計の時間とし、段の名前を増やさない。cache の鍵は変えない。
- 理由: PX03 の B7 の和（`frontend`）が壁時計の時間でなければ `total` と整合しない。
- 状態: 既定案（実装者はこの案に従う）

### D8: Phase 2（本体の型検査と所有権検査）

- 決定: 「Phase 分割」の順（`recursive::Cache` の `Mutex` 化 → 所有権検査 → 本体の型検査）で進める。
- 理由: `CheckedModule`・`TypeContext` の `Sync` 化は共有の型の変更で、`Checker` の `Send` 化には `src/exhaustiveness.rs`・`src/derive.rs` の
  調査が要る。G1 の達成にはこの段が必要である。
- 状態: 承認済み（2026-10-16 の全フェーズ実装依頼。順序は「着手時の再検討」の D8）

### D9: 並列化しない段

- 決定: 読み込み、名前・シグネチャの収集、`solve_members`、`constants::fold`、`recursion::check`、`polymorph::specialize`、`closures::lower`、
  `llvm::emit` は逐次のままにする。
- 理由: 単相化は worklist と特殊化の上限（`honors_the_exact_specialization_limit`）の判定が順序に依存し、IR 生成は文字列定数・metadata・
  トラップ地点の番号を出力順で決める。LLVM 側の並列化は PB07 の codegen unit の担当である。
- 状態: 既定案（実装者はこの案に従う）
