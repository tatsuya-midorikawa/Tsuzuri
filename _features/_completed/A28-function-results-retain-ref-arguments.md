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

