# A32: 規約 `[Read]`・`[Mut]` の関数型が期待される位置へ、引数を `ref T`・`ref mut T` と注釈したラムダを直接書くと `E1003` になる（`if n > 0 then text_length else \(text: ref string) -> …`）

| 項目 | 内容 |
| --- | --- |
| ID | A32 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | A18 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-06（A29 の 2 回目の修正の作業中に発見。`main` の `72f06862` で再現し、原因を実験で確かめた） |
| 承認 | 不要（今型エラーになるプログラムを受理するだけで、今受理しているプログラムの型と IR は変えない） |
| 改善する劣位 | 不具合: 引数規約 `[Read]` を持つ関数型（edition 2027 の `def text_length :: string -> i64`、edition 2026 の `pure def text_length :: string -> i64`）や `[Own, Mut]` を持つ関数型（`def bump :: i64 -> mut Counter -> unit`）が期待される位置へ、引数を `ref string`・`ref mut Counter` と注釈したラムダを直接書くと、`E1003 expected ref string, found string` になる。同じラムダを `let` で束縛してから使うと通り、注釈を `string` と書いても通る |
| 手本にする既存実装 | `src/closures.rs:22` の `lambda`（期待型の規約から read value parameter を決める、63〜81 行）と、`def` の引数の決め方（`src/check.rs:8815〜8842`）。注釈付きのパターン（`src/control.rs:693`） |
| 主な影響ファイル | `src/closures.rs`, `tests/parameter_conventions.rs`, `tests/fixtures/conventions/Main.tz`, `tests/features.mjs`, `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`, `_features/README.md` |

## 目的

規約を持つ関数型が期待される位置で、引数に参照の型を注釈したラムダを、`let` で束縛してから使う形と同じく受理する。

規約だけが違う関数型は、型検査の `unify`（`src/polymorph.rs`）にとって 1 つの型で、生成コードでも 1 つの型です（A29 の `codegen_identity`）。
規約 `[Read]` の `string -> i64` の引数の型はすでに `ref string` なので、`\(text: ref string) -> …` はその型そのものを注釈しています。
それなのに、ラムダを期待型と一緒に検査したときだけ注釈が引数の型と合わなくなります。

## 着手条件と停止条件

### 着手条件

- A18 が `_features/README.md` で done であること（HEAD で done）。確認: `grep -nE "^\| A18 " _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、「再現」を実行して、今の結果（失敗する形と束縛した形の値）を記録していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 今受理しているプログラム（`tests/` の全件、`examples/`、`tests/fixtures/`）の型・診断・`--emit llvm` の IR が、修正の前後で変わる。
- 修正が、注釈の無い引数（`\text -> text.length`）、値の型を注釈した引数（`\(text: string) -> …`）、分解するパターンの引数（`\(a, b) -> …`）の扱いを変える。
- 「対象外」の形まで直さないと、このチケットのテストが書けない。

## 現状（`main` の `72f06862` で確認・調査）

### 再現（検証済み。macOS arm64。manifest なし = edition 2027）

```text
def text_length :: string -> i64
fn text_length text = text.length

def pick :: own i64 -> i64
fn pick n =
    let chosen = if n > 0 then text_length else \(text: ref string) -> text.length * 10
    let text = "hello"
    chosen (ref text)

pick 3 + pick (-3) * 100
```

`tsuzuri run` → `Main.tz:6:50: error[E1003]: expected ref string, found string`（ラムダの引数のパターンの位置）。
`let scaled = \(text: ref string) -> text.length * 10` と `if n > 0 then text_length else scaled` に分けると `5005`（手で数えた `5 + 50 · 100`）。

### 同じ原因の形（`main` の binary で実行して確かめた。値は束縛した形のもので、どれも手で数えたもの）

| 形 | 直接書いた形 | 束縛した形 |
| --- | --- | --- |
| `if` の枝（上の「再現」） | `E1003 expected ref string, found string` | 束縛した形 `5005` |
| `match` の腕: `\| 0 -> text_length \| _ -> \(text: ref string) -> text.length * 10`（`pick 0 + pick 1 * 100`） | `E1003`（同じ文言） | 束縛した形 `5005` |
| ジェネリックなレコード: `if n > 0 then first else Box { value: \(text: ref string) -> text.length * 10 }`（`first = Box { value: text_length }`） | `E1003`（同じ文言） | ラムダだけを束縛する形（`let scaled = \(text: ref string) -> text.length * 10` と `if n > 0 then first else Box { value: scaled }`）は `main` でも `5005`（期待型が `Box` を `[Read]` の側に決め、`Box` の型は 1 つ）。`Box` ごと束縛する形（`let second = Box { value: \(text: ref string) -> text.length * 10 }` と `if n > 0 then first else second`）は `main` では A29 の `E2002`（`redefinition of type`）、A29 の後は `5005` |
| `Vec`: `Vec.push text_length readers` の後の `Vec.push (\(text: ref string) -> text.length * 10 + n) readers` | `E1003`（同じ文言） | 束縛した形は `main` では A29 の `E2002`（`invalid redefinition of function`）、A29 の後は `51005`（`n = 1`） |
| 規約 `[Own, Mut]`: `if n > 0 then bump else \(step: i64) (counter: ref mut Counter) -> counter.count = counter.count + step * 100` | `E1003 expected ref mut Main.Counter, found Main.Counter` | 束縛した形 `301004`（`pick 3 + pick (-3) * 1000`） |
| edition 2026: `pure def text_length :: string -> i64` と、上の「再現」の `if` | `E1003`（同じ文言） | 束縛した形は `E1029`（束縛したラムダは純粋と推論されない。別の規則） |

注釈を `string` と書いた `\(text: string) -> …`、注釈の無い `\text -> …`、明示の `ref string -> i64` が期待される位置の `\(text: ref string) -> …` は、どれも今通ります。

### 原因（実験で確かめた）

1. `src/parse_control.rs:579` の `explicit_lambda` は、名前だけでない引数（ここでは注釈付きのパターン `(text: ref string)`）を、生成した名前 `$lambda<位置>` の引数にし（607 行）、
   本体を `match $lambda<位置> with | (text: ref string) -> 本体`（`MatchOrigin::LambdaDestructuring`、662〜678 行）で包みます。
2. `src/closures.rs:22` の `lambda` は、期待型があると、引数の型を `call_signature` から取ります（規約 `[Read]` の引数は `ref string`）。期待型の規約が `Read` か `Mut` で、
   引数の型が参照なら、その局所を `read_value_parameters` に入れます（63〜81 行）。
3. `src/check.rs:11870` の `finish_expression` は、read value parameter の局所を読むと参照外しします（11878 行。型は `string`）。
4. 1 の `match` の主語は 3 で `string` になり、`src/control.rs:693` の注釈付きパターンが主語の型 `string` を注釈 `ref string` と `same` で比べて `E1003` になります。

期待型が無いとき（束縛した形）は、引数の型が新しい型変数で、パターンがそれを `ref string` に決め、ラムダの型は規約の無い `ref string -> i64` になります。
`unify` は規約を比べないので、`[Read]` の `string -> i64` と出会っても通ります。注釈が `string` なら、3 の主語の型と注釈が等しいので通ります。

実験: 2 で生成した名前（`$lambda` で始まる名前）の引数を `read_value_parameters` に入れないようにすると、上の表の edition 2027 の直接書いた形がすべて通り、表の値を返しました（2026 の行は試していません）。
ただしこの変更は分解するパターンの引数（`\(a, b) -> …`）の扱いも変えるので、修正の案ではありません（「設計」）。

## 仕様

1. 期待型の規約が `Read`（`Mut`）の引数で、その引数のパターンが `ref T`（`ref mut T`）の注釈を持つとき、ラムダはその引数を read value parameter にせず、注釈のとおり参照の局所として束縛する。
   注釈付きのパターンは参照の型の主語と比べられ、本体の `text` は `ref string` の局所になる（束縛した形と同じ型）。
2. 注釈の無い引数、値の型の注釈（`\(text: string) -> …`）、分解するパターン（tuple・record・構築子）の引数は、今のまま read value parameter にする。
3. 今受理しているプログラムの型・診断・IR は変えない（1 の形は今すべて `E1003` で失敗している）。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/closures.rs` | `lambda`（63〜81 行） | 本体が `MatchOrigin::LambdaDestructuring` の `match` の連なりで始まるとき、その主語の生成した名前と、腕のパターンが `PatternKind::Annotated(_, 参照の型)` かどうかを集める helper を足し、その引数を `read_value_parameters` に入れない |
| `tests/parameter_conventions.rs` | 新しいテスト | 下の「テスト計画」 |
| `tests/fixtures/conventions/Main.tz`・`tests/features.mjs` | suite `conventions` | 新しい export と期待値 |

### 1 の判定を `src/closures.rs` に置く理由

- 判定に要るのは「期待型の規約」と「引数のパターン」の 2 つで、どちらも `lambda` が持っている（パターンは本体の先頭の `match` にある）。
- `src/control.rs` の注釈付きパターンの側で、主語が read value parameter の参照外しなら外す、という案もあるが、パターンの検査は `match` 一般のもので、ラムダの引数だけの規則を持ち込むことになる。
- `def` の引数（`src/check.rs:8815〜8842`）は宣言の型に注釈を書くので、この食い違いは起きない。

## 実装手順

### 手順 1: ベースラインと記録

- 変更: なし（`target/scratch/a32/` の program だけ）。
- 内容: GUIDE §2.3 の基準コマンド。「再現」と「同じ原因の形」の表の全行を 2026 と 2027 で書き、結果を記録する。`tests/fixtures/`・`examples/` の `--emit llvm`（native と `--target wasm32`）を保存する。
- 確認: 記録が「現状」と一致する。違えば停止する。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/parameter_conventions.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`。
- 内容: 表の直接書いた形の受理のテストと、変えてはいけない形（注釈の無い引数、`\(text: string) -> …`、分解するパターン）の型を固定するテスト。
- 確認: 受理のテストだけが失敗する。

### 手順 3: 判定を足す（仕様 1）

- 変更: `src/closures.rs` の `lambda`。
- 確認: 手順 2 のテストがすべて成功する。`cargo nextest run --locked` の全件が成功する。

### 手順 4: 既存のプログラムが変わらないこと

- 確認: `tests/fixtures/`・`examples/` の `--emit llvm` が手順 1 の保存と `cmp` で一致する。

### 手順 5: ドキュメントと最終確認

- 「ドキュメント」の節のとおり。`node scripts/check-docs.mjs <変えたページ>`。T2。

## テスト計画

### Rust テスト

- `tests/parameter_conventions.rs`: `annotated_reference_lambdas_meet_convention_function_types`（新規。表の 2027 の形と 2026 の `pure def` の形の受理。`check(edition, …)` を使う）、
  `lambda_parameters_without_reference_annotations_keep_reading_values`（新規。仕様 2 の形の型）。

### E2E

- suite `conventions`（`tests/features.mjs`、native と WASM の `-O0`・`-O3`）に、`if`・`match`・ジェネリックなレコード・`Vec`・`[Own, Mut]` の形を 1 つにした export を足す。期待値は手で数える
  （表の値: `5005`・`5005`・`5005`・`51005`・`301004` から組み立てる）。

### 既存テストへの影響

- 無い（停止条件）。

### 性能

- 型検査の時間だけ。ラムダごとに本体の先頭の `match` を引数の数だけたどる。

## ドキュメント

- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`（「呼び出し引数の暗黙の借用」の、ラムダの仮引数に型を書く説明、266 行）と
  `_tsuzuri/language-reference/values-and-functions/functions.md`（引数規約）を `grep` し、規約を持つ関数型が期待される位置でも `\(text: ref string) -> …` と書けることを、必要なら足す。
- `docs/language.md` の引数規約の節を同じく確かめる。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] 「再現」が 2027 で `5005` を返す。
- [ ] 「同じ原因の形」の表の直接書いた形が、表の値（2026 の形は受理）になる。
- [ ] 注釈の無い引数・値の型の注釈・分解するパターンの引数の型が変わらない。
- [ ] `tests/fixtures/`・`examples/` の IR が変わらない。
- [ ] T2 が成功する。

## 落とし穴

- 生成した名前（`$lambda`）だけで判定すると、分解するパターンの引数まで read value parameter でなくなる（「現状」の実験）。パターンが参照の型の注釈かどうかで決める。
- パターンは `match` の連なりの中で、最初の引数のものが外側にある（`explicit_lambda` は `patterns` を逆順に包む）。引数の位置と名前で対応させる。
- `\(text: ref string) when … -> …` のようにガードがあると、`match` の連なりの内側にガードの `match` が入る。

## 対象外

- **規約の無い値の関数型が期待される位置**の注釈: `def apply :: (string -> i64) -> i64` の `apply (\(text: ref string) -> …)` も `E1003 expected ref string, found string` になるが、
  原因が違う。`def` の署名の中の関数型の引数 `string` は補完されず値の引数のまま（A18 の「補完は引数の型にだけ作用する」。`f text` の後に `text` を使うと `E1012`）なので、
  注釈 `ref string` は期待型と本当に違う。`let scaled = \(text: ref string) -> …` と `apply scaled` が通るのは、関数の値に `callback_adapter`（`src/check.rs:13265`）が
  `ref T` の引数を値の引数へ合わせる adapter を作るからで、期待型と一緒に検査するラムダには作らない。直接書いたラムダにも adapter を作るかどうかは別の判断なので、必要なら別のチケットにする。
- 束縛したラムダが純粋と推論されない edition 2026 の `E1029`（「同じ原因の形」の表の最後の行）。

## 着手時の再検討（2026-11-08、HEAD b3dd7f1c）

HEAD の dev binary（`target/scratch/base/tsuzuri`）で、「再現」、「同じ原因の形」の全行、対照の形、監査で見つけた形を実行しました。記録は `target/scratch/audit-A32/baseline.md` です。`main` の最新の `ci.yml`（`b3dd7f1c`）は成功しています。

### 確かめたこと

- 直接書いた形は、表の 2027 の 5 行と 2026 の `pure def` の 4 形（`if`・`match`・`Box`・`Vec`）がすべて今も `E1003` です。文言も同じです。
- A29 はマージ済みです。束縛した形は `Box` ごとの束縛が `5005`、`Vec` が `51005` になり、`E2002` は出ません。ほかの束縛した形は表の値（`5005`・`5005`・`301004`）のままです。2026 の束縛した形は `E1029` のままです。
- 次の形は今も通ります: 注釈の無い引数、`\(text: string)`、明示の `ref string -> i64`、`\(a, b)`。
- 対象外の `apply (\(text: ref string) -> …)` は `E1003` です。束縛した形は `50` で、`callback_adapter`（`src/check.rs:13558`）を通ります。
- 意味が同じで行番号だけ移ったもの:
  - `lambda` の規約の判定: `src/closures.rs:62〜82`
  - `def` の引数: `src/check.rs:8943〜8962`
  - `finish_expression`: `src/check.rs:12012`（参照外しは 12019）
  - `explicit_lambda`: ガードの `match` が `src/parse_control.rs:643〜663`、引数のパターンの `match` が 664〜681
  - `borrowing.md` の該当箇所: 271 行
- edition 2026 は `Tsuzuri.toml` に `edition = "2026"` と書きます（manifest の無い入力は `--edition 2026`）。

### チケットと食い違った点

1. **仕様 3 の前提「1 の形は今すべて `E1003`」は成り立ちません。** 参照外しした型が推論変数か参照のときは、今通るプログラムがあります。
   - 期待型が `[Read] ref ?t -> i64`（`def inspect :: 'a -> i64` の値）のとき、`if n > 0 then inspect else \(text: ref string) -> …` は通ります。型は `ref ref string -> i64` です（束縛した形は `ref string -> i64`）。
   - `Vec` の要素型が先に `[Read] ref (ref string) -> i64` に決まったあとの `Vec.push (\(text: ref string) -> …) readers` も通ります（`501050003`）。

   「設計」のとおり注釈だけで決めると、前者は型と IR が変わり、後者は `E1003` になります。どちらも停止条件に当たります。
2. **`def` でも同じ原因で失敗します。** `def bump :: i64 -> mut Counter -> unit = \(step: i64) (counter: ref mut Counter) -> …` では、`normalize_definition` が `$lambda…` を `def` の引数にし、本体を同じ `LambdaDestructuring` の `match` にします。`src/check.rs:8945〜8962` は、`Mut` の引数と、型変数・関数型を含む読み取り引数を read value parameter にするので、`E1003` になります。同じく失敗する形:
   - `def size :: 'a -> i64 = \(value: ref 'a) -> 3`
   - 2026 の `pure def` の型変数・関数型の引数
   - `\(a: ref string) (b: ref mut [i64])`

   したがって「def の引数ではこの食い違いは起きない」は誤りです。具体的な型の読み取り引数（`def text_length :: string -> i64 = \(text: ref string) -> …`）は read value parameter ではないので、今も通ります。
3. 次の形も、直接書くと `E1003`、束縛すると通ります:
   - 型の別名（`type Borrowed<'a> = ref 'a` の `\(text: Borrowed<string>) -> …`）
   - `\(_: ref string)`
   - 参照の注釈で包んだ分解（`\((a, b): ref (string * string)) -> …`）
   - ガード付き、複数の注釈付き引数、名前と注釈の混在
   - `['a]` の読み取りに対する `ref [i64]`
   - `mut [i64]` に対する `ref mut [i64]`

### 更新した方針

- **仕様 1（改）**: 次の条件をすべて満たすとき、その引数を read value parameter にしません。
  - 期待型の規約が `Read` か `Mut` で、引数の型が `ref inner` である。
  - 引数の分解パターンの最上位が型注釈 `(p: A)` で、`A` を解決した型が参照の型である（`ref` と `ref mut` のどちらでもよく、別名も含む）。
  - `inner` を解決した型が `Type::Infer` でも `Type::Reference` でもなく、誤りも含まない。

  最後の条件を満たすとき、参照外しした主語は参照の注釈と `unify`（`src/polymorph.rs:401`）で決して合いません。したがって変わるのは、今型エラーになるプログラムだけです。上の 1 の 2 つのプログラムは今の型のままです。
- **仕様 2（改）**: 最上位が参照の注釈でないパターン（注釈の無い引数、値の型の注釈、tuple・record・構築子）は、今のままにします。可変性の違う注釈（`Read` の位置の `ref mut string`）も `E1003` のままです。文言は `found ref string` になります（レビュー第1回で変更: 引数の参照の型と合わない注釈は値 view のままにしたので、文言も `found string` のまま変わりません）。
- 判定は `lambda`（`src/closures.rs`）と `def` の引数（`src/check.rs:8943〜8962`）の両方に置き、同じ helper を使います。D1 の理由はそのまま当てはまります。`def` の側は、`normalize_definition` がラムダの引数をそのまま移したものです。
- helper（`src/closures.rs`、`pub(super) fn parameter_annotations(body: &Expr) -> BTreeMap<&str, &TypeExpr>`）:
  - 本体が `ExprKind::Match { origin: MatchOrigin::LambdaDestructuring, .. }` で、主語が `ExprKind::Name`、腕が 1 つである間だけたどります。
  - 腕のパターンが `PatternKind::Annotated(_, annotation)` のとき、主語の名前と `annotation` を対にして集めます。
  - ガードの `match`（主語が `ExprKind::Unit`。連なりの最も内側）で止まります。
  - 名前だけの引数は連なりに無いので、対象になりません。`derive.rs`・`contracts.rs` が生成するラムダは `MatchOrigin::Explicit` なので、触れません。
  - `while let` の連鎖は使わず、`loop { let … else { break } }` と書きます。
- 注釈が参照かどうかは、副作用の無い `resolve_type_with_kinds(annotation, self.module, self.names, &self.kinds)`（`src/check.rs:10166`）が `Ok(Type::Reference(..))` を返すかで決めます。誤りは「参照でない」として扱い、今のとおりパターンの検査に報告させます。`Checker::annotation`（`src/polymorph.rs:3832`）は `self.constraints` に制約を足すので呼びません。
- `def` の側では、helper の結果を `computation::expand(&mut function.body, …)` の前に作ります。`Mut` の挿入と、型変数・関数型の読み取りの挿入を、同じ条件で外します。`default_reads` への登録はそのままです。
- 対象外に足すもの: 期待型の引数が `ref ?t` や `ref (ref T)` のときの参照の注釈（上の 1）。直すと今通るプログラムの型が変わるので、必要なら承認を得て別チケットにします。
- AST・`FRONTEND_FORMAT`・診断の kind は変えません。

### テストの更新

- `annotated_reference_lambdas_meet_convention_function_types`: 表の 2027 の形と 2026 の `pure def` の 4 形に、上の 3 の形と `def both :: string -> string -> i64 = \(a: ref string) -> \(b: ref string) -> …` を足します。
- `annotated_reference_def_parameters_meet_their_conventions`（新規）: 上の 2 の形を確かめます。
- `lambda_parameters_without_reference_annotations_keep_reading_values`: 仕様 2 の形に加え、上の 1 の 2 つのプログラムが通ることを固定します。`Read` の位置の `ref mut string` は `E1003` のコードだけを照合します。
- E2E: fixture の `conventions` には `def bump :: mut i64 -> unit` がすでにあるので、新しい関数には別の名前を付けます（`bump_counter`・`record Counter`）。`def` の形も 1 つ足し、期待値は手で数えます。
- 手順 4 の IR の比較には、規約の無い位置で注釈付きのラムダを使う `tests/fixtures/generic_records/Main.tz` を必ず含めます。

### ドキュメント

- 「ドキュメント」の節に次の 2 か所を足します:
  - `values-and-functions/lambda-expressions.md` の「型注釈とパターン引数」
  - `functions.md` の「書き換える引数（`mut T`）」（`def … = \step (counter: ref mut Counter) -> …` の形）

### 進め方（オーケストレーターの決定）

- この節は上の「着手条件と停止条件」「仕様」「設計」「テスト計画」「ドキュメント」より優先します。古い本文は残します。
- GUIDE §13 の停止条件は、「健全性を保ち、今受理している安全なプログラムをすべて受理したままにする最も保守的な選択を取り、チケットに記録して続ける」に置き換えます。既存のテストの期待値は弱めません。安全でないプログラムは受理しません。
- 監査の計画の変更をすべて採ります: 「参照外しした型が参照でないと確かに言える」ときだけ外す条件、`def` の引数への同じ修正、注釈の解決に `resolve_type_with_kinds` を使うこと、上の「テストの更新」の追加のテスト（`x16c`・`x18` が今の値のまま受理される回帰テストを含む）、suite `conventions` の E2E（native と WASM、`-O0` と `-O3`）、`tests/fixtures/`・`examples/`（`generic_records` を含む）の IR を基準の binary と比べること（今受理しているプログラムは byte 単位で一致すること）。
- 監査の記録は `target/scratch/audit-A32/baseline.md` と、各形のディレクトリです（同じ内容を作業用に `target/scratch/a32/baseline/` へ写して使います）。

## 決定事項

### D1: 判定は `src/closures.rs` の `lambda` に置き、パターンの注釈で決める

- 決定: 「設計」のとおり、期待型の規約と引数のパターンの注釈が参照の型であることの両方で決める。
- 理由: 実験で原因の位置は確かめた（「現状」）。生成した名前だけで決めると分解するパターンを変え、注釈付きパターンの側で決めるとラムダの規則が `match` 一般に入る。
- 状態: 既定案（実装者はこの案に従う）。

## 実装と検証（2026-11-08）

手順 1〜5 をすべて実施しました（このチケットに Phase の区別はありません）。方針は「着手時の再検討」のとおりで、監査の計画の変更をすべて採りました。AST・`FRONTEND_FORMAT`・診断の kind は変えていません。

### 実装したこと

- `src/closures.rs`:
  - `parameter_annotations(body)`: 本体の先頭の `MatchOrigin::LambdaDestructuring` の `match`（主語が名前、腕が 1 つでガードなし）の連なりをたどり、腕のパターンの最上位が `PatternKind::Annotated` の引数について、生成した名前と注釈を集めます。ガードの `match`（主語が `()`）、ラムダ、それ以外の式で止まります。
  - `Checker::binds_annotated_reference(parameter, annotation)`: 注釈が `resolve_type_with_kinds` で参照の型（`ref`・`ref mut`、別名を含む）に解決され、引数の型 `parameter` が `ref inner` で、参照先の型 `inner`（解決後）が `Type::Infer`・`Type::Reference`・`Type::Application`・`Type::Partial` のどれでもなく、誤りも含まず、注釈が `parameter` と合う（`Inference::unifies`、レビュー第1回で追加）ときだけ真です。`Checker::annotation` は制約を足すので呼びません。
  - `lambda`: 期待型の規約が `Read`・`Mut` で引数の型が `ref inner` でも、`binds_annotated_reference` が真なら `read_value_parameters` に入れません。規約が無いときは連なりをたどりません。
- `src/check.rs`（`def` の引数の束縛）: `computation::expand` の前に `parameter_annotations(&function.body)` を作り、`Mut` の挿入と、型変数・関数型を含む読み取りの挿入を同じ条件で外します。`default_reads` の登録はそのままです。

### 手順と結果

- 手順 1: 監査の 68 個のプログラム（`target/scratch/audit-A32/`）を `target/scratch/a32/baseline/` へ写し、base の binary（`target/scratch/base/tsuzuri`、`b3dd7f1c`）で実行しました。結果は `baseline.md` と一致しました（`x22` の 3 つは `baseline.md` に結果が無く、`x22` が `E1003`、`x22b` が `32`、`x22c` が `E1003` でした）。`tests/fixtures/`・`examples/` を写し、`Main.tz` を持つ 180 個のディレクトリと、それに属さない 21 個の `.tz` を、native と `--target wasm32` の `-O0` で `--emit llvm --no-cache --no-server` しました（366 個の `.ll`。失敗する 36 個は標準エラー出力と終了コードを記録）。
- 手順 2: `tests/parameter_conventions.rs` に 3 つのテストを足し、修正前は `annotated_reference_lambdas_meet_convention_function_types` と `annotated_reference_def_parameters_meet_their_conventions` だけが `E1003` で失敗しました（`lambda_parameters_without_reference_annotations_keep_reading_values` は成功）。fixture `conventions` も base の binary では足した形の `E1003` だけで失敗しました。
- 手順 3: 上の実装。`cargo test --locked --test parameter_conventions`: 42 passed（37 + 新規 5）。新規の 5 つ:
  - `annotated_reference_lambdas_meet_convention_function_types`: 2027 の `if`（型を `let probe: ref string -> i64` で、本体の `text` が `ref string` であることを `let view: ref string = text` で確かめる）・`match`・`Box`・`Vec`・`[Own, Mut]`・`[Mut]` だけ・ガード・注釈 2 つ・名前と注釈・値の注釈と参照の注釈・`((a, b): ref (string * string))`・`(_: ref string)`・`['a]` に対する `ref [i64]`・`mut [i64]` に対する `ref mut [i64]`・別名 `Borrowed<string>`、2026 の `pure def` の `if`・`match`・`Box`・`Vec`。
  - `annotated_reference_def_parameters_meet_their_conventions`: `def` の `\(step: i64) (counter: ref mut Counter)`、`\(value: ref 'a)`（2027 と 2026 の `pure def`）、`\(a: ref string) (b: ref mut [i64])`。関数を `ref (A -> B)` と注釈した引数（2027 と 2026 の `pure def`、`let` で束縛した形）は `E1005`。
  - `lambda_parameters_without_reference_annotations_keep_reading_values`: `\(text: string)`・`\text`・`\(a, b)` の型（`let probe` で）と、本体の `text` が `string` であること（`let view: ref string = text` が `E1003`）、明示の `ref string -> i64`、`def` の具体型の読み取り。監査の `x16c`（型は `ref ref string -> i64` のまま、`ref string -> i64` の注釈は `E1003`）と `x18` の受理。`Read` の位置の `ref mut string`（`E1003`）、`Mut` の位置の `\(counter: Counter)`（`E1014`）、`apply (\(text: ref string) -> …)`（`E1003`）、下の `x22`（`E1003`）。
  - `annotated_reference_lambdas_compile_like_their_bound_form`: `if` と `[Own, Mut]` の直接書いた形と `let` で束縛した形で、ラムダから作った関数（と apply の thunk）の IR が同じ。
  - `annotated_reference_parameters_keep_the_borrow_rules`: 参照を返すラムダ・`def … = \(value: ref 'a) -> value` は引数が生きている間は受理し、ブロックの外へ出すと `E1013`。`def grab :: mut Counter -> ref mut Counter = \(counter: ref mut Counter) -> counter` の結果を持ったまま元を書き換えると `E1014`（どれも注釈の無い形・束縛した形と同じ診断）。
- E2E: fixture `conventions` に `annotated_reference_lambdas`（`if`・`match`・ジェネリックなレコード `Holder`・`Vec`・`[Own, Mut]`、`def scale_counter` の `\(step: i64) (counter: ref mut Counter)`）と `annotated_reference_forms`（ガード・注釈 2 つ・名前と注釈・`(_: ref string)`・別名・参照で包んだ分解・`['a]` と `mut [i64]` に対する配列、`def annotated_size` の `\(_: ref 'a)`、`def store_length` の `\(text: ref string) (cells: ref mut [i64])`）を足し、入力 `-3`・`0`・`1` の期待値を `tests/features.mjs` の BigInt の式で手で書きました。`node tests/features.mjs target/debug/tsuzuri conventions`: `conventions: native/WASM at O0/O3 passed`（43 cases）。
- 手順 4: 修正後の binary で同じ 201 個を同じコマンドで出力し、`diff -rq` で `.ll`（366 個）・標準エラー出力・終了コードのすべてが base と byte 単位で一致しました（`generic_records`・`conventions`（修正前の内容）・`type_aliases` を含む）。監査のプログラムのうち base で受理する 26 個の native と wasm32 の IR（52 個）も一致しました。
- 監査のプログラムの base と修正後の結果（native の `-O0`・`-O3` は全部同じ）:
  - 受理されるようになった形（値は手で数えた値と一致）: `r01` `5005`、`r02` `5005`、`r03` `5005`、`r04` `51005`、`r05` `301004`、2026 の `pure def` の `if`・`match`・`Box` `5005` と `Vec` `51005`、`x01` `5005`、`x02`・`x03`・`x03b`・`x04` `250007`、`x06` `4`、`x07` `8`、`x11` `705`、`x13` `5005`、`x14` `101002`、`x17` `907`、`x19b` `8`、`x21` `5005`、`x22c` `5`。
  - 拒否のまま（コードも同じ）: `x09`（`E1003`）、`x15`（`E1014`）、`x16`（`E1003`）、`x16f`（`E1003`）、`x18b`（`E1003`）、`x22`（`E1003`）、2026 の束縛した `if`（`E1029`）。`x10` は `E1003` のまま文言が `found ref string` になりました（レビュー第1回の後は文言も base と同じ `expected ref mut string, found string`）。`x19c`・`x20` は `E1003` から `E1005`（`only a function value can be called`。束縛した形の `x20b` と同じ。レビュー第1回で help が `(deref f) x` を案内するようになりました）になりました。
  - 受理していた形は値も変わりません（`c01`〜`c03`、`x05`、`x08`、`x16b`〜`x16e`、`x18`、`x19`、`x22b` など）。

### 実行したコマンド（T0〜T2）

- `cargo fmt --all -- --check`: 成功。
- `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo nextest run --locked`: 2985 tests run: 2985 passed, 4 skipped（手順 3 の後と、文書とこのチケットを直した後の 2 回。base は 2980 件）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 成功（43 cases）。
- `node tests/features.mjs target/debug/tsuzuri`（全 suite）: `features: 25173 cases passed`（native と WASM の `-O0`・`-O3`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/ownership-and-memory/borrowing.md _tsuzuri/language-reference/values-and-functions/functions.md _tsuzuri/language-reference/values-and-functions/lambda-expressions.md`: 成功（3 pages, 69 links, 46 checked examples, 92 native runs）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs --into _features/README.md`: 0 broken。`node scripts/check-links.mjs`: 0 broken。
- 診断の登録簿と std のタグは変えていないので、`gen-docs`・`check-cli-docs` は要りません。`src/runtime/` も変えていません。

### チケットから外れた判断

- **`x22`（`def both :: string -> string -> i64 = \(a: ref string) -> \(b: ref string) -> …`）は直さず、対象外にしました。** 監査はこの形を受理のテストに入れていましたが、原因が違います。`normalize_definition` は本体がラムダである間だけ引数を `def` へ移すので、外側のラムダの本体（パターンの `match`）で止まり、`def` は `a` だけを受け取ります。署名の残り `string -> i64` は `def` の引数ではないので補完されず（A18）、型は `ref string -> string -> i64`、内側の `b` は規約の無い値の引数です。`ref string` の注釈は期待型と本当に違い、「対象外」の `apply` の形と同じです（`\(a: ref string) -> \b -> …` でも、`both left right` の後の `right` は `E1012`）。テストは `E1003` を確かめ、`functions.md` に `\(a: ref string) (b: ref string) -> …` と 1 つのラムダに並べる書き方を足しました。
- 条件の除外に `Type::Application`・`Type::Partial` も足しました（監査は `Infer`・`Reference`・誤りだけ）。`unify` は型適用を参照と比べると `E1003` ではなく構築子の kind の誤りを出すので、その診断も変えないためです（より保守的）。
- 関数を `ref (A -> B)` と注釈した引数（`x19c`・`x20`）は監査の予想どおり `E1005` になります（拒否のまま）。束縛した形と同じ診断なので、テストは `E1005` を確かめます。
- 連なりのたどりは、clippy（`while_let_loop`）に合わせて `while let` で書きました（let の連鎖は使っていません）。ガードを持つ腕でも止まるようにしました。
- 監査の挙げたテストに加え、直接書いた形と束縛した形の IR の比較（`annotated_reference_lambdas_compile_like_their_bound_form`）と、借用の規則のテスト（`annotated_reference_parameters_keep_the_borrow_rules`）を足しました。
- `docs/architecture.md` の「所有権」の値 view の説明に、この例外を足しました（内部の不変条件が変わるため）。
- `_features/README.md` には A32 の行が一覧の表の 1 行しかありません（2026-11-05 の追加チケットの表には A31・A32 が無い）。その 1 行を更新しました。

### 更新した文書

- `_tsuzuri/language-reference/values-and-functions/lambda-expressions.md`（型注釈とパターン引数: 参照の型の注釈と `run=5005` の例、`'a` の読み取りと規約の無い引数の制限）
- `_tsuzuri/language-reference/values-and-functions/functions.md`（引数の所有権: `\(value: ref 'a)`。書き換える引数: `\step (counter: ref mut Counter)` の `run=42` の例と、入れ子のラムダの 2 段目が補完されないこと）
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`（呼び出し引数の暗黙の借用: 規約を持つ関数型が期待される位置のラムダの注釈）
- `docs/language.md`（引数の補完の節）、`docs/architecture.md`（所有権）
- `_features/README.md`（状態）

### 変更したファイル

- `src/closures.rs`、`src/check.rs`
- `tests/parameter_conventions.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`
- 上の文書と、このチケット（`_features/_completed/` へ移動）

### レビューの指摘への対応（第1回）

指摘ごとに、修正後の dev binary と base の binary（`target/scratch/base/tsuzuri`、`b3dd7f1c`）で再現してから対応しました。再現に使ったプログラムは `target/scratch/review-a32-ce/` と `target/scratch/a32/review1/` にあります。

1. **[major] 新しく受理する注釈付きの形で、`Array.write` を通して解放済みのメモリを読める**: 再現しました。`h22`（`def put :: mut [(unit -> i64)] -> [i64] -> unit = \(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> Array.write 0 (\() -> xs[0] * 1000 + xs.length) slot`）は、base では `E1003`、修正後は `-O0` で `5004`、`-O3` で `4` です。`h21`（閉包の側）は `-4996` と `4` です。**A32 では直さず、A31 の範囲として記録しました。** 理由は次のとおりです。
   - A32 は所有権検査を変えていません。新しく受理するプログラムには、どれも base が同じ本文の型と IR で受理する綴りがあります。閉包の側は `let` で束縛したラムダです（IR の一致は `annotated_reference_lambdas_compile_like_their_bound_form` が確かめています）。`def` の側は明示の `ref`・`ref mut` の署名です。
   - base はすでに次の形を受理し、どれも `-O0` と `-O3` で値が分かれます。
     - `h19`（注釈の無い `def`）、`h20`（束縛したラムダ）、明示の `ref mut` の署名（`h12`・`h14`・`h16`）。
     - `def put :: ref mut [(unit -> i64)] -> ref [i64] -> unit = \(slot: ref mut [(unit -> i64)]) (xs: ref [i64]) -> Array.write 0 (\() -> xs[0] * 1000 + xs.length) slot`（`review1/alias`、`5004` と `4`）。これは、パターンで束縛した別名 `slot` を通る書き込みです。
   - 新しい綴りだけを拒否する保守的な規則も検討しましたが、それでは穴を閉じられません。書き込み先の `Mut` の引数の参照先が借用を持ちうるときだけ値 view に戻す規則なら、`h21`・`h22` は `E1003` に戻ります。しかし、借用の出どころだけを注釈した `\(xs: ref [i64]) slot -> Array.write 0 (\() -> xs[0] * 1000 + xs.length) slot`（`review1/src_side`）が残ります。この形は base では `E1003`、修正後は `-4996` と `4` で、束縛した形 `review1/src_side_bound` は base でも `-4996` と `4` です。これも拒むには、本体（std の関数を含む）が追跡されない格納をするかどうかを型検査の段で調べる必要があります。それは A31 が所有権検査で行うことです。利用者に見える一時的な制限を足しても穴は閉じないので、足しませんでした。
   - この既知の不具合は、すでに綴りによらない形で書いてあります（`lifetimes.md` の現在の制限、`docs/language.md` の A31）。
   - A31 への申し送り: `def` の右辺の新しい形と、A32 の前からある「明示の `ref mut` の署名＋注釈付きのラムダ」の形では、`Mut` の引数への書き込みが、隠れた引数 `$lambdaN` をパターンで束縛した別名 `slot` を通ります。引数の局所そのものや、その read value の参照外しは通りません。A31 の格納の事実は、この別名をたどる必要があります。このことは `docs/language.md` の引数の補完の節に書きました。`h19`〜`h22`、`alias`、`src_side` は、A31 の受け入れテストの候補です。
2. **[minor] 型の違う参照の注釈の `E1003` が、参照先の型だけを報告する**: 再現しました（`m01`・`m02`・`p06`、監査の `x10`）。**直しました。** `binds_annotated_reference` は、注釈が引数の参照の型そのものと合うときだけ参照を束縛します。
   - 判定は新しい `Inference::unifies`（`src/polymorph.rs`）で、`unify` を試します。試しの間に解いた推論変数は `Inference::trial` に記録して、あとで戻します。`unify` が解を書く所は 1 か所だけです。推論変数の表を複製しないので、費用は試す型の大きさだけです。
   - 合わない注釈の引数は値 view のままなので、パターンは A32 の前と同じ診断を同じ位置で報告します。例: `expected ref [string], found string`、`expected ref string, found 'a`、`expected ref Main.Other, found Main.Big`、`expected ref mut string, found string`、`Mut` の位置の `ref Counter` に対する `expected ref Main.Counter, found Main.Counter`。
   - 受理するプログラムは変わりません。監査の 68 個（native の `-O0`・`-O3`）を修正前の結果と比べると、違うのは `x10` の文言だけで、base と同じ文言に戻りました。レビューの 146 個も base と比べ（`-O0`）、規約を持つ引数への型の違う注釈は、どれも base と同じ文言と位置になりました（`m01`・`m02`・`p06` を含む）。
   - `ref (i64 -> i64)` と注釈した関数の引数（`d07`・`x19c`・`x20`）は、注釈が合っているので参照を束縛します。`f n` は `E1005` のままで、明示の `ref (i64 -> i64)` の署名で `f n` と書いたときと同じです。その help は、参照先が関数のとき `a value of type ref (i64 -> i64) is a reference to a function, not a function; dereference it first, as in `(deref f) x`` にしました（`call_signature`）。`(deref f) n` は通ります。
   - 回帰テストは `mistyped_reference_annotations_keep_their_diagnostics` です。修正前のコードでは、最初の形が `expected [string], found string` になって失敗することを確かめました。
3. **[minor] 状態が `todo` のまま**: 見出しの表を `done` にしました。
4. **[minor] `Array`／`List` の読み取り callback へ直接書く注釈付きラムダに、テストも文書も無い**: 再現しました。(a) `60`、(b) `6`、(c) `60`、(d) `E1013` で、base はどれも `E1003` です。次のテストと文書を足しました。
   - テスト `annotated_reference_read_callbacks`: `Array` と `List` の `map`（直接とパイプライン）・`mapi`・`iter`・`iteri`・`fold`（直接とパイプライン）・`fold_back` を確かめます。本体では `let view: ref string = text` が通ることも確かめます。参照をブロックの外へ返すと `E1013`（束縛した形も `E1013`）、ブロックの中なら受理です。
   - suite `conventions` の `annotated_read_callbacks`: native と WASM の `-O0`・`-O3` で、入力は `-3`・`0`・`1` です。期待値は手で数えた式で書きました。
   - 文書: `functions.md` の読み取り callback の段落、`lambda-expressions.md`、`array.md`、`list.md`、`docs/language.md`。
5. **[minor] `x16c` と `x18` の回帰テストが値を確かめていない**: suite `conventions` に `annotated_reference_kept` を足しました。次の値を手で数えた式で確かめます。
   - `x16c` の項: `inspect` が `3`、ラムダが `"hello"` を `ref ref string` を通して読んで `50`。
   - `x18` の項: `3 + 50 · 1000 + (500 + n) · 10^6`。入力 `1` では監査の基準と同じ `501050003` です。
   - `apply_ref` の `(deref f) ((deref f) value)`。
6. **[minor] `binds_annotated_reference` の doc comment に `Partial` が無い**: 書き直しました。2 の試しの条件も書き足しました。

確認したこと:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 44 passed（42 件に新規の 2 件）。
- `cargo nextest run --locked`: 2987 tests run: 2987 passed, 4 skipped（レビュー前は 2985 件）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 成功（49 cases、native と WASM の `-O0`・`-O3`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs functions.md lambda-expressions.md array.md list.md`（`_tsuzuri/language-reference/` の下）: 成功（4 pages, 96 links, 88 checked examples, 176 native runs）。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。

変更したファイル: `src/polymorph.rs`、`src/closures.rs`、`src/check.rs`、`tests/parameter_conventions.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`、`functions.md`、`lambda-expressions.md`、`array.md`、`list.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

### レビューの指摘への対応（第2回）

指摘ごとに、第1回の後の dev binary（`7c8d8811`。比較用に `target/scratch/a32/review2/tsuzuri-prev` へ作り直しました）と base の binary（`b3dd7f1c`）で再現してから対応しました。再現と比較に使ったプログラムと script は `target/scratch/review-a32-ce/r2_*` と `target/scratch/a32/review2/`（`compare.sh`・`compare.txt`）にあります。

1. **[major] 第1回の指摘 1（`Array.write` を通して解放済みのメモリを読む形）がまだ受理され、関数型の読み取り引数の綴り（`r2_ag02`）は落ちる**: 再現しました。`h22` は `-O0` で `5004`、`-O3` で `4` です。`r2_ag02`（`\(slot: ref mut [(unit -> i64)]) (f: ref (i64 -> i64)) -> Array.write 0 (\() -> (deref f) 1000) slot`）は `-O0` で SIGSEGV、`-O3` で SIGTRAP です。注釈の無い `r2_ag03` と明示の `ref` の署名の `r2_ag04` は base でも同じく落ちます。**直しました。** 第1回では A31 の範囲として受理したままにしましたが、決定事項の「安全でないプログラムは受理しない」に従い、A32 が新しく受理する綴りをこの穴へ加えない保守的な規則を足しました。
   - 規則: 参照の注釈で参照を束縛した本体は、その最初の引数について、A32 の前にパターンが報告した `E1003` を `Checker::annotated_reference` に残します（`binds_annotated_reference`。位置は注釈付きのパターン、文言は `expected <注釈>, found <参照先の型>`）。`check_modules_collect` が関数ごとにそれを集めます。`closures::lower` の後、所有権検査の直前に、特殊化した module で A28 の `untracked_loan_store` が真なら（`ownership::untracked_loan_store_in`）、集めた `E1003` を報告して止めます。help は理由を示します（借用を持ちうる値を `Array.write`・`SyncMap.__replace`、またはそれを呼ぶ std の関数で格納する所がプログラムにあること）。
   - 健全性の議論: A32 が新しく受理するプログラムには、どれも base が同じ型と IR で受理する綴り（束縛したラムダ、明示の `ref` の署名）があります。追跡しない書き込みの無いプログラムでは、A28 の議論（`lending_bodies` の doc comment）のとおり、どの値の借用も完全なので、注釈した引数もその綴りと同じく検査されます。追跡しない書き込みのあるプログラムでは、プログラムは A32 の前と同じ `E1003` で止まります。base が受理するプログラムは変わりません。印を残すのは、A32 の前にパターンが `E1003` で失敗した引数だけだからです（`binds_annotated_reference` の条件）。
   - 費用: 印の無いプログラム（今まで通ったプログラムのすべて）では何も計算しません。印があるときだけ、特殊化した関数の本体を 1 回走査します。生成コードは変わりません（拒否するだけです）。
   - 変わった結果: review の 376 個（`review-a32-ce` と `a32/review1`）の `-O0` を第1回の後と比べました。`h09`・`h10`・`h21`・`h22`・`h22w`・`src_side`・`r2_ag01`・`r2_ag02` の 8 個が、受理（`h22` は `5004`、`r2_ag02` は SIGSEGV）から、base と同じ位置と文言の `E1003` に戻りました。`r2_ag01` は格納した閉包が何も捕捉しない安全なプログラムですが、base も拒否していたので保守的に拒否します。help だけ変わった 6 個（`d07`・`review1/d07`・`review1/d07d`・`r2_ae01`〜`r2_ae03`）は下の 4 です。残りの 362 個は同じです。監査の 68 個（native の `-O0`・`-O3`、`review1/rerun.sh`）は第1回の後とすべて同じです。
   - A31 との関係: A31 のブランチの先端（`88c7ba64`）に、この規則を足す前の A32 の `src/` の差分を重ねた binary を `target/scratch/a32/combo/` で作りました。`h19`〜`h22`・`alias`・`src_side`・`src_side_bound`・`r2_ag02`〜`r2_ag04` は、どれも呼び出し側の `E1013`（`assignment lets a reference outlive its owner`）になりました（`r2_ag01` は受理）。A31 はパターンで束縛した別名を通る書き込みもたどるので、A31 が入ればこの規則は要りません。A31 は `untracked_loan_store` を消すので、A31 と合わせるときは、この規則（`untracked_loan_store_in`、`Checker::annotated_reference` とそれを集める処理、help、文書の段落）も消します。回帰テスト `annotated_reference_parameters_wait_for_tracked_in_place_writes` の期待値は、A31 の診断（呼び出し側の `E1013`）へ書き換えます。この申し送りは `docs/language.md` にも書きました。
   - 直さないもの: `def put :: mut [(unit -> i64)] -> [i64] -> unit = \slot (xs: ref [i64]) -> …`（具体型の読み取り引数への注釈）は base も受理し（`5004`・`4`）、A32 は触れていません。具体型の読み取り引数は base でも read value parameter ではないからです。注釈の無い形・明示の `ref` の署名と同じく A31 の範囲です（`target/scratch/a32/review2/g_src_def`。A31 では `E1013`）。
   - 回帰テスト `annotated_reference_parameters_wait_for_tracked_in_place_writes`（`tests/parameter_conventions.rs`）は、次の形のコード・文言・help・位置を確かめます: `def` の形（`h22`）、ラムダの形（`h21`）、源の側だけを注釈したラムダ（`src_side`）、関数型の読み取り引数（`r2_ag02`）、std の `Array.set` を別の関数で呼ぶ形。借用を持てない値の書き込み（`[i64]` への `Array.write`、`Array.set 0 n`）のあるプログラムでは注釈が参照を束縛すること、追跡しない書き込みがあっても注釈の無い引数は受理のままであることも確かめます。修正前のコード（`7c8d8811`）では、最初の形が受理されて失敗することを確かめました。
2. **[minor] 参照の参照と型適用の除外が利用者の文書に無い**: 再現しました。`r2_ah01` は base も branch も `E1003 expected ref string, found string`、束縛した `r2_ah02` は `30` です。`r2_u01` は両方とも `E1015`、注釈の無い `r2_u02` は `3` です。どちらも「着手時の再検討」と実装で残した形（`binds_annotated_reference` の `Type::Reference`・`Type::Application` の除外）なので、挙動は変えずに文書へ書きました（`lambda-expressions.md` の例外の箇条と代わりの書き方、`functions.md`、`array.md`、`list.md`、`docs/language.md` に型適用を追加）。`List.map` の同じ形も `E1003` であることを確かめました（`review2/l1`）。テスト `documented_exceptions_of_annotated_reference_parameters` が、文書に書いた診断と代わりの書き方（`\(r: ref string)`、束縛した形、`\(value: 'container<i64>)`、`\value`）を確かめます。
3. **[minor] `def` の `\mut (v: ref 'a)` が `E1003`**: 再現しました（base も同じです）。不具合ではないので、文書を直しました。`\mut (pattern)` の `mut` は隠れた引数 `$lambda…` を可変にし、`def` は可変束縛引数を値渡しにします（`check_modules_collect` の `!parameter.mutable`）。そのため引数の型は `ref 'a` ではなく `'a` で、注釈は本当に合いません。具体型でも同じで、`def size :: string -> i64 = \mut (t: ref string) -> …` は base も `E1003` です。`\mut (t: string)` は値渡しなので実引数を移動します（`review2/m3a`・`m3b`）。ラムダの位置と `mut T` の引数は、引数の型が期待型や規約で決まり `mut` で変わらないので、参照の注釈が合います（`r2_ad01` は `30`、`r2_ad03` は `2`）。`\(v: ref 'a)` と同じに扱うと、今受理している `\mut (pattern)` の引数の渡し方まで変わるので、変えませんでした。`lambda-expressions.md`・`functions.md`・`docs/language.md` に、`\mut (pattern)` の `mut` が分解前の引数に付くことと、その帰結を書きました。テストは `documented_exceptions_of_annotated_reference_parameters` の後半です。
4. **[minor] `E1005` の help が仮の名前で `(deref f) x` と示す**: 再現しました（`r2_ae02` の `3 |> r` でも `(deref f) x` でした）。**直しました。** help を ``a value of type … is a reference to a function, not a function; dereference it with `deref` before calling it`` にし、呼び出す名前と引数を書かないようにしました。関数の参照を呼ぶ形はどれも参照外しが要るので、base が拒否していた `r2_ae02`・`r2_ae03` にも同じ help を出します。base の help（`remove the argument(s) or call a function`）は関数の参照には当てはまらないためです。テスト `mistyped_reference_annotations_keep_their_diagnostics` の期待値をこの文言へ変え、パイプライン `3 |> r` の形を足しました。修正前のコードでは失敗することを確かめました。

確認したこと:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 46 passed（44 件に新規の 2 件）。新規の 2 件と、期待値を変えた `mistyped_reference_annotations_keep_their_diagnostics` は、修正前のコード（`7c8d8811` の `src/` に新しいテストを置いたもの）で、指摘 1 と 4 の形が失敗することを確かめました（`documented_exceptions_of_annotated_reference_parameters` は挙動を変えない、文書に書いた形の固定です。そこで使った指摘の形 `r2_ah*`・`r2_u*`・`r2_ad*` は、`compare.txt` で第1回の後の binary と同じ結果です）。
- `cargo nextest run --locked`: 2989 tests run: 2989 passed, 4 skipped（第1回の後は 2987 件）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 成功（49 cases、native と WASM の `-O0`・`-O3`）。`node tests/features.mjs target/debug/tsuzuri`（全 suite）: `features: 25179 cases passed`。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs lambda-expressions.md functions.md array.md list.md lifetimes.md`（`_tsuzuri/language-reference/` の下）: 成功（5 pages, 116 links, 94 checked examples, 188 native runs）。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。
- 診断の kind は増やしていません（規則は既存の `type_mismatch` の `E1003` を、A32 の前と同じ位置と文言で出します）。`src/runtime/` も std も変えていないので、`gen-docs`・`check-cli-docs`・`check-runtime-includes` は要りません。

変更したファイル: `src/closures.rs`、`src/check.rs`、`src/ownership.rs`、`src/polymorph.rs`、`tests/parameter_conventions.rs`、`lambda-expressions.md`、`functions.md`、`array.md`、`list.md`、`lifetimes.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

### A31 との統合（2026-11-11）

A31 のブランチ（`fix/a31-in-place-writes-keep-borrows` の `58cc81a6`、第3回のレビューの対応まで）をこのブランチへ merge し（`6a11e320`）、第2回のレビューで足した一時的な規則を消しました（`8cae16e1`）。オーケストレーターの決定（A31 を先に merge し、A32 を A31 の上に重ねる）に従った手順です。

- merge の衝突: `_features/README.md` の A31 と A32 の行は、どちらも `_completed/` へリンクする行（done の行）を残しました。`lifetimes.md` の「現在の制限」は、A31 の「その場の書き込み」の説明を残し、既知の不具合の段落（A32 の規則の一文を含む）を消しました。`src/ownership.rs` は、A31 が消した `untracked_writer`・`untracked_loan_store` と一緒に、その上に作った `untracked_loan_store_in` も消しました。merge commit がビルドできるように、`check_modules_collect` の規則の呼び出しも merge で消しました。
- 消したもの: `Checker::annotated_reference` とそれを集める処理（`check_modules_collect`）、`binds_annotated_reference` が残していた A32 の前の `E1003` と help（`UNTRACKED_STORE_HELP`）、そのために `parameter_annotations` が返していたパターンの位置、`polymorph::mismatch` の公開。`binds_annotated_reference` と `parameter_annotations` は第1回の後（`7c8d8811`）の形に戻りました。`E1005` の help（第2回の 4）はそのままです。文書では、`docs/language.md`・`docs/architecture.md` の規則と A31 への申し送り、`functions.md`・`array.md`・`list.md` の「書き込む所があるプログラムでは注釈は `E1003`」を消し、`lambda-expressions.md` の段落を、注釈した引数も A31 の書き込みの事実で検査され、借用元より長く書き込み先を使うと `E1013` になる、という今の挙動に書き換えました。
- 結果: 注釈した綴りは A32 のとおり参照を束縛し、安全でない書き込みは A31 の書き込みの事実が拒否します。第1回・第2回のレビューの種 `h19`〜`h22`・`r2_ag02`〜`r2_ag04`・`review1/alias`・`src_side`・`src_side_bound` は、native の `-O0`・`-O3` でどれも `E1013 assignment_extends_borrow`（`assignment lets a reference outlive its owner`）で、位置は `local`（`k`）を持つブロックの先頭です。ブロックが持ち主を落とす所で、持ち主の借用を持つ局所値（`arr`）が残っていれば報告する A31 の規則（`src/ownership.rs` の `AssignmentExtendsBorrow`）のとおりです。注釈の無い双子（`h19`、`r2_ag03`、明示の `ref` の署名の `r2_ag04`、`if` の枝を `put` にした形）も同じコード・kind・位置です。`r2_ag01`（格納する閉包が何も捕捉しない）は受理され、`-O0`・`-O3` とも `1` です（`put` が書き込むのは `\() -> 1` なので、手で数えて 1）。
- A31 の穴の探索: A32 の綴りを通る A31 の穴が無いか、次の形も試しました（`target/scratch/a32/stack/p/`）。std の `Array.set`・`Array.fill` で書き込む形、`match` の枝、ジェネリックなレコードのフィールド、`Vec` の要素、ジェネリックな `def`（`ref 'a` と `ref (ref 'a -> i64)` の注釈）、読み取り callback（`Array.map`・`Array.iteri`・`Array.fold`・`List.fold_back`）からロックを通して書き込む形、`Mutex.with_lock` の注釈した callback、持ち主をムーブする形です。借用元より書き込み先が長生きする形はどれも `E1013 assignment_extends_borrow`、書き込み先が生きている間に持ち主をムーブする形は `E1014 borrow_conflict` で、注釈の無い双子と同じでした（注釈した部分適用は `E1005`、`Vec.push` で書き込む形は region を求める `E1013` で、どちらも A31 の前からの規則です）。A31 の穴は見つからなかったので、`src/ownership.rs` は変えていません。
- テスト: 第2回の回帰テスト `annotated_reference_parameters_wait_for_tracked_in_place_writes` を `annotated_reference_parameters_keep_their_in_place_writes_tracked` に置き換えました（期待値の変更は、第2回の対応に書いた申し送りと、オーケストレーターの決定のとおりです）。種と上の形の 28 個（注釈した綴りと双子）が `E1013 assignment_extends_borrow` でブロックの位置（行と列）、ムーブの 2 個が `E1014 borrow_conflict` でムーブの位置になること、`r2_ag01` と安全な対照（`i64` の書き込み、`n` だけを捕捉した関数の `Array.set`、書き込み先より前に宣言した持ち主への書き込み 3 形）が受理されることを確かめます。conventions suite には `annotated_reference_stores`（n = -3, 0, 1, 5）を足し、安全な対照の値を、手で数えて JS の BigInt で書いた値と比べます（native と WASM の `-O0`・`-O3`）。
- 第2回の規則では、`keep slots (\() -> n)` と注釈した `chosen` を持つプログラムも `E1003` にしていましたが、格納する関数は `n` の値しか捕捉しない安全なプログラムなので、今は受理します（`pick n` は `n > 0` なら `5 + n`、それ以外は `50 + n`）。
- 変わった結果: review の 376 個（`review-a32-ce` と `a32/review1`）と上の探索の 30 個を、native の `-O0` で、A31 の先端の binary（`bd0242a1` の `src/`。`58cc81a6` は記録だけの commit）と merge した dev binary で比べました（`target/scratch/a32/stack/compare.sh`・`compare.txt`）。192 個は同じで、変わった 214 個は、A31 だけでは A32 の前の `E1003`（`expected ref …, found …`）か `E1005` の古い help だったものです。その 214 個を第1回の後の A32 の binary（`7c8d8811`。A31 も第2回の規則も無い）とも比べると（`compare3.sh`・`compare3.txt`）、188 個は同じ（A32 の受理と診断がそのまま残る。`r2_ag01` の `1` を含む）で、違う 26 個は、`E1005` の help の文言（第2回の 4。`d07` の 2 個・`d07d`・`r2_ae01`〜`r2_ae03`）の 6 個と、第1回の後は受理されて値を出すか落ちた安全でないプログラム 20 個（`h09`・`h10`・`h21`・`h22`・`h22w`・`src_side`・`r2_ag02` と探索の 13 個）が A31 の `E1013`・`E1014` になったものだけです。

確認したこと:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo nextest run --locked --test parameter_conventions annotated_reference_parameters_keep_their`: 1 passed。
- `cargo nextest run --locked`: 3027 tests run: 3027 passed (9 slow), 4 skipped（A31 のテストが増え、第2回の回帰テストを置き換えたので、第2回の後の 2989 件から変わりました）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 53 cases（第2回の後の 49 件に `annotated_reference_stores` の 4 件）、`in_place_writes`: 44 cases、`mutable_slices`: 49 cases。どれも native と WASM の `-O0`・`-O3` で成功。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs values-and-functions/lambda-expressions.md values-and-functions/functions.md built-in-types-and-modules/array.md built-in-types-and-modules/list.md ownership-and-memory/lifetimes.md`（`_tsuzuri/language-reference/` で実行）: 成功（5 pages, 113 links, 94 checked examples, 188 native runs (O0/O3)）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。
- 診断の kind は増やしていません。`src/runtime/` と std は変えていません。

変更したファイル（merge の後）: `src/check.rs`、`src/closures.rs`、`src/polymorph.rs`、`tests/parameter_conventions.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`、`lambda-expressions.md`、`functions.md`、`array.md`、`list.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

#### 2 度目の merge（2026-11-12、A31 の第4回〜第7回のレビューの対応）

A31 のブランチ（`fix/a31-in-place-writes-keep-borrows` の `ec955303`、第7回のレビューの対応の記録まで）を、もう一度このブランチへ merge しました（`717eaa28`）。前の merge との共通の祖先は `58cc81a6`（A31 の第3回まで）で、その後の A31 の変更は第4回〜第7回のレビューの対応です。作業の記録とプログラムは `target/scratch/a32/merge2/` にあります。

- merge の衝突: `_features/README.md` の A31 と A32 の行だけでした。A31 の行は A31 のブランチの行（第1回〜第7回の対応を述べる行）、A32 の行はこのブランチの行で、どちらも `_completed/` へリンクします。`src/check.rs`（A31 が `Type::stores_some` を移し、`Type::drops_some` を足した）、`src/ownership.rs`、`src/ownership_control.rs`、`docs/language.md`、`docs/architecture.md`、`pattern-matching.md`、`tests/features.mjs` は自動で merge され、merge commit はそのままビルドできました。`pattern-matching.md` には、A32 の段落（引数のパターンの as の二重消費）と A31 の段落（借用先からの分解）の両方が残り、どちらも今の挙動のとおりです。
- A32 のテストの確認: merge の直後に `tests/parameter_conventions.rs` の 48 件がすべて成功しました。`annotated_reference_parameters_keep_their_in_place_writes_tracked` の種と探索の 28 個は今も `E1013 assignment_extends_borrow`（借用元を持つブロックの行と列）、ムーブの 2 個は `E1014 borrow_conflict`（ムーブの位置）で、安全な対照 6 個は受理されます。conventions suite の A32 の値（`annotated_reference_stores` など）も変わりません。
- A31 の後の規則と注釈した綴り: 第4回〜第7回で A31 が足した規則（書き込める仮引数の先の借用を残した後の書き込み、関数値の仮引数へ渡した借用の上書き、共有参照の仮引数の先を照合したパターンの変数の借用、型注釈・パターンを書いたラムダの仮引数や仮引数のフィールドの関数の呼び出し、局所の `Rc`・`Arc` のブロックの共有、破棄の順序）が、注釈で参照を束縛した引数を、`ref` の型の引数を持つ名前付き関数（A31 自身の綴り）と同じに扱うかを確かめました。A31 の第6回・第7回のプログラム（`m01`〜`m03`、`wp01`・`wp02`、`wq01`、`y01`・`y02`・`y06`、`vw01`〜`vw04`、`v04`〜`v07`、`n18`〜`n23` など）から 28 個の形を作り、それぞれを 6 つの綴りで書きました: `ref` の署名の名前付き関数、規約を持つ `def` の注釈の無いラムダ、同じ `def` の注釈したラムダ、注釈したラムダを `else` の枝に置いた `if`、その双子として両方の枝を `def` にした `if`、`ref` の署名の `def` の注釈したラムダです（168 個、`merge2/gen.mjs` が `merge2/p/` に書きます）。merge した binary では、注釈した `def` の結果は名前付き関数の結果と、注釈した `if` の結果は双子の結果と、すべて同じコードと kind（拒否）か同じ値（受理。`-O0`・`-O3`）でした。前の A32 の binary（`bb9b40d1`。A31 の第3回まで）からの変化は、名前付き関数と注釈の無い綴りにも同じく起きた A31 の変化だけです（`m01`・`m03` が受理から `E1014`、`y02` が受理から `E1013`、`wp01`・`wq01`・`m04`・`n18`・`n20`・`n22`・`vw02`・`vw05`・`v05`・`v07` が拒否から受理）。A31 の新しい規則が、A32 が受理し注釈の無い双子も受理する安全なプログラムを拒否する例は見つからなかったので、A31 のコードも A32 のコードも変えていません。
  - `if` の綴りで、関数値の、読み取りの関数型の仮引数へラムダを直接渡す呼び出し（`chosen (ref mut v0) (\g -> g ())`）は、注釈の有無によらず `E1013 borrow_of_temporary` です。A31 の前からの、関数値の呼び出しの読み取りの引数の規則で、注釈の無い双子も同じなので変えていません。
- 比べたプログラム: (1) A32 のレビューと探索のプログラム 1658 個（`target/scratch/a32` の下と `review-a32-ce`・`review-a32-ce-r3`）の `check --json` は、前の A32 の binary と merge した binary で 1 つも変わりませんでした（`merge2/a32corpus-*.json`）。A31 の先端の binary（A31 の worktree の `target/debug/tsuzuri`。`src/` を最後に変えた commit `aaf3fab2` の後にビルドされたもの）と比べて違う 731 個は、A31 だけでは A32 の前の `E1003` などで拒否されるもの（504 個が受理へ、227 個が別の診断へ）で、受理から拒否に変わったものはありません。受理される注釈したプログラム 216 個（生成したものを除く）の値は、前の binary と merge した binary で `-O0`・`-O3` とも同じでした（`merge2/a32run-*.json`）。(2) A31 の第4回〜第7回のレビューと対応のプログラム 1651 個（性能のプログラムを除く。edition 2027 と 2026 で 3302 回の検査）を A31 の先端の binary と merge した binary で比べ、違う 13 個は、A31 のレビュー役が写した A32 の種（`h21`・`h22`・`h22w`・`r2_ag02`・`src_side`）が A32 の前の `E1003` から A31 の `E1013 assignment_extends_borrow` になったものだけでした（`merge2/a31corpus-a31-vs-merged.txt`）。
- テスト: 回帰テスト `annotated_reference_parameters_follow_the_later_rules_of_in_place_writes`（`tests/parameter_conventions.rs`）を足しました。安全でない 9 形（`m01`・`m03` の `E1014 borrow_conflict`、`y02`・`n19`・`n21`・`n23` の `E1013 store_local_borrow_through_reference`、`vw03`・`vw04`・`v06` の `E1013 assignment_extends_borrow`）では、注釈した `def` の最初の誤りが、名前付き関数の双子と同じコード・kind で、誤りの範囲が同じ式であることを確かめます。安全な 12 形では、双子と注釈した `def` と（関数値の呼び出しが綴りによらず受理される 9 形は）注釈した `else` の枝が受理されることを確かめます。前の A32 の binary では、`m01` が受理され `wp01` が拒否されるので、このテストは失敗します（`merge2/p-prev.json`）。conventions suite には `annotated_reference_later_rules`（n = -3, 0, 1, 5。書き込める引数の別のフィールドへの書き込み、関数を呼ぶだけの関数値へ渡した後の `Vec.push_mut`、タプルを照合したビューの書き込み、タプルのパターンで取り出した関数）を足し、手で数えた値を JS の BigInt の式で書きました。
- 文書: `lambda-expressions.md` の注釈した引数の所有権検査の段落、`docs/language.md`、`docs/architecture.md` に、A31 の後の規則（書き込める引数の先の借用を残した後の書き込み、照合した変数の借用、パターンやフィールドで取り出した関数）も `ref` の型の引数を持つ名前付き関数と同じであることを足しました。A31 が第4回〜第7回で書いた `lifetimes.md`（「型注釈やパターンを書いたラムダの仮引数」の段落を含む）・`drop.md`・`rc.md`・`pattern-matching.md` は、注釈した綴りにもそのまま当てはまるので変えていません。

確認したこと:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 49 passed（48 件に新規の 1 件）。
- `cargo nextest run --locked --no-fail-fast`: 3049 tests run: 3049 passed (12 slow), 4 skipped（1139 秒、`merge2/t1.log`）。A31 の第7回の 3036 件に、A32 の 12 件と新規の 1 件を足した数です。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 60 cases（56 件に `annotated_reference_later_rules` の 4 件）、`in_place_writes`: 56 cases、`mutable_slices`: 49 cases。どれも native と WASM の `-O0`・`-O3` で成功（`merge2/features.log`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`: `lambda-expressions.md` は 1 pages, 22 links, 13 checked examples, 26 native runs (O0/O3)、merge で入った A31 の `pattern-matching.md`・`lifetimes.md`・`drop.md`・`rc.md` は 4 pages, 69 links, 26 checked examples, 52 native runs (O0/O3)。どちらも成功（`merge2/check-docs.log`）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。
- 診断の kind は増やしていません。`src/` は merge のほかに変えていません。

変更したファイル（2 度目の merge の後）: `tests/parameter_conventions.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`、`lambda-expressions.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

#### 3 度目の merge（2026-11-13、A31 の第8回・第9回のレビューの対応）

A31 のブランチ（`fix/a31-in-place-writes-keep-borrows` の `3a9e4733`、第9回のレビューの対応の記録まで）を、もう一度このブランチへ merge しました（`99ba0fe6`）。前の merge との共通の祖先は `ec955303`（A31 の第7回まで）で、その後の A31 の変更は第8回・第9回のレビューの対応（`src/ownership.rs`・`src/ownership_control.rs`、`tests/rc.rs`、`in_place_writes` の fixture、文書）です。merge の前のこのブランチの先端は `e6ceff4d`（第4回のレビューの対応まで）です。作業の記録とプログラムは `target/scratch/a32/merge3/` にあります。比べた binary は、merge の前（`merge3/tsuzuri-prev`。`src/` を最後に変えた `e1070a3d` の後にビルドしたもの。`e6ceff4d` は記録と文書だけ）、A31 の先端（`merge3/tsuzuri-a31`。A31 の worktree の `target/debug/tsuzuri` で、`src/` を最後に変えた `a5050194` の後にビルドされ、それより新しいソースが無いことを確かめたもの）、merge した dev binary（`merge3/tsuzuri-merged`）です。

- merge の衝突: `_features/README.md` の A31 と A32 の行だけでした。A31 の行は A31 のブランチの行（第1回〜第9回の対応を述べる行）、A32 の行はこのブランチの行で、どちらも `_completed/` へリンクします。A33 の行はどちらのブランチにもありません（A31 は第8回・第9回でも別のチケットを作っていません）。`src/ownership.rs`、`src/ownership_control.rs`、`tests/features.mjs`、`docs/language.md`、`docs/architecture.md` と A31 の 7 ページは自動で merge され、merge commit はそのままビルドできました。
- A32 のテストの確認: merge の直後に `tests/parameter_conventions.rs` の 52 件と、領域のテスト（`rc` 68 件、`mutable_slices` 19 件、`currying` 16 件、`borrowed_records` 21 件、`user_drop` 27 件）がすべて成功しました（`merge3/area.log`）。A32 の拒否のテスト（第1回・第2回の種、2 度目の merge の 9 形、第3回・第4回の回帰テスト）は同じコードと kind のままで、安全な対照は受理されます。
- 比べたプログラム: (1) A32 のレビューと探索のプログラム 2055 個（`target/scratch/a32` の下（`merge3` を除く）と `review-a32-ce`・`review-a32-ce-r3`・`review-a32-ce-r4`）の `check --json` は、merge の前の binary と merge した binary で 1 つも変わりませんでした（`merge3/a32corpus-*.json`）。そのうち受理される 923 個（2 度目の merge で生成したプログラムを除く）の `-O0`・`-O3` の出力も、2 つの binary ですべて同じでした（`merge3/a32run-*.json`、`values-diff.txt`。`-O0` と `-O3` が違う 5 個はどちらの binary でも同じで、`-O0` の深い再帰のスタック溢れ 1 個と、`-O3` のビルドが探索の 180 秒の上限を超える 4k 個のリテラルのプログラム 4 個です）。(2) A31 の第8回・第9回のレビューと対応のプログラム 378 個（`review-a31-ce-r8`・`review-a31-conf-r8`・`review-a31-ce-r9`・`review-a31-conf-r9`・`a31/r8/adv`・`a31/r9/adv`。性能のプログラムを除く）を edition 2027 と 2026 で、A31 の先端の binary と merge した binary で比べ、756 回の検査のどれも変わりませんでした（`merge3/corpus-diff.txt`）。
- A31 の第8回・第9回の規則と注釈した綴り: 引数を持つ 16 個の形（`merge3/gen.mjs`、106 個）を、7 つの綴りで書きました。`ref` の署名の名前付き関数（fn）、同じ規約の署名の名前付き関数（fnc。読み取りの引数へ一時値を貸す形は `ref` の署名では書けないので、規約の綴りの双子はこちらです）、規約を持つ `def` の注釈の無いラムダ（conv）、同じ `def` の注釈したラムダ（annot）、注釈したラムダを `else` の枝に置いた `if`（ifa）と両方の枝を `def` にした双子（ifc）、`ref` の署名の `def` の注釈したラムダ（alias）です。形は、ロックの中身の借用を別の `Mutex` へ残す形と長さのコピー（`lk01`・`lk01c`。第8回の `m06`）、関数値の仮引数へ渡した共有の借用の後の呼び出し元の読み出しと書き込み（`sh01`・`sh02`。`h07`・`b01`）、`own` の仮引数が `ref` の引数の借用を持つ形（`pd01`。第8回の `p01` の参照版）、読み取りの引数へ貸した一時値の順序（`tp01`・`tp01c`。第9回の `pa05`・`F2c`）、呼び出し元の `Mutex` のセルへ残した借用の、局所値と一時値の順序（`cp01`〜`cp03`。第9回の `rg02`・`rg07` の参照版）、値の部分の借用を同じ値の `Mutex` へ残す形（`sp01`・`sp02` と、その `ref mut` 版 `sm01`・`sm02`。第9回の `a21`・`a21u`）、`ref Maybe<Pair>` の中身（`mp01`・`mp02`）です。merge した binary では、annot は conv・fnc と、ifa は ifc と、alias は fn と、すべて同じコードと kind（拒否）か同じ出力（受理。`-O0`・`-O3`）でした（`merge3/table.mjs`）。受理される出力は手で数えた値です（`lk01c` は `9`、`sh01` は `12`、`pd01` は `drop 1`・`drop 9`・`0`、`tp01c` は `drop 9`・`0`、`cp01` は `drop 0`・`drop 9`・`0`、`sp01`・`sm01` は `drop 8`・`0`）。merge の前の binary からの変化は、名前付き関数にも同じく起きた A31 の変化だけです（`lk01` が受理（`1`）から `E1013 mutex_result_retains_borrow`、`tp01`・`cp03` が受理（`drop 0`）から `E1013 assignment_extends_borrow`、`sp02` が受理（`drop -1`）から `E1013 self_referential_store`、`sh01` が `E1014` から受理）。A31 の先端の binary では annot と ifa が A32 の前の `E1003` なので、これらの形は A32 の綴りを通っています。
  - `if` の綴りで関数値へ一時値やラムダの引数を貸す形（`sh01`・`sh02`・`tp01`・`tp01c`・`cp03`）は、注釈の有無によらず ifa も ifc も `E1013 borrow_of_temporary` です。A31 の前からの、関数値の呼び出しの読み取りの引数の規則です。`mp01`・`mp02` はどの綴りでも `E1012`（照合した中身のレコードを参照から取り出せない）で、形として書けません。
  - 規約は `def` の仮引数の型にだけ付くので、署名の中の関数型の引数（`([string] -> unit)`）や `let` の型注釈は規約を持たず、`ref` の注釈は A32 の前と同じく `E1003` です（`merge3/h/cb1*`・`nm1`）。ロックのコールバックの型は `ref mut 'a -> 'b` なので、注釈したコールバックは alias と同じく A32 の前から受理される形です。第8回の呼び出し先の名前（`callee_subject`）は、`let` で束縛したラムダを `'leak'` と示します（merge の前は `'14'`）。
- A31 の穴（A32 の問題ではありません。A31 の第9回の後の対応で直り、4 度目の merge で確かめました）: この merge の時点では、`sm02`（`record Pair { s: string, h: Holder }` を `ref mut Pair` で受け取り、本文が `ref p.s` の借用を `p.h.cell` へ書き込む）が、A31 自身の綴り（`def target :: ref mut Pair -> i64`）を含むどの綴りでも受理され、`drop -1`（解放後の読み出し）を出しました（`ref mut` の変数を渡す形 `merge3/sm/var` と、別の関数へ中継する形 `sm/relay` も）。
- A31 の退行（A32 の問題ではありません。A31 の第9回の後の対応で直り、4 度目の merge で確かめました）: この merge の時点では、conventions suite の fixture の、A32 の前からある `callback_constructed_once`（`Array.map ({ calls = calls + 1; owned_inc }) [1, 2, 3]`）が、モジュールに `Drop` の instance と値を捕捉したラムダを入れた `Vec` があるため `E1013 assignment_extends_borrow` で拒否され（20 行の再現 `merge3/fx/min2`）、conventions suite と、すべての fixture の IR を調べる `effects_alloc_conservative`・`effects_trap` が失敗しました。fixture は変えていません。
- テスト: 回帰テスト `annotated_reference_parameters_follow_the_drop_order_and_lock_rules_of_in_place_writes`（`tests/parameter_conventions.rs`。`LaterRule` に、同じ規約の名前付き関数を作る `convention_twin` を足しました）を足しました。安全でない 6 形（`lk01` の `E1013 mutex_result_retains_borrow`、`sh02` の `E1014 borrow_conflict`、`tp01`・`cp02`・`cp03` の `E1013 assignment_extends_borrow`、`sp02` の `E1013 self_referential_store`）では、注釈した `def` の最初の誤りが、同じ規約の名前付き関数と同じコード・kind で同じ式を指すこと、`if` の綴りが同じく呼び出す 3 形（`lk01`・`cp02`・`sp02`）はそこでも同じコード・kind であることを確かめます。安全な 5 形（`lk01c`・`sh01`・`tp01c`・`cp01`・`sp01`）は、双子と注釈した `def`、`ref` の署名で書ける 4 形は `ref` の署名の名前付き関数、関数値へ一時値もラムダも貸さない 3 形は注釈した `else` の枝が受理されることを確かめます。merge の前の binary では `lk01` などの注釈した綴りが受理されるので、このテストは失敗します（`merge3/gen-prev.json`）。conventions suite には `annotated_reference_drop_rules`（n = -3, 0, 1, 5。関数値の仮引数へ渡した共有の借用の後の読み出し、値の部分の借用、一時値の順序、呼び出し元のセル。どの `drop` も何も出力せず、入れ子のロックも使いません）を足し、手で数えた値を JS の BigInt の式で書きました。単独のプログラムにした native の `-O0`・`-O3` は `3030305 1010104 2020104 2020204` で、JS の式と同じです。この merge の時点では上の退行のため、suite はその `callback_constructed_once` とその case だけを除いた写し（`merge3/root`。追跡しないファイル）で実行し、63 件が native と WASM の `-O0`・`-O3` で成功しました（実物の fixture では 4 度目の merge で確かめました）。
- 文書: `lambda-expressions.md` の注釈した引数の所有権検査の段落、`docs/language.md`、`docs/architecture.md` に、A31 の第8回・第9回の規則（ロックの中身の借用、関数値の仮引数へ渡す共有の借用、仮引数・一時値・値の部分の破棄の順序）も同じ規約の名前付き関数と同じであることを足しました。A31 が書いた `mutex.md`・`rwmutex.md`・`cond.md`・`syncmap.md`・`rc.md`・`drop.md`・`lifetimes.md` は、注釈した綴りにもそのまま当てはまるので変えていません。

確認したこと（ログは `target/scratch/a32/merge3/`）:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions annotated_reference_parameters_follow`: 2 passed（2 度目の merge のテストと新規のテスト）。
- `cargo nextest run --locked --no-fail-fast`: 3063 tests run: 3061 passed (12 slow), 2 failed, 4 skipped（1164 秒、`t1.log`）。3063 件は、第4回の後の 3052 件に A31 の第8回・第9回の 10 件と新規の 1 件を足した数です。失敗した 2 件は上の A31 の退行による `effects_alloc_conservative`・`effects_trap` で、どちらも conventions の fixture が IR を出せないことで止まります（4 度目の merge で成功に戻りました）。
- `node tests/features.mjs target/debug/tsuzuri <suite>`: `in_place_writes` 63 cases、`mutable_slices` 49 cases が native と WASM の `-O0`・`-O3` で成功。`conventions` は上の退行で fixture の検査が失敗し、写しの `merge3/root` では 63 cases が成功（`features.log`。4 度目の merge では実物の fixture で成功）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`lambda-expressions.md` と、merge で入った A31 の `cond.md`・`mutex.md`・`rc.md`・`rwmutex.md`・`syncmap.md`・`drop.md`・`lifetimes.md`）: 8 pages, 131 links, 38 checked examples, 76 native runs (O0/O3) で成功（`check-docs.log`）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。
- 診断の kind は増やしていません。`src/` は merge のほかに変えていません。

変更したファイル（3 度目の merge の後）: `tests/parameter_conventions.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`、`lambda-expressions.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

#### 4 度目の merge（2026-11-13、A31 の第9回の後・第10回・第11回のレビューの対応）

A31 のブランチ（`fix/a31-in-place-writes-keep-borrows` の `8890c1aa`、第11回のレビューの対応の記録まで）を、もう一度このブランチへ merge しました（`71445bc0`）。前の merge との共通の祖先は `3a9e4733`（A31 の第9回まで）で、その後の A31 の変更は、3 度目の merge で見つけた退行と穴の対応（第9回の後）と、第10回・第11回のレビューの対応です（`src/ownership.rs`・`src/ownership_control.rs`、`tests/rc.rs`、`in_place_writes` と conventions の fixture、`drop.md`・`lifetimes.md`、`docs/language.md`・`docs/architecture.md`）。merge の前のこのブランチの先端は `4cc20fea`（3 度目の merge の記録まで）です。作業の記録とプログラムは `target/scratch/a32/merge4/` にあります。比べた binary は、merge の前（`merge4/tsuzuri-prev`。3 度目の merge の dev binary で、`src/`・`std/` は `4cc20fea` と同じ）、A31 の先端（`merge4/tsuzuri-a31`。`8890c1aa` を `git archive` で scratch に展開してビルドしたもの）、merge した dev binary（`merge4/tsuzuri-merged`）です。

- merge の衝突: `_features/README.md` の A31 と A32 の行だけでした。A31 の行は A31 のブランチの行、A32 の行はこのブランチの行で、どちらも `_completed/` へリンクします。`src/ownership.rs`・`src/ownership_control.rs`・`tests/features.mjs`・`tests/fixtures/conventions/Main.tz`（A31 が `capturing_readers` を足しました）・`docs/language.md`・`docs/architecture.md` と A31 の 2 ページは自動で merge され、merge commit はそのままビルドできました。
- 3 度目の merge で見つけた A31 の退行と穴は、どちらも直っていました。(1) conventions suite は実物の fixture（`callback_constructed_once` を含む）で成功し、`effects_alloc_conservative`・`effects_trap` も成功します（下の確認）。(2) `sm02` は 7 つの綴り（fn・fnc・conv・annot・ifa・ifc・alias）のどれでも `E1013 self_referential_store`（`p0` を宣言した行）になりました。merge の前の binary ではどれも受理され `drop -1` でした。3 度目の merge の探索の 118 個（`merge3/gen-dirs.txt`）のうち、merge の前の binary と結果が変わったのはこの 7 個だけです（`merge4/gen-merged.json`）。`merge3/sm/var`・`sm/relay` も同じ `E1013` です。
- `ref mut` の自己の部分の探索: `merge4/gen.mjs` が、`sm02` とその安全な順序 `sm01`（`record Pair { h: Holder, s: string }`）を、4 つの形（直接の呼び出し、`ref mut` の変数 `let r = ref mut p0` を渡す呼び出し、名前付き関数 `inner` へ中継する形、注釈したラムダの `def inner` へ中継する形）と 7 つの綴りで書きます（56 個）。merge した binary では、annot は conv・fnc と、ifa は ifc と、alias は fn と同じで（`merge4/table.mjs`、`table.txt`）、安全でない 28 個はどれも `E1013 self_referential_store`、安全な 28 個はどれも `-O0`・`-O3` とも `drop 8` と `0` を出します（`"123456789"` を 4 回繰り返した文字列の最初の `"9"` の添字は 8 なので、手で数えて `8`。`run` の結果は 0）。merge の前の binary では安全でない 28 個がどれも受理されて `drop -1` を出し、A31 の先端の binary では annot・ifa と、注釈した `def inner` を持つ形のすべての綴りが、A32 の前の `E1003` です。
- テスト: `annotated_reference_parameters_follow_the_drop_order_and_lock_rules_of_in_place_writes` に、上の 4 形の安全でない版を拒否の case として足しました（3 度目の merge で予定したとおりです）。どれも、同じ規約の名前付き関数（`convention_twin`）が `E1013 self_referential_store` で、注釈した `def` が同じコード・kind で同じ式を指し、`if` の綴りも同じコード・kind になることを確かめます。拒否の case のうち `ref` の型で書ける形（`explicit` を持つもの）では、A31 自身の綴り（`ref` の型の名前付き関数、`twin`）も同じコード・kind で同じ式を指すことを確かめるようにしました（3 度目の merge の 5 形も、merge した binary では fn と fnc が同じ結果なので通ります）。安全な版の 4 形は、双子・`ref` の署名の名前付き関数・注釈した `def`・注釈した `else` の枝が受理されることを確かめます。merge の前の binary では安全でない版がどの綴りでも受理されるので、このテストは失敗します（`merge4/sm-prev.json`）。期待値を弱めたテストはありません。
- 比べたプログラム: (1) A32 のレビューと探索のプログラム 2055 個（3 度目の merge と同じ一覧）の `check --json` は、merge の前の binary と merge した binary で 1 つも変わりませんでした（`merge4/a32corpus-diff.txt`）。そのうち受理される 923 個の `-O0`・`-O3` の出力も、2 つの binary ですべて同じでした（`merge4/a32run-*.json`、`values-diff.txt`。`-O0` と `-O3` が違う 5 個は、3 度目の merge と同じく、`-O0` の深い再帰のスタック溢れ 1 個と、`-O3` のビルドが探索の 180 秒の上限を超える 4k 個のリテラルのプログラム 4 個で、どちらの binary でも同じです）。(2) A31 の第9回の後・第10回・第11回のレビューと対応のプログラム 717 個（`review-a31-ce-r10`・`review-a31-ce-r11`・`review-a31-conf-r10`・`review-a31-conf-r11`・`a31/r10`・`a31/r11`）を edition 2027 と 2026 で、A31 の先端の binary と merge した binary で比べ、1434 回の検査で違う 12 個は、A31 のプログラムの中の A32 の綴り（`a31/r10/sm/g` の `sp01`・`sp02`・`sm01`・`sm02` の annot と ifa）と、A31 のレビュー役が写した A32 の種（`h21`・`h22`・`r2_ag02`・`src_side`）が、A32 の前の `E1003` から、安全な形は受理、安全でない形は `E1013`（`self_referential_store`・`assignment_extends_borrow`）になったものだけでした（`merge4/a31corpus-diff.txt`）。(3) `a31/r12`（第11回の対応）の 35 個も edition 2027 と 2026 で比べ、70 回の検査のどれも変わりませんでした（`merge4/a31r12-diff.txt`）。(4) A31 の第9回の後・第10回・第11回のプログラム（(2) の 717 個と `a31/r12/adv` の 34 個）のうち、`ref` の型の仮引数を持つ名前付き関数で、本体が書き込み（`Array.write`・`Array.set` など、ロック、`Vec.push_mut`、`deref p = v`、フィールドへの代入、`Owned.call`）を含むものを、`merge4/spell.mjs` が 7 つの綴りに書き直しました（同じものを除いて 714 個の関数、4092 個のプログラム。edition 2027。`spell-merged.json`）。merge した binary では、annot は conv・fnc と、alias は fn と、ifa は ifc と、拒否のコードと kind か受理が同じでした（位置は綴りで変わるので比べません。`spell-table2.mjs`、`spell-table2.txt`）。次の 4 種類だけは、比べ方を変えたか、形として比べられないものです。
  - 本体が仮引数の先を `deref p = v` で置き換えるか読む 34 個（A31 の第11回の `d02`〜`d07`・`w01`〜`w04`、第10回の `k01`〜`kv06` など）は、規約の綴り（conv・fnc）では仮引数が値 view なので `E1005 deref_non_reference` です。そこで annot を `ref` の署名の名前付き関数（fn）と比べると、25 個は同じでした（`d02` の `E1013 self_referential_store`、`d03`・`d04`・`w01`・`k14` などの `E1014 borrow_conflict`、`d06`・`d07`・`w03`・`w04` などの受理）。残りの 9 個は次の `pick` です。
  - `ref i64` の仮引数（`pick`。fixture の写しの 9 個）は、規約の署名では Copy の `i64` になって規約を持たないので、注釈は A32 の前と同じ `E1003` です。
  - `make`（A31 の `N_closed_cell_read`。ロックのコールバックが仮引数を読むだけで、結果の関数は借用を持たない。fixture の写しの 9 個）は、`def` の右辺のラムダで引数を参照として束縛する綴り（alias と annot）では呼び出し元の `make (ref text)` が `E1013 borrow_outlives_block` で、名前付き関数（fn・fnc）と、引数を値 view にする注釈の無いラムダ（conv）では受理です。`let` で束縛したラムダ（`let make = \(text: ref string) -> …`）も同じ `E1013` で、base（A31 と A32 の前）・merge の前・A31 の先端・merge した binary のどれでも同じ結果です（`merge4/mk/`）。annot は A32 の約束どおり `let` で束縛したラムダと A32 の前から書ける alias と同じで、安全なプログラムを拒否する側の、A32 の前からの違いなので、変えていません。
  - `t04` の ifa は、`'a` を含む注釈のラムダを `run` の中に書くので `E1015 type_variable_absent_from_signature` です（生成の都合で、元のプログラムもすべての綴りも拒否されます）。
- 文書: `lambda-expressions.md` の注釈した引数の所有権検査の段落に、`(p: ref mut Pair)` と注釈した引数の先の部分の借用を同じ値の `Mutex` へ書き込むラムダも、`def target :: ref mut Pair -> i64` と同じく、部分の順序が逆なら `E1013`（`self_referential_store`）になることを足しました。`docs/language.md`・`docs/architecture.md` の A31 の規則の列挙には、第10回・第11回の規則（排他の借用で届く持ち主の共有の自己 loan、値全体の置き換え）を足しました。A31 が書いた `drop.md`・`lifetimes.md` は、注釈した綴りにもそのまま当てはまるので変えていません。3 度目の merge の記録の、A31 の退行と穴の段落は、直ったことを書いた短い記述に縮めました。

確認したこと（ログは `target/scratch/a32/merge4/`）:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 53 passed。
- `cargo nextest run --locked --no-fail-fast`: 3079 tests run: 3079 passed (11 slow), 4 skipped（1383 秒、`t1.log`）。3 度目の merge の 3063 件に、A31 の第9回の後・第10回・第11回の 16 件を足した数です（A32 は既存のテストに case を足しただけです）。3 度目の merge で失敗した 2 件も成功します。
- `node tests/features.mjs target/debug/tsuzuri <suite>`: `conventions` 66 cases（3 度目の merge の写しの 63 件に、除いていた `callback_constructed_once` の 1 件と、A31 の `capturing_readers` の 2 件を足した数。実物の fixture）、`in_place_writes` 72 cases、`mutable_slices` 49 cases。どれも native と WASM の `-O0`・`-O3` で成功（`features.log`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs values-and-functions/lambda-expressions.md ownership-and-memory/drop.md ownership-and-memory/lifetimes.md`（`_tsuzuri/language-reference/` で実行）: 3 pages, 67 links, 22 checked examples, 44 native runs (O0/O3) で成功（`check-docs.log`）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。
- 診断の kind は増やしていません。`src/` は merge のほかに変えていません。

変更したファイル（4 度目の merge の後）: `tests/parameter_conventions.rs`、`lambda-expressions.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

#### 5 度目の merge（2026-11-13、A31 のマージ前の確認・第12回（最終）のレビューの対応・後続のチケット A33）

A31 のブランチ（`fix/a31-in-place-writes-keep-borrows` の `da4e5b85`、第12回の対応の記録と A33 の起票まで）を、もう一度このブランチへ merge しました（`f70f44ca`）。前の merge との共通の祖先は `8890c1aa`（A31 の第11回まで）で、その後の A31 の変更は、マージ前の相対の健全さの確認（`2828b752`、記録だけ）、第12回のレビューの対応（`f31af85e`: `src/ownership.rs` の `referent_pieces`・`forget_block_share`・`contents_elsewhere`・`place_leaves_storage`・`replacement_frees_kept`・`same_block_loan`・`block_groups`、`tests/rc.rs` の 4 件、`in_place_writes` の fixture の 3 件、`drop.md`・`lifetimes.md`・`docs/language.md`・`docs/architecture.md`）、記録と A33（`da4e5b85`）です。merge の前のこのブランチの先端は `e1d6cb5c`（4 度目の merge の記録まで）です。作業の記録とプログラムは `target/scratch/a32/merge5/` にあります。比べた binary は、merge の前（`merge5/tsuzuri-prev`。4 度目の merge の dev binary で、`src/`・`std/` は `e1d6cb5c` と同じ）、A31 の先端（`merge5/tsuzuri-a31`。`da4e5b85` を `git archive` で scratch に展開してビルドしたもの）、merge した dev binary（`merge5/tsuzuri-merged`）と、base（`target/scratch/base/tsuzuri`、`main` の `b3dd7f1c`）です。

- merge の衝突: `_features/README.md` の行だけでした。A31 の行は A31 のブランチの行、A32 の行はこのブランチの行で、どちらも `_completed/` へリンクします。A31 が足した A33 の行（`_features/` の todo のチケット）はそのまま残しました。`src/ownership.rs`・`tests/features.mjs`・`docs/language.md`・`docs/architecture.md` と A31 の 2 ページは自動で merge され、merge commit はそのままビルドできました。
- A32 のテストの確認: merge の直後に `tests/parameter_conventions.rs` の 53 件と、領域のテスト（`rc` 87 件、`mutable_slices` 19 件、`currying` 16 件、`borrowed_records` 21 件、`user_drop` 27 件）がすべて成功しました（223 件、`merge5/area.log`）。A32 の拒否のテスト（第1回・第2回の種、2〜4 度目の merge の形、第3回・第4回の回帰テスト）は同じコードと kind のままで、安全な対照は受理されます。conventions suite の A32 の case（`annotated_reference_stores`・`annotated_reference_later_rules`・`annotated_reference_drop_rules` など）の値も、手で数えた期待値のまま成功します（下の確認）。
- 比べたプログラム: (1) A32 のレビューと探索のプログラム 2055 個（3 度目の merge と同じ一覧）の `check --json` は、merge の前の binary と merge した binary で 1 つも変わりませんでした（`merge5/a32corpus-diff.txt`。merge の前の結果は同じ binary の `merge4/a32corpus-merged.json`）。そのうち受理される 923 個の `-O0`・`-O3` の出力も、すべて同じでした（`merge5/values-diff.txt`。`-O0` と `-O3` が違う 5 個は、前の merge と同じ `-O0` の深い再帰のスタック溢れ 1 個と、`-O3` のビルドが 180 秒の上限を超える 4k 個のリテラルのプログラム 4 個です）。(2) A31 の第12回のレビューと対応のプログラム 1227 個（`review-a31-ce-r12` の 67 個（性能のプログラム `p_*` を除く）と、その `_corpus` の 981 個、`review-a31-conf-r12` の 103 個、`a31/r13/p` の 29 個、マージ前の確認の探査 `a31/gate/probes` の 47 個）を edition 2027 と 2026 で、A31 の先端の binary と merge した binary で比べ、2454 回の検査のどれも変わりませんでした（`merge5/a31corpus-diff.txt`）。`check` は検査したディレクトリに `target/tz/kg` を書くので、A31 の worktree のプログラムは `merge5/a31c/` へ写してから検査しました。
- 参照が 2 つの値のどちらかを指す実引数（A31 の第12回の指摘 1、`referent_pieces`）: `merge5/gen.mjs` が、`ref mut Pair` の引数の先の部分の借用を同じ値の `Mutex` へ書き込む `target` を、`let r = if n > 0 then ref mut a else ref mut b` を渡す形、`if` を直接渡す形、名前付きの `inner` へ中継する形、注釈したラムダの `inner` へ中継する形の 4 つで、安全な順序（`record Pair { h: Holder, s: string }`）と逆の順序について、7 つの綴り（fn・fnc・conv・annot・ifa・ifc・alias）で書きます（56 個）。merge した binary では、annot は conv・fnc と、ifa は ifc と、alias は fn と同じで（`merge5/table.txt`）、安全な 28 個はどれも `-O0`・`-O3` とも `drop 0`・`drop 8`・`0` を出し（`run 1` は `a` へ書き込むので、先に解放される `b` のセルは `\() -> 0` のまま `drop 0`、次の `a` のセルは `a.s` の最初の `"9"` の添字 8 を読むので `drop 8`。手で数えた値）、逆の順序の 28 個はどれも `E1013 self_referential_store`（`a` を宣言した行）です。merge の前の binary では、どちらの順序も、どの綴りでも `E1013 assignment_extends_borrow`（値どうしの借用とみなした `a` の宣言）でした。A31 の先端の binary では annot・ifa と、注釈した `inner` を持つ形のすべての綴りが A32 の前の `E1003` です。
- `deref p = v` の置き換えと共有の置き換え（指摘 2・3・5）: 規約の綴り（conv・fnc）は値 view を参照外しできない（`E1005 deref_non_reference`）ので、annot を `ref` の型の名前付き関数（fn）と、A32 の前から書ける alias と比べました（`merge5/dr/`）。セルの中身を別の引数の先へ写した後の置き換え（`co_unsafe`）と、呼び出し元と共有する `Arc` のブロックのセルへ書き込んだ後の置き換え（`arc_unsafe`）は 3 つの綴りとも置き換えの位置の `E1014 borrow_conflict`、`renew (ref mut inner)` で共有を置き換えた後に古いブロックへ書き込む形（`renew_unsafe`、第12回の `d02`）は 3 つとも `E1013 assignment_extends_borrow` です。merge の前の binary ではどれも受理され、解放した文字列を読みました（`co_unsafe` は `drop 8`・`drop -1`・`drop 0`・`-1`、`arc_unsafe` は `drop 8`・`drop 0`・`-1`、`renew_unsafe` は `drop 0`・`drop -1`・`1`）。置き換えの後に新しい値のセルを写す形（`co_safe`）と、写さずに置き換える形（`own_safe`）は 3 つとも受理され、値は `drop 8`・`drop 0`・`drop 0`・`0` と `drop 8`・`drop 0`・`0` で、前の binary と同じです（手で数えた値。古い値の `h` が先に解放され、まだある古い `a[0]` を読むので `drop 8`）。共有しない `Arc` のセルへ書き込む形（`arc_own`）と、置き換えた後の共有の自分のブロックへ書き込む形（`renew_safe`）は、fn を含む 3 つの綴りとも拒否です（base は受理し、`drop 8`・`drop 0`・`0` と `drop 8`・`drop 0`・`1`）。どちらも A31 が `drop.md` の「値全体の代入」と `Rc` の共有の段落に保守的な扱いとして書いた形で、annot は fn と同じなので、変えていません。
- A31 の第12回のプログラムの綴り直し: `merge4/spell.mjs`（`merge5/spell.mjs` に写したもの）で、(2) のうち `_corpus` を除く 246 個から、`ref` の型の仮引数を持ち本体が書き込む名前付き関数を 7 つの綴りに書き直しました（1394 組、7406 個。`merge5/spell-merged.json`、`spell-table2.txt`、`spell-other.txt`）。218 組は `look`（本体が複数行の `match`）で、生成器が腕を 1 行につなぐため、ラムダの綴りがすべて構文の誤り（`E0002`）になるので除きました。本体が仮引数を参照外しする 38 組は、annot がすべて fn と同じです。残りの組は、annot が conv・fnc と、alias が fn と、ifa が ifc と同じでした。次の 2 種類だけが違います。
  - 参照を選んで渡すか返す関数（`d1u11`・`d1u12` の `via`、`v17b`〜`v17d` の `show`、`h06` の `pick`。5 組）は、規約の綴りでは仮引数が値 view なので `E1012 move_out_of_reference_binding` か `E1003` で、書けない形です。annot は fn・alias と同じです（`d1u11`・`v17b`・`h06` は受理、`d1u12` は `E1013 self_referential_store`、`v17c` は `E1013 store_local_borrow_through_reference`、`v17d` は `E1019`）。
  - `s01_passed_safe` の `apply2`（`\f (a: ref PairOk) (b: ref PairOk) -> { f a b; 0 }`）は、annot と alias が呼び出しの位置の `E1013 assignment_extends_borrow` で、fn・fnc・conv は受理です。`merge5/s01/red/` の 28 個で小さくすると、2 つの一時値（`apply2 cross (ref (mkpo ())) (ref (mkpo ()))`）を、注釈した引数として受け取り、そのまま関数値へ渡す本体だけが拒否されます（引数を 1 つだけ注釈した形も同じ）。名前付き関数、注釈の無いラムダ（`ref` の署名と規約の署名）は受理して `drop 8`・`drop 0`・`1` を出し、逆の順序はどの綴りも `E1013` です。merge の前の binary も同じ結果で、この merge で生じたものではありません。alias（A32 の前から書ける綴り）は base では受理されるので、A31 の拒否です。A31 の制限 `h01`（2 つの仮引数の部分を関数値へ渡す一時値の順序。`Source::passed` と `origin` は仮引数をそのまま渡すときしか表さない）と同じ扱いを、隠れた引数をパターンで束縛した名前にも受けるものと見て、安全なプログラムを拒否する側の違いなので、オーケストレーターの決定（最も保守的で健全な選択を取り、記録して進める）のとおり変えていません。`lambda-expressions.md`・`docs/language.md`・`docs/architecture.md` に例外として書き、A33 の保守的な制限の表に行を足しました。
  - 小さくする途中で、注釈と関係の無い A31 の拒否も見つけました: 名前付き関数の直接の呼び出し `cross (ref p2) (ref p1)`（`p1` を先に宣言するので安全。base は `drop 8`・`drop 0`・`1`）を、A31 の先端も merge の前と後の binary も `E1013 self_referential_store`（`'p1' keeps a borrow of a part of itself`）で拒否します（`merge5/s01/red/p/loc_direct`）。どの綴りでも同じなので A32 の問題ではなく、A33 に記録しました（`src/ownership.rs` は変えていません）。この拒否は A31 のマージ前の追加の修正（`StoreFacts::kept_paths`）で直り、6 度目の merge で、どの綴りでも受理されることを確かめました（A33 の記述も A31 の段落に置き換わりました）。
- テスト: `annotated_reference_parameters_follow_the_drop_order_and_lock_rules_of_in_place_writes` に、参照が 2 つの値のどちらかを指す形（`let r = if …` を渡す形、`if` を直接渡す形、注釈したラムダの `inner` へ中継する形）の逆の順序を拒否の case（`E1013 self_referential_store`。同じ規約の名前付き関数、`ref` の型の名前付き関数、注釈した `def`、`if` の綴り）として、安全な順序を受理の case として足しました。新しい `annotated_reference_parameters_follow_the_replacement_rules_of_in_place_writes` は、`co_unsafe`・`arc_unsafe`・`renew_unsafe` の注釈した `def` が `ref` の型の名前付き関数と同じコード・kind で同じ式を指すことと、`co_safe`・`own_safe` が両方の綴りで受理されることを確かめます。新しい `annotated_reference_parameters_passed_to_a_function_value_with_temporaries` は、上の `apply2` の制限を固定します（注釈した 3 つの綴りは呼び出しの位置の `E1013 assignment_extends_borrow`、名前付き関数と注釈の無いラムダの 4 つの綴りは受理、逆の順序は 7 つの綴りとも `E1013`）。merge の前の binary では、安全な順序の 2 つの値の形が拒否され、置き換えの安全でない形が受理されるので、最初の 2 つのテストは失敗します（`merge5/gen-prev.json`、`merge5/dr/r-prev.json`・`r2-prev.json`）。期待値を弱めたテストはありません。
- 文書: `lambda-expressions.md` の注釈した引数の所有権検査の段落に、参照が 2 つの値のどちらかを指す呼び出し、`deref p = v` の置き換えが借用を終わらせる条件、上の `apply2` の例外（注釈を書かない綴りか名前付き関数で書く）を足しました。`docs/language.md`・`docs/architecture.md` の A31 の規則の列挙にも、第12回の規則と同じ例外を足しました。A33（A31 が起票した todo のチケット）には、`apply2` の注釈した綴りの行と、`loc_direct` の記述を足しました。A31 が書いた `drop.md`・`lifetimes.md` は、注釈した綴りにもそのまま当てはまるので変えていません。

確認したこと（ログは `target/scratch/a32/merge5/`）:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 55 passed（53 件に新規の 2 件）。
- `cargo nextest run --locked --no-fail-fast`: 3085 tests run: 3085 passed (14 slow), 4 skipped（1310 秒、`t1.log`）。4 度目の merge の 3079 件に、A31 の第12回の `tests/rc.rs` の 4 件と新規の 2 件を足した数です。
- `node tests/features.mjs <merge した dev binary> <suite>`: `conventions` 66 cases、`in_place_writes` 75 cases（A31 の第12回の 3 件を含む）、`mutable_slices` 49 cases。どれも native と WASM の `-O0`・`-O3` で成功（`features.log`）。
- `TSUZURI_BIN=<merge した dev binary> node scripts/check-docs.mjs values-and-functions/lambda-expressions.md ownership-and-memory/drop.md ownership-and-memory/lifetimes.md`（`_tsuzuri/language-reference/` で実行）: 3 pages, 71 links, 22 checked examples, 44 native runs (O0/O3) で成功（`check-docs.log`）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`TSUZURI_BIN=… node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`TSUZURI_BIN=… node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。
- 診断の kind は増やしていません。`src/` は merge のほかに変えていません。

変更したファイル（5 度目の merge の後）: `tests/parameter_conventions.rs`、`lambda-expressions.md`、`docs/language.md`、`docs/architecture.md`、`_features/A33-remaining-store-soundness-holes.md`、このチケット。

#### 6 度目の merge（2026-11-14、A31 のマージ前の追加の修正とその見直し・`main` の merge）

A31 のブランチ（`fix/a31-in-place-writes-keep-borrows` の `2ed5f692`）を、もう一度このブランチへ merge しました（`b5569bee`）。前の merge との共通の祖先は `da4e5b85`（A31 の第12回の記録と A33 の起票まで）で、その後の A31 の変更は次のとおりです: 5 度目の merge で見つけた `loc_direct` の誤った拒否の修正（`f0003aa6`: `StoreFacts::kept_paths`・`exact_loans`・`kept_copy`、`store_target`）、その修正のレビューの対応（`bf42fad4`: `Checker::points_at`・`pointing`・`bound_value`・`Source::element_paths`、`7ef32035`: `self_borrow_dropped_first`）、それぞれの記録（`298408bd`・`ffa466fe`）、`origin/main`（`168ae4a5`: TZ15 #101、A24 #102、A25 #103、A26 #104、#106、#107）の merge（`66778cdb`）、A25 の `State::same` が A31 の状態も比べる修正（`2ed5f692`）。merge の前のこのブランチの先端は `105cdc28`（5 度目の merge の記録まで）です。作業の記録とプログラムは `target/scratch/a32/merge6/` にあります。比べた binary は、merge の前（`merge5/tsuzuri-merged`。5 度目の merge の dev binary で、`src/`・`std/` は `105cdc28` と同じ）、A31 の先端（`merge6/tsuzuri-a31`。A31 の worktree の `target/debug/tsuzuri` で、それより新しい `src/`・`std/` のファイルが無いことを確かめたもの）、merge した dev binary（`merge6/tsuzuri-merged`）、base（`target/scratch/base/tsuzuri`、`main` の `b3dd7f1c`）です。base は匿名共用体を読めないので、匿名共用体のプログラムは A31 が `origin/main` から作った binary（`target/wt/a31/target/scratch/a31/m1/bin/tsuzuri-main`、`168ae4a5`）とも比べました。

- merge の衝突は 2 つのファイルでした。
  - `src/ownership.rs`: `Checker` のフィールドと初期化は、A32 の `matched_by`（第4回）と `main` の `injections`（A24）の両方を残しました。ムーブの後の使用の `E1012` を作る所では、A32 が第3回・第4回で `use_after_move` に分けた処理へ、`main` が、照合した一時値（`$match…`）の文言と、拡大（A25）・注入（A24・A26）の help を足していました。照合した一時値は `main` の文言、ソースが綴る名前で拡大か注入の help があるものは `main` の文言と help、それ以外（ソースが綴るそのほかの名前と、隠れた引数 `$lambda…` などの生成した名前）は A32 の `use_after_move` にしました。ソースが綴る名前で拡大も注入も無いときの文言と help は、どちらの側でも同じです。生成した名前（`$` で始まる）に拡大・注入の help を付けないのは、その help が名前を示すからです（第3回の規則: 生成した名前を示さない）。隠れた引数はソースから読めないので、拡大や注入でムーブされることもありません。
  - `_features/A33-remaining-store-soundness-holes.md`: `loc_direct` の段落は、A31 の段落（`kept_paths` で直した）を残し、A32 が 5 度目の merge で書いた「文書に無い拒否」の段落を消しました（A31 の先端と merge した binary で受理されることを、下で確かめました）。A32 の `apply2` の注釈した綴りの行は残しました。
  - `_features/README.md` は自動で merge されました。A31 の行は A31 のブランチの行、A32 の行はこのブランチの行、A24〜A26 の行は `main` の行で、A33 の行は両方で同じです。
- 意味の衝突: A24 が `src/anonymous_union.rs` に足した `Checker::literal_takes`（数のリテラルの推論変数が匿名共用体の要素の型になれるか）と、A32 が第3回で `src/closures.rs` に足した同じ型の同じ名前のメソッドが衝突し、merge した木はビルドできませんでした。A32 のメソッドを `annotated_literal_takes` に改名し（中身は変えていません）、`docs/architecture.md` の名前も直して、merge commit に含めました。
- A32 のテストの確認: merge commit で `tests/parameter_conventions.rs` の 55 件がすべて成功しました。A32 の拒否のテスト（第1回・第2回の種、2〜5 度目の merge の形、第3回・第4回の回帰テスト）は同じコードと kind のままで、安全な対照は受理されます。`apply2` の制限のテスト（5 度目の merge）も、注釈した 3 つの綴りが `E1013 assignment_extends_borrow` のままです。
- `loc_direct` と `apply2` の綴り（5 度目の merge の `merge5/s01/red/p` の 30 個を `merge6/s01/p` に写して検査。`s01/table.txt`）: 名前付き関数の直接の呼び出し `cross (ref p2) (ref p1)`（`loc_direct`）は、merge の前の binary では `E1013 self_referential_store` でしたが、A31 の先端と merge した binary では受理され、`-O0`・`-O3` とも `drop 8`・`drop 0`・`1` を出します（`p1` を先に宣言するので、先に解放される `p2` のセルは `p1.s`（`"123456789"` を 4 回繰り返した文字列）の最初の `"9"` の添字 8 を読んで `drop 8`、次の `p1` のセルは `\() -> 0` のままで `drop 0`、`run 1` の結果は 1。手で数えた値で、base と同じです）。同じ呼び出しを `apply2` を通して書いた `loc_safe` は、7 つの綴り（`ref` の署名と規約の署名の名前付き関数、注釈の無いラムダ 2 つ、注釈したラムダ 3 つ）のどれでも受理されて同じ値を出し、逆の順序の `loc_unsafe` と一時値の逆の順序の `tmp_unsafe` は、どの綴りでも `E1013 assignment_extends_borrow` です（base は受理して `drop 0`・`drop -1`・`1`、つまり解放した文字列を読みます）。一時値の安全な順序 `tmp_safe` は、注釈した 3 つの綴りが merge の前と同じ `E1013 assignment_extends_borrow` で、`ref` の型の署名に注釈したラムダの綴り（`tmp_safe__alias`）は A31 の先端でも同じなので、A33 の `apply2` の行はそのまま残ります。
- 注釈した綴りの直接の呼び出し（`merge6/gen.mjs`、120 個。`d-table.txt`）: `cross` そのものを 10 の綴り（`ref` の署名の名前付き関数 fn、規約の署名の名前付き関数 fnc、注釈の無いラムダ conv、両方の引数を注釈したラムダ annot、片方だけを注釈した annot1a・annot1b、`ref` の署名の注釈したラムダ alias と注釈の無いラムダ reflam、注釈したラムダを `else` の枝に置いた `if` の ifa と、両方の枝を `def` にした双子 ifc）で書き、6 つの形（直接の呼び出し、パイプライン `ref p1 |> cross (ref p2)`、部分適用 `let g = cross (ref p2); g (ref p1)`、不変な参照の変数 `let r = ref p1`、2 つの値のどちらかを指す参照 `if n > 0 then ref p1 else ref p0`、引数を名前付きの `cross` へ中継する `wrap`）の安全な順序と逆の順序を、edition 2027 で検査しました。merge した binary では、annot・annot1a・annot1b は conv・fnc と、alias・reflam は fn と、ifa は ifc と、すべての形で同じ結果（拒否のコードと kind、受理の値）でした。安全な順序は、下の 1 組を除いてどれも受理され、`-O0`・`-O3` とも手で数えた値（`drop 8`・`drop 0`・`1`。`p0` もある形は、`p0` のセルも `\() -> 0` のままなので `drop 8`・`drop 0`・`drop 0`・`1`）を出し、逆の順序はどれも `E1013 assignment_extends_borrow` です（base は受理し、`drop -1` を含む値を出します）。merge の前の binary では、安全な順序はどの綴りでも拒否されていました（`self_referential_store`、2 つの値のどちらかを指す形は `assignment_extends_borrow`）。
  - 例外の 1 組: パイプラインと部分適用を `if` の綴り（`let chosen = if n < 0 then cross else …` の `chosen`）で書いた安全な順序は、ifa も ifc も `E1013 assignment_extends_borrow`（`p1` の宣言）です。注釈の無い双子も同じなので A32 の問題ではなく、A31 の先端でも、名前付き関数そのものを束縛した `let chosen = cross` や `let chosen = if n < 0 then cross else cross` のパイプライン・部分適用が、edition 2027 と 2026 で同じく拒否されます（base は受理して `drop 8`・`drop 0`・`1`。`merge6/fv/`）。診断の注記は、`p1` が後に宣言した `p2` の借用を持つとしています。A31 の文書の制限にない拒否なので、A33 に記録しました（`fvpart`）。直接の呼び出し `chosen (ref p2) (ref p1)` は受理されます。
- 匿名共用体と注釈した綴り（`merge6/au/`、edition 2027）: 匿名共用体の参照を注釈したラムダ（`def size :: (i64 | string) -> i64 = \(x: ref (i64 | string)) -> match x with | :? i64 as n -> n | :? string as s -> s.length`、`Array.map` の読み取りの callback の `\(x: ref (string | i64)) -> …`（要素の順序は型を変えません）、`if` の `else` の枝、`ref mut` の注釈で匿名共用体のフィールドを書き換えるラムダ）は、注釈の無い双子と `ref` の署名の名前付き関数と同じく受理され、`-O0`・`-O3` とも手で数えた値（`45`、`321`、`11022`、`14`）を出します。`origin/main` の binary と A31 の先端では、callback・`if`・`ref mut` の注釈した形は A32 の前の `E1003` です（読み取りの引数の `def` の右辺の形は、`string` の引数と同じく A32 の前から受理される形で、どの binary でも `45` です）。型の違う注釈（`\(x: ref (i64 | f64))`）は、どの binary でも注釈の位置の `E1003` です。
- 比べたプログラム: (1) A32 のレビューと探索のプログラム 2055 個（3 度目の merge と同じ一覧）の `check --json` は、merge の前の binary と merge した binary で 1 つも変わりませんでした（`merge6/a32corpus-diff.txt`。merge の前の結果は同じ binary の `merge5/a32corpus-merged.json`）。そのうち受理される 923 個の `-O0`・`-O3` の出力も、すべて同じでした（`merge6/values-diff.txt`。`-O0` と `-O3` が違う 5 個は、前の merge と同じ `-O0` の深い再帰のスタック溢れ 1 個と、`-O3` のビルドが 180 秒の上限を超える 4k 個のリテラルのプログラム 4 個です）。(2) 5 度目の merge の後の A31 のプログラム 407 個（`a31/r14`（`kept_paths` の修正の探査）、`a31/r15/p`（その見直しの探査）、`a31/r14-review`（見直しのレビュー役の反例と適合の確認）、`a31/m1`（`main` の merge の匿名共用体の探査））を `merge6/a31c/` へ写し、edition 2027 と 2026 で、A31 の先端の binary と merge した binary で比べました。814 回の検査で違う 16 個は、A31 の探査が写した A32 の `apply2` の綴り（`r14/p` と `r14-review/conf/a32` の `loc_safe`・`loc_unsafe`・`tmp_safe`・`tmp_unsafe` の annot と annot1。edition 2027）が、A32 の前の `E1003` から、上の `s01` と同じ結果（`loc_safe` は受理、そのほかは `E1013 assignment_extends_borrow`）になったものだけです（`merge6/a31corpus-diff.txt`）。
- テスト: `annotated_reference_parameters_follow_the_kept_parts_of_in_place_writes`（`tests/parameter_conventions.rs`）を足しました。上の 6 つの形で、安全な順序が、同じ規約の名前付き関数（`convention_twin`）、`ref` の型の名前付き関数（`twin`）、注釈した `def` のどれでも受理されること、逆の順序が、`convention_twin` と同じ `E1013 assignment_extends_borrow` で、`twin` と注釈した `def` が同じ式を指すことを確かめます。`if` の綴り（注釈した `else` の枝と、注釈の無い双子）は、逆の順序が同じコードと kind になること、安全な順序が直接の呼び出し・参照の変数・2 つの値のどちらか・中継では受理され、パイプラインと部分適用では上の A31 の制限（`fvpart`）のとおり `E1013 assignment_extends_borrow` になることを固定します。merge の前の binary では安全な順序がどの綴りでも拒否されるので、このテストは失敗します（`merge6/d-prev.json`）。期待値を弱めたテストはありません。
- 文書: `lambda-expressions.md` の注釈した引数の所有権検査の段落に、`(a: ref PairOk) (b: ref PairOk)` と注釈した引数の一方の先の部分の借用をもう一方の先へ書き込むラムダも、名前付き関数と同じく書き込み先にその部分の借用だけを残すことを足しました。`docs/language.md`・`docs/architecture.md` の A31 の規則の列挙にも、`kept_paths`・`kept_copy`・`points_at`（注釈した引数を束縛したパターンの変数は `Checker::note_pointing` が記録する）を足しました。A33 には `fvpart` の段落を足し、段 3 の対象に加えました。A31 が書いた `drop.md`・`lifetimes.md` と、`main` が書いた匿名共用体のページは、注釈した綴りにもそのまま当てはまるので変えていません。

確認したこと（ログは `target/scratch/a32/merge6/`）:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 56 passed（55 件に新規の 1 件）。
- `cargo nextest run --locked --no-fail-fast`: 3302 tests run: 3302 passed (23 slow), 4 skipped（1980 秒、`t1.log`）。A31 の先端の 3282 件（A31 の `main` の merge の記録）に、A32 のテスト 20 件を足した数です。5 度目の merge の 3085 件からは、A31 の修正とその見直しのテスト、`main` の A24〜A26 などのテストと、新規の 1 件が増えました。
- `node tests/features.mjs target/debug/tsuzuri <suite>`: `conventions` 66 cases、`in_place_writes` 77 cases（A31 の `crossed_references_keep_the_part`・`bound_parts_keep_their_strings` を含む）、`mutable_slices` 49 cases。どれも native と WASM の `-O0`・`-O3` で成功（`features.log`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs <19 ページ>`（`_tsuzuri/language-reference/` で実行。この merge で変わった 18 ページ（A31 の `drop.md`・`lifetimes.md` と、`main` の匿名共用体・`match`・型・診断などのページ。`check-docs` が受け付けない `index.md` を除く）と `lambda-expressions.md`。一覧は `check-docs-pages.txt`）: 19 pages, 555 links, 119 checked examples, 226 native runs (O0/O3), 1 test projects, 3 generated blocks で成功（`check-docs.log`）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。`TSUZURI_BIN=… node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`TSUZURI_BIN=… node scripts/check-cli-docs.mjs`: 4 checks, 0 failed（`light-checks.log`）。
- 診断の kind は増やしていません。`src/` は merge の解決と改名のほかに変えていません。

変更したファイル（6 度目の merge の後）: `src/ownership.rs`・`src/closures.rs`（merge の解決と改名）、`tests/parameter_conventions.rs`、`lambda-expressions.md`、`docs/language.md`、`docs/architecture.md`、`_features/A33-remaining-store-soundness-holes.md`、このチケット。

### レビューの指摘への対応（第3回）

指摘ごとに、A31 の上に重ねたこのブランチの dev binary と、base の binary（A31 の worktree の `target/debug/tsuzuri`）で再現してから対応しました。再現に使ったプログラムは `target/scratch/review-a32-ce-r3/` と `target/scratch/a32/review3/`（`cmp.sh` が base と修正後を `-O0`・`-O3` で比べます）にあります。

1. **[minor] `ref mut` の注釈の as パターンで、`E1012` とその help が内部の名前 `$lambdaN` を出す**: 再現しました。`n02_as_mut_lambda`（`[Mut]` の位置の `\(step: i64) ((c as d): ref mut Counter) -> …`）は、base では `E1003 expected ref mut Main.Counter, found Main.Counter`、修正前のブランチでは `E1012 use of moved or partially moved value '$lambda193'`（help `borrow the value with 'ref $lambda193' or move it last`）でした。`def` の形の `p13_as_mut` は `'$lambda51'` です。束縛したラムダの `n02_as_mut_bound` と、明示の `ref mut` の署名の `p13_as_explicit` は base でも同じ文言です。同じ漏れは A32 と関係なく、所有値の `\((s as t): string) -> …`（`v01`）、`\(((a, b) as whole): (string * string)) -> …`（`v02`）、`&` パターン `\((s & t): string)`（`v06`）、構築子の中の `\((Maybe.Some (s as t)): Maybe<string>)`（`v07`）でも base から起きていました。**直しました。** `src/ownership.rs` の `E1012` を作る所を `use_after_move` に分けました。
   - コンパイラが作った名前（`$` で始まる名前）は表示しません。`escape` の `DeclaredHere` と、借用の衝突の help が名前を選ぶときと同じ規則です。
   - パターンを持つ引数の隠れた名前 `$lambdaN`（ラムダの引数と、`normalize_definition` が移した `def` の引数）は、本体から名前で使えません。二度目の使用は、パターン自身が Copy でない値を 2 つの名前に束縛するとき（`as`・`&`）だけです。そこで文言を `use of moved or partially moved value: this parameter's pattern binds the value, or a part of it, to more than one name`、help を `a value that is not Copy, such as an owned value or a 'ref mut' reference, moves into the first name that binds it, and an 'as' or '&' pattern cannot bind it again; keep only one of the names` にしました。
   - ほかの生成した名前は、名前を書かない `use of moved or partially moved value` と、help `borrow the value with 'ref' or move it last`（部分がムーブされたときは `borrow the field with 'ref' instead of moving it, or move it last`）にしました。
   - 利用者が書いた名前の文言は変わりません（`match text with | (s as t) -> …` は `'text'` と `ref text` のまま）。拒否するプログラム、コード（`E1012`）、位置（引数のパターン）、`value moved here` の注記も変わりません。`p13_as_mut` の 2 つ目の誤り（`E1013 reference has no live owner`）は、二重の束縛から回復した後の続きの誤りで、base の `p13_as_explicit` にもあるので変えていません。
   - 回帰テスト `patterns_that_bind_a_moved_value_twice_name_no_hidden_parameter`（`tests/parameter_conventions.rs`）は、`[Mut]` の位置のラムダ（`n02`）、`def` の `Mut` 引数、`p13` の `def put`、束縛したラムダ、明示の `ref mut` の署名、所有値の as、タプルの as、`&` の 8 形について、コード・文言・help・位置（引数のパターンの先頭）を確かめ、利用者の名前の文言が変わらないことも確かめます。修正前のコードでは、最初の形が `'$lambda199'` の文言で失敗することを確かめました。
2. **[minor] 整数リテラルの配列で、`\(x: ref i64)` の読み取り callback がリテラルの位置の `E1003` になり、文書の例外にも無い**: 再現しました。`i05`（`[1, 2, 3] |> Array.map (\(x: ref i64) -> deref x * 2)`）、`i01`（`let xs = [1, 2, 3]` の後の `Array.fold`）、`i08`（`List.map`）は、base もブランチも `E1003 expected an integer, found ref i64` で、位置はリテラルです。原因は次のとおりです。callback を検査する時点で、要素の型はリテラルの型（`Integer`・`SignedInteger`・`Float` の制約と既定の型を持つ推論変数）のままです。`binds_annotated_reference` は `Type::Infer` を除くので、引数は値 view のままでした。パターンの注釈がリテラルの型を `ref i64` に解き、最後の制約の検査がリテラルで失敗していました。**直しました。** 文書に例外として書く案もありましたが、束縛した形（`i06`）は base から `6` で通ります。A32 の目的は直接書いた形を束縛した形にそろえることなので、受理する方を選びました。
   - 規則: `binds_annotated_reference` は、`inner` が数値リテラルの型（`Inference::is_numeric_literal`）のときだけ、推論変数でも先へ進みます。参照を束縛するのは、新しい `literal_takes`（`src/closures.rs`）が真のときだけです。条件は次の 4 つです。
     - 注釈の参照先が整数か浮動小数点の型である。
     - その変数を含む class 制約のすべてが、変数を参照先に置き換えても成り立ち（`Classes::holds_class`）、ほかの未決の変数を含まない。
     - 変数そのものへの制約が 1 つ以上ある（リテラル自身の制約）。
     - member 制約がその変数を含まない。
   - 健全性と、今受理しているプログラムが変わらないこと: リテラルの型に付く `Integer`・`SignedInteger`・`Float` の制約は、参照では成り立ちません（`builtin_instance_by_type`）。利用者はこれらの instance を足せません（`E1016 built-in instances and marker classes cannot be overridden`。`review3/w_*` で `ref Foo` も試しました）。参照先は具体的な数値型なので、値 view ではリテラルの型が `ref i64` のような変数を含まない型に解け、制約の検査が必ず失敗していました。したがって変わるのは型エラーだったプログラムだけです。参照先が型変数の注釈（`ref 'a`）は条件に当たらないので、制約が `def` の呼び出し側へ持ち越される形（`j10`）も変わりません。束縛した後の引数の型は束縛した形と同じで、ラムダの生成コードも同じです（テストで確かめました）。所有権検査は変えていません。参照を束縛した引数は、これまでと同じく `let` で束縛したラムダの引数と同じ検査を受けます。
   - 合わない注釈の引数は値 view のままで、診断は base と同じです: `ref string`（`j01`・`k01`）、整数のリテラルへの `ref f64`（`j03`）、`ref mut i64`（`j04`）、負のリテラルへの `ref i8u`（`j02`・`k02`）、`i64` の instance の無い利用者の class の制約（`j13_show`・`k03`。base と同じ `E1015`）。
   - 受理するようになった形と値（手で数えた値。束縛した形があるものはその値とも一致）: `i01` `6`、`i05` `6`、`i08` `3`、`Array.map` を直接呼ぶ `j06` `6`、先に `xs[0] + 10` を使う `j07` `611`、配列を捕捉した関数のある `j08` `603`、負のリテラルの `j09` `19990`、`Array.mapi` の `j12` `203`、`i8u` の `j05` `251`（束縛した形も `251`）、利用者の class の `i32` の `j13_show_i32` `2107`（束縛した形も `2107`）。`i07` と `j11`（`f64` の要素）は要素の型がすでに `f64` に決まっているので、第3回の前から通っていました。リテラルは注釈で決まった型で範囲を検査されます。`[300, 1] |> Array.map (\(x: ref i8) -> …)`（`r01_range`）は `E1003` から、束縛した形（`r01_range_bound`）や `let xs: [i8] = [300, 1]` と同じ `E1009 integer literal is outside the range of i8` になりました（拒否のままで、文言だけが束縛した形にそろいます）。`[127, -128]` と `ref i8`（`r03_big`）は受理され、`127` です。
   - 回帰テスト `annotated_reference_callbacks_over_number_literals`（`tests/parameter_conventions.rs`）は、受理する 9 形（本体で `let view: ref i64 = x` が通ることを含む）、直接書いた形と束縛した形のラムダの IR が同じこと、範囲外のリテラルが直接書いた形でも束縛した形でも `E1009` になること、合わない参照先 4 形のコード・文言・位置（リテラル）、利用者の class の 2 形（`i32` は受理、`i64` は `E1015`）を確かめます。修正前のコードでは最初の形で失敗することを確かめました。suite `conventions` には `annotated_literal_callbacks` を足しました（入力は `-3`・`0`・`1`。期待値は `tests/features.mjs` の BigInt の式で手で書きました）。
3. **[minor] `tsuzuri explain call_non_function` が関数の参照の呼び出しを説明しない**: 再現しました。`f01`（`\(f: ref (i64 -> i64)) x -> f x`）はブランチで `E1005` になり、help は `deref` を案内します。しかし説明のページは、呼んだ値を数・レコードなどの値とだけ書き、Bad/Good の例も `x (x + 1)` だけでした。**直しました。** `src/explain/call_non_function.md` の Why に関数の参照を足しました。`ref (i64 -> i64)` と宣言した引数や、`\(g: ref (i64 -> i64)) -> …` と注釈したラムダの引数は呼べないので、`(deref g) x` と参照外ししてから呼ぶ、という内容です。Bad/Good の例には `def apply :: ref (i64 -> i64) -> i64 -> i64 = \g x -> g x` と `(deref g) x` の行を足しました。各節に例を 1 つずつ置く `tests/explain.rs` の形のままです。明示の `ref` の署名にしたのは、例は edition を指定しない `analyze_modules_all` で検査されるので、2026 でも 2027 でも同じ診断になる形にするためです（両方の edition で確かめました）。ページは 871 文字で、上限は 1200 文字です。回帰テスト `call_non_function_page_covers_references_to_functions`（`tests/explain.rs`）は、Why が関数の参照に触れること、Bad の例の診断に関数の参照の help を持つ `call_non_function` があること、Good の例が `(deref g) x` を含むことを確かめます。修正前のページでは失敗することを確かめました。

文書: `lambda-expressions.md`（数値リテラルの要素の段落と `run=615` の例、引数の as パターンの `E1012`）、`pattern-matching.md`（as パターンの二重消費が引数のパターンでも同じこと）、`array.md`・`list.md`（数値リテラルの要素）、`docs/language.md`（数値リテラルの型の条件、`E1012` の文言、`tsuzuri explain`）、`docs/architecture.md`（`literal_takes`）。

確認したこと:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions`: 48 passed（46 件に新規の 2 件）。`cargo test --locked --test explain`: 17 passed（16 件に新規の 1 件）。
- `cargo nextest run --locked`: 3030 tests run: 3030 passed (21 slow), 4 skipped（A31 との統合の後の 3027 件に新規の 3 件）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 成功（56 cases。第2回までの 53 件に `annotated_literal_callbacks` の 3 件。native と WASM の `-O0`・`-O3`）。`node tests/features.mjs target/debug/tsuzuri`（全 suite）: `features: 25230 cases passed`。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`lambda-expressions.md`・`pattern-matching.md`・`array.md`・`list.md`）: 成功（4 pages, 75 links, 72 checked examples, 144 native runs (O0/O3)）。
- 今受理しているプログラムの IR: `tests/fixtures/` の `Main.tz` を持つ 144 個（`conventions` を除く）の native と `--target wasm32`、`examples/` の 11 個の `.tz` を、base の binary と修正後の binary で `--emit llvm --no-cache` し、`.ll`・標準エラー出力・終了コードがすべて byte 単位で一致しました（`target/scratch/a32/review3/irall.sh`・`irex.sh`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`: 19 blocks checked, 0 stale。`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`: 4 checks, 0 failed。`node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`・`--into _features/README.md`: 0 broken。
- 診断の kind は増やしていません（`E1012 use_after_move` の文言だけ）。`src/runtime/` と std は変えていません。

変更したファイル: `src/ownership.rs`、`src/closures.rs`、`src/explain/call_non_function.md`、`tests/parameter_conventions.rs`、`tests/explain.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`、`lambda-expressions.md`、`pattern-matching.md`、`array.md`、`list.md`、`docs/language.md`、`docs/architecture.md`、このチケット。

### レビューの指摘への対応（第4回）

指摘ごとに、修正前のこのブランチの dev binary と、base の binary（A31 の worktree の `target/debug/tsuzuri`、A31 の `ec955303`）で再現してから対応しました。修正前の binary は、2 度目の merge の `717eaa28` で作った `target/scratch/a32/merge2/tsuzuri-merged` です（`717eaa28` と `0b2fbc9a` の `src/`・`std/` は同じです）。再現に使ったプログラムは `target/scratch/review-a32-ce-r4/`、`target/scratch/a32/review4/p/`、`target/scratch/a32/review4b/` にあります。`target/scratch/a32/review4b/cmp3.sh` が base・修正前・修正後を `-O0`・`-O3` で比べます。以下の結果は、どれも `-O0` と `-O3` で同じでした。

1. **[minor] パターンの引数の後の関数ガードの節で、`E1012` の文言が `as`・`&` パターンのせいにする**: 再現しました。`gd01`（`def classify :: own string -> i64 = \(s: string) -> | t when t.length > 3 -> 1 | _ -> 0`）は、base では `1:38 E1012 use of moved or partially moved value '$lambda37'`、修正前のブランチでは同じ位置で第3回の文言（`this parameter's pattern binds the value, or a part of it, to more than one name`、help は `as`・`&` の名前を 1 つにするよう案内）でした。A32 で新しく届く綴り `gd02`（`def bump :: mut Counter -> i64 = \(c: ref mut Counter) -> | d -> …`）と `gd03`（`[Mut]` の位置のラムダ）は、base の `E1003` から、この文言の `E1012` と続きの `E1013 reference has no live owner` になっていました。明示の `ref mut` の署名の `gd04` は base でも `'$lambda68'` の `E1012` と `E1013` です。原因は第3回の前提の誤りです。`guarded_definition`（`src/parse_control.rs`）は、関数ガードの節を、引数の名前（パターンの引数では隠れた名前 `$lambdaN`）を主語とする `MatchOrigin::FunctionGuard` の `match` にします。この `match` はパターンの分解の連なりの内側にあるので、Copy でない値はパターンがムーブした後に、節がもう一度照合します。**直しました。**
   - 所有権検査（`src/ownership.rs`）の `Checker` に、主語を読んでいる `match` の範囲 `matched_by` を足しました。`eval_match`（`src/ownership_control.rs`）が主語の評価（`alias_source`）の間だけ設定します。隠れた引数を名前で直接読むのは、その引数の宣言の位置（パターンの範囲）にある主語の式だけです（分解の `match` の主語と、関数ガードの節の主語。どちらも `name.span` で作られます）。`as`・`&` の二度目の束縛は分解の `match` の主語の局所からの射影で、範囲は分解の `match` 全体なので、宣言の位置とは違います。分解の `match` の主語は最初の使用なので、ムーブ後の使用になることはありません。そこで、ムーブ後の使用の範囲が隠れた引数の宣言の位置と同じなら、それは節の照合です。
   - そのときの `E1012` は、位置を節（最初の `|` から最後の節まで）にし、文言を `use of moved or partially moved value: these clauses match the parameter again after its pattern moved the value`、help を `a value that is not Copy, such as an owned value or a 'ref mut' reference, moves into the names of the parameter's pattern, so the clauses after '->' cannot match the parameter again; match those names with 'match' in the body instead, or write clauses that only test conditions on them, as in '| x > 0 -> …'` にしました。`value moved here` の注記は引数のパターン（部分だけがムーブされたときは最後にムーブされた部分）を指します。help の 2 つの直し方は、`gd01` と `gd02` のどちらの形でも受理されることを確かめました（本体の `match s with …`、条件だけの節 `| s.length > 3 -> 1 | otherwise -> 0`、`| _ when … -> …`）。
   - 拒否するプログラムとコード（`E1012`）は変わりません。`as` のパターンの二重の束縛（`\((s as t): string) -> …`）は、節があっても先にパターンで報告され、第3回の文言のままです。引数が 2 つの節（`\(s: string) (r: string) -> | (t, u) when … -> …`）は、base と修正前では引数ごとに 2 つの `E1012` でしたが、どちらも同じ節の位置と文言になるので 1 つにまとまります（`review4/p/g03`。注記は最初の引数のパターンを指します）。
   - 直さないもの: `gd02`〜`gd04` の 2 つ目の誤り `E1013 reference has no live owner` は、ムーブ後の使用から回復した後の続きの誤りです。利用者が書いた名前でも base から同じように出ます（`let d = c` の後の `let e = c` と `e.count = …`、`match c with | (d as e) -> { e.count = … }`、`let d = c` の後の `match c with | e -> …`。`review4/p/cas1`〜`cas3`）。所有権検査の回復の一般の動きなので、変えていません。
   - 回帰テスト `function_clauses_after_a_moving_pattern_are_named_as_the_second_use`（`tests/parameter_conventions.rs`）は、所有値の `def`（`gd01`）、`Mut` の `def`（`gd02`）、`[Mut]` の位置のラムダ（`gd03`）、明示の `ref mut` の署名（`gd04`）、束縛したラムダ、2 引数の節、部分だけをムーブするタプルのパターン、`\mut (s: string)` の 8 形について、コード・文言・help・位置（最初の節）・注記の文言と位置（引数のパターン）を確かめます。as パターンが先に報告されること、help の直し方の 5 形が受理されることも確かめます。修正前のコードでは、最初の形が第3回の文言で失敗することを確かめました。
2. **[minor] 要素の内側に数値リテラルの型がある配列で、型の違う参照の注釈の誤りがリテラルの位置に出る**: 再現しました。`l06`（`let xs = [[1, 2], [3]]` の後の `xs |> Array.map (\(x: ref [string]) -> x.length)`）は、base では注釈したパターンの位置の `3:32 E1003 expected ref [string], found [an undetermined type]`、修正前のブランチではリテラル `1` の位置の `2:16 E1003 expected an integer, found string` でした。1 行の `l02` も同じです（base は `2:43`、ブランチは `2:16`）。原因は、`binds_annotated_reference` が最上位の推論変数だけを除き、`literal_takes` も最上位のリテラルだけを見ていたことです。`Inference::unifies` の試しは `ref [?lit]` と `ref [string]` の内側の変数も解くので、形が合う注釈はどれも合うと数えられ、パターンの検査が `?lit` を `string` に解いて、リテラルの制約が遠くで失敗していました。**直しました。**
   - 規則: 参照先の型 `inner` が未決の変数を含むときは、`Inference::unifies` の試しの解が入っている間に、その変数を含む class 制約のすべてが、閉じて（ほかの未決の変数を含まず）成り立つときだけ参照を束縛します。その変数を含む member 制約があるときは束縛しません。`Inference::unifies` は、解を戻す前に試しの解で問い合わせる関数を受け取るようにし、`Inference::open_variables` を足しました。`inner` が変数を含まないときは今までと同じ判定です。最上位のリテラル（第3回の `literal_takes`）は、同じ問い合わせが `literal_takes` と同じ条件になるので、変わりません。
   - base が受理するプログラムが変わらないこと: この規則は束縛する条件を足すだけです。束縛しないときは、`inner` が推論変数・参照・型適用でない値 view なので、base と同じく注釈と `unify` で決して合わず、パターンの `E1003` になります。したがって base が受理するプログラムには触れません。A32 が受理するようになった形のうち、内側のリテラルが注釈の型を取れる形は受理のままです。`review4/p/n01`（`[[1, 2], [3]] |> Array.map (\(x: ref [i64]) -> x.length * 10 + x[0])` の `ys[0] + ys[1] * 100 + n`、`n = 5`）は `21 + 1300 + 5 = 1326`、`n05`（`[(1, [2, 3]), (4, [5])]` と `ref (i64 * [i64])`、`a * 10 + b.length`）は `12 + 4100 + 5 = 4117`、`l01`（要素 `[1, 2]`・`[3, 5]`）は `21 + 2300 = 2321` です（手で数えた値。base はどれも `E1003`）。制約の無い内側の変数（`review4b/m3`、型の決まっていない引数 `p` の `[[p]]` と `ref [string]`）も、修正前と同じく束縛して `11` です。
   - 狭くなった形（最も保守的な健全な選択として記録します）: 試しの解を入れても閉じない class 制約は、成り立つと数えません。そのため、内側の変数を含む class 制約がほかの未決の変数も含む形は、修正前のブランチでは束縛して受理していましたが、base と同じ注釈の位置の `E1003` に戻りました。`review4b/o4`（`\p q -> …` の本体で `(p, q) == (p, q)` と比べた後の `[[p]] |> Array.map (\(x: ref [i64]) -> …)`。`Eq` の制約が `p` と `q` の型を含みます）は、修正前は `115`（`(1 * 10 + 5) + 1 * 100`）、修正後は base と同じ `4:39 E1003 expected ref [i64], found [an undetermined type]` です。`p == p && q == q` と別々に比べる `review4b/o8` は、`p` の制約が試しの解で閉じるので、修正前と同じく `115` です。member 制約（モジュール関数制約）がその変数を含む形も同じく束縛しません。どちらも型エラーで止まるだけで、誤った受理にはなりません。ラムダを先に `let` で束縛すれば受理されます（`review4b/o5` は base・修正前・修正後とも `115`）。成り立つかどうかが試しの時点で決まらない制約まで受け入れると、決まらないまま失敗する制約の誤りが、また注釈から離れた位置に出るので、第3回の `literal_takes` と同じく閉じた制約だけを数えました。リファレンスの例外の一覧（`lambda-expressions.md`）にも書きました。
   - 合わない注釈は値 view に戻り、base と同じ位置と文言になりました: `l02`・`l06`（`expected ref [string], found [an undetermined type]`）、整数のリテラルへの `ref [f64]`（`n02`）、`ref (i64 * [string])`（`n06`）。束縛した形の `l02b` はリテラルの位置の `E1003` で、base と同じです（束縛した形は、ラムダの型が先に決まってからリテラルを照合するためです）。
   - 回帰テスト `annotated_reference_callbacks_over_nested_number_literals`（`tests/parameter_conventions.rs`）は、合わない 4 形のコード・文言・位置（注釈したパターン）と、合う 2 形の受理（本体で `let view: ref [i64] = x` が通ることを含む）を確かめます。修正前のコードでは、最初の形がリテラルの位置の `expected an integer, found string` で失敗することを確かめました。
3. **[minor] A32 の前からの不具合: `E1013 region_callback_stores_exclusive_input` の文言が内部の名前 `Vec.push_mut.3` を出す**: 再現しました。`vp02`（`if` の `else` の枝に注釈したラムダ。base は `E1003`）はブランチで `4:36 E1013 'Vec.push_mut.3' may store an input through an exclusive reference, so call it directly with all of its arguments` で、注釈の無い `vp03`、束縛した `vp04`、別名の `vp05`（`let push = Vec.push_mut`）は base でも同じ文言です。拒否そのものは正しい（借用を持つ要素の `Vec.push_mut` の関数値は、格納した借用を呼び出し側へ残せない）ので、文言だけを直しました。A32 の前からの不具合ですが、A32 が新しく届かせる綴りからも出るので、第3回の `$lambdaN` と同じく直しました。**直しました。**
   - `validate_contract_calls`（`src/regions.rs`）は、関数の名前を `written_name` で表示します。ジェネリックな関数の instance（`leak.$mono.2`）は宣言した名前にし（`ownership.rs` の `callee_borrow_stored` と同じ規則）、多相な builtin の instance が下ろされた関数（module が `$builtin` で、名前の最後が `.` と数字）は、builtin の名前（`Vec.push_mut`）にします。文言の形、コード、kind、位置は変わりません。
   - 回帰テスト `builtins_that_store_through_exclusive_references_are_named_as_written`（`tests/parameter_conventions.rs`）は、`vp02`・`vp03`・`vp05` の形のコード・kind・文言（`'Vec.push_mut' may store …`）・位置を確かめます。テストの別名の形と同じプログラム（`review4b/t3`）は、修正前の binary では `'Vec.push_mut.2'` の文言なので、このテストは修正前のコードでは失敗します。修正後の `vp02`〜`vp05` は、位置を変えずに `'Vec.push_mut' may store …` になりました。

文書: `lambda-expressions.md`（要素の内側の数値リテラルの段落、注釈を参照先の値と比べる場合の一覧に閉じない制約の形、パターンの引数の後の関数ガードの節の `E1012`）、`functions.md`（関数ガードの節は分解する前の引数を照合すること）、`docs/language.md`（同じ 3 点）、`docs/architecture.md`（内側の未決の変数の条件と、閉じない制約の形）。

修正は指摘ごとに 1 つずつ commit しました（`9be41866`、`7d5cd869`、`e1070a3d`）。最初の作業は T1 の途中で止まったので、この節と文書は後から確かめ直して仕上げました（上の `review4b/` の結果はそのときのものです）。

確認したこと:

- `cargo fmt --all -- --check` と `cargo clippy --all-targets --locked -- -D warnings`: 成功。
- `cargo nextest run --locked`: 3052 tests run: 3052 passed (25 slow), 4 skipped（2 度目の merge の後の 3049 件に新規の 3 件。`tests/parameter_conventions.rs` は 52 件）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 成功（60 cases。native と WASM の `-O0`・`-O3`）。suite の入力と期待値は変えていません。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs`（`lambda-expressions.md`・`functions.md`）: 成功（2 pages, 59 links, 38 checked examples, 76 native runs (O0/O3)）。`docs/language.md` と `docs/architecture.md` は文章だけの変更です。
- 第1回〜第4回のレビューのプログラム（`target/scratch/review-a32-ce*/`。生成した組み合わせを含みます）と、このチケットの baseline・各回と統合の再現プログラム（`target/scratch/a32/` の下。合わせて `review4/dirs.txt` の 1875 個）を、修正前と修正後の binary で `check --json` し、比べました（`review4/probe.mjs`、結果は `review4b/corpus-pre.json`・`corpus-new.json`・`corpus-diff.txt`）。違いは 15 個で、どれも拒否のまま、位置だけが変わりました。関数ガードの節の `E1012`（`g01`〜`g04`・`g06`・`gd01`〜`gd04`、`g03` は 2 つが 1 つに）と、内側の数値リテラルの `E1003` が注釈の位置へ戻ったもの（`n02`・`n06`・`l02`・`l06`、第2回の `r2_d01`・`r2_d04`。どれも base と同じ位置）です。受理から拒否、拒否から受理に変わったものはありません（`o4` の形はこの集合にありません）。
- `node scripts/check-guide-lite.mjs`: 88 references checked, 0 failed。`node scripts/check-links.mjs`: 0 broken。
- 診断の kind は増やしていません（`E1012 use_after_move` の文言と位置、`E1013 region_callback_stores_exclusive_input` の名前の表示だけ）。`src/runtime/` と std は変えていません。

変更したファイル: `src/ownership.rs`、`src/ownership_control.rs`、`src/closures.rs`、`src/polymorph.rs`、`src/regions.rs`、`tests/parameter_conventions.rs`、`lambda-expressions.md`、`functions.md`、`docs/language.md`、`docs/architecture.md`、このチケット。
