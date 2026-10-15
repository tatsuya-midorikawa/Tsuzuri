# A17: 暗黙の複製の除去（最終使用の move と、所有する scrutinee の match 束縛の move）

| 項目 | 内容 |
| --- | --- |
| ID | A17 |
| 優先度 | P1 |
| 規模 | L（Phase 1 は M、Phase 2 は M） |
| 依存 | A15（done。暗黙の複製の一覧 `copies::sites` と警告 `W1006`）、B07（done。Drop 型は Copy でない） |
| 後続 | A18（引数規約。A17 の Phase 1・2 の後に着手する）。PM07 の Phase 2「所有権検査の `live` 集合を使う最後の使用」を、本チケットが置き換える |
| 状態 | Phase 1・2 実装済み、統合検証中（親作業と共有ツリーで検証。チケット移動・コミットは行わない） |
| 起票 | 2026-10-13（人間の依頼「『次の一手』を進めて、開発チケットを作成して」。HEAD `2282fd96` で確認。棚卸しと試作は同じ日に行った） |
| 承認 | Phase 1・2 とも人間が実装を指示済み。言語の意味・構文・診断コードは変えない |
| 改善する劣位 | Rust 比: Copy のコストモデルの違い。Tsuzuri は全要素が Copy の配列・リストを Copy とみなし、暗黙の複製が長さに比例する確保とコピーを伴う（[なぜ Tsuzuri なのか](../_tsuzuri/language-reference/languages/why-tsuzuri.md)、A15） |
| 手本にする既存実装 | 最後の使用の判定: `src/call_specialization.rs` の `single_use_locals`。複製か移動かの分岐: `src/llvm.rs` の `FunctionEmitter::clones_on_take`・`read_place`（先頭の `emit` が `single_use` を作る）。複製の一覧: `src/copies.rs` の `sites`、`src/ownership.rs` の `Checker::note_copy`・`read_places`、`src/llvm.rs` の `check_copy_inventory`（debug build の照合）。match の束縛: `src/llvm_control.rs` の `match_expression`、`src/check.rs` の `TypedMatchArm::views`。IR の確保の検査: `tests/call_specialization.rs` の `only_unobserved_single_use_copies_are_transferred`（`body` ヘルパー）、`tests/copy_cost.rs`。確保回数の E2E: `tests/primitives.mjs` の `storage` 表。suite の書き方: `tests/features.mjs`（GUIDE §7.4） |
| 主な影響ファイル | 新規: `src/last_use.rs`, `tests/last_use.rs`, `tests/last_use_fuzz.mjs`（差分テスト。手本 `tests/local_recursion_fuzz.mjs`）。変更: `src/llvm.rs`（`last_use` のモジュール宣言、`FunctionEmitter` の欄、`emit`、`clones_on_take`）, `src/llvm_control.rs`（`match_expression`）, `src/copies.rs`（`sites`）, `tests/copy_cost.rs`（Phase 2 の既存テスト 1 件）, `tests/features.mjs`・`tests/fixtures/last_use/Main.tz`（起票時に追加済み。`inspect` を足す）, `tests/primitives.mjs`・`tests/fixtures/storage/Storage.tz`（行の追加は任意）, `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `README.md`（検証コマンドの一覧に差分テストを足す）, `_tsuzuri/language-reference/ownership-and-memory/ownership.md`・`built-in-types-and-modules/array.md`、`_perfs/PM07-copy-elision.md`。測定の道具: `tests/copy_inventory.rs`（起票時に追加済み。変更しない） |
| 計測対象 | 確保の回数（IR の `call ptr @tz.alloc` の数と、E2E の `allocations`）、`W1006` の位置の数（`copy_inventory`）、下の 3 つの再現プログラムの実行時間 |

## 目的

Copy の配列・リストと、それを含む値は、読むたびに長さに比例する確保とコピーを伴う。値をもう読まないと分かる位置では、複製せずに移動する。
今のコンパイラは「関数の本体でその局所が 1 か所にしか現れない」ときだけ移動する。次の 3 つの形は、利用者が書く自然なコードなのに複製になる。

1. 関数に 1 つでも `for`・`while` があると、それより前に数えたすべての局所の使用回数が 2 以上になり、移動されない（`single_use_locals` の規則）。
   ループで再束縛する `xs = Array.set xs i v` は、1 回の更新ごとに配列全体を複製する。
2. 分岐の両方で最後に読む形（`if base then acc else go (i + 1) (Array.set acc ..)`）は、出現が 2 か所なので複製になる。再帰で配列を持ち回る処理が 1 段ごとに複製する。
3. `match` の束縛は、Copy の値を常に複製する。`match read_file path with | Ok bytes -> ...` は、読んだバッファ全体を 1 回余分に複製する。

意味を変えずに、生成だけを正確にする。最後の使用は、所有権検査の結果ではなく、生成が持つ型付き IR の逆向きの生存解析（liveness）で判定する。
複製が減る位置は `W1006` の報告からも外し、「警告が出る位置は、実際に複製する位置」という A15 の約束を保つ。

Phase 1 は局所変数の全体の読みの最後の使用、Phase 2 は所有する scrutinee の match 束縛の move に限る。
実装者は Phase 1 だけを実装し、Phase 2 は人間が求めた場合だけ着手する。Phase 3（`Field` の連鎖、Copy の要素、閉包の環境など）は設計の方針だけを書く。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準コマンド（`cargo build --release --locked`、`cargo test --locked`）が成功する状態から始める。
- A15 が done であること。確認: `ls src/copies.rs tests/copy_cost.rs tests/copy_inventory.rs tests/fixtures/last_use/Main.tz`（4 つとも存在する）。
- 実装手順 1 のとおり、変更前のコンパイラ・棚卸し・計測値を `/tmp/tz-A17/` に残していること。
- PM07 の Phase 1（`Field` の連鎖の移動 R2 と文字列の連結 S1）とは独立している。どちらが先でもよいが、同時に着手しない。
  両方が `clones_on_take` を変えるので、後から着手する側が先に入った変更の上で `clones_on_take` の形を読み替える。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 解析が所有権検査の結果（`live` 集合・loan・`State`）を必要とすると分かった。D1 により、解析は型付き IR だけから作る。
- 手順 3 以降の確認で `tests/features.mjs last_use` の結果が 1 件でも変わった、または native と WASM、`-O0` と `-O3` で結果が違う。
  移動してよい証明が間違っている。上限を上げたり、テストを外したりして通さない。
- `live != 0`、二重解放、`tracked_free` の magic の `assert`、segmentation fault が出た。
- debug build の `check_copy_inventory`（A15 D5）が失敗した、または `W1006` が「移動する位置」に出た。解析結果を `sites` と生成の両方が同じ関数で読んでいない。
- `FunctionEmitter::call` の分岐を読んでも、引数が左から右に 1 回だけ評価されると確かめられない（手順 3）。その呼び出しは不透明にして報告する。
- 「既存テストへの影響」に挙げた以外の既存の期待値（IR、確保回数、診断）を変える必要が出た。
- IR が非決定的になった。または、複製が 1 つもないプログラムの IR が、手順 1 で保存した変更前の IR と 1 byte でも違う（手順 2 の確認）。
- `unsafe`、新しい crate、既定の WASM import、新しいランタイム記号が必要になった。
- 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状と計測（HEAD `2282fd96`）

### 複製と移動の経路

- `src/llvm.rs` の `FunctionEmitter::read_place(expression, take, relocate)` は、`take` かつ `needs_drop` の値について `clones_on_take(expression)` を見る。
  true なら `clone_value`（Copy の深い複製）、false なら読んだ slot に `store <ty> zeroinitializer` を書いて移動する。`relocate` なら frame から heap へ移す。
  移動済みの slot は零で、scope の終わりの drop は何もしない（既存の不変条件。`free(NULL)` と同じ扱い）。
- `FunctionEmitter::clones_on_take` は、`TypedExprKind::Local(id)` で `id ∈ self.single_use` かつ `id ∉ self.borrowed_locals` のときだけ「最後の使用」とし、
  `is_copy && !last_use` を返す。`Field` や `Index` の読みは、根が単一使用でも常に複製する。
- `self.single_use` は `FunctionEmitter::emit` の中で `call_specialization::single_use_locals(&self.function.body)` から作る。この関数は `TypedExprKind::Local` の出現を数え、
  `While`・`ForRange`・`ForEach` の子を数え終えるたびに、**それまでに数えたすべての局所の回数を 2 以上にする**。
  ループの外で先に使った局所も複製になるのは、この規則のためである。
- `self.borrowed_locals` には、所有する捕捉を借用で読む関数（B07 の `owned_captures`）の捕捉、特殊化した worker の借用引数、match の scrutinee と束縛の別名が入る。
- 所有権検査（`src/ownership.rs`）は `Checker::note_copy` で、Copy かつ `needs_drop` の値を `Use::Consume` で読む位置を `CopyRead { span, ty, kind, local }` として記録する。
  `copies::sites` はその一覧から、`local` が `single_use_locals` にあるもの（と B07 の借用捕捉）を除く。
  debug build の `check_copy_inventory`（`src/llvm.rs`）は「生成した複製がすべて `sites` にある」ことを `assert!` する（A15 D5）。逆向き（`sites` にあるが生成は複製しない）は検査されない。
- `FunctionEmitter::match_expression`（`src/llvm_control.rs`）は、scrutinee が place（`TypedExpr::is_place`）ならその場所を、そうでなければ値を一時の局所に置き、
  束縛を scrutinee の部分への別名にする。arm の本体の前で、束縛ごとに `Local(binding.id)` を `self.expression` で読み、`bind_local` で新しい局所にする。
  非 Copy の束縛はこの読みで移動する（部分 move）。**Copy の束縛は `clones_on_take` が複製を返す**。`TypedMatchArm::views` の注記が理由を書いている:
  「Copy の値は、後で一致した記憶域へ書かれても独立したままにするため、複製する」。scrutinee が一時値なら、後から書く者はいない。

### 計測済みの事実（2026-10-13、Apple M1 Max、`-O3`、HEAD の release コンパイラ）

下の 3 つの再現プログラムは、複製の有無だけが違う試作（使い捨て。付録 A）と HEAD で、結果が同じであることを確かめたうえで時間を測った。時間は `/usr/bin/time -p` の real、3 回。

| 形 | HEAD | 試作（移動を許した形） | 内容 |
| --- | --- | --- | --- |
| `loop`: `for` の中で `xs = Array.set xs (i % 1000000) i`（100 万要素の `[i64]`、2 万回） | 4.04 / 4.01 / 3.93 秒 | 0.02 / 0.02 / 0.02 秒 | 1 回の更新ごとに 8 MB を複製（約 0.2 ms） |
| `rec`: `let rec go = \i xs -> if i == n then xs else go (i + 1) (Array.set xs (i % 1000000) i)`（同じ 2 万回） | 4.27 / 4.02 / 3.98 秒 | 0.02 / 0.02 / 0.01 秒 | `xs` が then と else の両方に現れる |
| `match`: `match make 1000000 with \| Result.Ok xs -> go (i + 1) (acc + xs[(i * 7919) % 1000000])`（400 回） | 0.16 秒 | 0.06 秒 | `Result.Ok xs` の束縛が配列全体を複製する。`make` の確保と初期化が残りの時間 |

`--warn implicit-copy` は 3 つとも、`Array.set xs ..` の `xs`、`Result.Ok xs` の `xs` に `W1006` を出す。つまり複製であることを、コンパイラ自身が認めている。
`Vec`（常に非 Copy）の `Vec.push v i` の 100 万回と、`[string]`（非 Copy の要素）の `Array.set` の 2 万回は、同じ形で 0.01〜0.02 秒である（移動なので元から in-place）。

### 棚卸し（`tests/copy_inventory.rs`、HEAD）

```sh
cargo test --locked --test copy_inventory -- --ignored --nocapture
TZ_INVENTORY_SITES=1 cargo test --locked --test copy_inventory -- --ignored --nocapture   # 位置ごとの一覧（TSV）
```

`tests/fixtures`・`benchmarks`・`examples` の 116 個のプロジェクト（`tests/fixtures/bindgen` は `E1011` で解析できず除く）を `copies::sites` にかけ、配列・リストを複製する位置を
ソース上の位置ごとに 1 回数えた。std の位置は、これらのプロジェクトが具体化した std の関数だけなので下限である。

| 起点の式 | 親 | 利用者のコード | std |
| --- | --- | ---: | ---: |
| 束縛の Payload・Field・Tail | `match`（scrutinee が place でない一時値: `Call`・`Tuple`・`List` の構築など） | 18 | 48 |
| 束縛の Payload・Field | `match`（scrutinee が局所 `Local`） | 41 | 12 |
| 束縛の Payload | `match`（scrutinee が局所以外の place: `Index`・`Dereference`） | 1 | 3 |
| Local | 関数呼び出しの引数（`Call`） | 15 | 25 |
| Local | `let b = a`（`Block.binding`） | 23 | 2 |
| Local | 閉包の捕捉（`Closure`） | 13 | 6 |
| Local | レコード・タプル・共用体・`if`・代入・ブロックの結果 | 4 | 31 |
| そのほか（`Field` の呼び出し引数・レコードの構築、`Temporary`、`Dereference`、`Element`） | | 5 | 11 |
| **合計** | | **120** | **138** |

読み方:

- 利用者のコードの 120 件のうち 117 件は `tests/fixtures`（複製を試す目的のコードを含む）で、`benchmarks` は 1 件、`examples` は 1 件である。性能を気にして書いたコードは `ref` で複製を避けている。
- 約半分（利用者 60、std 63）は match の束縛である。scrutinee が一時値（利用者 18、std 48）なのは `match Net.read s with | Ok bytes -> ..` のような形で、Phase 2 の対象になる。
  scrutinee が局所（利用者 41、std 12）のものは、その局所が match の後で使われないときだけ Phase 2 の対象になる（Phase 1 の最後の使用の判定を使う）。
- Phase 1 の対象は、関数の引数（利用者 15、std 25）、レコード・タプルなどの構築（std 31）、閉包の捕捉（利用者 13、std 6）のうち、後で読まれない位置である。
  `let b = a`（利用者 23）の多くは、複製を試すテストなので複製のまま残る。

この表は着手前の数である。各 Phase の後に同じコマンドで数え直し、減った行と減らなかった行を完了報告に書く（目標は「増えない」と「Phase の対象の行が減る」。具体的な数の合否にしない）。

### 移動してはいけない形（試作で確かめた）

試作は「ループの外の局所を含め、借用されていない局所の読みをすべて移動する」という乱暴な規則だった。次の 3 つのプログラムで HEAD と結果が変わった。
最後の使用の証明が要る理由であり、手順の負のテストにそのまま使う（`tests/fixtures/last_use/Main.tz` に同じ形の関数がある）。

| プログラム | HEAD | 乱暴な移動 |
| --- | --- | --- |
| `let xs = new [i64](4, \i -> i + 1)` / `let ys = Array.set xs 0 9` / `xs[0] + ys[0]` | 10 | segmentation fault（`xs` が零） |
| `let xs = new [i64](3, \i -> i + 1)` の後、`for i in 0 .. 2 do` の中で `let ys = Array.set xs 0 i` と `total = total + ys[0] + xs[1]` | 9 | segmentation fault |
| `let r = ref xs` の後に `let ys = Array.set xs 0 9`、`r[0] + ys[0]` | 10 | **18（結果が黙って変わる）** |

3 つ目は、借用 `r` が元の配列を指したまま、`Array.set` が同じバッファを書き換えた例である。トラップもしない。借用された局所は、どこにも移動しない（規則 X1）。

## 目標と指標

目標は CI の合否条件にしない（GUIDE §14）。意味の検査（結果の一致、確保回数の固定値、`live == 0`）だけを合否にし、時間は before と after を同じ機械で記録する。

- G1（Phase 1）: ループの中で再束縛する局所（`xs = f xs ..`）と、分岐の各腕で最後に読む局所について、複製の確保をなくす。
- G2（Phase 1）: 関数に `for`・`while` があることだけを理由にした複製をなくす（ループの外で先に使った局所を含む）。
- G3（Phase 1）: 後で読まれる値、ループの不変の値、借用された値、閉包に捕捉された後に読まれる値は、従来どおり複製する。結果は 1 つも変わらない。
- G4（Phase 2）: 一時値の scrutinee と、match の後で使われない局所の scrutinee で、Copy の束縛を複製せず移動する。
- G5: `W1006` は、生成が複製する位置だけに出る。

| 指標 | 期待（規則から計算。計測で確かめる） |
| --- | --- |
| IR の確保の数（付録 B の 7 関数、`-O0`、`call ptr @tz.alloc`） | HEAD: `branch_reads` 2、`loop_update` 3、`read_twice` 1、`invariant_loop` 1、`build_pair` 4、`match_result` 1、`match_temp` 2。**Phase 1 の後**: 0、0、1、1、0、1、2。**Phase 2 の後**: 0、0、1、1、0、0、1 |
| `tests/features.mjs last_use`（102 件） | HEAD で通る。すべての段で、結果は同じ（native/WASM × `-O0`/`-O3`、`live == 0`） |
| 3 つの再現プログラム | 結果が同じ。`loop`・`rec` の時間が大きく減る（閾値は置かない）。`match` は Phase 2 の後に減る |
| `copy_inventory` の合計 | 利用者 120 以下、std 138 以下。Phase 1 の後に「関数呼び出しの引数」「レコード・タプルなどの構築」の行が減る |

## 変えてはいけない意味

- Copy の独立性: 値を複製する代わりに移動してよいのは、元の値を後で読む式が存在しないときだけである。観測できる違いがあってはならない。
- 評価順序（左から右）、短絡評価、トラップの時点・条件。移動は同じ位置の `clone_value` を `store zeroinitializer` に置き換えるだけで、評価する式の数と順序は変わらない。
- drop の時点と回数: 対象は Copy の型だけで、Copy の型は利用者 `Drop` を持つ値を含まない（B07）。移動済みの値は零として scope の終わりに drop され、何もしない。
- 所有権・借用の規則、`E1012`・`E1013`・`E1014` などの診断、`Type::is_copy`・`Type::needs_drop` の意味、`ownership.rs` の検査は変えない。
  所有権検査の結果は使わない（D1）。
- 非 Copy の値の移動、非 Copy の束縛の部分 move は変えない。
- IR の決定性: 集合は `BTreeSet`・`BTreeMap`。結果は式の順序と集合の順序だけで決まる。native と WASM、`-O0` と `-O3` の結果は同じ。
- 診断: 新しいコード・メッセージはない。`W1006` が出る位置が減るだけで、メッセージは変えない。

## 仕様

### 用語

- 候補の読み: `TypedExprKind::Local(id)` を値として読む式で、型が `is_copy` かつ `needs_drop`（配列・リスト・それらを含む record・tuple・union など）のもの。
- 最後の使用: その読みの後に、同じ局所の値を読みうる式が、その関数の中に存在しない。後続の読みには、ループの次の反復、`break`・`continue` の行き先、分岐の合流後を含む。
- 局所が再束縛（`x = ..`）されれば、再束縛より前の値は、その時点より後に読めない。再束縛は値を殺す（kill）。
- 位置の鍵 `ReadKey`: `(span.source, span.start, span.end, local id)`。同じ鍵の読みが複数あるとき（閉包の捕捉は局所の宣言の span を使う。builder の展開でも span が重なる）は、**すべての読みが最後の使用のときだけ**その鍵を「移動する」とする。

### Phase 1: 局所変数の全体の読み

| 規則 | 内容 |
| --- | --- |
| L1 | 候補の読みが最後の使用で、局所が除外（X1〜X4）に入らず、生成時に `borrowed_locals` にないなら、`clones_on_take` は false を返す（移動する）。`single_use_locals` による既存の移動は残す（OR） |
| L2 | 解析は関数ごとに、型付き IR の逆向きの生存解析で行う（「設計」の規則表）。式の評価順は `FunctionEmitter` の生成の順で、`TypedExpr::children` の順ではない（`Assign` は値が先、`Match` は arm の中身が入る） |
| L3 | ループは固定点まで繰り返す。`break` の行き先はループの出口の生存集合、`continue` の行き先はループの先頭の生存集合 |
| L4 | 表にない式は不透明にする: その部分木に現れるすべての局所を生存に足し、その中の候補の読みは「移動しない」にする。部分木に `break`・`continue` があれば、最も内側のループの出口と先頭の集合も足す |
| L5 | 予算（訪問する式の数）を超えたら、その関数の結果を空にする（何も移動しない）。診断は出さない |
| L6 | `sites`（`W1006`）は、移動する読みを一覧から除く。生成と `sites` は同じ `LastUses` を読む |

除外する局所（移動しない）:

| 規則 | 内容 |
| --- | --- |
| X1 | `Borrow`・`BorrowOperand`・`Slice` の被演算子の根になる局所（`ref x`、`ref x[a..b]`、暗黙の借用で挿入された `Borrow`、比較の `BorrowOperand`）。根は `Field`・`Index`・`UnionPayload`・`ListTail` をたどった先の `Local`。関数の本体のどこかに 1 回でもあれば、その局所は全体で除外する |
| X2 | `ForEach` の `source` の根になる局所 |
| X3 | パターンの束縛、`Match` の局所、`ForEach` の `owner`・`local`、`ForRange` の `local` |
| X4 | 所有する捕捉を借用で読む関数（`owned_captures`）の、先頭 `capture_count` 個の引数（`copies::sites` の `lent` と同じ集合） |

X1 は意図して粗い（呼び出しの間だけの借用も除く）。借用が呼び出しの後に生き残らないと言える場合（呼び出しの結果の型が loan を持たない、など）の緩和は Phase 3 とする。

### Phase 2: 所有する scrutinee の match 束縛

`Match { local, value, arms }` の arm の束縛 `b` を、複製せず移動する。次のすべてを満たすとき:

| 規則 | 内容 |
| --- | --- |
| M1 | scrutinee `value` が place でない（`!value.is_place()`。呼び出しの結果、タプルの構築など）。または `value` が `Local(x)` で、その読みが最後の使用（Phase 1 の解析。`x` の型は Copy でなくてもよい）で、`x` が X1〜X4 に入らない |
| M2 | `b` の型が `is_copy` かつ `needs_drop`、かつ `arm.borrowed` に `b` がない（参照・要素・tail を経由しない） |
| M3 | `b` の射影が `Local(match.local)` から `Field`・`UnionPayload` だけをたどる式である |
| M4 | その arm の束縛のどの 2 つも、射影の経路が重ならない（一方が他方の接頭辞、または同じ）。`as`・`&` のパターンで束縛が重なる arm は、その arm の束縛をすべて従来どおり複製する |

束縛の読みは、guard が成功した後にだけ行う（今もそうである。guard が失敗すれば次の arm が元の値を読む）。
`scrutinee` が局所のとき、移動で零になるのはその局所の部分であり、残りの部分は局所の scope の終わりに drop される。`sites` は M1〜M4 を満たす束縛の `W1006` を除く。

### 診断と警告

新しい診断コードはない。`W1006` の対象は「生成が複製する位置」のままで、移動する位置は報告しない。`--warn implicit-copy` の有無で IR は変わらない（A15 D3）。

### native と WASM

違いは無い。ランタイム、import、`tz.alloc`／`tz.free` の契約は変えない。

## 設計

### API（`src/last_use.rs`、新規）

`src/llvm.rs` の `#[path = "call_specialization.rs"] pub(crate) mod call_specialization;` の隣に `#[path = "last_use.rs"] pub(crate) mod last_use;` を足す（`copies.rs` は `crate::llvm::call_specialization::single_use_locals` を同じ形で呼んでいる）。

```rust
use std::collections::{BTreeMap, BTreeSet};

use crate::check::{CheckedFunction, CheckedModule, Local, TypedExpr, TypedExprKind};
use crate::diagnostic::Span;

/// `(source, start, end)` of a span.
pub(crate) type SpanKey = (Option<usize>, usize, usize);
/// A read of the whole local `local`: the position of the `Local` expression and its id.
pub(crate) type ReadKey = (SpanKey, usize);

/// The reads of Copy values that own heap storage and whose value no later read can observe.
#[derive(Debug, Default)]
pub(crate) struct LastUses {
    reads: BTreeSet<ReadKey>,
    /// Phase 2: the names (spans) of the pattern bindings that move out of an owned scrutinee.
    bindings: BTreeSet<SpanKey>,
}

impl LastUses {
    pub(crate) fn analyze(function: &CheckedFunction, module: &CheckedModule) -> Self { .. }
    pub(crate) fn moves_read(&self, expression: &TypedExpr) -> bool { .. }      // Local(id) の読み
    pub(crate) fn moves_read_at(&self, span: Span, local: usize) -> bool { .. } // copies::sites 用
    pub(crate) fn moves_binding(&self, span: Span) -> bool { .. }               // Phase 2
}
```

`analyze` は `function.body` だけを見る（所有権検査の結果は使わない。D1）。型の性質は `module.types()` の `TypeContext`（`Type::is_copy(&types)`・`needs_drop(&types)`）で調べる。

### 解析の骨子

走査は「式を評価した後に生きている局所の集合 `live`」を受け取り、「式を評価する前に生きている集合」に書き換える（逆向き）。集合は局所の id の `BTreeSet<usize>`（候補でない局所も含めてすべて追う。Phase 2 が非 Copy の局所の scrutinee を使う）。

```rust
struct Analysis<'a> {
    module: &'a CheckedModule,
    excluded: BTreeSet<usize>,                // X1〜X4
    loops: Vec<LoopContext>,                  // 内側が末尾
    recording: bool,                          // 固定点の最終の 1 回だけ true
    reads: BTreeMap<ReadKey, bool>,           // 鍵ごとの「すべて最後の使用か」
    scrutinees: BTreeMap<SpanKey, bool>,      // Phase 2: scrutinee の `Local` の読み
    budget: usize,                            // 訪問できる式の数
}
struct LoopContext { exit: BTreeSet<usize>, head: BTreeSet<usize> }
```

規則表（`FunctionEmitter` の生成の順に合わせて書く。手順 3 で `llvm.rs` を読み、食い違いは不透明にして報告する）:

| 式 | 逆向きの処理 |
| --- | --- |
| `Int`・`Float`・`String`・`Bool`・`Unit`・`Function(_)`・`Static`・`Error` | 何もしない |
| `Local(id)` | 候補なら `record(expression, id, last = !live.contains(&id))`。その後 `live.insert(id)` |
| `Unary`・`Cast`・`Checked`・`Field`・`Length`・`StringLength`・`UnionTag`・`UnionPayload`・`ListTail`・`Dereference`・`Construct { payload: Some(_) }` | 子を 1 つ訪問する |
| `Binary(op, a, b)`（`And`・`Or`・`Pipe` 以外） | `b`、`a` の順に訪問する（評価は `a`、`b`） |
| `Binary(And \| Or, a, b)` | `let mut after_b = live.clone(); 訪問(b, after_b); live ∪= after_b; 訪問(a, live)`（`b` は条件付き） |
| `Index(a, b)` | `b`、`a` の順（評価は `a`、`b`。手順 3 で確かめる） |
| `Call(callee, arguments)` | 引数を右から左へ、最後に callee を訪問する（評価は callee、引数の左から右）。`callee` が `Function(_)` なら評価はない |
| `Record(fields)`・`Tuple(elements)`・`Array(elements)`・`List(elements)`・`Closure(_, captures)` | 要素を右から左へ訪問する（評価は並びの順） |
| `Assign(Local(x), value)` | `live.remove(&x)` の後に `value` を訪問する（評価は値が先で、その後に旧値を drop して書く。旧値は書いた後に読めない） |
| `Assign(place, value)`（place が `Field`・`Index`・`Dereference`） | `place` を訪問し、次に `value` を訪問する（評価は値が先、場所が後）。根の局所は読み書きされるので生存に入る |
| `Block { bindings, result }` | `result` を訪問し、続けて `bindings` を後ろから前へ: `live.remove(&local.id)` の後に `init` を訪問する |
| `If { condition, then_branch, else_branch }` | `t = live の複製 → 訪問(then)`、`e = live の複製 → 訪問(else)`、`live = t ∪ e`、`訪問(condition)` |
| `While { condition, body }` | 下の疑似コード |
| `ForRange { local, start, step, finish, body }` | `start`・`step`・`finish` を 1 回評価してからループ。固定点は `head = exit ∪ (入口(body, head) − {local.id})`。入口の集合に `finish`、`step`、`start` を逆順に訪問する |
| `ForEach { owner, local, source, body }` | `ForRange` と同じ形。`source` の根は X2 で除外し、`owner`・`local` は本体の入口で殺す |
| `Break` | `*live = loops.last().exit.clone()` |
| `Continue` | `*live = loops.last().head.clone()` |
| `Match { local, value, arms }` | 下の疑似コード |
| 上以外（`Try`・`Reraise`・`TaskRun`・`TaskParallel`・`Parallel`・`Lambda`・`NewArray`・`NewList`・`NewLiteral`・`Interpolated`・`Structural*`・`HostCall`・`RecordUpdate`・`Slice`・`Borrow`・`BorrowOperand`・`DynDispatch`・`Method`・`GenericFunction`・`CaseConstructor`・`TypeFunction` など） | 不透明（L4） |

`While` の疑似コード（`ForRange`・`ForEach` も同じ形）:

```rust
let exit = live.clone();
let outer = std::mem::replace(&mut self.recording, false);
let mut head = BTreeSet::new();
loop {
    let mut next = self.round(condition, body, &exit, &head);   // 下
    if next.is_subset(&head) { break; }
    head.extend(next.drain(..));
}
self.recording = outer;
self.round(condition, body, &exit, &head);                       // 最終の 1 回。`recording` が true なら読みを記録する
*live = head;

fn round(&mut self, condition: &TypedExpr, body: &TypedExpr,
         exit: &BTreeSet<usize>, head: &BTreeSet<usize>) -> BTreeSet<usize> {
    self.loops.push(LoopContext { exit: exit.clone(), head: head.clone() });
    let mut live = head.clone();           // 本体の後はループの先頭へ戻る
    self.visit(body, &mut live);
    live.extend(exit.iter().copied());     // 条件が偽ならループを出る
    self.visit(condition, &mut live);
    self.loops.pop();
    live
}
```

`Match` の疑似コード（評価の順は、scrutinee、arm を上から順にパターンの手順・束縛の射影・guard、本体）:

```rust
let mut entries = BTreeSet::new();                 // どの arm にも入りうる集合
let mut arm_entries = Vec::new();
for arm in arms {
    let mut live_body = live.clone();
    self.visit(&arm.body, &mut live_body);
    // guard が偽なら次の arm へ落ちる。どの arm へも落ちうるものとして、後で足す集合を `fall` とする（保守的）
    arm_entries.push((live_body, arm));
}
let fall: BTreeSet<usize> = /* 全 arm の入口の和（2 回目の反復で確定する） */;
for (live_body, arm) in &mut arm_entries {
    let mut live_arm = live_body.clone();
    live_arm.extend(fall.iter().copied());
    if let Some(guard) = &arm.guard { self.visit(guard, &mut live_arm); }
    for alternative in &arm.alternatives {          // 手順・射影の式。`PatternStep::Bind(local, _)` は local を殺す
        /* 各 alternative の steps と bindings を、評価の順の逆に訪問し、入口を和に足す */
    }
    entries.extend(live_arm);
}
*live = entries;
self.visit(value, live);                            // scrutinee。Phase 2 は `Local(x)` ならここで `scrutinees` に記録する
```

arm の入口の和 `fall` は、arm の数に対する固定点で確定させる（簡単には「入口の集合が変わらなくなるまで繰り返す」）。
`Match { local }` の `local` と束縛の局所は X3 で除外し、arm の入口で殺す。scrutinee が局所 `x` のときの束縛・パターンの手順による `x` の記憶域の読み（`match.local` の別名経由）は、
明示的な `Local(x)` の出現ではないので集合に入らない。それでよい理由: それらの読みは arm を選ぶ間と、arm の入口で束縛を作る間だけに起き、後で `x` を明示的に読む式があれば `x` は入口の和に入るからである。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 解析 | `src/last_use.rs`（新規） | `LastUses`、`Analysis` | 上のとおり |
| 宣言 | `src/llvm.rs` | モジュール宣言 | `#[path = "last_use.rs"] pub(crate) mod last_use;` |
| 生成 | `src/llvm.rs` | `FunctionEmitter` の欄、`emit` | `last_uses: LastUses` を足し、`single_use` を作る行の隣で `LastUses::analyze(self.function, self.module)` を呼ぶ |
| 生成 | `src/llvm.rs` | `FunctionEmitter::clones_on_take` | `last_use` を `matches!(.., Local(id) if (self.single_use.contains(&id) \|\| self.last_uses.moves_read(expression)) && !self.borrowed_locals.contains(&id))` にする |
| 一覧 | `src/copies.rs` | `sites` | `read.local.is_some_and(\|id\| (single.contains(&id) \|\| last_uses.moves_read_at(read.span, id)) && !lent.contains(&id))` を除く条件にする。`last_uses` は関数ごとに `LastUses::analyze` で作る |
| 生成（Phase 2） | `src/llvm_control.rs` | `match_expression` | 束縛ごとに `self.last_uses.moves_binding(binding.span)` なら、`Local(binding.id)` を読む間だけ `self.moving_bindings` に入れ、`clones_on_take` が false を返すようにする。読んだ後に外す |
| 生成（Phase 2） | `src/llvm.rs` | `FunctionEmitter::clones_on_take` | `Local(id)` が `self.moving_bindings` にあれば複製しない |
| 一覧（Phase 2） | `src/copies.rs` | `sites` | `read.local` が `None` で `kind` が `Payload`・`Field` の読みは、`last_uses.moves_binding(read.span)` なら除く |
| 変更なし | `src/llvm.rs` | `read_place`、`clone_value`、`drop_value`、`drop_scope` | 移動済みの slot は零で、drop は何もしない（既存の不変条件） |
| 変更なし | `src/ownership.rs`、`src/check.rs` | `note_copy`、`TypedMatchArm::views` | 所有権の意味は変えない（D1）。`views` の注記は Phase 2 の後、「一時値・最後の使用の scrutinee では移動する」と直す |
| 変更なし | `src/call_specialization.rs` | `single_use_locals` | 残す（D8） |

## 実装手順

各段で `cargo test --locked` を実行し、`running N tests` の N が 0 でないことを確かめる（GUIDE §3.1）。複数の段をまとめて書いてから一度にテストしない。

### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の結果（件数）を控える。
  `mkdir -p /tmp/tz-A17 && cp target/release/tsuzuri /tmp/tz-A17/tsuzuri-before`。
  `cargo test --locked --test copy_inventory -- --ignored --nocapture > /tmp/tz-A17/inventory-before.txt`。
  付録 B の 7 関数と付録 A の 3 つのプログラムを `/tmp/tz-A17/` に作り、`tsuzuri-before` で IR の確保の数と時間（3 回）を控える。
  `tests/fixtures` と `examples` の各プロジェクトを `tsuzuri-before build <dir> --emit llvm -O0 -o /tmp/tz-A17/before/<名前>.ll` で出す（手順 2 の「IR が変わらない」確認に使う。出せないプロジェクトは一覧に残す）。
- 確認: `node tests/features.mjs target/release/tsuzuri last_use` が `features: 102 cases passed`。棚卸しの合計が利用者 120、std 138。確保の数が上の指標の HEAD 列と一致する。
  **差分テスト `tests/last_use_fuzz.mjs` を作り、HEAD のコンパイラで 200 本以上のプログラムが差分 0 であることを確かめる**（「差分テスト」の節。オラクルそのものの検証であり、これが通る前にコンパイラを変えない）。

### 手順 2: 骨格と除外規則（何も移動しない）

- 変更: `src/last_use.rs`（新規）、`src/llvm.rs`、`src/copies.rs`。
- 内容: 「API」の型と `analyze` を作る。`analyze` は X1〜X4 を計算し、**空の結果を返す**（まだ何も移動しない）。
  `emit` に `last_uses` を足し、`clones_on_take` と `sites` を書き換える（結果が空なので動作は変わらない）。
  X1〜X4 を計算する関数を、本体を `children()` で全部たどって根の局所を集める形で書く（不透明な部分木の中も含めて 1 回も漏らさない）。
- 確認: `cargo test --locked` が変更前と同じ件数で通る。手順 1 で出した IR と、`tsuzuri build --emit llvm -O0` の出力が **1 byte も変わらない**（全プロジェクト）。
  `cargo test --locked --lib` の `bounds_recursive_and_flat_expression_depth` が通る。

### 手順 3: 直線の構文と `Call` の評価順の確認

- 変更: `src/last_use.rs`、`tests/last_use.rs`（新規）。
- 内容: 先に `FunctionEmitter::call`（`src/llvm.rs`）の全分岐を読み、callee と引数が左から右に 1 回だけ評価されることを確かめる
  （`comparison_call`、`Dyn.of`、`FixedArray.init`、host call、`is_identity`、`borrowed_call` を含む）。確かめられない分岐は、その呼び出しを不透明にする。
  `Index`・`Record`・`Tuple`・`Array`・`List`・`Closure`・`Construct` の評価順も `emit_place`・`expression_mode` で確かめる。
  規則表のうち、定数、`Local`、単項、`Binary`（`And`・`Or`・`Pipe` 以外）、`Cast`、`Field`、`Length`、`Index`、`Call`、`Record`、`Tuple`、`Array`、`List`、`Closure`、`Construct`、`Block` を実装する。
  `analyze` は読みを記録して `reads` を作る。記録は鍵ごとに AND をとる。
- 確認: `tests/last_use.rs` に次を足し、通す。
  - `moves_the_last_straight_line_read`: 付録 B の `read_twice` に似た形で、最後の読みだけが移動し、最初の読みは複製（IR の確保が 1）。
  - `copies_when_a_later_argument_reads_the_local`: `Array.set xs 0 (xs[1] + 1)` の第 1 引数は複製（後の引数が `xs` を読む）。
  - `copies_a_borrowed_local`: 付録のとおり `let r = ref xs` の後の `Array.set xs ..` は複製し、`r[0]` が元の値を読む（結果が 10、18 でない）。
  - `closure_capture_copies_and_the_later_read_moves`: 閉包が `xs` を捕捉した後の `Array.set xs ..` は移動し、閉包の捕捉の読みは複製する（`closure_then_use` の形）。
  - `copies_every_read_that_shares_a_position_with_a_copy`: 同じ鍵の読みの 1 つでも最後でなければ、鍵ごと複製する。
  - `analysis_is_deterministic`: 同じ入力で `LastUses` が等しい。
  - 差分テスト: `cargo build --release --locked && node tests/last_use_fuzz.mjs target/release/tsuzuri 100` が差分 0（以降の手順 4〜6 でも毎回）。

### 手順 4: `If` と `Assign`

- 変更: `src/last_use.rs`、`tests/last_use.rs`。
- 内容: `If`、`Assign(Local, _)`、`Assign(place, _)` を規則表のとおり実装する。
- 確認: `moves_the_last_read_in_each_branch`（付録 B の `branch_reads`: 確保 2 → 0）、`moves_a_self_update`（直線の `xs = Array.set xs 0 1`）、
  `copies_a_read_before_a_partial_assignment`（`xs[0] = ..` のような場所への代入の前の読みは複製）。差分テストが差分 0。

### 手順 5: ループ

- 変更: `src/last_use.rs`、`tests/last_use.rs`。
- 内容: `While`・`ForRange`・`ForEach`・`Break`・`Continue` を実装する。固定点の最終の 1 回だけ読みを記録する（`recording`）。内側のループは外側の最終の 1 回の中で、自分の固定点を作り直す。
  不透明な部分木に `Break`・`Continue` があるとき、最も内側のループの出口と先頭の集合を足す。
- 確認: `moves_a_self_update_in_a_loop`（付録 B の `loop_update`: 確保 3 → 0）、`copies_a_loop_invariant_read`（`invariant_loop`: 確保 1 のまま）、
  `copies_before_a_break_when_the_local_is_read_after_the_loop`（`tests/fixtures/last_use/Main.tz` の `break_keeps_live` と同じ形）、
  `moves_a_loop_local`（反復の中で束縛して 1 回読む局所は移動する）、`continue_keeps_the_head_live`。差分テストが差分 0（ループと `break`・`continue` を含むプログラムが十分にあること）。

### 手順 6: `Match`（Phase 1 の範囲）

- 変更: `src/last_use.rs`、`tests/last_use.rs`。
- 内容: 疑似コードのとおり `Match` を実装する（arm の本体、guard、パターンの手順と射影、arm 間の落ち込み）。scrutinee の読みを `scrutinees` に記録する（Phase 2 で使う）。
  この段では束縛は従来どおり複製のまま（`bindings` は空）。
- 確認: `guard_falls_through_keeps_the_local_live`（`classify` の形: guard が偽なら次の arm が同じ値を読む）、`moves_in_each_arm`、`match_arm_reads_after_a_binding`。
  `cargo test --locked`（全体）、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check`。差分テストが差分 0。

### 手順 7: 有効化の確認（Phase 1 の仕上げ）

- 変更: 通常はなし（ここまでで有効になっている）。
- 内容: 付録 B の IR の確保の数が、Phase 1 の後の値になっていることを確かめる。debug build の全 Rust テストで A15 D5 の照合が通ることを確かめる。
  `tests/copy_cost.rs` に「移動する位置に `W1006` が出ない」テストを足す（`branch_reads`・`loop_update` の形で `assert_warnings(.., &[])`、`read_twice` の形では 1 件）。
- 確認: `cargo test --locked`。`node tests/features.mjs target/release/tsuzuri last_use` が 102 件通る（`cargo build --release --locked` の直後）。

### 手順 8: 全体の確認と計測

- 内容: GUIDE §3 の全体（fmt・clippy・`cargo test --locked`・`node tests/e2e.mjs`・`primitives.mjs`・`tasks.mjs`・`computations.mjs`・`control.mjs`・`numeric_casts.mjs`・`examples.mjs`）と、
  `node tests/features.mjs target/release/tsuzuri`（全 suite）、`node tests/last_use_fuzz.mjs target/release/tsuzuri 300 1000`（手順 1 までの seed と重ならない seed で 300 本）、
  `sh scripts/check-runtime-includes.sh`、§3.1 の回帰テスト 4 本。
  ASan の実行（GUIDE §3）: `TSUZURI_ASAN=1 ASAN_OPTIONS=detect_stack_use_after_return=1 node tests/control.mjs target/release/tsuzuri` と `computations.mjs`。
  GUIDE §14 の手順で付録 A の 3 つのプログラムの時間を before／after で記録し、棚卸しを数え直す。IR の決定性（同じ入力で 2 回ビルドして一致）を確かめる。
- 確認: すべて通る。差が出た行は完了報告に書く。

### 手順 9: 文書（Phase 1）

「ドキュメント」の節のとおり。`node scripts/check-docs.mjs` を変えたページに対して実行する。

### Phase 2（人間が求めた場合だけ）

手順 10〜13 は Phase 2 である。手順 1〜9 が完了し、人間が Phase 2 を求めたときだけ着手する。

- 手順 10（解析）: `Match` の処理に M1〜M4 を足し、`LastUses::bindings` を作る。射影の経路は `Field(i)`・`UnionPayload(case)` の列として作り、2 つの経路の一方が他方の接頭辞なら重なりとする。
  テスト: `moves_a_payload_out_of_a_temporary`（付録 B の `match_temp`: 確保 2 → 1）、`moves_a_payload_out_of_a_last_use_local`（`match_result`: 1 → 0）、
  `copies_a_payload_when_the_local_is_used_after_the_match`（`scrutinee_local_used_after` の形）、`copies_overlapping_bindings`（`as` パターン）、
  `copies_a_payload_reached_through_an_element`（`Index`・`ListTail`）、`copies_all_bindings_of_a_guarded_arm_until_the_guard_passes`。
- 手順 11（生成）: `match_expression` で `moves_binding` なら `moving_bindings` に入れて読む。`clones_on_take` が `moving_bindings` を見る。
  `frame_value` の frame（スタックのリテラルを scrutinee にした場合）の移動は、非 Copy の束縛と同じ経路（`read_place` の `relocate`）を通ることを確かめる。
  テスト: `tests/fixtures/last_use` の `scrutinee_temp`・`tuple_scrutinee` などに、`match Maybe.Some [1, 2, 3] with | Maybe.Some xs -> xs[0]`（スタックのリテラル）と、
  再帰的な共用体（A04）の Copy の payload を足し、native/WASM × `-O0`/`-O3` で `live == 0`。
- 手順 12（一覧）: `copies::sites` から束縛の複製を除く。`tests/copy_cost.rs` の `pattern_bindings_and_loop_elements_copy_at_their_names` は、`BINDING` が「match の後で `pair` を使わない形」で、
  束縛の複製が 1 件でなくなる。**この 1 件だけ**、`BINDING` を「match の後で `pair` を読む形」に直して元の期待（1 件）を保ち、移動する形の新しいテスト（0 件）を足す。
- 手順 13（全体と文書）: 手順 8・9 と同じ。`TypedMatchArm::views` の注記を直す。

## テスト計画

### Rust テスト

- `tests/last_use.rs`（新規。手本 `tests/call_specialization.rs`）: `analyze`・`llvm::emit` で IR を出し、`body(&ir, "tz.fn.Main.<名前>")` の `call ptr @tz.alloc` の数を比べる。
  手順 3〜6 と 10 の名前付きテスト。拒否（診断）は増えないので、受理の確認と IR の確保の数が中心。決定性（同じ入力で IR が一致）を各テストで確かめる。
- `tests/copy_cost.rs`: 手順 7 の `W1006` のテスト。`inventory_covers_every_emitted_clone` の入力に、移動する形の入力を足す（debug build の照合が働く）。
- 既存テスト: Phase 2 の 1 件（手順 12）に加え、`specializations_report_one_warning_per_source_position` は最後のタプル要素の move に伴い更新する（下記 D10）。その他の変更は親作業の PM06・PR01 と区別する。

### E2E

- `tests/features.mjs` の suite `last_use`（起票時に追加済み。102 件）。`tests/fixtures/last_use/Main.tz` の 17 個の関数が、移動してよい形と複製を保つ形を両方含む。
  期待値は JavaScript の独立した計算（suite の `cases` の中）で、コンパイラの出力を写していない。実装中、**この suite が 1 件でも変わったら止める**。
  `inspect(ir)` に、`self_update_loop`・`recursive_accumulate`・`branch_last_reads` の本体の `call ptr @tz.alloc` が、`new` の 1 回だけになる検査を足す（Phase 1 の後）。
- `tests/primitives.mjs` の `storage` 表（任意）: `tests/fixtures/storage/Storage.tz` に付録 B の形の関数を足し、確保の回数を固定値で検査する（`allocations - before == count`、`live == 0`）。
  期待値は「`new` の確保の数」＋「複製が残る位置の数」で、実装前の値を手順 1 で測って控える。
- native/WASM × `-O0`/`-O3` で `live == 0`、WASM の import が空（suite の共通の検査）。

### 差分テスト（`tests/last_use_fuzz.mjs`、新規。手順 1 で作る）

手書きのテストは書いた本人の想定に沿うので、同じ盲点を共有する（GUIDE の D-46 が B09 で得た教訓）。移動してよいかの判定を間違えると結果が黙って変わる（「現状と計測」の「移動してはいけない形」の 3 つ目が、試作で結果 18 になった例）ので、
乱数で作ったプログラムを、コンパイラの出力ではなく、**値のセマンティクスを仕様どおりに実装した独立した JavaScript の評価器**と照合する。構成は `tests/local_recursion_fuzz.mjs`（B09）に倣う
（seed つきの乱数、木の生成、ソースの出力、native のビルドと実行、評価器との比較）。

```sh
node tests/last_use_fuzz.mjs [compiler] [programs] [first seed] [depth]   # 既定: target/release/tsuzuri 40 1 4
```

- 生成: 1 つの `Main.tz` に `def run :: i64 -> i64 = \n -> ..` と、`run` を各入力で呼んで値を 1 行ずつ出す `def main :: unit -> i32`（`do! IO.write_line (to_string (run 5i64))`。B09 の `source` と同じ形）を作る。
  局所は、配列 `a0..a3`（`let mut aK = new [i64](4, \i -> i + K + 1)`）、整数の累積 `t`、`Maybe<[i64]>`（`tests/fixtures/last_use/Main.tz` の `maybe_of`）、閉包、借用 `ref`。
  値はすべて小さい非負の数で、添字は `% 4`、累積は `% 1000003` で割る（折り返しと負の添字を作らない）。`while` は本文の先頭で必ずカウンタを進め、`continue`・`break` はその後に置く（無限ループを作らない）。
- 文の形（それぞれが 1 本のプログラムに複数回現れる）:
  1. 再束縛 `aK = Array.set aK <idx> <num>`（`for`・`while` の中と外）。
  2. 派生 `let dK = Array.set aJ <idx> <num>` の後に `aJ[..]`・`dK[..]` を読む（`J = K` を含む）。
  3. `if`/`else` の各腕で最後に読む形、片方の腕だけで読む形。
  4. `for i in 0 .. 3 do`（`..` は両端を含むので 4 回。評価器の範囲を合わせる。`for-in.md`）、`while j < n do`、本文の中の `if c then continue else ()`・`if c then break else ()`（下の例の形）。
  5. 閉包 `let fK = \k -> aJ[k % 4] + <num>` を作った後に `aJ` を再束縛し、後で `fK` を呼ぶ。
  6. 借用 `let rK = ref aJ` の直後に `let eK = Array.set aJ <idx> <num>` と `rK[..] + eK[..]`（`ref aJ` の後で `aJ` を再束縛しない。`E1014` になるため）。
  7. `match` の scrutinee が、呼び出しの結果（一時値）、局所で後で使わない、局所で後でも使う、タプル。guard つきの arm、`as` パターン（Phase 2 の形。Phase 1 でも生成して結果を照合する）。
     構文は `tests/fixtures/last_use/Main.tz` の `scrutinee_temp`・`scrutinee_local_last`・`scrutinee_local_used_after`・`classify`（guard）・`as_pattern_overlap`・`tuple_scrutinee` をそのまま使う（HEAD で検証済み）。
     `Result<[i64], string>` は非 Copy なので、「後で使う」形は `Maybe<[i64]>` にする。
- 評価器（オラクル）: 配列は値として読むたびに複製する（`Array.set` の第 1 引数、`let d = a`、閉包の捕捉、`Maybe.Some a`、match の束縛）。`Array.set` は新しい配列を返し、元の配列は変えない。
  `ref` は元の配列への別名（複製しない）。閉包は作った時点の値を持つ。移動・最後の使用という概念を持たない。仕様（Copy の独立性）だけに従う。
- 比較: `tsuzuri run <dir> -O0 --no-cache` と `-O3` の標準出力の各行が、評価器の値と一致することを確かめる（入力は `n ∈ {0, 1, 2, 5, 9}`）。終了コードが 0 でなければ差分として扱う。
  生成したプログラムがコンパイルできなければ、生成器の誤りとして seed を出して失敗する。
- 網羅の計数: 文の形ごとに、生成したプログラム数を標準出力に出す。0 の形があれば失敗する（生成器が形を作っていない）。
- 通過の基準: **HEAD のコンパイラで 200 本以上が差分 0**（オラクルの検証。手順 1）。各手順の後と完了時に、別の seed で 300 本以上が差分 0。差が出たら seed とプログラムを出して止める。

生成するプログラムの形の例（HEAD で `-O0`・`-O3` とも `201 106 111 140 158` を出力し、下の JavaScript の評価器の値と一致することを確認済み。上の文の形のうち 1・4・5・6 を含む。2・3・7 は生成器が足す）:

```tsuzuri
def run :: i64 -> i64 = \n ->
    let mut a0 = new [i64](4, \i -> i + 1)
    let mut a1 = new [i64](4, \i -> i + 2)
    let mut t = 0
    for i in 0 .. 3 do
        a0 = Array.set a0 (i % 4) (i * 2)
        t = t + a0[i % 4]
    let f = \k -> a1[k % 4] + 5
    a1 = Array.set a1 1 100
    let r = ref a0
    let e = Array.set a0 2 77
    let mut j = 0
    while j < n do
        j = j + 1
        if j % 3 == 0 then continue else ()
        if j > 7 then break else ()
        a1 = Array.set a1 (j % 4) (j * 2)
        t = t + a1[j % 4]
    (t + f 1 + a1[1] + r[2] + e[2] + n) % 1000003

def main :: unit -> i32 = \() ->
    do! IO.write_line (to_string (run 0i64))
    do! IO.write_line (to_string (run 1i64))
    do! IO.write_line (to_string (run 2i64))
    do! IO.write_line (to_string (run 5i64))
    do! IO.write_line (to_string (run 9i64))
    0
```

```js
// 評価器の対応（配列は読むたびに複製、閉包は作った時点の値、ref は別名）
function run(n) {
  let a0 = [1, 2, 3, 4], a1 = [2, 3, 4, 5], t = 0;
  for (let i = 0; i <= 3; i++) { a0 = a0.slice(); a0[i % 4] = i * 2; t += a0[i % 4]; }
  const snap = a1.slice(); const f = (k) => snap[k % 4] + 5;
  a1 = a1.slice(); a1[1] = 100;
  const r = a0; const e = a0.slice(); e[2] = 77;
  let j = 0;
  while (j < n) { j++; if (j % 3 === 0) continue; if (j > 7) break; a1 = a1.slice(); a1[j % 4] = j * 2; t += a1[j % 4]; }
  return (t + f(1) + a1[1] + r[2] + e[2] + n) % 1000003;
}
```

実装する評価器は、この JavaScript を手で書く代わりに、生成した木を解釈する（B09 の `evaluate` と同じ）。上の `slice` は「読むたびに複製する」という仕様の書き方の例にすぎない。

### 既存テストへの影響

- Phase 1: 既存の期待値は変わらない（確保の回数が減る行があれば、その行が A17 の対象かを確かめ、対象外の変更なら停止条件）。
  `tests/primitives.mjs` の `loop_copies(10)`（10）、`loop_moves(10)`（30）などは、ループの中で局所を再束縛・移動する形で、移動が増えると確保の数が変わりうる。
  変わった行は、それが最後の使用の移動によることを IR で示し、**行ごとに**期待値を更新する（完了報告に旧値と新値を書く）。
- Phase 2: `tests/copy_cost.rs` の 1 件（手順 12）。

### 性能

付録 A の 3 つのプログラムを GUIDE §14 の手順で before／after 測る（9 回以上、中央値・最小・最大。M1 Max 以外の機械では別に記録する）。
`docs/benchmarks.md` に日付・環境・コミット・中央値・生データの保存先を書く。`benchmarks/` の既存の suite（`control` の `array_copy` など）を、退行がないことの確認に 1 回流す。速度の閾値は CI に置かない。

## ドキュメント

機能は利用者から見える（複製が起きる位置と `W1006`）。`node scripts/check-docs.mjs <ページ>` を変えたページに対して実行する（GUIDE §8）。

- `_tsuzuri/language-reference/ownership-and-memory/ownership.md`: 「コレクションの Copy とそのコスト」「暗黙のコピーの検出と明示コピー」に、「値を後で使わない位置では、複製せず移動する」ことと、移動する位置の例（ループの再束縛、分岐の各腕、Phase 2 の match）を足す。
  「`second` へ代入したあとも `first` を使っているので、…コピーされる」の例は、複製が残る形としてそのまま使える。
- `_tsuzuri/language-reference/built-in-types-and-modules/array.md`: 「単一の所有者しか持たない配列に対しては、内部バッファがインプレースで効率的に再利用されます」を、条件を正確に書き直す
  （Copy の要素の配列では、最後の使用のときだけ。ループで再束縛する形も含む）。計算量の表の `Array.set` の備考も合わせる。`list.md` に同じ記述があれば直す。
- `docs/language.md` の「Ownership / Borrowing」の Copy とコピーコストの段落、`W1006` の説明。`docs/architecture.md` に、生成が `last_use` を使うことと、A15 D5 の照合（`sites` と生成が同じ結果を読む）。`README.md` に該当する記述があれば合わせる。
- `README.md` の検証コマンドの一覧（`tests/local_recursion_fuzz.mjs` の行の隣）と `docs/architecture.md` のテストの説明（同じく `tests/local_recursion_fuzz.mjs` を説明する段落の隣）に、
  `node tests/last_use_fuzz.mjs [compiler] [programs] [first seed] [depth]`（最後の使用の移動の差分テスト。解析・生成を変えたときに手で回す）を足す。
  場所は `grep -n "local_recursion_fuzz" README.md docs/architecture.md` で探す。GUIDE §3 の全体の一覧には B09 の差分テストがないので、足さない。
- `docs/benchmarks.md`: 付録 A の測定。`_perfs/PM07-copy-elision.md`: Phase 2 の節を、本チケットへのリンクに置き換える（起票時に注記済み）。
- 「計画中」の注記・リンクのうち、PM07 の Phase 2 を指すものが `_tsuzuri/language-reference/` に残っていれば、実際の振る舞いの説明に置き換える（`grep -rn "PM07" _tsuzuri docs README.md`）。

## 受け入れ条件

- [ ] 手順 1〜9 を実施し、各段の確認が通る。Phase 2 は人間が求めた場合だけ（求めなかったときは「未実施」と書く）。
- [ ] `tests/last_use.rs` の名前付きテストがすべて通り、付録 B の IR の確保の数が Phase 1 の後の値になる（Phase 2 は Phase 2 の後の値）。
- [ ] `node tests/features.mjs target/release/tsuzuri last_use` が全手順を通して 120 件通り、全 suite も通る（native/WASM × `-O0`/`-O3`、`live == 0`、WASM の import が空）。
- [ ] `tests/last_use_fuzz.mjs` が、HEAD のコンパイラで 200 本以上、各手順の後と完了時に別の seed で 300 本以上、差分 0 で通る。文の形の計数に 0 がない。
- [ ] 付録 A のプログラムの結果が HEAD と同じで、`loop`・`rec` の時間が before／after で記録されている。
- [ ] debug build の全 Rust テストで A15 D5 の照合が通る。移動する位置に `W1006` が出ない。複製する位置には従来どおり出る。
- [ ] 複製のないプログラムの IR が、手順 1 の IR と 1 byte も変わらない。同じ入力で IR が 2 回とも一致する。
- [ ] 棚卸しの合計が増えず、対象の行が減っている（数を完了報告に書く）。
- [ ] `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`、GUIDE §3.1 の回帰テスト、`sh scripts/check-runtime-includes.sh` が通る。
- [ ] ドキュメントを更新し、`node scripts/check-docs.mjs` が通る。`_features/README.md` の状態を `done`（Phase 1 のみなら「done（Phase 1）」）にし、チケットを `_completed/` へ移す。
- [ ] GUIDE §13.2 の書式の完了報告を、チケット末尾の `## 実装と検証（YYYY-MM-DD）` に書く。コミットは人間の指示があるまで作らない。

## 落とし穴

- **`children()` の順は評価順ではない。** `Assign(a, b)` は `children()` が `[a, b]` だが、生成は値 `b` が先で場所 `a` が後。`Match` の `children()` は arm の手順・射影・guard・本体を並べるだけで、評価の分岐を表さない。規則表を使う。
- **`xs = Array.set xs i (xs[j] + 1)` の第 1 引数は最後の使用でない。** 後の引数が `xs` を読む。逆向きに訪問すれば自然に正しくなるが、引数を左から訪問する実装にすると壊れる（`later_argument_reads_it`）。
- **`break` が間にある再束縛。** `xs = f xs (if c then break else v)` で `break` したとき、`xs` は再束縛されない。`Break` が出口の集合に置き換えるので、出口の後に `xs` を読むなら、第 1 引数の読みは最後ではない。
- **借用の別名は黙って結果を変える。** `let r = ref xs` の後で `xs` を移動して `Array.set` が書き換えると、`r` が書き換え後を読む（結果が 18）。X1 を弱めない。
- **span の重なり。** 閉包の捕捉は局所の宣言の span を使う。同じ局所を 2 つの閉包が捕捉すると同じ鍵になる。鍵ごとの AND で扱い、片方だけ移動することは決してしない。
- **`single_use_locals` を直さない。** 「ループで数を 2 以上にする」規則は粗いが、残す（D8）。新しい解析はそれを広げるだけで、OR にする。
- **`borrowed_locals` は動的。** 解析は生成の前に走るので、`borrowed_locals` を見られない。`clones_on_take` の側で `!borrowed_locals.contains(&id)` を保ち、`sites` の側は X3・X4 で同じ集合を静的に作る。ずれると A15 D5 が失敗する。
- **worker の複製。** 特殊化した worker（`@tz.specialized.*`）は借用引数を複製し、`sites` に載らない（`note_copies` が false）。この挙動に触れない。
- **スタックのリテラルの移動。** frame を持つ値の移動は `read_place` の `relocate` が heap へ移す。解析側で特別扱いしない。
- **`as`・`&` パターン（Phase 2）。** 束縛が同じ記憶域に重なる。重なった arm の束縛を 1 つでも移動すると、もう 1 つが零を読む。M4 のとおり、その arm はすべて複製のまま。
- **guard の後に移動する（Phase 2）。** guard が偽なら次の arm が元の値を読む。移動は guard が成功した後にだけ行う（今の生成の順序を保つ）。
- **非決定性。** `HashMap`・`HashSet`・ポインタのアドレスを鍵にしない。鍵は span と id。
- **深い式。** 解析の再帰は式の深さに比例する。`syntax::MAX_NESTING`（128）を超える深さでは解析を打ち切って空の結果にする（何も移動しない）。debug build の 2 MiB の stack で動くことを `bounds_recursive_and_flat_expression_depth` で確かめる。

## 対象外

- `Field` の連鎖・`Index`（Copy の要素）・`ListTail`・`Dereference` の読みの移動（PM07 の Phase 1 の R2、Phase 3 で検討）、文字列の連結の再利用（PM07 の S1）。
- 閉包の環境の複製（`CopyCost::Environment` の位置。利用者 148、std 72。別の設計が要る）。借用で捕捉する閉包は A18 の後続で扱う。
- X1 の緩和（呼び出しの間だけの借用を除外しない）、`ForEach` の `source` の最後の使用、`Try` の中の最後の使用。
- 配列・リストを Copy でなくすこと、暗黙の複製を禁止すること（言語の意味の変更。A18 の後続で、人間の承認が要る）。
- `Rc`・`Arc` による共有（C10）、PM03・PM05 の表現・アロケーターの変更。
- `--warn implicit-copy` の既定を有効にすること（A15 D3）。

## 決定事項

### D1: 解析は型付き IR だけから作る

- 決定: `LastUses` は `CheckedFunction.body` の逆向きの生存解析で作る。所有権検査の結果（`live`・loan・`State`）は使わない。
- 理由: 所有権の結果は生成へ渡らず、span で式を対応させると builder の展開で衝突しうる（PM07 D2 と同じ）。生成と `copies::sites` が同じ関数を呼べば、A15 D5 の照合が守る。
- 状態: 既定案（実装者はこの案に従う）

### D2: 鍵は span と局所の id、鍵ごとに AND

- 決定: `ReadKey = ((source, start, end), local id)`。同じ鍵の読みがすべて最後の使用のときだけ移動する。
- 理由: 閉包の捕捉と builder の展開は span が重なる。片方だけ移動すると、もう片方が零を読む。
- 状態: 既定案（実装者はこの案に従う）

### D3: 除外は粗くてよい（X1〜X4）

- 決定: 借用・スライス・`ForEach` の source の根になる局所は、関数全体で移動しない。借用の期間の精密化は Phase 3。
- 理由: 試作の 3 つ目の形（結果が黙って 18 になる）が示すとおり、借用の見落としは誤コンパイルになる。機会の損失は許す。
- 状態: 既定案（実装者はこの案に従う）

### D4: Phase 1 は局所変数の全体の読みだけ

- 決定: `Local(id)` の読み。`Field` の連鎖・`Index`・`ListTail`・`Dereference` は複製のまま。
- 理由: 部分の移動は、残りの部分の drop と借用の規則（PM07 D1・D3）を広げる。棚卸しで、局所の読みと match の束縛だけで大半が占められる。
- 状態: 既定案（実装者はこの案に従う）

### D5: ループは固定点、`break`・`continue` は行き先の集合へ

- 決定: 規則表と疑似コードのとおり。固定点の最終の 1 回だけ読みを記録する。
- 理由: 反復で生存が広がる（次の反復で読まれる）ことと、`break` の後に読まれることを、正しく扱う最小の形。
- 状態: 既定案（実装者はこの案に従う）

### D6: 表にない式は不透明

- 決定: L4 のとおり。式を足すときは、評価順を `llvm.rs` で確かめて規則表に足す（Phase 3）。
- 理由: 評価順を取り違えると誤コンパイルになる。新しい式（`TypedExprKind` の variant）が増えても、`_ =>` で黙って最後の使用にしない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 予算を超えたら何も移動しない

- 決定: 訪問する式の数の予算（既定案: 関数ごとに 200,000）を超えたら結果を空にする。診断は出さない。
- 理由: この解析は最適化で、超過を理由にプログラムを拒否してはいけない（受理するプログラムが減る）。GUIDE の D-19 は「新しい解析は上限を持ち、超えたら `E1017`」と定めるが、
  ここでは D-19 の例外として、保守的に落とす（意味は変わらず、機会を失うだけ）。例外であることを完了報告に書く。
- 状態: 既定案（実装者はこの案に従う。人間が D-19 どおりを求めたら `E1017` にする）

### D8: `single_use_locals` は残す（OR）

- 決定: `clones_on_take` と `sites` は、`single_use` と `LastUses` のどちらかで移動とする。
- 理由: 既存の動作を狭めない（確保の数が増える変更を防ぐ）。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2 の scrutinee が局所のときは、非 Copy の局所も対象

- 決定: M1 の `x` は Copy でなくてもよい。移動するのは Copy の束縛だけで、非 Copy の束縛の部分 move は従来どおり。
- 理由: 棚卸しの局所の scrutinee（利用者 41、std 12）の多くは、エラー型が非 Copy の `Result` である。
- 状態: 既定案（実装者はこの案に従う）

### D10: 同時実装時の検証と期待値の整合

- 人間の指示により、PM07 R2/S1・PM06・PB02・PR01 と共有ツリーで並行して実装する。変更前の実行コンパイラは `target/perf/phase2-baseline/tsuzuri-before`、解析・測定の記録は `target/perf/a17/` に置く。`/tmp` は使わない。空の解析だけの段階を全プロジェクトで byte 比較する手順 2 は、この統合変更では未実施である。
- `CopyRead::local` は R2 のフィールド根も持つようになった。`single_use` は根を使い、`LastUses` は `CopyKind::Local` だけを使う。Field の根の `Local` を訪れた span を最後の全体読みと取り違えない。
- 所有する match の全体束縛（射影が `Local(match.local)`、経路が空）も M3 に含まれる。束縛の一覧から除く種別は `Local`・`Field`・`Payload`、かつ `read.local == None` とする。部分の経路は `Field`・`UnionPayload` のみである。
- `dup value = (value, value)` は最初の読みを複製し、最後の読みを移動する。2 特殊化ではサイトが 4 → 2、ソース位置で重複除去した警告が 2 → 1 になる。これは最後の使用の対象そのものなので、`tests/copy_cost.rs` の期待をこの証明に合わせる。
- PR01 が `call nonnull ptr @tz.alloc` を付けるため、確保回数の検査は `nonnull` の有無を正規化する。確保が消えたと誤判定しない。
- 要素・フィールドへ直接 `=` を書く probe は現行言語で `E1012` になるため、部分更新のテストは `Array.write (ref mut a[0..]) ...` を使う。後の引数の `break` は、両腕が同じ型になる `{ break; i }` で試す。
- 元の feature suite 102 件は残し、スタックのリテラル・再帰 union と、scrutinee の明示的な guard 読みの 18 件を追加する。合計は 120 件である。
- M1〜M4 だけでは、利用者 `Drop` 型の Copy フィールド／payload を零にしてデストラクタの観測を変えてしまう。そのため射影が `has_user_drop` の値を経由する arm は移動しない。record・union の両方を回帰テストする。
- guard・パターン手順の間は subject の別名がまだ生きている。そこに明示的な subject 根の消費読みがあると、通常の局所の後方生存だけでは早すぎる move を許してしまう。guard・手順を不透明にして、その中の消費読みは複製する。ガード成功後の安全な束縛の move は引き続き行う。

### 検証の中間記録（2026-10-14）

Phase 1・2 の実装:

- `src/last_use.rs`: 具体化 IR だけの後方生存解析。分岐の和、短絡、値が先の代入、ループの固定点、`break`/`continue` の行き先、match のガード・パターン失敗の固定点。訪問予算 100,000、深さ 128 を超えれば結果全体を空にする。借用・スライス、走査元、パターン・ループの別名、所有捕捉を除外する。位置と局所 id の鍵は AND、集合は順序付きである。
- 所有する match 束縛はガード成功後にだけ move する。全 alternative の射影の経路と重なりを検査する。要素・tail・参照や `as` の重なりはコピーを残す。
- 生成と警告は同じ `LastUses` を使う。R2 のフィールド根の単一使用と既存の借用 worker の扱いは保持する。未知の式と場所への読みは不透明に扱い、型・借用・drop の意味は変えない。
- 新しい依存、unsafe、runtime 記号、WASM import は追加していない。

実測済みの確認（統合検証の残りは完了前に追記する）:

- 変更前: 元の `features.mjs last_use` 102 件が native/WASM × O0/O3 で通過。付録 B の確保回数は `2, 3, 1, 1, 4, 1, 2`。
- 差分オラクル: AST の独立 JavaScript 評価器で、配列の値の読み・捕捉・match 束縛は複製、`ref` は別名として解釈する。変更前コンパイラで 200 本（seed 1、depth 4）、入れ子の生成追加後に別の 200 本（seed 2000、depth 3）が各 O0/O3 で差分 0。全 13 形の生成件数が正である。
- `cargo test --locked --test last_use --test copy_cost`: 最終 29 + 16 件通過。付録 B の後の確保回数は `0, 0, 1, 1, 0, 0, 1`。native/WASM の生成と IR の決定性を毎回検査する。再帰 worker は呼び出し元から到達する経路だけを追い、借用用の未到達 worker を取り違えず、配列の複製がないことを検査する。
- 解析の単体テスト 3 件（予算、深さ、同じ位置の読みの AND、決定性）通過。共有 `--lib` 検証は 211 件通過、PR04 の新 runtime 名の予約一覧 1 件が一時失敗した（PR04 担当が修正済み）。
- 拡張途中の `features.mjs last_use` は release コンパイラで 114 件通過。最後の guard 根の読みを追加した 120 件は、最終の debug コンパイラで native/WASM × O0/O3 通過。値、`live == 0`、空の WASM import と 3 関数の確保回数を検査する。親作業の最終 release で全 suite を再検査する。
- `copy_inventory` は 121 プロジェクトを解析（bindgen 1 件は元から `E1011` で除外）、利用者 59 / std 60。起票時の 120 / 138 より減った。共有作業でプロジェクト数が 116 → 121 に増えたため、行別の前後は完全には matched ではない。記録: `target/perf/a17/inventory-after.txt`。
- 正式な時間比較、全 integrated Rust/E2E、全体 fmt/clippy/ドキュメント・CI wiring は親作業が担当する。並行する全 f32 bit-pattern 検査など CPU 負荷がある間は測定しない。速度の改善は未計測として扱う。

更新ページ: `ownership-and-memory/ownership.md`、`built-in-types-and-modules/array.md`。中央の README・仕様・構成・benchmark 文書は親作業が更新する。`list.md` の tail コピーの制限は残るため変更しない。

変更ファイル: `src/last_use.rs`、`src/llvm.rs` の narrow hook、`src/llvm_control.rs`、`src/copies.rs`、`src/check.rs` の views 注記、`tests/last_use.rs`、`tests/last_use_fuzz.mjs`、`tests/copy_cost.rs`、last_use fixture・suite、上記 2 ページ。本チケットの移動、コミット、push は行っていない。

## 付録

### 付録 A: 再現プログラム（HEAD で `check` と実行を確認済み）

`/tmp/tz-A17/<名前>/Main.tz` に置き、`tsuzuri build <dir> -O3 -o prog` で実行ファイルを作る。結果は `loop` と `rec` が `1`、`match` が `572808600`。

```tsuzuri
// loop
let mut xs = new [i64](1000000, \_i -> 0)
for i in 0 .. 19999 do
    xs = Array.set xs (i % 1000000) i
xs[1] + xs[0]
```

```tsuzuri
// rec
def fill :: i64 -> [i64] -> [i64] = \n values ->
    let rec go = \i xs -> if i == n then xs else go (i + 1) (Array.set xs (i % 1000000) i)
    go 0 values

let data = new [i64](1000000, \_i -> 0)
let out = fill 20000 data
out[1] + out[0]
```

```tsuzuri
// match
def make :: i64 -> Result<[i64], string> = \n -> Result.Ok (new [i64](n, \i -> i * 3))

def total :: i64 -> i64 = \rounds ->
    let rec go = \i acc ->
        if i == rounds then acc
        else
            match make 1000000 with
            | Result.Ok xs -> go (i + 1) (acc + xs[(i * 7919) % 1000000])
            | Result.Error _ -> acc
    go 0 0

total 400
```

試作（使い捨て。リポジトリに入れない）は、`clones_on_take` に「`TZ_FORCE_MOVE` があれば借用されていない局所の読みをすべて移動する」を、
`match_expression` に「`TZ_FORCE_MOVE_MATCH` があり scrutinee が place でなければ、arm の束縛を `borrowed_locals` から外してから読む」を、それぞれ数行足しただけのものだった。
上の「移動してはいけない形」の 3 つのプログラムは、同じ試作で結果が変わった（壊れた）。

### 付録 B: IR の確保の数の probe（HEAD、`tsuzuri build <dir> --emit llvm -O0`）

```tsuzuri
def branch_reads :: [i64] -> [i64]
fn branch_reads xs = if xs.length > 2 then Array.set xs 0 1 else xs

def loop_update :: [i64] -> [i64] = \xs ->
    let mut ys = xs
    for i in 0 .. 3 do
        ys = Array.set ys i i
    ys

def read_twice :: [i64] -> i64 = \xs ->
    let ys = Array.set xs 0 9
    xs[0] + ys[0]

def invariant_loop :: [i64] -> i64 = \xs ->
    let mut total = 0
    for i in 0 .. 3 do
        let ys = Array.set xs 0 i
        total = total + ys[0]
    total + xs[1]

def build_pair :: [i64] -> [i64] -> ([i64] * [i64]) = \a b ->
    let size = a.length
    if size > 1 then (a, b) else (b, a)

def match_result :: Result<[i64], string> -> i64 = \r ->
    match r with
    | Result.Ok xs -> xs[0]
    | Result.Error e -> e.length

def wrap :: [i64] -> Result<[i64], string> = \xs -> Result.Ok xs

def match_temp :: i64 -> i64 = \n ->
    match wrap (new [i64](3, \i -> i + n)) with
    | Result.Ok xs -> xs[0]
    | Result.Error _ -> 0
```

関数 `@tz.fn.Main.<名前>` の本体（`define` から次の空行まで）の `call ptr @tz.alloc` の数: HEAD は 2、3、1、1、4、1、2（`branch_reads`、`loop_update`、`read_twice`、`invariant_loop`、`build_pair`、`match_result`、`match_temp`）。
`loop_update` の 3 は、`let mut ys = xs` の `xs`（関数に `for` があるので単一使用にならない）、ループの中の `ys`、最後の `ys` の複製である。
`match_temp` の 2 は、`new` の 1 と束縛の複製の 1 である。

## 親作業に渡す中央文書の追記

`docs/language.md` の Ownership / Borrowing と W1006:

> コンパイラが後続の読み取りがないと証明できる Copy の局所値は、複製せずムーブします。ループの再束縛と分岐の各腕も対象です。借用・スライス・走査元、後の引数の読み、未対応の式や解析上限ではコピーを残します。所有する一時値、または最後の局所 scrutinee の match 束縛も、重ならない Field/UnionPayload の射影ならガード成功後にムーブします。参照・要素・tail・利用者 Drop を経由する射影と重なる束縛はコピーを残します。ムーブする位置に W1006 は出ません。

`docs/architecture.md` の生成・コピー一覧:

> `llvm::last_use::LastUses` は具体化 IR の後方生存解析を行い、所有権チェッカーの State/loan は参照しません。ループと match の失敗先は固定点、訪問は 100,000 回・深さ 128 までで、超過時は最適化しません。生成と `copies::sites` が同じ結果を読みます。全体 Local は single-use または last-use、R2 の Field 根は single-use だけです。ガードとパターン手順は subject の別名が生きているため不透明です。束縛の move はガード成功後だけで、Drop を経由しない非重複の所有射影に限ります。

README と構成の検証コマンド一覧:

```sh
node tests/last_use_fuzz.mjs [compiler] [programs] [first seed] [depth]
# 既定: target/release/tsuzuri 40 1 4。独立した値セマンティクスの AST 評価器、
# native O0/O3、入力 0/1/2/5/9、全 13 文形式の件数を照合する。
```

benchmark 文書: 時間比較は親作業が、保存済みの `target/perf/a17/{loop,rec,match}/Main.tz` と `target/perf/phase2-baseline/tsuzuri-before` を使って CPU 負荷停止後に行う。まだ速度の実測を主張しない。

## 実装と検証（2026-10-14）

- Phase 1・2 の実装は完了し、ソースを固定した。予算・除外・順序・分岐・固定点、所有する match 束縛、共有 Copy 一覧を実装した。Phase 3 の精密な借用期間、要素・tail の move は実装していない。PM07 R2 と PM06 は親作業の別の変更である。
- 最終の `cargo test --locked --test last_use --test copy_cost`: **29 + 16 件**通過。`cargo test --locked --lib llvm::last_use`: **3 件**通過。IR 確保回数は before `2/3/1/1/4/1/2`、after `0/0/1/1/0/0/1`。生成は native/WASM で決定的。
- 差分検査は before **200 + 200 本**、release after **300 本**（seed 1000、depth 4）、各 O0/O3 で差分 0。after は **600 実行**、各実行は入力 `0/1/2/5/9` の全 5 出力を独立 AST 評価器と照合する。全 13 形式の件数は正。最後の追加 hardening は guard・Drop の最適化を保守的にする変更で、追加の guard runtime suite と Drop の IR 回帰で検査した。
- before feature **102 件**、release after **114 件**、最後の debug コンパイラによる拡張 suite **120 件**通過（native/WASM × O0/O3、`live == 0`、import 空）。最後の release 全 suite は親作業の統合ゲート。
- `node scripts/check-docs.mjs` は上記 2 ページ、**23 links、20 examples、40 native O0/O3 runs** 通過。グローバル build cache の既存の巨大ディレクトリ走査で最初の CLI 検証が timeout したため、親作業で原因を確認後 `TSUZURI_CACHE_DIR=''` で再実行した。タイムアウト上限・テストの意味は変更していない。
- 最終 fixture を含む inventory after は **user 62 / std 60**（122 プロジェクト、bindgen の既存エラー 1 件を除外）。before は起票時 user 120 / std 138、116 プロジェクトなので行別の完全な matched 比較ではない。記録: `target/perf/a17/inventory-final.txt`。途中の 59 / 60 は、scrutinee を明示的に読む guard の新しい 3 コピーを追加する前の記録である。
- 同時実装による差分と追加の安全条件は D10 に記録した。空の解析だけの段階の全プロジェクト byte 比較・各細分段階の全 suite は未実施。全 integrated cargo/fmt/clippy/E2E/CI と正式な 9 回の性能比較は、人間の指示で親作業へ引き継いだ。速さの数値をまだ主張しない。
- 更新ページ・ファイルは中間記録の一覧、中央文書の正確な追記は直前の節にある。コミット・push・チケット移動なし。scratch を清掃した。

### 限定された追加検証（2026-10-14、matched inventory と release correctness）

- 親作業の限定 GO により、棚卸しと release の last-use feature／差分検査だけを実施した。A17-off のソース、copy-free IR 比較、付録 A の実行ファイル・正式な時間測定には着手していない。
- 起票時の baseline `a621fada69bf27f425bfbb012cc7d0a699721db6` を `target/perf/A17-completion/source-before/` に展開し、現在のプロジェクト入力だけを同じ snapshot に置き換えた。比較する入力は **123 ディレクトリ・172 ソース**で byte hash が一致し、stdlib **47 ファイル**と棚卸し harness も baseline と一致する。両側とも **122 プロジェクトを解析**し、同じ `tests/fixtures/bindgen` の既存 `E1011` だけを除外した。成功したプロジェクトの intersection に狭めていない。両側のビルドには専用の cargo target directory を使用した。
- matched 棚卸しは **user 150 → 62、std 138 → 60**。origin・file/line・kind・source text のキーでは **user 88 件、std 79 件削除、std 1 件追加**である。追加された `std/Net.tz:449` の `Local / Closure` は、同じ行・同じソース・同じ型の削除された `Field / Match[scrutinee=Call]` と対応する。kind を外したソース行のキーでは新規位置は **0**。これは棚卸しの分類の差分であり、その行の物理確保回数の同一性を推測していない。残った `std/Tensor.tz:40` の位置には、代表 specialization の表示型 `[i32]` → `[i64]` の差も記録した。統合変更全体の比較なので、全削除を A17 単独の効果とは主張しない。
- 入力・解析コード・harness の hash、両側の全 SITE、行別差分は `target/perf/A17-completion/{inputs.json,inventory-before.txt,inventory-after.txt,inventory-comparison.json,inventory-removed.tsv,inventory-added.tsv}` に保存した。検査終了時にも入力・stdlib・解析コードの hash 一致を確認した。
- SHA-256 **`00d5d6a434fe6a3830741adc9f86e614f065794aaa426de0152adea6160e07c9`** の release は、`node tests/features.mjs target/perf/phase2-final/tsuzuri-after last_use` で **120 件通過**（native/WASM × O0/O3、結果・`live == 0`・import 空・決定性・確保回数の検査）。ログは `target/perf/A17-completion/features-release.log`、終了は **23:01:06 +09:00** で、後述の実行ファイルの置換より前である。
- 同じパスを使った `node tests/last_use_fuzz.mjs ... 300 4000 4` は完了していない。seed **4000–4264** の 265 本では O0/O3 の出力比較が通り、seed **4265 の O3** で child status が `null` となり **23:10:58 +09:00** に停止した。この時刻に親作業が実行ファイルを SHA-256 `2ab8e9d944f4ac7be501e78b19ad954425cedc2a25d86a25b41e313555213a5c` に置き換えたため、この run は compiler identity が固定された完了検査として無効である。300 本・600 実行通過とは数えない。失敗 seed と出力は `target/perf/A17-completion/fuzz-release-300.log` に保存した。新しい content-addressed compiler での 300 本再実行は、親作業の次の GO を待つ。
- 以上の全 worker は停止済み。production source の変更、timing、A17-off control の作成は行っていない。

### content-addressed release の差分検査再実行（2026-10-14）

- 親作業の次の限定 GO で、`node tests/last_use_fuzz.mjs target/perf/phase2-final/tsuzuri-2ab8e9d944f4ac7b 300 5000 4` だけを再実行した。SHA-256 **`2ab8e9d944f4ac7be501e78b19ad954425cedc2a25d86a25b41e313555213a5c`** と harness の SHA を実行前後に確認し、どちらも一致した。
- seed **5000–5299**、depth **4** の **300 本・600 native O0/O3 実行**が差分 **0** で通過した。各実行は入力 `0/1/2/5/9` の 5 出力を独立 AST 評価器と照合した。全 13 形式の件数は正で、`update=1500`、`derived=1200`、`branch=1200`、`for/while/closure/borrow/match_temp/match_last/match_kept/match_tuple/match_guard/match_overlap=各300` だった。
- 記録は `target/perf/A17-completion/fuzz-release-300-seed5000.log`、前後の identity は `fuzz5000-identity-before.json`／`fuzz5000-identity-after.json`、全体の結果は `bounded-results.json`。seed 4000 の identity が無効になった run は履歴として残し、今回の通過に置き換えて通過と数えていない。
- 全 worker は停止した。production source の編集、feature・棚卸しの重複実行、A17-off IR control、付録 A の実行・時間測定は行っていない。IR control と付録 A は UTF ソースの最終 freeze と親作業の別の GO を待つ。

### 付録 A の再現測定と A17-off IR control（2026-10-15）

baseline compiler（SHA-256 `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`）で保存済みの `loop`／`rec`／`match` 実行ファイルを before／before2 に再利用し、最終 compiler pin `de42dba363e1fa877c0dab5bd51cbad17b60b774bed5b91399d9d996a99d9d08` で after を構築した。全て同じ凍結済み入力、`-O3 --cpu generic --no-cache`、native arm64、Apple clang 21.0.0 を使い、各 run の stdout と終了コードを検査した。

**各9回の interleaved process wall time**（Python monotonic clock で `/usr/bin/time -p` の起動から終了まで測定。プロセス起動を含む）:

| workload | before median | after median | before2 median | after vs. before / before2 |
|---|---:|---:|---:|---:|
| `loop` | 4081.345 ms | 23.059 ms | 4100.127 ms | −99.44% / −99.44% |
| `rec` | 4102.315 ms | 16.425 ms | 4063.359 ms | −99.60% / −99.60% |
| `match` | 182.430 ms | 89.654 ms | 175.515 ms | −50.86% / −48.92% |

after の `loop`／`rec` は `/usr/bin/time -p` の分解能に近く、記載値はコンパイラ単体ではなくプロセス起動込みの下限寄りの end-to-end 時間である。before2 は独立してコンパイラを再ビルドした値ではなく、before と同じ SHA-256 の実行ファイルによる2回目の測定である。すべての実行で期待する `1\n`／`1\n`／`572808600\n` を確認した。raw 81-run log の SHA-256 は `8042ca8955698ef9deff57b11a26423ab13bfc34ff28ce38bb3a1e70bd75cfa1`。

A17-off control は最終ソースの隔離コピーを release build し、`FunctionEmitter::emit` における `LastUses::analyze` だけを `LastUses::default()` に置き換えた。これにより A17 の last-use／match-binding move decisions を無効化し、既存の `single_use` 規則は維持した。production source は変更していない。on/off は同じ3入力を同じ条件で構築し、Console entry の raw LLVM IR を保存した。静的 `call @tz.alloc` site 数（off→on）は loop **8→8**、rec **14→10**、match **6→4** だった。これは生成 IR 上の呼び出し位置数であり、実行時の確保回数そのものではない。A17-on の全プログラム結果は off と同じだった。

| workload | source SHA-256 | baseline executable SHA-256 | after executable SHA-256 |
|---|---|---|---|
| loop | `7648aca325500d2b8c5c806e63bcd8182241d33a73371e26be88f6f28a476d57` | `51dac044c3408c811c4827639fa645f2a8de440233625f31221a83eddebdca40` | `3468e102614d4dd38a6c85101f8a5078d0b819c6250ce1c7cbde30fff4c8582d` |
| rec | `5773cfbe81e9130d632e6a55589bafa4f9da2c997a52916cfafab4959b68224e` | `3d4d91c6a929cb6b89a558c06bd4bab50714a267c6fd8793a085484ce5465da8` | `d56fe29ec248d6236c2e7ad359f82d2f31b5d750c2564161347d90d0403264eb` |
| match | `290c76e65f29ed2cf4daa71eacb42622f4354322d9208b9e6a38c5af3079a887` | `7ad0a8750b5909338a6a478b43a61c1e436c4878f250c5f755ea18795b08d93d` | `eacf8d9dbf739ad0e51283214822c88d2d2219983c9085c5ff0003c84269d68b` |

baseline compiler、A17-off compiler、after compiler の SHA-256 はそれぞれ `ecbee0901ee678999a612a329018cfafaddc73a75183925a1db3e5770555d0dc`、`98841d07ed320eadcfa3a80b60fdf46662877c5ef761095f0987cde12751d5e9`、`de42dba363e1fa877c0dab5bd51cbad17b60b774bed5b91399d9d996a99d9d08`。final release の `last_use` feature **120 cases**（native/WASM × O0/O3）と fuzz **300 programs／600 native O0/O3 executions、seed 4000、depth 4、差分0** も通過した。詳細な raw measurements、control manifest、compiler/executable/IR hashes は ignored `target/perf/A17-completion/` の `a17-shortfix-control.json`、`appendix-shortfix-timings.jsonl`、`appendix-shortfix-timings.json` に保存した。
