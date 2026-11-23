# NS08: Owned.call の部分適用が Capture 検査をすり抜けて解放後読み出しになる不具合の修正

| 項目 | 内容 |
| --- | --- |
| ID | NS08 |
| 優先度 | P0（安全なコードで非 Copy 値の二重解放・解放後読み出し（heap use-after-free）が起きる健全性不具合） |
| 規模 | S（`Owned.function` / `Owned.call` の多引数カリー化に対する `Capture` 制約の統一、回帰テストと文書） |
| 依存 | – |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-22（[GUIDE D-64](../GUIDE.md#d-64-null-安全性の徹底ns01ns08)、および [A33](../A33-remaining-store-soundness-holes.md) が「格納と破棄の順序のモデルとは別に直す必要がある」と記録した `owned2` の独立チケット化。HEAD `edc61676` で確認） |
| 承認 | 不要（危険なプログラムで起きる解放後読み出しを既存の規則に沿って拒否する不具合修正） |
| 手本にする既存実装 | [A30](A30-owned-function-from-borrowing-value.md)（`Owned.function` の健全性修正）、`src/closures.rs` 31〜36 行（`OwnedFunctionParameterCount`）、`src/polymorph.rs` 3989〜4009 行（`OwnedFunction`・`OwnedCall` のインスタンス検査）と `captures`（部分適用への `Class::Capture` 要求） |
| 主な影響ファイル | `src/polymorph.rs`, `src/check.rs`, `src/ownership.rs`, `tests/owned.rs`（または `tests/drop.rs`）, `tests/features.mjs`, `_features/A33-remaining-store-soundness-holes.md`, `_tsuzuri/language-reference/built-in-types-and-modules/owned.md` |

## 実装者向け早見

- 範囲: 2 引数以上の関数値（例: `def wr :: own Holder -> (unit -> i64) -> i64`）を `Owned.function wr` で包み、`Owned.call (ref w) holder` を呼んで残りの関数 `(unit -> i64) -> i64` を得ると、`Owned.call` 自体は 2 引数の完全適用に見えるため第 2 引数 `holder` に `Capture` 制約が掛からず、かつ `src/llvm.rs` の `apply_value` が `holder` を環境へ格納した再利用可能な関数値を作ってしまう。結果として `Mutex` や `Drop` を持つ非 `Capture` / 非 `Copy` の `Holder` が複数回ムーブ・二重解放される（[A33](A33-remaining-store-soundness-holes.md) の `owned2`）。これを型検査・単相化検査で確実に拒否する。
- 最初に読む: ① 「現状」の再現と原因 ② `src/closures.rs` 31〜36 行 ③ `src/polymorph.rs` 3989〜4015 行と部分適用の `Class::Capture` 検査 ④ `src/llvm.rs` 8373〜8391 行（`Builtin::OwnedCall`）⑤ [A30](_completed/A30-owned-function-from-borrowing-value.md)。
- 最初に書くテスト: `owned_call_returning_function_requires_capture_on_bound_argument`（`Holder` を束縛する形が `E1005` で拒否され、`Arc` や `i64` を束縛する形は通って `live == 0` であること）。
- 触ってはいけない: 1 引数関数（結果が非関数型）を包む通常の `Owned.function` / `Owned.call` の型・所有権・生成 IR。
- 完了の見分け方: [A33](A33-remaining-store-soundness-holes.md) の `owned2` 再現コードが `E1005` で拒否され、ASan 付きの実行テストで use-after-free が一切起きない。

## 目的

Null 安全・メモリ安全の土台である「すべての所有値と参照は常に有効なメモリを指す」という保証において、[A33](A33-remaining-store-soundness-holes.md)（41 行・49 行）で発見された `owned2` は、格納検査モデル（A33 本体）とは独立した `Owned.function` / `Owned.call` の捕捉検査の抜け穴である。A33 は XL 規模の長期リファクタリングであるため、P0 の解放後読み出しである `owned2` を切り出して先行修正する。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief NS08` を読む。承認は不要。

### 停止条件

- **S1** 1 引数の `Owned.function` / `Owned.call` を使う既存のすべてのテスト（`tests/drop.rs`・`tests/rc.rs`・`tests/tzroutine.mjs` など）の挙動や IR が変わる。
- **S2** `Arc` などの `Capture` を満たす値を束縛した `Owned.call`（A33 の `d7_owned_call_arc_moves`）で、`borrowed = true` フラグに起因して環境クローン・解放のカウントが狂う（手順 2 で `live == 0` と ASan で必ず確認する。もし `borrowed = true` が部分適用の寿命と矛盾する場合は、決定事項 D1 の代替案 B「`Owned.function` で結果が関数型になる多引数関数自体をラムダと同じ `E1006` で拒否する」に切り替えて報告する）。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- **ラムダと名前付き関数の非対称**:
  - `src/closures.rs` 31〜36 行は、`Owned.function (\a b -> ...)` のように直接渡したラムダが 2 個以上の引数を持つとき `E1006`（`OwnedFunctionParameterCount`: `"Owned.function takes a lambda with one parameter; take a tuple or return another function"`）で拒否する。
  - しかし名前付き関数 `def wr :: own Holder -> (unit -> i64) -> i64` を `Owned.function wr` に渡すと、ラムダ構文を通らないため `Owned.Function<Holder, (unit -> i64) -> i64>` が作られる。
- **`Type::function` の平坦化と `apply_value`（`src/llvm.rs` 8384〜8390 行）**:
  - `Builtin::OwnedCall` の生成は `let run = Type::function(vec![parameters[1].clone()], ty.after_arguments(2));` を作る。`ty.after_arguments(2)` が関数型 `B -> C` のとき、`Type::function` は引数列を平坦化して `Function([A, B], C)`（2 引数関数）にする。
  - そこへ 1 引数 `%arg1` だけを渡して `apply_value(&closure, &run, Some((&parameters[1], "%arg1")), true)` を呼ぶため、実行時には `%arg1` を捕捉した部分適用クロージャが生成される。
  - さらに `borrowed = true` が渡されるため、元の `w: Owned.Function` が所有する環境を部分適用クロージャが寿命追跡なしに共有しつつ（これは `src/ownership.rs` 3402 行が「2 引数揃った `Owned.call` の結果は `w` を借用しない」と判定してしまうこととも絡む！）、束縛された `%arg1`（非 Copy の `Holder`）には `polymorph.rs` で `Class::Capture` が要求されていないため、返された関数値を 2 回呼ぶか破棄すると `Holder` が解放後に読まれる！

## 変えてはいけない意味

- 結果型 `'b` が関数型でない通常の `Owned.Function<'a, 'b>` と `Owned.call (ref f) a`（2 引数適用で結果が `f` の借用を持たない A30 の規則を含む）。

## 仕様（実装前に最新化）

1. **`Capture` 制約と環境複製の実態に合わせた最新化（D1 改訂）**:
   - `Owned.call :: ref (Owned.Function<'a, 'b>) -> 'a -> 'b` において、結果型 `'b` が関数型 `Type::Function(..)` に解決される場合、束縛される第 2 引数 `'a` に `Class::Capture` 制約を課す（直接呼び出し・パイプライン `x |> Owned.call (ref w)`・過飽和適用・1 引数部分適用・関数値としての使用・ジェネリック単相化の全経路）。これにより、`Mutex`・非 Copy で `Drop` を持つ `Holder`・`Rc`・排他参照などは通常の部分適用と同じ `E1005` で拒否され、`d6_owned_call_moves_bound` と `d4s_owned_call_holder_dropped_first`（`Type::Function` が秘密裏に `Drop` 型を所有して破棄順序検査をすり抜ける穴）の両方が塞がる。
   - **起票時の仮定の再検証**: 起票時は `borrowed = true` により元の `w: Owned.Function` の環境が複製されず共有されると仮定して `ownership.rs` の A30 免除を絞る案を併記していたが、`src/llvm.rs` の `closure_wrappers`（3138〜3274 行）を精査した結果、(a) インライン 1 引数ラムダ（`owned_captures = true`）は常に 1 引数で即座に実行されて独立した新しい関数値を返し（所有捕捉のムーブや借用の返却は `E1012` / `E1013` で既に禁止済み）、(b) 多引数関数値（`owned_captures = false`）は捕捉がすべて `Capture` を満たしかつ A30 で借用を持たず、`count + 1 < arity` の `borrowed = true` 呼び出しでは `@tz.env.clone` で環境をディープコピーした新しい独立環境を確保するため、返り値の関数が `w` の環境を共有することは一切ない。逆に `ownership.rs` で `ref w` の借用を残すと A30 の確定仕様（`tests/tzroutine.rs:366-380` および `owned.md:123-139` の `Owned.drop make; size ()`）を壊して停止条件 S1 に抵触するため、A30 の規則はそのまま維持する（第 2 引数 `'a` が貸す借用と 1 引数部分適用 `Owned.call (ref w)` の `w` 借用は従来どおり保持される）。

## 設計

- **`src/polymorph.rs`（`captures`）**: 型検査末尾と単相化時の両方で走る `captures` において、`Builtin::OwnedCall` の `types[1]`（`'b`）が `Type::Function(..)` であれば `("Capture", types[0].clone(), span)` を要求する（2 引数以上の呼び出しでは第 2 引数の `span`、`x |> Owned.call (ref w)` では `x.span`、それ以外の部分適用・関数値では `Owned.call` の `span`）。
- **`_features/A33-remaining-store-soundness-holes.md`**: 完了時に `owned2` の行へ「NS08 で修正済み」と注記する。

## 前提とする他チケットのインターフェース

- なし（[A30](A30-owned-function-from-borrowing-value.md) の実装の上に立つ）。

## 他チケットへ提供するインターフェース

- [A33](../A33-remaining-store-soundness-holes.md) の範囲から `owned2` を解消済みにする。

## 実装手順

1. **変更**: なし（ブランチ `fix/ns08-owned-call-partial-capture`）。**内容**: `node scripts/repo-kb.mjs brief NS08`、A33 の `d6_owned_call_moves_bound` と `d4s_owned_call_holder_dropped_first` と `d7_owned_call_arc_moves` を手元のテストとして再現確認。
2. **変更**: `src/polymorph.rs`, `src/check.rs`, `src/ownership.rs`。**内容**: `'b` が関数型のときの `'a: Capture` 要求（ジェネリック単相化時を含む）と、`ref w` の借用保持。**確認**: T0、再現ケース 2 つが `E1005` / `E1013` で拒否され、`Arc` ケースが通ること、T1。
3. **変更**: `tests/drop.rs`（または `tests/rc.rs`）、`tests/features.mjs`、`_tsuzuri/language-reference/built-in-types-and-modules/owned.md`、`_features/A33-remaining-store-soundness-holes.md`。**内容**: 回帰テストと E2E（`live == 0`）、`owned.md` の補足記述。**確認**: T2、`node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/owned.md`。

## テスト計画

- Rust テスト:
  - `d6_owned_call_moves_bound`: `Mutex` または `Drop` を持つ非 Copy `Holder` を `Owned.call (ref w) holder` で部分適用すると `E1005`（`Capture` 不足）になること（直接呼び出し・パイプライン `holder |> Owned.call (ref w)`・ジェネリック関数経由の単相化の全経路）。
  - `d4s_owned_call_holder_dropped_first`: 名前付き 2 引数関数（捕捉あり）を `Owned.function` で包み、ブロック内で作った `w` に対する `Owned.call (ref w) 1` の結果関数をブロックの外へ返すと `E1013` になること！
  - `d7_owned_call_arc_moves`: `w` が生存している間に `Arc<Atomic<i64>>` を束縛した `let g = Owned.call (ref w) arc` を 2 回呼び、カウントと結果が正しく `live == 0` であること。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/owned.md`: `Owned.call` の結果がさらに関数を返す場合、渡した引数には通常の部分適用と同じ `Capture` 制約が掛かり、返された関数は元の `Owned.Function` を借用する旨を明記する。

## 受け入れ条件

- [x] `d6`・`d4s`（非 `Capture` 引数の部分適用）が `E1005`、第 2 引数の借用流出と 1 引数部分適用の `w` 先行破棄が `E1013` で静的に拒否される（ジェネリック経由を含む）。
- [x] `d7`（`Arc` 等の `Capture` 値の部分適用、および通常の 1 引数 `Owned.call`）が native / WASM `-O0`・`-O3` で通り、`live == 0`。
- [x] 既存の全テストの動作と IR が変わらない。

## 落とし穴

- ジェネリック関数 `def step :: ref (Owned.Function<'a, 'b>) -> own 'a -> 'b = \w x -> Owned.call w x` を `'b = i64 -> i64` で呼んだ場合や、`let c = Owned.call (ref w)` / `let c = Owned.call` のように関数値経由で呼んだ場合も抜け道にならないよう、`polymorph::captures` で `Builtin::OwnedCall` のすべての出現と単相化後の検査を行う。

## 対象外

- [A33](../A33-remaining-store-soundness-holes.md) 本体の格納・破棄順序モデルの統合。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `Owned.call` の結果型 `'b` が関数型のとき、第 2 引数 `'a` に `Capture` を要求する。A30 の「2 引数の `Owned.call` の結果は `w` を借用しない」規則はそのまま保つ（`closure_wrappers` が多引数環境の捕捉を `@tz.env.clone` で独立環境へ複製するため共有は起きず、`ref w` を借用させると A30 の確定仕様を壊すため） | 通常のカリー化部分適用の規則と一致し、A30 の安全なプログラム（`Owned.drop w` 後の返り値関数呼び出しや `Owned.function` 再包装）を保ったまま `owned2`（`d6`・`d4s`）の解放後読み出しを根絶できるため | 確定・実装済み |

## 実装と検証（2026-11-23）

- `src/polymorph.rs` の `captures` で、`Builtin::OwnedCall` の結果型 `types[1]` が関数型に解決されたすべての箇所（直接呼び出し、パイプライン `x |> Owned.call (ref w)`、過飽和呼び出し、部分適用・関数値、ジェネリック単相化）について `types[0]: Capture` を検査するよう修正した。
- `tests/user_drop.rs`（`owned_call_returning_function_requires_capture_on_bound_argument`）、`tests/fixtures/user_drop/Main.tz`（`owned_curried`: `w` を先に `Owned.drop` した後も `Arc` と `string` を持つ部分適用関数を複数回呼んで `live == 0` になる検査）、`tests/user_drop.mjs`、`tests/user_drop_runtime.c`、`_tsuzuri/language-reference/built-in-types-and-modules/owned.md`、`_features/A33-remaining-store-soundness-holes.md` を更新した。

