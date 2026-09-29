# PM07: 複製の除去とバッファ再利用の一般化

| 項目 | 内容 |
| --- | --- |
| ID | PM07 |
| 分類 | メモリ |
| 優先度 | P1 |
| 規模 | M |
| 依存 | PX01, (A15) |
| 関連 | A15, C01, PM06 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1 の D1〜D9 はすべて既定案。Phase 2 は人間が依頼した場合だけ着手する） |
| 手本にする既存実装 | 単一使用の移動: `src/llvm.rs` の `FunctionEmitter::clones_on_take`・`FunctionEmitter::read_place` と `src/call_specialization.rs` の `single_use_locals`。バッファの伸長: `src/llvm_bulk.rs` の Vec の伸長（`@tz.realloc` を呼んで `make_vector` で組み直す箇所）。文字列ランタイム: `src/runtime/string.ll` の `@tz.string.concat`・`@tz.string.allocate`。確保回数の E2E: `tests/primitives.mjs` の `storage` 表と `tests/fixtures/storage/Storage.tz` の `transferred_array_update`・`copied_array_update`。IR の確保の検査: `tests/consuming_update.rs` の `update_builtins_do_not_clone_their_input_buffers`、`tests/call_specialization.rs` の `only_unobserved_single_use_copies_are_transferred` |
| 主な影響ファイル | `src/call_specialization.rs`, `src/llvm.rs`, `src/llvm_frame.rs`（`take_operand` の frame を読むだけ）, `src/runtime/string.ll`, `src/runtime/utf8string.ll`, `src/runtime/heap-native.ll`・`src/runtime/heap-wasm.ll`（`tz.realloc` の契約を読むだけ。変更しない）, `src/ownership.rs`・`src/copies.rs`（A15 が done の場合だけ。`CopyRead::local` と除外規則）, `tests/call_specialization.rs`, `tests/consuming_update.rs`, `tests/copy_cost.rs`（A15 が done の場合だけ）, `tests/primitives.mjs`, `tests/fixtures/storage/Storage.tz`, `docs/architecture.md`, `docs/language.md`, `docs/benchmarks.md` |
| 計測対象 | 改善を見る種目: cpp suite の `utf16_scan`・`utf16_compare`・`utf16_validate`（`benchmarks/cpp/Kernels.tz` の `make_text` が `text = copy + text` で連結する）。退行を見る対照: control suite の `array_copy`（`let copied = values` の後も `values` を読むので複製が残る）・`record_pipeline`。PX01 の tracked 実行の `calls`・`bytes`。E2E の確保回数（`tests/primitives.mjs` の `storage` 表） |

## 目的

Copy の配列・リスト・関数値の不要な複製と、所有値の更新でのバッファの作り直しを減らす。
所有権の情報から「元の値はもう読まれない」と証明できる位置では、複製の代わりに移動し、バッファをその場で再利用する。
言語の意味（Copy の独立性、評価順序、別名、drop の時点と回数、トラップ）は一切変えない。

Phase 1 は次の 2 点に限る。実装者は Phase 1 だけを実装し、Phase 2 は人間が求めた場合だけ着手する。

1. `clones_on_take` の移動を、単一使用のローカル全体から「単一使用のローカルを根とする `Field` の連鎖」へ広げる（証明規則 R2）。
   C01 の消費的な更新（`Array.set holder.values ..`）も同じ規則で field の入力バッファを再利用するようになる。
2. 文字列の連結 `+` で、左辺がヒープの所有バッファなら `tz.realloc` で伸ばして右辺だけを書き込む（`@tz.string.append`（新規））。

## 着手条件と停止条件

### 着手条件

- PX01 が `_perfs/README.md` の「一覧」の状態欄で done であり、`benchmarks/metrics.mjs` と `benchmarks/tracked_alloc.c` がある。
  確認: `grep -n "PX01" _perfs/README.md && ls benchmarks/metrics.mjs benchmarks/tracked_alloc.c`。
- A15 は開始条件にしない（依存欄の括弧付き）。`src/copies.rs` があれば A15 は done とみなし、実装手順 8 を行う。なければ手順 8 を飛ばし、
  完了報告に「A15 未完のため copies の除外規則は未更新」と書く。
- B07 は開始条件にしない。B07 が done なら、テスト計画の B07 ガード（Drop 型の field は移動のまま）を足す。
- GUIDE §2.3 の基準コマンドが成功し、GUIDE §14 と PX01 の「before／after」手順 1 のとおり変更前のコンパイラを
  `/tmp/tz-PM07/tsuzuri-before` に残していること（実装手順 1）。

### 前提とする他チケットのインターフェース

- PX01（依存）: `node24 benchmarks/run-<suite>.mjs <compiler> --metrics <dir>` が `<dir>/<suite>.jsonl` を書く。tracked 実行は種目ごとに
  `calls`・`bytes` を出し、結果が参照と違うか `live != 0` なら終了コード 1。比較は `node24 benchmarks/metrics.mjs report <after> --baseline <before>`。
- A15（任意）: `tsuzuri::copies::sites(&CheckedModule) -> Vec<CopySite>`。`CopySite { function, span, ty, kind: CopyKind, cost: CopyCost }`、
  `CopyKind::{Local, Field, Element, Tail, Payload, Dereference, Temporary}`。所有権検査は `Checker::read_places` で
  `CopyRead { span, ty, kind, local: Option<usize> }` を記録し、`sites` は `local` が `single_use_locals` にある read を除く（A15 D4）。
  debug build は「生成した暗黙の複製はすべて `sites` にある」ことを `assert!` する（A15 D5）。A15 は「PM07 が省略を広げる場合は同じ変更で
  除外規則を広げ、`tests/copy_cost.rs` の『PM07 で変わる』と注記した期待値を更新する」と定めている。
- B07（任意）: Drop 型は Copy でない。Drop 型を field・payload・要素に持つ型も Copy でない（B07 仕様「型規則」2）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- R2 の健全性に、所有権検査の結果（`live` 集合、loan）や新しい解析が要ると分かった。それは Phase 2 の範囲である。
- `tests/primitives.mjs` で `live != 0`、`tracked_free` の magic の `assert`、二重解放、use-after-free が出た。
- 手順 6 の IR で、`benchmarks/cpp/Kernels.tz` の `make_text` と同じ形の連結に `@tz.string.append` が出ない（左辺の `take_operand` が frame を返す）。
  frame を heap へ移す変更で解決しない。
- WASM の `tz.realloc` の上限（16777184 bytes）と `tz.alloc` の上限が異なり、変更前に成功した連結が trap する（またはその逆）ことが分かった。
- 「既存テストへの影響」に挙げた以外の既存の期待値（IR、確保回数、診断）を変える必要がある。
- A15 の debug 照合（A15 D5）が失敗する。
- `unsafe`、新しい crate、既定の WASM import、`@tz.string.append`・`@tz.utf8string.append`（新規）以外のランタイム記号が必要になった。
- field の取得も連結も含まないプログラムの IR が、`append` の定義の追加（D5）以外で変わった（手順 1 で保存した IR と比べる）。
- after の run で計測対象のどれかが広がりを超えて遅くなった。調整を重ねず、数値を添えて報告する。

## 現状と計測（HEAD `f8dc655`）

### 複製と移動の経路

- `src/llvm.rs` の `FunctionEmitter::read_place(expression, take, relocate)`: `take` かつ `needs_drop` の値について、
  `clones_on_take(expression)` が true なら `clone_value` の結果を返す。false なら slot へ `store <ty> zeroinitializer` を書いて移動し、
  `relocate` なら `frame_of_place` と `relocate` でスタックの frame から heap へ移す。
- `FunctionEmitter::clones_on_take`: `TypedExprKind::Local(id)` で、`id` が `self.single_use` にあり `self.borrowed_locals` にない場合だけ
  「最後の使用」とし、`is_copy && !last_use` を返す。`Field` など他の place は、根のローカルが単一使用でも常に複製する（GUIDE §11.1 の既知の注意点）。
- `self.single_use` は `call_specialization::single_use_locals(&self.function.body)`。`TypedExprKind::Local` の出現を数え、
  `While`・`ForRange`・`ForEach` の子を数え終えた時点で、それまでに数えたすべてのローカルの回数を 2 以上にする（ループ内の使用は単一使用にならない）。
- `self.borrowed_locals` は引数・`ref` 束縛・pattern の別名束縛で入る（`src/llvm.rs` と `src/llvm_control.rs` の `borrowed_locals.insert`）。
- `clone_value` の他の呼び出し元（`src/llvm_bulk.rs` の `clone_vector` など、`src/llvm_parallel.rs`、`src/llvm_recursive.rs`、
  `src/llvm.rs` の要素の読み出し）は明示的な複製・並列 kernel・再帰型の複製で、本チケットでは変えない。
- 所有権検査（`src/ownership.rs`）: `Checker::read_places` は `usage == Use::Consume && !is_copy` のときだけ移動として扱う。Copy の読み出しは
  記録しない（A15 が `CopyRead` を足す）。`Checker::eval_composed` は block の後続の使用を `count_uses` で数えて `live` を作る（Phase 2 の材料）。
- C01 の消費的な更新（`Array.set`・`Array.update`・`Array.swap`・`List.cons`・`List.tail`）は入力を `read_place(.., take = true, ..)` で取る。
  入力が単一使用のローカル全体なら元のバッファを書き換え、field なら複製してから書き換える。

### 文字列の連結

- `src/llvm.rs` の `FunctionEmitter::binary`: 文字列の `Add` は両辺を `take_operand`（`src/llvm_frame.rs`）で取り、
  `call {ty} @{runtime}.concat({ty} {lhs}, {ty} {rhs})` を出す（`runtime` は `tz.string` か `tz.utf8string`）。連結の後で両辺を解放する。
- `take_operand` は place なら `(値, frame_of_place の frame)`、place でなければ `frame_value` の結果を返す。frame が空なら値はスタックの frame にない。
- `@tz.string.concat`（`src/runtime/string.ll`）: `%length = add i64 %an, %bn` の溢れを `icmp ult` で検査して `@llvm.trap`、
  `@tz.string.allocate(i64 %length)` で確保し、`@tz.string.copy` を 2 回呼ぶ。`@tz.utf8string.concat`（`src/runtime/utf8string.ll`）も同じ形。
- 文字列リテラルは `FunctionEmitter::string_constant` が出す `private unnamed_addr constant` で、値として使うと frame に置かれる。

### Vec の伸長とバッファの再確保

- `src/llvm_bulk.rs` の Vec の伸長: 容量 0 なら 4、足りなければ倍増を繰り返す（`4611686018427387903` を超える倍増は `TrapKind::AllocationSize`）。
  `call ptr @tz.realloc(ptr {data}, i64 {old_size}, i64 {new_size})` の後に `make_vector` で組み直す。
- native の `@tz.realloc`（`src/runtime/heap-native.ll`）は `new_size == 0` で解放、それ以外は libc の `realloc`（失敗は trap）。
- WASM の `@tz.realloc`（`src/runtime/heap-wasm.ll`）は `new_size <= 16777184` を検査し、block の容量が足りればそのまま返し、
  足りなければ隣接する空き block との結合を試みる（その場での伸長がありうる）。

### 確保の計数

- `tests/primitives.mjs` の C host: `tracked_alloc`・`tracked_free`（16 bytes のヘッダーに大きさと magic `0x51a110ca7e`）と
  `live`・`peak`・`allocations`。IR の `@malloc`・`@free` を置き換えるが、`@realloc` は置き換えない（`tests/features.mjs` は `@realloc` も
  `@tracked_realloc` に置き換える）。`storage` 表は `[call, native result, heap allocations, WASM call, WASM result]` で、各行について
  `allocations - before == count` と `live == 0` を検査する。

### 計測済みの事実（2026-09-29 の probe。静的な数）

`build --emit llvm`（`-O0`）の IR で、関数 `@tz.fn.Main.<name>` の本体に現れる `call ptr @tz.alloc` と `string.concat` を数えた。
fixture 案（テスト計画の表）の全関数も同じ scratch で `build` が成功することを確かめた。

| 関数 | 内容 | `@tz.alloc` | `concat` |
| --- | --- | --- | --- |
| `whole_take` | `let values = new [1, 2, 3]`、`let moved = values` | 1（R1 で移動） | 0 |
| `field_take` | `let holder = Holder { values: new [1, 2, 3], id: 7 }`、`let values = holder.values` | 2（field は複製） | 0 |
| `field_update_reuse` | 同じ `holder` で `Array.set holder.values 0 42` | 2（複製してから書き換え） | 0 |
| `string_append_chain` | `let a = "ab" + "cd"`、`let b = a + "ef"`、`let c = b + "gh"` | 0（確保はランタイム内） | 3 |

`field_take` の本体では、`new` の確保（`%v7`）と複製の確保（`%v27`）の 2 つの `call ptr @tz.alloc` があり、scope の終わりに
`call void @tz.free` と `store %tz.array zeroinitializer` が並ぶ。実行時の確保回数・時間は PX01 の完了後に手順 1 で測る
（HEAD には PX01 の tracked 実行がない）。

### 再現（2026-09-29 に確認）

`/tmp/tz-PM07/field/Main.tz` に次を置く（検証済み）。

```tsuzuri
record Holder { values: [i64], id: i64 }

export def field_take :: i64
fn field_take =
    let holder = Holder { values: new [1, 2, 3], id: 7 }
    let values = holder.values
    values[0] + values.length

export def whole_take :: i64
fn whole_take =
    let values = new [1, 2, 3]
    let moved = values
    moved[0] + moved.length
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-PM07/field --emit llvm -o /tmp/tz-PM07/field.ll
for f in field_take whole_take; do
  printf '%s ' "$f"; awk "/^define .*@tz.fn.Main.$f\\(/,/^}/" /tmp/tz-PM07/field.ll | grep -c 'call ptr @tz.alloc'
done
```

期待（HEAD）: `field_take 2`、`whole_take 1`。Phase 1 の後は `field_take 1`、`whole_take 1`。

## 目標と指標

目標は CI の合否条件にしない。E2E の確保回数は意味の検査として固定値で検査するが、時間は専用の計測機で before と after を記録して判断する。

- G1: 単一使用の Copy のローカルを根とする field の取得（R2）で、複製の確保をなくす。
- G2: 左辺がヒープの所有バッファである文字列の連結（S1）で、新しいバッファを確保せず、左辺の内容を複写しない（`realloc` がその場で伸ばせた場合）。
- G3: 対照の種目（`array_copy`・`record_pipeline`）と確保回数を変えない。どの計測対象も遅くしない。

| 指標 | 単位・統計 | 対象 | 期待（規則から計算。計測で確かめる） |
| --- | --- | --- | --- |
| M1 E2E の確保回数 | 回（1 回の呼び出しの `allocations` の差。固定値） | `tests/primitives.mjs` の `storage` 表に足す行（テスト計画） | `field_take` 2 → 1、`field_take_nested` 2 → 1、`field_update_reuse` 2 → 1、`string_append_chain` 3 → 1。`field_take_shared` 2、`field_take_loop` 6、`string_append_literal_left` 2 は不変 |
| M2 静的な複製 | `@tz.fn.Main.<name>` の本体の `call ptr @tz.alloc` の数 | 「再現」と fixture の関数 | `field_take` 2 → 1、`whole_take` 1 → 1 |
| M3 tracked の確保 | `calls`・`bytes`（PX01。1 回の実行の合計） | 計測対象の 5 種目 | 不変。PX01 は `realloc` を `calls += 1`・`bytes += size` と数えるので、S1 の置き換え（`allocate` 1 回 → `realloc` 1 回、同じ大きさ）は数に現れない |
| M4 時間 | ms（PX01 runner の標本の中央値、最小、最大。標本数は runner の既定の 12） | 計測対象の 5 種目（既定の規模。cpp は 262144） | `utf16_*` は改善がありうる（左辺の複写が `realloc` のその場の伸長で消える分）。対照は変化なし |

M1 の計算: `field_take` は `new` の 1 回だけ。`field_take_loop` は反復 3 回 ×（`new` 1 + 複製 1）で、ループ内のローカルは単一使用にならないため不変。
`string_append_chain` は `"ab" + "cd"` の左辺がリテラル（frame）なので `concat` の 1 回、残る 2 回の `+` は `append` の `realloc`（新しい確保として数えない。
手順 5 の `tracked_realloc`。D8）。`string_append_literal_left` は `"x" + a` の左辺がリテラルなので 2 回とも `concat`。

## 変えてはいけない意味

- Copy の独立性: R2 は、根のローカルの出現が関数の本体でその 1 か所だけ（`single_use_locals`）で、ループの中でない場合に限る。
  元の値を後で読む式が存在しないので、独立した値かどうかを観測できない。`field_take_shared`（根を 2 回使う）は複製のまま。
- 評価順序: R2 は同じ位置で同じ place を読み、`clone_value` の呼び出しを `store zeroinitializer` に置き換えるだけ。S1 は `concat` と同じく、
  左辺を取り、右辺を評価して取り、その後に 1 回だけランタイムを呼ぶ。右辺の評価中の trap・`break`・`return` の時点は変わらない。
- 別名: R1・R2 は根が `borrowed_locals` にないことを要求する。単一使用の根には `ref` の借用も捕捉もない（どちらも出現として数えられる）。
  S1 の右辺は左辺と別の所有値である（同じ値の二重の消費は所有権検査が E1012 で拒否する）ので、`realloc` が左辺を動かしても右辺は無効にならない。
- drop の時点と回数: R2 は根の型が Copy の場合に限る。Copy の型は利用者 drop（B07）を持つ値を含まない（B07 仕様「型規則」2）ので、利用者 drop の
  呼び出しの時点・回数・引数の内容は変わらない。根の record は従来どおり同じ scope の終わりに drop され、移動済みの field は
  `zeroinitializer`（null の pointer、長さ 0）として drop される（R1 の移動済みローカルと同じ既存の不変条件。native の `free(NULL)`、
  WASM の `@tz.free` の null 検査）。変わるのは、同じバッファを解放するのが元の所有者か移動先かだけで、`live == 0` で検査する。
- 文字列は利用者 drop を持たない組み込み型で、S1 の後の左辺は `realloc` が所有する（`drop_framed` を呼ばない）。二重解放は E2E の
  `tracked_free` の magic 検査で検出する。
- 数値・トラップ: `append` は `concat` と同じ溢れの検査（`add` の後の `icmp ult`）と `@tz.string.allocate` と同じ長さの上限
  （`9007199254740991`。UTF-8 は `@tz.utf8string.allocate` の上限）を、確保の前に同じ `@llvm.trap` で行う。確保失敗は従来どおり trap する。
  省いた複製の確保失敗（資源に依存する trap）がなくなることは許す（R1・C01 と同じ）。
- 所有権・借用の規則、診断、`Type::is_copy` は変えない。所有権検査の結果は使わない（Phase 1 は生成の構文的な規則だけで証明する）。
- IR の決定性: 新しい規則はすべて式と集合の順序に依存しない判定。WASM は既定で import を持たないまま（`append` は `@tz.realloc` だけを呼ぶ）。
- native と WASM で同じ確保回数・同じ結果。`-O0` と `-O3` で同じ結果。

## 設計

### 証明規則

| 規則 | 対象の式 | 条件 | 生成 |
| --- | --- | --- | --- |
| R1（既存） | `Local(id)` | `id ∈ single_use`、`id ∉ borrowed_locals` | 移動（`store zeroinitializer`） |
| R2（新規） | `Field(.. Field(Local(id), i) .., j)`（`Field` だけの連鎖、1 段以上） | R1 の条件に加えて、根のローカルの型が `is_copy` | 移動（field の slot へ `store zeroinitializer`） |
| S1（新規） | 文字列（`string`・`Utf8String`）の `Add` | 左辺の `take_operand` が返した frame が空 | `@{runtime}.append`。左辺を `drop_framed` しない |

S1 の根拠: HEAD の `binary` は、frame が空の左辺を連結の後に `drop_framed` で解放している。つまり「frame が空の文字列の値は、
`tz.alloc` 由来のバッファか null を所有する」ことを既存の生成が前提にしており、それを `realloc` に渡すのは解放と同じ条件で安全である。

Phase 1 で扱わないもの（D3）: `Index`・`ListTail`・`UnionPayload`・`Dereference` の place、根が 2 回以上現れる場合の最後の使用、
根の型が Copy でない record の Copy の field。

### データ構造

型・enum は増やさない。`src/call_specialization.rs` に関数を 1 つ足す。

```rust
// src/call_specialization.rs（新規の関数）。visibility は single_use_locals と同じにする
/// The root local and its type when `expression` is a local or a chain of field projections.
pub(super) fn field_root(expression: &TypedExpr) -> Option<(usize, &Type)> {
    match &expression.kind {
        TypedExprKind::Local(id) => Some((*id, &expression.ty)),
        TypedExprKind::Field(value, _) => field_root(value),
        _ => None,
    }
}
```

`src/llvm.rs` の `FunctionEmitter::clones_on_take` は次の形にする（既存の `last_use` の意味を R1・R2 に広げる）。

```rust
fn clones_on_take(&self, expression: &TypedExpr) -> bool {
    let types = self.module.types();
    let last_use = call_specialization::field_root(expression).is_some_and(|(id, root)| {
        self.single_use.contains(&id)
            && !self.borrowed_locals.contains(&id)
            && (matches!(expression.kind, TypedExprKind::Local(_)) || root.is_copy(&types))
    });
    expression.ty.is_copy(&types) && !last_use
}
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 生成 | `src/call_specialization.rs` | `field_root`（新規） | 上の形。`all_children` は使わない（再帰は `Field` の連鎖だけ） |
| 生成 | `src/llvm.rs` | `FunctionEmitter::clones_on_take` | R1・R2（上の形） |
| 生成 | `src/llvm.rs` | `FunctionEmitter::read_place` | 変更なし。false の分岐が既に `self.place(expression)` の slot へ `zeroinitializer` を書き、`relocate` なら `frame_of_place` を使う |
| 生成 | `src/llvm_frame.rs` | `take_operand`, `frame_of_place` | 変更なし。`clones_on_take` が false なら `frame_of_place` の frame を返す |
| 生成 | `src/llvm.rs` | `FunctionEmitter::binary` の文字列の分岐 | `operator == Add && left_frames.is_empty()` なら `call {ty} @{runtime}.append({ty} {lhs}, {ty} {rhs})`。その場合は左辺の `drop_framed`（`src/llvm_frame.rs`）を呼ばず、`forget_temporary(&lhs)` だけを呼ぶ（`take_operand` の `remember_temporary` と対にする）。右辺は従来どおり |
| ランタイム | `src/runtime/string.ll` | `@tz.string.append`（新規） | 「生成 IR とランタイム」の形 |
| ランタイム | `src/runtime/utf8string.ll` | `@tz.utf8string.append`（新規） | 同じ形で要素が `i8`、大きさは長さそのもの、上限は `@tz.utf8string.allocate` と同じ |
| 所有権（A15 が done） | `src/ownership.rs` | `Checker::read_places` の `CopyRead` の記録 | `local` を `field_root` の根にする（根の型が Copy で `state.aliases` にない場合。`E::Local` は従来どおり） |
| 一覧（A15 が done） | `src/copies.rs` | `sites` | コードは変えない。`local` の意味が広がることで、R2 の site が `single_use_locals` の除外に入る |
| 変更なし | `src/llvm.rs` | `clone_value`, `drop_value`, `drop_scope`, `record_update`, `string_constant` | 移動済みの値の drop は既存の不変条件で安全 |
| 変更なし | `src/llvm_bulk.rs` | Vec の伸長 | D6 |
| 変更なし | `src/runtime/heap-native.ll`, `src/runtime/heap-wasm.ll`, `src/runtime/heap-wasm-threads.ll` | `@tz.realloc` | 既存の契約（0 で解放して null、null から確保、失敗は trap）をそのまま使う |

### 生成 IR とランタイム

R2 の前後（`field_take`、`-O0`。名前は例）。

```llvm
; before: holder.values を読んで複製する
  %v25 = load %tz.array, ptr %field
  %v27 = call ptr @tz.alloc(i64 %v26)          ; clone_value の確保と要素の複写が続く
; after: 同じ load の後に field の slot を空にし、確保しない
  %v25 = load %tz.array, ptr %field
  store %tz.array zeroinitializer, ptr %field
```

S1 の前後（`let b = a + "ef"`）。

```llvm
; before
  %v10 = call %tz.string @tz.string.concat(%tz.string %v7, %tz.string %v9)
  call void @tz.free(ptr %v11)                 ; 左辺の解放
; after: 左辺の解放はない。右辺（frame のリテラル）は従来どおり
  %v10 = call %tz.string @tz.string.append(%tz.string %v7, %tz.string %v9)
```

`@tz.string.append`（新規、`src/runtime/string.ll`。`@tz.string.concat` の直後に置く）。

```llvm
define internal %tz.string @tz.string.append(%tz.string %left, %tz.string %right) nounwind {
entry:
  %a = extractvalue %tz.string %left, 0
  %an = extractvalue %tz.string %left, 1
  %b = extractvalue %tz.string %right, 0
  %bn = extractvalue %tz.string %right, 1
  %length = add i64 %an, %bn
  %overflow = icmp ult i64 %length, %an
  %large = icmp ugt i64 %length, 9007199254740991
  %bad = or i1 %overflow, %large
  br i1 %bad, label %fail, label %grow
fail:
  call void @llvm.trap()
  unreachable
grow:
  %old = mul i64 %an, 2
  %new = mul i64 %length, 2
  %data = call ptr @tz.realloc(ptr %a, i64 %old, i64 %new)
  %second = getelementptr i16, ptr %data, i64 %an
  call void @tz.string.copy(ptr %second, ptr %b, i64 %bn)
  %result0 = insertvalue %tz.string zeroinitializer, ptr %data, 0
  %result = insertvalue %tz.string %result0, i64 %length, 1
  ret %tz.string %result
}
```

- 長さ 0 の結果（両辺が空）は `@tz.realloc(.., 0)` が左辺を解放して null を返し、`@tz.string.allocate(0)` と同じ `zeroinitializer` になる。
- 左辺が null（空文字列）なら `@tz.realloc` は新しく確保する（native は `realloc(NULL, n)`、WASM は `@tz.alloc`）。
- `src/llvm.rs` の heap の結合は `@tz.realloc(` を含む IR で heap runtime を加え、条件により `@tz.heap.realloc.unlocked(` へ置き換える。
  `append` は文字列の runtime の中から呼ぶので、その置き換えに含まれることを手順 6 で確かめる。

### Phase 分割

- Phase 1（本チケットの実装範囲）: R2、S1、E2E の確保回数の行、`tracked_realloc`、A15 の除外規則（A15 が done の場合）、計測、文書。
- Phase 2（設計方針。人間が依頼した場合だけ）: 所有権検査の `live` 集合を使う「最後の使用」（根が 2 回以上現れる場合）。
  `Checker::read_places` で `Use::Consume && is_copy && needs_drop` かつ根が後続で生きていない read の span を関数ごとに記録し、
  同じ span に生きている read もあれば移動しない（生成の builder 展開で span が重なる場合の保守的な扱い）。ループの中では、根がその反復の
  本体で束縛された場合だけ対象にする。あわせて `Index`（Copy の要素）・`ListTail`・`UnionPayload` の place、根が Copy でない record の
  Copy の field（B07 の `Type::has_user_drop` が経路のどこにもない場合）、文字列の容量（PM03 と合わせる）を検討する。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。E2E は Node 24 で動かす（`node24` は「計測手順」の関数）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: 変更前のコンパイラを残し、「再現」の scratch と control の IR を保存し、対象のテストの件数を記録する。計測手順の before もここで取る。
- 確認: 次がすべて成功し、「再現」の期待（`field_take 2`、`whole_take 1`）が出る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-PM07/control && cp target/release/tsuzuri /tmp/tz-PM07/tsuzuri-before
cp benchmarks/control/Main.tz /tmp/tz-PM07/control/Main.tz
target/release/tsuzuri build /tmp/tz-PM07/control --emit llvm -o /tmp/tz-PM07/control-before.ll
cargo test --locked --test call_specialization --test consuming_update --test storage 2>&1 | grep 'running'
node24 tests/primitives.mjs target/release/tsuzuri
```

### 手順 2: R2（`field_root` と `clones_on_take`）

- 変更: `src/call_specialization.rs` の `field_root`（新規）、`src/llvm.rs` の `FunctionEmitter::clones_on_take`。
- 内容: 「データ構造」の 2 つの形。`read_place`・`take_operand` は変えない。
- 確認: `cargo build --release --locked` の後、「再現」のコマンドが `field_take 1`、`whole_take 1`。手順 1 の 3 つの Rust テストが同じ件数で成功
  （失敗するのが `only_unobserved_single_use_copies_are_transferred` の field の複製の期待だけなら「既存テストへの影響」に従う）。
  `node24 tests/primitives.mjs target/release/tsuzuri` が成功する。

### 手順 3: R2 の Rust テスト

- 変更: `tests/call_specialization.rs`、`tests/consuming_update.rs`。
- 内容: 「Rust テスト」の 1、2、3 を足す。IR は既存の `only_unobserved_single_use_copies_are_transferred` と同じ helper で得て、
  関数の本体は `body(ir, name)` で切り出す。
- 確認: `cargo test --locked --test call_specialization` が手順 1 の件数 + 2、`cargo test --locked --test consuming_update` が 3 passed。

### 手順 4: R2 の E2E

- 変更: `tests/fixtures/storage/Storage.tz`（`Holder` は既存の record と名前が衝突するので `CopyHolder`・`CopyOuter`（新規）とする）、
  `tests/primitives.mjs` の `storage` 表。
- 内容: 「E2E」の表の field の 5 行。関数の本体は「計測済みの事実」で build を確かめた形（record 名だけを変える）。
- 確認: `node24 tests/primitives.mjs target/release/tsuzuri` が `-O0` と `-O3` の両方で成功し、表示の storage cases の数が 5 増える。

### 手順 5: `tracked_realloc`

- 変更: `tests/primitives.mjs` の C host と IR の置き換え。
- 内容: `@realloc` も `@tracked_realloc` に置き換える（`tests/features.mjs` と同じ）。`tracked_realloc(pointer, n)`（新規）: null なら
  `tracked_alloc(n)`、`n == 0` なら `tracked_free` して `NULL`、それ以外は magic を確かめ、`realloc(header, n + 16)` の後に大きさを書き換え、
  `live` を差し替えて `peak` を更新する。`allocations` は増やさない（D8）。
- 確認: `node24 tests/primitives.mjs target/release/tsuzuri` が成功する（この時点では IR に `@realloc` がなく、結果は変わらない）。

### 手順 6: S1（`append` と `binary`）

- 変更: `src/runtime/string.ll` の `@tz.string.append`（新規）、`src/runtime/utf8string.ll` の `@tz.utf8string.append`（新規）、
  `src/llvm.rs` の `FunctionEmitter::binary`、`tests/storage.rs`。
- 内容: 「生成 IR とランタイム」の形。UTF-8 版は `@tz.utf8string.allocate` の長さの検査を読んでそのまま写す。「Rust テスト」の 4 を足す。
- 確認: `cargo test --locked --test storage` が手順 1 の件数 + 1。`benchmarks/cpp/Kernels.tz` を `/tmp/tz-PM07/cpp/Main.tz` に写して
  `--emit llvm` し、`@tz.fn.Main.make_text` の本体に `@tz.string.append` が 1 回現れる（現れなければ停止条件）。IR 全体で
  `grep -c '@tz.realloc('` と `grep -c '@tz.heap.realloc.unlocked('` を見て、置き換えが `append` の呼び出しにも及ぶことを確かめる。

### 手順 7: S1 の E2E

- 変更: `tests/fixtures/storage/Storage.tz`、`tests/primitives.mjs` の `storage` 表。
- 内容: 「E2E」の表の文字列の 3 行。
- 確認: `node24 tests/primitives.mjs target/release/tsuzuri` が成功し、`tracked_free` の `assert` が出ない。

### 手順 8: A15 の除外規則（A15 が done の場合だけ）

- 変更: `src/ownership.rs` の `Checker::read_places` の `CopyRead` の記録、`tests/copy_cost.rs`。
- 内容: `CopyRead::local` を `field_root` の根にする（D9）。「PM07 で変わる」と注記した期待値だけを更新し、注記を消す。
- 確認: `cargo test --locked --test copy_cost` が全件成功。debug build の `cargo test --locked` で A15 D5 の `assert!` が落ちない。

### 手順 9: 全体の確認

- 変更: なし（B07 が done なら「Rust テスト」の 5 を足す）。
- 確認: `cargo test --locked` が成功。`cargo build --release --locked` の後、GUIDE §3.1 の選択表で「生成」「ランタイム」の行が指す E2E
  （少なくとも `node24 tests/primitives.mjs target/release/tsuzuri` と `node24 tests/features.mjs target/release/tsuzuri`）が native/WASM ×
  `-O0`/`-O3` で成功する。`/tmp/tz-PM07/control` を after で build し、`diff /tmp/tz-PM07/control-before.ll <after>` の差が
  R2 の位置と `append` の定義だけである。

### 手順 10: 生成コードの確認

- 変更: なし。
- 確認: 「生成コードの確認」のパターンがすべて成り立つ。

### 手順 11: 計測と記録

- 変更: なし。
- 確認: 「計測手順」の 3 run がそろい、`report` の表を完了報告に貼る。

### 手順 12: 文書

- 変更: 「ドキュメント」の表のファイル。
- 確認: `git diff --check` が空。`_perfs/README.md` の PM07 の行が done。

## 計測手順

PX01 の「before／after（他チケットの共通手順）」と GUIDE §14 に従う。環境の条件（AC 電源、他の作業の停止、load average）は PX01 の「環境と条件」と同じ。
標本は runner 既定の warm-up と 12 標本で、中央値・最小・最大を PX01 形式で `target/perf/<run_id>/` に残す。tracked 実行は `--metrics` が行う。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
node24() { npx --yes --package=node@24 node "$@"; }
measure() {
  mkdir -p "$1"
  { git rev-parse HEAD; git status --porcelain | wc -l; sw_vers; uname -m
    sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize; pmset -g ps | head -1; uptime
    clang --version | head -1; rustc --version; node24 --version; } > "$1/env.txt"
  for suite in control cpp; do
    node24 benchmarks/run-$suite.mjs "$2" --metrics "$1" > "$1/$suite.json" || return 1
  done
}
measure target/perf/PM07-before /tmp/tz-PM07/tsuzuri-before
measure target/perf/PM07-after target/release/tsuzuri
measure target/perf/PM07-before2 /tmp/tz-PM07/tsuzuri-before
node24 benchmarks/metrics.mjs report target/perf/PM07-before2 --baseline target/perf/PM07-before
node24 benchmarks/metrics.mjs report target/perf/PM07-after --baseline target/perf/PM07-before
```

- 読む行は「計測対象」の 5 種目だけ。before2 と before の比較で `改善`・`悪化` が出た時間の metric は「判定できない」と報告する。
- M3（`calls`・`bytes`）は期待どおり不変かを確かめる。変わったら理由を調べて報告する（確保が増えたなら停止条件）。
- M1 は `tests/primitives.mjs` が固定値で検査し、M2 は「生成コードの確認」で数える。`docs/benchmarks.md` への公開は人間が依頼した場合だけ（PX01 D2）。

## 生成コードの確認

`/tmp/tz-PM07/cases/Main.tz` に「E2E」の表の fixture 関数を置き、`-O0` と `-O3` で IR を出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-PM07/cases --emit llvm -o /tmp/tz-PM07/cases.ll
target/release/tsuzuri build /tmp/tz-PM07/cases --emit llvm -O3 -o /tmp/tz-PM07/cases-O3.ll
fn_body() { awk "/^define .*@tz.fn.Main.$1\\(/,/^}/" /tmp/tz-PM07/cases.ll; }
for f in field_take field_take_nested field_update_reuse field_take_shared; do
  printf '%s ' "$f"; fn_body "$f" | grep -c 'call ptr @tz.alloc'; done
fn_body string_append_chain | grep -oE '@tz.string.(append|concat)' | sort | uniq -c
awk '/^define internal %tz.string @tz.string.append/,/^}/' /tmp/tz-PM07/cases.ll | grep -E '@tz.realloc|@llvm.trap|icmp ult'
```

- 期待: `field_take 1`、`field_take_nested 1`、`field_update_reuse 1`、`field_take_shared 2`。`string_append_chain` は `2 @tz.string.append` と
  `1 @tz.string.concat`。`append` の定義に `@tz.realloc`・`@llvm.trap`・`icmp ult` がそろう。
- R2 の位置では、field の `load` の直後に同じ pointer への `store %tz.array zeroinitializer` があり、`clone_value` の要素の複写がない。
- `-O3` の IR では `append` が inline されうる。`@tz.fn.Main.string_append_chain` に `@malloc` の後の `@realloc` が 2 回あること、
  または `@tz.string.append` の呼び出しが残ることを確かめる。
- native の機械語: `node24 benchmarks/run-cpp.mjs target/release/tsuzuri --quick --artifacts /tmp/tz-PM07/cpp-artifacts` で保存した最適化後の
  IR から作った object を `/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d` で見て、`make_text` に当たるループに `_realloc` の呼び出しがあり、
  左辺を複写する 2 つ目の copy ループがないことを確かめる。SIMD や速度の主張はこの確認と計測の両方がある場合だけ行う。

## テスト計画

### Rust テスト

期待値は規則から数える（`new` 1 回 + 複製 1 回）。コンパイラの出力から写さない。

1. `tests/call_specialization.rs::fields_of_single_use_copy_roots_are_transferred`（新規）: `field_take`・`field_take_nested` の形で、本体の
   `call ptr @tz.alloc` がそれぞれ 1。
2. `tests/call_specialization.rs::fields_of_observed_looping_or_noncopy_roots_are_copied`（新規）: `field_take_shared` の形が 2、
   `field_take_loop` の形が 2（静的な数）、根が Copy でない `record Mixed { values: [i64], name: string }` の
   `let mixed = Mixed { values: new [1, 2, 3], name: "a" + "b" }`、`let values = mixed.values`、`values[0]` が 2（文字列の確保はランタイム内）。
3. `tests/consuming_update.rs::field_inputs_of_single_use_roots_are_updated_in_place`（新規）: `field_update_reuse` の形で本体の
   `call ptr @tz.alloc` が 1。
4. `tests/storage.rs::string_concatenation_extends_owned_left_operands`（新規）: `string_append_chain` の本体に `@tz.string.append` が 2、
   `@tz.string.concat` が 1。`string_append_literal_left` は `append` 0・`concat` 2。`utf8_append_chain` は `@tz.utf8string.append` が 1。
   同じ source を 2 回 emit して IR が一致する（決定性）。
5. B07 が done の場合だけ: `tests/call_specialization.rs::fields_of_drop_roots_are_copied`（新規）。Drop instance を持つ record の Copy の field を
   単一使用の根から取る形で、本体に複製の `call ptr @tz.alloc` が残る（根が Copy でないので R2 が効かない）。

### E2E

`tests/fixtures/storage/Storage.tz` に関数を足し、`tests/primitives.mjs` の `storage` 表に
`["<name>()", "<native result>", <heap allocations>, (api) => api.tz_<name>(), <WASM result>n]` の行を足す。native は `-O0`・`-O3` の両方で
確保回数と `live == 0` を、WASM は結果と import がないことを検査する（既存の仕組み）。結果は手計算。

| 関数 | 内容 | 結果 | 確保（after。before） |
| --- | --- | --- | --- |
| `field_take` | `values[0] + values.length`（`[1, 2, 3]`） | 4 | 1（2） |
| `field_take_shared` | `values[0] + holder.id`（id 7） | 8 | 2（2） |
| `field_take_nested` | `outer.inner.values` の `values[1]`（`[4, 5]`） | 5 | 1（2） |
| `field_update_reuse` | `Array.set holder.values 0 42` の `updated[0] + updated[2]` | 45 | 1（2） |
| `field_take_loop` | `while i < 3` で `values = [i, 10]` を足す（0 + 10 + 1 + 10 + 2 + 10） | 33 | 6（6） |
| `string_append_chain` | `"ab" + "cd"`、`+ "ef"`、`+ "gh"` の長さ | 8 | 1（3） |
| `string_append_literal_left` | `"x" + ("ab" + "cd")` の長さ | 5 | 2（2） |
| `utf8_append_chain` | `u8"ab" + u8"cd"`、`+ u8"ef"` の長さ（手順 7 で `tsuzuri check` を通す） | 6 | 1（2） |

### 既存テストへの影響

- `tests/primitives.mjs` の既存の `storage` 行は変わらない（`record_update_owned` の基はローカル全体、`debug_trace_transfer` の左辺はリテラル）。
- `tests/call_specialization.rs::only_unobserved_single_use_copies_are_transferred` が単一使用の Copy の根からの field の複製を期待している
  場合だけ、その期待を R2 に合わせて変え、完了報告に書く。
- A15 が done なら `tests/copy_cost.rs` の「PM07 で変わる」と注記した期待値（A15 が定めたもの）。それ以外はなし。

### 性能

「計測手順」のとおり。共有 CI に時間の合否条件を足さない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/architecture.md` | `## 性能設計の原則` | R1・R2・S1 の証明規則と、所有権検査の結果は使わないこと（D2） |
| `docs/architecture.md` | `確保回数は storage E2E で` で始まる段落 | field の移動と文字列の `append` も storage E2E で区別すること |
| `docs/language.md` | `### Ownership / Borrowing` の Copy の費用の段落 | 単一使用の Copy のローカルとその field からの取得は複製しないこと（意味は変わらない注記） |
| `_perfs/README.md` | `### PM. メモリと成果物サイズ` | PM07 の状態を done |
| `docs/benchmarks.md` | 変更しない | 公開は人間が依頼した場合だけ（PX01 D2） |

## 受け入れ条件

- [ ] R2: `field_take`・`field_take_nested`・`field_update_reuse` の確保が 1 回で、`field_take_shared`・`field_take_loop`・根が Copy でない形は複製のまま。
- [ ] S1: `string_append_chain` が 1 回、`utf8_append_chain` が 1 回、左辺がリテラルの連結は `concat` のまま。
- [ ] 全 E2E が native/WASM × `-O0`/`-O3` で成功し、`live == 0`、WASM の import がない。
- [ ] 「生成コードの確認」の期待がすべて成り立つ。
- [ ] PM07-before・after・before2 の 3 run と `report` の表があり、改善の主張は `verdict` によるものだけ。
- [ ] A15 が done なら `copies::sites` の除外が R2 に従い、A15 D5 の照合が通る。
- [ ] 「ドキュメント」の表を更新した。GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `single_use_locals` はループを見るとそれまでに数えた全ローカルを 2 回以上にする。ループの中の field の取得が改善しないのは仕様であり、
  `single_use_locals` を緩めない（Phase 2）。緩めると反復ごとに同じ field を空にして読む誤りになる。
- `field_root` は `Field` と `Local` 以外で `None` を返す。`Dereference` を通すと借用先のメモリを `zeroinitializer` で壊す。
- R2 の Copy の判定は根の型で行う。field の型だけで判定すると、Drop 型（B07）の record の field を空にして利用者 `drop` に見せてしまう。
- S1 で左辺に `drop_framed` を呼ぶと二重解放。`forget_temporary` を忘れると、ループの脱出で `realloc` 済みの古い pointer を解放する。
  どちらも `tracked_free` の magic の `assert` で検出できる。
- `tests/primitives.mjs` で `@realloc` を置き換え忘れると、libc の `realloc` がヘッダー付きの pointer を受け取り heap が壊れる（手順 5 を先に行う）。
- PX01 は `realloc` を `calls` に数えるので、M3 から「確保が減った」と主張しない。確保回数の減少は M1 で示す。
- WASM の `@tz.realloc` は `new_size <= 16777184` を検査する。`@tz.alloc` の上限と比べ、違えば停止条件。`heap-wasm-threads.ll` の `@tz.realloc` と
  `src/llvm.rs` の `@tz.heap.realloc.unlocked(` への置き換えは手順 6 で確かめる。
- `string.ll`・`utf8string.ll` は手書きのランタイムで、`src/runtime/generate.py` と `generate_math.py` の対象（`numeric.ll`・`math.ll`）ではない。
  生成物を手で直さない規則と混同しない。
- fixture の record 名は `Storage.tz` の既存の `Holder { values: [i64], name: string }`（Copy でない）と衝突する。そのまま流用すると R2 が効かず期待が外れる。
- 期待値は規則から数える。実装後のコンパイラの出力を写して期待にしない。

## 対象外

- 参照カウントによる共有（C10 Phase 2）、Copy の意味の変更。
- 所有権検査の `live` 集合による最後の使用、`Index`・`ListTail`・`UnionPayload` の place、根が Copy でない record の field（Phase 2）。
- 文字列の容量（PM03 と合わせて検討）、Vec の伸長方針の変更（D6）。
- 明示的な複製（`clone_string`、`Vec.clone`、A15 の `Array.copy`）、並列 kernel（`src/llvm_parallel.rs`）と再帰型（`src/llvm_recursive.rs`）の複製。

## 決定事項

### D1: field の移動の証明規則

- 決定: R2。`Field` だけの連鎖で、根のローカルが `single_use` にあり `borrowed_locals` になく、根の型が Copy の場合だけ移動する。
- 理由: 根の出現が 1 か所なら後続の読み出しも借用もない。根が Copy なら利用者 drop を含まず（B07）、空にした field の drop は R1 と同じ既存の不変条件で安全。
- 状態: 既定案（実装者はこの案に従う）

### D2: Phase 1 は所有権検査の結果を使わない

- 決定: 生成が持つ `single_use`・`borrowed_locals` だけで証明する。`CheckedModule` に所有権の結果を残さない。
- 理由: HEAD では所有権の結果は生成へ渡らず（A15 D1 も必要なときに再実行する）、span で式を対応させるのは builder 展開で衝突しうる。
- 状態: 既定案（実装者はこの案に従う）

### D3: Phase 1 の place の範囲

- 決定: `Local` と `Field` の連鎖だけ。`Index`・`ListTail`・`UnionPayload`・`Dereference` は従来どおり複製する。
- 理由: match の束縛は `src/llvm_control.rs` で `borrowed_locals` を使う別の経路、要素は配列に穴を残す、参照先は所有していない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 文字列の連結の再利用

- 決定: S1。左辺の frame が空の `Add` だけ `@tz.string.append`・`@tz.utf8string.append` を呼び、左辺は `forget_temporary` だけ行う。
- 理由: frame が空の左辺は HEAD でも連結の後に解放されており、`realloc` できる所有バッファ（または null）である。リテラルとスタックの左辺は frame を持つ。
- 状態: 既定案（実装者はこの案に従う）

### D5: `append` の置き場所

- 決定: `src/runtime/string.ll`・`utf8string.ll` の `concat` の直後に置く。文字列の runtime を含む全プログラムの IR に定義が 1 つずつ増えるのを許す。
- 理由: `concat` と同じ結合の仕組みに乗れ、使わない internal 関数は `-O3` で消える。Rust 側で条件付きに出す仕組みは要らない。
- 状態: 既定案（実装者はこの案に従う）

### D6: Vec の伸長方針

- 決定: 変えない（初回 4、倍増）。要素の大きさに応じた初期容量は、Vec を使う種目が PX02 で入ってから Phase 2 で決める。
- 理由: `benchmarks/` の Tsuzuri の種目に Vec を使うものがなく、変更の根拠を計測できない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 文字列の容量

- 決定: 容量を持たせず `realloc` だけを使う。追記の多い処理で不足する場合は `Vec<char>` や将来の builder API を案内する。
- 理由: `%tz.string` の表現を変えると PM03 と衝突する。`realloc` がその場で伸ばせない場合は従来と同じ複写で、償却の保証はない（効果は計測で判断する）。
- 状態: 既定案（実装者はこの案に従う）

### D8: E2E での `realloc` の数え方

- 決定: `tests/primitives.mjs` の `allocations` は新しい block だけを数える。`tracked_realloc` は old が null の場合だけ `allocations` を増やす。
- 理由: storage 表の数は「新しく作ったバッファ」の数で、S1 の効果を固定値で検査できる。`live`・`peak` は差し替えで正確に保つ。
- 状態: 既定案（実装者はこの案に従う）

### D9: A15 の一覧との整合

- 決定: A15 が done なら、`CopyRead::local` を `field_root` の根（根の型が Copy で `state.aliases` にない場合）に広げ、`sites` の除外は変えない。
- 理由: A15 D4 の除外（`local ∈ single_use_locals`）がそのまま R2 と一致し、W1006 が省いた複製を報告しない。A15 D5 の照合は生成が減る方向なので崩れない。
- 状態: 既定案（実装者はこの案に従う）
