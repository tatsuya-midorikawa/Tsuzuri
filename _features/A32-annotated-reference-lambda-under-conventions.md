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
| ジェネリックなレコード: `if n > 0 then first else Box { value: \(text: ref string) -> text.length * 10 }`（`first = Box { value: text_length }`） | `E1003`（同じ文言） | 束縛した形は `main` では A29 の `E2002`（`redefinition of type`）、A29 の後は `5005` |
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

## 決定事項

### D1: 判定は `src/closures.rs` の `lambda` に置き、パターンの注釈で決める

- 決定: 「設計」のとおり、期待型の規約と引数のパターンの注釈が参照の型であることの両方で決める。
- 理由: 実験で原因の位置は確かめた（「現状」）。生成した名前だけで決めると分解するパターンを変え、注釈付きパターンの側で決めるとラムダの規則が `match` 一般に入る。
- 状態: 既定案（実装者はこの案に従う）。
