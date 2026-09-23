# B05: コンピュテーション式の拡張（match!／and!／use／try）
| 項目 | 内容 |
|---|---|
| ID | B05 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | (B01) |
| 後続 | B06 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/computation.rs`, `src/check.rs`, `src/call_specialization.rs`, `tests/computations.rs`, `tests/fixtures/computations/`, `tests/computations.mjs`, `benchmarks/run-computations.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

F# の computation expression にある追加構文のうち、Tsuzuri の意味論に合うものだけを導入する。

- 実装する:
  - `match!`: builder 値を bind して、その成功 payload を pattern match する。
  - `and!`: 複数の独立した builder 値を applicative 風に束縛する。自動並列化はしない。
  - 最適化プロトコル: `MergeSources`, `BindReturn`, `Bind2` は builder が定義した場合だけ使う。
- 明示的に拒否する:
  - `use`: Tsuzuri には任意 destructor / `IDisposable` 相当がない。
  - `try...with` / `try...finally`: Tsuzuri には回復可能な例外や unwind がない。

## 現状

- `docs/language.md` の「コンピュテーション式の性能」は `and!`／`MergeSources`、`match!`、`try`、`use`、`BindReturn` を未対応としている。
- `src/computation.rs::OPERATIONS` は `Bind`, `Return`, `ReturnFrom`, `Yield`, `YieldFrom`, `Zero`, `Combine`, `Delay`, `Run`, `For`, `While` だけ。
- `src/parser.rs::computation_statement` は `let!`, `do!`, `return`, `yield`, `if`, `for`, `while`, 通常式だけを読む。
- `src/computation.rs::Lowering::block` は statement list を末尾から処理し、`let!` を `Bind`、`return` を `Return` へ下げる。
- `task { ... }` は `src/parser.rs::task` と `TaskRun` 専用 lowering であり、`.tc` builder ではない。
- `Task.parallel` は明示 API。`docs/language.md` は `and!` を未対応とし、結果型が異なる仕事を同じ配列に入れられないと説明している。

### 前提とする他チケットのインターフェース

- **B01（弱依存）**: `Option`/`Result` builder は `Bind`, `Return`, `ReturnFrom`, `Delay`, `Run`, `Combine` を提供する。B05 の機能は任意 builder に対して動くが、テストの代表として `Option`/`Result` を使う。

### 他チケットへの提供インターフェース

- B06 は `task` に自動 `and!` 並列化を期待しない。複数タスクの並列実行は引き続き `Task.parallel` / `Task.parallel_results`。

## 仕様

### 実装対象の一覧

| 機能 | phase 1 |
|---|---|
| `match!` | 実装する |
| `and!` | 実装する |
| `MergeSources` | builder が定義する場合だけ使用 |
| `BindReturn` | builder が定義する場合だけ、単純な `let!` + `return` を最適化 |
| `Bind2` | builder が定義する場合だけ、2 個の `and!` + `return` を最適化 |
| `use` | 明示拒否 |
| `try...with` | 明示拒否 |
| `try...finally` | 明示拒否 |
| task の `and!` 自動並列 | 実装しない。`Task.parallel` を使う |

### `match!`

構文:

```text
ComputationStatement ::=
    ...
  | "match" "!" Expression "with" ComputationMatchArms

ComputationMatchArms ::=
    "|" Pattern ["when" Expression] "->" ComputationBody
    { "|" Pattern ["when" Expression] "->" ComputationBody }
```

例:

```text
Option {
    match! parse text with
    | 0 -> return "zero"
    | n when n > 0 -> return "positive"
    | _ -> return "negative"
}
```

展開:

```text
B.Bind (parse text) (value ->
    match value with
    | 0 -> <展開済み computation body>
    | n when n > 0 -> <展開済み computation body>
    | _ -> <展開済み computation body>)
```

規則:

- matched computation expression は一度だけ評価する。
- `B.Bind` が継続を呼ばなければ match arms は評価しない。
- arm の pattern/guard は既存 `match` と同じ型・所有権・評価順序。
- arm body は computation body であり、`return`/`let!` などを含められる。
- 全 arm の結果型は builder の通常規則で揃う。
- 非網羅の場合の扱いは通常 `match` と同じ。A03 後は A03 の診断規則に従う。

### `and!`

構文:

```text
ComputationStatement ::=
    "let" "!" Binding "=" Expression
    { "and" "!" Binding "=" Expression }
    ";" ComputationBody
```

例:

```text
Option {
    let! a = Some 20
    and! b = Some 22
    return a + b
}
```

評価順序:

- `a` の右辺、`b` の右辺、... をソース順に一度ずつ評価する。
- `and!` は自動短絡評価ではない。各 source expression 自体は厳格評価される。
- source expression の内部に builder 短絡がある場合は、その式の中の意味に従う。
- `MergeSources`/`Bind2` が後で `None`/`Error` を返すことはできるが、右辺評価済みのトラップや確保を巻き戻さない。

基本展開:

2 個の場合:

```text
B.Bind (B.MergeSources m1 m2) (fx (x, y) -> C)
```

3 個以上の場合は左結合:

```text
let merged = B.MergeSources (B.MergeSources m1 m2) m3
B.Bind merged (fx ((x, y), z) -> C)
```

`MergeSources` がない場合:

- `and!` を使った時点で `E1018`。
- `Bind` のネストで代替しない。`and!` は applicative 合成を要求する構文であり、monadic 順次 bind と同じ意味にしない。

pattern:

- `let! (x, y) = ... and! z = ...` のように各 binding は既存 `binding_value` と同じ名前・`mut`・型注釈を受ける。
- phase 1 では `and!` の binding pattern は単純な識別子だけに制限する。`fx` と同じ一般 pattern へ拡張するのは別段階。
- `let! mut x` は通常 `let!` と同様に継続内の local を可変にする。

### `BindReturn`

builder が `BindReturn` を定義する場合、次の形を最適化してよい。

```text
B {
    let! x = m
    return f x
}
```

展開:

```text
B.BindReturn m (x -> f x)
```

制約:

- 構文的に tail が `return expression` だけの場合に限る。
- `return!`, `do!`, 通常 `let`, `if`, `for`, `while`, `match!`, `and!` を含む場合は通常 `Bind`。
- `BindReturn` がない場合は既存 `Bind` + `Return`。
- `BindReturn` は意味を変える最適化ではなく、builder 側が同じ意味を提供する責任を持つ。コンパイラはモナド則を仮定しない。

### `Bind2`

builder が `Bind2` を定義し、`and!` がちょうど 2 個で、tail が `return expression` だけの場合:

```text
B {
    let! x = mx
    and! y = my
    return f x y
}
```

展開:

```text
B.Bind2 mx my (x -> y -> f x y)
```

または builder がタプル callback を望む場合との曖昧さを避けるため、phase 1 のシグネチャは固定する。

説明上は「2 つの builder 値と 2 引数 callback」を受ける操作だが、Tsuzuri には HKT がないため実際は通常の関数型として builder ごとに定義する。例:

```text
def Bind2 :: Option 'a -> Option 'b -> ('a -> 'b -> 'c) -> Option 'c
```

`Bind2` がなく `MergeSources` がある場合は `MergeSources` + `Bind`。

### `use` の拒否

Tsuzuri には任意 destructor、RAII、`IDisposable`、`finally` unwind がない。所有値は既存の lexical scope と move/drop で管理するため、`use` は導入しない。

`use` は D-15 の予約語一覧にないため、グローバル予約語にしない。computation statement の先頭に identifier `use` があり、`use name = expr` または `use! name = expr` の形なら `E1018` で拒否する。

推奨メッセージ:

```text
"use bindings are not supported; bind the owned value with 'let' and rely on lexical drop"
```

### `try...with` / `try...finally` の拒否

Tsuzuri には回復可能な例外と unwind がない。失敗は `Option`/`Result`、トラップは回復不能。

`try` も予約語にしない。computation statement の先頭に identifier `try` があり、後続に `with` または `finally` 風の形が見える場合は `E1018`。

推奨メッセージ:

```text
"try expressions are not supported; represent recoverable failure with Option or Result"
```

### task builder との関係

- `task { ... }` は `.tc` builder ではないため、B05 の `match!`/`and!` は phase 1 では task に導入しない。
- `task { and! a = ... }` は既存 parser では `and` が statement として不正になり、`E0002`。実装で専用に検出するなら `E1018` にしてもよいが、受理して自動並列化してはいけない。
- 並列に実行したい場合は明示的に `Task.parallel [task { ... }, task { ... }]` を使う。
- 理由: `and!` の source expression 評価とタスク開始時点を自動並列にすると、cold・一回実行・所有権・スケジューリング費用の契約が変わる。

## 設計

### AST

`src/syntax.rs`:

```rust
pub enum ComputationStatementKind {
    ...
    Match(Expr, Vec<ComputationMatchArm>),
    LetAnd(Vec<Binding>),
    UnsupportedUse(Span),
    UnsupportedTry(Span),
}

pub struct ComputationMatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: ComputationBlock,
    pub span: Span,
}
```

ただし `LetAnd` は tail body と一緒に扱う必要があるため、実装上は `ComputationStatementKind::Let(Binding, true)` に `and_bindings: Vec<Binding>` を持たせる設計でもよい。小さい差分を優先するなら:

```rust
Let {
    binding: Binding,
    bind: bool,
    and_bindings: Vec<Binding>,
}
```

既存 enum を壊す範囲が広ければ `Let(Binding, bool, Vec<Binding>)` へ拡張する。

### Parser

`src/parser.rs::computation_statement`:

- `match` の後に `!` があれば `computation_match_bang`。
- `let!` を読んだ後、続く行/`;` の前に `and!` を複数読む。
- `and!` は `let!` の直後の binding group でだけ許可。単独 `and!` は `E0002` または `E1018`。
- statement 先頭が `Ident("use")` または `Ident("try")` なら unsupported diagnostic を返す。

`match!` arms の body は `computation_body()` を使う。通常 `match` の body ではない。

### computation::OPERATIONS

追加:

```rust
"MergeSources",
"BindReturn",
"Bind2",
```

`collect` はこれらも builder operation として認識する。

### Lowering

`Lowering::block` を段階的に拡張する。

#### `match!`

末尾からの処理を維持するため、`match!` statement は「値を作る statement」として扱う。

```rust
let matched = self.call("Bind", vec![value, continuation], span)?;
```

continuation body:

```text
fx $computation.match ->
    match $computation.match with
    | p1 when g1 -> lower(C1)
    | p2 -> lower(C2)
```

各 arm body は `self.block(&arm.body)` で builder 結果へ下げる。

#### `and!`

`let! x = mx and! y = my; C` を見つけたら:

1. tail `C` を `self.finish(result, bindings, body.span)` で作る。
2. tail が単純 `return expr` かどうかを `ComputationBlock` の構文段階で判定できるなら `Bind2` 候補。
3. `Bind2` が使える場合:
   - `B.Bind2 mx my (x -> y -> return_expr)`。
   - `return_expr` には `B.Return` を挟まない。`Bind2` の契約は payload を返す callback。
4. それ以外:
   - `MergeSources` が必要。なければ `E1018`。
   - `merged = B.MergeSources mx my`（3 個以上は左 fold）。
   - continuation の引数は nested tuple。注釈や `mut` は tuple を一度分解する block を生成して適用する。
   - `B.Bind merged continuation`。

`BindReturn`:

- `let! x = m; return expr` の形を lowering 前に検出し、`methods.contains("BindReturn")` なら使う。
- 失敗時の diagnostics を安定させるため、`BindReturn` が存在して型が合わない場合はその型エラーを出す。存在しない場合は通常経路。

### 型・所有権

Lowering 後は通常の `ExprKind::Call`, `Lambda`, `Match`, `Block` になるため、`check`, `ownership`, `llvm` に専用 IR は不要。

注意:

- `and!` の source expressions は lowering 後の `MergeSources` 呼び出しの引数として左から右に評価される。既存の関数呼び出し評価順序を使う。
- `MergeSources` が両方の計算を保持する場合、所有値は builder 実装の型・所有権で検査される。
- `Bind2`/`BindReturn` callback は通常の lambda。排他参照や task を再利用可能な関数値へ捕捉する場合は既存 `Capture` 制約で拒否。

### IR と性能

- `BindReturn`/`Bind2` は builder が定義している場合だけ関数呼び出し数を減らす。
- コンパイラは `Bind` + `Return` と `BindReturn` が同じ意味だと仮定しない。
- `call_specialization` は新操作名も通常関数として扱う。特別な最適化は不要。
- IR テスト:
  - `BindReturn` を定義した builder では `@tz.fn.Builder.BindReturn` が呼ばれ、`@tz.fn.Builder.Return` が不要な箇所で出ない。
  - `Bind2` を定義した builder では `@tz.fn.Builder.Bind2`。
  - `MergeSources` fallback builder では `@tz.fn.Builder.MergeSources` + `@tz.fn.Builder.Bind`。

## 実装手順

1. **operation 名だけ追加する**
   - `src/computation.rs::OPERATIONS` に `MergeSources`, `BindReturn`, `Bind2` を追加。
   - まだ構文は使わない。
   - 確認: `.tc` にこれらだけを定義しても builder として収集される Rust テスト。
2. **`match!` を実装する（phase 1 fully specified）**
   - AST に `ComputationMatchArm` と statement kind を追加。
   - parser で `match! expr with | pattern -> computation_body` を読む。
   - `expand_block` で pattern/guard/body 内の computation を再帰展開。
   - lowering で `Bind expr (value -> match value with arms)` を生成。
   - 確認:
     - `Option { match! Some 1 with | 1 -> return 42 | _ -> return 0 }`
     - `Result { match! Ok (1,2) with | (a,b) -> return a+b }`
     - `None`/`Error` で arms の `1/0` が実行されない。
3. **unsupported `use`/`try` を明示拒否する**
   - computation statement 先頭で `Ident("use")`/`Ident("try")` を検出し `E1018`。
   - グローバル予約語にはしない。
   - 確認: `Builder { use x = value; return x }` と `Builder { try return 1 with | _ -> return 0 }` が `E1018`。
4. **`and!` + `MergeSources` を実装する**
   - `let!` statement parser で `and!` group を読む。
   - `MergeSources` がなければ `E1018`。
   - 2 個以上を左 fold し、nested tuple を continuation block で分解する。
   - 確認: `Pair` builder fixture で評価順序と結果。
5. **`BindReturn` 最適化を追加する**
   - tail が単純 `return expr` の `let!` で、operation が存在する場合だけ使用。
   - 確認: IR に `BindReturn`。
6. **`Bind2` 最適化を追加する**
   - ちょうど 2 個の `and!` + tail `return expr` + operation 存在時だけ使用。
   - 確認: IR に `Bind2`、fallback では `MergeSources`。
7. **E2E と benchmark**
   - `tests/fixtures/computations/Extensions.tc` と `Main.tz` を追加。
   - native/WASM × `-O0`/`-O3`、heap tracking、WASM import なし。
   - `benchmarks/run-computations.mjs` に `and!`/`Bind2` workloads を追加。

## テスト計画

### Builder fixtures

`tests/fixtures/computations/Applicative.tc`:

```text
record Pair { ok: bool, value: i64 }

def Return :: i64 -> Pair
fn Return value = Pair { ok: true, value: value }

def Bind :: Pair -> (i64 -> Pair) -> Pair
fn Bind value next = if value.ok then next value.value else value

def MergeSources :: Pair -> Pair -> Pair
fn MergeSources left right =
    if left.ok then
        if right.ok then Pair { ok: true, value: left.value * 1000 + right.value }
        else right
    else left

def BindReturn :: Pair -> (i64 -> i64) -> Pair
fn BindReturn value next =
    if value.ok then Pair { ok: true, value: next value.value } else value

def Bind2 :: Pair -> Pair -> (i64 -> i64 -> i64) -> Pair
fn Bind2 left right next =
    if left.ok then
        if right.ok then Pair { ok: true, value: next left.value right.value }
        else right
    else left
```

### Rust 受理

- `match!`:
  ```text
  Option {
      match! Some (20, 22) with
      | (a, b) -> return a + b
      | _ -> return 0
  }
  ```
- `match!` short-circuit:
  ```text
  Option {
      match! None with
      | _ -> return 1 / 0
  }
  ```
  結果は `None` で、トラップしない。
- `and!`:
  ```text
  Applicative {
      let! a = Applicative.Return 20
      and! b = Applicative.Return 22
      return a + b
  }
  ```
- `BindReturn` IR:
  ```text
  Applicative { let! a = Applicative.Return 41; return a + 1 }
  ```
- `Bind2` IR:
  ```text
  Applicative {
      let! a = Applicative.Return 20
      and! b = Applicative.Return 22
      return a + b
  }
  ```

### Rust 拒否

| プログラム | 期待 |
|---|---|
| `Identity { let! a = 1 and! b = 2; return a + b }` | `E1018` (`MergeSources`) |
| `Identity { and! a = 1; return a }` | `E0002` or `E1018` |
| `Identity { use x = 1; return x }` | `E1018` |
| `Identity { try return 1 with | _ -> return 0 }` | `E1018` |
| `task { let! a = task { 1 } and! b = task { 2 }; return a + b }` | 拒否。自動並列化しない |

### E2E

`tests/computations.mjs` に追加:

- `match_bang_some() == 42`
- `match_bang_none() == 0`
- `and_merge() == 42`
- `bind_return_ir()` は実行値 42、IR に `BindReturn`
- `bind2_ir()` は実行値 42、IR に `Bind2`
- `merge_fallback_ir()` は IR に `MergeSources` と `Bind`
- owned string payload で `live == 0`
- WASM import なし

### 性能

- `and!` + `Bind2` と、`MergeSources` + `Bind` fallback と、手書き builder 呼び出しを比較。
- 速度閾値なし。
- IR allocation count を記録。

## ドキュメント

- `docs/language.md`:
  - 未対応一覧から `match!` と `and!` を外す。
  - `use`, `try`, destructor/exception は未対応として理由を明記。
  - `and!` は自動並列化ではない、source expression は左から右に厳格評価、と明記。
  - task では `Task.parallel` を使うと明記。
- `docs/architecture.md`:
  - computation lowering に `match!`, `and!`, `MergeSources`, `BindReturn`, `Bind2` を追加。
  - 専用 TypedExpr/runtime を追加しないことを明記。
- `README.md`:
  - computation expression 節の未対応リストを更新。
- `docs/benchmarks.md`:
  - 追加 benchmark の測定結果を書く場合のみ更新。

## 受け入れ条件

- [ ] `match!` が `Bind` + 通常 `match` へ展開され、`None`/`Error` で arms を実行しない。
- [ ] `and!` が `MergeSources` を要求し、source expression を左から右に一度ずつ評価する。
- [ ] `BindReturn`/`Bind2` は存在時だけ使い、存在しなければ既存意味に fallback する。
- [ ] `use` と `try` は明示診断で拒否される。
- [ ] `task` に `and!` 自動並列化を導入しない。
- [ ] 生成 IR は決定的で、native/WASM × `-O0`/`-O3`、heap tracking、WASM import なしを確認した。
- [ ] 速度閾値を CI に入れない。

## 落とし穴

- `and!` を nested `Bind` へ勝手に変換すると、applicative と monadic の意味が混ざる。`MergeSources` がないなら拒否。
- `and!` は source expressions を厳格評価する。短絡を期待するなら `let!` を使う。
- `BindReturn` はコンパイラが意味同値を保証するものではない。builder が定義した場合だけ、その builder の契約として使う。
- `try` を導入すると D-10 の失敗モデルと衝突する。
- `use` を導入すると lexical drop と任意 destructor の境界が曖昧になる。
- `task and!` を自動並列化すると cold task の開始時点が変わる。

## 対象外

- 任意の custom operation 名。
- `yield` 系の `and!` 最適化。
- `try`/exception/unwind/finally/destructor。
- task builder 専用 `and!`。
- HKT/Monad/Applicative 型クラス。

## 未決事項

- **既定案:** `Bind2` callback は curried `('a -> 'b -> 'c)` とする。tuple callback `(('a * 'b) -> 'c)` は採用しない。Tsuzuri の関数がカリー化されていることと既存 `Bind` 形に合わせる。
- `and!` binding に一般 pattern を許すかは phase 2。既定案は単純識別子のみで開始し、必要なら `fx` の pattern lowering を再利用して拡張する。
- unsupported `use`/`try` の診断コードは既存の「builder 操作やファイル種別の不正」に近い `E1018` を既定とする。構文段階でしか検出できない場合だけ `E0002` になりうるが、ユーザー向けには `E1018` を目標にする。
