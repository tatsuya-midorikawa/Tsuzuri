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

A31 の Copilot のレビューの後のレビューの第2回（2026-11-14）の確かめで、格納の検査の外にある次の穴も見つけました。型の検査（捕捉の規則）の穴で、このチケットの格納と破棄の順序のモデルとは別に直す必要があります。

| 名前（再現のパス） | 形 | 分かっていること |
| --- | --- | --- |
| `owned2`（`target/wt/a31/target/scratch/a31/rr2/d/d6_owned_call_moves_bound`、`d4s_owned_call_holder_dropped_first`） | 2 つの仮引数の名前付き関数を `Owned.function` で包み（`let w = Owned.function wr`、`wr :: own Holder -> (unit -> i64) -> i64`）、`Owned.call (ref w) (Holder { ... })` で、コピーできない値（`Mutex` を持ち `Drop` を持つ `Holder`）を束縛した部分適用を作り、それを 2 回呼ぶ（または 1 回呼んで捨てる） | A31 の先端・`main`・基準のすべてが受理し、どの呼び出しも束縛した `Holder` をムーブして `wr` が解放するので、2 回目の呼び出しと部分適用の破棄が解放した値を使います（native で trap、ASan で heap use after free）。関数値の呼び出しが関数を返す形（`g (Holder { ... })`、`g` は仮引数）は `E1005`（`capture_sync_primitive_in_function_value`）で拒否します（`polymorph::captures` は、引数が仮引数の数に足りない呼び出しの引数に `Capture` を求めます）が、`Owned.call` は組み込み関数の完全適用なので求めません。`Owned.function` はラムダには 1 つの仮引数を求めます（`E1006` の `owned_function_parameter_count`）が、名前付き関数とその部分適用には求めません。`Owned.call` の結果が関数のときに引数へ `Capture` を求める（パイプライン `x \|> Owned.call (ref w)` も）か、`Owned.function` に 2 つ以上の引数を取る関数値を渡すことを拒否する。`Arc` を束縛する形（`d7_owned_call_arc_moves`）は呼び出しごとに複製するので安全です（`57`） |

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
| `fvlam`（`target/wt/a31/target/scratch/a31/rr2/w2/l_applypl`・`l_applyyl`、edition 2026） | 引数で受け取った関数値を捕捉して呼ぶラムダ（`fn applypl x f = applyp x (\d -> f d)`）を、部分の借用を関数値へ渡す関数（`applyp x f = { ref x.a \|> f; 0 }`）へ渡し、`applypl (ref x) (crossd (ref p2))` と呼ぶ。edition 2027 は受理する。edition 2026 では、ラムダの中の捕捉した関数値の呼び出しが呼び出し元へ高階の事実として届かず、`crossd (ref p2)` の借用が `p2` 自身へ入りうるものとして扱う（A31 の第1回のレビューの前から） | `39:5` `E1013`（`self_referential_store`） | 受理 |
| `bindref`（`target/wt/a31/target/scratch/a31/rr2/g4/c3_q_ref`・`c3_ref_q2_safe`・`c3_param_partial`・`r_partial_q`・`r_param_partial`、反例の探索が生成した `target/wt/a31/target/scratch/a31/r16-review-2-ce/g/p/` の安全なもののうち A31 が拒否するものすべて（`rr2/g-main.json`）） | 部分適用の値（`let g = cross3 q1 (ref p2)`、部分適用を `if`・`match`・ブロックで選んだ局所値とその部分適用）が、場所の借用でない引数（参照の変数 `let q1 = ref p1` の `q1`、`ref (deref q1)`、参照の仮引数 `fn via3 a b c = { let g = cross3 a b; g c; 0 }`）を束縛する。`bound_value` が関数値の値から取り出すのは場所の借用の式が作った loan だけで（指す場所を記録した局所値の分岐もあるが、この形では取り出していない）、そうした引数は関数値の値全体（束縛したすべての引数の loan）を持つので、`g (ref p3)` の `p3` の借用を `p2` へも、`cross3 (ref p1) q2` の `p1` の借用を `p2` へも格納しうるものとして扱う。値全体の借用なので、書き込む値の側に束縛すると（`a.s` を `b.h` へ書き込む `crossr` の `let g = crossr q1; g (ref p2)`）、その後の格納が `p1` のセルへ届いて自己の借用になる。すべての引数をそろえた呼び出し、名前付き関数へのパイプライン（`ref p3 \|> cross3 q1 (ref p2)`）、場所の借用の束縛（`cross3 (ref p1) (ref p2)`）は受理する（前の先端 `88885b9f` からある） | `25:5` `E1013`（`assignment_extends_borrow`。`crossr` は `self_referential_store`） | 受理（`drop 16`・`drop 0`・`drop 0`・`1`、`drop 8`・`drop 0`・`1`） |
| `r06`・`r11`（`target/wt/a31/target/scratch/review-a31-conf-r12/p/r06_rs_good_same_scope_reassign`・`r11_rs_good_same_scope_move`） | `Rc.share (ref rc)` の持ち主を通して `rc` のブロックへ自分の部分の借用を書き込み、持ち主がスコープにある間に `rc` を代入・ムーブする。持ち主が借用を持ち続ける | `105:5`・`105:17` `E1014` | 受理 |
| `r07`・`r10`（同じディレクトリの `r07_rs_good_inner_block_move`・`r10_direct_self_move_rc`） | ブロックが自分の部分の借用を持つ `Rc` のムーブ（`main` も `Rc.get` の借用が生きている間のムーブは `E1014`。ブロックは動かないので実際には安全） | `103:17`・`104:17` `E1014` | 受理 |
| `m_named_owned`（`target/wt/a31/target/scratch/a31/L_m_named_owned`） | 所有の値だけを書く名前付き関数をロックのコールバックにする。ロックの組み込み関数はコールバックを実行する見えないコードなので、A28 の除外が事実を使わない | `13:5` `E1013` | 同じく拒否（A30 の誤った拒否） |

A32 の 5 度目の merge（2026-11-13）が見つけた、名前付き関数の直接の呼び出し `cross (ref p2) (ref p1)`（`cross a b` が `b.s` の借用を `a.h` の `Mutex` へ書き込む。`target/wt/a32/target/scratch/a32/merge5/s01/red/p/loc_direct`）の文書に無い拒否は、A31 のマージ前の追加の修正（`StoreFacts::kept_paths`。[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「マージ前の追加の修正」）で直しました。その修正のレビュー（2026-11-13、`target/wt/a31/target/scratch/a31/r14-review/`）が、修正が `loan` の場所をその借用した部分とみなしたこと（呼び出しの結果を通した借用 `let q = child b; ref q.s` を `b.s` の借用とした）による退行と、部分適用・関数値へ渡す部分の綴りの残りの拒否を見つけ、A31 で直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「マージ前の追加の修正の見直し」）。指す場所が分からない参照を通した綴り（上の `crossw`・`callres`・`mutref`）、5 つ以上の部分（`cross5`）と `Array.mapr`（`mapr`）が残ります。書き込むセルを自分で所有する関数値があるプログラムでは、部分適用が束縛した参照の引数への格納も隠れた環境へ入るものとして扱うので（上の `fx3` と同じ規則）、`let g = cross3 (ref p3) (ref p1); g (ref p2)` も拒否されます（`tests/fixtures/in_place_writes` のモジュール。パイプラインと名前付きの関数値は受理します）。

A32 の 6 度目の merge（2026-11-14）が見つけた、名前付き関数を束縛した局所値（`let chosen = cross`、名前付き関数を選ぶ `if`）のパイプライン `ref p1 |> chosen (ref p2)` と部分適用 `let g = chosen (ref p2); g (ref p1)` の文書に無い拒否（`target/wt/a32/target/scratch/a32/merge6/fv/`、`fvpart`）は、A31 で直しました（[チケット](_completed/A31-in-place-writes-keep-stored-borrows.md)の「Copilot のレビューへの対応（第1回）と fvpart」）。その調べで見つけた、関数値の仮引数を本体の中で部分適用する形（上の `fvparam`）が残ります。同じ調べで、セルを自分で所有する関数値（`mk a = \k f -> ...` が `Arc<Mutex<...>>` を値で捕捉して返すラムダ）を一部の引数に当てはめた値（`let mut w2 = w 0`）を通して局所値の借用をセルへ書き込む穴（`main` も受理し、解放した値を読む。`target/wt/a31/target/scratch/a31/cp1/reg/`）も見つけ、A31 で直しました（`sink_flags` で、コードを実行する前に 2 つ以上の引数を取る関数値を作る所が、その部分適用の型（残りの仮引数の型、`partial_types`）も同じく数える。最初の修正は関数値の呼び出しの結果に当てはめた関数値を持たせる形で、`Owned.call (ref w) 0`・`new [T](1, w)`・`Array.init 1 w`・`FixedArray.init w` を通した同じ穴が残り、`Array.foldr` の状態の型を所有する関数値の型にして `main` が受理する安全な形を拒否していたので、Copilot のレビューの後のレビューの第1回で今の形にしました。`target/wt/a31/target/scratch/a31/rr1/h/`）。同じ回に、関数値の仮引数へ渡す値の場所を短くした記録（上の `coarse`）を、その値が指す場所として扱っていた先端からの退行（`main` も受理する危険な形。`target/wt/a31/target/scratch/a31/r16-review-1-ce/p/pm_move`・`tp_move`）と、`match`・ブロック・部分適用の選択を束縛した局所値の文書に無い拒否（`target/wt/a31/target/scratch/a31/rr1/k/`）も直しました。その後のレビューの第2回で、名前付き関数を `Owned.function` で包んだ値の `Owned.call (ref w) cell`（とそのパイプライン、`Owned.function` の引数を別名や `wr3 0` にした形）、`List.mapr wr (ref cells)`、`List.foldr fold zero (ref cells)` のように、組み込み関数が受け取った関数値を少ない引数に当てはめて作る部分適用を通した同じ穴（`main` も受理し、解放した値を読む。`target/wt/a31/target/scratch/a31/r16-review-2-ce/n/`、`rr2/n/r/`）を直し（`builtin_partial_applications`）、部分適用の型を数えるのをモジュールが作りうる部分適用の型だけにして、その形の部分適用を作らないプログラムで無関係な同じ型の関数値を拒否していた前の先端からの退行（`r16-review-2-ce/c/c4_arr`・`c5_arr27`、`rr2/c/`）を直し（`partial_results`）、仮引数をそのまま渡す関数を通した `coarse` の呼び出しで呼び出し元が値全体を借用していた前の先端からの退行（`r16-review-2-conf/p/w_*`、`rr2/w/`）を直しました（`holding_loans`）。同じ確かめで見つけた、格納の検査の外にある `Owned.call` の穴（上の `owned2`）と、edition 2026 のラムダで包んだ関数値の仮引数の拒否（上の `fvlam`）が残ります。反例の探索が生成したプログラム（`r16-review-2-ce/g`）の検査で、部分適用が場所の借用でない引数を束縛する形の拒否（上の `bindref`。前の先端からあり、`main` は受理します）も見つけ、文書に書きました。

起票の依頼が挙げた `N_closed_cell_read`（`target/wt/a31/target/scratch/a31/N_closed_cell_read`）は、A31 の先端で両方の edition とも受理され、値は `main` と同じです（A31 の閉じた関数の取り消しの後も closed のまま）。制限ではありません。

### 検査の時間

`Rc.share` の持ち主の連なり（`let s_i = Rc.share (ref s_{i-1})` を n 個、最後の持ち主のフィールドの借用を最初の持ち主のセルへ書き込む、ブロックの型が `Drop` を持つ）の所有権の検査は、A31 の第12回の対応の後も n の 2 乗で伸びます（`target/wt/a31/target/scratch/review-a31-ce-r12/_tools/gen_chain.py cellshares n`、開発用のビルド: n = 400 で 121 ms、800 で 501 ms、1600 で 2009 ms。`main` は 800 で 86 ms）。第12回の対応で 3 乗（800 で 5009 ms）から下げました。束縛ごとの破棄の hook が持ち主の連なりを展開する（`expand_shares`）ためで、現実のプログラムの形ではありません。

## 再現

上の 2 つの表の各行のディレクトリに `Main.tz` があります。A31 のワークツリーで:

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
- 「保守的な制限」の表のプログラムは、両方の edition で受理し、`main` と同じ値を出す（`m_named_owned` は `main` も拒否するが、安全なので受理する）。受理できない形が残るなら、その理由を文書に書く。
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
3. 要素の添字（`slice_self_safe_elem`）、2 つの仮引数の部分（`h01`）、参照の先のフィールドを通した部分（`crossw`）、指す場所が分からない呼び出しの結果と可変の参照の変数（`callres`・`mutref`）、5 つ以上の部分（`cross5`）、`Array.mapr`（`mapr`）、パイプラインの共有（`g08`）、隠れた環境（`fx3`）、ロックのコールバックの名前付き関数（`m_named_owned`）、関数値の仮引数の部分適用（`fvparam`）、短くした場所（`coarse`）、edition 2026 のラムダで包んだ関数値の仮引数（`fvlam`）、部分適用が束縛した場所の借用でない引数（`bindref`）を、表が区別できる範囲で直す。`owned2` は型の検査（`polymorph::captures`）で直す。
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
