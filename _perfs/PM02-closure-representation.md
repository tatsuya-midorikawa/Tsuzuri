# PM02: 関数値の表現の縮小

| 項目 | 内容 |
| --- | --- |
| ID | PM02 |
| 分類 | メモリ |
| 優先度 | P2 |
| 規模 | M |
| 依存 | PX01 |
| 関連 | PR03, PM07, B06 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（関数値の内部表現を `{ 静的記述子, 環境 }` に変える。起票時の「環境の先頭のヘッダー」案からの変更を含む） |
| 手本にする既存実装 | `src/llvm.rs` の `closure_wrappers`（`(関数, 捕捉数)` ごとに `@tz.apply.*`・`@tz.env.clone.*`・`@tz.env.drop.*` を出す）と `FunctionEmitter::closure_descriptor`（関数値の組み立て）。immediate capture の導入（docs/benchmarks.md の「単一の小さなスカラー捕捉を関数記述子の環境欄へ直接格納」の節）は、生成・スタック関数値・adapter・借用読み出し・clone／drop を一度にそろえた前例 |
| 主な影響ファイル | 変更: `src/llvm.rs`, `src/runtime/closure.ll`, `src/llvm_task.rs`, `src/llvm_io.rs`, `src/llvm_debug.rs`, `tests/call_specialization.rs`, `tests/tasks.rs`, `tests/fixtures/currying/Functions.tz`, `tests/primitives.mjs`, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md`。確認のみ: `src/llvm_parallel.rs`, `src/llvm_frame.rs`, `src/check.rs`, `src/closures.rs`, `src/call_specialization.rs`, `src/llvm_traps.rs`, `src/llvm_abi.rs`, `tests/tasks.mjs`, `tests/features.mjs`, `tests/computations.mjs` |
| 計測対象 | 値の大きさ（`%tz.closure`、関数値を含む union・レコード・配列の要素）。`control/closure_capture`・`control/closure_churn`（`benchmarks/control/Main.tz`、`node benchmarks/run-control.mjs`）と `cpp/task_sequential`・`cpp/task_parallel`（`benchmarks/cpp/Kernels.tz`、`node benchmarks/run-cpp.mjs`）の時間と確保回数・確保量 |

## 目的

関数値（クロージャー）と Task の値を 4 ポインター（native 32 bytes、wasm32 16 bytes）から 2 ポインター（native 16 bytes、wasm32 8 bytes）に縮め、関数値を格納する配列・レコード・union のメモリ量と、関数値の複製・受け渡しで動くバイト数を半分にする。
関数値の意味（複製・解放・呼び出し・借用）、捕捉のない関数値と immediate capture の関数値が確保しない性質、Task の実行 ABI は変えない。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の一覧で `done` である。確保回数・確保量は PX01 の計数で測る。
- D1 が承認されている。承認前に行ってよいのは手順 1（ベースラインと棚卸し）だけである。
- GUIDE §2.3 の手順で `cargo build --release --locked` と `cargo test --locked` が通る状態から始める。
- 行うのは Phase 1 だけである。Phase 2（設計の「Phase 分割」）は人間が依頼した場合だけ行う。

### 前提とする他チケットのインターフェース

- PX01: `node benchmarks/run-control.mjs` と `node benchmarks/run-cpp.mjs` の出力に、種目ごとの確保回数と確保量（E2E の確保追跡と同じく `@malloc` を追跡関数へ置換する方式）が出る。欄の名前は PX01 の実装に従う。種目ごとの値が出ない場合は、本チケットで別の計数器を作らずに停止して報告する。

### 停止条件

次の場合は手を止め、コマンド・出力・該当箇所を添えて報告する。症状への個別の手当てで先へ進めない。

1. 手順 1 の棚卸しが「段ごとの変更」の表と一致しない（`insertvalue %tz.closure` を `FunctionEmitter::closure_descriptor` と `src/runtime/closure.ll` 以外で出す箇所や、`%tz.closure` の欄 2・3 を読む箇所がほかにある）。
2. 関数値のレイアウトが公開 ABI・ホスト境界に出ている（`--emit header` の出力、`tz_*` の引数・戻り値、`src/llvm_abi.rs` の変換に `%tz.closure` が現れる）。
3. `@tz.apply.*` の引数列、Task の実行関数の引数（環境だけ）、`tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の引数を変える必要が生じた。
4. 「既存テストへの影響」に挙げたもの以外で、既存テストの期待値を変える必要が生じた。
5. どれかの suite で `live != 0`、二重解放、WASM の検証エラー、trap の位置の変化が出た。
6. `src/check.rs` の大きさの見積もりや `src/llvm_frame.rs` の `stack_size` を変えないと通らないテストが出た（D7）。
7. `control/closure_capture`・`control/closure_churn`・`cpp/task_sequential`・`cpp/task_parallel` のどれかで after の中央値が before より遅く、その差が before と after の広がり（最大 − 最小）の大きい方を超えた。Phase 2 の手段を承認なしに入れない。
8. `@tz.desc.*`（新規）が未定義という LLVM のエラーが出た（`closure_wrappers` を通らずに関数値を作る経路がある）。記述子を別の場所で出して回避しない。

## 現状と計測（HEAD `f8dc655`）

### 表現と大きさ

- `src/llvm.rs` のモジュール先頭の型定義（`; Tsuzuri - deterministic LLVM IR` で始まる文字列）が `%tz.closure = type { ptr, ptr, ptr, ptr }` を定義し、`llvm_type` が `Type::Function(..)` と `Type::Task(_)` をこの型にする。欄は 0: コード（`@tz.apply.{name}.{count}`）、1: 環境、2: clone 関数、3: drop 関数。
- 関数値と Task は native 32 bytes、wasm32 16 bytes。捕捉のない関数値も同じ大きさを持つ（起票時の確認）。
- 大きさの見積もりは 4 か所にある。`src/llvm.rs` の `storage_layout` が `(32, 8)`（General union の `[K x i128]` の K を決める）。`src/llvm_debug.rs` の `fields` が `["code", "environment", "clone", "drop"]`、`layout` が `(4 * pointer, pointer)`（DWARF）。`src/check.rs` の `validate_size` が使う見積もりが 32（`MAX_VALUE_BYTES` の判定）。`src/llvm_frame.rs` の `stack_size` が 32。

### 生成・呼び出し・複製・解放の経路

- `closure_wrappers` は捕捉数 `count` ごと（`capture_count` から `arity - 1` まで。`arity == capture_count` なら `capture_count` だけ）に、`@tz.env.clone.{name}.{count}`（`count != 0`、immediate でない、Task でない、全捕捉が `can_capture`）、`@tz.env.drop.{name}.{count}`（`count != 0`、immediate でない）、`@tz.apply.{name}.{count}` を出す。`@tz.apply.*` の引数は（引数, `ptr %env`, `i1 %borrow`）で、Task は `ptr %env` だけ。
- `FunctionEmitter::make_closure` が環境を作る。捕捉なしは `null`、`immediate_capture` が真ならビット列（`inttoptr`。`Binary` は先に `bitcast`）、それ以外は `@tz.alloc` で `environment_type` の領域を確保して捕捉値を格納する。`FunctionEmitter::stack_closure` は `alloca`（`align 16`）の環境を使う。どちらも最後に `closure_descriptor` が 4 欄を `insertvalue` する。捕捉なしは欄 2・3 が null、immediate は `@tz.closure.immediate.clone`・`@tz.closure.immediate.drop`、Task と `can_capture` でない捕捉を含む環境は欄 2 が null。
- `immediate_capture` は、Task でなく、捕捉が 1 個で、その型が `Integer`・`Binary(32)`・`Binary(64)`・`Bool` で、幅が native ではポインター幅以下、wasm32 では 32 bit 以下のときに真。
- コード（欄 0）を読む箇所は 5 つ: `FunctionEmitter::apply_value`、Task の実行（`src/llvm.rs` の `extractvalue %tz.closure {task}, 0`）、`FunctionEmitter::parallel_tasks` が出す `@tz.task.item.*` の本文、`src/llvm_task.rs` の `parallel_result_tasks` が出す `@tz.task.item.results.*`、`src/llvm_io.rs` の `extractvalue %tz.closure {work}, 0`。これらは同じ値の欄 1 も読む。環境だけを読む箇所は、`src/llvm_parallel.rs` の借用呼び出し（`extractvalue %tz.closure {callback}, 1` を `closure_capture_value` に渡す）と `src/llvm.rs` の `extractvalue %tz.closure {value}, 1`。
- 欄 2・3 を読むのは `src/runtime/closure.ll` だけ。`@tz.closure.clone` は環境が null ならそのまま返し、そうでなければ欄 2 を呼んで環境を差し替える。`@tz.closure.drop` は環境が null なら何もせず、そうでなければ欄 3 を呼ぶ。`drop_value`・`clone_value` はこの 2 関数を呼び、Task の `clone_value` は `unreachable!("single-use tasks cannot be cloned")`。`src/llvm.rs` は出力が `@tz.closure.` を含むときにこのランタイムを連結する。
- `parallel_launch` は各 chunk の callback を解放した後、その snapshot の欄に `store %tz.closure zeroinitializer` を書く。後で snapshot の配列を解放するとき、この空の関数値は環境が null なので drop は何もしない。

### 計測済みの事実

- 捕捉が 1 個の小さなスカラーなら、環境の欄にビット列を直接置き、確保しない（immediate capture）。
- 2026-09-26 の初回の改善で、関数記述子の 4 ポインター表現・公開 ABI・Task ABI は変えずに adapter を改善した経緯がある（docs/benchmarks.md）。
- `control/closure_capture`（immediate capture の関数値 1 個を既定で 1,000,000 回呼ぶ）、`control/closure_churn`（反復ごとに immediate capture の関数値を作り、複製して 2 回呼ぶ）、`cpp/task_*` の時間は docs/benchmarks.md の control 節・cpp 節にある。本チケットの before はこれを流用せず、計測手順で同じ機械・同じ条件で取り直す。
- 関数値を多数格納する処理（イベントハンドラーの配列、関数値を持つレコード）のメモリ比較はない。

### 再現（2026-09-29 に確認）

`/tmp/tz-work-PM02/closures/Probe.tz`:

```tsuzuri
def make_adder :: i64 -> i64 -> i64
fn make_adder n = x -> x + n

def make_prefix :: string -> string -> string
fn make_prefix prefix = suffix -> prefix + suffix

def add3 :: i64 -> i64 -> i64 -> i64
fn add3 a b c = a + b + c

export def probe :: i64
fn probe = {
    let f = make_adder 40;
    let g = make_prefix "ab";
    let h = add3 1 2;
    let k = f;
    let s = g "cd";
    k 1 + s.length + h 3
}
```

`/tmp/tz-work-PM02/closures/Main.tz` は `Probe.probe()` の 1 行。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri run /tmp/tz-work-PM02/closures   # 51 = 41 + 4 + 6
target/release/tsuzuri build /tmp/tz-work-PM02/closures --emit llvm -O0 -o /tmp/tz-work-PM02/closures.ll
grep -nE '%tz\.closure|@tz\.closure\.' /tmp/tz-work-PM02/closures.ll
```

抜粋:

```llvm
%tz.closure = type { ptr, ptr, ptr, ptr }
; make_adder 40: i64 の immediate capture
  %v1 = insertvalue %tz.closure zeroinitializer, ptr @tz.apply.Probe.make_adder.1, 0
  %v2 = insertvalue %tz.closure %v1, ptr %v0, 1
  %v3 = insertvalue %tz.closure %v2, ptr @tz.closure.immediate.clone, 2
  %v4 = insertvalue %tz.closure %v3, ptr @tz.closure.immediate.drop, 3
; add3 1 2: 2 個の i64 を捕捉するヒープ環境（make_prefix "ab" の string の捕捉も同じ形）
  %v17 = insertvalue %tz.closure zeroinitializer, ptr @tz.apply.Probe.add3.2, 0
  %v18 = insertvalue %tz.closure %v17, ptr %v14, 1
  %v19 = insertvalue %tz.closure %v18, ptr @tz.env.clone.Probe.add3.2, 2
  %v20 = insertvalue %tz.closure %v19, ptr @tz.env.drop.Probe.add3.2, 3
```

関数値の 4 種類（捕捉なし、immediate、ヒープ、部分適用）を配列に入れて複製・呼び出す `/tmp/tz-work-PM02/mix/`（源は「テスト計画」）は、`run` の結果が `-O0`・`-O3` とも 61 だった。`--target wasm32 --emit llvm -O0` では捕捉なしの関数値が `insertvalue %tz.closure zeroinitializer, ptr @tz.apply.Mix.inc.0, 0` で作られ、`define internal i64 @tz.apply.Mix.inc.0(i64 %argument, ptr %env, i1 %borrow)` が呼び出し規約を示す。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する。

- G1: `%tz.closure` を native 16 bytes、wasm32 8 bytes にする。
- G2: 捕捉のない関数値と immediate capture の関数値は、生成・複製・解放で確保しない（現状維持）。ヒープ環境の確保量を増やさない。
- G3: 関数値の呼び出し・複製が多い種目の時間を悪化させない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 関数値の大きさ | bytes（固定値） | IR の `%tz.closure`、DWARF の大きさ | native 32 → 16、wasm32 16 → 8 |
| M2 関数値を含む値の大きさ | bytes（固定値、native） | `Option<i64 -> i64>`、`Result<i64 -> i64, string>`、関数値の配列の要素 | 40 → 24、48 → 32、32 → 16 |
| M3 確保 | 回・bytes（1 回の実行の合計） | M4 の 4 種目（PX01 の計数） | 関数値・Task を配列に格納する種目は、その配列の確保量が要素あたり 16 bytes 減る。それ以外は変化なし |
| M4 時間 | ms（9 回の中央値、最小、最大） | `control/closure_capture`・`control/closure_churn`（既定の規模 1,000,000）、`cpp/task_sequential`・`cpp/task_parallel`（既定の規模 500,000） | 変化なし（差が広がりの範囲内） |

M2 の計算: `Option<i64 -> i64>` は payload の LLVM 型が 1 種類なので `{ i32, %tz.closure }`（4 + 詰め物 4 + 関数値）。`Result<i64 -> i64, string>` は payload の LLVM 型が異なるので `{ i32, [K x i128] }`（16 + 16K）で、K は `storage_layout` の大きさの最大値を 16 で割って切り上げた値（32 なら 2、16 なら 1）。

## 変えてはいけない意味

- 関数値の Copy の意味（複製は環境を独立に複製する）と解放の一回性。すべての suite で `live == 0`。
- 呼び出し規約: `@tz.apply.{name}.{count}` の引数は（引数, 環境, `i1` 借用）の順で、Task は環境だけ。借用の呼び出しと所有の呼び出しの分岐（worker の直接呼び出し、`@tz.env.clone.*` による複製、消費後の `@tz.free`）を変えない。
- Task の実行 ABI と B06 の取り消し: `tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` の引数と、`parallel_result_tasks` が未開始の Task を `drop_value` で解放する経路。
- 評価順序と trap: 捕捉値の評価・格納の順、呼び出しの順、trap の位置。`src/llvm_traps.rs` は `@tz.apply.` の名前で呼び出しを識別するので、この名前を変えない。
- 数値: immediate capture のビット列をそのまま運ぶ。`f64`・`f32` の NaN の payload と符号付きゼロ、整数の全ビット（`-1` を含む）を保つ。
- 決定的な IR: 記述子は `closure_wrappers` の出力順に出す。
- WASM: 既定の import を増やさない。新しい記号は内部の定数だけである。
- 公開 ABI: `tz_*` は関数値を受け取らない（起票時の確認。手順 1 と停止条件 2 で確かめる）。
- 言語の意味と上限: 型検査、診断、`MAX_VALUE_BYTES` の判定を変えない（D7）。

## 設計

### 表現

`%tz.closure.desc`（新規）と `@tz.desc.{name}.{count}` を足す。

```llvm
%tz.closure = type { ptr, ptr }            ; 0: 静的記述子、1: 環境
%tz.closure.desc = type { ptr, ptr, ptr }  ; 0: コード（@tz.apply.*）、1: clone、2: drop
```

- 関数値は `{ 記述子, 環境 }` の 2 ポインター。記述子は `(関数, 捕捉数)` ごとの `internal constant` で、実行時に作らない（D1）。
- 環境の表現（null、immediate のビット列、`environment_type` のヒープ・スタック領域）と、`@tz.apply.*`・`@tz.env.clone.*`・`@tz.env.drop.*` の本体は変えない。環境にヘッダーを足さない。
- 呼び出しは `load ptr, ptr <記述子>` でコードを得る（欄 0 は先頭なので GEP は要らない）。clone／drop は、環境が null でないときだけ記述子の欄 1・2 を読む。

| 関数値の種類 | 記述子（コード, clone, drop） | 環境の欄 | 確保 |
| --- | --- | --- | --- |
| 捕捉なし（関数を値として使う場合を含む） | `@tz.apply.N.0`, `null`, `null` | `null` | なし |
| immediate capture | `@tz.apply.N.1`, `@tz.closure.immediate.clone`, `@tz.closure.immediate.drop` | ビット列 | なし |
| ヒープ環境 | `@tz.apply.N.c`, `@tz.env.clone.N.c`, `@tz.env.drop.N.c` | `@tz.alloc` の領域 | `environment_type` の大きさ（変更なし） |
| `can_capture` でない捕捉を含むヒープ環境 | `@tz.apply.N.c`, `null`, `@tz.env.drop.N.c` | 同上 | 同上 |
| スタック関数値（`stack_closure`） | 捕捉数に対応する上のどれか | `alloca` の領域 | なし |
| 部分適用・カリー化の途中の値 | `@tz.apply.N.c` が `make_closure` で作る `@tz.desc.N.(c+1)` | 捕捉数に応じて上のどれか | 同上 |
| Task | `@tz.apply.T.c`, `null`, `@tz.env.drop.T.c`（捕捉なしは `null`） | `null` かヒープ | 変更なし |
| 空の関数値（`zeroinitializer`） | 記述子も `null` | `null` | なし。clone／drop は何もせず、呼び出されない |

wasm32 ではポインターが 4 bytes なので、関数値は 8 bytes、記述子は 12 bytes になる。`immediate_capture` は target ごとに判定するため（wasm32 の `i64` の捕捉はヒープ環境）、同じ関数でも target によって記述子の中身が異なる。

### データ構造

`closure_wrappers` のループで、`@tz.apply.{name}.{count}` を `output` に足した直後に記述子を出す。clone／drop の選び方は現在の `closure_descriptor` の分岐を移したもの。

```rust
let (clone, drop) = if count == 0 {
    ("null".to_owned(), "null".to_owned())
} else if immediate {
    (
        "@tz.closure.immediate.clone".to_owned(),
        "@tz.closure.immediate.drop".to_owned(),
    )
} else if !function.is_task
    && function.signature.parameters[..count]
        .iter()
        .all(|ty| ty.can_capture(&module.types()))
{
    (
        format!("@tz.env.clone.{name}.{count}"),
        format!("@tz.env.drop.{name}.{count}"),
    )
} else {
    ("null".to_owned(), format!("@tz.env.drop.{name}.{count}"))
};
output.push_str(&format!(
    "@tz.desc.{name}.{count} = internal constant %tz.closure.desc {{ ptr @tz.apply.{name}.{count}, ptr {clone}, ptr {drop} }}\n"
));
```

`FunctionEmitter` の組み立てと読み出し。`closure_descriptor` は名前を変えず、`closure_code`（新規）を足す。

```rust
fn closure_descriptor(&mut self, id: usize, count: usize, environment: &str) -> String {
    let name = self.module.functions[id].qualified_name();
    let value = self.value(format!(
        "insertvalue %tz.closure zeroinitializer, ptr @tz.desc.{name}.{count}, 0"
    ));
    self.value(format!("insertvalue %tz.closure {value}, ptr {environment}, 1"))
}

fn closure_code(&mut self, closure: &str) -> String {
    let descriptor = self.value(format!("extractvalue %tz.closure {closure}, 0"));
    self.value(format!("load ptr, ptr {descriptor}"))
}
```

`closure_code` は `apply_value`、Task の実行、`parallel_result_tasks`、`src/llvm_io.rs` の 4 か所で使う（どれも `src/llvm.rs` の子モジュールか同じファイルなので非公開のメソッドでよい）。`parallel_tasks` の `@tz.task.item.*` は IR の文字列なので、同じ 2 命令を文字列に書く。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 型定義 | `src/llvm.rs` | モジュール先頭の型定義の文字列 | `%tz.closure = type { ptr, ptr }` にし、直後に `%tz.closure.desc = type { ptr, ptr, ptr }` を足す |
| 記述子 | `src/llvm.rs` | `closure_wrappers` | 各 `count` の `@tz.apply.{name}.{count}` の直後に `@tz.desc.{name}.{count}` を出す |
| 組み立て | `src/llvm.rs` | `FunctionEmitter::closure_descriptor` | `insertvalue` を 2 欄にする |
| 組み立て | `src/llvm.rs` | `FunctionEmitter::make_closure`, `FunctionEmitter::stack_closure`, `FunctionEmitter::closure_capture_value`, `immediate_capture`, `environment_type` | 変更なし |
| 呼び出し | `src/llvm.rs` | `FunctionEmitter::closure_code`（新規）, `FunctionEmitter::apply_value` | コードを記述子から読む |
| Task の実行 | `src/llvm.rs` | `extractvalue %tz.closure {task}, 0` を出す箇所 | `closure_code` を使う |
| Task の並列実行 | `src/llvm.rs` | `FunctionEmitter::parallel_tasks`（`@tz.task.item.*` の本文） | `%desc = extractvalue %tz.closure %task, 0` と `%code = load ptr, ptr %desc` にする |
| Task の並列実行 | `src/llvm_task.rs` | `parallel_result_tasks`（`@tz.task.item.results.*`） | `closure_code` を使う。未開始の Task の `drop_value` は変更なし |
| I/O | `src/llvm_io.rs` | `extractvalue %tz.closure {work}, 0` を出す箇所 | `closure_code` を使う |
| 環境の読み出し | `src/llvm.rs`, `src/llvm_parallel.rs` | `extractvalue %tz.closure {value}, 1`・`{callback}, 1` の箇所 | 変更なし（環境は欄 1 のまま） |
| 並列の snapshot | `src/llvm_parallel.rs` | `parallel_launch`, `parallel_apply` | 変更なし |
| 複製・解放 | `src/llvm.rs` | `drop_value`, `clone_value` | 変更なし（ランタイムを呼ぶ） |
| ランタイム | `src/runtime/closure.ll` | `@tz.closure.clone`, `@tz.closure.drop` | clone／drop を記述子の欄 1・2 から読む |
| 大きさ | `src/llvm.rs` | `storage_layout` | `Type::Function(..) \| Type::Task(_) => (16, 8)` |
| DWARF | `src/llvm_debug.rs` | `fields`, `layout` | `["descriptor", "environment"]`、`(2 * pointer, pointer)` |
| 見積もり | `src/check.rs`, `src/llvm_frame.rs` | `validate_size` の見積もり, `stack_size` | 変更なし（D7） |
| trap | `src/llvm_traps.rs` | `@tz.apply.` による判定 | 変更なし |
| 前段 | `src/closures.rs`, `src/call_specialization.rs` | – | 変更なし（`%tz.closure` を参照しない） |

### 生成 IR とランタイム

`src/runtime/closure.ll` の 2 関数を次にする（immediate の 2 関数は変えない）。

```llvm
define internal %tz.closure @tz.closure.clone(%tz.closure %value) nounwind {
entry:
  %env = extractvalue %tz.closure %value, 1
  %empty = icmp eq ptr %env, null
  br i1 %empty, label %static, label %copy
static:
  ret %tz.closure %value
copy:
  %desc = extractvalue %tz.closure %value, 0
  %slot = getelementptr inbounds %tz.closure.desc, ptr %desc, i32 0, i32 1
  %clone = load ptr, ptr %slot
  %new = call ptr %clone(ptr %env)
  %result = insertvalue %tz.closure %value, ptr %new, 1
  ret %tz.closure %result
}

define internal void @tz.closure.drop(%tz.closure %value) nounwind {
entry:
  %env = extractvalue %tz.closure %value, 1
  %empty = icmp eq ptr %env, null
  br i1 %empty, label %exit, label %drop
drop:
  %desc = extractvalue %tz.closure %value, 0
  %slot = getelementptr inbounds %tz.closure.desc, ptr %desc, i32 0, i32 2
  %destroy = load ptr, ptr %slot
  call void %destroy(ptr %env)
  br label %exit
exit:
  ret void
}
```

`parallel_tasks` の `@tz.task.item.*` の本文の該当部分:

```llvm
  %task = load %tz.closure, ptr %slot
  %desc = extractvalue %tz.closure %task, 0
  %code = load ptr, ptr %desc
  %env = extractvalue %tz.closure %task, 1
```

生成と呼び出しの期待される形は「生成コードの確認」に書く。ランタイムの連結の条件（出力が `@tz.closure.` を含む）は変えない。

### Phase 分割

- Phase 1（本チケット。単独で出荷できる）: D1〜D8 のとおり表現を切り替える。
- Phase 2（人間が Phase 1 の計測結果を見て依頼した場合だけ）:
  - P2-a: 記述子の読み込みに `!invariant.load` を付け、未知の関数値をループで呼ぶ場合に読み込みを巻き上げられるようにする。メタデータの番号は `Globals::FIRST_METADATA` の規則に従う。
  - P2-b: immediate の記述子の clone／drop を両方 null にし、ランタイムは「環境が null でなく drop が null」を immediate とみなしてビットを複製する（間接呼び出しが 1 回減る）。`can_capture` でない環境と捕捉のある Task は drop が null でないので区別できる。
  - P2-c: `src/check.rs` の見積もりと `stack_size` を 16 にする。受理されるプログラムが変わるため別途承認を得る。

## 実装手順

### 手順 1: ベースラインと棚卸し

- 変更: なし。
- 内容: 計測手順の「環境と基準のコンパイラ」を行い、before の計測（計測手順と生成コードの確認）を取る。「再現」と「テスト計画」の源で `/tmp/tz-work-PM02/closures/` と `/tmp/tz-work-PM02/mix/` を作る。関数値の組み立て・読み出しの箇所を数える。
- 確認: 次の出力が期待どおりである。23 行の内訳（`src/llvm.rs` 13、`src/runtime/closure.ll` 5、`src/llvm_task.rs` 2、`src/llvm_io.rs` 2、`src/llvm_parallel.rs` 1）が一致しなければ停止条件 1。最後の grep の各行が欄の数・順に依存しないことを目で確かめる。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -rnE 'insertvalue %tz\.closure|extractvalue %tz\.closure [^,]+, [0-3]' src/ | wc -l   # 23
grep -rn 'tz\.desc\.' src/ tests/                          # 出力なし
grep -rn 'tz\.closure' src/llvm_abi.rs                     # 出力なし（停止条件 2）
grep -ln 'closure' src/runtime/generate.py src/runtime/generate_math.py   # 出力なし（closure.ll は手書き）
grep -rnE 'Type::(Function|Task)\(' src/llvm*.rs
```

### 手順 2: 記述子を出す（まだ使わない）

- 変更: `src/llvm.rs` のモジュール先頭の型定義（`%tz.closure.desc = type { ptr, ptr, ptr }` を `%tz.closure` の行の直後に足す。`%tz.closure` はまだ 4 欄）と `closure_wrappers`。
- 内容: 「データ構造」の Rust のとおり記述子を出す。関数値の組み立てはまだ変えない。
- 確認: `cargo build --release --locked` の後、次の 2 つの数が等しい。`closures` の IR も同じ形で `/tmp/tz-work-PM02/closures-step2.ll` に保存する。`cargo test --locked` が通る。

```sh
target/release/tsuzuri build /tmp/tz-work-PM02/mix --emit llvm -O0 -o /tmp/tz-work-PM02/mix-step2.ll
grep -c '^define internal .*@tz\.apply\.' /tmp/tz-work-PM02/mix-step2.ll
grep -c '^@tz\.desc\..* = internal constant %tz\.closure\.desc' /tmp/tz-work-PM02/mix-step2.ll
```

### 手順 3: コードの読み出しを 1 か所に集める（IR は変えない）

- 変更: `src/llvm.rs`（`closure_code`、`apply_value`、Task の実行）、`src/llvm_task.rs`（`parallel_result_tasks`）、`src/llvm_io.rs`。
- 内容: この手順の `closure_code` は `extractvalue %tz.closure {closure}, 0` の 1 命令だけを返す。4 か所をこれに置き換える。
- 確認: 手順 2 と同じコマンドで作り直した `mix` と `closures` の IR が、手順 2 で保存した IR と `diff` で一致する。`cargo test --locked` が通る。

### 手順 4: 表現を切り替える

- 変更: 型定義（`%tz.closure = type { ptr, ptr }`）、`closure_descriptor`、`closure_code`（`load` を足す）、`parallel_tasks` の本文、`src/runtime/closure.ll`。
- 内容: 「設計」のとおり。一部だけを変えると IR が不正になるので、これらを一度に変える。
- 確認:

```sh
cargo build --release --locked
target/release/tsuzuri run /tmp/tz-work-PM02/mix -O0        # 61
target/release/tsuzuri run /tmp/tz-work-PM02/mix -O3        # 61
target/release/tsuzuri run /tmp/tz-work-PM02/closures -O0   # 51
grep -rnE 'extractvalue %tz\.closure [^,]+, [23]' src/      # 出力なし
grep -rnE 'insertvalue %tz\.closure .*, [23]"' src/          # 出力なし
cargo test --locked --test call_specialization --test tasks --test frontend --test generic_records
npx --yes --package=node@24 node tests/primitives.mjs target/release/tsuzuri
```

### 手順 5: 大きさの見積もりと DWARF

- 変更: `src/llvm.rs` の `storage_layout`、`src/llvm_debug.rs` の `fields` と `layout`。
- 内容: D7 のとおり。先に `grep -rn 'x i128\]' tests/` で関数値を payload に持つ General union の期待がないかを見る（「既存テストへの影響」）。
- 確認:

```sh
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-work-PM02/mix -g --emit llvm -O0 -o /tmp/tz-work-PM02/mix-g.ll
grep -c 'name: "descriptor"' /tmp/tz-work-PM02/mix-g.ll            # 1 以上
grep -cE 'name: "(code|clone|drop)"' /tmp/tz-work-PM02/mix-g.ll    # 0
cargo test --locked
```

### 手順 6: Rust テストを足す

- 変更: `tests/call_specialization.rs`、`tests/tasks.rs`。
- 内容: 「テスト計画」の `closure_values_use_static_descriptors`（新規）、`closure_descriptors_pair_with_apply_functions`（新規）と、`tests/tasks.rs` への検査の追加。
- 確認: `cargo test --locked --test call_specialization closure_values_use_static_descriptors` と `cargo test --locked --test call_specialization closure_descriptors_pair_with_apply_functions` がそれぞれ `running 1 test` で通る。`cargo test --locked --test tasks` が通る。

### 手順 7: E2E を足す

- 変更: `tests/fixtures/currying/Functions.tz`、`tests/primitives.mjs`。
- 内容: 「テスト計画」の `curried_mixed_kinds`（新規）と 3 ケース。
- 確認: 次がすべて通る。基準のコンパイラでも通ることで、期待値が表現に依存しないことを確かめる。

```sh
rm -rf /tmp/tz-work-PM02/fixture && cp -R tests/fixtures/currying /tmp/tz-work-PM02/fixture
target/release/tsuzuri check /tmp/tz-work-PM02/fixture
npx --yes --package=node@24 node tests/primitives.mjs target/perf/PM02/baseline-tsuzuri
npx --yes --package=node@24 node tests/primitives.mjs target/release/tsuzuri
```

### 手順 8: 全体の確認

- 変更: なし。
- 内容: 全 suite を実行し、各 suite が回す target と最適化段を確かめる（`tests/primitives.mjs` は wasm32 のビルドを `-O${optimization}` で行う）。
- 確認: 次がすべて通る。最後の grep の結果（suite ごとの target と最適化段）を完了報告に書く。

```sh
cargo test --locked
npx --yes --package=node@24 node tests/primitives.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/tasks.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/computations.mjs target/release/tsuzuri
grep -nE 'wasm32|-O\$\{|"-O[0-3]"' tests/tasks.mjs tests/features.mjs tests/computations.mjs
```

### 手順 9: 生成コードの確認

- 変更: なし。
- 内容: 「生成コードの確認」を after について行い、before と比べる。
- 確認: 期待どおりの IR と機械語であることを、`target/perf/PM02/` の保存物とともに完了報告に書く。

### 手順 10: 計測と記録

- 変更: `docs/benchmarks.md`。
- 内容: 計測手順の after を取り、before と並べて記録する。
- 確認: M1〜M4 の表が docs/benchmarks.md にある。停止条件 7 に当たる場合は報告して判断を待つ。

### 手順 11: 文書

- 変更: 「ドキュメント」の各ファイル。
- 内容: 「ドキュメント」のとおり。
- 確認: `git diff --check` の出力がない。`grep -nE 'ptr, ptr, ptr, ptr' docs/architecture.md` の出力がない。

## 計測手順

GUIDE §14 の共通手順に従う。生データは `target/perf/PM02/`（コミットしない）に置く。

### 環境と基準のコンパイラ

変更を始める前、`cargo build --release --locked` の直後に行う。after でも同じ内容を `target/perf/PM02/after/env.txt` に書き、`git diff --stat` を添える。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p target/perf/PM02/before target/perf/PM02/after
cp target/release/tsuzuri target/perf/PM02/baseline-tsuzuri
{ git rev-parse HEAD; sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -n 1; rustc --version; node --version; } > target/perf/PM02/before/env.txt
```

### ウォームアップと正しさ

`node benchmarks/run-control.mjs <compiler> --quick` と `node benchmarks/run-cpp.mjs <compiler> --quick` を、基準のコンパイラと after のコンパイラで 1 回ずつ実行する。時間は捨てる（`--quick` は正しさの確認だけ）。

### 時間（M4）

before と after を交互に 9 回ずつ実行する。計測中はほかの重い処理を止め、電源に接続する。

```sh
for trial in 1 2 3 4 5 6 7 8 9; do
  node benchmarks/run-control.mjs target/perf/PM02/baseline-tsuzuri > target/perf/PM02/before/control-$trial.json || break
  node benchmarks/run-control.mjs target/release/tsuzuri > target/perf/PM02/after/control-$trial.json || break
  node benchmarks/run-cpp.mjs target/perf/PM02/baseline-tsuzuri > target/perf/PM02/before/cpp-$trial.json || break
  node benchmarks/run-cpp.mjs target/release/tsuzuri > target/perf/PM02/after/cpp-$trial.json || break
done
```

各出力から 4 種目の Tsuzuri の時間を取り出し、種目ごとに 9 個の中央値・最小・最大を before と after で表にする。

### 確保と大きさ（M3・M1・M2）

- M3: PX01 の計数で、4 種目の確保回数・確保量を before と after で 1 回ずつ記録する（確保は決定的）。
- M1: 手順 5 の DWARF と「生成コードの確認」の IR の型定義から記録する。
- M2: `Option` と `Result` に関数値を入れて返す関数を `/tmp/tz-work-PM02/sizes/` に作り、`--emit llvm` の union の型定義（after で `{ i32, %tz.closure }` と `{ i32, [1 x i128] }` になる）から計算する。

### 記録

docs/benchmarks.md に PM02 の節を足し、日付、計測機、before と after のコミット、コンパイラ・Clang・Node の版、M1〜M4 の表、生データの保存先、そろえられなかった条件を書く。

## 生成コードの確認

`--emit llvm` は Tsuzuri が出した LLVM の最適化前の IR を書く。`-O3` を指定しても `call %tz.closure @tz.closure.clone(...)` や `ptrtoint (ptr getelementptr (...))` が残ることを 2026-09-29 に確認した。最適化後の命令は object を逆アセンブルして見る。before は `target/perf/PM02/baseline-tsuzuri` で同じ物を作る。

### IR（native と wasm32）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-work-PM02/mix --emit llvm -O0 -o /tmp/tz-work-PM02/mix-after.ll
target/release/tsuzuri build /tmp/tz-work-PM02/mix --target wasm32 --emit llvm -O0 -o /tmp/tz-work-PM02/mix-after-wasm.ll
grep -nE '^%tz\.closure(\.desc)? = type|^@tz\.desc\.Mix\.(inc|add3)\.|%tz\.closure .*, 0$' /tmp/tz-work-PM02/mix-after.ll
```

期待（native。SSA の番号は異なってよい。`@tz.desc.Mix.add3.0`（`null, null`）とラムダの記述子も出る）:

```llvm
%tz.closure = type { ptr, ptr }
%tz.closure.desc = type { ptr, ptr, ptr }
@tz.desc.Mix.inc.0 = internal constant %tz.closure.desc { ptr @tz.apply.Mix.inc.0, ptr null, ptr null }
@tz.desc.Mix.add3.1 = internal constant %tz.closure.desc { ptr @tz.apply.Mix.add3.1, ptr @tz.closure.immediate.clone, ptr @tz.closure.immediate.drop }
@tz.desc.Mix.add3.2 = internal constant %tz.closure.desc { ptr @tz.apply.Mix.add3.2, ptr @tz.env.clone.Mix.add3.2, ptr @tz.env.drop.Mix.add3.2 }
  %v4 = insertvalue %tz.closure zeroinitializer, ptr @tz.desc.Mix.inc.0, 0
  %v5 = insertvalue %tz.closure %v4, ptr null, 1
  %v84 = extractvalue %tz.closure %v83, 0
  %v85 = load ptr, ptr %v84
  %v86 = extractvalue %tz.closure %v83, 1
  %v87 = call i64 %v85(i64 10, ptr %v86, i1 true)
```

- wasm32 では `@tz.desc.Mix.add3.1` の欄 1・2 が `@tz.env.clone.Mix.add3.1`・`@tz.env.drop.Mix.add3.1` になる（wasm32 の `i64` は immediate にならない）。ほかの 2 行は native と同じ。
- before の同じ呼び出しは、`extractvalue %tz.closure %vN, 0` の結果を直接 `call i64 %vN(i64 10, ptr %vM, i1 true)` する（2026-09-29 に確認）。

### 機械語（native `-O3`）

```sh
target/release/tsuzuri build /tmp/tz-work-PM02/mix --emit object -O3 -o /tmp/tz-work-PM02/mix-after.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn /tmp/tz-work-PM02/mix-after.o > /tmp/tz-work-PM02/mix-after.s
target/release/tsuzuri build benchmarks/control --emit object -O3 -o target/perf/PM02/after/control.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn target/perf/PM02/after/control.o > target/perf/PM02/after/control.s
```

`benchmarks/control` を直接 `build` できることは未確認である。失敗したら出力を完了報告に書き、`mix` の確認だけで進める。見るもの:

- `tz_closure_capture`（macOS では `_tz_closure_capture`）のループ: 関数値は 2 つの定数の記述子のどちらかなので、記述子の読み込みが畳み込まれ、before と同じ命令列になる見込み（`blr` の直前に記述子からの `ldr` が増えない）。増えていたら記録し、停止条件 7 の計測で判断する。
- `tz_closure_churn` のループ: before と同じく `@tz.closure.clone`・`@tz.closure.immediate.clone` の呼び出しが残らない。
- `tz_closure_mix` の配列のループ: 要素ごとに記述子を読む `ldr` が 1 つ増えて `blr` する。未知の関数値の呼び出しで想定どおりの費用である。

## テスト計画

### 共通の源

`/tmp/tz-work-PM02/mix/Mix.tz`（2026-09-29 に `run` で `-O0`・`-O3` とも 61 を確認）。`Main.tz` は `Mix.closure_mix 5` の 1 行。

```tsuzuri
def inc :: i64 -> i64
fn inc x = x + 1

def add3 :: i64 -> i64 -> i64 -> i64
fn add3 a b c = a + b + c

export def closure_mix :: i64 -> i64
fn closure_mix seed =
    let prefix = "abc"
    let plain: i64 -> i64 = inc
    let immediate: i64 -> i64 = \x -> x + seed
    let heap: i64 -> i64 = \x -> x + prefix.length
    let partial: i64 -> i64 = add3 seed 2
    let functions = [plain, immediate, heap, partial]
    let copy = functions
    let mut total: i64 = 0
    for f in copy do total = total + f 10
    total + functions[1] 0
```

### Rust テスト

- `tests/call_specialization.rs::closure_values_use_static_descriptors`: 一時ディレクトリ（テストごとに別のルート）に上の `Mix.tz` と `Main.tz` を置き、ファイル内の既存テストと同じ方法で native と wasm32 の `--emit llvm -O0` を得て、次を検査する。
  - `%tz.closure = type { ptr, ptr }` と `%tz.closure.desc = type { ptr, ptr, ptr }` を含み、`%tz.closure = type { ptr, ptr, ptr, ptr }` を含まない。
  - 「生成コードの確認」の記述子の 3 行を、target ごとの期待どおりに含む。
  - `insertvalue %tz.closure` を含む行は、すべて `, 0` か `, 1` で終わる。
- `tests/call_specialization.rs::closure_descriptors_pair_with_apply_functions`: 同じ IR で `@tz.apply.*` の定義と `@tz.desc.*` が 1 対 1 に対応する。同じ源を 2 回ビルドした IR が一致する。検査の形:

```rust
let applies: BTreeSet<&str> = ir
    .lines()
    .filter(|line| line.starts_with("define internal "))
    .filter_map(|line| line.split("@tz.apply.").nth(1))
    .map(|rest| &rest[..rest.find('(').unwrap()])
    .collect();
let descriptors: BTreeSet<&str> = ir
    .lines()
    .filter_map(|line| line.strip_prefix("@tz.desc."))
    .map(|rest| &rest[..rest.find(" = internal constant").unwrap()])
    .collect();
assert_eq!(applies, descriptors);
for name in &descriptors {
    let head = format!("@tz.desc.{name} = internal constant %tz.closure.desc {{ ptr @tz.apply.{name}, ");
    assert!(ir.contains(&head), "{name}");
}
```

- `tests/tasks.rs`: `@tz.env.clone.$task.` の不在を検査している既存テストに、`$task.` を含む記述子の行が 1 行以上あり、どれも clone の欄（2 番目の欄）が `ptr null` である検査を足す。

### E2E

- `tests/fixtures/currying/Functions.tz` に、上の `Mix.tz` の識別子を `inc` → `mixed_inc`（新規）、`add3` → `mixed_add3`（新規）、`closure_mix` → `curried_mixed_kinds` に変えたものを足す。fixture のほかの関数は波括弧の書き方だが、確認済みの字下げの書き方のまま足す（手順 7 の `check` で確かめる）。
- `tests/primitives.mjs` の native の確認（`tz_curried_churn` と同じ並び。`&& live == 0` を付ける）と WASM の確認（`api.tz_curried_churn` と同じ並び。引数と結果は BigInt）に、次の 3 ケースを足す。各項は順に `inc 10`、`10 + seed`、`10 + 3`、`seed + 2 + 10`、`0 + seed`。

| 引数 | 期待値 | 独立の計算 | 押さえる境界 |
| --- | --- | --- | --- |
| `5` | `61` | 11 + 15 + 13 + 17 + 5 | 4 種類の関数値の複製・呼び出し・解放 |
| `0` | `46` | 11 + 10 + 13 + 12 + 0 | immediate の捕捉値 0（環境が null） |
| `-1` | `43` | 11 + 9 + 13 + 11 + (-1) | 全ビットが 1 の immediate |

- wasm32 では `i64` の捕捉がヒープ環境になるので、同じ 3 ケースがヒープの経路を押さえる。
- 既存の意味の保護: `tests/primitives.mjs`（currying の `tz_curried_*`。`tz_curried_inline_float` の NaN・符号付きゼロ、`tz_curried_churn`、`tz_curried_dynamic` などを含む）、`tests/tasks.mjs`、`tests/features.mjs`、`tests/computations.mjs`。すべて `live == 0`。

### 既存テストへの影響

- なし（見込み）。関数値の IR を検査する既存の期待（`tests/call_specialization.rs` の `tz.closure.clone`・`tz.apply.`・`call ptr @tz.env.clone.`、`tests/frontend.rs` の `call %tz.closure @tz.fn.Main.select_function`、`tests/generic_records.rs` と `tests/features.mjs` の `{ %tz.closure ... }` のレコード型、`tests/tasks.rs` の `@tz.env.clone.$task.`）は型名と関数名だけに依存し、本チケットは名前を変えない。
- 関数値を payload に持つ General union の `[2 x i128]` を期待するテストがあれば、K が 1 になるのは正当な変更である（手順 5 の grep で確かめ、該当すれば完了報告に書く）。これ以外の期待の変更は停止条件 4。

### 性能

- 計測手順のとおり。共有 CI に時間の閾値を足さない。

## ドキュメント

- `docs/architecture.md`: `%tz.closure` と `src/runtime/closure.ll` を説明している箇所（`grep -nE 'tz\.closure|closure\.ll' docs/architecture.md` で探す）を 2 欄の表現に直し、`%tz.closure.desc`・`@tz.desc.*` と「表現」の表の対応（immediate capture、Task、空の関数値）を書く。
- `docs/benchmarks.md`: PM02 の節を足す（計測手順の「記録」）。過去の節の「関数記述子の4ポインター表現」は履歴なので書き換えない。
- `_perfs/README.md`: 状態（着手時 `doing`、完了時 `done` と `_completed/` への移動）と、「メモリと成果物サイズ」の「関数値は 4 ポインター（32 bytes）」の記述。
- `docs/language.md`・`_docs/`: 変更なし（言語の意味は変わらない）。

## 受け入れ条件

- [ ] D1 の承認を得てから手順 2 以降を行った。
- [ ] `%tz.closure` が `{ ptr, ptr }`（native 16 bytes、wasm32 8 bytes）になり、記述子が「表現」の表どおりに出る（手順 6 の Rust テストが通る）。
- [ ] `grep -rnE 'extractvalue %tz\.closure [^,]+, [23]' src/` と `grep -rnE 'insertvalue %tz\.closure .*, [23]"' src/` の出力がない。
- [ ] `curried_mixed_kinds` の 3 ケースが native と wasm32 で通り、`live == 0`。
- [ ] `cargo test --locked` と `tests/primitives.mjs`・`tests/tasks.mjs`・`tests/features.mjs`・`tests/computations.mjs` が通る。
- [ ] 生成コードの確認（IR と機械語の before と after）の結果を保存し、要点を docs/benchmarks.md に書いた。
- [ ] M1〜M4 の before と after を docs/benchmarks.md に記録した。停止条件 7 に当たった場合は報告して判断を得た。
- [ ] docs/architecture.md と `_perfs/README.md` を更新した。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 欄 0 の意味が変わる。コードとして呼ぶ `extractvalue %tz.closure X, 0` が 1 か所でも残ると、記述子（データ）へ分岐する。native では `EXC_BAD_ACCESS` などで落ち、WASM では `call_indirect` の trap になる。手順 1 の 23 行を全部見直す。`parallel_tasks` の本文は Rust の文字列の中にあり `closure_code` を使えないので、特に見落としやすい。
- clone／drop は、環境が null かを先に調べてから記述子を読む。`parallel_launch` の snapshot などの空の関数値は記述子も null で、順序を逆にすると null を読む。
- immediate の捕捉値 0 は環境が null になり、clone／drop は記述子を読まずに戻る。immediate の clone／drop は恒等・何もしないので、これで正しい。`curried_mixed_kinds 0` がこの境界を押さえる。
- 記述子の中身は target で異なる（wasm32 の `i64` の捕捉はヒープ環境）。Rust テストの期待を target ごとに書き、native の期待を wasm32 に流用しない。
- `--emit llvm` の出力は LLVM の最適化前である（`-O3` を指定しても）。性能の判断に使う命令は object の逆アセンブルで見る。
- 記述子を出すのは `closure_wrappers` だけである。`@tz.desc.*` の未定義は、その `(関数, 捕捉数)` の wrapper が出ていないことを意味する（停止条件 8）。
- 記述子の初期化子が `@tz.closure.immediate.*` を参照するため、関数値を複製・解放しないプログラムでも `closure.ll` が連結される場合がある。内部関数なので実行結果と IR の決定性には影響しない。`-O0` の成果物の大きさの変化は計測の記録に書く。
- スタック関数値の環境は `alloca` の領域で、記述子の drop（`@tz.env.drop.*` は `@tz.free` を呼ぶ）に渡すと壊れる。`stack_closure` を使う条件を変えない。
- DWARF の `fields`・`layout` は型と同時に変える。変え忘れても実行は壊れないが、デバッガーの表示がずれる（手順 5 の `-g` の確認で検出する）。
- `parallel_result_tasks` の失敗時の経路は、未開始の Task を `drop_value` で解放する（B06）。Task の記述子の drop を null にすると、捕捉のある Task の環境が漏れる（`live != 0` で検出する）。
- `src/runtime/closure.ll` は手書きである。`numeric.ll`・`math.ll` のような再生成の対象ではなく、それらを再生成しない。
- Node 20.19.6 は BigInt の多い suite で V8 の `RepresentationChangerError` で落ちることがある。`npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri` を使う。
- `cargo test --locked <pattern>` は一致 0 件でも成功する。`running N tests` の N が 0 でないことを確かめる。
- E2E とテストの一時プロジェクトは、源の再帰的な探索（E03）のため、テストごとに別の一時ルートに置く。

## 対象外

- 関数値の実行時の特殊化、関数値の比較・Hash。
- 環境の確保の除去（PM06）、関数値の複製の除去（PM07）、抽象化コストの除去（PR03）、allocator（PM05）。
- 環境の参照計数・共有（C10 Phase 2）。
- extern のコールバック（E12 Phase 2）の ABI。`%tz.closure` は内部表現で公開しない。E12 は本チケットの配置を境界の表現に使わない。
- Phase 2 の手段（人間の依頼があるまで）。

## 決定事項

### D1: 関数値の表現

- 決定: `%tz.closure = type { ptr, ptr }`（欄 0: 静的記述子へのポインター、欄 1: 環境）とし、`%tz.closure.desc = type { ptr, ptr, ptr }`（コード、clone、drop）を `(関数, 捕捉数)` ごとの `internal constant` にする。環境の表現（null・immediate のビット列・ヒープ・スタック）は変えず、環境にヘッダーを足さない。
- 理由: (1) 2 ポインターで immediate capture を保てる形はこれだけである。immediate の環境の欄はビット列で、native の `i64`・`f64` と wasm32 の `i32`・`f32` はポインターの全ビットを使うため、ヘッダーを持つ環境と区別するビットが残らない。コードポインターにタグを付ける案は WASM の関数ポインターが table の添字なので成り立たず、型から決める案は関数値の型が捕捉の形を含まないので成り立たない。immediate capture をやめる案は `closure_capture`・`closure_churn` の環境の確保を戻す。(2) 環境の配置、`environment_type` の添字、`@tz.apply.*`・`@tz.env.*` の本体が変わらず、変更は組み立て・5 か所の読み出し・ランタイムの 2 関数に閉じる。(3) ヒープ環境が 8 bytes 増えず、スタック関数値へのヘッダーの書き込みも要らない。代償は、未知の関数値の呼び出しごとに記述子の読み込みが 1 回増えること（C++ の仮想呼び出しや Rust の `dyn Fn` と同じ形）。記述子が定数と分かる場合は LLVM が読み込みを畳み込む見込みで、生成コードの確認と計測で確かめる。
- 起票時の案との関係: 起票時の既定案「静的な記述子へのポインター 1 個（clone／drop を直接 2 個置く案より小さい）」の中身は保ち、置き場所を環境の先頭から関数値の欄 0 へ移す。「ヘッダーを持たないことをコードポインター側の情報で区別する」という起票時の考えは、コードの欄を記述子にすることで実現する。
- 状態: 要承認（内部表現と既定の出力を変えるため。承認前は実装手順の手順 2 以降に着手しない）

### D2: 記述子の中身

- 決定: 欄は（コード、clone、drop）の 3 個だけで、この順に置く。呼び出しは GEP なしの `load ptr, ptr <記述子>` でコードを読む。環境の大きさ・整列は持たない。
- 理由: `@tz.env.clone.*`・`@tz.env.drop.*` は `(関数, 捕捉数)` ごとに特殊化されて環境の大きさを知っており、`@tz.free` も大きさを取らない。読み手のない欄は記述子を大きくするだけである。
- 状態: 既定案（実装者はこの案に従う）

### D3: 記述子の名前と出す場所

- 決定: `@tz.desc.{name}.{count} = internal constant %tz.closure.desc { ptr @tz.apply.{name}.{count}, ptr <clone>, ptr <drop> }` を、`closure_wrappers` が `@tz.apply.{name}.{count}` を出した直後に出す。clone／drop の選び方は現在の `closure_descriptor` の分岐を移す。`closure_descriptor` の名前は変えない。
- 理由: `@tz.apply.*` と 1 対 1 に対応し、出力順が `closure_wrappers` の順で決まる（決定的）。接頭辞を `@tz.closure.` にしないので、ランタイムの連結の判定を記述子の名前で変えない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 捕捉のない関数値と空の関数値

- 決定: 捕捉のない関数値も記述子を持ち（clone／drop は null）、環境は null。clone／drop は環境が null なら記述子を読まずに戻る。`zeroinitializer` の関数値（記述子も null）は呼び出さない（現在と同じ不変条件）。
- 理由: 呼び出しの経路を 1 通りにし、捕捉のない関数値の生成・複製・解放で確保しない性質を保つ。
- 状態: 既定案（実装者はこの案に従う）

### D5: immediate capture

- 決定: `immediate_capture` の条件、ビット列の作り方（`make_closure`）と読み方（`closure_capture_value`）は変えない。immediate の記述子の clone／drop は `@tz.closure.immediate.clone`・`@tz.closure.immediate.drop` にする。環境の欄のビットをタグに使わない。
- 理由: immediate かどうかは関数値を作る時点で `(関数, 捕捉数, target)` から静的に決まり、記述子がその結果を運ぶ。clone／drop は環境のビットを解釈しないので、`-1` や NaN の payload を含む全ビットのパターンで正しい。
- 状態: 既定案（実装者はこの案に従う）

### D6: Task

- 決定: Task も `%tz.closure` を使う。記述子の clone は null、drop は捕捉があれば `@tz.env.drop.{name}.{count}`、なければ null。Task の実行は記述子からコードを読んで `code(ptr env)` を呼ぶ。`tsuzuri_task_parallel`・`tsuzuri_task_parallel_results` と B06 の解放経路は変えない。
- 理由: Task は複製されない（`clone_value` が `unreachable!`）。実行関数の引数が環境だけという ABI を保てば、ランタイムの変更が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 大きさの見積もり

- 決定: `storage_layout` を `(16, 8)`、`src/llvm_debug.rs` の `layout` を `(2 * pointer, pointer)`、`fields` を `["descriptor", "environment"]` にする。`src/check.rs` の見積もり（32）と `src/llvm_frame.rs` の `stack_size`（32）は変えない。
- 理由: `storage_layout` は General union の payload の大きさを決め、実際より大きいとメモリを無駄にする（16 は native の実際の大きさで、wasm32 の 8 より大きいので安全）。DWARF は実際の配置と一致させる。`check.rs` の見積もりは `MAX_VALUE_BYTES` による受理・拒否を決める言語上の上限で、下げると受理されるプログラムが変わる。`stack_size` も大きい側に外れて安全な見積もりである。
- 状態: 既定案（実装者はこの案に従う）。`check.rs` と `stack_size` の変更は Phase 2（P2-c）で別途承認を得る

### D8: 記述子の読み込みの最適化

- 決定: Phase 1 では記述子を普通の `load` で読み、`!invariant.load` などのメタデータや、immediate の clone／drop を null にする近道を入れない。
- 理由: 既知の関数値では定数の畳み込みで足りる見込みで、未知の関数値の費用は計測してから判断する。メタデータは `Globals::FIRST_METADATA` の番号の規則に関わり、Phase 1 の差分を大きくする。
- 状態: 既定案（実装者はこの案に従う）。Phase 2 は人間の依頼で行う
