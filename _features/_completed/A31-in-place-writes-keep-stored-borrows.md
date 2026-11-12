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
