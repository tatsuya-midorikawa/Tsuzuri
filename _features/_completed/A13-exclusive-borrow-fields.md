# A13: 排他借用フィールドと参照経由の更新

| 項目 | 内容 |
| --- | --- |
| ID | A13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | A12 |
| 後続 | C08, F10 |
| 状態 | done（Phase 1・2。2026-10-06） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 承認済み（2026-10-06、GUIDE D-38）: D1（GUIDE §9 D-28「排他参照fieldは禁止する」の変更。チケット全体の前提）, D5（旧版草案の「`let mut` 所有値か `ref mut` 経由が必要」と「貸し直し中は record 全体へのアクセスを拒否」を外す）, Phase 2 |
| 改善する劣位 | Rust 比: 排他借用フィールドがない（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 手本にする既存実装 | 共有借用フィールド（A09）: `src/check.rs` の record フィールド検査ループ（`contains_stored_mutable_reference`）と `Validation::check` の `Type::Record` 分岐、`src/ownership.rs` の `Checker::read_places` と `eval_value` の `E::Record`。排他参照の貸し直し: `src/check.rs` の `reborrow_operand`・`require_mutable_reference`・`coerce_argument`、`src/ownership.rs` の `Checker::place` の `E::Dereference` 分岐と `Checker::access` の `via` 判定。型の走査: `src/recursive.rs` の `stored_all`（明示スタックと `seen`）。パターン view: `src/ownership_control.rs` の `view` |
| 主な影響ファイル | 変更: `src/check.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `tests/borrowed_records.rs`, `tests/fixtures/borrowed_records/Main.tz`, `tests/features.mjs`, `docs/language.md`, `_docs/language-reference/lifetimes.md`, `_docs/language-reference/ownership.md`, `_docs/feature-status.md`, `_docs/learn/why-tsuzuri.md`, `README.md`, `_features/README.md`。確認のみ（変更しない）: `src/regions.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/derive.rs`, `tests/generic_records.rs` |

## 目的

`record Cursor {r} { buffer: ref mut {r} Vec<i64>, position: i64 }` のように、排他借用を持つ作業用 view を作れるようにする。
Rust の `struct Parser<'a> { out: &'a mut Vec<u8> }` と同じく、可変な作業領域と状態を一つの値として関数間で受け渡せるようにする。
フィールド経由の貸し直し・置換・move は既存の loan 機構で検査し、生成 IR・公開 ABI・既存プログラムの判定は変えない。

実装者は Phase 1 だけを実装する。Phase 2（借用を持つ参照先と、参照経由の borrowed aggregate 置換）は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- A12 が `_features/README.md` の状態欄で done であること。確認: `grep -n "| A12 \|| A13 " _features/README.md`。
- A12 が「前提とする他チケットのインターフェース」の I1〜I4 を提供していること。I1 は `CheckedRecord` の定義を読んで確かめる。
  I3 は次のプログラムの `check` が成功することで確かめる（A09 の単一 region のままなら `kept` が `left` の loan も持つので `E1014`）。
  新構文（A12 完了後に有効。未検証）:

```tsuzuri
record Pair {r s} { left: ref {r} string, right: ref {s} string }
let mut left = "a"
let right = "bc"
let pair = Pair { left: ref left, right: ref right }
let kept = pair.right
left = "changed"
kept.length
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-a13/a12
# 上のプログラムを /tmp/tz-a13/a12/Main.tz に保存してから
target/release/tsuzuri check /tmp/tz-a13/a12
```

期待: 出力なしで終了コード 0。

- D1 と D5 が承認済みであること（`## 決定事項`）。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（既存 fixture の IR、stack-depth テスト、テスト件数）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- I1〜I4 のどれかが A12 にない。特に I3 の確認プログラムが `E1014` になる、または region 添字が位置順でない（集合だけ）。
  I1 が `RecordDecl` にだけあり `CheckedRecord` にない場合は、`CheckedRecord` を作る箇所で同じ添字を写すだけにし、それ以上の変更が要るなら報告する。
- `Type`・`TypedExprKind`・`ExprKind`・字句・構文解析器を変える必要が出た。本チケットは既存の `ref mut {r} T` 構文と既存の型だけで実装する。
- `Loan`・`Value`・`Place`・`Use` の構造を変える必要が出た（D6）。A12 が導入した region 別の表現を使うことだけは除く。
- 「テスト計画」の受理ケースが、本チケットで変えない既存の機構（`read_places`・`access`・`loan`・`retain_live_loans`・パターンの部分 move）のせいで拒否される。
  「アルゴリズム」の追跡と実際の挙動が違うことを意味する。
- 手順 5 の後で、ガードのない所有値の `match`（T1 の 10 番）が M7 で拒否される。`view` がガードのない束縛にも使われているので、
  M7 を適用する呼び出し元の区別を報告する。
- 「テスト計画」の拒否ケースが受理される（健全性の問題）。特に共有参照経由の変更（D7）とガードの view（D8）。
- 既存テストの期待値（診断コード、`tests/generic_records.rs` のメッセージ断片、IR）を変える必要がある。
  旧メッセージ `record fields cannot store mutable references; use a shared borrow` を M5 に置き換えることだけは除く（D10。旧文を検査するテストはない）。
- 手順 1 で保存した既存 fixture の IR が変わる。
- `src/ownership_control.rs` に `view` 以外で、record 内の排他借用をパターン変数へ読み取り専用 view として束縛する経路が見つかった（D8 の範囲外）。
- stack-depth の 3 テストが失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

### 宣言の検査

- `src/check.rs` のレイアウト計算の後の record フィールド検査ループ（`for record in &records { for (_, ty) in &record.fields {`）は、
  `ty.contains_stored_mutable_reference(&types)` なら `E1013 "record fields cannot store mutable references; use a shared borrow"` を
  record 名の span に出し、そのフィールドの `validate_size` を飛ばす。record フィールドの `ref mut` を拒否しているのはここだけである。
- `Validation::check` の `Type::Record` 分岐は、具体化した各フィールドが `contains_stored_mutable_reference` なら
  `E1013 "{type} would store a mutable reference in field '{name}'; use a shared borrow"` を返す。宣言側の型は
  `record.fields.iter().zip(types.record_fields(*id, args))` の左側にあるが使っていない。`tests/generic_records.rs` がこの文の断片を 4 件で検査する。
- 同じ `Validation::check` の union 分岐は `E1013 "union payloads cannot store mutable references; {type} would store one in case '{name}'; use a shared borrow"`、
  `Array`／`List`／`Vec` 分岐は `E1005 "array and list elements cannot contain mutable references; collections are deeply immutable"`。
- `contains_stored_mutable_reference` は `TypeContext::stored_all`（`src/recursive.rs`）で record のフィールド・union の payload・
  コレクション要素・タプルを辿り、各型に非公開の `contains_mutable_reference` を適用する。`stored_all` は参照の内側へ進まず、
  `contains_mutable_reference` は record の内側へ進まない。したがって排他フィールドを持つ record への共有参照（`ref Counter`）はどちらでも検出されない。
- `src/regions.rs` の `labels` は `ref mut {r} T` を受け、`check_used` は未使用 region を拒否する。region は検査専用で `Type` に残らない。
- `src/llvm.rs` は `Type::Reference(..) => "ptr"` で、参照の可変性に関係なく同じ表現にする。

### 型の性質

- `Type::is_copy` は `Reference(_, true)` を非 Copy とし、`Record` は全フィールドが Copy のときだけ Copy。`Type::can_capture` は
  `Reference(_, true)` と `Task` を拒否して `Record` の全フィールドを辿る。`Type::can_send` は参照を含む値を拒否する。
  したがって排他参照を持つ record は、規則を足さなくても非 Copy・Capture 不可・Send 不可になる。
- 再利用可能な関数値による捕捉の拒否は `src/polymorph.rs` の
  `E1005 "cannot capture {type} in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks"`。
- `src/derive.rs` の導出はフィールドを共有の `ref`（`ExprKind::Borrow(.., false, Notation::Keyword)`）で渡す。

### 貸し直しの型付け

- keyword の `ref mut r` は、`r` の静的型が参照なら `reborrow_operand` が `Borrow(Dereference(r), true)` を作る。呼び出し時の補完
  （`coerce_argument`）も同じ木を作る。`ref mut counter.value` は `Borrow(Dereference(Field(counter, value)), true)` になる。
- `require_mutable_reference` は最外の `Dereference(reference)` の `reference.ty` だけを見て、共有参照なら
  `E1014 "cannot mutate or exclusively reborrow through a shared reference"`。経路のより内側の共有参照は見ない。

### 所有権検査

- `src/ownership.rs` に `Loan { place, mutable, parents, view }`、`Value { loans, closed_result }`、`Place { root, fields }`。
  `Checker::loan` は place の根 local が持つ loan を parents に加える。
- `Checker::place` の `E::Dereference` 分岐は、参照値の各 loan について「loan の place と、via = parents ∪ {loan}」を返す。
  `Checker::access` は `MutBorrow`／`Write` で、via が空なら根 local の `mutable`、空でなければ via の全 loan の `mutable` を要求し、
  `place.fields` が空でなければ `E1014 "mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable"`。
  via 以外の生きた loan と重なれば `E1014 "access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively"`。
- `Checker::read_places` は、読んだ式の型が loan を持つとき根 local の loan 全体（外部 root なら via）を値の loan にする。
  経路上の共有 loan は値に残らない。move のとき子の loan が生きていれば `E1014 "cannot move an exclusive reference while it is reborrowed"`。
- `retain_live_loans` は生きていない local の loan を捨て、`active` は parents を辿る。フィールド経由の貸し直しは親 loan を parents に持つので、
  record が死んでも貸し直しの生存中は親 loan が active に残る。
- `check_body` は借用を持つパラメーターに外部 loan を一つ作り、`mutable = matches!(parameter.ty, Type::Reference(_, true))`。
  record 型のパラメーターの外部 loan は常に共有で、Task 本体では外部 loan を作らない。
- この機構だけで、フィールド経由の貸し直し（`counter` が `let` でも via の loan が mutable なので通る）、貸し直し中の同じ参照先への
  アクセス拒否、record の move 拒否、部分 move、NLL による解放が成り立つことを手で追跡した（「アルゴリズム」の追跡）。
  足りないのは、宣言の許可、record パラメーターの外部 loan の mutable、共有参照経由の排他アクセスの拒否、パターン view の拒否である。
- `src/ownership_control.rs` の `view` はパターン変数を `Loan { mutable: false, parents: via, view: Some(ty) }` の読み取り専用 view にする。
  view の参照外しは束縛元の記憶域（例 `counter.[value]`）を place とし、参照先の所有者に loan を作らない。

### 再現（検証済み）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-a13/reject_field /tmp/tz-a13/capture_mut /tmp/tz-a13/reborrow_local
printf '%s\n' 'record Cursor {r} { buffer: ref mut {r} Vec<i64>, position: i64 }' 'let answer = 1' 'answer' > /tmp/tz-a13/reject_field/Main.tz
printf '%s\n' 'let mut total = 0' 'let r = ref mut total' 'let read = \() -> deref r' 'read ()' > /tmp/tz-a13/capture_mut/Main.tz
printf '%s\n' 'def bump :: ref mut i64 -> unit = \target -> { deref target = deref target + 1; }' 'let mut total = 40' 'let r = ref mut total' 'bump r' 'bump r' 'deref r' > /tmp/tz-a13/reborrow_local/Main.tz
target/release/tsuzuri check /tmp/tz-a13/reject_field
target/release/tsuzuri check /tmp/tz-a13/capture_mut
target/release/tsuzuri check /tmp/tz-a13/reborrow_local
```

```text
/tmp/tz-a13/reject_field/Main.tz:1:8: error[E1013]: record fields cannot store mutable references; use a shared borrow
/tmp/tz-a13/capture_mut/Main.tz:3:12: error[E1005]: cannot capture ref mut i64 in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks
```

`reborrow_local` は出力なしで成功する（排他参照の束縛は `mut` なしで貸し直せる）。region を宣言した record を region なしで使う
`record NamedView {r} { text: ref {r} string }` と `def len_of :: NamedView -> i64` も `check` に成功した。
既存 fixture のコピー（`tests/fixtures/borrowed_records/Main.tz`）は `build --emit llvm` と `-O3` で IR（2114 行）を出せる。

## 仕様

### 前提とする他チケットのインターフェース

A12 Phase 1 が次を提供することを前提にする。名前は仮で、A12 の実名に読み替える。

- I1: `CheckedRecord` が、フィールドごとに型へ現れる region の宣言内添字を出現順で持つ（本チケットでは `field_regions: Vec<Vec<usize>>`（A12）と呼ぶ）。
  `ref mut {r} T` なら `[r]`、`Split {r s}` の適用なら `[r, s]` の添字、region を書かないフィールドは空。宣言順の region 名（本チケットでは `region_names`（A12））も持つ。
- I2: 使用位置の region 適用（`Counter {r}`、`Split {r s}`）は宣言順に対応し、省略は全 region を一つの交差寿命とみなす（A09 互換）。
- I3: 所有権検査で値の loan が region ごとに区別され、`E::Field` の読み出しはそのフィールドの region の loan だけを返す。
  record 全体の move・捕捉・Task・合流・ループ固定点は全 region の和を使う。
- I4: `check_body` のパラメーターの外部 loan は、region ごとに分かれていても、パラメーターで一つでもよい。本チケットはその全外部 loan に同じ `mutable` を与える（D6）。

### 構文

新しい字句・構文はない。A09 の region 付き参照型をフィールドに書けるようにする。

```text
field       = field_name ":" field_type
field_type  = ("ref" "mut" | "&mut") region type        (* 排他借用フィールド。region は必須 *)
            | record_name [type_arguments] regions       (* 排他 region を持つ record。適用は必須 *)
            | 既存のフィールド型
region      = "{" region_name "}"
regions     = "{" region_name {[","] region_name} "}"
```

### 型規則

- R1: record フィールドの型の最上位が `ref mut {r} T` なら排他借用フィールドとし、`r` を排他 region とする。region のない `ref mut T` は M1。
- R2: フィールドの型が排他 region を持つ record `S` なら region 適用が必須（なければ M3）。`S` の排他 region の位置に置いた外側の region も排他 region になる。
- R3: 排他 region は record 内でちょうど一つのフィールドの一つの位置だけが使う。他のフィールド（共有・排他）や同じ適用の別の位置と共有すれば M2。
- R4: 排他借用フィールドの参照先 `T` は借用を持たない（`!T.carries_loans(types)`）。宣言時の違反は M4。`T` が型変数なら具体化時に検査し、違反は M6。
- R5: R1・R2 以外の格納位置（配列・リスト・Vec・タプル・union payload・共有参照の内側）で排他参照へ届くフィールドは M5。
  型変数のフィールドを排他参照や排他 record で具体化すると、既存の `would store a mutable reference in field` の E1013。
- R6: 排他借用を持つ record は非 Copy・Capture 不可・Send 不可（既存の `Type::is_copy`・`can_capture`・`can_send` による）。
  配列・リスト・Vec の要素（E1005）、union payload（E1013）、`export` の引数・結果（E1008）にはできない。local・引数・戻り値・タプルの成分にはできる。
- R7: `c.f` が排他借用フィールドなら型は `ref mut T`。`deref c.f` は `T`、`ref mut c.f` は一段の貸し直し（`Borrow(Dereference(Field(c, f)), true)`）、
  `ref c.f` は共有の貸し直しで、呼び出し時の補完も同じ。共有借用フィールドに対する `ref mut` と代入は既存の E1014。
- R8: region は検査専用のまま。単相化キー・`Type`・LLVM 型に含めない。

### 評価順序・所有権・借用

- O1: `Counter { value: ref mut total, step: 1 }` は `total` の排他 loan を作り、record の値がそれを持つ。record が生きている間、`total` への他のアクセスは E1014。
  record の最後の使用の後は NLL（`retain_live_loans`）で解放される。
- O2: `ref mut c.f`・`deref c.f = v`・呼び出し時の補完は、`c` が `let`（非 mut）の束縛でもよい（D5）。貸し直しの loan は record の loan を parents に持ち、
  寿命はその最後の使用までの匿名 region（`r` より短い）。`c` が外部由来（パラメーター）なら、貸し直しを関数の結果として返せる。
- O3: 貸し直しの生存中は、同じ参照先への経路（`deref c.f`、`c.f` の再貸し直し）は E1014 `access conflicts with a live borrow`、
  `c` 全体または `c.f` の move は E1014 `cannot move an exclusive reference while it is reborrowed`。他のフィールドの読み出し、`ref c`（共有借用）、
  `let mut c` の再代入は許す（D5）。再代入の後も、貸し直しの parents が旧 loan を active に保つ。
- O4: `deref c.f = v` は右辺、代入先の順に評価し、参照先の値全体を置換して旧値を解放する（既存の参照経由代入と同じ drop 経路）。
- O5: record の move（`let d = c`、値引数、戻り値）は loan を移す。部分 move（`let v = c.f`、ガードのない所有値のパターン）は `c.f` を moved にし、
  残りのフィールドは使える。`c` 全体と `c.f` の再使用は E1012 `use of moved or partially moved value`。
- O6: 共有参照（`ref Counter` の引数、`let view = ref counter`）を通して排他フィールドを変更・排他貸し直しすると
  E1014 `cannot mutate or exclusively reborrow through a shared reference`（D7）。`ref mut Counter` を通すなら許す。
- O7: パターン変数が record 内の排他借用へ届く型を view として束縛する場合（ガード中、参照先・要素の照合）は M7（D8）。
- O8: パラメーター（匿名関数のパラメーターを含む）が `ref mut T` か排他 region を持つ record なら、その外部 loan は mutable（D6）。
- O9: 再利用可能な関数値の捕捉は E1005、Task の捕捉・結果は E1013（既存）。一回実行の Task・並列 callback を含め、境界を越えて渡せない。
- O10: 分岐の合流・ループ固定点・関数結果の region 検査（`region_sources`）は既存（A12 拡張後）のまま。排他 loan の親子関係は `Loan.parents` にあるので合流で失われない。

### 数値・トラップ・native と WASM の差

なし。排他借用フィールドは共有借用フィールドと同じ `ptr` で、置換は既存の参照経由代入の drop を使う。新しいトラップ・ランタイム関数・WASM import はない。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1013 | M1: region のない `ref mut T` フィールド | `exclusive borrow field '{field}' needs a named region; declare one after the record name and write 'ref mut {r} T'` | record 名 |
| E1013 | M2: 排他 region を二つ以上の位置で使う | `region '{region}' belongs to an exclusive borrow, so only one field position may use it; give field '{field}' its own region` | record 名（`{field}` は 2 番目に使うフィールド） |
| E1013 | M3: 排他 record のフィールドに region 適用がない | `field '{field}' stores an exclusive borrow inside record '{record}'; apply its regions, for example '{record} {r}'` | record 名 |
| E1013 | M4: 参照先が借用を持つ（宣言時） | `exclusive borrow field '{field}' must point to data without borrows; store borrowed parts in separate fields` | record 名 |
| E1013 | M5: 許可位置以外で排他参照へ届く（旧文を置換） | `record fields can hold an exclusive reference only as a direct 'ref mut {r} T' field or inside a record field; arrays, lists, Vec, tuples, union payloads, and shared references cannot hold one` | record 名 |
| E1013 | M6: 型変数の参照先を借用を持つ型で具体化 | `{type} would store borrowed data behind the exclusive field '{field}'; exclusive borrow fields must point to data without borrows` | `Validation::check` の `span` |
| E1013 | M7: record 内の排他借用のパターン view | `pattern variable '{name}' cannot view an exclusive borrow stored in a record; bind it without a guard to move it out, or use 'value.field' instead` | `view` の `projection.span` |
| E1014 | M8: 共有参照経由の変更・排他貸し直し（新しい検出位置。文は既存） | `cannot mutate or exclusively reborrow through a shared reference` | `E::Borrow`／`E::Assign` の式 |
| E1013 | 既存: 型変数フィールドを排他参照・排他 record で具体化 | `{type} would store a mutable reference in field '{name}'; use a shared borrow` | 変更なし |
| E1013 | 既存: union payload | `union payloads cannot store mutable references; {type} would store one in case '{name}'; use a shared borrow` | 変更なし |
| E1005 | 既存: 配列・リスト・Vec の要素 | `array and list elements cannot contain mutable references; collections are deeply immutable` | 変更なし |
| E1005 | 既存: 再利用可能な関数値の捕捉 | `cannot capture {type} in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks` | 変更なし |
| E1013 | 既存: Task の捕捉・結果 | `task captures cannot retain borrowed values, including borrowed function environments`／`task results cannot retain borrowed values` | 変更なし |
| E1013 | 既存: 局所値の借用の返却、region 不一致 | `cannot return a reference to a local value`／`returned borrow does not match the declared result region; return a borrow from an input with that region` | 変更なし |
| E1013 | 既存: 参照経由の borrowed aggregate 置換（Phase 2） | `assigning borrowed values through references requires explicit lifetimes` | 変更なし |
| E1014 | 既存: 競合、貸し直し中の move、フィールドへの代入 | `access conflicts with a live borrow; ...`／`cannot move an exclusive reference while it is reborrowed`／`mutable access requires 'let mut' or an exclusive reference ...` | 変更なし |
| E1012 | 既存: move 後の使用、参照からの move | `use of moved or partially moved value '{name}'`／`cannot move a non-Copy value out of a reference` | 変更なし |
| E1008 | 既存: 借用を持つ record の export | 既存の文 | 変更なし |

### 資源上限

新しい上限はない。region の個数は A12 の上限に従う。`exclusive_regions` の固定点は全 record の region 数の和 + 1 回以内で止まり、
型の走査は `seen` 付きの明示スタックなので、既存の型サイズ上限（`bounded_type`）の範囲に収まる。

### 例

新構文（実装後に有効。未検証）:

```tsuzuri
record Counter {r} { value: ref mut {r} i64, step: i64 }

def add_to :: ref mut i64 -> i64 -> unit = \target amount -> { deref target = deref target + amount; }

let mut total = 1
let counter = Counter { value: ref mut total, step: 20 }
add_to counter.value counter.step
let again = ref mut counter.value
deref again = deref again + 1
total
```

結果は `22`（1 + 20 + 1）。最後の行では `counter` と `again` が死んでいるので `total` を読める。拒否の例は「テスト計画」の T2・T3 にまとめる。

### Phase 2（設計方針）

- 参照先が借用を持つ排他借用フィールド（`ref mut {r} View {s}`）と、参照経由の borrowed aggregate 置換（`deref target = View { .. }`）を、
  region の包含関係（`s` が `r` より長い）を検査して許可する。region 間の outlives 関係の表現が要るので、A12 の後の別設計とする。
- union payload・コレクション要素の排他参照は Phase 2 でも対象外。

## 設計

### データ構造

```rust
// src/check.rs
pub struct CheckedRecord {
    // 既存フィールドは変えない
    /// Region indices that hold exclusive borrows (direct `ref mut {r}` fields or nested exclusive records).
    pub(crate) exclusive_regions: BTreeSet<usize>, // 新規
}

impl Type {
    /// True when an exclusive reference is reachable through stored values, records, or shared references.
    pub(crate) fn reaches_exclusive(&self, types: &TypeContext<'_>) -> bool; // 新規
    /// True when a record with exclusive regions is reachable through stored values, records, or references.
    pub(crate) fn holds_exclusive_record(&self, types: &TypeContext<'_>) -> bool; // 新規
}

/// Fixed point of `CheckedRecord::exclusive_regions`, indexed like the record table.
fn exclusive_regions(records: &[CheckedRecord]) -> Vec<BTreeSet<usize>>; // 新規
```

```rust
// src/ownership.rs
/// Exclusive references and records holding exclusive borrows allow writes through their external loans.
fn exclusive_parameter(ty: &Type, module: &CheckedModule) -> bool; // 新規

impl Checker<'_> {
    /// Whether a mutated place dereferences a shared reference to an exclusive record.
    fn through_shared_exclusive(&self, place: &TypedExpr) -> bool; // 新規
}

// src/ownership_control.rs（`view` と同じ impl）
/// Whether a pattern projection passes through a record with exclusive regions.
fn projects_exclusive_record(&self, projection: &TypedExpr) -> bool; // 新規
```

`Loan`・`Value`・`Place`・`Use`・`Type`・`TypedExprKind` は変えない（D6）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 型の性質 | `src/check.rs` | `Type::reaches_exclusive`（新規）, `Type::holds_exclusive_record`（新規） | `contains_stored_mutable_reference` の近くに置く。`stored_all` と同じ明示スタックと `seen` で、共有・排他の参照の内側にも進む。関数型と `Task` の内側へは進まない |
| record 情報 | `src/check.rs` | `CheckedRecord` と全構築箇所 | `exclusive_regions` を追加し、構築箇所（`grep -n "CheckedRecord {" src/*.rs`）では `BTreeSet::new()` |
| 排他 region | `src/check.rs` | `exclusive_regions`（新規）、record フィールド検査ループの直前 | `let types = TypeContext { .. }` の前に固定点を計算し、各 `CheckedRecord` に代入する |
| 宣言検査 | `src/check.rs` | record フィールド検査ループ | `contains_stored_mutable_reference` の判定を「アルゴリズム」の分類（M1・M3・M4・M5）に置き換え、record ごとに M2 を検査する |
| 具体化検査 | `src/check.rs` | `Validation::check` の `Type::Record` 分岐 | 宣言側の型で分岐する（M6、既存文の維持）。union・コレクション分岐は変えない |
| 貸し直しの型付け | `src/check.rs` | `reborrow_operand`, `require_mutable_reference`, `coerce_argument` | 変更なし |
| 外部 loan | `src/ownership.rs` | `check_body`, `exclusive_parameter`（新規） | `mutable = exclusive_parameter(&parameter.ty, module)`。A12 が外部 loan を region ごとに分けていても全部に同じ値 |
| 共有経由の排他 | `src/ownership.rs` | `Checker::through_shared_exclusive`（新規）、`Checker::eval_value` の `E::Borrow`・`E::Assign` | `E::Borrow(value, true)` と `E::Assign(place, _)` の arm 先頭で検査して M8 |
| パターン view | `src/ownership_control.rs` | `view`, `projects_exclusive_record`（新規） | `view` の先頭で M7 |
| 読み出し・move・NLL | `src/ownership.rs` | `read_places`, `access`, `loan`, `retain_live_loans`, `active`, `eval_value` の `E::Record`・`E::Field` | 変更なし（A12 の region 別 loan を使う） |
| 合流・ループ | `src/ownership.rs`, `src/ownership_control.rs` | `Checker::merge`、ループ固定点 | 変更なし |
| region 宣言・契約 | `src/regions.rs` | `validate_modules`, `labels`, `contract` | 変更なし（A12 の変更のまま） |
| Capture・Send・Copy | `src/check.rs`, `src/polymorph.rs` | `Type::is_copy`, `can_capture`, `can_send`、捕捉の E1005 | 変更なし（record のフィールドを辿る） |
| 導出 | `src/derive.rs` | `record` | 変更なし（フィールドを共有の `ref` で渡す） |
| LLVM | `src/llvm.rs` | `Type::Reference(..) => "ptr"` | 変更なし |
| テスト | `tests/borrowed_records.rs`, `tests/fixtures/borrowed_records/Main.tz`, `tests/features.mjs` | 3 関数（新規）、fixture 関数、suite ケース | 「テスト計画」 |

### 生成 IR とランタイム

- 変更なし。排他借用フィールドは `ptr` で、record の LLVM 構造体・フィールド順・サイズは、同じ宣言を `ref {r} T` で書いた場合と同じになる。
- `deref c.f = v` は既存の参照経由代入の IR を使う。record の drop は参照フィールドを解放しない（`Type::needs_drop` は参照で false）。
- 既存 fixture の IR は byte 単位で変わらない（手順 7 で `cmp`）。

### アルゴリズム

#### 型の走査

`reaches_exclusive` は `stored_all` と同じ `seen` 付きの明示スタック（`let mut pending = vec![self.clone()]`）で型を辿り、
`Reference(_, true)` に会えば true を返す。`Reference(inner, false)`・`Array`・`List`・`Vec` は `inner`、`Tuple` は成分、
`Record(id, arguments)` は `types.record_fields(id, &arguments)`、`Union(id, arguments)` は `types.union_payloads(id, &arguments)` の payload を積む。
`Function` と `Task` は呼び出しの型で格納された参照ではないので積まない。`holds_exclusive_record` は同じ走査で、`Reference(inner, _)` は
どちらも `inner` を積み、`Record(id, _)` で `types.records[id].exclusive_regions` が空でなければ true を返す。

#### 排他 region の固定点

```text
exclusive = records.len() 個の空集合
loop:
    changed = false
    for (id, record) in records（id 順）:
        for (index, (_, ty)) in record.fields（宣言順）:
            regions = record.field_regions[index]                  -- I1（A12）
            positions = ty が Reference(_, true) なら [0]
                        ty が Record(inner, _) なら exclusive[inner] の要素（先に Vec へ写す）
                        それ以外は []
            for position in positions:
                regions[position] があれば exclusive[id] に入れ、増えたら changed = true
    changed でなければ exclusive を返す
```

id 順・フィールド順の走査で決定的。集合は単調に増え、各 record の region 数で頭打ちになる。region のないフィールドは何も足さず、後の M1・M3 で拒否される。

#### 宣言検査（record フィールド検査ループ）

```text
for record in records（id 順）:
    for (index, (field, ty)) in record.fields:
        regions = record.field_regions[index]
        match ty:
            Reference(inner, true):
                regions が空                  → M1、continue
                inner.carries_loans(types)    → M4、continue
            Record(inner_id, _) で records[inner_id].exclusive_regions が空でない:
                regions が空                  → M3、continue
            それ以外:
                ty.reaches_exclusive(types)   → M5、continue
        validate_size(ty, ..)（既存）
    for region in record.exclusive_regions:
        各フィールドの field_regions に region が現れる回数を位置ごとに数える
        合計が 2 以上 → M2（{field} は 2 回目に現れたフィールド、{region} は region_names[region]）
```

排他 region を持たない record 型のフィールドは「それ以外」へ進むので、`Holder<ref mut i64>` のような既存の拒否は同じコード E1013 のまま（文は M5）。

#### 具体化検査（`Validation::check` の `Type::Record`）

```text
for ((name, declared), field) in record.fields.zip(types.record_fields(id, args)):
    bounded_type(field)（既存）
    match declared:
        Reference(_, true):
            field が Reference(inner, true) で inner.carries_loans(types) → M6
        Record(inner_id, _) で exclusive_regions が空でない:
            何もしない（直後の self.check(field) が入れ子の具体化を検査する）
        それ以外:
            field.contains_stored_mutable_reference(types) → 既存の文（変更しない）
    self.check(field)（既存）
```

#### 外部 loan と共有経由の排他

`exclusive_parameter` は `Reference(_, mutable)` なら `mutable`、`Record(id, _)` なら `!module.types().records[*id].exclusive_regions.is_empty()`、
それ以外（タプルを含む）は false を返す。`through_shared_exclusive` は次の形。

```rust
fn through_shared_exclusive(&self, mut place: &TypedExpr) -> bool {
    let types = self.module.types();
    loop {
        place = match &place.kind {
            E::Dereference(reference) => {
                if let Type::Reference(inner, false) = &reference.ty {
                    if inner.holds_exclusive_record(&types) {
                        return true;
                    }
                }
                reference
            }
            E::Field(value, _) | E::Index(value, _) | E::ListTail(value, _) | E::UnionPayload { value, .. } => value,
            _ => return false,
        };
    }
}
```

`E::Borrow(value, mutable)` の arm は `*mutable && self.through_shared_exclusive(value)` なら、`E::Assign(place, _)` の arm は
`self.through_shared_exclusive(place)` なら、`error("E1014", "cannot mutate or exclusively reborrow through a shared reference", expression.span)`
を返す。最外の共有参照は型検査器が先に拒否するので、ここで重ねて検出しても差はない。

#### パターン view

`view` の先頭で、`local.ty.reaches_exclusive(&types) && self.projects_exclusive_record(projection)` なら M7 を返す。
`projects_exclusive_record` は `projection` から `E::Field`・`E::Index`・`E::ListTail`・`E::UnionPayload`・`E::Dereference` の被演算子を順に辿り、
途中のどれかの式の型（`projection` 自身を含む）が `holds_exclusive_record` なら true。
スカラーのフィールド（`step`）やタプルだけの経路（HEAD で使えるもの）は条件に入らない。

#### 追跡: 既存機構で成り立つ挙動

`E1014` の行は、その行だけを `again` の生存中に置いた場合の結果である。

```text
let mut total = 1
let counter = Counter { value: ref mut total, step: 20 }  -- L0 = loan(total, mutable, {})。counter.loans = {L0}
let again = ref mut counter.value     -- place(Deref(Field(counter, value))): Field の読み出しは {L0}（I3: region r だけ）
                                      -- Deref → (total, via {L0})。MutBorrow は via が全て mutable なので可
                                      -- L1 = loan(total, mutable, {L0})。again.loans = {L1}
add_to counter.value 1                -- 補完の Borrow(Deref(Field)) → (total, via {L0})。active の L1 と重なる → E1014
let moved = counter                   -- read_places(Consume): L1.parents ∩ {L0} ≠ ∅ → E1014 cannot move ... reborrowed
let step = counter.step               -- counter.[step] は L0・L1 の place（total）と重ならない → 可
deref again = 2                       -- (total, via {L1, L0}) への Write → 可
total                                 -- again・counter が死ねば retain_live_loans の後 L0・L1 は active でない → 可
```

M8 が要る理由も同じ追跡で分かる。`let view = ref counter` の後の `add_to view.value 1` は、`read_places` が `view.value` の値として
根 local `counter` の loan `{L0}` だけを返し、共有 loan を落とすので、M8 がなければ `total` への排他貸し直しが通る。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。N0 は手順 1 で記録する `borrowed_records` の件数（HEAD では 5。A12 が足した分を含む）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドと着手条件の A12 確認を行い、既存 fixture のコピーの IR と各テストの件数を保存する。
- 確認: 次がすべて成功する。`borrowed_records` の件数を N0、`generic_records` と `types_ownership` の件数も記録する。stack-depth の 3 テストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-a13/base
cp tests/fixtures/borrowed_records/Main.tz /tmp/tz-a13/base/Main.tz
target/release/tsuzuri build /tmp/tz-a13/base --emit llvm -o /tmp/tz-a13/base-O0.ll
target/release/tsuzuri build /tmp/tz-a13/base --emit llvm -O3 -o /tmp/tz-a13/base-O3.ll
cargo test --locked --test borrowed_records
cargo test --locked --test generic_records
cargo test --locked --test types_ownership
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
```

### 手順 2: 宣言と具体化の検査（共有変更）

- 変更: `src/check.rs` の `Type::reaches_exclusive`（新規）、`Type::holds_exclusive_record`（新規）、`CheckedRecord::exclusive_regions`（新規）と全構築箇所、
  `exclusive_regions`（新規）、record フィールド検査ループ、`Validation::check` の `Type::Record` 分岐。`tests/borrowed_records.rs` に T2。
- 内容: 「アルゴリズム」の走査・固定点・宣言検査・具体化検査を実装する。宣言と具体化は同じ手順で変える
  （宣言だけを許すと、`Validation::check` が `Counter` の使用を既存の `would store a mutable reference` で拒否する）。
- 確認: `cargo test --locked --test borrowed_records` が `N0 + 1 passed`。`cargo test --locked --test generic_records` が手順 1 と同じ件数で成功。
  `cargo test --locked` が成功する。

### 手順 3: パラメーターの外部 loan と既存機構の確認

- 変更: `src/ownership.rs` の `check_body` と `exclusive_parameter`（新規）。`tests/borrowed_records.rs` に T1 の全件と、T3 の 1〜7・11〜13・15・16。
- 内容: 外部 loan の `mutable` を `exclusive_parameter(&parameter.ty, module)` にする。A12 が外部 loan を region ごとに作っていれば、
  そのパラメーターの全外部 loan に同じ値を使う。他の所有権コードは変えない。ここで足すケースは既存機構だけで期待どおりになるはずで、ならなければ停止条件。
- 確認: `cargo test --locked --test borrowed_records` が `N0 + 3 passed`。`cargo test --locked --test types_ownership` が手順 1 と同じ件数で成功。

### 手順 4: 共有参照経由の排他アクセス（M8）

- 変更: `src/ownership.rs` の `Checker::through_shared_exclusive`（新規）と `Checker::eval_value` の `E::Borrow`・`E::Assign`。T3 に 8〜10。
- 内容: 先に T3 の 8〜10 を足し、受理されて失敗することを見てから「アルゴリズム」のとおり実装する。走査は `eval_value` の外の関数に置く。
- 確認: `cargo test --locked --test borrowed_records` が `N0 + 3 passed`。

### 手順 5: パターン view（M7）

- 変更: `src/ownership_control.rs` の `view` と `projects_exclusive_record`（新規）。T3 に 14。
- 内容: `view` の先頭で検査する。`view` の既存の処理（`access`、`Loan` の作成、`aliases`）は変えない。
- 確認: `cargo test --locked --test borrowed_records` が `N0 + 3 passed`。T1 の 10（ガードなしの `match`）が引き続き受理される。

### 手順 6: E2E fixture と suite

- 変更: `tests/fixtures/borrowed_records/Main.tz`（「E2E」の record 6 個、helper、export 関数 14 個を末尾に追加）、`tests/features.mjs` の `borrowed_records` suite。
- 内容: 「E2E」のコードとケースを足す。既存の宣言・関数・ケースは変えない。
- 確認: 次が成功し、新旧の全ケースが suite の既存の実行形態（native/WASM × `-O0`/`-O3`）で通る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
node tests/features.mjs target/release/tsuzuri borrowed_records
```

### 手順 7: 既存 IR の不変と決定性

- 変更: なし。
- 内容: 手順 1 の fixture コピーを新しいコンパイラで出し直して比べる。新しい fixture も 2 回出して比べる。
- 確認: 3 つの `cmp` がすべて無出力で終了コード 0。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-a13/base --emit llvm -o /tmp/tz-a13/after-O0.ll
target/release/tsuzuri build /tmp/tz-a13/base --emit llvm -O3 -o /tmp/tz-a13/after-O3.ll
cmp /tmp/tz-a13/base-O0.ll /tmp/tz-a13/after-O0.ll
cmp /tmp/tz-a13/base-O3.ll /tmp/tz-a13/after-O3.ll
mkdir -p /tmp/tz-a13/new
cp tests/fixtures/borrowed_records/Main.tz /tmp/tz-a13/new/Main.tz
target/release/tsuzuri build /tmp/tz-a13/new --emit llvm -O3 -o /tmp/tz-a13/new-1.ll
target/release/tsuzuri build /tmp/tz-a13/new --emit llvm -O3 -o /tmp/tz-a13/new-2.ll
cmp /tmp/tz-a13/new-1.ll /tmp/tz-a13/new-2.ll
```

### 手順 8: stack-depth と全体

- 変更: なし（失敗したら原因の新関数だけを直す）。
- 確認: 手順 1 の stack-depth 3 テストがそれぞれ `1 passed`。GUIDE §3 の検証コマンド（整形・lint・`cargo test --locked`）が成功する。

### 手順 9: ドキュメント

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/language-reference/lifetimes.md _docs/language-reference/ownership.md _docs/feature-status.md _docs/learn/why-tsuzuri.md`
  が成功し、`git diff --check` が無出力。

### 手順 10: 最終確認

- 変更: なし。
- 確認: `cargo test --locked` と `node tests/features.mjs target/release/tsuzuri borrowed_records` をもう一度実行して成功し、受け入れ条件を一つずつ確かめる（GUIDE §10）。

## テスト計画

### Rust テスト

すべて `tests/borrowed_records.rs` に置く。ソースは新構文（実装後に有効。未検証）。T1・T3 は共通の前置き `EXCLUSIVE`（新規の定数）を付ける。

```rust
const EXCLUSIVE: &str = "record Counter {r} { value: ref mut {r} i64, step: i64 }\nrecord Meter {r} { counter: Counter {r}, scale: i64 }\nrecord Split {r s} { left: ref mut {r} i64, right: ref mut {s} i64 }\nrecord Mixed {r s} { value: ref mut {r} i64, name: ref {s} string }\nrecord Slot<'a> {r} { value: ref mut {r} 'a }\ndef add_to :: ref mut i64 -> i64 -> unit = \\target amount -> { deref target = deref target + amount; }\n";
```

T1 `exclusive_borrow_fields_accept_reborrows_moves_and_regions`（新規）は、各本体について `analyze(&format!("{EXCLUSIVE}{body}"))` が成功し、
`llvm::emit_target(&module, llvm::Entry::Library, wasm)` を wasm が false・true それぞれで 2 回出して一致することを確かめる
（`shared_borrowed_fields_nesting_and_generic_views_are_supported` と同じ形）。値は実行しないが、手計算の結果をコメントに残す。

```rust
for body in [
    "let mut total = 1\nlet counter = Counter { value: ref mut total, step: 2 }\nadd_to counter.value counter.step\nadd_to (ref mut counter.value) 3\nlet again = ref mut counter.value\nderef again = deref again + 1\ntotal", // 1: 非 mut 束縛からの貸し直し（D5）・補完・NLL。値 7
    "def advance :: Counter -> Counter = \\counter -> { add_to counter.value counter.step; counter }\nlet mut total = 0\nlet counter = advance (advance (Counter { value: ref mut total, step: 4 }))\nlet step = counter.step\nstep + total", // 2: 値パラメーターの外部 loan（O8）。値 12
    "def bump :: ref mut Counter -> unit = \\counter -> { deref counter.value = deref counter.value + counter.step; }\nlet mut total = 0\nlet mut counter = Counter { value: ref mut total, step: 5 }\nbump (ref mut counter)\nbump (ref mut counter)\nlet step = counter.step\nstep + total", // 3: ref mut Counter 経由。値 15
    "def counter_of {r} :: ref mut {r} i64 -> Counter {r} = \\value -> Counter { value: value, step: 7 }\nlet mut total = 0\nlet counter = counter_of (ref mut total)\nadd_to counter.value counter.step\ntotal", // 4: region 付きの結果。値 7
    "def value_of {r} :: Counter {r} -> ref mut {r} i64 = \\counter -> counter.value\nlet mut total = 3\nlet value = value_of (Counter { value: ref mut total, step: 1 })\nderef value = 9\ntotal", // 5: パラメーターからの部分 move を返す。値 9
    "let mut total = 2\nlet meter = Meter { counter: Counter { value: ref mut total, step: 5 }, scale: 3 }\nadd_to meter.counter.value (meter.counter.step * meter.scale)\ntotal", // 6: 入れ子。値 17
    "let mut left = 10\nlet mut right = 20\nlet split = Split { left: ref mut left, right: ref mut right }\nlet held = ref mut split.left\nadd_to split.right 5\nderef held = deref held + 1\nleft * 100 + right", // 7: 別 region の同時貸し直し（I3）。値 1125
    "let mut total = 0\nlet name = \"abc\"\nlet mixed = Mixed { value: ref mut total, name: ref name }\nlet held = ref mut mixed.value\nadd_to held mixed.name.length\ntotal", // 8: 排他 region の貸し直し中に共有 region を読む（I3）。値 3
    "let mut a = 1\nlet mut b = 2\nlet mut counter = Counter { value: ref mut a, step: 0 }\nlet held = ref mut counter.value\ncounter = Counter { value: ref mut b, step: 0 }\nderef held = 10\nadd_to counter.value 20\na + b", // 9: 貸し直し中の再代入（D5）。値 32
    "let mut total = 5\nlet counter = Counter { value: ref mut total, step: 6 }\nmatch counter with | Counter { value: target, step: amount } -> add_to target amount\ntotal", // 10: ガードなしのパターン（部分 move）。値 11
    "let mut total = 1\nlet slot = Slot { value: ref mut total }\nderef slot.value = 8\ntotal", // 11: ジェネリックな参照先。値 8
    "let mut text = \"short\"\nlet slot = Slot { value: ref mut text }\nderef slot.value = \"a longer text\"\ntext.length", // 12: 非 Copy の参照先の置換。値 13
] { /* analyze と IR の決定性 */ }
```

T2 `exclusive_borrow_field_declarations_are_validated`（新規）は前置きなしの `(source, code, fragment)` の表で、`analyze(source).unwrap_err()` の
コードと、`fragment` が空でなければメッセージの部分一致を確かめる（`tests/generic_records.rs` の同じ形の表の assert を写す）。

```rust
[
    ("record Bad { value: ref mut i64 }", "E1013", "exclusive borrow field 'value' needs a named region"),
    ("record Bad {r} { first: ref mut {r} i64, second: ref mut {r} i64 }", "E1013", "region 'r' belongs to an exclusive borrow"),
    ("record Bad {r} { value: ref mut {r} i64, name: ref {r} string }", "E1013", "give field 'name' its own region"),
    ("record View { text: ref string }\nrecord Bad {r} { value: ref mut {r} View }", "E1013", "exclusive borrow field 'value' must point to data without borrows"),
    ("record Bad {r} { values: [ref mut {r} i64] }", "E1013", "can hold an exclusive reference only"),
    ("record Bad {r} { value: Maybe<ref mut {r} i64> }", "E1013", "can hold an exclusive reference only"),
    ("record Counter {r} { value: ref mut {r} i64 }\nrecord Bad { counter: Counter }", "E1013", "field 'counter' stores an exclusive borrow inside record 'Counter'"),
    ("record Counter {r} { value: ref mut {r} i64 }\nrecord Bad {r} { counter: Counter {r}, name: ref {r} string }", "E1013", "give field 'name' its own region"),
    ("record Counter {r} { value: ref mut {r} i64 }\nrecord Bad {r} { view: ref {r} Counter }", "E1013", ""),
    ("record Slot<'a> {r} { value: ref mut {r} 'a }\ndef bad :: Slot<ref i64> -> i64\nfn bad slot = 0", "E1013", "would store borrowed data behind the exclusive field 'value'"),
    ("record Holder<'a> { value: 'a }\nrecord Counter {r} { value: ref mut {r} i64 }\ndef bad :: Holder<Counter> -> i64\nfn bad holder = 0", "E1013", "would store a mutable reference in field 'value'"),
    ("record Counter {r} { value: ref mut {r} i64 }\nlet mut total = 0\nlet all = [Counter { value: ref mut total }]\n0", "E1005", "array and list elements cannot contain mutable references"),
    ("record Counter {r} { value: ref mut {r} i64 }\nlet mut total = 0\nlet maybe = Some (Counter { value: ref mut total })\n0", "E1013", "union payloads cannot store mutable references"),
    ("record Counter {r} { value: ref mut {r} i64 }\nexport def bad :: Counter -> i64\nfn bad counter = 0", "E1008", ""),
]
```

`view: ref {r} Counter` はコードだけを見る。A12 が region を省略した入れ子の record を別の E1013 で先に拒否しても期待を満たす。
同じ理由で `record Bad { counter: Counter }` が M3 より先に A12 の E1013 で拒否された場合は、その行の断片を空にし、完了報告に書く。

T3 `exclusive_borrow_fields_preserve_exclusivity`（新規）は `EXCLUSIVE` を前置した `(body, code, fragment)` の表で、T2 と同じ検査をする。

```rust
[
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet seen = total\nadd_to counter.value 1\nseen", "E1014", "access conflicts with a live borrow"), // 1: record の生存中に所有者を読む
    ("let mut total = 0\nlet first = Counter { value: ref mut total, step: 1 }\nlet second = Counter { value: ref mut total, step: 1 }\nadd_to first.value 1\nadd_to second.value 1\ntotal", "E1014", "access conflicts with a live borrow"), // 2: 同じ所有者を二つの record が借用
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet moved = counter\nadd_to counter.value 1\nadd_to moved.value 1\ntotal", "E1012", "use of moved or partially moved value 'counter'"), // 3: move 後の使用
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet held = ref mut counter.value\nlet moved = counter\nderef held = 3\nadd_to moved.value 1\ntotal", "E1014", "cannot move an exclusive reference while it is reborrowed"), // 4: 貸し直し中の move
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet held = ref mut counter.value\nadd_to counter.value 1\nderef held = 3\ntotal", "E1014", "access conflicts with a live borrow"), // 5: 貸し直し中に同じフィールドを使う
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet taken = counter.value\nadd_to counter.value 1\nadd_to taken 1\ntotal", "E1012", "use of moved or partially moved value 'counter'"), // 6: let は補完しないので部分 move
    ("let mut total = 0\nlet name = \"abc\"\nlet mixed = Mixed { value: ref mut total, name: ref name }\nlet bad = ref mut mixed.name\n0", "E1014", "cannot mutate or exclusively reborrow through a shared reference"), // 7: 共有フィールド（型検査器の既存検査）
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet view = ref counter\nadd_to view.value 1\ntotal", "E1014", "cannot mutate or exclusively reborrow through a shared reference"), // 8: M8（手順 4）
    ("def bad :: ref Counter -> unit = \\counter -> add_to counter.value 1\n0", "E1014", "cannot mutate or exclusively reborrow through a shared reference"), // 9: M8
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet view = ref counter\nderef view.value = 5\ntotal", "E1014", "cannot mutate or exclusively reborrow through a shared reference"), // 10: M8（代入）
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet read = \\() -> counter.step\nread ()", "E1005", "cannot capture Counter in a reusable function"), // 11: 関数値の捕捉
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\ntask { return counter.step }", "E1013", ""), // 12: Task の捕捉
    ("def bad :: i64 -> Counter = \\seed -> { let mut total = seed; Counter { value: ref mut total, step: 1 } }\n0", "E1013", ""), // 13: 局所値の借用を返す
    ("let mut total = 0\nlet counter = Counter { value: ref mut total, step: 1 }\nlet result = match counter with | Counter { value: target } when deref target > 0 -> 1 | _ -> 0\nresult", "E1013", "cannot view an exclusive borrow stored in a record"), // 14: M7（手順 5）
    ("def replace :: ref mut Counter -> ref mut i64 -> unit = \\target value -> { deref target = Counter { value: value, step: 0 }; }\n0", "E1013", "assigning borrowed values through references requires explicit lifetimes"), // 15: Phase 2 のまま
    ("def bad {r s} :: ref mut {r} i64 -> ref mut {s} i64 -> Counter {r} = \\left right -> Counter { value: right, step: 0 }\n0", "E1013", "returned borrow does not match the declared result region"), // 16: region 不一致
]
```

12 と 13 はコードだけを見る。Task の拒否（Send の検査か所有権の検査か）と局所値の拒否（ブロック終端か戻り値か）は、どちらが先に出ても E1013 である。

### E2E

`tests/fixtures/borrowed_records/Main.tz` の末尾に次を追加する。新構文（実装後に有効。未検証）:

```tsuzuri
record Counter {r} { value: ref mut {r} i64, step: i64 }
record Meter {r} { counter: Counter {r}, scale: i64 }
record Split {r s} { left: ref mut {r} i64, right: ref mut {s} i64 }
record Mixed {r s} { value: ref mut {r} i64, name: ref {s} string }
record Slot<'a> {r} { value: ref mut {r} 'a }
record Buffer {r} { items: ref mut {r} Vec<i64> }

private def add_to :: ref mut i64 -> i64 -> unit
fn add_to target amount = { deref target = deref target + amount; }

private def append :: Buffer -> i64 -> Buffer
fn append buffer value =
    deref buffer.items = Vec.push (Vec.clone buffer.items) value
    buffer

export def exclusive_loop :: i64 -> i64
fn exclusive_loop count =
    let mut total = 0
    let counter = Counter { value: ref mut total, step: 3 }
    let mut index = 0
    while index < count do
        add_to counter.value counter.step
        index = index + 1
    let step = counter.step
    step + total

export def exclusive_vec :: i64 -> i64
fn exclusive_vec count =
    let mut items: Vec<i64> = Vec.empty()
    let mut buffer = Buffer { items: ref mut items }
    let mut index = 0
    while index < count do
        buffer = append buffer index
        index = index + 1
    let length = Vec.length buffer.items
    length * 1000 + Vec.length items

export def exclusive_11 :: i64
fn exclusive_11 =
    let mut total = 1
    let slot = Slot { value: ref mut total }
    deref slot.value = 8
    total
```

さらに T1 の本体 n（1〜12）ごとに `export def exclusive_<n> :: i64` を置く（11 番は上の形）。本体の各行を `fn exclusive_<n> =` の下へ 4 スペースで字下げする。
2〜5 番の先頭の `def f :: T = \x -> body` の行は関数の外へ出し、`private def f :: T` と `fn f x = body` の組にする（例: `fn value_of counter = counter.value`）。
T1 の Rust 文字列の `\\` は Tsuzuri では `\`、`\"` は `"` である。7 番の局所変数は、既存 fixture の関数 `first` と衝突しないよう `held` にしてある。

`tests/features.mjs` の `borrowed_records` suite の `cases` の末尾に追加する:

```javascript
...[0n, 1n, 10n, 10000n].map((count) => ["exclusive_loop", [count], 3n + 3n * count]),
...[0n, 1n, 5n, 1000n].map((count) => ["exclusive_vec", [count], 1001n * count]),
...[7n, 12n, 15n, 7n, 9n, 17n, 1125n, 3n, 32n, 11n, 8n, 13n].map((value, index) => [`exclusive_${index + 1}`, [], value]),
```

期待値はコンパイラの出力ではなく手計算による。`exclusive_loop` はループごとに `total` へ 3 を足し、最後に `step`（3）を足す（`3 + 3 * count`）。
`exclusive_vec` は長さ `count` の Vec を record 経由と所有者の両方で数える（`count * 1000 + count`）。置換のたびに旧 Vec を解放するので、確保追跡が意味を持つ。
`exclusive_<n>` は T1 のコメントの値（`"a longer text"` は 13 バイト）。suite の既存の検査（native/WASM × `-O0`/`-O3`、確保追跡の `live == 0` など）をそのまま適用する。
新しいトラップのケースはない。

### 既存テストへの影響

- 期待値の変更はない。`borrowed_records_preserve_owner_lifetimes_and_exclusivity` の `record Bad { value: &mut i64 }` は M1、
  `record Bad { value: [&mut i64] }` は M5 に文が変わるが、コードは E1013 のままで、テストはコードだけを見る。
- 同じテストの `Holder<&mut i64>` と `tests/generic_records.rs` の 4 件は、宣言側が型変数・配列なので既存の文のまま。
- 既存 fixture の IR は変わらない（手順 7）。

### 性能

なし。検査だけの変更で生成コードを変えないので、計測もしない。

## ドキュメント

- `docs/language.md`「Ownership / Borrowing」: 「共有借用フィールドを持つレコードは、…元所有者を越える返却、Taskへの送信、排他借用fieldを拒否します。」を、
  排他借用フィールドの規則（R1〜R7、O3・O6・O7 の要約）を含む段落へ書き換える。
- `docs/language.md`「名前付き Region」: 最後の行の後続段階から「排他借用field」を外し（「参照経由のborrowed aggregate置換」は残す）、例に `Counter` の宣言を 1 行足す。
- `_docs/language-reference/lifetimes.md`: 「借用を格納するレコード」の「排他借用フィールドは拒否します」を書き換え、「排他借用フィールド」節（新規）に
  「例」の実行可能なサンプル（結果 `22`）を置く。「現在の制限」の表の排他借用フィールドの行を「対応（region 必須。参照先は借用を持たない型。配列・union には置けない）」にする。
- `_docs/language-reference/ownership.md`「再借用」: `ref mut record.field` が一段の貸し直し（Rust の `&mut *record.field`）であることを表に 1 行足す。
- `README.md`: 「排他借用フィールドと、レコード内の独立した複数regionは未対応です。」から排他借用フィールドを外す（A12 の更新と合わせて文を整える）。
- `_docs/feature-status.md`: A13 の行を実装済みの表記へ変え、未対応一覧から「排他借用フィールド」を外す。
- `_docs/learn/why-tsuzuri.md`「Rust に対する劣位点」: 「可変スライス、排他借用フィールド、…」から排他借用フィールドを外す。
- `_features/README.md`: A13 の状態を done にし、比較表の Rust 行から「排他借用フィールド」を外す。
- GUIDE §9: 承認された D1・D5 の内容と日付を記録する（承認者が記録済みなら不要）。

## 受け入れ条件

- [ ] D1・D5 が承認され、GUIDE §9 に記録されている。
- [ ] 排他借用フィールドを持つ record を宣言・作成・貸し直し・置換・move・部分 move でき、T1 の 12 件が受理される。
- [ ] E2E の新ケースが native/WASM × `-O0`/`-O3` で期待値どおりで、suite の既存の検査（`live == 0` を含む）を通る。
- [ ] M1〜M8 と既存の診断が「診断」の表どおりに出る（T2 の 14 件、T3 の 16 件）。
- [ ] 所有者への同時アクセス、共有参照経由の変更、ガードの view、捕捉・Task・コレクション・union への格納を拒否する。
- [ ] 既存 fixture の IR が手順 1 のベースラインと byte 単位で一致し、既存テストの期待値が変わらない。
- [ ] `Type`・`TypedExprKind`・構文・`Loan`・`Value`・`Place` を変えていない。
- [ ] stack-depth の 3 テストが成功し、上限を変えていない。
- [ ] 「ドキュメント」の更新が済み、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- A12 の I3 がないと T1 の 7・8 と `exclusive_7`・`exclusive_8` が E1014 になる。`read_places` を変えて通そうとしない（停止条件）。
- `contains_mutable_reference` は record の内側へ、`stored_all` は参照の内側へ進まない。M5・M7・M8 を既存の述語で代用すると `ref Counter` を見落とす。
- 新しい走査を再帰で書かない（debug のテストは 2 MiB stack）。`stored_all` と同じ明示スタックと `seen` を使う。`eval_value` は深く再帰するので、
  M8 の走査は別関数に置き、`eval_value` に大きな局所変数を足さない。stack-depth テストが落ちたら新関数に `#[inline(never)]` を付けて確かめる。上限は上げない。
- `format!` の中の `{r}` は `{{r}}` と書く（M1・M3）。M5 は補間がないので文字列リテラルのまま `{r}`。M6 の `{type}` は既存の `would store` と同じく `ty.display(&types)`。
- `Validation::check` で宣言側の型を見ずに mutable 参照の検査を外すと、`Holder<ref mut i64>` と `Holder<Counter>` が通る。`tests/generic_records.rs` の 4 件と T2 で検出できる。
- `exclusive_regions` を `TypeContext` を作った後で `records` に書き込もうとすると借用が衝突する。固定点は `&records` だけで計算し、代入してから `TypeContext` を作る。
- 排他 region を持たない record 型のフィールドを「record だから」と M5 の検査から外さない。`Holder<ref mut i64>` のフィールドが通ってしまう。
- 外部 loan の mutable をタプル・共有参照のパラメーターへ広げない（D6）。`ref (ref mut T)` を含むタプルで共有参照経由の変更が通る。
- M8 を型検査器の `require_mutable_reference` に入れない。`TypeContext` を持たない静的関数で、記号形式の `&r` による既存の判定まで変えてしまう。
- ガード中の束縛は view、ガードのない所有値のパターンは部分 move。T1 の 10 はガードなし、T3 の 14 はガード付き。M7 が T1 の 10 まで拒否したら停止条件。
- 貸し直しの loan の parents は、record が死んでも親 loan を active に保つ。漏れと誤解して parents を切ると、再代入後の貸し直し（T1 の 9）が健全でなくなる。
- 呼び出し時の補完は `let` に効かない。`let v = c.f` は部分 move、`let v = ref mut c.f` が貸し直しである（T3 の 6 と T1 の 1）。
- fixture の `match` の layout で E0002 が出たら、`docs/language.md`「match と分解」の書き方に合わせる。期待値は変えない。
- Node 20 で BigInt の多い suite が V8 の `RepresentationChangerError` で落ちたら `npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri borrowed_records` を使う。
- scratch のプロジェクトは `/tmp/tz-a13/<case>/` に 1 ケース 1 ディレクトリで置く（E03 の再帰探索）。

## 対象外

- union payload・配列・リスト・Vec の要素・タプルのフィールド型・共有参照の内側の排他参照（D4、D11）。
- 借用を持つ参照先（`ref mut {r} View {s}`）と、参照経由の borrowed aggregate 置換（Phase 2）。
- 型変数の排他参照・排他 record による具体化（`Holder<Counter>`、`Holder<ref mut i64>`）。
- タプル型のパラメーターを通した排他参照の変更（既存のタプルと同じく共有の外部 loan）。
- 記号形式 `&r` の `ref (ref mut T)` を経由した変更の判定の見直し（HEAD のまま）。
- 内部可変性（F10）、複数スレッドからの共有、static region。

## 決定事項

### D1: 排他借用フィールドの許可（GUIDE D-28 の変更）

- 決定: record のフィールドに region 付きの排他参照 `ref mut {r} T`（`&mut {r} T`）と、それを持つ record を region 適用付きで置くことを許可する。
  GUIDE §9 D-28 の「排他参照fieldは禁止する」を「region 付きの直接フィールドと、その record を入れ子にしたフィールドに限り許可する」へ改める。
- 理由: Rust の `struct Parser<'a> { out: &'a mut Vec<u8> }` に相当する作業用 view がないことが比較上の劣位である。HEAD の loan 機構
  （`Loan.mutable`・`parents`・`retain_live_loans`）がフィールド経由の貸し直しをそのまま表せることを追跡で確かめ、追加は宣言検査と三つの限定的な検査で済む。
- 状態: 承認済み（2026-10-06、GUIDE D-38）

### D2: 構文と region の規則

- 決定: A09 の `ref mut {r} T` をそのまま使い、字句・構文・`Type` は変えない。排他借用フィールドは named region が必須（M1）。
  排他 region は record 内でちょうど一つのフィールドの一つの位置だけが使う（M2）。排他借用を持つ record をフィールドに置くときは region 適用が必須（M3）で、
  その位置の外側の region も排他 region として同じ一意性に従う。
- 理由: A12 の region 別 loan（I3）で、排他フィールドの読み出しがそのフィールドの loan だけを返すようにするため。region を共有すると同じ region の共有 loan が
  書き込みの via に混ざり、安全だが理解しにくい E1014 の誤拒否になる。旧版の「一つの排他 region は一つのフィールドだけが持てる」を位置まで含めて明確化した。
- 状態: 既定案（実装者はこの案に従う）

### D3: 参照先の型

- 決定: Phase 1 の `ref mut {r} T` の `T` は借用を持たない型に限る（M4）。型変数は宣言時に許し、具体化時に検査する（M6）。
  借用を持つ参照先と参照経由の borrowed aggregate 置換は Phase 2。
- 理由: HEAD の参照経由代入は借用を含む値を `assigning borrowed values through references requires explicit lifetimes` で拒否する。
  region 間の outlives 関係を表す機構がないまま参照先の借用を許すと、置換で寿命の短い借用を書き込める。
- 状態: 既定案。Phase 2 を D12〜D14 のとおり実装した（2026-10-06）。M4 の文には Phase 2 の書き方（`or name the target's region as in 'ref mut {r} T {s}'`）を足した

### D4: 排他参照を保持できる位置

- 決定: record フィールドの型の最上位が `ref mut {r} T` の場合と、排他 region を持つ record 型の場合だけ許可する。配列・リスト・Vec・タプル・union payload・
  共有参照の内側（M5）と、型変数の具体化（既存の文）は拒否する。local・引数・戻り値・タプルの成分としての値は許可する。
- 理由: 所有経路（フィールド→フィールド→`ref mut`）だけに限ると、パラメーターの外部 loan を mutable にしても、共有参照経由の変更は型検査器と M8 が拒否する（D6、D7）。
  コレクションは D-13 で深く不変である。
- 状態: 既定案（実装者はこの案に従う）

### D5: 貸し直し中の record の扱い（旧版草案の変更）

- 決定: (a) `ref mut c.f`・`deref c.f = v`・呼び出し時の補完は、`c` が `let`（非 mut）の束縛でも許可する。`ref mut c` と `c` の再代入には従来どおり `let mut` が要る。
  (b) 貸し直しの生存中に拒否するのは、同じ参照先への経路と、`c`・`c.f` の move だけとする。他のフィールドの読み出し、`ref c`、`let mut c` の再代入は許可する。
- 理由: 既存規則「可変参照自身の束縛は、参照先を置換するだけなら `mut` である必要はありません」と一致し、Rust も `let c = C { b: &mut v }; c.b.push(1)` と、
  貸し直し中の別フィールドの使用を許す。健全性は参照先の place 上の loan と `parents` で保たれ、既存機構だけで実現できる（「アルゴリズム」の追跡）。
- 見直し提案: 旧版草案の「`cursor` は `let mut` 所有値か `ref mut Cursor` である必要がある」と「貸し直しの生存中は `cursor` 全体とそのフィールドへのアクセスを
  `E1014` で拒否する」を外す（本決定）。旧版を採る場合は、(a) 経路の根 local の `mutable` を `E::Borrow`・`E::Assign` で検査する手順と、
  (b) フィールド経由の貸し直しで record の place にも排他 loan を作る手順を追加する。
- 状態: 承認済み（2026-10-06、GUIDE D-38）

### D6: 所有権モデル

- 決定: `Loan`・`Value`・`Place`・`Use` は変えない。排他性は既存の `Loan.mutable`、region の区別は A12 の region 別 loan で表す。
  フィールド経由の貸し直しは既存の `Borrow(Dereference(Field(..)))` と `Checker::loan` の parents、NLL は `retain_live_loans` と `active` のまま使う。
  パラメーターの外部 loan は `exclusive_parameter` が true なら mutable にし、A12 が region ごとに分けていても全外部 loan に同じ値を与える。タプルのパラメーターは既存どおり共有。
- 理由: 旧版の「loan に排他・region を保持」は `Loan.mutable` と I3 で満たされる。外部 loan の root はそのパラメーターの射影からしか届かないので、
  mutable にしても他の経路と競合しない。共有参照の内側への変更は D4・D7 と型検査器が静的に拒否する。
- 状態: 既定案。実装では全外部 loan に同じ値を与えず、A12 の slot ごとに、排他 region の slot と `ref mut` の引数だけを mutable にした（D15）

### D7: 共有参照経由の排他アクセス

- 決定: 変更・排他貸し直しの place の経路上に、排他 region を持つ record へ届く共有参照の参照外しがあれば M8（E1014、既存の文）。
  検査は `src/ownership.rs` の `E::Borrow`（mutable）と `E::Assign` に置く。
- 理由: `read_places` は loan を持つ型の読み出しで根 local の loan を返して共有 loan を落とし、型検査器の `require_mutable_reference` は最外の参照外ししか見ない。
  排他 record に限ると、HEAD で書けるプログラム（記号形式の `&r` による `ref (ref mut T)` を含む）の判定は変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D8: パターンによる排他借用の view

- 決定: パターン変数の型が排他参照へ届き、projection の経路に排他 region を持つ record があるとき、`view` は M7（E1013）を返す。
  ガードのない所有値のパターンによる部分 move は許可する。
- 理由: view の参照外しは束縛元の記憶域を place とし、参照先の所有者に loan を作らない。フィールド経由の排他貸し直しと併用すると、
  参照先の置換中に view 由来の共有借用が生き残る。所有値の部分 move はフィールドの loan を移すので安全である。
- 状態: 実装時に撤回した（D16）。M7 は実装していない

### D9: Copy・clone・Capture・Send・Task・export

- 決定: 新しい規則は足さない。`Type::is_copy`・`can_capture`・`can_send` が record のフィールドを辿るので、排他借用を持つ record は自動的に非 Copy・
  Capture 不可・Send 不可になる。clone は Copy と関数値環境の複製だけで、どちらにも経路がない。export は既存の E1008。導出（`src/derive.rs`）は変えない。
- 理由: 型性質の定義を二重にすると食い違いの原因になる。
- 状態: 既定案（実装者はこの案に従う）

### D10: 診断コードとメッセージ

- 決定: 新しいコードは作らず、E1005・E1008・E1012・E1013・E1014 を使う（「診断」の表）。record フィールド検査の旧文
  `record fields cannot store mutable references; use a shared borrow` は M5 に置き換える。`Validation::check` の `would store a mutable reference in field` の文は変えない。
- 理由: 旧文は排他借用フィールドを許した後は誤りになる。旧文を検査するテストはなく、`would store ...` は `tests/generic_records.rs` が検査している。
- 状態: 既定案（実装者はこの案に従う）

### D11: 範囲（旧未決事項）

- 決定: union payload・コレクション要素・タプルのフィールド型の排他参照は対象外のまま（旧未決事項の既定案を確定）。Phase 2 は人間が求めた場合だけ別段階で設計する。
- 理由: union payload は全 case が記憶域を共有し（所有権検査の `PAYLOAD`）、case ごとの排他 loan の追跡が要る。必要性も示されていない。
- 状態: 既定案（実装者はこの案に従う）

### D12: Phase 2 の構文（参照先の region）

- 決定: 参照の後ろに参照先の region を書く `ref {r} T {s}`・`ref mut {r} T {s}` を、名前付き関数の引数・戻り値と record のフィールドに許可する。
  `s` は `r` と別名で、`T` は region が一つの型（排他借用を持たない）。`s == r`（`ref mut {r} Note {r}`）は従来どおり一つの region として扱う。
  region で量化した関数型の内側には書けない。参照先から読んだ借用は参照ではなく `s` の入力の loan を持つ。
- 理由: 新しい字句・構文・`Type` を足さずに、Rust の `&'r mut View<'s>` に当たる二つの寿命を A12 の slot（参照自身と参照先）で表せる。
- 状態: 実装時の決定（2026-10-06、Phase 2 の承認による）

### D13: 参照経由の借用集約型の置換

- 決定: `deref target = value` の `value` が借用を持つとき、`target` の根がローカルの所有者なら、所有者の loan に新しい loan を加える（weak update。region の注釈は要らない）。
  引数の参照先なら、`value` の loan がすべて参照先の region `s` を持つ入力に由来する場合だけ許可する。それ以外は `E1013`
  （region がない: `assigning borrowed values through references requires explicit lifetimes; name the target's region apart from the reference's, ...`、
  ローカル値の借用: `cannot store a borrow of a local value through a reference parameter; ...`、別の region: `the stored borrow does not have the region of the reference's target; ...`）。
- 理由: region 間の outlives 制約を足さずに、格納した借用の寿命を既存の loan と NLL で保てる。強い更新（古い loan の除去）は別名の追跡が要るので行わない。
- 状態: 実装時の決定（2026-10-06）

### D14: 格納効果を持つ関数の呼び出し

- 決定: 参照先へ入力を格納しうる関数（`s` を持つ入力が参照のほかにある関数。`RegionSlots.writes`）の直接の完全適用では、呼び出し側が格納されうる入力の loan を
  参照先の所有者へ加える。この関数は関数値・部分適用にできない（`E1013`、`'{name}' may store an input through an exclusive reference, so call it directly with all of its arguments`）。
- 理由: 関数値の型には region がなく（A12 D11）、格納効果を呼び出し側へ伝えられない。
- 状態: 実装時の決定（2026-10-06）

### D15: 外部 loan の可変性（D6 の変更）

- 決定: 引数の外部 loan は A12 の slot ごとに作り、排他 region の slot と `ref mut` の引数だけを mutable にする。書き込み・排他貸し直しの可否は、
  その place に重なる経路の loan だけで判定する（重なるものがなければ従来どおり全経路）。`let mut` の record の経路は、排他の経路があればそれに絞る。
- 理由: loan の可変性を slot が表す借用（排他・共有）に合わせ、Phase 2 の参照先の slot（共有借用だけを持つ）も同じ規則で扱う。経路の判定を重なる loan に限るのは、
  `ref mut (deref handle).value` のように共有借用フィールドも持つ record を排他参照経由で貸し直すとき、所有者が持つ共有 loan が経路に混ざって `E1014` の誤拒否になるため。
  既存テストの期待値と既存プログラムの IR は変わらない。
- 状態: 実装時の決定（2026-10-06）

### D16: M7 の撤回（D8 の変更）

- 決定: M7 を実装しない。ガード中のパターン束縛は `alias_source` による alias で、束縛元の loan を保ったまま追跡される。
  `ownership_control::view` は loan を持たない型だけに使われ、排他借用へ届く型には到達しない。
- 理由: ガード中に参照先を変更する T3 の 14 は、Rust の bind-by-move のガードと同じく健全で（実行して 111 を確認）、拒否する根拠がない。
- 状態: 実装時の決定（2026-10-06）

## 実装と検証（2026-10-06）

### 実装した範囲

- Phase 1（手順 1〜10）と Phase 2（D12〜D14）を実装した。M7（D8）は撤回した（D16）。対象外（D11）の union payload・コレクション要素・
  タプルのフィールド型の排他参照と、型変数の排他借用による具体化は実装していない。
- 宣言（`src/regions.rs`）: `exclusive_field_regions` が M1・M3、`exclusive_positions` が M2 を出す（region が一つの record では、名前のない借用も region 0 として数える）。
  `field_regions` は `RecordRegions { count, fields, targets }` を返し、参照先の region も集める。`target_form`・`target_labels` が D12 の形を検査する。
- 型の性質（`src/check.rs`、`src/recursive.rs`）: `TypeContext::reaches`、`Type::reaches_exclusive`・`Type::holds_exclusive_record`。
  `mark_exclusive_regions` の固定点が `CheckedRecord.exclusive_regions`・`field_targets`・`region_targets` を計算し、フィールドの検査が M4・M5、
  `Validation::check` の record 分岐（`instance_field_error`）が M6 と既存の `would store a mutable reference in field` を出す。
- 所有権（`src/ownership.rs`）: slot ごとの外部 loan（`external_loans`、`ExternalSlot`）、書き込み判定の精密化（D15。`unmarked_fields` で A12 の slot の印を除く）、
  M8（`shared_exclusive`）、`exclusive_places`、参照経由の置換（`store_through`、`target_loans`）、呼び出し側の格納効果（`input_slot_loans`・`store_arguments`）。
  `CheckedFunction.region_slots`（`RegionSlots`）を `src/regions.rs` の契約から設定し、`validate_contract_calls` が D14 の直接呼び出しを強制する。
- 文書: `docs/language.md`、`docs/architecture.md`、`README.md`、`_docs/language-reference/lifetimes.md`（実行する例 2 件）、`_docs/language-reference/ownership.md`、
  `_docs/feature-status.md`、`_docs/learn/why-tsuzuri.md`、`_features/README.md`、GUIDE D-38（D-28 の A09 の行も改めた）。

### 確認コマンドと結果

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`（作業ツリーの複製で実行）: 669 件成功、0 件失敗（変更前は 663 件。追加は `tests/borrowed_records.rs` の 6 件）。
- `cargo test --locked --test borrowed_records`: 21 件成功。追加は `exclusive_borrow_fields_accept_reborrows_moves_and_regions`（T1 の 16 本体）、
  `exclusive_borrow_field_declarations_are_validated`（T2）、`exclusive_borrow_fields_preserve_exclusivity`（T3）、
  `named_target_regions_store_borrowed_aggregates_through_references`、`named_target_regions_keep_stored_borrows_alive`、`named_target_regions_do_not_change_generated_ir`。
- §3.1 の stack-depth テスト 3 件と上限のテスト 4 件（既定の 2 MiB stack）: 成功。上限・stack の大きさは変えていない。
- `npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri borrowed_records`: 57 件（追加 36 件）が native／WASM × `-O0`／`-O3` で期待値どおり、`live == 0`。
  追加の export は `exclusive_loop`・`exclusive_vec`・`exclusive_1`〜`exclusive_16`・`target_local`・`target_swap`・`target_editor`・`target_kept`・`target_loop`・`target_owned`。期待値は手計算。
- IR: 変更前のコンパイラ（`0116bf2`）と比べ、fixture・例の 165 個の IR のうち差は fixture を足した `borrowed_records` だけ。変更前の `borrowed_records` の fixture は
  新しいコンパイラでも `-O0`／`-O3` で byte 単位で一致した。新しい fixture の IR は 2 回の出力で一致した（native・wasm32）。
- E2E（`cargo build --release --locked` の後）: `scripts/check-runtime-includes.sh`、`tests/` の e2e・primitives・strings・tasks・computations・control・io・numeric_casts・os・
  host_imports・ffi_extensions・trap_return・stack_overflow・user_drop・features（5365 件）・examples・wasm_memory・wasm64・cache・simd・docgen・lsp_sessions と
  `scripts/toolchain/smoke.mjs` が成功（primitives・features・wasm64 は Node 24）。debug_info は `llvm-dwarfdump --verify`（LLVM 21）が `-O3` の object で
  1 件の誤りを報告して失敗したが、変更前のコンパイラでも同じく失敗する（既存の問題で、本チケットとは無関係）。
- `node scripts/check-docs.mjs`: 変えたページを含む 99 ページ（914 リンク、219 例、native の `-O0`／`-O3` 実行 370 回、test 8 件）が成功し、
  `_docs/feature-status.md` の ID の順序も `_features/README.md` と一致した。全ページの実行は、`_docs/examples/README.md` の `../../README.md#コンソール` と
  `_docs/get-started.md` の `../README.md#ビルド` が README にない見出しを指すため失敗する。ベースの `0116bf2` からある既存の問題で、本チケットでは変えていない。

### 判断と残作業

- 決定事項に D12〜D16 を追記した（D6・D8 の変更を含む）。新しい診断コード・予約語・ランタイム・WASM import はない。性能は主張しない（検査だけの変更で、生成コードは変わらない）。
- 既知の制限: 複数 region の record を `let mut` で束縛すると全 region をまとめて扱うので、共有フィールドの読み出しと排他フィールドの貸し直しを同じ式で行うと
  `E1014` になりうる（A12 D6。先に `let` で読み出す）。region 間の outlives 制約は表さない（D13）。
- PR01 の不変条件 I1: 排他借用フィールドを持つ record の引数と、同じ参照先を指す別の引数は、呼び出し時の所有権検査が `E1014` で拒否するので保たれる。
- 残作業: union payload・コレクション要素の排他参照（D11）、`let mut` とループで合流する値の region ごとの追跡。

### 変更したファイル

- `src/check.rs`、`src/regions.rs`、`src/ownership.rs`、`src/recursive.rs`、`src/closures.rs`・`src/polymorph.rs`（`region_slots` の初期化だけ）。
- `tests/borrowed_records.rs`、`tests/fixtures/borrowed_records/Main.tz`、`tests/features.mjs`。
- 文書は「実装した範囲」のとおり。「主な影響ファイル」の予定と異なり、`src/ownership_control.rs` は変えていない（D16）。

### レビュー対応（PR #13）

- Copilot のレビューは指摘なしで、借用検査の健全性と関数をまたぐ loan の伝播は人による確認を勧めた。反例を 25 件試し（二つの参照の交換、
  転送する関数、record の引数、ループの局所値、部分適用、関数値、共有参照経由の貸し直し、Task の捕捉など）、すべて期待どおりに受理・拒否された。
- 回帰テストになかった拒否 5 件（別の参照の参照先の格納、交換、転送する関数、`retarget`、ループの局所値）と、交換の受理を `tests/borrowed_records.rs` に足した。
