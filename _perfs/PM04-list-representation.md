# PM04: 連結リストの連続表現

| 項目 | 内容 |
| --- | --- |
| ID | PM04 |
| 分類 | メモリ |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | C07, PM05 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（リストの記憶域を要素ごとのノードから chunk 列へ変え、docs/language.md の記憶域・計算量の記述を変える） |
| 手本にする既存実装 | 置き換える経路: `src/llvm.rs` の `list_node_type`・`list_element_pointer`・`list_builder`・`append_list`・`finish_list`・`list_loop`・`list_loop_control`。連続領域の手本: `src/llvm.rs` の `allocate_array`・`element_pointer`・`allocation_size`・`array_loop`、`src/llvm_frame.rs` の `frame_array` と、`heap_copy`・`drop_frame_contents` の `Frame::Array` の arm。反復 drop の手本: `src/llvm.rs` の `drop_value` の `Type::List` の arm（次を読んでから free する） |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_bulk.rs`, `src/llvm_control.rs`, `src/llvm_frame.rs`, `src/llvm_compare.rs`, `src/llvm_display.rs`, `src/llvm_hash.rs`, `tests/lists.rs`, `tests/primitives.mjs`, `tests/features.mjs`, `tests/fixtures/list_chunks/Main.tz`（新規）, `benchmarks/list-memory/Main.tz`（新規）, `benchmarks/run-list-memory.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`, `_docs/library-reference/arrays-and-lists.md`, `_docs/language-reference/ownership.md`, `_docs/language-reference/patterns.md`, `_docs/language-reference/types.md` |
| 計測対象 | `benchmarks/list-memory`（新規、6 種目）の `alloc_calls`・`alloc_bytes`・`wall_time`・`peak_rss`、`control/list_sum` の `wall_time`、`tests/primitives.mjs` の確保回数（決定的） |

## 目的

リスト `[|T|]` を、要素ごとのノードではなく「要素を連続して並べた chunk の連結列」で表す。`new`・リテラル・`Array.to_list`・複製など
一括で作るリストは chunk 一つ（配列と同じ連続領域）になり、`List.cons` で伸ばすリストは容量を倍々にした chunk を足していく。
1 要素あたりの記憶量と確保回数を減らし、走査・索引・比較を連続領域の処理にする。

Tsuzuri のリストは一意に所有され、複製は深く、ノードのリンクは公開しない（docs/language.md「配列・連結リストとレコード」）。
先頭の chunk を書き換えられるのは所有者だけなので、構造を共有しなくても観測できる意味を保ったまま表現を変えられる。

実装者は Phase 1 だけを実装する。Phase 2（設計の「Phase 分割」）は人間が求めた場合だけ着手する。

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
- 状態: 要承認（承認前はどの手順にも着手しない。小さい `cons_build` の記憶量の悪化を含めて承認を得る）

### D2: 計算量の保証

- 決定: `.length`・`List.cons`・`List.tail` は最悪 O(1)、`head :: tail` の分解は最悪 O(count)（chunk 内なら O(1)）、索引は O(先行する chunk 数)（一括生成なら O(1)）、走査・複製・解放は O(n)。
- 理由: docs の保証を弱めず、索引だけを改善する。
- 状態: 既定案（実装者はこの案に従う）

### D3: 容量の伸ばし方

- 決定: 先頭 chunk が満杯（または空リスト）のときだけ新しい chunk を作る。容量は空リストなら 1、それ以外は `min(2 × 先頭 chunk の capacity, K)`、`K = max(1, LIST_CHUNK_MAX_BYTES / max(1, storage_layout(T) の大きさ))`、`LIST_CHUNK_MAX_BYTES = 32768`。
- 理由: 1 要素のリストを最小（native 32 bytes）にし、確保回数を O(log n + n/K) にする。上限で最大の空きを 32 KiB に抑える。容量 1 だけで足す段階は要素あたり 16 → 32 bytes に悪化するので作らない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 一括生成

- 決定: リテラル・`new`・`Array.to_list`・`List.map`・`List.map_ref`・`List.reverse`・複製・`heap_copy` は非空なら chunk 一つ（capacity = 長さ）を 1 回で確保し、空なら確保しない。ヒープのリテラルは最初の要素の評価後に確保し、loop 内の prefix は `capacity` を途中の長さにして登録する。
- 理由: 確保回数を 1 にし、I1 と prefix の drop を保つ。
- 状態: 既定案（実装者はこの案に従う）

### D5: トラップの境界とメモリ不足

- 決定: `new [|T|](n, f)` の長さの検査は `allocation_size(list_node_type(T), n)`（旧ノードの stride）のまま行う。`List.cons` の `i64::MAX`、BoundsCheck、PatternMismatch も変えない。メモリ不足の `llvm.trap` は、一括生成では callback の前の 1 回の確保で起きるようになる。
- 理由: 実行できる長さの境界（大きさ 0 の要素を含む）を byte 単位で保つ。メモリ不足の時点は環境に依存し、種類は同じ。
- 状態: 既定案（実装者はこの案に従う）

### D6: drop・clone の順序と反復

- 決定: drop は chunk ポインタが null になるまでの外側の loop と、生きた slot をリストの順に drop する内側の loop。`next` を読んでから free する。clone は chunk 一つへリストの順に clone する。
- 理由: 要素の順序を HEAD と同じにし、スタックを使わない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 走査の形

- 決定: 状態は `(chunk, slot, index)`。前進は分岐なしの `select`（「アルゴリズム」の `cursor_advance`）で、`phi ptr` は各リストに 1 つ。
- 理由: 比較 helper の既存の IR 検査（`phi ptr` 2 つ、`@tz.alloc` なし）を保ち、loop の形を HEAD に近く保つ。二重 loop は Phase 2。
- 状態: 既定案（実装者はこの案に従う）

### D8: スタックのリテラル

- 決定: alloca は `{ ptr, i64, i64, [n x T] }` align 16（header は null・0・n）。`frame_test`・`relocate` は変えない。`frame_bytes` の式は据え置く（n ≥ 2 では上界、n = 1 で 16-byte 整列の要素だけ 16 bytes 下回る）。
- 理由: スタックとヒープの配置と確保回数の観測値を変えない。
- 状態: 既定案（実装者はこの案に従う）

### D9: `List.tail` の解放の粒度

- 決定: 先頭要素はその場で drop し、chunk は空になった時点で free する。縮小・再配置はしない。
- 理由: O(1) を保つ。保持する記憶は一括生成の元の大きさを超えない。
- 状態: 既定案（実装者はこの案に従う）

### D10: 計測の workload

- 決定: `benchmarks/list-memory/Main.tz` に `bulk_new`（`new` の 0..n−1 の和、n(n−1)/2）、`cons_build`（cons の 0..n−1 の和、n(n−1)/2）、`tail_consume`（`new` を `List.tail` で空にしながら先頭の和、n(n−1)/2）、`clone_list`（Copy の複製と元の和、n(n−1)）、`index_walk`（`values[i]` の和、n(n−1)/2）、`string_list`（`"ab" + "c"` の n 個の長さの和、3n）。runner は `benchmarks/run-list-memory.mjs`。
- 理由: 一括生成・cons・tail・複製・索引・非 Copy 要素を分けて測る。`control` 系列は他言語の実装が要るので増やさない。
- 状態: 既定案（実装者はこの案に従う）

### D11: Phase 分割

- 決定: Phase 1 は表現の切り替え全体（「Phase 分割」）。Phase 2 は人間が求めた場合だけ着手する。
- 理由: 表現は全経路で同時に切り替える必要があり、Phase 1 をさらに分けると二つの表現が混在する。高速経路は計測と生成コードの確認を個別に要する。
- 状態: 既定案（実装者はこの案に従う）
