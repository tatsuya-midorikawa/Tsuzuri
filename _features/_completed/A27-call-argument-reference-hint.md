# A27: 結果の型が未定の呼び出しを `ref T` の引数へ直接渡すと、結果を `ref T` と推測して `E1003` になる（`id (Owned.call (ref next) input)`）

| 項目 | 内容 |
| --- | --- |
| ID | A27 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | A18, D14 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-05（TZ12 の作業中に `Owned.call` で発見。D14 が既知の制限として記録していたもの。HEAD `5ee6b0f2` で確認・原因を調査） |
| 承認 | 不要（今型エラーになるプログラムを受理するだけで、今受理しているプログラムの型と IR は変えない） |
| 改善する劣位 | 不具合: `id (Owned.call (ref next) input)` が `E1003 expected an integer, found ref i64` になる。edition 2027 では型変数の引数が既定で読み取り（`'a -> 'a` は `ref 'a -> 'a`）なので、ジェネリックな関数の引数に呼び出しを直接書くだけで起きる。明示の `ref` の引数（`String.length (Owned.call …)`、2026 の `def len :: ref string -> i64`）でも起きる。`let` で束縛すると通る |
| 手本にする既存実装 | `src/check.rs` の `coerce_argument`（値の呼び出し結果を一時的に貸す）と `src/constants.rs:280` の `temporary_borrows`。`Inference::is_numeric_literal`（`src/polymorph.rs:570`。「数値リテラルの型は参照にならない」） |
| 主な影響ファイル | `src/check.rs`, `tests/parameter_conventions.rs`, `tests/user_drop.rs`, `tests/borrow_syntax.rs`, `docs/language.md`, `_tsuzuri/language-reference/built-in-types-and-modules/text.md`, `_features/README.md` |

## 目的

呼び出しを引数に直接書く形（`f (g x)`）を、`let` で束縛する形（`let r = g x` と `f r`）と同じ型で受理する。値の呼び出し結果を `ref T` の引数へ渡すと一時的に貸すという
規則（[借用](../../_tsuzuri/language-reference/ownership-and-memory/borrowing.md) の「呼び出し引数の暗黙の借用」）は文書どおりにあるのに、結果の型が未定の呼び出しでは、
型検査が先に「結果は `ref T`」と推測してしまうので、この規則まで届かない。

## 着手条件と停止条件

### 着手条件

- A18・D14 が `_features/README.md` で done であること（HEAD で done）。
- GUIDE §2.3 の基準コマンドが成功し、「再現」と「今の推測に頼って通るプログラム」を実行して、今の結果を記録していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 今受理しているプログラム（`tests/` の全件、`examples/`、「今の推測に頼って通るプログラム」）の型・診断・`--emit llvm` の IR が、修正の前後で変わる。
- 推測を遅らせる位置を決める規則（「設計」の 2）が、`Parallel.*` の引数の分岐や部分適用（`partial_application` の誤りの対応付け）と両立しない。
- 「対象外」の「通り抜け」の形まで直さないと、このチケットのテストが書けない。

## 現状（HEAD `5ee6b0f2` で確認・調査）

### 既知の制限として記録済み

- `docs/language.md:1842`: 結果の型が型変数そのもの（`Maybe.get :: own Maybe<'a> -> 'a`、`Text.trim :: ref 's -> 's`）の呼び出しを `ref T` の仮引数へそのまま渡すと、
  型変数が `ref T` に決まって `E1003` になる。`let` で束縛するか `ref` を明示する。
- `_tsuzuri/language-reference/built-in-types-and-modules/text.md:195`: 同じ制限（`Text.length (Text.trim text)`）。
- [D14](D14-text-class.md) の「見つけた不具合」（589 行）: 原因を `Checker::argument_hint` とし、後続に回した。これまでチケットは無かった。

### 原因

- `src/check.rs:12114` の `call_expression` は、呼び出しが期待型つきの引数のとき（`argument = true`）、**自分の引数を検査する前に**、結果の型を
  `argument_hint(result, expected)` と単一化する（12173〜12183 行）。
- `src/check.rs:13118` の `argument_hint` は、`actual` が未定のとき（13145 行の腕）、期待型をそのまま返す。期待型が `ref T` なら、結果の型変数は `ref T` に決まる。
  引数を 1 つも見ないうちの推測で、単一化なので後から戻らない。
- `src/check.rs:12933` の `argument`（12940 行の `Call` の腕）が、呼び出しの引数を `call_expression(…, Some(expected), argument = true)` に送る。
- edition 2027 の型変数の引数は既定で読み取り（`read_by_default`、4612 行。引数の変換は 8438〜8452 行の `ref 'a`・`Read`）。
- `Owned.function :: ('a -> 'b) -> Owned.Function<'a, 'b>`、`Owned.call :: ref Owned.Function<'a, 'b> -> 'a -> 'b`（3549〜3564 行の scheme）。結果 `'b` は型変数そのものである。
  `Owned.call` の引数の特別扱いは無い（`owned_lambda`、12207・12911 行は `Owned.function` のラムダを先に検査するだけ）。

### 「再現」の推論の順序

1. `\value -> value + 1` の `1` は `integer`（14210 行）で、期待型 `'p` の `Integer<'p>`（既定は `i32`）として `integer_literals` に記録される（14244〜14252 行）。`next : Owned.Function<'p, 'p>`。
2. `id (…)`: ブロックの結果が `i64` を期待するので、`id : ref 'a1 -> 'a1` の `'a1 = i64`、引数は `ref i64`。
3. 内側の `call_expression`（`argument = true`）: 結果 `'d` が未定なので、推測で `'d := ref i64`（12173〜12183 行）。
4. `ref next` が `Owned.Function<'p, 'p>` と `Owned.Function<'c, ref i64>` を単一化し、`'p := ref i64`。
5. 関数の終わりの `Checker::finish`（`src/polymorph.rs:3906`）が `Integer<ref i64>` を検査して失敗し、3951〜3972 行が `1` の位置（4:50）の
   `E1003 expected an integer, found ref i64` に書き換える。

`let r = Owned.call (ref next) input` は期待型が無いので推測が無く、`'d = 'p` のまま `input` が `'p = i64` に決め、`id r` は `place_argument`/`coerce_argument` で局所を貸す。
引数が値（`g :: i64 -> i64`）の非ジェネリックな callee で通るのは、推測が `i64` になるからで、`ref` の引数の非ジェネリックな callee は同じく失敗する。

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64。manifest なし = edition 2027）

```text
def id :: 'a -> 'a = \v -> v
def f :: i64 -> i64
fn f input =
    let next = Owned.function (\value -> value + 1)
    id (Owned.call (ref next) input)
f 41
```

`tsuzuri run` → `Main.tz:4:50: error[E1003]: expected an integer, found ref i64`（`value + 1` の位置）。`let r = Owned.call (ref next) input` と `id r` に分けると `42`。
同じプログラムに `edition = "2026"` の `Tsuzuri.toml` を置くと `42`（2026 の `'a` は値の引数）。

### 同じ原因の形（調査で実行して確かめた）

失敗する:

- `Owned.call (ref next) input` を次へ渡す: `id`（2027）、`Owned.function (\(value: i64) -> …)` か `next : Owned.Function<i64, i64>` と注釈して `id` へ（`expected ref i64, found i64`、`(ref next)` の位置）、
  2026 の `peek :: Copy<'a> => ref 'a -> 'a`、2026 の `len :: ref string -> i64` と 2027 の `String.length`（`expected ref string, found string`）、`id (Owned.call next input)`（`next` の暗黙の借用）。
- `Owned.function` の代わりのラムダ: `id ((\value -> value + 1) input)`、`let g = \value -> value + 1` と `id (g input)`（どちらも `expected an integer, found ref i64`）。
- `id (Array.get 0 xs)`（`expected ref i64, found i64`）。
- `id (apply (\value -> value + 1) input)`、`id (Routine.with_locked_thread (\() -> input + 1))`（`expected ref i64, found i64`）。

通る:

- `Maybe.Some (Owned.call …)`（結果が型変数そのものでない構築子）、ジェネリックなレコードの literal `Box { value: Owned.call … }`、`boxed :: own 'a -> Box<'a>`、
  `Array.sum [Owned.call …]`、`Owned.call (ref next) input |> id`、`id :: own 'a -> 'a`、明示の `id (ref (Owned.call (ref next) input))`。
- edition 2026 で、値の引数のジェネリックな関数へ渡すすべての形。

### 今の推測に頼って通るプログラム（変えてはいけない）

- 2026: `String.length (id s) + s.length` は `id` を通して `s` を借りるので、`s` は move されない（`let t = id s` に分けると `E1012`）。
- 2027: `let r = ref s` と `size (id r)` は `r` を `ref (ref string)` として通す（`let` に分けると `E1005 Copy<string>`）。
- そのため「常に引数を先に検査する」（D14 の案）は、今通るプログラムを壊す。

## 仕様

型の規則を次のように変える。今受理しているプログラムの型と IR は変えない。

1. **数値リテラルは参照にならない**: `argument_hint` で、`actual` が数値リテラルの未定の型（`Inference::is_numeric_literal`）で期待型が `ref T` なら、`T` を返す。
   今この形で `ref` に決まるプログラムは、`finish` で必ず失敗している（参照には `Integer`・`SignedInteger`・浮動小数点のインスタンスが無い）。
2. **推測を遅らせる**: 呼び出しが引数で、期待型が `Reference(..)`、結果が数値リテラルでない未定の型（`Infer`）のとき、自分の引数を検査する前には単一化しない。
   次の時点で決める。
   - 引数を検査する直前に、その引数の**今の解決済みの**仮引数の型が結果を通す（結果の型変数そのもの、またはその `ref`・`ref mut`。保守的には、結果を含む関数の型も含める）とき。
     通常の引数・コールバック・後回しのラムダのそれぞれの前で確かめる。
   - そうでなければ、最後の引数の後。
   - 決めるとき、結果がまだ未定（数値リテラルでない）か参照なら、今と同じく `argument_hint(result, expected)` と単一化する（`partial_application` の誤りの対応付けも同じ）。
     それまでの引数が結果を値に決めていたら単一化しない。最後の `coerce_argument`（12264 行）が一時的に貸す。
3. `Parallel.*` の引数の分岐は今のまま（推測を遅らせない）。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/check.rs` | `argument_hint`（13118 行） | 仕様 1 の分岐を、13145 行の腕の前に足す |
| `src/check.rs` | `call_expression`（12114 行。12173〜12183 行の推測） | 仕様 2 の条件なら推測を「保留」にし、引数の loop の各検査の前と、最後の引数の後で決める helper `settle_argument_hint` を呼ぶ |
| `tests/parameter_conventions.rs` | 新しいテスト | 下の「テスト計画」 |
| `tests/user_drop.rs`・`tests/borrow_syntax.rs` | 新しいテスト | 2026 の形と、変えてはいけない形 |
| `docs/language.md:1842`・`_tsuzuri/language-reference/built-in-types-and-modules/text.md:195` | 既知の制限 | 直した範囲を除き、残る形（「対象外」）だけを書く |

### 仕様 2 で「今の」仮引数を見る理由

`Owned.call` では、`ref next` を検査した後、仮引数 `'a` が閉包の型を通して結果と同じ型変数になりうる。宣言の型ではなく解決済みの型を見れば、その場合は今と同じ時点で推測が入る。

### 健全性

- 単一化を省くのは、結果がすでに値に決まったときだけである。`ref T` と参照でない型の単一化は必ず失敗し、数値リテラルが参照に決まると `finish` で失敗するので、省いて変わるのは今失敗するプログラムだけ。
- 結果が引数を通して貸される場合は、今と同じ時点で推測が入る。
- それより前に検査した引数が結果に触れるのは型引数の中だけで、型に従う強制（`argument`・`coerce_argument`・`callback_adapter`）はそこを見ない。

## 実装手順

### 手順 1: ベースラインと記録

- 変更: なし（`target/scratch/a27/` の program だけ）。
- 内容: GUIDE §2.3 の基準コマンド。「再現」「同じ原因の形」「今の推測に頼って通るプログラム」を書き、2026 と 2027 で結果を記録する。通るものは `--emit llvm` を保存する。
- 確認: 記録が「現状」と一致する。違えば停止する。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/parameter_conventions.rs`（2027。`check(edition, …)`・`ir(…)`、369 行の `generic_read_views_accept_borrowed_scalar_and_callable_temporaries` と 388 行の
  `value_expressions_are_lent_like_operator_temporaries` の近く）、`tests/user_drop.rs`（2026。407〜500 行の `Owned` のテスト）、`tests/borrow_syntax.rs`（2026。140 行の
  `implicit_arguments_preserve_inference_and_owned_parameters` の近く）。
- 内容: 受理のテスト（「再現」、2026 の明示の `ref` の 2 つ、`String.length (Owned.call …)`、`Array.get`、ラムダと局所の閉包の形）は、直接の形の IR が明示の
  `ref (…)` の形の IR と同じであることも確かめる。変えてはいけない形（2026 の `String.length (id s) + s.length`、2027 の `size (id r)`、`peek (id input)`）は、型と IR を固定する。
- 確認: 受理のテストだけが失敗する。

### 手順 3: 数値リテラルの分岐（仕様 1）

- 変更: `src/check.rs` の `argument_hint`。
- 確認: ラムダと局所の閉包の形（`id ((\value -> value + 1) input)`）のテストが成功する。ほかのテストは変わらない。

### 手順 4: 推測を遅らせる（仕様 2）

- 変更: `src/check.rs` の `call_expression`。
- 確認: 手順 2 の受理のテストがすべて成功し、変えてはいけない形のテストも成功する。`cargo test --locked` の全件が成功する。

### 手順 5: 既存のプログラムが変わらないこと

- 確認: `tests/fixtures/`・`examples/` の `--emit llvm` が修正の前後で `cmp` で一致する。手順 1 で保存した、今通るプログラムの IR も一致する。

### 手順 6: ドキュメントと最終確認

- `docs/language.md:1842` と `text.md:195` を、直した範囲（ラムダ・`Owned.call`・`Array.get` など、結果が引数を通らない呼び出し）と残る形（「対象外」）に分けて書き直す。
  `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/text.md`。T2。

## テスト計画

### Rust テスト

- `tests/parameter_conventions.rs`: `call_results_without_a_known_type_are_lent_to_read_parameters`（新規。受理と IR の一致）、`pass_through_calls_keep_their_types`（新規。変えてはいけない形）。
- `tests/user_drop.rs`: `peek (Owned.call (ref add) 1)`・`len (Owned.call (ref add) 1)`（2026）。
- `tests/borrow_syntax.rs`: `String.length (id s) + s.length`（2026。`s` が move されない）。

### E2E

- 「再現」を `tsuzuri run` で native の `-O0`・`-O3`（`42`）。必要なら `tests/features.mjs` の該当 suite に export を足す。

### 既存テストへの影響

- 無い（停止条件）。

### 性能

- 型検査の時間だけ。推測の保留は呼び出しごとに定数の手間しか足さない。

## ドキュメント

- `docs/language.md:1842`: 直した範囲と残る形。
- `_tsuzuri/language-reference/built-in-types-and-modules/text.md:195`: `Text.length (Text.trim text)` は残る形（通り抜け）なので、注意書きは残し、`Owned.call`・ラムダの形が通ることを書き分ける。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`（229〜281 行。呼び出し引数の暗黙の借用）と `owned.md`（86〜113 行。`Owned.call`）: すでに直した後の振る舞いを書いているので、変更不要であることを確かめる。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] 「再現」が 2027 で `42` を返し、直接の形の IR が `id (ref (Owned.call (ref next) input))` の IR と同じ。
- [ ] 「同じ原因の形」の失敗する形のうち、結果が引数を通らないもの（`Owned.call`・ラムダ・局所の閉包・`Array.get`・2026 の明示の `ref`・`String.length`）が受理される。
- [ ] 「今の推測に頼って通るプログラム」の型と IR が変わらない。
- [ ] `tests/fixtures/`・`examples/` の IR が変わらない。
- [ ] `check-docs.mjs`（`text.md`）と T2 が成功する。

## 落とし穴

- 「常に引数を先に検査する」に変えてはいけない（「今の推測に頼って通るプログラム」が壊れる）。
- 宣言の仮引数の型で「結果を通すか」を決めると、`Owned.call` のように、検査の途中で仮引数が結果と同じ型変数になる場合を見落とす。今の解決済みの型で決める。
- 単一化を省いた後の一時的な貸しは `coerce_argument` が作る。省いたのに `coerce_argument` へ届かない経路（部分適用）が無いことを、`partial_application` のテストで確かめる。

## 対象外

- **通り抜け**の形（結果を引数から受け取る呼び出し）: 2027 の `id (id input)`・`id (id 41)`、`Int.abs`、`Maybe.default_value 0 m`、`Array.fold f 0 xs`、`Text.trim s`。
  引数そのもの（すでに参照か、場所か）から決める別の規則が要る（`ref R` の仮引数は引数がすでに参照のときだけ推測する、など）。必要なら別のチケットにする。
- 調査の試作で、関数の型を「結果を通す」に数えない拡張版（`apply`・`Routine.with_locked_thread` も直る）は、全テストの確認が途中で止まった。採るなら別に確かめる。
- `Owned.call (ref mut next) input` が `let` の形でも `expected ref Owned.Function<…>, found ref mut Owned.Function<…>` になること（明示の `ref mut` を `ref` へ強制しない別の振る舞い）。

## 決定事項

### D1: 保守的な規則（関数の型も「結果を通す」に数える）を採る

- 決定: 仕様 2 の「結果を通す」仮引数に、結果を含む関数の型を含める。
- 理由: 調査の試作では、この版で `cargo nextest run --locked --no-fail-fast` の 2934 件のうち、試作の環境変数と一時ディレクトリの競合による 2 件を除いて成功し、
  今通るプログラムの `--emit llvm` は byte 単位で同じだった。拡張版は全件の確認が終わっていない。
- 状態: 既定案（実装者はこの案に従う）。

### D2: 数値リテラルは参照にならない

- 決定: 仕様 1 を、推測を遅らせる規則とは別に入れる。
- 理由: `Inference::is_numeric_literal` の doc comment がすでにこの性質を述べており、今この形で `ref` に決まるプログラムは必ず失敗する。ラムダと局所の閉包の形はこれだけで直る。
- 状態: 既定案。

## 実装と検証（2026-11-06）

### 実装した手順

1. ベースライン: HEAD（`src/` は `5ee6b0f2` と同じ）の dev バイナリで、`tests/fixtures/`・`examples/` の 203 個のビルドできるプログラム（対象は 243。残りは単体ではビルドできない module）と、
   「再現」「同じ原因の形」「今の推測に頼って通るプログラム」と追加の調査用プログラム（計 59 個。`target/scratch/a27/progs`・`progs2`）の `build --emit llvm -O0` と `check --json` を保存した。
   記録は「現状」と一致した（`peek (id input)` だけは、2027 では今も `E1003` の通り抜けの形だった。下の「判断」）。
2. 失敗するテストを先に足した: `tests/parameter_conventions.rs` の `call_results_without_a_known_type_are_lent_to_read_parameters`（2027。8 つの形の IR が明示の `ref (…)` の形の IR と一致）と
   `pass_through_calls_keep_their_types`、`tests/user_drop.rs` の `owned_call_results_are_lent_to_reference_parameters`（2026 の `peek`・`len`）、`tests/borrow_syntax.rs` の
   `implicit_arguments_preserve_inference_and_owned_parameters` への追記（2026 の `String.length (identity s) + s.length`）。旧コードでは受理のテスト 2 件だけが失敗した。
3. 仕様 1: `argument_hint` で、数値リテラルの未定の型と `ref T` の期待型から `T` を返す。ラムダと局所の閉包の形が通り、ほかの保存した出力は byte 単位で同じ。T1: 2937 件中 2935 件成功（失敗は手順 2 の受理のテスト 2 件）。
4. 仕様 2: `call_expression` が、引数で期待型が参照、結果が数値リテラルでない未定の型（局所の型に現れないもの）のとき、推測を `PendingHint` として保留する。
   `settle_argument_hint` が、通常の引数・コールバック・後回しのラムダのそれぞれの検査の直前と、最後の引数の後（`solve_families` の前）で決める。`Parallel.*` の引数の分岐は保留しない。
5. 保存した出力との比較: `tests/fixtures/`・`examples/` と今通るプログラムの `--emit llvm` と `check --json` は、手順 3・4 の後とも `cmp` で一致した。違ったのは、今まで拒否していたプログラム
   （`ann_lambda`・`ann_next`・`array_get`・`implicit_next`・`lambda_direct`・`lambda_local`・`len26`・`peek26`・`repro27`・`strlen26`・`strlen27`、`String.length (Maybe.get m)`、
   2026 の `len (unbox b)`）が通るようになったことと、拒否のままのプログラムの診断の文面（`id (Array.get 0 (Array.get 0 xss))`）だけ。
6. ドキュメントと E2E: `tests/fixtures/conventions` に `call_result_temporaries`（`Owned.call`・ラムダ・`Array.get`・`String.length (Owned.call …)`・`String.length (Maybe.get m)`。期待値 134 は手で数えた値）を足した。

### 判断（チケットを細かくしたもの）

- **「結果を見うる引数」を D1 より広げた。** チケットの規則（仮引数が結果そのもの・その参照・結果を含む関数の型のときだけ先に決める）では、今通る
  `id (Maybe.get (Maybe.Some x))`（2027）と `len (Maybe.get (Maybe.Some s)) + s.length`（2026）が変わる（前者は `x` を借りずに一時値を貸す IR に変わり、後者は `s` が move されて `E1012` になる）。
  呼び出しやリテラルの引数は、期待型の型引数を自分の引数へ渡す（`Maybe.Some x` は `Maybe<ref T>` から `x` を借りる）ので、「設計」の健全性の節の「型引数の中は強制が見ない」は成り立たない。
  停止条件に当たるので、規則を次のように直した: 引数の検査の直前に、(a) 今の解決済みの仮引数が結果を通す（D1。参照を外した型が結果そのものか、結果を含む関数の型）、
  (b) 引数が場所（名前・`Module.f`・フィールド・添字・`deref`）かキーワードの `ref` でなく、仮引数が結果をどこかに含む、(c) 結果の型変数が局所（と `@old` の局所）の型に現れる、のどれかなら決める。
  (b) の場所とキーワードの `ref` は、`argument` が期待型なしで検査して外側の型だけで強制する（`place_argument`・`coerce_argument`）ので、型引数に結果があっても見ない。(c) は、局所を読む引数が結果を見うる経路を塞ぐ。
  `Owned.call (ref next) input` では、`ref next` の後で `next` の型を通して結果が局所に現れるので、`input` の前で決まる（`'p` は数値リテラルの型なので仕様 1 で `i64`）。
- 決めるときに結果が値に決まっていれば単一化しない。未定・参照・`Error`・型構築子の適用（`Application`・`Partial`）なら、今と同じく `argument_hint` と単一化し、失敗は `partial_application` で対応付ける。
  数値リテラルの型なら仕様 1 で参照先の型に決まる（今は必ず失敗する形）。
- 保留の判定は呼び出しの最初でも、結果が局所の型に現れれば保留しない（今と同じ時点で推測する）。
- `pass_through_calls_keep_their_types` の 2027 の `peek (id input)` は今も `E1003` だった（通り抜けの形。「対象外」）。2026 の `peek (id input)`（`input` を借りる）を、明示の `peek (id (ref input))` と同じ IR として固定した。
  2027 の `size (id r)` は綴りで同じ型を書けない（2027 のキーワードの `ref r` は再借用）ので、`id` の特殊化が参照（`ptr`）を返すことと、`let` に分けると `E1005` になることで固定した。
- `Inference::mentions`（`src/polymorph.rs`）を足した。解かれた変数をたどりながら、型を複製せずに未定の変数を探す。

### 実行したコマンドと結果

- `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --test parameter_conventions --test user_drop --test borrow_syntax`: 36・27・13 件成功（手順 4 の後）。
- `cargo nextest run --locked --no-fail-fast`: 手順 3 の後 2937 件中 2935 件成功（失敗 2 件は手順 2 の受理のテスト）、手順 4 とドキュメントの後 2937 件すべて成功（4 件 skip）。
- `node tests/features.mjs target/debug/tsuzuri conventions`: native/WASM の O0/O3 で成功。
- 「再現」: `tsuzuri run` で `42`（2027。2026 も `42`）。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/text.md _tsuzuri/language-reference/ownership-and-memory/borrowing.md`: 2 ページ、14 例、28 回の native 実行が成功。

### 更新した文書

- `docs/language.md`（暗黙の借用の節の既知の制限）: 直した範囲（結果が引数から来ない呼び出し）と、残る通り抜けの形を書き分けた。
- `_tsuzuri/language-reference/built-in-types-and-modules/text.md`（注意点）: `Text.trim`・`Text.clone` の制限は残し、2027 の `String.length (Text.clone text)` が `E1005` であることを正した。結果が引数から来ない呼び出しが通ることを足した。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`（呼び出し引数の暗黙の借用）: 結果の型が未定の呼び出しの一時的な貸しと、残る形を 1 項目で足した。`owned.md` は変更不要だった。

### 変更したファイル

`src/check.rs`、`src/polymorph.rs`、`tests/parameter_conventions.rs`、`tests/user_drop.rs`、`tests/borrow_syntax.rs`、`tests/fixtures/conventions/Main.tz`、`tests/features.mjs`、
`docs/language.md`、`_tsuzuri/language-reference/built-in-types-and-modules/text.md`、`_tsuzuri/language-reference/ownership-and-memory/borrowing.md`、`_features/README.md`、このチケット。
