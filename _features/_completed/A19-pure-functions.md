# A19: 純粋関数 `pure def` — 副作用のない関数を署名で保証する（引数は共有借用か `own` だけ）

| 項目 | 内容 |
| --- | --- |
| ID | A19 |
| 優先度 | P2 |
| 規模 | L（Phase 1 と承認済みの全後続項目） |
| 依存 | [D11](D11-const-evaluation.md)（done。`const def` の修飾子・署名の検査・本体の検査の手本）、[A15](A15-copy-cost-visibility.md)（done。明示コピー `Array.copy`・`List.copy`、`W1006`）、[B07](B07-user-drop.md)（done。利用者の `Drop`）、[F10](F10-concurrency-primitives.md)（done。`Atomic`・`Mutex`・`Channel`）、[C10](C10-shared-ownership.md)（done。`Rc`・`Arc`）。弱い依存: [A17](A17-last-use-move.md)（ループの再束縛と、`own` の引数の最後の使用が複製でなくなると、`pure def` の書き方が実用の速さになる）。[A18](A18-parameter-conventions.md) とは、`own` の構文（A18 の D2）を共有する（D16）。先に着手したほうが `own` の構文を実装し、後のほうは受理する位置の規則だけを足す |
| 後続 | Phase 2（ジェネリックな `pure def`、`Parallel.*` の許可、関数型の純粋性）、[PR01](../../_perfs/_completed/PR01-ownership-facts.md) Phase 2(c)（関数の `memory(...)` に本チケットの解析を再利用）、std の関数への `pure` の付与 |
| 状態 | done（全 Phase。実装・ローカル受入完了、最終 head の CI・レビューは PR で追跡） |
| 起票 | 2026-10-13（人間の指示「`pure` は純粋関数を維持するために `ref` のみを受け取るようにすること。なお、定義するときは `ref` を省略でき、引数はすべて `ref` 扱いにすること」。同日、「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」に改められた。HEAD `cef6c67f` で確認） |
| 承認 | D1・D2・D4 は人間が承認済み（D1・D2: 2026-10-13 の指示。D4: 同日「構文を `pure def` にしてよいです」）。D1 は同日、「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」へ改められた。保証の根拠と、そのために要る型の制限（D15）は「`own` と純粋性」の節にある。承認待ちの決定はなく、手順 1 から着手できる。ほかの決定は既定案 |
| 改善する劣位 | Rust 比・Haskell 比・D 比: 関数が副作用を持たないことを署名で約束できない。`IO<T>` は I/O を型に出すが、`extern`・`Debug` の出力・直接形式の `do!` は型に出ない（[docs/language.md](../../docs/language.md) に「完全なエフェクトシステムではありません」と明記）。`Parallel.*` のコールバックの副作用は検査されず、呼ばれる順序は未規定。D 言語の `pure`（出典は付録 D）に当たる保証がない |
| 手本にする既存実装 | 修飾子と検査: `const def`（チケット D11）の `src/parser.rs`（`constant_function_ahead`・トップレベルの宣言の分岐・`misplaced_const`）、`src/const_rules.rs`（`check_signature`・`validate_function`・`value_part`。`Type` と `TypedExprKind` を wildcard なしで全部判定し、許す型を列挙する）、`src/check.rs` の `names.const_functions`。呼び出しグラフの検査（既定拒否・上限つき）: `src/gpu.rs` の `extract_kernel`（`E1017`）。引数の型の解決と、署名を見る検査の順序: `src/check.rs` の `check_modules` の関数宣言のループ。関数の由来: `src/check.rs` の `FunctionOrigin`（`generated`・`cpu`）。型の述語: `Type::is_copy`・`needs_drop`・`has_user_drop`（`src/check.rs`）、`TypeContext::stored_all`・`reaches`（`src/recursive.rs`）。文脈キーワードの登録: `src/syntax.rs` の `CONTEXTUAL_KEYWORDS`。`own` の前置き: `src/parser.rs` の `type_primary`・`reference_type`（`ref`・`ref mut` の読み方）、`TypeExprKind::Reference` を辿る全出現（`grep -rn "TypeExprKind::Reference" src/`）、[A18](A18-parameter-conventions.md) の手順 3。テスト: `tests/constants.rs`、`tests/call_specialization.rs`（IR の関数の切り出し）、suite `const_functions`（`tests/features.mjs`） |
| 主な影響ファイル | 新規: `src/pure_rules.rs`, `tests/pure_functions.rs`, `tests/fixtures/pure_functions/Main.tz`。変更: `src/syntax.rs`（`SignatureDecl.pure`・`FunctionDecl.pure`・`TypeExprKind::Own`・`CONTEXTUAL_KEYWORDS`）, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`（`FRONTEND_FORMAT` を上げる）, `src/check.rs`（`Names.pure_functions`・`FunctionOrigin.pure`・関数宣言のループ・検証の呼び出し）, `src/polymorph.rs`（`FunctionDecl` の構築箇所と `type_expression`）, `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/regions.rs`, `src/warnings.rs`, `src/api_diff.rs`, `tests/features.mjs`, `docs/language.md`, `docs/stability.md`, `docs/architecture.md`, `README.md`, `_features/GUIDE.md`（§4 のコードマップ）, `_tsuzuri/language-reference/` の複数ページ |

> **現行仕様と設計記録:** この本文には着手前の Phase 1 の制限を歴史として残している。
> 全 Phase 承認後は、通常の `def` の `own`、generic・純粋 callback・class／instance・`@cpu`、
> 閉じた明示並行処理を受理するため、旧 D8・D9・D12・D16 とそれに対応する拒否例を現行の制限とはしない。
> 現行契約は [関数のリファレンス](../../_tsuzuri/language-reference/values-and-functions/functions.md)、
> 実装範囲と検証結果は末尾の「実装と検証」を参照する。

## 目的

関数が副作用を持たないことを、署名に書き、コンパイラが検査する。

```tsuzuri
pure def square_sum :: i64 -> i64 = \n -> Array.sum (Array.init n (\i -> i * i))
```

`pure def` は次の 3 つを保証する。

1. **観測できる副作用がない。** 入出力・OS・ネットワーク・時刻と乱数の源・タスクと並列・原子操作・`Mutex`・`Channel`・参照カウント・`extern`・`Debug` の出力・利用者の `Drop` を、直接も、呼び出す関数を通した間接でも使わない。
2. **引数は共有借用か、所有権の移動（`own`）だけである。** 呼び出し側の値を書き換える手段（`ref mut`、`mut` で変更する引数）はない。書き手は `ref` を省略でき、**`own` を書かない引数はすべて `ref` 扱い**になる（D1・D2。人間の指示）。`own` の引数の型は、呼び出し側の状態への参照を持たず、破棄がメモリの解放だけの型（平たいデータ）に限る（D15。「`own` と純粋性」）。
3. **結果は引数の値だけで決まる。** 同じ値の引数には、同じ結果（またはトラップ）を返す。

保証しないのは、停止すること、トラップしないこと（範囲外の添字、`assert`）、メモリを確保しないことである。「純粋」は「結果が使われないなら呼び出しを消してよい」を意味しない（トラップを保存する開発方針）。

なぜ入れるか:

- 今の言語は、関数の副作用の有無を署名で約束できない。保証があるのは `const def`（コンパイル時に評価できる関数）だけで、通常の関数は `extern` や `Debug.print` を黙って呼べる。
- std の純粋な関数は、すでに「`ref` で受けて、新しい値を返す」形で書かれている（`Array.sort :: Ord<'a> => ref ['a] -> ['a]`、`String.trim :: ref string -> string`、`Array.append :: ref ['a] -> ref ['a] -> ['a]`）。`pure def` は、利用者のコードでその形を `ref` なしで書けるようにし、書いたとおりの純粋性を検査する。
- 並列・GPU・メモ化・共通部分式の除去・LLVM の `memory(...)` は、純粋性の保証を前提にする。本チケットは**保証と解析だけ**を入れ、最適化には使わない（Phase 2 以降）。

「引数は共有借用（`ref`。省略でき、既定）か `own` だけ」は人間の指示である（当初は `ref` のみ。同日、「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」に改められた）。Phase 1 はこの規則どおりに実装し、`own` が純粋性を保つ根拠を次の節に示す。

**型の上の表し方（D3）。** `pure def f :: string -> [i64] -> i64` は、`string` と `[i64]` を共有借用 `ref string`・`ref [i64]` で受け取る。数値・`bool`・`char`・小さな値の record など、Copy で drop が要らない型は、読むだけなら共有借用と値渡しを区別できない（呼び出し側の値は変わらず、複製の費用もない）ので、型の上では値のままにする。HEAD では `ref i64` が `a + b` にも戻り値にも使えず `E1003` になるため、文字どおり「スカラーも `ref i64`」にすると、最も基本的な純粋関数が書けなくなる（「現状」の表の 1・2）。所有権を受け取る引数は `own` と書く（`pure def f :: own [i64] -> i64 -> [i64]`）。`own` の引数は値渡しで、`def` の無印の引数（`2026`）と同じである。

### `own` と純粋性（D1 の根拠）

人間の指示は「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」である。結論は、**`own` の型を平たいデータに限る制限（D15）を付ければ保証される**。制限なしでは保証されない（下の表の 3・4。HEAD で壊れることを実測した）。

`pure def` の 2 つの性質（観測できる副作用がない・結果は引数の値だけで決まる）を、`own` の引数が壊しうる経路を、(a) 値を渡す操作そのものと、(b) 値が保持するものの種類、に分けて調べた。(b) は、型の構成子（`Type` の全 variant）を 1 つずつ判断し、許す型を wildcard なしの `match` で列挙する（D15。`src/const_rules.rs` の `value_part` と同じ書き方で、新しい variant が足されると判断を強いられる）。

| # | 経路 | HEAD の実測（付録 E） | 結論 |
| --- | --- | --- | --- |
| 1 | (a) 呼び出し側の値が奪われて、観測される | 非 Copy の値は移動で、移動後の変数は読めない（文字列を値渡しで消費したあとの `s.length` は `E1012`。e3）。Copy の値（配列・リスト）は、呼び出し側が後で読むなら複製して渡り、読まない最後の使用だけが移動になる | 保たれる |
| 2 | (a) 呼び出し先が、呼び出し側から見える値を書き換える | 値渡しの引数は唯一の所有者が持つ値で、別名がない（借用中は移動できない: `E1014`。e6）。Copy の配列を値渡しで受けて `Array.update_at` で書き換えても、呼び出し側の配列は変わらない（`bump a` のあとで `a[0]` は 1、結果の `[0]` は 99。e2） | 保たれる |
| 3 | (b) 値が、呼び出し側の変数への参照を保持する | `record Counter {r} { value: ref mut {r} i64, step: i64 }` を**値渡し**で受けた関数が、`deref c.value = deref c.value + c.step` で呼び出し側の変数を書き換える（`total` が 1 から 21 になり、出力は 41。e7）。`ref Counter` で受けると `E1014`（e8） | **壊れる。型で塞ぐ（D15）** |
| 4 | (b) 値の破棄が、メモリの解放以外を起こす | 利用者の `Drop` は、値渡しで消費すると呼び出し先の中で `drop` が走る（`drop 7` の出力が `before` と `after` の間に出る。e4）。`Rc` は、値渡しで消費して捨てると、呼び出し側の `Rc.strong_count` が 2 から 1 に変わる（e1）。`ref Rc` なら変わらない（e5）。`extern` のハンドルは未確認 | **壊れる。型で塞ぐ（D15）** |
| 5 | (b) 関数値が外から入る | 関数・`dyn`・`Task`（`Seq`・`IO`・`Async` の関数の field を含む）は、規則 4 が `ref` と `own` の両方で拒否する | 保たれる |

D15 の制限: `own` の引数の型が、保持するすべての型（record・union の field と payload、配列・リスト・`Vec`・固定長配列の要素、tuple の成分まで）で、次だけを許す。数値（整数・浮動小数点・decimal・SIMD）、`bool`、`char`、`utf8char`、`unit`、`string`、`utf8string`、配列・リスト・`Vec`・固定長配列・tuple、利用者の `Drop` を持たない record・union。許さないのは、参照（排他借用を保持する record を含む）、`Rc`・`Arc` と弱参照、利用者の `Drop` を持つ型、`extern` のハンドル、`Task`、`dyn`、関数（`Owned.Function` を含む）。違反は署名の検査で `E1029`。この制限があれば、`own` の引数は、`ref` だけの場合と同じ 2 つの性質を保つ。本体の検査（効果の源の走査）に新しい効果の源は要らない（破棄と書き換えの経路は、入口の型の制限で塞ぐ）。

保証が頼るのは、所有権検査の規則（`E1012`・`E1014`）が今のままであることと、借用検査の規則（共有参照を通した排他借用フィールドの変更は `E1014`）である。複製と移動の最適化（A17）は値の意味を変えない（A17 の差分テストが守る）ので、保証に影響しない。

`ref` の引数に `Rc` や排他借用を持つ型を渡すのは、今までどおり許す（`ref` は借用なので破棄されず、排他借用の先は共有参照を通して変更できない。使うには不純な組み込み関数が要るので、本体の規則で拒否される）。`own` が許すのは平たいデータの移動だけで、`ref mut` と `mut` は許さない（呼び出し側の値を共有したまま書き換える手段になる）。

## 着手条件と停止条件

### 着手条件

- **承認の記録。** D1・D2・D4 は 2026-10-13 に人間が承認した（本チケットの「決定事項」と、`_features/README.md` の「追加チケット（2026-10-13）」の表の承認欄）。D1 の `own` は、「`own` でも純粋性が保たれることが保証されるのであれば許容する」という条件つきの承認で、保証の根拠は「`own` と純粋性」の節と付録 E の実測である。着手時の HEAD で付録 E を再現してから進める。承認待ちの決定はない。承認の記録と食い違う変更（たとえば `def pure` の語順への変更）が必要になったら、着手せず報告する。
- GUIDE §2.3 の基準コマンドが成功する状態から始める。`const def` の実装（`src/const_rules.rs`・`src/constants.rs`・`tests/constants.rs`）と、GUIDE の §6.7（診断コード）・§6.9（走査箇所）・§11.1 を読む。
- 手順 1 のとおり、変更前のコンパイラと IR を残す。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `pure` を書かないプログラムの受理・拒否・診断・生成 IR が 1 つでも変わった（`pure` という名前の識別子を使うプログラムを含む）。
- 検査のために、型検査・所有権検査・特殊化・`closures::lower` の意味を変える必要が出た。検査は型付き IR を読むだけの解析でなければならない。
- 主要な std の関数（`Array.map`・`Array.fold`・`Array.sum`・`Array.sort`・`String.trim`・`String.split`・`List.fold`・`Maybe`・`Result` の操作・`Json.encode`）が、想定外に不純と判定され、`pure def` から呼べない。**判定を緩めて通さず**、到達した効果の源（連鎖）を添えて報告する。
- `Builtin`・`TypedExprKind` の既存の variant で、本チケットの分類（表）が誤りに見えるものがある。
- D2 を「スカラーも型として `ref i64` にする」と読むよう、人間が求めた（`ref` の算術と戻り値の自動参照外しが要り、本チケットの範囲を超える。D3）。
- `own` の引数を通して、呼び出し側から観測できる副作用の経路で、D15 の型の制限では塞げないものが見つかった（D1 の条件である保証が崩れる。`own` を許さない形へ戻すか、制限を足すかを、人間が決める）。
- `own` の構文を `pure def` の引数に限ることが、実装上できない、または A18 の手順 3 と衝突する（D16）。
- `@alias pure` を使う既存のコードが見つかった（手順 2 の `CONTEXTUAL_KEYWORDS` への追加が、そのコードを拒否に変える）。
- 新しい予約語、`unsafe`、新しい crate、新しい CLI が必要になった。
- 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状と計測（HEAD `cef6c67f`）

### 純粋性の現状

- `IO<T>` は遅延アクションで、作る・合成するだけでは何も実行されない（[docs/language.md](../../docs/language.md)「IO と標準入出力」）。ただし同書に「完全なエフェクトシステムではありません」とあり、次は型に現れない: `extern` の呼び出し、`Debug.print` の出力、戻り値型にビルダーのない関数の `do!`・`let!`（直接形式）。付録 B の `r_io`・`r_debug`・`r_extern` は、HEAD で通常の `def` として受理される。
- `Parallel.*` はデータ競合を `Send`・`Sync` と所有権検査で防ぐ。コールバックの副作用は検査せず、「チャンク間における並列処理の開始順序および完了順序は未規定」（同書）。
- 純粋性の保証がある関数は `const def` だけである。

### `const def`: すでにある厳しい純粋サブセット

`const def` は、署名が具体的な値型だけ（参照・関数型・`Task`・リスト・`Vec`・`dyn`・`Rc`・利用者の `Drop` を持つ型は `E1026`）、本体は `const def` だけを呼べ、std・`extern`・クロージャ・借用は使えない。
実行時には通常の関数でもある。検査は `src/const_rules.rs` が `Type` と `TypedExprKind` の全 variant を wildcard なしで判定するので、新しい variant を足すと判断を強いられる。`pure def` はこの形を手本にし、規則を緩める（`ref` の引数、std の呼び出し、クロージャ、確保を許す）。

### 無印の引数（共有借用）で何が書けるか（HEAD の実測）

`pure def` の無印の引数は、明示の `ref` と同じ型になる（D3 の例外を除く）。次は、`ref` を明示した通常の `def` で HEAD に書いた結果である（付録 A の再現プログラム）。

| # | 書いたもの | HEAD の結果 | `pure def` では |
| --- | --- | --- | --- |
| 1 | `def add :: ref i64 -> ref i64 -> i64 = \a b -> a + b` | `E1003` expected i64, found ref i64 | スカラーは値のまま（D3） |
| 2 | `def identity :: ref i64 -> i64 = \x -> x` | `E1003` | 同上 |
| 3 | 文字列の `a + b`（`a`・`b` は `ref string`） | `E1003`（`+` は消費する） | `$"{a}{b}"`（補間）と書く |
| 4 | `$"hello {a} and {b}"`（`ref string`） | 受理 | 受理 |
| 5 | `Array.map (\x -> x * 2) xs`（`xs: ref [i64]`） | 受理 | 受理 |
| 6 | `Array.update_at 0 v xs`（`xs: ref [i64]`） | 受理（引数の位置で暗黙の複製。元の配列は変わらない） | 受理 |
| 7 | `def same :: ref [i64] -> [i64] = \xs -> xs` | `E1003`（戻り値に `ref` は使えない） | 返すなら `own [i64]` で受ける（`pure def same :: own [i64] -> [i64]`）か、`Array.copy xs` で複製する |
| 8 | `def same :: ref string -> string = \s -> s` | `E1003` | 返すなら `own string` で受けるか、`clone_string s` で複製する |
| 9 | record の field の読み（`p.n`、`p.name.length`） | 受理 | 受理 |
| 10 | 非 Copy の field を返す `p.name` | `E1012` cannot move a non-Copy value out of a reference | `clone_string (ref p.name)` |
| 11 | `{ p with name = s }` | `E1012` | `Named { name: clone_string s, n: p.n }` と構築する |
| 12 | `match m with ...`（`m: ref Maybe<[i64]>`） | 受理（束縛は借用） | 受理 |
| 13 | `ref` の引数を捕捉するラムダを `Array.fold` に渡す | 受理 | 受理 |
| 14 | `def apply :: ref (i64 -> i64) -> i64 -> i64 = \f x -> f x` | `E1005` only a function value can be called | 関数型の引数は持てない（D8） |
| 15 | `export def total :: ref [i64] -> i64 = \xs -> Array.sum xs` | 受理（値渡しの `[i64]` は `E1008`） | `export pure def total :: [i64] -> i64` と書ける |
| 16 | `let mut ys = Array.copy xs` と、ループの `ys = Array.update_at .. ys` | 受理（HEAD は各反復で複製する。A17 の対象） | 受理 |

`Array.sort` は要素が Copy でなければ `E1005`（no instance for Copy<..>）になる。`string` を持つ record の配列は、並べ替える前に別の形にする。

### 蓄積引数の再帰（`ref` で受けるか、`own` で受けるか）

引数を蓄積しながら再帰する形（`go acc xs`）で、`acc` を `ref` で受けると、`acc` は移動できないので毎回複製することになる。`own` で受ければ、所有する値をそのまま次の呼び出しへ移せる（`own` を許す。D1）。HEAD の測定（Apple M1 Max、`-O3`、n = 40,000、各 3 回。付録 C）:

| 形 | 時間 | 備考 |
| --- | --- | --- |
| `def rec build :: i64 -> [\|i64\|] -> [\|i64\|]`（所有引数 `acc`、`List.cons n acc`。`pure def` では `own [\|i64\|]`） | 14.9 / 14.5 / 14.6 s | 分岐の両腕で `acc` を読むので、HEAD は毎回複製する（A17 の対象）。A17 の後は線形になる見込み（未測定） |
| 同じ形で `acc` を `ref [\|i64\|]` にし、`List.copy acc` を明示 | 20.1 / 18.5 / 18.0 s | 借用した `acc` は移動できないので、A17 の後も二乗のまま |
| ローカル可変のループ `acc = List.cons i acc`（`pure def` の中で書ける形） | 15.4 / 14.6 / 14.6 s | ループの再束縛は A17 の対象。A17 の後は線形になる見込み（未測定。`Array.update_at` の形は A17 の試作で 4.0 s → 0.02 s） |

HEAD では、`own` の形（所有引数）も `ref` の形も二乗である（`own` の形は A17 の対象）。`pure def` で蓄積する引数は `own` で受けるか、ローカル可変のループにする。`ref` で受けて複製する形は、A17 の後も二乗のままなので避ける。

### 効果の源（型付き IR と組み込み関数）

- 型付き IR（`TypedExprKind`）で構造的に効果を持つのは `HostCall`（`extern`）、`TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel`、`DynDispatch`。呼び出しは `Call` の子の `Function(FunctionRef)`・`Closure(id, ..)` として現れる。
- 残りは `Function(FunctionRef::Builtin(..))` の `Builtin`（`Builtin::ALL` は 246 個）。入出力・OS・ネットワーク・時刻と乱数・タスク・非同期・原子操作・`Mutex`・`Channel`・`Rc`・`Arc`・`Arena`・GPU・`dyn` に当たる 90 個が不純で、残りの 156 個（数値・文字列・配列・リスト・`Vec`・SIMD・`Assert` など）が純粋である（「組み込み関数の分類」）。
- `TypedExpr::children` は末尾に `_ => Vec::new()` を持つ非網羅的な走査である。**走査には使うが、分類には使わない**（分類は wildcard なしの `match`）。構造的な比較・hash・表示・補間のノードは、要素ごとのメソッドを子の式として持つので、走査で拾える。
- std の主要なモジュール（`Array`・`List`・`String`・`Utf8String`・`Vec`・`Map`・`Set`・`Math`・`Char`・`Json`・`Cbor`・`Format`・`Unicode`・`Path`・`Tensor`・`Regex`・`BigInt`・`Seq`）のソースは、不純なモジュール（`IO`・`Os`・`Net`・`Debug`・`Rc`・`Arc`・`Atomic`・`Mutex` など）を 1 件も参照しない（HEAD で grep した）。不純なのは、`HashMap.randomized`・`HashSet.randomized`・`Random.bytes`・`Random.next_u64`（OS から seed を読む）と、`Matrix` の乗算（`Parallel`・`Arc` を使う）。到達しなければ検査に影響しない。

### `constant` フラグが通る場所（新しい修飾子の変更箇所）

`const def` の印は、`src/syntax.rs` の `SignatureDecl.constant`・`FunctionDecl.constant`、`src/parser.rs`（トップレベルの宣言の分岐・`signature.constant`・`FunctionDecl` の構築 3 箇所）、`src/syntax_codec.rs`（`put`・`get` 各 2 箇所）、`src/check.rs`（`const_names`・`names.const_functions`・外部関数と定数の `FunctionDecl` の構築）、`src/polymorph.rs`（`FunctionDecl` の構築）、`src/docgen.rs`・`src/semantic.rs`（表示）、`src/api_diff.rs`（互換性）を通る。新しい `pure` の印は同じ場所に足す。構造体のリテラルはすべてのフィールドを要求するので、`pure` を足すと、足し忘れた箇所はコンパイルエラーになる。

## 変えてはいけない意味

- `pure` と `own` を書かないプログラムの受理・拒否・診断・生成 IR（`--emit llvm`）は 1 byte も変わらない。`pure`・`own` は文脈キーワードなので、`pure`・`own` という名前の変数・関数・フィールドは今までどおり使える。
- `pure def` の実行時の意味と生成 IR は、引数に `ref` を明示した（`own` の引数は値渡しにした）通常の `def` と同じである（関数名を除く）。検査と型の補完だけを足し、生成は変えない。
- 型検査・所有権検査・借用検査の規則は変えない。検査は、型付き IR を読むだけの解析で、`pure def` 以外の関数の解釈を変えない。
- `const def` は従来どおり（`E1026`）。`const def` から `pure def` は呼べない。
- 同じ入力で生成 IR は決定的である（集合は `BTreeSet`・`BTreeMap`）。

## 仕様

### 構文

```text
declaration    ::= [doc] [cpu-attribute] ["private"] ["export"] [("const" | "pure")] "def" ["rec"] name "::" signature
parameter-type ::= ["own"] type          // pure def の署名の引数の位置だけ（D16）
```

- `pure` は `def` の直前にだけ書く**文脈キーワード**（`const` と同じ位置。D4）。`pure` の次が `def` でなければ、ふつうの識別子である。予約語は増えず、すべての edition で受理する（追加のみの変更）。
- `pure def` の `fn`・`and`・`let` による実装は、署名の `pure` を引き継ぐ。`pure def rec f :: .. and g :: ..` は、`and` の署名も `pure` である（`const def rec` と同じ）。
- 位置の誤りは `E0002`: `pure fn f ..`・`pure let ..`、`pure export def`（`export pure def` と書く）、`const pure def`・`pure const def`（`const def` はすでに純粋）、`@cpu` と `pure def` の併用（D12）。`private pure def`・`export pure def` は書ける。
- `class` のメソッド・`instance` のメソッド・ラムダ・ローカルの関数には書けない（その位置では `pure` はふつうの識別子で、後ろの構文の誤りになる）。
- `pure` は `CONTEXTUAL_KEYWORDS` に登録する（`@alias pure` を拒否し、LSP の補完に出す）。
- `own T` は、`pure def` の署名の引数の型の前だけに書ける。意味は「所有権を受け取る」で、`2026` の通常の `def` の無印の `T` と同じである（Copy の型では複製、後で読まない最後の使用では移動）。
  `own` は予約語にしない。型の位置の `own` の直後に型が続くときだけ前置きとして読み、それ以外の位置・名前としての `own` は変えない（`EDITION_KEYWORDS` は空のまま）。`own` の構文は A18 の D2 と同じ設計で、受理する位置だけが違う（D16）。
- `own T` を `pure def` の引数の位置以外（通常の `def` の引数、結果の型、`let` の注釈、フィールド、関数型の中、型引数）に書くと `E1005`（メッセージ案: `'own' is only valid on the parameters of a 'pure def'`）。A18 が、すべての `def` の引数へ受理を広げる。

### 署名の規則（`E1029`）

| # | 規則 | 違反の例 |
| --- | --- | --- |
| 1 | 署名は具体的で、型変数と制約を持たない（Phase 2 でジェネリックに。D8） | `pure def f :: ['a] -> i64` |
| 2 | `mut` を付けた引数（`Parameter.mutable`）は書けない（`own` の引数にも）。変更するなら、`own` で受けた値を本体で `let mut` に束縛する | `mut` の引数 |
| 3 | `ref mut` の引数は書けない | `pure def sort_it :: ref mut [i64..] -> unit` |
| 4 | 引数の型が、関数・`dyn`・`Task`・`Owned.function` を、保持する値や参照の先も含めて持たない（D8）。`ref` の引数にも `own` の引数にもかかる | `pure def f :: (i64 -> i64) -> i64`、関数の field を持つ record の引数 |
| 5 | `own` の引数の型は平たいデータ: 保持するすべての型（record・union の field と payload、配列・リスト・`Vec`・固定長配列の要素、tuple の成分まで）が、数値・`bool`・`char`・`utf8char`・`unit`・`string`・`utf8string`・配列・リスト・`Vec`・固定長配列・tuple・利用者の `Drop` を持たない record／union のどれかである。参照（排他借用を保持する record を含む）、`Rc`・`Arc`・弱参照、利用者の `Drop`、`extern` のハンドル、`Task`、`dyn`、関数（`Owned.Function` を含む）は不可（D15） | `pure def f :: own Ticket -> i64`（`Ticket` が利用者の `Drop`）、`pure def f :: own Rc<string> -> i64`、`pure def f :: own Counter -> i64`（`Counter` が `ref mut` の field を持つ record） |
| 6 | `own` を参照型に付けない | `pure def f :: own ref string -> i64` |

明示の `ref T`・`ref {r} T` は書ける。`own T` は、`T` が Copy で drop が要らない型（数値など）でも書ける（値のままで、`own` の印を外すだけ）。`export` では公開 ABI の検査（`E1008`）が補完後の型にかかる（値渡しのバッファ、つまり `own [i64]`・`own string` は公開できず `E1008`）。

### 引数の補完

署名の型を解決し、規則 2〜6 を調べたあとで、次の規則で型を補う。edition に依らない。

| 書いた型 | 結果の型 |
| --- | --- |
| `ref T`・`ref {r} T` | そのまま |
| `own T` | `T`（値渡しにする。`own` の印を外すだけ） |
| `T` で、`T` が Copy で drop が要らない（数値・`bool`・`char`・`unit`・SIMD、そのような要素だけの tuple・record・union・固定長配列） | `T`（値のまま） |
| `T` で、上以外（`string`・`utf8string`・`[U]`・`[\|U\|]`・`Vec<U>`・drop の要る field を持つ record・union・`extern type` のハンドル・`Rc`・`Task` など） | `ref T` |

判定は、解決後の型に対する `is_copy(types) && !needs_drop(types)` である。

```rust
/// A19: a `pure def` parameter is a shared borrow unless it is written `own` or is a plain Copy value.
fn pure_parameter(ty: Type, owned: bool, types: &TypeContext<'_>) -> Type {
    if owned
        || matches!(ty, Type::Reference(..))
        || (ty.is_copy(types) && !ty.needs_drop(types))
    {
        ty
    } else {
        Type::Reference(Box::new(ty), false)
    }
}
```

### 本体の規則: 効果の源

型付き IR（特殊化と `closures::lower` のあと）で、`pure def` から到達できる関数の本体に、次が 1 つでもあれば拒否する。

| 分類 | 何が当たるか | 判定 |
| --- | --- | --- |
| 外界 | `HostCall`（`extern`）。`IOReadLine`・`IOWrite`・`Os*`・`Net*`。`DebugPrintString`。`BenchNow`・`BenchConsume`・`GenSeed` | 拒否 |
| 並行 | `TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel` の式。`Task*`・`TaskScope`・`Parallel*`・`Async*` | 拒否（D9） |
| 共有可変状態 | `Atomic*`・`Mutex*`・`Channel*`・`Rc*`・`Arc*`・`ArenaNextId` | 拒否 |
| GPU | `Gpu*` | 拒否 |
| 動的ディスパッチ | `DynOf`・`DynDispatch` | 拒否 |
| 利用者の `Drop` | 型が利用者の `Drop` を持つ値（record・union とそれを含む型。参照の先は含まない）を持つ式 | 拒否（D10） |
| 解決されていない式 | `Error`・`GenericFunction`・`Method`・`TypeFunction`・`Lambda`・`CaseConstructor` | 拒否（既定拒否。到達する本体には残らない） |
| 関数の参照 | `Function(FunctionRef::User(id))`・`Closure(id, ..)` | 参照先 `id` の本体も同じ規則で調べる。呼び出しでなくても「呼べる」とみなす |
| 組み込み関数の参照 | `Function(FunctionRef::Builtin(..))` | 「組み込み関数の分類」の判定に従う |
| そのほか | 算術・比較・`if`・`match`・ループ・record／tuple／配列／リストの構築と読み出し・借用・`Assign`（ローカルの更新）・`try`・`Checked`・`Static`・構造的な比較・hash・表示・補間・確保・トラップ・無限再帰 | 許す |

関数値の呼び出し（`Call` の callee が `Local` など）は許す。純粋関数の実行中に現れる関数値は、すべて**到達する関数の本体で作られた**もの（`Function`・`Closure` の参照）だけで、その参照をすべて調べるからである。外から関数値が入る道は、規則 4（引数）が塞ぐ。したがって、コールバックを取る std の関数（`Array.map`・`Array.fold` など）は、純粋なラムダを渡したときだけ受理される。不純なラムダを渡すと、そのラムダの本体で拒否される。

### 診断 `E1029`

新しい診断コードを 1 つ使う（D5。GUIDE の D-16 の表に追記済み）。メッセージは英語、小文字始まり、直し方を含める。

- 署名（位置は該当する引数の名前、規則 1 は関数名）:
  - `a pure def must have a concrete signature; remove type variables and constraints`
  - `a pure def parameter cannot be 'mut'; bind a mutable local in the body instead`
  - `a pure def takes only shared references; take 'x' with 'ref' and return the new value`（`ref mut`）
  - `a pure def cannot take a value that holds a function, 'dyn', or a task ('T'); pass the data it needs instead`
  - `an 'own' parameter of a pure def must be plain data; '{type}' holds {what}; take it with 'ref'`（規則 5。最初に違反した型で `{what}` を決める: `a reference`（排他借用を保持する record を含む）・`an 'Rc' or 'Arc'`・`a value with a user Drop`・`an extern handle`・`a function, 'dyn', or a task`。ほかは `a type that is not plain data`）
  - `'own' cannot apply to a reference; write 'ref T' or 'own T'`（規則 6）
  - `own` を `pure def` の引数以外に書いた誤りは `E1005`: `'own' is only valid on the parameters of a 'pure def'`
- 本体（位置は下の規則）:
  - 式そのものが効果の源: `this operation is not allowed in a pure def: it {reason}; move it to a def`
  - 呼び出し先が効果に到達する: `'{callee}' is not pure: it {reason}{chain}; call a pure function or move this call to a def`。`{chain}` は ` (through 'A' -> 'B')` の形で、途中の関数を最大 4 つまで示す。
- `{reason}`: `calls the extern function '{name}'`（`HostCall`）／`reads or writes the outside world ('{builtin}')`（`IO`・`Os`・`Net`）／`prints debug output`／`reads the clock, a random source, or a benchmark counter ('{builtin}')`／`starts or waits for a task or a parallel operation ('{builtin}')`／`uses shared mutable state ('{builtin}')`／`uses the GPU ('{builtin}')`／`uses dyn dispatch`／`holds a value whose type has a user Drop ('{type}')`／`cannot be proven pure`（解決されていない式）。
- **位置**: 効果の源が `pure def` 自身か、その中のラムダ（`origin` の `parent` をたどって `pure def` に着く関数）にあるときは、その式の位置。ほかの関数（std・別のモジュール・同じモジュールの通常の `def`）で見つかったときは、`pure def` 側の最初の参照の式の位置（std や別ファイルの位置を指さない）。
- 1 つの `pure def` につき診断は最初の 1 件（ソース順）。複数の `pure def` は、それぞれ報告する。
- 到達する関数・式が上限を超えたら `E1017`（決定 D11）。`pure def` を分けるよう促す。

### 例

受理される（本体は、`pure` を外し、無印の引数に `ref` を明示し、`own` を外した通常の `def` として、HEAD で `check` と実行を確認済み。付録 B・fixture）:

```tsuzuri
record Point { x: i64, y: i64 } deriving (Eq, Ord)
record Named { name: string, n: i64 }

pure def add :: i64 -> i64 -> i64 = \a b -> a + b
pure def greet :: string -> string -> string = \a b -> $"hello {a} and {b}"
pure def doubled :: [i64] -> [i64] = \xs -> Array.map (\x -> x * 2) xs
pure def with_first :: [i64] -> i64 -> [i64] = \xs v -> Array.update_at 0 v xs
pure def with_first_own :: own [i64] -> i64 -> [i64] = \xs v -> Array.update_at 0 v xs
pure def first_or :: Maybe<[i64]> -> i64 = \m ->
    match m with
    | Maybe.Some xs -> xs[0]
    | Maybe.None -> 0
pure def pick :: [i64] -> [i64] -> i64 = \idx data -> Array.fold (\acc i -> acc + data[i]) 0 idx
pure def pick_own :: own string -> own string -> bool -> string = \a b first -> if first then a else b
pure def shout :: own string -> string = \s -> $"{s}!"
pure def rec build_list :: i64 -> own [|i64|] -> [|i64|] = \n acc ->
    if n == 0 then acc else build_list (n - 1) (List.cons n acc)
pure def name_of :: Named -> string = \p -> clone_string (ref p.name)
pure def renamed :: Named -> string -> Named = \p s -> Named { name: clone_string s, n: p.n }
pure def largest_x :: [Point] -> i64 = \points ->
    let sorted = Array.sort points
    if sorted.length == 0 then 0 else (Array.at (sorted.length - 1) (ref sorted)).x
pure def rec fact :: i64 -> i64 = \n -> if n <= 1 then 1 else n * fact (n - 1)
export pure def total :: [i64] -> i64 = \xs -> Array.sum xs
```

拒否される（付録 B の本体は、`pure` なしの通常の `def` として HEAD で受理される）:

```tsuzuri
pure def r_io :: i64 -> i64 = \n ->
    do! IO.write_line "hi"          // E1029: reads or writes the outside world
    n

pure def r_callee :: i64 -> i64 = \n -> helper n + 1     // E1029: 'Main.helper' is not pure ... (through 'Main.helper')
pure def f :: i64 -> i64 = \n -> host_tick n             // E1029: calls the extern function 'host_tick'
pure def g :: ['a] -> i64 = \xs -> xs.length             // E1029: a pure def must have a concrete signature
pure def h :: ref mut [i64..] -> unit = \xs -> Array.sort_in_place xs   // E1029: a pure def takes only shared references
pure def r_own_drop :: own Ticket -> i64 = \t -> t.id                    // E1029: an 'own' parameter of a pure def must be plain data
pure def r_own_rc :: own Rc<string> -> i64 = \_r -> 0                    // E1029: an 'own' parameter of a pure def must be plain data
pure def r_own_borrow :: own Counter -> i64 = \c -> c.step              // E1029: ... holds a reference（`Counter` は付録 E の e7 の record）
def plain_own :: own string -> i64 = \s -> s.length                      // E1005: 'own' is only valid on the parameters of a 'pure def'
```

### ほかの機能との関係

- **`const def`**: `const def` は本体の規則がより厳しいので、暗黙に純粋である。`pure def` は `const def` を呼べる（通常の関数でもあり、本体の検査は同じ規則で通る）。`const def` から `pure def` は呼べない（`E1026` のまま）。`const` と `pure` は併用できない。
- **`export`**: `export pure def total :: [i64] -> i64` は、補完後の `ref [i64]` が公開 ABI の検査を通るので書ける（付録 A の 15）。`own` の引数（`own [i64]`・`own string`）は値渡しのバッファなので、公開できず `E1008`（通常の `def` の `[i64]` と同じ）。
- **`rec`・`and`**: 組の全員が `pure` で、各メンバーを調べる。
- **`IO`**: `IO<T>` を返す関数や、`IO` のアクションを作る式は、Phase 1 では `pure def` に書けない（アクションの本体が `IO` の組み込み関数を参照するため、作るだけでも拒否される）。Phase 2 で、アクションを不透明な値として許すかを決める。
- **A17**: `pure def` の中のローカル可変のループ（`ys = Array.update_at .. ys`）と、`own` の引数の最後の使用は、A17 の後に複製でなく移動になる。`pure def` の本体の読みも A17 の解析の対象である（`ref` の引数は借用なので移動されない）。A17 の「一意の所有者」の前提を、`pure` を根拠にした結果の共有（Phase 2 のメモ化・共通部分式の除去）が壊してはならない。
- **A18**: D13・D16（`own` の構文を共有する）。
- **PR01**: Phase 2(c) の「関数の `memory(...)`」は、確保・IO・Task・extern を副作用として扱う解析を要する。本チケットの `src/pure_rules.rs` が同じ分類を持つので、再利用を検討する（本チケットでは接続しない）。

## 設計

### データ構造の変更

| 領域 | ファイル | 場所 | 変更 |
| --- | --- | --- | --- |
| 構文 | `src/syntax.rs` | `SignatureDecl`・`FunctionDecl`、`TypeExprKind` | `pub pure: bool`（`constant` の次）。`TypeExprKind::Own(Box<TypeExpr>)`（`Reference` の隣。`Box` で `TypeExpr` を大きくしない）。`CONTEXTUAL_KEYWORDS` に `"pure"`（配列の長さを 7 から 8 に。`of` の後） |
| 構文 | `src/parser.rs` | `constant_function_ahead` の隣、`type_primary` | `pure_function_ahead()`（現在が `Ident("pure")` で次が `Def`）。トップレベルの宣言の分岐の条件に足す。`export` の後で `pure` を食べる。`signature.pure = if continuation { pure_group } else { pure }`。`FunctionDecl` の構築で `pure: signature.pure`。`misplaced_pure`（`E0002`）、`@cpu` との併用の拒否。`type_primary` は、識別子 `own` の直後が型の先頭なら `Own` を読む（`ref` の読み方 `reference_type` が手本。入れ子の数を増減する。GUIDE §11.1） |
| コーデック | `src/syntax_codec.rs` | `FunctionDecl`・`SignatureDecl` の `put`・`get`、`impl Wire for TypeExprKind` | `constant` の次に `pure`。`TypeExprKind::Own` の tag（15）と `put`・`get` の arm。同ファイルの単体テストの変数の数 `("TypeExprKind", 15)` を 16 にし、`own` を使うソースを、そのテストが作る `Program` に足す（tag を網羅する検査）。`FRONTEND_FORMAT`（`src/frontend_cache.rs`）を上げる（AST の符号化を変えた変更ごとに 1。D-45・D-46 と同じ規則。先に入ったチケットが 4 から 5、後のチケットが 5 から 6） |
| 構文の走査 | `src/check.rs`・`src/polymorph.rs`・`src/formatter.rs`・`src/docgen.rs`・`src/semantic.rs`・`src/regions.rs`・`src/warnings.rs` | `TypeExprKind` を辿る match | `Own` の arm を足す（GUIDE §6.9 の手順: `grep -rn "TypeExprKind::Reference\|TypeExprKind::Tuple" src/` の全出現を判断し、`_ =>` で黙って無視されないようにする）。整形（`tsuzuri fmt`）と文書（`tsuzuri doc`）は `own T` を書かれたとおりに出す |
| 型検査 | `src/check.rs` | `Names` | `pure_functions: BTreeSet<usize>`（`const_functions` の隣）。関数宣言のループで `function.pure` のとき `insert` |
| 型検査 | `src/check.rs` | `FunctionOrigin` | `pub pure: bool`。`source()` は false。`generated()` は `pure: false` を明示（`cpu: 0` の隣。生成した補助関数は根ではなく、`parent` をたどって根を見つける）。ユーザー関数の構築（`provenance: function.name.provenance` を持つ 1 箇所）は `pure: names.pure_functions.contains(&id)`。`$entry` の構築は `..FunctionOrigin::source(..)` のまま（false） |
| 型検査 | `src/check.rs` | 関数宣言のループ（`resolve_type_with_kinds`・`validate_size` の後）、`resolve_type_with_kinds` | `names.pure_functions` に入る関数だけ、引数の型の前の `Own` を剥がして印を覚え（`pure def` 以外の `Own` と、引数の位置以外の `Own` は `resolve_type_with_kinds` が `E1005` にする）、署名の規則 2〜6 の検査と `pure_parameter` の補完をする。規則 1 は `variables` が分かる位置（`constants::check_signature` の隣）。規則 5 は `pure_rules::own_violation(&ty, &types)`（下の「`own` の型の判定」。違反した最初の型を返し、診断の `{what}` に使う）。`entry_signature`・`abi::parameter` の検査は補完後の型を見る（`pure def main :: Array<string> -> i32` は、補完後の `ref [string]` が `E2004` になる。`main` は純粋でなくてよいので、そのままにする） |
| 型検査 | `src/check.rs` | `check_modules` の `gpu::validate_calls` の直後 | `diagnostics.extend(pure_rules::validate(&module))` |
| 検証 | `src/pure_rules.rs`（新規）, `src/check.rs`（`#[path = "pure_rules.rs"] mod pure_rules;`。`recursion` の隣） | | 「検証のアルゴリズム」。`check` の子モジュールにして `use super::*;` する（`TypeContext::stored_all`・`reaches` が `pub(super)` なので、`src/lib.rs` に独立したモジュールとして足すと呼べない）。`Builtin` の分類は wildcard なしの `match` |
| 特殊化 | `src/polymorph.rs` | `instantiate`（型引数のない関数の `origin` を保つ） | `pure` が落ちないことを確かめる。落ちる箇所があれば `cpu` と同じように戻す |
| ツール | `src/docgen.rs`・`src/semantic.rs`・`src/api_diff.rs` | `const ` を出す箇所 | `pure ` を足す。api-diff は `pure` を足すのを互換（約束を足す）、外すのを非互換にする |
| 変更なし | `src/llvm*.rs`・`src/ownership.rs` の規則・`src/closures.rs` | | 検査と補完は型検査の段で終わる |

### `own` の型の判定（`plain_part`。D15）

許す型を列挙する。禁止する型を列挙すると、新しい `Type` の variant が黙って通ってしまうので、`src/const_rules.rs` の `value_part` と同じく、`Type` の全 variant を wildcard なしの `match` で書く。置き場所は `src/pure_rules.rs`（`check` の子モジュールなので、`TypeContext::stored_all` と `CheckedRecord.user_drop` を使える）。

```rust
/// A19: what an `own` parameter of a `pure def` may hold. Every `Type` variant is named, without a wildcard.
fn plain_part(ty: &Type, types: &TypeContext<'_>) -> bool {
    match ty {
        Type::Integer(..)
        | Type::Binary(_)
        | Type::Decimal(_)
        | Type::Simd(_)
        | Type::Bool
        | Type::Unit
        | Type::Char
        | Type::Utf8Char
        | Type::String
        | Type::Utf8String
        | Type::Array(_)
        | Type::List(_)
        | Type::Vec(_)
        | Type::Tuple(_) => true,
        Type::FixedArray(_, length) => matches!(**length, Type::Length(_)),
        // A user `Drop` runs in the callee.
        Type::Record(id, _) => !types.records[*id].user_drop,
        Type::Union(id, _) => !types.unions[*id].user_drop,
        // These reach state outside the value, or run code or change a count when dropped.
        Type::Reference(..)
        | Type::Shared(..)
        | Type::Handle(_)
        | Type::Task(_)
        | Type::Dyn(_)
        | Type::Function(..)
        // Never the type of a value, or not settled in a concrete signature.
        | Type::ArrayView(_)
        | Type::Length(_)
        | Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => false,
    }
}

/// The first type inside `ty` that `plain_part` refuses, for the message; `None` if `ty` is plain data.
pub(super) fn own_violation(ty: &Type, types: &TypeContext<'_>) -> Option<Type> {
    let found = std::cell::RefCell::new(None);
    types.stored_all(ty, |part| {
        let plain = plain_part(part, types);
        if !plain {
            found.borrow_mut().get_or_insert_with(|| part.clone());
        }
        plain
    });
    found.into_inner()
}
```

`stored_all` は `Fn` の述語を取るので、違反した型は `RefCell` に控える。`stored_all` は record の field・union の payload・配列・リスト・`Vec`・固定長配列の要素・tuple の成分を辿り、参照の先は辿らない（参照そのものが `plain_part` で拒否される）。`std` の `Map`・`Set`・`HashMap`・`Matrix`・`Tensor` などは、宣言された field（`Vec` や配列）が平たいので受理される。`Seq`・`IO`・`Gen`・`Async`・`Owned.Function` は関数型の field を持ち、`MatrixView.Mut` は `ref mut` の field を持つので拒否される。`Atomic`・`Mutex`・`Net.*`・`File.Handle` は平たい field（整数や配列）で宣言されているので型としては受理されるが、使うには不純な組み込み関数が要り、本体の規則で拒否される。

### 検証のアルゴリズム

`src/pure_rules.rs` の入口は `pub(crate) fn validate(module: &CheckedModule) -> Vec<Diagnostic>`。`closures::lower` の後の `CheckedModule`（関数の id は `FunctionRef::User(id)` の添字。`src/gpu.rs` の `extract_kernel` と同じ前提）を読む。

1. **根。** `origin.pure` が true の関数（宣言された `pure def`）。1 つもなければ、何もせず空を返す（`pure` を使わないプログラムに費用はない）。
2. **関数ごとの局所の走査**（関数ごとに 1 回、結果を `BTreeMap<usize, Scanned>` に保存）。本体を、明示のスタックで全部の式について走査する（再帰しない。GUIDE §11.1 の stack の深さ）。
   - 各式で、(a) 式の型が利用者の `Drop` を持つ値なら `UserDrop`（`types.stored_all(&expression.ty, |ty| !ty.has_user_drop(&types))` が false。結果は型ごとにキャッシュ）、(b) `expression.kind` を**wildcard なしの `match`** で分類する: 効果の源なら `Effect` を、`Function(User(id))`・`Closure(id, _)` なら辺 `(id, expression.span)` を、`Function(Builtin(instance))` なら `builtin_effect(instance.builtin)` の結果を、そのほかは何もしない。(c) `expression.children()` をスタックに積む。
   - 結果は `Scanned { effect: Option<(Effect, Span)>, edges: Vec<(usize, Span)> }`。効果は**スパンが最小のもの**（`(source, start)` の順）を採る。辺はスパン順に並べる（決定性）。
   - 走査した式の総数を数え、上限（決定 D11）を超えたら `E1017`。
3. **根ごとの到達可能性。** 根 `r` から、辺をたどる幅優先探索（`visited`、`parent: BTreeMap<usize, (usize, Span)>`）。訪れた関数 `f` の `Scanned.effect` が `Some` なら、その根は不純として、経路 `r → … → f` から診断を作る。到達した関数の数が上限を超えたら `E1017`。**深さ優先で関数の結果をキャッシュしない**: 循環があると、探索の途中の祖先を「純粋」とみなしたまま、ほかの関数の結果を保存してしまい、別の根で効果を見逃す。幅優先の到達可能性と、関数ごとの局所の結果（辺と効果）だけを保存する。
4. **診断の位置。** 経路の関数のうち、`pure def` の「家族」（根と、`origin.parent` をたどって根に着く関数＝ラムダ）の外の最初の関数に入る辺の `Span`（家族の中の式）を位置にする。効果が家族の中にあれば、その式の `Span`。メッセージの `{callee}` はその最初の外の関数の `qualified_name()`（`.$mono.N` の接尾辞は落とす）、`{chain}` は経路の関数名。
5. **関数値の呼び出し**は、辺を作らず、効果にもしない（「本体の規則」の最後の段落）。

`Effect` は、`Host(String)`、`Builtin(&'static str, Category)`、`Concurrency`、`Dyn`、`UserDrop(String)`、`Unresolved`。`Category` は reason の文言を決める（外界・時計と乱数・並行・共有可変状態・GPU）。

### 組み込み関数の分類

`fn builtin_effect(builtin: Builtin) -> Option<Category>` を、`Builtin` の全 variant を並べた wildcard なしの `match` にする（新しい `Builtin` を足すと、判断を強いられる）。不純（`Some(..)`）は次の 90 個:

- 外界: `DebugPrintString`、`IOReadLine`、`IOWrite`、`OsRead`、`OsArgs`、`OsWrite`、`OsRandom`、`OsClock`、`OsSleep`、`OsOpen`、`OsHandle`、`OsClose`、`OsSpawn`、`NetResolve`、`NetOpen`、`NetAccept`、`NetRead`、`NetWrite`、`NetClose`、`NetClassify`、`NetWatch`、`NetUnwatch`、`NetConnect`、`NetNames`、`NetSend`（25）
- 時計・乱数・計測: `BenchNow`、`BenchConsume`、`GenSeed`（3）
- 並行: `TaskRun`、`TaskParallel`、`TaskParallelResults`、`TaskScope`、`ParallelInit`、`ParallelMap`、`ParallelMapRef`、`ParallelReduce`、`ParallelForEachChunk`、`AsyncResume`、`AsyncTake`、`AsyncPut`、`AsyncNextId`、`AsyncClock`、`AsyncWait`、`AsyncPosted`、`AsyncRetire`（17）
- 共有可変状態: `AtomicCreate`、`AtomicLoad`、`AtomicStore`、`AtomicSwap`、`AtomicCompareExchange`、`AtomicFetchAdd`、`AtomicFetchSub`、`AtomicFetchAnd`、`AtomicFetchOr`、`AtomicFetchXor`、`AtomicIntoInner`、`MutexCreate`、`MutexWith`、`MutexIntoInner`、`ChannelBounded`、`ChannelSend`、`ChannelRecv`、`ChannelCloneSender`、`ChannelCloseSender`、`ChannelCloseReceiver`、`RcNew`、`RcShare`、`RcGet`、`RcStrongCount`、`RcWeakCount`、`RcTryUnwrap`、`RcDowngrade`、`RcUpgrade`、`RcPtrEq`、`ArcNew`、`ArcShare`、`ArcGet`、`ArcStrongCount`、`ArcWeakCount`、`ArcTryUnwrap`、`ArcDowngrade`、`ArcUpgrade`、`ArcPtrEq`、`ArenaNextId`（39）
- GPU: `GpuOpen`、`GpuFeatures`、`GpuRun`、`GpuSelect`、`GpuLast`（5）
- 動的ディスパッチ: `DynOf`（1）

残りの 156 個は純粋である: 数値（`Math*`・`Int*`・`ToFloat`・`ToInt`・`Sqrt`・`Floor`・`Ceil`・`Abs`・`Test*`）、文字列と文字（`Clone*`・`String*`・`Utf8*`・`Char*`・`ToString`・`Parse`・`Display*`・`Hash*`・`Default`・`Unicode*`）、配列・リスト・`Vec`・固定長配列・`Seq`（`Array*`・`List*`・`Vec*`・`FixedArrayInit`・`SeqNext`）、SIMD（`Simd*`）、`Owned*`、`Assert`・`Unreachable`（トラップ）、`Not`・`Ignore`。`Vec` と配列を書き換える組み込み関数（`Vec.push` は値を受け取って返し、`ArrayWrite` などは `ref mut` を取る）は、`pure def` ではローカルの値にしか使えない（引数は共有借用で、`ref mut` も値の消費もできない）ので純粋とする。判断がつかない variant は、不純（拒否）に倒し、停止条件に当たるか確かめる。

テスト用の単体テスト（`src/pure_rules.rs` の `#[cfg(test)]`）が、不純の集合を名前で固定する（90 個。意図しない再分類を検出する）。

### 上限

`Limits { functions: 4096, nodes: 2_097_152 }`（総数。決定 D11）。単体テストは小さい `Limits` を `validate_with(module, limits)` に渡して、ちょうどの境界を調べる（手本: `honors_the_exact_specialization_limit`）。超えたら `E1017`、メッセージは `a pure def reaches more than {n} functions or {m} expression nodes; split it`。

### 生成 IR

変更なし。

## 実装手順

### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。`mkdir -p /tmp/tz-A19/before && cp target/release/tsuzuri /tmp/tz-A19/tsuzuri-before`。`tests/fixtures` と `examples` の各プロジェクトを `tsuzuri-before build <dir> --emit llvm -O0 -o /tmp/tz-A19/before/<名前>.ll` で出す（出せないプロジェクトは一覧に残す。手順 2 以降の「IR が変わらない」確認に使う）。付録 A・B・E の各プログラムを `/tmp/tz-A19/` に作り直し、HEAD の結果が表と一致することを確かめる（付録 E は「`own` と純粋性」の根拠。結果が違ったら、D1 の条件が崩れたとして停止条件）。付録 C を HEAD で 1 回測る。`grep -rn "@alias pure" tests examples std _tsuzuri` が 0 件であることを確かめる。
- 確認: すべて通る。付録の結果が一致する。

### 手順 2: 構文（`pure` の印を通すだけ。意味は変えない）

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/check.rs`・`src/polymorph.rs`（`FunctionDecl` の構築箇所）、`tests/pure_functions.rs`（新規）。
- 内容: 「データ構造の変更」の構文とコーデックの行のとおり。`SignatureDecl.pure`・`FunctionDecl.pure` を足し、コンパイラが「フィールドが足りない」と言う箇所をすべて `pure: false` で直す（`grep -rn "constant:" src` と同じ一覧）。`pure_function_ahead` は、`TokenKind::Ident(word) if word == "pure"` で、次のトークンが `TokenKind::Def` のときだけ真にする。`CONTEXTUAL_KEYWORDS` に `"pure"` を足す。この段では、検査も補完もしない。`FRONTEND_FORMAT` を 1 上げる（AST の符号化を変える手順ごとに上げる。手順 3 でもう 1 上げる）。
- 確認: `tests/pure_functions.rs` に `parses_pure_def_as_a_modifier_and_keeps_pure_as_an_identifier`・`rejects_misplaced_pure`・`formats_pure_def_and_round_trips` を足し、`cargo test --locked --test pure_functions` が通る。`cargo test --locked` が手順 1 と同じ件数（＋新規）で通り、`node tests/frontend_cache.mjs target/release/tsuzuri` が通る。手順 1 の IR と `--emit llvm -O0` の出力が 1 byte も変わらない。

### 手順 3: 構文（`own` の前置き。受理は `pure def` の引数だけ）

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/check.rs`、`src/polymorph.rs`、`src/formatter.rs`、`src/docgen.rs`、`src/semantic.rs`、`src/regions.rs`、`src/warnings.rs`、`tests/pure_functions.rs`。
- 内容: A18 の手順 3 と同じ実装（A18 が先に実装済みなら、`TypeExprKind::Own` と読み方は再実装せず、受理する位置の規則だけを足す。D16）。`TypeExprKind::Own(Box<TypeExpr>)` を足し、`type_primary` が、識別子 `own` の直後に型の先頭が続くときだけ `Own` を読む。`TypeExprKind` を辿る全出現（`grep -rn "TypeExprKind::Reference\|TypeExprKind::Tuple" src/`）に arm を足し（GUIDE §6.9）、`_ =>` で黙って無視されないようにする。符号化（`syntax_codec.rs` の `TypeExprKind` の tag 15、同ファイルの単体テストの数 `("TypeExprKind", 15)` を 16 にし、`own` を使うソースをテストの `Program` に足す）と、`FRONTEND_FORMAT` をもう 1 上げることを足す。
  `resolve_type_with_kinds` は `Own` を、`pure def` の引数以外の位置（通常の `def` の引数、結果の型、`let` の注釈、フィールド、関数型の中、型引数）で `E1005` にする。関数宣言のループは、`pure def` の引数の `Own` を剥がして印を覚える（この段では補完がないので、印を捨てるだけで意味を変えない。手順 4 で使う）。整形（`tsuzuri fmt`）と `tsuzuri doc` は `own T` を書かれたとおりに出す。
- 確認: `tests/pure_functions.rs` に `parses_own_on_a_parameter_type_and_keeps_own_as_an_identifier`・`rejects_own_outside_a_pure_def_parameter`・`formats_own_and_round_trips` を足して通る。`pure def f :: own string -> i64` が受理され、`own` を外した版と IR が同じ。`def f :: own string -> i64`・`let x: own string = ..`・`pure def f :: i64 -> own string`・`(own string -> i64)` が `E1005`。識別子 `own` を変数名に使うコードが従来どおり通る。`cargo test --locked` が通り、`syntax_codec` の tag の網羅の単体テストが通る。手順 1 の IR が 1 byte も変わらない。

### 手順 4: 署名の規則と引数の補完

- 変更: `src/check.rs`、`tests/pure_functions.rs`。
- 内容: `Names.pure_functions`、`FunctionOrigin.pure`（`source` は false、`generated` は false）、ユーザー関数の `origin` の構築を足す。関数宣言のループで、`names.pure_functions` に入る関数だけ、署名の規則 2〜6（`E1029`）を調べ、`pure_parameter` で補完する（`resolve_type_with_kinds`・`validate_size` の後、`parameters.push(ty)` の前）。規則 1 は `variables` が分かる位置で調べる。規則 4 は `types.reaches(&ty, |ty| matches!(ty, Type::Function(..) | Type::Dyn(_) | Type::Task(_)) || ty.is_owned_function(&types))`。規則 5（`own` の型）は、手順 3 で剥がした印が付いた引数だけに、「`own` の型の判定」の `own_violation` をかける（`plain_part` は `Type` の全 variant を wildcard なしで判定する）。規則 6 は、`own` の引数の解決後の型が `Type::Reference` のとき（規則 5 より先に調べて、`own ref T` に専用のメッセージを出す）。`polymorph::instantiate` が、型引数のない関数の `origin.pure` を保つことを確かめる。この段では本体を調べない。
- 確認: `tests/pure_functions.rs` に `pure_def_parameters_are_shared_references`・`pure_def_own_parameters_are_values`・`pure_def_own_requires_plain_data`・`pure_def_own_keeps_the_ownership_rules`・`pure_def_signature_rules`・`pure_def_ir_matches_the_explicit_ref_version`・`pure_def_exports_without_writing_ref` を足して通る。IR が手順 1 と 1 byte も変わらない（`pure` を使わないプロジェクト）。

### 手順 5: 検証器

- 変更: `src/pure_rules.rs`（新規）、`src/check.rs`（`mod pure_rules;` の宣言と呼び出し）、`tests/pure_functions.rs`。
- 内容: 「検証のアルゴリズム」と「組み込み関数の分類」のとおり実装し、`gpu::validate_calls` の直後で呼ぶ。式の分類は `TypedExprKind` の全 variant を wildcard なしで書く。`Builtin` の分類も同じ。`const_rules.rs` の書き方（`use TypedExprKind as K;`）に揃える。診断の位置と `{chain}` は「診断 `E1029`」のとおり。
- 確認: `tests/pure_functions.rs` に `pure_def_accepts_pure_bodies`・`pure_def_rejects_each_effect`・`pure_def_rejects_impure_callees_and_names_the_chain`・`pure_def_follows_calls_through_std_and_closures`・`pure_def_in_a_recursive_group_checks_every_member`・`lifted_lambdas_report_at_their_own_span`・`pure_defs_are_validated_even_when_unused` を足し、`src/pure_rules.rs` の単体テスト `every_builtin_has_a_verdict`・`honors_the_exact_limits` が通る。`cargo test --locked` が通る。**停止条件**: 主要な std の関数が不純と判定されたら、判定を変えずに止めて報告する。

### 手順 6: ツール

- 変更: `src/docgen.rs`・`src/semantic.rs`・`src/api_diff.rs`・`tests/pure_functions.rs`（と、既存のテストへの影響があれば該当ファイル）。
- 内容: `docgen` の署名の前置き（`export const def rec` を作る `format!`）、`semantic` の hover（`const def` を出す 2 箇所）に `pure ` を足す。`api_diff` は `Item` に `pure: bool` を足し、`text()` に `pure ` を、`compare_items` の `widened` に `old.pure <= new.pure` を足す。`CONTEXTUAL_KEYWORDS` に足した `pure` が LSP の補完に出る（`src/lsp.rs` は `KEYWORDS` と `CONTEXTUAL_KEYWORDS` を連ねるので、`lsp.rs` の変更は要らない。補完の件数を数えるテストがあれば直す）。formatter は変更なし（`tsuzuri fmt --check` が `pure def` で通ることをテストする）。
- 確認: `api_diff_treats_pure_as_a_promise`（`pure` を足すのは非破壊、外すのは破壊）、docgen の出力に `pure def`、LSP の hover と補完に `pure` が出るテストを足して通る。`cargo test --locked` が通る。

### 手順 7: E2E と IR の第二の検査

- 変更: `tests/fixtures/pure_functions/Main.tz`（新規）、`tests/features.mjs`（suite `pure_functions`）、`tests/pure_functions.rs`。
- 内容: fixture は付録 B の「fixture」のとおり（`own` の関数 4 つを含む）。suite の期待値は独立した JavaScript の計算で、コンパイラの出力を写さない（付録 B）。IR の第二の検査 `pure_defs_reach_no_effectful_runtime_symbol` は、fixture の IR で、各 `pure def` から `@tz.fn.` の参照をたどって到達できる関数の本体（BFS）に、効果のあるランタイムの記号がないことを調べる。記号の一覧は、実装時に `grep -ho "@tsuzuri_[a-z_]*\|@tz\.[a-z]*" src/*.rs src/runtime/*` で接頭辞を確かめてから固定する（候補: `tsuzuri_io_`・`tsuzuri_os_`・`tsuzuri_net_`・`tsuzuri_async_`・`tsuzuri_host_`・`@tz.async`・`@tz.gpu`・`@tz.mutex`・`@tz.debug`・`@tz.task`・`atomicrmw`・`cmpxchg`・`fence`）。検証器とは独立の、組み込み関数の分類の誤りを見つける検査である。
- 確認: `node tests/features.mjs target/release/tsuzuri pure_functions` が通る（件数を控える。native/WASM × `-O0`/`-O3`、`live == 0`、WASM の import が空）。`own` の関数（`t_with_first_own` など）は、呼び出し側の配列が呼び出しのあとも変わらないことを、期待値の `101n * n + 91n` が固定する。

### 手順 8: 全体の確認と文書

- 内容: GUIDE §3 の全体（fmt・clippy・`cargo test --locked`・`node tests/features.mjs target/release/tsuzuri`（全 suite）・e2e の各スクリプト）、§3.1 の回帰テスト 4 本、`sh scripts/check-runtime-includes.sh`。手順 1 の IR と、`pure` と `own` を使わない全プロジェクトの IR（`--emit llvm -O0`）が 1 byte も変わらないこと。同じ入力で 2 回ビルドして IR が一致すること。「ドキュメント」のとおり文書を直し、`node scripts/check-docs.mjs` を変えたページに対して通す。
- 確認: すべて通る。GUIDE §13.2 の書式の完了報告を、チケット末尾の `## 実装と検証（YYYY-MM-DD）` に書く。

## Phase 2 以降（全項目を承認済み。以下は着手前の設計記録）

2026-10-23 の依頼で以下の全項目を実装対象とした。旧 D8・D9・D12・D16 と Phase 1 の拒否例は、
着手前の範囲を記録したものである。現行の契約は README の「A19 全 Phase の追加設計の照合」と
言語リファレンスの「関数」を正とする。通常の `def` の `own` は A18 で受理され、
generic・純粋 callback・閉じた明示並行処理・`@cpu`・class／instance は後続実装に含む。
自動 offload、所有結果の共有、グローバル memo cache、停止・トラップの保証は対象にしない。

- **`own` を `pure def` 以外の引数にも許すこと。** A18 の D2（要承認）。本チケットは `pure def` の引数に限って `own` を入れる（D16）。
- **ジェネリックな `pure def`。** 型変数を持つ署名を許し、`'a` の引数を `ref 'a` にする。型クラスのメソッドが値の引数を取る場合（`Add.add`）との整合、`Copy<'a>` 制約の扱いを決める。特殊化した関数が `pure` の契約を引き継ぐ。
- **関数型の純粋性。** `pure (T -> U)` のような関数型を持ち、`pure def` がコールバックを取れるようにする。`Parallel.map` など、コールバックが純粋なときだけ許す API を、型で表せる。型システムの変更なので、A18 の Phase 3（規約を型の外に出す設計）と一緒に検討する。
- **`Parallel.*`・`Task` の許可。** コールバックが純粋なら、結果は決定的なので許せる（`Matrix` の乗算を呼べるようになる）。
- **最適化への利用。** PR01 Phase 2(c) の関数の `memory(...)`、共通部分式の除去、メモ化、並列化、GPU カーネルの検査の共通化。結果を共有する最適化は、A17 の「Copy コレクションの値は常に一意の所有者」を壊さない（共有した結果は複製して返すか、読み取り専用に限る）。未使用の呼び出しを消すには、「トラップしない」という別の保証（`total`）が要る。
- **std への `pure` の付与。** 検証器が推論するので、std を書き換えなくても `pure def` から呼べる。付与は API の契約として別チケットで行う。
- **`IO` アクションの構築**を、不透明な値の生成として許す。
- **`@cpu`・`instance` のメソッドへの `pure`。**
- **診断の強化。** `pure def` の補完した引数に対する `E1003`・`E1012` の末尾の文言（A18 の D8 と同じ仕組みを共有する）。
- **api-diff** が補完後の署名で比較すること（今は書かれた型を比べる）。

## テスト計画

### Rust テスト（`tests/pure_functions.rs`。新規。手本 `tests/constants.rs`）

helper は `tests/constants.rs` の `rejected(source)`（`analyze(source).expect_err(source)`）と、`tests/call_specialization.rs` の `body(ir, name)` を手本にする。

| 手順 | 名前 | 内容 |
| --- | --- | --- |
| 2 | `parses_pure_def_as_a_modifier_and_keeps_pure_as_an_identifier` | `pure def`・`export pure def`・`private pure def`・`pure def rec .. and ..`（`and` が `pure` を引き継ぐ）の `pure` の印。`def pure :: i64 -> i64`、`let pure = 1`、`pure` という名前のフィールドは、印のない通常の識別子として今までどおり構文解析される |
| 2 | `rejects_misplaced_pure` | `pure fn f x = x`、`pure export def`、`const pure def`、`pure const def`、`@cpu ["avx2"] pure def` が `E0002` |
| 2 | `formats_pure_def_and_round_trips` | `formatter::format_source` が `pure def` を冪等に整形する |
| 3 | `parses_own_on_a_parameter_type_and_keeps_own_as_an_identifier` | `pure def f :: own string -> i64`・`own [i64]`・`own i64`、`own` と無印の混在が構文解析できる。`let own = 1`、`own` という名前の関数・フィールドは、今までどおり通る（`own` の直後が型でなければ前置きにならない） |
| 3 | `rejects_own_outside_a_pure_def_parameter` | `def f :: own string -> i64`（通常の `def`）、`pure def f :: i64 -> own string`（結果の型）、`let x: own string = ..`、record の field の型、`(own string -> i64)`（関数型の中）、`Maybe<own string>`（型引数）が `E1005` |
| 3 | `formats_own_and_round_trips` | `formatter::format_source` が `own T` を冪等に整形し、`tsuzuri doc` が書かれたとおりに出す |
| 4 | `pure_def_parameters_are_shared_references` | 表で、`string` → `ref string`、`[i64]` → `ref [i64]`、`Maybe<[i64]>` → `ref Maybe<[i64]>`、`Named`（`string` を持つ record）→ `ref Named`、`i64`・`f64`・`bool`・`Point`（Copy で drop 不要の record）・`unit` は値のまま、明示の `ref string` はそのまま（`module.functions[id].signature.parameters`） |
| 4 | `pure_def_own_parameters_are_values` | `own string` → `string`、`own [i64]` → `[i64]`、`own i64` → `i64`（印を外すだけ）。`own` と無印の混在（`pure def f :: own [i64] -> string -> i64` は `[i64]` と `ref string`）。`own ref string` は `E1029` |
| 4 | `pure_def_own_requires_plain_data` | `own Ticket`（利用者の `Drop`）、`own Rc<string>`、`own Arc<string>`、`Rc` を持つ record の `own`、`own Maybe<Rc<string>>`、関数・`dyn`・`Task` を持つ型の `own`、`own Counter`（`ref mut` の field を持つ record。付録 E の e7）、`ref` の field を持つ record の `own` が `E1029`。メッセージの `{what}` が分類（`a reference`・`an 'Rc' or 'Arc'`・`a value with a user Drop` など）に合う。`own [string]`・`own Vec<i64>`・`own Maybe<[i64]>`・`own Point` は受理される。`ref Rc<string>` と `ref Counter`（`ref` の引数）は署名として受理される |
| 4 | `pure_def_own_keeps_the_ownership_rules` | `pure def f :: own string -> i64` を呼んだあとで、呼び出し側が同じ `string` を読むと `E1012`（所有権検査の規則は変わらない）。`own [i64]`（Copy）の呼び出しのあとも、呼び出し側は自分の配列を読める |
| 4 | `pure_def_signature_rules` | `['a]` の署名、制約つき、`ref mut` の引数、`mut` の引数（`own` 付きも）、関数型の引数、関数の field を持つ record の引数、`dyn` の引数が `E1029`。`const def` の `E1026`、`def` の `E1003` は変わらない |
| 4 | `pure_def_ir_matches_the_explicit_ref_version` | `pure def f :: string -> [i64] -> i64` と `def f :: ref string -> ref [i64] -> i64` の IR で、関数名を除いて `f` の本体と呼び出し側が同じ。`pure def f :: own [i64] -> i64` と `def f :: [i64] -> i64`（`2026` の値渡し）の IR も同じ |
| 4 | `pure_def_exports_without_writing_ref` | `export pure def total :: [i64] -> i64` が受理され、公開 ABI（`--emit` のヘッダー）が明示の `ref` と同じ。`export pure def f :: [string] -> i64` は `E1008`。`export pure def f :: own [i64] -> i64` も `E1008`（値渡しのバッファ） |
| 5 | `pure_def_accepts_pure_bodies` | 「例」の受理される定義（`add` から `total`）が、すべて受理される |
| 5 | `pure_def_rejects_each_effect` | 付録 B の拒否の表の 1 行ごと: `E1029` と、メッセージに分類の語（`outside world`・`debug output`・`extern`・`shared mutable state`・`task`・`user Drop`）を含む |
| 5 | `pure_def_rejects_impure_callees_and_names_the_chain` | `r_callee`: `pure def` が通常の `def` の `helper`（`IO` を使う）を呼ぶ。メッセージが `Main.helper` を含み、位置が `pure def` 内の `helper n` の式（`helper` の本体ではない）。2 段の連鎖で `through` に両方の名前が出る |
| 5 | `pure_def_follows_calls_through_std_and_closures` | `Array.map (\x -> x * 2) xs` は受理。`Array.map (\x -> helper x) xs`（`helper` は付録 B の `IO` を使う通常の `def`）は、ラムダの中の `helper x` の位置で `E1029`。`Array.sort` の導出された `Ord`（`Point`）は受理。利用者の `instance Display<T>` が `IO` を使うとき、`$"{t}"` が拒否される |
| 5 | `pure_def_in_a_recursive_group_checks_every_member` | `pure def rec f .. and g ..` の `g` が `IO` を使うと、`f` の位置でなく `g` の本体で `E1029` |
| 5 | `lifted_lambdas_report_at_their_own_span` | ラムダの中の効果の位置が、ラムダの中の式（`pure def` の先頭の行ではない） |
| 5 | `pure_defs_are_validated_even_when_unused` | どこからも呼ばれない `pure def` も検査される |
| 6 | `api_diff_treats_pure_as_a_promise` | `def` → `pure def` は非破壊、逆は破壊 |
| 7 | `pure_defs_reach_no_effectful_runtime_symbol` | IR の第二の検査（手順 7） |

`src/pure_rules.rs` の単体テスト: `every_builtin_has_a_verdict`（`Builtin::ALL` を回して `builtin_effect` が呼べ、不純が名前で 90 個、純粋が 156 個）、`honors_the_exact_limits`（小さい `Limits` で、ちょうどの境界は通り、1 超えると `E1017`）。

### E2E

- suite `pure_functions`（`tests/features.mjs`。手本: suite `const_functions`・`last_use`）。fixture は付録 B の「fixture」。期待値は独立した JavaScript の計算（付録 B）。native/WASM × `-O0`/`-O3` で `live == 0`、WASM の import が空。

### 既存テストへの影響

- `FRONTEND_FORMAT` を上げるので、キャッシュの形式を固定している期待値があれば直す（`tests/frontend_cache.mjs`・`src/frontend_cache.rs` の単体テストを `grep -rn "FRONTEND_FORMAT\|format.*4" tests src` で確かめる）。それ以外の既存の期待値は変わらない。変える必要が出たら停止条件。
- LSP の補完の件数や一覧を固定するテストがあれば、`pure` を足す（`grep -n "namespace\|finally" tests/lsp.rs`）。

### 性能

生成 IR を変えないので、性能は測らない（性能を主張しない）。手順 8 で、`pure` と `own` を使わないプロジェクトの IR が 1 byte も変わらないことだけを確かめる。

## ドキュメント

利用者から見える新しい構文と診断なので、すべて更新する（GUIDE §8）。`grep -rnE "const def|E1026|文脈キーワード" _tsuzuri/language-reference docs README.md` で漏れを探す。

- `_tsuzuri/language-reference/values-and-functions/functions.md`: 「純粋関数（`pure def`）」の節を足す。保証する 3 つ（副作用がない・引数は共有借用か `own` だけ・結果は引数の値だけで決まる）、`ref` を省略できて、`own` を書かない引数が共有借用になること（スカラーは値のまま）、`own` の書き方と、`own` が平たいデータの型に限られる理由（`Rc`・利用者の `Drop` を消費すると破棄が観測できる）、明示の複製（`clone_string`・`Array.copy`・`List.copy`）、書けない形（`E1029` の一覧）、効果の源の表、`const def` との関係。例は `run=` で検証する。
- `_tsuzuri/language-reference/values-and-functions/keywords.md`: 文脈キーワードに `pure`（`def` の前だけ）と `own`（`pure def` の引数の型の前だけ）を足す。`pure`・`own` という名前の識別子は使える。
- `_tsuzuri/language-reference/ownership-and-memory/borrowing.md`・`ownership.md`: `pure def` の引数は、無印なら共有借用、`own` なら所有権の移動であること。
- `_tsuzuri/language-reference/compiler/diagnostics.md`: `E1029` の行を足し、「`E1001`〜`E1028`」を `E1029` までにする。`E1005` の行に、`own` を `pure def` の引数以外に書いた誤りを足す。
- `docs/language.md`: 「ソースと宣言」の文脈キーワード（`of`・`where` の段落）、`const def` の近くに `pure def` の節、診断の表に `E1029`、`IO` の「完全なエフェクトシステムではありません」の段落に `pure def` への参照。`README.md` の該当する説明。
- `docs/stability.md`: api-diff の分類（`const` の行の隣）に `pure` を足す（足すのは互換、外すのは非互換）。
- `docs/architecture.md`: `src/pure_rules.rs` の位置（`closures::lower` の後、`gpu::validate_calls` の隣）、到達可能性の幅優先と循環の注意、`Builtin` の分類の維持、`TypeExprKind::Own` と、`FRONTEND_FORMAT` が上がったこと。
- `_features/GUIDE.md`: §4 のコードマップに `src/pure_rules.rs` の行。
- `_tsuzuri/language-reference/index.md`: 新しい節への導線。

## 受け入れ条件

- [x] D1・D2・D4 の承認が記録されている（2026-10-13。D1 の `own` は条件つき: 付録 E を HEAD で再現し、「`own` と純粋性」の根拠が崩れていないこと）。
- [x] 手順 1〜8 と承認済みの全後続項目を実施し、現行契約に更新した各段の確認が通る。旧 Phase 1 の拒否例は、上記の現行契約で置き換えたものを除く。
- [x] `tests/pure_functions.rs` の名前付きテストと `src/pure_rules.rs` の単体テストがすべて通る。
- [x] `node tests/features.mjs target/release/tsuzuri pure_functions` が通り（件数を完了報告に書く）、全 suite も通る。native/WASM × `-O0`/`-O3`、`live == 0`、WASM の import が空。
- [x] `pure` と `own` を使わないプログラムの受理・拒否・診断・IR が、手順 1 の記録と 1 byte も変わらない。同じ入力で IR が 2 回とも一致する。`pure`・`own` という名前の識別子が今までどおり使える。
- [x] `pure def f :: string -> [i64] -> i64` と `def f :: ref string -> ref [i64] -> i64` の IR が（関数名を除いて）一致する。スカラーの引数は値のまま。`pure def f :: own [i64] -> i64` と `def f :: [i64] -> i64` の IR も一致する。
- [x] pure の `own` の引数が平たいデータに限られる（`own Ticket`・`own Rc<string>`・`own Counter`（排他借用の record）が `E1029`）。型の全 variant を wildcard なしで分類する。通常の `def` の引数の `own` は A18 の契約どおり受理し、結果・field・型引数など無効な位置は `E1005`。所有権検査の規則（消費した `string` の再読は `E1012`）は変わらない。
- [x] 付録 B の拒否の表のすべての行が、期待どおりの分類で `E1029` になる。主要な std の関数（停止条件の一覧）が `pure def` から呼べる。
- [x] IR の第二の検査（`pure_defs_reach_no_effectful_runtime_symbol`）が通る。
- [x] `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`、GUIDE §3.1 の回帰テスト、`sh scripts/check-runtime-includes.sh` が通る。
- [x] ドキュメントを更新し、`node scripts/check-docs.mjs` が通る。`_features/README.md` の状態を `done（全 Phase）` にし、チケットを `_completed/` へ移す。
- [ ] GUIDE §13.2 の完了報告を末尾に書き、チケット単位のブランチでコミット・push・PR 作成を行う。依頼どおり Copilot レビューの妥当な指摘を解消し、最終 head の全 CI を確認する。

## 落とし穴

- **`TypedExpr::children` は非網羅。** 末尾の `_ => Vec::new()` に落ちる variant がある。走査（子を積む）にだけ使い、効果の分類は wildcard なしの `match` で書く。新しい `TypedExprKind`・`Builtin` の variant が、黙って「純粋」にならないようにする。
- **関数の参照は呼び出しの可能性。** `Interpolated` の `hole.method`、`StructuralCompare`・`StructuralHash`・`StructuralDisplay` が持つ要素ごとのメソッド、`Array.map` に渡す `Function(User(id))` は、呼び出しの式ではなく**参照**として現れる。`Call` の callee だけを辺にすると、これらを見逃す。`Function`・`Closure` の出現はすべて辺にする。
- **循環で結果をキャッシュしない。** 深さ優先で「探索中の祖先は純粋」とみなして関数ごとの結果を保存すると、循環の中の関数が誤って純粋と記録され、別の根で効果を見逃す。到達可能性（幅優先）と、関数ごとの局所の走査の結果だけを保存する。
- **診断の位置は `pure def` の側。** std や別のファイルの位置を指さない。`origin.parent` をたどって、`pure def` の中のラムダ（家族）を見分ける。`.$mono.N` の接尾辞はメッセージに出さない。
- **`generated` で `pure` を引き継がない。** 生成した補助関数と特殊化の実体を根にすると、std の本体の中で診断が出る。`generated()` は `pure: false` を明示し、`polymorph::instantiate` が型引数のない関数の `origin` を保つ（`pure` が落ちない）ことを確かめる。
- **補完の位置。** 引数の補完は関数宣言のループだけ（`resolve_type_with_kinds` に入れると、結果の型・field・`let` の注釈・関数型にも効いて意味が広がる）。`entry_signature`・`constants::check_signature`・`abi::parameter` は補完後の署名を見る。`pure def main :: Array<string> -> i32` は補完後の `ref [string]` で `E2004` になるが、`main` は純粋でなくてよいので問題にしない（`pure def main :: unit -> i32` は署名の検査を通り、本体で入出力を使えば本体の検査で拒否される）。
- **スカラーは値のまま。** `ref i64` は算術にも戻り値にも使えない（`E1003`）。`is_copy && !needs_drop` を、解決後の型で判定する。型変数は規則 1 で拒否済みなので、ここには来ない。
- **`ref` の関数値は呼べない**（`E1005`）。関数型の引数は規則 4 で拒否する。ラムダを `let` で束縛して `Array.map` に渡すのは、本体で作った関数値なので許される。
- **`dyn`・`Owned.function`。** `dyn` の vtable のメソッドは、式の中に参照として現れない。`DynOf`・`DynDispatch` を拒否するのは、そのためである（許すと、呼ばれる関数を調べられない）。
- **利用者の `Drop`。** 値が出る位置で暗黙に `drop` が呼ばれる。型付き IR にはその呼び出しが出ないので、型で拒否する（`stored_all`。`reaches` は参照の先まで見るので使わない）。
- **`own` の型は、許す型を列挙して判定する。** 禁止する型を列挙すると、新しい `Type` の variant（や、参照を持つ std の record）が黙って通る。`plain_part` は `src/const_rules.rs` の `value_part` と同じく、全 variant を wildcard なしの `match` で書く。判定は解決後の型に `stored_all` でかける（型の別名や record の field の中の `Rc`・利用者の `Drop`・`ref mut` も見つける）。`Type::has_user_drop` は浅い（その型自身の `Drop` だけ）ので、単独では足りない。値渡しの record が排他借用を持つと、呼び出し側の変数を書き換えられる（付録 E の e7）。`ref` の引数に `Rc` や排他借用を持つ型を渡すのは許す（`ref` は破棄せず、共有参照を通した排他借用の変更は `E1014`）ので、`own` の述語を `ref` の引数にかけない。
- **`own` は `pure def` の引数でだけ受理する。** 他の位置の `Own` を `resolve_type_with_kinds` に持ち込むと、結果の型・field・`let` の注釈・関数型にまで意味が広がる。関数宣言のループが引数の `Own` を剥がして印にし、残りの `Own` は `E1005` にする。
- **`TypeExprKind::Own` の走査漏れ。** `TypeExprKind::Reference` の出現（`docgen`・`regions`・`warnings`・`check`・`semantic`・`formatter`・`polymorph`・`parser`）と `syntax_codec.rs`（tag 15）に分岐を足し忘れると、`_ =>` で黙って無視される。`regions.rs` の `children` は `_ => Vec::new()` で終わるので、`Own` の arm がないと、中の領域注釈の検査が黙って抜ける。符号化の単体テストの数（`("TypeExprKind", 15)`）を 16 にする。A18 の手順 3 は同ファイルの更新を書き漏らしていたので、本チケットで直す。
- **`own` の保証は所有権検査に頼る。** `E1012`（消費後の読み）と `E1014`（借用中の移動）が今のままであることが前提で、`pure_def_own_keeps_the_ownership_rules` が守る。A17 の「最後の使用の移動」は値の意味を変えない（A17 の差分テストが守る）。
- **`own` の `export`。** 値渡しのバッファ（`own [i64]`・`own string`）は ABI で `E1008`。公開したい `pure def` は、無印か `ref` で書く。
- **`pure_rules` は `check` の子モジュール。** `src/lib.rs` に足すと、`TypeContext::stored_all`・`reaches`（`pub(super)`）を呼べない。`src/check.rs` の `#[path = "pure_rules.rs"] mod pure_rules;`（`recursion` の隣）で宣言し、`use super::*;` する（`const_rules.rs` と同じ）。
- **`FunctionDecl` の構築箇所。** `pure: false` の足し忘れは、コンパイルエラーで分かる。`const def` の構築と同じ箇所（`src/check.rs` の外部関数・定数、`src/polymorph.rs`）である。
- **`FRONTEND_FORMAT`。** `put`・`get` の順序を揃え、AST の符号化を変える手順（手順 2 と 3）ごとに 1 上げる（4 → 5 → 6）。上げ忘れると、古いキャッシュの AST が `pure` や `Own` を持たないまま読まれる。
- **`@alias pure`。** `CONTEXTUAL_KEYWORDS` に足すと、builder alias の名前に `pure` を使えなくなる。使用が 0 件であることを手順 1 で確かめる。
- **stack の深さ。** 走査は明示のスタックで書く（GUIDE §11.1）。再帰する関数を足したら、§3.1 の 4 つのテストを既定の stack で実行する。
- **上限を上げて通さない。** 上限（`E1017`）を上げたくなったら停止条件。

## 対象外

- Phase 2 以降の一覧（ジェネリック、関数型の純粋性、`Parallel`、最適化への利用、std への付与、`IO` アクションの構築、`@cpu`・`instance`、診断の強化、api-diff の補完後の比較）。
- `pure` の保証を最適化（共通部分式の除去、メモ化、並列化、`memory(...)`、デッドコードの除去）に使うこと。
- 停止性とトラップしないことの保証（`total`）。
- `pure` をラムダ・ローカルの関数・`class`・`instance` のメソッドに書くこと。
- `own` を通常の `def` の引数・結果の型・`let` の注釈などに書けるようにすること、`own` と edition `2027` の既定の変更、`ref mut` の扱い（A18 の D1・D2・D13・D16）。本チケットの `own` は `pure def` の引数だけで、`Rc`・利用者の `Drop` などを持つ型を `own` で受けることも対象外（D15）。

## 決定事項

### D1: `pure def` の引数は共有借用か `own` にする（`ref mut`・`mut` は不可）

- 決定: 引数は、共有借用（無印か `ref`）か、`own`（所有権の移動）だけにする。`ref mut` の引数と、`mut` を付けた引数は書けない。`own` の型は D15 の制限（平たいデータ）を満たす。
- 理由: 人間の指示。最初は「`pure` は純粋関数を維持するために `ref` のみを受け取るようにすること」（2026-10-13）、続けて「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」。`own` の保証は、「`own` と純粋性」の節の 5 つの経路と付録 E の HEAD での実測で確かめた。`ref mut` と `mut` は、呼び出し側の値を共有したまま書き換える手段なので許さない。`own` は呼び出し側の値を奪うが、奪われた値は読めず（`E1012`）、書き換えは別名を通らない（値が参照を持たない型に限る。D15）。
- 状態: **承認済み**（人間の指示。2026-10-13）。ただし `own` の部分は「保証されるのであれば」という条件つき。保証は D15 の制限つきで成り立つ（制限なしでは、値渡しの record が持つ排他借用が呼び出し側の変数を書き換え、`Rc` の `strong_count` と利用者の `Drop` が観測できて壊れる）。着手時の HEAD で付録 E を再現し、根拠が崩れていたら停止条件。

### D2: `ref` は省略でき、省略した引数は共有借用として扱う

- 決定: 書き手は引数の `ref` を省略できる。`own` を付けない引数は、共有借用として扱う。
- 理由: 人間の指示（2026-10-13）。「定義するときは `ref` を省略でき、引数はすべて `ref` 扱いにすること」。std の純粋な関数がすでにこの形で書かれている。`own` を許す指示（D1）で、所有権を受け取りたい引数だけが `own` と書く形になった。
- 状態: **承認済み**（人間の指示。2026-10-13）

### D3: 「`ref` 扱い」の型への表し方

- 決定: Copy で drop が要らない型の引数は、型の上では値のままにし、それ以外は `ref T` にする（`pure_parameter`）。意味は D2 のとおり共有借用である。
- 理由: スカラーの `ref` は、HEAD で算術にも戻り値にも使えず `E1003` になる（「現状」の表の 1・2）。値のままなら、読むだけの引数と区別できず、複製の費用もなく、`export` の ABI とコールバックの型も保てる。A18 の `R(T)`（小さい Copy の値は値渡し）と同じ理由である。
- 状態: 既定案（実装者はこの案に従う。人間が文字どおり「スカラーも `ref i64`」を求めたら、停止条件）

### D4: 構文は `pure def`（`const def` と同じ位置の文脈キーワード）

- 決定: `pure` を、`def` の直前にだけ書く文脈キーワードにする（予約語にしない）。`CONTEXTUAL_KEYWORDS` に登録する。すべての edition で受理する。
- 理由: 既存の修飾子（`export`・`private`・`const`）はすべて `def` の前にある。後ろに置く `def pure f` は、`def rec f` の位置に修飾子を置くことになり、`def rec pure f` のような順序の規則が新たに要る。GUIDE は新しい予約語を増やさない方針（予約語は新しい edition でだけ予約する）。文脈キーワードなら、edition を割らず、`pure` という名前の識別子も壊れない。
- 採らなかった案: `def pure f :: ..`（人間の例示の形）。`def pure :: 'a -> IO<'a>` のような `pure` という名前の関数とは、次のトークンが `::` かどうかで区別できるが、上の理由で `pure def` を選んだ。
- 状態: **承認済み**（人間の指示。2026-10-13「構文を `pure def` にしてよいです」）

### D5: 診断コードは `E1029` を 1 つだけ使う

- 決定: `pure def` の署名と本体の違反は、すべて `E1029` で、メッセージで区別する。位置の誤りは `E0002`、上限は `E1017`。
- 理由: `const def` の `E1026` と同じ作り。GUIDE §6.7 に従い、台帳（D-16 の表と D-48）に追記済み。
- 状態: 既定案（実装者はこの案に従う）

### D6: 純粋性の定義

- 決定: 「観測できる副作用がなく、結果が引数の値だけで決まる」。メモリの確保、トラップ、停止しないことは許す。効果の源は「本体の規則」の表の分類で、不明なものは拒否する（既定拒否）。
- 理由: トラップを保存する開発方針と、確保を禁じると std の大半が使えなくなることによる。
- 状態: 既定案（実装者はこの案に従う）

### D7: 検査は到達可能な関数の推論にし、呼び先に `pure` を要求しない

- 決定: `pure def` から到達する関数（std・同じパッケージの通常の `def`・生成した補助関数）の本体を調べ、`pure` の宣言は要求しない。
- 理由: 導出された `Eq`・`Ord`・`Hash`・`Display` の実体や std の関数は宣言を持たない。宣言を要求すると、`pure def` からほとんど何も呼べない。通常の `def` が不純になると、呼ぶ側の `pure def` が（連鎖つきで）拒否される。それが約束の意味である。
- 状態: 既定案（実装者はこの案に従う）

### D8: Phase 1 の署名は具体的で、関数・`dyn`・`Task` を持つ引数は不可

- 決定: 署名に型変数と制約を許さず、引数の型が関数・`dyn`・`Task`・`Owned.function` を（保持する値と参照の先を含めて）持つことを許さない。
- 理由: 外から関数値が入る道を塞ぐと、関数値の呼び出しを許しても健全になる（関数値はすべて到達する本体で作られる）。ジェネリックは `ref 'a` と型クラスの整合を要するので Phase 2。
- 状態: 既定案（実装者はこの案に従う）

### D9: 並行（`Task`・`Parallel`・`Async`）は不純とする

- 決定: Phase 1 は拒否する。
- 理由: コールバックが純粋なら結果は決定的だが、そのためには関数型の純粋性（Phase 2）が要る。`Matrix` の乗算が呼べない制限は、文書に書く。
- 状態: 既定案（実装者はこの案に従う）

### D10: 利用者の `Drop` を持つ型の値は不純とする

- 決定: 型が利用者の `Drop` を持つ値（それを含む型を含む）を持つ式を拒否する。
- 理由: 値が出る位置で暗黙に `drop` が呼ばれ、型付き IR にその呼び出しが現れないので、実体を調べられない。
- 状態: 既定案（実装者はこの案に従う）

### D11: 上限は到達する関数 4096、式 2,097,152

- 決定: 超えたら `E1017`（GUIDE D-19）。
- 理由: この解析は最適化ではなく**保証**なので、上限を超えても黙って受理せず、拒否する（A17 の解析は逆に、保守的に落とす）。
- 状態: 既定案（実装者はこの案に従う）

### D12: `@cpu`・`const`・`instance` のメソッドとは併用しない

- 決定: `@cpu` と `pure def`、`const` と `pure` は `E0002`。`instance`・`class` のメソッドには `pure` を書けない。
- 理由: `@cpu` は `def` の直後を前提にする構文で、併用の意味（CPU ごとの版の純粋性）は Phase 2 で決める。`const def` はすでに純粋。
- 状態: 既定案（実装者はこの案に従う）

### D13: A18 との関係

- 決定: `pure def` の引数は、edition に依らず本チケットの規則（無印は共有借用、`own` は移動）で補完する。A18 が入った後も、関数宣言のループは `function.pure` を先に判定し、`pure def` には A18 の補完をしない（二重に補完しない）。`own T` の構文は、本チケットの手順 3 が `pure def` の引数だけで先に導入する（D16）。A18 の D10（契約が固定された署名は補完しない）に `pure def` は含まない（`pure def` は自分で補完する）。
- 理由: `pure` は新しい構文なので、edition を割らずに無印の引数を読み取りにできる。A18 は、`IO` を使う関数や、消費する関数を含む `def` 全般についての別の決定である。
- 注: A18 の D1 の動機（`ref` を書く負担）のうち、純粋な関数については本チケットが先に解消する。A18 の優先度の見直しは人間が決める（A18 は要承認のまま）。A18 が先に実装済みなら、`TypeExprKind::Own` とその読み方を再利用し、手順 3 は `pure def` の規則（D15 と、`mut`・`ref mut` の拒否）だけを足す。
- 状態: 既定案（実装者はこの案に従う）

### D14: 蓄積引数の再帰は `own` で書く

- 決定: 蓄積引数の再帰（`go acc xs`）は、`acc` を `own` で受ける（D1）。`ref` で受けて `List.copy` などで複製する形は、`pure def` の書き方として勧めない（A17 の後も二乗のまま）。本チケットは性能を主張しない。
- 理由: 人間の指示（D1）で `own` を許したので、`ref` で受けて複製する形を強いられることがなくなった。HEAD では `own` の形もローカル可変のループも二乗で、A17 の後に線形になる見込みである（未測定。「現状」の表と付録 C）。
- 状態: 記録（人間への報告事項。実装者はこの案に従う。A17 の完了後に同じ測定をやり直して、見込みを実測に置き換える）

### D15: `own` の引数の型は平たいデータに限る（許す型を列挙する）

- 決定: `own` の引数の型は、保持するすべての型（record・union の field と payload、配列・リスト・`Vec`・固定長配列の要素、tuple の成分まで）が、数値（整数・浮動小数点・decimal・SIMD）、`bool`、`char`、`utf8char`、`unit`、`string`、`utf8string`、配列・リスト・`Vec`・固定長配列・tuple、利用者の `Drop` を持たない record・union のどれかでなければならない（`plain_part`。`Type` の全 variant を wildcard なしの `match` で列挙する）。参照（排他借用を保持する record を含む）、`Rc`・`Arc`・弱参照、利用者の `Drop` を持つ型、`extern` のハンドル、`Task`、`dyn`、関数は許さない。違反は署名の検査で `E1029`。`ref` の引数にはかけない。
- 理由: HEAD の実測（付録 E）。(1) 排他借用（`ref mut` の field）を持つ record を値渡しで受けた関数は、呼び出し側の変数を書き換えられる（e7: 出力 41。`ref` で受けると `E1014`。e8）。(2) `Rc` を値渡しで消費して捨てると、呼び出し側の `Rc.strong_count` が 2 から 1 に変わる（e1）。(3) 利用者の `Drop` は、呼び出し先の中で `drop` が走る（e4）。どれも「観測できる副作用がない」を壊す。`ref` で渡すなら、破棄されず、共有参照を通した排他借用の変更も拒否される。`Handle` の破棄が何もしないことは確かめていないので、保守的に拒否する（緩めるなら、確かめてから Phase 2 で）。許す型を列挙するのは、禁止する型を列挙すると、新しい `Type` の variant が黙って通るため（`const def` の `value_part` と同じ方針）。
- 状態: 既定案（実装者はこの案に従う。D1 の「保証されるのであれば」の条件を満たすための制限なので、緩めるには人間の承認が要る）

### D16: `own` の構文は A18 の D2 と同じ設計で、受理は `pure def` の引数だけにする

- 決定: `own T` を、型の位置の前置きとして読む（`TypeExprKind::Own`）。受理するのは `pure def` の署名の引数の型の前だけで、それ以外の位置は `E1005`。`own` は予約語にしない（`EDITION_KEYWORDS` は空のまま）。A18 が、受理する位置を通常の `def` の引数へ広げる。
- 理由: 人間の許可（D1）は `pure def` の引数についてである。通常の `def` に `own` を許すことは、edition `2027` の既定の変更と一体の A18 の D2 で、別の承認事項である。構文の実装（`TypeExprKind::Own`、読み方、符号化、走査箇所）は両チケットで共有できるので、先に本チケットで入れても、A18 が受理する位置を広げるだけで済む。
- 状態: 既定案（実装者はこの案に従う。A18 の D2 は要承認のまま）

## 付録

### 付録 A: 再現プログラム（HEAD `cef6c67f` で `tsuzuri run` と `check` を確認済み）

「現状」の表の 1〜16 は、いずれも `Main.tz` 1 ファイルのプロジェクトで、`def main :: unit -> i32 = \() -> 0`（または結果を出力する `main`）を足した次のソースである（着手時の HEAD で再確認すること。GUIDE §0.6）。

```tsuzuri
def add :: ref i64 -> ref i64 -> i64 = \a b -> a + b                  // 1: E1003 expected i64, found ref i64
def identity :: ref i64 -> i64 = \x -> x                              // 2: E1003
def cat :: ref string -> ref string -> string = \a b -> a + b         // 3: E1003 expected string, found ref string
def greet :: ref string -> ref string -> string = \a b -> $"hello {a} and {b}"   // 4: hello x and y
def doubled :: ref [i64] -> [i64] = \xs -> Array.map (\x -> x * 2) xs  // 5: 受理
def with_first :: ref [i64] -> i64 -> [i64] = \xs v -> Array.update_at 0 v xs   // 6: 受理（元の配列は変わらない）
def same :: ref [i64] -> [i64] = \xs -> xs                            // 7: E1003 expected [i64], found ref [i64]
def same_s :: ref string -> string = \s -> s                          // 8: E1003
record P { name: string, n: i64 }
def n_of :: ref P -> i64 = \p -> p.n                                  // 9: 受理（p.name.length も）
def name_of_bad :: ref P -> string = \p -> p.name                     // 10: E1012
def renamed_bad :: ref P -> string -> P = \p s -> { p with name = s } // 11: E1012
def first_or :: ref Maybe<[i64]> -> i64 = \m ->                       // 12: 受理
    match m with
    | Maybe.Some xs -> xs[0]
    | Maybe.None -> 0
def pick :: ref [i64] -> ref [i64] -> i64 = \idx data ->              // 13: 受理（結果 40: pick [0, 2] [10, 20, 30]）
    Array.fold (\acc i -> acc + data[i]) 0 idx
def apply :: ref (i64 -> i64) -> i64 -> i64 = \f x -> f x             // 14: E1005 only a function value can be called
export def total :: ref [i64] -> i64 = \xs -> Array.sum xs            // 15: 受理。`export def total :: [i64] -> i64` は E1008
def local_update :: ref [i64] -> i64 -> i64 = \xs n ->                // 16: 受理（local_update [1, 2, 3, 4] 10 は 23094）
    let mut ys = Array.copy xs
    for i in 0 .. (n - 1) do
        ys = Array.update_at (i % 4) (i * 3) ys
    ys[0] + ys[1] * 10 + ys[2] * 100 + ys[3] * 1000
```

複製の書き方（`clone_string (ref p.name)`、`Named { name: clone_string s, n: p.n }`、`String.concat [clone_string a, clone_string b]`）は、HEAD で受理され、`xyz12b7` を出力した。

### 付録 B: 検証した受理・拒否のソースと fixture

**拒否される本体**（すべて、`pure` なしの通常の `def` として HEAD で `check` が通る。`pure` を付けたとき、`E1029` と分類の語になる）:

```tsuzuri
record Ticket { id: i64 }

instance Drop<Ticket> {
    fn drop value = do! IO.write_line ("drop " + to_string value.id)
}

extern def host_tick :: i64 -> i64

def helper :: i64 -> i64 = \n ->
    do! IO.write_line "helper"
    n

def r_io :: i64 -> i64 = \n ->                  // outside world
    do! IO.write_line "hi"
    n

def r_debug :: i64 -> i64 = \n ->               // debug output
    Debug.print (ref n)
    n

def r_extern :: i64 -> i64 = \n -> host_tick n  // extern function

def r_callee :: i64 -> i64 = \n -> helper n + 1 // 'Main.helper' is not pure

def r_atomic :: i64 -> i64 = \n ->              // shared mutable state
    let a = Atomic.create 0
    Atomic.store (ref a) n
    Atomic.load (ref a)

def r_rc :: i64 -> i64 = \n ->                  // shared mutable state
    let r = Rc.new "abc"
    (Rc.get (ref r)).length + n

def r_drop :: i64 -> i64 = \n ->                // user Drop
    let t = Ticket { id: n }
    t.id

def r_parallel :: ref [i64] -> [i64] = \xs -> Parallel.map (\x -> x + 1) xs   // parallel operation

def r_task :: i64 -> i64 = \n ->                // task
    let t = task { n + 1 }
    Task.run t
```

**受理される本体**（`ref` を明示した通常の `def` として HEAD で `check` が通る。`Array.sort` は要素が Copy のときだけ）:

```tsuzuri
record Point { x: i64, y: i64 } deriving (Eq, Ord)

def largest_x :: ref [Point] -> i64 = \points ->
    let sorted = Array.sort points
    if sorted.length == 0 then 0 else (Array.at (sorted.length - 1) (ref sorted)).x

def squares :: i64 -> [i64] = \n -> Array.init n (\i -> i * i)
def total_squares :: i64 -> i64 = \n -> Array.sum (ref (squares n))
def rec fact :: i64 -> i64 = \n -> if n <= 1 then 1 else n * fact (n - 1)
def words :: ref string -> [string] = \text -> String.split text " "
def trimmed_length :: ref string -> i64 = \text -> (String.trim text).length
def next_random :: i64 -> i64 = \seed ->
    let generator = Random.pcg (seed as i64u) 54i64u
    match Random.pcg_next_u32 generator with
    | (value, _) -> value as i64
```

**fixture `tests/fixtures/pure_functions/Main.tz`**（`pure def` の形。`pure` を外して引数に `ref` を明示した通常の `def`（`own` の引数は、`pure` と `own` を外した値渡しの `def`）を HEAD で実行し、下の期待値と一致することを確認済み）:

```tsuzuri
record Point { x: i64, y: i64 } deriving (Eq, Ord)
record Named { name: string, n: i64 }

pure def square_sum :: i64 -> i64 = \n -> Array.sum (Array.init n (\i -> i * i))

pure def greeting_length :: string -> string -> i64 = \a b -> ($"hello {a} and {b}").length

pure def doubled_sum :: [i64] -> i64 = \xs -> Array.sum (Array.map (\x -> x * 2) xs)

pure def local_update :: [i64] -> i64 -> i64 = \xs n ->
    let mut ys = Array.copy xs
    for i in 0 .. (n - 1) do
        ys = Array.update_at (i % 4) (i * 3) ys
    ys[0] + ys[1] * 10 + ys[2] * 100 + ys[3] * 1000

pure def rec fact :: i64 -> i64 = \n -> if n <= 1 then 1 else n * fact (n - 1)

pure def largest_x :: [Point] -> i64 = \points ->
    let sorted = Array.sort points
    if sorted.length == 0 then 0 else (Array.at (sorted.length - 1) (ref sorted)).x

pure def renamed :: Named -> string -> Named = \p s -> Named { name: clone_string s, n: p.n }

pure def first_or :: Maybe<[i64]> -> i64 = \m ->
    match m with
    | Maybe.Some xs -> xs[0]
    | Maybe.None -> 0

pure def pick :: [i64] -> [i64] -> i64 = \idx data ->
    Array.fold (\acc i -> acc + data[i]) 0 idx

pure def with_first_own :: own [i64] -> i64 -> [i64] = \xs v -> Array.update_at 0 v xs

pure def rec build_list :: i64 -> own [|i64|] -> [|i64|] = \n acc ->
    if n == 0 then acc else build_list (n - 1) (List.cons n acc)

pure def shout :: own string -> string = \s -> $"{s}!"

pure def pick_own :: own string -> own string -> bool -> string = \a b first -> if first then a else b

export def t_square_sum :: i64 -> i64 = \n -> square_sum n
export def t_greeting :: i64 -> i64 = \n -> greeting_length (String.repeat n "x") (String.repeat (n + 1) "y")
export def t_doubled_sum :: i64 -> i64 = \n -> doubled_sum (Array.init n (\i -> i + 1))
export def t_local_update :: i64 -> i64 = \n -> local_update (Array.init 4 (\i -> i + 1)) n
export def t_fact :: i64 -> i64 = \n -> fact n
export def t_largest_x :: i64 -> i64 = \n -> largest_x (Array.init n (\i -> Point { x: (i * 7) % 11, y: i }))
export def t_renamed :: i64 -> i64 = \n ->
    let p = Named { name: String.repeat n "p", n: n }
    let q = renamed p (String.repeat (n + 2) "s")
    q.name.length + q.n
export def t_first_or :: i64 -> i64 = \n ->
    let m: Maybe<[i64]> = if n % 2 == 0 then Maybe.Some (Array.init 3 (\i -> n + i)) else Maybe.None
    first_or m
export def t_pick :: i64 -> i64 = \n -> pick [0, n % 3] (Array.init 3 (\i -> (i + 1) * (n + 1)))
export def t_with_first_own :: i64 -> i64 = \n ->
    let xs = Array.init 4 (\i -> i + n)
    let ys = with_first_own xs 9
    xs[0] * 100 + ys[0] * 10 + ys[1]
export def t_build_list :: i64 -> i64 = \n ->
    let empty: [|i64|] = [||]
    List.fold (\acc x -> acc * 3 + x) 0 (ref (build_list n empty))
export def t_shout :: i64 -> i64 = \n -> (shout (String.repeat n "z")).length
export def t_pick_own :: i64 -> i64 = \n ->
    (pick_own (String.repeat n "a") (String.repeat (n + 1) "b") (n % 2 == 0)).length
```

`if` の式を `ref` の引数へ直接渡すと `E1003`（型が決まらない）になるので、`t_first_or` は `let m: Maybe<[i64]>` に束縛している。`t_with_first_own` は、呼び出したあとで `xs` を読む（`own` で渡しても、Copy の値は複製されて、呼び出し側の配列は変わらない。付録 E の e2）。

suite `pure_functions` の期待値（独立した JavaScript。`n` は BigInt、0 から 12。`t_fact` は 0・1・5・10・20）:

```js
const range = (n) => Array.from({ length: Number(n) }, (_, i) => BigInt(i));
const square_sum = (n) => range(n).reduce((s, i) => s + i * i, 0n);
const greeting = (n) => 2n * n + 12n;                       // "hello " + x*n + " and " + y*(n+1)
const doubled_sum = (n) => n * (n + 1n);
const local_update = (n) => {
  const ys = [1n, 2n, 3n, 4n];
  for (const i of range(n)) ys[Number(i % 4n)] = i * 3n;
  return ys[0] + ys[1] * 10n + ys[2] * 100n + ys[3] * 1000n;
};
const fact = (k) => range(k + 1n).slice(1).reduce((r, i) => r * i, 1n);
const largest_x = (n) => range(n).reduce((m, i) => { const x = (i * 7n) % 11n; return x > m ? x : m; }, 0n);
const renamed = (n) => 2n * n + 2n;                         // (n + 2) + n
const first_or = (n) => (n % 2n === 0n ? n : 0n);
const pick = (n) => (n + 1n) + ((n % 3n) + 1n) * (n + 1n);
const with_first_own = (n) => 101n * n + 91n;               // n * 100 + 9 * 10 + (n + 1)
const build_list = (n) => range(n + 1n).slice(1).reduce((acc, x) => acc * 3n + x, 0n);   // [1..n] の左畳み込み
const shout = (n) => n + 1n;
const pick_own = (n) => (n % 2n === 0n ? n : n + 1n);
```

HEAD の出力で、`n = 0..12` の列が、上の計算と 13 行すべて一致した。`ref` の 8 つの値は、`pure` を外して引数に `ref` を明示した版で、`n = 12` の行は `506 36 156 36294 10 26 12 26`、`t_fact` は `1 1 120 3628800 2432902008176640000`。`own` の 4 つの値は、`pure` と `own` を外した値渡しの `def` で、`n = 12` の行は `1303 398574 13 12`（`n = 0` の行は `91 0 1 0`、`n = 1` は `192 1 2 2`）。

### 付録 C: 蓄積引数の測定（HEAD、Apple M1 Max、`-O3`、n = 40,000）

`tsuzuri build . -O3 -o out` のあと `./out` を 3 回。ソースは次のとおり（`def rec` が要る。空リストには型注釈が要る）。

```tsuzuri
def rec build :: i64 -> [|i64|] -> [|i64|] = \n acc ->
    if n == 0 then acc else build (n - 1) (List.cons n acc)
// main: let empty: [|i64|] = [||] ; List.length (build 40000 empty)    14.85 / 14.47 / 14.55 s

def rec build :: i64 -> ref [|i64|] -> [|i64|] = \n acc ->
    if n == 0 then List.copy acc else build (n - 1) (List.cons n (List.copy acc))
// 20.12 / 18.49 / 18.01 s

def build :: i64 -> [|i64|] = \n ->
    let mut acc: [|i64|] = [||]
    for i in 1 .. n do
        acc = List.cons i acc
    acc
// 15.40 / 14.64 / 14.64 s
```

### 付録 D: 出典（2026-10-13 に検索で確認）

- D 言語の `pure`: 引数を通す以外ではグローバルな可変状態にアクセスしない関数。引数が可変でなければ（可変な間接参照を持たなければ）strongly pure で、結果のキャッシュなどの最適化が許される。引数を書き換える関数は weakly pure（<https://dlang.org/spec/function.html#pure-functions>、<https://tour.dlang.org/tour/en/gems/functional-programming>）。本チケットの `pure def` は、引数が共有借用か、別名のない所有値（`own`）で、呼び出し側の値を書き換える経路がないので、D の strongly pure に相当する性質を持たせる（この対応づけは本チケットの判断で、D の仕様の記述ではない）。

### 付録 E: `own` の保証の実測（HEAD `cef6c67f`、2026-10-13、`target/release/tsuzuri run`）

`pure def` はまだないので、`own` の引数と同じ型になる値渡しの通常の `def` で確かめた。`pure def` の `own T` は、補完で値渡しの `T` になる（IR が同じ）ので、同じ結果になる。着手時の HEAD で再現し、結果が違ったら D1 の条件が崩れたとして停止する。

各ソースは `Main.tz` 1 つのプロジェクト（別々のディレクトリ）である。

```tsuzuri
// e1: Rc を値渡しで消費する
def consume :: Rc<string> -> i64 = \_r -> 0

def main :: unit -> i32 = \() ->
    let a = Rc.new "x"
    let b = Rc.share (ref a)
    let before = Rc.strong_count (ref a)
    let _ = consume b
    let after = Rc.strong_count (ref a)
    do! IO.write_line (to_string before + " " + to_string after)
    0
```

```tsuzuri
// e5: 引数を `ref Rc<string>` にして借用で渡す
def peek :: ref Rc<string> -> i64 = \_r -> 0

def main :: unit -> i32 = \() ->
    let a = Rc.new "x"
    let b = Rc.share (ref a)
    let before = Rc.strong_count (ref a)
    let _ = peek (ref b)
    let after = Rc.strong_count (ref a)
    do! IO.write_line (to_string before + " " + to_string after)
    0
```

```tsuzuri
// e2: Copy の配列を値渡しで受けて書き換える
def bump :: [i64] -> [i64] = \xs -> Array.update_at 0 99 xs

def main :: unit -> i32 = \() ->
    let a = [1, 2, 3]
    let b = bump a
    do! IO.write_line (to_string (a[0]) + " " + to_string (b[0]))
    let c = [4, 5, 6]
    let d = bump c
    do! IO.write_line (to_string (d[0]))
    0
```

```tsuzuri
// e3: 非 Copy の string を値渡しで消費したあとに読む
def eat :: string -> i64 = \s -> s.length

def main :: unit -> i32 = \() ->
    let s = "abc"
    let n = eat s
    do! IO.write_line (to_string (n + s.length))
    0
```

```tsuzuri
// e4: 利用者の Drop を値渡しで消費する
record Ticket { id: i64 }

instance Drop<Ticket> {
    fn drop value = do! IO.write_line ("drop " + to_string value.id)
}

def consume :: Ticket -> i64 = \t -> t.id + 1

def main :: unit -> i32 = \() ->
    let t = Ticket { id: 7 }
    do! IO.write_line "before"
    let n = consume t
    do! IO.write_line ("after " + to_string n)
    0
```

```tsuzuri
// e6: 借用が生きている間に、値渡しで消費する
def eat :: string -> i64 = \s -> s.length

def main :: unit -> i32 = \() ->
    let s = "abc"
    let r = ref s
    let n = eat s
    do! IO.write_line (to_string (n + r.length))
    0
```

```tsuzuri
// e7: 排他借用（ref mut の field）を持つ record を値渡しで受け、呼び出し側の変数を書き換える
record Counter {r} { value: ref mut {r} i64, step: i64 }

def bump :: Counter -> i64
fn bump c =
    deref c.value = deref c.value + c.step
    c.step

def main :: unit -> i32 = \() ->
    let mut total = 1
    let counter = Counter { value: ref mut total, step: 20 }
    let s = bump counter
    do! IO.write_line (to_string (s + total))
    0
```

```tsuzuri
// e8: e7 と同じ record を `ref Counter` で受ける
record Counter {r} { value: ref mut {r} i64, step: i64 }

def bump :: ref Counter -> i64
fn bump c =
    deref c.value = deref c.value + c.step
    c.step

def main :: unit -> i32 = \() ->
    let mut total = 1
    let counter = Counter { value: ref mut total, step: 20 }
    let s = bump (ref counter)
    do! IO.write_line (to_string (s + total))
    0
```

| # | 問い | HEAD の結果 | 結論 |
| --- | --- | --- | --- |
| e7 | 排他借用を持つ record を値渡しで受けた関数は、呼び出し側の変数を書き換えられるか | `41`（`total` が 1 から 21 になり、`20 + 21`） | 書き換えられる。`own` の型から参照を除く（D15） |
| e8 | 同じ record を `ref Counter` で受けると | `E1014` cannot mutate or exclusively reborrow through a shared reference | 書き換えられない。`ref` の引数には許す |
| e1 | `Rc` を値渡しで消費して捨てると、呼び出し側から見える状態が変わるか | `2 1`（`Rc.strong_count` が前 2、後 1） | 変わる。`own` の型から `Rc`・`Arc`・弱参照を除く（D15） |
| e5 | `ref Rc<string>` で渡すと | `2 2` | 変わらない。`ref` の引数には許す |
| e4 | 利用者の `Drop` を持つ値を値渡しで消費すると | `before`、`drop 7`、`after 8` の順（`drop` が呼び出し先の中で走る） | 観測できる。`own` の型から利用者の `Drop` を持つ型を除く（D15） |
| e2 | Copy の配列を値渡しで受けて書き換えると、呼び出し側の配列が変わるか | `1 99` と `99`（呼び出し側の `a[0]` は 1 のまま） | 変わらない |
| e3 | 非 Copy の `string` を値渡しで消費したあとに読むと | `E1012` use of moved or partially moved value `s` | 奪われた値は読めない |
| e6 | 借用が生きている間に値渡しで消費すると | `E1014` access conflicts with a live borrow | 別名を通した書き換えは起きない |

`Type::Handle`（`extern type` のハンドル）の破棄が何もしないことは確かめていない。保守的に `own` の型から除く（D15）。

平たいデータの値渡しが期待どおりの値を返すことは、付録 B の fixture の `own` の 4 つの関数（`pure` と `own` を外した値渡しの `def`）で、`n = 0..12` の 13 行すべてが独立した JavaScript の計算と一致した。

## 実装と検証（2026-10-24）

- 基盤は最終 A18 の `8f1df9d5`。検証した `b3343ed1` からの差は文書のみで、restack 前後の
  src／std／tests／benchmarks／scripts／vsc と Cargo manifest・lock は byte 同一である。
  Phase 1 と全後続項目を実装した。構文・codec・cache、読み取り補完、
  plain-data の `own`、generic の保留条件と特殊化後の再検証、純粋な関数型と呼び出し段階、
  class／instance・CPU variant、遅延 IO の構築、閉じた明示 Task／Parallel、std の Char・PCG 契約に対応する。
  通常の callback API を純粋 callback 専用に狭めず、通常の `def` の `own` は A18 の契約を維持した。
- `api-diff --effective` は依存込みの解決後の契約を比較し、既定の構文だけの比較は変えない。
  効果検証は wildcard なしの分類・既定拒否・従来の上限を保ち、GPU の既存検査からも同じ問い合わせを使う。
  G31 の字句的な callers／impact／item range は pure 宣言を認識し、識別子 `pure` は予約しない。
- 最適化は、明示 pure と既存の `memory(none)` の両方を満たす整数呼び出しの同一基本ブロック内の再利用だけ。
  最初の呼び出しを残し、捕捉・外部メモリ・浮動小数・所有結果・CPU multiversion を対象にしない。
  自動 offload、共有された所有結果、永続的な memo cache、投機実行、停止・トラップの保証は対象外。
  速度向上は未計測であり主張しない。依存、stack、解析上限、fast-math は変更していない。
- fmt・全 target Clippy（warnings denied）・build が成功。初回の全 Rust は **1,816 passed / 2 skipped**、
  別途 targets **12 passed**。付録 E は旧 compiler・通常 def・pure def の **8 プログラム／36 検査**が一致した。
  保存した 2026 の **114 プログラム**は旧・現在・現在の再実行で status／stdout／stderr が一致し、
  全 LLVM IR が byte 一致した。primitives は native／WASM × O0／O3 が成功。
- 変更したリファレンス **12 ページ／443 links／75 checked examples／134 native runs（O0/O3）／1 test project** が成功。
  `node tests/features.mjs target/debug/tsuzuri` は **20,012 cases passed**。pure_functions の **260 ケース**、
  pure_parallel の **12 ケース**と各 trap、native／WASM × O0／O3、live == 0 と import の検査を含む。
  `e2e`、`codegen_units`（26 cases）、`frontend_cache`、`lsp_sessions`（4 sessions／features）、`docgen`、`gpu` が成功。
  GPU は CPU 参照の整数 783・relaxed f32 1,315 検査、WGSL shape 9 と上限外 5 の拒否であり、実 GPU の実行・加速は主張しない。
  runtime include は **58 files** が成功。frontend format は A19 の型・class の codec を含む **9**。
  F14 は新しい直列化構文を足さない。7 件の統合検証と最終 head の CI／Copilot レビューは各 PR で追跡する。
- 受入で見つけた qualified namespace／class method の pure→通常 callable 変換は共通の finish path に統一した。
  shared view の中の排他参照は読み取りを許し、書き込みは所有権検査の E1014 を保つ。
  隠れた通常 callback は E1029 のまま。引用符付き SSA 定義に `=` がある spill escape も保守的に拒否する。
- F14 の新しい `Builtin::AllocCounter` の Debug 分類と拒否回帰は F14 側の互換修正で扱う。
  この A19 単独の base に存在しない variant の仮置きや、不純効果の見落としを隠す wildcard は足さない。

### PR #50 のレビュー修正と再受入（2026-10-24）

- Copilot の 4 件を修正前に失敗する回帰で再現した。parser と class の instance／既定実装で
  正規化を共有し、宣言した pure の段階で全引数を束縛しない実装を `E1029` とする。
  eta 展開で途中の計算・トラップを遅らせず、通常の `def` の途中評価は変えない。
  途中評価が必要なら factory の戻り段階を括弧で宣言し、返る関数も純粋なら結果に `pure` を書く。
  本物の factory の戻り lambda は、brace の有無にかかわらず余分なメソッド引数に取り込まない。
- 推論する `#function` と明示した `(#function: Type)` の両方で、対象の純粋性だけを通常の要求へ弱める。
  対象自身の署名・引数規約・factory の物理的段階を平坦化せず、逆向きは拒否する。
  `--effective` の extern 引数は全体の `own` だけを外し、入れ子・結果の不正な所有指定は拒否する。
- focused 104 件、fmt・全 target Clippy・build が成功。新しい fixture モジュールを Rust 側にも登録した後、
  clean な全 Rust **1,832 passed / 2 skipped（targets 12 件を含む）** が成功した。
  pure_functions **302 ケース**と pure_parallel **12 ケース**、trap・live/import 検査が native／WASM × O0／O3 で成功した。
  新規 42 ケースは未括弧の class instance／既定 factory、純粋な戻り段階、推論／明示したモジュール関数制約と factory の実行を含む。
  `e2e`、`frontend_cache`、`lsp_sessions` の再実行も成功した。
- 変更した manual 4 ページは **126 links／32 checked examples／58 native runs／1 test project** が成功した。
  guide-lite は **85 references／0 failures**。初回の全 feature・frozen IR・数値／GPU 参照の記録は上記のとおりで、
  今回の再検証範囲や実 GPU の実行と混同しない。最終 head の CI と再レビューは PR で追跡する。

### Windows の標準ライブラリ入力（2026-10-24）

- 旧 head `559058e8` の Windows x64／arm64 CI は、標準ライブラリ自身の実効 API を読む回帰で
  `std\Arena.tz` を不正な Std 入力として拒否していた。`load_for_docs` が認識した標準ライブラリに対して、
  実ファイルのパスは保持し、解析用の仮想名を全 OS 共通の `std/Name.tz` にする。
  Std 入力の検査を緩めたり、通常の利用者のファイル名の区切りを置換したりしない。
- 仮想パスの回帰を明示し、API／std／doc 関連 **62 tests passed**、全 target Clippy・fmt が成功した。
  usage は **62 links／5 checked examples／4 native runs／1 test project** を再検証した。
  native Windows の成否は、修正後の最終 head の x64／arm64 CI で判定する。

### 実効 API の inline 制約（2026-10-24）

- 追加 Copilot 指摘で、`'a -> bool` と `Ord<'a> -> bool` が同じ解決後の型になり、
  明示節だけの snapshot が inline／alias 経由の呼び出し側の要求を落としていた。
  先に追加した回帰のうち、要求変更と表記の同値性の 2 件が旧実装で失敗することを確認した。
- `CheckedFunction.constraints` を既存の instance／superclass 正規化に通し、残余の class 要求を保存する。
  ソースをもう一度解釈する独立した検査器は作らず、具体型で満たされた本体内の要求を API へ漏らさない。
  モジュール関数の明示制約を残し、公開名への変換・型変数の改名後に集合として整列・重複除去する。
  報告に使うソース表記と既存の制約変更の保守的な分類は維持する。
  重複除去は実効比較だけに限定し、既定の構文比較が重複を差分とする従来動作も回帰で固定する。
- 要求の追加／除去、alias の内部変更、inline／明示節／重複の同値性、alpha-renaming と順序、
  superclass と本体の要求、ユーザー定義 class、モジュール関数の要求保持を回帰で確認した。
  API 42＋docs 8＋std 7 の **57 tests passed**、全 target Clippy・fmt が成功した。
  最終差分で全 Rust **1,835 passed／2 skipped（targets 12 を含む）**、dev build、
  usage **62 links／5 checked examples／4 native runs／1 test project**、docgen、guide-lite **85／0** を再確認した。

### 推論・公開 alias の要求（2026-10-24）

- `ce9ec39f` の再レビューは前項の inline 制約修正を解決済みと確認したが、overview に
  推論された `#distance` の署名と、単独の公開 alias の制約という 2 件の追加指摘があった。
  表示の「No open issues」だけで完了扱いせず、両方が旧実装で失敗する回帰を確認した。
- モジュール関数の要求は `CheckedFunction.members` の推論・伝播済みの署名を保存する。
  型付きの使用で精密化された署名なしの宣言は重ねず、具体 receiver で解決済みの要求は除く。
  公開 alias 自体も既存の alias 展開と inline 制約抽出を使って要求を保存し、使用箇所に依存させない。
- 同じ snapshot 境界で、所有権が推論する Copy 要求も欠落していたことを回帰で確認した。
  API の収集を Copy 推論と class 要求の固定点伝播の完了後、generic template の消費直前へ移す。
  通常の semantic index・非推奨警告の収集時点、制約ソルバー、所有権・特殊化の動作は変えない。
- 推論された arity 変更、明示／推論の同値性、alpha-renaming、private helper 経由の伝播、
  具体 receiver の要求を漏らさないこと、単独／連鎖 alias、Copy の直接／間接要求を確認した。
  API 46＋docs 8＋std 7 の **61 tests passed**、全 target Clippy・fmt が成功した。
  最終差分で全 Rust **1,839 passed／2 skipped（targets 12 を含む）**、dev build、
  pure **302**＋parallel **12**（native／WASM × O0／O3）、E2E・frontend cache・LSP 4 件・docgen、
  usage **62 links／5 checked examples／4 native runs／1 test project**、guide-lite **85／0** を確認した。

### 未使用 generic の純粋関数値（2026-10-24）

- `c0318d9c` の Copilot review `5637157267` で、未使用の generic template が返す明示的な
  純粋 lambda の `Debug.print` が検査を逃れる指摘 `4366251465` を確認した。
  factory の実行と戻る callback の実行を区別する規則は保ち、明示した純粋関数値自体の検査を特殊化前にも行う。
- 既存の本体走査と幅優先探索を共有し、lambda・通常／generic の関数参照・builtin・捕捉先を検査する。
  関数値の証明根は実関数のキャッシュと分離するため、callback が外側の effectful factory に戻っても効果を見落とさない。
  閉じた局所再帰・相互再帰は alias と本体の重複走査を防いで扱い、副作用を含む循環は拒否する。
- 回帰は通常／純粋な factory、入れ子の純粋 lambda、既知／不透明な捕捉先、純粋／副作用を持つ
  局所再帰・相互再帰、外側の factory への再入を含む。通常の戻り callback の副作用と、
  通常 factory 自身の途中の副作用は引き続き許す。解析上限・評価順序・実行時の表現は変えない。
- 最終差分で全 Rust **1,841 passed／2 skipped（targets 12 を含む）**、fmt・全 target Clippy・dev build、
  pure **302**＋parallel **12**（native／WASM × O0／O3）、E2E・frontend cache・LSP 4 件・docgen が成功した。
  functions／usage の **2 pages／93 links／29 checked examples／52 native runs／1 test project**、
  guide-lite **85／0** も確認した。初回の全 feature・凍結 IR の記録とは再実行範囲を区別する。

### 字句的な caller の文脈キーワード（2026-10-24）

- 統合 head `50b52943` の review `5637515786` で、関数 `pure` の caller に無関係な
  `pure def` の修飾子を数える指摘 `4366525619` を再現した。`referenceLines` で既存の
  宣言の正規表現を使って接頭辞だけを除き、callers／impact に共通の根で修正した。
- 回帰は純粋・private・export 宣言の誤参照を除き、同じ宣言行の本体の `pure x` と `X.pure x`、
  正しい行範囲と推移的な impact を保持する。コンパイラ・std・Rust／数値の入力は変更しない。
  ユーザー向け説明は AI 作業コストのページにあるため、言語リファレンスの意味・例には変更がない。
- 修正前の回帰失敗を確認後、Node **20.19.6** と **24.21.0** で AI 全 suite を実行し、
  それぞれ **44 passed／0 failed／5 capability skips**、guide-lite **85／0**、fmt・diff check が成功した。
  Rust／std の入力は `50b52943` と同一なので、その全 Rust **1,842／2 skipped** と
  最終 F14 統合の **1,864／2 skipped** の受入を保持し、JS 修正後に再実行した数とは混ぜない。

### generic な純粋性の特殊化義務（2026-10-24）

- `b5114dec` の review `5637560496` は「No open issues」と表示しつつ、overview に
  `own 'a` への `pure` 追加を誤って non-breaking にする追加指摘を含んでいた。旧実装の比較の失敗と、
  通常版では受理する `inspect (Rc.new 1)` が純粋版では `E1029` になることを回帰で確認した。
- 所有引数だけでなく、共有の型変数で通常 callback を受ける呼び出しや、結果だけが generic な
  `Maybe<'a>` を利用者 Drop の型に特殊化する呼び出しでも、追加した純粋性の証明義務が現れることを確認した。
  入力の一部だけを記録せず、型検査済みの generic template の純粋性・所有条件の特殊化義務を
  既存の checked contract に保存する。新しい public 制約クラスや構文は作らない。
- `--effective` はその義務の新設を、ABI が同じでも保守的に breaking とする。
  型変数の改名、alias、入れ子の所有データを検査し、特殊化義務のない具体的な共有型・平たい所有型では
  互換な純粋性の追加を保つ。既定の構文比較、ソース表示、実際の型検査・所有権・特殊化・コード生成は変えない。
- 最終差分で全 Rust **1,843 passed／2 skipped（targets 12 を含む）**、fmt・全 target Clippy・dev build、
  pure **302**＋parallel **12**＋map_set_chunks **33**（native／WASM × O0／O3）、frontend cache・LSP 4 件・docgen が成功した。
  AI 全 suite は Node20／24 それぞれ **44／0 failed／5 capability skips**、usage は
  **62 links／5 checked examples／4 native runs／1 test project**、guide-lite **85／0** を確認した。

### 確認コマンド（最終受入）

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo build --locked`。
- `cargo nextest run --locked --test-threads 2 -E 'not binary(targets)'` と targets の別実行（短い絶対 TMPDIR）。
- `node tests/features.mjs target/debug/tsuzuri`（全 suite）、同じ dev バイナリで `e2e`・`primitives`・
  `codegen_units`・`frontend_cache`・`lsp_sessions`・`docgen`・`gpu`。
- `node tests/ai_cost.mjs target/debug/tsuzuri pure_items`（新しい字句的 callers／impact／item range の回帰 1 件）。
  続けて全 `tests/ai_cost.mjs` を Node 20 と Node 24 で実行し、両方 **43 passed / 0 failed / 5 skipped**。
  未実装の related spans・fix・G21/G27/G28 artifacts・kb commands・G29 MCP tools/list の能力検査を skip として明記し、
  実行できたものとして数えていない。
- `node scripts/check-docs.mjs <下記の12ページ>`（`TSUZURI_BIN=target/debug/tsuzuri` を絶対パスで指定）。
- `sh scripts/check-runtime-includes.sh`。旧 compiler と frozen 2026 corpus の比較は一時 runner で行い、
  出力・診断・IR の hash を比較してから現在の compiler をもう一度実行した。
- 重いローカル確認は共有 slot 内で実施した。相対 TMPDIR を使って cwd を変える E2E が失敗したため、
  短い絶対パスに直して全 suite を通した。共有キャッシュ・上限・stack は変更していない。

### 更新した言語リファレンス

- [_tsuzuri/language-reference/built-in-types-and-modules/char.md](../../_tsuzuri/language-reference/built-in-types-and-modules/char.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/gpu.md](../../_tsuzuri/language-reference/built-in-types-and-modules/gpu.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/io.md](../../_tsuzuri/language-reference/built-in-types-and-modules/io.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/random.md](../../_tsuzuri/language-reference/built-in-types-and-modules/random.md)
- [_tsuzuri/language-reference/compiler/diagnostics.md](../../_tsuzuri/language-reference/compiler/diagnostics.md)
- [_tsuzuri/language-reference/compiler/option.md](../../_tsuzuri/language-reference/compiler/option.md)
- [_tsuzuri/language-reference/compiler/usage.md](../../_tsuzuri/language-reference/compiler/usage.md)
- [_tsuzuri/language-reference/index.md](../../_tsuzuri/language-reference/index.md)
- [_tsuzuri/language-reference/ownership-and-memory/borrowing.md](../../_tsuzuri/language-reference/ownership-and-memory/borrowing.md)
- [_tsuzuri/language-reference/ownership-and-memory/ownership.md](../../_tsuzuri/language-reference/ownership-and-memory/ownership.md)
- [_tsuzuri/language-reference/values-and-functions/functions.md](../../_tsuzuri/language-reference/values-and-functions/functions.md)
- [_tsuzuri/language-reference/values-and-functions/keywords.md](../../_tsuzuri/language-reference/values-and-functions/keywords.md)
- [_tsuzuri/language-reference/types-and-type-inference/constraints.md](../../_tsuzuri/language-reference/types-and-type-inference/constraints.md)

### 変更ファイル

- [README.md](../../README.md)
- [_features/A20-effect-summary-and-checked-attributes.md](../A20-effect-summary-and-checked-attributes.md)
- [_features/A21-contracts-requires-ensures.md](../A21-contracts-requires-ensures.md)
- [_features/A23-integer-overflow-intent.md](../A23-integer-overflow-intent.md)
- [_features/B10-trap-backtrace-and-domains.md](../B10-trap-backtrace-and-domains.md)
- [_features/E15-trust-boundary-and-capabilities.md](../E15-trust-boundary-and-capabilities.md)
- [_features/F15-sequential-equivalent-data-parallel.md](../F15-sequential-equivalent-data-parallel.md)
- [_features/GUIDE.md](../GUIDE.md)
- [_features/README.md](../README.md)
- [_features/_completed/A18-parameter-conventions.md](A18-parameter-conventions.md)
- [_features/_completed/A19-pure-functions.md](A19-pure-functions.md)
- [_perfs/_completed/PR01-ownership-facts.md](../../_perfs/_completed/PR01-ownership-facts.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/char.md](../../_tsuzuri/language-reference/built-in-types-and-modules/char.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/gpu.md](../../_tsuzuri/language-reference/built-in-types-and-modules/gpu.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/io.md](../../_tsuzuri/language-reference/built-in-types-and-modules/io.md)
- [_tsuzuri/language-reference/built-in-types-and-modules/random.md](../../_tsuzuri/language-reference/built-in-types-and-modules/random.md)
- [_tsuzuri/language-reference/compiler/diagnostics.md](../../_tsuzuri/language-reference/compiler/diagnostics.md)
- [_tsuzuri/language-reference/compiler/option.md](../../_tsuzuri/language-reference/compiler/option.md)
- [_tsuzuri/language-reference/compiler/usage.md](../../_tsuzuri/language-reference/compiler/usage.md)
- [_tsuzuri/language-reference/index.md](../../_tsuzuri/language-reference/index.md)
- [_tsuzuri/language-reference/ownership-and-memory/borrowing.md](../../_tsuzuri/language-reference/ownership-and-memory/borrowing.md)
- [_tsuzuri/language-reference/ownership-and-memory/ownership.md](../../_tsuzuri/language-reference/ownership-and-memory/ownership.md)
- [_tsuzuri/language-reference/values-and-functions/functions.md](../../_tsuzuri/language-reference/values-and-functions/functions.md)
- [_tsuzuri/language-reference/values-and-functions/keywords.md](../../_tsuzuri/language-reference/values-and-functions/keywords.md)
- [_tsuzuri/language-reference/types-and-type-inference/constraints.md](../../_tsuzuri/language-reference/types-and-type-inference/constraints.md)
- [benchmarks/ai/lib/derive.mjs](../../benchmarks/ai/lib/derive.mjs)
- [docs/architecture.md](../../docs/architecture.md)
- [docs/language.md](../../docs/language.md)
- [docs/stability.md](../../docs/stability.md)
- [src/api_diff.rs](../../src/api_diff.rs)
- [src/call_specialization.rs](../../src/call_specialization.rs)
- [src/check.rs](../../src/check.rs)
- [src/closures.rs](../../src/closures.rs)
- [src/constants.rs](../../src/constants.rs)
- [src/derive.rs](../../src/derive.rs)
- [src/docgen.rs](../../src/docgen.rs)
- [src/driver.rs](../../src/driver.rs)
- [src/formatter.rs](../../src/formatter.rs)
- [src/frontend_cache.rs](../../src/frontend_cache.rs)
- [src/gpu.rs](../../src/gpu.rs)
- [src/llvm.rs](../../src/llvm.rs)
- [src/llvm_facts.rs](../../src/llvm_facts.rs)
- [src/llvm_frame.rs](../../src/llvm_frame.rs)
- [src/llvm_traps.rs](../../src/llvm_traps.rs)
- [src/main.rs](../../src/main.rs)
- [src/ownership.rs](../../src/ownership.rs)
- [src/parser.rs](../../src/parser.rs)
- [src/polymorph.rs](../../src/polymorph.rs)
- [src/pure_rules.rs](../../src/pure_rules.rs)
- [src/recursive.rs](../../src/recursive.rs)
- [src/regions.rs](../../src/regions.rs)
- [src/semantic.rs](../../src/semantic.rs)
- [src/semantic_api.rs](../../src/semantic_api.rs)
- [src/syntax.rs](../../src/syntax.rs)
- [src/syntax_codec.rs](../../src/syntax_codec.rs)
- [src/warnings.rs](../../src/warnings.rs)
- [std/Char.tz](../../std/Char.tz)
- [std/Random.tz](../../std/Random.tz)
- [tests/ai_cost.mjs](../../tests/ai_cost.mjs)
- [tests/api_diff.rs](../../tests/api_diff.rs)
- [tests/features.mjs](../../tests/features.mjs)
- [tests/fixtures/pure_functions/Classes.tt](../../tests/fixtures/pure_functions/Classes.tt)
- [tests/fixtures/pure_functions/Main.tz](../../tests/fixtures/pure_functions/Main.tz)
- [tests/fixtures/pure_functions/Members.tz](../../tests/fixtures/pure_functions/Members.tz)
- [tests/fixtures/pure_parallel/Main.tz](../../tests/fixtures/pure_parallel/Main.tz)
- [tests/gpu.rs](../../tests/gpu.rs)
- [tests/polymorphism.rs](../../tests/polymorphism.rs)
- [tests/pure_functions.rs](../../tests/pure_functions.rs)
- [tests/pure_scalar_reuse.rs](../../tests/pure_scalar_reuse.rs)
- [tests/std_argument_order.rs](../../tests/std_argument_order.rs)
