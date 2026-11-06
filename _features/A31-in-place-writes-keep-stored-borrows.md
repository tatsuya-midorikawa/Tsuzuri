# A31: `Array.write` と、それで書き込む std の関数が、書き込んだ値の借用を書き込み先の持ち主に残さない（借用元の解放後に読める）

| 項目 | 内容 |
| --- | --- |
| ID | A31 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | A13, A28 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-06（A28 のレビューで発見。A30 の先端 `f957a7d9`（PR #94）のバイナリで確認） |
| 承認 | 不要（健全性の穴を塞ぐ。今受理している危険なプログラムだけが `E1013`・`E1014` になる。今受理している安全なプログラムは変えない。変わるなら停止条件） |
| 改善する劣位 | 不具合（メモリ安全性）: `let mut arr = [\() -> 1]` に、局所の `text` を借用する関数を `Array.write 0 (\() -> String.length borrowed) (ref mut arr)` で書き込み、`arr[0]` を関数から返すと検査を通る。返した関数を呼ぶと解放した `text` を読み、9 ではなく不定の値を返す。`Array.set`・`Array.fill`・`Array.blit` など、`Array.write` で書き込む std の関数と、参照を要素に持つスライス（`ref string`）も同じ。`arr[0] = v` の代入なら正しく拒否される |
| 手本にする既存実装 | `src/closures.rs:446〜458`（`Vec.push_mut` の生成関数の `RegionSlots { writes: true }`）と `src/ownership.rs:2292` の `store_arguments`（呼び出し側が書き込んだ値の loan を書き込み先の持ち主へ足す）、`src/ownership.rs` の A28 の `parameter_retention`（callee の本文から呼び出しの事実を求める最小不動点。契約を定義で検査せず、呼び出し側で使う） |
| 主な影響ファイル | `src/ownership.rs`, `src/closures.rs`, `tests/mutable_slices.rs`, `tests/rc.rs`, `tests/fixtures/mutable_slices/Main.tz`, `tests/features.mjs`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `docs/language.md`, `docs/architecture.md`, `_features/README.md` |

## 目的

可変スライスや排他参照の先へ値を書き込む呼び出しが、`arr[i] = v` の代入と同じく、書き込んだ値の借用を書き込み先の持ち主に残すようにする。
そうすれば、借用元が書き込み先より先にムーブ・解放されるプログラムは `E1014`・`E1013` になり、安全なコードだけで解放後の読み出しが起きることはなくなる。

塞いだ後は、A28 が入れた保守的な除外（本体が排他借用を呼び出しへ渡す関数は事実を持たず、読み通しもしない。その中で作るラムダも読み通さない。`src/ownership.rs` の `lending_bodies`・`CallFacts.lending`）を外せる。

## 着手条件と停止条件

### 着手条件

- A13・A28 が `_features/README.md` で done であること。
- GUIDE §2.3 の基準コマンドが成功し、「再現」を基準のバイナリで確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 今受理している安全なプログラムが 1 つでも拒否される（「落とし穴」の一覧と「テスト計画」の受理の一覧。とくに、定義の位置での `store_region_unnamed`・`store_region_mismatch`・`self_referential_store` と、部分適用の `region_callback_stores_exclusive_input`）。この穴の修正はそれを許さない。A28 の 1〜2 回目のレビュー対応（D3）は、これで取り下げになった。
- `std/`・`examples/`・`tests/fixtures/` のプログラムの診断や IR が変わる。
- 書き込みの事実を求める不動点が、A28 の事実（`parameter_retention`）と循環して、どちらも決まらない。

## 現状（A30 の先端 `f957a7d9` で確認）

- `src/closures.rs:446〜458`: 書き込む値と排他参照を引数に持つ組み込み関数のうち、生成関数に `RegionSlots { writes: true }` を持つのは `Vec.push_mut`（書き込む値の型が loan を運ぶとき）だけ。
  `Array.write :: i64 -> 'a -> ref mut ['a..] -> unit`（`src/check.rs:1391`）と `SyncMap.__replace :: 'a -> ref mut 'a -> 'a`（std の非公開の基本関数）は契約を持たない。
- そのため `store_arguments`（`src/ownership.rs:2292`）は、`Array.write` の呼び出しで書き込む値の loan を書き込み先の持ち主に足さない。呼び出しの結果は `unit` で、`eval_call` は引数の loan を捨てる。
  書き込んだ借用は、配列の `Value` から消える。
- `Array.set`（`std/Array.tz:1719`）・`Array.fill`（1731）・`Array.blit`（1750）と、`Vec.copy_to*`・`random_shuffle_in_place_*`・`sort_in_place_*`・`MatrixView.write`・`fill`・`copy_from`・`map_in_place` など、
  本体が `Array.write` で書き込む std の関数も、同じく何も残さない。std の型変数には region を書けない（`own 'a { s }` は `RegionOnNonBorrow`）ので、宣言の region の契約で書くこともできない。
- 代入 `arr[0] = v`（edition 2027）と `deref r = v` は `store_through` を通り、局所の持ち主は新しい loan を持つ。外部の参照先への代入は region の契約を要求する（`store_region_unnamed`）。
- A28 は、callee の本文の検査が結果の `Value` に残す loan を信頼して、結果に届かない引数を「一時」にし、`Rc.share` などで所有者の中身を「読み通す」。この穴の上では、
  関数の境界を越えて穴が広がる（下の「A28 の事実が広げる形」）。A28 は `lending_bodies` で、そうした本体（とその中で作るラムダの本体）を A28 より前の規則に戻している。

### 再現（検証済み。A30 の先端 `f957a7d9`、macOS arm64、edition 2027）

1 つの関数の中の形。`tsuzuri check` は何も報告しない。値は `tsuzuri run -O0` と `-O3` の出力で、どれも期待は 9（`"123456789"` の長さ）。

```text
def leak :: i64 -> (unit -> i64)
fn leak n =
    let text = to_string n
    let borrowed = ref text
    let mut arr = [\() -> 1]
    Array.write 0 (\() -> String.length borrowed) (ref mut arr)
    arr[0]
let f = leak 123456789
f ()
```

| 名前 | 書き込みの行（ほかは上と同じ） | `-O0` | `-O3` |
| --- | --- | --- | --- |
| `h1_single_write` | `Array.write 0 (\() -> String.length borrowed) (ref mut arr)` | `4311485196` | `0` |
| `h2_single_set` | `Array.set 0 (\() -> String.length borrowed) (ref mut arr)` | `4297050984` | `0` |
| `h3_single_fill` | `Array.fill 0 1 (\() -> String.length borrowed) (ref mut arr)` | `4295232588` | `0` |
| `h4_single_slice` | `Array.write 0 (\() -> String.length borrowed) (ref mut arr[0..1])` | `4363750156` | `0` |
| `h7_array_return` | 同じ書き込みで、`def leak :: i64 -> [(unit -> i64)]` が `arr` を返し、`f[0] ()` を呼ぶ | `4370566216` | `0` |

ほかの形（どれも受理される）:

- `h5_rc_get`: `let mut arr = [Rc.new (\() -> 0)]` に `Array.write 0 (Rc.new (\() -> String.length borrowed)) (ref mut arr)` で書き、`deref (Rc.get (ref arr[0]))` を返す。`-O0` は `4328590444`、`-O3` は `0`。
- `h9_ref_elements`: 参照を要素に持つ `let mut arr = [ref other]` に、`if` の中の局所の `local = to_string n` を `Array.set 0 (ref local) (ref mut arr)` で書き、`if` の後で `arr[0].length` を読む。`-O0`・`-O3` とも `0`（期待は 9）。
- `h6_move_after`: トップレベルで書き込んだ後に `let moved = text` で借用元をムーブし、`arr[0] ()` を呼ぶ。受理される（ムーブは文字列を解放しないので 9 を出力するが、借用中の値のムーブで、拒否されるべき）。

拒否される形（正しい）: 同じ関数を `arr[0] = \() -> String.length borrowed` で書くと、`Main.tz:7:5` `E1013`/`borrow_outlives_block`。edition 2026 では要素への代入そのものが `E1012`/`assign_to_immutable_place`。

### A28 の事実が広げる形（A28 のレビューの再現。A30 の先端では拒否、A28 は `lending_bodies` で拒否のまま）

```text
def stash :: ref string -> own (unit -> i64) -> (unit -> i64)
fn stash text g =
    let mut arr = [g]
    Array.write 0 (\() -> String.length text) (ref mut arr)
    arr[0]
def leak :: i64 -> (unit -> i64)
fn leak n =
    let text = to_string n
    stash (ref text) (\() -> 1)
let f = leak 123456789
f ()
```

A30 の先端は `Main.tz:9:5` `E1013`/`borrow_outlives_block`（`stash` の結果がすべての引数の借用を持つ）。A28 の事実を除外なしで使うと受理され、`-O0`・`-O3` とも `0` を出力した。
4 行目を `Array.set 0 (...) (ref mut arr)`・`Array.fill 0 1 (...) (ref mut arr)`・`Array.write 0 (...) (ref mut arr[0..1])`・`ref mut arr |> Array.write 0 (...)`・`Array.blit 0 0 1 [...] (ref mut arr)` にしても同じ。
読み通しの形（`let mut arr = [Rc.new (\() -> 0)]` に `Array.write 0 (Rc.new (\() -> String.length borrowed)) (ref mut arr)` で書き、`Rc.share (ref arr[0])` を返す。パイプライン `ref arr[0] |> Rc.share` も）は、
A30 の先端で `Main.tz:7:5` `E1013`/`borrow_outlives_block`、除外なしの A28 で受理され、不定の値を出力した。

ラムダの形（A28 の 3 回目のレビュー対応で見つけた。除外が書き込む本体だけを見ていた `29af0335` の A28 で受理された）:

```text
def make :: i64 -> (unit -> Arc<unit -> i64>)
fn make n =
    let text = to_string n
    let borrowed = ref text
    let mut arr = [Arc.new (\() -> 0)]
    Array.write 0 (Arc.new (\() -> String.length borrowed)) (ref mut arr)
    \() -> Arc.share (ref arr[0])
let f = make 123456789
let g = f ()
(deref (Arc.get (ref g))) ()
```

ラムダは書き込みの前の `arr`（loan を持たない）を捕捉し、持ち上げた本体は捕捉を読み通すので、`Arc.share` の結果は何も借用しない。A30 の先端は `Main.tz:7:6` `E1013`/`task_result_retains_borrow`（ラムダの結果が捕捉の借用を持つ）、
`29af0335` の A28 は受理し、`-O0` は `4301672048`（`-O3` は `9`）を出力した。A28 は、書き込む本体から持ち上げたラムダ（ラムダの中のラムダを含む）にも除外を伝えて、A30 の先端と同じ診断に戻した（`eea6dbfc`、`tests/rc.rs` `a_share_of_an_element_holds_what_was_written_in`）。
パイプライン `\() -> ref arr[0] |> Arc.share`、入れ子の `\() -> \() -> Arc.share (ref arr[0])` も同じ。

## 仕様

- `Array.write`・`SyncMap.__replace` と、それで書き込む関数（std と利用者の関数、generic な関数の具体化）の呼び出しは、書き込む値の借用を、書き込み先の持ち主（局所の配列、`MatrixView.Mut` の窓なら窓が借用している配列の持ち主）に残す。
  残した後は、`arr[i] = v` と同じ規則で検査する: 書き込み先が生きている間の借用元のムーブ・変更は `E1014`/`borrow_conflict`、借用元が先に解放されるなら `E1013`（`borrow_outlives_block`・`assignment_extends_borrow`）。
- 書き込む値が借用を持たないとき、または書き込み先が外部（`ref mut` の引数）のとき、定義の位置では何も拒否しない。外部の書き込み先への書き込みは、その関数の「書き込みの事実」として呼び出し側へ伝える（設計）。
- 部分適用と関数値（`let w = Array.set 0 f`、`apply (Array.write 0 f) (ref mut arr)`）は、今と同じく受理する。保守的に、関数値の呼び出しは、loan を運ぶ引数（と関数値自身）の借用を、排他参照のすべての引数の書き込み先へ残す。
  これで拒否が増えるのは、残した借用の借用元を書き込み先が生きている間にムーブ・解放するプログラムだけで、それが安全なものを含むなら停止条件。
- 診断の code と kind は既存のものだけを使う。

## 設計

### 方針（D1）

A28 の `parameter_retention` と同じく、契約を定義で検査せず、callee の本文から呼び出しの事実を求め、呼び出し側で使う。
事実は「引数 `i` の loan（または `i` の参照先の loan）が、排他参照の引数 `j` の参照先へ入りうる」の組で、特殊化した本文ごとに最小不動点で求める。

- 本文の評価で、外部の書き込み先（`ref mut` の引数）への書き込みは、誤りにせず、書き込んだ loan の外部ルートを記録する（`store_through` の外部の枝の代わり）。
  局所の持ち主への書き込みは今の `store_through` のまま（`owner.loans.extend`）。
- 組み込み関数 `Array.write`・`SyncMap.__replace` は、書き込む値の引数から書き込み先の引数への組を最初から持つ（`Vec.push_mut` の `RegionSlots` と同じ向き）。
- 呼び出し側の `eval_call` は、callee の事実の組ごとに、引数 `i` の loan を引数 `j` の参照先の持ち主へ `store_through` で足す（`store_arguments` と同じ扱い）。
- `MatrixView.Mut` の窓のように、排他借用を持つ局所値への書き込みは、その排他借用の場所（窓が借りている配列）の持ち主にも届ける。

### 段ごとの変更（案）

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/ownership.rs` | 新しい `store_facts`（`parameter_retention` と同じ不動点の枠） | 特殊化した本文ごとの書き込みの組 |
| `src/ownership.rs` | `store_through` の外部の枝 | `region_slots` の無い関数では、誤りにせず書き込みの組を記録する（region の契約を持つ関数は今のまま） |
| `src/ownership.rs` | `eval_call`・パイプラインの完成（`piped_callee` と同じ形） | 事実の組で書き込み先の持ち主へ loan を足す |
| `src/ownership.rs` | `lending_bodies`・`CallFacts.lending`・`Contract.lends_exclusive`・`body_lends_exclusive` | 書き込みの事実がそろった後に外し、A28 のテストがそのまま通ることを確かめる |
| `src/closures.rs` | 生成関数の `RegionSlots` | 変えない（`Vec.push_mut` の契約はそのまま。`Array.write` には契約を足さない: D1） |

## 実装手順

### 手順 1: ベースラインと記録

- 変更: なし（`target/scratch/a31/` の program だけ）。
- 内容: 「再現」と「A28 の事実が広げる形」、「落とし穴」のすべてのプログラムを書き、基準のバイナリの `check --json` と `run -O0/-O3` を記録する。
- 確認: 結果をこのチケットの末尾に追記した。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/mutable_slices.rs`、`tests/rc.rs`。
- 内容: 「テスト計画」の拒否のテスト（今は受理されるので失敗する）と受理のテスト（「落とし穴」の一覧。今も受理されるので成功する）。
- 確認: 拒否のテストだけが失敗する。

### 手順 3: 書き込みの事実

- 変更: `src/ownership.rs`。
- 確認: 手順 2 のテストが成功する。`cargo nextest run --locked` の全件が成功する。

### 手順 4: A28 の除外を外す

- 変更: `src/ownership.rs` の `lending_bodies`（と `CallFacts.lending`・`Contract.lends_exclusive`）と、それを説明する文書（「ドキュメント」）。
- 確認: `tests/mutable_slices.rs` の `a_function_that_writes_in_place_keeps_its_arguments_loans` と `tests/rc.rs` の `a_share_of_an_element_holds_what_was_written_in` が、拒否の理由を書き込みの事実に変えても成功する。

### 手順 5: 既存のプログラムが変わらないこと

- 確認: `tests/fixtures/*`・`examples/*` の `check --json` と `build --no-cache --emit llvm` が基準と一致する。「落とし穴」の一覧が基準と同じ `check --json` と実行結果になる。

### 手順 6: ドキュメントと最終確認

- 「ドキュメント」の各ページ。T2。

## テスト計画

### Rust テスト

- 拒否: 「再現」の `h1`〜`h5`・`h7` が `E1013`/`borrow_outlives_block`、`h9` が `E1013`/`assignment_extends_borrow`、`h6` が `E1014`/`borrow_conflict`。
  「A28 の事実が広げる形」の 6 形と読み通しの 2 形が `E1013`/`borrow_outlives_block`、ラムダの 3 形が `E1013`/`task_result_retains_borrow`（どれも A30 の先端と同じ位置）。
  利用者の generic な書き込み関数 `def put :: own 'a -> ref mut ['a..] -> unit` で借用する関数を書き、借用元をムーブする形が `E1014`。`MatrixView.write` で窓へ書き、窓を使い終えた後に借用元をムーブして配列を使う形が `E1014`。
- 受理（今も受理。値は基準と同じ）: 「落とし穴」の一覧のすべて。

### E2E

- `tests/fixtures/mutable_slices` の `reference_writes`・`generic_writes`・`matrix_writes`（A28 が足した、基準も受理する形）が、native と WASM の `-O0`・`-O3` で同じ値（233723・91052425・20200076872）。

### 既存テストへの影響

- 拒否が増えるのは、今まで危険だったプログラムだけ（手順 5）。

### 性能

- 検査だけ。`TSUZURI_TIME_PASSES=1 tsuzuri check` で、`hash_map`・`list_fsharp`・`array_fsharp`・`vec_dotnet`・`rc`・`tzroutine` の fixture と `examples/tzroutines` の中央値が 10 % を超えて増えないこと（共有の CI には閾値を入れない）。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md` の「現在の制限」: 既知の不具合の注意（A31 へのリンク）を、実際の振る舞いに置き換える。排他借用を呼び出しへ渡す関数の除外の文も消す。
- `_tsuzuri/language-reference/values-and-functions/lambda-expressions.md`・`built-in-types-and-modules/rc.md`: 同じ除外の文を消す。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`・`built-in-types-and-modules/matrix-view.md`・`array.md`: 書き込んだ借用を書き込み先の持ち主が持つことを書く。
- `docs/language.md`（「関数の結果が持つ借用」と、排他借用フィールドの節の既知の不具合の段落）、`docs/architecture.md`（A28 の段落の `lending_bodies`）。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] 「再現」と「A28 の事実が広げる形」のプログラムが拒否される。
- [ ] 「落とし穴」の一覧のプログラムが基準と同じ `check --json` で、受理されるものは同じ値を出力する。
- [ ] `tests/fixtures/`・`examples/` の診断と IR が変わらない。
- [ ] A28 の `lending_bodies` の除外を外し、A28 のテストが成功する。
- [ ] `check-docs.mjs` と T2 が成功する。

## 落とし穴

A28 のブランチ（`fix/a28-function-result-retention`）の 1〜2 回目のレビュー対応（決定事項 D3）が、同じ穴を region の契約で塞ごうとした先行の実装である。
コミットは `4fc7a7cd`（`closures::derive_store_slots`、`Array.write`・`Vec.push_mut`・`SyncMap.__replace` の生成関数の `RegionSlots`、`store_arguments` の `own_contents` の前身、`store_through` の変更、パイプラインの `piped_store`・`eval_piped_store`）、
`8fa5bfc1`・`8f696752`（文書と、パイプラインを組み込み関数と std の関数だけにする `regions::validate_contract_calls` の規則）、`d407ea8c`（記録）、
`cd23f69e`（`CheckedFunction.instance`、`RegionSlots.derived`、利用者の generic な関数と `MatrixView` への派生の契約、`own_contents`、窓の後ろの持ち主への `store_through`）、`914dede0`（文書と記録）。
3 回目（`29af0335`）で全部を取り下げた。契約を定義で検査するので、どの回も、基準が受理する安全なプログラムを拒否した。次の実装はこれらを全部受理のまま保つこと（値は A30 の先端の `run -O0`・`-O3`）。

- 1 回目が拒否した（`derive_store_slots` が、`own 'a` が `ref i64` に具体化された値の引数に region を与えず、std の中で `E1013`/`store_region_mismatch`、`std/Array.tz:1720:29`・`1736:9`）:
  - `let a = 1`、`let b = 2`、`let mut arr = [ref a]`、`Array.set 0 (ref b) (ref mut arr)`、`deref arr[0]` → 2。`ref string` の要素（`Array.set 1 (ref b) (ref mut arr)` の `arr[1].length`）→ 3。
  - パイプライン `ref mut arr |> Array.set 0 (ref c)` → 323。`Array.fill 0 1 (ref c) (ref mut arr)` → 322。関数の中の `fn pick a b = let mut arr = [a]; Array.set 0 b (ref mut arr); deref arr[0]` → 7。
- 1 回目が拒否した（利用者の関数の定義の位置で `E1013`/`store_region_unnamed`）:
  - generic な書き込み関数 `def put :: own 'a -> ref mut ['a..] -> unit`、`fn put x arr = Array.write 0 x arr` を関数の要素（`put (\() -> 9) (ref mut a)` → 9）、参照の要素（→ 2）、`Array.set` の本体（→ 2）で使う。
  - 型変数を持たない書き込み関数 `def put :: own (unit -> i64) -> ref mut [(unit -> i64)..] -> unit` に借用する関数を渡す（`put (\() -> base) (ref mut arr)` → 4）。
- 1 回目が拒否した（std の中や部分適用）:
  - 関数を要素に持つ `MatrixView.write`・`fill`・`copy_from`・`map_in_place`（`store_region_unnamed`・`self_referential_store`）。`MatrixView.write 0 1 (\() -> 20)` から `ref mut grid |> MatrixView.write 0 0 (\() -> base)` までの 5 つの書き込みの後の `data[0] () + data[1] ()` → 1009。
  - std の書き込み関数の部分適用 `let w = Array.set 0 (\() -> 5)`、`w (ref mut arr)`（書き込む値が借用を持たなくても `region_callback_stores_exclusive_input`）→ 5。generic な `put` の部分適用 → 1。`MatrixView.write 0 0 (\() -> 2)` の部分適用 → 2。
  - 関数を要素に持つ `Array.sort_in_place_with`・`sort_in_place_by` は基準でも std の中で拒否される（`E1013`/`task_result_retains_borrow` と `E1014`/`borrow_conflict`、`std/Array.tz:2003`・`2084`・`2098`）が、診断が 1 つ増えた（`self_referential_store`）。同じ診断のままにする。
  - 名前付き region を宣言した利用者の書き込み関数のパイプライン形が受理に変わり、`tests/borrowed_records.rs` の `named_target_regions_keep_stored_borrows_alive` が失敗した（パイプラインの規則の範囲）。
- 2 回目が拒否した（`derive_store_slots` の利用者の関数の枝が、region を宣言した関数を飛ばし、型引数が loan を運ぶことを要求した。定義の位置の `E1013`/`store_region_unnamed`）:
  - `def g {r} :: own 'a -> ref mut {r} ['a..] -> unit`、`fn g x d = Array.write 0 x d`、`g (\() -> base) (ref mut data)` → 4。
  - std の形の `MatrixView` の包み関数 `def g {r} :: own 'a -> ref mut MatrixView.Mut<'a> {r} -> unit`（`MatrixView.write 0 0 x m`、`Copy<'a>` の `MatrixView.fill x m`）を関数の要素（→ 4）と参照の要素（→ 2）で使う。region なしの `def g :: own 'a -> ref mut MatrixView.Mut<'a> -> unit` は受理のまま（→ 4）。
  - `def put :: ref 'a -> ref mut [ref 'a..] -> unit`（`'a = i64`）→ 2。
  - `def store {s} :: ref {s} string -> ref mut {s} [ref {s} string..] -> unit = \x arr -> Array.write 0 x arr` → 3。スライスに参照先の region は書けない（`ref mut {r} [ref {s} string..] {s}` は `E1005`/`unsized_slice_type_position`、`ref mut {r} [ref {s} string..]` は `E1013`/`field_mixes_regions`）ので、契約で書かせる案は成り立たない。
- 2 回目の途中で見つかった回帰（`own_contents` で直したが、その後も sort で残った）: 書き込み先から読んだ要素を同じ先へ書き戻す（`let second = target[1]` の後の `Array.write 0 second target`、`rotate`・`put_last_first`・`map_in_place` の本体）が `E1013`/`self_referential_store`。基準ではどれも受理（→ 2）。
- 受理を広げた変更も混ざっていた（基準が拒否するプログラムを受理した）: `store_through` が外部の参照先への借用を持たない値の書き込みを何もしない（`Vec.push_mut (\() -> 1) v`、`v` は `ref mut` の引数。基準は `store_region_unnamed`）、generic な `def set_ref :: own 'a -> ref mut 'a -> unit`（`deref r = v`、基準は `Main.tz:2:18` `store_region_unnamed`）、generic な `Vec.push_mut` の包み関数（基準は `Main.tz:2:14` `store_region_unnamed`）。このチケットでは変えない（変えるなら別の判断）。
- そのほか: 派生の契約を持つ具体化は A28 の事実を失った。窓へ書き込んだ借用は窓にだけ残り、窓を使い終えた後に借用元をムーブして配列を使えた（2 回目で窓の後ろの持ち主へ届けて直した。D1 の方針でも同じ届け先が要る）。
- A28 の除外を外すときは、書き込む本体から持ち上げたラムダの読み通しも戻る（上の「ラムダの形」）。捕捉した値の loan も書き込みの事実で完全になっていることを、ラムダの形のテストで確かめる。
- A28 の `retained_parameters` と読み通しは、本文の結果の loan が完全であることを前提にする。書き込みの事実を A28 の事実より先に求め、両方の不動点を同じ呼び出しグラフで回すときは、片方の変化でもう片方の作業リストを戻す。

## 対象外

- 基準が拒否している書き込み（外部の参照先への `deref r = v` の `store_region_unnamed` など）を受理に変えること。
- 関数の型やスライスの型に、環境や要素の借用を表す region を書けるようにすること（言語の変更になる）。
- `Mutex`・`RwMutex` などのコールバックへ渡す排他参照の中身の追跡（中身は `Send` を要求され、借用を持つ値の扱いは別に調べる）。

## 決定事項

### D1: 契約を定義で検査せず、本文から求めた事実を呼び出し側で使う

- 決定: 書き込みを宣言の region の契約（`RegionSlots`）で表さず、特殊化した本文から「どの引数の借用がどの排他参照の先へ入りうるか」を求め、呼び出し側で書き込み先の持ち主へ足す。定義の位置では新しい診断を出さない。
- 理由: 型変数・関数の型・スライスの要素には region を書けないので、契約で表すと、どの形でも安全なプログラムを定義の位置で拒否する（「落とし穴」の 2 回分）。事実なら、拒否は残した借用が実際に破られる呼び出し側だけに出る。A28 の事実と同じ枠で求められる。
- 状態: 既定案（実装者はこの案に従う。成り立たなければ停止条件）。

### D2: 部分適用と関数値は保守的に扱い、拒否しない

- 決定: 関数値の呼び出しは、loan を運ぶ引数と関数値自身の借用を、排他参照のすべての引数の先へ残す。部分適用を `region_callback_stores_exclusive_input` で拒否しない。
- 理由: 基準は部分適用を受理している（`let w = Array.set 0 (\() -> 5)` など）。保守的に残すだけなら、拒否は残した借用を破る呼び出しに限られる。
- 状態: 既定案。
