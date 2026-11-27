# NS06: 条件付きパターン束縛 if let（elif let 連鎖を含む安全な短絡取り出し）

| 項目 | 内容 |
| --- | --- |
| ID | NS06 |
| 優先度 | P2（単一ケースの `Maybe` / `Result` / 共用体の取り出しで毎回 3 行の `match` を書く摩擦をなくし、`Maybe.get` への逃避を防ぐ） |
| 規模 | M（`if let` / `elif let` の構文・AST・バイナリキャッシュ符号化・`tsuzuri fmt`・`match_value` への下げ・テストと文書） |
| 依存 | (NS04) |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（新しい制御構文の追加。予約語の追加はなし。決定事項 D1 が要承認、D2〜D6 は既定案） |
| 手本にする既存実装 | `src/parser.rs` の `conditional`・`conditional_else`（4969〜5025 行）、`src/parse_control.rs` のパターンと `when` ガード解析、`src/control.rs` の `match_value`、`src/formatter.rs` の `ExprKind::If` 整形 |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/control.rs`, `src/closures.rs`, `src/computation.rs`, `src/const_rules.rs`, `src/const_machine.rs`, `src/effects.rs`, `src/prove_smt.rs`, `tests/if_let.rs`（新規）, `tests/formatter.rs`, `docs/language.md`, `_tsuzuri/language-reference/loops-and-conditionals/if.md`, `_tsuzuri/language-reference/pattern-matching/pattern-matching.md`, `_tsuzuri/language-reference/built-in-types-and-modules/maybe.md` |

## 実装者向け早見

- 範囲: 既存の予約語 `if`・`let`・`when`・`then`・`elif`・`else` だけを使い、`if let <pattern> = <expr> [when <guard>] then <then_expr> [elif ... | else <else_expr>]`（および波括弧形式 `if let <pattern> = <expr> { ... } else { ... }`）を導入する。通常の `if <cond>` と `elif let ...` / `elif <cond>` を自由に連鎖できる。`else` 省略時は既存の `if` と同じく `unit` 型（`()`）になる。型検査・所有権・借用・ビュー束縛・生成 IR は 2 アームの `match <expr> with | <pattern> [when <guard>] -> <then_expr> | _ -> <else_expr>` と完全に同一にする。
- 最初に読む: ① 仕様 1〜3 ② `src/parser.rs` 4969〜5025 行（`conditional`・`conditional_else`）③ `src/control.rs` 818 行（`match_value`）④ `src/formatter.rs` の `If` の整形。
- 最初に書くテスト: `tests/if_let.rs` の `if_let_evaluates_and_moves_like_two_arm_match` と `elif_chains_mix_boolean_and_let_conditions`。
- 触ってはいけない: `if let` を書かない既存の `if` 式の型検査・範囲証明（`if i >= 0 && i < xs.length` の添字証明）・生成 IR（1 byte も変えない）。新しい予約語は足さない。
- 完了の見分け方:

```tsuzuri
def greet :: Maybe<string> -> string = \name ->
    if let Some text = name when text.length > 0 then
        $"Hello, {text}!"
    else
        "Hello!"
```

が edition 2026・2027 の両方で動作し、等価な `match` と同じ IR を出す。

## 目的

Tsuzuri には複数の `Maybe` を束ねる `Maybe { let! ... }` ビルダーがあるが、「1 つの `Maybe` が `Some` のときだけ処理を行う（`else` なし `unit`）」「条件分岐の途中で `elif let Some x = ...` を混ぜる」といった場面では毎回 `match ... with | Some x -> ... | None -> ()` と書く必要がある。この記述量の差が、人間や AI が安易に `Maybe.get` を使ってしまう大きな要因になっている。

既存の予約語だけで書ける `if let` / `elif let` を `match` の厳密な糖衣構文として追加し、暗黙の型変換やフロー型変更（暗黙ルール）を一切持ち込まずに Null 安全な分岐を簡潔に書けるようにする。

## 着手条件と停止条件

### 着手条件

- 決定事項 D1 の承認があること。
- `node scripts/repo-kb.mjs brief NS06` を読み、`FRONTEND_FORMAT`（`src/frontend_cache.rs`）と `grep -rn "ExprKind::If" src/` の全箇所を控える（GUIDE §6.9）。

### 停止条件

- **S1** `if let` を使わないプログラムの IR・診断・`tsuzuri fmt` の出力が 1 byte でも変わる。
- **S2** コンピュテーション式（`IO { if let Some x = ... then do! ... }` など）の内部で `if let` の脱糖が既存の `if` / `match` の規則と食い違う。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- `src/parser.rs` 4969 行の `conditional` は `if` の直後に `self.expression(0, false)` を呼ぶ。`let` は式の先頭に置けない文キーワードのため、現在 `if let Some x = m then ...` はすべての edition で構文エラー `E0002` になる（既存コードとの衝突はゼロ）。
- `conditional_else`（5000 行）は `elif` または同一行の `else if` に出会うと再帰的に `self.conditional()` を呼ぶため、`conditional` 側で `let` の分岐を扱えば `elif let` と `else if let` も同じ規則で自然に連鎖する。

## 変えてはいけない意味

- 通常の `if <bool_expr> then ... else ...` の型検査、範囲証明（`@no_trap` の添字免除）、コンピュテーション式内の `Zero` 補完、生成 IR。
- パターンマッチの所有権移動・借用ビュー束縛・`when` ガード中の読み取り専用ビュー規則（`docs/language.md`「match と分解」）。

## 仕様

### 1. 構文

```text
if_let := ("if" | "elif") "let" pattern "=" expr [ "when" guard_expr ] ( "then" body_expr | block ) [ else_part ]
```

- `if`（および `elif`、`else if`）の直後にキーワード `let` が続くとき、`if let` として解析する。
- `pattern` は `match` のアームと同じパターン構文（OR パターン `Some a | Some b`、匿名共用体の型テスト `:? string as s`、ネストパターンを含む）を受理する。ただしトップレベルの `|` がインデントされた次行のアームと曖昧にならないよう、`=` の前までを 1 つのパターンとして読む。
- `=` の右辺 `expr` は `when`・`then`・波括弧 `{`（ブロック形式の場合）の手前までの式として読む。
- 省略可能な `when <guard_expr>` は `match` アームのガードと同一で、`bool` 型を要求する。
- `else` 節が省略された場合、偽側の式は通常の `if` と同じく `()`（`unit` 型、コンピュテーション式内ではビルダーの `Zero ()`）になる。
- edition 2026 と 2027 の両方で同じ意味で受理する。

### 2. 意味・所有権・網羅性

`if let <pat> = <scrutinee> [when <guard>] then <then_branch> else <else_branch>` の評価順序、型検査、所有権移動、借用ビュー束縛、効果、定数評価は、次の `match` 式と**完全に同一**である。

```text
match <scrutinee> with
| <pat> [when <guard>] -> <then_branch>
| _ -> <else_branch>
```

- `<scrutinee>` は 1 回だけ評価される。
- `<pat>` で束縛された変数のスコープは `<guard>`（読み取り専用ビュー）と `<then_branch>` の内部だけで、`<else_branch>` や `if let` 式の外側からは見えない。
- `<pat>` が反駁不可能なパターン（例: `if let (a, b) = pair then ... else ...`）であった場合は、末尾の `| _ -> <else_branch>` が到達不能になるため、既存の到達不能警告 `W1003`（または専用メッセージ `irrefutable 'if let' pattern; the 'else' branch is unreachable`、D4）で知らせる（`else` を省略した `if let (a, b) = pair then ...` でも `let (a, b) = pair` を使うよう `W1003` で案内する）。

### 3. コンピュテーション式・定数関数・フォーマッタ

- **コンピュテーション式**: `Maybe { }`・`Result { }`・`IO { }` などの内部でも、`match` が書ける位置すべてで `if let` を書ける（`else` 省略時は `if` と同じくビルダーの `Zero` を使う）。
- **`const def`**: `const def` 本体はすでに `match` をサポートしているため、`if let` もそのまま使用可能とする。
- **`tsuzuri fmt`**: `if let` / `elif let` の構文を保持して整形する（`match` へ展開した形で出力しない）。

## 設計

- **AST（`src/syntax.rs`）**: `ExprKind::If` を壊さないよう、`ExprKind::IfLet { pattern: Box<Pattern>, value: Box<Expr>, guard: Option<Box<Expr>>, then_branch: Box<Expr>, else_branch: Box<Expr> }` を追加する（または `If` に `let_binding: Option<Box<IfLetHead>>` を持たせる。`ExprKind` のサイズを増やさないよう `Box` で包む。GUIDE-LITE §10）。`FRONTEND_FORMAT` を +1 し、`syntax_codec.rs` と `formatter.rs` を更新する。
- **下げ（`src/control.rs` と `src/computation.rs`）**:
  - 通常式では、`IfLet` を検査する際に 2 アーム `[MatchArm { pattern, guard, body: then_branch }, MatchArm { pattern: Wildcard, guard: None, body: else_branch }]` を組み立てて `match_value` に渡す。ただし自動生成した `Wildcard` アームが到達不能なとき（パターンが反駁不可能なとき）の位置が `else` 節（省略時は `pattern`）を指すように調整する。
  - コンピュテーション式（`src/computation.rs`）では、`else` 省略時の偽側を通常の `if` と同じくビルダーの `Zero` にした 2 アームの計算式 `match` として展開する。

## 前提とする他チケットのインターフェース

- 弱い依存 [NS04](_completed/NS04-irrefutable-binding-patterns.md): 反駁不可能な `if let` に対して「`let <pat> = ...` を使える」と案内する help 文。

## 他チケットへ提供するインターフェース

- `if let` / `elif let` 構文（NS04・NS05 の help メッセージからも案内可能）。

## 実装手順

1. **変更**: なし（ブランチ `feat/ns06-if-let`）。**内容**: `node scripts/repo-kb.mjs brief NS06`、`grep -rn "ExprKind::If" src/` の全箇所を洗い出し、`FRONTEND_FORMAT` を控える。**確認**: 一覧を記録。
2. **変更**: `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`。**内容**: `if let` / `elif let` / `else if let`（`then` 形式と波括弧形式、`when` ガード）の構文解析・キャッシュ符号化・整形。**確認**: T0、構文と `tsuzuri fmt` の単体テスト。
3. **変更**: `src/control.rs`, `src/closures.rs`, `src/computation.rs`, `src/const_rules.rs`, `src/const_machine.rs`, `src/effects.rs`, `src/prove_smt.rs`（`ExprKind` を走査する箇所）。**内容**: 2 アーム `match` と同等の型検査・所有権・計算式展開・定数評価。**確認**: `tests/if_let.rs`、既存 fixture の IR が byte 一致すること、T1。
4. **変更**: `tests/fixtures/` と `tests/features.mjs`、文書。**内容**: native / WASM `-O0`・`-O3` での実行と解放追跡（`live == 0`）、仕様書とリファレンスの更新。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。

## テスト計画

- `tests/if_let.rs`:
  - `Some` / `None`、`Ok` / `Error`、匿名共用体 `:? string as s`、`when` ガード、`else` 省略（`unit`）、`elif` と `elif let` の混合連鎖。
  - 非 Copy 値（`Maybe<string>`）の所有ムーブ、`ref Maybe<string>` からの読み取り専用ビュー束縛、ガード不成立時に `else` 節で元の変数を使えること。
  - コンピュテーション式 `IO { }` / `Result { }` 内の `if let`（`else` あり・なし）。
  - `const def` 内の `if let`。
  - 反駁不可能パターン `if let (a, b) = (1, 2) then a + b else 0` に対する `W1003` 警告。
  - `tsuzuri fmt` の冪等性。

## ドキュメント

- `docs/language.md`: 「if / then / else」「match と分解」。
- `_tsuzuri/language-reference/loops-and-conditionals/if.md`、`_tsuzuri/language-reference/pattern-matching/pattern-matching.md`、`_tsuzuri/language-reference/built-in-types-and-modules/maybe.md`。

## 受け入れ条件

- [ ] `if let` / `elif let` が通常関数・コンピュテーション式・`const def` で動作し、native / WASM `-O0`・`-O3` で `live == 0`。
- [ ] `tsuzuri fmt` が `if let` の構文を保つ。
- [ ] `if let` を使わない既存プログラムの IR・診断・整形出力が 1 byte も変わらない。
- [ ] 新しい予約語を追加していない。

## 落とし穴

- `if let Some x = f a then ...` の `=` の右辺にレコードリテラル `R { ... }` が来るとき、波括弧形式の `if let p = x { ... }` の本体ブロック開始 `{` と衝突しないよう、通常の `if` の条件式と同じ `allow_record` 規則（`parser.rs` の `conditional`）に従わせる。
- ガード `when` の中で非 Copy 束縛を消費しようとした場合は、`match` と同じく借用エラーになることを確かめる。

## 対象外

- `while let <pat> = <expr> do ...`（必要であれば別チケットまたは Phase 2 として検討するが、副作用のない式では無限ループになりやすいため本チケットの対象外とする）。
- `if let a = x && let b = y`（let chain。タプル `if let (Some a, Some b) = (x, y)` または `Maybe { let! ... }` で書けるため入れない）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 既存の予約語を用いた `if let <pat> = <expr> [when <guard>] then ... [elif ... \| else ...]` を全 edition に導入する | 予約語も暗黙変換も増やさず、単一ケースの安全な取り出しを簡潔にするため | 要承認 |
| D2 | 意味は 2 アームの `match` への脱糖と厳密に一致させる | 所有権・ビュー束縛・ガード・評価順序に第 2 の規則を作らないため | 既定案（実装者はこの案に従う） |
| D3 | AST では `IfLet`（または `If` の条件種別）として保持し、型検査で `match` に下げる | `tsuzuri fmt`・doc・AST キャッシュで元の構文を保つため | 既定案（実装者はこの案に従う） |
| D4 | 反駁不可能な `if let` パターンには既存の到達不能アーム警告 `W1003` を出す | 新しい警告コードを増やさず、`match` の到達不能検査の仕組みをそのまま再利用できるため | 既定案（実装者はこの案に従う） |
| D5 | `let chain`（`&&` 連結）や `?.` / `??` 演算子は導入しない | タプルパターンや `Maybe { let! ... }`・パイプラインで明示的に書け、優先順位や短絡借用の複雑化を避けるため（D-10） | 既定案（実装者はこの案に従う） |
| D6 | `while let` は本チケットに含めない | ループ条件のパターン束縛は `break` を持つ `while true do match ...` で明快に書け、まずは最も頻度の高い `if let` に絞るため | 既定案（実装者はこの案に従う） |
