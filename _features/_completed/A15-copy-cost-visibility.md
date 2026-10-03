# A15: 暗黙の深いコピーの可視化

| 項目 | 内容 |
| --- | --- |
| ID | A15 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | G03, (G12) |
| 後続 | C10 |
| 状態 | done（Phase 1・2。Phase 2 の共有バッファによる O(1) の複製は C10 の範囲のまま） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D3 は、2026-10-03 に利用者から「A15 / A12 の実装を完遂して。複数フェーズある場合には、すべてのフェーズを完了させること」と依頼され、承認として扱った（`--warn implicit-copy` と `W1006` を確定。GUIDE D-16・D-33） |
| 改善する劣位 | Rust 比: Copy のコストモデルの違い（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 手本にする既存実装 | 全関数を同じ走査で 2 用途に使う形: `src/ownership.rs` の `check_functions`（`infer` フラグ）と `check_body`。警告の作り方と利用者コードだけの報告: `src/warnings.rs` の `unused_locals`（`Diagnostic::warning`）と `src/check.rs` の `check_modules_collect` での `ModuleOrigin::User` の判定。CLI の真偽フラグ: `src/main.rs` の `--deny-warnings`（`Arguments::deny_warnings`、重複の拒否、`fmt` との併用拒否、単体テスト）。CLI の統合テスト: `tests/warnings.rs` の `cli_caps_warnings_and_denies_before_touching_artifacts`。複製が生成される条件: `src/llvm.rs` の `FunctionEmitter::read_place`・`clones_on_take` |
| 主な影響ファイル | `src/copies.rs`（新規）, `src/lib.rs`, `src/ownership.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `src/main.rs`, `std/Array.tz`, `std/List.tz`, `tests/copy_cost.rs`（新規）, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/ownership.md`, `_docs/library-reference/arrays-and-lists.md`, `_docs/tools/diagnostics.md`, `_docs/tools/command-line.md`, `_docs/guides/performance.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

Rust の `Copy` はビット複製だが、Tsuzuri では全要素が Copy の配列・リストや、それらを含むレコード・タプルも Copy になり、
暗黙の複製に長さ比例の確保とコピーを伴う。言語の意味は変えずに、この複製が起きる位置を利用者が確認できるようにし
（既定無効の警告 `W1006`）、意図した複製は明示 API（`Array.copy`・`List.copy`）で書けるようにする。

同時に、コンパイラ内部に「暗黙の複製の一覧」（copy-site inventory）を作る。PM07（複製の除去）はこの一覧を入力として使い、
除去の対象と効果を数える。一覧は生成コードの `clone_value` 呼び出しと一致することを debug build で常に検査する。

## 着手条件と停止条件

### 着手条件

- G03（警告）が `_features/README.md` の状態欄で done であること（HEAD で done）。G12 は開始条件にしない（D7）。
  確認: `grep -nE "^\| (G03|G12|A15) " _features/README.md`。
- D3 が承認されるまでは、手順 1〜5（一覧・ライブラリ API・`Array.copy`／`List.copy`・debug 照合）だけを行い、手順 6（CLI）以降に着手しない。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR と stack-depth テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 手順 5 の debug 照合が不一致になり、一覧側の規則（D4）の修正だけでは一致させられない（生成コードや IR を変える必要がある）。
- 既存テストの期待値（IR、診断コード、メッセージ、警告の件数）を変える必要がある。
- 所有権検査の結果（エラーの有無・コード・位置）が記録の追加で変わる。記録は観測だけで、判定を変えてはならない。
- 記録のために `Checker::eval` やその呼び出し元の引数を広く変える必要がある（D4 は `Checker` への 1 field の追加で済む設計）。
- debug 照合で `cargo test --locked` 全体の実行時間が明らかに（2 割以上）増える。
- `Array.copy`／`List.copy` の追加で `honors_the_exact_specialization_limit` か stack-depth の 3 テストが失敗する。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- Copy の判定は `Type::is_copy`（`src/check.rs`）。`String`・`Utf8String`・`Vec`・`Task`・`ref mut`・型変数は false。配列・リストは要素、
  レコード・union・タプルは全 field／payload で決まり、それ以外（関数値を含む）は `_ => true`。`Type::needs_drop` は文字列・関数値・
  `Task`・配列・リスト・`Vec` と、それらを含む集成型で true。
- したがって実行時費用を伴う暗黙の複製は `is_copy && needs_drop` の型だけで起きる。Copy の配列・リスト、関数値、それらを含む
  レコード・タプル・union が該当する。`string` と `Vec` は Copy ではないので暗黙には複製されず移動する（`Vec.clone` は明示 API で、
  `src/check.rs` の `VecClone`、生成は `src/llvm_bulk.rs`）。
- 生成: `FunctionEmitter::read_place(expression, take, relocate)`（`src/llvm.rs`）は `take && needs_drop` かつ `clones_on_take` のとき
  `clone_value` を呼ぶ。`clones_on_take` は `is_copy` で、しかも「`TypedExprKind::Local(id)` で `id` が `single_use` にあり
  `borrowed_locals` にない」場合を除いて true。フィールド・要素・payload・参照外しからの取得は常に複製する。
- `single_use` は `FunctionEmitter::emit` が `call_specialization::single_use_locals(&self.function.body)` で求める
  （`src/call_specialization.rs`、`pub(super)`。`src/llvm.rs` に `mod call_specialization;`）。`borrowed_locals` へ入れる箇所は
  `FunctionEmitter::specialized`（呼び出し特殊化の worker の借用引数と callback）、`src/llvm_control.rs` の `for_each`・
  `match_expression`（照合対象が place のとき）と、同ファイルで `binding.id` を入れる箇所。
- 利用者の式から生じる複製は他に 2 つある。`FunctionEmitter::emit_expression_mode` の `shared_array_deref`（`ref [T]` の参照外しを
  取る）と、place でない値に対する `TypedExprKind::Index` の要素（`clone_value(element, ..)`、要素が drop 不要なら何もしない）。
  `src/llvm_bulk.rs`・`src/llvm_parallel.rs`・`src/llvm_recursive.rs` の `clone_value` は組み込み API の内部の複製で、API の費用に含まれる。
  `src/llvm_frame.rs` の `relocate`・`heap_copy` はスタック上のバッファを脱出時にヒープへ移す移動で、Copy の複製ではない。
- 検査の順序（`src/check.rs` の `check_modules_collect`）: 本体検査と W1001 など（`ModuleOrigin::User` の関数だけ）→ 警告を
  `(span.source, span.start)` で整列 → `CheckedModule` → `ownership::infer_copy_all` → `polymorph::specialize` → `closures::lower` →
  `ownership::check_all(&module)`。最後の段の module は具体化済みで、型変数を含まない。
- `ownership::check_all` は `check_functions(module, false)` で全関数（std・生成関数を含む）を `Checker` で走査する。読み取りの用途は
  `Use::{Consume, Read, Borrow, MutBorrow, Write}` で、`Checker::read_places` が `usage == Use::Consume && !self.is_copy(..)` のとき移動、
  それ以外は複製または借用として扱う。結果は診断だけで、複製の位置は保存しない。
- 警告の設定: `WarningOptions { shadowing }`（`src/warnings.rs`）は `check_modules_indexed_all` で常に `default()`。`shadowing: true` は
  単体テスト `shadowing_is_opt_in_and_lexically_scoped` だけが使い、W1004 に公開の有効化手段はない。
- CLI: 警告は `src/main.rs` が `project.analyze_all()` の後に `module.warnings` を `DiagnosticSet::from_diagnostics` へ渡して表示し、
  `--deny-warnings` なら生成の前に失敗する。`DiagnosticSet::from_diagnostics` は `(source, start, end, severity, code, message)` で
  整列し、同一の診断を一つにする（`src/diagnostic.rs` の `Diagnostics::push`）。`--deny-warnings` と `fmt` の併用は拒否される。
- LSP（`src/lsp.rs`）は `module.warnings` を `textDocument/publishDiagnostics` で送る。`initializationOptions` は読まない。VS Code 拡張は
  設定 `tsuzuri.denyWarnings` を `vsc/src/core.ts` の `commandArguments` で `--deny-warnings` に変えるだけ。
- 明示的な配列・リストの複製 API はない。`ref [i64]` を `*values`、`ref [|i64|]` を `deref values` で取ると複製になる（下の再現）。
- `tests/warnings.rs` は 7 テスト（W1001・W1002・W1004 と CLI）。

### 再現（検証済み）

各ケースを `/tmp/tz-work-A15/<case>/Main.tz` に置き、`target/release/tsuzuri run` で確認した。

| ケース | ソース（改行は `\n`） | 結果 | 暗黙の複製 |
| --- | --- | --- | --- |
| `reused` | `let a = [1, 2, 3]\nlet b = a\nArray.length (ref a) + Array.length (ref b)` | `6` | `let b = a` の `a`（後で再使用） |
| `moved` | `let a = [1, 2, 3]\nlet b = a\nArray.length (ref b)` | `3` | なし（`a` は単一使用で移動） |
| `field` | `record Bag { items: [i64] }\nlet bag = Bag { items: [1, 2] }\nlet xs = bag.items\nlet ys = bag.items\nArray.length (ref xs) + Array.length (ref ys)` | `4` | `bag.items` の 2 箇所 |
| `scalar` | `let n = 5\nlet m = n\nn + m` | `10` | なし（drop 不要） |
| `deref-array` | `def copy_array :: ref [i64] -> [i64]\nfn copy_array values = *values\n...` | `6` | `*values` |
| `deref-list` | `def copy_list :: ref [\|i64\|] -> [\|i64\|]\nfn copy_list values = deref values\n...` | `6` | `deref values` |

`target/release/tsuzuri check /tmp/tz-work-A15/reused --warn implicit-copy` は現在 `error[E2000]: unknown option '--warn'; use --help`
で終了コード 2。IR 全体の `call ptr @tz.alloc` の数は `reused` と `moved` で同じ（3）で、確保回数の静的な数では複製を区別できない。
テストは一覧と警告で判定する。

## 仕様

### 前提とする他チケットのインターフェース

- PM07（並行して詳細化中。依存欄は `PX01, (A15)`）: A15 が提供する `copies::sites` を入力として使う。A15 が約束するのは次の 2 点だけ。
  - `tsuzuri::copies::sites(&CheckedModule) -> Vec<CopySite>` は、具体化・クロージャ降格後の module で、利用者の式から生じる暗黙の複製
    （`FunctionEmitter::clone_value` が呼ばれる 3 経路。D4）を一件ずつ返す。std・生成関数も含む。
  - debug build では「生成した暗黙の複製はすべて一覧にある」ことを検査する（D5）。PM07 が複製の省略を広げる場合は、同じ変更で D4 の
    除外規則も広げ、`tests/copy_cost.rs` の期待値のうち「PM07 で変わる」と注記したものを更新する。
- G12: LSP 表示（inlay hint・hover）は G12 が実装する。A15 は `copies::sites` を公開するだけで、`src/lsp.rs` を変えない（D7）。
- C10: 共有バッファによる O(1) 化は C10 の範囲。A15 はそれを前提にしない。

### 構文

言語の構文は変えない。CLI とライブラリだけを足す。

```text
tsuzuri (check | build | run | test) <input> [--warn implicit-copy] [--deny-warnings] [...既存のオプション]
warn-option = "--warn" warning-name
warning-name = "implicit-copy"
```

新 API（実装後に有効。未検証）。`std/Array.tz` と `std/List.tz` に足す宣言と本体。

```tsuzuri
def copy :: Copy<'a> => ref ['a] -> ['a]
fn copy values = *values
```

```tsuzuri
def copy :: Copy<'a> => ref [|'a|] -> [|'a|]
fn copy values = *values
```

本体は参照外しの取得で、既存の複製経路（`shared_array_deref`・`read_place` の `clone_value`）をそのまま使う。利用者の関数で同じ形
（`*values`・`deref values`）が動くことは「再現」で確認済み。

### 型規則

- 型規則は変えない。`Array.copy`・`List.copy` は `Copy<'a>` を要求する（要素が Copy でない配列は、配列自体も Copy でなく複製できない）。
- 複製の位置（copy site）: 所有権検査が値として読む（`Use::Consume`）place の式 `e` で、`is_copy(e.ty) && needs_drop(e.ty)` のもの。
  ただし `e` が単一使用のローカル全体（`single_use_locals` にあり、借用として束縛されていない）なら移動なので除く。place でない値の
  添字 `v[i]` で、要素型が `is_copy && needs_drop` のものも含む（D4）。
- 費用の分類（`CopyCost`）: 型が配列かリストを（レコード・タプル・union の field／payload を通して）含めば `Length`、含まなければ
  `Environment`（関数値の環境の複製だけ）。
- `W1006` は `cost == Length` で、関数が利用者のソース（`origin.module == ModuleOrigin::User` かつ `origin.provenance == Provenance::User`）
  のものだけを報告する（D2）。`string`・`Vec`・`Task` は Copy でないので対象にならない。

### 評価順序・所有権・借用

変えない。記録は観測だけで、所有権検査の結果（エラー・Copy 推論）と生成 IR を変えない。`--warn implicit-copy` は警告の表示だけに
影響し、生成へは渡らない。`Array.copy (ref a)` は `a` を共有借用し、新しい所有値を返す。

### 数値・トラップ・native と WASM の差

なし。`Array.copy`・`List.copy` の確保失敗は既存の暗黙の複製と同じ経路で扱う。native と WASM で同じ結果になる。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `W1006` | `--warn implicit-copy` のとき、配列型の copy site | `implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit` | 取得する式（`let b = a` の `a`、`bag.items` 全体） |
| `W1006` | 同上、リスト型の copy site | `implicit copy of a list allocates a new node for every element; borrow it with 'ref', or call 'List.copy' to make the copy explicit` | 同上 |
| `W1006` | 同上、配列・リストを含むレコード・タプル・union の copy site | `implicit copy of a value that contains arrays or lists copies all of them; borrow it with 'ref', or use the value only once so that it moves` | 同上 |
| `E2000` | `--warn` の後に値がない | `--warn requires a warning name; supported: implicit-copy` | `<command line>` |
| `E2000` | 未知の警告名 | `unknown warning '<name>' for --warn; supported: implicit-copy` | `<command line>` |
| `E2000` | `--warn implicit-copy` の重複 | `warn implicit-copy specified more than once` | `<command line>` |
| `E2000` | `fmt` との併用 | `fmt` が `--deny-warnings` などを拒否する既存のメッセージ（`src/main.rs` の `Action::Fmt && (...)` の条件に足す） | `<command line>` |

- 重大度は警告。`--deny-warnings` と併用すると、他の警告と同様に生成の前に失敗する（D8）。
- 同じソース位置は、具体化が複数あっても 1 件（D11）。表示件数の上限は既存の `DiagnosticSet` の上限に従う。

### 資源上限

新しい上限はない。一覧はソースの位置と具体化の数（上限 1,024）に比例する。`copies::sites` は所有権検査をもう一回走らせる費用で、
`--warn implicit-copy` のときと debug build の照合（D5）のときだけ実行する。

### 例

検証済みの現行ソース（「再現」の各ケース）に対する、実装後の `check --warn implicit-copy` の期待。

| ケース | `W1006` の件数 | 位置（ソース上の文字列） | 理由 |
| --- | --- | --- | --- |
| `reused` | 1 | `let b = a` の `a` | `a` は後で借用されるので単一使用でない |
| `moved` | 0 | なし | 単一使用のローカル全体は移動 |
| `field` | 2 | 2 つの `bag.items` | field からの取得は常に複製（PM07 で変わる） |
| `scalar` | 0 | なし | `i64` は drop 不要 |
| `deref-array` | 1 | `*values` | 参照外しの取得は複製 |
| `deref-list` | 1 | `deref values` | 同上（リストのメッセージ） |

新 API（実装後に有効。未検証）。明示の複製は警告しない（期待: 警告 0 件、結果 `6`）。

```tsuzuri
let a = [1, 2, 3]
let b = Array.copy (ref a)
Array.length (ref a) + Array.length (ref b)
```

### Phase 2（設計方針）

- LSP の inlay hint／hover に複製の種類を表示する（G12 が `copies::sites` を使う）。
- 共有の不変バッファ（C10 の `Rc` 相当）で複製を O(1) にする選択肢は C10 で扱う。

## 設計

### データ構造

```rust
// src/copies.rs（新規）。lib.rs に `pub mod copies;`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopySite {
    pub function: usize, // CheckedModule::functions の添字
    pub span: Span,      // 取得する式の span
    pub ty: Type,        // 具体型（型変数を含まない）
    pub kind: CopyKind,
    pub cost: CopyCost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyKind { Local, Field, Element, Tail, Payload, Dereference, Temporary }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyCost { Length, Environment }

/// 所有権検査が記録する候補。単一使用の除外は copies::sites が行う。
pub(crate) struct CopyRead { pub span: Span, pub ty: Type, pub kind: CopyKind, pub local: Option<usize> }

pub fn sites(module: &CheckedModule) -> Vec<CopySite>;
pub fn warnings(module: &CheckedModule) -> Vec<Diagnostic>;
```

- `CopyKind` は取得する式の種類: `E::Local` → `Local`、`E::Field` → `Field`、place の `E::Index` → `Element`、`E::ListTail` → `Tail`、
  `E::UnionPayload` → `Payload`、`E::Dereference` → `Dereference`、place でない値の `E::Index` → `Temporary`。
- `CopyRead::local` は、式が `E::Local(id)` で、`id` が `Checker::state.aliases` にない（照合対象の place の別名として束縛されていない）
  ときだけ `Some(id)`。別名は生成で `borrowed_locals` に入り、単一使用でも複製されるため `None` にする（D4）。
- `src/ownership.rs`: `Checker` に `copies: Option<&'a mut Vec<CopyRead>>`（新規）を足す。`check_body` に同じ型の最後の引数を足し、
  既存の呼び出し元（`check_functions`）は `None` を渡す。新しい入口 `pub(crate) fn copy_reads(module: &CheckedModule) -> Vec<Vec<CopyRead>>`
  （新規）は `closed_returns(module)` を求め、関数ごとに `check_body(.., infer = false, .., Some(&mut reads))` を呼び、結果の `Err` は捨てる
  （module は `check_all` を通過済み）。
- `src/main.rs`: `Arguments` に `warn_implicit_copy: bool`（新規）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 公開 | `src/lib.rs` | module 宣言 | `pub mod copies;`（`pub mod ownership;` の隣、名前順） |
| 一覧 | `src/copies.rs`（新規） | `CopySite`, `CopyKind`, `CopyCost`, `CopyRead`, `sites`, `warnings`, `cost`（新規）, `message`（新規） | アルゴリズムの節のとおり |
| 所有権 | `src/ownership.rs` | `Checker`, `check_body`, `check_functions` | field と引数の追加。`check_functions` は `None` |
| 所有権 | `src/ownership.rs` | `Checker::read_places` | 先頭で、`usage == Use::Consume` かつ `self.copies.is_some()` かつ `self.is_copy(&expression.ty)` かつ `expression.ty.needs_drop(&self.module.types())` なら `CopyRead` を push。判定・エラーは変えない |
| 所有権 | `src/ownership.rs` | `Checker::eval` の `E::Index` で base が `Checker::is_place` でない経路 | 要素型が `is_copy && needs_drop` なら `CopyKind::Temporary`・`local: None` で push |
| 所有権 | `src/ownership.rs` | `copy_reads`（新規） | 上記 |
| 可視性 | `src/llvm.rs` | `mod call_specialization;` | `pub(crate) mod call_specialization;` |
| 可視性 | `src/call_specialization.rs` | `single_use_locals` | `pub(super)` → `pub(crate)`。本体は変えない |
| 生成（debug のみ） | `src/llvm.rs` | `Globals` | `#[cfg(debug_assertions)] emitted_copies: BTreeSet<(Option<usize>, usize, usize)>`（新規） |
| 生成（debug のみ） | `src/llvm.rs` | `FunctionEmitter::read_place`、`emit_expression_mode` の `shared_array_deref` の `take` 分岐、`TypedExprKind::Index`（place でない値）の arm | `clone_value` を呼ぶ直前に `self.note_copy(expression)`（新規、`#[cfg(debug_assertions)]`）。Index の arm は要素型が `is_copy && needs_drop` のときだけ。`self.symbol` が `@tz.specialized.` で始まる worker では記録しない |
| 生成（debug のみ） | `src/llvm.rs` | 公開 `emit_*` が共通に通るモジュール生成の最後 | `check_copy_inventory`（新規、`#[cfg(debug_assertions)]`）: `emitted_copies ⊆ {copies::sites の span}` を `assert!`。失敗時は一覧にない span を列挙する |
| CLI | `src/main.rs` | 使い方の文字列、`Arguments`、引数の解析、`fmt` の拒否条件、警告の表示 | D3。表示は `module.warnings` に `tsuzuri::copies::warnings(&module)` を連結して `DiagnosticSet::from_diagnostics` へ渡す |
| std | `std/Array.tz`, `std/List.tz` | `copy`（新規） | 「構文」の宣言と本体。既存の `reverse`・`fold` の近くに置き、`///` の説明を 1 行付ける |
| 変更なし | `src/warnings.rs`, `src/check.rs`, `src/lsp.rs`, `vsc/` | `WarningOptions` ほか | W1006 は検査の後段で作るので `WarningOptions` を経由しない（D1）。LSP は D7 |

### 生成 IR とランタイム

- release build の IR は変わらない。`--warn implicit-copy` の有無でも変わらない（手順 7 で byte 比較）。debug 用の記録は IR の文字列を出さない。
- `Array.copy`・`List.copy` は利用者の `*values` と同じ `clone_value` の経路を通る。新しいランタイム記号・WASM import はない。

### アルゴリズム

```text
sites(module):
  reads = ownership::copy_reads(module)              // 関数ごとの CopyRead
  result = []
  seen = BTreeSet<(usize, Option<usize>, usize, usize)>   // ループの再評価で同じ式が 2 回読まれる
  for (index, function) in module.functions:
    single = call_specialization::single_use_locals(&function.body)
    for read in reads[index]:
      if read.local == Some(id) and id in single: continue   // clones_on_take と同じ除外
      if not seen.insert((index, read.span.source, read.span.start, read.span.end)): continue
      result.push(CopySite { function: index, span: read.span, ty: read.ty,
                             kind: read.kind, cost: cost(module, &read.ty) })
  return result

cost(module, ty):
  contains(ty) = match ty
    Array(_) | List(_)         -> true
    Tuple(items)               -> any(contains)
    Record(id, args)           -> any(contains, types.record_fields(id, args))
    Union(id, args)            -> any(contains, flatten(types.union_payloads(id, args)))
    _                          -> false          // Copy の型は再帰しない（再帰型は Copy でない）
  return Length if contains(ty) else Environment

warnings(module):
  seen = BTreeSet<(Option<usize>, usize, usize)>
  out = []
  for site in sites(module):
    function = module.functions[site.function]
    if site.cost != Length or function.origin.module != User or function.origin.provenance != Provenance::User: continue
    if seen.insert((site.span.source, site.span.start, site.span.end)):
      out.push(Diagnostic::warning("W1006", message(&site.ty), site.span))
  sort out by (span.source, span.start)
  return out

message(ty): Array(_) -> 配列の文, List(_) -> リストの文, _ -> 集成型の文（診断の表）
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。テストの Tsuzuri ソースは、書く前に `/tmp/tz-a15/<case>/Main.tz` に置いて
`target/release/tsuzuri run` で動くことを確かめる（期待値は D4 の規則から手で数え、実装の出力から写さない）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の 6 ケースを `/tmp/tz-a15/<case>/Main.tz` に置き、IR を保存する。
- 確認: 次がすべて成功し、4 つのテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
for c in reused moved field scalar deref-array deref-list; do
  target/release/tsuzuri build /tmp/tz-a15/$c --emit llvm -o /tmp/tz-a15/$c.before.ll
  target/release/tsuzuri build /tmp/tz-a15/$c --emit llvm -O3 -o /tmp/tz-a15/$c.before-O3.ll
done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: module の骨格と可視性

- 変更: `src/lib.rs`、`src/copies.rs`（新規）、`src/llvm.rs` の `mod call_specialization;`、`src/call_specialization.rs` の `single_use_locals`、
  `tests/copy_cost.rs`（新規）。
- 内容: 「データ構造」の型を置き、`sites` と `warnings` は空の `Vec` を返す。可視性を `pub(crate)` にする。テストファイルに
  helper `module(source) -> CheckedModule`（`tsuzuri::analyze(source).unwrap()`）、`user_sites(source) -> Vec<CopySite>`（利用者の関数の
  site だけ）、`copy_warnings(source) -> Vec<Diagnostic>`、`text(source, &Diagnostic) -> &str` と、テスト 3（スカラー）を書く。
- 確認: `cargo test --locked --test copy_cost` が `1 passed`。`cargo test --locked` が成功する。

### 手順 3: 所有権検査の記録と一覧

- 変更: `src/ownership.rs` の `Checker`・`check_body`・`check_functions`・`Checker::read_places`・`Checker::eval`（place でない `E::Index`）・
  `copy_reads`（新規）、`src/copies.rs` の `sites`・`cost`。
- 内容: 「段ごとの変更」とアルゴリズムのとおり。記録は `self.copies` が `Some` のときだけ行い、既存の分岐・エラーの前後関係を変えない。
- 確認: テスト 1〜10 を `user_sites` で書き（件数・`kind`・`cost`・span の文字列）、`cargo test --locked --test copy_cost` が `10 passed`。
  `cargo test --locked` が成功する（所有権のエラーが一件も変わらないこと）。

### 手順 4: 警告の生成

- 変更: `src/copies.rs` の `warnings`・`message`。
- 内容: アルゴリズムのとおり。診断の表の 3 文をそのまま使う。テスト 1〜10 に `copy_warnings` の件数・コード・メッセージ・span の検査を足す。
- 確認: `cargo test --locked --test copy_cost` が `10 passed`。`cargo test --locked --test warnings` が `7 passed`（W1006 は既定の警告に出ない）。

### 手順 5: debug build の照合（D5）

- 変更: `src/llvm.rs` の `Globals`・`FunctionEmitter::read_place`・`emit_expression_mode`・place でない `TypedExprKind::Index` の arm・
  `note_copy`（新規）・`check_copy_inventory`（新規）、`tests/copy_cost.rs` のテスト 12。
- 内容: テスト 12 はこの時点ではテスト 1〜10 のソースを使い、手順 6 でテスト 11 のソースを足す。
  記録と照合はすべて `#[cfg(debug_assertions)]` の中に置き、release のコードと IR の文字列を変えない。照合の失敗メッセージは
  `implicit clone at <source>:<start>..<end> is missing from copies::sites` とし、足りない span をすべて並べる。
- 確認: `cargo test --locked` が成功する（debug build の全テストが照合を通る）。失敗したら落とし穴の「照合の不一致」に従う。
  `cargo test --locked --test copy_cost` が `11 passed`。

### 手順 6: `Array.copy` と `List.copy`

- 変更: `std/Array.tz`、`std/List.tz`、`tests/copy_cost.rs` のテスト 11、`tests/fixtures/arrays/ExplicitCopy.tz`（新規）、
  `tests/features.mjs` の suite `explicit_copy`（新規、GUIDE §7.4）。
- 内容: 「構文」の宣言と本体。fixture は E2E の節のソース。
- 確認: `cargo test --locked --test copy_cost` が `12 passed`。`cargo test --locked honors_the_exact_specialization_limit` と手順 1 の
  stack-depth 3 テストが成功する。`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri explicit_copy` が成功する。

### 手順 7: CLI `--warn implicit-copy`（D3 の承認後）

- 変更: `src/main.rs` の使い方の文字列（`--deny-warnings` の次の行に
  `--warn implicit-copy   Report W1006 at implicit copies of arrays and lists`）、`Arguments::warn_implicit_copy`、引数の解析
  （`--deny-warnings` の分岐の隣）、`fmt` の拒否条件（`Action::Fmt && (optimization.is_some() || cpu.is_some() || deny_warnings)` に追加）、
  警告の表示（`module.warnings` と `tsuzuri::copies::warnings(&module)` の連結）、`src/main.rs` の単体テスト、`tests/copy_cost.rs` のテスト 13。
- 内容: 値の読み方は既存の値付きオプション（`--cpu`）の分岐を写す。E2000 の 3 文は診断の表のとおり。
- 確認: `cargo test --locked --bin tsuzuri` が成功し、追加した解析テストが実行される（N が増える）。`cargo test --locked --test copy_cost` が
  `13 passed`。

### 手順 8: IR が変わらないことの確認

- 変更: なし。
- 内容: release build で手順 1 の 6 ケースの IR を `-O0`／`-O3` で出し直し、`--warn implicit-copy` 付きの `build` とも比べる。
- 確認: 次の `cmp` がすべて無出力で成功する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
for c in reused moved field scalar deref-array deref-list; do
  target/release/tsuzuri build /tmp/tz-a15/$c --emit llvm -o /tmp/tz-a15/$c.after.ll
  target/release/tsuzuri build /tmp/tz-a15/$c --emit llvm -O3 --warn implicit-copy -o /tmp/tz-a15/$c.after-O3.ll
  cmp /tmp/tz-a15/$c.before.ll /tmp/tz-a15/$c.after.ll
  cmp /tmp/tz-a15/$c.before-O3.ll /tmp/tz-a15/$c.after-O3.ll
done
target/release/tsuzuri check /tmp/tz-a15/reused --warn implicit-copy
```

最後のコマンドは `W1006` を 1 件表示して終了コード 0。

### 手順 9: ドキュメント

- 変更: 「ドキュメント」の節のファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/language-reference/ownership.md _docs/library-reference/arrays-and-lists.md _docs/tools/diagnostics.md _docs/tools/command-line.md _docs/guides/performance.md _docs/feature-status.md`
  が成功する。

### 手順 10: 最終確認

- 変更: `_features/README.md` と `_docs/feature-status.md` の状態を done にする。
- 確認: GUIDE §2.3 の基準コマンド、`cargo test --locked`、手順 1 の 4 テスト、`node tests/features.mjs target/release/tsuzuri explicit_copy`
  がすべて成功する。GUIDE §10 の完了の定義を満たす。

## テスト計画

### Rust テスト

`tests/copy_cost.rs`（新規）。ソースは改行を `\n` で書く。件数は利用者の関数の site（`user_sites`）と `copy_warnings` の両方で見る。

| # | テスト関数 | ソース（要点） | 期待 |
| --- | --- | --- | --- |
| 1 | `reports_array_copy_when_the_source_is_used_again` | 「再現」の `reused` | site 1（`Local`, `Length`）、W1006 1 件。span の文字列は `a` で、位置は `source.find("= a").unwrap() + 2`。配列の文 |
| 2 | `does_not_report_after_a_move` | `moved` と `let a = [1, 2, 3]\nlet b = a\nlet c = b\nArray.length (ref c)` | どちらも site 0、警告 0 |
| 3 | `never_reports_copy_scalars_or_scalar_records` | `scalar` と `record Point { value: i64 }\nlet p = Point { value: 1 }\nlet q = p\np.value + q.value` | site 0、警告 0（drop 不要） |
| 4 | `reports_each_take_of_an_array_field` | `field` | site 2（`Field`）、警告 2、どちらも `bag.items`。PM07 で変わる（2 回目は最後の使用） |
| 5 | `reports_list_copy_with_the_list_message` | `let a = [\|1, 2, 3\|]\nlet b = a\nList.length (ref a) + List.length (ref b)` | 警告 1、メッセージに `'List.copy'` |
| 6 | `reports_argument_copy_only_when_the_caller_keeps_the_value` | `def total :: [i64] -> i64\nfn total values = Array.length (ref values)\nlet a = [1, 2]\ntotal a + Array.length (ref a)` と、最後の行を `total a` にしたもの | 前者は警告 1（`total a` の `a`）、後者は 0 |
| 7 | `reports_aggregate_copy_with_the_aggregate_message` | `record Bag { items: [i64] }\nlet bag = Bag { items: [1] }\nlet other = bag\nArray.length (ref bag.items) + Array.length (ref other.items)` | 警告 1（`let other = bag` の `bag`）、集成型の文 |
| 8 | `reports_dereference_copy` | 「再現」の `deref-array` | site 1（`Dereference`）、span `*values` |
| 9 | `closure_copies_are_listed_but_not_reported` | `let offset = 1\nlet add = x -> x + offset\nlet f = add\nf 1 + add 2` | `let f = add` の `add` を含む `Environment` の site が 1 件以上、`Length` の site 0、警告 0 |
| 10 | `specializations_report_one_warning_per_source_position` | `def dup :: Copy<'a> => 'a -> ('a * 'a)\nfn dup value = (value, value)\nlet a = dup [1, 2]\nlet b = dup [1.5]\n0` | site 4（2 つの具体化 × 2 つの `value`）、警告 2。PM07 で変わる（2 つ目の `value` は最後の使用） |
| 11 | `explicit_copy_api_is_listed_in_std_but_not_reported` | 「例」の `Array.copy` と、同じ形の `List.copy` | 警告 0、`user_sites` 0。`sites` 全体には `origin.module == ModuleOrigin::Std` の site が 1 件以上ある |
| 12 | `inventory_covers_every_emitted_clone` | テスト 1〜11 のソース | `tests/polymorphism.rs` の `accepts` を写した helper で IR を出し、成功する（debug の照合が panic しない） |
| 13 | `cli_warn_implicit_copy_is_opt_in_deniable_and_ir_neutral` | `reused` を一時ディレクトリへ（`tests/warnings.rs` の CLI テストの一時 root の作り方を写す） | `check --json` の stderr に `W1006` がない。`check --json --warn implicit-copy` は成功し stderr に `"code":"W1006"`。`--deny-warnings` を足すと失敗。`--warn` 単独・`--warn shadowing`・重複・`fmt` との併用は終了コード 2 で `E2000`。`build --emit llvm` の出力はフラグの有無で byte 一致 |

`src/main.rs` の単体テスト: `parses_warn_implicit_copy`（新規。`check`・`build`・`run`・`test` で `warn_implicit_copy` が true）。
既存の拒否の表（`vec!["fmt", "Main.tz", "--deny-warnings"]` がある表）に `["check", "Main.tz", "--warn"]`、
`["check", "Main.tz", "--warn", "shadowing"]`、`["check", "Main.tz", "--warn", "implicit-copy", "--warn", "implicit-copy"]`、
`["fmt", "Main.tz", "--warn", "implicit-copy"]` を足す。

### E2E

- fixture `tests/fixtures/arrays/ExplicitCopy.tz`（新規）、suite `explicit_copy`（`tests/features.mjs`、GUIDE §7.4）。
- 新 API（実装後に有効。未検証）。期待値は手計算: 長さ 3 + 3 + 2 + 2、要素 `b[0] + b[2]` = 1 + 3、合計 `14`。

```tsuzuri
let a = [1, 2, 3]
let b = Array.copy (ref a)
let xs = [|4, 5|]
let ys = List.copy (ref xs)
Array.length (ref a) + Array.length (ref b) + List.length (ref xs) + List.length (ref ys) + b[0] + b[2]
```

- native と WASM × `-O0`／`-O3` の 4 通りで `14`、`live == 0`、WASM の import は空。

### 既存テストへの影響

なし。既存の警告・所有権・IR の期待値は変わらない。変わる必要が出たら停止条件。PM07 は本チケットのテスト 4・10 の期待を更新する予定。

### 性能

W1006 は既定無効で、既定の `check`／`build` の処理は増えない。`--warn implicit-copy` は所有権検査を一回追加する。閾値は設けない。

## ドキュメント

- `docs/language.md` の `### Ownership / Borrowing`: 配列・リストの Copy の費用の段落に、`--warn implicit-copy`（`W1006`）と
  `Array.copy`・`List.copy` を 2〜3 文で足す。同ファイルの診断コードの一覧で `W1004` の次に `W1006`（既定無効、`--warn implicit-copy`）を足す。
- `docs/architecture.md` の `## 性能設計の原則` の末尾に 1 段落: `copies::sites` は所有権検査の記録から暗黙の複製の一覧を作り、W1006 と
  PM07 が使う。debug build では生成した複製がすべて一覧にあることを検査する。
- `_docs/language-reference/ownership.md`: Copy の説明に費用・警告の有効化・明示 API の例（`Array.copy (ref a)`）。
- `_docs/library-reference/arrays-and-lists.md`: `Array.copy`・`List.copy` の署名と 1 行の説明。
- `_docs/tools/diagnostics.md`: `W1006` の行（条件、既定無効、有効化、`--deny-warnings` との併用で明示の複製を強制できること）。
- `_docs/tools/command-line.md`: `--warn implicit-copy` の説明と、`fmt` では使えないこと。
- `_docs/guides/performance.md`: 暗黙の複製を探す手順（`tsuzuri check <dir> --warn implicit-copy`）と、`ref` での借用・`Array.copy` の使い分け。
- `_docs/feature-status.md` と `_features/README.md` の A15 の状態。

## 受け入れ条件

- [ ] `--warn implicit-copy` を付けたときだけ `W1006` が報告され、既定の出力は変わらない。
- [ ] release build の IR が、手順 1 と比べて、またフラグの有無で byte 一致する（手順 8）。
- [ ] `copies::sites` が公開され、debug build の全テストで「生成した暗黙の複製はすべて一覧にある」照合が通る。
- [ ] Copy のスカラー・スカラーだけのレコード、単一使用の移動、明示 API、std・生成コードでは報告しない（テスト 2・3・9・11）。
- [ ] 配列・リスト・それらを含む集成型の暗黙の複製を、正しい位置とメッセージで報告する（テスト 1・4〜8・10）。
- [ ] `--warn implicit-copy --deny-warnings` で暗黙の複製があると生成前に失敗する。
- [ ] `Array.copy`・`List.copy` が native と WASM × `-O0`／`-O3` で動き、`live == 0`。
- [ ] 文書を更新し、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 照合の不一致（手順 5）: 一覧にない span が出たら、その式を所有権検査がどの `Use` で読んでいるかを調べる。`for` のループ変数や
  パターン束縛の local が単一使用として除外されている場合は、`src/llvm_control.rs` が `borrowed_locals` に入れる条件（`for_each`、
  `binding.id` を入れる箇所）と同じ条件で local を集める `borrowed_bindings(body)`（新規、`src/copies.rs`）を作り、`sites` の除外から外す。
  生成側（`clones_on_take` など）を変えて合わせてはいけない（停止条件）。
- 呼び出し特殊化の worker（`FunctionEmitter::specialized`、記号 `@tz.specialized.N`）は借用引数を必ず複製するので、通常の本体と複製の
  集合が違う。worker では記録しない。
- place でない値の `TypedExprKind::Index` は `clone_value` を無条件に呼ぶ（drop 不要なら何もしない）。記録は要素型が
  `is_copy && needs_drop` のときだけにする。そうしないとスカラー配列の添字がすべて一覧に入る。
- 所有権検査はループ本体などを 2 回評価することがあり、同じ式の `CopyRead` が重複する。`sites` の `(function, span)` の重複除去を省かない。
- `WarningOptions` に `implicit_copy` を足して `check_modules_collect` の中で作ると、具体化前の本体（型変数を含む）を見てしまい、
  Copy の判定ができない。W1006 は具体化後の module から作る（D1）。
- メッセージに型名を入れない。具体化ごとにメッセージが変わると `Diagnostics::push` の同一判定で一つにならない（D11）。
- ラムダ本体は `closures::lower` で別の関数になる。ラムダ内の暗黙の複製が報告されない場合は、降格した関数の `origin.provenance` を
  確認し、`provenance` を変えずに作業を止めて報告する（W1002 など他の警告に影響するため）。
- 生成された `deriving` のインスタンスなど、利用者のモジュールにある生成関数を報告しない。`origin.provenance == Provenance::User` を必ず見る。
- テストのソースは release コンパイラで動作を確認してから書く。期待値は D4 の規則から数え、実装の出力から写さない。
- `cargo test --locked copy_cost` のようなフィルターは 0 件でも成功する。`--test copy_cost` を使い、`running N tests` を見る。

## 対象外

- Copy の意味の変更、宣言による Copy の opt-in、共有バッファによる O(1) の複製（C10）。
- 複製の除去・移動への置き換え（PM07）。
- LSP・VS Code の表示、inlay hint・hover・code action、拡張の設定の追加（G12。D7）。
- 関数値の環境の複製の警告（一覧には `Environment` として入る。D2）。
- 組み込み API 内部の複製（`src/llvm_bulk.rs` など）と、スタックのバッファをヒープへ移す `relocate`・`heap_copy`（一覧に入れない）。
- W1004（shadowing）の公開。`--warn` が受ける名前は `implicit-copy` だけ。
- リテラルの長さなどによる報告の閾値（D10）。

## 決定事項

### D1: 一覧の作り方と実行位置

- 決定: 一覧は `src/copies.rs`（新規）の `sites` が作る。所有権検査（`src/ownership.rs` の `Checker`）に観測用の記録を足し、
  具体化・クロージャ降格後の module（`check_modules_collect` が `ownership::check_all` に渡すもの）に対して `copy_reads` でもう一回走らせる。
  単一使用の除外は生成と同じ `call_specialization::single_use_locals` を使う。`sites` は必要なとき（W1006 の有効時、debug の照合、PM07）
  だけ呼ぶ。`CheckedModule` に field は足さない。
- 理由: 所有権検査は値として読む位置（`Use::Consume`）を既に判定しており、生成の `take` と同じ判断を二重に書かずに済む。具体化後なので
  Copy が確定している。既定の検査・生成の費用が増えない。
- 状態: 既定案（実装者はこの案に従う）

### D2: 報告の対象

- 決定: `W1006` は `CopyCost::Length`（配列・リストを含む型）の site だけを、利用者のソースの関数（`ModuleOrigin::User` かつ
  `Provenance::User`）で報告する。関数値の環境の複製は一覧に `Environment` として入れるが報告しない。`string`・`Vec` は Copy でないので対象外。
- 理由: 長さ比例の費用が本チケットの対象。関数値は型から環境の有無が分からず、高階関数への受け渡しごとに警告が出て実用にならない。
  旧版の本文は「環境を持つ関数値」も対象にしていたが、調整者の横断決定（長さ比例の複製に限る）に合わせて外した。
- 状態: 既定案（実装者はこの案に従う）

### D3: 有効化の手段とコード

- 決定: 公開 CLI オプション `--warn implicit-copy`（`check`・`build`・`run`・`test` で有効、`fmt` では E2000）。コードは `W1006`
  （GUIDE D-30 の仮割り当て）。W1006 は `src/main.rs` で `copies::warnings` の結果を `module.warnings` に連結して表示する。
- 理由: 既存の既定無効の仕組み（`WarningOptions { shadowing }`）は内部用で公開の手段がなく、しかも具体化前に動く（D1）。
  `--warn <name>` は将来ほかの既定無効の警告を足せる形で、名前は一つに限る。
- 状態: 承認済み（2026-10-03。実装と検証を参照）

### D4: 記録の規則

- 決定: `Checker::read_places` で `usage == Use::Consume && is_copy && needs_drop` の式を記録する。`E::Local(id)` で `id` が
  `state.aliases` にないときだけ `local: Some(id)` とし、`sites` は `single_use_locals` にある `local` を除く。place でない値の `E::Index` は
  要素型で判定し `Temporary` として記録する。`CopyKind` は「データ構造」の対応表のとおり。
- 理由: 生成の `clones_on_take` と `emit_expression_mode` の 3 経路に一対一で対応する。別名の local は生成で `borrowed_locals` に入り常に複製される。
- 状態: 既定案（実装者はこの案に従う）

### D5: 生成との照合

- 決定: debug build だけで、`FunctionEmitter` が暗黙の複製を生成した span を `Globals` に集め、モジュール生成の最後に
  「すべて `copies::sites` にある」ことを `assert!` する。逆向き（一覧の site がすべて生成される）は検査しない（定数の `match` の表引きなど、
  生成が式を評価しない経路があるため）。逆向きの誤報はテスト 1〜11 の個別の期待で守る。worker は記録しない。
- 理由: 全テストが生成を通るので、一覧の漏れ（W1006 の見逃し、PM07 の数え落とし）を常に検出できる。release の IR と速度は変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 明示的な複製 API

- 決定: `Array.copy :: Copy<'a> => ref ['a] -> ['a]` と `List.copy :: Copy<'a> => ref [|'a|] -> [|'a|]` を std に足す。本体は `*values`。
- 理由: 旧版の決定を保つ。本体を参照外しにすると既存の複製経路をそのまま使え、新しい生成コードが要らない。
- 見直し提案: 既存の `Vec.clone` と名前をそろえて `Array.clone`・`List.clone` にする案がある。変える場合は承認時に D3 と合わせて決め、
  診断の表の文も直す。
- 状態: 既定案（実装者はこの案に従う）

### D7: LSP とエディターでの表示

- 決定: Phase 1 では LSP（`src/lsp.rs`）に W1006 を出さず、VS Code 拡張の設定も足さない。`--json` の出力には他の警告と同じ形で入る。
- 理由: LSP には設定を受ける仕組み（`initializationOptions`）がなく、既定無効の警告を出し分けられない。表示方法（inlay hint・hover）は G12 の範囲。
- 状態: 既定案（Phase 1 はこの案のとおり。Phase 2 で inlay hint を足した。実装と検証の追記 2）

### D8: 明示の複製を必須にするモード

- 決定: 専用のモードは作らない。`--warn implicit-copy --deny-warnings` で、暗黙の長さ比例の複製があるとき生成前に失敗する。
- 理由: 既存の `--deny-warnings` の経路だけで実現でき、追加の実装が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D9: 既定の有効化

- 決定: 既定は無効。
- 理由: 既存のコードに警告を増やさない（旧版の既定案）。
- 状態: 既定案（実装者はこの案に従う）

### D10: 報告の閾値

- 決定: 型だけで判定し、リテラルや実行時の長さは見ない。
- 理由: 長さは一般に実行時まで分からない。旧版の既定案を保つ。
- 状態: 既定案（実装者はこの案に従う）

### D11: 具体化と重複

- 決定: 具体化前の generic 本体では報告しない（`sites` は具体化後の module だけを見る）。同じソース位置は具体化がいくつあっても W1006 を
  1 件にする。メッセージに型名を入れない。一覧（`sites`）は具体化ごとに別の site として残す。
- 理由: Copy かどうかは具体化まで決まらない。利用者には位置が一つ分かれば足り、PM07 には具体化ごとの件数が要る。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-03）

「A15 / A12 の実装を完遂して。複数フェーズある場合には、すべてのフェーズを完了させること」という依頼を D3 の承認として扱い、Phase 1 と Phase 2 を実装した。
着手時の HEAD は `c995217`（ブランチ `Phase7-1`）で、A12 と同じ変更に含めた。Phase 2 の二つ目（共有の不変バッファによる O(1) の複製）は C10 の範囲のまま。
性能は主張しない（既定の検査と生成の経路は変えず、`--warn implicit-copy` の有無で IR は同一）。

### 実装

- Phase 1: `src/copies.rs`（新規。`CopySite`・`CopyKind`・`CopyCost`・`sites`・`costly_sites`・`warnings`）。`src/ownership.rs` の `Checker` に収集用の `copies` を足し、
  `read_places`（`Use::Consume` で Copy かつ drop の要る値）、place でない値の `Index` と一時値の field（`Temporary`）、`src/ownership_control.rs` の
  `match` の束縛（`Payload`・`Tail`・`Element`）で記録する。`ownership::copy_reads` は具体化・クロージャ降格後の module の全関数を収集モードで再検査する。
  単一使用の除外は `call_specialization::single_use_locals`（`pub(crate)` にした）。
- D5 の照合: debug build の `FunctionEmitter::note_copy`（`read_place` の複製、`shared_array_deref`、place でない `Index` の要素、一時値の field・payload の複製）と
  `check_copy_inventory`（生成の最後に「生成した暗黙の複製 ⊆ `copies::sites`」を `assert!`）。特殊化の worker（`@tz.specialized.`）は記録しない。
  記録を一つ外す変更で assert が落ちることを確かめ、照合が空振りしないことを確認した。
- CLI: `--warn implicit-copy`（check・build・run・test。`fmt` との併用、名前なし、未知の名前、重複は `E2000`）。`W1006` は `copies::warnings` を
  `module.warnings` に連結して表示し、`--deny-warnings` と `--json` に従う。
- std: `Array.copy :: Copy<'a> => ref ['a] -> ['a]`、`List.copy :: Copy<'a> => ref [|'a|] -> [|'a|]`（本体は `*values`）。API 文書を再生成した。
- Phase 2: `src/lsp.rs` が `inlayHintProvider: true` を公開し、`textDocument/inlayHint` で `copies::costly_sites`（W1006 と同じ位置）を返す。
  label は `copy (local)` のような複製の種類、tooltip は W1006 の文、位置は複製する式の終わり。解析に失敗している間は、semantic tokens と同じく
  直前の成功した解析を共通の接頭辞・接尾辞で写して使う（編集で長さが変わった式の hint は出さない）。
- テスト: `tests/copy_cost.rs`（新規 15 件）、`tests/fixtures/explicit_copy/Main.tz` と `tests/features.mjs` の suite `explicit_copy`（10 ケース）、
  `tests/lsp.rs`（`inlay_hints_show_implicit_copies` と capability の一覧）、`vsc/src/test/extension.test.ts`（VS Code が inlay hint を受け取る）。
- 文書: `docs/language.md`、`docs/architecture.md`、`_docs/language-reference/ownership.md`（「暗黙の複製を見つける」と `run=6` の例）、
  `_docs/library-reference/arrays-and-lists.md`・`api/Array.md`・`api/List.md`（再生成）、`_docs/tools/diagnostics.md`・`command-line.md`・`editor-tools.md`、
  `_docs/guides/performance.md`、`_docs/feature-status.md`、`_docs/learn/why-tsuzuri.md`、`vsc/README.md`、`_features/README.md`、`_perfs/README.md`、`_features/GUIDE.md`（D-16・D-30・D-33）。

### 決定事項への追記（チケットから外れた判断）

1. **D3 を承認として扱った。** `W1006` を GUIDE D-16 へ移し、`W1006` と `--warn implicit-copy` を D-30 の仮割り当ての表から削除した（D-33）。
2. **D7 の Phase 2 は inlay hint。** LSP は設定を受けないので、既定無効の W1006 を診断にはせず、同じ位置を inlay hint で常に返す。inlay hint は Problems に入らず、
   エディターの設定で隠せる。`textDocument/hover` の内容は変えず（名前の型と文書を示す既存の契約）、複製の説明は hint の tooltip に入れた。
   関数値の環境の複製は hint にしない（D2 と同じ範囲）。W1006 と inlay hint は `costly_sites`（利用者の関数の `CopyCost::Length` を位置ごとに一件、ソース順）を共有する。
3. **D4 の記録の範囲を一時値の field・payload へ広げた。** B07 の Drop 型の一時値から Copy の field・payload を読むときも生成は複製するので、
   `Temporary` として記録した（D5 の照合もこの経路を記録する）。
4. **`match` の束縛。** 照合対象の射影の複製は、束縛の名前の span で `Payload`・`Tail`・`Element` として記録する。射影を評価する間は記録を止め、二重に数えない。

### 確認（Apple M1 Max、macOS 27.0.1、Apple clang 21、Homebrew LLVM 21、rustc 1.98.1、Node v20.17.0）

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 cargo test --locked`（635 passed、0 failed。
  debug build の全テストで D5 の照合が通る）が成功。`tests/copy_cost.rs` は 15 passed、`tests/lsp.rs` は 23 passed。GUIDE §3.1 の 7 つのテストも既定の stack で成功。
- 停止条件の実行時間: `c995217` と共通の 63 個のテストバイナリの所要時間の和は 644.8 秒から 672.7 秒（+4.3%）で、2 割を下回る。
  差の大半は lib の単体テストの一回だけの遅れ（4.9 秒 → 19.4 秒）で、単独で測り直すと変更前後とも 5.0 秒だった。
- 手順 8: 再現 6 件の IR は `-O0`／`-O3` とも `c995217` と byte 一致し、`--warn implicit-copy` を付けた build も一致。
  `check --warn implicit-copy` の W1006 は「再現」の表のとおり（reused 1、field 2、deref-array 1、deref-list 1、moved 0、scalar 0）。
- fixtures と examples の 156 の IR は 94 が `c995217` と一致し、62 は `Array.copy`・`List.copy` の追加による生成 id の一様なずれだけが異なる（id を写す比較で差なし）。
- E2E: `explicit_copy`（10 ケース。native・WASM × `-O0`／`-O3`、`live == 0`）を含む features 5252 ケースと、`tests/` の E2E の script 30 個
  （`math.mjs` と `windows.mjs` を除く）のうち 29 個が成功。`tests/debug_info.mjs` だけは `c995217` のコンパイラでも同じ `llvm-dwarfdump --verify` の失敗。
- LSP: `tests/lsp_sessions.mjs` が成功。VS Code の結合テストは、ローカルで inlay hint の段（`copy (local)` を受け取る）とその後の整形まで通ることを確かめた。
  この環境ではその後の `executeCompletionItemProvider` で止まるが、`c995217` のサーバーでも同じ所で止まる既知の環境の問題で、全体は CI で確認する。
  inlay hint のための `costly_sites` の追加の解析は、`examples/control` の `check`（約 52 ms）で計測の揺れの範囲だった。
- `node scripts/check-docs.mjs`（97 ページ、851 リンク、184 例、native 302 回）、`npm run test:unit`（vsc。5 件）、
  Windows の `cargo check --all-targets`（x86_64／aarch64-pc-windows-msvc）が成功。
