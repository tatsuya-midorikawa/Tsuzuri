# A34: A31 の格納と破棄の順序の検査が `main` の受理する安全なプログラムを拒否する 6 つの形を、危険な双子を拒否したまま受理する

| 項目 | 内容 |
| --- | --- |
| ID | A34 |
| 優先度 | P1（`main` が受理する安全なプログラムを A31 が拒否する表現力の退行。どれも回避の書き方がある。危険なプログラムの受理ではない） |
| 規模 | L（6 つの項目は互いに独立で、項目ごとに S〜M。項目ごとに分けて入れてよい） |
| 依存 | A31 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-15（A31 の Copilot のレビューの後のレビューの第4回・第5回から。A31 の先端（`src/` が `6e4f0537` と同じもの）と `main`（`168ae4a5`）で再現を確かめた。第6回の修正（`5e99c1c0`）の後、`sharetype` に形を足した） |
| 承認 | 不要（今拒否している安全なプログラムを受理するだけで、危険なプログラムは拒否したまま。言語の意味は変えない） |
| 改善する劣位 | 表現力の退行: `main` が受理する安全なプログラムを、A31 の安全側の近似が `E1013`・`E1014` で拒否する。利用者は、項目ごとの回避の書き方（「現状」）を知らないと書けない |
| 手本にする既存実装 | `src/ownership.rs` の、`Checker::eval` の `E::Borrow` の腕（`typed`・`exact_loans`）と `exact_loan`・`points_at`、`StoreFacts::drop_only`・`Checker::parameter_part`・`drop_stores`・`value_runs_drop_code`・`apply_facts_once`（`dropping_part`）、`Checker::loan`・`kept_copy`・`path_copy`・`read_through_loan`（同じ場所の再借用を親として読む）・`Checker::captured_loans`、`keep_for_drop`・`share_sources`・`export_shared_block`・`store_exported_into`・`OwnerScan`（`may_own`: 値を作った式から、持ちえないものを決める）、`State::block_shares`・`note_block_share`・`forget_block_share`・`block_groups`・`same_block_loan`・`check_drop_order_of`・`moved_always`、`share_loans`（`Drop.drop` の中の `drop_root` の扱い） |
| 主な影響ファイル | `src/ownership.rs`, `tests/rc.rs`, `tests/mutable_slices.rs`, `tests/fixtures/in_place_writes/Main.tz`, `tests/features.mjs`, `_tsuzuri/language-reference/ownership-and-memory/drop.md`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `docs/language.md`, `docs/architecture.md`, `_features/A33-remaining-store-soundness-holes.md`, `_features/README.md` |

## 目的

A31（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)）は、その場の書き込み・ロックのコールバック・破棄が実行する `Drop.drop` が書き込んだ借用を、書き込み先の持ち主に残すようにしました。その検査は分からない所を安全側に近似するので、`main` が受理する安全なプログラムのいくつかを `E1013`・`E1014` で拒否します。このチケットは、そのうち A31 の Copilot のレビューの後のレビューの第4回・第5回で見つけ、A31 では文書に書くだけにした次の 6 つの形を、それぞれの危険な双子（安全な形にいちばん近い危険な形）を拒否したまま受理します。

| 名前 | 形 | 回避の書き方（A31 の先端が受理する） |
| --- | --- | --- |
| `rcgetpart` | フィールドや要素の場所の `Rc` から `Rc.get` で得た参照を束縛した変数（`let b = Rc.get (ref rs[0])`）を、書き込む値の側へ渡す・ラムダで捕捉する・部分適用で束縛する | `Rc.get (ref rs[0])` を引数にそのまま書く |
| `fvdrop` | 受け取った関数値を持つ値（関数値を入れたリテラル、`match` のパターンの変数、型注釈を書いたラムダの仮引数、edition 2027 で読み取りの仮引数やそのフィールドを束縛した局所値）を破棄する関数に、破棄しても何も実行しない関数値（`cross (ref p2)`）を渡す | 仮引数をそのまま破棄する（`fn keep f = ()`）か、`own` の仮引数を束縛して破棄する |
| `refsrc` | 書き込む部分の側へ不変の参照の変数（`let q1 = ref p1` の `q1`）を渡した後で、借用されていない部分（`p1.h`）をムーブ・書き換える | 呼び出しの位置で `ref p1` と書く |
| `sharetype` | `let r = Rc.share (ref r2)` の `r` を使い終えた後で、共有したブロックの型を持ちうる別の値へ、`r2` より後に宣言した値の借用を書き込む | 借用される値を `r2` より先に宣言する |
| `rcgetowner` | `Rc.get (ref r)` の借用を、`r` より前に宣言した値へ書き込む（ブロックは、もっと前に宣言した別の持ち主 `r2` が生かす） | 借用を先に宣言した持ち主から得る（`Rc.get (ref r2)`） |
| `shareparam` | 参照の仮引数の `Rc` から関数の中で作った共有（`fn dup r = Rc.share r` の結果、本体の `let s = Rc.share r`） | 呼び出し元で `Rc.share (ref r2)` と書く |

利用者にとっては、A31 の前に書けた安全なプログラムが、回避の書き方を知らなくても、そのまま書けるようになります。

## 背景

- A31 のレビューの第4回からの規則は「前の先端からの退行でない安全な形の拒否は `src/` を変えず、保守的な制限として文書に書く」でした（[A31 のチケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第4回」「レビューの第5回」）。6 つの形はこの規則で、`drop.md`・`lifetimes.md`・`docs/language.md` に安全側の扱いとして書き、[A33](A33-remaining-store-soundness-holes.md) の「保守的な制限」の表に行として足しました。`rcgetpart`・`fvdrop`・`refsrc`・`rcgetowner`・`shareparam` は A31 の前の先端からある拒否で、`sharetype` は第4回の修正（使い終えた共有の持ち主を通した格納を元の所有者へ型で届ける）が生んだ拒否です（第5回で、元の所有者が解放しえないブロックへの格納だけは届けないように狭めました）。どれも A31 のマージの条件 (3)（`main` が受理する安全なプログラムは、文書に書いた安全側の制限のほかは受理のまま）に従った扱いです。第6回では、第5回の狭めが dyn の値の中の共有を通した危険な形を受理していた（前の先端からの退行）ので、dyn の値と型引数に届く型では狭めないようにし、それによる安全側の拒否と、規則の実際の範囲（元の所有者のほかの部分が持ちうるブロック）を `sharetype` に足しました（「`sharetype` に A31 のレビューの第6回で足した形」）。
- 利用者の依頼で、A31 が直さずに残すこれらの安全な形の拒否を、文書と A33 の表の行だけでなく、このチケットで個別に直します。
- **A33 との関係**: A33 は、格納と破棄の順序の検査を、記憶域の寿命と破棄の順序の 1 つのモデルで作り直す XL のチケットです。A34 は、項目ごとに `src/ownership.rs` の 1〜2 個の近似を狭める小さな変更の集まりで、A33 のモデルを待たずに入れられます。6 つの項目は互いに独立で、どの順にも、項目ごとの PR にも分けられます。A33 が先に入るなら、A34 の各項目の安全なプログラムと危険な双子は A33 の受け入れのテストになり、A34 には A33 の後も拒否が残る項目だけが残ります。A34 の項目が先に入れば、その項目は A33 の段 3 の一覧から外れます。A33 の表では、この 6 つを A34 へ移したことを 1 行で示しています。
- 拒否の理由は、どれも検査が知らないことを安全側に扱うことです（「原因」）: (1) フィールドや要素の場所の `Rc` の借用を exact な loan にしない、(2) 破棄だけが作る格納（`drop_only`）を仮引数そのものとその部分の破棄に限る、(3) 参照の変数の再借用が元の借用（値全体の借用）を親に持つ、(4) 使い終えた共有の持ち主を通した格納を元の所有者へブロックの型で届ける、(5) `Rc.get (ref r)` の借用を `r` の借用として解放の順序を検査する、(6) 参照の仮引数の `Rc` から作った共有を共有として扱わない。

## 着手条件と停止条件

- A31 が `main` に入っていること。着手時の `main` で「現状」のプログラムを両方の edition で検査し直し、結果が違えば、このチケットを直してから始めます。
- 項目ごとに、その項目の危険な双子（「現状」）と、下の「テスト計画」のコーパスで、`main` が拒否して対応後が受理する危険なプログラムが出ないことを確かめてから入れます。双子を拒否したまま受理できない項目と、`Rc.share` の連なり（A33 の「検査の時間」の `gen_chain.py cellshares`）の検査を n の 2 乗より悪くする項目は入れず、理由をこのチケットに書いて A33 に戻します。

## 現状

### 共通の前置きと確かめ方

各項目のプログラムは、同じ 14 行の前置き（下の最初のプログラムの `record Holder` から `fn cross` まで）で始まります（それだけで検査・実行できるよう、各プログラムに含めています）。`Holder` は解放のときにセルの関数を呼んで `drop <値>` を出力します。`look t` は `t` の最初の `9` の位置で、`String.repeat 4 (ref "123456789")` の文字列なら 8、解放した文字列を読むと多くは -1 になります。`cross a b` は `b.s` の借用を読む関数を `a.h` の `Mutex` へ書き込みます（A31 の書き込みの事実は `(1, Value, 0)` で、`b` の先の部分 `s` だけを残します: `StoreFacts::kept_paths`）。`PairOk` は `h` を `s` より先に宣言するので、値の中では `h` の `drop` が `s` より先に走ります。

確かめたバイナリは、A31 の先端（ブランチ `fix/a31-in-place-writes-keep-borrows`、`src/` が `6e4f0537` と同じもの。開発用のビルド）と `main`（`168ae4a5`、A31 の前）です。どちらも `check --edition 2027` と `check --edition 2026` で検査し、受理したものを `run -O0`・`run -O3` で実行しました（edition 2027 は AddressSanitizer のビルドの `-O0`・`-O3` も）。位置は最初の誤りの `行:列` で、解放の順序の誤り（`E1013` の `assignment_extends_borrow`・`self_referential_store`）は関数の本体の最初の文の位置を指します（検査はスコープの終わりに行い、本体の範囲を指すため）。値は手で求め、実行の出力と一致しました。

| 項目 | 安全な形: A31 の先端 | 安全な形: `main` | 危険な双子: A31 の先端 | 危険な双子: `main` |
| --- | --- | --- | --- | --- |
| `rcgetpart` | `17:5` `E1013`（`self_referential_store`） | 受理、`drop 8`・`drop 0`・`1` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1`、ASan は heap-use-after-free |
| `fvdrop` | `20:5` `E1013`（`self_referential_store`） | 受理、`drop 0`・`1` | `10:5` `E1013`（`assignment_extends_borrow`） | 受理、`0`（正しくは 9） |
| `refsrc` | `21:18` `E1014`（`borrow_conflict`） | 受理、`drop 0`・`drop 8`・`1` | `21:18` `E1014`（`borrow_conflict`） | 受理、`drop -1`・`drop 0`・`1` |
| `sharetype` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 8`・`drop 0`・`drop 0`・`1` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1` |
| `rcgetowner` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 8`・`drop 0`・`1` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1`、ASan は heap-use-after-free |
| `shareparam` | `19:5` `E1013`（`self_referential_store`） | 受理、`drop 8`・`drop 0`・`1` | `19:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1` |

どれも edition 2027 と 2026 で同じ結果です（`fvdrop` の一部の形は edition 2027 だけ。下の各項目）。`main` の安全な形は、edition 2027 の ASan のビルドの `-O0`・`-O3` でも同じ値で、ASan は何も報告しませんでした。危険な双子で ASan が報告するのは、解放した `Rc` のブロックを読む形です（ほかの形は、`main` の出力が解放した文字列を読んだ値になりますが、ASan は報告しませんでした）。

### `rcgetpart`: フィールドや要素の場所の `Rc.get` を束縛した変数

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let rs = [Rc.new (mkpo ())]
    let p2 = mkpo ()
    let b = Rc.get (ref rs[0])
    cross (ref p2) b
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `17:5` の `E1013`（`self_referential_store`、`'rs' keeps a borrow of a part of itself, ...`。注記 `a part of 'rs' is borrowed here` は `20:20` の `b`）。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`1`。手で求めた値: 局所値は宣言の逆順に解放されます。`p2` の `h` が `rs[0]` のブロックの `s` を読んで `drop 8`、`rs` が最後の持ち主としてブロックを解放し、その `h` の元の関数が `drop 0`、最後に `run` の結果 `1` です。
- 同じく拒否する形（両方の edition。`main` は受理して同じ値）: フィールドの `Rc`（`record RB { r: Rc<PairOk> }` の `let rb = RB { r: Rc.new (mkpo ()) }` と `let b = Rc.get (ref rb.r)`。`18:5` の `self_referential_store`）、パイプライン `b |> cross (ref p2)` と部分適用 `let g = cross (ref p2)` の後の `g b`（`17:5` の `self_referential_store`）、`let a = ref p2` の後のラムダの捕捉 `let g = \(_u: unit) -> cross a b` と `g ()`（`17:5` の `assignment_extends_borrow`）、同じブロックの自分の部分を書き込む `p2` の無い形 `cross (Rc.get (ref rs[0])) b`（`17:5` の `self_referential_store`。`main` は `drop 8`・`1`）。
- 受理する形（A31 の先端と `main` が `drop 8`・`drop 0`・`1`）: 引数にそのまま書いた `cross (ref p2) (Rc.get (ref rs[0]))`、変数全体の `Rc` の `let r = Rc.new (mkpo ())` の後の `let b = Rc.get (ref r)`。

危険な双子 1（`rs` と `p2` の宣言を入れ替えたもの。`p2` の `drop` が読む `s` は、先に解放される `rs` のブロックにあります）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let p2 = mkpo ()
    let rs = [Rc.new (mkpo ())]
    let b = Rc.get (ref rs[0])
    cross (ref p2) b
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`、`'p2' keeps a borrow of a value that is dropped before it, ...`）。
- `main`: 両方の edition で受理し、`drop 0`・`drop -1`・`1`（`p2` の `h` が解放したブロックの文字列を読む）。edition 2027 の ASan のビルドは、`drop 0` の後に heap-use-after-free で止まります。

危険な双子 2（自分の部分の順序。前置きの後に次を書いたもの。`s` を `h` より先に宣言した `PairBad` では、ブロックの解放で `s` が先に解放され、その後の `h` の `drop` が読みます）:

```tsuzuri
record PairBad { s: string, h: Holder }
def mkpb :: unit -> PairBad
fn mkpb _u = PairBad { s: String.repeat 4 (ref "123456789"), h: Holder { cell: Mutex.create [\() -> 0] } }
def crossb :: ref PairBad -> ref PairBad -> unit
fn crossb a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let rs = [Rc.new (mkpb ())]
    let b = Rc.get (ref rs[0])
    crossb (Rc.get (ref rs[0])) b
    n
do! IO.write_line (to_string (run 1))
```

A31 の先端は両方の edition で `22:5` の `E1013`（`self_referential_store`）、`main` は受理して `drop -1`・`1` で、ASan は heap-use-after-free です。

危険な双子 3（edition 2027。安全な形を `let mut rs` にし、`cross` の後に `rs[0] = Rc.new (mkpo ())` を足したもの。置き換えで古いブロックが解放されます）: A31 の先端は `21:5` の `E1014`（`borrow_conflict`）、`main` は受理して `drop 0`・`drop -1`・`drop 0`・`1` で、ASan は heap-use-after-free です。edition 2026 では、どちらも `21:5` の `E1012`（`assign_to_immutable_place`）です。

### `fvdrop`: 受け取った関数値を持つ値の破棄

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record FR { f: ref PairOk -> unit, k: i64 }
def keep :: (ref PairOk -> unit) -> unit
fn keep f = { let _r = FR { f: f, k: 1 }; () }
def run :: i64 -> i64
fn run n =
    let p2 = mkpo ()
    keep (cross (ref p2))
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `20:5` の `E1013`（`self_referential_store`、`'p2' keeps a borrow of a part of itself, ...`。注記は `21:17` の `ref p2`）。
- `main`: 両方の edition で受理し、`drop 0`・`1`。手で求めた値: `keep` は部分適用を入れたレコードを作って捨てるだけで、部分適用は参照を束縛しただけなので破棄で何も実行しません。`p2` の `h` の元の関数が `drop 0`、`run` の結果が `1` です。
- 同じく拒否する形（`main` は受理）: `fn keep f b = { let _r = FR { f: f, k: 1 }; f b }` を `keep (cross (ref p2)) (ref p1)` で呼ぶ形（`p1` を先に宣言。両方の edition で `20:5`、`main` は `drop 8`・`drop 0`・`1`）、`match` のパターンの変数（`fn keep f = match f with | _h -> ()`。両方の edition で `20:5`、`main` は `drop 0`・`1`）、型注釈を書いたラムダの仮引数（`fn apply f b = { let g = \(h: ref PairOk -> unit) -> h b; g f }` を `apply (cross (ref p2)) (ref p1)` で呼ぶ形。両方の edition で `19:5`、`main` は `drop 8`・`drop 0`・`1`）、edition 2027 の読み取りの仮引数を束縛した局所値（`fn keep f = { let _h = f; () }`。edition 2027 は `19:5`、edition 2026 は受理）、edition 2027 の読み取りの仮引数のフィールドを束縛した局所値（`def keep :: FR -> unit`、`fn keep r = { let _h = r.f; () }` を `keep (FR { f: cross (ref p2), k: 1 })` で呼ぶ形。edition 2027 は `20:5`、edition 2026 は受理）。位置の誤りはどれも `self_referential_store` です。
- 受理する形（両方の edition。A31 の先端と `main` が `drop 0`・`1`）: 仮引数をそのまま破棄する `fn keep f = ()`、`own` の仮引数を束縛して破棄する `def keep :: own (ref PairOk -> unit) -> unit`・`fn keep f = { let _h = f; () }`。

危険な双子（関数の中で作ったリテラルの型が自分の `Drop` を持ち、その `drop` が受け取った関数値を共有のセルへ書き込む形。前置きは使いません）:

```tsuzuri
record Stash { cell: Arc<Mutex<[unit -> i64]>>, f: unit -> i64 }
instance Drop<Stash> {
    fn drop s = { let g = s.f; Mutex.with_lock (Arc.get (ref s.cell)) (\slot -> Array.write 0 g slot) }
}
def keep :: own Arc<Mutex<[unit -> i64]>> -> own (unit -> i64) -> unit
fn keep c f = { let _s = Stash { cell: c, f: f }; () }
def run :: i64 -> i64
fn run n =
    let a = Arc.new (Mutex.create [\() -> 0])
    { let text = to_string n; let t = ref text; keep (Arc.share (ref a)) (\() -> String.length t) }
    let other = [to_string 777777, to_string 88888888]
    Mutex.with_lock (Arc.get (ref a)) (\slot -> slot[0] ()) + other.length * 0
do! IO.write_line (to_string (run 123456789))
```

- A31 の先端: 両方の edition で `10:5` の `E1013`（`assignment_extends_borrow`、`assignment lets a reference outlive its owner`。注記は `10:11` の `text`）。
- `main`: 両方の edition で受理し、`0` を出力します（`keep` の中の `_s` の解放が `text` の借用を読む関数を `a` のセルへ書き込み、内側のブロックの終わりで `text` が解放された後に `run` がそれを呼ぶ。`text` が生きていれば 9）。edition 2027 の ASan のビルドは何も報告しません。

`fvdrop` の修正は、破棄する値の型自身の `drop` がコードを実行する形（この `Stash`）を、普通の格納のまま残す必要があります。呼び出し元の実引数が破棄でコードを実行しうる関数値のときは、今の規則（`value_runs_drop_code`）のとおり drop-only の格納も当てはめます。その形の双子（`Drop` を持つ値を束縛した部分適用 `stashf (Stash { ... })` を、`FR` のリテラルを作る `keep` へ渡す）は、型の検査が両方の edition で `E1005`（`capture_exclusive_in_reusable_function`）で拒否するので作れませんでした。

### `refsrc`: 書き込む部分の側の参照の変数が値全体を借用したままにする

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let p1 = mkpo ()
    let p2 = mkpo ()
    let q1 = ref p1
    cross (ref p2) q1
    let _taken = p1.h
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `21:18` の `E1014`（`borrow_conflict`。注記 `value borrowed here` は `19:14` の `ref p1`、help は `stop using 'p2' first`）。
- `main`: 両方の edition で受理し、`drop 0`・`drop 8`・`1`。手で求めた値: 最後に宣言した `_taken`（`p1` から取り出した `h`）が先に解放されて `drop 0`、`p2` の `h` が `p1.s` を読んで `drop 8`、`p1` は `s` だけが残っていて何も出力せず、`run` の結果が `1` です。
- 同じく拒否する形（両方の edition。`main` は受理して同じ値）: パイプライン `q1 |> cross (ref p2)`（`21:18`）、参照の仮引数として受け取る関数 `fn outer a b = cross a b` へ渡す `outer (ref p2) q1`（`23:18`）、再借用を束縛した変数のラムダの捕捉 `let q0 = ref p1`・`let q1 = ref (deref q0)`・`let q2 = ref p2` の後の `let g = \(_u: unit) -> cross q2 q1` と `g ()`（`24:18`）。どれも `E1014`（`borrow_conflict`）です。
- 受理する形（A31 の先端と `main` が `drop 0`・`drop 8`・`1`）: 呼び出しの位置で書いた `cross (ref p2) (ref p1)`。

危険な双子（借用された部分 `s` を取り出したもの。取り出した `s` が先に解放され、その後の `p2` の `drop` が読みます）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let p1 = mkpo ()
    let p2 = mkpo ()
    let q1 = ref p1
    cross (ref p2) q1
    let _taken = p1.s
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `21:18` の `E1014`（`borrow_conflict`）。
- `main`: 両方の edition で受理し、`drop -1`・`drop 0`・`1`。edition 2027 の ASan のビルドは何も報告しません。

### `sharetype`: 使い終えた共有の持ち主を通した格納を、元の所有者へブロックの型で届ける

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkpo ())
    let p = mkpo ()
    let r = Rc.share (ref r2)
    let a = Rc.get (ref r)
    let q = Rc.new (mkpo ())
    cross (Rc.get (ref q)) (ref p)
    look (ref a.s) * 0 + n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`、`'r2' keeps a borrow of a value that is dropped before it, ...`。注記は、`18:9` の `p` が `r2` の後に宣言されたこと、`22:28` の `ref p`）。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`drop 0`・`1`。手で求めた値: `q` が自分のブロックの最後の持ち主として解放され、その `h` が `p.s` を読んで `drop 8`、`r` の解放は `r2` のブロックの持ち主を 1 つ減らすだけで、`p` の `h` が `drop 0`、`r2` がブロックを解放してその `h` が `drop 0`、`run` の結果が `1` です。`q` のブロックは `r2` のブロックと無関係です。
- 同じく拒否する形（両方の edition、`main` は受理して同じ値）: ブロックでない値（`record W { h: Holder, keep: Maybe<Rc<PairOk>> }` の `let w = W { h: Holder { cell: Mutex.create [\() -> 0] }, keep: Maybe.None }` の `h` へ書き込む `crossw (ref w) (ref p)`。`20:5`）。`[Rc<PairOk>]` のフィールドを持つレコード（`record HM { h: Holder, rs: [Rc<PairOk>] }` の `rs` が空の値）、`Rc<PairOk>` を中に持つ別の型のブロック（`record Big { h: Holder, inner: Rc<PairOk> }` の `Rc<Big>`）、共有したブロックが中に持ちうる型の無関係なブロック（`record Tree { h: Holder, s: string, child: Maybe<Rc<Leaf>> }` を共有した後の、別の `Rc<Leaf>`）への書き込みも同じ規則で、A31 の第5回のレビューのプログラムで、A31 の先端が両方の edition で `E1013`（`assignment_extends_borrow`）であることを確かめました。
- 受理する形（A31 の先端と `main` が `drop 8`・`drop 0`・`drop 0`・`1`）: 共有を作らない形（`let r = Rc.share (ref r2)` と `let a = Rc.get (ref r)` を除き、結果を `n` にしたもの）。

危険な双子 1（共有したブロックそのものへ書き込むもの。`p` が解放された後で、`r2` が最後の持ち主としてブロックを解放し、その `h` が `p.s` を読みます）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkpo ())
    let p = mkpo ()
    let r = Rc.share (ref r2)
    let a = Rc.get (ref r)
    cross a (ref p)
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`）。
- `main`: 両方の edition で受理し、`drop 0`・`drop -1`・`1`。edition 2027 の ASan のビルドは何も報告しません。

危険な双子 2（共有を別の変数へムーブしてから書き込むもの。双子 1 の `let a = Rc.get (ref r)` と `cross a (ref p)` を `let q = r` と `cross (Rc.get (ref q)) (ref p)` にしたもの）: A31 の先端は両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`）、`main` は受理して `drop 0`・`drop -1`・`1` です。

#### `sharetype` に A31 のレビューの第6回で足した形

A31 の Copilot のレビューの後のレビューの第6回（[A31 のチケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第6回」）で、`sharetype` の規則の実際の範囲を確かめ、dyn の値と型引数を通した危険な形を塞いだので、次の形もこの項目に入ります。確かめたバイナリは A31 の先端（`src/` が第6回の修正 `5e99c1c0` と同じもの）と `main` です。前置きは上と同じ 14 行で、`Other`（`Holder` と数を持つレコード）・`mkother`・`crossother`（`cross` の `Other` 版）の 5 行が続きます。

元の所有者のほかの部分が持ちうるブロック（`r2` がレコードで、共有したブロックとは別のフィールドにもブロックを持つ。届け先の型は、共有したブロックとその中のブロックの型だけでなく、`r` の型と `r2` の型の両方が持ちうるセルを持つブロックの型のすべてです）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record RS { rc: Rc<PairOk>, other: Rc<Other> }
record RH2 { rc: Rc<PairOk>, other: Rc<Other> }
def run :: i64 -> i64
fn run n =
    let r2 = RS { rc: Rc.new (mkpo ()), other: Rc.new (mkother 1) }
    let p = mkpo ()
    let h = RH2 { rc: Rc.share (ref r2.rc), other: Rc.new (mkother 2) }
    let a = Rc.get (ref h.rc)
    let o = Rc.new (mkother 3)
    crossother (Rc.get (ref o)) (ref p)
    look (ref a.s) * 0 + n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `24:5` の `E1013`（`assignment_extends_borrow`）。第5回の先端と第4回の先端も同じです。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`drop 0`・`drop 0`・`drop 0`・`1`（native と、edition 2027 の ASan のビルドの `-O0`・`-O3`。ASan は報告なし）。手で求めた値: `o` が自分のブロックの最後の持ち主として解放され、その `h` が `p.s` を読んで `drop 8`、`h` の解放で `rc` のブロックの持ち主が 1 つ減り、`h.other` のブロック（`mkother 2`）が解放されて `drop 0`、`p` の `h` が `drop 0`、`r2` の `rc` のブロックと `other` のブロック（`mkother 1`）が解放されて `drop 0`・`drop 0`、`run` の結果が `1` です。`o` のブロックは `r2` のどのブロックとも無関係です。
- 受理する形（A31 の先端）: `h` を `Rc.share (ref r2.rc)` だけにした形（`r` の型が `Rc<Other>` に届かない）と、`r2` を `Rc.new (mkpo ())` にした形（`r2` の型が `Rc<Other>` に届かない）。

危険な双子（`r2` の `other` のブロックの共有を通してそのブロックへ書き込むもの。`p` が解放された後で `r2` が `mkother 1` のブロックを解放し、その `h` が `p.s` を読みます）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record RS { rc: Rc<PairOk>, other: Rc<Other> }
record RH2 { rc: Rc<PairOk>, other: Rc<Other> }
def run :: i64 -> i64
fn run n =
    let r2 = RS { rc: Rc.new (mkpo ()), other: Rc.new (mkother 1) }
    let p = mkpo ()
    let h = RH2 { rc: Rc.share (ref r2.rc), other: Rc.share (ref r2.other) }
    let a = Rc.get (ref h.other)
    crossother a (ref p)
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `24:5` の `E1013`（`assignment_extends_borrow`）。
- `main`: 両方の edition で受理し、`drop 0`・`drop 0`・`drop -1`・`1`。edition 2027 の ASan のビルドは何も報告しません。

dyn の値と型引数の値は、型が中身を示さず、型の走査（`TypeContext::reaches`）もその中を見ません。dyn の値のメソッドは中身の場所の参照を返せる（インスタンスの `fn leaf b = ref b.l`）ので、`Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))` は、`r2` の型からは見えない `r2` のブロックの共有です。第5回の狭め（`r` の型と `r2` の型の両方が届くブロックの型だけを届ける）はこの共有を通した格納を `r2` へ届けず、`main` も受理する解放後の読み出しを受理していた（前の先端からの退行）ので、第6回で、`r2` の型が dyn の値か型引数に届くなら狭めず、`r` の型が届くなら dyn の値か型引数かセルを持つブロックに届く型の値への格納をすべて `r2` へ届け、dyn の値か型引数に届く型の値への格納はブロックの共有が届いたどの先へも届けるようにしました。次の 3 つの形はそれによる安全側の拒否です。どれも同じ `Cls.tt` と、上の前置きと `Other` の 5 行に `Leaf`・`mkleaf`・`crossleaf`・`Box1` とそのインスタンスの 9 行が続く前置きを使います。

```tsuzuri
class LeafOf<'value> {
    def leaf :: ref 'value -> ref Rc<Main.Leaf>
}
class OtherOf<'value> {
    def other :: ref 'value -> ref Main.Other
}
```

共有を持つ値の型が dyn の値に届く形（`h` は `r2` のブロックの共有を dyn の値の中に持つ。その後の無関係な `Rc<Other>` への書き込みが `r2` へ届く）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Leaf { h: Holder, n: i64 }
def mkleaf :: i64 -> Leaf
fn mkleaf k = Leaf { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossleaf :: ref Leaf -> ref PairOk -> unit
fn crossleaf w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Box1 { l: Rc<Leaf> }
instance Cls.LeafOf<Box1> {
    fn leaf b = ref b.l
}
record H {r} { d: dyn Cls.LeafOf {r} }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkleaf 1)
    let p = mkpo ()
    let h = H { d: Dyn.of (Box1 { l: Rc.share (ref r2) }) }
    let a = Rc.get (Cls.LeafOf.leaf (ref h.d))
    let o = Rc.new (mkother 3)
    crossother (Rc.get (ref o)) (ref p)
    a.n * 0 + n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `32:5` の `E1013`（`assignment_extends_borrow`）。第5回の先端と第4回の先端は受理します。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`drop 0`・`1`（native と ASan の `-O0`・`-O3`。ASan は報告なし）。手で求めた値: `o` の `h` が `p.s` を読んで `drop 8`、`h` の解放は `Leaf` のブロックの持ち主を 1 つ減らすだけで、`p` の `h` が `drop 0`、`r2` が `Leaf` のブロックを解放してその `h` が `drop 0`、`run` の結果が `1` です。

危険な双子（dyn の値の中の共有を通して `r2` のブロックへ書き込むもの。A31 の第6回の前の先端と `main` が受理していた、`main` からある穴です）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Leaf { h: Holder, n: i64 }
def mkleaf :: i64 -> Leaf
fn mkleaf k = Leaf { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossleaf :: ref Leaf -> ref PairOk -> unit
fn crossleaf w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Box1 { l: Rc<Leaf> }
instance Cls.LeafOf<Box1> {
    fn leaf b = ref b.l
}
record H {r} { d: dyn Cls.LeafOf {r} }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkleaf 1)
    let p = mkpo ()
    let h = H { d: Dyn.of (Box1 { l: Rc.share (ref r2) }) }
    let a = Rc.get (Cls.LeafOf.leaf (ref h.d))
    crossleaf a (ref p)
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `32:5` の `E1013`（`assignment_extends_borrow`）。
- `main`: 両方の edition で受理し、`drop 0`・`drop -1`・`1`（`look` を `t.length` にした形は、ASan のビルドで heap-use-after-free。A31 のワークツリーの `target/scratch/a31/rr6/n/u_hrecdyn`）。

元の所有者の型が dyn の値に届く形（`r2` の dyn のフィールドが持つ `Leaf` のブロックを共有した `h` が、無関係な `Rc<Other>` も持つ。その後の無関係な `Rc<Other>` への書き込みが `r2` へ届く）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Leaf { h: Holder, n: i64 }
def mkleaf :: i64 -> Leaf
fn mkleaf k = Leaf { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossleaf :: ref Leaf -> ref PairOk -> unit
fn crossleaf w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Box1 { l: Rc<Leaf> }
instance Cls.LeafOf<Box1> {
    fn leaf b = ref b.l
}
record B {r} { h: Holder, d: dyn Cls.LeafOf {r} }
record RL { l: Rc<Leaf>, other: Rc<Other> }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (B { h: Holder { cell: Mutex.create [\() -> 0] }, d: Dyn.of (Box1 { l: Rc.new (mkleaf 1) }) })
    let p = mkpo ()
    let h = RL { l: Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d)), other: Rc.new (mkother 2) }
    let a = Rc.get (ref h.l)
    let o = Rc.new (mkother 3)
    crossother (Rc.get (ref o)) (ref p)
    a.n * 0 + n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `33:5` の `E1013`（`assignment_extends_borrow`）。第4回の先端も拒否し、第5回の先端は受理します。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`drop 0`・`drop 0`・`drop 0`・`1`（native と ASan の `-O0`・`-O3`。ASan は報告なし）。
- 危険な双子（`h = Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))` の `Rc.get (ref h)` へ `crossleaf` で書き込むもの。A31 のレビューの第6回の指摘 1 の形）: A31 の先端は両方の edition で `32:5` の `E1013`（`assignment_extends_borrow`）、`main` は受理して `drop 0`・`drop 0`・`drop -1`・`1` です。

dyn の値を持つ無関係な値への書き込み（共有に dyn の値は無い。使い終えた共有の後で、dyn の値を持つ局所値 `w` の中の `Other` へ書き込む）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Other { h: Holder, n: i64 }
def mkother :: i64 -> Other
fn mkother k = Other { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossother :: ref Other -> ref PairOk -> unit
fn crossother w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Leaf { h: Holder, n: i64 }
def mkleaf :: i64 -> Leaf
fn mkleaf k = Leaf { h: Holder { cell: Mutex.create [\() -> 0] }, n: k }
def crossleaf :: ref Leaf -> ref PairOk -> unit
fn crossleaf w b = { let t = ref b.s; Mutex.with_lock (ref w.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
record Box1 { l: Rc<Leaf> }
instance Cls.LeafOf<Box1> {
    fn leaf b = ref b.l
}
record Box2 { o: Other }
instance Cls.OtherOf<Box2> {
    fn other b = ref b.o
}
record WD {r} { d: dyn Cls.OtherOf {r} }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkpo ())
    let p = mkpo ()
    let r = Rc.share (ref r2)
    let a = Rc.get (ref r)
    let w = WD { d: Dyn.of (Box2 { o: mkother 3 }) }
    crossother (Cls.OtherOf.other (ref w.d)) (ref p)
    look (ref a.s) * 0 + n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `36:5` の `E1013`（`assignment_extends_borrow`）。第5回の先端と第4回の先端は受理します。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`drop 0`・`1`（native と ASan の `-O0`・`-O3`。ASan は報告なし）。
- 同じく拒否する形: `r2` の共有を呼び出し元の配列へ入れた関数の中で、dyn の値を持つ無関係な局所値へ書き込む形は、`E1013`（`store_local_borrow_through_reference`）です（`main` は受理して `drop 0`・`drop 8`・`drop 0`・`drop 0`・`1`。A31 のワークツリーの `target/scratch/a31/rr6/lim/s_dynstore_ext`）。
- 危険な双子（共有の共有 `Rc.share (ref h)` を dyn の値に入れた `g` を通して `r2` のブロックへ書き込むもの。`g` は `h` の共有だけを持ち、`h` から `r2` への道は読み替えで消えているので、`g` への格納は型の判定で `r2` へ届ける必要があります）: A31 の先端は両方の edition で `32:5` の `E1013`（`assignment_extends_borrow`）、`main` は受理して `drop 0`・`drop -1`・`1` です（A31 の第6回の前の先端も受理していた、`main` からある穴です）。

### `rcgetowner`: `Rc.get (ref r)` の借用を `r` の借用として解放の順序を検査する

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkpo ())
    let p = mkpo ()
    let r = Rc.share (ref r2)
    let a = Rc.get (ref r)
    cross (ref p) a
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`、`'p' keeps a borrow of a value that is dropped before it, ...`。注記は、`19:9` の `r` が `p` の後に宣言されたこと、`21:19` の `a`）。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`1`。手で求めた値: `r` の解放はブロックの持ち主を 1 つ減らすだけで、`p` の `h` が `r2` の生かすブロックの `s` を読んで `drop 8`、`r2` がブロックを解放してその `h` が `drop 0`、`run` の結果が `1` です。
- 同じく拒否する形（両方の edition、`main` は受理）: ブロックの自分の部分を別の持ち主 `r3` を通して書き込んだ後で、ブロックを `r3` より前に宣言した変数へムーブする形（`let mut hold = Rc.new (mkpo ())`・`let r = Rc.new (mkpo ())`・`let r3 = Rc.share (ref r)`・`let b = Rc.get (ref r3)`・`let a = Rc.get (ref r)`・`cross a b`・`hold = r`）。`17:5` の `E1013`（`'hold' keeps a borrow of a value that is dropped before it`、注記は `r3`）で、続けて `23:12` の `E1014`（ブロックが自分の部分の借用を持つ `Rc` のムーブ。A33 の `r07`・`r10`）です。`main` は `drop 0`・`drop 8`・`1`。
- 受理する形（A31 の先端と `main` が `drop 8`・`drop 0`・`1`）: 借用を `r2` から得る `let a = Rc.get (ref r2)`（`r` を作らない形）。

危険な双子 1（`p` を `r2` より先に宣言したもの。`r` と `r2` が `p` より先に解放され、ブロックが解放された後で `p` の `h` が読みます）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def run :: i64 -> i64
fn run n =
    let p = mkpo ()
    let r2 = Rc.new (mkpo ())
    let r = Rc.share (ref r2)
    let a = Rc.get (ref r)
    cross (ref p) a
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`、安全な形と同じ文）。
- `main`: 両方の edition で受理し、`drop 0`・`drop -1`・`1`。edition 2027 の ASan のビルドは、`drop 0` の後に heap-use-after-free で止まります。

危険な双子 2（安全な形の `cross (ref p) a` の後に `let k = { let z = r2; 0 }` を足し、結果を `n + k` にしたもの。`r2` を内側のブロックへムーブして先に手放すので、`r` が最後の持ち主になり、`p` より先にブロックを解放します）: A31 の先端は両方の edition で `17:5` の `E1013`（`assignment_extends_borrow`）、`main` は受理して `drop 0`・`drop -1`・`1` で、ASan は heap-use-after-free です。

### `shareparam`: 参照の仮引数の `Rc` から作った共有

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def dup :: ref Rc<PairOk> -> Rc<PairOk>
fn dup r = Rc.share r
def run :: i64 -> i64
fn run n =
    let p = mkpo ()
    let r2 = Rc.new (mkpo ())
    let r = dup (ref r2)
    let a = Rc.get (ref r)
    cross a (ref p)
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `19:5` の `E1013`（`self_referential_store`、`'r2' keeps a borrow of a part of itself, ...`。注記は `21:17` の `ref r2`）。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`1`。手で求めた値: `r` の解放はブロックの持ち主を 1 つ減らすだけで、`r2` がブロックを解放してその `h` が `p.s` を読んで `drop 8`、`p` の `h` が `drop 0`、`run` の結果が `1` です。
- 同じく拒否する形（両方の edition で `19:5` の `self_referential_store`。`main` は受理）: 書き込みの無い `let r = dup (ref r2)` だけの形（`p`・`a`・`cross` を除き、結果を `look (ref (Rc.get (ref r)).s) * 0 + n` にしたもの。`main` は `drop 0`・`1`）、共有を作って捨てる関数 `fn sh2 r = { let s = Rc.share r; look (ref (Rc.get (ref s)).s) }` の `sh2 (ref r2) * 0 + n`（`main` は `drop 0`・`1`）、共有を通して書き込む関数 `fn stashs r b = { let s = Rc.share r; cross (Rc.get (ref s)) b }` の `stashs (ref r2) (ref p)`（`p` を先に宣言。`main` は `drop 8`・`drop 0`・`1`）。
- 受理する形（A31 の先端と `main` が `drop 8`・`drop 0`・`1`）: 呼び出し元で作った共有 `let r = Rc.share (ref r2)`。

危険な双子 1（`p` を `r2` の後に宣言したもの。`p` が解放された後で、`r2` が最後の持ち主としてブロックを解放し、その `h` が `p.s` を読みます）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())))
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
record PairOk { h: Holder, s: string }
def mkpo :: unit -> PairOk
fn mkpo _u = PairOk { h: Holder { cell: Mutex.create [\() -> 0] }, s: String.repeat 4 (ref "123456789") }
def cross :: ref PairOk -> ref PairOk -> unit
fn cross a b = { let t = ref b.s; Mutex.with_lock (ref a.h.cell) (\c -> Array.write 0 (\() -> look t) c) }
def dup :: ref Rc<PairOk> -> Rc<PairOk>
fn dup r = Rc.share r
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (mkpo ())
    let p = mkpo ()
    let r = dup (ref r2)
    let a = Rc.get (ref r)
    cross a (ref p)
    n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `19:5` の `E1013`（`assignment_extends_borrow`、`'r2' keeps a borrow of a value that is dropped before it, ...`）。
- `main`: 両方の edition で受理し、`drop 0`・`drop -1`・`1`。edition 2027 の ASan のビルドは何も報告しません。

危険な双子 2（関数の中の共有を通して書き込む `stashs` の形で、`p` を `r2` の後に宣言したもの）: A31 の先端は両方の edition で `19:5` の `E1013`（`assignment_extends_borrow`）、`main` は受理して `drop 0`・`drop -1`・`1` です。

## 原因

関数名は `src/ownership.rs` のものです。

- **`rcgetpart`**: `Checker::eval` の `E::Borrow` の腕は、借用した場所が借用した部分そのものであること（`exact_place`）に加えて、場所の型が借用の型と同じこと（`typed`: `place_type(&place)` が `without_views(&value.ty)` と同じ）を求め、両方が成り立つ loan だけを `exact_loans` に入れます。`place_type` は場所をたどるたびに `Rc`・`Arc` を読み通し、最後の段の後にも読み通す（`unwrap(current)`）ので、`ref rs[0]`（借用の型は `Rc<PairOk>`）の場所の型は `PairOk` になって合わず、loan は exact になりません。`exact_loan` はフィールドの無い場所（変数全体の `ref r`）を `exact_loans` なしで exact とみなすので、変数全体の形は受理されます。その loan を持つ局所値 `b` を書き込む値の側へ渡すと、`kept_copy` は exact な loan だけを `kept_paths` の部分（`.s`）の借用に置き換えるので、`p2` は `rs[0]` の場所全体の借用を持ち、その後の格納は `apply_store_within` がその借用をたどって `rs` の中のセル（`store_target`）へ届けます。`rs` が自分の借用を持つものになり、`check_drop_order_of` が `self_referential_store` を報告します。ラムダの捕捉と部分適用の引数は、`bound_value` が exact な loan だけを引数の値とし、ほかは `None`（関数値の値全体）にします。引数にそのまま書いた `Rc.get (ref rs[0])` が受理される道（`points_at` は `Rc.get` の引数の借用を `exact_place` で判断します）は、着手時に確かめます。
- **`fvdrop`**: `Checker::drop_stores` は、破棄でコードを実行しうる値の破棄を、その値の loan をその値の書き込み先へ格納するもの（`apply_store(value, value)`）として扱い、破棄した値が仮引数の一部（`parameter_part`: 不変で参照でない仮引数、そのフィールド・要素）なら、その格納を `StoreFacts::drop_only` に記録します。呼び出し元の `apply_facts_once` は、drop-only の格納を、実引数の式が破棄でコードを実行しうるとき（`value_runs_drop_code`）だけ当てはめます。`parameter_part` は `E::Local`・`E::Field`・`E::Index` しか見ないので、リテラル（`FR { f: f, k: 1 }`、`[f]`、`(f, 1)`、`Maybe.Some f`）、パターンの変数（型注釈を書いたラムダの仮引数は `\$lambdaN -> match $lambdaN with h -> ...` に下ろされます）、edition 2027 の読み取りの仮引数（型が参照）を束縛した局所値の破棄は `None` になり、普通の格納として記録されます。呼び出し元は、`cross (ref p2)` が破棄で何も実行しないことを使えず、`(0, Value, 0)` を当てはめて `p2` の借用を `p2` 自身のセルへ入れます。
- **`refsrc`**: 呼び出しは、場所の借用でない参照の実引数 `q1` を `ref (deref q1)` として再借用します。`Checker::loan` は借用する場所の根の局所値が持つ loan を新しい loan の親に足す（`parents.extend(&value.loans)`）ので、再借用の loan は `q1` が持つ `p1` 全体の借用を親に持ちます。`kept_copy` は `kept_paths` の部分（`p1.s`）の写しを作りますが、写しは元の親を持つ（呼び出し元が前に格納した元の loan と同じく、借用した値が持つものを生かすため）ので、`p2` が生きている間は `p1` 全体の借用が生きたままになり、`p1.h` のムーブが `E1014` になります。親が「借用した値が持つもの」ではなく「同じ値の外側の借用」であることを区別していません（`read_through_loan` は、同じ場所の再借用を親として読む場合を区別しています）。ラムダの捕捉は `Checker::captured_loans` で狭めますが、再借用を束縛した局所値（`let q1 = ref (deref q0)`）の loan も同じく元の借用を親に持ちます。
- **`sharetype`**: `keep_for_drop` は、破棄でコードを実行しうる局所値のうち本文がもう使わないもの（`let a = Rc.get (ref r)` の後の `r`）が持つ共有の loan（`Loan::share`）を、元の所有者 `r2` が今持つ loan に読み替えます（`expand_shares`）。読み替えで `r` から `r2` への道が消えるので、`share_sources` の元の所有者へ、`export_shared_block(r, r2)` がブロックの型（`r` の型と `r2` の型の両方が届く、セルを持つブロックの型。`export_walk`）を届け先として記録し、以後の格納は `store_exported` → `store_exported_into` が、格納先の局所値の型がその型に届けば `r2` へも入れます。型の走査は dyn の値と型引数の中を見ないので（`walk_hides_blocks`）、`r2` の型がそうした値に届くなら狭めずに `r` の型が届くすべての型を、`r` の型が届くなら印 `HIDDEN_BLOCK`（dyn の値か型引数かセルを持つブロックに届く型の値への格納に当たる）を記録し、`store_exported_into` は、格納先の型が dyn の値か型引数に届けば、すべての届け先へ入れます（第6回）。判定は型だけなので、`r2` のブロックを持ちえない値（新しく作った `Rc.new (mkpo ())`、`keep` が `Maybe.None` の `W`）への格納も `r2` に入ります。事実は書き込み先の仮引数だけを記録し、その中のどの部分へ書き込んだかを記録しないので、呼び出し元はブロックでない部分（`w.h`）への格納を型で除くこともできません。局所値ごとの正確な道（読み替えた共有の loan を記録し、`apply_store_within` がたどる）は、`Rc.share` の連なりで検査が 3 乗になったので、A31 では採っていません（A31 のチケットの「レビューの第4回」）。
- **`rcgetowner`**: `Rc.get (ref r)` の結果は `ref r` の loan（場所の根は `r`）を持ちます。`check_drop_order_of` は、局所値が持つ loan の根の位置で解放の順序を判断し、根 `r` が `p` より後に宣言されていれば `borrow_dropped_first` で `assignment_extends_borrow` にします。同じブロックの持ち主の組（`State::block_shares`、`block_groups`）を見る `same_block_loan` は、借用を持つ値自身が同じブロックの持ち主のときだけ例外にするので、ブロックを持たない `p` には、組のほかの持ち主（`r2`）がブロックを生かすことが使われません。
- **`shareparam`**: `share_loans` は、`Rc.share`・`Arc.share` の結果が持つ loan を共有の印（`Loan::share`）を付けた写しにしますが、根が外部（仮引数）の loan は、`Drop.drop` の中の破棄する値（`drop_root`）を除いて印を付けません。そのため `fn dup r = Rc.share r` の結果と `let s = Rc.share r` は仮引数 `r` の先の借用のままで、その破棄（`drop_stores`。どれかの `Drop.drop` が書き込みうるプログラム、`facts.drops_write` だけ）は、その借用を共有したブロックのセルへ格納するものになります。呼び出し元では `dup (ref r2)` の結果が `r2` の借用で、その破棄が `r2` の借用を `r2` のブロックのセルへ入れ、`sh2 (ref r2)` は事実 `(0, Value, 0)` で同じことをし、`r2` が自分自身の借用を持つものとして `self_referential_store` になります。呼び出し元で書いた `Rc.share (ref r2)` は、根が局所値なので共有として扱われます（`note_block_share` も記録します）。

## 期待する振る舞い

- 「現状」の各項目の安全な形（主なプログラムと「同じく拒否する形」）を、書いた edition で受理し、`main` と同じ値（手で求めた値）を出す。
- 「現状」の危険な双子（主なプログラムと、文で書いた双子）を、両方の edition で、同じ code と kind で拒否する（`E1013` の `assignment_extends_borrow`・`self_referential_store`、`E1014` の `borrow_conflict`。edition 2026 の `rcgetpart` の双子 3 は `E1012` のまま）。位置が変わるなら、このチケットに記録する。
- A31 のテスト（`tests/rc.rs`・`tests/mutable_slices.rs`・`tests/currying.rs`・`tests/borrowed_records.rs`・`tests/user_drop.rs` など）と E2E（`in_place_writes`・`mutable_slices`・`rc`・`borrowed_records`・`conventions`・`control`）の期待は変えない。例外は、この 6 つの制限の拒否を固定したテストで、受理に変える前にこのチケットに記録する: `tests/rc.rs` の `a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block` の最後の `rejects_at`（`sharetype` の `W` の形、`50:5`）。
- 直せない項目が残るなら、その理由と残る形を文書（`drop.md`・`lifetimes.md`）とこのチケットに書き、A33 に戻す。

## 方針案

項目ごとの案です。どれも、その項目の危険な双子を拒否したままにすることを先に確かめるテストから始めます。

### `rcgetpart`

- `E::Borrow` の `typed` を、場所の最後の段の後の `Rc`・`Arc` を読み通さない型（`place_type` から最後の `unwrap` を除いたもの、または借用の型の `Rc`・`Arc` を外した型との比較）で判断し、`ref rs[0]`・`ref rb.r`・`ref x.a.e[0]` の loan を exact にする。loan の場所は `Rc` の要素（`rs[0]`）で、`kept_copy`・`path_copy` の写しの場所（`rs[0].s`）は `place_type` がブロックの中の値の部分として読みます。
- 先に、引数にそのまま書いた `Rc.get (ref rs[0])` が受理される道を確かめ、束縛した変数（`pointing_local`）とラムダの捕捉・部分適用（`bound_value`、`borrow_loans`）が同じ判断を使えるかを見る。
- 双子: 宣言の順序（双子 1）は loan の根 `rs` の位置で、自分の部分の順序（双子 2、`PairBad`）は exact になった loan の部分の順序（`ordered_self_borrow`、`self_borrow_dropped_first`）で、要素の置き換え（双子 3）は `rs[0]` と `rs[0].s` の重なりで、拒否のままになることを確かめる。`Rc` の要素の場所の借用を、別の持ち主（`Rc.share (ref rs[0])`）が持つブロックの借用として扱ってよいか（`same_block_loan` は局所値の持ち主だけを組にします）も確かめる。

### `fvdrop`

- `parameter_part` を、破棄する値が「仮引数の部分だけでできていて、それ以外に破棄で何も実行しない値」かを答える関数に広げる: 型自身の `Drop` を持たない（`has_own_drop` が偽）レコード・共用体のリテラルと、配列・リスト・タプルのリテラルで、要素がそれぞれ仮引数の部分か、破棄で何も実行しない値（`value_runs_drop_code` が偽）のもの。その破棄の格納は、仮引数の部分の要素ごとに drop-only として記録する（要素が 2 つの仮引数の部分なら、それぞれの `from` の組）。
- パターンの変数: 対象が仮引数の部分（と、それを `match` で取り出した部分）で、値として束縛した変数は、その仮引数の部分として扱う（パターンの変数を対象の別名として記録する `State::aliases` を使う）。型注釈を書いたラムダの仮引数は、持ち上げたラムダの仮引数 `$lambdaN` の部分として同じく扱えるかを確かめる（edition 2027 では `$lambdaN` が読み取りの仮引数になる）。
- edition 2027 の読み取りの仮引数: 関数値の読み取りの仮引数とそのフィールドの写し（`let h = f`、`let h = r.f`）の破棄は、呼び出し元の実引数の値（呼び出し元は値の式を書き、それを貸す）の写しの破棄なので、その実引数の式で `value_runs_drop_code` を判断できる。明示した `ref` の仮引数は、呼び出し元の式が参照（`ref fr`）で、その先の値で判断する必要があるので、この案の外に置く（`apply_facts_once` の `dropping_part` も参照を `None` にしています）。
- 双子: リテラルの型自身の `Drop` がコードを実行する形（`u_fvdrop` の `Stash`）は普通の格納のまま残し、実引数が破棄でコードを実行しうる関数値なら drop-only の格納も当てはめる今の規則を保つ。`join` が、どちらかが破棄以外で記録した組を drop-only から外す規則も保つ。

### `refsrc`

- `kept_copy`（と、必要なら `Checker::captured_loans` の狭め）で、写しの元の loan の親のうち、同じ値の外側の借用（親の場所が元の loan の場所と同じか、その上にあるもの: 参照の変数の再借用が足した親）を、同じ部分（`kept_paths` の道）へ狭めた写しに置き換える。借用した値が持つもの（親の場所が元の場所と別の値のもの）は、今と同じく親として残す。`read_through_loan` が同じ場所の再借用の親を読む規則と同じ区別を使う。
- 双子: 借用された部分 `p1.s` のムーブ・書き換え（`u_refsrc`）は、狭めた親 `p1.s` と重なるので `E1014` のまま。解放の順序（`cross (ref p1) q2` のように借用される値を後に宣言する形）は、今も部分で判断していて変わらない。`let q1 = ref (deref q0)` の連なりは、親の親も同じく狭める。

### `sharetype`

- `store_exported_into` で、`ExportTarget::Local(source)`（`keep_for_drop` の届け先）へ届けるかを、格納先の型ではなく、格納先の局所値が `source` のブロックを持ちうるかで判断する。持ちうるのは、読み替えた局所値そのもの、根が読み替えた局所値か `source` かその共有である loan（共有の loan を含む）を持つ値、値の出どころの分からない値（可変の局所値、セルの中身や呼び出しの結果を読んだ値、仮引数から得た値、本体の見えない関数値の結果）。loan を持たない値を `Rc.new`・リテラル・借用を持たない引数だけの呼び出しで作った不変の局所値（`let q = Rc.new (mkpo ())`、`W { ..., keep: Maybe.None }`、`mkhm ()`）は持ちえないので届けない。値を作った式から判断する `OwnerScan`（`may_own`）の形を手本にする。
- 局所値ごとの正確な道は、`Rc.share` の連なりで 3 乗になった（A31 の第4回）ので採らない。判断は局所値ごとに 1 度で済む形にし、`gen_chain.py cellshares` の n = 400・800・1600 の所有権の検査の時間を A31 の先端と比べる。
- 双子: 共有したブロックそのものへの格納（双子 1、`a` は根が `r` の loan を持つ）と、共有をムーブした変数への格納（双子 2、`q` は `r` の値を持つ）は届けたまま。A31 の第4回・第5回の安全な形と危険な形（中のブロックの共有を通した格納、2 つの元の所有者を持つ値）の結果を変えない。
- dyn の値と型引数（第6回の形）: 値を作った式から持ちえないものを決めるとき、`Dyn.of` の引数と型引数の値も同じく判断する。中身の型が分からないので、セル・呼び出しの結果・仮引数から得た dyn の値と、dyn の値のメソッドが返した参照の先は、持ちうるものとして残す（危険な双子は、`tests/rc.rs` の `a_store_through_a_share_behind_a_dyn_value_reaches_the_owner_that_frees_the_block` にあります）。

### `rcgetowner`

- `check_drop_order_of` で、局所値が持つ loan の根が `Rc`・`Arc` の局所値で、その場所がブロックの中の部分を指し（フィールドがある）、根が同じブロックの持ち主の組（`block_groups`）にあるとき、ブロックが解放されるのは組の最後の持ち主の解放なので、組のうちこのスコープで最も後に解放される（最も前に宣言した）持ち主の位置で判断する。その持ち主がスコープの終わりまで値を持ち続けることを条件にする: どの道でもムーブされない（`State::moved` に無い。`moved_always` はすべての道でムーブされたものだけなので足りない）、代入・排他借用で別のブロックの持ち主になりえない（`forget_block_share` が組から外す）。
- 双子: `p` を `r2` より先に宣言した形（双子 1、組のどの持ち主も `p` より先に解放される）と、`r2` を内側のブロックへムーブして先に手放す形（双子 2）は拒否のまま。`hold = r` の形は、`23:12` の `E1014`（A33 の `r07`・`r10`）が残るので、このチケットの範囲では最初の `E1013` だけが消える。

### `shareparam`

- `share_loans` で、根が参照の仮引数の loan にも共有の印を付け（`Drop.drop` の中の `drop_root` と同じ扱い: 共有はブロックの持ち主で、それを格納・破棄するのはブロックが持つもの（`Part::Contents`）の格納）、関数の結果が持つ外部の共有の loan を、呼び出し元が実引数の loan の共有（`Loan::share`、根は `r2`）として受け取るようにする（`parameter_retention` の結果の対応づけで、印を保つ）。共有の印の付いた結果は、`keep_for_drop`・`expand_shares`・`share_sources` の対象になる。
- 双子: 共有を通したブロックへの格納（双子 1）は、共有の loan をたどって `r2` へ届くので拒否のまま。関数の中の共有を通した格納（双子 2）は、事実 `(1, Value, 0)` が残るので拒否のまま。`Rc.share` を作って捨てるだけの関数（`sh2`）の事実が、`(0, Contents, 0)`（ブロックが持つものをブロックへ）になって呼び出し元で何も格納しないことを確かめる。

## テスト計画

### Rust テスト

- `tests/rc.rs` に、項目ごとに 1 つのテストを足す: 「現状」の安全な形の受理（`accepts_in_both_editions`。edition 2027 だけの形は `analyze_with_edition` で）と、危険な双子の拒否（`rejects_at` か両方の edition の code・kind・位置）。プログラムは「現状」から写す（前置きは `CROSS_PRELUDE` と同じ形）。項目を入れる前に双子のテストが成功し、安全な形のテストが失敗することを確かめる。
- `sharetype` を入れるときは、`a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block` の最後の `rejects_at`（`W` の形）を受理に変える（このチケットに記録してから）。`a_store_through_a_share_behind_a_dyn_value_reaches_the_owner_that_frees_the_block`（dyn の値を通した危険な形の拒否と、その安全な双子の受理）は変えない。

### E2E

- `tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs` の `in_place_writes` に、受理に変えた形を、値を手で数えた export として足す（A31 の fixture の `Recorder` の形。native と WASM の `-O0`・`-O3`）。`main` のバイナリでも同じ値になることを確かめる。

### コーパス

- A31 のマージの条件の確かめの道具（A31 のワークツリーの `target/scratch/a31/gate/`。A31 がマージされた後に消すなら、その前に `gate/p.tar.gz` と道具を保存する）と、A31 の第2回の反例の探索が生成したプログラム（`g-check.mjs`）を、項目を入れるたびに `main` と比べる。`main` が拒否して対応後が受理する危険なプログラムが無いこと、受理から拒否に変わる安全なプログラムが無いこと。受理に変わったものは、手で求めた値と native・ASan の実行で安全を確かめる。

### 性能

- `sharetype` と `rcgetowner` は、`gen_chain.py cellshares`（A33 の「検査の時間」）の n = 400・800・1600 の所有権の検査の時間（`TSUZURI_TIME_PASSES=1`）を、入れる前と後で同じ負荷の機械で続けて測り、このチケットに記録する。CI に速度の合否は入れない。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/drop.md`（「解放の順序（LIFO と宣言順）」の「別の仮引数の先へ書き込んだ部分」の一覧の `rcgetpart`・`refsrc` の項目、「`Rc`・`Arc` の別の持ち主を通した書き込み」の `sharetype`・`rcgetowner`・`shareparam` の文、「Drop 型に課される制約」の「解放が書き込む借用」の `fvdrop` の文）と `lifetimes.md`（「現在の制限」の「その場の書き込み」の段落と `Rc`・`Arc` の段落）の、直した制限の記述を実際の振る舞いに置き換え、このチケットへのリンクを外す。
- `docs/language.md`（A31 の破棄と格納の段落の安全側の列挙）と `docs/architecture.md`（A31 の書き込みの事実の節）の、直した近似の記述。
- `_features/A33-remaining-store-soundness-holes.md` の、A34 へ移した行の記述と段 3 の一覧（直した項目を外す）、`_features/README.md` の状態。

## 受け入れ条件

- [ ] 「現状」の各項目の安全な形を、書いた edition で受理し、値が `main` と同じ（直せない項目は理由を文書とこのチケットに書き、A33 に戻す）。
- [ ] 「現状」の危険な双子を、両方の edition で同じ code・kind で拒否する。
- [ ] A31 のテストと E2E の期待が変わらない（`sharetype` の `W` の形を除く。変えるものは記録した後）。
- [ ] コーパスで、`main` が拒否して対応後が受理する危険なプログラムが無く、受理から拒否に変わる安全なプログラムも無い。
- [ ] `gen_chain.py cellshares` の検査の時間が A31 の先端と同じ程度（n の 2 乗）。
- [ ] T2 が成功する。

## 落とし穴

- 6 つの近似は、どれも A31 のレビューが見つけた危険な形を塞ぐために入ったか、それを塞ぐ規則の安全側の端です。項目を狭めるたびに、その項目の双子だけでなく、A31 のレビューの危険な形（`gate/` のコーパス、名前が `u_`・`bad` のもの）を回す。A31 では、レビューの回ごとに、前の回の狭めが新しい穴を生みました（第3回のラムダの捕捉の狭めが第4回の指摘 1 になった）。
- `rcgetpart`: exact な loan は、部分の順序の検査（`ordered_self_borrow`）と写しの道（`kept_copy`・`path_copy`）の前提です。`Rc` の要素の場所を exact にすると、ブロックの中の部分の借用がその要素の場所の下の道として表されるので、要素の置き換え・ムーブ・別の持ち主を通した書き込みとの重なりを確かめる。
- `fvdrop`: リテラルの型自身の `Drop`、要素の中の破棄でコードを実行する値、2 つの仮引数の部分を持つリテラルを見落とすと、破棄が実行するコードの格納を落とす（`u_fvdrop`）。
- `refsrc`: 親を狭めてよいのは、親が同じ値の外側の借用のときだけです。借用した値が持つ借用（`p1` が持つ別の値の借用）の親を狭めると、その借用が先に終わってしまう。
- `sharetype`: 値の出どころは、セル（`Mutex`・`Rc` のブロック）を通して、loan を持たずに既存のブロックへ届くことがあります（セルの中身を読み出した値）。そうした値は型の判定のまま残す。dyn の値のメソッドは中身の場所の参照を返せ、型の走査はその中を見ません（A31 の第5回の狭めは、これを見落として dyn の値の中の共有を通した格納を落としました）。dyn の値と型引数に届く型を狭めの外に置く第6回の規則を保つ。
- `rcgetowner`: 組の持ち主は、ムーブ・代入・排他借用でブロックを手放しうる。`State::block_shares` が古くなる所（A33 の「落とし穴」の第12回の指摘 2）を確かめる。
- `shareparam`: 外部の loan に共有の印を付けると、呼び出し元に返る結果の対応づけ（`parameter_retention`）と、印の付いた loan を読む所（`exact_loan` は共有を exact にしない、`expand_shares`・`expand_shares_read`）のすべてに影響します。

## 対象外

- A33 のモデル（記憶域の寿命と破棄の順序の 1 つのモデル）と、A33 の表のほかの行（`mix4`・`w05`・`g08`・`f02`・`slice_self_safe_elem`・`fx3`・`h01`・`crossw`・`callres`・`mutref`・`cross5`・`mapr`・`fvparam`・`coarse`・`fvlam`・`matchref`・`blockalias`・`r06`・`r07`・`r10`・`r11`・`m_named_owned`）と、A33 の「未修正の穴」。
- 明示した `ref` の仮引数を通した関数値の破棄（`fvdrop` の方針案の外）。
- 言語の意味（破棄の順序、`Drop.drop` の規則）を変えること。
