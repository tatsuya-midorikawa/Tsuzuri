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

## 決定事項

### D1: 契約を定義で検査せず、本文から求めた事実を呼び出し側で使う

- 決定: 書き込みを宣言の region の契約（`RegionSlots`）で表さず、特殊化した本文から「どの引数の借用がどの排他参照の先へ入りうるか」を求め、呼び出し側で書き込み先の持ち主へ足す。定義の位置では新しい診断を出さない。
- 理由: 型変数・関数の型・スライスの要素には region を書けないので、契約で表すと、どの形でも安全なプログラムを定義の位置で拒否する（「落とし穴」の 2 回分）。事実なら、拒否は残した借用が実際に破られる呼び出し側だけに出る。A28 の事実と同じ枠で求められる。
- 状態: 既定案（実装者はこの案に従う。成り立たなければ停止条件）。

### D2: 部分適用と関数値は保守的に扱い、拒否しない

- 決定: 関数値の呼び出しは、loan を運ぶ引数と関数値自身の借用を、排他参照のすべての引数の先へ残す。部分適用を `region_callback_stores_exclusive_input` で拒否しない。
- 理由: 基準は部分適用を受理している（`let w = Array.set 0 (\() -> 5)` など）。保守的に残すだけなら、拒否は残した借用を破る呼び出しに限られる。
- 状態: 既定案。
