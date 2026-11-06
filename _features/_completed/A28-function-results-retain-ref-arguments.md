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

### D3: 本体が排他借用を呼び出しへ渡す関数は、事実も読み通しも使わない（3 回目のレビュー対応で決定）

- 決定: `retained_parameters` は、本体が排他参照に届く型の引数（`ref mut values`、`ref mut` の引数、それを持つ値）を呼び出しへ渡す関数に事実を与えない（結果はすべての引数の借用を持つ）。そうした本体では `Rc`／`Arc` の `share`・`downgrade`・`upgrade` も読み通さない。特殊化した本文と抽象本文の両方に適用する。`Array.write` などの書き込みの追跡は、A30 の先端のまま変えない。
- 理由: 所有権検査は `Array.write`（とそれで書き込む std の関数）と `SyncMap.__replace` が書き込む値の借用を書き込み先の持ち主に残さない（A28 より前からの穴、[A31](../A31-in-place-writes-keep-stored-borrows.md)）。A28 の事実と読み通しは本文の結果の loan を信頼するので、そのままでは穴を関数の境界の外へ広げる。これらの書き込み関数はどれも書き込み先を排他借用の引数で受け取るので、排他借用を呼び出しへ渡さない本文の結果の loan は完全で、渡す本文を A28 より前の規則に戻せば、A28 は基準が受理するプログラムを変えず、基準が拒否する危険なプログラムを受理しない。
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
