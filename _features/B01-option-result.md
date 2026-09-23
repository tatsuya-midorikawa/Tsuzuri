# B01: Option／Result 標準型
| 項目 | 内容 |
|---|---|
| ID | B01 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | A02, E02 |
| 後続 | B02, B04, C02, C06, C07, D01, D02, D04, A08, B06 |
| 状態 | todo |
| 主な影響ファイル | `std/Option.tc`, `std/Result.tc`, `src/check.rs`, `src/computation.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `tests/computations.rs`, `tests/fixtures/computations/`, `tests/computations.mjs`, `examples/computations/`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

予期できる失敗を例外ではなく値で表す標準の `Option 'a`／`Result 'a 'e` を追加する。これにより、検索・解析・境界検査付き変換・タスク失敗などを型で表し、後続の B02（早期伝播）、B04（Option 返却アクティブパターン）、D01（Parse）、B06（Task.parallel_results）で共通の失敗表現を使えるようにする。

このチケットでは型・基本関数・コンピュテーション式ビルダー操作を標準ライブラリとして提供する。`?` 演算子や例外は導入しない（D-10）。

## 現状

- `docs/language.md` の「組み込み関数」は `assert` などの部分関数をトラップとして扱うが、回復可能な失敗型はない。
- `docs/language.md` の「コンピュテーション式」は `.tc` ビルダーに `Bind`／`Return`／`ReturnFrom`／`Zero`／`Combine`／`Delay`／`Run`／`For`／`While` を実装できると説明している。
- `src/computation.rs`:
  - `OPERATIONS` は `Bind`, `Return`, `ReturnFrom`, `Yield`, `YieldFrom`, `Zero`, `Combine`, `Delay`, `Run`, `For`, `While` を収集する。
  - `collect` は `.tc` に少なくとも一つの操作があることを検査し、操作名を `Names.builders` に入れる。
  - `expand` は型検査前に `ExprKind::Computation` を通常の関数呼び出しへ変換する。
  - `Lowering::block` は `let!`/`do!` を `B.Bind value (x -> tail)`、`return` を `B.Return value`、`return!` を `B.ReturnFrom value`、空本体を `B.Zero()`、`for` を `B.For source (fx element -> body)`、`while` を `B.While (unit -> condition) (B.Delay (unit -> body))` へ下げる。
  - `Lowering::delay` は `Delay` がある場合だけ `B.Delay (unit -> body)` を挿入し、`Run` があれば全体を `B.Run delayed` で包む。
- `src/call_specialization.rs`:
  - `Specializations::new` は非 escaping な関数引数を固定点で判定する。
  - `target`, `transparent`, `can_borrow`, `request` により、既知の継続・`Delay` のような恒等関数・単一利用の捕捉を直接 worker へ特殊化できる。
  - 追加 worker は最大 1,024。上限後は通常の関数値経路へ戻る。
- `src/check.rs`:
  - 現在の `Type` は `Record(usize)` までで、A02 後に `Type::Union(usize, Vec<Type>)` が追加される前提。
  - `Names` は `builders`, `records`, `record_aliases`, `functions`, `active_patterns` を持つ。E02 後は標準ライブラリの型・ケース・ビルダーを常に読み込む前提。
  - `Builtin` は `Task.run`／`Task.parallel` を修飾名組み込みとして持つが、`Option`／`Result` は組み込みではなく std ソースで実装する。
- `src/llvm.rs`:
  - `emit_target` は到達した関数だけを出力し、IR は決定的に生成する。
  - `FunctionEmitter::expression_mode`, `drop_value`, `clone_value` は型ごとの clone/drop を担う。A02 の union 追加後はここへ union のタグ分岐 clone/drop が入る。
- 既存 fixture:
  - `tests/fixtures/computations/Choice.tc` は `record Result { valid: bool, value: i64 }` で短絡する疑似 Result を実装している。
  - `tests/fixtures/computations/Flow.tc` は `Delay`/`Run`/`Combine`/`For`/`While` の既存展開を検査する。
  - `examples/computations/Checked.tc` は `record Result { ok: bool, value: i64 }` で割り算失敗を表すが、型汎用ではない。

### 前提とする他チケットのインターフェース

- **A01**: 型適用は D-02 の前置並置。`Option i64`, `Result i64 string`, `[Option i64]` を受理する。
- **A02**: D-05 の union を実装済み。`union Option 'a = None | Some of 'a`、`union Result 'a 'e = Ok of 'a | Error of 'e` を `.tc` 内で宣言できる。`Type::Union(usize, Vec<Type>)`、union ケース構築、union パターン、タグ付き LLVM 表現、union payload の clone/drop が実装済み。
- **E02**: D-07 の標準ライブラリ同梱機構を実装済み。`std/Option.tc` と `std/Result.tc` は常にユーザーモジュールと一緒に検査され、未使用関数は IR に出力されない。
- **E01**: E02 の依存として `private` がある想定。ただしこのチケットの提案ソースは実装容易性を優先し、公開して問題ない補助だけで構成する。

### 他チケットへの提供インターフェース

- 型:
  - `Option 'a = None | Some of 'a`
  - `Result 'a 'e = Ok of 'a | Error of 'e`
- ケース名:
  - `None`, `Some`, `Ok`, `Error` は D-05/D-07 に従い、同名がユーザー側で曖昧でなければ無修飾でも解決される。
  - 明示時は `Option.None`, `Option.Some`, `Result.Ok`, `Result.Error` を使う。
- ビルダー:
  - `Option { ... }` と `Result { ... }` は B02 が使う。
  - `Bind`, `Return`, `ReturnFrom`, `Zero`, `Combine`, `Delay`, `Run`, `For`, `While` を提供する。
- 代表関数:
  - `Option.map`, `Option.bind`, `Option.filter`, `Option.default_value`, `Option.default_with`, `Option.or_else`, `Option.to_result`, `Option.of_result`
  - `Result.map`, `Result.map_error`, `Result.bind`, `Result.default_value`, `Result.or_else`, `Result.to_option`, `Result.of_option`

## 仕様

### 型と構文

```text
union Option 'a = None | Some of 'a
union Result 'a 'e = Ok of 'a | Error of 'e

Type        ::= ... | "Option" TypeAtom | "Result" TypeAtom TypeAtom
Pattern     ::= ... | "None" | "Some" Pattern | "Ok" Pattern | "Error" Pattern
Expression  ::= ... | "None" | "Some" Expression | "Ok" Expression | "Error" Expression
BuilderExpr ::= "Option" "{" ComputationBody "}"
              | "Result" "{" ComputationBody "}"
```

- `Option 'a` は値がないことを `None`、値があることを `Some payload` で表す。
- `Result 'a 'e` は成功値を `Ok payload`、予期できる失敗を `Error payload` で表す。
- `Option`, `Result`, `Some`, `None`, `Ok`, `Error` は標準ライブラリの型／ケースであり、新しいキーワードではない。
- 関数名は D-01 に従い snake_case、型とケースは PascalCase。
- 関数は他モジュールから常に `Option.map` のように修飾して呼ぶ。ケースは A02 のケース解決規則で無修飾も許可する。

### 評価順序

- すべての関数適用は既存規則どおり callee → 第 1 引数 → 第 2 引数 ... の左から右。
- `Option.default_value fallback option` と `Result.default_value fallback result` は `fallback` を先に評価する。遅延評価が必要なら `default_with` を使う。
- `Option.or_else option fallback` と `Result.or_else result fallback` は `option`/`result` を先に評価し、`None`/`Error` の場合だけ `fallback ()` を呼ぶ。
- `map`, `bind`, `filter`, builder `Bind`, `Combine`, `For`, `While` は継続や述語を必要な場合だけ呼ぶ。
- `Option.get None`、`Result.get (Error e)`、`Result.get_error (Ok v)` は `assert` と同じくトラップする部分関数として文書化する。トラップは回復可能な失敗ではない。
  実装は網羅的な `match` と、GUIDE D-21 の発散する組み込み関数 `unreachable : unit -> 'a` を使う（A03 の網羅性検査と両立させるため、非網羅 match に頼らない）。

### 型規則

- `Some value : Option T` if `value : T`。
- `None : Option T` は文脈から `T` を推論する。推論できない場合は既存の `E1015`。
- `Ok value : Result T E` if `value : T`。
- `Error error : Result T E` if `error : E`。`T` は文脈から推論する。
- `Option.Bind : Option 'a -> ('a -> Option 'b) -> Option 'b`
- `Result.Bind : Result 'a 'e -> ('a -> Result 'b 'e) -> Result 'b 'e`
- `Result` builder の全 `let!`/`return!`/`do!`/`Combine`/`For`/`While` は同じ `'e` に単一化される。暗黙の error 型変換はしない。
- `Option.For` と `Result.For` の phase 1 は配列のみ、かつ要素は `Copy 'a` を要求する。通常の `for...in` と同等の借用反復プロトコルは C07 の担当。
- 公開 ABI は E05 まで変更しない。`Option`/`Result` を `export def` の引数・戻り値にすると既存の `E1008`。

### 所有権・借用

- `Option 'a`/`Result 'a 'e` は union payload の性質に従う。
  - 全 payload が Copy なら Copy。
  - payload のいずれかが `needs_drop` なら union も `needs_drop`。
  - inactive case の payload は未初期化またはゼロ化領域であり、drop/clone しない。
- `is_some`, `is_none`, `is_ok`, `is_error` は `&Option 'a`／`&Result 'a 'e` を受け取り、payload を move しない。
- `map`, `bind`, `default_value`, `get`, `to_result`, `to_option` は入力を消費する。
- `map_ref`, `bind_ref`, `filter` は `&Option 'a` または `&Result 'a 'e` の payload を共有借用で扱うが、phase 1 の結果 payload は所有値に限る。
  `Option &T` や `Result &T E` のように借用を結果へ残す使い方は、関数引数と `&Option`／`&Result` の 2 つが loan-carrying input になり、`Signature::validate_borrows` の寿命省略規則で `E1013` として拒否する。借用結果を安全に返す API は A09 の名前付きライフタイムまで延期する。
- `Result.map_error` は `Error e` の error payload だけを消費し、`Ok a` はそのまま返す。
- union payload からの部分 move、payload を含む drop、match 失敗時の一時値解放は A02 の union パターン規則と `src/ownership_control.rs::eval_match` の既存パターン一時値規則に従う。

### LLVM IR の形

A02 の union 表現を前提に、このチケットでは std 関数を通常関数として単相化するだけでよい。

- `Option i64` の代表形:
  ```llvm
  ; A02 の規則。型名は実装側の決定的 mangling に従う。
  %"tz.union.Option.Option[i64]" = type { i32, [8 x i8] }
  ; tag 0 = None, tag 1 = Some
  ```
- `Result i64 %tz.string` の代表形:
  ```llvm
  %"tz.union.Result.Result[i64,%tz.string]" = type { i32, [16 x i8] }
  ; tag 0 = Ok, tag 1 = Error
  ```
- `Option.map f option` はタグ分岐を生成し、`Some` の場合だけ `f` を呼ぶ。
- `Result.bind r next` は `Ok` の場合だけ `next` を呼び、`Error` の payload は move してそのまま返す。
- `Option { let! x = a; return f x }` は `Option.Bind a (x -> Option.Return (f x))` へ展開後、`src/call_specialization.rs::Specializations::new` と `FunctionEmitter::call` により既知継続が特殊化されることを IR で確認する。
- 既知継続が外へ逃げない場合、IR に不要な `%tz.closure` clone/drop と `@tz.alloc` が出ないことを目標にする。ただし payload が所有値で結果へ move される場合の必要な確保・drop は維持する。
- `Delay` は `fn Delay body = body` なので `call_specialization::transparent` の恒等関数扱いに乗る。

## 設計

### `std/Option.tc` の完全な提案ソース

```text
union Option 'a =
    | None
    | Some of 'a

def is_some :: &Option 'a -> bool
fn is_some option =
    match option with
    | Some _ -> true
    | None -> false

def is_none :: &Option 'a -> bool
fn is_none option = ! (is_some option)

def get :: Option 'a -> 'a
fn get option =
    match option with
    | Some value -> value
    | None -> unreachable ()

def default_value :: 'a -> Option 'a -> 'a
fn default_value fallback option =
    match option with
    | Some value -> value
    | None -> fallback

def default_with :: (unit -> 'a) -> Option 'a -> 'a
fn default_with fallback option =
    match option with
    | Some value -> value
    | None -> fallback ()

def map :: ('a -> 'b) -> Option 'a -> Option 'b
fn map transform option =
    match option with
    | Some value -> Some (transform value)
    | None -> None

def map_ref :: (&'a -> 'b) -> &Option 'a -> Option 'b
fn map_ref transform option =
    match option with
    | Some value -> Some (transform (&value))
    | None -> None

def bind :: Option 'a -> ('a -> Option 'b) -> Option 'b
fn bind option next =
    match option with
    | Some value -> next value
    | None -> None

def bind_ref :: &Option 'a -> (&'a -> Option 'b) -> Option 'b
fn bind_ref option next =
    match option with
    | Some value -> next (&value)
    | None -> None

def filter :: (&'a -> bool) -> Option 'a -> Option 'a
fn filter predicate option =
    match option with
    | Some value ->
        if predicate (&value) then Some value else None
    | None -> None

def or_else :: Option 'a -> (unit -> Option 'a) -> Option 'a
fn or_else option fallback =
    match option with
    | Some value -> Some value
    | None -> fallback ()

def to_result :: 'e -> Option 'a -> Result.Result 'a 'e
fn to_result error option =
    match option with
    | Some value -> Result.Ok value
    | None -> Result.Error error

def of_result :: Result.Result 'a 'e -> Option 'a
fn of_result result =
    match result with
    | Result.Ok value -> Some value
    | Result.Error _ -> None

def Return :: 'a -> Option 'a
fn Return value = Some value

def ReturnFrom :: Option 'a -> Option 'a
fn ReturnFrom option = option

def Bind :: Option 'a -> ('a -> Option 'b) -> Option 'b
fn Bind option next = bind option next

def Zero :: Option 'a
fn Zero = None

def Delay :: (unit -> Option 'a) -> (unit -> Option 'a)
fn Delay body = body

def Run :: (unit -> Option 'a) -> Option 'a
fn Run body = body ()

def Combine :: Option unit -> (unit -> Option 'a) -> Option 'a
fn Combine first rest =
    match first with
    | Some _ -> rest ()
    | None -> None

def For :: Copy 'a => ['a] -> ('a -> Option unit) -> Option unit
fn For values body = for_loop values body 0

private def rec for_loop :: Copy 'a => ['a] -> ('a -> Option unit) -> i64 -> Option unit
fn rec for_loop values body index =
    if index == values.length then Some ()
    else
        match body values[index] with
        | Some _ -> for_loop values body (index + 1)
        | None -> None

def rec While :: (unit -> bool) -> (unit -> Option unit) -> Option unit
fn rec While guard body =
    if guard () then
        match body () with
        | Some _ -> While guard body
        | None -> None
    else Some ()
```

### `std/Result.tc` の完全な提案ソース

```text
union Result 'a 'e =
    | Ok of 'a
    | Error of 'e

def is_ok :: &Result 'a 'e -> bool
fn is_ok result =
    match result with
    | Ok _ -> true
    | Error _ -> false

def is_error :: &Result 'a 'e -> bool
fn is_error result = ! (is_ok result)

def get :: Result 'a 'e -> 'a
fn get result =
    match result with
    | Ok value -> value
    | Error _ -> unreachable ()

def get_error :: Result 'a 'e -> 'e
fn get_error result =
    match result with
    | Error error -> error
    | Ok _ -> unreachable ()

def default_value :: 'a -> Result 'a 'e -> 'a
fn default_value fallback result =
    match result with
    | Ok value -> value
    | Error _ -> fallback

def default_with :: (unit -> 'a) -> Result 'a 'e -> 'a
fn default_with fallback result =
    match result with
    | Ok value -> value
    | Error _ -> fallback ()

def map :: ('a -> 'b) -> Result 'a 'e -> Result 'b 'e
fn map transform result =
    match result with
    | Ok value -> Ok (transform value)
    | Error error -> Error error

def map_ref :: Copy 'e => (&'a -> 'b) -> &Result 'a 'e -> Result 'b 'e
fn map_ref transform result =
    match result with
    | Ok value -> Ok (transform (&value))
    | Error error -> Error error

def map_error :: ('e -> 'f) -> Result 'a 'e -> Result 'a 'f
fn map_error transform result =
    match result with
    | Ok value -> Ok value
    | Error error -> Error (transform error)

def bind :: Result 'a 'e -> ('a -> Result 'b 'e) -> Result 'b 'e
fn bind result next =
    match result with
    | Ok value -> next value
    | Error error -> Error error

def bind_ref :: Copy 'e => &Result 'a 'e -> (&'a -> Result 'b 'e) -> Result 'b 'e
fn bind_ref result next =
    match result with
    | Ok value -> next (&value)
    | Error error -> Error error

def or_else :: Result 'a 'e -> (unit -> Result 'a 'e) -> Result 'a 'e
fn or_else result fallback =
    match result with
    | Ok value -> Ok value
    | Error _ -> fallback ()

def to_option :: Result 'a 'e -> Option.Option 'a
fn to_option result =
    match result with
    | Ok value -> Option.Some value
    | Error _ -> Option.None

def of_option :: 'e -> Option.Option 'a -> Result 'a 'e
fn of_option error option =
    match option with
    | Option.Some value -> Ok value
    | Option.None -> Error error

def Return :: 'a -> Result 'a 'e
fn Return value = Ok value

def ReturnFrom :: Result 'a 'e -> Result 'a 'e
fn ReturnFrom result = result

def Bind :: Result 'a 'e -> ('a -> Result 'b 'e) -> Result 'b 'e
fn Bind result next = bind result next

def Zero :: Result unit 'e
fn Zero = Ok ()

def Delay :: (unit -> Result 'a 'e) -> (unit -> Result 'a 'e)
fn Delay body = body

def Run :: (unit -> Result 'a 'e) -> Result 'a 'e
fn Run body = body ()

def Combine :: Result unit 'e -> (unit -> Result 'a 'e) -> Result 'a 'e
fn Combine first rest =
    match first with
    | Ok _ -> rest ()
    | Error error -> Error error

def For :: Copy 'a => ['a] -> ('a -> Result unit 'e) -> Result unit 'e
fn For values body = for_loop values body 0

private def rec for_loop :: Copy 'a => ['a] -> ('a -> Result unit 'e) -> i64 -> Result unit 'e
fn rec for_loop values body index =
    if index == values.length then Ok ()
    else
        match body values[index] with
        | Ok _ -> for_loop values body (index + 1)
        | Error error -> Error error

def rec While :: (unit -> bool) -> (unit -> Result unit 'e) -> Result unit 'e
fn rec While guard body =
    if guard () then
        match body () with
        | Ok _ -> While guard body
        | Error error -> Error error
    else Ok ()
```

### コンパイラ段ごとの変更

| 段 | 変更 |
|---|---|
| lexer | 変更なし。`Option`, `Result`, `Some`, `None`, `Ok`, `Error` は識別子。 |
| parser | A01/A02/E02 の変更に依存。B01 固有の構文変更なし。`.tc` に `union` を許可するのは A02。 |
| computation::collect/expand | 変更なし。`std/Option.tc` と `std/Result.tc` が既存操作名を定義すれば `Option {}`/`Result {}` は自動的に展開される。 |
| check | A02 の `Type::Union` 対応と E02 の std fallback を利用する。B01 固有の組み込み型は追加しない。`Builtin` には GUIDE D-21 の `unreachable : unit -> 'a` を追加する（E02 の多相 builtin 機構を使い、型ごとの `@tz.builtin.unreachable.<mangled-T>(i8 %unit)` を `call void @llvm.trap()` + `unreachable` だけの本体で生成する。呼び出し側の制御フローは通常の一引数呼び出しのまま）。 |
| polymorph | A02 の union 型引数単一化を利用する。`Option.Return` などの多相関数は既存 `Scheme`/`Specializer` で単相化する。 |
| control | A02 の union パターンを利用する。std 内の match はすべて網羅的に書く（`unreachable ()` を使う）。 |
| closures | 変更なし。builder 継続は通常 lambda。 |
| recursion | `private for_loop` と `While` は `def rec`/`fn rec` として既存 `recursion::check` の対象。`For` は非再帰 wrapper として `for_loop` を呼ぶ。 |
| ownership | A02 の union clone/drop/move を利用する。`filter` は predicate に `&value` を渡すため、payload を消費せずに判定してから `Some value` へ move する。 |
| call_specialization | 変更なし。ただし IR テストで `Option.Bind`/`Result.Bind` の既知継続が `@tz.specialized.*` へ下がることを確認する。 |
| llvm/llvm_control/llvm_frame | A02 の union lowering を利用する。B01 固有の runtime は追加しない。 |
| runtime | 変更なし。 |
| driver/main | E02 の std 同梱機構以外は変更なし。 |

## 実装手順

1. **std ソースを追加する**
   - `std/Option.tc` と `std/Result.tc` を上記の内容で追加する。
   - E02 の std 埋め込みリストに 2 ファイルを追加する。
   - 確認: `cargo test --locked --test option_result`（`running N tests` の N が 0 でないこと）で `Option { return 1 }` と `Result { return 1 }` が、ユーザー側に `.tc` ファイルなしで解決される。
2. **A02 union との統合を確認する**
   - `Option i64`, `Result i64 string`, `[Option i64]`, `Result (Option i64) string` が `resolve_type` で正しく解決されることをテストする。
   - `Some`, `None`, `Ok`, `Error` の無修飾解決が std fallback で動き、同名ケースがユーザー側にある場合は A02 の曖昧性診断へ従う。
   - 確認: `tests/option_result.rs` で受理／曖昧名 `E1004`／未解決名 `E1004` を検査。
3. **基本関数の型・所有権を検査する**
   - `is_some`/`is_ok` が borrow を受けること、`map`/`bind` が所有値を消費すること、`filter` が payload を predicate 後に返せることを確認する。
   - `Option.get None` と `Result.get (Error e)` は型検査を通し、E2E でトラップを確認する。
   - 確認: `cargo test --locked --test option_result`（`running N tests` の N が 0 でないこと）。
4. **builder 操作を検査する**
   - `match Option { let! x = Some 20; return x + 22 } with | Some 42 -> true | _ -> false` が `true`。
   - `match Option { let! x = None; return x / 0 } with | None -> true | _ -> false` が `true` で、継続が呼ばれない。
   - `match Result { let! x = Ok 20; return x + 22 } with | Ok 42 -> true | _ -> false` が `true`。
   - `match Result { let! x = Error "bad"; return x / 0 } with | Error "bad" -> true | _ -> false` が `true` で、継続が呼ばれない。
   - `Delay`/`Run`/`Combine` により、`if` と `while` の後続が必要時だけ実行される。
   - 確認: `tests/computations.rs` に std builder ケースを追加。
5. **IR と特殊化を確認する**
   - スカラー payload の `Option`/`Result` builder に不要な heap allocation がないこと。
   - `Option.Bind`/`Result.Bind` 呼び出しが既知継続を使う箇所で `@tz.specialized.` を含むこと。
   - 同じ入力で `llvm::emit` と `llvm::emit_target(..., wasm = true)` が決定的であること。
   - 確認: `tests/call_specialization.rs` に `Option`/`Result` ケースを追加し、`cargo test --locked --test call_specialization`（`running N tests` の N が 0 でないこと）を実行する。
6. **E2E fixture を追加する**
   - `tests/fixtures/option_result/` を作り、`export def` で公開 ABI に収まる i64/bool 結果を返す。
   - native/WASM × `-O0`/`-O3`、トラップ、heap tracking `live == 0` を確認する `tests/option_result.mjs` を追加する。
   - 確認: `cargo build --release --locked && node tests/option_result.mjs target/release/tsuzuri`。
7. **既存 examples を更新する**
   - `examples/computations/Checked.tc` は削除する（または `Checked.example.disabled` など `.tc` 以外へ改名し、自動読み込み対象から外す）。`Checked` builder を残すと、標準 `Result` ではなく疑似 `record Result { ok, value }` の例が引き続き使われるため不可。
   - `examples/computations/Main.tz` の `divide` を `Result i64 i64`（error は例では `i64` の除数または固定コード）を返す関数へ書き換える。
   - `Checked { ... }` は `Result { ... }` に置き換える。
   - `answer.ok` / `answer.value` のフィールドアクセスは、`match answer with | Ok value -> value | Error _ -> -1` のような網羅的 `Ok`/`Error` match に置き換える。
   - ディレクトリ内に builder でない `.tc` が残らないことを確認する。必要な `.tc` は標準ライブラリ側の `std/Result.tc` だけ。
   - `tests/examples.mjs` の期待値は引き続き `42\n`（または Main.tz の新しい仕様に合わせた値）に固定し、`cargo build --release --locked && node tests/examples.mjs target/release/tsuzuri` で検証する。

## テスト計画

### Rust テスト

- `tests/option_result.rs` を新規作成する。
- 受理:
  - `let x: Option i64 = Some 1`
  - `let x: Option i64 = None`
  - `let r: Result i64 string = Ok 1`
  - `let r: Result i64 string = Error "bad"`
  - `Option.map (n -> n + 1) (Some 41)`
  - `Option.filter (n -> *n > 0) (Some 1)`
  - `Result.map_error (s -> s + "!") (Error "bad")`
  - `Option.to_result "missing" (Some 42)`
  - `Result.to_option (Ok 42)`
  - `Option { let! x = Some 20; return x + 22 }`
  - `Result { let! x = Ok 20; return x + 22 }`
  - `Option { for x in [1, 2] do do! Some (); return 42 }`
  - `Result { while false do do! Ok (); return 42 }`
- 拒否:
  - `let x = None` -> `E1015`
  - `let x = Error "bad"` -> `E1015`
  - `Result { let! x = Ok 1; return! Error 2 }` where error type cannot unify -> `E1003`
  - `Option { for s in ["a"] do do! Some (); return 1 }` in phase 1 if `string` is non-Copy -> `E1005` or `E1012`（実装の Copy 制約診断に合わせて固定）
  - `export def bad :: Option i64` -> `E1008`
  - `let option = Some 1\nlet borrowed: Option &i64 = Option.map_ref (r -> r) (&option)\n0` -> `E1013`
  - `let result = Ok 1\nlet borrowed: Result &i64 string = Result.bind_ref (&result) (r -> Ok r)\n0` -> `E1013`
- IR 不変条件:
  - `llvm::emit` が 2 回一致。
  - WASM `emit_target` が成功。
  - scalar-only builder に `@tz.alloc` が出ない。
  - known continuation ケースに `@tz.specialized.` が出る。

### E2E fixture

`tests/fixtures/option_result/Main.tz` に以下を含める。

```text
export def option_some :: i64
fn option_some =
    match Option.map (n -> n + 1) (Some 41) with
    | Some value -> value
    | None -> -1

export def option_none_short_circuit :: i64
fn option_none_short_circuit =
    match Option { let! x = None; return x / 0 } with
    | Some value -> value
    | None -> 42

export def result_ok :: i64
fn result_ok =
    match Result { let! x = Ok 20; return x + 22 } with
    | Ok value -> value
    | Error _ -> -1

export def result_error_short_circuit :: i64
fn result_error_short_circuit =
    match Result { let! x = Error 7; return x / 0 } with
    | Ok value -> value
    | Error error -> error * 6

export def option_loop :: i64 -> i64
fn option_loop count =
    match Option {
        for n in new [i64](count, i -> i) do
            do! if n < 0 then None else Some ()
        return count
    } with
    | Some value -> value
    | None -> -1

export def result_loop :: i64 -> i64
fn result_loop count =
    match Result {
        for n in new [i64](count, i -> i) do
            do! if n < 0 then Error n else Ok ()
        return count
    } with
    | Ok value -> value
    | Error error -> error

export def trap_option_get :: i64
fn trap_option_get = Option.get None

export def trap_result_get :: i64
fn trap_result_get = Result.get (Error 1)
```

`tests/option_result.mjs`:

- `check`、`--emit header`、`--emit llvm` を実行。
- IR を 2 回生成して一致。
- native `-O0`/`-O3` で C host から呼び、期待値を JavaScript/BigInt 側で独立計算:
  - `option_some() == 42`
  - `option_none_short_circuit() == 42`
  - `result_ok() == 42`
  - `result_error_short_circuit() == 42`
  - `option_loop(257) == 257`
  - `result_loop(257) == 257`
- `@malloc`/`@free` を `@tracked_alloc`/`@tracked_free` に置換し、各呼び出し後 `live == 0`。
- `trap_option_get`, `trap_result_get` は native 子プロセス異常終了、WASM `WebAssembly.RuntimeError`。
- WASM `-O0`/`-O3` は `WebAssembly.Module.imports(module) == []`、結果一致、`memory.buffer.byteLength <= 16 MiB`。

### 性能確認

- `benchmarks/run-computations.mjs` に `Option`/`Result` builder と手書き `match` の matched workload を追加する。
- 測るもの:
  - scalar `Option` success/failure ループ。
  - owned string payload の success/failure。
  - `Result` error propagation。
- 合否閾値は置かない。IR の allocation count と中央値を `docs/benchmarks.md` に記録する。

## ドキュメント

- `docs/language.md`:
  - 「型とメモリ」に `Option 'a`／`Result 'a 'e` を追加。
  - 「組み込み関数」または新しい「標準ライブラリ」節に `Option`/`Result` 関数一覧を追加。
  - 「コンピュテーション式」に `Option {}`／`Result {}` の例を追加し、`?` はないことを明記。
  - 「診断」の表は新コードなし。既存 `E1003`, `E1015`, `E1008` を使う。
- `docs/architecture.md`:
  - 標準ライブラリ同梱後の std 型として `Option`/`Result` を追記。
  - union clone/drop と builder 特殊化の IR 不変条件を記載。
  - 検証コマンドに `cargo build --release --locked && node tests/option_result.mjs target/release/tsuzuri` を追加。
- `README.md`:
  - 未実装一覧から「代数的データ型がないため失敗型がない」趣旨を更新。
  - 短い `Result { let! ... }` の例を追加。
- `docs/benchmarks.md`:
  - 性能を主張する場合だけ、測定日・環境・コミット・中央値・生データ保存先を追記。

## 受け入れ条件

- [ ] `std/Option.tc` と `std/Result.tc` が std 埋め込みで常に読み込まれる。
- [ ] `Option`/`Result` 型、ケース、関数、builder 操作が仕様どおり型検査される。
- [ ] `Option {}`/`Result {}` の `let!` が `None`/`Error` で継続を呼ばない。
- [ ] `Result` builder の error 型が全 `let!`/`return!` 間で単一化され、暗黙変換しない。
- [ ] `Option.get None` と `Result.get (Error e)` が native/WASM でトラップする。
- [ ] 組み込み `unreachable : unit -> 'a` が任意の型で使え（`def f :: string` / `fn f = unreachable ()` なども型検査を通る）、呼ぶと native/WASM × -O0/-O3 でトラップする。`unreachable` はユーザー関数名として再定義できない（`E1001`）。std の match はすべて網羅的で、A03 実装後も `E1021` にならない。
- [ ] 所有 payload の move/drop/clone が正しく、E2E の heap tracking が各呼び出し後 `live == 0`。
- [ ] scalar builder の既知継続が特殊化され、不要な closure allocation がないことを IR テストで確認した。
- [ ] native/WASM × `-O0`/`-O3`、WASM import なし、IR 決定性を確認した。
- [ ] README、`docs/language.md`、`docs/architecture.md`、必要なら `docs/benchmarks.md` を更新した。

## 落とし穴

- `default_value` は eager。高価またはトラップしうる fallback には `default_with` を使うよう文書化する。
- `Result.Zero` は `Ok ()`。`if condition { return value }` の else 省略で任意の `'a` を作れるわけではない。
- `Option.Zero` は `None` なので多相だが、型文脈がなければ `E1015`。
- `For` は phase 1 では `Copy 'a => ['a]`。通常の `for...in` のような非 Copy 要素の借用反復ではない。
- `Delay` を定義する場合は `Run` も必要。`Delay` だけだと builder 全体が `unit -> Option 'a` になってしまう。
- `Combine` の第 1 引数は `Option unit`／`Result unit 'e`。値を返す `return` はブロック末尾に置く。
- `Option.get`/`Result.get`/`Result.get_error` は非網羅 `match` ではなく、GUIDE D-21 の `unreachable ()` を使った網羅的 `match` で書く。
- `map_ref`/`bind_ref` は phase 1 では所有 payload を返す用途に限る。payload への共有借用を返り値に隠す使い方は、関数引数と `&Option`／`&Result` の両方が loan-carrying input になるため `E1013` で拒否し、借用結果 API は A09 まで延期する。
- std 関数を `Builtin` にしない。組み込みにすると A02 の union 経路と性能・意味がずれる。

## 対象外

- `?` 演算子、例外、巻き戻し、任意 destructor。
- `Option`/`Result` の `Eq`/`Ord`/`Display`/`Hash`/`Default` deriving。A07/D01 の担当。
- `Task.parallel_results`。B06 の担当。
- `Parse` クラスや `to_string`。D01 の担当。
- ユーザー定義反復プロトコルに基づく `For`。C07 の担当。
- 公開 ABI で `Option`/`Result` を直接渡すこと。E05 まで対象外。

## 未決事項

- **決定済み（GUIDE D-21）:** 任意型を返す部分関数は、網羅的な `match` と発散する組み込み関数 `unreachable : unit -> 'a` で実装する。
  bottom 型・`panic : string -> 'a`・std 限定の非網羅許可は採用しない（メッセージ付きの失敗は G04 のトラップ位置報告で補う）。
  `unreachable` の追加は B01 の作業に含める。E02 が未完了で多相 builtin が使えない場合は、B01 に着手しない（E02 は依存）。
- `For` の phase 1 を `Copy 'a => ['a]` に限定する。非 Copy payload の借用反復は C07 の反復プロトコル設計に合わせて拡張する。
- `map_ref`/`bind_ref` の命名は `map_borrow`/`bind_borrow` も候補。既定案は D-01 の簡潔さを優先して `map_ref`/`bind_ref`。
