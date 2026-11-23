# A33: 格納と破棄の順序の検査に残る穴と保守的な制限を、記憶域の寿命と破棄の順序の 1 つのモデルで直す

| 項目 | 内容 |
| --- | --- |
| ID | A33 |
| 優先度 | P0（安全なコードで解放後の読み出しが起きうる穴を含む） |
| 規模 | XL（分割前提。「方針案」の段ごと） |
| 依存 | A31 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-13（A31 の 12 回のレビューの最後の回とマージ前の確認から。A31 の先端で再現を確かめた） |
| 承認 | 不要（危険なプログラムを拒否し、今拒否している安全なプログラムを受理する。言語の意味は変えない） |
| 改善する劣位 | 不具合（メモリ安全性）と、安全なプログラムを拒否する保守的な制限。A31 は、その場の書き込み・ロックのコールバック・破棄の順序の穴を形ごとの規則で塞いだので、規則の境目に穴と過剰な拒否が残る |
| 手本にする既存実装 | `src/ownership.rs` の A31 の仕組み（`StoreFacts`、`State::kept`・`kept_elsewhere`・`contents_elsewhere`・`block_shares`、`replaced_value`・`replaced_target`・`replaced_field`、`check_drop_order_of`、`borrow_dropped_first`、`path_copy`・`shared_copy`・`referent_pieces`、`StoreFacts::kept_paths`・`exact_loans`・`kept_copy`、`Checker::points_at`・`pointing`・`bound_value`、`Source::element_paths`） |
| 主な影響ファイル | `src/ownership.rs`, `tests/rc.rs`, `tests/mutable_slices.rs`, `tests/fixtures/in_place_writes/Main.tz`, `tests/features.mjs`, `_tsuzuri/language-reference/ownership-and-memory/drop.md`, `_tsuzuri/language-reference/ownership-and-memory/lifetimes.md`, `docs/language.md`, `docs/architecture.md`, `_features/README.md` |

## 目的

格納（代入・その場の書き込み・ロックのコールバック・呼び出し先の書き込み・破棄が実行する `Drop.drop`）と破棄の順序の検査を、記憶域ごとの寿命と破棄の順序を表す 1 つのモデルで行い、A31 が形ごとの規則で塞いだ範囲の外に残る解放後の読み出しを拒否し、A31 の保守的な制限が拒否する安全なプログラムを受理する。

## 背景

A31（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)）は、`Array.write` などのその場の書き込みとロックのコールバックが書き込んだ借用を、書き込み先の持ち主に残すようにしました。その後の 12 回の敵対的なレビュー（反例の探索と適合の確認。プログラムは `target/wt/a31/target/scratch/review-a31-*`）は、回ごとに新しい解放後の読み出しを見つけ続けました。多くは A31 の規則どうしの境目にあり、`main` も同じく受理する穴（`main` は格納を追跡しないので、どれも受理します）でした。A31 は見つかった穴をすべて、その回の形ごとの規則（`kept`・`kept_elsewhere`、`block_shares`、値全体・`deref p = v`・フィールドの置き換え、参照が指しうる値ごとの自己の借用など）で塞ぎました。最後の回（第12回）でも、7 件の指摘のうち 3 件が `main` にもある穴（`Rc.share` の持ち主の記録が `ref mut` の後に古いまま残る、`deref p = v` が写しや共有のブロックに残る借用を終わらせる）で、1 件が A31 の第9回以降の退行でした。

回ごとに新しい穴が見つかることは、形ごとの規則を足し続ける方法が収束しないことを示しています。このチケットは、残る穴と制限を記録し、記憶域の寿命と破棄の順序の 1 つのモデルで置き換える計画を立てます。

### 未修正の穴（安全なコードで解放後の読み出しが起きうる。`main` も受理）

A31 のマージの時点で、12 回のレビューが報告した穴（`main` も受理するものを含む。第12回の指摘 2・3・5 も）は、すべて A31 で直しました。第12回の対応の後のマージの条件の確かめ（`target/wt/a31/target/scratch/a31/gate/`: レビューと確認のすべてのプログラム 10467 個とこの領域の Rust のテストのプログラム 1700 個を、`main` と A31 の先端で両方の edition の `check --json`。`results-r13.json`・`diff-r13.txt`）でも、`main` が拒否して A31 が受理するものは安全な 15 個だけで、危険なものはありません。

ただし、`main` と A31 がともに受理するプログラム（両方の edition で 7252 組）は、名前が危険を示す 16 個（`unsafe`・`bad`・`leak`・`_u` など）を実行して読んだだけで、全部は判定していません。16 個はどれも安全でした（名前と違い、パターンの変数は束縛の逆順に解放され、捕捉は写しで、借用した場所は書き換えられない）。12 回のどの回も新しい穴が見つかったので、未修正の穴は残っていると考えるべきです。次の形は、穴かどうかを決め切れていません。

| 名前（再現のパス） | 形 | 分かっていること |
| --- | --- | --- |
| `v17`（`target/wt/a31/target/scratch/review-a31-ce/v17_display_bad_forced`、`target/wt/a31/target/scratch/a31/r13/p/v17b_display_stores_param_part_check`） | `Display` の instance の `display` が、仮引数の部分の借用（`c.name`）を、仮引数の region のフィールドが指す呼び出し元の `Mutex` へ書き込み（`record Cell {r} { m: ref {r} Mutex<...>, name: string }`）、関数の局所値をその型で文字列の補間 `"{c2}"` に入れる | どちらのバイナリも受理します。補間が実行する `display` の暗黙の呼び出しには、書き込みの事実を当てはめていません。直接の呼び出し `Display.display (ref c2)`（`r13/p/v17c_display_direct_call`）は A31 が `8:13` の `E1013`（`store_local_borrow_through_reference`）で拒否し、`main` は受理して `-O0` で不定の値（`4307242976`）を出します。補間の形を実行すると、`display` の後もセルは元の関数のまま（`v17b` の `inside` が 0）で、解放後の読み出しは観測していません。補間の脱糖が `display` を何に対して呼ぶかを確かめ、書き込みが起きうるなら事実を当てはめる |

A31 の Copilot のレビューの後のレビューの第2回（2026-11-14）の確かめで、格納の検査の外にある次の穴も見つけました。型の検査（捕捉の規則）の穴で、このチケットの格納と破棄の順序のモデルとは別に直す必要があります（[NS08](_completed/NS08-owned-call-partial-capture-check.md) で修正済み）。

| 名前（再現のパス） | 形 | 分かっていること |
| --- | --- | --- |
| `owned2`（`target/wt/a31/target/scratch/a31/rr2/d/d6_owned_call_moves_bound`、`d4s_owned_call_holder_dropped_first`） | 2 つの仮引数の名前付き関数を `Owned.function` で包み（`let w = Owned.function wr`、`wr :: own Holder -> (unit -> i64) -> i64`）、`Owned.call (ref w) (Holder { ... })` で、コピーできない値（`Mutex` を持ち `Drop` を持つ `Holder`）を束縛した部分適用を作り、それを 2 回呼ぶ（または 1 回呼んで捨てる） | **[NS08](_completed/NS08-owned-call-partial-capture-check.md) で修正済み**: `Owned.call` の結果が関数型のときに第 2 引数へ `Capture` を要求するようにし、直接呼び出し・パイプライン・関数値・ジェネリック単相化のすべてで `E1005` で拒否されるようになった。`Arc` を束縛する形（`d7_owned_call_arc_moves`）は呼び出しごとに複製するので安全に受理される |

レビューが重点的には調べていない形（暗黙の呼び出しを含むもの）: 文字列の補間、コンピュテーション式のビルダーの操作（`Bind`・`Return` など、`src/derive.rs` が呼ぶ helper）、`deriving` が生成する instance、`use` の束縛と `Owned.drop`、非同期のタスク。

### 保守的な制限（`main` が受理する安全なプログラムを A31 が拒否する。文書に書いたもの）

A31 の先端（`target/wt/a31`、edition 2027 と 2026）で確かめました。どれも `drop.md` か `lifetimes.md` に安全側の扱いとして書いてあります。

| 名前（再現のパス） | 形 | A31 の先端 | `main` |
| --- | --- | --- | --- |
| `mix4`（`target/wt/a31/target/scratch/a31/r10/sm/mix4`） | 自分の部分の共有の借用を持つ値へ、もう一度 `ref mut p0` を渡す（`target (ref mut p0)` を 2 回）。書き込みの事実は呼び出し先が書き換える場所を持たない | `17:20` `E1014` | 受理（`drop 8`） |
| `w05`（`target/wt/a31/target/scratch/review-a31-ce-r11/w05_keep_then_caller_deref_replace`） | 自分の部分の借用を持つ値を `let r = ref mut p0; deref r = mkp 7` で置き換える（`mix4` と同じ理由） | `25:13` `E1014` | 受理 |
| `g08`（`target/wt/a31/target/scratch/a31/r12/adv/g08_pipe_share`） | パイプライン `ref rc \|> Rc.share` で作った持ち主を通したブロックの部分の借用。直接の呼び出し `Rc.share (ref rc)` だけを同じブロックの持ち主とみなす | `57:5` `E1013` | 受理 |
| `f02`（`target/wt/a31/target/scratch/a31/r12/adv/f02_sibling_holds_loans`、edition 2027） | 兄弟のフィールドが借用を持ちうる値（`record Box { h: Holder, g: Holder }`）のフィールドの代入。古い値の借用がどのフィールドにあったか分からない | `35:5` `E1013` | 受理 |
| `slice_self_safe_elem`（`target/wt/a31/target/scratch/review-a31-conf-r10/p/slice_self_safe_elem`）と `arr_elem_safe` | 配列やビューの要素の部分の借用を同じ要素へ書き込む（安全な順序）。要素は添字を区別しない | `15:5` `E1013`（`self_referential_store`） | 受理 |
| `fx3`（`target/wt/a31/target/scratch/a31/r10/e2e/fx3`、edition 2027） | 書き込むセルを自分で所有する関数値があるプログラムで、本体の見えない関数値へ `ref mut p` を渡す（隠れた環境の規則で、どの部分か分からない自己の借用） | `1056:5` `E1013` | 受理 |
| `h01`（`target/wt/a31/target/scratch/review-a31-conf-r12/p/h01_parts_two_params_safe`） | 2 つの参照の仮引数の先の部分を関数値へ渡す（`fn apply2 f a b = { f (ref a.h) (ref b.s); 0 }`）呼び出しの一時値の順序。`Source::passed` と `origin` は仮引数をそのまま渡すときしか表さない | `104:12` `E1013` | 受理（`drop 8 2 drop 0 2`） |
| `crossw`（`target/wt/a31/target/scratch/a31/r14/q/viafield_ok`・`viaref`） | 実引数の値が持つ参照の先の部分（`record Wrap {r} { p: ref {r} PairOk }` の `ref w.p.s`、`w.p` をそのまま渡す `cross a w.p`）を別の仮引数の先へ書き込む関数の呼び出し。region が 1 つの仮引数の外部の loan は先と region の両方を表すので、本体はその借用の場所を `w` の先の部分と区別できず（`Checker::exact_loan`）、呼び出し元は `w` 全体の借用を書き込み先に残し、その後の格納が `w` を通して `p1` へ届くものとして扱う | `20:5` `E1013`（`assignment_extends_borrow`） | 受理（`drop 8`・`drop 0`・`1`） |
| `callres`（`target/wt/a31/target/scratch/a31/r15/p/c1_callres_alone`、`target/wt/a31/target/scratch/a31/r14-review/conf/p7/s_tree_call`） | 呼び出しの結果（`fn child n = Rc.get (ref n.kids[0])`）を、その部分の借用を別の仮引数の先へ書き込む関数へ渡す（`crossn (ref p2) (child (ref n1))`）。結果は実引数の loan を持ちながら、その場所から届く別の値を指しうるので（`Checker::points_at` が偽）、呼び出し元は `n1` 全体の借用を `p2` に残し、その後の格納が `p2` を通して `n1` へ届くものとして扱う。`Rc.get`・`Arc.get` と参照の仮引数をそのまま返す関数（`returned_parameters`）の結果だけは指す場所が分かる | `34:5` `E1013`（`self_referential_store`） | 受理（`drop 8`・`drop 0`・`drop 0`・`1`） |
| `mutref`（`target/wt/a31/target/scratch/a31/r15/p/c2_mutref`、`target/wt/a31/target/scratch/a31/r14-review/conf/p/s_loop_refvar`） | `let mut r = ref p1` の参照の変数を、別の仮引数の先へ部分の借用を書き込む関数へ渡す。可変の変数は別の値を指しうるので、指す場所を記録しない（`Checker::pointing` は不変の局所値だけ） | `34:5` `E1013`（`self_referential_store`）、`24:5` `E1013`（`assignment_extends_borrow`） | 受理（`drop 8`・`drop 0`・`1`、`drop 8`・`drop 0`・`drop 0`・`1`） |
| `cross5`（`target/wt/a31/target/scratch/a31/r14-review/conf/p9/s_cross5`） | 1 つの仮引数の先の 5 つの部分の借用を別の仮引数の先へ書き込む（`cross5 (ref q2) (ref q1)`）。`add_self_path` は 5 つ目で先全体（`[]`）にするので、呼び出し元は `q1` 全体の借用を `q2` に残し、その後の格納が `q1` へ届く | `29:5` `E1013`（`self_referential_store`） | 受理（`drop 4`・`drop 0`・`1`） |
| `mapr`（`target/wt/a31/target/scratch/a31/r14-review/conf/p3/s_map_partial`） | std の `Array.mapr`（`new ['b](values.length, \index -> transform (ref values[index]))`）へ、部分の借用を書き込む関数の部分適用を渡す。配列を作る初期化の関数の格納を区別しないので、`mapr` の事実は `transform` を `values` へ格納しうるものになり、`arr` が `p2` の借用を持つものとして扱う | `26:5` `E1013`（`assignment_extends_borrow`） | 受理（`drop 8`・`drop 0`・`drop 0`・`1`） |
| `fvparam`（`target/wt/a31/target/scratch/a31/cp1/fv2/b_s_param_partial`・`b_s_param_pipe`・`b_s_param_partial_alias`） | 関数値の仮引数を本体の中で部分適用・パイプラインで呼ぶ関数（`fn applyp f a b = { let g = f a; g b; 0 }`、`fn applyq f a b = { b \|> f a; 0 }`）へ、部分の借用を別の仮引数の先へ書き込む `cross` を渡す（`applyp cross (ref p2) (ref p1)`）。高階の事実（`HigherOrderCall`）は仮引数をすべての引数で呼ぶ形だけを記録し、部分適用した `g` は本体の見えない関数値（`apply_opaque`）なので、呼び出し元はどの実引数の借用もほかの実引数の先へ入りうるものとして扱う | `27:5` `E1013`（`assignment_extends_borrow`） | 受理（`drop 8`・`drop 0`・`1`） |
| `coarse`（`target/wt/a31/target/scratch/a31/r16-review-1-ce/p/tp_ctl_s`） | 呼び出し先が関数値の仮引数へ渡す値の場所が 4 段より深い（`applyx x f = ref (deref (Rc.get (ref x.a.b.c.d.e[0]))) \|> f`）か、渡しうる場所が入れ子（`if k > 0 then ref (deref (Rc.get (ref x.a.e[0]))) else ref x.a`）なので、記録した場所（`[a, b, c, d]`、`[a]`）は借用した場所を含む大きな部分（`Source::coarse`）。呼び出し元はその部分の借用を持ち、関数値が書き込んだ `b.s` の場所へは狭めないので、借用されていない外側の `x.a.b.c.d.s` の書き換えも競合する。仮引数をそのまま渡す関数（`fn applyx2 x f = applyx x f`）を通した呼び出しも同じ（その関数も外側の場所を記録する。`holding_path`） | `31:5` `E1014`（`borrow_conflict`） | 受理（`drop 8`・`1`） |
| `fvlam`（`target/wt/a31/target/scratch/a31/rr2/w2/l_applypl`・`l_applyyl`、`target/wt/a31/target/scratch/a31/r17-review-3-conf/p/l/s_pc`・`s_pg`・`s_pd`・`s_pn`・`s_pl`、`target/wt/a31/target/scratch/a31/r17-review-3-conf/p/w/s_c_mvz`、`target/wt/a31/target/scratch/a31/rr3/t/sx_p0l_mvz`・`sx_p0n_mvz`、edition 2026） | 関数値の仮引数を捕捉するラムダを通した呼び出し: 引数にしたラムダ（`fn applypl x f = applyp x (\d -> f d)`）、すぐに呼ぶラムダ（`fn applyyc k x f = (\(_u: unit) -> applyy k x f) ()`、`fn applypd x f = (\(_u: unit) -> { ref x.a \|> f; 0 }) ()`）、束縛してから呼ぶラムダ（`fn applypn x f = { let g = \(_u: unit) -> { ref x.a \|> f; 0 }; g () }`）へ、環境から届くセルへ部分の借用を書き込む `crossd (ref p2)` を渡す。edition 2026 のラムダは関数値を値で捕捉して所有するので、ラムダの解放（`Checker::drop_stores`）は関数値の環境のコードを実行しうるものとして、ラムダが持つ借用（関数値と、ほかの捕捉が持つもの）を関数値の環境へ格納し、`applypd` の事実は `(1, Value, 1)`・`(0, Value, 1)` を持つ。解放した値は本体が作ったラムダで、仮引数から受け取った値そのものではないので `StoreFacts::drop_only` にならず、呼び出し元は `crossd (ref p2)` が本体の見えるコードで解放に何も実行しないことを使えない。edition 2027 の読み取りの仮引数は呼び出し元の関数値を借りたものなので、ラムダの解放は関数値を解放せず、受理する（捕捉の狭め方は A31 のレビューの第3回で直した。関数値をラムダの引数として渡して破棄させる形は [A34](A34-store-check-safe-rejections.md) の `fvdrop`） | `39:5`・`60:5` `E1013`（`self_referential_store`） | 受理（`x.a.s` が `"abcabc"` なので `drop -1`・`1`） |
| `matchref`（`target/wt/a31/target/scratch/a31/r16-review-2-ce/k/s_matchq`・`s_matchq_pipe`・`s_matchtup`・`s_nestmatch`、`target/wt/a31/target/scratch/a31/rr3/k2/v_match_direct`） | `match` の対象に書いた借用（`match ref p2 with`、`match (ref p1, ref p2) with`）を束縛したパターンの変数を、部分の借用を別の仮引数の先へ書き込む関数へ渡す（`match ref p2 with \| q -> crossr q (ref p1)`、`q` を束縛した部分適用 `crossr q`）。パターンの変数は対象の値の別名（`State::aliases`）で、対象が局所値でないときは指す場所を記録しない（`Checker::points_at`）ので、`p1` は `p2` 全体の借用を持ち、その後の格納が `p2` のセルへも届く。`let r = ref p2` の後の `match r with` は受理する（A31 のレビューの第3回から） | `23:5` `E1013`（`self_referential_store`） | 受理（`drop 8`・`drop 0`・`1`） |
| `blockalias`（`target/wt/a31/target/scratch/a31/r16-review-2-ce/k/s_blockfn`・`s_blockfn2`・`s_blockfn_part`） | ブロックの中で束縛した関数値の別名を通した部分適用（`let chosen = { let f = crossr; f (ref p2) }`、`let chosen = { let f = crossr; f }` の後の `let g = chosen (ref p2); g (ref p1)`）。ブロックの中の別名の部分適用は既知の関数値にならず（`known_functions`・`known_heads`）、本体の見えない関数値（`apply_opaque`）なので、どの引数の借用もほかの引数の先へ入りうるものとして扱う。別名をブロックの外で束縛する形、`{ let f = crossr; f }` を直接すべての引数で呼ぶ形、`{ crossr (ref p2) }` は受理する | `23:5` `E1013`（`assignment_extends_borrow`） | 受理（`drop 8`・`drop 0`・`1`） |
| `rcgetpart`・`fvdrop`・`refsrc`・`sharetype`・`rcgetowner`・`shareparam`・`replcopy`・`cellslots`（[A34](A34-store-check-safe-rejections.md)） | A31 の Copilot のレビューの後のレビューの第4回・第5回で表に足した 6 つの形（フィールドや要素の場所の `Rc.get` を束縛した変数、受け取った関数値を持つ値の破棄、書き込む部分の側の参照の変数、使い終えた共有の持ち主を通した格納の型の判定（第6回で、元の所有者のほかの部分のブロックと、dyn の値・型引数に届く型の形を足しました）、`Rc.get (ref r)` の借用の持ち主、参照の仮引数から作った共有）と、A31 の Copilot のレビュー 2 の対応のレビューで見つけた 2 つの形（ループの中で置き換えてから新しい値の中身を写す `replcopy`、セルのフィールドへ借用を書き込んだ複数 region のレコードの `cellslots`）は、項目ごとに直すチケット [A34](A34-store-check-safe-rejections.md) に移しました（検査が終わらない `loopplace` も、下の「検査の時間」から A34 へ移しました）。最小の再現のプログラム、危険な双子、原因と方針はそちらにあります | A34 の「現状」 | 受理 |
| `r06`・`r11`（`target/wt/a31/target/scratch/review-a31-conf-r12/p/r06_rs_good_same_scope_reassign`・`r11_rs_good_same_scope_move`） | `Rc.share (ref rc)` の持ち主を通して `rc` のブロックへ自分の部分の借用を書き込み、持ち主がスコープにある間に `rc` を代入・ムーブする。持ち主が借用を持ち続ける | `105:5`・`105:17` `E1014` | 受理 |
| `r07`・`r10`（同じディレクトリの `r07_rs_good_inner_block_move`・`r10_direct_self_move_rc`） | ブロックが自分の部分の借用を持つ `Rc` のムーブ（`main` も `Rc.get` の借用が生きている間のムーブは `E1014`。ブロックは動かないので実際には安全） | `103:17`・`104:17` `E1014` | 受理 |
| `apply2` の注釈した綴り（`target/wt/a32/target/scratch/a32/merge5/s01/red/p/tmp_safe__annot`・`tmp_safe__alias`。A32 の 5 度目の merge で記録） | `h01` と同じ一時値の順序で、仮引数の部分ではなく、`def` の右辺のラムダの注釈した引数（隠れた引数をパターンで束縛した名前）をそのまま関数値へ渡す（`\f (a: ref PairOk) (b: ref PairOk) -> { f a b; 0 }`）。名前付き関数と注釈の無いラムダは受理する。A32 の前から書ける `ref` の型の署名の綴り（`tmp_safe__alias`）も同じ結果 | `18:5` `E1013` | 受理（`drop 8`・`drop 0`・`1`。注釈した規約の綴りは `main` では `E1003`） |
| `m_named_owned`（`target/wt/a31/target/scratch/a31/L_m_named_owned`） | 所有の値だけを書く名前付き関数をロックのコールバックにする。ロックの組み込み関数はコールバックを実行する見えないコードなので、A28 の除外が事実を使わない | `13:5` `E1013` | 同じく拒否（A30 の誤った拒否） |

A32 の 5 度目の merge（2026-11-13）が見つけた、名前付き関数の直接の呼び出し `cross (ref p2) (ref p1)`（`cross a b` が `b.s` の借用を `a.h` の `Mutex` へ書き込む。`target/wt/a32/target/scratch/a32/merge5/s01/red/p/loc_direct`）の文書に無い拒否は、A31 のマージ前の追加の修正（`StoreFacts::kept_paths`。[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「マージ前の追加の修正」）で直しました。その修正のレビュー（2026-11-13、`target/wt/a31/target/scratch/a31/r14-review/`）が、修正が `loan` の場所をその借用した部分とみなしたこと（呼び出しの結果を通した借用 `let q = child b; ref q.s` を `b.s` の借用とした）による退行と、部分適用・関数値へ渡す部分の綴りの残りの拒否を見つけ、A31 で直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「マージ前の追加の修正の見直し」）。指す場所が分からない参照を通した綴り（上の `crossw`・`callres`・`mutref`）、5 つ以上の部分（`cross5`）と `Array.mapr`（`mapr`）が残ります。書き込むセルを自分で所有する関数値があるプログラムでは、部分適用が束縛した参照の引数への格納も隠れた環境へ入るものとして扱うので（上の `fx3` と同じ規則）、`let g = cross3 (ref p3) (ref p1); g (ref p2)` も拒否されます（`tests/fixtures/in_place_writes` のモジュール。パイプラインと名前付きの関数値は受理します）。

A32 の 6 度目の merge（2026-11-14）が見つけた、名前付き関数を束縛した局所値（`let chosen = cross`、名前付き関数を選ぶ `if`）のパイプライン `ref p1 |> chosen (ref p2)` と部分適用 `let g = chosen (ref p2); g (ref p1)` の文書に無い拒否（`target/wt/a32/target/scratch/a32/merge6/fv/`、`fvpart`）は、A31 で直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「Copilot のレビューへの対応（第1回）と fvpart」）。その調べで見つけた、関数値の仮引数を本体の中で部分適用する形（上の `fvparam`）が残ります。同じ調べで、セルを自分で所有する関数値（`mk a = \k f -> ...` が `Arc<Mutex<...>>` を値で捕捉して返すラムダ）を一部の引数に当てはめた値（`let mut w2 = w 0`）を通して局所値の借用をセルへ書き込む穴（`main` も受理し、解放した値を読む。`target/wt/a31/target/scratch/a31/cp1/reg/`）も見つけ、A31 で直しました（`sink_flags` で、コードを実行する前に 2 つ以上の引数を取る関数値を作る所が、その部分適用の型（残りの仮引数の型、`partial_types`）も同じく数える。最初の修正は関数値の呼び出しの結果に当てはめた関数値を持たせる形で、`Owned.call (ref w) 0`・`new [T](1, w)`・`Array.init 1 w`・`FixedArray.init w` を通した同じ穴が残り、`Array.foldr` の状態の型を所有する関数値の型にして `main` が受理する安全な形を拒否していたので、Copilot のレビューの後のレビューの第1回で今の形にしました。`target/wt/a31/target/scratch/a31/rr1/h/`）。同じ回に、関数値の仮引数へ渡す値の場所を短くした記録（上の `coarse`）を、その値が指す場所として扱っていた先端からの退行（`main` も受理する危険な形。`target/wt/a31/target/scratch/a31/r16-review-1-ce/p/pm_move`・`tp_move`）と、`match`・ブロック・部分適用の選択を束縛した局所値の文書に無い拒否（`target/wt/a31/target/scratch/a31/rr1/k/`）も直しました。その後のレビューの第2回で、名前付き関数を `Owned.function` で包んだ値の `Owned.call (ref w) cell`（とそのパイプライン、`Owned.function` の引数を別名や `wr3 0` にした形）、`List.mapr wr (ref cells)`、`List.foldr fold zero (ref cells)` のように、組み込み関数が受け取った関数値を少ない引数に当てはめて作る部分適用を通した同じ穴（`main` も受理し、解放した値を読む。`target/wt/a31/target/scratch/a31/r16-review-2-ce/n/`、`rr2/n/r/`）を直し（`builtin_partial_applications`）、部分適用の型を数えるのをモジュールが作りうる部分適用の型だけにして、その形の部分適用を作らないプログラムで無関係な同じ型の関数値を拒否していた前の先端からの退行（`r16-review-2-ce/c/c4_arr`・`c5_arr27`、`rr2/c/`）を直し（`partial_results`）、仮引数をそのまま渡す関数を通した `coarse` の呼び出しで呼び出し元が値全体を借用していた前の先端からの退行（`r16-review-2-conf/p/w_*`、`rr2/w/`）を直しました（`holding_loans`）。同じ確かめで見つけた、格納の検査の外にある `Owned.call` の穴（上の `owned2`）と、edition 2026 のラムダで包んだ関数値の仮引数の拒否（上の `fvlam`）が残ります。反例の探索が生成したプログラム（`r16-review-2-ce/g`）の検査で、部分適用が場所の借用でない引数を束縛する形の拒否（`bindref`。前の先端からあり、`main` は受理します）も見つけ、文書に書きました。その後のレビューの第3回で、`bindref` と、仮引数を部分適用で渡す関数（`fn applyyq k x f = { let g = applyy k x; g f }`）・仮引数を捕捉するラムダで渡す関数（`fn outerl a b = (\(_u: unit) -> cross a b) ()`、`fn applyyc k x f = (\(_u: unit) -> applyy k x f) ()`）の文書に無い拒否（`main` は受理。`target/wt/a31/target/scratch/a31/r17-review-3-conf/p/q/`・`x/`・`l/`・`w/`）を直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第3回」）。束縛した引数の部分を、関数値を呼ぶ所の状態（そこでは束縛した参照の仮引数はもう生きていない）ではなく、借用を評価した所で exact だったかで決め（`Checker::inexact_borrows`）、ラムダの捕捉も、ラムダを作った所で捕捉した局所値が指していた場所で狭めます（`captured_loans`）。edition 2026 で関数値の仮引数を捕捉するラムダの形は、上の `fvlam` の解放の規則で拒否したままです。同じ回のマージの条件の確かめで、第2回の反例の探索のプログラム（`r16-review-2-ce/k/`）のうち、前の先端から拒否していて文書に無かった安全な形（上の `matchref`・`blockalias`）を見つけ、表に足しました。その後のレビューの第4回で、ラムダの捕捉の狭め（第3回）が `let a = Rc.get (ref r)`（`let r = Rc.share (ref r2)`）を捕捉したラムダの格納を `r` だけへ入れ、`r2` が借用された値より後にブロックを解放する退行（`target/wt/a31/target/scratch/a31/r18-review-4-ce/p/u_rc_lam` など）と、同じ原因で `main` も受理する穴（その局所値を直接・部分適用・パイプライン・関数へ渡す形、呼び出しの無い `Mutex.with_lock (ref a.h.cell) ...`、パイプラインの共有・フィールドや要素に入れた共有を通した形、同じブロックの自分の部分を `PairBad` へ書き込む形。`u_rc_*`・`u_rcf_*`・`u_rcp_*`・`u_rcself_bad_*`）を直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第4回」。使われなくなった共有の局所値の値をブロックが持つ借用に読み替える `keep_for_drop` が、共有の元の所有者への道（共有の loan）を消していたので、その局所値への以後の格納を元の所有者へも届けます: `share_sources`）。同じ回の文書に無い拒否（どれも前の先端からあり、`main` は受理）を `rcgetpart`・`fvdrop`・`refsrc` として表に足し、修正が型で判定するために拒否するようになった安全な形を `sharetype` として足しました（どれも後に [A34](A34-store-check-safe-rejections.md) へ移しました）。その後のレビューの第5回で、第4回の修正が元の所有者へ届ける型を、読み替えた局所値の型が届くすべてのブロックの型から、元の所有者の型も届くブロックの型（共有したブロックとその中のブロック）に狭め（`export_shared_block`。共有を持つ値の型だけが届く無関係な `Rc<Other>` への格納 `target/wt/a31/target/scratch/a31/r18-review-5-ce/g5/t_rh2_other` を受理するようにしました）、型の判定が残す形（ブロックでない値、別の型のブロック、共有したブロックの中のブロックの型）を `sharetype` に足しました。同じ回の文書に無い拒否（どれも前の先端からあり、`main` は受理）として、参照の仮引数を受け取る関数へ渡した `q1` と再借用を束縛した `q1` のラムダの捕捉（`refsrc`）、関数値の仮引数を入れたリテラルと edition 2027 の読み取りの仮引数やそのフィールドを束縛した局所値の破棄（`fvdrop`）を、それぞれの行に足しました。同じ回のマージの条件の確かめで、そのレビューのプログラムのうち前の先端から拒否していて文書に無かった安全な形（`rcgetowner`・`shareparam`）を見つけ、表に足しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第5回」）。この 6 つ（`rcgetpart`・`fvdrop`・`refsrc`・`sharetype`・`rcgetowner`・`shareparam`）は、A31 のマージの前に、項目ごとに直すチケット [A34](A34-store-check-safe-rejections.md) へ移しました（上の表の 1 行）。 A31 の Copilot のレビューの後のレビューの第6回（2026-11-15）で、dyn の値の中の共有を通した格納の穴を直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「レビューの第6回」）。dyn の値のメソッドは中身の場所の参照を返せ、型の走査はその中を見ないので、共有を持つ値の型が dyn の値に届く形（`H { d: Dyn.of (Box1 { l: Rc.share (ref r2) }) }` を通した `r2` のブロックへの書き込み）、元の所有者のブロック全体を共有して、その dyn のフィールドの中のブロックへ書き込む形、共有の共有を dyn の値に入れた形、呼び出し元の配列へも共有を入れた形と、逆の向き（セルを持たないブロックの共有を持ち続けた後、元の所有者の dyn のフィールドの中のブロックへ書き込む形、そのブロックの共有を呼び出し元の配列へ入れた形）は、`main` も受理し、解放したブロックを読みます（`target/wt/a31/target/scratch/a31/rr6/n/u_hrecdyn`・`u_bdyn_whole`・`u_chain_dyn`・`u_extchain`・`u_extchain_rpfirst`・`u_rev_nocell`・`u_rev_nocell2`・`u_rev_lchain`・`u_ext_nocell`・`u_ext_lchain`。ASan は heap-use-after-free）。 A31 の Copilot のレビュー 2（2026-11-15）で、ループの不動点の穴を直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「Copilot のレビューへの対応（第2回）」）。不動点は反復の始まりの状態の要約を比べ、要約が変わらなかった反復の出口の状態から続けるので、要約に無い事実（`State::kept_elsewhere`・`contents_elsewhere`、局所値が参照の仮引数を通して読んだ中身としてだけ持つ loan の印 `Value::through`）だけを変えた本体の書き込みは、ループの後で失われていました。仮引数の先のセルの中身をほかの仮引数の先へ写すか、先が自分のセルに持つ借用をほかの先へ格納するループの後の `deref p = v`（`while`・`for`・`break`・`continue`・入れ子・中の `try`・`match`・`if` のどの形も）と、仮引数の中身を読んだ局所値を、仮引数を捕捉するラムダに束縛し直すループの後の格納は、`main` も受理し、解放した文字列を読みます（`target/wt/a31/target/scratch/a31/copilot2/p/u_ce_*`・`u_ke_*`・`u_th_while`。`u_ce_while` は ASan で heap-use-after-free）。要約は、`State` のすべての事実（`moved_always`・推論のジェネリックなムーブ・region の slot も）と、loan ごとの場所・可変性・印（共有、view、`through`、`exact_loans`・`ordered_loans`・`holding_loans`）、`exports` の数を比べるようにしました。 その修正のレビュー（2026-11-15）で、要約に足した `contents_elsewhere` と region の slot のために、ループの中で置き換えてから新しい値のセルの中身を写す形と、不変な複数 region のレコードのセルのフィールドへループの中で借用を書き込む形が、前の先端からの退行として安全な形の拒否になったことが分かり（ループの無い形と `if` の形は前の先端も拒否していました。`target/wt/a31/target/scratch/a31/copilot2-review-ce/p/s_rc_*`、`copilot2-review-conf/p/rg_cell_*`）、上の表の A34 の行（`replcopy`・`cellslots`）に足しました。

起票の依頼が挙げた `N_closed_cell_read`（`target/wt/a31/target/scratch/a31/N_closed_cell_read`）は、A31 の先端で両方の edition とも受理され、値は `main` と同じです（A31 の閉じた関数の取り消しの後も closed のまま）。制限ではありません。

### 検査の時間

`Rc.share` の持ち主の連なり（`let s_i = Rc.share (ref s_{i-1})` を n 個、最後の持ち主のフィールドの借用を最初の持ち主のセルへ書き込む、ブロックの型が `Drop` を持つ）の所有権の検査は、A31 の第12回の対応の後も n の 2 乗で伸びます（`target/wt/a31/target/scratch/review-a31-ce-r12/_tools/gen_chain.py cellshares n`、開発用のビルド: n = 400 で 121 ms、800 で 501 ms、1600 で 2009 ms。`main` は 800 で 86 ms）。第12回の対応で 3 乗（800 で 5009 ms）から下げました。束縛ごとの破棄の hook が持ち主の連なりを展開する（`expand_shares`）ためで、現実のプログラムの形ではありません。A31 の Copilot のレビューの後のレビューの第4回の修正（`keep_for_drop` が共有の元の所有者へも `export_block` する）で、2 乗のまま定数がおよそ 2 倍になりました（同じ負荷の機械で続けて測った所有権の検査: n = 400 で 273 ms（修正前 138 ms）、800 で 1210 ms（550 ms）、1600 で 5109 ms（2418 ms）。`target/wt/a31/target/scratch/a31/rr4/chain-times.txt`）。読み替えた共有ごとの正確な道（[A34](A34-store-check-safe-rejections.md) の `sharetype`）は 3 乗になったので採っていません。

A31 の Copilot のレビュー 2 の点検（2026-11-15）で、ループの検査が終わらない形を見つけ、ここに edition 2026 の形（`ref mut` の仮引数の先のセルの中身を、値の仮引数 `sink` を捕捉したラムダを通して隠れた環境へ書き込むループ。`target/wt/a31/target/scratch/a31/copilot2/p/s_hidden_while`・`u_hidden_while`）として記録していました。その後のレビュー（Copilot のレビュー 2 の対応のレビュー、`target/wt/a31/target/scratch/a31/copilot2-review-conf/ph/`）で、edition 2027 でも、`ref` の仮引数でも、`own` の仮引数や局所値の `Arc.share` を捕捉したラムダでも、`Drop`・置き換え・ループの前の借用が無くても起きることが分かりました。条件は、先にセルを持つ参照の仮引数の先の借用を、書き込むセルを所有する関数値へ渡すループです。セルを所有する関数値への格納（`Checker::store_hidden_except`）が仮引数の値にも先の場所の借用を足し、`deref p` の場所はその借用の場所からも作る（`Checker::place`）ので、反復ごとに場所が深くなり、ループの要約が変わり続けます（反復の上限の `E1017` に届く前に、検査が 10 分たっても終わりませんでした。`main` は 1 秒以内に受理します）。最小の再現（edition 2027 の `ref mut Box` の仮引数と `own` の `Arc` の仮引数）、同じく終わる・終わらない形、危険な双子、原因と方針案は [A34](A34-store-check-safe-rejections.md) の `loopplace` に移しました。A31 の最後に、この形の検査は終わらない代わりに `E1017`（`loop_ownership_limit`）で止まるようにしました（収束していない反復の借用が増え続けると打ち切る、`src/ownership_control.rs` の `LoanGrowth`: 5 回続けて増え方が速くなったとき、24 回続けて（200 個を超える借用を作る反復なら 8 回続けて）前の反復が増えた数以上に増えたとき、8 回続けて反復の後の状態が持つ借用の増え方が速くなった後で、前の反復が増えた数以上に増えたとき。借用の数そのものには上限を置きません。この形はリリースビルドで 0.2 秒ほど。借用をほとんど作らない文の多い本体で局所値に何度も渡す形は、24 回続けて増えるまで止まらず、何秒もかかります（A34 の `loopplace`）。場所の形から推し量った `773ea1c4` と `5f3c2a0b` の数え方は収束するループを止め、止まらない形を残しました。最初の反復より 500 個多い反復で止めた `dbbe8338` は、一度だけ大きく増えて収束するループを止め、止まるまで 10 秒以上かかる形を残しました。どれも置き換え、後に残した 5 回目からの反復の借用の数の上限も、先に止めるのが収束するループだけだったので外し、そのために遅くなった、収束していく部分と同じループにある形を状態の借用の増え方で止めるようにしました）。安全な形の受理は A34 です。

## 再現

上の 2 つの表の各行のディレクトリに `Main.tz` があります（A34 へ移した行のプログラムは、A34 のチケットの「現状」に書いてあります）。A31 のワークツリーで:

```sh
cd target/wt/a31
B=target/debug/tsuzuri                      # A31 の先端の開発用のビルド
M=/Users/tmidorikawa/pj/Tsuzuri/target/scratch/base/tsuzuri   # main（b3dd7f1c）
for d in <表のディレクトリ>; do
  for e in 2027 2026; do
    (cd $d && $B check --edition $e Main.tz; $M check --edition $e Main.tz && $M run --edition $e -O0 Main.tz)
  done
done
```

`target/wt/a31/target/scratch/a31/r13/t.sh <dir>...` は、A31 の先端・第12回の対応の前・`main` の最初の誤りと、受理するバイナリの `-O0`・`-O3` の出力を 1 行に並べます。

## 期待する振る舞い

- 「未修正の穴」の表のプログラムは、両方の edition で拒否する（code と kind は既存のもの: `E1013` の `assignment_extends_borrow`・`borrow_outlives_block`・`self_referential_store`・`store_local_borrow_through_reference`、`E1014` の `borrow_conflict`）。
- 「保守的な制限」の表のプログラムは、両方の edition で受理し、`main` と同じ値を出す（`m_named_owned` は `main` も拒否するが、安全なので受理する）。受理できない形が残るなら、その理由を文書に書く。A34 へ移した 9 つの形（`loopplace` を含む）は A34 が直す（A34 より先にこのチケットのモデルが入るなら、A34 の「現状」の安全なプログラムを受理し、危険な双子を拒否することも、このチケットの条件にする）。
- A31 のテスト（`tests/rc.rs`・`tests/mutable_slices.rs`・`tests/currying.rs`・`tests/borrowed_records.rs`・`tests/user_drop.rs` など）と E2E（`in_place_writes`・`mutable_slices`・`rc` ほか）の期待は変えない（制限の表の行を受理に変えるテストを除く。変える前にこのチケットに記録する）。
- 検査の時間は、`Rc.share` の連なりで n にほぼ比例する。

## 方針案

### 1 つのモデル

形ごとの規則の代わりに、検査の状態に次の 2 つを明示的に持たせます。

1. **記憶域の寿命**: 各 loan が指す記憶域（局所値・仮引数の先・`Rc`/`Arc` のブロック・関数値の環境・隠れた環境・一時値）と、その記憶域がいつ解放されうるか（スコープの終わり、置き換え、ムーブ、最後の持ち主の解放、呼び出し元の値として呼び出し元で）。ブロックの持ち主の組（`block_shares`）や、置き換えで終わる借用（`kept` と `kept_elsewhere`）は、この表の特別な場合です。
2. **格納の場所**: 各格納が書き込む記憶域と、そこから届く記憶域。今は持ち主の局所値と外部の根だけで表し、根より下の場所（セルが値の中の `Rc` のブロックにあるか、参照の先にあるか）は `place_leaves_storage` のような後付けの判定で補っています。格納先の記憶域を loan の場所と同じ粒度で記録すれば、置き換え・解放のたびに「その記憶域と一緒に終わる借用」と「生き残る記憶域へ写った借用」を同じ規則で分けられます。

破棄の順序の規則（借用を読みうる `drop` を実行する値は、より後に解放される記憶域の借用しか持てない）は、この 2 つから導きます。値全体・フィールド・`deref p = v` の置き換えも、記憶域の解放の 1 つとして同じ規則で扱います。

### 段（案）

1. 記憶域の表を足し、今の規則の結果を変えずにそれで表す（`kept`・`kept_elsewhere`・`contents_elsewhere`・`block_shares` を表の問い合わせに置き換える）。A31 のテストと E2E、レビューのコーパス（下）の結果が変わらないこと。
2. 格納先を場所の粒度で記録し、置き換え（`replaced_value`・`replaced_target`・`replaced_field`）とムーブを記憶域の解放として扱う。「未修正の穴」と、制限のうち `mix4`・`w05`・`r06`・`r07`・`r10`・`r11`・`f02` を直す。
3. 要素の添字（`slice_self_safe_elem`）、2 つの仮引数の部分（`h01`）、参照の先のフィールドを通した部分（`crossw`）、指す場所が分からない呼び出しの結果と可変の参照の変数（`callres`・`mutref`）、5 つ以上の部分（`cross5`）、`Array.mapr`（`mapr`）、パイプラインの共有（`g08`）、隠れた環境（`fx3`）、ロックのコールバックの名前付き関数（`m_named_owned`）、関数値の仮引数の部分適用（`fvparam`）、短くした場所（`coarse`）、edition 2026 の関数値の仮引数を所有するラムダの解放（`fvlam`）、`match` の対象に書いた借用のパターンの変数（`matchref`）、ブロックの中の関数値の別名の部分適用（`blockalias`）を、表が区別できる範囲で直す。`owned2` は型の検査（`polymorph::captures`）で直す。`rcgetpart`・`fvdrop`・`refsrc`・`sharetype`・`rcgetowner`・`shareparam` は [A34](A34-store-check-safe-rejections.md) が項目ごとに直す（この段の対象にしない。A34 より先にこの段が入るなら、A34 の各項目の安全なプログラムと危険な双子を、この段の受け入れのテストに含める）。
4. `expand_shares` の連なりの展開を、ブロックの持ち主の組ごとに 1 度にする（検査の時間）。

### 関門: 敵対的なレビューを続ける

A31 では、レビューの回ごとに新しい穴が見つかりました。このチケットは、段ごとに次の関門を通ってから次の段へ進みます。

- 反例の探索と適合の確認の 2 つのレビューを、新しい穴が見つからない回まで続ける（A31 と同じく、`main` との相対の健全さ（`main` が拒否する危険なプログラムを受理しない）と、退行の無いこと（`main` が受理する安全なプログラムを、文書に書いた制限のほかは拒否しない）を確かめる）。
- A31 のマージ前の確認の道具（`target/wt/a31/target/scratch/a31/gate/`: すべてのレビューのコーパスを、`main` と対応後のバイナリで両方の edition で検査し、`main` が拒否して対応後が受理するものを native・ASan で実行する）を、段ごとに回す。

## テスト計画

### Rust テスト

- `tests/rc.rs`（と、必要なら `tests/mutable_slices.rs`）に、「未修正の穴」の各行の拒否（両方の edition で code・kind・位置）と、「保守的な制限」の各行の受理（`accepts_in_both_editions`）を足す。A31 の `rejects_at`・`accepts_in_both_editions` と同じ形で、プログラムはレビューのディレクトリから写す。
- 段 1 は、テストを足さずに A31 の全テストが同じ結果であること（T1）。

### E2E

- `in_place_writes`（`tests/features.mjs`、native と WASM の `-O0`・`-O3`）に、受理に変える制限の形を、値を手で数えた export として足す（`Recorder` が解放の時に読んだ値を `Arc` へ記録する、A31 の fixture の形）。

### コーパス

- A31 のレビューと確認のすべてのプログラム（`target/wt/a31/target/scratch/review-a31-*`、`a31/`、`audit-A31`、A32 のレビュー。重複を除いて約 1 万個）を、段ごとに `main` と比べる。`main` が拒否して対応後が受理するものは、手で求めた値と native・ASan の実行で安全を確かめる。

### 性能

- `gen_chain.py cellshares` の n = 400・800・1600 の所有権の検査の時間（`TSUZURI_TIME_PASSES=1`）を記録する。CI に速度の合否は入れない。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/drop.md`（「解放の順序（LIFO と宣言順）」の表と各段落）と `lifetimes.md`（「現在の制限」）の、直した制限と既知の不具合の記述を、実際の振る舞いに置き換える。
- `docs/language.md`・`docs/architecture.md` の A31 の段落。
- `_features/README.md` の状態。

## 受け入れ条件

- [ ] 「未修正の穴」のプログラムを両方の edition で拒否する。
- [ ] 「保守的な制限」のプログラムを受理し（受理できないものは理由を文書に書く）、値が `main` と同じ。
- [ ] A31 のテストと E2E の期待が変わらない（変えるものは記録した後）。
- [ ] コーパスで、`main` が拒否して対応後が受理する危険なプログラムが無い。
- [ ] 敵対的なレビューの最後の回が新しい穴を見つけない。
- [ ] T2 が成功する。

## 落とし穴

- 形ごとの規則を、モデルに置き換えずに足すと、A31 と同じく境目に新しい穴ができる。段 1 で今の規則をモデルで表してから、規則を変える。
- 置き換えやムーブで借用を終わらせる判定は、借用の写し（呼び出し先の `path_copy`・`shared_copy`、セルの中身の写し、共有のブロック、`Drop.drop` の書き込み）がどこへ行ったかを知らないと緩めすぎになる（第12回の指摘 3・5）。
- `Rc.share` の持ち主の記録は、持ち主が書き換えられうる所（`ref mut` の借用、代入）で古くなる（第12回の指摘 2）。
- 参照が複数の値を指しうるときの自己の借用は、値ごとに分けないと値どうしの借用になる（第12回の指摘 1、A31 の `referent_pieces`）。

## 対象外

- A28 の見えないコードの除外（本体の見えない関数値・dyn のメソッド・コールバックを実行する組み込み関数へ借用を持ちうる値を渡す本体は事実も読み通しも使わない）そのものを外すこと。関数の型が環境を表さない限り、A31 と同じく残す。
- 言語の意味（破棄の順序、`Drop.drop` の規則）を変えること。
