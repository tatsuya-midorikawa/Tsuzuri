# NC19: 演算子の拡張インスタンス（`View` どうし・`Tensor` と `View`・スカラーとの演算子）

| 項目 | 内容 |
| --- | --- |
| ID | NC19 |
| 優先度 | P1（NumPy の `a[1:] - a[:-1]`・`2.0 * x + 1.0`・`t + v` を、関数ではなく演算子でそのまま書けるようにする） |
| 規模 | L（3 引数・2 引数のインスタンスのヘッダ、1 つの演算子表と OEI の振り分けの位置、`binary_expression`・`binary_type` の解決の順序と柔らかいヒント、呼び出しへの脱糖、std の 43 インスタンスの生成） |
| 依存 | [A37](A37-index-base-place-borrow.md)（手順 5: 算術クラスの orphan 規則と `dispatch_positions`。着手条件）, [NC06](NC06-tensor-ufuncs-and-elementwise.md), ([NC16](NC16-index-ref-and-zero-alloc-element-reads.md)), ([NC18](NC18-multi-axis-bracket-indexing.md)), ([A40](A40-reference-operands.md))（順序の推奨だけ。A40 を先に入れるのを推奨する。NC19 は A40 に依存しない） |
| 後続 | [NC20](NC20-compound-assignment-and-mutable-index-views.md), [NC21](NC21-complex-real-scalar-operators.md) |
| 状態 | done |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` で確認）。同日、D-66 の委任による決定を反映（差分は末尾の「改訂」） |
| 承認 | D1 は承認済み（D-66 の委任で決定、2026-11-25。完了済みの [NC03](_completed/NC03-borrowed-arithmetic-operators.md) の D2 の改訂。被覆の規則、総称関数の型変数、リテラルの型、互換性、`extern type` の 5 点を足した）。D2 は承認済み（D-66 の委任で決定、2026-11-25。1 つの演算子表。orphan 規則は [A37](A37-index-base-place-borrow.md) の D4 と同じ 1 つの規則）。D9 は (i) 却下、(ii) [A40](A40-reference-operands.md) に分けて承認、(iii) `Complex` は [NC21](NC21-complex-real-scalar-operators.md) に分けて承認・`BigInt` は却下（どれも D-66 の委任で決定、2026-11-25）。D10 は承認済み（D-66 の委任で決定、2026-11-25）。ほかは既定案 |
| 手本にする既存実装 | [NC04](_completed/NC04-generic-index-class-and-slice.md) の組み込み限定の 3 引数ヘッダ（`src/parser.rs` 914〜933 行付近、`src/polymorph.rs` の `register_index_instance` 2495〜2685 行付近と `resolve_index` 2758 行付近）、`Index` の呼び出しへの脱糖（`src/check.rs` 14748〜14833 行付近）、同じ型どうしの演算子の下げ方 `operator_call`（`src/polymorph.rs` 6678〜6741 行付近）。orphan 規則は A37 の手順 5 が作る `validate_orphan_instance`・`dispatch_positions` に OEI の位置を足すだけ（A37 の手本は `validate_drop_instance`、`src/polymorph.rs` 3913 行） |
| 主な影響ファイル | `src/parser.rs`、`src/polymorph.rs`（A37 の `dispatch_positions` に OEI の行を足す）、`src/check.rs`、`src/diagnostic_kinds.rs`、`src/explain.rs`・`src/explain/codes/E1043.md`（NC16 が未登録なら）、`src/stdlib.rs`（A37 のテスト `std_instances_with_open_dispatch_positions_are_listed` の一覧にスカラーの 24 個を足す）、`std/Tensor.tz`、`scripts/gen-tensor-operators.mjs`（新規）、`tests/operator_extension.rs`（新規）、`tests/tensor-operator-cases.mjs`（新規）、`tests/features.mjs`、`tests/conformance/TYP/`、`spec/rules/TYP.md`、`_tsuzuri/language-reference/types-and-type-inference/type-classes.md`、`_tsuzuri/language-reference/types-and-type-inference/type-inference.md`、`_tsuzuri/language-reference/values-and-functions/op-and-expressions.md`、`_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`、`docs/language.md`、`docs/architecture.md`、`docs/stability.md` |

## 実装者向け早見

- 範囲: (1) 組み込みの算術クラス `Add`・`Sub`・`Mul`・`Div`・`Rem`・`Pow` に 3 引数のヘッダ `instance … => Sub<L, R, O>`、`Neg` に 2 引数のヘッダ `Neg<S, O>` を許す（演算子の拡張インスタンス。以下 OEI）。L・R の一方が裸の型変数なら、その型変数はもう一方の側に現れなければならない（被覆の規則）。(2) 同じ型どうしのインスタンス（組み込みでない型）と OEI を 1 つの演算子表に入れ、重なりを `E1016` で検査する。orphan 規則は [A37](A37-index-base-place-borrow.md) の 1 つの規則で、NC19 は A37 の `dispatch_positions` に OEI の振り分けの位置（3 引数は L・R、`Neg` は S）を足すだけ。(3) 演算子の型付けで、両辺を単一化する前に (a)〜(d) の順で OEI を引き、見つかれば普通の総称呼び出しに下げる。総称関数の型変数は既知の型として扱う。(4) std の `Tensor`/`View` に、演算子ごとに 7 形（`View`×`View`、`Tensor`×`View`、`View`×`Tensor`、スカラー×`Tensor`、`Tensor`×スカラー、スカラー×`View`、`View`×スカラー）と `Neg<View, Tensor>` の 43 個を生成する。本体は NC06 のカーネルを 1 回呼ぶだけ。
- 最初に読む: ① 仕様 3（解決の順序）と仕様 4（下げ方） ② `src/check.rs` の `binary_expression`（14209〜14290 行付近）と `binary_type`（17001〜17026 行付近） ③ `src/polymorph.rs` の `register_index_instance` と `resolve_index` ④ [NC06](NC06-tensor-ufuncs-and-elementwise.md) の仕様 2（スカラー版の表）・仕様 4（同じ型どうしのインスタンス）・仕様 5（`*_or_trap`） ⑤ [A37](A37-index-base-place-borrow.md) の D4（orphan 規則と `dispatch_positions`）。
- 最初に書くテスト: `tests/operator_extension.rs` で、利用者の型 `Money` に `instance Mul<Money, i64, Money>` を書いて `price * 3` が `Money` になること、`Mul<Money, i32, Money>` を足すと `E1043` になること、`instance Mul<Tensor<f64>, f64, Tensor<f64>>` が `E1016`（kind `orphan_instance`）になること（1 引数の `instance Sub<Tensor.View<i64>>` は A37 で拒否済み）、`instance Mul<Money, 'b, Money>` が `E1016`（kind `operator_instance_form`）になること、スカラー・`string`・`BigInt` の演算子の IR が変わらないこと。
- 触ってはいけない: スカラー・SIMD・`string`・`BigInt`・`Complex`・利用者の同じ型どうしの演算子の型付けと IR（`Tensor` を名指すプログラムでも）。`Add.add` のメソッド値と `Add<'a> =>` の制約。評価の順序（EVL-001）と、非組み込み型の被演算子を共有借用で貸す契約（NC03 D1-A）。`Eq`・`Ord`・`Bits` のインスタンスの規則（D-20）。
- 完了の見分け方: edition 2028 で `ref a[1..] - ref a[..^1]`（NC17・NC18 の後）、どの edition でも `ref a[[Slice.from 1]] - ref a[[Slice.until (-1)]]`（NC16 の後）・`2.0 * x + 1.0`・`x - 1`・`t + Tensor.view (ref row)`・`-v` が native/WASM `-O0`/`-O3` で期待値を返し、関数版（`Tensor.sub`・`Tensor.scale` など）とビット単位で同じ値になる。

## 目的

NC06 が `Tensor` に足す演算子は同じ型どうし（`Tensor<'a> + Tensor<'a>`）だけで、NumPy でいちばんよく書く形、つまり窓どうしの差 `a[1:] - a[:-1]`、スカラーとの演算 `2.0 * x`・`x - 1`、所有テンソルと窓の混在 `t + v` は、今の型規則では書けない。`binary_type` が両辺の型を単一化し、結果を左辺の型にするからである（現状）。

利用者は `Tensor.sub (ref v) (ref w)` や `Tensor.scale 2.0 (ref (Tensor.view x))` と書くしかなく、数式がそのまま読めない。このチケットは、一般の多引数クラスを開かずに、組み込みの算術クラスに限った 3 引数のインスタンス（NC04 の `Index<Self, Key, Output>` と同じ形）を演算子の構文だけに効かせ、型推論の探索を増やさずにこれらを書けるようにする。結果はいつも所有の `Tensor` で、確保は型に見え、要素の計算は関数版と同じカーネルを通る。

## 着手条件と停止条件

### 着手条件

- [NC06](NC06-tensor-ufuncs-and-elementwise.md) が done（`*_or_trap`、スカラー版の表、同じ型どうしの `Tensor` のインスタンスがあること）。
- [A37](A37-index-base-place-borrow.md) の手順 5 が done（1 引数の算術クラスの orphan 規則、`dispatch_positions`、テスト `std_instances_with_open_dispatch_positions_are_listed` があること）。利用者と std のインスタンスの重なりを利用者の位置で報告する A37 の段 7（Q-std-overlap-location）も done であること。A37 の後に入るので、NC19 は入る時点で受理しているプログラムを拒否しない（D1 の (4)）。
- 決定事項 D1・D2 は承認済み（D-66 の委任で決定、2026-11-25）。D9 の (ii)・(iii) は別のチケット（A40・NC21）で、NC19 の着手条件ではない。
- [A40](A40-reference-operands.md) は依存ではない。A40 を先に入れるのを推奨する（edition 2027 の読み取り引数を、`deref` なしに演算子の被演算子にできる）。NC19 が先でもよく、後に入る方が他方の経路に参照を外す処理を足す（仕様 8）。
- [NC16](NC16-index-ref-and-zero-alloc-element-reads.md) は弱い依存。NC16 の後なら窓は `ref t[[Slice…]]` で作り、その型は `Tensor.View<'a>`。NC16 の前に実装するときは、テストで `t[[Slice…]]`（`ref` なし）と書く（NC16 が移す）。今の `ref t[[…]]` は `ref Tensor.View<'a>` になり、OEI は引かれない（現状のプローブ P15）。
- [NC18](NC18-multi-axis-bracket-indexing.md)・[NC17](NC17-edition-2028-inclusive-range-family.md) は弱い依存。両方が done なら、edition 2028 の短い形 `ref a[1..] - ref a[..^1]` のテストも足す。done でなければ長い形だけで受け入れ条件を満たす。
- 基準: `gh run list --branch main --workflow ci.yml --limit 1` の最新の結果（AGENTS.md の Verification）。

### 停止条件

- **S1** OEI の解決か柔らかいヒント（仕様 3）を入れたことで、今受理しているプログラムの型か生成 IR が 1 つでも変わる場合（段 1 のベースラインとの差）。`Tensor` を名指すプログラムのスカラーの演算子も含む。
- **S2** std の 43 インスタンスが互いに、または NC06 の同じ型どうしのインスタンスと `E1016` で重なる場合、あるいは重なりの検査が 1024 組の上限（`E1017`、kind `instance_overlap_limit`）に届く場合。仕様 2 の「重ならない理由」（単一化の出現検査）が成り立たないので、止めて報告する。
- **S3** OEI の呼び出しの結果が、普通の総称呼び出しと違う借用を持つ場合（仕様 4 のテストで、借用を持つ要素型の結果から土台を動かしても `E1014` にならない、など）。
- **S4** NC06 のカーネルの名前・型が仕様 6 の表と違い、本体を「カーネルを 1 回呼ぶだけ」にできない場合。即興で別のループを書かず、NC06 のチケットと合わせてから進める（起票時の名前は 2026-11-25 の統合で照合済み）。
- **S5** GUIDE-LITE §9 の共通の条件。
- **S6** A37 が入れた `dispatch_positions`・`validate_orphan_instance`・一覧のテストが、仕様 2 の「OEI の位置を足すだけ」で使えない場合（例えば位置の表がクラス名と型引数の数で引けない）。NC19 の中に別の orphan 規則を書かず、A37 のチケットと合わせてから進める。

## 現状（HEAD `858a949c` で確認）

コード:

- **演算子は同じ型どうしだけ**: `binary_type`（`src/check.rs` 17001〜17026 行付近）は、まず `self.same(&right.ty, &left.ty, right.span)`（17012 行）で両辺を単一化し、`require(binary_class(operator), left)` でクラスを要求し、結果を左辺の型にする（17022〜17025 行付近）。クラスのメソッドは `'a -> 'a -> 'a`。
- **被演算子のヒント**: `binary_expression`（14209〜14290 行付近）は、式の期待型を左の被演算子のヒントにし（14243 行付近、比較と `&&`・`||` は除く）、右の被演算子には左の型をヒントに渡す（14263 行付近の `sibling_hint`）。左だけが接尾辞のない数値リテラル（`Self::untyped_number`、16479 行付近。リテラルと、それに単項演算子を付けた式）なら、右を先に検査し、リテラルに右の型を渡す（14256〜14260 行付近）。ヒントは硬い。値の式は `finish_expression` の `self.same(&value.ty, expected, span)`（13742 行付近）、呼び出しは引数より先に `call_expression` の `self.same(&result, &hint, …)`（14096 行付近）で、ヒントと型が違えばその場で `E1003` になる。
- **単項の `-`**: `require("Neg", operand.ty)` で、結果は被演算子の型（14476〜14497 行付近）。
- **3 引数のヘッダは `Index`/`IndexMut` だけ**: パーサーは `Index`・`IndexMut` だけ 3 引数を受け、ほかのクラスは `expected exactly one type argument`（`E0002`、`src/parser.rs` 915〜931 行付近）。登録は `register_index_instance`（`src/polymorph.rs` 2495〜2685 行付近）で、(Self, Key) の組の重なりを組ごとに新しい `Inference` で単一化して `E1016` にし、メソッドの関数を `index_instance_function`（4004 行付近）で作る。
- **同じ型どうしのインスタンスの登録**: `src/polymorph.rs` 2128〜2330 行付近。(クラス, `head_constructor`) で索引して重なりを `E1016` で検査し、比べる組は 1024 組まで（超えると `E1017`、kind `instance_overlap_limit`）。std の型への算術インスタンスを利用者のモジュールが書くことを止める検査はない（A37 の手順 5 が orphan 規則を入れる。D2）。持ち主だけに許す前例は `Drop`・`Format` の対象の検査（`validate_drop_instance`、3913 行付近。std のモジュールは自分が宣言した型にだけ書ける `own_std_type`、3932 行付近）。
- **同じ型どうしの演算子の下げ方**: `operator_call`（6678〜6741 行付近）は、組み込みでない型の算術インスタンスを `ref` の引数の関数として呼び、被演算子を `TypedExprKind::BorrowOperand` で貸す（NC03 D1-A。`borrowed_arithmetic_class`、941〜943 行付近。`Bits` は入らない）。所有権検査は組み込みでない型の算術の被演算子を `Use::Read` で評価する（`src/ownership.rs` 15095〜15124 行付近）。
- **`resolve_index` は候補が複数でも先頭を選ぶ**: 候補ごとに `Inference` を丸ごと複製して試し（`src/polymorph.rs` 2780 行付近）、残った候補の先頭（2832 行）を選ぶ。巻き戻しで試す `Inference::unifies` は別にある（714〜735 行付近。A32）。単一化には出現検査がある（606〜617 行付近、`infinite inferred type`）。
- **頭の分からない型**: `is_unknown`（`src/polymorph.rs` 256〜258 行）は、総称関数の型変数 `Type::Variable` と未解決の推論変数 `Type::Infer` の両方を未知とみなす。`unify`（546 行以降）が束縛するのは `Type::Infer` だけで、`Type::Variable` は自分自身とだけ単一化する。
- **リテラルの型付け**: `integer`（`src/check.rs` 16487 行）は、接尾辞のない整数を、浮動小数点の期待型なら浮動小数点として（16498 行）、`BigInt` の期待型なら `BigInt` として（16515 行）読む。期待型がないか未知（`is_unknown`）なら、その型（なければ新しい推論変数）に `Integer`（負なら `SignedInteger`）の制約を足す（16519〜16530 行）。浮動小数点リテラルは、期待型が未知なら `Float` の制約を足す（14368〜14373 行）。そのため、総称関数の本体の `x * 2` はリテラルを型変数として読む。
- **総称関数の演算子の制約**: 総称関数の本体の演算子が要るクラスは、宣言しなくても関数の制約に加わり、呼び出し側で検査される（P19）。`E1027`（kind `undeclared_instance_constraint`）は instance のメソッドの本体だけで出る（`src/polymorph.rs` 5834〜5857 行）。
- **`extern type`**: 型は `Type::Handle`（`src/check.rs` 178 行）。利用者の `extern type` は 1 引数の算術インスタンスを持てる（P20）。
- **std の opt-in モジュールのインスタンスの検査**: `src/stdlib.rs` のテスト `opt_in_modules_are_reached_only_through_their_names`（708 行付近）は、インスタンスの最初の型引数（`instance.ty`）の頭がそのモジュールの型かを見る（739〜775 行付近）。L が型変数の OEI（`Mul<'a, Tensor<'a>, Tensor<'a>>`）はこのままでは通らない。A37 の手順 5 が、このテストを `dispatch_positions` のすべての位置で見るように直す。`Tensor` は名指したときだけ読み込まれる（`src/stdlib.rs` 577〜586 行）。
- `std/Tensor.tz` の演算子インスタンスは `Eq<Tensor<'a>>`（1514 行付近）と `Display` だけ（NC06 は todo）。`view`（207〜208 行）は shape の複製と `dense_strides` で 2 回確保する。

dev バイナリ（`target/debug/tsuzuri run -O0`、HEAD `858a949c`）のプローブ。`t`・`u` は `Tensor.init [4] (\i -> i)`（`Tensor<i64>`）、`v` は `t[[Slice.from 1]]`（`Tensor.View<i64>`）:

| # | 書いたもの | 結果 |
| --- | --- | --- |
| P1 | `instance Sub<Tensor.View<i64>, Tensor.View<i64>, Tensor<i64>> { … }` | `E0002` `expected exactly one type argument` |
| P2 | `let a = t[[Slice.from 1]]`、`let b = t[[Slice.until (-1)]]`、`a - b` | `E1005` `no instance for Sub<Tensor.View<i64>>; define an instance or use a supported type` |
| P3 | `t - 1` | `E1003` `expected Tensor<i64>, found i32` |
| P4 | `let x = Tensor.init [3] (\i -> 1.5)`、`2.0 * x` | `E1003` `expected Tensor<an undetermined type>, found f64` |
| P5 | `t + v` | `E1003` `expected Tensor<i64>, found Tensor.View<i64>` |
| P6 | `t + Tensor.view (ref t)` | `E1003` `expected Tensor<i64>, found Tensor.View<an undetermined type>`（右の呼び出しが、引数より先に左の型と単一化された） |
| P7 | `-v` | `E1005` `no instance for Neg<Tensor.View<i64>>; …` |
| P8 | 利用者のモジュールの `instance Sub<Tensor.View<i64>> { fn sub a _b = Tensor.slice [Slice.all()] a }`、`let w = v - v`、`Tensor.count (ref w)` | 受理され `3` を出す（`v` は 3 要素。std の型の演算子を利用者が決め、結果が被演算子を借りる `View` になる。A37 の手順 5 の後は `E1016`、kind `orphan_instance` で、NC19 の前に拒否される） |
| P9 | `record Money { cents: i64 }`、`instance Mul<Money>`、`m * 2` | `E1003` `expected Main.Money, found i32` |
| P10 | `instance Index<Foo, i8, string>` の後に `Index<Foo, i16, string>`、`f[1]` | `i8`（宣言を入れ替えると `i16`。宣言の順で決まる） |
| P11 | 利用者の `instance Mul<MatrixView<i64>> { fn mul a _b = deref a }`、`let q = mv * mv`、`let moved = base` | `E1014`（演算子の結果が被演算子の値の借用を持ち続ける。A37 の後は `MatrixView` が std の型なのでこのインスタンス自体が `E1016` になり、同じ形は利用者の region 付きの型で確かめる） |
| P12 | `record R { r } { own_data: [i64], b: ref { r } [i64] }`、`instance Add<R> { fn add x _y = R { own_data: [], b: ref x.own_data[0..] } }`、`let z = (mk 100 (ref e)) + (mk 200 (ref e))`、`z.b[0]` | `E1013` `reference has no live owner`（被演算子をローカルに束縛しても同じ。被演算子の場所を借りる結果は使う場所で拒否される） |
| P13 | `def ident :: 'a -> 'a`、`let k: f64 = 1.5`、`k * ident 2` / `let n = ident 2`、`k * n` | `3` / `E1003` `expected an integer, found f64`（スカラーの演算子でも右へのヒントが型を決めている） |
| P14 | 配列 `ref xs[1..] - ref xs[..<4]` | `E1005` `no instance for Sub<ref [i32]>` |
| P15 | `ref a[[Slice.from 1]] - ref a[[Slice.until (-1)]]` | `E1005` `no instance for Sub<ref Tensor.View<i32>>`（NC16 の前は `ref` が窓への参照を作る） |
| P16 | `t + u` | `E1005` `no instance for Add<Tensor<i64>>`（NC06 の前） |
| P17 | edition 2027 の `def add2 :: BigInt -> BigInt -> BigInt`、`fn add2 a b = a + b` | `E1003` `expected BigInt, found ref BigInt; parameter 'b' is read-only by default (edition 2027); …`（読み取り引数を被演算子にすると、同じ型どうしの演算子でも書けない） |
| P18 | 利用者のモジュールの `instance Eq<'a> => Eq<Maybe<'a>>`（`tests/fixtures/typeclasses/Main.tz` 16 行） | 受理される（`Eq` には orphan 規則がない） |
| P19 | `def lit :: Add<'a> => 'a -> 'a -> 'a`、`fn lit x y = x * y + x`、`lit 3 4` / `lit "x" "y"` | `15` / 呼び出しの場所で `E1005` `no instance for Mul<string>; …`（文脈にない `Mul<'a>` が関数の制約に加わる） |
| P20 | `extern type copy Buf`、`instance Add<Buf> { fn add x _y = deref x }` | 受理される |
| P21 | `let x = true &&& false` | `E1005` `no instance for Bits<bool>; …` |
| P22 | `let big = BigInt.of_i64 5`、`big * 2`・`3 * big` / `let n: i64 = 2`、`big * n` | 受理される / `E1003` `expected BigInt, found i64` |
| P23 | `let x: Tensor<f64> = Tensor.full 1.5 [3]`、`let k: f64 = 2.0`、`k * x` / `let k: i64 = 2`、`k * v` / `let y: Tensor<f64> = k * x` / `def scale_all :: own 'a -> own Tensor<'a> -> Tensor<'a>`、`fn scale_all s t = s * t` | どれも `E1003`: `expected f64, found Tensor<f64>`（位置は `x`。兄弟のヒント `f64` が硬い）/ `expected i64, found Tensor.View<i64>`（`v`）/ `expected Tensor<f64>, found f64`（`k`。式の期待型）/ `expected Tensor<'a>, found 'a`（`s`。結果の型のヒント）。NC19 の (i) で `k`・`s` へのヒントは柔らかくなるが、`x`・`v`・`t` への兄弟のヒント `f64`・`i64`・`'a` は (ii) がなければ硬いまま（仕様 3 の 1） |
| P24 | 利用者の同じ型どうしの `instance Add<Tensor<f64>> { fn add _l _r = Tensor.full 7.0 [1] }`・`instance Mul<Tensor<f64>> { fn mul _l _r = Tensor.full 8.0 [1] }`（NC06 のインスタンスの代わり。A37 の前なので書ける）、`t: Tensor<f64>` で `t + (if b then Tensor.full 2 [2] else Tensor.full 3 [2])`・`t + { Tensor.of_array [2] [1, 2] }`・`t * (match k with \| 3 -> Tensor.full 2 [2] \| _ -> Tensor.full 3 [2])`。利用者の型 `record V<'a> { x: 'a }` と `instance Mul<V<f64>>` で、`v: V<f64>` の `v * (if b then V { x: 2 } else V { x: 3 })`・`v * { V { x: 2 } }`・`match` の形、`v * V { x: 2 }` | 受理（`[7] [7] [8]`、`V` は `3 3 3` と `3`）。ヒント `Tensor<f64>`・`V<f64>` が枝とブロックの末尾に届き、`2` などを `f64` に読む。同じ被演算子を `let d = …` に束縛してから `t + d`・`t * d`・`v * d` にすると `E1003` `expected an integer, found f64`（位置はリテラル）。インスタンスがなければ `if` の形は `E1005` `no instance for Add<Tensor<f64>>`（NC06 の前） |
| P25 | `Tensor` を名指すプログラム（`let x: Tensor<f64> = Tensor.full 1.5 [3]`）で `let k: f64 = 2.0`、`k * (if b then 1 else 2)` / `let d = if b then 1 else 2`、`k * d` | `2` / `E1003` `expected an integer, found f64`（スカラーのヒントが枝のリテラルの型を決めている。P13 と同じ仕組み）。`let v: V<f64> = …` の `k * v`（P24 の `V`）も P23 と同じ `E1003` `expected f64, found Main.V<f64>` |

P19〜P22 は 2026-11-25 の決定のときに同じバイナリで確かめた（P20〜P22 は `tsuzuri check`）。P23〜P25 は同じ日のレビューの修正のときに、HEAD `858a949c` の dev バイナリ（`tsuzuri run`）で確かめた。

## 変えてはいけない意味

- 今受理しているプログラムの型と生成 IR。とくにスカラー・SIMD・`string` の演算子（組み込みの直接の命令）、`BigInt`・`Complex`・利用者の型の同じ型どうしのインスタンスの呼び出し、`Add.add` の直展開（PR02 D10、`call_specialization::intrinsic_operation`）。`Tensor` を名指して std の OEI が読み込まれたプログラムでも変えない（S1）。
- 評価の順序: 左を評価してから右を評価する（EVL-001）。一時値は呼び出しの後に左、右の順で解放する（NC03・A11 と同じ）。
- NC03 D1-A: 組み込みでない型の算術インスタンスは被演算子を共有借用で受け、`a - b` の後も `a`・`b` を使える。
- `Add<'a>` などのクラスは型変数 1 つのまま。`Add<'a> =>` の制約、`Add.add` のメソッド値、`Monoids` などの総称コードの意味。
- 数値の意味: 丸め、`NaN`、`±0.0`、整数の折り返し、整数のゼロ除算のトラップ、整数の `/` の 0 方向の切り捨てと `%` の符号（`_tsuzuri/language-reference/values-and-functions/op-and-expressions.md` 86 行付近）。暗黙の FMA・`fast-math`・再結合を入れない。
- D-20: `==`・`<` などはいつもスカラーの `bool`。`Eq`・`Ord`・`Bits` のインスタンスの規則（P18 の利用者の `Eq<Maybe<'a>>` は受理のまま）。
- NC06 の関数版の契約（形の不一致は `Result.Error`）。
- OEI が当てはまらないプログラムの診断のコード（型が違えば `E1003`、同じ型でインスタンスがなければ `E1005`）。
- NC19 が入る時点（A37 の orphan 規則の後）で受理しているプログラムを拒否しない（D1 の (4)）。

## 仕様

### 1. 宣言の形

```text
instance [Context =>] Add<L, R, O> { fn add left right = … }      // Sub・Mul・Div・Rem・Pow も同じ形
instance [Context =>] Neg<S, O>    { fn neg value = … }
```

- 対象は組み込みの算術クラス `Add`・`Sub`・`Mul`・`Div`・`Rem`・`Pow`（3 引数）と `Neg`（2 引数）だけ。1 引数の形（同じ型どうしのインスタンス）は今のまま。ほかのクラスの多引数のヘッダ、利用者が宣言する多引数のクラスは開かない（`E0002`）。
- メソッドの型は NC03 D1-A と同じく被演算子を共有借用で受ける: `add :: ref L -> ref R -> O`、`neg :: ref S -> O`。メソッド名は 1 引数の形と同じ（`add`・`sub`・`mul`・`div`・`rem`・`pow`・`neg`）。
- `O` は `L`・`R` に現れる型変数だけを使える。文脈の制約も頭に現れる型変数だけ（どちらも `E1027`、kind `instance_context_variable`。`Index` と同じ）。
- OEI は演算子の構文 `l op r`・`-s` にだけ効く。制約（`Add<'a> =>`）、メソッド値（`let f = Sub.sub`）、`Sub.sub l r` の直接の呼び出しの対象にはならない。新しいクラス名・予約語・トークンは足さない（`BUILTIN_CLASSES` は 36 のまま）。
- edition: 全 edition で同じ。今 `E0002`・`E1003`・`E1005` になる場所だけを受理に変える。
- 互換性（D1 の (4)）: NC19 は A37 の手順 5 の後に入る（D2）。P8 の `instance Sub<Tensor.View<i64>>` のような、std の型への利用者の 1 引数のインスタンスは、NC19 の前に A37 で `orphan_instance` になっている。std の OEI と重なりうる利用者のインスタンスは、std の型を頭に書いた OEI だけで、OEI は NC19 より前には書けない。したがって NC19 は、入る時点で受理しているプログラムを拒否しない。

### 2. 1 つの演算子表と宣言の検査（登録時）

- **演算子表**: クラスごとに、(L, R) を鍵として出力 O を決める 1 つの表を持つ。入るのは OEI と、組み込みでない型の同じ型どうしのインスタンス（`Add<T>` は (T, T) → T の項目）。組み込みのスカラー・SIMD・`string` の演算は表に入らない（今どおり直接の命令）。索引は (クラス, `head_constructor(L)`, `head_constructor(R)`)。型変数の側は頭なし（`None`）。
- **組み込みでない名前付き型**: この仕様では、レコード、名前付きの共用体、`extern type`（`Type::Handle`）を指す。型別名は展開してから見る。A37 の orphan 規則と NC20 の Target の規則が `extern type` を名前付きの型として数えるのとそろえる（D1 の (5)）。
- **頭の規則**: L・R（`Neg` は S）の少なくとも一方の頭は、組み込みでない名前付き型でなければならない。両方が組み込みの型（数値・`bool`・`string`・配列・タプルなど）の OEI は `E1016`（kind `operator_instance_form`）。L・R・O がすべて同じ型の 3 引数の形も `E1016`（同じ kind。help は `instance Add<T>` と書く。3 引数の形は総称コードの制約を満たさないので、2 つの綴りで意味を変えないため）。
- **被覆の規則**（D1 の (1)）: L・R の一方が裸の型変数なら、その型変数はもう一方の側の中に現れなければならない。`Mul<'a, Tensor<'a>, Tensor<'a>>`・`Mul<Vec3<'a>, 'a, Vec3<'a>>` は書ける。`Mul<Money, 'b, Money>`・`Mul<'a, Money, Money>` は `E1016`（kind `operator_instance_form`）。理由: この規則の下では、表の 2 つの項目が重なるのは、同じ側のいちばん外の頭に同じ名前付き型を書いたときだけになる。裸の型変数の側は、出現検査によって、相手の側の名前付き型の頭と重ならない。そのため重なりは、どの opt-in のモジュールを読み込んだかに依存しない。
- **重なり**: 表の 2 つの項目の L どうしと R どうしが同時に単一化できれば `E1016`（kind `overlapping_instance`、`overlapping instance for Sub<L, R>`）。O は比べない。OEI と同じ型どうしのインスタンスの組もここで 1 回だけ検査する（同じ型どうしの組は今の検査のまま。二重に報告しない）。比べる組の数は今の検査と同じ 1024 組の上限（`E1017`）に数える。利用者のインスタンスが std のインスタンスと重なれば、`overlapping_instance` を利用者のインスタンスのクラス名の位置で報告し、std のインスタンスを related（role `declared_here`）に付ける（A37 の段 7。Q-std-overlap-location）。std の 7 形が互いと NC06 の `Mul<Tensor<'a>>` と重ならないのは、単一化の出現検査による: 例えば `Mul<'a, Tensor<'a>, Tensor<'a>>` と `Mul<Tensor<'b>>`（(Tensor<'b>, Tensor<'b>)）は `'a ~ Tensor<'b>` と `Tensor<'a> ~ Tensor<'b>` から `'b ~ Tensor<'b>` になり単一化できない。
- **orphan 規則（D2。[A37](A37-index-base-place-borrow.md) の D4 と同じ 1 つの規則）**: 利用者のモジュール（依存パッケージを含む）の算術インスタンスでは、振り分けの位置の少なくとも 1 つの頭が、このプログラムで宣言した型（レコード・名前付きの共用体・`extern type`）でなければならない。振り分けの位置は、OEI では L・R、`Neg<S, O>` では S、1 引数の形では T。O は数えない。std の型、組み込みの型、`ref`・`dyn`、型変数は、このプログラムの型ではない。違反は `E1016`、kind `orphan_instance`。
  - 単位: プログラム。L か R を宣言したモジュールである必要はない。`Money` を宣言したモジュールと別の利用者のモジュールにある `instance Mul<Money, Tensor<f64>, Tensor<f64>>` は受理する。
  - std のモジュール: コンパイラの規則を当てない。std の演算子インスタンスの置き場所は、`src/stdlib.rs` の読み込みのテストが `dispatch_positions` で確かめる（`std/Tensor.tz` は `Tensor`・`View` を宣言するので 7 形を置ける）。
  - 分担: 1 引数の形（`instance Sub<Tensor.View<i64>>`・`instance Add<[i32]>` を含む）の規則は A37 が入れる。NC19 は `dispatch_positions` に OEI の位置（3 引数の [0, 1]、2 引数の `Neg` の [0]）を足すだけ。OEI は NC19 の前には `E0002` で書けないので、今受理しているプログラムへの拒否は増えない。`Index`/`IndexMut` は A37、`AddAssign` 系は [NC20](NC20-compound-assignment-and-mutable-index-views.md) が同じ表に載せる。
  - `Eq`・`Ord`・`Bits` とほかのクラスには当てない（P18）。`Bits` は表に足さない（D9 の (i) を却下した）。
  - 裸の型変数の位置: std の OEI のうち、L か R が裸の型変数の 4 形（6 演算子で 24 個）は、仕様 6 のとおり A37 のテストの一覧に足す。
- **検査の順**: `operator_instance_form`（頭の規則と被覆の規則）、`orphan_instance`、`overlapping_instance` の順。既存の条件（`builtin_instance_override`・`anonymous_union_instance`）はその前のまま（A37 の D4 の順）。
- 本体: メソッドの誤りは `Index` の登録と同じ診断（メソッドがない・知らないメソッドは `E1016`、2 つ書くと `duplicate or reserved name`、引数の数の違いは `E1006`、kind `instance_method_parameter_count`）。

### 3. 解決の順序

二項演算子 `e1 op e2`（`op` は `+`・`-`・`*`・`/`・`%`・`**`）の型付けを、次の順にする。クラス C は `binary_class(op)`（`src/polymorph.rs` 922 行付近）。

0. **C の演算子表に OEI が 1 つもなければ、以下を飛ばして今の道だけを通る**（`Tensor` を名指さないプログラムは何も変わらない）。
1. **被演算子の検査とヒント**（`binary_expression`）:
   - リテラルの側（`untyped_number` が真の被演算子。リテラルと、それに単項演算子を付けた `-1.0` など）: もう一方の型の頭が組み込みでない名前付き型なら (a) の型をヒントにする。そうでなければ今どおり兄弟の型をヒントにする。
   - リテラルでない側に渡すヒント（式の期待型と兄弟の型）は、次の (i) か (ii) のとき**柔らかいヒント**にする。(i) ヒントの型の頭が C の OEI のどれかの L・R・O の頭に現れる（std では `Tensor`・`View`）。(ii) ヒントの種類を問わず（スカラーのヒントと型変数のヒントを含む）、ヒントの型の頭か、被演算子自身の型の頭（値の式の型、呼び出しの結果の型）が、C の OEI のどれかの L・R の頭に現れる。(ii) は、下の単一化の場所で被演算子の型が出たときに判定する。例えば `let k: f64 = 2.0` の `k * x`（`x: Tensor<f64>`）で `x` に渡す兄弟のヒント `f64` と、`scale_all`（仕様 7）の `s * t` で `t` に渡す兄弟のヒント `'a` は、`x`・`t` の頭 `Tensor` が `Mul<'a, Tensor<'a>, Tensor<'a>>` の R に現れるので柔らかい（P23。硬いままだと `E1003` になり (c) に届かない）。どちらでもないヒント（std の OEI だけのプログラムでスカラーの被演算子に渡すスカラーのヒント、OEI に現れない型のヒントなど）は今どおり硬い。P13 の `k * ident 2` は、`ident 2` の結果の型に頭がなく、std の OEI の L・R の頭に `f64` は現れないので硬いまま。柔らかくなっても（利用者の `Mul<Money, f64, Money>` があるなど）、単一化できる場所では今どおり単一化するので結果は同じ。
   - 柔らかいヒントの試し: 被演算子の値の式（識別子・フィールド・添字・`ref` の付いた添字・レコードの構築など、`finish_expression` を通る式）か呼び出しで、`finish_expression`（13742 行付近）か `call_expression`（14096 行付近）が型とヒントを単一化する箇所で、先に `Inference::unifies` で試し、単一化できるときだけ単一化する。できなければその場所でヒントを捨て（エラーにしない）、(b)・(c) へ進む。レコードの構築は、ヒントが同じレコードを名指すときだけ型引数をヒントから取る（`record_literal`、`src/check.rs:15922`）ので、柔らかいヒントもそのまま渡す（`v * V { x: 2 }` の `2` は今どおり `f64`。P24）。
   - `if`・`match`・ブロックの被演算子: ヒントを今どおり中へ渡し、結果の式（各枝の値、ブロックの末尾。結果の位置にある入れ子の `if`・`match`・ブロックの結果の式も）に届ける。柔らかいヒントは結果の式ごとに上の試しで扱い、単一化できない結果の式でだけ捨てる。これらの形からヒントを取り上げない。今はヒントが枝とブロックの末尾の中のリテラルの型を決めていて、取り上げると `t + (if b then Tensor.full 2 [2] else Tensor.full 3 [2])`（`t: Tensor<f64>`、NC06 の後）や `k * (if b then 1 else 2)` が `E1003` になる（P24・P25）。
   - ほかの位置へは効かない: 呼び出しの引数、条件、`match` の対象、ブロックの末尾より前の文には柔らかいヒントを渡さない（入れ子の式）。ラムダの被演算子にも柔らかいヒントを渡さない（関数の型は std の OEI の頭に現れない）。
   - (i)・(ii) とも、試しが失敗してヒントを捨てるのは、今の道がその場所で `E1003` を報告するときだけなので、今受理しているプログラムの型と IR は変わらない。こうすると `t + Tensor.full 2 [3]`（`t: Tensor<f64>`）の `2` は今どおり `f64` に読まれ、`t + Tensor.view (ref row)` は P6 の誤りにならず、`k * x` と `scale_all` の `s * t` は (c) に届く。
2. **(a) リテラルの型**: 一方がリテラル、もう一方の型 T の頭が組み込みでない名前付き型のとき、C の表から、T の側が T と単一化でき、リテラルの側の型が「そのリテラルが取れる型」になる候補を集める。取れる型は、そのリテラルを期待型で検査する `integer`・`float`（`src/check.rs` 16487〜16530 行・14368〜14373 行）が受け付ける型そのものとする（D1 の (3)）。整数リテラルは整数型・浮動小数点型・`BigInt`、浮動小数点リテラルは浮動小数点型である。どちらも型変数と未解決の推論変数を含み、今どおり `Integer`・`SignedInteger`・`Float` の制約を足す。同じ型どうしの項目（`std/BigInt.tz` 240 行の `Mul<BigInt>` の (BigInt, BigInt) など）も候補になる。候補が決めるリテラルの型が 1 つならそれをリテラルの期待型にし、2 つ以上なら `E1043`、0 なら今どおり兄弟の型をヒントにして検査する（今と同じ `E1003`。help に「要素型を暗黙に変換しない」と書く）。リテラルの扱いはスカラーのコードの注入（`let y: f64 = 2.5; y - 1` が `1.5`）と同じで、新しい変換ではない。`let k = 2.0` で束縛した名前や `(1.0 / 3.0)` はリテラルとして扱わない（D3）。
3. **両辺の型が出た後**（`binary_type` の `same` の前）:
   - **(b) 同じ型どうし**: 両辺の型が単一化でき（`Inference::unifies` で試して巻き戻す）、その型に同じ型どうしのインスタンス（組み込み・構造的なものを含む）の頭が合うなら、今の道（`same` → `require`）。スカラーと NC06 の `Tensor + Tensor` はここを通る。region は `Type` に含まれない（`Type::Record(usize, Box<[Type]>)`、`src/check.rs` 139〜158 行付近）ので、別々の region の `View - View` も単一化できるが、`Sub<View>` の頭がないので (c) へ進む。
   - **(c) 演算子表**: どちらかの型の頭が組み込みでない名前付き型で、両辺とも未解決の推論変数（`Type::Infer`）でないなら、表を引く。総称関数の型変数（`Type::Variable`）は既知の型として扱い、表の裸の型変数の側にだけ当てはまる（D1 の (2)。`Type::Variable` も未知とみなす `is_unknown` は使わない）。L・R を組 `(L * R)` にして候補ごとに `Inference::unifies` で試し（巻き戻す。`Inference` を複製しない）、リテラルの制約を持つ推論変数を両立しない型に決める候補を除く（`resolve_index` の 2795〜2826 行付近の検査と同じ）。候補が 1 つなら仕様 4 の呼び出しにする。2 つ以上なら `E1043`（宣言の順で選ばない）。0 なら (d)。
   - **(d) 今の道**: `same` で単一化し `require` する。診断も今と同じ（型が違えば `E1003`、同じ型でインスタンスがなければ `E1005`、kind `no_instance`）。help に、左か右の頭が合う OEI を 4 つまで挙げる。
4. **単項の `-`**: 被演算子の型 S について、`Neg<S>` の頭が合えば今の道、合わず S の頭が組み込みでない名前付き型なら表の (S) を引き（`Neg<S, O>` の S は頭の規則で名前付き型なので、型変数と未解決の推論変数の S には当てはまらない）、1 つなら呼び出し、2 つ以上なら `E1043`、0 なら今と同じ `E1005`。期待型は 1 の規則で柔らかいヒントにする。

### 4. 型付けと下げ方

- OEI が選ばれた式は、型検査の時点で `Index` と同じ形の呼び出しに下げる（`src/check.rs` 14806〜14833 行付近）: `TypedExprKind::Call(GenericFunction(f, type_args), [BorrowOperand(left), BorrowOperand(right)])`（型引数がなければ `Function(User(f))`）。式の型は O を具体化したもの。インスタンスの文脈の制約は `self.constraints` に足す。今の演算子の制約と同じく、総称関数の本体で使えばその関数の制約に加わる。通常の `def` では呼び出し側で検査し、足りなければ呼び出しの場所で `E1005`（P19）。instance のメソッドの本体では、文脈になければ `E1027`（kind `undeclared_instance_constraint`）。
- これは同じ型どうしのインスタンスを `operator_call` が下げる形（`ref` の引数と `BorrowOperand`）と同じで、所有権検査は普通の総称呼び出しとして扱う。結果の借用を特別扱いしない: 被演算子の値が持つ借用は結果へ移り（要素型が借用を持つ `Tensor<MatrixView<i64>>` のような結果から土台を動かすと `E1014`。P11 と同じ）、被演算子の場所そのものを借りる結果は使う場所で `E1013`（P12 と同じ）。OEI の関数には名前付きの region を付けない（`index_instance_function` は出力の region を `Self` に結ぶが、OEI はそうしない）。
- 評価の順序: `e1`、`e2` の順に評価し、両方を貸して呼び、呼び出しの後に一時値を左、右の順に解放する。
- `@checked` の中でも OEI の呼び出しは包まない。呼び出し先の要素の演算は検査されない（「呼び出した関数の中までは検査しません」、`op-and-expressions.md` 95 行付近。同じ型どうしのインスタンスと同じ）。
- native と WASM の違いはない。演算子は移植可能なカーネルだけを呼び、並列・GPU・SIMD の経路を暗黙に選ばない。

### 5. 診断

| コード | kind | 条件 | メッセージ（英語） |
| --- | --- | --- | --- |
| `E0002` | `expected_single_type_argument`（既存） | 算術クラスのヘッダの型引数が 1・3（`Neg` は 1・2）以外 | `Add, Sub, Mul, Div, Rem and Pow instances take one type argument, or three: Add<Left, Right, Output>` / `Neg instances take one type argument, or two: Neg<Self, Output>` |
| `E1016` | `operator_instance_form`（新規） | 3 引数・2 引数の形で、どちらの側にも組み込みでない名前付き型がない／L・R・O がすべて同じ型／裸の型変数がもう一方の側に現れない（被覆の規則） | `an operator instance with three type arguments needs a record, union or extern type on one side` / `Add<Money, Money, Money> is a same-type instance; write instance Add<Money>` / `the type variable 'b on the right must also appear in the left type Money; write a concrete type such as Mul<Money, i64, Money>` |
| `E1016` | `orphan_instance`（A37 が登録。共有） | OEI の L・R（`Neg` は S）のどの頭もこのプログラムで宣言した型でない | `Mul<Tensor<f64>, f64, Tensor<f64>> needs a type declared in this program as its left or right operand type`。help は A37 と同じ `types from the standard library and built-in types such as tuples, arrays and references belong to the language; wrap the value in a record declared in this program, or call a function of the type's module` |
| `E1016` | `overlapping_instance`（既存） | 表の重なり（利用者と std の重なりは利用者の位置） | `overlapping instance for Sub<Tensor.View<'a>, Tensor.View<'a>>` |
| `E1027` | `instance_context_variable`（既存） | O か文脈が頭にない型変数を使う | `Sub output type may only mention type variables from its left and right types` |
| `E1027` | `undeclared_instance_constraint`（既存） | instance のメソッドの本体の OEI が、instance の文脈にない制約を要る | 今と同じ `instance method requires an undeclared constraint; add it to the instance context` |
| `E1043` | `ambiguous_instance_candidates`（[NC16](NC16-index-ref-and-zero-alloc-element-reads.md) と共有。先に入ったチケットが `src/diagnostic_kinds.rs` に登録する） | (a)・(c) で候補が 2 つ以上 | `ambiguous operator instance for Money * 2: Mul<Money, i32, Money> and Mul<Money, i64, Money> both apply; give the literal a suffix such as 2i64` |
| `E1003`・`E1005` | 既存 | (d)。OEI が当てはまらない。通常の `def` の本体の OEI が足した制約を、呼び出しが満たさない（呼び出しの場所で `E1005`） | 今と同じ。help に候補の OEI を足す |

位置は、ヘッダの誤りはクラス名、`E1043` と (d) は演算子の式（今の `E1005` と同じ）。

### 6. std のインスタンス（生成する）

`std/Tensor.tz` に、演算子ごとに 7 形、計 6 × 7 + 1 = 43 個を置く。文脈は `(<Op><'a>, Copy<'a>)`。下の表は `Sub` の例で、ほかの演算子は右の列のカーネルだけが変わる。

| 形 | ヘッダ | 本体 | 各要素 |
| --- | --- | --- | --- |
| `View`×`View` | `Sub<View<'a>, View<'a>, Tensor<'a>>` | `fn sub left right = sub_or_trap left right` | `l_i - r_i` |
| `Tensor`×`View` | `Sub<Tensor<'a>, View<'a>, Tensor<'a>>` | `sub_or_trap (ref (view left)) right` | `l_i - r_i` |
| `View`×`Tensor` | `Sub<View<'a>, Tensor<'a>, Tensor<'a>>` | `sub_or_trap left (ref (view right))` | `l_i - r_i` |
| スカラー×`Tensor` | `Sub<'a, Tensor<'a>, Tensor<'a>>` | `rsub_scalar (deref left) (ref (view right))` | `s - x_i` |
| `Tensor`×スカラー | `Sub<Tensor<'a>, 'a, Tensor<'a>>` | `sub_scalar (deref right) (ref (view left))` | `x_i - s` |
| スカラー×`View` | `Sub<'a, View<'a>, Tensor<'a>>` | `rsub_scalar (deref left) right` | `s - x_i` |
| `View`×スカラー | `Sub<View<'a>, 'a, Tensor<'a>>` | `sub_scalar (deref right) left` | `x_i - s` |

| 演算子 | 2 つのテンソル（NC06 仕様 5） | `x op s`（NC06 仕様 2） | `s op x`（NC06 仕様 2） |
| --- | --- | --- | --- |
| `+` | `add_or_trap` | `add_scalar` | `radd_scalar` |
| `-` | `sub_or_trap` | `sub_scalar` | `rsub_scalar` |
| `*` | `mul_or_trap` | `mul_scalar` | `scale` |
| `/` | `div_or_trap` | `div_scalar` | `rdiv_scalar` |
| `%` | `rem_or_trap` | `rem_scalar` | `rrem_scalar` |
| `**` | `pow_or_trap` | `pow_scalar` | `rpow_scalar` |

- `Neg` は `instance (Neg<'a>, Copy<'a>) => Neg<View<'a>, Tensor<'a>> { fn neg value = neg value }`（NC06 の公開の単項 `neg`）。同じ型どうしの `Neg<Tensor<'a>>` と `Add<Tensor<'a>>` などは NC06 が置く。
- 7 形はどれも被覆の規則を満たす（裸の型変数 `'a` は、もう一方の側の `Tensor<'a>`・`View<'a>` に現れる）。
- 裸の型変数の位置（D2）: L か R が裸の型変数の 4 形（スカラー×`Tensor`、`Tensor`×スカラー、スカラー×`View`、`View`×スカラー。6 演算子で 24 個）は、OEI のヘッダと同じ変更で入れ、A37 のテスト `std_instances_with_open_dispatch_positions_are_listed`（`src/stdlib.rs`）の一覧に足す。利用者は NC19 の前に OEI を書けないので、先にある利用者のインスタンスとは重ならない。この後に、振り分けの位置に裸の型変数を持つ std の OEI を足すのは、承認とリリースノートの Compatibility を要する互換性のない変更とする（`Complex` の 8 個は D9 の (iii) で承認し、[NC21](NC21-complex-real-scalar-operators.md) が足す）。`View`・`Tensor` どうしの 3 形と `Neg<View<'a>, Tensor<'a>>` は裸の型変数を持たないので、一覧に入らない。
- 本体はカーネルを 1 回呼ぶだけ（引数の中の `view`・`deref`・`ref` は被演算子の受け渡し）。要素の計算は左右の順のまま（`x * s` は `mul_scalar`、`s * x` は `scale`。要素型の `Mul` が可換でなくても順序を保つ）。融合・FMA・再結合はない。`a * b + c` は所有の一時値を 2 つ作る。形が合わなければ `*_or_trap` が出力を確保する前にトラップする（`assertion failed`）。整数のゼロ除算は要素の演算子のトラップ。
- `Tensor` の被演算子は、NC06 の同じ型どうしの本体と同じく `view` で窓にする（shape の複製と `dense_strides` の 2 回の確保。`std/Tensor.tz` 207〜208 行）。この確保をなくすときは、NC06 の本体と OEI の本体を同じ変更で直し、演算子と関数版が同じ道を通る形を保つ（対象外）。
- 43 個は `scripts/gen-tensor-operators.mjs` が `std/Tensor.tz` の `// generated:tensor-operators` と `// /generated:tensor-operators` の間に書く（`--check` で差を報告する。出力は入力だけで決まり、2 回の実行で同じバイト列）。手で直さない。
- カーネルの名前と型は NC06 の仕様 2・5 と照合済み（2026-11-25 の統合）。どちらかで改名するときは NC06・NC19・NC20 を同じ変更で直す（S4）。

### 7. 例

edition 2028（NC17・NC18 の後。範囲の意味は [NC17](NC17-edition-2028-inclusive-range-family.md) の基準表のとおり）:

```tsuzuri
let a = Tensor.of_array [5] [10.0, 20.0, 30.0, 40.0, 50.0]
let d = ref a[1..] - ref a[..^1]        // NumPy: a[1:] - a[:-1]
Tensor.to_array d                        // [10.0, 10.0, 10.0, 10.0]
```

`ref a[1..]` は境界 (1, 5) の `[20.0, 30.0, 40.0, 50.0]`、`ref a[..^1]` は (0, 4) の `[10.0, 20.0, 30.0, 40.0]`。`ref`（優先順位 15）は `-`（11）より強く結合するので、2 つの窓の差として読まれる（`op-and-expressions.md` の優先順位の表）。`d` は所有の `Tensor<f64>`。

どの edition でも（NC16 の後）:

```tsuzuri
let d = ref a[[Slice.from 1]] - ref a[[Slice.until (-1)]]   // 同じ [10.0, 10.0, 10.0, 10.0]

let x = Tensor.of_array [3] [0.5, 1.5, 2.5]
let y = 2.0 * x + 1.0          // [2.0, 4.0, 6.0]。スカラー×Tensor の結果に Tensor×スカラー
let z = x - 1                  // [-0.5, 0.5, 1.5]。1 は f64 として読む（スカラーの y - 1 と同じ）
let p = x ** 2                 // [0.25, 2.25, 6.25]

let t = Tensor.init [2, 3] (\i -> i)                 // [[0, 1, 2], [3, 4, 5]]
let row = Tensor.init [3] (\i -> (i + 1) * 10)       // [10, 20, 30]
let s = t + Tensor.view (ref row)                    // [[10, 21, 32], [13, 24, 35]]（ブロードキャスト）
let n = -(Tensor.view (ref row))                     // [-10, -20, -30]
```

利用者の型:

```tsuzuri
record Money { cents: i64 }

instance Mul<Money, i64, Money> {
    fn mul money factor = Money { cents: money.cents * deref factor }
}

let price = Money { cents: 250 }
(price * 3).cents              // 750。3 は (a) で i64 に決まる
```

ここに `instance Mul<Money, i32, Money>` を足すと `price * 3` は `E1043`（`3i64` と書く）。`instance Mul<Money, 'b, Money>`（どの型でも掛けられる形）は、被覆の規則で `E1016`（`operator_instance_form`）になる。掛ける型を `i64`・`f64` のように並べて書くか、利用者の型を要素型で総称にする（`Mul<Vec3<'a>, 'a, Vec3<'a>>` の形）。利用者のモジュールの `instance Sub<Tensor.View<i64>> { … }` は、A37 の orphan 規則で `E1016`（`orphan_instance`）。

総称関数（D1 の (2)）:

```tsuzuri
def scale_all :: own 'a -> own Tensor<'a> -> Tensor<'a>
fn scale_all s t = s * t       // 型変数 'a を既知の型として表を引く（スカラー×Tensor）
```

`s * t` は `Mul<'a, Tensor<'a>, Tensor<'a>>` を選び、その文脈 `(Mul<'a>, Copy<'a>)` が `scale_all` の制約に加わる。`scale_all 2.0 x` は通り、`BigInt` で呼ぶと呼び出しの場所で `E1005`（`Copy<BigInt>` がない）。

### 8. できないまま残すもの

- 総称コードで OEI を抽象化すること（`Sub<'l, 'r, 'o> =>` の制約、`Sub.sub` のメソッド値、`let f = \u -> 2.0 * u` を後で `Tensor` に使うこと。`u` は `f64` に決まる）。関数版（`Tensor.scale` など）を値として使う。
- 被演算子の頭が分からないときの OEI（`let k = 2.0` で束縛した名前、`(1.0 / 3.0)`）。`let k: f64 = 2.0` と型を書くか、リテラルをその場に書く（D3）。
- 参照の被演算子（`ref v - ref w`、edition 2027 の読み取り引数。P17）は [A40](A40-reference-operands.md) が扱う（D9 の (ii)）。NC19 は参照の被演算子を同じ型どうしの演算子と同じく扱い、OEI を引かない。後に入る方が他方の経路に参照を外す処理を足す。A40 が先なら、NC19 が (a)〜(d) を参照を 1 段外した型で引く。NC19 が先なら、A40 が OEI の経路に広げる。
- 被覆されない OEI（`Mul<Money, 'b, Money>`・`Mul<'a, Money, Money>`。どの型でも掛けられる形）。掛ける型を具体的に並べるか、利用者の型を要素型で総称にする（D1 の (1)）。
- 同じ側の頭に std の型を書き、要素型を利用者の型にした OEI（`instance Mul<Tensor<Money>, Money, Tensor<Money>>`、`instance Mul<Money, Tensor<Money>, Tensor<Money>>`）。orphan 規則は満たすが、std のスカラーの形（`Mul<Tensor<'a>, 'a, Tensor<'a>>`・`Mul<'a, Tensor<'a>, Tensor<'a>>`）と `'a = Money` で重なる（`overlapping_instance`、利用者の位置）。要素型が利用者の型でない `instance Mul<Money, Tensor<f64>, Tensor<f64>>` は書ける（D2）。
- 要素ごとの比較の演算子・ドット付きの演算子（D8）、`Eq`・`Ord`・`Bits` の OEI。マスクの `&&&`・`|||`・`^^^` のテンソル版は却下した（D9 の (i)）。マスクは NC06 の `Tensor.logical_and` などで組み合わせる。
- `BigInt` と整数の OEI（D9 の (iii) で却下）。`big * n`（`n: i64`）は `big * BigInt.of_i64 n` と書く。
- 配列どうし・組み込みの型だけの組（P14）。`Matrix`・`MatrixView`・`Tensor.Mut` の被演算子（`Tensor.freeze` で `View` にする）。
- 要素型の暗黙の変換（`2.0 * x` で `x: Tensor<i32>` は `E1003`、`Tensor<f32> + Tensor<f64>` は `E1003`）。
- 複合代入 `+=` などは [NC20](NC20-compound-assignment-and-mutable-index-views.md)。

## 設計

### データ構造（`src/polymorph.rs`）

```rust
struct OperatorInstanceTemplate {
    class: usize,              // Add..Pow, Neg
    left: Type,
    right: Option<Type>,       // Neg は None
    output: Type,
    variables: Vec<String>,    // 署名 ref L -> ref R -> O の型変数
    constraints: Vec<Constraint>,
    target: OperatorTarget,    // Extension(function id) | SameType(同じ型どうしのインスタンスの番号)
    span: Span,
}
operator_instances: Vec<OperatorInstanceTemplate>,
operator_heads: BTreeMap<(usize, Option<HeadKey>, Option<HeadKey>), Vec<usize>>,
```

### 段ごとの変更

| 段 | ファイル | 関数 | 内容 |
| --- | --- | --- | --- |
| 2 | `src/parser.rs` | インスタンスの解析（911〜944 行付近） | `Add`〜`Pow` は 1 か 3、`Neg` は 1 か 2 の型引数を受け、最初を `ty`、残りを `extra_args` に入れる（`Index` と同じ） |
| 2 | `src/polymorph.rs` | `register_operator_instance`（新規。`register_index_instance` を手本）、`operator_instance_function`（新規。`index_instance_function` を手本にし region を結ばない）、同じ型どうしの登録（2128〜2330 行付近）、`instance_views`（1041 行付近） | 仕様 2 の検査（頭の規則と被覆の規則）、表への登録（同じ型どうしの算術インスタンスも (T, T) で入れる）、A37 の `dispatch_positions` への OEI の位置（3 引数の [0, 1]、`Neg` の [0]）の追加、重なり（利用者と std の重なりは A37 の段 7 のとおり利用者の位置）、知識グラフ・`api-diff` 向けに OEI を `extra_args` 付きで見せる |
| 2 | `src/diagnostic_kinds.rs` | 登録表 | `operator_instance_form`（`E1016`）、`E1043` の kind（NC16 が未登録なら）。`orphan_instance` は A37 が登録済み。`E1043` を登録するときは、`src/explain/codes/E1043.md`（一般の「インスタンスの候補が 2 つ以上」として書く。NC16 の仕様 2）と `src/explain.rs` の `CODE_PAGES` も足す（`tests/explain.rs::every_code_has_a_page`） |
| 3 | `src/polymorph.rs` | `resolve_operator`・`operator_literal_hint`・`operator_hint_is_soft`（新規） | 仕様 3 の (a)・(c)。`Inference::unifies` で試す。候補は宣言の順に試すが、結果は数で決める。飛ばすのは `Type::Infer` の側だけで、`Type::Variable` は既知の型として扱う |
| 3 | `src/check.rs` | `binary_expression`（14209〜14290 行付近）、`binary_type`（17001 行付近）、単項（14476 行付近）、`finish_expression`（13742 行付近）、`call_expression`（14096 行付近） | ヒントの規則と柔らかいヒント、(b)〜(d)、呼び出しへの脱糖（`finish_binary` を通さない） |
| 4 | `scripts/gen-tensor-operators.mjs`（新規）、`std/Tensor.tz` | – | 43 インスタンスの生成 |
| 4 | `src/stdlib.rs` | `opt_in_modules_are_reached_only_through_their_names`（708 行付近）、`std_instances_with_open_dispatch_positions_are_listed`（A37 が足す） | A37 が `dispatch_positions` で直した読み込みのテストが OEI を通すことを確かめる（L・R のどちらかの頭が `Tensor`・`View`）。裸の型変数の一覧にスカラーの 24 個を足す |

### アルゴリズム（`resolve_operator`）

```text
resolve_operator(C, left, right):
  if C の表に OEI がない: return None
  if left か right が Type::Infer: return None   // Type::Variable は既知の型として続ける
  keys = { (C, h, k) | h ∈ {head(left), None}, k ∈ {head(right), None} }
  cands = []
  for idx in keys の項目（宣言の順）:
      新しい推論変数で項目を具体化し、(left * right) と (L' * R') を unifies で試す
      （試しの中でリテラルの制約が両立するかも見る）。通れば cands に足す
  match cands.len():
      0 => None（(d) へ）
      1 => 単一化を確定し、O・型引数・制約を返す
      _ => E1043（候補を列挙）
```

### 生成 IR の形

`v - w` は `@tz.fn.Tensor.$instance.<n>.sub.…` への呼び出し 1 つ（2 つの被演算子への ptr）になり、その中で `sub_or_trap` を呼ぶ。`-O3` では本体がインライン化され、関数版 `Tensor.sub` と同じ要素ループになる。`fmuladd`・`llvm.fma`・`fast`・`contract`・`reassoc` は出ない（`tests/features.mjs` の `tensor` スイートの `inspect` と同じ検査）。`tsuzuri build --emit llvm` は Clang に渡す前の IR なので、ベクトル化の確認は `clang -x ir -O3 -S -emit-llvm` の後で行う（NC06 の設計の確認ルール）。

## 前提とする他チケットのインターフェース

- [NC06](NC06-tensor-ufuncs-and-elementwise.md): 非公開の `add_or_trap`〜`pow_or_trap :: (<Op><'a>, Copy<'a>) => ref View<'a> -> ref View<'a> -> Tensor<'a>`（形が合わなければトラップ、ブロードキャストあり。公開の関数と同じ要素ループ）、公開のスカラー版 `add_scalar`・`radd_scalar`・`sub_scalar`・`rsub_scalar`・`mul_scalar`・`scale`・`div_scalar`・`rdiv_scalar`・`rem_scalar`・`rrem_scalar`・`pow_scalar`・`rpow_scalar :: own 'a -> ref View<'a> -> Tensor<'a>`、単項の `neg :: ref View<'a> -> Tensor<'a>`、同じ型どうしの `Add<Tensor<'a>>`〜`Pow<Tensor<'a>>`・`Neg<Tensor<'a>>`。
- 弱い依存 [NC16](NC16-index-ref-and-zero-alloc-element-reads.md): `ref t[[…]]` が `Tensor.View<'a>` になること、`E1043` の登録。
- 弱い依存 [NC18](NC18-multi-axis-bracket-indexing.md)・[NC17](NC17-edition-2028-inclusive-range-family.md): edition 2028 の `ref a[1..]`・`ref a[..^1]` が `Tensor.View<'a>` を返すこと。
- [NC03](_completed/NC03-borrowed-arithmetic-operators.md) D1-A（完了済み）: 組み込みでない型の算術インスタンスの借用の契約と `operator_call`。
- [A37](A37-index-base-place-borrow.md)（手順 5 と段 7。着手条件）: `validate_orphan_instance` と `dispatch_positions`（NC19 は OEI の位置を足す）、kind `orphan_instance`（A37 が登録）、テスト `std_instances_with_open_dispatch_positions_are_listed`（NC19 はスカラーの 24 個を足す）、`opt_in_modules_are_reached_only_through_their_names` が `dispatch_positions` のすべての位置を見ること、利用者と std のインスタンスの重なりを利用者の位置で報告すること、docs/stability.md の既知の例外「std への instance の追加」の項。
- 順序だけ [A40](A40-reference-operands.md): 参照の被演算子。どちらが先でもよく、後に入る方が他方の経路に参照を外す処理を足す（仕様 8）。

## 他チケットへ提供するインターフェース

- 3 引数・2 引数の算術インスタンスのヘッダと 1 つの演算子表（`OperatorInstanceTemplate`・`resolve_operator`）。
- `operator_literal_hint`（リテラルの型を表から決める）と柔らかいヒント。[NC20](NC20-compound-assignment-and-mutable-index-views.md) の `x += 1.0` のリテラルの扱いが同じ規則を使える。
- A37 の `dispatch_positions` の OEI の行（3 引数の [0, 1]、`Neg` の [0]）。
- [NC21](NC21-complex-real-scalar-operators.md) へ: OEI のヘッダと演算子表、仕様 3 の 1 のヒントの規則（柔らかいヒントを `if`・`match`・ブロックの結果の式に届けること、(ii) のスカラー・型変数のヒント）。NC21 は `std/Complex.tz` に 8 個の OEI を置き、裸の型変数の一覧に足す。`Complex` が OEI の頭になると `Complex<f64>` のヒントも柔らかくなるので、NC21 はこの規則を着手条件にする。
- [A40](A40-reference-operands.md) へ: 解決の (a)〜(d)。A40 は参照を 1 段外した型でこれを引く。
- std の 43 インスタンス（NC20 の `+=` は使わない。`+=` は `AddAssign` と NC06 の `*_in_place`）。

## 実装手順

1. **変更**: なし。**内容**: `rg -n 'binary_class|borrowed_arithmetic_class|"Add" \| "Sub"|extra_args' src/` で関わる箇所を控え、ベースラインの IR を `target/scratch/nc19/base/` に保存する（`tests/borrowed_arithmetic.rs` の各ソース、`tests/fixtures/polymorphism`・`tests/fixtures/typeclasses`、SIMD と文字列連結の fixture、`BigInt`・`Complex` の演算子、NC06 の `Tensor + Tensor` のテスト、P24 の `if`・`match`・ブロックの被演算子の形（NC06 の後なので、利用者のインスタンスなしで `Tensor` の版が受理される。`V` の版も）、P13 と P25。それぞれ `Tensor` を名指す 1 行を足した版も）。**確認**: 一覧と保存したファイルの数を記録する。
2. **変更**: `src/parser.rs`、`src/polymorph.rs`、`src/diagnostic_kinds.rs`（`E1043` を NC19 が登録するときは `src/explain.rs`・`src/explain/codes/E1043.md` も）。**内容**: 仕様 1・2（ヘッダ、頭の規則と被覆の規則、表、A37 の `dispatch_positions` への OEI の位置の追加、重なり、`instance_views`）。解決はまだ足さない。**確認**: T0、`cargo test --locked --test operator_extension`（登録のテスト）、`cargo test --locked --test diagnostic_kinds --test explain`、`cargo test --locked --test polymorphism --test borrowed_arithmetic --test index_class --test visibility --test pure_functions --test generic_records --test type_aliases`、段 1 の IR との差がないこと。
3. **変更**: `src/check.rs`、`src/polymorph.rs`。**内容**: 仕様 3・4（ヒントの規則、柔らかいヒント、(a)〜(d)、単項、呼び出しへの脱糖、`E1043`）。std のインスタンスはまだない（利用者の型 `Money` と、テストの中で宣言した `Tensor` 風のレコードで確かめる）。**確認**: T0、`cargo test --locked --test operator_extension`（解決・ヒント・借用のテスト）、段 2 と同じ既存のテスト、段 1 の IR との差がないこと（S1）、T1。
4. **変更**: `scripts/gen-tensor-operators.mjs`、`std/Tensor.tz`、`src/stdlib.rs`。**内容**: 仕様 6 の生成。A37 が直した読み込みのテストが OEI を通すことを確かめ、裸の型変数の一覧にスカラーの 24 個を足す。**確認**: `node scripts/gen-tensor-operators.mjs --check`、`cargo test --locked --test operator_extension --test tensor --test tensor_ops --test std_argument_order --test std_examples --test effects_alloc_conservative`、`cargo test --locked --lib opt_in_modules_are_reached_only_through_their_names`、`cargo test --locked --lib std_instances_with_open_dispatch_positions_are_listed`、`cargo test --locked --lib honors_the_exact_specialization_limit`（それぞれ `running 1 test` を確かめる）、`target/debug/tsuzuri test --doc std`、T1。
5. **変更**: `tests/tensor-operator-cases.mjs`、`tests/features.mjs`。**内容**: E2E（native/WASM × `-O0`/`-O3`、`live == 0`）、確保の回数、`-O3` 後の IR の確認。**確認**: `cargo build --locked && node scripts/slot.mjs -- node tests/features.mjs target/debug/tsuzuri tensor_operators`、`node tests/features.mjs target/debug/tsuzuri tensor`、確保と IR の確認の結果を PR に書く。
6. **変更**: 文書（下の「ドキュメント」）、`spec/rules/TYP.md`、`tests/conformance/TYP/`。**内容**: 文書と規則の更新。**確認**: `node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs` と `--check`、`node scripts/check-cli-docs.mjs`、`node scripts/check-spec.mjs --require-markers`、`node scripts/conformance.mjs target/debug/tsuzuri --rule <足した ID>`、T2。

## テスト計画

- **Rust テスト（`tests/operator_extension.rs`、新規。`tests/borrowed_arithmetic.rs` の `rejects`・`emits` と同じ形）**:
  - 宣言: 3 引数（`Add`〜`Pow`）と 2 引数（`Neg`）の受理、ほかの数と `Eq<A, B, C>` は `E0002`、両側が組み込みの型と L・R・O が同じ型は `E1016`（`operator_instance_form`）、O か文脈の型変数が頭にないと `E1027`、OEI どうしと OEI と同じ型どうしの重なりは `E1016`（`overlapping_instance`）、宣言の順を入れ替えても結果が同じ。
  - 被覆の規則と名前付き型: `instance Mul<Money, 'b, Money>` と `instance Mul<'a, Money, Money>` は `E1016`（`operator_instance_form`。メッセージを確かめる）。`instance Mul<Vec3<'a>, 'a, Vec3<'a>>` は受理。別々の利用者のモジュールの `instance Mul<'a, A<'a>, A<'a>>` と `instance Mul<B<'b>, 'b, B<'b>>` は重ならない。`extern type copy Buf` の `instance Mul<Buf, f64, Buf>` は受理。
  - orphan（規則は A37。1 引数の形の P8 は A37 のテストで `orphan_instance` 済みなので、ここでは足さない）: `instance Mul<Tensor<f64>, f64, Tensor<f64>>`・`instance Neg<Tensor.View<f64>, Tensor<f64>>`・`instance Mul<i64, Tensor<f64>, Tensor<f64>>` は `E1016`（`orphan_instance`）。`instance Mul<i64, f64, f64>` は `operator_instance_form`（検査の順）。`Money` を宣言したモジュールの `instance Mul<Money, Tensor<f64>, Tensor<f64>>`・`instance Mul<f64, Money, Money>` と、`Money` と別の利用者のモジュールにある `instance Mul<Money, Tensor<f64>, Tensor<f64>>` は受理。利用者の `Eq<Maybe<'a>>`（P18）は受理のまま。
  - std との重なり（`Tensor` を名指したプログラム）: `instance Mul<Money, Tensor<Money>, Tensor<Money>>` と `instance Mul<Tensor<Money>, Money, Tensor<Money>>` は `E1016`（`overlapping_instance`）で、位置が利用者のインスタンスのクラス名、related が std のインスタンス。
  - 解決: (a) `2.0 * x`・`x - 1`・`-1.0 * x`・`x * -1.0`・`x ** 2`・`1 - x`、`2.0 * x`（`x: Tensor<i32>`）は `E1003`、`Money` の `i32`・`i64` の 2 つで `price * 3` は `E1043`（宣言を入れ替えても `E1043`）。(b) `t + u` は同じ型どうしの道（IR が NC06 のテストと同じ）。(c) 同じテンソルの 2 つの窓の差（region が違う）、`t + v`、`v + t`、`Tensor<f32> + Tensor<f64>` は `E1003`。柔らかいヒント: `t + Tensor.view (ref t)`（P6）、`let y: Tensor<f64> = Tensor.view (ref t) - w`、`t + Tensor.full 2 [3]`（`t: Tensor<f64>`。`2` が `f64`）。(ii) のスカラー・型変数のヒント（P23）: `let k: f64 = 2.0` の `k * x` と `let y: Tensor<f64> = k * x`、`let k: i64 = 2` の `k * v` が (c) でスカラー×`Tensor`・スカラー×`View` を引く。`if`・`match`・ブロックの被演算子（P24）: `t + (if b then Tensor.full 2 [2] else Tensor.full 3 [2])`・`t + { Tensor.of_array [2] [1, 2] }`・`t * (match k with | 3 -> Tensor.full 2 [2] | _ -> Tensor.full 3 [2])`（`t: Tensor<f64>`）が同じ型どうしの道のまま通り、リテラルが `f64` で、IR が段 1 と同じ。`t + (if b then Tensor.view (ref u) else Tensor.view (ref w))` は枝ごとに柔らかいヒントを捨てて (c) の `Tensor`×`View` を引く。レコードの構築の被演算子 `v * V { x: 2 }`（P24 の `V`）も同じ型どうしの道のまま。手順 3（std のインスタンスの前）では、同じ形を P24 の利用者の型 `V<'a>`（同じ型どうしの `Mul<V<f64>>` と OEI `Mul<f64, V<f64>, V<f64>>`）で確かめる: `v * (if b then V { x: 2 } else V { x: 3 })` が同じ型どうしの道で `3`、`let k: f64 = 2.0` の `k * v` が OEI。(d) `let f = \u -> 2.0 * u` で `u` が `f64`、`Sub.sub v w` と `Sub<'a> =>` の総称関数に `View` を渡すと `E1005`、`ref v - ref w` は今と同じ診断（A40 が先に入っていれば A40 の規則で受理。後に入る方がこのテストを合わせる）、`let k = 2.0; x * k` は `E1003`（help を確かめる）。単項: `-v`、`-t`。
  - 総称関数の型変数: `def scale_all :: own 'a -> own Tensor<'a> -> Tensor<'a>`・`fn scale_all s t = s * t`（`t` への型変数のヒント `'a` は (ii) で柔らかい。P23）と、総称関数の本体の `t * 2`（`t: Tensor<'a>`）が OEI を引く。`scale_all` を `f64` で呼ぶと通り、`BigInt` で呼ぶと呼び出しの場所で `E1005`（`Copy`）。instance のメソッドの本体で OEI を使い、文脈に要るクラスがなければ `E1027`（`undeclared_instance_constraint`）。
  - 借用: 借用を持つ要素型（同じモジュールで `Mul` を持つ region 付きの利用者のレコード）の `Tensor` で、スカラー×`Tensor` の結果を生かしたまま土台を動かすと `E1014`（P11 と同じ形）。OEI の本体が被演算子の場所を借りた値を返すと使う場所で `E1013`（P12 と同じ形）。被演算子の一時値の解放の順序（左、右）を `Drop` の記録で確かめる。
  - 変えない: スカラー・SIMD・`string`・`BigInt`・`Complex`・利用者の同じ型どうしの演算子、`Add.add` のメソッド値と部分適用、P13 の `k * ident 2` が `3`、P25 の `k * (if b then 1 else 2)` が `2`（どちらも `Tensor` を名指した版と、利用者の OEI `Mul<Money, i64, Money>` もある版で。IR が段 1 と同じ）。`Tensor` を名指し、利用者の `Money` の OEI もあるプログラムで、`big * 2`・`3 * big` が `BigInt` のままで IR が段 1 と同じ（P22）。P19〜P21 の結果も変わらない。IR が決定的で、宣言が重複しない。
  - `@checked (t * 1000)`（`t: Tensor<i32>`）が例外を出さずに折り返す（文書どおり）。
  - std: 43 個がそろい、`E1016` にならない（7 形はどれも被覆の規則を満たす）。各本体が 1 回の呼び出しで、呼び先が仕様 6 の表と一致する（`tsuzuri::parser::parse` で `std/Tensor.tz` を読む）。`node scripts/gen-tensor-operators.mjs --check` を実行して通る。`std_instances_with_open_dispatch_positions_are_listed` の一覧がスカラーの 24 個を含み、ほかの 19 個を含まない。
- **適合性テスト**: `tests/conformance/TYP/` に OEI の宣言（頭の規則と被覆の規則）と解決の規則を 1 つずつ（`spec/rules/TYP.md` に足した ID。番号は実装の時点の次の空き）。orphan 規則の規則と適合性テストは A37 が足し、NC19 は OEI の位置の例をその適合性テストに足す。
- **E2E（`tests/features.mjs` の新しいスイート `tensor_operators`、`tests/tensor-operator-cases.mjs`）**: 仕様 7 の例、43 形のそれぞれ、ブロードキャスト（`[2, 3]` と `[3]`、`[2, 1]` と `[1, 3]`、0 次元）、負のストライドの窓（`Slice.rev()`）、空のテンソル、非可換の演算（`-`・`/`・`%`・`**` の左右）、整数の `/` と `%` の符号、形の不一致のトラップと整数のゼロ除算のトラップ（`trapKind: "assertion failed"` と算術のトラップ）。期待値は JavaScript で独立に計算する。native/WASM × `-O0`/`-O3` でビット一致、`live == 0`。`inspect` で `fmuladd|llvm\.fma|\bfast\b|\bcontract\b|\breassoc\b` が出ないこと。
- **確保**: `Alloc.snapshot`・`Alloc.since` で、同じ形の `v - w` と `Tensor.sub (ref v) (ref w)` の確保の回数が同じこと、`t + v` は `view` の 2 回だけ多いこと（NC06 の `t + u` と同じ数え方）。
- **既存テストへの影響**: `src/stdlib.rs` の `opt_in_modules_are_reached_only_through_their_names` は A37 が `dispatch_positions` で直したものがそのまま通ることを確かめ、`std_instances_with_open_dispatch_positions_are_listed` の一覧に 24 個を足す。std の知識グラフ（`src/kg/std_kg.rs`）にインスタンスのノードが増えるので、件数を固定したテストがあれば更新する。ほかの既存のテストは無修正で通る。
- **性能**: `v - w` と `Tensor.sub` の `-O3` 後の IR が同じ要素ループであることを確認して PR に書く。速度のしきい値は CI に入れない。

## ドキュメント

- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`: 「組み込み型クラス」の表の算術の行（127 行付近）に 3 引数・2 引数の形を足し、新しい節「演算子の拡張インスタンス」（宣言の形、被覆の規則、解決の順序、総称関数の中での働き、1 つの演算子表、制約・メソッド値にならないこと）を書く。orphan 規則は A37 が書く段落へリンクし、OEI の振り分けの位置（L・R、`Neg` は S）をその段落の表に足す。92 行付近の重なりの説明に表の重なりを足す。
- `_tsuzuri/language-reference/types-and-type-inference/type-inference.md`: 「数値リテラルの双方向推論と既定型」に、OEI の相手の要素型をリテラルが取ること、`let k = 2.0` は取らないこと、`\u -> 2.0 * u` の推論の順序。
- `_tsuzuri/language-reference/values-and-functions/op-and-expressions.md`: 算術演算子の節に、型の違う被演算子（`Tensor` とスカラーなど）が OEI で書けること、`@checked` が呼び出し先の要素を検査しないこと（95 行付近）、整数の `/`・`%` の意味は要素でも同じで NumPy と違うこと。
- `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`: 演算子の節（43 形の表、例、確保は結果 1 つと `Tensor` の被演算子の `view`、形の不一致はトラップで関数版は `Result`）、「計算量」、「できないこと」（287 行付近の未実装の記述を直す。NumPy の `m1 & m2` に当たる演算子はなく `Tensor.logical_and` などを使うこと。D9 の (i)）、「借用規則と診断」。NC07 が `numpy-migration.md` を作っていれば、整数の `/`・`%`、`x ** 2`、`a[1:] - a[:-1]` の行を足す。
- `docs/language.md`: 「多相関数と型クラス」（730 行付近）と「演算子」（4432 行付近）と「Tensor」（3527 行付近）。
- `docs/architecture.md`: `src/polymorph.rs` の行（53 行付近）に演算子表と索引を足す。
- `docs/stability.md`: A37 が足した既知の例外「std への instance の追加」の裸の型変数の一覧の例に、スカラーの形（`Mul<'a, Tensor<'a>, Tensor<'a>>`）を足す。別の項は作らない（D10）。
- `spec/rules/TYP.md`: OEI の宣言と解決の規則（G33 の手順。番号は実装の時点の次の空き）。orphan 規則の規則は A37 が足し、NC19 はその本文に OEI の振り分けの位置を足す。
- 診断の kind の生成ブロックは `node scripts/gen-docs.mjs` で作り直す。`README.md` の特徴の一覧が演算子に触れていれば直す。

## 受け入れ条件

- [ ] 仕様 1・2 の宣言の形と検査、仕様 3 の解決の順序、仕様 4 の下げ方、仕様 5 の診断が実装され、`tests/operator_extension.rs` が通る。
- [ ] 現状のプローブのうち、P1 のヘッダと P2〜P7 の式が受理され仕様どおりの値になる（P2 の `a - b` は `Tensor<i64>`、P3 の `1` は `i64`、P4 の `2.0` は `f64`。P15 は NC16 の後に受理）。P8・P11 の利用者のインスタンスが A37 のとおり `E1016`（`orphan_instance`）のままで、P11 と同じ借用の振る舞いを利用者の region 付きの型で確かめた。P9（`Money` には同じ型どうしの `Mul` しかない）・P10（`Index` は NC16 の範囲）・P12〜P14・P17〜P22 は変わらない（P17 は A40 が先なら A40 の規則で受理）。P23 の式が受理され（`k * x`・`k * v`・`scale_all` は OEI）、P24・P25 の形が段 1 と同じ型と IR で受理される。NC19 の (a)・(c) は宣言の順に依存しない。
- [ ] NC19 は入る時点で受理しているプログラムを拒否しない（`tests/` の受理のテストがすべて無修正で通る）。
- [ ] 被覆の規則（`Mul<Money, 'b, Money>` が `E1016`、`Mul<Vec3<'a>, 'a, Vec3<'a>>` が受理）、総称関数の型変数（`scale_all` と `t * 2` が OEI を引き、制約が呼び出し側で検査される）、リテラルの型（`integer`・`float` が受け付ける型。`big * 2` が変わらない）、`extern type` の OEI の受理を、テストで確かめた。
- [ ] std の 43 インスタンスが生成され（`--check` が通る）、互いと NC06 のインスタンスに重ならず、本体がカーネルの 1 回の呼び出し。スカラーの 24 個が `std_instances_with_open_dispatch_positions_are_listed` の一覧に入った。
- [ ] 仕様 7 の例が native/WASM × `-O0`/`-O3` で期待値を返し、関数版とビット単位で同じ。`live == 0`。
- [ ] 段 1 のベースラインと比べて、今受理しているプログラムの IR が変わらない（`Tensor` を名指す版を含む）。
- [ ] 確保の回数と `-O3` 後の IR を確かめ、PR に書いた。
- [ ] 文書と規則を更新し、`node scripts/check-docs.mjs <変更したページ>`、`node scripts/gen-docs.mjs --check`、`node scripts/check-spec.mjs`、T2 が通る。

## 落とし穴

- `binary_type` は先に `same` で両辺を単一化する（17012 行）。OEI の判定はその前に置く。さらにヒントが被演算子の検査の中で `E1003` を出す（P5・P6。`finish_expression` 13742 行付近と `call_expression` 14096 行付近）ので、`binary_type` だけを直しても `t + v` は通らない。
- 柔らかいヒントを入れ子の式へ漏らさない（角括弧の中の `stop_at_slice_dotdot` が入れ子に漏れた不具合と同じ種類。NC17）。被演算子のいちばん外側の式（`if`・`match`・ブロックではその結果の式）だけに効かせ、テストで `t + f (g 2)` のような入れ子を確かめる。
- 逆に、`if`・`match`・ブロックからヒントを取り上げない。今はヒントが枝の中のリテラルの型を決めている（P24・P25）。取り上げると、NC06 の後に受理している `t + (if b then Tensor.full 2 [2] else Tensor.full 3 [2])` と、`Tensor` を名指すプログラムの `k * (if b then 1 else 2)` が `E1003` になる。同じ理由で、ヒントを捨ててよいのは単一化の試しが失敗した場所だけ。スカラーのヒントを試しなしに捨てると P13 の `k * ident 2` が `E1003` になる。
- 被演算子自身の頭が OEI の L・R に現れるときのスカラー・型変数のヒント（仕様 3 の 1 の (ii)）を忘れない。忘れると、`let k: f64 = 2.0` の `k * x` と、仕様 7 の `scale_all` の `s * t` が、`x`・`t` への兄弟のヒントで `E1003` になり、(c) に届かない（P23）。
- region は `Type` に入らないので、別々の region の窓どうしは (b) で「同じ型」になる。(b) は「同じ型どうしのインスタンスの頭が合う」まで見ないと (c) へ進めない。
- `resolve_index` の「先頭を選ぶ」（2832 行）を写さない。候補が 2 つ以上なら `E1043`。試しは `Inference::unifies` で巻き戻し、`Inference` を複製しない。
- std の 7 形が重ならないのは出現検査による（仕様 2）。手で 1 形だけ型変数の置き方を変えると重なりうるので、生成器だけで書く。
- HEAD の `src/stdlib.rs` の持ち主の検査は `instance.ty` だけを見る。A37 の手順 5 が `dispatch_positions` のすべての位置を見るように直すので、NC19 は `dispatch_positions` に OEI の位置を足せばよい。足し忘れると、L が型変数の OEI で失敗する。
- 総称関数の本体で OEI を使うと、インスタンスの文脈の制約（`Mul<'a>`・`Copy<'a>`）が、今の演算子の制約と同じくその関数の制約に加わる。通常の `def` では呼び出し側で検査され、誤りは呼び出しの場所の `E1005` に出る（P19）。instance のメソッドの本体では、文脈になければ `E1027`（`undeclared_instance_constraint`）。
- (a)・(c) の「頭が分からない」の判定に `is_unknown`（`src/polymorph.rs` 256 行）を使わない。`Type::Variable` も未知とみなすので、総称関数の本体の `t * s`（`t: Tensor<'a>`、`s: 'a`）が `E1003` になる。
- 被覆の規則は「どの型でも掛けられる」利用者の OEI を拒否する。help は、具体的な型を並べる書き方と、要素型で総称にする書き方を示す。リテラルだけの式に具体的な候補が 2 つあれば `E1043` になることも help に書く。
- `Tensor.of_array [5] [10, 20, 30, 40, 50]` の要素は `i32`（リテラルの既定）。`2.0 *` と組むと `E1003` になる。テストでは要素型を明示する。
- 整数のテンソルの `t / 2` は 0 方向の切り捨て、`t % 2` は被除数の符号（NumPy の真の除算・床剰余と違う）。値は誤りなく変わるので、文書と E2E で明示する。
- `@checked (t * 1000)` はテンソルの要素を検査しない。スカラーと同じ字面なので読み手が誤解しやすい。文書に書く（新しい警告は足さない。D7）。
- `tsuzuri build --emit llvm` は Clang の前の IR。インライン化とベクトル化は Clang `-O3` の後で見る。

## 対象外

- 一般の利用者向けの多引数クラス、関連型、OEI を制約・メソッド値として使うこと。
- `Eq`・`Ord`・`Bits`・`&&`・`||` の OEI、要素ごとの比較の演算子、ドット付きの比較演算子（`a.<b` は `..<`・`<..` と 1 文字違い）。
- 配列・組み込みの型だけの組、`Matrix`・`MatrixView`・`Tensor.Mut` の演算子（NC06 D2 の `Matrix` の `*` の方針を含む）。
- 要素型の暗黙の変換、演算の融合・FMA・再結合、演算子からの暗黙の並列・SIMD・GPU。
- `Tensor` の被演算子の `view` の確保をなくすこと（NC06 の同じ型どうしの本体と一緒に直す別の変更）。
- `@checked` でテンソルの要素を検査すること、それを知らせる警告。
- 複合代入（[NC20](NC20-compound-assignment-and-mutable-index-views.md)）。
- D9 の候補: (i) マスクの `&&&`・`|||`・`^^^` のテンソル版は却下（マスクは NC06 の `logical_*` で組み合わせる）。(ii) 参照の被演算子は [A40](A40-reference-operands.md)。(iii) `Complex` と実数のスカラーは [NC21](NC21-complex-real-scalar-operators.md)、`BigInt` と整数は却下。

## 決定事項

- **D1（承認済み（D-66 の委任で決定、2026-11-25）。[NC03](_completed/NC03-borrowed-arithmetic-operators.md) の D2 の改訂）**: 組み込みの算術クラスに限って、3 引数（`Neg` は 2 引数）のヘッダの演算子の拡張インスタンスを入れ、演算子の構文だけに効かせる（仕様 1・3・4）。NC03 D2 は「多引数クラス `Add<'l, 'r, 'o>` を入れず、スカラーとの演算はライブラリ関数で担い、型推論の明快さを保つ」とした。その理由はこの形でも保たれる: (1) クラスは型変数 1 つのままで、OEI は制約・メソッド値にならないので、総称コードの推論も `Monoids` も PR02 D10 も OEI を見ない。(2) OEI を引くのは演算子の式の 1 か所だけで、両辺の頭が分かったとき（リテラルは構文上のものだけ）に限り、今 `E1003`・`E1005` になる場所だけで働く。後回しにする制約を作らず、探索も既定型の選び直しもしない。(L, R) から O が一意に決まることを宣言時（`E1016`）と使う時（`E1043`）に保証する。(3) ライブラリ関数（`Tensor.scale`・`sub_scalar` など）は値として使える第一級の形として残り（D-59）、演算子は同じカーネルを呼ぶ別の綴りに過ぎない。D-65 の「一般の利用者定義の多引数クラスは採用しない」も保つ。
  承認のときに、次の 5 点を足した。
  - (1) 被覆の規則（仕様 2）: L・R の一方が裸の型変数なら、その型変数はもう一方の側に現れる。規則がないと、利用者の `Mul<Money, 'b, Money>` と std の `Mul<'a, Tensor<'a>, Tensor<'a>>` が (Money, Tensor<Money>) で単一化できる。`Tensor` は名指したときだけ読み込まれる（`src/stdlib.rs` 577〜586 行）ので、どこかのソースが `Tensor` と書いた途端に、離れた利用者のインスタンスが `E1016` になる。規則があれば、裸の型変数の側は出現検査（`src/polymorph.rs` 606〜617 行付近）によって相手の名前付き型の頭と重ならない。
  - (2) 総称関数の型変数（仕様 3）: (a)・(c) が頭の分からない側として飛ばすのは `Type::Infer` だけにする。`is_unknown`（`src/polymorph.rs` 256 行）は `Type::Variable` も未知とみなすので、そのままでは総称関数の中の `t * s` が `E1003` になる。型変数は自分自身とだけ単一化するので、既知の型として扱っても探索は増えない。制約は今の演算子と同じく関数の制約に加わる（P19）。
  - (3) リテラルの型（仕様 3 の (a)）: そのリテラルを期待型で検査する `integer`・`float` が受け付ける型そのもの。総称関数の本体の `x * 2` は今もリテラルを型変数として読む（`src/check.rs` 16519〜16530 行）ので、これを除くと `t * s` は通るのに `t * 2` が `E1003` になる。(BigInt, BigInt) を候補にするので、`big * 2` は今と同じく `BigInt` になる（P22）。
  - (4) 互換性（仕様 1）: NC19 は A37 の手順 5 の後に入る（D2）。P8 の破壊は A37 の一度きりの変更（リリースノートの Compatibility）に含まれる。NC19 は入る時点で受理しているプログラムを拒否しない（`docs/stability.md` 7 行は、同じ edition の中で受理していたプログラムを拒否することを許さない）。
  - (5) `extern type`（仕様 2）: 組み込みでない名前付き型に数える。利用者の `extern type` は今 1 引数の算術インスタンスを持てる（P20）ので、OEI だけが数えないと、同じ型が 1 引数の形では書けて 3 引数の形では書けなくなる。A37 の orphan 規則と NC20 の Target の規則も `extern type` を数える。
  採らなかった案: 関数だけ（P2〜P7 が `E1003`・`E1005` のままで、関数版は形の不一致で `Result` を返すので数式がそのまま読めない）。一般の多引数クラス（推論のすべてに関数従属か関連型を持ち込み、D-65・D-66 が採らない）。狭い形（`View`・`Tensor` の組だけでは `2.0 * x` を落とし、スカラーの包みは値ごとに確保し、std の型だけに許すと利用者の `Date - Date = Duration` を禁じる）。完了済みの NC03 の D2 は履歴として変えず、改訂は GUIDE D-66 とこの D1 に記録する。全 edition で同じ。状態: 承認済み（D-66 の委任で決定、2026-11-25）。
- **D2（承認済み（D-66 の委任で決定、2026-11-25））**: 演算子表は起票のとおり 1 つにする。クラスごとに 1 つの表に、OEI と、組み込みでない型の同じ型どうしのインスタンスを入れる。(L, R) の重なりは 1 回だけ検査し、O は比べない。比べる組は 1024 組の上限（`E1017`）に数える。orphan 規則は [A37](A37-index-base-place-borrow.md) の D4 と同じ 1 つの規則にする（仕様 2）。利用者のモジュール（依存パッケージを含む）の算術インスタンスでは、L か R（`Neg<S, O>` は S、1 引数の形は T）の少なくとも 1 つの頭が、このプログラムで宣言した型でなければならない。単位はプログラムで、L か R を宣言したモジュールである必要はない。std のモジュールにはコンパイラの規則を当てず、`src/stdlib.rs` の読み込みのテストが `dispatch_positions` で置き場所を確かめる。1 引数の形の規則は A37 が入れ、NC19 は `dispatch_positions` に OEI の位置（3 引数の [0, 1]、`Neg` の [0]）を足すだけにする。A37 の手順 5 を NC19 の着手条件にする。検査の順は `operator_instance_form`、`orphan_instance`、`overlapping_instance`。`Eq`・`Ord`・`Bits` とほかのクラスには当てない。std のスカラーの 24 個は OEI のヘッダと同じ変更で入るので先にある利用者のインスタンスと重ならず、NC19 が A37 の一覧のテストに足す。その後に振り分けの位置に裸の型変数を持つ std の OEI を足すのは、承認とリリースノートの Compatibility を要する。同じ側の頭に std の型を書き、要素型を利用者の型にした利用者の OEI は、規則を満たしても std の裸の型変数の形と重なる（仕様 8。`overlapping_instance` を利用者の位置で報告する）。
  理由: (1) 規則を 1 つにすると、利用者が覚える文も実装の関数も 1 つになる。クラスで変わるのは `dispatch_positions` の 1 行だけである。(2) std の追加から守るには、プログラム単位で足りる。std は利用者の型を名指せないので、利用者の型を L か R に持つインスタンスは std のインスタンスと重ならない。例外は裸の型変数の位置だけで、その追加は一覧のテストで止める。インスタンスはプログラム全体に効く（`E1022`）ので、置き場所を制限しても利用者のコードの整理を妨げるだけである。(3) 組み込みの型を外の型にするので、`operator_instance_form` と 1 引数の形の扱いがそろう。(4) A37 を着手条件にすれば、規則が 2 つに分かれる道がなくなり、NC19 は入る時点で受理しているプログラムを拒否しない。(5) 被覆の規則（D1）のもとでは、残る重なりは std の型を頭に書いた利用者の OEI に限られる。(6) 表が分かれると、OEI と同じ型どうしのインスタンスの重なりが見つからず、(L, R) から O が一意に決まる保証（`E1016`・`E1043`）が崩れる。
  採らなかった案: 起票時のモジュール単位で std にも当てる規則（A37 の `Index` と別の規則になり、利用者は `Money` と別のモジュールに OEI を置けず、std のモジュール単位の規則は `PAIR_TRIGGERS` と二重になる）。std の型だけを外にする規則（組み込みの型の演算子の意味を依存パッケージが別々に決めて衝突しうる）。表だけで orphan 規則なし（P8 のように利用者が std の型の演算子の意味を決められる）。パッケージ単位（`Drop`・`Format`・利用者のクラスと食い違うので、別に決める）。状態: 承認済み（D-66 の委任で決定、2026-11-25）。
- **D3（既定案）**: 解決の順序は仕様 3 の (a)〜(d)。リテラルは構文上のもの（`untyped_number`）だけで、`let` で束縛した名前や式は (a) の対象にしない（推論の探索を増やさない）。OEI が当てはまらなければ今の道の診断（`E1003`・`E1005`）をそのまま出し、2 つ以上は `E1043`。
- **D4（既定案）**: 柔らかいヒントにするのは、OEI の表の L・R・O に頭が現れる型（std では `Tensor`・`View`）のヒントと、ヒントか被演算子自身の型の頭が OEI の L・R に現れるときのヒント（スカラーのヒントと型変数のヒントを含む）だけにする（仕様 3 の 1 の (i)・(ii)）。柔らかいヒントは `if`・`match`・ブロックの結果の式にも届け、単一化できない場所でだけ捨てる。理由: ヒントを捨てると `t + Tensor.full 2 [3]` の `2` が `f64` に読めなくなり（P13 と同じ仕組み）、`if`・`match`・ブロックから取り上げると枝の中のリテラルの型が決まらない（P24・P25）。硬いままだと `t + Tensor.view (ref row)` が P6 の `E1003` になり、`k * x`（`let k: f64`）と `scale_all` の `s * t` が `x`・`t` へのスカラー・型変数のヒントで `E1003` になる（P23）。柔らかいヒントは単一化できるときは今どおり単一化し、捨てるのは今の道が `E1003` を報告する場所だけなので、今受理しているプログラムの型は変わらない。ヒントも被演算子の型の頭も OEI の L・R に現れないスカラーのヒントは硬いまま（P13）。
- **D5（既定案）**: OEI の呼び出しは普通の総称呼び出し（`BorrowOperand` の 2 引数、名前付きの region なし）に下げ、結果の借用を特別扱いしない（仕様 4）。「演算子の結果は借用を持たない」とは仮定しない。
- **D6（既定案）**: std は演算子ごとに 7 形と `Neg<View, Tensor>` の 43 個を生成する。`Pow` も 7 形すべて（NumPy の `x ** 2`・`2.0 ** x`・`x ** y`）。NC06 が同じ型どうしの `Pow<Tensor<'a>>`・`pow_or_trap`・`pow_scalar`・`rpow_scalar` を持つため。2026-11-25 の統合で、NC06 がこれらを持つこと（NC06 の D5・仕様 2・5）を確かめた。NC06 の実装でこれらを落とすと決まった場合は、`Tensor ** スカラー` と `View ** スカラー` だけにする。本体はカーネルを 1 回呼ぶだけ。
- **D7（既定案）**: `@checked` はテンソルの要素の演算を検査しない（呼び出し先は検査しないという今の規則のまま）。新しい警告は足さない（D-55 の予約の警告は `W1011`・`W1015` だけで、`W1011` は NS05 が使う予定）。文書に書く。
- **D8（既定案）**: 要素ごとの比較は NC06 D3 の関数のまま（D-20）。ドット付きの演算子は入れない。`Matrix`・`MatrixView`・`Tensor.Mut`・配列の OEI は足さない。
- **D9（Phase 2 の候補。3 つとも D-66 の委任で決定、2026-11-25）**:
  - (i) マスクを組み合わせる `&&&`・`|||`・`^^^` のテンソル版（`Bits` を組み込みでない型で借用の契約にすることと、`Tensor<bool>` の同じ型どうし・OEI）。状態: 却下（D-66 の委任で決定、2026-11-25）。NC19 の Phase 2 にも新しいチケットにもしない。マスクは NC06 の `Tensor.logical_and`・`logical_or`・`logical_xor`・`logical_not`（形の不一致は `Result.Error`）で組み合わせる。整数のテンソルのビット演算は NC06 の `bit_and`・`bit_or`・`bit_xor`・`bit_not`・`shl`・`shr` を使う。理由: スカラーの `bool` に `&&&` がない（P21。`Bits` の組み込みのインスタンスは整数だけ、`src/polymorph.rs` 7366 行）。`Tensor<bool> &&& Tensor<bool>` は要素の演算子を持たないテンソルの演算子になり、「要素の演算は要素型の演算子そのもの」に反する。組み込みでない型の `Bits` は値を消費する契約で（`borrowed_arithmetic_class` に `Bits` はない、`src/polymorph.rs` 942〜944 行）、借用に変えると既存の利用者の `Bits` インスタンスのメソッドの型が変わる。比較はスカラーの `bool` のまま（D-20）なので、NumPy の `(a > 0) & (b < 1)` は `&` を演算子にしても演算子だけでは書けない。`Bits` は orphan 規則の表（A37 の `dispatch_positions`）に足さない。再提案は、要素ごとの比較の演算子を入れる決定（D-20 の改訂）か、スカラーの `bool` に `&&&` を与える決定と一緒にだけ行う。
  - (ii) 参照の被演算子（`ref T`・`ref mut T`、edition 2027 の読み取り引数。P17）。状態: 承認済み（D-66 の委任で決定、2026-11-25）。NC19 の Phase 2 ではなく、新しいチケット [A40](A40-reference-operands.md) に分けた。同じ型どうしの演算子も変わるので、1 チケット 1 PR に従う。A40 は、被演算子が `ref T`・`ref mut T` で今の道が `E1003`・`E1005` を報告するときだけ、参照を 1 段外して今の解決（NC19 の後は (a)〜(d)）を行う。NC19 は A40 を先に入れるのを推奨するが、A40 に依存しない。後に入る方が他方の経路に参照を外す処理を足す（仕様 8）。
  - (iii) std のほかの型の OEI。状態: `Complex` と実数のスカラーは承認済み、`BigInt` と整数は却下（D-66 の委任で決定、2026-11-25）。`Complex` は新しいチケット [NC21](NC21-complex-real-scalar-operators.md) に分け、NC19 の後に `std/Complex.tz` へ `Add`・`Sub`・`Mul`・`Div` × 2 形（`Complex<'a>`×`'a` と `'a`×`Complex<'a>`）の 8 個を置く。8 個は被覆の規則を満たし、振り分けの位置に裸の型変数を持つ。この決定が D2 の求める承認になり、NC21 が 8 個を A37 の一覧のテストに足してリリースノートの Compatibility に書く。`BigInt` は採らない。今 `big * 2`・`3 * big` が動く（P22。整数リテラルが `BigInt` になる、`src/check.rs` 16515 行）。`Mul<BigInt, i64, BigInt>` を足すと、(a) が `big * 2` のリテラルの型を 2 つ（`BigInt` と `i64`）見つけて `E1043` にし、受理していたプログラムを拒否する。残る `big * n` は `i64` から `BigInt` への暗黙の変換で、`big * BigInt.of_i64 n` と書く。「変えてはいけない意味」の `BigInt`・`Complex` の演算子の型付けと IR は、NC19 の範囲としてそのまま（`Complex` の OEI は NC21 が足す）。
- **D10（承認済み（D-66 の委任で決定、2026-11-25））**: `docs/stability.md` の「既知の例外」の「std への instance の追加」は 1 項で、A37 の手順 5（orphan 規則と同じ PR）が書く。NC06・NC19・NC21 は別の項を書かず、その項に例を足すだけにする。NC19 は、裸の型変数の一覧の例にスカラーの形（`Mul<'a, Tensor<'a>, Tensor<'a>>`）を足す（ドキュメント）。理由: std へのインスタンスの追加が利用者を壊しうることは規則の有無にかかわらず事実で、`docs/stability.md`（119〜126 行）の既知の例外はモジュールの追加と union への case の追加だけである。規則の後に残る危険は、裸の型変数の位置、表にないクラス、`E1043` の 3 つで、規則と同じ PR で 1 項にまとめるのが正確である。起票時の「D2 が承認されたら NC19 が既知の例外を書く」は、規則の前と後で範囲の違う 2 つの文を作るので採らない。

## 改訂（2026-11-25。D-66 の委任による決定）

利用者は、承認の要る項目と決めていない問いを、最善の選択肢で決めることを委ねた（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)）。その決定に合わせて、このチケットを次のとおり改めた。

- **承認**: D1・D2・D10 を承認済みにし、D9 を (i) 却下、(ii) A40、(iii) NC21 と `BigInt` の却下にした。着手条件から承認の待ちを外した。
- **D1 の 5 点**: 被覆の規則（仕様 2・5）、総称関数の型変数を既知の型として扱う規則（仕様 3・4）、リテラルの型（仕様 3 の (a)）、互換性の文（仕様 1）、`extern type` を名前付きの型に数えること（仕様 2）を足した。
- **D2**: orphan 規則を A37 の D4 の 1 つの規則にした。NC19 は `dispatch_positions` に OEI の位置を足すだけで、A37 の手順 5 を着手条件にした。スカラーの 24 個を裸の型変数の一覧に足す（仕様 6）。利用者と std の重なりは利用者の位置で報告する。起票時の `instance Add<[i32]>` を受理のままにする案は、A37 で `orphan_instance` になるので消した。
- **メタ情報**: 依存に A37、順序の推奨に A40、後続に NC21 を足した。
- **現状**: P8・P11 に A37 の後の扱いを書き、P19〜P22 と、`is_unknown`・リテラルの型付け・`E1027`・`Type::Handle` の行を足した（HEAD `858a949c` の dev バイナリとソースで確認）。
- テスト計画・受け入れ条件・落とし穴・ドキュメント・設計の表を、これらに合わせた。
- **ヒントの規則（同日のレビューの修正）**: 仕様 3 の 1 に、(ii) 被演算子自身の型の頭が OEI の L・R に現れるときのスカラー・型変数のヒントも柔らかくすること（P23。`let k: f64 = 2.0` の `k * x` と仕様 7 の `scale_all` の `s * t` が (c) に届く）と、柔らかいヒントを `if`・`match`・ブロックの結果の式に届けて、単一化できない場所でだけ捨てること（P24・P25。起票時の「ヒントを中へ渡す形には渡さない」は、NC06 の後に受理している式を拒否するので改めた）を足した。D4・落とし穴・テスト計画・受け入れ条件・実装手順 1 を合わせた。NC21 はこの規則を着手条件にする。
