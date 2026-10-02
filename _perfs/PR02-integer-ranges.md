# PR02: 整数範囲の証明による算術フラグとループ最適化

| 項目 | 内容 |
| --- | --- |
| ID | PR02 |
| 分類 | 実行速度 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | PR01, (F12) |
| 関連 | F12 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（言語の意味、GUIDE D-01〜D-30、crate、WASM import、必要な LLVM の版を変えない。D1〜D9 はすべて既定案） |
| 手本にする既存実装 | 構造から証明した事実を IR へ出す形: `src/llvm_control.rs` の `narrow_range_loop`（`llvm.assume` と、証明を命令の直前に書く一行コメント）。型と符号で命令を選ぶ形: `src/llvm.rs` の `FunctionEmitter::binary` と `checked_integer_arithmetic`。IR の文字列検査: `src/llvm.rs` の test `emits_explicit_tail_loop_and_checked_arithmetic`、`tests/control.rs` の `accepts`。端点の E2E と BigInt 参照: `tests/control.mjs` の `rangeReference` と `inclusive` の case |
| 主な影響ファイル | `src/llvm_control.rs`（`range_loop`, `narrow_range_loop`）, `src/llvm.rs`（`FunctionEmitter::binary`, `array_loop_control`, tail call 引数の遅延 `Argument::Arithmetic`, test module）, `src/ranges.rs`（F12 で新規。PR02 は算術の伝播を足す）, `tests/integer_ranges.rs`（新規）, `tests/fixtures/control/Main.tz`, `tests/control.mjs`, `docs/architecture.md`, `_perfs/README.md` |
| 計測対象 | PX01 形式の `control` suite の `for_mix`, `integer128_mix`, `match_dispatch`, `closure_capture`, `closure_churn`, `float32_mix`, `float64_mix`, `array_sum`, `array_copy` と、`cpp` suite の `format_parse`, `math_intrinsics`, `task_sequence`, `task_sequential`。生成コードは `/tmp/tz-PR02/kernels`（「生成コードの確認」） |

## 目的

C/C++ は符号付き整数の overflow を未定義動作とし、帰納変数の拡張やループの変形に使う。Tsuzuri の整数 `+ - *` は指定幅で
折り返すのが言語の意味なので、同じ仮定は置けない。ただし値の範囲を証明できる箇所では overflow が起きないことが確定する。
その箇所に限って LLVM の `nsw`／`nuw` を付け、折り返しの意味を保ったまま C と同じ最適化の材料を LLVM へ渡す。

- Phase 1: 範囲ループと配列・文字列の走査の増分。証明は生成するループの形だけから決まり、範囲解析を使わない。
- Phase 2: F12 の `RangeFacts` に `+ - *`・整数 cast・ビット積・剰余の区間の伝播を足し、利用者の整数 `+ - *` に、証明できた場合だけ旗を付ける。

実装者は Phase 1、Phase 2 の順に行い、どちらも単独で出荷できる形で終える。旗の効果は生成コードと計測で確かめ、測っていない改善を主張しない。

## 着手条件と停止条件

### 着手条件

- PR01 と F12 が `done` であること。確認: `grep -n "PR01" _perfs/README.md` と `grep -n "F12" _features/README.md` の状態欄。
  F12 は必須とする。PR02 は F12 の `src/ranges.rs` を拡張し（Phase 2）、両チケットとも `range_loop` と `FunctionEmitter` を変えるため、
  F12 の後に始めれば衝突しない。
- PX01 が `done`（PR01 の依存）で、`benchmarks/metrics.mjs` と runner の `--metrics` が使えること。確認: `ls benchmarks/metrics.mjs`。
- 次の「前提とする他チケットのインターフェース」が HEAD に存在すること。確認:
  `grep -n "struct RangeFacts\|pub(crate) fn analyze\|fn index_in_bounds\|fn enter_condition\|NODE_BUDGET" src/ranges.rs` と
  `grep -n "ranges" src/llvm.rs`。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（IR と変更前のコンパイラ）を保存していること。

### 前提とする他チケットのインターフェース

F12（詳細化済み）が `src/ranges.rs` に次を定める。PR02 はこの名前をそのまま使い、Phase 2 で同じファイルに `Interval` と
`RangeFacts::interval` を足す（F12 の D の決定「PR02 は同じファイルに問い合わせ関数を足す」に従う）。F12 の実装で名前が
異なる場合は、F12 の実装に合わせて読み替える。下の意味 1〜3 のどれかが成り立たない場合は停止条件に従う。

```rust
// src/ranges.rs（F12 で新規。F12 の「データ構造」の抜粋）
pub(crate) const NODE_BUDGET: usize = 65_536;
pub(crate) struct RangeFacts { /* below_length, constant, lengths, stable, guards（F12 が定める private field） */ }
pub(crate) fn analyze(module: &CheckedModule, function: &CheckedFunction) -> RangeFacts; // F12 の実装は module も受ける（std の `Array.length` を呼び出しから見分けるため）
impl RangeFacts {
    pub(crate) fn index_in_bounds(&self, array: &TypedExpr, index: &TypedExpr) -> bool;
    pub(crate) fn enter_condition(&mut self, module: &CheckedModule, condition: &TypedExpr) -> usize;
    pub(crate) fn leave_condition(&mut self, pushed: usize);
}
// src/llvm.rs の FunctionEmitter（F12 で新規の field。FunctionEmitter::emit で analyze の結果を入れる）
ranges: crate::ranges::RangeFacts,

// src/ranges.rs（PR02 の Phase 2 で新規）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Interval {
    pub min: i128, // 両端を含む数学的な整数。8〜64-bit の整数型の値だけを表す
    pub max: i128,
}
impl RangeFacts {
    /// 式の値が emitter の現在の位置で取りうる区間。分からなければ None。
    pub(crate) fn interval(&self, expression: &TypedExpr) -> Option<Interval>;
}
```

- 意味 1（健全性）: `interval(e) == Some(r)` なら、その位置で `e` が取りうる値（折り返しの後の値）はすべて `r` に入る。
  可変な局所変数（`let mut`）は、F12 の `stable` に入らない限り None を返す。
- 意味 2（位置）: emitter が式を出す時点で問い合わせてよい。分岐による絞り込み（`if i < values.length` の then 側など）は、
  F12 の `enter_condition`／`leave_condition` で emitter の走査位置に反映済みである。
- 意味 3（源）: F12 の事実（範囲ループの変数の `below_length`・`constant`、`lengths`、`stable`）と、PR02 が足す整数 literal・
  配列・文字列の `.length`（`[0, i64 の最大値]`）を使う。`+ - *` などの伝播は PR02 が足す（設計の「区間の伝播」）。
  PR02 が `RangeFacts` に field を足す場合は `analyze` の worklist の中で埋め、F12 の `NODE_BUDGET` を共有する。

`TypedExpr` は id を持たない（`src/check.rs` の `TypedExpr` は `kind`・`ty`・`span` だけ）。したがって `interval` は式の木を
構造的にたどり、局所変数の事実だけを F12 の表から引く形になる。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- F12 の `interval` が折り返しを考慮しない（型の範囲を越えうる数学的な区間をそのまま返す）、分岐の絞り込みが emitter の
  位置と一致しない、または式ごとに問い合わせる手段がない。
- 設計の規則 R1〜R5・P1〜P7 にない形に旗を付けたくなった。規則の追加は本チケットの改訂として報告する。
- `zext nneg`・`trunc nuw`・`samesign`・`range` 属性など、LLVM 18 以降にしかない構文を出したくなった（D6）。
- 既存テストの期待値を変える必要がある。R1〜R5 の増分の行そのものを照合する期待値と、`docs/architecture.md` の「`nsw`／`nuw` を付けません」の
  記述の更新は除く。
- native と WASM、`-O0` と `-O3` のどれかで結果・トラップ・`live` が変わった。旗を外すと一致するなら証明の誤りである。
- 生成 IR が決定的でなくなった（`accepts` の 2 回の emit が一致しない）。
- `interval` の再帰が 2 MiB の test stack で overflow した、または上限（D7）を上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状と計測（HEAD `f8dc655`）

### 算術の意味と生成

- 整数の `+ - *`、符号反転、ビット演算は指定幅で折り返す（`docs/language.md` の数値の節）。`docs/architecture.md` の
  整数 LLVM の段落は「整数の算術に `nsw`／`nuw` を付けません」と書く。
- `src/llvm.rs` の `FunctionEmitter::binary` は整数の `Add`・`Subtract`・`Multiply` を旗のない `add`・`sub`・`mul` にする。
  除算・剰余は 0 と符号付きの MIN / -1 を検査し（`TrapKind::IntegerDivisionByZero` ほか）、shift 量は `and ty rhs, bits - 1` で mask する。
- tail call の引数では、`call_specialization::binary_operation` が認識した整数の `Add`・`Subtract` だけを `Argument::Arithmetic` として
  latch の直前へ遅らせ、同じく旗のない `add`・`sub` を出す。演算子形と builtin 形（`benchmarks/control/Main.tz` の `Sub.sub remaining 1`）が同じ経路を通る。
- 旗のない `add`・`sub`・`mul` を出す箇所は他にもある: `UnaryOp::Negate` の `sub ty 0, x`、`IntIsPowerOfTwo`、`IntWideningMul`、
  `checked_integer_arithmetic` を使う `pow` 系、`src/llvm_control.rs` の switch の `sub`、`src/llvm_simd.rs` の lane 演算。PR02 はこれらを変えない（D5）。
- `src/llvm.rs` の test `emits_explicit_tail_loop_and_checked_arithmetic` は `@tz.fn.Main.sum` の本体に ` nsw `・` nuw ` がないことを確かめる。
- 値の範囲を解析する仕組みは HEAD にない（`src/ranges.rs` は F12 で新規）。`src/constants.rs` の `binary` は定数を折り返しで評価するが、実行時の範囲は扱わない。
- `src/main.rs` の usage と `src/driver.rs` の診断は外部ツールを「LLVM/Clang 17+」とする。この環境の clang は `Apple clang version 21.0.0`。

### ループの生成

| 経路 | 条件 | 形 | 増分 |
| --- | --- | --- | --- |
| `narrow_range_loop`（`src/llvm_control.rs`） | 64-bit 未満、step が literal の 1、または符号付きで -1 | 端点を `sext`／`zext` で i64 にし、先頭で `icmp sle`／`sge`。本体の前に `trunc`・再拡張・`llvm.assume` | `add i64 current, 1`／`-1` |
| `range_loop` の ±1 経路 | 64-bit 以上（i64・i64u・i128・i128u）、step が 1、または符号付きで -1 | 入口で空を判定（`sle`／`sge`／`ule`）。本体の後の latch で `icmp eq current, last` | latch が偽のときだけ `add ty current, stride` |
| `range_loop` の一般経路 | その他の step | `TrapKind::RangeStepZero` の検査、先頭で範囲の判定 | `llvm.{s,u}add.with.overflow`、overflow なら終了 |
| `array_loop_control`（`src/llvm.rs`） | 配列・文字列の `for … in`（`for_each` から） | 先頭で `icmp ult i64 index, length` | `add i64 index, 1` |

`continue` の飛び先は、±1 経路では latch、`narrow_range_loop` と `array_loop_control` では advance である。どの増分も、
それより前の判定を通った後にだけ実行される。ループ変数は不変の束縛で、増分は SSA の `current`（phi）に対して行う。

### 再現（2026-09-30 に確認）

`/tmp/tz-work-PR02/loops/Main.tz` に次の関数と、`n .. -1 .. 0` で降順に足す `down64`、`values[i * 2 + 1]` を読む `affine` を置いて確かめた。
`check` は成功する。

```tsuzuri
export def up64 :: i64 -> i64
fn up64 n =
    let mut total = 0i64
    for i in 0 .. n do total = total + i
    total

export def index32 :: i32 -> i64
fn index32 n =
    assert (n >= 1 && n <= 1000000)
    let values = new [i64](n as i64, \i -> i * 3)
    let mut total = 0i64
    for i = 0 to n - 1 do total = total + values[i as i64]
    total
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-work-PR02/loops --emit llvm -o /tmp/tz-work-PR02/O0.ll
target/release/tsuzuri build /tmp/tz-work-PR02/loops --emit llvm -O3 -o /tmp/tz-work-PR02/O3.ll
cmp /tmp/tz-work-PR02/O0.ll /tmp/tz-work-PR02/O3.ll && echo same
target/release/tsuzuri build /tmp/tz-work-PR02/loops --emit object -O3 -o /tmp/tz-work-PR02/O3.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn /tmp/tz-work-PR02/O3.o
```

- `--emit llvm` の IR は `-O0` と `-O3` で byte 単位で同じ（`same`）。最適化は clang が行う。
- `up64` の latch と増分（`down64` は `add i64 %v3, 18446744073709551615`、i32 の `for … to` は `add i64 %v8, 1`）:

```llvm
b4:
  %v10 = icmp eq i64 %v3, %v2
  br i1 %v10, label %b3, label %b2
b2:
  %v4 = add i64 %v3, 1
  br label %b1, !llvm.loop !267
```

- native `-O3`（arm64）の `_tz_index32` は、i32 のカウンターを i64 の添字に使っても内側のループが `ldp q…`／`add.2d` でベクトル化され、
  ループ内に境界検査がない（`brk #0x1` は assert と確保失敗の分岐先だけ）。2026-09-29 の確認でも
  `for i in 0i64 .. (values.length - 1) do total = total + values[i]` は `-O3` で境界検査が消え、ベクトル化された。
  単純な形は LLVM だけで最適化できており、PR02 の効果は小さい可能性が高い。
- `-O3` の build は既存のコードでも `loop not unrolled: the optimizer was unable to perform the requested transformation` の warning を出す。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を記録して判断する（PX01 の before／after、GUIDE §14）。

- G1: 証明できた増分・算術にだけ `nsw`／`nuw` が付き、それ以外の IR は byte 単位で変わらない。
- G2: 計測対象の種目で時間が悪化しない。生成コードが変わった種目に限り、改善を実測で報告する。
- G3: コンパイル時間を悪化させない（`interval` の上限、D7）。
- 帰納変数・添字の計算を含む新しい種目（ストライド、逆順、i32 カウンター、入れ子のループ）の追加と、C（符号付き int の添字）・Rust との
  比較は PX02 の範囲とする（D8）。

| 指標 | 単位・統計 | 対象 | 期待 |
| --- | --- | --- | --- |
| M1 旗の数 | 個（固定値） | `/tmp/tz-PR02/kernels` の IR の整数 `add`・`sub`・`mul` | 「生成コードの確認」の表のとおり。表にない命令は旗なし |
| M2 ループの機械語 | 命令列（記述） | M1 の kernels の native `-O3` と wasm32 `-O3` | 符号拡張（`sxtw`、`i64.extend_i32_s`）、ベクトル化（`add.2d`）、ループ内の trap 分岐の有無を before／after で記録する。変化を予測しない |
| M3 時間 | PX01 の時間系 metric（各 runner の標本の中央値と広がり） | 計測対象の control 9 種目、cpp 4 種目 | 悪化なし（`report --baseline` の verdict が `悪化` でない） |
| M4 コンパイル時間 | PX01 の process suite の `check` と build の時間 | PX01 の既定入力 | 変化なし |

## 変えてはいけない意味

- 折り返し: 証明のない整数 `+ - *` は旗なしのまま。旗付きの命令が overflow すると結果は poison になり、それを使う分岐・store・
  添字の判定が未定義になる。証明の誤りはすべて誤コンパイルで、テストで見つからないことがある。規則は設計の R1〜R5・P1〜P7 に限る。
- 範囲ループ: 両端を含む、型の MIN／MAX の端点で折り返して再列挙しない、step 0 のトラップ、空の範囲、`start → step → finish` の
  評価順（`docs/language.md` の `for` の段落）。
- トラップ: 位置・順序・種類（`TrapKind`）を変えない。除算の 0 と MIN / -1 の検査、F12 が除かない境界検査を残す。
- shift の mask、`checked_*`・`saturating_*`・`wrapping_pow` の結果、整数間の cast（等幅はビット保存、幅が変われば trunc か拡張）。
- `-O0`／`-O3`、native／wasm32 の結果の一致と `live == 0`。
- IR の決定性: 同じ入力から同じ IR（旗の有無を含む）。`HashMap` の反復順に依存しない。
- WASM の import を増やさない。LLVM/Clang 17 で読める IR のままにする（D6）。
- 浮動小数点の命令（`fadd` など）には何も付けない。NaN・符号付きゼロ・丸めに触れない。

## 設計

### 証明の規則（Phase 1: ループの形から）

c は増分の直前の SSA の値（phi）、MIN／MAX／UMAX は型の範囲。どの規則も「増分はその前の判定を通った後にだけ実行される」ことに依る。

| 規則 | 場所と命令 | 増分の直前で成り立つこと | 付ける旗 | 付けない旗（反例） |
| --- | --- | --- | --- | --- |
| R1 | `range_loop` ±1 経路、符号付き・昇順 `add ty c, 1` | 入口で `first ≤ last`、latch で `c != last`。帰納法で `first ≤ c < last ≤ MAX` | `nsw` | `nuw`（c = -1 は符号なしで桁あふれ） |
| R2 | 同、符号なし・昇順 `add ty c, 1` | 同じく `c < last ≤ UMAX` | `nuw` | `nsw`（i64u の c = 2^63 − 1） |
| R3 | 同、符号付き・降順 `add ty c, -1`（IR の定数は `18446744073709551615` など） | `MIN ≤ last < c ≤ first` | `nsw` | `nuw`（-1 の加算は c ≠ 0 で符号なしの桁あふれ） |
| R4 | `narrow_range_loop` の `add i64 c, ±1` | 先頭の判定で c は narrow 型の端点を拡張した区間（`[-2^31, 2^32 − 1]` の中） | `nsw`。符号なし型の昇順（端点を `zext`、c ≥ 0）は `nuw nsw` | 符号付き型の `nuw`（c < 0 がある） |
| R5 | `array_loop_control` の `add i64 index, 1` | 先頭の `icmp ult index, length` が真なので `index < length ≤ UMAX` | `nuw` | `nsw`（length が i64 の最大値以下であることを型が保証しない） |

一般経路（`llvm.{s,u}add.with.overflow`）は変えない。overflow の bit が終了条件そのものだからである。
`narrow_range_loop` の `trunc` と `llvm.assume` も変えない（`trunc nuw`／`nsw` は LLVM 19 以降、D6）。

### 区間の伝播（Phase 2: `src/ranges.rs`）

T は結果の型 `Type::Integer(bits, signed)`（bits ≤ 64）。`L`・`R` は左右の区間（T での値）。

| 規則 | 式（`TypedExprKind`） | 結果の区間 |
| --- | --- | --- |
| P1 | `Int(n)` | 点。符号付きで n ≥ 2^(bits−1) なら n − 2^bits（`Int` はビット列を `u128` で持つ） |
| P2 | `Binary(BinaryOp::Add, a, b)` | 下の判定で T 自身の見方の旗（符号付きなら `nsw`、符号なしなら `nuw`）が立つとき `[L.min + R.min, L.max + R.max]`、それ以外は None |
| P3 | `Binary(BinaryOp::Subtract, a, b)` | 同じ条件で `[L.min − R.max, L.max − R.min]` |
| P4 | `Binary(BinaryOp::Multiply, a, b)` | 同じ条件で 4 つの積の最小と最大 |
| P5 | `Cast(a)`（整数から整数） | `L` が行き先の型の範囲に入れば `L`、入らなければ None（等幅の符号変更と trunc はビット保存なので） |
| P6 | `Binary(BinaryOp::BitAnd, a, b)` | 左右の少なくとも一方の最小が 0 以上なら `[0, そのような側の max の最小]` |
| P7 | `Binary(BinaryOp::Remainder, a, b)` | `R.min > 0` のとき。符号なしは `[0, R.max − 1]`。符号付きは `L.min ≥ 0` なら `[0, min(L.max, R.max − 1)]`、それ以外は `[−(R.max − 1), R.max − 1]` |
| — | その他 | F12 の結果（局所変数・`.length` など）。なければ None |

区間の計算はすべて `i128` の `checked_add`・`checked_sub`・`checked_mul` で行い、失敗したら None とする（64-bit 同士の積は i128 を越えうる）。

旗の判定（`arithmetic`（新規））は二つの見方を使う。

- 符号付きの見方 `S(I)`: T が符号付きなら `I`。符号なしなら `I.max ≤ 2^(bits−1) − 1` のとき `I`、それ以外は無し。
- 符号なしの見方 `U(I)`: T が符号なしなら `I`。符号付きなら `I.min ≥ 0` のとき `I`、それ以外は無し。
- `nsw` ⇔ `S(L)`・`S(R)` があり、その演算結果の区間が `[−2^(bits−1), 2^(bits−1) − 1]` に入る。
- `nuw` ⇔ `U(L)`・`U(R)` があり、その演算結果の区間が `[0, 2^bits − 1]` に入る。
- 結果の区間（P2〜P4）は、T が符号付きで `nsw`、または符号なしで `nuw` のときだけ `Some`。

例: `for i in 0 .. 999 do total = total + values[i * 4 + 3]`（i64）。`i` は F12 から `[0, 999]`、`i * 4` は S・U とも `[0, 3996]` で
`mul nuw nsw`、`+ 3` は `[3, 3999]` で `add nuw nsw`。`total + …` は `total` が可変なので None で旗なし。

### データ構造

```rust
// src/ranges.rs（PR02 が足す）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flags { // 新規
    pub nsw: bool,
    pub nuw: bool,
}

impl Flags {
    /// LLVM の printer と同じ順: "", "nsw ", "nuw ", "nuw nsw "（新規）
    pub(crate) fn text(self) -> &'static str;
}

/// 整数の `+ - *` の旗と結果の区間（P2〜P4）。bits ≤ 64 だけ（新規）。
pub(crate) fn arithmetic(
    operator: BinaryOp,
    left: Interval,
    right: Interval,
    bits: u32,
    signed: bool,
) -> (Flags, Option<Interval>);
```

`RangeFacts::interval` の match に P1〜P7 の arm を足し、再帰の深さを数える（D7）。`FunctionEmitter` には
`integer_flags(operator, left, right) -> &'static str`（新規）を足し、`binary` と `Argument::Arithmetic` の両方がこれだけを通して旗を決める（D3）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| Phase 1 生成 | `src/llvm_control.rs` | `range_loop` の ±1 経路 | 増分を `format!("{next} = add {flags}{ty} {current}, {stride}")` にし、`flags` は R1〜R3（`signed` と `minus_one` から） |
| Phase 1 生成 | `src/llvm_control.rs` | `narrow_range_loop` | 増分を R4 の旗にする（`signed` と `descending` から） |
| Phase 1 生成 | `src/llvm.rs` | `array_loop_control` | 増分を `add nuw i64` にする（R5） |
| Phase 1 生成 | `src/llvm_control.rs` | `range_loop` の一般経路、`for_each`、`while_loop` | 変更なし |
| Phase 2 解析 | `src/ranges.rs` | `RangeFacts::interval`、`Flags`（新規）、`arithmetic`（新規） | P1〜P7、見方 S／U、深さの上限 |
| Phase 2 生成 | `src/llvm.rs` | `FunctionEmitter::binary` | `left.ty` が `Type::Integer(bits, _)`（bits ≤ 64）で operator が `Add`・`Subtract`・`Multiply` のとき、`integer_flags` の文字列を命令名の後に挿入する。float、`Char`・`Utf8Char`、i128 は変更なし |
| Phase 2 生成 | `src/llvm.rs` | tail call 引数の遅延（`Argument::Arithmetic` を作る箇所） | `binary_operation` が返した `left`・`right` で `integer_flags` を呼び、同じ旗を付ける |
| Phase 2 生成 | `src/llvm.rs` | `integer_flags`（新規） | `self.ranges.interval(left)`・`interval(right)` から `ranges::arithmetic` を呼ぶ。どちらか None なら `""` |
| 変更なし | `src/llvm.rs`, `src/llvm_simd.rs`, `src/llvm_control.rs` | `UnaryOp::Negate`、`IntIsPowerOfTwo`、`IntWideningMul`、`pow` 系、`checked_integer_arithmetic`、switch の `sub`、SIMD | 対象外（D5） |
| テスト | `tests/integer_ranges.rs`（新規）、`src/llvm.rs` の test module | 「テスト計画」 | IR の旗の有無と決定性 |
| E2E | `tests/fixtures/control/Main.tz`, `tests/control.mjs` | 新しい export と case | 端点と旗の境界の結果を BigInt 参照と比べる |

### 生成 IR

Phase 1 の `up64`（R1）は次の形になる。`down64`（R3）は `add nsw i64 %v3, 18446744073709551615`、i32 の `for … to`（R4）は
`add nsw i64 %v8, 1`、配列の `for … in`（R5）は `add nuw i64 %index, 1` になる。それ以外の行は変わらない。

```llvm
b2:
  %v4 = add nsw i64 %v3, 1
  br label %b1, !llvm.loop !267
```

Phase 2 の例（上の `values[i * 4 + 3]`）の期待形（実装後に有効。未検証）。`!range`・`llvm.assume`・trip count の metadata は出さない（D4）。

```llvm
  %v20 = mul nuw nsw i64 %v19, 4
  %v21 = add nuw nsw i64 %v20, 3
```

### Phase 分割

- Phase 1（実装手順 2〜5）: R1〜R5。`RangeFacts` を使わず、これだけで出荷できる。
- Phase 2（実装手順 6〜10）: P1〜P7 と `binary`・`Argument::Arithmetic` の旗。Phase 1 の IR を変えない。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。手順の「前後比較」は次の関数を使う。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
strip_flags() { sed -E 's/ (add|sub|mul) (nuw nsw|nuw|nsw) / \1 /' "$1"; }
only_flags() { cmp <(strip_flags "$1") <(strip_flags "$2") && echo only-flags; }
```

### 手順 1: ベースラインと変更前のコンパイラ

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「生成コードの確認」の kernels を `/tmp/tz-PR02/kernels/Main.tz` に書き、`check` が通ることを
  確かめる。F12 が `range_loop`・`narrow_range_loop`・`array_loop_control` を変えていれば読み、「ループの生成」の表（判定の後に
  増分があること、`continue` の飛び先）がまだ成り立つことを確かめる。成り立たなければ停止する。
- 確認: 次が成功し、`kernels-before.ll`・`.o`・`.wasm` ができる。最後の grep の出力を完了報告の before として残す。

```sh
cargo build --release --locked
mkdir -p /tmp/tz-PR02 && cp target/release/tsuzuri /tmp/tz-PR02/tsuzuri-before
target/release/tsuzuri check /tmp/tz-PR02/kernels
target/release/tsuzuri build /tmp/tz-PR02/kernels --emit llvm -o /tmp/tz-PR02/kernels-before.ll
target/release/tsuzuri build /tmp/tz-PR02/kernels --emit object -O3 -o /tmp/tz-PR02/kernels-before.o
target/release/tsuzuri build /tmp/tz-PR02/kernels --target wasm32 -O3 -o /tmp/tz-PR02/kernels-before.wasm
grep -cE ' (add|sub|mul) (nuw nsw|nuw|nsw) ' /tmp/tz-PR02/kernels-before.ll
cargo test --locked --lib emits_explicit_tail_loop_and_checked_arithmetic
```

### 手順 2: R1〜R3（`range_loop` の ±1 経路）

- 変更: `src/llvm_control.rs` の `range_loop`、`tests/integer_ranges.rs`（新規）。
- 内容: ±1 経路の増分を `add {flags}{ty}` にする。`flags` は `signed` なら `"nsw "`、符号なしなら `"nuw "`（符号なしは昇順しかない）。
  命令の直前に証明を一行で書く（例: `// The latch proved current != last, so stepping toward last cannot wrap.`）。テストファイルには
  `tests/control.rs` の `accepts` を写し、`llvm::emit` の IR から `define internal … @tz.fn.Main.<name>(` から `\n}` までを切り出す
  `body(ir, name)`（新規）を足す。`range_increments_carry_proven_flags` の R1〜R3 の行だけを書く。
- 確認: `cargo test --locked --test integer_ranges` が `1 passed`。`cargo test --locked --test control --test integers` が成功。

### 手順 3: R4（`narrow_range_loop`）

- 変更: `src/llvm_control.rs` の `narrow_range_loop`、`tests/integer_ranges.rs`。
- 内容: 増分を、符号なし型（`extension == "zext"`）の昇順は `add nuw nsw i64`、それ以外は `add nsw i64` にする。既存のコメント
  「A narrow range's first out-of-range value still fits i64」が証明になるので、旗の理由をその行に足すだけにする。`trunc` と `llvm.assume` は変えない。
- 確認: `cargo test --locked --test integer_ranges` が `1 passed`（R4 の行を足した同じテスト）。`node tests/control.mjs target/debug/tsuzuri` が成功（既存の `inclusive` の i32 端点）。

### 手順 4: R5（`array_loop_control`）

- 変更: `src/llvm.rs` の `array_loop_control`、`tests/integer_ranges.rs`。
- 内容: 増分を `add nuw i64 {index}, 1` にする。`nsw` は付けない（R5）。
- 確認: `cargo test --locked --test integer_ranges --test arrays --test strings` が成功。

### 手順 5: Phase 1 の E2E・生成コード・計測

- 変更: `tests/fixtures/control/Main.tz`、`tests/control.mjs`（「E2E」の Phase 1 の export と case）。
- 内容: 「テスト計画」の共通コマンドをすべて実行する。「生成コードの確認」を `phase1` として記録し、「計測手順」の
  `PR02-before` と `PR02-phase1` を測る。差がなくても Phase 1 は出荷してよい（落とし穴）。
- 確認: 共通コマンドがすべて成功。`only_flags /tmp/tz-PR02/kernels-before.ll /tmp/tz-PR02/kernels-phase1.ll` が `only-flags`。

### 手順 6: 旗の判定（純粋関数）

- 変更: `src/ranges.rs` の `Flags`、`arithmetic`、`cast`・`bit_and`・`remainder`（いずれも新規。P5〜P7 を `Interval` だけで計算する）と test module。
- 内容: 設計の見方 S／U と `checked_*` で実装する。bits > 64 は呼ばない（`debug_assert!`）。`2^bits` は `1i128 << bits` で計算し、bits ≤ 64 なので溢れない。
  この段では emitter から呼ばない。
- 確認: `cargo test --locked --lib ranges::tests` が F12 のテストに加えて 2 件多く成功（「Rust テスト」の総当たり 2 件）。

### 手順 7: `RangeFacts::interval` の P1〜P7

- 変更: `src/ranges.rs` の `RangeFacts::interval`。
- 内容: `TypedExprKind::Int`・`Binary(BinaryOp::Add | Subtract | Multiply | BitAnd | Remainder, ..)`・`Cast` の arm を足し、手順 6 の関数へ渡す。
  `expression.ty` が `Type::Integer(bits, signed)` で bits ≤ 64 でなければ最初に None。再帰は深さを数え、深さ 32 を超えたら None（D7）。
  F12 の arm（局所変数・`.length`）は変えない。
- 確認: `cargo test --locked --lib ranges::tests` と F12 の Rust テストが成功。`cargo test --locked` が成功（まだ旗は出ないので IR は不変）。

### 手順 8: `integer_flags` と `binary`

- 変更: `src/llvm.rs` の `integer_flags`（新規）と `FunctionEmitter::binary`、`tests/integer_ranges.rs`。
- 内容: `binary` の整数の `Add`・`Subtract`・`Multiply` だけ、命令名と型の間に `integer_flags` の文字列を挿入する。区間は
  オペランドを `self.expression` で出す前に問い合わせても後でもよいが、評価順と生成する命令の順は変えない。
- 確認: `cargo test --locked --test integer_ranges` が `4 passed`（`proven_arithmetic_carries_flags`、`unproven_arithmetic_has_no_flags`、
  `range_flags_are_deterministic_on_both_targets` を足す）。`cargo test --locked --lib emits_explicit_tail_loop_and_checked_arithmetic` が `1 passed`。

### 手順 9: tail call 引数の遅延

- 変更: `src/llvm.rs` の `Argument::Arithmetic` を作る箇所、`tests/integer_ranges.rs`。
- 内容: `call_specialization::binary_operation` が返した `left`・`right` で `integer_flags` を呼び、`format!("{opcode} {flags}{ty} …")` にする。
  区間は遅延した位置でなく、オペランドを読む位置（今の `self.expression(left)` の直前）で問い合わせる。値はその時点のものだからである。
- 確認: `cargo test --locked --test integer_ranges` が `5 passed`（`operator_and_builtin_forms_share_flags`）。`cargo test --locked --test tail_recursion --test control` が成功。

### 手順 10: Phase 2 の E2E と全体の確認

- 変更: `tests/fixtures/control/Main.tz`、`tests/control.mjs`（「E2E」の Phase 2 の export と case）。
- 内容: 「テスト計画」の共通コマンドをすべて実行する。
- 確認: すべて成功。`only_flags /tmp/tz-PR02/kernels-phase1.ll /tmp/tz-PR02/kernels-after.ll` が `only-flags`。

### 手順 11: 生成コード、計測、文書

- 変更: `docs/architecture.md`、`_perfs/README.md`（「ドキュメント」）。
- 内容: 「生成コードの確認」を `after` として記録し、「計測手順」の `PR02-after` と `PR02-before2` を測る。
- 確認: `report` の表、2 つの命令列の記録、`grep -n "nsw" docs/architecture.md` が新しい規則の文を示す。

## 計測手順

PX01 の「計測手順」の「環境と条件」に従う（AC 電源、端末を共有する他のエージェントの作業を止める、load average）。標本は各 runner の
既存の warm-up と 12 標本（≥ 9）で、中央値と広がりを PX01 形式で `target/perf/<run_id>/` に残す。run は `PR02-before`
（`/tmp/tz-PR02/tsuzuri-before`）、`PR02-phase1`、`PR02-after`、`PR02-before2`（再び before）の順に、suite と旗をそろえて測る。

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
  node24 benchmarks/metrics.mjs process "$2" --out "$RUN"
}
measure PR02-before /tmp/tz-PR02/tsuzuri-before   # 手順 5 の前に一度だけ
measure PR02-phase1 target/release/tsuzuri        # 手順 5
measure PR02-after target/release/tsuzuri         # 手順 11
measure PR02-before2 /tmp/tz-PR02/tsuzuri-before  # 手順 11
node24 benchmarks/metrics.mjs report target/perf/PR02-before2 --baseline target/perf/PR02-before
node24 benchmarks/metrics.mjs report target/perf/PR02-phase1 --baseline target/perf/PR02-before
node24 benchmarks/metrics.mjs report target/perf/PR02-after --baseline target/perf/PR02-before
```

- 判定: 計測対象の 13 種目の行だけを読む。`before2` 対 `before` で `改善`・`悪化` が出た metric は「判定できない」と報告する。
  `after` 対 `before` の verdict をそのまま完了報告に貼り、生成コードが変わっていない種目の改善を PR02 の効果と呼ばない。
- `--cpu native` は測らない（既定の `generic` だけ）。`--quick` は動作確認だけに使い、計測に使わない。

## 生成コードの確認

### kernels

`/tmp/tz-PR02/kernels/Main.tz` に次の export を置く。いずれも既存の構文だけで書き、手順 1 で `check` が通ることを確かめる。

| 関数 | ループ | 本体 | Phase 1 の後の旗 | Phase 2 で足される旗 |
| --- | --- | --- | --- | --- |
| `up64`、`down64`、`up32`、`index32` | 「再現」と同じ | 同じ | R1、R3、R4、R4 の増分 1 つずつ | なし（`total` は可変） |
| `stride_index :: i64 -> i64` | `for i in 0 .. 999` | `values` は長さ 4000、`total = total + values[i * 4 + 3]` | R1 | `mul nuw nsw`、`add nuw nsw` が 1 つずつ |
| `reverse_index :: i64 -> i64` | `for i in 999 .. -1 .. 0` | 長さ 1000 の `values[i]` を足す | R3 | なし |
| `nested :: i64 -> i64` | `for i in 0 .. 63` の中に `for j in 0 .. 63` | 長さ 4096 の `values[i * 64 + j]` を足す | R1 が 2 つ | `mul nuw nsw`、`add nuw nsw` が 1 つずつ |
| `sum_each :: i64 -> i64` | `for value in values` | 長さ `n` の配列の和 | R5 | なし |

引数は seed として `values` の初期化（`new [i64](…, \i -> i ^ seed)`）にだけ使い、ループの範囲は定数にする。配列の初期化が出す命令の旗は
予測しない（before／after で記録するだけ）。

### コマンドと見るもの

`<tag>` は `phase1` か `after`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
K=/tmp/tz-PR02/kernels; O=/tmp/tz-PR02/kernels-<tag>
target/release/tsuzuri build $K --emit llvm -o $O.ll
grep -nE ' (add|sub|mul) (nuw nsw|nuw|nsw) ' $O.ll
grep -cE 'nneg|samesign|trunc (nuw|nsw)|!range' $O.ll
clang -O3 -S -emit-llvm -Wno-override-module $O.ll -o $O.opt.ll
target/release/tsuzuri build $K --emit object -O3 -o $O.o
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn $O.o > $O.s
target/release/tsuzuri build $K --target wasm32 -O3 -o $O.wasm
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d $O.wasm > $O.wasm.txt
```

- IR: 旗の行が上の表と一致する。`nneg` などの grep は `0`（D4、D6）。`only_flags` が `only-flags`。
- 最適化後の IR（`$O.opt.ll`）: 各 `@tz_<関数>` のループに残る `sext`、`vector.body` の有無。
- native（`$O.s`、`_tz_<関数>`）: 後方分岐までのループ内の `sxtw`、`add.2d`／`ldp q`、`brk #0x1` への分岐の有無と命令数。
- wasm32（`$O.wasm.txt`）: ループ内の `i64.extend_i32_s`、`unreachable` への分岐の有無と命令数。
- before（手順 1）・phase1・after の 3 つを表にして完了報告に載せる。変化がない場合も「変化なし」と書く。

## テスト計画

### 共通コマンド（手順 5・10）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/control.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/e2e.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
npx --yes --package=node@24 node tests/integer_intrinsics.mjs target/release/tsuzuri
node tests/features.mjs target/release/tsuzuri
```

`tests/control.mjs`、`tests/numeric_casts.mjs`、`tests/integer_intrinsics.mjs` は native と wasm32 を `-O0`／`-O3` で実行する。
旗が効くのは `-O3` なので、`-O0` だけの成功を証拠にしない。

### Rust テスト

`src/ranges.rs` の test module（手順 6）:

- `arithmetic_matches_brute_force_for_8_bit_types`: i8 と i8u について、端点を境界値の集合（i8 は -128, -127, -2, -1, 0, 1, 2, 63, 64, 126, 127、
  i8u は 0, 1, 2, 63, 64, 127, 128, 254, 255）から選んだすべての区間の組と `Add`・`Subtract`・`Multiply` で、区間の全要素を両方の解釈で
  実際に計算した参照（`i128` の素朴な二重ループ）と、`nsw`・`nuw` が一致すること、`Some` の区間がすべての結果を含むこと。
- `derived_intervals_contain_every_value_for_8_bit_types`: 同じ区間の組で `cast`（i8・i8u・i16 の間）、`bit_and`、`remainder`（除数が正の区間だけ）が
  返す `Some` の区間が、実際の演算（Rust の `as`、`&`、`%`）の全結果を含むこと。

`tests/integer_ranges.rs`（新規。`accepts` と `body` を持つ）:

- `range_increments_carry_proven_flags`（手順 2〜4）: 各関数の本体について、`for i in 0 .. n`（i64）は `add nsw i64` が 1 つで `nuw` なし。
  `for i in 0i64u .. n`（i64u）は `add nuw i64` が 1 つで `nsw` なし。`for i in n .. -1 .. 0` は `add nsw i64 %` と `, 18446744073709551615`。
  i128 の昇順は `add nsw i128`。`for i = 0 to n` は `add nsw i64`、`for i = n downto 0` は `add nsw i64` と `, -1`。
  `for i in 0i32u .. n`（i32u）は `add nuw nsw i64`。`for value in values` は `add nuw i64`。`for i in 0 .. 2 .. n` は
  `@llvm.sadd.with.overflow.i64` を含み、本体に ` nsw `・` nuw ` がない。各 source は書く前に `target/release/tsuzuri check` で受理を確かめる。
- `proven_arithmetic_carries_flags`（手順 8）: kernels の `stride_index` と同じ形で `mul nuw nsw i64` と `add nuw nsw i64` が 1 つずつ。
  `total` への加算は旗なしの `add i64`。
- `unproven_arithmetic_has_no_flags`（手順 8）: 引数同士の `a + b`・`a * 3`、`for i in 0 .. 9223372036854775807` の中の `i + 1`、
  `for i in 0i8 .. 127i8` の中の `i * 2`、`let mut` の変数の `x + 1` に旗がない。`for i in 0i64u .. 10i64u` の中の `i - 1` は
  `sub nsw i64` で `nuw` がない（S の見方では `[-1, 9]`、U の見方では 0 - 1 が桁あふれ）。
- `range_flags_are_deterministic_on_both_targets`（手順 8）: 上の source を `llvm::emit_target(&module, llvm::Entry::Library, true)` でも出し、
  旗付きの行の集合が native と同じ。2 回の emit が byte 単位で一致。
- `operator_and_builtin_forms_share_flags`（手順 9）: `i + 1` と `Add.add i 1`、tail call の `n - 1` と `Sub.sub n 1` だけが異なる
  関数の組で、SSA 名を除いた `add`・`sub` の行の列が一致する（旗の有無を予測せず、同じであることだけを見る）。

### E2E

`tests/fixtures/control/Main.tz` に export を足し、`tests/control.mjs` の `cases` に足す。期待値は harness の中で BigInt から計算し
（既存の `rangeReference` と `wrap`、`BigInt.asIntN`／`asUintN`）、コンパイラの出力から写さない。

| export（新規） | 内容 | case |
| --- | --- | --- |
| `range_edges :: i64 -> i64 -> bool -> i64` | `inclusive` と同じ形の i64 版。偶数は `continue` で飛ばし、奇数だけ折り返しで足す | MAX−3..MAX 昇順、MIN+3..MIN 降順、MAX..MAX、MIN..MIN、空（2 方向） |
| `range_edges_u64 :: i64u -> i64u -> i64u` | i64u の昇順の和 | UMAX−3..UMAX、UMAX..UMAX、2^63−2..2^63+1、空 |
| `range_edges_u32 :: i32u -> i32u -> i64` | i32u の昇順の和（R4 の `nuw nsw`） | 4294967292..4294967295、0..3、空 |
| `range_edges_i128 :: i64` | i128 の MAX で終わる 4 要素と MIN で終わる 4 要素の降順の個数 | 8 |
| `each_break :: i64 -> i64` | 長さ n の配列を `for value in` で足し、値が 100 で `break` | n = 0, 1, 4096 |
| `affine_sum :: i64 -> i64`（Phase 2） | seed に `stride_index` と同じ和を折り返しで足す | seed = 0、MAX − 10（`total` の折り返し） |
| `wrap_edges :: i64`（Phase 2） | `for i in (MAX − 4) .. MAX` で `i + 10` を足す（折り返すので旗が付いてはならない） | 1 case |
| `narrow_wrap :: i64`（Phase 2） | `for i in 120i8 .. 127i8` で `(i * 2) as i64` を足す | 1 case |
| `unsigned_sub :: i64u`（Phase 2） | `for i in 0i64u .. 10i64u` で `i - 1` を足す | 1 case |

既存の harness と同じく native と wasm32 の `-O0`／`-O3` で実行し、トラップがないことと、確保を使う case では harness が確かめる `live` が 0 のままであることを見る。

### 既存テストへの影響

- `src/llvm.rs` の `emits_explicit_tail_loop_and_checked_arithmetic` は変えない。`sum` の `n - 1`・`acc + n` は区間が分からず、範囲ループもないので旗は出ない。
  失敗したら証明か F12 の絞り込みの誤りであり、停止する。
- 手順 2 の前に `grep -rnE "add (i64|i32)|nsw|nuw" tests/*.rs tests/*.mjs` で IR の文字列を照合する期待値を列挙する。R1〜R5 の増分の行そのものを
  照合する期待値だけは旗付きに更新してよい。それ以外はなし。

### 性能

「計測手順」と「生成コードの確認」に従う。共有 CI に速度の合否条件を足さない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/architecture.md` | 整数 LLVM の段落（「整数の算術に `nsw`／`nuw` を付けません」） | 「証明できた場合だけ付ける」に書き換え、R1〜R5 と P1〜P7 の要点、LLVM 17 の構文だけを使うこと、`!range` を出さないことを書く |
| `docs/language.md` | なし | 折り返しの意味は変わらないので変更しない |
| `docs/benchmarks.md` | なし | PX01 D2 に従い、人間が依頼した場合だけ要約を載せる |
| `_perfs/README.md` | `## 一覧` | PR02 の状態を `done` |

## 受け入れ条件

- [ ] R1〜R5 の増分にだけ旗が付き、`only_flags` が before と phase1 の kernels で `only-flags` を出す。
- [ ] P1〜P7 で証明できた整数 `+ - *` にだけ旗が付き、`unproven_arithmetic_has_no_flags` と brute-force の 2 テストが成功する。
- [ ] 演算子形と builtin 形が同じ IR になる（`operator_and_builtin_forms_share_flags`）。
- [ ] 「テスト計画」の共通コマンドがすべて成功し、E2E の新しい case が native／wasm32 × `-O0`／`-O3` で参照と一致する。
- [ ] 生成 IR に LLVM 18 以降の構文と `!range` がなく、IR が決定的である。
- [ ] 生成コードの before／phase1／after と計測の `report` の表を完了報告に載せ、測っていない改善を主張しない。
- [ ] 「ドキュメント」の更新。
- [ ] GUIDE §10 の完了の定義と §14 の性能チケットの共通手順を満たす。

## 落とし穴

- 誤った旗はテストで現れないことがある。poison は使われ方次第で正しい値のように振る舞い、`-O0` ではほぼ影響しない。
  規則は brute-force の単体テストと表 R1〜R5 の反例で確かめ、E2E の成功だけを証拠にしない。
- R1〜R3 は「latch の `icmp eq` の後に増分がある」形に依る。増分を判定の前へ動かす変更をすると、`last == MAX` で証明が崩れる。
  増分の直前の一行コメントで前提を書き、形を変えるときは旗を外す。
- 符号付きと符号なしの取り違え（符号なしに `nsw`）は値の検査では見つかりにくい。IR テストで旗の組を完全に照合する。
- 降順の -1 は i64 で `18446744073709551615` と印字され、narrow 経路では `-1` と印字される。テストの文字列を取り違えない。
- `TypedExprKind::Int` はビット列なので、符号付きの負の literal を正の巨大な値として扱うと P1 が誤る（brute-force テストでは見つからないので
  `unproven_arithmetic_has_no_flags` に負の literal を含める）。
- `2^bits` を bits = 128 で計算すると debug build が panic する。i128・`Char`・`Utf8Char` は `Type::Integer(bits ≤ 64, _)` の判定で最初に除く。
- `--emit llvm -O3` は `-O0` と同じ IR を出す。最適化後の IR は `clang -O3 -S -emit-llvm` で得る。
- 「再現」のとおり LLVM は単純な形を既に最適化する。差が出ないことは失敗ではない。差を出すために `llvm.assume` や `!range` を足したり、
  規則を広げたりしない（停止条件）。
- 端末は他のエージェントと共有され、計測が乱れる。`before2` で揺れを確かめ、出力はファイルへ redirect してから読む。
- Node 20 は重い BigInt の suite で V8 が abort することがある。`tests/integer_intrinsics.mjs` と計測は Node 24 で実行する。

## 対象外

- 利用者が overflow しないことを宣言する注釈、未定義動作を許す演算。
- 境界検査の除去（F12）。
- `!range`・`llvm.assume`・trip count の metadata（D4）、`zext nneg`・`trunc nuw`／`nsw`・`samesign`（D6）。
- 除算・剰余の簡約（D9）、関数をまたぐ範囲の伝播（D1）。
- i128 の Phase 2、符号反転、builtin の本体、SIMD の lane 演算（D5）。
- `src/runtime/*.ll` と `src/llvm.rs` に文字列で埋め込んだ runtime 関数のループ（`%next = add i64 %i, 1` など）。
- 新しい種目の追加と C・Rust との比較（D8、PX02）。

## 決定事項

### D1: 解析の範囲

- 決定: 関数内（F12 の `RangeFacts::compute` の単位）に限る。関数をまたぐ区間の伝播は行わない。
- 理由: 旧版の既定案。特殊化後の関数は inline されれば LLVM が呼び出し元の事実を使える。関数をまたぐ解析は計算量と決定性の管理が要る。
- 状態: 既定案（実装者はこの案に従う）

### D2: Phase 1 の規則

- 決定: R1〜R5 だけ。`range_loop` の一般経路、`narrow_range_loop` の `trunc` と `llvm.assume` は変えない。
- 理由: 証明がループの形だけで閉じ、解析を要しない。一般経路は overflow の bit が終了条件で、旗を付ける命令がない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 旗の決定を一か所にする

- 決定: 利用者の整数 `+ - *` の旗は `integer_flags` だけで決め、`binary` と `Argument::Arithmetic` の両方が呼ぶ。
- 理由: AGENTS.md の「同値の builtin 形と演算子形が別の性能経路を選ばない」。tail call の遅延だけ旗がないと形で性能が変わる。
- 状態: 既定案（実装者はこの案に従う）

### D4: `!range`・`llvm.assume`・trip count

- 決定: 出さない。旗（`nsw`／`nuw`）だけを出す。
- 理由: 局所変数は alloca を通るので、load に付けた `!range` は mem2reg で失われる。`llvm.assume` は use を増やし最適化を妨げうる。
  trip count は LLVM が終了条件から計算する。旧版の「範囲の事実を付ける」は旗だけに絞る。
- 状態: 既定案（実装者はこの案に従う）

### D5: 対象の型と命令

- 決定: Phase 2 は 8〜64-bit の `Type::Integer` の `add`・`sub`・`mul` だけ。Phase 1 は型の幅に関係なく R1〜R5（i128 を含む）。
  符号反転、`IntIsPowerOfTwo`・`IntWideningMul`・`pow` 系など builtin の本体、switch の `sub`、SIMD は変えない。
- 理由: `Interval` は i128 で、128-bit の型の範囲とその積を表せない。builtin の本体は引数 `%arg0` の区間を持たない。
- 状態: 既定案（実装者はこの案に従う）

### D6: LLVM 17 で読める構文だけを使う

- 決定: `add`・`sub`・`mul` の `nuw`／`nsw` だけを使い、順は LLVM の printer と同じ `nuw nsw`。旧版 Phase 1 の `zext nneg` は使わない。
- 理由: `src/main.rs` と `src/driver.rs` は LLVM/Clang 17+ を要件とし、`zext nneg` は LLVM 18、`trunc nuw`／`nsw` は 19 からである。
  「再現」の `index32` では LLVM が i32 のカウンターからの拡張を既に処理している。
- 見直し提案: 必要な LLVM の版が 18 以上に上がったら、P5 の非負の拡張を `zext nneg` にする案を計測付きで再検討する。
- 状態: 既定案（実装者はこの案に従う）

### D7: 資源上限

- 決定: `interval` の再帰は深さ 32 まで。超えたら None を返し、診断は出さない。
- 理由: D-19 の資源上限の考え方（超過時は事実を付けないだけ）。`binary` は両辺を問い合わせるので、深い式でも仕事量は
  式の大きさの 32 倍以下で、debug build の 2 MiB stack を使い切らない。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計測の種目

- 決定: 既存の control 9 種目と cpp 4 種目で測り、kernels は生成コードの確認にだけ使う。ストライド・逆順・i32 カウンター・
  入れ子の種目の追加と C・Rust との比較は PX02 が行う。PX02 がそれらを追加済みなら計測対象に加える。
- 理由: runner は種目名の一覧と C・Rust の参照実装を持ち（`benchmarks/run-control.mjs` の `assert.deepEqual`）、種目の追加は PX02 の管理下にある。
- 状態: 既定案（実装者はこの案に従う）

### D9: 除算・剰余の簡約

- 決定: 行わない。`sdiv`／`srem` を `udiv`／`urem` に替える処理を足さない。
- 理由: LLVM の InstCombine は両辺が非負と分かれば同じ変換を行い、旗はその材料になる。除算の命令を変えなければ 0 と MIN / -1 の
  検査の位置も変わらない。
- 状態: 既定案（実装者はこの案に従う）
