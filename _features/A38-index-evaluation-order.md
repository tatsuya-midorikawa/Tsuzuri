# A38: 添字の評価順を組み込みの添字にそろえる（土台、キーの成分を左から右。`IndexMut` の `=` は右辺が先）

| 項目 | 内容 |
| --- | --- |
| ID | A38 |
| 優先度 | P0（同じ `x[k]` の副作用の順が型で変わる。NC03・NC04 を含む最初のリリースの条件（D5）。NC16・NC18・NC20 の Phase 3 の前提） |
| 規模 | M |
| 依存 | [A37](A37-index-base-place-borrow.md)（生成関数の印と、土台を借りる `Index` の下げ方を使う） |
| 後続 | [NC16](NC16-index-ref-and-zero-alloc-element-reads.md), [NC18](NC18-multi-axis-bracket-indexing.md), [NC20](NC20-compound-assignment-and-mutable-index-views.md)（Phase 3） |
| 状態 | todo |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20) の未決の問い (1) を、委任による決定で新しいチケットにした。HEAD `858a949c` で確認） |
| 承認 | 決定（D-66 の委任で決定、2026-11-25）: 評価順の規則（D1）、キーを評価する間の土台の共有の守り（D2）、`=` と複合代入の順（D3）、edition を使わないこと（D4）、リリースの条件と代わりの道（D5）、仕様の規則 2 つ（D7）。D6・D8・D9 は既定案 |
| 改善する劣位 | 一貫性: 組み込みの添字は土台をキーより先に評価し、`Index` の添字はキーを土台より先に評価する。`IndexMut` の `t[k] = v` はキーを右辺より先に評価し、組み込みの `xs[i] = v`（EVL-012。右辺が先）と違う。配列の `xs[i]` とテンソルの `t[[i]]` で副作用の順が変わり、利用者も AI も順を予測できない（GUIDE D-66「1 つの記号は 1 つの意味」）。不具合（借用）: 組み込みの添字ではキーが土台を書き換えると `E1014` だが、`Index`・`IndexMut` では通る（現状の表） |
| 手本にする既存実装 | `src/ownership.rs:7895` の `exclusive_slice`（土台に共有の守りを置いて境界を評価し、その後に排他の loan を取る）、`src/ownership.rs:7756`〜7778 の `place()` の組み込みの添字の腕（土台の場所に共有の守りを置いてキーを評価する）、`src/ownership.rs:15326`〜15337 の `E::Index`（土台を `held` に置いてキーを評価する）、`src/control.rs` の生成の束縛（名前は `$` で始め、`Provenance::Generated` にする。`NARROWED_PREFIX`、`src/check.rs:5368`） |
| 主な影響ファイル | `src/check.rs`（`ExprKind::Index` 14748〜14837 行、`ExprKind::Assign` の `IndexMut` の道 14895〜14987 行）、`src/polymorph.rs` と `src/check.rs` の `CheckedFunction`（5178 行）か `FunctionOrigin`（4892 行）（A37 の生成関数の印を、`Index`・`IndexMut` のすべての生成関数に広げる）、`src/ownership.rs`（`eval_call` 14153 行のキーの守り）、`tests/index_class.rs`、`tests/features.mjs` と `tests/fixtures/index_order/Main.tz`（新しい suite `index_order`）、`spec/rules/EVL.md`・`spec/rules/OWN.md`、`tests/conformance/EVL/`・`tests/conformance/OWN/`、`_tsuzuri/language-reference/values-and-functions/op-and-expressions.md`、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/compiler/conformance.md`（生成）、`docs/language.md`、`docs/architecture.md`。`src/llvm.rs` と `src/last_use.rs` は変えない見込み（並べ替えは型付き IR の並びで表す。設計）。D5 の代わりの道のときだけ `docs/stability.md` とリリースノート |

## 実装者向け早見

- 範囲: (1) `Index`・`IndexMut` の呼び出しで、土台をキーより先に評価する（組み込みの添字と同じ）。(2) キーを評価する間、土台の場所に共有の守りを置く。土台へ代入する・move する・排他借用するキーは `E1014`（kind `borrow_conflict`）にする。キーは土台を読める。(3) `IndexMut` の `b[k] = v` は、右辺、土台、キーの順にする（EVL-012 と同じ）。右辺が move する局所をキーが読む形（`b[s.length] = s`、`b[(x.n, 0)] = x`、今は `E1014` の `b[x.n] = x`）は、組み込みの添字と同じ `E1012`（kind `use_after_move`）になる。(4) 土台が単純な `Index` の呼び出し、キーの成分がすべて定数の式の `Index` の呼び出し、土台が単純で右辺とキーの順を観測できない（キーか右辺が定数の式か、両方が副作用のない式で、一方が move する局所を他方が読まない）`IndexMut` の呼び出しは、型付き IR と LLVM IR を変えず、所有権の守りだけを足す（定義は設計。キーが定数の式なら守りも置かない）。(5) 仕様の規則を EVL と OWN に 1 つずつ足し、順を E2E で固定する（native と WASM、`-O0` と `-O3`）。
- 変えないもの: 組み込みの 6 型（`[T]`、`[|T|]`、`Vec<T>`、`[T; N]`、`string`、`utf8string`）の添字の順と IR、`=` の EVL-012、[NC20](NC20-compound-assignment-and-mutable-index-views.md) の複合代入の順。
- 最初に読む: ① 下の「現状」の 2 つの表 ② `src/check.rs` 14748〜14837 行と 14895〜14987 行 ③ `src/ownership.rs:7895` の `exclusive_slice` と 15326 行の `E::Index` ④ A37 の設計の段 2（生成関数の印）と段 3（場所の土台の `Borrow(base, false)`）。
- 最初に書くテスト: `tests/index_class.rs` に `index_evaluates_the_base_before_the_key`（今は順が `21` なので失敗する）と `index_key_cannot_write_its_base`（今は受理されるので失敗する）。
- 触ってはいけない: 組み込みの添字（読み出し・`ref`・スライス・代入）の型付き IR と LLVM IR、A37 の一時値の土台の規則と診断（`E1013` の位置と help）、NC04 のキーの借用の終わり（式の終わりで閉じる）、`Index`・`IndexMut` のメソッドの型（`index :: ref Key -> ref Self -> Output`、`index_mut :: ref Key -> Value -> ref mut Self -> unit`。対象は最後: D-59）、部分適用やクロージャを作らない直接の呼び出し。
- 完了の見分け方: 「現状」の順の記録が、組み込みとクラスで同じになる（読み出しは `12`、代入は `312`）。native と WASM の `-O0`・`-O3` で同じ値になる。土台が単純な `Index` の呼び出しの IR が、手順 1 の記録と 1 バイトも変わらない。

## 目的

添字 `x[k]` は、組み込みの配列でもテンソルでも同じ形で書く。ところが今は、型によって副作用の順が違う。配列の `(arr ())[key ()]` は土台、キーの順に評価する。`Index` を持つ型の `(mk ())[key ()]` は、キー、土台の順に評価する。代入も同じで、配列の `xs[i] = v` は右辺を先に評価し（EVL-012）、`IndexMut` の `t[k] = v` はキーを右辺より先に評価する。

組み込みの順はリリース済みで、文書にもある（スライスは「配列、開始、終了」、代入は「右辺、そのあと代入先」）。`Index`・`IndexMut`（NC04）はまだどのリリースにも入っていない。このチケットは、`Index`・`IndexMut` の経路を組み込みの順にそろえ、1 つの規則にする。あわせて、キーが土台を書き換えるプログラムを、組み込みの添字と同じく `E1014` にする。規則は仕様の規則と適合性テストで固定し、NC16・NC18・NC20 が同じ順の上に作れるようにする。

## 着手条件と停止条件

### 着手条件

- [A37](A37-index-base-place-borrow.md) が done（生成関数の印と、土台を借りる `Index` の下げ方。A38 は印を広げ、下げ方の上に並べ替えを置く）。
- GUIDE §2.3 の基準コマンドが成功し、「現状」を基準の dev バイナリ（`cargo build`）で確かめていること。
- リリースの条件（D5）: このチケットは、NC03・NC04 を含む最初のリリースの前に入れる。着手時に `git tag --contains bd994606`（NC04）と `git tag --contains 5d107cb5`（NC03）が何も出さないことを確かめる。何か出たら S6。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- **S1** 組み込みの 6 型の添字（読み出し・`ref`・スライス・代入）の型付き IR か LLVM IR が変わる。
- **S2** 土台が単純な `Index` の呼び出し、キーの成分がすべて定数の式の `Index` の呼び出し、または土台が単純で右辺とキーの順を観測できない `IndexMut` の呼び出しの型付き IR か LLVM IR が、手順 1 の記録と違う（「単純」「定数の式」「副作用のない式」「右辺とキーの順を観測できない」は設計の定義）。
- **S3** 所有権の検査（`src/ownership.rs`）、最後の使用の解析（`src/last_use.rs`）、コード生成（`src/llvm.rs`）のどれかが、型付き IR の並びと違う順を前提にする。E2E の順の記録が、Rust のテストの期待と違うか、native と WASM、`-O0` と `-O3` で違うときもこれに当たる。順は型付き IR の並びだけで表し、各段で別々に並べ替えない。
- **S4** 生成の束縛のために、A37 の一時値の土台の受理（std の窓の窓 `t[[Slice.range 1 3]][[Slice.at 1]]` など）か、その診断（`E1013` の位置と help）が変わる。
- **S5** `std/`・`examples/`・`tests/`・`_tsuzuri/language-reference/` のプログラムのうち、キーが土台に代入・move・排他借用しないもの（下の `E1012` の形を除く）の診断か出力が変わる。新しい `E1014` の対象は、キーが土台を書き換える形、土台の場所の部分が作った参照が生きている間にキーが元の持ち主に触れる形（組み込みの添字と同じ。現状の表）、`IndexMut` で可変な束縛の参照を土台にし、キーがその束縛を書き換える形（D9）だけである。また、右辺が先になるため、右辺が move する局所をキーが読む `IndexMut` の代入（`b[s.length] = s`、`b[x.n + 1] = x`、キーがタプルのリテラルの `b[(x.n, 0)] = x`）は、組み込みの添字と同じく `E1012`（kind `use_after_move`。位置はキーの中の使用、related `moved_here` は右辺の move）になる。`b[x.n] = x` は今も `E1014`（`borrow_conflict`。キーの場所の借用が生きている間に右辺が `x` を move する。位置は右辺の `x`、related `borrowed_here` はキーの `x.n`）で拒否されており、変わるのは診断のコード、kind、文言、help、位置、related のすべてである（`E1012`・`use_after_move`、`use of moved or partially moved value 'x'`。位置はキーの `x.n`、related `moved_here` は右辺の `x`。組み込みの `xs[x.n] = x` と同じ）。逆に、キーのタプルが move する局所を右辺が読む `b[(x, 0)] = x.n` は、右辺が先になるので、今の `E1012` から受理に変わる。新しい拒否はこの `E1014` と `E1012` の形だけで、見込みは 0 件（未確認。手順 1 で `rg` で確かめる）。
- **S6** 着手時か作業中に、NC03・NC04 を含むリリースが出ている（数え方は A37 の D8）。作業を止めて報告し、そのうえで D5 の代わりの道で続ける。順の修正は文書の規則（左から右）に合わせる不具合の修正として入れ、キーが土台を書き換えるプログラムを新しく `E1014` にすることと、右辺が move する局所をキーが読む `IndexMut` の代入（`b[s.length] = s`）を新しく `E1012`（`use_after_move`）にすることを、`docs/stability.md` の「既知の例外」とリリースノートに書く。この道は D-66 の委任で決まっているので、新たな承認は要らない。
- **S7** GUIDE §13.1 の共通の条件。

## 現状（HEAD `858a949c` で確認）

### 評価順の実装

- 組み込みの添字の読み出し: `src/check.rs:14764` が `TypedExprKind::Index(土台, キー)` を作る。コード生成は土台を先に読み（`read_operand`、`src/llvm.rs:6550`）、次にキーを評価する（6551 行）。場所の形も同じ順である（6712〜6714 行）。所有権の検査は、土台を `Use::Read` で評価して `held` に置き（`src/ownership.rs:15334`〜15335）、その間にキーを評価する（15336 行）。
- 組み込みの添字への代入: コード生成は右辺を先に評価し（`src/llvm.rs:6199`）、次に代入先の場所を求める（6200 行）。場所の中では、土台の場所に共有の守りを置いてキーを評価する（`place()` の腕、`src/ownership.rs:7756`〜7778）。EVL-012 のとおり右辺、土台、キーの順である。
- `Index` の読み出し: 型検査は土台を先に型付けする（`src/check.rs:14749`）が、作る呼び出しは `Call(生成関数, [BorrowOperand(キー), BorrowOperand(土台)])` である（14817〜14836 行。キーは 14825 行、土台は 14830 行）。引数は左から右へ評価する（EVL-002）ので、実行はキー、土台の順になる。所有権の検査も引数の順に評価し、キーを評価する間、土台に守りはない。
- `IndexMut` の代入: 呼び出しは `Call(生成関数, [BorrowOperand(キー), 右辺, 対象])` である（`src/check.rs:14971`〜14986。キーは 14979 行、右辺は 14983 行、対象は 14984 行）。対象は `Borrow(土台, true)`（14926 行）か、土台がもともと `ref mut` ならその値である。実行はキー、右辺、土台の順になる。
- `Index` のメソッドの型は `index :: ref Key -> ref Self -> Output`、`IndexMut` は `index_mut :: ref Key -> Value -> ref mut Self -> unit` で、対象は最後の引数である（D-59）。引数の位置を入れ替えて順を直すことはできない。

### 順の記録（検証済み。HEAD の dev バイナリ、macOS arm64、マニフェストなし（edition 2027））

土台とキーと右辺がそれぞれ記録 `log` に 1 桁を足す（土台 1、キー 2、右辺 3）。記録の値は評価した順に並ぶ。

```text
record Foo { v: i64 }
instance Index<Foo, i64, i64> {
    fn index k self = self.v + deref k
}
instance IndexMut<Foo, i64, i64> {
    fn index_mut _k x self = deref self = Foo { v: x }
}
def mark :: ref mut i64 -> i64 -> i64 -> i64 = \log tag value ->
    deref log = deref log * 10 + tag
    value
def mk :: ref mut i64 -> Foo = \log ->
    deref log = deref log * 10 + 1
    Foo { v: 100 }
def arr :: ref mut i64 -> [i64] = \log ->
    deref log = deref log * 10 + 1
    [10, 20, 30]
def pick { r } :: ref mut { r } Foo -> ref mut i64 -> ref mut { r } Foo = \foo log ->
    deref log = deref log * 10 + 1
    foo
def pick_array { r } :: ref mut { r } [i64] -> ref mut i64 -> ref mut { r } [i64] = \xs log ->
    deref log = deref log * 10 + 1
    xs

export def class_read :: i64 -> i64
fn class_read _n =
    let mut log = 0
    let _v = (mk (ref mut log))[mark (ref mut log) 2 1]
    log
// array_read は (arr (ref mut log))[mark (ref mut log) 2 1]
// class_assign は (pick (ref mut foo) (ref mut log))[mark (ref mut log) 2 0] = mark (ref mut log) 3 7
// array_assign は (deref (pick_array (ref mut xs) (ref mut log)))[mark (ref mut log) 2 0] = mark (ref mut log) 3 7
```

| 関数 | 形 | native `-O0`・`-O3` | WASM `-O0`・`-O3` | 順 | A38 の後 |
| --- | --- | --- | --- | --- | --- |
| `array_read` | 組み込みの読み出し | `12` | `12` | 土台、キー | `12`（変えない） |
| `class_read` | `Index` の読み出し | `21` | `21` | キー、土台 | `12` |
| `array_assign` | 組み込みの代入 | `312` | `312` | 右辺、土台、キー | `312`（変えない） |
| `class_assign` | `IndexMut` の代入 | `231` | `231` | キー、右辺、土台 | `312` |

native は `node scripts/slot.mjs -- target/debug/tsuzuri run -O0 <dir>/Main.tz` と `-O3` で、関数を呼ぶ最後の行を足して確かめた。WASM は `tsuzuri build --target wasm32 -O0`（と `-O3`）で作ったモジュールの export `tz_class_read` などを Node から呼んで確かめた（モジュールは import を持たない）。IO で印字する同じ形のプローブ（`say` が引数を印字する）も、native で同じ順を出した。

### キーが土台に触れる形（検証済み。同じ dev バイナリ、`tsuzuri check --json` と `run -O0`）

| 形 | 組み込みの配列 | `Index`・`IndexMut` |
| --- | --- | --- |
| キーが土台に代入（読み出し）`xs[{ xs = [9]; 0 }]`・`foo[{ foo = Foo { v: 5 }; 0 }]` | `E1014`（kind `borrow_conflict`。位置はキーの中の代入、related `borrowed_here` は土台） | 受理。出力 `5` |
| キーが土台を排他借用 `xs[bump (ref mut xs)]`・`foo[bump (ref mut foo)]` | `E1014`（`borrow_conflict`。位置はキーの `bump (ref mut xs)`） | 受理。出力 `50`（`bump` が `Foo { v: 50 }` を書く） |
| キーが非 Copy の土台を move `xs[consume xs]`・`foo[consume foo]` | `E1014`（`borrow_conflict`。位置はキー）。ほかに要素の move の `E1012` | `E1012`（kind `use_after_move`。位置は土台） |
| 代入の対象のキーが土台に代入 `xs[{ xs = [9]; 0 }] = 7`・`foo[{ foo = Foo { v: 5 }; 0 }] = 7` | `E1014`（`borrow_conflict`） | 受理。出力 `7` |
| キーが土台を読む `xs[xs.length - 1]`・`foo[foo.v]` | 受理。出力 `3` | 受理。出力 `2` |
| `IndexMut` のキーが土台の中の場所そのもの `foo[foo.v] = 3`、計算したキー `foo[foo.v - 1] = 3` | – | 場所のキーは `E1014`（`borrow_conflict`。位置は対象の `foo`。キーは場所のまま貸され、呼び出しの終わりまで生きる）。計算したキーは受理（出力 `3`） |
| 土台が `ref mut` を返す呼び出しで、キーが元の持ち主を読む `(deref (pick (ref mut xs)))[xs.length - 1] = 9`・`(pick (ref mut foo))[foo.v] = 9` | `E1014`（`borrow_conflict`。位置はキーの `xs`。土台の参照が生きている） | `E1014`（`borrow_conflict`。位置は土台の `(ref mut foo)`。キーの借用が先に生きている） |
| 土台が可変な束縛の参照の参照外しで、キーがその束縛に代入する（読み出し）。`let mut r = ref xs` の `(deref r)[{ r = ref ys; 0 }]`・`let mut r = ref a` の `(deref r)[{ r = ref b; 0 }]` | 受理。出力 `1`（代入の前の参照の先 `xs` を読む） | 受理。出力 `7`（代入の後の参照の先 `b` を読む。キーが先なので） |
| 同じ形の代入。`let mut r = ref mut xs` の `(deref r)[{ r = ref mut ys; 0 }] = 5` の後の `xs[0] * 100 + ys[0]` | 受理。出力 `507`（代入の前の参照の先 `xs` に書く） | （A38 の後は D9） |
| 代入の対象が参照の値（`Borrow` で包まない `ref mut` の式。`src/check.rs:14919`）で、キーが参照の先に代入する。`let r = ref mut xs` の `r[{ deref r = [9, 9]; 0 }] = 7`・`let r = ref mut foo` の `r[{ deref r = Foo { v: 5 }; 0 }] = 7`（本文は `deref self = Foo { v: self.v * 100 + x }`） | `E1014`（`borrow_conflict`。位置はキーの中の代入 `deref r = [9, 9]`、related `borrowed_here` は `r`）。読み出しの `r[{ deref r = [9, 9]; 0 }]` も同じ | 受理。出力 `507`（読み出しの形も受理。出力 `5`） |
| 右辺が move する局所をキーが読む代入（土台には触れない）。`s` が `string` の `xs[s.length] = s`・`b[s.length] = s`（`IndexMut<Box, i64, string>`） | `E1012`（kind `use_after_move`。位置はキーの `s`、related `moved_here` は右辺の `s`）。`xs[x.n + 1] = x`（非 Copy のレコード）も同じ | 受理（キーが右辺より先なので）。`b[x.n + 1] = x` も受理 |

### 文書と仕様の規則

- `_tsuzuri/language-reference/values-and-functions/op-and-expressions.md` の「評価順序」（252 行）の表は、代入（264 行。「右辺 `expr`、そのあと代入先」）とスライス（265 行。「配列、開始、終了」）を書く。添字の行はない。
- `docs/language.md:1961` は代入の順（「右辺の評価 → 代入先プレースの特定」。EVL-012 の印は 1957 行）を、3084 行はスライスの順（「元の配列、開始インデックス式、終了インデックス式は左から右」）を書く。
- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md:128` の `Index`・`IndexMut` の行は、順を書かない。
- `spec/rules/EVL.md` は EVL-020 まで、`spec/rules/OWN.md` は OWN-030 まで。添字の順と添字のキーの借用の規則はない。`scripts/check-spec.mjs` は ID の重複と、ファイルの中の昇順だけを検査する（118〜127 行）。
- `docs/stability.md:7` は、同じ edition の中で受理していたプログラムを拒否しないと定める（バグの修正と受理を増やす拡張だけが同じ edition で許される）。13 行は評価順序を edition ごとに保つものに数える（バグの修正を除く）。89 行は 2027 の差について「呼び出しの評価順 … は変更しません」と書く。

### リリース

- `git tag` はコンパイラを同梱する VS Code 拡張の `vsc-v*` だけで、最新は `vsc-v0.1.9`（2026-11-21）。NC04 の `bd994606`（2026-11-25）と NC03 の `5d107cb5`（2026-11-25）は、どちらも `git tag --contains` が何も出さない。`Index`・`IndexMut` はまだどのリリースにも入っていない。

## 変えてはいけない意味

- 組み込みの 6 型の添字の評価順・境界の検査・トラップの位置・型付き IR・LLVM IR（読み出し、`ref xs[i]`、`ref xs[a..b]`、`ref mut xs[a..b]`、edition 2027 の添字代入）。
- `=` の評価順（EVL-012: 右辺が先、その後で代入先）。範囲の検査は右辺の後に行う（`xs[2] = { xs = [9]; 5 }` は範囲外へ書かずトラップする）。
- 複合代入 `b[k] op= e`（NC20）の順。場所の部分（土台、キーの成分）、古い値、右辺の順に評価してから書く。順は型で変えない。
- A37 の土台の下げ方（場所の土台は `Borrow(base, false)`、一時値の土台は事実が立つときだけ `BorrowOperand`、ほかは `E1013`）と、一時値を呼び出しの直後に解放する時点。
- NC04 のキーの規則（キーは `BorrowOperand` で貸し、借用は式の終わりで閉じる。インスタンスの本文がキーの借用を返すと `E1013`）。
- `Index`・`IndexMut` のメソッドの型と、インスタンスの書き方。読み出しは直接の呼び出しのままで、部分適用もクロージャも作らない。
- 値を返す `Index` と土台を借りる `Index` の区別（A37 の `ResolvedIndex::borrows_base`）。

## 仕様

### 1. 評価順の規則（D1。EVL の新しい規則）

- 添字 `b[k]` は、読み出しでも代入の対象でも、次の順に評価する。
  1. 土台 `b`。土台が場所なら、その場所を決める部分（呼び出し、一時値、組み込みの添字の位置と境界の検査、参照の式）を評価する。土台が一時値なら、その値を作る。
  2. キーの成分を左から右へ。1 つのキーなら、そのキーの式。[NC18](NC18-multi-axis-bracket-indexing.md) の多成分のキー `b[c1, …, cn]` なら `c1` から `cn` の順。範囲の成分の中は始点、刻み、終点の順で、`^k` はその被演算子 `k` を評価する（[NC17](NC17-edition-2028-inclusive-range-family.md)）。
  3. 長さの読み出し、境界の検査、`^k` の位置への変換、`Slice.Sel` の形への解決、インスタンスの呼び出し、トラップ。
- 対象: 組み込みの 6 型の `b[k]`・`ref b[k]`・`ref b[a..b]`・`ref mut b[a..b]`、`Index` の読み出し（`b[k]`、[NC16](NC16-index-ref-and-zero-alloc-element-reads.md) の後の `ref b[k]`）、`IndexMut` の代入の対象、`IndexBorrowMut` の `ref mut b[k]`（NC20）、NC18 の `b[c1, …, cn]`。組み込みの添字は今もこの順なので、変わるのは `Index`・`IndexMut` の経路だけである。
- タプルのキー `t[(i, j)]` は 1 つのキーで、タプルの要素は左から右へ評価する（EVL-011）。NC18 の `t[i, j]` と同じ順になる。

### 2. キーの共有の守り（D2。OWN の新しい規則）

- キーの成分を評価する間、土台の場所に共有の守りを置く（`exclusive_slice` と同じ `self.held`）。キーは土台を読める。土台へ代入する・move する・排他借用するキーは `E1014`（kind `borrow_conflict`）。キーの成分がすべて定数の式なら、キーは土台に触れられないので守りを省く（設計）。
- 土台が参照の値のとき（`let r = ref mut foo` の `r[k] = v`、自動の参照外しの読み出し `r[k]`）は、守りを参照の指す場所に置く。キーの中の `deref r = …` は `E1014` になる（組み込みの添字と同じ。「現状」の表）。
- これは今の組み込みの添字と同じ規則である（「現状」の表）。診断の文言、help、関連する位置（`borrowed_here` は土台）は、組み込みの添字の `E1014` と同じにする（D8）。
- キーの貸し方は変えない（NC04: キーは `BorrowOperand` で貸し、借用は式の終わりで閉じる）。そのため、キーが土台の中の場所そのもの（`foo[foo.v] = 3` の `foo.v`）なら、場所の借用が呼び出しの終わりまで生き、`IndexMut` の対象の排他の借用と重なって今と同じく `E1014` になる。計算したキー（`foo[foo.v - 1] = 3`）は一時値を貸すので通る。読み出しの `foo[foo.v]` は共有どうしなので通る。
- 土台の場所の部分が作った参照（`pick (ref mut foo)` の結果）は、キーを評価する間も生きている。そのため、キーが元の持ち主 `foo` に触れると、ふつうの借用の規則で `E1014` になる。組み込みの添字の `(deref (pick (ref mut xs)))[xs.length - 1] = 9` と同じである。
- 一時値の土台は名前を持たないので、キーから書き換えられない。一時値が借用を持つ（`Tensor.view (ref t)` の結果など）なら、キーがその借用先を書き換えると、ふつうの借用の規則で `E1014` になる。
- 排他の借用は、キーを評価した後に取る。`IndexMut` の書き込みの対象（`Borrow(土台, true)`）と、NC20 の `IndexBorrowMut` の `ref mut t[k]` は、共有の守りの下でキーを評価し、守りを外してから排他の loan を取る（2 段。`exclusive_slice` と同じ）。
- 土台が可変な束縛の参照の参照外し（`let mut r` の `deref r` と、その上のフィールドの経路）で、キーがその束縛 `r` に代入するとき（D9）: 読み出しは、組み込みの添字と同じく、代入の前の参照の先を読む（「現状」の表の `1`）。`IndexMut` の代入では、キーは `r` に代入・move・排他借用できない（`E1014`、kind `borrow_conflict`）。排他の参照は複製できず、キーの前に排他の借用を取るとキーが土台を読めなくなるためである。組み込みの添字への代入はこの形を受理する（「現状」の表の `507`）ので、ここだけクラスの道が組み込みより厳しい。キーで `r` を変えたいときは、キーを `let` で先に求める。

### 3. 代入と複合代入（D3）

- 添字への代入 `b[k] = v`: EVL-012 のとおり、まず右辺 `v` を評価する。次に仕様 1 の順で対象（土台、キーの成分）を評価する。最後に境界を検査するか、インスタンスを呼んで書く。右辺は土台を書き換えてよい（`=` の今の規則）。範囲の検査とインスタンスの呼び出しは右辺の後なので、右辺が土台を置き換えても古い土台へ書かない。右辺が先なので、右辺が move する局所をキーが読む形（`b[s.length] = s`）は、組み込みの添字と同じく `E1012`（kind `use_after_move`）になる（今の `IndexMut` はキーが先なので受理する。「現状」の表）。
- 複合代入 `b[k] op= e`（NC20）: NC20 の EVL の規則のまま。場所の部分（土台、キーの成分を左から右）、古い値（読むとき）、右辺の順に評価してから書く。右辺を評価する間は対象に共有の守りがある（NC20 の OWN の規則）。A38 はこの規則を変えない。
- 順は型で変えない。組み込みの添字と `Index`・`IndexMut` で同じである。

### 4. edition・native と WASM・IR（D4・D6）

- edition は使わない。全 edition（2026・2027）で同じ。組み込みの添字の順と `=` の EVL-012 は変えず、変わるのはリリース前の `Index`・`IndexMut` の経路だけである。
- 実行時の順は native と WASM で同じ。並べ替えは型付き IR の並びで表すので、所有権の検査、最後の使用の解析、コード生成が同じ順を見る。
- 土台が単純なとき（設計の定義）、`Index` の呼び出しの型付き IR と LLVM IR は変えない。キーの成分がすべて定数の式のときも変えない。`IndexMut` は、土台が単純で、右辺とキーの順を観測できない（設計の定義）なら変えない。どの場合も、変わるのは所有権の守りだけである（キーの成分がすべて定数の式なら守りも置かない）。
- 新しい診断のコードと kind はない（D8）。

### 5. 例（実装後の意味）

```text
record Foo { v: i64 }
instance Index<Foo, i64, i64> {
    fn index k self = self.v + deref k
}
def say :: i64 -> i64 = \n ->
    do! IO.writeln (to_string n)
    n
def mk :: unit -> Foo = \() ->
    do! IO.writeln "base"
    Foo { v: 100 }
let v = (mk ())[say 1]      // "base" の後に "1"。今は "1" の後に "base"
let mut foo = Foo { v: 1 }
let a = foo[foo.v]          // 受理。キーは土台を読める
let b = foo[{ foo = Foo { v: 5 }; 0 }]   // E1014（borrow_conflict）。今は受理されて 5
```

## 設計

### 単純な土台

- **単純な土台**: 局所 `x`、局所を根とするレコードのフィールドの経路 `x.f.g`、可変でない束縛（`let r`、可変でない仮引数）の参照の参照外し `deref r`（自動の参照外し `r[k]` を含む）とそのフィールドの経路。どれも評価に副作用もトラップもなく、キーが評価を変えられない（キーが `x` に代入すれば守りで `E1014`、`let r` には代入できない）。組み込みの添字の段（`ts[i]`）は境界の検査でトラップしうるので、単純に含めない。可変な束縛の参照の参照外し（`let mut r` の `deref r`）は、キーが `r` に代入すると土台の場所が変わるので、単純に含めない（D9）。
- **定数の式**: リテラル（単項の `-` を付けたものを含む）と、リテラルだけからなるタプルと配列のリテラル。何も読まず、副作用もトラップもない。
- **副作用のない式**: 定数の式、局所の読み出し、局所を根とするフィールドの経路の読み出し、それらだけからなるタプルと配列のリテラル。呼び出し、組み込みの添字、ブロック、代入を含まない。`.length`（型付き IR の `Length`・`StringLength`。`src/check.rs:16404`〜16410）や演算子も含まない。
- 2 つの式の順は、一方が定数の式なら観測できない。一方が読む局所を他方が書きうるので、副作用のない式どうしでない限り、局所の読み出しだけの式でも順は観測できる（`foo[i] = { i = 2; 7 }` の右辺は `i` を書く）。副作用のない式どうしでも、move があれば順は観測できる。`IndexMut` の右辺は `Value` の引数として値で渡るので、型が Copy でない局所を読む右辺はその局所を move する。キーがその局所を読むと、右辺が先なら `E1012` になる（`b[x.n] = x`。`x` は非 Copy のレコード。組み込みの `xs[x.n] = x` は HEAD で `E1012`、位置はキーの `x.n`、related `moved_here` は右辺の `x`）。キーも、値で渡る部分は move する。キーのタプルや配列のリテラルの要素（NC18 の 2 成分以上のキーでは各成分）は値でキーに入るので、型が Copy でない局所を読む要素はその局所を move する。右辺がその局所を読むと、キーが先なら `E1012` になる（`b[(x, 0)] = x.n`。右辺が先なら受理する）。1 成分のキーの局所やフィールドの経路そのもの（`b[x] = …`、`b[x.n] = …`）は場所のまま貸すので move しない。
- **右辺が move する局所**: 右辺が副作用のない式のとき、その中の局所の読み出しとフィールドの経路の読み出し（タプルと配列のリテラルの中のものを含む）のうち、読む値の型が Copy でないものの根の局所。`x.n`（`i64`）は `x` を move しない。
- **キーが move する局所**: キーの成分が副作用のない式のとき、成分の中のタプルと配列のリテラルの要素（NC18 の 2 成分以上のキーでは各成分。成分はキーのタプルか配列に入る）の局所の読み出しとフィールドの経路の読み出しのうち、読む値の型が Copy でないものの根の局所。1 成分のキーの局所やフィールドの経路そのものは場所のまま貸すので数えない。
- **右辺とキーの順を観測できない**: キーの成分がすべて定数の式か右辺が定数の式であること、または、右辺とキーの成分がすべて副作用のない式で、右辺が move する局所（を根とする場所）をキーのどの成分も読まず、キーが move する局所（を根とする場所）を右辺が読まないこと。`b[i] = s`（`s` は非 Copy、キーは `i`）はこれに当たる。`b[x.n] = x`、`b[(x.n, 0)] = x`、`b[(x, 0)] = x.n` は当たらない。
- 単純な土台なら、キーの前に評価しても後に評価しても観測できないので、型付き IR を変えない（D6）。所有権の検査だけが、キーの前に土台の場所を求めて守りを置く。

### 並べ替えの方法（D6。既定案）

並べ替えが要るときは、型付き IR に生成の束縛を足して、順を IR の並びで表す。生成の束縛の名前は `$` で始め（ソースが書けない名前）、`Provenance::Generated` にする（`src/check.rs:5368` の `NARROWED_PREFIX` と同じ慣行）。束縛の位置（span）は元の式の位置にして、診断の位置を変えない。

| 土台の形 | 例 | 読み出しの下げ方 |
| --- | --- | --- |
| 単純な場所 | `t`、`m.w`、`deref r`、`r.w` | 変えない |
| 一時値 | `(mk n)`、窓の窓の 1 段目 `t[[Slice.range 1 3]]`、ブロック | `{ let $b = <土台>; <呼び出し> }`。`$b` は A37 の一時値の土台として扱う（下の項） |
| 局所でない参照の参照外し | `(pick (ref mut t))`（自動の参照外し） | `{ let $r = <参照の式>; <呼び出し（土台は deref $r）> }` |
| 可変な束縛の参照の参照外し | `let mut r` の `deref r`・`r.w` | 読み出しは `{ let $p = Borrow(<土台>, false); <呼び出し（土台は deref $p）> }`（共有の再借用。キーは `r` に代入でき、土台は代入の前の参照の先になる）。`IndexMut` は並べ替えず、守りを `r` にも置く（D9） |
| ほかの場所（組み込みの添字の段、上の形の上のフィールドの経路） | `ts[g ()]`、`ts[i]`、`(pick (ref mut h)).w` | 土台の場所の中で、呼び出し・一時値・組み込みの添字（境界の検査を含む）を含む最も外側の部分を、生成の束縛に 1 回だけ評価する。束縛は場所の共有の参照（型付き IR の `Borrow(<部分>, false)`）か、参照の式の値。土台はその束縛を根とする単純な場所になる |

- キーの成分がすべて定数の式なら、土台が単純でなくても並べ替えず（観測できない）、守りも置かない（定数の式は土台に触れられない）。
- `IndexMut`: 右辺を最初に、次に土台の部分、最後にキーを評価する。並べ替えの規則は次のとおり。
  - 土台が単純でなく、キーが定数の式でないとき（土台の部分を束縛するとき）: 土台の部分の前に、右辺も `let $v = <右辺>` に束縛する（右辺が定数の式なら束縛しない）。土台の部分が書くものを右辺が読みうるためである。
  - 土台が単純か、キーが定数の式のとき（土台の部分を束縛しないとき）: 順が問題になるのは右辺とキーだけで、右辺とキーの順を観測できない（上の定義）なら右辺を束縛しない。そうでなければ右辺を `$v` に束縛する。`b[s.length] = s` はキーが副作用のない式でないので、`b[x.n] = x` と `b[(x.n, 0)] = x` は右辺が move する局所 `x` をキーが読むので、`$v` に束縛する。右辺の move の後でキーが読むので `E1012`（`use_after_move`）になる（組み込みの添字と同じ。S5）。`b[(x, 0)] = x.n` はキーが move する局所 `x` を右辺が読むので同じく `$v` に束縛し、右辺が先に `x.n` を読むので受理する。
  - `$v` は呼び出しの `Value` の引数へ move する。土台の部分の束縛は読み出しと同じ表に従う。ただし土台を排他借用する `Borrow(土台, true)` は、キーの後の呼び出しの引数のまま作る（キーの前に排他の参照を束縛しない。キーが土台を読めなくなるため）。
- 読み出しは直接の呼び出しのままにし、`x |> Index.index (ref k)` のようなパイプライン、部分適用、クロージャには下げない（パイプラインは `src/llvm.rs:6283` の道でクロージャを作りうる）。
- **一時値の土台と A37**: `$b` は場所だが、A37 の一時値の規則を保つ。土台を借りる `Index`（`borrows_base`）でも `$b` は `Borrow($b, false)` にせず `BorrowOperand($b)` で渡し、所有権の検査の A37 の判定（引数 1 が一時値のとき、事実 `returns_contents` が立てば受理、立たなければ `E1013`）を `$b` にも当てる。そのため、生成の束縛が一時値の土台を表すことを `Local` に印として持たせる（名前の接頭辞か専用の印。`Local::hides_narrowed`（`src/check.rs:5391`）と同じ形）。`$b` はブロックの終わり、つまり呼び出しの直後に解放され、A37 の `release_operands` の時点と同じになる。`E1013` の位置は元の土台の式の位置にする（S4）。
- 印の使い方を広げる（A37 の段 2 の印）: A37 は土台を借りる `Index` の生成関数に印を付ける。A38 は、この印を `Index`・`IndexMut` のすべての生成関数に広げ、役割（読み出しか書き込みか）と土台の引数の位置（最後）を持たせる。特殊化で複製される関数にも引き継ぐ。名前の接頭辞 `$instance.` では判定しない。NC20 の `IndexBorrowMut` は、同じ印に役割を足す。

### 段ごとの変更

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1 | `src/polymorph.rs`（`register_index_instance` 2495 行、`resolve_index` 2758 行）と `src/check.rs` の `CheckedFunction`（5178 行）か `FunctionOrigin`（4892 行） | A37 の生成関数の印を、`Index`・`IndexMut` のすべての生成関数に広げる（役割と土台の引数の位置）。生成物は変えない |
| 2 | `src/ownership.rs` の `eval_call`（14153 行） | callee が印を持つとき、キーの引数を評価する前に、土台の引数の中の場所を `place()` で求め、`Use::Borrow` で確かめて共有の守りを `self.held` に置く。キーの引数を評価し、守りを外す。その後は今と同じ順で残りの引数を評価する（`IndexMut` の対象の `Borrow(土台, true)` はここで排他の loan になる）。キーの成分がすべて定数の式なら守りを置かない。守りの場所を `place()` で求めるのは、土台が単純な場所か、生成の束縛を根とする場所か、D9 の可変な束縛の参照の参照外しのときだけである（段 3・4 の後か、土台がもともとその形のとき。どれも呼び出しも組み込みの添字も含まないので、`place()` を 2 回呼んでも副作用がない）。場所でない土台の引数（一時値・呼び出し。場所でない `BorrowOperand` を含む）には守りを置かない。対象の引数が参照の値のとき（`src/check.rs:14919` の道。`Borrow` で包まない `ref mut` の式。並べ替えた後の生成の束縛 `$r` を含む）は、守りをその参照の指す場所（`place(Dereference(引数))`。`exclusive_places`（7930 行）と同じ求め方）に置く。引数が可変な束縛（`let mut r`）なら、束縛 `r` そのものにも置く（D9）。手本は `exclusive_slice`（7895 行）と `place()` の組み込みの添字の腕（7756〜7778 行） |
| 3 | `src/check.rs` の `ExprKind::Index`（14748〜14837 行） | 土台とキーを型付けした後、土台が単純でなく、キーが定数の式でないときだけ、設計の表のとおり生成の束縛で土台の部分を先に評価するブロックに下げる。呼び出しの引数の並び（キー、土台）は変えない |
| 4 | `src/check.rs` の `ExprKind::Assign` の `IndexMut` の道（14895〜14987 行） | 設計の `IndexMut` の規則のとおり、右辺と土台の部分を束縛する。束縛の順は右辺、土台の部分。呼び出しの引数の並び（キー、右辺、対象）は変えない。可変な束縛の参照の参照外しの土台では、段 2 の守りを束縛 `r` にも置く（D9） |
| 5 | `src/ownership.rs` の A37 の一時値の判定 | 生成の束縛が一時値の土台を表すとき、`BorrowOperand($b)` を一時値の土台として扱う（設計の「一時値の土台と A37」） |

- `src/llvm.rs`・`src/last_use.rs`・`src/constants.rs` は変えない見込み。順は型付き IR の並びで決まり、生成の束縛はふつうの `let` として下がる。変える必要が出たら S3 として止める。

### 生成 IR

- 単純な土台の `Index` の呼び出し、キーの成分がすべて定数の式の `Index` の呼び出し、土台が単純で右辺とキーの順を観測できない `IndexMut` の呼び出し: 変更なし（S2）。
- 並べ替える呼び出し: 生成の束縛の `alloca` と `store`・`load` が増える。`IndexMut` の右辺を `$v` に束縛する呼び出し（`b[s.length] = s` のようにキーが副作用のない式でないもの、`b[x.n] = x`・`b[(x, 0)] = x.n` のように一方が move する局所を他方が読むもの）もこれに入る。右辺が move する局所をキーが読むと、束縛の後のキーの使用が `E1012`（`use_after_move`）になる（S5）。`-O3` では mem2reg が消す見込み（性能は主張しない）。一時値の土台の解放は、呼び出しの直後のまま（ブロックの終わり）。

## 実装手順

1. **変更**: なし（`target/scratch/a38/` に記録する）。**内容**: 「現状」の 2 つの表を dev バイナリで確かめる。`tests/index_class.rs`・`tests/matrix_views.rs`・`tests/tensor.rs` の受理する例と、`tests/fixtures/tensor`・`tests/fixtures/matrix_view`・`tests/fixtures/matrix` を `tsuzuri build --emit llvm`（native と `--target wasm32`、`-O0`）で IR にして保存する。`rg -n '\]\s*=[^=]' std examples tests/fixtures _tsuzuri/language-reference` などで、`Index`・`IndexMut` を使い、キーに呼び出しかブロックを持つ式と、右辺が move する局所をキーが読む `IndexMut` の代入と、キーのタプルか配列のリテラルが move する局所を右辺が読む `IndexMut` の代入（今の `E1012` が受理に変わる。`b[(x, 0)] = x.n`）を探し、S5 の対象がないことを控える。**確認**: 表と同じ出力と診断。
2. **変更**: `tests/index_class.rs`。**内容**: テスト計画の Rust テストを足す。順のテストと `index_key_cannot_write_its_base` は今は失敗する。`index_key_may_read_its_base` は通る。**確認**: `node scripts/slot.mjs -- cargo nextest run --locked --test index_class` で、足した順と拒否のテストだけが失敗する。
3. **変更**: `src/polymorph.rs`、`src/check.rs`（段 1）、`src/ownership.rs`（段 2）。**内容**: 印を広げ、キーの守りを入れる。型付き IR はまだ変えない。**確認**: T0。`index_key_cannot_write_its_base` が通る。手順 1 の IR と `diff` してすべて一致する（守りは IR を変えない）。
4. **変更**: `src/check.rs`（段 3・4）、`src/ownership.rs`（段 5）。**内容**: 生成の束縛による並べ替え。**確認**: T0。順のテストが通る。手順 1 の IR と `diff` し、違うのが並べ替えた呼び出しだけであることを確かめる（S1・S2）。A37 の一時値の土台のテスト（`tests/index_class.rs` の窓の窓と、再現の `E1013`）が今のまま通る（S4）。`node scripts/slot.mjs -- cargo nextest run --locked --test index_class --test matrix_views --test tensor --test tensor_ops --test types_ownership --test borrowed_records --test mutable_slices`、T1。
5. **変更**: `spec/rules/EVL.md`・`spec/rules/OWN.md`、`tests/conformance/EVL/`・`tests/conformance/OWN/`、`tests/features.mjs`・`tests/fixtures/index_order/Main.tz`。**内容**: 仕様の規則 2 つ（番号は実装の時点の EVL と OWN の次の空き）、適合性テスト、E2E の suite `index_order`。**確認**: `cargo build`、`node scripts/conformance.mjs target/debug/tsuzuri --rule <EVL の新しい ID>` と OWN の新しい ID、`node scripts/check-spec.mjs --write`、`node scripts/check-spec.mjs`、`node tests/features.mjs target/debug/tsuzuri index_order`。
6. **変更**: 文書（ドキュメントの節）。**内容**: 評価順序の表、`docs/language.md`、`type-classes.md`、`docs/architecture.md`、規則の印。`node scripts/gen-docs.mjs` で `conformance.md` の生成部分を更新する。**確認**: T2（テスト計画の検証コマンド）。

## テスト計画

### Rust テスト（`tests/index_class.rs`）

拒否はコード生成の前に決まるので、`analyze`（edition 2026）の 1 回で native と WASM を覆う。順は `tsuzuri run --no-cache -O0`・`-O3` の出力で確かめる（同じファイルの `user_defined_index_and_index_mut_work_with_temporary_and_borrowed_keys` の書き方）。受理の例は `emits`（native と WASM の IR、決定性、宣言の重複なし）でも確かめる。

- `index_evaluates_the_base_before_the_key`: 「現状」の順の記録のプログラム（`say` で印字する形）で、`Index` の読み出しが `base` の後にキーを印字する。一時値の土台（`(mk ())[say 1]`）、局所でない参照の土台（`(pick (ref mut foo))[say 1]`）、窓の窓（`t[[Slice.range 0 2]][[say_slice 1]]` のように 2 段目のキーが副作用を持つ形）、組み込みの読み出し（`(arr ())[say 1]`。変わらない）。
- `index_mut_evaluates_the_right_side_first`: `IndexMut` の `(pick (ref mut foo))[say 1] = say 2` が右辺、土台、キーの順に印字し、その後でインスタンスの本文が書く。組み込みの `(deref (pick_array (ref mut xs)))[say 1] = say 2`（変わらない）。キーか右辺が定数の式の形（`foo[0] = say 2`、`foo[say 1] = 7`）も同じ出力の順になる。右辺がキーの読む局所を書く形（`let mut i = 0` の後の `foo[i] = { i = 2; 7 }`）は、組み込みの `xs[i] = { i = 2; 7 }`（HEAD で確認。添字 2 に書き、出力 `107`）と同じく、右辺の後の `i` を使う。右辺が move する局所をキーが読む形（`b[s.length] = s`。`IndexMut<Box, i64, string>`、今は受理）は `E1012`（kind `use_after_move`）になり、組み込みの `xs[s.length] = s`（HEAD で確認。位置はキーの `s`、related `moved_here` は右辺の `s`）と同じコード・kind・位置の種類になる（`analyze`）。`b[x.n] = x`（`IndexMut<PairBox, i64, Pair>`、`record Pair { n: i64, tag: string }` の `x`。キーも右辺も副作用のない式で、右辺の `x` は非 Copy。今は、キーの場所の借用 `x.n` が生きている間に右辺が `x` を move するので `E1014`。位置は右辺の `x`、related `borrowed_here` はキーの `x.n`。HEAD で確認）は `E1012`（`use_after_move`）に変わり、組み込みの `xs[x.n] = x`（HEAD で確認。位置はキーの `x.n`、related `moved_here` は右辺の `x`）と同じになる。キーがタプルのリテラルの `b[(x.n, 0)] = x`（`IndexMut<PairBox, (i64 * i64), Pair>`。今は受理。HEAD で確認）も `E1012` になる。キーのタプルが move する局所を右辺が読む `b[(x, 0)] = x.n`（`IndexMut<IBox, (Pair * i64), i64>`。今は右辺の `x.n` の位置で `E1012`。HEAD で確認）は、右辺が先になるので受理に変わる。
- `index_key_reassigning_a_mutable_base_reference`（D9）: `let mut r = ref a` の `(deref r)[{ r = ref b; 0 }]` が受理され、代入の前の参照の先を読む（出力 `1`。組み込みと同じ）。`let mut r = ref mut foo` の `(deref r)[{ r = ref mut other; 0 }] = 5` は `E1014`（`borrow_conflict`）。`(deref r)[r.v - 1] = 5`（キーが `r` を読むだけ）は受理。
- `index_key_cannot_write_its_base`: 「現状」の表の `Index`・`IndexMut` の 4 つの形（キーの代入、キーの排他借用、キーの move、代入の対象のキーの代入）がそれぞれ `E1014`（kind `borrow_conflict`）。位置はキーの中のアクセス、related `borrowed_here` は土台。組み込みの同じ形の診断（コード、kind、位置の種類）と一致する。代入の対象が参照の値の形（`let r = ref mut foo` の後の `r[{ deref r = Foo { v: 5 }; 0 }] = 7`）と、その読み出し（`r[{ deref r = Foo { v: 5 }; 0 }]`）も `E1014`（`borrow_conflict`。位置はキーの中の代入、related `borrowed_here` は `r`。組み込みの `let r = ref mut xs` の `r[{ deref r = [9, 9]; 0 }] = 7` と同じ）。
- `index_key_may_read_its_base`（`emits`）: `foo[foo.v]`、`foo[foo.v - 1] = 3`（`IndexMut`。キーが土台を読んだ後で排他の loan を取る）、局所の `Tensor` の `t[[Tensor.count (Tensor.view (ref t)) - 1]]`（HEAD で受理、出力 `3`）のようにキーが土台を読む形。`foo[foo.v] = 3`（キーが土台の中の場所そのもの）は今と同じ `E1014` のまま（仕様 2）。
- `index_temporary_base_is_dropped_on_early_exit_in_key`（`emits`）: 一時値の土台の後でキーが例外を投げる形（`try (mk n)[@checked (k + huge)] with | e -> e`）が受理される。解放は E2E の `live == 0` で確かめる。
- `index_simple_base_keeps_the_ir`: 設計の生成 IR の項で「変更なし」とした呼び出し（単純な土台の `Index`、キーが定数の式の `Index`、土台が単純で右辺とキーの順を観測できない `IndexMut`。右辺が非 Copy の局所でキーがそれを読まない `b[i] = s` を含む）の型付き IR に生成の束縛（名前が `$` で始まる `Provenance::Generated` の局所）がないこと。
- A37 の一時値の土台のテスト（窓の窓の受理と、再現の `E1013` の位置と help）は変えずに通す（S4）。

### 適合性テスト（`tests/conformance/`）

- `tests/conformance/EVL/EVL-0xx-index-evaluates-base-before-key.tz`（番号は実装の時点の次の空き）: 「現状」の順の記録の 4 つの関数を `test` の `Test.equal` で確かめる（`expect: tests-pass 4`）。`tests-pass` は native と WASM の `-O0`・`-O3` の 4 通りで走る（`scripts/conformance/run.mjs:135` の `ALL_RUNS`）。
- `tests/conformance/OWN/OWN-0xx-index-key-cannot-change-base.tz`: `Index` のキーが土台に代入する形が `check-error E1014 line=N kind=borrow_conflict`。

### E2E（`tests/features.mjs` の新しい suite `index_order`、native と WASM の `-O0`／`-O3`、`live == 0`）

`tests/fixtures/index_order/Main.tz` の export 関数が順の記録を整数で返し、`tests/features.mjs` の `index_order` の表と比べる（既存の suite の書き方。native はすべての確保を追跡し、呼び出しごとに `live == 0` を確かめる）。

- `class_read` → `12`、`array_read` → `12`、`class_assign` → `312`、`array_assign` → `312`（「現状」の表の A38 の後の列）。
- 一時値の土台が非 Copy の値（文字列を持つレコード）で、キーが `@checked` の溢れで例外を投げる形 → `try` が `Error` を返し、`live == 0`（一時値の土台が 1 回だけ解放される）。
- `IndexMut` で右辺が非 Copy の値（`$v` に束縛される）で、キーが例外を投げる形 → `live == 0`。
- 窓の窓（`t[[…]][[…]]`）で 2 段目のキーが記録を書く形 → 順は 1 段目の土台、1 段目のキー、2 段目のキー。
- NC18 が入った後は、NC18 が `multi_axis_key_evaluates_base_then_components` をこの suite に足す（NC18 のテスト計画）。

### 既存テストへの影響

- 期待値の変更はない見込み（S5）。HEAD の `Index`・`IndexMut` を使うテスト（`tests/index_class.rs`、`tests/matrix_views.rs`、`tests/tensor.rs`、`tests/api_diff.rs`、`tests/kg.rs`）のキーは、リテラル・局所・`Slice` の関数の呼び出しで、土台を書き換えないと見ている（全件は未確認。手順 1 で確かめる）。

### 性能

実行時の振る舞いは順の変更だけで、土台が単純な呼び出しの IR は変わらない。性能は主張しない。

### 検証コマンド

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`node scripts/slot.mjs -- cargo nextest run --locked --test index_class --test matrix_views --test tensor`。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`。
- T2: T1、`cargo build`、`node tests/features.mjs target/debug/tsuzuri index_order`、同じく `tensor`・`matrix_view`・`matrix`・`borrowed_records`・`mutable_slices`・`in_place_writes`（suite は 1 回に 1 つ）、`node tests/control.mjs target/debug/tsuzuri`、`TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/debug/tsuzuri`（README の「所有権・借用・region」の行）、`node scripts/conformance.mjs target/debug/tsuzuri --area EVL`、同じく `--area OWN`、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`、`node scripts/check-docs.mjs <変更したページ>`。

## ドキュメント

- `_tsuzuri/language-reference/values-and-functions/op-and-expressions.md` の「評価順序」の表（252 行）: 代入の行（264 行）の後に、1 列目が「添字 `xs[i]`・`t[k]`・`t[i, j]`（組み込みとクラス）」、2 列目が「土台、キーの成分を左から右、その後に境界の検査と読み出し。`=` の代入の対象も同じで、右辺の後（複合代入は NC20 の行）」の行を足す。1 列目のセルの先頭に EVL の新しい規則の印（`<!-- rule: EVL-0xx -->`）を置く。表の後に、添字のキーは土台を読めるが、土台への代入・move・排他借用は `E1014` であることを 1 段落で書き、OWN の新しい規則の印を置く。スライスの行（265 行）は変えない。
- `docs/language.md`: 1961 行の代入の順の文に、添字への代入は右辺の後に土台、キーの順で対象を決めることを足す。「式と評価順序」（3010 行）に、添字は土台、キーの成分を左から右の順で、型によらないことを 1 文で書く。
- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md:128` の `Index`・`IndexMut` の行: `x[k]` は組み込みの添字と同じ順（土台、キー）で評価し、`x[k] = v` は右辺の後であることを足す。
- `docs/architecture.md` の所有権検査の事実の段落（A37 が土台を借りる `Index` の下げ方を書く段落）: 生成関数の印、キーの守り、生成の束縛による並べ替えと、単純な土台では IR を変えないことを書く。
- `spec/rules/EVL.md` と `spec/rules/OWN.md`: 次の 2 行を、実装の時点の次の空きの番号で足す（D7）。
  - EVL: `An index expression MUST evaluate its base before its key components and the components left to right, before any bounds check or instance call; as the target of a plain assignment `=` it does so after the right-hand side.`（文書は `op-and-expressions.md`）
  - OWN: `The key components of an index expression MAY read its base but MUST NOT assign, move or exclusively borrow it (E1014).`（文書は `op-and-expressions.md`）
- `_tsuzuri/language-reference/compiler/conformance.md` の生成部分（`<!-- generated:spec-coverage -->`）: `node scripts/gen-docs.mjs` で作る。
- D5 の代わりの道のときだけ: `docs/stability.md` の「既知の例外」（119 行）に、キーが土台を書き換える `Index`・`IndexMut` のプログラムを新しく `E1014` にしたことと、右辺が move する局所をキーが読む `IndexMut` の代入（`b[s.length] = s`）を新しく `E1012`（`use_after_move`）にしたことを 1 項で書く。リリースノートの Compatibility にも同じ 2 つを書く（PR の説明に文案を置く）。
- README: 書いてあること（特徴の一覧・検証手順）は変わらないので変更しない。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/values-and-functions/op-and-expressions.md _tsuzuri/language-reference/types-and-type-inference/type-classes.md _tsuzuri/language-reference/compiler/conformance.md` を通す。

## 受け入れ条件

- [ ] 「現状」の順の記録が、`class_read` `12`、`array_read` `12`、`class_assign` `312`、`array_assign` `312` になる（native と WASM の `-O0`・`-O3`）。
- [ ] `Index`・`IndexMut` のキーが土台に代入・move・排他借用する形が `E1014`（`borrow_conflict`）で、組み込みの添字と同じ位置の種類になる（土台が参照の値の `r[{ deref r = …; 0 }] = 7` と、その読み出しを含む）。キーが土台を読む形は受理のまま。
- [ ] 右辺が move する局所をキーが読む `IndexMut` の代入（`b[s.length] = s`）が、組み込みの `xs[s.length] = s` と同じ `E1012`（`use_after_move`）と位置の種類になる。
- [ ] 組み込みの 6 型の添字の IR と、設計の生成 IR の項で「変更なし」とした `Index`・`IndexMut` の呼び出しの IR が、手順 1 の記録と一致する（native と WASM）。
- [ ] 可変な束縛の参照を土台にし、キーがその束縛に代入する形が D9 のとおり（読み出しは代入の前の参照の先を読み、`IndexMut` は `E1014`）。
- [ ] A37 の一時値の土台の受理と診断（窓の窓、`E1013` の位置と help）が変わらない。
- [ ] E2E の `index_order` が native と WASM の `-O0`／`-O3` で通り、例外で抜ける形でも `live == 0`。
- [ ] EVL と OWN の新しい規則（実装の時点の次の空きの番号）と、その適合性テストがある。`check-spec.mjs --require-markers` が通る。
- [ ] NC03・NC04 を含む最初のリリースの前に入っている（D5）。先にリリースが出ていれば、`docs/stability.md` の既知の例外とリリースノートの文案がある。
- [ ] T2 が通る。ドキュメントの節のページが更新されている。

## 落とし穴

- 順を所有権の検査やコード生成の中だけで入れ替えない。最後の使用の解析（A17）、範囲の証明（F12）、定数の評価も順を見るので、型付き IR の並びで表す（S3）。
- 引数の位置を入れ替えて順を直さない。メソッドの型は対象が最後（D-59）で、インスタンスの本文の引数名もこの順に従う。
- 一時値の土台を生成の束縛にしたとき、`Borrow($b, false)` にしない。A37 の一時値の規則が外れ、窓の窓が `$b` を借りたままブロックの外へ出て、別の診断になる（S4）。`BorrowOperand($b)` と一時値の印で、A37 の判定を通す。
- `IndexMut` の排他の参照をキーの前に束縛しない。キーが土台を読む形（`foo[foo.v - 1] = 3`）が `E1014` になる。排他の loan はキーの後（仕様 2）。
- 「単純」を「局所の読み出しだけ」と取り違えない。局所の読み出しだけの式でも、相手の式がその局所を書けば順が観測できる（`foo[i] = { i = 2; 7 }`）。並べ替えを省けるのは、右辺とキーの順を観測できないときだけ（設計）。副作用のない式どうしでも、一方が move する局所を他方が読めば順が観測できる。`b[x.n] = x` は組み込みと同じ `E1012` にする。右辺を束縛しないと、キーの場所の借用が生きたまま右辺が `x` を move して今と同じ `E1014` になり、組み込みとコードが食い違う。キーがタプルのリテラルの `b[(x.n, 0)] = x` は受理してしまい、`b[(x, 0)] = x.n` は右辺が先なら通るのに `E1012` になる。
- `let mut r` の `deref r` を単純な土台に含めない。キーが `r` に代入すると土台の場所が変わる（「現状」の表。組み込みは代入の前の参照の先を使う）。
- 組み込みの添字の段（`ts[i]`）を単純な土台に含めない。境界の検査のトラップの位置が順に入る（EVL-019）。
- キーの守りは共有なので、キーの中の土台の読み出しは通る。守りを排他にしない。
- 土台の場所を所有権の検査で 2 回求める（守りのためと、引数の評価のため）。守りのために土台の場所を求めるのは、土台が単純な場所か生成の束縛を根とする場所（と D9 の可変な束縛の参照の参照外し）のときだけである（`place()` を 2 回呼んでも副作用がない）。土台が単純でなく並べ替えなかったとき（キーの成分がすべて定数の式のとき）は守りを置かない。組み込みの添字の段や呼び出しを含む土台に `place()` を 2 回呼ぶと、その中のキーの move や呼び出しを 2 回数える（`place()` の組み込みの添字の腕は `self.eval(index, Use::Consume, live)`。`src/ownership.rs:7771`）。今は通る `ts[idx s][0]`（`idx :: own string -> i64`。HEAD で受理、出力 `20`）が偽の `E1012` になり、S5 に当たる。
- 生成の束縛の名前を、利用者の名前と衝突させない（`$` で始める）。名前の使用の索引は `Provenance::User` の名前だけを記録し（`note_name`、`src/check.rs:13117`）、診断の `scope` の一覧も `$` で始まる名前と利用者のものでない局所を外す（13495〜13497 行）。生成の束縛はこの扱いに乗るように作る。

## 対象外

- 組み込みの添字の順の変更（今のまま）。
- 複合代入の順（NC20 の規則のまま）と、`IndexBorrowMut` の実装（NC20 の Phase 3。順は仕様 2 に従う）。
- 多成分のキーの構文（NC18。成分の順は仕様 1 に従う）。範囲の成分の構文（NC17）。
- 演算子・呼び出しなど、添字以外の評価順（EVL-001〜EVL-020 のまま）。
- 新しい診断のコード・kind・文言。

## 決定事項

### D1: 評価順の規則（決定）

- 決定: 仕様 1。添字の評価順を、組み込みの添字とすべての `Index` の経路で 1 つの規則にする。先に土台を評価し（場所の部分か一時値）、次にキーの成分を左から右へ評価する。長さ・境界・`Slice.Sel` の解決、インスタンスの呼び出し、トラップはその後に行う。読み出しにも代入の対象にも当てる。対象は組み込みの 6 型の添字、`Index`、`IndexMut`、`IndexBorrowMut`、NC18 の `b[c1, …, cn]`。
- 理由: 同じ `x[k]` の形で、型によって副作用の順が変わると、利用者も AI も順を予測できない。配列の `xs[i][j]` とテンソルの `t[i, j]` で順が違うのは、D-66 の「1 つの記号は 1 つの意味」に反する。組み込みの順はリリース済みで文書にもあり（スライスは「配列、開始、終了」、代入は EVL-012）、`docs/stability.md` は評価順を edition ごとに保つ（13 行、89 行）。変えてよいのはリリース前の `Index` の側だけである。
- 採らなかった案: 今のまま（`Index` だけキーが先）、すべてをキーが先にそろえる（リリース済みの組み込みの順を変える）、edition で切り替える（D4）。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D2: キーを評価する間、土台に共有の守りを置く（決定）

- 決定: 仕様 2。キーは土台を読めるが、代入・move・排他借用は `E1014`（kind `borrow_conflict`）。排他の借用（`IndexMut` の書き込み、`IndexBorrowMut` の `ref mut t[k]`）は、キーを評価した後に取る（共有の守りの後に排他の借用を取る 2 段）。
- 理由: 組み込みの添字と `exclusive_slice` が今使っている規則なので、所有権の検査に新しい解析は要らない。キーが土台を読む書き方（`xs[xs.length - 1] = v`、`foo[foo.v]`）はそのまま通る。守りは今は実装にしかないので、OWN の規則にしてすべての経路を適合性テストで固定する。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D3: 代入は右辺が先、複合代入は NC20 の規則（決定）

- 決定: 仕様 3。`b[k] = v` は EVL-012 のとおり右辺、土台、キーの成分の順で、最後に境界を検査するかインスタンスを呼んで書く。`b[k] op= e` は NC20 の EVL の規則のまま（場所の部分、古い値、右辺、書く）。順は型で変えない。右辺が move する局所をキーが読む `IndexMut` の代入（`b[s.length] = s`）は、組み込みの添字と同じく `E1012`（`use_after_move`）になる。
- 理由: 代入で右辺を先にするのは Rust・Python・C++17 と同じ（リポジトリの外の知識。C#・Java・F# の順は未確認）。境界の検査が右辺の後なので、右辺が土台を置き換えても範囲外へ書かない。複合代入で場所を先にすると、`p = p op e` の右辺が最初の `p` を読む位置と同じになり、場所の部分は 1 回だけ評価される（Python の拡張代入と同じ。記憶による）。NC20 の `AddAssign` の道と `IndexBorrowMut` も、「場所の部分 → 守り → 排他の借用」に合う。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D4: edition を使わない（決定）

- 決定: 全 edition で同じ規則にする。`=` の EVL-012 と組み込みの添字の順は変えず、変わるのは `Index`・`IndexMut` の経路だけである。
- 理由: `Index`・`IndexMut` を入れた NC04（`bd994606`）はどのリリースにも入っていない（`git tag --contains` が何も出さない。「現状」のリリース）。リリース前の機能の修正なので、`docs/stability.md:7` の「受理していたプログラムを拒否しない」に当たらない。std は全 edition で 1 つのコピーで（`docs/stability.md:67`）、edition で順を分けると std の本文の順も分かれる。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D5: NC03・NC04 を含む最初のリリースの条件と、先に出たときの道（決定）

- 決定: A38 は NC03・NC04 を含む最初のリリースの条件にする（[NC16](NC16-index-ref-and-zero-alloc-element-reads.md) と同じ）。推奨順は [A37](A37-index-base-place-borrow.md) → A38 → NC16。NC18 と NC20 の Phase 3 は A38 の後に入れる。リリースの数え方は A37 の D8（NC17 の未決の質問 3 の決定。利用者向けの定義は NC17 の Phase 2 が `docs/stability.md` に書く）。A38 より先にそのリリースが出たら、停止条件 S6 にする。そのうえで、順の修正は文書の規則（左から右）に合わせる不具合の修正として入れてよい。ただし、キーが土台を書き換えるプログラムを新しく `E1014` にすることと、右辺が move する局所をキーが読む `IndexMut` の代入（`b[s.length] = s`）を新しく `E1012`（`use_after_move`）にすることは、`docs/stability.md` の「既知の例外」とリリースノートに書く。
- 理由: リリースの後では、キーが土台を書き換える健全なプログラムを新しく拒否することになる。`docs/stability.md:7` は新しい拒否を新しい edition に限る。リリースの前なら費用は 0 で、NC16 と同じ窓に入る。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D6: IR を変えない範囲と並べ替えの方法（既定案）

- 決定: 土台が単純な場所（局所とそのフィールドの経路、可変でない束縛の参照の参照外し）なら、型付き IR と LLVM IR を変えず、所有権の守りだけを足す。評価を並べ替えるのは、土台が単純でなく（呼び出し、一時値、組み込みの添字、可変な束縛の参照）、キーが定数の式でないときだけにする。`IndexMut` の右辺は、設計の規則（右辺とキーの順を観測できないなら束縛しない）で先に束縛する。並べ替えは型付き IR の生成の束縛で表す。読み出しは直接の呼び出しのままにし、部分適用もクロージャも作らない。
- 理由: 順を IR の並びで表せば、所有権の検査、最後の使用の解析、範囲の証明、コード生成が同じ順を見る（S3 のリスクを構造でなくす）。並べ替えを要する形だけを変えれば、ほとんどの呼び出し（局所の土台、リテラルや局所のキー）の生成物は 1 バイトも変わらない。生成の束縛は NC20 の仕様 7 の `$b`（土台の場所の部分）と同じ形で、NC20 はこの方法をそのまま使う。
- 採らなかった案: 所有権の検査とコード生成の中だけで引数の評価順を入れ替える（`src/last_use.rs` などほかの段と順が食い違いうる）、パイプライン `x |> Index.index (ref k)` に下げる（クロージャを作りうる）、新しい型付き IR の節点（すべての段に腕を足す）。
- 状態: 既定案（実装者はこの案に従う）。

### D7: 仕様の規則を 2 つ足す（決定）

- 決定: EVL と OWN に 1 つずつ、実装の時点の次の空きの番号で足す（ドキュメントの節の英文）。EVL: 添字は土台をキーの成分より先に評価し、成分は左から右へ評価する（組み込みとクラスの添字、読み出しと代入の対象のすべて）。OWN: 添字のキーは土台を読めるが、代入・move・排他借用はできない。起票の文には番号を書かない。NC20 の規則と番号が重なったら、後に入る方が付け直す（`scripts/check-spec.mjs` が重複を報告する）。
- 理由: 規則の番号はリポジトリの慣行で実装の時点に決める（W02 の `CTL-027` と同じ）。`check-spec.mjs` は重複と昇順だけを検査するので、欠番は許される。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D8: 診断は新しいコード・kind・文言を足さない（既定案）

- 決定: キーが土台を書き換える形は、組み込みの添字と同じ `E1014`（kind `borrow_conflict`）、同じ文言と help、同じ関連する位置（`borrowed_here` は土台）にする。
- 理由: 同じ規則を同じ診断で伝える。kind・コードを足さないので、D-55 の予約を使わず、`diagnostics.md` の生成部分も変わらない。
- 状態: 既定案。

### D9: 可変な束縛の参照を土台にし、キーがその束縛に代入する形（既定案）

- 決定: 仕様 2 の最後の項。読み出しは、共有の再借用を生成の束縛にして、組み込みの添字と同じく代入の前の参照の先を読む。`IndexMut` の代入では、所有権の守りを束縛 `r` にも置き、キーが `r` に代入・move・排他借用すると `E1014`（`borrow_conflict`）にする。キーは `r` と土台を読める。
- 理由: 組み込みの添字への代入は、代入の前の参照の先に書く（「現状」の表の `507`）。クラスの道で同じ意味にするには、キーの前に土台の参照を取っておく必要がある。排他の参照は複製できず、move すると後の `r` の使用が壊れ、キーの前に排他の再借用を取るとキーが土台を読めなくなる（`(deref r)[r.v - 1] = 5` が `E1014` になる）。キーが `r` を書き換える形は、土台を読む形よりずっと少ないので、拒否する側を選ぶ。拒否は、意味の違う受理より安全で、後から受理を増やせる。
- 採らなかった案: 組み込みの添字への代入も拒否する（リリース済みの受理を減らす）、`IndexMut` だけキーの後の参照の先に書く（組み込みと実行時の意味が分かれる）、キーの前に排他の再借用を取る（キーが土台を読めなくなる）。
- 状態: 既定案（実装者はこの案に従う）。仕様 1 の規則（土台が先）には反しない。組み込みより厳しいのはこの形だけで、OWN の新しい規則（土台そのものの代入・move・排他借用）には含めない。

## 他チケットとの関係

- [A37](A37-index-base-place-borrow.md)（依存）: 生成関数の印（段 2）と、土台を借りる `Index` の下げ方（場所は `Borrow(base, false)`、一時値は事実が立つときだけ `BorrowOperand`）。A38 は印を広げ、一時値の土台の判定を生成の束縛にも当てる。A37 は評価順を変えず、順を固定するテストも足さない（A37 の D9）。
- [NC16](NC16-index-ref-and-zero-alloc-element-reads.md)（後続）: `ref` を必須にし、要素の読み出しを確保 0 回にする。A38 の順を保つ。順を固定するテストは A38 が足す。
- [NC17](NC17-edition-2028-inclusive-range-family.md): 範囲の成分の中の順（始点、刻み、終点）と `^k` の被演算子は、仕様 1 の「キーの成分」に含まれる。
- [NC18](NC18-multi-axis-bracket-indexing.md)（後続）: 多成分のキー `t[c1, …, cn]` の成分を左から右へ評価し、キーを評価する間は土台に共有の守りがある。`t[i, j]` と `t[(i, j)]` は同じ順なので IR も同じ。NC18 はテスト `multi_axis_key_evaluates_base_then_components`（native と WASM、`-O0` と `-O3`）を足す。
- [NC20](NC20-compound-assignment-and-mutable-index-views.md)（Phase 3 は A38 の後）: `IndexMut` の `t[k] = v` の順は A38 の規則（右辺、土台、キーの成分）。複合代入の順と右辺の守りは NC20 の規則のまま。`IndexBorrowMut` は、土台の場所の部分、共有の守りの下のキー、排他の loan の順で、A38 の印と生成の束縛を使う。単独の `ref mut t[k]` は A38 の印と段 2 の守りを使う。NC20 の §7 の `let $k` の守りは NC20 が同じ処理で置く。NC20 の EVL・OWN の規則の番号は、A38 の規則の後の空きになりうる。
- [A39](A39-field-exclusive-borrow.md)・[A40](A40-reference-operands.md): 独立。NC20 の Phase 3 のフィールドの土台（`model.w[1, 2] += 1.0`）は A38 と A39 の両方を使う。
