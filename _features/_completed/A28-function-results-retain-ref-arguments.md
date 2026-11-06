# A28: 関数を含む型の呼び出し結果が、`ref` 引数の借用をすべて持つとみなされる（`HashMap.remove` と、関数の `Arc` の `Arc.share` が誤って `E1013`）

| 項目 | 内容 |
| --- | --- |
| ID | A28 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | C09, TZ02 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-05（TZ12 の作業中に発見。HEAD `5ee6b0f2` で確認・原因を調査） |
| 承認 | 不要（健全なまま、誤って拒否しているプログラムを受理する。受理していたプログラムは変えない） |
| 改善する劣位 | 不具合: 値の型が関数を含む `HashMap`（`HashMap<i64, unit -> i64>`・`Owned.Function`・`Maybe<fn>`・関数を持つレコード）の `HashMap.remove` が、std の中で `E1013`（`cannot return a reference to a local value`）になる。関数の `Arc` を `Arc.share` した値を tzroutine が捕捉すると `E1013`（`the captures of a tzroutine cannot retain borrowed values, ...`）になる。利用者の関数でも、`ref` 引数を使わずに関数値を返すだけの呼び出しが同じく拒否される |
| 手本にする既存実装 | `src/ownership.rs` の `closed_returns`・`closed`（結果が借用を持たないことの、呼び出しグラフ上の証明）と、`read_places`（`deref (Arc.get (ref shared))` が、`shared` の借用ではなく、`shared` が持つ借用だけを持つことを表す） |
| 主な影響ファイル | `src/ownership.rs`, `tests/hash_map.rs`, `tests/rc.rs`, `tests/tzroutine.rs`, `tests/currying.rs`, `docs/architecture.md`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `_features/README.md` |

## 目的

呼び出しの結果が関数を含む型のとき、`ref` で渡した引数の借用を結果がすべて持つとみなす保守的な規則を、callee の本文から分かる事実で狭める。
`HashMap.remove` を関数の値の表で使えるようにし、[tzroutine](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) のページが勧める「各 tzroutine に `Arc.share` を渡す」書き方を、
関数の `Arc` でも書けるようにする。健全性（借用が持ち主より長く生きないこと）は保つ。

## 着手条件と停止条件

### 着手条件

- C09・TZ02 が `_features/README.md` で done であること（HEAD で done）。
- GUIDE §2.3 の基準コマンドが成功し、「再現」と「拒否し続けるプログラム」をすべて実行して、今の結果（受理・拒否と kind）を記録していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 「拒否し続けるプログラム」のどれかが受理される。
- 今受理しているプログラム（`tests/` の全件と `examples/`）の診断・IR が変わる。
- 事実の計算が呼び出しグラフで収束しない、または 1 つの module の検査時間が 10 % を超えて伸びる。
- A30（所有関数の借用の穴）を直さないと、このチケットのテストが書けない（A30 を先にする）。

## 現状（HEAD `5ee6b0f2` で確認・調査）

### 規則

- `src/check.rs:787` の `Type::carries_loans` は、`Reference` と `Function` で真（795 行）。レコード・union・`Vec`・配列・tuple・`Rc`/`Arc` を再帰でたどるので、
  `HashMap<i64, unit -> i64>`、`Maybe<fn>`、関数を持つレコード、`Owned.Function`（`std/Owned.tz:4` の `run: 'a -> 'b` を持つレコード）、`Arc<fn>` はどれも真になる。
- 関数の型（`src/check.rs:173` の `Type::Function`）は、環境が借用を持つかどうかを表さない。関数の型には名前付きの region を書けない
  （`src/regions.rs:85〜91` の `NamedRegionInFunctionType`、108〜113 行の `RegionOnNonBorrow`）。std が注釈で避ける手段も無い。
- `src/ownership.rs:2675` の `eval_call`: callee に region の契約が無ければ、各引数の `Value` の loan を結果に足す（2797 行 `None => current.loans.extend(&value.loans)`）。
  落とすのは、callee が `closed`（結果が借用を持たないと証明済み）か、残りの型が `carries_loans` でないときだけ（2801〜2814 行）。結果の型が `carries_loans` なら、
  結果はその loan を持つ（2815〜2818 行）。`(ref x)` の引数の loan は `E::Borrow`（3033〜3056 行）が `loan()`（1147〜1166 行）で作り、そのとき `x` が持つ loan を `parents` に記録する。
- `closed_returns`/`closed`（582〜667 行）は、結果が借用をまったく持たないことだけを証明する。`own` の引数から作った結果は `closed` にならない。
- 検査は、特殊化して下げた本文で行う（`src/check.rs:9241〜9271`: `polymorph::specialize` → `closures::lower` → `ownership::check_all_with`）。

### 不具合 1: `HashMap.remove`

- `std/HashMap.tz:395〜396` の `remove key map = remove_ref (ref key) map`。`remove_ref`（368〜384 行）は `map` か作り直した表を返すので `closed` でなく、
  結果が `remove` の所有引数 `key` の loan を持つ。`check_result`（`src/ownership.rs:1243〜1287`）は、`key` が外から来た場所（`self.external`）でないので
  `cannot return a reference to a local value`（kind `task_result_retains_borrow`。tzroutine と関係なくてもこの kind）を報告する。
- 実際には、`remove_ref` は `key` を `hash_of (ref map) key`（90 行。`i64` を返す）と `probe (ref map) key hash`（128 行。`i64` を返す）にしか渡さない。
- `Map.remove` が通るのは、`std/Map.tz:340〜351` が `ref key` を `locate`（148 行。`(i64 * i64)` を返す）にしか渡さず、その型が loan を運ばないから。
- f64ca392（2026-10-23）より前の `remove` も同じ形で、不具合はそれより古い。

### 不具合 2: 関数の `Arc` の `Arc.share`

- `Arc.share` は `$builtin` の wrapper（`src/closures.rs:400〜490`。`region_sources: None`、本文は `Call(Builtin ArcShare, [Local 0])`）に下がる。wrapper は `closed` でないので、
  `mine` は `shared` の loan を持つ。tzroutine の本文は `Type::Once` の閉包で、`E::Closure` の捕捉の検査（`src/ownership.rs:3129〜3139`）が loan を持つ捕捉を
  `task_capture_retains_borrow` で拒む。
- 実際には、`Arc.share` は新しい強い所有者を作り、`shared` の借用ではなく、ブロックの中身（loan の `parents`）を共有するだけである。`read_places`（2406〜2425 行）は
  同じことを `deref (Arc.get (ref shared))` についてすでに表している（関数が借用を持たなければ tzroutine へ渡せ、持てば拒む）。
- `Arc<i64>` で通るのは型が loan を運ばないから。`shared` そのものの捕捉が通るのは、`Arc.new` の引数の閉包が捕捉を持たず、`shared` が loan を持たないから。

### 同じ原因の別の形（調査で実行して確かめた）

| プログラム | 今の結果 |
| --- | --- |
| `let key = 1`、`let map = HashMap.remove_ref (ref key) map`、`tzroutine { let _n = HashMap.length (ref map) }` | `task_capture_retains_borrow` |
| 上の `key` を string にし、`remove_ref` の後に `let moved = key` | `E1014`（`borrow_conflict`） |
| `fn make _unit = let shared = Rc.new (\v -> v + 1)` に続けて `Rc.share (ref shared)` を返す（`Arc` も同じ） | `borrow_outlives_block` |
| `Arc.share` の後に `shared` を move する | `E1014` |
| 利用者の関数: `def pass :: ref i64 -> own (unit -> i64) -> (unit -> i64)`、`fn pass _key f = f`、`fn wrap key f = pass (ref key) f`（generic な `ref 'k -> own 'v -> 'v` も同じ） | `task_result_retains_borrow`（HEAD `5ee6b0f2` で確認） |
| 同じ generic な `pass` を `i64` の値で使う | 受理 |

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64）

```text
let map = HashMap.insert 1 (\() -> 42) (HashMap.empty())
let map = HashMap.remove 1 map
HashMap.length (ref map)
```

`tsuzuri run` → `std/HashMap.tz:396:21: error[E1013]: cannot return a reference to a local value`（`fn remove key map = remove_ref (ref key) map` の位置）。

```text
let shared = Arc.new (\value -> value + 1)
let mine = Arc.share (ref shared)
tzroutine {
    let _n = Arc.strong_count (ref mine)
}
0
```

`tsuzuri run` → `Main.tz:3:1: error[E1013]: the captures of a tzroutine cannot retain borrowed values, including borrowed function environments`。

### 拒否し続けるプログラム（調査で実行して確かめた。どれも今は `E1013` か `E1014`）

| 名前 | 内容 | 今の kind |
| --- | --- | --- |
| `keep_leak` | `def keep :: ref string -> (unit -> i64)`、`fn keep text = \() -> String.length text`、`fn leak text = keep (ref text)` | `task_result_retains_borrow`（呼び出し方により `borrow_of_temporary`） |
| `keep_tz` | 同じ `keep (ref text)` を tzroutine が捕捉する | `task_capture_retains_borrow` |
| `class_escape` | `Traits.tt` に `class Make<'value> { def make :: ref string -> 'value }`、`instance Traits.Make<Holder> { fn make text = Holder { run: \() -> String.length text } }`、`fn pick text map = HashMap.insert 1 (Traits.Make.make text) map`、`fn leak text = pick (ref text) (HashMap.empty())` | `task_result_retains_borrow` |
| `arc_env_tz` | `let text = "abc"`、`let borrowed = ref text`、`let shared = Arc.new (\() -> String.length borrowed)`、`let mine = Arc.share (ref shared)` を tzroutine が捕捉する（`shared` を直接捕捉しても） | `task_capture_retains_borrow`（HEAD `5ee6b0f2` で確認） |
| `arc_env_return` | 同じ `shared` を関数の中で作り、`Arc.share (ref shared)` を返す | `borrow_outlives_block` |
| `rc_get_return` | `Rc.get (ref shared)` を持ち主のブロックから返す | `borrow_outlives_block` |
| `hashmap_env_return` | `HashMap.remove 2 (HashMap.insert 1 (\() -> String.length borrowed) …)` を返す | `borrow_outlives_block` |
| `mapr_leak` | `Array.mapr (\text -> \() -> String.length text) (ref texts)` を返す | `borrow_outlives_block` |
| `copy_out_tz` | `fn copy_out f = deref f` に、`text` を借用する `g` の `ref g` を渡し、結果を tzroutine が捕捉する | `task_capture_retains_borrow` |
| `arc_get_env_tz` | 借用する関数の `deref (Arc.get (ref shared))` を tzroutine が捕捉する | `task_capture_retains_borrow` |
| 既存 | `tests/currying.rs:75〜135` と 163 行以降（`keep (&x)`、`make :: &'a -> (unit -> &'a)`、`Callback { run: u -> r.length }`） | 既存の期待どおり |

`keep_leak` と `pass` は署名が同じ（`ref` の引数と、関数を返す）なので、署名からの推論（parametricity）では区別できない。`class_escape` は、generic な `ref string -> HashMap<i64, 'v>`
がクラスのメソッドを通して参照を捕捉する `'v` を作る例である。

## 仕様

言語の意味・構文・診断の文言は変えない。借用検査の結果を次のように変える。

- 既知の callee（利用者・std の関数、`$builtin` の wrapper）を、region の契約なしに、全引数を渡して呼ぶとき、引数 *i* が結果に渡すものを次の 3 つから決める。
  - **保持**（今の振る舞い）: 引数の `Value` 全体（`ref x` の loan を含む）。callee の中で、引数 *i*（その再借用・フィールド・要素を含む）が次のどれかに当たるとき。
    (a) 結果へ参照として流れる。(b) 閉包の環境・`let rec` のグループ・部分適用・借用の `dyn` に捕捉される。(c) 結果が loan を運ぶ呼び出しの、保持しうる位置へ渡される
    （不明・コールバックの callee、クラスのメソッド、高階の組み込み関数、その引数の事実が「保持」の既知の callee）。(d) `ref mut` を通して書く呼び出し（`region_slots.writes`）へ渡される。
    部分適用は常に「保持」。
  - **一時**: 何も渡さない。引数 *i* のすべての出現が、型が `carries_loans` でない位置（`i64`・`bool` を返す呼び出しの引数、比較、`.length` など）か、
    その引数の事実が「一時」の既知の callee の引数であるとき。
  - **読み通し**: 借りた場所が持つ loan だけを渡す。引数の各 loan について、局所の根なら `parents`、外から来た根と名前付きの target slot ならその loan 自身
    （`read_places` と同じ）。組み込み関数の許可リストの第 1 引数だけ: `Rc.share`・`Arc.share`・`Rc.downgrade`・`Arc.downgrade`・`Rc.upgrade`・`Arc.upgrade`。
    ただし局所の根の loan の `parents` に外から来た根の loan があれば（借りた場所の値が引数・持ち上げたラムダの捕捉から来た）、その局所の loan 自身を渡す（4 回目のレビュー対応で追加）。
    `Rc.get`・`Arc.get` とほかの組み込み関数は「保持」のまま（高階の組み込み関数はコールバックへ参照を渡す）。
- この結果、「再現」の 2 つと「同じ原因の別の形」の表の誤った拒否は受理され、「拒否し続けるプログラム」はすべて今と同じ kind で拒否される。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/ownership.rs` | `parameter_retention`（新規） | 特殊化した各関数の各引数について「保持」「一時」を、呼び出しグラフの最小不動点（初期値「一時」、規則で「保持」へ上げる）として計算する。`closed_returns` と同じ 4 か所（207・229・265・531 行の呼び出し元）で計算し、`self.closed` と同じく `Checker` に渡す |
| `src/ownership.rs` | `eval_call` の 2797 行 | 既知の callee で全引数を渡す呼び出しは、引数の事実が「一時」なら loan を足さず、「読み通し」の組み込み関数の第 1 引数なら `parents` を足す |
| `src/ownership.rs` | `read_places` | 「読み通し」と共有できる部分を helper に出す |
| `tests/hash_map.rs`・`tests/rc.rs`・`tests/tzroutine.rs`・`tests/currying.rs` | 新しいテスト | 下の「テスト計画」 |
| `docs/architecture.md` | 1578〜1581 行（「すべての入力に対する保守的な借用追跡」）、1080・1524 行 | 3 つの事実を書く |

### アルゴリズム（`parameter_retention`）

1. 各関数の各引数を「一時」で始める。
2. 関数の本文を走査し、引数（とその再借用・射影）の各出現の位置を見る。位置が仕様の (a)〜(d) のどれかなら「保持」にする。既知の callee の引数の位置なら、その callee の
   その引数の現在の事実を使う。
3. どれかの事実が変わったら 2 をやり直す（「保持」へ上がるだけなので有限回で止まる）。
4. 再帰・相互再帰の関数は、不動点で正しく扱われる（最初は「一時」と仮定し、矛盾すれば「保持」に上がる）。

## 実装手順

### 手順 1: ベースラインと記録

- 変更: なし（`target/scratch/a28/` の program だけ）。
- 内容: GUIDE §2.3 の基準コマンド。「再現」「同じ原因の別の形」「拒否し続けるプログラム」をすべて `target/scratch/a28/<名前>/Main.tz` に書き、`tsuzuri check <dir> --json` の
  code と kind を記録する。表と違えば停止する。
- 確認: 記録をこのチケットの末尾に追記した。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/hash_map.rs`、`tests/rc.rs`、`tests/tzroutine.rs`、`tests/currying.rs`。
- 内容: 「テスト計画」の受理のテスト（修正前は失敗する）と、拒否のテスト（修正前も後も成功する）。
- 確認: 受理のテストだけが失敗する。

### 手順 3: 「一時」を入れる

- 変更: `src/ownership.rs`（`parameter_retention` と `eval_call`）。
- 確認: 不具合 1・`pass`・`E1014` の 2 つの形の受理のテストが成功し、拒否のテストがすべて成功する。`cargo test --locked` の全件が成功する。

### 手順 4: 「読み通し」を入れる

- 変更: `src/ownership.rs`（`eval_call` と `read_places` の helper）。
- 確認: 不具合 2・`Rc.share` を返す形の受理のテストが成功し、`arc_env_tz`・`arc_env_return`・`rc_get_return` の拒否が今の kind のまま。

### 手順 5: 既存の結果が変わらないこと

- 確認: `tests/` の全 fixture と `examples/` の `tsuzuri check --json` の診断と `--emit llvm` の IR が、修正の前後で一致する（受理が増えるだけで、受理済みのプログラムの IR は変わらない）。
  `tests/conformance/` の OWN・TSK の規則（`node tests/conformance.mjs` かそれに相当する runner）が成功する。検査の時間（`cargo test --locked --test hash_map` などの大きい std を使うテスト）が 10 % を超えて伸びない。

### 手順 6: ドキュメントと最終確認

- 「ドキュメント」の各ページ。T2。

## テスト計画

### Rust テスト

- `tests/hash_map.rs`（`seeded_hash_containers_and_borrowed_keys_type_check` の近く）: `HashMap.remove` を、値が `unit -> i64`・`Owned.Function`・`Maybe<fn>`・関数を持つレコードの表で受理する。
  `remove_ref (ref key)` の後に `key` を move できる。`hashmap_env_return` は拒否のまま。
- `tests/rc.rs`（345〜358 行の `apply (Arc.share (ref shared))` の近く）: `Arc.share` の後に `shared` を move できる。`Rc.share`・`Arc.share` を返せる。`arc_env_return`・`rc_get_return` は拒否のまま。
- `tests/tzroutine.rs`（`checks_the_body_and_the_captures`、67 行の近く）: 不具合 2 を受理する。`arc_env_tz`・`arc_get_env_tz`・`copy_out_tz` は拒否のまま。
- `tests/currying.rs`（75 行の近く）: `pass`（generic も）を受理する。`keep_leak`・`keep_tz`・`mapr_leak` は拒否のまま。
- `class_escape`: 2 つの module が要るので `analyze_modules` を使うテストのファイル（`grep -ln analyze_modules tests/*.rs`）に置く。
- 必要なら `src/ownership.rs` の単体テスト（4252 行以降）に `parameter_retention` の不動点のテスト（相互再帰の関数）。

### E2E

- 不具合 1・2 のプログラムを `tsuzuri run` で native の `-O0`・`-O3` に通す（受理しただけでなく、正しく動き、解放の追跡で漏れが無いこと。`tests/features.mjs` の suite `hash_map`
  か `rc` に export を足す）。

### 既存テストへの影響

- 拒否の期待を持つ既存テストは変わらない（停止条件）。`tests/hash_map.rs` と fixture の `hash_map` が TZ12 で `Owned.Function` の代わりに Drop のレコードを使った箇所
  （TZ12 のチケットの「実装と検証」）は、`Owned.Function` に戻してよい。

### 性能

- 検査時間（手順 5）。生成コードは変わらない。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md:280〜293`（現在の制限の表）: 関数を含む型の結果が `ref` 引数の借用を持つとみなされる、という制限を書くなら、
  直した後の規則（一時・読み通し）に置き換える。今は書かれていないので、書かれていないことを確かめる。
- `_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md:79〜107`: 各 tzroutine へ `Arc.share` を渡す説明に、関数の `Arc` の例を足してもよい（`check-docs.mjs` を通す）。
- `_tsuzuri/language-reference/built-in-types-and-modules/rc.md:116` と 160〜171 行、`values-and-functions/lambda-expressions.md:200〜216`: 説明が直した後の振る舞いと一致するか確かめる。
- `docs/architecture.md:1578〜1581`・1080・1524 行と、`docs/language.md:711`・2539・3556〜3573 行: 借用の追跡の説明に 3 つの事実を足す。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] 「再現」の 2 つが受理され、native の `-O0`・`-O3` で正しく動く。
- [ ] 「同じ原因の別の形」の誤った拒否（`pass`、`E1014` の 2 つ、`Rc.share` を返す形）が受理される。
- [ ] 「拒否し続けるプログラム」がすべて、手順 1 で記録した kind のまま拒否される。
- [ ] 既存の fixture・examples の診断と IR が変わらない。
- [ ] 検査の時間が 10 % を超えて伸びない。
- [ ] T2 が成功する。

## 落とし穴

- 署名だけから「一時」を決めてはいけない（`keep` と `pass` は同じ署名）。必ず特殊化した本文から計算する。
- generic な callee の事実は、特殊化ごとに違いうる（`class_escape`）。特殊化した関数ごとに計算する。
- `Arc.get`・`Rc.get` を「読み通し」に入れてはいけない。返すのは `shared` が指すブロックへの参照で、`shared` の借用そのものである（`rc_get_return`）。
- A30 の穴（所有関数が借用を持つ関数値を受け取れる）と混ぜない。このチケットは誤った拒否だけを直し、受理していた危険なプログラムは A30 が塞ぐ。

## 対象外

- `deref` で値を写して返す利用者の関数（`copy_out`）を「読み通し」と推論すること（`copy_out (ref g)` の `g` が借用を持たなくても今は拒否する。拒否のまま）。
- `closed()` の `E::Call` の腕（631〜642 行）を同じ事実で広げること（追加の改善。必要なら別のチケット）。
- 関数の型に、環境が借用を持つかどうかを表す印を足すこと（言語の変更になる）。

## 決定事項

### D1: 型の規則ではなく、callee の本文の事実で狭める

- 決定: 引数ごとの「保持」「一時」を特殊化した本文から計算し、`Arc.share` などの許可リストの組み込み関数だけを「読み通し」にする。
- 理由: 関数の型は環境の借用を表さず、署名が同じでも振る舞いが違う（`keep` と `pass`）。型の印を足す案は言語の変更で、承認が要る。
- 状態: 既定案（実装者はこの案に従う）。

### D2: 診断の kind は変えない

- 決定: 拒否し続けるプログラムの code と kind は今のまま。`HashMap.remove` の不具合で tzroutine と関係なく `task_result_retains_borrow` が出ていた点も、このチケットでは変えない。
- 理由: 誤って拒否されていたものが受理されれば、その kind は出なくなる。kind の名前の整理は別の問題。
- 状態: 既定案。

### D3: 本体が書き込める場所をほかのコードへ渡す関数は、事実も読み通しも使わない（3 回目のレビュー対応で決定、5 回目と 6 回目で広げ、7 回目で値の破棄を加え、8 回目でプログラム全体の条件を主な理由にした）

- 決定: `retained_parameters` は、本体が排他参照に届く型の引数（`ref mut values`、`ref mut` の引数、それを持つ値）を呼び出しへ渡す関数に事実を与えない（結果はすべての引数の借用を持つ）。そうした本体と、その中で作るラムダ（持ち上げた関数。ラムダの中のラムダを含む）の本体では、`Rc`／`Arc` の `share`・`downgrade`・`upgrade` も読み通さない。特殊化した本文と抽象本文の両方に適用する。`Array.write` などの書き込みの追跡は、A30 の先端のまま変えない。
- 5 回目の対応で広げた決定: 「渡す」は、呼び出し（`Call`・`HostCall`・`Parallel`）の引数、パイプラインの左辺、ほかの二項演算子の被演算子、クロージャの捕捉のどれかの型が `Type::lends_mutable` を満たすことです。格納された値・参照先・`Rc`／`Arc` とその弱参照の中身・関数型の引数と結果の型をたどって、排他参照（`ref mut`、排他 region を持つレコード、`MatrixView.Mut`）か、型引数が loan を運ぶ共有セル（`Type::is_mutable_cell`: std の `Mutex`・`RwMutex`・`OnceValue`・`Atomic`・`SyncMap`・`Channel`・`SendOnly`・`ReceiveOnly`・`DataChannel.Sender`・`DataChannel.Receiver`・`ErrGroup`）に届けば渡します。持ち上げたラムダが渡すなら、ラムダを持ち上げた元の関数も渡すとみなします（`lending_bodies` は両方向に伝えます）。読み通しを本文の局所値の中身に限る 4 回目の制限はそのままです。関数名は `lends_exclusive` から `lends_mutable` に変えました。
- 6 回目の対応で広げた決定: 本体が借用を持ちうる値を、本体の見えないコードへ渡す関数も同じにする（オーケストレーターの判断、2026-11-06）。見えないコードへの呼び出しは、(1) callee が名前付き関数・組み込み関数の直接の参照でない呼び出し（引数・捕捉・局所値・フィールド・要素・呼び出しの結果・部分適用を入れた値。仮引数より多い引数を渡した名前付き関数の結果の呼び出しも）、(2) dyn のメソッドの dispatch（`E::DynDispatch`）、(3) `Owned.call`、(4) 引数を実行しうる組み込み関数（scheme の引数が関数型か、関数を持つ std の型（`Owned.Function`・`Seq`）を含むか、制約を持つ: `Array.map`・`Array.fold`・`Mutex.with_lock`・`Parallel.*`、コールバックを取る host 関数、`new` の初期化関数）へ関数を実行しうる値を渡す呼び出し、(5) そうした呼び出しをする関数（std の関数を含み、持ち上げたラムダとの両方向にも伝える）を、借用を持ちうる値と関数を実行しうる値（関数・dyn・`Owned.Function`・tzroutine の本体に届く型）を含む全引数で呼ぶ呼び出しです。どれも借用を持ちうる引数を渡すときで、呼ぶ関数値の環境だけが借用を持ちうる呼び出し（`t ()`）は事実だけを捨て、読み通しはします。抽象本文では型変数を受け手とするクラスのメソッドを名前付き関数と同じに扱います（オーケストレーターの判断からの変更。理由は 6 回目の対応の段落）。
- 7 回目の対応で加えた決定（オーケストレーターの判断、2026-11-06。2 点を変えた。下の 7 回目の段落）: 値の破棄は本文に呼び出しを書かずに `Drop.drop` を実行するので、プログラムのどれかの `Drop.drop` の本体が事実を持たない（書き込める場所を渡すか、見えないコードへ渡す: `Lending::drops_store`）とき、本体が受け取る・作る値で、型が破棄でコードを実行しうる（`Type::drop_runs_code`: 格納された値として、利用者の `Drop` を持つ型・関数・dyn・`Owned.Function`・tzroutine の本体に届く）ものが持つ loan の引数を保持にし、そうした値で loan を持つものがあり、読み通しを使う本体は読み通さない。`HashMap.remove` の事実は変わらない（破棄する要素の loan は `map` のもので、結果はもともとそれを持つ）。
- 8 回目の対応で加えた決定（オーケストレーターの判断、2026-11-07。下の 8 回目の段落）: 健全さの主な理由をプログラム全体の条件にする。プログラムのどれかの関数（特殊化したモジュールの関数。std の関数、持ち上げたラムダ、`$builtin` の生成関数を含む）が、借用を持ちうる型の値を格納を追跡しない組み込み関数（`Array.write`・`SyncMap.__replace`・`Simd.store`・`Atomic.store`・`Atomic.swap`・`Atomic.compare_exchange`）で格納するなら、どの関数も事実を持たず、読み通しもしない（A30 の先端と同じ）。上の 3〜7 回目の局所の除外は、この条件に重ねて残す。
- 理由: 所有権検査は `Array.write`（とそれで書き込む std の関数）と `SyncMap.__replace` が書き込む値の借用を書き込み先の持ち主に残さない（A28 より前からの穴、[A31](../A31-in-place-writes-keep-stored-borrows.md)）。A28 の事実と読み通しは本文の結果の loan を信頼するので、そのままでは穴を関数の境界の外へ広げる。この前提（書き込み関数はどれも書き込み先を排他借用の引数で受け取る）は誤りでした（5 回目）: `Mutex.with_lock`・`RwMutex.with_write`・`Cond.wait_with` は lock を共有参照で受け取り、中身の `ref mut` をコールバックへ貸すので、コールバックが関数値なら呼び出しの引数に排他参照が現れません。5 回目からの理由は、書き込む関数を列挙しません。追跡されない格納は排他参照か可変セルを通して書き込み（代入・`deref r = v`・`Vec.push_mut`・region の契約を持つ関数は追跡され、それ以外の値への共有参照は書き込む手段を渡さない）、本文の値へ届くその排他参照やセルをほかのコードが得るのは、本文が渡す被演算子と本文が作るラムダの捕捉からだけで、関数値の引数が書き込めるのは捕捉したものと呼び出しで受け取るもの（引数の型が示す）だけです（6 回目に誤りと分かった: 捕捉したセルには本文の別の入力も届きうるので、本文が値を見えないコードへ渡すことを 3 つめの経路として除外する。下の 6 回目の決定）。そのため、書き込める場所を渡さない本文の局所値の loan は完全で、渡す本文を A28 より前の規則に戻せば A31 の穴を関数の境界の外へ広げない（健全な理由の全文は `src/ownership.rs` の `lends_mutable` のコメント）。ただし引数（持ち上げたラムダの捕捉を含む）の外部 loan は呼び出し元の値の loan を表し、呼び出し元が書き込める場所を渡す本文なら不完全でありうるので、読み通しは外部 loan に届くときには使わない（4 回目の対応で追加）。事実は callee が結果に入れない引数を除くだけなので、呼び出し元の完全さに依らない。こうして A28 は基準が受理するプログラムを変えず、基準が拒否する危険なプログラムを受理しない。
- 取り下げた案: 1〜2 回目の対応では、書き込みを追跡して穴そのものを塞ぎました（書き込み関数に region の契約を与える。旧 D3）。どの回も基準が受理する安全なプログラムを拒否し、A28 の承認（誤った拒否を受理に変えるだけ）を超えるので、オーケストレーターの判断で全部を取り下げ、穴は A31 に分けました。拒否した形の一覧は A31 の「落とし穴」にあります。
- 状態: 実装済み（オーケストレーターの判断、2026-11-06）。

## 実装と検証（2026-11-06）

A30 のブランチ（`fix/a30-owned-function-borrow`、`03cb108e`）の上に積みました。手順 1〜6 をすべて行いました。

### 手順 1: ベースライン（A30 の先端のバイナリ、`tsuzuri check <dir> --json`）

「再現」「同じ原因の別の形」「拒否し続けるプログラム」を `target/scratch/a28/p/<名前>/Main.tz` に書いて記録しました（CLI の既定の edition 2027。読むだけの引数は借用です）。表と食い違いはありませんでした。

| 名前 | 修正前 | 修正後 |
| --- | --- | --- |
| 再現 1（`HashMap.remove`） | `E1013`/`task_result_retains_borrow`（`std/HashMap.tz:396`） | 受理 |
| 再現 2（関数の `Arc.share` を tzroutine が捕捉） | `E1013`/`task_capture_retains_borrow` | 受理 |
| `remove_ref (ref key)` の後に tzroutine が表を捕捉 | `E1013`/`task_capture_retains_borrow` | 受理 |
| string の `key` で `remove_ref` の後に `key` を move | `E1014`/`borrow_conflict` | 受理 |
| `Rc.share (ref shared)` を返す（`Arc` も） | `E1013`/`borrow_outlives_block` | 受理 |
| `Arc.share` の後に `shared` を move | `E1014`/`borrow_conflict` | 受理 |
| `pass`・generic な `pass` の `wrap`（関数だけ） | `E1013`/`task_result_retains_borrow` | 受理 |
| 同じ `wrap` を `wrap 1 (\() -> 42)` と呼ぶ（edition 2027） | 上と `E1013`/`borrow_of_temporary` | `borrow_of_temporary` だけ（`wrap` の `f` は読むだけなので借用で、結果はその一時値を持つ。edition 2026 の `analyze` では受理） |
| generic な `pass` を `i64` で使う | 受理 | 受理 |
| `keep_leak`（`own string` の `leak`） | `E1013`/`task_result_retains_borrow` | 同じ |
| `keep_leak`（読むだけの `leak "abc"`） | `E1013`/`borrow_of_temporary` | 同じ |
| `keep_tz` | `E1013`/`task_capture_retains_borrow` | 同じ |
| `class_escape`（`own string` の `leak`） | `E1013`/`task_result_retains_borrow` | 同じ |
| `class_escape`（ブロックの局所値） | `E1013`/`borrow_outlives_block` | 同じ |
| `arc_env_tz`（`mine` を捕捉・`shared` を直接捕捉） | `E1013`/`task_capture_retains_borrow` | 同じ |
| `arc_env_return` | `E1013`/`borrow_outlives_block` | 同じ |
| `rc_get_return` | `E1013`/`borrow_outlives_block` | 同じ |
| `hashmap_env_return` | `E1013`/`borrow_outlives_block` | 同じ |
| `mapr_leak` | `E1013`/`borrow_outlives_block` | 同じ |
| `copy_out_tz` | `E1013`/`task_capture_retains_borrow` | 同じ |
| `arc_get_env_tz` | `E1013`/`task_capture_retains_borrow` | 同じ |

### 実装（`src/ownership.rs`、`src/check.rs`）

- `CallFacts`（`closed` と `retained`）が `closed_returns` の結果を置き換え、`check_body`・`Checker` に渡ります。`closed_returns` の 4 か所（`check_functions`・`check_recovered`・`copy_reads`・`events_abstract`）で `call_facts(module, infer)` を計算します。
- `check_body` の前半を `start_checker`（`Checker` を作り、引数に呼び出し元の loan を置く）に分けました。`retained_parameters` は同じ `start_checker` で本文を評価し、結果の `Value` の loan とその `parents` をたどって、外部ルートに届く引数を「保持」にします。本文の検査が失敗するか、結果が局所値の loan を持てば、すべて保持です。
- `parameter_retention` は、完全な直接呼び出しで、結果の型が `retention_applies`（`carries_loans` で `contains_stored_reference` でない）で、`closed` でない callee（`$builtin` の生成関数・`is_once`・region の契約・`Drop.drop`・引数のない関数を除く）を集め、すべて「一時」から始めて、callee を先にした後順と、変わった関数の呼び出し元を戻す作業リストで最小不動点を求めます。loan を受け取らないのに借用を持ちうる引数（型変数の引数と `Owned.Function`）は最初から保持です。検査の回数が「関数の数 × 8 + 1024」を超えれば、事実をすべて捨てて以前の規則に戻ります。
- `eval_call`: 事実が「一時」の引数は loan を結果に足しません（引数は `held` にあるので、呼び出しの間は貸したままです）。`Rc`／`Arc` の `share`・`downgrade`・`upgrade` の第 1 引数は `read_through` で、局所値の loan は `parents`、外部ルートと静的データの loan はそれ自身を足します。
- `Type::contains_unknown`（`src/check.rs`）: 型変数・未解決の型を含むか。

### チケットから外れた判断

1. **結果の型が参照を含む呼び出しには事実を使いません（`retention_applies`）。** 最初の実装（事実をすべての結果に使う）は、`tests/maybe_result.rs` の `choose :: ref i64 -> ref i64 -> Maybe<ref i64>`（`fn choose left right = Some left`）と `tests/types_ownership.rs` の同じ形の拒否を受理に変えました。`ownership-and-memory/lifetimes.md` は「名前付き region がない、2 つ以上の参照を受け取る関数の結果は両方の入力に拘束される」と、参照の結果ではシグネチャを契約にしています（本体を変えても呼び出し側が壊れない）。これは停止条件（既存の拒否の期待が変わる）に当たるので、チケットの目的（関数の型は環境の借用を表せず、名前付き region も書けない）に合わせて、型が参照を含まず、借用を関数の環境にだけ持ちうる結果に限りました。チケットの再現と別の形はすべてこの範囲です。
2. **事実は「読み通し」以外を、本文の検査そのもので求めます。** チケットの設計の (a)〜(d) を構文の位置で判定する代わりに、callee の本文を通常の借用検査（外部 loan を置いた `start_checker`）で評価し、結果の loan に残る引数を保持にしました。`check_result` が局所値の逃げを拒む根拠と、A12 の region の契約（`check_result` が結果の loan の持ち主を照合し、呼び出し側は `argument_regions` で選ぶ）が頼っているのと同じ追跡なので、新しい健全性の仮定を足しません。(c)（未知の callee・クラスのメソッド・高階の組み込み・保持の callee）は、その呼び出しの結果が引数の loan を持つことで、(b)（捕捉・部分適用・借用の `dyn`）は閉包と部分適用の値が捕捉の loan を持つことで、(d)（`ref mut` を通した書き込み）は、外部の参照を通した格納が名前付き region（`region_slots`）を要し、その関数を対象外にすることで、それぞれ保持になります。
3. **抽象本文（`infer`）の事実は Copy 制約の推論だけに効きます。** generic な `pass` の `ref 'k` は外部 loan を受け取るので一時になり、推論の段で `wrap` が通ります。型変数の値が隠す借用（`class_escape` の `'v`）は抽象本文では見えませんが、受理・拒否を決めるのは特殊化した本文の検査で、そこでは型が具体です（`class_escape` は特殊化した `pick<Holder>` が `text` を保持して拒否されます）。全 fixture と `examples/` で、事実ありと事実なし（以前の規則）の `check_body` の結果（Copy 制約と診断）が、推論の段と最後の段の両方ですべての関数で一致することを、一時的な検査で確かめました（コミットしていません）。
4. **部分適用と、`()` まで取る関数。** `fn keep text = \() -> ...` は `()` まで引数に取るので `keep (ref text)` は部分適用で、事実を使いません（単体テストで確認）。
5. **`Rc.get`・`Arc.get` は保持のまま**（`rc_get_return` が拒否のまま）。`read_places` との共通化は、`read_places` が場所の値（`stored_value`）を読むのに対し、読み通しは借用した時点の loan（`parents`）を読むので、小さな helper（`read_through`）を別に足しました。

### テスト

- 手順 2: 新しいテストを先に書き、修正前のコードで受理の部分だけが失敗し、拒否の部分が成功することを確かめました（受理の部分を一時的に外した版で 5 件すべて成功）。
  - `tests/hash_map.rs` `removing_from_a_map_of_functions_does_not_borrow_the_key`（値が関数・`Owned.Function`・`Maybe<fn>`・関数を持つレコードの表、`remove_ref` の後の move と tzroutine、拒否 3 件: `hashmap_env_return`・借用する関数を持つ表の tzroutine・その借用元の move）
  - `tests/rc.rs` `sharing_a_function_keeps_the_borrows_of_its_environment_only`（`Rc`・`Arc` の両方。share の後の move、share・upgrade を返す、拒否 5 件: `arc_env_return`・downgrade を返す・`rc_get_return`・環境の持ち主の move・`get` の後の move）
  - `tests/tzroutine.rs` `a_tzroutine_captures_a_share_of_a_function`（再現 2 ほか 3 件、拒否: `arc_env_tz` 2 形・`arc_get_env_tz`・downgrade・`copy_out_tz`）
  - `tests/currying.rs` `function_results_retain_only_the_arguments_that_reach_them`（`pass`・generic・再帰・相互再帰・`pick`、拒否: `keep_leak`・`keep_tz`・`mapr_leak`・再帰で上がる 2 形・コールバック）、`function_result_facts_follow_each_specialization`（`class_escape` を `analyze_modules` で 2 形、捕捉しない `Counter` の具体化は受理）
  - `src/ownership.rs` `retention_tests::parameter_retention_is_the_least_fixed_point_of_the_bodies`（相互再帰の 2 組と `pass`、推論の段と最後の段）
- 拒否の kind はすべて手順 1 の記録と同じです。A30 の拒否のテスト（`an_owned_function_from_a_function_value_holds_no_borrow` など）も成功します。
- E2E: `tests/fixtures/hash_map`（`hash_remove_functions`・`hash_remove_ref_then_move`・`hash_owned_function_drop`。`Owned.Function` の値の表を戻しました。TZ12 の `Pending` も残します）、`tests/fixtures/rc`（`rc_share_function`・`arc_share_function_return`）、`tests/fixtures/tzroutine`（`shares_a_function`）。期待値は JavaScript の BigInt と手計算です。

### 実行したコマンド

- `cargo test --locked --no-fail-fast --test hash_map --test rc --test tzroutine --test currying`（修正前）: 新しい 5 件だけ失敗。修正後: hash_map 6/6、rc 12/12、tzroutine 20/20、currying 11/11。`--test maybe_result` 9/9、`--test types_ownership` 12/12、`--test user_drop` 26/26、`--test timers` 12/12。
- `cargo nextest run --locked`: 手順 3 の後は（読み通しの前なので）rc・tzroutine の新しいテストと、上の判断 1 の 2 件が失敗。判断 1 と手順 4 の後 2941 passed、最後（T2）2942 passed、4 skipped。
- `cargo test --locked --lib retention_tests`: 1 passed。
- `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。
- 手順 5: `tests/fixtures/*`（164）と `examples/*`（13）の `tsuzuri check --json` と `tsuzuri build --no-cache --emit llvm`（151 件が IR を出力）が、A30 の先端のバイナリと最後のバイナリで byte 単位で一致（fixture は A30 の版で比較）。
- `node scripts/conformance.mjs target/debug/tsuzuri --area OWN`: 30 ok。`--area TSK`: 21 ok。
- 検査の時間（`TSUZURI_TIME_PASSES=1 tsuzuri check`、15 回の中央値、A30 の版の fixture）: `check` の段は hash_map 98.2→98.9 ms、list_fsharp 114.6→112.5、array_fsharp 108.8→106.0、vec_dotnet 111.9→112.9、rc 101.6→101.1、tzroutine 91.1→89.7、examples/tzroutines 86.5→84.7。`ownership` の段は 0.1〜0.8 ms 増えます（list_fsharp 3.05→3.87 ms）。全体は −8〜+4 %（ゆれの範囲）で、10 % を超えません。増えた分の大半は呼び出しを集める本文の走査です。
- `node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases（native/WASM、-O0/-O3）。`rc`: 39 cases（WASM threads を含む）。`tzroutine`: 16 cases（native -O0/-O3）。`context` 15、`timers` 9、`channel` 15。`node tests/tzroutine.mjs target/debug/tsuzuri`: 159 runs。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（tzroutine.md・rc.md・lifetimes.md・lambda-expressions.md）: 4 pages、127 links、47 examples、88 native runs ok。`gen-docs.mjs --check`: 19 blocks、0 stale。`check-cli-docs.mjs`: 4 checks、0 failed。
- `node scripts/check-guide-lite.mjs`（チケットを `_completed/` へ移した後）: 88 references、0 failed。

### 更新した文書

- `_tsuzuri/language-reference/values-and-functions/lambda-expressions.md`（共有借用の寿命: 関数値を返す呼び出しの規則と `pass` の例）
- `_tsuzuri/language-reference/built-in-types-and-modules/rc.md`（新しい節「所有者を増やした値の借用」）
- `_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`（関数の `Arc` を tzroutine ごとに `Arc.share` する例）
- `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`（現在の制限の後に、参照の結果はシグネチャ、関数の環境だけの結果は本体、という規則。チケットが確かめるよう求めた制限の表には、この制限は書かれていませんでした）
- `docs/language.md`（新しい小節「関数の結果が持つ借用」、ラムダの捕捉の段落、`Rc / Arc` の「結果の借用」）
- `docs/architecture.md`（名前付き region の段落の後に `CallFacts`・`parameter_retention`・読み通し）

### 変更したファイル

`src/ownership.rs`、`src/check.rs`、`tests/hash_map.rs`、`tests/rc.rs`、`tests/tzroutine.rs`、`tests/currying.rs`、`tests/features.mjs`、`tests/fixtures/hash_map/Main.tz`、`tests/fixtures/rc/Main.tz`、`tests/fixtures/tzroutine/Main.tz`、上の文書、`_features/README.md`、このチケット。

### レビューの指摘への対応（2026-11-06）

独立したレビューの指摘 8 件（健全性 2 件、実装と費用 6 件）に対応しました。先に A30 のレビュー修正（`fix/a30-owned-function-borrow` の `3aa1708b`）をマージし、比較の基準は、その A30 を作り直したバイナリです（以下「基準」）。指摘ごとに、レビューの証拠のプログラムを `target/scratch/a28r/p/`（作業の後に削除）に書いて修正前に再現し、回帰テストを書いてから直しました。新しいテスト 7 件は、修正前のブランチ（`39366e2e` を `git archive` で展開し、新しいテストのファイルだけを差し替えたもの）ですべて失敗します。

#### 1. 重大: in-place の書き込みが格納した値の借用を残さない（以前からの穴を A28 が広げた）

（この項の書き込みの追跡は 3 回目の対応で取り下げました。下の「3 回目の対応」を見てください。）

- 再現: レビューの `stash`/`leak`（`Array.write 0 (\() -> String.length text) (ref mut arr)` の後に `arr[0]` を返す）は、基準で `E1013`/`borrow_outlives_block`、修正前の A28 で受理（実行すると解放後の `text` を読み、9 ではなく 0）。`Rc.share (ref arr[0])` の形も同じ。1 つの関数の中の穴（`Array.write` の後に `text` をムーブして `arr[0] ()`）は、基準でも受理でした。
- 原因: `region_slots.writes` を持つのは `Vec.push_mut` の生成関数だけで、`Array.write` と、それを通して書く std の関数（`Array.set`・`Array.fill`・`Array.blit`・`Vec.copy_to*` など）は、書き込み先の持ち主へ格納した値の loan を足していませんでした。A28 以前は、呼び出し側がすべての引数の loan を結果に残す規則がこれを関数の境界で隠していました。
- 対応（根本原因）:
  - `closures::lower`: 書き込む値と排他参照を引数に持つ組み込み関数（`Vec.push_mut`・`Array.write`・`SyncMap.__replace`）の生成関数は、書き込む値の型が loan を運ぶとき、`Vec.push_mut` と同じ形の `RegionSlots { writes: true }` を持ちます。呼び出し側の `store_arguments` が、値の loan を書き込み先の持ち主に足します。
  - std の関数は型変数に region を書けない（`own 'a { s }` は `RegionOnNonBorrow`）ので、`closures::derive_store_slots` が、特殊化した std の名前付き関数のうち、参照先が loan を運ぶ `ref mut` 引数を持ち、region の契約がない関数に同じ形の契約を与えます（loan を運ぶ引数と、参照先が loan を運ぶ参照の参照先が region 0、参照はそれぞれ別の region）。本体はこの契約で検査され、呼び出し側は書き込みを記録します。`writes` を持つ関数は、`regions::validate_contract_calls` で直接の完全適用か、それを完成させるパイプラインに限られます（パイプラインは `piped_store`・`eval_piped_store` が `store_arguments` を呼びます）。
  - 列挙: `ref mut` を取る組み込み関数は `Vec.push_mut`・`Array.write`・`SyncMap.__replace`（値を格納する）、`Array.swap_in`・`Array.split_at_mut`・`Vec.__view`・`Simd.store`（格納する値がないか数値だけ）、コールバックへ `ref mut` を渡す `Mutex.with_lock`・`RwMutex.with_write`・`Cond.wait_with`・`Parallel.for_each_chunk`（コールバックの中の書き込みは外部の参照への格納で、借用を持つ値は以前から `store_region_unnamed`）です。std で `ref mut` を取る関数は `Array`（`set`・`fill`・`blit`・`sort_in_place*`・`random_shuffle_in_place*` と private の補助関数）、`Vec`（`copy_to*` と private の補助関数）、`Matrix`・`Regex`・`Tensor`（数値の要素だけ）、`MatrixView`（宣言した region の契約を持つので対象外）です。`HashMap`・`Map`・`Set`・`List`・`String` の更新は値で受けて返すので、結果がすべての引数の loan を持ち、対象外です。
  - `store_arguments` は、書き込み先がすでに持つ loan（その参照の参照先のスロット）を足しません。`store_through` は、外部の参照先へ loan のない値を書くときは何もしません（代入の `E::Assign` と同じ）。この 2 つで、`fn reset target = Array.write 0 (\() -> 0) target` が通り、以前は `store_region_unnamed` だった `Vec.push_mut (\() -> 1) v`（`v` は `ref mut` の引数）も通るようになりました。
  - 借用した一時値（`BorrowOperand`）の値は一時値が持つ loan そのものなので、参照先の loan としてそのまま扱います（`Array.blit 0 0 1 [\() -> ...] (ref mut arr)` の穴）。
- テスト: `tests/mutable_slices.rs` `in_place_writes_keep_the_borrows_that_they_store`（`Array.write`・スライス・`Array.set`・`Array.fill`・`Array.blit`（一時値も）・`Vec.copy_to`・`Vec.copy_to_at`・パイプライン・`ref mut` の束縛経由で `text` のムーブが `E1014`、`ref string` の要素、`stash` の 4 形と 1 つの関数の 4 形が `E1013`/`borrow_outlives_block`、利用者の関数の格納が `store_region_unnamed`、関数を要素に持つ std の書き込み関数とパイプラインの受理）、`tests/rc.rs` `a_share_of_an_element_holds_what_was_written_in`（読み通しの形）。
- 証拠（修正後）: `stash`/`leak` は `E1013`/`borrow_outlives_block`（9:5、基準と同じ）、読み通しの形は 7:5 で同じ。1 つの関数の穴は `E1014`/`borrow_conflict`。関数を要素に持つ `Array.set`・`fill`・`blit`・`random_shuffle_in_place_*`・`Vec.copy_to*`・`Vec.reverse`・`Vec.remove_all` を使うプログラムは受理され、`-O0`・`-O3` で基準と同じ 1024。
- 判断（旧 D3。3 回目の対応で取り下げ、穴は A31 に分けました）: 以前からの穴（基準でも受理していた解放後の読み出し）を塞ぎます。A28 の事実はこの追跡に依存するので、塞がなければ A28 が穴を広げます。代わりに次のプログラムが新たに拒否されます。どれも要素の型が借用を持ちうる（関数・参照）ときだけで、std・`examples/`・`tests/fixtures/` には現れません（手順 5）。
  - 利用者の関数が、自分の `ref mut` 引数の先へ、借用を持つ値を `Array.write` で書く（`fn put f target = Array.write 0 f target`）: `E1013`/`store_region_unnamed`。同じ関数を `target[0] = f` で書くと基準でも同じ診断です。
  - 関数を要素に持つ std の書き込み関数の部分適用・関数値: `E1013`/`region_callback_stores_exclusive_input`（`Vec.push_mut` と同じ）。
  - 関数を要素に持つ `Array.sort_in_place_with`・`sort_in_place_by`（と `Vec.sort_with`）は基準でも std の中で `E1013`/`E1014` で、診断が 1 つ増えます（`self_referential_store`）。
  - 宣言した region の契約を持つ `MatrixView.write`・`fill`・`copy_from`・`map_in_place` は契約を与え直さないので、関数を要素に持つと std の中で `E1013`（`store_region_unnamed`、`map_in_place` は `self_referential_store`）。基準では書き込んだ借用を追跡しないまま受理していました。数値の要素は変わりません。
  - パイプライン形は組み込み関数と std の書き込み関数だけです。名前付き region を宣言した利用者の書き込み関数は、以前どおり直接の完全適用だけです（`tests/borrowed_records.rs` の期待）。

#### 2. 主要: 所有者の借用が生きている間の share/downgrade の move が誤って `E1014`

- 再現: mv10・mv2・mv6・mv8・mv11（weak）・mv12（tuple）は基準で受理（5・5・5・4・4・5）、修正前の A28 で `E1014`/`move_reborrowed_exclusive_reference`。rt6 は kind が `task_capture_retains_borrow` から変わっていました。
- 原因: `read_through` が `ref shared` の loan の `parents` をそのまま結果に渡し、`Rc.get (ref shared)` などの生きている借用もそれを親に持つので、move の検査が再借用中の値の move とみなしました。
- 対応: `read_through` は選んだ loan の写し（同じ `Place`・可変性・`parents` の新しい loan）を返します。写しは同じ場所を借用するので、ブロックの終わり・競合・`check_result` の検査は原本と同じで、誰の親でもないので move を妨げません。同じ場所の再借用（`(ref shared) |> Arc.share` が作る `ref (deref (ref shared))`）は、親をさらにたどります。
- テスト: `tests/rc.rs` `a_new_owner_moves_while_the_owner_is_borrowed`（6 形を `Rc` と `Arc` で、パイプライン形を含めて受理、写しがムーブ・ブロックの終わりで原本と同じく拒否される 2 形）、`tests/tzroutine.rs` `a_tzroutine_captures_a_share_of_a_function`（rt6 とパイプライン形が `task_capture_retains_borrow`、借用しない関数のパイプライン形を受理）。
- 証拠: 6 つとも受理、`tsuzuri run` は `-O0`・`-O3` で基準と同じ 5・5・5・4・4・5。rt6・`arc_env_tz` の 2 形は `E1013`/`task_capture_retains_borrow`（基準と同じ位置）。

#### 3. 軽微: パイプライン形に事実と読み通しが効かない

- 対応: `x |> f a` が名前付き関数 `f` の呼び出しを完成させるとき（`piped_callee`）、`eval_piped_arguments` が `eval_call` と同じく引数ごとに事実で loan を選び、渡された値の loan は最後の引数の事実で選びます。`parameter_retention` はこの形も呼び出しとして集めます。右が読み通しの組み込み関数（`ref shared |> Arc.share`）なら左の値を読み通します。書き込む関数のパイプラインは 1 のとおり書き込みを記録します。
- テスト: `tests/currying.rs` `a_pipeline_that_completes_a_call_uses_its_facts`（`f |> pass (ref key)` の受理、保持する引数を左に置く 2 形の拒否）、2 の rc・tzroutine のパイプライン形。
- 証拠: pipe2（`f |> pass (ref key)`）は受理、`run` は 42（基準は `E1013`/`task_result_retains_borrow`）。pipe1（`ref shared |> Arc.share` を tzroutine が捕捉）は受理（基準は `task_capture_retains_borrow`）。

#### 4. 軽微: 推論の段の事実が型変数の値を通る流れを見落とす

- 対応: 抽象本文（`infer`）の事実は、`hidden_parameters` が求める引数を最初から保持にします。型変数を含み loan を運ばない型の式（局所値の読み出しを除く）の値が使う局所値を、束縛（`let`・`match` とパターン・`for`・代入）をたどった閉包で引数まで戻します。具体的な型で loan を運ばない式に丸ごと消費される値（`hash_of (ref map) key : i64` の中の `key`）は数えず、束縛・代入の値と、排他参照を引数に取る呼び出しの引数（抽象本文は書き込みを見ないので）は数えます。
- テスト: `tests/currying.rs` `inferred_facts_keep_what_a_type_variable_may_hold`（sneaky/g が `g` の呼び出しの位置で `E1005`/`no_instance`、`i64` の具体化は受理、同じ形の `Array.write` 版も呼び出しの位置で `E1005`）。
- 証拠: sneaky は基準と同じ 8:1 `E1005`/`no_instance`、受理する変種は基準と同じ（`run` は 5）。KG の事実は手順 5 の全 fixture・examples で基準と一致。`HashMap.remove_ref (ref key)` の後の `key` の move（string のキー）は受理のまま（最初の版の規則は粗すぎてこれを拒否したので、消費される式を除く形にしました）。

#### 5. 軽微: 予算の打ち切りが黙ってすべての事実を捨てる

- 対応: 予算が尽きたら、作業リストに残る関数と、その推移的な呼び出し元の事実だけを捨てます（すべて保持）。ほかの関数は呼び出す関数の今の事実の下で検査済みなので、その部分グラフの不動点として残ります。`parameter_retention_within` は予算を引数に取ります（単体テスト用）。
- テスト: `src/ownership.rs` `retention_tests::an_exhausted_budget_discards_only_the_unsettled_facts`（予算 0〜15 のどれでも、事実は完全な不動点と同じか `None`、不安定な再帰の組の呼び出し元は `None`、`a28_pass` だけが残る予算がある）、`tests/currying.rs` `an_exhausted_fixed_point_keeps_the_facts_that_settled`（201 関数・16 引数の環で、`HashMap.remove` を受理、環を外へ返す形は `task_result_retains_borrow`）。
- 証拠: 201 関数の環は受理（基準と修正前の A28 は `std/HashMap.tz:396:21` `E1013`）。
- 先送り: 事実を `events_abstract`・`copy_reads` で再計算しない共有は、`CheckedModule` に結果を持たせる変更になり単純でないので、していません（KG の段の増分は下の計測のとおり数 ms）。診断（`E1017` など）も足していません（打ち切りは保守的な側で、残る誤った拒否は打ち切られた関数とその呼び出し元に限られます）。

#### 6. 文書の指摘 3 件

- `keep` と部分適用: `fn keep text = \() -> ...` はラムダの引数も関数の引数になり、`keep (ref text)` は部分適用で、本体が使わなくても借用を持つ、と書き、`keep` の例を `let g = \() -> String.length text` と `g` の形にしました（`lambda-expressions.md`・`docs/language.md`）。
- 参照の結果と `closed_returns`: 本体が借用をまったく返さないと示せる関数の結果は借用を持たず、本体を変えると呼び出し側が壊れうる、と書き直しました（`lifetimes.md`・`lambda-expressions.md`・`docs/language.md`）。
- `Rc` と tzroutine: `Arc` の結果だけが tzroutine へ捕捉できる、と直しました（`docs/language.md`）。`rc.md`・`tzroutine.md` の記述は `Arc` だけを tzroutine に渡していて正しいままです。
- 書き込みの規則: `lifetimes.md` の新しい小節「可変スライスと排他参照への書き込み」、`borrowing.md` の可変スライスの API の後、`docs/language.md` の参照先のリージョンの段落、`docs/architecture.md` に書きました。パイプライン形も `lifetimes.md`・`lambda-expressions.md`・`rc.md`・`docs/language.md`・`docs/architecture.md` に書きました。

#### 手順 5 のやり直し（基準 = A30 のレビュー修正後のバイナリ）

- `tests/fixtures/*`（ディレクトリ 163）と `examples/*`（13）の `tsuzuri check --json`（終了コードを含む）と `tsuzuri build --no-cache --emit llvm`（151 件が IR を出力）を、A30 のブランチの木で基準と最後のバイナリで比べ、503 ファイルすべてが一致しました（途中の版で 3 件に一時ディレクトリの削除の警告が混じったのは並行実行のためで、再実行で一致）。KG の事実（`target/tz/kg/facts`）も 176 件すべてで一致しました。A28 が足した `hash_map`・`rc`・`tzroutine` の fixture（このブランチの木）は受理され、IR は 2 回の生成で一致します。
- 「拒否し続けるプログラム」は、すべて基準と同じ位置・code・kind です: `keep_leak`（`own string`）4:16 `task_result_retains_borrow`、`keep_leak`（読むだけの引数）5:14 `borrow_of_temporary`、`keep_tz` 5:1 `task_capture_retains_borrow`、`class_escape`（`own string`）9:16 `task_result_retains_borrow`、`class_escape`（ブロックの局所値）11:5 `borrow_outlives_block`、`arc_env_tz`（`mine`）5:1・（`shared`）4:1 `task_capture_retains_borrow`、`arc_env_return` 6:5・`rc_get_return` 3:50・`hashmap_env_return` 5:5・`mapr_leak` 4:5 `borrow_outlives_block`、`copy_out_tz` 7:1・`arc_get_env_tz` 5:1 `task_capture_retains_borrow`。「再現」の 2 つと「同じ原因の別の形」（`remove_ref` の後の tzroutine と move、`Rc.share` を返す、`Arc.share` の後の move、generic な `pass`）は受理のままです。
- 検査の時間（`TSUZURI_TIME_PASSES=1 tsuzuri check`、基準と交互に 15 回の中央値、A30 の木の fixture。ほかの worktree の作業と同時なのでゆれが大きい）: 全体は hash_map 211.1→220.1 ms、list_fsharp 267.2→277.0、array_fsharp 253.1→259.9、vec_dotnet 230.8→238.3、rc 198.0→209.2、tzroutine 166.5→169.8、examples/tzroutines 156.3→167.9（+2〜7 %、10 % を超えません）。`check` の段は +2〜3 ms、`ownership` は +0〜1 ms、`kg-capture` は +1.6〜2.1 ms（`events_abstract` が推論の段の事実を計算し直すため）。病的な環: 200 関数 325.6→450.6 ms、201 関数（予算が尽きる）330.0→837.6 ms（`ownership` 4.5→120.8、`kg-capture` 11.7→145.2）。予算の打ち切りは検査の回数を数えるので、尽きるまでの検査（推論の段・最後の段・`events_abstract` の 3 回）は残ります。

#### 実行したコマンド

- `cargo test --locked --no-fail-fast --test currying --test rc --test tzroutine --test mutable_slices` を修正前の木（`39366e2e` に新しいテストのファイルだけを入れたもの）で: 新しい 7 件（`a_pipeline_that_completes_a_call_uses_its_facts`・`an_exhausted_fixed_point_keeps_the_facts_that_settled`・`inferred_facts_keep_what_a_type_variable_may_hold`・`in_place_writes_keep_the_borrows_that_they_store`・`a_new_owner_moves_while_the_owner_is_borrowed`・`a_share_of_an_element_holds_what_was_written_in`・`a_tzroutine_captures_a_share_of_a_function`）だけが失敗し、ほかは成功（currying 11/14、rc 12/14、tzroutine 21/22、mutable_slices 9/10）。単体テスト `an_exhausted_budget_discards_only_the_unsettled_facts` は修正前にない `parameter_retention_within` を呼ぶので、修正前には置けません。
- 修正後: `--test currying` 14/14、`--test rc` 14/14、`--test tzroutine` 22/22、`--test mutable_slices` 10/10、`--test hash_map` 6/6、`--test maybe_result` 9/9、`--test types_ownership` 9/9、`--test borrowed_records` 21/21。`cargo test --locked --lib retention_tests`: 2 passed。
- `cargo nextest run --locked`: 修正の途中で 2950 passed、1 failed（`borrowed_records::named_target_regions_keep_stored_borrows_alive`: 名前付き region を宣言した利用者の書き込み関数のパイプライン形が受理に変わっていたので、パイプライン形を組み込み関数と std の関数に限りました）。最後は 2951 passed、4 skipped（新しいテスト 9 件を含む）。
- `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。
- `node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`tzroutine`: 16、`parallel`: 36、`borrowed_records`: 57（native/WASM、-O0/-O3。tzroutine は native）。`node tests/tzroutine.mjs target/debug/tsuzuri`: 163 runs。`node scripts/conformance.mjs target/debug/tsuzuri --area OWN`: 30 ok、`--area TSK`: 21 ok。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（lambda-expressions.md・lifetimes.md・rc.md・borrowing.md・tzroutine.md）: 5 pages、142 links、56 examples、106 native runs ok（lifetimes.md は最後の変更の後にもう一度: 1 page、11 links、6 examples、12 native runs ok）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks、0 stale。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`: 4 checks、0 failed。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。

#### 2 回目の対応（検証者の判定の新しい問題 2 件と、前回の懸念 1 件）

（この回の派生の契約も 3 回目の対応で取り下げました。）

先に `fix/a30-owned-function-borrow` の最新（`36196cd6`）をマージし、A30 のバイナリを作り直して基準にしました。前回の修正（D3）が新たに拒否していた、健全なプログラムを受理に戻しました。証拠のプログラムは `target/scratch/r2/`（作業の後に削除）に書き、基準と、この修正の前のブランチで再現してから直しました。

- 主要: 参照を要素に持つ `Array.set`・`Array.fill` が std の中で `E1013`/`store_region_mismatch`。原因は、`derive_store_slots` が、宣言で型変数そのものだった値の引数（`own 'a`）が `ref i64` に具体化されると、参照先が loan を運ばない参照として region を与えなかったことです。特殊化が宣言の引数の型と型引数を `CheckedFunction.instance` に残すようにし、宣言で型変数だった引数は、具体化が共有参照でも書き込みうる値（region 0）にしました。`Array.set 0 (ref b)`・関数の中・`ref string`・パイプライン・`Array.fill` は受理され、値は基準と同じです（2・322・132・7・33）。`b` のムーブ・代入は `E1014`/`borrow_conflict`、`b` が先に解放されるブロックでの書き込みは `E1013`/`assignment_extends_borrow` です（基準は受理し、解放後の値を読みました）。
- 軽微: generic な利用者の書き込み関数（`def put :: own 'a -> ref mut ['a..] -> unit`）が、関数・参照の要素に具体化されると定義の位置で `E1013`/`store_region_unnamed`。std と同じく型変数に region を書けないので、利用者の関数の具体化のうち、型変数が loan を運ぶ型になり、型変数を含む宣言の `ref mut` 引数を持ち、本文が書き込む（`writes` を持つ関数の直接呼び出しか、局所値でない代入。利用者の関数どうしは不動点まで）ものに同じ契約を与えました（`RegionSlots.derived`）。本文はその契約で検査され、呼び出し側は書き込んだ借用を持ち主に残すので健全です。パイプライン形も許します。`put` の関数・参照・`i64` の具体化、`put` を呼ぶ generic な関数、`ref mut 'a` へ `deref r = v` で書く関数（基準は `store_region_unnamed`）は受理され、借用する値の書き込みの後のムーブは `E1014`、局所値の借用を返す形は `E1013`/`borrow_outlives_block`、部分適用は `region_callback_stores_exclusive_input` です。
- 前回の懸念: `MatrixView.write`・`fill`・`copy_from`・`map_in_place` の関数・参照の要素が std の中で `E1013`。借用を持たない値でも拒否していたので回帰でした。宣言した region が書き込みも結果の region も持たず、型変数が loan を運ぶ型に具体化され、排他借用を内に持つ参照先（`Mut`）への `ref mut` を持つ std の関数にも、派生の契約を与えました。派生の契約で参照先が排他借用を内に持つ `ref mut` 引数は、参照先のスロットも可変にし、その先の借用（窓の配列の要素）も参照先の region を持ちます（`external_loans`）。呼び出し側では書き込みが局所値の窓 `grid` に入るので、`store_through` が、局所値の所有者が持つ排他借用の場所（`data`）へも同じ loan を格納するようにしました。これがないと、窓を使い終えた後に借用元をムーブして `data[1] ()` を呼べました（修正の途中で確かめた穴）。4 つの書き込み関数とパイプライン形は、借用を持たない値と、配列より長生きする借用で受理され、値は基準と同じです（4201・55・67・33・21）。借用元のムーブは `E1014`/`borrow_conflict`、局所値の借用を返す形は `E1013`/`borrow_outlives_block`、部分適用は `region_callback_stores_exclusive_input` です。
- 同じ修正で見つけた回帰: 書き込み先から読んだ要素を同じ先へ書き戻す（`let second = target[1]` の後の `Array.write 0 second target`）のが `E1013`/`self_referential_store` でした（`map_in_place` の本文と、型変数を持たない利用者の関数）。`store_arguments` は、書き込み先がすでに持つ loan（`own_contents`: 参照先の loan と、参照先の region のない外部の引数自身の loan。排他的な引数そのものは写せず、新しい借用は別の loan なので、読み出した値だけがそれを持ちます）を格納する値から除いてから自己参照を検査し、格納します。基準と同じく受理し、値も同じ（2）です。
- テスト: `tests/mutable_slices.rs` `derived_contracts_store_references_and_functions_of_type_variables`（受理 12 形と拒否 18 形。この修正の前の `src/` で失敗します）。`tests/fixtures/mutable_slices` に `reference_writes`・`generic_writes`・`matrix_writes` を足し、`tests/features.mjs` の期待値を手で計算しました（233723・91052425・20200076872）。基準のバイナリでも同じ fixture が同じ値で通ります（49 cases）。
- 手順 5（基準 = 作り直した A30 のバイナリ）: A30 の木の `tests/fixtures/*`（163）と `examples/*`（13）の `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（151 件）は、327 ファイルすべて一致しました。このブランチの木では、A28 が足した `hash_map`・`rc`・`tzroutine` の fixture だけが基準で拒否され、ほかの 324 ファイル（新しい関数を足した `mutable_slices` を含む）は一致し、IR は 2 回の生成で一致します。「拒否し続けるプログラム」13 形（`keep_leak` 2 形、`keep_tz`、`class_escape` 2 形、`arc_env_tz` 2 形、`arc_env_return`、`rc_get_return`、`hashmap_env_return`、`mapr_leak`、`copy_out_tz`、`arc_get_env_tz`）は、すべて基準と同じ位置・code・kind です。
- 実行したコマンド: `cargo nextest run --locked`: 2952 passed、4 skipped。`cargo nextest run --locked --test mutable_slices --test rc --test currying --test tzroutine --test borrowed_records --test hash_map --test matrix_views --test matrix --test slices`: 105 passed。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`node tests/features.mjs target/debug/tsuzuri` の `mutable_slices` 49・`matrix_view` 303・`borrowed_records` 57・`rc` 39・`hash_map` 199・`parallel` 36・`tensor` 34 cases（native/WASM、-O0/-O3）。`node scripts/conformance.mjs target/debug/tsuzuri --area OWN`: 30 ok。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`Array.set`・`Array.fill`・`Array.write`・`MatrixView` を使う 18 ページ）: 584 links、143 examples、278 native runs ok。`gen-docs --check`: 19 blocks、0 stale。`check-cli-docs`: 0 failed。`check-guide-lite`: 0 failed。
- 文書: `lifetimes.md`（可変スライスと排他参照への書き込み: 参照の要素、`MatrixView`、書き戻し、generic な書き込み関数の `run=9` の例、型変数を持たない関数の制限）、`matrix-view.md`（借用規則と診断の表に 2 行）、`docs/language.md`、`docs/architecture.md`、D3 の代償の一覧。

#### 3 回目の対応（書き込みの追跡を取り下げ、排他借用を渡す本体を以前の規則に戻す）

- 判断（オーケストレーターの判断、2026-11-06）: 1〜2 回目の対応（旧 D3: 書き込む関数への region の契約と、その派生）は、どの回も基準が受理する安全なプログラムを拒否しました（1 回目は参照を要素に持つ `Array.set`・`Array.fill`、2 回目は region を宣言した generic な書き込み関数、`ref 'a -> ref mut [ref 'a..]` の書き込み関数、`MatrixView` の包み関数。`fix2-results.json`）。旧 D3 が塞ぐ穴は main にもあり、A28 の承認は誤った拒否を受理に変えることだけで、基準が受理するプログラムを変えることは含みません。そこで旧 D3 を全部取り下げ、穴は [A31](../A31-in-place-writes-keep-stored-borrows.md) に起票し、A28 には穴を広げないための保守的な除外（新しい D3）だけを入れました。
- 取り下げたもの（`29af0335`）: `closures::derive_store_slots`、`Array.write`・`SyncMap.__replace` の生成関数の `RegionSlots`、`CheckedFunction.instance`・`check::Instance`、`RegionSlots.derived`、`regions::validate_contract_calls` のパイプラインの規則、`piped_store`・`eval_piped_store`、`store_through` の変更（借用を持たない値の早期の戻り、窓の後ろの持ち主への格納）、`input_slot_loans` の一時値の枝、`store_arguments` の `own_contents`、`external_loans` の `deep`、テスト `in_place_writes_keep_the_borrows_that_they_store`・`derived_contracts_store_references_and_functions_of_type_variables`、`lifetimes.md` の小節「可変スライスと排他参照への書き込み」と、`borrowing.md`・`matrix-view.md`・`docs/language.md`・`docs/architecture.md` の該当の段落。`git diff fix/a30-owned-function-borrow...HEAD` の `src/` は、`src/check.rs` の `Type::contains_unknown` と `src/ownership.rs` の A28 の部分だけで、`src/closures.rs`・`src/closures_rec.rs`・`src/polymorph.rs`・`src/regions.rs`・`std/` は A30 の先端と同じです。`store_through`・`store_arguments`・`input_slot_loans`・`target_loans`・`external_loans` の本文も A30 の先端と同じです。
- 残したもの: 読み通しの写し（誤った `E1014` の修正）、パイプラインの事実と読み通し、推論の段の型変数の規則（`hidden_parameters`）、予算の打ち切りの局所化、文書の修正（`keep` と部分適用、`closed_returns`、`Rc` と `Arc`）。
- 除外（`src/ownership.rs` の `lends_exclusive`・`lending_bodies`、`CallFacts.lending`）: 本体が、排他参照に届く型（`reaches_exclusive` か `holds_exclusive_record`）の引数を呼び出し（`Call`・`HostCall`・`Parallel`・クロージャの捕捉・パイプラインの左辺）へ渡す関数では、`retained_parameters` が `None` を返し（結果はすべての引数の借用を持つ）、`Rc`／`Arc` の `share`・`downgrade`・`upgrade` の読み通しもしません。`lending_bodies` は各本文を 1 回だけ走査し、`E::Closure` をたどって、その本文から持ち上げたラムダ（ラムダの中のラムダ、`let rec` の組）にも伝えます。各本文の検査は `CallFacts::contract` が作る `Contract.lends_exclusive` でそれを受け取り、`Checker::body_lends_exclusive` が読みます（持ち上げる前の `E::Lambda` は囲む本文の値を受け継ぎ、契約が言わない本文は自分の本文を走査します）。特殊化した本文と抽象本文の両方に効き、抽象本文では型変数を排他参照と数えません（その事実は Copy 制約の推論だけに使い、具体化は自分の事実で検査されます）。
- 健全な理由: 所有権検査が見落とす格納は、`Array.write`（とそれで書き込む std の関数）と `SyncMap.__replace`（std の非公開の基本関数）だけで、どれも書き込み先を排他借用の引数で受け取ります。呼び出し元が貸す場所へ書き込む関数（コールバック、std の関数）も排他借用でしか受け取れず、そうした callee は自分の本文が排他借用を渡すので事実を持ちません。ほかの格納（代入、`deref r = v`、`Vec.push_mut`、region の契約を持つ関数）は追跡されます。したがって排他借用を呼び出しへ渡さない本文では、局所値の loan は完全で、結果の `Value` の loan も完全です。事実は callee の本文だけから決まり（引数の外部 loan は、呼び出し元の値が実際に持つものすべてを表します）、呼び出し元の値の loan が不完全でも、callee が結果に入れない引数は入れないので、事実の使用は呼び出し元の完全さに依りません。読み通しだけは、借用した場所の値が持つ loan を結果に渡すので、その値の loan が完全である本文でだけ使います（4 回目の対応で訂正: 本文が排他借用を渡さなくても、借用した場所の値が引数から来たもの（外部 loan を持つ）なら完全とは限らないので、その場合は読み通しません）。ラムダの捕捉の loan は、ラムダを作った本文での値の loan なので、その本文が排他借用を渡せば不完全になりえます。そのため除外はラムダへ伝えます。渡す本文は A28 より前の規則（基準と同じ）で検査されます。
- 3 回目の検証で見つけた穴（修正済み、`eea6dbfc`）: 最初の除外（`29af0335`）は、検査する本文だけを見ていました。`Array.write 0 (Arc.new (\() -> String.length borrowed)) (ref mut arr)` の後の `\() -> Arc.share (ref arr[0])` を返す関数は、持ち上げたラムダの本文が読み通すので受理され（基準は `Main.tz:7:6` `E1013`/`task_result_retains_borrow`）、実行すると `-O0` で `4301672048`（期待は 9）を出力しました。`lending_bodies` で除外をラムダへ伝え、基準と同じ位置・kind に戻しました。パイプライン形・入れ子のラムダ・`Arc.upgrade (ref (Arc.downgrade (ref arr[0])))`・tzroutine の形も基準と同じです。書き込みのない形（`let arr = [Arc.new (\() -> n)]` を捕捉するラムダの `Arc.share`）は、基準の誤った拒否（`task_result_retains_borrow`）が受理に変わり、`123456789` を出力します。A31 の「落とし穴」に、除外を外すときの注意として書きました。
- A30 のマージ: `01185ea4`（借用を `Owned.Function` の中にだけ持ちうる引数は呼び出し元の loan を受け取らない。`4e7ae226`）と `f957a7d9`（`Owned.function` の例外を `$builtin` の生成関数の引数に限る `Contract.builtin_wrapper`・`is_builtin_wrapper_parameter`、`carries_loans_beside_owned_functions` の 1 回の走査。`2c55a773`）。`check_body`・`start_checker`・`Contract` の衝突は両方の振る舞いを残して解きました（`start_checker` が `builtin_wrapper` と `lends_exclusive` の両方を `Checker` に置きます）。`parameter_retention` の初期値は A30 の規則に合わせ、借用を持ちうる（`carries_loans` か `contains_unknown`）のに loan を受け取らない（`carries_loans_beside_owned_functions` が偽）引数を最初から保持にします。借用を持ちうるのに loan を受け取らない引数が「一時」から始まることはありません。A30 のバイナリ（`f957a7d9`）を作り直して基準にしました。
- テスト（先に書き、除外を無効にした版で失敗することを確かめました）: `tests/mutable_slices.rs` `a_function_that_writes_in_place_keeps_its_arguments_loans`（レビューの `stash` を `Array.write`・`Array.set`・`Array.fill`・スライスへの `Array.write`・パイプライン・一時値からの `Array.blit` で書いた 6 形が `E1013`/`borrow_outlives_block`、`text` が生きている間の呼び出しは受理、書き込む関数へ渡すだけの関数とコールバックの形も拒否）、`in_place_writes_that_main_accepts_stay_accepted`（1〜2 回目が拒否した 12 形の受理）、`tests/rc.rs` `a_share_of_an_element_holds_what_was_written_in`（呼び出し形とパイプライン形、ラムダの 3 形が `task_result_retains_borrow`、書き込みのないラムダの受理。ラムダの形は修正の前のバイナリで受理されることを確かめました）、`src/ownership.rs` `retention_tests::a_body_that_lends_an_exclusive_borrow_has_no_facts`。`tests/fixtures/mutable_slices` の `reference_writes`・`generic_writes`・`matrix_writes` は、基準も受理する形の回帰として残しました。
- 証拠（基準 = A30 の先端 `f957a7d9` のバイナリ、`check --json` と `run -O0`/`-O3`。プログラムは `target/scratch/a28-r3/`・`a28-r4/`、作業の後に削除）:
  - レビューの重大な指摘のプログラム（上の 6 形と、`arr[0] = ...` の形、`Rc.share (ref arr[0])` の呼び出し形・パイプライン形）は、基準と同じ `Main.tz:9:5`・`7:5` の `E1013`/`borrow_outlives_block`。ラムダの 5 形も基準と同じです。
  - 1〜2 回目が壊したプログラム 37 個（参照の要素の `Array.set`・`Array.fill`・パイプライン・関数の中の形、generic な書き込み関数（`{r}` の形、`ref 'a -> ref mut [ref 'a..]`、`{s}` の形、2 つの型変数、ラムダの本体、入れ子、`twice`、書き戻し）、型変数を持たない書き込み関数、借用を持たない値の書き込み、`MatrixView` の std の書き込み関数と包み関数（関数・参照の要素、region あり・なし）、部分適用、sort の 2 つ、`set_ref`、generic な `Vec.push_mut`）は、診断（位置・code・kind）が基準と同じで、受理される 33 個は `-O0`・`-O3` で基準と同じ値です（322・9・2・2・6・11・4・5・7・323・2・3・2008・4・4・4・4・2・1009・2・4・4・4・2・3・6・1・2・2・2・3・4・11）。
  - 書き込みの形を変えた検査（コールバックで書く、書く関数へ渡すだけ、`ref mut` を入れたタプル、`ref mut` の局所値への添字の代入、`for` と `Parallel.for_each_chunk`、関数の中の書き込みの後の `Rc.share`、callee が書いた要素の `Rc.share`）も、すべて基準と同じ診断です。
  - A31 の「再現」（1 つの関数の中の穴）は、基準でもこのブランチでも受理され、同じく解放後の値を出力します（`h1`〜`h5`・`h7` は `-O0` で不定の値、`-O3` で `0`。`h9` は `0`）。A28 はこれを変えません。
- 手順 5（基準 = A30 の先端 `f957a7d9`）: A30 の木の `tests/fixtures/*` と `examples/*`（176 個）の `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（151 件が IR を出力）は 503 ファイルすべて一致。このブランチの木では、A28 が関数を足した `hash_map`・`rc`・`tzroutine` の 3 つの check と build だけが違い（基準は `Main.tz:344:17` `E1014`/`borrow_conflict`、`383:17` `E1014`・`392:5` `E1013`/`borrow_outlives_block`、`138:9` `E1013`/`task_capture_retains_borrow` で拒否、このブランチは受理）、ほかの 497 ファイル（`mutable_slices` を含む）は一致。IR は 2 回の生成で 151 件すべて一致。
- 拒否し続けるプログラム: すべて基準と同じ位置・code・kind です（`keep_leak` 4:16 `task_result_retains_borrow`・読むだけの引数 5:14 `borrow_of_temporary`、`keep_tz` 5:1 `task_capture_retains_borrow`、`class_escape`（`Traits.tt` を置いた 2 ファイルの project）の `own string` 15:16 `task_result_retains_borrow`・ブロックの局所値 17:5 `borrow_outlives_block`、`arc_env_tz` 5:1・4:1 `task_capture_retains_borrow`、`arc_env_return` 6:5 `borrow_outlives_block`、`rc_get_return` 2:41、`hashmap_env_return` 5:5、`mapr_leak` 4:5 `borrow_outlives_block`、`copy_out_tz` 7:1・`arc_get_env_tz` 5:1 `task_capture_retains_borrow`）。
- 再現（native の `-O0`・`-O3`。基準はどれも拒否）: `HashMap.remove` の再現は受理され `0`、関数の `Arc.share` を tzroutine が捕捉する再現は `0`、`Rc.share (ref shared)` を返す形は `7`、`Arc.share` の後に `shared` をムーブする形は `43`、generic な `pass` の `wrap` と、パイプラインの `wrap` は `42`、`class_escape` の `Counter` の具体化は受理。
- 実行したコマンド: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`cargo nextest run --locked`: 2952 passed、1 failed、4 skipped。失敗した `diagram_cli::kg_diagram_json_line` は、同時に走らせていた手順 5 のスクリプトがこの木の `tests/fixtures/*` を検査していたためで（KG の出力を共有します）、単独で 3 回続けて実行すると 12/12 で成功します。`cargo nextest run --locked --test rc --test mutable_slices --test currying --test tzroutine --test hash_map`: 67 passed。`--lib retention`: 3 passed。`node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`mutable_slices`: 49（native/WASM、-O0/-O3）、`tzroutine`: 16（native）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`tzroutine.md`・`rc.md`・`lifetimes.md`・`lambda-expressions.md`）: 4 pages、130 links、47 examples、88 native runs ok。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。
- 文書: `lifetimes.md` の「現在の制限」に、除外の文（ラムダを含む）と既知の不具合の注意（A31 へのリンク、1 つの関数の中の再現）、`lambda-expressions.md`・`rc.md` に除外の文、`docs/language.md` の「関数の結果が持つ借用」の箇条（除外とラムダ、借用を `Owned.Function` の中にだけ持ちうる引数）と `Rc / Arc` の「結果の借用」の但し書き、排他借用フィールドの節の既知の不具合の段落、`docs/architecture.md` の `lending_bodies` と初期値の段落。`_features/README.md` の A28 の行の要約と、A31 の行。

#### 4 回目の対応（読み通しを本文の局所値の中身に限る）

- 重大（検証者の判定の新しい問題 1）: 読み通しが、引数の中身を読む helper を通して A31 の穴を広げていました。`def wrap :: own [Arc<unit -> i64>] -> Arc<unit -> i64>`、`fn wrap arr = Arc.share (ref arr[0])` を、`arr` へ `Array.write 0 (Arc.new (\() -> String.length borrowed)) (ref mut arr)` で書き込んだ `make` が呼んで返すと、`c2717abc` は受理し、`-O0` で `4341141376`（期待は 9）を出力しました（A30 の先端は `Main.tz:2:15` `E1013`/`task_result_retains_borrow`）。`wrap` 自身は排他借用を渡さないので 3 回目の除外が効かず、`ref arr[0]` の親である `arr` の外部 loan を読み通し、`make` の結果は `arr` の loan（書き込む前のもの）だけを持ちました。`own` の引数の外部 loan は呼び出し元の値の loan を表し、排他借用を渡す呼び出し元ではそれが不完全でありうるからです。
- 判断（オーケストレーターの判断、2026-11-06）と実装（`18635c2d`）: 読み通しは、読む loan が本文の局所値に根を持ち、その中身も本文のものであるときだけにし、外から来た根（引数、持ち上げたラムダの捕捉）のときは A30 の先端と同じく loan 自身を保ちます。実装では、判断の文言（読む loan の根が引数か捕捉か）より少し広く、第 1 引数の loan ごとに、読み通した先に外部ルートの loan が 1 つでもあれば（その loan 自身が外部ルートのときを除く）原本を渡します（`Checker::read_through`・`read_through_loan`）。根だけを見ると、`let a = arr` の後の `Arc.share (ref a[0])` は `a` が局所値なので読み通し、`c2717abc` と同じく `-O0` で `4337913976` を出力したからです（A30 の先端は `4:5` `E1013`/`borrow_outlives_block`）。保った loan は原本で、写しにしません（A30 の先端と同じ）。健全な理由はコードのコメント（`read_through`・`lends_exclusive`）に書きました: 既存の除外で本文は排他借用を渡さないので、本文の局所値が本文の中で得た loan は完全です。外部 loan は呼び出し元の値が持つものを表し、その loan は不完全でありえます。事実（結果に届かない引数を除く）は呼び出し元の完全さに依らないので変えていません。`ref string` の引数を捕捉した関数を局所の `Arc` に入れて共有する形（外部 loan が借用そのものを表すので読み通しても健全）も読み通さなくなりますが、A30 の先端も拒否する形で、受理を基準より狭めはしません。
- 拒否に戻った形（`tests/rc.rs` `a_share_of_an_element_holds_what_was_written_in`、どれも A30 の先端と同じ code・kind・位置）: `wrap` の `Arc` 形 2:15、`Rc` 形 2:15（`c2717abc` は `-O0` で `4366143260`）、helper のパイプライン `ref arr[0] |> Arc.share` 2:15、呼び出し側のパイプライン `arr |> wrap` 2:15、generic な `def first_share :: own [Arc<'a>] -> Arc<'a>` 2:22、レコードの引数 `fn wrap bag = Arc.share (ref bag.items[0])` 3:15、引数を捕捉するラムダ `fn wrap arr = \() -> Arc.share (ref arr[0])` 2:16、`own Arc<[Arc<unit -> i64>]>` の引数そのものの `Arc.share (ref shared)` 2:18（すべて `E1013`/`task_result_retains_borrow`）、`let a = arr` を経た形 4:5 `E1013`/`borrow_outlives_block`。どれも `c2717abc` は受理し、`-O0` で不定の値を出力しました。`Arc.downgrade (ref arr[0])` を `weak` に束ね `Arc.upgrade (ref weak)` を返す形は、両方が拒否しますが、A30 の先端は `4:5` `borrow_outlives_block`（`weak` の借用）、このブランチは `3:5` `task_result_retains_borrow`（`weak` は `wrap` の局所値なので読み通し、`weak` が保った `arr` の借用を持つ）です。
- 基準の誤った拒否に戻った形: 書き込みのない `wrap [Arc.new (\() -> 41)]`（`c2717abc` は `41` を出力、今は 2:15 `task_result_retains_borrow`）と、3 回目に受理に変えた、書き込みのない `let arr = [Arc.new (\() -> n)]` を捕捉するラムダの `Arc.share (ref arr[0])`（4:6 `task_result_retains_borrow`、A30 の先端と同じ）。`tests/rc.rs` の後者の `emits` は `rejects_with_kind` に変えました。関数の `own` の引数を共有して tzroutine へ渡す形（`fn spawn shared total = let mine = Arc.share (ref shared) ...`）と、引数を共有して返す `fn again shared = Rc.share (ref shared)` も、A30 の先端と同じ `task_capture_retains_borrow`・`task_result_retains_borrow` です（`rc.md`・`tzroutine.md` に書きました）。
- `lending_bodies` がラムダへ除外を伝える処理は残しました。捕捉は持ち上げた関数の引数なので、読み通しには今は効きませんが、ラムダの事実を落とす保守的な働きは 3 回目と同じです。
- 軽微（問題 2）: A31 の「再現」に、`Mutex.with_lock`・`RwMutex.with_write` のコールバックが `ref mut` の引数へ `Array.write` で書き、`into_inner` で取り出す形（A30 の先端で `-O0` が `0`、`-O3` が `9`。lock を返す形は `1`/`9`）と、`Array.write` で書いた配列の要素を `Arc.share` する tzroutine の形（`0`/`9`）を、プログラムと A30 の先端・このブランチで測った値とともに足しました。「対象外」の lock のコールバックの項目は、コールバックの書き込みを lock の持ち主へ届けることを対象とする文に変えました。`h6` は、書いてあるとおりのプログラムが `0`/`0`（解放後の読み出し）を出力するので、プログラムを載せて直し、`moved.length` を足す形の `9` も `0 * 100 + 9` であることを書きました（`1bb51596`）。同じ本文の中で lock のコールバックの書き込みの後に共有・事実を使う形（`Mutex.into_inner m` の要素の `Arc.share` を返す、`stash` が `arr[0]` を返し `leak` が返す）は、両方とも A30 の先端と同じ `8:5`・`10:5` `E1013`/`borrow_outlives_block` です。
- 小さな指摘（問題 3）: 読み通しで最初の誤りがなくなると、拒否されるプログラムの最初の診断が変わることがあります。`let shared = Arc.new (SyncMap.create ())`、`SyncMap.store (Arc.get (ref shared)) 0 (\() -> String.length borrowed)` の後に `Arc.share (ref shared)` を返す関数は、A30 の先端では `Main.tz:7:5` `E1013`/`borrow_outlives_block` だけですが、このブランチでは Main の誤りがなく、`std/SyncMap.tz:62:13` `self_referential_store`・`E1014`、`142:43` `mutex_result_retains_borrow` です（関数を値に持つ `SyncMap` のほかのプログラムでは A30 の先端も同じ std の診断を出します）。どちらも拒否します。
- 証拠（基準 = A30 の先端 `f957a7d9` のバイナリ、`check --json` と `run -O0`/`-O3`。プログラムは `target/scratch/a28-r5/`、作業の後に削除）: 「拒否し続けるプログラム」（`keep_leak` 4:16 `task_result_retains_borrow`・読むだけの引数 5:14 `borrow_of_temporary`、`keep_tz` 5:1、`arc_env_tz` 5:1・4:1、`copy_out_tz` 7:1、`arc_get_env_tz` 5:1（どれも `task_capture_retains_borrow`）、`class_escape` 15:16 `task_result_retains_borrow`・17:5 `borrow_outlives_block`、`arc_env_return` 6:5、`rc_get_return` 4:5、`hashmap_env_return` 5:5、`mapr_leak` 4:5（どれも `borrow_outlives_block`））はすべて基準と同じ位置・code・kind。1 回目の重大な指摘の `stash`/`leak` の 7 形（`Array.write`・`Array.set`・`Array.fill`・スライス・パイプライン・`Array.blit`・`arr[0] =`）は 9:5、`Rc.share (ref arr[0])` の呼び出し形とパイプライン形は 7:5（`borrow_outlives_block`）、ラムダの 3 形は 7:6・7:6・7:13（`task_result_retains_borrow`）で基準と同じ。A28 の再現は受理のまま（`HashMap.remove` `0`、tzroutine の `Arc.share` `0`、`Rc.share` を返す `7`、`Arc.share` の後のムーブ `43`、`pass`/generic な `pass`/パイプラインの `wrap` `42`、`remove_ref` の後の tzroutine `0` と文字列の鍵のムーブ `0`、`class_escape` の `Counter` の具体化 `1`。`-O0`・`-O3` で同じ）。A31 の `h1`〜`h4` は両方で受理され、`-O0` で不定の値、`-O3` で `0`。
- 手順 5（基準 = A30 の先端 `f957a7d9`）: A30 の木の `tests/fixtures/*` と `examples/*`（176 個）の `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（151 件が IR を出力）は、両方のバイナリで 503 ファイルすべて一致。このブランチの木では、A28 が関数を足した `hash_map`・`rc`・`tzroutine` の check と build だけが違い（基準は `Main.tz:344:17` `E1014`/`borrow_conflict`、`383:17` `E1014`、`138:9` `E1013`/`task_capture_retains_borrow` で拒否、このブランチは受理）、ほかは一致。このブランチの IR は 2 回の生成で 503 ファイルすべて一致。
- 実行したコマンド: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`cargo nextest run --locked`: 2953 passed（8 slow）、4 skipped。`cargo nextest run --locked --test rc --test mutable_slices --test currying --test tzroutine --test hash_map --test borrowed_records --test maybe_result --test types_ownership`: 106 passed。`--lib retention`: 3 passed。`node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`tzroutine`: 16（native）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`rc.md`・`tzroutine.md`・`lifetimes.md`・`lambda-expressions.md`）: 4 pages、130 links、47 examples、88 native runs ok。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。テストの変更は、修正を無効にした版で `a_share_of_an_element_holds_what_was_written_in` が失敗することを確かめました。
- 文書: `rc.md`（所有者を増やした値の借用: 結果が `shared` の借用を持つ 2 つの場合と `again` の例）、`tzroutine.md`（引数の `Arc` の共有は捕捉できない）、`lifetimes.md`（現在の制限）、`docs/language.md`（関数の結果が持つ借用の 2 つの箇条と `Rc / Arc` の結果の借用）、`docs/architecture.md`（読み通しと除外の段落）、このチケットの仕様・D3・3 回目の健全な理由の訂正。

#### 5 回目の対応（除外を、書き込む関数の列挙に依らない形に広げる）

- 重大（検証者の判定の新しい問題 1）: 除外の前提（書き込む関数はどれも書き込み先を排他参照の引数で受け取る）が、lock の組み込み関数で崩れていました。`Mutex.with_lock :: ref Mutex<'a> -> (ref mut 'a -> 'b) -> 'b` は lock を共有参照で受け取り、中身の `ref mut` をコールバックへ貸します。コールバックが関数値（部分適用 `put text`、`put :: ref string -> ref mut [(unit -> i64)] -> unit`）なら、`reaches_exclusive` は関数型をたどらないので、`stash` の本体は除外されませんでした。`Mutex.with_lock (ref m) (put text)` の後に `Mutex.into_inner m` の要素を返す `stash` を、局所の `text` を貸して呼ぶ `leak` は、A30 の先端では `Main.tz:13:5` `E1013`/`borrow_outlives_block`、`d18ede5c` は受理して `-O0` で `2`（期待は 11、解放した `text` を読む）を出力しました。`RwMutex.with_write`、`Array.set`、呼び出し側のパイプライン、helper `fn fill_lock text m = Mutex.with_lock m (put text)` も同じ（15:5）。読み通しの形（`Mutex.with_lock (ref m) (put (ref text))` の後の `Arc.share (ref arr[0])` を返す `make`）は A30 の先端の `9:5` に対して受理して `-O0` で `4304867260`、helper の形は `11:5` に対して同じく、tzroutine の形は `16:5` `task_capture_retains_borrow` に対して受理して `0`/`9` でした。
- 判断（オーケストレーターの判断、2026-11-06）と実装（`04ab20fb`）: 除外の定義を、書き込む関数の列挙に依らない保守的な形に広げました。(a) 呼び出しの引数・パイプラインの左辺・ほかの二項演算子の被演算子・クロージャの捕捉の型が、格納された値・参照先・`Rc`／`Arc` とその弱参照の中身・関数型の引数と結果の型（tzroutine の本体の結果も）をたどって、排他参照か可変セルに届けば、本体は書き込める場所を渡します（`Type::lends_mutable`、`src/check.rs`）。可変セル（`Type::is_mutable_cell`）は、`src/check.rs` の `Builtin` と `std/` を調べて、builtin が共有参照を通して中身を書く std の型です: `Mutex`（`with_lock`、`Cond.wait_with`）、`RwMutex`（`with_write`）、`OnceValue`（`get`）、`Atomic`（`store` など）、`SyncMap`（`store` など。中の `RwMutex`）、`Channel`・`SendOnly`・`ReceiveOnly`・`DataChannel.Sender`・`DataChannel.Receiver`（送った値を受信が返す）、`ErrGroup`（`Channel` と `Mutex` を持つ）。型引数が loan を運ばないセル（`Atomic<i64>`、`Mutex<i64>`）は借用を隠せないので数えません。抽象本文では、型変数を排他参照と数えない既存の規則と同じく、型変数をセルの中身の借用と数えません。`Rc`・`Arc` には書き込む builtin がなく、`Cond`・`WaitGroup`・`Once`・`Semaphore`・`Timer`・`Ticker`・`Context`（`after_func` の `Owned.Function` は借用を持たない）と `File`・`Net`・`Process` のハンドルは、loan を運ぶ利用者の値を持ちません（調べた結果をコードのコメントに書きました）。(b) `lending_bodies` は、持ち上げたラムダが渡すなら、そのラムダを持ち上げた元の関数も渡すとみなします（下向きの伝播に加えて上向き）。持ち上げる前の本文の走査は `E::Lambda` の本体も見るので、検査の側はもともと同じです。(c) 読み通しを本文の局所値の中身に限る 4 回目の制限はそのままです。健全な理由は `src/ownership.rs` の `lends_mutable` のコメントに書きました: 追跡されない格納は排他参照か可変セルを通して書き込み、本文の値へ届くそれをほかのコードが得るのは、本文が渡す被演算子と本文が作るラムダの捕捉からだけです。関数値の引数（`put text`、`Mutex.with_lock m`）が書き込めるのは、捕捉したもの（それを作った本文が被演算子として渡した）と、呼び出しで受け取るもの（引数の型が示す）だけです。関数名は `lends_exclusive`・`body_lends_exclusive`・`Contract.lends_exclusive` から `lends_mutable`・`body_lends_mutable`・`Contract.lends_mutable` に変えました。
- 拒否に戻った形（`tests/rc.rs` `a_lock_callback_lends_a_way_to_write`、どれも A30 の先端と同じ code・kind・位置）: `m_partial`・`rw_partial`・`m_partial_set`・`m_partial_pipe` 13:5、`m_helper_fill` 15:5、`Cond.wait_with (ref c) (ref m) (\_ -> true) (put text)` の `cond_partial` 14:5、`ref Holder`（`Mutex` を持つレコード）を渡す helper 16:5、`ref Arc<Mutex<...>>` を渡す helper 18:5（`d18ede5c` はどちらも受理して `2`/`11`）、`rt_partial` 9:5、`rt_helper` 11:5、`cond_rt` 10:5（ここまで `E1013`/`borrow_outlives_block`）、`rt_tz` 16:5 `task_capture_retains_borrow`。前から拒否していた形（lambda を helper へ渡す `closure_param_writer`・`lambda_lock_lifted` 13:5、`Arc<Mutex>` を捕捉するラムダを `run` で呼ぶ `arc_mutex_capture` 17:5）も同じ位置のままです。捕捉した排他借用で書くラムダを関数値として呼ぶ形は、ラムダが排他借用を捕捉できないので所有権検査の前に `7:9` `E1005`/`capture_exclusive_in_reusable_function` で、両方のバイナリで同じです。`Cond.wait_with` は lend します（`ref m` を受け取り、中身の `ref mut` をコールバックへ貸す）。`OnceValue.get` とチャネルの形は、別の規則（`mutex_result_retains_borrow`・`channel_item_holds_borrow`）で両方のバイナリが同じ位置で拒否します。`SyncMap` と `DataChannel` の形は書けませんでした（`SyncMap.with_value` の名前と `let (tx, rx)` の構文の誤り）が、セルの一覧には入れました。
- 受理のまま: このチケットの再現（`HashMap.remove` `0`、tzroutine の `Arc.share` `0`、`remove_ref` の後の tzroutine `0` と文字列の鍵のムーブ `0`、`Rc.share` を返す `7`、`Arc.share` の後のムーブ `43`、`pass`/generic な `pass`/パイプラインの `wrap` `42`。`-O0`・`-O3` で同じ）と、対照の `m_partial_ctl` `11`、`rt_partial_ctl` `9`、書き込まない関数値の部分適用を渡す `fnvalue_param_partial` `42`。`Arc<Atomic<i64>>` の計数と関数の `Arc` の共有を同じ tzroutine が捕捉する形は、セルの中身が借用を持てないので受理のまま（`0`。A30 の先端は `5:1` `task_capture_retains_borrow`）。再現のうち拒否に変わったものはありません。
- 基準の誤った拒否に戻った形: 4 回目の判定が受理を確かめた、所有の値を書く名前付き関数をコールバックにする `Mutex.with_lock (ref m) writer`（`d18ede5c` は `7`/`7`）は、`ref m` を渡すので A30 の先端と同じ `13:5` `borrow_outlives_block` です。lock を渡すことだけを見る規則では避けられません（コールバックが何を書くかを見るのは A31 の書き込みの事実です）。A31 の「落とし穴」に書きました。
- A31: 「再現」に `Cond.wait_with` の形（`cond_lock`、両方で `0`/`9`）を、「A28 の事実が広げる形」に lock のコールバックの 8 形と読み通しの 4 形を、プログラムと両方の値とともに足し、「テスト計画」「落とし穴」「対象外」（`Cond.wait_with` も同じ形と確かめた）と関数名を直しました。
- 証拠（基準 = A30 の先端のバイナリ `target/wt/a30/target/debug/tsuzuri`、`check --json` と `run -O0`/`-O3`。プログラムは `target/scratch/a28r5/`、作業の後に削除）: 「拒否し続けるプログラム」（`keep_leak` 6:16・4:16 `task_result_retains_borrow`、`keep_tz` 7:1、`arc_env_tz` 5:1・4:1、`copy_out_tz` 7:1、`arc_get_env_tz` 5:1（どれも `task_capture_retains_borrow`）、`class_escape` 8:16 `task_result_retains_borrow`（2 ファイルの形）、`arc_env_return` 6:5、`hashmap_env_return` 5:5、`mapr_leak` 4:5（どれも `borrow_outlives_block`））はすべて基準と同じ位置・code・kind。3 回目・4 回目の重大な指摘の形（`Arc`・`Rc`・パイプライン・generic・レコード・ラムダ・書き込みのない形 2:15〜3:15、`let a = arr` 4:5）も基準と同じ。A31 の「再現」の lock と tzroutine の形と `h6` は両方で同じ値（`0`/`9`、`1`/`9`、`0`/`0`、`9`/`9`）。A31 の「落とし穴」の受理の 26 形は両方で同じ値。
- 手順 5（基準 = A30 の先端）: A30 の木の `tests/fixtures/*` と `examples/*`（176 個）の `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（151 件が IR を出力）は、両方のバイナリですべて一致。このブランチの木では、A28 が関数を足した `hash_map`・`rc`・`tzroutine` の check と build だけが違い（基準は拒否、このブランチは受理）、ほかの 173 個は一致。その 3 つの IR は 2 回の生成で一致。
- 実行したコマンド: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`cargo nextest run --locked`: 2955 passed（7 slow）、4 skipped（新しいテストは `tests/rc.rs` `a_lock_callback_lends_a_way_to_write` と `ownership::retention_tests::a_body_that_lends_a_mutable_cell_has_no_facts`）。`node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`tzroutine`: 16（native）。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`rc.md`・`lifetimes.md`・`lambda-expressions.md`）: 3 pages、46 links、26 examples、52 native runs ok。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。
- 文書: `rc.md`（結果が `shared` の借用を持つ 2 つめの場合）、`lifetimes.md`（現在の制限の除外の文と、既知の不具合に lock のコールバックの書き込み）、`lambda-expressions.md`（除外の文）、`docs/language.md`（関数の結果が持つ借用の除外の箇条と健全な理由、`Rc / Arc` の結果の借用、排他借用フィールドの既知の不具合）、`docs/architecture.md`（除外の段落と健全な理由）、このチケットの D3、`_features/README.md` の A28 の行。

#### 6 回目の対応（本体の見えないコードへの受け渡しも除外する）

- 重大（5 回目の検証者の判定の新しい問題 1）: 関数の型は環境を表さないので、関数値を呼ぶ本体は、借用を運ぶ引数を、その関数値が捕捉したセルへ入れさせられ、別の入力（兄弟の関数値、dyn 値、`Owned.Function`）が後でそれを読み戻せました。事実はその借用を落としていました。同じ `Arc<Mutex<[unit -> i64]>>` を捕捉する書き込みと読み出しの関数値 `w`・`r` を受け取り、`w (\() -> String.length text)` を呼んで `\() -> r () + String.length other` を返す `pass` を、局所の `text` を貸して呼ぶ `leak`（`mx_leak`）は、A30 の先端では `Main.tz:12:5` `E1013`/`borrow_outlives_block`、`ae455566` は受理して `-O0` で不定の値（`4382790018` など。`-O3` は `11`）を出力しました。`RwMutex.with_write` の形（`rw_leak`）と、`w`・`r` を `run` の中のラムダで書く形（`mxi_leak` 8:5）も同じです。`Type::lends_mutable` は `pass` の引数と結果の型（`unit -> i64`・`unit`・`i64`）に排他参照もセルも見つけません。`lends_mutable` のコメントの健全な理由（関数値が書けるのは、捕捉したものと、引数の型が示すものだけ）は正しいものの、捕捉したセルに別の引数（`r`）も届き、結果がそれを見せることを見落としていました。
- 判断（オーケストレーターの判断、2026-11-06）と実装（`6936679d`）: 借用を持ちうる引数で見えないコードを呼ぶ本体も、事実を持たず読み通しません（D3 の 6 回目の決定）。`ownership::opaque_calls` が本文を走査し、(1) callee が `E::Function`・`E::GenericFunction` でない `Call` とパイプライン（仮引数より多い引数を渡した名前付き関数の、結果の呼び出しも）、(2) `E::DynDispatch`、(3) `Owned.call`、(4) `builtin_runs_arguments`（scheme の引数が関数型か、宣言が関数を持つ std の型（`scheme_runs_code`: `Owned.Function`・`Seq`）を含むか、scheme が制約を持つ）の組み込み関数へ関数を実行しうる値（`may_run_code`: 関数・dyn・tzroutine の本体に届く型）を渡す呼び出し、コールバックを取る `HostCall`、`Parallel`、`new` の初期化関数、穴の `Display` のメソッド、dyn に届く構造的な比較・hash・表示を見つけます。全引数を渡す名前付き関数の呼び出しで、借用を持ちうる引数と関数を実行しうる引数があれば記録し、`opaque_bodies` が、その呼び出しの辺と、持ち上げたラムダとの両方向の辺（`related_bodies`）の最小不動点を求めます（`Array.map` は `new` で作るラムダが `transform` を呼ぶので見えないコードで、`Array.map w (ref arr)` を呼ぶ本体も同じ）。`lending_bodies` は `Lending { facts, reads, opaque }` を返し、`facts` が事実を、`reads` が読み通しを止めます。健全な理由は `lends_mutable` のコメントに書き直しました: 本文の値（引数の loan を含む）が結果の見えない場所に入るのは、(a) 本文が被演算子として渡す排他参照かセルを通す格納（5 回目の規則）、(b) 本文の見えないコードへの受け渡し（この規則）、(c) 本文が作るラムダ（ラムダの規則）だけです。tzroutine の捕捉・チャネルの要素・`OnceValue` の初期化関数・`Owned.function` の引数は借用を持たないと検査済みです。
- オーケストレーターの判断から変えた点 2 つ:
  - callee の環境: 判断は借用を持つ「引数」を数えますが、`leak` が `\() -> w (\() -> String.length borrowed)` を作って `pass` へ渡し、`pass` が `t ()` で呼ぶ形（`thunk_env`）は、引数 `()` が借用を持たないので、判断どおりでは受理のままでした（`-O0` で不定の値、`-O3` は `9`）。呼ぶ値の環境が借用を持ちうる呼び出し（閉包なら捕捉、名前付き関数なら持たない: `value_may_carry_loans`）も数え、環境だけが持ちうるとき（`OpaqueCalls::environment`）は事実だけを捨て、読み通しはします。読み通しまで止めると、再現 `rep_arc_move`（`(deref (Arc.get (ref mine))) 40` を呼ぶ本文の `Arc.share`）が A30 の先端と同じ `3:13` `E1014` に戻るためです。環境が入れるものは環境が持つもので、それは検査が追った値から来るので、本文の値の loan は欠けません。環境の印も、呼び出しの辺と持ち上げたラムダの辺で伝えます。
  - 抽象本文の、型変数を受け手とするクラスのメソッド: 判断は見えないコードと数えますが、そうすると抽象本文で `HashMap.hash_of`・`probe`（`Hash.hash`・`Eq.eq`）が見えないコードになり、それを呼ぶ `remove_ref` も同じになって、抽象の検査（具体化しない本文の診断も出す）が `main` の `HashMap.remove_ref (ref key) map` の再現を拒否しました（`rep_remove_ref_tz` `4:1` `task_capture_retains_borrow`、`rep_remove_ref_move` `4:13` `E1014`）。型変数を関数を実行しうる値とも借用を持つ値とも数えず（`lends_mutable` の既存の規則と同じ）、型変数を受け手とするメソッドを名前付き関数と同じに扱います（dyn に届く受け手だけを見えないコードと数えます）。健全さは具体化の検査が持ちます。具体化した本文では、メソッドは名前付きのインスタンスか、vtable の dispatch（`E::DynDispatch`）です。特殊化した `HashMap` の本文が `Hash.hash`・`Eq.eq` のインスタンスを名前付き関数として呼ぶことは、`HashMap.remove` の再現が受理のままであることで確かめました（`remove_ref` が見えないコードになれば `remove` の事実が消え、`HashMap.tz:396:21` で拒否されます）。
- 拒否に戻った形（`tests/rc.rs` `an_opaque_call_hands_a_borrow_to_unseen_code`、どれも A30 の先端と同じ `E1013`/`borrow_outlives_block` と位置）: `mx_leak`・`rw_leak` 12:5、`mxi_leak` 8:5、`Owned.call w (Maybe.Some (...))` の `owned_call` 8:5、`Array.iter w`・`Array.map w`・`Array.fold w 0` の `array_iter`・`array_map`・`array_fold` 13:5、レコードのフィールドに入れた部分適用 `store a1` を呼ぶ `field_partial` 13:5、`t ()` の `thunk_env` 13:5、`dyn Sinks.Sink {r}`（`ref {r} Mutex<...>` を持つ `Cell`）で具体化した generic な `pass` の `dyn_generic` 15:5。`ae455566` はどれも受理し、`-O0` で不定の値（`-O3` は `11`、`mxi_leak` は `14`、`thunk_env` は `9`）、`dyn_generic` は `SIGABRT` でした。`IO.bind` の形（`IO.map (...) (IO.bind w (IO.pure (\() -> String.length text)))` を返す `pass`）は、`IO` の値が `w` と `text` を捕捉するので、`ae455566` でも A30 の先端と同じ `10:5` `borrow_outlives_block` です（`IO` を本体の中で実行する公開の手段はありません。`action.work ()` は `E1022`）。dyn に region の無い形は、`Dyn.of` が借用を運ぶ型の値を `dyn_holds_borrow_without_region` で拒否するので書けません。対照（`text` が生きている間に呼ぶ `mx_ctl`・`owned_ctl`）は `11`/`11` で受理のまま。
- 受理のまま: このチケットの再現（`HashMap.remove` `0`、tzroutine の `Arc.share` `0`、`remove_ref` の後の tzroutine `0` と文字列の鍵のムーブ `0`、`Rc.share` を返す `7`、`Arc.share` の後のムーブ `43`、`pass`/generic な `pass`/パイプラインの `wrap` `42`。`-O0`・`-O3` で同じ）。拒否に変わった再現はありません。
- 証拠（基準 = A30 の先端のバイナリ `target/wt/a30/target/debug/tsuzuri`、`check --json` と `run -O0`/`-O3`。プログラムは `target/scratch/a28-r6/`、作業の後に削除）: 4・5 回目の判定の全スイートと 5 回目の判定の `n1`〜`n12`（234 個）のうち 208 個が両方のバイナリで同じ結果、違う 26 個は 5 回目と同じ組（再現と `f_base` の受理 19 個と、両方が受理し `-O0` の不定の値だけが違う A31 の穴 7 個）。「拒否し続けるプログラム」（`keep_leak` 4:16・6:16、`keep_tz` 7:1、`arc_env_tz` 5:1・4:1、`copy_out_tz` 7:1、`arc_get_env_tz` 5:1、`class_escape`（2 ファイルの形）8:16、`arc_env_return` 6:5、`hashmap_env_return` 5:5、`mapr_leak` 4:5、`rc_get_return2` 4:5）はすべて基準と同じ位置・code・kind。
- 手順 5（基準 = A30 の先端）: A30 の木の `tests/fixtures/*` と `examples/*`（177 個）の `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（151 件が IR を出力）は、両方のバイナリですべて一致。このブランチの木では、A28 が関数を足した `hash_map`・`rc`・`tzroutine` の check と build だけが違い（基準は拒否、このブランチは受理）、ほかの 174 個は一致。その 3 つの IR は 2 回の生成で一致。
- 実行したコマンド: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`cargo nextest run --locked`: 2957 passed（9 slow）、4 skipped（新しいテストは `tests/rc.rs` `an_opaque_call_hands_a_borrow_to_unseen_code` と `ownership::retention_tests::a_body_that_calls_unseen_code_has_no_facts`）。`node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`tzroutine`: 16。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`lambda-expressions.md`・`lifetimes.md`・`rc.md`）: 3 pages、48 links、26 examples、52 native runs ok。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。
- 文書: `lambda-expressions.md`（見えないコードへ渡す関数の結果と `t ()`）、`lifetimes.md`（現在の制限の除外の文）、`rc.md`（結果が `shared` の借用を持つ 3 つめの場合）、`docs/language.md`（関数の結果が持つ借用の除外の箇条と健全な理由、`Rc / Arc` の結果の借用）、`docs/architecture.md`（除外の段落の実装）、このチケットの D3、`_features/README.md` の A28 の行。A31 の「A28 の事実が広げる形」に見えないコードの 10 形を足し、この除外は手順 4 で外さない（書き込みの事実は関数の型を通して呼び出し元へ届かない）ことを「落とし穴」と「テスト計画」に書きました。

#### 7 回目の対応（値の破棄が実行する `Drop.drop`）

- 重大（6 回目の検証者の判定の新しい問題）: 値の破棄は本文に呼び出しを書かずに利用者の `Drop.drop` を実行し、`Drop.drop` は破棄する値の中身を値の中のセルへ入れられます。`lends_mutable` も `opaque_calls` もそれを見ないので、使わない引数 `_s` を持つだけの `fn pass _s r = r` が事実を持ち、`_s` の `Stash { cell: a, f: \() -> String.length borrowed }` の解放が、`r` も読む `Arc<Mutex<…>>` へ借用する関数を入れても、`leak` の結果は `text` の借用を持ちませんでした（`drop_unused`: A30 の先端は `13:5` `E1013`/`borrow_outlives_block`、`2cbc512a` は受理して `-O0` で `3602879701896396803`、`-O3` は `12`）。同じ形: `let _held = s`（`drop_param` 15:5）、`pass` の中で引数から作った `Stash`（`drop_local` 14:5）、再代入（`drop_reassign` 16:5）、`[Stash]` を持つレコード（`drop_nested_array` 14:5）、別の `Arc` 引数を返す `pass`（`drop_rc_repro` 11:5）、セルを持たず関数値のフィールド `w` を呼ぶ `Drop.drop`（`drop_opaque_field` 15:5）と、その `Owned.drop s`（`owned_drop_opaque` 17:5）。`lends_mutable` のコメントの完全さの議論（「ほかにコードを実行する経路はない」）と `docs/language.md` の「3 つだけ」は誤りでした。
- 判断（オーケストレーターの判断、2026-11-06）と実装（`8dbe05f0`）: (1) 事実: 本文が破棄する値で、破棄でユーザーのコードを実行しうる型（利用者の `Drop` に届く型、関数型、`Owned.Function`、dyn）で loan を持つものの loan（またはその親）が外部 loan である引数を保持にする。(2) 読み通し: そうした値を破棄する本体は読み通さない。(3) 暗黙にユーザーのコードを実行するほかの経路を調べ、名前付き関数の本文として解析されるか、6 回目の規則で見えないコードと数えられるかを確かめる。実装: `Type::drop_runs_code`・`Type::runs_on_drop`（`src/check.rs`）、`Checker::dropped`・`collect_drops`（引数の loan で始め、排他参照を通してそうした値に届く引数も数える。`deref target = v` は古い値を破棄する）と `Checker::eval` での収集、`retained_parameters` が結果の loan と一緒に `dropped` を `parents` までたどる。`drops_stop_reads` は読み通しの呼び出しを含む本体（`calls_read_through`）を読み通しありで一度検査し、読み通しを使い `dropped` が空でなければ `Checker::drops_stop_reads` を立てる（`check_body` と `retained_parameters`）。`Rc.share` などの判定は自由関数 `reads_through`・`builtin_called` に移した。(3) の調査は `lends_mutable` のコメントに (d) として書きました: 利用者の型の演算子は `polymorph::operator_call` が名前付きのインスタンスの直接の呼び出しにし、クラスのメソッド（穴の `Display`、std のコレクションが呼ぶ `Eq`・`Hash`・`Ord`・`Default`、`src/derive.rs` が作るインスタンス）は特殊化で名前付きのインスタンスか `E::DynDispatch` になり、穴のメソッドは `opaque_calls` の呼び出し、`for x in seq` は `control::sequence_loop` が `Seq` の関数の呼び出しにし、`as`（`E::Cast`）は数値と関数の純粋さだけを変え、`deref` は場所を読み、パターンのガードと `finally` は本文の式、`new` の初期化関数は `opaque_calls`。直すべき経路はほかにありませんでした。
- オーケストレーターの判断から変えた点 2 つ:
  - プログラムの条件 `Lending::drops_store`: (1)(2) は、プログラムのどれかの `Drop.drop` の本体が事実を持たない（`lends_mutable` か見えないコード。持ち上げたラムダを含む）ときだけ使います。判断どおりにどのプログラムでも (2) を使うと、`tests/rc.rs` `sharing_a_function_keeps_the_borrows_of_its_environment_only` の `let shared = Rc.new (\() -> String.length borrowed)`、`Rc.share (ref shared)`、`let moved = shared`（A28 が受理にした形）が、`moved` と `mine`（借用する関数の `Rc`）の破棄で読み通しを失い、A30 の先端と同じ `E1014` に戻りました（`Drop` 型の無いプログラムです）。`Drop.drop` は破棄する値だけを受け取り、`Drop.drop` の中の格納は (a)〜(c) の形でしか追跡を逃れない（全体置換と `ref mut` での受け渡しは `E1012`、フィールドへの代入も禁止）ので、どの `Drop.drop` も事実を持つプログラムでは、破棄は追跡されない格納をしません。条件が真のプログラムでは、上の `Rc` の形は `E1014` です（A30 の先端と同じ。`tests/rc.rs` `an_implicit_drop_keeps_what_the_dropped_value_holds` に記録）。
  - 破棄する値だけでなく、本体が受け取る・作るそうした型の値をすべて数えます（`Checker::eval` が評価したすべての値と引数）。値を名前付きの callee へ渡し、callee がそれを破棄して loan を運ばない結果（`unit`）を返す形（`drop_consume_opaque`: `fn consume _s = ()` と `fn pass s r = { consume s; r }`）では、callee の事実が引数を「保持」にしても、`unit` の結果がそれを運ばないので、判断どおりに本体が自分で破棄する値だけを数えると受理のままになるためです（推論。直前のバイナリでは確かめていません。A30 の先端は `19:5` で拒否し、このブランチも同じ）。結果へ移る値は結果がもともと loan を持つので、数えても事実は変わりません。
- 拒否に戻った形（`tests/rc.rs` `an_implicit_drop_keeps_what_the_dropped_value_holds`、どれも A30 の先端と同じ `E1013`/`borrow_outlives_block` と位置）: `drop_unused` 13:5、`drop_param` 15:5、`drop_local` 14:5、`drop_reassign` 16:5、`drop_nested_array` 14:5、`drop_rc_repro` 11:5、`drop_opaque_field` 15:5、`owned_drop_opaque` 17:5、`drop_consume` 17:5、`drop_consume_opaque` 19:5。対照: `text` が生きている間に呼ぶ `drop_ctl` は `12`/`12`、借用を持たない値を破棄する `pass` を `wrap` が `ref key` と呼ぶ `ctl_drop_no_loan` は受理して `5`/`5`（A30 の先端は `12:19` `task_result_retains_borrow`）。そうした `Drop.drop` があるプログラムでも、`HashMap.remove`（`0`）、`remove_ref` の後の鍵のムーブ（`1`）、`pass`（`42`）は受理のまま。事実の単体テスト `ownership::retention_tests::a_dropped_value_keeps_the_loans_that_it_holds`（`drops_store` が真なら `_s`・`Stash` に入れた引数・`consume` へ渡す引数を保持、偽なら以前の事実、借用で受け取る `Stash` はどちらも一時）。
- 受理のまま: このチケットの再現（`HashMap.remove` `0`、tzroutine の `Arc.share` `0`、`remove_ref` の後の tzroutine `0` と鍵のムーブ `0`（s10 の形 `1`）、`Rc.share` を返す `7`、`Arc.share` の後のムーブ `43`、`pass`/generic な `pass`/パイプラインの `wrap` `42`。`-O0`・`-O3` で同じ）。
- 証拠（基準 = A30 の先端のバイナリ `target/wt/a30/target/debug/tsuzuri`、`check --json` と `run -O0`/`-O3`。プログラムは `target/scratch/a28-r7/`、作業の後に削除）: v4〜v7 と 6 回目の判定の v8（p1〜p5）の 294 個のうち 266 個が同じ、違う 28 個は 6 回目と同じ組（再現と `f_base` の受理 19 個、両方が受理し `-O0` の不定の値だけが違う A31 の穴 7 個、v7 の `d1`・`d3`）。v8 の 27 個はすべて同じ（6 回目の判定の 8 個の違いが無くなった）。新しい探査（`r7/q1`〜`q3`、16 個）は 11 個が同じで、違う 5 個はどれも受理への変化（`ctl_drop_no_loan` `5`、何もしない `Drop.drop` のプログラムで `Rc` を共有した後のムーブ `5`、追跡されない格納をする `Drop.drop` のプログラムの `HashMap.remove` `0`・`remove_ref` の後の鍵のムーブ `1`・`pass` `42`）。`ref mut` の先を置き換える形（`drop_replace_ref_mut`）は両方 `9:5` `store_region_unnamed`。閉包が `Drop` 型を捕捉する形は `E1005`（`capture_exclusive_in_reusable_function`）で書けません。読み通しの形（`drop_read_through`、`Stash` が `Arc.share (ref b)` を持つ）は、`ref b` がセルに届くので `lends_mutable` で拒否されます（両方 `9:5`）。「拒否し続けるプログラム」はすべて基準と同じ位置・code・kind（`keep_leak` 4:16・6:16 `task_result_retains_borrow`、`keep_tz` 7:1・`arc_env_tz` 5:1・4:1・`copy_out_tz` 7:1・`arc_get_env_tz` 5:1 `task_capture_retains_borrow`、`class_escape` 8:16 `task_result_retains_borrow`、`arc_env_return` 6:5・`hashmap_env_return` 5:5・`mapr_leak` 4:5・`rc_get_return2` 4:5 `borrow_outlives_block`）。
- 手順 5（基準 = A30 の先端）: この木の `tests/fixtures/*` と `examples/*` の 176 個のディレクトリの `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（基準で 148 件が IR を出力）は、A28 が関数を足した `hash_map`・`rc`・`tzroutine` だけが違い（基準は拒否、このブランチは受理）、ほかの 173 個は一致。その 3 つの IR は 2 回の生成で一致。
- 実行したコマンド: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`cargo nextest run --locked`: 2959 passed（9 slow）、4 skipped。`node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`tzroutine`: 16。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`rc.md`・`drop.md`・`lifetimes.md`・`lambda-expressions.md`）: 4 pages、71 links、29 examples、58 native runs ok。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。
- 文書: `docs/language.md`（4 つめの経路の箇条と調べた経路、`Rc / Arc` の結果の借用の除外）、`docs/architecture.md`（`drops_store`・`dropped`・`drops_stop_reads` の実装）、`rc.md`（結果が `shared` の借用を持つ 4 つめの場合）、`lifetimes.md`・`lambda-expressions.md`（破棄の文）、`drop.md`（「Drop 型に課される制約」の関数の結果の借用と例）、このチケットの D3、`_features/README.md` の A28 の行、A31 の「A28 の事実が広げる形」に破棄の形。

#### 8 回目の対応（プログラム全体の条件: 追跡されない格納のないプログラムだけで A28 を使う）

- 重大（7 回目の検証者の判定の新しい問題）: 作った後のフィールドへの代入が、引数の借用を Drop 型の値へ入れても、`Checker::dropped` に数えられませんでした。`let mut held = Stash { cell: a, t: u }` の後の `held.t = t` は、`t`（`ref string`）が破棄でコードを実行しないので `dropped` に入らず、`pass` の事実が `t` を外し、`held` の解放が `t` を借用する関数を `r` が読むセルへ入れました（`fa_ref`: A30 の先端は `15:5` `E1013`/`borrow_outlives_block`、`722c421e` は受理して `-O0` で `3602879701896396803`、`-O3` で `12`）。同じ形が 13 個（`fa_param_move`・`fa_nested`・`fa_whole_inner`・`fa_wrapper`・`fa_array_elem`・`fa_field_array`・`fa_alias`・`fa_if`・`fa_while`・`fa_maybe_ref_pass`・`fa_opaque`・`xmod_fa`）。
- 判断（オーケストレーターの判断、2026-11-07）: 3〜7 回目の局所の除外はどれも 1 つの経路を塞いだだけで、新しい重大な指摘はどれも、借用を持つ値の追跡されない in-place の格納（`Array.write` とそれで書く std の関数を、ロックのコールバック、見えない呼び出し、`Drop.drop`、フィールドへの代入を通して）を要していました。そこで健全さの主な理由をプログラム全体にします。(1) 検査するモジュール（最後の検査では特殊化したモジュール、推論の段では抽象モジュール。`$builtin` の生成関数を含む）のどれかの関数が、格納を追跡しない組み込み関数を、借用を持ちうる型（`carries_loans` か型変数を含む）の格納する値で使うなら、A28 をプログラム全体で使わない（すべての関数の事実が `None`、読み通さない。A30 の先端と同じ）。(2) 3〜7 回目の局所の除外は重ねて残し、`lends_mutable` のコメントの健全な理由を、プログラム全体の条件を先に書き直す。(3) テスト。
- 実装（`948d6ffa`）: `src/ownership.rs` の `untracked_writer`（組み込み関数と格納する引数の位置）・`untracked_loan_store`（すべての関数の本文を 1 回走査し、`E::Function(FunctionRef::Builtin)` の言及の型から格納する引数の型を読む。完全適用・パイプライン・部分適用・関数値のどれでも当たる。型が引数を示さない言及は数える）・`Lending::untracked`・`Lending::no_facts`・`Lending::no_reads`。立つと `parameter_retention_within` はすべての関数に `None` を返し、`CallFacts::contract` と `Checker::body_lends_mutable` は書き込める場所を渡す本体と同じに扱い、`drops_stop_reads` は何もしません。`Lending.facts`・`reads`・`drops_store` は局所の除外だけを表します（局所の除外の単体テストが、条件を外した `local_facts` で事実を求められるように）。
- `Builtin::ALL` の調べ（337 個のうち、scheme に `ref mut` を含む組み込み関数 11 個と、std の型か共有ポインタへの参照とほかの引数を受け取る組み込み関数 28 個。`untracked_writer` のコメントに一覧）: 数える: `Array.write`（第 2 引数）・`SyncMap.__replace`（第 1 引数）・`Simd.store`（数値だけ）・`Atomic.store`・`Atomic.swap`・`Atomic.compare_exchange`（中身は数値か `bool`）。追跡されるので数えない: `Vec.push_mut`（region slots）、`Channel.send`・`Channel.try_send`・`SendOnly.send`・`SendOnly.try_send`・`DataChannel.send`（要素は借用を持たないと検査）、`OnceValue.get`（初期化関数の結果を同じく検査）、`WaitGroup.go`・`Context.__after_func`（`Owned.Function`）、`Atomic.fetch_*`（整数）。何も格納しない: `Array.swap_in`・`Array.split_at_mut`・`Vec.__view`・`RwMutex.with_read`・`Rc.ptr_eq`・`Arc.ptr_eq`・`Owned.call`、整数だけの `Timer.__reset`・`Ticker.__reset`・`WaitGroup.add`・`Semaphore.*`・`Context.__with_timeout`・`__with_deadline`。コールバックへ `ref mut` を貸すだけ: `Mutex.with_lock`・`RwMutex.with_write`・`Cond.wait_with`・`Parallel.for_each_chunk`（コールバックの格納は代入（追跡）か数える組み込み関数か関数の呼び出しで、コールバックの本文はモジュールの関数として走査する）。値で受けて返す `Array.__set`・`Array.update`・`Array.swap`・`Vec.push` などは参照を通して格納しません。std の関数は書き込み関数そのものではなく、モジュールの関数として走査します。
- オーケストレーターの判断から変えた点: 推論の段（抽象モジュール）では、格納する値の型の型変数を借用を持たないものとして数えます（特殊化したモジュールでは判断どおり数えます）。判断どおりにすると、std の `fn set index value values = Array.write index value values`（`'a`）が、`Array` を読み込むどのプログラムの抽象モジュールにもあるので、推論の段の検査がすべてのプログラムで A30 の規則になり、このチケットの再現（`HashMap.remove`、tzroutine の `Arc.share`、`Rc.share` を返す形、`pass`/`wrap`、パイプライン、`remove_ref` の後の形）が、推論の段で A30 の先端と同じ位置・kind で拒否されました（`target/scratch/verify8-a28` で確かめた）。抽象モジュールの事実は Copy 制約の推論にだけ使い、generic な書き込み関数の具体化は特殊化したモジュールの具体的な関数なので、受理を決める最後の検査はそれを数えます。この違いが出たのは `generic_drop_inst`（下）だけです。
- 健全な理由（`lends_mutable` のコメントを書き直した）: 追跡されない組み込み関数を除けば、格納は追跡されます（代入、`deref r = v`、`Vec.push_mut`、region の契約を持つ関数。参照を通した格納は借用を持つ値に名前付き region を要し、チャネルの要素・`OnceValue` の初期化関数・`Owned.Function` は借用を持たないと検査される）。暗黙に実行するコード（破棄・演算子・クラスのメソッド・dyn のメソッド）と関数値のコードもモジュールの関数の本文で、同じ規則で検査されます。借用を持たない値の格納は loan を不完全にしません。したがって条件が偽のプログラムでは、検査が値に与える loan はどの値でも完全で、事実（結果に届かない引数を除く）と読み通し（所有者の loan が示す中身を渡す）はそこから従います。条件が真なら A28 より前の規則です。A31 がその格納を追跡すれば、条件は何も見つけず、A28 はすべてのプログラムに効きます（A31 の手順 4 に、この条件を先に外すことを書いた）。
- テスト: `tests/rc.rs` `an_untracked_store_keeps_the_rule_from_before_a28_in_the_whole_program`（7 回目の判定の 13 形が、A30 の先端と同じ位置で `E1013`/`borrow_outlives_block`。edition 2027 で検査。`text` が生きている間の対照 `fa_ref_ctl` は受理）、`src/ownership.rs` `retention_tests::an_untracked_store_anywhere_leaves_no_facts`（関数を `Array.write` で書く本体、その部分適用、パイプラインの `Array.set` があるモジュールは `untracked` で事実が無く、すべての本体が読み通さない。`i64` を書くモジュールは `a28_pass` の事実を持つ）。条件を外した版で、新しい `rc` のテストと、下の 2 つの変えたテストが失敗することを確かめました。それまでの回の重大な指摘の形は、それぞれの回のテスト（`tests/mutable_slices.rs` `a_function_that_writes_in_place_keeps_its_arguments_loans`（1 回目）、`tests/rc.rs` `a_share_of_an_element_holds_what_was_written_in`（3・4 回目）、`a_lock_callback_lends_a_way_to_write`（5 回目）、`an_opaque_call_hands_a_borrow_to_unseen_code`（6 回目）、`an_implicit_drop_keeps_what_the_dropped_value_holds`（7 回目））が、A30 の先端と同じ code・kind（4〜7 回目は位置も）で拒否のままです。どれも `Array.write` で借用する関数を書くので、今は主にプログラム全体の条件で拒否されます。局所の除外の単体テスト `a_body_that_lends_an_exclusive_borrow_has_no_facts`・`a_body_that_lends_a_mutable_cell_has_no_facts` は `local_facts` を使うようにしました。
- 変えた 2 つの期待（どちらも A30 の先端と同じになった）: `a_share_of_an_element_holds_what_was_written_in` の `Arc.upgrade (ref weak)` の形は、7 回目までの `3:5` `task_result_retains_borrow` から、A30 の先端と同じ `4:5` `borrow_outlives_block` になりました（`make` が `Array.write` で書くので読み通さない）。`an_implicit_drop_keeps_what_the_dropped_value_holds` の対照 `ctl_drop_no_loan`（7 回目で受理にした形）は、`Drop.drop` が `Array.write` で書くので、A30 の先端と同じ `12:19` `E1013`/`task_result_retains_borrow` です。
- 再現の確認: このチケットの再現は、追跡されない借用を持ちうる格納に届きません（条件が偽で受理される）。`HashMap.remove` `0`、tzroutine の `Arc.share` `0`、`remove_ref` の後の tzroutine `0` と鍵のムーブ `0`（s10 の形 `1`）、`Rc.share` を返す `7`、`Arc.share` の後のムーブ `43`、`pass`/generic な `pass`/パイプラインの `wrap` `42`。`-O0`・`-O3` で同じ。`HashMap` は `Array.write` を使わず、`Array` を読み込む再現でも、抽象モジュールの `Array.set` の `'a` は上の理由で数えず、特殊化したモジュールに借用を持ちうる型の具体化はありません。
- 証拠（基準 = A30 の先端のバイナリ `target/wt/a30/target/debug/tsuzuri`、`check --json` と `run -O0`/`-O3`。プログラムは `target/scratch/verify8-a28/`、作業の後に削除）: v4〜v9 と r7 の 344 個のうち 311 個が同じ、違う 33 個。違うのは、再現と `f_base` の受理 19 個（上の値と `f_base` `2`）、両方が受理し `-O0` の不定の値だけが違う A31 の穴 7 個（`h1`〜`h5_rc_get`・`h7_array_return`・`base_hole_array_return`。どちらも追跡されない格納をするプログラムで、A28 は使われない）、v7 の `d1`（`HashMap.remove_ref` を返す `0`）・`d3`（`Arc.share` を返す。基準の 2 つの診断のうち `borrow_outlives_block` が無くなり `borrow_of_temporary` だけ）の A28 の受理・診断の減少、r7 の受理 4 個（何もしない・関数値を呼ぶ `Drop.drop` のプログラムの `share_with_quiet_drop` `5`、`remove_with_storing_drop` `0`、`remove_ref_move_with_storing_drop` `1`、`pass_with_storing_drop` `42`。どれも追跡されない格納が無い）、`generic_drop_inst`（下）です。7 回目の違いのうち `ctl_drop_no_loan` は同じになりました（上）。v9 の 13 形を含む 7 回目の判定の探査はすべて同じです。
- `generic_drop_inst`（`instance Drop<Holder<'a>>` の `drop` が `'a` の値を `Array.write` で書く）: 基準は `13:5` `E1013`/`borrow_outlives_block`、このブランチは `2:10` `E1027`/`undeclared_instance_constraint` だけ。推論の段で型変数の格納を数えないので抽象本文の検査が通り、特殊化の前の E1027 が先に出ます。基準も `leak` を除くと同じ E1027 を出すので、もともと受理されないプログラムの診断の順序だけの違いです。
- 拒否し続けるプログラム: すべて基準と同じ位置・code・kind（`keep_leak` 4:16・6:16 `task_result_retains_borrow`、`keep_tz` 7:1・`arc_env_tz` 5:1・`arc_env_tz_direct` 4:1・`copy_out_tz` 7:1・`arc_get_env_tz` 5:1 `task_capture_retains_borrow`、`class_escape` 8:16 `task_result_retains_borrow`、`arc_env_return` 6:5・`hashmap_env_return` 5:5・`mapr_leak` 4:5・`rc_get_return2` 4:5 `borrow_outlives_block`）。
- 手順 5（基準 = A30 の先端）: この木の `tests/fixtures/*` と `examples/*` の 176 個のディレクトリの `check --json`（終了コードを含む）と `build --no-cache --emit llvm`（基準で 148 件が IR を出力）は、A28 が関数を足した `hash_map`・`rc`・`tzroutine` だけが違い（基準は拒否、このブランチは受理）、ほかの 173 個は一致。その 3 つの IR は 2 回の生成で一致。
- 実行したコマンド: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: ok。`cargo nextest run --locked`: 2961 passed（8 slow）、4 skipped。`node tests/features.mjs target/debug/tsuzuri hash_map`: 199 cases、`rc`: 39、`tzroutine`: 16。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`lifetimes.md`・`rc.md`・`lambda-expressions.md`・`drop.md`）: 4 pages、75 links、29 examples、58 native runs ok。`node scripts/check-guide-lite.mjs`: 88 references、0 failed。
- 文書: `lifetimes.md`・`lambda-expressions.md`（2 つの規則はプログラムに追跡されない借用の格納が無いときだけ、A31 との関係、局所の除外は念のため残すもの）、`rc.md`（結果が `shared` の借用を持つ場合の 1 つめ）、`drop.md`（プログラム全体の条件を先に書き、例を関数値を呼ぶ `Drop.drop` に変えた。`drop` が何もしなければ受理）、`docs/language.md`（プログラム全体の条件の箇条、`Rc / Arc` の結果の借用の除外）、`docs/architecture.md`（`untracked_loan_store` の実装）、このチケットの D3、`_features/README.md` の A28 の行、A31 の手順 4。

#### Copilot のレビュー（#100）への対応

- 指摘（`src/ownership.rs` の `piped_callee`）: 1 引数の名前付き関数へのパイプライン `x |> f` も `f x` の完全な直接呼び出しですが、右が `E::Call` でない（関数そのもの）ときは事実を使っていませんでした。`pass _key f = f`、`make key = pass key (\() -> 42)` で、`make (ref key)` は `key` を借用しない関数を返すのに、`ref key |> make` は借用を持ち、`wrap` から返すと `E1013`/`task_result_retains_borrow` でした（このブランチのバイナリで再現）。
- 対応: `piped_application` が、パイプラインの右から callee と先に束縛した引数を取り出します（`f a` なら `f` と `[a]`、裸の `f` なら `f` と `[]`）。`parameter_retention_within` の呼び出しの収集と、`piped_callee`・`eval_piped_arguments`・パイプラインの評価の `piped` がこれを使うので、`x |> f` は束縛した引数が 0 個の場合として `f x` と同じ事実（とその除外: プログラム全体の追跡されない格納、書き込める場所を渡す本体、見えない呼び出し、破棄、読み通しの制限）を使います。2 引数以上の関数への `x |> f` は部分適用なので、今までどおり事実を使わず、借用を持ちます。
- テスト: `tests/currying.rs` `a_pipeline_into_a_unary_function_uses_its_facts`（直接の形とパイプライン形が同じ結果: `make` の受理、`keep_leak`・書き込める場所を渡す本体・`Array.write` のあるプログラムの `E1013`/`task_result_retains_borrow`）。
- 文書: `docs/language.md` と `lifetimes.md` の「それを完成させるパイプライン」に `x |> f`（1 引数の関数）を足した。
