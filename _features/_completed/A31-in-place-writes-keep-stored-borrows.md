# A31: `Array.write` と、それで書き込む std の関数が、書き込んだ値の借用を書き込み先の持ち主に残さない（借用元の解放後に読める）

| 項目 | 内容 |
| --- | --- |
| ID | A31 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | A13, A28 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-06（A28 のレビューで発見。A30 の先端 `f957a7d9`（PR #94）のバイナリで確認） |
| 承認 | 不要（健全性の穴を塞ぐ。今受理している危険なプログラムだけが `E1013`・`E1014` になる。今受理している安全なプログラムは変えない。変わるなら停止条件） |
| 改善する劣位 | 不具合（メモリ安全性）: `let mut arr = [\() -> 1]` に、局所の `text` を借用する関数を `Array.write 0 (\() -> String.length borrowed) (ref mut arr)` で書き込み、`arr[0]` を関数から返すと検査を通る。返した関数を呼ぶと解放した `text` を読み、9 ではなく不定の値を返す。`Array.set`・`Array.fill`・`Array.blit` など、`Array.write` で書き込む std の関数と、参照を要素に持つスライス（`ref string`）も同じ。`arr[0] = v` の代入なら正しく拒否される |
| 手本にする既存実装 | `src/closures.rs:446〜458`（`Vec.push_mut` の生成関数の `RegionSlots { writes: true }`）と `src/ownership.rs:2292` の `store_arguments`（呼び出し側が書き込んだ値の loan を書き込み先の持ち主へ足す）、`src/ownership.rs` の A28 の `parameter_retention`（callee の本文から呼び出しの事実を求める最小不動点。契約を定義で検査せず、呼び出し側で使う） |
| 主な影響ファイル | `src/ownership.rs`, `src/closures.rs`, `tests/mutable_slices.rs`, `tests/rc.rs`, `tests/fixtures/mutable_slices/Main.tz`, `tests/features.mjs`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `docs/language.md`, `docs/architecture.md`, `_features/README.md` |

## 目的

可変スライスや排他参照の先へ値を書き込む呼び出しが、`arr[i] = v` の代入と同じく、書き込んだ値の借用を書き込み先の持ち主に残すようにする。
そうすれば、借用元が書き込み先より先にムーブ・解放されるプログラムは `E1014`・`E1013` になり、安全なコードだけで解放後の読み出しが起きることはなくなる。

塞いだ後は、A28 が入れた保守的な除外（本体が書き込める場所（排他借用、借用を持ちうる値を入れた `Mutex` などの共有セル、それらを引数・結果に持つ関数値）をほかのコードへ渡す関数は事実を持たず、読み通しもしない。その中で作るラムダも、そうしたラムダを作る関数も同じ。`src/ownership.rs` の `lends_mutable`・`lending_bodies`・`CallFacts.lending`、`src/check.rs` の `Type::lends_mutable`・`is_mutable_cell`）を外せる。

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
  関数の境界を越えて穴が広がる（下の「A28 の事実が広げる形」）。A28 は `lending_bodies` で、そうした本体（書き込める場所をほかのコードへ渡す本体と、その中で作るラムダの本体）を A28 より前の規則に戻し、読み通しを、借用した場所の値が呼び出し元の loan を持たないときだけに限っている（「落とし穴」）。

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
- `h6_move_after`: トップレベルで書き込んだ後に借用元をムーブし、`arr[0] ()` を呼ぶ。受理され、`-O0`・`-O3` とも `0` を出力する（期待は 9。ムーブした後の `text` を読んでいる。借用中の値のムーブで、拒否されるべき）。プログラムは次のとおり。

  ```text
  let text = to_string 123456789
  let borrowed = ref text
  let mut arr = [\() -> 1]
  Array.write 0 (\() -> String.length borrowed) (ref mut arr)
  let moved = text
  arr[0] ()
  ```

  最後の行を `arr[0] () * 100 + moved.length` にすると `-O0`・`-O3` とも `9` を出力する。これも `arr[0] ()` が `0` を返した値（`0 * 100 + 9`。期待は `909`）で、解放後の読み出しである（以前の版のこのチケットは「9 を出力する」と書いていたが、誤り）。
- `m_lock`（`Mutex.with_lock` のコールバック）: 書き込み先は、コールバックの `ref mut` の引数で、持ち主は lock である。`Send` はこれを止めない。

  ```text
  def stash :: ref string -> (unit -> i64)
  fn stash text =
      let m = Mutex.create [\() -> 0]
      Mutex.with_lock (ref m) (\slot -> Array.write 0 (\() -> String.length text) slot)
      let arr = Mutex.into_inner m
      arr[0]
  def leak :: own i64 -> (unit -> i64)
  fn leak n =
      let text = to_string n
      stash (ref text)
  let f = leak 123456789
  f ()
  ```

  `-O0` は `0`、`-O3` は `9`。`stash` が `m` を返し（`def stash :: ref string -> Mutex<[(unit -> i64)]>`）、トップレベルで `Mutex.into_inner (leak 123456789)` の `arr[0] ()` を呼ぶ形（`m_lock_return`）は `-O0` が `1`、`-O3` が `9`。
  `RwMutex.create`・`RwMutex.with_write`・`RwMutex.into_inner` に置き換えた 2 つの形（`rw_write`・`rw_write_return`）も同じく受理され、`0`/`9` と `1`/`9` を出力する。
  `let c = Cond.create ()` の後の `Cond.wait_with (ref c) (ref m) (\_ -> true) (\slot -> Array.write 0 (\() -> String.length text) slot)` に置き換えた形（`cond_lock`）も受理され、`0`/`9` を出力する（A28 の 5 回目のレビュー対応で、A30 の先端と A28 の `04ab20fb` の両方で測った）。
- `tz_capture`（tzroutine の捕捉）: `start` の中で `let mut arr = [Arc.new (\() -> 0)]` に `Array.write 0 (Arc.new (\() -> String.length borrowed)) (ref mut arr)` で書き、`tzroutine { let s = Arc.share (ref arr[0]) ... }` が `(deref (Arc.get (ref s))) ()` を `Arc<Atomic<i64>>` の引数に足す。`start` が返った後（`text` の解放後）にトップレベルが完了を待って値を読む。受理され、`-O0` は `0`、`-O3` は `9`（期待は 9）。

  ```text
  def wait_for :: ref Arc<Atomic<i64>> -> i64 -> unit
  fn wait_for counter target =
      while Atomic.load (Arc.get counter) < target do
          Routine.yield ()
  def start :: own i64 -> ref Arc<Atomic<i64>> -> ref Arc<Atomic<i64>> -> unit
  fn start n result done =
      let text = to_string n
      let borrowed = ref text
      let mut arr = [Arc.new (\() -> 0)]
      Array.write 0 (Arc.new (\() -> String.length borrowed)) (ref mut arr)
      let mine = Arc.share result
      let finished = Arc.share done
      tzroutine {
          let s = Arc.share (ref arr[0])
          let _sum = Atomic.fetch_add (Arc.get (ref mine)) ((deref (Arc.get (ref s))) ())
          let _count = Atomic.fetch_add (Arc.get (ref finished)) 1i64
      }
  let result = Arc.new (Atomic.create 0i64)
  let done = Arc.new (Atomic.create 0i64)
  start 123456789 (ref result) (ref done)
  wait_for (ref done) 1
  Atomic.load (Arc.get (ref result))
  ```

`m_lock` 以降の 5 形の値は、A30 の先端 `f957a7d9` と A28 の `18635c2d` で同じ（A28 の 4 回目のレビュー対応で測った）。

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

lock のコールバックの形（A28 の 4 回目のレビューの判定で見つけた。除外が排他参照の引数だけを見ていた `d18ede5c` の A28 で受理された）。`Mutex.with_lock` は `ref Mutex` を共有参照で受け取り、中身の `ref mut` をコールバックへ貸すので、コールバックが関数値（部分適用 `put text`）なら、呼び出しの引数の型に排他参照が現れない:

```text
def put :: ref string -> ref mut [(unit -> i64)] -> unit
fn put text slot = Array.write 0 (\() -> String.length text) slot
def stash :: ref string -> ref string -> (unit -> i64)
fn stash text other =
    let m = Mutex.create [\() -> 0]
    Mutex.with_lock (ref m) (put text)
    let arr = Mutex.into_inner m
    let f = arr[0]
    \() -> f () + String.length other
def leak :: ref string -> own i64 -> (unit -> i64)
fn leak keep n =
    let text = to_string n
    stash (ref text) keep
let k = "ab"
let f = leak (ref k) 123456789
f ()
```

| 名前 | 変えた所（ほかは上と同じ） | A30 の先端 | `d18ede5c` の A28（`-O0`/`-O3`） |
| --- | --- | --- | --- |
| `m_partial` | 上のとおり | `13:5` `borrow_outlives_block` | `2`/`11`（期待は 11） |
| `rw_partial` | `RwMutex.create`・`RwMutex.with_write`・`RwMutex.into_inner` | `13:5` | `2`/`11` |
| `m_partial_set` | `put` の本体が `Array.set` | `13:5` | `2`/`11` |
| `m_partial_pipe` | `leak` が `keep \|> stash (ref text)` | `13:5` | `2`/`11` |
| `m_helper_fill` | helper `fn fill_lock text m = Mutex.with_lock m (put text)`（`ref Mutex<...>` の引数）を `fill_lock text (ref m)` で呼ぶ | `15:5` | `2`/`11` |
| `cond_partial` | `Cond.wait_with (ref c) (ref m) (\_ -> true) (put text)` | `14:5` | `2`/`11` |
| `record_mutex_helper` | `record Holder { m: Mutex<...>, n: i64 }` を `ref Holder` で helper へ渡し、helper が `Mutex.with_lock (ref holder.m) (put text)` | `16:5` | `2`/`11` |
| `arc_mutex_helper` | `Arc<Mutex<...>>` を `ref` で helper へ渡し、helper が `Mutex.with_lock (Arc.get am) (put text)`、`Arc.try_unwrap` で取り出す | `18:5` | `2`/`11` |

読み通しの形: `put` が `Arc.new (\() -> String.length text)` を書き、`make` が `Mutex.with_lock (ref m) (put (ref text))` の後に `Arc.share (ref arr[0])` を返す形（`rt_partial`）は、A30 の先端で `9:5` `borrow_outlives_block`、`d18ede5c` は受理して `-O0` で `4304867260`（`-O3` は `9`）。helper を通す形（`rt_helper`）は `11:5`、`Cond.wait_with` の形（`cond_rt`）は `10:5`（どちらも `borrow_outlives_block`。`d18ede5c` は受理して不定の値）。共有を `start` の `tzroutine { }` が捕捉し、`start` が返った後に読む形（`rt_tz`）は A30 の先端で `16:5` `task_capture_retains_borrow`、`d18ede5c` は受理して `0`/`9`。
A28 は 5 回目の対応（`04ab20fb`）で、除外を書き込む関数の列挙に依らない形に広げ、どれも A30 の先端と同じ位置・code・kind に戻した（`tests/rc.rs` `a_lock_callback_lends_a_way_to_write`）。`text` が生きている間に呼ぶ対照（`m_partial_ctl` `11`、`rt_partial_ctl` `9`）は受理のまま。

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
| `src/ownership.rs`・`src/check.rs` | `lends_mutable`・`lending_bodies`・`CallFacts.lending`・`Contract.lends_mutable`・`body_lends_mutable`、`Type::lends_mutable`・`is_mutable_cell` | 書き込みの事実がそろった後に外し、A28 のテストがそのまま通ることを確かめる |
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

- 先に外すもの（A28 の 8 回目のレビュー対応で入った、プログラム全体の条件）: `src/ownership.rs` の `untracked_loan_store`・`untracked_writer`・`Lending::untracked`。借用を持ちうる値をこれらの組み込み関数で格納する関数がプログラムのどこかにあると、A28 はプログラム全体で事実も読み通しも使わない。この手順で `Array.write`・`SyncMap.__replace` の格納を追跡したら、その 2 つを `untracked_writer` から外し、条件が何も見つけないことを確かめる（A28 の再現と「A28 の事実が広げる形」が、書き込みの事実で拒否のままになる）。下の局所の除外は、その後に外す。
- 変更: `src/ownership.rs` の `lending_bodies`（と `lends_mutable`・`CallFacts.lending`・`Contract.lends_mutable`）、`src/check.rs` の `Type::lends_mutable`・`is_mutable_cell` と、それを説明する文書（「ドキュメント」）。
- 確認: `tests/mutable_slices.rs` の `a_function_that_writes_in_place_keeps_its_arguments_loans` と `tests/rc.rs` の `a_share_of_an_element_holds_what_was_written_in` が、拒否の理由を書き込みの事実に変えても成功する。

### 手順 5: 既存のプログラムが変わらないこと

- 確認: `tests/fixtures/*`・`examples/*` の `check --json` と `build --no-cache --emit llvm` が基準と一致する。「落とし穴」の一覧が基準と同じ `check --json` と実行結果になる。

### 手順 6: ドキュメントと最終確認

- 「ドキュメント」の各ページ。T2。

見えないコードの形（A28 の 5 回目のレビューの判定で見つけた。`ae455566` の A28 で受理された）。関数の型・dyn 型・`Owned.Function` は環境を表さないので、書き込みの関数値 `w` と読み出しの関数値 `r` が同じ `Arc<Mutex<...>>` を捕捉していても、`pass` の引数の型には可変セルが現れない:

```text
def writer :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)
fn writer a = \g -> Mutex.with_lock (Arc.get (ref a)) (\slot -> Array.write 0 g slot)
def reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)
fn reader a = \() -> Mutex.with_lock (Arc.get (ref a)) (\slot -> slot[0] ())
def pass :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> ref string -> (unit -> i64)
fn pass w r text other =
    w (\() -> String.length text)
    \() -> r () + String.length other
def leak :: own ((unit -> i64) -> unit) -> own (unit -> i64) -> ref string -> own i64 -> (unit -> i64)
fn leak w r keep n =
    let text = to_string n
    pass w r (ref text) keep
def run :: unit -> i64
fn run u =
    let a = Arc.new (Mutex.create [\() -> 0])
    let a1 = Arc.share (ref a)
    let a2 = Arc.share (ref a)
    let k = "ab"
    let w = writer a1
    let r = reader a2
    let f = leak w r (ref k) 123456789
    f ()
run ()
```

| 名前 | 変えた所（ほかは上と同じ） | A30 の先端 | `ae455566` の A28（`-O0`/`-O3`） |
| --- | --- | --- | --- |
| `mx_leak` | 上のとおり | `12:5` `borrow_outlives_block` | 不定の値/`11`（期待は 11） |
| `rw_leak` | `RwMutex` と `RwMutex.with_write` | `12:5` | 不定の値/`11` |
| `mxi_leak` | `w`・`r` を `run` の中のラムダで書き、`Arc` を捕捉する | `8:5` | 不定の値/`14` |
| `owned_call` | `w` が `ref Owned.Function<Maybe<(unit -> i64)>, i64>`（`Some g` で書き、`None` で読む）、`Owned.call w (Maybe.Some (...))` | `8:5` | 不定の値/`11` |
| `array_iter`・`array_map`・`array_fold` | `pass` が `Array.iter w (ref arr)`・`Array.map w (ref arr)`・`Array.fold w 0 (ref arr)` で `w` を呼ぶ | `13:5` | 不定の値/`11` |
| `field_partial` | `record Sink { put: (unit -> i64) -> unit }` に部分適用 `store a1` を入れ、`s.put (...)` | `13:5` | 不定の値/`11` |
| `thunk_env` | `leak` が `\() -> w (\() -> String.length borrowed)` を作り、`pass` が `t ()` で呼ぶ | `13:5` | 不定の値/`9` |
| `dyn_generic` | `class Sink` の `put`・`read`、`record Cell {r} { m: ref {r} Mutex<...> }` の `dyn Sinks.Sink {r}` を generic な `pass` へ渡す | `15:5` | `SIGABRT` |

A28 は 6 回目の対応で、本文が借用を持ちうる値を見えないコードへ渡す本体を除外し、どれも A30 の先端と同じ位置・code・kind に戻した（`tests/rc.rs` `an_opaque_call_hands_a_borrow_to_unseen_code`）。この除外は、手順 4 で外す除外ではない: 書き込みは呼び出し元に見えないコードの中で起き、書き込みの事実は関数の型を通して呼び出し元へ届かないので、A31 の後も残す（「落とし穴」）。

暗黙の解放の形（A28 の 6 回目のレビューの判定で見つけた。`2cbc512a` の A28 で受理された）。値の破棄は本文に呼び出しを書かずに `Drop.drop` を実行し、`Drop.drop` は破棄する値の中身を、値の中の `Arc<Mutex<...>>`（呼び出し元の `r` も読む）へ `Array.write` で入れられる:

```text
record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }
instance Drop<Stash> {
    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\slot -> Array.write 0 g slot) }
}
def reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)
fn reader a = \() -> Mutex.with_lock (Arc.get (ref a)) (\slot -> slot[0] ())
def pass :: own Stash -> own (unit -> i64) -> (unit -> i64)
fn pass _s r = r
def leak :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> own i64 -> (unit -> i64)
fn leak a r n =
    let text = to_string n
    let borrowed = ref text
    pass (Stash { cell: a, f: \() -> String.length borrowed }) r
def run :: unit -> i64
fn run u =
    let a = Arc.new (Mutex.create [\() -> 0])
    let a1 = Arc.share (ref a)
    let a2 = Arc.share (ref a)
    let r = reader a2
    let f = leak a1 r 123456789
    let junk = [to_string 987654321, to_string 111111111, to_string 222222222]
    f () + junk.length
run ()
```

| 名前 | 変えた所（ほかは上と同じ） | A30 の先端 | `2cbc512a` の A28（`-O0`/`-O3`） |
| --- | --- | --- | --- |
| `drop_unused` | 上のとおり | `13:5` `borrow_outlives_block` | 不定の値/`12`（期待は 12） |
| `drop_param` | `pass` が `let _held = s` | `15:5` | 不定の値/`12` |
| `drop_local` | `pass` が引数 `a` と `ref string` の `text` から `Stash` を作る | `14:5` | 不定の値/`12` |
| `drop_reassign` | `let mut held = s` の後に `held = t` | `16:5` | 不定の値/`12` |
| `drop_nested_array` | `record Outer { items: [Stash] }`（`Drop` を持たない） | `14:5` | 不定の値/`12` |
| `drop_rc_repro` | `fn pass _s a = a` が別の `Arc` 引数を返す | `11:5` | 不定の値/`12` |
| `drop_opaque_field` | `record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }`、`Drop.drop` が `w g` を呼ぶ（セルを持たない） | `15:5` | 不定の値/`12` |
| `owned_drop_opaque` | 上の `Stash` を `Owned.drop s` で解放する | `17:5` | 不定の値/`12` |
| `drop_consume_opaque` | 上の `Stash` を `fn consume _s = ()` へ渡す `pass s r = { consume s; r }` | `19:5` | （確かめていない） |

A28 は 7 回目の対応で、プログラムのどれかの `Drop.drop` が事実を持たない（書き込める場所を渡すか、見えないコードへ渡す: `Lending::drops_store`）とき、破棄でコードを実行しうる型の値が持つ引数の借用を結果に残し、そうした値で借用を持つものを破棄する本体では読み通さないようにして、どれも A30 の先端と同じ位置・code・kind に戻した（`tests/rc.rs` `an_implicit_drop_keeps_what_the_dropped_value_holds`）。同じ本体の中で `Drop` の解放が書き込む形（`drop_inleak`: `leak` が `Stash` を作って `r` を返す）は A30 の先端でも受理され、不定の値を出す（この穴そのもの）。この A28 の規則は、`Drop.drop` の中の書き込みの事実が呼び出し元へ届けば外せる（書き込みは `Drop.drop` の本文の中で起き、`Drop.drop` は名前付きの関数なので、手順 3 の事実の対象になる）。ただし、`Drop.drop` が関数値のフィールドを呼ぶ形（`drop_opaque_field`）は見えないコードへの受け渡しなので、外さない。

## テスト計画

### Rust テスト

- 拒否: 「再現」の `h1`〜`h5`・`h7` が `E1013`/`borrow_outlives_block`、`h9` が `E1013`/`assignment_extends_borrow`、`h6` が `E1014`/`borrow_conflict`。
  「A28 の事実が広げる形」の 6 形と読み通しの 2 形が `E1013`/`borrow_outlives_block`、ラムダの 3 形が `E1013`/`task_result_retains_borrow`（どれも A30 の先端と同じ位置）。
  「再現」の `m_lock`・`m_lock_return`・`rw_write`・`rw_write_return`・`cond_lock`・`tz_capture` が `E1013`（kind は実装で決め、このチケットに記録する）。
  「A28 の事実が広げる形」の lock のコールバックの 8 形と読み通しの 4 形が、A30 の先端と同じ位置・code・kind（`tests/rc.rs` `a_lock_callback_lends_a_way_to_write`。除外を外した後は書き込みの事実で）。
  「A28 の事実が広げる形」の見えないコードの 10 形が、A30 の先端と同じ位置・code・kind（`tests/rc.rs` `an_opaque_call_hands_a_borrow_to_unseen_code`。見えないコードの除外で。手順 4 の後も同じ）。
  「A28 の事実が広げる形」の暗黙の解放の 9 形が、A30 の先端と同じ位置・code・kind（`tests/rc.rs` `an_implicit_drop_keeps_what_the_dropped_value_holds`。`Drop.drop` の書き込みの事実で外す場合も、関数値を呼ぶ `Drop.drop` の形は見えないコードの除外で）。
  利用者の generic な書き込み関数 `def put :: own 'a -> ref mut ['a..] -> unit` で借用する関数を書き、借用元をムーブする形が `E1014`。`MatrixView.write` で窓へ書き、窓を使い終えた後に借用元をムーブして配列を使う形が `E1014`。
- 受理（今も受理。値は基準と同じ）: 「落とし穴」の一覧のすべて。

### E2E

- `tests/fixtures/mutable_slices` の `reference_writes`・`generic_writes`・`matrix_writes`（A28 が足した、基準も受理する形）が、native と WASM の `-O0`・`-O3` で同じ値（233723・91052425・20200076872）。

### 既存テストへの影響

- 拒否が増えるのは、今まで危険だったプログラムだけ（手順 5）。

### 性能

- 検査だけ。`TSUZURI_TIME_PASSES=1 tsuzuri check` で、`hash_map`・`list_fsharp`・`array_fsharp`・`vec_dotnet`・`rc`・`tzroutine` の fixture と `examples/tzroutines` の中央値が 10 % を超えて増えないこと（共有の CI には閾値を入れない）。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md` の「現在の制限」: 既知の不具合の注意（A31 へのリンク）を、実際の振る舞いに置き換える。書き込める場所をほかのコードへ渡す関数の除外の文も消す。
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
- A28 の読み通しは、借用した場所の値が呼び出し元の loan（外部 loan）を持つとき（引数、持ち上げたラムダの捕捉、それらを受け取った局所値）は使わない（A28 の 4 回目のレビュー対応、`18635c2d`）。外部 loan は呼び出し元の値の loan を表し、呼び出し元が `Array.write` で書いた借用を欠きうるからで、`fn wrap arr = Arc.share (ref arr[0])`（`own [Arc<unit -> i64>]`）を、`arr` へ借用する値を書き込んだ本体が呼ぶと、読み通す版の A28 は受理し、`-O0` で解放後の値を出力した（A30 の先端は `Main.tz:2:15` `E1013`/`task_result_retains_borrow`）。`Rc`、helper と呼び出し側のパイプライン、generic な helper、レコードの引数、引数を捕捉するラムダ、`let a = arr` を経た形も同じ。書き込みの事実で呼び出し元の loan が完全になった後に、この制限も外せる。外すときは、`tests/rc.rs` `a_share_of_an_element_holds_what_was_written_in` の helper の形が書き込みの事実で拒否のまま、書き込みのない形（`wrap [Arc.new (\() -> 41)]`、捕捉した `arr` のラムダ）が受理に変わることを確かめる。
- A28 の除外（5 回目のレビュー対応、`04ab20fb`）は、書き込む関数を列挙しない: 呼び出しの引数・パイプラインの左辺・演算子の被演算子・ラムダの捕捉の型が、格納された値・参照先・`Rc`／`Arc` の中身・関数型の引数と結果の型をたどって、排他参照か、型引数が loan を運ぶ共有セル（`Mutex`・`RwMutex`・`OnceValue`・`Atomic`・`SyncMap`・チャネル・`ErrGroup`）に届けば、その本体を除外する。外すときは、書き込みの事実が、関数値のコールバック（部分適用 `put text`）と、`ref Mutex` を受け取って lock する helper（`fill_lock`、レコードや `Arc` の中の `Mutex` を含む）を通しても lock の持ち主へ届くことを、「A28 の事実が広げる形」の lock の形で確かめる。除外の文言どおりに「排他参照の引数」だけを見ると、この 4 回目までの A28 と同じ穴になる。除外は、書き込む値を受け取らない `Mutex` の受け渡しでも効くので、所有の値を書く名前付き関数をコールバックにする形（`Mutex.with_lock (ref m) writer`、`7` を出力）は A30 の先端の誤った拒否（`13:5` `borrow_outlives_block`）のままである。
- A28 の除外を外すときは、書き込む本体から持ち上げたラムダの読み通しも戻る（上の「ラムダの形」）。捕捉した値の loan も書き込みの事実で完全になっていることを、ラムダの形のテストで確かめる。
- A28 の `retained_parameters` と読み通しは、本文の結果の loan が完全であることを前提にする。書き込みの事実を A28 の事実より先に求め、両方の不動点を同じ呼び出しグラフで回すときは、片方の変化でもう片方の作業リストを戻す。
- A28 の 7 回目の対応（`Lending::drops_store`）は、追跡されない格納をしうる `Drop.drop` があるプログラムだけで、破棄でコードを実行しうる型（関数値・dyn 値・`Owned.Function` を含む: 環境が `Arc` に入れた `Drop` 型の値を持ちうる）の値の借用を結果に残す。手順 4 で外すときは、関数値の環境や dyn 値を通した解放（`Arc` を捕捉した関数値の破棄）でも、書き込みの事実が呼び出し元へ届くことを確かめる。届かない形は除外を残す。
- A28 の 6 回目の対応の除外（本文が借用を持ちうる値を、名前付き関数でない関数値・dyn のメソッド・`Owned.call`・コールバックを実行する組み込み関数・そうした呼び出しをする関数へ渡す本体は、事実を持たず読み通さない）は、手順 4 で外さない。書き込みの事実は callee の本文から求めるが、関数の型・dyn 型・`Owned.Function` は環境を表さないので、`pass w r text` の `w` が捕捉した `Mutex` へ書いた借用は、`pass` の呼び出し元にも `pass` の本文にも見えない（「A28 の事実が広げる形」の見えないコードの形）。

## 対象外

- 基準が拒否している書き込み（外部の参照先への `deref r = v` の `store_region_unnamed` など）を受理に変えること。
- 関数の型やスライスの型に、環境や要素の借用を表す region を書けるようにすること（言語の変更になる）。
- `Mutex`・`RwMutex` のコールバックの引数（`ref mut` の中身）への `Array.write` は対象外ではない（「再現」の `m_lock`・`rw_write`）。書き込みの事実は、コールバックの `ref mut` の引数から、`with_lock`・`with_write` の呼び出しを通して lock の持ち主（`m`）へ届ける。`Cond.wait_with` も同じ形で確かめた（「再現」の `cond_lock`、「A28 の事実が広げる形」の `cond_partial`・`cond_rt`）ので、同じく届ける。

## 着手時の再検討（2026-11-08、HEAD b3dd7f1c）

A28（PR #100）のマージ後の HEAD で、コードと「再現」を確かめ直しました。基準のバイナリは HEAD（`target/scratch/base/tsuzuri`、dev build）です。
154 個のプログラムの結果と本文は `target/scratch/a31/baseline.md`（監査の `target/scratch/audit-A31/` の写し）にあり、これを手順 1 の記録にします（末尾の「手順 1: ベースラインの記録」）。
この節は、監査（`target/scratch/audit/A31.json`・`A31-revision.md`）とオーケストレーターの決定（`target/scratch/audit/DECISIONS.md`）から作りました。上の古い記述は残しますが、食い違うところはこの節が優先します。

### 進め方の決定

- GUIDE §13 の停止条件と、このチケットの「停止条件」は、次に置き換えます: 健全性を保ち、今受理している安全なプログラムをすべて受理のまま保つ、最も保守的な選択を取り、その判断をこのチケットに記録して続けます。テストの期待を弱めて通すことはせず、危険なプログラムを受理することもしません。
- 優先順位: (1) このチケットとベースラインの危険なプログラムをすべて拒否する。(2) 受理している安全なプログラムを同じ値のまま受理する。(3) A28 の除外をできるだけ外す。(1)・(2) を壊さずに外せない除外は残し、このチケットと文書に記録します。
- `drop_inleak` を塞げないときは、保守的な門で拒否のままにします（受理しません）。新しく見つけた穴を黙って受理のまま残すことはしません。
- 共有の数（診断の kind の数・`Builtin::ALL`・テストの件数など）は、分岐の先端で数えます。

### 位置の移動（意味は同じ）

`src/ownership.rs` の位置は次のとおりです。

- 基本: `store_arguments` 3709、`store_through` 3593、`target_loans` 3655、`input_slot_loans` 3677、`eval_call` 4227（`store_arguments` の呼び出し 4403、closed と unit による loan の消去 4404〜4410）、`closed`・`closed_returns` 645・716
- A28 の事実: `parameter_retention_within` 854、`retained_parameters` 1011、`piped_callee` 2251、`read_through` 2348、パイプラインの評価 4892〜4950
- A28 の除外: `lends_mutable` 1205、`opaque_calls` 1365、`Lending` 1541、`lending_bodies` 1587、`untracked_writer` 1649、`untracked_loan_store` 1677、`drops_stop_reads` 1961、`collect_drops` 4010

`src/check.rs` は `drop_runs_code` 739、`is_mutable_cell` 918、`Type::lends_mutable` 946、`Array.write` の scheme 3196、`SyncMap.__replace` の scheme 4226 です。`src/closures.rs:446〜458` と `std/Array.tz` の 1719・1731・1750 は変わっていません。

### 結果が変わっていないもの

- 「再現」は HEAD でもすべて受理され、不定の値を出します（値はチケットどおり）。
- 「A28 の事実が広げる形」は、表と同じ位置・code・kind で拒否されます。主な理由は A28 の 8 回目のプログラム全体の条件（`Lending::untracked`）です。

### 計画に効く食い違い

1. **m_lock 系は `closed_returns` で受理されている。** m_lock・m_lock_return・rw_write・rw_write_return・cond_lock（と `tests/currying.rs` の直接の呼び出しの形）が対象です。`let m` は `let mut` でないので closed とされ、`eval_call` が結果の loan を消します（4404〜4410）。`let mut m` にしただけの形（`m_lock_mut_probe`）は HEAD でも `10:5` で拒否されます。lock の持ち主へ loan を足すだけでは塞がりません。
2. **外部の書き込み先へ局所の借用を書く形は、定義の位置で拒否するしかない。** `N_local_into_param` は callee の局所値の借用を `ref mut` の引数へ書く形で、HEAD で受理され、解放後の値を読みます。呼び出し側の事実では表せません（仕様の「書き込み先が外部のとき定義の位置では何も拒否しない」と食い違います）。
3. **D2 の文言どおりだと安全な形を拒否する。** 関数値の引数へ読むだけのラムダを渡す `N_fnvalue_read_then_move` と、lock の生成関数の事実を使うときの `N_lock_read_then_move`・`N_partial_read_then_move` が `E1014` になります。
4. **書き込み先は排他参照の引数だけではない。** lock の helper は `ref Mutex`・`ref Holder`・`ref Arc<Mutex<...>>`・`own Arc<Mutex<...>>` で受け取ります。また `ref [T]` の仮引数の外部 loan は 1 つだけなので、「参照そのもの」と「参照先の中身」を loan では区別できません。参照そのものを格納すると、`N_blit_then_mutate_source`（12）が `E1014` になります。
5. **`Lending` には残す除外も入っている。** 見えないコードの除外と `drops_store` を含むので、`CallFacts.lending` を丸ごとは外せません。また中身がセルの `Rc`/`Arc` を読み通すと、後の書き込みが読み通した写しに届きません（`N_arc_cell_write_alias`）。
6. **`Drop.drop` の書き込みの事実を呼び出しの位置で使う手段がない。** 暗黙の解放には呼び出しの位置が無く、`retention_eligible` も `Drop.drop` を除きます。書き込み先は値の中の共有された `Arc<Mutex<...>>` です。`drops_store` は外せず、`drop_inleak`（HEAD で受理、不定の値）には別の手段が要ります。
7. **8 回目のフィールドへの代入の 13 形（`F_fa_*`・`xmod_fa`）がチケットに無い。** どれも `untracked` だけで拒否されています（`Checker::dropped` は、作った後のフィールドへの代入 `held.t = t` を数えません）。
8. **`untracked` を外すとテストの期待が戻る（推定）。** `D_ctl_drop_no_loan` は受理（5）、`W_upgrade` は `3:5` `task_result_retains_borrow`、`tests/currying.rs` の 3 つめの拒否は受理、`retention_tests::an_untracked_store_anywhere_leaves_no_facts` は不要になります。「A28 のテストがそのまま通る」は文字どおりには成り立ちません。
9. **パイプラインは `piped_callee` を流用できない。** `piped_callee` は `$builtin` の生成関数と unit の結果を除くので、`ref mut arr |> Array.write ...` に当たりません。パイプラインは `store_arguments` も呼びません。
10. **不動点の循環は起きない。** 書き込みの事実と A28 の事実はどちらも単調なので、合同の不動点は必ず止まります。危険なのは予算の打ち切りで、書き込みの事実の安全な既定値は全組（top）です。
11. **監査の Value/Contents の型の規則は健全でない（この再検討で見つけた）。** 監査は「格納した値の型が仮引数の参照の型そのものでなければ Contents」としましたが、`fn put text slot = Array.write 0 (\() -> String.length text) slot` の格納する値は `unit -> i64` で、`text : ref string` の参照そのものを捕捉しています。Contents にすると呼び出し側は `text` の参照先の中身（文字列は何も借用しない）しか格納せず、`m_partial` が受理に戻ります。区別は型ではなく、値がその loan をどう得たか（参照を通して読んだか）で決めます（下の「事実の形」）。

### 更新した計画（D1 は保つ）

#### 事実の形

`CallFacts` に関数ごとの書き込みの事実 `stores: Vec<StoreFacts>` を足します。`StoreFacts` は次を持ちます。

- `stores: BTreeSet<(from, Part, to)>`: 仮引数 `from` の loan が、仮引数 `to` の書き込み先へ入りうる。`Part::Value` は引数の値そのものの loan（呼び出し側は引数の loan）、`Part::Contents` は引数の参照先の中身の loan（呼び出し側は `target_loans`）です。
- `calls: BTreeSet<HigherOrderCall>`: 関数値の仮引数を、どの仮引数（の値・中身・書き込み先）を渡して呼ぶか（高階の事実）。呼び出し元で実引数の事実に置き換えます。
- `top: bool`: 分からない（予算の打ち切り、本文の検査の失敗）。loan を運ぶすべての引数が、書き込み先になりうるすべての引数へ入りうる。

`to` は、型が排他参照か、loan を運ぶ可変セル（`is_mutable_cell`。`Rc`/`Arc`・参照・レコードの中を含む）に届く仮引数です。
`from` の Part は値の来歴で決めます: `Value` に、参照の仮引数を通して読んだ loan の集合 `through` を足し（`read_places` の外部の枝で入れ、`join`・局所値の読み出し・捕捉・配列・レコード・呼び出しの結果へ伝える）、格納する値がその loan を `through` としてだけ持つなら `Contents`、ほかは `Value` です（伝え損ねた所は `Value` になり、保守的です）。
同じ仮引数の `Contents` から自分への組（`rotate`・`put_last_first`・`sort`・`map_in_place` の書き戻し）は捨てます。

#### 不動点

`parameter_retention_within` を、closed・保持・書き込みの事実の合同の最小不動点にします。

- 本文ごとに一度検査して、保持（今の `retained_parameters`。`Lending::no_facts` の本文は `None`）と書き込みの事実（`no_facts` の本文でも求める）を返します。検査に失敗した本文は保持 `None`・書き込み `top` です。
- 種: `Array.write` は (1, Value)→2、`SyncMap.__replace` は (0, Value)→1（格納する型が loan を運ぶときだけ。組み込み関数の直接の呼び出しと、その `$builtin` の生成関数）。
- 呼び出し元: 完全な直接呼び出しとパイプラインの完成（結果の型を見ない）、lock のコールバックの位置と関数値の引数の位置の名前付き関数・部分適用・持ち上げたラムダ。どれかの事実が変われば、その関数を挙げる関数を作業リストへ戻します。
- 打ち切り: 未確定の関数は保持 `None`・書き込み `top`、closed の取り消しの候補は取り消し（closed でない）にします。
- 抽象の本文（infer）: 型変数を loan を持たないものと数えます（Copy の制約と IR を変えない）。
- 前処理: 書き込みうる関数（loan を運ぶ型を格納する `Array.write`・`SyncMap.__replace`、lock の組み込み関数、書き込み先の引数を持つ見えない呼び出し、書き込みうる関数を挙げる関数）だけを検査し、ほかは空の事実にします。

#### closed

`closed_returns` の証明は、局所値の loan が束縛の後に増えないことを前提にします。セルへの書き込み（`let m` の `Mutex` へのコールバックの書き込み）はこれを破ります。
closed の関数のうち、結果の型が loan を運び（`owned` でない）、書き込みうる本文を持つものを合同の不動点の候補にし、本文の検査の結果が仮引数の loan を持てば closed を取り消します（単調）。`N_closed_cell_read` の `make` は結果が何も借用しないので closed のままです。
パイプラインの closed の門（4934 の `!lending.untracked`）は、この取り消しの後に外します。

#### 適用

`eval_call` の境界（`store_arguments` と同じ所、closed と unit で loan を消す前）と、新しいパイプラインの完成判定（`piped_application` で callee と束縛した引数を取り、仮引数の数で完全適用を判定。結果の型と `retention_eligible` は見ない）で、組ごとに格納します。

- 書き込み先の解決: `to` の引数の値の loan のうち、排他の loan と、場所の型が loan を運ぶ可変セルに届く loan が書き込み先です。
- 書き込み先が局所: `store_through` の局所の枝で持ち主へ足します。持ち主が持つ排他借用（`MatrixView.Mut` の窓、`split_at_mut` の半分）と、中身がセルの `Rc`/`Arc` の借用（外部 loan を含む）の先へも推移的に届けます。
- 書き込み先が外部: その本文の事実として記録します（`E::Assign` の `store_region_unnamed` の枝は変えません）。格納する loan が本文の局所値に根を持つなら、既存の `E1013`/`store_local_borrow_through_reference` にします（書き込んだ呼び出しの位置。下の「遅らせる診断」）。

#### lock のコールバック

`Mutex.with_lock`（lock 0・コールバック 1）、`RwMutex.with_write`（0・1）、`Cond.wait_with`（1・3）は、`locked_result` と同じ判定で見つけ、生成関数の事実を使わず、コールバックの実引数の事実を使います。

- 持ち上げたラムダ `E::Closure(id, captures)`: 捕捉の仮引数を捕捉の値へ、最後の `ref mut` の仮引数を lock の引数の書き込み先へ対応させます。捕捉ごとの値は、コールバックの実引数を捕捉ごとに評価して得ます。
- 部分適用: 束縛した引数を前の仮引数へ。名前付き関数はそのまま。
- 見えない関数値（局所値・フィールド・要素・呼び出しの結果・dyn）のときだけ、下の保守的な規則で関数値の loan を lock の持ち主へ残します。

#### 高階の事実と D2 の範囲

関数値の仮引数の呼び出しは、高階の事実として呼び出し元へ伝え、実引数がラムダ・名前付き関数・部分適用なら、その事実で置き換えます（`N_fnvalue_read_then_move` の受理のため）。実引数が呼び出し元の関数値の仮引数なら、呼び出し元の高階の事実にします。
本当に見えない関数値（フィールド・要素・呼び出しの結果・局所値の関数値・dyn・より多くの引数を受けた結果）の呼び出しだけを、保守的な規則にします: 関数値と loan を運ぶ引数の loan を、排他の値として持つ loan とセルに届く loan のすべての先へ残します（`through` としてだけ持つ loan は書き込み先にしません）。
高階の呼び出しへ本文の局所値の借用を渡すときは、その引数に「局所」の印を付け、呼び出し元で置き換えた事実がそれを書き込み先へ格納するなら、呼び出し元の呼び出しの位置で `store_local_borrow_through_reference` にします。

#### 暗黙の解放

- `store_through` の局所の枝と、`E::Assign` の局所の枝（`via` が空の `join`）で、持ち主の型が `drop_runs_code` なら、格納した loan を `dropped` に足します（`F_fa_*` 12 形と `xmod_fa` を、`untracked` なしで拒否のまま保ちます）。
- `drop_inleak` のフックを足します: ブロックの終わり（移動していない局所値）、局所値全体への再代入、`Owned.drop`、値で callee へ渡すこと（callee が解放しうる）で、型が書き込む `Drop.drop`（書き込みの事実か見えない呼び出しを持つ）の値を解放するとき、その値の loan のうち場所の型がセルに届くものを書き込み先、値の loan のすべてを格納するものとして、上の「適用」と同じく扱います。
  局所の書き込み先には `store_through` で足し、外部の書き込み先へ本文の局所値に根を持つ loan を格納するなら `E1013`/`store_local_borrow_through_reference` にします。位置は、ブロックの終わりと再代入では解放する局所値の宣言、ほかは呼び出しの位置です。
- フックは呼び出しの事実を作りません。callee の中の解放は、A28 の `dropped`（解放でコードを実行しうる値の loan を結果に残す）が呼び出し元へ伝えます。

#### 遅らせる診断

`store_local_borrow_through_reference` は、見つけた所で返さず、本文の評価が成功した後に、最初に見つけたものを返します。
`an_implicit_drop_keeps_what_the_dropped_value_holds` の 10 形は、callee へ値で渡す所でもこの診断の条件を満たしますが、同じ本文の末尾の `borrow_outlives_block`（A28 の `dropped` による）が先に見つかるので、位置・code・kind は変わりません。

#### 手順 4（外す除外と残す除外）

- 外す: `untracked_loan_store`・`untracked_writer`・`Lending::untracked`（`ArrayWrite`・`SyncMapReplace` を追跡し、残りの書き込み関数が loan を運ぶ値を格納しえないことを確かめてから）、パイプラインの closed の門、排他参照の lends（事実と読み通し）、セルの lends（事実だけ）。
- 残す: 見えないコードの除外（`opaque_calls`・`opaque_bodies`、`Lending` の構造）、中身がセルの `Rc`/`Arc` を読み通さない型の規則（`is_mutable_cell` と、`Type::lends_mutable` を狭めたもの）、`drops_store`・`dropped`・`drops_stop_reads`（`drops_store` は `Drop.drop` の書き込みの事実か見えない呼び出しから求め直す）、読み通しの外部 loan の制限（4 回目）、`retention_applies`。
- 外せない除外があれば残し、理由をこのチケットと文書に書きます。

### 実装手順の改訂

- 手順 1: 済み（末尾の記録）。道具は `target/scratch/a31/_tools/`（`run.mjs` は `BIN`・`OUT`・`EDITION`・`JOBS` を読み、どのバイナリでも 154 個を回せる。`compare.mjs` が基準と分岐の結果を、下の期待で突き合わせる）。
- 手順 2: 失敗するテストを先に足す（下の「テスト計画の改訂」）。拒否のテストだけが失敗することを確かめる。
- 手順 3a: 書き込みの事実（事実の形・種・不動点・適用・パイプライン・lock のコールバック・高階の事実・定義の位置の診断・窓）と closed の取り消し。
- 手順 3b: 暗黙の解放（`dropped` の追加と `drop_inleak` のフック）。
- 手順 4: 上の「外す除外」。外すたびに A28 の各回のテスト（`F_fa_*`、`drop_*`、lock の形、見えないコードの形）を確かめる。
- 手順 5・6: 変わらない（`tests/fixtures/*`・`examples/*` の `check --json` と `build --no-cache --emit llvm` を基準と比べる。`compare.mjs` が 154 個の期待を満たす。文書と T2）。

### テスト計画の改訂

拒否（今は受理されるので手順 2 で失敗する）。期待は、同じ場所の書き込みを代入（`arr[0] = ...`。lock は `let mut m`）にした双子のプログラムに基準のバイナリが出す診断で、仕様の「`arr[i] = v` と同じ規則」から決めました。どれも edition 2026 と 2027 で同じです。

| テスト | プログラム | 期待（最初の診断） |
| --- | --- | --- |
| `tests/mutable_slices.rs` `an_in_place_write_keeps_the_written_borrow_in_the_owner` | `h1`〜`h4`・`h7` | `E1013`/`borrow_outlives_block` `7:5` |
| 同上 | `h9` | `E1013`/`assignment_extends_borrow` `4:19` |
| `a_written_borrow_keeps_its_source_from_moving` | `h6`・`h6b` | `E1014`/`borrow_conflict` `5:13` |
| 同上 | `N_generic_put_move`・`N_owned_value_then_move` | `E1014`/`borrow_conflict` `7:13` |
| 同上 | `N_matrix_window_move` | `E1014`/`borrow_conflict` `6:13` |
| `a_write_of_a_local_borrow_through_a_parameter_is_rejected_where_it_is_made` | `N_local_into_param` | `E1013`/`store_local_borrow_through_reference` `5:5` |
| `tests/rc.rs` `a_shared_pointer_written_in_place_keeps_its_borrow` | `h5` | `E1013`/`borrow_outlives_block` `7:5` |
| 同上 | `tz_capture` | `E1013`/`task_capture_retains_borrow` `13:5` |
| `a_lock_callback_write_stays_in_the_lock` | `m_lock`・`rw_write` | `E1013`/`borrow_outlives_block` `10:5` |
| 同上 | `m_lock_return`・`rw_write_return` | `E1013`/`borrow_outlives_block` `9:5` |
| 同上 | `cond_lock` | `E1013`/`borrow_outlives_block` `11:5` |
| `an_implicit_drop_cannot_store_a_local_borrow_in_a_shared_cell` | `drop_inleak` | `E1013`/`store_local_borrow_through_reference` `11:9`（`_s` の宣言） |
| `tests/currying.rs` `a_call_of_a_lock_writer_keeps_the_written_borrow_in_both_forms` | `C_direct_stash`（と、今も拒否のパイプラインの形） | `E1013`/`borrow_outlives_block` `10:5` |

m_lock 系の kind は `borrow_outlives_block` に決めました（`let mut m` の双子と同じ）。

受理（今も受理。値は `tests/features.mjs` の新しい suite `in_place_writes`（`tests/fixtures/in_place_writes/Main.tz`）が native と WASM の `-O0`・`-O3` で確かめる。期待値は手で計算した値）:
落とし穴の `P_*` のすべて（既存の `in_place_writes_that_main_accepts_stay_accepted` の 12 形と、`tests/mutable_slices.rs` `writes_that_store_nothing_new_stay_accepted`）、`N_blit_then_mutate_source`・`N_rotate_then_use`・`N_fnvalue_read_then_move`（同じテスト）、`N_lock_read_then_move`・`N_partial_read_then_move`・`N_closed_cell_read`（`tests/rc.rs` `a_lock_callback_that_only_reads_stores_nothing`）。

拒否のまま（今も同じ診断）: `P_sort_with_fn`・`P_sort_by_fn`（std の中の診断が増えない）、`P_slice_region_*`、`P_widen_*`（受理を広げない）、`N_par_chunk`（`tests/mutable_slices.rs` `writes_that_main_rejects_keep_their_diagnostics`）、`N_arc_cell_write_alias`・`N_syncmap_store_fn`（`tests/rc.rs` `a_cell_written_after_it_was_shared_keeps_the_rule`）。A28 の各回のテスト（`F_fa_*`・`xmod_fa` を含む）は、そのまま拒否の期待を保ちます。

期待を変えるテスト（手順 4 で `untracked` を外すときに、このチケットに記録してから変える）:
`D_ctl_drop_no_loan`（`12:19` → 受理 5）、`W_upgrade`（`4:5` `borrow_outlives_block` → `3:5` `task_result_retains_borrow`）、`tests/currying.rs` の 3 つめの拒否（受理）、`retention_tests::an_untracked_store_anywhere_leaves_no_facts`（不要）。
セルの lends を外すと、A30 の誤った拒否 `m_named_owned`（`a_lock_callback_lends_a_way_to_write`、所有の値だけを書く名前付き関数をコールバックにする形、7）も受理に変わりえます。これも同じく記録してから変えます。

### 受け入れ条件の改訂

- 「再現」・「A28 の事実が広げる形」と、上の表の危険なプログラムが、表の code・kind・位置で拒否される。
- 落とし穴と `N_*` の安全な形が、基準と同じ値で受理される（`in_place_writes` suite、`compare.mjs`）。
- `tests/fixtures/`・`examples/` の診断と IR が基準と一致する。
- A28 の除外のうち「外す」ものを外し、A28 のテストが、上に列挙した期待の変更のほかは成功する。外せなかった除外は記録する。
- `check-docs.mjs` と T2 が成功する。

### 記録して続ける条件（停止条件の置き換え）

- 予算で `top` になった関数が安全なプログラムを拒否する。
- `drop_inleak` を塞ぐ手段が、暗黙の解放の 10 形の位置・code・kind を変える。
- `std/`・`examples/`・`tests/fixtures/` の診断か IR が変わる。

どれも、最も保守的な健全な選択を取り、ここに記録して続けます。

### ドキュメントの追加

上の「ドキュメント」に、`_tsuzuri/language-reference/ownership-and-memory/drop.md:197`（プログラム全体の条件）と `docs/architecture.md:1585`（パイプラインの closed の門）・1586（`untracked_loan_store`）・1587（`lends_mutable`）、`docs/language.md:1977`・1978・2009、`rc.md:178`、`lambda-expressions.md:236`、`lifetimes.md:297` を足します。

### 手順 3 の判断（2026-11-08、段階 2）

手順 3 を実装したときに決めたことです。どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」です。

- **事実の形と適用の位置:** 計画どおり `CallFacts.stores: Vec<StoreFacts>` を足し、`eval_call` の境界（`store_arguments` の後、closed と unit による loan の消去の前）と、パイプラインの完成で適用します。`$builtin` の生成関数は、その本文の組み込み関数の直接の呼び出しから事実を得ます（`Array.write` は (1, Value)→2、`SyncMap.__replace` は (0, Value)→1）。コールバックを受ける組み込み関数（`Mutex.with_lock`・`RwMutex.with_write`・`RwMutex.with_read`・`Cond.wait_with`・`OnceValue.get`・`List.map` など、scheme に関数型の仮引数があるもの）は、コールバックを「関数型でないほかの引数を渡して呼ぶ」高階の事実として扱います。lock は lock の引数を渡すので、計画の「lock のコールバックの事実を lock の引数の参照先へ対応させる」と同じになります。
- **Part の決め方:** 計画どおり値の来歴（`Value::through`）で決めます。`through` を保てない所は `Value` になり、保守的です。
- **書き戻しの除外は排他の経路だけ:** 仮引数の中身を同じ仮引数の参照先へ格納する組 `(q, Contents, q)` は、排他の借用だけをたどって届くときだけ捨てます（`rotate`・`sort`）。セル（`Arc<Mutex<...>>` など）を通って届くときは捨てません。セルはほかの持ち主と共有されうるので、値が持つものをセルへ入れると、ほかの持ち主がそれを読めます（`Drop.drop` が `s.f` を `s.cell` へ入れる形）。
- **書き込み先の経路で自分を指す loan:** 排他の借用だけをたどって届く持ち主には、その経路の持ち主に根を持つ loan を格納しません（書き込みに使っている借用そのものか、その借り直しで、書き戻しです。ほかの借用は排他の借用と衝突するので、ありえません）。セルを通って届く持ち主には格納します（セルの中に自分の借用を入れる形は、本当に自分を指すので、持ち主のムーブを拒否するのが正しい）。ただし `Rc`/`Arc` の持ち主に根を持つ共有の loan は格納しません。これは A28 が読み通さなかった `share` の代わりの loan で、ブロックの中に共有のポインタを入れるのは循環（解放されないだけ）で、持ち主のムーブで壊れません。
- **見えない関数値の規則（D2 の狭めたもの）:** 関数値・フィールド・要素・呼び出しの結果・dyn・より多くの引数を受けた結果の呼び出しは、関数値と引数の loan を、それらの書き込み先（排他の loan と、場所の型がセルに届く loan）のすべてへ格納します。`through` としてだけ持つ loan も、型が排他参照なら書き込み先に数えます（計画の「`through` だけの loan は書き込み先にしない」は、要素が関数値で、その環境が排他参照を持つときに健全でないため）。
- **高階の事実の捕捉の値:** ラムダの捕捉ごとの値と部分適用の引数ごとの値は分けず、関数値全体の値で代えます（保守的）。
- **局所値の借用の印:** 関数値の仮引数を呼ぶときに本文の局所値の借用を渡した引数は、呼び出し元で置き換えた事実がそれを外から来た値（呼び出し元の値、捕捉、部分適用の引数）へ格納するとき、呼び出しの位置で `store_local_borrow_through_reference` にします。見えない関数値を置き換えるときは、その関数値の環境も外から来た値に数えます。
- **closed の取り消し:** 計画どおり、closed で、書き込みうる本文を持ち、結果の型が `owned` でない関数を検査し、結果が loan を持てば closed を取り消します。取り消した関数は、完全な呼び出しが事実を要するなら保持の事実の候補に加えます（`Lending::untracked` の間は保持 `None` で、すべての引数の loan を残します）。
- **不動点の候補と依存:** 保持の候補（今までの `callers`）と、書き込みうる関数（`store_candidates`。名前を挙げる関係の最小不動点）を検査します。ある関数の事実が変われば、その関数の名前を挙げる候補を作業リストへ戻します。dyn の method の本文（`E::DynDispatch`）は検査せず `top` です。予算を使い切ったら、未確定の関数は保持 `None`・書き込み `top`・closed の取り消しです。
- **抽象の本文（infer）:** 書き込みの事実も closed の取り消しも使いません（型変数は loan を持たない）。Copy の制約と IR は変わりません。
- **暗黙の解放（手順 3b の前倒し）:** `store_through` の局所の枝と `E::Assign` の局所の枝で、持ち主の型が `drop_runs_code` なら、格納した loan を A28 の `dropped` に足します。`drop_inleak` のフックは、`Drop.drop` のどれかが書き込みうる（`store_candidates`）プログラムで、型が `drop_runs_code` の値を、束縛するとき（ブロックの終わりではなく、束縛の位置。生きていない局所値の loan は `retain_live_loans` が消すので、解放の位置では値の loan が分からないため）、局所値全体へ代入して古い値を捨てるとき、フィールドへ代入した後（持ち主の新しい値で）、値で呼び出しへ渡すときに、その値の loan をその値自身の書き込み先へ格納します。位置は、束縛では局所値の宣言、ほかは代入・呼び出しの式です。束縛の位置は解放より前なので、保守的です。
- **W_generic の診断が 1 つ増える:** 最初の診断（`first_share` の `2:22` `task_result_retains_borrow`）は同じで、`make` の `9:5` の `borrow_outlives_block` が 2 つめに増えます（`Array.write` が入れた借用を配列が持ち、`first_share` の事実はそれを結果に残す）。危険なプログラムの診断の列が増えるだけなので、`compare.mjs` の `allowed` に加えました。
- **`Vec.push_mut` も種にする:** `Vec.push_mut` の region slots（`store_arguments`）は直接の呼び出しにしか効かず、lock のコールバックや関数値の引数として呼ばれると、押し込んだ値の loan がベクタの持ち主に残りませんでした（`apply (Vec.push_mut f) (ref mut v)`・`Mutex.with_lock (ref m) (Vec.push_mut f)`）。種 (0, Value)→1 を足し、間接の呼び出しも事実で扱います。直接の呼び出しでは `store_arguments` が先に走るので、`store_region_unnamed` などの診断は変わりません。
- **種の無いプログラムは何もしない:** check が見落とす格納は、loan を運ぶ値を格納する種（`Array.write`・`SyncMap.__replace`・`Vec.push_mut`）を必ず通ります（代入は追跡し、受け取った参照を通す格納は名前付きの region を要し、関数値・dyn の method・コールバック・`Drop.drop` のコードはどれもモジュールの本文です）。そこで、種を挙げる関数が特殊化したモジュールに 1 つも無ければ、書き込みの事実・closed の取り消し・暗黙の解放のフックをすべて省きます（`StoreCandidates::seeded`、`CallFacts::seeded`）。高階の関数の多い fixture（`list_fsharp`・`vec_dotnet` など）で、ownership の時間が 2 倍前後になるのを避けるためです。
- **コールバックの closed も取り消す:** `callback_returns_closed` はコールバックの本文の closed を構文で証明するので、lock のコールバックが自分の `let m` へ locked value の借用を書き込んで読み戻す形を、F10 の検査（`mutex_result_retains_borrow`）が closed と数えていました（基準は受理します）。合同の不動点が closed を取り消した関数（`CallFacts::revoked`）は、コールバックとしても closed と数えません。この形は `3:27` の `mutex_result_retains_borrow` になります（`tests/rc.rs` `a_lock_callback_that_returns_what_it_stored_in_its_own_cell_keeps_the_rule`。借用を持たない関数を書く対照は受理のまま、値は 7）。F10 の規則に反する形なので、基準が受理していても拒否します。
- **解決していない形（記録して続ける）:** 次は、この手順で塞いでいません。どれも基準でも同じで、新しく受理したものではありません。
  - 名前付きの region の契約を持つ利用者の関数（`deref r = v` を region で許すもの）を、関数値やコールバックとして間接に呼ぶ形は、本文の格納を事実として記録しません（直接の呼び出しは `store_arguments` が扱います）。
  - 暗黙の解放のフックは、束縛・代入・値で渡す位置で働きます。それ以外で値が捨てられる所（`Array.write` が上書きする古い要素、ほかの関数の中で捨てられる値）は、A28 の `dropped` と `Lending::drops_store` に任せたままです（手順 4 で確かめます）。

### 手順 4 の判断（2026-11-08、段階 3）

手順 4 で A28 の除外を外したときに決めたことです。外すたびに T1 と 154 個の照合（`compare.mjs`）を回しました。

- **パイプラインの closed の門を外しました**（`ab0edbf0`）。パイプラインは `self.facts.closed[id]` だけを見ます。段階 2 の closed の取り消し（書き込みの事実で局所のセルへ入った loan を返す関数の closed を取り消す）が直接の呼び出しと同じく効くので、`tests/currying.rs` の `stash` のパイプラインの形は拒否のままです（コメントだけ直しました）。154 個の結果は段階 2 と同じ（変化は許した `W_generic` だけ）、T1 は 2993 件すべて成功しました。
- **プログラム全体の条件を外しました**（`88c7ba64`。`untracked_loan_store`・`untracked_writer`・`Lending::untracked`）。残りの書き込み関数は数しか格納しません: `Simd.store` の値は `SimdNumeric`（数の SIMD ベクタだけ）、`Atomic.store`・`Atomic.swap`・`Atomic.compare_exchange` の値は `AtomicValue`（整数と `bool` だけ。組み込みの class にはプログラムが instance を書けません）です。種の 3 つ（`Array.write`・`SyncMap.__replace`・`Vec.push_mut`）は段階 2 から書き込みの事実を持ちます。`untracked_writer` の監査の一覧は `store_seed` のコメントへ移しました。抽象モジュール（推論）からも外しました（推論の事実は Copy 制約の推論と推論の検査にだけ使い、受理を決めるのは書き込みの事実を適用する特殊化したモジュールの検査です。使われない関数も特殊化したモジュールで検査されることを確かめました）。
  - 計画が挙げた期待の変更をしました: `D_ctl_drop_no_loan`（`tests/rc.rs` `an_implicit_drop_keeps_what_the_dropped_value_holds` の `ctl_drop_no_loan`）は受理（5。手で計算した値、`-O0`・`-O3` で確認）で、対照の列へ移しました。`W_upgrade`（`a_share_of_an_element_holds_what_was_written_in` の upgrade）は `wrap` の本文（`3:5`）の `task_result_retains_borrow`（`arr` の借用を返す）です。`tests/currying.rs` `a_pipeline_into_a_unary_function_uses_its_facts` の 3 つめの拒否は受理（42。両方の形、`-O0`・`-O3` で確認）で、受理の列へ移しました。`retention_tests::an_untracked_store_anywhere_leaves_no_facts` は、同じ 4 つのプログラムで `a28_pass` が事実を持つことを確かめる `a_tracked_store_anywhere_keeps_the_facts` に置き換えました。`tests/rc.rs` `an_untracked_store_keeps_the_rule_from_before_a28_in_the_whole_program` は、期待を変えずに `a_field_assignment_into_a_dropped_value_keeps_the_rule_from_before_a28` に改名しました（`F_fa_*`・`xmod_fa` は段階 2 の `dropped` の追加で同じ位置の拒否のままです）。
  - 154 個: 段階 2 からの変化は `D_ctl_drop_no_loan`（受理 5）と `W_upgrade`（`3:5`）だけで、どちらも許した変化です。T1 は 2993 件すべて成功しました。
- **lends の除外を、特殊化したモジュールで外しました**（手順 4c）。特殊化したモジュールでは、排他参照の lends を事実と読み通しから、セルの lends を事実から外しました（`Lending::facts` は見えない呼び出しだけ、`reads` はセルを渡す本文と見えない呼び出し）。セルの lends を読み通しに残すのは、セルが `Rc`/`Arc` の共有するブロックの中にありうるからです。あるセルへの書き込みは、書き込みに使った持ち主の loan に入り、別の持ち主が読み通しで持つ写しには入りません（`N_arc_cell_write_alias`）。`Type::lends_mutable` に `exclusive` の引数を足し、偽ならセルだけを数えます（計画の「`Type::lends_mutable` を狭めたもの」）。計画の「`read_through` の型の規則」は足しませんでした。`Arc.share (ref a)` の引数そのものがセルに届く型なので、セルを共有する読み通しはどれも本文の規則で止まります（基準も同じです）。
  - **推論ではセルと排他参照の lends を、書き込みうる本文に残しました（狭めて残した除外）。** 段階 2 の決定どおり、抽象モジュールは書き込みの事実を適用しません。推論にも適用する案を試すと、抽象モジュールのラムダは持ち上げられていない（`E::Lambda`）ので見えない関数値の規則に落ち、lock の引数の借用をコールバックの値と一緒に `m` へ格納して `Mutex.into_inner m` が `E1014` になり（m_lock の系）、`O_ctl_2` などの安全な形が `store_local_borrow_through_reference` で拒否されました（154 個のうち 60 個が期待と違う）。そこで推論では、書き込みの事実を求める前処理（`store_candidates`。型変数は loan を持たない）が書き込みうるとする本文だけに、A28 の lends の規則（事実なし・読み通しなし）を残します。書き込みえない本文（種・コールバック・見えない呼び出しのどれにも届かない）は lends でも事実を持ちます。推論の検査も診断を出すので、書き込みうる本文がセルや排他参照を渡すプログラムは、全体として基準の規則のままです。
  - `m_named_owned` は計画の「受理に変わりうる」になりません。lock の組み込み関数は関数値（`writer`）を走らせるので、`stash` は見えない呼び出し（`opaque_calls`、残す除外）を持ち、どちらのモジュールでも事実を持たず、`13:5` の拒否のままです。lock にコールバックを渡す本文は、どれもこの除外に当たります。
  - `drops_store` は A28 の求め方（`Drop.drop` とその関係する本文が lends か見えない呼び出しを持つ）のまま残しました。計画の「書き込みの事実から求め直す」は、`Vec.push_mut` だけが種のプログラムなどで基準より広くなり、基準が受理しているプログラムを拒否しうるためです。A31 の解放のフック（`drops_write`）はそれに加えて働きます。
  - 期待を変えたテスト（再検討の一覧に無いもの。変える前にここに記録しました）: `tests/currying.rs` `a_pipeline_into_a_unary_function_uses_its_facts` の 2 つめの拒否（`mark` が `ref mut g` へ借用しない関数 `\() -> 2` を書く形）は、排他参照の lends を外した直接の結果として受理になります。`make` の結果は何も借用しないので安全で、`leak "abc"` の結果を呼ぶと 2 です（手で計算、`-O0`・`-O3` で確認）。受理の列へ移し、拒否の列には、`mark` が `text` を借用する関数を `Array.write` で書く形を足しました（基準と同じ `9:16` の `task_result_retains_borrow`、両方の形）。単体テスト `a_body_that_lends_an_exclusive_borrow_has_no_facts`・`a_body_that_lends_a_mutable_cell_has_no_facts` は、保持の期待を変えずに `a_body_that_lends_an_exclusive_borrow_keeps_what_the_write_stores`・`a_body_that_lends_a_mutable_cell_keeps_what_the_callback_stores` に改名し、前者は特殊化したモジュールでは事実を持ち推論では持たないこと、後者は見えない呼び出しでどちらのモジュールでも事実を持たないことも確かめるようにしました。
- **lends の走査を必要な本文だけにしました（性能）。** `reads` を尋ねるのは、読み通す本文（`calls_read_through`）とそれに関係する本文だけなので、セルの走査はそこだけで行います。排他参照も数える走査は、`Drop.drop` に関係する本文（`drops_store`）と推論の書き込みうる本文だけで行います。最初の版はすべての本文で、排他参照を見つけても止まらずにセルを探し続けたので、`vec_dotnet` の ownership の相が基準より 14 % 遅くなっていました（下の「手順 5 の記録」）。答えは変わりません（154 個の照合と手順 5 の比較で確認）。手順 4c の後（走査を狭めた版）の T1 は 2993 件すべて成功、154 個は期待と違うもの 0 です。
- **残した除外:** 見えないコードの除外（`opaque_calls`・`opaque_bodies`・`OpaqueCalls::environment`）、セルを渡す本文の読み通しの禁止、推論の書き込みうる本文の lends、`drops_store`・`dropped`・`drops_stop_reads`、読み通しの外部 loan の制限（4 回目）、`retention_applies`。
- **段階 2 の「解決していない形」の 1 つめは起きません:** region の契約で入力を排他参照の先へ格納しうる関数（`RegionSlots::writes`）は、直接の呼び出しにしか使えません。関数値・部分適用・コールバックにすると `regions::validate_contract_calls` が `E1013`/`region_callback_stores_exclusive_input` で拒否します（`tests/borrowed_records.rs` `named_target_regions_keep_stored_borrows_alive`）。直接の呼び出しは `store_arguments` が扱います。

### 手順 5 の記録（2026-11-08、段階 3）

手順 4 の後の分岐（`target/scratch/a31/_bin/tsuzuri-4c3`、dev build）で確かめました。

- `tests/fixtures/*`・`examples/*` のうち `Main.tz` か `Tsuzuri.toml` を持つ 154 個で、`check --json` と `build --no-cache --emit llvm` の出力（終了コード、標準出力と標準エラー、`.ll`）を基準のバイナリと比べ、すべてバイト単位で同じでした（`target/scratch/a31/_tools/step5.mjs`）。build が失敗する `domain`・`domain_host` も、両方で同じ `E1038` です。
- 154 個のプログラムの照合（`compare.mjs`）: edition 2027・2026 とも、期待と違うものは 0 です。段階 2 からの変化は `D_ctl_drop_no_loan`（受理 5）と `W_upgrade`（`3:5`）で、どちらも許した変化です。
- `in_place_writes` の suite: native と WASM の `-O0`・`-O3` で 28 件すべて成功しました。
- 性能: `TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値です（ms、11 回、基準・手順 4a の後・手順 4 の後を 1 回ずつ交互に実行。負荷平均 40 前後の共有の機械なので揺れがあります。CI の閾値にはしません）。`check` の相（型検査と推論の ownership を含む）は、どれも ±5 % の揺れの中で同じでした。

| fixture | 基準 | 手順 4a の後 | 手順 4 の後 |
| --- | --- | --- | --- |
| `hash_map` | 5.8 | 6.3 | 5.2 |
| `list_fsharp` | 9.1 | 9.5 | 8.0 |
| `array_fsharp` | 7.5 | 7.8 | 7.0 |
| `vec_dotnet` | 4.4 | 4.5 | 4.2 |
| `rc` | 1.4 | 1.5 | 1.4 |
| `tzroutine` | 0.4 | 0.5 | 0.5 |
| `examples/tzroutines` | 0.1 | 0.1 | 0.1 |

別の時刻（負荷平均 25 前後）にもう一度、基準と手順 4 の後だけを比べると、`vec_dotnet` 9.1 → 9.4、`hash_map` 13.4 → 10.9、`list_fsharp` 23.5 → 20.3、`array_fsharp` 16.0 → 14.4、`rc` 2.7 → 2.5、`tzroutine` 0.7 → 0.7、`examples/tzroutines` 0.1 → 0.1 で、10 % を超えて遅いものはありません（`check` の相も ±7 % の中）。
`tzroutine` の差（0.1 ms）は測定の分解能と同じ大きさです。段階 2 の記録にもあり（0.7 → 1.0）、書き込みの事実の前処理（`store_candidates`）がどのモジュールでも本文を 1 回走査する分と見ています。

## 決定事項

### D1: 契約を定義で検査せず、本文から求めた事実を呼び出し側で使う

- 決定: 書き込みを宣言の region の契約（`RegionSlots`）で表さず、特殊化した本文から「どの引数の借用がどの排他参照の先へ入りうるか」を求め、呼び出し側で書き込み先の持ち主へ足す。定義の位置では新しい診断を出さない。
- 理由: 型変数・関数の型・スライスの要素には region を書けないので、契約で表すと、どの形でも安全なプログラムを定義の位置で拒否する（「落とし穴」の 2 回分）。事実なら、拒否は残した借用が実際に破られる呼び出し側だけに出る。A28 の事実と同じ枠で求められる。
- 状態: 既定案（実装者はこの案に従う。成り立たなければ停止条件）。

### D2: 部分適用と関数値は保守的に扱い、拒否しない

- 決定: 関数値の呼び出しは、loan を運ぶ引数と関数値自身の借用を、排他参照のすべての引数の先へ残す。部分適用を `region_callback_stores_exclusive_input` で拒否しない。
- 理由: 基準は部分適用を受理している（`let w = Array.set 0 (\() -> 5)` など）。保守的に残すだけなら、拒否は残した借用を破る呼び出しに限られる。
- 状態: 既定案。

## 手順 1: ベースラインの記録（2026-11-08、HEAD b3dd7f1c）

基準のバイナリ `target/scratch/base/tsuzuri`（HEAD `b3dd7f1c`、dev build、macOS arm64）で、154 個のプログラムの `check --json` と、受理なら `run -O0`・`run -O3`（native）を記録しました。
プログラムと表は `target/scratch/a31/`（`baseline.md`、各プログラムは `<名前>/Main.tz`）、生の結果は `target/scratch/a31/_tools/results.json` です（監査が `target/scratch/audit-A31/` に作ったものの写し）。

- 再実行: `node scripts/slot.mjs -- env BIN=<binary> node target/scratch/a31/_tools/run.mjs [名前...]`（結果は `BIN` を渡すと `_tools/results-branch.json`、`OUT` で変えられる。`EDITION=2026` で edition 2026、`JOBS` で並列数）。
- 照合: `node target/scratch/a31/_tools/compare.mjs [分岐の結果] [基準の結果]` が、下の「危険」の 21 個はこの節の上の表の code・kind・位置で拒否、`D_ctl_drop_no_loan`・`W_upgrade`・`T_lambda_nowrite`・`W_nowrite` は基準の診断か列挙した変化、ほかは基準と同じ（受理なら同じ値、拒否なら同じ診断の列）であることを確かめ、違うものを出力して 1 で終わります。基準の結果どうしで照合すると、危険な 21 個だけを出力します（道具の自己検査）。
- 表の作り直し: `node target/scratch/a31/_tools/report.mjs`（`RESULTS`・`OUT` で入力と出力を変えられる）。
- 拒否の期待の根拠: 書き込みを代入（`arr[0] = ...`・`data[0] = ...`、lock は `let mut m`）にした双子を `target/scratch/a31/_twins/` に置き、基準のバイナリの診断を `_twins/results-base.txt` に記録しました。

結果の要約:

- 154 個のうち、受理 60 個、拒否 94 個。
- 受理されて危険なもの（21 個。A31 の後は拒否）: `h1`〜`h4`（`-O0` は 43 億前後の不定の値、`-O3` は 0）、`h5_rc_get`（同じ）、`h7_array_return`（同じ）、`h9_ref_elements`（0/0、期待は 9）、`h6_move_after`（0/0）、`h6b_move_after_sum`（9/9、期待は 909）、`m_lock`（0/9）、`m_lock_return`（1/9）、`rw_write`（0/9）、`rw_write_return`（1/9）、`cond_lock`（0/9）、`tz_capture`（0/9）、`C_direct_stash`（0/9。`m_lock` と同じプログラム）、`D_drop_inleak`（3602879701896396803/12、期待は 12）、`N_generic_put_move`・`N_matrix_window_move`・`N_owned_value_then_move`（9/9、期待は 909）、`N_local_into_param`（0/0、期待は 9）。
- 受理されて安全なもの（39 個。A31 の後も同じ値で受理）: 落とし穴の `P_acc_1`〜`P_acc_12`（2・6・7・7・4・2・3・4・2・6・1009・5）、`P_ref_string_set` 3、`P_pipe_set` 323、`P_fill_ref` 322、`P_pick_set` 7、`P_partial_set` 5、`P_partial_put` 1、`P_partial_matrix` 2、`P_matrix_wrapper_noregion` 4、`P_matrix_wrapper_fill` 6、`P_put_last_first` 2、`P_map_in_place_body` 5、新しい安全な形の `N_lock_read_then_move` 24、`N_partial_read_then_move` 24、`N_fnvalue_read_then_move` 24、`N_closed_cell_read` 7、`N_blit_then_mutate_source` 12、`N_rotate_then_use` 4、A28 のテストの対照 `L_ctl_1`〜`L_ctl_4`（11・9・0・42）、`O_ctl_1`・`O_ctl_2`（11・11）、`D_ctl_1`〜`D_ctl_3`（0・42・12）、`F_ctl_fa_ref` 12。値はどれも `-O0` と `-O3` で同じです。
- 拒否されているもの（94 個）: 「A28 の事実が広げる形」と A28 の各回のテスト（`S_*`・`T_*`・`W_*`・`L_*`・`O_*`・`D_*`・`F_*`）は、チケットとテストの表と同じ位置・code・kind です（主に `Lending::untracked` による）。対照（`h0` `7:5`、`h6_assign_control` `5:13`、`h9_assign_control` `4:19`、`S_assign_control` `9:5`、`m_lock_mut_probe` `10:5`）、落とし穴の拒否（`P_sort_with_fn` は `std/Array.tz:2003:32` `task_result_retains_borrow` と `2084:15` `borrow_conflict`、`P_sort_by_fn` は `2098:15`、`P_slice_region_unsized` `1:50`、`P_slice_region_mix` `1:38`、`P_widen_*` は `store_region_unnamed` `2:13`・`2:18`・`2:15`）、`N_arc_cell_write_alias` `8:5`、`N_par_chunk` `4:27` `parallel_callback_borrows_environment`、`N_syncmap_store_fn` `8:5`、`N_local_into_param_assign` `5:5` `store_region_unnamed`。
- edition 2026（`EDITION=2026`、`_tools/results-base-2026.json`）で違うのは、要素・フィールドへの代入を使う 19 個（`h0`・`h6_assign_control`・`h9_assign_control`・`S_assign_control`・`N_local_into_param_assign`・`F_*` 14 個）だけで、どれも `E1012`/`assign_to_immutable_place` になります。危険な 21 個と安全な 39 個（`F_ctl_fa_ref` を除く）は両方の edition で同じなので、手順 2 の Rust のテストは両方の edition で確かめます。
- 安全な `P_*`・`N_*` を関数にした `tests/fixtures/in_place_writes/Main.tz`（手順 2 で足す suite `in_place_writes`）は、基準のバイナリで native と WASM の `-O0`・`-O3` の 28 件がすべて通りました（`node tests/features.mjs target/scratch/base/tsuzuri in_place_writes`）。

## 実装と検証（2026-11-08）

ブランチ `fix/a31-in-place-writes-keep-borrows`（`main` の `b3dd7f1c` から）。4 段で実装しました。判断の詳細は上の「着手時の再検討」「手順 3 の判断」「手順 4 の判断」「手順 5 の記録」にあります。

### 実装した手順

- 手順 1（`22bb065f`・`e39069b2`）: 監査の 154 個のプログラムを基準のバイナリで記録し、再実行と照合の道具（`target/scratch/a31/_tools/`）を作りました（「手順 1: ベースラインの記録」）。
- 手順 2（`ff395453`）: 失敗する拒否のテスト 7 件（`tests/mutable_slices.rs` 3、`tests/rc.rs` 3、`tests/currying.rs` 1）、受理と診断を保つテスト、E2E の suite `in_place_writes`（`tests/fixtures/in_place_writes/Main.tz`、値は手で計算）を足しました。
- 手順 3a・3b（`6ad78bce`・`32561263`・`a37b26fe`・`ccfa9fc4`）: 書き込みの事実（`StoreFacts`: `(from, Part, to)` の組・高階の事実・`top`）、種（`Array.write`・`SyncMap.__replace`・`Vec.push_mut`）、`Value::through` による Part、A28 の事実・closed と合同の最小不動点（`joint_facts`、予算の打ち切りは保持 `None`・書き込み `top`・closed の取り消し）、`eval_call`・パイプライン・コールバック・関数値での適用（`apply_facts`・`apply_higher`・`apply_opaque`・`builtin_stores`・`pipe_stores`）、lock のコールバック、`MatrixView.Mut` の窓の後ろの持ち主、定義の位置の `E1013`/`store_local_borrow_through_reference`（遅らせる診断）、closed の取り消し（コールバックの closed も）、暗黙の解放（`dropped` の追加と `drop_stores` のフック）。種の無いモジュールでは何もしません。
- 手順 4（`ab0edbf0`・`88c7ba64`・`500d34ec`・`329e7451`）: パイプラインの closed の門、プログラム全体の条件（`untracked_loan_store`・`untracked_writer`・`Lending::untracked`）、特殊化したモジュールの排他参照の lends（事実と読み通し）とセルの lends（事実）を外しました。lends の走査は必要な本文だけで行います。
- 手順 5（`329e7451`・`2dc000c1`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とバイト単位で同じ。性能は「手順 5 の記録」。
- 段階 4（`76ae9b39`、文書を書くための確かめで見つけた穴の修正）: edition 2027 の読み取りの仮引数（`own` の無い仮引数。共有参照として渡る）を通した書き込みが、事実に載っていませんでした。関数値の読み取りの仮引数は本文が `deref` を通して呼ぶので、`apply_higher` の「本文の仮引数」の判定に当たらず、見えない関数値の規則で `(0, Contents, 1)` になり、呼び出し側では借用した一時値（`E::BorrowOperand`。値は一時値が持つ loan そのもの）の中身として、ラムダの捕捉した借用を落としていました。値の読み取りの仮引数（`def put :: (unit -> i64) -> ref mut [...] -> unit`、generic な `'a -> ref mut ['a..] -> unit`）も同じく中身を落としていました。`apply_higher` が共有参照の仮引数を `deref` する呼び出しでも高階の事実を記録し、実引数の借用した一時値の中の関数値を見るようにし、`Arg::temporary`・`Source::temporary` で借用した一時値の `Part::Contents` をその loan で格納するようにしました。修正の前は `apply_read (Array.set 0 (\() -> String.length borrowed)) (ref mut arr)`・`put_read (\() -> String.length borrowed) (ref mut arr)` などが edition 2027 で受理され、解放した文字列を読みました（`-O0` で 43 億前後の不定の値）。edition 2026（仮引数が値）では修正の前から拒否です。テスト `tests/mutable_slices.rs` `a_read_parameter_keeps_what_its_writer_stores`（拒否 7 形を両方の edition で `9:5`・`10:5` の `borrow_outlives_block` と `7:13` の `borrow_conflict`、受理 2 形）と、`in_place_writes` の 2 件（`read_parameter_writes_owned` 13、`read_parameter_reads_capture` 18、手で計算。基準のバイナリでも同じ値で受理）を足しました。
- 手順 6（この節と同じコミット）: 文書（下の一覧）。

### 計画と違う判断

- 監査の Value/Contents の型の規則は健全でないので、値の来歴（`Value::through`）で決めました（再検討の食い違い 11）。
- 見えない関数値の規則は、`through` だけで持つ loan も、型が排他参照なら書き込み先に数えます（手順 3 の判断）。
- 推論（抽象モジュール）は書き込みの事実を使わず、書き込みうる本文が排他参照かセルを渡すときは A28 の lends の規則（事実なし・読み通しなし）を残しました（手順 4 の判断。推論に適用すると 154 個のうち 60 個が期待と違った）。そのため、`Array.write` を借用を持ちうる具体的な型で書き込む本文（またはそれを呼ぶ本文）が `ref mut` を渡す関数は、A28 より前の規則のままです（`Array.set` のような generic な関数を通した書き込みは数えません）。文書の「現在の制限」に書きました。
- `drops_store` は A28 の求め方のまま残しました（「書き込みの事実から求め直す」は基準が受理するプログラムを拒否しうるため）。
- `m_named_owned`（所有の値を書く名前付き関数を lock のコールバックにする形）は、見えない呼び出しの除外（残す除外）のため拒否のままです（基準と同じ）。
- 期待を変えたテストは「手順 4 の判断」に列挙したものだけです（`ctl_drop_no_loan` が受理 5、`W_upgrade` が `3:5` `task_result_retains_borrow`、`tests/currying.rs` の 2 つの拒否が受理 42・2 と、その代わりの拒否、単体テストの置き換えと改名）。`W_generic` は最初の診断が同じで、2 つめの診断が増えます。lock のコールバックが自分の `Mutex` へロック中の値の借用を書いて読み戻す形は、基準が受理していましたが `3:27` `mutex_result_retains_borrow` になります（F10 の規則に反する危険な形）。
- `_features/README.md` には A31 の行が一覧の表に 1 つだけあり（2026-11-05 の追加チケットの表には A31 の行がありません）、その行を更新しました。A28 の行の状態の説明には、A31 で外した条件を書き足しました。

### 確かめたこと

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` はどちらも警告なし（`76ae9b39`）。
- T1: `cargo nextest run --locked --no-fail-fast` は、段階 1 で 2991 件中 2983 件成功（意図した失敗 7 件と、共有の機械の flake 1 件）、段階 2・3 の各手順の後で 2993 件すべて成功、`76ae9b39` で 2994 件すべて成功（4 件 skip）。
- 領域のテスト（`76ae9b39`）: `cargo test --locked --test mutable_slices` 17 件、`--test rc` 24 件、`--test currying` 16 件、`--lib ownership` 13 件、すべて成功。
- 154 個の照合（`compare.mjs`、`76ae9b39`）: edition 2027・2026 とも期待と違うもの 0。基準からの変化は危険な 21 個の拒否と、許した `D_ctl_drop_no_loan`・`W_generic`・`W_upgrade` だけです。
- 手順 5 の比較（`76ae9b39`）: `tests/fixtures/*`・`examples/*` の 154 個で、`check --json` と `build --no-cache --emit llvm` が基準とすべて同じ。
- E2E（dev のバイナリ、`76ae9b39`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 30 件・`mutable_slices` 49 件・`rc` 39 件・`borrowed_records` 57 件・`slices` 11 件・`matrix_view` 303 件・`concurrency` 15 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs` 701 件（native と WASM の `-O0`・`-O3`）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）、`TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs <bin> 100 1 4`（100 個、3200 回の実行で差なし）。すべて成功しました。`in_place_writes` は基準のバイナリでも 30 件すべて同じ値で通ります。
- 文書（2026-11-08 に書いた 8 ページ）: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs <8 ページ>` は `Docs: 8 pages, 138 links, 75 checked examples, 150 native runs (O0/O3), 0 test projects, 1 generated blocks.` で成功（2026-11-11 に実行。この実行は中断の前に終わっておらず、再開後に確かめました）。

### 更新した文書

- `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 既知の不具合の注意を実際の振る舞い（その場の書き込み、見えない関数値の規則、局所値の借用の `E1013`）に置き換え、プログラム全体の条件の文を消し、残る除外を箇条にした）
- `ownership-and-memory/drop.md`（プログラム全体の条件の文を消し、解放が書き込む借用の規則と例を足した）
- `ownership-and-memory/borrowing.md`（「書き込んだ値の借用」の節）
- `values-and-functions/lambda-expressions.md`・`built-in-types-and-modules/rc.md`（除外の文を今の規則にした）
- `built-in-types-and-modules/array.md`・`matrix-view.md`・`mutex.md`（書き込んだ借用を書き込み先の持ち主が持つこと）
- `built-in-types-and-modules/rwmutex.md`・`cond.md`（`with_write`・`wait_with` の本体が書き込んだ借用もロックの持ち主に残ること。2026-11-11 に追加）
- `README.md`（変更の種類別の表の「所有権・借用・region」の行に、`tests/mutable_slices.rs`・`tests/rc.rs` と suite `mutable_slices`・`in_place_writes`・`rc` を足した。2026-11-11）
- `docs/language.md`（関数の結果が持つ借用: 書き込みの事実の項目、セルと推論の項目、破棄の項目、読み通しの理由、`Rc / Arc` の結果の借用、排他借用フィールドの節の既知の不具合の段落を実際の振る舞いに、排他スライスの節）
- `docs/architecture.md`（所有権の節: パイプラインの門、書き込みの事実の段落、lends の段落）

### 変えたファイル

`src/ownership.rs`、`src/check.rs`（`Type::lends_mutable` の `exclusive`）、`tests/mutable_slices.rs`、`tests/rc.rs`、`tests/currying.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、`.github/workflows/targets.yml`、上の文書、`README.md`、`_features/README.md`、`_features/_completed/A28-function-results-retain-ref-arguments.md`（A31 へのリンク）、このチケット（`_features/_completed/` へ移動）。

### 再開後の仕上げ（2026-11-11）

段階 4 は文書の検査の途中で中断していました。コンパイラーのコードは `76ae9b39` から変えていません（開発用のバイナリも同じものです）。再開後に行ったことは次のとおりです。

- 文書の主張をコンパイラーで確かめ直しました。中断の前の確かめ用のプログラム 31 個（`target/scratch/a31/_docs/`）を枝のバイナリと基準のバイナリで実行し、文書どおりであること（`Array.set` で書く本体は規則を使い受理 2、`Array.write` で書く本体は以前の規則で拒否、`_s` の宣言の `9:9` の `store_local_borrow_through_reference` など）を見ました。足りなかった主張には 39 個を足しました（`_docs2/`。結果は `_docs2-2027.txt`・`_docs2-2026.txt`）: `Array.set`・`Array.fill`・`Array.blit`・参照の要素の書き込みの後の借用元のムーブが `E1014`、書き込んだ配列を返すと `E1013`、`MatrixView.write`・`fill`・`copy_from`・`map_in_place` と `row_mut` の行への `Array.write` が窓の後ろの配列に借用を残すこと、`Mutex.with_lock`・`RwMutex.with_write` のコールバックの書き込みの後のムーブが `E1014`（借用元を使い続けるだけなら受理 18）、`let w = Array.set 0 f` の `w (ref mut arr)`、`Vec.push_mut`、書き込んだ要素の `Rc.share`、排他借用だけを渡す本体の読み通し（`Array.set` の形は受理 5、`Array.write` の形は `E1014`）、セルを渡す本体が読み通さないこと、`Array.set` で局所の配列へ書いた借用を返す関数の結果（保持する引数だけを持つ: 受理 2、基準は拒否）。edition 2026 でも、次の 3 個を除いて同じ結果です: 2026 に無い添字代入を使う 1 個、2026 では `Vec.push` が `Vec` を返すので型が合わない 1 個、`Array.map` にラムダを渡す本体の `Rc.share`（2027 は引数を貸すので見えないコードの除外に当たり読み通さず `E1014`、2026 は値で渡すので読み通して受理 7。どちらも基準と同じ扱いの安全な形）。`vec.md` の「借用を持つ要素を格納する push は直接の完全適用だけ」は基準と同じ（`region_callback_stores_exclusive_input`）なので変えていません。
- 文書で触れていない形の危険なプログラムを 17 個足して確かめました（`_adv/`: 関数値の引数を 2 段に渡す形、レコードのフィールドの書き込み関数、関数が返した書き込み関数、参照先の配列の要素を別の配列へ書く形（`ref` と 2027 の読み取りの仮引数）、generic な書き込み関数の 2 段、`Maybe` の要素、`while` の中の書き込み、`Array.split_at_mut` の片方への書き込み（直接と関数の中）、2 つの `ref mut` の間の書き込み、パイプライン、配列に入れた部分適用）。型や構文の誤りで検査に届かない 2 個を除き、すべて両方の edition で `E1013` か `E1014` です。受理される危険な形は見つかりませんでした。
- `mutable_slices` と `in_place_writes` の suite はどの workflow からも実行されていなかったので、`.github/workflows/targets.yml` の features の step の並びに「Exclusive slices and in-place writes (C08, A31)」を足しました（release のバイナリ、3 つの host）。`ci.yml` には足していません（時間の上限のため。ほかの追加の suite と同じ扱いです）。
- `docs/architecture.md` の `parameter_retention` の説明を、今の実装（`joint_facts` が `check_facts` で求め、`parameter_retention` はテスト用の関数）に合わせました。`_features/README.md` の A28 の行は、プログラム全体の条件が A28 の時点のものだと分かる書き方にしました。
- T0（2026-11-11）: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1（2026-11-11、文書と workflow を変えた後）: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 2994 件すべて成功、4 件 skip（1238 秒。共有の機械が混んでいて 12 件が slow）。
- T2（2026-11-11、dev のバイナリ）: `node tests/features.mjs target/debug/tsuzuri <suite>` は `in_place_writes` 30 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件が native と WASM の `-O0`・`-O3` で成功。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs <変えた 10 ページ>` は `Docs: 10 pages, 151 links, 77 checked examples, 154 native runs (O0/O3), 0 test projects, 1 generated blocks.` で成功。`node scripts/gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`node scripts/check-cli-docs.mjs` は `4 checks, 0 failed`（診断の種類と std の関数は増やしていません。`store_local_borrow_through_reference` は A31 より前から登録済みの種類です）。`node scripts/check-guide-lite.mjs` は `88 references checked, 0 failed`、`node scripts/check-links.mjs --into _features/README.md` は `0 broken`。`src/runtime/` は変えていないので `check-runtime-includes.sh` は対象外です。そのほかの suite（`slices`・`matrix_view`・`concurrency`・`sync_types`・`tzroutine`・`control.mjs`・`user_drop.mjs`・`progfuzz.mjs`）は同じバイナリで `76ae9b39` の時点に成功しています（上の「確かめたこと」）。

### 残る制限（文書に書いたもの）

- 見えないコード（名前付き関数でない関数値の呼び出し、dyn のメソッド、`Owned.call`、コールバックを実行する組み込み関数）へ借用を持ちうる値を渡す本体は、事実も読み通しも使いません（A28 の除外のまま）。lock の組み込み関数にコールバックを渡す本体もこれに当たるので、所有の値だけを書く名前付き関数を lock のコールバックにする形（`m_named_owned`）は拒否のままです。
- 推論の段（抽象本文）は書き込みの事実を使わないので、書き込みうる本体が排他借用かセルを渡すとき、その段は A28 より前の規則を保ちます。`Array.write` を借用を持ちうる具体的な型で呼ぶ本体が `ref mut` を渡すと、結果はすべての引数の借用を持ちます（`Array.set` などの generic な関数を通せば事実を使います）。
- 借用を持ちうる値のセルを渡す本体は `Rc.share` などを読み通しません。`drops_store` は A28 の求め方のままです。
- 本体が見えない関数値の呼び出しは、安全側に扱います（第 1 回のレビュー対応で規則を作り直しました。下の「レビューの指摘への対応（第1回）」）。プログラムのどこかに「受け取った値を自分の環境が届く場所へ書き込む関数値」があると、見えない関数値の呼び出しは引数の借用を関数値の環境の先にも残し、その関数値がセルを所有していると、環境を隠す型かセルに届く型のすべての局所値に残します。この判定はプログラム全体で行うので、離れた所の関数値が、別の関数の中の見えない呼び出しの検査を厳しくします。
- lock のコールバックが、ロック中の値の借用を別の `Mutex` へ書いて読み戻した値を返す形は `mutex_result_retains_borrow` です（基準は受理していた危険な形）。
- 読み取りの仮引数（edition 2027）や演算子が借りた一時値への参照そのものを、呼び出す関数が引数の先へ書き込む形は `E1013` です（`let` で束縛してから渡します）。本体が見えない関数値へ一時値を貸す形は、最後の引数なら、参照の仮引数そのものを格納する関数がプログラムにあるときだけ `E1013`、最後でない引数なら、書き込み先があれば `E1013` です。

### レビューの指摘への対応（第1回）（2026-11-11）

レビューの 19 件の指摘（反例の探索 12 件、適合の確認 7 件）を、再現プログラム（`target/scratch/review-a31-ce/` の 279 個、`target/scratch/review-a31-conf/p/` の 117 個）で確かめました。どれも再現しました（指摘 12 は受理だけを再現。実行は基準でも対照でも `SIGABRT` です）。対応の前後の `check --json`（両方の edition、792 件）は `target/scratch/a31/r1/before.json`（対応前の先端 `49997477`）・`base.json`（基準）・`after-final.json`（対応後）にあり、`r1/probe.mjs`・`diff.mjs`・`st.mjs` で作り直せます。

設計の変更（`src/ownership.rs`。`src/check.rs` には `Type::owns_mutable_cell`・`Type::stores_some` を足しただけです）:

- **見えない関数値の規則を、プログラムの関数値から決める 3 つの旗に作り直しました（`sink_flags`、`CallFacts::sinks`・`owned_sinks`・`reference_sinks`、`owned_types`）。** 関数の型は環境を表しませんが、関数値を作る所（持ち上げたラムダと捕捉、名前付き関数の部分適用、コールバックを取る組み込み関数の部分適用、`Dyn.of`）はすべてモジュールの本文にあり、その本体の書き込みの事実も分かります。そこで、(1) 作る所のどれかが「束縛した値（環境）へ書き込む、束縛した値を関数値の呼び出しへ渡す、分からない」事実を持てば `sinks`、(2) そのうえ束縛した値が可変セルを所有していれば（参照を通さずに `Mutex` などに届く型。`Arc<Mutex<...>>` の値での捕捉）`owned_sinks` とその関数値の型（とそれを環境に持ちうる関数値の型）を `owned_types`、(3) どれかの関数が共有参照の仮引数そのもの（`Part::Value`）を格納するか、それを持つ値を関数値へ渡せば `reference_sinks` にします。旗は書き込みの事実から求め、事実は旗の下の検査から求めるので、`joint_facts` は旗が上がるたびに不動点を取り直します（旗は上がるだけなので高々数回。検査に失敗する本文の `top` は数えません: そのプログラムは拒否されるので、ほかの本文に余計な診断を出さないためです）。
  - `sinks` が偽のモジュールでは、どの関数値を呼んでも書き込みは引数の先にしか起きません（関数値の本体はモジュールの関数で、環境へ書き込むものが 1 つも無いので、呼び出しの帰納法で従います）。見えない関数値の呼び出しは、関数値と引数の loan を引数の書き込み先へ格納するだけです。
  - `sinks` が真なら、引数の loan を関数値の loan の書き込み先（環境が借用する場所）へも格納し、`store_target` は関数値・`Owned.Function` を持つ型の場所も書き込み先に数えます（dyn 値に届く型の場所は常に数えます）。
  - `owned_sinks` が真で、呼ぶ関数値の型（か、引数をいくつか渡した後の関数の型）が `owned_types` のどれかでありうるか、引数の型が環境を隠す・セルに届くなら、`Checker::store_hidden` が、環境を隠す型かセルに届く型のすべての局所値へ格納します。関数値はコピーでき、コピーは借用なしにセルを共有するからです。その文の中で先に評価して後から束縛する値（`let t = (w, w f)`）は `hidden_log` と束縛の位置の `absorb_hidden`、関数の結果は本文の検査の後、呼び出し元の値は `StoreFacts::hidden` の事実で扱います。
- **不変な局所値の関数値は見える本体として扱います（`known_functions`、`Checker::known`）。** `let` がラムダ・名前付き関数・部分適用・それらの `if`・`Owned.function` の引数に束縛されていれば、その局所値の呼び出し（`w x`、`Owned.call (ref w) x`、関数値の引数として渡した先での呼び出し）に、その本体の事実を当てはめます。
- **dyn のメソッドの dispatch は、vtable のスロット関数（仮引数の数が同じもの）の事実を合わせたものにしました**（`StoreCandidates::dispatch`。候補が無ければ今までどおり `top`）。
- **排他の書き込み先は、場所が借用を持てる型のときだけ数えます**（`Checker::hidden_type` の `holds`。`ref mut [i64..]`・`ref mut i64` は数えません。`[T..]` は `[T]` と読みます）。
- **事実の適用は、格納が何も変えなくなるまで繰り返し、1 回の走査では同じ事実と同じ引数の適用を 1 度だけ行います**（`Checker::settle`・`store_memo`）。ある格納が別の事実の書き込み先を広げる順序の問題も、これで閉じます。
- **`Rc.share (ref owner)` などの「読み通さないときの代わりの loan」に印を付けました（`Loan::share`、`share_loans`）。** 持ち主のブロックへ格納しないのは印の付いた loan だけで、`ref owner` そのものは格納します（持ち主のムーブは `E1014`）。印の付いた局所の loan を呼び出し元の書き込み先へ格納するときは、その局所値が持つ loan に読み替えます。
- **`retain_live_loans` は、借用されたままの局所値が今持つ loan のルートも生かします。**
- **破棄のフックを、束縛しない値を評価するすべての所へ広げました**（`eval`、位置は囲む宣言か呼び出し: `Checker::drop_site`）。どれかの `Drop.drop` が書き込みうるとき、破棄でコードを実行しうる型の値を持つ本文は書き込みうる本文に数え（`store_candidates`）、関数の中の破棄の格納がその関数の事実になります。関数の本文の結果は呼び出し元が束縛するので、フックを当てません。
- **`Drop.drop` が破棄する値そのものの借用を格納する形**は、`Drop.drop` の事実（仮引数の `Part::Value`）で見つけ（`drop_borrows_self`）、束縛では局所値の loan を値の書き込み先へ格納し（`drop_keeps_self`）、一時値と値で渡す値は書き込み先があれば `E1013` です。
- **演算子と一時値:** 演算子が利用者の instance へ貸す被演算子（`E::BorrowOperand` の場所）は、値が「場所の中身」なので、`Part::Value` の事実のために場所の loan を別に持ちます（`Checker::argument`、`Arg::reference`）。貸した一時値への参照を格納する事実は `E1013` です（`Arg::dying`）。構造的な比較・ハッシュ・表示は、被演算子の後ろに並ぶ要素のメソッドを高階の呼び出しとして当てはめます。`Owned.call` は owned function を入力で呼ぶ高階の呼び出し、結果の型が仮引数の型と同じコールバック（`List.foldr`）はコールバック自身の値も受け取るものとして扱います。

指摘ごとの対応（期待は、レビューの対照と、直接の形の双子の診断から決めました。値は手で計算したものです）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1・16（参照・要素・フィールド・別名の関数値） | 再現（`k01`〜`k03`・`t04`〜`t07`・`k07`・`k09`・`i01`・`i04`・`i05`・`op1`〜`op3`・`op6`〜`op8` が受理） | `sinks` の規則。`k01` などはブロックの位置の `assignment_extends_borrow`、`op1`〜`op3` はムーブの位置の `borrow_conflict`、`op7`・`op8` は呼び出しの位置の `store_local_borrow_through_reference` | `tests/rc.rs` `a_function_value_behind_a_reference_or_an_alias_keeps_what_its_call_stores`（`n01`・`n14` も。対照 `k05`・`k06`・`n15` は受理） |
| 2・13（セルを所有する関数値、`Owned.call`） | 再現（`d01`・`d04`・`d06`・`d07`・`e04`〜`e06`・`x02`・`oc1`〜`oc5` が受理） | `owned_sinks` の規則、`known`、`Owned.call`。`x02` は、関数値が loan を持たなくても呼び出しの位置で拒否（`apply_opaque` の早い return を無くしました） | `a_function_value_that_owns_its_cell_keeps_what_its_call_stores`（この対応で作った `n02`〜`n04`・`n11`〜`n13` も。対照 `d09`・`n08`・`n09` は受理）、`a_function_value_handed_to_later_code_takes_no_borrow_afterwards`（`n16`・`n17`） |
| 3（callee の中の破棄） | 再現（`g17`・`g38` が受理） | 破棄する値を持つ本文を書き込みうる本文に数える | `a_drop_in_a_callee_or_of_a_temporary_stores_what_the_value_holds` |
| 4（演算子と利用者の instance） | 再現（`v07`・`z04`・`v10` が受理）。同じ原因で、読み取りの仮引数へ貸した一時値（`r1/q5`、2027）も受理でした | 被演算子の場所の loan と、一時値の `E1013` | `an_operator_or_a_structural_operation_applies_the_stores_of_the_instance`（対照 `v08` は受理） |
| 5・17（束縛しない一時値の破棄） | 再現（`g37`・`g13`・`g39`・`dr1`〜`dr4`・`dr6`・`dr7` が受理） | `eval` の破棄のフック | 3 と同じテスト（対照 `dr0`・`dr5` は同じ診断） |
| 6（破棄する値そのものの借用） | 再現（`g33`・`g32` が受理） | `drop_borrows_self`・`drop_keeps_self` | 3 と同じテスト |
| 7（`List.foldr` の状態） | 再現（`u04`・`u02` が受理） | コールバック自身の値を引数に足す | `a_fold_callback_stores_the_state_that_it_returned`（対照 `u05` は受理、`u03` は同じ診断） |
| 8（部分適用の入れ子で指数時間） | 再現（深さ 10 で 321 秒） | `settle`・`store_memo`。深さ 10・16・24 が 2 秒未満（共有の機械。基準と同じ程度） | `tests/mutable_slices.rs` `nested_function_values_are_substituted_once`（深さ 14。時間の閾値は置きません） |
| 9（借用を持てない書き込み先） | 再現（`r01`・`r03`・`r11`〜`r13`・`r15`〜`r18` が拒否） | 書き込み先の型の条件、`known`、dyn の事実。すべて受理に戻り、値は基準と同じ（11・11・11・13・11・11・12・12・11） | `a_call_of_a_function_value_stores_only_where_a_borrow_fits`、`tests/rc.rs` `a_dyn_method_applies_the_stores_of_the_instances`（`r11`）、E2E `local_lambda_adds_lengths`・`handlers_then_move` |
| 10（`Arc.share (ref a)` を持つ値の束縛） | 再現（`y01` が拒否） | `Loan::share` の読み替え。`y01` は受理（8） | 3 と同じテストの受理の列、E2E `drop_stores_an_owned_function` |
| 11（呼び出し元の診断の文言） | 再現 | 呼び出し先の名前を挙げる文言と help（`cannot pass this function value here: 'leak' passes it a borrow of a local value of its own, ...`）。kind は同じ | `a_borrow_of_a_callee_local_handed_to_an_unseen_function_value_names_the_callee` |
| 12（region 付きの dyn 値） | 受理を再現（実行は基準でも `SIGABRT`） | dyn の事実と、dyn 値に届く型の場所を書き込み先に数えること。`v24`・`v02` は拒否、対照 `v03`・`v20` は受理 | `a_dyn_method_applies_the_stores_of_the_instances` |
| 14（構造的な比較・表示） | 再現（`disp4`〜`disp6`・`eq2` が受理） | 要素のメソッドの事実を当てはめる | 4 と同じテスト |
| 15（`Rc`／`Arc` の持ち主の loan を落とす） | 再現（`arcself`・`rcself`・`arcself2` が受理、`-O0` で `SIGSEGV`） | `Loan::share` | `a_borrow_kept_in_a_cell_holds_its_owner_and_its_source` |
| 18（古い `parents` だけをたどる） | 再現（`chain3`・`chain3_order`・`chain3_arc` が受理） | `retain_live_loans` | 15 と同じテスト（対照 `chain3_ctl` は受理） |
| 19（`docs/architecture.md` の記述） | 確認（`through` の除外は無く、closed の取り消しは静的データ以外の loan で行う） | `docs/architecture.md` の段落を実装に合わせて書き直しました | — |

この対応で見つけて塞いだ形（レビューの指摘に無いもの）のうち、規則を足したもの:

- **関数値の段階的な適用（`n14`）:** 本体の見えない関数値に引数を一部だけ渡した結果（`let g = h.w2 (ref m)`）は、渡した引数を束縛した関数値で、作る所が `sink_flags` の走査に現れません。そこで、callee が名前付き関数・組み込み関数の直接の参照でない呼び出し（と、名前付き関数が返した関数への引数の追加）で結果の型が関数のものを、引数が書き込み先になりうるか借用を持ちうるなら `sinks` の所（引数がセルを所有するなら `owned_sinks` の所）に数えます。
- **後で実行するコードへ渡した共有者（`n16`）:** セルを所有する関数値のコピーを tzroutine が捕捉した後で、元の関数値に借用を持つ値を書き込ませる形は、基準でも受理され、tzroutine が解放後の値を読みます（`-O3` で 0、期待は 9）。捕捉・チャネルの要素・`Routine.go` などへ渡す owned function・`OnceValue` の初期化・host 関数・region のない dyn 値は、渡す時点で借用を持たないことしか検査しない（A30）ので、モジュールがそうした所で `owned_types` の関数値を持ちうる型の値を渡すとき（`CallFacts::escapes`）は、`store_hidden` が借用の格納を `E1013`/`task_capture_retains_borrow` にします（既存の kind。文言は新しいもの）。

ほかにこの対応で見つけて塞いだ形: 読み取りの仮引数へ貸した一時値への参照の格納（`q5`）、`Mutex.with_lock (ref m)` の部分適用を呼ぶ形（`n01`）、`let mut` の関数値（`n02`）、先に作った `Rc.share` の写し（`n03`）、同じ文の中の先に評価したコピー（`n04`）、セルを所有する関数値を包むラムダ・返す関数・引数に受ける関数値（`n11`〜`n13`）、文字列補間の中の instance の書き込み（`v05i`）。プログラムは `target/scratch/a31/r1/` と `r1/adv/` にあります。

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 9 の「受理が離れた所の文に依存する」は、旗がプログラム全体の性質なので残ります。ただし旗は「種があるか」ではなく「環境へ書き込む関数値を作るか」で、`sinks` が偽のモジュールでは、以前の先端よりも受理が広がります（見えない関数値の環境への格納を数えないので）。文書（`lifetimes.md`）に書きました。
- `owned_sinks` の規則は粗いので、対象を型で狭めました（`owned_types`）。E2E の fixture（`in_place_writes`）に、セルを所有する関数値・書き込む `Drop.drop`・参照を格納する `Eq` の instance を足しても、以前の 30 件を含む 36 件がすべて同じ値で通ります。
- 一時値の `E1013` と、呼び出し先の局所値の借用の `E1013` は、既存の kind `store_local_borrow_through_reference` を使い、文言だけ分けました（診断の kind は増やしていません）。
- `v14`（`to_string c2` で、`Display.display` が `c2` への参照を `Mutex` へ書く）と `v05i`・`v09i` は、基準が受理して解放後の値を出す危険な形（`v14` は `-O0` で不定の値）で、拒否になります。
- テストの期待を変えた既存のテストはありません（`retention_tests::store_facts_follow_what_the_bodies_store` の `Source` に新しい欄を足しただけです）。

確かめたこと（2026-11-11、最後のソースの変更の後。ログは `target/scratch/a31/r1/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3006 件すべて成功、4 件 skip（979 秒、`nextest-final.log`。その後の変更はコメントだけで、`tests/rc.rs`・`tests/mutable_slices.rs`・`tests/currying.rs` の 68 件を再実行して成功）。対応前は 2994 件で、増えた 12 件は `tests/rc.rs` の 9 件、`tests/mutable_slices.rs` の 2 件、単体テスト `retention_tests::the_sink_flags_follow_the_function_values_of_the_module` です。
- 拒否のテストが対応前に失敗すること: 対応前の先端 `49997477` のバイナリの結果（`r1/before.json`）で、表の再現プログラムは受理（指摘 9・10 は拒否）でした。テストはそれらのプログラムの本文をそのまま使っています。
- レビューの 396 個（792 件）: `r1/after-final.json`。対応前から受理に変わったのは指摘 9・10 の安全なプログラムと `blow_10`（時間切れだったもの）だけで、受理のまま残るものは、実行して `-O0` と `-O3` が同じ値でした（`r1/run5.json`。`"{c2}"` と書いた `v05`・`v09`・`v16`・`v17` は文字列補間ではないので、`Display.display` を呼びません）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r1/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r1/step5-final.txt`。`in_place_writes` の fixture には 6 件を足しましたが、基準も同じ値で受理します）。
- E2E（dev のバイナリ、`r1/e2e-final.log`）: `in_place_writes` 36 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`sync_types` 14 件、`tzroutine` 16 件、`control.mjs` 701 件、`user_drop.mjs`。すべて成功。`in_place_writes` の 36 件は基準のバイナリでも同じ値で通ります。
- 文書: `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`・`borrowing.md`）は `Docs: 3 pages, 50 links, 19 checked examples, 38 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`。
- 性能（`TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、21 回、基準と対応後を交互に実行。共有の機械）: `hash_map` 12.8 → 12.6、`list_fsharp` 18.1 → 17.6、`array_fsharp` 10.8 → 10.5、`vec_dotnet` 5.1 → 5.1、`rc` 1.6 → 1.7、`tzroutine` 0.5 → 0.6、`examples/tzroutines` 0.1 → 0.1（0.1 ms の差は分解能と同じ大きさ）。書き込みの事実を多く使う fixture は遅くなります: `mutable_slices` 3.5 → 8.4、`in_place_writes`（セルを所有する関数値・書き込む `Drop.drop`・参照を格納する instance を足した後）2.3 → 13.1（9 回の中央値。旗が上がるたびの不動点の取り直しと、`settle` の 2 回の走査の分です）。指摘 8 のプログラムは、深さ 10・16・24 とも 2 秒未満（基準と同じ程度）です。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 本体の見える局所値・演算子・dyn のメソッド、見えない関数値の 3 つの規則と例、後で実行するコードへ渡した関数値、呼び出し元と一時値の `E1013`）、`drop.md`（解放が書き込む借用: 一時値・呼び出す関数の中の破棄・破棄する値そのものの借用）、`borrowing.md`（書き込んだ値の借用: 演算子と文字列補間）、`docs/language.md`・`docs/architecture.md`（書き込みの事実の段落を実装に合わせて書き直し。指摘 19）。

変えたファイル: `src/ownership.rs`、`src/check.rs`（`Type::owns_mutable_cell`・`Type::stores_some`）、`tests/rc.rs`、`tests/mutable_slices.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第2回）（2026-11-11）

レビューの 11 件の指摘（反例の探索とその再現 `target/scratch/review-a31-ce-r2/`、適合の確認 `target/scratch/review-a31-conf-r2/p/`）を、対応前の先端（`d9b2a96f`）・基準・対応後のバイナリで確かめました。指摘はどれも再現しました（性能の指摘 3・10 は数値も同じ傾向）。結果は `target/scratch/a31/r2/` にあります（`before.json`・`base.json`・`after-r2h.json`、第 1 回の 1146 件の再実行 `r1corpus-r2h.json`、`corpora.sh` で作り直せます）。

設計の変更（`src/ownership.rs`、`src/ownership_control.rs` の呼び出し 1 か所）:

- **環境へ書き込む関数値（`sinks`）を作る所の判定を、渡す値の型で狭めました。** 作る所の本体が、束縛した値を関数値の呼び出しへ渡すときは、その引数の型を通して書き込みが届きうるとき（`may_reach_store`: 借用を持ちうる値の可変セル、排他参照、関数値・dyn 値・`Owned.Function`）だけ数えます（`Source::target`・`Arg::target`。`staged` と同じ考え方）。std の `Array.map`・`Array.mapi` のラムダ（`\index -> transform values[index]`）は要素を、`Array.mapr` は要素への共有参照を渡すだけなので、`[i64]` では数えません。コールバックを取る組み込み関数の部分適用も、束縛した引数の型を通して書き込みが届きうるときだけです（`Mutex<i64>` の `Mutex.with_lock (ref m)` は数えません）。指摘 1・5・6・11。
- **`sinks` の規則を型で狭めました（`CallFacts::sink_types`、`may_be_sink`）。** 環境へ書き込む関数値の型と、それを環境に持ちうる関数値の型だけが、環境の先へ格納する呼び出しと、関数値を持つ場所を書き込み先に数える規則（`store_target`）の対象です。`owned_types` と同じく、関数値はどれもモジュールの作る所で作るので、型が違えば別の関数値です（指摘 7 の `r9`、指摘 6 の形が `sinks` の真なモジュールにあるとき）。
- **セルを所有する関数値（`owned_sinks`）の規則で、引数と局所値を型で狭めました。** 見えない呼び出しが隠れた環境へ届くのは、関数値か引数の型が、所有する関数値の型（`owned_types`。引数をいくつか渡した後の型も: `owned_suffix`）の関数値・dyn 型が対象なら dyn 値・`Owned.Function` を持ちうるか、セルに届くときだけです（`argument_hides`、式の無い引数は呼び出し先が見た `Arg::hides`）。格納を受け取る局所値も同じ条件の型だけです（`shares_hidden`）。`ref string` だけを捕捉したクロージャや、型の違うハンドラーは数えません。仮引数の局所値には呼び出し元の loan だけを足し、局所値の loan は文書どおりの `E1013`（`store_local_borrow_through_reference`）で報告します（以前は仮引数への吸収が先に `assignment_extends_borrow` を出していました）。指摘 2・7。
- **holder（環境を隠す値を束縛した関数値）の型は、束縛した値が所有する関数値（環境へ書き込む関数値）の型の値を持ちうるときだけ加えます**（不動点）。名前付き関数を束縛した値は何も持ちません（`named_locals`。edition 2027 の `callback_adapter` が名前付き関数を包む閉包を含む）。以前は関数値に届く値を束縛した関数値をすべて加えていたので、`Eq` の instance の `\() -> String.length b.name`（`b` が `Mutex<[unit -> i64]>` への参照を持つ）が `unit -> i64` をすべて所有する関数値の型にしていました（`in_place_writes` に足した事例で見つけました）。
- **関数の型の比較（`sink_type`）で、引数の型を囲む参照を除きました。** edition 2027 のラムダの読み取りの仮引数は `ref (unit -> i64)` で、それを持つ値の宣言の型は `unit -> i64` なので、同じ関数値が 2 つの型を持ちます。除くことで比較が一致します（違う型が一致することは集合を広げるだけで、安全側です）。狭めた規則を足した段階で、2027 の `w18` がこのために受理されていたのを見つけて直しました。
- **呼び出し先の局所値の借用の `E1013` は、その借用を書き込みうる先があるときだけ出します。** 関数値自身の loan の書き込み先（`may_be_sink` のときだけ）、呼び出し元から来た引数の書き込み先、隠れた環境のどれかです。呼び出し先で作った関数値を引数なしで呼ぶだけ（`\get -> get () * 2`、名前付きの `plus_one`）は何も格納しません。指摘 5。
- **dyn 値を持つ場所は、その dyn 型の vtable のメソッドが書き込みうるとき（スロット関数の事実が空でない）か、結果の型を通して書き込みが届きうるときだけ書き込み先に数えます**（`CallFacts::dyn_targets`・`dyn_types`、`sink_flags` で求める。vtable の無い dyn 型は数える）。指摘 8。
- **値の破棄のフックで、関数値は作った式で判定します**（`value_runs_drop_code`）。ラムダは捕捉、部分適用は束縛した引数、名前付き関数は何も実行しません。不変な局所値に束縛したものは束縛した式で決めます。指摘 9。
- **不変な局所値が、ラムダ・名前付き関数・部分適用だけを要素に持つ配列リテラル、そうした値をフィールドに持つレコードリテラルに束縛されているときは、要素・フィールドの呼び出しにその本体の事実を当てはめます**（`Checker::known_parts`）。所有する関数値と型が同じハンドラーの表（`r4`）も受理になります。
- **生成した関数の名前の `.$mono.N` を、呼び出し先を挙げる `E1013` の文言から除きました。** 指摘 4。
- **性能:** `Value::absorb` を、足す loan の数に比例する手間にしました（`store_hidden` がすべての局所値へ足すため、以前は局所値の loan の数に比例し、全体で 3 乗でした）。`joint_facts` は、旗が読む関数（作る所の本体と vtable のスロット関数、`flag_sites`）とそれが呼ぶ関数を先に旗と一緒に確定させ、ほかの本文はその後で検査します（以前は旗が上がるたびに大きな本文を検査し直していました）。仮引数のどれも書き込みの届きうる型でない関数は、書き込みの事実を求めるための検査をしません（`storing`。事実は空にしかなりえないため）。予算を超えたときは、確定した事実の旗の型の集合を取り直し、記録した呼び出しの引数を `hides`・`target` とみなし、すべての dyn 型を書き込み先にします。指摘 3・10。

この対応で見つけて塞いだ穴（レビューの指摘に無いもの）:

- **見えない関数値が、自分の環境の値を環境の先へ格納する形。** `apply_opaque` は、引数の loan だけを関数値の環境の先へ格納していたので、借用とセルを捕捉した閉包（`\() -> { Mutex.with_lock mr (\slot -> Array.write 0 (\() -> String.length text) slot); 1 }`）を、配列の要素・レコードのフィールド・返された配列・`IO` の動作・`let mut` などを通して呼ぶと、`text` の借用がセルに残ることを見落としていました。基準と第 1 回の先端は受理し、解放した文字列を読みます（`review-a31-ce/q01`〜`q03`・`q11`〜`q19`・`q21`〜`q24` の 14 個と `r2/adv/v01`・`v02`。期待 9 のところ `-O0`・`-O3` とも 0）。関数値の loan も環境の先と隠れた環境へ格納するようにしました。

指摘ごとの対応（期待はレビューの対照と手の計算から。値は基準と同じことを `-O0`・`-O3` で確かめました: `r2/run-c.json`・`run-base.json`）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（`Array.map`・`mapi`・`mapr` の std のラムダが `sinks` を上げる） | 再現（`t02`・`t03`・`t06`・`u02`・`u11`・`u16` が 2027 で、`u24` が両方の edition で拒否） | 作る所の判定を渡す値の型で狭めた。すべて受理（`t06` 19、`u24` 19 など、基準と同じ） | `tests/rc.rs` `a_bound_value_that_no_store_reaches_through_makes_no_sink`、E2E `dispatch_table_then_move` |
| 2（セルを所有する関数値があると、無関係なハンドラーの配列が拒否される。仮引数があると文言が違う） | 再現（`w05`・`w06`・`w08`・`w09`・`w15`） | 引数と局所値を型で狭め、仮引数には呼び出し元の loan だけを足す。すべて受理（`w06` 19、`w08` 20、`w15` 13） | `a_function_value_of_a_type_that_no_owner_has_reaches_no_hidden_cell`、E2E `other_type_handlers_then_move` |
| 3（`owned_sinks` のもとで見えない呼び出しの数の 3 乗） | 再現（`p01_many_locals_*`） | `Value::absorb`、`flag_sites`、`storing`。`p01` は `store_hidden` を通らなくなった | 時間の閾値は置かない（下の表） |
| 4（`'leak.$mono.2'`） | 再現（`g07`・`g01`） | 宣言の名前にした | `the_error_of_a_generic_callee_names_its_declaration` |
| 5（呼び出し先の閉包を呼ぶだけのコールバックが `E1013`） | 再現（`r17`・`r20_Array_mapi`・`r13m` が 2027 で、`r13j`・`r14`・`r15`・`r13b`・`r13c`・`r13g` が両方で拒否） | 書き込みうる先があるときだけの `E1013` と、作る所の判定の型。すべて受理（`r17` 21 など） | `a_call_that_hands_nothing_keeps_no_borrow_of_the_callee`（と指摘 1 のテストの `r17`・`r20`・`r13j`・`r14`）、E2E `callback_calls_a_callee_closure` |
| 6（参照越しのハンドラーのレコード） | 再現（`r18` が 2027 で拒否） | 指摘 1 と `sink_types`。受理（22） | 指摘 1 のテスト、E2E `handler_record_then_move` |
| 7（純粋なハンドラーにクロージャを渡すと、すべての局所値へ格納） | 再現（`r10`・`r3`・`r4`・`r9`） | 引数の型の判定、`known_parts`、`sink_types`。すべて受理（43・19・19・17） | 指摘 2 のテスト |
| 8（dyn 値を持つ場所が常に書き込み先） | 再現（`dyn1`・`dyn2`） | `dyn_targets`。受理（37）。書き込むメソッド・`ref Mutex` を返すメソッドの dyn は拒否のまま（`r2/adv/v04`・`v05`） | `a_dyn_value_is_a_store_target_only_through_methods_that_store` |
| 9（参照だけを捕捉した閉包の破棄） | 再現（`d6`・`d3`） | `value_runs_drop_code`。受理（35・25） | `dropping_a_closure_runs_only_what_its_captures_drop`、E2E `closure_over_references_then_move` |
| 10（局所値の多い関数の検査が 7 倍） | 再現（`perf_locals_*`） | 指摘 3 と同じ | 下の表 |
| 11（`lifetimes.md` の作る所の一覧が実装より狭い） | 確認 | 実装を狭め、文書を実装に合わせた（下の「更新した文書」） | — |

新しいテストは、どれも対応前の先端（`d9b2a96f`）の `src/` では失敗します（`src/` だけを戻して `cargo nextest run --test rc` で確かめました: 7 件が失敗）。

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 型による区別は、型が同じなら別の関数値でも同じに扱います。所有する関数値と型が同じハンドラーを、本体の見えない形（関数の結果、フィールド、`let mut`）で呼ぶと、ハンドラーが書き込まなくても借用が残ります。不変な配列リテラル・レコードリテラルに束縛したハンドラーは `known_parts` で受理します。文書（`lifetimes.md`）に例を書きました。
- 隠れた環境の規則は、どのセルかを区別しません。関数の中で作ったセルを所有する関数値へ局所値の借用を渡し、所有する関数値と型の同じ仮引数を持つ関数（`r2/adv/w18`）は、仮引数がそのセルを共有しえないのに `E1013` です（第 1 回の先端から同じ。基準は受理）。セルの同一性を追う変更が要るので、この対応では変えていません。
- 単体テスト `retention_tests::store_facts_follow_what_the_bodies_store` の `Source` の期待を、欄の意味の変更に合わせて `hides: false, target: true` にしました（`hides` は「所有する関数値を持ちうるかセルに届く」になり、この単体テストのプログラムには所有する関数値がありません。`target` は新しい欄です）。単体テスト `retention_tests::the_sink_flags_follow_the_function_values_of_the_module` の 3 つめの事例（`a31_mk`）の `owned_types` の数を 2 から 1 にしました。数えていた 2 つめは、ロックへ渡すコールバック `\slot -> Array.write 0 g slot`（`g` を束縛した holder）で、`g` の型 `unit -> i64` は所有する関数値の型ではなく、それを持ちうる型でもないので、holder の不動点では加わりません（上の holder の変更）。ほかの既存のテストの期待は変えていません。
