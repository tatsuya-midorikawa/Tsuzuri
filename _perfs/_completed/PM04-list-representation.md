# PM04: 連結リストの連続表現

| 項目 | 内容 |
| --- | --- |
| ID | PM04 |
| 分類 | メモリ |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | C07, PM05 |
| 状態 | done |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-17 に HEAD 9dd0fbbc で再検討（「着手時の再検討」） |
| 承認 | 2026-10-17 の全フェーズ実装依頼で D1（記憶域を chunk 列へ変え、docs/language.md の記憶域・計算量の記述を変える）と、改めた D3・D7・D10・D11、新設の D12〜D15、Phase 2（連続領域の高速経路）を承認済み（測定で効果を確認できない既定の切り替えは行わない。D14） |
| 手本にする既存実装 | 置き換える経路: `src/llvm.rs` の `list_node_type`・`list_element_pointer`・`list_builder`・`append_list`・`finish_list`・`list_loop`・`list_loop_control`。連続領域の手本: `src/llvm.rs` の `allocate_array`・`element_pointer`・`allocation_size`・`array_loop`、`src/llvm_frame.rs` の `frame_array` と、`heap_copy`・`drop_frame_contents` の `Frame::Array` の arm。反復 drop の手本: `src/llvm.rs` の `drop_value` の `Type::List` の arm（次を読んでから free する） |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_bulk.rs`, `src/llvm_control.rs`, `src/llvm_frame.rs`, `src/llvm_compare.rs`, `src/llvm_display.rs`, `src/llvm_hash.rs`, `tests/lists.rs`, `tests/primitives.mjs`, `tests/features.mjs`, `tests/fixtures/list_chunks/Main.tz`（新規）, `benchmarks/list-memory/Main.tz`（新規）, `benchmarks/run-list-memory.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_docs/library-reference/arrays-and-lists.md`, `_docs/language-reference/ownership.md`, `_docs/language-reference/patterns.md`, `_docs/language-reference/types.md`。着手時に足したもの（「着手時の再検討」。`_docs/` は `_tsuzuri/language-reference/` に読み替える）: `src/llvm_debug.rs`, `scripts/lldb/tsuzuri_lldb.py`, `src/copies.rs`, `tests/storage.rs`, `tests/integer_ranges.rs`, `tests/debug_info.rs`, `tests/debug_info.mjs`, `tests/debugger.mjs`, `tests/copy_cost.rs`, `benchmarks/list-memory/host.c`（新規）, `_tsuzuri/language-reference/` の `list.md`・`stack-and-heap.md`・`array.md`・`vec.md`・`for-in.md`・`compiler/debugging.md` |
| 計測対象 | `benchmarks/list-memory`（新規、実装後は 12 種目）の `alloc_calls`・`alloc_bytes`・`wall_time`・`peak_rss`、`control/list_sum` の `wall_time`、`tests/primitives.mjs` の確保回数（決定的） |

## 目的

リスト `[|T|]` を、要素ごとのノードではなく「要素を連続して並べた chunk の連結列」で表す。`new`・リテラル・`Array.to_list`・複製など
一括で作るリストは chunk 一つ（配列と同じ連続領域）になり、`List.cons` で伸ばすリストは容量を倍々にした chunk を足していく。
1 要素あたりの記憶量と確保回数を減らし、走査・索引・比較を連続領域の処理にする。

Tsuzuri のリストは一意に所有され、複製は深く、ノードのリンクは公開しない（docs/language.md「配列・連結リストとレコード」）。
先頭の chunk を書き換えられるのは所有者だけなので、構造を共有しなくても観測できる意味を保ったまま表現を変えられる。

実装者は Phase 1 だけを実装する。Phase 2（設計の「Phase 分割」）は人間が求めた場合だけ着手する。

## 着手時の再検討（2026-10-17）

2026-10-17 の依頼で、全フェーズ（Phase 1・Phase 2、D1〜D11 と下の D12〜D15 を含む）を実装する。以下の「着手条件と停止条件」「現状と計測（HEAD `f8dc655`）」「設計」「実装手順」「決定事項」は
起票時の記録で、このセクションと食い違う箇所はこのセクションが優先する（旧 `_docs/` のパスは GUIDE §8.1 で `_tsuzuri/language-reference/` に読み替える）。
着手時の HEAD は `a2313d19`（main `9dd0fbbc` + 第4フェーズの計画）。偵察は main `9dd0fbbc` の release コンパイラ（SHA-256 `5e1b9687…`。HEAD から build した release と byte 単位で同一）、
Apple M1 Max、macOS 27.0.1、Apple clang 21.0.0、rustc 1.98.1、Node 20.19.6、1 分の load average 6〜27（ほかの作業と共有）で行った。時間は共有機の参考値で、正式な前後比較は統合後に行う。
生データと偵察用の scratch は `target/perf/PM04/`・`target/perf/PM04-before/`（git の管理外）。

### 起票時の記述と現行のコード

| 起票時 | 現行 | 影響 |
| --- | --- | --- |
| 依存 PX01 は todo。PX01 の interface を前提にする | PX01〜PX03 は done。`benchmarks/metrics.mjs` に `instrumentAllocations`・`makeRecord(context, { suite, workload, size, language, variant, opt, cpu_mode, metric, samples })`・`writeRecords`・`captureRun`・`measureProcess`・`compareRuns`・`report --baseline`、metric `alloc_calls`・`alloc_bytes`・`wall_time`・`peak_rss` と `benchmarks/tracked_alloc.c` が実在する。`measureProcess` は `os.tmpdir()` に時間の報告を書く | 着手条件は満たす。runner は `TMPDIR` を worktree の下へ向けて動かす |
| `%tz.list`・`stack_size`・`storage_layout` は 16 bytes。`src/llvm_debug.rs` は「変更なし」 | 記述子は同じ（`layout_bound` の `Type::List` は (16, 8)）。ただし `llvm_debug.rs` の `list_node` は `head` を `<list>.node { next, value }` へのポインターとして書き、`scripts/lldb/tsuzuri_lldb.py` の `ListProvider` は `next` を辿って `value` を読む（G16）。`tests/debug_info.rs`（`[\|i64\|].node` の `value` offset 64）・`tests/debug_info.mjs`・`tests/debugger.mjs`（`chain = length=2 {…}` と garbage の probe）・`docs/architecture.md` の DWARF の節・`compiler/debugging.md` が連動する。natvis は list の view を持たず（メンバーのまま）、`vsc/` に list 固有の code はない | ticket の「変更なし」は誤り。DWARF・LLDB provider・3 つのテスト・2 つの文書を変える（D12） |
| `List.cons`・`List.tail` は `emit_typed_builtin` の arm | builtin ごとの関数 `@tz.builtin.List.cons.<T>`・`List.tail.<T>`（`src/llvm.rs` の builtin 本体の `Builtin::ListCons`・`ListTail`）を wrapper `@tz.fn.$builtin.List.cons.N` が呼ぶ。wrapper は移動済みの引数の slot を drop する loop を持つ（`-O0` では長さ 0 を見て抜ける）。`-O3` では inline される | `list_push_front`・`list_pop_front` は builtin 本体に置く。wrapper の drop は chunk の外側 loop が null を見て抜ける形になる |
| 再現: `@tz.alloc` の呼び出し 7 箇所、node の GEP 22 箇所（`grep -c "call ptr @tz.alloc"`） | 同じ sample で `@tz.alloc(` は 5 箇所（リスト関連は 3: `List.cons` 本体・`new` の loop・clone の loop。PR01 が使用箇所に `nonnull` を付けるので `call nonnull ptr @tz.alloc(`）、node の `getelementptr inbounds { ptr, i64 }` は 18、サイズ式の `getelementptr ({ ptr, i64 }, ptr null, i32 1)` は 5。`run` は 99444・9 で一致 | 数え方を `grep -c "@tz.alloc("` と型付き GEP の別計数にする。`--emit llvm` の IR は `-O0` と `-O3` で同一（最適化は clang の段）なので「`-O3` の IR」は `clang -O3 -S -emit-llvm` で作る。sample は `-O3` で定数 99444 に畳まれる（`-O3` の証拠は list-memory の export で取る） |
| D7: 比較 helper の IR は `phi ptr` がちょうど 2 つ、`@tz.alloc`・`@tz.list.index` なし（features の typeclasses `inspect`） | 同じ検査が実在する。加えて PR02 が list の loop に pin を足した: `tests/integer_ranges.rs` の `STRUCTURAL`（`each_list` の算術が `add i64 %, %` と `add nuw i64 %, 1` だけ）・`assert_index_loop`（添字の phi が先頭、増分の block が header へ戻る）・`structural_comparison_loops_count_with_an_unsigned_index`（`LIST_COMPARE`）・audit の件数 | 下の一覧で先に declare する。loop の増分は `increment_block`・`audit_increment` の規約を守る |
| 既存テストへの影響は `lists.rs`・`primitives.mjs`・features の 3 か所 | 加えて `tests/storage.rs`（`alloca [2 x { ptr, i64 }]`・`[2 x { ptr, %tz.string }]` の検査）、`tests/copy_cost.rs`・`src/copies.rs`・`ownership-and-memory/ownership.md` の W1006 の文言「allocates a new node for every element」 | 一覧に足す（D12・D13） |
| D3: `K = 32768 / size`（要素領域）、容量は 1 から倍々 | macOS の `malloc` は 1 KiB を超えると丸めが粗い（1,040→1,280、4,112→5,120、16,400→20,480、32,776〜→49,152。実測）。K = 4,096 の i64 の chunk は header 込み 32,792 bytes で 49,152 に丸まり、2 のべきの容量は 1 KiB 以上で約 23〜25% を捨てる。WASM の heap は block = max(32, (n + 31) & ~15)（16 bytes の header。PM05）、`--allocator small` は 512 bytes まで 16 bytes 刻みで header なし | モデルと実測で D3 を改める（D3'）。M1 の `alloc_bytes` は要求量で丸めを見ないので、M4（`peak_rss`）と footprint のモデルで見る |
| D7: 分岐なしの `select` で chunk を進める flat loop（「loop の形を HEAD に近く保つ」） | C の試作（clang -O3、i64 を 1,000,000 個足す）で、node の連結 1.00〜1.06 ns/要素に対し select の cursor は 1.87〜1.98（1.9 倍遅い）、同じ cursor を分岐にすると 0.53〜0.55、chunk ごとの二重 loop は 0.10〜0.13（配列 0.13〜0.15 と同じ。ベクトル化） | D7 を改める（D7'）。Phase 1 は分岐の cursor、Phase 2 は二重 loop |
| WASM の確保量は計測しない（計算値） | wasm32 の module で heap の高水位を測れる（`memory.buffer`）。node の list は 100,000 個で +3.1 MB（32 bytes/要素）、400,000 個で +12.8 MB、600,000 個で OOM trap（既定 16 MiB の heap）。chunk は 8 bytes/要素なので約 1.9M 要素まで通る見込み | WASM を実測に加え、上限の拡大を文書に書く |
| workload `cons_build` は `values = List.cons i values` の loop と総和 | 同じ関数が `for value in values` でも使うと、A17 の last-use 解析が `ForEach` の source の局所変数を丸ごと除外し、loop の `List.cons i values` が毎回 `values` を複製する（`@tz.alloc` は n = 1,000 で 500,500 回、n = 16,000 で 2.8 s。`--warn implicit-copy` が W1006 を出す）。cons の loop を別関数 `build` に出すと n 回 | PM04 の外の既知の制約（完了報告に書く）。workload は `build` を関数に分け、表現の効果と混ぜない |
| `control/list_sum` は Tsuzuri が最速。参照は node ごとの確保 | 参照（`benchmarks/control/reference.{c,rs}`、C++ は同じ C）は node ごとの `malloc` の連結リスト。今日の 100,000 個（load 11〜14、12 標本）は Tsuzuri 1.826 ms、C 1.869、C++ 1.872、Rust 1.872。`--allocator small` は 0.405 倍（docs/benchmarks.md）。確保は 100,000 回・1,600,000 bytes | PM04 の後は Tsuzuri のリストが連続領域になり、この種目は同じデータ構造の比較ではなくなる（近いのは `array_sum`）。参照は変えず、文書に書く（D15） |
| Phase 2: `heap_copy` を「要素が drop 不要の Copy のとき」memcpy にする | `heap_copy` は stack の frame を heap へ**移す**（要素の所有権ごとビット単位）。frame 候補がすべて空なら要素の型を問わず複製しない。`src/llvm*.rs` に `llvm.memcpy` はまだない（runtime の手書き IR だけ） | memcpy の条件は「frame 候補が空」。clone・`List.to_array`・`Array.to_list` は `!needs_drop`。intrinsic は `memcpy.p0.p0.i{ポインター幅}`（wasm32 は i32、wasm64・native は i64）を `intrinsics` に一度だけ登録 |
| `docs/benchmarks.md` に節「リストの表現（PM04）」を新設 | 第4フェーズの足場（`<!-- phase4:PM04 -->` の組）が先に入った | その組の間にだけ書く |
| README.md の `// ノードもスタック`、`_docs/library-reference/arrays-and-lists.md`、`_docs/language-reference/{ownership,patterns,types}.md` | 注釈は `docs/language.md` の 1515・1521・2878 行。README は 168 行（「添字アクセスは $O(n)$」）。`_docs/` は `_tsuzuri/language-reference/` の `built-in-types-and-modules/{list,array,vec}.md`・`ownership-and-memory/{stack-and-heap,ownership}.md`・`pattern-matching/`・`types-and-type-inference/types.md`・`loops-and-conditionals/for-in.md`。`list.md` の 5・39 行は「末尾のノードは複数のリストで共有できます」「残りを共有します」と書き、同じページの 166 行以降（所有者は常に 1 つ）と矛盾する（PM04 と無関係に不正確） | 同じ変更で直す（WP6）。共有の記述は誤りとして直す |
| 手本: `allocate_array`・`element_pointer`・`allocation_size`・`array_loop`・`frame_array` | すべて実在。`array_loop`・`list_loop_control` は PR02 の `increment_block`・`audit_increment` を使う。`allocate_array` は長さ 0 でも 1 byte を確保するが、リストの空は null（I1） | 新しい loop も同じ規約で書く |
| niche: `Maybe<リスト>` で null の head を使うか | 使わない。`niche_layout` の payload は string・utf8string・array・参照だけで、`Type::List` は tag の layout。空リストは `zeroinitializer`（null と 0）で、移動済みの slot も同じ | I1 は保つ。`Maybe<[\|T\|]>` を負の長さの niche に加えるのは別施策（PM04 に含めない） |
| Task の環境・結果・export | `llvm_task.rs` に list 固有の経路はない。list は export できない（`Type::exportable` は 8〜64 bit の整数・`f32`・`f64`・`bool`。list の戻り値は `E1008`。実測）。bindgen にも list はない | 影響なし |

### 現行の経路の一覧（行番号は HEAD `a2313d19`）

起票時の「生成と消費の経路」の表は名前のうえでは今も正しい。場所と、PM04 で置き換わる先は次のとおり。リストの表現を直接読み書きするのはこの一覧がすべて（`grep -rn "Type::List\|%tz.list" src` で確認。`src/check.rs`・`ownership.rs`・`control.rs`・`last_use.rs`・`recursive.rs`・`type_arena.rs` は型の解析だけで表現に依存しない）。

| 経路 | 場所 | 現行 | PM04 |
| --- | --- | --- | --- |
| リテラル（heap） | `src/llvm.rs` `heap_collection`（4809） | `list_builder`・要素ごとの `append_list`。loop の中は prefix を `finish_list(&head, index)` で一時値に登録 | `list_builder(length)`・slot の番号で格納。prefix は `capacity = index` の細工（D4） |
| `new [\|T\|](n, f)` | `llvm.rs` の `TypedExprKind::NewList`（4660） | `allocation_size(list_node_type, n)` → `array_loop` の中で `append_list` | 同じ検査 → chunk 1 つ → `array_loop` の添字が slot |
| リテラル（stack） | `src/llvm_frame.rs` `frame_list`（462）・`frame_node`（507）・`Frame::List`（500） | `alloca [n x { ptr, T }]` を連結 | `alloca { ptr, i64, i64, [n x T] }`（D8） |
| `Array.to_list`・`List.map`・`map_ref`・`reverse`・`to_array`・`fold_ref` | `src/llvm_bulk.rs` `bulk_collection_builtin`（400。`ArrayToList` 467、`ListMap` 以降 481） | `list_loop` と `append_list`。reverse は出力の先頭へ差し込む | `list_builder(length)`。reverse は slot `length − 1 − i` |
| `List.cons`・`List.tail` | `llvm.rs` の builtin 本体（`emit_typed_builtin` 6956。`ListCons` 7218・`ListTail` 7234）と wrapper | node 1 個の確保・解放 | `list_push_front`・`list_pop_front` |
| 複製・解放 | `llvm.rs` `clone_value`（5083。List の arm 5205）・`drop_value`（4932。arm 5023） | `list_loop` で node ごと | 外側は chunk、内側は生きた slot |
| 索引 | `llvm.rs` `checked_element_pointer`（5392。arm 5401） | `list_loop(&data, index)` | `list_locate` |
| パターン | `llvm.rs` `emit_place` の `ListTail`（4892）。`[\|a, b\|]` は `Index(matched, i)`、`h :: t` は `Index(matched, 0)` と `ListTail(matched, 1)`（`src/control.rs`） | count 個進める loop | `list_skip`・`list_locate` |
| `for` | `src/llvm_control.rs` `for_each`（259。271 行） | `list_loop_control` | D7' |
| 比較・表示・Hash | `src/llvm_compare.rs` `structural_compare`（69）、`llvm_display.rs` `structural_display`（419）、`llvm_hash.rs` `structural_hash`（17） | 比較は左右の `phi ptr`。表示・Hash は `array_loop` と memory の cursor（`load ptr`／`store ptr`） | `list_cursor_*`（比較は lockstep）、表示・Hash は `list_loop_control` |
| stack からの移動 | `llvm_frame.rs` `frame_test`（648）・`heap_copy`（733。arm 782）・`drop_frame_contents`（848。arm 890）・`relocate` | node 単位 | D8 |
| helper | `llvm.rs` の `list_node_type`（5428）〜`list_loop_control`（5479） | node の表現の知識 | 置き換え（`list_element_pointer`・`frame_node` は削除） |
| 大きさ | `llvm_frame.rs` の `stack_size`・`frame_bytes`（List の arm）、`llvm_layout.rs` `layout_bound`（233）、E1010 | 16 bytes | 変更なし |
| デバッガ・診断 | `llvm_debug.rs`（407・533）、`scripts/lldb/tsuzuri_lldb.py`、`src/copies.rs`（134・155） | node を辿る。W1006 の文言 | D12・D13 |

### 偵察で測った事実（基準は main `9dd0fbbc`、2026-10-17）

- 再現: `run` は sample 99444・nolist 9（ticket のとおり）。基準の focused test は `copy_cost` 16・`debug_info` 13・`integer_ranges` 16・`lists` 7・`storage` 21 件が通り、`node tests/debugger.mjs`（LLDB。24 s）と `node tests/debug_info.mjs` も通る（`TMPDIR=$PWD/target/tmp`）。
- 確保（`benchmarks/tracked_alloc.c`。要求量。回数／bytes）。`tail_consume`・`index_walk` は `bulk_new` と同じ:

| 種目 | n = 1 | n = 2 | n = 16 | n = 1,000 | n = 1,000,000 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `bulk_new`・`cons_build`・`tail_consume` | 1 / 16 | 2 / 32 | 16 / 256 | 1,000 / 16,000 | 1,000,000 / 16,000,000 |
| `clone_list` | 2 / 32 | 4 / 64 | 32 / 512 | 2,000 / 32,000 | 2,000,000 / 32,000,000 |
| `string_list` | 2 / 30 | 4 / 60 | 32 / 480 | 2,000 / 30,000 | 2,000,000 / 30,000,000 |

- 時間と peak RSS（native、`-O2` の host、1 回の呼び出しの平均を 7 標本の中央値で。load 6.4〜6.9）: `bulk_new` n = 1,000,000 は 19.4 ms・16.95 MB（空の process は 1.61 MB）、n = 1,000 は 19.3 µs。`cons_build` 1,000,000 は 19.0 ms、`tail_consume` 17.5 ms、`clone_list` 38.1 ms・32.3 MB、`string_list` 41.5 ms・47.7 MB、`index_walk` は n = 1,000 で 464 µs、n = 10,000 で 49.8 ms（$O(n^2)$）。`control/list_sum` は上の行のとおり。
- WASM（Node 20.19.6、wasm32 `-O3`、import 空）: `bulk_new`・`cons_build` は 100,000 個で memory 1,114,112 → 4,259,840 bytes、400,000 個で 13,893,632、600,000 個で OOM（`unreachable`）。`string_list` は 100,000 個で 9,109,504、200,000 個で OOM。
- C の試作（`target/perf/PM04/proto.c`。clang -O3、1,000,000 個の i64 の総和、最小の ns/要素）: 配列 0.13〜0.15、node 1.00〜1.06、D7 の select cursor 1.87〜1.98（bulk の 1 chunk も cons 構築の 250 超の chunk も同じ）、分岐の cursor 0.53〜0.55、二重 loop 0.10〜0.13（bulk）・0.12〜0.18（cons 構築）。
- D3 の footprint モデル（`target/perf/PM04/model2.mjs`。native の `system` は macOS の実測の丸め、`small` は 16 bytes 刻み・512 bytes まで、WASM は上の block の式。i64、header 24 bytes、各 chunk の確保サイズの和）:

| n | node | 一括（1 chunk） | cons・起票の規則 | cons・D3' | WASM の node | WASM の一括 | WASM・起票の規則 | WASM・D3' |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 16 | 32 | 32 | 32 | 32 | 48 | 48 | 48 |
| 2 | 32 | 48 | 80 | 80 | 64 | 64 | 112 | 112 |
| 4 | 64 | 64 | 144 | 144 | 128 | 80 | 192 | 192 |
| 8 | 128 | 96 | 240 | 224 | 256 | 112 | 304 | 288 |
| 16 | 256 | 160 | 400 | 336 | 512 | 176 | 480 | 416 |
| 100 | 1,600 | 832 | 1,232 | 1,088 | 3,200 | 848 | 1,344 | 1,216 |
| 1,000 | 16,000 | 8,192 | 10,192 | 12,096 | 32,000 | 8,048 | 8,656 | 11,776 |
| 10,000 | 160,000 | 81,920 | 139,216 | 96,064 | 320,000 | 80,048 | 98,960 | 86,880 |
| 1,000,000 | 16,000,000 | 8,011,776 | 12,034,000 | 8,025,920 | 32,000,000 | 8,000,048 | 8,040,432 | 8,020,608 |

  cons 構築の footprint ÷ node（native の i64）: 起票の規則は n = 1 が 2.00、n = 2 が 2.50、n = 4 が 2.25、n = 17〜256 の最悪 1.47、n = 257〜100,000 の最悪 1.37、n = 1〜100,000 の幾何平均 0.899、n = 1,000,000 は 0.752（K が header 込みで 32 KiB を超え、macOS が 49,152 へ丸めるため）。
  D3' は n = 1 が 2.00、n = 2 が 2.50、n = 4 が 2.25（同じ）、n = 17〜256 の最悪 1.24、n = 257〜100,000 の最悪 0.87、幾何平均 0.721、n = 1,000,000 は 0.502（chunk の数は 256 → 261）。WASM の i64 は幾何平均 0.394 → 0.370、n = 1,000,000 は 0.251（node の 4 倍）。
  string（16 bytes の slot）の幾何平均は 0.857 → 0.672、WASM の i8 は node の 0.06 倍。n = 1,000 のように個々の n では D3' が上回ることもある（鋸歯）ので、判断は範囲の最悪値と幾何平均で行う。

### 決定事項の更新

- D1（表現）: 維持。`%tz.list = { ptr, i64 }` はそのまま、field 0 は先頭 chunk `{ ptr next, i64 rest, i64 capacity, [0 x T] }`（header 24 bytes は native・wasm32・wasm64 で同じ。slot は `round_up(24, align(T))` から）。不変条件 I1〜I5 は維持する。**表現の切り替えに opt-out の flag は置かない**（D14）。
- D2（計算量）: 維持。`values[index]` は $O(先行する chunk 数)$（bulk なら $O(1)$）。`values[0]` と `head :: tail` の head は chunk の数によらず $O(1)$。
- D3'（容量の伸ばし方）: 改める。`LIST_CHUNK_MAX_BYTES = 32768` は **header を含む chunk 全体**の上限で、`K = max(1, (32768 − slot の offset) / max(1, size(T)))`。最初の chunk は 32 bytes に収まる容量 `c0 = max(1, (32 − slot の offset) / max(1, size(T)))`（i64 は 1、i32 は 2、i8 は 8、string・24 bytes の record は 1）、先頭 chunk が満杯のときの次の容量は `min(c + c / 2 + 1, K)`（約 1.5 倍）。size は `layout_bound(T, module, false)`（両 target で同じ値）で足りる（容量は性能の定数で、意味に関わらない）。上の定数は測って再調整できるよう Rust の定数 3 つ（上限・最初の bytes・header）にする。**報告する悪化**: 一括生成は native の i64 で n = 1 が node の 2.0 倍（32 bytes）、n = 2 が 1.5 倍、n = 3・4 で同じ、n ≥ 5 で小さい（string は n = 1 が 1.5 倍、n = 2 で同じ、n ≥ 3 で小さい。WASM の i64 は n = 1 が 1.5 倍、n = 2 で同じ）。cons で伸ばすリストは n = 1〜4 が 2.0 / 2.5 / 1.7 / 2.25 倍、n = 8 が 1.75 倍、n = 16 が 1.31 倍、n = 17〜256 で最大 1.24 倍、n = 257〜100,000 では最大 0.87 倍で、それより大きい n は 0.5 倍へ収束する（native の `system`・`small` の i64。モデル。WP6 で実測して確かめる）。WASM の i64 は n ≤ 4 が最大 1.75 倍、n = 17〜256 が最大 0.76 倍、n ≥ 257 が最大 0.44 倍。G2 の「1,000 要素以上では記憶量を HEAD 以下」は「257 要素以上のすべての n で」に改める。
- D4（一括生成）: 維持。builder は長さを先に受け取り、`list_builder(element, length)` は長さ 0 で確保せず null を返す（`append_list` は slot の番号で格納）。heap のリテラルの prefix（`tracking`。loop の中のリテラルが途中で `break`・`continue` で抜ける場合と、`try` の中で例外が出る場合。後者は 2026-10-18 の review で足した）は、prefix を登録する前に `capacity = index` を store し、最後に `capacity = n` にする（I5 と prefix の drop を保つ）。
- D5（トラップの境界）: 維持。`new [|T|](n, f)` の長さの検査は `allocation_size(list_node_type(T), n)` のまま、callback の前。chunk の bytes（`offset + n × size`）は D5 の上限の内側で溢れない。
- D6（drop・clone の順序と反復）: 維持。外側は chunk、内側は生きた slot。`next` を読んでから free する。再帰しない。
- D7'（走査の形）: 改める。Phase 1 の単一リストの走査（`for`・`List.fold_ref`・`map`・`map_ref`・`reverse` と `to_array` と clone の source・hash・display）は、状態 `(chunk, slot, index)` を持ち、chunk の終わりを **分岐**（`br`）で次へ進める cursor にする（`select` の cursor は node の連結より遅いので採らない）。`phi ptr` は各リストに 1 つ、添字の phi を header の先頭に置き増分の block を header へ戻す形を保ち、PR02 の R4・R5 の pin に合う。helper の API は `list_loop_control(list, |emitter, 要素の pointer, 添字, exit, advance|)` に固定し、Phase 2（D11）は helper の中身だけを二重 loop に替える。compare は左右の cursor を lockstep で進める（`phi ptr` が 2 つ）。
- D8（stack のリテラル）: 維持。alloca は `{ ptr, i64, i64, [n x T] }` align 16（header は null・0・n）で、`frame_test`・`relocate` は変えない。`frame_bytes` の式も据え置く（n = 1 で 16 bytes の slot の要素は alloca が 8〜16 bytes 上回るが、上限 64 KiB の見積もりには影響しない）。`List.cons`・`List.tail`・`drop_value` へ渡るのは、consuming な読み出しが `relocate` で heap へ移した後のリストだけ、という不変条件を `escape_*`・`stack_escape` のテストで固定する（stack の chunk を `next` に持つ heap の chunk は、関数を出ると dangling になる）。
- D9（`List.tail` の解放の粒度）: 維持。先頭要素をその場で drop し、chunk は空になった時点で free する。縮小・再配置はしない。
- D10（計測の workload）: 改める。6 種目（`bulk_new`・`cons_build`・`tail_consume`・`clone_list`・`index_walk`・`string_list`）の式は ticket のまま。`cons_build` は cons の loop を別関数 `build` に出す（A17 の制約。上の表）。確保の回数・bytes は n = 1、2、3、4、8、16、1,000、1,000,000（`index_walk` は 10,000 まで）、時間と peak RSS は n = 1、16、1,000、1,000,000、WASM は 100,000・400,000・600,000・1,000,000・2,000,000 での heap の高水位と OOM の境目。`tracked_alloc.c` は要求量なので、native の footprint は `/usr/bin/time` の peak RSS とモデルで補う。
- D11（Phase 分割）: 改める。Phase 1 = D1〜D10 と D12・D13 の全体（表現、D7' の分岐 cursor、D3'）。Phase 2 は 2a（`list_loop_control` の中身を chunk ごとの二重 loop に替える。compare は左右の run の loop）と 2b（`llvm.memcpy`: Copy で drop 不要の clone・`List.to_array`・`Array.to_list`、frame 候補が空の `heap_copy`）。1 件ごとに AGENTS.md のとおり before・after と生成コードの確認を付け、効果が測れない件は撤回して証拠を記録する（PM01 3b・PR03 Phase 3 の前例）。
- D12（新設。デバッガ、G16）: `llvm_debug.rs` の `list_node` を chunk の型 `<list>.chunk`（`next`・`rest`・`capacity`・`slots`。`slots` は要素型の長さ 0 の配列）に替える。`scripts/lldb/tsuzuri_lldb.py` の `ListProvider` は位置 p を `(chunk, slot)` に変える同じ算法（先頭は `slot = capacity − (length − rest)`、`p` が chunk の生きた数に届かなければ次の chunk へ）で読み、要約 `length=N` と子の並び `[0] = …` の出力、address が 2^64 を超える入力で子を返さない振る舞いを保つ。natvis は list の view を持たないので変えない（メンバーの表示は新しい型になる）。`vsc/` は変えない。
- D13（新設。W1006 の文言）: リストの暗黙の複製の警告の文言を、配列と同じ形の「implicit copy of a list allocates and copies every element; borrow it with 'ref', or call 'List.copy' to make the copy explicit」に改める（`src/copies.rs`・`tests/copy_cost.rs`・`ownership.md`。完了済みの A15 ticket は履歴なので直さない）。「新しい node を確保する」は表現の変更で偽になるため。診断のコード・位置は変えない。
- D14（新設。既定の切り替えの扱い）: 表現の切り替えは既定の出力を変える。PM01・PM06 と同じく flag は置かず、次の gate を測って満たしたときだけ残す。満たさなければ止めて報告する。fallback は基準コンパイラ（保存済み）と commit の revert で、list は export・ABI に出ないので host から表現は見えない。gate 1（Phase 1）: ①「変えてはいけない意味」の全テスト（下）が通る。② 一括生成した 1,000 個以上のリストの `alloc_bytes` が要素あたり `size(T) + 1` bytes 以下（header は 24 bytes）。③ `bulk_new`・`cons_build` の n = 1,000,000 の `peak_rss` が before の 0.6 倍以下、`clone_list` は 0.6 倍以下。④ `for` の総和の ns/要素（D7' の分岐 cursor）が before 以下。⑤ `control/list_sum` の中央値が before の最大値を超えない。⑥ 小さい n の悪化は D3' の範囲で、数値を `docs/benchmarks.md` に書く。gate 2（Phase 2 の各 item）: 生成コードに意図した形（ベクトル命令・`memcpy` の呼び出し）があり、同じ workload で 9 標本の最小値が A/A の揺れを超えて改善する。
- D15（新設。計測の整合）: ①`control/list_sum` の C・C++・Rust の参照は node ごとの確保の連結リストのまま変えず、PM04 の後は同じデータ構造の比較ではないと表に注記する（matched な比較は `array_sum`）。②native の `alloc_bytes` は要求量で、allocator の丸めは見えない。D3 の判断は peak RSS と footprint のモデルで行い、モデルの node の値は実測（上の peak RSS の 16.95 MB など）で検証する。③ A17 の「`for` の source を丸ごと除外する」制約で loop の `List.cons` が毎回複製される件は PM04 の範囲外で、完了報告に再現を書く（W1006 で見える）。

### 先に宣言する既存テストの期待値の変更

意味の検査は変えない。変えるのは、表現（ノード・chunk）や loop の形を直接読む検査だけで、意図は保つ。「ticket」は起票時に挙げていたもの。

| 場所 | 変更 | 時期 |
| --- | --- | --- |
| `tests/lists.rs` `runtime_fixture_uses_linked_nodes_and_entry_allocas`（ticket） | `runtime_fixture_uses_list_chunks_and_entry_allocas` に改名。`{ ptr, i64, i64, [0 x i64] }` を含み、`getelementptr inbounds { ptr, i64 },` を含まず、`phi ptr` を含み、`loop:` の後に alloca がない | WP3 |
| `tests/storage.rs` `bound_literals_use_stack_storage`・`stack_literals_stay_in_the_entry_block_of_tail_loops` | `alloca [2 x { ptr, i64 }]` → `alloca { ptr, i64, i64, [2 x i64] }`、`alloca [2 x { ptr, %tz.string }]` → `alloca { ptr, i64, i64, [2 x %tz.string] }`（入口の block にある、heap の確保が増えない、は同じ） | WP3 |
| `tests/primitives.mjs`（ticket） | `tz_list_length(100000)` の確保 100,000 → 1。表 `storage`: `escape_list()` 3 → 1、`heap_literals()` 4 → 3。`stack_list`・`empty_stack`・`temporary_stack` は 0 のまま | WP3 |
| `tests/integer_ranges.rs` の `STRUCTURAL` の `each_list`・audit の件数・`assert_index_loop`・`structural_comparison_loops_count_with_an_unsigned_index`（`LIST_COMPARE`） | 分岐の cursor の slot の増分（`add i64 %, 1`。proof のない通常の加算）が加わり、増分の block の形が変わる。R4・R5 の証明（`index < length` の unsigned の増分に `nuw`、audit は `uadd.with.overflow` 1 件）の主張は変えず、期待を新しい形に合わせる。Phase 2 でもう一度更新する | WP3・WP7 |
| features `typeclasses` の `inspect`（ticket は「変えない」） | Phase 1 は変えない（分岐の cursor で `phi ptr` が各リスト 1 つ）。Phase 2 は compare が run の loop になるので、「ちょうど 2 つ」を「`@tz.alloc` と index helper の呼び出しがなく、左右の cursor が lockstep」の形の検査に置き換える（`phi ptr` の数の下限は残す） | WP7 |
| `tests/debug_info.rs`（306〜338 行）・`tests/debug_info.mjs`（69 行）・`tests/debugger.mjs` | `[\|i64\|].node`・`value` offset 64 → `[\|i64\|].chunk` の `next`・`rest`・`capacity`・`slots` の offset（24 bytes の header、slot は 24）。LLDB の期待 `chain = length=2 {[0] = 4 [1] = 5}` と garbage の probe の `None` は同じ | WP4 |
| `tests/copy_cost.rs` の `LIST`・`src/copies.rs`・`ownership.md` の W1006 | 文言を D13 の形に | WP5 |
| ほかの `phi ptr` の検査（`tests/control.rs` の `for n in xs`、`tests/polymorphism.rs`、`tests/local_recursion.rs`） | 変わらない（Phase 1）。Phase 2 の後も `phi ptr` は残る見込みで、残らなければ同じ表で宣言する | WP7 |

### 作業の順序（コミット）

| 作業 | 内容 | 確認 |
| --- | --- | --- |
| WP0 | この再検討（docs だけ） | `git diff --check` |
| WP1 | `benchmarks/list-memory`（6 種目の `Main.tz`・`host.c`）と `benchmarks/run-list-memory.mjs`、before の記録（`target/perf/PM04-before/`） | checksum が式と一致、`bulk_new` 1,000,000 が 1,000,000 回・16,000,000 bytes、tracked で `live == 0` |
| WP2 | 表現を helper に集める（node のまま）。`list_loop_control` が要素の pointer と添字を渡し、`list_locate`・`list_skip`・`list_push_front`・`list_pop_front`・`list_cursor_*`・builder を既存 code の移動で作る。hash・display・compare・`for`・builtin は helper だけを呼ぶ | 基準 IR（sample・nolist・`tests/fixtures/lists`・`storage`・`typeclasses`）の差が、`%vN`・label の番号と、hash・display の走査が helper 経由になる箇所だけであること（差分を保存して読む。それ以外は基準コンパイラの出力と `cmp` で一致）。`cargo test --test lists --test storage --test integer_ranges`、`node tests/primitives.mjs` |
| WP3 | chunk 表現へ切り替え（D1・D3'・D4〜D9・D7'）。`list_element_pointer`・`frame_node` を削除、`Frame::List.nodes` → `chunk`。宣言済みの期待値だけを更新 | 同じ test と features（typeclasses・consuming_update・recursive_types・tasks・storage）。`nolist` の IR が基準と byte 同一 |
| WP4 | デバッガ（D12）: DWARF・LLDB provider・3 つのテスト・文書 | `cargo test --test debug_info`、`node tests/debug_info.mjs`、`node tests/debugger.mjs` |
| WP5 | テスト: `tests/lists.rs`（決定性・空・境界）、E2E suite `list_chunks`（境界は D3' の容量列から計算して ±1。native・wasm32 × `-O0`・`-O3`、`live == 0`、import 空）、W1006（D13） | `cargo test --test lists --test copy_cost`、`node tests/features.mjs target/release/tsuzuri list_chunks` |
| WP6 | Phase 1 の計測（after）と gate 1 の判定、文書（language.md・architecture.md・README・LR）と `docs/benchmarks.md` の marker | `node scripts/check-docs.mjs <変更したページ>`、`git diff --check`、before/after を `target/perf/PM04-after/` に保存し、Phase 1 のコンパイラを `tsuzuri-phase1` として保存 |
| WP7 | Phase 2a（D11）: `list_loop_control` の二重 loop、compare の run の loop、pin の更新 | `clang -O3` の IR・`llvm-objdump` でベクトル命令を確認、`for` の総和・`bulk_new`・`list_sum` の ns/要素を `tsuzuri-phase1` と before に対して測る、`TSUZURI_RANGE_FLAGS=audit` の suite |
| WP8 | Phase 2b（D11）: `llvm.memcpy`。item ごとに before/after・IR・asm。測れない item は撤回して記録 | 各 item の IR に `memcpy`、時間が改善 |
| WP9 | 全 suite と追加の検査、文書の仕上げ、「実装と検証」、`_completed/` へ移動 | 下の一覧 |

WP9 の確認: `cargo test --locked --no-fail-fast`。Node の suite は `primitives`・`features`（全 suite。list を使う fixture は array_bulk・consuming_update・control・deriving・explicit_copy・generic_records・higher_kinds・iteration_protocol・json・last_use・maybe_result・recursive_types・simd・storage・strings・tasks・type_aliases・typeclasses・unions と新規の list_chunks）・`control`・`computations`・`tasks`・`json`・`wasm_threads`・`stack_overflow`（1,000,000 要素の drop・clone・compare・走査）・`last_use_fuzz`・`debug_info`・`debugger`。追加の検査は `TSUZURI_TEST_ALLOCATOR=small`、`TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1`、`TSUZURI_TEST_WASM_TARGET=wasm64`、`TSUZURI_RANGE_FLAGS=audit`（`integer_ranges` と list の suite）。GUIDE §3.1 の 4 つの回帰テストも通す。`cargo +1.99.0 fmt`・clippy（`-D warnings`）と `sh scripts/check-runtime-includes.sh`（runtime を変えない見込みなので不変の確認）。

### 停止条件の更新

- 維持: `%tz.list` の 2 語を変えないと実現できない、リストを使わないプログラムの IR が 1 byte でも変わる（基準コンパイラとの `cmp`）、要素の評価順・drop・clone の順序・トラップの種類と境界が変わる、再帰する helper が要る（1,000,000 要素で stack overflow）、`@tz.realloc`・runtime の新しい関数・`unsafe`・新しい crate・既定の WASM import が要る。
- 改める: 「既存テストの期待値の変更」は上の一覧が範囲。「`alloc_bytes` が要素あたり `size(T) + 1` bytes を超える」は n ≥ 1,000 の一括生成に限る（header 24 bytes）。「`control/list_sum` の中央値が before の最大値を超える」は A/A（基準どうし）の揺れを併記して判断する。「Phase 2 の項目に手を付けたくなった」は取り下げ（Phase 2 も実装する）。
- 追加: D7' の分岐 cursor の `for` の総和が before（node）より遅い、LLDB の provider を `tests/debugger.mjs` の期待のまま通せない、D14 の gate を満たさない（止めて報告）。

### 受け入れ条件の変更

- 「D1 が承認され」は承認済み。Phase 1（WP1〜WP6）に加え Phase 2（WP7・WP8。撤回した item は証拠と理由を `docs/benchmarks.md` と「実装と検証」に書く）が終わっている。
- 「全 suite が native・WASM × `-O0`・`-O3`」に、LLDB のテスト、`TSUZURI_RANGE_FLAGS=audit`、`--allocator small`、ASan、wasm64 の通過を加える。
- 「M1〜M4 の before・after」に、小さい n の悪化（node の 2.0〜2.5 倍）、WASM の heap の高水位と上限（node 0.5M → chunk 1.9M 要素）、`control/list_sum`、`for` の総和の ns/要素、Phase 2 の各 item の判定を加える。
- G2 は「257 要素以上のすべての n で記憶量が HEAD 以下」。文書は `docs/language.md`・`docs/architecture.md`・`README.md`・LR（`list.md`・`stack-and-heap.md`・`ownership.md`・`array.md`・`vec.md`・`for-in.md`・`pattern-matching`・`types.md`・`compiler/debugging.md`）で、`list.md` の共有の誤記も直す。

### 他チケットとの関係

- PM02（`%tz.closure` の縮小）・PM03（`%tz.string` の Latin-1）: list の slot の大きさ・DWARF の要素型は `ty(element)`・`layout_bound` を通すので自動で追従する。`llvm_debug.rs` の list の arm は `Type::Function`・string の arm の隣で、textual な衝突が起きる。統合順は PM02 → PM04 → PM03。PM03 の後は `string_list` の bytes が変わるので、比較は同じ系列のコンパイラどうしで行う。
- PM01: 影響なし（list は niche の payload ではない）。PM05: chunk は 512 bytes まで `small` の size class、超えると `malloc`。`@tz.realloc` は使わない。WASM は chunk の確保が 16 MiB − 32 bytes 以下であること（bulk の i64 で約 2.09M 要素。heap 全体の既定 16 MiB の方が先に尽きる）。PM06・PM07: `Frame::List` の形だけが変わり、`relocate`・`take` の規約は同じ。PR01: 確保は `call ptr @tz.alloc(` の形で出す（後処理が文字列で探す）。PR02: loop の増分は `increment_block`・`audit_increment` を通す。PR03: 影響なし。
- PB05: 新しい IR の形（`getelementptr inbounds { ptr, i64, i64, [0 x T] }, ptr %c, i32 0, i32 3, i64 %s`・`select`・Phase 2 の `llvm.memcpy.p0.p0.i32`／`i64`）を扱えること。PB06・PR07・PR08: 影響なし。
- 衝突しやすいファイル: `src/llvm.rs`（list の helper、`drop_value`・`clone_value`・`heap_collection`・builtin 本体）、`src/llvm_bulk.rs`・`src/llvm_frame.rs`・`src/llvm_debug.rs`、`tests/features.mjs`・`tests/primitives.mjs`（storage の表）、`docs/language.md`・`docs/architecture.md`、`docs/benchmarks.md`（marker の中だけ）。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の状態欄で done であること（確認: `grep -n "PX01\|PM04" _perfs/README.md`）。HEAD では todo。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR・確保回数・計測の before）を保存していること。
- PM01・PM05・PM09 は着手条件にしない。どれも `%tz.list` の 2 語を変えない。PM09 が先に入ってリスト リテラルの alloca に
  `llvm.lifetime.*` が付いていても、alloca の型を置き換えるだけで済む（落とし穴）。

### 前提とする他チケットのインターフェース

- PX01: PX01 形式（schema 1）のレコードを `target/perf/<run_id>/<suite>.jsonl` に 1 行 1 レコードで書く。`benchmarks/metrics.mjs` の
  `instrumentAllocations`・`makeRecord`・`writeRecords`・`measureProcess`・`compareRuns`、確保追跡の `benchmarks/tracked_alloc.c`、
  metric `alloc_calls`（回）・`alloc_bytes`（bytes）・`wall_time`（ms）・`peak_rss`（bytes）、`node benchmarks/metrics.mjs report --baseline`。
- 名前が PX01 の実装と違う場合は PX01 の実装に合わせ、読み替えを完了報告に書く。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `%tz.list = type { ptr, i64 }` の 2 語を変えないと実現できない（`stack_size`・`storage_layout`・DWARF・`frame_test` が変わる）。
- 「既存テストへの影響」に挙げていない既存テストの期待値（結果・確保回数・IR・診断）を変える必要がある。
- 要素の評価順序、要素の drop・clone の順序、トラップの種類か境界（D5）が変わる。
- chunk の走査・drop・clone に再帰する helper が要る。または 1,000,000 要素のテストが stack overflow する。
- `@tz.realloc`、`src/runtime/*.ll` の新しい関数、`unsafe`、新しい crate、既定の WASM import が必要になった。
- リストを使わないプログラムの IR が 1 byte でも変わる（手順 1 と手順 10 の比較）。
- `tests/features.mjs` の typeclasses suite の `inspect`（比較 helper ごとに二入力以上の `phi ptr` がちょうど 2 つ）を D7 の形で満たせない。
- 計測で、一括生成したリストの `alloc_bytes` が要素あたり `size(T) + 1` bytes を超える、または `control/list_sum` の時間の中央値が
  before の最大値を超える。数値を合わせるための調整はしない。
- Phase 2 の項目（索引の高速経路、`memcpy`、内側ループの SIMD 化）に手を付けたくなった。

## 現状と計測（HEAD `f8dc655`）

### 現在の表現

- `%tz.list = type { ptr, i64 }`（先頭ノード、長さ）。空リストは `zeroinitializer`（null と 0）で確保しない。`stack_size`・`storage_layout`
  はどちらも 16 bytes、DWARF（`src/llvm_debug.rs`）は `head`・`length` の 2 member。
- ノードは `FunctionEmitter::list_node_type` の `{ ptr, T }`（次、要素）。要素は `list_element_pointer`（field 1 の GEP）で読む。
- 走査は `list_loop`／`list_loop_control`。`phi i64` の添字と `phi ptr` のノードを持ち、次のノードを `load ptr` で読む。
- リストは一意に所有され、構造を共有しない。Copy のリストの複製と所有する tail のパターン束縛は独立したノード列を作る
  （docs/language.md「match と分解」）。借用する tail は同じノード列を指す view で、確保しない。
- リストは直接 export できない（docs/language.md「公開 ABI」、`Type::exportable`）。Task の環境・結果は他の値と同じく `%tz.list` を
  move するだけで、リスト専用の経路はない。再帰 union の要素（`union Linked = Links of [|Linked|]`）は `drop_value` から A04 の反復 drop に入る。
- `std/List.tz` は builtin を包むだけで、表現に依存しない。

### 生成と消費の経路

| 経路 | 場所 | 現在の形 |
| --- | --- | --- |
| リテラル（ヒープ） | `src/llvm.rs` の `heap_collection` | 要素ごとに `append_list`（`@tz.alloc` 1 回）。loop の中では途中までの prefix を `finish_list(&head, index)` で一時値に登録する |
| リテラル（スタック） | `src/llvm_frame.rs` の `frame_list`・`frame_node` | 入口の `alloca [n x { ptr, T }]` を連結し、`Frame::List { nodes, elements }` を返す |
| `new [\|T\|](n, f)` | `src/llvm.rs` の `TypedExprKind::NewList` の arm | `allocation_size(list_node_type, n)` の検査の後、`array_loop` で `f` を添字順に呼んで `append_list` |
| `List.cons` | `src/llvm.rs` の `emit_typed_builtin`（`Builtin::ListCons`） | `length < i64::MAX` を検査（違反は AllocationSize）し、ノード 1 個を先頭へ |
| `List.tail` | 同（`Builtin::ListTail`） | 空なら BoundsCheck。先頭要素を drop し、先頭ノードを free |
| `Array.to_list`・`List.map`・`List.map_ref`・`List.reverse` | `src/llvm_bulk.rs` の `bulk_collection_builtin` | 要素ごとに確保。`List.reverse` は出力の先頭へ差し込む |
| `List.to_array`・`List.fold_ref` | 同 | `list_loop` で読む |
| 複製・解放 | `src/llvm.rs` の `clone_value`・`drop_value` | `list_loop`。解放は要素を drop してからノードを free |
| 索引 | `src/llvm.rs` の `checked_element_pointer` | BoundsCheck の後、`list_loop(&data, index)` で index 個進む |
| パターンの tail | `src/llvm.rs` の `emit_place` の `TypedExprKind::ListTail(value, count)` | `length >= count` を検査（違反は PatternMismatch）し、count 個進んだ view を slot に置く |
| `for` | `src/llvm_control.rs`（`list_loop_control`） | 要素へのポインタを loop 変数にする |
| 比較・表示・Hash | `src/llvm_compare.rs`（`linked`）・`src/llvm_display.rs`・`src/llvm_hash.rs` | 左右のノードを lockstep で進める。表示・Hash は `list_element_pointer` で読む |
| stack からの移動 | `src/llvm_frame.rs` の `frame_test`・`relocate`・`heap_copy`・`drop_framed`・`drop_frame_contents` | field 0 が alloca と等しく長さが n なら stack。外へ move する前にヒープへ複製し、stack の領域は free しない |

このため `List.cons`・`List.tail`・`drop_value` が受け取るリストのノードはすべてヒープにある。`frame_bytes` の見積もりは
n ×（16 + `stack_size(T)` を 16 の倍数に切り上げた値）。

### 計測済みの事実

大きさは IR の型と allocator から計算した値（PX01 の計測はまだない）。

| 要素 | ノード（native） | native の確保（macOS `malloc`、16 bytes 単位） | WASM の確保（`src/runtime/heap-wasm.ll`、header 16 bytes と 16 bytes 単位） | 配列 `[T]` の要素 |
| --- | --- | --- | --- | --- |
| `i64` | 16 | 16 | 32 | 8 |
| `string` | 24 | 32 | 48 | 16 |

- 確保回数（`tests/primitives.mjs` が検査している値）: `tz_list_length(100000)` は 100,000 回、`escape_list()` は 3 回、
  `heap_literals()` は 4 回（うちリスト `new [|4, 5|]` が 2 回）。
- 時間: 2026-09-26 の `control/list_sum`（10,000 要素）は Tsuzuri が最速（docs/benchmarks.md の `control/list_sum` の行）。記憶量は計測していない。
- 計算量（docs/language.md）: `.length`・`List.cons`・`List.tail` は O(1)、`values[index]` は O(index + 1)、パターンの tail は O(count)。

### 再現（2026-09-30 に確認）

`/tmp/tz-pm04/sample/Main.tz`（範囲 `0i64 .. n` は n を含むので、`build 5` は `[|5, 4, 3, 2, 1, 0|]`）:

```tsuzuri
def build :: i64 -> [|i64|]
fn build n =
    let mut values: [|i64|] = [||]
    for index in 0i64 .. n do values = List.cons index values
    values

let values = build 5
let values = List.tail values
let first =
    match values with
    | head :: tail -> head * 100 + tail.length * 10 + tail[0]
    | [||] -> 0
let literal = [|7, 8, 9|]
let generated = new [|i64|](4, i -> i * i)
first + values[3] + literal[2] * 1000 + generated[3] * 10000
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri run /tmp/tz-pm04/sample
target/release/tsuzuri build /tmp/tz-pm04/sample --emit llvm -o /tmp/tz-pm04/before.ll
grep -c "call ptr @tz.alloc" /tmp/tz-pm04/before.ll
grep -c "getelementptr inbounds { ptr, i64 }" /tmp/tz-pm04/before.ll
grep -n "^%tz.list\|alloca \[3 x { ptr, i64 }\]" /tmp/tz-pm04/before.ll
```

結果は `99444`、静的な `@tz.alloc` 呼び出し 7 箇所、ノードの GEP 22 箇所、`%tz.list = type { ptr, i64 }` と
`alloca [3 x { ptr, i64 }], align 16`。

## 目標と指標

目標は CI の合否条件にしない。計測手順の before と after を記録して判断する。

- G1: 一括生成したリストの確保を、非空なら 1 回・`header + n × size(T)` bytes にする（`i64` で要素あたり 8 + 24/n bytes）。
- G2: `List.cons` で伸ばすリストの確保回数を O(log n + n / K)（K は D3 の最大容量）にし、1,000 要素以上では記憶量を HEAD 以下にする。
- G3: 時間を悪化させない。`values[index]` の走査は一括生成したリストで O(1) になる。
- G4: 観測できる意味（「変えてはいけない意味」）を一切変えない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 確保量 | `alloc_bytes`（1 回の実行、決定的） | `benchmarks/list-memory` の 6 種目、n = 1、2、16、1,000、1,000,000 | `bulk_new` 1,000,000: 16,000,000 → 8,000,024。`cons_build` 1,000,000: 16,000,000 → 8,034,296。`clone_list` 1,000,000: 32,000,000 → 16,000,048 |
| M2 確保回数 | `alloc_calls`（決定的） | 同上 | `bulk_new` 1,000,000: 1,000,000 → 1。`cons_build` 1,000,000: 1,000,000 → 256。`clone_list`: 2,000,000 → 2 |
| M3 時間 | `wall_time` ms（9 標本の中央値、最小、最大） | 6 種目（`index_walk` は n = 10,000、他は 1,000,000）と `control/list_sum`（既定の規模） | 悪化しない。`index_walk` は O(n²) の走査が O(n) になる |
| M4 最大 RSS | `peak_rss` bytes（9 標本の中央値） | `bulk_new`・`cons_build` の n = 1,000,000（native） | M1 の差（約 8 MB）に近い減少 |

計算の根拠（native、`i64`、header 24 bytes）: `cons_build` は容量 1, 2, 4, …, 4,096 の 13 chunk（8,191 要素）と容量 4,096 の 243 chunk で
256 回・24 × 256 + 8 × 1,003,519 bytes。小さい `cons_build` は悪化する（n = 1: 16 → 32、n = 2: 32 → 72、n = 16: 256 → 368 bytes。
n = 15 で 240 → 216）。この悪化は D1 の承認の対象として完了報告に数値で書く。

## 変えてはいけない意味

- 評価順序: リテラルは左から右、`new [|T|](n, f)` は添字 0 から n − 1 の順に `f` を呼ぶ、`List.map`・`List.map_ref`・`List.fold_ref` は
  リストの順に callback を呼ぶ。`new` の長さと初期化関数の評価順（`list_order`）も同じ。
- 解放: 各要素をちょうど一回 drop し、順序はリストの順（先頭から）。`List.tail` は先頭要素をその場で drop する（chunk の解放は D9）。
  複製はリストの順に要素を clone する。drop・clone・比較・表示・Hash は反復で処理し、長さに比例するスタックを使わない。
- トラップ: 種類と境界を変えない（D5）。`new [|T|](n, f)` の AllocationSize は `f` を呼ぶ前、`List.cons` は長さ `i64::MAX` で
  AllocationSize、索引と空の `List.tail` は BoundsCheck、パターンの tail は PatternMismatch。例外は `@tz.alloc` の失敗（メモリ不足）が
  起きる時点だけ（D5）。
- 所有権・借用: 型規則を変えない。所有者だけが先頭の chunk を書き換え、借用中の view は書き換えられない（借用規則が保証する）。
  Copy の複製は独立した chunk を作る。リンクの共有は観測できない。
- 計算量: どの操作も最悪の計算量を悪化させない。`List.cons`・`List.tail`・`head :: tail` の分解は償却ではなく最悪 O(1)（D2）。
- 値の大きさ: `%tz.list = type { ptr, i64 }`、`stack_size`・`storage_layout`（16 bytes）、DWARF、E1010 の見積もりを変えない。
  リストは export できないので公開 ABI は変わらない。
- 生成コード: IR は決定的。リストを使わないプログラムの IR は byte 単位で同じ。runtime 関数を増やさず、既定の WASM import を増やさない。

## 設計

### 表現

`%tz.list` は変えない。field 0 を「先頭ノード」から「先頭 chunk」に読み替える。chunk は要素 `T` ごとの literal struct 型（D1）。

```llvm
%tz.list = type { ptr, i64 }            ; 先頭 chunk（長さ 0 のときだけ null）、長さ
; chunk of T: { ptr next, i64 rest, i64 capacity, [0 x T] slots }
;   i64 リストの例: { ptr, i64, i64, [0 x i64] }、要素は field 3（native・wasm32 とも offset 24、align 16 の T は 32）
```

不変条件（chunk `c`、長さ `len` の記述子。所有する値と view の両方）:

- I1: `len == 0` と `c == null` は同値。
- I2: `c.rest < len <= c.rest + c.capacity`。先頭 chunk の生きた要素数は `live = len - c.rest`。
- I3: リストの位置 p（0 が先頭）は、`p < live` なら `c` の slot `c.capacity - live + p`、それ以外は `c.next` と長さ `c.rest` の位置 `p - live`。
  chunk の中ではリストの順とアドレスの順が同じ。
- I4: 先頭以外の chunk は満杯（`d = c.next` なら `d.rest + d.capacity == c.rest`）。したがって次の chunk の最初の要素は slot 0。
- I5: 所有者の先頭 chunk の slot `[0, c.capacity - live)` は未使用で、どの値も指さない。
- `next`・`rest` は chunk を作った後は変えない。`capacity` を変えるのはヒープのリテラルの組み立て中だけ（D4）。

### データ構造

```rust
// src/llvm.rs
const LIST_CHUNK_MAX_BYTES: usize = 32_768; // （新規）cons で足す chunk の要素領域の上限（D3）

// src/llvm_frame.rs
pub(super) enum Frame {
    /// One list chunk `{ ptr, i64, i64, [n x T] }` stored in the alloca `chunk`.
    List { chunk: String, elements: Vec<Vec<Frame>> }, // field 名 nodes → chunk
    // Array・String・Aggregate は変えない
}
```

`FunctionEmitter`（`src/llvm.rs`）の helper。表現の知識はこの一群だけに置き、他のファイルは helper を呼ぶ。

| helper | 役割 |
| --- | --- |
| `list_node_type` | 長さの上限の stride（`{ ptr, T }` の大きさ）だけに使う（D5） |
| `list_chunk_type`（新規） | `{ ptr, i64, i64, [0 x T] }` |
| `list_slot_pointer(element, chunk, slot)`（新規） | `getelementptr inbounds <chunk>, ptr c, i32 0, i32 3, i64 slot` |
| `list_chunk_alloc(element, capacity, next, rest)`（新規） | `@tz.alloc(offset(slots) + capacity × size(T))` と header の 3 store |
| `list_builder(element, length)`・`append_list(element, chunk, slot, value)`・`finish_list(chunk, length)` | 一括生成（D4）。`list_builder` は `length == 0` なら確保せず null |
| `list_cursor_start`・`list_cursor_advance`（新規） | 走査の状態 `(chunk, slot)` の開始と前進（D7） |
| `list_loop`・`list_loop_control` | 本体へ要素のポインタを渡す（ノードではなく）。`list_loop` は値を返さない |
| `list_locate(element, list, index)`（新規） | 索引の要素ポインタ |
| `list_skip(element, list, count)`（新規） | パターンの tail の view |
| `list_push_front`・`list_pop_front`（新規） | `List.cons`・`List.tail` の本体 |

### アルゴリズム

```text
cursor_start(c, len):            live = len - c.rest; return (c, c.capacity - live)      ; len > 0 のときだけ呼ぶ
cursor_advance(c, slot):         s = slot + 1; end = (s == c.capacity)
                                 return (select end, c.next, c ; select end, 0, s)       ; 分岐しない。I4 で次は slot 0
loop(c, len, body):              if len == 0 goto exit; (c, s) = cursor_start
                                 for i in 0..len: body(slot_pointer(c, s)); (c, s) = cursor_advance(c, s)
locate(c, len, p):               loop { live = len - c.rest; if p < live: return slot_pointer(c, c.capacity - live + p)
                                        p -= live; len = c.rest; c = c.next }              ; BoundsCheck の後。chunk 数だけ回る
skip(c, len, k):                 n = len - k                                              ; PatternMismatch の検査の後
                                 while n != 0 && n <= c.rest: c = c.next                  ; 最大 k 回
                                 return (n == 0 ? null : c, n)
push_front(c, len, v):           guard len < i64::MAX (AllocationSize)
                                 if len > 0 && c.capacity - (len - c.rest) > 0: store v at slot c.capacity - (len - c.rest) - 1; return (c, len + 1)
                                 k = len == 0 ? 1 : umin(2 * c.capacity, K)               ; D3
                                 d = chunk_alloc(k, next = c, rest = len); store v at slot k - 1; return (d, len + 1)
pop_front(c, len):               guard len > 0 (BoundsCheck); drop element at slot c.capacity - (len - c.rest)
                                 if len - 1 == c.rest: n = c.next; free c; return (n, len - 1)
                                 return (c, len - 1)
drop(c, len):                    while c != null: live = len - c.rest
                                   for s in c.capacity - live .. c.capacity: drop slot s   ; リストの順
                                   n = c.next; free c; len = c.rest; c = n                 ; next を読んでから free
clone(c, len):                   d = builder(len); i = 0; loop(c, len, |p| append(d, i, clone(*p)); i += 1); finish(d, len)
```

K は `max(1, LIST_CHUNK_MAX_BYTES / max(1, storage_layout(T) の大きさ))` を Rust 側で計算した定数（両 target で同じ値）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| helper | `src/llvm.rs` | 「データ構造」の helper 一覧 | 追加と置き換え。`list_element_pointer` は削除する（呼び出しが残らない） |
| 生成 | `src/llvm.rs` | `heap_collection` の `TypedExprKind::List` の arm | 最初の要素を評価してから `list_chunk_alloc(n)`、slot i に格納。`tracking` のときは prefix を登録する前に `capacity = index` を store し、最後に `capacity = n`（D4） |
| 生成 | `src/llvm.rs` | `TypedExprKind::NewList` の arm | `allocation_size(list_node_type, n)` を残し（D5）、`list_builder(n)`、`array_loop` の添字を slot にする |
| builtin | `src/llvm.rs` | `emit_typed_builtin` の `Builtin::ListCons`・`Builtin::ListTail` | `list_push_front`・`list_pop_front` |
| builtin | `src/llvm_bulk.rs` | `bulk_collection_builtin` の `ArrayToList`・`ListMap`・`ListMapRef`・`ListReverse`・`ListToArray`・`ListFoldRef` | 出力は `list_builder(length)`。`ListReverse` は slot `length - 1 - i` に格納する。読み取りは `list_loop` の要素ポインタ |
| 複製・解放 | `src/llvm.rs` | `clone_value`・`drop_value` の `Type::List` の arm | 「アルゴリズム」の clone・drop |
| 索引 | `src/llvm.rs` | `checked_element_pointer` の `Type::List` の arm | `list_locate` |
| パターン | `src/llvm.rs` | `emit_place` の `TypedExprKind::ListTail` の arm | `list_skip`。PatternMismatch の検査はそのまま |
| for | `src/llvm_control.rs` | `list_loop_control` を呼ぶ箇所 | 受け取った要素ポインタを loop 変数にする |
| 比較 | `src/llvm_compare.rs` | `Type::List` の `linked` の経路 | 左右それぞれ `list_cursor_start`・`list_cursor_advance`。`phi ptr` は左右の chunk の 2 つだけ（D7） |
| 表示・Hash | `src/llvm_display.rs`・`src/llvm_hash.rs` | `list_element_pointer` を呼ぶ loop | `list_loop` の要素ポインタ |
| stack | `src/llvm_frame.rs` | `frame_list`・`frame_node`・`Frame::List` | alloca `{ ptr, i64, i64, [n x T] }` align 16 に header（null, 0, n）と slot 0..n−1。`frame_node` は `list_slot_pointer` に置き換えて削除 |
| stack | `src/llvm_frame.rs` | `frame_test`・`relocate`・`heap_copy`・`drop_framed`・`drop_frame_contents` | `frame_test` と `relocate` は変更なし（field 0 と長さの比較のまま）。`heap_copy` は `list_builder(n)` へ slot を複製、`drop_frame_contents` は slot を順に drop し、free しない |
| stack | `src/llvm_frame.rs` | `stack_size`・`frame_bytes` | 変更なし（D8） |
| 変更なし | `src/llvm_debug.rs`・`src/check.rs`・`src/ownership.rs`・`src/control.rs`・`src/call_specialization.rs`・`std/List.tz` | — | 記述子・型規則・所有権は変わらない |

### Phase 分割

- Phase 1（この実装）: 表現の切り替え、上の全経路、D3 の伸長、テスト、文書、計測。chunk の中の走査は D7 の平らな loop のまま。
  cons で容量 1 の chunk だけを足す中間段階は作らない（cons で作るリストが要素あたり 16 → 32 bytes に悪化するため。D3）。
- Phase 2（人間が求めた場合だけ）: 連続領域を使う高速経路。`for`・`List.fold_ref`・比較の内側を chunk ごとの二重 loop にして
  ベクトル化できる形にする、要素が drop 不要の Copy のとき clone・`List.to_array`・`Array.to_list`・`heap_copy` を `llvm.memcpy` にする。
  どれも AGENTS.md に従い、同じ workload の before・after と生成コードの確認を付ける。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。重い suite は Node 24 で動かす（`npx --yes --package=node@24 node tests/<suite>.mjs ...`）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の sample と、リストを使わない `/tmp/tz-pm04/nolist/Main.tz`（下）の IR を保存し、
  コンパイラを `/tmp/tz-pm04/tsuzuri-before` に複製する。
- 確認: `run` がそれぞれ `99444` と `9` を出す。

```tsuzuri
def twice :: i64 -> i64
fn twice x = x * 2

let values = [1, 2, 3]
twice (values[2]) + values.length
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cp target/release/tsuzuri /tmp/tz-pm04/tsuzuri-before
for p in sample nolist; do
  target/release/tsuzuri run /tmp/tz-pm04/$p
  for o in 0 3; do target/release/tsuzuri build /tmp/tz-pm04/$p --emit llvm -O$o -o /tmp/tz-pm04/$p-before-O$o.ll; done
done
```

### 手順 2: 計測の workload と before

- 変更: `benchmarks/list-memory/Main.tz`（新規）、`benchmarks/run-list-memory.mjs`（新規）。
- 内容: D10 の 6 種目と runner を作り、before を記録する（計測手順）。
- 確認: `target/perf/PM04-before/list-memory.jsonl` に 6 種目 × 5 規模の `alloc_calls`・`alloc_bytes` と時間の記録がある。
  checksum が D10 の式と一致し、`bulk_new` の n = 1,000,000 は `alloc_calls` 1,000,000・`alloc_bytes` 16,000,000。

### 手順 3: 表現の知識を helper に集める（表現は変えない）

- 変更: `src/llvm.rs`、`src/llvm_bulk.rs`、`src/llvm_control.rs`、`src/llvm_compare.rs`、`src/llvm_display.rs`、`src/llvm_hash.rs`、`src/llvm_frame.rs`。
- 内容: ノード表現のまま、`list_loop`・`list_loop_control` が本体へ要素ポインタを渡すようにし、`list_locate`・`list_skip`・
  `list_push_front`・`list_pop_front`・`list_cursor_start`・`list_cursor_advance` を既存のコードの移動で作る。比較・表示・Hash・索引・
  パターン・builtin はこれらだけを呼ぶ。`list_element_pointer` を呼ぶのは helper の中だけにする。
- 確認: `cargo test --locked` と `node tests/primitives.mjs target/release/tsuzuri` が成功する。手順 1 の 4 つの IR と `diff` し、
  差がないか `%vN` の番号だけであること。

### 手順 4: chunk 表現へ切り替える

- 変更: 「段ごとの変更」の全行。`tests/lists.rs` と `tests/primitives.mjs` の「既存テストへの影響」に挙げた期待値。
- 内容: helper を D1〜D9 の形にし、`list_element_pointer` と `frame_node` を削除する。`Frame::List` の field を `chunk` にする。
- 確認: `cargo test --locked --test lists` が `7 passed`。`node tests/primitives.mjs target/release/tsuzuri` と
  `node tests/features.mjs target/release/tsuzuri typeclasses consuming_update recursive_types tasks` が成功する。

### 手順 5: Rust テスト

- 変更: `tests/lists.rs`。
- 内容: 「テスト計画」の Rust テストを足す。
- 確認: `cargo test --locked --test lists` が `9 passed`。

### 手順 6: E2E suite `list_chunks`

- 変更: `tests/fixtures/list_chunks/Main.tz`（新規）、`tests/features.mjs`（GUIDE §7.4 の手順で suite を追加）。
- 内容: 「テスト計画」の E2E。
- 確認: `node tests/features.mjs target/release/tsuzuri list_chunks` が native・WASM × `-O0`・`-O3` で成功し、`live == 0`。

### 手順 7: 全体の確認

- 確認: `cargo test --locked`、`node tests/primitives.mjs target/release/tsuzuri`、`node tests/features.mjs target/release/tsuzuri`、
  `node tests/control.mjs target/release/tsuzuri`、`node tests/computations.mjs target/release/tsuzuri`、`node tests/tasks.mjs target/release/tsuzuri`、
  `node tests/wasm_threads.mjs target/release/tsuzuri` がすべて成功する。stack-depth の 3 テスト（GUIDE §11.1）も成功する。

### 手順 8: 生成コードの確認

- 確認: 「生成コードの確認」のすべての pattern が期待どおり。`nolist` の IR が手順 1 と byte 単位で同じ。

### 手順 9: after の計測

- 確認: `target/perf/PM04-after/` に before と同じ種類の記録があり、`report --baseline` の比較を保存した。checksum が before と同じ。

### 手順 10: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md docs/benchmarks.md README.md _docs/library-reference/arrays-and-lists.md _docs/language-reference/ownership.md _docs/language-reference/patterns.md _docs/language-reference/types.md` と `git diff --check` が成功する。

## 計測手順

- 環境: PX01 の `captureRun` が記録する項目（CPU、OS、clang・rustc・node の版、commit）を JSONL に入れる。電源に接続し、他の重い処理を止める。
- runner: `benchmarks/run-list-memory.mjs --compiler <tsuzuri> --metrics <dir>`。`-O3` の library を 2 通り作る。確保用は PX01 の
  `instrumentAllocations` を通した IR を `benchmarks/tracked_alloc.c` と host にリンクし、各種目・各規模を 1 回呼んで `alloc_calls`・
  `alloc_bytes` を記録する（決定的。1 標本）。時間用は追跡なしで、1 回の warm-up の後に 9 標本を取り、中央値・最小・最大を記録する。
  `peak_rss` は `measureProcess` で host の `--only <name> <n>` を 9 回起動する。
- WASM の確保量は計測しない。「現状と計測」の表と同じ式で計算した値を文書に書く。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node benchmarks/run-list-memory.mjs --compiler /tmp/tz-pm04/tsuzuri-before --metrics target/perf/PM04-before
node benchmarks/run-control.mjs /tmp/tz-pm04/tsuzuri-before --metrics target/perf/PM04-before
cargo build --release --locked
node benchmarks/run-list-memory.mjs --compiler target/release/tsuzuri --metrics target/perf/PM04-after
node benchmarks/run-control.mjs target/release/tsuzuri --metrics target/perf/PM04-after
node benchmarks/metrics.mjs report --baseline target/perf/PM04-before target/perf/PM04-after | tee target/perf/PM04-after/report.txt
```

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for o in 0 3; do target/release/tsuzuri build /tmp/tz-pm04/sample --emit llvm -O$o -o /tmp/tz-pm04/sample-after-O$o.ll; done
grep -c "{ ptr, i64, i64, \[0 x i64\] }" /tmp/tz-pm04/sample-after-O0.ll
grep -c "getelementptr inbounds { ptr, i64 }," /tmp/tz-pm04/sample-after-O0.ll
grep -n "alloca { ptr, i64, i64, \[3 x i64\] }, align 16" /tmp/tz-pm04/sample-after-O0.ll
grep -c "llvm.umin\|llvm.memcpy" /tmp/tz-pm04/sample-after-O0.ll
for o in 0 3; do
  target/release/tsuzuri build /tmp/tz-pm04/nolist --emit llvm -O$o -o /tmp/tz-pm04/nolist-after-O$o.ll
  cmp /tmp/tz-pm04/nolist-before-O$o.ll /tmp/tz-pm04/nolist-after-O$o.ll
done
```

期待: 1 行目は 1 以上、2 行目は 0（旧ノードの GEP がない）、3 行目は 1 件、4 行目は 0（Phase 1 は intrinsic を増やさない）、`cmp` は差なし。`-O0` の `main` の本体では、`new [|i64|](4, …)` の `call ptr @tz.alloc` が
初期化 loop の `phi` を持つ block より前に 1 回だけある。`-O3` では `for` の loop の本体に `call` がなく、chunk の前進が `select` になる。

## テスト計画

### Rust テスト（`tests/lists.rs`）

- `runtime_fixture_uses_linked_nodes_and_entry_allocas` を `runtime_fixture_uses_list_chunks_and_entry_allocas` に改名する。native と wasm32 で
  `%tz.list = type { ptr, i64 }`、`{ ptr, i64, i64, [0 x i64] }` を含み、`getelementptr inbounds { ptr, i64 },` を含まず、`phi ptr` を含み、
  `loop:` の後に alloca がないことを確かめる。
- `list_chunk_ir_is_deterministic`（新規）: fixture の IR を 2 回出して一致し、リストを使わない source の IR に `[0 x` がない。
- `list_helpers_are_not_recursive`（新規）: fixture の IR の各 `define` の本体が自分自身を `call` しない。

### E2E（`tests/fixtures/list_chunks/Main.tz`、suite `list_chunks`）

期待値は式から計算する（コンパイラの出力から取らない）。`build n` は `List.cons` で 0 から n − 1 を足したリスト（先頭が n − 1）。

| 関数 | 内容 | 期待 |
| --- | --- | --- |
| `cons_sum n` | `build n` の `for` の和 | n(n − 1)/2 |
| `cons_index n` | `build n` の `values[i] × (i + 1)` の和 | (n³ − n)/6 |
| `tail_then_cons n k m` | `new` の 0..n−1 に `List.tail` を k 回、`100 + j`（j = 0..m−1）を m 回 cons して和 | (n(n − 1) − k(k − 1))/2 + 100m + m(m − 1)/2 |
| `pattern_views n` | `build n` を `a :: b :: rest` で分解し `a × 10⁶ + b × 10³ + rest.length` | (n − 1)·10⁶ + (n − 2)·1001 |
| `string_churn n` | `"ab" + "c"` を n 回 cons、⌊n/2⌋ 回 `List.tail`、残った要素の長さの和 + 残りの長さ | 4(n − ⌊n/2⌋) |
| `stack_escape` | 関数が返した 3 要素の stack リテラル（7, 8, 9）に 1 を cons し `v[0] + v[3] × 10 + length × 100` | 491 |
| `deep_ops n` | `let b = build n` の Copy、`a == b`、`List.to_array (ref a)` の長さ + `b[n − 1]` | n |

```javascript
list_chunks: {
  cases: [
    ...[1n, 2n, 3n, 4n, 7n, 8n, 15n, 16n, 8191n, 8192n, 8193n, 12287n, 12288n].flatMap((n) => [
      ["cons_sum", [n], n * (n - 1n) / 2n], ["cons_index", [n], (n ** 3n - n) / 6n]]),
    ...[[5n, 0n, 1n], [5n, 2n, 2n], [5n, 2n, 3n], [5n, 5n, 0n], [1n, 1n, 2n], [4096n, 17n, 17n], [4096n, 17n, 18n]].map(([n, k, m]) =>
      ["tail_then_cons", [n, k, m], (n * (n - 1n) - k * (k - 1n)) / 2n + 100n * m + m * (m - 1n) / 2n]),
    ...[2n, 3n, 4n, 8n, 9n].map((n) => ["pattern_views", [n], (n - 1n) * 1000000n + (n - 2n) * 1001n]),
    ...[1n, 2n, 3n, 8n, 9n, 1000n].map((n) => ["string_churn", [n], 4n * (n - n / 2n)]),
    ["stack_escape", [], 491n], ["deep_ops", [100000n], 100000n],
  ],
  nativeCases: [["deep_ops", [1000000n], 1000000n], ["cons_sum", [1000000n], 499999500000n]],
},
```

境界の意図: 8191・8192・8193 と 12287・12288 は容量 4,096 の chunk の境目（D3）、`tail_then_cons` の (4096, 17, 17) は空いた slot への
上書きだけ、(4096, 17, 18) は満杯の後の新しい chunk、`pattern_views 3` は view が次の chunk へ移る場合。`string_churn` は `"ab" + "c"`
の文字列を要素にして、非 Copy の要素の解放を `live == 0` で確かめる。

### 既存テストへの影響

- `tests/lists.rs`: 上の改名と期待の変更だけ。
- `tests/primitives.mjs`: `tz_list_length(100000)` の確保回数 100,000 → 1、表 `storage` の `escape_list()` 3 → 1、`heap_literals()` 4 → 3。
  `tests/fixtures/storage/Storage.tz` の他のリスト（`stack_list`・`empty_stack`・`temporary_stack`）は stack のままで 0 回。
- `tests/features.mjs` の typeclasses suite の `inspect`: 変えない（D7 で満たす）。他の期待値: なし。

### 性能

合否の閾値は置かない。計測手順の記録だけを残す。

## ドキュメント

- `docs/language.md`: 「型とメモリ」（連結リストの記憶域）、「スタックとヒープ（`new`）」（`// ノードもスタック` などの注釈と見積もりの説明。式は D8 で据え置き）、
  「配列・連結リストとレコード」（`new [|T|]` の確保、`List.cons`・`List.tail`・索引の計算量）、「match と分解」（所有する tail の複製）。
- `docs/architecture.md`: 「不変条件」の **連結リスト:** の段落を D1 の表現と I1〜I5 に書き換え、「開発と検証」のリストの試験の記述を更新する。
- `README.md` の `// ノードもスタック`・`// ノードごとにヒープ`、`_docs/library-reference/arrays-and-lists.md` の冒頭と「作成と読み取り」の表、
  `_docs/language-reference/ownership.md`・`patterns.md`・`types.md` の「ノード」の記述。
- `docs/benchmarks.md`: 節「リストの表現（PM04）」（新規）に M1〜M4 の before・after、条件、WASM の計算値を書く。
- 完了時に `_perfs/README.md` の PM04 の状態を更新する。

## 受け入れ条件

- [ ] D1 が承認され、Phase 1 の手順 1〜10 が終わっている。
- [ ] 「変えてはいけない意味」のすべてを保ち、既存テストの期待値の変更が「既存テストへの影響」の範囲に収まる。
- [ ] `list_chunks` を含む全 suite が native・WASM × `-O0`・`-O3` で成功し、`live == 0`、WASM の import が空。
- [ ] 1,000,000 要素の `deep_ops`・`cons_sum` が native で stack overflow しない。
- [ ] 「生成コードの確認」の pattern が期待どおりで、リストを使わないプログラムの IR が byte 単位で同じ。
- [ ] M1〜M4 の before・after が `target/perf/PM04-before`・`PM04-after` と docs/benchmarks.md にあり、小さい `cons_build` の悪化も書いてある。
- [ ] 文書を更新し、GUIDE §10 の完了の定義を満たす。

## 落とし穴

- null の chunk の header を読むと未定義動作になる。`list_cursor_start` は長さ 0 の分岐の後でだけ呼ぶ。`list_cursor_advance` は現在の
  （null でない）chunk だけを読む。最後の要素の後で `next`（null）へ進んでも、loop は抜けるので読まない。
- `{ c, 0 }` の記述子を作らない（I1）。`list_skip` は残りが 0 なら null を返す。`new [|T|](0, f)` は確保しない（`allocations == before` の検査）。
- ヒープのリテラルは最初の要素を評価してから chunk を確保する。loop 内の prefix は `capacity = index` を store してから登録する。これを
  忘れると prefix の drop が未初期化の slot を drop する。
- stack の alloca（`[n x T]`）にも `[0 x T]` の chunk 型で GEP する。`inbounds` は確保した領域の中なので正しい。
- `List.reverse` は slot `length - 1 - i` に書く。callback と clone の順序はリストの順のまま。
- K は `storage_layout` の大きさから Rust で計算した定数にする。`@llvm.umin.i64` などの intrinsic を増やさず、`icmp` と `select` で書く。
- `frame_bytes` を変えると `large_stack()`・`large_heap()` の配置が動く。変えない（D8）。
- `List.tail` で小さくなるリストも chunk が空になるまで記憶域を保持する（D9）。`tail_consume` の `peak_rss` は下がらなくてよい。
- 再帰 union を要素に持つリスト（`list_drop`）は要素ごとの `drop_value` が A04 の待ちリストへ入る。chunk の drop から再帰呼び出ししない。
- PM09 が list リテラルの alloca に `llvm.lifetime.*` を付けた後なら、marker の大きさを新しい alloca 型に合わせる。

## 対象外

- 構造共有する永続リスト、要素の直接の書き換え、chunk の縮小・再配置、`%tz.list` を 3 語にすること。
- Phase 2 の高速経路（内側 loop のベクトル化、`llvm.memcpy`）。`frame_bytes` の式の変更。WASM の確保量の実測。
- 利用者 drop（B07）の順序の観測テスト（B07 の実装後に B07 が足す）。

## 決定事項

### D1: 表現（chunk 列）

- 決定: `%tz.list = { ptr, i64 }` はそのまま、field 0 は先頭 chunk `{ ptr next, i64 rest, i64 capacity, [0 x T] }`。chunk の中はリストの順とアドレスの順が同じ。不変条件は「表現」の I1〜I5。
- 理由: 伸縮する単一バッファ（旧案）は `List.cons` が償却 O(1) になり、最悪 O(n) の複製と一時的な 2 倍の記憶が要る。unrolled list に `rest` を持たせると cons・tail・分解が最悪 O(1) のまま、一括生成は配列と同じ連続領域になり、記述子の大きさも変わらない。
- 状態: 承認済み（2026-10-17）。実装済み（opt-out の flag なし。D14 の gate を満たした。「実装と検証」）

### D2: 計算量の保証

- 決定: `.length`・`List.cons`・`List.tail` は最悪 O(1)、`head :: tail` の分解は最悪 O(count)（chunk 内なら O(1)）、索引は O(先行する chunk 数)（一括生成なら O(1)）、走査・複製・解放は O(n)。
- 理由: docs の保証を弱めず、索引だけを改善する。
- 状態: 実装済み（`tests/lists.rs`・`list_chunks` の `index_walk`・`bulk_new` の確保の回数で確認）

### D3: 容量の伸ばし方

- 決定: 先頭 chunk が満杯（または空リスト）のときだけ新しい chunk を作る。容量は空リストなら 1、それ以外は `min(2 × 先頭 chunk の capacity, K)`、`K = max(1, LIST_CHUNK_MAX_BYTES / max(1, storage_layout(T) の大きさ))`、`LIST_CHUNK_MAX_BYTES = 32768`。
- 理由: 1 要素のリストを最小（native 32 bytes）にし、確保回数を O(log n + n/K) にする。上限で最大の空きを 32 KiB に抑える。容量 1 だけで足す段階は要素あたり 16 → 32 bytes に悪化するので作らない。
- 状態: 変更して実装（D3'。1.5 倍・header 込みの 32 KiB。一括生成は上限なしの 1 つの chunk）

### D4: 一括生成

- 決定: リテラル・`new`・`Array.to_list`・`List.map`・`List.map_ref`・`List.reverse`・複製・`heap_copy` は非空なら chunk 一つ（capacity = 長さ）を 1 回で確保し、空なら確保しない。ヒープのリテラルは最初の要素の評価後に確保し、loop 内の prefix は `capacity` を途中の長さにして登録する。
- 理由: 確保回数を 1 にし、I1 と prefix の drop を保つ。
- 状態: 実装済み（`heap_copy` は frame 候補が空のとき loop のまま。Phase 2b の `memcpy` は取り下げ）。ヒープのリテラルは最初の要素の評価後でなく前に確保し、loop と `try` の中では要素ごとに prefix（`capacity` を途中の長さにした chunk。最初の要素の前に抜けるときは長さ 0 で chunk だけ）を一時値に登録する。最初の版は `tracking` が loop だけで、`try` の中の最初の要素が例外を出すと chunk が漏れた（配列のリテラルは以前から、どの要素でも漏れた）。2026-10-18 の review で見つかり、`tracking` に `try` を加えて直した（`list_chunks` の `raise_list_*`・`raise_array_*`）

### D5: トラップの境界とメモリ不足

- 決定: `new [|T|](n, f)` の長さの検査は `allocation_size(list_node_type(T), n)`（旧ノードの stride）のまま行う。`List.cons` の `i64::MAX`、BoundsCheck、PatternMismatch も変えない。メモリ不足の `llvm.trap` は、一括生成では callback の前の 1 回の確保で起きるようになる。
- 理由: 実行できる長さの境界（大きさ 0 の要素を含む）を byte 単位で保つ。メモリ不足の時点は環境に依存し、種類は同じ。
- 状態: 実装済み（`list_guard_length`。`tests/primitives.mjs` の WASM の trap の境界で確認）

### D6: drop・clone の順序と反復

- 決定: drop は chunk ポインタが null になるまでの外側の loop と、生きた slot をリストの順に drop する内側の loop。`next` を読んでから free する。clone は chunk 一つへリストの順に clone する。
- 理由: 要素の順序を HEAD と同じにし、スタックを使わない。
- 状態: 実装済み（`deep_ops` の 1,000,000 要素、`tests/stack_overflow.mjs`）

### D7: 走査の形

- 決定: 状態は `(chunk, slot, index)`。前進は分岐なしの `select`（「アルゴリズム」の `cursor_advance`）で、`phi ptr` は各リストに 1 つ。
- 理由: 比較 helper の既存の IR 検査（`phi ptr` 2 つ、`@tz.alloc` なし）を保ち、loop の形を HEAD に近く保つ。二重 loop は Phase 2。
- 状態: 変更して実装（D7'。Phase 1 は分岐の cursor、slot は分岐の合流点の `phi`。Phase 2a で chunk ごとの二重 loop。`select` の cursor は node の連結と同じ約 1 ns で採らなかった）

### D8: スタックのリテラル

- 決定: alloca は `{ ptr, i64, i64, [n x T] }` align 16（header は null・0・n）。`frame_test`・`relocate` は変えない。`frame_bytes` の式は据え置く（n ≥ 2 では上界、n = 1 で 16-byte 整列の要素だけ 16 bytes 下回る）。
- 理由: スタックとヒープの配置と確保回数の観測値を変えない。
- 状態: 実装済み（alloca は `{ ptr, i64, i64, [n x T] }`。`tests/storage.rs` と `stack_escape`・`words_escape` で確認）

### D9: `List.tail` の解放の粒度

- 決定: 先頭要素はその場で drop し、chunk は空になった時点で free する。縮小・再配置はしない。
- 理由: O(1) を保つ。保持する記憶は一括生成の元の大きさを超えない。
- 状態: 実装済み（`tail_consume`・`list_chunks` の `tail_walk`・`words_tail`）

### D10: 計測の workload

- 決定: `benchmarks/list-memory/Main.tz` に `bulk_new`（`new` の 0..n−1 の和、n(n−1)/2）、`cons_build`（cons の 0..n−1 の和、n(n−1)/2）、`tail_consume`（`new` を `List.tail` で空にしながら先頭の和、n(n−1)/2）、`clone_list`（Copy の複製と元の和、n(n−1)）、`index_walk`（`values[i]` の和、n(n−1)/2）、`string_list`（`"ab" + "c"` の n 個の長さの和、3n）。runner は `benchmarks/run-list-memory.mjs`。
- 理由: 一括生成・cons・tail・複製・索引・非 Copy 要素を分けて測る。`control` 系列は他言語の実装が要るので増やさない。
- 状態: 変更して実装（D10'。`walk_list`・`fold_list`・`compare_list`・`list_to_array`・`array_to_list`・`escape_list` を足して 12 種目、runner に `--opt O0|O3`）

### D11: Phase 分割

- 決定: Phase 1 は表現の切り替え全体（「Phase 分割」）。Phase 2 は人間が求めた場合だけ着手する。
- 理由: 表現は全経路で同時に切り替える必要があり、Phase 1 をさらに分けると二つの表現が混在する。高速経路は計測と生成コードの確認を個別に要する。
- 状態: 変更して実装（D11'。Phase 1 の後に 2a と 2b も実装した。2b の `heap_copy` は効果が測れず取り下げ）

## 実装と検証

Phase 1（WP1〜WP6）、Phase 2a（WP7）、Phase 2b（WP8）をブランチ `perf/p4-PM04` に実装した。Phase 2b の 4 件のうち `heap_copy` の `memcpy` だけは、効果が測れないので取り下げた（下）。

### コミット

| 作業 | コミット | 内容 |
| --- | --- | --- |
| WP0 | `ae070af7` | 着手時の再検討（このチケットだけ） |
| WP1 | `5683911d` | `benchmarks/list-memory`（最初は 6 種目）・`benchmarks/run-list-memory.mjs`・参照式・`tests/benchmark_list_memory.mjs`、before の記録 |
| WP2 | `32f08c73` | 表現の知識を `src/llvm_list.rs` に集める（node のまま。list を使わないコーパスの IR は基準と同一、list を使う 1 件は hash・display の走査が helper 経由になる差だけ） |
| WP3 | `53815eb5` | chunk 表現への切り替え（D1・D3'・D4〜D9）と、宣言した pin の更新 |
| WP5 | `a39434ce` | E2E suite `list_chunks`（最初は 3,224 件）、`tests/lists.rs` の chunk の形のテスト、W1006 の文言（D13） |
| WP4 | `fb7def50` | デバッガ（D12）: DWARF の `<list>.chunk`、LLDB の `ListProvider`、3 つのテスト |
| WP6 | `d0bdfe33`・`0f9f16af`・`cdfc03e5` | `walk_list`、cursor の slot を `select` でなく `phi` に、文書と Phase 1 の測定 |
| WP7 | `a85856eb` | Phase 2a: chunk ごとの二重 loop、compare の run の loop、`fold_list`・`compare_list`、pin の更新 |
| WP8 | `156bd043` | Phase 2b: `llvm.memcpy`（clone・`List.to_array`・`Array.to_list`）、runner の `--opt O0\|O3`、`list_to_array`・`array_to_list`・`escape_list`、`list_chunks` の複製・変換の 504 件 |
| 統合 | `fa362764`・`52104c86`・`cccea74e`・`ee00e255`・`d6a1df4a`・`4194f716`・`4e4bc4c0`・`421c3880` | PR07・PR08、PB06、PM02、PM03 の merge と、それぞれの review fix の取り込み（`421c3880` は PM03 の review fix と CI の配線）。衝突は PM02 の `src/llvm_debug.rs`（`descriptor_pointer` と `list_chunk` の並び）と `docs/architecture.md` の 2 件、PM03 の `src/llvm_control.rs`（`for_each`。list の arm は `list_loop_control` のまま、PM03 の Latin-1 の arm を続けた）と `src/llvm_frame.rs`（`frame_test`。`Frame::List { storage, elements }` と `Frame::String { data, word }`、長さは `u64`）の 2 件で、両方の意図を残した。ほかは衝突なし |
| 修正 | `0cb1c5e5` | `tests/server.mjs`: PB06 の review fix（節 8b）と PM03（節 11）が同じ block で `const build` を宣言し、統合ブランチの先頭（`71365e0c`・`e4025531`）では SyntaxError で suite が始まらなかった。PM03 の helper を `stringsBuild` に改名した |
| WP9 | `80b418fa`・`1c16cbfe`・`96a1e19a`・`cad51a27`・`506ee1af`・`9a983158`・最後のコミット | README の検証コマンド、決定の状態、`list_free_chunks`（最初の chunk の解放を loop の前に出して、`escape_list` の確保を消す）、最終の測定と `benchmark_list_memory` の検査の修正、小さい定数サイズのリストの悪化の明記（`list.md`・`docs/language.md`・`docs/architecture.md`・`README.md`）、チケットの完了と検証の記録 |

### 実装したこと

- 表現（`src/llvm_list.rs`）: `%tz.list = { ptr, i64 }` はそのまま、先頭の chunk `{ ptr next, i64 rest, i64 capacity, [0 x T] slots }`（header 24 bytes は native・wasm32・wasm64 で同じ）。リストの表現を知るのはこのファイルだけで、ほかの emitter は helper（`list_builder`・`list_loop`・`list_each`・`list_loop_control`・`list_lockstep_loop`・`list_locate`・`list_skip`・`list_push_front`・`list_pop_front`・`list_drop`・`list_clone`・`list_stack_*`）を呼ぶ。一度に作るリストは要素数ちょうどの 1 つの chunk（空は確保しない）、`List.cons` は先頭の chunk の空き slot に書き、満杯なら 1.5 倍（header 込みで 32 KiB が上限、最初は 32 bytes）の chunk を足す。`List.tail` とパターンの tail は同じ chunk に短い長さを組み合わせた view。長さの検査（D5）は旧 node の stride のまま callback の前。
- 走査: Phase 1 は `(chunk, slot)` の分岐の cursor、Phase 2a で chunk ごとの二重 loop（外側が chunk、内側が `run` 個の slot を数える loop）。`for`・`List.fold_ref`・clone・hash・display・`List.to_array` が通る。比較は左右の run（現在の chunk に残る slot の数と残りの組数の最小）の loop。位置が要らない本体は位置を計算しない（`list_each`・`list_loop_control`）。
- Phase 2b: 要素が drop 不要なら、`List.copy`・`List.to_array`・`Array.to_list` は chunk ごとの `llvm.memcpy.p0.p0.i64`（wasm32 は `.i32`）。宣言は `intrinsics` に 1 回だけ。
- drop: 要素が drop 不要のリストは、最初の chunk の解放を chunk をたどる loop の前に出す（`list_free_chunks`）。LLVM は作ったばかりの chunk の `next` をそれを書いた store から知るので、同じ関数で作って捨てるリスト（`new` のリテラル、小さいリテラルを返す関数の結果）の確保が `-O3` で消える（`escape_list` は 1,000,000 回で 29.2 ms・確保 1,000,000 回が 3.46 ms・確保 0 回に）。
- デバッガ（D12）: DWARF の `<list>.chunk`（`slots` は長さ 0 の配列）、LLDB の `ListProvider` が chunk をたどる（`tests/debug_info.rs`・`tests/debug_info.mjs`・`tests/debugger.mjs` の期待）。
- W1006 の文言（D13）、文書（`docs/language.md`・`docs/architecture.md`・`README.md`・LR の 8 ページ）、`docs/benchmarks.md` の PM04 の節。
- 道具: `benchmarks/list-memory`（12 種目）と `benchmarks/run-list-memory.mjs`（`--baseline`・`--wasm`・`--opt O0|O3`・`--metrics`・`--json`）、`tests/benchmark_list_memory.mjs`、E2E suite `list_chunks`（chunk の境界の前後 ±2 を JS で計算。native・wasm32 × `-O0`・`-O3`。`TSUZURI_ASAN=1`・`TSUZURI_TEST_ALLOCATOR=small`・`TSUZURI_RANGE_FLAGS=audit|proven`・`TSUZURI_TEST_WASM_TARGET=wasm64` でも通る）。

### 実装しなかったこと、取り下げたこと

- Phase 2b の `heap_copy`（frame 候補が空のとき、スタックの chunk を 1 回の `memcpy` でヒープへ）: `-O3` では `escape_list` の asm が Phase 2a と同一（LLVM が SROA でスタックのリテラルをヒープへの store につなぐ）、`-O0` の差は A/A の幅の中（1,000,000 回で 103.4 ms と 101.7 ms、A/A は 106.6 ms と 107.9 ms）。D14 の gate 2 を満たさないので、コードは Phase 2a の loop のまま（`docs/benchmarks.md`）。
- 比較のベクトル化: 比較は要素ごとに早く抜けるので、内側の loop は scalar（`ldr`・`ldr`・`cmp`・`b.ne`）のまま。Phase 1 の cursor の 2 倍速いが、整数の `==` を run ごとの分岐なしの reduction にする案（配列の比較にも効く）は、PM04 の範囲を越えるので入れていない。
- `tests/debugger.mjs` の実機の LLDB での実行: この機械は macOS の Developer mode が無効で、`lldb` の `run` が戻らない（hello world の C でも）。DWARF は `llvm-dwarfdump --verify`、provider は LLDB の値の代役でたどり方を確かめる `tests/lldb_lists.py`（2026-10-18 の review で、最初は管理外の scratch だった検査を `tests/` に入れた）と、実際の DWARF の型と模擬の process（scratch）で検査した。CI の LLDB で実行して確かめる必要がある。
- Windows・Linux・x86-64 の測定（型の検査は `cargo check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` で警告 0）。wasm32 の `memory.copy` の速度は測っていない。
- A17 の last-use 解析の制約（範囲外。下の「訂正」）。
- 定数の大きさの小さいリストの確保の除去: 基準の node の連結では、リストの大きさが定数で、作る・走査する・捨てるが 1 つの関数に収まると、LLVM が確保を全部消した（1 回約 1 ns）。chunk では `new` のリテラルの走査だけが消え（最初の chunk の解放を loop の前に出した後）、`List.map`・`List.reverse`・`List.copy`・`List.cons` の連鎖・文字列のリストでは 1〜3 個の chunk の確保が残り、1 回あたり 25〜80 ns かかる（表は `docs/benchmarks.md`）。大きさが実行時に決まるリストでは chunk の方が速い。直すには、複数の chunk をたどる loop の回数を LLVM に見せる、または小さい定数の結果を stack の chunk にする（PM06 の escape 解析の拡張）必要があり、PM04 の範囲を越えるので入れていない。

### 測定

基準は main `9dd0fbbc` の release コンパイラー（SHA-256 `5e1b9687…`）。Apple M1 Max、macOS 27.0.1、Apple clang 21.0.0、Node 20.19.6、rustc 1.98.1、native `-O3` と wasm32 `-O3`。確保の回数と bytes は決定的（要求量で、allocator の丸めは含まない）で、時間と peak RSS は他の作業と並行する共有機の値（1 分の load average は 8〜65）。`node benchmarks/run-list-memory.mjs $AFTER --baseline $BEFORE --samples 9 --wasm`（2 つの build を交互に、標本ごとに 1 回の warm-up の後、中央値。同じコンパイラーどうしの A/A の差は 0.1〜1.2%、最大 4.7%）。時間は参考値で、速度の合否の閾値は置いていない。表、生成コードの確認（asm・IR）、WASM の heap の高水位の全体は `docs/benchmarks.md` の PM04 の節、生データは `target/perf/PM04-before/`・`PM04-after*/`（git の管理外）。

**gate（D14）の判定.**

| gate | 判定 | 値 |
| --- | --- | --- |
| 1-① 意味のテスト | 満たした | 下の「検証」 |
| 1-② 1,000 個以上の一括生成が `size(T) + 1` bytes/要素以下 | 満たした | 1,000 個で 8,024 bytes、1,000,000 個で 8,000,024 bytes（8.02 bytes/要素） |
| 1-③ n = 1,000,000 の peak RSS が 0.6 倍以下 | 満たした | `bulk_new` 0.546、`cons_build` 0.552、`tail_consume` 0.546、`clone_list` 0.523 |
| 1-④ `for` の総和の ns/要素が before 以下 | 満たした（`select` の cursor は node と同じ約 1 ns で満たせず、`phi` に改めた） | 約 1.20 → 約 0.58 ns（Phase 2a の後は約 0.12 ns） |
| 1-⑤ `control/list_sum` が before の最大値を超えない | 満たした | 1.903 → 0.0916 ms（20.8 倍。Phase 2a の後は下の 40.4 倍） |
| 1-⑥ 小さい n の悪化が D3' の範囲 | 満たした | 下 |
| 2a `for`・`List.fold_ref`・clone の二重 loop | 満たした | 内側の loop が `ldp q`・`add.2d`（`i64` を 8 個ずつ）。Phase 1 比で `walk_list` 0.24 倍、`fold_list` 0.25 倍（n = 1,000,000） |
| 2a compare の run の loop | 満たしたが、ベクトル化はしない | 要素ごとの早期脱出が残るので scalar（`ldr`・`ldr`・`cmp`・`b.ne`）。Phase 1 比 0.51 倍。ベクトル化は対象外（下） |
| 2b `llvm.memcpy`（clone・`List.to_array`・`Array.to_list`） | 満たした（`-O0` と、`-O3` の `List.to_array`） | `-O0` で 0.74〜0.90 倍（A/A は 0.3〜2.9%）、`-O3` の `List.to_array` は 0.84 倍。`-O3` の clone・`Array.to_list` は asm が同一（loop idiom の変換がすでに `memcpy` にしていた） |
| 2b `heap_copy` の `memcpy` | 取り下げ | `-O3` の asm が同一、`-O0` の差は A/A の幅の中 |

**最終の測定（全フェーズ、n = 1,000,000）.** 変更後は PM02・PR07・PR08・PB06 を取り込んだ merge 後の tree（`ee00e255`）の release コンパイラー（SHA-256 `e92ced7a…`）、基準は main。9 標本の中央値。

| workload | before ms | after ms | 比 | before RSS MB | after RSS MB | 確保 before → after（回） |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `bulk_new` | 19.69 | 0.545 | 0.028 | 16.97 | 9.28 | 1,000,000 → 1 |
| `cons_build` | 19.48 | 1.139 | 0.058 | 16.97 | 9.39 | 1,000,000 → 261 |
| `tail_consume` | 17.75 | 1.095 | 0.062 | 16.97 | 9.28 | 1,000,000 → 1 |
| `clone_list` | 38.62 | 1.084 | 0.028 | 32.30 | 16.94 | 2,000,000 → 2 |
| `string_list` | 41.44 | 18.42 | 0.445 | 47.67 | 32.25 | 2,000,000 → 1,000,001 |
| `walk_list` | 34.74 | 2.480 | 0.071 | 16.97 | 9.28 | 1,000,000 → 1 |
| `fold_list` | 34.54 | 2.122 | 0.061 | 16.98 | 9.28 | 1,000,000 → 1 |
| `compare_list` | 62.29 | 7.689 | 0.123 | 32.31 | 17.05 | 2,000,000 → 262 |
| `list_to_array` | 18.95 | 0.811 | 0.043 | 24.64 | 16.94 | 1,000,001 → 2 |
| `array_to_list` | 19.39 | 0.863 | 0.045 | 24.63 | 16.94 | 1,000,001 → 2 |
| `index_walk`（n = 10,000） | 50.38 | 0.0157 | 0.0003 | 1.80 | 1.69 | 10,000 → 1 |
| `escape_list` | 0.001 | 3.243 | （下） | 1.59 | 1.61 | 0 → 0 |

- `control/list_sum`（100,000 個）: 1.808 → 0.0447 ms（40.4 倍。C の参照は 1.844 ms で、node ごとに `malloc` する連結リストなので同じデータ構造の比較ではない）。ほかの 15 種目は 0.991〜1.005 倍。
- `escape_list`（32 要素のリストを返す関数を n 回）: 基準では LLVM が確保を全部消し、総和を閉じた式に畳む（0.001 ms）。最初の chunk 版は確保が残って 29.2 ms だったが、`list_free_chunks` で消え 3.24〜3.46 ms になった（確保 0 回）。閉じた式までは畳まれないので基準の 0.001 ms には届かない。この workload は WP8 で足したので、WP6 の gate では測っていなかった。
- WASM（wasm32、既定の 16 MiB の heap）: `i64` のリストが収まる上限は 400,000 個（600,000 個で trap）から 1,000,000 個（2,000,000 個で trap）になった。`string_list` は文字列が heap の大半を使うので 400,000 個で trap のまま。
- **報告する悪化（要素が数個のリスト）**: 要求 bytes は `i64` の 1 個で 16 → 32（2.0 倍）、`cons_build` の 2 個で 32 → 72（2.25 倍）、4 個で 64 → 128（2.0 倍）、16 個で 256 → 320（1.25 倍）、1,000 個で 0.72 倍、1,000,000 個で 0.50 倍。一度に作るリストは 2 個で 1.25 倍、3 個で同じ、4 個以上で小さい。malloc の丸めを入れた footprint は 257 個以上のすべての `i64` で node 以下（モデル）。
- **報告する悪化（同じ関数で作って使って捨てる、要素数が定数の小さなリスト）**: node の連結では LLVM が確保を全部消して 1 回約 1 ns だった。chunk では `new` のリテラルの走査だけが消え（約 1 ns）、`List.map`・`List.reverse` が約 25 ns、`List.copy` が約 45 ns、`List.cons` の 3 回の連鎖が約 55〜65 ns、文字列 2 個の `new` が約 75〜80 ns（3 要素、1〜3 個の chunk の確保が残る）。要素数が実行時に決まるリストは逆に速い（上の表）。2026-10-18 の判断で受け入れ、`list.md`・`docs/language.md`・`docs/architecture.md`・`README.md`・`docs/benchmarks.md` に数値と原因を書いた。直す方法は下の「実装しなかったこと、取り下げたこと」に書いた。
- **生成コードの同一性**: list を使わないプロジェクトの IR は、PM04 を含まない統合ブランチ（`e4025531` を `git archive` で展開して build。PM02・PM03 までの review fix を含む）と byte 単位で同一。`--emit llvm` の `-O0` と `-O3` で native は 96 件が同一、wasm32 も `-O0` と `-O3` で 91 件が同一で、違う 25 件（`tests/fixtures` の 23 件、`benchmarks/control`、`benchmarks/list-memory`）はすべて list を使う（基準が build できない native の 8 件と wasm32 の 13 件は除く。比較の道具は `target/perf/PM04/ir-compare.mjs`）。PM04 だけの段階では、main に対して list を使わない 94 件が同一だった。

### 検証

- 静的: `cargo +1.99.0 fmt --check`、`clippy --all-targets --locked -- -D warnings`、`cargo check --all-targets --locked --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc`（いずれも警告 0。Windows の実行は CI の Windows のジョブで確かめる）、`git diff --check`、`sh scripts/check-runtime-includes.sh`、`node scripts/check-docs.mjs`（LR の 8 ページ。122 link、66 個の例、native の `-O0`・`-O3` の実行 132 回）。
- Rust（全体）: `cargo test --locked --no-fail-fast`（最後の merge の後の tree `184019ce`。`RUST_MIN_STACK=4194304`）は 109 の test binary（doc-test を含む）で 1,595 件が通り、失敗 0、無視 1（23 分 45 秒）。その前の `ee00e255` の tree では 108 binary で 1,563 件が通った（26 分）。§3.1 の 4 つの回帰テストは各 1 件が通る。`TSUZURI_STRINGS=compact` では `lists`・`storage`・`integer_ranges`・`array_bulk`・`copy_cost`・`debug_info`・`compact_strings` の 93 件が通る。
- Rust（個別に数えた値。`d6a1df4a`）: `--lib` 386 件と `--bin tsuzuri` 30 件（PB06 の guard を含む）、`lists` 12、`integer_ranges` 16、`array_bulk` 3、`debug_info` 16、`lto` 21、`storage` 21、`copy_cost` 16、`closure_representation` 9。
- Node（最後の merge の後の tree `184019ce`）: `features` の全 suite（19,113 件）は既定の UTF-16 と `TSUZURI_STRINGS=compact` の両方で通る。`list_chunks`（3,728 件。native・WASM × `-O0`・`-O3`）は UTF-16・compact に加え、`TSUZURI_TEST_ALLOCATOR=small`、`TSUZURI_ASAN=1`、`TSUZURI_RANGE_FLAGS=audit`・`proven`、`TSUZURI_TEST_WASM_TARGET=wasm64`（Node 24）でも通り、`storage` は UTF-16・compact・small・ASan で通る。`primitives`・`e2e`・`examples`・`debug_info`（native・WASM の `-O0`・`-O3`、`llvm-dwarfdump --verify`）・`benchmark_list_memory` は UTF-16 と compact の両方、`pgo`（16 節）・`server`・`watch`、`control`（470）・`computations`・`json`・`tasks`・`wasm_threads`・`stack_overflow`・`last_use_fuzz`（300）・`allocator`・`small_allocator`・`user_drop`・`trap_boundary` も通る。PM03 の `node tests/compact_strings_modes.mjs target/release/tsuzuri quick compact`（11 suite）も通る。`benchmark_list_memory` は、`escape_list` の確保が 0 回になったのに合わせて「すべての workload が確保する」の検査を直した。
- LLDB（実機なし）: `python3 tests/lldb_strings.py`（PM03。11 の読み取り、長い文字列、空、不正な語 3）と `python3 tests/lldb_lists.py`（`ListProvider` の chunk のたどり方。先頭側の空き slot、短い長さの view、1・4・16 byte の要素、200 個の chunk の二分探索、読んだ chunk の記憶、壊れたメモリ。formatter を 6 通りに壊して検出を確かめた）が通る。
- 最後の merge（`421c3880`）の後の短い確認: release を build し直し、`list_chunks`（UTF-16・compact・small・wasm64）、`storage`・`primitives`・`benchmark_list_memory`（UTF-16・compact）、`debug_info`・`e2e`・`examples`・`pgo`・`server`・`watch`、PM03 の `compact_strings_modes.mjs quick compact`（11 suite。PM03 の review fix（F3）で `compact_strings_raw` は `llvm-objdump` が無いと失敗するので、`TSUZURI_OBJDUMP` を指定した）、cargo の `--lib --bin tsuzuri`（425 件）・`lists` ほか 9 binary（129 件）・compact の 7 binary（96 件）、fmt・clippy 1.99・Windows の型検査（警告 0）・`check-docs`（8 ページ）が通る。
- 統合ブランチの先頭（`71365e0c`・`e4025531`）の `tests/server.mjs` は、PB06 と PM03 の merge で `const build` が二重に宣言され、SyntaxError で suite が始まらない。`0cb1c5e5` で直した（上のコミットの表）。
- 中断した 1 回の cargo test の実行で、`tests/targets.rs` の 3 件（`a_foreign_target_runs_through_the_runner_and_only_then`・`a_foreign_target_tests_through_the_runner_and_reports_each_test`・`a_different_triple_or_sysroot_never_reuses_a_cached_build`）が失敗した。単独（12 件）と、上の全体の実行では通る。原因は特定していない（共有機の負荷、またはその間に走らせていた node の構文検査と git の操作との競合）。
- 確認できていないこと: 実機の LLDB での `tests/debugger.mjs`、Windows・Linux・x86-64 の実行と時間、wasm32 の `memory.copy` の速度（上の「実装しなかったこと、取り下げたこと」）。wasm64 は `list_chunks` だけで確かめた。

### 起票・再検討との違いと訂正

- D3: 起票の「容量は倍々」は、Phase 0 で 1.5 倍・header 込みの 32 KiB に改めた（D3'）。実装でさらに、一括生成は上限のない 1 つの chunk とし、上限 `K` は `List.cons` が足す chunk だけを縛る。
- D7: Phase 1 の cursor で slot を `select` で進めると、`add`・`cmp`・`csinc` の 3 cycle が slot の再帰に入り、総和が node の連結と同じ約 1 ns/要素になった（C の試作の分岐の cursor は 0.53 ns）。slot を分岐の合流点の `phi` に改めると約 0.58 ns/要素（`walk_list` から `bulk_new` を引いた 15 回分）で、Phase 2a の二重 loop は約 0.12 ns/要素。
- features `typeclasses` の `inspect`（Phase 0 は「Phase 1 では変えない」）: compare の cursor の `phi ptr` が各リスト 2 つ（chunk と、chunk の合流点の chunk）になるので、WP3 で 2 → 4 に変えた。Phase 2a の run の loop も 4 のままなので、文を「run ごとに左右が lockstep」に直しただけ。
- 宣言し損ねた pin: `tests/array_bulk.rs` の `list_fold_ref_reads_elements_with_the_list_node_layout` は node の型 `{ ptr, i64 }` を読んでいたが、WP7 で全体の `cargo test` の一部を走らせるまで気づかなかった。`..._chunk_layout` に改名し、`{ ptr, i64, i64, [0 x T] }` を読む検査にした（意味は同じ）。この種の pin は、早い段階で `cargo test` を全部走らせると見つかる。
- D10: 6 種目の `benchmarks/list-memory` に、`walk_list`（`for` の総和 16 回。build を引くと 1 要素の走査の時間になる）・`fold_list`・`compare_list`・`list_to_array`・`array_to_list`・`escape_list` を足して 12 種目にした。Phase 2 の各 item に、別の workload で before・after と生成コードを付けるため。runner に `--opt O0|O3` を足した（Phase 2b は `-O0` でだけ効く）。
- 起票の「Phase 2 は人間が求めた場合だけ」は、全フェーズ実装の依頼（2026-10-17）で実装した。Phase 2b の `-O3` では、clone と `Array.to_list` の asm が Phase 2a と同一（LLVM の loop idiom の変換が先に `memcpy` にしていた）なので、効くのは `-O0`（REPL の既定、PB05 の Cranelift のような LLVM の最適化を持たない backend の入力）と `List.to_array` の `-O3` だけ。
- A17 の制約（範囲外のまま）: last-use 解析が `for` の source の局所変数を丸ごと除外するので、同じ関数で `values = List.cons i values` の loop と `for v in values` を書くと、`List.cons` のたびにリストが複製されて $O(n^2)$ になる。`--warn implicit-copy` の `W1006` で見える。再現: `let mut values: [|i64|] = [||]`、`for i in 0 .. n - 1 do values = List.cons i values`、`for v in values do total = total + v` を 1 つの関数に書く。`benchmarks/list-memory` は cons の loop を別関数 `build` に出している。
