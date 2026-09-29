# G20: 関数内の型・所有権エラーからの回復

| 項目 | 内容 |
| --- | --- |
| ID | G20 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | G02 |
| 後続 | G12, G13, G17 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要 |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: 一度のビルドで得られる診断が rustc や C# コンパイラより少なく、編集途中のコードで LSP の診断が一件ずつしか出ない |
| 手本にする既存実装 | G02 の poison: `src/polymorph.rs` の `Scheme::poisoned`・`Scheme::is_poisoned`、`src/check.rs` の `Checker::poisoned`・`Checker::finish_expression`・`TypedExpr::error`。関数単位の収集: `src/check.rs` の `check_modules_collect`（`diagnostics.push(classes.derived_error(id, error))`）。所有権の関数単位の収集: `src/ownership.rs` の `check_functions`。テストの形: `tests/diagnostics.rs` の `poisoned_signatures_suppress_dependent_errors_on_every_expression_surface`・`display_cap_counts_all_unique_errors_until_the_separate_hard_limit` |
| 主な影響ファイル | `src/check.rs`（`Checker`, `Checker::expression`, `Checker::composed_expression`, `check_modules_collect`, `recovery_module`（新規））, `src/ownership.rs`（`Checker`, `Checker::access`, `check_body`, `check_functions`, `closed_returns`, `check_recovered`（新規））, `tests/diagnostics.rs`, `docs/language.md`, `docs/architecture.md`, `_docs/tools/diagnostics.md`, `_docs/feature-status.md`, `_features/README.md`。確認のみ（変更しない）: `src/polymorph.rs`, `src/control.rs`, `src/computation.rs`, `src/diagnostic.rs`, `src/main.rs`, `src/lsp.rs`, `src/llvm.rs`, `tests/e2e.mjs` |

## 目的

一つの関数の中にある独立した複数の型エラーと所有権エラーを一度に報告し、回復に起因する二次エラーは一件も出さない。
一度の `check` で得られる診断を増やし、LSP（G12）の publishDiagnostics と REPL（G13）の入力検査の前提にする。
エラーのないプログラムの診断・IR・生成コードは byte 単位で変えない。

Phase 1（型検査の式単位の回復）と Phase 2（所有権検査の回復）の両方を実装対象とする。Phase 1 だけでも出荷できる。
Phase 3（構文の文単位の回復と、回復した本体の意味索引）は設計方針だけを記し、実装しない。

## 着手条件と停止条件

### 着手条件

- G02 が `_features/README.md` の状態欄で done であること。確認: `grep -n "^| G02 " _features/README.md` の行末が `| done |`。
- 承認は不要（決定事項はすべて既定案）。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（stack-depth テスト、`tests/diagnostics.rs` の 11 件、IR）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `Checker::expression` の `Err` を `?` 以外（`.ok()`、`.is_err()`、`match` による別経路の試行、状態の巻き戻し）で扱う呼び出し元が見つかった。
  回復はその推測的な検査を壊す。確認方法は手順 2。
- 回復を足した後、「テスト計画」の stack-depth テスト 7 件のどれかが失敗する。または stack サイズ・`MAX_NESTING`・収集上限を上げたくなった。
- 既存テストの期待値（診断の数・コード・順序・メッセージ、IR）を変える必要がある。
  回復で新しく見つかった独立したエラーのために `analyze(..).unwrap_err()` の先頭が変わるテストも含む（人間が判断する）。
- cascade-negative テストで余分な診断が出て、`Checker::finish_expression` と既存の `contains_error` 分岐だけでは止まらず、
  `src/check.rs`・`src/control.rs`・`src/computation.rs` の外（`src/polymorph.rs` の制約解決など）に分岐を足す必要が出た。
- Phase 2: `FunctionRef::User(id)` の `id` が `function_declarations` の添字と一致しない（手順 9 の `debug_assert` が失敗する）、
  回復用の module で所有権検査が panic する、または回復に起因する `E1012`／`E1013`／`E1014` を D7 の規則で止められない。
- エラーのないプログラムの IR（`--emit llvm` の `-O0`／`-O3`）や警告が変わる。
- `unsafe`、新しい crate、新しい診断コードが必要になった。

## 現状（HEAD `f8dc655` で確認）

- `docs/language.md` の「診断」節は「関数のシグネチャが壊れている場合は、その関数の使用による二次エラーを抑制し、他の関数を検査します」
  「関数内の最初の型／所有権エラーからの完全な回復は行いません」と書く。`_docs/tools/diagnostics.md` と `docs/architecture.md`
  （「不正なシグネチャは名前と ID を残し、返却型が `Type::Error` の poisoned Scheme として表します」の段落）も同じ内容。
- 型検査の各関数は `Result<TypedExpr, Diagnostic>` を返し、`?` で伝播する。`check_modules_collect` は関数ごとに closure の中で
  `checker.expression(&function.body, Some(&signature.result))?` を呼び、`Err` を `diagnostics.push(classes.derived_error(id, error))`
  で記録してその関数を捨てる。**関数本体の最初のエラーで、その本体の検査が終わる**。entry（`$entry`）も同じ形の二つ目の loop。
- ブロックは `Checker::composed_expression` の `ExprKind::Block` arm で、束縛ごとに `self.annotation(ty)` の `.transpose()?`、
  `validate_size(..)?`、`self.expression(&binding.value, annotation.as_ref())?` を順に呼ぶ。どれかが失敗するとブロック全体が失敗する。
- G02 の cascade 抑制は既にある。`Type::Error`（表示は `an erroneous type`）、`Type::contains_error`、`TypedExprKind::Error`、
  `TypedExpr::error(span)`。`Checker::expression` は期待型が `Type::Error` を含むと `self.poisoned = true` にして
  `TypedExpr::error` を返す。poisoned な Scheme の参照（`src/polymorph.rs` の `scheme.is_poisoned()` の分岐）も `self.poisoned = true`
  にして `(TypedExprKind::Error, Type::Error)` を返す。`Checker::finish_expression` は `self.poisoned` のとき、値の型か子の型が
  `Type::Error` を含めば診断なしで `TypedExpr::error(span)` を返す。`src/polymorph.rs` の `unify` は片方が `contains_error` なら成功する。
  フィールド・添字・呼び出し・二項演算・match・for の各所にも `contains_error` の分岐がある（`src/check.rs`、`src/control.rs`）。
- 本体の検査の後、`if checker.poisoned { continue; }` がその関数を `Ok` でも `Err` でも捨てる。したがって poisoned な参照より後にある
  **独立したエラーは報告されない**（再現 d）。
- 二つ目の loop は残った関数に `checker.finish`（`src/polymorph.rs` の `Checker::finish`）と `checker.check_coverage` を行う。
  `polymorph::solve_members` はエラーがないときだけ呼ぶ。その後の `diagnostics.check()?` で、エラーが一件でもあれば
  `semantic::collect`、`constants::fold`、`recursion::check`、`ownership::infer_copy_all`、`polymorph::specialize`、`closures::lower`、
  `ownership::check_all` へ進まない。**型エラーが一件でもあるプロジェクトでは所有権検査が一度も走らない**。
- 所有権検査は `src/ownership.rs` の `check_functions` が関数ごとに `check_body` を呼び、`Err` を一件だけ記録する。
  `check_body` は `Checker`（所有権用）の中の入れ子の本体でも呼ばれる（`self.copy_variables.extend(check_body(..))`）。
  `Checker::access` は移動済みの使用（`E1012` "use of moved or partially moved value '{name}'"）、不変な値への可変アクセス（`E1014`）、
  生きている借用との競合（`E1014`）で最初に `return Err` する。**同じ本体の二つ目の所有権エラーは報告されない**（再現 e）。
- 関数値の参照は `FunctionRef::User(id)` で、所有権検査は `self.module.functions[*id]` と `closed_returns(module)` の添字として使う。
  成功時の `functions` は宣言順に全関数を持つので、`id` は `function_declarations` の添字と一致する（手順 9 で確かめる）。
- 診断の集約は `src/diagnostic.rs` の `Diagnostics`（重複除去、`MAX_UNIQUE_DIAGNOSTICS = 1000` で収集停止）と
  `DiagnosticSet`（`MAX_REPORTED_ERRORS = 50` 件の表示、`omitted`、`omitted_is_lower_bound`）。順序はソース ID・開始位置・終了位置・
  重大度・コード・メッセージ。CLI は `src/main.rs` の `print_diagnostics` が human と JSON lines を出す。LSP は `src/lsp.rs` の
  `ProjectState` の診断を publish する。
- LLVM の入口は `TypedExprKind::Error` を拒否する（`tests/diagnostics.rs` の `error_type_properties_and_codegen_barrier_are_explicit`）。
- `tests/diagnostics.rs` は 11 件のテストを持つ。構文の回復は宣言境界だけ（`parser_recovers_only_at_balanced_top_level_declarations`）。

### 再現（検証済み）

各ケースを `/tmp/tz-work-G20/<case>/Main.tz` に置き、`target/release/tsuzuri check /tmp/tz-work-G20/<case> --json` を実行した。
どのケースも JSON は一行だけ出る。

```tsuzuri
// a: 未定義名 2 件と移動済みの使用 1 件が同じ関数にある
def consume :: string -> unit
fn consume s = ()
def f :: i64
fn f = { let a = missing_one; let s = "x"; consume s; consume s; let b = missing_two; 0 }
```

```tsuzuri
// b: 型不一致 2 件
fn f() -> i64 { let a: i64 = true; let b: bool = 1; 0 }
```

```tsuzuri
// c: 未定義名の連鎖（正しい報告は 1 件）
fn f() -> i64 { let x = missing; let y = x + 1; let z = y.field; x }
```

```tsuzuri
// d: poisoned な参照の後の独立したエラー
def bad :: Missing -> i64
fn bad x = x
def caller :: i64
fn caller = { let a = bad 1; let b: i64 = true; 0 }
```

```tsuzuri
// e: 別々の値の移動済みの使用 2 件
def consume :: string -> unit
fn consume s = ()
def f :: unit
fn f = { let s = "a"; let t = "b"; consume s; consume s; consume t; consume t }
```

```text
a: E1002 unknown value 'missing_one'                      （E1012 と 2 件目の E1002 は出ない）
b: E1003 expected i64, found bool                         （2 件目の E1003 は出ない）
c: E1002 unknown value 'missing'                          （正しい。回復後も 1 件のまま）
d: E1004 unknown record, union, or type alias 'Missing'   （caller の E1003 は出ない）
e: E1012 use of moved or partially moved value 's'        （'t' の E1012 は出ない）
```

コメント行は説明用で、検証したファイルには含めていない。

## 仕様

### 前提とする他チケットのインターフェース

- G02（done）: `Diagnostics`・`DiagnosticSet`・poisoned Scheme・`Type::Error`・`TypedExprKind::Error`。HEAD にあり、形を変えない。
- G12・G13 へ渡すもの: 同じ `DiagnosticSet` に、独立したエラーがより多く入る。G12 D1 は `ProjectState::diagnostics` の `Severity::Error`
  の有無で索引の使い方を決めるので、G20 の後もそのまま動く。意味索引（`semantic::collect`）はエラーがあれば作らない（Phase 3 まで不変）。

### Phase 1: 型検査の式単位の回復

- R1 回復点: 関数本体と entry を検査する `Checker` だけ `recovering`（新規）を true にする。`Checker::expression` の dispatch が `Err(d)` を
  返したら、`d` を `Checker::recovered`（新規）へ積み、`self.poisoned = true` にして `Ok(TypedExpr::error(expression.span))` を返す。
  `expression` を通る部分式はすべて回復点で、最も内側の `expression` が捕まえる。親は子の `Type::Error` を見て R3 で黙る。
- R2 ブロックの束縛: `Checker::composed_expression` の `ExprKind::Block` arm で、注釈の解決（`self.annotation`）か `validate_size` が
  失敗したら、その診断を `recovered` へ積み、値は期待型なし（`None`）で検査し、束縛は `Type::Error` にする。値の失敗は R1 が
  `TypedExpr::error` にするので、束縛は自動的に `Type::Error` になる（未定義名を束縛した `let x = missing` の `x` は `Type::Error`）。
- R3 cascade 抑制: 既存の規則をそのまま使う。`self.poisoned` のとき、`Checker::finish_expression` は値の型か子の型が `Type::Error` を
  含む式を診断なしで `TypedExpr::error` にする。`unify` は `Type::Error` と何でも一致する。フィールド・添字・呼び出し・演算・match・for の
  `contains_error` 分岐も既存のまま。したがって `Type::Error` の部分式を含む式は、それ以上何も報告しない。
  兄弟の部分式が持つ独立したエラーは、それぞれの R1 で先に報告される（`missing_one + missing_two` は 2 件）。
- R4 状態の復元: R1 で回復するとき、`Checker` の入れ子状態を dispatch 前の値へ戻す。対象は `scopes` の長さ、`normal_loop_depth`、
  `computation_depth` の 3 つ（`RecoveryMark`（新規））。`Block` arm の `self.scopes.push` の後で `?` が抜けると scope が残るため。
  蓄積だけの field（`constraints`、`members`、`undecided_borrows`、`families`、`coverage`、`borrowed`、`shadowing_warnings`）は戻さない。
- R5 回復しないもの: 本体より前の `regions::contract`、`classes.constraint_kinds`、`computation::expand` の失敗は従来どおりその関数の
  唯一の診断にする。`recovered` を持つ関数は従来の poisoned と同じく二つ目の loop（`finish`・`check_coverage`）へ進めないので、
  そこで出る `E1015`・`E1021`・undecided borrow などは、回復したエラーのない関数でだけ報告される。`infer_active_result` など本体以外の
  `Checker` は `recovering` が false のままで、`Err` をそのまま返す。
- R6 本体の後: `recovered` の各診断を `classes.derived_error(id, _)`（entry はそのまま）で `diagnostics` へ入れ、続いて `Err` があれば
  従来どおり入れる。`checker.poisoned` の関数は `functions` へ入れない（従来どおり）。
- R7 上限: `recovered.len()` が `MAX_UNIQUE_DIAGNOSTICS` に達したら R1 は捕まえずに `Err` を返し、その本体の検査を終える。

### Phase 2: 所有権検査の回復

- O1 同じ本体の複数エラー: `src/ownership.rs` の `Checker::access` の 3 つの `return Err`（移動済みの使用 `E1012`、可変アクセスの
  `E1014`、借用との競合 `E1014`）を `Checker::report`（新規）へ置き換える。`report` は `place.root` が `reported`（新規）にあれば黙って
  `Ok(())`、なければ診断を `recovered`（新規）へ積み、root を `reported` へ入れて `Ok(())` を返す。競合では競合した各 loan の
  `place.root` も `reported` へ入れる。`access` は `report` の後すぐ `Ok(())` で戻る。`access` 以外の `E1012`／`E1013`／`E1014` の
  `return Err` は変えない（その本体の最後の診断になる）。`infer` の真偽どちらでも同じ。
- O2 型エラーのあるプロジェクト: 二つ目の loop の後でエラーがあり、`diagnostics.is_full()` でなければ、`recovery_module`（新規）で
  関数 ID と添字をそろえた `CheckedModule` を作り、`ownership::check_recovered`（新規）の診断を `diagnostics` へ入れてから、従来どおり
  `diagnostics.check()?` で止まる。検査する関数は、二つ目の loop を通った関数と、`checker.finish` が成功した回復済みの関数。
  回復済みの関数の `finish` の `Err` は報告せず、その関数を stub にする。`check_coverage` は回復済みの関数に呼ばない。
- O3 stub: 検査できない宣言（poisoned、本体前の失敗、`finish` の失敗）は、本体が `TypedExpr::error(span)`、`parameters` が空、
  `signature` が Scheme の `signature` の `CheckedFunction` で埋める。stub の本体は検査しない。stub への呼び出しの結果は loan を持たない
  （`closed_returns` と `Checker::callback_returns_closed` は本体が `E::Error` の関数を closed とみなす）。
- O4 Error 節点: `Checker::eval_value` は既に `E::Error` を何もしない arm に持つ。R3 が親の式ごと `TypedExpr::error` にするので、
  壊れた呼び出しの引数の move・借用は所有権検査に現れない。`report` は root の局所変数の型が `contains_error` なら診断を捨てる。
- O5 結果: 回復は偽陰性（修正後に初めて見えるエラー）を許し、偽陽性を許さない。回復 pass は Copy 制約を捨て、特殊化後の
  `ownership::check_all` は従来どおりエラーのないプロジェクトでだけ走る。

### 順序・上限・出力

- 順序は変えない。`Diagnostics` がソース ID・開始位置・終了位置・重大度・コード・メッセージで並べるので、同じファイルでは関数をまたいで
  位置順、ファイルはソース ID 順になる。同じ位置・コード・メッセージの診断は一件にまとまる。
- 上限は変えない。表示 `MAX_REPORTED_ERRORS = 50`、収集 `MAX_UNIQUE_DIAGNOSTICS = 1000`、省略通知の文言も同じ。
- `--json` の形式（1 行 1 オブジェクト、キー、省略通知の `note`）と human 出力の形式は変えない。行数だけが増える。
- エラーが一件でもあれば `check`・`build`・`run`・`test`・`doc` は従来どおり止まり、LLVM・リンク・実行へ進まず、既存の出力を変えない。
  警告もエラーがあれば出さない（従来どおり）。
- 旧 API（`analyze`、`check_modules`、`ownership::check`）は並べた集合の先頭を返す。一つの関数に複数のエラーがあるソースでは、
  回復で見つかった位置の早いエラーが先頭になりうる。

### 診断

新しい診断コードは足さない。メッセージも変えない。変わるのは一つの本体で報告される件数だけ。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E1001`–`E1010`, `E1020`, `E1022`–`E1024` など式の検査で出るもの | R1・R2 の回復点ごとに独立して報告 | 既存のまま（例: `unknown value 'missing_one'`, `expected i64, found bool`） | 既存のまま |
| `E1012` | `Checker::access` の移動済みの使用。root ごとに最初の一件 | `use of moved or partially moved value '{name}'` | 使用位置 |
| `E1014` | `Checker::access` の可変アクセス・借用競合。root ごとに最初の一件 | 既存の 2 文 | アクセス位置 |
| `E1012`（`access` 以外）, `E1013` | 本体の最初の一件で、その本体の所有権検査を終える（従来どおり） | 既存のまま | 既存のまま |
| `E1015`, `E1021` など `finish`・`check_coverage` のもの | 回復したエラーのない関数だけ（従来どおり） | 既存のまま | 既存のまま |

### 資源上限

- 回復は新しい再帰を足さない。深さは従来どおり `MAX_NESTING = 128`（`src/syntax.rs`）で制限され、R1 は `expression` の各段で
  `RecoveryMark`（usize 3 個）だけを frame に足す。回復の処理は `#[cold]`・`#[inline(never)]` の `Checker::recover_expression`（新規）へ出す。
- 一つの本体の回復済み診断は R7 で `MAX_UNIQUE_DIAGNOSTICS` 件まで。所有権の `recovered` も同じ上限で止める。

### 例

実装後の期待（「再現」のソースをそのまま使う。コードは位置順）。

| ケース | HEAD | 実装後 | 段 |
| --- | --- | --- | --- |
| a | `["E1002"]` | `["E1002", "E1012", "E1002"]` | Phase 1 + O2 |
| b | `["E1003"]` | `["E1003", "E1003"]` | Phase 1 |
| c | `["E1002"]` | `["E1002"]` | R3（cascade なし） |
| d | `["E1004"]` | `["E1004", "E1003"]` | R1 + R6 |
| e | `["E1012"]` | `["E1012", "E1012"]` | O1 |

### Phase 3（設計方針・実装しない）

- 構文の文単位の回復: ブロック内の束縛の境界で回復し、壊れた束縛を `ExprKind::Error`（新規）に置き換える。LSP と `check` だけがその
  AST を後段へ渡す。stack-depth の parser 規則（GUIDE §11.1）を満たす設計を別チケットで行う。
- 意味索引: 回復済みの本体を `semantic::collect` へ渡し、hover と G12 の補完に使う。`finish` を通らない型は未確定なので、表示規則を
  G12 と合わせて決める。

## 設計

### データ構造

```rust
// src/check.rs
struct Checker<'a> {
    // 既存の field …
    poisoned: bool,
    recovering: bool,          // 新規: 本体と entry の loop だけ true
    recovered: Vec<Diagnostic>, // 新規: 回復した診断（検出順）
}

#[derive(Clone, Copy)]
struct RecoveryMark {          // 新規
    scopes: usize,
    normal_loop_depth: usize,
    computation_depth: usize,
}
```

```rust
// src/ownership.rs
struct Checker<'a> {
    // 既存の field …
    recovered: Vec<Diagnostic>, // 新規
    reported: BTreeSet<usize>,  // 新規: 診断済みの place.root
}

fn check_body(/* 既存の引数 */, recovered: &mut Vec<Diagnostic>) -> Result<BTreeSet<String>, Diagnostic>;
pub(crate) fn check_recovered(module: &CheckedModule, checkable: &[bool]) -> Vec<Diagnostic>; // 新規
```

`check_body` は `recovered` を `std::mem::take` で `Checker::recovered` へ移し、戻る前（`Ok`・`Err` とも）に書き戻す。
入れ子の本体の呼び出し（`E::Lambda` arm の `check_body(..)`）は `&mut self.recovered` を渡す。`reported` は本体ごとに空から始める。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 検査 | `src/check.rs` | `Checker`, `Checker::new` | `recovering: false`, `recovered: Vec::new()` |
| 検査 | `src/check.rs` | `Checker::expression` | 先頭の既存の `contains_error` 分岐の後で `RecoveryMark` を取り、dispatch の `match` を `let result = …;` にする。`Err` かつ `recovering` なら `recover_expression` |
| 検査 | `src/check.rs` | `Checker::recover_expression`（新規） | R4 の復元、R7 の上限、`recovered.push`、`poisoned = true`、`Ok(TypedExpr::error(span))` |
| 検査 | `src/check.rs` | `Checker::composed_expression` の `ExprKind::Block` arm | R2。注釈と `validate_size` の `?` を `recovering` のときだけ回復に置き換える |
| 検査 | `src/check.rs` | `check_modules_collect` の関数 loop と entry loop | 本体前の処理の後で `checker.recovering = true`。R6。Phase 2 のため `ids`（新規、`functions` と並ぶ宣言 ID）と回復済みの関数 `(id, CheckedFunction, Checker)` を残す |
| 検査 | `src/check.rs` | `check_modules_collect` の二つ目の loop の後 | O2。`diagnostics.check()` が `Err` のときだけ `recovery_module` と `ownership::check_recovered` |
| 検査 | `src/check.rs` | `recovery_module`（新規） | O3。長さ `function_declarations.len()`（entry があれば +1）の `functions` を作る |
| 検査 | `src/check.rs` | `Checker::finish_expression`, `infer_active_result` | 変更なし |
| 検査 | `src/polymorph.rs` | `Checker::function`, `unify`, `Checker::finish`, `solve_members` | 変更なし（`Type::Error` の既存分岐をそのまま使う） |
| 検査 | `src/control.rs`, `src/computation.rs` | `control_expression`, `check_implicit` | 変更なし。停止条件の範囲で `contains_error` 分岐を足すことだけ許す |
| 所有権 | `src/ownership.rs` | `Checker`, `check_body`, `check_functions` | `recovered`・`reported`。`check_functions` は `Err` の前に `recovered` を `diagnostics` へ入れる |
| 所有権 | `src/ownership.rs` | `Checker::access`, `Checker::report`（新規） | O1 |
| 所有権 | `src/ownership.rs` | `closed_returns`（`closed`）, `Checker::callback_returns_closed` | 本体が `E::Error` の関数は closed（O3） |
| 所有権 | `src/ownership.rs` | `check_recovered`（新規） | `checkable[id]` の関数だけ `check_body(.., infer: true, ..)`。制約は捨て、`recovered` と `Err` を返す |
| 所有権 | `src/ownership.rs` | `Checker::eval_value` | 変更なし（`E::Error` は既に何もしない） |
| LLVM・CLI・LSP | `src/llvm.rs`, `src/main.rs`, `src/lsp.rs`, `src/semantic.rs` | `emit`, `print_diagnostics`, `ProjectState` | 変更なし |

### 生成 IR とランタイム

変更しない。回復は検査中だけの状態で、エラーのあるプロジェクトは `diagnostics.check()?` で LLVM より前に止まる。
エラーのないプログラムでは `recovered` が空、`recovery_module` は呼ばれず、IR は byte 単位で同じ。

### アルゴリズム

```text
for each declaration id (then entry):
    pre-body steps (regions, constraint kinds, expand)   // Err → push, stub
    checker.recovering = true
    body = checker.expression(body, Some(result))        // R1/R2 inside
    push derived_error(id, d) for d in checker.recovered  // R6
    if checker.poisoned: keep (id, function, checker) for Phase 2; continue
    functions.push(function); ids.push(id)
second loop: finish + coverage (unchanged)
if errors and not full:                                   // Phase 2 (O2)
    for each recovered (id, f, checker): checker.finish(&mut f.body) or stub
    module = recovery_module(ids, functions, recovered, stubs)
    diagnostics.extend(ownership::check_recovered(&module, checkable))
diagnostics.check()?                                      // unchanged barrier
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。手順 1–6 が Phase 1、手順 7–9 が Phase 2。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の a–e と「テスト計画」の f–s を `/tmp/tz-g20/<case>/Main.tz` に置き、
  JSON を保存する。examples の IR を保存する（build に失敗する example は除き、名前を記録する）。
- 確認: `diagnostics` は `11 passed`。stack-depth の 7 件はそれぞれ `1 passed`。各ケースの JSON は一行。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked --test diagnostics
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test control control_syntax_has_bounded_depth
cargo test --locked --test currying rejects_invalid_definitions_and_bounds_lambda_nesting
cargo test --locked --test tasks bounds_nested_task_syntax_and_types
cargo test --locked --test iteration_protocol bounds_expanded_sequence_loop_nesting
mkdir -p /tmp/tz-g20/before
for e in hello control functional polymorphism tasks; do for o in -O0 -O3; do
  target/release/tsuzuri build examples/$e --emit llvm $o -o /tmp/tz-g20/before/$e$o.ll; done; done
for c in a b c d e f g h i j k l m n o p q r s; do
  target/release/tsuzuri check /tmp/tz-g20/$c --json 2> /tmp/tz-g20/before/$c.json; done
```

### 手順 2: 推測的な呼び出しと入れ子状態の棚卸し（変更なし）

- 変更: なし。
- 内容: 次の 2 つの grep の結果をすべて読む。1 つ目では、`expression(` の結果が `?`、関数の末尾の値、または
  `let result = self.expression(..);` の後で状態を戻して `result` を返す形（`ExprKind::ComputationBoundary` arm）だけであることを確かめる。
  2 つ目では、`?` をまたいで保存・復元される field が R4 の 3 つだけであることを確かめる。
- 確認: 例外が見つかれば停止条件。結果の要約を完了報告に書く。

```sh
grep -nE "\.expression\(" src/check.rs src/control.rs src/computation.rs src/polymorph.rs | grep -v "?"
grep -nE "self\.(scopes|normal_loop_depth|computation_depth|active_result|type_parameters|kinds|members)\b.*(=|push|pop|truncate)|mem::(take|replace)\(&mut self\." src/check.rs src/control.rs src/computation.rs src/polymorph.rs
```

### 手順 3: 回復の器を足す（まだ無効）

- 変更: `src/check.rs` の `Checker`、`Checker::new`、`Checker::expression`、`RecoveryMark`（新規）、`Checker::recover_expression`（新規）。
- 内容: 「データ構造」のとおり。`expression` は既存の `contains_error` 分岐の後で mark を取り、dispatch を `let result = match … ;` にして、
  `Err(error) if self.recovering` のときだけ `self.recover_expression(error, mark, expression.span)` を返す。`recover_expression` は
  `#[cold]`・`#[inline(never)]` で、R7 の上限を超えていれば `Err(error)`、そうでなければ scope を `truncate`、2 つの深さを戻し、
  `recovered.push(error)`、`poisoned = true`、`Ok(TypedExpr::error(span))`。`use crate::diagnostic::MAX_UNIQUE_DIAGNOSTICS` を足す。
- 確認: `cargo test --locked --test diagnostics` が `11 passed`。stack-depth の 7 件が成功。`cargo test --locked` が成功する。

### 手順 4: 本体と entry で回復を有効にする

- 変更: `src/check.rs` の `check_modules_collect`（関数 loop と entry loop）、`tests/diagnostics.rs`。
- 内容: 本体前の処理（`regions::contract`、`constraint_kinds`、`computation::expand`）の後、`checker.expression(..)` の直前に
  `checker.recovering = true`。closure の後、`if checker.poisoned` より前に R6 を行う（関数は `classes.derived_error(id, d)`、entry は `d`）。
  テスト `recovers_independent_type_errors_within_one_function`（新規）と `recovery_does_not_report_cascades`（新規）を足す。
- 確認: `cargo test --locked --test diagnostics` が `13 passed`。`cargo test --locked` が成功する（既存の期待値が変われば停止）。
  stack-depth の 7 件が成功する。

### 手順 5: ブロックの注釈の回復（R2）

- 変更: `src/check.rs` の `Checker::composed_expression` の `ExprKind::Block` arm、`tests/diagnostics.rs`。
- 内容: `recovering` のとき、`self.annotation(ty)` と `validate_size` の `Err` を `recovered` へ積み、注釈なしとして値を検査し、
  `bind` には `Type::Error` を渡す。`recovering` が false の経路は `?` のまま。テスト (1) に `j2`、(2) に `j` を足す。
- 確認: `cargo test --locked --test diagnostics` が `13 passed`。`cargo test --locked` が成功する。

### 手順 6: 状態の復元・上限・深さのテスト

- 変更: `tests/diagnostics.rs`。
- 内容: `recovery_restores_scopes_after_a_failed_construct`、`recovery_limits_match_the_existing_caps`、`recovery_keeps_stack_depth_bounded`
  （すべて新規）を足す。
- 確認: `cargo test --locked --test diagnostics` が `16 passed`。stack-depth の 7 件が成功する。ここで Phase 1 は出荷できる。

### 手順 7: 同じ本体の複数の所有権エラー（O1）

- 変更: `src/ownership.rs` の `Checker`（`recovered`、`reported`）、`Checker::report`（新規）、`Checker::access`、`check_body`、
  `check_functions`、入れ子の `check_body` の呼び出し、`tests/diagnostics.rs`。
- 内容: 「データ構造」と O1 のとおり。`check_functions` は各本体の後で `recovered` を `diagnostics.extend` し、`Err` があれば続けて入れる。
  `infer_copy_all` の `Err` の型（`Vec<Diagnostic>`）は変えない。テスト `recovers_ownership_errors_on_distinct_roots`（新規）を足す。
- 確認: `cargo test --locked --test diagnostics` が `17 passed`。
  `cargo test --locked --test diagnostics collects_ownership_errors_during_inference_instead_of_stopping_at_first_function` が `1 passed`。
  `cargo test --locked` が成功する。

### 手順 8: 回復用 module と stub（O3）

- 変更: `src/check.rs` の `check_modules_collect`（`ids`、回復済みの関数の保持）、`recovery_module`（新規）、`src/ownership.rs` の
  `closed_returns`（`closed`）、`Checker::callback_returns_closed`。
- 内容: 成功時の経路で `debug_assert_eq!(ids, (0..function_declarations.len()).collect::<Vec<_>>())`（entry を除く）を入れ、
  `FunctionRef::User(id)` が宣言の添字であることを全テストで確かめる。`closed` は本体が `E::Error` なら true、`callback_returns_closed` は
  参照先の本体が `E::Error` なら true を返す。まだ `check_recovered` は呼ばない。
- 確認: `cargo test --locked` が成功する（`debug_assert` が失敗したら停止条件）。

### 手順 9: 型エラーのあるプロジェクトの所有権検査（O2）

- 変更: `src/check.rs` の `check_modules_collect`、`src/ownership.rs` の `check_recovered`（新規）、`Checker::report`（O4 の root の型）、
  `tests/diagnostics.rs`。
- 内容: 「アルゴリズム」のとおり。回復済みの関数に `checker.finish` を行い、`Err` なら stub。`check_recovered` の診断を入れてから
  従来の `diagnostics.check()?`。テスト `checks_ownership_of_recovered_bodies_without_cascades` と
  `cli_json_lists_every_recovered_error_in_order`（新規）を足す。
- 確認: `cargo test --locked --test diagnostics` が `19 passed`。`cargo test --locked` が成功する。

### 手順 10: 不変の確認

- 変更: なし。
- 内容: release を作り直し、手順 1 の IR と `cmp` する。E2E の診断 suite を実行する。
- 確認: `cmp` がすべて一致。`node tests/e2e.mjs target/release/tsuzuri` が成功。stack-depth の 7 件が成功。
  f–s の JSON が「テスト計画」の期待と一致する。

```sh
cargo build --release --locked
for e in hello control functional polymorphism tasks; do for o in -O0 -O3; do
  target/release/tsuzuri build examples/$e --emit llvm $o -o /tmp/tz-g20/$e$o.ll
  cmp /tmp/tz-g20/before/$e$o.ll /tmp/tz-g20/$e$o.ll; done; done
node tests/e2e.mjs target/release/tsuzuri
```

### 手順 11: 文書と最終確認

- 変更: 「ドキュメント」の各ファイル。
- 内容: 回復の規則（R1–R7 の利用者向けの要点、O1・O2、回復しないもの）を書く。
- 確認: `node scripts/check-docs.mjs _docs/tools/diagnostics.md _docs/feature-status.md` が成功。`cargo test --locked` が成功。
  GUIDE §10 の完了の定義を満たす。

## テスト計画

### Rust テスト

すべて `tests/diagnostics.rs` に足す。補助 `codes(source) -> Vec<&'static str>`（新規）は既存の `errors(source)` の
`diagnostics` のコードを順に返す（`errors` は 2 回の実行の一致と旧 API の先頭を既に確かめる）。`C` は
`def consume :: string -> unit\nfn consume s = ()\n`。期待値は仕様の規則から手で数えたもので、実装の出力から作らない。

| テスト | ソース | 期待 |
| --- | --- | --- |
| (1) `recovers_independent_type_errors_within_one_function` | b | `["E1003", "E1003"]` |
| (1) | f `fn f() -> i64 { missing_one + missing_two }` | `["E1002", "E1002"]`、message は `'missing_one'`・`'missing_two'` の順 |
| (1) | g `def add :: i64 -> i64 -> i64\nfn add x y = x + y\ndef f :: i64\nfn f = add missing true` | `["E1002", "E1003"]` |
| (1) | q `fn f() -> i64 { let l = [m0, m1, m2]; 0 }` | `["E1002", "E1002", "E1002"]` |
| (1) | s（entry）`missing_one + missing_two` | `["E1002", "E1002"]` |
| (1) | d | `["E1004", "E1003"]` |
| (1) | j2 `fn f() -> i64 { let x: Missing = 1; let y: i64 = true; 0 }` | `["E1004", "E1003"]` |
| (2) `recovery_does_not_report_cascades` | c | `["E1002"]` |
| (2) | h `fn f() -> i64 { let x = missing; if x then 1 else x }` | `["E1002"]` |
| (2) | i `fn f() -> i64 { let x = missing; match x with \| Some y -> y \| None -> 0 }` | `["E1002"]` |
| (2) | j `fn f() -> i64 { let x: Missing = 1; x + 1 }` | `["E1004"]` |
| (2) | k `fn f() -> i64 { let g = y -> missing; g 1 }` | `["E1002"]` |
| (3) `recovery_restores_scopes_after_a_failed_construct` | l `fn f() -> i64 { let r = match (1, 2) with \| (y, true) -> y \| _ -> 0; y }` | `["E1003", "E1002"]`、2 件目は `unknown value 'y'` |
| (7) `recovers_ownership_errors_on_distinct_roots` | e | `["E1012", "E1012"]`（`'s'`、`'t'`） |
| (7) | m `C` + `def f :: unit\nfn f = { let s = "a"; consume s; consume s; consume s }` | `["E1012"]` |
| (7) | n `C` + `def f :: unit\nfn f = { let s = "a"; let r = &s; consume s; r; let t = "b"; consume t; consume t }` | `["E1014", "E1012"]` |
| (8) `checks_ownership_of_recovered_bodies_without_cascades` | a | `["E1002", "E1012", "E1002"]` |
| (8) | o `C` + `def f :: unit\nfn f = { let s = "a"; missing s; consume s }` | `["E1002"]` |
| (8) | p `C` + `def f :: unit\nfn f = { let x = missing; consume x; consume x }` | `["E1002"]` |
| (8) | `C` + `def g :: i64\nfn g = true\ndef f :: unit\nfn f = { let s = "a"; consume s; consume s }` | `["E1003", "E1012"]` |
| (8) | `C` + `def h :: string -> string\nfn h s = missing\ndef f :: unit\nfn f = { let s = "a"; let t = h s; consume s }` | `["E1002", "E1012"]` |

- (2) は各ケースで、どの message も `erroneous` を含まないことも確かめる。(3) が `["E1003"]` になったら、pattern 変数が push した
  scope の外へ束縛されている。停止条件として報告する（期待値を変えない）。
- (4) `recovery_limits_match_the_existing_caps`: 一つの関数に未定義名 62 個（`[m0, …, m61]`）で、表示 50、`omitted == 12`、
  `!omitted_is_lower_bound`、`omission_note()` が `Some("12 more errors not shown")`。100 要素の list を 11 個の `let` に置いた
  1,100 個では、表示 50、`omitted == 950`、`omitted_is_lower_bound`、`Some("at least 950 more errors not shown")`。
  list の要素数と束縛数は `MAX_NESTING`（128）未満に保つ。
- (5) `recovery_keeps_stack_depth_bounded`: r（括弧 100 段の `missing` と `let b: bool = 1`）が `["E1002", "E1003"]`。
  `(0..100).fold("missing100".to_string(), |inner, i| format!("(missing{i} + {inner})"))` を本体にした関数で、表示 50、`omitted == 51`。
  debug build の既定のテスト thread（2 MiB）で走らせ、thread を作らない。
- (10) `cli_json_lists_every_recovered_error_in_order`: `tests/warnings.rs` の `cli_caps_warnings_and_denies_before_touching_artifacts` と
  同じ temp dir・`Command` の作り方で、a を `check <dir> --json` する。終了コード 1、stderr は 3 行、各行が `{"severity":"error","code":"`
  で始まり、コードは `E1002`・`E1012`・`E1002` の順、note 行はない。
- f–s の HEAD の結果は、手順 1 の JSON（各一行）で確認済み。f・g・h・i・k・o・p・q・r・s は `E1002`、j は `E1004`、l は `E1003`、
  m は `E1012`、n は `E1014`。

### E2E

codegen を変えないので新しい E2E suite は作らない。`node tests/e2e.mjs target/release/tsuzuri`（JSON lines、62 件と 1,003 件の省略通知、
出力保護）を手順 10 で実行し、変化がないことを確かめる。native／WASM × `-O0`／`-O3` の差は生じない（手順 10 の IR の `cmp`）。

### 既存テストへの影響

なし（期待）。特に `poisoned_signatures_suppress_dependent_errors_on_every_expression_surface` の 26 本体は、どれも独立した二つ目の
エラーを持たないので 2 件のまま。変わるテストがあれば停止条件。

### 性能

エラーのないプログラムでは `recovering` の分岐と `RecoveryMark` の保存だけが増える。閾値は置かない。
`/usr/bin/time -l target/release/tsuzuri check examples/control` の前後を記録するだけにする。

## ドキュメント

- `docs/language.md` の「診断」節: 「関数内の最初の型／所有権エラーからの完全な回復は行いません。」を、式単位の回復、二次エラーの抑制、
  値ごとの所有権エラー、回復しないもの（本体前の失敗、`finish`・網羅性のエラー、`access` 以外の所有権エラーは本体ごとに一件）に置き換える。
- `docs/architecture.md`: 「不正なシグネチャは名前と ID を残し…」の段落へ回復点（`Checker::expression`、ブロック注釈）、状態の復元、
  回復用 module と `ownership::check_recovered`、「Type／式の Error は検査中だけの状態」の不変条件を足す。テストの説明
  「型／所有権の関数単位の継続」を「関数内の回復」に直す。
- `_docs/tools/diagnostics.md`: 「関数内の最初の型・所有権エラーから完全に回復して全問題を報告する保証はありません。」を新しい挙動に直す。
- `_docs/feature-status.md` と `_features/README.md`: G20 の状態。

## 受け入れ条件

- [ ] 「例」の a–e と「テスト計画」の表のすべてのソースで、コードの列が期待と一致する。
- [ ] cascade-negative（c、h、i、j、k、o、p）で余分な診断がなく、どの message も `erroneous` を含まない。
- [ ] 表示 50 件・収集 1,000 件・省略通知・JSON lines の形式が変わらない（(4)、(10)、`tests/e2e.mjs`）。
- [ ] エラーのない examples の IR が `-O0`／`-O3` で byte 単位で同じ。エラーがあれば LLVM へ進まない。
- [ ] stack-depth の 7 件が成功し、stack サイズ・`MAX_NESTING`・上限を変えていない。
- [ ] 既存テストの期待値を変えていない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `Block` arm は `self.scopes.push` の後に `?` がある。R4 なしで回復すると内側の束縛が外へ漏れ、偽陰性になる（(3) で検出）。
- `if checker.poisoned { continue; }` は `Err` も捨てる。R6 をこの行より前に置かないと、回復した診断が消える。
- 回復の処理を `expression` に inline で書くと debug build の frame が増え、stack-depth テストが落ちる。`#[cold]`・`#[inline(never)]` の
  補助関数へ出す。上限や stack を上げて通さない。
- `infer_active_result` の `Checker` で回復を有効にすると、active pattern の結果型に `Type::Error` が入り、signature が壊れる。
  `recovering` は本体と entry の loop でだけ true にする。
- 計算式は `computation::expand` 後の式を検査するので、同じ位置の診断が重複しうる。`Diagnostics` の重複除去に任せ、手で除かない。
- 所有権の入れ子の `check_body` に `&mut self.recovered` を渡し忘れると、lambda 本体の回復した診断が消える。
- stub を closed でないとみなすと、呼び出しの結果が引数の loan を持ち続け、偽の `E1014`／`E1013` が出る（O3）。
- `solve_members` はエラーがあると呼ばれないので、回復済みの関数の `finish` はメンバー制約で失敗しうる。その `Err` は報告せず stub にする。
- `cargo test --locked <pattern>` は 0 件でも成功する。`running N tests` を必ず見る。

## 対象外

- Phase 3（構文の文単位の回復、回復した本体の意味索引と hover）。自動修正の提案（G12 の code action）。
- 本体前の段（`regions::contract`、制約の kind、計算式の展開）の回復。回復済みの関数の `finish`・網羅性の診断。
- 型エラーのあるプロジェクトでの特殊化後の所有権検査（`ownership::check_all`）、`access` 以外の所有権エラーの複数報告。
- 字句・構文の回復範囲の変更、新しい診断コード。

## 決定事項

### D1: 回復点

- 決定: 回復点は `Checker::expression`（R1）とブロック束縛の注釈（R2）だけにし、関数本体と entry の検査でだけ有効にする。
- 理由: すべての部分式が `expression` を通るので、一か所で最も内側の失敗を捕まえられる。本体以外の `Checker` の用途（active pattern の
  結果型の推論）は `Err` に依存する。
- 状態: 既定案（実装者はこの案に従う）

### D2: 二次エラーの抑制

- 決定: 新しい抑制規則を作らず、G02 の `poisoned` と `finish_expression`・`unify`・既存の `contains_error` 分岐を使う。
- 理由: G02 のテストが 26 の式の形で抑制を確かめている。規則を一つに保つと LSP と CLI で結果が一致する。
- 状態: 既定案（実装者はこの案に従う）

### D3: 状態の復元

- 決定: 回復時に `scopes` の長さ、`normal_loop_depth`、`computation_depth` を dispatch 前へ戻す。蓄積だけの field は戻さない。
- 理由: 手順 2 の棚卸しの範囲で、`?` をまたいで入れ子になる状態はこの 3 つ。蓄積 field は poisoned な関数で後段へ渡らない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 回復した関数の扱い

- 決定: 回復した関数は従来の poisoned と同じく `functions` へ入れず、`finish`・網羅性・特殊化・コード生成へ進めない。
  エラーがあれば `build`／`run` は LLVM へ進まない。Phase 1 ではエラー型を含む関数の所有権検査を行わない（旧未決事項の既定案を保つ）。
- 理由: `Type::Error` が LLVM へ届かないことを構造で保証する（GUIDE §1.9）。
- 状態: 既定案（実装者はこの案に従う）

### D5: 同じ本体の所有権エラー

- 決定: `Checker::access` の 3 つの診断だけを root ごとに最初の一件で報告して続ける。ほかの所有権診断は本体の最後の一件のまま。
- 理由: 移動済みの使用と借用の競合が編集中に最も多く、root 単位の抑制で連鎖を止められる。寿命の診断は loan の連鎖が長く、抑制規則が要る。
- 状態: 既定案（実装者はこの案に従う）

### D6: 型エラーのあるプロジェクトの所有権検査（Phase 2）

- 決定: 関数 ID と添字をそろえた回復用 module を作り、`check_body` を infer mode で一度だけ走らせ、Copy 制約は捨てる。
- 理由: 所有権検査は `self.module.functions[id]` で関数を引くので、そろえないと別の関数を読むか panic する。infer mode は型変数の Copy を
  制約として扱い、未解決の多相で偽の診断を出さない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 偽陽性を出さない

- 決定: Error 節点は所有権で何もしない。stub は closed。root の型が `contains_error` の診断は捨てる。回復済みの関数の `finish` の `Err` は捨てる。
- 理由: 回復は偽陰性を許し偽陽性を許さない（O5）。修正後の再検査で残りのエラーは見える。
- 状態: 既定案（実装者はこの案に従う）

### D8: 上限・順序・出力

- 決定: `MAX_REPORTED_ERRORS`・`MAX_UNIQUE_DIAGNOSTICS`・並び順・JSON と human の形式を変えない。一つの本体の回復は R7 で止める。
- 理由: 利用者と LSP の出力互換を保つ。上限の意味は G02 で決まっている。
- 状態: 既定案（実装者はこの案に従う）

### D9: Phase の番号と意味索引

- 決定: 旧 Phase 3（所有権）を Phase 2 として実装し、旧 Phase 2（構文の文単位の回復）と `SemanticIndex` への回復部分の記録を Phase 3
  （設計方針）へ移す。
- 理由: 所有権の回復は型の回復だけで実装でき、構文の回復は parser の stack-depth 規則の設計が別に要る。回復した本体の型は `finish` を
  通らず未確定なので、hover の表示規則を G12 と決める必要がある。G12 D1 は索引なしでも動く。
- 状態: 既定案（実装者はこの案に従う）

### D10: 旧 API の先頭

- 決定: 旧 API は並べた集合の先頭を返す規則を保つ。回復で既存テストの先頭が変わる場合は期待値を変えず停止する。
- 理由: 期待値の変更は人間の判断を要する（GUIDE §13）。
- 状態: 既定案（実装者はこの案に従う）
