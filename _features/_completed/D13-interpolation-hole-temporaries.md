# D13: 補間の穴の一時値（`{s.length}` の `E1013` の解消）

| 項目 | 内容 |
| --- | --- |
| ID | D13 |
| 優先度 | P1 |
| 規模 | S（Phase 1 だけ） |
| 依存 | なし |
| 後続 | なし。[F14](../F14-structural-performance-assertions.md) の落とし穴の行（「`{s.length}` の補間は `E1013`（D13 が直す）」）は、D13 の出荷後に F14 の実装者が消す |
| 状態 | done（2026-10-23。Phase 1 と全手順を実装・検証済み） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーの `9dd0fbbc` との差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D8 はすべて承認済み |
| 改善する劣位 | 他の言語（Rust の `format!("{}", s.len())`、Swift・Kotlin・C# の補間）は穴に任意の式を書ける。Tsuzuri は `$"len = {s.length}"` が、利用者が書いていない一時値を指す `E1013`（`bind the temporary with 'let' first`）になり、AI も人間も最初の 1 回で書けない。言語リファレンスの説明も「ことがあります」で、どの式がなるのか決まっていない |
| 手本にする既存実装 | 穴の型付け: `src/check.rs` の `Checker::interpolation_hole`・`hole_operand`（D07。追記 5 で `Name` の穴だけを同じ原因から直した）。呼び出しの引数の一時値: `src/constants.rs` の `temporary_borrows`・`plain_constant`。`BorrowOperand` の下げ方: `src/llvm.rs` の `operand_borrow`・`release_operands`、`src/llvm_display.rs` の `hole_piece`。`E1013` の発行箇所: `src/ownership.rs` の `Ownership::place`。テスト: `tests/string_interpolation.rs`（`accepts`・`rejects`）、`tests/constants.rs` の型付き木を歩く補助 `integers_of`、`tests/features.mjs` の suite `string_interpolation` |
| 主な影響ファイル | `src/check.rs`（`hole_operand` と新しい 2 関数）、`tests/string_interpolation.rs`、`tests/features.mjs`、`tests/fixtures/interpolation_temporaries/Main.tz`（新規）、文書 4 ファイル（「ドキュメント」） |

## 実装者向け早見

- **読む順**: 「現状」の原因 → 「仕様」の決定表 → 「設計」のコード → 「実装手順」。コードは `src/check.rs` の `hole_operand`、`src/ownership.rs` の `Ownership::place`、`src/constants.rs` の `temporary_borrows`。
- **直す場所は 1 つ**: `src/check.rs` の `hole_operand`。穴の式が場所かどうかを、書き方（`ExprKind`）ではなく、型付けした式で決める（`TypedExpr::is_place` と、平たい定数の一部）。`src/ownership.rs`・`E1013` の文言・`llvm*.rs` は変えない（D1）。
- **最初に書くテスト**: `tests/string_interpolation.rs::holes_that_are_not_places_are_temporaries`（HEAD では、場所の行が通り、一時値の行が `E1013` で落ちる）。
- **変えない**: 今受理している穴の型付き木と生成 IR（場所の穴は `Borrow` のまま）、場所の穴の借用が補間の全体のあいだ続くこと（`E1014`）、`E1013` の発行条件と文言、書式指定の規則。
- **作る新しい名前**: `Checker::hole_place`・`Checker::constant_part`（どちらも非公開）、`tests/fixtures/interpolation_temporaries/`、suite `interpolation_temporaries`。診断コード・構文・予約語・CLI・edition は増えない。
- **完了の見分け方**: 付録 C の fixture が `check` を通り、suite が native/wasm32 × `-O0`/`-O3` で通る（`live == 0`、import なし）。手順 1 のコーパス 102 件の IR が 1 byte も変わらない。`tsuzuri check` の `$"len = {s.length}"` が通る。
- **迷ったら**: 決定表のとおりに書く。表にない形は一時値にする。ただし、今受理している形（コーパスの IR が変わる形）は場所のまま。

## 目的

`$"len = {s.length}"` は HEAD で `E1013` になる（`s: string`。`[i64]`・`ref [i64]`・スライスの借用でも同じ。実出力は「現状」）。呼び出しの引数なら同じ形が通る（`get s.length`）ので、**穴だけ**が書けない。

コードで確かめた原因は、所有権の検査ではなく、型検査の穴の分類である。`Checker::hole_operand` は、穴が場所かどうかを書き方で決める（`Field`・`Index`・`Dereference` と書いてあれば場所）。`.length` は書き方が `Field` でも、型付けすると `Length`・`StringLength`（値）になる。値を場所として借りるので、`Ownership::place` が `E1013` を返す。`(f x).name`・`(f x)[0]`・`s[0]`（文字列の添字）・定数の一部も同じ。

本チケットは次の 2 つだけを行う。

1. **穴の分類を型付けした式で決める**（`hole_operand` の約 25 行）。場所でない値は、他の一時値（`{n + 1}`・`{id n}`）と同じ経路（`BorrowOperand`）で、隠れた領域に置いて借りる。受理を増やす変更で、今受理している穴の型付き木と IR は変えない。
2. **穴に書ける式の決定表を仕様にし、文書の「ことがあります」を表に置き換える**。

D-50 は D13 を「性能の構造的な保証」の群に数える。本チケットが性能に関わるのは、今受理している穴の IR が変わらないことと、一時値の穴が確保を増やさないこと（関数の stack の slot）だけで、速度の主張はしない。範囲外は「対象外」。

## 着手条件と停止条件

### 着手条件

- 承認は済んでいる（D-50）。GUIDE §2.3 の基準（`cargo build --release --locked && cargo test --locked`）を取り、件数を控える。全体の `cargo test` は `RUST_MIN_STACK=4194304` を付けて流す（GUIDE の D-45。HEAD でも既定の 2 MiB では 1 件が溢れる）。
- 読む: `docs/stability.md` の冒頭（バグの修正と受理を増やす拡張は edition なしで入れてよい）、[D07](D07-string-interpolation.md) の「アルゴリズム」と「決定事項への追記」の 5、`tests/string_interpolation.rs`、GUIDE の §3.1（テスト選択表）と §11.1（再帰する経路の stack）。
- 手順 1 のとおり、変更前のコンパイラとコーパスの IR を残す。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

1. 手順 2 で、HEAD でも通るはずの行（決定表の場所・参照）が通らない。補助 `hole_kinds` の前提が違う。選択肢: A 補助か期待値を直す（コンパイラは触らない）、B 報告する。
2. 手順 4 でコーパスの IR が 1 つでも変わった。選択肢: A `constant_part` の条件を HEAD の受理に合わせて直す、B 報告する。**IR を変えて期待値を直さない**。
3. 修正後も、決定表で「OK」の行が `E1013` になる。選択肢: A その行の型付き木を調べ（`value.kind`）、`hole_place` の条件を直す、B 報告する。
4. `src/ownership.rs` の `E1013` を緩めたくなった。止めて報告する（D1。`let r = ref (f x)` の `E1013` が消える）。
5. 手順 5 の suite で `live != 0`（漏れ）か二重の drop が出た。報告する（backend の `operand_borrow` の cleanup は本チケットの範囲外）。
6. 2 MiB のテスト stack が溢れた（GUIDE §3.1 の 4 つの回帰テスト）。新しい関数に `#[inline(never)]` を付ける。それでも溢れたら報告する。
7. 新しい診断コード・予約語・crate・既定の WASM の import が必要に見えた。
8. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

実測は `target/release/tsuzuri`（HEAD の release ビルド）。`check` は、型の誤りが 1 つでもあると所有権の検査（`E1012`・`E1013`・`E1014`）まで進まないので、型を通してから見る。1 つのファイルに関数を 1 行ずつ並べると、`E1013` は関数ごとに 1 件ずつ出るので、全部を 1 回で見られる（付録 A）。

### 原因

1. 構文解析は `s.length` を `ExprKind::Field`（式 `s` と名前 `length`）にする。
2. `Checker::field_access` は、記録（record）なら `TypedExprKind::Field` にするが、配列・リスト・`Vec`・固定長配列の `.length` は `TypedExprKind::Length`、文字列・`utf8string` の `.length` は `TypedExprKind::StringLength` にする（型は `i64`）。どちらも場所ではない。
3. `Checker::hole_operand` は、穴が場所かどうかを書き方で決める。

```rust
let place = match &expression.kind {
    ExprKind::Name(_) => !matches!(
        value.kind,
        TypedExprKind::Construct { .. } | TypedExprKind::Call(..)
    ),
    ExprKind::Field(..) | ExprKind::Index(..) | ExprKind::Dereference(..) => true,
    _ => false,
};
```

   場所なら `coerce_argument(value, &ref _)` が `TypedExprKind::Borrow(value)` を作る。場所でなければ、参照はそのまま、排他の参照は共有の再借用、それ以外は `BorrowOperand(value)`（隠れた領域）にする。
4. 所有権の検査は `E::Borrow(value)` で `Ownership::place(value)` を呼ぶ。`place` が受けるのは、ローカル・静的データ・場所のフィールド（`UnionPayload`・`ListTail` を含む）・配列／リスト／`Vec`／固定長配列の場所の添字・参照外しだけで、ほかは最後の腕が `E1013`（`borrow requires a local place; bind the temporary with 'let' first`）を返す。**この判定は正しい**（`let r = ref (f x)` は `E1013` でなければならない）。誤っているのは、場所でない値に `Borrow` を作る側である。
5. 呼び出しの引数が通るのは、`argument()` が同じ書き方（`Field`・`Index`）を `place_argument` で `Borrow` にしたあと、`constants::temporary_borrows` が「場所でない共有の借用」を `BorrowOperand` に書き換えるから（単一段の呼び出しで、結果が借用を持たないとき）。穴は補間の結果が `string`（借用を持たない）なので常に条件を満たすが、この書き換えを通らない。
6. D07 の設計は、穴の分類を「`ownership.rs` の place 判定と同じ規則」（型付けした式）としていた。実装は書き方で判定し、D07 の追記 5 が `Name` の穴（union の case・引数のない関数）だけを直した。本チケットは `Field`・`Index` を同じ原則で直す。

### 実測の要約

穴の式の全形を HEAD で `check` した（付録 A のプログラム。約 100 通り）。整理した表は「仕様」の決定表である。要点:

- `E1013` になる形: `.length`（文字列・配列・リスト・固定長配列・`utf8string`・参照・スライスの借用・クロージャの捕捉・配列リテラル・呼び出し結果・定数）、文字列・`utf8string` の添字 `s[0]`、呼び出し結果のフィールドと添字 `(f x).name`・`(f x)[0]`、非平たい定数の一部 `Origin.x`・`Words[1]`、書式指定つきのそれら（`{s.length:>5}`）、明示の `ref (f x)`・`ref s.length`。
- 通る形（変わらない）: ローカル・場所のフィールドと添字（`p.name`・`l.a.x`・`xs[i]`・`ws[1]`・`rx[0]`・`rp.x`）・`deref r`・平たい定数の一部（`Nums[i]`・`Corner.a`）・参照・`ref n`・`ref mut m`・呼び出し・算術（`{s.length + 1}` も通る）・リテラル・`if`・`match`・ラムダの呼び出し・入れ子の補間・配列リテラル。
- 書式指定の誤りは今も先に出る（`$"{s.length:.2}"` は `error[E1003]: format spec '.2' does not apply to i64; precision needs a floating-point or decimal number`）。
- 場所の穴は補間の全体のあいだ借りたまま（`$"{a[0]}{poke (&mut a)}"` は `E1014`）。一時値の穴は、評価した時点の値だけを持つ（`$"{m + 0}{bump (&mut m)}"` は今も通る）。
- 既存のテスト・文書の例・std・fixture に、穴の中の `.length` などを書いたものはない（`tests/*.rs` の `E1013` の期待はどれも穴ではない）。直接書けないので、文書は「ことがあります」と書き、F14 は `String.length (ref s)` へ回している。
- 文書で `E1013` を穴と結ぶ記述は 2 か所: `literals-and-strings/interpolated-strings.md` の「穴は 1 行に収めます」の段落と、`literals-and-strings/strings.md` の添字の段落。

headline の実出力:

```text
Main.tz:2:10: error[E1013]: borrow requires a local place; bind the temporary with 'let' first
  2 | $"len = {s.length}"
    |          ^
```

## 変えてはいけない意味

- 今受理している穴の型付き木と生成 IR（場所の穴は `Borrow` のまま。平たい定数の一部は静的データを読む場所のまま）。手順 4 のコーパスの IR で示す。
- 場所の穴の借用が補間の全体のあいだ続くこと（`$"{x}{bump (&mut x)}` は `E1014`）。
- 穴を左から右へ 1 回ずつ評価すること、値を消費しないこと（非 Copy のローカルは補間のあとで使える）、表示は `Display.display`（`Format`）で行うこと、結果を 1 回だけ確保すること。
- `Ownership::place` と `E1013`（`borrow requires a local place; bind the temporary with 'let' first`）の発行条件と文言。`let r = ref (f x)`・`Tensor.rank (ref (Tensor.permute …))` は今も `E1013`。
- 書式指定の規則（`E1003`）と、`Display` がない型の `E1005`。
- 既存の診断コード・メッセージ。新しい診断は増えない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `hole_operand` の分類を型付けした式で行う。Rust のテストと E2E の suite、文書 | S | なし | 受理を増やすだけ（`docs/stability.md`: バグの修正と受理を増やす拡張）。今受理している穴の型付き木と IR は変わらない |

**Phase は 1 つだけ**（修正は 1 関数で、テストと文書は同じ変更に含める）。

## 仕様

### 分類の規則

穴 `{e}` の式 `e` を型付けしたあと、`Checker::hole_operand` が次の 3 つのどれかにする。**書き方は決め手にならない**（`s.length` は `Field` と書くが値。括弧で包んでも同じ）。

| 分類 | 型付けした式 | 穴の扱い（`TypedHole.operand`） |
| --- | --- | --- |
| 場所 | 次の「場所」 | `Borrow(e)`。その場で共有借用し、借用は補間の全体のあいだ続く（`e` が参照型のローカルなら、参照先の共有借用） |
| 参照 | 場所でなく、型が `ref T` の値（`{ref n}`・参照を返す呼び出し） | `ref T` はそのまま、`ref mut T` は共有の再借用（`reborrow_operand`） |
| 一時値 | それ以外 | `BorrowOperand(e)`。評価した値を隠れた領域（関数の stack の slot）に置き、表示し、結合のあとで drop する |

**場所**（`TypedExpr::is_place` に、平たい定数の一部を足したもの。`Ownership::place` が受ける形と一致する）:

1. `Local`。
2. `Field`・`UnionPayload`・`ListTail` で、中の式が場所。
3. `Index(base, i)` で、`base` の型が配列・リスト・`Vec`・固定長配列で、`base` が場所。**文字列・`utf8string` の添字は場所ではない**。
4. `Dereference`（中の式は何でもよい）。
5. 平たい定数（`constants::statics::eligible` が真の型の `const`）の一部: `Field` の鎖、または配列・固定長配列の `Index` の鎖で、根が `Call(constant)`。`constants::fold` が `C` を静的データ `Static(C)` に変え、`Dereference(Borrow(Static))` を根にした場所として読むので、型検査の時点では場所に見えなくても、今も受理される（`{Nums[1]}`・`{Corner.a}`）。ここを一時値にすると、静的データの借用がコピーの退避に変わり IR が変わる。

`ExprKind::Name` の穴は従来どおり（引数のない関数の呼び出しと union の case は一時値、それ以外は場所）。`Field`・`Index`・`Dereference` と書いた穴だけが、上の 5 つで場所かどうかを決める。それ以外の書き方（`Call`・`Binary`・`If`・リテラル・`Borrow`）は従来どおり一時値か参照である。

### 一時値の意味

- **評価**: 補間の中の位置で、左から右へ 1 回だけ評価する。式が副作用を持つなら、その順に起きる。
- **値の保持**: 評価した値を stack の slot に置く。`Copy` の値（`i64` の `.length`）は複製で、元の場所はそのあと借りたままにならない。非 Copy の値は slot が所有する（`(mk n).name` のフィールドは一時の記録から slot へ移る）。
- **解放**: 全穴の表示文字列を結果へ書き込んだあとに、穴の順に drop する（`release_operands`）。穴の表示文字列の解放はその前。
- **借用との関係**: 一時値の穴は元の場所を借りたままにしない。`$"{a.length}{poke (&mut a)}{a[0]}"` は通り、左の穴は評価した時点の長さを表示する。場所の穴は借りたままなので、`$"{a[0]}{poke (&mut a)}"` は `E1014`。
- **確保**: 一時値そのものは確保しない（slot は関数の frame の `alloca`）。補間が確保するのは、結果の文字列（1 回）と、`Display` が返す穴ごとの文字列だけで、場所の穴と同じ。
- **書式指定**: 一時値の型（`.length` は `i64`、`s[0]` は `i16u`）に、場所の穴と同じ規則で適用する。

### 決定表

各行を HEAD で `check` した（「HEAD」の列。付録 A）。「修正後」の列は、設計のコードと付録 C〜E を作業用の複製に当てて 1 回ビルドしたコンパイラの実測（付録 B）。実装者は手順 3〜5 で同じ結果になることを確かめる。

束縛の型: `s: string`、`u: utf8string`、`xs: [i64]`、`ws: [string]`、`ls: [| i64 |]`、`fx: [i64; 3]`、`n: i64`、`b: bool`、`m: i64`（`let mut`）、`p: Pt`、`ln: Line`、`r: ref i64`、`rs: ref string`、`rx: ref [i64]`、`rp: ref Pt`、`part: ref [i64]`（`ref xs[1..3]`）。`record Pt { x: i64, name: string }`、`record Line { a: Pt, b: Pt }`、`record Res { id: i64, name: string }`（`Drop` あり）。`const Nums: [i64]`・`const Corner: Pair { a: i64, b: i64 }`（平たい）、`const Origin: Pt`・`const Words: [string]`・`const Title: string`（平たくない）。`id n: i64`、`mk n: Pt`、`mkres n: Res`、`make3 n: [i64]`、`names (): [string]`、`mkref r: ref i64`。

| # | 穴 | 型付けした式 | 分類 | HEAD | 修正後 |
| --- | --- | --- | --- | --- | --- |
| P01 | `{s}`・`{n}` | `Local` | 場所 | OK | OK |
| P02 | `{p.name}`・`{ln.a.x}` | `Field` の鎖が場所 | 場所 | OK | OK |
| P03 | `{xs[0]}`・`{xs[i]}`・`{ws[1]}` | `Index(場所)`（配列・リスト・`Vec`・固定長配列） | 場所 | OK | OK |
| P04 | `{rx[0]}` | `Index(Dereference(rx))` | 場所 | OK | OK |
| P05 | `{deref r}`・`{*r}`・`{rp.x}`・`{(deref rp).x}`・`{deref (mkref r)}` | `Dereference`・`Field(Dereference)`・`Dereference(Call)`（参照を返す呼び出しの参照外しも `Dereference` なので場所） | 場所 | OK | OK |
| P06 | `{Nums[1]}`・`{Nums[i]}`・`{Corner.a}` | 平たい定数の一部（根が `Call(constant)`） | 場所（静的データを読む） | OK | OK（IR は同一） |
| P07 | `{xs[xs.length - 1]}` | `Index(場所, 式)`。添字の式の `.length` は添字として普通に評価する | 場所 | OK | OK |
| R01 | `{r}`・`{rs}` | 参照型の `Local` | 場所（参照先を共有借用する） | OK | OK |
| R02 | `{ref n}`・`{&n}`・`{ref xs[1..3]}` | `Borrow(場所)`・スライスの借用 | 参照（そのまま） | OK | OK |
| R03 | `{ref mut m}` | 排他の参照 | 参照（共有の再借用） | OK | OK |
| T01 | `{id n}`・`{Array.length xs}`・`{String.length s}`・`{(\(x: string) -> x.length) s}` | `Call` | 一時値 | OK | OK |
| T02 | `{n + 1}`・`{-n}`・`{n == 1}`・`{!b}`・`{n as i32}`・`{s.length + 1}`・`{(id n) + 1}` | `Binary`・`Unary`・`Cast` | 一時値 | OK | OK |
| T03 | `{1}`・`{"abc"}`・`{'c'}`・`{true}`・`{1.5}`・`{()}`・`{[1, 2, 3]}`・`{make3 n}` | リテラル・配列・`Call` | 一時値 | OK | OK |
| T04 | `{if b then 1 else 2}`・`{match n with \| 1 -> "one" \| _ -> "other"}`・`{$"{n}"}` | `If`・`Match`・`Interpolated` | 一時値（選んだ非 Copy のローカルは移る。`{if b then s else t}` のあと `s` を使うと `E1012`。既存） | OK | OK |
| T05 | `{Title}`・`{Words}`・union の case・引数のない関数の呼び出し | `Call`・`Construct` | 一時値（D07 の追記 5） | OK | OK |
| N01 | `{s.length}`・`{u.length}` | `StringLength(Local)` | 一時値 | E1013 | OK |
| N02 | `{xs.length}`・`{ls.length}`・`{fx.length}` | `Length(Local)` | 一時値 | E1013 | OK |
| N03 | `{rs.length}`・`{rx.length}`・`{part.length}` | `StringLength`・`Length(Dereference)` | 一時値 | E1013 | OK |
| N04 | `{p.name.length}`・`{ln.a.name.length}`・`{ws[1].length}` | `StringLength(場所の鎖)` | 一時値 | E1013 | OK |
| N05 | `{s[0]}`・`{rs[1]}`・`{u[0]}` | `Index(string／utf8string)` | 一時値（`i16u`・`i8u` の値） | E1013 | OK |
| N06 | `{(mk n).x}` | `Field(Call)`（Copy のフィールド） | 一時値 | E1013 | OK |
| N07 | `{(mk n).name}`・`{(mk n).name.length}` | `Field(Call)`（非 Copy。`Drop` なし） | 一時値（フィールドが slot へ移る） | E1013 | OK |
| N08 | `{(make3 n)[1]}` | `Index(Call)`（Copy の要素） | 一時値 | E1013 | OK |
| N09 | `{(make3 n).length}`・`{(names ()).length}`・`{[1, 2, 3].length}` | `Length(Call／Array)` | 一時値 | E1013 | OK |
| N10 | `{(mkres n).id}`（`Res` は `Drop`） | `Field(Call)`（Copy の部分） | 一時値 | E1013 | OK |
| N11 | `{Nums.length}`・`{Words.length}`・`{Title.length}` | `Length`・`StringLength(Call(constant))` | 一時値 | E1013 | OK |
| N12 | `{Origin.x}`・`{Origin.name}` | `Field(Call(constant))`（平たくない定数） | 一時値（定数の複製から取る） | E1013 | OK |
| N13 | `{s.length:>5}`・`{xs.length:x}`・`{s.length:+}`・`{xs.length:*^7}` | N01・N02 と同じ。書式は `i64` に適用 | 一時値 | E1013 | OK |
| N14 | クロージャの中の `{xs.length}`（`xs` を捕捉） | `Length(Local)` | 一時値 | E1013 | OK |
| E01 | `{(names ())[0]}` | `Index(Call)`（非 Copy の要素） | 一時値から非 Copy の要素は取り出せない | E1013 | E1012 `cannot move a non-Copy array or list element; borrow it instead` |
| E02 | `{Words[1]}` | `Index(Call(constant))`（平たくない定数の非 Copy の要素） | 同上 | E1013 | E1012（同上） |
| E03 | `{(mkres n).name}`（`Res` は `Drop`） | `Field(Call)`（非 Copy。`Drop` あり） | 一時値から非 Copy のフィールドは取り出せない | E1013 | E1012 `cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead` |
| X01 | `{ref (id n)}`・`{ref s.length}` | `Borrow(場所でない値)` | 明示の借用 | E1013 | E1013（変わらない。`ref` を外す） |
| K01 | `$"{a[0]}{poke (&mut a)}"`・`$"{x}{bump (&mut x)}"` | 場所の穴は借りたまま | — | E1014 | E1014（変わらない） |
| K02 | `$"{a.length}{poke (&mut a)}{a[0]}"`・`$"{m + 0}{bump (&mut m)}{m}"` | 一時値の穴は評価した時点の値だけを持つ | — | 前者 E1013、後者 OK | OK（`379`。`n` `n+1` `n+1`） |

`E01`〜`E03` は、診断のコードが `E1013` から `E1012` へ変わるだけで、どちらも拒否である（受理するプログラムは減らない）。直し方は「`let` で束縛してから、場所として書く」。`X01` と `K01` は変えない。

### 診断

新しい診断コード・メッセージは増えない。変わるのは、穴の中で出るコードだけである。

| 場合 | 出るコード | メッセージ | 直し方 |
| --- | --- | --- | --- |
| `ref`・`&` を付けた場所でない値（`X01`） | `E1013` | `borrow requires a local place; bind the temporary with 'let' first` | `ref` を外す。穴は一時値を借りる |
| 一時値の非 Copy の要素（`E01`・`E02`） | `E1012` | `cannot move a non-Copy array or list element; borrow it instead` | `let` で束縛してから `{xs[0]}` と書く |
| `Drop` を持つ値の非 Copy のフィールド（`E03`） | `E1012` | `cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead` | `let` で束縛してから `{r.name}` と書く |
| 場所の穴の借用が続くあいだの排他借用（`K01`） | `E1014` | `access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively` | 穴の前に値を `let` へ取り出す（`let first = a[0]`） |

位置は、どれも穴の式の先頭（`Main.tz:2:10` のように、`s.length` の `s`）。

## 設計

### 変更するもの

| ファイル | 変更 |
| --- | --- |
| `src/check.rs` | `hole_operand` の `Field`・`Index`・`Dereference` の腕を `self.hole_place(&value)` にする。新しい非公開の 2 関数 `hole_place`・`constant_part` |
| ほか | なし（`src/ownership.rs`・`src/constants.rs`・`src/llvm*.rs`・`src/derive.rs` は変えない） |

### コード

`hole_operand` の `place` の計算だけを変える。そのあとの 3 つの分岐（場所は `coerce_argument`、参照はそのまま／再借用、それ以外は `BorrowOperand`）は変えない。

```rust
        // A bare name is a place unless it builds a value: a union case such as
        // `Light`, or a call of a function without parameters. A field, an index, or
        // a dereference is a place only if its typed value is one: `s.length`,
        // `s[0]`, and `(f x).name` are written like places but are values (D13).
        let place = match &expression.kind {
            ExprKind::Name(_) => !matches!(
                value.kind,
                TypedExprKind::Construct { .. } | TypedExprKind::Call(..)
            ),
            ExprKind::Field(..) | ExprKind::Index(..) | ExprKind::Dereference(..) => {
                self.hole_place(&value)
            }
            _ => false,
        };
```

次の 2 つの関数を、同じ `impl Checker` の `hole_operand` の直後（`check_format_specs` の前）に足す。

```rust
    /// Whether `value` names storage that a shared borrow takes where it is: a place of the
    /// function, or a part of a constant of plain data, which `constants::fold` turns into a
    /// place in static data. The second kind stays a borrow, so the code of a program that was
    /// accepted before does not change.
    #[inline(never)]
    fn hole_place(&self, value: &TypedExpr) -> bool {
        value.is_place() || self.constant_part(value)
    }

    /// A `Field` or `Index` chain whose root is a constant of plain data.
    #[inline(never)]
    fn constant_part(&self, value: &TypedExpr) -> bool {
        match &value.kind {
            TypedExprKind::Field(base, _) => self.plain_constant(base) || self.constant_part(base),
            TypedExprKind::Index(base, _) => {
                matches!(
                    base.ty,
                    Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                ) && (self.plain_constant(base) || self.constant_part(base))
            }
            _ => false,
        }
    }
```

- `TypedExpr::is_place` は `src/check.rs` の `impl TypedExpr`（`pub(crate)`）。`Checker::plain_constant` は `src/constants.rs` の `impl Checker`（`pub(super)`）で、`temporary_borrows` が同じ形で使う。どちらも新設しない。
- `constant_part` の再帰は式の入れ子（`MAX_NESTING` = 128）で抑えられ、`is_place` と同じ深さ。
- `hole_operand` の doc コメントの 1 文目（`a place is borrowed ...`）はそのまま残す。

### 型付き木の変化

| 穴 | HEAD | 修正後 |
| --- | --- | --- |
| `{s.length}` | `Borrow(StringLength(Local s))` → 所有権の検査で `E1013` | `BorrowOperand(StringLength(Local s))`（型 `ref i64`） |
| `{(mk n).name}` | `Borrow(Field(Call(mk, [n]), name))` → `E1013` | `BorrowOperand(Field(Call(mk, [n]), name))` |
| `{p.name}` | `Borrow(Field(Local p, name))` | 同じ（変わらない） |
| `{Nums[1]}` | `Borrow(Index(Call(Nums), 1))` → `fold` が `Borrow(Index(Dereference(Borrow(Static)), 1))` | 同じ（`constant_part` が真） |

`BorrowOperand` の下流はすでに一般の式を受ける: 所有権の検査は `E::BorrowOperand(value) => self.eval(value, Use::Read, ..)`、`llvm_display.rs::hole_piece` は `operand_borrow` で、場所なら借用し、場所でなければ `frame_value` で評価して slot へ退避し、`release_operands` が結合のあとで drop する。`last_use.rs`・`ranges.rs`・`closures_rec.rs`・`const_*.rs` も `BorrowOperand` を扱う。

### 他の経路との関係

- 呼び出しの引数: `argument()` が `Field`・`Index` を `place_argument`（`Borrow`）にし、`constants::temporary_borrows` が場所でない共有の借用を `BorrowOperand` にする。本チケットは穴で同じ結果を、`Borrow` を作ってから壊さずに、分類の時点で得る。
- `ExprKind::Borrow`（明示の `ref`・`&`）の穴は `hole_operand` の `_` の腕（場所ではない）で、型が参照なので値がそのまま使われ、場所でない値の借用は `E1013`（`X01`）。変えない。
- `derive.rs` が合成する `Display`・`Format` の穴は、引数の `Local` からのフィールド（場所）で、変わらない。

### 却下した案

| 案 | 却下の理由 |
| --- | --- |
| `Ownership::place` が一時値の借用を受ける | `let r = ref (f x)` の `E1013` が消える。借りた一時値が文の終わりで消えるので、`E1013` が正しい |
| `coerce_argument` のあとで `temporary_borrows` と同じ書き換えをする | 同じ結果になるが、`Borrow` を作ってから壊す。平たい定数の例外も同じだけ要る。分類のほうが読みやすい |
| パーサで `s.length` を `String.length (ref s)` の呼び出しへ脱糖する | `s[0]`・`(f x).name`・`(f x)[0]` が直らない。D07 の「脱糖は一時値で `E1003` になる」と同じ理由で、型付けのあとで分類する |
| 一時値の基底を slot に置き、要素・フィールドを場所として借りる（`{(names ())[0]}` を通す） | 新しい型付き木の形（一時の基底と場所の射影）が要る。Phase 1 の範囲外。今は `E1012` で拒否し、`let` で束縛すれば通る |
| `.length` だけを特例にする | `s[0]`・`(f x).name`・定数の一部が残る。原因は書き方で分類していること |

### 性能と IR

- 今受理している穴の型付き木と IR は変わらない（コーパスの IR の比較。手順 4）。
- 一時値の穴は、評価した値を関数の frame の `alloca` に置く。確保（`@tz.alloc`）は増えない。付録 B で、穴を 6 つ（うち 5 つが一時値）持つ関数の `-O0` の IR を数えた: `@tz.string.allocate` は結果の 1 回、`Display.display` の呼び出しは穴ごとに 1 回（場所の `{n}` の穴と同じ）で、一時値のための確保はない。`--emit llvm -O3` の IR も同じ（コンパイラの `--emit llvm` は clang の最適化の前の IR）。
- 速度の主張はしない。

## 前提とする他チケットのインターフェース

なし。関係するチケット:

| チケット | 関係 |
| --- | --- |
| D07（完了） | 穴の型付けの元の設計。本チケットは D07 の意図（型付けした式で分類する）に合わせる |
| [F14](../F14-structural-performance-assertions.md) | 落とし穴の行が D13 を待つ。D13 の出荷後に F14 の実装者が消す。本チケットは F14 を編集しない |
| [A22](../A22-edition-2027-canonical-forms.md) | `$"{&x}"` の穴の `&` を検出する（A22 のテスト T25）。本チケットは `&x`・`ref x` の穴の扱いを変えない |

## 実装手順

各手順は「変更 → 内容 → 確認」。各手順の最後で `cargo build --release --locked` と `cargo test --locked`（全体は `RUST_MIN_STACK=4194304` を付ける）が通る（`running N tests` の N を控える。GUIDE §3.1）。`/tmp/tz-D13/` は作業用の一時ディレクトリでリポジトリには置かない。

**手順 1: ベースライン。**
- 変更: なし。
- 内容: `cargo build --release --locked && RUST_MIN_STACK=4194304 cargo test --locked` の件数を控え、GUIDE §3.1 の 4 つの回帰テストを流す。変更前のコンパイラとコーパスの IR を残す（以後「手順 1 の IR」）:

```sh
mkdir -p /tmp/tz-D13/bin /tmp/tz-D13/before /tmp/tz-D13/x
cp target/release/tsuzuri /tmp/tz-D13/bin/tsuzuri-before
for d in tests/fixtures/*/ examples/*/; do
  [ -f "$d/Main.tz" ] || continue
  n="$(basename "$(dirname "$d")")_$(basename "$d")"
  /tmp/tz-D13/bin/tsuzuri-before build "$d" --emit llvm -O0 -o "/tmp/tz-D13/before/$n.ll" >/dev/null 2>&1 || echo "FAILED $d"
done
ls /tmp/tz-D13/before/*.ll | wc -l
```

  `Main.tz` のないライブラリの fixture 7 件とトップレベルの `tests/fixtures/Semantics.tz` は、変更前の `check` の標準出力と標準エラーを `/tmp/tz-D13/before/` に残す。付録 C の fixture を `/tmp/tz-D13/x/Main.tz` に写し、`/tmp/tz-D13/bin/tsuzuri-before check /tmp/tz-D13/x` を流す。
- 確認: IR は **102 件**（fixtures 93 + examples 9）で `FAILED` なし。`check` の出力は `E1013` の 15 件（`export def` の 15 個の関数にちょうど 1 件ずつ）。

**手順 2: テスト先行（Rust）。**
- 変更: `tests/string_interpolation.rs`。
- 内容: 付録 E の補助 `hole_kinds` と 7 つのテストを足す。
- 確認: `cargo test --locked --test string_interpolation` は `running 18 tests`（既存 11 + 新規 7）。HEAD では `holes_that_are_places_stay_borrows` と `explicit_borrows_of_temporaries_stay_rejected` の 2 つが通り（通らなければ停止条件 1）、ほかの 5 つは `E1013`（または期待した `E1012`・`E1014`）の違いで落ちる。落ちる件数を控える。

**手順 3: 修正。**
- 変更: `src/check.rs`。
- 内容: 「設計」のコードのとおり。`hole_operand` の `place` の計算を直し、`hole_place`・`constant_part` を足す。**ほかは触らない**。
- 確認: `cargo test --locked --test string_interpolation` の 18 件がすべて通る。`cargo build --release --locked` のあと、付録 C の fixture を `target/release/tsuzuri check` が通す（手順 1 の 15 件が 0 件になる）。`$"len = {s.length}"` の例（「現状」）が通る。

**手順 4: 今受理しているプログラムが変わらないこと。**
- 変更: なし。
- 内容: 手順 1 の IR のループを `/tmp/tz-D13/after/` へ出して `cmp`（102 件）。ライブラリ 7 件と `Semantics.tz` の `check` の出力を手順 1 と `diff`。`node scripts/check-docs.mjs` を、文書を変える前の既存の補間のページ（`literals-and-strings/interpolated-strings.md`・`strings.md`）に流す。
- 確認: 102 件が 1 byte も違わない（違えば停止条件 2）。`check` の出力は手順 1 と同一。

**手順 5: E2E の suite。**
- 変更: `tests/fixtures/interpolation_temporaries/Main.tz`（新規。付録 C）、`tests/features.mjs`（付録 D の suite を `string_interpolation` の次に足す）。
- 内容: 付録 C を写し、付録 D の suite を足す。期待値は suite の中の JavaScript で作る（コンパイラの出力を写さない）。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri interpolation_temporaries` が通る（native と wasm32 の `-O0`・`-O3`、`live == 0`、import なし、IR は決定的）。続けて `node tests/features.mjs target/release/tsuzuri string_interpolation`・`constants`・`const_statics`・`borrowed_records` が通る。

**手順 6: 文書。** 「ドキュメント」のとおり。`node scripts/check-docs.mjs <変更したページ>` を通す。

**手順 7: 全体の確認。** GUIDE §3 の全体（`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`RUST_MIN_STACK=4194304 cargo test --locked`・`node tests/features.mjs target/release/tsuzuri`）、§3.1 の 4 つの回帰テスト、`sh scripts/check-runtime-includes.sh`。手順 1 の IR と、ライブラリの `check` の出力が前後で 1 byte も違わない。

## テスト計画

コードは付録 C（fixture）・D（suite）・E（Rust）に全文がある。期待値は、決定表の「修正後」と、JavaScript で書いた参照から作り、**コンパイラの現在の出力を写さない**。

| ファイル | テスト | 内容 |
| --- | --- | --- |
| `tests/string_interpolation.rs` | `holes_that_are_places_stay_borrows`（手順 2 の時点で通る） | P01〜P07・R01 の穴が `Borrow` のまま（`Nums[1]`・`Corner.a` を含む）。回帰の固定 |
| 同上 | `holes_that_are_not_places_are_temporaries` | N01〜N14 の代表が `BorrowOperand`（型付き木を歩く補助 `hole_kinds`） |
| 同上 | `property_and_projection_holes_are_accepted` | N01〜N14 の全形が `analyze` を通り、IR が決定的で `@printf` などを含まない（`accepts`） |
| 同上 | `holes_that_move_out_of_a_temporary_are_rejected` | E01〜E03 が `E1012` |
| 同上 | `explicit_borrows_of_temporaries_stay_rejected` | X01 が `E1013` |
| 同上 | `a_temporary_hole_leaves_its_source_free` | K01 が `E1014`、K02 が通る |
| 同上 | `format_specs_apply_to_property_holes` | `{s.length:x}` が通り、`{s.length:.2}` が `E1003` |
| `tests/features.mjs` の suite `interpolation_temporaries` | 15 関数・39 ケース | 値・評価順・`live == 0`・drop・WASM の import なし（native/wasm32 × `-O0`/`-O3`） |

**既存テストへの影響**: なし。穴の中の `.length` などを期待するテストも、穴の `E1013` を期待するテストも既存にない。既存のテストの期待値が変わったら停止条件 2 か 3 を疑う。**全部流すもの**（GUIDE §3.1 の選択表）: `tests/string_interpolation.rs`・`strings.rs`・`display_parse.rs`・`types_ownership.rs`・`borrowed_records.rs`・`constants.rs`・`copy_inventory.rs`・`warnings.rs`・`diagnostics.rs`、`features.mjs` の suite `string_interpolation`・`constants`・`const_statics`・`borrowed_records`・`interpolation_temporaries`、`strings.mjs`・`display_parse.mjs`。

**IR の確認**: 手順 4 の 102 件の比較が、今受理している穴の IR が変わらないことの証拠。suite の `inspect(ir)` は `@printf`・`@snprintf`・`@strtod`・`@strtof` を含まないことを見る（`string_interpolation` の suite と同じ）。

**性能**: 速度の主張はしない。一時値の穴の IR は付録 B の数（確保が増えない）を記録する。

## ドキュメント

意味が変わる（書けなかった形が書ける）ので更新する（GUIDE §8）。漏れは `grep -rn "E1013\|\.length" _tsuzuri/language-reference/literals-and-strings docs/language.md docs/architecture.md` で探す。置き換える文は付録 F にある。

- `_tsuzuri/language-reference/literals-and-strings/interpolated-strings.md`: ①「穴は 1 行に収めます」の段落から、`添字や .length も、…E1013 になることがあります。` の 1 文を削除する。②「この記事のポイント」に 1 行足す。③「穴の評価と所有権」の「各穴の値は消費しません。…」の段落を、付録 F の段落と決定表（場所・参照・一時値の 3 行と、書けない 3 つの形）に置き換える。④ `run=` つきの例を 1 つ足す（`$"{s.length} {xs.length} {s[0]}"` → `5 3 104`）。
- `_tsuzuri/language-reference/literals-and-strings/strings.md`: 添字の段落の「添字や `.length` を補間の穴へ直接書くと、…先に `let` で束縛してください。」を、付録 F の 1 文に置き換える。
- `docs/language.md` の「文字列補間」の箇条書き「各穴の値は消費せず借用して表示します。…」を、付録 F の文に置き換える。
- `docs/architecture.md` の補間の段落（「`TypedHole.operand` は常に共有借用 `ref U` となります。評価場所（…）」）を、付録 F の文に置き換える。
- `_features/README.md` の状態と、GUIDE §13.2 の書式の完了報告（`## 実装と検証（YYYY-MM-DD）`）は、実装者が末尾に書く。F14 の落とし穴の行は編集しない（F14 の実装者の仕事）。
- 例は `node scripts/check-docs.mjs <ページ>` を通す。`run=` の期待値は URL エンコードの標準出力（`5%203%20104`）。

## 受け入れ条件

- [x] `hole_operand` が型付けした式で分類し、決定表の「修正後」の列のとおりになる: N01〜N14 が通る、E01〜E03 が `E1012`、X01 が `E1013`、K01 が `E1014`、K02 が通る。
- [x] 今受理している穴の型付き木と IR が変わらない: 現在のコーパス 114 件の IR が 1 byte も変わらず、`Nums[1]`・`Corner.a` が `Borrow` のまま（Rust のテスト）。
- [x] 付録 E の 7 つの Rust のテストが通る。
- [x] suite `interpolation_temporaries` が native/wasm32 × `-O0`/`-O3` で通る（`live == 0`、import なし、IR は決定的）。
- [x] `git diff --stat` が `src/check.rs`・`tests/`・文書だけ（`src/ownership.rs` と診断の文言は変わらない）。
- [x] 文書 4 か所と索引を更新し、変更したリファレンスの `check-docs.mjs` が通る。
- [x] 現在の AGENTS の T0〜T2 に従い、fmt・all-target Clippy・全 Rust テスト・関連 E2E・GUIDE §3.1 の 4 回帰テスト・runtime include inventory が通る。stack の大きさは変更しない。
- [x] `_features/README.md` の状態を `done` にし、GUIDE §13.2 の完了報告を末尾に書く。人間の今回の指示に従い、自分のブランチにコミットする。

## 落とし穴

- **平たい定数の一部は、型検査の時点で根が `Call(constant)`。** `is_place` だけで判定すると `{Nums[1]}` が一時値になり、静的データの借用がコピーの退避に変わって IR が変わる（手順 4 が落ちる）。`constant_part` が要る。平たくない定数（`Origin.x`・`Words[1]`）は静的データにならないので、一時値でよい。
- **文字列・`utf8string` の添字は場所ではない。** `is_place` も `Ownership::place` も、添字の場所は配列・リスト・`Vec`・固定長配列だけ。
- **`.length` の型は `i64`。** 書式指定は `i64` に適用する（`{s.length:.2}` は `E1003`）。
- **`ExprKind::Name` の腕を触らない。** D07 の追記 5 の修正（引数のない関数・union の case）がそこにある。
- **`coerce_argument` を呼ぶのは場所のときだけ。** 呼ぶと `Borrow` ができ、場所でない値なら `E1013` に戻る。
- **新しい `rejects` のテストは、型を通してから書く。** `E1012`・`E1013`・`E1014` は所有権の検査で、型の誤りがあると出ない。
- **評価順。** 一時値の穴は補間の中の位置で 1 回だけ評価する（`evaluation_order` の `stamp` が固定する）。解放は結合のあとで、穴の順。
- **E2E の `export` の型。** 引数と結果は `i64`（`bool` は 1／0）。文字列は `digest` で `i64` にして返す。
- **新しい関数に `#[inline(never)]`。** `hole_place`・`constant_part` は、再帰する型検査の経路の frame を増やさない（GUIDE §11.1。2 MiB のテスト stack の回帰テスト）。

## 対象外

- 明示の `ref (一時値)`・`ref s.length` を穴で通すこと（`X01`。D4）。
- 一時値の基底から非 Copy の要素・フィールドを借りること（`E01`〜`E03`。D5。新しい型付き木が要る）。
- `Ownership::place` の緩和、`E1013` の文言の変更、穴以外の `E1013`（`Tensor.rank (ref (Tensor.permute …))`）。
- 4 つある `is_place`（`check.rs`・`constants.rs`・`control.rs`・`ownership.rs`）の重複の整理。
- 注釈のない整数の束縛を借りた参照の穴（`let n = 5`、`let r = ref n`、`$"{r}"`）が HEAD で `E1005`（`no instance for Display<ref i32>`）になる既存の挙動。穴の一時値とは別の問題で、`coerce_argument` の `literal_match`（`ref {integer}` は参照ではないとみなす）が関係すると見られるが、確かめていない。直すなら別のチケットにする。
- 書式指定・`Display`・`Format` の規則の変更、構文・edition・CLI・予約語の追加。
- 速度の主張。

## 決定事項

状態はすべて承認済み（D-50）。

- **D1: 直す場所は型検査の `hole_operand`。所有権の検査は変えない。** 理由: `Ownership::place` は正しい（`let r = ref (f x)` は `E1013` でなければならない）。誤りは、場所でない値に `Borrow` を作る側。D07 の設計も「型付けした式の place 判定と同じ規則」だった。却下: `place` が一時値を受ける（本物の `E1013` が消える）、パーサで `.length` を呼び出しに脱糖する（`s[0]`・`(f x).name` が直らず、D07 が避けた型の誤りになる）。
- **D2: 場所かどうかは型付けした式で決める（`TypedExpr::is_place`）。書き方は使わない。** 理由: `.length`・`s[0]`・`(f x).name` は書き方が場所で値が値。却下: `.length` だけの特例（原因が残る）。
- **D3: 平たい定数の一部（`C[i]`・`C.f`）は場所のまま（`constant_part`）。** 理由: 今受理していて、静的データを読む借用で、IR を変えない。却下: 一時値にする（IR が変わる）。
- **D4: 明示の `ref (一時値)` は `E1013` のまま。** 理由: 明示の借用は場所を借りる書き方で、ほかの場所と同じ規則にする。穴は `ref` なしで一時値を借りる。却下: 穴だけ通す（`let r = ref (f x)` と食い違う）。
- **D5: 一時値から取り出す非 Copy の要素・フィールドは `E1012`（既存の規則）。** 理由: `let x = (names ())[0]` と同じ規則で、隠れた基底の射影を作らない。却下: 基底を slot に置いて借りる（新しい型付き木が要る）。
- **D6: 一時値の穴は評価した時点の値だけを持ち、元の場所を借りたままにしない。** 理由: 既存の `{n + 1}` と同じ。却下: `.length` を場所のように借りる（`E1014` が増え、書き方で規則が割れる）。
- **D7: 診断のコードと文言を足さない。** 既存の `E1012`・`E1013`・`E1014`・`E1003` を使う。
- **D8: edition・構文・CLI・予約語を足さない。** 理由: バグの修正と受理を増やす拡張（`docs/stability.md`）。

## 付録

実測に使ったプログラム（A）、設計のコードを当てた複製での実測（B）、fixture の全文（C）、suite の全文（D）、Rust のテストの全文（E）、文書の置き換え文（F）は [D13-appendix.md](D13-appendix.md)。

## 実装と検証（2026-10-23）

### 実装した範囲

Phase 1（唯一の Phase）と手順 1〜7 を完了しました。`hole_operand` の射影だけを型付きの場所で分類し、`hole_place`・`constant_part` を追加しました。場所・平たい定数の部分は既存の借用を保ち、値の射影は既存の `BorrowOperand` の経路を使います。`src/ownership.rs`・診断の文言・LLVM の実装・名前だけの穴は変更していません。対象外の明示的な一時値の借用や非 Copy の取り出しは、仕様どおり拒否します。

### 確認と結果

重いコマンドはすべて `node scripts/slot.mjs --` で実行しました。LLVM/LLD は Homebrew の PATH を使い、`RUST_MIN_STACK` は未設定のままです。

| 確認 | 結果 |
| --- | --- |
| 基準 | `main` の `41f80bc7`、CI `39954858404` は成功 |
| 変更前の再現 | 見出しの例は `E1013`。付録 A は `E1013` 38 件・`E1014` 1 件、fixture は `E1013` 15 件 |
| テスト先行 | `cargo test --locked --test string_interpolation`: 18 件中 13 件成功・5 件失敗。既存の場所と明示的な借用の回帰テストは成功 |
| 修正後の Rust | 同じ 18 件が全件成功。固定長配列・参照を返す呼び出しの参照外し・補間外の明示的な借用・分岐によるムーブも固定 |
| 決定表 | 付録 A の診断は `E1012` 3 件・`E1013` 2 件・`E1014` 1 件だけ。見出しの例と fixture の `check` は成功 |
| コーパス | 105 fixtures + 9 examples = 114 件の `--emit llvm -O0` が前後で byte 一致。非 entry fixture 5 ディレクトリと `Semantics.tz` の 6 件の `check` の stdout・stderr・終了コードも一致 |
| fmt / Clippy / build | `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo build --locked` が成功 |
| 全 Rust | `cargo nextest run --locked`: 1,725 件成功・0 件失敗・2 件 skip。GUIDE §3.1 の 4 回帰テストを含む |
| 新規 E2E | `features.mjs ... interpolation_temporaries`: 15 関数・39 ケース、native/wasm32 × `-O0`/`-O3` が成功。決定的 IR・`live == 0`・import なし |
| 既存 E2E | `string_interpolation` 82・`constants` 9・`const_statics` 52（WASM threads の静的データも）・`borrowed_records` 57 ケースが同じ 4 経路で成功 |
| 文字列 | `strings.mjs`: 各最適化で 3,973 ケース・75 traps、全 Unicode scalar の 4,344 bounded batches・28 strict traps、native/WASM と console の検査が成功 |
| 表示・解析 | `display_parse.mjs`: 各最適化で 109,803 displays・147,480 parse/round-trip、native/WASM と console・custom instance・cleanup・standalone link の検査が成功 |
| リファレンス | `check-docs.mjs`: 3 ページ・141 links・10 例・20 native runs（O0/O3）成功 |
| Runtime inventory | `sh scripts/check-runtime-includes.sh`: 58 files |

確保の形は既存の `emits_one_allocation_and_deterministic_ir` を拡張し、プロパティの穴も UTF-16 の native/WASM IR で結果の確保が 1 回、穴の退避の `@tz.alloc` が 0 回であることを固定しました。速度は主張しません。

### チケットから読み替えた点・残作業

- 実装前に README の開発メモへ、D-59 の対象が最後の API（`Array.write 0 9 a`）と現在の AGENTS の T0〜T3 を優先することを書きました。古い release ビルド・stack 増加・全 E2E の一律実行の指示ではなく、dev バイナリ・既定の stack・関連 E2E を使い、全 platform/release は T3 に任せます。
- コーパスの件数は現在の木で数え直しました（102 → 114 IR、8 → 6 check 出力）。診断を出す非 entry の入力も、成功と取り違えず stdout・stderr・終了コードを比較しています。
- 長い絶対 TMPDIR は macOS の Unix socket の上限を超え、相対 TMPDIR は cwd を変える既存の target-cache テスト 1 件を落としました。最終確認は専有の短い絶対 scratch path を使い、server 19 件 + target-cache 1 件の 20 件が成功した後、全 1,725 件を通しました。共有 build cache の I/O 停滞もあり、最終確認では自分の scratch の `TSUZURI_CACHE_DIR` を使いました。共有 cache やソース・stack・資源上限は変更していません。
- 機能の残作業はありません。F14 の注意書きや他チケットは編集していません。

### 更新した文書とファイル

利用者向けは `_tsuzuri/language-reference/literals-and-strings/interpolated-strings.md`・`strings.md` と `index.md`、契約は `docs/language.md`・`docs/architecture.md` を更新しました。ほかは `README.md`・`src/check.rs`・`tests/string_interpolation.rs`・`tests/features.mjs`・`tests/fixtures/interpolation_temporaries/Main.tz`・`_features/README.md` と、本チケット・付録の `_completed/` への移動です。
