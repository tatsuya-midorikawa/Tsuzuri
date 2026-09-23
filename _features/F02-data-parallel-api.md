# F02: データ並列 API（Parallel.init／map／reduce）
| 項目 | 内容 |
|---|---|
| ID | F02 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | F01, C03, C04 |
| 後続 | F07 |
| 状態 | todo |
| 主な影響ファイル | `std/Parallel.tz`（新規または E02 後）, `src/check.rs`, `src/closures.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/call_specialization.rs`, `tests/parallel.rs`（新規）, `tests/parallel.mjs`（新規）, `tests/fixtures/parallel/*`（新規）, `benchmarks/run-parallel.mjs`（新規）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md` |

## 目的

- 配列の生成・変換・集計を、明示的なデータ並列 API として提供する。
- `Task.parallel` のタスク配列より低い allocation / closure / scheduling overhead で、連続データを処理できるようにする。
- F01 の常駐ワーカープールを再利用し、ネイティブでは CPU コアを使える。
- WASM の既定 backend では D-18 に従い import なしの逐次 fallback を使う。
- 浮動小数点や非結合的な reducer でも、実行スレッド数・CPU・WASM fallback に依存しない結果順序を定義する。
- F07 の GPU backend が同じ高水準 API から offload 可能かを判断できる、明確な kernel 境界を作る。

## 現状

- 標準ライブラリ同梱機構は E02 の対象であり、現状 `std/` はまだない。
- `_features/GUIDE.md` の D-07 は `Parallel` モジュールを標準ライブラリ名として予約している。
- `src/check.rs` の `Builtin` には `TaskRun` と `TaskParallel` があり、名前は `Task.run` / `Task.parallel`。
- `src/check.rs` の `Checker::value_expression` は `ExprKind::Field(Name("Task"), field)` を特別扱いし、`Task.run` / `Task.parallel` 以外を `E1002` にする。
- `src/check.rs` の `Builtin::signature` は現在 1 引数組み込みを前提にしている。
- `src/closures.rs` の `Checker::lambda` は `task: bool` 引数を持ち、task lambda では捕捉値と結果に `Send` を要求する。
- `src/closures.rs` の `lower_expression` は `Builtin::TaskParallel` を通常関数ではなく `TaskParallel` を返す task lambda へ変換する。
- `src/ownership.rs` は `TypedExprKind::TaskRun` と `TaskParallel` を consume として扱う。
- `src/ownership.rs` は task closure / task lambda の捕捉値が loans を保持している場合 `E1013` を返す。
- `src/llvm.rs` の `%tz.closure` ABI は `{ ptr code, ptr environment, ptr clone, ptr drop }`。
- `src/llvm.rs` の `FunctionEmitter::make_closure` は捕捉環境を `@tz.alloc` し、`closure_descriptor` を作る。
- `src/llvm.rs` の `FunctionEmitter::closure_descriptor` は非 task かつ捕捉 prefix が `can_capture` のときだけ clone pointer を入れる。
- `src/llvm.rs` の apply adapter は `closure_wrappers` で生成され、`@tz.apply.<name>.<count>` は環境を消費して次段階または worker を呼ぶ。
- `src/llvm.rs` の apply adapter は `count != 0` の環境を `@tz.free` する。
- `src/llvm.rs` の `FunctionEmitter::apply_value` は `%tz.closure` から code/environment を取り出し、一引数ずつ呼ぶ。
- `src/llvm.rs` の `FunctionEmitter::clone_value` は `Type::Function(..)` に `@tz.closure.clone` を使う。
- `src/llvm.rs` の `FunctionEmitter::clone_value` は `Type::Task(_)` を `unreachable!("single-use tasks cannot be cloned")` としている。
- `src/llvm.rs` の `FunctionEmitter::allocate_array` は `%tz.array = { ptr, i64 }` と data pointer を返す。
- `src/llvm.rs` の `FunctionEmitter::array_loop` は LLVM IR の単純な index loop を生成する。
- `src/llvm.rs` の `FunctionEmitter::parallel_tasks` は `tsuzuri_task_parallel` を呼ぶ既存のネイティブ並列 lowering。
- `src/runtime/task-wasm.ll` は `@tsuzuri_task_parallel` を input order の逐次 loop として定義している。
- `src/runtime/task.c` は F01 後に常駐ワーカープールになる予定で、このチケットはその C ABI を使う。
- `docs/language.md` の D-14 相当の仕様では、一括演算は評価・集計順序を API 契約として文書化する必要がある。
- `docs/benchmarks.md` は性能主張に matched workload と生成コード確認を要求している。

## 仕様

### モジュールと名前

- 新 API は D-07 に従い `Parallel` モジュールに置く。
- 利用者は `Parallel.init`, `Parallel.map`, `Parallel.map_ref`, `Parallel.sum`, `Parallel.reduce` と修飾名で呼ぶ。
- 無修飾 `init`, `map`, `map_ref`, `sum`, `reduce` は追加しない。
- 既存の `Task` 名前空間とは別物。
- `Parallel` は標準ライブラリ予約モジュール名で、ユーザーファイル `Parallel.tz` との衝突は E02 の規則で `E1011`。
- 低水準 lowering が必要なため、実装は純粋な `std/Parallel.tz` だけに閉じなくてよい。
- std 側には署名・docs 用 wrapper を置き、実体は `Builtin` または専用 `TypedExprKind` で下げる。

### 型

- `Parallel.init : i64 -> (i64 -> 'a) -> ['a]`
- `Parallel.map : (Copy 'a, Send 'a, Send 'b) => ('a -> 'b) -> &['a] -> ['b]`
- `Parallel.map_ref : (Send 'a, Send 'b) => (&'a -> 'b) -> &['a] -> ['b]`
- `Parallel.sum : (Numeric 'a, Add 'a, Copy 'a, Send 'a) => &['a] -> 'a`
- `Parallel.reduce : (Copy 'a, Send 'a) => 'a -> ('a -> 'a -> 'a) -> &['a] -> 'a`
- `Parallel.init` の結果要素 `'a` には `Send 'a` を要求する。
- `Parallel.map` は C04 の `Array.map` と同じく、共有 slice から mapper へ owned element を渡すため `Copy 'a` を要求する。
- `Parallel.map` は各 input element を `clone_value` で複製してから mapper へ渡す。`Copy` でも配列・リスト・関数値は deep clone や closure clone を伴うため、bit copy や descriptor reuse にしてはならない。
- `Parallel.map_ref` は非 Copy input 用で、mapper に `&'a` を渡す。
- `Parallel.map` の入力要素 `'a` と出力要素 `'b` には `Send` を要求する。
- `Parallel.map_ref` は入力要素 `'a` と出力要素 `'b` に `Send` を要求し、借用引数 `&'a` は fork/join 内に閉じる。
- `Parallel.map` の入力は C03 の `&['a]`、つまり配列全体またはスライスの共有借用。
- `Parallel.reduce` は borrowed input から値を読むため、要素型に `Copy` を要求する。
- `Parallel.sum` は `Parallel.reduce` の特殊形で、空入力では対象型の加法単位元を返す。
- `Parallel.sum` の整数単位元は `0` を対象整数幅・符号にした値。
- `Parallel.sum` の binary / decimal 浮動小数点単位元は `+0`。
- `Parallel.sum` は string には使えない。`Add string` は存在するが `Numeric string` ではない。
- 関数引数は reusable な関数値であり、task ではない。
- 関数値の捕捉環境が借用を保持する場合は拒否する。
- 未知の関数値で hidden borrowed environment があるか証明できない場合は保守的に拒否する。

### 評価順序

- `Parallel.init length initializer` は、`length` 式を評価し、次に `initializer` 式を評価する。
- `length < 0` は配列生成と同じく trap。
- `length` と要素サイズの積が overflow する場合は trap。
- `initializer` 式自体は一度だけ評価する。
- `Parallel.map mapper input` は `mapper` 式を評価し、次に `input` 式を評価する。
- `Parallel.map_ref mapper input` も `mapper` 式を評価し、次に `input` 式を評価する。
- `Parallel.reduce identity reducer input` は `identity`, `reducer`, `input` の順に評価する。
- input slice の範囲計算・借用は fork 前に完了する。
- result array の確保は worker 起動前に行う。
- 各 worker / chunk 内の element callback は、その chunk の添字昇順で呼ぶ。
- chunk 間の実行順は未規定。
- reducer の chunk 内 fold 順は添字昇順。
- partial results の結合順は chunk index 昇順。

### 決定的 chunking

- D-14 に従い、chunk 境界は入力長だけで決める。
- thread count、online CPU 数、F01 pool の worker 数、実行マシン、OS、WASM/native の違いで chunk 境界を変えない。
- このチケットの既定 chunking は次の式とする。
- `PARALLEL_TARGET_CHUNK_ITEMS = 4096`。
- `PARALLEL_MAX_CHUNKS = 1024`。
- `chunk_count(n) = 0` if `n == 0`。
- `raw_chunks(n) = n / PARALLEL_TARGET_CHUNK_ITEMS + (if n % PARALLEL_TARGET_CHUNK_ITEMS == 0 then 0 else 1)` if `n > 0`。
- `chunk_count(n) = min(PARALLEL_MAX_CHUNKS, raw_chunks(n))` if `n > 0`。
- `chunk_start(c, n, k) = (n / k) * c + ((n % k) * c) / k`。
- `chunk_end(c, n, k) = (n / k) * (c + 1) + ((n % k) * (c + 1)) / k`。
- ここで `k = chunk_count(n)`、`0 <= c < k`。
- `c` と `k` は `i64` へ zext した非負値で、`k <= 1024`、`0 <= c < k`。
- `base = n / k`、`rem = n % k` として計算し、`base * c <= n`、`base * (c + 1) <= n`、`rem * (c + 1) <= 1023 * 1024` なので i64 overflow しない。
- native と WASM はこの除算先行の同じ整数式を使い、`n * c / k` や `i128` helper へ置き換えない。
- `n` は `i64` だが、事前に `n >= 0` を検査する。
- parallel threshold は実行方法だけを変える。
- `k == 1` でも同じ chunk order を使う。
- 実装が「小さい入力では sequential にする」場合でも、同じ chunking と同じ reduction order を使う。
- WASM sequential fallback も同じ chunk loop を使う。

### reduce / sum の意味

- `Parallel.reduce identity reducer input` は次の仕様上の参照実装と同じ結果。
- まず `k = chunk_count(input.length)`。
- `k == 0` なら `identity` を返す。
- 各 chunk `c` について `partial[c] = identity` から始める。
- chunk 内では `i = start..end-1` の昇順に `partial[c] = reducer partial[c] input[i]`。
- 全 chunk 完了後、`result = identity` から始める。
- `c = 0..k-1` の昇順に `result = reducer result partial[c]`。
- この仕様により、非結合的 reducer の結果は左から右の逐次 reduce と一致しない場合がある。
- ただし同じ入力長・同じ要素・同じ reducer・同じ identity では、スレッド数と実行 backend に関係なく同じ。
- `Parallel.sum input` は `Parallel.reduce zero Add.add input` と同じ chunk order。
- 浮動小数点の NaN、符号付きゼロ、丸めはこの順序で通常演算を行った結果。
- fast-math、reassociation、暗黙 FMA は使わない。
- `Array.sum` / `Array.reduce` が C04 で左から右逐次を定義する場合、`Parallel.sum` は別 API として文書化する。

### closure と thread safety

- `Parallel.init` / `map` / `reduce` の関数値は fork 前に評価済み。
- 関数値を worker 間で共有して同じ環境を同時に消費してはならない。
- `%tz.closure` の apply adapter は環境を消費して `@tz.free` するので、同じ closure descriptor を複数 worker が直接 apply するのは禁止。
- 安全な既定実装は「chunk ごとに保持用 snapshot を作り、各 full callback application の直前にさらに clone する」。
- caller は `@tz.closure.clone` で chunk 数分の保持用 closure snapshot を作り、chunk context に置く。
- 各 chunk は保持用 snapshot を直接 `apply_value` に渡してはならない。
- `%tz.apply.*` は `closure_wrappers` で非空 `%env` を消費し、呼び出し前に `@tz.free(ptr %env)` するため、同じ snapshot を二つ目の要素で再利用すると use-after-free / double-free になる。
- 各 element / reducer application は、保持用 snapshot から `@tz.closure.clone` で一時 closure を作り、その一時 closure を `apply_value` へ渡す。
- `Parallel.reduce` の reducer は curried two-argument function なので、full application ごとに一時 clone を一つ作る。第一引数の apply がその env を消費して中間 closure を返し、第二引数の apply が中間 env を消費する。
- すでに `@tz.apply.*` に消費された一時 closure は drop しない。
- chunk 完了時に drop するのは、未消費の保持用 snapshot だけ。
- `clone` pointer がない closure は clone 不可なので、parallel API では拒否するか direct borrowed-call lowering を使う。
- immediate lambda / known function で `call_specialization::target` が取れる場合は、F02 専用の borrowed-call path を使って closure allocation を省いてよい。
- borrowed-call path でも、捕捉値は fork/join の間だけ caller が所有し、worker は読み取り専用に借りる。
- borrowed-call path は `@tz.apply.*` を経由しない、専用の非消費 worker adapter を生成できる場合だけ使う。
- borrowed-call path は捕捉値を move/drop しない worker に限る。
- 非消費 adapter を証明できない場合は、必ず「保持用 snapshot + application ごとの clone」経路を使う。
- 既存の `Specializations::can_borrow` と同じ条件を再利用し、結果が loans を運ぶ関数は除外する。
- 関数値が hidden loans を持つ場合は `E1013`。
- 関数値が unknown parameter で、ownership が loans なしを証明できない場合は `E1013`。
- これは task の `Send` ルールより狭い lifetime を使うが、borrowed input slice と同じく fork/join 内に閉じているため安全。

### borrowed input slice と Send の関係

- `Task` は cold で所有者より後に実行される可能性があるため、捕捉値と結果に参照を保持できない。
- `Parallel.map` / `map_ref` / `reduce` は同期的 fork/join で、呼び出しが戻る前に全 worker が完了する。
- そのため `&['a]` という input slice そのものは worker に貸してよい。
- input slice は共有借用であり、要素の変更・move はできない。
- `Parallel.map` で mapper に owned value を渡す場合は、共有 slice の要素を move せず、`clone_value` により独立した owned copy を作る。
- `Parallel.map_ref` で mapper に shared reference を渡す場合は、要素の owned copy を作らない。
- fork 中は所有権検査により input owner の move / `&mut` 借用を禁止する。
- worker が結果配列へ書く領域は各 index / chunk ごとに disjoint。
- 出力要素 `'b` は `Send` を要求し、結果配列に借用を格納しない。
- reducer partial も `Send` を要求する。
- 共有 mutable state は導入しない。

### trap と失敗

- worker 内で `assert`, 範囲外 access, 整数除算 trap, allocation failure が起きた場合はプロセスを異常終了する。
- trap 時の partial result drop、他 worker cancellation、捕捉値解放は保証しない。
- これは既存の `Task.parallel` trap 規則と同じ。
- 正常終了時は、保持用 closure snapshot、application ごとの一時 closure、partial array、identity clone、input clone、result array、input borrow の cleanup をすべて行う。
- `pthread_create` 失敗など runtime failure は F01 の診断に従う。

### 前提とする他チケットのインターフェース

- F01: `tsuzuri_task_parallel` が常駐 pool と caller participation を持ち、callback ABI は `void (*run)(void *, uint64_t)`。
- C03: `&[T]` は配列全体または `&xs[a..b]` の共有スライスを表し、pointer + length を LLVM lowering から取得できる。
- C04: `Array` API は逐次の `Array.init` / `Array.map` / `Array.reduce` / `Array.sum` を提供し、`Parallel.*` との semantics 差を docs で比較できる。
- E02: `std/Parallel.tz` を予約標準モジュールとして常時読み込み、利用者モジュールとの衝突を `E1011` にできる。

### 他チケットへの提供インターフェース

- F07 は `Parallel.init` / `Parallel.map` / `Parallel.map_ref` の純粋な element function を GPU kernel subset 候補として再利用できる。
- F07 は `Parallel.reduce` の deterministic chunk tree を CPU fallback と GPU fallback の参照順序として使える。
- F04/F05 は `Parallel.sum` の内部 kernel を SIMD / ISA dispatch 対象にしてよいが、chunk order を変えてはならない。

## 設計

### 型検査

- `src/check.rs` の `Builtin` に `ParallelInit`, `ParallelMap`, `ParallelSum`, `ParallelReduce` を追加する。
- `Builtin::ALL` の配列長を更新する。
- `Builtin::name` は `Parallel.init`, `Parallel.map`, `Parallel.map_ref`, `Parallel.sum`, `Parallel.reduce` を返す。
- `Builtin::signature` は複数引数と制約を表せないため、E02/C04 と同じ拡張方式を使う。
- 既定案: `Builtin::signature` を `Scheme` 相当ではなく、`Checker::builtin` 側で専用処理する。
- `Checker::value_expression` に `ExprKind::Field(Name("Parallel"), field)` の分岐を追加する。
- `Parallel` の未知 field は `E1002` で `"Parallel has no function '<name>'; use Parallel.init, Parallel.map, Parallel.map_ref, Parallel.sum, or Parallel.reduce"` とする。
- `Parallel.init` の type check は `length: i64`, `initializer: i64 -> 'a`, result `['a]`。
- `Parallel.map` の type check は `mapper: 'a -> 'b`, `input: &['a]`, result `['b]` で、`Copy 'a`, `Send 'a`, `Send 'b` を要求する。
- `Parallel.map_ref` の type check は `mapper: &'a -> 'b`, `input: &['a]`, result `['b]` で、`Send 'a`, `Send 'b` を要求する。
- `Parallel.reduce` の type check は `identity: 'a`, `reducer: 'a -> 'a -> 'a`, `input: &['a]`, result `'a`。
- `Parallel.sum` の type check は `input: &['a]`, result `'a`。
- `Send` / `Copy` / `Numeric` / `Add` constraints を `Checker::require` で追加する。
- 新しい `TypedExprKind` を追加するなら、`TypedExpr::children` と `children_mut` に必ず追加する。
- 推奨 variant は `ParallelInit { length, initializer }`, `ParallelMap { mapper, input, by_ref: bool }`, `ParallelReduce { identity, reducer, input, sum: bool }`。
- 既存の `_ => Vec::new()` fallback に頼らない。

### ownership

- `ownership.rs` は新 `TypedExprKind` を `eval_value` / `eval_composed` に追加する。
- `Parallel.init` は `length` を consume、`initializer` を consume して fork boundary に渡す。
- `Parallel.map` と `Parallel.map_ref` は `mapper` を consume、`input` を borrow として扱う。
- `Parallel.reduce` は `identity` と `reducer` を consume、`input` を borrow として扱う。
- closure/function argument の evaluated `Value` に loans が残る場合、`E1013`。
- `input` slice の loan は parallel expression の評価終了まで保持する。
- 出力配列に参照を格納する型は `Send` 制約で拒否する。
- reducer の partial values は worker local 所有値として扱う。
- worker context に置いた保持用 closure snapshot は group 完了後に drop する。
- application ごとに作った一時 closure は `@tz.apply.*` に消費されるため、追加で drop しない。

### lowering

- `src/llvm.rs` に `FunctionEmitter::parallel_init`, `parallel_map`, `parallel_reduce` を追加する。
- 共通 helper `parallel_chunk_count(length)` を LLVM IR として生成する。式は `n / 4096 + (n % 4096 != 0)` から `min(1024, raw)` を取る除算先行形に固定する。
- 共通 helper `parallel_chunk_bounds(chunk, length, chunk_count)` を LLVM IR として生成する。式は `base*c + (rem*c)/k` と `base*(c+1) + (rem*(c+1))/k` に固定する。
- callback ABI は F01 と同じ `void (ptr context, i64 chunk_index)`。
- `Parallel.init` context は `{ i64 length, ptr output, ptr closures }` または型ごとの構造体。
- `Parallel.map` / `map_ref` context は `{ ptr input_data, i64 input_length, ptr output, ptr closures }`。
- C03 slice が pointer + length descriptor を持つ場合、その shape を使う。
- 現行 `&[T]` が `%tz.array*` の共有参照の場合、C03 完了後に slice descriptor helper へ寄せる。
- `Parallel.reduce` context は `{ ptr input_data, i64 input_length, ptr partials, ptr closures, T identity }`。
- `partials` は chunk_count 要素の array。
- `Parallel.sum` は reducer closure を使わず、型ごとの add lowering を callback 内に直接生成してよい。
- ただし `Add.add` と `+` の性能経路を分けてはならない。既存 `FunctionEmitter::binary` と同じ lowering を使う。
- callback 名は deterministic にする。
- 例: `@tz.parallel.init.<element llvm type>`, `@tz.parallel.map.<in>.<out>`, `@tz.parallel.reduce.<type>.<function id or builtin key>`。
- 名前集合は `BTreeSet` / deterministic key で管理する。
- closure snapshot 配列は chunk_count 分だけ caller が作る。
- callback は `closures[chunk_index]` を保持用 snapshot として読む。
- callback は各 element / reducer application の直前に保持用 snapshot を `clone_value` し、clone した一時 closure だけを `apply_value` に渡す。
- callback は chunk 完了時に保持用 snapshot を drop する。`apply_value` に消費された一時 closure は drop しない。
- `Parallel.map` は input element を load した後、`clone_value(element, loaded)` で owned argument を作る。
- `Parallel.map_ref` は bounds 済みの element pointer を shared reference として mapper へ渡す。
- `Parallel.reduce` は各 chunk の初期 partial を `clone_value(identity_type, identity)` で作る。final combine の初期 result も別に `clone_value` で作る。
- `Copy` 値でも arrays/lists/functions は deep clone や closure clone が必要なため、identity descriptor や input descriptor を複数 partial / callback に reuse しない。
- sequential fallback でも同じ callback を chunk index 昇順に呼ぶ。
- result array は `FunctionEmitter::allocate_array` を使い、`@tz.alloc` / overflow guard を再利用する。
- 各 output element は一度だけ store する。
- map/init で worker が異なる index に同じ output pointer へ書くことは data race ではない。各 index は disjoint。
- reduce partials は chunk ごとに disjoint。
- final combine は caller thread で chunk index 昇順に行う。

### WASM

- `src/runtime/task-wasm.ll` の逐次 `@tsuzuri_task_parallel` をそのまま使う。
- WASM default は D-18 に従い import なし。
- WASM でも chunk loop は native と同じ。
- `tests/parallel.mjs` は `WebAssembly.Module.imports(module)` が空であることを確認する。
- F06 が有効な場合の threaded WASM は別チケットで、このチケットでは扱わない。

### benchmarks

- `benchmarks/run-parallel.mjs` を追加する。
- workload は sequential `Array.init` / `Array.map` / `Array.reduce` / `Array.sum` と `Parallel.*` を同じ入力で比較する。
- 仕事には allocation、保持用 closure snapshot、一時 closure clone、input / identity の `clone_value`、dispatch、synchronization、result collection、drop を含める。
- native は wall time を使う。CPU time は multi-thread の経過時間比較に使わない。
- WASM は sequential fallback として報告し、multi-core speedup として扱わない。
- `--quick` は小さい入力で正しさ・determinism・WASM import なしを検査する。
- 速度閾値は共有 CI に入れない。
- 生成 IR / assembly の artifact 保存オプションを用意する。

## 実装手順

1. **API 名の解決だけを追加する。**
   - `Builtin` または専用 resolver に `Parallel.init`, `Parallel.map`, `Parallel.map_ref`, `Parallel.sum`, `Parallel.reduce` を追加する。
   - まだ専用 lowering はせず、型検査で `E1002` / `E1003` が安定するようにする。
   - 確認: `tests/parallel.rs` で `Parallel.missing` が `E1002`、引数型不一致が `E1003`。

2. **`TypedExprKind` を追加する。**
   - `ParallelInit`, `ParallelMap`, `ParallelReduce` を追加する。
   - `children` / `children_mut`、`polymorph.rs` の traversal、`closures.rs::free_locals` 経路、`recursion.rs`, `call_specialization.rs` を更新する。
   - 確認: `llvm::emit` まで到達しない accept/reject テストで panic しない。

3. **所有権と Send 検査を追加する。**
   - input slice の shared loan を expression 終了まで保持する。
   - function value に loans があるケースを `E1013` で拒否する。
   - result / partial type の `Send` 制約を検査する。
   - `Parallel.map` は `Copy 'a` を要求し、非 Copy input は `Parallel.map_ref` へ誘導する。
   - 確認: closure が `&n` を捕捉する `Parallel.map` を `E1013` で拒否。

4. **sequential reference lowering を実装する。**
   - native/WASM ともまず `tsuzuri_task_parallel` を使わず、chunk order だけ実装して逐次実行する。
   - reduce の chunk order が仕様と一致することを固定する。
   - 確認: floating / subtraction reducer の期待値を独立 JS 参照で照合。

5. **`tsuzuri_task_parallel` chunk callback を導入する。**
   - chunk_count を除算先行の overflow-free 式で length から計算し、callback に chunk index を渡す。
   - chunk_start / chunk_end も `base/rem` 式で計算し、native/WASM で同じ IR 形にする。
   - F01 runtime と WASM fallback の両方で同じ結果にする。
   - 確認: native/WASM `-O0` / `-O3` で同じ checksum。

6. **closure snapshot と application ごとの clone を実装する。**
   - clone pointer がある closure は chunk_count 分の保持用 snapshot を clone する。
   - 各 element / reducer application の直前に保持用 snapshot をさらに clone し、その一時 closure を `apply_value` に渡す。
   - `@tz.apply.*` に消費された一時 closure は drop しない。chunk 完了時に drop するのは保持用 snapshot だけ。
   - clone pointer がない場合は proven non-consuming borrowed-call path か `E1013`。
   - cleanup を正常完了時に確認する。
   - 確認: native heap tracking で `live == 0`。

7. **known closure borrowed-call optimization を実装する。**
   - immediate lambda / known function は `call_specialization::target` と `Specializations::can_borrow` を使う。
   - `@tz.apply.*` を使わない非消費 worker adapter を生成できる場合だけ有効にする。
   - unknown function は通常 clone path。
   - 確認: IR に `@tz.specialized.` が出るケースと `@tz.closure.clone` が残るケースを分けてテスト。

8. **docs と benchmarks を追加する。**
   - `docs/language.md`, `docs/architecture.md`, `README.md`, `docs/benchmarks.md` を更新。
   - `benchmarks/run-parallel.mjs` と fixtures を追加。
   - 確認: `node benchmarks/run-parallel.mjs target/release/tsuzuri --quick`。

## テスト計画

- `tests/parallel.rs` を追加する。
- `tests/parallel.rs` は `Parallel.init 0` の型受理を確認する。
- `tests/parallel.rs` は `Parallel.init (-1)` が実行時 trap であり型検査では受理されることを fixture 側で確認する。
- `tests/parallel.rs` は `Parallel.map (x -> x + 1) (&values)` の IR 決定性を確認する。
- `tests/parallel.rs` は `Parallel.map` が非 Copy input を `E1005` または Copy constraint error で拒否し、`Parallel.map_ref` が `[string]` など非 Copy input を受理することを確認する。
- `tests/parallel.rs` は `Parallel.reduce 0 (x -> y -> x - y) (&values)` を受理し、決定的 chunk order の IR を確認する。
- `tests/parallel.rs` は borrowed closure capture を `E1013` で拒否する。
- `tests/parallel.rs` は `&mut` capture を `E1014` または `E1013` で拒否する。
- `tests/parallel.rs` は result type containing reference を `E1013` で拒否する。
- `tests/parallel.rs` は `Parallel.sum` の string input を `E1005` または constraint error で拒否する。
- `tests/parallel.rs` は intrinsic declaration が重複しないことを確認する。
- `tests/parallel.mjs` を追加する。
- fixture は `export def` で scalar checksum を返す。
- native `-O0` / `-O3` を両方ビルドする。
- WASM `-O0` / `-O3` を両方ビルドする。
- native と WASM は同じ JS BigInt / Number 参照実装で照合する。
- `Parallel.init` は長さ 0, 1, 4095, 4096, 4097, 8192, 4096*1024 前後を検査する。
- `Parallel.map` は input slice offset を含む C03 の `&xs[a..b]` を検査する。
- `Parallel.map` は Copy input を `clone_value` して mapper へ渡すことを、Copy だが drop を要する関数値 descriptor や、Copy 要素を含む配列の解放追跡で確認する。
- `Parallel.map_ref` は input element を clone せず shared reference として渡すことを確認する。
- `Parallel.reduce` は空 input、単一 chunk、複数 chunkを検査する。
- `Parallel.reduce` は非結合 reducer で thread count 非依存の期待値を検査する。
- `Parallel.sum` は整数 overflow wrapping を検査する。
- `Parallel.sum` は f64 の NaN、+0、-0 を検査する。
- worker trap は子プロセスで異常終了を確認する。
- native heap tracking で保持用 closure snapshot、一時 application closure、identity clone、input clone、result array の `live == 0` を確認する。
- WASM は `WebAssembly.Module.imports(module)` が空であることを確認する。
- テストは wall-clock speed を合否条件にしない。

## ドキュメント

- `docs/language.md` に `Parallel` 節を追加する。
- `docs/language.md` には chunking 式をそのまま書く。
- `docs/language.md` には `Parallel.reduce` が左から右逐次 reduce ではないことを書く。
- `docs/language.md` には borrowed input slice と `Send` の違いを書く。
- `docs/language.md` には `Parallel.map` が `Copy 'a` を要求し、非 Copy input は `Parallel.map_ref` を使うことを書く。
- `docs/language.md` には `%tz.closure` の消費 ABI により implementation が application ごとに closure clone することは書きすぎず、利用者向けには関数値が独立 snapshot として扱われることを書く。
- `docs/architecture.md` に lowering、closure clone、callback ABI、F01 runtime 依存を書く。
- `README.md` に短い使用例を書く。
- `docs/benchmarks.md` に `run-parallel.mjs` の条件と限界を書く。
- C04 docs には `Array.*` と `Parallel.*` の意味の違いを cross-link する。

## 受け入れ条件

- [ ] `Parallel.init`, `Parallel.map`, `Parallel.sum`, `Parallel.reduce` が修飾名で使える。
- [ ] `Parallel.map_ref` が修飾名で使える。
- [ ] `Parallel` は D-07 の予約標準モジュールとして扱われる。
- [ ] chunk 境界が入力長だけで決まる。
- [ ] native thread count を変えても reduce / sum の結果が変わらない。
- [ ] WASM sequential fallback と native の結果が一致する。
- [ ] closure environment を複数 worker が同時に消費しない。
- [ ] closure snapshot を二つ目の element に再利用して `@tz.apply.*` へ渡していない。
- [ ] borrowed closure capture を拒否する。
- [ ] `Parallel.map` は `Copy 'a` を要求し、borrowed slice の要素を `clone_value` してから owned mapper に渡す。
- [ ] `Parallel.map_ref` は非 Copy input を shared reference で処理できる。
- [ ] identity と input は Copy でも descriptor reuse せず、必要箇所ごとに `clone_value` する。
- [ ] input slice は fork/join 内だけ shared borrow される。
- [ ] result array allocation と cleanup が正しい。
- [ ] `Parallel.sum` は string を受け付けない。
- [ ] float reduction に fast-math / reassociation / FMA を使っていない。
- [ ] native/WASM `-O0` / `-O3` の E2E が通る。
- [ ] WASM default は import なし。
- [ ] benchmarks は matched workload で、速度閾値を CI に入れていない。

## 落とし穴

- `%tz.closure` apply adapter は環境を消費するため、同じ closure を複数 thread で直接呼ぶと use-after-free になる。
- chunk ごとの closure clone だけでは足りない。同じ chunk 内の二つ目の element でも `@tz.apply.*` が env を消費済みなので、full callback application ごとに clone が必要。
- curried reducer は第一引数適用で元 env を消費し、第二引数適用で中間 env を消費する。reducer 呼び出し全体ごとに fresh clone を作る。
- `Type::Function(..)` の `can_send` だけでは hidden borrowed environment を検出できない。
- immediate lambda の borrowed-call optimization は、捕捉値を drop しない worker に限定する。
- input slice が安全でも、result に reference を入れると owner より長生きしうる。
- parallel threshold で chunk_count を変えると、浮動小数点結果が thread count に依存する。
- `Parallel.sum` を `Array.sum` と同じ意味だと docs に書いてはいけない。
- `@tz.task.item.*` と同じ callback key にすると型の違う callback が衝突する。
- chunk bounds の `n * c` は i64 overflow しうる。
- `Copy` でない element を borrowed slice から reduce で move してはいけない。
- `Copy` でない element を `Parallel.map` の owned mapper に渡してはいけない。`Parallel.map_ref` を使う。
- Copy な配列・リスト・関数値は trivial copy ではない。descriptor を reuse せず `clone_value` を使う。
- `chunk_start = n * c / k` は overflow しうる。必ず `base/rem` 式を使う。
- worker 内 trap 時の cleanup を保証するように見せてはいけない。

## 対象外

- 自動並列化。
- `Array.map` を暗黙に parallel 化すること。
- work stealing policy のユーザー制御。
- thread count のユーザー指定。
- 非同期 / detach API。
- WASM threads。
- GPU offload。
- `Parallel.filter`, `scan`, `sort`。
- 非 Copy 要素の borrowed reduce。
- cancellation / recoverable error propagation。

## 未決事項

- `Parallel.reduce` の identity を chunk ごとに使う仕様でよいか。
  - 既定案: 上記仕様で固定し、逐次左 fold が必要なら C04 の `Array.reduce` を使う。
- `PARALLEL_TARGET_CHUNK_ITEMS = 4096` が初期値として妥当か。
  - 既定案: 意味に影響しない定数として固定し、変更時は docs と tests の期待値を更新する。
- unknown function values をどこまで受け入れるか。
  - 既定案: ownership が loans なしを証明できないものは拒否し、known function / immediate lambda を主経路にする。
- `Parallel.sum` に decimal を含めるか。
  - 既定案: `Numeric` 全体を対象にし、decimal は既存 `tz_soft_op` と同じ順序で処理する。
- 台帳の見直し提案: なし。D-07, D-14, D-17, D-18 と整合する。
