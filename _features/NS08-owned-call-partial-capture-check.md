# NS08: Owned.call の部分適用が Capture 検査をすり抜けて解放後読み出しになる不具合の修正

| 項目 | 内容 |
| --- | --- |
| ID | NS08 |
| 優先度 | P0（安全なコードで非 Copy 値の二重解放・解放後読み出し（heap use-after-free）が起きる健全性不具合） |
| 規模 | S（`Owned.function` / `Owned.call` の多引数カリー化に対する `Capture` 制約または引数個数検査の統一、回帰テストと文書） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)、および [A33](A33-remaining-store-soundness-holes.md) が「格納と破棄の順序のモデルとは別に直す必要がある」と記録した `owned2` の独立チケット化。HEAD `edc61676` で確認） |
| 承認 | 不要（危険なプログラムで起きる解放後読み出しを既存の規則に沿って拒否する不具合修正） |
| 手本にする既存実装 | [A30](_completed/A30-owned-function-from-borrowing-value.md)（`Owned.function` の健全性修正）、`src/closures.rs` 31〜36 行（`OwnedFunctionParameterCount`）、`src/polymorph.rs` 3989〜4009 行（`OwnedFunction`・`OwnedCall` のインスタンス検査）と `captures`（部分適用への `Class::Capture` 要求） |
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

## 仕様

1. **`Capture` 制約と環境借用の整合（D1）**:
   - `Owned.call :: ref (Owned.Function<'a, 'b>) -> 'a -> 'b` において、結果型 `'b` が関数型 `Type::Function(..)` に解決される場合:
     1. 束縛される第 2 引数 `'a` に `Class::Capture` 制約を課す（`Mutex`・非 Copy で `Drop` を持つ `Holder`・`Rc`・排他参照などは通常の部分適用と同じ `E1005` で拒否される）。
     2. 返される関数値は元の `Owned.Function` の環境（`borrowed = true` で複製されなかった先頭環境）を参照し続けるため、`src/ownership.rs`（3402 行付近の A30 特例「2 引数の `Owned.call` は `f` の借用を残さない」）を **`'b` が関数型でないときだけ**に限定し、`'b` が関数型のときは返り値が第 1 引数 `ref f` の共有借用を保持する（`f` を先に drop してから返り値の関数を呼ぶコードが `E1013` / `E1014` で拒否される！ A33 の `d4s_owned_call_holder_dropped_first` もこれで完全に塞がる！）。

## 設計

- **`src/polymorph.rs`**: `Builtin::OwnedCall` の具体化・制約検査（および `captures` 検査パス）において、`types[1]`（`'b`）を解決した型が `Type::Function(..)` であれば `self.require(Class::Capture, types[0].clone(), span)` を要求する（ジェネリック関数内で `'b` が型変数のまま `Owned.call` された場合も、単相化で具体型 `'b = B -> C` が決まった時点で同じ検査を走らせる）。
- **`src/ownership.rs`（3402 行付近）**: A30 が入れた「`Owned.call` を 2 引数に直接適用した結果は第 1 引数 `ref w` の loan を持たない」という免除に、`!matches!(result_ty, Type::Function(..))` の条件を追加する。これにより `let g = Owned.call (ref w) x` で `g` が関数のときは `g` が `w` の借用を保持し、`w` が先にスコープを抜けて解放される `d4s_owned_call_holder_dropped_first` を `E1013` で確実に防ぐ。
- **`_features/A33-remaining-store-soundness-holes.md`**: 完了時に `owned2` の行へ「NS08 で修正済み」と注記する。

## 前提とする他チケットのインターフェース

- なし（[A30](_completed/A30-owned-function-from-borrowing-value.md) の実装の上に立つ）。

## 他チケットへ提供するインターフェース

- [A33](A33-remaining-store-soundness-holes.md) の範囲から `owned2` を解消済みにする。

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

- [ ] `d6`（非 `Capture` 引数の部分適用）が `E1005`、`d4s`（`w` が先に破棄される部分適用）が `E1013` で静的に拒否される（ジェネリック経由を含む）。
- [ ] `d7`（`Arc` 等の `Capture` 値の部分適用、および通常の 1 引数 `Owned.call`）が native / WASM `-O0`・`-O3` で通り、`live == 0`。
- [ ] 既存の全テストの動作と IR が変わらない。

## 落とし穴

- **2 つの穴の両方を塞ぐこと**: `Holder` に `Capture` を求めるだけでは、第 1 引数が `i64`（`Capture` を満たす）で `w` 自身が所有環境を持つ 2 引数クロージャだった場合に、`let g = { let w = ...; Owned.call (ref w) 1 }` で `w` の解放後に `g ()` が解放済み環境を読む穴（`d4s`）が残る。必ず `src/ownership.rs` 3402 行の借用免除も「結果が非関数型のときだけ」に絞ること！
- ジェネリック関数 `def step :: ref (Owned.Function<'a, 'b>) -> own 'a -> 'b = \w x -> Owned.call w x` を `'b = i64 -> i64` で呼んだ場合も抜け道にならないよう、単相化時の検査と `ownership.rs` の関数要約（A28）の両方で確認する。

## 対象外

- [A33](A33-remaining-store-soundness-holes.md) 本体の格納・破棄順序モデルの統合。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `Owned.call` の結果型が関数型のとき、(1) 第 2 引数に `Capture` を要求し、(2) 結果が第 1 引数 `ref w` の共有借用を保持するようにする（代替案 B「`Owned.function` で結果が関数型になる関数自体を `E1006` にする」は、`d7` のような安全な `Arc`/スカラー束縛を壊すため採らない。ただし停止条件 S2 に当たった場合のみ B を検討） | 通常のカリー化部分適用の規則と一致し、既存の安全なプログラムを最大限保ったまま `d6` と `d4s` の両方の解放後読み出しを根絶できるため | 既定案（実装者はこの案に従う） |
