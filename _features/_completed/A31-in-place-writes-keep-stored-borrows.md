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
- 12 回のレビューの後に残る保守的な制限（基準が受理する安全な形の拒否: 自分の部分の借用を持つ値へのもう一度の `ref mut`（`mix4`・`w05`）、パイプラインの `Rc.share`（`g08`）、兄弟が借用を持ちうるフィールドの代入（`f02`）、要素の部分（`slice_self_safe_elem`）、隠れた環境の規則（`fx3`）、2 つの仮引数の部分を関数値へ渡す一時値（`h01`）、実引数の値が持つ参照の先の部分を別の仮引数の先へ書き込む関数の呼び出し（`crossw`、マージ前の追加の修正の後に残るもの）、指す場所が分からない参照（ほかの呼び出しの結果 `callres`、`let mut` の参照の変数 `mutref`）を通した同じ呼び出し、5 つ以上の部分（`cross5`）、`Array.mapr` を通した呼び出し（`mapr`）、書き込むセルを自分で所有する関数値があるプログラムでの部分適用（`fx3` と同じ規則。マージ前の追加の修正の見直しの後に残るもの）、関数値の仮引数を本体の中で部分適用・パイプラインで呼ぶ関数を通した呼び出し（`fvparam`。下の「Copilot のレビューへの対応（第1回）と fvpart」の後に残るもの）、関数値の仮引数へ渡す値の場所を 4 段で切ったか入れ子の外側にまとめたときの、その大きな部分の借用（`coarse`。同じ節の「レビューの第1回」の後に残るもの。仮引数をそのまま渡す関数を通した呼び出しも同じ大きさの部分になったのは「レビューの第2回」）、edition 2026 で関数値の仮引数を捕捉するラムダを通した呼び出し（ラムダは関数値を所有し、その解放が関数値の環境へ格納しうるものとして扱う `fvlam`。「レビューの第2回」で見つけ、「レビューの第3回」で原因を確かめたもの。「レビューの第2回」で生成したプログラムを検査して見つけた、部分適用が束縛した場所の借用でない引数の `bindref` は「レビューの第3回」で直しました）、`match` の対象に書いた借用を束縛したパターンの変数（`matchref`）とブロックの中で束縛した関数値の別名の部分適用（`blockalias`。どちらも前の先端からあり、「レビューの第3回」のマージの条件の確かめで見つけたもの）、フィールドや要素の場所の `Rc.get` を束縛した局所値（書き込む値の側の実引数、ラムダの捕捉、ほかの引数と一緒に束縛した部分適用の引数。`rcgetpart`）、受け取った関数値を持つ値のうち、仮引数そのものかそのフィールド・要素でないもの（関数値を入れたレコード・配列・タプル・共用体のリテラル、`match` のパターンの変数、型注釈を書いたラムダの仮引数、edition 2027 で読み取りの仮引数やそのフィールドを束縛した局所値と、読み取りの仮引数の関数値をそれを破棄する関数へ渡したもの）の破棄の格納（`fvdrop`。リテラルと局所値の形は「レビューの第5回」で足したもの）、書き込む部分の側の不変の参照の変数・`if`・`ref (deref q1)` が借用元の値全体を生かすこと（参照の仮引数を受け取る関数へ `q1` を渡す形と、再借用を束縛した `q1` のラムダの捕捉も。`refsrc`。この 3 つは前の先端からあり、「レビューの第4回」と「レビューの第5回」で文書にしたもの）、使い終えた共有の持ち主を通した格納を元の所有者へブロックの型で届けるので、共有を持つ値の型と元の所有者の型の両方が持ちうるブロックの型（共有したブロック、その中のブロック、元の所有者のほかの部分のブロック）を持ちうる別の値（同じ型の別のブロック、その型のフィールドを持つブロックでない値、それを中に持つ別の型のブロック、元の所有者のほかのフィールドが持ちうる型の無関係なブロック）への格納も元の所有者に入り、どちらかの型が dyn の値か型引数に届くと届け先がさらに広がり、dyn の値か型引数に届く型の値への格納はブロックの共有が届いたどの先へも入ること（`sharetype`。「レビューの第4回」の修正によるもので、「レビューの第5回」で届ける型を元の所有者の型も届くものに狭め、「レビューの第6回」で dyn の値と型引数に届く型では狭めないようにしたもの）、`Rc.get (ref r)` の借用を、ほかの持ち主がブロックを生かしていても `r` の借用として検査する解放の順序（`rcgetowner`）と、参照の仮引数の `Rc` から関数の中で作った共有が仮引数の借用のままで、その破棄が呼び出し元の持ち主に自分自身の借用を持たせること（`shareparam`。どちらも前の先端からあり、「レビューの第5回」のマージの条件の確かめで見つけたもの）、部分適用の型をプログラム全体の型で比べること（同じ型の部分適用を作るプログラムでの無関係な関数値。「レビューの第2回」）、共有を通したブロックの自己の借用を持つ `Rc` の代入・ムーブ（`r06`・`r07`・`r10`・`r11`）、`m_named_owned`）と、格納の検査の外にある型の検査の穴（2 つ以上の引数を取る関数値を包んだ `Owned.Function` の `Owned.call` が、コピーできない値を束縛した再利用できる部分適用を作る `owned2`。「レビューの第2回」で見つけたもの）、検査の時間、記憶域の寿命と破棄の順序の 1 つのモデルで直す計画は、後続のチケット [A33](../A33-remaining-store-soundness-holes.md) に記録しました（第12回、最終の対応）。そのうち `rcgetpart`・`fvdrop`・`refsrc`・`sharetype`・`rcgetowner`・`shareparam` は、項目ごとに直すチケット [A34](../A34-store-check-safe-rejections.md) へ移しました。Copilot のレビュー 2 の点検で見つけた、ループの検査が終わらない形は、検査の時間に edition 2026 の形として足した後、その対応のレビューで、どちらの edition でも、先にセルを持つ参照の仮引数の先の借用を、書き込むセルを所有する関数値へ渡すループで起きることが分かったので、A34 の `loopplace` へ移しました（その後、検査が終わらない代わりに、ループの本体の位置の `E1017`（`loop_ownership_limit`）で止まるようにしました。下の「Copilot のレビューへの対応（第2回）」の「検査が終わらないループの打ち切り（2026-11-15）」と「借用の数による打ち切り（2026-11-15）」、今の数え方の「借用の増え方による打ち切り（2026-11-16）」）。同じレビューで見つけた、ループの前に先へ自分の部分の借用を書き込んだ関数がループの中で `deref p = v` と置き換えてから新しい値のセルの中身を写す形の拒否（`replcopy`。`if` の片方の枝で置き換えて写した後の置き換え、写した先を後で上書きする形も）と、region を持たないセルのフィールドへ借用を書き込んだ不変な複数 region のレコードが region を 1 つにまとめること（`cellslots`。ループの無い形も）も、A34 に足しました（下の「Copilot のレビューへの対応（第2回）」の「修正のレビュー」）。

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
- 予算を超えたときの扱いを、段階を分けた不動点に合わせました。旗が読む関数を確定させる前に検査が尽きたら、まだ検査していないほかの本文も未確定として `top` にします。どれかの `Drop.drop` が書き込みうるときは、値を破棄する本文がその `Drop.drop` の事実を読むので、すべての本文を `Drop.drop` の依存先にし、`Drop.drop` も先に確定させます。単体テスト `retention_tests::an_exhausted_budget_before_the_other_bodies_leaves_them_storing_anything` を足しました。

確かめたこと（2026-11-11。ログは `target/scratch/a31/r2/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は `3952d22a` で 3013 件すべて成功、4 件 skip（1159 秒、`nextest-final.log`）。対応前の 3006 件から増えた 7 件は `tests/rc.rs` の新しいテストです。その後の変更（予算を超えたときの段階の扱いと、その単体テスト）の後は、`--lib retention_tests` 10 件、`--test rc --test mutable_slices --test currying --test borrowed_records` 96 件を再実行して成功しました。
- レビューの 2 回分のプログラム: 第 2 回の 171 個（338 件）と、第 1 回のディレクトリの 573 個（1146 件。第 1 回の 396 個と、その後に足された 177 個）を `r2/corpora.sh` で確かめました。指摘の安全なプログラム 32 個はすべて受理になり、`-O0`・`-O3` の値が基準と同じです（`run-c.json`・`run-base.json`）。基準が受理する危険なプログラムで対応後に受理されるものはありません。第 1 回の先端から変わったのは、上の受理になった 32 個（両方の edition で受理）と、新たに拒否になった危険な `q` の 14 個（28 件）と、診断の位置が変わった `g18`・`g19`（どちらも拒否のまま。`borrow_outlives_block` から、呼び出しの位置の `store_local_borrow_through_reference` へ）だけです。この対応で書いた危険な形 `r2/adv/`（`v01`〜`v05`、`w17`〜`w21`）はすべて拒否です（`w21` の 2027 は型の誤り）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r2/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r2/step5-final.txt`。`in_place_writes` の fixture に足した 5 件も基準と同じ値で受理されます）。
- E2E（`3952d22a` のバイナリ、`r2/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 41 件（基準のバイナリでも 41 件同じ値で成功）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs` 701 件、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`）は `Docs: 2 pages, 35 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません。
- 性能（`TSUZURI_TIME_PASSES=1 tsuzuri check --edition 2027` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 5 回。共有の機械）:

| プログラム | 基準 | 対応前（`d9b2a96f`） | 対応後 |
| --- | --- | --- | --- |
| `p01_many_locals_50` | 6.0 | 32.2 | 6.6 |
| `p01_many_locals_200` | 85.0 | 452.7 | 84.2 |
| `p01_many_locals_600` | 820 | 4847 | 796 |
| `p01_many_locals_1200` | 3428 | 21875 | 3455 |
| `perf_locals_300` | 153 | 1226 | 399 |
| `perf_locals_600` | 671 | 4437 | 1059 |
| `p05_many_locals_600_nosink` | 382 | 825 | 429 |
| `perf_chain_400` | 3.2 | 22.9 | 18.5 |
| `p03_nested_24`・`p06_nested_loops_8`・`perf_twice_12`・第 1 回の `blow_24` | 0.1〜0.2 | 0.7〜1.7 | 0.4〜1.3 |

  `p01` は `store_hidden` を通らなくなり、基準と同じです。`perf_locals` はセルを所有する関数値を実際に呼ぶ形で、呼び出しごとに同じ条件の型の局所値すべてへ足すので、基準の 1.6〜2.6 倍です（どちらも 2 乗で増えます）。fixture（`hash_map`・`list_fsharp`・`array_fsharp`・`vec_dotnet`・`rc`・`tzroutine`、21 回の中央値）は基準と同じ程度です（`r2/time-fixtures.txt`）。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 配列・レコードのリテラルのハンドラー、dyn 値の場所の規則、環境へ書き込む関数値を作る所の一覧と型による区別、関数値自身の環境の値の格納、呼び出し先の閉包を引数なしで呼ぶ形、セルを所有する関数値の規則の引数と局所値の条件、型が同じハンドラーの例。指摘 11）、`drop.md`（関数値の破棄は作った式で決めること）、`docs/language.md`・`docs/architecture.md`（書き込みの事実の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第3回）（2026-11-11）

レビューの 7 件の指摘（反例の探索 `target/scratch/review-a31-ce-r3/`、適合の確認 `target/scratch/review-a31-conf-r3/mine/`）を、対応前の先端（`fb3c0611`）・基準・対応後のバイナリで確かめました。指摘 1・2・3・5・6 は再現し、指摘 4・7 は指摘のとおりの状態（基準からある診断の位置と、記録済みの制限）でした。結果は `target/scratch/a31/r3/` にあります（`before.json`・`base.json`・`after-c.json`。対応で書いたプログラムは `r3/adv/`）。

設計の変更（`src/ownership.rs`、`src/ownership_control.rs` の呼び出し 1 か所）:

- **`Seq.next` は、列が持つ step 関数を `()` で呼ぶ、本体の見えない関数値の呼び出しとして扱います**（`builtin_stores` の `SeqNext`、`store_candidates` でもコールバックを実行する組み込み関数に数える）。列を関数値とする `apply_opaque` で、列の loan を列の loan の書き込み先へ、セルを所有する関数値の規則では隠れた環境へも格納します。列を仮引数で受け取る本文では、その格納が本文の書き込みの事実になり（`store_external`）、呼び出し元が実引数に当てはめます。`may_be_sink`・`may_own_sink` は、列の型が届く step 関数の型で判定します（`sequence_steps`）。指摘 1。
- **捕捉・束縛した関数値を関数値の呼び出しへ渡すだけのラムダは、環境へ書き込む関数値（sink）に数えません**（`Source::target` を `may_reach_store_beside_code` で求める: 可変セル、排他参照、dyn 値、`Owned.Function` に届く型だけ）。関数値の環境へ書き込めるのはその関数値自身のコードだけで、そのコードを作る所はそれ自体で sink かどうかを判定し、それを捕捉した関数値は holder の不動点でその型を受け継ぎます。関数を返す見えない呼び出し（staged）も、束縛した引数が環境を隠すか、`may_reach_store_beside_code` のときだけ sink にしました（edition 2027 の読み取りの仮引数へ貸した `ref i64` は借用を持つだけで、書き込める場所ではありません。この対応の E2E の事例を足したときに、`Array.map` の `transform values[index]` が `i64 -> i64` を sink の型にし、`TextHandler` の事例を 2027 で拒否させたので見つけました）。指摘 2。
- **値の破棄で実行するコードを、作った式でより広く決めます**（`value_runs_drop_code`）。配列・リスト・タプル・レコード・共用体のリテラルは要素を（レコードと共用体はその型自身の `Drop` も、`has_own_drop`）、不変な局所値は束縛した値を破棄します（`Checker::bound`。共有セルに届く型は中身が後から変わりうるので型で決めます）。呼び出された関数が、不変で参照でない仮引数の値の一部（仮引数、そのフィールド・要素: `parameter_part`）を破棄する格納は、事実の `StoreFacts::drop_only` にも入れ、呼び出し元は実引数の式が破棄でコードを実行しうるときだけ当てはめます（`join` は、どちらかが破棄以外で記録した組を drop-only から外します。破棄以外の格納と合わさった組は常に当てはめます）。場所から読んだ関数値を呼ぶだけの呼び出し（`c.cb ()`）は関数値を複製しないので、callee の値に破棄のフックを当てません（`eval_call` とパイプラインで `bound_next`）。指摘 3。
- **セルを所有する関数値の規則で、値の出どころから所有する関数値を持ちえないと分かる値を除きました**（`Own`・`OwnerScan`・`owner_facts`、`CallFacts::owners`）。値の出どころは、仮引数の外から来た `fresh`、仮引数の値 `values`、関数値の仮引数の呼び出しの結果 `results` の 3 つで表し、型が関数値・dyn 値・`Owned.Function`・可変セルに届かない値は無し、可変セルに届く値は `fresh`（中身が後から変わりうる）、名前付き関数は無し、ラムダ・部分適用・リテラル・フィールド・要素・借用・参照外しは部品の和、不変な局所値は束縛の値（`for` の要素は列の値）、名前付き関数の完全適用は、その関数の事実（本文の結果の出どころの最小不動点）を実引数に当てはめたもの、`Owned.function` は引数、関数値の仮引数の呼び出しは `results`、`new [T](n, f)` は `f` の結果、ほかの組み込み関数・`let mut`・パターンの束縛・ほかの関数値の呼び出しは `fresh` です。出どころの無い式（`Checker::may_own` が偽）は、呼ぶ関数値（`may_own_sink`）にも、隠れた環境へ届く引数（`argument_hides`）にも数えず、そうした値に束縛した不変な局所値（`holds_no_owner`）は `store_hidden` の格納を受けず、束縛と関数の結果の `absorb_hidden` も行いません。事実はセルを所有する関数値の旗（`owned_sinks`）が上がったときに 1 度求めます。指摘 5・6。

この対応で見つけて塞いだ穴: 第 1 回のレビューの `review-a31-ce/m01`〜`m05`・`m07`（`Seq.map`・`for`・`Seq.next`・`Seq.defer` の step 関数が `Mutex` へ借用を書き込む形）は、第 1 回・第 2 回の先端で受理されていました（指摘 1 と同じ原因）。両方の edition で拒否になります。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました: `r3/run-after.json`・`run-base.json`）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（`Seq.next` が実行する step 関数の書き込みを追わない。危険な形を両方の edition で受理） | 再現（`b02`・`b04`〜`b08`・`c05`・`c06`・`b09` が受理、`-O0`・`-O3` とも 0） | `SeqNext` を本体の見えない関数値の呼び出しにした。すべて `E1013`（`assignment_extends_borrow`、ブロックの位置）。対照 `b02_twin`・`b07_twin` は受理（9） | `tests/rc.rs` `a_step_function_that_seq_next_runs_keeps_what_it_stores`（第 1 回の `m07` も）、E2E `seq_step_writes_a_long_lived_borrow` |
| 2（2027 で `twice`・`bracket` のような組み合わせ関数が `E1013`） | 再現（`f01`・`f03`・`f06_min_1`・`f07_d`、第 2 回の `perf_twice_4`〜`12` が 2027 で拒否） | 捕捉した関数値を渡すだけのラムダを sink にしない。すべて受理（`f06_min_1` 10、`f07_d` 37、`f03` 13） | `a_lambda_that_hands_a_captured_function_value_on_is_no_sink` |
| 3（書き込む `Drop.drop` があると、参照とその参照だけを捕捉したラムダのレコードリテラルが `E1014`） | 再現（`g22` が 2026 で拒否）。原因は 2 つで、呼び出し元の一時値の破棄のフックと、呼び出された関数の中の `c.cb ()` の callee の破棄のフック（仮引数の値の一部を型で判定）でした | リテラル・不変な局所値の破棄、drop-only の事実、場所から読んだ callee。`g22` は受理（14）。`let` で束縛した同じ形（`r3/adv/d03`）も受理。呼び出された関数が部品から `Stash` を作って破棄する形（`r3/adv/d01`）は拒否のまま | `dropping_a_literal_runs_only_what_its_parts_drop` |
| 4（利用者のコードの誤りが `std/Array.tz:2003:32` に出る。文言が基準と違う） | 指摘のとおり（`g19`・`c08` とも基準も枝も拒否、位置は同じ） | 変えていません（下の判断） | — |
| 5（セルを所有するレジストリが 1 つあると、無関係な同じ型のハンドラー・サンクの呼び出しが `E1014`） | 再現（`s01`・`s02`・`s05`・`s06`・`s07`・`s09`・`s13`・`s16`） | 出どころの規則。すべて受理（22・23・22・46・20・43・27・27） | `a_value_that_cannot_hold_an_owner_reaches_no_hidden_cell`、E2E `registry_and_mapped_handlers`・`functions_beside_a_named_function` |
| 6（名前付き関数・捕捉の無いラムダに束縛した局所値が隠れた環境の格納を受ける） | 再現（`s15`・`s13` の `base`） | `holds_no_owner`。受理（27） | 指摘 5 のテスト（`s15`・`s13`） |
| 7（`w18` が拒否のまま） | 指摘のとおり（両方の edition で `10:5` の `store_local_borrow_through_reference`） | 変えていません（下の判断） | — |

出どころの規則は、所有する関数値のコピーを型で数えていた所を狭めるので、危険な形を 29 個書いて確かめました（`r3/adv/p01`〜`p19`、`q01`〜`q20` の番号の一部: レジストリのコピーを配列リテラル・`Array.map`・`Array.mapi`・恒等関数・レコードを返す関数・ラムダの捕捉・関数値の結果・`Maybe` の `match`・`for`・`Array.fold` のコールバックの仮引数・`if`・タプル・`Owned.function`・部分適用・再帰関数を通して呼ぶ形と、レジストリ自身へ書き込んで、コピーから読み戻す形）。どれも両方の edition で `E1013`（`assignment_extends_borrow`）です（基準はすべて受理）。

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 4 は変えていません。std の関数の本文で見つけた誤りを std の本文の位置で報告するのは基準からの振る舞いで、受理の結果は基準と同じです（`g19` は安全ですが基準も拒否します）。呼び出し側の位置へ移すのは、検査した本文の外へ診断を写す別の変更なので、このチケットでは扱いません。
- 指摘 7（`r2/adv/w18`）は第 2 回の記録どおり残します。仮引数の関数値が局所のセルを共有しえないことは、どのセルへの格納かを区別しないと示せません（出どころの規則は値が所有する関数値を持ちうるかだけを見ます）。
- 出どころの規則でも、型が同じで出どころを追えない値は数えます。この対応で、次の 2 つの形を `lifetimes.md` の「現在の制限」に書きました（どちらも基準は受理する安全な形で、環境へ書き込む同じ型の関数値がモジュールに無ければ受理です）: 環境へ書き込む `unit -> i64` の関数値があるモジュールで、引数のフィールドの関数値を呼ぶ関数（`fn fill_with c = { let v = c.cb (); Array.write 0 (\() -> v) c.buf }`、`r3/adv/l02`）は、フィールドの関数値の環境を引数全体で近似するので、引数の借用を引数の書き込み先へ残します（両方の edition で `E1014`）。edition 2027 で、同じモジュールの `twice h g = h (\() -> h g)` に読み取りの仮引数を持つラムダを渡す形（`r3/adv/l01`）は `E1013` です。そのため、E2E の fixture（`in_place_writes`。環境へ書き込む `unit -> i64` の関数値を含みます）には、指摘 2・3 の形を足していません（`tests/rc.rs` のテストが、そうした関数値の無いモジュールで受理を確かめます）。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-11。ログは `target/scratch/a31/r3/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（`r3/clippy.log`）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3018 件すべて成功、4 件 skip（1614 秒、`r3/nextest-1.log`。ソースはこの対応の最後のものと同じで、その後はテストと文書だけを変えました）。第 2 回の 3014 件から増えた 4 件は `tests/rc.rs` の新しいテストです。
- 新しいテストが対応前に失敗すること: `src/ownership.rs`・`src/ownership_control.rs` だけを `fb3c0611` に戻して `cargo test --test rc` を実行し、新しい 4 件がすべて失敗することを確かめました（戻した後は先端に戻しています）。
- レビューの 3 回分のプログラム: 第 3 回の 97 個（194 件、`r3/after-c.json`）と、この対応の `r3/adv/` の 39 個（78 件）、第 1 回・第 2 回の 1146 件・338 件・`r2/adv/` の 26 件（`r2/corpora.sh r3c`）を確かめました。第 1 回・第 2 回のプログラムで変わったのは、上の `m01`〜`m05`・`m07`（12 件、受理から拒否）だけです。受理になった安全なプログラム（指摘 2・3・5・6 の 14 個と、この対応の `d03`・`g22r` など）は、`-O0`・`-O3` の値が基準と同じです（`r3/run-after.json`・`run-base.json`）。基準が受理する危険なプログラムで対応後に受理されるものはありません。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r3/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r3/step5-final.txt`。`in_place_writes` の fixture に足した 3 件も基準と同じ値で受理されます）。
- E2E（`r3/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 44 件（基準のバイナリでも 44 件同じ値で成功）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs` 701 件、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`）は `Docs: 2 pages, 35 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`（どちらも `TSUZURI_BIN` に dev のバイナリ）、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません。
- 性能（`TSUZURI_TIME_PASSES=1 tsuzuri check --edition 2027` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 5 回。共有の機械が混んでいて、同じプログラムでも回によって 2 割ほど違います。`r3/time-perf.txt`）: `p01_many_locals_600` 387 / 408 / 414、`p05_many_locals_600_nosink` 420 / 437 / 431、`perf_locals_600` 354 / 789 / 799、`perf_twice_12` 0.17 / 0.56 / 0.57、第 3 回の `p01_known_tree_24` 0.33 / 1.32 / 1.34、`p03_hof_chain_40` 0.50 / 1.41 / 1.43、`ring_400` 2.2 / 11.0 / 11.2、`chain_1600` 15.9〜23.1 / 84.7〜117.0 / 108.0〜138.3、`perf_chain_400` 3.2〜5.1 / 18.5〜22.2 / 22.9〜33.9、fixture の `hash_map` 5.6 / 5.0 / 5.0、`list_fsharp` 9.0 / 7.9 / 7.9、`iteration_protocol` 2.5 / 2.4 / 2.3。セルを所有する関数値と多くの関数を持つ `chain_*` は、出どころの事実（全関数の本文の走査の不動点）と本文ごとの出どころの走査の分だけ遅くなります（2 割前後）。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 見えないコードの除外に `Seq.next`、`Seq.next` の step 関数の書き込み、sink を作る所の一覧（捕捉した関数値を渡すだけのラムダ、関数を返す呼び出しの引数の条件）、出どころの規則と、出どころを追えない所の例、同じ型の関数値がある場合の 2 つの形）、`drop.md`（リテラル・不変な局所値の破棄、呼び出された関数の中の破棄、場所から読んだ関数値の呼び出し）、`docs/language.md`・`docs/architecture.md`（書き込みの事実の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第4回）（2026-11-12）

レビューの 6 件の指摘（反例の探索 `target/scratch/review-a31-ce-r4/`、適合の確認 `target/scratch/review-a31-conf-r4/`）を、対応前の先端（`58cc81a6`）・第 3 回の前（`fb3c0611`、`review-a31-conf-r4/pre-bin/`）・基準・対応後のバイナリで確かめました。6 件とも指摘のとおりに再現しました。結果は `target/scratch/a31/r4/` にあります（`r4corpus-before.json`・`r4corpus-b.json` ほか、対応で書いたプログラムは `r4/adv/`、`r4/corpora.sh` で作り直せます）。

設計の変更（`src/ownership.rs` だけ）:

- **値の破棄は、`Drop.drop` の本文が隠れた環境へ格納するもの（`StoreFacts::hidden`）を当てはめます**（`DropHidden`、`Checker::drop_hidden`）。`owned_sinks` のモジュールで、破棄でその `Drop.drop` を実行しうる値について、事実が中身（`(0, Contents)`）を持てば、破棄のフック（束縛・代入で捨てる値・`eval` が評価する束縛しない値）が値の loan を `store_hidden` で格納し、束縛の後に値のセルへ入った loan も、`apply_store_within` と `store_through` の局所の枝で同じく格納します（`drop_hides_owner`。入れ子の格納は `drop_hiding` で 1 度に）。事実が値そのもの（`(0, Value)`）を持てば、束縛した局所値の loan を `store_hidden_except`（その局所値自身は受けない）で格納し、一時値と値で渡す値は `E1013`（`store_local_borrow_through_reference`、`cannot drop this value here: its Drop.drop hands a borrow of the dropped value to a function value ...`）です。`top` は両方です。値の出どころ（`may_own`、`holds_no_owner`）から所有する関数値もセルも持ちえない値は除きます。そうした値の `Drop.drop` は、値の外のセルを所有する関数値に届かないためです（`drop` の中で作ったレジストリは `drop` の終わりで解放され、値のセルにも入りません。後で実行するコードへ渡せば `escapes` で `drop` の本文が拒否されます）。指摘 1。
- **`Drop.drop` の中で、破棄する値の場所の借用を `Rc.share`・`Arc.share` などに渡した結果の loan に `Loan::share` の印を付けます**（`share_loans`・`read_through` の写し。今までは外部のルートの loan をそのまま返していました）。`store_external`・`store_hidden`・`source_of` は、印の付いた仮引数の loan を `Part::Contents`（値の中身）として記録します。共有はブロックの所有者で値そのものの借用ではないので、それを通して `s.inbox` の要素を `s.out` のセルへ移す `drop` は、`drop_borrows_self` に数えなくなります。印は `Drop.drop` の本文の中だけで付き、ほかの本文の事実は変わりません。指摘 4。
- **`OwnerScan` で、`mut` の仮引数は `fresh` です**（`owner_facts` と `Checker::owner_scan` の `params` から外す。`let mut` の局所値と同じ扱い）。指摘 5。
- **`OwnerScan::own` は式ごとの答えを覚えます**（`memo`、式のアドレスが鍵。本文の束縛は使う所より前にあるので、答えは見つけた後に変わりません）。呼び出しは実引数を自分のためと callee の事実のために 2 度読むので、引数を返す関数の連鎖が段ごとに 2 倍になっていました。指摘 2。
- **`owner_facts` は、挙げる関数（呼び出し・部分適用・持ち上げたラムダ）の事実が増えた本文だけを走査し直す作業リストです**（最後の関数から始める。型の判定 `owner_types` は走査の間で共有）。指摘 6。
- **`OwnerScan::call` で、`Rc`・`Arc` の `new`・`get`・`share`・`try_unwrap`・`downgrade`・`upgrade` は、`Owned.function` と同じく引数の出どころを返します**（ブロックの値は作った後に置き換わらず、セルのブロックは型で `fresh` です）。指摘 3 の一部。

この対応で見つけて塞いだ穴（どれも基準と対応前の先端が受理していた危険な形で、両方の edition で `E1013` になります）: 値のセルに最初から借用があり、`drop` がそれをレジストリへ移す形（`review-a31-ce-r4/d11`）、`Array.write` が上書きで捨てる要素の `drop`（`d22`）、値そのものの借用（`ref s.name` を捕捉したラムダ）をレジストリへ渡す `drop`（`d23`、`d23w`）とその一時値（`r4/adv/s02`。`cannot drop this value here: ... hands a borrow of the dropped value to a function value ...`）、`Rc` に入れた文字列を借りる形（`d12h`）。`drop` 自身が後で実行するコードへ渡したレジストリを呼ぶ形（`r4/adv/e01`）は、今までどおり `drop` の本文で `task_capture_retains_borrow` で、束縛の位置にも同じ誤りが出ます。指摘 4 と同じ原因で拒否していた安全な形（`drop` の中の `Rc.share (ref s.inbox)`・`Arc.share (ref s.out)`、`r4/adv/s06`）も受理になります（5）。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました: `r4/run-after.json`・`run-base.json`）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（暗黙の破棄が `Drop.drop` の隠れた環境への格納を当てはめない。危険な形を両方の edition で受理） | 再現（`d12`・`d13`・`d14`・`d15`・`d20`（2026）・`d21`・`d09` が受理、`-O0`・`-O3` とも 0。対照 `d12_ctl` は 9） | 破棄のフックが `Drop.drop` の hidden の事実を当てはめる。すべて `E1013`（`assignment_extends_borrow`、借用元を宣言したブロックの位置。`d13`・`d14` は `let k = { ... }` のブロック）。対照 `d12_ctl`・`d10`（9）、`d17`・`d18`・`d19` は今までどおり | `tests/rc.rs` `an_implicit_drop_applies_what_its_drop_stores_in_hidden_environments`（`d11`・`d22`・`d23`・`s02` と、所有の値だけを渡す `s01`・`s04`、所有する関数値を持たない値の `s03`、値と一緒に解放されるレジストリの `s05` の受理も） |
| 2（`OwnerScan` が実引数を 2 度評価し、呼び出しの連鎖の深さで検査時間が倍になる） | 再現（`p08_builder_pipe_22` の `ownership` が 5 秒台、`p01_nested_id_26` が 33 秒） | 式ごとの答えを覚える。下の表 | 単体テスト `the_owner_provenance_looks_at_each_expression_once_and_follows_the_callers`（24 段の入れ子で `own_uncached` が段の数に比例。時間の閾値は置きません） |
| 3（`Seq.to_array`・`Rc.get` から読んだハンドラーを所有する関数値に数える） | 再現（`u_seq_to_array`・`u_rc_get`・`u_seq_map_lit` が両方の edition で `E1014`、基準は受理 14） | `Rc`・`Arc` の関数は引数の出どころ。`u_rc_get`・`u_rc_direct` は受理（14）。`Seq.to_array`・`Seq.map` の形は拒否のまま（下の判断。`lifetimes.md` の出どころを追えない所の一覧と説明に書いた） | `a_mutable_parameter_holds_what_the_body_assigns`（`u_rc_get`） |
| 4（自分のセルの要素を別のセルへ移す `Drop.drop` を、値そのものの借用を書き込む `drop` とみなす） | 再現（`d06` が両方の edition で `E1013` 9:5、基準は受理 9）。原因は `drop` の本文の `let out = Arc.share (ref s.out)` の束縛のフックで、`out` の loan（`s.out` の借用そのもの）が `(0, Value, 0)` になっていました | `drop` の中の共有の loan に印を付け、`Part::Contents` として記録する。`d06` は受理（9）、借用元が先に解放される `d05` は拒否のまま（8:5） | `a_share_of_a_block_of_the_dropped_value_is_no_borrow_of_it`（`d06`・`d08`・`s06` の受理、`d05` の拒否） |
| 5（`mut` の仮引数を実引数の値とみなし、レジストリを返す形を受理。第 3 回からの退行） | 再現（`m01`（2026）・`m02`・`m03`・`m04` が受理、`-O0`・`-O3` とも 0。`fb3c0611` は拒否） | `mut` の仮引数は `fresh`。すべて `fb3c0611` と同じ位置の `E1013`（`m03`・`m04` 10:17、`m01`・`m02` 11:17） | `a_mutable_parameter_holds_what_the_body_assigns`、単体テスト（`renew`・`pick` の事実が `fresh`） |
| 6（宣言の順が呼び出しの逆の連鎖で `owner_facts` が 2 乗） | 再現（`perf_rev_chain_2000` の `ownership` が 2 秒台） | 呼び出し元の作業リスト。下の表 | 単体テスト（120 個の逆順の連鎖で、本文の走査が関数の数の 3 倍以下） |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 3 の `Seq` の形は受理に戻していません。列の要素は列が持つ step 関数の結果で、`for` も `Seq.next` と `match` に展開されます（`src/control.rs`）。出どころ（`Own`）は値が捕捉したものを表し、値が持つ関数を呼んだ結果を表さないので、`Seq.next` の結果を列の出どころとみなすと、呼ぶたびにレジストリを作る step 関数の列（`r4/adv/f04`〜`f06`: 利用者の関数が `Seq.defer` で作る列を `for`・`Seq.next`・`Seq.to_array` で読む形）を受理してしまいます。step 関数の結果まで追うには、列のリテラルが step 関数の結果を含む規則、パターンの束縛、`let mut` の流れを区別しない合併を足す必要があり、それぞれ別の健全性の確認が要るので、このチケットでは扱いません。安全な形は、ハンドラーを配列リテラルに入れるか名前付き関数の結果として受け取れば受理されます（`lifetimes.md` に書きました）。
- 一時値の `drop` が値そのものの借用をセルを所有する関数値へ渡しうるときの `E1013` は、値の出どころがそうした関数値もセルも持ちえないときは出しません（`s03`）。`Drop.drop` の事実は、値のフィールドの関数値を呼ぶ形（`o s.f`）では中身（`Contents`）、値の場所の借用を渡す形（`ref s.name`）では値そのもの（`Value`）になるので、所有の値だけを渡す一時値（`s01`）は受理のままです。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-12。ログは `target/scratch/a31/r4/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（`r4/clippy.log`）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3022 件すべて成功、4 件 skip（1164 秒、`r4/nextest-1.log`。共有の機械が混んでいて 12 件が slow）。第 3 回の 3018 件から増えた 4 件は `tests/rc.rs` の 3 件と単体テスト 1 件です。
- 新しいテストが対応前に失敗すること: `src/ownership.rs` だけを `58cc81a6` に戻して `cargo test --test rc` を実行し、新しい 3 件がすべて失敗することを確かめました（戻した後は先端に戻しています）。単体テストは対応前の関数（`owner_facts_counted`・`visits`）が無いので、対応後のコードの一部を 1 つずつ戻して確かめました: 覚えた答えを使わないと `33554434 looks` で、`mut` の仮引数を外さないと `renew` の事実で、呼び出し元の作業リストを毎回すべての関数の走査に戻すと `11651 scans of 131 functions` で失敗します。
- レビューの 4 回分のプログラム（`r4/corpora.sh`、対応前と対応後のバイナリ）: 第 4 回の 346 件で変わったのは 38 件で、すべて上の表のとおり（危険な形 17 個が受理から `E1013`（`d20`・`m01` は edition 2026 だけ）、`d06`・`u_rc_get`・`u_rc_direct` が拒否から受理）です。第 3 回と `r2/adv`・`r3/adv`・`r4/adv` の 294 件で変わったのは `s02`（受理から `E1013`）・`s06`（拒否から受理）と、新しく書いた `e01` だけ、第 2 回の 338 件と第 1 回の 1146 件は 0 件です。性能のプログラム（`p0*`・`perf_*`）は対応前のバイナリで終わらないものがあるので、下の表で別に測りました。受理になった安全なプログラムと受理のままの対照（12 個）は、`-O0`・`-O3` の値が基準と同じで手の計算どおりです（`d06`・`d08`・`d10`・`d12_ctl` 9、`s01` 8、`s03` 1、`s04` 7、`s05` 3、`s06` 5、`u_rc_get`・`u_rc_direct` 14。`r4/run-after.json`・`run-base.json`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r4/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r4/step5-final.txt`）。
- E2E（`r4/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 44 件（基準のバイナリでも 44 件同じ値で成功）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs` 701 件、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`）は `Docs: 2 pages, 37 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`（どちらも `TSUZURI_BIN` に対応後のバイナリ）、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（新しい文言は既存の `store_local_borrow_through_reference` です）。
- 性能（`TSUZURI_TIME_PASSES=1 tsuzuri check --edition 2027` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。機械の負荷が 50〜70 で、同じプログラムでも回によって 3 割ほど違います。`r4/time-perf.txt`）: 指摘 2 の `p08_builder_pipe_22` 0.29 / 5273 / 0.80、`p07_builder_chain_22` 0.33 / 4859 / 0.82、`p01_nested_id_20` 0.23 / 464 / 0.86、`p05_nested_wrap_own_22` 0.31 / 4697 / 0.65（`p01_nested_id_26` は対応後 1.0、`p01_nested_id_60` は 0.74 で終わります）。指摘 6 の `perf_rev_chain_500` 1.0 / 242 / 4.4、`perf_rev_chain_2000` 3.3 / 4199 / 15.8、`perf_fwd_chain_2000` 4.4 / 22.3 / 18.6。第 3 回までの表の `p01_many_locals_600` 805 / 799 / 792、`perf_locals_600` 1146 / 2577 / 2647、`chain_1600` 43 / 241 / 206、`ring_400` 7.7 / 24.6 / 25.7、`perf_chain_400` 6.4 / 48.7 / 62.7、`p01_known_tree_24` 1.05 / 3.24 / 2.21、`p03_hof_chain_40` 0.84 / 2.34 / 3.06、`perf_twice_12` 0.37 / 0.95 / 1.43。セルを所有する関数値を持つモジュールの `ownership` は、基準より 2〜5 倍のままです（出どころの事実と本文ごとの走査の分）。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 出どころを追える値に `Rc`・`Arc` の関数、`mut` の仮引数は追えない値、値の破棄も同じ規則で検査すること、追えない所の一覧に `mut` の仮引数・組み込み関数の結果・列から読んだ値、列の要素の説明）、`drop.md`（解放が書き込む借用: セルを所有する関数値へ渡す `drop`、一時値の `E1013` の文言、例、`drop` の中の共有）、`docs/language.md`・`docs/architecture.md`（書き込みの事実の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`tests/rc.rs`、上の文書、このチケット。

### レビューの指摘への対応（第5回）（2026-11-12）

レビューの 5 件の指摘（反例の探索 `target/scratch/review-a31-ce-r5/`、適合の確認 `target/scratch/review-a31-conf-r5/`）を、対応前の先端（`a562f692`。ソースは `de7a3574` と同じ）・基準・対応後のバイナリで確かめました。指摘 1・3・4・5 は指摘のとおりに再現しました。指摘 2 は、指摘のとおり受理される危険なプログラムとしては再現できませんでしたが、鍵の誤りは実在するので直しました。結果は `target/scratch/a31/r5/` にあります（レビューの 5 回分のプログラムの対応前と対応後は `r5/corpora.sh before|a` の `r1corpus-*.json`〜`r5corpus-*.json`、この対応で書いたプログラムは `r5/adv/` と `r5/f3/`、`r5/t.sh` で対応前・対応後・基準を並べます）。

設計の変更（`src/ownership.rs` だけ）:

- **holder の不動点を作業リストで求めます**（`HolderReach`、`SuffixTrie`）。holder が持つ値の届く関数値の型を、結果と最後から最初への引数の列の道として 1 つの trie に入れ（結果と引数の型には 1 度だけ番号を付けます）、道の節ごとにそこを通る holder を覚えます。型がいくつかの引数の後に与える型（`owned_suffix` の接尾）は道の上の節なので、`owned_types`（`sink_types`）に型が加わったら、その節を通る holder だけを見直します。今までは、型が 1 つ加わるたびにすべての holder を見直し、そのたびに持つ値の型の接尾をすべて作って集合と比べていたので、前の閉包を捕捉した閉包を型を長くしながら連ねる形で、検査の時間が段の数の 3.5 乗ほどで伸びていました。求める集合は前と同じ最小不動点です。指摘 1。
- **本文の検査の出どころの走査は、本文の式の答えだけを覚えます**（`OwnerScan::lasting_in`、`OwnerScan::query`）。答えの鍵は式のアドレスで、検査は捕捉ごとに一時的な `TypedExpr` を作って評価するので（`capture_loans`）、次の捕捉の一時的な式が前の式のアドレスを取ると、前の答え（レジストリを持たない値の答え）を受け取りえました。本文の式（検査の間ずっと生きている）だけを覚え、ほかの式の答えは 1 つの問いの間だけ持ちます。`owner_facts` の走査はモジュールの本文しか見ないので、今までどおりすべて覚えます。指摘 2。
- **パターンの局所値は、パターンがそこへ入れる照合した値の射影の出どころを持ちます**（`OwnerScan::bind_patterns`）。今までは `match` で取り出した値をすべて `fresh`（出どころを追えない値）にしていたので、`Rc.try_unwrap` の `Result` や `Maybe`・タプルから取り出したハンドラーを、セルを所有する関数値に数えていました。射影はコード生成が局所値へ入れる式そのもので（`PatternAlternative::bindings`）、アクティブパターンの段は関数の呼び出しなので、その事実（`owner_facts`）を当てはめます。OR パターンの選択肢は同じ局所値を束縛するので、出どころは選択肢の和です。`mut` の局所値と、`Seq.next` の結果（`for x in seq` の要素）は今までどおり `fresh` です。指摘 3。
- `owner_scan` の上に残っていた段落を `shares_hidden` へ移しました。指摘 5。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（前の閉包を型を長くしながら捕捉する閉包の連鎖で、検査時間が段の数の 3.5 乗ほどで伸びる） | 再現（`perf_holders3_480` の `ownership` が 1 万 4 千 ms 台、基準は 25 ms。標本では時間のほぼすべてが `sink_flags` の holder の不動点の `owned_suffix`） | 作業リストと trie。下の表（2 乗の伸びに戻り、基準の数倍） | 単体テスト `closing_the_types_under_the_holders_follows_the_types_that_join`（100 段の連鎖のすべての閉包の型が `owned_types` に入る、連鎖の held 型が trie の 1 本の道になる、見直しの回数が道の節の数以下、200 回の乱数の holder の組で前のパスと同じ集合。時間の閾値は置きません） |
| 2（`OwnerScan` の答えを式のアドレスで覚え、`capture_loans` の一時的な式も問う。未確認） | 受理される危険なプログラムは作れませんでした（`om02`・`om03` は対応前も対応後も `E1014`、基準は受理）。アドレスが再利用されれば前の答えを受け取ることは、単体テストで確かめました | 本文の式だけを覚える | 単体テスト `the_owner_provenance_forgets_the_expressions_made_for_a_moment`（同じアドレスの一時的な式を、捕捉の無いラムダ、レジストリの順に問う。2 つめが `fresh`） |
| 3（`lifetimes.md` は `Rc.try_unwrap`・`Rc.upgrade` で取り出したハンドラーを数えないと書くが、枝は数える） | 再現（`ok01`・`ok02` が両方の edition で `E1013`、基準は受理 9） | パターンの局所値の出どころ。`ok01` は受理（9）。`ok02` は拒否のまま（下の判断。`lifetimes.md` に書いた）。同じ原因で拒否していた第 4 回の `u_maybe_map`・`u_maybe_some_match`・`u_tuple_match` も受理（14、基準と同じ） | `tests/rc.rs` `a_local_that_a_pattern_binds_holds_what_the_matched_value_holds`（`ok01`・`u_maybe_map` と、この対応の安全な形 11 個の受理、危険な形 17 個の拒否） |
| 4（第 4 回に受理になった安全なプログラムの値を、自動のテストが確かめない） | 指摘のとおり（`d06`・`d08`・`s06`・`u_rc_get` は `accepts_in_both_editions` だけ） | `in_place_writes` の fixture と `tests/features.mjs` に足した | E2E `drop_shares_its_rc_and_writes_the_out`（5）、`handler_read_from_an_rc`（13）、`handlers_taken_out_by_patterns`（37、第 5 回の形）、native だけの `drop_moves_its_inbox_through_a_share`・`drop_moves_its_inbox_in_a_helper`（9） |
| 5（`owner_scan` の上に `shares_hidden` の段落が残り、`shares_hidden` に説明が無い） | 指摘のとおり | 段落を移した | — |

パターンの局所値の規則は、追えない値を狭めるので、危険な形を 17 個書いて確かめました（`r5/adv/p01`〜`p17`: レジストリを `Maybe`・タプル・レコード・リスト（`::`）・配列の要素のタプル・入れ子のパターン・ガード・`ref` の照合・OR パターン（どちらの選択肢から取り出すものも、`if` でどちらかを作るものも）・`Rc.try_unwrap`・`Rc.upgrade` から取り出す形、引数を返すアクティブパターン、ラムダの中の `match`、レジストリを作るアクティブパターン・名前付き関数の結果から取り出す形）。どれも両方の edition で `E1013`（`assignment_extends_borrow`、ブロックの位置）で、基準はすべて受理します。同じ形でレジストリのかわりに捕捉の無いラムダを取り出す安全な形（`s01`〜`s14`）は、`s08`・`s14` を除いて受理になり、値は基準と同じ 10 です。

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 3 の `ok02`（`match Rc.upgrade (ref w) with | Maybe.Some s -> deref (Rc.get (ref s))` で取り出したハンドラーを呼ぶ形。`s08` も同じ）は受理に戻していません。対応後、このハンドラーはセルを所有する関数値の規則では数えません（出どころが無い）。拒否の理由は、環境へ書き込む関数値の規則（`lifetimes.md` の 2 つめの規則）です: 借用を持つ値を本体の見えないコードへ渡す本体では、`Rc.share`・`Rc.upgrade` などの結果は元の `Rc` の借用を持ち（A28 の読み通しの制限。`lifetimes.md` に書いてある規則）、`Rc.get` で読んだハンドラーはその借用を持つので、環境へ書き込みうる型の関数値の呼び出しが、渡した借用を元の `Rc` に残します。`Rc.share` で作った値から読む形（`r5/f3/v7`）も同じで、`Rc.get (ref r)` で元の `Rc` から直接読む形（`v9`、第 4 回の `u_rc_get`）は受理です。受理するには、ハンドラーのコードが環境へ書き込まないこと（`Rc` のブロックを通した関数値の追跡）か、そうした本体での読み通しが要り、どちらも別の健全性の確認が要るので、このチケットでは扱いません。第 3 回の前（`fb3c0611`）と第 4 回（`58cc81a6`）も同じく拒否していました。`lifetimes.md` の出どころの規則の段落に、この規則で数えないことと、2 つめの規則では残ることを書きました。
- この対応で書いた `s14`（ラムダに束縛した局所値を呼んだ結果を取り出す形）は拒否のままです。名前付き関数でない関数値の呼び出しの結果は、これまでどおり出どころを追えない値です（`lifetimes.md` の一覧の「関数値の呼び出しの結果」）。
- 指摘 1 の単体テストは、対応前のコードに trie が無いので、対応前に失敗することは示せません。効果は下の時間の表で示し、テストは不動点の結果（前のパスとの一致）と構造（接尾を作り直さないこと）を確かめます。
- 指摘 4 の `d06`・`d08` の形は、`drop` が inbox のロックを持ったまま `a` のロックを取るので、tzroutine のランタイムを使わない WebAssembly ではトラップします（`mutex.md` の「WebAssembly のロック」の規則どおりで、基準のバイナリも同じです）。そのため `nativeCases` に置きました。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-12。ログは `target/scratch/a31/r5/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（`r5/clippy.log`）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3025 件すべて成功、4 件 skip（787 秒、`r5/nextest-1.log`。7 件が slow。ソースはこの対応の最後のものと同じで、その後は文書だけを変えました）。第 4 回の 3022 件から増えた 3 件は、単体テスト 2 件と `tests/rc.rs` の 1 件です。
- 新しいテストが対応前に失敗すること: `src/ownership.rs` だけを `a562f692` に戻して `cargo test --test rc` を実行し、`a_local_that_a_pattern_binds_holds_what_the_matched_value_holds` が失敗することを確かめました（`ok01` の受理で失敗。戻した後は先端に戻しています）。単体テスト `the_owner_provenance_forgets_the_expressions_made_for_a_moment` は、本文の式だけを覚える条件を外す（すべて覚える）と `the registry at the same address` で失敗します。
- レビューの 5 回分のプログラム（`r5/corpora.sh`、対応前と対応後のバイナリ、両方の edition）: 第 1 回の 1146 件・第 2 回の 338 件・第 3 回と `r2/adv`・`r3/adv` の 270 件で変わったものは 0 件、第 4 回と `r4/adv`・`r4/safe` の 394 件で変わったのは `u_maybe_map`・`u_maybe_some_match`・`u_tuple_match`（6 件、拒否から受理。値は基準と同じ 14）、第 5 回と `r5/f3` の 338 件で変わったのは `ok01`・`f3/v5`（4 件、拒否から受理。9 と 10）だけです。受理から拒否に変わったものはありません。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r5/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r5/step5-final.txt`。`in_place_writes` の fixture に足した 5 件も基準と同じく受理されます）。
- E2E（`r5/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 47 件と native だけの 2 件（基準のバイナリでも同じ値で成功）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs` 701 件、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`）は `Docs: 2 pages, 37 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`（どちらも `TSUZURI_BIN` に対応後のバイナリ）、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check --edition 2027` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。`r5/time-perf.txt`）: 指摘 1 の `perf_holders3_240` 3.9 / 444 / 26.2、`perf_holders3_480` 15.4 / 3735 / 108、この対応で作った 960 段（`r5/p960`）64.6 / 35058 / 476、呼び出しの連鎖も持つ `perf_holders2_120` 3.6 / 59.8 / 10.9、`perf_holders2_240` 13.8 / 452 / 41.2、`perf_holders2_480` 60.4 / 3817 / 173、閉包の型が 1 つの対照 `perf_wrappers_480` 53.2 / 69.4 / 68.1。段の数を 2 倍にすると対応後は 4 倍ほどで、基準と同じ 2 乗の伸びです（基準の 3〜8 倍。標本では、残りは本文の検査（`check_functions` と、合同の不動点の `check_facts`）です）。第 4 回までの表の `perf_locals_600` 357 / 803 / 801、`chain_1600` 13.5 / 91.8 / 91.8、`ring_400` 2.1 / 10.9 / 11.0、`perf_chain_400` 2.9 / 21.3 / 21.6、`p01_known_tree_24` 0.34 / 1.35 / 1.40、`p03_hof_chain_40` 0.50 / 1.43 / 1.44、`perf_twice_12` 0.17 / 0.60 / 0.60、`p08_builder_pipe_22` 0.18 / 0.48 / 0.51、`perf_rev_chain_2000` 1.5 / 8.1 / 8.1 と、第 5 回の `perf_drops_600` 6.7 / 33.6 / 33.7、`perf_hdrops_400` 26.1 / 62.4 / 62.3 は、対応の前後で変わりません。機械の負荷は測定の間に 30 ほどから 5 ほどへ下がりました。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 出どころを追える値に `match` のパターンで取り出した値、`Rc.try_unwrap`・`Rc.upgrade` の結果は `match` で取り出すこと、そうしたハンドラーも 2 つめの規則では元の `Rc` に借用を残しうること、追えない所の一覧からパターンを外した）、`docs/architecture.md`（holder の不動点の作業リスト、パターンの局所値の出どころ、本文の式だけを覚える走査）、`docs/language.md`（出どころの規則）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第6回）（2026-11-12）

レビューの 5 件の指摘（反例の探索 `target/scratch/review-a31-ce-r6/`、適合の確認 `target/scratch/review-a31-conf-r6/`）を、対応前の先端（`a3695d79`、`r6/bin/tsuzuri-before`）・基準・対応後のバイナリで確かめました。5 件とも指摘のとおりに再現しました（指摘 4 は基準でも同じく受理される、A31 より前からの穴です）。結果は `target/scratch/a31/r6/` にあります（この対応で書いたプログラムは `r6/f1`〜`r6/f5`、`r6/c.sh` が検査の最初の診断を、`r6/corpora.sh <tag>` がレビューの 6 回分のプログラムの `check --json` を作ります）。

設計の変更（`src/ownership.rs`、`src/ownership_control.rs`（ビューの記録）、`src/check.rs`（`Type::drops_some`。使われなくなった `Type::stores_some` は消しました））:

- **局所の `Rc`・`Arc` の所有者のブロックの共有を、呼び出し元の書き込み先か隠れた環境へ格納すると、ブロックはその後に本体がセルへ書き込むものも受け取ります**（`export_block`・`expand_shares`・`store_exported`、`Checker::exports`、`ExportTarget`）。今までは、共有（`Loan::share`）をその時点の所有者の loan に読み替えるだけだったので、後で所有者などを通してセルへ書き込んだ借用は局所の所有者に残るだけで、呼び出し元のデータへ届きませんでした。読み替えるときに、セルを持つブロックの所有者の型（弱参照は強い所有者の型に直します）を記録し、以後の局所値への格納（`apply_store_within`・`store_hidden_except`・`store_through` の局所の枝）で、その型に届く局所値へ格納した loan を同じ先へも格納します。所有者のムーブ、別の共有、`Rc.get` で得た中身の参照、`Rc.upgrade` で戻した値を通した書き込みも届くよう、局所値の根ではなく型で判定します。指摘 1。
- **隠れた環境へ格納する共有も、所有者の loan に読み替えます**（`store_hidden_except` の初めの `expand_shares`）。今までは局所値の枝が共有の loan（元の `Rc` の借用）をそのまま吸収していたので、レジストリが元の `Rc` の借用を持ち、`Rc` を内側のブロックで宣言すると `E1013` でした。仮引数の事実の枝は前から読み替えていたので、その重複した処理は消しました。**A31 の破棄のフック（`runs_drop_code`・`drop_hidden`・`drop_borrows_self`）は、弱参照の先の値の `Drop.drop` を数えません**（`Type::drops_some`、`Checker::drop_code`。弱参照の破棄はブロックの値を破棄しないため。A28 の `dropped` は今までどおり `drop_runs_code` です）。指摘 5。
- **型注釈やパターンを書いたラムダの仮引数、仮引数を `match` で取り出した値、仮引数のフィールドの呼び出しを、仮引数の呼び出しとして記録します**（`parameter_path`、`HigherOrderCall::path`、`Checker::projections`、`apply_part`、`unknown_part`、`record_higher`）。`\(use: T) -> ...` は構文解析で `\$lambdaN -> match $lambdaN with use -> ...` になるので、`use` はパターンが束縛する局所値で、仮引数でも `known` でもなく、見えない関数値の規則で定義の位置の `E1013` でした。`known_functions` が `match` の対象とパターンの局所値の射影（`PatternAlternative::bindings`・`PatternStep::Bind`）を集め、`parameter_path` が局所値の射影・フィールド・共用体の中身（`PAYLOAD`。OR パターンの選択肢は同じ道のときだけ）・共有参照の `deref` をたどって仮引数と道を求めます。呼び出し元は、実引数のタプル・レコードのリテラルの成分、共用体の値の中身、`known_parts`、不変な局所値の束縛した値（`bound`）をたどってその関数値の事実を当てはめ、分からなければ実引数全体を（式を持たない）見えない関数値として当てはめます。実引数が呼び出し元自身の仮引数なら、道をつないで呼び出し元の事実に記録します。道は 6 段（`HIGHER_ORDER_PATH`）までで、それより長くなる呼び出し（再帰的な値の一部を自分へ渡す再帰。`tests/fixtures/generic_records` の `chain_total rest text` は、段ごとに道を伸ばして事実が収束せず、予算の打ち切りの `top` から `E1013` になっていました）は見えない関数値の呼び出しとして扱います。指摘 2。
- **パターンがその場で見る変数（`TypedMatchArm::views`）の借用は、呼び出し元の書き込み先・隠れた環境・高階の事実の出どころでは、ビューの loan（照合した場所の借用）に読み替えます**（`Checker::views`、`expand_views`）。コード生成はビューの変数を照合した場所のアドレスに束縛するので（`llvm_control.rs`）、`ref s` は照合した場所を指します。共有参照の仮引数（2027 の読み取りの仮引数を含む）の先を照合したなら、その loan は仮引数の根なので事実（`ref r.name` と同じ `Part::Value`）になり、局所値や一時値を照合したなら局所値の根のままで `E1013` です。書き込める仮引数（型が排他参照に届く仮引数・捕捉、`writable_external`）の先のビューは読み替えません: 本文が後で照合した値を置き換えうる（`deref m = Maybe.None`）ので、格納した借用が別のケースの中身を指すためです（この対応で書いた `f3/m02` は、読み替えると `-1` を読みました）。局所値への格納は今までどおり、ビューの規則（`borrow_outlives_binding`）で検査します。指摘 3。
- **書き込める仮引数の先の借用を呼び出し元のデータへ格納すると、仮引数がその loan を持ちます**（`store_external`）。今までは格納した借用はどの局所値にも残らないので、本文がその後で同じ仮引数を通して書き込めました（`f3/m03`: `ref v[0]` を呼び出し元の配列へ書き込んだ後の `Vec.push_mut` が要素を動かし、`-O0` で `0`（期待は `9`））。基準と対応前も受理していた、この対応で見つけた穴です。仮引数が loan を持つので、その後の排他の使用は、局所値に残した借用と同じく `E1014` です。指摘 3 の確かめで見つけたもの。
- **ブロックの終わりで、利用者の `Drop.drop` を実行しうる値が、同じブロックで後に宣言した局所値の借用を持つと `E1013` です**（`check_drop_order`、`drop_reads`、`drop_environments`、`DropEnvironments`、`CallFacts::drop_environments`）。局所値は宣言の逆順で破棄されるので、その `Drop.drop` は先に解放された値を読みえます。値の型が（参照と弱参照を通さずに）利用者の `Drop` 型を格納するか、環境に利用者の `Drop` 型の値を持ちうる関数値を格納するときに数えます。関数値は、そうした値を束縛する所（ラムダの捕捉、部分適用の引数、引数の多い呼び出しの結果）の関数の型を、holder の閉包（`HolderReach`）で閉じたものだけを数え、dyn 値・`Owned.Function`・tzroutine の本体がそうした値（かそうした関数値・dyn 値・`Owned.Function`）を持ちうるモジュールでは、すべての関数値を数えます（`DropEnvironments::others`）。利用者の `Drop` を持たないモジュールでは何もしません。そうした値の loan は、本文が後で使わなくてもスコープの終わりまで生きます（`keep_for_drop`・`dropped_later`。今までは使い終わった局所値の loan を `retain_live_loans` が消していたので、破棄が使う借用が検査から漏れていました）。使われないそうした値が持つ共有は、元の所有者の借用ではなく所有者の loan に読み替え、以後のブロックへの格納は `ExportTarget::Local` で届けます（所有者のムーブを `E1014` にしないため）。エラーは既存の kind `assignment_extends_borrow` で、文言は `'holder' keeps a borrow of a value that is dropped before it, and dropping 'holder' may run a Drop.drop that uses the borrow` です。指摘 4。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（局所のブロックの共有を呼び出し元へ格納した後のセルへの書き込みが、呼び出し元に届かない。危険な形を受理） | 再現（`sh01`・`sh03`・`sh09` が受理、`-O0` で不定の値、`sh11`・`sh20`・`sh22` が受理、`0`（期待は `9`））。この対応で書いた `f1/v02`（所有者のムーブ）・`v03`（先に作った共有）・`v07`（`Rc.get` の参照）・`v08`（弱参照）・`v09`（レコードのフィールド）も受理 | ブロックの型の記録。`sh01`・`sh03`・`sh09`・`v*` は定義の位置、`sh11`・`sh20`・`sh22` は呼び出しの位置の `E1013`（両方の edition）。対照 `sh21`（9）、`v05`（所有の値だけを書く、7）、`v06`（共有だけ、7）は受理 | `tests/rc.rs` `a_share_of_a_local_block_takes_what_the_body_writes_into_it_later` |
| 2（型注釈・パターンを書いたラムダの仮引数を呼ぶ本体が、定義の位置で `E1013`。基準からの退行） | 再現（`lk01`・`lk06` が両方の edition で拒否、基準は受理 1）。同じ原因で、仮引数を `match` で取り出す名前付き関数（`f2/n05`・`n12`・`n17`・`n18`・`n21`）と仮引数のフィールドの呼び出し（`n07`）、第 3 回の `r3/adv/l02_literal_with_sink` も拒否 | 仮引数の道。すべて受理（値は基準と同じ）。危険な呼び出し元（`n02`・`n04`・`n06`・`n08`・`n09`・`n11`・`n13`・`n19`・`n20`、第 2 回の `g02`・`g05`、第 3 回の `d01`、第 5 回の `dg02`・`dg04`、第 6 回の `g02`・`g03`）は呼び出しの位置の `E1013` | `a_parameter_that_a_pattern_binds_is_called_as_the_parameter` |
| 3（参照の仮引数の先を照合したビューの借用を格納すると、定義の位置で「局所値の借用」の `E1013`。基準からの退行） | 再現（`rg01`〜`rg03`・`rg05`・`rg07`・`rg09`・`rg10`・`rg12`、2027 の `rg08` が拒否、基準は受理 9・10） | ビューの読み替え。すべて受理（`rg08` は 2026 では値の仮引数のビューなので拒否のまま）。危険な呼び出し元 `th09`・`th10` は呼び出しの位置、一時値・局所値・`own` の仮引数・`ref mut` の仮引数のビュー（`f3/v01`〜`v03`・`m02`）は定義の位置の `E1013`。この確かめで見つけた `m03` は `E1014` | `a_borrow_of_a_view_of_the_callers_data_is_a_borrow_of_that_data`（`m02`・`m03` も） |
| 4（`Drop` を持つ値を、借用元より先に宣言した持ち主に入れると、借用元の解放の後で `drop` が読む。基準も受理） | 再現（`do01`（2027）・`do02`・`do03`・`do04`・`do06`・`do07`・`dc01` が受理、`drop 0`（期待は `drop 9`）） | 破棄の順序の検査。すべてブロックの位置の `E1013`。この対応で書いた `f4/k04`（`own` の仮引数を受けた持ち主）・`k10`（局所の `Mutex`）・`k11`（`Drop` を持つ値が共有するブロックへ後で書き込む）も拒否。対照 `do03_ctl`・`dc02_ctl`、`k01`（持ち主が後）・`k02`（`Drop` を持たない関数値の配列）・`k06`（持ち主をムーブ）・`k07`（仮引数の借用）・`k09`（`Drop` の無いモジュール）・`k12`（共有を持つ `Drop` 値の後で元をムーブ）・`k13` は受理 | `a_value_whose_drop_runs_code_keeps_no_borrow_of_a_later_local` |
| 5（`Rc.share`・`Rc.downgrade`・`Arc.share` の結果の破棄が、レジストリへ元の `Rc` の借用を残す。`fb3c0611` と基準からの退行） | 再現（`s03`・`s08`・`s10`・`s11`・`s12`・`s13` が `E1013`、基準と `fb3c0611` は受理 9・7） | 共有の読み替えと弱参照の除外。すべて受理（9・7）。`sd02_ctl` も受理になり、借用元を内側で宣言する危険な形（`sd01`、`f5/u01`〜`u04`）は拒否のまま | `a_dropped_share_or_weak_pointer_keeps_no_borrow_of_its_owner` |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 4 は A31 より前からの穴（代入でも同じ）ですが、危険な受理は範囲外にしない方針（DECISIONS.md の追補）に従って直しました。規則は Rust の drop check と同じく保守的で、`Drop.drop` が借用を実際に読むかは見ません（借用を読まない `drop` を持つ値でも、後に宣言した値の借用を持てば `E1013`）。そうした値の借用はスコープの終わりまで生きるので、使い終わった `Drop` 値が借用している値を後でムーブ・変更する形は `E1014` になりえます（`drop` が読めば危険な形）。レビューの 6 回分のプログラムと `tests/fixtures/*`・`examples/*` では、受理から拒否に変わった安全な形はありません（下の確かめたこと）。`drop.md` の「解放の順序」と `docs/language.md` に書きました。
- 指摘 1 の判定はブロックの型で行うので、共有を呼び出し元へ渡した本体が、同じ型のブロックを持つ別の局所値へ局所値の借用を書き込むと、その局所値が呼び出し元と無関係でも `E1013` です（保守的）。所有者のムーブや弱参照を通した書き込みを、根をたどらずに確実に届けるためです。`lifetimes.md` に書きました。
- 指摘 2 の道は、タプル・レコードのフィールドと共用体の中身（ケースを区別しない、場所の `PAYLOAD` と同じ）だけです。配列の要素などほかの射影で取り出した関数値の呼び出しは、今までどおり見えない関数値の規則です。
- 指摘 3 の読み替えは、呼び出し元の書き込み先・隠れた環境・高階の事実の出どころだけです。ビューの借用を返す形は今までどおり `borrow_outlives_binding` です（`rg20`）。
- 書き込める仮引数の先の借用を格納した後の、その仮引数を通した書き込みは、どれも `E1014` です。フィールドの置き換え（`r.name = to_string 5`。`ref r.name` はフィールドの場所を指すので、置き換えた後も新しい文字列を読み、メモリーは安全な `f3/m01`）も、局所値に残した借用と同じ規則で拒否します（基準と対応前は受理）。要素を動かす書き込み（`m03`）と区別するには、書き込みの種類ごとの規則が要り、局所値の借用の規則とも食い違うので、局所値と同じ規則を選びました。`lifetimes.md` に書きました。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-12。ログは `target/scratch/a31/r6/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（`r6/clippy.log`）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は、指摘 1〜5 の対応の後（`w3`）で 3030 件中 2 件が失敗しました（`effects_trap`・`effects_alloc_conservative` が `tests/fixtures/generic_records` を検査できない。上の道の長さの上限で直しました。`r6/nextest-1.log`）。上限と、ビューの制限、書き込める仮引数の借用の対応の後（`w5`）は 3030 件すべて成功、4 件 skip（1936 秒、`r6/nextest-2.log`）。最後のコード（同じ判定のまま、破棄の順序の走査を局所値ごとの答えの記憶と位置の表にしたもの）では 3030 件中 3029 件が成功、4 件 skip で、失敗した 1 件は所有権と関係しない子プロセスの時間切れのテスト `driver::test_runner::tests::runner_rejects_malformed_indices_and_reaps_timed_out_children`（1 秒の上限。共有の機械が混んでいた）で、単独で 3 回実行してすべて成功しました（1609 秒、`r6/nextest-3.log`）。第 5 回の 3025 件から増えた 5 件は、`tests/rc.rs` の新しいテストです。
- 新しいテストが対応前に失敗すること: `src/ownership.rs`・`src/ownership_control.rs`・`src/check.rs` を `a3695d79` に戻して `cargo test --test rc` を実行し、新しい 5 件がすべて失敗することを確かめました（戻した後は先端に戻しています）。
- レビューの 6 回分のプログラム（`r6/corpora.sh before|w5`、両方の edition）: 第 1 回の 1202 件と第 4 回の 394 件は変化なし、第 2 回は `g02`・`g05`（定義の位置から呼び出しの位置へ）、第 3 回は `r3/adv/l02_literal_with_sink`（拒否から受理。値は基準と同じ 14）と `d01`（位置）、第 5 回は `dg02`・`dg04`（位置）だけです。第 6 回と `r6/f1`〜`f5` の 668 件では、受理から拒否が 43 件（すべて上の表の危険な形）、拒否から受理が 53 件（すべて安全な形。値は基準と同じ）、拒否のまま位置か種類が変わったものが 32 件（危険な呼び出し元の位置へ移ったもの）です。
- 受理になった安全な形と対照の値は、`-O0`・`-O3` とも手の計算どおりで基準と同じです（`sh21` 9、`v05`・`v06` 7、`lk01`・`lk06` 1、`n05`・`n07`・`n10` 2、`n12`・`n17` 3、`n18`・`n21` 4、`n15` 9、`rg01`〜`rg12`・`f3/v05`・`v07` 9（`rg02` は 10）、`s03`・`s10`・`s11` 9、`s12` 9 + 2、`s08`・`s13` 7、`sd02_ctl` 9、`l02_literal_with_sink` 14、`do03_ctl`・`dc02_ctl`・`k01` は `drop 0`・`drop 9`・`0`。`r6/vals.txt`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r6/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r6/step5-final.txt`。`in_place_writes` の fixture に足した 5 件も基準と同じく受理されます）。
- E2E（`r6/e2e-final.log`、`w5`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 52 件（基準のバイナリでも 52 件同じ値で成功）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。最後のコードでも `in_place_writes` 52 件・`rc` 39 件・`mutable_slices` 49 件・`user_drop.mjs` を実行し、すべて成功しました（`r6/e2e-final2.log`）。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`・`rc.md`・`pattern-matching.md`）は `Docs: 4 pages, 69 links, 26 checked examples, 52 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`（どちらも `TSUZURI_BIN` に対応後のバイナリ）、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（破棄の順序の文言は既存の `assignment_extends_borrow` です）。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check --edition 2027` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。機械の負荷が高く、同じプログラムでも回によって 3 割ほど違います。`r6/time-perf.txt`）: この対応で作った `r6/perf/drop_locals_600`（利用者の `Drop` を持つ 600 個の局所値）7.3 / 10.1 / 12.2、`r6/perf/pattern_param_chain_200`（タプルの仮引数を `match` して次へ渡す 200 段）4.1 / 9.5 / 13.3、第 6 回の `perf_match_chain_400` 249 / 211 / 203、`perf_match_chain_1600` 3171 / 3683 / 3612、`perf_active_chain_200` 43.5 / 53.6 / 51.3、第 5 回までの `perf_locals_600` 708 / 1769 / 1688、`chain_1600` 37 / 232 / 223、`ring_400` 6.0 / 22.7 / 21.6、`perf_holders3_480` 26 / 216 / 222、`p08_builder_pipe_22` 0.35 / 0.98 / 1.16、`perf_rev_chain_2000` 3.7 / 17.8 / 18.6。利用者の `Drop` を持つ値の多い `perf_drops_600` 14 / 76 / 95、`perf_hdrops_400` 51 / 136 / 179 は、そうした値の借用をスコープの終わりまで生かす分（局所値ごとの走査を局所値ごとの答えの記憶と位置の表にした後の値）です。`perf_nested_match_200` は構文の入れ子の上限（128）を超えるので、どのバイナリも所有権の検査に届きません。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限: 引数のフィールドの関数値の呼び出し、型注釈・パターンを書いたラムダの仮引数と `match` で取り出した仮引数、パターンが照合した値を見る変数の借用、書き込める仮引数の先の借用、関数の中の共有を呼び出し元へ書き込んだ後のセルへの書き込みと例）、`drop.md`（解放の順序と借用、例。共有と弱参照の破棄）、`built-in-types-and-modules/rc.md`（所有者を増やした値の借用）、`pattern-matching/pattern-matching.md`（借用先からの分解）、`docs/language.md`（書き込みの事実・値の破棄・`Rc.share` の項目）、`docs/architecture.md`（書き込みの事実の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`src/check.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第7回）（2026-11-12）

レビューの 9 件の指摘（反例の探索 `target/scratch/review-a31-ce-r7/`、適合の確認 `target/scratch/review-a31-conf-r7/`）を、対応前の先端（`8aaa360b`、`r7/bin/tsuzuri-before`）・基準・対応後のバイナリで確かめました。指摘 1 と 6 は同じ穴（別のレビュー役が別々に見つけたもの）です。9 件とも指摘のとおりに再現しました（指摘 1・2・6・7・8 は基準でも同じく受理される、A31 より前からの穴か、第 6 回の対応が閉じきれなかった穴です）。結果は `target/scratch/a31/r7/` にあります（この対応で書いたプログラムは `r7/adv`・`r7/f4`、`r7/corpora.sh <tag>` がレビューの 7 回分のプログラムの `check --json` を、`r7/perf.sh` が性能の表を作ります）。

設計の変更（`src/ownership.rs`、`src/ownership_control.rs`、`src/check.rs`（`Type::stores_some` を戻しました））:

- **すべての経路でムーブした値だけを、破棄の順序の検査と借用の延長から除きます**（`State::moved_always`、`moved_always`）。今までは `State::moved` を見ていましたが、`moved` は合流（`merge`）で和をとるので、片方の枝でだけムーブした値も入っていました。ムーブしなかった経路では値はスコープの終わりで解放されるので、`check_drop_order` も `keep_for_drop`（`dropped_later`）も、その値を見落としていました。`moved_always` はムーブで局所値を入れ、束縛・代入・スコープの終わり・ループの本体の終わりで外し、合流では積をとります（念のため、使う所で `moved` の全体の場所との積もとります）。指摘 1・6。
- **`break`・`continue` と、`try` が受け止める例外の位置でも、破棄の順序を検査します**（`check_exit_drop_order`、`note_binding`、`Checker::binding_order`）。脱出は抜けるスコープの局所値をその場で解放するので、ブロックの終わりの状態だけを見ると、脱出の後で借用を持たない値を代入した形（`do27`・`do28`）を見落としました。抜けるスコープの局所値はスタックをなすので、束縛した順（束縛のたびに記録する番号）が宣言の順です。`eval_loop_jump` と `raise_edge` が、`finish_control_scope` の前に、抜ける局所値をその順に並べて `check_exit_drop_order` を呼びます（`check_drop_order` と同じ判定 `check_drop_order_of`）。指摘 2。
- **借用を持ちうる利用者の `Drop` 型だけを、借用を読みうる破棄に数えます**（`drop_part_holds_loans`）。`Drop.drop` は自分の値しか受け取らないので、参照・関数値・dyn 値・tzroutine の本体・`Owned.Function`・未確定の型を、フィールド・要素・セル・強い参照と弱参照の先のどこにも格納しない `Drop` 型（`record Guard { id: i64 }`）の破棄は、隣のフィールドの借用（`label: ref string`）を読めません。Rust の drop check が `Drop` を持たない構造体の参照のフィールドに寿命を求めないのと同じです。関数値・dyn 値などの環境の規則（`drop_environments`）は変えていません。指摘 3。
- **書き込める仮引数の先の借用を呼び出し元のデータへ書き込んだ後は、その借用を、仮引数の値ではなく本文の残りの間生きる loan として持ちます**（`State::kept`）。第 6 回は仮引数の局所値に loan を足したので、仮引数が `r.name` も指す参照に見え、`r.count = 5` が「`let mut` か排他参照が要る」という誤った文言の `E1014` でした。`kept` は `active`・`retain_live_loans`・ループの要約が数え、合流では和をとるので、借用した場所に届く書き込み（`Vec.push_mut`、仮引数のムーブ）だけが、局所値に残した借用と同じく `E1014` です。`conflict` は `kept` の loan に、関数が借用を呼び出し元のデータへ書き込んだことを述べる help を付けます。さらに、`ref mut` の仮引数の先の共有の借用だけを書き込む事実の組を `StoreFacts::shared_only` に記録し、呼び出し元が新しく作った `ref mut` の借用（`put (ref mut v0) (ref mut arr)`）を渡すときは、その排他の loan の代わりに同じ場所・同じ親の共有の loan を書き込み先へ残します（`shared_copy`）。排他の借用は呼び出しで終わるからです。これで呼び出し元は、書き込み先が生きている間も `v0` を読めます（`wp01` は本文を直しても、呼び出し元の `v0.count` の読み出しが `E1014` でした。これは第 1 回から、事実が実引数の排他の loan をそのまま書き込み先へ残していたためで、`wq01`（本文が書き込まない形）も同じでした）。指摘 4。
- **書き込める仮引数の先の借用を関数値の仮引数へ渡した後で、その仮引数を通して借用した場所に届く書き込みをする本文は、呼び出し元に、受け取った値を残しうる関数値を渡させません**（`Checker::handed`、`overwrite_handed`、`Source::overwritten`、`Arg::overwritten`）。`record_higher` が、書き込める仮引数の先の `Part::Value` の借用を持つ実引数を、場所と記録した呼び出しと引数の位置で覚え、その後の場所が重なる書き込み・排他借用（`access`）と参照のムーブ（`read_places`）が、その引数を `overwritten` にした呼び出しを事実に記録し直します。呼び出し元では、`overwritten` の出どころは局所値の借用（`local`）と同じく扱い、受け取った値を残しうる関数値なら `E1013`（`cannot pass this function value here: 'put' passes it a borrow of what one of its own 'ref mut' parameters points at and writes there afterwards, ...`）、受け取った関数を呼ぶだけの関数値なら受理です（無条件に借用を生かすと、`\g -> g ()` を渡す安全な形を基準より厳しく拒否するため）。ループの要約は `handed` の大きさも数えるので、次の反復の書き込みも届きます。隠れた環境へ書き込む形（`store_hidden_except` の仮引数の事実の枝）は、呼び出し元のデータへの書き込みと同じく `kept` に足します。指摘 8。
- **局所の所有者をムーブして渡した呼び出しの格納を、ブロックの共有が行った先へ届けます**（`store_exported_into`）。第 6 回の `store_exported` は局所の根への格納だけで動くので、所有者を値で受け取る関数の事実（`apply_facts_once`）や本体の見えないコード（`apply_opaque`）がブロックのセルへ書き込むと、ムーブした所有者は何も持たず、格納はどこにも届きませんでした。実引数の式の型がブロックの所有者の型に届くなら、その共有が行った先（呼び出し元のデータ、隠れた環境、破棄でブロックを読む局所値）へも格納します。指摘 7。
- **`parameter_path` の答えを局所値ごとに覚えます**（`parameter_path_at`、`Checker::parameter_paths`）。OR パターンの選択肢が同じ所から取り出すとき、今までは選択肢ごとに下の局所値をたどり直したので、入れ子の深さに対して指数的でした。答えと、その答えが歩いた深さ（高さ）を一緒に覚え、上限で切られなかった答え（深さと高さの和が上限以下）だけを残します。そうした答えは歩く余地のあるどの深さからでも同じで、余地の無い深さからは切られて `None` になる（どの切り口も `None` を上へ伝える）ので、覚えた答えと高さから、覚えないときと同じ答えが出ます。指摘 9。
- **関数値の仮引数の共有の借用（`E::Borrow(_, false)`）も、仮引数の呼び出しとしてたどります**（`parameter_path_at`）。edition 2027 で関数値の仮引数を読み取りの仮引数へ貸す `fn relay v use = put v use` は、今まで本体の見えない関数値の呼び出しとして `relay` の中で `E1013` でした（局所値の借用を渡す形でも、第 6 回の前から）。指摘 8 の規則が、受け取った関数を呼ぶだけの呼び出し元をこの形で拒否しないよう、呼び出し元で検査します。この対応で見つけたもの。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1・6（片方の経路でだけムーブした `Drop` の値を、破棄の順序の検査と借用の延長が見落とす。危険な形を受理） | 再現（`do01`・`do02`・`do03`・`do21`・`do23`・`do35`、`m01`・`m02` が両方の edition で受理、`drop 0`（期待は `drop 9`）。`do12` は受理、`drop 0`・`drop 0`・`9`） | `moved_always`。どれもブロックの位置の `E1013`、`do12` は `E1014`。この対応で書いた `x07`（すべての経路でムーブした後、片方の経路で借用を持つ値を入れる）も `E1013`。対照 `do06`・`m01_ctl`・`do13`、`x05`（両方の経路でムーブ）・`x06`（片方でムーブした後、借用の無い値を入れる）は受理 | `tests/rc.rs` `a_value_that_a_branch_moves_or_a_jump_drops_keeps_no_borrow_of_a_later_local` |
| 2（`break`・`continue` の後で借用の無い値を代入すると、脱出の位置の破棄の順序を検査しない） | 再現（`do27`・`do28` が受理、`drop 0`） | `check_exit_drop_order`。`do27`・`do28` と、この対応で書いた `x02`（内側のブロックからの `break`）・`x04`（`for` の `break`）・`x08`（`try` が受け止める例外）は脱出の位置の `E1013`。`do24`・`do25`・`do26`・`do32` は拒否のまま、位置がブロックから `break` などへ移りました。対照 `do29`、`x03`（借用される値を先に宣言）は受理 | 同上 |
| 3（借用を持てない `Drop` 型の隣の借用も、スコープの終わりまで生かす。基準からの退行） | 再現（`rd01`・`rd03` が `E1014`、`rd04` が `E1013`、基準は受理 `bye 1`/`10` など） | `drop_part_holds_loans`。3 件とも受理、値は基準と同じ。弱参照の先にセルを持つ `Drop` 型（`z01`）は数えたまま `E1013` | `a_drop_that_cannot_hold_a_borrow_reads_none_of_the_borrows_beside_it` |
| 4（借用を呼び出し元へ書き込んだ後の、別のフィールドへの書き込みが誤った文言の `E1014`。基準からの退行） | 再現（`wp01` が 2027 で `mutate_immutable_value`）。本文を直すと呼び出し元の読み出しが `E1014` になることも確かめた（`wq01`・`wq03`〜`wq05`） | `kept` と `shared_only`。`wp01` は受理 `14`、`wq01` 受理 `12`。呼び出し元の置き換え（`wq06`）、本文の仮引数のムーブ（`y05`）、可変スライス全体の排他借用（`wp03`）は `E1014` のまま | `a_borrow_stored_from_a_mutable_parameter_keeps_only_what_it_borrows` |
| 5（`Drop` の値が多いと検査の時間が基準の約 17 倍。どちらも 2 乗） | 再現（下の性能） | 時間の閾値は無く記録のためという指摘なので、判定は変えていません。そうした値の借用をスコープの終わりまで生かすため、文ごとにそうした値をすべて見る分の定数で、2 乗は基準と同じです | なし |
| 7（局所の所有者をムーブして渡した関数の書き込みが、共有を書き込んだ呼び出し元にも、共有を持つ `Drop` の値にも届かない。危険な形を受理） | 再現（`e06` が受理、`-O0` で不定の値、`e01` が受理 `drop 0`） | `store_exported_into`。`e06` と、この対応で書いた `e07`（本体の見えない関数値へムーブ）・`e08`（関数値の仮引数へ渡す）は定義の位置の `E1013`、`e01` はブロックの位置の `E1013`。対照 `e06_ctl`（9）・`e01_ctl`（`drop 9`）は受理 | `a_store_into_an_owner_moved_into_a_callee_reaches_where_its_block_went` |
| 8（書き込める仮引数の先の借用を関数値の仮引数へ渡した後の `Vec.push_mut`。危険な形を受理） | 再現（`h01`・`h02`・`h03` が受理、`0`（期待は `9`）） | `handed`・`overwritten`。3 件と、この対応で書いた `y02`（仮引数を押し込む関数へ渡す）・`y04`（仮引数を別の関数へそのまま渡す）は呼び出しの位置の `E1013`。対照 `h01_ctl`・`h03_ctl`（9）、`y01`・`y04c`（受け取った関数を呼ぶだけ、9）は受理 | `a_borrow_handed_to_a_function_parameter_and_written_over_cannot_be_kept` |
| 9（`parameter_path` が入れ子の OR パターンの深さに対して指数的） | 再現（下の性能） | 局所値ごとの答えと高さの記憶 | 単体テスト `the_parameter_path_looks_at_each_local_once`（12 ケース 3 段で 260 歩以下。記憶を外すと 5656 歩で失敗） |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 1・6 は「すべての経路でムーブした」集合を新しく持つ方を選びました（指摘の案の 1 つ）。破棄のフラグを見る案と違い、どの経路でも値が残らないときだけ除きます。
- 指摘 2 の判定は、ブロックの終わりの判定と同じ規則を、脱出が抜ける局所値に当てはめるものです。そのため、同じ形を両方で拒否する `do24` などは、先に評価する脱出の位置で報告します（位置が変わっただけで、どれも拒否のままです）。
- 指摘 3 は型で判定するので、`Drop` 型が関数値を格納すれば（`Caller { f: unit -> i64 }`）、その値が実際には借用を持たなくても数えます。関数の型は環境を表さないためです。弱参照の先にセルを持つ `Drop` 型（`z01`）も、`Drop.drop` が弱参照を戻してセルの関数値を呼べるので数えます。
- 指摘 4 の `shared_only` は、呼び出し元が新しく作った `ref mut` の借用を渡すときだけです。`ref mut` の変数や仮引数をそのまま渡すときは、その借用が呼び出しの後も続くので、今までどおり排他の loan を残します。残す共有の loan は値全体（`v0`）の借用なので、書き込み先が生きている間の `v0.count = 1` は、`ref v0.name` を局所値に残した形と違い `E1014` です（安全側）。可変スライス・ベクタへの書き込み（`Array.write 1 x v`）は全体を排他借用するので、`ref v[0]` を書き込んだ後は別の要素への書き込みでも `E1014` です（局所値に残した借用と同じ規則。`wp03`）。
- 指摘 7 の格納は型で判定するので、実引数の型が共有したブロックの所有者の型に届けば、呼び出し先がその部分へ書き込まなくても届けます（第 6 回の局所値の判定と同じ近似）。関数値の仮引数へ所有者と局所値の借用を渡す本文（`e08`）は、呼び出し元の関数値が書き込むかに関わらず定義の位置の `E1013` です（関数値が何も書き込まなければ安全な形も拒否します）。高階の事実にブロックの行き先を持たせれば呼び出し元で判定できますが、この形はブロックの共有を呼び出し元へ渡した本文に限られるので、安全側にしました。
- 指摘 8 は、借用を渡した後の書き込みを事実に記録し、呼び出し元が関数値で判定する方を選びました。本文の中で借用を生かす（無条件に `E1014` にする）方が簡単ですが、受け取った関数を呼ぶだけの関数値を渡す安全な形（`y01`）を、基準と違って拒否するためです。記録は本文の評価の順で書き込みが後に来るときだけで、別の枝の書き込みでも記録します（安全側）。
- 指摘 5 は判定を変えていません。指摘が閾値を求めず、基準も同じ 2 乗であるためです。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-12。ログは `target/scratch/a31/r7/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3036 件すべて成功、4 件 skip（1719 秒、`r7/nextest-1.log`）。第 6 回の 3030 件から増えた 6 件は、`tests/rc.rs` の新しい 5 件と単体テスト `the_parameter_path_looks_at_each_local_once` です。
- 新しいテストが対応前に失敗すること: `src/ownership.rs`・`src/ownership_control.rs`・`src/check.rs` を `8aaa360b` に戻して `cargo test --test rc` を実行し、新しい 5 件がすべて失敗することを確かめました（戻した後は先端に戻しています）。単体テストは、答えの記憶を外すと 5656 歩で失敗します（記憶ありは上限 260 歩以内）。
- レビューの 7 回分のプログラム（`r7/corpora.sh before|c`、両方の edition）: 第 1 回の 1202 件、第 2 回の 338 件、第 4 回の 394 件、第 5 回の 382 件、第 6 回の 668 件は変化なし。第 3 回は `c08_sort_with_cmp_store`・`g19_mapin_own_closures`（危険な形。std の中の位置の `E1013` のまま、kind が `store_local_borrow_through_reference` から `task_result_retains_borrow` に変わりました: `sort_with` が比較関数の仮引数の共有の借用を渡す呼び出しを、上の `E::Borrow` の規則で仮引数の呼び出しとして記録するようになったため）だけです。第 7 回と `r7/adv`・`r7/f4` の 334 件では、受理から拒否が 43 件（すべて上の表の危険な形。基準で実行すると `drop 0`・`0`・不定の値）、拒否から受理が 14 件（`rd01`・`rd03`・`rd04`・`wp01`・`wq01`・`wq03`〜`wq05`。すべて安全な形で、値は基準と同じ）、拒否のまま位置が変わったものが 9 件（`do24`・`do25`・`do26`・`do32` の脱出の位置と、`y04b` の呼び出し元の位置）です。
- 受理されるレビューの第 7 回のプログラムと `r7/adv`・`r7/f4` の 65 件（両方の edition）は、`-O0`・`-O3` の値がすべて基準と同じです（`r7/run-c.json`・`run-base.json`。`wp01` 14、`wq01` 12、`rd01`・`rd03` `bye 1`/`10`、`rd04` `bye 1`/`bye 2`/`9`、`y01`・`y04c`・`h01_ctl`・`h03_ctl`・`e06_ctl` 9、`e01_ctl` `drop 9`/`0`、`x05` `drop 0`/`drop 9`/`9`、`x06` `drop 0`/`drop 9`/`drop 3`/`9` など）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r7/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r7/step5-final.txt`。`in_place_writes` の fixture に足した 4 件も基準と同じく受理されます）。
- E2E（`r7/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 56 件（基準のバイナリでも 56 件同じ値で成功。第 6 回の対応後のバイナリは、足した 4 件のうち 3 件（指摘 3・4 の形と、edition 2027 で関数値の仮引数をそのまま渡す形）を拒否します）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`（701 件）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`・`rc.md`）は `Docs: 3 pages, 59 links, 18 checked examples, 36 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（指摘 8 の文言は既存の `store_local_borrow_through_reference` です）。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。機械の負荷平均が 30〜45 と高く、同じプログラムでも回によって 3 割以上違います。`r7/time-perf.txt`・`time-drops.txt`）: 指摘 9 の `or_10_3_5` 75 / 1037 / 159、`or_8_4_5` 313 / 13767 / 723、`or_20_3_5` 559 / 54744 / 1338、`or_10_4_5` 739 / 66619 / 1707（指数的だった増え方が基準と同じ形になり、残る 2 倍ほどは書き込みの事実の検査の回数の分です。レビューが時間切れと報告した `or_20_4_5` は対応後 18.8 秒、基準 12.9 秒、`or_12_5_5` は 64 秒、基準 15.8 秒で、型検査はどちらも 15〜37 秒です）。指摘 5 の `perf_droplocals_500`・`1000`・`2000`（edition 2026）は 5.8 / 46.5 / 59.1、18.0 / 230 / 269、64.5 / 951 / 1061 で、基準と同じ 2 乗のまま（判定は変えていません）。第 6 回までの表: `drop_locals_600` 8.2 / 14.7 / 15.8、`pattern_param_chain_200` 3.2 / 12.0 / 11.9、`perf_match_chain_400` 177 / 228 / 207、`perf_active_chain_200` 44.0 / 51.8 / 51.7、`perf_locals_600` 707 / 1696 / 1749、`chain_1600` 41.5 / 266 / 271（5 回）、`ring_400` 7.8 / 29.3 / 25.6（5 回）、`perf_holders3_480` 31.5 / 209 / 193、`perf_drops_600` 12.9 / 118 / 101、`perf_hdrops_400` 47.7 / 189 / 151、`p08_builder_pipe_22` 0.36 / 0.99 / 1.16、`perf_rev_chain_2000` 3.2 / 20.5 / 14.5。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/drop.md`（解放の順序: 一部の経路でだけムーブした値、`break`・`continue`・例外での脱出、借用を持てない `Drop` 型）、`lifetimes.md`（現在の制限: 書き込める仮引数の先の借用は届く書き込みだけを拒否すること、呼び出し元の共有の借用、関数値の仮引数へ渡した借用とその後の書き込み、仮引数をそのまま渡す関数、所有者をムーブした先）、`built-in-types-and-modules/rc.md`（所有者をムーブして渡した関数の書き込み）、`docs/language.md`（書き込みの事実・値の破棄・`Rc.share` の項目）、`docs/architecture.md`（書き込みの事実の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`src/check.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第8回）（2026-11-12）

レビューの 6 件の指摘（反例の探索 `target/scratch/review-a31-ce-r8/`、適合の確認 `target/scratch/review-a31-conf-r8/`）を、対応前の先端（`ec955303`、`r8/bin/tsuzuri-before`）・基準・対応後のバイナリで確かめました。6 件とも指摘のとおりに再現しました（指摘 1 と 4 は基準でも同じく受理される穴です。指摘 4 は「[pre-existing, plain]」ですが、下のとおり A31 の破棄の順序の検査に小さな局所的な変更を足すだけで閉じたので、この対応で直しました）。結果は `target/scratch/a31/r8/` にあります（この対応で書いたプログラムは `r8/adv`、`r8/corpora.sh <tag>` がレビューの 8 回分のプログラムの `check --json` を、`r8/perf.sh` が性能の表を作ります）。

設計の変更（`src/ownership.rs`、`src/ownership_control.rs`）:

- **ロックがコールバックへ貸す中身の借用を、コールバックの外へ残させません**（`Arg::locked`、`Source::locked`、`locked_borrow_kept`）。`Mutex.with_lock`・`RwMutex.with_read`・`RwMutex.with_write`・`Cond.wait_with` のコールバックが、中身から取った借用を別のセルへ書き込むと、今までは書き込み先にロックの共有の loan が残るだけでした。ロックは共有の参照で取れるので、後の `Mutex.with_lock (ref m) (\a -> grow a)` は中身を伸ばして解放でき、残した借用は解放された値を指しました。F10 が結果について述べる理由（ロックの外から中身に触れる道がない）と同じく、`builtin_stores` がロックのコールバックへ渡す出どころを `locked` にし、コールバックの事実がその `Part::Value`（中身の借用。中身が持つもの `Part::Contents` は数えません）を格納する組、隠れた環境へ格納する組、残しうる見えないコードへ渡す呼び出し（`apply_opaque`。局所値の借用と同じ条件に、中身そのものへの書き込みを足したもの。中身の型が書き込み先にならず、セルを所有する関数値も持たないなら数えないよう、ロックが貸す参照の `hides`・`target` は中身の型から求めます。中身にある関数値を呼ぶだけの呼び出しは、その関数値が自分の環境（中身が持つもの）で走るので数えません）を、呼び出しの位置の `E1013`（`mutex_result_retains_borrow`、`a lock's callback cannot keep a borrow of the locked value beyond the call: ...`）にします。`locked` は高階の事実（`Source::locked`。ロックへコールバックの仮引数を渡す本文、`$builtin` の部分適用、`SyncMap.with_value`）と呼び出しの出どころ（`Part::Value` から作った値だけ）へ伝わります。指摘 1。
- **ガードが無く何も検査しない選択肢を持つ腕より後の腕を、到達しないものとして評価します**（`eval_match`）。`try` のハンドラーは、どの腕にも合わない例外を外側の `try` へ送る `Reraise` の腕を常に最後に持つので、`| e -> e` の後でもその腕が `raise_edge` へ届き、第 7 回の `check_exit_drop_order` が外側の `try` へ抜けるスコープの破棄を、内側で例外が起きた位置の状態で検査していました。指摘 2。
- **エラーが名前を挙げる呼び出し先を、ソースの名前で示します**（`callee_subject`）。持ち上げたラムダ（`$lambda`・`$once` のモジュール、番号の名前）は、本文でそれを束縛した局所値（`let` と `let rec`）の名前、見つからなければ `the lambda`、instance のメソッド（`$instance.<n>.<method>`）は `the instance method '<method>'` です。指摘 3。
- **仮引数の破棄の順序も検査します**（`check_parameter_drop_order`）。仮引数は本体の局所値の後に宣言の逆順で破棄されるので、`check_body` が本体の評価の後で、仮引数を宣言の順に並べて `check_drop_order_of` を呼びます（位置は本体の始まり）。`ref` の仮引数（edition 2027 の読み取りの仮引数を含む）は参照なので `drop_reads` が数えず、その借用の根は呼び出し元のデータなので数えません。指摘 4。
- **高階の事実と隠れた環境の事実でも、共有の借用だけの出どころを記録します**（`Source::shared`、`StoreFacts::hidden_shared`、`fresh_exclusive`）。第 7 回の `shared_only` は直接の格納の組だけだったので、同じ借用を関数値の仮引数へ渡す本文（`use (\() -> String.length sr)`）や、配列の要素から読んだレジストリへ渡す本文（隠れた環境）では、新しく作った `ref mut v0` を渡した呼び出し元に排他の loan が残り、その後の `v0.count` の読み出しが `E1014` でした。`source_of` は、`ref mut` の仮引数の先の共有の借用だけでできた仮引数を `Source::shared` に、`store_hidden_except` は同じ組を `hidden_shared`（`join` は `shared_only` と同じ規則）に記録し、呼び出し元は新しく作った `ref mut` の借用の実引数なら `shared_copy` を渡します。指摘 5。
- **関数値の仮引数へ渡す局所の所有者の共有を、所有者の借用ではなくブロックの所有者として扱います**（`record_higher` の `expand_shares`）。`source_of` は局所値の loan をすべて `local` にするので、`Arc.share (ref lr)` を関数値の仮引数へ渡すと、呼び出し元が共有を残す関数値を渡す安全な形が `E1013` でした。`store_external` と同じく共有を所有者が持つ借用に読み替え、ブロックをその仮引数の外部ルートと隠れた環境へ書き出します（`export_block`。関数値はセルを所有する自分の環境へ共有を残しうるため）。その後で本体がブロックのセルへ局所値の借用を書き込むと `E1013`（`s01`）、仮引数の借用なら仮引数への事実と隠れた環境の事実として呼び出し元へ伝えます。外部ルートだけへ書き出した途中の版では、第 6 回の `sh19`（関数値がセルを所有するレジストリ）が edition 2026 で受理されたので（基準は `0`、期待は `9`）、隠れた環境へも書き出し、テストに足しました。指摘 6。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（ロックの中身の借用が、別のセルへのその場の書き込みやレジストリを通ってコールバックの外へ出る。危険な形を受理） | 再現（`m05`〜`m09`・`m11`・`m13`・`m14`・`m30` が両方の edition で受理、`0`（期待は `9`）、`m30` は `10`。`m15` は `E1014` で、loan が共有だけだったことを確かめた） | `locked`。どれもコールバックを渡した呼び出しの位置（`m30` は `SyncMap.with_value` の呼び出し）の `E1013`、`mutex_result_retains_borrow`。この対応で書いた `a04`（`Cond.wait_with`）・`a07`（名前付き関数の部分適用）・`a08`（借用を関数値の仮引数へ渡すラッパー）・`a09`（`Mutex.with_lock (ref m)` の部分適用）も同じ（基準ではどれも `0`）。対照 `a01`（中身の関数値のコピー）・`a02`（所有値）・`a03`（`Arc.share` の強い参照）・`a05`（借用をコールバックの中だけで使う）・`a06`（外の借用を中身へ書き込む）・`a10`（読むだけのラッパー）・`a11`（`SyncMap.with_value` で読む）・`a13`（`RwMutex.with_read` から関数値をコピー）・`a15`〜`a18`（本体の見えないコールバック。`a17` はセルを所有するレジストリのあるモジュール）は受理、値は基準と同じ。第 6 回の `sm01`・`sm02`（中身の関数値を `(deref g) ()` で呼ぶ）の診断も変わりません | `tests/rc.rs` `a_borrow_of_a_locked_value_cannot_be_kept_beyond_its_callback` |
| 2（どの例外にも合うハンドラーの内側の `try` で、外側の `try` へ抜ける破棄を検査する。基準からの退行） | 再現（`c02` が両方の edition で `12:17` `E1013`、基準は受理 `drop 0`/`drop 9`/`drop 9`/`1`） | `eval_match`。`c02` と、この対応で書いた `c11`（ガードの付いた腕の後に `| e -> e`）は受理、値は基準と同じ。対照 `d02`（ガードの付いた腕だけ）は `12:17` の `E1013` のまま | `a_handler_that_catches_every_exception_raises_nothing_to_the_outer_try` |
| 3（`E1013` の文言が持ち上げた関数の内部名 `'14'`・`'$instance.816.leak'` を示す） | 再現（`n01`・`n02`・`n04`・`n05`） | `callee_subject`。`n01`・`n02` は `'leak'`、`n04` は `the instance method 'leak'`、`n05` は `'put'`。`n03`（名前付き関数）は `'leak'` のまま | `an_error_names_a_lifted_callee_as_the_source_does` |
| 4（後に宣言した仮引数の借用を持つ仮引数の `Drop.drop` が、解放後の値を読む。[pre-existing, plain]） | 再現（`p01`・`p05`・`p06`・`p07` が受理、`drop 0` など。`p09` は `drop 9`） | `check_parameter_drop_order`。4 件と、この対応で書いた `d13`（借用される仮引数が間にある）は本体の始まりの `E1013`、`assignment_extends_borrow`。対照 `p09`、`d11`（`ref` の仮引数の借用）、`d12`（借用される仮引数が先）、`d14`（保持する仮引数をムーブ）は受理 | `a_parameter_whose_drop_runs_code_keeps_no_borrow_of_a_later_parameter` |
| 5（関数値の仮引数・レジストリを通した共有の借用で、呼び出し元に排他の loan が残る。基準からの退行） | 再現（`h06`（2027）・`h07`・`h09`・`h10` が `E1014`、基準は受理 `14`・`12`） | `Source::shared`・`hidden_shared`。4 件とも受理、値は基準と同じ。この対応で書いた `b01`・`b02`（呼び出し元が書き込み先が生きている間に `v0.name` を書く）と `b03`（`ref mut` の変数をそのまま渡し、その後で書く）は `E1014` のまま | `a_shared_borrow_handed_to_a_function_value_leaves_the_caller_a_shared_borrow` |
| 6（関数値の仮引数へ渡す局所の `Arc` の共有が局所値の借用に数えられ `E1013`。基準からの退行） | 再現（`s04`・`s02` が `E1013`、基準は受理 `9`） | `expand_shares`（外部ルートと隠れた環境）。2 件とも受理 `9`。危険な形 `s01`（後でブロックのセルへ局所値の借用を書き込む）・`s03`（共有をレコードに入れ、所有者をムーブ。基準は `-O0` で不定の値）は `E1013` のまま、第 6 回の `sh19` は両方の edition で `14:13` の `E1013` | `a_share_of_a_local_block_handed_to_a_function_parameter_is_an_owner` |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 1 は、指摘の案のとおり F10 の規則をコールバックの事実へ広げる方を選びました。書き込み先に残す loan を、ロックへの共有の借用とも競合する特別な loan にする案は、ほかの tzroutine がロックを通して書き込む場合を防げないため採りません。格納先がコールバックの外か中身そのものかは区別しません（中身へ中身の借用を書き込む自己参照も、ロックの外からの書き込みで解放されうるため）。中身が持つもの（コピーした関数値、所有値、`Arc.share` の強い参照）は今までどおり格納できます。基準はこれらの危険な形をすべて受理するので、A31 の範囲の健全性の穴として直しました（「plain」の双子は無く、外の `let mut` へはコールバックから書けず、`ref mut` の捕捉は `E1005` です）。
- 指摘 2 で、腕が型や条件で例外を選ぶハンドラー（`| e is OverflowException -> e`、ガードの付いた腕だけ）は、選ばれない例外が外側へ進みうるものとして今までどおり検査します。`@checked` の演算が起こしうる例外の種類は見ないので、`OverflowException` だけを選ぶハンドラーで包んだ `c12` は、実際には例外が外へ出なくても拒否のままです（第 7 回からの安全側の制限。基準は受理。どの例外にも合う腕を足せば受理されます。`drop.md` に書きました）。
- 指摘 3 は文言だけの変更です。instance の型やクラスの名前は持ち上げた関数から分からないので、メソッドの名前で示します。
- 指摘 4 は「[pre-existing, plain]」ですが、A31 の破棄の順序の検査（`check_drop_order_of`）を仮引数にも呼ぶ 15 行ほどの変更で閉じたので、別のチケット（A33）は作らず、この対応で直しました。
- 指摘 5 の共有の写しは、第 7 回と同じく呼び出し元が新しく作った `ref mut` の借用を渡すときだけです。`ref mut` の変数や仮引数をそのまま渡すときや、関数がその `ref mut` を別の関数へそのまま渡す（中継する）ときは、今までどおり排他の loan を残します（安全側）。
- 指摘 6 の書き出し先は、共有を渡した関数値の仮引数の外部ルートと隠れた環境です。呼び出し元が渡す関数値が共有を残さなくても、その後のブロックのセルへの局所値の借用の書き込みは定義の位置の `E1013` です（第 7 回の `e08` と同じく安全側）。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-12。ログは `target/scratch/a31/r8/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3042 件すべて成功、4 件 skip（1255 秒、`r8/nextest-1.log`）。第 7 回の 3036 件から増えた 6 件は `tests/rc.rs` の新しいテストです。T1 の後で `tests/rc.rs` に足した 2 つの形（`a17`、第 6 回の `sh19`）を含め、`cargo test --locked --test rc` は 64 件すべて成功します。途中の版の T1（`r8/nextest-0-partial.log`）は 2333 件まで失敗なしで、上の `sh19` などを直すために止めました。
- 新しいテストが対応前に失敗すること: `src/ownership.rs`・`src/ownership_control.rs` を `ec955303` に戻して `cargo test --test rc` で新しい 6 件を実行し、6 件とも失敗することを確かめました（`r8/fail-before.txt`。戻した後は先端に戻し、バイナリも作り直しています）。
- レビューの 8 回分のプログラム（`r8/corpora.sh b`、両方の edition）: 第 1 回の 1202 件、第 2 回の 338 件、第 3 回の 270 件、第 4 回の 394 件、第 5 回の 382 件、第 7 回の 338 件（`r7/adv`・`r7/f4` を含む）は、第 7 回の対応後（`r7/*corpus-c.json`）から変化なし。第 6 回の 668 件は `sh19` の edition 2026 だけで、拒否のまま kind と位置が edition 2027 と同じ `14:13` の `assignment_extends_borrow` になりました。第 8 回と `r8/adv` の 408 件（対応前 `r8corpus-before.json` と対応後 `r8corpus-b.json`）では、受理から拒否が 38 件（すべて上の表の危険な形と、`mutex.md` の例。基準で実行すると `0`・`drop 0` など）、拒否から受理が 17 件（`c02`・`c11`・`c13`、`h06`・`h07`・`h09`・`h10`、`s02`・`s04`。すべて安全な形）、拒否のまま最初の誤りが変わったものが 2 件（`m15` の両方の edition で、先に評価するコールバックの `E1013` が `E1014` の前に出ます）です。
- 受理されるレビューの第 8 回のプログラムと `r8/adv` の 88 件（両方の edition）は、`-O0`・`-O3` の値がすべて基準と同じです（`r8/run-final.json`・`run-base.json`、`r8/vals.txt`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r8/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r8/step5-final.txt`。`in_place_writes` の fixture に足した 5 件も基準と同じく受理されます）。
- E2E（`r8/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 59 件（基準のバイナリでも 59 件同じ値で成功。対応前のバイナリは足した 5 件のうち 4 件を拒否します）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`（701 件）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。入れ子のロックを使う 2 件（中身のコピー、関数値の仮引数へ渡す共有）は、WebAssembly が一度に 1 つのロックしか持てないので `nativeCases` です。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`lifetimes.md`・`drop.md`・`rc.md`・`mutex.md`・`rwmutex.md`・`cond.md`・`syncmap.md`）は `Docs: 7 pages, 104 links, 25 checked examples, 50 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（指摘 1 の文言は既存の `mutex_result_retains_borrow` です）。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。`r8/time-perf.txt`）: レビューの `perf_ho_chain_80`・`160`・`320`（関数値の仮引数を中継する関数の鎖）は 0.62 / 5.77 / 6.01、0.86 / 10.4 / 10.8、1.66 / 21.6 / 22.2（鎖の長さに線形で、対応前とほぼ同じ）、`drop_locals_600` 3.7 / 5.9 / 6.0、`pattern_param_chain_200` 1.4 / 5.8 / 5.8。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/built-in-types-and-modules/mutex.md`（結果とロック: コールバックの外へ中身の借用を残せないこと）、`rwmutex.md`・`cond.md`・`syncmap.md`（同じ規則へのリンク）、`rc.md`（関数値の仮引数へ渡す共有）、`ownership-and-memory/drop.md`（仮引数の解放の順序と検査、どの例外にも合うハンドラー、型で選ぶハンドラーの制限）、`lifetimes.md`（関数値へ渡す共有の借用と呼び出し元の共有の借用、関数値の仮引数へ渡す `Arc` の共有）、`docs/language.md`・`docs/architecture.md`（書き込みの事実と破棄の順序の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第9回）（2026-11-12）

レビューの 7 件の指摘（反例の探索 `target/scratch/review-a31-ce-r9/`、適合の確認 `target/scratch/review-a31-conf-r9/`）を、対応前の先端（`2f53734c`、`r9/bin/tsuzuri-before`）・基準・対応後のバイナリで確かめました。7 件とも指摘のとおりに再現しました。指摘 1 と 5、2 と 6、3 と 7 はそれぞれ同じ穴で、どれも基準でも同じく受理されます。7 件とも代入の双子が無い（パターンの変数は `mut` にできず、一時値の `ref mut` は `borrow requires a local place`、レコードのフィールドは代入できない）ので「[pre-existing, plain]」ではなく、A31 の範囲（ロックのコールバックのその場の書き込み）として直しました。別のチケット（A33）は作っていません。結果は `target/scratch/a31/r9/` にあります（この対応で書いたプログラムは `r9/adv`、`r9/corpora.sh <tag>` がレビューの 9 回分のプログラムの `check --json` を、`r9/perf.sh` が性能の表を作ります）。

実行時の順序は、対応の前に基準と対応前のバイナリで確かめました: パターンの変数は腕の終わりで束縛の逆順（`po10`: `| (x, y)` は `drop 2`、`drop 1`。`z_order`）、文の一時値は作った順（`tp01`: `f (T { id: 1 }) (T { id: 2 })` は `drop 1`、`drop 2`。`t03` は `noisy dropped` が先）、レコードのフィールドは宣言順（`sf03`）、タプルの要素も順（`r9/adv/a03_tuple_order`: `drop 1`、`drop 2`）、文の末尾の式の一時値はブロックの局所値より先（`t06`）。

設計の変更（`src/ownership.rs`、`src/ownership_control.rs`）:

- **腕の終わりでパターンの変数の破棄の順序を検査します**（`eval_match`）。腕の本体の後、`finish_control_scope` の前に、パターンの変数と一時値（`PatternStep::Bind`）に `check_exit_drop_order` を当てます（位置は腕の本体の始まり）。コード生成は、束縛した変数を逆順に、その後でパターンの一時値を逆順に破棄するので、`binding_order`（一時値が先）の逆順と一致します。参照の先をその場で見るビュー（`views`）は照合した値の一部で、腕の後で照合した値とともに破棄されるので除きます。`break` で抜ける位置は第 7 回から検査していたので、腕が普通に終わる位置だけが抜けていました。指摘 1・5。
- **後に作った一時値（か自分自身）へ一時値の借用を保持させる呼び出しを拒否します**（`temporary_dropped_first`、`temporary_hidden_first`、`temporaries_dropped_first`、`temporary_dropped_first_error`）。今までは、借用した一時値（`Arg::dying`）の借用を格納する事実は、格納先が場所に届くか環境のときだけ `temporary_borrow_stored` でした。格納先も借用した一時値なら `has_targets` が偽で、何も比べていませんでした。一時値は作った順に破棄され、それはソースの位置の順（引数は左から、パイプラインは左辺が先）なので、格納元の式の位置が格納先より前か同じで、格納先の一時値の型が `drop_reads` なら、呼び出しの位置で `E1013`（`assignment_extends_borrow`、`cannot keep a borrow of a temporary value in itself or in a temporary made after it: ...`）です。式が無い値（呼び出し先が渡した値）は順序が分からないので数えます。隠れた環境へ格納する事実は環境を持ちうる型の一時値の引数について、本体の見えないコードは書き込み先になりうる型の一時値の引数（自分自身を含む）について同じ判定をします。自分自身への格納でも、事実が下の `ordered` なら受理します。指摘 2・6。
- **値が自分の部分の借用を持つときも、部分の破棄の順序を検査します**（`check_drop_order_of` の自己 loan、`self_borrow_dropped_first`、`parts_drop_read`、`self_borrow_error`）。今までは、後に宣言した別の局所値をルートとする loan だけを見ていました。局所値が自分自身をルートとする loan（共有の印の無いもの）を持つとき、場所のフィールドをたどり、レコードでは後に宣言したフィールド、タプルでは後の要素の型が `drop_reads` なら、また最後に借用した部分自身の部分が `drop_reads` なら（値自身の `Drop.drop` は部分より先に、すべてがある間に走るので数えません）、`E1013`（`self_referential_store`、`'p' keeps a borrow of a part of itself, ...`）です。要素（`ELEMENT`）・中身（`PAYLOAD`）など順序の分からない段では、その入れ物の部分が `drop_reads` なら拒否します。書き込んだ場所は追わないので、借用した部分より後のどの部分も読みうるものとします。局所値が 1 つでも自己 loan はありうるので、`check_drop_order`・`check_parameter_drop_order`・`check_exit_drop_order` の「2 つ以上」という条件を外しました（`r9/adv/a06u` は局所値 1 つの危険な形で、外す前は受理でした）。指摘 3・7。
- **呼び出し先が仮引数の先の部分の借用を同じ先へ格納する事実に、部分の順序の証明を付けます**（`StoreFacts::ordered`、`ordered_self_borrow`、`ordered_copy`、`ordered_loans`）。`keep (ref p)` の本体が `let t = ref p.s` を `p.h` のセルへ書き込むと、事実は `(0, Value, 0)` で、呼び出し元では実引数の借用（`p` 全体）が `p` に残るので、上の検査は `p` 全体の借用として拒否します（安全な順序 `Pair { h, s }` でも）。`store_external` が自分自身への `Part::Value` の格納を記録するとき、参照の仮引数の先の型で `self_borrow_dropped_first` が偽なら `ordered` に入れ（合流は `shared_only` と同じ）、呼び出し元は格納する loan を印付きの複製にして、`check_drop_order_of` は実引数の場所より上の段だけを検査します。
- **値で受け取った仮引数のセルを、呼び出し元のデータとして扱いません**（`store_target`、`apply_store_within`、`owned_parameter`、`place_is_cell`、`reaches_shared_cell`）。所有の仮引数（型が参照でないもの）の外部 loan は呼び出し元の値が持つものを表しますが、型がセルに届くだけで書き込み先にしていたので、`own Mutex<[Caller]>` の仮引数のセルへ局所値の借用を書き込むと、外部への格納として `store_local_borrow_through_reference`（`... through a reference parameter; the caller's data would outlive it`）でした。値で受け取ったセルは関数が所有し、`Arc` のように共有できないので、`store_target` はその型が参照か共有ポインターの先でセルに届くときだけ、セルを理由にその loan を書き込み先とします（関数値・dyn 値を理由にする規則はそのまま）。`apply_store_within` は、格納が所有の仮引数自身が格納するセル（フィールド・タプルの要素・コレクションの要素をたどった先が `Mutex` などのセル。`Mutex.with_lock (ref c.cell)`、`ref m`）へ入るときは、仮引数自身の外部 loan を辿りません。そのセルへの格納は、呼び出し元の値が持つもののどれにも届かないためです（仮引数が持つ関数値の環境へは、その関数値の呼び出しだけが書き込み、その callee がその loan を直接持ちます）。そうした格納は仮引数の破棄の順序（第 8 回）で検査します。指摘 4。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1・5（腕のパターンの変数の破棄の順序を検査しない。危険な形を受理） | 再現（`F1`・`F1u`・`po11`・`po16`・`po17`・`po21`・`po22`、`m01`・`m06` が両方の edition で受理、`drop holder reads -1`・`drop 0`（期待は `8`・`9`）。`m12`（`break`）は拒否、`po18`・`m02`・`F1c` は正しい値） | `eval_match` の `check_exit_drop_order`。どれも腕の本体の始まりの `E1013`、`assignment_extends_borrow`。この対応で書いた `a16`（3 つのうち最後に束縛した変数の借用）・`a17`（共用体の中身）・`a15` の edition 2027（読み取りの仮引数へ渡した保持する値は腕に残る）も同じ。`F1g`（関数の腕）と、ラムダのパターン `po12`・`m04`・`m11` は、今までは指摘 4 の誤分類の `store_local_borrow_through_reference` で、この順序の `E1013` になりました（基準は `drop 0`）。`F1r`（レジストリ）は拒否のまま、最初の誤りが腕の終わりの順序になりました。対照 `F1c`・`po18`・`m02`・`a16c`・`a15` の edition 2026（保持する値を値でムーブ）は受理、値は基準と同じ | `tests/rc.rs` `a_pattern_variable_whose_drop_runs_code_keeps_no_borrow_of_a_later_one` |
| 2・6（呼び出しが一時値の借用を後に作った一時値へ保持させる。危険な形を受理） | 再現（`F2`・`pa06`・`t02`・`t03` が両方の edition で、`F2r`・`pa05`・`pa09` が edition 2027 で受理、`-1`・`drop 0`・`caller drop reads 0`。`F2c`・`t01` は正しい値） | `temporary_dropped_first` ほか。どれも呼び出しの位置の `E1013`、`assignment_extends_borrow`。この対応で書いた `a09`（利用者の `Eq` の演算子の 2 つの一時値。基準は `drop holder reads 0`、期待は `9`）・`a06t`（呼び出し先が一時値全体の借用を自分へ）・`a21u` の一時値版も同じ。対照 `F2c`・`t01`・`a21` の一時値版（部分の順序の証明付き）は受理、値は基準と同じ | `a_temporary_whose_drop_runs_code_keeps_no_borrow_of_one_made_before_it` |
| 3・7（値の中の自己参照の破棄の順序を検査しない。危険な形を受理） | 再現（`sf01`・`sf05`・`sf10`、`s01` が両方の edition で受理、`drop holder reads -1`・`caller drop reads 0`。`sf06`・`s01b` は正しい値。代入の双子 `sf02`・`sf11` は `self_referential_store`、ムーブ `sf07`・`sf08`・`s06` は `E1014`） | 自己 loan の検査。どれもブロック（仮引数なら本体、腕なら腕の本体）の始まりの `E1013`、`self_referential_store`。この対応で書いた `a05`（入れ子のレコード）・`a07`・`a08`（所有の仮引数）・`a18`（パターンの変数）・`a21u`（呼び出し先が先に解放される部分の借用を格納）・`a06u`（局所値 1 つ）も同じ（基準はどれも `-1`）。対照 `sf06`・`s01b`・`a05c`・`a08c`・`a18c`・`a21`（呼び出し先、部分の順序の証明付き）は受理、値は基準と同じ | `a_value_whose_parts_run_drop_code_keeps_no_borrow_of_a_part_dropped_before_them` |
| 4（値で受け取った `Mutex` の仮引数が参照の仮引数として扱われ、安全な形が誤った文言の `E1013`。基準からの退行） | 再現（`rg02` が両方の edition で `8:5` の `store_local_borrow_through_reference`、基準は受理 `drop 0`/`drop 9`/`0`。`F2r`・`pa05`・`pa09` の edition 2026 も同じ） | `owned_parameter`・`reaches_shared_cell`。`rg02`、`F2r`・`pa05`・`pa09`・`r9/adv/a02c`（パイプライン）・`a10`（本体の見えない関数値）の edition 2026 は受理、値は基準と同じ。危険な順序 `rg07`、第 8 回の `u02`、`pa02`・`pa04`・`po12`・`m04`・`m11`・`a02b`・`a10c` の edition 2026（基準は `drop 0`・`-1`）は、仮引数の破棄の順序の `E1013`（`assignment_extends_borrow`）になりました。`Arc` の仮引数 `rg06` は `store_local_borrow_through_reference` のまま。セルを所有する関数値のあるモジュールでも受理されることを、`in_place_writes` の fixture の `owned_parameter_cells` で確かめました | `an_owned_parameter_owns_the_cells_that_it_stores` |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 1・5 は、指摘の案のとおり腕の終わりで `check_exit_drop_order` を当てました。ガードの中の書き込み（`r9/adv/a01`: ガードで呼ぶ関数が `s` の借用を `c` のセルへ書き込む。基準は `-1`）は、ガードの束縛が照合した値の別名なので照合した値の自己 loan になり、その後のパターンの変数への取り出しが対応前から `E1014` です。
- 指摘 2・6 の順序はソースの位置で比べます。引数は左から評価し、パイプラインは左辺を先に評価するので、位置の順は作った順です。式の無い値は順序が分からないので数えます（安全側）。edition 2027 では、パイプラインと本体の見えない関数値へ一時値を貸すこと自体が `borrow_of_temporary` です。
- 指摘 3・7 は、書き込んだ場所を追わず、借用した部分より後に破棄される部分のどれもが読みうるものとして扱います。そのため次は安全でも拒否します（基準は受理。`drop.md` に書きました）: 呼び出し先が仮引数そのものを捕捉した関数値（`\() -> look (ref p.s)` で `p` を捕捉）を同じ値へ格納する形（`r9/adv/a06`・`a06b`・`a20`、`a19c`: 値全体の借用なので、値の部分の破棄が借用を読みうるなら拒否）、書き込んだ先より後に読む部分がある形（`a19`: `Outer { p: Pair { h, s }, h2 }` の `h2`）、配列などの別の要素の借用（添字を区別しない）。関数値がどの部分を読むか、どこへ書き込んだかは、今の検査の形（場所ごとの loan、事実は仮引数の番号）では分からないためです。指摘の保守的な案（自己 loan を持つ値の破棄が読みうるなら常に拒否）は、安全な順序の `sf06`・`s01b`（基準も対応前も受理）を拒否するので採りません。
- 同じく、隠れた環境への格納（セルを所有する関数値の呼び出し）が局所値へ自分自身の loan を残すことがあります（第 5 回の `sn01`・`tt06`: `Drop.drop` がレジストリへ借用を書き込む値）。値自身の `Drop.drop` は部分より先に走るので、最後の段では値の部分だけを数えます（`parts_drop_read`）。途中の版（値自身の `Drop.drop` も数えた版、`r9/bin/tsuzuri-b`。レビューの 9 回分のプログラムを `r9/corpora.sh b` で確かめた版）では `sn01`・`tt06` が `self_referential_store` になったので直しました。
- 指摘 4 は、所有の仮引数の外部 loan を辿らない場合を、仮引数自身のセルへの格納に絞りました。途中の版（`r9/bin/tsuzuri-d`）は、仮引数の記憶域への格納すべてで、外部 loan を参照か共有ポインターの先のセルに届く型のときだけ辿ったので、関数値の仮引数を借用して渡す形（第 3 回の `l01` の edition 2027）が受理になりました。`l01` 自体は安全（基準と同じ `10`）ですが、関数値の環境は呼び出し元のコピーと共有しうるので、その環境へ届く格納を見落とす形がありうるため、今の版に絞りました（`l01` は第 3 回からの安全側の制限のまま）。セルを所有する関数値のあるモジュールでも、仮引数のセルへの直接の書き込みは仮引数の破棄の順序だけで検査します（`r9/adv/a25`、fixture の `owned_parameter_cells`）。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-13。ログは `target/scratch/a31/r9/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（clippy は最初、新しいテストの要素 1 つの `for` を 2 か所指摘したので、直接の呼び出しにし、`a25` を足しました）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は、最初の版（`bb8881f0`）で 3046 件中 3044 件成功、2 件失敗（`diagram_cli` の `kg_build_writes_views_and_verify_passes`・`kg_diagram_prints_the_diagram`。共有のマシンの `kg` の揺れで、次の実行では成功）、最終の版（`a5050194`）で 3046 件すべて成功、4 件 skip（1377 秒、`r9/nextest-2.log`）。第 8 回の 3042 件から増えた 4 件は `tests/rc.rs` の新しいテストです。領域のテスト `cargo test --locked --test rc --test mutable_slices --test currying --test borrowed_records --test user_drop` は 68・19・16・21・27 件すべて成功。
- 新しいテストが対応前に失敗すること: `src/ownership.rs`・`src/ownership_control.rs` を `2f53734c` に戻して `cargo test --test rc` で新しい 4 件を実行し、4 件とも失敗することを確かめました（`r9/fail-before.txt`。戻した後は先端に戻しています）。後で足した `a25` も対応前のバイナリは `12:5` の `store_local_borrow_through_reference` で拒否します。
- レビューの 9 回分のプログラム（`r9/corpora.sh final`、両方の edition。対応前は `r9/*corpus-before.json`）: 第 1 回の 1202 件、第 2 回の 338 件、第 3 回の 270 件、第 4 回の 394 件、第 5 回の 382 件、第 6 回の 668 件、第 7 回の 338 件は変化なし。第 8 回の 408 件は `u02`（所有の仮引数の危険な順序）の両方の edition で、拒否のまま kind が仮引数の破棄の順序の `assignment_extends_borrow`（`7:5`）になりました。第 9 回と `r9/adv` の 348 件では、受理から拒否が 68 件（すべて危険な形で、基準で実行すると `-1`・`0`・`drop 0` などの誤った値か、解放した値を読みます。`sf01` は解放後の長さがたまたま `9` で、同じ形に別の割り当てを挟んだ `sf05` は `-1` です。ただし、この対応で書いた安全な `a06`・`a06b`・`a19`・`a19c`・`a20`（両方の edition の 10 件）は、上の判断のとおり安全側で拒否します）、拒否から受理が 9 件（`rg02` の両方の edition、`F2r`・`pa05`・`pa09`・`a02c`・`a10` の edition 2026、`a08c` の両方の edition。すべて安全な形）、拒否のまま最初の誤りが変わったものが 18 件（`F1g`・`po12`・`m04`・`m11`・`rg07`・`a08` の両方の edition と `pa02`・`pa04`・`a02b`・`a10c` の edition 2026 は危険な順序の `E1013` へ、`F1r` の両方の edition は腕の終わりの順序へ）です。途中の版の確かめ: `r9/corpora.sh b` で第 5 回の `sn01`・`tt06` が拒否になったので `parts_drop_read` を、`r9/corpora.sh d` で第 3 回の `l01`（edition 2027）が受理になったので仮引数自身のセルに絞る規則を入れ、最終の版で両方とも第 8 回と同じ結果に戻ったことを確かめました。
- 受理されるレビューの第 9 回のプログラムと `r9/adv` の 91 件（両方の edition）は、`-O0`・`-O3` の出力がすべて基準と同じです（`r9/vals.txt`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`r9/compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`r9/step5-final.txt`。`in_place_writes` の fixture に足した 4 件も基準と同じく受理されます）。
- E2E（`r9/e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `in_place_writes` 63 件（基準のバイナリでも 63 件同じ値で成功。対応前のバイナリは足した `owned_parameter_cells` の `keep_owned` を `1026:5` の `store_local_borrow_through_reference` で拒否します）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`（701 件）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`）は `Docs: 2 pages, 40 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（既存の `assignment_extends_borrow` と `self_referential_store` です）。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。マシンが混んでいて、第 8 回の測定のおよそ 2 倍の値です。`r9/time-perf.txt`）: レビューの `perf_loop_breaks_400` 291 / 758 / 779、`perf_try_checked_400` 392 / 860 / 853、`perf_ho_wrap_120` 1.10 / 3.59 / 3.33、`perf_match_nest_30`（5 回）2.39 / 3.05 / 2.64、第 8 回の `perf_ho_chain_320` 3.96 / 46.2 / 48.8、第 6 回の `drop_locals_600` 6.67 / 11.9 / 11.4、`pattern_param_chain_200` 2.62 / 16.5 / 17.0（どれも対応前とほぼ同じ）。`perf_match_nest_60`・`120` と `perf_nested_lock_*` は、どのバイナリでも構文の入れ子の上限か `E1005` で検査まで進みません。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/drop.md`（解放の順序: パターンの変数、文の一時値、タプル・要素の順。検査: パターンの変数、一時値、値の自分自身の部分と安全側の扱い）、`lifetimes.md`（値で受け取った仮引数のセル）、`docs/language.md`・`docs/architecture.md`（破棄の順序の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第9回の後）（2026-11-13）

第9回の対応（`3a9e4733`）を A32 のブランチへ merge した作業（`target/wt/a32/target/scratch/a32/merge3/`）で、2 つの問題が見つかりました。1 つは第9回の退行、もう 1 つは基準からある穴です。どちらも対応前の先端（`3a9e4733`、`r10/bin/tsuzuri-before`）・基準・対応後のバイナリで再現を確かめてから直しました。結果は `target/scratch/a31/r10/` にあります（再現は `r10/min2`・`r10/fx`・`r10/sm`、`r10/corpora.sh <tag>` がレビューの 9 回分と第10回のプログラムの `check --json` を、`r10/cdiff.mjs` がその差を出します）。

- **問題 1（第9回の退行、修正の妨げ）**: conventions の fixture の `callback_constructed_once`（`Array.map ({ calls = calls + 1; owned_inc }) [1, 2, 3]`）が、モジュールに `Drop` の instance と、値を捕捉したラムダを入れた `Vec` があると `E1013`（`assignment_extends_borrow`、`cannot keep a borrow of a temporary value in itself or in a temporary made after it`）になりました（A32 の fixture。20 行の再現 `r10/min2` は、対応前のバイナリが拒否し、基準は手で数えた `50110` を出します）。原因: `Array.map` のラムダ（`\index -> transform values[index]`）が渡す要素は、高階の事実の出どころ（`Source`）で `dying`（借用した一時値の配列から読んだ値）を受け継ぎ、呼び出し元では式の無い引数になります。値を捕捉したラムダがあるとこの呼び出しは見えないコードの規則（`temporaries_dropped_first`）に落ち、式の無い引数は型が分からない（`temporary_type` が `None`）ので、「自分自身へ借用を保持させる一時値」に数えていました。モジュールに `Drop` の instance があるかだけで決まり、渡す値の型（`i64`）の破棄が何も読まないことを見ていませんでした。第9回の E2E は conventions suite を実行していませんでした（A31 の fixture には `Vec` が無いので、A31 では suite も Rust のテストも通っていました）。
- **問題 2（健全性の穴、基準にもある）**: `record Pair { s: string, h: Holder }` を `ref mut Pair` で受け取る関数が `let t = ref p.s` を `p.h.cell` へ書き込む（ロックのコールバックのその場の書き込み）と受理され、`drop -1`（解放後の読み出し）を出しました。名前付き関数、`ref mut` の変数を渡す形、別の関数へ中継する形のどれでもです（`ref Pair` の版は第9回で拒否）。原因: 呼び出し元の `apply_store_within` は、排他の借用だけで届く持ち主へ格納する値から、その持ち主をルートとする loan をすべて「書き戻し」として除いていました。呼び出し先の事実 `(0, Value, 0)` は `shared_only` なので、呼び出し元は `shared_copy` で共有の借用の複製を格納しますが、その複製も除かれ、`p0` は自分の部分の借用を持たないままでした。そのため第9回の自己 loan の検査が届かず、さらに安全な順序 `Pair { h, s }` でも、呼び出しの後の `p0.s = ...` が受理され、`h` の `drop` が解放した文字列を読みました（`r10/sm/m5`、基準も `drop -1`）。この対応で、同じ穴が関数値の仮引数を通した呼び出し（`apply target (ref mut p0)`、`r10/sm/w_apply`）と本体の見えない関数値（`Vec` から読んだ関数値 `w_opaque_vec`、関数の結果 `w_opaque_dyn`）にもあることを見つけました（どれも基準で `drop -1`）。

どちらも「[pre-existing, plain]」ではありません（問題 2 は基準にもありますが、代入の双子が無い: フィールドの `ref mut` は書けず、ロックのコールバックのその場の書き込みは A31 の範囲です）。A31 で直し、別のチケット（A33）は作っていません。

#### 1 つの破棄の順序の規則

第6回〜第9回は、破棄の順序の穴を記憶域の種類ごとに閉じてきました（局所値、脱出、仮引数、パターンの変数・一時値・値の部分）。オーケストレーターの指示に従い、判定を 1 つの規則にまとめました（`src/ownership.rs` の `DropSlot`・`Borrowed`・`Checker::borrow_dropped_first`）。

> 借用を読みうる `drop` を実行する値（保持する値）は、その値より後に破棄される記憶域の借用か、自分自身の部分のうち、読みうる部分より後に破棄される部分の借用しか持てない。

「借用を読みうる」は保持する値の型で決めます（`drop_reads`: 型の格納された部分から届く、借用を持ちうる利用者の `Drop` 型と、`drop_environments` が求めたそうした値を環境に持ちうる関数値。弱参照の先は数えない）。型の分からない値だけを数えます（モジュールに `Drop` の instance があるかでは決めません）。借用の出どころ（代入、その場の書き込み、ロックのコールバック、呼び出し先の書き込みの事実、隠れた環境）は問わず、保持する値が破棄される所で、その値が持つものを見ます。破棄の位置は記憶域の種類ごとに次のとおりで、どの検査も `borrow_dropped_first` を呼びます（`DropSlot::Scoped` は同時に終わるスコープの束縛の順位で、大きいほど先に破棄。`Temporary` は文の一時値を作ったソースの位置で、作った順に破棄。`Unknown` は位置の分からない値で、常に先に破棄されうるものとする。`Borrowed::Part` は自分自身の部分で `self_borrow_dropped_first`）。

| 記憶域 | 破棄の位置と順序 | 規則での位置 | 検査する所 | テスト |
| --- | --- | --- | --- | --- |
| ブロックの局所値 | ブロックの終わり、宣言の逆順 | `Scoped`（宣言の位置） | `eval_block` の `check_drop_order` | `tests/rc.rs` `a_value_whose_drop_runs_code_keeps_no_borrow_of_a_later_local`（第6回） |
| 一部の経路でだけムーブした局所値 | ムーブしなかった経路でブロックの終わり（すべての経路でムーブした値は破棄しない: `State::moved_always`） | `Scoped` | 同上 | `a_value_that_a_branch_moves_or_a_jump_drops_keeps_no_borrow_of_a_later_local`（第7回） |
| `break`・`continue` で抜けるスコープの局所値 | その位置、束縛の逆順 | `Scoped`（`binding_order`） | `eval_loop_jump` の `check_exit_drop_order` | 同上 |
| `try` が受け止める例外で抜けるスコープの局所値 | 送出の位置、束縛の逆順（受け止める `try` が無ければトラップで、破棄しない） | `Scoped`（`binding_order`） | `raise_edge` の `check_exit_drop_order` | 同上、`a_handler_that_catches_every_exception_raises_nothing_to_the_outer_try`（第8回） |
| `match` の腕のパターンの変数とパターンの一時値 | 腕の終わり、束縛の逆順（照合した値をその場で見るビューは照合した値の一部） | `Scoped`（`binding_order`） | `eval_match` の `check_exit_drop_order` | `a_pattern_variable_whose_drop_runs_code_keeps_no_borrow_of_a_later_one`（第9回） |
| 関数・ラムダの値の仮引数（パターンの仮引数を含む） | 本体の終わり、局所値の後、宣言の逆順 | `Scoped`（宣言の位置） | `check_body` の `check_parameter_drop_order` | `a_parameter_whose_drop_runs_code_keeps_no_borrow_of_a_later_parameter`（第8回）、`an_owned_parameter_owns_the_cells_that_it_stores`（第9回） |
| `ref` の仮引数の先とその部分 | 呼び出し元の値（呼び出しより長く生きる）。先の部分の借用を同じ先へ格納すると、呼び出し元の値の自己 loan | 呼び出し元で `Part`（呼び出し先が順序を示せば `ordered`） | `store_external`（`ordered_self_borrow`）→ 呼び出し元の `check_drop_order_of` | `a_value_whose_parts_run_drop_code_keeps_no_borrow_of_a_part_dropped_before_them`（第9回） |
| `ref mut` の仮引数の先とその部分 | 同上（格納したのは共有の借用: `shared_some`） | 同上 | 同上と `apply_store_within`（共有の loan を残す）、`apply_facts_once`（どの実引数でも共有の複製）、`apply_opaque`（`self_borrow_types`） | `a_borrow_of_a_part_kept_through_an_exclusive_reference_keeps_the_drop_order`（この対応） |
| 文・呼び出しの一時値 | 文の終わり、作った順（引数は左から、パイプラインは左辺が先）。文の後も使う局所値より先 | `Temporary`（式の位置） | `temporary_dropped_first`（書き込みの事実・隠れた環境 `temporary_hidden_first`・見えないコード `temporaries_dropped_first`） | `a_temporary_whose_drop_runs_code_keeps_no_borrow_of_one_made_before_it`（第9回） |
| 呼び出し先が関数値へ渡す値（一時値の要素、呼び出し先が作った値） | 分からない | `Unknown`、型は呼び出し先から見た型（`Source::ty`・`Arg::ty`） | 同上 | `a_value_that_a_callee_passes_on_reads_what_its_type_reads_when_dropped`（この対応）、conventions の fixture の `capturing_readers` |
| レコードのフィールド | 値自身の `drop` の後、宣言順 | `Part`（`self_borrow_dropped_first`） | `check_drop_order_of` の自己 loan（局所値・仮引数・腕・脱出）、一時値は `temporary_dropped_first` | `a_value_whose_parts_run_drop_code_keeps_no_borrow_of_a_part_dropped_before_them`（第9回） |
| タプルの要素 | 値自身の `drop` の後、先頭から | `Part` | 同上 | 同上（`r9/adv/a03_tuple_order` の順序） |
| 配列・`Vec` などの要素、共用体の中身、`Mutex` などの中身 | 値自身の `drop` の後（検査は要素の順序を区別しない: 入れ物が読みうるなら拒否） | `Part`（順序の分からない段） | 同上 | 同上（`sf10`） |
| 隠れた環境（セルを所有する関数値、`Rc`・`Arc` のブロック） | 環境を共有しうる局所値の破棄（その局所値が保持する値） | その局所値の `Scoped` | `store_hidden`・`exports` で局所値へ渡し、上の検査 | `a_value_whose_drop_runs_code_keeps_no_borrow_of_a_later_local`（`dc01`・`k10`・`k11`、第6回） |

第10回で、この表に代入したときの古い値・持ち上げたラムダの捕捉・ビューの先・`Rc`・`Arc` のブロックの中の行を足し、`ref`・`ref mut` の仮引数の先と呼び出し先が関数値へ渡す値の行を変えました（下の「レビューの指摘への対応（第10回）」の表）。

違うのは保持する値と借用の先を集める所（スコープの終わりの局所値の値、呼び出しの一時値の引数と事実）とエラーの文言だけです。一時値と局所値をまたぐ組（局所値が一時値の借用を持つ、文の中で終わる腕の変数の借用を一時値が持つ）は、一時値の借用の規則（`borrow_of_temporary`・`temporary_borrow_stored`）と借用の生存の規則（`borrow_outlives_block`）が先に拒否するので、`dropped_before` は `Temporary` の借用を局所値が持つ組を常に先に破棄されるものとし、局所値の借用を一時値が持つ組を後に破棄されるものとします。

#### 設計の変更（`src/ownership.rs`）

- **1 つの規則**（`DropSlot`、`DropSlot::dropped_before`、`Borrowed`、`Checker::borrow_dropped_first`）。`check_drop_order_of` は局所値ごとに、後に宣言した局所値の loan を `Borrowed::Other(Scoped)`、自分自身の loan を `Borrowed::Part` として判定します（判定と報告の順序は今までと同じ）。`temporary_dropped_first` は、2 つの一時値を `Temporary`（式の無い値は `Unknown`）として判定し、同じ引数なら `Borrowed::Part` です。一時値が自分自身の借用を持つ組は、今まで値自身の `Drop.drop` も数えていましたが（`drop_reads`）、局所値と同じく部分の規則（値自身の `drop` は部分より先に走る）にしました。
- **呼び出し先が渡す値の型**（`Arg::ty`、`Source::ty`、`temporary_type`）。`record_higher` が高階の事実の出どころに、呼び出し先から見た実引数の型（借用した一時値なら一時値の型）を記録し、`apply_facts_once` の高階の呼び出しの適用が式の無い引数へ渡します。特殊化した本文の型なので具体的です。`temporary_type` は式があれば式の型、無ければこの型を返し、`temporary_dropped_first`・`temporary_hidden_first`・`temporaries_dropped_first` はその型で、書き込み先になりうるか（`may_reach_store`・`hidden_type`）と、破棄が読みうるか（`drop_reads`）を判定します。型の分からない値だけを数えます。問題 1。
- **排他の借用で届く持ち主の共有の自己 loan を残します**（`apply_store_within`）。書き戻しとして除くのは、排他の借用だけで届く持ち主をルートとする排他の loan だけにしました（フィールドや要素の `ref mut` は書けないので、排他の loan はその持ち主全体の排他の借用か、その再借用です）。共有の loan は呼び出し先が同じ値へ格納した共有の借用の複製なので、持ち主が持ちます。持ち主が持つ自分自身をルートとする loan はたどりません（たどると持ち主へ共有の借用で届いたことになり、排他の借用そのものが書き戻しでなくなるため。本体の見えない関数値の安全な順序 `ok_w_opaque_vec` を、2 回目の評価で自己の排他の loan として拒否していました）。問題 2。
- **どの実引数でも共有の複製を格納します**（`StoreFacts::shared_some`、`apply_facts_once`）。`shared_some` は、共有の借用を格納した組（`shared_only` の上位集合。合流は和、単調）です。組が `(p, Part::Value, p)` なら、呼び出し元は実引数が新しく作った `ref mut` でなくても（`ref mut` の変数、仮引数を渡す関数、高階の事実が渡す式の無い値）共有の複製を格納し、`shared_only` でない組（呼び出し先が排他の借用の書き戻しも格納する）なら実引数の loan とともに格納します。`ordered` の組では複製も `ordered_copy` にします。問題 2 の `w_apply` など。
- **中継した `ordered` を伝えます**（`ordered_self_borrow`）。`ordered_loans` の複製（呼び出し先が実引数の場所の中の順序を示した loan）は、実引数の場所より上の段だけを見ます。仮引数をそのまま渡す関数（`fn target p = inner p`）の安全な順序（`ok_relay`）を、最初の版は値全体の借用として拒否していました。
- **本体の見えない関数値**（`CallFacts::self_borrow_types`・`self_borrow_any`、`self_borrow_types`、`apply_opaque`）。プログラムの関数のうち、`ref mut T` の仮引数の先へ自分の部分の共有の借用を格納するもの（`shared_some` の `(p, Part::Value, p)`）の `T` と、その組がどれも `ordered` かを、`joint_facts` の不動点で求めます（集合は増えるだけ、`ordered` は減るだけで、変われば本文を検査し直します。予算を超えたときと、検査に通る `top` の関数があるときは `self_borrow_any`）。`apply_opaque` は、`ref mut T` の実引数の `T` がそれに入るなら、実引数の値の共有の複製（すべて `ordered` なら `ordered_copy`）を実引数の先へ格納します。そうした関数が無いプログラムでは何も変わりません。問題 2 の `w_opaque_vec`・`w_opaque_dyn`。

指摘ごとの対応（期待は基準の対照と手の計算から。安全なプログラムの値は基準と同じことを確かめました）:

| 問題 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（第9回の退行） | 再現（`r10/min2` は対応前が `10:18` の `E1013`、基準は `50110`。`r10/fx/v2`・`v3`（`Array.map owned_inc [1, 2, 3]`）も同じ。`v1`（ラムダの `Vec` が無い）・`v4`・`v5`（借用した一時値が無い）は対応前も受理） | 呼び出し先から見た型（`Source::ty`）で判定。どれも受理、`min2` は `50110` | `a_value_that_a_callee_passes_on_reads_what_its_type_reads_when_dropped`（edition 2027）、conventions の fixture の `capturing_readers`（`50000`・`50007`、`"hello".length * 10000 + n`。対応前のバイナリはこの fixture の `callback_constructed_once` を `141:18` で拒否） |
| 2（`ref mut` の自己の部分の借用） | 再現（`r10/sm/fn`・`var`・`relay` が受理、`drop -1`。`m5`: 安全な順序でも呼び出しの後の `p0.s = ...` で `drop -1`。この対応で見つけた `w_apply`・`w_opaque_vec`・`w_opaque_dyn` も受理、`drop -1`。`w_pipe`・`w_lambda`・`w_partial`・`w_param`・`w_own`・`w_ownp`・`w_arm`・`w_loop` も対応前は受理、基準は `drop -1`（`w_partial` は `drop 0`）） | 上の 5 つ。危険な順序はどの綴りも、値を宣言したブロック（`w_arm` は腕、`w_own` は `let mut q` のブロック）の始まりの `E1013`、`self_referential_store`。呼び出しの後の借用した部分の書き換え（`m4`・`m5`）は `E1014`、`borrow_conflict`。安全な順序（`ok_*`、`m6`）は受理、値は基準と同じ（`drop 8`・`0`、`ok_w_partial` は `drop 9`・`0`） | `a_borrow_of_a_part_kept_through_an_exclusive_reference_keeps_the_drop_order`（12 の綴りを両方の edition で拒否し、安全な順序の双子を受理。edition 2027 で conventions の綴り・`Vec` から読んだ関数値・書き換えの 2 つの順序）、`in_place_writes` の fixture の `exclusive_parts_in_drop_order`（安全な順序を直接・中継・関数値の仮引数で呼び、どれも記録係が `9 + 2` を読む。`111111111`） |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 自分の部分の共有の借用を持つ値へ、もう一度 `ref mut p0` を渡す呼び出し（`r10/sm/mix4`、安全な順序で `target (ref mut p0)` を 2 回。基準と対応前は受理して `drop 8`）は `E1014` になりました。呼び出し先が借用された部分を書き換えうる（`m5` の書き換えを呼び出し先がする形）ためで、書き込みの事実は「どの場所を書き換えるか」を持たないので区別できません。同じ関数を中継する本体の中（`mix3`: `fn twice p = target p + target p`）は第7回から同じ理由で `E1014` で、それと揃います。`ref p0` を渡す呼び出しは受理されます（`mix5`、`36`）。`drop.md` に書きました。
- 本体の見えない関数値は、プログラムに `ref mut T` の自己の部分の借用を格納する関数があるときだけ、`T` の値へ同じ格納をしうるものとします。型の一致で判定するのは、特殊化したモジュールの型が具体的で、フィールドや要素の `ref mut` は書けない（`ref mut T` を受け取ったコードがその部分の `ref mut` を作って別の関数へ渡すことはできない）ためです。そうした関数がどれも順序を守るなら順序を守るものとし、安全な順序の `ok_w_opaque_vec`・`ok_w_opaque_dyn` を受理のまま保ちました。
- 書き込むセルを自分で所有する関数値を作るプログラム（隠れた環境、`CallFacts::owned_sinks`）では、本体の見えない関数値へ `ref mut p` を渡すと、見えないコードが渡された値の借用を隠れた環境へ残しうるので、`store_hidden` がその環境を持ちうる局所値（`p` 自身を含む）に実引数の loan を残します。そのため、`p` の部分の `drop` が借用を読みうるなら、安全な順序でも `p` の自己 loan（どの部分か分からない）として `E1013` です（`r10/e2e/fx3`: `in_place_writes` の fixture に `Vec` から読んだ関数値の呼び出しを足した形。対応前のバイナリも同じく拒否し、基準は受理します）。第6回からの隠れた環境の規則による安全側の扱いで、この対応では変えていません。`drop.md` に書きました。セルを所有する関数値の無いプログラムでは、上の `self_borrow_types` の規則で受理されます（`ok_w_opaque_vec`）。
- 安全な順序で、ロックの中身（`Mutex<Pair>` のコールバックへ貸した `p`）の部分の借用を中身へ格納する `ok_w_lock` は、対応前の `self_referential_store` から、ロックの中身の借用を残す `mutex_result_retains_borrow`（`locked_borrow_kept`、第8回）に変わりました（どちらも拒否。中身の借用はロックを外した後の書き込みで無効になりうるので、第8回の規則が正しい理由です）。
- 書き戻しとして除く loan を排他の loan に絞ったことは、レビューの 9 回分のプログラム（4496 回の検査）と fixture・examples のどれでも、除いていた loan が排他のものだけだったことを、計装したバイナリで確かめてから決めました（`r10/dbg-corpora.txt`。共有の loan を除いていたのは `r10/sm` の再現だけ）。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-13。ログは `target/scratch/a31/r10/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は、最初の版（`17465441`）で 3048 件中 3047 件成功、1 件失敗（`ownership::retention_tests::store_facts_follow_what_the_bodies_store`: 書き込みの事実を丸ごと比べる単体テストで、期待の `Source` に新しい `ty` が無かった。期待に、本文から見た実引数の型（仮引数 `arr` の宣言の型）を足しました。期待を弱めたのではなく、増えた項目の値を書いたものです）、直した版（`89814a68`）で 3049 件すべて成功、4 件 skip（915 秒、`nextest-2.log`）、`in_place_writes` の fixture に case を足した最終の版でも 3049 件すべて成功、4 件 skip（793 秒、`nextest-3.log`）。第9回の 3046 件から増えた 3 件は `tests/rc.rs` の 2 件と単体テスト `shared_self_borrows_through_exclusive_references_are_facts_of_the_module` です。すべての fixture をビルドする `effects_trap`（9 件）・`effects_alloc_conservative` も成功しています。領域のテスト（`rc`・`mutable_slices`・`currying`・`borrowed_records`・`user_drop`）は 70・19・16・21・27 件すべて成功。
- 新しいテストが対応前に失敗すること: `src/ownership.rs` を `3a9e4733` に戻して `cargo test --test rc` で新しい 2 件を実行し、2 件とも失敗することを確かめました（`fail-before.txt`。退行のテストは `E1013`、穴のテストは最初の綴りの受理で失敗。戻した後は先端に戻しています）。単体テストは新しい事実（`shared_some`・`self_borrow_types`）を読むので、対応前にはコンパイルできません。
- レビューの 9 回分のプログラム（`corpora.sh a`、両方の edition。対応前は `corpora.sh before`）: 第 1 回〜第 9 回の 4348 回の検査は 1 つも変わりませんでした（`diff-before-a.txt`）。第10回のプログラム（`r10/sm`・`r10/sm/g`（A32 の綴りのうち A31 で書けるもの）・`r10/fx`・`r10/min2`、164 回）は、拒否から受理が 3 件（`min2`、`fx/v2`・`v3`。問題 1）、受理から拒否が 38 件（危険な順序のすべての綴りと書き換えの 36 件（基準で実行すると `drop -1` か `drop 0`）、上の判断の `mix4` の両方の edition）、拒否のまま kind が変わったものが 2 件（`ok_w_lock` の両方の edition、上の判断）です。
- 受理される第10回のプログラム 57 回（両方の edition）は、`-O0`・`-O3` の出力がすべて基準と同じです（`vals.sh`、`vals.txt`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じ（`step5-final.txt`。この対応で conventions の fixture に足した `capturing_readers` と `in_place_writes` の fixture に足した `exclusive_parts_in_drop_order` も、基準と同じく受理されます）。
- E2E（`e2e.sh`、`e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `conventions` 39 件（基準のバイナリでも同じ値で成功。対応前のバイナリは fixture の検査で `E1013` になり失敗します）、`in_place_writes` 64 件（この対応で足した `exclusive_parts_in_drop_order` を含む。基準でも 64 件同じ値で成功）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`（701 件）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`）は `Docs: 2 pages, 41 links, 9 checked examples, 18 native runs (O0/O3)` で成功（最後の `drop.md` の追記の後も同じ、`docs-final2.log`）。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。`time-perf.txt`）: レビューの `perf_loop_breaks_400` 147 / 375 / 374、`perf_try_checked_400` 224 / 389 / 399、`perf_ho_wrap_120` 0.55 / 1.73 / 1.71、`perf_match_nest_30` 1.41 / 1.53 / 1.54、第 8 回の `perf_ho_chain_320` 1.58 / 22.7 / 22.7、第 6 回の `drop_locals_600` 3.67 / 5.78 / 5.75、`pattern_param_chain_200` 1.39 / 6.20 / 6.34（どれも対応前とほぼ同じ）。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/drop.md`（1 つの規則と、値の種類ごとの解放の位置の表。`ref mut` の仮引数を通した自分自身の部分、呼び出し先が関数値へ渡す値、もう一度の `ref mut` と、セルを所有する関数値のあるプログラムで本体の見えない関数値へ渡す `ref mut` の制限）、`lifetimes.md`（`ref`・`ref mut` の引数の先の部分の借用を同じ先へ書き込む関数）、`docs/language.md`・`docs/architecture.md`（破棄の順序の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/conventions/Main.tz`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第10回）（2026-11-13）

レビューの 8 件の指摘（反例の探索 `target/scratch/review-a31-ce-r10/`、適合の確認 `target/scratch/review-a31-conf-r10/`）を、対応前の先端（`c2180597`、`r11/bin/tsuzuri-before`）・第9回の前（`2f53734c`）・第9回の終わり（`3a9e4733`）・基準・対応後のバイナリで確かめました。8 件とも指摘のとおりに再現しました。指摘 1 と 4 は同じ原因（第9回の所有の仮引数の規則が持ち上げたラムダの捕捉も含めた）です。どれも「[pre-existing, plain]」ではありません（指摘 1・4 は捕捉が不変なのでロックのコールバックのその場の書き込みでしか起きず、指摘 6 は代入の双子 `ps[0].h = ...` が拒否され、指摘 2・3・5・7・8 は基準が受理する安全な形を A31 の途中の版が拒否した退行か、第10回が受理した危険な形です）。A31 で直し、別のチケット（A33）は作っていません。結果は `target/scratch/a31/r11/` にあります（`r11/m.sh` が 5 つのバイナリの最初の誤りを並べ、`r11/corpora.sh <tag>` がレビューの 10 回分のプログラムの `check --json` を、`r11/cdiff.mjs` がその差を出します。この対応で書いたプログラムは `r11/adv`）。

#### 設計の変更（`src/ownership.rs`）

- **持ち上げたラムダの捕捉は所有の仮引数ではありません**（`Contract::captures`、`Checker::capture_roots`、`owned_parameter`、`store_target`）。捕捉は関数値の環境が所有し、生成されるコードは呼び出しのたびに環境から読んだ捕捉をそのフレームへコピーして使い（`tz.apply.$lambda.N` が `%env` から読み、`$lambda.N` は stack の alloca に置いて破棄しない）、環境が後で破棄します。第9回の `owned_parameter` は `parameter_roots` の値の仮引数をすべて所有の仮引数としたので、`Owned.function` のラムダが値で捕捉した `Mutex` もそうなり、(a) 関数値自身のセルへの格納が環境への格納でなくなり（`Owned.call` に渡した借用が `w` に残らない）、(b) 本文の局所値の借用を捕捉したセルへ格納でき、(c) 捕捉の借用（フレームのコピーを指す）を別の捕捉のセルへ格納できました。捕捉の外部 loan は第9回の前と同じく呼び出し元のデータとして扱い、`store_target` は捕捉のセルを書き込み先にします。指摘 1・4。
- **参照を通して呼ぶ関数値のコードは、自分への参照を受け取りません**（`apply_opaque`）。`Owned.call (ref w) x` の `w` の本体が見えないとき、見えないコードの規則は関数値の値（`ref w` の借用そのもの）を格納しうるものとしていました。関数値のコードが受け取るのは自分の環境で、関数値への参照ではないので、参照を通した関数値（`Arg::lent`）からは環境の中身（`contents_value`）だけを格納しうるものとしました。上の変更で `Owned.function` のラムダが再びセルの所有者になったとき、安全な順序の `of_outer_safe`・`b05c` が `'w' keeps a borrow of a part of itself` になったので入れました。
- **ビューと `Rc`・`Arc` のブロックを部分の順序の規則でたどります**（`self_borrow_dropped_first`）。型はまず `without_views` で読みます。`ref mut [Pair..]` の仮引数の先（ビュー）は何も破棄しないので、以前は要素の段で `drop_reads([Pair..])` が偽になり、順序を証明したことになっていました（指摘 6）。ビューの要素はその後ろの配列の要素なので、配列と同じく順序の分からない段です。`Rc`・`Arc` のブロックへの段は、場所のフィールドがブロックの中の値を直接指すので、中の値へ進みます（ブロックを最後の持ち主が破棄するとき、値の部分は宣言順に破棄されます）。フィールドの無い場所は持ち主自身でありうるので、値全体の規則のままです（指摘 7）。
- **呼び出し先がそのまま渡す引数は呼び出し元の実引数です**（`Source::passed`、`Arg::origin`、`passed_parameter`、`temporary_type`、`value_type`、`temporary_slot`）。`record_higher` は、関数値へ渡す引数が仮引数そのもの（か参照の仮引数の先全体の再借用 `ref *p`）なら、その番号を記録し、`apply_facts_once` は高階の事実を当てはめるとき、呼び出し元の実引数の式を `origin` として渡します。一時値の規則は式か `origin` があればその位置と型で判定し、無ければ値の型（`Source::ty`、呼び出し先から見た型。借用した一時値は一時値への共有参照）から、参照・ビューは参照の先、借用を持たない値はその型、借用を持つほかの値（参照を捕捉したクロージャなど）は分からないもの（`None`、数える）とします。第10回は参照の型そのもので判定したので、`drop_reads(ref Pair)` が偽になり、関数値の仮引数を通した危険な順序を受理していました。指摘 5。
- **値全体の代入は、古い値の自分の部分の借用とともに古い値を解放します**（`replaced_value`）。局所値全体への代入（`p1 = mkp 7`）は新しい値を作った後で古い値を解放するので、古い値が持つ自分の部分の loan をその解放の順序（`borrow_dropped_first`、`Borrowed::Part`）で検査し、逆の順序なら代入の位置で `E1013`（`self_referential_store`）、守られていれば書き込みの競合の検査では古い値からその loan を除きます（ほかの値も持つ loan は競合します）。指摘 3。
- **呼び出し先が借用した部分だけを呼び出し元に残します**（`StoreFacts::self_paths`、`add_self_path`、`self_path`、`path_copy`、`place_type`、`CallFacts::self_borrow_paths`）。呼び出し先が参照の仮引数 `p` の先の部分の借用を同じ先へ格納するとき、`store_external` はその場所（`p` の先からのフィールドの列）を記録し、呼び出し元は共有の複製を、実引数の loan の場所にその列を足した loan にします。列は 4 段まで（長い場所は 4 段目までの部分の借用）、仮引数ごとに 4 つまで（それより多ければ先全体）で、短い列が長い列を覆うので、事実は有限で単調です。ビュー・参照の参照・ビューを通した借用は先全体です。足すのは loan の場所の型が仮引数の先の型と同じとき（`place_type`）だけで、`Array.iterir` のラムダへ渡した要素のように、より大きな値（配列）の loan は全体のままです（最初の版は配列の loan に `[s]` を足して、安全な `iterir_local_safe` を拒否しました）。指摘 3 の `k11`・`kv06`。
- **本体の見えない関数値は、参照の実引数の先へ、プログラムの関数がするのと同じ自分の部分の借用だけを格納します**（`apply_opaque`、`self_borrow_types`）。`CallFacts::self_borrow_types` を `ref T` の仮引数にも広げ（キーは `without_views` した `T`、排他か共有かは区別しません）、`apply_opaque` は参照の実引数ごとに、関数値とほかの実引数が渡すものと、その実引数が自分の借用のほかに持つものを格納し、`T` が `self_borrow_types` に入るときだけ、`self_borrow_paths` の部分の `path_copy`（すべて `ordered` なら `ordered_copy`）を格納します。第9回からは、共有参照の実引数の借用全体を自分自身へ格納していたので、値の部分の解放が読みうるなら安全な順序でも拒否していました。指摘 2・8。

#### 1 つの破棄の順序の規則（表の更新）

第9回の後の表のうち、次の行を変え、足しました。ほかの行は同じです。

| 記憶域 | 破棄の位置と順序 | 規則での位置 | 検査する所 | テスト |
| --- | --- | --- | --- | --- |
| 局所値全体へ代入したときの古い値（この対応） | 代入の位置、新しい値を作った後 | `Part`（古い値の自己 loan） | `replaced_value` | `tests/rc.rs` `replacing_a_whole_value_ends_the_borrows_of_its_own_parts_that_it_keeps`、`in_place_writes` の `whole_replacement_in_drop_order` |
| 持ち上げたラムダの捕捉（`Owned.function` のラムダを含む、この対応） | 関数値の環境とともに（呼び出しはフレームのコピーを使う） | 捕捉は呼び出し元のデータ（`capture_roots`）、捕捉の借用はフレームの借用 | `store_target`・`store_external`（`local_borrow_stored`）、関数値の隠れた環境（`store_hidden`） | `a_function_value_owns_its_captures_and_its_code_works_on_copies_of_them` |
| `ref`・`ref mut` の仮引数の先とその部分 | 呼び出し元の値。先の部分の借用を同じ先へ格納すると、呼び出し元の値のその部分（`self_paths`）の自己 loan | 呼び出し元で `Part` | 第9回の後と同じ、`path_copy`、本体の見えない関数値は `self_borrow_types`・`self_borrow_paths`（`ref T` も） | `unseen_code_keeps_a_borrow_of_a_part_of_a_reference_argument_only_as_a_function_of_the_module_does` |
| ビュー（`ref mut [T..]`）の先（この対応） | その後ろの配列の要素 | `Part`（順序の分からない段） | `self_borrow_dropped_first`（`without_views`） | `a_view_parameter_borrows_the_parts_of_the_array_behind_it` |
| 呼び出し先が関数値へ渡す値 | 仮引数をそのまま渡すなら呼び出し元の実引数の一時値。ほかは分からない | `Temporary`（`origin` の位置）か `Unknown`、型は `temporary_type`（参照・ビューは先、借用を持つほかの値は分からない） | `temporary_dropped_first` ほか | `a_reference_that_a_callee_passes_on_points_at_the_temporary_whose_drop_may_read`、`in_place_writes` の `relayed_temporaries_in_drop_order` |
| `Rc`・`Arc` のブロックの中の値（この対応） | 最後の持ち主の破棄。値の部分は宣言順 | `Part`（ブロックへの段は順序を保つ） | `self_borrow_dropped_first` | `a_borrow_of_a_part_of_a_shared_block_kept_in_the_block_keeps_the_order_of_its_value`、`in_place_writes` の `shared_block_parts_in_drop_order` |

第11回で、この表に `deref p = v` とフィールドへの代入の古い値・借用の先の値が持つもの・`where` の束縛・`for` の変数の行を足し、局所値全体への代入・`Rc`・`Arc` のブロック・呼び出し先が関数値へ渡す値・`ref`・`ref mut` の仮引数の先の行を変えました（下の「レビューの指摘への対応（第11回）」の表）。

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（`Owned.function` の環境が値で持つセルへ `Owned.call` が書き込む借用を追わない）・4（第9回の所有の仮引数の規則が捕捉も含める） | 再現（`b06m`・`b10`〜`b14`、`of_outer_unsafe`・`of_local_into_capture`・`x1_owned_fn_safe`・`order_sm`・`x1_v1`〜`x1_v3` が両方の edition で受理、`-1`・`drop holder reads -1`・`drop 0`・`72`・`drop 4371993424` など。第9回の前のバイナリは拒否） | 捕捉を所有の仮引数から外し、参照を通した関数値の自分への参照を格納しない。`b06m`・`b11`〜`b14`・`of_outer_unsafe` は `E1013`（`assignment_extends_borrow`）、`b10`・`of_local_into_capture`・`x1_*`・`order_sm` は `E1013`（`store_local_borrow_through_reference`）。どれも第9回の前と同じ位置と kind です。安全な順序の `b06c`・`b05c`・`of_outer_safe` は受理、値は基準と同じ | `a_function_value_owns_its_captures_and_its_code_works_on_copies_of_them` |
| 2（共有参照を受け取る本体の見えない関数値で、安全な順序の自己の部分の借用を拒否。第9回からの退行） | 再現（`q05` が両方の edition で、`q01c` が edition 2027 で `self_referential_store`、基準は `drop holder reads 8`） | 見えないコードの参照の実引数の規則。`q05`・`q01c` は受理、値は基準と同じ。危険な順序の `q01`・`q02` と、仮引数全体を捕捉した関数値を格納する `r11/adv/o01` は拒否のまま | `unseen_code_keeps_...`（`q05`・`q01c`・`q04`・`ho_safe` の受理、`q01`・`q02`・`o01`・`o02`・`ho_unsafe` の拒否） |
| 3（自分の部分の借用を持つ値全体の代入と、借用されていない部分の書き換えが `E1014`。第10回までの退行） | 再現（`k12`・`k15` は対応前も第9回の前も `E1014`、`k10`・`kv02`・`k11`・`kv06` は第10回から `E1014`。基準は受理） | `replaced_value` と `self_paths`。`k10`・`k12`・`k15`・`kv02`（edition 2027）・`k11`・`kv06`（edition 2027）は受理、値は基準と同じ。危険な順序の代入（`r11/adv/r01`・`r03`）は代入の位置の `self_referential_store`、代入の後も使う借用（`r02`）と借用された部分の書き換え（`r05`・`r07`・`r08`）は `E1014` | `replacing_a_whole_value_ends_the_borrows_of_its_own_parts_that_it_keeps`、`in_place_writes` の `whole_replacement_in_drop_order` |
| 5（関数値の仮引数を通して渡した一時値の型を参照の型で判定。第10回の退行） | 再現（`tmp_ho_unsafe`・`tmp2_ho_unsafe`・`tmp_ho_elem_unsafe`・`iterir_tmp_unsafe` が両方の edition で受理、`drop -1`・`drop 0`。第9回の終わりのバイナリは拒否） | `origin` と `temporary_type`。4 つとも呼び出しの位置の `E1013`（`assignment_extends_borrow`）。この対応で書いた 2 段の中継 `r11/adv/t02` と引数を入れ替える `t03` も同じ。安全な順序の `tmp2_ho_safe`（第9回の終わりのバイナリは拒否）・`t01`・`t04` は受理 | `a_reference_that_a_callee_passes_on_points_at_the_temporary_whose_drop_may_read`、`in_place_writes` の `relayed_temporaries_in_drop_order` |
| 6（`ref mut [T..]` の仮引数で順序を証明したことになる） | 再現（`slice_va`・`slice_self_unsafe` が両方の edition で受理、`drop -1`。`ref mut [Pair]` の `arr_self_unsafe` は拒否） | `without_views`。どちらも値を宣言したブロックの始まりの `E1013`（`self_referential_store`）。この対応で `slice_vb`（危険な順序）も拒否になりました | `a_view_parameter_borrows_the_parts_of_the_array_behind_it` |
| 7（`Rc` のブロックの中の安全な自己の部分の借用を拒否。第9回からの退行） | 再現（`rc_parts_safe`・`rc_share_safe` が両方の edition で `self_referential_store`、基準は `drop 8`） | ブロックへの段。どちらも受理、値は基準と同じ（`drop 8`）。危険な順序の `rc_parts_unsafe`・`rc_share_unsafe` は拒否のまま | `a_borrow_of_a_part_of_a_shared_block_kept_in_the_block_keeps_the_order_of_its_value`、`in_place_writes` の `shared_block_parts_in_drop_order` |
| 8（関数値の仮引数へ局所値のセルの共有の借用を渡すと、セルの自己の借用になる。第9回からで、記録が無い） | 再現（`ho_safe` が両方の edition で `self_referential_store`、基準は `drop 36`） | 記録するのではなく、指摘 2 と同じ規則で狭めました（`put` のようにセルの型の値へ自分の借用を書き込む関数がプログラムに無ければ、セルは自分の借用を持ちません）。`ho_safe` は受理、値は基準と同じ。`ho_unsafe`・`ho_lambda_unsafe` は拒否のまま。セルへ自分の借用を書き込む関数がある `r11/adv/o02` は拒否 | `unseen_code_keeps_...` |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- 指摘 1・4 の修正で、基準と対応前が受理し、基準でたまたま正しい値（`36`）を出す `of_holder`・`owned_fn_read2`・`x1_call2`・`x1_call2_nodrop`・`of_capture_into_capture_nodrop` も `store_local_borrow_through_reference` になりました。どれも `Owned.function` のラムダが捕捉した値の借用を、捕捉したセルへ書き込む形で、借用は呼び出しのフレームのコピーを指すので、呼び出しが終わった後の読み出しは解放したフレームを読みます（同じ形の `x1_owned_fn_safe` は基準の `-O0` で `drop 4335866736`）。第9回の前のバイナリと同じ扱いです。
- 指摘 6 の修正で、安全な順序の `slice_self_safe_elem`（`Pair { h, s }` を `ref mut arr[0..<1]` で渡す。基準と対応前は受理）も拒否になりました。ビューの要素は配列の要素で、配列の要素は添字を区別しない（第9回からの安全側の扱い。局所値の配列の `arr_elem_safe` も対応前から拒否）ためです。`drop.md` に書きました。
- 指摘 3 の、借用されていない部分の書き換え（`k11`・`kv06`）は、呼び出し先が借用した場所を事実に持たせて受理しました（記録だけにする選択肢より、退行を受理に戻す方を採りました）。場所の数と深さは有限に切り、切ったときは大きな部分の借用（保守的）です。同じ値へもう一度 `ref mut` を渡す呼び出し（`kv01`・`kv04`・`k14`、第10回の `mix4`）は、呼び出し先が借用された部分を書き換えうるので、`E1014` のままです。
- 指摘 2・8 の規則は、本体の見えない関数値が、プログラムのどれかの関数が参照の仮引数を通してする自分の部分の格納だけをしうるものとします（型の一致で判定。特殊化したモジュールの型は具体的で、ビューは `without_views` でそろえ、排他か共有かは区別しません）。`top` の関数があれば、第10回と同じく、どの参照の実引数にも値全体の借用を残します。書き込むセルを自分で所有する関数値を作るプログラムで、見えないコードが隠れた環境へ残す借用（第10回の判断）は変えていません: `in_place_writes` の fixture に `pick_keeper`（実行時に選ぶ関数）の呼び出しを足すと、対応前も対応後も `self_referential_store` です（`r11/fx/unseen`）。同じく、fixture に `Owned.function` のラムダが `Recorder`（`Arc` の共有セルを持つ）を捕捉する形を足すと、モジュールにセルを所有する関数値ができ、既存の 3 つの case が隠れた環境の規則（第6回）で拒否されます（`r11/fx/owned`。対応前も同じ）。そのためこの 2 つは fixture に入れず、Rust のテストで受理を確かめています。
- 期待を変えた既存のテストはありません（書き込みの事実を丸ごと比べる単体テスト `ownership::retention_tests::store_facts_follow_what_the_bodies_store` の `Source` は、新しい `passed` が `None` で、期待のまま一致しました。`passed` が `None` の値は位置の分からない値として扱うので、安全側です）。

確かめたこと（2026-11-13。ログは `target/scratch/a31/r11/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（clippy は最初、新しいテストの要素 1 つの `for` を 2 か所指摘したので、直接の検査にし、`q04` を足しました）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3055 件すべて成功、4 件 skip（953 秒、`nextest-1.log`）。第10回の 3049 件から増えた 6 件は `tests/rc.rs` の新しいテストです。すべての fixture をビルドする `effects_trap`・`effects_alloc_conservative` も成功しています。領域のテスト（`rc`・`mutable_slices`・`currying`・`borrowed_records`・`user_drop`）は 76・19・16・21・27 件、`cargo test --lib ownership` は 20 件すべて成功。
- 新しいテストが対応前に失敗すること: `src/ownership.rs` を `c2180597` に戻して `cargo test --test rc` で新しい 6 件を実行し、6 件とも失敗することを確かめました（`fail-before.summary`。戻した後は先端に戻し、ビルドし直しています）。
- レビューの 10 回分のプログラム（`corpora.sh final`、両方の edition。対応前は第10回の `corpora.sh a`（ソースは `c2180597` と同じ）の結果と、第10回のレビューのプログラムを対応前のバイナリで検査したもの）: 第 1 回〜第 9 回と第10回の後（`r10/sm` ほか）の 4514 回の検査は 1 つも変わりませんでした。第10回のレビューのプログラムと `r11/adv`（516 回）は、拒否から受理が 23 件（`k10`・`k12`・`k15`・`rc_parts_safe`・`rc_share_safe`・`ho_safe`・`q05`・`r11/adv/o04`・`r04` の両方の edition、`k11`・`kv02`・`kv06`・`q01c`・`r11/adv/r06` の edition 2027。すべて安全な形）、受理から拒否が 60 件（安全な `slice_self_safe_elem` の両方の edition（上の判断）のほかは、すべて危険な形。基準で実行すると誤った値か、解放した値を読み、`of_holder` などフレームのコピーを読む形は値がたまたま正しいものがあります）、拒否のまま最初の誤りが変わったものが 10 件（`r11/adv/r01`・`r03` は `E1014` から代入の位置の `self_referential_store` へ、`order_ms`・`x1_owned_fn_unsafe` は第9回の前と同じ `store_local_borrow_through_reference` へ、`cap_owned_outer` は 2 つ目の誤りが無くなった）です（`diff-before-b-r11.txt`）。
- 受理される第10回のレビューのプログラムと `r11/adv` の 106 回（両方の edition）は、`-O0`・`-O3` の出力がすべて基準と同じです（`vals.sh`、`vals.txt`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`compare-2027.txt`・`compare-2026.txt`）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準と同じです（`step5-c.txt`。違いとして出た 1 件は、基準の `matrix_view` の検査が出した一時ディレクトリの削除の警告（`Directory not empty`）で、手で検査し直すと両方とも出力は空で同じでした。この対応で `in_place_writes` の fixture に足した 3 件も、基準と同じく受理されます）。
- E2E（`e2e.sh`、`e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `conventions` 39 件、`in_place_writes` 67 件（この対応で足した 3 件を含む。基準のバイナリでも 67 件同じ値で成功。対応前のバイナリは足した case を `E1014` と `self_referential_store` で拒否して失敗します）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`（701 件）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`）は `Docs: 2 pages, 41 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（既存の `assignment_extends_borrow`・`self_referential_store`・`store_local_borrow_through_reference`・`borrow_conflict` です）。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。`time-perf.txt`）: `perf_loop_breaks_400` 141 / 350 / 356、`perf_try_checked_400` 221 / 371 / 378、`perf_ho_wrap_120` 0.54 / 1.63 / 1.65、`perf_ho_chain_320` 1.49 / 21.8 / 22.5、`drop_locals_600` 3.55 / 5.50 / 5.53、`pattern_param_chain_200` 1.28 / 6.07 / 6.07、`perf_match_nest_30` 1.31 / 1.49 / 1.49、第10回のレビューの `p01_relay_chain_400` 1.26 / 10.3 / 10.9、`p02_relay_chain_400_unsafe` 1.33 / 11.6 / 11.9（どれも対応前とほぼ同じ）。時間の閾値は置きません。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/drop.md`（解放の位置の表に、代入したときの古い値・ラムダが捕捉した値・ビューの先・`Rc`・`Arc` のブロックの中を足し、呼び出し先が関数値へ渡す値を更新。値の自分自身の部分（ビューと `Rc`・`Arc` のブロック）、`ref mut` の仮引数を通した自分自身の部分（借用した部分だけ、本体の見えない関数値の `ref T`）、値全体の代入、ラムダが捕捉した値の段落）、`lifetimes.md`（ラムダが捕捉した値、借用した部分と値全体の代入、本体の見えない関数値）、`docs/language.md`・`docs/architecture.md`（破棄の順序の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### レビューの指摘への対応（第11回）（2026-11-13）

レビューの 10 件の指摘（反例の探索 `target/scratch/review-a31-ce-r11/`、適合の確認 `target/scratch/review-a31-conf-r11/`）を、対応前の先端（`81a639f7`、`r12/bin/tsuzuri-before`）・基準・対応後のバイナリで確かめました。10 件とも指摘のとおりに再現しました（指摘 6 の原因だけは指摘の推測（`path_copy` の `plain` の判定）と違い、指摘 5 と同じ複製の親でした。下の表）。どれも「[pre-existing, plain]」ではありません（指摘 1 は代入の双子 `m = Mutex.create [...]` が対応前に `E1014` で、ロックのコールバックと呼び出し先のその場の書き込みでしか起きず、指摘 2・3・5〜10 は基準が受理する安全な形を A31 が拒否した退行か、A31 の規則の記述漏れで、指摘 4 は文書の漏れです）。A31 で直し、別のチケット（A33）は作っていません。結果は `target/scratch/a31/r12/` にあります（`r12/m.sh` が基準・対応前・対応後の最初の誤りを並べ、`r12/corpora.sh <tag>` がレビューの 11 回分のプログラムの `check --json` を、`r12/cdiff.mjs` がその差を出します。この対応で書いたプログラムは `r12/adv`、Rust のテストは `r12/gen_tests.py` がプログラムから作りました）。

#### 設計の変更（`src/ownership.rs`）

- **借用の先の値が持つものも、借用を持つ値が読みうるものです**（`expand_shares_read`）。破棄の順序の規則（`check_drop_order_of`）は、保持する値が持つ loan のうち、別の局所値の借用について、その局所値が今持つ loan も（推移的に）保持する値のものとして判定します。`drop` は借用を通して先の値の中身（`Mutex` の中身）を読みうるので、`Caller { f: read_cell (ref m) }` を作った後でロックのコールバックや呼び出し先が `m` のセルへ書き込んだ、`Caller` より後に宣言した局所値の借用は、`Caller` の解放の時には解放されています。今までは `Rc`・`Arc` の共有（`Loan::share`）だけをたどっていました。借用の生存（`retain_live_loans`）は以前から推移的なので、保持する値が生きている間は先の値の loan も生きます（`r12/adv/h01`: 書き込みの後の借用元のムーブは `E1014`）。指摘 1。
- **古い値の解放は、古い値自身の部分へ格納しません**（`replaced_root`、`apply_store_within`）。値全体の代入で古い値を解放するとき（`drop_stores`）、古い値が持つ自分の局所値の loan（自分の部分の借用）は、解放される古い値の中を指すので、格納の届く先にしません。今までは `store_target` が局所値の型（セルを持つ）で判定したので、古い値の自己 loan を通して新しい値へ古い値の loan を格納し、代入の後の局所値のムーブ・`ref mut`・フィールドの書き換えが、終わった借用と競合していました。古い値のほかの届く先（共有した `Arc` のブロックなど）は、自己の借用を格納したときに同じ loan を受け取っているので、何も失いません（`r12/adv/d01`: 古い値の `drop` が自分のセルの関数値を外の `Arc` へ移す形は、対応前と同じく `E1014`）。指摘 2。
- **`ref mut` の仮引数の先の丸ごとの置き換え**（`replaced_target`、`State::kept_elsewhere`）。`deref p = v` は呼び出し元の古い値を、新しい値を作った後で解放します。本体がその先へ書き込んだ自分の部分の借用（`State::kept` のうち、根が同じ仮引数で、ほかの仮引数の先や隠れた環境へは書き込んでいないもの）を、`replaced_value` と同じく部分の順序で検査し（逆なら代入の位置の `E1013`、`self_referential_store`）、書き込みの競合の検査の前に `kept` から外します。ほかの先へも書き込んだ借用（`kept_elsewhere`、合流は和）と、本体の局所値が持つ借用は競合のままです。書き込みの事実は変えないので、呼び出し元は共有の複製を持ったままです（安全側）。指摘 3。
- **ほかの部分が借用を持ちえないフィールドの代入は、値全体の代入と同じです**（`replaced_part`、`replaced_field`）。局所値のフィールドの列への代入（`bx.p = mkpo ()`）で、途中のどの段のほかのフィールド・要素も借用を持ちえない型（`drop_part_holds_loans` が偽）なら、局所値が持つ loan はすべてそのフィールドの中にあります。そこで、古いフィールドの値が持つ自分の部分の loan（根が局所値で、場所がフィールドの列で始まるもの）をフィールドの型の部分の順序で検査し、競合の検査から外し、代入の後は局所値が持つものを新しい値の loan に置き換えます（`regions` は `None`、`closed_result` は偽）。要素や共用体の中身への段、借用を持ちうる兄弟がある場合は、今までどおり古い値の loan を残します（`join`）。指摘 9・10。
- **呼び出し先が残した借用の複製は親を持ちません**（`shared_copy`、`path_copy`）。複製は、呼び出し先が実引数の場所の部分の借用を同じ値へ格納したことを表す、その場所そのものの共有の借用です。今までは元の loan の親を引き継いだので、`ref mut` の変数・`ref` の変数・パイプラインの左辺の再借用（親は変数の値全体の借用）を複製すると、値全体の借用が複製とともに生き続け、呼び出しの後の値の読み出し・ほかのフィールドの書き換えが `E1014` でした。新しく書いた `ref mut p0` の loan は親を持たないので、直接の呼び出しだけが受理されていました。変数を通した後の書き込み（`r.s = ...`）、変数をもう一度渡す呼び出し、値のムーブは、複製の場所との重なりで今までどおり `E1014` です（`r12/adv/v01`〜`v08`）。指摘 5・6（指摘 6 の推測の `plain` の判定は真で、原因は指摘 5 の推測どおりの親でした）。
- **同じブロックの別の持ち主を通した借用は、ブロックの部分の借用です**（`State::block_shares`、`note_block_share`、`same_block_loan`）。`let inner = Rc.share (ref rc)`（`Arc.share` も。直接の呼び出しで、実引数が局所値全体の `ref` のときだけ）を束縛・代入した局所値を、`rc` のブロックの持ち主として記録します（束縛し直すと消え、合流は両方の経路で同じものだけを残します。ループの要約にも入れました）。保持する値（`Rc`・`Arc` の局所値）が、同じブロックの持ち主（記録をたどってつながるもの）を根とし、フィールドを持つ場所の loan を持つとき、`check_drop_order_of` はそれを自分の部分の借用（`Borrowed::Part`）として判定し、ブロックの終わりで外のブロックの持ち主が内側の共有の loan を持つ場合（`eval_block`）も、その場で部分の順序を検査して、局所値より長く生きる借用の誤りにはしません。枝によって別のブロックになりうる値（`if n > 0 then Rc.share (ref rc) else Rc.new ...`）、代入し直した共有、型が同じ別のブロックは、今までどおり別の値です（`r12/adv/g01`〜`g06`）。指摘 7。
- **関数値へ渡した仮引数の先の部分は、部分のままです**（`Source::paths`、`Arg::part`、`temporary_dropped_first`）。高階の事実の出どころは、参照の仮引数の loan がすべてその先の部分の共有の借用（`ref p.h`）なら、その部分の場所（`self_path`、`add_self_path` の上限）を記録し、呼び出し元は実引数の値をその場所の `path_copy` にして渡します。一時値を渡したときは、2 つの値が同じ外側の実引数の部分であることを `Arg::part` で示し、一時値の規則は部分の順序で判定します。指摘 8。

#### 1 つの破棄の順序の規則（表の更新）

第10回の表のうち、次の行を変え、足しました。ほかの行は同じです。

| 記憶域 | 破棄の位置と順序 | 規則での位置 | 検査する所 | テスト |
| --- | --- | --- | --- | --- |
| 局所値全体へ代入したときの古い値 | 代入の位置、新しい値を作った後。古い値の自己の部分への格納はしない（この対応） | `Part`（古い値の自己 loan） | `replaced_value`、`replaced_root` | `a_local_replaced_whole_stays_usable_after_the_borrows_of_its_old_parts_end`、`in_place_writes` の `replaced_values_stay_usable` |
| `deref p = v` で置き換えた `ref mut` の仮引数の先の古い値（この対応） | 代入の位置、新しい値を作った後 | `Part`（`kept` の自己の借用） | `replaced_target` | `replacing_the_target_of_a_reference_ends_the_borrows_of_its_own_parts`、同上 |
| フィールドへ代入したときの古いフィールドの値（この対応） | 代入の位置、新しい値を作った後。途中のほかの部分が借用を持ちえないときだけ、局所値が持つものを置き換える。それ以外は古い値の借用を局所値に残す（安全側） | `Part`（フィールドから見た場所） | `replaced_part`、`replaced_field` | `a_part_that_holds_all_that_its_local_holds_is_replaced_like_the_local`、同上 |
| `Rc`・`Arc` のブロックの中の値 | 最後の持ち主の破棄。値の部分は宣言順。`Rc.share (ref rc)` を束縛した局所値を通した借用も同じブロックの部分（この対応） | `Part` | `self_borrow_dropped_first`、`same_block_loan`（`check_drop_order_of`、`eval_block`） | `a_borrow_through_another_owner_of_a_block_borrows_a_part_of_the_block`、`in_place_writes` の `shared_owner_parts_in_drop_order` |
| 呼び出し先が関数値へ渡す値 | 仮引数の先の部分の借用なら、その部分（この対応）。仮引数をそのまま渡すなら呼び出し元の実引数の一時値。ほかは分からない | 部分は `path_copy` の場所、一時値の部分は `Part` | `apply_facts_once`、`temporary_dropped_first` | `parts_of_a_reference_handed_to_a_function_value_stay_parts`、`in_place_writes` の `parts_handed_to_function_values` |
| `ref`・`ref mut` の仮引数の先とその部分 | 呼び出し元の値。`ref mut` の変数・`ref` の変数・パイプラインを通した呼び出しも、借用した部分の共有の借用だけ（この対応） | 呼び出し元で `Part` | `shared_copy`・`path_copy`（親を持たない複製） | `a_reference_variable_or_a_pipeline_leaves_a_shared_borrow_of_the_borrowed_part_only`、`in_place_writes` の `reference_variables_leave_the_part` |
| 借用の先の値が持つもの（この対応） | 借用を持つ値の破棄の位置（先の値の中身を借用を通して読む） | 先の値の loan を保持する値の loan として、その根の位置（`Scoped`） | `expand_shares_read`（`check_drop_order_of`）、生存は `retain_live_loans` | `a_value_that_reads_through_a_borrow_reads_what_the_borrowed_value_holds_when_dropped`、`in_place_writes` の `reads_through_a_borrow_in_drop_order` |
| ガード付き関数の `where` の束縛（記録の追加） | 本体の終わり、宣言の逆順（節の後に書くが、節より先に束縛する） | `Scoped`（宣言の位置） | `check_body` のブロックと同じ `check_drop_order` | 既存（`c01_where_hs` は対応前から拒否、`c01_where_sh` は受理） |
| `for` の変数（`Seq`・`Channel` から受け取る値、記録の追加） | 各回の終わり、本体の局所値の後 | 本体のブロックの外の局所値（本体の局所値の借用は `eval_block` の外の保持者の規則） | `eval_block` | 既存（`fl01`・`fl02`・`a04`・`e01` は対応前から拒否） |

指摘ごとの対応（期待はレビューの対照と手の計算から。安全なプログラムの値は基準と同じことを `-O0`・`-O3` で確かめました）:

| 指摘 | 再現 | 対応 | テスト |
| --- | --- | --- | --- |
| 1（借用を通して読む値が、借用の先へ後で書き込まれた借用を持たない。危険な形の受理） | 再現（`r08_min_unsafe`・`r01`・`r06`・`r07`・`r09`・`r10`・`r12`・`r13`・`r16`・`r17`・`r22` が両方の edition で受理、基準と対応前の `-O0`・`-O3` で `drop reads length 0 2`・`reads -1` など。代入の双子 `r06p` は対応前が `E1014`、基準は受理。`r20` は対応前も隠れた環境の規則で拒否） | `expand_shares_read`。どれも値を宣言したブロックの始まりの `E1013`（`assignment_extends_borrow`）。この対応で書いた借用の借用 `r12/adv/h03`（対応前は受理）も同じ。`r06p`・`r18` は `E1013` に加えて以前の `E1014`。安全な順序の `r08_min_safe`・`r06c` は受理、値は基準と同じ。書き込みの後の借用元のムーブ `h01` は対応前と同じく `E1014` | `a_value_that_reads_through_a_borrow_reads_what_the_borrowed_value_holds_when_dropped`、`in_place_writes` の `reads_through_a_borrow_in_drop_order`（`117`） |
| 2（値全体の代入の後の局所値の使用が `E1014`。第10回の退行） | 再現（`z01`・`z03`・`z04`・`z05`・`w09` が両方の edition で、`z02`・`w13` が edition 2027 で、`w10`・`w12`・`w14` が edition 2026 で `E1014`。基準は受理） | `replaced_root`。どれも受理、値は基準と同じ。古い値の借用を外へ持ち出す形（`w02`、古い値の `drop` が自分のセルの関数値を外の `Arc` へ移す `r12/adv/d01`）は対応前と同じく `E1014` | `a_local_replaced_whole_stays_usable_after_the_borrows_of_its_old_parts_end`、`in_place_writes` の `replaced_values_stay_usable` |
| 3（`deref p = v` の置き換えが `E1014`） | 再現（`w04`・`w03` が両方の edition で `E1014`、局所値の双子 `w06` は受理。基準はどれも受理） | `replaced_target`・`kept_elsewhere`。`w04`・`w03` と、この対応で書いた枝の中の置き換え `r12/adv/d06`・置き換えた後にもう一度書き込んで置き換える `d07` は受理、値は基準と同じ。逆の順序の `d02` は代入の位置の `E1013`（`self_referential_store`、対応前は `E1014`）、ほかの仮引数の先へも書き込んだ `d04`・`w01` と、置き換えの後も使う借用 `d03` は `E1014`、新しい値が古い部分を借用する `d05` は対応前と同じ `self_referential_store` | `replacing_the_target_of_a_reference_ends_the_borrows_of_its_own_parts`、`in_place_writes` の `replaced_values_stay_usable` |
| 4（表に `for` の変数・`where` の束縛・借用を通して読む値の行が無い） | 確認（`fl01`・`fl02`・`a04`・`e01`・`c01_where_hs`・`e09` は対応前も対応後も `E1013`、基準は `drop holder reads -1`。`c01_where_sh` は受理、`8`） | 文書。`drop.md` とこのチケットの表に行と段落を足しました（指摘 1 の行は上の変更とともに）。振る舞いは変えていません | なし（文書。振る舞いはレビューのプログラムで確かめました） |
| 5（`ref mut` の変数・パイプラインを通した呼び出しの後、値全体の排他の借用が残る。第9回の後からの退行） | 再現（`pp7`・`pp6`・`pp1`・`pp2`・`pp3`・`fa8` が両方の edition で、`pp10`・`fa4`・`fa9` が edition 2027 で `E1014`、基準は受理）。原因は、複製が再借用の親（変数の `ref mut p0` の借用）を引き継いだこと | 親を持たない複製。どれも受理、値は基準と同じ。この対応で書いた `r12/adv/v03`（変数を通した `r.n` の書き換え）も受理。変数をもう一度渡す呼び出し・変数を通した借用された部分の書き換え・値のムーブ（`v01`・`v02`・`v04`・`v05`・`v06`・`v07`・`v08`）は `E1014` | `a_reference_variable_or_a_pipeline_leaves_a_shared_borrow_of_the_borrowed_part_only`、`in_place_writes` の `reference_variables_leave_the_part`（`1145`） |
| 6（`ref` の変数・パイプラインで値全体の共有の借用が残る） | 再現（`pp12`・`fa6` が edition 2027 で `E1014`、直接の `fa7` は受理。基準は受理）。原因は指摘 5 と同じ親（`plain` の判定は真でした） | 同上。どちらも受理、値は基準と同じ | 同上 |
| 7（同じブロックの別の持ち主を通した部分の借用が、別の値の借用になる。第9回の前から） | 再現（`rs1`・`rs2`・`rs8`・`rs9` が両方の edition で `57:5`/`58:5` の `E1013`、`rs5`・`rs12s` が `58:13` の `E1013`。基準は `drop 8`） | `block_shares`・`same_block_loan`。どれも受理、値は基準と同じ。この対応で書いた共有の共有 `r12/adv/g07` も受理。危険な順序の `rs3`・`rs4u`・`rs7u`・`rs10u`・`rs11u` は `self_referential_store`（`rs3`・`rs11u` は対応前の `assignment_extends_borrow` から変わりました）、共有の代入・ムーブ（`rsx1`〜`rsx5`）、別のブロック（`g01`・`g04`）、枝によって別のブロックになりうる値（`g05`）、代入し直した共有（`g06`）は `E1013`。パイプラインの `ref rc |> Rc.share`（`g08`）は対応前と同じく拒否（下の判断） | `a_borrow_through_another_owner_of_a_block_borrows_a_part_of_the_block`、`in_place_writes` の `shared_owner_parts_in_drop_order`（`118`） |
| 8（関数値へ渡した仮引数の先の部分が値全体の借用になる。第9回からの退行） | 再現（`ho4`・`ho7` が `self_referential_store`、`ho2` が `assignment_extends_borrow`、両方の edition。基準と第9回の前は受理、`drop 8`） | `Source::paths`・`Arg::part`。どれも受理、値は基準と同じ。危険な順序の `ho1`・`ho3`・`ho5` は拒否のまま。要素の場所（`Array.iterir` が渡す `ref values[index]`）は記録しません（最初の版は `iterir_local_safe` を拒否しました。下の判断） | `parts_of_a_reference_handed_to_a_function_value_stay_parts`、`in_place_writes` の `parts_handed_to_function_values`（`114`） |
| 9（自分の部分の借用を持つフィールドの丸ごとの代入が `E1014`。第9回の前から） | 再現（`rp4` が edition 2027 で `E1014`、`rp5`（局所値全体）は受理。基準は受理） | `replaced_part`・`replaced_field`。受理、値は基準と同じ（`drop 8 2 drop 0 2 1`）。この対応で書いた入れ子のフィールド `r12/adv/f07`・代入の後の値の書き換えと `ref mut` `f10` も受理。危険な順序の `rp3` は代入の位置の `self_referential_store`（対応前は `E1014`）、代入の後も使う借用 `f03` と、入れ子の危険な順序 `f08` は `E1014` | `a_part_that_holds_all_that_its_local_holds_is_replaced_like_the_local`、`in_place_writes` の `replaced_values_stay_usable` |
| 10（フィールドの代入が古いフィールドの値の借用を終わらせない。A31 の規則で安全な形を拒否） | 再現（`fo1`・`fo2` が edition 2027 で `E1013`、局所値の双子 `fo3`・`fo4` は受理。基準は受理） | 同上。どちらも受理、値は基準と同じ。代入の後の借用元のムーブ `r12/adv/f04` も受理。古いフィールドが後の値の借用を持ったまま残る `f09` は `E1013`。兄弟のフィールドが借用を持ちうる `f02` は対応前と同じく `E1013`（下の判断） | 同上 |

判断（どれも「健全性を保ち、受理している安全なプログラムを受理のまま保つ、最も保守的な選択」）:

- どの指摘も「[pre-existing, plain]」ではないので、別のチケット（A33）は作っていません（指摘 1 の代入の双子は対応前に `E1014` で拒否され、ほかの指摘は A31 の規則の退行か漏れです）。
- 指摘 1 の規則は、借用を読みうる `drop` を実行する値が持つ別の局所値の借用を、すべて読み通すものとして数えます（`drop` が本当にその局所値の中身を読むかは見ません。今までの型による判定と同じ安全側の扱いです）。読み通しが新しく拒否するのは、借用の先の局所値が、保持する値より後に宣言した局所値の借用を後から（セルへの書き込みで）受け取る形だけで、セルを通さない代入は借用の競合が先に拒否するので、ほかの安全な形は変わりません（レビューの 11 回分のプログラムと fixture・examples で確かめました）。
- 指摘 2 の修正で、古い値の破棄は、古い値自身の部分（根が置き換えた局所値の loan）へは格納しません。古い値が届くほかの場所は、自分の部分の借用を格納したときに同じ loan を受け取っているので、この省略は健全です（`r12/adv/d01` で、古い値の `drop` が自分のセルの関数値を共有の `Arc` へ移す形が、対応前と同じく `E1014` のままであることを確かめました）。
- 指摘 3 の修正は、本体の検査の中で `kept` から外すだけで、書き込みの事実（`(p, Value, p)` と `shared_some`）は変えません。呼び出し先が先を置き換えた後も、呼び出し元は自分の部分の共有の複製を持つものとします（安全側）。呼び出し元が自分の部分の借用を持つ値へもう一度 `ref mut p0` を作る形（`w05`: `let r = ref mut p0; deref r = mkp 7`）は、第10回の `mix4` と同じく `E1014` のままです。
- 指摘 9・10 は、記録するだけにせず直しました。フィールドの代入が局所値の持つものを置き換えるのは、途中のほかのフィールド・要素がどれも借用を持ちえない型のときだけです（`drop_part_holds_loans`。型変数・未解決の型は持ちうるものとします）。兄弟のフィールドが借用を持ちうる形（`record Box { h: Holder, g: Holder }`、`r12/adv/f02`、基準は受理）は、古い値の借用がどのフィールドにあったか分からないので、今までどおり古いフィールドの値の借用を局所値に残し、`E1013` です。Rust も構造体の借用の期間を変数の解放まで続くものとして同じ形を拒否するので、`drop.md` に制限として書きました。
- 指摘 5・6 の修正で、複製は親を持たないので、呼び出しの後で `ref mut` の変数そのものをムーブしても `move_reborrowed_exclusive_reference` にはなりません。変数を通した後のアクセスのうち、複製した部分に届くもの（`r.s = ...`、`touch r`、`target r` をもう一度）は複製の場所との重なりで `E1014` で、届かないもの（`r.n = 4`）は受理します。
- 指摘 7 で同じブロックの持ち主とみなすのは、`Rc.share (ref x)`・`Arc.share (ref x)` の直接の呼び出し（`x` は局所値全体）を束縛・代入した局所値だけです。パイプライン `ref rc |> Rc.share`（`g08`、基準は受理）は左辺が別の形に下ろされるので認めず、対応前と同じく拒否です（安全側。`drop.md` に書きました）。枝の合流では、両方の経路で同じ記録だけを残します（`g05`: 片方の枝だけで共有になる値は別のブロック）。
- 指摘 8 の場所は、レコード・タプルのフィールドの列だけを記録します。要素・共用体の中身への段（`ELEMENT`・`PAYLOAD`）を含む借用は、どの要素か区別できないので先全体のままです。最初の版はこれを記録し、`Array.iterir` のラムダへ渡す要素の借用（`ref values[index]`）を要素の場所の借用にしたので、ラムダが同じ要素の中で順序を示した（`ordered`）形 `iterir_local_safe`（第10回のレビュー、基準と対応前は受理）を、要素の段（順序の分からない段）で拒否しました（コーパスの比較 `diff-before-a.txt` で見つけ、直しました）。一時値の部分（`Arg::part`）は、出どころが 1 つの仮引数の 1 つの場所のときだけで、ほかは今までどおり位置の分からない値です。
- 期待を変えた既存のテストはありません。

確かめたこと（2026-11-13。ログは `target/scratch/a31/r12/`）:

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` は警告なし（clippy は最初、新しいテストの要素 1 つの `for` を指摘したので、edition 2027 だけで受理する `w13` を足しました）。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は、作業中の版（`a0c60433`）で 3055 件中 3054 件成功、1 件失敗（`tests::examples_and_fixtures_analyze_identically_on_every_thread_count`: 実行中に `in_place_writes` の fixture へ case を足していて、関数名が重複した途中の状態を読んだため。単独で実行し直すと成功）、最終の版（`b224c570`）で 3062 件すべて成功、4 件 skip（1081 秒、`nextest-2.log`）。第10回の 3055 件から増えた 7 件は `tests/rc.rs` の新しいテストです。すべての fixture をビルドする `effects_trap`（9 件）・`effects_alloc_conservative` も成功しています。領域のテスト（`rc`・`mutable_slices`・`currying`・`borrowed_records`・`user_drop`）は 83・19・16・21・27 件すべて成功。
- 新しいテストが対応前に失敗すること: `src/ownership.rs`・`src/ownership_control.rs` を `81a639f7` に戻して `cargo test --test rc` で新しい 7 件を実行し、7 件とも失敗することを確かめました（`fail-before.sh`、`fail-before.log`。戻した後は先端に戻し、ビルドし直しています）。
- レビューの 11 回分のプログラム（`corpora.sh final`、両方の edition。対応前は第10回の `corpora.sh final`（ソースは `81a639f7` と同じ）の結果と、第11回のレビューのプログラムを対応前のバイナリで検査したもの）: 第 1 回〜第 10 回のレビューと第9回の後の確認のプログラムの 5034 回の検査は 1 つも変わりませんでした（`r10` の `min2` は置き場所の名前が変わっただけで、結果は同じです）。第11回のレビューのプログラムと `r12/adv`（762 回）は、拒否から受理が 66 件（すべて安全な形。上の表の受理）、受理から拒否が 23 件（すべて危険な形。指摘 1 の綴り）、拒否のまま最初の誤りが変わったものが 10 件（`d02`・`rp3`・`rs3`・`rs11u` が `self_referential_store` へ、`r06p`・`r18` が `E1013` を先に）です（`diff-before-final.txt`）。最初の版（`corpora.sh a`）は、第10回のレビューの安全な `iterir_local_safe`（両方の edition）を拒否しました（上の判断。最終の版で受理に戻り、第10回のプログラムの変化は 0 件です）。対応前の検査の後に書いた `r12/adv/h01`〜`h03` は表のとおりです（`h03` は対応前が受理、`h01`・`h02` は対応前も拒否）。
- 受理される第11回のレビューのプログラムと `r12/adv` の 219 回（両方の edition）は、`-O0`・`-O3` の出力がすべて基準と同じです（`vals.sh`、`vals.txt`）。
- 154 個の照合（`compare.mjs`）: edition 2027・2026 とも期待と違うもの 0（`compare-2027.txt`・`compare-2026.txt`。`NEW` の行は作業用のディレクトリ `r1`〜`r12` で、第10回と同じです）。
- 手順 5 の比較（`step5.mjs`）: `tests/fixtures/*`・`examples/*` の 154 個の `check --json` と `build --no-cache --emit llvm` が基準とすべて同じです（`step5-final.txt`。この対応で `in_place_writes` の fixture に足した 5 件も、基準と同じく受理されます）。
- E2E（`e2e.sh`、`e2e-final.log`）: `node tests/features.mjs <bin> <suite>` の `conventions` 39 件（基準でも成功）、`in_place_writes` 72 件（この対応で足した 5 件を含む。基準のバイナリでも 72 件同じ値で成功。対応前のバイナリは足した case を `E1014`・`E1013` で拒否して失敗します）、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`iteration_protocol` 26 件（native と WASM の `-O0`・`-O3`）、`sync_types` 14 件・`tzroutine` 16 件（native の `-O0`・`-O3`）、`node tests/control.mjs`（701 件）、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）。すべて成功。
- 文書: `TSUZURI_BIN=<bin> node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`）は `Docs: 2 pages, 42 links, 9 checked examples, 18 native runs (O0/O3)` で成功。`gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`check-cli-docs.mjs` は `4 checks, 0 failed`、`check-guide-lite.mjs` は `88 references checked, 0 failed`、`check-links.mjs --into _features/README.md` は `0 broken`。診断の種類は増やしていません（既存の `assignment_extends_borrow`・`self_referential_store`・`borrow_conflict` です）。
- 性能（`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の `ownership` の相の中央値、ms、基準・対応前・対応後を交互に 3 回。`time-perf.txt`）: `perf_loop_breaks_400` 160 / 413 / 411、`perf_try_checked_400` 238 / 433 / 414、`perf_ho_wrap_120` 0.62 / 2.15 / 2.07、`perf_ho_chain_320` 1.96 / 27.9 / 28.1、`drop_locals_600` 5.04 / 7.01 / 6.89、`pattern_param_chain_200` 1.78 / 7.42 / 7.42、`perf_match_nest_30` 1.47 / 1.66 / 1.73、`p01_relay_chain_400` 1.55 / 12.8 / 13.0、第11回のレビューの `x01_many_holders_800` 756 / 1383 / 1391、`x04_nested_loops_30` 4.39 / 7.65 / 7.48（どれも対応前とほぼ同じ。この回の計測は他の作業と並行したので、第10回より全体に遅く出ています）。時間の閾値は置きません。読み通し（`expand_shares_read`）は、破棄が借用を読みうる値についてだけ行います。

更新した文書: `_tsuzuri/language-reference/ownership-and-memory/drop.md`（解放の位置の表に `where` の束縛・`for` の変数・`deref p = v` の古い値・フィールドの古い値・借用の先の値が持つものを足し、`Rc`・`Arc` のブロックの行に別の持ち主を通した借用を足しました。値全体の代入（代入の後のムーブ、`deref p = v`、フィールドの代入とその制限）、`ref mut` の仮引数を通した自分自身の部分（`ref mut`・`ref` の変数とパイプライン）、値の自分自身の部分（`Rc.share` を通した借用、関数値へ渡した部分）の段落と、借用の先の値が持つもの・`for` の変数と `where` の束縛の段落）、`lifetimes.md`（変数とパイプラインを通した呼び出し、`deref p = v`、借用の先の値が持つもの）、`docs/language.md`・`docs/architecture.md`（破棄の順序の段落）、`_features/README.md`（A31 の行）。

変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`tests/rc.rs`、`tests/features.mjs`、`tests/fixtures/in_place_writes/Main.tz`、上の文書、このチケット。

### マージ前の確認（2026-11-13）

マージの条件（オーケストレーターの決定、2026-11-13）は、(1) 基準（main、`b3dd7f1c`、`target/scratch/base/tsuzuri`）が拒否する危険なプログラムを受理しないこと、(2) このチケットと監査のベースラインの危険なプログラムをすべて拒否すること、(3) 基準が受理する安全なプログラムを、文書に書いた保守的な制限を除いて受理のまま保つことです。基準も受理する穴はマージを止めません。この確認は (1) を、今までのすべてのプログラムで基準と対応後（`8890c1aa` の開発用のビルド、ソースは `b224c570`）を直接比べて確かめたものです。分類の表・道具・結果は `target/scratch/a31/gate/` にあります（表は `relative-soundness.md`）。

- 対象: 中身で重複を除いた 12141 個のプログラムです。`target/scratch/review-a31-*`（第 1〜12 回のレビュー。第 12 回のレビューが足している途中のプログラムも、2 度集めて含めました）、`target/scratch/a31/`（r1〜r12 の探査、`_adv`・`_docs`・`_docs2`・`_twins`、監査の写し）、監査のベースライン `target/scratch/audit-A31`、A32 のレビューと merge のプログラム（`target/wt/a32/target/scratch/review-a32-*` と `a32/` の merge2〜merge4・review1〜4b・baseline・stack）の、Main.tz を持つすべてのディレクトリ（12352 個、重複を除いて 10442 個）と、この領域の Rust のテスト 17 本（`rc`・`mutable_slices`・`currying`・`borrowed_records`・`slices`・`matrix_views`・`concurrency`・`sync_types`・`tzroutine`・`user_drop`・`parameter_conventions`・`types_ownership`・`consuming_update`・`last_use`・`effects_trap`・`effects_alloc_conservative`・`closure_representation`）の中に書いたプログラム（1699 個）です。テストのプログラムは、`analyze_modules_in_edition` に一時的に足した環境変数のフックで書き出しました（フックは元に戻し、コミットしていません）。`tests/fixtures` と `examples` は、各回の step5 の比較で基準と同じ出力です。
- 方法: 各プログラムを両方のバイナリの `check --json` で、edition 2027 と 2026 で検査しました（Tsuzuri.toml を持つプロジェクトは edition を書き換えた写しで検査）。24282 組です。基準が拒否し対応後が受理する組（RA）は、対応後のバイナリで native の `-O0`・`-O3` と AddressSanitizer のビルド（`-O0`・`-O3`。`TSUZURI_CLANG` に置いた `gate/tools/asan-clang.sh` が、プログラムの IR のすべての関数に `sanitize_address` を、C の runtime に `-fsanitize=address` を付け、`ASAN_OPTIONS=detect_stack_use_after_return=1` で実行）で実行し、ソースから手で求めた値と比べました。ASan の手順は、基準が受理する監査の `h1_single_write` で、`-O0` の stack-use-after-return を報告することを確かめてあります。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4085 | 3171 |
| 両方が拒否 | 4785 | 6391 |
| 基準が拒否し対応後が受理（RA） | 14 | 14 |
| 基準が受理し対応後が拒否（AR） | 3257 | 2565 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 28 組（15 個）で、すべて安全です。** 危険なものは 0 個で、マージを止めるものはありません。どれも `-O0`・`-O3`・ASan の `-O0`・`-O3` の出力が同じで、手で求めた値と一致し、ASan は何も報告しません。内訳は、A28 のプログラム全体の除外（プログラムのどこかに追跡しない `Array.write` があると事実も読み通しも使わない）で基準が拒否していた形 14 個（`a31/_docs/t2`・`t8`・`t10`、`a31/_docs2/le_set_ok`・`le_top_write`・`rc_excl_set`・`rc_excl_cb`（2026 だけ。2027 は `Array.map` のラムダの除外で両方とも `E1014`）、監査の `D_ctl_drop_no_loan`、`review-a31-conf/p/d_set`、`tests/currying.rs` の 4 個と `tests/rc.rs` の 1 個。A31 が期待を受理に変えたテストです）と、フィールドの代入が古い値の借用を外す形 1 個（`review-a31-ce-r12/i03_plain_field_replace_safe`、2027 だけ。基準はフィールドの代入の後も古い値の loan を残して `E1014`。値は 41）です。
- この確認で書いた探査（`gate/probes/`、47 個 × 2 edition）: 基準に対する緩め方（フィールドと値全体の代入が古い値の loan を外すこと、A28 のプログラム全体の除外を外したこと）を狙い、古い値を別の場所へ移した後の代入、片方の枝だけの代入、新しい値が同じ借用を捕捉する代入、兄弟のフィールドや同じ `Rc` のブロックや `Mutex` が借用を持つ形、呼び出し先・名前付きのロックのコールバック・3 段の中継・返された書き込み関数・フィールドの書き込み関数・`Array.fill`・`Drop` を通した格納などを書きました。RA は 7 組（`f06`・`f11`・`f21` の 2027、`u11`・`u15` の両方）で、すべて手で求めた値と一致し ASan も何も報告しません。危険な形の受理は 0 個です。基準が受理し対応後が拒否したのは `u03`（名前付きの関数をロックのコールバックにした格納）と `u14`（ロックのコールバックで局所値の借用を格納）で、どちらも基準の穴です。
- AR は 5822 組（3327 個）で、ほとんどはレビューの反例を A31 が塞いだものです（誤りの kind は `assignment_extends_borrow`・`borrow_conflict`・`self_referential_store`・`store_local_borrow_through_reference` など）。条件 (3) は各回のレビューの対応で、その回の前の先端と比べて確かめてきたので、この確認では全部を判定し直していません。基準のバイナリで 479 個を実行し（`-O0`・`-O3`・ASan の `-O0`）、171 個で ASan が誤りを報告しました（ASan が何も報告しないことは安全を示しません。レビューの反例の多くは、ムーブや置き換えの後の、まだ解放されていない値を読みます）。名前が安全を示す AR（`safe`・`ok`・`ctl`・`twin`・`c` で終わる ID）105 組は表の「AR」の節に分けました: 基準の実行で危険と分かったもの 42 組（ASan、または安全な順序の値と違う `-1`・`0` など）、このチケットか文書が保守的な制限として書いているもの 32 組（`a06b`・`a19c`・`a20`、`o01`、`arr_elem_safe`・`slice_self_safe_elem` と同じ要素の形（`d10_view_safe_order` の 9 個）、`ok_w_lock`、基準が代入の双子を同じ誤りで拒否する形（`ctl_scope_rule2`・`sh16`・`sh17`）、セルを所有する関数値の規則（`na05`・`na06`）、ロックのコールバックの規則（`mutex_contents_safe`）など）、判定していないもの 31 組です。判定していないものは、第 12 回のレビューが今調べているプログラム 12 組と、`hid_arm_safe`・`hid_param2_safe`・`hid_tmp_safe`（第 10・11 回の適合の確認と、その A32 の綴り 15 個）19 組です。後者は基準の実行が安全な値を出し、形は `lifetimes.md` の隠れた環境の規則（書き込むセルを自分で所有する関数値はコピーが借用を持たずにセルを共有する）に当たるので、文書に書いた制限だと考えられますが、この確認では確かめていません。
- 両方が拒否し最初の誤りが違うものは 14 組（7 個）で、位置か kind だけの違いです。検査の失敗はありません。
- 変更: 危険な RA が無かったので、コンパイラーもテストも変えていません。別のチケット（A33）に送る穴も、この確認では見つかりませんでした。
- T1（2026-11-13、変更の無い枝の先端で）: `node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast` は 3062 件すべて成功、4 件 skip（1067 秒、`gate/nextest.log`）。

### レビューの指摘への対応（第12回、最終）（2026-11-13）

マージ前の最後のレビュー（第12回。反例の探索 `target/scratch/review-a31-ce-r12/`、適合の確認 `target/scratch/review-a31-conf-r12/`）の 7 件の指摘を、対応前の先端（`2828b752`、`r13/bin/tsuzuri-before`）・基準・対応後のバイナリで確かめました。7 件とも指摘のとおりに再現しました。マージの条件（基準との相対の健全さ、上の「マージ前の確認」）で分けると、指摘 1 は基準が受理する安全な形を拒否する退行（第9回から）、指摘 2・3・5 は基準も受理する穴（第11回の前の先端は拒否していた）、指摘 6・7 は基準が受理する安全な形の拒否（指摘 7 は文書に書いた制限）、指摘 4 は検査の時間です。指摘 1〜6 は小さく局所的な変更で直し（指摘 6 は一部。残りは文書に書いた制限）、指摘 7 と、このチケットで見つかった穴と制限の記録は、後続のチケット [A33](../A33-remaining-store-soundness-holes.md) に送りました。結果は `target/scratch/a31/r13/` にあります（`r13/t.sh` が対応後・対応前・基準の最初の誤りと実行の値を並べ、`r13/p` がこの対応で書いた探査、`r13/gen_tests.py` が Rust のテストをプログラムから作ります）。

#### 設計の変更（`src/ownership.rs`）

- **参照が指しうる値ごとの自己の借用**（指摘 1）: 参照の実引数が複数の loan を持つとき（`if n > 0 then ref a else ref b`、`[ref a, ref b]` の要素、参照の引数のどれかを返す関数の結果）、自己の事実 `(p, Value, p)` の写し（`path_copy`・`shared_copy`）を、すべての loan の場所へ（直積で）書き込んでいたので、`a` が `b.s` の、`b` が `a.s` の借用を持つことになり、宣言の順序で `E1013` でした。参照は実行時にそのどれか 1 つを指し、呼び出し先はその値の部分の借用をその値へ書き込むので、`referent_pieces` が実引数の値を、値自身が持つ loan ごと（内容として持つ loan はどの片にも足す）の片に分け、片ごとに写しを作ってその片の場所へだけ書き込みます（対角）。本体の見えない関数値の、同じ型への自己の借用の書き込み（`apply_opaque`）も同じです。`apply_store` は場所から届く先へ推移的に書き込むので、参照がある値の中の別の値を指すときも、その値へ届きます。借用した演算子（`Arg::reference`）は分けません。
- **`ref mut` で貸した共有はブロックの持ち主でなくなる**（指摘 2）: `access` が、局所値への書き込みと排他借用（`Use::Write`・`Use::MutBorrow`）で、その局所値の `block_shares` の記録と、その局所値を持ち主とする記録を消します（`forget_block_share`）。`renew (ref mut inner)` の本体の `deref r = Rc.new (...)` や `let r = ref mut inner; deref r = ...` の後の `inner` は別のブロックの持ち主でありうるので、`inner` を通した借用は別の値の借用として破棄の順序を検査します（第11回の前の先端と同じ `E1013`）。値全体の代入は、その後で `note_block_share` が新しい値で記録し直します。
- **`deref p = v` が終わらせない借用**（指摘 3・5）: `replaced_target` は、次のどれかのとき `kept` から何も外さず、置き換えは借用と競合して `E1014` です（第11回の前の先端と同じ位置）。
  - 本体が、その仮引数の先の中身（`Part::Contents`、セルの中身の写し）を別の仮引数の先か隠れた環境へ書き込んだ（`State::contents_elsewhere`、`store_external` と `store_hidden_except` で記録。合流は和）。セルの中身の写しは、写した自己の借用の loan を持たない（外部の内容の loan を持つ）ので、`kept_elsewhere` では分かりませんでした（指摘 3、`f14`・`f09`）。
  - 自己の借用を、仮引数の先の外でありうる場所へ書き込んだ: 書き込み先の loan の場所の途中か先の値の中に、`Rc`・`Arc`（弱参照も）のブロック、書き込みの届く参照（`ref mut`、セルに届く `ref`）、dyn 値、型変数がある（`place_leaves_storage`。`store_external` の `leaves` で `kept_elsewhere` に入れます。`Rc` のブロックが呼び出し元のデータへ行った後の書き込み（`ExportTarget::External`）も同じ）。指摘 5 (a)、`f1`・`t05`・`t08`・`t17`・`t18`。
  - 先の型が格納する型の `Drop.drop` が書き込みの事実を持つ（先の型が dyn 値・型変数・`Owned.Function`・tzroutine の本体を格納するなら、プログラムのどれかの `Drop.drop` が持つ）（`replacement_frees_kept`）。古い値の解放がセルの中身をほかの場所へ書き込みうるためで、局所値の置き換えの `drop_stores` の hook に当たるものです。関数値の環境は `Drop` の instance を持つ値を所有できない（`E1005`）ので数えません。指摘 5 (b)、`f1b`・`d01`。
  型だけで判定する案（先の型が `Rc`・`Arc` を格納すれば外さない）は、自己の借用を値の中の自分のセルへ書き込む安全な形（`Recorder` の `out: Arc<...>` を持つ `in_place_writes` の fixture の `replaced_values_stay_usable` など）まで拒否したので、書き込み先の場所で判定しました。
- **`Rc` の代入は、共有を通したブロックの自己の借用も古い値の借用として終わらせる**（指摘 6 の一部）: `replaced_value` は、根がその局所値の loan に加えて、同じブロックの別の持ち主を通した loan（`same_block_loan`）も古い値の自己の借用として扱います。持ち主（`inner`）を内側のブロックで宣言した形（`r03`）は受理し、値は基準と同じです。持ち主がまだスコープにある形（`r06`・`r11`）と、ブロックが自分の部分の借用を持つ `Rc` のムーブ（`r07`・`r10`）は、持ち主が借用を持ち続けるので `E1014` のままです（第11回の前の先端も同じ）。持ち主の借用を、ブロックが生きている間は代入やムーブと競合しないものとして扱えば受理できますが、ブロックの持ち主の組が正しいことに健全さが懸かるので、この回では変えず、`drop.md` に保守的な制限として書き、A33 に送りました。
- **検査の時間**（指摘 4）: `same_block_loan` は、呼ぶたびに `block_shares` の組をすべて走査していました（持ち主ごと・loan ごとに組の大きさの 2 乗）。1 つの共有は 1 つの持ち主を指すので組は木か輪で、`block_groups` が組ごとの代表を 1 度に求め、`same_block_loan` は代表を比べます。残りの時間は、束縛ごとの破棄の hook が連なりを展開する `expand_shares` の `export_block`（型の走査と `exports` の線形の探索）と `store_exported_into`（輸出先ごとの型の走査）だったので、型ごとの走査を覚え（`export_walks`）、輸出先の有無を集合で引き（`exported`）、ブロックの型ごとに 1 度だけ走査します。`gen_chain.py cellshares 800` の所有権の検査は 5009 ms から 501 ms（400 で 121 ms、1600 で 2009 ms。基準は 800 で 86 ms）になりました。残る 2 乗は A33 に記録しました。

#### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（参照が複数の値を指しうると、自己の借用が値どうしの借用になる。基準からの退行） | 再現（`h01`・`h02`・`h03`・`h05`・`h06` が両方の edition と Tsuzuri.toml の 2026 で `36:5`/`38:5` の `E1013`、`h04` は受理。基準は `drop 0 2\|drop 8 2\|0` など） | `referent_pieces`。すべて受理、値は基準と同じ。この対応で書いた探査の、呼び出し先が選ぶ参照（`r13/p/d1u11`）・関数値（`d1u9b`）・内側のブロックの値（`d1u4`・`d1u4b`）も受理。危険な順序（`d1u1`・`d1u7`・`d1u12`・`d1u9`、`self_referential_store`）、別の引数への書き込み（`d1u3`）、先に宣言した保持する値への書き込み（`d1u6`）は `E1013` のまま | `a_reference_to_one_of_several_values_keeps_a_borrow_of_its_own_part_in_each`、`in_place_writes` の `one_of_two_targets_keeps_its_own_part`（`26008`） |
| 2（`ref mut` で書き換えた共有が古いブロックの持ち主のまま。基準も受理） | 再現（`d02`・`d06`・`d08`・`d09`・`d10` が受理し、基準の実行は `drop -1`。対照 `d03`・`d05` は指摘のとおり） | `forget_block_share`。どれも `E1013`（`assignment_extends_borrow`）、対応前の `81a639f7` と同じ位置。片方の枝だけで書き換える `r13/p/d2u1` も `E1013`。対照 `d03`・`d11`・共有の共有 `d2u3` は受理 | `a_share_written_through_an_exclusive_borrow_no_longer_shares_the_block` |
| 3（`deref p = v` が、セルの中身を写した後も自己の借用を終わらせる。基準も受理） | 再現（`f14`・`f09` が受理し `-1`。対照 `f10` は受理） | `contents_elsewhere`。どちらも `deref` の位置の `E1014`。写しを隠れた環境へ書き込む `r13/p/f5u3`（2027 は対応前に受理、基準の `-O0` は SIGSEGV）も `E1014`。局所値へ写して後で外へ写す `f3u1` は対応前と同じ `E1014`。置き換えの後に新しい値の中身を写す `f3u2` と `f10` は受理 | `replacing_the_target_of_a_reference_ends_no_borrow_that_a_copy_or_a_shared_block_keeps` |
| 4（`Rc.share` の連なりで検査の時間が 3 乗） | 再現（800 で 5009 ms、基準 86 ms） | `block_groups`、`export_walks`・`exported`、`store_exported_into` の型ごとの走査。800 で 501 ms。2 乗が残る（A33） | なし（CI に速度の合否は入れない） |
| 5（`deref p = v` が、共有のブロックに残る自己の借用や、古い値の `Drop.drop` が写す借用を終わらせる。基準も受理） | 再現（`min/f1` が `-1`、`min/f1b` が `1`、`p/t05`・`t08`・`t17`・`t18`・`d01` が `read -1`） | `place_leaves_storage`・`replacement_frees_kept`。どれも `deref` の位置の `E1014`。値の中に `Arc` があっても自己の借用を自分のセルへ書き込む `r13/p/f5u2` と、第11回の安全な形 `w03`・`w04`・`r12/adv/d06`・`d07` は受理、値は基準と同じ | 同上、`in_place_writes` の `replaced_target_with_an_arc_drops_its_parts`（`153`） |
| 6（共有を通したブロックの自己の借用を持つ `Rc` を代入・ムーブできない。基準からの退行で、文書に無い） | 再現（`r03`・`r06`・`r07`・`r11` が `E1014`、`r10` も。`r09` は受理。第11回の前の `81a639f7` も拒否） | `replaced_value` の `same_block_loan`。`r03` は受理（`drop 8 2\|drop 0 2\|1`、基準と同じ）。`r06`・`r07`・`r10`・`r11` は `E1014` のままで、`drop.md` の「値全体の代入」に制限として書いた（A33） | `replacing_an_rc_ends_the_borrows_that_its_block_keeps_through_a_share`（`r06`・`r07` は制限として拒否を固定）、`in_place_writes` の `replaced_rc_drops_its_block_parts`（`182`） |
| 7（2 つの参照の仮引数の先の部分を関数値へ渡すと一時値の順序が分からない。文書に書いた制限） | 再現（`h01` が `104:12` の `E1013`、基準は `drop 8 2 drop 0 2`。危険な双子 `h02`・`h03` は拒否） | 変えない。`drop.md` の「呼び出し先が関数値へ渡す値」にこの形を例として足し、A33 に記録した | なし |

#### 判断

- 指摘 1 の対角の書き込みは、参照の値が持つ loan のどれかが参照の指す場所（かその場所を含む値）であることに基づきます。内容として持つ loan（`Value::through`）は、どれが指す先か分からないので、どの片にも足しました（今までどおりの直積）。借用した演算子（edition 2027 の読み取りの仮引数）は、演算子の値（場所が持つもの）と借用（`Arg::reference`）が別なので分けていません。
- 指摘 2 は、指摘の案のとおり局所値全体の排他借用で記録を消すだけでなく、局所値を根とする書き込みと排他借用のすべてで消しました（どちらも保守的な側）。
- 指摘 3 と 5 は、指摘の案（3: 仮引数の中身を別の先へ書き込んだら外さない。5: 先の型が `Rc`・`Arc` に届くか、`drop` が書き込みうるなら外さない）を、5 (a) だけ型でなく書き込み先の場所で判定する形にしました（上の「設計の変更」）。5 (b) の `drop` の判定は、`drops_write`（どれかの `Drop.drop` がロックのコールバックを書くだけで真になる構文の近似）では第11回の安全な形がすべて拒否になったので、`Drop.drop` の書き込みの事実（`StoreFacts`）で判定しました。
- 指摘 6 の残り（持ち主がスコープにある間の代入、ムーブ）と指摘 7 は、マージの条件の「文書に書いた保守的な制限」として残し、A33 に送りました。

#### マージの条件の確かめ（この回の変更の後）

上の「マージ前の確認」の道具（`target/scratch/a31/gate/`）で、この回の変更の後のビルド（`gate/bin/tsuzuri-r13`）を、すべてのプログラムで基準と比べ直しました（`gate/tools/check-r13.mjs` が基準の結果を `results.json` から使い、新しいプログラムだけ基準でも検査します。結果は `results-r13.json`、分類は `gate/tools/diff-r13.mjs` の `diff-r13.txt`）。対象は、前の確認のプログラムに第12回のレビューの残りとこの回の探査（`r13/p`）を足した 12167 個（コーパス 10467 個、テストのプログラム 1700 個）、24334 組です。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4083 | 3169 |
| 両方が拒否 | 4788 | 6394 |
| 基準が拒否し対応後が受理（RA） | 14 | 14 |
| 基準が受理し対応後が拒否（AR） | 3282 | 2590 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は前の確認と同じ 28 組（15 個）で、新しい RA はありません。** 条件 (1) はこの回の変更の後も成り立ちます。
- 対応前の先端（前の確認の `tsuzuri-branch`）からの変化は 57 組です。受理から拒否が 36 組で、どれも第12回のレビューの危険な形（指摘 2 の `d02`・`d06`・`d08`・`d09`・`d10`（と Tsuzuri.toml の綴り）、指摘 3 の `f08`・`f09`・`f14`（と綴り）、指摘 5 の `min/f1`・`min/f1b`・`p/d01`・`t03`・`t05`・`t08`・`t17`・`t18`）です。`t03` と `f08` は基準の実行が正しい値（`read 8`、`8`）を出しますが、`t03` は `t05` からごみの確保を除いただけの形、`f08` は `f09`・`f14` と同じく解放した文字列を読む形で、解放したメモリがたまたま書き換わっていないだけです（基準の ASan の `-O0` のビルドも何も報告しませんでした。前の確認のとおり、ASan が何も報告しないことは安全を示しません）。拒否から受理が 14 組で、指摘 1 の `h01`・`h02`（と綴り）・`h03`・`h05`・`h06` と指摘 6 の `r03` の両方の edition、どれも安全な形で値は基準と同じです。拒否のまま最初の誤りが変わったのが 7 組（`s02_cond_wait_with_selfborrow` とその A32 の綴り 4 個が `assignment_extends_borrow` から `mutex_result_retains_borrow` へ（対角の書き込みで、保持する値どうしの借用が無くなり、次の誤りが先に出る）、`f05` が `E1013` から `deref` の位置の `E1014` へ）です。
- 基準も対応後も受理するプログラムのうち、名前が危険を示す 16 個（`fs1_sh_unsafe` とその綴り、`g04_compose_leak`、`v17_display_bad_forced`、`arc_moved_unsafe`、`view_tuple_unsafe`、`depth_u`、`oc1_unsafe`、A32 の `u02`・`r2_u02`）を実行して読みました。`v17` のほかは安全です（パターンの変数は束縛の逆順に解放され、捕捉は写しで、借用した場所は書き換えられない）。`v17`（`Display` の instance が仮引数の部分の借用を呼び出し元のセルへ書き込み、文字列の補間で表示する）は、補間の暗黙の呼び出しに事実を当てはめていませんが、実行では書き込みが見えず、解放後の読み出しを観測できなかったので、穴かどうかを決め切れません（直接の呼び出し `r13/p/v17c` は A31 が拒否し、基準は不定の値）。A33 に調べる項目として記録しました。

#### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3066 件すべて成功、4 件 skip（1296 秒、`r13/nextest.log`。`rc` 87 件、`effects_trap`・`effects_alloc_conservative` 10 件を含む）。その後で、1 つの要素だけを受理する新しいテストの `for` を直接の呼び出しに書き換え（clippy の `single_element_loop`）、`tests/rc.rs` の新しい 4 件をもう一度実行して成功しました。
- 新しいテストの 4 件は、対応前のバイナリでは失敗する形です（`r13/t.sh` の `before` の列: 受理する `h01`〜`h06`・`d1u*` の安全な形と `r03` を対応前は拒否し、拒否する `d02`〜`d10`・`f14`・`f09`・`min/f1`・`min/f1b`・`t05`・`t08`・`t17`・`t18`・`d01`・`f5u3`（2027）を対応前は受理）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`。`sync_types`・`tzroutine` は native）: `in_place_writes` 75 件（この回の 3 件を含む。基準のバイナリでも 75 件が成功し、対応前は `one_of_two_targets_keeps_its_own_part` を拒否して失敗）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`slices` 11 件、`matrix_view` 303 件、`concurrency` 15 件、`sync_types` 14 件、`tzroutine` 16 件、`node tests/user_drop.mjs`（`-O0`・`-O3`）、`node tests/control.mjs`（701 件）がすべて成功（`r13/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`、基準と対応後の `check --json`・`build --no-cache --emit llvm`）: `tests/fixtures`・`examples` の 154 個のどれも出力が同じ（`r13/step5.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・45 個のリンク・9 個の例・18 回の native の実行）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功。
- 検査の時間（`r13/perf.sh`、所有権の検査の中央値、3 回、ms。基準・対応前・対応後）: 前の回の性能の表のプログラムは対応前とほぼ同じ（`perf_loop_breaks_400` 148・375・399、`perf_try_checked_400` 224・388・410、`perf_ho_chain_320` 1.8・24.7・25.1、`p01_relay_chain_400` 1.5・12.5・15.0、`x01_many_holders_800` 704・1276・1287、ほかは 8 ms 以下）で、`p_cellshares_800` は 87・5332・469 です（`r13/time-perf.txt`）。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（4 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（3 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、`_features/README.md`（A31 の行と A33 の行）、このチケット（「残る制限」と、この節）、新しい `_features/A33-remaining-store-soundness-holes.md`。

### マージ前の追加の修正（2026-11-13、A32 の 5 度目の merge で見つかった誤った拒否）

A32 の 5 度目の merge（A31 の先端 `da4e5b85` を A32 へ）が、`main` が受理する安全なプログラムを A31 が拒否し、文書にも書いていないことを見つけました（マージの条件 (3) に反します）。名前付き関数 `cross a b` が `b.s` の借用を `a.h` の `Mutex` へ書き込み（`let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c)`）、`p1` を先に宣言して `cross (ref p2) (ref p1)` と呼ぶ形です（`target/wt/a32/target/scratch/a32/merge5/s01/red/p/loc_direct`）。`p2` が先に解放され、その `drop` が読む `p1.s` はまだあるので安全で、基準は `drop 8`・`drop 0`・`1` を出します。A31 は `E1013`（`self_referential_store`、`'p1' keeps a borrow of a part of itself`、注は `ref p1`）でした。関数値の仮引数・ラムダ・規約の綴り（`loc_safe__*`）も同じで、一時値の形（`loc_direct_tmp`）は受理、逆の呼び出し（`loc_unsafe__*`、基準は `drop 0`・`drop -1`）は正しく `assignment_extends_borrow` でした。結果は `target/scratch/a31/r14/` にあります（`r14/p` が A32 の再現の写し、`r14/q` と `r14/t` がこの修正で書いた探査、`r14/v.sh` が修正後・A31 の先端（`r14/tip-target` でビルド）・基準の最初の誤りを並べます）。

#### 原因

自己の事実ではありませんでした。`cross` の書き込みの事実は `(1, Value, 0)` だけです（引数 1 の値の借用が引数 0 の書き込み先へ）。呼び出し元はこの事実で `p1` 全体の借用（`ref p1` の loan）を `p2` に残します。`settle` は事実を変化が無くなるまで当てはめ直し、2 回目の `apply_store_within` は書き込み先 `p2` が持つ loan をたどります。`store_target` は共有の loan を根の宣言の型で判定するので、`PairOk`（`Mutex` を持つ）全体の借用は書き込み先になり、2 回目の格納は `p1` 自身へ届いて、`p1` が自分全体の借用を持つことになりました（`check_drop_order_of` の自己の借用の検査）。呼び出し先が実際に書き込んだのは `b.s`（文字列）の借用で、そこを通してほかの値へ届くことはありません。同じ理由で、2 つの値のどちらかを指す参照（`cross (ref p2) (if n > 0 then ref p1 else ref p2)`）は `p1` が後に宣言した `p2` の借用を持つことになって `assignment_extends_borrow`、同じ値を 2 回渡す安全な `cross (ref p) (ref p)`（`PairOk { h, s }`）と、呼び出しの後の借用されていない部分の書き換え（`p1.h = ...`）も拒否でした。

#### 変えたこと（`src/ownership.rs`）

- **`StoreFacts::kept_paths`**: 参照の仮引数 `p` の先の借用をほかの仮引数 `q` の先へ格納した組 `(p, Value, q)` ごとに、借用した場所（`p` の先からのフィールドの列。`self_paths` と同じく `self_path` が求め、4 段まで、4 つまで）を記録します。合流は和で、片方が場所の無い組を持てば先全体（`[]`）です。
- **借用の場所がその借用した部分であること（`exact_loan`）**: region が 1 つの仮引数の外部の loan は、仮引数の先と、先が持つ参照の先の両方を表すので、`ref w.p.s`（`record Wrap {r} { p: ref {r} PairOk }`）の場所は `w` の外部の根に `PairOk` のフィールド `s` を足したものになり、`Wrap` の部分としては誤りです。呼び出しの結果の参照を通した借用（`ref (geth q).n`）も同じです。そこで借用の式が局所値か、loan がどれも場所そのもの（フィールドの無い loan か exact な loan）で先の型と同じ参照の局所値（仮引数を含む）の先の、フィールドと要素の列であるとき（`exact_place`。値はそれ自身の型の部分を持たないので、その参照は場所そのものを指します）だけ、その借用を `Checker::exact_loans` に入れます。`kept_paths` はこの借用のときだけ場所の列を記録し、ほかは先全体です。
- **呼び出し元の写し（`kept_copy`）**: `ref` の仮引数の組と、`shared_only` の組へ新しく書いた `ref mut` を渡したとき、実引数の loan のうち場所が借用した部分で場所の型が仮引数の先の型と同じもの（`path_copy` と同じ条件）を、その列を足した共有の loan にします。写しは元の loan の parents を保ち（以前に残していた元の loan と同じく、借用した値が持つものを生かし続けます）、exact です。それ以外の loan は全体のままです。
- **`store_target`**: exact で、場所の型が何も格納できない（`hidden_type` が `(false, false, false)`、型変数を含まない）共有の loan は書き込み先にしません。文字列の借用を通して格納は届きません。

これで `cross (ref p2) (ref p1)` は `p2` に `p1.s` の借用だけを残し、2 回目の当てはめは `p1` へ届かず、受理します。同じ値を 2 回渡すと `p` が `p.s` の借用を持つので、部分の順序で判断します（`PairOk` は受理、`PairBad { s, h }` は `self_referential_store`）。自己の事実（`self_paths`・`path_copy`・`referent_pieces`）は変えていません。

#### 確かめた形（`r14/v.sh`、両方の edition）

| 形 | 修正後 | A31 の先端 | 基準 |
| --- | --- | --- | --- |
| `cross (ref p2) (ref p1)` と、パイプライン・部分適用・関数値の仮引数（`apply2`、型変数の `apply2g`）・ラムダ・仮引数をそのまま渡す関数・参照の局所値・参照を返す関数・配列の要素・`Rc.get`・`ref mut` の仮引数（`crossm (ref p2) (ref mut p1)`、`ref mut` の変数）・`loc_safe__*` | 受理 | `E1013` | `drop 8`・`drop 0`・`1` |
| 2 つの値のどちらかを指す参照（`if n > 0 then ref p1 else ref p2`、`pick`） | 受理 | `E1013` | 同上 |
| 同じ値（`cross (ref p) (ref p)`）、1 つの値の 2 つの部分（`cross (ref t.a) (ref t.b)`、`a` が先） | 受理 | `E1013` | `drop 8`・`1`、`drop 8`・`drop 0`・`1` |
| 呼び出しの後の借用されていない部分の書き換え（`p1.h = ...`、edition 2027） | 受理 | `E1014` | `drop 0`・`drop 8`・`drop 5`・`1` |
| 逆の呼び出し（直接・パイプライン・関数値・型変数・仮引数を渡す関数・`ref mut`・`loc_unsafe__*`）、後に宣言した値を指しうる `pick` | `E1013`（`assignment_extends_borrow`） | 同じ | `drop 0`・`drop -1`（解放した文字列を読む） |
| `PairBad` の同じ値、`PairBad` の値を指しうる参照、2 つの部分の逆の順序（`TwoBad { b, a }`） | `E1013`（`self_referential_store`） | 同じ（`PairBad` の参照は `assignment_extends_borrow`） | `drop -1`・`1` など |
| 呼び出しの後の借用された部分の書き換え・ムーブ | `E1014` | 同じ | `drop -1` など |
| 参照のフィールドの先の部分（`crossw (ref p2) (ref w)`、`cross a w.p`） | `E1013`（`assignment_extends_borrow`、安全側の制限） | 同じ | `drop 8`・`drop 0`・`1` |

最後の行は残る制限です（基準が受理する安全な形の拒否で、A31 の先端も同じ）。`drop.md`（「解放の順序」の新しい項目「別の仮引数の先へ書き込んだ部分」）、`lifetimes.md` の「現在の制限」、上の「残る制限」と A33 の表に書きました。A32 の記録した `tmp_safe__alias`（`def` の右辺のラムダの注釈した引数を関数値へ渡す一時値の形）は、この修正の前と同じ拒否で、A32 が A33 に記録した制限です。

#### テスト

- `tests/rc.rs` の `a_call_that_stores_a_part_of_one_reference_argument_in_another_keeps_a_borrow_of_that_part`（プログラムは `CROSS_PRELUDE` と本体）: 受理 11 形（直接・パイプライン・部分適用・関数値・型変数・ラムダ・仮引数を渡す関数・どちらかを指す参照・同じ値・2 つの部分・`ref mut`）を両方の edition で、`p1.h` の書き換えの受理と `p1.s` の書き換えの `38:5` の `E1014`（`borrow_conflict`）を edition 2027 で、拒否 10 形（逆の呼び出し 6 綴り、`PairBad` の同じ値、`PairBad` を指しうる参照、`TwoBad`、制限の `crossw`）を両方の edition で `35:5` の `E1013` と kind で確かめます。どの受理も A31 の先端では拒否です。
- `in_place_writes` の `crossed_references_keep_the_part`（native と WASM の `-O0`・`-O3`）: 関数値を通した `p2` への `p1.s`（`"12345"`、5 + 1）、パイプラインの同じ値 `p3`（`"123"`、3 + 1）、どちらかを指す参照の `p4`（`k = 1` なので `p1.s`、6）、`p1` は 0、関数値の結果 1 で、手で求めた値は `1 * 10000 + 0 * 1000 + 6 * 100 + 4 * 10 + 6 = 10646` です（基準の `-O0`・`-O3` も同じ値）。

#### マージの条件の確かめ（この修正の後）

- 上の道具を `gate/tools/check-r14.mjs`・`diff-r14.mjs` に写し（基準と第12回の対応の後の結果を `results-r13.json` から使い、`gate/bin/tsuzuri-r14` だけを検査します）、12167 個・24334 組を両方の edition で検査し直しました（`results-r14.json`・`diff-r14.txt`）。分類は第12回の表と同じです（2027: 両方が受理 4083、両方が拒否 4788、RA 14、AR 3282。2026: 3169、6394、14、2590。検査の失敗 0）。RA は同じ 28 組（15 個）で、新しい RA はありません。第12回の対応の後のバイナリ（`r13`）からの判定の変化は 0 組です（このコーパスには、ある参照の実引数の部分を別の参照の実引数の先へ書き込む呼び出しの安全な形がありませんでした）。
- そこで、この形を A32 の再現の写しとこの修正の探査（`r14/p`・`r14/q`・`r14/t`、88 個）で両方の edition で比べました（`r14/v-final.txt`・`v-final-2026.txt`・`v-final-rest.txt`）。基準が拒否して修正後が受理するものはありません（基準が拒否するのは A32 の注釈の綴り `*__annot*` の型の誤りなど 11 個で、修正後も同じ誤り）。A31 の先端からの変化は 2027 で 36 個、2026 で 31 個で、拒否から受理が 35 個と 30 個（上の表の受理の行。どれも基準が受理する安全な形）、拒否のまま kind が変わったのが 1 個（`n_several_bad`: `PairBad` の値を指しうる参照が、`p1` に後の `q2` の借用を持たせる `assignment_extends_borrow` から、`q2` 自身の部分の借用の `self_referential_store` へ。値を指しうる参照の片方が `q2` 自身なので、こちらが正しい理由です）です。受理から拒否はありません。
- 受理に変わった形と一時値の形の 41 個（2027）と 30 個（2026）を、修正後のバイナリで native の `-O0`・`-O3` と ASan（`gate/tools/asan-clang.sh`、`-O0`・`-O3`）で実行し（`gate/runs-r14-probes.json`）、どれも手で求めた値（`drop 8`・`drop 0`・`1`、同じ値は `drop 8`・`1`、`p1.h` の書き換えは `drop 0`・`drop 8`・`drop 5`・`1`、3 つの値は `drop 8`・`drop 0`・`drop 0`・`1`、`in_place_writes` の形は `10646`）を出し、ASan は何も報告しませんでした。

#### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3067 件すべて成功、4 件 skip（1788 秒、`r14/nextest-2.log`）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 76 件（この修正の 1 件を含む。基準のバイナリでも 76 件が成功）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件がすべて成功（`r14/suites.log`・`e2e-base.log`）。
- 手順 5 の比較（`_tools/step5.mjs`、基準と修正後の `check --json`・`build --no-cache --emit llvm`）: `tests/fixtures`・`examples` の 154 個のどれも出力が同じ（`r14/step5.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・47 個のリンク・9 個の例・18 回の native の実行）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（1 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（制限の表の `crossw` の行、`loc_direct` を直したこと）、このチケット（「残る制限」と、この節）。

### マージ前の追加の修正の見直し（2026-11-13、前の修正のレビューの指摘）

上の「マージ前の追加の修正」（`f0003aa6`）のレビュー（反例の探索 `target/scratch/a31/r14-review/ce/`、適合の確認 `target/scratch/a31/r14-review/conf/`）の 6 件の指摘を、修正後・A31 の先端（`da4e5b85`）・基準のバイナリで確かめ、どれも指摘のとおりに再現しました。指摘 1 は基準からの退行（基準が別の理由で拒否する危険な形を受理）、指摘 2 は A31 の先端からの退行（基準も受理する危険な形を受理）、指摘 3 は基準も先端も受理する穴、指摘 4・5 は基準が受理する安全な形の拒否（指摘 5 は修正が直したと書いた形の残り）、指摘 6 は文書です。結果は `target/scratch/a31/r15/` にあります（`r15/v4.sh` が修正後・前の修正（`r15/r14-target` でビルド）・先端・基準の最初の誤りを両方の edition で並べ、`r15/all-v4.tsv` がレビューと探査の 369 個の結果、`r15/p` がこの見直しで書いた探査です）。

#### 原因

前の修正は、loan の場所がその借用した部分であること（`exact_loan`）を loan だけの性質とし、フィールドの無い loan はどれもそうだとしました。しかし、呼び出しの結果は実引数の loan を持ちながら、その場所から届く別の値を指しえます。`record Node { kids: [Rc<Node>], s: string }` の `fn child n = Rc.get (ref n.kids[0])` の結果は `n` の loan（`(n, [])`）を持ち、`n` の子という同じ型の別の値を指します。そのため次の 3 か所が誤りでした。

- 呼び出し先の `exact_place`: `let q = child b; ref q.s` の場所 `(b, [s])` を `b.s` の借用とし、`kept_paths` に `[s]` を記録した（指摘 1・2 の呼び出し先の側）。
- 呼び出し元の `kept_copy`: `crossn (ref p) (child (ref p))` の実引数の loan `(p, [])` を `p.s` に狭めた。仮引数を渡す `viachild a b = crossn a (child b)` は、その写しを exact として本体の事実へ伝えた（指摘 2）。
- 自己の事実の `self_path`・`ordered`: exact かどうかを見ずに場所を記録した（指摘 3。第12回の対応からの穴で、基準も受理する）。

指摘 4 と 5 は前からの不精確さです。部分適用が束縛した引数はどれも関数値全体の値を持ち（`bound`、パイプラインの右辺も同じ）、呼び出し先が関数値へ渡す仮引数の先の部分の値は `path_copy` の写しで exact でなく（フィールド）、要素の場所は `Source::paths` に入りませんでした。

#### 変えたこと（`src/ownership.rs`、`src/ownership_control.rs`）

- **参照の値が指す場所（`Checker::points_at`）**: 参照の値が、持つ loan のどれかの場所そのものを指す（その場所から届く別の値ではない）かを、値の作り方で決めます。場所の借用（`exact_place`）、不変の参照の仮引数、そうした式に束縛した不変の局所値とパターンの変数（`Checker::pointing`: 束縛したときの loan だけを今も持つもの。`match` の対象はそれが別名にした局所値。注釈した lambda の仮引数 `\(q: ref mut Pair) -> ...` もこれで、`note_pointing`）、`if`・`match`・束縛の無いブロックの選択、`Rc.get`・`Arc.get`、本体の結果が参照の仮引数かその選択である関数（`returned_parameters`: `pick k x y = if k > 0 then x else y`、`id_ref x = x`）の完全な呼び出しで、引数がそうであるものです。ほかの呼び出しの結果は指す場所が分からないものとします。
- **`exact_place`**: 参照の局所値の先は、その局所値が `pointing` のときだけ exact です。`ref (deref r)`（パイプラインの左辺、`if` の参照の実引数）は `r` が `points_at` のとき exact です。exact な借用の loan は、場所の型（`place_type`）が借用した値の型と同じときだけ `exact_loans` に入れます。
- **実引数（`argument_points`）**: 式の値が式自身のものなら式で決め（借用した演算子は場所が exact なとき）、そうでない引数（部分適用が束縛した引数、パイプラインの右辺が適用する引数 `Arg::approximate`、呼び出し先の事実が作る式の無い引数）は `Arg::pointing` が持ちます。`Source::pointing` は、関数値の仮引数へ渡した値の loan がどれも仮引数の先全体か `paths`・`element_paths` の部分で、引数の式が `points_at` であることで、呼び出し元はそうした実引数から作った値を `pointing` にします。
- **写し**: `kept_copy` は `points_at` の実引数だけを狭めます。`path_copy` の写しは、実引数が `points_at` で元の loan が exact なときだけ exact で、そうでない写しは元の loan を親に持ちます（`active` が元を生かすので、借用した部分が実際にある、元の場所から届く値を解放しうる書き込みは、写しがどこへ写っても競合します）。`shared_copy`・`ordered_copy` の写しは元と同じ場所なので、元が exact なら exact です。
- **自己の事実**: `self_path` は exact でない loan に先全体（`[]`）を返し、`ordered` も exact な loan だけで判断します（指摘 3）。`Source::paths` も同じ `self_path` を使います。
- **部分適用（`bound_value`）**: 部分適用とパイプラインの右辺が適用する引数で、場所の借用か `pointing` の局所値であるものの値は、関数値の loan のうちその借用が作ったもの（`Checker::borrow_loans`）か局所値が持つものだけで、`pointing` です（指摘 4）。lambda の捕捉は今までどおり関数値全体の値です。
- **要素の場所（`Source::element_paths`）**: 要素を通る場所（`f (ref arr[0])`、std の `Array.iterr` の `action (ref values[index])`）は、`Source::paths`（`Arg::part` の順序の判断に使う）には入れず、別の `element_paths` に記録し、呼び出し元の実引数が `points_at` のときだけ写しに使います（指摘 5）。
- **順序を示した自己の借用の要素の段（`self_borrow_dropped_first`）**: `element_paths` で `Array.iterir` の要素を `(arr, [ELEMENT])` と狭めると、呼び出し先が順序を示した自己の借用（`ordered_loans`）の写しの場所が `[ELEMENT, s]` になり、要素の段で拒否しました（下のマージの条件の確かめで見つけた `review-a31-conf-r10/p/iterir_local_safe` の退行。前は配列全体の写しで、段が無く受理していた）。順序を示した写しは、呼び出し先が仮引数の指す値の中へ書き込んだもので、借用とそれを持つ部分は実引数の指す 1 つの要素の中にあるので、その写しに限って要素の段は要素の中へ進みます。ほかの要素の借用を持つ形（`cross (ref arr[1]) (ref arr[0])`、それを本体に持つ関数）は呼び出し先で `ordered` にならず、今までどおり拒否します。これで `selfok (ref arr[0])`（先端・前の修正は拒否、基準は受理）も受理します。

#### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（`let q = child b; ref q.s` の借用を `b.s` とし、`n1.kids = []` が子を解放しても競合しない。基準からの退行） | 再現（`ce/n18_m1` を受理し `read -1 8 2`、ASan で解放後の読み出し。`n20_m1`・`n20_m2` も両方の edition で受理） | `points_at`・`pointing`。`n18_m1` は `31:5` の `E1014`、`n20_m1`・`n20_m2` は `30:5` の `E1013`（`self_referential_store`）で、先端と同じ | `a_borrow_through_the_result_of_a_call_is_one_of_the_whole_argument` |
| 2（呼び出しの結果の実引数、それを仮引数として渡す関数、そうした局所値を通した借用。先端からの退行） | 再現（`n9`・`n10`・`n1`・`n2`・`n3`・`n4`・`b1`〜`b6` を受理、ASan で解放後の読み出し） | 同上と `kept_copy` の `points`。どれも先端と同じ `E1013`・`E1014`。`b5_capture`・`n8_call_arg` も拒否のまま | 同上 |
| 3（自己の事実が呼び出しの結果を通した借用の場所を記録する。基準・先端も受理） | 再現（`s1`・`s2` を受理し `drop -1`） | `self_path`・`ordered` の exact。どちらも `25:5` の `E1013`（`self_referential_store`）。同じ根の、自己の事実を指す場所が分からない実引数に当てはめた写しを別の値へ写す形（`r15/p/h1_selfchild_copy`: `selfn (child (ref n1))` の後で `copyh` が `n1` の持つ関数を `p2` へ写し、`n1.kids = []`。修正後・先端・基準が受理し `drop 8`・`drop -1`）も見つけ、写しが元の loan を親に持つことで `34:5` の `E1014` にした。写しを作らない `selfn (child (ref n1))` だけの形と、写しても書き換えない形は受理（`drop 8`・`drop 0`・`1` など） | 同上 |
| 4（2 つ以上の引数を束縛した部分適用・パイプライン・関数値の仮引数へ渡した部分適用。基準が受理） | 再現（`p8/s_partial2`・`_pipe`・`_apply` が `E1013`、基準は `drop 16`・`drop 0`・`drop 0`・`1`） | `bound_value`、`ref (deref r)` の exact。3 つとも受理。逆の順序は `assignment_extends_borrow` | `a_part_bound_or_passed_to_a_function_value_keeps_a_borrow_of_that_part`、`in_place_writes` の `bound_parts_keep_their_strings` |
| 5（関数値へ渡す仮引数の先の部分、`Array.iterr`・`Array.mapr`。前の修正が直したと書いた形の残り） | 再現（`p3/s_each_field`・`s_each_user`・`p/s_iterr_partial`・`p3/s_iterr_partial_arrfirst_named`・`p3/s_map_partial` が `E1013`） | `path_copy` の exact、`element_paths`。`s_map_partial` のほかは受理。`Array.mapr` は配列を作る初期化の関数の格納を区別しない事実のままで、`drop.md` と A33（`mapr`）に制限として書いた | 同上（`mapr` は制限として拒否を固定） |
| 6（5 つ以上の部分、文書） | 再現（`p9/s_cross5` が `E1013`、`s_cross4` は前の修正で受理） | `drop.md` の「別の仮引数の先へ書き込んだ部分」に、値全体として扱う借用の一覧（参照の先のフィールド、呼び出しの結果、`let mut` の参照の変数、5 つ以上の部分、`Array.mapr`、書き込むセルを所有する関数値があるプログラムでの部分適用）と、それぞれの `E1013` を書き、`lifetimes.md` の「現在の制限」、A33（`callres`・`mutref`・`cross5`・`mapr` の行）とこのチケットの「残る制限」を直した | 同上（`cross5` は制限として拒否を固定） |

レビューと探査の 369 個（`r14-review/ce`・`conf`、`r14/p`・`q`・`t`、`r15/p`）を両方の edition で比べると（`r15/all-v4.tsv`）、前の修正からの変化は、上の指摘の形のほかに、前の修正が受理した安全な形のうち指す場所が分からない参照を通すもの（`conf/p/s_loop_refvar` の `let mut` の参照の変数、`conf/p11/s_wrap_field_arg` の `cross (ref p2) w.p`）が先端と同じ拒否に戻ったことです（文書に書いた制限）。`r14/q/idref`・`pick3`・`rcget`・`several`、`conf/p/s_several3`・`s_match3` は、`returned_parameters`・`Rc.get`・選択の `points_at` で受理のままか受理になりました。`conf/p7/s_tree_call`（`kid0` が子か自分を返す）は最初の誤りが基準と同じ `E1012` から `E1013` に変わりました（どちらも拒否）。基準が拒否して修正後が受理するのは `ce/n16d`・`n16e`・`n17`・`n17b`・`n17c`・`r197`・`n18_ctl`・`n20_ctl` で、前の修正と先端も受理する、A28 の保持の事実の精度による安全な形です。どれも native の `-O0`・`-O3` と ASan で手で求めた値（`9`、`4`、`2`、`read 0 8 2`・`1`、`drop 0`・`drop 0`・`-1`）を出し、ASan は何も報告しませんでした（`r15/ra-runs.txt`）。

#### テスト

- `tests/rc.rs` の `a_borrow_through_the_result_of_a_call_is_one_of_the_whole_argument`（`NODE_PRELUDE`）: 指摘 1〜3 の拒否（両方の edition で `44:5` の `E1013` の `self_referential_store` 6 形、edition 2027 で `47:5`・`48:5` の `E1014` の `borrow_conflict` 3 形、`n18` の `31:5` の `E1014`、`n20` の 2 形の `30:5` の `E1013`）と、安全な形の受理（`crossn (ref p2) (ref n1)` の後の `n1.kids = []`、`selfn (child (ref n1))`、写しても書き換えない形）。関数値の仮引数を通した `b2`（`apply2n crossn`）は、Rust のテストのスレッドのスタックでは再帰型の検査があふれる（CLI では `E1013`。レビューの報告のとおり `tsuzuri run` は基準でもあふれる、前からの問題）ので入れていません。
- 同じく `a_part_bound_or_passed_to_a_function_value_keeps_a_borrow_of_that_part`（`PARTS_PRELUDE`）: 指摘 4・5 と選択の受理 10 形、要素へ順序を守る自己の借用を書き込む受理 3 形（`selfok (ref arr[0])`、`Array.iterr selfok (ref arr)`、`iterir_local_safe` の形）、逆の順序の拒否 8 形（`41:5` の `assignment_extends_borrow`）、ほかの要素の借用の拒否 2 形（`41:5` の `self_referential_store`）、制限の `Array.mapr`（`assignment_extends_borrow`）と `cross5`（`self_referential_store`）の拒否。
- `in_place_writes` の `bound_parts_keep_their_strings`: パイプラインの右辺が 2 つの引数を束縛した `cross3_recorded` と、要素を関数値へ渡す `each_recorded`。手で求めた値は `(((1 * 100 + 0) * 100 + 10) * 100 + 12) * 100 + 0 = 100101200`（`p3` の記録は `"12345"` と `"1234567"` の 5 + 7、`p2` は 2 番目の要素の `"123456789"` の 9 + 1、`p1` と要素は 0）で、基準も同じ値です。このモジュールには書き込むセルを所有する関数値があり、部分適用の形は隠れた環境の規則で拒否されるので、部分適用は Rust のテストで確かめます。

#### マージの条件の確かめ（この見直しの後）

上の道具を `gate/tools/check-r15.mjs`・`check-r16.mjs`・`diff-r15.mjs`・`diff-r16.mjs` に写しました（基準と前の修正の結果を `results-r14.json` から使い、新しいバイナリだけを検査します）。`gate/tools/collect.mjs` を回し直して、このレビューのプログラム（`r14-review/ce`・`conf`）とこの見直しの探査（`r15/p`）、まだ入っていなかった前の修正の探査（`r14/p`・`q`・`t`）、A32 の 5 度目の merge の写しなど 448 個を足し、12615 個・25230 組を両方の edition で検査しました（`results-r15.json`、要素の段の変更の後の `results-r16.json`、分類は `diff-r15.txt`・`diff-r16.txt`、まとめは `r15/gate-summary.txt`）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4224 | 3282 |
| 両方が拒否 | 4938 | 6608 |
| 基準が拒否し対応後が受理（RA） | 26 | 22 |
| 基準が受理し対応後が拒否（AR） | 3427 | 2703 |
| 検査の失敗・時間切れ | 0 | 0 |

- **前からある 24334 組では、前の修正（`r14`）からの判定の変化は 0 組で、RA は同じ 28 組（15 個）です。** 新しい RA は、足した 896 組の中の 20 組（12 個）だけで、レビューの対照のプログラム `ce/n16d`・`n16e`・`n17`・`n17b`・`n17c`・`n18_ctl`・`n20_ctl`（前の修正と先端も受理する、上の確かめたとおり安全な形）と、A32 の 5 度目の merge が写した最初の確認の探査 `probes/f06_replace_inner_f`・`f11_rc_share_dropped`・`f21_replace_both_arms`・`u11_safe_twin_set_no_capture`・`u15_set_safe_param_kept`（最初の確認で native・ASan で安全と確かめた RA の 7 組、`gate/runs-probes.json`）です。条件 (1) は成り立ちます。
- 最初の `r15` の検査では、受理から拒否が 2 組ありました（`review-a31-conf-r10/p/iterir_local_safe` の両方の edition。`Array.iterir` の要素へ部分の順序を守る自己の借用を書き込む安全な形で、基準・先端・前の修正は受理）。要素の写しの場所の要素の段で拒否したためで、上の「順序を示した自己の借用の要素の段」で直し、`r16` で受理に戻りました（`r16` の `r15` からの変化はこの 2 組だけ）。
- 基準も対応後も受理するプログラムのうち、名前が危険を示すものに足されたのは、レビューが安全な形として書いた `conf/p/s_bad_distinct`（別々の `PairBad` の値）と `conf/p9/s_deep_write_u`（借用されていないフィールド `u` の書き換え）で、どちらも下の実行で基準と同じ値を出し、ASan は何も報告しませんでした。
- レビューと探査で先端が拒否して対応後が受理する 130 個のうち、前の修正も拒否した指摘 4・5 の 7 個（`p8/s_partial2`・`_pipe`・`_apply`、`p3/s_each_field`・`s_each_user`、`p/s_iterr_partial`、`p3/s_iterr_partial_arrfirst_named`）は、両方の edition で native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で実行し、どれも手で求めた値（`drop 16`・`drop 0`・`drop 0`・`1`、`drop 8`・`drop 0`・`1`、`drop 8`・`drop 0`・`drop 0`・`1`）を出しました（`r15/new7-runs.txt`）。残りの 123 個は edition 2027 の native と ASan の `-O0` で実行し、どれも基準と同じ出力で、ASan は何も報告しませんでした（`r15/others-runs.tsv`）。要素の段の変更で受理になった `r15/p/e1_iterr_selfok`・`e3_elem_selfok`・`e6_selfall`・`iterir_local_safe` も両方の edition で native・ASan の `-O0`・`-O3` で手で求めた値（`drop 8`・`drop 8`・`1`、`drop 8`・`drop 0`・`1`、`drop 8`・`0`）を出しました（`r15/elem-runs.txt`）。ほかの要素の借用を持つ `e4_crossel`・`e5_crossel10`・`e8_cross_direct_elems`、順序の逆の `e2_iterr_selfbad`・`e7_iterir_bad` は `E1013`（`self_referential_store`）のままです。
- 最終のバイナリで、レビューと探査の 383 個（上の 369 個と、この見直しの後に書いた `r15/p` の 14 個）を両方の edition で検査し直し、369 個は `r15/all-v4.tsv` から変化が無く、14 個は上のとおりであることを確かめました（`r15/final-new.tsv`）。

#### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3069 件すべて成功、4 件 skip（1750 秒、`r15/nextest-2.log`。要素の段の変更の前の 1 回目も 3069 件成功、`r15/nextest.log`）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 77 件（この見直しの 1 件を含む。基準のバイナリでも 77 件が成功）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件がすべて成功（`r15/suites-2.log`、`e2e-ipw-base.log`）。
- 手順 5 の比較（`_tools/step5.mjs`、基準と対応後の `check --json`・`build --no-cache --emit llvm`）: `tests/fixtures`・`examples` の 154 個のどれも出力が同じ（`r15/step5.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・47 個のリンク・9 個の例・18 回の native の実行）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功。
- 変えたファイル: `src/ownership.rs`、`src/ownership_control.rs`、`tests/rc.rs`（2 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（制限の表の `callres`・`mutref`・`cross5`・`mapr` の行、段 3、見直しの記録）、このチケット（「残る制限」と、この節）。

### Copilot のレビューへの対応（第1回）と fvpart（2026-11-14）

PR #105 への Copilot のレビュー（`drop.md` の「文の一時値」の項目）と、A32 の 6 度目の merge が見つけた、`main` が受理する安全なプログラムの文書に無い拒否（`fvpart`）に対応しました。結果は `target/scratch/a31/cp1/` にあります（`cp1/v.sh` が修正後・`main`（`168ae4a5`、`m1/bin/tsuzuri-main`）・基準の最初の誤りを並べ、`RUN=1` で修正後の native の `-O0`・`-O3` と ASan の `-O0`・`-O3` の出力を足します。`o1`〜`o9` が lowering を確かめた探査、`p/` が一時値の探査、`fv/` が A32 の再現の写し、`fv2/` が fvpart の探査です）。

#### Copilot の指摘: 一時値の解放の順序

指摘は「作った順は文が正常に終わるときだけで、`break`・`continue` と受け止めた例外は、まだ解放していない一時値を逆の順に解放する（`src/llvm_control.rs:44`、`src/llvm_exception.rs:27`）」というものです。lowering を出口ごとに確かめると、指摘のとおりで、加えて文書の「文の終わり」も誤りでした。

| 出口 | 実際の順序（`src/llvm*.rs`） |
| --- | --- |
| 正常な完了 | 呼び出しが借用した一時値（`BorrowOperand`: `ref (T { ... })` の引数、edition 2027 の読み取りの仮引数へ渡した値、比較の演算子の被演算子）は、その呼び出しの直後に `release_operands` が作った順に解放します。文の終わりではありません（`o3`: `k (h (ref T11) (ref T21) 0) (...)` の 11・21 は後の引数の評価より前、`o6`: フィールドを読んだ一時値は読んだ直後）。 |
| `break`・`continue` | `emit_loop_jump` が、ループの本体を始めてから残っている一時値（`temporaries[temporary_base..]`）を作った逆の順に解放し、次にスコープを内側から束縛の逆順に解放します（`o2`: 後の引数の `break` で 22、12）。 |
| 同じ関数の `try` が受け止める例外 | `raise` が `break` と同じく、`try` の本体を始めてから残っている一時値を逆の順に、次にスコープを解放します。ハンドラーの中の再送出（`Reraise`）も外側の `try` まで同じで、`finally` の中の送出は保留中の結果を一時値として解放します。 |
| `try` の無い例外、別の関数の例外 | 例外は関数の境界を越えず、その場でトラップします（解放は走りません）。 |
| `return`、コンピュテーション式の早期の脱出 | `return` は言語にありません。`?` などの早期の脱出は `Bind` が継続を呼ばないだけで、関数の通常の戻りです。 |
| `match` の対象とパターンの変数 | スコープとして、正常な経路と脱出で同じ順に解放します。 |

捕捉の後始末（既知のコールバックのクロージャ、`cleanup.iter().rev()`）は逆の順ですが、`Drop` 型の値は再利用できる関数値に捕捉できない（`E1005`）ので、`Drop.drop` の順序には関わりません。

健全さ: 検査の一時値どうしの規則（`DropSlot::Temporary`、`temporary_dropped_first`）は、呼び出しが借用した一時値（`Arg::dying`）を、呼び出しの直後の作った順で比べます。逆の順になる脱出は、その呼び出しの引数の評価中、つまり呼び出しの前にしか起きず、そのとき呼び出しはまだ一時値の借用を書き込んでいません。呼び出しが戻ってから一時値を解放するまでの間に利用者のコードはありません。探査で確かめました（どれも両方の edition）。

- `p/t1_break_later_arg`・`t2_continue_later_arg`・`t3_raise_later_arg`（先に作った `Holder` が後の文字列の借用を持つ `keep3` の 3 つ目の引数で `break`・`continue`・`@checked` の例外）、`t4_store_then_break`・`t5_store_then_raise`・`t7_pipe_store_then_break`（呼び出しの後の、同じ文の後の部分で脱出）: 修正後・`main`・基準が受理し、native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で手で求めた値（`t1`: `drop holder reads 8`・`reads 0`・`total 1`、`t2`: `8`・`0`・`8`・`total 4`、`t3`: `8`・`small 9`・`0`・`big -1`、`t4`・`t7`: `8`・`8`・`total 2`、`t5`: `8`・`small 10`・`8`・`big -1`）を出し、ASan は何も報告しませんでした。`break` した反復の `Holder` は書き込まれる前の関数（`0`）を読みます。
- `p/u1_break_later_holder`・`u2_raise_later_holder`（保持する側を後に作る）: 修正後は `E1013`（`assignment_extends_borrow`）、`main`・基準は受理し、基準は正常な経路で解放した文字列を読みます（`drop holder reads -1`）。
- `t6_fnvalue_break`（関数値への一時値の借用）は、どのバイナリも `E1013`（`borrow_of_temporary`）です。

検査の変更は要りませんでした。変えたのは文書と説明です。

- `drop.md`: 「文の一時値」の項目を「一時値」に書き直し（使い終わった位置で解放し、呼び出しが借用した一時値は呼び出しの直後に作った順。`g (f (ref (T { id: 1 })) 0) (h 0)` の 1 は `h` の前）、「`break`・`continue`・例外で式の途中から抜けるとき」の項目を足し、`run=` の例（`drop 11`・`drop 12`・`drop 22`・`drop 21`・`23`）を足しました。表は「文の一時値」の行を「呼び出しが借用した一時値」と「式の途中から抜けるときに残っている一時値」（作った逆の順、抜けるスコープの変数より先）の 2 行にし、「抜けるスコープの変数」の行に「残っている一時値の後」を足しました。検査の項目「呼び出しが借用した一時値」に、脱出で逆になっても呼び出しの前なので借用はまだ無いことを書きました。
- `lifetimes.md`: 一時値は文の終わりではなく呼び出しが戻った直後に解放されることに直しました。
- `docs/language.md`（ループの脱出）・`docs/architecture.md`（`emit_loop_jump`、例外の `raise`）: 脱出の順序と、検査が正常な経路の順序だけを見る理由を書きました。
- `src/ownership.rs`: `DropSlot::Temporary`・`temporary_slot`・`Arg` などのコメントと、2 つのメッセージ（`temporary_borrow_stored` の `... dropped when the call returns`、`temporary_dropped_first_error` の `the temporaries that a call borrows are dropped when it returns, in the order in which they were made`）を、実際の解放の位置に合わせました（code と kind は同じ）。

テスト: `tests/rc.rs` の `an_early_exit_before_the_call_keeps_the_order_of_its_borrowed_temporaries`（受理 6 形を両方の edition で、拒否 3 形を `29:25`・`27:13` の `E1013` の `assignment_extends_borrow` で）、`in_place_writes` の `early_exits_drop_pending_temporaries`（native と WASM の `-O0`・`-O3`。1 回目の反復の記録が `"12345"` の 5 + 1、`continue`・`break` の反復は書き込まれる前の 72・73、`3` では `"123"` の 3 + 9 と結果 9、`2^62` では桁あふれで 75 と結果 -1 なので、手で求めた値は `((((((1 * 100 + 6) * 100 + 72) * 100 + 73) * 100 + 9) * 100 + 12) * 100 + 75) * 10 + (-1 + 2) = 10672730912751`。基準も同じ値で、単独のプログラム `p/e1_fixture_case` は ASan でも同じ値）。

#### fvpart: 名前付き関数を束縛した局所値のパイプラインと部分適用

A32 の 6 度目の merge が、名前付き関数 `cross`（`b.s` の借用を `a.h` の `Mutex` へ書き込む）を局所値に束縛して（`let chosen = cross`、名前付き関数を選ぶ `if`）、パイプライン `ref p1 |> chosen (ref p2)` か部分適用 `let g = chosen (ref p2); g (ref p1)` で呼ぶ安全な形を、A31 が両方の edition で `E1013`（`assignment_extends_borrow`、`p1` が `p2` の借用を持つ）で拒否することを見つけました（`target/wt/a32/target/scratch/a32/merge6/fv/`。基準は `drop 8`・`drop 0`・`1`。直接の呼び出し `chosen (ref p2) (ref p1)` は受理）。

原因は 3 つでした。

- `apply_higher_once` は、部分適用の先頭が名前付き関数（`direct_callee`）のときだけ事実で置き換え、先頭が既知の局所値（`Checker::known` の `chosen`）の部分適用 `chosen (ref p2)` は本体の見えない関数値の規則（`apply_opaque`: どの引数の借用もほかの引数の先へ）でした。パイプラインは `pipe_stores` がこの形で `apply_higher` を呼びます。
- `known_functions` は `let g = f (x)` を `f` が名前付き関数のときだけ既知の局所値にしたので、`g (ref p1)` も本体の見えない関数値でした。
- 部分適用 `chosen (ref p2)` そのものも、`call_stores` が関数値の呼び出しとして `apply_higher` を呼び、`chosen` の式（2 つの仮引数）と引数の数が合わないので `apply_opaque` になり、`p2` が自分の借用を持ちました。さらに、`g` の破棄は、`value_runs_drop_code` が名前付き関数の部分適用しか知らないので、どんなコードも実行しうるものでした（部分適用だけの形 `fv2/s_partial_only` も `self_referential_store` で拒否していました）。

変えたこと（`src/ownership.rs`）:

- `known_heads`（既知の局所値の式。別の既知の局所値はその式に展開し、`let b = a` の連なりをたどる）、`known_arity`（式が実行の前に取る残りの引数の数: 名前付き関数の仮引数、持ち上げたラムダの捕捉の後の仮引数、名前付き関数か、どの式も同じ数を取る既知の局所値の部分適用の残り）、`partial_heads`（どの式もそれより多くの引数を取る）。
- `known_functions`: どの式もまだ引数を取る既知の局所値の部分適用（`let g = chosen (ref p2)`）も既知の局所値にします。
- `apply_higher_once`: どの式にも引数が足りない既知の局所値の呼び出しは部分適用で、何も格納しません（名前付き関数の部分適用と同じ）。先頭が既知の局所値の部分適用は、どの式も束縛した引数と残りの引数でちょうどそろうとき、束縛した引数（`bound_value` で狭めた値）を先に並べて、それぞれの式の事実で置き換えます。
- `value_runs_drop_code`: 既知の局所値の部分適用の破棄は、局所値の関数値と束縛した引数の破棄です。

確かめた形（`cp1/fv2/gen.py`、両方の edition。受理はどれも native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で手で求めた値、ASan の報告なし: `cp1/runs-fv-2027.txt`・`runs-fv-2026.txt`）:

| 形 | 修正後 | 前（`m1` の merge のビルド） | `main`・基準 |
| --- | --- | --- | --- |
| 別名・`if` の選択・別名の別名・ラムダ・`cross` と `outer` の選択の、直接・パイプライン・部分適用・部分適用を完成させるパイプライン（`s_*` 20 個と A32 の再現 6 個） | 受理（`drop 8`・`drop 0`・`1`） | 直接だけ受理 | 受理 |
| 3 つの値の `cross3` の別名で、2 つを束縛した部分適用・部分適用の部分適用（`s_three_*` 5 個） | 受理（`drop 16`・`drop 0`・`drop 0`・`1`） | `E1013` | 受理 |
| 部分適用だけ（`s_partial_only`） | 受理（`drop 0`・`drop 0`・`1`） | `E1013`（`self_referential_store`） | 受理 |
| 参照を捕捉したラムダの別名、部分適用を関数値の仮引数へ渡す呼び出し（`a_s_*` 4 個） | 受理（`drop 8`・`drop 0`・`1`） | `E1013` | 受理 |
| 上の逆の呼び出し（`u_*` 28 個）、逆に書き込むラムダとの選択（`u_mix_*`）、逆の捕捉・渡した部分適用・タプルに入れた部分適用（`a_u_*`） | `E1013`（`assignment_extends_borrow`、`p1` の位置） | `E1013` | 受理（基準は解放した文字列を読み、`drop 0`・`drop -1`・`1`、3 つの値は `drop -2`） |
| 部分適用の後で借用された `p1` をムーブ（`a_u_move_after`） | `E1014`（名前付き関数の部分適用と同じ） | `E1013`（部分適用の `self_referential_store`） | 受理 |

残る制限（文書に書いたもの、A33 の `fvparam`）: 関数値の仮引数を本体の中で部分適用・パイプラインで呼ぶ関数（`fn applyp f a b = { let g = f a; g b; 0 }`、`fn applyq f a b = { b |> f a; 0 }`）へ `cross` を渡す `applyp cross (ref p2) (ref p1)` は、修正後も前のビルドも `E1013`（`assignment_extends_borrow`）で、`main`・基準は受理します（`drop 8`・`drop 0`・`1`、`fv2/b_s_param_*`）。高階の事実（`HigherOrderCall`）は仮引数の関数値をすべての引数で呼ぶ形だけを記録し、部分適用した `g` の呼び出しは本体の見えない関数値の呼び出しなので、呼び出し元はどの実引数の借用もほかの実引数の先へ入りうるものとして扱います。直すには、仮引数の部分適用を高階の事実に記録する必要があり、`drop.md`（「別の仮引数の先へ書き込んだ部分」の安全側の一覧）、`lifetimes.md` の「現在の制限」（本体の見えない関数値の一覧）、A33 の表、上の「残る制限」に書きました。

テスト: `tests/rc.rs` の `a_local_bound_to_a_named_function_keeps_the_parts_in_a_pipeline_or_partial_application`（`CROSS_PRELUDE`。7 組の受理と逆の拒否、部分適用だけの受理、逆に書き込むラムダとの選択の拒否、3 つの値の 4 組の受理と逆の拒否を、拒否は `35:5` の `E1013` の `assignment_extends_borrow` で、両方の edition。制限の `applyp`・`applyq` の拒否も固定）、`in_place_writes` の `local_function_values_keep_the_part`（`cross_recorded` を選ぶ `if` を束縛した局所値のパイプラインと部分適用。`p2` と `p3` の記録が `p1` の `"12345"` の 5 + 1、`p1` は 0 なので、手で求めた値は `1 * 1000 + 0 * 100 + 6 * 10 + 6 = 1066`。基準も同じ値で、前のビルドはこのモジュールを `E1013` で拒否します）。

#### 確かめの中で見つけて直した穴（`main` も受理）

下のマージの条件の確かめ（`r17`）で、`review-a31-ce/p01_staged_int_first`（第1回のレビューの反例）が受理から拒否に変わりました。`mk a = \k f -> ...` が `Arc<Mutex<...>>` を値で捕捉したラムダを返し、`let w = mk (...)`、`let w2 = w 0` の後で、内側のブロックの局所値の借用を持つ関数を `w2` でセルへ書き込み、次の `w2` の呼び出しがそれを実行する形です。基準・`main`・前のビルド・この回の前の先端（`2ed5f692`）は受理し、局所値を配列の要素にした形（`cp1/reg/p01b`）では基準が解放した値を通して `72` を出し、ASan は stack use after scope を報告します（元の形は文字列の局所値を読むので、出力は `0` で、ASan の報告もありません）。危険な形で、fvpart の修正（既知の局所値の部分適用を事実で置き換える）が `w2` の呼び出しに `mk` のラムダの事実を当てはめ、借用を `w2` の環境に残すようになって拒否しました。

同じ形でも、`let mut w2 = w 0`（`reg/v4_mut_local`）、`let mut w = mk (...)`（`v8_mut_w_imm_w2`）、本体が束縛の後でラムダを返す `mk` を引数を多く当てはめる形（`v10_block_mk_mut`）、パイプライン `0 |> w`（`v11_pipe_mut`）は、既知の局所値を通らないので受理のままで、どれも ASan が stack use after scope を報告しました（`main` も受理する前からの穴）。原因は `sink_flags` です。セルを所有する関数値の型（`owned_types`）は、関数値を作る所ごとに求め、それを環境に持つ関数値の型は holder として受け継ぎますが、関数値の呼び出しが関数を返す所（`staged`）は束縛した引数だけを見て、当てはめた関数値そのもの（結果はそれを環境に持つ）を数えていませんでした。`mk` のラムダの型 `i64 -> (unit -> i64) -> i64` は対象でも、`w 0` の型 `(unit -> i64) -> i64` は対象でなく、見えない呼び出しの規則（関数値と引数の借用を、そうした型のすべての局所値に残す）が働きませんでした。

直したこと（`src/ownership.rs` の `sink_flags`、`e2593013`）: 関数値の呼び出し、名前付き関数が返す関数値への当てはめ、それを完成させないパイプラインの結果は、当てはめた関数値（`applied`・`returned`）を環境に持つ値として holder に入れます。小さな変更で、ラムダが捕捉した関数値と同じ扱いです。上の 4 形と元の形・`p01b`・関数の結果への当てはめ（`v1_callres_partial`）は、両方の edition で内側のブロックを指す `E1013`（`assignment_extends_borrow`）になり、借用される配列を関数値より先に宣言した形（`s10_mut_outer_safe`・`s12_block_mk_safe`、手で求めた値 `72`）と所有する関数だけを書き込む形（`s11_mut_owned_safe`、`7`）は受理のままで、native の `-O0`・`-O3` と ASan で同じ値でした。`lifetimes.md`（そうした関数値の型の説明）と `docs/architecture.md`（`sink_flags`）に書き、`tests/rc.rs` の `a_partial_application_of_a_function_value_that_owns_its_cell_shares_the_cell`（拒否 5 形を位置と kind で、受理 3 形を両方の edition で）を足しました。

#### マージの条件の確かめ（この対応の後）

上の道具を `gate/tools/check-r17.mjs`・`diff-r17.mjs`（基準と前の見直しの後の `r16` の結果を `results-r16.json` から使い、`r16` に無いプログラムは m1 の merge のビルド（`prev`）でも検査）、穴の修正の後の `check-r18.mjs`・`diff-r18.mjs`（`results-r17.json` を使う）に写し、変化を merge と この回のどちらによるかを `prev-r17.mjs`（`prev-r17.json`、前のビルドと `main` で検査し直す）で確かめました。`gate/tools/collect.mjs` で、この対応の探査（`cp1/p`・`fv`・`fv2`・`reg`・`ra`）、m1 の探査、A32 の 6 度目の merge の写しなど 286 個を足し、12901 個・25802 組を両方の edition で検査しました（`results-r17.json`・`results-r18.json`、分類は `diff-r17.txt`・`diff-r18.txt`。`cp1/gate-diff-r17.txt`・`gate-diff-r18.txt` に写し）。最終のバイナリ（`r18`）の分類です。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4328 | 3366 |
| 両方が拒否 | 5010 | 6719 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 3525 | 2787 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、前の見直しの後のビルドが受理しないものは 0 組です。** 48 組は前回と同じ（安全と確かめたもの）で、新しい 19 組は、基準が匿名共用体の構文を読めない（`E0002`）プログラムです: m1 の探査 `s1`〜`s8`（とその A32 の写し）、A32 の 6 度目の merge の `au/*`、この回のビルドの写しの中の `tests/fixtures/anonymous_unions`。どれも `main`（`168ae4a5`）も受理し、探査は native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で手で求めた値（`9`、`5`、`9`、`9`、`1009`、`9`、`45`、`45`、`321`、`14`、`14`）を出し、ASan は何も報告しませんでした（`cp1/ra-runs.txt`）。fixture は `anonymous_unions` のスイートが値を確かめるものです。条件 (1) は成り立ちます。
- **前からある 25230 組の `r16` からの変化は 3 組で、どれもこの回の変更によります**（`main` の merge だけのビルドは `r16` と同じ判定）。受理から拒否の 2 組は上の `p01_staged_int_first`（危険な形で、前からの穴を閉じたもの）、拒否から受理の 1 組は `a32/merge4/spell/p.fa3_partial_other_part__target2__ifc`（`target2` を選ぶ `if` を束縛した局所値の部分適用 `let g = chosen 1; g (ref mut p0)` の後で借用されていない `p0.n` を書き換える。基準も受理し、native・ASan で手で求めた `drop 8 2`・`11`、ASan の報告なし）です。
- 足した 572 組では、前のビルドからの変化は上の fvpart の受理（68 組。A32 の再現と `merge6/d/*safe__ifc` を含み、どれも基準が受理する安全な形）、`a_u_move_after` の kind（`E1013` から `E1014`）、`r17` から `r18` の穴の 4 形（8 組、受理から拒否）だけです。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。

#### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3285 件すべて成功、4 件 skip（1786 秒、`cp1/nextest-2.log`。穴の修正の前の `21caa7fe` でも 3284 件すべて成功、`cp1/nextest.log`）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 79 件（この回の 2 件を含む。基準のバイナリでも 79 件が成功）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control.mjs` 701 件、`user_drop.mjs`（`-O0`・`-O3`）がすべて成功（`cp1/suites-2.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`cp1/step5-main-2.txt`）。基準と比べると 25 個が違い、`tests/fixtures/anonymous_unions`（基準が読めない）と、LLVM IR だけが違う 24 個で、`main` との比較で同じなので merge した `main` の変更によるものです（`cp1/step5-2.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・48 個のリンク・10 個の例・20 回の native の実行。新しい `run=` の例を含む）、`node scripts/check-guide-lite.mjs`、`node scripts/check-links.mjs`、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（3 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（2 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（`fvparam` の行、fvpart と穴の記録）、このチケット（「残る制限」と、この節）。

#### レビューの第1回（2026-11-14）

この対応の後の先端（`88885b9f`）のレビュー（反例の探索 `target/scratch/a31/r16-review-1-ce/`、適合の確認 `target/scratch/a31/r16-review-1-conf/`）の 4 件の指摘を、修正後・先端・その前の先端（`298408bd`）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どれも指摘のとおりに再現しました。指摘 1・2 は前の先端からの退行（`main` も受理する危険な形を受理）、指摘 3 は先端の穴の直し損ね（`main` も受理）、指摘 4 は `main` が受理する安全な形の文書に無い拒否です。結果は `target/scratch/a31/rr1/` にあります（`rr1/v.sh` が修正後・先端・前の先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`h/` が指摘 3 の探査、`k/` が指摘 4 の探査、`c/` が指摘 1・2 の安全な形、`fx*` が fixture の実験です）。

##### 原因と変えたこと（`src/ownership.rs`）

- **指摘 1・2（関数値の仮引数へ渡す値の場所を短くした記録）**: 呼び出し先が関数値の仮引数へ渡す値の場所（`Source::paths`・`element_paths`）は `add_self_path` が記録し、4 段で切るか、渡しうるほかの場所の中の場所を外側の場所にまとめます（`if k > 0 then ref (deref (Rc.get (ref x.a.e[0]))) else ref x.a` の `[a, e, ELEMENT]` と `[a]` は `[a]`、`ref (deref (Rc.get (ref x.a.b.c.d.e[0])))` は `[a, b, c, d]`）。それでも `Source::pointing` は真のままで、呼び出し元は短くした場所 `(x, [a])` の exact な複製を作り、`crossd` の `kept_copy` がそれを `(x, [a, s])` へ狭めました（`record D { s: string, e: [Rc<D>] }` では外側の場所も同じ型 `D`）。実際の借用は `x.a.e[0]` の `Rc` が指す `D` の `s` なので、`x.a.e` のムーブ・置き換えと競合せず、`p2` の `Holder` の `drop` が解放した文字列を読みました。`add_place_path` が場所を短くしたかを返し、短くした仮引数を `Source::coarse` に記録します。そうした仮引数の値は `pointing` にせず、`path_copy` の複製を exact にしません。複製の場所（外側の場所、4 段目までの場所）は借用した場所を含むので、元の loan を親にはしません（`path_copy` の `coarse`）。これで `x.a.e`・`x.a.b.c.d.e` のムーブ・置き換えは前の先端と同じ `E1014` になり、借用された部分の外（`x.z`）の書き換えは受理します（`c/c_merged_sibling`・`c_cut_sibling`。前の先端は拒否、`main` は `drop 8`・`1`）。外側の場所の中の借用されていない部分（`x.a.b.c.d.s`）の書き換えは、先端と同じく `E1014` で、安全側の制限です（`r16-review-1-ce/p/tp_ctl_s`、`main` は `drop 8`・`1`。A33 の `coarse`）。
- **指摘 3（セルを所有する関数値の部分適用を組み込み関数で作る形）**: 先端の修正（`e2593013`）は、関数値の呼び出し（と名前付き関数が返す関数値への当てはめ、それを完成させないパイプライン）の結果に、当てはめた関数値を型で持たせる（holder）ものでした。組み込み関数（`Owned.call (ref w) 0`、そのパイプライン、`new [T](1, w)` と、それを使う std の `Array.init 1 w`、`FixedArray.init w`）が当てはめる形は対象外で、`w2` の型 `(unit -> i64) -> i64` は所有する関数値の型にならず、局所値の借用をセルへ書き込めました（`h1_owned_call` など。先端・前の先端・`main`・基準が受理し、ASan で stack use after scope、出力 `72`）。この規則を組み込み関数へ広げると、`in_place_writes` の fixture の 15 個の関数を拒否しました。`List.foldr` の std の関数は、コールバックの仮引数（セルを所有するラムダの型）を組み込み関数へ渡すので、状態の型 `unit -> i64` が所有する関数値の型になったためです。先端でも、fixture の `fold_state_comes_back` の `List.foldr` を `Array.foldr` に変えると同じく拒否します（`rr1/fx4`。`Array.foldr` はコールバックを `folder state (ref values[index])` と呼ぶので、先端の規則に当たります。`main`・基準は受理）。呼び出しの結果が呼び出した関数値の部分適用になるのは、その関数値がまだ本体を実行しないときだけで、そうでなければ結果は本体が作った値で、本体の中の作る所で数えます。そこで規則を作り直しました: `sink_flags` の `found` は、環境を持つ関数値を作る所で、その値が本体を実行する前に取る引数の数（持ち上げたラムダは捕捉の後の仮引数の数、部分適用は残りの仮引数の数、組み込み関数の部分適用は残りの引数の数、関数を返す見えない呼び出しの結果は分からないので仮引数のすべて）も受け取り、`partial_types` がそれに足りない数の引数を当てはめた後の型（`sink_type` で平らにした仮引数の後ろの部分）を、作る所の型と同じく sink・所有・holder に入れます。部分適用はどこで作っても同じ環境と型を持つので、当てはめる所を問わず数えます。`e2593013` の `applied`・`returned` の holder は除きました（`staged` の束縛した引数の規則は残します）。
- **指摘 4（`match`・ブロック・部分適用の選択を束縛した局所値）**: `known_functions` の `shown` は、`if` のほかは選択を知らず、`let chosen = match n with | 0 -> cross | _ -> cross` と `let chosen = { cross }` は本体の見えない関数値（`apply_opaque`）でした。部分適用を選ぶ `if`（`if n < 0 then cross (ref p2) else cross (ref p2)`）は既知の局所値でしたが、その破棄（`value_runs_drop_code`）が `if` を知らず、どんなコードも実行しうるものとして関数値の loan を引数の先へ格納し、`p2` が自分の借用を持ちました（束縛だけの形も拒否）。`shown` が `match` のすべての腕とブロックの結果を、`value_runs_drop_code` が `if` の枝・`match` の腕・ブロックの結果を見るようにしました。ブロックの束縛した局所値は不変か、引数に現れても関数値全体の値を持つので（`bound_value`）、束縛のあるブロックも同じです。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（入れ子の場所をまとめた記録を指す場所とした。前の先端からの退行） | 再現（`pm_move`・`pm_assign` を先端が受理、native で `drop -1`・`1`、ASan で heap use after free。前の先端は `28:27`・`28:5` の `E1014`、`main`・基準は受理） | `Source::coarse`。`28:27`・`28:5` の `E1014`（`borrow_conflict`）で前の先端と同じ、両方の edition（`pm_assign` は edition 2027）。安全な `pm_safe` は受理（`drop 8`・`1`） | `a_borrowed_place_merged_into_a_larger_part_is_no_place_that_the_argument_points_at` |
| 2（4 段で切った記録。前の先端からの退行） | 再現（`tp_move`・`tp_assign`・`tr_assign`。ASan で heap use after free） | 同上。`31:27`・`31:5` の `E1014`。`tp_safe` は受理、`tp_ctl_s`・`tp_ctl_whole` は `E1014` のまま | 同上 |
| 3（組み込み関数の当てはめ。直し損ね） | 再現（`h1_owned_call`・`h6_owned_pipe`・`h8_new_array`・`h9_array_init`・`h10_owned_imm` を先端が受理、ASan で stack use after scope） | `partial_types`。どれも `7:17`・`8:17` の `E1013`（`assignment_extends_borrow`）、両方の edition。`rr1/h` の `List` の `new [\|T\|](1, w)`、`FixedArray.init w`（edition 2026）、`let g = Owned.call (ref w); g 0` も同じ。安全な形（配列を先に宣言 `s_*`: `72`、所有する関数だけ `o_*`: `7`）は受理し、native と ASan の `-O0`・`-O3` で同じ値 | `a_partial_application_by_a_builtin_shares_the_cell_of_the_function_value`、`in_place_writes` の `array_fold_state_comes_back` |
| 4（`match`・ブロック・部分適用の選択。`main` からの退行） | 再現（`k_ifpartial_*`・`k_block_*`・`k_matchguard_*`・`m2`・`m3`・`m4`・`s_mixarity`・`c1_match_direct`・`s_match_choice` を先端が `23:5` の `E1013` で拒否、`main`・基準は手で求めた値） | `known_functions`・`value_runs_drop_code`。どれも受理 | `a_local_bound_to_a_match_block_or_choice_of_partial_applications_keeps_the_part`、`in_place_writes` の `matched_function_values_keep_the_part` |

指摘 4 の形は `rr1/k/gen.py` の 100 個で両方の edition で確かめました（`rr1/k/res-2027.txt`・`res-2026.txt`）。

| 形 | 修正後 | 先端 | `main`・基準 |
| --- | --- | --- | --- |
| `match` の選択（ガード付き、`outer` との選択）、ブロック（束縛あり・なし）、ブロックを枝にした `if`、`if` を腕にした `match` の、直接・パイプライン・部分適用・部分適用を完成させるパイプライン（`s_*` 24 個） | 受理（`drop 8`・`drop 0`・`1`） | `E1013`（`assignment_extends_borrow`） | 受理 |
| 部分適用の `if`・`if`（`outer` と）・`match`・ブロックの、呼び出し・パイプライン・別名（`s_*` 12 個）と束縛だけ（4 個） | 受理（`drop 8`・`drop 0`・`1`、束縛だけは `drop 0`・`drop 0`・`1`） | `E1013`（`self_referential_store` か `assignment_extends_borrow`） | 受理 |
| `cross3 (ref p3)` と `cross` の `if`・`match`、`cross3 (ref p3)` 同士の `if` の、直接・パイプライン・部分適用（`s_*` 9 個） | 受理（`drop 0`・`drop 8`・`drop 0`・`1`、`cross3` 同士は `drop 16`・`drop 0`・`drop 0`・`1`） | `E1013` | 受理 |
| 上の逆の呼び出しと逆の宣言の順（`u_*` 45 個）、逆に書き込む `crossr` との選択（`u_mix_*` 6 個） | `23:5` の `E1013`（`assignment_extends_borrow`） | 同じ | 受理（解放した文字列を読み `drop -1`、3 つの値は `drop -2`） |

受理の 49 個は、両方の edition で native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で手で求めた値を出し、ASan は何も報告しませんでした（`rr1/k/runs.txt`）。

##### テスト

- `tests/rc.rs` の `a_borrowed_place_merged_into_a_larger_part_is_no_place_that_the_argument_points_at`（`COARSE_PRELUDE`）: 指摘 1・2 のムーブ（両方の edition で `36:27` の `E1014`）と置き換え（edition 2027 で `36:5` の `E1014`）、ムーブの無い形と借用の外の `x.z` の書き換えの受理。
- 同じく `a_partial_application_by_a_builtin_shares_the_cell_of_the_function_value`: 指摘 3 の 6 形（`Owned.call`、そのパイプライン、`Owned.call` の部分適用、`new [T]`、`new [|T|]`、`Array.init`）の拒否（`7:17`・`8:17` の `E1013`）と安全な 12 形の受理を両方の edition で、`FixedArray.init` の 3 形を edition 2026 で、`List.foldr`・`Array.foldr` の状態の形の受理。
- 同じく `a_local_bound_to_a_match_block_or_choice_of_partial_applications_keeps_the_part`（`CROSS_PRELUDE`）: 指摘 4 の受理と逆の拒否（`35:5` の `E1013`）。
- `in_place_writes` の `matched_function_values_keep_the_part`（`match` の選択のパイプライン、ブロックの別名の直接の呼び出し、部分適用。`p2`・`p3`・`p4` の記録が `p1` の `"12345"` の 5 + 1、`p1` は 0 なので、手で求めた値は `1 * 10000 + 0 * 1000 + 6 * 100 + 6 * 10 + 6 = 10666`）と `array_fold_state_comes_back`（`fold_state_comes_back` の `Array.foldr` の形。セルは 1 つ目の要素の後の状態 `9 + 1`、最後の状態は `9 + 2` なので `10 * 100 + 11 = 1011`）。どちらも基準のバイナリで同じ値で、先端はこのモジュールを拒否します。部分適用の選択はこのモジュールの隠れた環境の規則（`fx3`）に当たるので、Rust のテストで確かめます。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r19.mjs`・`diff-r19.mjs` に写し（基準と先端（`r18`、`88885b9f` のビルドで `e2593013` と同じソース）の結果を `results-r18.json` から使い、`r18` に無いプログラムは先端でも検査）、`gate/tools/collect.mjs` でこのレビューのプログラム（`r16-review-1-ce`・`r16-review-1-conf`）とこの回の探査（`rr1/`）など 515 個を足して、13416 個・26832 組を両方の edition で検査しました（`results-r19.json`、分類は `diff-r19.txt`、`rr1/gate-diff-r19.txt` に写し）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4533 | 3562 |
| 両方が拒否 | 5045 | 6767 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 3800 | 3058 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、どれも先端も受理するもの（前回と同じ 67 組）です。新しい RA はありません。** 条件 (1) は成り立ちます。
- **前からある 25802 組では、先端からの受理・拒否の変化は 0 組です。** 拒否のまま最初の誤りが変わったのが 2 組（`review-a31-ce-r3/h02_known_array_mixed`・`h03_known_record_mixed` の edition 2027）で、どちらも基準が受理する危険な形（内側のブロックの局所値の借用を、セルを所有する関数値を要素・フィールドに持つ配列・レコードを通してセルへ書き込む）です。先端は呼び出しの結果がその借用を持つものとしてブロックの値の束縛で `assignment_extends_borrow`、対応後は呼び出しでその借用を格納する所で `store_local_borrow_through_reference`（`cannot pass this function value here: ...`）で、edition 2026 は両方とも `assignment_extends_borrow` です。どちらの誤りも、内側のブロックの局所値の借用がブロックの外へ残ることを理由にしています。
- 足した 1030 組での先端からの変化は、受理から拒否の 22 組（指摘 1〜3 の危険な形と `rr1/h` の `u_*`）と、拒否から受理の 137 組（指摘 4 のレビューの形と `rr1/k` の `s_*`、`rr1/x`、fixture の実験 `rr1/fx4`・`fx8_*`）です。拒否から受理のどれも基準が受理し、`rr1/k` の `s_*` と `rr1/x` は native と ASan で手で求めた値を出し、`rr1/fx4`・`fx8_*` は `in_place_writes` の fixture に安全な関数を足したものです（値は `in_place_writes` のスイートで確かめた `array_fold_state_comes_back` など）。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 安全な形の実行: `rr1/h` の `s_*`（`72`）・`o_*`（`7`）を両方の edition（`FixedArray.init` は edition 2026）で、`rr1/c` の `c_merged_sibling`・`c_cut_sibling`（`drop 8`・`1`）を edition 2027 で、native と ASan の `-O0`・`-O3` で実行し、どれも手で求めた値で、ASan は何も報告しませんでした（`rr1/h/runs.txt`）。危険な形は `main` で実行し、指摘 1・2 は解放した文字列を読み（`drop -1`）、指摘 3 は ASan が stack use after scope を報告しました。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3288 件すべて成功、4 件 skip（1877 秒、`rr1/nextest.log`）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 81 件（この回の 2 件を含む。基準のバイナリでも 81 件が成功し、先端はこのモジュールを拒否します）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件がすべて成功（`rr1/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`rr1/step5-main.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・48 個のリンク・10 個の例・20 回の native の実行）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（3 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（2 件）、`drop.md`、`lifetimes.md`、`docs/architecture.md`、A33（`coarse` の行、この回の記録）、このチケット（「残る制限」と、この節）。

#### レビューの第2回（2026-11-14）

第1回の対応の後の先端（`b6cef060`）のレビュー（反例の探索 `target/scratch/a31/r16-review-2-ce/`、適合の確認 `target/scratch/a31/r16-review-2-conf/`）の 4 件の指摘を、修正後・先端・その前の先端（`88885b9f`）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どれも指摘のとおりに再現しました。指摘 1 は `main` も受理する前からの穴（第1回の指摘 3 と同じ種類）、指摘 2・3 は前の先端からの退行（`main` が受理する安全な形の文書に無い拒否）、指摘 4 は文書の不足です。結果は `target/scratch/a31/rr2/` にあります（`rr2/v.sh` が修正後・先端・前の先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`n/` が指摘 1 の探査（`n/gen.py` の `n/r/`）、`c/` が指摘 2 の探査（`c/gen.py`）、`w/` が指摘 3 の探査（`w/gen.py`）、`d/`・`w2/` がこの回に見つけた別の形、`g-check.mjs` が反例の探索の生成したプログラム（`r16-review-2-ce/g`）の検査です）。

##### 原因と変えたこと（`src/ownership.rs`）

- **指摘 1（組み込み関数が作る部分適用）**: `sink_flags` が環境を持つ関数値を作る所として数えるのは、ラムダ、名前付き関数の部分適用、コールバックの組み込み関数の部分適用、本体の見えない関数値の呼び出し（`staged`）で、名前付き関数は環境を持たないので数えません。`Owned.function wr`（`wr` は 2 つの仮引数の名前付き関数）を `Owned.call (ref w) cell` で呼ぶと、実行時に `wr` を `cell` に当てはめた部分適用ができて `cell` のセルを所有しますが、組み込み関数の完全適用は数える所ではなかったので、その型 `(unit -> i64) -> i64` は所有する関数値の型にならず、ブロックの局所値の借用をセルへ書き込めました。同じ形は、組み込み関数がコールバックをそれが取るより少ない引数に当てはめる所（`List.mapr wrr (ref cells)` の要素、edition 2026 の `List.foldr fold zero (ref cells)` の状態）と、`Owned.function (wr3 0)`・`Owned.function wr3` の `Owned.call` にもありました（`rr2/n/q3`・`q4`・`q7`、`n/r/u_excess` 以外の `u_*`。どのバイナリも受理し、native で解放した文字列を読んで `-1`）。`builtin_partial_applications` が、組み込み関数が受け取った関数値をそれが取るより少ない引数に当てはめうる所（スキームの関数型の仮引数のうち、その引数の数より多くを取りうるコールバック。名前付き関数・持ち上げたラムダ・名前付き関数の部分適用で、取る数が足りると分かるもの（`shown_arity`）は除きます。結果が関数の `Owned.call`。組み込み関数の仮引数より多い引数）と、その部分適用が束縛しうる値の型（ほかの引数と、部分適用した組み込み関数が後で受け取る引数）を返し、`sink_flags` はそれを本体の見えない関数値の呼び出しの結果と同じく数えます（`staged_types`。呼び出し・パイプライン・`Parallel`）。`drop_environments` も同じ所を、束縛する値で数えます。
- **指摘 2（作られない部分適用の型）**: 第1回の修正（`partial_types`）は、コードを実行する前に 2 つ以上の引数を取る関数値を作る所で、その部分適用の型を、部分適用が作られるかによらず所有・sink・holder の型にしました。そのため、`w` をいつもすべての引数に当てはめるプログラムで、参照越しに受け取った配列の中の無関係な `(unit -> i64) -> i64`（`apply_thunk`）へ局所値の借用を持つ関数を渡す呼び出しを `E1013`（`store_local_borrow_through_reference`）で拒否しました。部分適用は、関数値をそれが取るより少ない引数に当てはめる所でしか作られません。`partial_results` が、モジュールが作りうる部分適用の型を、式の種類ごとの網羅的な `match` で集めます（関数値の呼び出しとパイプラインのうち型が関数のもの、名前付き関数か組み込み関数の仮引数より多い引数、`builtin_partial_applications`、`new [T](n, f)`・`new [|T|](n, f)` の初期化の関数、コールバックを取る host 関数。ほかの種類の式は関数値を当てはめないことを名前で書いたので、新しい種類の式は、関数値を当てはめるかを決めなければコンパイルできません）。`partial_types` の型はそこにあるものだけを数えます。特殊化した本文では式の型がその所で作りうる部分適用の型なので、型で比べて足ります。
- **指摘 3（仮引数をそのまま渡す関数を通した `coarse` の場所）**: 第1回の修正で、短くした場所（`Source::coarse`）の `path_copy` の複製は exact でなく、親も持ちません。仮引数をそのまま渡す関数（`fn applyy2 k x f = applyy k x f`）の本体では、この複製が関数値の仮引数へ渡る値になり、`source_of` は `self_path`（exact でない loan は先全体）で仮引数全体を記録したので、`applyy2` の呼び出し元は `x` 全体の借用を持ち、`x.a` の外の `x.z` の書き換えとムーブを `E1014` で拒否しました（直接の呼び出し `applyy` は受理）。そうした複製（とその `shared_copy`・`ordered_copy`）を `holding_loans` に入れ、`source_of` は `holding_path` でその場所を coarse な場所として記録します。呼び出し元は直接の呼び出しと同じく、外側の場所（`x.a`、`x.a.b.c.d`）の、元を親に持たない exact でない複製を持ちます。`self_path` と、それを使う自己の事実・`kept_paths` は変えていません。
- **指摘 4（文書）**: `docs/language.md` の書き込みの事実の段落に、`match`（すべての腕）・ブロック（その結果）・部分適用どうしの選択を束縛した局所値と、組み込み関数が作る部分適用（`builtin_partial_applications`）、部分適用の型の規則（`partial_types`・`partial_results`）を書きました。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（`Owned.call` で作る名前付き関数の部分適用。前からの穴） | 再現（`n1`・`n1b`・`n1c`・`n2_owned_alias` をすべてのバイナリが両方の edition で受理。`n1b` は native で `-1`、`n1` は ASan で stack use after scope）。同じ穴を `List.mapr`・`List.foldr`・`Owned.function (wr3 0)` でも見つけた（`rr2/n`） | `builtin_partial_applications`。`n1` は `7:17`、`n1b`・`n1c` は `12:17`、`n2_owned_alias` は `15:17`、`rr2/n/r` の `u_*` は `20:17`〜`23:17` の `E1013`（`assignment_extends_borrow`）、両方の edition（`List.foldr` は edition 2026）。文字列を先に宣言した `s_*`（`0 + 8`）と所有する関数だけを書き込む `o_*`（`0 + 7`）は受理し、native と ASan の `-O0`・`-O3` で同じ値（`rr2/n/runs.txt`、30 個） | `a_builtin_that_applies_a_function_value_to_fewer_arguments_shares_the_cell` |
| 2（作られない部分適用の型。前の先端からの退行） | 再現（`c4_arr`（edition 2026）・`c5_arr27`（edition 2027）・`c3_b`（edition 2026）を先端が `9:5`・`10:5` の `E1013`、前の先端・`main`・基準は `317`） | `partial_results`。どれも受理し、`317`。部分適用を作る形（`w 3`、`3 \|> w`、`Owned.call`、`Array.init 1 w`、`new [T](1, w)`、`Maybe.map`、タプルの成分、edition 2026 の `List.map`）は `9:5`（edition 2026）・`10:5`（edition 2027）の `E1013`（`store_local_borrow_through_reference`）のまま（`rr2/c`） | `the_partial_application_types_of_an_owner_count_only_when_the_module_makes_one`、`lifetimes.md` の `run=` の例（`317`） |
| 3（仮引数をそのまま渡す関数の `coarse`。前の先端からの退行） | 再現（`w_c_z`・`w_m_z`・`w_c_mvz`・`w_m_mvz` を先端が `40:5`・`40:27` の `E1014`、前の先端・`main`・基準は受理） | `holding_loans`・`holding_path`。`x.z` の書き換え（edition 2027）・ムーブ（両方の edition）は受理し、`drop 8`・`1`。`x.a.e`・`x.a.e[0]`・`x.a`・`x` のムーブ・置き換えは `E1014` のまま（`rr2/w`、76 個） | `a_function_that_passes_its_parameters_on_keeps_the_coarse_part_of_its_callee`、`in_place_writes` の `wrapped_coarse_parts_keep_the_outside` |
| 4（文書） | 指摘のとおり（`docs/language.md` は `if` の選択だけを書き、部分適用の型の規則が無かった） | `docs/language.md` を直した | — |

指摘 3 の形は `rr2/w/gen.py` で確かめました（両方の edition。`x.z` の書き換えは edition 2027 だけが型の検査を通ります）。

| 呼び出し | `x.z` の書き換え・ムーブ、何もしない | `x.a.e`（`x.a.b.c.d.e`）・`x.a.e[0]`・`x.a`（`x.a.b.c.d`）・`x` のムーブ・置き換え |
| --- | --- | --- |
| 直接の `applyy`・`applyx` | 受理（先端も） | `E1014`（先端も） |
| `applyy2`・`applyy3`（`applyy2` をさらに渡す）・`applyyr`（`ref (deref x)` を渡す）・`applyyw`（`ref w.x` を渡す）・パイプライン `crossd (ref p2) \|> applyy2 n (ref x)`・`applyx2`・`applyx3` | 受理（先端は `E1014`、前の先端・`main`・基準は受理） | `E1014`（先端も。前の先端は一部を受理していた危険な形） |
| `f` をラムダで包んで渡す `applyyl` | edition 2027 は受理、edition 2026 は `E1013`（`self_referential_store`、先端・前の先端も。下の `fvlam`） | `E1014`（edition 2026 は `E1013`） |

受理の形は edition 2027 の native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で `drop 8`・`1` を出し、ASan は何も報告しませんでした（`rr2/w/runs.txt`。`applyyl` の形は `main` のバイナリでも実行の前にコンパイラーのスタックがあふれるので実行していません）。

##### この回の確かめで見つけたもの

- **`owned2`（格納の検査の外の穴、`main` も受理）**: `Owned.call (ref w) holder` は、`w` が 2 つ以上の引数を取る関数値（名前付き関数、その部分適用）を包むとき、コピーできない値（`Mutex` と `Drop` を持つ `Holder`）を束縛した再利用できる部分適用を作り、呼び出すたびに束縛した値をムーブします（`rr2/d/d6_owned_call_moves_bound`: 2 回目の呼び出しで ASan が heap use after free、`d4s_owned_call_holder_dropped_first` も）。関数値の呼び出しが関数を返す形は `E1005`（`capture_sync_primitive_in_function_value`）で拒否する（`polymorph::captures`）のに、`Owned.call` は組み込み関数の完全適用なので `Capture` を求めないためで、型の検査の穴です。この回の修正は、`Holder` の部分適用をセルを所有する関数値として扱うので、借用を書き込む形（`d4`、`d6` の edition 2027）は `E1013` になりますが、ムーブそのものは残ります。A33 の「未修正の穴」に別の表として記録しました。`Arc` を束縛する形は呼び出しごとに複製するので安全です（`d7_owned_call_arc_moves`、`57`）。
- **`fvlam`（文書に無い拒否、前の先端からある）**: edition 2026 で、引数で受け取った関数値を捕捉して呼ぶラムダ（`fn applypl x f = applyp x (\d -> f d)`）を渡す呼び出しは、`coarse` でない呼び出し先でも、先端・前の先端・修正後が `E1013`（`self_referential_store`）で、`main` は受理します（`rr2/w2/l_applypl`・`l_applyyl`。edition 2027 は受理）。`drop.md` の安全側の一覧、`lifetimes.md` の「現在の制限」（本体の見えない関数値の一覧）、A33 の表（`fvlam`）、上の「残る制限」に書きました。
- **`bindref`（文書に無い拒否、前の先端からある）**: 反例の探索が生成したプログラムの検査（下）で、部分適用の値が場所の借用でない引数（参照の変数 `let q1 = ref p1` の `q1`、`ref (deref q1)`、参照の仮引数）を束縛する形を、修正後・先端・前の先端が `E1013` で拒否し、`main`・基準が受理することを見つけました。`rr2/g4` の双子: `let g = cross3 q1 (ref p2); g (ref p3)`（`p3` を `p2` より後に宣言）と `let g = cross3 (ref p1) q2; g (ref p3)`（`p1` を `p2` より後に宣言）、`fn via3 a b c = { let g = cross3 a b; g c; 0 }` は `assignment_extends_borrow`、`a.s` を `b.h` へ書き込む `crossr` の `let g = crossr q1; g (ref p2)`・`let g = crossr (ref (deref q1)); g (ref p2)`・`fn via a b = { let g = crossr a; g b; 0 }` は `self_referential_store` で、両方の edition で同じです。`main` は native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で手で求めた値（`cross3` は `drop 16`・`drop 0`・`drop 0`・`1`、`crossr` は `drop 8`・`drop 0`・`1`）を出し、ASan は何も報告しませんでした（`rr2/g4/runs.txt`・`runs-extra.txt`）。場所の借用を束縛した形（`cross3 (ref p1) (ref p2)`、`crossr (ref p1)`）、すべての引数をそろえた呼び出し（`cross3 q1 (ref p2) (ref p3)`、`crossr a b`）、名前付き関数へのパイプライン（`ref p3 |> cross3 q1 (ref p2)`、`ref p2 |> crossr q1`）は受理し、同じ値を出します。部分適用の引数で `bound_value` が関数値の値から取り出すのは場所の借用の式が作った loan だけで、そうした引数は関数値の値全体（束縛したすべての引数の loan）を値全体の借用として持つためです（第1回の記録の「引数に現れても関数値全体の値を持つ」）。指す場所を記録した局所値の分岐もありますが、この形では取り出していません（原因は A33 で調べます）。安全な形の拒否なので、この回は `src/` を変えず、`drop.md`（値全体の借用の一覧と、部分適用の束縛した引数の文）、`lifetimes.md`（その場の書き込みの段落）、`docs/language.md`、A33 の表（`bindref`）、上の「残る制限」に書きました。
- `rr2/w/s_m_lam_*`（`applyyl` を使う安全な形）は、`main` のバイナリでも `tsuzuri run` がコンパイラーのスタックのあふれで止まります（検査は通ります）。A31 と関係の無い、関数値の仮引数を捕捉したラムダの lowering の問題です。

##### テスト

- `tests/rc.rs` の `a_builtin_that_applies_a_function_value_to_fewer_arguments_shares_the_cell`: 指摘 1 の 6 形（`Owned.call`、そのパイプライン、`Owned.function` の別名、`Owned.function (wr3 0)`、`List.mapr`、そのパイプライン）の拒否（`20:17`・`21:17`・`23:17` の `E1013`）と、文字列を先に宣言した形・所有する関数だけの形の受理を両方の edition で、`List.foldr` の 3 形を edition 2026 で、取る引数の数が足りるラムダのコールバックの受理。
- 同じく `the_partial_application_types_of_an_owner_count_only_when_the_module_makes_one`: 指摘 2 の受理（部分適用を作らない形、別の型の部分適用だけを作る形）と、部分適用を作る 7 形の拒否（edition 2026 は `9:5`、edition 2027 は `10:5` の `E1013` の `store_local_borrow_through_reference`）。
- 同じく `a_function_that_passes_its_parameters_on_keeps_the_coarse_part_of_its_callee`（`COARSE_PRELUDE` に仮引数を渡す関数を足したもの）: 指摘 3 の 7 つの呼び出しのムーブ（両方の edition で `49:27` の `E1014`）と置き換え（edition 2027 で `49:5` の `E1014`）、`z` のムーブ・何もしない形の受理（両方の edition）と `z` の書き換えの受理（edition 2027）。
- `in_place_writes` の `wrapped_coarse_parts_keep_the_outside`（`apply_coarse` へ仮引数をそのまま渡す `apply_coarse_on` の後で `x.z` を書き換える。`coarse_pairs` は `0` と新しい `x.z` の `"123"` の長さ 3 を返し、`p1` の記録は `x.a.e[0]` の `"12345"` の 5 + 1 なので、手で求めた値は `3 * 10 + 6 = 36`）。基準のバイナリでも同じ値で、先端はこのモジュールを `E1014` で拒否します。指摘 2 の安全な形は、このモジュールではほかの関数が同じ型の所有する関数値を作るので（文書に書いた型の規則）、`lifetimes.md` の `run=` の例（`check-docs` が native の `-O0`・`-O3` で `317` を確かめます）にしました。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r20.mjs`・`diff-r20.mjs`・`table-r20.mjs` に写し（基準と先端（`r19`、`b6cef060` のビルドで `423592d8` と同じソース）の結果を `results-r19.json` から使い、`r19` に無いプログラムは先端でも検査）、`gate/tools/collect.mjs` でこのレビューのプログラム（`r16-review-2-ce`・`r16-review-2-conf`）とこの回の探査（`rr2/`）など 476 個を足して、13892 個・27784 組を両方の edition で検査しました（`results-r20.json`、分類は `diff-r20.txt`、`rr2/gate-diff-r20.txt` に写し）。反例の探索が生成した 15552 個（`r16-review-2-ce/g`: `cross`・`crossr`・`cross3` の呼び出しの形と宣言の順の組み合わせで、どれが安全かを `meta.json` が持ちます）は、数が多いので別に `rr2/g-check.mjs` で検査しました（下）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4657 | 3669 |
| 両方が拒否 | 5134 | 6916 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4063 | 3278 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、どれも先端も受理するもの（前回と同じ 67 組）です。新しい RA はありません。** 条件 (1) は成り立ちます。
- **前からある 26832 組では、先端からの受理・拒否の変化も、最初の誤りの変化もありません。**
- 足した 952 組での先端からの変化は、受理から拒否の 30 組（指摘 1 のレビューの形 `n1`・`n1b`・`n1c`・`n2_owned_alias`、`rr2/n` の `q3`・`q4`・`q7` と `n/r/u_*`、`rr2/d` の `d4`・`d4s`・`d6`（`d4s`・`d6` は edition 2027）。どれも基準が受理する危険な形）、拒否から受理の 35 組（指摘 2 の `c3_b`・`c4_arr`・`c5_arr27`、指摘 3 の `w_c_z`・`w_m_z`・`w_c_mvz`・`w_m_mvz`、`rr2/c` の `s_other_partial_*`、`rr2/doc`（`lifetimes.md` の例）、`rr2/w` の `s_*`。どれも基準が受理する安全な形）、拒否のまま最初の誤りが変わった 3 組（`r16-review-2-ce/o/o_owned_unsafe` の両方の edition が、同じ位置で `self_referential_store` から `assignment_extends_borrow` に。基準が受理する危険な形で、`Owned.call` の部分適用をセルを所有する関数値として扱うようになったため。`rr2/w/s_m_lam_mvz` の edition 2026 は、2 つ目の誤りが無くなっただけ）です。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 安全な形の実行: `rr2/c` の `s_*` と `c3_b`・`c4_arr`・`c5_arr27`（`317`）、`r16-review-2-conf/p/w_*`（`drop 8`・`1`）を native と ASan の `-O0`・`-O3` で実行し、どれも手で求めた値で、ASan は何も報告しませんでした（`rr2/c/runs.txt`。`rr2/n/runs.txt`・`rr2/w/runs.txt` は上の表のとおり）。`rr2/w/s_m_lam_*`（edition 2027）は、`main` のバイナリでもコンパイラーのスタックがあふれて実行できないので、実行していません（`s_m_wrap_*` と同じ借用の形です）。
- 生成したプログラム（`rr2/g-check.mjs`、15552 個・31104 組。結果は `rr2/g-results.json`・`g-summary.json`）: 危険な 18048 組（9024 個、両方の edition）はどれも拒否しました（条件 (2)）。安全な 13056 組のうち 9468 組を受理し、3588 組（両方の edition で 1794 組ずつ）を拒否しました。拒否した 3588 組は、どれも先端（`b6cef060`）も拒否し、`main`（`168ae4a5`）・基準は受理します（`rr2/g-main.mjs`、`rr2/g-main.json`）。どれも、部分適用の値（生成の `chosen`）が場所の借用でない引数（参照の変数 `qN`、`ref (deref qN)`）を束縛する同じ 1 つの形です: `crossr` の書き込む値の側に束縛した 1872 組（`alias`・`if`・`match`・ブロックなどのすべての包み方と、直接・パイプライン・`apply`・別名のすべての呼び出しの形）、`cross3` に 2 つを束縛した 1248 組（すべての呼び出しの形）、`cross3` に 1 つを束縛して残りの 2 つを部分適用・パイプラインで 1 つずつ渡す 468 組（2 つ目に束縛した引数が最後の引数より先に宣言されたもの）。同じ形を `ref pN` で束縛したもの、すべての引数をそろえた呼び出し、名前付き関数へのパイプラインは受理します。前の先端からある安全側の制限で、文書に無かったので、文書に書きました（下の「この回の確かめで見つけたもの」の `bindref`）。条件 (3) の、`main` が受理する安全な形の拒否は、どれも文書に書いた安全側の制限です。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3291 件すべて成功、4 件 skip（2306 秒、`rr2/nextest-3.log`。この回の 3 件を含みます。最初の実行 `rr2/nextest-2.log` は 2537 件の所で SIGTERM で止められたので、すべてやり直しました）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 82 件（この回の 1 件を含む。基準のバイナリでも 82 件が成功し、先端はこのモジュールを拒否します）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`rr2/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`rr2/step5-main.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・48 個のリンク・11 個の例・22 回の native の実行。`bindref` を書いた後にもう一度、`rr2/docs-3.log`）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功（`rr2/doc-checks-3.log`）。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（3 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（`owned2`・`fvlam`・`bindref` の行、段 3 の一覧、この回の記録）、このチケット（「残る制限」と、この節）。

#### レビューの第3回（2026-11-14）

第2回の対応の後の先端（`231404d0`、その前のレビューの先端は `b6cef060`）のレビュー（反例の探索 `target/scratch/a31/r17-review-3-ce/`、適合の確認 `target/scratch/a31/r17-review-3-conf/`）の 2 件の指摘を、修正後・先端・前の先端（`b6cef060`）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どちらも指摘のとおりに再現しました。どちらも `main`・基準が受理する安全な形の文書に無い拒否で、前の先端も同じく拒否します（前の先端からの退行ではありません）。反例の探索は危険な形の受理を報告しませんでした。結果は `target/scratch/a31/rr3/` にあります（`rr3/v.sh` が修正後・先端・前の先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`f1/` が指摘 1 の再現、`f2/` が指摘 2 の再現、`t/` がこの回の双子（`t/gen.py`）、`fx/` が fixture の実験、`g-check.mjs` が第2回の反例の探索が生成したプログラムの検査です）。

##### 原因と変えたこと（`src/ownership.rs`）

- **指摘 1（仮引数を部分適用で渡す関数）**: `fn applyyq k x f = { let g = applyy k x; g f }` の本体では、`g f` の位置で `g` の事実（`applyy` の事実に束縛した引数を先に並べたもの）を当てはめます。束縛した引数 `x` は `ref (deref x)` の借用の式で、部分適用の値から取り出すその引数の値（`bound_value`）は、その借用の式が作った loan（`borrow_loans`）でした。ただし `bound_value` は、借用の場所が exact か（`exact_place`）を、関数値を呼ぶ `g f` の位置の状態でもう一度求めていました。そこでは `x` は最後の使用（`applyy k x`）を過ぎて生きておらず、局所値の値が空なので `pointing_local` が偽になり、束縛した引数は部分適用の値全体を持つ exact でない値になりました。`path_copy` は exact な写しも、借用した部分を持つ写し（`holding_loans`）も作らず、`source_of` は仮引数全体を記録したので、`applyyq` の呼び出し元は `x` 全体の借用を持ちました。第2回で文書に書いた `bindref`（`let g = cross3 q1 (ref p2); g (ref p3)`、`fn via3 a b c = { let g = cross3 a b; g c; 0 }`）も同じ原因です（`q1` も `ref (deref q1)` として束縛され、`q1` は `g (ref p3)` の位置で生きていません）。借用の式を評価した所で場所が exact でなかったものを `inexact_borrows` に記録し、`bound_value` は、そこに無い借用の式なら、評価した所で exact だった loan（`borrow_loans` はそうした評価の loan だけを持ちます）を、関数値を呼ぶ所の状態によらずその引数の値とします。exact でない評価が 1 つでもあった式は、今までどおり関数値の値全体です。loan の場所は借用を作った所で決まり、その後に借用元の参照が生きていなくても変わらないので、借用を作った所で exact だった loan は exact なままです。
- **指摘 2（仮引数を捕捉するラムダで渡す関数）**: ラムダを呼ぶ位置では、持ち上げたラムダの事実に捕捉を先に並べて当てはめますが、捕捉は狭めず（`bound(.., false)`）、どの捕捉もラムダの値全体（すべての捕捉の loan）を持っていました。`fn outerl a b = (\(_u: unit) -> cross a b) ()` では `a` が `b` の借用も持つので、`b.s` の借用を `a.h` へ書き込む事実が `p1` の借用を `p1` 自身へも入れ（`E1013`）、関数値の仮引数を捕捉する形では `x` が `f` の借用を、`f` が `x` の借用を持つので、高階の事実の引数が `f` を含み、呼び出し元は `crossd (ref p2)` の借用を `p2` 自身へ入れました。部分適用の引数と同じく捕捉も `bound_value` で狭めます。捕捉した局所値はラムダを呼ぶ所ではもう生きていないことがあるので（`let g = \(_u: unit) -> ...; g ()`）、ラムダを作った所（`E::Closure` の評価）で、捕捉した不変の局所値がその loan の場所を指していたか（`pointing_local`）を記録し（`note_capture`、`captured_loans`。指していなかった評価は `inexact_captures`）、`bound_value` はそれを使います。捕捉した局所値は不変で、ラムダより前に束縛されたものなので、ラムダが持つ値はラムダを作った所の値です。
- **edition 2026 で関数値の仮引数を捕捉するラムダ（残る制限 `fvlam`）**: 上の修正の後も、edition 2026 では `applypl`（第2回の `fvlam`）、`applyyc`・`applypd`・`applypn` などは `E1013`（`self_referential_store`）のままです。第2回では、ラムダの中の捕捉した関数値の呼び出しが高階の事実として呼び出し元へ届かないためと書きましたが、事実を出して確かめると、高階の事実は届いていて、原因は別でした: edition 2026 のラムダは関数値の仮引数を値で捕捉して所有するので、本体の中でのラムダの解放（`drop_stores`）が、関数値の環境のコードを実行しうるものとして、ラムダが持つ loan（`x` と `f` の loan）を関数値の環境へ格納し（`applypd` の事実の `(0, Value, 1)`・`(1, Value, 1)`、`applypl` は `(1, Value, 1)`）、呼び出し元は `crossd (ref p2)` の借用を `p2` へ入れます。解放した値は本体が作ったラムダで、仮引数から受け取った値そのものではないので、呼び出し元の実引数が解放でコードを実行しないときだけ当てはめる `StoreFacts::drop_only` になりません。edition 2027 の読み取りの仮引数は呼び出し元の関数値を借りたもので、ラムダの解放は関数値を解放しないので、この格納はありません。直すには、本体が作った値の解放の格納を、その値が持つ仮引数の値の解放として `drop_only` に記録する必要があり、この回では直さず、文書に原因を直して書きました（下）。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（仮引数を部分適用で渡す関数。`main` が受理する安全な形の拒否） | 再現（`q/s_yq_z`・`s_yq_mvz`・`s_pq_*`、`x/a_p0q`・`a_p0q2`・`a_p0q3` を修正前・前の先端が `E1014`（`borrow_conflict`）、`main`・基準は受理して `drop 8`・`1`、`a_p0q` は `drop 8`・`3`） | `inexact_borrows`。どれも受理（edition 2026 は `x.z` の書き換えが型の検査で `E1012` の形を除いて同じ）。`x.a.e` のムーブ（`q/u_*`、`t/ux_p0q_*`）・置き換えは `E1014` のまま。第2回の `bindref` の双子（`rr2/g4` の 16 個）も両方の edition で受理 | `a_partial_application_or_a_lambda_that_passes_parameters_on_keeps_the_part`、`a_lambda_or_a_partial_application_over_reference_parameters_keeps_each_part`（`via3`）、`in_place_writes` の `passed_on_parts_keep_the_outside` |
| 2（仮引数を捕捉するラムダで渡す関数。同上） | 再現（`x/c_outerl` を修正前・前の先端が両方の edition で `E1013`（`assignment_extends_borrow`）、`x/a_p0d`・`a_p0l`・`l/s_pc`・`s_pg`・`s_pd`・`s_pn`・`w/s_c_*` を `E1013`（`self_referential_store`）。`main` は手で求めた値） | 捕捉の狭め（`bound(.., true)`、`captured_loans`）。edition 2027 ではどれも受理、`c_outerl` は両方の edition で受理。関数値の仮引数を捕捉する形の edition 2026 は `fvlam` の制限（上）で `E1013` のまま | 同上 2 件と `in_place_writes` の同じケース（`apply_coarse_lambda`） |
| 文書 | `drop.md`・`lifetimes.md`・A33 の `fvlam` は edition 2027 で届くと書き、`drop.md` は仮引数をそのまま渡す関数とラムダを直接の呼び出しと同じと書いていた | 部分適用・ラムダで渡す関数の受理を書き、`bindref` の一覧を消し、`fvlam` の原因と形を直した（`drop.md`、`lifetimes.md` の「現在の制限」、`docs/language.md`、`docs/architecture.md`、A33、上の「残る制限」） | — |

この回の双子（`rr3/t/gen.py`、両方の edition）:

| 形 | 安全な順序 | 逆の順序 |
| --- | --- | --- |
| すぐに呼ぶラムダ・束縛してから呼ぶラムダで `cross a b`・`cross b a`（`lam_*`・`lamg_*`）、`if` で選んだ参照を捕捉（`lamif_*`・`lammix_*`）、部分適用を捕捉したラムダ（`lampart*`）、部分適用（`part_*`・`part3r_*`） | 受理（`drop 8`・`drop 0`・`1`、`part3r_21` は `drop 8`・`drop 8`・`drop 0`・`1`） | `27:5` の `E1013`（`assignment_extends_borrow`。`main` は解放した文字列を読んで `drop -1`） |
| `cross3 a b c` に 2 つを束縛して最後の引数を後で渡す `part3`（どちらの順でも危険） | — | `27:5` の `E1013` |
| 指摘 1・2 の形で `x.z` をムーブ（`sx_*`） | 受理（`drop 8`・`2`。ラムダで関数値の仮引数を捕捉する 2 つは edition 2027 のみ） | `x.a` のムーブ・置き換え（`ux_*`）は `E1014`（edition 2026 のラムダの形は `E1013`）。`main` は `drop -1` |

受理の形は edition 2027 の native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で手で求めた値を出し、ASan は何も報告しませんでした（`rr3/t/runs-2027.txt`。危険な形の `main` での実行は `rr3/t/runs-unsafe-main.txt`）。

##### テスト

- `tests/rc.rs` の `a_partial_application_or_a_lambda_that_passes_parameters_on_keeps_the_part`（`COARSE_PRELUDE` に、部分適用で渡す `applyyq`・`applypq`・`applypr` と、ラムダで渡す `applyyc`・`applypn` を足したもの）: `x.a.e` のムーブ（`48:27` の `E1014`。部分適用の形は両方の edition、ラムダの形は edition 2027）と置き換え（edition 2027 で `48:5` の `E1014`）、`x.z` のムーブ・何もしない形の受理（部分適用は両方の edition、ラムダは edition 2027。edition 2026 のラムダの形は `45:5` の `E1013`（`self_referential_store`）で、`fvlam` の制限を固定します）、`x.z` の書き換えの受理（edition 2027）。
- 同じく `a_lambda_or_a_partial_application_over_reference_parameters_keeps_each_part`（`CROSS_PRELUDE` に `cross3`・`outerl`・`outerg`・`via3`・`mixl` を足したもの）: 安全な順序の 4 つの受理と、逆の順序の 4 つの拒否（両方の edition で `45:5` の `E1013`（`assignment_extends_borrow`））。
- `in_place_writes` の `passed_on_parts_keep_the_outside`（`apply_coarse` へ部分適用で渡す `apply_coarse_partial` と、ラムダで渡す `apply_coarse_lambda` の後で `x.z` を書き換える。`passed_pairs` は `0 + 0` と新しい `x.z` の `"1234"` の長さ 4 を返し、`p1`・`p2` の記録は `x.a.e[0]` の `"12345"` の 5 + 1 なので、手で求めた値は `4 * 100 + 6 * 10 + 6 = 466`）。基準のバイナリでも同じ値で、先端はこのモジュールを `E1013` で拒否します。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r21.mjs`・`diff-r21.mjs`・`table-r21.mjs` に写し（基準と先端（`r20`、`231404d0` のビルド）の結果を `results-r20.json` から使い、`r20` に無いプログラムは先端でも検査）、`gate/tools/collect-r21.mjs`（`collect.mjs` から、第2回の生成したプログラム `r16-review-2-ce/g` を除いたもの）でこのレビューのプログラム（`r17-review-3-ce` の 167 個、`r17-review-3-conf` の 127 個）とこの回の探査（`rr3/` の 38 個）、第2回の後に足した探査（`rr2/` の 28 個）の 360 個を足して、14252 個・28504 組を両方の edition で検査しました（`results-r21.json`、分類は `diff-r21.txt`、`rr3/gate-diff-r21.txt` に写し）。第2回の反例の探索が生成した 15552 個は、数が多いので別に `rr3/g-check.mjs` で検査しました（下）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 4813 | 3773 |
| 両方が拒否 | 5160 | 7043 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4241 | 3407 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、前回と同じ 67 組です。新しい RA はありません。** 条件 (1) は成り立ちます。
- **受理から拒否に変わった組はありません。**
- 前からある 27784 組での先端からの変化は、拒否から受理の 16 組と、拒否のまま最初の誤りが変わった 2 組です。拒否から受理の 16 組は、第2回の反例の探索の `r16-review-2-ce/k/` の安全な形 8 個（`s_blocklam`・`s_blockq2`・`s_blockq2_alias`・`s_blockq2_pipe`・`s_derefq`・`s_nestblock`・`s_shadow`・`s_shadow2`。ブロックの中の `let q = ref p2` を束縛した `crossr q` など、束縛した参照が呼ぶ所で生きていない部分適用で、指摘 1 と同じ原因）の両方の edition で、どれも基準が受理し、逆の順序の `u_*`（22 個）は両方の edition で `E1013` のままです。最初の誤りが変わったのは `_adv/x10_iter_cb`（局所値 `text` の借用を `Mutex` へ書き込んで、`Mutex.into_inner` の結果を返す危険な形）の両方の edition で、拒否のまま、`rm` の捕捉が `b` の借用を持たなくなったので `m` 自身へ `m` の借用が入らず、1 つ目の誤り（`8:32` の `E1014`）が無くなり、2 つ目の誤り（`9:5` の `E1013` の `borrow_outlives_block`、`text` の借用がブロックの外へ出る、本当の誤り）が最初になりました。
- 足した 720 組での先端からの変化は、拒否から受理の 67 組（指摘 1・2 のレビューの形、`r17-review-3-ce/w/s_xF_z`（指摘 1 と同じ形）、第2回の `bindref` の双子 `rr2/g3/a_*`・`rr2/g4/*`、この回の双子 `rr3/t/` の安全な形。どれも基準が受理する安全な形）と、拒否のまま最初の誤りが変わった 8 組（`r17-review-3-conf/p/w/u_c_*` の 4 個と `rr3/t/ux_p0l_*`・`ux_p0n_*` の 4 個の edition 2027。どれも基準が受理する危険な形で、`E1013` の `self_referential_store` から、借用された `x.a` のムーブ・置き換えの位置の `E1014` に変わりました）です。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 安全な形の実行: 拒否から受理に変わったこの回の双子以外のプログラム（`k/s_*` 8 個、`rr2/g3/a_*` 5 個、`s_xF_z`、`r17-review-3-conf` の `q/s_pq_*`・`s_yq_*`・`w/s_c_*`・`s_q_*`・`x/a_*`・`c_outerl`・`l/s_pc`・`s_pd`・`s_pg`・`s_pn`、32 個を edition 2027 で、そのうち edition 2026 で受理する 17 個を edition 2026 で）を、native と ASan の `-O0`・`-O3` で実行し、どれも手で求めた値を出し、ASan は何も報告しませんでした（`rr3/runs-other-2027.txt`・`runs-other-2026.txt`。`k/s_*` は `p1` が `p2.s` を読む `drop 8`・`drop 0`・`1`、`rr2/g3/a_*` は `drop 16`・`drop 0`・`drop 0`・`1`、`l/s_*` は `x.a.s` が `"abcabc"` なので `drop -1`・`1`）。
- 前の先端から拒否していて文書に無かった安全な形: `r16-review-2-ce/k/` の `s_matchq`・`s_matchq_pipe`・`s_matchtup`・`s_nestmatch`（`match ref p2 with | q -> crossr q` のように、`match` の対象に書いた借用を束縛したパターンの変数。直接の呼び出し `crossr q (ref p1)` も拒否します: `rr3/k2/v_match_direct`）と `s_blockfn`・`s_blockfn2`・`s_blockfn_part`（`let chosen = { let f = crossr; f (ref p2) }` のように、ブロックの中で束縛した関数値の別名を通した部分適用）は、修正後・先端・前の先端が `E1013` で拒否し、`main` は `drop 8`・`drop 0`・`1` を出します（`rr3/k2/main-runs.txt`）。どちらもこの回の変更と関係の無い、前からの安全側の制限で、`drop.md` の安全側の一覧、`lifetimes.md` の「現在の制限」、A33 の表（`matchref`・`blockalias`）、上の「残る制限」に書きました。`s_blockmutq`（`let mut` の参照の変数、`mutref`）は文書にある制限で、`s_blockmutq2` は後で宣言した `p3` の借用を `p1` が持つので解放した値を読む形、`s_matchn_q` は `n` が 0 の腕で同じく危険な形です（`run 1` は安全な腕を通ります）。
- 生成したプログラム（`rr3/g-check.mjs`、15552 個・31104 組。結果は `rr3/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました（第2回の後は 3588 組を拒否していました。どれも `bindref` の形です）。

条件 (3) の、`main` が受理する安全な形の拒否は、どれも文書に書いた安全側の制限です。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3293 件のうち 3292 件が成功、4 件 skip（1550 秒、`rr3/nextest.log`。この回の 2 件を含みます）。失敗した 1 件 `diagram_cli::kg_diagram_no_refresh_without_a_kg_is_e2009`（KG の図の CLI、所有権の検査と関係の無いテスト）は、負荷の高い中での KG の構築の失敗で、単独で 3 回実行してどれも成功しました。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 83 件（この回の 1 件を含む。基準のバイナリでも 83 件が成功し、先端はこのモジュールを拒否します）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`rr3/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`rr3/step5-main.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・49 個のリンク・11 個の例・22 回の native の実行、`rr3/docs.log`）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功（`rr3/doc-checks.log`）。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（2 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（`bindref` の行を消し、`fvlam` の行を直し、`matchref`・`blockalias` の行、段 3 の一覧、この回の記録）、このチケット（「残る制限」と、この節）。

#### レビューの第4回（2026-11-14）

第3回の対応の後の先端（`dff92288`・`7ccca2e7`、その前のレビューの先端は `231404d0`）のレビュー（反例の探索 `target/scratch/a31/r18-review-4-ce/`、適合の確認 `target/scratch/a31/r18-review-4-conf/`）の 6 件の指摘を、修正後・先端（`dff92288`）・前の先端（`231404d0`）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どれも指摘のとおりに再現しました。指摘 1 は前の先端からの退行（危険な形の受理）、指摘 2 は `main` も受理する前からの穴で、原因は同じ 1 つでした。指摘 3〜6 は `main`・基準が受理する安全な形の文書に無い拒否で、前の先端も同じく拒否します。この回からの規則（前の先端からの退行でない安全な形の拒否は `src/` を変えず、保守的な制限として文書に書く）に従い、文書に書きました。結果は `target/scratch/a31/rr4/` にあります（`rr4/v.sh` が修正後・先端・前の先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`t/` がこの回の双子（`t/gen.py`）、`x/` が共有の別の作り方の探査（`x/gen.py`）、`p/` が共有の元の持ち主のムーブの探査です）。

##### 原因と変えたこと（`src/ownership.rs`）

- **指摘 1・2（`Rc` の共有を通した格納）**: `let r = Rc.share (ref r2)` の値は `r2` のブロックの共有の loan（`Loan::share`、場所の根は `r2`）を持ち、`Rc.get (ref r)` を通した格納は、`apply_store_within` が `r` に入れた後、`r` が持つ loan のうち書き込み先になりうるもの（`store_target`）をたどって `r2` にも入れていました。ところが、破棄でコードを実行しうる局所値のうち本文がもう使わないものは、束縛のたびに `keep_for_drop` が共有の loan を元の所有者が今持つ loan に読み替え（`expand_shares`。元の所有者はムーブしうるため）、`r` の値から `r2` への道が消えます。`let a = Rc.get (ref r)` の後は `r` を使わないので、`a` を通した格納（`cross a (ref p)`、呼び出しの無い `Mutex.with_lock (ref a.h.cell) ...`、部分適用・パイプライン・`a` を渡す関数）は `r` だけに入り、`r2` は `p` の借用を持たず、`p` の後で `r2` がブロックを解放してブロックの `Holder` が解放した `p.s` を読む形を受理していました（`main` は `drop -1`。文字列をヒープのブロックに置く形は ASan が heap use after free）。一時値の `cross (Rc.get (ref r)) (ref p)` は格納の時点で `r` を使うので共有の loan が残り、拒否していました。第3回のラムダの捕捉の狭めは、捕捉した `a` に自分の loan（`r` の借用）だけを持たせたので、ラムダの形もこの道を通るようになりました（前の先端は捕捉がラムダの値全体を持ち、`p` が `r` の借用を持つという別の理由で拒否していました）。共有をパイプライン（`ref r2 |> Rc.share`）で作った形、フィールドや要素に入れた形（`RH { rc: Rc.share (ref r2) }`、`[Rc.share (ref r2)]`）も同じで、同じブロックの自分の部分 `s` を `PairBad` の `h` へ書き込む形（`let r2 = ref r |> Rc.share` の後の `crossb (Rc.get (ref r2)) (Rc.get (ref r))`）も、格納が `r2` だけに入り、`r2` を `r` と別の値として扱う（`block_shares` は直接の `Rc.share (ref r)` だけを記録します）ので受理していました。
  `keep_for_drop` が局所値の共有を読み替えるとき、その共有の元の所有者（共有の共有もたどる、`share_sources`）へ、局所値の型が届くブロックの型で `export_block(id, ExportTarget::Local(source))` を足します。以後、その型のブロックを持ちうる局所値への格納は、`store_exported` が元の所有者へも届けます（読み替えの逆の向き `export_block(source, ExportTarget::Local(id))` は前からありました）。型で判定するので、同じ型のブロックを持つ別の局所値への格納も元の所有者へ届きます（安全側。`r` を使い終えた後で、`p` より後に宣言した同じ型の別のブロック `q` へ `p.s` の借用を書き込む安全な形 `rr4/p/s_unrelated_after`・`s_unrelated_local` も、`r2` が `p` の借用を持つものとして `26:5` の `E1013` になります。`main`・先端は受理し、`drop 8`・`drop 0`・`drop 0`・`1`（native と ASan の `-O0`・`-O3`、ASan は報告なし）。この回の規則のとおり保守的な制限として、`drop.md`・`lifetimes.md`・A33 の表（`sharetype`）・「残る制限」に書きました）。
  局所値ごとの正確な道（読み替えた共有の loan を記録し、`apply_store_within` が持ち主の loan と同じくたどる）も試しましたが、`Rc.share` の連なり（A33 の「検査の時間」の `gen_chain.py cellshares`）では、格納が共有の loan を連なりの持ち主へ吸収させて読み替えが繰り返されるので、道の数が連なりの長さに比例して増え、検査が 3 乗になりました（n = 400 で 4491 ms、800 で 37073 ms、1600 で 300 秒を超える）。型の判定の修正も同じ連なりで 2 乗のまま定数がおよそ 2 倍になります（同じ負荷の機械で続けて測った所有権の検査: n = 400 で 273 ms（先端 138 ms）、800 で 1210 ms（550 ms）、1600 で 5109 ms（2418 ms）。`rr4/chain-times.txt`。`expand_shares` の走査を使い回し、局所値と所有者の組ごとに 1 度だけ `export_block` する形にしても変わらず、増えたのは所有者への早い出力による `store_exported_into` の呼び出しと届け先でした）。現実のプログラムの形ではないので、型の判定の修正を採り、A33 の「検査の時間」に書きました。
  この回の規則は、前の先端からの退行には、それを入れた緩めを戻すか狭めることを求めます。ただし指摘 2 のとおり、同じ穴は捕捉を狭めない形（局所値を直接渡す呼び出し、呼び出しの無い書き込み）にもあり、捕捉の狭めを戻しても塞がりません。そこで原因の格納の規則を締め、ラムダの形を含むすべての形を拒否しました。捕捉の狭めは、部分適用と局所値の形と同じ規則（`points_at`）のまま残しています（緩めも特別な扱いも足していません）。
- **指摘 3〜6**: 文書に書きました（下の表と「残る制限」）。原因はレビューの読みのとおりで、指摘 3・4 は `place_type` を読んで確かめました。指摘 3・4 は、フィールドや要素の場所の `Rc` の借用（`ref rs[0]`、`ref x.a.e[0]`）が、`place_type` が `Rc` を読み通してブロックの中の値の型を返すので借用の型（`Rc<PairOk>`）と合わず、`exact_loans` に入らないこと（`E::Borrow` の `typed`）です。変数全体の `ref r` は場所にフィールドが無いので exact です。捕捉・束縛した引数は `bound_value` が `None` で関数値の値全体を持ち、書き込む値の側の実引数は持ち主全体の借用として書き込み先に入り、その後の格納が持ち主のセルへ届きます。指摘 5 は、`Checker::drop_stores` が破棄を `drop_only` にするのが `parameter_part`（不変で参照でない仮引数とその部分）の破棄だけで、パターンの変数（型注釈を書いたラムダの仮引数は `\$lambdaN -> match $lambdaN with h -> ...` に下ろされます）は仮引数の根でなく、edition 2027 の読み取りの仮引数は参照の型なので `apply_facts_once` の `dropping_part` も `None` になることです。指摘 6 は、呼び出しが場所でない参照の実引数を `ref (deref q1)` として再借用し、`Checker::loan` が根の局所値の loan（`q1` が持つ `p1` 全体の借用）を親に足し、`kept_copy` の写しが元の親を持つことです。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（ラムダの捕捉を通した `Rc` の共有への格納。前の先端からの退行） | 再現（`u_rc_lam`・`u_rc_lam_direct`・`u_rcf_lam`・`u_rc_pipe_share_lam`・`u_arr_lam`・`u_rcp_outerg_loc`・`u_rcp_outerl_loc`・`u_rcself_bad_pipe_lam2` を先端が両方の edition で受理し、前の先端は `E1013`、`main`・基準は受理） | `share_sources`（上）。どれも両方の edition で `E1013`（`assignment_extends_borrow`、`u_rcself_bad_*` は `self_referential_store`） | `a_store_through_one_owner_of_a_block_reaches_the_other_owners` |
| 2（同じ原因の前からの穴） | 再現（`u_rc_inline_local`・`u_rc_local_direct`・`u_rc_part`・`u_rc_partb`・`u_rc_pipe`・`u_rcf_part`・`u_rcp_outer_loc`・`u_rcp_outerq_loc`・`u_rcself_bad_pipe_loc2`・`u_rcself_bad_pipe_part2`・`u_rcself_bad_pipe_inline2`・`u_rcself_bad_arr_loc` と `_heap` の形を、どのバイナリも受理） | 同上。どれも両方の edition で `E1013` | 同上 |
| 3（捕捉・ほかの引数と一緒に束縛した `Rc.get (ref x.a.e[0])`。`main` が受理する安全な形の拒否） | 再現（`q/C3_lam_b_only_a_bound` を修正後・先端・前の先端が両方の edition で `39:5` の `E1013`（`self_referential_store`）、`q/C7_bind2_local` は `41:5`、`p/s_arr_rc_src_lam`・`s_fld_rc_src_lam` は `27:5` の `assignment_extends_borrow`。`main` は `drop 8`・`1`） | 文書（`rcgetpart`） | — |
| 4（書き込む値の側の `Rc.get (ref rs[0])` の局所値。同上） | 再現（`p/s_src_rcarr_local`・`s_src_rcfld_local`・`s_src_rcarr_local_pipe`・`s_arr_rc_src_part` を両方の edition で `27:5` の `E1013`（`self_referential_store`）。`main` は `drop 8`・`drop 0`・`1`） | 文書（`rcgetpart`） | — |
| 5（パターンの変数・型注釈を書いたラムダの仮引数・読み取りの仮引数を通した関数値の破棄。同上） | 再現（`d/s_matchnamed_run` の `21:5`、`d/s_unused`・`s_min` の `19:5`、`b/s_xlamarg` の `39:5`、`c/s_lamarg` の `41:5` の `E1013`（`self_referential_store`）は両方の edition、`d/s_min_untyped`・`s_ownnamed`・`k/v4_cross_call` は edition 2027 だけ。`main` は `drop 0`・`drop 0`・`1`、`drop 8`・`drop 0`・`1`） | 文書（`fvdrop`）。`lifetimes.md` の「関数値の仮引数として扱います」は呼び出しの扱いだと書き、`fvlam` の「edition 2027 は受理」はラムダが捕捉する形だと書きました | — |
| 6（書き込む部分の側の参照の変数が値全体を生かす。同上） | 再現（`e/w_q1` の `21:5`、`w_if` の `24:5`、`w_q1_reborrow`・`w_q1_pipe`・`w_q1_part`・`w_src_part`、`s_direct` の `23:27`・`s_part_local` の `24:27` の `E1014`（`borrow_conflict`）。`main` は `drop 0`・`drop 8`・`drop 5`・`1`、`drop 0`・`drop 8`・`1`。`w_fresh`・`w_q1_lam` は修正後も受理） | 文書（`refsrc`）。`drop.md` の「借用されていない `p1.h` は書き換えられ」と、第3回の文書の束縛した参照の変数の文に、書き込む部分の側では値全体を生かすと書きました | — |

この回の双子（`rr4/t/gen.py`、両方の edition）:

| 形 | 安全な順序（`p` を先に宣言） | 逆の順序（`p` をブロックの最初の持ち主より後に宣言） |
| --- | --- | --- |
| 直接の共有 `Rc.share (ref r2)` の `Rc.get (ref r)` を束縛した `a` を、束縛してから呼ぶラムダ・すぐに呼ぶラムダ・直接の呼び出し・呼び出しの無い `Mutex.with_lock`・部分適用・パイプライン・`outer`・`outerg`・`outerl`・`outerq` に使う形と、一時値の `Rc.get (ref r)` | 受理（`drop 8`・`drop 0`・`1`） | `40:5` の `E1013`（`assignment_extends_borrow`） |
| パイプラインの共有・フィールドに入れた共有・要素に入れた共有の同じ 11 形 | 受理 | 同上 |
| 書き込んだ後でブロックを前に宣言した変数へムーブ（`hold = r2`） | 受理（`drop 0`・`drop 8`・`drop 0`・`1`） | 同上（`'hold' keeps a borrow ...`） |
| 同じブロックの自分の部分 `s` を、別の持ち主（直接・パイプライン・要素の共有）を通して `h` へ（直接の呼び出し・ラムダ・部分適用） | `PairOk` は受理（`drop 8`・`1`） | `PairBad` は `40:5` の `E1013`（`self_referential_store`） |

安全な順序の 54 個はどれも先端と同じく両方の edition で受理し、逆の順序の 54 個はどれも両方の edition で拒否しました（先端はそのうち 46 個を両方の edition で受理していました。拒否していたのは一時値の 4 形と、直接の共有を通した `PairBad` の 3 形、要素の共有を通したラムダの形です）。安全な形のうち 24 個（直接の共有の 11 形、各共有のラムダ、自分の部分の 9 形、ムーブ）を edition 2027 の native の `-O0`・`-O3` と ASan の `-O0`・`-O3` で実行し、どれも手で求めた値で、ASan は何も報告しませんでした（`rr4/t-runs-2027.txt`）。共有の別の作り方（`rr4/x/`: `Maybe` に入れた共有、弱参照の `Rc.upgrade`、共有を値で受けて返す関数、`Rc.get` を返す関数、`if` で選んだ共有、内側のブロックで作った共有、ループの中の格納）も、安全な順序はどれも受理し、逆の順序はどれも拒否しました。そのうち共有を値で受けて返す関数・`Rc.get` を返す関数・`if`・内側のブロック・ループ（edition 2026）の形は、先端と `main` が受理する危険な形で、この修正で拒否になりました。書き込んだ後で共有の元の持ち主をムーブする `rr4/p/u_mvsrc_after_store`（`main` は `drop -1`）も同じです。共有が生きている間に元の持ち主をムーブする形（`rr4/p/u_mvsrc_*`）は、どのバイナリも `E1014` です。

##### テスト

- `tests/rc.rs` の `a_store_through_one_owner_of_a_block_reaches_the_other_owners`（`CROSS_PRELUDE` に `RH`・`outerg`・`outerl`・`outerq` を足したもの）: 直接の共有の 10 形とパイプライン・フィールド・要素の共有の 3 形ずつの、安全な順序の受理と逆の順序の拒否（両方の edition で `42:5` の `E1013`（`assignment_extends_borrow`））、書き込んだ後のムーブ（同じ）、パイプライン・要素の共有を通した自分の部分の 3 形ずつ（`PairOk` の受理、`PairBad` の `42:5` の `E1013`（`self_referential_store`））。修正を外したビルドでは、このテストが失敗することを確かめました。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r22.mjs`・`diff-r22.mjs`・`table-r22.mjs` に写し（基準と先端（`r21`、`dff92288` のビルド）の結果を `results-r21.json` から使い、`r21` に無いプログラムは先端でも検査）、`gate/tools/collect-r22.mjs`（第2回の生成したプログラム `r16-review-2-ce/g` を除くもの）でこのレビューのプログラム（`r18-review-4-ce` の 256 個、`r18-review-4-conf` の 282 個）とこの回の探査（`rr4/` の 142 個）、第3回の後に足した探査（`rr3/` の 7 個）の 687 個を足して、14939 個・29878 組を両方の edition で検査しました（`results-r22.json`、分類は `diff-r22.txt`、`rr4/gate-diff-r22.txt` に写し）。第2回の反例の探索が生成した 15552 個は、別に `rr4/g-check.mjs` で検査しました（下）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 5085 | 4025 |
| 両方が拒否 | 5185 | 7102 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4631 | 3783 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、前回と同じ 67 組です。新しい RA はありません。** 条件 (1) は成り立ちます。
- **拒否から受理に変わった組はありません。**
- 前からある 28504 組では、先端からの受理・拒否の変化はなく、最初の誤りが変わったのは `review-a31-conf-r12/p/r01_rs_bad_order_reassign_rc`（基準が受理する危険な形）の両方の edition だけです。拒否のまま、先端の 1 つ目の誤り（`99:5` の `E1013` の `self_referential_store`）と 2 つ目（`103:5` の `E1014`）に代わって、`103:5` の `self_referential_store` が最初の誤りになりました（`let inner = Rc.share (ref rc)` を通した書き込みが元の持ち主 `rc` にも届くので、`rc = Rc.new (mkp ())` の位置で、古い値が持つ自分の部分の借用を部分の順序で検査します）。
- 足した 1374 組での先端からの変化は、受理から拒否の 155 組だけで、どれも名前が `u_` の危険な形です: このレビューの指摘 1・2 の形（`r18-review-4-ce/p/u_rc_*`・`u_rcf_*`・`u_rcp_*_loc`・`u_rcself_bad_*`・`u_arr_lam`）、この回の双子（`rr4/t/u_*`）、共有の別の作り方（`rr4/x/u_getp`・`u_ident`・`u_if`・`u_inner`・edition 2026 の `u_loop`）、書き込んだ後のムーブ（`rr4/p/u_mvsrc_after_store`）。安全な形（`s_*`、`r18-review-4-conf/p/*/s_*`、`rr4/t/s_*`、`rr4/x/s_*`）は、どれも先端と同じ判定です。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 安全な形の実行: この回の双子の安全な形のうち 24 個（上）と、`sharetype` の 2 個（`main` のバイナリ）、文書にした制限の形（`rcgetpart`・`fvdrop`・`refsrc` の 6 個、`main` のバイナリ）は、手で求めた値を出しました（`rr4/t-runs-2027.txt`、`rr4/limits-main-runs.txt`）。この回の修正で受理に変わった形はありません。
- 生成したプログラム（`rr4/g-check.mjs`、15552 個・31104 組。結果は `rr4/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました。

条件 (3) の、`main` が受理する安全な形の拒否は、どれも文書に書いた安全側の制限です（この回の `rcgetpart`・`fvdrop`・`refsrc` と、修正による `sharetype`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3294 件すべて成功、4 件 skip（1551 秒、`rr4/nextest.log`。この回の 1 件を含みます）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 83 件（基準のバイナリ・先端でも 83 件が成功）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`rr4/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`rr4/step5-main.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・53 個のリンク・11 個の例・22 回の native の実行、`rr4/docs-2.log`）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功（`rr4/doc-checks.log`）。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（`rcgetpart`・`fvdrop`・`refsrc`・`sharetype` の行、`fvlam` の行、段 3 の一覧、この回の記録、検査の時間）、このチケット（「残る制限」と、この節）。

#### レビューの第5回（2026-11-15）

第4回の対応の後の先端（`2084a9e4`・`a5f81372`・`b6d30189`、その前のレビューの先端は `7ccca2e7`）のレビュー（反例の探索 `target/scratch/a31/r18-review-5-ce/`、適合の確認 `target/scratch/a31/r18-review-5-conf/`）の 5 件の指摘を、修正後・先端（`b6d30189`。`src/` は `2084a9e4` と同じ）・前の先端（`dff92288`）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どれも指摘のとおりに再現しました。指摘 1・4 は第4回の締めつけによる前の先端からの退行（安全な形の拒否）、指摘 2・3 は `main`・基準が受理する安全な形の文書に無い拒否で、前の先端も同じく拒否します。指摘 5 は文書の誤りです。この回からの規則（前の先端からの退行は、締めつけを狭めるのが明らかに健全なら狭め、そうでなければ文書に書く。前の先端からの退行でない安全な形の拒否は `src/` を変えず、保守的な制限として文書に書く）に従いました。結果は `target/scratch/a31/rr5/` にあります（`rr5/v.sh` が修正後・先端・前の先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`rr5/n/` が狭めの探査（`rr5/n/gen.py`）、`rr5/limits-main-runs.txt` が文書にした制限の形の `main` での実行です）。

##### 原因と変えたこと（`src/ownership.rs`）

- **指摘 1・4（使い終えた共有の持ち主を通した格納の型の判定が、文書より広い）**: 第4回の修正は、`keep_for_drop` が局所値 `r` の共有を読み替えるとき、`export_block(r, ExportTarget::Local(r2))` で、`r` の型が届く、セルを持つすべてのブロックの型を元の所有者 `r2` への届け先として記録し、`store_exported_into` は、格納先の局所値の型がそのどれかに届けば `r2` へも格納していました。そのため、文書の書いた「同じ型の別のブロック」より広く、(a) ブロックでない値（`record W { h: Holder, keep: Maybe<Rc<PairOk>> }` の `keep` が `Maybe.None` の `w`、`record HM { h: Holder, rs: [Rc<PairOk>] }` の `rs` が空の値、`record HR { h: Holder, rc: Rc<PairOk> }` の `Holder` への格納）、(b) 共有したブロックが中に持ちうる型の無関係なブロック（`Tree { ..., child: Maybe<Rc<Leaf>> }` の後の別の `Rc<Leaf>`）、(c) 共有を持つ値の型だけが届くブロック（`RH2 { rc: Rc.share (ref r2), other: Rc<Other> }` の後の無関係な `Rc<Other>`）、(d) それを中に持つ別の型のブロック（`record Big { h: Holder, inner: Rc<PairOk> }` の `Rc<Big>`）への格納も `r2` へ届き、`p` の後に宣言した書き込み先でも `E1013` でした。
  このうち (c) は、届け先 `r2` が解放しえないブロックです。`r2` の破棄が解放するのは `r2` の値から届くブロックで、その型は `r2` の型から型で届くもの（共有したブロックと、その中のブロック）だけです（関数値の環境の中のブロックは型で届きません。`Rc` は関数値に捕捉できない（`E1005`）ので、`Rc` のブロックはそこに入りません。`Arc` を捕捉した関数値は捕捉した共有の loan を持つので、それを書き込んだブロックの持ち主はその loan を通して共有の元の持ち主が後で持つ借用を読みます（`expand_shares_read`）。元の持ち主そのものを捕捉へムーブする形は、ほかの共有が生きている間は `E1014` です（`rr5/arc/`）。第4回の型の判定がそうしたブロックへ届けたのは、共有を持つ値の型に同じ `Arc` の型があるときだけでした）。そこで、`keep_for_drop` の元の所有者への届け先を、`r` の型と `r2` の型の両方が届くブロックの型に狭めました（`export_shared_block`。`export_block` の型の走査を `export_walk` に分け、型ごとに 1 度だけ走査します）。共有したブロックは両方の型が届くので必ず残り、その中のブロックも `r2` の型から届くので残ります（第4回の対応の前の先端が受理し、第4回の修正が塞いだ、中のブロックの共有を通した危険な形 `rr5/n/u_leaf_nested`・`u_leaf_nested_late`・`u_leaf_in_box`・`u_box_inner`・`u_box_inner_direct` は、修正後も両方の edition で拒否します。`main` は `u_leaf_nested` で `drop -1`）。共有を持つ値が 2 つの元の所有者の共有を持つ形（`RH2 { rc: Rc.share (ref r2), other: Rc.share (ref o2) }`、`rr5/n/u_two_sources*`）も、格納は `r2` と `o2` のそれぞれへ届き、拒否のままです。
  (a)・(b)・(d) は狭めません。格納先の型が共有したブロックの型かその中のブロックの型に届く値で、その値がブロックを共有しているかどうかは型からは分かりません。呼び出し元には、格納が局所値の中のどの部分へ入ったかも分からない（事実は書き込み先の仮引数を記録し、その中の場所は記録しません）ので、ブロックでない値の部分を型で除くこともできません。局所値ごとの正確な道は第4回で 3 乗になったので採っていません。`drop.md`・`lifetimes.md`・`docs/language.md`・`docs/architecture.md`・A33 の表（`sharetype`）・「残る制限」に、実際の規則（共有したブロックの型か、それが中に持ちうるブロックの型を持ちうる値への格納）と (a)・(b)・(d) の形を書きました。`p` を `r` を使い終えた後で宣言した形（`r18-review-5-ce/g5/s_plainrec`・`s_bigblock`・`s_sametype`）は、読み替えの逆の向き（`export_block(r2, ExportTarget::Local(r))`）が前から型で判定するので、前の先端も拒否します（`r` が借用を持つ）。
- **指摘 2・3・5**: 文書に書きました（下の表と「残る制限」）。指摘 2 は、ラムダが捕捉した局所値の loan を `captured_loans` で狭めても、再借用を束縛した局所値（`let q1 = ref (deref q0)`、`let q1 = ref (deref (ref p1))`）の loan は `Checker::loan` が元の借用の loan（`q0` が持つ `p1` 全体の借用、`ref p1` の一時の借用）を親に持つので、書き込み先が `p1` 全体を生かすことです（`refsrc` と同じ仕組み）。指摘 3 は、`Checker::drop_stores` が破棄を `drop_only` にするのが `parameter_part`（不変で参照でない仮引数、そのフィールド・要素）の破棄だけで、仮引数を入れたリテラルは仮引数の根でなく、edition 2027 の読み取りの仮引数は参照の型なので、それを束縛した局所値（`let h = f`、`let h = r.f`）の破棄も当たらないことです。`own` の仮引数と edition 2026 の値の仮引数のフィールドを束縛した局所値（`let h = r.f`）は、不変な局所値が束縛した値と同じものを破棄するので `parameter_part` に当たり、受理します（`r18-review-5-conf/p/fr_m7`、edition 2026 の `m11`）。`lifetimes.md` の「本体がそうした値を破棄するときの書き込みは、仮引数の一部の破棄として記録しません」は仮引数のフィールドを含めていたので、`own` の仮引数と edition 2026 の値の仮引数のフィールドは記録すると直しました。指摘 5 は、`drop.md` の「参照の仮引数として受け取る関数を通す」が `q1` を渡す形を含むように読めたことで、受理されるのは呼び出しの位置で `ref p1` を書く形だけだと書き直しました。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（使い終えた共有の持ち主を通した格納の型の判定が、ブロックでない値・別の型のブロック・中のブロックの型・共有を持つ値の型だけが届くブロックにも及ぶ。前の先端からの退行） | 再現（`g5/t_plainrec`・`t_rh2_other`・`t_bigblock` を先端が両方の edition で `36:5`、`t_tree_leaf` を `42:5` の `E1013`（`assignment_extends_borrow`）。前の先端・`main`・基準は受理し、`main` は `drop 8`・`drop 0`・`drop 0`・`1`、`drop 8`・`drop 0`・`drop 0`・`drop 0`・`1`（native と ASan の `-O0`・`-O3`、ASan は報告なし）） | `t_rh2_other`（と `s_rh2_other`）は `export_shared_block` で受理（修正後の native と ASan の `-O0`・`-O3` で `drop 8`・`drop 0`・`drop 0`・`drop 0`・`1`）。ほかは文書（`sharetype`） | `a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block` |
| 2（再借用を束縛した参照の変数のラムダの捕捉が、借用元の値全体を生かす。`main` が受理する安全な形の拒否） | 再現（`g7/s_lamderef_mvh` を修正後・先端・前の先端が両方の edition で `32:17`、`g7/s2_lamderefref_mvh` を `35:18` の `E1014`（`borrow_conflict`）。`main` は `drop 0`・`drop 8`・`1`。場所の借用・別名・`if`・`pick` を束縛した `q1` の捕捉（`s2_lamalias_mvh`・`s_lamif_mvh`・`s2_lampick_mvh`）は受理） | 文書（`refsrc`） | — |
| 3（仮引数を入れたリテラル、edition 2027 の読み取りの仮引数やそのフィールドを束縛した局所値の破棄。同上） | 再現（`p/d_m13`・`d_m14`・`d_m15`・`d_m17` を両方の edition で `37:5`、`p/m8`・`m11` を edition 2027 で `35:5`、`d_m16` を `37:5`、`fr_m2` を `39:5` の `E1013`（`self_referential_store`）。`main` は `drop 0`・`drop 0`・`1`。`fr_m7`（`own`）・`fr_m3` は受理） | 文書（`fvdrop` を `parameter_part` の一般の規則で書き直し、`lifetimes.md` の仮引数のフィールドの文を直した） | — |
| 4（指摘 1 と同じ仕組みで、局所値自身の `Holder` への格納も型で届く。前の先端からの退行） | 再現（`p/st_hm`・`st_hr` を先端が両方の edition で `36:5` の `E1013`（`assignment_extends_borrow`）。前の先端・`main`・基準は受理し、`main` は `drop 8`・`drop 0`・`drop 0`・`1`、`drop 8`・`drop 0`・`drop 0`・`drop 0`・`1`。`st_hm_noshare` は受理） | 文書（`sharetype`。格納先の値の型が共有したブロックの型に届くので、指摘 1 の (a) と同じく狭めない） | 同上（`W` の形の拒否を制限として固定） |
| 5（`refsrc` の「参照の仮引数として受け取る関数を通す」が `q1` には当たらない。文書） | 再現（`p/rs_q1_outer`・`rs_q1_outerq` を修正後・先端・前の先端が edition 2027 で `37:5` の `E1014`。`main` は `drop 0`・`drop 8`・`drop 5`・`1`） | 文書（呼び出しの位置で `ref p1` を書く形だけを受理と書き、`outer (ref p2) q1` は拒否と書いた。`docs/language.md` も） | — |

##### この回の確かめで見つけたもの

マージの条件の確かめ（下）で、このレビューのプログラムのうち `main` が受理し、前の先端から拒否していて、指摘にも文書にも無かった安全な形を 4 個見つけました。どれも `main` のバイナリの native と ASan の `-O0`・`-O3` で手で求めた値を出し、ASan は何も報告しませんでした（`rr5/extra-main-runs.txt`）。この回の規則のとおり `src/` は変えず、保守的な制限として `drop.md`（「`Rc`・`Arc` の別の持ち主を通した書き込み」）、`lifetimes.md` の「現在の制限」、`docs/language.md`、A33 の表、「残る制限」に書きました。

- **`rcgetowner`**（`r18-review-5-ce/g2/s_innerblock`・`g11/s_hold_self`）: `Rc.get (ref r)` の loan は `r` の loan なので、ブロックをほかの持ち主が生かしていても、それを持つ値が `r` より後に破棄されると `E1013`（`assignment_extends_borrow`）です。内側のブロックの `{ let p = mkpo (); let r = Rc.share (ref r2); let a = Rc.get (ref r); cross (ref p) a; 0 }`（`main` は `drop 8`・`drop 0`・`1`）と、ブロックの自分の部分を別の持ち主 `r3` を通して書き込んだ後の `hold = r`（`drop 0`・`drop 8`・`1`。続く `E1014` は A33 の `r07`・`r10` と同じ）。前の形は `Rc.get (ref r2)` から得れば受理します（`rr5/sp/w_inner`）。
- **`shareparam`**（`r18-review-5-ce/g1/s_dup`・`g8/s_stashs2`）: 参照の仮引数の `Rc` から関数の中で作った共有（`fn dup r = Rc.share r` の結果、本体の `let s = Rc.share r`）は、`share_loans` が外部の loan に共有の印を付けないので仮引数の先の借用のままで、その破棄（`drop_stores`）がその借用をブロックのセルへ格納するものとして記録され（`facts.drops_write` のプログラムだけ）、`let r = dup (ref r2)` や、共有を作って捨てるだけの `sh2 (ref r2)` の呼び出しで、`r2` が自分自身の借用を持つものとして `E1013`（`self_referential_store`）です（`main` は `drop 8`・`drop 0`・`1`。`rr5/sp/v_*`。`298408bd` のビルドも拒否します）。呼び出し元で `Rc.share (ref r2)` と書けば受理します（`rr5/sp/w_dup`）。

ほかに拒否される安全な形は、どれも文書にある制限でした: 関数値の仮引数やそのフィールドを束縛した局所値を呼ぶ関数（`fn m9 f b = { let h = f; h b }` の edition 2027、`fn m10 r b = { let h = r.f; h b }` の両方の edition。`r18-review-5-conf/p/m9`・`m10`・`fr_m6`）は、`lifetimes.md` の「現在の制限」が本体の見えない関数値として挙げる「引数で受け取った関数値を別名にしたもの」の呼び出しで、`r18-review-5-conf/p/rg_*` は `rcgetpart`、`r18-review-5-ce/g7/s_*` と `r18-review-5-conf/p/rs_q1_*` は `refsrc`、`r18-review-5-conf/p/fr_m4` は `fvdrop`（パターンの変数）です。`r18-review-5-conf/p/nd_inner*` は危険な形（`main` は解放した文字列を読んで `-1`）です。

##### テスト

- `tests/rc.rs` の `a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block`: 共有を持つ値の型だけが届く無関係な `Rc<Other>` への格納の受理（`p` を共有の前と後に宣言する 2 形、両方の edition）、その値が共有する 2 つ目の元の所有者 `o2` のブロックへの格納の受理（`p` が先）と拒否（`o2` が先、`50:5` の `E1013`（`assignment_extends_borrow`））、共有したブロックの中のブロックの共有を通した格納の受理と拒否（同じ）、制限 `sharetype` の `W` の形の拒否（`50:5`）。修正を外したビルドでは、最初の受理が `'r2' keeps a borrow ...` で失敗することを確かめました。
- `tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs` の `unrelated_blocks_keep_no_borrow`: 共有を持つ値の型だけが届く無関係なブロックへの格納（`t_rh2_other` の形を `Recorder` で書いたもの）。`o` の記録が `p1` の "12345" を読んで 6、ほかの記録は 0、関数は `r2` の "1234567" の長さ 7 を返すので、`70006`（記録の初期値を 9 にして、どの記録も書かれたことを確かめます）。基準のバイナリも受理し、先端は `1551:5` の `E1013` で拒否します。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r23.mjs`・`diff-r23.mjs`・`table-r23.mjs` に写し（基準と先端（`r22`、`2084a9e4` のビルド）の結果を `results-r22.json` から使い、`r22` に無いプログラムは先端でも検査）、`gate/tools/collect-r23.mjs`（第2回の生成したプログラム `r16-review-2-ce/g` を除くもの）でこのレビューのプログラム（`r18-review-5-ce` の 139 個、`r18-review-5-conf` の 66 個）とこの回の探査（`rr5/` の 18 個）ほか 2 個の 225 個を足して、15164 個・30328 組を両方の edition で検査しました（`results-r23.json`、分類は `diff-r23.txt`、`rr5/gate-diff-r23.txt` に写し）。第2回の反例の探索が生成した 15552 個は、別に `rr5/g-check.mjs` で検査しました（下）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 5156 | 4099 |
| 両方が拒否 | 5213 | 7136 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4757 | 3900 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、前回と同じ 67 組です。新しい RA はありません。** 条件 (1) は成り立ちます。
- 前からある 29878 組では、先端からの受理・拒否の変化も、最初の誤りの変化もありません。
- 足した 450 組での先端からの変化は、拒否から受理の 4 組（`r18-review-5-ce/g5/t_rh2_other`・`s_rh2_other` の両方の edition。上の狭めによるもので、`main`・基準も受理し、修正後の native と ASan の `-O0`・`-O3` で `drop 8`・`drop 0`・`drop 0`・`drop 0`・`1`）だけで、受理から拒否はありません。
- 足した組のうち基準が受理し対応後が拒否する 243 組は、名前が危険を示す形（`u_*`・`*_bad`・`arc_u*`。`r18-review-5-conf/p/nd_inner*` も危険な形）と、文書にした制限（`sharetype`・`refsrc`・`fvdrop`・`rcgetpart`、本体の見えない関数値の別名の呼び出し、この回に足した `rcgetowner`・`shareparam`）のどれかでした（`gate/ar-fresh-r23.txt`）。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 文書にした制限の形（`sharetype` の 6 個、`refsrc` の 4 個、`fvdrop` の 8 個、この回に足した 4 個）は、`main` のバイナリの native と ASan の `-O0`・`-O3` で手で求めた値を出し、ASan は何も報告しませんでした（`rr5/limits-main-runs.txt`、`rr5/extra-main-runs.txt`）。
- 生成したプログラム（`rr5/g-check.mjs`、15552 個・31104 組。結果は `rr5/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました。

条件 (3) の、`main` が受理する安全な形の拒否は、どれも文書に書いた安全側の制限です（指摘 1・4 の残りの `sharetype`、指摘 2・5 の `refsrc`、指摘 3 の `fvdrop`、この回の確かめで見つけた `rcgetowner`・`shareparam`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3295 件すべて成功、4 件 skip（1376 秒、`rr5/nextest.log`。この回の 1 件を含みます）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 84 件（基準のバイナリでも 84 件が成功。先端はこの回の件のモジュールを拒否します）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`rr5/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`rr5/step5-main.txt`）。
- 検査の時間: `gen_chain.py cellshares` の所有権の検査は、n = 400 で 226 ms（先端 214 ms）、800 で 1007 ms（908 ms）で、同じ程度です（同じ負荷の機械で続けて測りました）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・55 個のリンク・11 個の例・22 回の native の実行、`rr5/docs.log`）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功（`rr5/doc-checks.log`）。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（1 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（`sharetype`・`refsrc`・`fvdrop` の行、新しい `rcgetowner`・`shareparam` の行、段 3 の一覧、この回の記録）、このチケット（「残る制限」と、この節）。

#### レビューの第6回（2026-11-15）

第5回の対応の後の先端（`6e4f0537`・`e5614a3e`・`1191ba64`・`253a78e7`・`c18c0433`、A34 の起票 `953747a2`）の最後のレビュー（反例の探索 `target/scratch/a31/r19-review-6-ce/`、適合の確認 `target/scratch/a31/r19-review-6-conf/`）の 2 件の指摘を、修正後・先端（`953747a2`。`src/` は `6e4f0537` と同じ）・前の先端（`b6d30189`。`src/` は `2084a9e4` と同じ）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どちらも指摘のとおりに再現しました。指摘 1 は第5回の狭めによる前の先端からの退行で、`main` も受理する危険なプログラムの受理です。指摘 2 は文書の誤りです。最後の回の規則（検査を緩める変更はしない。前の先端や `main` からの危険な退行は、締めつけ（第5回の狭めを戻す・狭める）と回帰のテストで直し、それで拒否に変わる安全な形は制限として文書に書く。安全な形の拒否は `src/` を変えずに文書と A34 に書く）に従いました。結果は `target/scratch/a31/rr6/` にあります（`rr6/v.sh` が修正後・先端・前の先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`rr6/n/` が修正の周りの探査、`rr6/lim/` と `rr6/a34/` が修正による安全側の拒否の形です）。

##### 原因と変えたこと（`src/ownership.rs`）

- **指摘 1（dyn の値の中の共有を通した格納が、ブロックを解放する元の所有者へ届かない。前の先端からの退行）**: 第5回の `export_shared_block(h, r2)` は、`keep_for_drop` が読み替えた局所値 `h` の型が届くブロックの型のうち、元の所有者 `r2` の型も届くものだけを `r2` への届け先にしていました。型の走査（`TypeContext::reaches`。`type_arena::children` は `Type::Dyn`・型引数などに子を持たせません）は dyn の値の中を見ないので、`record B {r} { h: Holder, d: dyn Cls.LeafOf {r} }` の `Rc<B>` から届くのは `Rc<B>` だけで、dyn の値の中の `Box1` の `Rc<Leaf>` は見えません。一方、dyn の値のメソッドは中身の場所の参照を返せる（`def leaf :: ref 'value -> ref Rc<Defs.Leaf>`、`fn leaf b = ref b.l`）ので、`let h = Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))` は、`r2` の破棄が（`B` → dyn → `Box1` → `Rc<Leaf>` と）解放しうる `Leaf` のブロックの共有です。`let a = Rc.get (ref h)` の後で `h` は使われないので読み替えられ、届け先は `{Rc<Leaf>} ∩ {Rc<B>} = ∅` になり、`Defs.crossleaf a (ref p)` の `p` の借用は `h` にだけ入りました。実行では `h`（`Leaf` の持ち主を 2 から 1 に）、`p`、`r2`（`B`・dyn・`Leaf` を解放）の順に解放され、`Leaf` の `Holder` の `drop` が解放した `p.s` を読みます（`d1` は `drop -1`、`d1_heap`・`d2_*`・`d6_plain`・`d7_arc` は ASan の `-O0`・`-O3` で heap-use-after-free）。第4回の先端は `h` の型が届くすべての型（`{Rc<Leaf>}`）を届けたので拒否していました。第5回のチケットの「`r2` の破棄が解放するのは `r2` の型から型で届くブロックだけ」は、dyn の値（と元の所有者のほかの部分。指摘 2）について誤りでした。関数値は環境の中の場所を渡せない（捕捉した値の参照を返すラムダは `E1013`。`r19-review-6-ce/p/f1`）ので、関数値の環境は同じ問題を持ちません。
  修正（`5e99c1c0`）: 値の型が dyn の値か型引数（`Type::Dyn`・`Variable`・`Infer`・`Partial`・`Application`・`Error`）に届くかを答える `walk_hides_blocks` を足し、
  (a) `export_shared_block` は、`r2` の型がそうした値に届くなら狭めず、`h` の型が届くセルを持つブロックの型をすべて届けます（第4回の先端の扱い。指摘の勧めた直し方です）。
  (b) `h` の型がそうした値に届くなら（`H { d: Dyn.of (Box1 { l: Rc.share (ref r2) }) }`。共有そのものが dyn の値の中にあり、`h` の型からはブロックが見えません）、印 `HIDDEN_BLOCK`（`Type::Error`。ブロックの型にはならない）を `r2` への届け先に足し、`store_exported_into` は、格納先の型が dyn の値か型引数かセルを持つブロックに届けば、この印の届け先へ格納します。
  (c) `store_exported_into` は、格納先の型が dyn の値か型引数に届けば、記録したすべての届け先へ格納します（そうした値はどの型のブロックも持ちうるため。型で届くブロックの型が同じことで連なりを伝えていた規則の、dyn の値の場合です）。
  (d) 逆の向き（`expand_shares` から、元の所有者を通した後の格納を共有の持ち主・呼び出し元のデータ・隠れた環境へ届ける `export_block`）も、所有者の型が dyn の値か型引数に届くなら `HIDDEN_BLOCK` を記録します（`e45887c8`。セルを持たない `B` の `Rc<B>` の dyn の値の中のブロックはセルを持ちうるのに、`export_walk(Rc<B>)` は空でした）。
  確かめの中で、`main`・基準・前の先端・先端がどれも受理する、同じ仕組みの危険な形を見つけ、(b)・(c) で塞ぎました（`main` の ASan の `-O0`・`-O3` は heap-use-after-free）: 共有を持つ値の型が dyn の値に届く形（`rr6/n/u_hrecdyn`）、セルを持たない `B {r} { n: i64, d: dyn ... }` の `Rc<B>` 全体を共有して dyn のフィールドの中の `Leaf` へ書き込む形（`u_bdyn_whole`。`export_walk(Rc<B>)` は空なので、第4回の先端も何も届けませんでした）、共有の共有を dyn の値に入れた形（`u_chain_dyn`。`h` の読み替えで `h` から `r2` への道が消え、`store_exported` は届け先の局所値の届け先をたどらない（`exporting`）ので、dyn の値への格納はすべての届け先へ届ける必要があります）、呼び出し元の配列へも共有を入れた形（`u_extchain`・`u_extchain_rpfirst`。届け先は呼び出し元のデータ）。(d) で、逆の向きの同じ穴（セルを持たない `B` の `r2` の共有 `hd` を持ち続け、`r2` の dyn のフィールドの中の `Leaf` の共有を通して書き込む `u_rev_nocell`・`u_rev_nocell2`・`u_rev_lchain`、`r2` の共有を呼び出し元の配列へ入れた `u_ext_nocell`・`u_ext_lchain`）も塞ぎました。
  修正による安全側の拒否（`main` と先端は受理）: `h` の型が dyn の値に届くときの無関係な `Rc<Other>` への格納（`rr6/lim/s_dynholder_other`、`rr6/a34/s_sharetype_dynholder`）、`r2` の型が dyn の値に届き `h` の型が `Rc<Other>` を持ちうるときの同じ格納（`s_dynsrc_other`・`s_sharetype_dynsrc`。前の先端も拒否）、使い終えた共有の後の、dyn の値を持つ無関係な局所値への格納（`s_dynstore`・`s_sharetype_dynstore`）、呼び出し元の配列へ共有を入れた関数の中の同じ格納（`s_dynstore_ext`、`store_local_borrow_through_reference`）、(d) による、`r2` の型が dyn の値に届くときに `r2` の共有を持ち続ける値やその共有を入れた呼び出し元のデータへの、無関係な `Rc<Other>` への格納（`s_rev_dynowner_other`・`s_sharetype_dynowner`、`s_ext_dynowner_other`）。どれも `main` の native と ASan の `-O0`・`-O3`（edition 2027。A34 の形は edition 2026 の native の `-O0`・`-O3` も）で手で求めた値を出し、ASan は何も報告しませんでした（`rr6/lim-main-runs.txt`・`lim-main-runs2.txt`、`rr6/a34-runs.txt`）。`drop.md`・`lifetimes.md`・`docs/language.md`・`docs/architecture.md`・A34 の `sharetype`（「`sharetype` に A31 のレビューの第6回で足した形」。最小のプログラムと危険な双子）・A33 の表の行・「残る制限」に書きました。
- **指摘 2（`sharetype` の文書が、届け先を共有したブロックとその中のブロックの型と書いていた。文書）**: `export_shared_block` の判定は、`h` の型と `r2` の型全体の両方が届くブロックの型で、`r2` がレコードなどでほかのフィールドにもブロックを持てば、そのブロックの型も入ります（`r2 = RS { rc: Rc<PairOk>, other: Rc<Other> }` と `h = RH2 { rc: Rc.share (ref r2.rc), other: Rc.new (mkother 2) }` の後の無関係な `Rc<Other>` への格納が `r2` へ届く。`r19-review-6-conf/st/s_srcrec_other`）。前の先端も同じく拒否する安全な形で、`src/` は変えず、実際の規則とこの形を `drop.md`・`lifetimes.md`・`docs/language.md`・`docs/architecture.md`・A34 の `sharetype`（`s_sharetype_srcrec` とその危険な双子）・「残る制限」に書きました。指摘の言うとおり、共有したブロック自身の型の走査に狭めても健全です（`r2` のほかのフィールドのブロックは、そのブロック自身の共有の loan で `r2` へ届くため）が、最後の回なので検査を緩める変更はしていません。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 | テスト |
| --- | --- | --- | --- |
| 1（dyn の値の中の共有を通した格納が元の所有者へ届かない。前の先端からの退行、危険） | 再現（`r19-review-6-ce/p/d1`・`d1_heap`・`d2_nocell`・`d2_heap`・`d6_plain`・`d7_arc` を先端が両方の edition で受理し、前の先端は `8:5` の `E1013`（`assignment_extends_borrow`）。`main`・基準は受理し、`d1` は `drop 0`・`drop 0`・`drop -1`・`1`、ほかは ASan の `-O0`・`-O3` で heap-use-after-free。安全な双子 `d3_safe`・`d3_heap`・`d6_plain_safe` はどのバイナリも受理。`d4_lam`・`d5_pipe`・`e1_rev` は先端も拒否） | 修正後はどれも両方の edition で `8:5` の `E1013`（`assignment_extends_borrow`）。安全な双子は受理（`drop 0`・`drop 8`・`drop 0`・`1`、`drop 0`・`drop 36`・`drop 0`・`1`）。同じ仕組みの `main` からの穴 10 個も拒否 | `a_store_through_a_share_behind_a_dyn_value_reaches_the_owner_that_frees_the_block` |
| 2（`sharetype` の文書の規則が実際より狭い。文書） | 再現（`r19-review-6-conf/st/s_srcrec_other` を先端・前の先端が両方の edition で `63:5` の `E1013`。`main`・基準は受理し `drop 8`・`drop 0`・`drop 0`・`drop 0`・`drop 0`・`1`。`s_srcrec_other_nob`・`s_rh2_other` は受理） | 文書（実際の規則と、元の所有者のほかの部分のブロックの形） | — |

##### テスト

- `tests/rc.rs` の `a_store_through_a_share_behind_a_dyn_value_reaches_the_owner_that_frees_the_block`（`Defs.tz`・`Cls.tt`・`Main.tz` の 3 つのモジュール）: dyn の値を通した 7 つの危険な形（指摘 1 の `d1`・`d2_nocell`・`d6_plain`・`d7_arc`、`Rc<B>` 全体の共有の後の dyn の中の `Leaf` への書き込み、共有を dyn の値に持つ値、共有の共有を dyn の値に持つ値）を両方の edition で `8:5` の `E1013`（`assignment_extends_borrow`）で拒否し、それぞれの安全な双子（借用される値を元の所有者より先に宣言する）を受理します。共有を呼び出し元の配列へも入れた形は `8:5` の `E1013`（`assignment_extends_borrow`）と、借用される値を先に宣言すると `13:5` の `E1013`（`store_local_borrow_through_reference`）です。dyn の値に共有を持つ値を通して書き込んだ後で元の所有者をムーブする形は受理します（読み替えは保つ）。逆の向き（`u_rev_nocell`）は `8:5` の `E1013`（`assignment_extends_borrow`）で、その安全な双子は受理し、呼び出し元の配列へ共有を入れた形（`u_ext_nocell`）は `12:5` の `E1013`（`store_local_borrow_through_reference`）です。`5e99c1c0` の前のビルドでは最初の拒否（`d1`）が、`e45887c8` の前のビルドでは逆の向きの拒否が受理されて失敗することを確かめました。
- E2E は足していません（dyn の値の形はクラスのファイルが要り、`in_place_writes` の fixture は 1 つのモジュールです）。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r24.mjs`・`check-r25.mjs` と `diff-r24.mjs`・`diff-r25.mjs`・`table-r24.mjs`・`table-r25.mjs` に写し、`gate/tools/collect-r24.mjs`・`collect-r25.mjs` でこのレビューのプログラム（`r19-review-6-ce` と `r19-review-6-conf`）とこの回の探査（`rr6/n/`・`rr6/lim/`・`rr6/a34/`）ほかの 187 個を足して、15351 個・30702 組を両方の edition で検査しました。`r24` は最初の修正（`5e99c1c0`）のビルド、`r25` は今の先端（`e45887c8`）のビルドで、基準・先端（`r23`、`6e4f0537` のビルド）・`r24` の結果は前の結果から使い、無いプログラムはそれぞれで検査しました（`results-r25.json`、分類は `diff-r25.txt`、`rr6/gate-diff-r25.txt` に写し）。第2回の反例の探索が生成した 15552 個は、別に `rr6/g-check.mjs` で検査しました（下）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 5208 | 4155 |
| 両方が拒否 | 5230 | 7156 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4875 | 4011 |
| 検査の失敗・時間切れ | 0 | 0 |

- **RA は 67 組で、前回と同じ 67 組です。新しい RA はありません。** 条件 (1) は成り立ちます。
- 前回からある 30328 組では、先端からの受理・拒否の変化も、最初の誤りの変化もありません。
- 足した 374 組での先端からの変化は、受理から拒否の 62 組（31 個）だけで、拒否から受理はありません。31 個は、指摘 1 の危険な形（`d1`・`d1_heap`・`d2_nocell`・`d2_heap`・`d6_plain`・`d7_arc`）、この回に塞いだ `main` からの穴（`rr6/n/u_*` と、名前に反して危険な `s_extchain` = `u_extchain_rpfirst`、`rr6/lim2/t` = `d1` の `Defs` を `Main.tz` に入れた形）、A34 の危険な双子（`rr6/a34/u_sharetype_dyn*`）、修正による安全側の制限として文書に書いた形（`rr6/lim/s_*`、`rr6/a34/s_sharetype_dyn*`）のどれかです。`r24` から `r25` への変化は、`e45887c8` が塞いだ逆の向きと呼び出し元のデータの 5 個と A34 の危険な双子 `u_sharetype_dynowner`、文書に書いた 3 個（`s_rev_dynowner_other`・`s_ext_dynowner_other`・`s_sharetype_dynowner`）だけです。
- 足した組のうち基準が受理し対応後が拒否する安全そうな名前の形（`rr6/ar-fresh-r25.txt`）は、どれも先端も拒否する文書にある制限（A34 の 6 つの項目の形と、`r19-review-6-conf` の `fv`（`fvdrop`）・`rg`（`rcgetowner`）・`rs`（`refsrc`）・`sp`・`sp2`（`shareparam`）・`st`・`t5`（`sharetype`、指摘 2 の `s_srcrec_other` を含む））、危険な形（`e1_rev`）、この回の制限、または `rr6/n/s_gen`・`s_gen2` です。後の 2 つは型引数の値（`own 'a`）を局所値へムーブして、そのクラスのメソッドが返した参照の先の共有を通して書き込む汎用の関数で、A31 のどの先端も `E1013`（`store_local_borrow_through_reference`）で拒否します。型引数の値は呼び出し元のデータの参照を持つ値でもありうる（インスタンスのメソッドがその参照を返せる）ので、本体はその場合にも健全でなければならず、安全な形の拒否ではありません。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- キャッシュを足した `a2726750` のビルド（`r26`、`gate/tools/check-r26.mjs`、`results-r26.json`）は、30702 組のすべてで `r25` と同じ結果でした（検査の失敗も無し）。
- 前の回の先端（第4回）の回帰の確かめに使われた 40 個（`r19-review-6-ce/r4/`、`r18-review-4-ce/p/u_rc_*`・`u_rcf_*`・`u_arr_*`・`u_rcself_*`）は、修正後も両方の edition で 80 組すべて拒否します。1232 個の探査のディレクトリ（`a34/`、`r18-review-4-*`・`r18-review-5-*`・`r19-review-6-*`、`rr4/`・`rr5/`・`rr6/n/`）で `5e99c1c0` のビルドと先端を比べた結果は、指摘 1 の 6 個とこの回の `rr6/n/u_*` だけが拒否に変わりました（`rr6/diffnh-1.txt`）。
- 生成したプログラム（`rr6/g-check.mjs`、15552 個・31104 組を `e45887c8` のビルドで。結果は `rr6/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました。

条件 (3) の、`main` が受理する安全な形の拒否は、どれも文書に書いた安全側の制限です（この回の修正による `sharetype` の dyn の値と型引数の形、指摘 2 で文書にした `sharetype` の元の所有者のほかの部分の形、前からの A34 の 6 つの項目）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: `a2726750` で 3296 件すべて成功、4 件 skip（1393 秒、`rr6/nextest2.log`。この回の 1 件を含みます）。その前の `e45887c8` の実行では、`tests/formatter.rs` の `corpus_is_idempotent_and_semantically_unchanged` が 1 件、走査中のディレクトリが同時に走るほかのテストで消えた（`read_dir` の `NotFound`）ために失敗し、単独で回すと成功しました（`rr6/nextest.log`・`nextest-rerun.log`）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 84 件（基準のバイナリと先端でも 84 件）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件が、`e45887c8` と `a2726750` のビルドの両方ですべて成功（`rr6/suites.log`・`suites2.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`rr6/step5-main.txt`）。
- 検査の時間: `5e99c1c0`・`e45887c8` の `walk_hides_blocks` は呼び出しのたびに型を走査したので、`gen_chain.py cellshares` の所有権の検査が先端のおよそ 3.5 倍になりました（n = 400 で 1389 ms（先端 380 ms）、800 で 5911 ms（1707 ms）、1600 で 24540 ms（7632 ms））。型ごとのキャッシュ（`hidden_walks`、`a2726750`）で、n = 400 で 401 ms（先端 380 ms）、800 で 1693 ms（1707 ms）、1600 で 10022 ms（9732 ms）に戻りました（同じ負荷の機械で続けて測りました。`rr6/chain-times.txt`・`rr6/chain2.sh`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・62 個のリンク・11 個の例・22 回の native の実行、`rr6/docs.log`）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功（`rr6/doc-checks.log`）。A34 の第6回の 8 個のプログラムのブロックは `rr6/a34/` のファイルと同じ内容です（取り出して比べました）。
- コミット: `5e99c1c0`（修正とテスト）、`f1e8636f`（文書）、`e45887c8`（`export_block` の締めつけとテスト）、`d381de9b`（文書）、`a2726750`（キャッシュ）、この節。
- 変えたファイル: `src/ownership.rs`、`tests/rc.rs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（A34 へ移した行の `sharetype` の説明と、この回に塞いだ穴の記録）、A34（`sharetype` の第6回の形、原因・方針・落とし穴・テスト計画）、このチケット（「残る制限」と、この節）。

### Copilot のレビューへの対応（第2回）（2026-11-15）

PR #105 への Copilot の 2 度目のレビュー（`src/ownership_control.rs` への指摘 4575655989、重大度は高）に対応しました。指摘は「ループが、対象がすでに借用を持つ中身をほかへ書き出すと、要約のどの事実も変えずに `contents_elsewhere` が変わる。`check_loop` は本体の前の `exit` の状態を返すので書き出しが失われ、後の置き換えが、書き出した写しがまだ必要とする借用を終わらせる。`contents_elsewhere` と `kept_elsewhere` を `LoopSummary` とその構成に入れ、`kept_elsewhere` は `kept` と同じく `(place, mutable)` で正規化すること」というものです。修正後・先端（`fbd7d194`。`src/` は `a2726750` と同じ）・`main`（`168ae4a5`）・基準のバイナリで確かめ、指摘のとおりに再現しました。結果は `target/scratch/a31/copilot2/` にあります（`copilot2/v.sh` が修正後・先端・`main`・基準の最初の誤りを並べ、`RUN=1 ASAN=1` で native と ASan の `-O0`・`-O3` の出力を足します。`p/` が探査で、`gen/gen1.py` がループの形を生成します。`tip-runs-gen1.txt` が先端での実行、`new-safe-runs.txt` が修正後の安全な形の実行です）。

#### 再現と原因

`check_loop` は、反復の始まりの状態の要約（`loop_summary`）が、本体の後の状態（入口の状態と `continue` の状態の合流）の要約と同じになるまで反復し、同じになった反復の出口の状態（その反復の始まりの状態から条件を評価したもの）を返します。要約は、ムーブした場所、局所値ごとの loan の `(place, mutable)`、`copy_variables`、`kept` の `(place, mutable)`、`handed` の数、`block_shares` だけで、`State` の `kept_elsewhere`・`contents_elsewhere`・`moved_always`・`generic_moves` と、loan の場所と可変性のほかの性質（`Value::through` など）を持ちませんでした。本体がそれだけを変えると、最初の反復で要約が変わらず、ループの後は本体の前の状態になり、本体もその事実の下で検査し直されません。

- **`contents_elsewhere`**（`p/u_ce_*`）: `fn renew p out n` が `let t = ref p.a[0]` の借用を `p.h` のセルへ書き込み（`kept` の自分の部分の借用）、ループの中で `Mutex.with_lock (ref p.h.cell) (\b -> copy_out b (ref out.cell))` でセルの中身を `out` のセルへ写し、ループの後で `deref p = mkpa ()` で置き換えます。ループの無い形（第12回の `f09`、`p/u_ce_noloop`）は先端も `deref` の位置の `E1014` ですが、ループの形では写しが失われ、置き換えが借用を終わらせて受理していました。呼び出し元は `out` の写しを通して、置き換えで解放した文字列を読みます。
- **`kept_elsewhere`**（`p/u_ke_*`）: ループの中で、セルに書き込んだのと同じ借用 `t` を `out` のセルへも書き込む形。loan は同じ番号なので `kept` の `(place, mutable)` は変わらず、`kept_elsewhere` への追加だけが失われました。
- ループの形は、`while`、範囲の `for`、配列の `for`、本体の前の `break`、`continue`、入れ子のループ、ループの中の `try`・`match`・`if` の 9 つです。どれも先端・`main`・基準が両方の edition で受理し、先端の native の `-O0`・`-O3` で `drop 8 2`・`drop -1 2`・`drop 0 2`・`-1`（正しくは 2 行目が `drop 8 2`、結果が `8`）、ASan の `-O0`・`-O3` で heap-use-after-free でした（`copilot2/tip-runs-gen1.txt`）。条件の中で書き込む形（`u_*_condblock`）は、出口の状態が条件の後なので、先端も拒否していました。仮引数の先を部分の借用ごと渡す関数で写す形（`p/u_helper_while`、第12回の `f14` のループの形）と、写しを隠れた環境へ書き込む形（`p/u_hidden_while`、`f5u3` のループの形、edition 2027）も、先端は受理していました。
- **`Value::through`**（点検で見つけた、同じ仕組みの穴。`p/u_th_while`）: `fn stash p out n` が `let mut x = p.f`（`p` の中身の写しで、loan を `through` で持つ）の後、ループの中で `x = \() -> look (ref p.s)`（`p` そのものを捕捉し、同じ loan を `through` でなく持つ）とし、ループの後で `x` を `out` のセルへ書き込む形です。合流はその loan を `through` でなくしますが、要約は場所と可変性だけなので変わらず、ループの後の `x` は `p` の中身の写しのままで、格納は `p` の先の借用でなく中身の写し（`Part::Contents`）として記録されました。呼び出し元は `out` が `p1` の借用を持つことを知らず、`out` を `p1` より先に宣言した `run` を受理し、`out` の破棄が `p1` の破棄の後の `p1.s` を読みます（先端の native と ASan の `-O0`・`-O3` で `drop -1 2`。正しくは `drop 8 2`）。ループの無い形（`u_th_noloop`）は先端も `24:5` の `E1013`（`assignment_extends_borrow`）です。

どれも `main`・基準も受理する穴（`main` は格納を追跡しません）ですが、ループの無い形は A31 が拒否するもので、A31 の規則（`contents_elsewhere`・`kept_elsewhere`・`through`）がループで抜けていました。

#### 変えたこと（`src/ownership_control.rs`、`df307296`）

`LoopSummary` を構造体にし、`State` の事実をすべて入れました。

- loan は `LoanKey`（場所、可変性と、検査が読むほかの性質の印: ブロックの共有 `Loan::share`、パターンの view、値が `through` で持つこと、`exact_loans`・`ordered_loans`・`holding_loans` に入ること）で比べます。印はどれも loan を作る所で決まるので、反復ごとに作り直す loan も同じ組になり、不動点は前と同じく収束します。指摘の勧めた `(place, mutable)` より細かい組です（同じ場所の loan が印だけ違う形も比べるため）。
- 比べる事実: ムーブした場所、`moved_always`、推論のジェネリックなムーブの場所、局所値ごとの loan と region の slot（`Value::regions`）、`copy_variables`、`kept`・`kept_elsewhere`（`LoanKey` で）、`contents_elsewhere`、`block_shares`、`handed` の数、`exports` の数（ブロックの共有が行った先。`handed` と同じく、増えるだけで、後の格納がその場で読みます）。
- 入れないもの: `State::aliases`（match・`for`・パターンの view の束縛が自分の局所値にだけ足し、スコープの終わりで外すので、ループの前の局所値のものは反復で変わりません）と `Value::closed_result`（`State::merge` が最初の状態のものを残すので、合流した状態の要約で変わりません）、借用の親 `Loan::parents`（下の点検の表。`docs/architecture.md` と `LoopSummary` の doc comment には、この対応の後のレビューの指摘 4 で書きました）。

#### 点検（同じ種類の抜け）

ループの不動点のほかの部分と、検査のほかの不動点・状態の要約を点検しました。探査は両方の edition で、先端と修正後で確かめました。

| 部分 | 反復で変わりうるか | 危険な受理 | 対応 |
| --- | --- | --- | --- |
| `kept_elsewhere`・`contents_elsewhere`（A31） | 変わる | ある（`u_ke_*`・`u_ce_*`・`u_helper_while`・`u_hidden_while`） | 要約に入れた |
| `Value::through`（A31） | 変わる | ある（`u_th_while`） | `LoanKey` の印 |
| `moved_always`（A31） | 丸ごとムーブした局所値をループで代入し直すと変わる | 見つからない。丸ごとのムーブは局所値の loan を消す（`clear_loans`）ので、意味のある代入し直しは loan を足して要約を変え、loan の無い値は破棄の順序の検査が見ません（`u_ma_while`・`u_ma2_while` は先端も拒否、`s_ma_*` は受理） | 要約に入れた |
| `generic_moves`（基準から） | 推論のときだけ、ムーブと一緒に | 無い（失うと推論が Copy を求めず、ムーブの後の使用が誤りになる側） | 場所を要約に入れた |
| `aliases`（基準から） | ループの前の局所値のものは変わらない | — | 入れない |
| `Loan::share`（A31） | 原理的には | 見つからない（`u_sh*_while` は先端も拒否。保持する値の共有は、ループの前に `keep_for_drop` がブロックの中身に読み替えます） | 印に入れた |
| `exact_loans`・`ordered_loans`・`holding_loans`（A31） | loan を作る所で決まり、同じ場所の別の loan が足されうる | 見つからない | 印に入れた |
| `Loan::view`（基準から） | 推論のときだけ | 無い（Copy を求める側） | 印に入れた |
| `Loan::parents`（基準から） | 同じ場所の loan を別の参照を通して得るには、排他の借用と競合する | 見つからない | 入れない（`cur = ref mut (deref cur).next` のような再借用の連なりで組が伸び続けるため） |
| `Value::regions`（A12） | 可変の局所値は代入で slot を失い（`s_rg_mut` はどのバイナリも拒否）、不変の局所値はセルへの格納で `None` になる | 見つからない（`u_rg_while` はどのバイナリも拒否） | 要約に入れた |
| `Value::closed_result`（基準から） | 合流が最初の状態のものを残す | 無い（ロックのコールバックの検査は、局所値を通した関数値を証明済みとしません。`s_cr_let`・`s_cr_mut` はどのバイナリも拒否） | 入れない |
| `handed`（A31） | 増える | — | 前から要約に入っている |
| `exports`（A31、`Checker`） | 増え、後の格納がその場で読む | 見つからない（`u_ex*_while` は先端も拒否。共有を通した格納は元の所有者の loan に届きます） | 数を要約に入れた |
| `break`・`continue`（`LoopFlow`） | 状態をまるごと持ち、合流する | — | 最後の反復のものを使う（要約がそろえば、最後の反復の始まりの状態は前の反復のものを含みます） |
| `TryFlow` | 状態をまるごと持ち、合流する | — | — |
| `State::merge`・`merge_reachable` | 和（`kept`・`kept_elsewhere`・`contents_elsewhere`・`moved`・`generic_moves`）と積（`moved_always`・`block_shares`）。`through` は両方が `through` のときだけで、region は違えば `None` | — | — |
| `State::same`（A25 の重複の省略、`2ed5f692` で A31 の事実も比べる） | loan の番号まで比べる（同じ状態だけ） | — | — |
| `joint_facts`・`StoreFacts::join` | 要素をすべて比べる（`stores`・`calls`・`hidden` の数、`drop_only`・`shared_only`・`ordered`・`hidden_shared`、`shared_some` の数、場所の道） | — | — |
| `owner_facts` | `Own` 全体を比べる | — | — |
| `settle`（呼び出しごとの格納の不動点） | `store_changes` が、局所値の loan と `through` の数、`exports`、隠れた環境の記録、事実の変化を数える | — | — |
| `closed_returns` | 単調な印 | — | — |

点検で見つけた危険な受理は、すべてこの修正で拒否します。

点検の探査の中で、先端からある、edition 2026 でループの検査が収束しない形を見つけました。`ref mut` の仮引数 `p` の先のセルの中身を、値の仮引数 `sink` を捕捉したラムダ `w` を通して隠れた環境へ書き込むループ（`p/u_hidden_while`、置き換えをループの前にした安全な形 `p/s_hidden_while`）です。隠れた環境への格納（`store_hidden_except`）が仮引数 `p` の値にも写した中身の loan（`p` の先の `h.cell` の場所）を足し、`deref p` の場所は `p` の値のすべての loan の場所から作る（`Checker::place`）ので、反復ごとに `p.h.cell.h.cell` と場所が 2 段ずつ伸び、要約が変わり続けます（先端の要約も局所値の loan の場所を持つので同じです）。先端も修正後も、両方の形の edition 2026 の検査が 50 分（CPU で 45 分）で終わらず、`main` は 0.7 秒で受理します（安全な形は native と ASan の `-O0`・`-O3` で手で求めた `drop 0 2`・`drop 8 2`・`8`、ASan の報告なし）。edition 2027 ではどちらも 1 秒ほどで終わり、安全な形を受理し、危険な形を修正後は `31:5` の `E1014` で拒否します（先端は受理していました）。直すには、参照の指す場所の求め方か場所の長さを変える必要があり、小さな変更ではないので、A33 の「検査の時間」に再現のプログラムとともに記録し、`lifetimes.md` の「現在の制限」に書きました。

#### テスト

- `tests/rc.rs` の `a_loop_keeps_what_its_body_stored_elsewhere`: 2 つの事実（写しと、別の先への借用）× 9 つのループの形を、両方の edition で `deref` の位置の `E1014`（`borrow_conflict`）で拒否し、置き換えをループの前にした安全な双子を受理します。`through` の形は `27:5` の `E1013`（`assignment_extends_borrow`）で、ループで `p.f` を束縛し直す双子は受理します。先端の `src/ownership_control.rs` に戻したビルドでは、最初の形（`while`）が受理されて失敗することを確かめました。
- `tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs` の `loop_lends_borrows_of_the_new_value`: 置き換えの後に、新しい値の借用をループの中で別の仮引数の先へ書き込む安全な形です。古い `q` の記録が 0、`d` と新しい `q` の記録が新しい "123456789" を読んで 9 + 4 と 9 + 2、関数は新しい値の 7 を返すので、`((7 * 100 + 0) * 100 + 13) * 100 + 11 = 7001311`。基準と先端のバイナリでも同じ値で、85 件が成功します。
- 安全な双子（`s_ce_while`・`s_ke_while`・`s_ce_nested`・`s_ke_tryin`・`s_ce_breakfirst`・`s_ke_foreach`）は、修正後の native と ASan の `-O0`・`-O3` で手で求めた `drop 0 2`・`drop 8 2`・`drop 8 2`・`8`（`s_th_while` は `drop 5 2`・`0`）を出し、ASan は何も報告しませんでした（`copilot2/new-safe-runs.txt`）。

#### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/collect-r27.mjs`・`check-r27.mjs`・`diff-r27.mjs`・`table-r27.mjs` に写し（基準と先端（`r26`、`a2726750` のビルド。`fbd7d194` と同じ）の結果は `results-r26.json` から使い、無いプログラムは基準と先端でも検査）、この回の探査（`copilot2/p/` の 88 個）を足して、15439 個・30878 組を両方の edition で検査しました（`results-r27.json`、分類は `diff-r27.txt`、`copilot2/gate-diff-r27.txt` に写し）。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 5240 | 4186 |
| 両方が拒否 | 5241 | 7167 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4920 | 4057 |
| 検査の失敗・時間切れ | 0 | 2 |

- **RA は 67 組で、前回と同じ 67 組です。新しい RA はありません。** 条件 (1) は成り立ちます。
- 前回からある 30702 組では、先端からの受理・拒否の変化も、最初の誤りの変化もありません。
- 足した 176 組での先端からの変化は、受理から拒否の 41 組だけで、拒否から受理はありません。41 組は、`u_ce_*`・`u_ke_*` の 9 つの形（`ce_while` は `u_ce_while` と同じプログラム）と `u_th_while` の両方の edition の 38 組、`u_helper_while` の両方の edition、`u_hidden_while` の edition 2027 で、どれも危険な形です。
- 時間切れの 2 組（240 秒）は、上の edition 2026 の `s_hidden_while`・`u_hidden_while` で、先端も同じく時間切れです。表では AR に数えています。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 生成したプログラム（`copilot2/g-check.mjs`、15552 個・31104 組。結果は `copilot2/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました。

条件 (3) の、`main` が受理する安全な形の拒否に、この回で増えたものはありません。edition 2026 の収束しない形は先端からあるもので、A33 と `lifetimes.md` に書きました。（この 2 つの文は誤りでした。後のレビューで、ループの中で置き換えてから新しい値の中身を写す形と、セルのフィールドへ借用を書き込んだ複数 region のレコードのループの形が、この修正で拒否に変わった安全な形であること、収束しない形が edition 2026 に限らないことが分かりました。下の「修正のレビュー」）

#### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3297 件すべて成功、4 件 skip（1801 秒、`copilot2/nextest.log`。この回の 1 件を含みます）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件（基準と先端のバイナリでも 85 件）、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`copilot2/suites.log`）。
- 手順 5 の比較（`_tools/step5.mjs`）: `main`（`168ae4a5`）と比べて 155 個のどれも出力が同じ（`copilot2/step5-main.txt`）。
- 検査の時間（`copilot2/perf.sh`、`TSUZURI_KG=0 TSUZURI_TIME_PASSES=1 tsuzuri check` の所有権の検査の中央値、交互に 5 回、ms。基準・先端・修正後）: `perf_loop_breaks_400` 143 / 390 / 366、`perf_try_checked_400` 225 / 402 / 377、`perf_ho_wrap_120` 0.55 / 2.68 / 2.61、`perf_ho_chain_320` 1.48 / 26.2 / 26.4、`drop_locals_600` 3.73 / 5.87 / 5.79、`pattern_param_chain_200` 1.38 / 6.84 / 7.05、`perf_match_nest_30` 1.40 / 1.60 / 1.51、`p01_relay_chain_400` 1.34 / 15.5 / 15.3、`x01_many_holders_800` 676 / 1254 / 1208、`x04_nested_loops_30` 4.48 / 8.12 / 8.01、`p_cellshares_400` 21.3 / 233 / 232、`p_cellshares_800` 85.5 / 995 / 995 で、先端と同じ程度です（`copilot2/time-perf.txt`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・63 個のリンク・11 個の例・22 回の native の実行、`copilot2/docs.log`）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）が成功（`copilot2/doc-checks.log`）。
- コミット: `df307296`（修正とテスト）、`aadc449d`（文書）、この節と A33 の検査の時間・`lifetimes.md` の制限。
- 変えたファイル: `src/ownership_control.rs`、`tests/rc.rs`（1 件）、`tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs`（1 件）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（この回に塞いだ穴の記録、検査の時間）、このチケット（「残る制限」と、この節）。

#### 修正のレビュー（2026-11-15）

この対応（`df307296`・`aadc449d`・`8fbcd40e`）のレビュー（反例の探索 `target/scratch/a31/copilot2-review-ce/`、適合の確認 `target/scratch/a31/copilot2-review-conf/`）の 4 件の指摘を、修正後（`8fbcd40e`。`src/` は `df307296` と同じ）・先端（`fbd7d194`）・`main`（`168ae4a5`）・基準のバイナリで確かめ、どれも指摘のとおりに再現しました。危険なプログラムの受理はありません。指摘 1・3 は修正による先端からの退行（`main` も先端も受理する安全な形の拒否）、指摘 2 は先端からある検査が終わらない形の文書の誤り（範囲が実際より狭い）、指摘 4 は文書の誤りです。この回の規則（検査を緩める変更はしない。安全な形の拒否は、`src/` を変えずに文書と A34 に書く）に従い、`src/` は `LoopSummary` の doc comment だけを直しました。結果は `target/scratch/a31/copilot2/rv/` にあります（`p/`・`ph/` が再現と探査、`a34/` が A34 に書いたプログラム、`tools/quick.sh` が修正後と `main` の両方の edition の最初の誤り。`../v.sh` は修正後・先端・`main`・基準を並べ、`RUN=1 ASAN=1` で `main` の native と ASan の `-O0`・`-O3` を足します）。

##### 指摘ごとの判定

| 指摘 | 確かめたこと | 対応 |
| --- | --- | --- |
| 1（ループの中で置き換えてから新しい値の中身を写す形の拒否。先端からの退行） | 再現（`p/s_rc_while` を修正後が両方の edition で `28:9`、範囲の `for` の `s_rc_forrange` を `27:9` の `E1014`（`borrow_conflict`）。先端・`main`・基準は受理し、`main` の native と ASan の `-O0`・`-O3` は手で求めた `drop 8 2`・`drop 0 2`・`drop 0 2`・`drop 0 2`・`0`、ASan の報告なし）。`if` の形 `s_rc_if` は先端も `27:5` の `E1014`（A31 の前からの拒否で、文書に無かったもの）。写した先を後で上書きする `u_ce_replin`（名前に反して安全）と入れ子の `u_ce_nestrepl` は修正後が `28:23`、先端は受理、`main` は `drop 8 2`・`drop 0 2`・`drop 0 2`・`0`。対照の `s_rc_twice`・`s_rc_nokept`・`s_rc_noloop` はどのバイナリも受理 | 文書（`drop.md` の「値全体の代入」、`lifetimes.md` の「現在の制限」、`docs/language.md`、`docs/architecture.md`）と A34 の `replcopy`（安全な形、同じく拒否する形、危険な双子 2 つ）、「残る制限」 |
| 2（検査が終わらない形は edition 2026 だけではない。文書） | 再現（`ph/h8_simple_own`（edition 2027、`ref mut Box` と `own` の `Arc`）を修正後と先端が両方の edition で 120 秒で打ち切り、`main`・基準は 1 秒以内に受理して `3`。`h10_localsink`・`h16_refparam_local`・`h2_own2027`・`h9_refp_own` も両方の edition で同じ。ループの無い `h12_noloop` はどれも受理）。範囲を広げて調べ（下）、条件を「先にセルを持つ参照の仮引数の先の借用を、書き込むセルを所有する関数値へ渡すループ」と特定 | 文書（`lifetimes.md` の「現在の制限」、`docs/architecture.md` の `E1017` の文）と A34 の `loopplace`（A33 の「検査の時間」から移した）、「残る制限」 |
| 3（セルのフィールドへ借用を書き込んだ複数 region のレコードの region。先端からの退行） | 再現（`p/rg_cell_while2` を修正後が両方の edition で `20:17`、`rg_cell_for2` を `18:17` の `E1014`（`borrow_conflict`）。先端・`main`・基準は受理し、`main` の native と ASan の `-O0`・`-O3` は手で求めた `836`、ASan の報告なし）。ループの無い `rg_cell_noloop2` と `if` の `rg_cell_if2` は先端も `17:17`（A31 の前からの拒否で、文書に無かったもの） | 文書（`lifetimes.md` の「現在の制限」の表と複数 region のレコードの段落、`docs/architecture.md` のスロットの段落）と A34 の `cellslots`、「残る制限」 |
| 4（`docs/architecture.md` と `LoopSummary` の doc comment が、比べない事実から `Loan::parents` を落としていた。文書） | 確認（`loan_key` は `(place, mutable, flags)` で、`Loan::parents` は `active`・`retain_live_loans`・`held_keeps`・`holders` と再借用した排他参照のムーブの検査が読む） | `docs/architecture.md` のループの不動点の段落と `src/ownership_control.rs` の `LoopSummary` の doc comment に、比べないものとその理由（再借用の連なりで組が伸び続ける。同じ場所と印の借用を別の親で得るには親の排他の借用と競合する）を書いた。上の「変えたこと」の「入れないもの」にも足した |

##### 原因

- **指摘 1**: `check_loop` の反復の始まりの状態は、入口の状態と `continue` の状態の合流（`State::merge` の和）です。`renew` はループの前に `p.a[0]` の借用 `t` を `p.h` のセルへ書き込むので、入口の状態の `kept` は `t` の借用を持ちます。1 回目の反復は、置き換えで `t` の借用を終わらせ（`replaced_target`）、新しい値の中身を写して `contents_elsewhere` に `p` を入れます。2 回目の反復の始まりの状態は、入口の `kept` の `t` の借用と、1 回目の反復の `contents_elsewhere` の `p` を合わせて持ち、`replaced_target` は根が `contents_elsewhere` にあると `kept` の借用を終わらせずに戻るので、置き換えが `t` の借用と競合します。実行では、この 2 つが同時に成り立つことはありません。先端は、要約が `contents_elsewhere` を比べなかったので 2 回目の反復を検査せず、受理していました（その抜けが Copilot の指摘した穴です）。`if` の合流も同じ和なので、`s_rc_if` は先端も拒否していました。
- **指摘 2**: セルを所有する関数値の呼び出しは、`store_hidden_except` で、セルに届く型か環境を隠す型のすべての局所値へ、渡した値の借用を格納します。先にセルを持つ参照の仮引数 `p` もその局所値なので、`p` の値は先の場所の借用を持つようになり、`Checker::place` は参照の先の場所を参照の値のすべての借用の場所から作るので、反復ごとに場所が 1 段ずつ深くなります。要約は場所を比べるので変わり続け、反復ごとに借用が増えて 1 回の反復の検査も重くなり、反復の上限（`MAX_NESTING` + 1 回、`E1017`）に届く前に検査が実用的な時間で終わりません（検査中のプロセスの標本は、`joint_facts` の中の本体の検査の `check_loop` の下の `access`・`active` でした。`rv/h8-sample.txt`）。関数値がセルを所有することが条件で、edition 2026 の値の仮引数に限りません（edition 2027 の `own` の仮引数、局所値の `Arc.share` の捕捉、部分適用）。
- **指摘 3**: `absorb_owner` は、局所の持ち主へ借用を持つ値を格納すると、格納先のフィールドを問わず、持ち主の region の slot を捨てます（`owner.regions = None`）。`let t = x.b` は `x` のすべての借用を持ち、`t` が生きている間の `eat s1` が競合します。先端は要約が slot を比べなかったので、ループの後は本体の前の状態（slot がある）に戻り、受理していました。修正が slot を要約に入れたので、ループの形もループの無い形と同じ拒否になりました。

##### 決めたこと

- 指摘 1 の勧め（根ごとに、写したときの `kept` の借用を記録し、置き換えはそれだけを残す）は、拒否を受理に変える変更で、写しの記録とループの要約の両方に関わるので、この回では入れず、A34 の `replcopy` の方針案にしました。危険な双子（置き換えの前に古い値の中身を写す形、片方の枝の写しを別の反復の置き換えが無効にする形）は、写しが `t` の借用を記録するので、その案でも拒否のままです。写した先を後で上書きする形（`u_ce_replin`）は、置き換えの前に古い値の中身を写しているので、その案でも拒否のまま残ります（セルへの書き込みを上書きとして扱わないため。A34 の「対象外」）。
- 指摘 2 は、直すには隠れた環境への格納先か参照の先の場所の求め方を変える必要があり、小さな変更ではないので、文書と A34 の `loopplace` に書きました。場所の長さに上限を設けて `E1017` で打ち切る締めつけも考えましたが、上限の値と、上限に届く既存の安全なプログラムが無いことを確かめる必要があり、検査の結果を変える変更なので、A34 の方針案 (c) に置きました（場所をまとめて受理に変える案です。検査を止めるだけの打ち切りは、後で入れました。下の「検査が終わらないループの打ち切り（2026-11-15）」、「借用の数による打ち切り（2026-11-15）」と「借用の増え方による打ち切り（2026-11-16）」）。`docs/architecture.md` の「上限を超える複雑な解析は `E1017` として安全に打ち切ります」は、反復の回数の上限と、その上限に届く前に終わらない形があることに書き直しました。
- 指摘 3 の勧め（region を持たないフィールドへの格納では slot を保つ）は、レコードリテラル（`record_value`）の扱いと同じですが、拒否を受理に変える変更なので入れず、A34 の `cellslots` の方針案にしました。

##### この回の確かめで見つけたもの

- 検査が終わらない形の範囲（`rv/f2-verdicts.txt`・`f2-scope.txt`・`f2-scope2.txt`。修正後を 20 秒か 120 秒で打ち切り）: 終わらないのは、edition 2027 の読み取りの仮引数 `Box`（`ph/h20_readparam`。edition 2026 では値の仮引数なので終わります）、セルの中身でなく `ref p.s` を捕捉した関数を渡す形（`h21_fieldborrow`、両方の edition）、範囲の `for`（`h25_forrange`、両方の edition）、edition 2026 の部分適用 `put (Arc.share (ref sink))`（`h15_partial`）、edition 2026 の値の仮引数の形（`h1_doc` = `s_hidden_while`・`h3_nokeep`・`h4_noreplace`・`h5_refp`・`h7_simple`）。終わるのは、ループの無い並び（`h12_noloop`）、局所値の先のセル（`h13_localowner`・`h14_localref`）、先にセルを持たない仮引数（`h17_array`・`h18_array_for`・`h19_recarray`・`h24_mutexparam`・`h26_fieldborrow_nocell`）、セルへの参照を捕捉した関数値（`p/s_loopplace_ref`。回避の書き方）です。危険な双子（`a34/u_loopplace`）も修正後と先端で終わらず、`main` は受理して `-1`（解放した文字列を読む）を出します。
- 指摘 1 の探査の `u_ce_replorcopy`（ループの本体を `if i > 0 then deref p = mkpa () else` 写し、にしたもの）は、先端が受理し、`main` の ASan の `-O0`・`-O3` が heap-use-after-free の危険な形で、修正後は `28:23` の `E1014` です（Copilot のレビュー 2 の修正で塞いだ穴。A34 の `replcopy` の危険な双子 2）。
- `replcopy` の回避の書き方（最初の反復をループの前に書く `p/s_replcopy_peel2`）は受理し、`if n > 0` で囲んだ形（`s_replcopy_peel`）は合流で `32:9` の `E1014` です。`cellslots` の回避の書き方（書き込みの前に読む `p/s_cellslots_readfirst`）は受理します。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/collect-r28.mjs`・`check-r28.mjs`・`diff-r28.mjs`・`table-r28.mjs` に写し、この回のプログラム（レビューの `copilot2-review-ce/`・`copilot2-review-conf/` と `copilot2/rv/`）のうち新しい 111 個を足して、15550 個・31100 組を両方の edition で検査しました（`r28` はこの回のビルド。`src/` の変更は doc comment だけです）。前回からある組は、基準・先端（`r26`、`fbd7d194`）・前回の修正後（`r27`、`df307296`）の結果を `results-r27.json` から使い、`r28` はすべての組で検査し直しました。新しいプログラムは 4 つのバイナリで検査しました（120 秒で打ち切り）。結果は `results-r28.json`、分類は `diff-r28.txt`（`copilot2/rv/gate-diff-r28.txt` に写し）、新しい組の一覧は `copilot2/rv/gate-fresh-r28.txt` です。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 5272 | 4209 |
| 両方が拒否 | 5259 | 7190 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4981 | 4122 |
| 検査の失敗 | 0 | 0 |
| 時間切れ（AR に数える） | 9 | 20 |

- **RA は 67 組で、前回と同じ 67 組です。新しい RA はありません。** 条件 (1) は成り立ちます。
- `r27` から `r28` への受理・拒否の変化はどの組にもありません（前回からある 30878 組のうち、最初の誤りの文字列が変わったのは、前回 `FAIL SIGKILL` と記録した edition 2026 の `s_hidden_while`・`u_hidden_while` の 2 組が `TIMEOUT` になったことだけで、道具が時間切れを別に記録するようにしたためです）。新しい 222 組でも `r27` と `r28` の結果は同じです。
- 先端（`r26`）から `r28` への変化は、受理から拒否の 98 組だけで、拒否から受理はありません。98 組は、前回の 41 組（危険な形）と、新しい 57 組です。新しい 57 組は、危険な形（`u_ce_contlast`・`u_ce_forown`・`u_ce_matcharms`・`u_ce_tryaround`・`u_ce_replorcopy`、`u_hid_*`（edition 2027）、`u_th_*`、`l_ce_breakrepl`・`l_ce_second`・`l_ke_second`、例外のハンドラーの中で写す `l_ce_try_handler`・`l_ke_try_handler`（`@checked i * 2` が溢れる入力では写しの後に置き換える））と、文書に書いた安全側の制限（`replcopy`: `s_replcopy`・`s_rc_forrange`・`u_ce_replin`・`u_ce_nestrepl`。`cellslots`: `s_cellslots`・`rg_cell_for2`。前回の文書の「写しや別の仮引数の先への書き込みがループの本体にあっても同じ」に当たる、先の別のセル `h2` の中身を写す `v2_keep_copyh2_while`（ループの無い形は先端も拒否））のどれかです。
- 時間切れの 29 組は、どれも `loopplace` の形（edition 2026 の `u_hid_*`・`s_hidden_while`・`u_hidden_while` を含む）で、先端と前回の修正後も同じく時間切れ、基準は受理します。
- 基準が受理し対応後が拒否する新しい組のうち名前が危険を示さないものは、上の文書に書いた制限、危険な形（`l_*`。条件の中で写す `l_ce_whilecond_true`・`l_ke_whilecond_true` とラムダの中で格納する `l_ke_lambda` は先端も拒否）、時間切れの `loopplace` の形、`if n > 0` で囲んだ回避の書き方 `s_replcopy_peel`（A34 の `replcopy` に書いた形）のどれかです。
- 基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 生成したプログラム（`copilot2/rv/g-check.mjs`、15552 個・31104 組を `r28` のビルドで。結果は `copilot2/rv/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました。

条件 (3) の、`main` が受理する安全な形の拒否のうち、この回の前の修正で増えたもの（`replcopy`・`cellslots` のループの形）と、前からあって文書に無かったもの（`replcopy` の `if` の形、`cellslots` のループの無い形と `if` の形）は、文書と A34 に書いた安全側の制限です。検査が終わらない形（`loopplace`）は先端からあるもので、範囲を正して文書と A34 に書きました。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功（`rv/clippy.log`）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3297 件すべて成功、4 件 skip（2509 秒、`rv/nextest.log`。この回はテストを足していません。`src/` の変更は doc comment だけです）。
- E2E（開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`rv/suites.log`）。
- 文書: `node scripts/check-docs.mjs`（`drop.md`・`lifetimes.md`、2 ページ・67 個のリンク・11 個の例・22 回の native の実行、`rv/docs.log`）、`node scripts/gen-docs.mjs --check`（19 個のブロック）、`node scripts/check-cli-docs.mjs`（4 個の検査）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs`（117 個のリンク）が成功（`rv/doc-checks.log`）。
- コミット: `37817ff2`（文書と `LoopSummary` の doc comment）、`1b1c3afb`（A34 の `replcopy`・`cellslots`・`loopplace`、A33、`_features/README.md`）、この節。
- 変えたファイル: `src/ownership_control.rs`（doc comment）、`drop.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、A33（「検査の時間」の段落を A34 への案内に、表の A34 の行、記録）、A34（3 つの項目）、`_features/README.md`（A34 の行）、このチケット（「入れないもの」、条件 (3) の誤った文への注記、「残る制限」、この節）。

#### 検査が終わらないループの打ち切り（2026-11-15）

この節の仕組み（`Checker::leaves_type`、9 回）は、下の「借用の数による打ち切り（2026-11-15）」で、反復ごとの借用の数の上限（`LOOP_LOAN_GROWTH`）に置き換え、さらに「借用の増え方による打ち切り（2026-11-16）」で、借用の増え方の打ち切り（`LoanGrowth`）に改めました。

この節の数え方（`Checker::leaves_type`、9 回）は `773ea1c4` のものです。そのレビューで、型から外れる場所の判定が誤って収束するループを止めることと、止まらない形が残ることが分かり、`5f3c2a0b` で置き換えました（下の「打ち切りのレビューへの対応（2026-11-15）」）。

上の「修正のレビュー」の指摘 2（`loopplace`）は先端からある形ですが、`main` が 1 秒以内に受理する短い安全なプログラムの検査が終わらないコンパイラは出荷できないので、この回の規則（検査を緩める変更はしない。許すのは締めつけと、解析を止めることだけ）の中で、検査が終わらない代わりに `E1017` で止まるようにしました。収束するループの結果は変えず、`loopplace` の安全な形の受理は A34 に残します。結果は `target/scratch/a31/loopplace/` にあります（`profile.md` と `prof/` が計測、`t.sh` が最初の誤りと検査の時間、`tests/` が下のテストのプログラム、`conv2/` が参照の連なり、`wk/` が回避の書き方の形、`bin/` がこの回のビルド）。

##### 何が増えるか（計測）

`check_loop`・`Checker::place`・`settle` に一時的に計測を入れた開発用のビルド（計測は戻しました）で、ユーザーのループの反復ごとの状態を記録しました（時間はループの検査を始めてからの、反復の始まりまでの時間）。

| 反復（0 から） | `h8_simple_own`（edition 2027）: 時間 | loan の数 | いちばん深い場所の段数 | 局所値が持つ loan | `u_hidden_while`（edition 2026）: 時間 | いちばん深い場所の段数 | 局所値が持つ loan |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2 | 0.025 秒 | 9 | 2 | 8 | 0.001 秒 | 4 | 8 |
| 6 | 0.027 秒 | 77 | 6 | 128 | 0.003 秒 | 12 | 128 |
| 10 | 0.080 秒 | 1045 | 10 | 2048 | 0.054 秒 | 20 | 2048 |
| 11 | 0.231 秒 | 2071 | 11 | 4096 | 0.210 秒 | 22 | 4096 |
| 12 | 0.930 秒 | 4121 | 12 | 8192 | 0.921 秒 | 24 | 8192 |
| 13 | 4.045 秒 | 8219 | 13 | 16384 | 4.115 秒 | 26 | 16384 |
| 14 | 18.463 秒 | 16413 | 14 | 32768 | 18.973 秒 | 28 | 32768 |

- いちばん深い loan の場所は、反復ごとに書き込む道の長さ（`p.cell` は 1 段、`p.h.cell` は 2 段）ずつ深くなり、局所値が持つ loan の数は反復ごとに倍になります。`deref p` の場所（`Checker::place` の `E::Dereference`）は反復 `k` で `2^k` 個で、`p` の loan ごとに 1 つ（場所は `p.cell` に `cell` を `k` 回付けたもので、場所が同じで親が違うものが多い）です。`settle` の pass は呼び出しごとに 2 回のままでした。
- 1 回の反復の時間は前の反復の約 4 倍です（`active` が、アクセスのたびに局所値が持つすべての loan とその親をたどり、アクセスの数も場所の数とともに増えるため）。上限（129 回）までの反復を検査することはなく、14 回目の反復（反復 13）が約 4 秒、15 回目が約 18 秒の時点で始まります。
- 検査が終わらない探査（`copilot2/rv/ph/*`・`copilot2/rv/a34/{s,u}_loopplace`・`copilot2-review-conf/ph/*`・`copilot2/p/{s,u}_hidden_while`、両方の edition の 47 組。`prof/*.txt`）は、どれも同じ形でした。

##### 変えたこと（`src/ownership_control.rs`、`773ea1c4`）

- `check_loop` は、収束しなかった反復（最初の反復を除く）のうち、型から外れる場所の loan を、それまでの反復が作ったどの型から外れる場所よりも深く作った反復を数え、`DEEPER_ITERATIONS`（9）回になると `E1017`（`loop_ownership_limit`）を報告します。位置と文は反復の上限の `E1017` と同じ（ループの本体、`loop ownership analysis exceeds the compiler limit; simplify loop-carried borrows`）で、`help` が回避の書き方を言います（「the loop's borrows reach a deeper place in every iteration and never settle; when the loop passes a borrow of a reference parameter's target to a function value that owns a cell (a lambda that captures an `Arc` by value), capture a reference to the cell instead, such as `let cell = Arc.get (ref sink)`」）。反復の上限の `E1017` は前のままで、`help` はありません。
- 型から外れる場所（`Checker::leaves_type`）は、その段の型に無い段を持つ場所です: レコードとタプルでない値のフィールドか、レコードやタプルに無いフィールド、コレクションでない値の要素。各段は `place_type` と同じく `Rc`・`Arc` を読み通します。共用体のペイロードの後（どのケースか場所が言わない）、型変数・その適用・`dyn` の値、型の分からない根（静的データ）は外れるとしません。region の slot の印は段に数えません（`unmarked_fields`）。`loopplace` の場所は、`record Mutex<'a> { cell: [(i32u * 'a)] }` なので `p.cell.cell` までは型に沿い、3 回目の反復の `p.cell.cell.cell`（配列のフィールド）から外れます（`ref p.s` を捕捉する形は 2 回目の反復の `p.s.s`（文字列のフィールド）から）。
- 収束するループの結果を変えない理由: 数えるのは収束しなかった反復だけで、最初の反復は数えないので、10 回以内の反復で収束するループは 9 回に届きません。場所が型に沿ってだけ深くなるループは数えられません（共用体を通らない型の入れ子には限りがあるので、そうした場所は限りなく深くはなりません。ペイロードの後は外れるとしないので、共用体を通るものも数えません）。止まりうるのは、11 回以上の反復で収束し、そのうち 9 回で型から外れる場所がそれまでより深くなるループだけで、ゲートのコーパス・生成したプログラム・テストに無いことを下で確かめました。（2 つ目の理由は誤りでした: 場所のフィールドを根の宣言の型から読むので、関数の呼び出しの結果、region を持つレコードの仮引数、仮引数が持つ参照から始まる連なりは、型に沿っていても型から外れると読まれ、10 個以上の連なりが `E1017` になりました。region の slot の印の後のフィールドも、印の region の先の型のものです。下の節で直しました。）
- 退けた案: (1) 型を問わず、それまでより深い場所を作った反復を数える案は、それぞれが次の参照の先のフィールドを指す 11 個以上の参照の連なり（`conv2/c11`〜`c13`。`q0 = ref (deref q1).a` … のループで、先端・`main` とも受理）を `E1017` にしたので採りませんでした（反復ごとに場所が深くなり、連なりの数の反復の後に収束します）。(2) `place_type` が型を返さない場所を数える案は、`loopplace` の場所では同じ結果ですが、静的データの根・ペイロード・region の slot の印を持つ場所も数えるので、専用の `leaves_type` にしました。(3) 関数の構文から作れる深さに余裕を足した絶対の上限は、呼び出し先の事実が足す道（`kept_copy`・`path_copy`）を数えると `loopplace` の打ち切りが遅れすぎ（反復の時間が 4 倍ずつ増えるため）、数えないと事実を通した連なりを止めうるので採りませんでした。(4) 場所を大きな部分にまとめてループを収束させる案は受理に変える変更なので、A34 の方針案 (c) のままです。
- `help` の書き方は、知られているどの形でも正しいことを確かめました: 関数値に `let cell = Arc.get (ref sink)`（局所値の形は `Arc.get (ref sink2)`）を捕捉させた形（`wk/w_*`、15 の形）は両方の edition で受理し、native の `-O0`・`-O3` の出力は `main` と同じ（`3`、`ref p.s` の形は `4`、`Holder` を持つ形は `drop 0 2`・`drop 8 2`・`8` など。`wk-runs.txt`）です。同じ書き方にした危険な双子（`wk/w_u_loopplace`・`w_u_hidden_while`）は、`19:13` の `E1013`（`assignment_extends_borrow`）と `32:5` の `E1014` で拒否し、`main` は受理します。

##### 時間

リリースビルドの `r29`（`loopplace/bin/tsuzuri-r29`）で、キャッシュを温めた後に 3 回ずつ `check` を測った中央値です（`loopplace/time.sh`、`time-probes-r29.txt`。std を含むプログラム全体の経過時間と CPU 時間）。ほかの作業で機械の負荷が高いとき（load average 26〜42）に測ったので、経過時間は CPU 時間より長く出ています。

| 探査 | edition 2027 | edition 2026 |
| --- | --- | --- |
| `a34/s_loopplace` | `E1017`、1.19 秒（CPU 0.73 秒） | `E1017`、1.30 秒（CPU 0.73 秒） |
| `a34/u_loopplace` | `E1017`、1.20 秒（CPU 0.73 秒） | `E1017`、1.25 秒（CPU 0.73 秒） |
| `ph/h10_localsink` | `E1017`、1.38 秒（CPU 0.80 秒） | `E1017`、1.38 秒（CPU 0.79 秒） |
| `ph/h15_partial` | `E1013`、0.43 秒（CPU 0.25 秒） | `E1017`、1.26 秒（CPU 0.79 秒） |
| `ph/h16_refparam_local` | `E1017`、1.38 秒（CPU 0.79 秒） | `E1017`、1.32 秒（CPU 0.79 秒） |
| `ph/h1_doc` | 受理、0.99 秒（CPU 0.28 秒） | `E1017`、0.66 秒（CPU 0.40 秒） |
| `ph/h20_readparam` | `E1017`、1.11 秒（CPU 0.69 秒） | 受理、0.39 秒（CPU 0.28 秒） |
| `ph/h21_fieldborrow` | `E1017`、0.67 秒（CPU 0.38 秒） | `E1017`、0.56 秒（CPU 0.38 秒） |
| `ph/h25_forrange` | `E1017`、1.14 秒（CPU 0.70 秒） | `E1017`、1.37 秒（CPU 0.77 秒） |
| `ph/h2_own2027` | `E1017`、0.73 秒（CPU 0.38 秒） | `E1017`、0.70 秒（CPU 0.40 秒） |
| `ph/h3_nokeep` | 受理、0.50 秒（CPU 0.27 秒） | `E1017`、0.67 秒（CPU 0.40 秒） |
| `ph/h4_noreplace` | 受理、0.48 秒（CPU 0.26 秒） | `E1017`、0.65 秒（CPU 0.39 秒） |
| `ph/h5_refp` | 受理、1.15 秒（CPU 0.27 秒） | `E1017`、0.69 秒（CPU 0.39 秒） |
| `ph/h7_simple` | 受理、0.40 秒（CPU 0.27 秒） | `E1017`、1.23 秒（CPU 0.71 秒） |
| `ph/h8_simple_own` | `E1017`、1.03 秒（CPU 0.65 秒） | `E1017`、1.26 秒（CPU 0.70 秒） |
| `ph/h9_refp_own` | `E1017`、0.68 秒（CPU 0.39 秒） | `E1017`、0.63 秒（CPU 0.39 秒） |
| `ph/s_hidden_while` | 受理、0.51 秒（CPU 0.28 秒） | `E1017`、0.62 秒（CPU 0.40 秒） |
| `ph/u_hidden_while` | `E1014`、0.48 秒（CPU 0.29 秒） | `E1017`、0.67 秒（CPU 0.40 秒） |
| `p/u_hid_nested` | `E1014`、0.48 秒（CPU 0.28 秒） | `E1017`、0.66 秒（CPU 0.40 秒） |
| `conv2/c12` | 受理、0.64 秒（CPU 0.36 秒） | 受理、0.58 秒（CPU 0.37 秒） |
| `wk/w_h8_simple_own` | 受理、0.48 秒（CPU 0.28 秒） | 受理、0.45 秒（CPU 0.27 秒） |

- `E1017` で止まる形は、どれも経過時間 1.4 秒以内（CPU 0.8 秒以内）です。同じ大きさの受理するプログラム（回避の書き方の `wk/w_h8_simple_own` など）が CPU 0.27 秒ほどなので、打ち切りまでのループの検査は CPU 0.1〜0.5 秒です。開発用のビルドでは、負荷の低いときに経過時間 0.3〜1.1 秒でした（`t.sh`）。`15d84aaf` は同じ形を 120 秒で打ち切っても終わらず（`tests-head-120.txt`）、`main` は 1 秒以内に受理します。
- `p.cell` の形（1 段ずつ深くなる）は 3 回目の反復から型から外れ、11 回目の反復の後に止まります。`p.h.cell` の形（2 段ずつ）と `ref p.s` の形は 2 回目の反復から外れ、10 回目の反復の後に止まり、借用も少ないので早く止まります（CPU 0.4 秒ほど）。

##### テスト

- `tests/rc.rs` の `a_loop_whose_borrows_keep_getting_deeper_stops_with_e1017`: `own` の仮引数の形（`h8`、A34 の安全な形）、`ref p.s` を捕捉した関数を渡す形、局所値の `Arc.share` を捕捉する形、危険な双子を、両方の edition でループの本体の位置（`7:9`・`7:9`・`9:9`・`12:9`）の `E1017`（`loop_ownership_limit`）と回避の書き方の `help` で止めること、edition 2026 の値の仮引数の形を `7:9` で止め、edition 2027 では受理すること（読み取りの仮引数を捕捉したラムダはセルを所有しません）、回避の書き方にした安全な形を両方の edition で受理し、双子を `19:13` の `E1013` で拒否すること、12 個の参照の連なり（12 回目の反復まで場所が深くなり、13 回目で収束する）を両方の edition で受理することを固定しました。位置は、反復の上限の `E1017` と同じループの本体の最初の式で、手で数えました。
- 同じプログラムを `15d84aaf` のビルドで 120 秒で打ち切ると、止める形（`own`・`field`・`local`・`twin` の両方の edition と、`value` の edition 2026）はどれも時間切れで、連なりと回避の書き方の形は受理、回避の書き方の双子は `19:13` の `E1013` でした（`loopplace/tests-head-120.txt`）。

##### マージの条件の確かめ（この回の後）

上の道具を `gate/tools/check-r29.mjs`・`diff-r29.mjs`・`table-r29.mjs` に写し、15550 個・31100 組のすべてを `r29`（`773ea1c4` の開発用のビルド。`r26`〜`r28` と同じ開発用のビルドにしました。`gate/bin/REV`）で、両方の edition で検査し直しました（新しいプログラムは足していません。240 秒で打ち切り）。基準・先端（`r26`、`fbd7d194`）・`r27`（`df307296`）・`r28`（`15d84aaf` と同じ `src/`）の結果は `results-r28.json` から使いました。結果は `results-r29.json`、分類は `diff-r29.txt`（`loopplace/gate-diff-r29.txt` に写し）です。

| 分類 | 2027 | 2026 |
| --- | --- | --- |
| 両方が受理 | 5272 | 4209 |
| 両方が拒否 | 5259 | 7190 |
| 基準が拒否し対応後が受理（RA） | 38 | 29 |
| 基準が受理し対応後が拒否（AR） | 4981 | 4122 |
| 検査の失敗 | 0 | 0 |
| 時間切れ | 0 | 0 |

- **`r28` が検査を終えた 31071 組は、どれも受理・拒否と、道具が記録する最初の誤り（最初の 2 つの誤りの code・kind・位置）が `r28` と同じです。** 変わったのは `r28` の時間切れの 29 組（edition 2027 が 9 組、2026 が 20 組）だけで、どれもループの本体の位置の `E1017`（`loop_ownership_limit`）になりました: `a34/s_loopplace`（両方の edition、`7:9`）・`a34/u_loopplace`（両方、`12:9`）、`ph/h10_localsink`・`h16_refparam_local`（両方、`9:9`）・`h21_fieldborrow`・`h25_forrange`（両方、`7:9`）・`h2_own2027`（両方、`30:9`）・`h9_refp_own`（両方、`27:9`）・`h20_readparam`（2027、`7:9`）・`h15_partial`（2026、`10:9`）・`h3_nokeep`（2026、`28:9`）・`h4_noreplace`・`h5_refp`（2026、`27:9`）・`h7_simple`（2026、`7:9`）、edition 2026 の `copilot2/p/s_hidden_while`（`30:9`）・`u_hidden_while`（`29:9`）、`copilot2-review-ce/p/u_hid_breakfirst`・`u_hid_continue`・`u_hid_tryin`（`29:9`）・`u_hid_forrange`（`28:9`）・`u_hid_nested`（`31:13`、内側のループ）。
- RA は 67 組で、前回と同じ 67 組です。新しい RA はありません（条件 (1)）。受理から拒否に変わった組はなく（`r28` が受理したものはどれも受理のまま）、先端（`r26`）からの変化は前回と同じ受理から拒否の 98 組だけで、拒否から受理はありません。基準も対応後も受理するプログラムのうち名前が危険を示すものは、前回と同じ 33 組です。
- 時間切れだった危険な形（`u_loopplace`・`u_hidden_while`・`u_hid_*`）は、検査が終わって拒否になりました（条件 (2)）。安全な形（`s_loopplace`・`s_hidden_while`・`h*`）は、前も受理していなかった（時間切れ）ので、条件 (3) の「`main` が受理する安全な形」の拒否として、文書（`lifetimes.md` の「現在の制限」）と A34 の `loopplace` に書いた安全側の制限です。
- 生成したプログラム（`loopplace/g-check.mjs`、15552 個・31104 組を `r29` のリリースビルドで。結果は `loopplace/g/g-results.json`・`g-summary.json`）: 危険な 18048 組はどれも拒否し（条件 (2)）、安全な 13056 組はすべて受理しました。31104 組のどれも、結果（受理か、最初の誤りの code と位置）が `r28` の `copilot2/rv/g-results.json` と同じです。
- 探査（`copilot2/rv/ph/*`・`copilot2/rv/a34/{s,u}_loopplace`・`copilot2-review-conf/ph/*`・`copilot2/p/{s,u}_hidden_while`、両方の edition の 100 組。`loopplace/probes-all-head.txt`・`probes-all-r29.txt`）: `15d84aaf` で 15 秒で打ち切った 47 組はどれも `E1017` になり、ほかの 53 組は最初の誤りまで同じでした。
- `examples/` と `tests/fixtures/` の 155 個のプロジェクト（`_tools/step5.mjs`、`check --json` と `build --no-cache --emit llvm`）を `main`（`168ae4a5`）と `r29` で比べました: 155 個のどれにも違いはありません（`loopplace/step5-main.txt`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功（`loopplace/clippy.log`）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3298 件すべて成功、4 件 skip（1479 秒、`loopplace/nextest-2.log`。新しいテスト 1 件を含みます）。最初の実行（`nextest.log`）では、`E1017` の `tsuzuri explain` のページに回避の書き方を 7 つ目の対処として足したので `tests/explain.rs` の `every_code_has_a_page`（対処は 6 つまで）が失敗し、ループの本体の対処の項目にまとめて（`2287d47f`）から通しました。
- E2E（`r29` の開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`loopplace/suites.log`）。
- 文書: `node scripts/check-docs.mjs`（`lifetimes.md`・`diagnostics.md`、2 ページ・83 個のリンク・7 個の例・14 回の native の実行、`loopplace/docs.log`）、`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 個のブロック、古いもの 0）、`node scripts/check-cli-docs.mjs`（4 個の検査）、`node scripts/check-guide-lite.mjs`（88 個の参照）、`node scripts/check-links.mjs --into`（変えた `_features/` と言語リファレンスのページへの 128 個のリンク、壊れたもの 0）が成功。
- コミット: `773ea1c4`（`src/ownership_control.rs` の打ち切りと `tests/rc.rs` のテスト）、`2d3c9f5a`（文書）、`2287d47f`（`E1017` のページの対処の数）、この節。
- 変えたファイル: `src/ownership_control.rs`、`tests/rc.rs`（1 件）、`src/explain/codes/E1017.md`、`lifetimes.md`（「現在の制限」）、`compiler/diagnostics.md`（`E1017` の行）、`docs/language.md`（`E1017` の行）、`docs/architecture.md`（ループの不動点の段落）、A34（`loopplace` の「現状」・原因・方針・テスト計画・受け入れ条件、題名）、A33（「検査の時間」）、`_features/README.md`（A34 の行）、このチケット（「残る制限」と上の「決めたこと」の案内、この節）。
- 性能: 所有権の検査の時間は前回と同じ組で変わりません（下の表）。

所有権の検査の時間（`loopplace/perf.sh`。`TSUZURI_TIME_PASSES=1` の `ownership` の段の ms を、`r28`（`15d84aaf` の `src/`）と `r29` の開発用のビルドで交互に 7 回ずつ測った中央値。前回と同じ組。`loopplace/time-perf-2.txt`）:

| プログラム | `r28`（ms） | `r29`（ms） |
| --- | --- | --- |
| `perf_loop_breaks_400` | 730.424 | 751.984 |
| `perf_try_checked_400` | 738.886 | 726.257 |
| `perf_ho_wrap_120` | 4.394 | 4.234 |
| `perf_ho_chain_320` | 45.904 | 48.621 |
| `drop_locals_600` | 8.668 | 9.288 |
| `pattern_param_chain_200` | 11.849 | 12.517 |
| `perf_match_nest_30` | 2.272 | 2.329 |
| `p01_relay_chain_400` | 27.814 | 27.452 |
| `x01_many_holders_800` | 2346.298 | 2329.912 |
| `x04_nested_loops_30` | 12.799 | 14.934 |
| `p_cellshares_400` | 411.884 | 390.858 |
| `p_cellshares_800` | 1725.426 | 1746.188 |

差は揺れの範囲です。差がいちばん大きかった `x04_nested_loops_30` を 15 回ずつ測り直すと 12.16 ms と 12.38 ms、`perf_loop_breaks_400` は 670.6 ms と 681.4 ms でした（`time-perf-3.txt`。最初の 5 回ずつの測定 `time-perf.txt` は負荷の高いときのもので、値は約 2 倍ですが、同じく揺れの範囲です）。打ち切りの検査は、収束しなかった反復で作った loan を 1 度ずつたどり、型をたどるのはそれまでの型から外れる場所より深いもの（最初はすべて）だけなので、ループの多い関数でも遅くなりません。

#### 打ち切りのレビューへの対応（2026-11-15）

この節の仕組み（`artifact_loans`・`DEEPER_ITERATIONS`）は、下の「借用の数による打ち切り（2026-11-15）」で、反復ごとの借用の数の上限（`LOOP_LOAN_GROWTH`）に置き換え、さらに「借用の増え方による打ち切り（2026-11-16）」で、借用の増え方の打ち切り（`LoanGrowth`）に改めました。

上の打ち切り（`773ea1c4`）のレビューの 6 件の指摘（反例の探索 `target/scratch/a31/loopplace-review-ce/`、適合の確認 `loopplace-review-conf/`）は、どれも確かめられました。この回の規則（締めつけと解析を止めることだけ。`15d84aaf` が収束させたループの結果は変えない）で直しました。結果は `target/scratch/a31/loopplace2/` にあります（`bin/` がこの回のビルド、`cmp.sh`・`one.sh` が新しいビルド・`a8623235`・`15d84aaf`・`main` の最初の誤り、`probes.sh` が前回の 100 組、`q/` が足した形、`tests/` が下のテストのプログラム）。

| 指摘 | 判定と対応 |
| --- | --- |
| 1・4（結果が変わった: 近似した場所から始まる 10 個以上の参照の連なりが `E1017`） | 確かめました。`leaves_type` は場所のフィールドを根の宣言の型から読みますが、関数の呼び出しの結果は引数の借用（`top1 (ref y1)` の結果は `y1` 全体の借用）を持ち、region を持つレコードの仮引数の外部の借用や、仮引数が持つ参照（`w.r`）から読んだ借用は、参照の先を表すので、フィールドは別の値のものです。型から外れる場所の判定をやめ、どの値にも無い場所だけを数えるようにしました（下）。レビューの連なり（`call_*`・`chain_*`・`refw-*`・`valw-*`・`arrw-*`・`generic-12`・`capturew-12`）はどれも、両方の edition で `15d84aaf`・`main` と同じく受理します |
| 2（終わらない: フィールドが `dyn` の値の形） | 確かめました。`leaves_type` は `dyn` の値の後の段を外れるとしないので数えませんでした。新しい数え方は型の段を見ないので、`d_field`・`d_arr`・`d_local` と、`main` が受理して実行が SIGSEGV になる危険な双子 `d_twin` を、両方の edition で `7:9`（局所値の形は `9:9`）の `E1017` で止めます（開発用のビルドで 0.3〜1.2 秒）。回避の書き方にすると、安全な形は受理、双子は `15:13` の `E1013` です |
| 5（終わらない: 借用を局所値に渡してから関数値へ渡す形） | 確かめました（`lag2` は `773ea1c4` で 146 秒、`lag3` は 1 時間ほど）。いちばん深い場所は `k + 1` 回の反復に 1 回しか深くなりませんが、`t0`・`t1`・`p` が順に深くなるので、保持者ごとに数えるようにしました。`lag1`〜`lag3` と足した `lag4`・`lag6`・`lag9`（`q/`）は、両方の edition でループの本体の `E1017` で 1 秒以内に止まります |
| 3・6（文書・テストの説明） | 確かめました。`lifetimes.md`・`diagnostics.md`・`docs/language.md`・`docs/architecture.md`・A34・A33、`DEEPER_ITERATIONS` とテストの doc comment を新しい数え方に書き直し、上の節の誤った理由に注を付けました。テストの説明の「10 回」は、新しい数え方で数える 8 回（10 回目の反復の後に止まる）にしました |

##### 変えたこと（`src/ownership.rs`・`src/ownership_control.rs`、`5f3c2a0b`）

- 場所の借用（`ref`・`ref mut`・スライス・読み取りの借用・パターンの view）は、借用した式の型を `loan_types` に記録し（同じ場所の写しにも写します）、どの値にも無い場所の借用（`artifact_loans`）かを決めます（`Checker::artifact_place`）。場所が `deref r` から来て、その参照外しが返した借用（`Checker::place` の `via` の最も新しいもの。親はそれより前に作られます）が、(1) それ自身どの値にも無い場所の借用か、(2) `r` が束縛されたときに持っていた場所を指す変更できない参照の局所値（`pointing`）で、束縛のときに持っていなかった借用（後の格納が足したもので、先が持つもの）か、(3) 借用した値の型から `r` の先の型に届かない（`Checker::reaches`: レコードのフィールドと型引数、共用体のペイロードと型引数、タプルとコレクションの要素、参照の先、`Rc`・`Arc` のブロック、関数値の結果、プログラムが `dyn` へ入れるすべての型をたどり、型変数とその適用、extern のハンドルはすべてに届きます。文字列はレコード・共用体・タプル・関数・`dyn`・ブロック・参照以外に届きます）借用なら、そうした場所です。(2)・(3) のとき、参照外しした式の根の局所値を元（`artifact_sources`）として覚えます。写しは印を受け継ぎます（場所を深くする `kept_copy`・`path_copy` は型を受け継ぎません）。
- `check_loop` は、収束しなかった反復（最初を除く）の終わりに、反復の始まりにもそうした借用を持っていて、それより深く持つ保持者（局所値とその region の slot、`kept`、`kept_elsewhere`）があるかを見て、元の局所値自身が深くなった反復から後、そうした反復が `DEEPER_ITERATIONS`（8）回になると、前と同じ `E1017` と `help` を報告します。`leaves_type` は消しました。
- `loopplace` の形の数え方: `deref p` が格納の足した借用（`p` が束縛のときに持っていなかったもの）の場所を返し、2 回目の反復がそのフィールドの借用（`p.s.s`）を作って `p` へ格納し、3 回目の反復から毎回 `p` が深くなるので、10 回目の反復の後に止まります（前と同じ反復。`p.cell` の形は前より 1 回早くなりました）。
- 収束するループの結果を変えない理由: 数えるのは収束しなかった反復だけで、10 回以内の反復で収束するループは届きません。参照の連なりは、局所値・仮引数・関数の呼び出しの結果のどこから始まっても、参照外しが返す借用が参照の指すものかそれに届く値の借用なので、どの値にも無い場所を作りません。ループの前の格納で作ったそうした場所を写したり、そこから借用を連ねたりするだけのループ（`q/art_copy`・`art_deref`・`art_each`。`art_deref`・`art_each` は `773ea1c4` が `E1017` にしていました）は、保持者がそれぞれ一度受け取るだけで、元の局所値も深くならないので数えません。止まりうるのは、元の局所値へそうした場所を戻して深くしながら、なお 11 回以上の反復で収束するループだけで、そうした形は知られていません（そうした戻りは反復ごとに場所を深くするので、収束しません）。
- 退けた案: (1) `leaves_type` の根の型をあいまいにする（各段の前に、届くすべての型を許す）案は、近似には効きますが、`Box` に深い入れ子のフィールドがあると `loopplace` の打ち切りがその深さの分だけ遅れるおそれがあり（反復ごとに時間が約 4 倍）、`dyn` の値も止められないので、試さずに退けました。(2) 外部の根と slot の印だけを除く案は、関数の呼び出しの結果（局所値の根）の連なりを止めたままでした。(3) 保持者ごとに数えるだけ（元の局所値の条件なし）の案は、`art_each`（ループの前の格納のそうした場所から始まる 12 個の連なり。`15d84aaf`・`main` は受理）を止めたので、元の局所値が深くなる条件を足しました。

##### テスト

- `tests/rc.rs` の `a_loop_stops_only_when_places_that_no_value_has_keep_getting_deeper`（新しい 1 件）: 12 個の参照の連なりを、関数の呼び出しの結果・region を持つレコードの仮引数・参照の仮引数が持つ参照・ループの前の格納のどの値にも無い場所から始めた 4 つの形で、両方の edition で受理すること（`a8623235` はどれも `E1017`、`15d84aaf`・`main` は受理。`main` の実行は連なりの形が `3`、最後の形が `16`。手で求めた値: 3 回の反復の後の `q0` は `R0`（`v` が 1）、`q1` は `R1`（2）で、最後の形はセルの関数の `t.v` が 13）、`dyn` の値のフィールドの形とその危険な双子、借用を局所値に 2 つ・3 つ渡してから関数値へ渡す形を、両方の edition でループの本体（`7:9`・`7:9`・`9:9`・`10:9`）の `E1017` と `help` で止めること、回避の書き方にした `dyn` の形と 2 つ渡す形を受理し、`dyn` の双子を `15:13` の `E1013` で拒否することを固定しました。
- 前のテスト `a_loop_whose_borrows_keep_getting_deeper_stops_with_e1017` の期待は変えず、説明を新しい数え方にしました。

##### マージの条件の確かめ（この回の後）

- ゲート（`gate/tools/check-r30.mjs`・`diff-r30.mjs`。15550 個・31100 組を、`r30`（`5f3c2a0b` の開発用のビルド、`gate/bin/tsuzuri-r30`）で両方の edition で検査し直し、ほかの列は `results-r29.json` から。結果は `results-r30.json`、分類は `diff-r30.txt`（`loopplace2/gate-diff-r30.txt` に写し））: **31100 組のどれも、受理・拒否と最初の 2 つの誤り（code・kind・位置）が `r29` と同じです。** `r28`（`15d84aaf`）と比べると、RA は前と同じ 67 組で新しいものは無く、受理から拒否も拒否から受理も無く、変わったのは `r28` の時間切れの 29 組（`r29` と同じ `E1017`）だけです。検査の失敗・時間切れは 0。edition 2027 は両方が受理 5272・両方が拒否 5259・RA 38・AR 4981、2026 は 4209・7190・29・4122 で、前回と同じです。
- 生成したプログラム（`loopplace2/g-check.mjs`、31104 組を `r30` のリリースビルドで）: 危険な 18048 組はどれも拒否し、安全な 13056 組はすべて受理しました。31104 組のどれも、結果が `r29` の `loopplace/g/g-results.json` と同じです。
- 探査（前回の 100 組、`loopplace2/probes.sh`・`probes-v2.txt`）: どれも最初の誤りまで `r29` と同じです（47 組が `E1017`）。レビューの探査（`loopplace-review-ce/p`・`loopplace-review-conf/p` と `q/`、82 個・164 組、`cmp.sh`・`review-v2.txt`）: `15d84aaf` が検査を終える組は、どれも最初の誤りまで `15d84aaf` と同じで、時間切れは 0 です。`a8623235` と違うのは、上の連なり（受理に戻った）と `dyn`・局所値に渡す形（`E1017` で止まるようになった）だけです。
- `examples/` と `tests/fixtures/` の 155 個のプロジェクト（`_tools/step5.mjs`）を `main` と `r30` で比べ、違いはありません（`loopplace2/step5-main.txt`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3299 件すべて成功、4 件 skip（1434 秒、`loopplace2/nextest.log`。新しいテスト 1 件を含みます）。
- E2E（`r30` の開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`loopplace2/suites.log`）。
- 文書: `check-docs.mjs`（`lifetimes.md`・`diagnostics.md`、2 ページ・83 個のリンク・7 個の例・14 回の native の実行）、`gen-docs.mjs --check`（19 個、古いもの 0）、`check-cli-docs.mjs`（4 個）、`check-guide-lite.mjs`（88 個）、`check-links.mjs --into`（変えた 7 つの文書への 467 個のリンク、壊れたもの 0）が成功。
- 時間（リリースビルド、キャッシュを温めた後の 3 回の中央値、`loopplace2/time-probes-r30.txt`。負荷 load average 17 ほど）: `E1017` で止まる形はどれも経過時間 0.16〜0.18 秒（`lag9` は 0.29 秒）、受理する連なりは 0.16〜0.30 秒、回避の書き方の形は 0.12 秒です。同じときに測った `r29` は `h8_simple_own` が 0.35 秒（この回は 1 回早く止まります）、`h21_fieldborrow` が 0.17 秒でした。
- 所有権の検査の時間（`loopplace2/perf.sh`、`r29` と `r30` の開発用のビルドを交互に 7 回ずつ、前回と同じ 12 個。`time-perf.txt`）: 揺れの範囲です。差が出た `perf_ho_chain_320`・`perf_match_nest_30`・`perf_ho_wrap_120` を 15 回ずつ測り直すと、44.7 と 44.3 ms、1.88 と 2.11 ms、3.71 と 4.21 ms で、後の 2 つを順番を逆にして（`r30` を先に）測ると 2.09 と 2.02 ms、2.64 と 2.71 ms でした（`time-perf-2.txt`）。差は測る順番によるものです。借用ごとに型を 1 つ記録し、どの値にも無い場所の借用が無い本体（ほとんどすべて）では、ループの数え方は何もしません。
- コミット: `5f3c2a0b`（`src/ownership.rs`・`src/ownership_control.rs` と `tests/rc.rs`）、`7aa2f63d`（文書）、この節。

#### 借用の数による打ち切り（2026-11-15）

借用の数の上限そのもの（`LOOP_LOAN_GROWTH`・`LOOP_LATE_GROWTH`・`LOOP_GROWTH_FROM`）は、下の「借用の数の上限を外す（2026-11-16）」で外しました。先に止めるのが収束するループだけだったためです。

この節の上限（2 回目からの反復が最初の反復より 500 個を超えて多く借用を作ると止める）は、下の「借用の増え方による打ち切り（2026-11-16）」で、借用の増え方の打ち切り（`LoanGrowth`）と、5 回目からの、前の反復より増えた反復に働く 500 個の上限に改めました。この節の上限のレビューで、一度だけ大きく増えて収束するループを止め、最初の反復がいちばん大きい形とゆっくり増え始める形を 10〜35 秒止めないことが分かったためです。

上の打ち切り（`5f3c2a0b`）のレビュー（第1回。反例の探索 `target/scratch/a31/lp-r1-ce/`、適合の確認 `lp-r1-conf/`）の指摘はどれも確かめられました。`773ea1c4` と `5f3c2a0b` は、どちらも借用した場所の形（型から外れる段、どの値にも無い場所、それを作った局所値へ戻ること）から収束しないループを推し量っていて、レビューのたびに、収束するループを止める反例と止まらない形の両方が見つかりました。場所の形を推し量るのをやめ、検査を遅くしているものそのもの、つまり 1 回の反復が作る借用の数で止めるようにしました。検査を緩める変更はしない、`15d84aaf` が実用的な時間で検査を終えるループの結果は上限を超えない限り変えない、という前の回からの規則のままです。結果は `target/scratch/a31/lp-budget/` にあります（`bin/` がこの回のビルドと計測用のビルド、`one.sh`・`probes.sh` が探査の検査、`agg.mjs`・`stops.mjs`・`chains.mjs`・`ratio.mjs` が記録の集計、`chain/` が参照の連なり、`bigbody/` が本体の大きな `loopplace` の形、`wa/` が回避の書き方の形、`tests/` が下のテストの形）。

| 指摘 | 判定と対応 |
| --- | --- |
| 収束する連なりを止めた（`lp-r1-conf/p/each9_store0_live` など、`lp-r1-ce/p/es_live_9`・`es_live_10`・`each_store_live`・`esl_*`） | 確かめました。ループの中の格納が連なりの場所を仮引数へ戻しても、ループが仮引数を参照外ししなければ場所は深くなり続けず、連なりの数の反復の後に収束します（`15d84aaf`・`main`・基準は受理し、`main` の値は 5）。新しい上限では、これらの反復が最初の反復より多く作る借用は多くても 132 個（12 個の `each_store_live`）で、どれも両方の edition で受理します |
| 止まらない: 本体の局所値を参照外しする形（`lp-r1-conf/p/e_copy2`・`e_field2`、`lp-r1-ce/p/bl_*`） | 確かめました（元の局所値が反復の終わりに無いので、数え始めませんでした）。新しい上限では、10 回目の反復（最初の反復が 3 個、10 回目が 514 個）で止まります |
| 止まらない: `if`・`match`・ブロック・ラムダの結果を参照外しする形（`lp-r1-ce/p/gl_*`・`ge_lamin_ref`） | 確かめました（根が局所値でないので、元を記録しませんでした）。新しい上限では、10 回目の反復（最初が 4 個、10 回目が 515 個）で止まります |
| 止まらない: `ref mut` の仮引数の再借用を選ぶ形（`lp-r1-ce/p/rm_*`。借用が反復ごとに 3 倍） | 確かめました。新しい上限では、6 回目の反復（最初が 7 個、6 回目が 975 個）で止まります |
| 文書（収束する反復の数の 1 つのずれ、「元へ戻すと収束しない」という誤った主張、`diagnostics.md`・`docs/language.md` の条件の書き落とし） | 確かめました。数え方ごと書き直しました（下） |

##### 変えたこと（`src/ownership.rs`・`src/ownership_control.rs`）

- `src/ownership.rs` は `15d84aaf` に戻しました（`loan_types`・`artifact_loans`・`artifact_sources`・`reach_cache`・`Checker::artifact_place`・`note_borrowed`・`reaches`・`inherit_marks` を消しました）。`src/ownership_control.rs` からも `artifact_depths`・`DEEPER_ITERATIONS`・`LOOP_DEEPER_HELP` を消し、`check_loop` に次の数え方だけを足しました。
- `check_loop` は、反復ごとに、条件と本体の検査（中のループと検査を含みます）の前後の `self.loans.len()` の差を、その反復が作った借用の数とします。収束しなかった反復（最初を除く）が、最初の反復より `LOOP_LOAN_GROWTH`（500）個を超えて多く借用を作ると、反復の上限と同じ `E1017`（`loop_ownership_limit`、ループの本体の位置、同じ文）を、次の `help` を付けて報告します: 「the loop's borrows keep growing: one iteration made N borrows, more than 500 beyond the M of the first iteration, so the check stops before the loop settles; when the loop passes a borrow of a reference parameter's target to a function value that owns a cell (a lambda that captures an `Arc` by value), let the function value capture a reference to the cell instead, such as `let cell = Arc.get (ref sink)`」。上限は長い参照の連なりのような `loopplace` でない形も止めるので、回避の書き方は条件を付けて言います。上限で止まった 104 個の探査（関数値を作るラムダがある形すべて）を回避の書き方にすると、どれも `E1017` にならず、192 組が受理、危険な双子 14 組と、edition 2027 の呼び出しの位置の `E1013`（関数値の実引数、2 組）が拒否でした（`wares/results.txt`）。
- 最初の反復を基準にした理由: 状態は反復ごとに入口の状態との合流で大きくなるだけなので、後の反復が作る借用は最初の反復より少なくなっても数個です（探査のループで最大 8 個、gate のループで 7 個少ない）。いちばん少ない反復を基準にしても、どのコーパスの最大も同じでした（`minbase.mjs`）。
- 最初の反復の何倍という上限にしなかった理由: レビューの連なりを余裕をもって受理する倍率（48 倍、下の (a)）では、`loopplace` の形の本体に借用を足すほど止まるのが遅れ、最初の反復が 63 個の形は 8.9 秒、203 個の形は 120 秒たっても止まりませんでした（`bigbody/`）。500 個を超えて多いという上限では、同じ形が 0.4〜0.7 秒で止まります。

##### 上限の決め方

(a) 収束するループが後の反復で多く作る借用（計測用のビルド `bin/tsuzuri-stat`・`tsuzuri-stat-rel`。`15d84aaf` の `src/` に、ループごとの反復の借用の数を `TZ_LOOPSTAT` のファイルへ書く計測を足し、`TZ_LOOPSTAT_NOSTOP` で止めないもの。計測は戻しました）:

| 実行 | 組・件 | 収束したループ（関数ごと、検査の回ごと） | 後の反復が最初の反復より多く作った借用の最大 | 収束までの反復の最大 |
| --- | --- | --- | --- | --- |
| ゲート（15550 個、両方の edition。`tests/` から取り出した 1721 個を含む。`gate/tools/stat-r31.mjs`、開発用のビルド） | 31100 組 | 8,816,803 | 3（`p29396`、`3,4,6`） | 3 |
| 生成したプログラム（`r16-review-2-ce/g`、両方の edition。`g-stat.mjs`、リリースビルド） | 31104 組 | 9,051,264 | 0 | 3 |
| `examples/` と `tests/fixtures/`（step5 の 155 個） | 155 個 | 49,579 | 2（`tests/fixtures/borrowed_records`、`10,12`） | 3 |
| std の例（`tsuzuri test --doc std`） | 873 個（すべて成功） | 263,523 | 0 | 3 |
| 時間を測る 12 個（両方の edition） | 24 組 | 7,070 | 0 | 3 |
| 前の回までの探査（414 個、両方の edition。連なりの探査 `es_live_*`・`each*_live`・`call_*`・`chain_*`・`refw`/`valw`/`arrw-*`・`generic-12`・`capturew-12`・`art_*` を含む） | 828 組 | 233,925 | 132（`each12` などの 12 個の連なり。参照がそれぞれ 2 つの場所から始まる） | 14（`conv2/c13`） |

収束しなかったループ（反復の上限に届いたもの）はどの実行にもありません。探査の 197 組は、この回のビルドでも止まる `loopplace` の形です（上限を 1000 にした計測でも同じ 197 組）。いちばん少ない反復を基準にした最大も、どの実行でも同じでした。

(b) 参照の連なりの閉じた形: それぞれが次の参照の先のフィールドを指す参照 L 個を 1 つのループで進める（`q0 = ref (deref q1).a`、…、`q(L-1) = ref (deref qL).a`）と、反復 k（0 から）は、参照 j ごとに参照 j+1 が反復の始まりに持つ場所をすべて借用します。参照がそれぞれ m 個の場所から始まるとき、参照 j+1 が持つ場所は m(1 + min(k, L−1−j)) 個なので、反復 k が最初の反復より多く作る借用は m·Σ_j min(k, L−1−j) = m(k(k−1)/2 + k(L−k)) 個で、最大は反復 L−1 の mL(L−1)/2 個、反復 L で収束します。計測と合います（m = 1 の `chain/c10`〜`c45` は 45・190・435・780・990 個、m = 2 の `each` の形は 12 個で 132 個）。上限 500 個に届くのは m = 1 で 33 個から（32 個は 496 個）、m = 2（参照の仮引数の先のフィールドから始まり、ループの前の格納がその仮引数へ借用を足した連なり）で 23 個から（22 個は 462 個）です。反復の上限（129 回）が 129 個以上の連なりを止めるのと同じ種類の上限です。上限は本体のすべての借用を数えるので、8 個の連なり（28 個）を 18 本進めるループ（504 個）も止まり、17 本（476 個）は止まりません。連なりの検査はもともと L の 7 乗ほどで延びます（負荷の低いときのリリースビルド、`chain/`）: 20 個は 0.8 秒、30 個は 8.8 秒、32 個は 13.5 秒（どれも受理で、上限が働かないので `15d84aaf` と同じ検査です）、33 個は `15d84aaf` で 16.4 秒かけて受理していたものを、この回のビルドが 4.4 秒で止めます。参照がそれぞれ 2 つの場所から始まる連なりは、22 個が 6.2 秒で受理、23 個は `15d84aaf` が 8.5 秒で受理していたものを 6.9 秒で止めます。8 個の連なりを 18 本進めるループも、`15d84aaf` の 1.8 秒の受理を 0.7 秒で止めます。この 2 つは、`15d84aaf` が 10 秒以内に検査を終える形の結果を変えますが、上限を超える形として文書に書いたもので、実際のプログラムが書く形ではありません。上限が働かない 32 個以下の連なりは、どれも `15d84aaf` と同じ結果です。

(c) `loopplace` の形が止まるまでの時間: この回のリリースビルド（`bin/tsuzuri-r31`）で、負荷の低いとき（load average 3〜5）に、上限で止まる探査の 197 組、`copilot2-review-ce/p/u_hid_*`（edition 2026 の 5 組。入れ子のループの `u_hid_nested` を含む）、本体に借用を足した `bigbody/k10`・`k30`・`k100`（最初の反復が 23・63・203 個。両方の edition の 6 組）の計 208 組を 1 回ずつ測りました（`time-r31.txt`。std を含むプログラム全体の `check` の経過時間）。どれもループの本体の `E1017` で、経過時間は 0.19〜0.66 秒（中央値 0.22 秒）です。いちばん長いのは局所値に 9 回渡す `lag9`（0.66 秒）で、`lag2`〜`lag6` は 0.29〜0.46 秒、倍になる形（`h*`・`d_*`・`e_copy2`・`e_field2`・`bl_*`・`gl_*`・`ge_lamin_ref` など）は 0.19〜0.3 秒、再借用を選ぶ形（`rm_*`）は 0.23〜0.28 秒、入れ子のループ（`u_hid_nested` の内側のループ、`v_nested3`）は 0.2〜0.3 秒、`k100` は 0.48 秒でした。受理する同じ大きさのプログラム（回避の書き方の形）は 0.16〜0.21 秒です。

(a) と (c) は、借用の数の差だけの上限では両立しませんでした。チケットの方針は (a) の最大の 8 倍以上の余裕（132 × 8 = 1056 個以上）でしたが、1056 個以上にすると、倍になる形は 11 回目の反復（2050 個）まで進んで 1.2 秒、局所値に 9 回渡す形（`lag9`）は 2.4 秒、`lag2` と再借用を選ぶ形は 1.1 秒かかりました（負荷 4 ほどのときのリリースビルド）。1 秒以内にするには 640 個ほどまでで、`lag9` の時間を見て 500 個にしました。余裕は、レビューの連なり（132 個）に対して 3.8 倍、それ以外のすべてのコーパス（多くても 3 個）に対して 160 倍以上です。倍率の上限なら両立しますが、上のとおり本体の大きな `loopplace` の形が止まらなくなります。

##### テスト

- `tests/rc.rs` の `a_loop_whose_borrows_keep_growing_stops_with_e1017`（前の `a_loop_whose_borrows_keep_getting_deeper_stops_with_e1017`）: 期待は変えていません（両方の edition の `7:9`・`7:9`・`9:9`・`12:9` の `E1017` と回避の書き方の `help`、edition 2026 の値の仮引数の `7:9`、回避の書き方の受理と双子の `19:13` の `E1013`、12 個の連なりの受理）。`own` の形の `help` が両方の edition で「one iteration made 514 borrows, more than 500 beyond the 3 of the first」を含むことを足し、説明を新しい数え方にしました（反復 k が 2^k + 2 個を作り、反復 9 の 514 個が最初の 3 個より 500 個を超えて多い最初の反復。連なりは反復 k が k(k−1)/2 + k(12−k) 個多く、最大 66 個）。
- `a_loop_stops_by_the_loans_of_its_iterations_not_by_the_shapes_of_its_places`（前の `a_loop_stops_only_when_places_that_no_value_has_keep_getting_deeper`）: 期待は変えていません（12 個の連なりの 4 つの形の受理、`dyn` の値のフィールドの形と双子、局所値に 2 つ・3 つ渡す形の `7:9`・`7:9`・`9:9`・`10:9` の `E1017`、回避の書き方の受理と `15:13` の `E1013`）。局所値に渡す形を作るクロージャを関数 `late_hand_on` にし、説明を新しい数え方にしました（`each` の連なりは参照がそれぞれ 2 つの場所から始まるので最大 132 個、局所値に 2 つ・3 つ渡す形は借用が反復ごとに約 1.6 倍・1.5 倍で、反復 12（612 個）・15（597 個）で止まる）。
- 新しい `a_loop_stops_when_an_iteration_makes_500_loans_more_than_the_first`: レビューの形を固定しました。受理（両方の edition）: `each9_store0_live`・`es_live_10`・`esl_local`（ループの中の格納が連なりの場所を仮引数へ戻す 9・10・9 個の連なり。最大 72・90・72 個多い。`15d84aaf`・`main`・基準は受理し、`main` の native の `-O0` の値はどれも 5）と、局所値から始まる 16 個の連なり（最大 120 個）。`E1017`（両方の edition、ループの本体、回避の書き方の `help`）: 本体の局所値を参照外しする形（`let r = p` と `let q = p`、`7:9`。反復 9 が 514 個、最初が 3 個）、`if` の結果を参照外しする形（`7:9`、515 個と 4 個）、再借用を選ぶ形（`7:9`、反復 5 が 975 個、最初が 7 個）、局所値に 4 つ渡す形（`late_hand_on(4)`、`11:9`、反復 18 が 659 個、最初が 4 個）。上限の位置: 8 個の連なりを 17 本進めるループ（最大 17 × 28 = 476 個多い）を受理し、18 本（反復 7 が 504 個多い）をループの本体の最初の式の位置の `E1017` で止め、`help` が「more than 500 beyond」を含むこと。1 本の連なりでも、局所値から始まる 33 個の連なり（反復 26 が 26 × 25 / 2 + 26 × 7 = 507 個多く、反復 25 は 500 個）を両方の edition でループの本体の最初の式の位置の `E1017` で止めることを固定しました（上限に届かない 32 個の連なりは、リリースビルドでも検査に 13 秒ほどかかるので、テストにしていません）。位置と数は手で数え、計測用のビルドの記録（`tests/`、`chain/m17x8`・`m18x8`・`c33`）と合わせました。
- 3 件はどれも成功しました（`cargo test --test rc`、`running 3 tests`、負荷の高いときに 26 秒。33 個の連なりを足した後の新しいテストだけで 27.8 秒）。

##### マージの条件の確かめ（この回の後）

- ゲート（`gate/tools/check-r31.mjs`・`diff-r31.mjs`。15550 個・31100 組を、`r31`（この回の開発用のビルド、`gate/bin/tsuzuri-r31`）で両方の edition で検査し直し、ほかの列は `results-r30.json` から。結果は `results-r31.json`、分類は `diff-r31.txt`（`lp-budget/gate-diff-r31.txt` に写し））: **31100 組のどれも、受理・拒否と最初の 2 つの誤り（code・kind・位置）が `r30` と同じです**（変わった組は 0 なので、前の打ち切りの `E1017` だけで拒否していた組の変化を一つずつ確かめる必要もありませんでした）。`r28`（`15d84aaf`）と比べると、RA は前と同じ 67 組で新しいものは無く、受理から拒否も拒否から受理も無く、変わったのは `r28` の時間切れの 29 組（`r29`・`r30` と同じ位置の `E1017`）だけです。検査の失敗・時間切れは 0。edition 2027 は両方が受理 5272・両方が拒否 5259・RA 38・AR 4981、2026 は 4209・7190・29・4122 で、前回と同じです。上の (a) の計測用のビルドの実行（`gate/stat-r31.json`、止めないもの）も、31100 組のどれも `r28` と同じでした。
- 探査（`probe-dirs.txt` の 414 個、両方の edition の 828 組。前の回までのすべての探査: `lp-r1-ce/p`・`lp-r1-conf/p`・`loopplace-review-ce/p`・`loopplace-review-conf/p`・`loopplace2/{p,q,tests}`・`copilot2/rv/{ph,a34,p}`・`copilot2-review-conf/ph`・`copilot2/p`・`loopplace/{conv,conv2,tests,wk}`。`pr31/results.txt`）: `r31` は 197 組を `E1017` で止め、時間切れは 0 です。止めなかった 631 組は、上限が一度も働かないので `15d84aaf` と同じ検査で、最初の誤りまで計測用のビルド（`15d84aaf` の `src/`）と同じでした。止めた 197 組は、どれも `15d84aaf` の `src/` のリリースビルドで止めずに 20 秒で打ち切っても終わらず、反復ごとの借用が倍・3 倍・1.2〜1.6 倍に増え続けていました（`ns20.txt`。前のレビューでも `15d84aaf` は 60〜120 秒で終わりませんでした）。どれも `loopplace` の形で、止める位置はループの本体です。前の打ち切り（`5f3c2a0b`）のレビューの探査（`loopplace-review-ce/p`・`loopplace-review-conf/p`・`loopplace2/q` の 162 組）は、最初の誤りまで `r30` と同じです（`loopplace2/review-v2.txt`）。
- 生成したプログラム（`g-stat.mjs`、15552 個・31104 組を、`15d84aaf` の `src/` に計測を足したリリースビルドで止めずに）: 危険な 18048 組はどれも拒否し、安全な 13056 組はすべて受理しました。31104 組のどれも、結果（受理か、最初の誤りの code と位置）が `r30` の `loopplace2/g/g-results.json` と同じです。どのループも後の反復で最初の反復より多く借用を作らない（最大 0 個）ので、`r31` の上限は一度も働かず、`r31` の結果もこれと同じです。
- `examples/` と `tests/fixtures/` の 155 個のプロジェクト（`_tools/step5.mjs`、`check --json` と `build --no-cache --emit llvm`）を `main`（`168ae4a5`）と `r31` で比べ、違いはありません（`lp-budget/step5-main.txt`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功（`lp-budget/clippy.log`）。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3300 件すべて成功、4 件 skip（1454 秒、`lp-budget/nextest.log`。新しいテスト 1 件を含みます）。その後、新しいテストに 33 個の連なりを足し、そのテストだけを実行し直して成功しました（`running 1 test`、27.8 秒）。
- E2E（この回の開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`lp-budget/suites.log`）。
- 文書: `check-docs.mjs`（`lifetimes.md`・`diagnostics.md`、2 ページ・83 個のリンク・7 個の例・14 回の native の実行）、`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 個、古いもの 0）、`check-cli-docs.mjs`（4 個）、`check-guide-lite.mjs`（88 個）、`check-links.mjs --into`（変えた 7 つの文書へのリンク、壊れたもの 0）が成功。
- 所有権の検査の時間（`lp-budget/perf.sh`。`TSUZURI_TIME_PASSES=1` の `ownership` の段の ms を、`r28`（`15d84aaf` の `src/`）・`r30`（`5f3c2a0b`）・`r31` の開発用のビルドで交互に 7 回ずつ測った中央値。前回と同じ 12 個。負荷の低いとき。`time-perf.txt`）: 揺れの範囲です。数え方は反復ごとに `self.loans.len()` を 2 回読むだけです。

| プログラム | `r28`（ms） | `r30`（ms） | `r31`（ms） |
| --- | --- | --- | --- |
| `perf_loop_breaks_400` | 363.401 | 363.647 | 365.539 |
| `perf_try_checked_400` | 376.360 | 377.133 | 377.821 |
| `perf_ho_wrap_120` | 2.592 | 2.674 | 2.595 |
| `perf_ho_chain_320` | 25.439 | 25.432 | 25.291 |
| `drop_locals_600` | 5.648 | 5.799 | 5.729 |
| `pattern_param_chain_200` | 6.696 | 6.669 | 6.796 |
| `perf_match_nest_30` | 1.491 | 1.517 | 1.497 |
| `p01_relay_chain_400` | 14.665 | 14.764 | 14.565 |
| `x01_many_holders_800` | 1189.964 | 1196.397 | 1194.678 |
| `x04_nested_loops_30` | 7.885 | 7.907 | 7.844 |
| `p_cellshares_400` | 229.191 | 230.439 | 228.516 |
| `p_cellshares_800` | 983.059 | 991.956 | 979.364 |

- 変えたファイル: `src/ownership.rs`（`15d84aaf` に戻した）、`src/ownership_control.rs`、`tests/rc.rs`（2 件の名前と説明、新しい 1 件）、`src/explain/codes/E1017.md`、`lifetimes.md`（「現在の制限」）、`compiler/diagnostics.md` と `docs/language.md`（`E1017` の行）、`docs/architecture.md`（ループの不動点の段落）、A34（`loopplace` の「現状」・原因・方針案、背景）、A33（「検査の時間」）、このチケット（上の 2 つの節の冒頭の注、「残る制限」と「修正のレビュー」の案内、この節）。
- コミット: `dbbe8338`（`src/` と `tests/rc.rs`）、`509760e6`（33 個の連なりのテストと doc comment の時間）、`43aae285`（文書とこの節）。

#### 借用の増え方による打ち切り（2026-11-16）

借用の数の上限そのもの（`LOOP_LOAN_GROWTH`・`LOOP_LATE_GROWTH`・`LOOP_GROWTH_FROM`）は、下の「借用の数の上限を外す（2026-11-16）」で外しました。先に止めるのが収束するループだけだったためです。

この節の 500 個の上限は、下の「最後のレビュー（2026-11-16）」で、4 回目の反復より 100 個（`LOOP_LATE_GROWTH`）を超えて多く借用を作った反復だけに働くようにしました。2 回目の反復で 500 個を超えて広がってから数個ずつ増えて収束するループ（参照の履歴や戻り先を局所値に持つインタプリタなど）を、5 回目の反復で止めていたためです。この節の止まるまでの時間（0.16〜1.61 秒）は測ったプログラムの範囲のもので、借用をほとんど作らない文の多い本体で局所値に何度も渡す形は、何秒もかかります（同じ節）。

上の「借用の数による打ち切り」（`dbbe8338`・`509760e6`・`43aae285`・`1b21f734`）のレビュー（第1回。反例の探索 `target/scratch/a31/lb-r1-ce/`、適合の確認 `lb-r1-conf/`）の指摘 8 件は、どれも確かめられました。1 回の反復の借用の数を最初の反復と比べるだけの上限は、代入し直す参照を通した借用の多いループが一度だけ大きく増えて収束するのを止め、最初の反復がいちばん大きい形と、ゆっくり増え始める形を 10〜35 秒止めませんでした。借用の数の大きさではなく、反復ごとの増え方の形で止めるようにしました。終わらないループの借用は、増え方が速くなり続けるか、前以上の増え方が続きます。収束するループの借用は、一度だけ増えるか、反復ごとに増え方が小さくなります（連なり）。規則は前の回と同じです（検査を緩める変更はしない、`15d84aaf` が 10 秒以内に検査を終えるループの結果は上限を超えない限り変えず、上限を超える形は実際のプログラムが書かない形として文書に書く、終わらない形は 1 秒ほどで止める）。結果は `target/scratch/a31/lb-fix1/` にあります（`instr/` が計測と規則の切り替えを足した `src/` の写し、`bin/` がそのビルド `tsuzuri-instr3` と候補の規則のビルド `tsuzuri-v1`〜`v4`（`v4` が `1506be7e`）、`sim.mjs` が記録から規則を試す道具、`trace.sh`・`knob.sh`・`cmp.sh`・`v.sh`・`one.sh`・`probes.sh` が探査の検査、`gen/` が参照を写していく局所値（`cc_*`）・局所値に渡して読む形（`win_*`）・遅れて写すインタプリタの形（`wl_*`）の探査、`tp/` が下のテストのプログラム、`tr/` が記録、`pstat/`・`pr32/` が探査の結果）。

| 指摘 | 判定と対応 |
| --- | --- |
| [結果の変化] 連なりの無いループが 500 個の上限を超える: 何か所も借用される参照が 2 回目の反復で P 個の場所に広がり、3 回目で収束する（`lb-r1-ce/p/int_o32_r8`・`ws_n8_l1_m64`・`n_d5_b8_u1`・`n_d3_b8_u16`・`n_d5_b2_u16`） | 確かめました。ループの中で代入し直す参照は最初の合流で場所が広がり、写す局所値（`last = acc`・`prev = cur`）は 1 回遅れて広がるので、それらを通した借用は 2 回目の反復で一度だけ（借用する箇所の数 × 場所の数だけ）増えます。増え方の打ち切りは一度の増加では止めず、500 個の上限は 5 回目の反復からなので、どれも両方の edition で受理します（`15d84aaf`・`main`・基準と同じ。`main` の値は 79・290・186・314・1274） |
| [結果の変化] 26 の腕のインタプリタのループ（`lb-r1-conf/p/interps_20_4`。`interp_83`・`interps_30_3`・`pipematch_42_2`・`pipe_45_4` も） | 確かめました。同じ仕組みです（`interps_20_4` は 176 個、712 個の後、3 回目で収束）。どれも両方の edition で受理します（`main` の値は `interps_20_4` で 31） |
| [終わらない] 中のループのせいで最初の反復がいちばん大きく、500 個を大きく超えてから止まる（`lb-r1-ce/p/hrs_1200` 10.7 秒、`hr_15_250` 32.9 秒、`hrn_2_1000` 21.4 秒） | 確かめました。外のループが倍にする状態は、中のループの 1 回の検査も長くします（`hrs_1200` の外のループの状態は反復ごとに倍、中のループの検査は 20 ミリ秒から反復ごとに約 2.3 倍）。外のループの借用は 2 回目から 2・4・8 個…と増え方が速くなるので、増え方の打ち切りが 6 回目の反復で止めます。500 個の上限も、最初の反復ではなく、それより前でいちばん少なかった反復から数えます（`hr_15_250` は、中のループが参照を 16 個の局所値に写していき、1 回の反復で 200 個を超える借用を作る、上限を超える形なので、それより先に中のループで止まります）。時間は下の表 |
| [終わらない] 局所値に多く渡す形と本体の大きな形（`lb-r1-ce/p/lag_14`〜`lag_140`、`lagf_9_1200`。`lag_140` は 500 個の上限に届かず 129 回の上限まで 25.7 秒） | 確かめました。局所値に k 回渡す形は、k 回ほどの反復で借用が 1 個ずつ増えてから速く増えます。1 個ずつでも前以上に増える反復が 24 回続けば止め（200 個を超える反復なら 8 回）、速く増え始めたら 5 回で止めます。時間は下の表 |
| [上限を超える] 選択から始まる連なりは、利用者向けの文書の 33 個より短く止まる（`if` で 23 個、4 つの腕の `match` で 17 個。`15d84aaf` は 17 個を 6.7 秒で受理） | 確かめました。連なりは増え方が反復ごとに小さくなるので、今も 500 個の上限だけが当たり、数は変わりません。`lifetimes.md` に、参照がそれぞれ 2 つの場所から始まる連なり（`if`）は 23 個以上、4 つの場所から始まる連なり（4 つの腕の `match`）は 17 個以上と書きました |
| [文書] 上限の `help` が、次の反復で収束し関数値も無いループに「増え続ける」と言い、関数値の回避の書き方を指す | 確かめました。そうしたループはもう止めません。`help` は止めた増え方（速くなり続ける、前以上に続く、それより前でいちばん少ない反復より 500 個多い）を言い、連なりなら連なりかループを分けるように言い、関数値の回避の書き方は条件付きで最後に言います |
| [終わらない] 局所値に多く渡す形、本体の大きな形が 12〜22 秒（`lb-r1-conf/p/lag_25`〜`lag_100`、`lagbig_15_400`・`lagbig_25_400`） | 確かめました（上の指摘と同じ仕組み）。時間は下の表 |
| [文書] 局所値に渡す形の局所値の数え方が 2 通り | 確かめました。`late_hand_on(k)` は `t0`〜`t(k-1)` の k 個の局所値に渡す形なので、A34 を「局所値が 2 つ（`t0`・`t1`）で約 1.6 倍、3 つで約 1.5 倍、9 つで約 1.2 倍」にし、テストの説明の「one or two locals」を「two or three locals」にしました |

##### 変えたこと（`src/ownership_control.rs`、`93f25d12`・`1506be7e`）

- `check_loop` は、収束しなかった反復ごとに、その反復が作った借用の数（条件と本体の検査の前後の `self.loans.len()` の差）を `LoanGrowth::note` へ渡します。`LoanGrowth` は、借用がいちばん少なかった反復の数、前の反復の数とその前からの増えた数、増え方が速くなった反復と前以上に増えた反復が何回続いたかを持ち、次のどれかに当たると、反復の上限と同じ `E1017`（`loop_ownership_limit`、ループの本体の位置、同じ文）を、止めた増え方を言う `help` を付けて報告します。
  - `LOOP_FASTER_ITERATIONS`（5）: 5 回続けて、前の反復より多く、しかも前の反復が増えた数より多く借用を作った（`help`: 「the loop's borrows keep growing faster: each of the last 5 iterations made more borrows than the one before it, and more of them than that one added (the last made N), so the check stops before the loop settles; …」）。
  - `LOOP_GROWING_ITERATIONS`（24）と `LOOP_GROWING_LARGE`（8 回と 200 個）: 24 回続けて、前の反復より多く、前の反復が増えた数以上に借用を作った。その反復が 200 個を超える借用を作ったなら 8 回続けて（`help`: 「the loop's borrows keep growing: each of the last 24 iterations made more borrows than the one before it, and at least as many more as that one added (the last made N), …; a reference that the loop passes along 25 or more locals, one per iteration (`u1 = u0`), and borrows through grows this way, and along 9 or more when its iterations make more than 200 borrows; …」）。
  - `LOOP_LOAN_GROWTH`（500）と `LOOP_GROWTH_FROM`（4）: 5 回目（反復 4）からの反復が、前の反復より多く、しかもそれより前でいちばん少なかった反復より 500 個を超えて多く借用を作った（`help`: 「one iteration of the loop made N borrows, more than the one before it and more than 500 beyond the M of the iteration before it that made the fewest, so the check stops before the loop settles; a chain of dozens of references, … advanced in one loop (`q0 = ref (deref q1).a`) grows this way: split the chain or the loop; …」）。前の回は最初の反復を基準にし、2 回目から数えていました。
  - どの `help` も、最後に回避の書き方（関数値にセルへの参照 `Arc.get (ref sink)` を捕捉させる）を条件付きで言います。指摘 5 の「増え続ける」は、止めた増え方の形を言う文にしました。
- 増え方の 2 つの打ち切りは、一度だけの増加（2 回目の反復）を数えません（1 回では続いたことにならず、5 回か 24 回（200 個を超える借用を作る反復なら 8 回）続く必要があります）。500 個の上限は 5 回目から、前の反復より増えた反復だけに働くので、代入し直す参照を写す局所値が多くて収束が遅れても（`prev3 = prev2`、…、`prev0 = cur`。`1506be7e`）、増えない反復では止めません。

##### 上限の決め方

(a) 何が時間を使うか（計測用のビルド `bin/tsuzuri-instr3`。`TZ_LB_TRACE=1` で反復ごとに作った借用の数・状態が持つ借用の数・評価した式の数・時間を書き、`TZ_LB_NOSTOP=1` で止めません）: 1 回の反復の時間は、本体で評価する式の数と、状態が持つ借用の数の積でほぼ決まります。`hrs_1200` の外のループの状態は反復ごとに倍（8、16、…、2048 個）で、中のループの 1 回の検査は 20 ミリ秒から 428 ミリ秒まで延びますが、外のループが作る借用は 1208 個から 1718 個までしか増えません。`lag_140` は反復ごとに借用が 1 個ずつしか増えないのに、1 回の反復が 3 ミリ秒から 260 ミリ秒まで延びます（140 個の写しの文）。借用の数の差だけの上限では、本体が大きいほど、またゆっくり増える形ほど止まるのが遅れます。

(b) 増え方の形（`sim.mjs` で記録から規則を試しました）: 終わらない形は、倍・3 倍の形が 2 回目の反復から増え方が速くなり続け（増えた数が 1・2・4・8…）、局所値に k 回渡す形は k 回ほど 1 個ずつ増えてから速くなり（`late_hand_on(9)` は 3、4、…、12、14、17、21、26、32）、中のループのある形は最初の反復の後に減ってから速くなります（2410、1208、1210、1214、1222…）。収束する形は、一度だけ増えるか（インタプリタ 176・712・712、`widening(8, 36, 4)` 97・673・673・673・673・673）、増え方が反復ごとに小さくなるか（連なり m(L−k)。8 個の連なり 10・17・23・28・32・35・37・38、局所値に渡してそれぞれを通して読む形 `win_25` 29・54・78・…・354）、参照を局所値に写していく形が 1 個ずつ増えます（`cc_25` 4、5、…、28。最後の局所値だけを通して読む形）。

(c) 探査の収束したループの増え方（`pstat/`: 前の回までの 414 個に `lb-r1-ce/p`・`lb-r1-conf/p` の 315 個を加えた 729 個の両方の edition を、この回の候補の規則で止めながら記録し、`pstat-sim.txt` に集計）: 1458 組の記録の、収束した 424,031 回のループの検査（std の前置きを含む）のどれも、増え方が続けて速くなった回数は多くても 1 回、前以上に増えた回数も、`lb-r1-ce/p/hr_*`（中のループで参照を 11〜41 個の局所値に写していく、レビューが作った形。収束したものは 10〜30 回）を除いて多くても 1 回です（`pstat-simall.txt`）。gate・生成したコーパス・examples と tests/fixtures・std の例・時間を測るプログラムは、前の回の計測（`lp-budget/`）で、どのループも 3 回以内の反復で収束し、後の反復が増えても 3 個なので、5 回続けて増えることも、5 回目の反復もありません。

(d) 数の選び方: 増え方が速くなる回数は、5 回で `hrs_1200` を 0.82 秒（6 回で 1.04 秒）、倍になる形を 6 回目の反復（34 個）で止めます。前以上に増える回数は、24 回で `lag_140` を 0.82 秒（32 回で 1.28 秒）にし、参照を写していく局所値は 25 個から止めます。32 回のままでは、本体に 1000 個の借用がある `lagf_40_1000` が 5.6 秒かかったので、200 個を超える借用を作る反復は 8 回にしました（0.96 秒。参照を写していく局所値はそうしたループで 9 個から）。8 回をすべてのループに使うと 9 個の局所値に写すだけのループも止めるので、本体の大きさで分けました。500 個の上限は前の回と同じ値にし（連なりの数 33・23・17 個と、8 個の連なりの 17 本・18 本は変わりません）、5 回目からにして、それより前でいちばん少なかった反復から数え、前の反復より増えた反復だけに働くようにしました。

これらの上限と、下の「最後のレビュー」の `LOOP_LATE_GROWTH` は、リポジトリの外の計測用のビルドと記録（`lp-budget/`・`lb-fix1/`・`lg-fix/`）で一度だけ決めたものです。後の変更が実際のプログラムのループの借用を長く・速く増やすようにしても、そうしたループを含むテストが無ければ気づけません。収束したループの借用の増え方の統計をテストだけが読めるようにし、`tests/` のプログラム・`examples/`・std の例で、その最大が上限の決まった割合（反復の数の上限の 1/2、借用の数の上限の 1/8）に近づいたら失敗するテストを足すことを、[A35](../A35-loop-growth-margin-check.md) に起票しました（2026-11-16）。

##### 時間

この回のリリースビルド（`bin/tsuzuri-v4`）と前の版（`1b21f734` のリリースビルド `lb-r1-ce/bin/tsuzuri-rel`）で、レビューが時間を指摘したプログラムとテストの止まる形を、edition 2027 で 1 回ずつ交互に検査しました（`timing.sh`、`time-v4.txt`。std を含むプログラム全体の `check` の経過時間。負荷 5〜6 のとき）。どれも前の版と同じ `E1017` で、止まるまでの時間は 0.16〜1.61 秒です（前の版は 0.21〜46.2 秒）。1 秒を超えるのは、本体が 1400 行ほどの `hrs_1400`（1.05 秒）と 2000 行ほどの `lagf_9_2000`（1.61 秒）だけです。`hr_15_250` だけは止まる位置が変わり、中のループ（`30:13`）が先に止まります。中のループは参照を 16 個の局所値に写していき、1 回の反復が 200 個を超える借用を作るので、上限を超える形（9 個以上）として、8 回続けて増えたところで止めます（`help` は「each of the last 8 iterations … (the last made 261)」）。前の版は外のループ（`26:9`）を 33.6 秒で止めていました。

| プログラム | この回（`1506be7e`） | 前の版（`1b21f734`） |
| --- | --- | --- |
| `lb-r1-ce/p/hrs_1200` | `E1017` 11:9、0.90 秒 | `E1017` 11:9、9.85 秒 |
| `lb-r1-ce/p/hrs_1400` | `E1017` 11:9、1.05 秒 | `E1017` 11:9、11.46 秒 |
| `lb-r1-ce/p/hr_15_250` | `E1017` 30:13、0.30 秒 | `E1017` 26:9、33.57 秒 |
| `lb-r1-ce/p/hrn_2_1000` | `E1017` 12:9、0.84 秒 | `E1017` 12:9、22.27 秒 |
| `lb-r1-ce/p/lag_14` | `E1017` 21:9、0.16 秒 | `E1017` 21:9、1.11 秒 |
| `lb-r1-ce/p/lag_20` | `E1017` 27:9、0.19 秒 | `E1017` 27:9、1.73 秒 |
| `lb-r1-ce/p/lag_40` | `E1017` 47:9、0.23 秒 | `E1017` 47:9、3.86 秒 |
| `lb-r1-ce/p/lag_80` | `E1017` 87:9、0.39 秒 | `E1017` 87:9、12.83 秒 |
| `lb-r1-ce/p/lag_100` | `E1017` 107:9、0.52 秒 | `E1017` 107:9、22.88 秒 |
| `lb-r1-ce/p/lag_120` | `E1017` 127:9、0.67 秒 | `E1017` 127:9、22.13 秒 |
| `lb-r1-ce/p/lag_140` | `E1017` 147:9、0.82 秒 | `E1017` 147:9、25.21 秒 |
| `lb-r1-ce/p/lagf_9_1200` | `E1017` 18:9、0.77 秒 | `E1017` 18:9、9.81 秒 |
| `lb-r1-ce/p/lagf_9_2000` | `E1017` 18:9、1.61 秒 | `E1017` 18:9、17.14 秒 |
| `lb-r1-ce/p/lagf_40_1000` | `E1017` 49:9、0.87 秒 | `E1017` 49:9、46.23 秒 |
| `lb-r1-conf/p/lag_9` | `E1017` 16:9、0.18 秒 | `E1017` 16:9、0.63 秒 |
| `lb-r1-conf/p/lag_25` | `E1017` 32:9、0.20 秒 | `E1017` 32:9、2.46 秒 |
| `lb-r1-conf/p/lag_60` | `E1017` 67:9、0.30 秒 | `E1017` 67:9、7.16 秒 |
| `lb-r1-conf/p/lag_100` | `E1017` 107:9、0.50 秒 | `E1017` 107:9、22.08 秒 |
| `lb-r1-conf/p/lagbig_15_400` | `E1017` 24:9、0.32 秒 | `E1017` 24:9、5.55 秒 |
| `lb-r1-conf/p/lagbig_25_400` | `E1017` 34:9、0.32 秒 | `E1017` 34:9、12.48 秒 |
| `lb-fix1/tp/own` | `E1017` 7:9、0.16 秒 | `E1017` 7:9、0.21 秒 |
| `lb-fix1/tp/late2` | `E1017` 9:9、0.16 秒 | `E1017` 9:9、0.28 秒 |
| `lb-fix1/tp/late9` | `E1017` 16:9、0.17 秒 | `E1017` 16:9、0.64 秒 |
| `lb-fix1/tp/reborrow` | `E1017` 7:9、0.23 秒 | `E1017` 7:9、0.23 秒 |

##### テスト

- 新しい `tests/rc.rs` の `a_loop_that_grows_once_settles_and_one_that_keeps_growing_stops`。受理（両方の edition）: 命令 20 個・サブルーチン 4 個のインタプリタ（`interpreter(20, 4)`。`lb-r1-conf/p/interps_20_4` と同じプログラムで、`main` は 31）、8 個のレコードのどれかを指す参照を 64 回借用して 1 つの局所値に写すループ（`widening(8, 32, 1)`。`lb-r1-ce/p/ws_n8_l1_m64` と同じ借用で、290）と 4 つの局所値に順に写すループ（`widening(8, 36, 4)`。97 個の後に 673 個が 4 回続き、反復 5 で収束。325）、参照を 24 個の局所値に写していくループ（`copy_chain(24, 0)`。18）、200 個の借用を足して 8 個の局所値に写していくループ（`copy_chain(8, 200)`。3618）。`E1017`（両方の edition、ループの本体の位置、`help` の文と回避の書き方）: 本体に中のループがある `loopplace` の形（中のループの借用 100 個。外のループの本体の `11:9`、「keep growing faster: each of the last 5 iterations」）、40 個の局所値に渡す形（`late_hand_on(40)`。`47:9`、「keep growing: each of the last 24 iterations」）、25 個の局所値に写していくループ（`33:9`、同じ文）、200 個の借用を足して 9 個の局所値に写していくループ（`17:9`、「each of the last 8 iterations … (the last made 212)」）。位置と値は手で数え、`main`（`168ae4a5`）の native の `-O0`・`-O3` の値（31・290・325・18・3618）と、前の版（`1b21f734` のリリースビルド）がインタプリタと 2 つの `widening` を `E1017` で止めることと合わせました。
- 前の 3 件（`a_loop_whose_borrows_keep_growing_stops_with_e1017`・`a_loop_stops_by_the_loans_of_its_iterations_not_by_the_shapes_of_its_places`・3 件目）: 期待の code・kind・位置は変えていません。`own` の形の `help` の確かめを、「one iteration made 514 borrows, more than 500 beyond the 3 of the first」から「keep growing faster: each of the last 5 iterations made more borrows than the one before it, and more of them than that one added (the last made 34)」に変えました（反復 k が 2^k + 2 個を作り、反復 5 が 5 回続けて速く増えた反復）。説明を新しい止め方にしました（局所値に 2 つ・3 つ渡す形は反復 7（36 個）・8（30 個）、本体の局所値・`if` の結果・再借用を選ぶ形は反復 5（34・35・975 個）、局所値に 4 つ渡す形は反復 9（28 個）。計測用のビルドの記録 `tr/` と合わせました）。局所値の数え方を直しました（指摘 8）。3 件目の名前を `a_loop_stops_when_an_iteration_makes_500_loans_more_than_the_first` から `a_loop_stops_when_a_later_iteration_makes_500_loans_more_than_an_earlier_one` に変えました（8 個の連なりを 17 本進めるループは受理、18 本と 33 個の連なりは 500 個の上限の `E1017` で、期待は同じです。連なりの借用は増え続けるので、前の反復より増えた反復だけを数えても同じ反復で止まります）。
- `tests/rc.rs` の `a_loop_` の 5 件（この 4 件と `a_loop_keeps_what_its_body_stored_elsewhere`）は、下の T1 でどれも成功しました（負荷の高いときに、新しいテストが 7.0 秒、3 件目が 54.7 秒）。

##### マージの条件の確かめ（この回の後）

- ゲート（`gate/tools/check-r32.mjs`・`diff-r32.mjs`。15550 個・31100 組を、`r32`（この回の開発用のビルド `1506be7e`。`lb-fix1/bin/tsuzuri-v4-dev` を `gate/bin/tsuzuri-r32` に写して使い、実行の後に写しを消しました）で両方の edition で検査し直し、ほかの列は `results-r31.json` から。結果は `results-r32.json`、分類は `diff-r32.txt`（`lb-fix1/gate-diff-r32.txt` に写し））: **31100 組のどれも、受理・拒否と最初の 2 つの誤り（code・kind・位置）が `r31` と同じです**（変わった組は 0 なので、前の上限の `E1017` だけで拒否していた組の変化を一つずつ確かめる必要もありませんでした）。`r28`（`15d84aaf`）と比べると、RA は前と同じ 67 組で新しいものは無く、受理から拒否も拒否から受理も無く、変わったのは `r28` の時間切れの 29 組（`r29`〜`r31` と同じ位置の `E1017`）だけです。この 29 組は、`15d84aaf` のビルド（`copilot2-review-ce/bin/tsuzuri-new`）で検査し直しても 120 秒で終わりません（`lb-fix1/gate-x/x15.txt`）。検査の失敗・時間切れは 0。edition 2027 は両方が受理 5272・両方が拒否 5259・RA 38・AR 4981、2026 は 4209・7190・29・4122 で、前回と同じです。
- 探査（`probe-dirs-all.txt` の 777 個、両方の edition の 1554 組。前の回までの 414 個に、`lb-r1-ce/p`・`lb-r1-conf/p` の 315 個と、この回の `gen/`・`tp/` を加えたもの。この回のリリースビルド、`pr32/results.txt`）: 時間切れは 0 です。前の回までの 828 組は、どれも最初の誤りまで `r31`（`lp-budget/pr31/results.txt`）と同じです。新しい 726 組のうち、`E1017` で止めた 267 組は、どれも `loopplace` の形（`lag*`・`lagf*`・`lagbig*`・`hr*`・`hrs*`・`hrn*`・`lv_*`、`tp/` の止まる形、`t_copy`・`t_choice`・`t_reborrow`）か、上限を超える収束する形（`c_match4_17`・`_22`・`_23`、`c_if_23`、`t_each_23`、`t_chains_1x33`・`18x8`・`2x23`・`3x19`、`gen/cc_25_0`・`cc_26_0`・`cc_33_0`・`cc_25_150`・`cc_9_190`・`cc_9_200`・`cc_9_300`、`gen/win_32`・`win_33`）です（この後に足した `gen/win_31` は、両方の edition で受理します。局所値が 31 個の窓は最初の反復より 496 個、32 個は 528 個多く作ります）。止めなかった 459 組では打ち切りが一度も働かないので、検査は `15d84aaf` と同じです。前の版（`1b21f734` のリリースビルド `lb-r1-ce/bin/tsuzuri-rel`、`prhead/results.txt`）でこの 230 個の両方の edition の 460 組（止めた `tp/value2026` の edition 2026 を含む）を検査し直すと、384 組は最初の誤りまで同じで、76 組（38 個の両方の edition）を `E1017` で止めていました。76 組はどれも一度だけ広がって収束する形で、この回が受理するものです（`lb-r1-ce/p` の `int_o32_r8`・`int_o128_r2`・`n_d*` 8 個・`ws_*` 8 個・`wsa_*` 8 個、`lb-r1-conf/p` の `interp_83`・`interp_84`・`interp_100`・`interps_20_4`・`interps_30_3`・`pipe_45_4`・`pipematch_42_2`・`pipematch_50_2`、`gen/wl_4_72`・`wl_5_72`、`tp/dump_interp`・`dump_widen`）。前の版の時間切れも 0 です。
- 生成したプログラム（`lb-fix1/g-check.mjs`、15552 個・31104 組を、この回のリリースビルドで）: 危険な 18048 組はどれも拒否し、安全な 13056 組はすべて受理しました。31104 組のどれも、結果（受理か、最初の誤りの code と位置）が `r30` の `loopplace2/g/g-results.json`（`r31` も同じ）と同じで、`E1017` は 1 組もありません（`lb-fix1/g/g-results.json`・`g-summary.json`）。
- この回のリリースビルド（`bin/tsuzuri-v4`）は `d0f469f0` から作り直したリリースビルドとバイト単位で同じで、開発用のビルド（`bin/tsuzuri-v4-dev`、ゲートの `r32`）も、コードとデータの部分が同じです（違いはリンカの記号表と署名だけ。`85189d19` と `d0f469f0` は `src/` ではコメントと `E1017.md` の文だけを変え、その文はこのビルドにもう入っていました。`lb-fix1/v5/`）。上の結果は、どれも `d0f469f0` の検査の結果です。
- `examples/` と `tests/fixtures/` の 155 個のプロジェクト（`_tools/step5.mjs`、`check --json` と `build --no-cache --emit llvm`）を `main`（`168ae4a5`）とこの回の開発用のビルドで比べ、違いはありません（`lb-fix1/step5-main.txt`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3301 件を実行し、3300 件が成功、4 件 skip（1519 秒、`lb-fix1/nextest.log`。上の 5 件を含みます）。残りの 1 件の `tests/formatter.rs` の `corpus_is_idempotent_and_semantically_unchanged` は、走査中のディレクトリが同時に走るほかのテストで消えた（`read_dir` の `NotFound`。前に `rr6/nextest.log` でも起きたもの）ために失敗し、単独で回すと成功しました（`running 1 test`）。
- E2E（この回の開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`lb-fix1/suites.log`）。
- 文書: `check-docs.mjs`（`lifetimes.md`・`diagnostics.md`、2 ページ・83 個のリンク・7 個の例・14 回の native の実行）、`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 個、古いもの 0）、`check-cli-docs.mjs`（4 個）、`check-guide-lite.mjs`（88 個）、`check-links.mjs --into`（変えた 8 つの文書へのリンク 467 個、壊れたもの 0）が成功。`f62058ac` の後に、`check-docs.mjs`（`lifetimes.md`、1 ページ・33 個のリンク・7 個の例・14 回の native の実行）、`gen-docs.mjs --check`（19 個、古いもの 0）、`check-cli-docs.mjs`（4 個）、`check-guide-lite.mjs`（88 個）をもう一度実行し、成功しました。
- 所有権の検査の時間（`lb-fix1/perf.sh`。`TSUZURI_TIME_PASSES=1` の `ownership` の段の ms を、`r28`（`15d84aaf` の `src/`）・`r31`（`dbbe8338`）・`r32`（この回）の開発用のビルドで交互に 7 回ずつ測った中央値。前回と同じ 12 個。負荷 5 ほどのとき。`lb-fix1/time-perf.txt`）: 揺れの範囲です（`r28` との差は多くても 1.8%。前回の `r28`・`r30`・`r31` の差も 1% ほどでした）。数え方は、反復ごとに `self.loans.len()` を 2 回読み、`LoanGrowth::note` で数をいくつか比べるだけです。

| プログラム | `r28`（ms） | `r31`（ms） | `r32`（ms） |
| --- | --- | --- | --- |
| `perf_loop_breaks_400` | 360.406 | 361.052 | 366.954 |
| `perf_try_checked_400` | 375.461 | 376.402 | 378.747 |
| `perf_ho_wrap_120` | 2.588 | 2.579 | 2.596 |
| `perf_ho_chain_320` | 25.324 | 25.203 | 25.527 |
| `drop_locals_600` | 5.647 | 5.763 | 5.748 |
| `pattern_param_chain_200` | 6.722 | 6.784 | 6.73 |
| `perf_match_nest_30` | 1.514 | 1.488 | 1.491 |
| `p01_relay_chain_400` | 14.851 | 14.734 | 14.908 |
| `x01_many_holders_800` | 1188.116 | 1192.014 | 1200.945 |
| `x04_nested_loops_30` | 7.957 | 7.939 | 7.962 |
| `p_cellshares_400` | 228.983 | 230.855 | 229.939 |
| `p_cellshares_800` | 985.462 | 983.351 | 986.812 |

- 変えたファイル: `src/ownership_control.rs`、`tests/rc.rs`（新しい 1 件、3 件の説明と `help` の確かめ、1 件の名前）、`src/explain/codes/E1017.md`、`lifetimes.md`（「現在の制限」）、`compiler/diagnostics.md` と `docs/language.md`（`E1017` の行）、`docs/architecture.md`（ループの不動点の段落）、A34（`loopplace` の「現状」・原因、背景、局所値の数え方）、A33（「検査の時間」）、このチケット（前の節への案内、この節）。
- コミット: `93f25d12`（`src/` と `tests/rc.rs`）、`1506be7e`（500 個の上限を増えた反復だけにする、4 つの局所値に写す形のテスト）、`85189d19`（文書）、`d0f469f0`（doc comment の直しとテストの説明の折り返し）、`f62058ac`（前の節の冒頭の注と、`lifetimes.md` の局所値に渡して読む形の 32 個）、この節。

#### 最後のレビュー（2026-11-16）

借用の数の上限そのもの（`LOOP_LOAN_GROWTH`・`LOOP_LATE_GROWTH`・`LOOP_GROWTH_FROM`）は、下の「借用の数の上限を外す（2026-11-16）」で外しました。先に止めるのが収束するループだけだったためです。

上の「借用の増え方による打ち切り」（`93f25d12`・`1506be7e`・`85189d19`・`d0f469f0`・`f62058ac`・`2a7d9e9f`）の最後のレビュー（反例の探索 `target/scratch/a31/lg-ce/`、適合の確認 `lg-conf/`）の指摘 6 件は、どれも確かめられました。最後の回なので打ち切りの規則は増やさず、500 個の上限に条件を 1 つ足して狭めただけです（`f88d8a82`）。結果は `target/scratch/a31/lg-fix/` にあります（`sim2.mjs`・`sim3.mjs` が記録から候補の規則を試す道具、`trall.sh` が計測用のビルド `lb-fix1/bin/tsuzuri-instr3` で止めずに記録する道具、`trs/` がレビューの探査の記録、`bin/tsuzuri-v5`・`v5-dev` がこの回のリリースビルドと開発用のビルド、`tp/` が下のテストのプログラム、`pr33/` が探査の結果）。

| 指摘 | 判定と対応 |
| --- | --- |
| [結果の変化] 代入し直す参照が 2 回目の反復で 500 個を超えて広がるインタプリタや状態機械のループが、5 個の参照の窓（`lg-ce/p/vm_trace5`）・4 回分の状態の履歴（`fsm_16_72_4`）・4 段の戻り先（`vmbt_20_4`）を持つと、5 回目の反復で `E1017` | 確かめました。500 個の上限はそれより前でいちばん少なかった反復、つまり広がる前の最初の反復から数えるので、2 回目の反復の広がり（548 個など）だけで上限を超えていて、後の反復が窓や履歴の分だけ 1〜19 個増えると、5 回目の反復で止めていました。500 個の上限に、4 回目の反復より `LOOP_LATE_GROWTH`（100）個を超えて多く作ることを足しました。指摘のループ（`vm_trace5`・`vm_trace6`・`vm_trace8`、`fsm_16_72_4`・`fsm_8_128_4`・`fsm_8_160_4`、`vmbt_*`、`vmr_*`）は、どれも両方の edition で受理します（`15d84aaf`・`main`・基準と同じ） |
| [終わらない] 局所値に渡す形で、本体の文が多く借用をほとんど作らないもの（`lg-ce/p/lagz_*`）は、24 回続けて増えるまで止まらず、リリースビルドで 10〜17 秒（`main` は 1 秒以内） | 確かめました。規則は増やさず、A34 の `loopplace` の「現状」と `lifetimes.md` の「現在の制限」に、止まるまで時間のかかる形として、形と時間を書きました（下の「時間」） |
| [文書] `lifetimes.md` と `LOOP_GROWING_LARGE` の doc comment が、借用の少ない大きな本体の止まるまでの時間を小さく書いている | 確かめました。`lifetimes.md`・`docs/architecture.md` に、1 回の反復の時間は反復が作る借用の数ではなく本体が評価する文の数と状態の借用の数で決まることと、借用をほとんど作らない文の多い本体の時間を書き、`LOOP_GROWING_ITERATIONS`・`LOOP_GROWING_LARGE` の doc comment も同じように直しました。500 個の上限の `help` は、連なりと、参照を局所値に渡してそれぞれを何度も通して読む形を言います |
| [結果の変化] 一度広がったループの 5 回目からの反復が 1 個でも増えると止める（`lg-conf/p/wl_4_72_read`・`interpw2_20_4_5`・`interpdh_20_4_4`） | 確かめました。上の指摘と同じ仕組みで、同じ変更で、どれも両方の edition で受理します（`main` の値は 328・271・101）。3 つは下のテストで受理を固定しました |
| [上限を超える] 2 つのフィールドのどちらかを指す 7 個の参照の連なり（`lg-conf/p/bc_if_7`）を、増え方が速くなる打ち切りが止める（`15d84aaf` は 0.79 秒で受理） | 確かめました。参照ごとに場所が倍になるので、増え方が反復ごとに大きくなります。`lifetimes.md`・`docs/architecture.md`・`LOOP_FASTER_ITERATIONS` と `LOOP_LOAN_GROWTH` の doc comment に、7 個以上で止まる形として書きました |
| [文書] 上限を超える形の一覧を、それで全部のように書いている（`docs/architecture.md`、A34、`lifetimes.md`、doc comment）。`bc_if_7` と、5 個の局所値に渡してそれぞれを 20 回読む `lg-conf/p/pipe_5_8_20`（500 個の上限で止まる。`15d84aaf` は 0.23 秒で受理）が反例 | 確かめました。一覧を「分かっている形」とし、`bc_if_7` の形と、局所値に渡してそれぞれを何度も読み、最初の局所値がいくつもの場所のどれかを指す形を足しました（`pipe_5_8_20` は 4 回目の反復より 180 個多く作るので、この回の後も止まります）。`E1017.md` の直し方にも足しました |

##### 変えたこと（`src/ownership_control.rs`、`f88d8a82`）

- `LOOP_LATE_GROWTH`（100）: `LoanGrowth` は 4 回目の反復（反復 3）が作った借用の数を覚え、500 個の上限は、5 回目からの反復が、前の反復より多く、それより前でいちばん少なかった反復より 500 個を超えて多く、しかも 4 回目の反復より 100 個を超えて多く作ったときだけ働きます。`help` は「one iteration of the loop made N borrows, more than the one before it, more than 500 beyond the M of the iteration before it that made the fewest, and more than 100 beyond the K of the fourth iteration, so the check stops before the loop settles; a chain of dozens of references, …, or a reference passed along locals and borrowed through each of them many times (`r1 = r0`), grows this way: split the chain or the loop; …」です。
- 増え方の 2 つの打ち切りと、500 個と 5 回目からという数は変えていません。

##### 上限の決め方

計測用のビルドの記録（`lb-fix1/pstat/` の 1459 個、`lb-fix1/tr/`、`lg-ce/trs/`、この回の `trs/`、合わせて 1746 個）で候補を試しました（`sim3.mjs`）。500 個の上限が最初に当たるのは、収束する形（連なり、局所値に渡して読む形、`fc4_5`、`pipe_5_8_20`）と指摘のループだけで、`loopplace` の形はどれも増え方の打ち切りが先に止めます。そのため、500 個の上限を狭めても `loopplace` の形の止まり方は変わりません。候補のうち、2 回目から 4 回目の反復の増え方が 500 個を超えたときだけ数え直す形は、増え方が 500 個の少し手前の広がりで同じ指摘が残るので、4 回目の反復からの増え方を別に数える形にしました。100 個は、指摘と探査の一度広がるループの 4 回目からの増え方（多くて 20 個。8 段の戻り先を持つ `vmbt_*_8`。1 回の反復では多くて 19 個）より大きく、上限が止める収束する形が止まるときの 4 回目からの増え方（8 個の連なりを 18 本進めるループの 180 個がいちばん小さく、33 個の連なりは 414 個）より小さい数です。記録のどれでも、100 個の条件は指摘のループのほかは止める反復を変えません（`pipe_5_8_20` も 4 回目より 180 個多い反復 4 で止まります）。

##### 時間

この回のリリースビルド（`bin/tsuzuri-v5`）と前の回のリリースビルド（`lb-fix1/bin/tsuzuri-v4`、`1506be7e`）で、前の回の「時間」の表の 24 個のプログラムを edition 2027 で 1 回ずつ交互に検査しました（`timing.sh`、`time-v5.txt`。負荷 4〜6 のとき）。どれも前の回と同じ位置の `E1017` で、止めた打ち切りも同じです（`help` から: 増え方が速くなる打ち切りが 10 個、24 回続けて増えた打ち切りが 8 個、8 回続けて増えた打ち切りが 6 個で、500 個の上限で止まるものはありません）。この回のビルドで 0.12〜1.54 秒です（いちばん長いのは本体が 2000 行ほどの `lagf_9_2000`、次が `hrs_1400` の 0.92 秒）。指摘のループは、この回のビルドで 0.16〜0.44 秒で受理します（`15d84aaf` は 0.19〜0.43 秒）。

止まるまで時間のかかる形（指摘 2。`lg-ce/p/lagz_*`、`lg-ce/gen/lagz.mjs <局所値の数> <借用を作らない文の数>`）は、この回のビルドと `main`（`168ae4a5`、開発用のビルド）で次のとおりです（`time-lagz.txt`。負荷 4〜6 のとき）。どれも前の回のビルドと同じ位置の `E1017` で（探査の `pr32lg/`）、`help` は 500 個の上限ではなく増え方の打ち切りを言います。`15d84aaf` は `lagz_60_2000` を 60 秒たっても終えません。

| プログラム | この回（`f88d8a82`） | `main` |
| --- | --- | --- |
| `lagz_300_0`（局所値 300 個） | `E1017` 308:9、2.70 秒 | 受理、0.32 秒 |
| `lagz_500_0` | `E1017` 508:9、7.01 秒 | 受理、0.38 秒 |
| `lagz_60_2000`（局所値 60 個と文 2000 個） | `E1017` 68:9、8.48 秒 | 受理、0.49 秒 |
| `lagz_100_1500` | `E1017` 108:9、8.42 秒 | 受理、0.45 秒 |
| `lagz_200_1000` | `E1017` 208:9、9.32 秒 | 受理、0.40 秒 |
| `lagz_23_3000` | `E1017` 31:9、10.86 秒 | 受理、0.68 秒 |
| `lagz_150_1500` | `E1017` 158:9、11.16 秒 | 受理、0.49 秒 |
| `lagz_60_3000` | `E1017` 68:9、14.81 秒 | 受理、0.79 秒 |
| `lagz_9_10000`（増え方が速くなる打ち切り） | `E1017` 17:9、41.31 秒 | 受理、4.44 秒 |
| `lagz_23_10000` | `E1017` 31:9、83.36 秒 | 受理、4.60 秒 |

##### テスト

- 新しい `tests/rc.rs` の `a_loop_whose_reference_widens_once_may_settle_a_few_iterations_later`。受理（両方の edition）: `widening(8, 36, 4)` の最後の写しをループの中で読む形（`s + String.length prev3.a`。98 個、675 個の後に 1 個ずつ増え、反復 5 で収束。`main` は 328）、`interpreter(20, 4)` が直前の 5 つの名前を局所値に持って読む形（新しい `keeping_interpreter(20, 4, 5, 0)`。`lg-conf/p/interpw2_20_4_5` と同じプログラムで、183・724・728・731・733 個、271）、4 段の戻り先を局所値に持つ形（`keeping_interpreter(20, 4, 0, 4)`。`interpdh_20_4_4` と同じで、180・716・720・724・728 個、101）。`E1017`（両方の edition）: 8 個のレコードのどれかを指す参照を 5 個の局所値に渡してそれぞれを 20 回読むループ（新しい `pipeline(5, 8, 20)`。`pipe_5_8_20` と同じで、`20:9`、`help` は「made 965 borrows, … more than 500 beyond the 125 …, and more than 100 beyond the 785 of the fourth iteration」）。受理の 3 つは `1506be7e` では `E1017` です。値は `main`（`168ae4a5`）の native の `-O0`・`-O3`、位置と数は計測用のビルドの記録と合わせました。
- `tests/rc.rs` の `a_loop_` の 6 件は、下の T1 でどれも成功しました。

##### マージの条件の確かめ（この回の後）

- ゲート（`gate/tools/check-r33.mjs`・`diff-r33.mjs`。15550 個・31100 組を、`r33`（この回の開発用のビルド。`lg-fix/bin/tsuzuri-v5-dev` を `gate/bin/tsuzuri-r33` に写して使い、実行の後に写しを消しました）で両方の edition で検査し直し、ほかの列は `results-r32.json` から。結果は `results-r33.json`、分類は `diff-r33.txt`（`lg-fix/gate-diff-r33.txt` に写し））: **31100 組のどれも、受理・拒否と最初の 2 つの誤り（code・kind・位置）が `r32` と同じです**。`r28`（`15d84aaf`）と比べると、RA は前と同じ 67 組で新しいものは無く、受理から拒否も拒否から受理も無く、変わったのは `r28` の時間切れの 29 組（前と同じ位置の `E1017`）だけです。検査の失敗・時間切れは 0。edition 2027 は両方が受理 5272・両方が拒否 5259・RA 38・AR 4981、2026 は 4209・7190・29・4122 で、前回と同じです。
- 探査（`lb-fix1/probe-dirs-all.txt` の 777 個と `lg-ce/p`・`lg-conf/p` の 96 個、両方の edition の 1746 組。この回のリリースビルド、時間切れは 120 秒、`pr33/results.txt`）: 前の回の 1554 組は、どれも最初の誤りまで前の回（`lb-fix1/pr32/results.txt`）と同じです。`lg-ce/p`・`lg-conf/p` の 192 組を前の回のビルドで検査した結果（`pr32lg/results.txt`）と比べると、148 組は同じで、変わった 44 組は指摘の 22 個の一度広がるループ（`fsm_16_72_4`・`fsm_8_128_4`・`fsm_8_160_4`、`vm_trace5`・`6`・`8`、`vmbt_20_*`・`30_*`・`60_*`（4・6・8 段）、`vmr_20_4`・`20_8`・`40_4`・`40_8`、`interpdh_20_4_4`、`interpw2_20_4_5`、`wl_4_72_read`）の両方の edition が `E1017` から受理になったものだけです。時間切れは `lagz_23_10000` の両方の edition だけで、前の回のビルドも同じく時間切れです（負荷 30 ほどのとき。負荷 4 ほどでは 83 秒で止まります）。`bc_if_7`〜`10`、`fc4_5`、`pipe_5_8_20`、`lagz_*`、`lagdip_30_2`、`lp_copyp`、`t_inner` は前の回と同じ `E1017` です。
- 生成したプログラム（`lg-fix/g-check.mjs`、15552 個・31104 組を、この回のリリースビルドで）: 危険な 18048 組はどれも拒否し、安全な 13056 組はすべて受理しました。31104 組のどれも、結果が `r30` の `loopplace2/g/g-results.json` と同じで、`E1017` は 1 組もありません（`lg-fix/g/g-results.json`・`g-summary.json`）。
- `examples/` と `tests/fixtures/` の 155 個のプロジェクト（`_tools/step5.mjs`）を `main`（`168ae4a5`）とこの回の開発用のビルドで比べ、違いはありません（`lg-fix/step5-main.txt`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3302 件を実行し、3301 件が成功、4 件 skip（1113 秒、負荷 25〜35 のとき。`lg-fix/nextest.log`。上の 6 件を含みます）。残りの 1 件の `tests/explain.rs` の `every_code_has_a_page` は、`E1017.md` の直し方に足した文でページが 1500 文字の上限を超えた（1572 文字）ためで、文を 1 つの言い方（「passes a reference along many locals」。写していく連なりと、渡してそれぞれを読む形の両方）にまとめて 1494 文字にし（`e592e6a8`）、`tests/explain.rs` の 16 件を回し直して成功しました。この直しの後のビルドは、上のゲート・探査・生成したプログラム・step5・E2E の検査に使ったビルド（`f88d8a82`）と `E1017` の説明のページの文だけが違います。
- E2E（この回の開発用のビルド `bin/tsuzuri-v5-dev`、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`lg-fix/suites.log`）。
- 文書: `check-docs.mjs`（`lifetimes.md`・`diagnostics.md`、2 ページ・83 個のリンク・7 個の例・14 回の native の実行）、`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 個、古いもの 0。`e592e6a8` の後にもう一度）、`check-cli-docs.mjs`（4 個）、`check-guide-lite.mjs`（88 個）、`check-links.mjs --into`（変えた 7 つの文書へのリンク 467 個、壊れたもの 0）が成功。
- 変えたファイル: `src/ownership_control.rs`（`LOOP_LATE_GROWTH`、500 個の上限の条件と `help`、打ち切りの doc comment）、`src/explain/codes/E1017.md`、`tests/rc.rs`（新しい 1 件と `keeping_interpreter`・`pipeline`）、`lifetimes.md`（「現在の制限」）、`compiler/diagnostics.md` と `docs/language.md`（`E1017` の行）、`docs/architecture.md`（ループの不動点の段落）、A33（「検査の時間」）、A34（`loopplace` の「現状」の止まるまで時間のかかる形と原因）、このチケット（前の節の冒頭の注、この節）。
- コミット: `f88d8a82`（`src/` と `tests/rc.rs`）、`7cc8e272`（文書と doc comment の数）、`e592e6a8`（`E1017.md` の長さ）、この節。

#### 借用の数の上限を外す（2026-11-16）

上の「最後のレビュー」（`f88d8a82`・`7cc8e272`・`e592e6a8`・`c2085c7d`）の後の点検（`target/scratch/a31/lg-recheck/`）で、500 個の上限が、まだ収束するループを止めることが分かりました。状態の履歴を持つ状態機械（`lg-recheck/p/fsmh_16_72_6_2` は 16 個の状態のうち直前の 6 つを局所値に持ち、それぞれの 2 つのフィールドを読むもの）と、32 個の状態の状態機械（`fsm_32_40_6`）など 15 個のディレクトリの両方の edition で、`15d84aaf`・`main`・基準は受理し、`f88d8a82` 以降は `E1017` です。代入し直す参照が 2 回目の反復で一度広がり（105 個から 693 個）、その後の反復が履歴の分だけ 34〜42 個ずつ増えて、4 回目の反復より 100 個多くなった反復で上限に当たっていました。記録で確かめると、借用の数の上限は収束するループしか先に止めていなかったので、条件を足して狭めるのをやめ、上限そのものを外しました。打ち切りは、増え方の 2 つ（5 回続けて速くなる、24 回（200 個を超える反復なら 8 回）続けて前以上に増える）だけです。結果は `target/scratch/a31/nobudget/` にあります（`sim4.mjs` が記録で今の規則と上限を外した規則を比べる道具、`classify.sh` が最初の誤りと `help` から止めた打ち切りを出す道具、`trs-recheck/`・`trs-chains/`・`trs-win/` がこの回に止めずに取った記録、`bin/tsuzuri-nb`・`tsuzuri-nb-dev` がこの回のリリースビルドと開発用のビルド、`pr34/` が探査の結果）。

##### 外した理由

- 計測用のビルド（`lb-fix1/bin/tsuzuri-instr3`。`15d84aaf` の検査に、止めずに反復ごとの借用の数を書く計測を足したもの）の記録 1887 個（`lb-fix1/pstat/` の 1458 個、`lb-fix1/tr/` の 59 個、`lg-ce/trs/` の 56 個、`lg-fix/trs/` の 172 個、`lg-fix/trs-tp/` の 8 個と、この回に `lg-recheck/p` の 67 個のディレクトリを両方の edition で取った 134 個（反復の上限は 129 回））で、今の規則と上限を外した規則が最初に止める所を比べました（`sim4.mjs`、`sim4.txt`）。増え方が速くなる打ち切りが先に止める 356 個と、前以上の増え方が続く打ち切りが先に止める 82 個は、どちらの規則でも同じ反復の同じ数で止まります。どちらも止めない 1394 個も同じです。違うのは、今の規則で 500 個の上限が先に止める 55 個だけで、上限を外すとどれも収束します（記録の終わりまでにすべてのループが収束し、検査は成功で終わります）。
- `lb-fix1/pstat/` の連なりの記録は、上限で止まった所で終わっていたので、連なりの 9 個のディレクトリ（`lb-r1-ce/p/c_if_23`・`c_match4_17`・`22`・`23`、`lb-r1-conf/p/t_chains_18x8`・`1x33`・`2x23`・`3x19`・`t_each_23`）と、局所値に渡してそれぞれを読む `lb-fix1/gen/win_32`・`win_33` を、両方の edition で止めずに記録し直しました（`trs-chains/`・`trs-win/`）。22 個のどれも収束します。
- 突き合わせ: 前の回のリリースビルドの探査で `E1017` か時間切れだった 502 組と `lg-recheck/p` の 134 組を、前の回のリリースビルドで検査し直して止めた打ち切りを `help` から分けると（`classify-v6.txt`）、増え方が速くなる打ち切り 368 組、24 回続けて増えた打ち切り 74 組、8 回続けて増えた打ち切り 34 組、500 個の上限 56 組、受理 104 組です。500 個の上限の 56 組は、`win_32`・`win_33`、`c_if_23`、`c_match4_17`・`22`・`23`、`t_chains_18x8`・`1x33`・`2x23`・`3x19`、`t_each_23`、`fc4_5`、`pipe_5_8_20`、`lg-recheck/p` の 15 個（`fsm_32_40_6` と `fsmh_*`）の両方の edition で、どれも上の記録で収束します。収束しない形で 500 個の上限に頼るものはありません。
- 代わりに、収束する長い連なりは、`15d84aaf` と同じく収束するまで検査します（下の「時間」）。それぞれの参照が 2 つのフィールドのどちらかを指す連なり（7 個以上）と、参照を写していく 25 個以上の局所値（200 個を超える反復なら 9 個以上）は、前の回と同じく増え方の打ち切りが止めます。
- 増え方が反復ごとに大きくなったり小さくなったりしながら収束しないループは、どちらの打ち切りにも当たらず、反復の上限（129 回）まで検査します。記録にはそうした形はありません（`docs/architecture.md` に書きました）。

##### 変えたこと（`src/ownership_control.rs`）

- `LOOP_LOAN_GROWTH`・`LOOP_LATE_GROWTH`・`LOOP_GROWTH_FROM`、`LoanGrowth` の `fewest`・`before_late`、500 個の上限の `help`、`LoanGrowth::note` の反復の番号の引数を外しました。`LoanGrowth` は前の反復の数と増えた数（`last`）と 2 つの連続（`faster`・`growing`）だけを持ちます。増え方の 2 つの打ち切りの条件・数・`help` は変えていません。
- 打ち切りの説明の doc comment（前は `LOOP_LOAN_GROWTH` に付いていました）を `LoanGrowth` に移し、連なりと受け渡しはどちらの打ち切りにも当たらず `15d84aaf` の結果のままであることと、その検査の時間、借用の数の上限を外した理由を書きました。`LOOP_FASTER_ITERATIONS` の doc comment に、4 つのフィールドのどれかを指す 5 個の連なり（`fc4_5`）が 4 回続けて速くなってから収束することを足しました。
- `src/explain/codes/E1017.md` の直し方は、写していく局所値と、いくつものフィールドのどれかを指す参照の連なりを言います（1497 文字）。

##### テスト

- `tests/rc.rs` の `a_loop_stops_when_a_later_iteration_makes_500_loans_more_than_an_earlier_one` を `a_loop_whose_loans_grow_by_less_in_each_iteration_settles_however_many_it_makes` に改名し、8 個の連なりの 17 本（受理）と 18 本・33 個の連なり（`E1017`）の代わりに、2 つのレコードのどちらかを指す参照を 16 個の局所値に渡してそれぞれを 4 回読むループ（`pipeline(16, 2, 4)`。71・139・203・…・659・671 個と反復ごとに 4 個ずつ少なく増え、最初より 600 個多く作って反復 16 で収束。前の回は反復 11 で止めていました。`main` は 600）の受理を両方の edition で確かめます。18 本の連なりは開発用のビルドで 1 回の検査に 2.4 秒ほど、33 個の連なりは 26〜54 秒かかるので、テストには入れません（どちらも探査で受理を確かめました）。`loopplace` の形の 5 つの打ち切り（`COPY`・`local`・`choice`・`reborrow`・`late_hand_on(4)`）はそのままです。
- `a_loop_whose_reference_widens_once_may_settle_a_few_iterations_later` は、`pipeline(5, 8, 20)` の `E1017` を受理に変え（`main` は 0）、新しい `state_machine(16, 72, 6, 2)`（`lg-recheck/p/fsmh_16_72_6_2` と同じプログラム。105・693・735・775・813・849・883 個の後、反復 7 で収束。`main` は 9288）の受理を両方の edition で足しました。前の回のビルドは 2 つとも `E1017` です。
- ほかの 2 件（`a_loop_stops_by_the_loans_of_its_iterations_not_by_the_shapes_of_its_places`・`a_loop_that_grows_once_settles_and_one_that_keeps_growing_stops`）は doc comment だけを直し、増え方の打ち切りの境界（写していく 24 個と 25 個の局所値、200 個の借用を足した 8 個と 9 個、中のループのある形、局所値 40 個に渡す形）はそのままです。
- `a_loop_` の 6 件は開発用のビルドで成功します（`rc-loop.log`。改名した 1 件は 6.4 秒で、前の回の 33 個の連なりを含む同じテストは T1 で 54.7 秒でした）。

##### 時間

連なりの検査の時間（リリースビルド、edition 2027、1 回ずつ、負荷 2〜5 のとき。`chaintime.txt`・`chainrel.txt`）。この回のビルドは、`15d84aaf` の検査を止めずに走らせるリリースビルド（計測用のビルド `lb-fix1/bin/tsuzuri-instr3` に `TZ_LB_NOSTOP=1`）と同じ時間で受理します。前の回のビルドは、どれも 500 個の上限で `E1017` でした（33 個以上の連なり、18 本の連なりなど）。

| プログラム | この回 | 止めない `15d84aaf`（リリース） |
| --- | --- | --- |
| 20 個の参照の連なり（`lp-budget/chain/c20`） | 受理、1.51 秒 | 1.51 秒 |
| 30 個（`c30`） | 受理、17.54 秒 | 17.64 秒 |
| 33 個（`c33`） | 受理、32.78 秒 | 32.46 秒 |
| 40 個（`c40`） | 受理、117.29 秒 | 117.22 秒 |
| 8 個の連なり 18 本（`m18x8`） | 受理、2.79 秒 | 2.84 秒 |
| 2 つの場所から始まる 23 個の `each` の連なり（`lb-r1-conf/p/t_each_23`） | 受理、16.41 秒 | 17.46 秒 |
| 4 つの腕の `match` から始まる 17 個（`lb-r1-ce/p/c_match4_17`）・23 個（`c_match4_23`） | 受理、6.43 秒・45.91 秒 | – |
| 5 個の局所値を 20 回ずつ読む窓（`lg-conf/p/pipe_5_8_20`）、履歴を持つ状態機械（`lg-recheck/p/fsmh_16_72_6_2`） | 受理、0.32 秒・0.26 秒 | – |

`15d84aaf` の参照に使ってきた `copilot2-review-ce/bin/tsuzuri-new` は開発用のビルドで、同じ連なりに 1.6 倍ほどかかります（20 個 2.39 秒、30 個 27.64 秒、33 個 51.27 秒、40 個 182.23 秒）。

前の回の「時間」の表の 24 個のプログラムを、この回と前の回のリリースビルドで edition 2027 で 1 回ずつ交互に検査しました（`timing.sh`、`time-nb.txt`。負荷 3 ほどのとき）。どれも同じ位置の `E1017` で、止めた打ち切りと `help` の借用の数も同じです（増え方が速くなる打ち切り 10 個、24 回続けて増えた打ち切り 8 個、8 回続けて増えた打ち切り 6 個）。この回のビルドで 0.11〜1.54 秒です（いちばん長いのは `lagf_9_2000`、次が `hrs_1400` の 0.93 秒）。

止まるまで時間のかかる形（`lg-ce/p/lagz_*`）は前の回と同じ打ち切りで止まり、時間も変わりません（`time-lagz.txt`。edition 2027、負荷 3 ほどのとき。括弧は `main` の開発用のビルド）: `lagz_300_0` 2.67 秒（0.24 秒）、`lagz_500_0` 6.97 秒（0.40 秒）、`lagz_60_2000` 8.54 秒（0.49 秒）、`lagz_100_1500` 8.47 秒（0.49 秒）、`lagz_200_1000` 9.45 秒（0.39 秒）、`lagz_23_3000` 10.92 秒（0.68 秒）、`lagz_150_1500` 11.32 秒（0.49 秒）、`lagz_60_3000` 14.88 秒（0.80 秒）、`lagz_9_10000`（増え方が速くなる打ち切り）41.37 秒（4.47 秒）、`lagz_23_10000` 83.47 秒（4.65 秒。edition 2026 も 83.44 秒）。いちばん遅く止まる `loopplace` の形は、今も `lagz_23_10000` です。

所有権の検査の時間（`perf.sh`。`TSUZURI_TIME_PASSES=1` の `ownership` の段の ms を、`r28`（`15d84aaf` の `src/`）・前の回・この回の開発用のビルドで、12 個のプログラムを 7 回ずつ交互に測った中央値。`time-perf.txt`、負荷 3〜5 のとき）: この回と前の回の差は 0.5% 以内で、向きもまちまちです（`perf_loop_breaks_400` 369.4 / 367.8、`x01_many_holders_800` 1216.0 / 1212.1、`p_cellshares_800` 976.0 / 978.3）。

##### マージの条件の確かめ（この回の後）

- ゲート（`gate/tools/check-r34.mjs`・`diff-r34.mjs`。15550 個・31100 組を、`r34`（この回の開発用のビルド。`bin/tsuzuri-nb-dev` を `gate/bin/tsuzuri-r34` に写して使い、実行の後に写しを消しました）で両方の edition で検査し直し、ほかの列は `results-r33.json` から。結果は `results-r34.json`、分類は `diff-r34.txt`（`nobudget/gate-diff-r34.txt` に写し））: **31100 組のどれも、受理・拒否と最初の 2 つの誤り（code・kind・位置）が `r33` と同じです**（ゲートには 500 個の上限で止まるループがありませんでした）。`r28`（`15d84aaf`）と比べると、RA は前と同じ 67 組で新しいものは無く、受理から拒否も拒否から受理も無く、変わったのは `r28` の時間切れの 29 組（前と同じ位置の `E1017`）だけです。検査の失敗・時間切れは 0。edition 2027 は両方が受理 5272・両方が拒否 5259・RA 38・AR 4981、2026 は 4209・7190・29・4122 で、前回と同じです。
- 探査（`lg-fix/probe-dirs-r33.txt` の 873 個と `lg-recheck/p` の 67 個、両方の edition の 1880 組。この回のリリースビルド、時間切れは 120 秒、4 つずつ並べて。`pr34/results.txt`、比べる道具 `cmp-probes.mjs`、`pr34/cmp.txt`）: 1822 組は、最初の誤りと止めた打ち切りまで前の回のリリースビルド（`lg-recheck/v6.txt`・`classify-v6.txt`）と同じです。変わった 58 組は、500 個の上限で止めていた 56 組（上の「外した理由」の一覧）が受理になったものと、`lagz_23_10000` の両方の edition が並べて走らせた負荷で 120 秒を超えたものです（1 つだけで走らせると 83.4 秒と 83.5 秒で、前と同じ `31:9` の `E1017` で止まります。前の回の探査 `pr33/` でも負荷の高いときは時間切れでした）。受理になった 56 組は、`15d84aaf`（参照の開発用のビルド、120 秒）もどれも受理します（`pr34/old-changed.txt`。いちばん長いのは `c_match4_23` の 39.7 秒）。打ち切りで止まる組は、増え方が速くなる打ち切り 368 組、24 回続けて増えた打ち切り 74 組（`lagz_23_10000` を含む）、8 回続けて増えた打ち切り 34 組で、どれも前の回と同じ位置と打ち切りです。`loopplace` の形はどれも止まり、`help` の無い `E1017`（反復の上限）は 1 組もありません。打ち切りで止まる組のうち `15d84aaf` が受理するのは、前から書いてある増え方の打ち切りの形（2 つのフィールドのどちらかを指す 7 個以上の連なり `bc_if_7`〜`10`、写していく 25 個以上の局所値や 200 個を超える反復で 9 個以上の局所値 `cc_*`・`hr_*`）だけで、この回で変わったものはありません。
- 生成したプログラム（`nobudget/g-check.mjs`、15552 個・31104 組を、この回のリリースビルドで）: 危険な 18048 組はどれも拒否し、安全な 13056 組はすべて受理しました。31104 組のどれも、結果が `r30` の `loopplace2/g/g-results.json` と同じで、`E1017` も検査の失敗も 1 組もありません（`nobudget/g/g-results.json`・`g-summary.json`）。
- `examples/` と `tests/fixtures/` の 155 個のプロジェクト（`_tools/step5.mjs`）を `main`（`168ae4a5`）とこの回の開発用のビルドで比べ、違いはありません（`step5-main.txt`）。

##### 確かめたこと

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`: 成功。
- T1（`node scripts/slot.mjs -- cargo nextest run --locked --no-fail-fast`）: 3302 件を実行し、3301 件が成功、4 件 skip（1594 秒、生成したプログラムの検査と同時に走らせた負荷 30〜50 のとき。`nextest.log`。上の 6 件を含み、改名した 1 件は 12.1 秒）。残りの 1 件の `tests/diagram_cli.rs` の `kg_diagram_argument_errors` は、一時ディレクトリの KG の置き場所が「KG ではない」と言われたもので（ほかのテストと同時に走ったときの競合）、1 つだけで回し直すと `tests/diagram_cli.rs` の 12 件が成功しました（`diagram-rerun.log`）。この回の変更は所有権の検査の打ち切りだけで、図の CLI は通りません。
- E2E（この回の開発用のビルド、native と WASM の `-O0`・`-O3`）: `in_place_writes` 85 件、`conventions` 39 件、`mutable_slices` 49 件、`rc` 39 件、`borrowed_records` 57 件、`control` 701 件がすべて成功（`suites.log`）。
- 文書: `check-docs.mjs`（`lifetimes.md`・`diagnostics.md`、2 ページ・83 個のリンク・7 個の例・14 回の native の実行）、`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 個、古いもの 0）、`check-cli-docs.mjs`（4 個）、`check-guide-lite.mjs`（88 個）、`check-links.mjs --into`（変えた 8 つの文書へのリンク 470 個、壊れたもの 0）、`tests/explain.rs`（16 件。`E1017.md` は 1497 文字）が成功。
- 変えたファイル: `src/ownership_control.rs`（`LoanGrowth` から借用の数の上限を外し、打ち切りの doc comment を `LoanGrowth` へ）、`src/explain/codes/E1017.md`、`tests/rc.rs`（1 件の改名と、`pipeline(16, 2, 4)`・`pipeline(5, 8, 20)`・新しい `state_machine(16, 72, 6, 2)` の受理）、`lifetimes.md`（「現在の制限」）、`compiler/diagnostics.md` と `docs/language.md`（`E1017` の行）、`docs/architecture.md`（ループの不動点の段落）、A33、A34（`loopplace` の原因と、双子の止まり方の古い記述）、A35（借用の数の上限の統計 `beyond_fewest`・`beyond_fourth` を外し、定数を 3 つに、行番号と探査の今の値を直しました）、このチケット（前の 3 つの節の冒頭の注、この節）。
- コミット: `73a3402b`（`src/` と `tests/rc.rs`）、`2a85ef4b`（文書）、`b0ec89ab`（この節。A35 の行番号の直しを含みます）、`15dd4be0`（`_features/README.md` の A35 の行）。
