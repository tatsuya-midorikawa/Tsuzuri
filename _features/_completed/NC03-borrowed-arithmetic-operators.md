# NC03: 算術演算子型クラス（`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Neg`）の借用対応（案 C・言語拡張 ①）

| 項目 | 内容 |
| --- | --- |
| ID | NC03 |
| 優先度 | P1（非 Copy 数値型 `Tensor`・`Matrix`・`BigInt` で `a + b` が値を消費せず自然に書けるようにする） |
| 規模 | L（型クラス契約・型検査・単相化・所有権検査・edition 互換・`BigInt` 移行） |
| 依存 | – |
| 後続 | ([NC06](NC06-tensor-ufuncs-and-elementwise.md)) |
| 状態 | done |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（組み込み算術型クラスのメソッド引数契約を借用へ改める言語仕様拡張。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | [A11](_completed/A11-borrowed-comparisons.md)（D-20: `Eq`/`Ord` の借用シグネチャ化）、[A18](_completed/A18-parameter-conventions.md)（edition 2027 の読み取り既定引数規約と `own`）、`src/polymorph.rs` の演算子ディスパッチ、`std/BigInt.tz` の `instance Add<BigInt>` |
| 主な影響ファイル | `src/check.rs`、`src/polymorph.rs`、`src/ownership.rs`、`src/call_specialization.rs`、`src/llvm.rs`、`src/effects.rs`、`std/BigInt.tz`、`std/Complex.tz`（NC01 完了済みの場合）、`std/Monoids.tz`（`Add<'a>` / `Mul<'a>` ジェネリック利用者の整合確認）、`tests/borrowed_arithmetic.rs`（新規）、`tests/features.mjs`、`tests/fixtures/`、`docs/language.md`、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/values-and-functions/op-and-expressions.md`、`_tsuzuri/language-reference/built-in-types-and-modules/bigint.md` |

## 実装者向け早見

- 範囲: 組み込み算術型クラス `Add`・`Sub`・`Mul`・`Div`・`Rem`・`Pow`・`Neg` のユーザー定義・標準ライブラリ非 Copy 型インスタンス（`BigInt`・`Tensor`・`Matrix`・ユーザーレコード等）および演算子適用（`+`, `-`, `*`, `/`, `%`, `**`, 単項 `-`）を、**オペランドをムーブせずに共有借用で評価できる**契約へ拡張する（[A11](_completed/A11-borrowed-comparisons.md) の D-20 と [A18](_completed/A18-parameter-conventions.md) の読み取り規約の統合）。
- 最初に読む: ① [A11](_completed/A11-borrowed-comparisons.md)（D-20 の前例） ② `src/polymorph.rs` 1070〜1120 行・5790〜5830 行・6090〜6115 行 ③ `src/ownership.rs` 14485〜14515 行 ④ `src/llvm.rs` 7754〜7805 行（`string` の `take_operand`/`append` vs `concat`） ⑤ `src/call_specialization.rs` 315〜345 行（PR02 D10 `intrinsic_operation`）。
- 最初に書くテスト: `tests/borrowed_arithmetic.rs` で非 Copy レコード（`BigInt` や独自 `Vec2Buf`）に対し `let c = a + b; let d = a + c` が `E1012` にならず通り、一時値 `(a + b) + c` が解放（`live == 0`）されること、および既存 `let f: i32 -> i32 -> i32 = Add.add`・`let add = Add.add 20`・`string + string` が壊れないこと。
- 触ってはいけない: スカラー数値（`i8`〜`i128`, `f16`〜`f128`, `d32`〜`d128`）・SIMD 型の演算子および `Add.add` の直展開 IR（PR02 D10）、`string + string` の消費＋バッファ再利用（`@tz.string.append`）の安全性。多パラメータ型クラス（異種型演算）はこのチケットのスコープ外。
- 完了の見分け方:

```tsuzuri
let a = 100000000000000000000I
let b = 200000000000000000000I
let c = a + b
let d = (a - b) * c   // a, b, c を消費せずに何度でも式で使える
```

が edition 2027 でコンパイル・実行でき、`live == 0` を満たす。

## 目的

現状の Tsuzuri では、`Eq.eq` / `Ord.lt` は `ref 'a` を受け取る非消費契約（D-20）になっているのに対し、`Add.add`・`Sub.sub`・`Mul.mul`・`Div.div`・`Rem.rem`・`Neg.neg` は値を受け取る契約のため、非 Copy 型（`BigInt`・`Tensor`・`Matrix`・ユーザー定義の多項式やベクトル型）に演算子インスタンスを実装すると `let c = a + b` の時点で `a` と `b` がムーブされ、直後に `a` を参照するだけで `E1012` になる。

数値計算で `(x - mean) / std` や `a + b * c` のような自然な数式を、Tsuzuri の簡潔な単一パラメータ型クラスと所有権安全性を保ったまま書けるようにするため、算術演算子型クラスを借用ベースへ統一する。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認（言語仕様の型クラス契約変更）。
- `grep -rn '"Add"\|"Sub"\|"Mul"\|"Div"\|"Rem"\|"Neg"' src/` で組み込み算術クラスを参照する全箇所を洗い出す。

### 停止条件

- **S1** `string + string`（`llvm.rs` 7770 行で `take_operand` + `@tz.string.append` を行う既存最適化）で、借用化による use-after-free や不要な `@tz.string.concat` 退行が生じる場合。設計 §1 のとおり `string` 連結は既存の組み込み所有権契約を維持する（あるいは A17 最終使用時のみ `append`、継続使用時のみ `concat` を明示切替する）方針で整合させ、食い違えば止めて報告する。
- **S2** スカラー数値や SIMD 演算の生成 IR（および PR02 D10 の `call_specialization::intrinsic_operation` による `Add.add` 直展開）に余分な stack spill や関数呼び出しが残り、`tests/fixtures` の既存スカラー/SIMD IR が退行する。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `src/polymorph.rs` 1075〜1115 行で `Eq`/`Ord` だけが `ref 'a -> ref 'a -> bool` として登録され、`Add`〜`Rem`/`Pow`/`Neg` は `'a -> 'a -> 'a`（値渡し）となっている。
- `src/ownership.rs` 14489 行では `Eq`/`Ord` の 6 演算子だけが `Use::Read` で、`Add` 等は一律 `Use::Consume`。`src/llvm.rs` 7770 行では `string + string` が `take_operand` で左辺バッファを破壊的再利用（`@tz.string.append`）している。
- 既存テストに値型メソッド値 `let f: i32 -> i32 -> i32 = Add.add`（`tests/fixtures/polymorphism/Main.tz:75`, `tests/fixtures/currying/Functions.tz:124`, `tests/polymorphism.rs:82,784`）および部分適用 `let add = Add.add 20`（`tests/currying.rs:531`, `tests/integer_ranges.rs:469`）が存在し、`src/call_specialization.rs:315` の `intrinsic_operation` は引数がスカラー値型の `Add.add`/`Sub.sub`/`Mul.mul` を直接演算命令へ下げている。
- `std/BigInt.tz` は `instance Add<BigInt>` 等を持ち、実装内部で `add_values (ref left) (ref right)` と借用に変換しているにもかかわらず、呼び出し側では `left` と `right` がムーブされる。

## 変えてはいけない意味

- 左から右への評価順序（`a + b` は必ず `a` を評価してから `b` を評価）。
- 組み込みスカラー型・SIMD 型の `+`〜`%`・単項 `-` および `Add.add` 直接適用の生成 IR、トラップ、`@checked`、折り返し契約。
- `string + string` のメモリ安全性（解放済みバッファへの `@tz.string.append` 絶対禁止）。
- 単一パラメータ型クラスの単純さ（多パラメータ型クラスや暗黙の型変換は入れない）。

## 仕様

### 1. 算術型クラスの借用契約（D1 の 2 案と推奨設計）

- **推奨案（D1-A・既定案: A18 `[Read]` 規約型 / 非組み込み型の借用化）**:
  - 組み込みのプリミティブ（スカラー数値・SIMD・`string`）に対する `+`〜`%`・`**`・単項 `-` および `Add.add` 等は既存の型・所有権・IR 契約（スカラー/SIMD は値渡し、`string + string` は既存の `take_operand`/`append` 消費契約）を完全に維持する。これにより `let f: i32 -> i32 -> i32 = Add.add`・`let add = Add.add 20`・PR02 D10 `intrinsic_operation`・`string` 連結 IR を 1 byte も壊さない。
  - ユーザー定義インスタンスおよび標準ライブラリのレコード/共用体インスタンス（`BigInt`・`Tensor`・`Matrix`・ユーザー型）では、`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` のインスタンスメソッド引数を共有借用（`ref`）契約とし、`ownership.rs` の `E::Binary` / `E::Unary` で「非組み込み（または `BigInt`/`Tensor` 等ユーザーインスタンス）の算術演算子は `Use::Read`」として検査する。
  - `polymorph.rs` の `operator_call` は、非組み込み算術演算子を借用引数（ローカルは `ref place`、一時値は内部 materialize 後に解放）で呼ぶ形へ下げる（`A11` の比較演算子と同じ一時値スピル・解放順: 左解放 → 右解放）。
  - **代替案（D1-B）**: `Eq`/`Ord`（D-20）と完全に同じ `ref 'a -> ref 'a -> 'a` を全型へ一括適用する場合、`string + string` は `llvm.rs` で `Use::Read`（常に `@tz.string.concat`、または A17 最終使用時のみ `append`）へ改修し、下記の既存メソッド値テストを新署名へ移行する。着手前の承認で D1-A / D1-B を確定する。

### 2. 所有権・借用検査

- 非 Copy レコード（`BigInt`・`Tensor`・ユーザー型）で `let c = a + b` の後も `a`・`b` はムーブされず再利用できる。
- `a + a` のように同一変数を左右両辺に書いても共有借用同士なので受理される。
- `left + { left = ...; left }` のように左辺借用中に右辺で左辺を変更する式は従来どおり `E1014` で拒否される。
- `(a + b) * (c - d)` のネスト式では、中間結果の一時値が評価順序通りに確保・借用・解放される（`live == 0`）。

### 3. `std/BigInt.tz`・`std/Complex.tz`・ジェネリック利用者の更新と `Drop`/edition の扱い

- `std/BigInt.tz` の `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` インスタンスを借用契約に合わせて更新する（`sub`/`power` 内の不要な所有消費を取り除く）。
- [NC01](NC01-complex-and-extended-math.md) が先に完了している場合は `std/Complex.tz` の算術インスタンスも同時更新する。また `std/Monoids.tz` のように `Add<'a>` / `Mul<'a>` 制約を持つ既存ジェネリックインスタンスが単相化・所有権検査を引き続き通ることを確認する。
- **ユーザー定義 `Drop` 型の解放時点**: 従来の値渡しでは `a + b` の直後に演算子関数末尾で `a`・`b` の `Drop` が走っていたが、借用化により `a`・`b` がローカル変数であれば呼び出し元の通常スコープ末尾（一時値であれば演算子式の完了時、左解放 → 右解放の順）で `Drop` が走る。これは [A11](_completed/A11-borrowed-comparisons.md)（`Eq`/`Ord` の借用化）と同じ挙動であり、テストおよびリファレンスで明示する。
- **edition 2026 / 2027 の扱い**: [A11](_completed/A11-borrowed-comparisons.md)（D-20）と同様、edition による分岐を作らず全 edition で同一の契約とする（既定案 D1-A であれば、スカラー・SIMD・`string` と `Add.add` メソッド値の既存 2026 コードは一切壊れない: D4）。

## 設計

- [A11](_completed/A11-borrowed-comparisons.md) が `Eq`/`Ord` で実装した `operator_call` の借用引数化と一時値スピル・解放、`ownership.rs` の `Use::Read`、`call_specialization.rs` の借用判定を算術演算子へ拡張する。
- D1-A を採る場合、`polymorph.rs` の組み込み算術メソッド定義で「スカラー/SIMD/文字列の組み込み解決時は値型、ユーザーインスタンスのパラメータ解決時は `ref`（edition 2027 の `[Read]` 規約と同様）」とするか、ジェネリック関数 `Add<'a> =>` 内の `Add.add` 呼び出しでもスカラーと非 Copy 型が統一して単相化されるよう、ジェネリック制約下では `ref 'a -> ref 'a -> 'a`（呼び出し時の引数規約でスカラーリテラルも透過適用）とする詳細を実装手順 1 の設計確認で確定する。

## 前提とする他チケットのインターフェース

- なし（[A11](_completed/A11-borrowed-comparisons.md) と [A18](_completed/A18-parameter-conventions.md) は完了済み）。

## 他チケットへ提供するインターフェース

- 非 Copy 型への非消費算術演算子 `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg`（[NC06](NC06-tensor-ufuncs-and-elementwise.md) が `Tensor` に実装する）。

## 実装手順

1. **変更**: なし。**内容**: `Eq` と `Add` のコンパイラ内での差異を `src/check.rs`・`src/polymorph.rs`・`src/ownership.rs`・`src/call_specialization.rs`・`src/llvm.rs` で全数調査し、既存 fixture の IR をベースライン保存する。**確認**: 差分リストを記録。
2. **変更**: `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `src/effects.rs`。**内容**: 承認された D1 に従い `Add`〜`Neg`・`Pow` の借用契約と一時値解放、`string` 連結の安全性、PR02 D10 直展開の維持を実装する。**確認**: T0、`tests/borrowed_arithmetic.rs`、既存スカラー/SIMD fixture の IR 不変確認。
3. **変更**: `std/BigInt.tz`、`std/Complex.tz`（NC01 完了済みの場合）、`std/Monoids.tz`（確認・必要なら調整）、および既存テスト（D1-B の場合のみ移行対象テストを更新）。**内容**: 標準ライブラリの算術インスタンスを借用契約へ更新し、ジェネリック制約利用者を確認する。**確認**: T1、`node tests/features.mjs target/debug/tsuzuri bigint`（native/WASM `-O0`/`-O3`、`live == 0`）。
4. **変更**: `tests/fixtures/`・`tests/features.mjs`、`docs/language.md`、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`・`values-and-functions/op-and-expressions.md`・`built-in-types-and-modules/bigint.md`。**内容**: 非 Copy レコード・ユーザー `Drop` 型（解放順序・時点確認）の複合演算子式 E2E と文書更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/borrowed_arithmetic.rs`）**: 非 Copy レコードでの `a + b`・`a + a`・`(a + b) * (a - b)`・`-a`・`a ** b`、`Add.add` 直接呼び出し、借用競合（`a + { a = b; a }` → `E1014`）、`let m = ref mut a; a + b` → `E1014`、ユーザー `Drop` 型の解放時点（ローカルはスコープ末尾・一時値は式完了時に左→右順）、ジェネリック関数 `Add<'a> => ...`（`Monoids` 相当含む）、`string + string` での use-after-free が起きないこと、edition 2026/2027 双方での整合、スカラー/SIMD IR 不変。
- **E2E（`tests/features.mjs`）**: `BigInt` と非 Copy ユーザーレコードの複合算術式で結果一致＋確保追跡 `live == 0`（native/WASM `-O0`/`-O3`）。
- **既存テストへの影響**:
  - D1-A の場合: `tests/fixtures/polymorphism/Main.tz:75`, `tests/fixtures/currying/Functions.tz:124`, `tests/currying.rs:531`, `tests/integer_ranges.rs:469`, `tests/strings.rs:280` はすべて無修正で通過する。`instance Add<[i32]> { fn add x y = x }`（`tests/polymorphism.rs:1060`）や `tests/pure_functions.rs:235` のように仮引数 `x` をそのまま返す既存ユーザーインスタンスは `deref x`（または Copy 読み取り）の確認が必要。
  - D1-B の場合: 上記メソッド値テスト群の型注釈を `ref T -> ref T -> T` へ更新する。

## ドキュメント

- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`（「`Add.add` は値を受け取る」の記述を新契約へ更新）
- `_tsuzuri/language-reference/values-and-functions/op-and-expressions.md`
- `_tsuzuri/language-reference/built-in-types-and-modules/bigint.md`
- `docs/language.md`

## 受け入れ条件

- 非 Copy 型（`BigInt` およびユーザー定義レコード）の `+`, `-`, `*`, `/`, `%`, `**`, 単項 `-` が値をムーブせず動作し、中間一時値のリークがない（`live == 0`）こと。
- `string + string` に use-after-free がなく、スカラー・SIMD の既存 IR と PR02 D10 直展開が変わらないこと。
- T2 と `node scripts/check-docs.mjs` が通ること。

## 落とし穴

- `llvm.rs` の `string + string` は `Use::Consume` を前提に `take_operand` + `@tz.string.append`（左辺のインプレース破壊再利用）を行っている。`ownership.rs` で `string` の `+` を安易に `Use::Read` へ変えると解放済みバッファアクセスになるため必ず同期させる。
- ユーザー定義 `instance Add<MyRecord>` で仮引数 `left` をそのまま返すコード（`fn add left right = left`）は、借用契約になると非 Copy なら `E1012`/コピー不可、Copy なら `deref left`（または自動読み取り view）となる点に注意する。
- `NC01` が先行して `std/Complex.tz` に `Add`〜`Neg` インスタンスが入っている場合、NC03 の承認案（D1-A / D1-B）と同時整合させる必要がある。また `std/Monoids.tz` のような `Add<'a>`/`Mul<'a>` のジェネリック利用者も確認する。
- ユーザー `Drop` を実装した型では `a + b` での解放時点が「演算子呼び出し末尾」から「ローカルのスコープ末尾（または一時値評価完了時）」へ移る。既存テストに演算子での即時 `Drop` を前提とするものがないか確認すること。
- `call_specialization::intrinsic_operation`（PR02 D10）の条件を外すと、`Add.add x y` が演算子 `+` より遅い関数呼び出しへ退行する（GUIDE §1 規則 3 違反）。

## 対象外

- 多パラメータ型クラス・関連型による異種型二項演算（`f64 * Tensor<f64>` 等）。
- 演算融合（式テンプレート / Kernel Fusion）。
- `Bits`（`&&&`/`|||`/`^^^`）や `==`/`<` の契約変更（`Eq`/`Ord` はすでに A11 で借用化済み）。

## 決定事項

- **D1（要承認）**: `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Pow`/`Neg` の非 Copy 型（ユーザー定義および `BigInt`/`Tensor` 等）に対する契約を非消費の共有借用へ改める。既定案は **D1-A**（スカラー・SIMD・`string` 組み込み演算の既存値渡し/消費 IR と `Add.add` メソッド値互換を保ちつつ、非組み込みインスタンスを共有借用契約にする）、代替案は **D1-B**（全型を `ref 'a -> ref 'a -> 'a` に統一し、`string` 連結を非消費化＋既存テスト移行）。着手前に D1-A / D1-B を承認確定する。
- **D2（要承認）**: 異種型演算（多パラメータ型クラス `Add<'l, 'r, 'o>`）は導入しない（案 C の方針。スカラーとの演算は `Tensor.scale` 等のライブラリ関数で担い、型推論の明快さを維持する）。
- **D3（既定案）**: 既存の組み込みスカラー・SIMD の演算は直接 lowering と PR02 D10 直展開を維持し、性能退行をゼロにする。
- **D4（既定案）**: [A11](_completed/A11-borrowed-comparisons.md)（D-20）と同様に edition 2026 / 2027 を分けず全 edition で同一契約とする（D1-A では既存のスカラー・SIMD・`string` および `Add.add` メソッド値コードと完全互換）。
