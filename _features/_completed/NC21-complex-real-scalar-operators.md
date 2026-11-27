# NC21: `Complex` と実数のスカラーの演算子（演算子の拡張インスタンス 8 個）

| 項目 | 内容 |
| --- | --- |
| ID | NC21 |
| 優先度 | P2（`c * 2.0`・`2.0 - c`・`c / s` のような複素数と実数の式を、演算子のまま書けるようにする。今は `E1003` で、`Complex.scale` か `Complex.from_real` を経由する） |
| 規模 | S（`std/Complex.tz` に 8 個のインスタンスを足し、一覧のテストとテスト・文書を直す。コンパイラの変更はない） |
| 依存 | [NC19](_completed/NC19-operator-extension-instances.md)（OEI の宣言の形、演算子表、解決 (a)〜(d)、`dispatch_positions` の OEI の位置を使う。NC19 は [A37](_completed/A37-index-base-place-borrow.md) の手順 5 の後に入るので、`std_instances_with_open_dispatch_positions_are_listed` もある）。土台の [NC01](_completed/NC01-complex-and-extended-math.md) の `std/Complex.tz` は完了済み |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20) の委任による決定。NC19 の D9 (iii) を分けた。HEAD `858a949c` で確認） |
| 承認 | 承認済み（D-66 の委任で決定、2026-11-25）: `Complex` と実数のスカラーの 8 個の OEI（D1）。8 個は振り分けの位置に裸の型変数 `'a` を持つ std のインスタンスで、NC19 の D2 が求める承認はこの決定で与えた。`BigInt` と整数の OEI は却下（D-66 の委任で決定、2026-11-25。D2）。ほかは既定案か決定 |
| 手本にする既存実装 | `std/Complex.tz` 174〜175 行の `Complex.scale`（`s * c` と同じ式）、354〜416 行の同じ型どうしのインスタンス（文脈の書き方）、NC19 が `std/Tensor.tz` に置く `Tensor` とスカラーの OEI（宣言の形）、`tests/borrowed_arithmetic.rs` 197〜245 行（native と WASM の `-O0`・`-O3` を 1 つの Rust テストで実行する形）、`tests/math.mjs` 345〜505 行（NC01 の native と WASM のビット比較。NaN は NaN かどうかだけを比べる） |
| 主な影響ファイル | `std/Complex.tz`、`src/stdlib.rs`（A37 のテスト `std_instances_with_open_dispatch_positions_are_listed` の一覧に 8 項目）、`tests/complex.rs`、`_tsuzuri/language-reference/built-in-types-and-modules/complex.md`、`docs/language.md`（「演算子」）、`docs/stability.md`（既知の例外「std への instance の追加」の例）、リリースノートの Compatibility（PR の説明に文案。リリースのときに `vsc/CHANGELOG.md`） |

## 実装者向け早見

- 範囲: `std/Complex.tz` に `Add`・`Sub`・`Mul`・`Div` × 2 形（`Complex<'a>`×`'a` と `'a`×`Complex<'a>`）の 8 個の OEI を置く。実数の被演算子は、虚部が +0.0 の複素数としてではなく、虚部を持たない数として扱う（C99 Annex G と同じ）（`s / c` を除く。`s / c` は `Complex.div (Complex.from_real s) c` で、実数を虚部 +0.0 の複素数に直して Smith の方法で割る。D4）。式は仕様 1 の表。`Pow`・`Rem` は足さない。8 個を A37 の一覧のテストに足し、リリースノートの Compatibility に書く。
- 最初に読む: ① 仕様 1 の表 ② `std/Complex.tz` 1〜27 行（レコード、`of`、`from_real`）、109〜175 行（`div_smith`・`div`・`neg`・`scale`）、336〜416 行（インスタンス） ③ NC19 の仕様 1〜4（OEI の宣言の形・演算子表・解決の順） ④ 決定事項 D3（直接の式）。
- 最初に書くテスト: `tests/complex.rs` に、8 形の値（`c * 2.0` が `2+4i` など）と、符号付きゼロと無限大の 3 つの例（仕様 2）のテストを書く。今はどれも `E1003` で失敗する。
- 触ってはいけない: 同じ型どうしの `Complex` の演算子とその IR、`Complex.scale`・`Complex.div` などの関数、`BigInt` の演算子（`big * 2` は今どおり `BigInt` のリテラル）、NC19 の宣言の検査と解決の規則、`Complex` を名指さないプログラム。
- 完了の見分け方: 8 形が受理されて仕様 1 の式どおりの値を出し、符号付きゼロと無限大の例が独立に計算した期待値とビット単位で一致し（native と WASM、`-O0`・`-O3`）、一覧のテストが 8 項目を含み、`c ** 2.0` と `big * n` が今と同じ診断のまま。

## 目的

複素数の式では、実数との四則をよく書く（NumPy・.NET・C++ の `std::complex`・Julia は `複素数 * 実数` を書ける。リポジトリの外の知識で、未検証）。今の Tsuzuri では `c * 2.0` が `E1003` になり、`Complex.scale 2.0 c` か `c * Complex.from_real 2.0` と書く。前者は実数倍しか書けない。後者は実数を複素数に直してから計算するので、演算が増え、無限大と符号付きゼロで結果が変わる（仕様 2）。

NC19 は、組み込みの算術クラスに 3 引数のヘッダ（OEI）を開く。このチケットは、その仕組みの上に `Complex` と実数のスカラーの 8 形を std だけで足す。`Complex` には今リテラルの注入がないので、受理するのは新しいプログラムだけである。ただし `Complex` が OEI の頭になると `Complex<f64>` のヒントが柔らかくなるので、今受理している `if`・`match`・ブロックの被演算子（C17）とスカラーのヒント（C18）の型が変わらないことは、NC19 の仕様 3 の 1 のヒントの規則に頼る（着手条件。守られなければ S7）。

## 着手条件と停止条件

### 着手条件

- [NC19](_completed/NC19-operator-extension-instances.md) が done であること（OEI の宣言の形・演算子表・解決 (a)〜(d)・`dispatch_positions` の OEI の位置）。NC19 は A37 の手順 5 の後なので、A37 のテスト `std_instances_with_open_dispatch_positions_are_listed` と、`opt_in_modules_are_reached_only_through_their_names` を `dispatch_positions` のすべての位置で判定する改修もある。
- NC19 の仕様 3 の 1（[NC19](_completed/NC19-operator-extension-instances.md) の「被演算子の検査とヒント」）が、次の 2 つの性質を持つこと。NC21 は `Complex` を OEI の頭にするので、この 2 つがないと今受理しているプログラムを拒否するか、スカラーが左の変数の形が書けない。
  - (i) NC19 の演算子のヒントは、今どおり `if`・`match`・ブロックの被演算子の中へ、その結果の式（各枝の値、ブロックの末尾）まで届く。柔らかいヒントは結果の式（値の式か呼び出し）ごとに試し、単一化できない結果の式でだけ捨てる。これらの形からヒントを取り上げない（C17）。
  - (ii) OEI を持つクラス C の被演算子のヒントの単一化の場所（`finish_expression`・`call_expression`）では、ヒントの種類（式の期待型か兄弟の型か。スカラーのヒントと型変数のヒントを含む）を問わず、ヒントの型の頭か、被演算子自身の型の頭（値の式の型、呼び出しの結果の型）が C の OEI の L か R に現れるなら、単一化を試し（`Inference::unifies`）、通るときだけ単一化する。通らなければヒントを捨てて (b)・(c) へ進む（C16）。
  - どちらも今の道が `E1003` を報告する場所でしか働かないので、受理しているプログラムは変わらない。NC19 がこの 2 つを持たずに done になっていたら、着手せずに NC19 へ報告する。
- 承認は済んでいる（D-66 の委任で決定、2026-11-25）。
- GUIDE §2.3 の基準コマンドが成功し、「現状」のプローブ C1〜C19 を基準の dev バイナリで確かめていること（NC19 の後なので、C1〜C5・C16 の診断の help は NC19 の (d) の文に変わっていてよい。コードと位置は同じはず）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- **S1** 8 個のどれかが NC19 の宣言の検査（頭の規則・被覆の規則・`overlapping_instance`）を通らない。`Complex` と `Tensor` を両方名指したプログラムで、std のインスタンスどうしが `E1016` になる。NC19 の規則を変えて通さない。
- **S2** NC21 の前に受理していたプログラムの型・IR・出力が変わる（同じ型どうしの `Complex` の演算子、`big * 2`・`3 * big`、スカラーの演算子、`Complex` を名指さないプログラム）。
- **S3** 要素型が決まった `Complex` との `c * 2.0`・`2.0 * c`・`c * 2`（`Complex<f64>`）が `E1043`（候補が 2 つ以上）になる。
- **S4** E2E のビットが、独立に計算した期待値と一致しない（NaN の符号とペイロードを除く）。期待値を合わせて直さない。FMA への縮約か再結合が起きていないかを生成 IR で確かめて報告する。
- **S5** NC19 の後で、利用者が書いた `Complex` を頭に持つ OEI がリポジトリの中（`tests/`・`examples/`・`_tsuzuri/`）にあり、8 個と重なる。
- **S6** GUIDE §13.1 の共通の条件。
- **S7** NC19 の後の dev バイナリに NC21 の 8 個を入れると、`c: Complex<f64>` の `c + (if b then Complex.of 1 2 else Complex.of 3 4)`・`c + { Complex.of 1 2 }`・`c * (match k with | 3 -> Complex.of 1 0 | _ -> Complex.of 0 1)`・`c + Complex { re: 1, im: 2 }` のどれかが `2+4i`・`2+4i`・`1+2i`・`2+4i` を出さないか、`let k: f64 = 2.0` の `k * (if b then 1 else 2)` が `2` を出さない（C17・C18）。作業を止めて NC19 へ報告する。この破壊を抱えたまま入れない。
- **S8** NC21 の 8 個を入れた dev バイナリで、`let k: f64 = 2.0` の `k * c` か、総称の `fn f c s = s * c` がまだ `E1003` になる（C16）。作業を止めて NC19 へ報告する。リテラルの形だけのスカラーが左の形を入れない。

## 現状（HEAD `858a949c` で確認）

### `std/Complex.tz`

- `record Complex<'a> { re: 'a, im: 'a }`（2 行。1 行の説明は「`Copy`、ヒープ確保なし」）。opt-in のモジュールで、`Complex` を名指したプログラムだけが読み込む（`src/stdlib.rs` 479〜484 行の `OptIn`）。
- `from_real :: (Float<'a>, Numeric<'a>, Copy<'a>) => own 'a -> Complex<'a>` は `Complex { re: re, im: 0.0 as 'a }`（18〜19 行）。虚部は +0.0。
- `div` は非公開の `div_smith`（109〜157 行。大きさを補正した Smith の方法）を呼ぶ（158〜159 行）。文脈は `(Float<'a>, Add<'a>, Sub<'a>, Mul<'a>, Div<'a>, Ord<'a>, Copy<'a>)`。
- `scale :: (Float<'a>, Mul<'a>, Copy<'a>) => own 'a -> Complex<'a> -> Complex<'a>` は `Complex { re: factor * z.re, im: factor * z.im }`（174〜175 行）。
- 同じ型どうしのインスタンス: `Add<Complex<'a>>`（354 行）、`Sub`（358 行）、`Mul`（362 行。素朴な式 `re: l.re * r.re - l.im * r.im, im: l.re * r.im + l.im * r.re`、363〜367 行）、`Div`（370 行。`div_smith` と同じ本体を写している）、`Neg`（414 行。文脈に `Neg<'a>`）。`Pow`・`Rem` のインスタンスはない。
- `Float` には上位クラスがない（組み込みのクラスの上位クラスは `Ord` の `Eq` だけ。`src/polymorph.rs` 1110〜1130 行）。そのため `-im` には `Neg<'a>` を文脈に書く。
- std のインスタンスの本体は、同じモジュールの非公開の関数を呼べる（`std/Tensor.tz:1678` の `Index` の本体が非公開の `slice_raw`（1593 行）を呼ぶ。行番号は、このチケットと一緒に入るこのブランチの `std/Tensor.tz` のもの。A40 の `std/Tensor.tz:1519` と同じ基準）。

### プローブ（検証済み。HEAD `858a949c` の dev バイナリ、macOS arm64 の native、マニフェストなし（edition 2027）、`-O0`）

`let c = Complex.of 1.0 2.0` を前置きにした。

| # | 形 | 結果 |
| --- | --- | --- |
| C1 | `c * 2.0` | `E1003`（`expected Complex<an undetermined type>, found f64`、位置は `2.0`） |
| C2 | `2.0 * c` | `E1003`（同じ文、位置は `2.0`） |
| C3 | `c ** 2.0`、`c % 2.0` | どちらも `E1003`（同じ文、位置は `2.0`）。`c % c` は `E1005`（`no instance for Rem<Complex<f64>>`） |
| C4 | `c + 2.0` | `E1003`（同じ文） |
| C5 | `c * 2` | `E1003`（`expected Complex<an undetermined type>, found i32`） |
| C6 | `let big = BigInt.of_i64 5` / `big * 2`・`3 * big` | 通る（`10 15`。`src/check.rs:16515` で整数リテラルが `BigInt` になる） |
| C7 | `let n: i64 = 2` / `big * n` | `E1003`（`expected BigInt, found i64`） |
| C8 | `let inf = 1.0 / 0.0` / `let c = Complex.of 1.0 inf` / `c * Complex.from_real 2.0`、`Complex.scale 2.0 c`、`Complex.of 1.0 (-0.0) + Complex.from_real 2.0`、`Complex.from_real 0.0 - Complex.of 1.0 0.0` | `nan+infi 2+infi 3+0i -1+0i`。実数を複素数に直すと、実部が `1 * 2 - inf * 0 = NaN` になり、虚部の -0.0 が +0.0 に変わる |
| C9 | `Complex.div (Complex.from_real 2.0) c` と `Complex.from_real 2.0 / c` | どちらも `0.4-0.8i`、`==` は `true` |
| C10 | `let c: Complex<f32> = Complex.of 1.0 2.0` / `c * 2.0` | `E1003`（`expected Complex<f32>, found f64`） |
| C11 | `let x: f32 = 2.0` / `c * x` | `E1003`（`expected Complex<an undetermined type>, found f32`） |
| C12 | `Complex.of 1.0 (-0.0)` の表示、`-(Complex.of 1.0 0.0)` の表示、`f64` の `-0.0`・無限大・NaN・`5e-324`・`1.0e308` の表示 | `1-0i`、`-1-0i`。`-0 inf -inf nan 5e-324 1e+308`（`-O3` も同じ）。`Display` は虚部の符号を `Math.signbit` で決める（345〜352 行） |
| C13 | `let t = Tensor.init [2] (\i -> Complex.of 1.0 0.0)` / `t * 2.0` | `E1003`（`expected Tensor<Complex<an undetermined type>>, found f64`） |
| C14 | `record Money { cents: i64 }` / `let c: Complex<Money> = Complex { re: Money { cents: 1 }, im: Money { cents: 2 } }` | 通る（`3`）。`Complex<Money>` は書ける型なので、利用者の OEI `Mul<Complex<Money>, Money, Complex<Money>>` も書ける |
| C15 | スカラーの `c.re * 2`（注釈のない `c`） | `E1003`（`expected an integer, found f64`）。`let x = 1.5` の `x * 2` は通る |
| C16 | `let c: Complex<f64> = Complex.of 1.0 2.0` / `let k: f64 = 2.0` / `k + c`・`k - c`・`k * c`・`k / c`、`k / Complex.of 1.0 1.0`、`def f :: (Float<'a>, Mul<'a>, Copy<'a>) => Complex<'a> -> 'a -> Complex<'a>` / `fn f c s = s * c`（`Div` の文脈で `s / c` も） | どれも `E1003`。`k op c` は `c` の位置で `expected f64, found Complex<f64>`（兄弟のヒント `f64` が硬い）。`k / Complex.of 1.0 1.0` は右の位置で `expected f64, found Complex<an undetermined type>`。総称の `s * c`・`s / c` は `s` の位置で `expected Complex<'a>, found 'a`（結果の型のヒント）。NC19 の柔らかいヒントの規則（ヒントの頭が OEI に現れる）で `s` へのヒントは柔らかくなるが、`c` への兄弟のヒント `f64`・`'a` は着手条件の (ii) がなければ硬いまま |
| C17 | `let c: Complex<f64> = Complex.of 1.0 2.0` / `let b = true` / `let k = 3` / `c + (if b then Complex.of 1 2 else Complex.of 3 4)`・`c + { Complex.of 1 2 }`・`c * (match k with \| 3 -> Complex.of 1 0 \| _ -> Complex.of 0 1)`・`c + Complex { re: 1, im: 2 }` | 通る（`2+4i 2+4i 1+2i 2+4i`、`-O0`・`-O3` で同じ）。ヒント `Complex<f64>` が枝・ブロックの末尾・レコードの構築に届き、`1`・`2` を `f64` に読む。同じ被演算子を `let d = …` に束縛してから `c + d`・`c * d` にすると、どれも `E1003`（`expected an integer, found f64`、位置はリテラル） |
| C18 | `Complex` を名指すプログラム（`let c: Complex<f64> = …`）で `let k: f64 = 2.0` / `k * (if b then 1 else 2)` | `2`。`let d = if b then 1 else 2` / `k * d` は `E1003`（`expected an integer, found f64`）。スカラーのヒントが枝のリテラルの型を決めている |
| C19 | `Complex.div (Complex.from_real 2.0) (Complex.of 1.0 0.0)`、`Complex.div (Complex.from_real 1.0) (Complex.of 0.0 0.0)`、`let inf = 1.0 / 0.0` / `Complex.div (Complex.from_real 1.0) (Complex.of inf inf)` | `2+0i nan+nani nan+nani`（`-O0`・`-O3` で同じ）。`s / c`（D4）はこの値になる。実数を虚部のない数として扱う式なら、1 つ目の虚部は -0 になる |

`docs/language.md` は `Complex` に触れていない（`grep -n Complex docs/language.md` が 0 件）。`tests/*.mjs` にも `Complex` の E2E はない（`tests/gen_docs.mjs` だけが名前を含む）。`Complex` のテストは `tests/complex.rs`（native の `run_native`、7 行）だけ。

## 変えてはいけない意味

- 同じ型どうしの `Complex` の演算子（`c + d`・`c * d`・`c / d`・`-c`）の型と IR。`Complex.mul` の素朴な式も変えない（D3 の範囲は実数の被演算子だけ）。
- `Complex.scale`・`Complex.from_real`・`Complex.div` などの関数の型と値。
- `BigInt` の演算子とリテラル（`big * 2` は `BigInt` のリテラル。C6）。
- `Complex` を名指さないプログラム（opt-in のモジュールは読み込まない）。
- IEEE 754 の意味: 1 回の演算ごとに最近接偶数へ丸める。FMA への縮約、再結合、fast-math をしない。NaN・無限大・符号付きゼロを保つ（AGENTS.md）。

## 仕様

### 1. 8 個のインスタンス

`'a` は `Complex` の要素型（`f32`・`f64` など `Float<'a>` を満たす型）。`c = Complex { re, im }`、`s` は `'a` の値。NC19 の OEI のメソッドは被演算子を共有借用で受ける（`add :: ref L -> ref R -> O`）。

| 式 | ヘッダ | 文脈 | 結果 `(re, im)` |
| --- | --- | --- | --- |
| `c + s` | `Add<Complex<'a>, 'a, Complex<'a>>` | `(Float<'a>, Add<'a>, Copy<'a>)` | `(re + s, im)` |
| `s + c` | `Add<'a, Complex<'a>, Complex<'a>>` | `(Float<'a>, Add<'a>, Copy<'a>)` | `(s + re, im)` |
| `c - s` | `Sub<Complex<'a>, 'a, Complex<'a>>` | `(Float<'a>, Sub<'a>, Copy<'a>)` | `(re - s, im)` |
| `s - c` | `Sub<'a, Complex<'a>, Complex<'a>>` | `(Float<'a>, Sub<'a>, Neg<'a>, Copy<'a>)` | `(s - re, -im)` |
| `c * s` | `Mul<Complex<'a>, 'a, Complex<'a>>` | `(Float<'a>, Mul<'a>, Copy<'a>)` | `(re * s, im * s)` |
| `s * c` | `Mul<'a, Complex<'a>, Complex<'a>>` | `(Float<'a>, Mul<'a>, Copy<'a>)` | `(s * re, s * im)`（`Complex.scale s c` と同じ） |
| `c / s` | `Div<Complex<'a>, 'a, Complex<'a>>` | `(Float<'a>, Div<'a>, Copy<'a>)` | `(re / s, im / s)` |
| `s / c` | `Div<'a, Complex<'a>, Complex<'a>>` | `(Float<'a>, Numeric<'a>, Add<'a>, Sub<'a>, Mul<'a>, Div<'a>, Ord<'a>, Copy<'a>)`（`Div<Complex<'a>>` の文脈に、`from_real` の `Numeric<'a>` を足したもの） | `Complex.div (Complex.from_real s) c` |

- 文脈は本体の式が使うクラスの和で、どれも `Float<'a>`・`Copy<'a>` を含む。頭に現れる型変数は `'a` だけ（NC19 の仕様 1 の `E1027` の規則を満たす）。
- 左右の順序は要素の式でも保つ（`s + re` を `re + s` と書かない）。
- 8 個とも NC19 の被覆の規則を満たす（裸の型変数 `'a` は、もう一方の側の `Complex<'a>` に現れる）。頭の規則も満たす（一方の側の頭が std のレコード `Complex`）。
- 重ならないこと（単一化の出現検査による）: 同じ型どうしの `Mul<Complex<'b>>`（(`Complex<'b>`, `Complex<'b>`)）と `Mul<Complex<'a>, 'a, Complex<'a>>` は `'a ~ Complex<'b>` かつ `'a ~ 'b` で単一化できない。NC19 の `Mul<'a, Tensor<'a>, Tensor<'a>>` と `Mul<Complex<'b>, 'b, Complex<'b>>` は `'b ~ Tensor<Complex<'b>>` になり単一化できない。`Mul<Tensor<'a>, 'a, Tensor<'a>>` とは左の頭が違う。ほかの組も同じ。S1 のテストで確かめる。

### 2. 実数の被演算子の扱い（D3）

- 実数の被演算子は、虚部が +0.0 の複素数としてではなく、虚部を持たない数として扱う（C99 Annex G と同じ。リポジトリの外の知識）（`s / c` を除く。`s / c` は `Complex.div (Complex.from_real s) c` で、実数を虚部 +0.0 の複素数に直して Smith の方法で割る。D4）。そのため、実数を複素数に直してから計算する式（`c * Complex.from_real s`）と、無限大・NaN・符号付きゼロで結果が違う。
- 違いの例（C8 と仕様 1 の式から）:

| 式 | 直接の式（NC21） | 複素数に直す式（今） |
| --- | --- | --- |
| `Complex.of 1.0 inf * 2.0` | `(2, inf)`（`2+infi`） | `(NaN, inf)`（`nan+infi`。`1 * 2 - inf * 0`） |
| `Complex.of 1.0 (-0.0) + 2.0` | `(3, -0.0)`（`3-0i`） | `(3, +0.0)`（`3+0i`。`-0.0 + 0.0 = +0.0`） |
| `0.0 - Complex.of 1.0 0.0` | `(-1, -0.0)`（`-1-0i`） | `(-1, +0.0)`（`-1+0i`。`0.0 - 0.0 = +0.0`） |

- 演算の数: `c * s` は乗算 2 回。複素数に直す式は乗算 4 回と加減算 2 回。
- `s / c` は `Complex.div (Complex.from_real s) c` と同じ値（ビット単位）。`s` の虚部 +0.0 は Smith の方法の中で使われる。直接の式を別に作らない（D4）。

### 3. 解決（NC19 の規則のまま）

- `c * 2.0`・`2.0 * c`（`c: Complex<'x>`）: NC19 の (a)。`Complex` の側が合い、リテラルの側の型が `'x` になる候補は 1 つなので、`2.0` を `'x` として読む。同じ型どうしの `Mul<Complex<'a>>` は、リテラルの側が `Complex` になるので候補にならない。
- `c * 2`（`c: Complex<f64>`）: (a) で `2` を `f64` として読む（整数リテラルは浮動小数点型を取れる。NC19 の D1 (3)）。要素型が未解決で `Float` の制約だけを持つ `c` の `c * 2` は、スカラーの `c.re * 2`（C15）と同じ結果になる。
- `c * x`（`x: f64`、`c: Complex<f64>`）: (b) で単一化できず、(c) の表で 1 つに決まる。
- `c * x`（`x: f32`、`c: Complex<f64>`）: どの候補にも合わず、(d) で今と同じ `E1003`。要素型を暗黙に変換しない。
- 総称関数の中の `c * s`（`c: Complex<'a>`、`s: 'a`）: NC19 の D1 (2) で `'a` を既知の型として扱い、(c) で 1 つに決まる。文脈の制約は関数の制約に加わる。
- スカラーが左の変数の形 `k + c`・`k - c`・`k * c`・`k / c`（`let k: f64 = 2.0`、`c: Complex<f64>`）: 右の `c` に渡す兄弟のヒント `f64` は、`c` の頭 `Complex` が `Mul<'a, Complex<'a>, Complex<'a>>` などの R に現れるので、着手条件の (ii)（NC19 の仕様 3 の 1）で単一化の試しになる。試しが失敗してヒントを捨て、(b) で単一化できず、(c) の表で `'a`×`Complex<'a>` の 1 つに決まる。今は `c` の位置の `E1003`（C16）。
- 総称関数の `fn f c s = s * c`・`s / c`（`c: Complex<'a>`、`s: 'a`）: `s` への結果の型のヒント `Complex<'a>` は頭 `Complex` が OEI に現れるので柔らかく（NC19 の仕様 3 の 1）、`c` への兄弟のヒント `'a`（型変数）は着手条件の (ii) で試しになる。どちらも単一化できずに捨てられ、(c) で 1 つに決まる。今は `s` の位置の `E1003`（C16）。
- `if`・`match`・ブロック・レコードの構築の被演算子（C17）と、スカラーのヒントの `k * (if b then 1 else 2)`（C18）: 着手条件の (i)（NC19 の仕様 3 の 1）で、ヒントは今どおり結果の式に届き、単一化できる場所では今どおり単一化する。NC21 の後も型と IR は今と同じ（S7）。

### 4. できないまま残すもの

- `c ** s` と `c % s`: 同じ型どうしの `Pow`・`Rem` が `Complex` にないので足さない。今と同じ `E1003`（C3）。べき乗は `Complex.pow`。
- `Tensor<Complex<'a>>` と `'a` の演算子（`t * 2.0`）: OEI は組み合わさらないので、今と同じ `E1003`（C13）。GUIDE D-66 の残る問い。
- 要素型の違う組（`Complex<f64>` と `f32`）、整数の型の変数（`Complex<f64>` と `i64` の変数）: `E1003`。
- `BigInt` と整数の型の変数の演算子（`big * n`）: 足さない（D2）。`big * BigInt.of_i64 n` と書く。

### 5. 互換性

- 8 個は振り分けの位置に裸の型変数 `'a` を持つ。NC21 は 8 個を A37 のテスト `std_instances_with_open_dispatch_positions_are_listed`（`src/stdlib.rs`）の一覧に足す。
- 重なりうるのは、NC19 の後・NC21 の前に利用者が書いた、`Complex` を一方の側の頭に持ち、要素型を利用者の型にした OEI だけ（例 `instance Mul<Complex<Money>, Money, Complex<Money>>`。`'a = Money` で `Mul<Complex<'a>, 'a, Complex<'a>>` と重なる）。NC21 の後は `E1016`（`overlapping_instance`）で、利用者のインスタンスのクラス名の位置に出る（A37 の D7）。要素型が利用者の型でない `instance Mul<Money, Complex<f64>, Complex<f64>>` は重ならない（`'a ~ Money` と `Complex<'a> ~ Complex<f64>` が両立しない）。
- この破壊をリリースノートの Compatibility に書く（ドキュメントの節）。`docs/stability.md` の既知の例外「std への instance の追加」（A37 が書く 1 項）に例を足す。

### 6. edition・native と WASM

- 全 edition で同じ（std は `Edition::LATEST` で読む 1 つのコピー）。
- native と WASM の差はない。要素の演算は `'a` の四則と符号反転だけで、`f32`・`f64` では IEEE 754 の 1 回の丸めの命令になる。新しいランタイム関数・intrinsic・SIMD の経路は足さない。

### 7. 例（実装後の意味）

```text
let c = Complex.of 1.0 2.0
c * 2.0                       // 2+4i
2.0 * c                       // 2+4i（Complex.scale 2.0 c と同じ）
c + 0.5                       // 1.5+2i
0.5 - c                       // -0.5-2i
c / 2.0                       // 0.5+1i
2.0 / Complex.of 1.0 1.0      // 1-1i（Complex.div (Complex.from_real 2.0) (Complex.of 1.0 1.0)）
c ** 2.0                      // E1003（今のまま）
```

## 設計

### 段ごとの変更

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1 | `std/Complex.tz`（同じ型どうしのインスタンスの後、416 行の後） | 仕様 1 の 8 個を置く。本体は仕様 1 の式だけで、ほかの関数を呼ばない（`s / c` だけは `div_smith` と `from_real` を呼ぶ）。公開の関数は足さない |
| 2 | `src/stdlib.rs` の `std_instances_with_open_dispatch_positions_are_listed`（A37 が作る） | 一覧に 8 項目を足す（形式は A37 の一覧のまま） |
| 3 | `tests/complex.rs` | テスト計画のテスト |
| 4 | 文書 | ドキュメントの節 |

### 本体の書き方

NC19 の OEI のメソッドは `ref L -> ref R -> O` なので、本体の被演算子は参照である。`Complex` の側はフィールドの読み出し（`left.re`）で読める（同じ型どうしのインスタンスと同じ）。実数の側 `ref 'a` を値の view で読むか `deref` で読むかは、NC19 の OEI のメソッドの引数の規約に従う（未確認。NC19 の実装の時点で `tsuzuri check` で確かめる）。[A40](_completed/A40-reference-operands.md) は参照先が型変数の被演算子に働かないので、A40 には頼らない。下の素案は `deref` で読む形である。

```text
instance (Float<'a>, Add<'a>, Copy<'a>) => Add<Complex<'a>, 'a, Complex<'a>> {
    fn add left right = Complex { re: left.re + deref right, im: left.im }
}

instance (Float<'a>, Sub<'a>, Neg<'a>, Copy<'a>) => Sub<'a, Complex<'a>, Complex<'a>> {
    fn sub left right = Complex { re: deref left - right.re, im: -right.im }
}

instance (Float<'a>, Numeric<'a>, Add<'a>, Sub<'a>, Mul<'a>, Div<'a>, Ord<'a>, Copy<'a>) => Div<'a, Complex<'a>, Complex<'a>> {
    fn div left right = div_smith (from_real (deref left)) (deref right)
}
```

- `s / c` の本体は `div_smith`（`Complex.div` が呼ぶ非公開の関数）に、`from_real s` と `c` を渡す。結果は `Complex.div (Complex.from_real s) c` とビット単位で同じになる。同じ型どうしの `Div<Complex<'a>>`（370 行）は `div_smith` の本体を写しているが、写す理由は未確認。NC21 は写さずに呼ぶ。
- `-right.im` は `Neg<'a>` の符号反転（`f32`・`f64` では符号ビットだけを反転する命令。NaN の符号も反転する）。

### 生成 IR の形

- 各 OEI の呼び出しは、NC19 の仕様 4 の形（被演算子を `BorrowOperand` で貸す呼び出し）で、本体は `'a` の四則 2 回まで（`s / c` を除く）。`-O3` では本体がインライン展開される見込み（性能は主張しない）。
- 確保はない。`@no_alloc`・`@no_trap` の `pure def` で使える（テスト計画）。

## 前提とする他チケットのインターフェース

- [NC19](_completed/NC19-operator-extension-instances.md)（NC19 の D1・D2 の決定）: 3 引数のヘッダ `Add<L, R, O>` の宣言の形（メソッドは `ref L -> ref R -> O`）、1 つの演算子表、宣言の検査（頭の規則・被覆の規則・`overlapping_instance`）、解決の順 (a) リテラルの型・(b) 同じ型どうし・(c) 演算子表・(d) 今の道、`dispatch_positions` の OEI の位置（3 引数の L と R）。NC21 はこれらを変えない。
- [A37](_completed/A37-index-base-place-borrow.md)（A37 の D4・D7 の決定）: テスト `std_instances_with_open_dispatch_positions_are_listed`、利用者と std のインスタンスの重なりを利用者の位置で報告すること、`docs/stability.md` の既知の例外「std への instance の追加」の 1 項、`opt_in_modules_are_reached_only_through_their_names` が `dispatch_positions` のすべての位置で、そのモジュールが宣言する頭を探すこと（8 個は L か R に `Complex` を持つので `std/Complex.tz` に置ける）。

## 他チケットへ提供するインターフェース

- なし。`Tensor<Complex<'a>>` とスカラーの演算子を足すチケットは、NC21 の 8 形と重ならないことを同じ一覧のテストで確かめる。

## 実装手順

1. **変更**: なし（`target/scratch/nc21/` に記録）。**内容**: C1〜C19 を dev バイナリで確かめる。ベースラインとして、同じ型どうしの `Complex` の演算子だけを使うプログラム（`a + b`・`a - b`・`a * b`・`a / b`・`-a`）、C17 の 4 つの被演算子のプログラム（`if`・ブロック・`match`・レコードの構築）、C18 の `k * (if b then 1 else 2)` のプログラム、`big * 2`・`3 * big` のプログラム、`Complex` を名指さないプログラムを `tsuzuri build --emit llvm`（native と `--target wasm32`、`-O0`・`-O3`）で IR にして保存する。**確認**: 「現状」の表と同じ診断と出力（C1〜C5・C16 の help は NC19 の文に変わっていてよい）。C16 の形はこの時点（NC19 の後、NC21 の前）では OEI がないので `E1003` のまま。
2. **変更**: `tests/complex.rs`。**内容**: テスト計画のテストを足す。受理のテストと E2E は今は失敗する。今と同じ結果を確かめるテスト（`c ** 2.0`・`big * n`・`Tensor<Complex<f64>> * 2.0`）は通る。**確認**: `node scripts/slot.mjs -- cargo nextest run --locked --test complex` で、足した受理のテストだけが失敗する。
3. **変更**: `std/Complex.tz`、`src/stdlib.rs`（段 1・2）。**内容**: 8 個のインスタンスと一覧の 8 項目。**確認**: T0。`node scripts/slot.mjs -- cargo test --locked --lib std_instances_with_open_dispatch_positions_are_listed` と `node scripts/slot.mjs -- cargo test --locked --lib opt_in_modules_are_reached_only_through_their_names`（どちらも `running 1 test` を確かめる）。`node scripts/slot.mjs -- cargo nextest run --locked --test complex --test std_examples --test std_argument_order --test borrowed_arithmetic`。手順 1 の IR と `diff` して一致する（S2）。
4. **変更**: 文書（段 4）。**内容**: ドキュメントの節。**確認**: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/complex.md`。
5. **確認**: T2（検証コマンド）。

## テスト計画

### Rust テスト（`tests/complex.rs`）

受理と拒否は `analyze` で確かめ（解析は native と WASM で共通）、値は既存の `run_native`（7 行）で `-O0`・`-O3` の出力を確かめる。

- `complex_real_scalar_operators_have_eight_forms`: `let c = Complex.of 1.0 2.0` で、`c + 0.5`・`0.5 + c`・`c - 0.5`・`0.5 - c`・`c * 2.0`・`2.0 * c`・`c / 2.0`・`2.0 / Complex.of 1.0 1.0` が `1.5+2i`・`1.5+2i`・`0.5+2i`・`-0.5-2i`・`2+4i`・`2+4i`・`0.5+1i`・`1-1i`（`-O0`・`-O3`）。変数の版: `let c: Complex<f64> = Complex.of 1.0 2.0`・`let k: f64 = 2.0` の `c + k`・`k + c`・`c - k`・`k - c`・`c * k`・`k * c`・`c / k`・`k / c` が仕様 1 の式どおりの値（`3+2i`・`3+2i`・`-1+2i`・`1-2i`・`2+4i`・`2+4i`・`0.5+1i`・`0.4-0.8i`）。スカラーが左の 4 形は着手条件の (ii) の道（S8）。
- `complex_real_scalars_keep_signed_zero_and_infinity`: 仕様 2 の表の 3 つの例が `2+infi`・`3-0i`・`-1-0i`。同じプログラムの複素数に直す 3 つの式が今と同じ `nan+infi`・`3+0i`・`-1+0i`（C8）。
- `complex_real_scalar_matches_scale_and_div`: `s * c` が `Complex.scale s c` と、`s / c` が `Complex.div (Complex.from_real s) c` と、表示で一致する（E2E と同じ値の組で）。
- `complex_real_scalar_literal_types`: `let c: Complex<f64> = …` の `c * 2` と `2 * c` が通る。`let c: Complex<f32> = Complex.of 1.0 2.0` の `c * 2.0` が通り、結果が `Complex<f32>`。注釈のない `c` の `c * 2` が、スカラーの `c.re * 2`（C15）と同じ結果になる（違えば NC19 の (a) との食い違いとして報告する）。
- `complex_real_scalar_rejections`: `let c: Complex<f64> = …` / `let x: f32 = 2.0` / `c * x` が `E1003`。`c ** 2.0`・`c % 2.0` が今と同じ `E1003`（位置は `2.0`）。`Tensor.init [2] (\i -> Complex.of 1.0 0.0) * 2.0` が今と同じ `E1003`（C13）。
- `complex_and_tensor_operator_instances_coexist`: `Complex` と `Tensor` を両方名指すプログラムが通る（std の OEI どうしが `E1016` にならない。S1）。`c * 2.0` と `t * 2.0`（`t: Tensor<f64>`）を同じプログラムで使う。
- `user_complex_operator_instances`: `record Money { cents: i64 }` と `instance Mul<Complex<Money>, Money, Complex<Money>> { … }` が `E1016`（kind `overlapping_instance`）で、位置が利用者のインスタンスのクラス名、related（role `declared_here`）が `std/Complex.tz` の `Mul<Complex<'a>, 'a, Complex<'a>>`。`instance Mul<Money, Complex<f64>, Complex<f64>>` は通る。
- `generic_complex_real_scalar`: `def f :: (Float<'a>, Mul<'a>, Copy<'a>) => Complex<'a> -> 'a -> Complex<'a>` / `fn f c s = c * s` が通り、`f64` と `f32` で呼べる。スカラーが左の `fn f c s = s * c` と、`s / c`（`s / c` の文脈を書いた `def`）も通る（C16。S8）。edition 2026 と 2027 の両方（`tests/parameter_conventions.rs:8` の `check` と同じ形で edition を選ぶ）。
- `complex_real_scalar_is_pure`: `@no_alloc` / `@no_trap` / `pure def g :: Complex<f64> -> f64 -> Complex<f64> = \c s -> c * s + s - s / c` が `analyze` を通る（既存の `complex_is_pure_no_alloc_and_opt_in`、163 行と同じ形）。
- `bigint_integer_operands_unchanged`: `big * 2`・`3 * big` が通り（C6）、`big * n`（`n: i64`）が今と同じ `E1003`（C7）。
- `complex_same_type_operator_ir_unchanged`: 手順 1 のベースラインのプログラムの IR が同じ（native と WASM、`-O0`・`-O3`）。C17 の 4 つ（`c + (if b then Complex.of 1 2 else Complex.of 3 4)`・`c + { Complex.of 1 2 }`・`c * (match k with | 3 -> Complex.of 1 0 | _ -> Complex.of 0 1)`・`c + Complex { re: 1, im: 2 }`）が受理されたまま `2+4i`・`2+4i`・`1+2i`・`2+4i` を出し、IR も同じ（S7）。C18 の `k * (if b then 1 else 2)` が `2` で、IR も同じ。
- 一覧: `src/stdlib.rs` の `std_instances_with_open_dispatch_positions_are_listed` が 8 個を含む（手順 3 の確認）。

### E2E（`tests/complex.rs` の `complex_real_scalar_bits_native_and_wasm`。native と WASM の `-O0`・`-O3`）

`tests/borrowed_arithmetic.rs` 197〜245 行の形で、1 つの Rust テストが native の `tsuzuri run` と、`--target wasm32` で作ったモジュールの node での実行を、`-O0` と `-O3` で行う（D5）。

- 被演算子: スカラーは `let` で束縛した変数（`f64`・`f32` の注釈付き）で書き、スカラーが左の形も変数のまま書く（(a) ではなく着手条件の (ii) と (c) の道を通す）。`re`・`im`・`s` を、`f64` では `0.0`・`-0.0`・`1.0`・`-2.5`・`5e-324`・`1e308`・`inf`・`-inf`・`NaN`、`f32` では対応する値（最小の非正規化数、最大に近い有限の値など。`f32` に正確に表せる値を選ぶ）から取る組。8 形の実部と虚部を求める。
- 期待値: テストの Rust のコードが `f64`・`f32` の四則と符号反転で、仕様 1 の式から独立に計算する（Rust の `f64`・`f32` の四則は IEEE 754 の 1 回の丸め）。`s / c` は同じプログラムの `Complex.div (Complex.from_real s) c` と比べ、加えて正確な例 `2 / (1+1i) = (1, -1)`・`1 / (0+1i) = (0, -1)` を独立に比べる。
- native の読み方: プログラムが各値を表示し、Rust がそれを `f64`・`f32` に読み直してビットを比べる（`f64` の表示は最短の往復表記で、`-0`・`inf`・`-inf`・`nan` を出す。C12。`_tsuzuri/language-reference/types-and-type-inference/cast.md` 199 行）。
- WASM の読み方: 同じ式を `export def`（`f64`・`f32` を返す）にし、node が `DataView` でビットを読む。モジュールは import を持たない（`WebAssembly.Module.imports(m).length == 0`）。
- 比べ方: NaN の期待値には NaN かどうかだけを比べる（符号とペイロードは比べない。`tests/math.mjs` の NC01 の節と同じ）。ほかはビット単位で一致させる。仕様 2 の 3 つの例は必ず含める。

### 既存テストへの影響

- 期待値の変更はない見込み。`Complex` の E2E は今ない。`tests/complex.rs` の既存のテストは同じ型どうしの演算子と関数だけを使う。
- `tests/std_argument_order.rs` は `def` の行と組み込みの型の型を見るので、インスタンスの追加では変わらない。

### 性能

確保なし、本体は四則 2 回まで（`s / c` を除く）。複素数に直す式より演算が少ないが、性能は主張しない（計測しないため）。

### 検証コマンド

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`node scripts/slot.mjs -- cargo nextest run --locked --test complex`。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`。
- T2: T1、`node scripts/slot.mjs -- cargo test --locked --lib std_instances_with_open_dispatch_positions_are_listed`（`running 1 test`）、`cargo build`、`node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/complex.md`、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。`std/Complex.tz` の doc の例を足したら `node scripts/slot.mjs -- cargo nextest run --locked --test std_examples`。

## ドキュメント

- `built-in-types-and-modules/complex.md`:
  - 「この記事のポイント」の 3 つ目（`Add`・`Sub`・`Mul`・`Div`・`Neg`・… を備えます）に、実数のスカラーとの `+`・`-`・`*`・`/`（両方の順）を足す。
  - 「構築と演算」の後に節「実数との演算子」を足す。仕様 1 の 8 形の表（式と結果）、実数を虚部のない数として扱うこと（`+`・`-`・`*` と `c / s`。`s / c` は `Complex.div (Complex.from_real s) c` と同じ値で、例: `1.0 / Complex.of 0.0 0.0` は `nan+nani`）、仕様 2 の違いの表、`**`・`%` と `Tensor<Complex<'a>>` とスカラーが書けないこと、要素型を暗黙に変換しないことを書く。例は `run=` で、`check-docs` で確かめる。素案（期待値は仕様 1 の式から手で求めた。実装後に `check-docs` で確かめる）:

    ````text
    ```tsuzuri run=2+4i%201.5+2i%20-0.5-2i%201-1i
    let c = Complex.of 1.0 2.0
    $"{c * 2.0} {c + 0.5} {0.5 - c} {2.0 / Complex.of 1.0 1.0}"
    ```

    ```tsuzuri run=2+infi%20nan+infi%20-1-0i%20-1+0i
    let inf = 1.0 / 0.0
    let c = Complex.of 1.0 inf
    let z = Complex.of 1.0 0.0
    $"{c * 2.0} {c * Complex.from_real 2.0} {0.0 - z} {Complex.from_real 0.0 - z}"
    ```
    ````

  - API リファレンスの `Complex.scale` の行に「`s * c` と同じ」を足す。
- `docs/language.md`: HEAD は `Complex` に触れていない。NC19 が「演算子」（4432 行）に書く OEI の段落に、std の `Complex<'a>` が実数 `'a` との `+`・`-`・`*`・`/` を両方の順で持ち、実数を虚部のない数として扱うこと（`s / c` は `Complex.div (Complex.from_real s) c` と同じ）を 1 文で足す。
- `docs/stability.md` の既知の例外「std への instance の追加」（A37 が書く 1 項）の例外 1 に、NC21 の例（`Mul<Complex<'a>, 'a, Complex<'a>>` と、重なる利用者の `instance Mul<Complex<Money>, Money, Complex<Money>>`）を足す。別の項は書かない。
- NC19 が OEI を説明するページ（`type-classes.md`・`op-and-expressions.md`）に std の OEI の一覧があれば、`Complex` の 8 形を足す。
- リリースノート: PR の説明に Compatibility の文案を置く。文案: `**Compatibility:** the standard library's Complex now has +, -, * and / with a real scalar on either side (c * 2.0, 2.0 - c). For +, -, * and c / s, a real operand counts as a number without an imaginary part, so these keep infinities and signed zeros that promoting the real to Complex loses; s / c equals Complex.div (Complex.from_real s) c. A user instance that writes Complex at the head of one side with an element type of your program, such as instance Mul<Complex<Money>, Money, Complex<Money>>, now overlaps them and is E1016 (overlapping_instance) at your instance.`（リリースのときに `vsc/CHANGELOG.md` に写す）。
- `std/Complex.tz` の `scale` の説明に「`s * c` と同じ」を足す場合は、doc の例を `tests/std_examples.rs` で確かめる（任意）。
- README: 書いてあること（特徴の一覧・検証手順）は変わらないので変更しない。

## 受け入れ条件

- [ ] 8 形が受理され、仕様 1 の式どおりの値を出す（`-O0`・`-O3`）。スカラーが左の形は、リテラル（`2.0 * c`）と変数（`let k: f64` の `k * c`、総称の `s * c`・`s / c`）の両方で通る（S8）。
- [ ] C17 の 4 つの被演算子と C18 の `k * (if b then 1 else 2)` が、NC21 の後も今と同じ値と IR で受理される（S7）。
- [ ] 仕様 2 の 3 つの例と E2E の値の組が、独立に計算した期待値とビット単位で一致する（native と WASM、`-O0`・`-O3`。NaN は NaN かどうか）。
- [ ] `s * c` が `Complex.scale s c` と、`s / c` が `Complex.div (Complex.from_real s) c` と一致する。
- [ ] `c ** 2.0`・`c % 2.0`・`Tensor<Complex<f64>> * 2.0`・`c * x`（`x: f32`）・`big * n` が今と同じ診断のまま。`big * 2`・`3 * big` が通る。
- [ ] `std_instances_with_open_dispatch_positions_are_listed` が 8 個を含み、`opt_in_modules_are_reached_only_through_their_names` が通る（どちらも `running 1 test`）。
- [ ] 利用者の `instance Mul<Complex<Money>, Money, Complex<Money>>` が、利用者の位置の `overlapping_instance` になる。
- [ ] 同じ型どうしの `Complex` の演算子と `BigInt` の演算子の IR が、手順 1 の記録と一致する。
- [ ] `complex.md`・`docs/language.md`・`docs/stability.md` の例が更新され、PR の説明に Compatibility の文案がある。
- [ ] T2 が通る。

## 落とし穴

- 実数を `Complex.from_real` で直してから同じ型どうしの演算子に渡さない（`s / c` を除く）。無限大で NaN になり、虚部の -0.0 が +0.0 になる（仕様 2）。
- 要素の式の左右を入れ替えない。`s + re` と `re + s` は IEEE 754 の加算として同じ値だが、NaN のペイロードの伝わり方を変えないため、書いた順を保つ。
- `s - c` の虚部は `-im` で、`0.0 - im` ではない（`0.0 - 0.0 = +0.0` で、-0.0 にならない）。`Neg<'a>` を文脈に書く（`Float` に上位クラスはない）。
- `s / c` の文脈に `Numeric<'a>` を忘れない（`from_real` が `0.0 as 'a` を使う。18〜19 行）。
- E2E の期待値をコンパイラの出力から作らない（`s / c` の比べる相手を除く）。Rust の `f64`・`f32` の演算で独立に作る。
- `f32` の E2E の値は `f32` に正確に表せる値を選ぶ。`f64` のリテラルを `f32` に丸めた値を期待値に使うなら、Rust 側でも同じ丸め（`as f32`）をしてから計算する。
- 一覧のテストに 8 個を足さずに通そうとしない。足さなければテストが失敗するのが正しい（A37 の D4）。
- スカラーが左の形をリテラル（`2.0 * c`）だけで確かめない。リテラルは NC19 の (a) を通るが、変数の `k * c` と総称の `s * c` は着手条件の (ii) に頼る（C16。S8）。
- `Complex` が OEI の頭になると `Complex<f64>` のヒントが柔らかくなる。`if`・`match`・ブロック・レコードの構築の被演算子（C17）でヒントが結果の式に届かなくなると、今受理している式が `E1003` になる（S7）。

## 対象外

- `Tensor<Complex<'a>>` とスカラーの演算子（GUIDE D-66 の残る問い）。
- 整数の型の被演算子（`Complex<f64>` と `i64` の変数）と、要素型の違う組。
- `Complex` の `**`・`%`、`Complex.pow` の演算子。
- `BigInt` と整数の型の演算子（D2 で却下）。
- 同じ型どうしの `Complex` の `Mul` の式の改善（Annex G の無限大の回復など）。

## 決定事項

### D1: `Complex` と実数のスカラーの 8 個の OEI を足す（承認済み。D-66 の委任で決定）

- 決定: `std/Complex.tz` に、`Add`・`Sub`・`Mul`・`Div` × 2 形の 8 個を置く（仕様 1）。8 個は振り分けの位置に裸の型変数を持つので、A37 の一覧に載せ、リリースノートの Compatibility で知らせる。NC19 の D2 が求める承認は、この決定で与えた。NC19 の Phase 2 ではなく、別のチケットにする。
- 理由: `Complex` には今リテラルの注入がなく、`c * 2.0` は `E1003` なので（C1）、受理するのは新しいプログラムだけである。8 個は NC19 の被覆の規則を満たし、`Tensor` の OEI とも、`Complex` を頭に書かない利用者の OEI とも重ならない。重なりうるのは NC19 の後に書かれた、`Complex` を頭に持つ利用者の OEI だけで、範囲が小さい。比べられる複素数のライブラリ（.NET、C++ の `std::complex`、Julia、NumPy）は、どれも `複素数 * 実数` を書ける（リポジトリの外の知識で、未検証）。std だけの作業で NC19 の仕組みの上に載るので、NC19（L）を膨らませずに 1 チケット 1 PR にする。
- 採らなかった案: NC19 の Phase 2 にする（NC19 が大きくなる）、足さない（`c * 2.0` が `E1003` のまま）。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D2: `BigInt` と整数の OEI は足さない（却下。D-66 の委任で決定）

- 決定: `Mul<BigInt, i64, BigInt>` などの `BigInt` と整数の型の OEI は足さない。`big * n`（`n: i64`）は `big * BigInt.of_i64 n` と書く。
- 理由: `big * 2`・`3 * big` は今でも動く（整数リテラルが `BigInt` になる。`src/check.rs:16515`。C6）。そこに `Mul<BigInt, i64, BigInt>` を足すと、NC19 の (a) が `big * 2` のリテラルの型を 2 つ（`BigInt` と `i64`）見つけて `E1043` にし、受理していたプログラムを拒否する。避けるには解決に優先の特例が要る。残る `big * n` は `i64` から `BigInt` への暗黙の変換で、Tsuzuri は型を暗黙に変換しない（GUIDE §1 の 2）。
- 状態: 却下（D-66 の委任で決定、2026-11-25）。

### D3: 実数の被演算子を虚部のない数として扱う直接の式にする（決定）

- 決定: 仕様 1 の式。実数を複素数に直してから計算しない（`s / c` を除く。D4）。
- 理由: 直接の式は演算が少なく（`c * s` は乗算 2 回。直すと乗算 4 回と加減算 2 回）、IEEE 754 の意味でも正しい。std の `Mul<Complex<'a>>` は素朴な式なので（363〜367 行）、`Complex.of 1.0 inf` に `2.0` を直して掛けると実部が `inf * 0 = NaN` になる。直接の式では `(2, inf)` になる。`c + s` は虚部の -0.0 を保つ。`from_real` は虚部を +0.0 にする（18〜19 行）ので、直すと -0.0 が +0.0 に変わる（C8）。C99 Annex G も実数の被演算子を虚部のない数として扱う（リポジトリの外の知識）。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D4: `s / c` は `Complex.div (Complex.from_real s) c` と同じにし、`Pow`・`Rem` は足さない（決定）

- 決定: `s / c` は `div_smith` に `from_real s` と `c` を渡す。`Pow`・`Rem` の OEI は足さない。
- 理由: 実数を分子にする除算でも、分母の大きさの補正（Smith の方法）が要る。同じ関数を使えば、`Complex.div` と結果がビット単位で一致し、別の式の検証が要らない。`Pow`・`Rem` は、`Complex` に同じ型どうしのインスタンスがないので、実数との形だけを足す理由がない。`c ** 2.0` は今と同じ `E1003` のまま（C3）。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D5: E2E は `tests/complex.rs` の Rust テストで native と WASM を実行する（既定案）

- 決定: 符号付きゼロと無限大のビットの検査は、`tests/complex.rs` の 1 つの Rust テストが native の `tsuzuri run` と WASM の node の実行を `-O0`・`-O3` で行う形にする（`tests/borrowed_arithmetic.rs` 197〜245 行と同じ形）。期待値は Rust の `f64`・`f32` の演算で独立に作る。
- 理由: `Complex` の E2E は今どの suite にもない。`tests/math.mjs` の NC01 の節は native と WASM のビット比較の手本だが、`tests/math.mjs` は重く（README の表で「重い。Node 24」）、`.github/workflows/` のどこからも呼ばれない（確認済み）。Rust テストなら CI の Rust のテストの段で毎回走る。
- 採らなかった案: `tests/math.mjs` に節を足す（CI で走らない）、`tests/features.mjs` の suite にする（fixture の関数は主に `i64` を返す。浮動小数点のビットを整数として取り出す組み込みの関数は見つからなかった（`src/` の組み込みの名前を `bits` で探すと `Int.reverse_bits` だけ）ので、符号付きゼロを `Math.signbit` などで整数に直す手間が増える）。
- 状態: 既定案。
