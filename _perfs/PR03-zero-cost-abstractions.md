# PR03: 抽象化コストの除去（計算式・Maybe／Result・関数値）

| 項目 | 内容 |
| --- | --- |
| ID | PR03 |
| 分類 | 実行速度 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | PM06, PM02, B05 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D3（Phase 3 の型付き IR での標準ビルダーの展開。PR03 の実装者は行わない）。Phase 1・2 は承認不要 |
| 手本にする既存実装 | 既知の継続の worker: `src/call_specialization.rs` の `Specializations`（`new`・`can_borrow`・`request`）と `target`・`transparent`、`src/llvm.rs` の `FunctionEmitter::specialized`・`prepare_known_call`・`prepare_borrowed_call`・`borrowed_call` と `known_closures`。IR の到達可能な呼び出しの走査: `tests/call_specialization.rs` の `body` と `standard_maybe_result_continuations_use_allocation_free_workers`。match の生成: `src/llvm_control.rs` の `match_expression`、`src/llvm.rs` の `union_tag`・`union_layout`。計測: `benchmarks/run-computations.mjs`（tracked 実行ファイルの確保数、`--baseline`・`--artifacts`）と PX01 の `--metrics` |
| 主な影響ファイル | `src/llvm.rs`（`FunctionEmitter` の呼び出し・match の emission）、`src/llvm_control.rs`（`match_expression`）、`src/call_specialization.rs`（展開の可否の判定、新規関数を置く）、`src/computation.rs`（読むだけ。展開の形を変えない）、`std/Maybe.tc`・`std/Result.tc`（読むだけ。本体を変えない）、`tests/call_specialization.rs`、`tests/computations.rs`、`tests/fixtures/computations/Optimization.tz`、`tests/computations.mjs`、`benchmarks/computations/`（変更なし。計測だけ）、`docs/language.md`、`docs/architecture.md`、`docs/benchmarks.md`、`_perfs/README.md` |
| 計測対象 | suite `computations` の対の種目 `bind`・`checked`・`delayed`・`array_for`・`array_bind`・`owned_capture`・`std_option`・`std_result`（`benchmarks/computations/native.cpp` の `workloads`）。`std_option_owned` は PX02 Phase 1a が条件をそろえるまで比較から外す。metric は PX01 の `wall_time`・`alloc_calls`・`alloc_bytes` と、-O3 IR・機械語の命令数（文書の表だけ） |

## 目的

関数型の書き方（コンピュテーション式、Maybe／Result、関数値、部分適用、パイプ）を使っても、手書きの分岐とループと同じ機械語になる
「ゼロコストの抽象化」に近づける。高水準の書き方を選んでも速度を失わないことは、Tsuzuri が C/C++・Rust を上回るための前提である。

対象の費用は次の 3 種類に限る。

- A1: 標準ビルダー（`Maybe`・`Result`）の `let!` 連鎖が作る union 値の構築と直後の分解。HEAD では確保も間接呼び出しもないが、
  -O3 後も union 値の `phi` とタグの `switch` が残る（`std_option`）。
- A2: 呼び先が静的に分かる関数値の間接呼び出し（`FunctionEmitter::apply_value` の code pointer 経由）。
- A3: LLVM が inline できない、または inline しない wrapper・worker の呼び出し。HEAD の生成関数は `define internal ... nounwind` で
  inline 属性を持たないので、属性・linkage による阻害は現状ない。Phase 1 で -O3 後に残る呼び出しの有無を種目ごとに確かめる。

実装者は Phase 1（原因の分類と現状の固定）を先に終え、Phase 2 は Phase 1 の分類で該当した段だけを行う（設計の「Phase 分割」）。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の一覧で `done` であること。PR03 は PX01 の `run-computations.mjs --metrics <dir>`・PX01 形式の
  JSONL・`alloc_calls`／`alloc_bytes` を使う。確認: `grep -n "PX01\|PR03" _perfs/README.md` と
  `grep -n "\-\-metrics" benchmarks/run-computations.mjs`（1 件以上）。
- PX02 は開始条件にしない。`std_option_owned` は PX02 の完了まで PR03 の比較表から外す（D6）。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR・機械語・計測）を保存していること。
- runner と E2E は Node 24 で動かす（`npx --yes --package=node@24 node ...`）。PATH の Node 20.19.6 は重い BigInt で異常終了することがある。

### 前提とする他チケットのインターフェース

- PX01: `node benchmarks/run-computations.mjs <compiler> --metrics <dir>` が `<dir>/computations.jsonl`（schema 1）を書き、
  metric `wall_time`（ms/呼び出し、9 標本以上の中央値・最小・最大）・`alloc_calls`・`alloc_bytes` を持つ。比較は PX01 の
  `### before／after（他チケットの共通手順）` の手順に従う。
- PM06: 関数値の捕捉環境のスタック化・確保の除去は PM06 が持つ。PR03 は確保の数を増やさないことだけを保証し、確保を減らす変更をしない。
- PM02: `%tz.closure` の表現は PM02 が持つ。PR03 は `apply_value` の呼び出しを減らすだけで、`%tz.closure` の型・欄を読まない新しい経路を作らない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況・コマンド・出力を添えて報告する（GUIDE §13）。

1. 変更で `tests/computations.mjs` の出力・native の未解放 0・`run-computations.mjs` の確保数の一致が崩れる。または既存テストの期待値
   （IR の文字列、診断、確保数）を変えたくなった。
2. 簡約・展開を成立させるために、評価順序、一回だけの評価、所有値の move・drop の位置、trap の種類と発生の有無のどれかを変える必要がある。
3. `specialization_budget_falls_back_instead_of_growing_unbounded`（`tests/call_specialization.rs`、ちょうど 1,024 件）か
   `honors_the_exact_specialization_limit`（`tests/polymorphism.rs`）が失敗する。または `MAX_SPECIALIZATIONS` を変えたくなった。
4. stack-depth の 3 テスト（`bounds_type_growing_polymorphic_recursion`、`bounds_recursive_and_flat_expression_depth`、
   `bounds_nested_builder_expansion_not_just_source_syntax`）のどれかが失敗する。上限や stack サイズは上げない。
5. 同じ入力から 2 回出した IR が一致しない。
6. -O3 の object の `__text` の大きさが `benchmarks/computations` で手順 1 の値より 5% を超えて増える（コードサイズの歯止め、D5）。
7. F1 の return slot で、`scopes`・`borrowed_locals`・frame（`frame_slots`・`frame_locals`）の扱いを変えないと未解放 0 を保てない、
   または二重解放が起きる。
8. Phase 1 の分類で、主因が K1〜K4（設計）のどれでもない。
9. `unsafe`、新しい crate、既定の WASM import、LLVM の pass pipeline の変更（clang の `-mllvm` 旗を含む）が必要になった。

## 現状と計測（HEAD `f8dc655`）

### 抽象化の経路（コードで確認）

- 展開: `src/computation.rs` の `expand` → `lower` → `Lowering::block`。`let!` の続きは `Lowering::continuation` が `ExprKind::Lambda` にし、
  `Lowering::call` が `Maybe.Bind` などの `ExprKind::QualifiedFunction` 呼び出しを作る。`Delay` を持つビルダーでは `Lowering::delay` が本体を
  `Lowering::thunk`（`unit` を受ける lambda）で包み、全体を `Run` に渡す。操作名は `OPERATIONS`。
- 標準ビルダー: `std/Maybe.tc` は `Bind maybe next = bind maybe next`、`Return value = Some value`、`Delay body = body`、
  `Run body = body ()`、`BindReturn value next = map next value`、`default_value`・`bind`・`map` を持つ。`std/Result.tc` も同じ形。
- 特殊化: `Specializations::new` が関数型引数の非 escaping を固定点で求め（`eligible`）、呼び出しの emission が既知の継続
  （`call_specialization::target` が返す `ClosureTarget`）を `Specialization { function, callbacks, borrowed }` として `request` する。
  `request` は `MAX_SPECIALIZATIONS`（1,024）件目以降で `None` を返し、呼び出しは `@tz.fn.*` と `%tz.closure` の通常経路へ戻る。
  worker は `FunctionEmitter::specialized` が `@tz.specialized.{id}` として作り、callback の引数を `known_closures` に入れる。
  let で束縛した局所も既知の target なら `known_closures` に入る。
- 間接呼び出し: `FunctionEmitter::apply_value` は `extractvalue %tz.closure {callee}, 0` の code pointer を
  `call <ty> {code}(<arg>, ptr {env}, i1 {borrowed})` で呼ぶ。
- 属性: 生成関数は `FunctionEmitter::emit` と `auxiliary` が `define internal ... nounwind` で出し、export だけが `define`。
  `alwaysinline`・`inlinehint`・`noinline` は `src/` にない。
- union: `Maybe<i64>` は `%"tz.union.Maybe.Maybe[i64]"`（`union_layout` の `UnionLayout::Common`）。`Some v` は
  `insertvalue ... { i32 1, i64 0 }, i64 v, 1`、`None` は `zeroinitializer`。match は `src/llvm_control.rs` の `match_expression` が
  タグの `switch` にし、到達しない既定の枝は `llvm.trap` を呼ぶ。

### 計測済みの事実

- 2026-09-26（`docs/benchmarks.md` の表）: 相対性能は `std_option` 0.873、`array_for` 0.968、`array_bind` 0.982、`checked` 0.981、
  `std_result` 0.982。`std_option_owned` は参照実装が確保しないため比較にならない（PX02 の `### std_option_owned の不一致`）。
- 2026-09-29 の起票時の記録: `ce_std_option` 34 命令、`direct_std_option` 24 命令、どちらも関数呼び出しなし。数え方の記録がないので、
  以後は下の再現コマンドの数え方を使う。
- 2026-09-29 の probe（HEAD `f8dc655`、Apple clang 21.0.0、arm64 macOS）で -O3 IR の export 関数の本体を比べた結果:

| 関数 | 本体の行 | `insertvalue` | `extractvalue` | `switch` | `select` | `phi` | `call` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `tz_ce_std_option` | 35 | 1 | 2 | 1 | 0 | 5 | 2 |
| `tz_direct_std_option` | 22 | 0 | 0 | 0 | 1 | 3 | 1 |
| `tz_ce_std_result` | 28 | 0 | 0 | 0 | 1 | 4 | 1 |
| `tz_direct_std_result` | 28 | 0 | 0 | 0 | 1 | 4 | 1 |

- 上の表の `call` は `llvm.trap`（`assert` の失敗と match の既定の枝）だけで、worker の呼び出しは残らない。`tz_ce_std_option` では、
  inline された `@tz.specialized.14`（`Run` 以下の連鎖）の二つの `ret` が union 値の `phi`（`insertvalue` と `zeroinitializer`）になり、
  `Maybe.default_value` の match がその `extractvalue` を `switch` し、既定の枝に `llvm.trap` が残る。`direct_std_option` は同じ条件を
  `select` 1 個にしている。これが A1 の具体形（K1）である。`std_result` は -O3 IR の大きさが既に同じ。
- 同じ probe の機械語の命令数（export 関数全体、ループを含む）: `ce_std_option` 263／`direct_std_option` 229、`ce_std_result` 205／176、
  `ce_checked` 893／864、`ce_bind` 943／918、`ce_delayed` 837／802。分岐・呼び出し命令（`bl`・`b` から `_` 名への）は各対で同数。
  `std_result` は IR が同じ大きさなのに機械語が 29 命令違う。Phase 1 で原因（ブロック配置など）を分類する。
- `let!` 2 段の連鎖の -O0 IR（下の再現）: `@tz.fn.Main.run` は `@tz.specialized.0` を呼び、worker の連鎖は
  `0 → 4 → 3 → 6 → 8 → 2 → 5 → 7` の 8 段で、`@tz.alloc` も間接呼び出しもない。ほかに `@tz.specialized.1`
  （`Maybe.map` の形、本体に間接呼び出し `call i64 %v8(i64 %v7, ptr %v9, i1 true)`）が定義されるが、`run` からは到達しない。
  到達しない worker も 1,024 件の予算を消費する。

### 再現（2026-09-29 に確認）

`let!` 2 段の連鎖（既存構文。`run` は `62` を出力して終了コード 0）:

```tsuzuri
def run :: i64 -> i64
fn run offset = Maybe.get (Maybe { let! x = Some 20; let! y = Some (x + offset); return x + y })
run 22
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
W=/tmp/tz-pr03 && mkdir -p $W/chain   # 上の 3 行を $W/chain/Main.tz に置く
target/release/tsuzuri run $W/chain
target/release/tsuzuri build $W/chain --emit llvm -o $W/chain.ll
awk '/^define .*@tz.specialized/,/^}/' $W/chain.ll | grep -E '^define|call '
target/release/tsuzuri build benchmarks/computations/Main.tz --emit llvm -o $W/comp.ll
/usr/bin/clang -O3 -fPIC -fno-fast-math -ffp-contract=off -Wno-override-module -S -emit-llvm $W/comp.ll -o $W/comp.opt.ll
/usr/bin/clang -O3 -fPIC -fno-fast-math -ffp-contract=off -Wno-override-module -S $W/comp.ll -o $W/comp.s
for f in ce_std_option direct_std_option ce_std_result direct_std_result; do
  printf '%s ' $f; awk "/^define .*@tz_$f\(/,/^}/" $W/comp.opt.ll | grep -cE '^  '
done
for f in ce_std_option direct_std_option ce_std_result direct_std_result ce_checked direct_checked; do
  printf '%s ' $f; awk "/^_tz_$f:/{on=1;next} on&&/\.cfi_endproc/{exit} on" $W/comp.s | grep -cE '^[[:space:]]+[a-z]'
done
```

期待（HEAD）: IR の行数は `35 22 28 28`、機械語は `263 229 205 176 893 864`。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する。

- G1: `std_option` の -O3 IR で、`tz_ce_std_option` の本体が `tz_direct_std_option` と同じ形になる（union の `insertvalue`・`extractvalue`・
  `switch` が 0 個、条件は `select` 1 個）。
- G2: 8 種目のどれでも確保の回数と量を増やさない。
- G3: 8 種目の ce と direct の時間の比を悪化させない。`std_option` は差を縮める。
- G4: Rust の同等の書き方（`Option` と `?`）と同等以上を長期目標とする。PR03 の受け入れ条件にはしない。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 -O3 IR の本体 | 行数と命令の種類の個数（固定値） | 8 対の `tz_ce_*`・`tz_direct_*` | `tz_ce_std_option` 35 → 22 行。他は増えない |
| M2 機械語の命令数 | 個（固定値、arm64 と x86-64 は別の行） | 同上 | `ce_std_option` 263 → 229 に近づく。他は増えない |
| M3 確保 | `alloc_calls` 回・`alloc_bytes` bytes（決定的な 1 標本） | 8 種目 | 変化なし |
| M4 時間 | `wall_time` ms/呼び出し（9 標本以上の中央値・最小・最大） | 8 種目の ce と direct（既定の大きさ） | `std_option` の ce/direct 比が縮む。他は広がりの範囲内 |
| M5 コードサイズ | `__text` bytes（固定値） | `benchmarks/computations/Main.tz` の -O3 object | 増えても 5% 以内（超えたら停止条件 6） |
| M6 worker 数 | `@tz.specialized.` の定義の個数（固定値） | 「再現」の `chain.ll` と `comp.ll` | 変化なし |

## 変えてはいけない意味

- 評価順序と一回だけの評価: 引数、`let!` の源、継続の本体を今と同じ順に一回ずつ評価する。`and!` の順序（B05）も同じ。
- 所有権: move・clone・drop の位置と回数を変えない。return slot（F1）へ格納した値は move であり、slot は drop の対象（`scopes`）に入れない。
- trap: `assert`、範囲、整数除算、確保の失敗、match の既定の枝の `llvm.trap` を IR から消さない。消してよいのは LLVM が到達しないと
  証明したものだけで、`unreachable` への置き換えや既定の枝の付け替えはしない。
- 数値: 算術の命令を変えない。overflow の wrap、NaN、符号付きゼロ、丸めは変わらない。fast-math・reassociation を使わない。
- 決定性: 同じ入力から同じ IR を出す。新しいラベル・値の名前は固定の文字列にする。
- WASM: import を増やさない。native と wasm32 で同じ IR の形を出す。
- 利用者ビルダー: `Delay`・`Run` や複数回の継続呼び出しを持つ利用者ビルダーの意味を仮定しない（`docs/language.md` の
  `### コンピュテーション式の性能` の「ビルダー名やモナド則を仮定せず」を守る）。
- 関数値の ABI・公開 C／WASM ABI・`%tz.closure` の欄を変えない。

## 設計

### 分類（Phase 1）

Phase 1 は 8 対の -O3 IR と機械語を次の 4 分類に当てはめる。一つの対が複数の分類に当たってよい。

| 分類 | 判定（`awk` で切り出した export 関数の本体） | 対策 | Phase |
| --- | --- | --- | --- |
| K1 union 値の `phi` | `phi %"tz.union.` がある、または union の `extractvalue` を `switch` している | F1 | 2a |
| K2 残る呼び出し | `call` の先が `@tz.specialized.`・`@tz.fn.`・`@tz.apply.` | F3 | 2b |
| K3 間接呼び出し | `call <型> %`（呼び先が値） | 記録だけ（D4） | なし |
| K4 機械語だけの差 | IR の本体が行数・命令の種類で同じなのに機械語の命令数が違う | 記録だけ | なし |

HEAD の probe では `std_option` が K1、`std_result` が K4 に当たる。`bind`・`checked`・`delayed`・`array_*`・`owned_capture` は Phase 1 で判定する。

### F1: union を返す関数の return slot（Phase 2a）

K1 の原因の仮説（手順 7 で確かめる）は、union を返す worker の複数の `ret` が inline 後に集約値の `phi` になり、LLVM の SROA が集約値の `phi` を欄ごとに分けないことにある
（`direct_std_option` の `let` の union は同じ条件でも `select` になる）。F1 では、union を返す関数の戻り値を entry block の alloca（return slot）に
格納し、一つの return block で load して返す。LLVM の SROA は集約値の store・load を欄ごとに分けるので、タグは定数の `phi` になり、呼び出し側の
`switch` が畳まれる見込みである。これは見込みであり、手順 7 で確かめる。消えなければ F1 を戻して止める（D1）。

- 対象: `FunctionEmitter::emit` が出す関数（利用者関数、`$lambda`、`@tz.specialized.*`）のうち、結果の型が union で `union_layout` が
  `UnionLayout::Enum` 以外のもの。`auxiliary`・`closure_wrappers`（`@tz.apply.*`・`@tz.env.*`）・`src/llvm_abi.rs` の export wrapper は対象外。
- 名前: slot は `%tz.return.slot`、block は `tz.return`、load した値は `%tz.return.value`（すべて固定。`%v<n>`・`b<n>` と衝突しない）。
- `self.function.body` の自己末尾呼び出しは今までどおり `br label %loop` で、slot を通らない。`src/` に `musttail` はない。

### データ構造

```rust
// src/llvm.rs
struct FunctionEmitter<'a> {
    // 既存の欄は変えない
    return_slot: Option<String>, // （新規）F1 の `%tz.return.slot`。対象外の関数では None
}

impl FunctionEmitter<'_> {
    fn uses_return_slot(&self) -> bool;                 // （新規）結果の型の union_layout が Enum 以外
    fn return_value(&mut self, ty: &Type, value: &str); // （新規）slot があれば store と br、なければ ret
}
```

`FunctionEmitter` の実際の lifetime 引数・欄の並びは HEAD の定義に合わせる。欄の初期値は `known_closures: BTreeMap::new()` と同じ場所で `None`。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 分類（Phase 1） | `tests/call_specialization.rs` | `standard_maybe_result_continuations_use_allocation_free_workers` の隣 | 新規テスト 3 件（テスト計画）。HEAD の性質を固定する |
| 分類（Phase 1） | `docs/benchmarks.md` | `## コンピュテーション式の比較` | 分類の表（K1〜K4）と M1・M2・M5・M6 の before の値 |
| F1 | `src/llvm.rs` | `FunctionEmitter`（欄）、`uses_return_slot`・`return_value`（新規） | 上のデータ構造 |
| F1 | `src/llvm.rs` | `FunctionEmitter::emit` | `tail` の前に `uses_return_slot()` なら `self.allocas` へ `%tz.return.slot = alloca <ty>, align <n>` を足す（align は既存の alloca と同じ求め方）。`tail` の後、slot が一度でも使われたら `tz.return:` block（load と `ret`）を末尾に足す |
| F1 | `src/llvm.rs` | `FunctionEmitter::tail` | `ret {} {value}` の `instruction` を `return_value` にする |
| F1 | `src/llvm_control.rs` | `match_expression` | 末尾位置の `ret {} {result}` を `return_value` にする |
| F1 | `src/llvm_bulk.rs` | `ret` を出す 2 か所（`ret {} {empty}`・`ret {} {tuple}`） | `return_value` にする（結果が union でなければ今と同じ IR） |
| F1 | `src/llvm.rs` | `emit` の cpu dispatch の `ret i64 {result}`、`auxiliary`、`closure_wrappers`、`src/llvm_abi.rs`・`src/llvm_task.rs`・`src/llvm_recursive.rs` の `ret` | 変更なし（結果が union でない、または対象外の関数） |
| F3 | `src/llvm.rs` | `FunctionEmitter::emit` の `define internal {} {}({parameters}) nounwind{debug}` | `self.symbol` が `@tz.specialized.` で始まるときだけ `nounwind inlinehint` にする（Phase 2b、K2 のときだけ） |
| 予算 | `src/call_specialization.rs` | `MAX_SPECIALIZATIONS`、`Specializations::request` | 変更なし（D5） |
| 展開 | `src/computation.rs`、`std/Maybe.tc`、`std/Result.tc` | `Lowering`、各操作 | 変更なし |

### 生成 IR とランタイム

ランタイム関数・`%tz.*` の型は増やさない。`let!` 2 段の連鎖（「再現」の `run`）の -O0 IR は HEAD で次の形（検証済み。worker の本体は抜粋）。

```llvm
define internal i64 @tz.fn.Main.run(i64 %arg0) nounwind {
  %v7 = call %"tz.union.Maybe.Maybe[i64]" @tz.specialized.0(%tz.closure %v6)
  %v8 = call i64 @tz.fn.Maybe.get.$mono.2(%"tz.union.Maybe.Maybe[i64]" %v7)
  ret i64 %v8
}
; 連鎖: @tz.specialized.0 → .4 → .3 → .6 → .8 → .2 → .5 → .7（Run → thunk → Bind → bind → 継続 → BindReturn → map → 継続）
```

HEAD の -O3（`tz_ce_std_option` の抜粋、検証済み）:

```llvm
tz.specialized.14.exit.i.i.i:
  %common.ret.op.i.i.i.i.i.i.i = phi %"tz.union.Maybe.Maybe[i64]" [ %v12.i.i.i.i.i.i.i, %b2.i.i.i.i.i.i.i ], [ zeroinitializer, %b1.i.i ]
  %arg1.fca.0.extract.i.i.i.i = extractvalue %"tz.union.Maybe.Maybe[i64]" %common.ret.op.i.i.i.i.i.i.i, 0
  switch i32 %arg1.fca.0.extract.i.i.i.i, label %b1.i.i.i.i [
    i32 0, label %tz.fn.Main.option_step.exit.i.i
    i32 1, label %b2.i.i.i.i
  ]
```

F1 の後の worker の形（計画。未検証）:

```llvm
define internal %"tz.union.Maybe.Maybe[i64]" @tz.specialized.2(%"tz.union.Maybe.Maybe[i64]" %arg0, %tz.closure %arg1) nounwind {
entry:
  %tz.return.slot = alloca %"tz.union.Maybe.Maybe[i64]", align 8
  br label %loop
loop:
  ; 既存の本体。各 ret の位置は次の 2 行になる
  store %"tz.union.Maybe.Maybe[i64]" %v4, ptr %tz.return.slot
  br label %tz.return
tz.return:
  %tz.return.value = load %"tz.union.Maybe.Maybe[i64]", ptr %tz.return.slot
  ret %"tz.union.Maybe.Maybe[i64]" %tz.return.value
}
```

F1 の後の -O3 の期待（計画。未検証）: `tz_ce_std_option` に `phi %"tz.union.`・`extractvalue`・`switch` がなく、`select` が 1 個。
-O0 では union を返す関数ごとに store・load が 1 組増える（-O0 は性能の対象外）。

### アルゴリズム

```text
emit():
  if uses_return_slot(): return_slot = Some("%tz.return.slot"); allocas.push(alloca of result type)
  tail(body)                        # ret の位置はすべて return_value を通る
  if return_slot が一度でも使われた: lines.push("tz.return:", load, ret)

return_value(ty, value):
  match return_slot:
    Some(slot) => instruction("store <ty> value, ptr slot"); instruction("br label %tz.return"); used = true
    None       => instruction("ret <ty> value")
```

`instruction` は今の `ret` と同じ debug location を付けるので、DWARF の行情報は store と br に移る。`tz.return` block の命令には
関数の最後の debug location を付ける（既存の `emit` が block を足すときと同じ扱い）。

### Phase 分割

| Phase | 内容 | 条件 | 出荷 |
| --- | --- | --- | --- |
| 1 | 分類、HEAD の性質を固定するテスト、before の記録（手順 1〜5） | なし | 単独で出荷できる |
| 2a | F1 | Phase 1 で K1 が一つ以上 | Phase 1 の上に単独で出荷できる |
| 2b | F3 | Phase 1 か 2a の後に K2 が一つ以上 | 同上 |
| 3 | F2: 型付き IR での標準ビルダーの展開と、構築子が既知の match の簡約（D3） | 人間の承認 | 設計方針だけ。PR03 の実装者は行わない |

実装者は Phase 1 を必ず行い、2a・2b は条件に当たるときだけ行う。Phase 3 には着手しない。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。端末は他の作業と共有されることがあるので、長い出力はファイルへ書いてから読む。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、before のコンパイラを `/tmp/tz-pr03/tsuzuri-before` に写す。「現状と計測」の再現を実行し、
  `chain.ll`・`comp.ll`・`comp.opt.ll`・`comp.s` を `/tmp/tz-pr03/before/` に保存する。「生成コードの確認」の M5・M6 も保存する。
- 確認: 再現の期待値（IR `35 22 28 28`、機械語 `263 229 205 176 893 864`。clang の版が違えば値を記録し直す）。次の 5 つがそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && mkdir -p /tmp/tz-pr03/before && cp target/release/tsuzuri /tmp/tz-pr03/tsuzuri-before
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
cargo test --locked --test call_specialization specialization_budget_falls_back_instead_of_growing_unbounded
```

### 手順 2: HEAD の性質を固定するテスト（Phase 1）

- 変更: `tests/call_specialization.rs`。
- 内容: テスト計画の `standard_let_chain_workers_are_direct_and_allocation_free`・`known_local_function_values_call_directly`（どちらも新規）を足す。
  到達可能な呼び出しの走査は `standard_maybe_result_continuations_use_allocation_free_workers` の while ループを写す。
  `known_local_function_values_call_directly` が HEAD で失敗したら、テストを消して K3（let 束縛の局所）として手順 3 の表に記録する（D4）。
- 確認: `cargo test --locked --test call_specialization` が `8 passed`（失敗して消した場合は `7 passed`）。

### 手順 3: 分類（Phase 1）

- 変更: なし（記録は `/tmp/tz-pr03/classes.md`）。
- 内容: 「生成コードの確認」の IR・機械語のコマンドを 8 対に実行し、設計の K1〜K4 の表を埋める。`chain.ll` の worker のうち `run` から到達しない
  ものの数も数える（`@tz.specialized.1` の例）。arm64 と x86-64 の機械語を別の行にする。
- 確認: 8 対すべてに分類（または「差なし」）が付く。K1〜K4 のどれでもない差が主因なら停止条件 8。

### 手順 4: before の計測（Phase 1）

- 変更: なし。
- 内容: 「計測手順」の before を実行する。
- 確認: `target/perf/PR03-before-*/computations.jsonl` があり、8 種目の `wall_time`・`alloc_calls`・`alloc_bytes` を持つ。runner が終了コード 0。

### 手順 5: Phase 1 の文書

- 変更: `docs/benchmarks.md`（「ドキュメント」の Phase 1 の行）。
- 内容: 分類の表、M1・M2・M5・M6 の before、計測日と環境を書く。Phase 2 の条件に当たらなければ、ここで手順 11 へ進む。
- 確認: `node scripts/check-docs.mjs docs/benchmarks.md` が終了コード 0。`cargo test --locked` が成功する。

### 手順 6: F1 の return slot（Phase 2a、K1 のときだけ）

- 変更: `src/llvm.rs` の `FunctionEmitter`（欄 `return_slot`）・`uses_return_slot`・`return_value`（新規）・`emit`・`tail`、
  `src/llvm_control.rs` の `match_expression`、`src/llvm_bulk.rs` の `ret` 2 か所、`tests/call_specialization.rs`。
- 内容: 設計の「段ごとの変更」の F1 の行どおり。`ret` を直接書く箇所が残っていないことを
  `grep -n 'format!("ret ' src/llvm.rs src/llvm_control.rs src/llvm_bulk.rs` で確かめ、残りが「変更なし」の行の箇所だけであることを見る。
  テスト `union_results_return_through_one_slot`（新規）を足す。
- 確認: `cargo test --locked --test call_specialization` が `9 passed`。`cargo test --locked --test computations` が成功する。

### 手順 7: F1 の効果の確認

- 変更: なし。
- 内容: `cargo build --release --locked` の後、「現状と計測」の再現と「生成コードの確認」の IR のコマンドを実行する。
- 確認: `tz_ce_std_option` の本体に `phi %"tz.union.`・`extractvalue`・`switch` がなく、`select` がある（G1）。満たさなければ手順 6 の変更を
  手で元に戻し（git の操作はしない）、IR の抜粋を添えて報告して止める（D1）。

### 手順 8: 意味の保持の確認

- 変更: `tests/computations.mjs`（テスト計画の E2E の 4 件を足す）。
- 内容: テスト計画の「E2E」の表の関数を既存の case の形で足す。期待値は表の値（手計算）を使う。
- 確認: 手順 1 の 5 テストが `1 passed`。`cargo test --locked` が成功する。次の 4 つが終了コード 0（native／WASM × `-O0`／`-O3`、未解放 0）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && cargo build --release --locked
for s in computations primitives control tasks; do npx --yes --package=node@24 node tests/$s.mjs target/release/tsuzuri || break; done
```

### 手順 9: F3 の inline 属性（Phase 2b、K2 のときだけ）

- 変更: `src/llvm.rs` の `FunctionEmitter::emit`、`tests/call_specialization.rs`。
- 内容: `@tz.specialized.*` の定義だけに `inlinehint` を付ける（D7）。テスト `specialized_workers_carry_only_inlinehint`（新規）を足す。
- 確認: `cargo test --locked --test call_specialization` が 1 件増えて成功する。K2 の呼び出しが「生成コードの確認」で消える。消えなければ
  F3 を戻して記録する（それ以上の手当てはしない）。

### 手順 10: after の計測と生成コード

- 変更: なし。
- 内容: 「計測手順」の after と「生成コードの確認」を実行する。
- 確認: M3・M6 が before と同じ。M5 が 5% 以内。M1・M2・M4 を before と並べた表ができる。

### 手順 11: 文書と状態

- 変更: 「ドキュメント」の表のファイル。
- 内容: Phase 2 の結果（行った段、行わなかった段と理由）を書く。
- 確認: `node scripts/check-docs.mjs docs/benchmarks.md docs/architecture.md` が終了コード 0。`git diff --check` が何も出さない。

## 計測手順

PX01 の `## 計測手順` と GUIDE §14 に従う。ここには PR03 に固有の値だけを書く。

### 環境

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
{ git rev-parse --short HEAD; sysctl -n machdep.cpu.brand_string; sw_vers -productVersion; /usr/bin/clang --version | head -1
  rustc --version; npx --yes --package=node@24 node --version; } > /tmp/tz-pr03/env.txt
```

計測中は他の重い処理を止め、電源に接続する。before と after は同じ機械・同じ clang で測る。

### before と after

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
R=target/perf/PR03-before-$(date -u +%Y%m%dT%H%M%SZ) && mkdir -p "$R"
npx --yes --package=node@24 node benchmarks/run-computations.mjs /tmp/tz-pr03/tsuzuri-before --metrics "$R" --artifacts "$R/artifacts" > "$R/computations.json"
# F1・F3 の後
R=target/perf/PR03-after-$(date -u +%Y%m%dT%H%M%SZ) && mkdir -p "$R"
npx --yes --package=node@24 node benchmarks/run-computations.mjs target/release/tsuzuri --baseline /tmp/tz-pr03/tsuzuri-before --metrics "$R" --artifacts "$R/artifacts" > "$R/computations.json"
```

- 標本: PX01 の既定（warm-up を除いて 9 標本以上）。after は `--baseline` で before のコンパイラの出力と同じ実行の中で交互に測る。
- 繰り返し: after を 3 回実行し、各回の中央値を並べる。3 回とも同じ向きの差だけを「改善」「悪化」と書く。
- 比較: PX01 の `### before／after（他チケットの共通手順）` の比較コマンドで before と after の JSONL を比べる。
- 大きさ: 既定（`native.cpp` の `workloads` の値）。`--quick` の時間は意味を持たないので記録しない。
- 保存: `target/perf/PR03-*/`（リポジトリに入れない。PX01 D2）。要約だけを `docs/benchmarks.md` に書く。

## 生成コードの確認

### IR（M1、native と wasm32）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && W=/tmp/tz-pr03 && mkdir -p $W/after
C='/usr/bin/clang -O3 -fPIC -fno-fast-math -ffp-contract=off -Wno-override-module'
target/release/tsuzuri build benchmarks/computations/Main.tz --emit llvm -o $W/after/comp.ll
$C -S -emit-llvm $W/after/comp.ll -o $W/after/comp.opt.ll
for p in bind checked delayed array_for array_bind owned_capture std_option std_result; do for k in ce direct; do
  awk "/^define .*@tz_${k}_$p\(/,/^}/" $W/after/comp.opt.ll > $W/after/$k-$p.ll
  printf '%s_%s lines=%s ' $k $p "$(grep -cE '^  ' $W/after/$k-$p.ll)"
  for q in 'phi %"tz.union.' insertvalue extractvalue 'switch ' 'select ' 'call '; do printf '[%s]=%s ' "$q" "$(grep -c "$q" $W/after/$k-$p.ll)"; done; echo
done; done > $W/after/ir.txt
grep -E 'call [^@]*%' $W/after/*-*.ll            # K3: 何も出ないこと
target/release/tsuzuri build benchmarks/computations/Main.tz --target wasm32 --emit llvm -o $W/after/comp32.ll
grep -c '%tz.return.slot = alloca' $W/after/comp.ll $W/after/comp32.ll   # F1 の後: 2 つの数が等しい
```

見る点: F1 の後の `ce_std_option` の行（G1）。`call ` は `llvm.trap` だけであること（`grep 'call ' ... | grep -v llvm.trap` が空）。

### 機械語（M2）とコードサイズ（M5）、worker 数（M6）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri && W=/tmp/tz-pr03
C='/usr/bin/clang -O3 -fPIC -fno-fast-math -ffp-contract=off -Wno-override-module'
$C -S $W/after/comp.ll -o $W/after/comp.s && $C --target=x86_64-apple-macos -S $W/after/comp.ll -o $W/after/comp-x86.s
for f in ce_std_option direct_std_option; do printf '%s ' $f
  awk "/^_tz_$f:/{on=1;next} on&&/\.cfi_endproc/{exit} on" $W/after/comp.s | grep -cE '^[[:space:]]+[a-z]'; done
$C -c $W/after/comp.ll -o $W/after/comp.o && /opt/homebrew/opt/llvm@21/bin/llvm-size -A $W/after/comp.o | grep __text
grep -c '^define internal .*@tz\.specialized\.' $W/after/comp.ll
```

8 対すべてについて同じ `awk` を実行し、x86-64 は `comp-x86.s` で数える。before は手順 1 で保存した `comp.ll` に同じコマンドを使う。

## テスト計画

### Rust テスト（`tests/call_specialization.rs`）

| テスト（すべて新規） | Phase | 入力 | 確かめること |
| --- | --- | --- | --- |
| `standard_let_chain_workers_are_direct_and_allocation_free` | 1 | 「再現」の `run`（`let!` 2 段） | native と wasm32 で `tz.fn.Main.run` が `@tz.specialized.` を呼ぶ。そこから到達する呼び出しに `tz.alloc`・`tz.closure.clone`・間接呼び出しがない。IR が 2 回とも一致 |
| `known_local_function_values_call_directly` | 1 | `let` で束縛した捕捉付き lambda を同じ関数で 2 回呼ぶ `i64 -> i64` の関数 | 関数の本体に `@tz.specialized.` か `@tz.fn.` への直接呼び出しがあり、`call i64 %` がない |
| `union_results_return_through_one_slot` | 2a | `if` で `Some`／`None` を返す関数、末尾 `match` で `Ok`／`Error` を返す関数、`i64` を返す関数 | union を返す 2 関数の本体に `%tz.return.slot = alloca` がちょうど 1 個、`ret %"tz.union.` がちょうど 1 個。`i64` の関数に `tz.return` がない。native と wasm32、IR の一致 |
| `specialized_workers_carry_only_inlinehint` | 2b | `standard_maybe_result_continuations_use_allocation_free_workers` と同じ入力 | `define internal` の行で `@tz.specialized.` を持つものはすべて `inlinehint` を含む。IR 全体に `alwaysinline` がない |

既存の `tests/call_specialization.rs` の 6 件、`tests/computations.rs` の `runtime_fixture_lowers_deterministically_for_both_targets`・
`bounds_nested_builder_expansion_not_just_source_syntax`・`deterministic_computation_mutations_do_not_panic` は変えずに通す。

### E2E（`tests/computations.mjs` に追加、Phase 2a）

既存の case の形を写し、native／WASM × `-O0`／`-O3` で結果と未解放 0 を見る。関数は既存構文で書き、足す前に `target/release/tsuzuri run` で
動くことを確かめる。期待値は手計算。

| case | 関数の内容 | 入力 → 期待 |
| --- | --- | --- |
| `slot_if` | `if flag then Some value else None` を返し、呼び出し側が `Maybe.default_value 0` で開く | `(true, 5)` → 5、`(false, 5)` → 0 |
| `slot_match_string` | `n` が 0 なら `Error "zero"`、それ以外は `Ok (n * 2)` を末尾 `match` で返し、呼び出し側が Error の文字列の長さか Ok の値を返す | 0 → 4、21 → 42（所有文字列の move と解放） |
| `slot_tail_loop` | `n * n > limit` になる最初の `n` を自己末尾再帰で探し `Some n` を返す | `(1, 50)` → 8 |
| `slot_nested` | 非末尾の再帰で `Maybe.map` を 1,000 段重ねる（深さ 0 は `Some 0`） | 1,000 → 1000（WASM `-O0` の stack） |

### 既存テストへの影響

なし。-O0 の IR は union を返す関数で store・load が増えるが、IR の文字列を固定した既存テストは union を返す関数の `ret` を見ていない見込みである。
一つでも期待値の変更が要るなら停止条件 1。

### 性能

「計測手順」の M3・M4 と「生成コードの確認」の M1・M2・M5・M6。合否の閾値は置かない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/benchmarks.md` | `## コンピュテーション式の比較` | Phase 1: 分類の表（K1〜K4）、M1・M2・M5・M6 の before、計測日・環境・コマンド。Phase 2: after と、行わなかった段の理由 |
| `docs/architecture.md` | `tests/call_specialization.rs` を説明する段落 | 新しい 3〜4 テストが確かめること。F1 を行った場合は union を返す関数の return slot を 1 文で |
| `docs/language.md` | `### コンピュテーション式の性能` | 変更なし（意味・保証を変えない）。F1 は内部の生成の形で、利用者に見える性質ではない |
| `_perfs/README.md` | 一覧の PR03 の行 | 状態を `done`、Phase 3 は未着手（要承認）と書く |

## 受け入れ条件

- [ ] 8 対の分類の表（K1〜K4）と before の M1〜M6 が `docs/benchmarks.md` にあり、コマンド・日付・clang の版が付いている。
- [ ] Phase 1 の Rust テストが通る。F1・F3 を行った場合はその Rust テストも通る。
- [ ] F1 を行った場合、`tz_ce_std_option` の -O3 IR が G1 を満たす。満たさない場合は F1 が入っておらず、その旨が報告・記録されている。
- [ ] M3（`alloc_calls`・`alloc_bytes`）と M6 が 8 種目・2 ファイルで before と同じ。M5 の増加が 5% 以内。
- [ ] M4 を 3 回測り、before・after・広がりが記録されている。速さの主張は 3 回とも同じ向きのものだけ。
- [ ] `tests/computations.mjs`・`tests/primitives.mjs`・`tests/control.mjs`・`tests/tasks.mjs` が native／WASM × `-O0`／`-O3` で通り、未解放 0。
- [ ] 同じ入力から 2 回出した IR が一致する（native と wasm32）。
- [ ] `cargo test --locked` が成功し、手順 1 の 5 テストが通る。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `ret` を直接書く箇所を一つでも残すと、その経路だけ slot を通らず、union の `phi` が残る。手順 6 の grep と
  `union_results_return_through_one_slot` の「`ret %"tz.union.` がちょうど 1 個」で見つける。
- slot を `scopes` に登録すると、返した所有値を関数の出口で解放して二重解放になる。slot は drop の対象にしない。E2E の `slot_match_string` と
  未解放 0 で見つける。
- WASM の `-O0` では alloca が shadow stack を使う。非末尾の再帰が深いと stack を使い切る。`slot_nested` と stack-depth の 3 テストで見る。
  失敗しても上限・stack サイズを上げない（停止条件 4）。
- 起票時の「34／24 命令」とこの版の数え方は違う。before と after は必ず同じコマンド・同じ clang で数える。
- `std_result` の機械語の差（K4）は IR が同じなので、PR03 では追わない。ブロック配置を変える属性や旗を足さない。
- `awk` の関数名の範囲指定では `$` を含む内部名がそのままでは合わない。export 名（`tz_ce_*`）だけを切り出す。
- 共有の端末では他の作業の出力が混ざることがある。計測や grep の結果はファイルへ書いてから読む。
- `run-computations.mjs` を Node 20 で動かすと V8 が異常終了することがある。Node 24 を使う。
- `inlinehint` は `alwaysinline` と違い -O0 に影響しない。-O0 の worker の連鎖（8 段）は PR03 の対象外であり、減らそうとしない。

## 対象外

- 利用者定義のビルダー（`benchmarks/computations/*.tc` を含む）の自動展開と、関数値の実行時の特殊化。
- 型付き IR での標準ビルダーの展開と構築子が既知の match の簡約（Phase 3、D3 の承認後）。
- 関数値の捕捉環境のスタック化・確保の除去（PM06）と `%tz.closure` の表現（PM02）。
- `std_option_owned` の条件の修正（PX02）。
- -O0 の性能、LLVM の pass pipeline の変更、K4（機械語だけの差）の解消。
- `MAX_SPECIALIZATIONS` と予算の数え方の変更（D5）。

## 決定事項

### D1: 簡約の段階と Phase 2a の対策

- 決定: Phase 2a は LLVM IR の形の変更（F1、union を返す関数の return slot）で K1 を解く。型付き IR での簡約は Phase 3 に回す。F1 で K1 が
  消えなければ F1 を入れずに止めて報告する。
- 理由: HEAD では -O3 後に worker の呼び出しも確保も残らず、残る差は inline 後の union 値の `phi` だけである。F1 は `FunctionEmitter` の
  `ret` の出し方だけで済み、評価順序・所有権・trap に触れない。型付き IR の展開は局所 id の付け替えと drop の位置の再現が要り、費用が大きい。
- 見直し提案: 起票時の既定案「簡約は型付き IR（LLVM 生成の前）で行い、LLVM の最適化に依存しない」を Phase 3 の方針として残し、Phase 2a を
  F1 にする。F1 は LLVM の SROA に依存するので、LLVM の版を上げたら手順 7 の確認をやり直す。
- 状態: 既定案（実装者はこの案に従う）

### D2: return slot の対象と名前

- 決定: `FunctionEmitter::emit` が出す関数で、結果の union の `union_layout` が `UnionLayout::Enum` 以外のものだけ。名前は
  `%tz.return.slot`・`tz.return`・`%tz.return.value` に固定する。slot は drop の対象にしない。
- 理由: `UnionLayout::Enum` は `i32` で集約値の `phi` が生じない。固定名は決定性を保ち、`%v<n>`・`b<n>` と衝突しない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 型付き IR での標準ビルダーの展開（Phase 3）

- 決定: 承認されるまで行わない。方針: 対象は `ModuleOrigin::Std` の `Maybe`・`Result` の `Bind`・`bind`・`BindReturn`・`map`・`Return`・
  `ReturnFrom`・`Delay`・`Run`・`Zero`・`default_value` に限る。関数型の引数がすべて `call_specialization::target` で既知かつ `can_borrow` の
  場合だけ、引数を左から順に一度ずつ評価して callee の本体を呼び出し元へ展開する。構築子が既知の `match` は選ばれた枝へ直接つなぐ。
  worker を作らないので予算を使わない。
- 理由: 生成器に関数本体の展開の仕組みがなく、局所 id の対応・drop の位置・debug 情報の扱いを新しく作る必要がある（非自明な新しい仕組み）。
- 状態: 要承認（承認前は Phase 3 に着手しない）

### D4: 呼び先が既知の関数値の呼び出し

- 決定: 新しい既知化はしない。HEAD の `call_specialization::target`（`Function`・`Closure`・部分適用、`known_closures` の局所）と
  `FunctionEmitter::bind` の不変な let 束縛の登録で直接呼び出しになる範囲を、`known_local_function_values_call_directly` で固定する。K3 が対象の
  種目に見つかったら記録だけにする。
- 理由: 残る間接呼び出しは逃げる関数値・可変の局所・record の欄など、流れの解析なしには呼び先を決められないものである。表現は PM02 が持つ。
- 状態: 既定案（実装者はこの案に従う）

### D5: 特殊化の予算とコードサイズ

- 決定: `MAX_SPECIALIZATIONS`（1,024）と数え方を変えない。到達しない worker（「現状と計測」の `@tz.specialized.1`）は数を記録するだけにする。
  コードサイズの歯止めは `benchmarks/computations` の -O3 object の `__text` の 5% で、超えたら停止する。
- 理由: `specialization_budget_falls_back_instead_of_growing_unbounded` はちょうど 1,024 件を確かめている。F1・F3 は worker の数を変えない。
  数え方の変更は予算の意味（生成量の上限）を変える。
- 状態: 既定案（実装者はこの案に従う）

### D6: `std_option_owned` の扱い

- 決定: PX02 Phase 1a が完了するまで PR03 の比較表・M4 から外す。M3 は他の種目と同じく記録する。
- 理由: 参照実装が確保しないため、時間の比が抽象化の費用を表さない（PX02 の `### std_option_owned の不一致`）。
- 状態: 既定案（実装者はこの案に従う）

### D7: inline 属性

- 決定: `alwaysinline` は使わない。`inlinehint` は K2 が見つかった場合だけ `@tz.specialized.*` の定義に付ける。std の関数・`$lambda`・
  `@tz.apply.*` には付けない。
- 理由: `alwaysinline` は -O0 でも展開してデバッグ時の段の対応とコードサイズを変える。HEAD では -O3 後に呼び出しが残らないので、
  属性は必要が確かめられたときだけ足す。
- 状態: 既定案（実装者はこの案に従う）
