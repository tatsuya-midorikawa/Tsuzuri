# PM09: スタック使用量の削減と末尾呼び出しの保証

| 項目 | 内容 |
| --- | --- |
| ID | PM09 |
| 分類 | メモリ |
| 優先度 | P3 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | PR01, PM06, E14, B08 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D1（Phase 2: 末尾呼び出しの保証。docs/language.md「再帰とスタック」の保証を広げる言語仕様の変更）, D6（Phase 2: CLI の値 `--wasm-feature tail-call` の追加） |
| 手本にする既存実装 | 末尾位置の判定と「drop してから制御を移す」順序: `src/llvm.rs` の `FunctionEmitter::emit_tail`・`tail_arguments`・`is_self`・`drop_all`。フレーム領域の確保と解放の判定: `src/llvm_frame.rs` の `frame_array`・`frame_list`・`frame_value`・`release_operand`・`frame_test`。WASM の opt-in 機能: `src/main.rs` の `--wasm-feature`（`wasm_simd`・`wasm_threads`）と `src/driver.rs` の `-msimd128`／`-mno-simd128`。IR の形の検査: `tests/tail_recursion.rs` の `function_ir` |
| 主な影響ファイル | `src/llvm.rs`, `src/llvm_frame.rs`, `src/check.rs`（`MAX_VALUE_BYTES` の隣に定数）, `src/main.rs`, `src/driver.rs`, `tests/tail_recursion.rs`, `tests/stack_frames.rs`（新規）, `tests/control.mjs`, `tests/fixtures/control/Frames.tz`（新規）, `tests/fixtures/control/TailCalls.tz`（新規、Phase 2）, `tests/stack.mjs`（新規、Phase 2）, `tests/fixtures/stack/Main.tz`（新規、Phase 2）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_docs/language-reference/functions.md`, `_docs/guides/performance.md`, `_perfs/README.md` |
| 計測対象 | 関数ごとのフレームの大きさ（clang `-fstack-usage` の `.su`、bytes）、深さ 10,000,000 の相互再帰が完走するか（native・WASM × `-O0`／`-O3`）、PX01 の `control` suite の時間（退行がないことの確認） |

## 目的

関数のフレームを小さくし、相互再帰や継続渡しの関数型の書き方でも一定のスタックで動くようにする。
スタックの消費を減らすことで、深い再帰でも省メモリで動作し、WASM の 1 MiB のスタックでも処理できる範囲を広げる。

二つの独立した施策からなる。

- Phase 1（承認不要・実装対象）: フレームの縮小。スタックに置くリテラルの領域へ `llvm.lifetime.start`／`llvm.lifetime.end` を付け、
  関数あたりのフレーム領域に上限を設け、スタックの大きさと枯渇時の挙動を文書にする。言語の意味は変えない。
- Phase 2（要承認 D1・D6）: 末尾呼び出しの保証。rec グループ内の末尾位置の直接呼び出しに `musttail` を付け、`-O0` でも定数スタックにする。
  WASM は `--wasm-feature tail-call`（新規）を指定したときだけ使う。

実装者は Phase 1 だけを実装する。Phase 2 は D1 と D6 が承認され、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の状態欄で done であること（HEAD では todo）。確認: `grep -n "| PX01\|| PM09" _perfs/README.md`。
  PX01 が todo の間は、計測手順の `metrics.mjs` を使う部分を除いて進めてよい（フレームの大きさは `.su` だけで測れる）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR・`.su`・深さの結果）を `/tmp/tz-pm09/before/` に保存していること。
- Phase 2 は D1 と D6 の承認後。承認前は Phase 2 のどの手順（手順 8 以降）にも着手しない。
- PM06 と同時に進めない。PM06 は「入口ブロックの領域に `llvm.lifetime.*` を付ける作業は PM09 が行う」と定めており、
  PM06 がスタックへ移す領域にも本チケットの規則（D2・D3）が適用される。先に完了した側の IR を基準に取り直す。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `llvm.lifetime.end` を置く位置で、その領域を指す値がまだ生きている可能性を否定できない（D2 の二つの場所以外に置きたくなった）。
- `frame_test` が偽の経路にも `llvm.lifetime.end` が必要に見える、または `llvm.lifetime.start` を同じ領域へ、生きている間に二度出す経路がある。
- 既存テストの期待値（IR の文字列・`!tz.site` の番号・診断）を変える必要がある。本チケットが足す行（`@llvm.lifetime.*`・`musttail`）で
  壊れる期待だけは「既存テストへの影響」に列挙したものに限って直してよい。
- `-O0`・`-O3` の native・wasm32 のどれかで、clang が生成 IR を拒否する（verifier、`WebAssembly 'tail-call' feature not enabled` など）。
- Phase 2: 保証の条件（D1）を満たす呼び出しに `musttail` を付けられない場合がある（引数が呼び出し元の alloca を指す、結果が sret に落ちる、
  呼び出し規約が異なる）。黙って通常の呼び出しに戻さない。
- Phase 2: `wasm-ld` が tail-call を使うオブジェクトと使わないランタイムのオブジェクトの混在を拒否する。
- stack-depth の 3 テスト（GUIDE §3.1）か `honors_the_exact_specialization_limit` が失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状と計測（HEAD `f8dc655`）

### 末尾呼び出し

- `src/llvm.rs` の `FunctionEmitter::emit_tail` は、末尾位置の block・if・match と、自己への完全適用の直接呼び出し
  （`TypedExprKind::Call` と `BinaryOp::Pipe`）を扱う。自己呼び出しは `tail_arguments` で引数を評価し、`drop_all` で残りの所有値を解放してから
  `back_edges` に積み、`jump("loop")` で `loop` ブロックの phi へ戻る。それ以外の末尾の式は `expression` で値を作り、`drop_all` の後で `ret` する。
- `is_self` は、呼び出し先が `FunctionRef::User` で自分自身、`borrowed_worker` でなく、どの引数型も `carries_loans` でないときだけ真。
- 相互再帰は通常の `call` の直後に `ret` が続く形になる（`--emit llvm` で確認）。`musttail`・`tail`・`tailcc` の出力は `src/` に一つもない。

```llvm
  %v5 = call i1 @tz.fn.Main.odd(i64 %v4)
  ret i1 %v5
```

- 利用者関数は全て `define internal <結果> @tz.fn.<Module>.<name>(...) nounwind` で、呼び出し規約は既定（C）である（`FunctionEmitter::emit`）。
- docs/language.md「再帰とスタック」: 「関数値経由の再帰、相互再帰、呼び出した後に演算が続く再帰には、定数スタックの保証はありません」。

### フレーム

- `FunctionEmitter::emit` は `self.allocas` を全て `entry:` に並べ、`br label %loop` の後に本体を置く。
  `src/llvm_frame.rs` の `frame_array`・`frame_list` はリテラルの領域を `alloca [N x T], align 16` として `allocas` に積む。
  一つのリテラルが `MAX_VALUE_BYTES`（`src/check.rs`、`64 * 1024`）を超えると `frame_value` はヒープに置く。関数全体の合計には上限がない。
- 関数内の全てのリテラル領域が、使う時期が重ならなくても同時にフレームに存在する。生成 IR に `llvm.lifetime.start`／`llvm.lifetime.end` はない
  （出現するのは生成済みランタイム `src/runtime/numeric.ll`・`src/runtime/math.ll` だけ。形は `declare void @llvm.lifetime.start.p0(i64 immarg, ptr)`）。
- `drop_framed` はフレームの領域を `frame_test`（値がちょうどこのフレームの領域を持つかのアドレス比較）で見分け、free しない。

### スタックの大きさと枯渇時の挙動

| 実行環境 | 大きさ | 決めている場所 | 枯渇時 |
| --- | --- | --- | --- |
| native の主スレッド | OS の既定（この Mac の `ulimit -s` は 8176 KiB） | OS | SIGSEGV。`tsuzuri run` は `E2005`（確認済み。下の再現） |
| native の Task worker | pthread の既定（macOS は 512 KiB。未計測） | `src/runtime/task.c` の `pthread_create(..., NULL, tz_task_worker_main, NULL)`（属性なし） | SIGSEGV／SIGBUS（未検証） |
| WASM の線形メモリ上のスタック | 1 MiB | `src/driver.rs` と `src/test_runner.rs` の `stack-size=1048576`（stack-first 配置） | 範囲外アクセスのトラップ（未検証） |
| WASM のエンジンの呼び出しスタック | エンジン依存 | V8 の既定 | `RangeError: Maximum call stack size exceeded`（確認済み） |
| WASM の worker（`--wasm-feature threads`） | 256 KiB | `src/runtime/task-wasm-threads.c` の `tsuzuri_thread_stack_size`、`src/runtime/wasm-threads.mjs` | 同上（未検証） |

### 再現（2026-09-30 に確認。M1 Max、macOS、Node 20.19.6、LLVM 21）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-pm09/mutual /tmp/tz-pm09/lib /tmp/tz-pm09/deep
cat > /tmp/tz-pm09/mutual/Main.tz <<'EOF'
def rec even :: i64 -> bool = \n -> if n == 0 then true else odd (n - 1)
and odd :: i64 -> bool = \n -> if n == 0 then false else even (n - 1)

if even 10000000 then 1 else 0
EOF
cat > /tmp/tz-pm09/lib/Main.tz <<'EOF'
def rec even :: i64 -> bool = \n -> if n == 0 then true else odd (n - 1)
and odd :: i64 -> bool = \n -> if n == 0 then false else even (n - 1)

export def deep_even :: i64 -> bool
fn deep_even n = even n
EOF
cat > /tmp/tz-pm09/deep/Main.tz <<'EOF'
def rec depth :: i64 -> i64 = \n -> if n == 0 then 0 else 1 + depth (n - 1)

depth 100000000
EOF
target/release/tsuzuri run /tmp/tz-pm09/mutual -O0
target/release/tsuzuri run /tmp/tz-pm09/mutual -O3
target/release/tsuzuri run /tmp/tz-pm09/deep -O3
for o in -O0 -O3; do
  target/release/tsuzuri build /tmp/tz-pm09/lib --target wasm32 --emit wasm $o -o /tmp/tz-pm09/lib$o.wasm
  node -e "WebAssembly.instantiate(require('fs').readFileSync('/tmp/tz-pm09/lib$o.wasm')).then(({instance})=>{try{console.log('$o',instance.exports.tz_deep_even(10000000n))}catch(e){console.log('$o',e.constructor.name,e.message)}})"
done
```

| ケース | native `-O0` | native `-O3` | WASM `-O0` | WASM `-O3` |
| --- | --- | --- | --- | --- |
| 相互再帰 `even 10000000` | `error[E2005]: program terminated with signal: 11 (SIGSEGV); ...` | `1` | `RangeError Maximum call stack size exceeded` | `1` |
| 非末尾 `depth 100000000` | `E2005`（SIGSEGV） | `100000000` | 未計測 | 未計測 |

`-O3` で完走するのは LLVM の最適化（inline と末尾呼び出しの除去）の結果で、保証ではない。関数が大きくなる、呼び出しが別モジュールにある、
などで崩れる。`-O0` のフレームの大きさは、生成 IR を clang で `-fstack-usage` 付きでコンパイルすると得られる（`even`・`odd` とも `32 static`）。

### `musttail` の実現性（2026-09-30、生成 IR を手で書き換えて確認）

上の `lib`・`mutual` の IR の `%v5 = call i1 @tz.fn.Main.odd(i64 %v4)`（と `even` の同じ行）を `musttail call` に書き換えて clang 21 で確かめた。

- native `-O0`: `b _tz.fn.Main.odd`（分岐）になる。書き換える前は `bl`（呼び出し）。
- wasm32 `-O0` で `-mtail-call` なし: `error: ... in function tz.fn.Main.even i1 (i64): WebAssembly 'tail-call' feature not enabled`。
- wasm32 `-O0` で `-mtail-call` あり: `return_call` が 2 個。`wasm-ld` で結合し、Node 20.19.6 で `tz_deep_even(10000000n)` が `1` を返した。

関数ごとのフレームの大きさ、深さあたりのスタック消費、スタックに置く領域の合計の分布は、これまで記録されていない。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（PX01、GUIDE §14）。

- G1（Phase 1）: 使う時期が重ならないリテラル領域を持つ関数の `-O3` のフレームを、領域の合計ではなく最大値に近づける。
- G2（Phase 1）: 一つの関数がスタックに置くリテラル領域の合計を `MAX_FRAME_BYTES`（新規、64 KiB）以下にする。`-O0` でも効く。
- G3（Phase 2）: 保証の対象（D1）の相互再帰が、深さ 10,000,000 で native の `-O0`・`-O3` と、`--wasm-feature tail-call` 付き WASM の
  `-O0`・`-O3` で完走する。指定なしの WASM の出力は変えない。
- G4: 時間を悪化させない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 フレームの大きさ | bytes（固定値。`-O0` は `.su`、`-O3` は prologepilog の remark） | `tests/fixtures/control/Frames.tz`（新規）の `frames_disjoint`・`frames_loop`、「再現」の `total` | `frames_disjoint` の `-O3` は 2 領域（各 64 bytes）の合計 128 から最大 64 へ近づく。`-O0` は変化なし。上限（G2）は Rust テスト 5・6 で確かめる |
| M2 到達できる深さ | 完走したか（`1` を返したか） | `tests/fixtures/stack/Main.tz`（新規）の `stack_even 10000000` | Phase 2 の後、native `-O0`・`-O3` と tail-call 付き WASM `-O0`・`-O3` で完走。既定の WASM `-O0` は今と同じ `RangeError` |
| M3 時間 | ms（9 回の中央値、最小、最大） | PX01 の `control` suite（既定の規模） | 変化なし（差が広がりの範囲内） |

`frames_disjoint` の計算: 各分岐が `[|a, b, c, d|]`（`i64` 4 要素）を一つ作る。list のノードは `{ ptr, i64 }` の 16 bytes なので、
領域は `[4 x { ptr, i64 }]` の 64 bytes（「再現」の IR と同じ形）。寿命の印がなければ 2 領域が同時に存在する。

## 変えてはいけない意味

- 評価順序: 引数は左から右に一度だけ評価する。保証対象の末尾呼び出しでも、引数の評価 → 残りの所有値の解放 → 呼び出し、の順は
  自己末尾呼び出し（`emit_tail` の `back_edges` の経路）と同じにする。`tail_arguments` の native 専用の遅延加減算は自己呼び出しだけに使い、
  保証対象の呼び出しには使わない。
- 数値・トラップ: overflow・丸め・NaN・符号付きゼロ・飽和に触れる変更はない。トラップの位置の番号（`--trap-info` の `!tz.site`）を変えない。
  寿命の印の行は `FunctionEmitter::instruction` を通さずに出す（`instruction` は `call ` を含む行に `!tz.site` を付けて番号を消費する）。
- 所有権: Phase 1 は解放の時点を変えない。Phase 2 は、保証対象の末尾呼び出しに限り、呼び出し元に残る所有値を呼び出しの前に解放する（D1）。
  これは自己末尾呼び出しの現在の規則と同じで、保証の対象外の呼び出しは今どおり呼び出しの後で解放する。B07（利用者 drop）はこの順序に従う。
- 借用: 借用を含みうる引数型（`Type::carries_loans`）を呼び出し元か呼び出し先に持つ呼び出しは、保証の対象外（`is_self` と同じ規則）。
  呼び出し先が呼び出し元のフレーム（alloca）を指す値を受け取ってはならない。フレームの領域を持つ所有値は、既存の `relocate` でヒープへ移してから渡す。
- 生成 IR の決定性: 同じ入力から同じ IR を出す（`function_ir` が 2 回の出力の一致を確かめる）。intrinsic の宣言は `intrinsics`
  （`BTreeSet<String>`）で重複なく出し、連結するランタイム `.ll` の宣言とも重複させない。rec グループの番号は関数 id の順で決める。
- WASM: 既定の出力に import・tail-call 命令を足さない（GUIDE D-18）。`--wasm-feature tail-call` の指定時だけ `return_call` を出す。
- native と WASM の差: 保証の範囲（D1）は native と tail-call 付き WASM で同じ。既定の WASM では Phase 2 の保証がないことを文書に書く。
- デバッグ: 保証対象の末尾呼び出しでは、呼び出し元のフレームがバックトレースから消える。これは仕様として文書に書く。

## 設計

### Phase 分割

| Phase | 内容 | 承認 | 独立して出せるか |
| --- | --- | --- | --- |
| 1a | リテラル領域の `llvm.lifetime.start`／`llvm.lifetime.end`（D2） | 不要 | 出せる |
| 1b | 関数あたりのフレーム領域の上限 `MAX_FRAME_BYTES`（D3） | 不要 | 出せる |
| 1c | スタックの大きさと枯渇時の挙動の文書、フレームの計測手順 | 不要 | 出せる |
| 2a | native の保証対象の末尾呼び出しを `musttail` にする（D1・D4・D5・D9） | 要承認 D1 | 出せる（WASM は今どおり） |
| 2b | `--wasm-feature tail-call`（D6） | 要承認 D6 | 2a の後 |

### データ構造

Phase 1（新規の名前だけ）。

```rust
// src/check.rs（MAX_VALUE_BYTES の隣）
pub const MAX_FRAME_BYTES: usize = 64 * 1024; // （新規）一つの関数がスタックに置くリテラル領域の合計

// src/llvm.rs の FunctionEmitter
frame_bytes_used: usize, // （新規）frame_value がスタックに置くと決めた bytes の合計。emit の開始で 0
```

Phase 2。

```rust
// src/llvm.rs の EmitOptions
pub wasm_tail_call: bool, // （新規）

// src/llvm.rs の Globals
tail_calls: bool,          // （新規）!wasm || wasm_tail_call
tail_groups: Vec<usize>,   // （新規）関数 id → 直接呼び出しの強連結成分の番号。tail_calls が偽なら空

// src/driver.rs の BuildOptions
pub wasm_tail_call: bool,  // （新規）Default は false
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 1a | `src/llvm_frame.rs` | `frame_array`, `frame_list`, `emit_frame_inner`（`Frame::String` を作る分岐） | alloca を `allocas` に積んだ後、現在のブロックで最初の store より前に `call void @llvm.lifetime.start.p0(i64 <bytes>, ptr <alloca>)` を `self.lines` へ直接出す。`<bytes>` は alloca の型の大きさ（`[N x T]` なら N × `stack_size`） |
| 1a | `src/llvm_frame.rs` | `drop_framed` | `frame_test` が真の分岐（領域を free しない側）で、要素の解放の後に `call void @llvm.lifetime.end.p0(i64 <bytes>, ptr <alloca>)` を出す。偽の分岐には出さない（D2） |
| 1a | `src/llvm_frame.rs` | `lifetime`（新規） | 上の 2 行を作る小さな helper。`intrinsics` に `declare void @llvm.lifetime.start.p0(i64 immarg, ptr)` と `...end.p0(...)` を入れる |
| 1a | `src/llvm.rs` | ランタイム `.ll` を連結する箇所（`include_str!("runtime/numeric.ll")` を含む分岐。`declare void @llvm.trap()` を `replace` で消している所） | 連結する `.ll` が同じ intrinsic を宣言するので、生成側の `declare void @llvm.lifetime.*` の行を `@llvm.trap` と同じ方法で消す。`math.ll` を連結する箇所も同じ |
| 1b | `src/check.rs` | `MAX_FRAME_BYTES`（新規） | 定数を足す |
| 1b | `src/llvm_frame.rs` | `frame_value` | `frame_bytes(expression)` を一度だけ計算し、`> MAX_VALUE_BYTES` または `frame_bytes_used + bytes > MAX_FRAME_BYTES` ならヒープの経路（今の `> MAX_VALUE_BYTES` と同じ経路）。スタックに置くときだけ `frame_bytes_used += bytes` |
| 1b | `src/llvm.rs` | `FunctionEmitter` の構築 | `frame_bytes_used: 0` |
| 2a | `src/llvm.rs` | `EmitOptions`, `Globals` と構築 | `wasm_tail_call`、`tail_calls`、`tail_groups`（`tail_calls` のときだけ計算） |
| 2a | `src/llvm.rs` | `tail_groups`（新規の関数） | `module.functions` の本体の `TypedExprKind::Function(FunctionRef::User(id))`・`GenericFunction(id, _)` を辺とする強連結成分。反復版の Tarjan（再帰しない。本体の走査は `TypedExpr::children` を明示の stack で回す） |
| 2a | `src/llvm.rs` | `FunctionEmitter::guaranteed_tail`（新規） | D1 の条件を判定し、満たせば呼び出し先の id を返す（アルゴリズム） |
| 2a | `src/llvm.rs` | `FunctionEmitter::emit_tail` | `_ =>` の前に 2 つの arm: `TypedExprKind::Call` と `BinaryOp::Pipe` で `guaranteed_tail` が `Some` のとき。自己呼び出しの arm（`is_self`）より後に置き、自己呼び出しは今どおり loop にする |
| 2b | `src/main.rs` | `--wasm-feature` の解析 | `Some("tail-call") => &mut wasm_tail_call`。誤りの message を `supported WASM features are 'simd128', 'threads', and 'tail-call'; relaxed SIMD is not supported` にする。`(wasm_simd \|\| wasm_threads \|\| wasm_tail_call) && action != Action::Build` の検査と `BuildOptions` への受け渡し。usage の行 `--wasm-feature <name>` の説明に `tail-call` を足す |
| 2b | `src/driver.rs` | `BuildOptions`, `Default`, `validate` | `wasm_tail_call`。WGSL の拒否条件に足す。`--wasm-feature tail-call requires wasm32 object, LLVM IR, or WASM output`（`wasm_simd` と同じ条件） |
| 2b | `src/driver.rs` | `--emit llvm` の注記（`; wasm-feature: simd128; ...` を出す所） | `; wasm-feature: tail-call; compile this IR with -mtail-call` |
| 2b | `src/driver.rs` | wasm32 の clang 引数（`-msimd128`／`-mno-simd128` を足す所） | `-mtail-call`／`-mno-tail-call`。既定で `-mno-tail-call` を明示し、clang の既定が変わっても既定の出力を保つ |
| 2b | `src/driver.rs` | build cache の鍵 | `wasm_simd` が鍵に入る全ての場所に `wasm_tail_call` を足す（`grep -n wasm_simd src/*.rs` で列挙） |

### 生成 IR とランタイム

Phase 1a。`frames_disjoint` の片方の分岐（名前は説明用）。

```llvm
b3:
  call void @llvm.lifetime.start.p0(i64 64, ptr %v7)
  ; ... ノード 4 個の store と %tz.list の構築 ...
  ; スコープの終わりの drop_framed
  %v20 = icmp eq ptr %v18, %v7
  ; ... length の比較と and ...
  br i1 %v22, label %b8, label %b9
b8:
  call void @llvm.lifetime.end.p0(i64 64, ptr %v7)
  br label %b10
```

- 印の pointer 引数は alloca の結果そのものにする（GEP を渡さない）。LLVM 21 の宣言は `(i64 immarg, ptr)`。LLVM 22 以降の 1 引数の形は
  ランタイムの `.ll` と同じく自動変換に任せる（GUIDE §2.2。生成済みランタイムを手で直さない）。
- `-O0` では clang は寿命の印を使わない（stack coloring は最適化時だけ）。`-O0` のフレームは 1b の上限でだけ小さくなる。
- ループ本体のリテラルは、今どおり領域を入口に一つだけ置く。反復ごとに start と end が対になり、蓄積しない。

Phase 2a。「現状」の `even` は次になる。`drop_all` の出力（あれば）は `musttail call` より前、`ret` は直後。

```llvm
  %v4 = sub i64 %v3, 1
  %v5 = musttail call i1 @tz.fn.Main.odd(i64 %v4)
  ret i1 %v5
```

- `musttail` の条件（LLVM）: 呼び出しの直後が `ret` で、その値を返す。呼び出し元と呼び出し先のプロトタイプ（引数と結果の LLVM 型）と
  呼び出し規約が一致する。結果が sret に変換されない。呼び出し先は呼び出し元の alloca に触れない。D1 はこれらを全ての対象で満たすように決めてある。
- `tail`・`tailcc` は出さない（D4）。
- ランタイムの変更はない。スタックの大きさ（1 MiB、256 KiB）も変えない（D8）。

### アルゴリズム

保証の判定（Phase 2a）。`if let` の match guard は使わず、guard は `.is_some()`、arm の本体で同じ関数をもう一度呼ぶ。

```rust
fn guaranteed_tail(&self, callee: &TypedExpr, count: usize) -> Option<usize> {
    let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else { return None };
    if !self.globals.tail_calls || self.borrowed_worker || id == self.function_id {
        return None; // 自己呼び出しは is_self の loop か、借用があれば通常の呼び出し
    }
    let target = &self.module.functions[id];
    let (caller, callee) = (&self.function.signature, &target.signature);
    let types = self.module.types();
    let llvm = |list: &[Type]| list.iter().map(|ty| self.ty(ty)).collect::<Vec<_>>();
    let result = self.ty(&caller.result);
    let ok = self.globals.tail_groups[id] == self.globals.tail_groups[self.function_id]
        && count == target.parameters.len()
        && self.function.capture_count == 0 && target.capture_count == 0
        && !self.function.is_task && !target.is_task
        && !caller.parameters.iter().chain(&callee.parameters).any(|ty| ty.carries_loans(&types))
        && llvm(&caller.parameters) == llvm(&callee.parameters)
        && result == self.ty(&callee.result)
        && matches!(result.as_str(), "i1" | "i8" | "i16" | "i32" | "i64" | "float" | "double" | "ptr");
    ok.then_some(id)
}
```

`emit_tail` の新しい arm の本体は次の順にする。引数は `self.expression` で左から右に評価する（フレームを持つ所有値は既存の経路で
`relocate` される。手順 10 のテストで確かめる）。`drop_all` の後、`musttail call <結果> @tz.fn.<qualified_name>(<型付き引数>)` を
`self.value` で出し、直後に `ret <結果> <値>` を `self.instruction` で出す。記号は `FunctionEmitter::call` の `known` の経路と同じ
`format!("@tz.fn.{}", function.qualified_name())`。呼び出しの特殊化（`specializations`・`borrowed_call`）は通らない。

`tail_groups` は反復版の Tarjan。

```text
edges[f] = 本体に現れる FunctionRef::User(g) と GenericFunction(g, _) の g（重複を除き昇順）
for f in 0..functions.len() (昇順):
    if index[f] 未定義: 明示の stack で strongconnect(f) を回す（再帰呼び出しをしない）
group[f] = f を含む強連結成分の、成分が確定した順の番号
```

本体の走査も `TypedExpr::children` を `Vec` の stack で回す。番号は関数 id の昇順の走査だけで決まり、決定的である。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。手順 1〜7 が Phase 1、手順 8〜13 が Phase 2（D1・D6 の承認後）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: 「再現」の 3 つの project と下の `frames` を `/tmp/tz-pm09/` に作り、結果と IR を `/tmp/tz-pm09/before/` に保存する。
  `cp target/release/tsuzuri /tmp/tz-pm09/tsuzuri-before`。`frames/Main.tz` は「再現」と同じ形で、`total` の本体が
  `let a = [|n, n + 1, n + 2|]`・`let b = List.length a`・`let c = [|b, b, b, b|]`・`List.length c + b`、トップレベルの `total 1`（2026-09-30 に `run` で `7`）。
- 確認: 次が全て成功し、`run` の結果が「再現」の表どおり。`frames` の `-O0` の remark が `352 stack bytes in function 'tz.fn.Main.total'`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
cargo test --locked --test tail_recursion
node tests/control.mjs target/release/tsuzuri
TSUZURI_ASAN=1 node tests/control.mjs target/release/tsuzuri
target/release/tsuzuri build /tmp/tz-pm09/frames --emit llvm -o /tmp/tz-pm09/before/frames.ll
/opt/homebrew/opt/llvm@21/bin/clang -x ir -Wno-override-module -O0 -c /tmp/tz-pm09/before/frames.ll -o /dev/null -Rpass-analysis=prologepilog
```

### 手順 2: Phase 1 の fixture と E2E の期待値

- 変更: `tests/fixtures/control/Frames.tz`（新規）、`tests/control.mjs` の `cases`。
- 内容: 次の export を書く（既存の構文だけを使う。`check` で確かめる）。`frames_disjoint n` は
  `if n > 0 then { let a = [|n, n, n, n|]; List.length a + n } else { let b = [|1, 2, 3, n|]; List.length b - n }`。
  `frames_loop n` は `def rec` の補助 `fn rec count n acc = if n == 0 then acc else { let xs = [|n, n, n|]; count (n - 1) (acc + List.length xs) }`
  を `count n 0` で呼ぶ。リテラルは `let` で束縛する（`List.length ref [|n, n|]` のような一時値の借用は `E1013` で拒否される。確認済み）。
  この 2 関数は 2026-09-30 に scratch で確認済み（トップレベルの `frames_disjoint 7 * 1000 + frames_loop 1000` が `-O0`・`-O3` とも `14000`、
  `frames_disjoint` の entry に `[4 x { ptr, i64 }]` の alloca が 2 個、`count` は `loop:` を持ち自己呼び出しがない）。
  期待値は式から計算する: `frames_disjoint` は n > 0 で `4 + n`、それ以外で `4 - n`（n = -5, -1, 0, 1, 7, 2^62）。`frames_loop` は `3n`（n = 0, 1, 1000, 100000）。
- 確認: `target/release/tsuzuri check tests/fixtures/control` が成功。`node tests/control.mjs target/release/tsuzuri` が成功（コードの変更前でも通る）。

### 手順 3: 寿命の印（1a）

- 変更: `src/llvm_frame.rs` の `lifetime`（新規）・`frame_array`・`frame_list`・`emit_frame_inner`・`drop_framed`、`tests/stack_frames.rs`（新規）。
- 内容: 段ごとの変更の 1a の行。テストファイルは `tests/tail_recursion.rs` の `function_ir` を写す。
- 確認: `cargo test --locked --test stack_frames` が `3 passed`（テスト計画の 1〜3）。`cargo test --locked --test tail_recursion` が `6 passed`。

### 手順 4: ランタイムの宣言との重複

- 変更: `src/llvm.rs` の `include_str!("runtime/numeric.ll")`・`include_str!("runtime/math.ll")` を連結する箇所。
- 内容: 段ごとの変更の 1a の最後の行。
- 確認: `cargo test --locked --test stack_frames` が `4 passed`（テスト計画の 4）。`cargo test --locked` が成功。

### 手順 5: 関数あたりの上限（1b）

- 変更: `src/check.rs` の `MAX_FRAME_BYTES`（新規）、`src/llvm_frame.rs` の `frame_value`、`src/llvm.rs` の `FunctionEmitter`。
- 内容: 段ごとの変更の 1b の行。先に `sed -n '/^fn frame_bytes/,/^}/p' src/llvm_frame.rs` で list の 1 要素あたりの見積もり B を確かめ、
  テストのコメントに式を書く。
- 確認: `cargo test --locked --test stack_frames` が `6 passed`（テスト計画の 5・6）。`grep -rn "MAX_VALUE_BYTES\|65536\|64 \* 1024" tests src` の
  既存テストが全て成功する。

### 手順 6: 全体の確認（Phase 1）

- 変更: なし。
- 確認: 次が全て成功する。`TSUZURI_ASAN=1` の実行は、生成 IR に `sanitize_address` を付けて計装する（`tests/control.mjs` の `sanitizer`）ので、
  誤った `llvm.lifetime.end` は stack-use-after-scope として検出される。

```sh
cargo test --locked
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
TSUZURI_ASAN=1 node tests/control.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
```

### 手順 7: 計測・生成コード・文書（1c）

- 変更: 「ドキュメント」の Phase 1 の行、`_perfs/README.md`（Phase 1 の完了を状態欄の注記に書く。done は Phase 2 の判断後）。
- 内容: 「計測手順」と「生成コードの確認」の Phase 1 の部分を実行し、表を完了報告に貼る。
- 確認: `node scripts/check-docs.mjs _docs/language-reference/functions.md _docs/guides/performance.md` が成功。`git diff --check` が空。

### 手順 8: rec グループ（Phase 2a、D1 の承認後）

- 変更: `src/llvm.rs` の `EmitOptions`・`Globals`・`tail_groups`（新規）。
- 内容: `tail_calls` は `!wasm || wasm_tail_call`。まだ誰も使わない。
- 確認: `cargo test --locked` が成功。手順 1 の stack-depth 3 テストが成功。`--emit llvm` の IR が手順 1 と byte 単位で同じ（`cmp`）。

### 手順 9: `musttail` の出力

- 変更: `src/llvm.rs` の `guaranteed_tail`（新規）・`emit_tail`、`tests/tail_recursion.rs`。
- 内容: アルゴリズムのとおり。
- 確認: `cargo test --locked --test tail_recursion` が `10 passed`（既存 6 + テスト計画の 7〜10）。

### 手順 10: 浅い E2E

- 変更: `tests/fixtures/control/TailCalls.tz`（新規）、`tests/control.mjs` の `cases`。
- 内容: 3 関数の循環 `a → b → c → a`（各 `\n acc -> if n == 0 then acc else <次> (n - 1) (acc + k)`、k は 1・2・3）、
  フレームを持つ list のローカルを move で渡す相互再帰（`let xs = [|n, n + 1|]` を作って相手へ渡し、相手は `List.length` と要素の和を読んで次へ渡す）、
  保証の対象外の形（`string` を返す相互再帰、`&i64` を受ける相互再帰）。深さは 1000 以下。期待値は JavaScript の BigInt で独立に計算する。
- 確認: `node tests/control.mjs target/release/tsuzuri` と `TSUZURI_ASAN=1 node tests/control.mjs target/release/tsuzuri` が成功。

### 手順 11: 深い E2E（native）

- 変更: `tests/fixtures/stack/Main.tz`（新規）、`tests/stack.mjs`（新規。`tests/wasm_simd.mjs` の形を写す）。
- 内容: fixture は「再現」の `even`・`odd` と 3 関数の循環（各段 `acc + 1`）を持ち、トップレベルの結果式は `if even 10000000 then a 10000000 0 else -1`。
- 確認: `node tests/stack.mjs target/release/tsuzuri` が native `-O0`・`-O3` で期待値を出す。

### 手順 12: `--wasm-feature tail-call`（Phase 2b、D6 の承認後）

- 変更: `src/main.rs`、`src/driver.rs`（段ごとの変更の 2b の行）、`src/main.rs` の既存の CLI テスト（`--wasm-feature` の受理・拒否の表）。
- 内容: `tail-call` の受理、`check`・`run` と native・header での拒否、重複指定の拒否をテストに足す。
- 確認: `cargo test --locked --bin tsuzuri` が成功。`node tests/stack.mjs target/release/tsuzuri` が WASM の 4 通りも期待どおり。

### 手順 13: 文書と最終確認

- 変更: 「ドキュメント」の Phase 2 の行。
- 確認: 手順 6 の全コマンド、`node tests/stack.mjs target/release/tsuzuri`、`node tests/wasm_simd.mjs target/release/tsuzuri`、
  `node tests/wasm_threads.mjs target/release/tsuzuri`、stack-depth の 3 テストが成功。

## 計測手順

環境の記録、before と after の実行、比較は PX01 の「計測手順」の「before／after（他チケットの共通手順）」に従う。run は
`target/perf/PM09-before`・`PM09-after`・`PM09-before2`、suite は `control` だけ（M3）。compiler は `/tmp/tz-pm09/tsuzuri-before` と
`target/release/tsuzuri`。PX01 が未完了なら M3 は測らず、そう報告する。M1 と M2 は PX01 に依存しない。

M1（フレームの大きさ）は固定値なので 1 回でよい。結果は `target/perf/PM09-after/frames.txt` に置き、Git に加えない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
C=/opt/homebrew/opt/llvm@21/bin
mkdir -p target/perf/PM09-after
for tz in /tmp/tz-pm09/tsuzuri-before target/release/tsuzuri; do
  n=$(basename $(dirname $tz))-$(basename $tz)
  $tz build tests/fixtures/control --emit llvm -o /tmp/tz-pm09/$n.ll
  for o in -O0 -O3; do
    $C/clang -x ir -Wno-override-module $o -c /tmp/tz-pm09/$n.ll -o /dev/null -Rpass-analysis=prologepilog 2>&1 \
      | grep -E "frames_|Frames\." | sed "s/^/$n $o /" >> target/perf/PM09-after/frames.txt
  done
done
```

- `-O3` では内部関数が export の関数へ inline されて消えるので、`tz_frames_disjoint` などの export の行を比べる
  （2026-09-30 の probe では `-O3` の `tz.fn.Main.total` の remark は出なかった。`-fstack-usage` の `.su` も `-O3` では作られなかった）。
- M2 は `node tests/stack.mjs target/release/tsuzuri` の出力（native・WASM × `-O0`／`-O3` の完走か失敗か）をそのまま記録する。

## 生成コードの確認

1. 寿命の印（Phase 1）: 手順 1 の `frames` を after の compiler で `--emit llvm` し、`total` の本体で数える。
   `awk '/^define internal i64 @tz.fn.Main.total/,/^}/' f.ll | grep -c 'call void @llvm.lifetime.start.p0(i64 '` が 2、
   `...end.p0(i64 ` が 2 以上、`grep -c '^declare void @llvm.lifetime.start.p0' f.ll` が 1。印の pointer が `alloca` の結果の名前であること。
2. `-O3` のフレーム: 計測手順の表で `tz_frames_disjoint` の bytes が before より小さい。小さくなければ、`llc` 前の IR に印が残っているかを
   `clang -x ir -O3 -S -emit-llvm` の出力で確かめ、原因を報告する（改善を主張しない）。
3. `musttail`（Phase 2a）: 「再現」の `mutual` の IR に `musttail call i1 @tz.fn.Main.odd(` と `musttail call i1 @tz.fn.Main.even(` があり、
   各行の次の行が `ret i1`。`clang -x ir -Wno-override-module -O0 -S` の出力に `b` の行 `b\s+_tz\.fn\.Main\.odd`（2026-09-30 に
   手で書き換えた IR で確認）があり、`bl\s+_tz\.fn\.Main\.odd` がない。
4. WASM（Phase 2b）: `--target wasm32 --emit llvm` の IR に `musttail` が 0 個。`--wasm-feature tail-call` を付けると先頭の注記に
   `; wasm-feature: tail-call; compile this IR with -mtail-call` があり `musttail` が 2 個。`--emit object` の `llvm-objdump -d` で
   `return_call` が `-O0` で 2 個以上、既定の出力では 0 個。

## テスト計画

### Rust テスト

`tests/stack_frames.rs`（新規、Phase 1）。`function_ir` は 2 回の出力の一致も確かめる。

| # | 名前 | 検証すること |
| --- | --- | --- |
| 1 | `brackets_binding_literals_with_lifetime_markers` | 手順 1 の `total` で start が 2、end が 2 以上。各 start は対応する alloca の最初の store より前、各 end は `frame_test` の真の分岐の中 |
| 2 | `brackets_temporary_literals_and_loop_literals` | `frames_disjoint` の形で start と end が分岐ごとに 1 組。`frames_loop` の形で `loop:` より後に alloca がなく、start と end が本体に 1 組 |
| 3 | `lifetime_markers_do_not_consume_trap_sites` | `llvm::emit_with_trap_info` の出力で、寿命の印の行に `!tz.site` がなく、site の数が寿命の印のない関数と同じ形の関数で変わらない |
| 4 | `declares_lifetime_intrinsics_once_with_runtime_ir` | ランタイムの `.ll` を連結するプログラム（浮動小数点を表示するなど、`include_str!("runtime/numeric.ll")` の分岐を通るもの）で `declare void @llvm.lifetime.start.p0` がちょうど 1 回 |
| 5 | `keeps_literals_on_the_stack_within_the_function_budget` | `let` で束縛した `[|i64|]` のリテラル 5 個（各 600 要素）の関数。`frame_bytes` の list の見積もりは要素ごとに `16 + stack_size(element).next_multiple_of(16)` = 32 なので 1 個 19,200 bytes。`alloca [600 x` の行が 3 個（57,600 ≤ 65,536）、残り 2 個はヒープ（実際の alloca は 1 要素 16 bytes） |
| 6 | `budget_boundary_is_inclusive` | 1,024 要素のリテラル 2 個（合計 65,536 bytes）は両方スタック。続く 1 要素の 3 個目はヒープ |

`tests/tail_recursion.rs`（Phase 2a に追加）。

| # | 名前 | 検証すること |
| --- | --- | --- |
| 7 | `marks_mutual_tail_calls_musttail` | `even`・`odd` と 3 関数の循環で `musttail call` の次の行が `ret`。`llvm::emit_target(..., true)`（既定の WASM）では `musttail` がない |
| 8 | `drops_owned_locals_before_guaranteed_tail_calls` | 所有の `string` ローカルを持つ相互再帰で `call void @tz.free` が `musttail call` より前、`musttail call` と `ret` の間に行がない |
| 9 | `keeps_ordinary_calls_outside_the_guarantee` | 次は `musttail` がない: 別グループの関数への末尾呼び出し、`string` を返す相互再帰、`&i64` を受ける相互再帰、関数値経由の呼び出し、呼び出しの後に `+ 1` が続く呼び出し、`\|>` で 2 引数の関数へ渡す部分適用 |
| 10 | `self_tail_calls_stay_loops` | 自己呼び出しは今どおり `loop:` への分岐で、`musttail call i64 @tz.fn.Main.f(` がない（既存 6 テストの形も変わらない） |

`src/main.rs` の CLI テスト（Phase 2b）: `build Main.tz --target wasm32 --wasm-feature tail-call` を受理、`--wasm-feature tail-call` の重複と
`check`・`run` での指定を拒否。`BuildOptions::validate` で native・header との組み合わせを `E2000` で拒否。

### E2E

- Phase 1: `tests/control.mjs` の `cases` に `frames_disjoint`・`frames_loop`（手順 2）。`control.mjs` は native と wasm32 の各最適化レベルと
  確保追跡（`trackedIr`）を回す。`TSUZURI_ASAN=1` でも実行する。
- Phase 2: `tests/control.mjs` の `cases` に手順 10 の浅い相互再帰。`tests/stack.mjs`（新規）は `tests/fixtures/stack` を
  native `-O0`・`-O3` は `run`、WASM は `build --target wasm32 --emit wasm`（既定と `--wasm-feature tail-call`）で作り、Node で `tz_main()` を呼ぶ。

| 対象 | native `-O0` | native `-O3` | 既定の WASM `-O0`／`-O3` | tail-call 付き WASM `-O0`／`-O3` |
| --- | --- | --- | --- | --- |
| `main`（深さ 10,000,000） | `10000000` | `10000000` | 結果を検査しない。IR に `musttail` がないことだけを見る | `10000000n` |

`tests/fixtures/stack` の循環は各段で 1 を足す（`acc + 1`）ので期待値は n（10000000）。手順 10 の浅い循環（k = 1・2・3）の期待値は
JavaScript の `let acc = 0n; for (let i = 0n; i < n; i++) acc += [1n, 2n, 3n][i % 3n]` で計算する。

### 既存テストへの影響

- Phase 1: 予想はなし（寿命の印は `alloca`・`!tz.site` を含まず、`declare` は重複させない）。IR の文字列を比べるテストが印の行だけで
  壊れた場合に限り期待を直してよい。それ以外の変化は停止条件。
- Phase 2: 予想はなし。`musttail call i64 @tz.fn.Main.f(` は `call i64 @tz.fn.Main.f(` を部分文字列として含むので、
  `contains("call ...")` の検査は自己呼び出し以外の変更で結果が変わりうる。失敗した検査は、呼び出しが D1 の対象かを確かめて報告する。

### 性能

M3 の `control` suite の時間。CI に閾値は置かない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/language.md` | 「再帰とスタック」 | Phase 1: 「現状」のスタックの大きさと枯渇時の挙動の表（未検証の欄は書かない）。Phase 2: D1 の保証の条件、呼び出し前の解放、バックトレースからフレームが消えること、既定の WASM では保証しないこと |
| `docs/language.md` | 「スタックとヒープ」（64 KiB の段落） | 関数あたりの合計 `MAX_FRAME_BYTES` と、超えた分はヒープ（D3） |
| `docs/language.md`, `README.md` | `--wasm-feature` の説明（`grep -rn "wasm-feature" docs README.md _docs`） | Phase 2b: `tail-call` と engine の要件 |
| `docs/architecture.md` | 「末尾再帰」の段落、フレームの段落 | 寿命の印の位置（D2）、`musttail` の条件と `tail_groups` |
| `docs/benchmarks.md` | 「スタックとフレームの大きさ」（新規の節） | 計測手順の M1 のコマンドと、`-O0`／`-O3` の違い |
| `_docs/language-reference/functions.md` | 再帰の段落（「直接の自己末尾再帰は `-O0` でも…」） | Phase 2: 相互の末尾呼び出しの保証 |
| `_docs/guides/performance.md` | 最適化レベルに依存しない処理の段落 | Phase 1・2 の要約 |
| `_perfs/README.md` | 一覧の PM09 の行 | 状態 |

## 受け入れ条件

- [ ] Phase 1: `tests/stack_frames.rs` の 6 テストと手順 6 の全コマンド（`TSUZURI_ASAN=1` を含む）が成功する。
- [ ] Phase 1: 生成コードの確認 1 が期待どおりで、M1 の before／after の表が完了報告にある（改善がなければそう書く）。
- [ ] Phase 1: 既定の WASM の import と stack サイズが変わらない（`tests/control.mjs` の wasm32 と `tests/wasm_threads.mjs` が成功）。
- [ ] Phase 2（承認後）: 手順 9〜13 の確認が成功し、M2 の表が期待どおり。既定の WASM の IR に `musttail` がない。
- [ ] stack-depth の 3 テストと `honors_the_exact_specialization_limit` が成功し、上限を変えていない。
- [ ] 同じ入力の IR が 2 回の出力で一致する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `FunctionEmitter::instruction` は `call ` を含む行に `!tz.site` を付けて metadata 番号を消費する。寿命の印を通すと `--trap-info` の番号が
  全てずれる。`self.lines.push` で直接出す。テスト 3 が検出する。
- ランタイムの `.ll` は IR の文字列に連結される（`@llvm.trap` の宣言を `replace` で消している）。`@llvm.lifetime.*` の宣言が重なると
  clang が `invalid redefinition of function` で拒否する。テスト 4 と、浮動小数点を扱う fixture の `-O0` の build が検出する。
- 印の pointer に GEP を渡さない。`[N x T]` の alloca の結果をそのまま渡す。
- `-O0` の数値は寿命の印では変わらない。`-O0` の改善を主張しない。
- `frame_test` の偽の分岐（move 済み・代入済み）に end を置くと、移った先の値が使う領域を LLVM が再利用する。ASan の
  stack-use-after-scope が検出する。
- `musttail call` と `ret` の間に drop や debug の行を入れると clang が拒否する。`drop_all` は必ず呼び出しより前。
- 既定の WASM で `musttail` を出すと clang が `WebAssembly 'tail-call' feature not enabled` で失敗する（確認済み）。`tail_calls` の判定を一か所にする。
- 集約（`%tz.string` など）や `i128` を返す関数は、x86-64 や wasm32 で sret に変わり `musttail` が成り立たない。D1 の結果の型の制限を緩めない。
- `tail_groups` と本体の走査を再帰で書くと、深い式や大きい call graph で compiler の stack を使い切る。明示の stack で書く。
- Node 20.19.6 は `return_call` を実行できる（確認済み）。BigInt の多い suite で V8 が落ちたら Node 24 を使う
  （`npx --yes --package=node@24 node tests/control.mjs target/release/tsuzuri`）。
- `cargo test --locked <filter>` は 0 件でも成功する。`running N tests` を見る。

## 対象外

- 分割スタック・セグメント化スタック、スタックの動的な拡張、スタックの大きさの変更（D8）、スタック枯渇の報告（E14）。
- プロトタイプの異なる呼び出しの保証（`tailcc` による呼び出し規約の変更）、関数値経由の呼び出し（継続渡しの closure）、集約を返す関数の保証、
  明示の末尾呼び出し構文（`become` など）。
- async のビルダー（B08）の再開、WASM の multivalue ABI、フレームの大きさを出す CLI 指定（D7）。

## 決定事項

### D1: 保証の範囲と解放の順序

- 決定: 次を全て満たす呼び出しは、native と `--wasm-feature tail-call` 付き WASM で呼び出し元のフレームを残さない（定数スタック）。
  末尾位置（`emit_tail` が扱う block・if・match・関数ガードの末尾と `value |> g`）の、利用者関数 `g` の完全適用の直接呼び出しで、
  `g` が呼び出し元と同じ rec グループ（直接呼び出しの強連結成分、D9）にあり、両者とも捕捉を持たず Task の本体でなく、
  両者の引数型に借用を含みうる型（`carries_loans`）がなく、引数と結果の LLVM 型が一致し、結果が `bool`・整数（`i128` を除く）・
  `f32`・`f64`・unit などの単一の scalar（LLVM で `i1`・`i8`・`i16`・`i32`・`i64`・`float`・`double`・`ptr`）であること。
  この呼び出しでは、引数を評価した後、呼び出し元に残る所有値を呼び出しの前に解放する。自己呼び出しは今どおり loop にする。
- 理由: 条件は LLVM の `musttail` の要件（同じプロトタイプ・同じ呼び出し規約・sret なし・呼び出し元の alloca を渡さない）を
  全ての target で静的に満たす最小の集合で、利用者は型と位置だけで判断できる。解放の順序は自己末尾呼び出しの現在の規則と同じ。
- 状態: 要承認（承認前は手順 8 以降に着手しない）

### D2: 寿命の印の位置

- 決定: `llvm.lifetime.start` はリテラルの構築の直前（その領域への最初の store より前）。`llvm.lifetime.end` は `drop_framed` の
  `frame_test` が真の分岐（値がちょうどその領域を持ち、free しない側）で要素の解放の後だけ。
- 理由: 一意の所有者が破棄される点なので、その後に領域を指す値は存在しない。move・代入で所有者が移った経路（偽の分岐）には置かず、
  LLVM は保守的に扱う。新しい解析が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 関数あたりのフレーム領域の上限

- 決定: `MAX_FRAME_BYTES = 64 * 1024`。`frame_value` が emit の順に `frame_bytes` を足し、超える分のリテラルはヒープに置く（境界は含む）。
- 理由: 今は 1 個あたりの上限だけで、非末尾再帰の関数のフレームが上限なく大きくなりうる。`-O0` にも効き、順序は決定的。
  PM06 がスタックへ移す領域も同じ合計に数える（PM06 と整合させる）。
- 状態: 既定案（実装者はこの案に従う）

### D4: `musttail` だけを出す

- 決定: 保証対象には `musttail` を付ける。`tail` と `tailcc` は出さない。
- 理由: `tail` は `-O0` で効かず、`-O1` 以上では LLVM が自分で付ける。`tailcc` は内部の呼び出し規約を変え、デバッグ情報とトラップ位置の確認が要る。
- 状態: 既定案（実装者はこの案に従う。着手は D1 の承認後）

### D5: 保証できない呼び出しの扱い

- 決定: D1 の条件を満たさない呼び出しは今どおりの通常の呼び出しで、診断を出さない。条件を満たす呼び出しは必ず `musttail` にし、
  通常の呼び出しへ戻す経路を作らない（戻したくなったら停止条件）。
- 理由: 条件は型と位置だけで決まるので、保証は文書だけで予測できる。rec グループの対象外の呼び出しごとの警告は既存のコードで大量に出る。
- 状態: 既定案（実装者はこの案に従う。着手は D1 の承認後）

### D6: WASM の opt-in

- 決定: `--wasm-feature tail-call`（build だけ、wasm32 の object・LLVM IR・WASM 出力）。指定時は IR に `musttail` を出し clang に
  `-mtail-call`、指定なしは `-mno-tail-call` を明示する。
- 理由: tail-call に対応しない engine では instantiate できないので既定にできない（D-18）。`simd128` と同じ形で CLI の利用者に分かりやすい。
- 状態: 要承認（承認前は手順 12 に着手しない）

### D7: フレームの大きさの計測

- 決定: CLI の指定を足さない。`--emit llvm` の IR を clang の `-Rpass-analysis=prologepilog`（と `-O0` の `-fstack-usage`）で測る。
- 理由: 検証済みの外部の手段で足り、compiler の表面を増やさない。
- 状態: 既定案（実装者はこの案に従う）

### D8: スタックの大きさ

- 決定: WASM の 1 MiB・worker の 256 KiB・native の OS 既定を変えない。文書にだけ書く。
- 理由: 大きさを上げても深さの問題は移るだけで、線形メモリ上限（16 MiB）を圧迫する。
- 状態: 既定案（実装者はこの案に従う）

### D9: rec グループの求め方

- 決定: LLVM の出力の前に、`module.functions`（特殊化の後）の直接参照の強連結成分を `tail_groups` で求める。
  `src/recursion.rs` の検査結果（`recursion::check` の戻り値は `unused_private` 用の参照）は使わない。
- 理由: 特殊化で関数 id が増えた後の本体に対して判定でき、`rec` の宣言と実際の循環が一致する。
- 状態: 既定案（実装者はこの案に従う。着手は D1 の承認後）
