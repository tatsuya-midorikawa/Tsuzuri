# E06: ホスト関数のインポート
| 項目 | 内容 |
|---|---|
| ID | E06 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E02 |
| 後続 | なし |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/llvm.rs`, `src/driver.rs`, `src/main.rs`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

Tsuzuri から host が提供する関数を呼び出せるようにする。

現状は公開 ABI で Tsuzuri 関数を host から呼ぶ方向だけを持ち、Tsuzuri 側から時刻・乱数・描画・ログなどの host API を呼ぶ方法がない。

D-18 に従い、WASM imports は明示的に使った場合だけ増やす。

副作用を持つ host call は最適化で削除・並べ替えない。

Phase 1 は同期関数・E05 ABI subset・明示 `extern def` のみに限定する。

## 現状

`docs/language.md` は「外部関数宣言はありません」「WASM は import を必要としません」と説明している。

`src/lexer.rs` に `extern` / `import` keyword はない。

`src/parser.rs` の top-level declaration は `record`, `class`, `instance`, `def`, `fn`, `let`, `export` を扱う。

`src/syntax.rs` の `FunctionDecl` は body を必ず持つ。

`src/check.rs` の `check_modules` は signature と definition を結合し、宣言だけの関数を `E0002` にする。

`src/llvm.rs` の function emission はすべて `@tz.fn.Module.name` の body を定義する。

`src/llvm.rs` の `emit_target` は WASM import を生成しない。

`src/driver.rs` の WASM link は `--no-entry` と explicit exports を渡すだけで import module 設定はない。

E02 後は builtin namespace と std modules が存在する。

E05 後は export/import に使える ABI classifier を共有できる。

## 仕様

### 構文

Phase 1 は `extern def` を採用する。

```text
extern def host_now :: unit -> i64
extern def host_log :: &string -> unit
extern def random_f64 :: unit -> f64
```

`extern` は新規予約語。

`import def` は採用しない。

理由は Rust/C 系の `extern` と近く、「外から来る宣言だけで body がない」意味を表しやすいため。

EBNF:

```text
extern-decl ::= "extern" "def" function-name "::" signature
```

`extern def` は body を持たない。

`fn host_now ...` を同じ module に書いたら `E1001`。

`export extern def` と `extern export def` は `E1022` または `E0002` ではなく `E1008` で拒否する。

host import は host から公開される Tsuzuri ABI ではないため `export` と併用しない。

`private extern def` は E01 の private と組み合わせても意味が薄いが、同一 module 内からだけ呼ぶ host import として許す。

ただし public 関数が private extern を呼ぶことは同一 module 内なので許す。

### 型

Phase 1 の allowed signature は E05 ABI subset と同じ。

Parameter:

- existing scalar.
- `&[i64]`, `&[f64]`, `&[ubyte]`.
- `&string`.
- scalar-only record by value or `&Record` if E05 completed.

Result:

- existing scalar.
- `unit`.
- `[i64]`, `[f64]`, `[ubyte]`, `string` only if E05 result ownership ABI is implemented for imports.

E06 の依存は E02 だけだが、E05 未完了環境では Phase 1 実装を scalar + `unit` + `&string` に限定してよい。

ただしチケットの最終仕様は E05 helper を再利用する形にする。

`Task`, function values, list, tuple, arbitrary record, owned array parameter, `&mut` は不可。

invalid signature は `E1008`。

### Symbol names

Native symbol name:

```text
tsuzuri_host_<module>_<name>
```

module path dots are replaced with `_`.

例: `extern def host_now` in module `Main` lowers to `declare i64 @tsuzuri_host_Main_host_now()`.

`extern def time.now` のような dotted function name は通常 function name ではないので不可。

WASM import module/name:

```text
module: "tsuzuri"
name:   "<Module>.<name>"
```

例: `Main.host_now` is imported as `{ module: "tsuzuri", name: "Main.host_now" }`.

JS instantiation:

```js
WebAssembly.instantiate(bytes, {
  tsuzuri: {
    "Main.host_now": () => 42n,
  },
});
```

Native object/exe link では host が `tsuzuri_host_Main_host_now` を提供する。

`--emit llvm` は `declare` を含むだけで link しないため LLVM installed 不要。

### WASM opt-in

WASM imports は extern function が到達可能な場合だけ出る。

未使用 extern は型検査するが、E02 の std pruning と同じく unused extern declaration は WASM import を増やさない。

ただし user function は未使用でも IR に出す既存方針があるため、root user function 内に extern call があれば import は出る。

`WebAssembly.Module.imports(module)` は extern 使用時だけ非空になる。

extern がない program は従来通り imports 空。

### Side effects and evaluation order

extern function は副作用ありとして扱う。

LLVM call には `readnone`, `readonly`, `speculatable`, `willreturn` などを付けない。

call を削除・共通化・再順序化する compiler pass を Tsuzuri 側で仮定しない。

callee expression と arguments は通常の左から右で評価する。

argument validation wrapper は parameter order で行う。

`let _ = host_log (&text)` を未使用だから削除してはならない。

`if flag { host_a() } else { host_b() }` は選ばれた側だけ呼ぶ。

`&&` / `||` の短絡は維持する。

### Tasks and parallel

task 内から extern function を呼ぶことは Phase 1 では許す。

Native `Task.parallel` では複数 pthread から host function が呼ばれる可能性がある。

docs は host function が thread-safe でなければ `Task.parallel` から呼ばないよう明記する。

WASM は現状 task parallel が逐次なので single thread。

将来 F06 WASM threads では host import thread-safety を再検討する。

host function が blocking しても Tsuzuri runtime は cancellation しない。

### Purity story

Tsuzuri は効果を型で追跡しない。

`extern def` は通常の関数型を持つが、言語仕様上は副作用を持ちうる。

optimizer は extern call を pure として扱わない。

標準ライブラリの pure API と混同しないため、extern は明示構文で宣言する。

### 使用禁止 context

compile-time const eval が D06 で入る場合、extern call は const context で禁止する。

deriving / type-level computation / pattern recognizer の compile-time 実行には使わない。

現在の Tsuzuri には const context がないため、Phase 1 では通常式からのみ呼べる。

### 前提とする他チケットのインターフェース

E02 の builtin/std infrastructure は直接使わないが、std module reservation と origin を保つ。

E05 が完了していれば `ExportAbi` classifier を `ImportAbi` として再利用する。

E05 未完了なら Phase 1 scalar-only subset で実装し、E05 後に buffer/record を有効化する。

### 他チケットへの提供インターフェース

E07 の WASM debug output opt-in は import generation の設計を参考にする。

F06 は tasks + imports の thread-safety docs を更新する。

`CheckedFunction` に body のない extern を表す field を追加する。

```rust
pub enum FunctionBody {
    Tsuzuri(TypedExpr),
    Extern(ExternFunction),
}

pub struct ExternFunction {
    pub native_symbol: String,
    pub wasm_module: String,
    pub wasm_name: String,
    pub abi: ExternAbi,
}
```

または最小変更として:

```rust
pub externed: Option<ExternFunction>
```

既定案は `FunctionBody` enum。body なし invariant を型で表すため。

## 設計

### AST

`TokenKind::Extern` を追加する。

`Lexer::identifier` に `"extern"` を追加する。

`FunctionDecl` は body 必須なので、extern 用宣言を分ける。

```rust
pub struct ExternDecl {
    pub name: Ident,
    pub visibility: Visibility,
    pub parameters: Vec<TypeExpr>,
    pub result: TypeExpr,
    pub constraints: Vec<ConstraintExpr>,
}

pub struct Program {
    pub externs: Vec<ExternDecl>,
    // existing...
}
```

Phase 1 では extern に typeclass constraints を許さない。

`extern def show :: Display 'a => ...` は `E1008` または `E1015` で拒否する。

理由は host ABI が concrete であり polymorphic extern は symbol を決められないため。

extern signature must be concrete after resolving type aliases.

### Parser

`Parser::program` に `Extern` 分岐を追加する。

`extern` の後は必ず `def`。

`extern fn` は `E0002`。

`extern def rec` は `E1019` ではなく `E0002`。extern は recursion body を持たない。

`extern def and` は不可。

`private extern def` は E01 parser と連携して visibility を設定する。

### Name collection

extern は通常関数 namespace に入る。

同じ module の `def` / `extern def` 重複は `E1001`。

extern は `CheckedFunction` の id を持ち、関数値として渡せる。

関数値として渡した extern を later call しても、最終 call は extern symbol に lower する。

extern を `export` できない。

### Type checking

extern signature は `resolve_type` する。

型変数が残る場合は `E1015`。

constraints がある場合は Phase 1 `E1008`。

ABI classifier を通す。

extern body は `TypedExpr` を持たない。

`recursion::check` は extern を graph node として扱うが outgoing references はなし。

`ownership::check` は extern body を検査しない。

extern function call の arguments は通常 call と同じ ownership move/borrow rules。

### LLVM

Extern function body は `define` しない。

Native:

```llvm
declare i64 @tsuzuri_host_Main_host_now()
```

WASM:

```llvm
declare i64 @tsuzuri.import.Main.host_now()
```

LLVM IR だけでは import module/name が表現しにくい。

既定案は wasm target で symbol を `@tsuzuri_import_Main_host_now` とし、`wasm-ld --import-memory` ではなく LLVM object の undefined function import nameを使う。

より正確に module/name を固定するには LLVM IR の `wasm-import-module` / `wasm-import-name` attributes を使う。

E06 は attributes を生成する。

例:

```llvm
declare i64 @tsuzuri_import_Main_host_now() #0
attributes #0 = { "wasm-import-module"="tsuzuri" "wasm-import-name"="Main.host_now" }
```

FunctionEmitter call は `FunctionRef::User(id)` の function が extern なら symbol を extern symbol にする。

Partial application of extern is allowed through existing closure machinery.

完全適用 known call は direct extern call。

extern function pointer environment は既存 static function ref と同じ扱い。

### Driver

Native executable build with extern requires host object/library at link time, but Phase 1 CLI に `--host-object` は追加しない。

したがって `tsuzuri build --emit object` と `--emit llvm` を主用途にする。

`tsuzuri run` or `--emit exe` with unresolved extern will link fail with `E2002` unless user supplies symbols by future option.

Phase 1 で `--host-object PATH` を追加するかは未決だが、既定案は追加しない。

WASM build succeeds and leaves imports.

### Phase 1 fully specified subset

実装する最初の PR:

- `extern` keyword。
- concrete scalar/unit signatures。
- native LLVM declarations。
- WASM import attributes。
- direct call and function value call。
- E2E scalar WASM import test。
- native object/header test via C host manually linking object and host C.

E05 buffers/records は E05 完了後に同じ ABI classifier で拡張する。

## 実装手順

1. `extern` keyword と `ExternDecl` AST を追加する。
   - 確認: parser test で `extern def host_now :: unit -> i64` が parse される。

2. `Parser::program` に extern 分岐を入れる。
   - 確認: `extern fn`, `extern def rec`, `extern def and` を拒否。

3. `check_modules` で extern を function namespace に収集する。
   - 確認: 通常関数との duplicate は `E1001`。

4. concrete scalar/unit ABI validation を実装する。
   - 確認: `extern def id :: 'a -> 'a` は `E1015`、`extern def f :: string -> unit` は Phase 1 `E1008`。

5. `CheckedFunction` body representation を `FunctionBody` に変更する。
   - 確認: existing user functions の body path が壊れない。

6. recursion/ownership/closures/polymorph traversal を extern body なしに対応させる。
   - 確認: extern function value を引数に渡す test が通る。

7. LLVM call emission を extern symbol に対応させる。
   - 確認: `--emit llvm` に `declare` が出て `define @tz.fn.Main.host_now` は出ない。

8. WASM import attributes を生成する。
   - 確認: `WebAssembly.Module.imports(module)` が `{ module: "tsuzuri", name: "Main.host_now" }` を含む。

9. Native object E2E を追加する。
   - 確認: C host defines `int64_t tsuzuri_host_Main_host_now(void)` and links with Tsuzuri object.

10. Docs update。
    - 確認: WASM default importなしと extern使用時importありの差が説明される。

## テスト計画

### Rust accepted

`extern def host_now :: unit -> i64` を呼ぶ関数が typecheck する。

extern function を関数値として `apply Main.host_now ()` のように渡せる。

`private extern def helper :: unit -> i64` を同じ module から呼べる。

extern を未使用でも typecheck はされる。

### Rust rejected

`extern fn host_now() -> i64 { 1 }` は `E0002`。

`extern def rec host_now :: unit -> i64` は `E0002`。

`extern def host :: 'a -> 'a` は `E1015`。

`extern def host :: string -> unit` は Phase 1 `E1008`。

`export extern def host :: unit -> i64` は `E1008`。

同名 `def` と `extern def` は `E1001`。

### LLVM tests

Native library IR contains `declare i64 @tsuzuri_host_Main_host_now()`.

WASM IR contains import attributes.

Extern call is not marked readnone/readonly.

Unused extern not reachable from emitted function does not add WASM import after pruning where applicable.

### E2E

WASM fixture imports `Main.host_now` and returns host value + 1.

JS instantiate without import rejects; with import succeeds.

`WebAssembly.Module.imports` exactly matches expected module/name.

Native object fixture links host C object defining `tsuzuri_host_Main_host_now`.

native and WASM `-O0`/`-O3`。

## ドキュメント

`docs/language.md` に `extern def` と effect/purity story を追加する。

WASM imports は extern 使用時だけ増えると明記する。

host function thread-safety と `Task.parallel` からの呼び出し注意を書く。

`docs/architecture.md` の Host/WASM invariant に import opt-in を追加する。

`README.md` に WASM import JS example と native object link example を追加する。

## 受け入れ条件

- [ ] `extern def` が concrete host function declaration として使える。
- [ ] extern は通常関数 namespace に参加し、duplicate を拒否する。
- [ ] extern call は副作用ありとして IR に出る。
- [ ] WASM import module/name が安定している。
- [ ] extern がない WASM は imports 空のまま。
- [ ] scalar Phase 1 E2E が native object/WASM で通る。
- [ ] docs が effects, tasks, opt-in を説明する。

## 落とし穴

extern call に `readnone` を付けると LLVM が削除・並べ替えできてしまう。

WASM import attribute を付けないと module/name が toolchain 依存になる。

polymorphic extern を許すと symbol と ABI が決まらない。

native exe/run は host symbol を link できないため、Phase 1 では object/wasm を主対象にする。

`extern` keyword は D-15 に未記載なので台帳更新が必要。

## 対象外

async host imports。

callbacks。

host object link CLI。

WASI。

DOM/JS glue generator。

effect type system。

E05 buffer/record ABI の実装そのもの。

## 未決事項

native executable へ host object を渡す CLI `--host-object` を追加するかは未決。既定案は Phase 1 では追加しない。

E05 未完了時にどこまで ABI 型を許すかは未決。既定案は scalar/unit のみ。

台帳の見直し提案: D-15 の新規予約語一覧に `extern` を追加する必要がある。D-18 の opt-in 方針とは整合する。
