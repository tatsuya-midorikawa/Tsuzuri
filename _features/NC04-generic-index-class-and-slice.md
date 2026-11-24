# NC04: 汎用 `Index` 型クラスとスライス記述子モジュール `Slice`（案 C・言語拡張 ②）

| 項目 | 内容 |
| --- | --- |
| ID | NC04 |
| 優先度 | P1（パーサーに Python 専用文法を足さずに `Tensor`/`Matrix`/`Map`/`HashMap` で `x[k]` を書けるようにする） |
| 規模 | L（`Index` 組み込みクラスまたは型クラスディスパッチ・`ExprKind::Index` 拡張・`std/Slice.tz`） |
| 依存 | ([NC02](NC02-tensor-core-views-and-broadcasting.md)) |
| 後続 | ([NC05](NC05-tensor-shape-indexing-and-masking.md)) |
| 状態 | todo |
| 起票 | 2026-11-24（[GUIDE D-65](GUIDE.md#d-65-数値計算基盤と-numpy-互換機能案-c-ハイブリッドnc01nc14)。HEAD `27069465` で確認） |
| 承認 | 要承認（組み込み型クラス `Index` の追加・`x[k]` のユーザー型開放・opt-in std モジュール `Slice` の登録。決定事項 D1・D2 が要承認、ほかは既定案） |
| 手本にする既存実装 | `src/check.rs` 13779 行（`ExprKind::Index` の型検査）、`src/polymorph.rs`（組み込みクラス解決）、`std/Text.tt`（標準型クラス）、`std/Tensor.tz`（`narrow`/`index_axis`） |
| 主な影響ファイル | `src/check.rs`、`src/polymorph.rs`（`BUILTIN_CLASSES`）、`src/ownership.rs`、`src/llvm.rs`、`src/stdlib.rs`、`src/diagnostic_kinds.rs`、`std/Slice.tz`（新規）、`std/Tensor.tz`、`std/Matrix.tz`、`std/Map.tz`、`std/HashMap.tz`、`tests/index_class.rs`（新規）、`tests/std_argument_order.rs`、`tests/std_examples.rs`、`tests/fixtures/effects_sweep/`、`vsc/src/core.ts`、`_tsuzuri/language-reference/built-in-types-and-modules/slice.md`（新規）、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`_tsuzuri/language-reference/built-in-types-and-modules/matrix.md`、`_tsuzuri/language-reference/built-in-types-and-modules/map.md`、`_tsuzuri/language-reference/built-in-types-and-modules/hashmap.md`、`_tsuzuri/language-reference/index.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: (1) 現在の `ExprKind::Index`（配列・リスト・`Vec`・固定長配列・`string`・`utf8string` だけにハードコードされている `x[k]`）を、組み込み型クラス `Index`（Phase 2 で `IndexMut` による `x[k] = v` も対応）経由でユーザー定義型および標準コレクションへ開放する。(2) 数値スライス指定用の小さな opt-in 標準モジュール `std/Slice.tz`（`Slice.all`, `Slice.at i`, `Slice.range start stop`, `Slice.step start stop step`, `Slice.from start`, `Slice.to stop`, `Slice.rev`）を追加し、`Tensor`・`Matrix`・`Map`・`HashMap` に `Index` インスタンスを提供する。
- 最初に読む: ① `src/check.rs` 13779〜13815 行（`ExprKind::Index`） ② 仕様 1〜3 ③ `std/Tensor.tz` と `std/Matrix.tz`。
- 最初に書くテスト: `tests/index_class.rs` で組み込み型（`[i64]` 等）の既存動作が不変なこと、ユーザー定義レコードでの `x[k]` 呼び出し、非対応型の `E1005` / `IndexNonIndexable` 診断。
- 触ってはいけない: 新しい予約語・Python 風コロン区切りスライス文法（`a[1::2, ::-1]` は入れない）。既存の `ref xs[a..b]`・`ref mut xs[a..b]` と組み込み配列の添字境界検査除去（F12）および IR。
- 完了の見分け方:

```tsuzuri
let sub = t[[Slice.step 1 10 2, Slice.rev]]
let elem = m[(1, 2)]
let v = map["key"]
```

が型安全かつパーサー文法を複雑にせずに動作し、既存配列の IR が 1 byte も変わらない。

## 目的

Tsuzuri の `x[k]` は現在、組み込みの配列・リスト・`Vec`・固定長配列・文字列の 6 型に固定されており、`Tensor`・`Matrix`・`Map`・`HashMap` やユーザー定義コレクションでは常に `Tensor.at [i, j] v`・`Map.find k m` のように書く必要がある。

一方で、Python のような `a[1::2, ::-1]` 専用文法をパーサーに導入すると既存の `:`（型注釈）・`..`（範囲）・`ref xs[a..b]` と衝突する。そこで、既存の `x[k]` 構文をそのまま汎用 `Index` クラスで開放し、スライス指定は標準型 `Slice` の値として渡すことで、**言語文法を増やさずに**数値計算と一般コレクション双方の記述性を高める（案 C）。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1・D2 の承認。
- `grep -rn "ExprKind::Index\|TypedExprKind::Index" src/` で添字式を扱う全箇所を控える。

### 停止条件

- **S1** Tsuzuri の単一パラメータ型クラス体系の中で、「キー型 `K` と出力型 `V` がコレクション型 `C` によって変わる `Index`」を無理なく表現できない場合（例: `Map<K, V>` のキーは `K`、出力は `V` であり、`Tensor<'a>` では `[i64]` や `[Slice.Axis]` で出力が異なる）。→ 設計 §1 の「単一キー固定の組み込みフック」か「最小限の関連型付き組み込み `Index`」の選択（D1）について止めて相談する。
- **S2** 組み込み配列・リスト・`Vec`・固定長配列・文字列の添字 IR または範囲証明（F12）が 1 byte でも変わる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `27069465` で確認）

- `src/check.rs` 13779 行の `ExprKind::Index(value, index)` は `Type::Array`/`List`/`Vec`/`FixedArray`/`String`/`Utf8String` のみを受理し、それ以外は `DiagnosticKind::IndexNonIndexable` を返す。
- 添字は `index_operand` により整数型（`i8`〜`i128u`）のみ許可されている。
- 型クラスは単一型パラメータ（高階型を除く）で、関連型（associated type）はまだ持っていない。

## 変えてはいけない意味

- 組み込み 6 型（`[T]`, `[|T|]`, `Vec<T>`, `[T; N]`, `string`, `utf8string`）の添字規則（全整数型を受理、境界検査と F12 の範囲証明除去、2027 の `xs[i] = v` 添字代入、生成 IR）。
- 共有スライス `ref xs[a..b]` と排他スライス `ref mut xs[a..b]` の構文と意味。

## 仕様

### 1. 組み込み `Index`（Phase 1: 読み取り、Phase 2: 書き込み `IndexMut`）

- 組み込み 6 型以外の型 `C` に対する `c[k]` で、`C` が `Index` の実装を持つ場合、`Index.index (ref k) (ref c)`（キー `k` もベース `c` も非消費の共有借用・対象 `c` が最後: D-59）へ脱糖・ディスパッチする。
- **キーの所有権と一時値借用（重要）**:
  - `Map<string, V>` や `HashMap<string, V>` で `map[k]` と書いたときに非 Copy のローカル変数 `k` が消費されないよう、メソッド署名は **`index :: ref 'key -> ref 'self -> 'output`**（キー・ベース双方とも共有借用）に一本化する。呼び出し側は既存の読み取り規約によりローカル変数 `map[k]` も一時値 `t[[Slice.range 1 5, Slice.rev]]` もそのまま書ける。
  - クラスメソッド宣言には名前付き region `{ r }` を書けない（`src/regions.rs:496-501` の `reject_local`）。そのまま通常の 2 引数借用関数として扱うと、一時値キー `[Slice.range 1 5, Slice.rev]` の loan が返却値（`Tensor.View<'a>` / `ref 'a`）に混入して常に `E1013` になる。これを防ぐため、**`ownership.rs` において既存の組み込み `E::Index`（14681 行）と同じ「返却値の借用はベース（コンテナ `c`）の loan のみを継承し、キー `k` の一時借用は式評価の完了で閉じる」規則**を強制する（同時に、`Index` インスタンス本体がキー `k` の借用を返却値へ逃がす実装は `E1013` で静的に拒否する）。
- **インスタンス宣言構文とメソッド型（D1 で確定・要承認）**:
  - 組み込みクラス `Index`（`src/polymorph.rs` の `BUILTIN_CLASSES` に追加し、承認時に `GUIDE.md` D-55 へ登録）に限定して、**3 引数ヘッダ `instance Index<Self, Key, Output>`**（`(Self, Key)` の組で一意に `Output` を決める専用規則、重複する `(Self, Key)` 頭は `E1016`）を許す。これにより関連型構文を新設せずに 1 つの `Tensor<'a>` へ 3 種のキーを曖昧さなく載せられる:
    ```tsuzuri
    // 例 1: 座標配列で要素を借用（または Copy 読み取り）
    instance Copy<'a> => Index<Tensor<'a>, [i64], 'a> {
        fn index coords self = deref (Tensor.at (Array.copy coords) (Tensor.view self))
    }
    // 例 2: スライス記述子でゼロコピービューを返す
    instance Index<Tensor<'a>, [Slice.Axis], Tensor.View<'a>> {
        fn index axes self = Tensor.slice (Array.copy axes) (Tensor.view self)
    }
    // 例 3: ブールマスクで 1D テンソルを抽出（NC05 で追加）
    instance Copy<'a> => Index<Tensor<'a>, Tensor<bool>, Tensor<'a>> {
        fn index mask self = Tensor.mask (Tensor.view mask) (Tensor.view self)
    }
    ```
  - 代替案として関連型 `type Output = ...` を `Index` 限定で許す案もあるが、パーサー変更が小さい 3 引数ヘッダ `Index<Self, Key, Output>` を既定案とする（一般のユーザー定義多パラメータ型クラスは開放しない）。
- 返却値が借用（`ref 'a` や `Tensor.View<'a>`）を含む場合、ベース式 `c` の寿命を正しく継承し、`E1013`/`E1014` 検査と完全整合させる。

### 2. `std/Slice.tz`（opt-in）

- 1 軸分のスライス指定を表す値型共用体:
  ```tsuzuri
  union Axis =
      | All
      | At of i64
      | Range of (i64 * i64)
      | From of i64
      | To of i64
      | Step of (Maybe<i64> * Maybe<i64> * i64)
      | NewAxis
  ```
- コンストラクタ関数（すべて `pure`、負のインデックスは軸長 `len` に対して `len + i` として解釈する規約を `Slice.resolve` で一元提供）:
  - `Slice.all()`（全要素）
  - `Slice.at :: own i64 -> Axis`（軸を固定してランクを 1 下げる。負インデックス対応）
  - `Slice.range :: own i64 -> own i64 -> Axis`（半開区間 `[start, stop)`）
  - `Slice.from :: own i64 -> Axis`（`start` から末尾まで）
  - `Slice.to :: own i64 -> Axis`（先頭から `stop` 未満まで）
  - `Slice.step :: own i64 -> own i64 -> own i64 -> Axis`（`step != 0`、負ステップで逆順）
  - `Slice.every :: own i64 -> Axis`（全範囲を `step` 刻み）
  - `Slice.rev()`（全範囲をステップ `-1` で逆順）
  - `Slice.new_axis()`（長さ 1 の軸を挿入）

### 3. 標準コレクションへの `Index` 提供

- `Tensor` / `Tensor.View`:
  - キー `[i64]` → 要素アクセス（`Tensor.at` 相当、範囲外トラップ）
  - キー `[Slice.Axis]` → ゼロコピー `Tensor.View<'a>`（`NC02` の負ストライド・`narrow`/`index_axis`/長さ 1 挿入を利用）
  - キー `Tensor<bool>` → ブールマスク抽出（[NC05](NC05-tensor-shape-indexing-and-masking.md) で追加）
- `Matrix` / `MatrixView`:
  - キー `(i64 * i64)` → 要素アクセス（`Matrix.at row col` 相当）
  - キー `(Slice.Axis * Slice.Axis)` → 部分 `MatrixView`
- `Map<K, V>` / `HashMap<K, V>`:
  - キー `K` → 値アクセス（見つからない場合はトラップ、`try_find` は従来どおり `Maybe`）

### 4. Phase 2（`IndexMut` による `x[k] = v`）

- 既存の edition 2027 添字代入（`xs[i] = v`）を `IndexMut` 実装型へ拡張し、`mut t` や `Tensor.Mut` に対して `t[[1, 2]] = 9.0` や `t[mask] = 0.0` を書けるようにする。

## 設計

- `ExprKind::Index` は組み込み 6 型を最優先で従来どおり処理し、それ以外の型でのみ `Index` クラス解決へフォールバックする。これにより既存コードの型検査・IR・F12 境界検査除去を一切変えない。
- 構文解析（`src/parser.rs`）とフォーマッター（`src/formatter.rs`）の変更は不要（既存の `x[expr]` をそのまま使う）。
- `std/Slice.tz` は opt-in として `src/stdlib.rs` の `OPT_IN`（`uses: &[]`）・`RESERVED_MODULES` に追加し、`tests/std_argument_order.rs` の `MODULES` へ `("Slice", subject(&["Axis"]))` として登録する。追加した公開関数は `tests/fixtures/effects_sweep/` で網羅呼び出しする。
- 新規診断 `kind` を追加する場合は `src/diagnostic_kinds.rs` と `GUIDE.md` D-55 へ登録する。

## 前提とする他チケットのインターフェース

- 弱い依存 [NC02](NC02-tensor-core-views-and-broadcasting.md): `Tensor` の `Index` インスタンスで負ストライド・`strided` を使用（`Slice` モジュールと `Index` クラス本体自体は NC02 より前にも単独開発可能）。

## 他チケットへ提供するインターフェース

- 汎用 `Index` / `IndexMut` 型クラスと `Slice.Axis`（[NC05](NC05-tensor-shape-indexing-and-masking.md) のマスク添字・ファンシーインデキシングが利用）。

## 実装手順

1. **変更**: なし。**内容**: `src/check.rs`・`src/polymorph.rs`（`BUILTIN_CLASSES` の件数）・`src/ownership.rs` の `Index` 関連全箇所を確認し、既存 fixture の IR を保存する。**確認**: 一覧記録。
2. **変更**: `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/diagnostic_kinds.rs`。**内容**: Phase 1 として組み込み `Index` クラス解決・`ExprKind::Index` フォールバック・コンテナ限定借用継承（一時値キーで `E1013` にならない規則＋キー借用漏出の拒否）を実装する。**確認**: T0、`tests/index_class.rs`、既存 fixture の IR 不変確認。
3. **変更**: `std/Slice.tz`, `src/stdlib.rs`, `std/Tensor.tz`, `std/Matrix.tz`, `std/Map.tz`, `std/HashMap.tz`, `tests/std_argument_order.rs`, `tests/std_examples.rs`, `tests/fixtures/effects_sweep/`, `vsc/src/core.ts`。**内容**: `Slice` モジュールと各標準型の `Index` インスタンス、コーパス網羅呼び出しを追加する。**確認**: T0、`cargo test --locked --test std_argument_order --test std_examples --test effects_alloc_conservative`、`tsuzuri test --doc std`、T1。
4. **変更**: Phase 2（承認後）`IndexMut` による `x[k] = v` 拡張、E2E（`tests/features.mjs`）、文書更新。**確認**: T2、`node scripts/check-docs.mjs`。

## テスト計画

- **Rust テスト（`tests/index_class.rs`）**: ユーザー定義レコードの `Index`、一時値キー（`t[[Slice.range 1 5, Slice.rev]]`, `m[(1, 2)]`, `map["key"]`）が `E1013` にならず通ること、逆にインスタンス本体がキーの借用を返すと `E1013` で弾かれること、借用ビューを返す `Index` の寿命超過 `E1013` / 借用中破壊 `E1014`、重複インスタンス `E1016`、未実装型の診断、組み込み 6 型の IR 不変。
- **E2E（`tests/features.mjs`）**: `Tensor` の座標/スライス添字、`Matrix` の `(row, col)` 添字、`Map`/`HashMap` のキー添字、`Slice` の負インデックス・負ステップ・範囲外トラップを native/WASM `-O0`/`-O3` で確認。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/slice.md`（新規）
- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`（`Index` / `IndexMut`）
- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`・`matrix.md`・`map.md`・`hashmap.md`・`index.md`
- `docs/language.md`

## 受け入れ条件

- パーサーへ Python 専用スライス構文や新予約語を足さずに、`Tensor`/`Matrix`/`Map`/`HashMap` およびユーザー定義型で `x[k]` が動くこと。
- 一時値キーでも `E1013` にならず、かつ借用を返す `Index` でダングリング参照（`E1013`）と破壊競合（`E1014`）が確実に弾かれること。
- 組み込み 6 型の既存 IR・F12 境界検査除去が変わらないこと。

## 落とし穴

- `Map`・`HashMap` などの常時/opt-in モジュールへ `Index` インスタンスを足すとき、`std/Map.tz`（opt-in ではない既存扱い）や `std/Tensor.tz`（`OPT_IN`）の `uses` を確認し、`Tensor` を名指さないプログラムへ `Slice` が漏れ込まないようにする。また `std/Slice.tz` を `RESERVED_MODULES` に追加する際は `src/stdlib.rs:843` の件数アサーションも同時更新する。
- `BUILTIN_CLASSES`（`src/polymorph.rs`）に `Index`/`IndexMut` を追加すると予約クラス名が増えるため、同名のユーザーレコード宣言との衝突（`E1001`）テストと `D-55` 更新を忘れないこと。

## 対象外

- Python 風コロン区切りスライス構文（`a[1::2, ::-1]`）や `:` の文法拡張。
- 一般のユーザー定義多パラメータ型クラス・関連型。

## 決定事項

- **D1（要承認）**: 組み込み型クラス `Index`（Phase 2 で `IndexMut`）を `BUILTIN_CLASSES` に追加し、組み込み限定の 3 引数ヘッダ `instance Index<Self, Key, Output>`（`(Self, Key)` 一意で `Output` を決定、メソッド `index :: ref Key -> ref Self -> Output`）と、コンテナ由来の loan のみ継承する所有権規則を導入する（承認時に `GUIDE.md` D-55 へ登録）。一般のユーザー定義多パラメータ型クラスは開放しない。
- **D2（要承認）**: opt-in std モジュール `std/Slice.tz` を追加し、`RESERVED_MODULES` に `"Slice"` を登録する（`D-55` へ反映）。
- **D3（既定案）**: Python の `a[1::2, ::-1]` 構文やコロン区切りスライス文法は導入しない（Tsuzuri の `:` 型注釈・`..` 範囲・`ref xs[a..b]` との直交性を保つ）。
- **D4（既定案）**: 負のインデックス解釈は `Slice` の範囲指定に限定し、既存の `Array` 添字 `xs[-1]`（トラップ）の契約は変更しない。
