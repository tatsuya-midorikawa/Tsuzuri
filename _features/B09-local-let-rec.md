# B09: ローカルな再帰関数（`let rec`）

| 項目 | 内容 |
| --- | --- |
| ID | B09 |
| 優先度 | P2 |
| 規模 | L（Phase 1 は M） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-10-07（人間の依頼「`let rec` も許容されるような機能追加」。HEAD `5b896b7` で確認） |
| 承認 | Phase 1 は不要（構文の追加は 2026-10-07 の依頼による）。要承認: D9（Phase 2 の相互再帰 `let rec ... and ...`、解放処理を持つ値の捕捉、ほかの `let rec` 関数の参照） |
| 改善する劣位 | F# の局所 `let rec` や Rust の局所 `fn` のように、関数の中に再帰する補助関数を書けない。今は `def rec` でモジュールへ出し、外側のローカル値を引数で渡し直す必要がある |
| 手本にする既存実装 | 分離形式のトップレベル `let rec`（`src/parser.rs` の `program_all` で `self.at(&TokenKind::Let)` と既存の署名を照合する分岐）。ラムダの型検査と捕捉の収集（`src/closures.rs` の `Checker::lambda`・`checked_lambda`・`free_locals`）。ラムダのリフト（同ファイルの `lower_expression` の `Lambda` 分岐が `$lambda` 関数を作り `Closure(id, 捕捉)` に置き換える）。末尾ループ（`src/llvm.rs` の `emit_tail`・`is_self`）。未使用警告（`src/warnings.rs` の `unused_locals`）。テスト: `tests/tail_recursion.rs`（`function_ir`）、`tests/currying.rs`、`tests/features.mjs` の suite（GUIDE §7.4） |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/computation.rs`, `src/check.rs`, `src/closures.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/semantic.rs`, `src/warnings.rs`, `src/formatter.rs`, `src/llvm.rs`（`Lambda` の照合だけ）, `tests/local_recursion.rs`（新規）, `tests/fixtures/local_recursion/Main.tz`（新規）, `tests/features.mjs`, `tests/lsp.rs`, `docs/language.md`, `README.md`, `_tsuzuri/language-reference/values-and-functions/functions.md`・`values.md`・`keywords.md`・`lambda-expressions.md`, `_features/README.md` |

## 目的

関数の本体やブロックの中で、自分自身を呼ぶ関数を `let rec` で定義できるようにする。

```text
def total :: i64 -> i64 = \limit ->
    let rec go = \n acc -> if n > limit then acc else go (n + 1) (acc + n)
    go 1 0
```

今は同じ処理を書くのに、`go` を `def rec` でモジュールへ出し、`limit` を引数に足す必要がある。補助関数の名前がモジュールに漏れ、
外側の値を受け渡す引数が増える。`let rec` は外側のローカル値を捕捉したまま再帰でき、直接の末尾再帰は既存の `def rec` と同じくループになる。

実装者は Phase 1 だけを実装する。Phase 2（相互再帰、解放処理を持つ値の捕捉、ほかの `let rec` 関数の参照）は D9 の承認後、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準コマンドが成功し、手順 1 の再現（ローカルな `let rec` が `E0002` になること）を確かめていること。
- Phase 2 は D9 の承認が記録されるまで着手しない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 分離形式のトップレベル `let rec`（先行する `def rec` の実装）の受理・拒否や生成 IR が変わる。
- `let rec` を書かないプログラムの IR が byte 単位で変わる（手順 9）。
- コンピュテーション式の中の `let rec` を、`recursive` を引き継ぐだけでは展開できない（束縛が継続のラムダをまたいで分かれる、など）。
  その場合はコンピュテーション式の中を Phase 1 の対象から外す案と合わせて報告する。
- リフトした関数の自己呼び出しを、捕捉を渡し直す直接呼び出しにすると、所有権検査（`ownership::check_all`）が通らない。
- 捕捉の規則（D4）を緩めたくなった。または、捕捉した値の複製が `copies::sites` の一覧（A15）に現れる。
- 新しい予約語・診断コード・`unsafe`・crate が必要になった。

## 現状（HEAD `5b896b7` で確認）

- 予約語: `rec` は `src/lexer.rs` の予約語で、`TokenKind::Rec` になる。予約語の追加は要らない。
- 分離形式のトップレベル: `src/parser.rs` の `program_all` は、`let` の次（`rec` があれば `rec` の次）が既存の署名名で、まだ実装が無いときだけ
  `let [rec] name = ラムダ` を `def` の実装として読む（値がラムダでなければ「a declared top-level 'let' function needs a lambda ...」の
  `E0002`）。`def rec` の実装なら `rec` が必要で、無いと `E1019`（「'def' and its implementation must have matching 'rec'/'and' groups」）。
  これは動く。
- ブロックの `let`: `src/parser.rs` の `binding_value` が `[mut] name [: 型] = 式` を読み、`Binding { name, mutable, using, annotation, value }`
  （`src/syntax.rs`）を作る。`rec` は受け付けない。呼び出し元は `binding`（波括弧のブロックの `src/parser.rs` のループ、インデントの
  ブロックの `src/parse_control.rs`、`Main.tz` のトップレベルの文）と、コンピュテーション式の文、関数ガードの `where`
  （`src/parse_control.rs`）である。`Binding` を作る箇所は `grep -rn "Binding {" src` で確かめられ（定義とパターンを含めて 37 行）、多くは `src/computation.rs` にある。
- 型検査: `src/check.rs` の `composed_expression` は `ExprKind::Block` の各束縛について、値を検査してから `self.bind` で名前を束縛する。
  したがって値の中から自分の名前は見えず、自己参照は `E1002`（unknown value）になる。ビルダー名を省略したコンピュテーション式で、
  ビルダーが最初の `let!`・`do!` の右辺で決まるとき（期待する型が無い本体。例: 注釈の無いローカルのラムダ `let compute = \n -> ...`）は、
  `src/computation.rs` の `implicit_root` が、それより前の通常の `let` を `composed_expression` を通さずに、同じく値を検査してから
  `self.bind` する。それ以外の展開（`direct_root` や、ビルダーが決まった後の `let`）は束縛を `ExprKind::Block` に戻すので `composed_expression` を通る。
- 捕捉: `src/closures.rs` の `Checker::lambda` は、外側の全スコープのローカルを `outer` に集め、`checked_lambda` が本体の自由変数
  （`free_locals`）のうち `outer` にあるものを捕捉にする。通常のラムダ（`LambdaKind::Function`）の捕捉は `Capture` を満たす必要があり、
  `string`・配列・`Vec` も捕捉できる（関数値を複製すると環境ごと複製する）。
- 捕捉の型の確定: `checked_lambda` の時点では、捕捉の型が推論変数のことがある。注釈の無い整数リテラルは `Type::Infer` のまま `i32` を
  既定として登録するだけで（`src/check.rs` の整数リテラルの検査の `default_numeric`）、既定の型は関数の検査の最後の `finish`
  （`src/polymorph.rs`）の `apply_defaults` で決まる。`Type::is_copy` は `Infer` と型変数に偽を返す。`finish` は型を解決した後、
  `captures` で各ラムダの捕捉に `Capture`・`Send` を改めて要求する。
- リフト: 型検査・`recursion::check`・`polymorph::specialize` のあとで `closures::lower` が呼ばれ、`lower_expression` が各ラムダを
  「捕捉 + 引数」を引数に持つモジュール `$lambda` の `CheckedFunction`（`capture_count` = 捕捉の数）にし、式を `Closure(id, 捕捉の値)` に置き換える。
  捕捉は同じローカルの id のまま、リフトした関数の先頭の引数になる。リフトした関数は `recursion::check`・`check_specialized` の対象外である。
  `analyze`（`src/check.rs`）は特殊化の直後に `closures::lower` と `ownership::check_all` を呼ぶので、型検査だけを通してリフトの前で止まる入口は無い。
- 末尾ループ: `src/llvm.rs` の `emit_tail` は、呼び出し先が `Function(User(自分の id))` で引数の数が関数の引数の数と同じとき
  （`is_self`）、ループへの後方分岐にする。`is_self` は、引数の型のどれかが借用を持つ（`carries_loans`）関数では成り立たない。
  `value |> self` は引数が 1 つの関数だけが対象。
- 未使用警告: `src/warnings.rs` の `unused_locals` は、本体の `Local(id)` をすべて使用として数える。
- 関数の中で再帰する処理の今の書き方は、`def rec` でモジュールに出し、外側の値を引数で渡すことだけである。

### 再現（検証済み。HEAD `5b896b7`）

```sh
mkdir -p /tmp/tz-b09/local && cd /tmp/tz-b09/local
printf 'def total :: i64 -> i64 = \\x ->\n    let rec go = \\n acc -> if n == 0 then acc else go (n - 1) (acc + n)\n    go x 0\n\ntotal 100\n' > Main.tz
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri check Main.tz
```

```text
./Main.tz:2:9: error[E0002]: expected an identifier
  2 |     let rec go = \n acc -> if n == 0 then acc else go (n - 1) (acc + n)
    |         ^
```

`rec` を外すと `./Main.tz:2:48: error[E1002]: unknown value 'go'`（2 行目の `go (n - 1)` の位置）になる。

書き方ごとの結果（`tsuzuri run . --no-cache`）:

| 書き方 | 結果 |
| --- | --- |
| `def rec sum :: ...` と `fn rec sum n total = ...`（分離形式） | `5050` |
| `def rec sum :: ...` と `let rec sum = \n total -> ...`（分離形式） | `5050` |
| `def rec sum :: ...` と `let sum = ...`（`rec` なし） | `E1019` |
| 互換構文 `fn rec sum(n: i64, total: i64) -> i64 { ... }` | `5050` |
| `Main.tz` のトップレベルで `def` なしの `let rec sum = ...` | `E0002`（expected an identifier） |
| 関数本体（インデント）の `let rec go = ...` | `E0002` |
| 関数本体（波括弧）の `let rec go = ...;` | `E0002` |
| `Result { let rec go = ... }` | `E0002` |

## 仕様

### 構文

```text
binding        = "let" ["rec"] ["mut"] name [":" type] "=" expression
recursive_let  = "let" "rec" name [":" type] "=" lambda
```

- `let rec` は、通常の `let` を書けるすべての文の位置に書ける: 波括弧のブロック、インデントのブロック、`Main.tz` のトップレベルの文
  （同名の署名が無いとき。署名があれば従来の分離形式）、コンピュテーション式の通常の `let` の位置（明示ブロックと、ビルダー名を省略した本体）。
- 値はラムダ式そのもの（`\x -> ...`。引数の型注釈を含む）でなければならない。`let rec` の後ろに `mut` は書けない。
- 次は Phase 1 では書けない: `let rec ... and ...`（Phase 2）、`use rec`、`let! rec`、関数ガードの `where` の束縛、`@checked` を付けた `let rec`。

### 意味

- `let rec f = \x -> body` の `f` は、`body` の中と、その文より後ろのブロックで有効である。`body` の中の `f` は「同じ捕捉を持つ自分自身」を指す。
- `f` の型は単相である（ローカルな `let` と同じ）。注釈があればそれを、無ければ推論変数を使い、本体の中の使い方と統一する。
  `f` を本体の中で別の型で使うと `E1003`。
- 本体の中の `f` の呼び出し（関数適用・`x |> f`・部分適用）は、リフトした関数への直接の呼び出しになる。直接の末尾呼び出しは、
  既存の `def rec` と同じ条件（引数と捕捉のどれも借用を持たない）でループになる。捕捉に `ref T` があるとループにならない。
- 本体の中で `f` を値として使う（ほかの関数へ渡す、`rec` の無いほかのラムダが捕捉する）と、通常のラムダ式と同じく新しい関数値を作る。
  入れ子の `let rec` は `f` を捕捉できない（D4。関数値の捕捉になる）。
- ブロックの中で `f` は通常の関数値で、返したり渡したりできる。`rec` は名前解決の静的な契約で、自分を呼ばない `let rec` も受理する。

### 評価順序・所有権・借用

- `let rec` の右辺はラムダ式なので、束縛の時点で評価されるのは捕捉の値だけである（左から右、通常のラムダ式と同じ）。
- Phase 1 の捕捉は、Copy で解放処理を持たない型（`is_copy` が真で `needs_drop` が偽）に限る（D4）。数値・`bool`・`unit`・`char`・`utf8char`・
  共有参照 `ref T`、それらだけからなるタプル・レコード・共用体・固定長配列が該当する。`string`・`utf8string`・配列・リスト・`Vec`・
  関数値（ほかの `let rec` で定義した関数を含む）・`Task`・`Drop` を持つ型・排他参照・型変数（`'a`）の値は捕捉できない。
  配列を読むときは、`let data = ref values` のように共有参照を捕捉する。
- 捕捉の型は、関数の型を解決し既定の型を適用した後で検査する。注釈の無い整数リテラルで作った値（既定の `i32`）も捕捉できる。
- 自己呼び出しでは捕捉をそのまま引数として渡し直す。Copy で解放処理を持たない値だけなので、複製は値のコピーで、確保も解放も起きない。

### 診断

既存のコードを使う。新しい診断コードは無い。

| 条件 | コード | メッセージ | 位置 |
| --- | --- | --- | --- |
| 値がラムダ式でない | `E0002` | `a 'let rec' binding needs a lambda such as '\n -> ...'; bind other values with 'let'` | 値 |
| `let rec mut` | `E0002` | `a 'let rec' function cannot be mutable; rebind the function or use 'let mut' for data` | `mut` |
| `@checked` を付けた | `E0002` | `@checked cannot apply to a 'let rec' binding; put it on the statements inside the function` | `let` |
| `where` の束縛に `rec` | `E0002` | `'where' bindings cannot be recursive; define the function with 'let rec' in the body` | `rec` |
| `let rec ... and ...` | `E0002` | `'and' after a local 'let rec' is not supported yet; define mutually recursive functions with 'def rec' and 'and'` | `and` |
| `let! rec`・`and! rec` | `E0002` | `a 'let!' binding cannot be recursive; define the function with 'let rec' and bind its result with 'let!'` | `rec` |
| 関数値を捕捉（ほかの `let rec` 関数を含む） | `E1005` | `a 'let rec' function cannot capture the function '{name}'; pass it as an argument, or define it with 'def' or 'def rec'` | ラムダ |
| そのほかの捕捉できない型 | `E1005` | `a 'let rec' function captures only values that copy without allocation; '{name}' has type {ty}; pass it as an argument or capture a 'ref'` | ラムダ |
| 本体の中で別の型で使う | `E1003` | 既存の型の不一致のメッセージ | 使用位置 |
| `rec` の無い自己参照 | `E1002` | 既存の `unknown value '{name}'`（変えない） | 使用位置 |
| 自分の本体の中でしか使わない | `W1001` | 既存の `unused local '{name}'; prefix it with '_' to silence this warning` | 名前 |

`use rec` は `use` 束縛の先読み（`use_binding_ahead`）に一致しないので、従来どおりの構文エラー（`E0002`）のままでよい。テストで現在のメッセージを固定する。

### native と WASM

違いは無い。リフトした関数と閉包は既存の経路で生成し、ランタイムや import は増えない。

### 例

```text
def triangle :: i64 -> i64 = \limit ->
    let rec go = \n acc -> if n > limit then acc else go (n + 1) (acc + n)
    go 1 0

def count_above :: [i64] -> i64 -> i64 = \values threshold ->
    let data = ref values
    let rec walk = \index found ->
        if index == data.length then found
        elif data[index] > threshold then walk (index + 1) (found + 1)
        else walk (index + 1) found
    walk 0 0
```

`triangle 100` は `5050`。`triangle` の `go` は捕捉が `i64` だけなので末尾呼び出しがループになり、`limit` が 1,000,000 でもスタックを使い切らない。
`count_above` の `walk` は `ref [i64]` を捕捉するのでループにはならない（深さは配列の長さに比例する）。

## 設計

### データ構造

- `src/syntax.rs` の `Binding` に `pub recursive: bool` を足す。`Binding { ... }` を字面で構築する全箇所に明示的に書く（compile error になった箇所を
  すべて直す。既存の `..binding.clone()` による引き継ぎはそのままでよい）。
  コンピュテーション式の展開（`src/computation.rs`）は、利用者の `let` 文から作る `Binding` に元の `recursive` を引き継ぎ、展開のために
  コンパイラが作る `Binding` は `false` にする。
- `src/check.rs` の `TypedExprKind::Lambda` に `recursive: Option<usize>`（自分を指すローカルの id）を足す。照合している箇所
  （`grep -rn "Lambda {" src`。`check.rs` 3、`closures.rs` 4、`llvm.rs` 2、`ownership.rs` 6、`polymorph.rs` 3、`semantic.rs` 1、`warnings.rs` 1）を
  すべて直す。`polymorph` の特殊化はこの id をそのまま写す。

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/parser.rs` | `binding_value` | `let` の後の `rec` を読み、`recursive` に入れる。`rec` の後の `mut` と、値がラムダ式でないことを「診断」の `E0002` にする。`recursive` の束縛の直後に `and` が続くときは Phase 2 の `E0002` |
| `src/parser.rs` | `binding`・`computation_binding` | `binding_value` はタスクブロックの `let!`（`binding` の `run` が真）、コンピュテーション式の `let!`（`bind` が真）と `and!` の束縛にも使われるので、そこで `recursive` なら「診断」の `let! rec` の `E0002` にする。`let`（`run`・`bind` が偽）の `recursive` は受理する |
| `src/parser.rs`・`src/parse_control.rs` | `@checked` を読む箇所 | `checked` と `recursive` が両方なら `E0002` |
| `src/parse_control.rs` | `where` の束縛 | `binding_value` の結果が `recursive` なら `E0002` |
| `src/computation.rs` | 利用者の `let` 文から `Binding` を作る箇所 | `recursive` を引き継ぐ（`binding.clone()`・`..binding.clone()` はそのまま引き継ぐ。`Expression` 文から作る `_` の束縛は `false`） |
| `src/check.rs` | `composed_expression` の `ExprKind::Block` | 束縛 1 つの検査（注釈・値・`require_use`・`bind`）を補助関数 `Checker::let_binding`（新規。`(Local, TypedExpr)` を返す）にまとめる。`recursive` の束縛は、型（注釈か推論変数）で名前を先に束縛し、その `Local` を `Checker::lambda` に渡して値を検査する。値の型と束縛の型を `same` で統一する |
| `src/computation.rs` | `implicit_root` の前置きの束縛（ビルダーが決まる前の通常の `let`） | 値の検査と `self.bind` を `Checker::let_binding` に置き換える。これが無いと、最初の `let!`・`do!` より前の `let rec` だけ自己参照が `E1002` になる |
| `src/closures.rs` | `Checker::lambda`・`checked_lambda` | 引数 `recursive: Option<&Local>` を足す。自分の id を `outer` から除き（捕捉にしない）、`TypedExprKind::Lambda { recursive: Some(id), .. }` にする。D4 の検査はここでは行わない（捕捉の型がまだ推論変数のことがある） |
| `src/polymorph.rs` | `finish` が呼ぶ `captures` の `Lambda` 分岐 | `recursive` が `Some` なら、型を解決し既定の型を適用した後の各捕捉に D4（`is_copy && !needs_drop`）を検査し、満たさなければ「診断」の `E1005`（関数型なら関数値の行のメッセージ）。型変数も `is_copy` が偽なので `E1005` |
| `src/closures.rs` | `lower_expression` の `Lambda` 分岐 | `recursive` が `Some(self_id)` なら、リフトした関数の本体で `self_id` を次のとおり書き換える（「アルゴリズム」）。そのあとで `Closure(id, 捕捉)` に置き換えるのは従来どおり |
| `src/warnings.rs` | `unused_locals`・`collect_locals` | `Lambda` の `recursive` の id は、そのラムダの本体の中での参照を使用に数えない |
| `src/semantic.rs` | `Lambda` の照合 | 自分を指す参照も、束縛の定義へ解決されることを確かめる（LSP の定義・参照・名前の変更） |
| `src/formatter.rs` | なし（確認だけ） | 字句は保持される。`ast_fingerprint` が `recursive` を区別することをテストで確かめる |

### アルゴリズム（自己参照の書き換え）

`lower_expression` の `Lambda` 分岐で、内側の式を先に下ろした（内側のラムダは先にリフトされる）あと、リフトする関数の id を `id`、
捕捉を `captures`、関数の型を `T = Type::function(捕捉の型 ++ 引数の型, 結果)` として、本体を明示スタックで走査し、次の順で書き換える。

1. `Call(callee, arguments)` で `callee` が `Local(self_id)` のもの → `Call(Function(User(id)) : T, captures の Local ++ arguments)`。
   引数が関数の引数より少なければ、既存の部分適用の経路で閉包になる。
2. `Binary(Pipe, argument, callee)` で `callee` が `Local(self_id)` のもの → `Call(Function(User(id)) : T, captures の Local ++ [argument])`。
   評価順は `argument` が先で変わらない（捕捉は値の読み出しだけ）。末尾にあればループの対象になる。
3. それ以外の `Local(self_id)` → `Closure(id, captures の Local)`（型は `Local(self_id)` の型のまま）。内側のラムダの捕捉の一覧に
   `Local(self_id)` が入っているものも、この規則で関数値を渡す形になる。

書き換えはリフトした関数の本体だけに行い、ブロックの残り（`let rec` の後ろ）の `Local(self_id)` は通常の閉包の変数のまま残す。
走査は Rust の再帰を使わず、明示スタックで行う（深い式で Rust のスタックを使い切らないため。GUIDE §11.1）。

### 生成 IR とランタイム

新しいランタイム関数や intrinsic は無い。リフトした関数は `$lambda` の関数として既存の経路で生成され、自己の末尾呼び出しは
`emit_tail` の `is_self` に一致してループになる。ネイティブの実行ファイルは、IR に再帰があると既存のスタック枯渇の報告（E14）を含む。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインと再現

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、「再現」を確かめる。`let rec` を含まない fixture の IR を比較用に保存する。
- 確認:

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked
mkdir -p /tmp/tz-b09/base
for fixture in tests/fixtures/constants tests/fixtures/string_interpolation; do
  name=$(basename "$fixture")
  target/release/tsuzuri build "$fixture" --emit llvm -o "/tmp/tz-b09/base/$name.ll"
  target/release/tsuzuri build "$fixture" --target wasm32 --emit llvm -o "/tmp/tz-b09/base/$name-wasm.ll"
done
```

### 手順 2: 構文

- 変更: `src/syntax.rs`、`src/parser.rs`、`src/parse_control.rs`、`src/computation.rs`、`Binding` を作る全箇所、`tests/local_recursion.rs`（新規）。
- 内容: 「段ごとの変更」の構文の行。`recursive: false` を全構築箇所に書く。テスト `parses_local_let_rec_bindings`・
  `rejects_invalid_let_rec_syntax`（新規）。この段では型検査の前で `E1002` などが出てもよい（構文のテストは `tsuzuri::parser::parse` の結果で見る）。
- 確認: `cargo test --locked --test local_recursion` が `2 passed`。`cargo test --locked` が成功する。

### 手順 3: 自己参照の表現とリフトの書き換え

- 変更: `src/check.rs`（`TypedExprKind::Lambda` に `recursive`）、`TypedExprKind::Lambda` を照合する全箇所、`src/closures.rs`（`lower_expression`）。
- 内容: `recursive: Option<usize>` を足し、型検査はまだ常に `None` を入れる。「アルゴリズム」の書き換えを `lower_expression` に入れる
  （`None` のときは何もしない）。`analyze` は型検査の直後にリフトと所有権検査まで通すので（「現状」）、型検査より先に書き換えを入れておく。
  この段では動作は変わらない。
- 確認: `cargo test --locked` が成功し、`cargo test --locked --test local_recursion` が `2 passed` のまま。手順 9 の `cmp` が差分なしで終わる。

### 手順 4: 型検査と捕捉の規則

- 変更: `src/check.rs`、`src/computation.rs`、`src/closures.rs`、`src/polymorph.rs`、`tests/local_recursion.rs`。
- 内容: 「段ごとの変更」の `Checker::let_binding`（`composed_expression` と `implicit_root` の両方から使う）、名前の先行束縛、捕捉からの除外、
  `finish` の `captures` での D4 の検査。ここで `recursive: Some(id)` が入り、手順 3 の書き換えが働く。テスト
  `checks_local_let_rec_types_and_captures`・`local_let_rec_calls_itself_directly_and_loops_in_tail_position`・
  `local_let_rec_values_escape_as_closures`・`local_let_rec_in_computation_expressions`（新規）。
- 確認: `cargo test --locked --test local_recursion` が `6 passed`。「再現」のプログラムが `5050` を出す
  （`cargo build --release --locked && target/release/tsuzuri run /tmp/tz-b09/local`）。

### 手順 5: 未使用警告と LSP

- 変更: `src/warnings.rs`、`src/semantic.rs`、`tests/local_recursion.rs`、`tests/lsp.rs`。
- 内容: テスト `unused_local_let_rec_warns`（新規）と、`tests/lsp.rs` の `local_let_rec_names_resolve_in_the_editor`（新規。定義へ移動・
  参照・名前の変更が、自己呼び出しを含めて束縛を指す）。
- 確認: `cargo test --locked --test local_recursion` が `7 passed`、`cargo test --locked --test lsp local_let_rec` が `1 passed`。

### 手順 6: 整形

- 変更: `tests/formatter.rs`。
- 内容: テスト `formats_local_let_rec_without_changing_meaning`（新規。`tsuzuri fmt` の冪等性、`ast_fingerprint` が `let` と `let rec` を区別する）。
- 確認: `cargo test --locked --test formatter local_let_rec` が `1 passed`。

### 手順 7: E2E suite `local_recursion`

- 変更: `tests/fixtures/local_recursion/Main.tz`（新規）、`tests/features.mjs`（suite `local_recursion`、GUIDE §7.4）。
- 内容: 「E2E」の cases。
- 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri local_recursion` が成功する
  （native・WASM × `-O0`・`-O3`、`live == 0`、WASM import なし）。

### 手順 8: 深い末尾再帰

- 変更: `tests/local_recursion.rs`。
- 内容: テスト `deep_local_tail_recursion_does_not_grow_the_stack`（新規。`limit` が 1,000,000 の `triangle` を `-O0` の native で実行し、
  `500000500000` を出す。IR に後方分岐があることも確かめる）。
- 確認: `cargo test --locked --test local_recursion deep_local` が `1 passed`。

### 手順 9: 既存 IR の不変

- 変更: なし。
- 内容: 手順 1 で保存した IR と比べる。
- 確認: `cmp` がすべて差分なしで終わる（手順 1 の 2 fixture × native・wasm32）。

### 手順 10: ドキュメントと最終確認

- 変更: 「ドキュメント」の各ファイル。
- 確認: GUIDE §3 の全体、`node tests/features.mjs target/release/tsuzuri`、`node tests/control.mjs target/release/tsuzuri`、
  `node tests/computations.mjs target/release/tsuzuri`、`node scripts/check-docs.mjs`（変更したページ）。

## テスト計画

### Rust テスト

`tests/local_recursion.rs`（新規）。IR は `tests/tail_recursion.rs` の `function_ir` と同じく、2 回出して一致することを確かめてから見る。

| テスト | 内容 |
| --- | --- |
| `parses_local_let_rec_bindings` | 波括弧・インデント・`Main.tz` のトップレベル・`Result { ... }`・`do!` を使う `def main` の本体で、`Binding::recursive` が真になる。署名のある名前のトップレベル `let rec` は従来どおり分離形式の実装として読まれる |
| `rejects_invalid_let_rec_syntax` | 「診断」の `E0002` の 6 行（値がラムダでない、`let rec mut`、`@checked`、`where`、`and`、`let! rec`・`and! rec`）と、`use rec` の現在のメッセージ |
| `checks_local_let_rec_types_and_captures` | 捕捉 `i64`・`bool`・`char`・`ref [i64]`・`ref string`・Copy なレコードとタプル・`[i64; 3]`・注釈の無い整数リテラルで作った値（`let scale = 2`。既定の `i32`）は受理。`string`・`[i64]`・`[\|i64\|]`・`Vec<i64>`・`Drop` を持つレコード・型変数（`'a`）の値は `E1005`（「診断」の「そのほか」のメッセージ）。関数値と、ほかの `let rec` 関数（前の兄弟、入れ子の外側）は `E1005`（関数値のメッセージ）。本体の中で別の型で使うと `E1003`。`rec` の無い自己参照は `E1002` のまま |
| `local_let_rec_calls_itself_directly_and_loops_in_tail_position` | 捕捉が `i64` の `go` は、リフトした `$lambda` 関数の中に `br label %loop` 相当の後方分岐を持ち、閉包の呼び出しを含まない。捕捉が `ref [i64]` の `walk` は直接呼び出しで、ループにはならない。`n \|> go` も直接呼び出しになる |
| `local_let_rec_values_escape_as_closures` | ブロックから `go` を返す、`Array.map go xs` に渡す、部分適用 `go 1` を返す、本体の中で自分を `Array.map` に渡す、の各プログラムが analyze でき、IR が決定的 |
| `local_let_rec_in_computation_expressions` | 明示ブロック `Maybe { ... }` の中の `let rec`、ビルダーが期待する型で決まる本体（`def f :: i64 -> Maybe<i64>`）の `let rec`、ビルダーが最初の `let!` の右辺で決まる本体（注釈の無いローカルのラムダ `let compute = \n -> ...`）で最初の `let!` より前に置いた `let rec`（`implicit_root` の前置きの束縛）が、どれも自分を呼べて期待どおりの値を返す |
| `unused_local_let_rec_warns` | 自分の本体の中でしか使わない `let rec helper` は `W1001`。ブロックで一度でも使えば警告しない |
| `deep_local_tail_recursion_does_not_grow_the_stack` | 手順 8 |

ほかに `tests/lsp.rs` の `local_let_rec_names_resolve_in_the_editor` と、`tests/formatter.rs` の `formats_local_let_rec_without_changing_meaning`。

### E2E

suite `local_recursion`（新規）、fixture `tests/fixtures/local_recursion/Main.tz`（新規）。期待値は suite 内の JavaScript の BigInt 計算から作り、
コンパイラ出力から写さない。

| case | 期待値 | 参照 |
| --- | --- | --- |
| `triangle` | `5050n` | 1 から 100 の和 |
| `triangle_deep` | `500000500000n` | 1 から 1,000,000 の和（`-O0` でもスタックを使い切らない） |
| `factorial_scaled` | 捕捉した `scale` × 20! | JS BigInt |
| `gcd` | `gcd(1071, 462) = 21n` | ユークリッドの互除法 |
| `fib_memo_free` | `fib(25) = 75025n` | 非末尾の二重再帰 |
| `count_above` | `[5, 12, 7, 30, 1]` で 6 より大きい数 → `3n` | 直接の数え上げ |
| `binary_search` | 整列済み配列の中の位置 | 線形探索の結果 |
| `escaped` | ブロックから返した `go` を 2 回呼んだ結果の和 | 同じ式の JS 計算 |
| `nested` | 1 から 100 の各数の桁の和の合計 `901n`。外側の `let rec` の本体の中に、外側の関数を参照しない `let rec digits` を書く | JS の計算 |
| `piped` | `n \|> go` を末尾で使う再帰の結果 | 同じ式の JS 計算 |

native・WASM × `-O0`・`-O3`、`live == 0`、WASM import なしは harness が確かめる。既存の `node tests/control.mjs` と
`cargo test --locked --test currying`・`--test tail_recursion` も手順 10 で回す。

### 既存テストへの影響

なし。分離形式のトップレベル `let rec` と、`let rec` を書かないプログラムの IR は変わらない（手順 9）。

### 性能

数値の目標は無い。末尾呼び出しのループ化は既存の `def rec` と同じ経路であることを、IR の確認（手順 4・8）で示す。速度の主張はしない。

## ドキュメント

- `docs/language.md` の「関数の宣言と適用」と「再帰とスタック」: ローカルな `let rec` の構文・意味・捕捉の規則（D4）・末尾ループの条件・
  Phase 1 で書けない位置。「`let rec name = \value -> ...` や互換構文の `fn rec name(x: T) -> T { ... }` に対しても同一の規則が適用されます」を、
  分離形式とローカルな `let rec` を分けて書き直す。
- `README.md` の「構文と表現」の「明示的再帰（`rec`／`and`）」に、ローカルな `let rec` を足す。
- `_tsuzuri/language-reference/values-and-functions/functions.md` の「再帰」: 「ローカル束縛に `let rec` はありません」を、`let rec` の説明と
  実行例に置き換える。`values.md`（`let` の種類）、`keywords.md`（`rec` の行の例）、`lambda-expressions.md`（捕捉の規則）。
  `node scripts/check-docs.mjs` で検査する。
- `_features/README.md` の B09 の状態（GUIDE §10）。

## 受け入れ条件

- [ ] 「再現」のプログラムが `5050` を出し、「仕様」の位置（波括弧・インデント・`Main.tz` のトップレベル・コンピュテーション式。ビルダー名を省略した本体で最初の `let!` より前に置く場合を含む）で `let rec` を書ける。
- [ ] 「診断」の表のとおりに拒否・警告する（Rust テスト）。注釈の無い整数リテラルで作った値の捕捉は受理する。
- [ ] 捕捉が D4 を満たし、捕捉と引数のどれも借用を持たない（`carries_loans` が偽。`ref T` の捕捉や参照の引数は借用を持つ）とき、直接の末尾呼び出しがループになり、深さ 1,000,000 でもスタックを使い切らない。
- [ ] suite `local_recursion` が native・WASM × `-O0`・`-O3` で成功し、`live == 0`、WASM import なし。
- [ ] 分離形式のトップレベル `let rec` と、`let rec` を書かないプログラムの IR が変わらない。
- [ ] LSP の定義・参照・名前の変更が、自己呼び出しを含めて束縛を指す。`tsuzuri fmt` が意味を変えない。
- [ ] Phase 2 に着手していない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `Checker::lambda` の `outer` は外側の全スコープのローカルを集める。自分の id を除かないと、閉包が自分自身を捕捉しようとし、
  `Capture<関数型>` の要求と循環した環境になる。
- `lower_expression` は内側のラムダを先にリフトする。外側の本体を書き換える時点で、内側の `Closure` の捕捉の一覧に `Local(self_id)` が
  入っている。これも規則 3 で関数値にする（取りこぼすと、リフト後の関数に存在しないローカルを参照する）。
- 自己呼び出しを `Closure` の呼び出しのまま残すと、`is_self` に一致せずループにならない。末尾の `x |> f` も直接呼び出しに直す。
- 捕捉に `ref T` があると `carries_loans` によりループにならない。これは `def rec` と同じ既存の制約で、このチケットでは変えない。文書に書く。
- `Binding` を作る箇所の多くは `src/computation.rs` にある。展開のためにコンパイラが作る束縛まで `recursive` を引き継ぐと、継続のラムダが
  自己参照を持つことになる。利用者の文から作るものだけに引き継ぐ。
- `let rec` の名前を `self.bind` で先に束縛すると、値の検査中にシャドーイングの警告や未使用の判定が変わることがある。G03 の警告のテストを回す。
- `implicit_root` は、ビルダーが決まる前の通常の `let` を `composed_expression` を通さずに検査する。`composed_expression` だけを直すと、
  最初の `let!`・`do!` より前の `let rec` だけが `E1002` になる。束縛の検査は `Checker::let_binding` の 1 か所にまとめる。
- D4 の検査を `checked_lambda` で行うと、注釈の無い整数リテラルで作った値（`Type::Infer`。既定の `i32` は `finish` で決まる）の捕捉を
  誤って拒否する。型を解決した後の `finish` の `captures` で検査する。
- 入れ子の `let rec` から外側の `let rec` を呼ぶと、外側の関数値の捕捉になり、D4（関数型は `needs_drop` が真）で `E1005` になる。
  これは Phase 1 の仕様で、テストで固定する（Phase 2 の案は D9）。
- `analyze` は型検査の直後にリフトと所有権検査まで通す。型検査で `recursive: Some(id)` を入れる前に、リフトの書き換えを入れておかないと、
  リフトした関数の本体に、引数にも捕捉にも無いローカル（自分の id）が残る（手順 3 を先に行う理由）。
- `cargo test --locked local_rec` は他の binary の同名テストも拾う。`--test local_recursion` を使い、件数を見る。

## 対象外

- 相互再帰 `let rec f = ... and g = ...`（Phase 2、D9）。
- 解放処理を持つ値（`string`・配列・`Vec`・関数値など）の捕捉（Phase 2、D9）。今は共有参照を捕捉するか、引数で渡す。
- ほかの `let rec` 関数（前の兄弟、入れ子の外側）の参照（Phase 2、D9）。今は引数で渡すか、捕捉の無い関数はモジュールの `def rec` にする。
- 型変数（`'a`）の値の捕捉。多相な関数の中の `let rec` には、その値を引数で渡す。
- 多相な局所関数（ローカルな `let` の単相性は変えない）。
- 関数値を経由する再帰や非末尾再帰のスタック使用量の保証（既存の `def rec` と同じ）。
- 分離形式のトップレベル `let rec`、互換構文 `fn rec` の変更。

## 決定事項

### D1: 構文

- 決定: `let rec name [: type] = ラムダ式`。`rec` は `let` の直後に置き、`mut` とは組み合わせない。
- 理由: 既存の予約語だけで表せ、分離形式のトップレベル `let rec` や F# と同じ語順になる。
- 状態: 既定案（実装者はこの案に従う）

### D2: 値はラムダ式に限る

- 決定: 右辺はラムダ式の字面に限る。関数を返す式（部分適用や `Owned.function`）は `E0002`。
- 理由: 自分を参照する値を、評価の前に閉包として作れるのはラムダ式だけである。値の再帰（無限の構造）を導入しない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 書ける位置

- 決定: 通常の `let` を書ける文の位置すべて（コンピュテーション式の通常の `let` を含む）。`use`・`let!`・`where`・`@checked` との組み合わせは不可。
- 理由: 利用者が `let` と同じ場所に書けると期待するため。束縛の意味が変わる位置（`use` の解放、`let!` の展開、`where` の評価時点）は Phase 1 から外す。
- 状態: 既定案（実装者はこの案に従う。コンピュテーション式で停止条件に当たったら報告する）

### D4: Phase 1 の捕捉は Copy で解放処理を持たない型だけ

- 決定: `let rec` のラムダの捕捉は `is_copy && !needs_drop` を満たす型に限り、違反は `E1005`。検査は関数の型を解決し既定の型を適用した後
  （`finish` の `captures`）に行う。型変数の値と関数値（ほかの `let rec` 関数を含む）は満たさない。
- 理由: 自己呼び出しのたびに捕捉を引数として渡し直すため、複製が値のコピーで済む型に限れば、隠れた確保・深いコピー・ムーブ後の使用が起きない。
  配列は共有参照を捕捉すれば読める。型の解決前に検査すると、注釈の無い整数リテラルの値（既定の `i32`）を誤って拒否する。
- 状態: 既定案（実装者はこの案に従う）

### D5: 自己参照の表現

- 決定: 型検査では `Local(self_id)` のまま持ち、`TypedExprKind::Lambda` の `recursive` に id を記録する。リフト時に、呼び出しは直接呼び出し、
  それ以外は `Closure` へ書き換える。
- 理由: 型検査・特殊化・所有権検査の既存の経路をそのまま通せ、末尾ループの判定（`is_self`）に変更が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 型は単相

- 決定: `let rec` の名前の型は 1 つの型に決まる。注釈が無ければ推論変数で、本体の中の使い方と統一する。
- 理由: ローカルな `let` の単相性（`docs/language.md`）と揃える。多相な再帰は `def rec` で書ける。
- 状態: 既定案（実装者はこの案に従う）

### D7: 未使用の判定

- 決定: 自分の本体の中の参照は使用に数えず、ほかで使わなければ `W1001`。
- 理由: 呼ばれない補助関数を見つけられる。自己参照だけで「使っている」と扱うと、警告が意味を失う。
- 状態: 既定案（実装者はこの案に従う）

### D8: 新しい診断コードを作らない

- 決定: 構文の誤りは `E0002`、捕捉の型は `E1005`、型の不一致は `E1003` を使う。
- 理由: いずれも既存のコードの意味（構文、インスタンス・捕捉の制約、型の不一致）に収まる。GUIDE D-16・D-30 の割り当てを増やさない。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase 2（相互再帰、解放処理を持つ値の捕捉、ほかの `let rec` 関数の参照）

- 決定案: `let rec f = ... and g = ...` を、グループの捕捉の和を共通の先頭引数に持つ複数のリフト関数として実装する。解放処理を持つ値の捕捉は、
  自己呼び出しで捕捉を借用として渡す経路（リフト関数の捕捉引数を共有借用にする）を設計してから許す。ほかの `let rec` 関数（前の兄弟、入れ子の外側）の
  参照は、その関数値を捕捉する代わりに相手の捕捉を自分の捕捉に加え、相手のリフト関数への直接呼び出しに書き換える（相手の id はリフトの前に予約する）。
- 理由: 相互再帰は互いの閉包を作るために全員の捕捉が要る。所有値の捕捉を値で渡し直すと、呼び出しごとの深いコピーかムーブが起きる。
  関数値の捕捉は環境の複製で確保が起きるので、D4 のままでは許せない。
- 状態: 要承認（承認前は Phase 2 に着手しない）
