# B02: Option／Result ビルダーによる早期伝播
| 項目 | 内容 |
|---|---|
| ID | B02 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | B01 |
| 後続 | D01, D02, D04, C02, C06, C07, B06 |
| 状態 | todo |
| 主な影響ファイル | `std/Option.tc`, `std/Result.tc`, `src/computation.rs`, `src/parser.rs`, `src/check.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `tests/computations.rs`, `tests/fixtures/computations/`, `tests/computations.mjs`, `tests/call_specialization.rs`, `benchmarks/run-computations.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `docs/benchmarks.md` |

## 目的

`Option { ... }` と `Result { ... }` のコンピュテーション式を、失敗時に後続を呼ばない早期伝播の標準記法として確立する。D-10 に従い `?` 演算子や例外は導入せず、既存の `.tc` builder 展開・型推論・所有権・LLVM 特殊化でゼロコストに近いコードを生成する。

ユーザーが書く形:

```text
def parse_pair :: &string -> &string -> Result (i64 * i64) string
fn parse_pair left right = Result {
    let! a = D01.parse_i64 left
    let! b = D01.parse_i64 right
    return (a, b)
}

def find_positive :: [i64] -> Option i64
fn find_positive values = Option {
    for n in values do
        do! if n > 0 then Some () else None
    return values[0]
}
```

## 現状

- B01 で `std/Option.tc`／`std/Result.tc` が `Bind`, `Return`, `ReturnFrom`, `Zero`, `Combine`, `Delay`, `Run`, `For`, `While` を提供する前提。
- `src/computation.rs::Lowering::block` は既に早期伝播に必要な関数呼び出し形へ展開できる。
  - `let! x = value; C` → `B.Bind value (x -> C)`
  - `do! value; C` → `B.Bind value (_: unit -> C)`
  - `return value` → `B.Return value`
  - `return! value` → `B.ReturnFrom value`
  - `if condition { C1 } else { C2 }` → 通常の `if` で選んだ分岐だけ実行
  - `C1; C2` → `B.Combine C1 (delay(C2))`
  - `for pattern in values do C` → `B.For values (fx element -> match element with pattern -> C)`
  - `while condition do C` → `B.While (unit -> condition) (B.Delay (unit -> C))`
- `src/parser.rs::computation_sequence` は `return`/`return!` がブロック末尾にあることを構文上検査する。
- `src/call_specialization.rs` は builder 名を特別扱いせず、通常の高階関数として既知継続を特殊化する。
- `tests/fixtures/computations/Choice.tc` は短絡の既存例だが、`record Result { valid: bool, value: i64 }` 固定で、標準の union ではない。

### 前提とする他チケットのインターフェース

- **B01**:
  - `Option.Bind : Option 'a -> ('a -> Option 'b) -> Option 'b`
  - `Option.Zero : Option 'a`
  - `Option.Combine : Option unit -> (unit -> Option 'a) -> Option 'a`
  - `Result.Bind : Result 'a 'e -> ('a -> Result 'b 'e) -> Result 'b 'e`
  - `Result.Zero : Result unit 'e`
  - `Result.Combine : Result unit 'e -> (unit -> Result 'a 'e) -> Result 'a 'e`
  - `Delay` は恒等、`Run` は thunk 呼び出し。
- **A02/E02**: `Option`/`Result` の union と std fallback が実装済み。

### 他チケットへの提供インターフェース

- D01/D02/D04 は回復可能な失敗を返す関数で `Result { let! ... }` を使える。
- B06 は `Result` を含む `Task (Result 'a 'e)` を合成できる。
- B04 は Option 返却アクティブパターンの recognizer 実装例として `Option { ... }` を使える。

## 仕様

### 方針

- 新しい `?` 演算子は導入しない。
- `Option { ... }` は `None` を見つけた時点で後続を呼ばず、全体を `None` にする。
- `Result { ... }` は最初の `Error error` を保持し、後続を呼ばず、全体を `Error error` にする。
- `Result` の error 型はすべて同じ `'e` に単一化する。暗黙の `Option` → `Result` 変換、`string` → 独自 error 型変換、error 型の upcast はない。
- `Option` と `Result` を混ぜる場合は明示的に `Option.to_result`／`Result.to_option` を使う。

### 正確な展開

以下の `B` は `Option` または `Result`。

```text
B {
    let! x = m
    return f x
}
```

既存 `src/computation.rs::Lowering::block` により:

```text
B.Run (B.Delay (u ->
    B.Bind m (x ->
        B.Return (f x))))
```

`Option` では:

```text
match m with
| Some x -> Some (f x)
| None -> None
```

`Result` では:

```text
match m with
| Ok x -> Ok (f x)
| Error e -> Error e
```

#### `let!`

```text
B { let! x [: T] = value; C }
```

- `value` は左から右の通常評価で一度だけ評価する。
- `Option`: `value : Option T`、成功 payload を `x : T` に束縛する。
- `Result`: `value : Result T E`、成功 payload を `x : T` に束縛し、`E` をブロック全体の error 型へ単一化する。
- `value` が `None`/`Error e` のとき継続 `x -> C` は呼ばれない。
- `let! mut x` は継続内のローカル `x` を可変にするだけで、外側の可変状態共有を許可しない。

#### `do!`

```text
B { do! value; C }
```

- `value` は `Option unit` または `Result unit E`。
- 成功時だけ `C` を実行する。
- `do!` の payload は必ず `unit`。`Option i64`／`Result i64 E` を渡すと既存の型エラー。

#### `return` / `return!`

```text
B { return value }
B { return! value }
```

- `return value` は成功値を作る。
  - `Option.Return value = Some value`
  - `Result.Return value = Ok value`
- `return! value` は既に builder 結果である値をそのまま返す。
  - `Option.ReturnFrom : Option 'a -> Option 'a`
  - `Result.ReturnFrom : Result 'a 'e -> Result 'a 'e`
- `return`/`return!` は早期脱出ではなく、その computation block の末尾だけに書ける。これは既存 parser の規則を維持する。

#### 通常 `let`

```text
B {
    let x = side_effect_free_or_trapping_expression
    let! y = value
    return x + y
}
```

- 通常 `let` は builder に渡さず、同じブロック内の通常束縛として展開する。
- 右辺はソース順に必ず評価される。後続の `let!` が `None`/`Error` でも、先に書いた通常 `let` の評価・トラップは省略されない。

#### 通常式

```text
B {
    assert condition
    return value
}
```

- 通常式文は `let _: unit = expression` として扱う。
- expression は必ず実行され、型は `unit` を要求する。
- `assert false` は従来どおりトラップであり、`None`/`Error` には変換しない。

#### `if`

```text
B {
    if condition {
        C1
    } else {
        C2
    }
    C3
}
```

- `condition` は通常評価で一度だけ評価し、`bool` を要求する。
- 選ばれた分岐だけが builder 展開された計算として実行される。
- `else` 省略時は `B.Zero()` を補う。
- 後続 `C3` がある場合、分岐結果は `B.Combine branch (B.Delay (u -> C3))` に入る。
- `Option.Zero : Option 'a` なので、`Option { if flag { return 1 } }` は `Option i64` として成立しうる。
- `Result.Zero : Result unit 'e` なので、`Result { if flag { return 1 } }` は `Ok 1` と `Ok ()` が合わず `E1003`。`else { return ... }` を書く。

#### `for`

```text
B {
    for pattern in values do C
    return value
}
```

- 既存展開は `B.For values (fx element -> match element with | pattern -> C)`。
- B01 phase 1 の `Option.For`/`Result.For` は `Copy 'a => ['a]` の配列だけを受ける。
- `values` は一度だけ評価する。
- 各要素は添字順に処理し、最初の `None`/`Error` で後続要素と `return value` を実行しない。
- パターン不一致は通常の `match` と同じくトラップであり、`None`/`Error` には変換しない。
- 非 Copy 要素の配列反復は C07 まで対象外。

#### `while`

```text
B {
    while condition do C
    return value
}
```

- `condition` は各反復の前に `unit -> bool` thunk として評価する。
- body は `B.Delay (unit -> C)` として `B.While` に渡る。
- `condition` が false なら `B.Zero()` 相当の成功 `unit` で終了し、後続へ進む。
- body が `None`/`Error` を返した時点で条件の再評価と後続を実行しない。
- `condition` のトラップは `None`/`Error` へ変換しない。

### 変換

- `Option.to_result error option` は `Some a -> Ok a`, `None -> Error error`。
- `Result.to_option result` は `Ok a -> Some a`, `Error _ -> None`。
- これらは明示呼び出しだけ。builder が暗黙に変換しない。

### 診断

- 新しい診断コードは追加しない。
- 代表例:
  - builder 不在・操作不在: `E1018`
  - payload 型不一致: `E1003`
  - `None`/`Error` の型文脈不足: `E1015`
  - 非 Copy 配列を `For` に渡す phase 1 制約違反: 既存の制約／move 診断（実装結果に合わせてテスト固定）

## 設計

### コンパイラ変更の最小化

B02 の基本機能は B01 の std ソースだけで成立する。`src/computation.rs` の展開規則は変更しない。

ただし、品質目標として以下を確認し、必要なら `src/call_specialization.rs` の解析を補強する。

- `Option.Bind`/`Result.Bind` の第 2 引数が外へ逃げない場合、`Specializations::new` が eligible と判定できる。
- `Delay` は `fn Delay body = body` なので `call_specialization::transparent` で透過できる。
- `Run` は `body ()` なので、`Run (Delay thunk)` が余分な closure allocation を残さないことを IR で確認する。
- `Combine` の第 2 引数は delayed tail であり、`None`/`Error` の場合に呼ばれないことを IR と E2E のトラップ回避で確認する。

### ゼロコスト期待の IR

手書き:

```text
match first with
| Some x ->
    match second x with
    | Some y -> Some (x + y)
    | None -> None
| None -> None
```

builder:

```text
Option {
    let! x = first
    let! y = second x
    return x + y
}
```

`-O0` の IR でも以下を満たすことを目標にする。

- `first` は一度だけ評価。
- `second` は `first` が `Some` の場合だけ呼ばれる。
- 継続 closure の heap allocation がない（既知継続の stack/worker 特殊化）。
- `None` 経路で `x + y` の除算・添字・関数呼び出しなどが存在しない、または到達不能。
- union payload の drop は active case のみ。

### `Result` error 型の単一化

次は受理:

```text
Result {
    let! a = Ok 20
    let! b = if a > 0 then Ok 22 else Error "bad"
    return a + b
}
```

次は拒否:

```text
Result {
    let! a = Error "bad"
    let! b = Error 1
    return a + b
}
```

期待診断は `E1003`。`string` と `i64` の error 型を暗黙に合流しない。

### performance benchmark

`benchmarks/run-computations.mjs` の既存方針に合わせ、速度を CI 合否にしない。

- workload:
  1. `option_builder_success`: `Option { let! ... }` 成功を 262,144 回。
  2. `option_builder_failure`: 先頭 `None` で後続の expensive call を呼ばない。
  3. `result_builder_error`: `Error` payload を move して返す。
  4. `manual_match_*`: 同じ意味の手書き `match`。
- 記録:
  - optimized IR の `@tz.alloc`/`@tz.closure` 出現数。
  - native `-O3` の中央値。
  - C++ ではなく手書き Tsuzuri match との比較を主、必要なら C++ は参考。

## 実装手順

1. **B01 の std builder 操作を固定する**
   - `Option`/`Result` に `Delay` と `Run` が両方あることを確認する。
   - `Combine` が delayed tail を必要時だけ呼ぶことを Rust テストで確認する。
   - 確認: `cargo test --locked --test computations result_propagation_operations_exist`（`running N tests` の N が 0 でないこと）。
2. **展開の snapshot テストを追加する**
   - `tests/computations.rs` に `Option { let! ... }` と `Result { let! ... }` の受理例を追加する。
   - `llvm::emit` の決定性と WASM IR 生成を確認する。
   - 確認: `cargo test --locked --test computations option_result_builder_expansion`（`running N tests` の N が 0 でないこと）。
3. **短絡の実行テストを追加する**
   - `/tmp` ではなく `tests/fixtures/option_result/` または `tests/fixtures/computations/` に E2E 関数を追加する。
   - `let! _: unit = None; return 1 / 0` と `let! _: unit = Error 42; return 1 / 0` がトラップせず失敗値を返すことを確認する。
   - 確認: `cargo build --release --locked && node tests/option_result.mjs target/release/tsuzuri`。
4. **`if`/`for`/`while` の伝播を確認する**
   - `if` の選択されない分岐が実行されない。
   - `for` が最初の失敗で後続要素を実行しない。
   - `while` が body 失敗後に condition を再評価しない。
   - 確認: カウンターではなくトラップ回避で検査する（例: 失敗後の body に `1 / 0`）。
5. **型エラーのテストを固定する**
   - `Result` error 型不一致、`Option`/`Result` 混在、`None`/`Error` の推論不足を拒否。
   - 確認: `cargo test --locked --test computations result_builder_rejects_mismatched_error_types`（`running N tests` の N が 0 でないこと）。
6. **特殊化と allocation を検査する**
   - `tests/call_specialization.rs` に builder と手書き match の IR 形を追加。
   - `Option` scalar 成功経路に `@tz.alloc` がないこと。
   - known continuation に `@tz.specialized.` が出ること。
   - 確認: `cargo test --locked --test call_specialization option_result_builders_specialize_continuations`（`running N tests` の N が 0 でないこと）。
7. **benchmark を追加する**
   - `benchmarks/run-computations.mjs` に workload を追加し、`--quick` では小さい入力だけ検査する。
   - `docs/benchmarks.md` に測定手順と、性能は合否でないことを追記。

## テスト計画

### 受理プログラム

```text
export def option_chain :: i64
fn option_chain =
    match Option {
        let! a = Some 20
        let! b = Some 22
        return a + b
    } with
    | Some value -> value
    | None -> -1
```

```text
export def option_stop :: i64
fn option_stop =
    match Option {
        let! _: unit = None
        return 1 / 0
    } with
    | Some value -> value
    | None -> 42
```

```text
export def result_chain :: i64
fn result_chain =
    match Result {
        let! a = Ok 20
        let! b = Ok 22
        return a + b
    } with
    | Ok value -> value
    | Error error -> error
```

```text
export def result_stop :: i64
fn result_stop =
    match Result {
        let! _: unit = Error 42
        return 1 / 0
    } with
    | Ok value -> value
    | Error error -> error
```

```text
export def result_explicit_conversion :: i64
fn result_explicit_conversion =
    match Result {
        let! value = Option.to_result 42 (Some 20)
        return value + 22
    } with
    | Ok value -> value
    | Error error -> error
```

### 拒否プログラム

| プログラム | 期待コード | 理由 |
|---|---|---|
| `Result { let! x = Error "bad"; let! y = Error 1; return 0 }` | `E1003` | error 型が `string` と `i64` で不一致 |
| `Option { let! x = Error "bad"; return x }` | `E1003` | `Option.Bind` は `Result` を受けない |
| `Result { let! x = None; return x }` | `E1003` | 暗黙の `Option`→`Result` 変換なし |
| `let x = Option { return None }` | `E1003` または `E1015` | `return` は payload を包むので `Option (Option 'a)` になる。型文脈不足なら推論不足 |
| `Result { if true { return 1 } }` | `E1003` | `Result.Zero` は `Result unit 'e` |
| `Option { for s in ["a"] do do! Some (); return 1 }` | 既存の Copy 制約診断 | phase 1 の `For` は `Copy 'a` |

### E2E 期待値

- native/WASM × `-O0`/`-O3`:
  - `option_chain() == 42`
  - `option_stop() == 42`
  - `result_chain() == 42`
  - `result_stop() == 42`
  - `result_explicit_conversion() == 42`
  - `option_for_stop(10) == 3`（3 番目で `None`、4 番目以降の `1/0` に到達しない）
  - `result_while_stop(10) == 5`（5 回目で `Error 5`、次の condition/body に到達しない）
- heap tracking:
  - 所有 string error payload を伝播しても `live == 0`。
  - `Error` で捨てられる未使用 `Ok` payload が drop される。
- deterministic IR:
  - `--emit llvm` を 2 回実行して一致。
  - intrinsic 宣言重複なし。
  - WASM imports なし。

## ドキュメント

- `docs/language.md`:
  - 「コンピュテーション式」に `Option`/`Result` の早期伝播例を追加。
  - `return` は関数脱出ではなく builder の結果生成であり、早期脱出は `Bind` が継続を呼ばないことで起きると明記。
  - `?` 演算子はないことを D-10 と一致させる。
- `docs/architecture.md`:
  - `src/computation.rs` の既存展開と std builder の関係を追記。
  - 既知継続特殊化の確認項目に `Option`/`Result` を追加。
- `README.md`:
  - `Result { let! ... }` の短い例を追加。
- `docs/benchmarks.md`:
  - builder vs hand-written match の比較手順と限界を追加。

## 受け入れ条件

- [ ] `Option { let! ... }` が `None` で後続を呼ばない。
- [ ] `Result { let! ... }` が `Error` で後続を呼ばない。
- [ ] `return`/`return!`/`do!`/通常 `let`/通常式/`if`/`for`/`while` の展開規則がドキュメントと一致する。
- [ ] `Result` error 型の不一致が安定した既存診断で拒否される。
- [ ] `Option` と `Result` の相互変換は明示関数だけで、暗黙変換しない。
- [ ] 既知継続の特殊化により、scalar payload の代表ケースで不要な heap closure allocation がない。
- [ ] native/WASM × `-O0`/`-O3`、heap tracking `live == 0`、WASM import なしを確認した。
- [ ] benchmark は追加されているが、速度閾値を CI 合否にしていない。

## 落とし穴

- `return` は早期 return ではない。`if flag { return 1 }; return 2` のような形は `Combine` の型に従う。
- `Result.Zero` は `Ok ()` であり、任意の成功型を作らない。
- `Option.Zero` は多相 `None` なので、文脈がないと型が決まらない。
- `do!` は `unit` payload 専用。値を取り出すなら `let!`。
- `for` は B01 phase 1 では Copy 配列だけ。通常 loop の `for...in` と同じ非 Copy 借用反復だと思わない。
- `Delay` があっても、builder 実装が body を即時呼び出せば遅延しない。std `Option`/`Result` では `Delay` は恒等で、`Run` と `Combine` が呼び出しタイミングを決める。
- トラップは `Error` に変換されない。`assert false`, ゼロ除算、範囲外アクセスは従来どおりプロセス／WASM trap。

## 対象外

- `?` 演算子。
- `try`/`use`/`and!`/`match!` などの追加 CE 構文。B05 の担当。
- `Task` builder との自動統合。`Task (Result 'a 'e)` の合成は B06。
- error 型の subtyping、暗黙変換、標準 error 階層。
- 非 Copy 配列・リスト・ユーザー定義 iterator の builder `For`。C07 の担当。

## 未決事項

- `Option.For`/`Result.For` の phase 1 制約違反診断は、A02/A06 の実装後に `Copy` 制約エラーとして `E1005` か `E1016` かが決まる。既定案は既存の型クラス制約不足と同じ診断に合わせ、テストは実装後に固定する。
- `Result { if flag { return 1 } }` を便利にするため `Zero : Result 'a 'e` を導入する案は、error payload を作れないため採用しない。
- `Option`/`Result` builder の `While` は再帰関数で実装する。大きなループの性能が問題なら、後続チケットで std builder `While` の直接 lowering を検討するが、このチケットでは通常関数＋末尾再帰最適化に任せる。
