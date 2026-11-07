# A32: 規約 `[Read]`・`[Mut]` の関数型が期待される位置へ、引数を `ref T`・`ref mut T` と注釈したラムダを直接書くと `E1003` になる（`if n > 0 then text_length else \(text: ref string) -> …`）

| 項目 | 内容 |
| --- | --- |
| ID | A32 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | A18 |
| 後続 | – |
| 状態 | todo |
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
- **仕様 2（改）**: 最上位が参照の注釈でないパターン（注釈の無い引数、値の型の注釈、tuple・record・構築子）は、今のままにします。可変性の違う注釈（`Read` の位置の `ref mut string`）も `E1003` のままです。文言は `found ref string` になります。
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
  - `Checker::binds_annotated_reference(inner, annotation)`: 注釈が `resolve_type_with_kinds` で参照の型（`ref`・`ref mut`、別名を含む）に解決され、参照先の型 `inner`（解決後）が `Type::Infer`・`Type::Reference`・`Type::Application`・`Type::Partial` のどれでもなく、誤りも含まないときだけ真です。`Checker::annotation` は制約を足すので呼びません。
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
  - 拒否のまま（コードも同じ）: `x09`（`E1003`）、`x15`（`E1014`）、`x16`（`E1003`）、`x16f`（`E1003`）、`x18b`（`E1003`）、`x22`（`E1003`）、2026 の束縛した `if`（`E1029`）。`x10` は `E1003` のまま文言が `found ref string` になりました。`x19c`・`x20` は `E1003` から `E1005`（`only a function value can be called`。束縛した形の `x20b` と同じ）になりました。
  - 受理していた形は値も変わりません（`c01`〜`c03`、`x05`、`x08`、`x16b`〜`x16e`、`x18`、`x19`、`x22b` など）。

### 実行したコマンド（T0〜T2）

- `cargo fmt --all -- --check`: 成功。
- `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo nextest run --locked`: 2985 tests run: 2985 passed, 4 skipped。
- `node tests/features.mjs target/debug/tsuzuri conventions`: 成功（43 cases）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/ownership-and-memory/borrowing.md _tsuzuri/language-reference/values-and-functions/functions.md _tsuzuri/language-reference/values-and-functions/lambda-expressions.md`: 成功（3 pages, 69 links, 46 checked examples, 92 native runs）。

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
