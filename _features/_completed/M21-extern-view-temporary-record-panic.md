# M21: `extern view` の書き込みに一時の record を渡すとコンパイラが panic する

| 項目 | 内容 |
| --- | --- |
| ID | M21 |
| 優先度 | P1（`check` が通るプログラムで、`build`・`run`・`test` がコンパイラの panic（終了コード 101）になる。回避策は `let` で束縛してから `ref` で渡すことだが、panic の文 `polymorphism is resolved before LLVM` からはそれが分からない） |
| 規模 | S |
| 依存 | – |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「調査中に見つけたコンパイラの不具合 B1（自分で再現済み）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 不要（`check` が受理するプログラムを正しく下げる修正。新しい名前・診断・ABI を足さない）。台帳は [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22)（承認の要らない M21 はすぐに進めてよい） |
| 手本にする既存実装 | `src/llvm_imports.rs` の `FunctionEmitter::host_call`（`BorrowOperand` の引数を `operand_borrow` で一時の領域に置き、呼び出しの後に `release_operands` で解放する）、`src/llvm_parallel.rs` の `parallel_scope`（同じ 2 つを 1 つの引数に使う）、完了した [W01](_completed/W01-callback-extern-temporary-string.md)（`host_call` の経路の同じ種類の修正）と [A27](_completed/A27-call-argument-reference-hint.md) の 5 番（`parallel_expression` の同じ種類の修正） |
| 主な影響ファイル | `src/llvm_imports.rs`、`tests/extern_view.rs`、`tests/extern_view.mjs` |

## 実装者向け早見

- 範囲: `extern view def w :: i64 -> ref R -> unit`（書き込みのビュー。W24）の呼び出しで、第 2 引数が一時値の借用（`TypedExprKind::BorrowOperand`）のとき。record のリテラル、関数の結果、`{ p with … }`、`if` の式、`ref` を書かない値（暗黙の借用）が当たる。
- 原因: `FunctionEmitter::call` は、ビューの呼び出しを、`BorrowOperand` を扱う汎用の分岐より前に `extern_view` へ送る。`extern_view` の書き込みの分岐は第 2 引数を `self.expression` で読む。`emit_expression_mode` は `BorrowOperand` を `unreachable!` にしている。
- 直し方: 書き込みの分岐で、第 2 引数が `BorrowOperand` なら `operand_borrow` で一時の領域に置いて借用し、`write_host_record` の後に `release_operands` で解放する。`host_call` と同じ 2 つの関数を使う。それ以外の形は今のまま。
- 最初に読む: 「現状」の表 → `src/llvm.rs` の `FunctionEmitter::call`（7536 行。7541-7547 行がビューの分岐、7556-7561 行が汎用の分岐）→ `src/llvm_imports.rs` の `extern_view`（35 行。51 行が第 2 引数）→ `host_call` の 123-127 行と 423・449 行 → `src/llvm.rs` の `operand_borrow`（7501-7524 行）・`release_operands`（7526-7534 行）。
- 最初に書くテスト: `tests/extern_view.rs` の `temporary_records_reach_extern_view_writes`（一時値の形が IR を作り、`let` の形の IR との差が解放の `store` 1 行だけ）。
- 触ってはいけない: 場所を渡す形（`ref p`・`ref p.field`・定数）の IR。読むほうのビュー。アドレスと record の評価順。NULL と整列の検査（`host_pointer`）の位置。
- 完了の見分け方: 次のプログラムの `build --emit llvm` が panic せずに IR を出す（今は `src/llvm.rs:6087:17` の panic）。

```tsuzuri
@c_layout
record P { x: i32, y: i32 }

@trusted
extern view def write_p :: i64 -> ref P -> unit

def main :: unit -> i32 = \() ->
    write_p 16 (ref (P { x: 1, y: 2 }))
    0
```

## 目的

`extern view` の書き込みは、ホストが渡したアドレスへ C の配置の record を複写する（W24）。書き込む値をその場で組み立てて渡す書き方（`write_p addr (ref (P { … }))`、`write_p addr (make_point n)`）は、ほかの関数と `extern` の呼び出しでは動く。ところが `extern view` では、`check` は通るのに、`build` がコンパイラの内部の誤り（Rust の panic）で止まる。利用者が受け取るのは `internal error: entered unreachable code: polymorphism is resolved before LLVM` という文だけで、何を直せばよいか分からない。

このチケットは、`extern view` の書き込みでも一時値の借用を、ほかの呼び出しと同じ規則（呼び出しの間だけ一時の領域に置き、終わったら解放する）で下げる。あわせて、同じ形の見落とし（汎用の分岐より前に独自に引数を下げる経路）がほかにないかを洗う。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M21`。承認は要らない（D-67 で M21 はすぐに進めてよい）。
- 「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。行番号は `grep -n` で取り直す（下の `grep` が 1 件ずつ当たること）。食い違えば、このチケットを直してから始める。

```sh
grep -n 'fn call(&mut self, callee' src/llvm.rs
grep -n 'if let Some(direction) = import.view' src/llvm.rs
grep -n 'let source_ref = self.expression(&arguments\[1\]);' src/llvm_imports.rs
grep -n 'unreachable!("polymorphism is resolved before LLVM")' src/llvm.rs
```

### 停止条件

- **S1** 今通るプログラムの IR が 1 byte でも変わる。対象は `tests/fixtures`・`examples` の `Main.tz`（native と `--target wasm32`、`--emit llvm`）、「現状」の t6・t7。
- **S2** 修正が `extern_view` の外（`FunctionEmitter::call` の分岐の順序、`check` の `temporary_borrows`、所有権の検査）に及ぶ。範囲を広げずに止めて報告する。
- **S3** 洗い出し（手順 4）で、`extern view` のほかにも同じ panic に届く経路が見つかり、その修正が `operand_borrow`・`release_operands` を足すだけでは済まない。直さずに報告する（別のチケットの候補）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

開発版 `target/debug/tsuzuri`（HEAD で作り直したもの。macOS 27.2.1 arm64）で確かめた。作業は `target/scratch/m21/` で行い、確認後に消した。表の各行は、次の宣言の後の `main` の本体である。

```tsuzuri
@c_layout
record P { x: i32, y: i32 }

@trusted
extern view def write_p :: i64 -> ref P -> unit

def make :: i32 -> P = \n -> P { x: n, y: n + 1 }
```

| # | `main` の本体（最後は `0`） | `check` | `build --emit llvm` |
| --- | --- | --- | --- |
| t1 | `write_p 16 (ref (P { x: 1, y: 2 }))`（早見のプログラム） | 終了コード 0 | panic（終了コード 101） |
| t2 | `write_p 16 (P { x: 1, y: 2 })`（`ref` を書かない暗黙の借用） | 0 | 同じ panic |
| t3 | `write_p 16 (ref (make 3))` | 0 | 同じ panic |
| t4 | `let p = P { x: 1, y: 2 }` / `write_p 16 (ref { p with x = 5 })` | 0 | 同じ panic |
| t5 | `let c = true` / `write_p 16 (ref (if c then P { x: 1, y: 2 } else P { x: 3, y: 4 }))` | 0 | 同じ panic |
| t6 | `let p = P { x: 1, y: 2 }` / `write_p 16 (ref p)` | 0 | IR を出す |
| t7 | トップレベルの `const ORIGIN: P = P { x: 0, y: 0 }` の後に `write_p 16 (ref ORIGIN)` か `write_p 16 ORIGIN` | 0 | IR を出す（純粋なデータの定数は静的な場所に畳まれる） |
| t8 | `ref (P { x: 1, y: 2 }) \|> write_p 16` | `E1008`（kind `extern_view_call_form`、`extern view 'write_p' must be called directly with all its arguments`） | – |
| t9 | t1 を `--target wasm32` で `build` | – | `E1008`（kind `abi_native_only`。LLVM の前に止まる） |

t1 の出力（`run` も同じ。`RUST_BACKTRACE=1` の上の枠は `emit_expression_mode` ← `expression_mode` ← `expression` ← `extern_view` ← `call`）:

```text
$ tsuzuri check t1; echo $?
0
$ tsuzuri build t1 --emit llvm -o out.ll; echo $?

thread 'main' (6227150) panicked at src/llvm.rs:6087:17:
internal error: entered unreachable code: polymorphism is resolved before LLVM
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
101
```

t6 の IR（`main` の本体の要部）は、record を `alloca` の領域に `store` し、`inttoptr i64 16`、NULL の検査（`icmp ne ptr`）、整列の検査（`and i64 …, 3`）の後に、領域から `load` して field ごとに `store i32 …, align 4` する。

### 原因（コード）

- `src/llvm.rs:7536` の `FunctionEmitter::call`: 7537-7553 行が「callback externs and extern views have no function body and lower inline」の分岐である。呼び出し先の本体が `HostCall` で `import.view` があれば、7541-7547 行で `extern_view(direction, arguments, …)` を返す。引数のどれかが `BorrowOperand` なら `comparison_call` へ送る汎用の分岐は、その後ろの 7556-7561 行にある。
- `src/llvm_imports.rs:35` の `extern_view`: 書き込みの分岐（49 行から）は、アドレスを `self.expression(&arguments[0])`（50 行）、record の参照を `self.expression(&arguments[1])`（51 行）で読む。第 2 引数の `BorrowOperand` は、そのまま `self.expression` に渡る。
- `src/llvm.rs:6059` の `emit_expression_mode`: `TypedExprKind::BorrowOperand(_)` の腕（6084 行）は、型引数の残る形と同じ腕で `unreachable!("polymorphism is resolved before LLVM")`（6087 行）になる。`BorrowOperand` は呼び出しの側が自分で下げる約束で、式としては下げない。
- 同じ約束を守っている側: `host_call` は `if let TypedExprKind::BorrowOperand(operand) = &argument.kind { self.operand_borrow(argument, operand, &mut operands) }`（`src/llvm_imports.rs:123-127`）で借用し、呼び出しの後の 2 か所（423・449 行）で `self.release_operands(operands)` を呼ぶ。`operand_borrow`（`src/llvm.rs:7501-7524`）は、場所ならその場所を、それ以外なら値を `spill` した領域を返し、解放の記録を積む。`release_operands`（7526-7534 行）は値を drop し、領域を `zeroinitializer` で埋める。
- `BorrowOperand` を作るのは `check` の側である。`src/constants.rs` の `Checker::temporary_borrows`（中の `make_operand`）が、全引数を一度に適用する呼び出しの、場所でない実引数の `Borrow(value, false)` を `BorrowOperand(value)` にする。純粋なデータの定数は静的な場所として畳まれるので、t7 は通る。`ref` を書かない t2 も、引数の型 `ref P` への暗黙の借用で同じ形になる。
- ビューには単独の関数の本体がない（`src/llvm.rs:2152-2156` が `import.callbacks || import.view.is_some()` の関数を飛ばす）。したがって、ビューの呼び出しを汎用の分岐（`comparison_call`）へ回すと、定義のない `@tz.fn.Main.write_p` を呼ぶ IR になる。W01 が直す前のコールバックの extern と同じ失敗である（「設計」の採らない案）。

### 同じ形の経路（起票時の洗い出し）

`rg -n "self\.expression\(&arguments\[" src/llvm*.rs` は 14 件で、起票時に次のとおり分けた。実装の手順 4 で HEAD に対して繰り返す。

| 場所 | 扱い |
| --- | --- |
| `src/llvm_imports.rs:43・50・51`（`extern_view`） | 43・50 行はアドレス（`i64`）で借用にならない。51 行がこの不具合 |
| `src/llvm.rs:7625`（恒等関数の呼び出し） | 汎用の分岐（7556 行）の後ろなので、`BorrowOperand` の引数は届かない |
| `src/llvm_compare.rs:77-78`・`src/llvm_hash.rs:21`・`src/llvm_display.rs:552`（`StructuralCompare`・`StructuralHash`・`StructuralDisplay`） | 引数はコンパイラが作る本体の仮引数で、`temporary_borrows` を通らない |
| `src/llvm_parallel.rs:623・625・640`（`parallel_scope`。610 行） | 623 行は 620 行の `BorrowOperand` の検査（`operand_borrow`。TZ11）の else の枝。625 行は個数（`i64`）、640 行はコールバックで、借用にならない |
| `src/llvm_parallel.rs:683・698・700`（`parallel_chunks`。681 行。`Parallel.for_each_chunk`） | 引数は大きさ（`i64`）・コールバック・排他のスライス（`ref mut ['a..]`）。`make_operand` が `BorrowOperand` にするのは共有の借用（`Borrow(value, false)`）だけなので、届かない |

ほかに `BorrowOperand` を受ける所は、`comparison_call`（7472 行）、`Binary(BinaryOp::Pipe, …)` の枝（6287 行）、`src/llvm_display.rs:343`（文字列の穴）、`src/llvm_parallel.rs:455`（`parallel_expression`。A27）で、どれも `operand_borrow` を使う。自分を末尾で呼ぶ再帰（`emit_tail` の `tail_arguments`）に一時値を渡す形（`def rec walk :: ref P -> i64 -> i64` の中の `walk (ref (P { … })) (n - 1)`、`count (String.repeat 2 "ab") (n - 1)`）も試し、どちらも `build` が通った。

## 変えてはいけない意味

- 引数は左から右に一度だけ評価する。アドレスの式を先に、record の式を後に評価する（今の 50・51 行の順）。
- NULL と整列の検査（`host_pointer`。違反は呼び出しの位置の `BoundsCheck` のトラップ）は、両方の引数を評価した後、書き込みの前に行う（今と同じ）。
- 借用した一時値は、書き込みが終わった直後に解放する（ほかの呼び出しと同じ規則。W01 の「変えてはいけない意味」）。トラップしたときはプロセスが終わるので、解放しない。
- 書き込みは field ごとで、パディングのバイトを上書きしない（W24）。
- 読むほうのビュー（`i64 -> R`）と、場所を渡す書き込み（t6・t7）の IR。

## 仕様

1. `extern view` の書き込みの第 2 引数が一時値の借用（`BorrowOperand`）なら、値を呼び出しの間だけ一時の領域に置き、その領域を書き込みの元にする。書き込みの後に、一時値を解放する。
2. 第 2 引数が場所（`let` の束縛、field、`ref` で受けた引数、畳まれた定数）なら、今と同じくその場所を借りる。
3. 評価順、検査の位置、書き込みの仕方は「変えてはいけない意味」のとおりで、今と同じ。
4. 診断は足さない。t8 の `E1008`（`extern_view_call_form`）と t9 の `E1008`（`abi_native_only`）は今と同じ。
5. 結果: t1〜t5 が IR を出し、実行すると書いた値をホストの領域に残す。t6・t7 の IR は 1 byte も変わらない。

## 設計

`src/llvm_imports.rs` の `extern_view` の書き込みの分岐だけを変える。

```rust
crate::syntax::ViewDirection::Write => {
    let address = self.expression(&arguments[0]);
    // M21: a borrowed temporary lives in a slot until the record is written, as in `host_call`.
    let mut operands = Vec::new();
    let source_ref = if let TypedExprKind::BorrowOperand(operand) = &arguments[1].kind {
        self.operand_borrow(&arguments[1], operand, &mut operands)
    } else {
        self.expression(&arguments[1])
    };
    // … `record_ty`、`inttoptr`、`host_pointer`、`load`、`write_host_record` は今のまま …
    self.release_operands(operands);
    "0".into()
}
```

- `operand_borrow` は `ref R`（スライスでない）について、場所なら `self.place(operand)`、それ以外なら `spill` した領域を返す。どちらも `ptr` なので、続く `load {ty}, ptr {source_ref}` はそのまま使える。
- `operand_borrow`・`release_operands` は `src/llvm.rs` の `impl FunctionEmitter` の非公開の関数で、`host_call` が同じ子モジュールから呼んでいる。可視性は変えない。
- `operands` が空（場所を渡した形）なら `release_operands` は何も出さないので、t6・t7 の IR は変わらない。

### 起票時の試作

起票時に、HEAD のソースの複製（`target/scratch/` の下。確認後に消した）に上の変更を当てて開発版を作り、次を確かめた。HEAD のコードには入っていない。

- t1〜t5 の `build --emit llvm` が通った。
- t1 の IR と t6 の IR の差は、field の `store` の後の `store %tz.record.Main.P zeroinitializer, ptr %v4` の 1 行（解放）だけだった。
- 下の E2E のプログラムが `-O0`・`-O3` の両方で終了コード 94 を返した。
- `tests/fixtures` の 152 個の `Main.tz`（native の `--emit llvm`）と `examples` の 9 個（native と `--target wasm32`）の IR が、HEAD と 1 byte も変わらなかった。
- `cargo test --test extern_view --test trusted --test c_layout --test extern_ref_mut` がすべて通った。

レビューでも、HEAD のソースの複製（`git archive HEAD`、`target/scratch/` の下。確認後に消した）に同じ変更を当てた開発版で確かめ直した。t1〜t5 の `build --emit llvm` が通り、t1 と t6 の IR の差は同じ 1 行で、t6・t7 の IR は HEAD と同じだった。E2E のプログラムは `-O0`・`-O3` で 94 を返した。`tests/fixtures` の 152 個のうち 150 個の IR と、`examples` の 9 個（native と `--target wasm32`）の IR が HEAD と同じだった。残る 2 個（`domain`・`domain_host`）は、HEAD でも試作でも `--emit llvm` が同じ `E1038`（`--emit llvm needs --trap-mode return`）になり、IR を出さない。

### 採らない案

- **`FunctionEmitter::call` の分岐の順序を入れ替え、ビューの呼び出しも汎用の分岐へ送る**: `comparison_call` は `@tz.fn.Main.write_p` を呼ぶが、ビューには単独の本体がない（`src/llvm.rs:2152-2156`）。panic が Clang の IR の検証の `E2002` に変わるだけである。
- **`check` で一時値を拒否する（`E1008` の新しい kind）**: `check` が受理し、ほかの `extern` が受理する形を、ビューだけで拒否することになる。規則が増え、利用者は `let` を書き足すだけの回避を強いられる。
- **`emit_expression_mode` に `BorrowOperand` の腕を足し、式として一時の領域を返す**: 解放の時点（呼び出しの後）を知る者がいなくなり、一時値が漏れるか、早すぎる解放になる。`BorrowOperand` は呼び出しの側が下げる約束を保つ。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- なし。M07 などが `extern view` の書き込みを生成するときも、一時値を渡す書き方がそのまま動く。

## 実装手順

1. **変更**: なし（基準の記録）。
   **内容**: 着手条件の `grep`。HEAD の開発版で、`tests/fixtures`・`examples` の `Main.tz` を `build --emit llvm`（native、`examples` は `--target wasm32` も）にして `target/scratch/m21/before/` に保存する。IR を出さないもの（起票時は `tests/fixtures/domain`・`domain_host` が `E1038`）は、診断の文と終了コードを保存する。
   **確認**: 保存した数を控える。
2. **変更**: `tests/extern_view.rs`（テスト計画の Rust）。
   **内容**: 新しいテストを先に書く。
   **確認**: `cargo test --locked --test extern_view temporary_records` が `running 1 test` で失敗する（panic）。
3. **変更**: `src/llvm_imports.rs`（設計）。
   **内容**: 書き込みの分岐の `operand_borrow` と `release_operands`。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test extern_view`。手順 1 の IR と `cmp` で一致する（S1）。
4. **変更**: なし（洗い出し）。
   **内容**: `rg -n "self\.expression\(&arguments\[" src/llvm*.rs` と `rg -n "BorrowOperand" src/llvm*.rs` を HEAD で繰り返し、「現状」の表と比べる。`FunctionEmitter::call` の汎用の分岐（今の 7556 行）より前に足された特別扱いの経路（W25 以降のチケットが足したものを含む）を 1 つずつ開き、引数を `self.expression` で読むものがあれば、t1 と同じ形の一時値で `build` を試す。
   **確認**: 結果を完了報告の「見つけた問題」に書く。panic する経路が見つかり、同じ 2 つの関数を足すだけで直るなら、このチケットで直してテストを足す。そうでなければ S3。
5. **変更**: `tests/extern_view.mjs`（テスト計画の E2E）。
   **内容**: native の `-O0`・`-O3` の段。
   **確認**: `cargo build` の後に `node tests/extern_view.mjs target/debug/tsuzuri`。T1、T2。`target/scratch/m21/` を消す。

## テスト計画

### Rust（`tests/extern_view.rs`）

`temporary_records_reach_extern_view_writes`: 既存の `inline_lowering_effects_audit_and_wasm` と同じ `EmitOptions`（`Entry::Library`、`wasm: false`）で IR を作る。ソースは「現状」の宣言に、`export def f :: i64 -> i32 = \_a ->` の本体として t1〜t7 の行（書き込みの後に `0`）を 1 つずつ足したもの。アドレスは定数 `16` のままにし、仮引数を使わない。

- 一時値の形: t1（record のリテラル）、t2（`ref` なし）、t3（関数の結果）、t4（`{ p with x = 5 }`）、t5（`if` の式）。どれも `llvm::emit_with_options` が `Ok` を返す（今は panic する）。
- t1 と t6 の IR の差: 2 つの IR を行に分け、t1 の側にだけある行がちょうど 1 行で、`store %tz.record.Main.P zeroinitializer, ptr ` で始まる（`release_operands` を通った印）。それ以外の行は同じ順で一致する。期待値は設計から手で書き、コンパイラの出力を写さない（GUIDE-LITE §2 規則 10）。
  - レビューの試作で、この `export def` の形（入口のない module の `build --emit llvm`）でも差がこの 1 行だけになることを確かめた。
  - アドレスを仮引数（`write_p a (…)`）にすると、t1 は仮引数の `load` を record を組み立てる前に、t6 は `let` の後に行うので、差は 1 行にならない（試作で確かめた）。評価順のとおりの違いで、誤りではない。
  - `export def f :: unit -> i32` は使わない。`export` の C ABI の検査で `E1008` になる。
- t7（定数）の IR に、この解放の行がない。
- 2 回作った IR が同じ文字列になる（決定的）。
- wasm（`wasm: true`）では、t1 の module が今と同じく `DiagnosticKind::AbiNativeOnly` を返す。

### E2E（`tests/extern_view.mjs`）

既存の段の後に、一時のディレクトリの `Main.tz` を `build`（`-O0`・`-O3`、`--no-cache`）して実行する段を足す。期待値は手で数えた和（1 + 2 + 10 + 11 + 30 + 40 = 94）。

```tsuzuri
@c_layout
record P { x: i32, y: i32 }

@trusted
extern "calloc" def c_calloc :: i64u -> i64u -> i64

@trusted
extern view def read_p :: i64 -> P

@trusted
extern view def write_p :: i64 -> ref P -> unit

def shifted :: i32 -> P = \n -> P { x: n, y: n + 1i32 }

def main :: unit -> i32 = \() ->
    let base = c_calloc 4 8
    let c = true
    write_p base (ref (P { x: 1i32, y: 2i32 }))
    write_p (base + 8) (shifted 10i32)
    write_p (base + 16) (ref (if c then P { x: 30i32, y: 40i32 } else P { x: 0i32, y: 0i32 }))
    let a = read_p base
    let b = read_p (base + 8)
    let d = read_p (base + 16)
    a.x + a.y + b.x + b.y + d.x + d.y
```

- 終了コードが 94（`-O0`・`-O3`）。`calloc` の領域は解放しない。`free` は予約された symbol（`src/abi.rs` の `RESERVED_HOST_SYMBOLS`）で `extern` に宣言できないので、領域はプロセスが終わるまで使う。
- 書き込む値を `let` で束縛した版（t6 の形。最初の 2 つの書き込みだけで 1 + 2 + 10 + 11 = 24）が HEAD で `-O0`・`-O3` とも 24 を返すことは、起票時に確かめた。一時値の版は HEAD では `build` が panic する。

### 既存のテストへの影響

- `tests/extern_view.rs` の 3 つのテスト、`tests/extern_view.mjs` の既存の段は変わらない。
- `--backend fast` は `-O0` の IR を翻訳するので、同じ修正が効く。別のテストは足さない。

## ドキュメント

- 不要。`rg -n 'extern view' _tsuzuri` は `compiler/native-interop.md` の「ホストメモリの読み書きビュー（`extern view`）」の節（268 行から）だけに当たり、そこに一時値についての既知の不具合や制限は書かれていない（起票時に確かめた）。修正の後も、書いてあること（2 つの形、直接の全引数の適用、インライン展開、NULL と整列のトラップ）は正しい。PR の説明に、ページを変えない理由としてこれを書く。

## 受け入れ条件

- [ ] t1〜t5 の `build` が通り、E2E のプログラムが `-O0`・`-O3` で 94 を返す。
- [ ] t1 と t6 の IR の差が解放の `store` 1 行だけで、t6・t7 と手順 1 の集合の IR が 1 byte も変わらない。
- [ ] t8・t9 の診断が今と同じ。
- [ ] 手順 4 の洗い出しの結果を完了報告に書いた。
- [ ] `tests/extern_view.rs` の新しいテストと既存の全件、`node tests/extern_view.mjs target/debug/tsuzuri`、T1、T2 が通る。

## 落とし穴

- **解放を早めない**: `release_operands` は値を drop した後、領域を `zeroinitializer` で埋める。`load` より前に置くと、ホストの領域に 0 を書く。`host_call` と同じく、`write_host_record` の後に置く。
- **読むほうに足さない**: 読むビューの引数は `i64` のアドレスだけで、`BorrowOperand` にならない。対称にするつもりで分岐を足さない。
- **`self.expression` の分岐を消さない**: 場所を渡す形（t6）は `BorrowOperand` にならず、今の `self.expression`（場所の参照の値）で読む。両方の形を残す。
- **洗い出しの範囲**: `rg` の 14 件だけで終わらせない。手順 4 は、汎用の分岐より前に足された経路を、`FunctionEmitter::call` の先頭から読んで確かめる。

## 対象外

- `extern view` の型の規則（2 つの形、record の field の型、`@c_layout` の扱い）の変更。
- NS07 の `ExternAddr`（アドレスの型）。このチケットはアドレスを `i64` のまま扱う。
- `check` が一時値を受理する規則（`temporary_borrows`）の変更。
- 洗い出しで見つかった、同じ 2 つの関数を足すだけでは直らない経路の修正（S3。別のチケットの候補）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `extern_view` の書き込みの分岐の中で、`BorrowOperand` を `operand_borrow` で借用し、書き込みの後に `release_operands` で解放する。退けた案: `call` の分岐の順序を入れ替える（ビューに本体がなく `E2002` になる）、`check` で拒否する（受理していたプログラムを拒否し、ほかの `extern` と規則が分かれる）、`emit_expression_mode` で下げる（解放の時点が失われる） | `host_call`（W01）・`parallel_expression`（A27）と同じ形で、変更が 1 つの分岐に閉じる。場所を渡す形の IR を変えない | 既定案（実装者はこの案に従う） |
| D2 | 同じ形の経路の洗い出しは行い、同じ 2 つの関数を足すだけで直るものだけをこのチケットで直す | 小さな修正の範囲を保ちつつ、W01・A27 に続く 3 件目の見落としを繰り返さないため | 既定案（実装者はこの案に従う） |
| D3 | 言語リファレンスは変えない | 一時値についての既知の不具合の記述がなく、修正の後も今の記述が正しい | 既定案（実装者はこの案に従う） |
