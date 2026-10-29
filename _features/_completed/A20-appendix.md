# A20 付録: 分類表・規則の詳細・実測・検証テストの設計

本文: [A20-effect-summary-and-checked-attributes.md](A20-effect-summary-and-checked-attributes.md)。確認した HEAD は `9dd0fbbc`（起票の指示の `a2313d19` との差は `docs/benchmarks.md` だけ）。

## 付録 A: `TypedExprKind` の分類（64 variant。`src/check.rs`）

件数の確認: `awk '/^pub enum TypedExprKind/,/^}/' src/check.rs | grep -cE '^    [A-Z]'` が **64**。`src/effects.rs` の `match &expression.kind` は**この 64 行をそのまま arm にし、wildcard を置かない**。共通: どの式も、型が利用者の `Drop` を持つ値なら `user_drop`（と Drop 関数への辺）。`copies::sites` の箇所は式と別に関数へ足す。子は、下で触れないものは `expression.children()` で積む（`read = false`、`pattern` は親から引き継ぐ）。

| # | variant | 子の文脈 | 効果・辺・未知・免除 |
| --- | --- | --- | --- |
| 1 | `Error` | — | `unknown`（未解決） |
| 2 | `Int(u128)` | — | なし |
| 3 | `Float(String)` | — | なし |
| 4 | `String(StringLiteral)` | — | 非空: `read` でなければ `alloc`（`string-literal-moved`）、`frame_bytes > 65,536` も `alloc`（`literal-too-large`）。空: なし |
| 5 | `Bool(bool)` | — | なし |
| 6 | `Unit` | — | なし |
| 7 | `Break` | — | なし |
| 8 | `Continue` | — | なし |
| 9 | `Local(usize)` | — | なし |
| 10 | `Static(usize)` | — | なし（読み取り専用の静的データ） |
| 11 | `Function(FunctionRef)` | — | `User(id)`: 辺 `id`。`Builtin(i)`: `builtin_facts(i.builtin)`（`$builtin` の生成関数の本体にだけ残る） |
| 12 | `GenericFunction(usize, Vec<Type>)` | — | `unknown`（未解決） |
| 13 | `Method(usize, usize, Type)` | — | `unknown`（未解決） |
| 14 | `TypeFunction { .. }` | — | `unknown`（未解決） |
| 15 | `GenericInteger(u128, bool)` | — | `unknown`（未解決） |
| 16 | `GenericFloat(String)` | — | `unknown`（未解決） |
| 17 | `Unary(op, e)` | `children()` | なし（`Negate` は折り返し） |
| 18 | `Binary(op, l, r)` | 比較（`Equal`〜`GreaterEqual`）は両辺 `read = true`。他は `children()` | 文字列 `Add`: `alloc`（A4）。整数 `Divide`/`Remainder`: T1。整数 `Power`: T2。`Pipe`: 右辺が `Function`/`Closure` でなければ値の呼び出し。他: なし |
| 19 | `Call(callee, args)` | callee は `read = true`、引数は `read = false` | callee が `Function`/`Closure` でなければ値の呼び出し。`callee.ty` が関数型で引数が少なければ `alloc`（A3 `partial-application`） |
| 20 | `HostCall(import, args)` | `children()` | `extern`。免除の `trap` 箇所 `host-call` |
| 21 | `Lambda { .. }` | — | `unknown`（未解決。`closures::lower` の後には残らない） |
| 22 | `Closure(id, captures)` | `children()` | 辺 `id`。`captures` が非空、または型が `Task` なら `alloc`（A2） |
| 23 | `TaskRun(e)` | `children()` | `task`・`alloc`。**値の呼び出し**（冷たい `Task` を実行する。本体は被演算子の外で作られうる。常に） |
| 24 | `TaskParallel(e)` | `children()` | `task`・`alloc`（束ねた冷たい `Task` を作るだけ。実行は `TaskRun`） |
| 25 | `TaskParallelResults(e)` | `children()` | `task`・`alloc`（同上） |
| 26 | `Parallel(builtin, args)` | `children()` | `builtin_facts(builtin)` の `effect`・`trap`・`diverge`、`alloc`（A9）。`callbacks` が ○ なら値の呼び出し（常に） |
| 27 | `StructuralCompare(op, v)` | `children()`（要素ごとのメソッドが子） | なし（辺は子の `Function` が作る） |
| 28 | `StructuralHash(v)` | `children()` | なし |
| 29 | `StructuralDisplay(v)` | `children()` | `alloc`（A4） |
| 30 | `Interpolated(i)` | `children()`（`hole.operand`・`hole.method`） | `alloc`（A4） |
| 31 | `If { .. }` | `[cond][Enter(cond)][then][Leave][EnterElse(cond)][else][Leave]`。`then`・`else` は親の `read` を引き継ぐ | なし |
| 32 | `While { .. }` | `children()` | `diverge`（V1） |
| 33 | `ForRange { step, .. }` | `children()` | `trap`（T5）。免除 `literal-step` |
| 34 | `ForEach { source, body, .. }` | `source` は `read = true`、`body` は `false` | なし（配列・リスト・`Vec`・文字列だけ。有界） |
| 35 | `Match { value, arms, .. }` | `value` は `read = false`。腕の `steps`・`bindings` は `pattern = true`、`guard`・本体は `false`。本体は親の `read` を引き継ぐ | なし（T10） |
| 36 | `Block { bindings, result }` | 束縛の初期化子は `read = staying_locals に束縛の局所が入るか`。`result` は親の `read` を引き継ぐ | なし |
| 37 | `Record(fields)` | 要素は親の `read` を引き継ぐ | なし |
| 38 | `RecordUpdate { .. }` | `children()` | なし |
| 39 | `Array(elements)` | 要素は親の `read` を引き継ぐ（`pattern = false`） | 型が `Array` で非空: `read` でなければ `alloc`（`array-literal-moved`）、`frame_bytes > 65,536` も `alloc`。固定長配列（型が `FixedArray`）・空: なし |
| 40 | `List(elements)` | 同上 | 非空: `read` でなければ `alloc`（`list-literal-moved`）、`frame_bytes > 65,536` も `alloc` |
| 41 | `Tuple(elements)` | 要素は親の `read` を引き継ぐ | なし |
| 42 | `ListTail(v, n)` | `children()` | 免除の `trap` 箇所 `pattern-projection` |
| 43 | `NewArray(len, init)` | `children()` | `alloc`（A1）。`trap`（T6）、免除 `non-negative-length` |
| 44 | `NewList(len, init)` | `children()` | 同上 |
| 45 | `NewLiteral(e)` | `children()` | `alloc`（A1） |
| 46 | `Field(e, i)` | `children()` | なし |
| 47 | `Index(a, i)` | `a` は `read = true`（`pattern` を引き継ぐ）、`i` は `false` | `trap`（T3）。免除 `fixed-literal-index`・`pattern-projection`・`proven-index` |
| 48 | `Slice { value, start, end }` | `value` は `read = true` | `trap`（T4） |
| 49 | `Length(e)` | `e` は `read = true` | なし |
| 50 | `StringLength(e)` | `e` は `read = true` | なし |
| 51 | `Borrow(e, mutable)` | `e` は `read = !mutable` | なし |
| 52 | `BorrowOperand(e)` | `e` は `read = true` | なし |
| 53 | `Dereference(e)` | `children()` | なし（暗黙の複製は `copies::sites`） |
| 54 | `Assign(place, v)` | `children()` | なし |
| 55 | `Cast(e)` | `children()` | なし |
| 56 | `CaseConstructor { .. }` | — | `unknown`（未解決） |
| 57 | `Construct { payload, .. }` | `children()` | payload があり型が再帰的な union なら `alloc`（A5） |
| 58 | `UnionTag(e)` | `children()` | なし |
| 59 | `UnionPayload { .. }` | `children()` | なし |
| 60 | `Checked(e)` | `children()` | `trap`（T7） |
| 61 | `Try(t)` | `children()`（`body`・`handler`・`finally`） | なし |
| 62 | `RaisedException` | — | なし |
| 63 | `Reraise` | — | `trap`（T8） |
| 64 | `DynDispatch { .. }` | — | `unknown`（`dyn`） |

## 付録 B: `Builtin` の分類（246 variant）

件数の確認: `awk '/^pub enum Builtin \{/,/^}/' src/check.rs | grep -cE '^    [A-Z]'` が **246**、`Builtin::ALL` も 246（うち `#[cfg(test)]` の 3 つは通常のビルドにない）。下の **37 行がそのまま `builtin_facts` の arm**（variant の合計 246。数えた結果）。列: 効果 = D-53 の名前（`-` は純粋、`dyn` は未知）／`alloc`・`trap`・`diverge`（○）／`cb`（引数の関数を呼ぶ。値の呼び出し）／免除 = IR にトラップがあるが論理でない理由（`host_status`: ホストの状態コードの検査、`dead_guard`: 到達不能な守り、`refcount_overflow`: 参照カウントの桁あふれ、`runtime_internal`: ランタイム補助関数の内部検査）。IR の実測は `tests/fixtures`・`examples` の IR（150 個の組み込み関数が出現）と `Math.*`・`Int.*` の探索プログラム（付録 H の e5）で確かめた。

| 行 | 件数 | 効果 | alloc | trap | diverge | cb | 免除 | variant |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| B1 | 105 | - | - | - | - | - | - | Sqrt Floor Ceil Abs MathSqrt MathFloor MathCeil MathTrunc MathRound MathRoundEven MathAbs MathMin MathMax MathClamp MathFma MathCopysign MathIsNan MathIsInfinite MathIsFinite MathPi MathE MathSin MathCos MathTan MathAsin MathAcos MathAtan MathAtan2 MathExp MathExp2 MathLog MathLog2 MathLog10 MathPow MathCbrt MathHypot ToFloat ToInt StringIsWellFormed UnicodeTableLength Default Hash HashMix VecEmpty SimdSplat SimdOfLanes2 SimdOfLanes4 SimdOfLanes8 SimdOfLanes16 SimdOfLanes32 SimdSum SimdEq SimdNe SimdLt SimdLe SimdGt SimdGe SimdSelect SimdAll SimdAny VecLength VecCapacity VecIsEmpty VecPop VecOfArray VecToArray CharToU16 CharOfU16 Utf8CharToU32 Utf8CharOfU32 Utf8StringToBytes Utf8StringFromBytes StringCompare Utf8StringCompare IntMin IntMax IntAbs IntUnsignedAbs IntAbsDiff IntCountOnes IntLeadingZeros IntTrailingZeros IntRotateLeft IntRotateRight IntSwapBytes IntReverseBits IntIsPowerOfTwo IntCheckedAdd IntCheckedSub IntCheckedMul IntCheckedDiv IntCheckedRem IntCheckedNeg IntSaturatingAdd IntSaturatingSub IntSaturatingMul IntCheckedPow IntWideningMul OwnedDrop OwnedFunction Not Ignore TestAdd TestUnsigned TestWiden |
| B2 | 3 | - | - | - | - | - | dead_guard | VecTruncate VecClear VecGet |
| B3 | 4 | - | - | - | - | ○ | - | FixedArrayInit ListFoldRef SeqNext OwnedCall |
| B4 | 20 | - | - | ○ | - | - | - | Assert Unreachable UnicodeTableEntry ArraySet ArraySwap ArrayWrite ArraySwapIn ArraySplitAtMut ListTail SimdExtract SimdReplace SimdLoad SimdStore VecSet VecSwap VecAt Utf8CharOfU32Unchecked Utf8StringDecodeAt IntClamp IntWrappingPow |
| B5 | 1 | - | - | ○ | - | ○ | - | ArrayUpdate |
| B6 | 20 | - | ○ | - | - | - | - | CloneString CloneUtf8String StringFromUtf8 Utf8StringFromString StringToWellFormed ToString Display Parse DisplayQuoted ListCons ArrayConcat ArrayToList ListReverse ListToArray VecWithCapacity VecPush VecReserve VecClone StringToCodeUnits StringFromCodeUnits（#63 のレビュー: compact な文字列では単位を新しい領域へ広げる・狭める） |
| B7 | 3 | - | ○ | - | - | ○ | - | ArraySortBy ListMap ListMapRef |
| B8 | 1 | dyn | ○ | - | - | - | - | DynOf |
| B9 | 3 | io | - | - | - | - | - | IOWrite AsyncClock AsyncRetire |
| B10 | 4 | io | - | - | - | - | runtime_internal | AsyncTake AsyncPut AsyncNextId AsyncPosted |
| B11 | 1 | io | - | - | - | ○ | - | AsyncResume |
| B12 | 1 | io | - | - | ○ | - | runtime_internal | AsyncWait |
| B13 | 1 | io | ○ | - | ○ | - | host_status | IOReadLine |
| B14 | 2 | os | - | - | - | - | - | BenchNow GenSeed |
| B15 | 3 | os | - | - | - | - | host_status | OsClock OsSleep OsClose |
| B16 | 2 | os | ○ | - | - | - | host_status | OsArgs OsRandom |
| B17 | 5 | os | ○ | - | ○ | - | host_status | OsRead OsHandle OsSpawn OsWrite OsOpen（#63 のレビュー: パスを C のヒープへ複製し、FIFO を開くと相手を待つ） |
| B18 | 5 | net | - | - | - | - | host_status | NetClose NetClassify NetWatch NetUnwatch NetSend |
| B19 | 2 | net | - | - | ○ | - | host_status | NetWrite NetConnect |
| B20 | 1 | net | ○ | - | - | - | host_status | NetNames |
| B21 | 4 | net | ○ | - | ○ | - | host_status | NetResolve NetOpen NetAccept NetRead |
| B22 | 3 | task | ○ | - | - | - | - | TaskRun TaskParallel TaskParallelResults |
| B23 | 5 | task | ○ | - | - | ○ | - | ParallelInit ParallelMap ParallelMapRef ParallelReduce TaskScope |
| B24 | 1 | task | ○ | ○ | - | ○ | - | ParallelForEachChunk |
| B25 | 14 | atomic | - | - | - | - | - | AtomicLoad AtomicStore AtomicSwap AtomicCompareExchange AtomicFetchAdd AtomicFetchSub AtomicFetchAnd AtomicFetchOr AtomicFetchXor AtomicIntoInner MutexIntoInner ChannelCloneSender ChannelCloseSender ChannelCloseReceiver |
| B26 | 1 | atomic | - | - | - | - | runtime_internal | ArenaNextId |
| B27 | 2 | atomic | - | ○ | ○ | - | - | ChannelSend ChannelRecv |
| B28 | 1 | atomic | - | ○ | ○ | ○ | - | MutexWith |
| B29 | 2 | atomic | ○ | - | - | - | - | AtomicCreate MutexCreate |
| B30 | 1 | atomic | ○ | ○ | - | - | - | ChannelBounded |
| B31 | 10 | rc | - | - | - | - | - | RcGet RcStrongCount RcWeakCount RcTryUnwrap RcPtrEq ArcGet ArcStrongCount ArcWeakCount ArcTryUnwrap ArcPtrEq |
| B32 | 6 | rc | - | - | - | - | refcount_overflow | RcShare RcDowngrade RcUpgrade ArcShare ArcDowngrade ArcUpgrade |
| B33 | 2 | rc | ○ | - | - | - | - | RcNew ArcNew |
| B34 | 1 | debug | - | - | - | - | - | BenchConsume |
| B35 | 1 | debug | ○ | - | - | - | - | DebugPrintString |
| B36 | 4 | gpu | - | - | - | - | - | GpuOpen GpuFeatures GpuSelect GpuLast |
| B37 | 1 | gpu | ○ | - | - | - | - | GpuRun |

集計（`src/effects.rs` の単体テストが固定）: 不純（効果が `-` でない）は **90**（io 10・os 12・net 12・task 9・atomic 21・rc 18・debug 2・gpu 5・dyn 1）、純粋は 153（`cfg(test)` では 156）。`alloc` 49・`trap` 26（B4 の 20 + B5 + B24 + B27 の 2 + B28 + B30）・`diverge` 14・`cb` 16。A19 の「不純 90・純粋 156」と同じ件数。理由: `Assert`・`Unreachable` は論理トラップ。`Array.set` 系（`ArraySet`・`ArraySwap`・`ArrayWrite`・`ArraySwapIn`・`ArraySplitAtMut`・`ArrayUpdate`）・`ListTail`・`SimdExtract`・`SimdReplace`・`SimdLoad`・`SimdStore`・`VecSet`・`VecSwap`・`VecAt` は添字、`UnicodeTableEntry` は表の範囲（内部用）、`Int.clamp` は条件（最小が最大以下）、`Int.wrapping_pow` は負の指数、`Utf8Char.of_u32_unchecked`・`Utf8String.decode_at` は不正な符号位置、`Mutex.with_lock` は再入、`Channel.send`・`recv` は臨界区間内・全タスク待ち（デッドロック検出）、`Channel.bounded` は容量が 0 以下、`Parallel.for_each_chunk` は大きさが 0 以下（IR の `RangeStepZero`）。`diverge` は外部の事象を上界なしで待つもの（入出力の読み・待機・`accept`・チャネル・ロック）。`Parse` と `Display` は IR の出現がコーパスになく、`alloc` と保守的に置いた（結果の文字列）。

## 付録 C: `alloc` の規則の詳細

**C.1 `Why.code`（alloc）**: `new-collection`・`new-literal`・`closure-environment`・`partial-application`・`string-concat`・`string-build`・`recursive-node`・`string-literal-moved`・`array-literal-moved`・`list-literal-moved`・`literal-too-large`・`copy-site`・`task`・`parallel`・`builtin`。診断の `{what}`・`{help}`（本文 7.5）の鍵。

**C.2 IR の実測**（`tsuzuri build <dir> --emit llvm`。`@tz.alloc` に静的に至るか。試作の規則の判定と並べる。プローブは付録 H の H.3（e1）・H.7（大きいリテラル・再帰・部分適用））:

| 書き方 | 規則 | IR |
| --- | --- | --- |
| `let xs = [1, 2, 3]` を `xs[0]` で読む | なし | なし |
| `[n, n + 1, n + 2]` を返す | `alloc` | あり |
| `new [1, 2, 3]` | `alloc` | あり |
| `let s = "hello"` を `s.length` で読む | なし | なし |
| `"hello"` を `string` の引数へ値で渡す／返す | `alloc` | あり |
| `$"{a}{b}"` | `alloc` | あり |
| 500 要素・2,000 要素（16 KB）の配列リテラルを読むだけ | なし | なし |
| 9,000 要素（72 KB）の配列リテラルを読むだけ | `alloc` | あり（`frame_bytes > 65,536`） |
| 再帰関数の中の `let xs = [1, 2, 3]` を読む | なし | なし（配列の frame は `reserve_buffer` を使わない） |
| 再帰関数の中の `$"{s}x"` | `alloc` | あり |
| 1 つの捕捉つきクロージャ（`\x -> x + n`） | `alloc` | なし（即値の捕捉。規則が保守的） |
| 捕捉なしのクロージャ | なし | なし |
| `Node (Leaf, n, Leaf)`（再帰的な union） | `alloc` | あり |
| `([1, 2], [3, 4])` を `match` で取り出す | `alloc`（`match` の `value` は読み取りでない） | あり |
| ループの中の `let xs = [i, i + 1]` を読む | なし | なし |
| `String.starts_with "hello" "he"`（`ref string` の引数） | なし（`BorrowOperand`） | なし |
| `curry_add 0.1d128`（部分適用） | `alloc` | あり |
| `""`（空の文字列リテラル） | なし | 静的には `@tz.string.new(ptr, i64 0)`（確保しない）。テストの IR 走査は除く |

**C.3 偽陽性（規則が `alloc` で、IR に確保がない）**。コーパス 102 プログラム: 規則の `alloc` 3,384 関数、IR の確保 2,879 関数、偽陽性 **505**。理由別の関数数（1 関数が複数の理由を持つので合計は 505 を超える）: `closure-environment` 221・`copy-site` 125・`partial-application` 66・`task` 59・`builtin` 48・`string-literal-moved` 17・`array-literal-moved` 10・`string-concat` 8・`list-literal-moved` 1。意味: 捕捉が即値で入るクロージャ（IR は環境を作らない）、`Environment` の複製、タスクの生成を保守的に `alloc` にしている。精密にするなら IR 側の `immediate_capture`（`llvm_frame.rs`）を使う別チケット。**偽陽性の件数にテストの閾値を置かない**（報告するだけ）。

**C.4 試作が見つけた穴と対処**（最初の版で `alloc` の違反が 43 関数あった。直した順）: ① 部分適用の `Call`（A3）→ `alloc`。② 空の文字列リテラルの静的呼び → 規則でなく IR 走査を直す（長さ定数 0 の `@tz.string.new`・`@tz.utf8string.new` を除く）。③ `K::Parallel` の式を `builtin_facts` の全列で扱う（`trap` を落としていた）。④ `HostCall` の免除の箇所を記録する（ホストの入出力の検査が `BoundsCheck`・`Encoding` として IR に出る）。この 4 つで違反が 0 になった。

## 付録 D: `trap` の規則の詳細

**D.1 `TrapKind`（16）の方針**（`src/trap.rs`。IR の発行元は `guard`・`emit_trap`）:

| `TrapKind` | 主な発行元 | 方針 |
| --- | --- | --- |
| `Assert` | `assert`・`Mutex.with_lock` の再入・`Channel` の臨界区間内操作・容量 0 | 論理 |
| `IntegerDivisionByZero`・`IntegerDivisionOverflow` | 整数の `/`・`%` | 論理（T1） |
| `BoundsCheck` | `Index`・`Slice`・`Array.set` 系・`Vec.at`/`set`/`swap`・`Simd.load`/`store`/`extract`/`replace`・`List.tail` | 論理（T3・T4、付録 B） |
| `RangeStepZero` | `ForRange` の刻み 0・`Parallel.for_each_chunk` の大きさ 0 | 論理（T5） |
| `MatchFailure` | `unreachable` | 論理 |
| `Overflow` | `@checked` の桁あふれが `try` に捕捉されない | 論理（T7） |
| `NumericRuntime` | 整数の `**` の負の指数・`Int.clamp`・`Int.wrapping_pow` | 論理（T2・付録 B）。参照カウントの桁あふれ・ランタイム補助関数の既定の種類は**論理でない**（免除の理由 `refcount_overflow`・`runtime_internal`） |
| `Encoding` | `Utf8Char.of_u32_unchecked`・`Utf8String.decode_at`・ABI の文字列の検査 | 論理（組み込み関数）。ABI は `export` の境界で、関数の本体のトラップでない |
| `PatternMismatch` | `match` の全腕が外れた後・`ListTail` | **到達不能**（網羅は `E1021`、パターンの長さの `Test` が先）。含めない |
| `AllocationSize` | `allocation_size`（長さが負か大きすぎる）・`Channel.bounded` の大きさ | 資源。ただし `NewArray`・`NewList` の**負の長さ**は論理として T6 で拾う |
| `AllocationFailure`・`StringConcatOverflow`・`StackOverflow`・`DebugOutput`・`WasmRuntime` | 確保失敗・文字列の長さ・スタック・デバッグ出力・WASM | 資源。含めない（D-53） |

**D.2 免除の理由コード**（`Site.exempt = true` の `why`）: `literal-divisor`・`unsigned-or-literal-exponent`・`fixed-literal-index`・`pattern-projection`・`proven-index`・`literal-step`・`non-negative-length`・`host-call`、組み込み関数の `host_status`・`dead_guard`・`refcount_overflow`・`runtime_internal`。**理由なしの免除は書かない**（停止条件 2）。

**D.3 `RangeFacts::enter_else_condition`（`src/ranges.rs`。検証済みの試作。付録 I にコード）**: `if c then A else B` の `B` で成り立つのは `not c`。`c` が `a \|\| b \|\| ...` の鎖なら `not a && not b ...`。各比較を否定し（`Less`↔`GreaterEqual`、`LessEqual`↔`Greater`。`Equal`・`NotEqual` は否定しても使えない）、`lower_term`・`upper_term`（既存）で `non_negative` と `below` を集め、`enter_condition` と同じく、**両方が `stable` で `non_negative` に入る組だけ**を `guards` に積む。`narrowed` は 0（区間の絞り込みはしない）。戻り値は `Entered` で、`leave_condition` が元へ戻す。エミッターは呼ばない。試作の確認: `if i < 0 \|\| i >= xs.length then 0 else xs[i]` → 免除 `proven-index`（`Array.get` の本体も）、`i > xs.length`（弱い）→ 拒否のまま。

**D.4 限界**: 証明は `Type::Array` の添字だけ（`Vec`・`List`・`String`・`Slice` は常に `trap`）。`Map`・`Set`・`HashMap` の内部の計算された添字は証明できず `trap` になる（付録 E の表の `Map.get`・`HashMap.insert`）。したがって `@no_trap` から呼べる std は、添字を持たないもの（`Array.length`・`Array.is_empty`・`Math.*`・`Int.*`・`String.length`・`Maybe`・`Result` の操作）と、証明できる `Array.get` に限られる。広げる手段（std の監査と E15 の `@trusted`）は対象外。

## 付録 E: 実測

**E.1 コーパスと結果**（試作を `tests/fixtures` の 100 個のディレクトリと `examples` の 13 個に当てた。`Main.tz` を持つ 102 個が対象。残りの 11 個は `tsuzuri build` が E2001（入口 `Main.tz` がない）で止まる。`ls -d tests/fixtures/*/ examples/*/ | wc -l` が 113）:

| 項目 | 値 |
| --- | --- |
| プログラム / 関数 / IR の記号に対応した関数 | 102 / 11,910 / 11,837 |
| 規則の `alloc` / IR の確保 / 違反（IR にあって規則にない）/ 偽陽性 | 3,384 / 2,879 / **0** / 505 |
| IR の論理トラップの箇所（利用者のコード。`@tz.trap.report(i32 定数)`） | 1,101（規則が拒否または免除で対応しない: **0**） |
| 継承文脈の論理トラップがある std・組み込み・生成関数 | 848（規則に `trap` の箇所がない: **0**） |
| 使われた免除の理由と件数 | `literal-divisor` 347・`host-call` 9・`literal-step` 4・`pattern-projection` 8 |
| 組み込み関数の IR（150 個が出現）の論理トラップの種類と表の食い違い | 付録 B の「免除」で説明（`host_status`・`dead_guard`・`refcount_overflow`・`runtime_internal`）。表が `trap` で IR にない: `UnicodeTableEntry` だけ（トラップは補助関数の中。内部用なので `trap` のまま） |
| std の公開 `def`（`grep -hE "^(export )?def " std/*.tz std/*.tc \| wc -l`） | 559。うちコーパスで実体が出たもの 443、未被覆 116（付録 G。署名 116 行） |

**E.2 std の関数 45 個の規則と IR**（試作の**局所の**判定（自分の本体。呼び先の分は含まない）と、IR の自分の本体の確保・種類。`--trap-info` なしの `@llvm.trap` の種類は継承文脈の `add i32 %tz.context, KIND` から。コーパスで最初に出た実体）:

| 関数 | 規則 alloc | 理由 | IR alloc | 規則 trap | 箇所 | IR のトラップの種類 |
| --- | --- | --- | --- | --- | --- | --- |
| `Array.length` | - | - | - | - | - | - |
| `Array.is_empty` | - | - | - | - | - | - |
| `Array.get` | - | - | - | -（else の証明） | 免除 `proven-index` | bounds（else の守りは IR に残る） |
| `Array.at` | - | - | - | ○ | index | bounds |
| `Array.init` | ○ | new-collection | ○ | ○ | allocation-size | allocsize |
| `Array.sub` | ○ | copy-site | ○ | ○ | slice | allocsize, bounds |
| `Array.reverse` | ○ | closure-environment, new-collection | ○ | - | 免除 non-negative-length | allocsize |
| `Array.copy` | ○ | copy-site | ○ | - | - | allocsize |
| `Array.append` | ○ | closure-environment, new-collection | ○ | ○ | allocation-size | allocsize |
| `Array.zip` | ○ | closure-environment, new-collection | ○ | - | 免除 non-negative-length | allocsize |
| `Array.map`・`Array.mapi` | ○ | closure-environment, new-collection | ○ | - | 免除 non-negative-length | allocsize |
| `Array.fold` | ○ | copy-site | -（偽陽性） | - | - | - |
| `Array.sum`・`Array.sort`・`Array.contains` | - | - | - | - | - | - |
| `List.length` | - | - | - | - | - | - |
| `List.fold` | ○ | copy-site | -（偽陽性） | - | - | - |
| `String.length`・`String.starts_with`・`String.contains`・`String.concat` | - | - | - | - | - | - |
| `String.trim`・`Utf8String.trim` | - | - | - | ○ | index | bounds |
| `String.split` | ○ | closure-environment, new-collection | ○ | - | 免除 non-negative-length | allocsize |
| `Utf8String.length`・`Vec.iter`・`Set.contains`・`Set.insert`・`HashMap.get`・`Maybe.is_none`・`Seq.defer` | - | - | - | - | - | - |
| `Map.get` | - | - | - | ○ | index | bounds |
| `Map.insert`・`Maybe.map`・`Result.map`・`Json.serialize` | - | - | - | - | - | pattern（到達不能） |
| `HashMap.insert` | - | - | - | ○ | index | bounds, pattern |

一致の読み方: 「IR alloc ○」の行はすべて規則も ○（違反 0）。規則が ○で IR が - の行は偽陽性（`Array.fold`・`List.fold` は `Environment` の複製の箇所）。`Array.get` の else の添字は `enter_else_condition` で免除される（IR には守りが残る。免除の箇所として記録されるので、検証テストは対応ありとみなす）。

**E.3 走査の費用**（試作。release。同一機械。`copies::sites`・`ranges::analyze`・走査の合計。解析 = `Project::analyze`）:

| プログラム | 関数 | 解析 (ms) | 走査 (ms) | 比 |
| --- | --- | --- | --- | --- |
| `tests/fixtures/json` | 947 | 248〜270 | 35 | 13% |
| `tests/fixtures/net_sockets` | 407 | 113〜132 | 10 | 8% |
| `tests/fixtures/regex` | 191 | 98〜106 | 10〜11 | 10% |
| `tests/fixtures/value_layout` | 167 | 234〜278 | 64〜71 | 27% |
| `tests/fixtures/matrix` | 210 | 59〜64 | 3 | 5% |
| `tests/fixtures/tensor` | 131 | 61〜65 | 2〜3 | 4% |

debug ビルドの内訳（`value_layout`: 全体 669 ms のうち `copies::sites` 630 ms、残りの走査 38 ms、そのうち `ranges::analyze` 13 ms。`json`: 330 ms のうち 287 ms が `copies::sites`）。**費用の約 90% は `copies::sites`**。したがって Phase 1（`copy`・`alloc` を計算しない）の走査は解析の約 2%、`copies::sites` を呼ぶ Phase 2 以降は 8〜27%。属性・`pure` がなければ計算しない（D3）。上の数字は試作の測定で、最適化していない（`children()` が節ごとに `Vec` を作る）。

## 付録 F: 規則が IR より保守的であることの検証テストの設計

**共通**（`tests/effects_alloc_conservative.rs`・`tests/effects_trap.rs`。AGENTS の「境界と参照のテスト」）:

- コーパス: `tests/fixtures/*/` と `examples/*/` のうち `Main.tz` を持つディレクトリ（HEAD で 102 個）と、掃引の fixture `tests/fixtures/effects_sweep/`（付録 G）。`assert!(projects >= 100)` で、パスの取り違えを検出する。
- 規則側: `Project::load(dir)`→`analyze()`→`check::effects::summarize(&module, Options { sites: true, ..Options::new(Scope::All, EffectSet::ALL) })`。関数ごとに**直接の源だけ**の集合 `FunctionEffects::local(id) -> EffectSet` と、`FunctionEffects::sites(id) -> &[Site]`（`Site { effect, span, why, exempt }`。拒否も免除も）を使う（本文の API に足す。`Options.sites` が真のときだけ持つ）。**推移的な集合は使わない**（IR の「自分の本体」と比べるため）。
- IR 側: `std::process::Command` で `env!("CARGO_BIN_EXE_tsuzuri") build <dir> --emit llvm --trap-info -o <out>.ll`。`<out>.ll` と `<out>.ll.trap.json` ができる。**`--emit llvm` の IR は `-O0` と `-O3` で同一**（`cmp` で確認）なので `-O` は固定でよい。
- 記号の対応: `function.qualified_name()` ↔ `@tz.fn.<名前>`。対応しない関数（`@tz.specialized` に置き換わったもの・出力されなかったもの）は数えず、対応した数を出力する（HEAD: 11,910 関数中 11,837）。
- 失敗の出し方: 違反の関数名・理由別の箇所・IR の確保の経路（`string.new -> string.allocate -> alloc` のような補助関数の連鎖）を列挙して `panic!`。

**F.1 `tests/common/ir_scan.rs`（新規。正規表現の crate は使えないので手で書く）**:

```rust
pub struct IrFunction { pub calls: BTreeSet<String>, pub indirect: bool, pub trap: bool }
pub fn parse(ir: &str) -> BTreeMap<String, IrFunction>;           // 記号（先頭の '@' つき）→ 本体
pub fn own_alloc(functions: &BTreeMap<String, IrFunction>, symbol: &str) -> bool;
```

1. `define ` で始まる行が関数の始まり。記号は最初の `(` の直前の `@名前`（文字は英数字と `_ . $ -`、または `@"..."`）。行が `}` だけなら終わり。
2. 本体の各行で、`call ` から最初の `(` までを取り、空白で分けた**最後の語**が呼び先。`%…` は間接呼び出し（`indirect`）、`@llvm.trap` は `trap`、それ以外の `@名前` は `calls` に入れる。ただし `@tz.string.new`・`@tz.utf8string.new` は、行に `, i64 0,` か `, i64 0)` を含む（長さが定数 0。確保しない）とき入れない。
3. 利用者レベルの接頭辞 `@tz.fn.`・`@tz.specialized.`・`@tz.apply.`。それ以外の関数は補助関数。
4. `own_alloc(f)`: `f` の `calls` に確保の記号（`@tz.alloc`・`@tz.realloc`・`@tz.alloc.base`・`@tz.realloc.base`・`@tz.heap.alloc.unlocked`・`@tz.heap.realloc.unlocked`・`@malloc`・`@realloc`・`@calloc`・`@tsuzuri_alloc`）がある、または補助関数だけを経由して（利用者レベルの関数へは入らない）到達する。利用者レベルの関数へ入らない理由: その先の確保は呼び先の関数自身の `alloc` と、要約の伝播が表す。

**F.2 `alloc`**: `rule_is_at_least_as_conservative_as_the_ir`: 対応した全関数で `own_alloc(f) ⇒ local(f).contains(Alloc)`。**違反 0**。`false_positive_count_is_reported`: 規則が `alloc` で IR にない関数の数と、`Site.why` ごとの内訳を `println!`（閾値を置かない）。違反が出たら、規則を保守的な向きに足す（付録 C.4 の ①〜④ の例）。

**F.3 `trap`**（3 つのテストは `tests/effects_trap.rs`）:

- **(a) 利用者のコードの箇所** `ir_trap_sites_are_covered_by_the_rule`: `<out>.ll` の `call void @tz.trap.report(i32 N)`（`N` が数字の定数）の `N` を、その行を含む `define` の記号とともに集める（`real`）。保持する関数が `@tz.fn.`・`@tz.apply.`・`@tz.specialized.` でないもの（`@tz_*` の ABI の入口。ホストの引数の検査）は除く。`trap.json` の `sites` のうち `id ∈ real` かつ `kind` が論理（`assertion failed`・`integer division by zero`・`integer division overflow`・`index out of bounds`・`range step is zero`・`numeric runtime trap`・`invalid Unicode encoding`・`unhandled OverflowException…`・`non-exhaustive match`）のものすべてに、同じ `(path, start, end)`（なければ同じ `(path, start)`）の `Site`（拒否・免除の両方。`effect == Trap`）が規則側にあること。`path` は `build` に渡したディレクトリ（テストは絶対パスで渡す）の文字列で始まるので、`<dir>/` を落とし、`\` を `/` にそろえて、規則側の `span.source`（`Option<usize>`。`Some(i)` のとき）の `Project::source_paths()[i]`（新規。`src/driver.rs`。各 source の `relative_path` を文字列にして返すだけ）と比べる。`kind` が資源・到達不能（`pattern mismatch`）のものは除く。`id ∉ real` の箇所（**呼び出し文脈**: 呼び出し式の span に callee の種類の和を持たせたもの）は比べない。
- **(b) 継承文脈**（std・組み込み・生成関数）`inherited_trap_kinds_are_covered`: 関数の本体の `%tz.trap.N = add i32 %tz.context, KIND` の `KIND`（`TrapKind` の判別値: `Assert` 0・`IntegerDivisionByZero` 1・`IntegerDivisionOverflow` 2・`BoundsCheck` 3・`AllocationSize` 4・`AllocationFailure` 5・`RangeStepZero` 6・`MatchFailure` 7・`PatternMismatch` 8・`StringConcatOverflow` 9・`NumericRuntime` 10・`WasmRuntime` 11・`Encoding` 12・`DebugOutput` 13・`StackOverflow` 14・`Overflow` 15）が `{0, 1, 2, 3, 6, 7, 10, 12, 15}` を含む関数は、規則側に `effect == Trap` の `Site` が 1 つ以上ある。HEAD: 対象 848 関数、違反 0。
- **(c) 組み込み関数の表** `builtin_table_explains_every_ir_trap`: `@tz.builtin.<surface>.<型>` の本体の `add i32 %tz.context, KIND`（補助関数へは入らない）が論理を含むなら、`builtin_facts(b).trap || exempt_trap.is_some()`。HEAD: 出現 150 個で、`trap` が偽かつ免除なしは 0（付録 B）。

**F.4 掃引** `std_public_functions_are_all_swept`: `std/*.tz`・`std/*.tc` の `^(export )?def ` の名前（559。`grep -hE "^(export )?def " std/*.tz std/*.tc | wc -l`）が、(i) コーパスのプログラムの関数名（`origin == Std`・`.$mono.N` を除く）か、(ii) 掃引の fixture の関数のどれかに現れること。HEAD: (i) で 443、残りの 116 を (ii) の `tests/fixtures/effects_sweep/Main.tz` が受け持つ。fixture は付録 G の署名ごとに、型変数を `i64` に、`'key`・`'value` を `i64` に、制約を満たす具体型に置き換えた**型注釈つきの値**を 1 行ずつ書く（例: `let array_get: i64 -> ref [i64] -> Maybe<i64> = Array.get`）。モジュール内の型名は修飾する（`Handle` → `File.Handle`）。**この fixture は `check` が通ること**を手順 14 で確かめ、書けなかった関数は `tests/effects_alloc_conservative.rs` の `SWEEP_EXCLUDED: &[(&str, &str)]`（名前と理由）に入れる（理由なしは禁止。件数が 10 を超えたら停止して報告）。

## 付録 G: 未被覆の std 公開関数（116。コーパスで実体が出ないもの。署名は `std/` の `def` の行そのまま）

型名はそのモジュールの中の名前（`Handle` は `File.Handle`、`Device`・`Backend`・`Buffer` は `Gpu` の型、`Pcg` は `Random.Pcg`）。掃引の fixture の対象。

```text
Array.count :: (ref 'a -> bool) -> ref ['a] -> i64
Array.equal :: Eq<'a> => ref ['a] -> ref ['a] -> bool
Array.get :: Copy<'a> => i64 -> ref ['a] -> Maybe<'a>
Bench.of :: (unit -> 'b) -> i64 -> i64
Bench.with_input :: (unit -> 'a) -> (ref 'a -> 'b) -> i64 -> i64
Dir.create :: string -> IO<Result<unit, Os.Error>>
Dir.list :: string -> IO<Result<[string], Os.Error>>
Dir.remove :: string -> IO<Result<unit, Os.Error>>
Dir.walk :: string -> IO<Result<[string], Os.Error>>
Env.args :: unit -> IO<Result<[string], Os.Error>>
Env.current_dir :: unit -> IO<Result<string, Os.Error>>
Env.var :: string -> IO<Result<Maybe<string>, Os.Error>>
File.append_text :: string -> string -> IO<Result<unit, Os.Error>>
File.close :: Handle -> IO<Result<unit, Os.Error>>
File.flush :: Handle -> IO<Result<unit, Os.Error>>
File.link_metadata :: string -> IO<Result<Metadata, Os.Error>>
File.metadata :: string -> IO<Result<Metadata, Os.Error>>
File.open :: string -> Mode -> IO<Result<Handle, Os.Error>>
File.read :: Handle -> i64 -> IO<Result<[ubyte], Os.Error>>
File.read_bytes :: string -> IO<Result<[ubyte], Os.Error>>
File.read_text :: string -> IO<Result<string, Os.Error>>
File.remove :: string -> IO<Result<unit, Os.Error>>
File.with_open :: Capture<'a> => string -> Mode -> (Handle -> IO<'a>) -> IO<Result<'a, Os.Error>>
File.write :: Handle -> [ubyte] -> IO<Result<unit, Os.Error>>
File.write_bytes :: string -> [ubyte] -> IO<Result<unit, Os.Error>>
File.write_text :: string -> string -> IO<Result<unit, Os.Error>>
Gen.array :: Gen<'a> -> Gen<['a]>
Gen.array_up_to :: i64 -> Gen<'a> -> Gen<['a]>
Gen.bind :: ('a -> Gen<'b>) -> Gen<'a> -> Gen<'b>
Gen.bool :: Gen<bool>
Gen.char :: Gen<char>
Gen.check :: (ref 'a -> string) -> i64 -> Gen<'a> -> ('a -> bool) -> unit
Gen.constant :: Copy<'a> => 'a -> Gen<'a>
Gen.element :: Copy<'a> => ['a] -> Gen<'a>
Gen.f64 :: Gen<f64>
Gen.for_all :: Display<'a> => Gen<'a> -> ('a -> bool) -> unit
Gen.for_all_cases :: Display<'a> => i64 -> Gen<'a> -> ('a -> bool) -> unit
Gen.i32 :: Gen<i32>
Gen.i64 :: Gen<i64>
Gen.map :: ('a -> 'b) -> Gen<'a> -> Gen<'b>
Gen.maybe :: Gen<'a> -> Gen<Maybe<'a>>
Gen.one_of :: [Gen<'a>] -> Gen<'a>
Gen.pair :: Gen<'a> -> Gen<'b> -> Gen<('a * 'b)>
Gen.range :: i64 -> i64 -> Gen<i64>
Gen.result :: Gen<'a> -> Gen<'e> -> Gen<Result<'a, 'e>>
Gen.string :: Gen<string>
Gen.string_of :: Gen<char> -> Gen<string>
Gen.unicode_char :: Gen<char>
Gpu.backend :: ref Device -> Backend
Gpu.from_array :: Copy<'a> => ref Device -> ref ['a] -> Buffer<'a>
Gpu.init :: ref Device -> i64 -> (i32 -> 'a) -> Buffer<'a>
Gpu.init_relaxed :: ref Device -> i64 -> (i32 -> 'a) -> Buffer<'a>
Gpu.last_backend :: unit -> Backend
Gpu.map :: Copy<'a> => ref Device -> ('a -> 'b) -> Buffer<'a> -> Buffer<'b>
Gpu.map_relaxed :: Copy<'a> => ref Device -> ('a -> 'b) -> Buffer<'a> -> Buffer<'b>
Gpu.request :: Backend -> Result<Device, Error>
Gpu.to_array :: Buffer<'a> -> ['a]
HashMap.is_empty :: ref HashMap<'key, 'value> -> bool
HashMap.randomized :: unit -> IO<HashMap<'key, 'value>>
HashMap.to_array :: (Copy<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> [('key * 'value)]
HashMap.try_randomized :: unit -> IO<Result<HashMap<'key, 'value>, Os.Error>>
HashMap.values :: Copy<'value> => ref HashMap<'key, 'value> -> ['value]
HashSet.is_empty :: ref HashSet<'key> -> bool
HashSet.randomized :: unit -> IO<HashSet<'key>>
HashSet.try_randomized :: unit -> IO<Result<HashSet<'key>, Os.Error>>
HashSet.with_capacity :: i64 -> HashSet<'key>
IO.Combine :: IO<unit> -> IO<'a> -> IO<'a>
IO.For :: (Copy<'a>, Capture<'a>) => ['a] -> ('a -> IO<unit>) -> IO<unit>
IO.MergeSources :: IO<'a> -> IO<'b> -> IO<('a * 'b)>
IO.Return :: Capture<'a> => 'a -> IO<'a>
IO.ReturnFrom :: IO<'a> -> IO<'a>
IO.Using :: (('a -> 'b) -> 'c) -> ('a -> IO<'b>) -> IO<'c>
IO.While :: (unit -> bool) -> IO<unit> -> IO<unit>
IO.try_write_error :: (Display<'a>, Capture<'a>) => 'a -> IO<Result<unit, Error>>
IO.try_write_error_line :: (Display<'a>, Capture<'a>) => 'a -> IO<Result<unit, Error>>
IO.write_error :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
IO.write_error_line :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
IO.writeln :: (Display<'a>, Capture<'a>) => 'a -> IO<unit>
Json.decode :: Decode<'a> => ref Value -> Result<'a, Error>
Json.serialize_pretty :: Encode<'a> => i64 -> ref 'a -> Result<utf8string, Error>
Json.to_i64 :: ref Numeral -> Result<i64, Error>
List.is_empty :: ref [|'a|] -> bool
Map.is_empty :: ref Map<'key, 'value> -> bool
Maybe.ReturnFrom :: Maybe<'a> -> Maybe<'a>
Net.bind_async :: Address -> Async<Result<TcpListener, Os.Error>>
Net.bind_udp_async :: Address -> Async<Result<UdpSocket, Os.Error>>
Net.with_accepted :: Capture<'a> => TcpListener -> Maybe<i64> -> (TcpStream -> IO<'a>) -> IO<Result<'a, Os.Error>>
Net.with_listener :: Capture<'a> => Address -> (TcpListener -> IO<'a>) -> IO<Result<'a, Os.Error>>
Os.decode :: [ubyte] -> Result<string, Error>
Os.message :: ref Error -> string
Os.split_names :: ref [ubyte] -> Result<[string], Error>
Path.extension :: ref string -> Maybe<string>
Path.file_name :: ref string -> Maybe<string>
Path.join :: ref string -> ref string -> string
Path.parent :: ref string -> Maybe<string>
Random.bytes :: i64 -> IO<Result<[ubyte], Os.Error>>
Random.next_u64 :: unit -> IO<Result<i64u, Os.Error>>
Random.pcg :: i64u -> i64u -> Pcg
Random.pcg_next_u32 :: Pcg -> (i32u * Pcg)
Random.pcg_next_u64 :: Pcg -> (i64u * Pcg)
Set.is_empty :: ref Set<'key> -> bool
Set.remove :: Ord<'key> => 'key -> Set<'key> -> Set<'key>
String.sub :: i64 -> i64 -> ref string -> Maybe<string>
String.trim_end :: ref string -> string
String.trim_start :: ref string -> string
Tensor.data { r } :: ref View<'a> { r } -> ref { r } ['a]
Test.equal :: Eq<'a> => ref 'a -> ref 'a -> unit
Test.is_true :: bool -> unit
Test.not_equal :: Eq<'a> => ref 'a -> ref 'a -> unit
Time.monotonic_ns :: unit -> IO<Result<i64, Os.Error>>
Time.sleep_ms :: i64 -> IO<Result<unit, Os.Error>>
Time.unix_ns :: unit -> IO<Result<i64, Os.Error>>
Unicode.version :: unit -> string
Utf8String.sub :: i64 -> i64 -> ref utf8string -> Maybe<utf8string>
Utf8String.trim_end :: ref utf8string -> utf8string
Utf8String.trim_start :: ref utf8string -> utf8string
```

## 付録 H: 再現コマンドと実出力（HEAD `9dd0fbbc`）

作業の場所 `$WORK` はリポジトリの外に作る（`$WORK/p1/Main.tz` のように、1 ディレクトリ 1 プログラム）。`tsuzuri` は `cargo build --release` の `target/release/tsuzuri`。下の出力は 2026-10-18 に HEAD で取った実測。

**H.1 件数と IR の性質**

```sh
# 件数
awk '/^pub enum TypedExprKind/,/^}/' src/check.rs | grep -cE '^    [A-Z]'          # 64
awk '/^pub enum Builtin \{/,/^}/' src/check.rs | grep -cE '^    [A-Z]'              # 246
awk "/pub const ALL: &'static \[Self\] = &\[/,/^    \];/" src/check.rs | grep -cE '^        Self::[A-Za-z0-9]+,'   # 246
awk '/^pub enum Builtin \{/,/^}/' src/check.rs | grep -c 'cfg(test)'               # 3
awk '/^pub enum TrapKind/,/^}/' src/trap.rs | grep -cE '^    [A-Z][A-Za-z]*,'        # 16
awk '/^pub enum Type \{/,/^}/' src/check.rs | grep -cE '^    [A-Z]'                  # 30
grep -hE "^(export )?def " std/*.tz std/*.tc | wc -l                                # 559
# --emit llvm の IR は -O に依らない（p1 = 付録 H.2 の 3 行。出力は identical、各 3,759 バイト）
tsuzuri build $WORK/p1 --emit llvm -O0 -o $WORK/a.ll
tsuzuri build $WORK/p1 --emit llvm -O3 -o $WORK/b.ll
cmp $WORK/a.ll $WORK/b.ll && echo identical
# --trap-info は <out>.ll と <out>.ll.trap.json（{"version", "sites": [{"id", "kind", "path", "span": {"start", "end", ...}}]}）を書く
tsuzuri build $WORK/e4 --emit llvm --trap-info -o $WORK/e4.ll        # e4 は H.4。trap.json の sites は 15 件
# 全プログラム: Main.tz を持つディレクトリ 102 個が build できる。残りの 11 個は E2001（入口 Main.tz がない）で止まる
for d in tests/fixtures/*/ examples/*/; do [ -f "$d/Main.tz" ] && tsuzuri build "$PWD/$d" --emit llvm --trap-info -o "$WORK/$(basename $(dirname $d))-$(basename $d).ll"; done
```

**H.2 p1**（IR 同一の確認）:

```tsuzuri
def f :: f64 -> f64 = \x -> Math.sqrt x + 1.0
def g :: ref [i64] -> i64 = \xs -> Array.sum xs
def main :: unit -> i32 = \() -> 0
```

**H.3 e1（alloc。規則 / IR）**。`$WORK/e1/Main.tz`（`tsuzuri check` は警告なしで通る）:

```tsuzuri
record P { x: i64, y: i64 }
union Tree = Leaf | Node of (Tree * i64 * Tree)

def a_scalar :: i64 -> i64 = \n -> n * 2 + 1
def a_record :: i64 -> P = \n -> P { x: n, y: n + 1 }
def a_arr_read :: i64 -> i64 = \n ->
    let xs = [1, 2, 3]
    xs[0] + n
def a_arr_return :: i64 -> [i64] = \n -> [n, n + 1, n + 2]
def a_arr_new :: i64 -> i64 = \n ->
    let xs = new [1, 2, 3]
    xs[0] + n
def a_str_return :: i64 -> string = \_n -> "hello"
def a_takes :: string -> i64 = \s -> s.length
def a_str_pass :: i64 -> i64 = \n -> a_takes "hello" + n
def a_str_concat :: ref string -> ref string -> string = \a b -> $"{a}{b}"
def a_closure_env :: i64 -> i64 = \n ->
    let f = \x -> x + n
    f 3
def a_closure_noenv :: i64 -> i64 = \n ->
    let f = \x -> x + 1
    f n
def a_tree :: i64 -> Tree = \n -> Node (Leaf, n, Leaf)
def a_fixed :: i64 -> i64 = \n ->
    let a: [i64; 3] = FixedArray.init (\i -> i + n)
    a[1]

def main :: unit -> i32 = \() -> 0
```

試作の判定と IR（自分の本体だけ。呼び先は辺として別。実測）:

| 関数 | 規則 `alloc` | IR の確保 | 備考 |
| --- | --- | --- | --- |
| `a_scalar`・`a_record`・`a_arr_read`・`a_takes`・`a_closure_noenv` | なし | なし | スタックの値・フレームの配列・捕捉なしのクロージャ |
| `a_arr_return`・`a_arr_new`・`a_str_return`・`a_str_pass`・`a_str_concat`・`a_tree` | あり | あり | 返す配列・`new`・文字列リテラルの移動・連結・再帰的な union |
| `a_closure_env`・`a_fixed` | あり | なし | **偽陽性**（1 つの `i64` の捕捉は即値で入る。規則は保守的に `closure-environment`） |

加えて 500 要素の `i64` 配列リテラル（4,000 バイト）を `xs[0]` で読むだけの関数は、規則・IR ともなし。

**H.4 e4（trap・diverge）**。`$WORK/e4/Main.tz`（`tsuzuri check` は警告なし）:

```tsuzuri
def a_div :: i64 -> i64 -> i64 = \a b -> a / b
def a_div2 :: i64 -> i64 = \a -> a / 2
def a_idx :: ref [i64] -> i64 -> i64 = \xs i -> xs[i]
def a_get :: ref [i64] -> i64 -> i64 = \xs i ->
    match Array.get i xs with
    | Maybe.Some v -> v
    | Maybe.None -> 0
def a_match :: Maybe<i64> -> i64 = \m ->
    match m with
    | Maybe.Some v -> v
    | Maybe.None -> 0
def a_list :: ref [|i64|] -> i64 = \xs ->
    match xs with
    | [|a; b|] -> a + b
    | _ -> 0
def a_guard :: ref [i64] -> i64 -> i64 = \xs i ->
    if i >= 0 && i < xs.length then xs[i] else 0
def a_guard2 :: ref [i64] -> i64 -> i64 = \xs i ->
    if i < 0 || i >= xs.length then 0 else xs[i]
def a_loop :: ref [i64] -> i64 = \xs ->
    let mut s = 0
    for i in 0 .. (xs.length - 1) do
        s = s + xs[i]
    s
def a_assert :: i64 -> i64 = \a ->
    assert (a > 0)
    a
def a_step :: i64 -> i64 -> i64 = \a s ->
    let mut t = 0
    for i in 0 .. s .. a do
        t = t + i
    t
def a_while :: i64 -> i64 = \n ->
    let mut i = 0
    while i < n do
        i = i + 1
    i
def main :: unit -> i32 = \() -> 0
```

試作の局所の判定（`Site` = 効果・理由・拒否か免除か。辺 = 呼び先の数。実測）:

| 関数 | 局所の効果 | `Site` | 辺 |
| --- | --- | --- | --- |
| `a_div` | `trap` | `integer-division`（拒否） | 0 |
| `a_div2` | なし | `literal-divisor`（免除。IR は守りを残す） | 0 |
| `a_idx` | `trap` | `index`（拒否） | 0 |
| `a_get` | なし | なし | 1（`Array.get`） |
| `a_match` | なし | なし（`match` は `trap` を出さない） | 0 |
| `a_list` | なし | `pattern-projection` 2 件（免除） | 0 |
| `a_guard` | なし | `proven-index`（免除。IR に守りなし） | 0 |
| `a_guard2` | なし | `proven-index`（免除。`enter_else_condition` が要る。IR には守りが残る） | 0 |
| `a_loop` | なし | `literal-step`・`proven-index`（どちらも免除） | 0 |
| `a_assert` | なし | なし | 1（`$builtin.assert`。そこに `trap` の `builtin`） |
| `a_step` | `trap` | `range-step`（拒否） | 0 |
| `a_while` | `diverge` | なし | 0 |

`enter_else_condition` を入れる前の試作では `a_guard2` は `index`（拒否）だった。入れた後は免除。

**H.5 e6（`enter_else_condition` の境界。`$WORK/e6/Main.tz`、警告なし）**:

```tsuzuri
def uses_get :: ref [i64] -> i64 -> i64 = \xs i ->
    match Array.get i xs with
    | Maybe.Some v -> v
    | Maybe.None -> 0
def uses_sub :: ref [i64] -> [i64] = \xs -> Array.sub 0 1 xs
def a_guard2 :: ref [i64] -> i64 -> i64 = \xs i ->
    if i < 0 || i >= xs.length then 0 else xs[i]
def a_guard3 :: ref [i64] -> i64 -> i64 = \xs i ->
    if i < 0 || i > xs.length then 0 else xs[i]
def uses_mapget :: i64 -> i64 = \k ->
    let m = Map.insert k k Map.empty()
    match Map.get k (ref m) with
    | Maybe.Some v -> v
    | Maybe.None -> 0
def main :: unit -> i32 = \() -> 0
```

試作の局所の判定: `a_guard2` = `proven-index`（免除）、**`a_guard3` = `index`（拒否。`i > xs.length` では `i == xs.length` が通るので証明しない）**。呼び先の実体: `Array.get.$mono` = `proven-index`（免除）で `trap` なし、`Array.sub.$mono` = `alloc`・`copy`・`trap`（`slice` の拒否と `copy-site`）、`Map.get.$mono` = `trap`（`index` の拒否）、`Map.insert.$mono` = `diverge`（`while`）、`$builtin.Vec.push` = `alloc`（`builtin`）、`$builtin.assert` = `trap`（`builtin`）。したがって `@no_trap` の関数は `Array.get` を呼べて、`Map.get`・`Array.sub` は呼べない（D.4）。

**H.6 費用の測り方**（`#[ignore]` の測定テストか、使い捨ての `examples/` で。結果を印字するだけ。しきい値を置かない）: `Project::load(dir)` の後、`Instant::now()` で `project.analyze()` と `effects::summarize(&module, Options { sites: true, ..Options::new(Scope::All, EffectSet::ALL) })` をそれぞれ測り、`tests/fixtures/json`（947 関数）・`value_layout`（167 関数）で release の比を出す。試作の実測: `json` 解析 248〜270 ms に対し走査 35 ms、`value_layout` 解析 234〜278 ms に対し走査 64〜71 ms。比が 50% を超えたら、`copies::sites` を関数ごとに呼び直していないか（手順 11。呼ぶのは `summarize` で 1 回）を見直す。

**H.7 付録 C.2 の残りの行**（大きいリテラル・再帰・部分適用。規則 / IR は実測）。大きいリテラルは生成して作る（`python3 $WORK/gen_e2.py $WORK/e2/Main.tz`）:

```python
import sys

def literal(count):
    return "[" + ", ".join(str(i) for i in range(count)) + "]"

source = "".join(
    f"def f{count} :: i64 -> i64 = \\x ->\n    let xs = {literal(count)}\n    xs[0] + x\n"
    for count in (500, 2000, 9000)
)
open(sys.argv[1], "w").write(source + "def main :: unit -> i32 = \\() -> 0\n")
```

`f500`（4,000 バイト）・`f2000`（16,000 バイト）は なし／なし、`f9000`（72,000 バイト > 65,536）は alloc／あり。続いて `$WORK/e3/Main.tz`（`check` は警告なし）:

```tsuzuri
def rec rec_small :: i64 -> i64 = \n ->
    let xs = [1, 2, 3]
    if n <= 0 then xs[0] else rec_small (n - 1) + xs[1]
def rec rec_env :: i64 -> i64 = \n ->
    let f = \x -> x + n
    if n <= 0 then 0 else rec_env (n - 1) + f 1
def rec rec_concat :: ref string -> i64 -> i64 = \s n ->
    let t = $"{s}x"
    if n <= 0 then t.length else rec_concat s (n - 1)
def list_lit :: i64 -> i64 = \n ->
    let xs = [|1, 2, 3|]
    List.length (ref xs) + n
def str_ref :: i64 -> bool = \_n ->
    String.starts_with "hello" "he"
def tuple_arr :: i64 -> i64 = \n ->
    let t = ([1, 2], [3, 4])
    match t with
    | (a, b) -> a[0] + b[0] + n
def loop_lit :: i64 -> i64 = \n ->
    let mut total = 0
    for i in 0 .. n do
        let xs = [i, i + 1]
        total = total + xs[1]
    total
def add3 :: i64 -> i64 -> i64 -> i64 = \a b c -> a + b + c
def partial :: i64 -> i64 -> i64 = \a -> add3 a 1
def main :: unit -> i32 = \() -> 0
```

| 関数 | 規則 `alloc` | IR の確保 | 理由 |
| --- | --- | --- | --- |
| `rec_small`・`list_lit`・`str_ref`・`loop_lit` | なし | なし | 再帰関数の中でもフレームの配列は確保しない。ループの中のリテラルも読むだけ。`ref string` の引数は借用 |
| `rec_env` | あり | なし（偽陽性） | 1 つの `i64` の捕捉は即値で入る |
| `rec_concat` | あり | あり | 再帰関数の文字列連結は `reserve_buffer` を使えずヒープ |
| `tuple_arr` | あり | あり | `match` の `value` は読み取りでなく、配列が外へ出る。`a[0]` の添字は規則の `trap` |
| `partial` | あり | あり | 部分適用は環境を作る（A3） |

## 付録 I: 試作の骨格（コンパイルと全コーパスで検証済み。実装の手本。そのまま使ってよい）

試作は `src/effects.rs`（約 550 行。このチケットの設計の Phase 1〜5 の局所の走査）と `examples/effects_probe.rs`（走査して JSON Lines を出す。どちらも試作の複製にだけあり、リポジトリにはない）で、複製したリポジトリで `cargo check` 13 秒、`cargo build --example` 33 秒（debug）、release 2 分 36 秒だった。**`match &expression.kind` は 64 variant をすべて `_ =>` なしで列挙してコンパイルが通り**、`builtin_facts` も `_ =>` なしでコンパイルが通った（付録 B の 37 行を arm にし、B1 から `#[cfg(test)]` の 3 つを別 arm にした。付録 A・B が漏れなくそろっている証拠）。骨格:

```rust
enum Item<'a> { Node(&'a TypedExpr, Ctx), Enter(&'a TypedExpr), EnterElse(&'a TypedExpr), Leave }
#[derive(Clone, Copy, Default)]
struct Ctx { read: bool, pattern: bool }

fn scan_function(module: &CheckedModule, id: usize, copy_sites: &[CopySite], user_drops: bool) -> Local {
    let function = &module.functions[id];
    let types = module.types();
    let mut local = Local::default();
    for site in copy_sites {                                   // A8
        local.add(Effect::Alloc, site.span, "copy-site");
        if site.cost == CopyCost::Length { local.add(Effect::Copy, site.span, "copy-site"); }
    }
    let facts = &mut crate::ranges::analyze(module, function, false);
    let staying = crate::llvm::staying_locals(&function.body);       // pub(crate) の再公開（手順 12）
    let mut entered: Vec<crate::ranges::Entered> = Vec::new();
    let mut stack = vec![Item::Node(&function.body, Ctx::default())];
    while let Some(item) = stack.pop() {
        let (expression, ctx) = match item {
            Item::Enter(condition) => { entered.push(facts.enter_condition(module, condition)); continue; }
            Item::EnterElse(condition) => { entered.push(facts.enter_else_condition(module, condition)); continue; }
            Item::Leave => { facts.leave_condition(entered.pop().unwrap()); continue; }
            Item::Node(expression, ctx) => (expression, ctx),
        };
        local.nodes += 1;
        if user_drops && !types.stored_all(&expression.ty, |ty| !ty.has_user_drop(&types)) {
            local.add(Effect::UserDrop, expression.span, "user-drop");
        }
        let mut pushed = false;
        match &expression.kind {                                  // wildcard なしの 64 arm（付録 A）
            K::Binary(operator, left, right) => { /* A4・T1・T2・Pipe。比較は両辺 read = true で積んで pushed = true */ }
            K::Call(callee, arguments) => {                       // 値の呼び出し・部分適用（A3）・callee は read、引数は非 read
                if !matches!(callee.kind, K::Function(_) | K::Closure(..)) { local.value_call.get_or_insert(expression.span); }
                if matches!(&callee.ty, Type::Function(p, _) if arguments.len() < p.len()) { local.add(Effect::Alloc, expression.span, "partial-application"); }
                for argument in arguments.iter().rev() { stack.push(Item::Node(argument, Ctx { read: false, pattern: ctx.pattern })); }
                stack.push(Item::Node(callee, Ctx { read: true, pattern: ctx.pattern }));
                pushed = true;
            }
            K::If { condition, then_branch, else_branch } => {    // llvm.rs と同じ入退場の順序
                stack.push(Item::Leave);
                stack.push(Item::Node(else_branch, ctx));
                stack.push(Item::EnterElse(condition));
                stack.push(Item::Leave);
                stack.push(Item::Node(then_branch, ctx));
                stack.push(Item::Enter(condition));
                stack.push(Item::Node(condition, Ctx::default()));
                pushed = true;
            }
            K::Index(array, index) => {                           // T3
                let verdict = match &array.ty {
                    Type::FixedArray(_, length) => { let Type::Length(count) = **length else { unreachable!() };
                                                     matches!(index.kind, K::Int(v) if v < count as u128).then_some("fixed-literal-index") }
                    _ if ctx.pattern && matches!(index.kind, K::Int(_)) => Some("pattern-projection"),
                    _ if facts.index_in_bounds(array, index) => Some("proven-index"),
                    _ => None,
                };
                match verdict { Some(why) => local.exempt(Effect::Trap, expression.span, why), None => local.add(Effect::Trap, expression.span, "index") }
                stack.push(Item::Node(index, Ctx::default()));
                stack.push(Item::Node(array, Ctx { read: true, pattern: ctx.pattern }));
                pushed = true;
            }
            /* ... 残りの 59 arm は付録 A の表のとおり ... */
        }
        if !pushed { let mut children = expression.children(); children.reverse(); for child in children { stack.push(Item::Node(child, Ctx { read: false, pattern: ctx.pattern })); } }
    }
    local
}
```

`src/ranges.rs` に足した 2 つ（検証済み。`enter_condition` の隣）:

```rust
/// Records what the `else` branch of `if condition` knows: `not (a || b)` is `not a && not b`.
/// Only the effect summary calls it; the emitter does not, so no emitted check changes (A20).
pub(crate) fn enter_else_condition(&mut self, module: &CheckedModule, condition: &TypedExpr) -> Entered {
    let mut non_negative = BTreeSet::new();
    let mut below = Vec::new();
    let mut terms = vec![condition];
    while let Some(term) = terms.pop() {
        let TypedExprKind::Binary(operator, left, right) = &term.kind else { continue };
        if *operator == BinaryOp::Or { terms.push(right); terms.push(left); continue; }
        let Some(negated) = negate(*operator) else { continue };
        non_negative.extend(lower_term(negated, left, right));
        below.extend(upper_term(module, negated, left, right));
    }
    let before = self.guards.len();
    for (index, array) in below {
        if non_negative.contains(&index) && self.stable.contains(&index) && self.stable.contains(&array) {
            self.guards.push((index, array));
        }
    }
    Entered { guards: self.guards.len() - before, narrowed: 0 }
}

/// The comparison that holds when `operator` does not.
fn negate(operator: BinaryOp) -> Option<BinaryOp> {
    match operator {
        BinaryOp::Less => Some(BinaryOp::GreaterEqual),
        BinaryOp::LessEqual => Some(BinaryOp::Greater),
        BinaryOp::Greater => Some(BinaryOp::LessEqual),
        BinaryOp::GreaterEqual => Some(BinaryOp::Less),
        _ => None,
    }
}
```

`builtin_facts` は付録 B の 37 行から機械的に作る（試作は Python で行ごとに arm を生成。人が書くなら、同じ列の variant を `B::A | B::B | ...` に並べ、`BuiltinFacts { effect: Some(Effect::Io), alloc: true, trap: false, diverge: true, callbacks: false, dynamic: false, exempt_trap: Some("host_status") }` のように値を書く）。

試作と本チケットの違い（本チケットに従う）: ① 試作は `TaskRun` を `diverge`（`task-join`）にしたが、本チケットは `diverge` にせず**値の呼び出し**とする（付録 A の 23。冷たい `Task` の本体は辺が運ぶ）。② 試作は `Parallel` の `callbacks` を値の呼び出しにしなかったが、本チケットは常に値の呼び出しとする（付録 A の 26）。③ 試作の `why` の名前 `parallel-chunk`・`task-join` と、組み込み関数の `diverge` の `builtin` は、付録 J の `builtin`・（なし）・`blocking` にする。④ 試作の `Site` は `Local` に直接持つが、本チケットは `FunctionEffects::sites` で返す。どれも、IR との比較（違反 0）の結果を変えない。

## 付録 J: 診断の部品（`Why.code` の一覧・プレースホルダー・`{what}`・`{help}`）

**J.1 `Why.code` の一覧**（**ここが唯一の定義**。付録 B・C・D と本文の表の名前はここから取る。直接の源の箇所の `why`）:

| 効果 | 拒否の箇所の `why` |
| --- | --- |
| `alloc` | `new-collection`・`new-literal`・`closure-environment`・`partial-application`・`string-concat`・`string-build`・`recursive-node`・`string-literal-moved`・`array-literal-moved`・`list-literal-moved`・`literal-too-large`・`copy-site`・`task`・`parallel`・`builtin`（15） |
| `copy` | `copy-site` |
| `trap` | `integer-division`・`negative-exponent`・`index`・`slice`・`range-step`・`allocation-size`・`checked-arithmetic`・`reraise`・`builtin`（9） |
| `diverge` | `while`・`cycle`・`blocking`（組み込み関数の `diverge` 列） |
| `extern` | `extern-call`（`HostCall` を本体に持つ関数の中） |
| `user_drop` | `user-drop` |
| `task` | `task`（`TaskRun`・`TaskParallel`・`TaskParallelResults`）・`parallel`（`Parallel`） |
| `atomic`・`debug`・`gpu`・`io`・`net`・`os`・`rc` | `builtin`（`Why.builtin` を持つ） |
| 免除（`Site.exempt = true`） | `literal-divisor`・`unsigned-or-literal-exponent`・`fixed-literal-index`・`pattern-projection`・`proven-index`・`literal-step`・`non-negative-length`・`host-call`、組み込み関数の `host_status`・`dead_guard`・`refcount_overflow`・`runtime_internal`（付録 B・D） |

`cycle` の箇所は、その関数から**同じ強連結成分の関数への辺**のうち span が最小のもの（自己再帰は自分への辺）。メッセージの `{chain}` は `'{fn}' -> '{その辺の先}'`（自己再帰は `'f' -> 'f'`）。環の全体は書かない。

**J.2 プレースホルダー**:

| 名前 | 値 |
| --- | --- |
| `{fn}` | 属性を持つ宣言の関数の `qualified_name()`。`.$mono.N` の接尾辞は落とす（インスタンスの診断も宣言の名前） |
| `{attr}` | 属性名（`no_alloc`・`no_copy`・`no_trap`・`terminates`。`@` を含めない。メッセージには `'@{attr}'` と書く） |
| `{callee}` | 家族の外へ出る最初の関数の表示名。`qualified_name()` から `.$mono.N` を落とし、`$builtin` の生成関数は `Builtin::name()`（例 `Vec.push`） |
| `{builtin}` | `Why.builtin` の `Builtin::name()` |
| `{name}` | `effect_extern_in_attribute` で、連鎖の最後（`HostCall` を持つ関数）の `qualified_name()` |
| `{chain}` | 連鎖が 2 関数以上のとき ` (through 'A' -> 'B')`（`A` は `{callee}`、最大 4 個。超える分は `-> ...`）。1 関数、または `why` が `cycle` のときは空 |
| `{cycle}` | `'{fn}' -> '{辺の先}'`（J.1 の `cycle` の規則）。環の全体は書かない |
| `{because}` | 直接の証拠: `this expression {what}`。呼び出し経由: `this call reaches '{callee}', which {what}{chain}` |
| `{why_unknown}` | `Unknown::Dyn`: `uses dynamic dispatch ('dyn')`。`Unresolved`: `contains an expression that is not resolved`。`ValueCall`: `runs a function or task value that its caller can choose` |

**J.3 `{what}`（`{because}` の動詞句）と `{help}`**（効果と `why` で決まる。同じ行の `why` は同じ文）:

| 効果 | `why` | `{what}` | `{help}` |
| --- | --- | --- | --- |
| `alloc` | `new-collection`・`new-literal` | `allocates an array or list` | `write into an existing buffer ('ref mut [T..]' with 'Array.write'), or remove the attribute` |
| `alloc` | `string-literal-moved`・`array-literal-moved`・`list-literal-moved` | `leaves its slot, so it is copied to the heap` | `borrow it ('ref') instead of passing or returning it` |
| `alloc` | `literal-too-large` | `is larger than 65536 bytes, so it is heap allocated` | `use static data ('const') or a fixed-length array` |
| `alloc` | `closure-environment`・`partial-application` | `needs a closure environment` | `use a named function, or a lambda that captures nothing` |
| `alloc` | `string-concat`・`string-build` | `builds a new string` | `write bytes into a buffer you own, or remove the attribute` |
| `alloc` | `recursive-node` | `creates a heap-allocated node of a recursive union` | `use an array and indices instead of a recursive union` |
| `alloc`・`copy` | `copy-site` | `implicitly copies every element` | `borrow it with 'ref', or call 'Array.copy' to make the copy explicit` |
| `alloc` | `task`・`parallel` | `creates tasks` | `remove the tasks, or remove the attribute` |
| `alloc` | `builtin` | `allocates` | `use a non-allocating alternative, or remove the attribute` |
| `trap` | `index` | `indexes without a proof that the index is in range` | `guard it ('if i >= 0 && i < xs.length then xs[i] else ...') or use 'Array.get' and match the Maybe` |
| `trap` | `slice` | `slices without a proof that the range is valid` | 同上 |
| `trap` | `integer-division` | `divides by a value that may be zero` | `check the divisor first, or divide by a non-zero literal` |
| `trap` | `negative-exponent`・`allocation-size`・`range-step` | `uses a value that may be negative or zero` | `check it before use, or use a non-negative literal` |
| `trap` | `checked-arithmetic`・`reraise` | `can raise an unhandled OverflowException` | `use the default (wrapping) arithmetic, or 'Int.checked_*' and match the Maybe` |
| `trap` | `builtin` | `can trap` | `check its arguments first, or remove the attribute` |
| `diverge` | `while` | `contains a 'while' loop, which may not terminate` | `use 'for' over a range or a collection` |
| `diverge` | `cycle` | `is recursive (through {cycle})` | `'@terminates' accepts only non-recursive functions; use a loop over a range` |
| `diverge` | `blocking` | `waits for an event outside the program` | `remove the wait, or remove the attribute` |

`{cycle}` の例: 直接の証拠なら `this expression is recursive (through 'f' -> 'f')`、経由なら `this call reaches 'g', which is recursive (through 'g' -> 'g')`。`copy-site` は `alloc` と `copy` の両方の効果の理由で、`@no_copy` の診断も同じ文。

**J.4 診断の実例**（すべて想定出力。G22 の人間向け形式。G22 が未実装なら `kind`・`related`・`help` を省き、メッセージだけが出る）:

```text
Main.tz:9:15: error[E1031]: 'Main.fill' is marked '@no_alloc' but this call reaches 'Array.init', which allocates an array or list; write into an existing buffer ('ref mut [T..]' with 'Array.write'), or remove the attribute
  9 |     let out = Array.init n (\i -> i * 2)
    |               ^^^^^^^^^^
  note: required by this attribute
    --> Main.tz:7:1
  7 | @no_alloc
    | ^^^^^^^^^
```

```text
Main.tz:5:5: error[E1031]: 'Main.pick' is marked '@no_trap' but this expression indexes without a proof that the index is in range; guard it ('if i >= 0 && i < xs.length then xs[i] else ...') or use 'Array.get' and match the Maybe
  5 |     xs[i]
    |     ^^^^^
```

```text
Main.tz:3:5: error[E1031]: 'Main.run' is marked '@terminates' but this expression is recursive (through 'Main.run' -> 'Main.step'); '@terminates' accepts only non-recursive functions; use a loop over a range
  3 |     step (n - 1)
    |     ^^^^
```

```text
Main.tz:1:1: warning[W1008]: '@no_alloc' on 'Main.first' is checked for each instantiation, and nothing instantiates it
  1 | @no_alloc
    | ^^^^^^^^^
```

最初の例の JSON（`--json`。1 診断 1 行）。キーの順序は D-51 のとおり `severity`・`code`・`message`・`path`・`span`・`kind`・`related`・`help`。`kind`・`related`・`help` は値があるときだけ出し、`fixes` は常に空なので出さない。`related` の要素は `role`・`message`・`path`・`span`（`span` は主スパンと同じ 6 キー: `start`・`end`・`line`・`column`・`end_line`・`end_column`。`start`・`end` は UTF-8 のバイトオフセット、`end` は排他）。`role` の文字列は G22 が決める（ここでは `required_by`、連鎖は `via_call`）:

```json
{"severity":"error","code":"E1031","message":"'Main.fill' is marked '@no_alloc' but this call reaches 'Array.init', which allocates an array or list; write into an existing buffer ('ref mut [T..]' with 'Array.write'), or remove the attribute","path":"Main.tz","span":{"start":120,"end":130,"line":9,"column":15,"end_line":9,"end_column":25},"kind":"effect_alloc_in_no_alloc","related":[{"role":"required_by","message":"required by this attribute","path":"Main.tz","span":{"start":84,"end":93,"line":7,"column":1,"end_line":7,"end_column":10}}],"help":"write into an existing buffer ('ref mut [T..]' with 'Array.write'), or remove the attribute"}
```

**J.5 `EffectRow`（本文 7.6。G26 の `effects`・`effect_sites`・`complete` の元）**: `EffectRow { id: usize, effects: Vec<&'static str>（辞書順の名前）, unknown: Option<&'static str>（`dyn`・`unresolved`・`value_call`）, sites: Vec<EffectSite { effect, span, why: &'static str, via: Vec<usize> }>（効果ごとに最初の 1 件、辞書順。`via` は関数 id の鎖で、直接なら空）, exempt_sites: Vec<EffectSite>（免除の箇所。G26 の `proven: true`）, complete: bool（`computed` が 14 個すべて。G26 の `effects_complete`）}`。`rows` は関数 id と位置だけを返し、ID の文字列化と JSON は G26。

