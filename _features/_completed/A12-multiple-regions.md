# A12: レコード内の独立した複数 region と region 付き関数値型

| 項目 | 内容 |
| --- | --- |
| ID | A12 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | A09 |
| 後続 | A13, C08 |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D10・D13 は、2026-10-03 に利用者から「A15 / A12 の実装を完遂して。複数フェーズある場合には、すべてのフェーズを完了させること」と依頼され、承認として扱った。D10 は見直し提案（64 へ下げず、parser の 128 を正式な上限にする）を選び、D13 は前置量化を名前付き関数の引数の型に限って実装した（GUIDE D-33、実装と検証） |
| 改善する劣位 | Rust 比: 借用で表せるデータ構造の制限（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 手本にする既存実装 | region の宣言検査: `src/regions.rs` の `labels`・`validate_modules`・`contract`（`TypeExpr` を worklist で走査し再帰しない形）。返却元の検査: `src/ownership.rs` の `check_body`（外部 loan と `allowed_roots`）。直接完全適用の入力選択: `eval_composed` の `E::Call` 腕（`region_sources`）。場所の印: `Place.fields` の `ELEMENT`・`PAYLOAD`。合流: `Checker::merge` と `src/ownership_control.rs` の `eval_match` |
| 主な影響ファイル | `src/check.rs`, `src/regions.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `tests/borrowed_records.rs`, `tests/fixtures/borrowed_records/Main.tz`, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/lifetimes.md`, `README.md`, `_docs/feature-status.md`, `_features/README.md`。変更なしを確認するだけ: `src/syntax.rs`, `src/parser.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/closures.rs`, `src/polymorph.rs`, `src/llvm.rs`, `tests/types_ownership.rs` |

## 目的

Rust の `struct Pair<'a, 'b> { left: &'a T, right: &'b U }` に相当する、寿命の異なる借用を一つのレコードへまとめる契約を書けるようにする。
A09 はレコード値全体に一つの region を割り当てるため、長寿命と短寿命の借用を同じ view に入れると、どのフィールドを読んでも短い方の寿命に縛られる。
Phase 1 はレコードに独立した複数 region を宣言でき、フィールドの読み出し、`def` の返却契約、直接の完全適用で region ごとに寿命を追跡する。
region は A09 と同じく検査専用で、評価順序・生成 IR・公開 ABI を変えない。

Phase 2 は `for<'a> fn(&'a T) -> &'a T` に相当する region 付き関数値型で、この文書では設計方針だけを書く（D13、要承認）。
実装者は Phase 1 だけを実装する。Phase 2 は人間が求め、D13 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- A09 が done であること。確認: `grep -n "A09\|A12" _features/README.md` で A09 の行の状態欄が `done`、A12 が `todo`。
- Phase 1 は承認なしで着手できる。D10 が承認されるまで、関数あたりの region 上限（parser の 128）を変えない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（A09 サンプルの IR、テスト件数、stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 既存テストの期待値（診断コード、IR、E2E の値）を変える必要がある。特に `tests/borrowed_records.rs` の既存 5 テスト、
  `tests/types_ownership.rs`、`tests/control.rs`・`tests/docs.rs`・`tests/host_imports.rs` の region ケース、
  `tests/features.mjs` の `borrowed_records` suite の既存 10 ケース。
- region なし・単一 region のレコードや関数で、受理・拒否・診断メッセージ・生成 IR のどれかが変わる（D12）。
- region を `Type`、単相化キー、LLVM 型、公開 ABI に入れる必要が出た（D11）。
- `src/parser.rs` を変える必要が出た（D1。`Pair {r r}` の許可や新しい region 構文を含む）。
- 「例」や「テスト計画」で受理と書いたケースが通らず、直すには `let mut` の値やループの固定点（`loop_summary`）で region を追跡する必要がある（D6 の範囲外）。
- 返却検査で slot を決めるのに loan の祖先（`Loan.parents`）をたどる必要が出た（D7 の場所の印で slot が決まらない loan が見つかった）。
- 所有権検査が `src/closures.rs`・`src/polymorph.rs` の作る関数（`region_sources: None`）にも契約を必要とすることが分かった。
- 手順 1 の stack-depth テスト 6 件のどれかが失敗し、処理を helper へ移しても直らない。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 構文はすでに複数 region を読む。
  - `Parser::region_list` は `{r s}`（カンマ区切り可）を読み、名前が小文字 ASCII で互いに異なることを検査する
    （違反は `E1013 "region names must be distinct lowercase identifiers"`、129 個目は `E1017 "too many region parameters"`）。
    record 宣言と `def` の region は `RecordDecl.regions`・`FunctionDecl.regions`（`Vec<Ident>`）に入る。
  - 型の後ろの `{r s}` は `Parser::finish_type_primary`（`region_suffix` と `region_list_ahead` で判定）が
    `TypeExprKind::Regions(Box<TypeExpr>, Box<[Ident]>)` にする。使用位置の名前の重複も `region_list` が拒否する。
  - `ref {r} T` は `Parser::reference_type` が読み、region が 2 個以上なら `E1013 "a reference has exactly one region"`。
  - `src/formatter.rs` と `src/docgen.rs`（`region_text`）は region の並びを `Vec` のまま出力する。
- 宣言検査は `src/regions.rs`（`check` の子モジュール。`TypeExpr` を worklist で走査し、再帰しない）。
  - `validate_modules` が record の `regions.len() > 1` を
    `E1013 "records currently have one shared region; use separate views for independent regions"`（位置は record 名）で拒否する。
  - `labels` は一つの型式に現れる region 名の集合を返す。`Regions` の名前が 1 個でなければ
    `"each borrowed value has one named region; split values with independent regions"`、集合が 2 個以上なら
    `"one parameter or result cannot mix distinct named regions; use separate parameters"`、借用を持たない型なら
    `"a named region requires a reference or an aggregate carrying borrows"`、未宣言なら
    `"undeclared region '{r}'; add it after the declaration name"`、高階関数型の内部なら
    `"named regions inside higher-order function types are not supported; use a named function with all parameters bound"`（すべて E1013）。
  - `check_used` は `"unused region '{r}'; remove it or use it in a borrowed type"`。`reject_local` は alias・const・union payload・
    class method の region を `"named regions belong in function signatures or record declarations; let local borrows be inferred"` で拒否する。
  - `contract` は `def` の結果 region と同じ名前の region を持つ引数の添字集合（`Option<BTreeSet<usize>>`）を返す。
    結果の型全体が `Regions` でなければ `"annotate the whole borrowed result with one region, for example 'View {r}' or 'ref {r} T'"`、
    対応する入力がなければ `"result region '{r}' has no matching input; bind all function parameters and name an input region"`。
- 型は region を持たない。`resolve_type` は `Regions(inner, _)` を `inner` として解決する。`Type::carries_loans` は参照と関数型で真、
  型変数で偽。`CheckedRecord` は `name`・`visibility`・`origin`・`parameters`・`fields: Vec<(String, Type)>`・`span`・`size`・`recursive` だけを持ち、
  `check_modules_collect` の record ループの `Ok(CheckedRecord { .. })` 一か所で作る（record id は `record_declarations` の添字）。
  `regions::validate_modules` は全 record の構築後に呼ばれる。
- `CheckedFunction.region_sources: Option<BTreeSet<usize>>` は `check_modules_collect` が `regions::contract` の結果を入れる。
  `src/closures.rs`（4 か所）と `src/polymorph.rs`（2 か所）が作る関数は `None`。
- 所有権（`src/ownership.rs`）はレコード値の loan をフィールドで区別しない。
  - `Value { loans: BTreeSet<usize>, closed_result: [bool; 2] }`。`eval_value` の `E::Record(_) | E::RecordUpdate { .. }` 腕は全フィールドの loan の和を作る。
    フィールドの読み出しは、場所なら `read_places`、右辺値なら `eval_value` の `E::Field` 腕が、元の値の loan 全体を結果に渡す。
  - `check_body` は借用を持つ引数ごとに外部 loan を一つ作る（場所は `Place { root: usize::MAX - parameter.id, fields: vec![] }`）。
    結果の loan の root が外部でなければ `"cannot return a reference to a local value"`、`region_sources` の引数の root でなければ
    `"returned borrow does not match the declared result region; return a borrow from an input with that region"`（どちらも E1013）。
  - `eval_composed` の `E::Call` 腕は、既知の利用者関数（`E::Function(FunctionRef::User(id))`・`E::GenericFunction(id, _)`）への引数数ちょうどの
    呼び出しでだけ、`region_sources` にある引数の loan を結果へ入れる。関数値・部分適用は全引数の loan を保持する（GUIDE D-28）。
  - 合流は `Checker::merge`（local ごとの loan の和）。ループは `src/ownership_control.rs` の `check_loop` が `loop_summary`
    （local ごとの loan の場所の集合）を比べて固定点を取る。match と `if` の結果は `eval_match` の `result.loans.extend(value.loans)`。
  - `Place.fields` には実フィールド添字のほか、`ELEMENT`（`usize::MAX`）と `PAYLOAD`（`usize::MAX - 1`）の印がある。
- テスト: `tests/borrowed_records.rs` の 5 テスト（`parses_named_regions_without_conflicting_with_type_parameters` ほか）と、
  `tests/fixtures/borrowed_records/Main.tz` を使う `tests/features.mjs` の `borrowed_records` suite（10 ケース。`named_view`・`named_reference` を含む）。
- 文書: `docs/language.md` の「名前付き Region」、`_docs/language-reference/lifetimes.md` の「現在の制限」表、`README.md` の所有権節、
  `_docs/feature-status.md` の A12 行が「レコード内の独立した複数 region は未対応」と書く。

### 再現（検証済み）

各ケースを別ディレクトリの `Main.tz` に置き、`target/release/tsuzuri run <dir>` で確認した。

A09 の単一 region 契約は `42` を出す。

```tsuzuri
record View<'a> {r} { value: ref {r} 'a }

def view {r s} :: ref {r} 'a -> ref {s} string -> View<'a> {r} = \value unused ->
    assert (unused.length > 0)
    View { value: value }

let outer = 42
let selected = { let inner = "short"; view (ref outer) (ref inner) }
deref selected.value
```

二つ目の record region は宣言だけで拒否される。

```tsuzuri
record Pair {r s} { left: ref {r} string, right: ref {s} string }

let a = "x"
let b = "yz"
let p = Pair { left: ref a, right: ref b }
p.left.length + p.right.length
```

```text
Main.tz:1:8: error[E1013]: records currently have one shared region; use separate views for independent regions
```

region のない二参照レコードでは、フィールドを一つ読んでも両方の loan が残る。

```tsuzuri
record TwoView { left: ref string, right: ref string }

let long = "abcdefg"
let kept = { let short = "xy"; let view = TwoView { left: ref long, right: ref short }; view.left }
kept.length
```

```text
Main.tz:4:89: error[E1013]: borrowed value does not live long enough to leave this block
```

単一 region のレコードへ region を二つ与えると次のとおり。

```tsuzuri
record View {r} { text: ref {r} string }

def size {r s} :: View {r s} -> i64 = \view -> view.text.length

let text = "abc"
size (View { text: ref text })
```

```text
Main.tz:3:19: error[E1013]: each borrowed value has one named region; split values with independent regions
```

Phase 1 の後、1 件目は `42`、3 件目と 4 件目は同じ E1013・同じメッセージのまま（D12）。2 件目は受理され `3` を出す。

## 仕様

### 前提とする他チケットのインターフェース

なし。A09（done）の構文・`src/regions.rs`・`CheckedFunction.region_sources` の上に作る。A13 と C08 は A12 の出力を使う（「設計」の「他チケットへの提供インターフェース」）。

### 構文

Phase 1 は parser を変えない（D1）。現在の parser が読む形に意味を与える。

```text
record-decl ::= "record" Name type-params? region-list? "{" field ("," field)* "}"
def-decl    ::= "def" name region-list? "::" type
region-list ::= "{" region (","? region)* "}"         -- 名前は互いに異なる（parser が検査）
region      ::= [a-z][a-z0-9_]*
applied     ::= type-primary region-list               -- 使用位置の region 適用
reference   ::= ("ref" | "&") "mut"? ("{" region "}")? type-primary
```

- record 宣言では型パラメーター `<'a, 'b>` の後、フィールドの `{` の前に region を並べる。例: `record Pair<'a, 'b> {r s} { left: ref {r} 'a, right: ref {s} 'b }`。
- 使用位置 `Pair<i64, string> {r s}` の名前は record の宣言順に対応する（1 番目の名前が宣言の 1 番目の region）。
  宣言側と使用側の名前は無関係で、使用側の名前は囲む `def`・record の宣言から取る。
- 同じ region を二つの位置へ渡す `Pair {r r}` は parser が `E1013 "region names must be distinct lowercase identifiers"` で拒否する（D1）。

### 型規則

1. record `R` の region 数を `n(R)` とする。宣言がない・一つなら `n(R) = 1`（A09 の共有 region）。値の region slot は `n(R)` 個。
   多 region record でない型の値は slot を一つ持つ。
2. 使用位置（D2）:
   - `n(R) >= 2` の `R<..> {r1 .. rk}` は `k = n(R)` でなければ E1013（個数）。`ri` は宣言の i 番目の region に対応する。
   - `n(R) = 1` なら従来どおり `k = 1`。`k >= 2` は既存メッセージの E1013。
   - region を省略した `R<..>` は全 slot を一つの交差寿命として扱い、名前付き結果 region の入力元にならない（A09 と同じ）。
3. フィールド（D3）: フィールドの region 集合は、その型式に現れる region 名の集合である。
   - `n(R) = 1`: A09 と同じ。名前のない借用も唯一の region に属する。
   - `n(R) >= 2`: 借用を直接格納する型（`Type::contains_stored_reference` が真）のフィールドは region を一つ以上書く（ないと E1013）。
     関数型・型変数のように region を書けないフィールドは全 region に属する（具体化で借用型になっても保守的）。
   - 一つのフィールドが複数の region を持てるのは、フィールド型全体が多 region record の直接適用（`pair: Pair {b a}`）のときだけで、
     内側の i 番目の slot が外側の `names[i]` の slot に対応する。タプル・配列・参照の中で region を混ぜると E1013。
   - 宣言した各 region は一つ以上のフィールドで使う（既存の未使用検査）。
4. 引数・結果（D4）: 型全体が多 region record の直接適用 `R<..> {r1 .. rn}` のときだけ、一つの引数・結果に異なる region を混ぜられる。
   `ref Pair {r s}`、`[Pair {r s}]`、`(ref {r} T, ref {s} U)`、直接適用の型引数の中の region（`Pair<ref {q} i64, i64> {r s}`）は E1013。
5. 関数契約（D5）: 結果の各 slot `j` について、同じ名前の region を持つ（引数添字、引数の slot）の組の集合を入力元とする。
   入力元がない slot は既存の E1013。同じ名前が複数の入力にあれば、それらの交差寿命（A09 と同じ）。
6. region の間に outlives 制約・部分型の宣言はない（D15）。region は型の同一性・単一化・単相化キーに入らない（D11）。
   呼び出しでは値の slot を位置で対応させ、寿命は loan で検査する。

### 評価順序・所有権・借用

- 評価順序、move・Copy、drop、生成コードは変えない。region は `resolve_type` で消え、LLVM へ渡らない。
- 所有権検査は多 region record 型の値について slot ごとの loan 集合を持つ（D6）。精度を持つのは次の経路だけ。
  - record リテラル・更新（`E::Record`・`E::RecordUpdate`）: フィールドの loan を、そのフィールドの slot へ入れる。更新は元の値の slot を保って追加する。
  - フィールドの読み出し（場所と右辺値）: 読んだフィールドの slot の loan だけを持つ。入れ子の record では対応を合成する。
  - 不変の local・不変の引数に保存した値、`match` と `if` の各腕の結果（slot ごとの和）。
  - `def` の引数（slot ごとの外部 loan、D7）と結果の検査、既知の関数への引数数ちょうどの直接呼び出し（入力元の slot の loan だけを結果の slot へ入れる）。
- 次は全 slot の和として扱う（A09 と同じ保守的な扱い）: `let mut` の local と可変引数、分岐・ループで異なる値が合流した local、
  関数値・部分適用を経た結果（D8）、クロージャー捕捉、配列・リスト・タプル・union payload への格納、外部の場所（`deref` した引数参照）からの読み出し。
- Task への送信・捕捉は従来どおり E1013。排他借用フィールド（`record fields cannot store mutable references; use a shared borrow`）も従来どおり E1013（A13 の範囲）。

### 数値・トラップ・native と WASM の差

なし。生成 IR は region を消した同じプログラムと一致する（テスト `multiple_regions_do_not_change_generated_ir`（新規））。WASM import も増えない。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1013（新規メッセージ） | `n(R) >= 2` の record へ `n(R)` 個でない region を適用 | `the record declares {n} regions; write exactly {n} region names in declaration order` | その `Regions` 型式 |
| E1013（新規メッセージ） | `n(R) >= 2` の record で、借用を直接格納するフィールドが region を書かない | `field '{field}' stores a borrow without a region; name one of the record's regions in its type, for example 'ref {r} T'` | フィールドの型式 |
| E1013（新規メッセージ） | フィールドの型が直接適用以外の形で異なる region を混ぜる | `one field cannot mix distinct named regions; give each field one region or use a record type with several regions` | フィールドの型式（入れ子の直接適用ならその型式） |
| E1017（新規メッセージ） | record の region が 16 個を超える | `too many regions in record '{name}'; declare at most 16 and share one region among borrows with the same lifetime` | 17 番目の region 名 |
| E1013（既存） | 引数・結果の型が直接適用以外の形で異なる region を混ぜる | `one parameter or result cannot mix distinct named regions; use separate parameters` | 型式全体（入れ子の直接適用ならその型式） |
| E1013（既存） | `n(R) = 1` の record・参照以外へ region を二つ以上 | `each borrowed value has one named region; split values with independent regions` | その `Regions` 型式 |
| E1013（既存） | 結果 slot の region を持つ入力がない | `result region '{r}' has no matching input; bind all function parameters and name an input region` | 結果の型式 |
| E1013（既存） | 本体の結果 slot の loan が、その slot の入力元以外を指す | `returned borrow does not match the declared result region; return a borrow from an input with that region` | 本体 |
| E1013（既存） | 未宣言・未使用の region、借用を持たない型への region | 「現状」の既存メッセージ | 既存どおり |
| E1013（既存・parser） | `Pair {r r}` など名前の重複 | `region names must be distinct lowercase identifiers` | 重複した名前 |
| E1013（既存） | 短い寿命の slot の loan がブロック・所有者より長生きする | 既存の寿命メッセージ（例: `borrowed value does not live long enough to leave this block`） | 既存どおり |
| E1017（既存・parser） | 宣言の region が 128 個を超える | `too many region parameters` | 129 番目の名前 |

`records currently have one shared region; use separate views for independent regions` は出さなくなる。E1012・E1014 の条件とメッセージは変えない。

### 資源上限

- record あたりの region は 16 個まで（D9）。17 個目で E1017。`RegionMask`（新規、`u16`）の 1 bit が 1 region に対応する。
- `def` あたりの region は parser の現行上限 128（`MAX_NESTING`）のまま。64 へ下げる変更は D10（要承認）。
- 所有権検査に新しい反復上限は足さない。ループの固定点は既存の `check_loop` の上限（超過は既存の E1017）のまま。

### 例

新構文（実装後に有効。未検証）。受理され、`7` を出す。

```tsuzuri
record Pair {r s} { left: ref {r} string, right: ref {s} string }
record Swapped {a b} { pair: Pair {b a} }

def make_pair {r s} :: ref {r} string -> ref {s} string -> Pair {r s} = \left right -> Pair { left: left, right: right }
def left_of {r s} :: Pair {r s} -> ref {r} string = \pair -> pair.left
def swap {r s} :: Pair {r s} -> Pair {s r} = \pair -> Pair { left: pair.right, right: pair.left }

let long = "abcdefg"
let a = { let short = "xy"; left_of (make_pair (ref long) (ref short)) }
let b = { let short = "xy"; let pair = Pair { left: ref long, right: ref short }; pair.left }
let c = { let short = "xy"; let swapped = Swapped { pair: Pair { left: ref long, right: ref short } }; swapped.pair.left }
let d = { let short = "xy"; (swap (make_pair (ref short) (ref long))).left }
(a.length + b.length + c.length + d.length) / 4
```

新構文（実装後に有効。未検証）。上の宣言に次のどれか一つを足すと拒否される。

| 追加する宣言・行 | 結果 |
| --- | --- |
| `let e = { let short = "xy"; let pair = Pair { left: ref long, right: ref short }; pair.right }` | E1013（`right` は `short` の寿命） |
| `let f = { let short = "xy"; left_of (make_pair (ref short) (ref long)) }` | E1013（結果は slot `r` = `short`） |
| `let g = { let short = "xy"; let mut pair = Pair { left: ref long, right: ref short }; pair.left }` | E1013（`let mut` は全 slot の和、D6） |
| `let get = left_of` と `let h = { let short = "xy"; get (make_pair (ref long) (ref short)) }` | E1013（関数値は保守的、D8） |
| `def bad {r s} :: Pair {r s} -> ref {r} string = \pair -> pair.right` | E1013（返却元の不一致） |
| `def bad {r s} :: ref {r} string -> ref {s} string -> Pair {r s} = \left right -> Pair { left: right, right: left }` | E1013（返却元の不一致） |
| `def bad {r} :: Pair {r} -> i64 = \pair -> pair.left.length` | E1013（個数） |
| `def bad {r s} :: ref Pair {r s} -> i64 = \pair -> 0` | E1013（引数内の混在） |
| `record Bad {r s} { left: ref {r} string, right: ref {s} string, other: ref string }` | E1013（名前のない借用フィールド） |
| `record Bad {r s} { both: (ref {r} string, ref {s} string) }` | E1013（フィールド内の混在） |

### Phase 2（設計方針）

承認（D13）前は着手しない。方針だけを記録する。

- 関数型の前置量化 `{r} ref {r} T -> ref {r} T`（rank-1）で、関数値の呼び出し結果へ契約を反映する。量化のない関数値型は従来どおり全入力を保持する。
- `Type::Function` に量化 region の対応を持たせるため、関数型の同一性・単一化・`llvm.rs` の `canonical_type` に影響し、D11 と GUIDE D-28 の
  「関数値は全入力を保持」を変える。parser では型の先頭の `{` を新しく読むため、`Parser::type_primary` へ分岐を足さず helper に分ける。

## 設計

### データ構造

```rust
// src/check.rs
/// A12: one bit per declared region of a record; bit i is the i-th declared region.
pub(crate) type RegionMask = u16; // （新規）
pub(crate) const MAX_RECORD_REGIONS: usize = 16; // （新規）
/// A12: per region slot of a function result, the (parameter index, parameter slot) pairs it may borrow from.
pub(crate) type RegionSources = Vec<BTreeSet<(usize, usize)>>; // （新規）

pub struct CheckedRecord {
    // 既存フィールドは変えない
    /// Declared region count. 0 and 1 both mean one shared region (A09).
    pub(crate) region_count: usize, // （新規）
    /// Only when region_count >= 2: per field in declaration order, the record-region mask of each
    /// region slot of the field type. A single entry applies to every slot of the field type.
    pub(crate) field_regions: Vec<Vec<RegionMask>>, // （新規）
}

pub struct CheckedFunction {
    pub(crate) region_sources: Option<RegionSources>, // 型を変更（旧 Option<BTreeSet<usize>>）
    // 他は変えない
}
```

```rust
// src/regions.rs
pub(super) fn field_regions(record: &RecordDecl) -> Result<(usize, Vec<Vec<RegionMask>>), Diagnostic>; // （新規）E1017 もここ
fn record_regions(expression: &TypeExpr, module: &str, names: &Names, types: TypeContext<'_>) -> usize; // （新規）
fn slot_labels(expression: &TypeExpr, declared: &BTreeSet<String>, module: &str, names: &Names,
               types: TypeContext<'_>, field: bool) -> Result<Vec<BTreeSet<String>>, Diagnostic>; // （新規）
fn labels(/* 既存の引数 */, field: bool) -> Result<BTreeSet<String>, Diagnostic>; // 引数 field を追加
pub(super) fn contract(/* 既存の引数 */) -> Result<Option<RegionSources>, Diagnostic>; // 戻り値を変更
```

```rust
// src/ownership.rs
#[derive(Clone, Default)]
struct Value {
    loans: BTreeSet<usize>,
    /// A12: loans per region slot of a record with several regions; None means every loan may belong
    /// to every slot. Read only through Checker::slots, which validates it.
    regions: Option<Box<[BTreeSet<usize>]>>, // （新規）
    closed_result: [bool; 2],
}

/// A12: Place.fields marker of a parameter's external loan for one region slot.
const REGION_SLOT: usize = usize::MAX - 2; // （新規）
fn region_field(slot: usize) -> usize { REGION_SLOT - slot } // （新規）
fn place_slot(place: &Place) -> usize; // （新規）先頭の印の slot。印がなければ 0
fn mask_slots(mask: RegionMask) -> impl Iterator<Item = usize>; // （新規）
fn join(result: &mut Value, value: Value); // （新規）match の腕の結果を合わせる

// Checker のメソッド（すべて新規）
fn region_count(&self, ty: &Type) -> usize; // Type::Record(id, _) で records[id].region_count >= 2 ならその数、他は 1
fn slots<'v>(&self, value: &'v Value, ty: &Type) -> Option<&'v [BTreeSet<usize>]>;
fn slot_loans(&self, value: &Value, ty: &Type, mask: RegionMask) -> BTreeSet<usize>;
fn project(&self, value: &Value, root: &Type, path: &[usize], ty: &Type) -> Value;
fn record_value(&mut self, expression: &TypedExpr, during: &BTreeSet<usize>) -> Result<Value, Diagnostic>;
fn argument_regions(&self, sources: &RegionSources, index: usize, argument: &TypedExpr, value: &Value,
                    current: &mut Value, slots: &mut [BTreeSet<usize>]);
```

- `Checker::slots` は `value.regions` が `Some`、長さが `region_count(ty)`（2 以上）、全 slot の和が `value.loans` と等しいときだけ `Some` を返す。
  それ以外は `None`（全 slot の和として扱う）。`value.regions` を直接読むのはこのメソッドと `join`・`merge` だけにする。
- `slot_loans` は `slots` が `Some` なら `mask` の slot の和、`None` なら `value.loans` 全体を返す。
- `place_slot` は `place.fields.first()` が `REGION_SLOT - (MAX_RECORD_REGIONS - 1) ..= REGION_SLOT` にあれば `REGION_SLOT - field`、他は 0。
  印は `ELEMENT`・`PAYLOAD` と重ならない。
- `record_value`・`project`・`argument_regions` には `#[inline(never)]` を付ける（落とし穴「stack」）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 字句・構文 | `src/lexer.rs`, `src/syntax.rs`, `src/parser.rs` | `Parser::region_list`, `Parser::finish_type_primary`, `Parser::reference_type`, `RecordDecl.regions`, `TypeExprKind::Regions` | 変更なし（D1）。複数名の宣言・使用はすでに読む |
| 整形・文書生成・LSP・警告 | `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/warnings.rs`, `src/polymorph.rs` | `TypeExprKind::Regions` の各 match、`region_text` | 変更なし。整形の冪等性をテストで確認する |
| 型の定義 | `src/check.rs` | `RegionMask`・`MAX_RECORD_REGIONS`・`RegionSources`（新規）、`CheckedRecord`、`CheckedFunction` | `region_count`・`field_regions` を追加。`region_sources` の型を変更 |
| record の構築 | `src/check.rs` | `check_modules_collect` の record ループの `Ok(CheckedRecord { .. })` | `let (region_count, field_regions) = regions::field_regions(record)?;` で埋める（E1017、アルゴリズム A） |
| 宣言検査 | `src/regions.rs` | `validate_modules`, `labels`, `slot_labels`・`record_regions`（新規） | 一 region 制限の削除、個数・フィールド・混在の規則（アルゴリズム B、C） |
| 関数契約 | `src/regions.rs`, `src/check.rs` | `contract`、`check_modules_collect` の `region_sources` | `Option<RegionSources>` を返し、保持する（アルゴリズム D） |
| 関数の生成 | `src/closures.rs`, `src/polymorph.rs` | `CheckedFunction { region_sources: None, .. }` の 6 か所 | 変更なし（`None` のまま型が合う） |
| 所有権の値 | `src/ownership.rs` | `Value`、`region_count`・`slots`・`slot_loans`・`project`・`record_value`（新規）、`eval_value` の `E::Record`・`E::RecordUpdate`・`E::Field` 腕、`read_places`、`merge` | slot ごとの loan の生成・射影・合流（アルゴリズム E、F、I） |
| 所有権の関数 | `src/ownership.rs` | `check_body`、`eval_composed` の `E::Call` 腕、`argument_regions`・`REGION_SLOT`・`region_field`・`place_slot`・`mask_slots`（新規） | slot ごとの外部 loan、返却検査、直接完全適用（アルゴリズム G、H） |
| 所有権の制御 | `src/ownership_control.rs` | `eval_match`（`join` を使う） | 腕の結果を slot ごとに合わせる。`check_loop`・`loop_summary`・`alias_source`・`view`・`finish_control_scope` は変更なし |
| LLVM・ランタイム | `src/llvm.rs` ほか `src/llvm_*.rs`、`src/runtime/` | – | 変更なし（region は `resolve_type` で消える） |
| テスト | `tests/borrowed_records.rs`, `tests/fixtures/borrowed_records/Main.tz`, `tests/features.mjs` | 新規 6 テスト、fixture の export 4 個、suite の 6 ケース | 「テスト計画」 |
| 文書 | 「ドキュメント」の 6 ファイル | – | 「ドキュメント」 |

### 生成 IR とランタイム

変更なし。`resolve_type` が region を消すので、`Type`・`llvm_type`・record の LLVM struct・drop・clone・公開 ABI は region の有無で変わらない。
参照フィールドは従来どおり `ptr`（`ref [T]` は C03 の descriptor）。ランタイム関数・WASM import は増えない。
region 付きと region を消したソースの IR が一致すること（手順 8）で確認する。

### アルゴリズム

A. record 構築時の mask（`regions::field_regions`）。

```text
count = record.regions.len()
count > MAX_RECORD_REGIONS なら record.regions[MAX_RECORD_REGIONS].span で E1017。count < 2 なら (count, [])
bit(name) = 1 << (record.regions での位置)。未宣言の名前は all（validate_modules が後で E1013）
all = ((1u32 << count) - 1) as RegionMask                     -- count = 16 でも桁あふれしない
各フィールド（宣言順）の masks:
    field.ty.kind が Regions(_, names) で names.len() >= 2 なら [bit(n) for n in names]   -- 内側の slot i -> names[i]
    それ以外は [field.ty に現れる名前の bit の OR]（名前がなければ [all]。収集は has_regions と同じ worklist）
```

B. `record_regions`・`slot_labels`・`labels`（D3、D4）。型の解決の失敗は無視し、既存の検査に任せる（D12）。

```text
record_regions(expr) = resolve_type(expr).ok() が Type::Record(id, _) なら types.records[id].region_count、他は 0
slot_labels(expr, field):
    expr が Regions(inner, names) で n = record_regions(inner) >= 2 かつ names.len() == n なら
        has_regions(inner) なら mixing(expr.span, field)（型引数の中の region）。未宣言の名前は既存の "undeclared region ..."
        return [{name} for name in names]
    return [labels(expr, field)]
labels(expr, field): 既存の worklist。Regions(inner, names) の腕の先頭で n = record_regions(inner) >= 2 なら、
    names.len() != n は E1013 個数、そうでなければ mixing（入れ子の直接適用）。span はどちらもこの Regions
    以降は既存のまま。最後の labels.len() > 1 の検査も mixing(expr.span, field)
mixing(span, field) = field なら新規のフィールド用メッセージ、でなければ既存の引数・結果用メッセージ
```

C. `validate_modules` の record ループ。既存の `record.regions.len() > 1` の拒否を消し、各フィールドで
`slot_labels(&field.ty, &declared, .., true)` の全名前 `found` を集める。`record.regions.len() >= 2` で `found` が空、かつ
`resolve_type(&field.ty, ..)?.contains_stored_reference(&types)` なら名前のない借用フィールドの E1013（`field.ty.span`）。
最後に全フィールドの名前で `check_used(&record.regions, &used)`。

D. `contract`（D5）。

```text
inputs = [slot_labels(p.ty, declared, false) for p in function.parameters]; result = slot_labels(function.result, declared, false)
check_used(function.regions, inputs と result の全名前)
result のどの slot も名前を持たなければ None（A09 と同じ）。function.result.kind が Regions でなければ既存の "annotate the whole ..."
各 result slot（名前はちょうど一つ）で pairs = {(i, k) | inputs[i][k] がその名前を含む}。空なら既存の "result region '{r}' has no matching input; ..."
return Some([各 slot の pairs])
```

E. `project`（`read_places` と右辺値の `E::Field` から呼ぶ）。

```text
project(value, root, path, ty):
    slots = self.slots(value, root)
    if slots が None: return Value { loans: value.loans, regions: None, closed_result: value.closed_result }
    maps = [1 << k for k in 0..region_count(root)]    -- 今の型の各 slot -> root の slot の mask
    current = root
    for field in path:
        if current == Type::Record(id, args) and records[id].region_count >= 2 and field < フィールド数:
            masks = records[id].field_regions[field]; next = record_fields(id, args)[field]
            via(m) = OR(maps[k] for k in mask_slots(m))
            maps = masks.len() >= 2 かつ masks.len() == region_count(next) なら [via(m) for m in masks]
                   そうでなければ [OR(via(m) for m in masks)]
            current = next
        else:                                          -- 単一 region の record、タプル、ELEMENT、PAYLOAD
            maps = [OR(maps)]; break
    loans(m) = ∪ slots[k] for k in mask_slots(m)
    if maps.len() >= 2 and maps.len() == region_count(ty):
        return Value { loans: ∪ loans(m), regions: Some([loans(m) for m in maps]), closed_result: value.closed_result }
    return Value { loans: loans(OR(maps)), regions: None, closed_result: value.closed_result }
```

`read_places` の借用を持つ型の分岐は、場所が 1 個、root の local が不変（`!local.mutable`）、`region_count(&local.ty) >= 2` のときだけ
`project(stored, &local.ty, &place.fields, &expression.ty)` の `loans` と `regions` を使い、他は既存どおり `stored.loans` を足す。
右辺値の `E::Field(inner, index)` は `region_count(&inner.ty) >= 2` のとき `project(&record, &inner.ty, &[index], &expression.ty)`、他は既存どおり。
`E::UnionPayload` は既存のまま（全 slot）。

F. `record_value`（`eval_value` の `E::Record`・`E::RecordUpdate` 腕はこの呼び出しだけにする）。

```text
n = region_count(expression.ty); slots = n >= 2 なら n 個の空集合、でなければなし
start = held.len()
for (index, child) in [(None, base)]（更新のとき）に続けて fields の並び順の (Some(index), expr):   -- expression.children() と同じ順
    value = eval(child, Use::Consume, during)
    if slots がある:
        index が None（base）: for k in 0..n: slots[k] ∪= slot_loans(value, expression.ty, 1 << k)
        index が Some(i): masks = records[id].field_regions[i]
            masks.len() >= 2 かつ masks.len() == region_count(child.ty):
                for (s, m) in masks: for k in mask_slots(m): slots[k] ∪= slot_loans(value, child.ty, 1 << s)
            それ以外: for k in mask_slots(OR(masks)): slots[k] ∪= value.loans
    result.loans ∪= value.loans; held.push(value)
held.truncate(start)
if slots がある: result.regions = Some(slots)
```

`n < 2` のときは既存の腕と同じ処理（全フィールドの loan の和）になる。

G. `check_body`（D7）。

```text
for parameter in parameters:
    if !task and parameter.ty.carries_loans(types):
        root = usize::MAX - parameter.id; external.insert(root)
        n = region_count(parameter.ty)
        if n >= 2:
            slots = [{loan(Place { root, fields: vec![region_field(k)] }, false, {})} for k in 0..n]
            value = Value { loans: ∪ slots, regions: Some(slots), .. }
        else: 既存どおり（fields が空の外部 loan 一つ）
result = eval(body); slots = self.slots(result, body.ty)
for id in result.loans（id の昇順。一つのループのまま）:
    既存: task または root が外部でなければ既存の E1013
    if let Some(sources) = region_sources:
        loan = loans[id]; owner = parameters の中で usize::MAX - p.id == loan.place.root となる添字
        for (j, allowed) in sources:
            if (slots が None または slots[j] が id を含む)
               and !(owner == Some(i) かつ allowed が (i, place_slot(loan.place)) を含む):
                return 既存の "returned borrow does not match the declared result region; ..."
```

単一 slot の引数・結果では `place_slot` が 0、`sources` が 1 個なので、A09 の `allowed_roots` と同じ判定になる。

H. `eval_composed` の `E::Call` 腕と `argument_regions`。

```text
region_sources = 既存の条件（既知の関数、引数数ちょうど）で function.region_sources
slots = region_sources があれば sources.len() 個の空集合（callee の値の loan で初期化する）
for (index, argument):
    value = eval(argument)
    region_sources があれば argument_regions(sources, index, argument, value, current, slots)
    なければ既存どおり current.loans ∪= value.loans
    既存の held・boundary の処理。current を Value::default() に戻す分岐では slots も空にする
if expression.ty.carries_loans:
    result = current
    if sources.len() >= 2 and region_count(expression.ty) == sources.len(): result.regions = Some(slots)

argument_regions(sources, index, argument, value, current, slots):
    for (j, allowed) in sources: for (i, k) in allowed where i == index:
        loans = slot_loans(value, argument.ty, 1 << k); current.loans ∪= loans; slots[j] ∪= loans
```

I. 合流。`Checker::merge` は既存の loan の和に加え、local ごとに `value.regions != other_value.regions` なら `value.regions = None` にする。
`eval_match` の `result.loans.extend(value.loans)` は `join(&mut result, value)` に置き換える。`join` は、`result` が loan も `regions` も
持たなければ `value.regions` を引き継ぎ、両方 `Some` で長さが同じなら slot ごとの和、`value.loans` が空なら `result.regions` を保ち、
それ以外は `None` にしてから `value.loans` を足す。

### 他チケットへの提供インターフェース

- A13: `CheckedRecord.region_count`・`field_regions`、`RegionMask`、`MAX_RECORD_REGIONS`、`Value.regions` と `Checker::slots`・`slot_loans`、
  `region_field`・`place_slot`、`RegionSources`。排他 region を足すときは、slot ごとの外部 loan の `mutable` をその slot のフィールドから決める。
- A13 への注意: `Checker::access` は `Use::Write`・`Use::MutBorrow` で `place.fields` が空でない場所を E1014 にする。
  多 region 引数の外部 loan の場所は先頭に slot 印を持つので、`deref` した排他フィールドへの書き込みを通すにはこの判定で先頭の印を除く必要がある。
- C08: 影響なし（A12 は `&mut [T]` の意味に触れない）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。新規テストはすべて `tests/borrowed_records.rs` に足す。
中間の手順では未使用の警告が出てよい（手順 11 の clippy で 0 にする）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の 4 ケースを `/tmp/tz-a12/a09`・`pair`・`flat`・`arity` の `Main.tz` に置き、`a09` の IR を保存する。
- 確認: `run` の結果が「再現」のとおり（`a09` は `42`、他の 3 件は記載の E1013）。`borrowed_records` は `5 passed`。stack-depth の 6 件はそれぞれ `1 passed`。
  `borrowed_records` suite の 10 ケースが成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
for name in a09 pair flat arity; do target/release/tsuzuri run /tmp/tz-a12/$name; done
target/release/tsuzuri build /tmp/tz-a12/a09 --emit llvm -o /tmp/tz-a12/a09-before.ll
target/release/tsuzuri build /tmp/tz-a12/a09 --emit llvm -O3 -o /tmp/tz-a12/a09-before-O3.ll
cargo test --locked --test borrowed_records
cargo test --locked --test types_ownership
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --lib honors_exact_expression_and_parenthesis_depth_limits
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test generic_records bounds_generic_lists_and_nested_types
cargo test --locked --test control control_syntax_has_bounded_depth
node tests/features.mjs target/release/tsuzuri borrowed_records
```

### 手順 2: record の region 数・mask と E1017

- 変更: `src/check.rs`（`RegionMask`・`MAX_RECORD_REGIONS`・`RegionSources`、`CheckedRecord` の 2 フィールド、`check_modules_collect` の record ループ）、
  `src/regions.rs`（`field_regions`）、テスト `record_regions_are_bounded`（新規）。
- 内容: アルゴリズム A。結果で `region_count`・`field_regions` を埋める。この手順ではまだ誰も読まない。
- 確認: `cargo test --locked --test borrowed_records` が `6 passed`（17 region の record が E1017）。`cargo test --locked --test types_ownership` が成功。

### 手順 3: 宣言検査（record）

- 変更: `src/regions.rs` の `validate_modules`・`labels`、`slot_labels`・`record_regions`、テスト `multiple_record_regions_validate_declarations`（新規）。
- 内容: アルゴリズム B と C。`contract` はまだ `labels(.., false)` を使うので、`Pair {r s}` を引数・結果に書くとこの手順では既存メッセージの E1013 になる。
  所有権はまだ全 slot の和なので、受理された多 region record は保守的に安全である。
- 確認: `borrowed_records` が `7 passed`。`cargo test --locked --test control --test docs --test host_imports` が成功。

### 手順 4: `region_sources` を `RegionSources` へ（挙動は変えない）

- 変更: `src/regions.rs` の `contract`、`src/check.rs` の `CheckedFunction.region_sources`、`src/ownership.rs` の `check_body`・`eval_composed` の `E::Call` 腕、
  `REGION_SLOT`・`region_field`・`place_slot`。
- 内容: `contract` は `labels` のまま `Some(vec![{(i, 0) | 引数 i が結果の名前を持つ}])` を返す。`check_body` の `allowed_roots` をアルゴリズム G の
  （引数添字, `place_slot`）の検査に置き換える（この手順では印がないので slot は常に 0）。`E::Call` 腕は `sources[0]` に `(index, 0)` がある引数の loan だけを入れる。
- 確認: `borrowed_records` が `7 passed`。`cargo test --locked` が成功する（純粋な置き換え）。

### 手順 5: slot ごとの loan（値・射影・合流）

- 変更: `src/ownership.rs` の `Value`、`region_count`・`slots`・`slot_loans`・`project`・`record_value`・`mask_slots`・`join`、`eval_value` の
  `E::Record`・`E::RecordUpdate`・`E::Field` 腕、`read_places`、`merge`、`src/ownership_control.rs` の `eval_match`、
  テスト `multiple_record_regions_split_local_loans`（新規）。
- 内容: アルゴリズム E、F、I。`eval_value` の腕には helper の呼び出しだけを置く。
- 確認: `borrowed_records` が `8 passed`。手順 1 の stack-depth 6 件が成功。`cargo test --locked --test types_ownership --test control` が成功。

### 手順 6: 関数契約の slot（外部 loan・返却・直接呼び出し）

- 変更: `src/regions.rs` の `contract`（`slot_labels` を使う）、`src/ownership.rs` の `check_body`・`E::Call` 腕・`argument_regions`、
  テスト `multiple_region_contracts_select_input_slots`（新規）。
- 内容: アルゴリズム D、G、H。
- 確認: `borrowed_records` が `9 passed`。手順 1 の stack-depth 6 件と `cargo test --locked` が成功。

### 手順 7: 保守的な経路の固定

- 変更: テスト `multiple_regions_stay_conservative_through_values`（新規）。実装は原則変えない。
- 内容: 関数値・部分適用・捕捉・Task・`let mut` が全 slot の和になることを固定する。拒否されるはずのケースが受理されたら、slot の情報がどこから漏れたか
  （`slots` の検証を通らずに `regions` を読んだ箇所）を直す。精度を足して辻褄を合わせない。
- 確認: `borrowed_records` が `10 passed`。

### 手順 8: 生成 IR の不変

- 変更: テスト `multiple_regions_do_not_change_generated_ir`（新規）。
- 内容: 「Rust テスト」6 の比較と、手順 1 の A09 サンプルの IR の再生成。
- 確認: `borrowed_records` が `11 passed`。次の `cmp` が何も出さない。

```sh
cargo build --release --locked
target/release/tsuzuri build /tmp/tz-a12/a09 --emit llvm -o /tmp/tz-a12/a09-after.ll
target/release/tsuzuri build /tmp/tz-a12/a09 --emit llvm -O3 -o /tmp/tz-a12/a09-after-O3.ll
cmp /tmp/tz-a12/a09-before.ll /tmp/tz-a12/a09-after.ll
cmp /tmp/tz-a12/a09-before-O3.ll /tmp/tz-a12/a09-after-O3.ll
```

### 手順 9: E2E

- 変更: `tests/fixtures/borrowed_records/Main.tz`、`tests/features.mjs` の `borrowed_records.cases`（「E2E」）。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri borrowed_records` が native・WASM × `-O0`・`-O3` で
  16 ケースとも成功する。`target/release/tsuzuri run /tmp/tz-a12/pair` が `3` を出す。

### 手順 10: 文書

- 変更: 「ドキュメント」の 6 ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md _docs/language-reference/lifetimes.md _docs/feature-status.md` が成功し、
  新しい実行サンプル（`run=7`）が一致する。

### 手順 11: 最終確認

- 確認: GUIDE §2.3 の全コマンド（fmt・clippy を含む）、`cargo test --locked`、`node tests/features.mjs target/release/tsuzuri`（全 suite）、
  `git diff --check` が成功する。完了報告（GUIDE §13）に D10・D13 が未承認であること、Phase 2 に着手していないことを書く。

## テスト計画

### Rust テスト

新構文（実装後に有効。未検証）。`tests/borrowed_records.rs` に共通の定数と helper を足す。

```rust
const PAIR: &str = "record Pair {r s} { left: ref {r} string, right: ref {s} string }\n";
const FUNCTIONS: &str = "def make_pair {r s} :: ref {r} string -> ref {s} string -> Pair {r s}\n\
    fn make_pair left right = Pair { left: left, right: right }\n\
    def left_of {r s} :: Pair {r s} -> ref {r} string\nfn left_of pair = pair.left\n\
    def swap {r s} :: Pair {r s} -> Pair {s r}\nfn swap pair = Pair { left: pair.right, right: pair.left }\n\
    def pick {r s} :: bool -> ref {r} string -> ref {r} string -> ref {s} string -> Pair {r s}\n\
    fn pick flag first second other = if flag then Pair { left: first, right: other } else Pair { left: second, right: other }\n";

fn program(declarations: &str, body: &str) -> String {
    format!("{declarations}let long = \"abcdefg\"\n{body}")
}

fn many_regions(count: usize) -> String {
    let regions: Vec<_> = (0..count).map(|index| format!("r{index}")).collect();
    let fields: Vec<_> = regions.iter().map(|region| format!("f{region}: ref {{{region}}} i64")).collect();
    format!("record Big {{{}}} {{ {} }}", regions.join(" "), fields.join(", "))
}
```

受理は `analyze(&source).unwrap_or_else(|error| panic!("{source}\n{error:?}"))`、拒否は `assert_eq!(analyze(&source).unwrap_err().code, code, "{source}")`
（既存テストと同じ形）。以下の本文は `program(..)` の `body` に改行区切りで入れる。

1. `record_regions_are_bounded`（手順 2）: `many_regions(17)` が E1017。
2. `multiple_record_regions_validate_declarations`（手順 3）:
   - 受理: `program(PAIR, "let short = \"xy\"\nlet pair = Pair { left: ref long, right: ref short }\npair.left.length + pair.right.length")`、
     `"record Pair<'a, 'b> {r s} { left: ref {r} 'a, right: ref {s} 'b }"`、`format!("{PAIR}record Swapped {{a b}} {{ pair: Pair {{b a}} }}")`、
     `"record Keep<'a> {r s} { left: ref {r} i64, right: ref {s} i64, extra: 'a, call: i64 -> i64 }"`（型変数・関数型は全 region）、`many_regions(16)`。
   - E1013: `format!("{PAIR}record Outer {{r}} {{ pair: Pair {{r}} }}")`（個数）、`format!("{PAIR}record Outer {{r s}} {{ pairs: [Pair {{r s}}] }}")`（入れ子の直接適用）、
     `"record Bad {r s} { left: ref {r} string, right: ref {s} string, other: ref string }"`（名前なし）、
     `"record Bad {r s} { both: (ref {r} string, ref {s} string) }"`（混在）、`"record Bad {r s t} { left: ref {r} string, right: ref {s} string }"`（未使用）、
     `"record Bad {r s} { left: ref {r} string, right: ref {q} string }"`（未宣言）、
     `"record View {r} { text: ref {r} string }\nrecord Bad {r s} { view: View {r s} }"`（既存メッセージ）。
   - `tsuzuri::parser::parse(PAIR).unwrap().records[0].regions.len() == 2`。`format!("{PAIR}{FUNCTIONS}")` に `formatter::format_source` を 2 回かけた結果が一致する
     （既存の `named_return_regions_exclude_unrelated_input_lifetimes` と同じ形）。
3. `multiple_record_regions_split_local_loans`（手順 5、宣言は `PAIR`）:
   - 受理: `let kept = { let short = "xy"; let pair = Pair { left: ref long, right: ref short }; pair.left }` と `kept.length`。
     `record Swapped {a b} { pair: Pair {b a} }` を足し、`swapped.pair.left` を返す同じ形。
     block 内で `let pair = if long.length > 3 then Pair { left: ref long, right: ref short } else Pair { left: ref long, right: ref short }` と束縛して `pair.left` を返す形（`join`）。
     `let mut short = "xy"`、`let kept = { let pair = Pair { left: ref long, right: ref short }; pair.left }`、`short = "changed"`、`kept.length`（片方の所有者だけを置換）。
     `let t = (Pair { left: ref long, right: ref long }, 1)` と `long.length`（タプル内の局所値）。
     `record Named {r s} { left: ref {r} string, right: ref {s} string, name: string }` の部分 move（`let name = named.name` の後に `named.left.length + name.length`）。
   - E1013: 1 件目の `pair.right` 版、`swapped.pair.right` 版、1 件目の `let mut pair` 版（D6）。
   - E1014: 4 件目の `pair.right` 版（`kept` が `short` を借用したまま置換する）。
   - E1012: `Named` の値を `let moved = named` で move した後の `named.left.length`。
4. `multiple_region_contracts_select_input_slots`（手順 6、宣言は `format!("{PAIR}{FUNCTIONS}")`）:
   - 受理: 宣言だけ、`let kept = { let short = "xy"; left_of (make_pair (ref long) (ref short)) }`、
     `let kept = { let short = "xy"; (swap (make_pair (ref short) (ref long))).left }`、`let kept = { let other = "xy"; (pick false (ref long) (ref long) (ref other)).left }`
     （それぞれ `kept.length` で終える）。generic の `record Pair2<'a, 'b> {r s} { left: ref {r} 'a, right: ref {s} 'b }` と
     `def first_of {r s} :: Pair2<'a, 'b> {r s} -> ref {r} 'a`・`fn first_of pair = pair.left` を、`let n = 42`、
     `let kept = { let s = "xy"; first_of (Pair2 { left: ref n, right: ref s }) }`、`deref kept` で使う形。
   - E1013: `def bad {r s} :: Pair {r s} -> ref {r} string` と `fn bad pair = pair.right`。`def bad {r s} :: ref {r} string -> ref {s} string -> Pair {r s}` と
     `fn bad left right = Pair { left: right, right: left }`。`def bad {r} :: Pair {r} -> i64` と `fn bad pair = pair.left.length`（個数）。
     `def bad {r s} :: ref Pair {r s} -> i64` と `fn bad pair = 0`（混在）。`def bad {r} :: Pair {r r} -> i64`（parser）。
     `let kept = { let short = "xy"; left_of (make_pair (ref short) (ref long)) }` と `kept.length`。
5. `multiple_regions_stay_conservative_through_values`（手順 7、宣言は `format!("{PAIR}{FUNCTIONS}")`）:
   - E1013: `let get = left_of` と `let kept = { let short = "xy"; get (make_pair (ref long) (ref short)) }`。`let partial = make_pair (ref long)` と
     `let kept = { let short = "xy"; left_of (partial (ref short)) }`。`let reader = { let short = "xy"; let pair = make_pair (ref long) (ref short); \() -> pair.left.length }` と
     `reader ()`。既存の `borrowed_records_preserve_owner_lifetimes_and_exclusivity` の Task 捕捉ケースの借用 record を `make_pair (ref long) (ref long)` に置き換えた 1 件。
   - 受理: `let get = left_of` と `(get (make_pair (ref long) (ref long))).length`（関数値そのものは使える）。
6. `multiple_regions_do_not_change_generated_ir`（手順 8）: 次の `annotated` と、`annotated.replace(" {r s}", "").replace(" {r}", "").replace(" {s}", "")` を
   `analyze` し、`llvm::emit_target(&module, llvm::Entry::Library, wasm)` の出力が `wasm` = `false`・`true` の両方で一致する。

```rust
let annotated = format!("{PAIR}def make_pair {{r s}} :: ref {{r}} string -> ref {{s}} string -> Pair {{r s}}\n\
    fn make_pair left right = Pair {{ left: left, right: right }}\n\
    def left_of {{r s}} :: Pair {{r s}} -> ref {{r}} string\nfn left_of pair = pair.left\n\
    let long = \"abcdefg\"\nlet short = \"xy\"\n(left_of (make_pair (ref long) (ref short))).length");
```

### E2E

新構文（実装後に有効。未検証）。`tests/fixtures/borrowed_records/Main.tz` の末尾に足す（既存の `PairView`・`pair` と名前を衝突させない）。

```tsuzuri
record RegionPair {r s} { left: ref {r} string, right: ref {s} string }
record RegionSwapped {a b} { pair: RegionPair {b a} }

private def make_region_pair {r s} :: ref {r} string -> ref {s} string -> RegionPair {r s}
fn make_region_pair left right = RegionPair { left: left, right: right }

private def region_left {r s} :: RegionPair {r s} -> ref {r} string
fn region_left pair = pair.left

export def region_call :: i64
fn region_call =
    let long = "abcdefg"
    let kept = { let short = "xy"; region_left (make_region_pair (ref long) (ref short)) }
    kept.length

export def region_loop :: i64 -> i64
fn region_loop count =
    let long = "abcdefg"
    let short = "xy"
    let pair = make_region_pair (ref long) (ref short)
    let mut total = 0
    let mut index = 0
    while index < count do
        total = total + (region_left pair).length + pair.right.length
        index = index + 1
    total
```

`export def region_field :: i64` と `export def region_nested :: i64` は `region_call` と同じ形で、`kept` の block だけを次に変える
（`swap`・`pick` の経路は Rust テスト 4 で検査する）。

- `region_field`: `{ let short = "xy"; let pair = RegionPair { left: ref long, right: ref short }; pair.left }`
- `region_nested`: `{ let short = "xy"; let swapped = RegionSwapped { pair: RegionPair { left: ref long, right: ref short } }; swapped.pair.left }`

`tests/features.mjs` の `borrowed_records.cases` に足す。期待値は文字列長から手で計算した（`"abcdefg"` = 7、`"xy"` = 2、`region_loop` は 1 周 7 + 2 = 9）。

```javascript
["region_call", [], 7n], ["region_field", [], 7n], ["region_nested", [], 7n],
...[0n, 1n, 1000n].map((count) => ["region_loop", [count], count * 9n]),
```

suite の既存の検査（native・WASM × `-O0`・`-O3`、確保追跡の `live == 0`、IR の決定性、WASM import なし）がそのまま適用される。trap のケースはない。

### 既存テストへの影響

なし。既存の期待値を変える必要が出たら停止条件。

### 性能

計測対象なし。所有権検査は射影ごとに経路長 × slot 数（最大 16）と `slots` の検証（loan 数に比例）だけ増える。性能の主張はしない。

## ドキュメント

- `docs/language.md` の「名前付き Region」: 例に `record Pair {r s}` と `def left_of` を足す。「一つのparameter/resultと一つのrecordは一つの共有regionだけを持てます。」を
  宣言順の対応・個数・フィールドの region・直接適用だけの混在・`let mut` と関数値は保守的・record あたり 16 の規則に置き換える。
  後続段階の一覧から「レコード内の複数regionの独立追跡」を消す（関数値型・排他借用field・参照経由の置換は残す）。
- `docs/architecture.md`: `regions.rs` の段落（単一 region の `region_sources`、「複数regionのfield別追跡と高階region型は未対応」）を `RegionSources`・
  `CheckedRecord.field_regions`・`Value.regions`・外部 loan の slot 印の説明に更新し、未対応の列挙から「独立した複数regionのfield別追跡」を外す。
- `_docs/language-reference/lifetimes.md`: 「借用を格納するレコード」の後に `## 独立した region を持つレコード` を足す（「例」の受理プログラムを `tsuzuri run=7` で載せる）。
  「現在の制限」表の「一つのレコード」を「宣言した region ごとに独立（最大 16）」、「レコードの独立した複数 region」を
  「対応。`let mut`・関数値・分岐やループの合流では全 region をまとめて保持」にする。
- `README.md` の所有権節: 「排他借用フィールドと、レコード内の独立した複数regionは未対応です。」を複数 region 対応に合わせて直す。
- `_docs/feature-status.md`: A12 の行を Phase 1 実装済みにし、未対応の列挙から「独立した複数 record region」を外す。
- `_features/README.md`: A12 の状態（GUIDE §10。Phase 2 は D13 承認待ちと注記）、A09 の注記の「設計段階の独立した複数record regionは未対応」、Rust 比の劣位表の A12。

## 受け入れ条件

- [ ] 手順 1〜11 の `確認:` がすべて成功し、`tests/borrowed_records.rs` が `11 passed`。
- [ ] 多 region record の宣言・使用・フィールド読み出し・返却契約・直接の完全適用で region ごとの寿命を検査する（「例」の受理プログラムが `7`、拒否表が指定のコード）。
- [ ] region なし・単一 region のプログラムの受理・拒否・メッセージ・IR が変わらない（既存テストは無変更、手順 8 の `cmp` が空）。
- [ ] 関数値・部分適用・捕捉・Task・`let mut` は全 region の和で保守的に扱う。
- [ ] 生成 IR と公開 ABI が region に依存せず、WASM import が増えない。
- [ ] record の region は 16 個まで（17 個目で E1017）。関数の上限は D10 の承認まで 128 のまま。
- [ ] `borrowed_records` suite の 16 ケースが native・WASM × `-O0`・`-O3` で成功する。
- [ ] 文書を更新し、`node scripts/check-docs.mjs` が成功する。
- [ ] D13 が未承認なら Phase 2 に着手していない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- stack（所有権）: `eval`・`eval_value`・`eval_composed`・`eval_control`・`place` は式の入れ子（最大 128）ごとに相互再帰する。これらに `Vec`・`BTreeSet` の
  局所変数を足すと全段の frame が大きくなり、debug build の 2 MiB stack で guard テストが落ちる。新しい処理は `#[inline(never)]` の helper に入れ、
  腕には呼び出しだけを置く。`Value.regions` は `Option<Box<[..]>>`（16 byte）にして `Vec` を直接持たない。上限や stack を上げない（停止条件）。
- stack（parser）: parser は変えない。`Parser::type_primary` は型の入れ子ごとに再帰する frame で、A09 は region の作業領域を `region_list`・
  `finish_type_primary`・`reference_type` へ分けて frame を保った。`region_list` に引数を足す、`type_primary` に分岐を足す変更は D1 違反であり、
  stack の回帰の原因にもなる。Phase 2 で再帰的な helper を足すときは `self.nesting` の増減を必ず対にする（減らし忘れで深さ 128 の前に溢れた前例がある）。
- ループの固定点: `loop_summary` は local ごとの loan の場所の集合だけを比べる。次の擬似コードで `let mut pair` の slot を信じると、1 周目の後の合流で
  loan の集合が変わらないため固定点とみなし、入口の対応（`left` = `long`）のまま抜けて `pair.left` を `long` だけとみなす。
  D6 の「`let mut` の local は射影で `regions` を使わない」を必ず守る。

```text
let mut pair = Pair { left: ref long, right: ref short }
while more do
    pair = Pair { left: pair.right, right: pair.left }
pair.left
```

- 古い `regions`: move（`loans.clear()`）と `retain_live_loans` は `loans` だけを消す。`value.regions` を直接読むと消えた loan の対応を使うので、
  必ず `Checker::slots`（長さと和の検証）を通す。`merge` で `loans` だけを足して `regions` を残すと、違う対応の分岐の後で古い対応を使う（不一致なら `None`）。
- mask の桁あふれ: 16 region で `1u16 << 16` は debug で panic する。`all` は `u32` で計算してから変換し、E1017 の検査を mask の計算より先に行う。
- 評価順序: `record_value` は `expression.children()` と同じ順（更新は base、次に `fields` の並び順）で評価して `held` へ積む。
  フィールド添字の順に並べ替えると衝突検査の結果が変わる。
- 診断の順序: `check_body` の検査を二つのループに分けると、局所値への参照と region の不一致が同時にあるときのメッセージが変わる。
  loan id 順の一つのループで、従来の「外部か」を先に検査する。
- `resolve_type` の順序: `labels` の `Regions` 腕で先に型を解決して失敗を返すと、未知の型名に region を二つ付けた既存の E1013 が別のコードに変わる。
  `record_regions` は `resolve_type(..).ok()` で失敗を無視する。
- slot 印の範囲: 印は外部 root の場所にだけ付け、local の場所へ押さない。`project` は local の型を実フィールド添字でたどるので、印があると誤読する。
- 型変数のフィールド: `record_fields(id, args)` で具体化したフィールド型は多 region record になりうる。`field_regions[f].len() == 1` と
  `region_count(next) == 2` が食い違うときは一様な mask として扱う（アルゴリズム E、F）。
- 排他借用フィールドの拒否（`check_modules_collect` の `contains_stored_mutable_reference`）は A13 まで残す。
- テスト件数と期待値: `cargo test --locked <filter>` は 0 件でも成功するので `running N tests` を見る。E2E の期待値は文字列長から手で計算し、
  コンパイラの出力から写さない。

## 対象外

- 排他借用フィールドと参照経由の更新（A13）。
- region 付き関数値型（Phase 2、D13）。
- outlives 制約・region の部分型・static region・一時値の寿命延長・Polonius 相当の推論（D15）。
- `let mut`・ループや分岐の合流・コレクション・タプル・union payload・関数値を経た slot ごとの追跡（D6、D8）。
- 同じ region を二つの位置へ渡す `Pair {r r}`、直接適用の型引数の中の region（D1、D4）。
- 借用 record の公開 ABI（A09 と同じく不可）。

## 決定事項

### D1: 構文は A09 の形を使い、parser を変えない

- 決定: record 宣言 `record R<'a> {r s} { .. }`、使用位置 `R<T> {r s}`（宣言順に対応）、`ref {r} T` をそのまま使う。`R {r r}` は parser の既存検査（E1013）のまま拒否する。
- 理由: 現在の parser がすでに複数名を読む（「現状」）。parser・formatter・docgen を変えずに済み、`type_primary` の frame と入れ子の上限に触れない。
  `R {r r}` は region を省略した使用か、別名の region で書ける。
- 状態: 既定案（実装者はこの案に従う）

### D2: 使用位置の個数と省略

- 決定: 多 region record へは宣言と同数の名前を与える。1 個だけの `Pair {r}` も E1013。省略した `Pair` は全 slot を一つの交差寿命とし、
  名前付き結果 region の入力元にならない。
- 理由: 旧版の既定案（互換優先）を維持する。1 個で全 region を束ねる形を許すと、宣言順の対応の誤りを検出できない。
- 状態: 既定案（実装者はこの案に従う）

### D3: フィールドへの region の割り当て

- 決定: 「型規則」3。多 region record では借用を直接格納するフィールドに region を必須とし、関数型・型変数のフィールドは全 region に属する。
  フィールド内の混在は直接適用だけ。単一 region の record は A09 のまま。
- 理由: 名前のない借用を全 region に属させると、そのフィールドを含む値の返却が必ず不一致になり、原因が宣言から遠い箇所で出る。
  関数型には region を書けない（既存の `a named region requires ...`）ため、全 region に属させるしかない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 引数・結果での region の混在

- 決定: 型全体が多 region record の直接適用のときだけ混在を許す。参照・配列・タプルの中の混在と、直接適用の型引数の中の region は E1013。
- 理由: 所有権検査が slot を持つのは多 region record の値だけで、入れ子の構造ごとに slot を持たせると `Value` が再帰構造になる。拒否は後から緩められる。
- 状態: 既定案（実装者はこの案に従う）

### D5: `region_sources` の一般化

- 決定: `Option<RegionSources>`（結果 slot ごとの（引数添字, 引数 slot）の集合）。直接の完全適用では入力元の slot の loan だけを結果の slot へ入れる。
- 理由: 単一 region では `(i, 0)` だけになり、A09 の添字集合と同じ意味になる。
- 状態: 既定案（実装者はこの案に従う）

### D6: 所有権で slot を分ける範囲

- 決定: `Value.regions` を持ち、精度は「評価順序・所有権・借用」の経路だけにする。`let mut` の local と可変引数は射影で `regions` を使わない。
  `merge` は両方の `regions` が等しいときだけ保つ。match と `if` の腕の結果は slot ごとの和。検証に通らない `regions` は無視する。
- 理由: `loop_summary` は loan の場所しか比べないので、再代入される値の slot を信じるとループの固定点で古い対応が残る（落とし穴）。
  不変の local は再代入されず、合流しても同じ値か消えた値なので対応は変わらない。match の腕の結果は同じ型の独立した値で、位置がずれない。
  旧版の「合流は全 region の和」は local の合流に適用する。
- 状態: 既定案（実装者はこの案に従う）

### D7: 引数の外部 loan と slot の印

- 決定: 多 region record 型の引数は slot ごとに外部 loan を作り、場所を `Place { root: usize::MAX - parameter.id, fields: vec![region_field(slot)] }` にする。
  返却検査は loan の root から引数、`place_slot` から slot を決める。単一 slot の引数は従来どおり `fields` が空の外部 loan 一つ。
- 理由: `deref` と再借用で作る loan は元の loan の場所を引き継ぐので、祖先をたどらずに slot が分かる。異なる slot の場所は重ならず、独立した借用として扱える。
- 状態: 既定案（実装者はこの案に従う）

### D8: 関数値・部分適用は保守的なまま

- 決定: 契約を使うのは既知の関数への引数数ちょうどの直接呼び出しだけ（A09・GUIDE D-28 と同じ）。関数値・部分適用・捕捉の結果は全入力・全 slot の和。
- 理由: 関数型は region を持たないので、値から契約を取り出せない。緩めるのは Phase 2 の範囲。
- 状態: 既定案（実装者はこの案に従う）

### D9: record の region 上限

- 決定: 16 個。17 個目で E1017（「診断」のメッセージ）。`regions::field_regions` で record の構築時に検査する。
- 理由: 旧版の決定を維持する。`u16` の mask に収まる。これまで 2 個以上は拒否していたので、受理済みのプログラムに影響しない。
- 状態: 既定案（実装者はこの案に従う）

### D10: 関数あたりの region 上限

- 決定: 旧版は 64 としたが、parser は `def` の region を 128 個まで受理している（`Parser::region_list` の `MAX_NESTING`）。64 へ下げると受理済みの
  プログラムを拒否するため、Phase 1 では変えず 128 のままにする。64 へ下げるのは承認後に別の変更として行う。
- 理由: 所有権検査の費用は record の slot 数（上限 16）で抑えられ、関数の region 数は名前の照合にしか使わない。
- 状態: 承認済み（2026-10-03。見直し提案を採用し、128 を正式な上限として `docs/language.md` に書いた）
- 見直し提案: 64 への変更を取りやめ、parser の 128 を正式な上限として文書化する。

### D11: region を型・単相化・IR に入れない

- 決定: region は `resolve_type` で消し、`Type`・単相化キー・LLVM 型・公開 ABI に入れない。region の情報は `CheckedRecord` と `CheckedFunction` の検査用フィールドだけに置く。
- 理由: A09 の設計を維持する。型の同一性に含めると単相化の数が増え、上限 1,024 を無駄に消費する。
- 状態: 既定案（実装者はこの案に従う）

### D12: A09 との互換

- 決定: region なし・単一 region の record と関数は、A12 の各経路で A09 と同じ結果にする（`field_regions` が空、`Value.regions` が `None`、外部 loan は一つ、
  `RegionSources` は `(i, 0)` だけ、`check_body` は一つのループ、`resolve_type` の失敗は既存の診断に任せる）。既存の診断メッセージの文言は変えない。
- 理由: 既存テストを無変更で通し、受理済みのプログラムの意味を変えないため。
- 状態: 既定案（実装者はこの案に従う）

### D13: Phase 2（region 付き関数値型）

- 決定: 「Phase 2（設計方針）」のとおり、関数型の前置量化 `{r} ref {r} T -> ref {r} T`（rank-1）を既定案とする。型変数 `'a` と混同しない表記を優先する。
- 理由: 関数型の同一性・単一化・関数値の表現に region が入り、D11 と GUIDE D-28 の「関数値は全入力を保持」を変える。
- 状態: 承認済み（2026-10-03。表記はこの案のとおり。実現は `Type::Function` を変えず、名前付き関数の引数の契約とした。実装と検証の追記 1）

### D14: 診断コードとメッセージ

- 決定: 新しいコードは作らず E1013 と E1017 を使う。新規メッセージは「診断」の 4 つ。既存メッセージの文言は変えない。
  `records currently have one shared region; ...` は出さなくなる。
- 理由: 寿命と region は E1013、資源上限は E1017 の分類に合う。E1024（型パラメーター・別名の命名）と E1015（多相性の不適合）は region の個数・対応に当たらない。
- 状態: 既定案（実装者はこの案に従う）

### D15: region の間の関係

- 決定: outlives 制約（Rust の `'a: 'b`）、region の部分型の宣言、`'static` 相当は導入しない。関係は同じ名前（交差寿命）と loan の包含だけで表す。
- 理由: loan に基づく検査で Phase 1 の用途（寿命の異なる借用の束ね）を表せる。制約の解決器は新しい基盤になる。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-03）

「A15 / A12 の実装を完遂して。複数フェーズある場合には、すべてのフェーズを完了させること」という依頼を D10・D13 の承認として扱い、
Phase 1 と Phase 2 を実装した。着手時の HEAD は `c995217`（ブランチ `Phase7-1`）で、A15 と同じ変更に含めた。性能は主張しない（region は生成 IR を変えない）。

### 実装

- Phase 1（手順 2〜9）: `src/check.rs` に `RegionMask`（`u16`）・`MAX_RECORD_REGIONS`（16）・`RegionSources`、`CheckedRecord.region_count`・`field_regions`、
  `CheckedFunction.region_sources: Option<RegionSources>`。`src/regions.rs` は record の region 数と field ごとの mask（17 個目で `E1017`）、使用位置の個数、
  名前のない借用 field、field・引数・結果の中の混在を検査し、結果の slot ごとの入力元（引数、slot）を作る。
- `src/ownership.rs`: `Value.regions`（`Option<Box<[BTreeSet<usize>]>>`。多 region record の値だけ slot ごとの loan を持つ）。record リテラル・更新（`record_value`）、
  field の読み出し（`field_value`・`project`）、不変の local・引数（`stored_value`）、`if`／`match` の腕（`join`）、直接の完全適用（`call_contract`・`argument_regions`）で
  slot を保ち、ほかの経路は `None`（全 loan）にする。外部 loan は slot ごとに `region_field(slot)` の印を持ち（D7）、`check_result` が結果の slot ごとに入力元を検査する。
- Phase 2: 構文 `TypeExprKind::Quantified`（parser の `quantified_type`。`type_primary` は変えない）と、formatter・docgen・semantic・warnings・polymorph の走査。
  `regions::contract` が名前付き関数の引数の量化型を `CallbackContract`（引数の位置・arity・結果 slot の入力元）にし、`regions::validate_contract_calls` が
  その関数を直接の完全適用だけに限る。所有権検査は、本体でその引数を完全適用した結果に契約の入力の loan だけを入れ、呼び出し側では渡した関数が契約を満たすことを検査する（`Checker::unsatisfied`）。
- テスト: `tests/borrowed_records.rs`（9 件を追加して計 14 件）、`tests/fixtures/borrowed_records/Main.tz` と `tests/features.mjs` の suite `borrowed_records`（6 export を追加して 20 ケース）。
- 文書: `docs/language.md`（名前付き Region）、`docs/architecture.md`、`_docs/language-reference/lifetimes.md`（2 つの節と `run=7` の例）、`README.md`、
  `_docs/feature-status.md`、`_docs/learn/why-tsuzuri.md`、`_features/README.md`、`_features/GUIDE.md`（D-33）。

### 決定事項への追記（チケットから外れた判断）

1. **D13 の実現方法。** 方針は量化を `Type::Function` に持たせ、関数型の同一性・単一化・`canonical_type` を変えるものだった。実装では型を変えず（D11 を保つ）、
   量化型は名前付き関数の引数の型全体にだけ書けるようにし、契約は `CheckedFunction.callback_contracts` に置いた。契約が関数値へ漏れないよう、その関数は
   直接の完全適用だけにし（関数値・部分適用は `E1013`）、呼び出しごとに渡す関数が契約を守ることを検査する。戻り値・ローカル・field・入れ子の量化型と、
   量化型の引数の `mut` は `E1013`。型を変えないので、単相化・生成 IR・GUIDE D-28（量化のない関数値は全入力を保持）は変わらない。
   量化型の値を持ち回る（戻り値・field に置く）には型に契約を入れる必要があり、後続の段階とする。
2. **渡せる関数。** 名前付き関数は、引数の個数が一致し、`region_sources` の各結果 slot の入力元が契約の入力元に含まれるときに受ける。先頭の引数を部分適用した
   名前付き関数は、束縛した引数を入力元に含まないときだけ受ける。ラムダは本体を契約で検査する（捕捉は入力に数えない）。引数として受けた量化型の関数は、
   同じ arity で契約を含意すれば渡せる。局所の関数値などそれ以外は `E1013`
   （`pass a named function with matching named regions, a lambda, or a parameter with the same region-quantified type here`）。診断はその引数の式（括弧を含む）を指す。
3. **D10 は見直し提案を採用。** 関数あたりの region は parser の 128 のまま（`docs/language.md` に記載）。
4. **stack。** 所有権検査の `eval_composed` を `eval_block`・`eval_call`・`eval_lambda`（`#[inline(never)]`）に分け、新しい処理は helper に置いた。
   型検査の `call_expression` の引数の走査は iterator adapter の連鎖をやめて素の `for` にした（式の入れ子ごとの frame が小さくなる）。上限と stack の大きさは変えていない。

### 確認（Apple M1 Max、macOS 27.0.1、Apple clang 21、Homebrew LLVM 21、rustc 1.98.1、Node v20.17.0）

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 cargo test --locked`（635 passed、0 failed）が成功。
  `tests/borrowed_records.rs` は 14 passed。GUIDE §3.1 の 7 つの深さと上限の回帰テストは既定の 2 MiB の stack で成功し、
  `bounds_nested_builder_expansion_not_just_source_syntax` が要する stack は 2098 KiB（`c995217`。単独の実行で溢れていた）から 1858 KiB になった。
- `node tests/features.mjs target/release/tsuzuri borrowed_records`（20 ケース）が native・WASM × `-O0`／`-O3` で成功（解放追跡と WASM の import なしを含む）。
  features 全体（5252 ケース）を含め、`tests/` の E2E の script 30 個（`math.mjs` と Windows 用の `windows.mjs` を除く）のうち 29 個が成功。`tests/debug_info.mjs` だけは
  失敗するが、`c995217` のコンパイラでも同じ `llvm-dwarfdump --verify`（`.debug_names`）の失敗で、この変更とは無関係。
- 生成 IR: A09 の例と A15 の再現 6 件の IR は `-O0`／`-O3` とも `c995217` と byte 一致。fixtures と examples の 156 の IR は 94 が一致し、62 は
  A15 の `Array.copy`・`List.copy` の追加による生成 id の一様なずれだけが異なる（id を写す比較で差なし）。region を使う新しい fixture は変更前のコンパイラが受理しないので比較の外。
- Phase 2 の受理と拒否を、テストの外の 24 個のプログラム（量化型を書ける位置と書けない位置、名前付き関数・部分適用・ラムダ・引数の受け渡し、
  捕捉した借用を返すラムダ、総称関数、関数の region と同じ名前の量化、`mut` の引数など）でも release のコンパイラで確かめた。
- `node scripts/check-docs.mjs`（97 ページ、851 リンク、184 例、native 302 回）、Windows の `cargo check --all-targets`（x86_64／aarch64-pc-windows-msvc）が成功。
