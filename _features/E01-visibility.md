# E01: 可視性制御（private）
| 項目 | 内容 |
|---|---|
| ID | E01 |
| 優先度 | P0 |
| 規模 | S |
| 依存 | なし |
| 後続 | E02, G09 |
| 状態 | done |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/parse_control.rs`, `src/recursion.rs`, `src/llvm.rs`, `tests/modules.rs`, `tests/frontend.rs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

標準ライブラリと通常プロジェクトで、同じモジュール内だけから使う補助関数・補助型を明示できるようにする。

`export` はホスト ABI への公開指定であり、現状は通常の `fn`・`record` が全モジュールから参照可能である。

E02 以降の `std/` では補助関数を大量に置くため、公開 API と実装詳細を分けないと名前衝突・誤用・API 文書生成の対象が増える。

台帳 D-09 に従い、`private` は新規予約語で、違反と不正指定は `E1022` を使う。

このチケットでは `private` の構文・型検査・名前解決・診断・テストを実装し、実行時表現と公開 ABI は変えない。

## 現状

調査コミットは `19d8cdd`。

`src/syntax.rs` の `TokenKind` には `Private` がない。

`src/lexer.rs` の `Lexer::identifier` は `fn`, `def`, `export`, `record`, `class`, `instance` などを予約語化しているが、`private` は現在通常識別子になる。

`src/parser.rs` の `Parser::program` はトップレベルで `record`、`class`、`instance`、`let` 実装、`fn`/`def`/`export`/`and` を分岐する。

`src/parser.rs` の `Parser::program` は `export` を `def`/旧 `fn` の前だけで読む。

`src/parser.rs` の `Parser::program` は `record` 宣言を `RecordDecl { name, fields }` として追加し、可視性を保持しない。

`src/syntax.rs` の `RecordDecl`、`SignatureDecl`、`FunctionDecl` は可視性フィールドを持たない。

`src/check.rs` の `Names` は `records: BTreeMap<String, usize>`、`record_aliases: BTreeMap<String, Vec<String>>`、`functions: BTreeMap<String, usize>` を保持する。

`src/check.rs` の `Names::record` は「自モジュール → 完全修飾名 → 一意な無修飾別名 → 曖昧なら `E1004`」でレコードを解決する。

`src/check.rs` の `Checker::name` はローカル → 自モジュール関数 → 無修飾 builtin の順で値を解決する。

`src/check.rs` の `Checker::value_expression` は `ExprKind::QualifiedFunction` を `names.functions` から直接引く。

`src/check.rs` の `Checker::value_expression` は `ExprKind::Field(Name(module), field)` をモジュール関数として解決する。

`src/check.rs` の `Checker::value_expression` は `Task.run` / `Task.parallel` を `Builtin::ALL` から特別解決する。

`src/control.rs` のパターン検査は `PatternKind::Record` と active pattern を扱う。

`src/parse_control.rs` の `Parser::record_pattern` は `PatternKind::Record(Option<Ident>, fields)` を作る。

`src/polymorph.rs` の `Classes::find` と `Classes::resolve` は自モジュール名または完全名で型クラスを解決する。

`src/check.rs` の `check_modules` は全レコード・全関数の名前を先に収集し、宣言順に依存しない。

`src/check.rs` の `check_modules` は組み込み関数名をユーザー関数名として拒否する。

`src/check.rs` の `check_modules` は `export def` の型を `Type::exportable` と `unit` 結果に制限する。

`src/llvm.rs` の `emit_target`、`header`、`export_wrapper` は `CheckedFunction.exported` だけを見てホスト ABI を出力する。

`tests/modules.rs` は別モジュール関数の無修飾参照拒否、レコード無修飾解決、`tz_` export 名重複、`Main.tz` entry を検査している。

`docs/language.md` は現在「通常の `fn` も他モジュールから呼べる」「`export` はモジュール間可視性ではない」と説明している。

## 仕様

### 構文

EBNF 風の追加構文は次の通り。

```text
visibility       ::= "private"?
record-decl      ::= visibility "record" TypeName "{" fields "}"
function-sig     ::= visibility export? "def" rec-or-and? function-name "::" signature
union-decl       ::= visibility "union" ...        // A02 が追加する構文に先行して予約
type-alias-decl  ::= visibility "type" ...         // A05 が追加する構文に先行して予約
export           ::= "export"
rec-or-and       ::= "rec" | "and" | ε
```

`private` は `def`、`record`、将来の `union`、将来の `type` の直前だけに置ける。

`private export def` は `E1022`。

`export private def` も `E1022`。

`private fn` は `E1022`。実装側の `fn` は対応する `def` の可視性を継承する。

`private let name = x -> ...` は導入しない。トップレベル `let` 実装は対応する `def` の可視性を継承する。

旧互換構文 `export fn f(...) -> T { ... }` は従来通り公開 export として扱うが、`private export fn` は `E1022`。

`private class` はこのチケットでは導入しない。指定されたら `E1022`。

`private instance` は導入しない。指定されたら `E1022`。

インスタンスはグローバル coherence に参加するため、private にはできない。

active pattern は独立した `private` 構文を持たない。

active recognizer の元関数が private なら、その active pattern も同じモジュール内だけで使える。

`.tc` のビルダー操作も通常関数であり、可視性は `def` に付ける。

ただし `computation.rs` の `OPERATIONS` に含まれる `Bind`, `Return`, `ReturnFrom`, `Yield`, `YieldFrom`, `Zero`, `Combine`, `Delay`, `Run`, `For`, `While` は、ビルダー API なので private にしてはならない。

必要なビルダー操作を `private def Bind` のように宣言した場合は `E1022`。

補助関数を `private def helper :: ...` とすることは `.tc` でも許す。

相互再帰グループでは各 `def` / `def and` が個別に `private` を指定できる。

同じ `rec` グループ内で public と private を混在できる。

public 関数が private 関数を呼ぶことは同一モジュール内なら許す。

private 関数が public 型だけで構成されていても、他モジュールから参照できない。

### 型規則

private な関数・型・レコードは同じモジュール内からだけ参照できる。

同じモジュール内の参照は public と同じ型規則に従う。

他モジュールから private な関数・型・レコード・active pattern を参照した場合は `E1022`。

public 関数の引数型・返却型に同一モジュールの private 型が含まれる場合は `E1022`。

public レコードのフィールド型に同一モジュールの private 型が含まれる場合は `E1022`。

将来の public union case payload と public type alias の右辺に private 型が含まれる場合も `E1022`。

private 関数・private レコード・private union・private type alias の署名やフィールドには private 型を使える。

`export def` は常に public なので、private 型の leak は `private export` より前に構文上拒否されてもよいが、診断コードはどちらも `E1022`。

型クラス制約に private 型が出る場合も、その制約を持つ関数が public なら `E1022`。

型クラス自体はこのチケットで private にしない。

インスタンス宣言の対象型が private 型でも、宣言自体は許す。

その private 型が他モジュールから書けないため、インスタンスの利用も外部 API としては漏れない。

### 名前解決

関数の可視性解決は修飾参照だけに影響する。

他モジュール関数は従来通り修飾必須であり、private はその修飾参照を拒否する。

レコード・型名の無修飾解決は次の順にする。

1. 自モジュールの同名宣言。private でも可。
2. 利用者モジュール内で一意な public 宣言。
3. E02 後は標準ライブラリ内で一意な public 宣言。
4. visible 候補がなく private 候補だけが見つかる場合は `E1022`。
5. visible 候補が複数ある場合は `E1004`。
6. 候補がまったくなければ従来通り `E1004`。

private 候補は他モジュールの曖昧性を増やさない。

`Point.Point` のような修飾レコード名が private なら、他モジュールからは `E1022`。

`Module.fn` が private なら、他モジュールからは `E1022`。

`ExprKind::QualifiedFunction` の完全名が private なら、他モジュールからは `E1022`。

ローカル変数は従来通り最優先であり、同名モジュールを隠す。

`name.field` はローカル値がある場合フィールドアクセスを優先し、private module function とは解釈しない。

active pattern は、参照元モジュールと recognizer の宣言モジュールが異なる場合に可視性を検査する。

型クラスメソッド解決は `Classes::find` / `Classes::resolve` の範囲では private class を扱わない。

E02 の修飾 builtin は source function 優先なので、private source function が同じモジュール内から builtin を隠すことは従来の名前解決と同じく許す。

### 評価順序・所有権・ABI

`private` はコンパイル時の名前解決だけを変える。

評価順序、短絡、トラップ、move、borrow、Copy 判定は変えない。

private 関数の IR シンボルは従来と同じ内部 `@tz.fn.Module.name` でよい。

public だが非 export の関数も従来通り `define internal` でよい。

`export def` の `tz_` 公開 ABI は互換維持し、private とは独立に扱う。

private はホストヘッダーには一切出ない。これは `export` 以外がヘッダーに出ない現状と同じ。

### 診断

`E1022` を新設用途どおり使う。

メッセージは英語、小文字始まり、修正方法を含める。

代表メッセージは次の通り。

```text
private name 'Secret.value' is only visible inside module 'Secret'; expose a public wrapper or move the use into the same module
private type 'Secret.Token' leaks from public function 'Secret.make'; make the function private or expose a public type
'private export' is not allowed; remove 'private' or remove 'export'
builder operation 'Bind' cannot be private; make the operation public and keep helpers private
```

Span は参照した名前、または不正な `private` / `export` の位置に置く。

private leak は public 宣言名の Span ではなく、leak した型参照の Span を優先する。

`resolve_type` は `Type` へ変換すると入れ子の `TypeExpr` span を失うため、private leak 検査は解決済み `Type` だけを後から走査してはいけない。

public signature / public record field / public class method signature の元 `TypeExpr` を再帰的に歩き、各 `TypeExprKind::Named` / 将来の `Apply` / `Constrained` の名前をその場で解決し、private named type を見つけた span で `E1022` を出す。

型置換後に private leak が初めて判明する形はこのチケットでは導入しないが、A01/A02/A05 は型適用・union・alias の元構文 span を同じ walker に渡す。

### 前提とする他チケットのインターフェース

このチケットは他チケットに依存しない。

A02 が `UnionDecl` を追加するときは、E01 の `Visibility` を `UnionDecl` に持たせる。

A05 が `TypeAliasDecl` を追加するときは、E01 の `Visibility` を `TypeAliasDecl` に持たせる。

E02 は `std/` の補助関数・補助型を private にする前提でこのチケットを利用する。

G09 は API 文書生成時に `Visibility::Private` を既定で除外する。

### 他チケットへの提供インターフェース

`src/syntax.rs` に次を追加する。

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}
```

`RecordDecl` に `pub visibility: Visibility` を追加する。

`SignatureDecl` に `pub visibility: Visibility` を追加する。

`FunctionDecl` に `pub visibility: Visibility` を追加する。

`CheckedRecord` に `pub visibility: Visibility` を追加する。

`CheckedFunction` に `pub visibility: Visibility` を追加する。

`FunctionDecl.exported == true` の場合は `visibility == Visibility::Public` を不変条件にする。

A02/A05 は同じ `Visibility` 型を使い、新しい可視性 enum を作らない。

`check.rs` 内部の `Names` は raw id ではなく次の形の情報を保持する。

```rust
#[derive(Clone, Debug)]
struct NameInfo {
    id: usize,
    module: String,
    visibility: Visibility,
    span: Span,
}
```

必要なら `RecordInfo` と `FunctionInfo` に分けてよいが、可視性判定の規則は同じにする。

`Names::record(&self, module: &str, name: &str, span: Span) -> Result<usize, Diagnostic>` の公開される動作は維持する。

内部では `visible_from(requester)` を通して `E1022` を返す。

`Names::function(&self, requester: &str, qualified: &str, span: Span) -> Result<usize, Diagnostic>` を追加し、`Checker::value_expression` と `Checker::name` から直接 map を引かない。

`Names::active_pattern(&self, requester: &str, qualified: &str, span: Span) -> Result<(usize, bool), Diagnostic>` を追加し、`control.rs` から使う。

## 設計

### AST と字句

`TokenKind::Private` を追加する。

`Lexer::identifier` の予約語表に `"private" => TokenKind::Private` を追加する。

`docs/language.md` の予約語説明に `private` を追加する。

キーワード追加により、ファイル名 `private.tz` とモジュール名 `private` は `E1011` になる。

識別子 `let private = 1` は構文エラーになる。

### Parser

`Parser::program` のループ先頭で `let private = self.eat(&TokenKind::Private);` を読む。

`private` の直後が `Record`、`Def`、将来の `Union`、将来の `Type` でない場合は `E1022` を返す。

現時点で `private class` / `private instance` / `private fn` / `private let` が来た場合は `E1022`。

`export` と `private` の順序はどちらでも `E1022` だが、推奨構文は `private def` だけにする。

`export` を読んだ後に `private` が来た場合も `E1022`。

`private` を読んだ後に `export` が来た場合も `E1022`。

`record` 宣言では `RecordDecl { visibility, name, fields }` を作る。

`def` 宣言では `SignatureDecl { visibility, exported, ... }` を作る。

旧 inline `fn` 構文は visibility を `Public` 固定にする。

`fn` 実装は signatures map から visibility を引き継いで `FunctionDecl` に入る。

`let` 実装も signatures map から visibility を引き継ぐ。

`SignatureDecl` を `definitions` と結合する `Self::define` に visibility を渡す。

### 名前収集

`check_modules` の最初のモジュール名検査は、`private` がキーワード化されることでそのまま `E1011` を返す。

レコード収集時に `NameInfo { id, module, visibility, span }` を `names.records` に入れる。

`record_aliases` は `BTreeMap<String, Vec<NameInfo>>` または key の Vec ではなく id Vec にし、可視性判定できるようにする。

関数収集時に `NameInfo` を `names.functions` に入れる。

同一モジュール内の重複判定は public/private に関係なく従来通り `E1001`。

別モジュールの同名関数は従来通り許す。

別モジュールの同名レコードは従来通り許すが、無修飾解決では visible な候補だけで曖昧性を判定する。

`Builtin::ALL` との重複拒否は public/private に関係なく維持する。

### public API leak 検査

`check_modules` でレコード field 型と関数 signature 型を解決した直後に leak 検査を行う。

`fn validate_public_type_expr_visibility(expression: &TypeExpr, requester: &str, owner: &str, names: &Names) -> Result<(), Diagnostic>` を追加する。

この関数は `resolve_type` と同じ名前解決を呼ぶが、解決結果の `Type` だけでなく、private と判定した named type の `TypeExpr.span` を保持して `E1022` を作る。

E01 時点の private named type は `Type::Record(id)` だけでよい。

A02/A05 は `Type::Union` / alias 展開後の型を同じ関数に追加する。

public record の各 field `TypeExpr` で private named type を見つけたら `E1022`。

public function の parameter/result/constraints の元 `TypeExpr` で private named type を見つけたら `E1022`。

private function と private record では leak 検査をしない。

`exported` な関数は public と同じ leak 検査を受ける。

`resolve_type` 自体は従来通り `Type` を返してよいが、private leak 用の診断 span を作る責務を持たせない。

### 関数解決

`Checker::name` の自モジュール関数参照は常に visible なので、`Names::function(self.module, ...)` を使っても成功する。

`ExprKind::QualifiedFunction` は `Names::function(self.module, &name.text, name.span)` を使う。

`ExprKind::Field(Name(module), field)` の module function 分岐は `Names::function(self.module, &qualified, field.span)` を使う。

`module_function` 判定は `names.functions.contains_key` ではなく、`Names::has_visible_function_or_private_candidate` のような helper に置き換える。

private 候補がある場合はフィールドアクセスに落とさず `E1022` を返す。

ローカル値が同名にある場合は従来通りフィールドアクセスへ進む。

### レコード・パターン・型解決

`resolve_type` は `Names::record` を通るため、private 型参照を一箇所で拒否できる。

`ExprKind::Record` のリテラル解決も `Names::record` を通るため、private レコード構築を一箇所で拒否できる。

`PatternKind::Record` の型検査も `Names::record` を通るよう、既存コードの直接 map 参照があれば helper に寄せる。

`record_aliases` の候補表示は public 候補だけを出す。

private 候補しかない場合は「unknown」ではなく `E1022` にする。

### Active patterns

`check_modules` で active pattern を `names.active_patterns` に入れる際、元の function visibility を保存する。

`control.rs` の active pattern 解決は `Names::active_pattern` を使う。

同じモジュール内なら private recognizer を pattern として使える。

他モジュールなら `E1022`。

### Computation builders

`computation::collect` は `.tc` の `program.functions` を見る。

`program.functions` のうち `function.name.text` が `OPERATIONS` に含まれ、`visibility == Private` なら `E1022`。

private helper は `methods` 集合に入ってもよいが、ビルダー operation の存在判定には public operation だけを使う。

`Builder { ... }` の展開で呼ばれる operation が private になる経路をなくす。

### LLVM / ABI

`emit_target` は private/public を見なくてよい。

IR 内部シンボルは従来通り全 CheckedFunction を emit する。

E02 の std 到達可能性 pruning は `CheckedFunction.visibility` と `origin` を別々に扱う。

`header` と `export_wrapper` は `exported` だけを見る。

`private export` が Parser/checker で拒否されるため、LLVM 側に防御的分岐は不要。

## 実装手順

1. `src/syntax.rs` に `Visibility` と `TokenKind::Private` を追加する。
   - 確認: `cargo test --locked --lib lexer` と `cargo test --locked --test frontend` を実行し（N > 0 を確認）、`private` が識別子でなくなることを確認する。

2. `src/lexer.rs` の `Lexer::identifier` に `"private"` を追加する。
   - 確認: `private` を含む最小ソースを `parser::parse` し、予約語になったため旧識別子利用が構文エラーになることを Rust テストで確認する。

3. `RecordDecl`、`SignatureDecl`、`FunctionDecl`、`CheckedRecord`、`CheckedFunction` に `visibility: Visibility` を追加する。
   - 確認: コンパイルエラーになった構築箇所をすべて修正し、`Visibility::Public` の既定で既存テストの期待 IR が変わらないことを確認する。

4. `Parser::program` に `private` の読み取りと不正指定 `E1022` を追加する。
   - 確認: `private record R {}`、`private def f :: i64` を受理し、`private fn f() -> i64 { 1 }`、`private export def f :: i64`、`export private def f :: i64` を `E1022` で拒否する。

5. `Parser::program` の signatures/definitions 結合で visibility を保持する。
   - 確認: `private def f :: i64\nfn f = 1` と `private def f :: i64\nlet f = fx () -> 1` の `CheckedFunction.visibility` が `Private` になる。

6. `Names` を `NameInfo` ベースに変更する。
   - 確認: `tests/modules.rs` の既存ケース、特に同名レコード曖昧性と別モジュール同名関数が従来通り動く。

7. `Names::record` と新規 `Names::function` に可視性判定を実装する。
   - 確認: 他モジュールから `Secret.Token`、`Secret.make`、無修飾 `Token` を参照したとき `E1022` になる。

8. `Checker::name`、`Checker::value_expression` の `ExprKind::QualifiedFunction`、`ExprKind::Field` module branch を helper に差し替える。
   - 確認: ローカル `let Secret = ...; Secret.value` はフィールドアクセスとして残り、private module function エラーに変わらない。

9. record literal、record pattern、type annotation の private 参照がすべて `E1022` になることを確認する。
   - 確認: `fn f(x: Secret.Token) -> i64`、`Secret.Token { ... }`、`match x with | Secret.Token { ... } -> ...` を拒否する。

10. public signature leak 検査を元 `TypeExpr` walker として追加する。
    - 確認: 同一モジュール内の `private record Token { value: i64 }\ndef make :: Token\nfn make = ...` は `Token` の span で `E1022`、`private def make :: Token` は受理。

11. active pattern の visibility 継承を実装する。
    - 確認: private recognizer を同じモジュールの `match` で使える。他モジュールから `Secret.Pattern` を使うと `E1022`。

12. `.tc` builder operation の private 禁止を `computation::collect` に追加する。
    - 確認: `private def Bind` は `E1022`、`private def helper` は受理。

13. docs と README を更新する。
    - 確認: `docs/language.md` の予約語・モジュール可視性・診断表に `private` / `E1022` が載る。

14. 最小 E2E を追加する。
    - 確認: native/WASM の `-O0`/`-O3` で private helper を使う public export が従来通り動く。

## テスト計画

### Rust 受理テスト

`tests/modules.rs` または新規 `tests/visibility.rs` に追加する。

`private record Token { value: i64 }` を同じモジュール内の private 関数で構築できる。

private 関数を同じモジュールの public 関数から呼べる。

private 関数を `fn` 実装と `let` lambda 実装の両方で使える。

private レコードを同じモジュール内の record pattern で分解できる。

private active recognizer を同じモジュール内の match で使える。

private helper を `.tc` に置き、public `Return` から呼べる。

public と private が混在する `def rec` / `def and` グループを型検査できる。

`export def api :: i64\nfn api = private_helper()` が `tz_api` だけを公開し、private helper は header に出ない。

### Rust 拒否テスト

`private` を識別子として使う `let private = 1` は `E0002`。

ファイル名 `private.tz` を `analyze_modules(&[("private", "...")])` で使うと `E1011`。

`private fn f() -> i64 { 1 }` は `E1022`。

`private export def f :: i64` は `E1022`。

`export private def f :: i64` は `E1022`。

`private class C<'a> { ... }` は `E1022`。

`private instance Add<T> { ... }` は `E1022`。

他モジュールから `Secret.hidden()` を呼ぶと `E1022`。

他モジュールから `let f = Secret.hidden` と関数値を取ると `E1022`。

他モジュールから `Secret.Token { value: 1 }` を構築すると `E1022`。

他モジュールの型注釈 `let x: Secret.Token = ...` は `E1022`。

他モジュールの public signature `def f :: Secret.Token -> i64` は `E1022`。

public 関数が同一モジュール private 型を返すと `E1022`。

public record が private field 型を含むと `E1022`。

private 型と同名の public 型が別モジュールにある場合、無修飾名は public 候補へ解決し、private 候補で `E1004` を増やさない。

visible 候補が複数ある場合は従来通り `E1004`。

`.tc` の `private def Bind` は `E1022`。

### 診断 Span テスト

外部参照 `Secret.hidden()` は `hidden` の span。

外部型 `Secret.Token` は `Secret.Token` の `TypeExpr` span。

private leak は public 宣言名ではなく、入れ子も含めて leak した private named-type の具体的な `TypeExpr` span。

例: `def f :: [Secret.Token] -> i64` は `Secret.Token` の span、`def f :: Wrapper Secret.Token -> i64` は A01 後も `Secret.Token` の span。

`private export` は `private` から `export` までの span。

### LLVM / header テスト

private 関数は `@tz.fn.Module.helper` として IR に出てもよい。

`llvm::header` は `exported` 関数だけを出すため、private helper は出ない。

`private` を追加しない既存ソースの IR は決定的で、既存の `tests/modules.rs` 期待を変えない。

### E2E

fixture `tests/fixtures/visibility/Main.tz` を追加する。

public export `tz_answer` が private helper と private record を使って `42` を返す。

C host は `tz_answer()` だけを呼ぶ。

WASM は `WebAssembly.Module.imports(module)` が空で、`tz_answer()` が `42n` または `42` を返す。

native/WASM × `-O0`/`-O3` を実行する。

所有値を含む private helper の呼び出し後、既存の heap tracking が可能な fixture では `live == 0` を確認する。

## ドキュメント

`docs/language.md` の「ソースと宣言」に `private` 予約語を追加する。

`docs/language.md` の「1 ファイル = 1 モジュール」に、既定 public と `private` の意味を追加する。

`docs/language.md` の「公開 ABI」に、`export` と `private` は別概念であり `private export` は不正と追記する。

`docs/language.md` の「診断」表に `E1022` を追加する。

`docs/architecture.md` の「モジュール」不変条件に、private は型検査時の名前解決で完結し IR ABI を変えないと追記する。

`README.md` のファイルとモジュール説明に短い例を追加する。

E02 の標準ライブラリ説明では、補助関数を private にできることを前提として書く。

## 受け入れ条件

- [x] `private` が予約語として実装され、モジュール名にも識別子にも使えない。
- [x] `private def` と `private record` が同一モジュール内で使える。
- [x] 他モジュールから private 関数・レコード・型・active pattern を参照すると `E1022`。
- [x] `private export`、`private fn`、`private class`、`private instance` が `E1022`。
- [x] public signature と public record field から private 型が漏れると `E1022`。
- [x] private helper を持つ `.tc` が動き、operation 自体の private は `E1022`。
- [x] 既存の public-only コードの IR と ABI が変わらない。
- [x] `tz_` 公開名の互換性が保たれる。
- [x] `docs/language.md`、`docs/architecture.md`、`README.md` が更新される。
- [x] 関連 Rust テスト、E2E native/WASM × `-O0`/`-O3` が通る。

## 落とし穴

`private` は `export` の反対ではない。`export` はホスト ABI、private はモジュール間可視性である。

private 候補を無修飾レコードの曖昧性に含めると、外部から見えない型で `E1004` が発生する。

`Names::record` だけ直しても、`ExprKind::QualifiedFunction` と `ExprKind::Field` の直接 map 参照が漏れる。

同一モジュール内 public 関数が private 型を返す leak は、通常の visibility check だけでは検出できない。

`fn` 実装に visibility を重複指定させると、`def` と `fn` の不一致ケースが増えるため導入しない。

`.tc` の operation を private に許すと、別モジュールの `Builder {}` 展開が private 関数呼び出しになって壊れる。

active pattern の visibility は元関数から継承しないと、private recognizer が pattern 経由で漏れる。

`private` keyword 追加により、既存テストで `private` を識別子にしていた場合は期待を `E0002` に更新する。

## 対象外

`private class` は対象外。

`private instance` は対象外。

package 内可視性、friend module、`internal`、`protected` は対象外。

公開 API 文書生成は G09 で扱う。

未使用 private の警告は G03 の `W1002` で扱う。

union の実装は A02、type alias の実装は A05 で扱う。

標準ライブラリ同梱と std reserved modules は E02 で扱う。

IR から private 関数を削除する最適化は E02 の std pruning 以外では行わない。

## 未決事項

private な型クラスを将来許すかは未決。既定案は許さず、必要になったら A06 で capability と coherence を設計する。

private な instance を将来許すかは未決。既定案は許さない。インスタンス探索の順序と coherence を壊さないため。

public signature の private leak を「同じモジュール内なら許し、外部使用時だけ拒否」にする案は却下する。既定案は宣言時 `E1022`。

private 候補しかない無修飾型名を `unknown record type` にする案は却下する。既定案は情報のある `E1022`。

台帳の見直し提案はない。D-09 と D-16 に従う。

### 実装時の判断（E01）

- `NameInfo` に可視性と所属モジュールを集約し、修飾／無修飾の型名・関数名・認識器の解決に同じ規則を使う。
  private 候補は曖昧性に数えず、private 候補しかない場合は `E1022` とする。
- public 宣言からの private 型の漏れは元の TypeExpr を検査し、宣言順によらず漏れた参照位置で報告する。
  インスタンスメソッドはクラスの dispatch 経由でのみ呼ぶため、private 型のインスタンスを公開 API の漏れとは扱わない。
- `fn`／`let`／`and` は def の可視性を継承し、`.tc` の操作は public のまま、補助だけを private にできる。
  `export` のホスト ABI と LLVM のシンボル・通常の関数出力は変更しない。
- union の可視性は A02 の同じ NameInfo 経路へ統合し、case は union の可視性を継承する。
