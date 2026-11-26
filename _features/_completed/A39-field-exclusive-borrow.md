# A39: レコードのフィールドの経路の排他借用（`ref mut b.f`・`ref mut b.data[a..b]`。edition 2027 以降）

| 項目 | 内容 |
| --- | --- |
| ID | A39 |
| 優先度 | P1（[NC20](NC20-compound-assignment-and-mutable-index-views.md) の Phase 2・3 の前提。std の本体の `ref mut target.data[0..]` と、利用者の `model.w += g` がこの規則で通る。単独でも、2027 のフィールドへの代入と同じ場所を `ref mut` で関数へ渡せるようになる） |
| 規模 | M |
| 依存 | –（[A13](_completed/A13-exclusive-borrow-fields.md)・[A17](_completed/A17-last-use-move.md)・[A18](_completed/A18-parameter-conventions.md)・[A31](_completed/A31-in-place-writes-keep-stored-borrows.md)・[C08](_completed/C08-mutable-slices.md) は完了済み） |
| 後続 | [NC20](NC20-compound-assignment-and-mutable-index-views.md)（Phase 2・3）。[PM10](../_perfs/PM10-std-consume-copies.md) は A39 が作る借用の根を Phase 2 で扱う（A39 は PM10 の前提ではない） |
| 状態 | done |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20) の委任による NC20 の D2 の決定。HEAD `858a949c` で確認） |
| 承認 | 承認済み（D-66 の委任で決定、2026-11-25）: std だけの特権を作らず一般の規則にすること（D1。NC20 の D2）、edition 2027 以降に限ること（D2）、対象の場所と可変性の判定（D3）、拒否のまま残す形（D4）、健全さを示せない構成の縮め方（D5）。仕様の規則 1 つ（D8）は決定（D-66 の委任で決定、2026-11-25）。D6・D7 は既定案 |
| 改善する劣位 | 表現力と性能: edition 2027 では `b.n = 5` と `b.xs[0] = 9` が書けるのに、同じ場所を `ref mut b.n` で関数へ渡すこと、`ref mut b.xs[0..]` で排他スライスにすること、`Array.write 0 9 b.xs` と書くこと、`h.g[1] = 42`（`IndexMut` の対象がフィールド）は `E1014` になる。案内（「let mut へ移して戻す」）は、Copy の配列のフィールドでは中身を複製し、`Tensor` のような Copy でないフィールドでは、すべてのフィールドを並べて持ち主を作り直す書き方になる（`{ b with f = x }` は部分 move の `E1012`）。std も、所有する `Tensor` の `data` を排他借用する本体を書けない |
| 手本にする既存実装 | `src/ownership.rs` の `access`（7230 行。edition 2027 のフィールドへの書き込み `Use::Write` を、根の可変性と経路の loan で判定し、別のフィールドとは `Place::overlaps` で重ねない）、`src/ownership.rs` の `place()` の `through_drop`（7731〜7747 行。段を足すときに入れ物の型で印を足し、参照外しの場所は loan の場所から受け継ぐ。A39 の段の記録はこの形にする）、`src/check.rs:15003`〜15011 の 2027 の射影の代入（`read_conventions && projection`） |
| 主な影響ファイル | `src/ownership.rs`（`access` の 7429 行の拒否、`Drop.drop` の判定 7388 行の隣、`Place` と `place()` の段の記録、`store_arguments` の自己参照の判定）、`src/check.rs`（関数ごとの edition の印。`CheckedFunction`（5178 行））、`src/closures.rs`・`src/closures_rec.rs`（持ち上げた関数への印の受け継ぎ）、`tests/borrowed_records.rs`、`tests/edition_2027.rs`、`tests/features.mjs` と `tests/fixtures/borrowed_records/Main.tz`、`spec/rules/OWN.md`・`tests/conformance/OWN/`、`_tsuzuri/language-reference/ownership-and-memory/borrowing.md`、`_tsuzuri/language-reference/built-in-types-and-modules/record.md`、`_tsuzuri/language-reference/compiler/conformance.md`（生成）、`docs/language.md`、`docs/stability.md`、`docs/architecture.md`、`src/explain/mutate_immutable_value.md`、`guide/card.md` |

## 実装者向け早見

- 範囲: edition 2027 以降で読むソース（std を含む）で、`let mut` の持ち主か排他参照（`ref mut`）の参照先を根とし、region の印（A13）の後の段がすべてレコードのフィールドである場所を、排他借用できるようにする（`ref mut b.f`、`ref mut b.f.g`、`ref mut r.f`、`ref mut b.data[a..b]`、主語の暗黙の排他借用、2027 の `mut T`、`IndexMut` の対象）。変えるのは `src/ownership.rs:7429` の「フィールドを経由する排他のアクセスの拒否」だけで、レコードのフィールドの段に限って外す（ほかに、呼び出しによる自己参照の格納の判定を代入の判定にそろえる。監査の表）。可変かどうかの判定（根が `let mut` か、経路の loan がすべて排他か）は変えない。
- edition 2026 は今のまま（`ref mut b.f` は `E1014`、kind `mutate_immutable_value`）。help だけを edition 2027 の案内に直す。
- 最初に読む: ① 下の「現状」の表 ② `src/ownership.rs` 7380〜7462 行（`access` の `Drop.drop` と可変性の判定）と 60〜142 行（場所の印と `Place::overlaps`）と 7706〜7800 行（`place()`。段の記録を足す所） ③ 決定事項 D1・D2（特権ではなく一般の規則にする理由と、2027 に限る理由）。
- 最初に書くテスト: `tests/borrowed_records.rs` に、edition 2027 の `ref mut b.xs[0..]` が受理されること（今は `E1014` なので失敗する）と、edition 2026 の同じソースが `E1014` のままであること（今も通る）。
- 触ってはいけない: 既存のプログラムの生成 IR（フィールドを排他借用しないプログラムは 1 バイトも変えない）、edition 2026 の受理と診断のコード・kind・位置、可変性の判定、`Place::overlaps` の意味、要素・ペイロード・リストの tail・タプルの成分の拒否、`Drop.drop` の規則、不透明なレコードの規則（`E1022`）。std だけの特権や許可表を作らない。
- 完了の見分け方: 「現状」の表の 2027 の列のうち「A39 の後」が受理のものがすべて通り、拒否のものが同じコードと kind で拒否される。2026 の列はコード・kind・位置が今と同じ。native と WASM の `-O0`・`-O3` の E2E が通り、`live == 0`。

## 目的

edition 2027 は、可変な持ち主か排他参照を根にしたフィールドと要素への代入を許した（A18。`docs/stability.md:85`）。ところが、同じフィールドを排他借用することはできない。`b.n = 5` は書けるが、`bump (ref mut b.n)` は `E1014` になる。`b.xs[0] = 9` は書けるが、`ref mut b.xs[0..]` で排他スライスにして `Array.write` で書くことも、`Array.write 0 9 b.xs` と主語で渡すこともできない。今の案内（help `write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'`）は、値を `let mut` へ移して戻すことである。要素が Copy の配列のフィールドでは、この書き方は中身を複製する。`Tensor<f64>` のような Copy でない型のフィールドは複製せずに move できるが、`let mut w = model.w` の後の `{ model with w = w }` は部分 move の `E1012` になり（HEAD で確認）、`model = Model { w: w, b: model.b }` とすべてのフィールドを並べて作り直すことになる。

NC20 は、`Tensor` の in-place の更新を std の本体で書くために、所有する `Tensor` の `data` を排他借用する必要がある。起票時の案は std だけの特権（許可表 `exclusive_field_record`）だったが、D-66 の委任による決定で、特権を作らず一般の規則にした（D1）。このチケットは、レコードのフィールドだけからなる経路の排他借用を、edition 2027 以降のすべてのソースに入れる。std も利用者のコードと同じ規則で書く。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準コマンドが成功し、「現状」を基準の dev バイナリ（`cargo build`）で確かめていること。
- 依存はない。A13・A17・A18・A31・C08 は完了済み。A38 とは独立に入れられる。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- **S1** 監査の表（設計）のどれかで健全さを示せない。まず D5 のとおり、その構成の loan を持ち主の全体に置く粗い規則へ縮める。粗い規則でも健全さを示せないときに止める。std だけの特権や許可表で回避しない。
- **S2** フィールドを排他借用しないプログラム（`std/`・`tests/fixtures/`・`examples/`）の生成 IR が、手順 1 の記録と違う。
- **S3** edition 2026 で読むソースの診断のコード・kind・位置か受理が変わる。変えてよいのは、フィールドの経路の `E1014` の help の文言だけである。
- **S4** NC20 の std の本体の形（`self: ref mut Tensor<'a>` の `ref mut self.data[0..]`、`IndexBorrowMut` の生成関数の `self`。region の印が付いた外部の場所も含む）は、段の記録（仕様 1 の 2.）で受理される。印の後の段がレコードのフィールドなのに受理できない（段がレコードのフィールドと記録されない）なら、NC20 の S3 と同じ問題なので、両方のチケットに書いて報告する。
- **S5** `Place::overlaps`（`src/ownership.rs:140`〜142）の意味を変えたくなる（別のフィールドが同じメモリを指す構成が見つかった、など）。
- **S6** GUIDE §13.1 の共通の条件。

## 現状（HEAD `858a949c` で確認）

### 拒否の場所

- 排他のアクセス（`Use::MutBorrow`）は、`access`（`src/ownership.rs:7230`）の中で判定する。可変性は、経路の loan がなければ根の局所の `mutable`、あれば経路の loan がすべて排他かで決める（7419〜7428 行）。そのうえで、`if !mutable || (usage != Use::Write && !unmarked_fields(place).is_empty())`（7429 行）が、フィールドを経由する排他のアクセスをすべて `E1014`（kind `mutate_immutable_value`）にする。メッセージは `mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable`（7432 行）。根が `let` で経路にフィールドがないときだけ related（`declared_here`）と help `declare 'x' with 'let mut', or take it by 'ref mut'` を付け（7435〜7457 行）、ほかは help `write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'`（7459 行）。
- `Use::Write`（edition 2027 のフィールドと要素への代入）は 7429 行の第 2 の条件に当たらないので、可変性の判定だけで通る。edition による切り分けは型検査の `src/check.rs:15003`〜15011（`read_conventions && projection`）にあり、所有権の検査は edition を見ない（`docs/architecture.md:1610`）。
- 場所の段（`src/ownership.rs:60`〜108）: レコードのフィールドの段はフィールドの添字（`place()` の `E::Field`、7731〜7738 行）。共用体のペイロードは `PAYLOAD`（7740〜7747 行）、リストの tail と組み込みの添字は `ELEMENT`（7749〜7754 行、7756〜7778 行）。外部の loan（仮引数）の場所は、region の slot が 2 つ以上のときだけ先頭に印 `REGION_SLOT - slot` を持ち（`external_loans`、6794〜6798 行）、`unmarked_fields`（103 行）が印を外す。タプルの成分は、パターンが作る場所でフィールドと同じ小さな添字を持つ（段の値だけでは区別できない。`place_type` は型で区別する、10577 行の `Type::Tuple`）。
- `Place::overlaps`（140〜142 行）は、根が同じで、短い方の長さまでフィールドが一致すれば重なるとする。別のフィールドは重ならず、持ち主の全体とは重なる。
- region を持つフィールド（`Tensor.Mut` の `data: ref mut {r} ['a..]` など）を経由する排他のアクセスは今も通る。参照外しの場所は参照先の place になり、経路にフィールドが残らないためである（`place()` の `E::Dereference`、7779 行付近）。

### 受理と拒否の表（検証済み。HEAD の dev バイナリ、マニフェストなしで `--edition 2026` と `--edition 2027`。`record B { n: i64, xs: [i64] }`）

| 形 | 2026 | 2027（今） | 2027（A39 の後） |
| --- | --- | --- | --- |
| `let mut b`、`b.n = 5` | `E1012`（`assign_to_immutable_place`） | 受理 | 受理（変えない） |
| `let mut b`、`b.xs[0] = 9` | `E1012` | 受理 | 受理（変えない） |
| `let mut b`、`let r = ref mut b.xs` | `E1014`（`mutate_immutable_value`。help `write through a 'let mut' variable …`） | `E1014`（同じ） | 受理 |
| `let mut b`、`ref mut b.xs[0..]` の後に `Array.write 0 9 s` | `E1014` | `E1014` | 受理 |
| `let mut b`、`Array.write 0 9 b.xs`（主語の暗黙の排他借用） | `E1014`（位置は `b.xs`） | `E1014` | 受理 |
| `let mut b`、`bump (ref mut b.n)` | `E1014` | `E1014` | 受理 |
| `let mut b`、`put b.xs`（`put :: mut [i64] -> unit`） | `E1005`（`mut_parameter_needs_edition_2027`。宣言の位置） | `E1014`（位置は `b.xs`） | 受理 |
| `r: ref mut B` の仮引数で `ref mut r.xs[0..]` | `E1014` | `E1014` | 受理 |
| 入れ子 `record O { inner: B, m: i64 }` の `ref mut o.inner.xs[0..]` | `E1014` | `E1014` | 受理 |
| `record Holder { g: Grid, n: i64 }` と `instance IndexMut<Grid, i64, i64>` の `h.g[1] = 42` | `E1014`（ほかに本文の `self.data[…] = v` の `E1012`） | `E1014`（位置は土台の `h.g`） | 受理 |
| `Mutex.with_lock (ref m) (\value -> value.n = 7)` | `E1012` | 受理（出力 `7`） | 受理（変えない） |
| `Mutex.with_lock (ref m) (\value -> bump (ref mut value.n))` | `E1014` | `E1014` | 受理 |
| `let b`（`mut` なし）、`ref mut b.n` | `E1014` | `E1014` | `E1014`（変えない） |
| `let mut arr = [B { … }]`、`ref mut arr[0].n`（要素の段） | `E1014` | `E1014` | `E1014`（変えない） |
| `r: ref B` の仮引数で `ref mut r.xs`（共有参照を経由） | `E1014`（`mutate_immutable_value`） | `E1014` | `E1014`（変えない） |
| `match u with \| U.A b -> ref mut b.n`（`let mut u`。パターンの変数） | – | `E1014`（`mutate_immutable_value`。パターンの変数は不変の view） | `E1014`（変えない） |
| `ref mut (mk ()).n`（一時値のフィールド） | – | `E1013`（`borrow_of_temporary`） | `E1013`（変えない） |
| `record P {r} { hv: View {r}, s: string }` の `let mut p` で `set_view (ref mut p.hv) (ref p.s)`（`set_view {q r} :: ref mut {q} View {r} -> ref {r} string -> unit`） | `E1014`（`mutate_immutable_value`。位置は `(ref mut p.hv)`） | `E1014`（同じ） | `E1013`（`self_referential_store`。代入 `p.hv = View { data: ref p.s }` と同じ。監査の表） |
| 閉包の中で捕捉した `b` の `b.n = 5` | – | `E1014`（`mutate_immutable_value`） | `E1014`（変えない。`ref mut b.n` も同じ） |
| `Drop.drop` の中の `ref mut self.xs[0..]` | `E1014`（`mutate_immutable_value`） | `E1014` | `E1012`（`drop_replace_whole_value`。フィールドへの代入と同じ。D4） |
| `Drop.drop` の中の `self.xs = [5]` | – | `E1012`（`drop_replace_whole_value`） | 同じ（変えない） |
| `Drop` を持つ型の値の、`Drop.drop` の外の `d.xs = [2, 3]` | – | 受理 | 受理（`ref mut d.xs` も受理） |
| std の不透明な `Tensor` の、モジュールの外の `ref mut t.data` | `E1022`（`opaque_type_representation`） | `E1022` | `E1022`（変えない） |
| `Rc` の中のフィールド `ref mut r.n`（`r: Rc<B>`） | `E1007`（`field_access_non_record`） | `E1007` | `E1007`（変えない。構文で書けない） |
| タプルの成分 `t.0` | `E0002` | `E0002` | `E0002`（変えない。構文で書けない） |

- パターンの view の loan はフィールドの場所に付く（2027 で確認）。`r: ref mut B` で `match deref r with | B { xs = xs } -> r.n = 5; xs.length` は受理され、`xs: [string]` で `r.xs = [...]` を書くと、生きている view `xs` との `E1014`（`borrow_conflict`）になる。
- 別のフィールドへの書き込みは、生きている共有の借用と重ならない（2027 で確認）。`let a = ref r.xs` の後の `r.n = 5` は受理、`let a = ref b.xs` の後の `b.n = 5` は受理で、続く `b = B { … }` は `E1014`（`borrow_conflict`）。

### edition と std

- `Edition::LATEST` は 2027（`src/syntax.rs:34`）。std のモジュールは `Edition::LATEST` で読む（`src/driver.rs:2784`・3052、`docs/stability.md:67`）。
- 型検査の `read_conventions` は、モジュールが `names.borrowing_modules` にあるか（`src/check.rs:13099`）で決まる。`borrowing_modules` は edition が `borrows_parameters()` のモジュールの集合（8303〜8307 行）で、`borrows_parameters` は今は `matches!(self, Self::E2027)`（`src/syntax.rs:43`〜45）。
- `docs/stability.md` の「edition の差として許す」4 つ（74〜77 行。72 行が前置き）の 4 つ目が A18 の引数規約で、85 行が「2027 の `mut T` … 可変な所有者か排他参照を根にした field・collection 要素への代入を許します」と書く。89 行は「明示した借用・呼び出しの評価順・数値の意味・トラップ・所有権検査の規則は変更しません」と書く。

### 借用の根と最後の使用

- `src/last_use.rs:55`〜58（A17 の規則 X1）は、`Borrow`・`BorrowOperand`・`Slice` の根の局所を、関数全体で移動の対象から外す。共有の `ref b.xs` もすでに `b` を外すので、フィールドの排他借用は新しい種類の根を作らない。A39 で新しく受理されるプログラムでは、借用した後の `b` の move が複製になる（PM10 の Phase 2 が直す）。

### 文書

- `docs/language.md:1963` は「レコードの特定フィールドのみ … をインプレースで部分破壊的に書き換えることは言語仕様上禁止されています」と書く（2027 の代入の前の文のまま）。
- `_tsuzuri/language-reference/built-in-types-and-modules/record.md` は 3 行・84〜91 行・335 行で、2027 のフィールドへの代入を書く。排他借用は書かない。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md` の「排他借用（ref mut）と排他性規則」（67 行）は、排他借用に `let mut` が要ることを書き、236 行の表は `ref mut x` の借用元を書く。332 行は要素 1 つの排他借用がないことを書く。
- `guide/card.md:86`〜88 の edition 2027 の項は、`p.x = 1` と `xs[i] = v` が `let mut` の持ち主か `mut T` の仮引数で書けることを書く。

## 変えてはいけない意味

- フィールドを排他借用しないプログラムの型・所有権・診断・生成 IR（S2）。
- edition 2026 の受理と、診断のコード・kind・位置（S3）。2026 では、レコードのフィールドへの代入（`E1012`）も排他借用（`E1014`）も今のまま拒否する。
- 可変性の判定（根が `let mut` か、経路の loan がすべて排他か）。`let` の持ち主、共有参照を経由する経路、閉包が捕捉した持ち主、パターンの変数は、今のまま可変でない。
- `Place::overlaps` の意味。別のフィールドは重ならず、持ち主の全体の読み出し・move・代入とは重なる。
- 要素・共用体のペイロード・リストの tail・タプルの成分を含む経路は排他借用できない（`E1014`）。要素単位の排他借用はない（排他スライス `ref mut xs[a..b]` と `Array.write` は今のまま）。
- `Drop.drop` の中で持ち主の全体を `ref mut` で渡す拒否（`E1012`、kind `drop_pass_exclusive_value`）と、フィールドへの代入の拒否（`E1012`、kind `drop_replace_whole_value`）。
- 不透明なレコードのフィールドに、モジュールの外から触れないこと（`E1022`）。std の型の不変条件は、今と同じくモジュールの中のコードが守る。
- region を持つフィールドを経由する再借用（`ref mut counter.value`、`Tensor.Mut` の `data`）の規則。共有参照のフィールドを経由する排他の再借用の拒否（`tests/borrowed_records.rs:926` の `ref mut mixed.name` の `E1014`）。
- 一時値の解放の時点、`Domain`・arena のフレームの値の扱い、A31 の格納の事実の意味。

## 仕様

### 1. 対象の場所（D3。OWN の新しい規則）

- edition 2027 以降で読むソース（`Edition::borrows_parameters` が真のモジュール。std を含む）で、次の 3 つを満たす場所を排他借用できる。
  1. 根が `let mut` の局所か、排他参照（`ref mut`）の参照先である。可変性は今の判定のまま決める。
  2. region の印（A13）の後のすべての段が、レコードのフィールドである。段を足すとき（`place()` の `E::Field`）に入れ物の型が `Type::Record` かを場所に記録し、参照外しの場所は loan の場所の記録を受け継ぐ。根の宣言の型からはたどらない（参照外しの場所の根と最初の段は loan の場所のもので、呼び出しの結果の参照は引数のすべての loan を持つので、根の型と段が合わない）。タプルの成分（入れ物が `Type::Tuple`）、`ELEMENT`・`PAYLOAD` の段（要素・リストの tail・共用体のペイロード）は、レコードのフィールドと認めない。`Rc`・`Arc` の中身のフィールドは書けない（`E1007`。フィールドの参照は `autoderef` の後の `Type::Record` にだけ `Field` を作る）。印の後の段がレコードのフィールドなら、region の印の付いた外部の場所（`record P {r s}` の仮引数の `p.left.n`）も受理する。2027 の `p.left.n = 5` と同じ場所である。
  3. 経路が共有参照を経由しない（経由すれば 1. の可変性の判定で落ちる）。
- 書き方: 明示の `ref mut b.f`・`ref mut b.f.g`・`ref mut r.f`（`r: ref mut B`）、排他スライス `ref mut b.data[a..b]`・`ref mut b.data[0..]`（C08）、主語の暗黙の排他借用（`Array.write 0 9 b.xs`）、2027 の `mut T` の仮引数への `b.xs`、`IndexMut` の対象（`h.g[k] = v`。対象の `Borrow(土台, true)`）。NC20 が足す複合代入の対象（`model.w += g`）と書き窓（`ref mut model.w[…]`）も、この規則で通る。
- loan はフィールドの場所に置く。`Place::overlaps` のとおり、別のフィールドの借用とは重ならず、持ち主の全体の読み出し・move・代入とは重なる。同じフィールドとその内側（`b.f` と `b.f.g`）は重なる。
- 呼び出しが、フィールドの排他借用の先へ同じ持ち主の別のフィールドの借用を格納する形（`set_view (ref mut p.hv) (ref p.s)`）は、代入 `p.hv = View { data: ref p.s }` と同じ `E1013`（`self_referential_store`）にする（監査の表の「自己参照の格納」）。
- 排他スライスは長さが固定なので（`ReplaceExclusiveSlice`、`src/check.rs:14998`）、フィールドの配列の長さは借用を通して変わらない。

### 2. 拒否のまま残す形（D4）

| 形 | 診断 |
| --- | --- |
| 要素・共用体のペイロード・リストの tail・タプルの成分を含む経路（`ref mut arr[0].n`） | `E1014`（`mutate_immutable_value`） |
| 根が `let`（`mut` なし）、閉包が捕捉した持ち主、パターンの変数 | `E1014`（`mutate_immutable_value`）。根が `let` のときは related（`declared_here`）を付ける（D7） |
| 共有参照を経由する経路（`r: ref B` の `ref mut r.xs`） | `E1014`。kind は今のまま（`mutate_immutable_value`。参照先が排他の region を持つレコードのとき、または共有参照の直接の参照外しのときは `mutate_through_shared_reference`。`shared_exclusive`、`src/ownership.rs:7866`、`src/check.rs:15683`） |
| `Drop.drop` の中の、持ち主のフィールドの排他借用 | `E1012`（`drop_replace_whole_value`。フィールドへの代入と同じ。メッセージは借用の文に分ける: D7） |
| 不透明なレコードの、モジュールの外のフィールド | `E1022`（`opaque_type_representation`。型検査のフィールドの参照で、今のまま） |
| 一時値のフィールド（`ref mut (mk ()).n`） | `E1013`（`borrow_of_temporary`。今のまま） |
| edition 2026 で読むソースの、フィールドの経路の排他借用 | `E1014`（`mutate_immutable_value`。今のまま。help だけを直す: D7） |

### 3. edition（D2）

- edition 2027 以降で読むソースだけで受理する。edition 2026 は今と同じく `E1014`。
- これは A18 の射影の代入（`docs/stability.md` の「edition の差として許す」4 つ目の承認済みの差。85 行）の借用の形で、すでに宣言した差の中で受理を増やす。新しい edition の差は足さない。
- 判定は関数を書いたモジュールの edition で行う。std は `Edition::LATEST`（2027）で読むので、std の本体は使える。生成関数（インスタンスの本体、特殊化、持ち上げたラムダ）は、作った元の関数の印（設計の `field_borrows`）を受け継ぐ。
- [NC17](NC17-edition-2028-inclusive-range-family.md) が edition 2028 を足すとき、判定は 2028 も含める（`borrows_parameters` を「2027 以降」にする変更と同じ）。

### 4. 診断（D7）

新しいコードと kind はない。

| 条件 | コード | kind | メッセージ（英語。help） | 位置 |
| --- | --- | --- | --- | --- |
| 2027 以降、レコードのフィールドの経路で、可変でない（根が `let` など） | `E1014` | `mutate_immutable_value` | `mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut')`。根が `let` なら related `'b' is declared here without 'mut'` と help `declare 'b' with 'let mut', or take it by 'ref mut'` | 借用の式 |
| 2027 以降、要素・ペイロード・tail・タプルの成分を含む経路 | `E1014` | `mutate_immutable_value` | `mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); array elements, list elements, union payloads, and tuple components cannot be borrowed exclusively`。help `write elements through an exclusive slice with 'Array.write', or keep the value in a 'let mut' local or a record field` | 借用の式 |
| 2027 以降、`Drop.drop` の中の持ち主のフィールド | `E1012` | `drop_replace_whole_value` | `cannot borrow a field exclusively inside Drop.drop; read it or borrow it with 'ref' instead` | 借用の式 |
| 2026、レコードのフィールドの経路（可変な根） | `E1014` | `mutate_immutable_value` | メッセージは今のまま。help `record fields are immutable in edition 2026: copy the field into a 'let mut' local, change it, and rebuild the record with '{ b with f = ... }'; edition 2027 can borrow it with 'ref mut b.f'` | 今のまま |

- 2027 以降のフィールドの経路の `E1014` で、閉包が捕捉した持ち主（持ち上げた関数の `parameters[..capture_count]`）には related を付けず、help は `a closure captures 'b' as an immutable value; pass it by 'ref mut' as an argument instead` にする。捕捉した持ち主は持ち主と同じ名前と宣言の位置を持つ不変の局所なので、今の related の条件（7435〜7457 行）をそのまま使うと、`let mut b` を指して「`mut` なしで宣言した」と誤って書く。パターンの変数も related を付けない（`match` の束縛で作った局所の集合、`views` で見分ける）。経路にフィールドがない場合は今のまま（baseline）。
- 経路にフィールドがない場合（`let x = 1` の `x = 2` など）のメッセージと help は変えない（`tests/diagnostic_baseline/mutate_immutable_value` の baseline を変えない）。
- 文言は保証の対象外（`docs/stability.md`「保証の単位」の表）。メッセージの断片を照合するテスト（`tests/mutable_slices.rs:91` の `mutable access requires 'let mut'` など）は、断片が残るようにする。

### 5. native/WASM・IR・資源

- 変わるのは所有権の検査の受理だけである。新しく受理したプログラムは、既存の下げ方（`Borrow(場所, true)` の GEP、`src/llvm.rs:6673`〜6679。排他借用の前のフレームの値の移し替え、6183〜6195 行）で動く。native と WASM の差はない。
- 既存のプログラムの生成 IR は変えない（S2）。

## 設計

### 判定の置き場所（D6。既定案）

- `src/ownership.rs:7429` の条件を、`usage != Use::Write && !unmarked_fields(place).is_empty() && !(self.field_borrows && self.record_field_path(place))` にする。可変性の判定（7419〜7428 行）は変えない。
- `record_field_path(place)`: 根の宣言の型からたどらない。段を足すときに記録する。`place()` の `E::Field`（7731〜7738 行）で `value.ty` が `Type::Record` のときだけ、段をレコードのフィールドと認める（`field_access` は `autoderef` の後で `Type::Record` にだけ `Field` を作る。`src/check.rs:16376`〜16402）。記録は `through_drop` と同じく `Place` に持たせる（例 `non_record_step: bool`。印の後にレコードのフィールドでない段が 1 つでもあれば真で、`record_field_path(place)` は `!place.non_record_step`）。`through_drop` と同じ形にするのは、根を作るすべての場所（`E::Local`・`E::Static`・外部の loan（`external_loans` の印だけの場所）・view の根）が同じく偽で始まり、段で `|=` するだけで済むためである。`E::Field` で `value.ty` が `Type::Record` でなければ真、`E::UnionPayload`・`E::ListTail`・組み込みの `E::Index` で真。参照外しの場所は loan の場所の値を受け継ぐ（7779〜7795 行は `loan.place.clone()` を積む）。`place()` の外で段を足す場所（反復の別名が `ELEMENT` を足す `src/ownership_control.rs:853` など）は同じ規則で求めるか真にする（拒否のまま）。タプルの成分の射影（`src/control.rs:1364`）は `value.ty` が `Type::Tuple` なので真になる。根の宣言の型から段をたどると、参照外しの場所は loan の場所の段に参照先の段を足すので、型が合わない（`src/ownership.rs:8316`〜8317 の注記「the fields of a borrow through a reference that a value holds name a part of another value」）。
- `field_borrows`: 型検査の時点で、関数ごとの `read_conventions`（`names.borrowing_modules.contains(module)`、`src/check.rs:13099`）を `CheckedFunction` の新しい印（例 `pub(crate) field_borrows: bool`）に写す。`closures::lower`（`src/closures.rs` 526〜535 行・689 行、`src/closures_rec.rs` 360〜365 行）・bridge・特殊化が作る関数は、作った元の関数の印を受け継ぐ（`origin.parent` と同じ元。`origin.parent` をたどって引いてもよい）。`closures::lower` が作る関数の `CheckedFunction::module` は `"$lambda"`・`"$once"`・`"$case"` で、`closures::lower`（`src/check.rs:10479`）は所有権の検査（10492 行）の前に走るので、モジュール名から edition を引かない。所有権の検査が edition を見る唯一の場所になるので、`docs/architecture.md:1610` に書く。
- `Drop.drop`: `access` の `Use::Write` の判定（7388 行）の隣に、`usage == Use::MutBorrow && Some(place.root) == self.drop_root && !place.fields.is_empty() && self.field_borrows && self.record_field_path(place)` のとき `E1012`（`drop_replace_whole_value`）を返す判定を足す。2026 と、レコードのフィールドでない経路は、今の `E1014` のまま。
- 健全さを示せない構成が監査で見つかったら（S1）、その構成に限って、loan を持ち主の全体（`fields` を空にした場所）に置く粗い規則に縮める（D5）。受理は保ち、別のフィールドの同時の借用だけを諦める。

### 監査（手順 3 で 1 つずつ確かめ、テストで固定する）

| 項目 | 場所 | 確かめること |
| --- | --- | --- |
| 格納の事実（A31） | `store_target`（`src/ownership.rs:8301`）、`StoreFacts::paths`（1072 行付近）・`self_paths`（1150 行） | 排他の loan の `store_target` は根の宣言の型で判定する（8326〜8331 行）。フィールドの loan でも根の型を見るので保守的（過大）で、健全である。`paths`・`self_paths` は共有の借用の部分だけを記録し、排他のフィールドの loan はどちらにも入らない（全体として扱われる）ことを確かめる。フィールドの借用を通して借用を格納すると、持ち主に loan が残る（テスト） |
| 自己参照の格納 | `store_arguments`（`src/ownership.rs:8088`。8128〜8138 行は `overlaps` だけで判定）と代入の判定（14792〜14797 行） | 排他のフィールドの loan（`ref mut p.hv`）へ、同じ持ち主の別のフィールドの借用（`ref p.s`）を呼び出しが格納する形。`p.[hv]` と `p.[s]` は重ならないので、今の `store_arguments` の判定を通り、`store_through` が `p` に自分のフィールドの借用を持たせる。代入と同じ根の判定（`!unmarked_fields(&place).is_empty() && self.loans[*id].place.root == place.root`）を `store_arguments` の条件に足して `E1013`（`self_referential_store`）にする（代入 `p.hv = View { data: ref p.s }` は今 `E1013`）。今は 7429 行のために、印の後にフィールドを持つ排他の loan がここへ来ないので、既存のプログラムの受理は変わらない |
| region の場所（A12・A13） | `external_loans`（6770 行）、`unmarked_fields`（103 行） | 印は slot が 2 つ以上の仮引数だけに付く（6794〜6798 行）。印は `external_loans` が作る場所の先頭にあり、`place()` の段ではないので、段の記録（仕様 1 の 2.）は印の後の段だけを見る。印の後の段がレコードのフィールドの場所は受理する（`record P {r s} { left: ref mut {r} B, right: ref mut {s} B }` の仮引数の `ref mut p.left.n`。2027 の `p.left.n = 5` は今も受理され、同じ場所である）。印の後に要素・ペイロードの段があれば拒否のまま。`IndexBorrowMut` の生成関数（region `r` を持つ `self`）で印が付くかを確かめる（未確認。付いても段の記録で受理される。S4） |
| パターンの別名 | `src/ownership_control.rs` の束縛（538 行の `aliases`、パターンの変数の view） | パターンの変数は不変の view で、根の可変性で落ちる。view の loan はフィールドの場所に付き（「現状」の表の後の項）、フィールドの排他借用とは `Place::overlaps` で重なる。`match` の隠れた持ち主の別名（538 行）は利用者が名指せない |
| コールバック | `Mutex.with_lock`（`src/check.rs:1959` の説明）、region の契約を持つコールバック（`CheckedFunction::callback_contracts`） | 参照先を根とするフィールドの借用（`ref mut value.n`）は、コールバックの本体で閉じる。結果に借用を持ち出せない規則（`Mutex.with_lock` の結果の検査）は変わらない |
| クロージャ | 捕捉の規則（`borrowing.md:432`） | 捕捉した持ち主は閉包の中で可変でないので、フィールドの排他借用は今のまま `E1014`。排他借用を捕捉する閉包の拒否も変わらない |
| 窓 | 共用体のペイロード（`PAYLOAD`）、`match` の view | ペイロードの段を含む経路は拒否のまま。ペイロードの中のレコードのフィールドも、`PAYLOAD` の段の後なので拒否のまま |
| フレームの再配置 | `src/llvm.rs:6183`〜6195、`src/llvm_frame.rs:585`〜598 の `frame_of_place` | 排他の `Borrow` は、場所がフレームの値を持てば借りる前にヒープへ移す。`frame_of_place` は `Field` をたどるので、フィールドの場所でもフィールドのフレームの部分だけを移す。`Domain`・arena の値を持つフィールドを借用で置き換えて `live == 0`（E2E） |
| PR01 の属性 | `parameter_facts`（`src/llvm_facts.rs:43`〜90。`noalias` は 65〜71 行）、`scopes`（275 行） | `noalias` と借用の scope は、呼び出しの引数どうしが重ならないことを所有権の検査が保証していることに依る。別のフィールドは別のメモリなので、`f (ref mut b.n) (ref b.xs)` で両方に属性が付いても健全。重なる組（`ref mut b.f` と `ref b`、`ref mut b.f` と `ref b.f.g`）は `E1014` になることをテストで固定する |
| 最後の使用（A17） | `src/last_use.rs:55`〜58 | フィールドの借用は持ち主を根として移動の対象から外す（共有の借用と同じ）。既存のプログラムの移動は変わらない |

### 段ごとの変更

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1 | `src/check.rs`（`CheckedFunction`）、`src/closures.rs`・`src/closures_rec.rs` | 関数ごとに edition 2027 以降かの印を持たせ、生成関数に受け継がせる（D6）。生成物は変えない |
| 2 | `src/ownership.rs` の `Place` と `place()`、`access`（7230 行）、新しい `record_field_path`、`store_arguments`（8088 行） | 段の記録を足し、7429 行の条件を変え、`Drop.drop` の判定を足す（設計の「判定の置き場所」）。`store_arguments` に代入と同じ根の判定を足す（監査の表の「自己参照の格納」） |
| 3 | `src/ownership.rs` の `access` の診断（7430〜7461 行） | 仕様 4 のメッセージと help。経路にフィールドがない場合は変えない |
| 4 | `tests/`、`spec/rules/OWN.md`、文書 | テスト計画とドキュメントの節 |

### 生成 IR

- 既存のプログラム: 変更なし（S2）。
- 新しく受理したプログラム: `Borrow(Field(…), true)` は `place()` の GEP（`src/llvm.rs:6673`〜6679）を渡す。排他スライスは今の `Slice` の下げ方。どちらも局所の場合と同じ道である。

## 実装手順

1. **変更**: なし（`target/scratch/a39/` に記録する）。**内容**: 「現状」の表を dev バイナリで確かめる。`tests/fixtures/` の各ディレクトリ、`examples/`、std を使う代表のプログラムを `tsuzuri build --emit llvm`（native と `--target wasm32`、`-O0`）で IR にして保存する。edition 2027 で読むテスト（`tests/edition_2027.rs`、`tests/conformance/` のマニフェストなしの `.tz`、`tests/fixtures/` のマニフェストなしの fixture）のうち、フィールドの排他借用を `E1014` と期待するものを `rg -n 'ref mut [a-z_]+\.[a-z_]'` と診断の期待で洗い出す（件数は未確認）。`IndexBorrowMut` の形の生成関数で、外部の場所に region の印が付くかを確かめる（監査の表の 2 行目）。**確認**: 表と同じ出力と診断。
2. **変更**: `tests/borrowed_records.rs`、`tests/edition_2027.rs`。**内容**: テスト計画の Rust テストを足す。2027 の受理のテストは今は失敗し、2026 と拒否のテストは通る。**確認**: `node scripts/slot.mjs -- cargo nextest run --locked --test borrowed_records --test edition_2027` で、足した 2027 の受理のテストだけが失敗する。
3. **変更**: `src/check.rs`（段 1）、`src/ownership.rs`（段 2・3）。**内容**: 判定と診断。監査の表を 1 行ずつ確かめ、テスト計画の対応するテストを通す。健全さを示せない構成は D5 で縮める。**確認**: T0。足したテストがすべて通る。手順 1 の IR と `diff` して一致する（S2）。`node scripts/slot.mjs -- cargo nextest run --locked --test borrowed_records --test edition_2027 --test types_ownership --test mutable_slices --test rc --test slices --test user_drop --test concurrency`、T1。
4. **変更**: `spec/rules/OWN.md`、`tests/conformance/OWN/`、`tests/features.mjs`・`tests/fixtures/borrowed_records/Main.tz`。**内容**: 仕様の規則（番号は実装の時点の OWN の次の空き）と適合性テスト、E2E の追加。**確認**: `cargo build`、`node scripts/conformance.mjs target/debug/tsuzuri --rule <OWN の新しい ID>`、`node scripts/check-spec.mjs --write`、`node scripts/check-spec.mjs`、`node tests/features.mjs target/debug/tsuzuri borrowed_records`。
5. **変更**: 文書（ドキュメントの節）。**確認**: T2（テスト計画の検証コマンド）。

## テスト計画

### Rust テスト（`tests/borrowed_records.rs`、`tests/edition_2027.rs`）

`analyze` は edition 2026 で読む（`docs/stability.md:102`）。2027 は `analyze_with_edition(…, Edition::E2027)`（`src/lib.rs:105`）で読む。受理は `emits` 相当（native と WASM の IR、決定性、宣言の重複なし）で確かめる。

- `field_exclusive_borrows_need_edition_2027`: 「現状」の表の、2027 の A39 の後が受理になる 9 つの形（`ref mut b.xs`、`ref mut b.xs[0..]` と `Array.write`、`Array.write 0 9 b.xs`、`bump (ref mut b.n)`、`put b.xs`、`r: ref mut B` の `ref mut r.xs[0..]`、`ref mut o.inner.xs[0..]`、`h.g[1] = 42`、`Mutex.with_lock` の `bump (ref mut value.n)`）が 2027 で受理される。`put b.xs` と `h.g[1] = 42` を除く同じソースが 2026 で `E1014`（`mutate_immutable_value`）のままで、位置が今と同じで、help が `edition 2027` を含む。`h.g[1] = 42` の 2026 版は、インスタンスの本体を 2026 で受理される形（例 `let mut arr = Array.copy (ref self.data)`、`Array.write (deref k) v (ref mut arr)`、`deref self = Grid { data: arr }`）に替えて、`h.g[1] = 42` の `E1014` と位置（土台の `h.g`）を確かめる（本体が `self.data[…] = v` のままだと、`analyze_with_edition` は最初の診断の `E1012` を返す。`src/lib.rs:105`〜119）。
- `field_exclusive_borrows_keep_rejections`（2027）: 仕様 2 の表のすべての行が、表のコードと kind になる。根が `let` のときは related `declared_here` がある。閉包の中の `ref mut b.n`（`\() -> bump (ref mut b.n)` が捕捉した `let mut b`）は `E1014` で、`declared_here` を付けない（仕様 4）。
- `disjoint_field_borrows_coexist`（2027）: `let a = ref mut b.n` と `let c = ref mut b.xs[0..]` を同時に生かして両方に書ける。同じフィールドを 2 回排他借用すると `E1014`（`borrow_conflict`）。`ref mut b.n` が生きている間の `b` の全体の読み出し・move・代入が `E1014`。`ref mut o.inner` が生きている間の `ref o.inner.xs` が `E1014`、`ref mut o.inner.xs` と `ref o.m` は共存できる。
- `field_borrows_through_ref_mut_parameters`（2027）: `r: ref mut B` の `ref mut r.xs[0..]` と `ref mut r.n` が共存し、呼び出し元の `b` に書いた値が残る。
- `field_borrows_through_call_results`（2027）: `record Log { count: i64 }` と `def choose :: ref mut B -> ref mut Log -> ref mut B` で、`let r = choose (ref mut b) (ref mut log)` の後の `Array.write 0 9 r.xs` が受理される（今は `E1014`、`mutate_immutable_value`。同じ場所への `r.xs[0] = 7` は今も受理）。`r` が生きている間の `log.count = 3` は `E1014`（`borrow_conflict`）のまま。
- `field_borrow_through_region_marker`（2027）: `record P {r s} { left: ref mut {r} B, right: ref mut {s} B }` を仮引数 `p: P {r s}` で受け取り、`bump (ref mut p.left.n)` が受理される（今は `E1014`、`mutate_immutable_value`。同じ場所への `p.left.n = 5` は今も受理）。
- `field_borrow_conflicts_with_live_view`（2027）: `r: ref mut B` で `match deref r with | B { xs = xs } ->` の中、`xs: [string]` が生きている間の `ref mut r.xs` が `E1014`（`borrow_conflict`）、`ref mut r.n` は受理。
- `field_borrows_follow_the_creating_function`: 2027 の閉包の本体（`Mutex.with_lock (ref m) (\value -> bump (ref mut value.n))`）と tzroutine の本体（`$once`。`tzroutine { let mut b = …; bump (ref mut b.n); … }`）が受理され、同じソースを 2026 で読むと `E1014`（`mutate_immutable_value`）のまま（設計の `field_borrows` の受け継ぎ）。
- `field_borrow_self_store_is_e1013`（2027）: `record View {r} { data: ref {r} string }`、`record P {r} { hv: View {r}, s: string }`、`def set_view {q r} :: ref mut {q} View {r} -> ref {r} string -> unit` で、`set_view (ref mut p.hv) (ref p.s)` が `E1013`（`self_referential_store`）。代入 `p.hv = View { data: ref p.s }` と同じ（監査の表の「自己参照の格納」）。
- `field_borrow_stores_keep_loans_on_the_owner`（2027）: フィールドの排他スライスを通して借用を格納すると、持ち主に loan が残る（共有参照を要素に持つ配列のフィールドに `Array.write` で `ref other` を書いた後、持ち主が生きている間の `other` の置き換えが `E1014`。A31 の規則。書き方は手順 3 で決める）。
- `drop_field_exclusive_borrow_is_e1012`（2027）: `Drop.drop` の中の `ref mut self.xs[0..]` が `E1012`（`drop_replace_whole_value`）。2026 は `E1014`（`mutate_immutable_value`）のまま。
- `parallel_field_arguments_keep_attributes`（2027、`emits`）: `f (ref mut b.n) (ref b.xs)` が受理され、`-O3` の IR で呼び出し先の仮引数の属性が今の規則のとおりに付く。`f (ref mut b.xs) (ref b)` は `E1014`。
- `field_borrows_keep_existing_ir`: `tests/fixtures/borrowed_records/Main.tz` など、フィールドを排他借用しない fixture の IR が手順 1 の記録と一致する（手順 3 の `diff` を固定する。比較の方法はテストの中で 2 回作って比べる既存の決定性の検査と、手順 1 の記録の `diff`）。

### 適合性テスト（`tests/conformance/OWN/`）

- `OWN-0xx-record-field-exclusive-borrow.tz`（番号は実装の時点の次の空き。マニフェストなしなので edition 2027）: `ref mut b.xs[0..]` と `bump (ref mut b.n)` で書いた値を印字する（`expect: stdout "…"`）。
- `OWN-0xx-element-path-exclusive-borrow.tz`: `ref mut arr[0].n` が `check-error E1014 line=N kind=mutate_immutable_value`。
- edition 2026 の `E1014` は Rust テストで固定する（規則は edition 2027 の文なので、2026 の適合性テストは置かない）。

### E2E（`tests/features.mjs` の `borrowed_records`、native と WASM の `-O0`／`-O3`、`live == 0`）

`tests/fixtures/borrowed_records/Main.tz` はマニフェストがないので edition 2027 で読む。

- フィールドの排他スライスへの書き込み（`ref mut b.xs[0..]` と `Array.write`）の後の合計。
- 別のフィールドを 1 つの呼び出しに渡す（`f (ref mut b.n) (ref b.xs)`）。
- 文字列を持つフィールドを `ref mut` で置き換える（古い値の解放、`live == 0`）。
- `Domain`・arena の中で作った値を持つフィールドを、借用を通して置き換える（フレームの値の移し替え。`live == 0`）。
- `IndexMut` の対象がフィールド（`h.g[1] = 42`）。

### 既存テストへの影響

- 期待値の変更は、手順 1 で洗い出した edition 2027 のテストのうち、フィールドの排他借用を `E1014` と期待するものだけ（件数は未確認）。それらは受理の期待に直し、PR の説明に一覧を書く。
- `tests/diagnostic_baseline/mutate_immutable_value` は経路にフィールドがない形なので変わらない。

### 性能

受理を増やすだけで、既存のプログラムの生成物は変わらない。性能は主張しない（NC20 が `+=` の文の確保を測る）。

### 検証コマンド

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`node scripts/slot.mjs -- cargo nextest run --locked --test borrowed_records --test edition_2027 --test mutable_slices`。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`。
- T2: T1、`cargo build`、`node tests/features.mjs target/debug/tsuzuri borrowed_records`、同じく `mutable_slices`・`in_place_writes`・`rc`（suite は 1 回に 1 つ）、`node tests/user_drop.mjs target/debug/tsuzuri`、`node tests/control.mjs target/debug/tsuzuri`、`TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/debug/tsuzuri`（README の「所有権・借用・region」の行）、`node scripts/conformance.mjs target/debug/tsuzuri --area OWN`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`、`node scripts/check-docs.mjs <変更したページ>`。

## ドキュメント

- `docs/language.md:1963`: 「レコードの特定フィールドのみ … 禁止されています」を、edition 2026 ではレコードのフィールドへの代入も排他借用もできないこと、2027 以降は可変な持ち主か排他参照を根にしたレコードのフィールドへの代入（A18）と排他借用（A39）ができること、要素単位の排他借用はどの edition にもないこと、に書き直す。続く 1964 行は、代入構文 `values[index] = value` がないことを edition 2026 に限る文に直す（2027 は可変な持ち主か排他参照を根にした要素への代入を受理する。A18）。要素単位の排他借用 `ref mut values[index]` がないことと、排他スライスを通した `Array.write` 等の置き換えは、全 edition の文として残す。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`: 「排他借用（ref mut）と排他性規則」（67 行）に、edition 2027 のレコードのフィールドの排他借用の小節を足す（`ref mut point.x`、`ref mut b.xs[0..]` の例を `run=` 付きで、別のフィールドは同時に借りられること、持ち主の全体は使えないこと、要素は借りられないこと）。236 行の表に、`ref mut T` の借用元として「可変な持ち主か排他参照を根にしたレコードのフィールド（edition 2027 以降）」を足す。
- `_tsuzuri/language-reference/built-in-types-and-modules/record.md`: 84〜91 行の 2027 のフィールドへの代入の段落に、同じ場所を `ref mut` で借りられることを足す。3 行・335 行・351 行の要約に排他借用を足す。
- `docs/stability.md:85`: 2027 の差の説明に、可変な持ち主か排他参照を根にしたレコードのフィールドの排他借用も許すことを足す（A18 の差の一部。新しい差ではない）。
- `docs/stability.md:89`: 「明示した借用・…・所有権検査の規則は変更しません」に、A39 の例外（edition 2027 以降で、可変な持ち主か排他参照を根にしたレコードのフィールドの経路の排他借用を受理する。A18 の射影の代入の借用の形）を除く、と足す。
- `docs/architecture.md:1610`（「生成と所有権検査は edition を見ず」）: 例外として、所有権の検査がレコードのフィールドの排他借用の許可にだけ関数のモジュールの edition を見ることを書く。`record_field_path` と、健全さの根拠（`Place::overlaps`、監査の表）を書く。
- `src/explain/mutate_immutable_value.md`: edition 2027 以降はレコードのフィールドを `ref mut` で借りられること、要素は借りられないことを 1 文で足す。
- `guide/card.md:87`（edition 2027 の項）: レコードのフィールドは `let mut` の持ち主から `ref mut p.x` でも借りられることを 1 句足す（card の長さを検査するテストがあれば、それに合わせる。未確認）。
- `spec/rules/OWN.md`: 次の 1 行を、実装の時点の次の空きの番号で足す（D8）。文書は `borrowing.md` の新しい小節。
  - ``In edition 2027, a record-field path rooted at a `let mut` owner or a `ref mut` referent MAY be borrowed exclusively; a path that steps through an element, union payload, list tail or tuple component MUST NOT (E1014).``
- `_tsuzuri/language-reference/compiler/conformance.md` の生成部分: `node scripts/gen-docs.mjs` で作る。
- README: 書いてあること（特徴の一覧・検証手順）は変わらないので変更しない。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/ownership-and-memory/borrowing.md _tsuzuri/language-reference/built-in-types-and-modules/record.md _tsuzuri/language-reference/compiler/conformance.md` を通す。

## 受け入れ条件

- [ ] 「現状」の表の 2027（A39 の後）の列のとおりに受理・拒否される。2026 の列はコード・kind・位置が今と同じで、フィールドの経路の help だけが edition 2027 を案内する。
- [ ] 別のフィールドを同時に排他借用でき、同じフィールド・内側のフィールド・持ち主の全体との組は `E1014` になる。
- [ ] `Drop.drop` の中の持ち主のフィールドの排他借用が `E1012`（`drop_replace_whole_value`）。
- [ ] 監査の表のすべての行に対応するテストがあり、通る。縮めた構成（D5）があれば、PR の説明とこのチケットの末尾に書いた。
- [ ] フィールドを排他借用しないプログラムの生成 IR が手順 1 の記録と一致する（native と WASM）。
- [ ] E2E の `borrowed_records` の追加ケースが native と WASM の `-O0`／`-O3` で通り、`live == 0`。
- [ ] OWN の新しい規則（実装の時点の次の空きの番号）と、その適合性テストがある。`check-spec.mjs --require-markers` が通る。
- [ ] std だけの特権・許可表・インスタンスの本体かどうかの判定がない。
- [ ] [PM10](../_perfs/PM10-std-consume-copies.md) が done なら、Copy の持ち主の `ref mut b.xs[0..]`（`record B { xs: [f64] }`）を借りて書いた後に `b` を返す関数が要素を複製しないテストを足した（`Tensor` のフィールドを持つ `model` は Copy でないので、今も move で複製しない）。done でなければ、PM10 の手順 9 が足す。
- [ ] T2 が通る。ドキュメントの節のページが更新されている。

## 落とし穴

- `unmarked_fields` が空でないことだけを見て許さない。タプルの成分の段はフィールドと同じ小さな添字を持つので、段の値では区別できない。段を足すときの入れ物の型（`E::Field` の `value.ty` が `Type::Record` か）で記録する。
- 根の宣言の型から段をたどらない（`place_type` を使わない）。参照外しの場所は loan の場所の段に参照先の段を足すので、根の型と合わない。たどると、呼び出しの結果の参照（`let r = choose (ref mut b) (ref mut log)` の `r.xs`。`r` は `b` と `log` の両方の loan を持つ）や、region の印の付いた外部の場所（`p.left.n`）を、同じ場所への代入は 2027 で受理されるのに誤って拒否する。
- 可変性の判定を緩めない。`let` の持ち主、共有参照を経由する経路、パターンの変数、閉包の捕捉は今のまま可変でない。変えるのは 7429 行の第 2 の条件だけ。
- edition による拒否を型検査の側に置かない（型検査は印を写すだけ）。排他借用は明示の `ref mut`、主語の暗黙の借用、`mut T`、`IndexMut` の対象、排他スライスなど型検査の多くの場所で作られるが、所有権の検査の `access` は 1 か所である。
- 2026 の診断を変えすぎない。help だけを直し、メッセージ・コード・kind・位置は変えない（S3）。経路にフィールドがない場合の文言も変えない（baseline）。
- 縮める規則（D5）を特権に戻さない。std の本体も利用者のコードと同じ規則で書く。
- 新しく受理したプログラムで、借用の後に Copy の持ち主（配列をフィールドに持つレコードなど）を move すると複製になる（A17 の X1）。Copy でない持ち主（`Tensor` のフィールドを持つ `model`）は move で、複製しない。これは今の共有の借用と同じ制限で、PM10 の Phase 2 が直す。A39 で `src/last_use.rs` を変えない。

## 対象外

- 要素単位の排他借用（`ref mut xs[i]`、`ref mut arr[0].n`）。
- edition 2026 での受理（D2）。
- region の印の後の段に要素・ペイロードを含む外部の場所の受理（仕様 2 のとおり `E1014`）。印の後の段がすべてレコードのフィールドの場所は対象で、受理する（仕様 1 の 2.。NC20 に要る形が受理できなければ S4 で報告する）。
- 複合代入 `+=` と書き窓 `ref mut t[…]`（NC20）。借用の根の局所の最後の使用での移動（PM10）。
- 不透明なレコードのフィールドの公開（`E1022` のまま）。

## 決定事項

### D1: std だけの特権を作らず、一般の規則にする（承認済み。NC20 の D2）

- 決定: std だけの特権は作らない。許可表 `exclusive_field_record` も、インスタンスの本体かどうかの判定も置かない。レコードのフィールドの経路の排他借用を、一般の規則として入れる。NC20 の Phase 2・3 はこのチケットに依存し、`std/Tensor.tz` の本体はこの規則で `ref mut target.data[0..]` を取る。
- 理由: (1) 健全さの義務は、特権でも一般の規則でも変わらない。特権の本体もフィールドの場所に排他の loan を作るので、所有権の検査が正しいことに依存する点は同じである。特権は誤りに触れるプログラムの数を減らすだけで、正しさの根拠を増やさない。一般の規則なら、適合性テストとすべてのプログラムが検査する。必要な仕組みはほぼそろっている（`Place::overlaps`、2027 のフィールドへの代入、`store_target`、`frame_of_place` の `Field`）。(2) 特権の境界は何も守らない。2027 では、std のどの関数も `self.data[i] = v` で `Tensor` の中身を書き換えられ、不変条件を破る `Tensor { … }` も作れる。(3) 利用者の型に性能の崖と書きにくさを作らない。特権のままでは `record Model { w: Tensor<f64>, b: Tensor<f64> }` の `model.w += g` は `E1014` である。案内の「let mut へ移して戻す」は、`Tensor<f64>` のような Copy でないフィールドでは `{ model with w = w }` が部分 move の `E1012` になるので `Model { w: w, b: model.b }` とすべてのフィールドを並べて作り直す書き方になり、Copy の配列のフィールド（`[f64]`）では全要素を複製する。(4) 排他スライスは長さが固定なので、`Tensor` の `data.length` の不変条件も保たれる。(5) 内部の組み込み関数（型ごとのコンパイラの知識）や、ランタイムのカーネル（借用検査の外の信頼するコード）より、検査される普通のコードがよい。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D2: edition 2027 以降に限る（承認済み）

- 決定: edition 2027 以降で読むソースだけで受理する。edition 2026 は今の `E1014`（`mutate_immutable_value`）のままで、help が edition 2027 を案内する。std は `Edition::LATEST`（2027）で読むので、NC20 の std の本体は使える。
- 理由: edition 2026 はレコードのフィールドを不変に保つ（`b.f = v` は `E1012`）。2026 で `ref mut b.f` を受理すると、同じフィールドへの代入は書けないのに、借用を通して書けることになり、2026 の規則が食い違う。A39 は A18 の射影の代入（`docs/stability.md` の「edition の差として許す」4 つ目の承認済みの差）の借用の形なので、宣言済みの差の中で受理を増やすだけで、凍結の規則（NC17 の未決の質問 2 の決定で明確にした）に合い、新しい edition の差は要らない。NC20 のフィールドの対象（`model.w += g`）と、フィールドの経路を土台にした `IndexBorrowMut` は 2027 から働く。局所の `let mut t` の対象（`t += u`、`ref mut t[[…]]`）はフィールドの借用が要らないので、全 edition で働く。
- 採らなかった案: 全 edition に入れる（起票時の NC20 の D2 の理由 (6)。2026 のフィールドの不変性と食い違うので改めた）。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D3: 対象の場所と可変性の判定（承認済み）

- 決定: 仕様 1。根が `let mut` か排他参照の参照先で、region の印の後のすべての段がレコードのフィールドである場所。判定は所有権の検査の場所で行う。可変かどうかは今の判定のまま変えない。変えるのは `src/ownership.rs:7429` の「フィールドを経由する排他のアクセスの拒否」だけで、レコードのフィールドの段に限って外す。loan はフィールドの場所に置き、`Place::overlaps` のとおり別のフィールドとは重ならない。
- 理由: 2027 のフィールドへの書き込み（`Use::Write`）がすでに同じ判定と同じ重なりの規則で動いている。排他借用は書き込みと同じ場所を、少し長く借りるだけである。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D4: 拒否のまま残す形（承認済み）

- 決定: 仕様 2。要素・共用体のペイロード・リストの tail・タプルの成分を含む経路は `E1014`（`mutate_immutable_value`）。共有参照を経由する経路は `E1014`（kind は今のまま）。`Drop.drop` の中で持ち主のフィールドを排他借用するのは、フィールドへの代入と同じ `E1012`（`drop_replace_whole_value`）。不透明なレコードのフィールドは、モジュールの外から触れない（`E1022`）。
- 理由: 要素とペイロードは今の所有権の検査で 1 つの場所（`ELEMENT`・`PAYLOAD`）にまとめてあり、別の要素を区別できない。`Drop.drop` の中のフィールドへの代入はすでに `E1012` で、借用を通した置き換えも同じ理由（解放の途中の値を書き換える）で拒否する。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D5: 健全さを示せない構成は、持ち主の全体の loan に縮める（承認済み）

- 決定: 監査で健全さを示せない構成が見つかったら、特権には戻さず、その構成に限って loan を持ち主の全体に置く粗い規則に縮める。それでも示せなければ S1 で止める。
- 理由: 粗い規則でも、`ref mut b.f` を関数へ渡す・排他スライスにするという主な用途は通る。失うのは別のフィールドの同時の借用だけである。特権は正しさの根拠を増やさない（D1）。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D6: 判定は `access` の 1 か所と、関数ごとの edition の印で行う（既定案）

- 決定: 設計の「判定の置き場所」。`access` の 7429 行の条件に `field_borrows && record_field_path(place)` を足す。`field_borrows` は関数を書いたモジュールが edition 2027 以降かで決め、型検査の `read_conventions` を `CheckedFunction` の印に写す。生成関数（持ち上げたラムダ・bridge・特殊化）は作った元の関数の印を受け継ぐ（持ち上げたラムダの `module` は `"$lambda"`・`"$once"` なので、モジュール名からは引けない）。
- 理由: 排他借用を作る場所は型検査に多数あるが、所有権の検査で判定する場所は 1 つである。所有権の検査に edition を持ち込むのはここだけにして、`docs/architecture.md` に書く。
- 採らなかった案: 型検査で 2026 のフィールドの排他借用をそれぞれ拒否する（作る場所ごとに検査が要り、漏れやすい）。
- 状態: 既定案（実装者はこの案に従う）。

### D7: 診断は新しいコード・kind を使わず、フィールドの部分の文言を直す（既定案）

- 決定: 仕様 4。`E1014`（`mutate_immutable_value`）のメッセージと help のフィールドの部分を直す。経路にフィールドがない場合は変えない。2026 は help だけを直す。`Drop.drop` は `E1012`（`drop_replace_whole_value`）のメッセージを借用の文に分ける。フィールドの経路の `E1014` で、閉包が捕捉した持ち主（持ち上げた関数の `parameters[..capture_count]`）には related を付けず、help は `a closure captures 'b' as an immutable value; pass it by 'ref mut' as an argument instead` にする。パターンの変数も related を付けない（`match` の束縛で作った局所の集合で見分ける）。経路にフィールドがない場合は今のまま（baseline）。
- 理由: 2027 では「record fields … are immutable」が誤りになる。文言は保証の対象外で、kind とコードは変えない。断片を照合するテストは断片が残るように直す。
- 状態: 既定案。

### D8: 仕様の規則を 1 つ足す（決定）

- 決定: OWN に 1 つ、実装の時点の次の空きの番号で足す（ドキュメントの節の英文。edition 2027 の規則）。起票の文には番号を書かない。A38・NC20 の OWN の規則と番号が重なったら、後に入る方が付け直す。
- 理由: 規則の番号はリポジトリの慣行で実装の時点に決める。`scripts/check-spec.mjs` は重複と昇順だけを検査する。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

## 他チケットとの関係

- [NC20](NC20-compound-assignment-and-mutable-index-views.md)（後続）: Phase 2・3 は A39 に依存する。`std/Tensor.tz` の `AddAssign<Tensor<'a>, Tensor<'a>>` の本体は `ref mut target.data[0..]`、`IndexBorrowMut<Tensor<'a>, [Slice.Sel], Tensor.Mut<'a>>` の本体は `self.data` の排他借用を、この規則で書く。利用者の `model.w += g` と `ref mut model.w[…]` は edition 2027 から通る。NC20 の S3 は「A39 の規則だけでは std の本体が書けない」ことで、A39 の S4 と同じ問題である。NC20 は 2026 の `model.w += g` が `E1012` のままであることを確かめるテストを足す。
- [PM10](../_perfs/PM10-std-consume-copies.md): フィールドの借用は持ち主を借用の根にするので、借用の後の Copy の持ち主の move は複製になる（A17 の X1。Copy でない持ち主は move）。PM10 の Phase 2 が、A39 のフィールドの借用と NC20 の対象の借用の根も含めて精密にする。A39 は PM10 の前提ではない。
- [NC17](NC17-edition-2028-inclusive-range-family.md): edition 2028 を足すとき、A39 の判定（`borrows_parameters` と同じ集合）も 2028 を含める。
- [A38](A38-index-evaluation-order.md): 独立。NC20 の Phase 3 のフィールドの土台（`model.w[1, 2] += 1.0`）は A38 の評価順と A39 の排他借用の両方を使う。
- [A13](_completed/A13-exclusive-borrow-fields.md)・[A18](_completed/A18-parameter-conventions.md)・[A31](_completed/A31-in-place-writes-keep-stored-borrows.md)・[C08](_completed/C08-mutable-slices.md)・[A17](_completed/A17-last-use-move.md)（完了）: region の印と外部の loan（A13）、2027 の射影の代入（A18）、格納の事実（A31）、排他スライス（C08）、最後の使用の規則 X1（A17）の上に作る。
