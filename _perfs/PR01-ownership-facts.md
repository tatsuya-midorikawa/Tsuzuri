# PR01: 所有権・借用から導く最適化情報の付与

| 項目 | 内容 |
| --- | --- |
| ID | PR01 |
| 分類 | 実行速度 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | PR02, F12, A09, A13。規則の拡張点を使う: F10（内部可変性）, C08（`ref mut [T]` の表現）。PM07 とは独立 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1 の範囲。Phase 2・3 は人間が依頼した場合だけ着手する） |
| 手本にする既存実装 | 関数定義の出力: `src/llvm.rs` の `FunctionEmitter::emit`（`define internal ... nounwind{debug}`）と、既存の関数属性 `noreturn nounwind`（`@tz.builtin.unreachable.*`）・`cold nounwind`（`src/llvm_traps.rs` の `@tz.trap.report`）。型の大きさ: `src/llvm.rs` の `storage_layout`。型性質の述語: `src/check.rs` の `Type::can_send`・`Type::carries_loans` と `src/recursive.rs` の `TypeContext::stored_all`。IR を調べる Rust テスト: `tests/slices.rs`（`@tz.fn.Main.replace(ptr` の検査）、`tests/call_specialization.rs`（`define internal` で関数を切り出す） |
| 主な影響ファイル | 変更: `src/llvm.rs`（`FunctionEmitter::emit`、`storage_layout`、`layout_bound`（新規）、`parameter_facts`（新規））, `src/check.rs`（`Type::is_frozen`（新規））, `tests/param_facts.rs`（新規）, `tests/fixtures/param_facts/Main.tz`（新規）, `tests/features.mjs`（suite `param_facts`（新規））, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md`。確認のみ（変更しない）: `src/llvm_abi.rs`（`wrapper`）, `src/llvm_frame.rs`, `src/llvm_bulk.rs`, `src/call_specialization.rs`, `src/ownership.rs`, `src/closures.rs`, `tests/slices.rs`, `tests/host_abi.rs`, `tests/call_specialization.rs`, `tests/primitives.mjs` |
| 計測対象 | 時間: `control` の 15 種目（`ref` 引数を持つのは `record_pipeline` の `advance :: ref State<i64u> -> State<i64u>`）、`cpp` の全種目（`ref string` 引数を持つ `text_sum` を `utf16_scan`・`utf16_validate` が呼ぶ）。生成コード: PR01 kernels（「生成コードの確認」）。`dispatch`・`simd` の `ref [i64]` 引数は値渡しの記述子なので対象外（対照） |

## 目的

### 現行契約との照合と承認済み実装方針（2026-10-14）

利用者が Phase 1・2・3 全部の実装と保守的な設計判断を承認した。以下は旧 HEAD
`f8dc655` 向けの本文に優先する。チケットの移動・共有の一覧の編集は調整担当が行う。

- F10 は実装済み。`Atomic`／`Mutex` だけでなく、参照カウント、ホストハンドル、
  型に環境を表さない関数・Task・dyn を介する到達先もある。`readonly`／共有
  `noalias` は、閉じた格納グラフで不変性を証明できる場合だけにする。
- C08 の `ref [T]` と `ref mut [T..]` は値渡しの記述子であり、
  `slice_element()` で両方を除外する。`ref mut [T]` は記述子の slot を指す。
  A13 の格納参照を含む pointee に独立性を推測しない。借用を返す関数、借用を
  格納できる排他参照引数、未知の捕捉環境がある場合は `nocapture` を省く。
- レイアウトの割当上界を変えず、引数の保証には wasm32 を含む下界を使う。
  128-bit 値・SIMD の下界 alignment は 8 以下、ポインターは 4。
  具象 field、Drop 生存フラグ、再帰 union、A16 の長さ 0 を反映する。
- Phase 2 の scoped alias は、スカラー参照／スカラー要素の借用 slice の
  SSA provenance が確定した load/store に限る。共有同士の独立性は主張しない。
  排他 loan と他の loan の独立性だけを記述し、関数呼出しごとの scope 宣言を置く。
  slot の書換え、未知の pointer 導出、callback、自己末尾ループ、解析予算超過は
  根拠なしの metadata を出さずにフォールバックする。
- 空・move 済み・frame の所有 buffer は null にできる。所有型だけから
  `nonnull` を出さない。成功時に null を返さない `tz.alloc` と、その証明済み
  pointer の private reload だけを扱う。
- 値 `noundef` は初期化済みの scalar 引数に限り、aggregate／SIMD は除外する。
  戻り値は poison を生む演算・未知の呼出しを含まない限定した IR のみ。
  `memory(...)` は生成 IR の private stack／直接の pointer 引数へのアクセスと
  memory-free intrinsic だけを分類する。alloc/free/IO/Task/extern/Drop/未知の
  アクセスや呼出しには属性を出さない。分岐／反復／trap も totality の証明なしには
  要約しない。`llvm.trap` は memory-free intrinsic ではない。記述子引数の buffer を安易に `argmem` としない。
- Phase 3 の TBAA は、具象 scalar 要素の provenance とアクセス型が一致した
  load/store のみ。符号違いと同じ LLVM 表現は同じ tag、byte は無印（すべてと alias）。
  union の重なった payload、集約値／field 全体、runtime byte copy、atomic、
  外部 ABI の pointer に型名だけで tag を付けない。
- 属性・metadata は通常の内部関数のみ。public wrapper、adapter、Task と
  捕捉付き worker は変えない。生成 IR は設定ごとに決定的であり、現在すでにある
  native/WASM 固有 lowering の差を消すための変更は行わない。
- baseline は `target/perf/phase2-baseline/tsuzuri-before`。検証出力は repository
  配下に置く。測定は調整担当と排他に行い、未測定の速度向上を主張しない。

C/C++ のコンパイラは、ポインターの別名関係が分からないため多くの最適化をあきらめる（`restrict` を書かない限り）。
Tsuzuri は所有権・借用・不変性により、共有参照の参照先は借用中に変更されず、排他参照の参照先は借用中に他の経路から触れられないことを
型検査と所有権検査で保証している。この保証を LLVM の引数属性として伝え、ループ内の読み書きのレジスター化、ループ外への移動、
冗長な読み込みの除去を可能にする。誤った属性は誤った計算結果になるため、付与は証明できる場合だけに限り、規則ごとに付与と非付与のテストを置く。

実装者は Phase 1（内部関数の `ptr` 引数の属性）だけを実装する。Phase 2・3 は「設計」の方針だけを示し、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の一覧で done であること（計測を PX01 形式で記録するため）。確認: `grep -n "PX01\|PR01" _perfs/README.md`。
  着手時に PR01 の状態を `doing` にする。
- GUIDE §2.3 の基準コマンド（`cargo build --release --locked`、`cargo test --locked`）が成功する状態から始める。
- 手順 1 で変更前のコンパイラを `/tmp/tz-PR01/tsuzuri-before` へ複製し、kernels の IR を保存している（PR02 と同じ手順）。

### 他チケットとの約束

PR01 はこれらに依存しないが、これらのチケットは PR01 の規則を壊してはならない。各チケットの実装者が参照できるよう、規則の入口を一つにする。

- F10（`Atomic`・`Mutex` など、共有参照経由で書き換わる型）: F10 は `Type::has_interior_mutability`（F10 で新規）を定める。
  `Type::is_frozen`（新規）はその否定（`!has_interior_mutability`）として、その型とそれを格納する型に
  false を返すようにする。false の型を指す `ref T` 引数には `noalias`・`readonly` を付けない（規則 S2）。
- A13（排他借用フィールド）: 不変条件 I1（「変えてはいけない意味」）を保つ。record に保持した `ref mut` を通じて、別の引数の参照先に届いてはならない。
  I1 が保てない設計になる場合は、A13 側で `parameter_facts`（新規）の規則 M1 から該当する型を除く。
- C08（`ref mut [T]` の意味の変更、GUIDE D-30 で承認待ち）: `ref mut [T]` を値渡しの記述子にする場合、その引数は `ptr` でなくなるので
  PR01 の属性は自然に付かなくなる（規則 P0）。記述子の中のポインターの別名情報は Phase 2 の対象とする。
- A09（借用フィールド・region）: 既存の `CheckedFunction::region_sources` は Phase 1 では使わない（D5）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. 属性を付けた後、E2E のどれか（native／wasm32 × `-O0`／`-O3`）の結果・トラップ・`live` が変わった。属性を個別に外して通さない。
   誤った属性による誤最適化の可能性が高く、規則そのものの見直しが要る。
2. 手順 3 の監査で、`readonly` を付ける引数への `store`、または `nocapture` を付ける引数の保存（`store ptr %pN`）・返却・`ptrtoint` が見つかった。
3. `ptr` 引数に渡る値として、`alloca`・`getelementptr`・引数・`load` 以外（`null`・`undef`・`poison`・`inttoptr`）が生成 IR に見つかった。
4. LLVM/Clang 17 以降のどれか（README.md の要件）が属性の構文を拒否した。`captures(none)` への書き換えで通さない（D3）。
5. 規則が型と `CheckedFunction` の情報だけでは足りず、所有権検査の結果を codegen へ渡す副表が必要になった（D2 の前提が崩れる）。
6. 既存テストの期待値を変える必要がある。ただし「既存テストへの影響」に挙げた、引数の並びを完全一致で比べる箇所の更新は除く。
7. 同じ入力から 2 回出した IR が一致しない、または native と wasm32 で異なる IR 文字列が必要になった。
8. `unsafe`、新しい crate、既定の WASM import、公開 wrapper（`@tz_*`）の引数の変更が必要になった。

## 現状と計測（HEAD `f8dc655`）

### 関数の定義と引数

- `src/llvm.rs` の `FunctionEmitter::emit` は `define internal {結果} {symbol}({parameters}) nounwind{debug}` を出す。引数は
  `{llvm_type} %arg{index}` で、本体は `loop:` ブロックの `%p{index} = phi ... [ %arg{index}, %entry ], [ ..., %{block} ]` から始まる。
  phi の後方の辺は自己末尾呼び出し（`back_edges`）で、2 回目以降の反復の引数は `%arg{index}` ではない。
- 付随する関数（`FunctionEmitter::auxiliary`）は `define internal {signature} nounwind`。`src/llvm_abi.rs` の `wrapper` はこれを使い、
  `.replacen("define internal ", "define ", 1)` で公開 wrapper `@tz_{name}` にする。関数値の adapter は `@tz.apply.{name}.{count}`、
  環境の解放は `@tz.env.drop.*`。
- `llvm_type`: `Type::Reference(_, false)` のうち `shared_array_element()` が Some のもの（共有の配列参照 `ref [T]`）は `%tz.array`
  の値渡し、それ以外の `Type::Reference(..)` は `ptr`。したがって `ref mut [T]`、`ref string`、`ref <record>`、`ref mut f64` は `ptr` で、
  `ref [f64]` は `{ ptr, i64 }` の値である。LLVM の引数属性は `ptr` 型の引数にしか付かない。
- ヘッダーの型（`src/llvm.rs` の固定ヘッダー）: `%tz.string`・`%tz.utf8string`・`%tz.array`・`%tz.list` は `{ ptr, i64 }`、
  `%tz.vec` は `{ ptr, i64, i64 }`、`%tz.closure` は `{ ptr, ptr, ptr, ptr }`（`Type::Function` と `Type::Task` の両方）。
- `storage_layout` は「64-bit・16 bytes の `i128`」の大きさと align で、wasm32 と古い layout の上界である（関数の doc comment）。
  生成 IR に `target datalayout`・`target triple` はなく（`src/` の検索で 0 件）、native と wasm32 は同じ IR 文字列を clang に渡す。
- 内部関数の属性は `nounwind`（一部の builtin の `noreturn`・`cold`）だけで、`noalias`・`readonly`・`nocapture`・`nonnull`・
  `dereferenceable`・`align`・`noundef` の引数属性は出していない。`docs/architecture.md` の `## 性能設計の原則` は
  「別名関係や独立性を証明できる場合だけ最適化し、根拠のない `noalias` 等を付けません」とする。

### 所有権の事実の所在

- `CheckedFunction`（`src/check.rs`）の field は `module`・`region_sources`・`origin`・`name`・`visibility`・`exported`・`parameters`
  （`Vec<Local>`、各 `Local` は `id`・`ty`・`name`・`mutable`・`span`・`provenance`）・`signature`・`body`・`span`・`capture_count`・`is_task`
  などで、所有権検査の結果は持たない。
- `crate::ownership::check_all(&module)` は `src/check.rs` で `polymorph::specialize` と `closures::lower` の後に、lower 済みの module 全体
  （std・単相化・持ち上げた closure を含む）へ実行される。`FunctionEmitter::emit` に届く関数はすべて借用検査を通っている。
- 規則に使える型の述語: `Type::carries_loans`・`contains_stored_reference`・`contains_stored_mutable_reference`・`can_capture`・`can_send`
  （`src/check.rs`）、`TypeContext::stored_all`（`src/recursive.rs`）、`CheckedModule::types`。
- 言語の事実: record の field・配列の要素・リストの要素は不変で、`ref mut` を取れるのは `let mut` の局所変数全体か既存の排他参照の貸し直しだけ
  （E1014 のメッセージ）。`ref mut` で貸すときスタックの配列は先にヒープへ移す。排他借用を取る関数はその場で完全適用し、部分適用を保存できない
  （docs/language.md の Ownership / Borrowing）。

### 計測済みの事実（2026-09-30 の probe）

- 「再現」の `ok` は `check` が成功し、IR は `define internal i8 @tz.fn.Main.add_into(ptr %arg0, ptr %arg1) nounwind {` と
  `define internal i64 @tz.fn.Main.total(ptr %arg0) nounwind {`、公開 wrapper は `define i64 @tz_probe(i64 %arg0) nounwind {`。
- 同じ局所変数を `ref mut` と `ref` で同時に渡すと E1014（`access conflicts with a live borrow; ...`）で拒否される。
- `kern` の `accumulate :: ref mut f64 -> ref [f64] -> unit` は `ptr %arg0, %tz.array %arg1`。inline を止めた `opt -passes='default<O3>'`
  では、属性なしだと `store double %v12, ptr %arg0` がループの各反復に残り、`%arg0` に `noalias noundef nonnull align 8 dereferenceable(8)` を
  手で足すと store がループの出口（`b0.b3_crit_edge`）へ移る。LLVM は属性なしでも `%arg0` に `captures(none)` を推論する。
- Apple Clang 21.0.0 と `/opt/homebrew/opt/llvm@21/bin/clang` は `nocapture` と `captures(none)` の両方を受け付ける（`-c -O3`）。
- 既存の計算カーネルの多くは C と同等（相対性能 0.99 以上が 25 種目）。別名関係で差が出る種目（参照を受け取る関数内のループ、
  複数の配列の同時走査）は PX02 の管理下にあり、まだない。時間の効果は未計測である。

### 再現（2026-09-30 に確認）

`/tmp/tz-PR01/ok/Main.tz`（`check` 成功）:

```tsuzuri
record Pair { left: i64, right: i64 }

def add_into :: ref mut i64 -> ref i64 -> unit
fn add_into target source = deref target = deref target + deref source

def total :: ref Pair -> i64
fn total pair = pair.left + pair.right

export def probe :: i64 -> i64
fn probe seed =
    let mut a = seed
    let b = seed + 1
    add_into (ref mut a) (ref b)
    let pair = Pair { left: a, right: b }
    total (ref pair)
```

`/tmp/tz-PR01/conflict/Main.tz` は上の `(ref b)` を `(ref a)` にしたもの（E1014）。`/tmp/tz-PR01/kern/Main.tz`（`check` 成功）:

```tsuzuri
def accumulate :: ref mut f64 -> ref [f64] -> unit
fn accumulate total values =
    for value in values do deref total = deref total + value

export def kernel_accumulate :: i64 -> f64
fn kernel_accumulate seed =
    let values = new [f64](1000, \index -> (index + seed) as f64)
    let mut total = 0.0
    accumulate (ref mut total) (ref values)
    accumulate (ref mut total) (ref values)
    total
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
T=/tmp/tz-PR01; OPT=/opt/homebrew/opt/llvm@21/bin/opt
target/release/tsuzuri check $T/ok && target/release/tsuzuri check $T/conflict   # 2 つ目は E1014
target/release/tsuzuri build $T/kern --emit llvm -o $T/kern.ll
grep -n '^define.*accumulate' $T/kern.ll
sed -E 's/(@tz\.fn\.Main\.accumulate\()ptr %arg0/\1ptr noalias noundef nonnull align 8 dereferenceable(8) %arg0/' \
  $T/kern.ll > $T/kern-attr.ll
for f in kern kern-attr; do
  $OPT -passes='default<O3>' -inline-threshold=-10000 -S $T/$f.ll -o $T/$f.opt.ll
  awk '/^define.*@tz.fn.Main.accumulate\(/,/^}/' $T/$f.opt.ll
done
```

期待: `kern` ではループのブロック（`b1`）に `store double`、`kern-attr` ではループの出口のブロックに `store double` がある。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を PX01 形式で記録して判断する。

- G1: Phase 1 の規則（「設計」の P0〜N1）が、証明できる引数にだけ属性を付け、それ以外の IR を 1 byte も変えない。
- G2: 意味を変えない。全 E2E の結果・トラップ・`live == 0`・WASM import なしが before と同じ。
- G3: 既存の種目で時間を悪化させない。改善は生成コードが変わった種目についてだけ PR01 の効果と呼ぶ。
- 長期目標（Phase 1 の完了条件ではない）: PX02 が別名の種目（`ref [f64]` 二つと `ref mut` の作業領域を受け取るループ、参照経由の繰り返し読み込み）を
  追加した後、`restrict` なしの C より速く、`restrict` ありの C・Rust と同等以上にする。配列の buffer どうしの別名は Phase 2 の対象である。

| 指標 | 単位・統計 | 対象 | 期待（計算値。確認で確かめる） |
| --- | --- | --- | --- |
| M1 属性 | 引数ごとの属性の文字列（静的） | kernels の IR（「生成コードの確認」） | 「生成 IR」の表と一致 |
| M2 ループ内の store | 個（静的） | `kern` の `accumulate` の最適化後 IR（inline なし） | 1 → 0（出口へ移る） |
| M3 時間 | ms（PX01、12 標本の中央値と最小・最大） | 計測対象の `control`・`cpp` の全種目 | 変化なし、または `record_pipeline`・`utf16_*` の改善（未計測） |
| M4 IR の変化 | 行（`diff` の行数） | 計測対象の Tsuzuri 実装の IR（`--emit llvm`） | `define internal` の行だけが変わる |

## 変えてはいけない意味

Phase 1 の属性は、次の不変条件 I1〜I4 が成り立つことに依存する。どれも HEAD の型検査・所有権検査がすでに保証しており、PR01 は検査を変えない。
I1〜I4 を弱める変更（A13・C08・F10 など）は、同時に「設計」の規則を狭める責任を負う。

- I1 排他: 呼び出しの間、`ref mut T` 引数の参照先には、その引数から導いたポインターだけが触れる。同じ所有者の別の借用は E1014 で拒否され、
  `ref mut` は関数値に捕捉できず（`Type::can_capture`）、排他借用を取る部分適用は保存できない。
- I2 凍結: 呼び出しの間、`ref T` 引数の参照先は誰からも書き換えられない。共有借用は所有者の変更を禁じ、HEAD には共有参照経由で書き換わる型
  （内部可変性）がない。F10 がそれを入れるときは `Type::is_frozen`（新規）で除く。
- I3 生存: 参照先は呼び出しの間ずっと生存し、少なくとも下界の大きさだけ読める。所有者は借用より長く生き、呼び出し先は参照先そのものを解放できない
  （解放できるのは参照先から読んだポインターの先だけ）。
- I4 逃避: 参照は結果の型か、`ref mut` 引数の参照先の中にしか保存できない。Task へは送れず（`Type::can_send`）、書き換えられる大域変数はない。

このほか次を保つ。

- 数値: 整数の overflow・キャスト、浮動小数点の丸め・NaN・符号付きゼロ、評価順序は変わらない。属性は命令を変えず、メモリ操作の最適化だけに効く。
  fast-math・再結合は有効にしない。
- トラップ: トラップの有無・種類・`@tz.trap.report` の site を変えない。トラップ時点での参照先の中身は観測できる結果ではない
  （host は trap 後に Tsuzuri の局所値を読む API を持たない）。
- 所有権・借用: 検査・診断・drop の位置は変えない。新しい診断はない。
- 決定性: 同じ入力から同じ IR を出す。属性の順序は固定し、大きさと align は target に依らない下界を使うので、native と wasm32 は同じ IR 文字列のまま。
- WASM: import を増やさない。wasm32 では `align` は memarg の align hint になるだけで、下界なので trap を生まない。
- host ABI: 公開 wrapper `@tz_*` の引数・結果・属性は変えない。wrapper は host の buffer を検査してから spill した slot のポインターを内部関数へ渡すので、
  内部関数の属性は host が契約を守るかどうかに依存しない。

## 設計

### 規則（Phase 1）

規則は型と `CheckedFunction` だけから決まる純粋関数 `parameter_facts`（新規）にまとめる。`FunctionEmitter::emit` が出す関数以外
（`auxiliary`、公開 wrapper、adapter `@tz.apply.*`、`@tz.env.drop.*`、builtin、runtime、`src/llvm_bulk.rs` の callback）は対象外で、IR は変わらない。

| 規則 | 条件 | 付ける属性 | 根拠 |
| --- | --- | --- | --- |
| P0 対象 | `capture_count == 0`、`!is_task`、引数の型が `Type::Reference(inner, mutable)` で `shared_array_element()` が None（LLVM 型が `ptr`）、`layout_bound(inner, module, true).0 > 0` | P0 を満たさない引数には何も付けない | 捕捉つきの closure と Task 本体は D7 で除く。値渡しの記述子に属性は付かない |
| C1 共通 | P0 | `noundef nonnull align A dereferenceable(N)`、`(N, A) = layout_bound(inner, module, true)` | I3。参照は `alloca`・`getelementptr`・引数・参照の `load` からだけ作られる（停止条件 3） |
| M1 排他 | P0 かつ `mutable` | `noalias` | I1 |
| S1 共有 | P0 かつ `!mutable` かつ `inner.is_frozen(&types)` | `noalias readonly` | I2。LLVM の `noalias` は呼び出し中に変更されるメモリだけを制約するので、変更されない参照先には常に正しく、他の store が参照先を壊さないことを伝える |
| S2 可変な共有 | P0 かつ `!mutable` かつ `!inner.is_frozen(&types)` | C1 だけ | HEAD では該当なし（F10 の拡張点） |
| N1 逃避なし | P0 かつ、結果の型が `carries_loans` でなく、`carries_loans` な型を指す `ref mut` 引数がない | `nocapture` | I4。関数値の環境に入った参照も `carries_loans`（`Type::Function` は true）で捉える |

属性の文字列は常に `noalias nocapture noundef nonnull readonly align A dereferenceable(N)` の順で、該当しないものを省く。

### データ構造

```rust
// src/check.rs（新規）
impl Type {
    /// False for types whose shared references permit writes (F10 interior mutability).
    pub(crate) fn is_frozen(&self, _types: &TypeContext<'_>) -> bool {
        true
    }
}

// src/llvm.rs
fn storage_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
    layout_bound(ty, module, false)
}

/// `lower == false`: the 64-bit upper bound used for allocation.
/// `lower == true`: a bound that every supported target meets (4-byte pointers, 8-byte-aligned 128-bit scalars).
fn layout_bound(ty: &Type, module: &CheckedModule, lower: bool) -> (usize, usize) { /* 既存の本体。葉だけ lower で切り替える */ }

/// One attribute prefix per parameter, each empty or ending with a space.
fn parameter_facts(function: &CheckedFunction, module: &CheckedModule) -> Vec<String> { /* 「アルゴリズム」 */ }
```

`layout_bound` の `lower == true` で値が変わる葉（それ以外の葉は `storage_layout` と同じ。record・tuple・union は既存の畳み込みがそのまま下界を作る）:

| 型 | LLVM 型 | 上界（`storage_layout`） | 下界 |
| --- | --- | --- | --- |
| `Type::Reference(..)`（`ptr`） | `ptr` | (8, 8) | (4, 4) |
| `Type::Function`・`Type::Task` | `%tz.closure` = `{ ptr, ptr, ptr, ptr }` | (32, 8) | (16, 4) |
| 128-bit の `Integer`・`Binary`・`Decimal` | `i128` など | (16, 16) | (16, 8) |
| `Type::Simd`（mask 以外） | `<N x T>` | (16, 16) | (16, 8) |
| 上の表以外で `storage_layout` が (8, 8) を返す arm（再帰型の box など） | `ptr` を含む | 既存値 | 実装者が LLVM 型を確かめ、`ptr` を (4, 4) として計算する |

`%tz.string`・`%tz.array`・`%tz.list`（`{ ptr, i64 }`）は wasm32 でも (16, 8)、`%tz.vec`（`{ ptr, i64, i64 }`）は (24, 8) で変わらない。
畳み込みは大きさと align が小さくなると offset も大きくならないので、下界の field から作った record の値も下界になる。

### アルゴリズム

```rust
fn parameter_facts(function: &CheckedFunction, module: &CheckedModule) -> Vec<String> {
    let types = module.types();
    let excluded = function.capture_count > 0 || function.is_task;
    let escapes = function.signature.result.carries_loans(&types)
        || function.parameters.iter().any(|parameter| {
            matches!(&parameter.ty, Type::Reference(inner, true) if inner.carries_loans(&types))
        });
    function.parameters.iter().map(|parameter| {
        let Type::Reference(inner, mutable) = &parameter.ty else { return String::new() };
        if excluded || parameter.ty.shared_array_element().is_some() {
            return String::new();
        }
        let (size, align) = layout_bound(inner, module, true);
        if size == 0 {
            return String::new();
        }
        let frozen = !*mutable && inner.is_frozen(&types);
        let mut text = String::new();
        if *mutable || frozen { text.push_str("noalias "); }
        if !escapes { text.push_str("nocapture "); }
        text.push_str("noundef nonnull ");
        if frozen { text.push_str("readonly "); }
        let _ = write!(text, "align {align} dereferenceable({size}) ");
        text
    }).collect()
}
```

`FunctionEmitter::emit` の引数の組み立ては `format!("{} {}%arg{index}", self.ty(&parameter.ty), facts[index])` にする。
属性は `%arg{index}`（入口の値）にだけ付き、`loop:` の phi（`%p{index}`）には付かない。自己末尾呼び出しで別のポインターが phi に入っても、
LLVM の `noalias` は「`%arg` から導いたポインター」と「それ以外」の区別なので I1〜I4 のもとで正しい。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 型性質 | `src/check.rs` | `Type::is_frozen`（新規） | 常に true。`can_send` の近くに置く |
| 大きさ | `src/llvm.rs` | `storage_layout`、`layout_bound`（新規） | 本体を `layout_bound` へ移し、`storage_layout` は `lower == false` で呼ぶだけにする。既存の呼び出し元は変えない |
| 引数属性 | `src/llvm.rs` | `parameter_facts`（新規） | 「アルゴリズム」 |
| 関数定義 | `src/llvm.rs` | `FunctionEmitter::emit` | 引数の文字列に `parameter_facts` の接頭辞を足す。`phi` の行は変えない |
| 付随関数 | `src/llvm.rs` | `FunctionEmitter::auxiliary`、`@tz.apply.*`・`@tz.env.drop.*` の生成 | 変更なし |
| 公開 wrapper | `src/llvm_abi.rs` | `wrapper` | 変更なし（`auxiliary` 経由なので属性は付かない） |
| frame・bulk | `src/llvm_frame.rs`、`src/llvm_bulk.rs` | `relocate`・callback | 変更なし |
| 特殊化 | `src/call_specialization.rs` | `single_use_locals` など | 変更なし。特殊化した関数も `emit` を通るので同じ規則が付く |
| 所有権 | `src/ownership.rs` | `check_all` | 変更なし（D2） |
| 文書 | `docs/architecture.md` | `## 性能設計の原則`、`## 不変条件` | 規則表と I1〜I4 |

### 生成 IR

「再現」の `ok` の期待（計算値）。`Pair` は下界 (16, 8)、`i64` は (8, 8)。

```llvm
define internal i8 @tz.fn.Main.add_into(ptr noalias nocapture noundef nonnull align 8 dereferenceable(8) %arg0, ptr noalias nocapture noundef nonnull readonly align 8 dereferenceable(8) %arg1) nounwind {
define internal i64 @tz.fn.Main.total(ptr noalias nocapture noundef nonnull readonly align 8 dereferenceable(16) %arg0) nounwind {
define internal i64 @tz.fn.Main.probe(i64 %arg0) nounwind {
define i64 @tz_probe(i64 %arg0) nounwind {
```

`kern` の `accumulate` は `ptr noalias nocapture noundef nonnull align 8 dereferenceable(8) %arg0, %tz.array %arg1`（`%arg1` は変わらない）。
ランタイム（`src/runtime/*.ll`）、ヘッダー、metadata の番号は変わらない。

### Phase 分割

- Phase 1（実装対象）: P0〜N1、`layout_bound`、`is_frozen`、テスト、文書、計測。
- Phase 2（設計方針のみ）: (a) 配列の記述子から取り出した buffer のポインターへの `!alias.scope`／`!noalias`（異なる所有者の配列は別の buffer であることを
  使う。`ref [T]` の値渡しの記述子と `ref mut [T]` の buffer の別名を消す）、(b) 値の引数・結果への `noundef`（`undef`・`poison` を作る箇所の監査が先）、
  (c) 関数の `memory(...)`（確保・解放・IO・Task・extern を副作用として扱う。[A19](../_features/A19-pure-functions.md) の `src/pure_rules.rs` が、同じ副作用の分類と到達可能性の解析を先に持つので、再利用を検討する）、(d) 所有 buffer の `!nonnull`。`!range` は PR02 の D4、
  `llvm.lifetime.*` は PM09 が扱う。
- Phase 3（設計方針のみ）: 型の異なる要素の読み書きへの TBAA。ランタイムの byte 単位のアクセスは「すべてと別名になり得る」型にする。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインと変更前のコンパイラ

- 変更: なし。
- 内容: 「再現」の 3 つの project を `/tmp/tz-PR01/` に置き、変更前のコンパイラと IR を保存する。
- 確認: 次がすべて成功し、`conflict` だけが E1014 で失敗する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
cp target/release/tsuzuri /tmp/tz-PR01/tsuzuri-before
for p in ok kern; do
  target/release/tsuzuri build /tmp/tz-PR01/$p --emit llvm -o /tmp/tz-PR01/$p-before.ll
  target/release/tsuzuri build /tmp/tz-PR01/$p --emit llvm -O3 -o /tmp/tz-PR01/$p-before-O3.ll
done
target/release/tsuzuri check /tmp/tz-PR01/conflict   # E1014
```

### 手順 2: `Type::is_frozen` と `layout_bound`（IR は変えない）

- 変更: `src/check.rs`（`Type::is_frozen`（新規））、`src/llvm.rs`（`storage_layout` の本体を `layout_bound`（新規）へ移す）。
- 内容: 「データ構造」のとおり。`lower == true` の葉は「データ構造」の表に従い、`storage_layout` が (8, 8) を返す arm を一つずつ LLVM 型で確かめる。
  `parameter_facts` はまだ作らない。
- 確認: `cargo test --locked` が成功する。手順 1 と同じコマンドで出した IR が `cmp /tmp/tz-PR01/ok-before.ll /tmp/tz-PR01/ok.ll` などで一致する。

### 手順 3: E2E fixture と suite（属性なしで先に通す）

- 変更: `tests/fixtures/param_facts/Main.tz`（新規）、`tests/features.mjs`（suite `param_facts`（新規）、GUIDE §7.4）。
- 内容: 「テスト計画」の E2E の export を置く。期待値は手で計算した値（「テスト計画」の式）で、変更前のコンパイラの出力から取らない。
  `inspect` はまだ足さない。未検証の形（表の「要確認」）は `target/release/tsuzuri check` で確かめ、通らなければ同じ規則を示す別の形に替える。
  替えられなければその case を削り、完了報告に書く。
- 確認: `node tests/features.mjs target/release/tsuzuri param_facts` が成功する（属性を付ける前の意味の基準）。

### 手順 4: 監査（コードの変更なし）

- 変更: なし。
- 内容: 変更前の IR で、S1・N1 を付ける予定の引数を LLVM 自身の推論と突き合わせる。LLVM が `readonly`・`captures(none)` を推論できた引数は本体と矛盾しない。
  推論できない引数は本体を読み、理由（間接呼び出し、phi、別の関数への受け渡し）を完了報告に書く。理由が store・保存・返却なら停止条件 2。
  `ptr` 引数へ渡る値の出どころも調べる（停止条件 3）。
- 確認: 次の出力を `/tmp/tz-PR01/audit.txt` に保存し、完了報告に要約する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
OPT=/opt/homebrew/opt/llvm@21/bin/opt; B=/tmp/tz-PR01/tsuzuri-before
$B build tests/fixtures/param_facts --emit llvm -o /tmp/tz-PR01/fixture-before.ll
for f in ok-before kern-before fixture-before; do
  $OPT -passes='function(sroa,early-cse,instcombine,simplifycfg),cgscc(function-attrs)' -S /tmp/tz-PR01/$f.ll -o - |
    grep -E '^define internal .*@tz\.fn\.'
done > /tmp/tz-PR01/audit.txt
grep -cE 'call [^(]*@tz\.fn\.[^(]*\(.*ptr (undef|poison|null|inttoptr)' /tmp/tz-PR01/fixture-before.ll   # 0
```

### 手順 5: `parameter_facts` と `FunctionEmitter::emit`

- 変更: `src/llvm.rs`（`parameter_facts`（新規）、`FunctionEmitter::emit`）。
- 内容: 「アルゴリズム」のとおり。`auxiliary` と `wrapper` には触れない。
- 確認: `target/release/tsuzuri build /tmp/tz-PR01/ok --emit llvm -o /tmp/tz-PR01/ok.ll` の `define` の行が「生成 IR」と一致する。
  `diff /tmp/tz-PR01/ok-before.ll /tmp/tz-PR01/ok.ll` の差が `define internal ... @tz.fn.Main.add_into(`・`@tz.fn.Main.total(` の 2 行だけ。
  `cargo test --locked` が成功する（「既存テストへの影響」以外の期待値を変えない。停止条件 6）。

### 手順 6: Rust テスト

- 変更: `tests/param_facts.rs`（新規）。
- 内容: 「テスト計画」の Rust テスト。`tests/slices.rs` と同じく `analyze(source)` と `llvm::emit(&module, llvm::Entry::Library)` を使う。
- 確認: `cargo test --locked --test param_facts` が `10 passed`。

### 手順 7: E2E の inspect と全 suite

- 変更: `tests/features.mjs`（suite `param_facts` に `inspect(ir)`）。
- 内容: `add_into` と `accumulate` の define の行の属性を正規表現で確かめ、`@tz_` で始まる行に `noalias` がないことを確かめる。
- 確認: 「テスト計画」の共通コマンドがすべて成功する。

### 手順 8: 生成コードの確認

- 変更: なし。
- 内容: 「生成コードの確認」のコマンドを before と after で実行し、表にする。
- 確認: M1 が「生成 IR」と一致し、M2 が 1 → 0。native と wasm32 の IR 文字列が同じ（`--target wasm32 --emit llvm` と比べる）。

### 手順 9: 計測と記録

- 変更: なし（記録は `target/perf/`）。
- 内容: 「計測手順」。
- 確認: `target/perf/PR01-before`・`PR01-after`・`PR01-before2` に `env.txt` と suite ごとの記録がある。

### 手順 10: 文書と状態

- 変更: `docs/architecture.md`、`docs/benchmarks.md`、`_perfs/README.md`（PR01 を done）。
- 内容: 「ドキュメント」。
- 確認: `node scripts/check-docs.mjs docs/architecture.md docs/benchmarks.md` が成功し、`git diff --check` が空。

## 計測手順

PX01 の「計測手順」の「環境と条件」に従う（AC 電源、端末を共有する他の作業を止める、load average の記録）。標本は各 runner の既存の warm-up と
12 標本（≥ 9）で、中央値と広がりを PX01 形式で `target/perf/<run_id>/` に残す。run は `PR01-before`、`PR01-after`、`PR01-before2` の順。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node24() { npx --yes --package=node@24 node "$@"; }
measure() { # $1 = run id, $2 = compiler
  local RUN=target/perf/$1
  mkdir -p "$RUN"
  { git rev-parse HEAD; sw_vers; uname -m; sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize
    uptime; clang --version | head -1; rustc --version; node24 --version; } > "$RUN/env.txt"
  for suite in control cpp; do
    node24 benchmarks/run-$suite.mjs "$2" --metrics "$RUN" > "$RUN/$suite.json" || return 1
  done
}
measure PR01-before /tmp/tz-PR01/tsuzuri-before
measure PR01-after target/release/tsuzuri
measure PR01-before2 /tmp/tz-PR01/tsuzuri-before
node24 benchmarks/metrics.mjs report target/perf/PR01-before2 --baseline target/perf/PR01-before
node24 benchmarks/metrics.mjs report target/perf/PR01-after --baseline target/perf/PR01-before
```

- 判定: `before2` 対 `before` で差が出た metric は「判定できない」と報告する。`after` 対 `before` の verdict をそのまま完了報告に貼る。
- M4: 各 suite の Tsuzuri 実装を before と after で `--emit llvm` し、`diff` が `define internal` の行だけであることを確かめる。IR が変わらない種目の
  差を PR01 の効果と呼ばない。
- `--quick` は動作確認だけに使う。`--cpu native` は測らない。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
T=/tmp/tz-PR01; OPT=/opt/homebrew/opt/llvm@21/bin/opt; OBJDUMP=/opt/homebrew/opt/llvm@21/bin/llvm-objdump
for p in ok kern; do
  target/release/tsuzuri build $T/$p --emit llvm -o $T/$p.ll
  target/release/tsuzuri build $T/$p --target wasm32 --emit llvm -o $T/$p-wasm.ll
  cmp $T/$p.ll $T/$p-wasm.ll                      # 同じ IR 文字列（停止条件 7）
  grep -nE '^define' $T/$p.ll
done
$OPT -passes='default<O3>' -inline-threshold=-10000 -S $T/kern.ll -o $T/kern.opt.ll
awk '/^define.*@tz.fn.Main.accumulate\(/,/^}/' $T/kern.opt.ll
target/release/tsuzuri build $T/kern --emit object -O3 -o $T/kern.o
$OBJDUMP -d --no-show-raw-insn $T/kern.o > $T/kern.s
target/release/tsuzuri build $T/kern --target wasm32 -O3 -o $T/kern.wasm
$OBJDUMP -d $T/kern.wasm > $T/kern.wasm.txt
```

- IR（M1）: `ok` の define の行が「生成 IR」と一致する。`kern` の `accumulate` は `%arg0` にだけ属性が付き、`%tz.array %arg1` は変わらない。
  `@tz.apply.*`・`@tz.env.drop.*`・`@tz_*` の行は before と同じ。
- 最適化後の IR（M2）: `accumulate` のループのブロックに `store double` がなく、出口のブロックに 1 つある。
- native（`kern.s`）と wasm32（`kern.wasm.txt`）: `-O3` では `accumulate` が inline される見込みで、before と同じでもよい。変化の有無を表に書く。
- `cmp` が失敗したら（IR の `--target` による差が元からある場合を含む）、before でも同じかを確かめてから報告する。

## テスト計画

### 共通コマンド（手順 7）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/e2e.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/tasks.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
```

### Rust テスト

`tests/param_facts.rs`（新規）。期待する文字列は「設計」の規則と「データ構造」の下界の表から手で作る。

| テスト | 入力 | 期待 |
| --- | --- | --- |
| `exclusive_and_shared_scalar_references_get_facts` | 「再現」の `ok` の `add_into` | 「生成 IR」の `add_into` の行と完全一致 |
| `shared_record_reference_uses_the_lower_bound` | `ok` の `total` | `dereferenceable(16)` と `align 8`、`readonly` あり |
| `string_fields_keep_their_descriptor_size` | `record Named { name: string, id: i64 }` と `def id_of :: ref Named -> i64`（要確認） | `readonly align 8 dereferenceable(24) %arg0` |
| `exclusive_array_reference_points_to_the_descriptor` | `tests/slices.rs` の `replace :: ref mut [i64] -> unit` | `@tz.fn.Main.replace(ptr noalias nocapture noundef nonnull align 8 dereferenceable(16) %arg0)` |
| `shared_array_descriptors_are_unchanged` | `tests/slices.rs` の `length :: ref [i64] -> i64` | `@tz.fn.Main.length(%tz.array %arg0) nounwind` |
| `loan_carrying_results_disable_nocapture` | 共有参照を捕捉した関数値を返す関数（要確認） | その関数の `ptr` 引数に `noalias` はあり `nocapture` はない |
| `captured_closures_get_no_facts` | 局所値を捕捉し `ref i64` 引数を取る lambda（要確認） | 持ち上げた関数（`capture_count > 0`）の引数に属性がない |
| `public_wrappers_are_unchanged` | `export def` で `ref string` 引数を取る関数（要確認） | `@tz_` の行は before と同じで `noalias` を含まない。内部関数には S1 の属性 |
| `facts_are_deterministic` | `ok` と `kern` | 2 回の `llvm::emit` が一致 |
| `aliasing_calls_stay_rejected` | 「再現」の `conflict`、`ref mut pair.left` | どちらも `E1014`（I1 の根拠。PR01 は検査を変えない） |

「要確認」の入力は手順 6 で `analyze` が通ることを確かめる。通らない場合は同じ規則を示す別の形を探し、なければそのテストを削って報告する。

### E2E

`tests/fixtures/param_facts/Main.tz`（新規）、`tests/features.mjs` の suite `param_facts`（新規）。features.mjs の既定どおり native は確保を追跡して
`live == 0`、WASM は import なし。期待値は式から計算する（`seed` は i64）。

| export | 内容 | seed | 期待 |
| --- | --- | --- | --- |
| `probe` | 「再現」の `ok` | -1000、0、7、10^12 | `3 * seed + 2` |
| `sum_twice` | `kern` の `accumulate` を `i64` にしたもの（`new [i64](1000, \index -> index + seed)` を 2 回足す） | -1000、0、7、10^12 | `999000 + 2000 * seed` |
| `replace_length` | `let mut values = [1, 2, 3]`、`replace (ref mut values)`（`deref values = [42]`）の後の `values.length + seed` | 0、5 | `1 + seed` |
| `shared_twice` | 同じ record を `ref` で 2 つの引数に渡し、field を足す（`noalias readonly` 同士） | 0、9 | 計算式を fixture のコメントに書く |

`inspect(ir)`（手順 7）: `add_into` と `accumulate` の属性の正規表現、`^define [^\n]*@tz_[^\n]*noalias` に一致しないこと。

### 既存テストへの影響

- `tests/slices.rs` の `contains("@tz.fn.Main.replace(ptr")` は属性の後も部分一致する（変更不要）。`tests/host_abi.rs` の `define i64 @tz_sum(ptr %arg0_ptr, i64 %arg0_len)`
  は wrapper なので変わらない。
- `src/llvm.rs` の unit test は builtin（`@tz.builtin.*`）の define を完全一致で比べるが、builtin は `emit` を通らないので変わらない。
- `define internal ... @tz.fn.*(ptr %arg` を完全一致で比べるテストが見つかった場合だけ、その期待値を属性つきに更新してよい（停止条件 6 の例外）。

### 性能

時間の合否条件は置かない。「計測手順」の記録と「生成コードの確認」の表を完了報告に載せる。

## ドキュメント

- `docs/architecture.md` の `## 性能設計の原則`: 「根拠のない `noalias` 等を付けません」の後に、Phase 1 で付ける属性と対象（内部関数の `ptr` 引数だけ、公開 wrapper は変えない）を 2〜3 文で書く。
- `docs/architecture.md` の `## 不変条件`: I1〜I4 と規則の表（P0〜N1）を載せ、F10・A13・C08 が `Type::is_frozen` と規則を更新する責任を書く。
- `docs/benchmarks.md`: PR01 の before／after（M3）と生成コードの表（M1・M2）を、既存の結果の節と同じ形式で追記する。改善がなければ「変化なし」と書く。
- `_perfs/README.md`: PR01 の状態を done にする。

## 受け入れ条件

- [ ] Phase 1 の規則（P0〜N1）が `parameter_facts` 一か所にあり、`FunctionEmitter::emit` だけが使う。
- [ ] `tests/param_facts.rs` が成功し、付与と非付与の両方を検査している。
- [ ] 「テスト計画」の共通コマンドがすべて成功し、E2E の結果・トラップ・`live == 0`・WASM import なしが before と同じ。
- [ ] PR01 単独の変更では公開 wrapper・adapter・runtime に属性を推測付与せず、ABI を保つ。native と wasm32 の各構成で繰り返し出力が一致する。統合時の runtime 差は PB01・PR04・PR05 の検証と区別する。
- [ ] 手順 4 の監査結果と、生成コードの表（M1・M2）が完了報告にある。
- [ ] 計測（M3）を PX01 形式で記録し、`before2` 対 `before` の広がりと合わせて報告した。未計測の改善を主張していない。
- [ ] `docs/architecture.md` に I1〜I4 と規則が書かれている。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `dereferenceable` と `align` に `storage_layout`（64-bit の上界）を使うと、wasm32 で実際の object より大きい範囲を読めると主張し、LLVM が範囲外の
  投機的な load を作り得る。必ず `layout_bound(.., true)` を使う。
- 大きさ 0 の参照先に `dereferenceable(0)` を出さない（P0 で属性を付けない）。
- 属性を `loop:` の phi（`%p{index}`）や `auxiliary` の署名に付けない。`auxiliary` は公開 wrapper と共有なので、付けると host ABI が変わる。
- `captures(none)` は LLVM 21 からの綴りで、要件の LLVM 17〜20 は読めない。`nocapture` を使う（D3）。
- `-O3` の通常の build では参照を取る小さな関数は inline され、効果が見えないことが多い。M2 は inline を止めた `opt` で確かめ、その結果を時間の改善と呼ばない。
- LLVM は `nocapture` と `readonly` を本体から推論できることが多い（「計測済みの事実」）。効果の中心は `noalias`・`dereferenceable`・`align` である。
- `ref [T]`（共有）は `%tz.array` の値渡しなので属性が付かない。配列の buffer どうしの別名を消すのは Phase 2 で、Phase 1 の成果として書かない。
- F10 が内部可変な型を入れるとき `Type::is_frozen` を更新し忘れると、S1 が誤った `noalias readonly` を付け、並行の書き込みが見えなくなる。
  `docs/architecture.md` の不変条件に書き、F10 のチケットから参照させる。
- `cargo test --locked param_facts` のような filter は 0 件でも成功する。`--test param_facts` を使い、`running 10 tests` を確かめる。
- Node 20.19.6 は重い suite で V8 が異常終了することがある。`tests/tasks.mjs` と runner は Node 24 で動かす。

## 対象外

- 利用者が書く `restrict` 相当の注釈、ホスト側のポインターに対する推測。
- 公開 wrapper `@tz_*`・adapter・runtime・builtin の属性。
- 内部の呼び出し規約の変更（記述子を `ptr` と `i64` に分けて渡すなど）。
- 結果への属性、値の引数への `noundef`、関数の `memory(...)`、`!alias.scope`、`!nonnull`、TBAA（Phase 2・3）。`!range`（PR02）、`llvm.lifetime.*`（PM09）。
- `region_sources` による N1 の精密化（D5）。

## 決定事項

### D1: Phase 1 の範囲

- 決定: `FunctionEmitter::emit` が出す内部関数の、LLVM 型が `ptr` の参照引数だけに属性を付ける（P0〜N1）。結果・値の引数・関数属性は付けない。
- 理由: 証明が型だけで閉じ、誤りの影響範囲が引数の参照先（固定の大きさ）に限られる。probe で `noalias` の効果（store のループ外への移動）を確かめた。
- 状態: 既定案（実装者はこの案に従う）

### D2: 事実の流れ（副表を作らない）

- 決定: 所有権検査の結果を codegen へ渡す副表は作らず、`parameter_facts` が型と `CheckedFunction` から再計算する。
- 理由: 規則はすべて「借用検査を通ったプログラムでは型ごとに成り立つ」事実で、関数ごとの検査結果を要しない。`ownership::check_all` は特殊化と
  closure の lower の後の module 全体を検査するので、`emit` に届く関数はすべて対象になっている。副表は特殊化・lower の後に古くなる危険がある。
- 状態: 既定案（実装者はこの案に従う）

### D3: `nocapture` の綴り

- 決定: `nocapture` を出す。`captures(none)` は使わない。
- 理由: README.md の要件は LLVM/Clang 17 以降で、`captures(none)` は LLVM 21 から。Apple Clang 21 と LLVM 21 は `nocapture` を受け付ける（probe）。
- 見直し提案: 要件が LLVM 21 以上に上がったら `captures(none)` へ移す。
- 状態: 既定案（実装者はこの案に従う）

### D4: 大きさと align は target に依らない下界

- 決定: `dereferenceable` と `align` は `layout_bound(.., true)`（`ptr` は 4 bytes、128-bit の scalar と vector は align 8）から出す。
- 理由: IR は native と wasm32 で同じ文字列で、上界を使うと wasm32 で誤りになる。target ごとの IR にすると決定性の不変条件と cache を変える。
- 状態: 既定案（実装者はこの案に従う）

### D5: `region_sources` を使わない

- 決定: N1 は「結果が `carries_loans` でなく、`carries_loans` な型を指す `ref mut` 引数がない」だけで判断し、`region_sources` で引数ごとに絞らない。
- 理由: LLVM は多くの場合 `nocapture` を自分で推論でき、精密化の効果は小さい。規則が一つの型の述語で済み、A09 の規則変更の影響を受けない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 値の `noundef` は Phase 2

- 決定: 旧版 Phase 1 の「すべての値引数・戻り値に `noundef`」は Phase 2 へ移す。Phase 1 の `noundef` は参照の `ptr` だけ。
- 理由: 値の `noundef` は union の payload や一部だけ書いた aggregate に `undef`・`poison` が残らないことの監査が先に要り、誤ると即座に未定義動作になる。
- 状態: 既定案（実装者はこの案に従う）

### D7: 捕捉つき closure と Task 本体を除く

- 決定: `capture_count > 0` または `is_task` の関数には属性を付けない。
- 理由: 捕捉は環境経由で渡り、引数の並びと捕捉の対応を監査する費用に対して、性能上の利点が小さい。旧版の「関数値の引数には付けない」を具体化したもの。
- 状態: 既定案（実装者はこの案に従う）

### D8: 公開 wrapper は変えない（旧未決事項）

- 決定: 属性は内部関数だけに付け、公開 wrapper `@tz_*` の引数・属性は変えない。
- 理由: host が契約を破ったとき未定義動作にしない。wrapper は検査後に spill した slot を内部関数へ渡すので、内部関数の属性は host に依存しない。
- 状態: 既定案（実装者はこの案に従う）

### D9: 内部可変性の入口

- 決定: `Type::is_frozen`（新規、HEAD では常に true）を S1 の唯一の条件にする。F10 が `Type::has_interior_mutability`（F10 で新規）を入れたら、
  `is_frozen` は `!has_interior_mutability` を返す（共有参照経由で書き換わる型とそれを格納する型で false）。判定の実体は F10 の一か所にしか書かない。
- 理由: Rust の `Freeze` と同じ扱い。規則の入口が一つなら、F10 の実装者は PR01 のコードを読まずに済む。
- 状態: 既定案（実装者はこの案に従う）

### D10: 計測の種目

- 決定: 既存の `control`・`cpp` の全種目で時間を測り、kernels は生成コードの確認にだけ使う。別名の種目の追加と C・Rust との比較は PX02 が行う。
- 理由: runner は種目名の一覧と参照実装を持ち（`benchmarks/run-control.mjs` の `assert.deepEqual`）、種目の追加は PX02 の管理下にある（PR02 の D8 と同じ）。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-14）

利用者が承認した全 3 phase を、証明できる範囲に限定して実装した。旧 Phase 1
限定案・未実装 F10 の仮定は、冒頭の照合と以下の実装結果に置き換わる。
チケットの移動・共有一覧・中央の文書・測定の排他調整は調整担当の範囲であり、
ここでは commit／push／移動を行っていない。許可された最終 frontend 計測は以下に記録する。
全体の完了・CI・公開の checkbox は調整担当が判断するまで変更しない。

### 実装した事実と意図的な非付与

| phase | 実装 | 非付与／フォールバック |
| --- | --- | --- |
| 1 | 通常の内部関数の `ptr` 参照に、正の portable layout 下界から `noundef nonnull align dereferenceable`。閉じた pointee の排他性／不変性から `noalias`、共有の `readonly`。借用の返却・格納・隠れた環境がなければ `nocapture` | zero-sized pointee、slice 記述子、Task、捕捉付き worker、public wrapper／adapter。格納参照・opaque std・Shared・callable／host state を unique とみなさない |
| 2 | 初期化済み scalar 引数の `noundef`、限定した poison-free scalar result。分岐・反復・trap のない生成 IR の private stack／直接の pointer 引数と明示 whitelist intrinsic の memory effects。成功時の `tz.alloc` の `nonnull` と private reload の `!nonnull`。scalar 参照／scalar slice の確定した SSA 由来に scoped alias | 集約値／SIMD の value `noundef`、空／move 済み／frame buffer の blanket nonnull、記述子の `argmem` 推測、未知の呼出し、alloc/free/IO/Task/extern/Drop、分岐・反復・trap の totality 推測、書換え／露出した spill、未知の load／cast／phi 由来、callback |
| 3 | scalar pointee／slice element と LLVM access 型が一致した load/store に TBAA。符号違い・同じ LLVM integer encoding は同じ node | byte・bool/unit は無印で全型と alias。aggregate／record field／union payload／runtime byte copy／atomic へ型名だけで付与しない |

`src/llvm_facts.rs` に規則と bounded SSA grammar をまとめ、`Globals` の共有 metadata
番号で決定的に発行・TBAA と nonnull node を重複除去する。任意の pointer load を、
その load の address に由来する pointer と扱わない。private な single-store spill
だけが既知の由来を中継し、slice の data projection／inbounds GEP／descriptor の
再構築だけを追跡する。IR bytes と instruction 分の合計 200,000、引数 64、
最大 8 pass の予算を超えた場合は任意の事実を省く。

scoped alias は共有同士を独立としない。少なくとも片方が排他の loan にだけ
`!noalias` を記述し、関数呼出しごとに `llvm.experimental.noalias.scope.decl` を置く。
scalar-only `Array.write`／`Array.swap_in` の **実際の lifted builtin** だけは call の
access scope も記述し、inline 後の buffer store に伝える。一般の同じ形の関数、
要素 Drop／callback に外挿しない。更新された自己末尾 phi の pointer 由来を、
初回 `%argN` の由来と混同しない。

`Type::is_frozen` と `TypeContext::reaches` の可視性は PB02 と調整した。
F10 cells だけでなく、`Channel` の `{block:i64}` のような隠れた状態を含むすべての
opaque std record も保守的に frozen ではない。`storage_layout` の割当上界は維持し、
`layout_bound` は具象 field／Drop live flag／recursive union／FixedArray を反映する。
`dyn` 下界は `(8,4)`、closure は `(16,4)`、pointer は `(4,4)`、128-bit と SIMD
alignment は 8 以下。CPU stub が scalar result の `noundef` を `ret` 命令の型へ
誤ってコピーしないため、関連する `llvm_cpu.rs` の出力も修正した。

### facts 解析の余分な仕事の削減

統合後の frontend 計測で emit-LLVM CPU の悪化が見つかり、属性／metadata を
減らさずに `llvm_facts.rs` の解析を改善した。

- analysis 用に instruction を一度だけ分解し、provenance／memory／value proof で
  借用した文字列を再利用する。metadata のない行・通常の SSA assignment では
  不要な複数文字 substring search を避け、従来の grammar の結果を保つ。
- 最大 8 pass の forwarding は、その grammar で由来を持ち得る candidate だけに
  限定する。private slot の store／exposure 判定で同じ access を繰り返し分解せず、
  最後に捨てる非 slot の store 記録も作らない。
- capture proof は正サイズの pointer 参照属性が必要になったときだけ評価する。
  scalar の hidden-state graph traversal、更新された自己末尾 phi の解析、
  loan root／allocation／memory summary のいずれにも関係しない body の解析を省く。
- access の型／address は借用し、文字列を毎回 clone しない。builtin call の scope
  検査は scope があるときだけ行い、allocator の return attr は元の行へ挿入する。
- fresh profile で残っていた annotation 段階の再分解も、loan root と allocator call が
  ともにない body では省く。必要な memory／result facts を計算した **後** の省略で、
  pointer の由来・nonnull・TBAA／scope の証明規則を弱めたものではない。

`skipped_and_linear_bodies_keep_exact_ir_and_facts` は、直線的な memory-free body、
summary 対象外の分岐 body、自己末尾 body の IR と parameter／result／memory facts、
不要な metadata／intrinsic を発行しないことをまとめて検査する。
最終 `00d5` には、調整担当による `llvm_simd.rs::wide_types` の同値な
`starts_with('%')` 前置 filter も含まれる。この変更は PR01 agent の編集ではない。

### 検証で発見して直した誤分類

初回の intrinsic whitelist に `llvm.trap` を入れたのは誤りだった。
`memory(none)` の unit-returning call が WASM `-O0` で削除され、
既存 `control` の `trap_step` が trap しなくなった。trap を whitelist から除き、
totality の別の証明がない分岐／反復／unreachable body も function effects の対象外に
した。属性を外してテストだけを通したのではなく、規則を修正し、step 0・assert・
unreachable・未使用 checked overflow・未使用 conditional failure・loop trap の
6 つの native/WASM 回帰と、非終了 loop の非付与 IR test を追加した。

### 正しさの結果

- `cargo test --locked --test param_facts`: **9 passed**。
  pointer attrs／layout、F10／opaque／Shared、returned/stored loans／callback、
  descriptor ABI／scopes／TBAA、scalar/aggregate effects、public wrapper、
  union／nonnull reload、captured workers／Task、trap／termination の付与と非付与。
- `cargo test --locked --lib llvm::facts::tests`: **4 passed**。
  allocator private reload、書換え／露出／任意 pointer load/cast の非追跡、予算超過、
  上記 skipped／linear path の exact-IR 回帰。解析改善後にも再実行した。
- CPU `scalar_return_facts_do_not_become_ret_instruction_attributes`: **1 passed**。
- 関連する既存 Rust tests: `debug_info` **9**、`host_abi` **4**、
  `maybe_result` **9**、`parallel` **2**、`slices` **3**、`stdlib` **7**、
  `user_drop` **26** passed。既存の allocation 数を変えず、call return attr と
  scalar result attr に依存しない形へ exact-IR assertion を直した。
- GUIDE の 4 selector（growing polymorphism、recursive/flat expression depth、
  nested builder expansion、exact specialization limit）は、それぞれ **1 passed**。
  `RUST_MIN_STACK` は未設定の通常 stack。limit／stack の変更はしていない。
- `features.mjs ... param_facts`: **113 cases + 6 traps**、
  native／WASM × `-O0`／`-O3`、通常 allocator、ASan、host allocator の 3 実行で pass。
  native は各 case で `live == 0`、WASM import なし、IR deterministic、
  intrinsic declaration の重複なしを確認。
- cache-free `control.mjs`: **270 cases** と既存 trap／loop boundary／cleanup、
  native／WASM × `-O0`／`-O3` pass。上記 trap regression の修正後の結果。
- `cargo clippy --locked --all-targets -- -D warnings`: 解析改善後にも pass。
- manual は `borrowing.md` と `strategy.md` を更新。
  `check-docs.mjs` は **2 pages、53 links、8 examples、16 native runs (O0/O3)** pass。

共有 worktree の全 suite／全 target／Windows gate は調整担当が検証する。
ここでの実行検証は macOS arm64、Apple Clang 21.0.0、Node 20.19.6 と WASM。
Windows で実行したとは主張しない。

### 生成コードの確認

baseline は `target/perf/phase2-baseline/tsuzuri-before`。同じ fixture の IR と、
inline を含まない `sroa,loop-simplify,lcssa,loop-mssa(licm),gvn,dse,simplifycfg`
の結果を `target/perf/PR01-validation/{before,after}{,-opt}.ll` に保存した。
`accumulate` は before で input load と output store が loop 内、after で input load が
preheader、output store が exit へ移り、loop は phi と加算だけになる。

LLVM/Clang 23.1.1 で、呼出し元の閉世界最適化を防ぐ **inspection 用** standalone
kernel を出して確認した（実際の public ABI は変更していない）。
before は alias check と scalar/vector loop を持つが、after は input load、`madd`、
1 回の output store になった。`*-kernel.ll`／`*-kernel.s` も同じ directory にある。
全 module の `-O3` では before も単一の既知 caller から独立性を推論できるので、
その asm だけでは PR01 の差を示せないことも確認した。

LLVM 23 の `opt` 出力を Apple Clang 21 に渡すと、新しい
`nocreateundeforpoison` の attribute を 21 が拒否するため、inspection は同じ 23 の
tool pair で行った。compiler が出す raw IR は Apple Clang 21 の上記 native tests
で検証済みで、23 の syntax を compiler 出力へ足したわけではない。

### 最終 frontend 計測・IR 同値性（final2）

調整担当の排他的な許可を受け、`benchmarks/run-frontend.mjs` の正式な各 9 sample
（別に warmup 1 回）を before／after／before2 の順で実行した。hello・200・1000 の
check／emit-LLVM、generic CPU、cache 無効。最終 after は独立に pin した
`00d5d6a434fe6a3830741adc9f86e614f065794aaa426de0152adea6160e07c9`、
before／before2 は `target/perf/phase2-baseline/tsuzuri-before`
（`ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`）。

以下は **before／after／before2** の中央値。CPU／wall は ms、RSS は MiB。

| workload | CPU | wall | peak RSS |
| --- | --- | --- | --- |
| hello/check | 40 / 30 / 40 | 97.572 / 85.348 / 88.304 | 25.19 / 20.83 / 25.20 |
| hello/emit-LLVM | 40 / 40 / 40 | 89.508 / 87.156 / 93.979 | 26.67 / 23.55 / 26.69 |
| 200/check | 390 / 330 / 400 | 421.530 / 352.182 / 430.808 | 214.48 / 141.09 / 214.34 |
| 200/emit-LLVM | 550 / 550 / 550 | 584.110 / 583.162 / 590.072 | 223.73 / 154.11 / 223.06 |
| 1000/check | 1890 / 1600 / 1890 | 2147.114 / 1872.570 / 2145.012 | 965.78 / 615.61 / 965.53 |
| 1000/emit-LLVM | 2630 / 2640 / 2640 | 2897.476 / 2907.470 / 2910.752 | 1006.95 / 662.98 / 1002.02 |

全体の CPU／wall／total phase の判定は「差なし」または「改善」、RSS はすべて
「改善」、before2 drift の「悪化」は 0。**emit phase 単体のコスト増は残る**:
hello は 2.486 → 4.742 ms、200 は 139.695 → 200.405 ms、1000 は
679.220 → 969.691 ms（**+290.471 ms、約 +42.8%**）。3 種目とも phase gate は
「悪化」であり、全体が neutral だからこのコストを隠す、または compiler 全体が
高速化したと表現することはしない。最終の統合比較なので、増分全部を facts だけの
因果効果ともみなさない。調整担当は必要な情報生成のコストを明記した全体 neutral の
結果を受け入れた。

`target/perf/PR01-frontend-final2/{before,after,before2}/frontend.jsonl` に全 sample・
phase・host・hash、`comparison.txt`／`drift.txt`／`gates.json` に公式判定を保存した。
中央の full triples／ranges と実行性能の計測は `docs/benchmarks.md` を調整担当が管理する。
timing threshold を共有 CI の pass/fail 条件へ追加していない。

最終 compiler の **36 native／WASM LLVM module** は、
`target/perf/PR01-annotation-skip/{ir-before,ir-after}` の SHA380／SHA021 の両出力と
**byte-identical**。測定した control／CPP／computations／allocation fixture、
storage、param_facts、last_use、CPU dispatch、strings と 8 個の facts source を含む。
最終出力は `target/perf/PR01-frontend-final2/ir-final/`。
解析改善後の correctness は Rust **4 unit + 9 integration**、all-target clippy、
param_facts **113 cases + 6 traps**、control **270 cases + traps／cleanup／loop boundary**、
native／WASM × O0／O3 で pass。元の ASan／host allocator・CPU stub・depth・manual の
結果は上記の通りで、今回の doc-only 更新のために再実行したものではない。

Phase 2 の precise region-based capture、aggregate value facts、一般 call effects、
全 owned-buffer provenance、Phase 3 の struct-path TBAA は未実装であり、上記の保守的な
非付与が現在の実際の範囲である。全体の CI／Windows／LLVM17 と公開・完了の判断は
調整担当に残す。source は freeze 済みで、この最終報告更新は文書のみ。
