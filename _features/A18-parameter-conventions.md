# A18: 引数規約 — 読み取りを既定にする（`own`、edition 2027）

| 項目 | 内容 |
| --- | --- |
| ID | A18 |
| 優先度 | P2 |
| 規模 | XL（Phase 1 は L） |
| 依存 | A15（done）、G19（done。edition）、(A17)。A17 は弱い依存: A17 を先に入れると、`own` を付けた Copy コレクションの引数と更新の複製が減る。着手の順は A17 → A18 を推奨する。[A19](A19-pure-functions.md) とは、`own` の構文（`TypeExprKind::Own`）を共有する。A19 は `pure def` の引数だけで先に導入し（A19 の D16）、本チケットは受理する位置を通常の `def` の引数へ広げる。どちらが先でも、後のほうは再実装せず、受理する位置の規則だけを足す（D11） |
| 後続 | Phase 2（移行ツールと std・fixture の移行、`LATEST` の更新）、Phase 3（ジェネリックな裸の型変数と関数型の引数）、Phase 4（`mut` の引数規約と要素・フィールドの代入） |
| 状態 | Phase 1〜4 実装・全ローカル検証済み。PR48 の最終 CI／review 待ち |
| 起票 | 2026-10-13（人間の依頼「『次の一手』を進めて、開発チケットを作成して」。推奨案 A（引数規約）の実装チケット。HEAD `2282fd96` で確認） |
| 承認 | **要承認: D1**（無印の引数を読み取りにする。言語の意味の変更で、edition の方針の改訂を伴う）、**D2**（`own` を導入する）、**D3**（edition `2027` を新設し、`LATEST` は `2026` のままにする）、**D4**（std は `2026` のまま、書き換えない）。承認が記録されるまで、手順 2 以降に着手しない |
| 改善する劣位 | Rust 比・Mojo 比: 読むだけの関数の引数すべてに `ref` を書かされる。引数の既定が「移動」なので、読むだけの関数を呼ぶと呼び出し側の値が使えなくなり（`E1012`）、Copy の配列は黙って複製される（A15、`W1006`）。Mojo は既定が読み取り専用の参照で呼び出し側に印が要らず、Hylo は `let`、Swift は借用が既定（出典は付録） |
| 手本にする既存実装 | edition の追加: `src/syntax.rs` の `Edition`（`E2026`・`LATEST`・`MANIFEST_DEFAULT`・`SUPPORTED`）、`docs/stability.md`「edition を追加するとき」、`tests/editions.rs`。型の前置き構文: `src/parser.rs` の `Parser::type_primary`・`reference_type`（`ref`・`ref mut`）、`src/syntax.rs` の `TypeExprKind::Reference`。引数の型の解決: `src/check.rs` の `check_modules` の関数宣言のループ（`for parameter in &function.parameters` の中の `resolve_type_with_kinds`）。呼び出しの暗黙の借用: `src/check.rs` の `Checker::coerce_argument`。参照の外への move: `src/ownership.rs` の `E1012`「cannot move a non-Copy value out of a reference」。位置の誤りの診断: `src/check.rs` の `resolve_type_with_kinds` の `TypeExprKind::ArrayView` の `E1005` |
| 主な影響ファイル | `src/syntax.rs`（`Edition`、`TypeExprKind::Own`）, `src/parser.rs`（`type_primary`）, `src/syntax_codec.rs`（`TypeExprKind` の tag）, `src/frontend_cache.rs`（`FRONTEND_FORMAT`）, `src/check.rs`（`resolve_type_with_kinds`、関数宣言のループ、`CheckedFunction`）, `src/polymorph.rs`（`type_expression`）, `src/ownership.rs`（`E1012` の文言）, `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/regions.rs`・`src/warnings.rs`（`TypeExprKind` を辿る match）, `src/package.rs`（対応する edition の一覧の文言）, `tests/editions.rs`, `tests/parameter_conventions.rs`（新規）, `tests/fixtures/conventions/`（新規）, `tests/features.mjs`, `docs/stability.md`, `docs/language.md`, `docs/architecture.md`, `README.md`, `_tsuzuri/language-reference/` の複数ページ |

## 目的

### 2026-10-23 の承認と優先する契約

利用者は D1〜D4 と全 Phase への着手を承認しました。以下の起票時の「要承認」は解消済みです。
実装前の契約の照合は README.md に記録しました。D-54 の意味不変の約束には A18 の
引数規約だけを例外として追記し、D-59 の現行の引数順、dev profile の opt-level 1、
リポジトリ内の scratch と共有 slot を優先します。
着手の承認は実装完了を意味しません。未実装の Phase の状態は末尾に個別に記録します。

関数の引数は、「読むだけ」が圧倒的に多い。いまの Tsuzuri は無印の型 `T` を「所有権を受け取る（Copy なら複製）」と読むので、読むだけの引数すべてに `ref` を書く必要がある。

HEAD での実測（正規表現による概算。詳細は「現状」）:

- std の単行の `def` 署名 815 件のうち 476 件（58%）が `ref`・`ref mut` を含む。引数は 共有 `ref` 588 : `ref mut` 19 : 非 Copy の値渡し 134（約 79 : 3 : 18）。
- 共有 `ref` の 94%（555 / 588）は具体的な型の引数（`ref string` 222、`ref [T]`・`ref [|T|]` 135、そのほかの具体型 198）。裸の型変数（`ref 'a`）は 33（6%）だけ。
- 言語リファレンスの例 550 個（所有権の章を除く）の 38% が `ref` を使う。`ref mut` は 1%。
- `Array.map` と `Array.map_ref`、`fold` と `fold_ref`、`HashMap.get` と `get_ref` など、値渡しと参照渡しの双子の API が 10 組、`Copy<'a>` 制約のある関数が 51 件。

「読むだけ」を既定にし、所有権を受け取る引数だけを `own T` と書く。呼び出し側は変わらない（実引数の暗黙の借用が既にある）。コピーを既定にするのではない:
**読み取りの引数は借用であり、複製も移動も起きない**。移動は `own` の引数にだけ残り、`own` の引数へ「後で使う値」を渡すと従来どおり `E1012` になる（暗黙の複製は入れない）。

意味が変わる（無印の引数の読み方が変わる）ので、edition `2027` を新設し、その edition のパッケージだけに適用する。`2026` のパッケージと std は変わらない（D3・D4）。
Phase 1 は、構文 `own`、edition `2027`、引数の補完、診断、文書、テストまでである。実装者は Phase 1 だけを実装し、Phase 2 以降は人間が求めた場合だけ着手する。

**`pure def`（[A19](A19-pure-functions.md)）との関係。** 人間の指示（2026-10-13）で、`pure def` は引数が共有借用（`ref` を省略でき、既定）か `own` の関数として別チケットになった。`pure def` の引数は、edition に依らず A19 の規則で補完されるので、純粋な関数の `ref` の負担は edition を割らずに解消される。`own` の構文は A19 が `pure def` の引数だけで先に導入できる（A19 の D16）ので、本チケットの D2 は、受理する位置を通常の `def` の引数へ広げる決定になる。
本チケットは、`IO` を使う関数や引数を消費する関数を含む `def` 全般の既定を変える別の決定で、D1 の動機のうち純粋な関数に当たる部分は A19 が先に解消する。優先度の見直しは人間が決める（D11）。

## 着手条件と停止条件

### 着手条件

- **D1〜D4 の承認が記録されていること。** `_features/README.md` の「追加チケット（2026-10-13）」の表の承認欄と本チケットの「決定事項」に承認の日付がなければ、着手しない。
- GUIDE §2.3 の基準コマンドが成功する状態から始める。`docs/stability.md` の「edition」の節と `tests/editions.rs`、本チケットの「現状」を読む。
- 手順 1 のとおり、変更前のコンパイラと、std・fixture・文書の例が `2026` で受理されることの記録を残す。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `2026` のソースの受理・拒否・診断・生成 IR が 1 つでも変わった（`2027` を選ばないパッケージ、manifest のないプロジェクト、std が対象）。
- 引数の補完のために、所有権検査の規則（`E1012`・`E1013`・`E1014` の条件）、`Type::is_copy`・`needs_drop` の意味、region の推論を変える必要が出た。
- 型変数・関数型・`Task`・`dyn` の引数を補完しないと、`2027` の既存の機能（`let rec`、`Owned.function`、型クラス、`dyn`）が動かない。補完の範囲（D5）を変えたくなったときは、先に人間へ報告する。
- `LATEST` を `2027` にしないと目的が達せられない（D3 の変更）。
- `tests/editions.rs` の期待値のうち「既存テストへの影響」に挙げた以外を変える必要が出た。
- 新しい予約語、新しい診断コード、`unsafe`、新しい crate が必要になった。
- 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状と計測（HEAD `2282fd96`）

### 引数の型の読み方

- `def f :: A -> B -> C = \a b -> ..` の引数 `a`・`b` の型は、`check_modules` の関数宣言のループが `resolve_type_with_kinds(&parameter.ty, ..)` で `Type` にする
  （`src/check.rs`。`parameters.push(ty)` の直後に `Signature { parameters, result }` を作る）。`ref T` は `TypeExprKind::Reference(inner, mutable)` から `Type::Reference(Box<Type>, bool)` になる。
  借用は型の一部で、引数の「規約」という別の概念はない。
- 呼び出しの実引数は、仮引数が `ref T` なら自動で借用される（`Checker::coerce_argument`。`docs/language.md` の「呼び出し引数の暗黙の借用」）。呼び出し側に印は要らない。
- 型の性質: `Type::needs_drop` は、`String`・`Utf8String`・`Function`・`Task`・`Dyn`・`Shared`・`Array`・`List`・`Vec`、drop が要る要素を持つ `Record`・`Union`・`Tuple`・再帰型・利用者 `Drop` を true にし、
  型変数（`Variable`・`Infer`）と数値などを false にする（`src/check.rs` の `Type::needs_drop`）。
- edition: `Edition::E2026` だけが `SUPPORTED` で `LATEST`。`docs/stability.md` は「edition の差として許すのは、予約語を増やす、非推奨の構文を削除する、警告の既定を変える、の 3 つだけ」と定め、
  「両方の edition が受理するプログラムは、…所有権と借用…が同じ」と約束している。**この変更は、約束の例外になる**（D1）。

### いまの挙動（HEAD の release コンパイラで確認）

| ソース | 結果 |
| --- | --- |
| `def length_of :: string -> i64 = \s -> s.length` を呼んだ後に、渡した `text` を読む | `E1012` use of moved or partially moved value 'text' |
| `def total :: [i64] -> i64` に `a` を渡し、その後も `a` を使う | 配列が複製される（IR に `call ptr @tz.alloc`。`--warn implicit-copy` で `W1006`）。100 万要素の配列に O(1) の `xs[k]` を 2,000 回呼ぶと、素の `[i64]` 引数は約 0.4 秒、`ref [i64]` は 0.01 秒未満 |
| `string` を引数に取る関数へ渡す、閉包に捕捉する、`a + b` の被演算子にする | いずれも移動（直後の使用が `E1012`） |
| スタックの配列リテラルを値渡しする | ヒープへ昇格する（確保と複製） |
| `ref string` の引数を値として返す（`\s -> s`、`\s -> s + "!"`） | `E1003` expected string, found ref string |
| `ref [string]` の引数を返す | `E1003` expected [string], found ref [string] |
| `ref Parser`（`string` を含む record）の引数で `{ p with pos = 1 }` や `p.text` を返す | `E1012` cannot move a non-Copy value out of a reference |
| `ref [i64]` の引数を `Array.update_at 0 1 xs` に渡す | 受理される（Copy の配列は、値を要求する引数では参照外しで複製される） |
| `let mut ys = xs`（`xs` は `ref [i64]`）に続けて `ys = Array.update_at 0 5 ys` | `E1003` expected ref [i64], found [an undetermined type]（分かりにくい。D8 で扱う） |

下の 2 つの表は、2027 の補完が「明示的に `ref` を書いたのと同じ」になることを示す。`ref` を書いた引数の挙動は、そのまま 2027 の既定の挙動になる。

### std の署名の集計（`std/*.tz` の単行の `def` 815 件、正規表現による概算）

| 項目 | 数 |
| --- | ---: |
| `ref`・`ref mut` を引数に含む署名 | 476（58%） |
| 引数: 共有 `ref` / `ref mut` / 非 Copy の値渡し（型名の一覧による概算） | 588 / 19 / 134 |
| 共有 `ref` の内訳: `ref string`・`ref utf8string` / `ref [T]`・`ref [\|T\|]` / そのほかの具体型 / 裸の型変数 `ref 'a` | 222 / 135 / 198 / 33 |
| 結果に `ref` を含む署名（借用を返す） | 34（4%） |
| `{r s}` の region を宣言する署名 | 40（5%） |
| `_ref` の双子の API / `Copy<'a>` 制約のある関数 | 10 組 / 51 |

集計は再現できる: 引数の分割は `->` を括弧・山括弧の外で切り、`=>` の前の制約を除く。`std/*.tz` の変更に応じて数は変わる。計測の目的は方向（読み取りが多数）であり、合否にしない。

## 目標と指標

- G1: `2027` のパッケージで、読むだけの引数に `ref` を書かずに済む。`def length_of :: string -> i64` を呼んだ後も、渡した値を使える。
- G2: 素の `[T]`・`[|T|]` の引数が、複製ではなく借用になる（IR の確保がなくなる）。
- G3: `2026` のパッケージ、manifest のないプロジェクト、std、既存の fixture・文書の例は、1 byte も変わらない（診断、IR、結果）。
- G4: 所有権を受け取る関数は `own T` と書ける。無印のまま消費すると、直し方を示す診断が出る。

| 指標 | 期待 |
| --- | --- |
| `tests/parameter_conventions.rs`（新規）の名前付きテスト | すべて通る |
| 2026 の既存テスト・E2E・`check-docs.mjs` | 変更前と同じ件数が通る（`tests/editions.rs` の edition 名の一覧の文言だけが変わる） |
| 同じ関数を `ref T` と書いた版と、無印の版（2027）の生成 IR | 1 byte も違わない |
| 素の `[i64]` 引数の確保（2026 と 2027） | 2026: 1、2027: 0（呼び出し側の値を使い続けるとき） |

## 変えてはいけない意味

- `2026` の意味のすべて。補完は `2027` の `def` の引数だけに働く。
- 明示的な `ref T`・`ref mut T`・`ref {r} T` の意味。明示した引数は補完しない（二重に借用しない）。
- 呼び出しの評価順、実引数の暗黙の借用の規則、region の推論、借用検査の規則と診断コード。
- スカラーや Copy で drop の要らない小さい値は、これまでどおり値渡し（物理的にも借用にしない。D5）。

## 仕様

### 構文

```text
parameter_type = ["own"] type            // def の署名の引数の位置だけ
```

- `own T` は、`def` の署名の引数の型の前だけに書ける。意味は「所有権を受け取る」で、`2026` の無印の `T` と同じである（Copy の型では複製）。
- `own` は予約語にしない。型の位置の `own` の直後に型が続くときだけ前置きとして読み、それ以外の位置・名前としての `own` は変えない（edition の語彙に触れない。`EDITION_KEYWORDS` は空のまま）。
- `own T` を引数の位置以外（結果の型、`let` の注釈、フィールド、関数型の中、型引数）に書くと `E1005`（メッセージ案: `'own' is only valid on the type of a parameter in a 'def' signature`）。
- `own` は **すべての edition で受理する**（追加のみの変更）。`2026` では無印と同じ意味で、書いても書かなくても同じ。
- `mut T` の前置き、`read T` の前置きは作らない（D6）。読み取りは「何も書かない」か、従来どおり `ref T`。

### edition 2027 の引数の補完

`Tsuzuri.toml` の `[package]` に `edition = "2027"` と書いたパッケージの `def`（`export def` と `private def` を含む。`extern` 宣言、型クラスのメソッドの署名、`const def`、エントリーポイント `def main`、`pure def`（A19 が補完する。D11）を除く）の引数について、
署名の型を解決したあとで、次の規則で型を補う。

| 引数の型の書き方 | 補完 |
| --- | --- |
| `ref T`・`ref mut T`・`ref {r} T`（明示の借用） | そのまま |
| `own T` | `T`（値渡し）にする。`own` の印を外すだけ |
| 無印の `T` で、`R(T)` が真 | `ref T`（共有借用）にする |
| 無印の `T` で、`R(T)` が偽 | そのまま（値渡し） |
| 引数が `mutable`（`Parameter.mutable`。引数を変更する束縛） | そのまま（値渡し。変更は所有が前提） |

**契約が固定された署名は補完しない（D10）。** 次の 3 つは、解決後の署名を別の検査が見るので、補完すると 2027 のパッケージの正しいプログラムが拒否される（HEAD で確認済み）。

| 署名 | 検査 | HEAD の挙動 | 2027 でも |
| --- | --- | --- | --- |
| `const def` | `constants::check_signature`（`E1026`。値型だけ。定数評価器は参照を持たない） | `const def f :: ref string -> i64` は `E1026`、`const def f :: string -> i64` は受理され `const` の初期化式からも呼べる | 値渡しのまま |
| エントリーポイント `def main`（エントリーのモジュール） | `entry_signature`（`E2004`。`[Unit]` か `[Array<string>]` だけ） | `def main :: ref [string] -> i32` は `E2004` | `Array<string>` の値渡しのまま |
| `extern` 宣言 | `abi::parameter`（`E1008`。バッファは `ref` だけ。ホストの ABI の契約） | `extern def f :: string -> i64` は `E1008` | 補完せず、従来どおり `E1008` |

`extern` 宣言は `function_declarations` に `FunctionDecl` として積まれ（`external_functions` が id で引く）、補完の対象のループを**通る**。除外は明示的に書く。
同じ edition の中で、`def` の無印の `string` は読み取り、`const def`・`main` の無印の `string` は値渡し、という差が残る。

`R(T)`（読み取りが既定になる型）:

```rust
/// A18: whether a `def` parameter written without `ref`, `ref mut` or `own` is borrowed in edition 2027.
fn read_by_default(ty: &Type, types: &TypeContext<'_>) -> bool {
    !matches!(
        ty,
        Type::Reference(..) | Type::Function(..) | Type::Variable(_) | Type::Infer(_)
            | Type::Task(_) | Type::Dyn(_) | Type::Simd(_)
    ) && ty.needs_drop(types)
}
```

- 補完するのは、drop が要る（ヒープを所有する）具体的な型: `string`、`utf8string`、`[T]`、`[|T|]`、`Vec<T>`、`Rc`・`Arc`、drop が要る record・union・tuple、再帰型、利用者 `Drop` の型。
- 補完しない: 数値・`bool`・`char`・`unit`、drop の要らない record・tuple・union・固定長配列（小さい Copy の値）、**裸の型変数 `'a`**（呼び出しごとに型が決まるため。D5）、関数型、`Task`、`dyn`、SIMD 型。
  型変数を含む型でも、`needs_drop` が型変数に依らず true（`['a]`、`Vec<'a>`）なら補完する。`Maybe<'a>` のように型変数だけで drop の要否が決まる型は補完しない。
- 結果の型は補完しない（結果は常に所有値）。入れ子の関数型の引数（コールバック）は補完しない（D5）。
- 補完した型は `Signature` に入り、関数の型（`signature.as_type()`）、呼び出し、単相化、所有権検査、生成のすべてが、明示的に `ref T` と書いたものと同じに扱う。
  補完した引数には印を付ける（`CheckedFunction` に引数ごとの `read_by_default` を足す。診断の文言だけが使う）。

### 呼び出し側

変えない。`ref T` の仮引数への実引数は、今の規則で暗黙に借用される。所有値の実引数は呼び出しの後も使え、一時値は呼び出しの間だけ借用される。`own T` の仮引数への実引数は、今の無印と同じく移動する。
`own` の仮引数へ、後で使う値を渡すと `E1012`（暗黙の複製は入れない。Copy の配列・リストは従来どおり複製で、`W1006` と A17 の対象）。

### 診断

新しい診断コードは作らない。次の 3 つのメッセージだけを、補完した引数に限って強める（他の診断は変えない）。
引数の根が `read_by_default` の印のある引数（`p`、`p.field`）のときだけ、末尾に `; parameter '<name>' is read-only by default (edition 2027); write 'own <type>' in the signature to take ownership` を足す。

| 条件 | コード | 既存のメッセージ（変えない部分） |
| --- | --- | --- |
| 読み取りの引数を値として返す・値を要求する位置へ渡す（`expected T, found ref T`） | `E1003` | `expected string, found ref string` |
| 読み取りの引数の非 Copy の値を move する（`{ p with .. }`、`p.field` を返す） | `E1012` | `cannot move a non-Copy value out of a reference` |
| `own` を引数の位置以外に書く | `E1005` | 上の「構文」のメッセージ |

### edition・manifest・cache

- `Edition::E2027` を足し、`SUPPORTED = [E2026, E2027]` にする。**`LATEST` は `E2026` のまま**（D3）。`MANIFEST_DEFAULT` は変えない。`Edition::parse("2027")` が通る。
  `unsupported edition '2030'; supported editions: 2026, 2027` のように一覧の文言が変わる。
- 標準ライブラリは常に `LATEST`（`2026`）で読む。`2027` のパッケージが std の関数を呼ぶと、std の署名（`2026` の意味で補完されたもの）のまま使う。
- 依存のパッケージは自分の edition で読まれる。`2027` のパッケージが `2026` のパッケージの関数を呼んでも、型は依存側で解決済みで、呼び出しの意味は依存側の署名どおり。
- ビルドの cache の鍵に edition が入っていること（G11・G17。すでに入っている）を確かめる。`2026` と `2027` の同じソースが別の結果になる。
- 単独のファイル・`script`・`repl`・manifest のないプロジェクトは `LATEST`（`2026`）で読むので、`2027` を試す方法は manifest に `edition = "2027"` を書くことである（D3）。

### native と WASM

違いは無い。補完は型検査の段で終わり、生成は `ref T` の引数と同じ経路を使う。ランタイム・import は増えない。

## 設計

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| edition | `src/syntax.rs` | `Edition` | `E2027`、`SUPPORTED`。`LATEST`・`MANIFEST_DEFAULT` は変えない |
| edition | `src/package.rs` | 対応する edition の文言 | `supported_names()` を使っていれば変更なし。直書きがあれば直す |
| 構文 | `src/syntax.rs` | `TypeExprKind` | `Own(Box<TypeExpr>)` を足す（A19 が先に入っていれば再利用する）。`TypeExpr` を辿る全 match（`children`・`has_regions`・`type_expression` の逆変換・formatter・docgen・semantic・warnings・regions）に足す（GUIDE §6.9。`_ =>` で黙って無視されないよう grep する: `grep -rn "TypeExprKind::Reference" src/`） |
| 符号化 | `src/syntax_codec.rs`・`src/frontend_cache.rs` | `impl Wire for TypeExprKind`、`FRONTEND_FORMAT` | `Own` に新しい tag（15。A19 が先なら実装済み）を割り当て、単体テストの `("TypeExprKind", 15)` を 16 にし、`own` を使うソースをテストの `Program` に足す。AST の符号化が変わるので `FRONTEND_FORMAT` を上げる（古いキャッシュの AST が `Own` を持たないまま読まれないように） |
| 構文 | `src/parser.rs` | `Parser::type_primary` | 識別子 `own` の直後が型の先頭なら `Own` を読む。入れ子の数を増減する（GUIDE §11.1） |
| 型検査 | `src/check.rs` | `resolve_type_with_kinds` | `TypeExprKind::Own` を、引数の位置以外では `E1005` で拒否する（A19 が先に入っていれば、`pure def` の引数だけの受理を、すべての `def` の引数へ広げる） |
| 型検査 | `src/check.rs` | `check_modules` の関数宣言のループ | 引数の型を解決する前に `Own` を剥がし（印を覚える）、解決のあとで、モジュールの edition が `2027` なら補完する。`extern` 宣言（`function_declarations` に `FunctionDecl` として積まれ、`external_functions` が id で引く。このループを**通る**）、型クラスのメソッド・instance のメソッドの署名、`names.const_functions` の `const def`、エントリーのモジュールの `main`（`entry_module` と名前で判定。定数でないもの）は補完しない（D10）。`function.pure` の関数も補完しない（A19 の規則で補完済み。D11）。除外の判定は関数の種類だけで決まり、型には依らない（`entry_signature`・`constants::check_signature`・`abi::parameter` は補完後の署名を見るので、補完してから除くのでは遅い）。 |
| 型検査 | `src/check.rs` | `CheckedFunction` | 引数ごとの `read_by_default: Vec<bool>`（`pub(crate)`）を足す。単相化・特殊化・`closures::lower` が作る関数は、元の関数から引き継ぐか `false` |
| 診断 | `src/check.rs`・`src/ownership.rs` | `E1003`（`expected .., found ref ..`）の生成箇所、`E1012`「cannot move a non-Copy value out of a reference」 | 根の局所が `read_by_default` の引数なら、末尾に文言を足す |
| 整形・文書 | `src/formatter.rs`・`src/docgen.rs`・`src/semantic.rs` | 型の描画 | `own T` を書かれたとおりに出す。補完した `ref` は出さない（書かれた形を表示する。D7） |
| 変更なし | `src/llvm*.rs`・`src/ownership.rs` の規則・`src/regions.rs` | | 補完は型検査の段で終わる |

### 手順に入れる確認の要点

- 補完は引数の型にだけ作用する。関数本体の式、`let` の注釈、ラムダの引数の注釈、フィールドの型は変えない。
- 補完を `resolve_type_with_kinds` の中に入れない（結果の型・フィールドにも効いてしまう）。関数宣言のループだけに入れる。
- `R(T)` を判定する時点で、record・union の宣言が登録済みであること（`TypeContext` が使える段）。先に宣言を集めてから署名を解決する今の順序を保つ。

## 実装手順

承認（D1〜D4）が記録されていることを最初に確認する。各段で `cargo test --locked` を実行し、`running N tests` の N が 0 でないことを確かめる（GUIDE §3.1）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数を控える。変更前のコンパイラを `/tmp/tz-A18/tsuzuri-before` に写す。
  `tests/fixtures`・`examples`・`std` を使うプロジェクトの IR（`--emit llvm -O0`）を `/tmp/tz-A18/before/` に保存する（手順 3 の「2026 は変わらない」確認に使う）。
  「現状」の表の各行を `/tmp/tz-A18/` の小さいプロジェクトで再現し、結果を控える。
- 確認: すべて通る。表の結果が一致する。

### 手順 2: edition `2027` を足す（挙動は変えない）

- 変更: `src/syntax.rs`、`tests/editions.rs`。
- 内容: `Edition::E2027` と `SUPPORTED` を足す。`LATEST`・`MANIFEST_DEFAULT` は変えない。`Edition::parse("2027")` が通る。
  `tests/editions.rs` の `rejects_unknown_and_unsupported_editions` の期待文言（`supported editions: 2026`）を `2026, 2027` に直す（既存テストへの影響として許す）。
  `edition_keywords_are_consistent_with_the_supported_editions` と、`docs/stability.md` の edition の表の更新方針を読み、予約語を足さないことを確かめる。
- 確認: `cargo test --locked --test editions`。`2027` を書いた manifest が読め、`2026` と同じ結果になる（補完はまだない）。手順 1 の IR と 1 byte も違わない。

### 手順 3: `own` の構文

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/check.rs`、`src/polymorph.rs`、`src/formatter.rs`、`src/docgen.rs`、`src/semantic.rs`、`src/regions.rs`、`src/warnings.rs`、`tests/parameter_conventions.rs`（新規）。
- 内容: `TypeExprKind::Own` と、`type_primary` の読み方、`resolve_type_with_kinds` の `E1005`、関数宣言のループで `Own` を剥がす処理を足す（この段では印を捨てるだけ）。
  GUIDE §6.9 の手順で、`TypeExprKind` を辿る全出現を grep し、`Own` を足す。整形（`tsuzuri fmt`）で `own T` が往復する。
  符号化を足す: `src/syntax_codec.rs` の `impl Wire for TypeExprKind` に `Own` の tag（15）、同ファイルの単体テストの数 `("TypeExprKind", 15)` を 16 に、`own` を使うソースをテストの `Program` に足す（数だけ上げて例を足さないと `no example writes TypeExprKind tags [15]` で落ち、数を上げ忘れると新しい tag が検査されない）。`FRONTEND_FORMAT`（`src/frontend_cache.rs`）を 1 上げる（上げ忘れると、古いキャッシュの AST が `Own` を持たないまま読まれる）。
  [A19](A19-pure-functions.md) の手順 3 が先に実装済みなら、`TypeExprKind::Own`・読み方・符号化・走査箇所は実装済みなので再実装しない。足すのは、受理する位置を `pure def` の引数だけから、すべての `def` の引数へ広げる規則だけである（`pure def` の `own` の制限は A19 の D15 のまま）。逆に本チケットが先なら、A19 の手順 3 が、受理する位置の規則だけを足す。
- 確認: すべての edition で次が通る。`def f :: own string -> i64` が受理され、`own` を外した版と IR が同じ。`let x: own string = ..`・`def f :: i64 -> own string`・`(own string -> i64)` が `E1005`。
  識別子 `own` を変数名に使うコードが従来どおり通る。`tsuzuri fmt --check`・`tsuzuri doc` が `own` を壊さない。`syntax_codec` の tag の網羅の単体テストと `node tests/frontend_cache.mjs target/release/tsuzuri` が通る。手順 1 の 2026 の IR が 1 byte も変わらない。

### 手順 4: 引数の補完（edition 2027）

- 変更: `src/check.rs`、`tests/parameter_conventions.rs`、`tests/fixtures/conventions/`（新規）。
- 内容: 「edition 2027 の引数の補完」の表と `read_by_default` のとおり、関数宣言のループで補完する。`2027` を選んだモジュールだけを補完する。モジュールごとの edition は `check_modules` が受け取る `ModuleInput.edition`（`src/check.rs`。「The edition that the module's source was read with」の欄。G19。いまは `lexer::lex(name, module.edition)` の 1 か所だけが使う）である。`CheckedFunction.read_by_default` を足す。
- 確認: `tests/parameter_conventions.rs` に次を足す（`Project::load` で manifest 付きの一時プロジェクトを作る。手本は `tests/editions.rs`）。
  - 2027: `def length_of :: string -> i64` を呼んだ後に `text.length` を読んで受理され、結果が 2 つの長さの和（`E1012` にならない）。同じソースが 2026 では `E1012`。
  - 2027: `def total :: [i64] -> i64` に値を渡しても IR に確保がない（`call ptr @tz.alloc` が呼び出し側の関数にない）。2026 は 1。`--warn implicit-copy` で、2027 は `W1006` が出ず、2026 は出る。
  - 2027 の `def f :: string -> i64` と `def f :: ref string -> i64` の IR（`tz.fn.Main.f` の本体と呼び出し側）が同じ。
  - 2027: `def g :: i64 -> f64 -> i64`、`def h :: Point -> i64`（`Point { x: f64, y: f64 }`）は値渡しのまま（IR の引数が `i64`、`%tz.record.Main.Point`）。
  - 2027: `def id :: 'a -> 'a`、`def apply :: ('a -> 'b) -> 'a -> 'b` は補完されない（従来どおり値渡し）。`def head :: ['a] -> i64` は補完される。
  - 2027: `def wrap :: string -> (string * i64) = \s -> (s, s.length)` は `E1012`（診断の強化は手順 5）。`own string` と書いた版は受理される。
  - 2027: `mutable` の引数は補完されない。明示の `ref mut`・`ref {r}` は変わらない。`export def f :: string -> i64` は `ref string` として受理され、公開 ABI の検査（`E1008`）を通る。
  - 2027: `const def name_length :: string -> i64`（`fn name_length s = s.length`）は補完されず、`const Eight: i64 = name_length "abcdefgh"` が 8 になり、実行時の呼び出しも値を返す（`E1026` にならない。2026 と同じ）。
    明示の `ref string` の `const def` は 2027 でも `E1026` のまま。`def` の無印の `string` は読み取りで、`const def` の無印の `string` は値渡し、という差を 1 つのテストに並べる。
  - 2027: `def main :: Array<string> -> i32 = \args -> args.length as i32` は補完されず、`E2004` にならない。`tsuzuri build` で作った実行ファイルに引数を 2 つ渡すと終了コードが 2（`tsuzuri run` は追加引数を渡さず、`main` は空配列を受け取るので 0。HEAD の 2026 で確認済み）。別モジュールの `main` という名前の関数は、通常どおり補完される。
  - 2027: `extern def host_len :: string -> i64` は補完されず、2026 と同じ `E1008`。`extern def host_len :: ref string -> i64` は受理される（HEAD で確認済み）。
  - 2026・manifest なしのファイルでは、上のどれも補完されない（無印の意味が変わらない）。
  - 依存: 2027 のパッケージが 2026 のパッケージの関数（無印の `string` を消費する）を呼ぶと、呼び出しは移動で、後の使用が `E1012`。

### 手順 5: 診断

- 変更: `src/check.rs`、`src/ownership.rs`、`tests/parameter_conventions.rs`。
- 内容: 「診断」の表の 2 件（`E1003`・`E1012`）に、補完した引数に限る末尾の文言を足す。根の局所の判定は `read_by_default` を使う。他の診断の文言は変えない。
- 確認: 2027 の `\s -> s`（結果が `string`）と `{ p with pos = 1 }`、`p.text` を返す関数が、文言つきの `E1003`・`E1012` になる。同じソースの 2026 版と、明示 `ref` の 2027 版は、従来の文言のまま（変わらない）。

### 手順 6: 全体の確認

- 内容: GUIDE §3 の全体（fmt・clippy・`cargo test --locked`・E2E）、§3.1 の回帰テスト、`sh scripts/check-runtime-includes.sh`。`tests/features.mjs` に suite `conventions` を足す
  （fixture `tests/fixtures/conventions/` は `Tsuzuri.toml`（`edition = "2027"`）つき。`export def` で結果を整数にし、期待値は JavaScript の独立した計算。native/WASM × `-O0`/`-O3`、`live == 0`）。
  `2026` の fixture・例・文書の例が変わらないこと（手順 1 の IR と比較）。
- 確認: すべて通る。IR が決定的。2026 の IR が 1 byte も変わらない。

### 手順 7: 文書

「ドキュメント」の節のとおり。`node scripts/check-docs.mjs <変更したページ>` を通す。2027 の挙動を示すコード例は、`check-docs.mjs` が manifest なしで検査する（`LATEST` = 2026）ため、`run=` を付けない `text` のブロックにする。

## テスト計画

- Rust: `tests/parameter_conventions.rs`（新規。手順 3〜5 の名前付きテスト）。`tests/editions.rs`（手順 2 の 1 件の文言）。`tests/formatter.rs`・`tests/docs.rs`・`tests/lsp.rs` に `own T` が往復する 1 ケースずつ。
- E2E: `tests/features.mjs` の suite `conventions`（手順 6）。`tests/cache.mjs` に、同じソースが edition で別の cache になる 1 ケース（任意）。
- 既存テストへの影響: `tests/editions.rs` の edition 名の一覧の文言だけ。ほかの期待値を変える必要が出たら停止条件。

## ドキュメント

意味が変わる機能なので、すべて更新する（GUIDE §8）。`grep -rnE "ref |ref mut|E1012|edition" _tsuzuri/language-reference` で漏れを探す。

- `docs/stability.md`: 「edition の差として許すのは」に 4 つ目（引数の既定の借用。A18）を足し、「edition が左右するのは字句と警告の既定だけで、型検査や IR の生成は edition を見ません」を、`2027` から型検査が引数の補完のために edition を見る、と直す。edition の表に `2027`（opt-in。`LATEST` ではない）を足す。
- `docs/language.md`: 「Ownership / Borrowing」「呼び出し引数の暗黙の借用」に、`own` と 2027 の補完を足す。「ソースと宣言」の予約語は変えない（`own` は予約語でない）。`README.md` の該当する説明。
- `_tsuzuri/language-reference/`: `ownership-and-memory/borrowing.md`（引数の既定）、`ownership.md`（移動する場面に `own`）、`values-and-functions/functions.md`、`organizing-tsuzuri/packages.md#package-セクション`（edition `2027`）、`compiler/diagnostics.md`（`E1003`・`E1012` の文言）、`index.md`。
- `docs/architecture.md`: 補完が型検査の段で終わり、生成・所有権検査は `ref T` と同じに扱うこと。

## 受け入れ条件

- [ ] D1〜D4 の承認が記録されている。
- [ ] 手順 1〜7 を実施し、各段の確認が通る。Phase 2 以降は人間が求めた場合だけ（求めなかったときは「未実施」と書く）。
- [ ] `tests/parameter_conventions.rs` の名前付きテストがすべて通る。`2026` の既存テスト・E2E・文書の例が、`tests/editions.rs` の文言 1 件を除き変わらない。
- [ ] `2026`（manifest なしを含む）と std の IR が 1 byte も変わらない。2027 の `def f :: string -> i64` と `def f :: ref string -> i64` の IR が一致する。
- [ ] `own T` が すべての edition で受理され、引数の位置以外では `E1005`。`own` を識別子に使うコードが変わらない。
- [ ] 診断の強化が補完した引数にだけ付く。
- [ ] 2027 のパッケージの `const def`・エントリーの `main`・`extern` が補完されない。`const def f :: string -> i64` が `E1026` にならず `const` の初期化式から呼べる。`def main :: Array<string> -> i32` が `E2004` にならない。`extern` の無印の `string` は従来どおり `E1008`。
- [ ] native/WASM × `-O0`/`-O3`、`live == 0`、WASM の import が空（suite `conventions`）。
- [ ] `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`、§3.1 の回帰テスト、`sh scripts/check-runtime-includes.sh` が通る。
- [ ] ドキュメントを更新し、`node scripts/check-docs.mjs` が通る。`_features/README.md` の状態を `done（Phase 1）` にし、チケットを `_completed/` へ移す。
- [ ] GUIDE §13.2 の書式の完了報告を末尾の `## 実装と検証（YYYY-MM-DD）` に書く。コミットは人間の指示があるまで作らない。

## 落とし穴

- **補完は関数宣言のループだけ。** `resolve_type_with_kinds` に入れると、結果の型・フィールド・`let` の注釈・関数型の中にも効き、意味が広がりすぎる。
- **二重の借用。** 明示の `ref T` を、さらに `ref ref T` にしない。補完は無印の引数だけ。
- **型変数と関数型。** `'a`・`('a -> 'b)` を補完すると、型クラスの制約・コールバックの契約・`let rec` の捕捉の規則と衝突する。補完しない（D5）。補完しない型が `Type::Variable` を含むとき（`['a]` は補完、`Maybe<'a>` は補完しない）の判定は、`needs_drop` の結果に従う。
- **ABI。** `export def` の引数の型は公開 ABI の検査（`E1008`）にかかる。補完した `ref string`・`ref [ubyte]` は、明示の `ref` と同じに通るはずである。`extern` は補完しない。
- **契約が固定された署名（`E1026`・`E2004`・`E1008`）。** 関数宣言のループは、補完の候補地（引数の型の解決の直後）のすぐ後で、解決後の署名を 3 つの検査にかける。
  `constants::check_signature` は `Type::Reference` を `E1026`、`entry_signature` は `[Unit]`・`[Array<string>]` 以外を `E2004`、`abi::parameter` はバッファの値渡しを `E1008` にする。
  補完をこれらに掛けると、2027 の `string`・配列の引数を持つ `const def` と、引数を読む `main` がすべて壊れる。`extern` は「`FunctionDecl` ではない」のではなく、同じループを通るので、除外を明示する。
  除外の対象を増やす変更（新しい署名の検査が補完後の署名を見る機能）は、このリストを見直す責任を負う（`grep -n "signature" src/check.rs` の関数宣言のループ）。edition 内の差（D10）は文書に書く。
- **`mutable` の引数。** 引数の変更は所有が前提。補完しない。
- **moved の診断の根。** 診断の強化は、根の局所が補完した引数のときだけ。`p.field` のような射影、`{ p with .. }` の基底も根をたどって判定する。
- **cache。** edition が違うと同じソースの型検査が変わる。cache の鍵に edition が入っていることを確かめる（G11・G17）。
- **2026 が変わらないこと。** std と manifest なしのファイルは `LATEST`（2026）で読む。`LATEST` を動かさない限り変わらないが、`Edition` の比較（`>=`）を使うと将来の edition で意図しない edition にも効く。`== E2027` ではなく、「補完を行う edition の集合」を 1 か所に定義する。
- **`edition` を型検査が見る最初の変更。** これまで edition は字句だけを左右していた。`ModuleInput.edition` が関数宣言のループから引けること（`names` や `module` の名前から、そのモジュールの `ModuleInput` を引く表を作る必要があるかもしれない）を、手順 4 の最初に確かめる。引けなければ停止条件。

## 対象外

- 移行ツール（無印の消費する引数へ `own` を足す `tsuzuri fix`）、std・fixture・文書の書き換え、`LATEST` を `2027` にすること（Phase 2。要承認）。
- 裸の型変数と関数型の引数の読み取り（`Array.map` と `Array.map_ref` の統合、`Copy<'a>` 制約の整理）。規約を型の外に出す設計が要る（Phase 3）。
- `mut` の引数規約、要素・フィールドへの代入構文（`xs[i] = v`、`s.f = v`）、`Vec.push` を `mut` にすること（Phase 4。D-13 の再検討が要る）。
- パターン束縛の既定を読み取りにすること（A17 の Phase 2 が、所有する scrutinee の束縛を移動にする）。
- 戻り値の借用（`ref {r} T`）の省略規則、region の構文の変更。
- `const def` の引数の読み取り（定数評価器が参照を持てるようにして、`const def` も補完の対象にすること。チケット D11 の拡張で、別チケット）。
- `tsuzuri api-diff` が edition の違いによる意味の変更を検出すること（署名だけを読むので検出できない。既知の限界として文書に書く）。

## 決定事項

### D1: 無印の引数を読み取りにする（edition 2027）

- 決定: `2027` の `def` の無印の引数は、`R(T)` が真なら `ref T`。`docs/stability.md` の「edition の差として許すのは 3 つだけ」を 4 つにする。
- 理由: 読むだけの引数が大多数（std の引数の約 79%）で、既定が「移動」だと毎回 `ref` が要る。Mojo・Hylo・Swift は読み取りが既定。意味が変わるので edition に閉じ込める。
- 状態: **要承認**（承認前は手順 2 以降に着手しない）

### D2: `own` を導入する

- 決定: 型の前置き `own T`（引数の位置だけ）。予約語にしない。すべての edition で受理する。
- 理由: 所有権を受け取る引数を明示する印が要る。追加のみで、2026 の意味を変えない。
- 注: [A19](A19-pure-functions.md) の D1・D16 は、人間の許可（2026-10-13「`own` でも純粋性が保たれることが保証されるのであれば、`ref` に加えて `own` も許容する」）に基づいて、`pure def` の引数だけで `own` を先に受理する。この決定（通常の `def` の引数へ受理を広げること）は、その許可に含まれないので、要承認のまま。A19 が先に入ると、`own` の構文は実装済みで、本チケットは受理する位置を広げるだけになる。
- 状態: **要承認**

### D3: edition 2027 を新設し、`LATEST` は 2026 のままにする

- 決定: `SUPPORTED = [2026, 2027]`、`LATEST = 2026`、`MANIFEST_DEFAULT = 2026`。2027 は manifest に書いたパッケージだけ。
- 理由: `LATEST` を動かすと、manifest なしのファイル・`script`・`repl`・std・文書の例が一斉に 2027 になり、std と 132 個の fixture と 550 個の文書の例を同時に書き換える必要が出る（Phase 2）。
- 状態: **要承認**

### D4: std は 2026 のまま書き換えない

- 決定: std の署名は 2026 の意味（明示の `ref`、消費する無印）のまま。2027 のパッケージが呼んでも、std の署名どおり。
- 理由: 明示の `ref` は 2027 でも同じ意味なので、書き換えなくても共存できる。移行は Phase 2。
- 状態: **要承認**

### D5: 補完の範囲

- 決定: `R(T)` = `needs_drop` が真で、参照・関数型・型変数・`Task`・`dyn`・SIMD でない型。入れ子の関数型の引数、結果、フィールドは補完しない。
- 理由: 実測の 94% は具体的な型。型変数や関数型は規約を型の外に出さないと意味が決まらない（Phase 3）。小さい Copy の値は値渡しのほうが速い（ポインタを経由しない）。
- 状態: 既定案（実装者はこの案に従う）

### D6: `read`・`mut` の前置きを作らない

- 決定: 読み取りは無印か `ref T`、変更は従来の `ref mut T`。`mut T` は Phase 4。
- 理由: 構文を最小にし、意味の変更を 1 つに保つ。
- 状態: 既定案（実装者はこの案に従う）

### D7: 補完した型を表示に出さない

- 決定: `tsuzuri fmt`・`doc`・LSP は、書かれた形（`string`、`own string`）で表示する。補完した `ref` は出さない。
- 理由: 利用者が書いたものと一致し、edition を知っていれば意味が決まる。診断のメッセージ（型の表示）は従来どおり `ref string`。
- 状態: 既定案（実装者はこの案に従う）

### D8: 診断の強化は 2 件の末尾の文言だけ

- 決定: 「診断」の表のとおり。`expected ref [i64], found [an undetermined type]` のような別の分かりにくい診断は、このチケットでは直さない。
- 理由: 型推論の診断全体の整理は別のチケット。補完が生む典型の 2 つの失敗だけを案内する。
- 状態: 既定案（実装者はこの案に従う）

### D9: 呼び出し側に印を足さない

- 決定: `f (mut x)` や `f (move x)` のような呼び出し側の印は、Phase 1 では作らない。
- 理由: 読み取りの呼び出しは今も印なしで、`ref mut` の自動借用も今の規則のまま。変更の可視化は Phase 4 で、`mut` と一緒に決める。
- 状態: 既定案（実装者はこの案に従う）

### D10: 契約が固定された署名（`const def`・エントリーの `main`・`extern`）は補完しない

- 決定: 2027 でも、`const def`、エントリーのモジュールの `def main`、`extern` 宣言の引数は補完せず、従来どおりに解釈する（`names.const_functions`・`entry_module` と名前・`external_functions` の id を、補完の対象から外す）。
- 理由: 3 つとも、解決後の署名を別の検査が見る（`E1026` は値型だけ、`E2004` は `Array<string>` の値渡しだけ、`E1008` はバッファの `ref` だけ）。補完すると、正しいプログラムが拒否される（HEAD の診断で確認済み）。
  定数評価器に参照を教えるのは、チケット D11（`const def`）の設計の変更で、ランタイムの入口と ABI は A18 の範囲外。edition 内の差（`def` の無印は読み取り、これらの無印は値渡し）は、文書の例で明示して許す。
- 状態: 既定案（実装者はこの案に従う。起票後の `pure def` の検討で見つかった抜けの補正）

### D11: `pure def`（A19）との関係

- 決定: 関数宣言のループは `function.pure` を先に判定し、`pure def` にはこのチケットの補完をしない（A19 の `pure_parameter` が edition に依らず補完する。二重に補完しない）。`pure def` の引数の `own T` は、A19 の規則（D1・D15・D16）で受理され、`E1029` の制限（`own` の型は平たいデータ。`mut`・`ref mut` の引数は不可）がかかる。通常の `def` の `own T` は本チケットの規則（D2）に従う。D10 の「補完しない署名」に `pure def` は含まない（`pure def` は A19 が自分で補完する）。
- 理由: `pure def` は新しい構文なので、edition の改訂なしに無印の引数を読み取りにできる。A18 の対象は、`IO` を使う関数や消費する関数を含む `def` 全般で、人間の承認（D1〜D4）が要る別の決定である。`own` の構文は両チケットで共有するので、先に入るほうが実装し、後のほうは受理する位置の規則だけを足す（手順 3）。
- 状態: 既定案（実装者はこの案に従う）

## 付録

### 出典（2026-10-13 に確認）

- Mojo: 引数の既定は読み取り専用の参照で、呼び出し側に印は要らない。「Rust は既定で値を移動する」との対比が手引きにある（<https://mojolang.org/docs/manual/values/ownership/>）。
- Hylo: 引数の規約 `let`・`inout`・`sink`・`set`、既定は `let`（<https://docs.hylo-lang.org/language-tour/functions-and-methods>）。
- Swift: SE-0377。通常の関数は引数を既定で借用し、初期化子・setter は消費する（<https://github.com/swiftlang/swift-evolution/blob/main/proposals/0377-parameter-ownership-modifiers.md>）。
- 可変値セマンティクス（参照を 2 級にする規律。JOT 2022）: <https://www.jot.fm/contents/issue_2022_02/article2.html>。

### 検討して採らなかった案

- GC・サイクルコレクター・`new`/`delete`: 方針に反する（[Tsuzuri 言語の戦略](../_tsuzuri/language-reference/languages/strategy.md)の「採らないもの」）。
- 既定の参照カウント（Lobster・Perceus）: 注釈は要らないが、カウントの操作が見えず、drop の時点が最後の使用の直後になってスコープから読めなくなり、循環でリークする。
- 世代付き参照（Vale）を既定にすること: 誤りが実行時のトラップになり、コンパイル時に拒否する方針と逆になる（`Arena` のハンドルが局所的に採用済み）。
- 呼び出し先の本体から引数の規約を推論する案（Lean の borrow inference）: 本体の変更が呼び出し側の `E1012` を変え、署名に契約が現れない（API の差分が見えない）。

## 実装と検証（2026-10-23、Phase 1〜4）

- **承認と旧案の読み方**: 同日の利用者の指示が D1〜D4 と Phase 1〜4 を承認した。
  上の Phase 1 限定の対象外・D3 の `LATEST = 2026`・D4 の std 非移行・Phase 3/4 の未承認という
  初期案は最終状態を表さない。実装前の README の決定記録で D-54／D-13／D-59 と照合した。
  `MANIFEST_DEFAULT = 2026` は維持し、`LATEST = 2027` と std・裸の CLI・script・REPL の移行を実施した。
- **Phase 1 実装済み**: edition、全版の `own`、具体型の読み取り補完、固定署名の例外、
  補完した引数だけの診断、formatter／doc／semantic／LSP／cache／region。
  `594ef574` の時点の全 Rust は 1726 件通過・2 skip。PR48 の初回 Copilot 6 指摘も修正した。
- **Phase 2 実装済み**: `fix --edition 2027 --from 2026` の解析済み参照に基づく checked migration、
  失敗時の無変更・依存保護・atomic 書き込み・再実行。std の所有署名と版別 API、
  114 fixture/example と文書の移行、既定版の更新。版を取らない Rust 解析 API は 2026 を保つ。
- **Phase 3 実装済み**: 型とは別の引数規約、裸の型変数と関数引数の read view、
  特殊化、文脈付き callback adapter、Array／List の 12 組の read／owned 入口。
  `Copy` の保証、所有 callback の旧推論順、callback の一度だけの構築を保つ。
- **Phase 4 実装済み**: `mut T`、field／要素の代入、貸し出しと借用格納の弱い更新、
  排他 `Vec.push` と `Vec.push_owned`。射影先の旧値の解放に stack frame の候補を引き継ぐ。
  固定 Drop protocol 内の自己 field 置換と const evaluator の射影代入は対応対象にせず、診断で拒否する。
- **最新の focused 実行**: `conventions` 31 ケース・10 トラップが native／WASM × O0／O3 で通過。
  native は `live == 0`、WASM は import なし。非 Copy の Array／List／固定長配列／入れ子 field の
  繰り返し代入と外側からの Drop record の field 置換を含む。
  射影 frame 修正後、明示 2026 の凍結 114 プロジェクトと、移行した 2027 の同 114 プロジェクトが、
  旧版との 3 回の比較で結果・診断内容・IR が一致した（移行後の診断の物理位置だけ正規化）。
  全 Rust の最初の最終実行は 1737 通過・10 失敗・2 skip。既定版に追従する消費署名・Vec 入口・
  cache／new の検査と api-diff の定数・curry 比較を修正し、関連 62 件は全通過した。
  第 2 review の引数別 mutable binding の api-diff 漏れも回帰テストとともに修正した。
  一時 callback の検査は escape 条件を保つ陽性・陰性の両方を検証した。fmt・strict clippy・dev build も通過。
- **最終ローカル検証**: 修正後の全 Rust は 1762 通過・2 skip
  （通常 1750 と、cwd を変更する targets 12 を TMPDIR の形式を分けて実行）。
  fmt・strict clippy・dev build が通過。`features.mjs` 全体の 19700 ケース、
  `control.mjs` の 659 ケース、computations（1 億回の定数スタック再帰と移行済み benchmark を含む）、
  IO、async の利用可能経路、frontend cache、REPL、script が通過した。
  数値・借用の fixture は native／WASM × O0／O3、IO は ASan／UBSan と zero-live も検査した。
  Node 20 にない JSPI だけは既存の skip で、対応済みとはしない。
- **移行と文書の最終確認**: benchmark 23 入力を checked migration で移行し、型検査を通した。
  明示 2026 の凍結 114 入力と、移行済み 2027 の同 114 入力を最終 compiler で再検証し、
  それぞれ旧版／現在／再実行の結果・診断・114 本の IR が一致した。
  変更したリファレンス 51 ページ・1027 リンク・360 例・native O0/O3 の 700 実行・
  2 test project が通過。runtime include 58 ファイルも確認した。
- **残作業**: full implementation を PR48 に公開し ready にする。最終 Copilot／CI の指摘や失敗を
  読んで修正し、すべて通過してから archive する。A19 は別 branch／stacked PR で続ける。
