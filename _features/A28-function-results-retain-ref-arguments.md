# A28: 関数を含む型の呼び出し結果が、`ref` 引数の借用をすべて持つとみなされる（`HashMap.remove` と、関数の `Arc` の `Arc.share` が誤って `E1013`）

| 項目 | 内容 |
| --- | --- |
| ID | A28 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | C09, TZ02 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-05（TZ12 の作業中に発見。HEAD `5ee6b0f2` で確認・原因を調査） |
| 承認 | 不要（健全なまま、誤って拒否しているプログラムを受理する。受理していたプログラムは変えない） |
| 改善する劣位 | 不具合: 値の型が関数を含む `HashMap`（`HashMap<i64, unit -> i64>`・`Owned.Function`・`Maybe<fn>`・関数を持つレコード）の `HashMap.remove` が、std の中で `E1013`（`cannot return a reference to a local value`）になる。関数の `Arc` を `Arc.share` した値を tzroutine が捕捉すると `E1013`（`the captures of a tzroutine cannot retain borrowed values, ...`）になる。利用者の関数でも、`ref` 引数を使わずに関数値を返すだけの呼び出しが同じく拒否される |
| 手本にする既存実装 | `src/ownership.rs` の `closed_returns`・`closed`（結果が借用を持たないことの、呼び出しグラフ上の証明）と、`read_places`（`deref (Arc.get (ref shared))` が、`shared` の借用ではなく、`shared` が持つ借用だけを持つことを表す） |
| 主な影響ファイル | `src/ownership.rs`, `tests/hash_map.rs`, `tests/rc.rs`, `tests/tzroutine.rs`, `tests/currying.rs`, `docs/architecture.md`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `_features/README.md` |

## 目的

呼び出しの結果が関数を含む型のとき、`ref` で渡した引数の借用を結果がすべて持つとみなす保守的な規則を、callee の本文から分かる事実で狭める。
`HashMap.remove` を関数の値の表で使えるようにし、[tzroutine](../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) のページが勧める「各 tzroutine に `Arc.share` を渡す」書き方を、
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
