# A40: 演算子の被演算子に参照 `ref T`・`ref mut T`（edition 2027 の読み取り引数を含む）を直接書けるようにする

| 項目 | 内容 |
| --- | --- |
| ID | A40 |
| 優先度 | P1（edition 2027 は最新の edition（`Edition::LATEST`）で、マニフェストのないソース・スクリプト・文書の例に使われる。この edition は所有する具体型の引数を既定で共有借用にするので、`string`・`BigInt`・非 Copy のレコード・`Tensor` を受ける関数の本体では、演算子のたびに `deref` が要る。`string` の `+` には `deref` でも書く方法がない（現状 P8）。`pure def` の借用の引数も同じで、こちらは全 edition） |
| 規模 | M |
| 依存 | –（土台の [A11](_completed/A11-borrowed-comparisons.md)・[NC03](_completed/NC03-borrowed-arithmetic-operators.md)・[A18](_completed/A18-parameter-conventions.md) は完了済み。A11 は `Eq`・`Ord` の被演算子の借用、NC03 は算術の被演算子の借用 D1-A、A18 は edition 2027 の読み取りの既定） |
| 後続 | ([NC19](NC19-operator-extension-instances.md))・([NC20](NC20-compound-assignment-and-mutable-index-views.md))（順序はどちらでもよい。後に入る方が、他方の経路に参照を外す処理を足す。NC19 の前に入れるのを推奨: D4） |
| 状態 | todo |
| 起票 | 2026-11-25（[GUIDE D-66](GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20) の委任による決定。NC19 の D9 (ii) を分けた。HEAD `858a949c` で確認） |
| 承認 | 承認済み（D-66 の委任で決定、2026-11-25）: NC19 の D9 (ii)（参照の被演算子を、借りる演算子へ直接貸す）を、NC19 の Phase 2 ではなくこのチケットに分けて承認した（D1）。対象を、参照先の頭が分かる型と、今の道が `E1003`・`E1005` を報告する場所に限る（D2）。NC19・NC20 との順序（D4）は決定（D-66 の委任で決定、2026-11-25）。ほかは既定案 |
| 改善する劣位 | 書きにくさ（最新の edition の既定）: `fn same a b = a == b`（`string`）は `E1005`、`fn greet a b = a + b`（`string`）・`fn add2 a b = a + b`（`BigInt`）・非 Copy のレコードの `a + b` は `E1003` になる（現状 P1〜P5）。`E1003` の help が勧める `own` は実引数を move させる。今の回避は `deref a == deref b` で、`string` の連結には演算子の回避がない（P8）。一貫性: 演算子はすでに被演算子を暗黙に借りている（A11、NC03 D1-A）のに、すでに参照である被演算子は受けない。呼び出しは `ref mut T` を `ref T` の仮引数へ借り直して渡す |
| 手本にする既存実装 | `src/check.rs:15423` の `coerce_argument`（参照を `ref T` の仮引数へ渡すとき、15456〜15467 行で `Borrow(Dereference(r), false)` を作る。`reborrow_operand` は 16441 行）、`src/polymorph.rs:720` の `Inference::unifies`（単一化を試して巻き戻す。A32 の `src/closures.rs:164`・181 行が使う）、`src/polymorph.rs:3676` の `Classes::validate`（`&self` で状態を変えずにインスタンスの有無を確かめる）、`src/polymorph.rs:6678` の `operator_call`（`Eq`・`Ord`・組み込みでない型の算術の引数を `BorrowOperand` で貸す）、`src/llvm.rs:8060` の `binary`（`string` の比較は `read_operand`、`+` は `take_operand` と `append`・`concat`） |
| 主な影響ファイル | `src/check.rs`（`binary_expression` 14209 行、`finish_expression` 13694 行、`call_expression` 14022 行、単項の型付け 14476 行付近、`binary_type` 17001 行、`TypedExprKind` 5421 行）、`src/polymorph.rs`（`operator_call` 6678 行、演算子の下げ方 6557〜6585 行）、`src/ownership.rs`（`E::Binary` の使い方 15095〜15123 行）、`src/llvm.rs`（`binary` 8060 行、`StringLength` の分岐 6586 行）、`src/llvm_frame.rs`（`frame_concat` 419 行、`concat_length` 441 行）、型付き IR を走査するほかのファイル（新しい種類を足す場合。設計の D3）、`tests/reference_operands.rs`（新規）、`tests/fixtures/reference_operands/`（新規。`Tsuzuri.toml` で edition 2027）、`tests/features.mjs`、`spec/rules/TYP.md`・`tests/conformance/TYP/`（規則の番号は実装の時点の TYP の次の空き）、`_tsuzuri/language-reference/values-and-functions/functions.md`、`_tsuzuri/language-reference/values-and-functions/op-and-expressions.md`、`docs/language.md`（「演算子」） |

## 実装者向け早見

- 範囲: 算術・比較・ビット・論理の演算子で、被演算子の型が `ref T` か `ref mut T` で、参照先の頭が分かり、今の道がその演算子の式（被演算子へのヒントを含む）で `E1003`・`E1005` を報告するときだけ、参照を 1 段外した型 `T` で今の解決をやり直す。借りる演算子には参照をそのまま貸し、値を取る演算子は `deref` と同じ読み出し（Copy の型だけ）を渡し、`string`・`utf8string` の `+` は参照の被演算子を消費しない連結にする。全 edition。今受理しているプログラムの型と IR は変えない。
- 最初に読む: ① 下の「現状」の P1〜P17 ② `src/check.rs` 14209〜14291 行（`binary_expression`）と 17001〜17027 行（`binary_type`） ③ `src/polymorph.rs` 6557〜6585 行と 6678〜6735 行（`operator_call`） ④ `src/llvm.rs` 8060〜8133 行（`string` の演算子）と `src/llvm_frame.rs` 419〜457 行（`string` の `+` のほかの 2 つの道 `frame_concat`・`concat_length`） ⑤ 決定事項 D2・D3。
- 最初に書くテスト: `tests/reference_operands.rs` に、P1〜P7 の各形が通って期待の値を出すテスト（今は失敗する）と、P10 の総称の `ref 'a` と利用者の `instance Eq<ref Foo>` が今と同じ結果になるテスト（今も通る）を書く。
- 触ってはいけない: 今受理しているプログラムの型付き IR と LLVM IR。`string + string` の消費と、左のバッファを再利用する `@tz.string.append` の道（NC03 の「触ってはいけない」）。評価の順序（EVL-001）と一時値の解放の順序。`|>`・`>>`・`<<`・`as`・`ref`・`deref`・添字。総称の `ref 'a` の被演算子（今の道のまま）。`std/Text.tt` と `text.md` の `deref left < deref right`。
- 完了の見分け方: P1〜P7 と P16 が受理され、P9 の `string` の消費・P10 の総称の道・P11 の `E1014`・P12 の `E1012`・P17 の 2 つの `E1003` が今と同じになり、段 1 のベースラインの IR が 1 バイトも変わらない。

## 目的

edition 2027 の通常の `def` は、所有する具体型の引数を既定で共有借用にする（A18）。本体ではその引数が `ref T` の局所なので、`a + b`・`a == b`・`-a` が書けない。今の回避は `deref a + deref b` で、最もよく書くコードに毎回 `deref` が要る。`string` の連結は `deref` でも書けない（P8）。help が勧める `own` は実引数を move させ、呼び出し側の値を使えなくする。

演算子はすでに被演算子を暗黙に借りている（A11 の `Eq`・`Ord`、NC03 D1-A の算術）。すでに参照である被演算子を受けるのはその裏返しで、借用の意味も所有権の規則も変わらない。このチケットは、参照の被演算子を、借りる演算子へはそのまま貸し、値を取る演算子へは `deref` と同じ読み出しを渡す。働くのは今 `E1003`・`E1005` になる場所だけなので、受理していたプログラムは変わらない。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準コマンドが成功し、「現状」の P1〜P17 を基準の dev バイナリ（`cargo build`）で確かめていること。
- 承認は済んでいる（D-66 の委任で決定、2026-11-25）。依存はない。
- [NC19](NC19-operator-extension-instances.md) か [NC20](NC20-compound-assignment-and-mutable-index-views.md) が先に入っていたら、「前提とする他チケットのインターフェース」の順序の規則に従い、その経路にも参照を外す処理を足す（D4）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- **S1** 段 1 のベースライン（実装手順 1）の型付き IR か LLVM IR が 1 つでも変わる。`std/`・`examples/`・`tests/`・`_tsuzuri/language-reference/` のプログラムで、今受理しているものの出力か、今拒否しているもののうち参照の被演算子の形でないものの診断が変わる。
- **S2** `string`・`utf8string` の `+` の参照の被演算子が、どの道でも `@tz.string.append`（左のバッファを `tz.realloc` で伸ばす。`src/runtime/string.ll:80`）に届くか、参照先を解放する。解放後のメモリの使用になる。道は 3 つある: `src/llvm.rs:8060` の `binary`、`src/llvm_frame.rs:419` の `frame_concat`（`concat_into`）、441 行の `concat_length`（`src/llvm.rs:6586` の `StringLength` の分岐から）。どれかで参照の被演算子が `take_operand` か `drop_framed` に渡る。
- **S3** 今の道の試行（仕様 2）が、`Inference::unifies` と `Classes::validate` では状態を残さずに書けず、受理しているプログラムの推論を変える。
- **S4** 参照先の頭が型変数か未解決の推論変数の被演算子で、A40 が働く形が見つかる（総称の `ref 'a` の `==` と利用者の `instance Eq<ref Foo>` の組（P10）が `true` を出さなくなる、など）。
- **S5** NC19 か NC20 が先に入っていて、その解決の道に参照を外す処理を足すと、その道で受理していたプログラムの型か IR が変わる。
- **S6** GUIDE §13.1 の共通の条件。
- **S7** 入れ子の演算子の式の木（仕様 2 の「確定と戻し方」）で、戻したときに今の道の診断（コード・位置・文言・help・順序）を同じに保てない形が見つかり、その形を A40 が働かない範囲（木の形か深さで決める）に絞れない。絞れるなら、絞った形を対象外に書き、完了報告で知らせる。

## 現状（HEAD `858a949c` で確認）

### 参照の被演算子の型付け

- `binary_expression`（`src/check.rs:14209`）は、左の被演算子を式の期待型をヒントにして検査し、右の被演算子を左の型（兄弟の型）をヒントにして検査する（14256〜14269 行）。比較と `&&`・`||` には式の期待型を渡さない（14243〜14254 行）。左がリテラルなら右を先に検査する（14256 行）。
- 被演算子の型とヒントの単一化は、値の式では `finish_expression` の `self.same(&value.ty, expected, span)?`（13742 行）で行う。合わなければ `E1003`。
- `binary_type`（17001 行）は、右と左の型を `same` で単一化し（17012 行）、`&&`・`||` では左を `bool` と単一化し、ほかは `require(binary_class(op), 左の型)` でクラスを確かめる。結果の型は比較なら `bool`、ほかは左の型。単項は 14476〜14498 行で、`!` は `bool` との単一化、`-` は `Neg`、`~~~` は `Bits` を要る。
- `require`（`src/polymorph.rs:5088`）は、型に未解決の推論変数がなければ `Classes::validate`（3676 行）で確かめ、なければ `E1005`（`no instance for C<T>`）。型変数を含む制約は呼び出し側へ送る。
- edition 2027 の既定の読み取り引数と `pure def` の借用の引数は、型が型変数か関数に届くときだけ、本体で値の view（`Dereference`）として読む（`bind_function_parameters`、13189〜13230 行。`finish_expression` の 13702〜13710 行）。`string`・`BigInt`・非 Copy のレコード・`Tensor<i64>` などの具体型は `ref T` の局所のまま残る。`E1003` のメッセージに足す `parameter 'a' is read-only by default (edition 2027); write 'own T' …` は `read_only_hint`（5170 行）。

### 下げ方と所有権

- 組み込みの命令を持たない型の演算子（`intrinsic` が偽。`src/polymorph.rs:3648`）は、特殊化で `operator_call`（6678 行）に下がる（6557〜6585 行）。配列・リスト・タプルの `Eq`・`Ord` は構造的なインスタンス（3533 行の `structural`）で、これも `operator_call` を通る。`Eq`・`Ord` と、組み込みでない型の算術クラス（`borrowed_arithmetic_class`、942 行。`Add`〜`Pow`・`Neg`）は、各引数を `BorrowOperand` で包む（6718〜6728 行）。`Bits` は包まず、値を渡す（942 行のクラスに入らない）。
- 所有権検査の `E::Binary`（`src/ownership.rs:15095`）は、比較と、スカラー・SIMD・`string`・`utf8string` 以外の型の算術を読み出し（`Use::Read`）、ほか（スカラーの算術、`string` の `+`、ビット）を消費（`Use::Consume`）として評価する（15096〜15123 行）。
- `string` の `+`（`src/llvm.rs:8060`）は 2 つの被演算子を `take_operand` で取り、左がスタックの文字列でなければ `@{runtime}.append`（左のバッファを伸ばす）、そうでなければ `@{runtime}.concat`（新しく確保して両方を写す。`src/runtime/string.ll:48`）を呼ぶ（8091〜8099 行）。比較は `read_operand` で読み、解放しない。コンパクト文字列（`TSUZURI_STRINGS=compact`）も同じ名前の関数を持つ（`src/runtime/string_compact.ll:53`・102）。
- `string` の `+` を下げる道はほかに 2 つある。結果が外へ出ない式（`let s = a + b` の後に `s.length` だけを使う、など）は、`emit_frame_inner`（`src/llvm_frame.rs:241`〜255）が `reserve_buffer` に成功すると `frame_concat`（419〜439 行）に送る。`frame_concat` は両方を `take_operand` で取り、`@{runtime}.concat_into`（スタックのバッファに写すだけ。`src/runtime/string.ll:104`、`src/runtime/string_compact.ll:164`）を呼び、両方を `drop_framed` で解放する。`(a + b).length` は `StringLength` の分岐（`src/llvm.rs:6586`〜6588）が `concat_length`（`src/llvm_frame.rs:441`〜457）を呼ぶ。`concat_length` も両方を `take_operand` で取り、長さだけを足して、両方を `drop_framed` で解放する（連結の関数を呼ばない）。所有する `string` の引数で確かめた IR（`tsuzuri build --emit llvm -O0`）: `let s = a + b` / `s.length` は `@tz.string.concat_into`、`(a + b).length` は連結の関数を呼ばず、`fn greet a b = a + b` は `@tz.string.append`。`read_operand` で読んだ被演算子を放す補助は `release_operand`（`src/llvm_frame.rs:678`。場所なら解放しない）。
- 呼び出しは、参照を `ref T` の仮引数へ渡すとき `Borrow(Dereference(r), false)` を作る（`coerce_argument`、15456〜15467 行）。`ref mut T` の参照も共有に借り直して渡せる（P14）。

### プローブ（検証済み。HEAD `858a949c` の dev バイナリ、macOS arm64 の native。マニフェストなし（edition 2027）。`--edition 2026` と書いたものはその edition）

各プローブは、`def` の型と本体を書き、`tsuzuri run` で確かめた。

| # | 形 | 結果 |
| --- | --- | --- |
| P1 | `def greet :: string -> string -> string` / `fn greet a b = a + b` | `E1003` を 2 つ（`a` と `b`）: `expected string, found ref string; parameter 'a' is read-only by default (edition 2027); write 'own string' in the signature to take ownership`。`utf8string` も同じ |
| P2 | `def same :: string -> string -> bool` / `fn same a b = a == b` | `E1005`: `no instance for Eq<ref string>; define an instance or use a supported type`（位置は `a == b` の始まり）。`def eqs :: ref string -> ref string -> bool` も同じ。`pure def same :: string -> string -> bool` は `--edition 2026` でも同じ `E1005` |
| P3 | `BigInt` の引数で `a + b`、`-a`、`a + 1`、`a < b` | `a + b`・`-a`・`a + 1` は `E1003`（`expected BigInt, found ref BigInt; parameter 'a' is read-only …`）、`a < b` は `E1005`（`no instance for Ord<ref BigInt>`） |
| P4 | `def lt :: ref mut [i64] -> ref [i64] -> bool` / `a < b`、`ref [i64]` どうしの `a < b` | 前者は `E1003`（`expected ref mut [i64], found ref [i64]`、位置は `b`）。後者は `E1005`（`no instance for Ord<ref [i64]>`） |
| P5 | `record Foo { n: i64, tag: string }` と `instance Add<Foo>`、`def plus :: Foo -> Foo -> Foo` / `a + b` | `E1003` を 2 つ（`expected Main.Foo, found ref Main.Foo; …`） |
| P6 | 明示の `ref i64` の引数で `a + b`（結果 `i64`）、`a &&& b`、`a + 1`、`1 + a`、`ref i64` と `i64` の `a + b`、`ref mut i64` どうしの `a + b`、`ref bool` の `!a` と `a && b`、`let x: i64 = 3` / `let r = ref x` / `r + 1` | どれも `E1003`。結果の型がヒントになる形は被演算子の位置（`expected i64, found ref i64`、`expected i64, found ref mut i64`、`expected bool, found ref bool`）。`r + 1` は `1` の位置（`expected ref i64, found i32`）。`a + b` は `--edition 2026` でも同じ |
| P7 | `Tensor<i64>` の引数で `a == b`、`Tensor<f64>` の引数で `a + b` | `E1005`（`no instance for Eq<ref Tensor<i64>>`）と `E1003`（`expected Tensor<f64>, found ref Tensor<f64>; …`）。`Tensor` の算術インスタンスは HEAD にない（NC06 が足す） |
| P8 | 回避: 明示の `ref i64` の `deref a + deref b`、`BigInt` の `deref a + deref b`、`string` の `deref a == deref b`、`ref [i64]` の `deref a < deref b`、`string` の `deref a + deref b` | 前の 4 つは通る（`3`・`3`・`true`・`true`）。`string` の `deref a + deref b` は `E1012`（`cannot move a non-Copy value out of a reference; parameter 'a' is read-only …`） |
| P9 | `let s = "ab"` / `let u = s + "c"` / `let w = s + u` | `E1012`（`use of moved or partially moved value 's'`、note `value moved here` は `s + "c"`）。`string` の `+` は左を消費する |
| P10 | `def gen :: ref 'a -> ref 'a -> bool` / `fn gen a b = a == b` | 検査を通る（`Eq<ref 'a>` を呼び出し側へ送る）。`record Foo` と `instance Eq<ref Foo>` があれば `gen (ref x) (ref y)` は `true`。`let x = 1` の `gen (ref x) (ref y)` は呼び出しの位置で `E1005`（`no instance for Eq<ref i32>`） |
| P11 | `let mut x = 1` / `let r = ref x` / `let v = deref r + bump (ref mut x)`（`bump :: ref mut i64 -> i64`）。`string` で `(deref r).length + bump (ref mut s)` | どちらも `E1014`（`access conflicts with a live borrow; …`、位置は `ref mut x`、note は `let r = ref x`）。参照を通した読み出しは演算子の評価が終わるまで続く |
| P12 | `def take :: ref Foo -> Foo` / `fn take a = deref a`（`Foo` は非 Copy） | `E1012`（`cannot move a non-Copy value out of a reference`） |
| P13 | `def eqg :: Eq<'a> => Tensor<'a> -> Tensor<'a> -> bool` / `a == b` | 通る（`true`）。型が型変数に届く読み取り引数は値の view で読む |
| P14 | `def total :: ref [i64] -> i64` に `let r = ref mut v` の `r` を渡す（`total r`） | 通る（`3`。`ref mut` を共有に借り直して渡す） |
| P15 | `let x: i64 = 3` / `let r = ref x` / `let rr = ref r` / `rr + 1`。`--edition 2026` で `let rr = &r` / `rr + 1` | 前者は `E1003`（`expected ref i64, found i32`）。`ref r`（`r` が参照）は再借用で `ref i64` になり、`ref ref i64` にはならない（`src/check.rs:16440` の注釈）。後者は `E1003`（`expected ref ref i64, found i32`、位置は `1`）。edition 2026 の記号の借用 `&r` は `ref ref i64` を作る（`tests/borrow_syntax.rs:310`〜317 がこれに頼る）。edition 2027 の `&` は `E0004` |
| P16 | 式の期待型がない混在: `def c1 :: ref i64 -> i64 -> bool` / `a == b`・`a < b`、`def c3 :: ref i64 -> i64 -> i64` / `let z = a + b` / `z`、読み取り引数の `string` の `a` と所有の `let s = "x" + ""` の `a == s`・`s == a`、`a == "x"` | どれも `E1003`。`a == b`・`a < b`・`let z = a + b` は `b` の位置で `expected ref i64, found i64`。`a == s` は `s` の位置で `expected ref string, found string`、`s == a` は `a` の位置で `expected string, found ref string; parameter 'a' is read-only …`。`a == "x"` は `"x"` の位置で `expected ref string, found string`。比較と `let` は左に期待型を渡さないので、失敗するのは右の被演算子で、参照の型でないこともある |
| P17 | `record Foo { n: i64, tag: string }`（`Add` なし）で、`def plus :: Foo -> Foo -> Foo` / `fn plus a b = a + b`（3 行目）。`def plus :: Foo -> i64 -> Foo` の同じ本体 | 前者は `E1003` を 2 つ（3:15 の `a` と 3:19 の `b`。どちらも `expected Main.Foo, found ref Main.Foo; parameter '…' is read-only by default (edition 2027); …`）。後者は `a` の同じ `E1003` と、`b` の位置の `expected Main.Foo, found i64`。`tsuzuri check` も同じ。今の道は回復して、同じ式から 2 つ報告する |

参照の被演算子の演算子の誤りを期待する既存のテストはない（`rg -n 'read-only by default' tests` の 3 ファイル `tests/pure_functions.rs`・`tests/parameter_conventions.rs`・`tests/anonymous_unions.rs` の例は、どれも演算子を使わない。`rg -n 'Eq<ref|Ord<ref|Add<ref' tests` は 0 件。確認済み）。`deref` を使う既存の書き方（`tests/fixtures/param_facts/Main.tz:12`、`tests/fixtures/text_class/Main.tz:40`、`std/Text.tt:10`〜11 など）は `deref a` の型が `T` なので A40 の対象にならず、変わらない。

## 変えてはいけない意味

- 今受理しているプログラムの型、型付き IR、LLVM IR、出力、トラップ。A40 は今 `E1003`・`E1005` になる式だけを受理に変える（D2）。
- 評価の順序（EVL-001: `a` の後に `b`）、短絡評価（`&&`・`||` の右は必要なときだけ評価する）、一時値の解放の順序。
- 借用の規則: 参照を通した読み出しは演算子の評価（呼び出し）が終わるまで続く。右の被演算子が参照先を書き換える・move する・排他借用するプログラムは、`deref a` と同じく `E1014`（P11）。
- `string + string`（所有する被演算子どうし）の消費と `@tz.string.append` によるバッファの再利用（NC03）。
- 総称の `ref 'a` の被演算子の型付け（呼び出し側へ制約を送る道。P10）。
- 整数の折り返し・丸め・NaN・符号付きゼロ。A40 は値を変える演算を足さない。

## 仕様

### 1. 対象の演算子と被演算子

- 対象の演算子（同じ演算の別名の綴りを含む）: 算術 `+`・`-`・`*`・`/`・`%`・`**`・単項の `-`、比較 `==`・`!=`・`<`・`<=`・`>`・`>=`、ビット `&&&`・`|||`・`^^^`・`<<<`・`>>>`・単項の `~~~`、論理 `&&`・`||`・`!`。
- 対象外: `|>`・`>>`・`<<`・`as`・`ref`・`deref`・添字・代入。NC20 の `op=` の右辺は NC20 の側で同じ規則に従う（D4）。
- 参照の被演算子: 解決した型が共有参照 `ref T` か排他参照 `ref mut T` で、`T` の頭が分かるもの。頭が分かるとは、`T` が型変数（`Type::Variable`）でも未解決の推論変数（`Type::Infer`）でもないこと（`Tensor<'a>`・`[?]` は頭が分かる）。`ref ref T` は 1 段だけ外して `ref T` にする。
- 被演算子の式の形は問わない（局所、引数、`ref x`、参照を返す呼び出しなど）。そのため `ref a + ref b` も、`a + b` と同じ意味で受理される。

### 2. 働く条件（D2）

A40 は、演算子の式の型付けの次の 3 つの段のどれかが今の道で誤りになり、その段で参照を 1 段外した型なら通るときだけ働く。どの段の試行も `Inference::unifies`（`src/polymorph.rs:720`）か `Classes::validate`（3676 行）で行い、状態を残さない。参照の型でない被演算子には、段 1 の (1-b)（兄弟が参照の被演算子のときの試行）のほかに試行を足さない。

1. **被演算子とヒント（決めるのを後に回す）**: 値の式（局所・フィールド・添字など）は `finish_expression`（13742 行）、呼び出しは `call_expression` で、被演算子の型とヒントを単一化する。演算子の被演算子として検査している（`binary_expression` と単項の型付けが印を立てる）ときは、この単一化を先に `Inference::unifies` で試し、通れば今どおり単一化する。ただし、ヒントが式の期待型で、それが未解決の推論変数であり、被演算子の型が参照の被演算子で、その変数がスコープにある局所の型にも未解決の制約にも現れないなら、試しが通ってもその場では単一化せず記録する（下の「確定と戻し方」）。現れないことは、局所の型ごとに `Inference::open_variables`（`src/polymorph.rs:738`）で確かめる（`let m = Some (a + b)` の `Some` の型引数の変数は現れない）。局所の型に現れる変数（`let g = \y -> y` の `g (a &&& first (g c) b)` の `g` の引数の型）は、今どおりその場で単一化する。記録すると、後の部分式（`g c`）がその変数を別の型に決め、確定しても戻しても今の道と違う結果になるからである。通らず、次の (1-a) か (1-b) が試しで通るときは、その場では単一化しない。(場所、ヒント、元の型) を記録し、被演算子の `TypedExpr` は元の型のまま返す。どちらも通らなければ、今どおり `same` を呼ぶ（記録した左の場所があれば、その前に記録した場所の `same` を報告する。下の「確定と戻し方」）。
   - (1-a) 被演算子の型が参照の被演算子 `ref T` で、外した型 `T` とヒントの単一化が通る。この被演算子を「外した」と記録し、`T` は兄弟とリテラルの被演算子へ渡すヒントにだけ使う。
   - (1-b) ヒントが兄弟の型で、その型が参照の被演算子 `ref S`（`S` の頭が分かる）であり、被演算子の型（被演算子自身が参照の被演算子なら外した型）と `S` の単一化が通る。兄弟も「外した」と記録する。P4 の `ref mut [i64] < ref [i64]`（`b` を外した `[i64]` と、`a` を外した `[i64]`）と、P16 の `ref i64` と `i64` の `a == b`・`a < b`・`let z = a + b`、読み取り引数の `string` と所有の `string` の `a == s`、`a == "x"`（文字列リテラル）はこれで通る。今の道が `E1003` を報告する場所でしか働かないので、受理しているプログラムは変わらない。参照の型でない被演算子に足す試行はこれだけである。
   - ヒントは式の期待型（二項は算術とビットだけ。単項の `-`・`~~~`・`!` は式の期待型をそのまま渡す）と兄弟の型。兄弟が外されていれば、兄弟の型は外した型を使う。接尾辞のない数値リテラル（`untyped_number`）の被演算子に渡す兄弟の型が参照の被演算子なら、今の規則のまま外した型を渡す（数値リテラルは参照の型を取れないので、今の道は必ず `E1003`）。`if`・`match`・ブロックなど、ヒントを中へ渡す形の被演算子には働かない（対象外）。
2. **`binary_type` の `same`**: 右と左の型の単一化（17012 行）が失敗し、参照の被演算子の側を外せばできるとき、外した型で単一化する。`&&`・`||` では、左と `bool`、右と `bool` の単一化をそれぞれ試し、失敗した側が参照の被演算子で、外した型なら通るとき、その側を外す。両辺が `ref bool`（`def both :: ref bool -> ref bool -> bool` の `a && b`）では、段 1 で右が兄弟のヒント `ref bool` のまま通り、右と左の単一化も `ref bool` どうしで通る。左だけを外すと右が `ref bool` のまま残り、`short_circuit`（`src/llvm.rs:8037`）の `phi i1` に参照が入るので、両側を試す。段 1 ですでに外した被演算子は 2 度外さない（1 段だけ。仕様 1）。外しても `bool` にならない側があれば働かない（edition 2026 の `let r = &x` / `let rr = &r` の `r && rr` は、段 1 で `rr` を `ref bool` に外すが、今と同じ `E1003`）。`&&`・`||` は段 3 を通らないので、この規則をここに置く。単項の `!` は、被演算子と `bool` の単一化（14479 行）に同じ規則を当てる。単項は式の期待型を `!` でもそのまま被演算子へ渡す（14477 行）ので、期待型がある `!a`（`def f :: ref bool -> bool` の `fn f a = !a`）は、被演算子とヒントの単一化で先に失敗する（P6 の位置。HEAD で `a` の位置の `E1003`）。この形は段 1 の (1-a) で外す。
3. **クラスの検査**: `require(binary_class(op), 左の型)`（単項は `Neg`・`Bits`）が、左の型 `ref T` のままでは今の道で `E1005` になり（`Classes::validate` で試す）、参照を外した `T` ならインスタンスがあるとき、`T` で `require` する。今の道でインスタンスがある式（利用者の `instance Eq<ref Foo>` など）は今のまま。段 2 の後は両辺が同じ型なので、型が参照の被演算子である被演算子は、左だけでなく両方を外す（下げ方の仕様 3 は外した被演算子のそれぞれに効く）。段 1 ですでに外した被演算子は 2 度外さない。外した後に両辺の型が同じにならなければ働かない（edition 2026 の `rr = &r` の `r == rr` は今と同じ `E1003`）。

- 外した後の段も今の道と同じ規則で型付けする。NC19 が先に入っていれば、外した型で NC19 の解決 (a)〜(d) を行う（D4）。
- **確定と戻し方**: 被演算子を（二項では両方を）検査した後、段 2 と段 3 を外した型で `Inference::unifies`・`Classes::validate` で試す。式の結果の型と式の期待型の単一化（`finish_binary` と単項の後の `finish_expression`。13742 行の `same`）も、外した結果の型で `Inference::unifies` で試す。式の期待型が、スコープにある局所の型にも未解決の制約にも現れない未解決の推論変数のとき（段 1。`let m = Some (a + b)`。`Some` の引数の期待型は新しい推論変数で、`argument` がそのまま渡す。`src/check.rs:15143`）は、段 1 は参照の被演算子とその変数をその場で単一化せず、(場所、ヒント、元の型) を記録する（外して確定すれば変数は `T` に、外さなければ `ref T` に決まる）。式の期待型が参照の型のとき（`x: ref i64` の `[x, a + b]`・`[x, -a]`、`x: ref bool` の `[x, !a]`）は、外した結果の型が合わないので働かず、今の診断（HEAD で `E1005` の `no instance for Add<ref i64>`・`Neg<ref i64>`、`E1003` の `expected bool, found ref bool`）を出す。ただし、演算子の式がほかの演算子の被演算子のとき（`a == b + c` の `b + c`、`b + c == a` の `b + c`）は、入れ子の演算子の式の木（被演算子として検査している演算子の式をたどった、いちばん外の演算子の式までの全体）を 1 つの単位にする。木の中の演算子の式は自分では確定せず、記録と試しをいちばん外の演算子の式へ集める。内の式の結果の型と期待型の単一化は外の演算子の段 1 そのものである（`finish_binary` の後の `finish_expression` が、外の被演算子とヒントの単一化になる）ので、その試しには外の段 1 の (1-a)・(1-b) を当てる。外の演算子から来たヒントは、木のどの深さでも兄弟のヒントとして扱い、(1-b) を当てる。外の兄弟のヒント `ref S` と、それを算術・ビットの左の被演算子、数値リテラルが左にある形の右の被演算子、単項の被演算子へそのまま渡したものがこれに当たる（`a`・`b`・`c`・`d` が参照の被演算子、`n` が `i64`、`s` が所有の `string` の `a == b + c`・`a == b + "x"`・`a < b + 1`・`a == -b`・`a && !b`・`let z = a + (b + c)`・`a < n + 1`・`a == n + b`・`a == s + "x"`・`a == b + c + d`・`a == 1 + (b + c)`・`a == -(b + c)`）。外した内の式の結果の型と、(1-b) で記録した被演算子の型は、後に検査する兄弟へのヒントにも使う。このヒントが今の道のヒント（前の被演算子が今の道で誤りなら外の演算子のヒント、それもなければ `Error`。A40 が記録した被演算子は、今の道では誤りである）と違うとき、兄弟の段 1 の単一化は、試しが通ってもその場では行わず記録する（S3）。兄弟が値の式（局所・フィールド・引数など）・リテラル・演算子の式のどれでもないときは、戻せないので、その兄弟を検査する前に木を戻し、兄弟を今の道のヒントで検査する（A40 はこの木で働かない。S7 の絞った形）。呼び出しは、引数を検査する前に結果の型とヒントを単一化し、引数をその型で検査する（`src/check.rs:14083`〜14100）。ラムダや `if` なども、中の検査がヒントで変わる。いちばん外の演算子の式は、木の記録と試しがすべて通ったときだけ全体を確定し、どれかが通らなければ全体を戻す。戻すときは記録をソースの順に戻し、内の式の記録は、その式を被演算子とする外の記録より先に戻す（今の道の診断は内の式から出る）。戻すときは、記録した場所のすべてで `same(元の型, 今の道のヒント, 場所の範囲)` を呼び直す。今の道のヒントは、前の被演算子を戻した後に今の道が渡すものである。兄弟の型は `sibling_hint`（13646 行）で、前の被演算子が今の道で誤りなら外の演算子のヒント、つまり算術・ビットの式の期待型、それもなければ `Error` になる。記録したヒントと違うことがある（`b + c == a` の `a` は `Error`、`a == n + b` の `b` は `ref i64` で、どちらも診断を出さない）。今の道と違うヒントで検査したリテラルの被演算子（`a < n + 1` の `1`、`a == s + "x"` の `"x"`）は、試しが通っていても、今の道のヒントで検査し直す（葉なので副作用がない）。今の道の診断をこの規則で同じに保てない入れ子の形が見つかったら、S7 に従う。段 1 の記録と、段 2・3 と結果の型の試しがすべて通ったときだけ確定する。記録した各場所で `T ~ ヒント`（(1-b) は被演算子の型 ~ `S`）を本物の `same` で単一化し、それから本物の `same`・`require` を呼ぶ。確定の前には単一化も制約も残さない（S3）。
- どこかが通らなければ、A40 は働かず、今の道の診断（同じ式から出るすべてを同じ順序で。コード・位置・文言・help）をそのまま出す（D5）。記録した場所ごとに、ソースの順に今の `same(元の型, 今の道のヒント, 場所の範囲)` を呼ぶ（今の `E1003`、help、回復）。今の道と違うヒントで検査したリテラルの被演算子（数値・文字列）は、今の道のヒントで検査し直す（葉なので副作用がない）。その後は今の道が続ける処理（`binary_type` など）をそのまま通る。ほかの被演算子の式は検査し直さない（クロージャ・新しい推論変数・部分式の診断を作り直すことになり、副作用がない形にできない）。右の被演算子が参照と関係のない理由で失敗し、左の場所が記録されたままなら、左の場所の `same` を先に報告する（P17 の後者の順序）。
- 参照先の頭が型変数か未解決の推論変数なら働かず、今どおり参照の型の制約を呼び出し側へ送る（P10）。後で推論変数が具体的な型に決まっても、A40 は働かない。

### 3. 型と下げ方（D3）

外した被演算子の型は `T`。式の型は今の規則のまま（比較は `bool`、ほかは左の型。左が外されていれば `T`）。下げ方は演算子の種類で決まる。

| 演算子の種類 | 参照の被演算子 `a` の渡し方 | 所有権 | IR |
| --- | --- | --- | --- |
| 被演算子を借りる演算子で、`operator_call` に下がるもの（組み込みでない型の `Eq`・`Ord`・算術クラス、配列・リスト・タプルの構造的な `Eq`・`Ord`（`src/polymorph.rs:3533` の `structural`）、NC19 の後は OEI） | `Borrow(Dereference(a), false)`（呼び出しが参照を `ref T` の仮引数へ渡すときと同じ木）。`BorrowOperand` は付けない | 参照先の共有の読み出しで、呼び出しが終わるまで続く。`ref mut T` は共有に借り直す | 参照先の番地をそのまま渡す。複製しない |
| 組み込みの命令の比較（スカラー・`string`・`utf8string` など、`intrinsic`（`src/polymorph.rs:3648`。表は 7361 行の `builtin_instance_by_type`）が真の `Eq`・`Ord`） | `Dereference(a)`（`deref a` と同じ木） | `E::Binary` の比較は読み出し（今の `deref a == deref b` と同じ。P8） | `deref a == deref b` と同じ |
| 値を取る組み込みの命令（スカラー・SIMD の算術とビット、`bool` の論理） | `Dereference(a)`（`deref a` と同じ木） | `deref a` と同じ読み出し。これらの型は Copy | `deref a + deref b` と同じ |
| 組み込みでない型の `Bits` のインスタンス（値を取る） | `Dereference(a)` | `deref a` と同じ。型が Copy でなければ `deref a` と同じ `E1012`（`cannot move a non-Copy value out of a reference`。P12） | 値を渡す |
| `string`・`utf8string` の `+` | 消費しない被演算子（D3 の印） | 参照の被演算子は読み出し（`Use::Read`）で、move しない。所有する被演算子は今どおり消費する | 3 つの道（`src/llvm.rs:8060` の `binary`、`src/llvm_frame.rs:419` の `frame_concat`（`concat_into`）、441 行の `concat_length`（`src/llvm.rs:6586` の `StringLength` の分岐から））のどれでも、参照の被演算子は `read_operand` で読んで `release_operand` で放し、`take_operand`・`drop_framed` に渡さない。左が参照の被演算子なら `append` に渡さない（結果が外へ出れば `@{runtime}.concat`、フレームの結果なら `concat_into`）。左が所有する文字列で右だけが参照なら、今どおり `append`・`concat`・`concat_into` のどれか（右は解放しない）。参照先の複製は作らない |

- 評価の順序: 今と同じく左、右の順に評価する。読み出しは演算子の評価が終わるまで続く。右の被演算子が参照先を書き換える・move する・排他借用するなら `E1014`（`deref a` と同じ。P11）。
- 一時値: 参照を返す呼び出しの被演算子の一時値は、今と同じ規則で解放する（演算子の後に左、右の順）。

### 4. edition・native と WASM

- 全 edition で同じ。今 `E1003`・`E1005` になる場所だけを受理に変える拡張なので、edition を使わない（`docs/stability.md` 7 行。受理を増やす拡張は同じ edition で入れてよい）。2026 では明示の `ref T` と `pure def` の借用の引数に効き、2027 では既定の読み取り引数にも効く。
- 解析と下げ方だけの変更で、native と WASM の差はない。実行時の値・トラップは変えない。

### 5. 診断

新しい診断コードと kind は使わない（D5）。

| 条件 | コード | 位置 | メッセージ |
| --- | --- | --- | --- |
| A40 が働いた後の段が失敗した | 今の道の診断（同じ式から出るすべてを同じ順序で。`E1003`・`E1005`） | 今と同じ | 今と同じ |
| 組み込みでない型の `Bits` で、参照先が Copy でない | `E1012`（既存） | 被演算子 | `cannot move a non-Copy value out of a reference`（`deref a` と同じ） |
| 右の被演算子が参照先を書き換える・move する・排他借用する | `E1014`（既存） | 衝突したアクセス。note は参照の借用 | 既存のまま |

### 6. 例（実装後の意味）

```text
def greet :: string -> string -> string
fn greet a b = a + b                 // 連結。a と b は消費しない。確保は結果の 1 回

def same :: string -> string -> bool
fn same a b = a == b                 // Eq<string>（組み込みの比較）

def add2 :: BigInt -> BigInt -> BigInt
fn add2 a b = a + b + 1              // Add<BigInt> に参照をそのまま貸す。1 は BigInt のリテラル

def lt :: ref mut [i64] -> ref [i64] -> bool
fn lt a b = a < b                    // a は共有に借り直す

def gen :: ref 'a -> ref 'a -> bool
fn gen a b = a == b                  // 今のまま。Eq<ref 'a> を呼び出し側へ送る
```

## 設計

### 段ごとの変更

| 段 | ファイル・関数 | 変更 |
| --- | --- | --- |
| 1 | `src/check.rs` に補助関数 | `reference_operand(ty) -> Option<Type>`: 解決した型が `ref T`・`ref mut T` で `T` の頭が分かれば `T` を返す（1 段だけ）。NC19・NC20 も使う（他チケットへ提供するインターフェース） |
| 2 | `src/check.rs` の `finish_expression`（13742 行）・`call_expression`（ヒントと結果の型の単一化） | 仕様 2 の段 1。演算子の被演算子として検査している（`binary_expression` と単項の型付けが印を立てる）ときだけ、`Inference::unifies` で今の単一化を試す。失敗し (1-a)（外した `T` とヒント）か (1-b)（兄弟の `ref S` の `S` と、被演算子の型か外した型）が試しで通るとき、その場では単一化せず、(場所、ヒント、元の型) を記録し、外した被演算子（(1-b) では兄弟も）を記録する。被演算子の `TypedExpr` は元の型のまま返す |
| 3 | `src/check.rs` の `binary_expression`（14209 行）と単項（14476 行付近） | 被演算子に印を立てて検査する。兄弟の型のヒントに、外した型（外した内の式の結果の型を含む）を使う。数値リテラルの被演算子のヒントも同じ。外の兄弟のヒントを算術・ビットの左の被演算子、数値リテラルが左にある形の右の被演算子、単項の被演算子へ渡すときは、兄弟のヒントだという印も渡す。(1-b) が兄弟（外の演算子の被演算子のこともある）を外したら、その兄弟を木の記録に足す。演算子の被演算子として検査している演算子の式は確定せず、記録と試しをいちばん外の演算子の式へ渡す。今の道と違うヒントを受ける兄弟が値の式・リテラル・演算子の式でなければ、その前に木を戻す。いちばん外の式が確定した後に、外した被演算子（両方のこともある）を段 5 の形に包む |
| 4 | `src/check.rs` の `binary_type`（17001 行）と、単項の `!` の `same`（14479 行）・`-`・`~~~` の `require` | 仕様 2 の段 2・3 と「確定と戻し方」。`same` は `Inference::unifies` で、`require` は `Classes::validate` で今の道を試し、失敗し外した型で通るときだけ外す。いちばん外の演算子の式で、木の段 1 の記録と段 2・3 と結果の型の試しがすべて通ったときだけ、記録した場所の単一化と本物の `same`・`require` で木の全体を確定する（内の式は自分では確定しない）。通らなければ木の全体を戻す: 記録した場所の `same` をソースの順に（内の式の記録を、その式を被演算子とする外の記録より先に）今の道のヒントで呼び、今の道と違うヒントで検査したリテラルを今のヒントで検査し直してから、今の道を続ける（今の道の診断をすべて同じ順序で出す。D5） |
| 5 | `src/check.rs` の `TypedExprKind`（5421 行）と型付き IR を走査する各所 | 消費しない被演算子の印（D3）。値を取る組み込みの命令と組み込みの比較では印を使わず `Dereference(a)` にする（`deref a` と同じ木なので、型付き IR と LLVM IR が `deref a` の形と同じになる） |
| 6 | `src/polymorph.rs` の演算子の下げ方（6557〜6585 行）と `operator_call`（6678 行） | 印の付いた被演算子を、借りるクラスでは `Borrow(Dereference(a), false)` の引数にし（`BorrowOperand` で包まない）、`Bits` では `Dereference(a)` にする。組み込みの命令に決まった演算子（`intrinsic` が真）では、`string` の `+` を除いて `Dereference(a)` に戻す |
| 7 | `src/ownership.rs` の `E::Binary`（15095 行） | 印の付いた被演算子は、`string` の `+` でも読み出し（`Use::Read`）にする。`Dereference(a)` の扱いは今のまま |
| 8 | `src/llvm.rs` の `binary`（8060 行）、`src/llvm_frame.rs` の `frame_concat`（419 行）と `concat_length`（441 行。`src/llvm.rs:6586` の `StringLength` の分岐から呼ばれる） | `string`・`utf8string` の `+` で、3 つの道のどれでも、印の付いた被演算子を `read_operand`（`src/llvm_frame.rs:636`）で読み、`release_operand`（678 行）で放す。`take_operand` と `drop_framed` には渡さない。左が印の付いた被演算子なら `append` に渡さない（`binary` は `concat`、`frame_concat` は `concat_into` のまま）。コンパクト文字列でも同じ関数名なので分岐は要らない見込み |

### 生成 IR の形

- 値を取る組み込みの命令: `Dereference(a)` は `deref a` と同じ木なので、`fn f a = a + 1`（`a: ref i64`）と `fn f a = deref a + 1` の IR は同じになる（-O0・-O3）。
- 借りる演算子: `Borrow(Dereference(a), false)` は `self.place(value)`（`src/llvm.rs:6183`〜6187）で場所 `deref a` の番地、つまり `a` の値を渡す。今の回避 `deref a + deref b`（`BigInt`。P8）の `BorrowOperand(Dereference(a))` も `operand_borrow` の `self.place(operand)`（7501 行以降）で同じ番地を渡すので、IR は同じになる見込み。手順 4 で比べ、違えば差を PR に書く。
- `string` の `+`（参照の被演算子どうし）: `append` を呼ばず、参照先を解放しない。結果が外へ出れば `@{runtime}.concat` を 1 回、フレームの結果（`let s = a + b` の `s` が外へ出ない）なら `concat_into` を 1 回呼ぶ。`(a + b).length` は連結の関数を呼ばない。文字列の複製の呼び出しはない。

## 前提とする他チケットのインターフェース

- [A11](_completed/A11-borrowed-comparisons.md)・[NC03](_completed/NC03-borrowed-arithmetic-operators.md)（完了）: `operator_call` が `Eq`・`Ord`・組み込みでない型の算術の引数を共有借用で渡すこと。
- [A18](_completed/A18-parameter-conventions.md)（完了）: edition 2027 の既定の読み取り引数と `read_value_parameters`。
- [NC19](NC19-operator-extension-instances.md) との順序（NC19 の D9 (ii) の決定）: A40 が先なら、NC19 は外した型で (a)〜(d) を引く（NC19 の解決の前に段 1〜4 の試行を置く）。NC19 が先なら、A40 は NC19 の (a)〜(d) を含む今の道を試行の対象にし、OEI の呼び出しにも参照をそのまま貸す。どちらの場合も、OEI の被演算子の渡し方は仕様 3 の「借りる演算子」の行。
- [NC20](NC20-compound-assignment-and-mutable-index-views.md) との順序: `op=` の右辺の参照の被演算子は A40 の規則に従う。後に入る方が合わせる。組み込み型の `p op= e` は `p = p op e` と同じ型付き IR に下がるので、右辺 `e` が参照の被演算子なら、A40 の規則で外す。`AddAssign` 系の右辺（`ref Rhs`）には参照をそのまま貸す。

## 他チケットへ提供するインターフェース

- `reference_operand(ty)`（段 1）と、被演算子を外した印・下げ方（段 5・6）。NC19 の (a)〜(d) と NC20 の `op=` の右辺が使う。
- 回帰テスト `tests/reference_operands.rs`。NC19 は OEI の被演算子が参照の形（`Tensor` の読み取り引数どうしの `a - b`）を、NC20 は `s += r`（`r: ref string`）を同じファイルに足す。

## 実装手順

1. **変更**: なし（`target/scratch/a40/` に記録）。**内容**: P1〜P17 を dev バイナリで確かめる。ベースラインとして、`tests/borrowed_arithmetic.rs` の `emits` の例、`tests/fixtures/typeclasses`・`tests/fixtures/strings`・`tests/fixtures/borrowed_comparisons`・`tests/fixtures/conventions`・`tests/fixtures/param_facts` を `tsuzuri build --emit llvm`（native と `--target wasm32`、`-O0`・`-O3`）で IR にして保存する。**確認**: 「現状」の表と同じ診断と出力。
2. **変更**: `tests/reference_operands.rs`（新規）。**内容**: テスト計画の Rust テストを足す。受理のテストは今は失敗し、今と同じ結果を確かめるテスト（P9〜P12、総称の道）は通る。**確認**: `node scripts/slot.mjs -- cargo nextest run --locked --test reference_operands` で、足した受理のテストだけが失敗する。
3. **変更**: `src/check.rs`（段 1〜4）。**内容**: 試行と外し方。この段では型付けだけを変え、下げ方は `Dereference(a)` に置く（`string` の `+` は段 5 まで今の道の診断のまま）。**確認**: T0。スカラー・`bool`・比較の受理のテストが通る。手順 1 の IR と `diff` して一致する（S1）。
4. **変更**: `src/check.rs`・`src/polymorph.rs`（段 5・6）。**内容**: 借りる演算子に参照をそのまま貸す。`BigInt`・非 Copy のレコード・`Tensor` の比較の受理のテストを通す。`BigInt` の `a + b` と `deref a + deref b` の IR を比べる（設計の「生成 IR の形」）。**確認**: T0、`node scripts/slot.mjs -- cargo nextest run --locked --test reference_operands --test borrowed_arithmetic --test polymorphism --test parameter_conventions --test pure_functions`。
5. **変更**: `src/ownership.rs`・`src/llvm.rs`・`src/llvm_frame.rs`（段 7・8）。**内容**: `string`・`utf8string` の消費しない連結。**確認**: T0、`string` の受理のテスト、`node scripts/slot.mjs -- cargo nextest run --locked --test strings`、`TSUZURI_STRINGS=compact node scripts/slot.mjs -- cargo nextest run --locked --test reference_operands --test strings`、T1。
6. **変更**: `tests/fixtures/reference_operands/`（新規）、`tests/features.mjs`、`spec/rules/TYP.md`・`tests/conformance/TYP/`、文書。**内容**: E2E、TYP の規則（番号は実装の時点の TYP の次の空き）と適合性テスト、ドキュメントの更新。**確認**: T2（テスト計画の検証コマンド）。

## テスト計画

### Rust テスト（`tests/reference_operands.rs`。新規）

解析は native と WASM で共通なので、拒否のテストは `analyze` の 1 回で両方を覆う。受理のテストは `emits`（native と WASM の IR、決定性、宣言の重複なし。`tests/borrowed_arithmetic.rs:9` と同じ形）と、実行の出力で確かめる。edition を選ぶテストは `tests/parameter_conventions.rs:8` の `check(edition, source)` と同じ形にする。

- `string_operands_borrow_their_references`: P1 の `a + b` が通り、`greet "ab" "cd"` が `abcd`。P2 の `a == b` が通る。`utf8string` も同じ。`a + "!"`・`"<" + a`（参照と所有の混在）も通る。
- `string_concatenation_does_not_consume_references`: 読み取り引数の `a`・`b` で、`let s = a + b` の後に `a` と `b` を読める。IR に、左の引数が参照先である `@tz.string.append` の呼び出しも、参照先の解放もない。結果が外へ出る形（`fn greet a b = a + b`）は `concat` を 1 回、フレームの結果の形は `concat_into` を 1 回呼ぶ。`(a + b).length` と、外へ出ない `let s = a + b` / `s.length + a.length + b.length` が通り、正しい値を出す（`frame_concat`・`concat_length` の道）。`TSUZURI_STRINGS=compact` でも同じ（実装手順 5 の確認）。`let s = "ab"` / `let r = ref s` / `r + "c"` の後に `s` を使える。P9（所有する `s` の `s + "c"` の後の `s`）は今と同じ `E1012`。
- `bigint_operands_borrow_their_references`: P3 の `a + b`・`-a`・`a + 1`・`a < b` が通り、値が正しい。`a + b` の IR が `deref a + deref b` の IR と同じ（設計の「生成 IR の形」。違えば差を PR に書く）。
- `record_operands_stay_usable`: P5 の `a + b` が通り、その後に `a.n + b.n` を読める。呼び出し側の実引数も使える。
- `array_and_mutable_reference_operands`: P4 の `ref mut [i64] < ref [i64]` と `ref [i64]` どうしの `<` が通る。`ref mut i64` どうしの `a + b` が通る。
- `mixed_reference_and_value_operands`: `ref i64` と `i64` の `a + b`、`a + 1`、`1 + a`、`ref BigInt` と `BigInt` の `a + b`、`let x: i64 = 3` / `let r = ref x` / `r + 1`（P6・P15）が通る。式の期待型がない形（P16）も通る: 読み取り引数の `string` と所有の `string` の `a == s` と `s == a`、`a == "x"`、`ref i64` と `i64` の `a == b` と `a < b`、`let z = a + b`（`ref i64`、`i64`）。式の期待型が未解決の推論変数の形も通る: `let m = Some (a + b)`・`Some (-a)`（`ref i64`）、`Some (!a)`（`ref bool`）。右の被演算子が演算子の式の形も通る: `a == b + c`（`BigInt`）、`a == b + "x"`（`string`）、`a < b + 1`・`a == -b`・`let z = a + (b + c)`（`ref i64`）、`a && !b`（`ref bool`）。外の兄弟のヒントを期待型に受ける形も通る: `a < n + 1`・`a == n + b`（`a`・`b` は `ref i64`、`n` は `i64`）、`a == s + "x"`（所有の `s`）、`a == b + c + d`・`a == 1 + (b + c)`・`a == -(b + c)`（`ref i64`）。HEAD ではそれぞれ `n` と `1` の `E1003`、`n` の `E1003`、`s` と `"x"` の `E1003`、`b + c` の `E1005`、`1` の `E1003` と `b + c` の `E1005`、`b + c` の `E1005`。
- `scalar_reference_operands_match_deref`: `def f :: ref i64 -> i64` の `fn f a = a + 1` と `fn f a = deref a + 1` の IR が同じ（native と WASM、`-O0`・`-O3` の `tsuzuri build --emit llvm`）。`&&&`・`<<<`・単項の `~~~` と `-`、`ref bool` の `!a`・`a && b`・`a || b` も通る（両辺が `ref bool` の `a && b`・`a || b` は、型付き IR で両方の被演算子が外した `bool` になり、右が `ref bool` のまま残らない。左だけ・右だけが `ref bool` の形も通る。期待型がある `!a`（`def f :: ref bool -> bool` の `fn f a = !a`）と、期待型のない `let c = !a` も通る）。`a && b` の右の被演算子が、左が偽のとき評価されない（短絡評価）。
- `tensor_operands`: P7 の `Tensor<i64>` の引数の `a == b` が通る（`Eq<Tensor<'a>>`、`std/Tensor.tz:1519`）。`Tensor` の算術は NC06 の後に足す（NC06 が先に入っていれば、ここで `a + b` も確かめる）。
- `explicit_borrow_operands`: `ref a + ref b`（`a`・`b`: `BigInt`）が `a + b` と同じ値になる。
- `user_reference_instances_keep_today_path`: `record Foo` に `instance Eq<Foo>`（`n` を比べる）と `instance Eq<ref Foo>`（`tag` を比べる）があるとき、`ref Foo` の引数の `a == b` は今どおり `Eq<ref Foo>` を選ぶ（出力で見分ける）。`Add<ref Foo>` は A37 の後は `orphan_instance` なので、テストに使わない。
- `generic_reference_operands_keep_today_path`: P10。総称の `gen` が通り、`Eq<ref Foo>` のプログラムが `true`、`i32` の呼び出しが呼び出しの位置で `E1005`。`std/Text.tt` の `earlier`（`deref left < deref right`）の例が今どおり通る。
- `reference_operands_keep_borrow_conflicts`: P11 の 2 つの形を `deref` なしで書く（`r + bump (ref mut x)`、`string` の `r == take s` など、右の被演算子が参照先を書き換える・move する）と `E1014`。
- `non_copy_bits_operands_match_deref`: 非 Copy の利用者のレコードに `instance Bits<Foo>` を置き、`Foo` の読み取り引数の `a &&& b` が、`deref a &&& deref b` と同じ `E1012` になる。Copy のレコードなら通る。
- `nested_reference_strips_one_level`: 型注釈で作った `ref (ref i64)` の `rr + 1` が、1 段外した `ref i64 + 1` でも型付けできないので、今と同じ診断になる（D5）。`check("2026", …)` の `let x: i64 = 3` / `let r = ref x` / `let rr = &r` / `rr + 1` も今と同じ `E1003`（`expected ref ref i64, found i32`。P15）。`check("2026", …)` の `let r = &x` / `let rr = &r` / `r && rr`（`x: bool`）と `r == rr`（`x: i64`）も、今と同じ `E1003`（位置は `rr`）。
- `failed_strip_keeps_today_diagnostic`: `record Foo` に `Add` がないとき（P17）、`def plus :: Foo -> Foo -> Foo` の `a + b` は今と同じ 2 つの `E1003`（3:15 の `a` と 3:19 の `b`、今のメッセージ）を、この順で出す。`def plus :: Foo -> i64 -> Foo` は、`a` の `E1003` の後に `b` の `expected Main.Foo, found i64` を出す。`Eq` のない型の `a == b` は今と同じ `E1005`。式の期待型が参照の型の `[x, a + b]`・`[x, -a]`（`x: ref i64`）と `[x, !a]`（`x: ref bool`）は、今と同じ診断（`E1005` の `no instance for Add<ref i64>`・`Neg<ref i64>`、`E1003` の `expected bool, found ref bool`）。期待型の推論変数が局所の型に現れる形（`let g = \y -> y` の `(g (a &&& first (g c) b)).n`。`record Foo { n: i64 }`、`def f :: ref Foo -> ref Foo -> Foo -> i64` の `a`・`b`・`c`、`def first :: own 'a -> own 'b -> 'a`）は、`Foo` に `Bits` がなければ今と同じ `E1005`（`no instance for Bits<ref Main.Foo>`。位置は `a &&& first (g c) b`）を出す。`instance Bits<ref Foo>`（2 引数のメソッドは左の引数を、`bit_not` は引数を返す）があれば、今どおり受理して `a.n`（`a.n` が 4 なら `4`）を出す（`Bits` は A37 の orphan 規則の表にないので、`Add<ref Foo>` の代わりに使う）。外の演算子が戻す入れ子の形（`Foo` に `Add` があって `Eq` がないときの `a == b + c`）は、今と同じ診断を同じ順に出す。左の被演算子が演算子の式の `b + c == a`（`Add` があって `Eq` がない）は、今と同じ診断（HEAD で `b + c` の位置の `E1005` の 1 つ）を出す。結果の型が `Foo` の `def` の `a * (b + c)`・`(b + c) * a`（`Add` があって `Mul` がない）も、今と同じ診断（HEAD で `a`・`b`・`c` の 3 つの `E1003`）を出す。`def pick :: own 'a -> 'a` の `b + c == pick 1`（`Add` があって `Eq` がない）は、今と同じ `b + c` の `E1005` の 1 つだけを出す（`pick 1` の `1` に `E1003` を出さない）。`Foo` に `Add` があって `Eq` がないときの `a == n + b`（`n` は所有の `Foo`）は、今と同じ `n` の `E1003` の 1 つだけを出す（`b` に `E1003` を足さない）。結果の型が `string` の `def` の `a < n + 1`（`a` は `ref i64`、`n` は `i64`）は、今と同じ `n` と `1` の 2 つの `E1003` を出す。
- edition: `check("2026", …)` で、明示の `ref string` の `a == b` と `pure def` の `string` の引数の `a + b` が通る。`check("2027", …)` で P1 が通る。

### E2E（`tests/features.mjs` の新しい suite `reference_operands`、native と WASM の `-O0`／`-O3`、`live == 0`）

`tests/fixtures/reference_operands/`（`Tsuzuri.toml` で edition 2027。`tests/fixtures/conventions` と同じ形）に、次を `i64` で返す関数を置く。

- `text_concat`: 読み取り引数の `a + b` と `a == b` を使い、結果の長さと、後で `a`・`b` を読んだ長さの和を返す。呼び出しごとに確保が残らない（`live == 0`）。
- `text_concat_loop n`: `n` 回（0・1・10000）連結と比較を繰り返す。参照先を解放しないこと（解放後の使用がないこと）と、確保が残らないことを確かめる。
- `text_concat_frame`: 読み取り引数の `(a + b).length` と、外へ出ない `let s = a + b` / `s.length + a.length + b.length` の和を返す（`concat_length`・`frame_concat` の道）。
- `bigint_sum`: `BigInt` の読み取り引数の `a + b * a - b` と `a < b`。
- `record_sum`: 非 Copy のレコードの `a + b` の後に `a` と `b` を読む。
- `scalar_refs`: `ref i64` の被演算子の算術・ビット・比較と、`ref bool` の論理。

### 既存テストへの影響

- 期待値の変更はない見込み（「現状」の最後の段落）。
- `tests/diagnostic_baseline/` には、参照の被演算子の演算子の誤りを固定したものはない（`rg -c 'found ref|Eq<ref|Ord<ref|read-only by default' tests/diagnostic_baseline` が 0 件。確認済み）。手順 1 でもう一度確かめ、見つかれば PR に書いて直す。

### 性能

`deref` の読み出しがなくなるだけで、新しい確保・複製はない。`string` の連結は、参照の被演算子どうしで `concat` の 1 回の確保（今の `own` を使う書き方では、呼び出し側が実引数を手放すか複製する）。性能は主張しない。

### 検証コマンド

- T0: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`node scripts/slot.mjs -- cargo nextest run --locked --test reference_operands --test borrowed_arithmetic --test strings`。
- T1: `node scripts/slot.mjs -- cargo nextest run --locked`。
- T2: T1、`cargo build`、`node tests/features.mjs target/debug/tsuzuri reference_operands`、同じく `typeclasses`・`conventions`・`borrowed_comparisons`・`param_facts`（suite は 1 回に 1 つ）、`node tests/strings.mjs target/debug/tsuzuri`、`node tests/compact_strings_modes.mjs target/debug/tsuzuri quick compact utf16`（`string` の演算子を変えたため。AGENTS.md）、`TSUZURI_STRINGS=compact node tests/features.mjs target/debug/tsuzuri reference_operands`、`node scripts/conformance.mjs target/debug/tsuzuri --rule <足した TYP の規則>`、`node scripts/check-spec.mjs --require-markers`、`node scripts/check-docs.mjs <変更したページ>`。

## ドキュメント

### 修正までの注記（任意）

A40 より前に、別の PR で `functions.md` の「引数の所有権（`own` と edition 2027）」に既知の制限を置く場合の文案（リンクは `functions.md` からの相対パスなので、コードブロックに入れる）:

```markdown
edition 2027 の既定の読み取り引数（`string`・`BigInt`・非 Copy のレコードなど）を演算子の被演算子にすると `E1003`・`E1005` になります。`deref a == deref b` と書いてください。`string` の連結は `own` の引数か `$"{a}{b}"` で書きます。修正は [A40](../../../_features/A40-reference-operands.md) です。
```

置いたら `node scripts/check-docs.mjs` を通し、A40 の PR で消す。

### A40 の PR で直すページ

- `values-and-functions/functions.md`「引数の所有権（`own` と edition 2027）」（19 行）: 読み取り引数は演算子の被演算子にそのまま書けること（`a + b`・`a == b`・`-a`）、`string` の `+` は参照の被演算子を消費しないこと、`deref` の書き方も使えることを、例（`run=`）と一緒に書く。修正までの注記を置いていれば消す。「純粋関数（`pure def`）」の借用の引数の段落にも 1 文足す。
- `values-and-functions/op-and-expressions.md`「メモリとその他の演算子」（236 行）の後か「比較演算子と論理演算子」（150 行）の前に、参照の被演算子の段落を足す: 対象の演算子、参照を 1 段外すこと、借りる演算子は参照をそのまま借り、値を取る演算子は `deref` と同じ読み出しをすること、総称の `ref 'a` には働かないこと（`deref` が要る）。「評価順序」（252 行）の表は変えない。
- `docs/language.md`「演算子」（4432 行）: 同じ規則を 1 段落で書く。
- `spec/rules/TYP.md`: 規則を 1 行足す（番号は実装の時点の TYP の次の空き。HEAD の最後は `TYP-022`。A37 も TYP の規則を足すので、後に入る方が付け直す）。文案: `An operand of type ref T or ref mut T whose head of T is known MUST be typed as T when the operator does not type-check with the reference type (E1003, E1005); a borrowing operator MUST receive the reference, and a value-taking operator MUST read the referent as deref does.`。文書の参照先は `op-and-expressions.md`。`tests/conformance/TYP/` に適合性テストを足す。
- `std/Text.tt` と `built-in-types-and-modules/text.md`（98 行・114 行）の `deref left < deref right` は変えない（総称の `ref 's` なので A40 は働かない）。
- README: 書いてあること（特徴の一覧・検証手順）は変わらないので変更しない。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/values-and-functions/functions.md _tsuzuri/language-reference/values-and-functions/op-and-expressions.md` を通す。

## 受け入れ条件

- [ ] P1〜P7、P15 の前者（`ref r` の再借用）、P16 の形が受理され、期待の値を出す（native と WASM、`-O0`・`-O3`）。
- [ ] P9（所有する `string` の消費）、P10（総称の `ref 'a`）、P11（`E1014`）、P12（`E1012`）が今と同じ。利用者の `instance Eq<ref Foo>` が今どおり選ばれる。
- [ ] 段 1 のベースラインの IR が一致する（native と WASM、`-O0`・`-O3`）。`ref i64` の被演算子の IR が `deref` の形と同じ。
- [ ] `string`・`utf8string` の `+` の参照の被演算子が、3 つの道（`binary`・`frame_concat`・`concat_length`）のどれでも `append` に届かず、参照先を解放しない。参照の被演算子どうしの連結は、結果が外へ出れば `concat` の 1 回、フレームの結果なら `concat_into` の 1 回。`(a + b).length` と外へ出ない `let s = a + b` が通る。コンパクト文字列でも同じ。
- [ ] 外した後の型付けが失敗する式は、今の道の診断のまま（同じ式から出るすべてを同じ順序で。P17）。
- [ ] E2E の `reference_operands` が native と WASM の `-O0`／`-O3` で通り、`live == 0`。
- [ ] TYP の規則と適合性テストがある。`functions.md`・`op-and-expressions.md`・`docs/language.md` が更新されている。
- [ ] T2 が通る。

## 落とし穴

- 試行で状態を残さない。`same` や `require` を一度呼んでから失敗を拾うと、途中までの単一化か制約が残る。必ず `Inference::unifies` と `Classes::validate` で試し、通ると分かってから本物の `same`・`require` を呼ぶ（S3）。
- 参照の型でない被演算子に、段 1 の (1-b) のほかの試行を足さない。足すと、受理しているプログラムの推論の順序が変わりうる（S1）。(1-b) は兄弟が参照の被演算子で、今の道が `E1003` を報告する場所だけで働く。
- 段 1 で `T ~ ヒント` をその場で単一化しない。段 3 の失敗を後で知っても単一化は戻せず、今の道の 2 つ目以降の診断（P17）を出せなくなる。記録して、段 2・3 の試しの後に確定する（仕様 2 の「確定と戻し方」）。
- `string` の `+` の参照の被演算子を `Dereference(a)` のまま `take_operand` に渡さない。所有権検査は `E1012` を出し、コード生成は参照先を解放する（S2）。`binary` だけを直しても足りない。`frame_concat`（`let s = a + b` の `s` が外へ出ないとき）と `concat_length`（`(a + b).length`）も両方の被演算子を `take_operand` で取り、`drop_framed` で解放する。3 つの道のどれでも、印の付いた被演算子は `read_operand` と `release_operand` で扱う。所有する左の被演算子と参照の右の被演算子の組で、右を解放しないことも確かめる。
- 総称の `ref 'a` を外さない。今は `Eq<ref 'a>` を呼び出し側へ送り、利用者の `instance Eq<ref Foo>` を持つプログラムがこれで動いている（P10。S4）。
- 型が型変数に届く読み取り引数（`Tensor<'a>` など）は、本体ですでに値の view（`Dereference`）なので、A40 の対象にならない（P13）。二重に外さない。
- `ref ref T` は 1 段だけ外す。`ref r`（`r` が参照）は再借用で `ref T` になる。`ref ref T` は型注釈、総称の具体化、edition 2026 の記号の借用 `&r`（`tests/borrow_syntax.rs:310`〜317）で現れる（P15）。
- 比較と `&&`・`||` には式の期待型が渡らない（14243〜14254 行）。仕様 2 の段 1 は、これらでは兄弟の型のヒントにだけ働く。
- 利用者の `Add<ref Foo>` をテストに使わない。A37 の orphan 規則で `orphan_instance` になる。`Eq` は orphan 規則の表にないので `Eq<ref Foo>` を使う。

## 対象外

- 入れ子の演算子の式の木で、今の道と違うヒントを受ける兄弟が値の式・リテラル・演算子の式でない形（`b + c == pick 1` など。仕様 2 の「確定と戻し方」、S7）。今の道の診断のまま。
- 総称の `ref 'a` の被演算子（`std/Text.tt` の `deref left < deref right`）。互換を保つ直し方は別の問い（GUIDE D-66 の残る問い）。
- `if`・`match`・ブロックなど、ヒントを中へ渡す形の被演算子の中の参照（各枝のヒントは今のまま）。
- `|>`・`>>`・`<<`・`as`・添字・代入の被演算子。`op=` の右辺は NC20 が同じ規則にする。
- 参照の被演算子の診断の改善（外した型での `no instance for Add<Foo>` の表示など。D5）。
- 新しい演算子・新しいインスタンス。OEI は [NC19](NC19-operator-extension-instances.md)。

## 決定事項

### D1: NC19 の Phase 2 ではなく、別のチケットにする（承認済み。D-66 の委任で決定）

- 決定: NC19 の D9 (ii)（参照の被演算子を、借りる演算子へ直接貸す）を承認し、新しいチケット A40 に分ける。
- 理由: 参照の被演算子は OEI とも NC06 とも独立している。NC19 の Phase 2 に置くと、今必要な `string`・`BigInt`・利用者の型の関数が NC06 と NC19 を待つ。同じ型どうしの演算子も変わるので、1 チケット 1 PR に従って分ける。
- 採らなかった案: NC19 の Phase 2 にする、診断の help だけを直す（`deref` を勧める。最新の edition の最もよく書くコードに毎回 `deref` が残る）、何もしない。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D2: 参照先の頭が分かる型と、今の道が `E1003`・`E1005` を報告する場所に限る（承認済み。D-66 の委任で決定）

- 決定: A40 は、参照先の頭が分かり、今の道がその演算子の式（被演算子へのヒントを含む）で `E1003`・`E1005` を報告するときだけ働く（仕様 2）。参照先が型変数か未解決の推論変数なら働かない。
- 理由: 働く条件を今の道の誤りに結ぶと、受理していたプログラムの型と IR が変わらないことを構造で保証できる。参照先が型変数の被演算子を外すと、今受理しているプログラムを変える。今は `ref 'a` の `a == b` が参照の型の制約を呼び出し側へ送り、利用者の `instance Eq<ref Foo>` があるプログラムはこの制約で動いている（P10）。
- 状態: 承認済み（D-66 の委任で決定、2026-11-25）。

### D3: 下げ方は、借りる演算子に参照を貸し、値を取る演算子に `deref` の読み出しを渡す（既定案）

- 決定: 仕様 3 の表のとおり。借りる演算子（`operator_call` に下がる `Eq`・`Ord`・組み込みでない型の算術・OEI）には、呼び出しが参照を `ref T` の仮引数へ渡すときと同じ `Borrow(Dereference(a), false)` を渡し、`BorrowOperand` を付けない。組み込みの比較と値を取る組み込みの命令には `Dereference(a)`（`deref a` と同じ木）を渡す。組み込みでない型の `Bits` は値を取るので `Dereference(a)` で、Copy でなければ `deref a` と同じ `E1012`。`string`・`utf8string` の `+` は消費しない連結にする。
- 印の形: 型付き IR に、消費しない被演算子の印を足す。新しい種類（例 `TypedExprKind::ReferenceOperand(Box<TypedExpr>)`。型は `T`、子は参照の式）にするか、既存の種類の印にするかは実装者が選ぶ。条件は、(1) 今受理しているプログラムの型付き IR に現れない、(2) 所有権検査とコード生成が `deref a` と区別できる、(3) 生成 IR が決定的。値を取る組み込みの命令と組み込みの比較では印を使わず `Dereference(a)` にする。
- 理由: 参照は演算子の評価の間だけ借りればよく、複製も move も要らない。呼び出しと同じ木を使えば、所有権検査の規則を足さずに済む。`Dereference(a)` を使う道は、今の `deref a` と同じ型付き IR になるので、IR が同じことを構造で保証できる。`string` の `+` は今、被演算子を消費し、左のバッファを `append` で再利用する（P9）。参照の被演算子でこの道を通ると、参照先を解放する。
- 採らなかった案: すべての被演算子を `Dereference(a)` にする（`string` の `+` が `E1012` のまま。借りる演算子では `BorrowOperand(Dereference(a))` になり、参照を借り直す木と食い違う）、参照先を複製する（確保が増え、性能の原則に反する）。
- 状態: 既定案（実装者はこの案に従う）。

### D4: NC19・NC20 との順序は自由で、後に入る方が合わせる（決定）

- 決定: A40 は NC19・NC20 のどちらの前後に入ってもよい。後に入る方が、他方の経路に参照を外す処理を足す。推奨は NC19 の前（GUIDE D-66 の推奨順 `NC06 →（A40）→ NC19 → NC21`）。
- 理由: NC19 の演算子も、edition 2027 の `Tensor` の読み取り引数の本体では、A40 がないと `deref` が要る。A40 が先なら、NC19 は外した型で (a)〜(d) を引くだけで済む。
- 状態: 決定（D-66 の委任で決定、2026-11-25）。

### D5: 外した後の型付けが失敗したら、今の道の診断を出す（既定案）

- 決定: A40 が外した後の段が失敗したら、A40 は働かず、今の道の診断（同じ式から出るすべてを同じ順序で。コード・位置・文言）をそのまま出す。新しい診断コードと kind は使わない。そのため段 1 は、その場で単一化せずに記録し、段 2・3 の試しが通ってから確定する。通らなければ、記録した場所で今の `same` を呼び直す（仕様 2 の「確定と戻し方」）。今の道は回復して、同じ式から 2 つ以上の診断を出すことがある（P17）。
- 理由: 受理しないプログラムの診断も今と同じになり、診断のベースラインが変わらない。A40 は受理を増やすだけの変更として閉じる。外した型での診断（`no instance for Add<Foo>`）のほうが直接的な場合はあるが、それは診断の改善として別に扱える（`docs/stability.md`: 診断の文言は改善してよい）。
- 採らなかった案: 外した型での診断を出す（`E1003` が `E1005` に変わる式があり、既存の診断のベースラインと help の `own` の案内が変わる）。
- 状態: 既定案。
