# PM06: 確保の除去とスタック化

| 項目 | 内容 |
| --- | --- |
| ID | PM06 |
| 分類 | メモリ |
| 優先度 | P1 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | PR03, PM07, PX02, D06 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（確保失敗・スタック枯渇の文言と、記憶域の表の「演算の結果はヒープ」の変更。承認前は手順 3 以降に着手しない） |
| 手本にする既存実装 | スタック上の値の追跡・実行時のアドレス判定・移動時のヒープ移送: `src/llvm_frame.rs` の `Frame`・`frame_value`・`frame_inner`・`bind_frames`・`frame_test`・`relocate`・`drop_framed` と `src/llvm.rs` の `remember_temporary`。入口ブロックの環境: `src/llvm.rs` の `stack_closure`。決定的な解析と予算超過時の fallback: `src/call_specialization.rs` の `Specializations`（`MAX_SPECIALIZATIONS`）。テストの形: `tests/storage.rs` の `temporary_operands_use_stack_storage`・`oversized_literals_keep_the_heap`、`tests/primitives.mjs` の `storage` 表 |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_escape.rs`（新規）, `src/runtime/string.ll`, `src/runtime/utf8string.ll`, `tests/storage.rs`, `tests/fixtures/storage/Storage.tz`, `tests/primitives.mjs`, `tests/call_specialization.rs`, `benchmarks/allocation/Main.tz`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`。起票時の一覧の `src/ownership.rs`・`src/call_specialization.rs`・`src/constants.rs` は変更しない（D3・D4） |
| 計測対象 | 確保の回数・bytes（PX01 の `alloc_calls`・`alloc_bytes`）、最大 RSS（`peak_rss`）、時間（`wall_time`）。種目は `benchmarks/computations` の `ce_std_option_owned`・`direct_std_option_owned` と `benchmarks/allocation/Main.tz`（新規）の 4 種目。`-O3` 後の IR に残る確保の呼び出し箇所の数 |

## 目的

外へ逃げない一時的な値のヒープ確保を、コンパイル時に除去するかスタックへ移す。
所有権により値の寿命が正確に分かる Tsuzuri では、C++ や Rust より積極的に確保を減らせる。確保の削減はメモリ量と時間の両方に効く。

対象は次の 3 種類に限る（D2）。`new` で作った値、再帰 union のノード、Option／Result の payload（HEAD でも箱に入れていない）は対象外。

1. 文字列リテラル同士の連結と、長さだけを使う連結（Phase 1）。
2. 外へ逃げない文字列の連結の結果を、関数の入口に置く固定長の小さな領域へ置く（Phase 2）。
3. 外へ逃げない局所の関数値の捕捉環境を、入口ブロックへ置く（Phase 2）。

実装者は Phase 1 と Phase 2 を行う。「対象外」の Phase 3 の案は人間が依頼した場合だけ扱う。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の状態欄で done であること（計数・記録の形式を使う）。確認: `grep -n "PX01\|PM06\|PM02\|PM07" _perfs/README.md`。
- PM02（関数値の表現）と PM07（複製の除去。文字列の連結に `append` を足す）が `doing` でないこと。両者と `make_closure` と連結の生成箇所を共有する。
  片方が done なら、その変更の上で「段ごとの変更」の関数名を読み替える（D7）。
- D1 が承認済みであること。承認前は手順 1・2（ベースラインと確保の計数テスト。IR を変えない）だけ行う。
- GUIDE §2.3 の基準コマンドと §14 の性能チケットの共通手順が成功し、手順 1 のベースライン（IR・計数・計測）を保存していること。

### 前提とする他チケットのインターフェース

- PX01: PX01 形式（`target/perf/<run_id>/<suite>.jsonl`）、metric `alloc_calls`・`alloc_bytes`・`peak_rss`・`wall_time`、runner の `--metrics <dir>`、
  `node benchmarks/metrics.mjs process ...`・`report ...`、`--baseline` のコンパイラによる variant `before`。
- PM07 D4: 左辺の frame が空の `Add` だけを `append` にする。PM06 の連結の領域は frame を持つので `append` の対象にならず、両立する。
- PM09: 入口ブロックの領域に `llvm.lifetime.*` を付ける作業は PM09 が行う。PM06 は寿命の印を出さない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 連結の領域のポインターを、それを受け取った値の静的な位置以外（union の payload、`Vec`、関数値の環境、Task）へ置く必要が生じた。
  `src/llvm_frame.rs` の不変条件（先頭のコメント）が崩れ、実行時のアドレス判定が効かなくなる。
- 追跡の計数で `live != 0`、magic の不一致による `abort`、または `TSUZURI_ASAN=1` の実行で AddressSanitizer の報告が出た。
- 「既存テストへの影響」に挙げた以外の既存の期待値（`tests/primitives.mjs` の `storage` 表の確保数、`tests/storage.rs`、`tests/call_specialization.rs` の IR）を変える必要が生じた。
- `rec` 関数・Task 本体の IR が変わった（D5 で対象外にしている）。深い再帰のテスト（テスト計画）が WASM で失敗した。
- stack-depth の 3 テスト（GUIDE §11.1）か `honors_the_exact_specialization_limit` が失敗した。上限・stack の大きさ・`MAX_VALUE_BYTES` を上げたくなった。
- 同じ入力で 2 回出した IR が一致しない。
- `src/llvm_traps.rs` が新しい runtime 関数のトラップを、元の関数（`concat`・`allocate`）と異なる種類に分類する。
- 所有権検査（`src/ownership.rs`）の変更、`unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状と計測（HEAD `f8dc655`）

### 記憶域の規則

- docs/language.md「スタックとヒープ（`new`）」の表は、`new` なしで `let` の右辺に書いた配列・リスト・レコード・タプルのリテラルをスタックに、
  文字列リテラルを静的領域の参照に置き、「関数・演算子・組み込み関数の結果（`f x`、`a + b`、`clone_string` など）| 呼び出し先・演算が確保したヒープ」と定める。
  束縛しない一時的なリテラルも、`.length`・索引・比較・連結の被演算子のようにその式で使い終わる場合はスタックに置く。
- 実装は `src/llvm_frame.rs`。`frame_value` は `frame_bytes` が `MAX_VALUE_BYTES`（`src/check.rs`、64 KiB）を超える木をヒープの経路へ回す。
  上限はリテラル 1 つあたりで、関数あたりの合計の上限はない。領域は入口ブロックの `alloca`（`tests/storage.rs` の `stack_literals_stay_in_the_entry_block_of_tail_loops`）。
- `Frame` は `Array`・`List`・`String { data, length }`・`Aggregate` の 4 種。値を束縛から移すときは `relocate` がスタックの部分をヒープへ写す。
  解放（`drop_framed`）は `frame_test` の `icmp eq ptr` によるアドレス判定で、スタックの部分を `@tz.free` に渡さない。
- 文字列の連結は `src/llvm.rs` の `FunctionEmitter::binary` が `take_operand` で両辺を取り、`@{runtime}.concat` を呼ぶ
  （コメント「Concatenation reads both operands and then destroys them, so stack strings stay put.」）。
  `src/runtime/string.ll` の `@tz.string.concat` は長さの和の符号なし overflow で `llvm.trap`、続く `@tz.string.allocate` は長さが 2^53 − 1 を超えると `llvm.trap`、
  それ以外は結果を必ず `@tz.alloc` で確保する（長さ 0 は確保しない）。
- 関数値の環境は `make_closure` が `@tz.alloc` で確保し、`@tz.env.drop.<name>.<count>`（`closure_wrappers`）が捕捉を解放してから `@tz.free` する。
  例外は immediate capture（`immediate_capture`）と、呼び先が既知で外へ逃げない引数の `stack_closure`（`src/call_specialization.rs` の worker）。
  `let` で束縛して局所で呼ぶだけのラムダ式はヒープの環境を持つ。
- Option／Result の payload は union の値の中に直接置く（`UnionLayout`）。箱に入れる確保はない。再帰 union のノードだけが `src/llvm_recursive.rs` でヒープに置かれる。
- エスケープ解析に相当する解析はない。`src/llvm.rs` の `clones_on_take` は Copy の複製の省略だけを扱う。

### 確保失敗とスタックの大きさ

- `TrapKind::AllocationFailure`（「allocation failed」）、`AllocationSize`（「allocation size overflow」）、`StringConcatOverflow`（「string length overflow」）が `src/trap.rs` にある。
- docs/language.md の現在の文言（D1 で変える対象）:
  - 冒頭: 「非停止、スタック枯渇、検査違反によるトラップはあり、全関数の停止は保証しません。」
  - 値の節: 「追加の `new`／`box` 構文はありません。確保失敗はトラップし、WASM のヒープ上限は16 MiBのままです。」
  - 組み込み関数の節: 「メモリ確保失敗もトラップします。トラップ時のスタック巻き戻しや destructor 実行は保証しません。」
  - スタックとヒープの節の最後: 「スタックの上限は `[再帰とスタック](#再帰とスタック)` のとおりです。」（docs/language.md 内のリンクとして書く）
- スタック: WASM の主スタックは 1 MiB（`src/driver.rs` の `stack-size=1048576`）。WASM threads の worker は 256 KiB（`src/runtime/task-wasm-threads.c` の
  `tsuzuri_thread_stack_size`）。native の Task の thread は `src/runtime/task.c` が属性 `NULL` で `pthread_create` するため OS の既定（macOS 512 KiB）。

### 計測済みの事実（2026-09-30 の probe、M1 Max、macOS 27.0）

- `benchmarks/computations` の `--emit llvm -O3` の IR で、`@tz.fn.Main.direct_option_owned_step` に `concat` の呼び出しが 2 つ、`@tz.free` が 7 つある。
  Option ビルダー版の `option_owned_step` は worker に特殊化され、その関数自身には呼び出しがない。IR 全体の `tz.string.concat(` は 6 箇所。
- 同じ IR を `opt -O3`（LLVM 21）に通しても、`@tz_ce_std_option_owned` と `@tz_direct_std_option_owned` のそれぞれに確保（`@malloc`・`@tz.alloc`・`@tz.string.allocate`）の
  呼び出し箇所が 1 つ、解放が 1 つ残る。LLVM は連結の確保を消せない。
- 起票時の記録: この種目の時間比は C++ の約 11 倍。参照実装が確保せず定数 12 を使うためで、PX02 が比較条件を直す。
- `tests/primitives.mjs` の `storage` 表の確保数（HEAD の期待値）: `concat_stack()` 2、`capture_stack()` 4、`escape_strings()` 3、`string_join_allocation()` 1、
  `temporary_stack()` 0、`tail_frames(100000)` 200000。

### 再現（2026-09-30 に確認）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
rm -rf /tmp/tz-work-PM06/bench && mkdir -p /tmp/tz-work-PM06/bench
cp benchmarks/computations/Main.tz benchmarks/computations/*.tc /tmp/tz-work-PM06/bench/
target/release/tsuzuri build /tmp/tz-work-PM06/bench --emit llvm -O3 -o /tmp/tz-work-PM06/bench-O3.ll
grep -c "tz.string.concat(" /tmp/tz-work-PM06/bench-O3.ll   # 6
/opt/homebrew/opt/llvm@21/bin/opt -O3 -S /tmp/tz-work-PM06/bench-O3.ll -o /tmp/tz-work-PM06/opt.ll
awk '/^define/{n=$0;a=0;f=0} /call .*@(tz\.alloc|tz\.string\.allocate|malloc)\(/{a++} /call .*@(tz\.free|free)\(/{f++} /^}/{if (n ~ /std_option_owned/) print a, f, substr(n,1,60)}' /tmp/tz-work-PM06/opt.ll
# 1 1 define i64 @tz_ce_std_option_owned(...
# 1 1 define i64 @tz_direct_std_option_owned(...
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（GUIDE §14）。

- G1（Phase 1）: 文字列リテラル同士の連結と、長さだけを使う連結で確保しない。
- G2（Phase 2）: 外へ逃げない連結の結果（128 UTF-16 code unit、または 256 UTF-8 byte 以下）と、外へ逃げない局所の関数値の環境で確保しない。
- G3: 外へ逃げる値の確保回数・bytes を増やさない。`rec` 関数と Task 本体の IR を変えない。時間と最大 RSS を悪化させない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 確保回数 | 回（1 回の実行の合計、PX01 の `alloc_calls`） | `computations` の `ce_std_option_owned`・`direct_std_option_owned`（`--quick` の規模） | Some の反復（`state & 7 != 0`）あたり 2 → 1（Phase 1）。None の反復は 0 のまま |
| M2 確保 bytes | bytes（同上、`alloc_bytes`） | M1 と同じ | Some の反復あたり `2 × 11 + 2 × 12` = 46 → 22 |
| M3 E2E の確保回数 | 回（関数 1 回の呼び出し、`tests/primitives.mjs` の追跡） | テスト計画の新しい `storage` の行 | 表の期待値のとおり |
| M4 時間 | ms（9 回の中央値、最小、最大。`wall_time`） | M1 の 2 種目と `benchmarks/allocation` の 4 種目 | 悪化しない（差が広がりの範囲内）。確保が消える種目は短くなる見込み |
| M5 最大 RSS | bytes（9 回の中央値。`peak_rss`） | `benchmarks/allocation` の `execute`（`-O3`） | 悪化しない |
| M6 残る確保の呼び出し箇所 | 個（静的） | `opt -O3` 後の `@tz_ce_std_option_owned`・`@tz_direct_std_option_owned` | 1 → 1（リテラルを `Some` に渡す移送が残る。0 には Phase 3 が要る） |
| M7 PM06 の領域 | bytes（関数ごと、静的） | `--emit llvm` の入口ブロックの PM06 の `alloca` の合計 | どの関数も 4,096 以下 |

M2 の計算: `"hello" + " world"` は 11 code unit で 22 bytes、`text + "!"` は 12 code unit で 24 bytes。Phase 1 後は `Some "hello world"` の移送（22 bytes）だけが残る。

## 変えてはいけない意味

- 評価順序: 連結の左辺、右辺、長さの overflow の検査、確保の大きさの検査、左辺の解放、右辺の解放の順（`FunctionEmitter::binary` の順）を保つ。
  長さだけの連結（D6）でも被演算子は両方とも評価し、同じ順で解放する。
- 所有権と解放の時点: 記憶域だけを変え、所有者・移動・解放の位置は変えない。関数値の捕捉の解放順は `@tz.env.drop.<name>.<count>` と同じ添字の順。
  B07（利用者の drop）が done なら、利用者の drop が走る位置と順序も同じ。スタックの値を外へ移すときは HEAD と同じく `relocate` がヒープへ移す。
- 決定的なトラップ: `StringConcatOverflow`（長さの和の overflow）と `AllocationSize`（UTF-16 の長さが 2^53 − 1 を超える）を、同じ条件・同じ種類・同じ順で残す。
  除去してよいのは環境に依存する `AllocationFailure` だけ（D1）。
- 文字列の表現: 空文字列はポインター null・長さ 0。連結の結果が空なら領域を使わない（D9）。code unit の内容・長さ・比較・索引の結果は同じ。
- native と WASM、`-O0` と `-O3` で結果・トラップ・確保回数が同じ。WASM の import は増やさない（D-18）。
- 生成 IR は決定的。PM06 の対象がない関数（文字列の `+`、局所の関数値がない関数）の IR は変えない。

## 設計

### 対象と規則

| 対象 | 条件 | 変換 | Phase |
| --- | --- | --- | --- |
| リテラルの連結 | 文字列の `Add` の木で、葉がすべて `TypedExprKind::String` | 一つの文字列リテラルとして生成する。以後は HEAD のリテラルの規則（静的領域・移送） | 1 |
| 長さだけの連結 | `StringLength` の被演算子が文字列の `Add`（リテラルの連結を除く） | 両辺の長さの和。検査と解放は保ち、結果の文字列を作らない | 1 |
| 連結の結果 | `frame_value` の文脈（`let` の初期化子、被演算子の一時値）の文字列の `Add`。`let` なら束縛が「留まる」（エスケープ解析） | 入口ブロックの 256 bytes の領域に書く。長さが容量を超えればヒープ | 2 |
| 局所の関数値の環境 | `let` の初期化子の `Closure(id, values)`（捕捉 1 つ以上、immediate capture でない）。束縛が「留まる」。環境の大きさが 1 KiB 以下 | 環境を入口ブロックの `alloca` に置く。捕捉の評価と移動は HEAD と同じ | 2 |

Phase 2 の変換は、関数が次のどれかなら行わない（D5）: 参照グラフの循環に含まれる（`rec` 関数と、それを参照するラムダ）、Task 本体（`CheckedFunction::is_task`）、
その関数の PM06 の領域の合計が 4,096 bytes を超える（ソース順で超えた箇所以降はヒープ）。

### エスケープ解析

- 役割: 性能の判断だけ。正しさは既存の frame の追跡が保証する（D3）。スタックの部分を持つ値が束縛から移るときは `relocate` がヒープへ写し、
  解放は `frame_test` で判定する。解析が誤って「留まる」と判定しても、移送が 1 回増えるだけで use-after-free にはならない。
- 入力: 所有権検査を通った `CheckedFunction::body`（使用後の移動などは既に拒否済み）。`src/ownership.rs` の結果は使わない。
- 規則: `staying_locals(body)`（新規）は、`Block` の束縛で `Local::mutable` が false の局所のうち、すべての `TypedExprKind::Local(id)` の出現が次のどれかであるものの集合を返す。
  - `StringLength(_)`・`Length(_)` の被演算子。文字列の `Binary(_, a, b)` の `a` または `b`（`Add` は被演算子を破棄するが、`drop_framed` が判定する）。
  - `Index(base, _)` の `base`。`ForEach { source, .. }` の `source`。`Borrow(_, false)` の中身。
  - `Call(callee, _)` の `callee`（場所の callee は `apply_value` へ `borrowed = true` で渡り、環境を消費しない）。
  それ以外の出現（戻り値、値渡しの実引数、捕捉、代入、`Borrow(_, true)`、`Match` の対象、`Slice`、record・配列の要素など）が一つでもあれば留まらない。
- 実行場所と費用: `FunctionEmitter::emit` の先頭、`call_specialization::single_use_locals` の直後で 1 回。`TypedExpr::children` を使う明示的なスタックの走査で O(本体の節点数)。
  固定点の反復はなく、予算の上限は要らない。再帰呼び出しを使わない（stack-depth の落とし穴）。
- 循環の判定: `recursive_functions(module)`（新規）が、各関数の本体に現れる `Function(FunctionRef::User(id))` と `Closure(id, _)` を辺とするグラフで、
  自己辺を持つか大きさ 2 以上の強連結成分に属する関数を true にする。Tarjan を明示的なスタックで実装し、`pub fn emit` の先頭で 1 回計算して `Globals` に置く。

### データ構造

```rust
// src/llvm_frame.rs
pub(super) const STRING_BUFFER_BYTES: usize = 256;        // （新規）連結 1 箇所の領域
pub(super) const MAX_ENVIRONMENT_BYTES: usize = 1024;     // （新規）スタックに置く環境の上限
pub(super) const MAX_FUNCTION_BUFFER_BYTES: usize = 4096; // （新規）関数あたりの PM06 の領域の合計

pub(super) enum Frame {
    // 既存の Array・List・String・Aggregate は変えない
    /// 連結の結果の code unit が入口ブロックの `buffer` にあるかもしれない（新規）
    Buffer { buffer: String },
    /// 関数値の環境が入口ブロックの `environment` にある（新規）。`ty` は環境の LLVM 型
    Environment { environment: String, ty: String, function: usize, count: usize },
}

// src/llvm.rs の FunctionEmitter に足す field（新規）
staying: BTreeSet<usize>,   // staying_locals の結果
buffer_bytes: usize,        // この関数で使った PM06 の領域
heap_initializer: bool,     // 留まらない let の初期化子を生成している間だけ true

// src/llvm.rs の Globals に足す field（新規）
recursive: Vec<bool>,       // recursive_functions の結果（関数 id で引く）
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 生成 | `src/llvm_frame.rs` | `literal_concat`（新規） | `fn literal_concat(expression: &TypedExpr) -> Option<StringLiteral>`。`String` はそのまま、文字列の `Binary(BinaryOp::Add, a, b)` は両辺が `Some` のとき内容を連結する。明示的なスタックで走査する |
| 生成 | `src/llvm.rs` | `FunctionEmitter::binary` | 文字列の `Add` の先頭で `literal_concat` が `Some` なら、同じ型・span の `TypedExprKind::String` を作り `self.expression` へ渡す |
| 生成 | `src/llvm_frame.rs` | `emit_frame_inner` | `literal_concat` が `Some` の `Add` は `String` の arm と同じ処理。それ以外の文字列の `Add` は `frame_concat`（新規）、`Closure` は `frame_closure`（新規）。どちらも条件（対象と規則の表）を満たさなければ既存の fallback |
| 生成 | `src/llvm.rs` | `FunctionEmitter::expression` の `TypedExprKind::StringLength` の arm | 被演算子がリテラルでない文字列の `Add` なら `concat_length`（新規）。D6 の順で `take_operand`・検査・`drop_framed` |
| 生成 | `src/llvm_frame.rs` | `frame_concat`（新規） | 領域の `alloca [256 x i8], align 16` を `self.allocas` に足し、`take_operand` で両辺を取り `@{runtime}.concat_into` を呼び、`binary` と同じ順で両辺を `drop_framed`。frame は `Buffer` |
| 生成 | `src/llvm_frame.rs` | `frame_closure`（新規） | 捕捉の値を HEAD の `Closure` の生成と同じ順・同じ移動で評価し、`stack_closure` と同じ形で環境を `alloca` に置く。記述子は `closure_descriptor`（`@tz.env.drop` を持つ通常の形）。frame は `Environment` |
| 生成 | `src/llvm_frame.rs` | `frame_test` | `Buffer`: 値の field 0 が `buffer` と等しく、長さが 0 でない。`Environment`: `%tz.closure` の field 1 が `environment` と等しい |
| 生成 | `src/llvm_frame.rs` | `relocate`, `heap_copy` | `heap_copy` に引数 `value: &str` を足す（既存の arm は使わない）。`Buffer`: `@{runtime}.new(ptr buffer, i64 <値の長さ>)`。`Environment`: `@tz.alloc` で環境の大きさを確保し、環境を load・store で写して field 1 を差し替える |
| 生成 | `src/llvm_frame.rs` | `drop_frame_contents` | `Buffer`: 何もしない。`Environment`: 捕捉を添字の順に load して `drop_value`（`@tz.env.drop` の本体と同じ順。`@tz.free` は呼ばない） |
| 生成 | `src/llvm_frame.rs` | `read_operand`, `take_operand` | 評価の間だけ `heap_initializer` を false にして戻す（被演算子の一時値は留まる） |
| 生成 | `src/llvm.rs` | `Block` の束縛を生成する箇所（`bind_frames` を呼ぶ箇所） | 初期化子の生成の前後で `heap_initializer = !self.staying.contains(&local.id)` を設定・復元する |
| 生成 | `src/llvm.rs` | `FunctionEmitter::emit`, `FunctionEmitter::new`, `pub fn emit`, `Globals` | field の初期化、`staying_locals` と `recursive_functions` の呼び出し |
| 生成 | `src/llvm.rs` | `Frame` を match する他の箇所（`remember_temporary` など。`grep -n "Frame::" src/llvm.rs src/llvm_*.rs` で全件） | 新しい 2 variant の arm。`_ =>` の fallback で黙って通さない |
| 解析 | `src/llvm_escape.rs`（新規） | `staying_locals`, `recursive_functions` | `src/llvm.rs` に `#[path = "llvm_escape.rs"] mod llvm_escape;` を既存の `llvm_frame` と同じ形で足す。単体テストを同じファイルに置く |
| runtime | `src/runtime/string.ll`, `src/runtime/utf8string.ll` | `@tz.string.concat_into`（新規）, `@tz.utf8string.concat_into`（新規） | `concat` の直後に置く（PM07 D5 の `append` とは別の関数）。IR は「生成 IR とランタイム」 |
| トラップ | `src/llvm_traps.rs` | 関数名による分類 | 変更なし。`concat_into` は名前に `concat` を含み `StringConcatOverflow`、ヒープの経路は `.allocate` を呼び `AllocationSize` になることを手順 7 で確かめる |
| 変更なし | `src/ownership.rs`, `src/call_specialization.rs`, `src/constants.rs`, `src/check.rs`, `src/closures.rs` | — | 型付き IR と所有権検査は変えない（D3・D4） |

### 生成 IR とランタイム

UTF-16 の形（UTF-8 は `%tz.utf8string`、単位 1 byte、`fits` の検査なし、容量 256）。

```llvm
define internal %tz.string @tz.string.concat_into(ptr %buffer, i64 %capacity, %tz.string %left, %tz.string %right) nounwind {
entry:
  ; %a, %an, %b, %bn と %length・%overflow は @tz.string.concat と同じ。overflow なら llvm.trap
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %zero, label %size
zero:
  ret %tz.string zeroinitializer
size:
  %small = icmp ule i64 %length, %capacity
  br i1 %small, label %local, label %heap
local:
  ; @tz.string.copy で %buffer に左辺・右辺を写し、{ %buffer, %length } を返す
heap:
  ; @tz.string.concat の allocate 以降と同じ（@tz.string.allocate を呼ぶ）
}
```

呼び出し側（`frame_concat`、容量は UTF-16 で 128）:

```llvm
%buf.3 = alloca [256 x i8], align 16          ; 入口ブロック（self.allocas）
%r = call %tz.string @tz.string.concat_into(ptr %buf.3, i64 128, %tz.string %lhs, %tz.string %rhs)
```

長さだけの連結（`concat_length`、UTF-16）は runtime を呼ばず、`add i64`、`icmp ult`、`guard(.., TrapKind::StringConcatOverflow)`、
`icmp ule i64 %length, 9007199254740991`、`guard(.., TrapKind::AllocationSize)` の順に出す。UTF-8 は後ろの検査を出さない（`@tz.utf8string.allocate` にない）。

### Phase 分割

| Phase | 手順 | 内容 | 出荷 |
| --- | --- | --- | --- |
| 1 | 3〜5 | リテラルの連結、長さだけの連結 | 単独で出荷できる |
| 2 | 6〜10 | `llvm_escape`、連結の領域、局所の関数値の環境 | Phase 1 の後 |
| 3 | なし | 「対象外」の案。人間が依頼した場合だけ別チケットで扱う | — |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので `running N tests` の N を必ず見る。
重い `.mjs` は Node 24 で動かす。以下の `node24` は `alias node24='npx --yes --package=node@24 node'` の略。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 を実行し、HEAD の `target/release/tsuzuri` を `/tmp/tz-pm06/before-tsuzuri` に写す。「再現」と「計測手順」の before を取る。
  `/tmp/tz-pm06/cases/Main.tz` にテスト計画の E2E の関数と `def main :: i64 = ...`（全関数の和）を置き、`--emit llvm`（`-O0`・`-O3`、native・`--target wasm32`）を保存する。
- 確認: 「再現」の出力が一致する。stack-depth の 3 テストと `honors_the_exact_specialization_limit` がそれぞれ `1 passed`。
  `node24 tests/primitives.mjs target/release/tsuzuri` が成功する（手順 2 の `deep_recursion(10000)` が WASM で通らなければ 5000 に下げて記録する）。

### 手順 2: 確保の計数テストを先に足す（IR は変えない）

- 変更: `tests/fixtures/storage/Storage.tz`、`tests/primitives.mjs` の `storage` 表と生成部（`largeLiteral` と同じ位置）。
- 内容: テスト計画の E2E の行を HEAD の確保数で足す。`boundary_concat` の 2 つのリテラルは JS の `"x".repeat(128)`（`n > 0`）と `"x".repeat(127)` で生成する。
- 確認: `node24 tests/primitives.mjs target/release/tsuzuri` が成功し、出力の `stack/heap storage cases` の数が 14 増える（UTF-8 の 2 行を含む）。

### 手順 3: リテラルの連結（Phase 1、D1 承認後）

- 変更: `src/llvm_frame.rs` の `literal_concat`（新規）と `emit_frame_inner`、`src/llvm.rs` の `FunctionEmitter::binary`、`tests/storage.rs`。
- 内容: 「段ごとの変更」の最初の 3 行。`literal_concatenation_is_one_constant` を足す。表の `literal_concat_stack()` を 1 → 0 にする。
- 確認: `cargo test --locked --test storage` が HEAD より 1 多い N で成功。`cargo build --release --locked && node24 tests/primitives.mjs target/release/tsuzuri` が成功。

### 手順 4: 長さだけの連結（Phase 1）

- 変更: `src/llvm.rs` の `StringLength` の arm と `concat_length`（新規）、`tests/storage.rs`。
- 内容: D6 の順で出す。`length_only_concatenation_keeps_the_checks` を足す。表の `length_only_concat(1)`・`(0)` を 1 → 0 にする。
- 確認: `cargo test --locked --test storage` が手順 3 より 1 多い N で成功。`node24 tests/primitives.mjs target/release/tsuzuri` と `node24 tests/strings.mjs target/release/tsuzuri` が成功。

### 手順 5: Phase 1 の全体確認

- 変更: なし（失敗の修正だけ）。
- 内容: GUIDE §3 の全 suite を native/WASM × `-O0`/`-O3` で流す。`TSUZURI_ASAN=1` で `tests/computations.mjs` と `tests/features.mjs` を流す。
- 確認: `cargo test --locked` と全 `.mjs` が成功。ASan の報告なし。手順 1 の stack-depth の 4 テストが成功。ここで Phase 1 を出荷できる。

### 手順 6: `src/llvm_escape.rs`（IR は変えない）

- 変更: `src/llvm_escape.rs`（新規）、`src/llvm.rs` の `#[path]` 宣言、`Globals`・`FunctionEmitter` の field と初期化。
- 内容: `staying_locals`・`recursive_functions` を明示的なスタックで実装し、計算して保持するだけで使わない。単体テスト 2 つを同じファイルに置く。
- 確認: `cargo test --locked --lib llvm_escape` が `2 passed`。手順 1 の IR と `diff` で差がない。

### 手順 7: runtime の `concat_into`

- 変更: `src/runtime/string.ll`、`src/runtime/utf8string.ll`。
- 内容: 「生成 IR とランタイム」の形で `concat` の直後に置く。まだ呼ばない。
- 確認: `cargo test --locked` が成功。`cargo build --release --locked` の後、手順 1 の `--emit llvm` を作り直して `/opt/homebrew/opt/llvm@21/bin/llvm-as` が通る。
  `src/llvm_traps.rs` の分類で `@tz.string.concat_into` 内の trap が `StringConcatOverflow` になることを読んで確かめる。

### 手順 8: 連結の領域（Phase 2）

- 変更: `Frame::Buffer`（新規）、`frame_concat`（新規）、`frame_test`・`relocate`・`heap_copy`・`drop_frame_contents`、`read_operand`・`take_operand`、
  `src/llvm_control.rs` と `src/llvm.rs` の `bind_frames` を呼ぶ箇所（`heap_initializer`）、`grep -n "Frame::" src/llvm*.rs` の全件。
- 内容: 予算と対象外の関数（D5）を `frame_concat` の入口で判定する。Rust テスト 4 つを足す。表の `buffer_concat`・`boundary_concat(0)`・UTF-8 の行・`concat_stack()` を更新する。
- 確認: `cargo test --locked --test storage` が手順 4 より 4 多い N で成功。`node24 tests/primitives.mjs`・`tests/strings.mjs`・`tests/computations.mjs`（引数 `target/release/tsuzuri`）が成功。

### 手順 9: 局所の関数値の環境（Phase 2）

- 変更: `Frame::Environment`（新規）、`frame_closure`（新規）、手順 8 と同じ frame の関数、`tests/storage.rs`、`tests/primitives.mjs`。
- 内容: 捕捉の評価は `src/llvm.rs` の `TypedExprKind::Closure(id, captures)` の arm と同じ処理を関数に切り出して共有する（写さない）。Rust テスト 3 つ。
  表の `local_closure(10)` を 1 → 0、`capture_stack()` を 4 → 3 にする。
- 確認: `cargo test --locked --test storage` が手順 8 より 3 多い N、`--test call_specialization` が HEAD と同じ N で成功。`node24 tests/primitives.mjs`・`tests/tasks.mjs` が成功。

### 手順 10: Phase 2 の全体確認

- 変更: なし。
- 内容: 手順 5 と同じ。加えて `ASAN_OPTIONS=detect_stack_use_after_return=1 TSUZURI_ASAN=1` で `tests/features.mjs` を流す。
- 確認: 手順 5 と同じ。`rec_concat`・`deep` の関数本体が手順 1 の IR と一致する。

### 手順 11: 生成コードの確認と計測

- 変更: なし。
- 内容: 「生成コードの確認」「計測手順」を行い、結果を `docs/benchmarks.md` に記録する。
- 確認: 各 pattern が期待どおり。`target/perf/PM06-before`・`PM06-after` の JSONL が `readRecords` を通る。

### 手順 12: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/language-reference/ownership.md` が成功。`git diff --check` が空。

## 計測手順

PX01 の計測手順「before／after（他チケットの共通手順）」に従い、run_id を `PM06-before`（手順 1、HEAD）と `PM06-after`（手順 11）にする。
環境（CPU、OS、clang・rustc・node の版、commit）は PX01 形式の `host` 欄に入る。warm-up 1 回、標本 9 回、中央値・最小・最大を見る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
alias node24='npx --yes --package=node@24 node'
node24 benchmarks/run-computations.mjs --baseline /tmp/tz-pm06/before-tsuzuri --metrics target/perf/PM06-after
node24 benchmarks/metrics.mjs process --project benchmarks/allocation --metrics target/perf/PM06-after
node24 benchmarks/metrics.mjs report target/perf/PM06-before target/perf/PM06-after
```

`benchmarks/allocation/Main.tz`（新規）は次の内容（2026-09-30 に HEAD で `run` を確認。出力 `667000000` は、各関数を手で計算した和
5,500,000 + 648,000,000 + 8,000,000 + 5,500,000 と一致）。

```tsuzuri
def keep_text :: string -> string
fn keep_text text = text

def length_only_concat :: i64 -> i64
fn length_only_concat n =
    let a = if n > 0 then "ab" else "abc"
    (a + "xyz").length

def buffer_concat :: i64 -> i64
fn buffer_concat n =
    let a = if n > 0 then "ab" else "abc"
    let joined = a + "xyz"
    joined.length * 100 + joined[1] as i64

def buffer_escape :: i64 -> i64
fn buffer_escape n =
    let a = if n > 0 then "ab" else "abc"
    let joined = a + "xyz"
    (keep_text joined).length

def local_closure :: i64 -> i64
fn local_closure n =
    let a = n + 1
    let b = n * 2
    let add = \i -> i + a + b
    add 1 + add 2

def run :: (i64 -> i64) -> i64 -> i64
fn run step count =
    let mut total = 0
    for i in new [i64](count, i -> i) do total = total + step (i & 1)
    total

def main :: i64 = run length_only_concat 1000000 + run buffer_concat 1000000 + run local_closure 1000000 + run buffer_escape 1000000
```

`run` の `step` は関数値の間接呼び出しで、各種目の関数は `run` へ特殊化されない（PR03 の変更の影響を受けにくい）。`buffer_escape` は対照で、確保数は変わらない。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for o in O0 O3; do
  target/release/tsuzuri build /tmp/tz-pm06/cases --emit llvm -$o -o /tmp/tz-pm06/after-native-$o.ll
  target/release/tsuzuri build /tmp/tz-pm06/cases --emit llvm -$o --target wasm32 -o /tmp/tz-pm06/after-wasm32-$o.ll
done
/opt/homebrew/opt/llvm@21/bin/opt -O3 -S /tmp/tz-pm06/after-native-O3.ll -o /tmp/tz-pm06/after-opt.ll
target/release/tsuzuri build /tmp/tz-pm06/cases --emit object -O3 -o /tmp/tz-pm06/after.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d /tmp/tz-pm06/after.o > /tmp/tz-pm06/after.s
```

| 関数（`@tz.fn.Main.*`） | 期待する pattern（`-O0` の IR、native と wasm32） |
| --- | --- |
| `literal_concat_stack` | `.concat(` と `@tz.alloc` がない |
| `length_only_concat` | `.concat` と `@tz.alloc` がない。`icmp ult i64` と `9007199254740991` がある |
| `buffer_concat` | 入口ブロックに `alloca [256 x i8], align 16`。`@tz.string.concat_into(ptr` と `i64 128`。`@tz.string.concat(` がない |
| `buffer_escape`, `closure_escape` | 手順 1 の同じ関数と一致（`diff` が空） |
| `local_closure` | `@tz.alloc` がない。入口ブロックに環境の `alloca` |
| `rec_concat`, `deep` | 手順 1 の同じ関数と一致 |

`after-opt.ll` では `buffer_concat`・`length_only_concat`・`local_closure` の本体に `@malloc`・`@tz.alloc` の呼び出しがない。M6 は「再現」の awk で数える。
`after.s` では `buffer_concat` に `bl _malloc` がなく、prologue の `sub sp, sp, #N` が before より大きい（256 bytes 前後）。

## テスト計画

### Rust テスト

`tests/storage.rs`（既存の `emit`・`function`・`entry` を使う。`emit` は IR を 2 回出して一致を確かめる）:

- `literal_concatenation_is_one_constant`: `"hello" + " world"`・`"a" + "b" + "c"` の束縛と一時値で `.concat(` がない（native・wasm）。
- `length_only_concatenation_keeps_the_checks`: `(a + "xyz").length` に `.concat` と `@tz.alloc` がなく、overflow と `9007199254740991` の検査がある。UTF-8 版には後者がない。
- `local_concatenation_uses_a_bounded_buffer`: 入口ブロックに `alloca [256 x i8]`、`concat_into` の容量が UTF-16 で `i64 128`、UTF-8 で `i64 256`。
- `escaping_concatenation_keeps_the_heap`: 束縛を返す・値渡しする・捕捉する・`ref mut` で貸す 4 形で `.concat(` を使い、`[256 x i8]` がない。
- `buffer_budget_falls_back_to_the_heap`: 1 関数に留まる連結の束縛を 17 個書くと `alloca [256 x i8]` が 16 個、`.concat(` が 1 個。
- `recursive_functions_keep_heap_storage`: `rec` 関数、相互再帰の `and`、`rec` 関数を呼ぶラムダで `concat_into` と環境の `alloca` がない。
- `local_closures_use_entry_block_environments`: `local_closure` に `@tz.alloc` がない。
- `escaping_closures_keep_heap_environments`: 関数値を値渡し・返却・捕捉すると `@tz.alloc` が 1 回。
- `oversized_environments_keep_the_heap`: 65 個の `i64` を捕捉する（16 × 65 = 1,040 > 1,024 bytes）と `@tz.alloc` を使う。ソースは Rust の `format!` で生成する。
- `src/llvm_escape.rs` の単体テスト `staying_locals_accept_reads_and_reject_moves`、`recursive_functions_mark_cycles_through_lambdas`。

### E2E

`tests/primitives.mjs` の `storage` 表（native `-O0`/`-O3` で確保数と `live == 0`、WASM で結果と import なし）。関数の形は「計測手順」の同名の関数と
2026-09-30 に HEAD で確認した probe（`/tmp/tz-work-PM06/cases`、`run` の出力が手計算の和 1920 と一致）。結果は手で計算した値。

| 呼び出し | 結果 | HEAD | Phase 1 後 | Phase 2 後 |
| --- | --- | --- | --- | --- |
| `literal_concat_stack()` | 115（11 + `'h'` 104） | 1 | 0 | 0 |
| `length_only_concat(1)`・`(0)` | 5・6 | 1・1 | 0・0 | 0・0 |
| `buffer_concat(1)`・`(0)` | 598・698 | 1・1 | 1・1 | 0・0 |
| `buffer_escape(1)` | 5 | 1 | 1 | 1 |
| `boundary_concat(0)`・`(1)`（127・128 code unit に `"y"`） | 128・129 | 1・1 | 1・1 | 0・1 |
| `local_closure(10)` | 65（32 + 33） | 1 | 1 | 0 |
| `closure_escape(10)`（`apply_one add`） | 32 | 1 | 1 | 1 |
| `rec_concat(3)` | 8（3 + 3 + 2 + 0） | 4 | 4 | 4 |
| `deep_recursion(10000)` | 10000 | 0 | 0 | 0 |

`literal_concat_stack` は `let text = "hello" + " world"` と `text.length + text[0] as i64`。`rec_concat` は `def rec rec_concat :: i64 -> i64`・`fn rec rec_concat n =`、
`let t = (if n > 1 then "ab" else "a") + "c"`、`if n == 0 then 0 else t.length + rec_concat (n - 1)`。`deep` は `fn rec deep n = if n == 0 then 0 else 1 + deep (n - 1)`。
UTF-8 の行は既存の `utf8_join_allocation` と同じ書き方で UTF-8 文字列を作り、`buffer_concat` と同じ形の 2 行（`n = 1`・`0`）を足す。
意味の保持: GUIDE §3 の全 suite（`tests/strings.mjs`・`tests/computations.mjs`・`tests/tasks.mjs`・`tests/features.mjs` ほか）を native/WASM × `-O0`/`-O3` で流す。

### 既存テストへの影響

- `tests/primitives.mjs` の `concat_stack()` 2 → 0（Phase 2。`joined`・`prefixed` が留まる）、`capture_stack()` 4 → 3（Phase 2。環境だけがスタックへ移り、捕捉した配列の移送は残る）。
- runtime の IR に `concat_into` の定義が 2 つ増える。それ以外の期待値は変えない（変わったら停止条件）。

### 性能

「計測手順」の記録だけ。速度の合否閾値は置かない。

## ドキュメント

- `docs/language.md`: 冒頭、値の節、「スタックとヒープ（`new`）」の表と新しい段落「確保の省略」、組み込み関数の節の確保失敗、「再帰とスタック」（D1 の文言）。
- `docs/architecture.md`: モジュール表の `src/llvm_frame.rs` の行（連結の領域・環境）と `src/llvm_escape.rs`（新規）の行、「不変条件」の frame の不変条件。
- `docs/benchmarks.md`: 「コンピュテーション式の比較」の「条件・出力・確保数」の `std_option_owned` の確保数と、PM06 の before／after の記録。
- `_docs/language-reference/ownership.md`: 「記憶域と new」に確保の省略を 1 段落。`_perfs/README.md` の状態を done にする。

## 受け入れ条件

- [ ] D1 が承認され、docs/language.md の文言が D1 のとおり。
- [ ] Phase 1・2 の E2E の表の確保数が native `-O0`/`-O3` で期待どおり、`live == 0`。WASM の結果が一致し import がない。
- [ ] 「生成コードの確認」の pattern をすべて満たし、`rec` 関数・Task 本体・外へ逃げる形の IR が HEAD と同じ。
- [ ] `cargo test --locked`、全 `.mjs`、ASan（`detect_stack_use_after_return=1`）、stack-depth の 4 テストが成功。
- [ ] `PM06-before`・`PM06-after` の PX01 形式の記録があり、`docs/benchmarks.md` に実測値だけを書いた。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 領域は必ず `self.allocas`（入口ブロック）に置く。ループの本体で `alloca` するとループごとにスタックが伸びる。`stack_literals_stay_in_the_entry_block_of_tail_loops` と同じ形で確かめる。
- `Buffer` の `frame_test` は長さを定数と比べない（実行時の長さ）。長さ 0 は null なので `length != 0` で移動済みの slot と区別する。
- `heap_copy` の引数の追加で既存の 3 arm を直す。`_ =>` の fallback に新しい variant を落とさない（`unreachable!` が走る）。
- 予算は生成の順（ソース順）で数える。`HashMap` を使うと IR が非決定的になる。
- `frame_closure` は既存の `stack_closure`（呼び先が既知の引数）の判定を奪わない。`tests/call_specialization.rs` の期待値が変わったら判定の順序の誤り。
- `Environment` の解放順は `closure_wrappers` の `@tz.env.drop` の本体を読んで合わせる。捕捉の型が drop を要らなければ何も出さない。
- `llvm_escape` の走査を再帰関数で書くと深い式で stack を使い切る（GUIDE §11.1）。`TypedExpr::children` と `Vec` のスタックで書く。
- スタック枯渇は WASM では範囲外のメモリアクセスのトラップ、native では SIGSEGV（`tsuzuri run` は E2005）。上限を上げて通さない。
- `.length` の被演算子が place（束縛）なら `read_operand` の経路で連結はない。`concat_length` は `Add` の一時値だけを扱う。

## 対象外

- `new` の値（明示のヒープ）、再帰 union のノード、Option／Result の payload（HEAD でも箱に入れない）、レコード・タプル（値としてスタック）。
- Phase 3 の案（人間が依頼した場合だけ）: union の payload に frame を通して `Some (a + b)` の確保を消す、配列の連結（`Builtin::ArrayConcat`）、文字列の組み込み関数（join・replace・repeat）、
  同じ寿命の確保の統合、実行時の長さの `alloca`。
- 確保のプール化（PM05）、GC、寿命の印（PM09）、連結の再利用と `append`（PM07）、展開と特殊化（PR03）、関数値の表現（PM02）。

## 決定事項

### D1: 確保失敗・スタック枯渇の文言と記憶域の表

- 決定: docs/language.md を次のとおり変える。冒頭の文の後に「スタック枯渇が起きる再帰の深さは規定せず、コンパイラの版や最適化で変わります。」を足す。
  値の節は「確保失敗（メモリ不足）はトラップします。確保失敗は環境に依存する資源の失敗で、コンパイラが確保を省いた箇所では起きません。WASM のヒープ上限は16 MiBのままです。」。
  組み込み関数の節は「メモリ確保失敗もトラップします（確保を省いた箇所を除く）。」。記憶域の表の演算の行に「（下の確保の省略を除く）」を足し、段落「確保の省略」を置く:
  「文字列リテラル同士の連結は一つのリテラルになり、長さだけを使う連結は文字列を作りません。外へ逃げない連結の結果は関数の入口の 256 バイトの領域に収まればそこに置き、
  外へ逃げない局所の関数値の環境は 1 KiB 以下ならスタックに置きます。一つの関数で 4 KiB まで、再帰する関数と Task の本体では行いません。
  評価順序・所有権・借用・解放の時点と、長さ・確保サイズの overflow のトラップは変わりません。」
- 理由: 表の「演算の結果はヒープ」と確保失敗が起きる位置は観測できる仕様であり、それを変える。決定的なトラップは残すので入力で決まる結果は変わらない。
- 状態: 要承認（承認前は手順 3 以降に着手しない）

### D2: 対象の範囲

- 決定: 対象は「設計」の対象と規則の表の 4 形だけ。Option／Result の payload は HEAD でも union の値に直接置くので箱の除去はない。一時的な集成体は HEAD でもスタック。
- 理由: 起票時の「配列・レコード」は HEAD の frame で既に扱われ、`new` は利用者が明示したヒープである。
- 状態: 既定案（実装者はこの案に従う）

### D3: エスケープ解析の役割

- 決定: 解析は「ヒープに直接作るか」の判断だけに使う。正しさは frame の追跡・`relocate`・`frame_test` が保証する。`src/ownership.rs` は変えない。
- 理由: 解析の誤りが use-after-free にならず、所有権検査の変更（PR01 の範囲）も要らない。
- 状態: 既定案（実装者はこの案に従う）

### D4: リテラルの連結を畳む場所

- 決定: 生成時（`binary` と `emit_frame_inner`）に `literal_concat` で畳む。`src/constants.rs` と型付き IR は変えない。
- 理由: 所有権検査・formatter・LSP が見る木を変えず、2 箇所の変更で済む。
- 状態: 既定案（実装者はこの案に従う）

### D5: スタックの上限と対象外の関数

- 決定: 連結の領域は 1 箇所 256 bytes、環境は 1 KiB 以下、関数あたり合計 4,096 bytes。参照グラフの循環に入る関数と Task 本体は対象外。
- 理由: 最も小さい WASM threads の worker スタック（256 KiB）に対して 1 関数の増分を 1.6% 以下に抑える。循環に入らない関数は、関数値を経由しない限り一つのスタックに一度しか現れず、
  再帰の深さによる増分を避けられる。関数値経由の繰り返しは D1 の「深さは規定しない」で扱う。
- 状態: 既定案（実装者はこの案に従う）

### D6: 長さだけの連結の検査

- 決定: 左辺・右辺を `take_operand` で取り、長さの和の overflow（`StringConcatOverflow`）、UTF-16 だけ 2^53 − 1 の検査（`AllocationSize`）、左辺・右辺の `drop_framed` の順に出す。
- 理由: `@tz.string.concat` と `@tz.string.allocate` の検査の条件・種類・順序と同じ。`@tz.utf8string.allocate` に検査はない。
- 状態: 既定案（実装者はこの案に従う）

### D7: PM02・PM07 との関係

- 決定: 同時に `doing` にしない。PM02 が先なら `Environment` の判定は新しい表現の環境の field を読む。PM07 が先なら `Buffer` の frame を持つ左辺は `append` の対象外のまま。
- 理由: `make_closure`・`closure_descriptor` と連結の生成箇所を共有する。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測の種目

- 決定: `computations` の 2 種目と `benchmarks/allocation/Main.tz`（新規、process suite）。runner と suite は増やさない。確保数は E2E の追跡と `run-computations.mjs --metrics` で取る。
- 理由: PX02 の種目の変更と衝突せず、PX01 の既存の経路だけで M1〜M5 が取れる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 空文字列

- 決定: `concat_into` は長さ 0 で `zeroinitializer` を返し、領域を使わない。
- 理由: 空文字列は null・長さ 0 という既存の表現を保ち、`frame_test` の `length != 0` と合う。
- 状態: 既定案（実装者はこの案に従う）

### D10: 容量の単位

- 決定: 領域は両方の符号化で 256 bytes。容量は UTF-16 で 128 code unit、UTF-8 で 256 byte。
- 理由: frame の大きさを符号化によらず一定にし、予算の計算を単純にする。
- 状態: 既定案（実装者はこの案に従う）
