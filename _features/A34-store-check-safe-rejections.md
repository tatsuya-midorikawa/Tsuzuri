# A34: A31 の格納と破棄の順序の検査が `main` の受理する安全なプログラムを拒否する（または検査を終えない）9 つの形を、危険な双子を拒否したまま受理する

| 項目 | 内容 |
| --- | --- |
| ID | A34 |
| 優先度 | P1（`main` が受理する安全なプログラムを A31 が拒否する表現力の退行。どれも回避の書き方がある。危険なプログラムの受理ではない） |
| 規模 | L（9 つの項目は互いに独立で、項目ごとに S〜M。項目ごとに分けて入れてよい） |
| 依存 | A31 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-15（A31 の Copilot のレビューの後のレビューの第4回・第5回から。A31 の先端（`src/` が `6e4f0537` と同じもの）と `main`（`168ae4a5`）で再現を確かめた。第6回の修正（`5e99c1c0`）の後、`sharetype` に形を足した。A31 の Copilot のレビュー 2 の対応（`df307296`）のレビュー（2026-11-15）から `replcopy`・`cellslots`・`loopplace` を足し、A31 の先端（`8fbcd40e`、`src/` は `df307296` と同じ）・その前の先端（`fbd7d194`）・`main` で再現を確かめた） |
| 承認 | 不要（今拒否している安全なプログラムを受理するだけで、危険なプログラムは拒否したまま。言語の意味は変えない） |
| 改善する劣位 | 表現力の退行: `main` が受理する安全なプログラムを、A31 の安全側の近似が `E1013`・`E1014` で拒否する（`loopplace` は検査が終わらない）。利用者は、項目ごとの回避の書き方（「現状」）を知らないと書けない |
| 手本にする既存実装 | `src/ownership.rs` の、`Checker::eval` の `E::Borrow` の腕（`typed`・`exact_loans`）と `exact_loan`・`points_at`、`StoreFacts::drop_only`・`Checker::parameter_part`・`drop_stores`・`value_runs_drop_code`・`apply_facts_once`（`dropping_part`）、`Checker::loan`・`kept_copy`・`path_copy`・`read_through_loan`（同じ場所の再借用を親として読む）・`Checker::captured_loans`、`keep_for_drop`・`share_sources`・`export_shared_block`・`store_exported_into`・`OwnerScan`（`may_own`: 値を作った式から、持ちえないものを決める）、`State::block_shares`・`note_block_share`・`forget_block_share`・`block_groups`・`same_block_loan`・`check_drop_order_of`・`moved_always`、`share_loans`（`Drop.drop` の中の `drop_root` の扱い）、`replaced_target`・`State::contents_elsewhere`・`State::merge` と `src/ownership_control.rs` の `check_loop`・`LoopSummary`（`replcopy`）、`absorb_owner`・`record_value`・`field_value`・`project`（`cellslots`）、`store_hidden_except`・`Checker::place`・`Value::through`（`loopplace`） |
| 主な影響ファイル | `src/ownership.rs`, `src/ownership_control.rs`, `tests/rc.rs`, `tests/mutable_slices.rs`, `tests/fixtures/in_place_writes/Main.tz`, `tests/features.mjs`, `_tsuzuri/language-reference/ownership-and-memory/drop.md`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `docs/language.md`, `docs/architecture.md`, `_features/A33-remaining-store-soundness-holes.md`, `_features/README.md` |

## 目的

A31（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)）は、その場の書き込み・ロックのコールバック・破棄が実行する `Drop.drop` が書き込んだ借用を、書き込み先の持ち主に残すようにしました。その検査は分からない所を安全側に近似するので、`main` が受理する安全なプログラムのいくつかを `E1013`・`E1014` で拒否します。このチケットは、そのうち A31 の Copilot のレビューの後のレビューの第4回・第5回で見つけた 6 つの形と、A31 の Copilot のレビュー 2 の対応のレビューで見つけた 3 つの形（`replcopy`・`cellslots`・`loopplace`。`loopplace` は拒否ではなく、検査が終わらない形です）、合わせて A31 では文書に書くだけにした次の 9 つの形を、それぞれの危険な双子（安全な形にいちばん近い危険な形）を拒否したまま受理します。

| 名前 | 形 | 回避の書き方（A31 の先端が受理する） |
| --- | --- | --- |
| `rcgetpart` | フィールドや要素の場所の `Rc` から `Rc.get` で得た参照を束縛した変数（`let b = Rc.get (ref rs[0])`）を、書き込む値の側へ渡す・ラムダで捕捉する・部分適用で束縛する | `Rc.get (ref rs[0])` を引数にそのまま書く |
| `fvdrop` | 受け取った関数値を持つ値（関数値を入れたリテラル、`match` のパターンの変数、型注釈を書いたラムダの仮引数、edition 2027 で読み取りの仮引数やそのフィールドを束縛した局所値）を破棄する関数に、破棄しても何も実行しない関数値（`cross (ref p2)`）を渡す | 仮引数をそのまま破棄する（`fn keep f = ()`）か、`own` の仮引数を束縛して破棄する |
| `refsrc` | 書き込む部分の側へ不変の参照の変数（`let q1 = ref p1` の `q1`）を渡した後で、借用されていない部分（`p1.h`）をムーブ・書き換える | 呼び出しの位置で `ref p1` と書く |
| `sharetype` | `let r = Rc.share (ref r2)` の `r` を使い終えた後で、共有したブロックの型を持ちうる別の値へ、`r2` より後に宣言した値の借用を書き込む | 借用される値を `r2` より先に宣言する |
| `rcgetowner` | `Rc.get (ref r)` の借用を、`r` より前に宣言した値へ書き込む（ブロックは、もっと前に宣言した別の持ち主 `r2` が生かす） | 借用を先に宣言した持ち主から得る（`Rc.get (ref r2)`） |
| `shareparam` | 参照の仮引数の `Rc` から関数の中で作った共有（`fn dup r = Rc.share r` の結果、本体の `let s = Rc.share r`） | 呼び出し元で `Rc.share (ref r2)` と書く |
| `replcopy` | ループの前に `ref mut` の仮引数の先へ自分の部分の借用を書き込んだ関数が、ループの中で `deref p = v` と置き換えてから新しい値のセルの中身をほかへ写す（`if` の片方の枝で置き換えて写した後の置き換えも） | ループが少なくとも 1 回回るなら、最初の反復（置き換えと写し）をループの前に書く |
| `cellslots` | 不変な複数 region のレコードの、region を持たないセルのフィールド（`Mutex`）へ借用を書き込んだ後で、別の region のフィールドを読んだ値を持ったまま、書き込んだ借用の元でない region の借用元をムーブする | フィールドを書き込みの前に読む |
| `loopplace` | 先にセルを持つ参照の仮引数の先の借用（セルの中身、`ref p.s` を捕捉した関数）を、書き込むセルを所有する関数値へ渡すループ（検査が終わらない） | 関数値には `Arc` ではなく、セルへの参照（`let sref = Arc.get (ref sink)`）を捕捉させる |

利用者にとっては、A31 の前に書けた安全なプログラムが、回避の書き方を知らなくても、そのまま書けるようになります。

## 背景

- A31 のレビューの第4回からの規則は「前の先端からの退行でない安全な形の拒否は `src/` を変えず、保守的な制限として文書に書く」でした（[A31 のチケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第4回」「レビューの第5回」）。6 つの形はこの規則で、`drop.md`・`lifetimes.md`・`docs/language.md` に安全側の扱いとして書き、[A33](A33-remaining-store-soundness-holes.md) の「保守的な制限」の表に行として足しました。`rcgetpart`・`fvdrop`・`refsrc`・`rcgetowner`・`shareparam` は A31 の前の先端からある拒否で、`sharetype` は第4回の修正（使い終えた共有の持ち主を通した格納を元の所有者へ型で届ける）が生んだ拒否です（第5回で、元の所有者が解放しえないブロックへの格納だけは届けないように狭めました）。どれも A31 のマージの条件 (3)（`main` が受理する安全なプログラムは、文書に書いた安全側の制限のほかは受理のまま）に従った扱いです。第6回では、第5回の狭めが dyn の値の中の共有を通した危険な形を受理していた（前の先端からの退行）ので、dyn の値と型引数に届く型では狭めないようにし、それによる安全側の拒否と、規則の実際の範囲（元の所有者のほかの部分が持ちうるブロック）を `sharetype` に足しました（「`sharetype` に A31 のレビューの第6回で足した形」）。
- A31 の Copilot のレビュー 2（ループの不動点の要約が `State` の事実の一部を持たず、ループの中の写しや格納がループの後で失われていた）の修正（`df307296`）の後のレビューで、3 つの形を足しました。`replcopy` と `cellslots` は、要約が `contents_elsewhere` と region の slot を比べるようになったために、ループの中の形が前の先端（`fbd7d194`）からの退行として拒否されるようになったものです（ループの無い形と `if` の形は前の先端も拒否していました）。`loopplace` は前の先端からある、検査が終わらない形で、A31 では edition 2026 の形として A33 の「検査の時間」に記録していましたが、どちらの edition でも起きることが分かったので、ここへ移しました。最後の回の規則（検査を緩める変更はしない）で、A31 では文書に書くだけにしました。
- 利用者の依頼で、A31 が直さずに残すこれらの安全な形の拒否を、文書と A33 の表の行だけでなく、このチケットで個別に直します。
- **A33 との関係**: A33 は、格納と破棄の順序の検査を、記憶域の寿命と破棄の順序の 1 つのモデルで作り直す XL のチケットです。A34 は、項目ごとに `src/ownership.rs`（`replcopy` は `src/ownership_control.rs` も）の 1〜2 個の近似を狭める小さな変更の集まりで、A33 のモデルを待たずに入れられます。9 つの項目は互いに独立で、どの順にも、項目ごとの PR にも分けられます。A33 が先に入るなら、A34 の各項目の安全なプログラムと危険な双子は A33 の受け入れのテストになり、A34 には A33 の後も拒否が残る項目だけが残ります。A34 の項目が先に入れば、その項目は A33 の段 3 の一覧から外れます。A33 の表では、この 9 つを A34 へ移したことを 1 行で示しています（`loopplace` は A33 の「検査の時間」から移しました）。
- 拒否の理由は、どれも検査が知らないことを安全側に扱うことです（「原因」）: (1) フィールドや要素の場所の `Rc` の借用を exact な loan にしない、(2) 破棄だけが作る格納（`drop_only`）を仮引数そのものとその部分の破棄に限る、(3) 参照の変数の再借用が元の借用（値全体の借用）を親に持つ、(4) 使い終えた共有の持ち主を通した格納を元の所有者へブロックの型で届ける、(5) `Rc.get (ref r)` の借用を `r` の借用として解放の順序を検査する、(6) 参照の仮引数の `Rc` から作った共有を共有として扱わない、(7) ループの反復の始まりの状態を入口の状態と前の反復の終わりの状態の和にし、先のセルの中身の写しを根だけで記録する、(8) セルへの格納で複数 region のレコードの region の slot を捨てる。`loopplace` の理由は (9) セルを所有する関数値への格納を参照の仮引数の値にも入れ、参照の先の場所をその借用の場所からも作るので、ループの要約が収束しないことです。

## 着手条件と停止条件

- A31 が `main` に入っていること。着手時の `main` で「現状」のプログラムを両方の edition で検査し直し、結果が違えば、このチケットを直してから始めます。
- 項目ごとに、その項目の危険な双子（「現状」）と、下の「テスト計画」のコーパスで、`main` が拒否して対応後が受理する危険なプログラムが出ないことを確かめてから入れます。双子を拒否したまま受理できない項目と、`Rc.share` の連なり（A33 の「検査の時間」の `gen_chain.py cellshares`）の検査を n の 2 乗より悪くする項目は入れず、理由をこのチケットに書いて A33 に戻します。

## 現状

### 共通の前置きと確かめ方

各項目のプログラムは、同じ 14 行の前置き（下の最初のプログラムの `record Holder` から `fn cross` まで）で始まります（それだけで検査・実行できるよう、各プログラムに含めています）。`Holder` は解放のときにセルの関数を呼んで `drop <値>` を出力します。`look t` は `t` の最初の `9` の位置で、`String.repeat 4 (ref "123456789")` の文字列なら 8、解放した文字列を読むと多くは -1 になります。`cross a b` は `b.s` の借用を読む関数を `a.h` の `Mutex` へ書き込みます（A31 の書き込みの事実は `(1, Value, 0)` で、`b` の先の部分 `s` だけを残します: `StoreFacts::kept_paths`）。`PairOk` は `h` を `s` より先に宣言するので、値の中では `h` の `drop` が `s` より先に走ります。`replcopy`・`cellslots`・`loopplace` のプログラムは、それぞれの項目に書いた別の前置きで始まります。

確かめたバイナリは、A31 の先端（ブランチ `fix/a31-in-place-writes-keep-borrows`、`src/` が `6e4f0537` と同じもの。開発用のビルド）と `main`（`168ae4a5`、A31 の前）です。`replcopy`・`cellslots`・`loopplace` は、A31 の先端（`src/` が `df307296` と同じもの。開発用のビルド）、その前の先端（`fbd7d194`）、`main` で確かめました（`loopplace` の検査は 120 秒で打ち切りました）。どちらも `check --edition 2027` と `check --edition 2026` で検査し、受理したものを `run -O0`・`run -O3` で実行しました（edition 2027 は AddressSanitizer のビルドの `-O0`・`-O3` も）。位置は最初の誤りの `行:列` で、解放の順序の誤り（`E1013` の `assignment_extends_borrow`・`self_referential_store`）は関数の本体の最初の文の位置を指します（検査はスコープの終わりに行い、本体の範囲を指すため）。値は手で求め、実行の出力と一致しました。

| 項目 | 安全な形: A31 の先端 | 安全な形: `main` | 危険な双子: A31 の先端 | 危険な双子: `main` |
| --- | --- | --- | --- | --- |
| `rcgetpart` | `17:5` `E1013`（`self_referential_store`） | 受理、`drop 8`・`drop 0`・`1` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1`、ASan は heap-use-after-free |
| `fvdrop` | `20:5` `E1013`（`self_referential_store`） | 受理、`drop 0`・`1` | `10:5` `E1013`（`assignment_extends_borrow`） | 受理、`0`（正しくは 9） |
| `refsrc` | `21:18` `E1014`（`borrow_conflict`） | 受理、`drop 0`・`drop 8`・`1` | `21:18` `E1014`（`borrow_conflict`） | 受理、`drop -1`・`drop 0`・`1` |
| `sharetype` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 8`・`drop 0`・`drop 0`・`1` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1` |
| `rcgetowner` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 8`・`drop 0`・`1` | `17:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1`、ASan は heap-use-after-free |
| `shareparam` | `19:5` `E1013`（`self_referential_store`） | 受理、`drop 8`・`drop 0`・`1` | `19:5` `E1013`（`assignment_extends_borrow`） | 受理、`drop 0`・`drop -1`・`1` |
| `replcopy` | `28:9` `E1014`（`borrow_conflict`）。前の先端は受理 | 受理、`drop 8 2`・`drop 0 2`・`drop 0 2`・`drop 0 2`・`0` | `29:9` `E1014`（`borrow_conflict`） | 受理、`drop 8 2`・`drop -1 2`・`drop 0 2`・`-1`、ASan は heap-use-after-free |
| `cellslots` | `20:17` `E1014`（`borrow_conflict`）。前の先端は受理 | 受理、`836` | `21:17` `E1014`（`borrow_conflict`） | 受理、`835`（解放した文字列を読む） |
| `loopplace` | 検査が終わらない（前の先端も） | 受理、`3` | 検査が終わらない（前の先端も） | 受理、`-1`（解放した文字列を読む） |

どれも edition 2027 と 2026 で同じ結果です（`fvdrop` の一部の形は edition 2027 だけ。下の各項目）。`main` の安全な形は、edition 2027 の ASan のビルドの `-O0`・`-O3` でも同じ値で、ASan は何も報告しませんでした。危険な双子で ASan が報告するのは、解放した `Rc` のブロックや配列の要素を読む形です（ほかの形は、`main` の出力が解放した文字列を読んだ値になりますが、ASan は報告しませんでした）。

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

dyn の値と型引数の値は、型が中身を示さず、型の走査（`TypeContext::reaches`）もその中を見ません。dyn の値のメソッドは中身の場所の参照を返せる（インスタンスの `fn leaf b = ref b.l`）ので、`Rc.share (Cls.LeafOf.leaf (ref (Rc.get (ref r2)).d))` は、`r2` の型からは見えない `r2` のブロックの共有です。第5回の狭め（`r` の型と `r2` の型の両方が届くブロックの型だけを届ける）はこの共有を通した格納を `r2` へ届けず、`main` も受理する解放後の読み出しを受理していた（前の先端からの退行）ので、第6回で、`r2` の型が dyn の値か型引数に届くなら狭めず、`r` の型が届くなら dyn の値か型引数かセルを持つブロックに届く型の値への格納をすべて `r2` へ届け（逆の向きも同じ）、dyn の値か型引数に届く型の値への格納はブロックの共有が届いたどの先へも届けるようにしました。次の 4 つの形はそれによる安全側の拒否です。どれも同じ `Cls.tt` と、上の前置きと `Other` の 5 行に `Leaf`・`mkleaf`・`crossleaf`・`Box1` とそのインスタンスの 9 行が続く前置きを使います。

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

逆の向き（元の所有者を通した書き込みを、その共有を持つ値へ届ける規則。`export_block`）で、元の所有者の型が dyn の値に届く形（`r2` の `B` はセルを持たないが、dyn の値の中のブロックはセルを持ちうるので、`r` を使い終えた後の、セルを持つブロックに届く型の値への書き込みを `r` へも届ける）:

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
record B {r} { n: i64, d: dyn Cls.LeafOf {r} }
def run :: i64 -> i64
fn run n =
    let r2 = Rc.new (B { n: 0, d: Dyn.of (Box1 { l: Rc.new (mkleaf 1) }) })
    let r = Rc.share (ref r2)
    let p = mkpo ()
    let o = Rc.new (mkother 3)
    crossother (Rc.get (ref o)) (ref p)
    let r3 = r2
    (Rc.get (ref r3)).n * 0 + n
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `32:5` の `E1013`（`assignment_extends_borrow`、`r` が借用を持つ）。第5回の先端と第4回の先端は受理します。
- `main`: 両方の edition で受理し、`drop 8`・`drop 0`・`drop 0`・`1`（ASan の `-O0`・`-O3` も同じ値で、報告なし）。手で求めた値: `r3` の解放は `B` のブロックの持ち主を 1 つ減らすだけで、`o` の `h` が `p.s` を読んで `drop 8`、`p` の `h` が `drop 0`、`r` が `B` のブロックを解放して dyn の値の中の `Leaf` の `h` が `drop 0`、`run` の結果が `1` です。
- 同じく拒否する形: `r2` の共有を呼び出し元の配列へ入れた関数の中の、無関係な `Rc<Other>` への書き込みは `E1013`（`store_local_borrow_through_reference`）です（`main` は受理して `drop 0`・`drop 8`・`drop 0`・`drop 0`・`1`。A31 のワークツリーの `target/scratch/a31/rr6/lim/s_ext_dynowner_other`）。
- 危険な双子（`o` の代わりに、`r2` の dyn のフィールドの中の `Leaf` の共有 `lf` を通して書き込むもの。`p` が解放された後で `r` が `B` のブロックを解放し、`Leaf` の `h` が `p.s` を読みます。A31 の第6回の前の先端と `main` が受理していた、`main` からある穴です）: A31 の先端は両方の edition で `32:5` の `E1013`（`assignment_extends_borrow`）、`main` は受理して `drop 0`・`drop -1`・`1` です（`look` を `t.length` にした形は ASan のビルドで heap-use-after-free。`target/scratch/a31/rr6/n/u_rev_nocell`）。共有を呼び出し元の配列へ入れた同じ形（`rr6/n/u_ext_nocell`）は `E1013`（`store_local_borrow_through_reference`）です。

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

### `replcopy`: ループの中で置き換えてから新しい値の中身を写す

この項目と次の 2 つは、A31 の Copilot のレビュー 2 の対応のレビューで足したものです。前置きの `Holder` は、解放のときにセルの関数の値と、`Drop.drop` の中で作る `junk` の長さを `drop <値> 2` と出力します（`junk` と `run` の `junk` は、解放した文字列の領域を別の文字列で使い直させ、解放した文字列を読むと `look` が -1 になりやすくするためのものです）。`renew` は、ループの前に `p.a[0]` の借用を読む関数を `p.h` のセルへ書き込み（自分の部分の借用）、ループの中で `deref p = mkpa ()` と置き換えてから、新しい値のセルの中身を `out` のセルへ写します。

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = {
        let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")];
        do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())) + " " + to_string junk.length)
    }
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def mkh :: unit -> Holder
fn mkh _u = Holder { cell: Mutex.create [\() -> 0] }
def mks :: unit -> string
fn mks _u = String.repeat 4 (ref "123456789")
record PA { h: Holder, a: [string] }
def mkpa :: unit -> PA
fn mkpa _u = PA { h: mkh (), a: [mks ()] }
def copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit
fn copy_out b out = Mutex.with_lock out (\o -> Array.write 0 b[0] o)
def renew :: ref mut PA -> ref Holder -> i64 -> i64
fn renew p out n =
    let t = ref p.a[0]
    Mutex.with_lock (ref p.h.cell) (\b -> Array.write 0 (\() -> look t) b)
    let mut i = 0
    while i < n do
        deref p = mkpa ()
        Mutex.with_lock (ref p.h.cell) (\b -> copy_out b (ref out.cell))
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let mut p0 = mkpa ()
    let out = mkh ()
    let k = renew (ref mut p0) (ref out) n
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy"), String.repeat 4 (ref "xxxxxxxxx")]
    Mutex.with_lock (ref out.cell) (\b -> b[0] ()) + k + junk.length * 0
do! IO.write_line (to_string (run 2))
```

- A31 の先端: 両方の edition で `28:9` の `E1014`（`borrow_conflict`。ループの中の `deref p = mkpa ()`）。前の先端（`fbd7d194`）は受理していました。前の先端の要約は `contents_elsewhere` を比べず、1 回目の反復で要約が変わらなかったので、写しの後の状態で本体を検査し直しませんでした。その修正（`df307296`）の後は、2 回目の反復の始まりの状態（入口の状態と 1 回目の反復の終わりの状態の和）が、入口の状態の `kept` の借用（`t`）と、1 回目の反復の写しの `contents_elsewhere` の根（`p`）を合わせて持ちます。`replaced_target` は根が `contents_elsewhere` にあると `kept` の借用を終わらせないので、置き換えが `t` の借用と競合します。実行では、1 回目の置き換えが `t` の借用を古い値とともに終わらせ、その後で写すのは借用を持たない新しい値の中身なので、この 2 つが同時に成り立つことはありません。
- `main`: 両方の edition で受理し、`drop 8 2`・`drop 0 2`・`drop 0 2`・`drop 0 2`・`0`（edition 2027 の ASan のビルドの `-O0`・`-O3` も同じ値で、報告なし）。手で求めた値: 1 回目の置き換えで `p0` の古い値が解放され、その `h` がまだ解放されていない `a[0]` を読んで `drop 8 2`、写すのは新しい値のセルの `\() -> 0` です。2 回目の置き換えで 1 つ目の新しい値が解放されて `drop 0 2`、`run` は `out` のセルの `0` を読み、`out` と `p0` の解放が `drop 0 2`・`drop 0 2`、結果が `0` です。
- 同じく拒否する形（両方の edition。`main` は受理して手で求めた値を出し、edition 2027 の ASan のビルドは何も報告しません）:
  - 範囲の `for`（`for i in 0 ..= (n - 1) do` に同じ本体）: `27:9`。前の先端は受理。
  - ループの無い `if`（24〜25 行目の後を `if n > 0 then { deref p = mkpa (); Mutex.with_lock (ref p.h.cell) (\b -> copy_out b (ref out.cell)) } else ()` と `deref p = mkpa ()` にしたもの）: `27:5`。前の先端も拒否（合流の和は同じです）。`main` は同じ値。
  - 写した先を後で上書きする形（ループの本体を `if i > 0 then deref p = mkpa () else ()` と写しにしたもの。1 回目の反復は古い値の中身 `\() -> look t` を `out` へ写し、2 回目の反復は置き換えてから新しい値の中身を写し直すので、古い写しは読まれる前に上書きされます。写しを入れ子のループに入れた形も同じ）: `28:23`（置き換え）。前の先端は受理。`main` は `drop 8 2`・`drop 0 2`・`drop 0 2`・`0`。これは置き換えの前に古い値の中身を写した形なので、下の方針案で直すのは `contents_elsewhere` の精度ではなく、セルへの書き込みを上書きとして扱えるかで、このチケットの範囲の外です（下の「対象外」）。
- 受理する形（A31 の先端と `main`）: ループの前の書き込み（24〜25 行目）の無い形、ループを開いた `deref p = mkpa ()`・写し・`deref p = mkpa ()` の並び、最初の反復をループの前に書いた形（25 行目の後に `deref p = mkpa ()` と写しを書き、`let mut i = 1` から回すもの。ループが少なくとも 1 回回るときの回避の書き方です。`if n > 0 then { ... } else ()` で囲むと、合流の和が戻るので `E1014` です）。

危険な双子 1（ループの本体の置き換えと写しを入れ替え、`run 1` にしたもの。写すのは古い値の中身 `\() -> look t` で、その後の置き換えが `t` の指す文字列を解放します）:

```tsuzuri
record Holder { cell: Mutex<[unit -> i64]> }
instance Drop<Holder> {
    fn drop h = {
        let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")];
        do! IO.write_line ("drop " + to_string (Mutex.with_lock (ref h.cell) (\b -> b[0] ())) + " " + to_string junk.length)
    }
}
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def mkh :: unit -> Holder
fn mkh _u = Holder { cell: Mutex.create [\() -> 0] }
def mks :: unit -> string
fn mks _u = String.repeat 4 (ref "123456789")
record PA { h: Holder, a: [string] }
def mkpa :: unit -> PA
fn mkpa _u = PA { h: mkh (), a: [mks ()] }
def copy_out :: ref [unit -> i64] -> ref Mutex<[unit -> i64]> -> unit
fn copy_out b out = Mutex.with_lock out (\o -> Array.write 0 b[0] o)
def renew :: ref mut PA -> ref Holder -> i64 -> i64
fn renew p out n =
    let t = ref p.a[0]
    Mutex.with_lock (ref p.h.cell) (\b -> Array.write 0 (\() -> look t) b)
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref p.h.cell) (\b -> copy_out b (ref out.cell))
        deref p = mkpa ()
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let mut p0 = mkpa ()
    let out = mkh ()
    let k = renew (ref mut p0) (ref out) n
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy"), String.repeat 4 (ref "xxxxxxxxx")]
    Mutex.with_lock (ref out.cell) (\b -> b[0] ()) + k + junk.length * 0
do! IO.write_line (to_string (run 1))
```

- A31 の先端と前の先端: 両方の edition で `29:9` の `E1014`（`borrow_conflict`。置き換え）。
- `main`: 両方の edition で受理し、`drop 8 2`・`drop -1 2`・`drop 0 2`・`-1`（`run` と `out` の解放が、解放した `a[0]` の文字列を読みます）。edition 2027 の ASan のビルドは、`-O0`・`-O3` とも `drop 8 2` の後に heap-use-after-free で止まります。

危険な双子 2（安全な形のループの本体を `if i > 0 then deref p = mkpa () else Mutex.with_lock (ref p.h.cell) (\b -> copy_out b (ref out.cell))` にしたもの。1 回目の反復の古い値の中身の写しを 2 回目の反復の置き換えが無効にし、上書きはありません）: A31 の先端は両方の edition で `28:23` の `E1014`（`borrow_conflict`）で、前の先端は受理していました（Copilot のレビュー 2 の修正で塞いだ穴です）。`main` は受理して `drop 8 2`・`drop -1 2`・`drop 0 2`・`-1` で、ASan は heap-use-after-free です。

### `cellslots`: セルのフィールドへ借用を書き込んだ複数 region のレコード

`Pair3` は 2 つの region `r`・`s` のフィールドと、region を持たないセルのフィールド `c` を持ちます。`eat` は文字列をムーブして長さを返します。

```tsuzuri
record Pair3 {r s} { a: ref {r} string, b: ref {s} string, c: Mutex<[unit -> i64]> }
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def eat :: own string -> i64
fn eat s = String.length s
def run :: i64 -> i64
fn run n =
    let s1 = String.repeat 4 (ref "123456789")
    let s2 = String.repeat 4 (ref "abcdefgh9")
    let x = Pair3 { a: ref s1, b: ref s2, c: Mutex.create [\() -> 0] }
    let ra = x.a
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref x.c) (\b -> Array.write 0 (\() -> look ra) b)
        i = i + 1
    let t = x.b
    let k = eat s1
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")]
    look t * 100 + k + junk.length * 0
do! IO.write_line (to_string (run 1))
```

- A31 の先端: 両方の edition で `20:17` の `E1014`（`borrow_conflict`。`eat s1` の `s1`。注記の借用は `t`）。前の先端は受理していました。`absorb_owner` は、局所の持ち主へ借用を持つ値を格納すると、持ち主の region の slot を捨てます（`owner.regions = None`）。そのため `let t = x.b` は `x` のすべての借用（`s1` と `s2`）を持ち、`t` が生きている間の `s1` のムーブが競合します。前の先端の要約は slot を比べず、ループの後は本体の前の状態（slot がある）に戻っていました。
- `main`: 両方の edition で受理し、`836`（edition 2027 の ASan のビルドの `-O0`・`-O3` も同じ値で、報告なし）。手で求めた値: `t` は `s2`（`"abcdefgh9"` の 4 回の繰り返し）だけを指すので `look t` は 8、`eat s1` は `s1` の長さ 36 で、`8 * 100 + 36 = 836` です。`x.c` の関数は `s1` のムーブの後に呼ばれず、`Pair3` の解放は何も実行しません。
- 同じく拒否する形（両方の edition。`main` は受理して `836`、ASan は報告なし）: 範囲の `for`（`18:17`。前の先端は受理）、ループの無い書き込み（`17:17`。前の先端も拒否）、`if n > 0 then ... else ()` の中の書き込み（`17:17`。前の先端も拒否）。
- 受理する形: 書き込みの無い形、書き込みの前に `let t = x.b` を読む形（回避の書き方）。

危険な双子（ループで `x.c` へ書き込む関数が、`x` のフィールドの借用元でない `s3` を読み、`s3` のムーブの後で `x.c` の関数を呼ぶもの。直し方が格納した借用を slot に足さないとき、それでも `x` 全体の借用に残っていることを確かめる双子です）:

```tsuzuri
record Pair3 {r s} { a: ref {r} string, b: ref {s} string, c: Mutex<[unit -> i64]> }
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def eat :: own string -> i64
fn eat s = String.length s
def run :: i64 -> i64
fn run n =
    let s1 = String.repeat 4 (ref "123456789")
    let s2 = String.repeat 4 (ref "abcdefgh9")
    let s3 = String.repeat 4 (ref "123456789")
    let x = Pair3 { a: ref s1, b: ref s2, c: Mutex.create [\() -> 0] }
    let r3 = ref s3
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref x.c) (\b -> Array.write 0 (\() -> look r3) b)
        i = i + 1
    let t = x.b
    let k = eat s3
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")]
    look t * 100 + k + Mutex.with_lock (ref x.c) (\b -> b[0] ()) + junk.length * 0
do! IO.write_line (to_string (run 1))
```

- A31 の先端と前の先端: 両方の edition で `21:17` の `E1014`（`borrow_conflict`。`eat s3`）。
- `main`: 両方の edition で受理し、`835`（`x.c` の関数がムーブして解放した `s3` を読んで -1。解放していなければ 8 で `844`）。edition 2027 の ASan のビルドは同じ値で、何も報告しません。

### `loopplace`: 参照の仮引数の先の借用を、セルを所有する関数値へ渡すループ

```tsuzuri
record Box { cell: Mutex<[unit -> i64]> }
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref p.cell) (\b -> w b[0])
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let keep = Arc.new (Mutex.create [\() -> 0])
    let mut b = Box { cell: Mutex.create [\() -> 3] }
    let k = run2 (ref mut b) (Arc.share (ref keep)) n
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k
do! IO.write_line (to_string (run 1))
```

- A31 の先端と前の先端: 両方の edition で検査が終わりません（120 秒で打ち切り。レビューでは edition 2027 の先端が 600 秒（CPU 597 秒）たっても終わりませんでした）。
- `main`: 両方の edition で 1 秒以内に受理し、`3`（native の `-O0`・`-O3`。edition 2027 の ASan のビルドの `-O0`・`-O3` も同じ値で、報告なし）。手で求めた値: ループは 1 回で、`b` のセルの `\() -> 3` を `keep` と共有するセルへ書き込み、`run` がそれを呼んで `3`、`k` は `0` です。
- 同じく終わらない形（`main` は受理）: 関数値が局所値の `Arc.share` を捕捉する形（`let sink = Arc.new (...)`・`let sink2 = Arc.share (ref sink)` を捕捉。両方の edition）、`ref mut` でなく `ref Box` の仮引数で、局所値の `Arc.share` を捕捉する形（両方の edition）、edition 2027 の読み取りの仮引数 `Box`（edition 2026 では値の仮引数で、局所値なので終わります）、セルの中身でなく `ref p.s` を捕捉した関数を渡す形（`record Box { cell: Mutex<[unit -> i64]>, s: string }`。両方の edition）、範囲の `for`（両方の edition）、edition 2026 の値の仮引数の `Arc` を捕捉する形（A31 の Copilot のレビュー 2 の点検で見つけた、`ref mut PA` の `p.h.cell` の中身を写す形を含みます）、edition 2026 の部分適用 `let w = put (Arc.share (ref sink))`（`fn put s g = Mutex.with_lock (Arc.get (ref s)) (\o -> Array.write 0 g o)`）。
- 終わる形（A31 の先端と `main` が受理）: ループを開いた 3 回の呼び出しの並び、局所値の `b` の先のセル（`ref b.cell`、`let p = ref b` の `ref p.cell`）、先にセルを持たない仮引数（`ref [unit -> i64]` の `p[0]`、`for f in p`、`record Box { items: [unit -> i64] }`、`ref Mutex<[unit -> i64]>` そのもの、`record Box { n: i64, s: string }` の `ref p.s`）、edition 2027 の読み取りの仮引数の `Arc` を捕捉する形（関数値はセルを所有しません）、関数値が `Arc` でなくセルへの参照を捕捉する形（`let sref = Arc.get (ref sink)` の後の `let w = \g -> Mutex.with_lock sref (\o -> Array.write 0 g o)`。回避の書き方）。

危険な双子（`b` のセルの関数が、内側のブロックの文字列 `s` の借用を読み、`run2` がそれを `keep` と共有するセルへ書き込んだ後で、ブロックが `s` を解放するもの）:

```tsuzuri
record Box { cell: Mutex<[unit -> i64]> }
def look :: ref string -> i64
fn look t =
    match String.find (ref "9") t with
    | Maybe.Some i -> i
    | Maybe.None -> -1
def run2 :: ref mut Box -> own Arc<Mutex<[unit -> i64]>> -> i64 -> i64
fn run2 p sink n =
    let w = \g -> Mutex.with_lock (Arc.get (ref sink)) (\o -> Array.write 0 g o)
    let mut i = 0
    while i < n do
        Mutex.with_lock (ref p.cell) (\b -> w b[0])
        i = i + 1
    0
def run :: i64 -> i64
fn run n =
    let keep = Arc.new (Mutex.create [\() -> 0])
    let k = {
        let s = String.repeat 4 (ref "123456789");
        let t = ref s;
        let mut b = Box { cell: Mutex.create [\() -> look t] };
        run2 (ref mut b) (Arc.share (ref keep)) n
    }
    let junk = [String.repeat 4 (ref "zzzzzzzzz"), String.repeat 4 (ref "yyyyyyyyy")]
    Mutex.with_lock (Arc.get (ref keep)) (\c -> c[0] ()) + k + junk.length * 0
do! IO.write_line (to_string (run 1))
```

- A31 の先端と前の先端: 両方の edition で検査が終わりません（120 秒で打ち切り）。
- `main`: 両方の edition で 1 秒以内に受理し、`-1`（`keep` のセルの関数が、内側のブロックが解放した `s` を読みます。解放していなければ `look t` は 8）。edition 2027 の ASan のビルドは同じ値で、何も報告しません。

## 原因

関数名は `src/ownership.rs` のものです。

- **`rcgetpart`**: `Checker::eval` の `E::Borrow` の腕は、借用した場所が借用した部分そのものであること（`exact_place`）に加えて、場所の型が借用の型と同じこと（`typed`: `place_type(&place)` が `without_views(&value.ty)` と同じ）を求め、両方が成り立つ loan だけを `exact_loans` に入れます。`place_type` は場所をたどるたびに `Rc`・`Arc` を読み通し、最後の段の後にも読み通す（`unwrap(current)`）ので、`ref rs[0]`（借用の型は `Rc<PairOk>`）の場所の型は `PairOk` になって合わず、loan は exact になりません。`exact_loan` はフィールドの無い場所（変数全体の `ref r`）を `exact_loans` なしで exact とみなすので、変数全体の形は受理されます。その loan を持つ局所値 `b` を書き込む値の側へ渡すと、`kept_copy` は exact な loan だけを `kept_paths` の部分（`.s`）の借用に置き換えるので、`p2` は `rs[0]` の場所全体の借用を持ち、その後の格納は `apply_store_within` がその借用をたどって `rs` の中のセル（`store_target`）へ届けます。`rs` が自分の借用を持つものになり、`check_drop_order_of` が `self_referential_store` を報告します。ラムダの捕捉と部分適用の引数は、`bound_value` が exact な loan だけを引数の値とし、ほかは `None`（関数値の値全体）にします。引数にそのまま書いた `Rc.get (ref rs[0])` が受理される道（`points_at` は `Rc.get` の引数の借用を `exact_place` で判断します）は、着手時に確かめます。
- **`fvdrop`**: `Checker::drop_stores` は、破棄でコードを実行しうる値の破棄を、その値の loan をその値の書き込み先へ格納するもの（`apply_store(value, value)`）として扱い、破棄した値が仮引数の一部（`parameter_part`: 不変で参照でない仮引数、そのフィールド・要素）なら、その格納を `StoreFacts::drop_only` に記録します。呼び出し元の `apply_facts_once` は、drop-only の格納を、実引数の式が破棄でコードを実行しうるとき（`value_runs_drop_code`）だけ当てはめます。`parameter_part` は `E::Local`・`E::Field`・`E::Index` しか見ないので、リテラル（`FR { f: f, k: 1 }`、`[f]`、`(f, 1)`、`Maybe.Some f`）、パターンの変数（型注釈を書いたラムダの仮引数は `\$lambdaN -> match $lambdaN with h -> ...` に下ろされます）、edition 2027 の読み取りの仮引数（型が参照）を束縛した局所値の破棄は `None` になり、普通の格納として記録されます。呼び出し元は、`cross (ref p2)` が破棄で何も実行しないことを使えず、`(0, Value, 0)` を当てはめて `p2` の借用を `p2` 自身のセルへ入れます。
- **`refsrc`**: 呼び出しは、場所の借用でない参照の実引数 `q1` を `ref (deref q1)` として再借用します。`Checker::loan` は借用する場所の根の局所値が持つ loan を新しい loan の親に足す（`parents.extend(&value.loans)`）ので、再借用の loan は `q1` が持つ `p1` 全体の借用を親に持ちます。`kept_copy` は `kept_paths` の部分（`p1.s`）の写しを作りますが、写しは元の親を持つ（呼び出し元が前に格納した元の loan と同じく、借用した値が持つものを生かすため）ので、`p2` が生きている間は `p1` 全体の借用が生きたままになり、`p1.h` のムーブが `E1014` になります。親が「借用した値が持つもの」ではなく「同じ値の外側の借用」であることを区別していません（`read_through_loan` は、同じ場所の再借用を親として読む場合を区別しています）。ラムダの捕捉は `Checker::captured_loans` で狭めますが、再借用を束縛した局所値（`let q1 = ref (deref q0)`）の loan も同じく元の借用を親に持ちます。
- **`sharetype`**: `keep_for_drop` は、破棄でコードを実行しうる局所値のうち本文がもう使わないもの（`let a = Rc.get (ref r)` の後の `r`）が持つ共有の loan（`Loan::share`）を、元の所有者 `r2` が今持つ loan に読み替えます（`expand_shares`）。読み替えで `r` から `r2` への道が消えるので、`share_sources` の元の所有者へ、`export_shared_block(r, r2)` がブロックの型（`r` の型と `r2` の型の両方が届く、セルを持つブロックの型。`export_walk`）を届け先として記録し、以後の格納は `store_exported` → `store_exported_into` が、格納先の局所値の型がその型に届けば `r2` へも入れます。型の走査は dyn の値と型引数の中を見ないので（`walk_hides_blocks`）、`r2` の型がそうした値に届くなら狭めずに `r` の型が届くすべての型を、`r` の型が届くなら印 `HIDDEN_BLOCK`（dyn の値か型引数かセルを持つブロックに届く型の値への格納に当たる）を記録し（逆の向きの `export_block` も、元の所有者の型が届くなら `HIDDEN_BLOCK` を記録します）、`store_exported_into` は、格納先の型が dyn の値か型引数に届けば、すべての届け先へ入れます（第6回）。判定は型だけなので、`r2` のブロックを持ちえない値（新しく作った `Rc.new (mkpo ())`、`keep` が `Maybe.None` の `W`）への格納も `r2` に入ります。事実は書き込み先の仮引数だけを記録し、その中のどの部分へ書き込んだかを記録しないので、呼び出し元はブロックでない部分（`w.h`）への格納を型で除くこともできません。局所値ごとの正確な道（読み替えた共有の loan を記録し、`apply_store_within` がたどる）は、`Rc.share` の連なりで検査が 3 乗になったので、A31 では採っていません（A31 のチケットの「レビューの第4回」）。
- **`rcgetowner`**: `Rc.get (ref r)` の結果は `ref r` の loan（場所の根は `r`）を持ちます。`check_drop_order_of` は、局所値が持つ loan の根の位置で解放の順序を判断し、根 `r` が `p` より後に宣言されていれば `borrow_dropped_first` で `assignment_extends_borrow` にします。同じブロックの持ち主の組（`State::block_shares`、`block_groups`）を見る `same_block_loan` は、借用を持つ値自身が同じブロックの持ち主のときだけ例外にするので、ブロックを持たない `p` には、組のほかの持ち主（`r2`）がブロックを生かすことが使われません。
- **`shareparam`**: `share_loans` は、`Rc.share`・`Arc.share` の結果が持つ loan を共有の印（`Loan::share`）を付けた写しにしますが、根が外部（仮引数）の loan は、`Drop.drop` の中の破棄する値（`drop_root`）を除いて印を付けません。そのため `fn dup r = Rc.share r` の結果と `let s = Rc.share r` は仮引数 `r` の先の借用のままで、その破棄（`drop_stores`。どれかの `Drop.drop` が書き込みうるプログラム、`facts.drops_write` だけ）は、その借用を共有したブロックのセルへ格納するものになります。呼び出し元では `dup (ref r2)` の結果が `r2` の借用で、その破棄が `r2` の借用を `r2` のブロックのセルへ入れ、`sh2 (ref r2)` は事実 `(0, Value, 0)` で同じことをし、`r2` が自分自身の借用を持つものとして `self_referential_store` になります。呼び出し元で書いた `Rc.share (ref r2)` は、根が局所値なので共有として扱われます（`note_block_share` も記録します）。
- **`replcopy`**: `check_loop`（`src/ownership_control.rs`）は、反復の始まりの状態を入口の状態と `continue` の状態の合流（`State::merge`: `kept`・`kept_elsewhere`・`contents_elsewhere` は和）にし、その要約（`LoopSummary`）が変わらなくなるまで本体を検査し直します。`State::contents_elsewhere` は、先のセルの中身をほかの先へ写した根（仮引数）の集合で、写したときにどの借用が中身にあったかを持ちません。`replaced_target` は、根が `contents_elsewhere` にあれば、`kept` の借用（先のセルへ書き込んだ自分の部分の借用）を終わらせずに戻ります（写しが借用を持ちうるため）。合流した状態は、入口の状態の `kept` の借用と、前の反復が置き換えの後に写した根を同時に持つので、置き換えは借用を終わらせず、生きたままの `kept` の借用（`p.a[0]` の借用）が先全体への書き込みと競合します。`if` の合流も同じです。
- **`cellslots`**: 複数 region のレコードの値は、region ごとの借用の集合（`Value::regions`）を持ち、フィールドの読み出し（`field_value`・`project`）はそのフィールドの region の slot の借用だけを持ちます。`absorb_owner` は局所の持ち主へ借用を持つ値を格納すると、格納先のフィールドを問わず `owner.regions = None` にするので、その後の読み出しは値全体の借用を持ちます。レコードリテラル（`record_value`）は、region を持たないフィールドの借用をどの slot にも入れず値全体の借用にだけ入れるので、作った時点の同じ関数は slot を保ちます。ループの要約は region の slot を比べる（A31 の Copilot のレビュー 2）ので、ループの中の格納も slot を失わせます。
- **`loopplace`**: セルを所有する関数値の呼び出し（`owned_sinks`）は、`store_hidden_except` で、セルに届く型か環境を隠す型のすべての局所値へ、渡した値の借用を格納します。先にセルを持つ参照の仮引数 `p` もその局所値なので、`p` の値は先の場所（`(deref p).cell` や `(deref p).s`）の借用を持つようになります。`Checker::place` は参照の先の場所を参照の値のすべての借用の場所から作るので、次の反復の `ref p.cell` は `(deref p).cell.cell` の場所も作り、その借用がまた `p` に入ります。反復ごとに場所が 1 段ずつ深くなり、借用が増えるので、要約が変わり続け、1 回の反復の検査（`active` などが局所値の借用と親をたどる）も重くなって、反復の上限（`MAX_NESTING` + 1 回、`E1017`）に届く前に検査が実用的な時間で終わりません。edition 2027 の読み取りの仮引数の `Arc` を捕捉したラムダや、セルへの参照を捕捉したラムダはセルを所有しないので（`owned_sinks` にならない）、終わります。

## 期待する振る舞い

- 「現状」の各項目の安全な形（主なプログラムと「同じく拒否する形」）を、書いた edition で受理し、`main` と同じ値（手で求めた値）を出す。
- 「現状」の危険な双子（主なプログラムと、文で書いた双子）を、両方の edition で、同じ code と kind で拒否する（`E1013` の `assignment_extends_borrow`・`self_referential_store`、`E1014` の `borrow_conflict`。edition 2026 の `rcgetpart` の双子 3 は `E1012` のまま）。位置が変わるなら、このチケットに記録する。`loopplace` の危険な双子は、今は検査が終わらないので、拒否の code・kind・位置を、直したときにこのチケットに記録する（呼び出し元の `run2 (ref mut b) (Arc.share (ref keep)) n` が `keep` に `s` の借用を残すことによる、`keep` より後に宣言した `s` の解放の順序の `E1013` を期待します）。
- `loopplace` の安全な形と同じく終わらない形の検査が、`main` と同じ程度の時間（1 秒ほど）で終わる。
- A31 のテスト（`tests/rc.rs`・`tests/mutable_slices.rs`・`tests/currying.rs`・`tests/borrowed_records.rs`・`tests/user_drop.rs` など）と E2E（`in_place_writes`・`mutable_slices`・`rc`・`borrowed_records`・`conventions`・`control`）の期待は変えない。例外は、この 9 つの制限の拒否を固定したテストで、受理に変える前にこのチケットに記録する: `tests/rc.rs` の `a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block` の最後の `rejects_at`（`sharetype` の `W` の形、`50:5`）。
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

### `replcopy`

- `State::contents_elsewhere` を根の集合から、根ごとに「写したときに `kept`（と `kept_elsewhere`）にあった、その根の先の借用の集合」にする（A31 の Copilot のレビュー 2 の対応のレビューの勧め）。`replaced_target` は、根が写しを持つとき、写しに入った借用だけを `kept` に残し、ほかの借用（入口の状態から来た、写しの後に書き込まれていない借用）は今と同じく古い値とともに終わらせる。`State::merge` は根ごとの和、ループの要約（`LoopSummary`）は根ごとの借用を `LoanKey` で比べる。
- 双子: 置き換えの前に古い値の中身を写す形（双子 1、写しが `t` の借用を記録する）と、片方の枝の写しを別の反復の置き換えが無効にする形（双子 2）は、写しが `t` の借用を持つので拒否のまま。写しの後に同じ値へ書き込んだ借用（写しには入っていないが、写しの前の借用と同じ場所のもの）の扱いと、`kept_elsewhere` の借用（別の先へ書き込んだので、置き換えでは終わらない）の扱いを確かめる。
- 写した先を後で上書きする形（「同じく拒否する形」の 3 つ目）は、写しに `t` の借用が入るので、この案では拒否のまま残る（セルへの書き込みを上書きとして扱わないため）。直さない形として「対象外」に置く。

### `cellslots`

- `absorb_owner` で、格納先がレコードの region を持たないフィールド（`field_regions` の mask が 0）の中のセルだと分かるとき（格納の書き込み先の loan の場所がそのフィールドの下にある）、`record_value` と同じく、格納した借用を値全体の借用にだけ足して region の slot を保つ。分からないとき（場所が値全体、region を持つフィールドの下、`let mut` の値）は今と同じく slot を捨てる。
- 先に、region を持たないフィールドの読み出し（`project` の mask 0 の扱い）が何を持つかを確かめる。そのフィールドの中のセルへ書き込んだ借用は、セルを借りた（`ref x.c`）値全体の借用と、値全体の借用で検査されている必要があります。
- 双子: 書き込んだ借用の元のムーブの後で `x.c` の関数を呼ぶ形（双子）は、`x` 全体の借用で拒否のまま。`x` をムーブする・`ref x` を渡す・`x` を値として破棄する（`Drop` を持つフィールドがある）形も、値全体の借用で検査されることを確かめる。

### `loopplace`

- 次のどれかで、ループの要約を有限にする。(a) `store_hidden_except` は、参照の仮引数（外部の根）への格納を、その値の借用に足さず、呼び出し元への事実（`StoreFacts::hidden`）だけにする（呼び出し元のデータは関数の中の局所値ではないので、関数の中で守る必要があるのは、その値を通した後の読み出しが格納した借用を持つことだけです）。(b) `Checker::place` が参照の先の場所を作るとき、参照の値が格納として受け取った借用（`Value::through` か、隠れた環境への格納で足した借用）を、参照の指す場所から除く。(c) 場所の長さに上限を設け、超えた場所はその接頭辞（大きな部分）の借用にする（`Source::coarse` と同じ安全側の扱い）。
- (a)・(b) は、参照の仮引数の値が持つ借用を読む所（呼び出し元の事実の `Part::Contents`、`deref p = v` の `replaced_target`、`self_paths`）のすべてで、格納した借用を失わないことを確かめる。
- 双子: 呼び出し元の `keep` に `s` の借用が残る（事実 `hidden`）ので、`keep` より後に宣言した `s` を内側のブロックで解放する形は拒否になることを確かめる（今は検査が終わらないので、拒否の code・kind・位置は分かっていません）。

## テスト計画

### Rust テスト

- `tests/rc.rs` に、項目ごとに 1 つのテストを足す: 「現状」の安全な形の受理（`accepts_in_both_editions`。edition 2027 だけの形は `analyze_with_edition` で）と、危険な双子の拒否（`rejects_at` か両方の edition の code・kind・位置）。プログラムは「現状」から写す（前置きは `CROSS_PRELUDE` と同じ形。`replcopy`・`cellslots`・`loopplace` は項目の前置き）。項目を入れる前に双子のテストが成功し、安全な形のテストが失敗することを確かめる（`loopplace` は、入れる前のテストが終わらないので、入れた後のビルドで双子の拒否と安全な形の受理を確かめ、入れる前のビルドでは時間を区切って終わらないことを確かめる）。`replcopy` は `tests/rc.rs` の `a_loop_keeps_what_its_body_stored_elsewhere`（ループの中の写しの危険な形の拒否）を、`cellslots` は region の検査のテスト（`tests/borrowed_records.rs` など）を変えない。
- `sharetype` を入れるときは、`a_store_through_an_expanded_share_reaches_the_owners_that_may_free_the_block` の最後の `rejects_at`（`W` の形）を受理に変える（このチケットに記録してから）。`a_store_through_a_share_behind_a_dyn_value_reaches_the_owner_that_frees_the_block`（dyn の値を通した危険な形の拒否と、その安全な双子の受理）は変えない。

### E2E

- `tests/fixtures/in_place_writes/Main.tz` と `tests/features.mjs` の `in_place_writes` に、受理に変えた形を、値を手で数えた export として足す（A31 の fixture の `Recorder` の形。native と WASM の `-O0`・`-O3`）。`main` のバイナリでも同じ値になることを確かめる。

### コーパス

- A31 のマージの条件の確かめの道具（A31 のワークツリーの `target/scratch/a31/gate/`。A31 がマージされた後に消すなら、その前に `gate/p.tar.gz` と道具を保存する）と、A31 の第2回の反例の探索が生成したプログラム（`g-check.mjs`）を、項目を入れるたびに `main` と比べる。`main` が拒否して対応後が受理する危険なプログラムが無いこと、受理から拒否に変わる安全なプログラムが無いこと。受理に変わったものは、手で求めた値と native・ASan の実行で安全を確かめる。

### 性能

- `sharetype` と `rcgetowner` は、`gen_chain.py cellshares`（A33 の「検査の時間」）の n = 400・800・1600 の所有権の検査の時間（`TSUZURI_TIME_PASSES=1`）を、入れる前と後で同じ負荷の機械で続けて測り、このチケットに記録する。CI に速度の合否は入れない。
- `replcopy` は根ごとの借用の集合を持つので、ループの多い関数（A31 のチケットの「Copilot のレビューへの対応（第2回）」の `copilot2/perf.sh` の `perf_loop_breaks_400`・`x04_nested_loops_30` など）の所有権の検査の時間を、入れる前と後で比べる。
- `loopplace` は、安全な形と「同じく終わらない形」の検査の時間を記録する（`main` は 1 秒以内）。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/drop.md`（「解放の順序（LIFO と宣言順）」の「別の仮引数の先へ書き込んだ部分」の一覧の `rcgetpart`・`refsrc` の項目、「`Rc`・`Arc` の別の持ち主を通した書き込み」の `sharetype`・`rcgetowner`・`shareparam` の文、「Drop 型に課される制約」の「解放が書き込む借用」の `fvdrop` の文）と `lifetimes.md`（「現在の制限」の「その場の書き込み」の段落と `Rc`・`Arc` の段落）の、直した制限の記述を実際の振る舞いに置き換え、このチケットへのリンクを外す。`replcopy` は `drop.md` の「値全体の代入」と `lifetimes.md` の「現在の制限」のループの段落、`cellslots` は `lifetimes.md` の「現在の制限」の表（関係する region をすべて保持する行）と複数 region のレコードの段落、`loopplace` は `lifetimes.md` の「現在の制限」の検査が終わらないループの段落です。
- `docs/language.md`（A31 の破棄と格納の段落の安全側の列挙、`replaced_target` の括弧の中のループの文）と `docs/architecture.md`（A31 の書き込みの事実の節、ループの不動点の段落の `replcopy`・`loopplace` の文）の、直した近似の記述。
- `_features/A33-remaining-store-soundness-holes.md` の、A34 へ移した行の記述と段 3 の一覧（直した項目を外す）、`_features/README.md` の状態。

## 受け入れ条件

- [ ] 「現状」の各項目の安全な形を、書いた edition で受理し、値が `main` と同じ（直せない項目は理由を文書とこのチケットに書き、A33 に戻す）。
- [ ] 「現状」の危険な双子を、両方の edition で同じ code・kind で拒否する（`loopplace` の双子は、直したときの code・kind・位置を記録する）。
- [ ] `loopplace` の安全な形と「同じく終わらない形」の検査が `main` と同じ程度の時間で終わる。
- [ ] A31 のテストと E2E の期待が変わらない（`sharetype` の `W` の形を除く。変えるものは記録した後）。
- [ ] コーパスで、`main` が拒否して対応後が受理する危険なプログラムが無く、受理から拒否に変わる安全なプログラムも無い。
- [ ] `gen_chain.py cellshares` の検査の時間が A31 の先端と同じ程度（n の 2 乗）。
- [ ] T2 が成功する。

## 落とし穴

- 9 つの近似は、どれも A31 のレビューが見つけた危険な形を塞ぐために入ったか、それを塞ぐ規則の安全側の端です（`loopplace` は A28 の隠れた環境の規則と参照の先の場所の求め方の組み合わせです）。項目を狭めるたびに、その項目の双子だけでなく、A31 のレビューの危険な形（`gate/` のコーパス、名前が `u_`・`bad` のもの）を回す。A31 では、レビューの回ごとに、前の回の狭めが新しい穴を生みました（第3回のラムダの捕捉の狭めが第4回の指摘 1 になった）。
- `rcgetpart`: exact な loan は、部分の順序の検査（`ordered_self_borrow`）と写しの道（`kept_copy`・`path_copy`）の前提です。`Rc` の要素の場所を exact にすると、ブロックの中の部分の借用がその要素の場所の下の道として表されるので、要素の置き換え・ムーブ・別の持ち主を通した書き込みとの重なりを確かめる。
- `fvdrop`: リテラルの型自身の `Drop`、要素の中の破棄でコードを実行する値、2 つの仮引数の部分を持つリテラルを見落とすと、破棄が実行するコードの格納を落とす（`u_fvdrop`）。
- `refsrc`: 親を狭めてよいのは、親が同じ値の外側の借用のときだけです。借用した値が持つ借用（`p1` が持つ別の値の借用）の親を狭めると、その借用が先に終わってしまう。
- `sharetype`: 値の出どころは、セル（`Mutex`・`Rc` のブロック）を通して、loan を持たずに既存のブロックへ届くことがあります（セルの中身を読み出した値）。そうした値は型の判定のまま残す。dyn の値のメソッドは中身の場所の参照を返せ、型の走査はその中を見ません（A31 の第5回の狭めは、これを見落として dyn の値の中の共有を通した格納を落としました）。dyn の値と型引数に届く型を狭めの外に置く第6回の規則を保つ。
- `rcgetowner`: 組の持ち主は、ムーブ・代入・排他借用でブロックを手放しうる。`State::block_shares` が古くなる所（A33 の「落とし穴」の第12回の指摘 2）を確かめる。
- `shareparam`: 外部の loan に共有の印を付けると、呼び出し元に返る結果の対応づけ（`parameter_retention`）と、印の付いた loan を読む所（`exact_loan` は共有を exact にしない、`expand_shares`・`expand_shares_read`）のすべてに影響します。
- `replcopy`: A31 の Copilot のレビュー 2 は、ループの要約が `contents_elsewhere` を持たなかったために、ループの中の写しが失われた穴でした。写しの記録を細かくするときは、要約がその記録を比べること（ループの中で写しに借用が足されたら、要約が変わること）を確かめる。`contents_elsewhere` は、写しの後の置き換えのほか、`deref p = v` の後の `kept_elsewhere` の扱いにも関わります。
- `cellslots`: region の slot は、フィールドの読み出しが持つ借用を決めます。セルの中身はセルを借りた値を通してしか読めませんが、セルを持つフィールドを値として読み出す（ムーブする・写す）形があれば、そのフィールドの読み出しが格納した借用を持つ必要があります。
- `loopplace`: 参照の仮引数の値が持つ借用は、呼び出し元の事実（`Part::Contents`）と、`deref p = v` の後の自分の部分の借用の検査（`replaced_target`）が読みます。隠れた環境への格納を参照の仮引数の値に足さないようにすると、その後で同じ仮引数を通して読んだ値が格納した借用を持たなくなりうるので、A31 の Copilot のレビュー 2 の点検の危険な形（`copilot2/p/u_hidden_while` の edition 2027 の形と、その `own` の形）で確かめる。

## 対象外

- A33 のモデル（記憶域の寿命と破棄の順序の 1 つのモデル）と、A33 の表のほかの行（`mix4`・`w05`・`g08`・`f02`・`slice_self_safe_elem`・`fx3`・`h01`・`crossw`・`callres`・`mutref`・`cross5`・`mapr`・`fvparam`・`coarse`・`fvlam`・`matchref`・`blockalias`・`r06`・`r07`・`r10`・`r11`・`m_named_owned`）と、A33 の「未修正の穴」。
- 明示した `ref` の仮引数を通した関数値の破棄（`fvdrop` の方針案の外）。
- セルへの書き込みを上書きとして扱うこと（`replcopy` の「同じく拒否する形」の、写した先を後で上書きする形）。セルの要素への書き込みは今と同じく借用を足すだけにします。
- 言語の意味（破棄の順序、`Drop.drop` の規則）を変えること。
