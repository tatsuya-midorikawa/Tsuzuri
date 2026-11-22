# NS04: ラムダ式引数・for...in の反駁可能パターンの静的検査と、let の反駁不可能パターン分解

| 項目 | 内容 |
| --- | --- |
| ID | NS04 |
| 優先度 | P1（`\(Some x) -> x` や `for Some x in items` が無警告でコンパイルを通り、`None` でキャッチ不能な実行時トラップになる抜け穴を塞ぐ） |
| 規模 | M（Phase 1: 既存の束縛分解の反駁不可能性検査と edition 分岐、Phase 2: `let` / `let mut` の反駁不可能パターン分解） |
| 依存 | – |
| 後続 | ([NS06](NS06-if-let-conditional-binding.md)) |
| 状態 | todo |
| 起票 | 2026-11-22（[GUIDE D-64](GUIDE.md#d-64-null-安全性の徹底ns01ns08)。Null 安全性の調査報告に対する改善チケット作成依頼から起票。HEAD `edc61676` で確認） |
| 承認 | 要承認（言語仕様のパターン検査規則の変更と `W1012` の割り当て（D-55）。決定事項 D1・D2・D3 が要承認、D4〜D6 は既定案） |
| 手本にする既存実装 | `src/control.rs` の `match_value`（818 行）と `for...in` の分解（323 行）、`src/syntax.rs` の `MatchOrigin`（1425 行）、`src/exhaustiveness.rs` の `NonExhaustiveMatch`（315 行）、`src/closures.rs` のラムダ引数展開（324 行）、[A22](_completed/A22-edition-2027-canonical-forms.md) の edition による警告／エラー切り替え |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/control.rs`, `src/closures.rs`, `src/computation.rs`, `src/select.rs`, `src/exhaustiveness.rs`, `src/diagnostic_kinds.rs`, `src/explain/codes/`, `tests/match_exhaustiveness.rs`, `tests/trap_locations.rs`, `tests/prove.rs`, `docs/language.md`, `_tsuzuri/language-reference/pattern-matching/pattern-matching.md`, `_tsuzuri/language-reference/loops-and-conditionals/for-in.md`, `_tsuzuri/language-reference/values-and-functions/lambda-expressions.md`, `_tsuzuri/language-reference/values-and-functions/values.md`, `_tsuzuri/language-reference/compiler/diagnostics.md` |

## 実装者向け早見

- 範囲:
  - **Phase 1**: 単一パターンしか書けない分解箇所（ラムダ式の引数、通常の `for pat in expr do`、コンピュテーション式の `let! pat =`・`and! pat =`・`for pat in`、`select` の `recv pat from ch`）に `src/exhaustiveness.rs` の網羅性検査を適用する。`Some x`・`[a, b]`・`0`・`when` ガード付き引数などの反駁可能パターン（不足ケースがあるもの）を、**edition 2027 では `E1021`（kind `refutable_binding_pattern`）で拒否し、edition 2026 では互換性のため警告 `W1012`（同じメッセージと help）で報告する**。
  - **Phase 2**: 通常の `let pat [: Type] = expr`（および `let mut (a, b) = ...` ではなく不変 `let`、D3 参照）で、タプル・レコード・1 ケース union などの**反駁不可能なパターン分解**を許す（反駁可能な `let Some x = m` は Phase 1 と同じ `refutable_binding_pattern` で拒否し、`match` または [NS06](NS06-if-let-conditional-binding.md) の `if let` を案内する）。
- 最初に読む: ① 仕様 1〜3 ② `src/syntax.rs` 1425〜1442 行（`MatchOrigin`）③ `src/control.rs` 305〜345 行（`for...in`）・815〜845 行（`match_value`）④ `src/exhaustiveness.rs` 290〜345 行。
- 最初に書くテスト: `tests/match_exhaustiveness.rs` の `refutable_lambda_and_for_patterns_are_rejected_in_2027` と `edition_2026_warns_on_refutable_binding_patterns`。
- 触ってはいけない: `match` 式と関数ガード（`\x -> | Some v -> ... | None -> ...`）の `E1021`・`W1003` の文言と動作、反駁不可能なパターン（`\(x, y) -> ...`、`for (k, v) in items`）の生成 IR（1 byte も変えない）。
- 完了の見分け方: edition 2027 で `[Some 1, None] |> Array.map (\(Some x) -> x)` が `E1021`（`pattern in a lambda parameter is refutable; missing: None`）になり、`let (a, b) = (20, 22)`（Phase 2）が `42` を返す。

## 目的

現行仕様（`docs/language.md` 2856 行、`_tsuzuri/language-reference/pattern-matching/pattern-matching.md` 160〜170 行）では、`match` 式と関数ガードは網羅性検査（`E1021`）を強制する一方、ラムダ式の引数・`for...in`・コンピュテーション式の分解（`MatchOrigin::LambdaDestructuring`・`ComputationDestructuring`・`for...in` の `None` coverage）は網羅性検査を一切行わず、不一致時に `TrapKind::PatternMismatch` で実行時トラップする。

その結果、`\(Some x) -> x` や `for Some x in xs do ...` や `Maybe { let! (Some x) = ... }` といった「`None` で即クラッシュする暗黙の強制アンラップ」が警告すら出ずにコンパイルを通ってしまう。Rust（`E0005`）と同様に単一束縛箇所には反駁不可能パターンのみを許すことで、値の不在の見落としをコンパイル時に確実に捕捉する。あわせて Phase 2 で反駁不可能な `let (a, b) = pair` を解禁し、タプルやレコードの分解のためにダミーの `match` やラムダを書く手間をなくす。

## 着手条件と停止条件

### 着手条件

- Phase 1: 決定事項 D1・D2 の承認があること。`node scripts/repo-kb.mjs brief NS04` を読む。
- Phase 2: Phase 1 が `done` であり、決定事項 D3 の承認があること。
- 着手時に `FRONTEND_FORMAT`（`src/frontend_cache.rs`）と `DiagnosticKind` の個数を控える（GUIDE-LITE §11）。

### 停止条件

- **S1** `std/` または `examples/`、`guide/`、`_tsuzuri/language-reference/` の検証済みコードに反駁可能なラムダ引数や `for...in` パターンが存在し、修正方針が決まらない（着手時の手順 1 で全件走査する）。
- **S2** 反駁不可能なパターン（変数、`_`、タプル、レコード、単一ケース union）だけを使う既存プログラムの生成 IR が変わる。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `edc61676` で確認）

- **`MatchOrigin`（`src/syntax.rs` 1425〜1441 行）**: `Explicit`・`FunctionGuard`・`LambdaDestructuring`・`ComputationDestructuring` の 4 つがあり、`checks_coverage` は `Explicit | FunctionGuard` のみ `true` を返す。
- **通常の `for...in`（`src/control.rs` 323〜334 行）**: 単一識別子でも `_` でもないパターンのとき、`self.match_value(..., None)` を直接呼んでおり、`MatchOrigin` すら渡していない。
- **`select` の `recv pat from ch`（`src/select.rs` 199・214 行）**: チャネルから受信した要素の分解に `ComputationDestructuring` / `match_value(..., None)` を使っている。
- **`try ... with`（`src/exceptions.rs` 164 行）**: 例外ハンドラの `| e is OverflowException ->` はキャッチ対象外の例外を再送出するための分岐であり、ここが `None` なのは正しい（変えてはならない）。
- **既存テストで `PatternMismatch` を使っている箇所（全件 `grep` 済み）**:
  1. `tests/trap_locations.rs` 325 行: `export def pattern_failure :: unit` の `for [only] in [[1, 2]] do assert (only == 0)`（マニフェストなしのため既定で edition 2027。`Tsuzuri.toml` で `edition = "2026"` を指定すれば `W1012` のもとで既存の実行時トラップ検証をそのまま維持できる）。
  2. `tests/prove.rs` 144 行: `def positive :: i64 -> i64 = \x when x > 0 -> x`（`project("guard", ...)` の edition を確認し、2026 プロジェクトとして `PatternMismatch` の証明テストを保つ）。
- **通常の `let`（`pattern-matching.md` 95 行）**: `let` の左辺は識別子のみを受理し、`let (x, y) = pair` は `E0002`（`expected an identifier`）になる。

## 変えてはいけない意味

- `match` 式と関数ガード（`\x -> | p1 -> ... | p2 -> ...`）の検査・診断・Quick Fix。
- `try ... with` の例外フィルタの意味。
- 反駁不可能なパターン（タプル `(a, b)`、全フィールドまたは部分フィールドのレコード `{ x = a }`、単一ケース union `Wrap x`、全ケース網羅の OR パターン `(true | false)`、型注釈 `(x: i64)`、`as`）の所有権・ビュー束縛・生成 IR（防御用の失敗トラップブロックの維持を含む。D-10 の多重防御）。

## 仕様

### Phase 1: 束縛分解位置での反駁不可能性検査

1. **対象位置と場所の表示名**:
   - ラムダ式の引数（`\(Some x) -> ...`、`\x when x > 0 -> ...`）: `"a lambda parameter"`
   - 通常の `for pat in expr do`: `"a 'for' binding"`
   - コンピュテーション式の `let! pat =`・`and! pat =`・`for pat in`: `"a computation binding"`
   - `select` の `recv pat from ch`: `"a 'select' receive binding"`
2. **判定規則**:
   - 単一アーム（ラムダ引数の `when` ガードがあれば `guarded: true`）に対して `exhaustiveness` の検索を走らせ、未網羅の witness `missing` が見つかれば反駁可能と判定する。
   - 到達不能アーム警告 `W1003` は単一アームの束縛位置では出さない（ガード付き単一アームは到達可能であり、OR パターン内部は `match` と同じ規則）。
3. **診断（edition 分岐）**:
   - **edition 2027**: エラー `E1021`、kind `refutable_binding_pattern`。
     - メッセージ: `pattern in <場所> is refutable; missing: <witness>`
     - 位置: そのパターンの `Span`。
     - help:
       - ラムダ引数: `use function guards ('\x -> | <pattern> -> ... | _ -> ...') or 'match' inside the body`
       - `for...in`: `bind the element with a name and 'match' on it in the loop body`
       - その他: `bind the value with a name and 'match' on it`
   - **edition 2026**: 警告 `W1012`、kind `refutable_binding_pattern_warning`（同じメッセージと help）。生成コードは従来どおり不一致時に `PatternMismatch` でトラップする。

### Phase 2: `let` の反駁不可能パターン分解

1. **構文**:
   - モジュールトップレベル、ブロック内、`fn` / ラムダ本体、コンピュテーション式内の通常の不変 `let` で、`let <pattern> [: <Type>] = <expr>` を受理する（edition 2026・2027 の両方。従来 `E0002` だった構文の解禁なので既存コードは壊れない）。
   - 例: `let (left, right) = pair`、`let Point { x, y } = origin`、`let (first, _): (i64 * string) = item`。
   - `let mut` は単一識別子のままとする（`let mut (a, b) = ...` は `E0002` のまま。複数変数の一括可変束縛や部分再代入の誤解を防ぐため。D3）。
   - 関数実装の `let name args = body`（先行する `def name :: ...` に対する実装）は従来どおり先頭が識別子のときだけ関数実装として読む。
2. **型検査・所有権・網羅性**:
   - 型注釈 `: Type` があれば右辺の期待型とし、なければ右辺を推論した型に対してパターンを照合する（`let (a, b): (i64 * i64) = ...` とパターンの内側の注釈 `(a: i64, b)` の両方が書ける）。
   - 反駁不可能性を常に `E1021`（`refutable_binding_pattern`、場所名 `"a 'let' binding"`）で検査する（`let Some x = m` は全 edition で `E1021`。以前は `E0002` だった構文のため 2026 でも警告にする必要がない）。help は `use 'match' (or 'if let' once available) to handle 'None'`（NS06 実装後は `'if let'` を直接案内）。
   - 所有権・部分 move・Copy の最終使用 move（A17）は、後続の式を本体とする 1 アームの `Match`（`MatchOrigin::LetDestructuring`）と同じ規則で動作する。単一識別子 `let x = expr` とワイルドカード `let _ = expr` は既存の `Let` AST のままにし、既存の最適化（スタックリテラル配置・確保省略・`const_static` など）を 1 byte も変えない。

## 設計

- **`MatchOrigin` の拡張（`src/syntax.rs`）**: `ForDestructuring`、`SelectDestructuring`、（Phase 2 で）`LetDestructuring` を追加し、`MatchOrigin::binding_place_name(self) -> Option<&'static str>` を持たせる。`syntax_codec.rs` の `MatchOrigin` 符号化を更新し、`FRONTEND_FORMAT` を +1 する。
- **エディション情報の受け渡し**: `Checker` はソースごとの `Edition` を持っている（またはモジュール単位で保持する）ため、`MatchCoverage` に `origin: MatchOrigin` と `edition: Edition` を持たせ、`exhaustiveness.rs` が `origin.binding_place_name()` の有無と `edition` から `E1021`（`Explicit`/`FunctionGuard`、または 2027 の束縛分解・全 edition の `LetDestructuring`）と `W1012`（2026 の既存束縛分解）を切り替える。
- **Phase 2 の構文解析（`src/parser.rs`）**: `let` の直後が `mut` でも `rec` でもなく、単純識別子の後ろに `:`・`=` が続く通常の形であれば従来の `Stmt::Let` / `ExprKind::Let` を作り、`(`・`{` やコンストラクタパターンで始まる場合はパターンを読んで後続のブロック残余を本体とする `MatchOrigin::LetDestructuring`（または `Stmt::LetPattern` → フォーマッタが `let <pat> = <expr>` の形で保つための AST ノード）にする。**重要**: `tsuzuri fmt` が `let (a, b) = pair` を `match` に書き換えてしまわないよう、AST（`Stmt` / `TopLevel`）では `LetPattern` を保持し、型検査で `match_value` へ下げる。

## 前提とする他チケットのインターフェース

- なし（既存の `A03`・`G19`・`G22`・`G23` の上に立つ）。

## 他チケットへ提供するインターフェース

- `DiagnosticKind::RefutableBindingPattern`（`E1021`）・`RefutableBindingPatternWarning`（`W1012`）。
- Phase 2 の `let <pattern>` の構文解析補助（[NS06](NS06-if-let-conditional-binding.md) の `if let <pattern> = <expr>` とパターン読み取りを共有できる）。

## 実装手順

### Phase 1

1. **変更**: なし（ブランチ `feat/ns04-irrefutable-patterns`）。**内容**: `node scripts/repo-kb.mjs brief NS04`、リポジトリ内の全 `.tz` / `.tc` / `.md` 例で反駁可能束縛パターンがないか確認。**確認**: `tests/trap_locations.rs` と `tests/prove.rs` の 2 か所のみであることを確認。
2. **変更**: `src/syntax.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/control.rs`, `src/select.rs`, `src/exhaustiveness.rs`, `src/diagnostic_kinds.rs`。**内容**: 束縛分解位置の網羅性検査と edition 2026 (`W1012`) / 2027 (`E1021`) 分岐。**確認**: `tests/match_exhaustiveness.rs` の新規テスト、T0。
3. **変更**: `tests/trap_locations.rs`, `tests/prove.rs`。**内容**: 実行時 `PatternMismatch` トラップ自体のテストに `edition = "2026"` マニフェストを添えて意図を明示し、2027 で `E1021` になるテストを並置する。**確認**: T1（全 Rust テスト）。
4. **変更**: 文書と `node scripts/gen-docs.mjs`。**内容**: Phase 1 の文書更新。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。

### Phase 2

5. **変更**: `src/syntax.rs`, `src/parser.rs`, `src/syntax_codec.rs`, `src/frontend_cache.rs`, `src/formatter.rs`, `src/control.rs`, `src/closures.rs`, `src/ownership.rs`（必要箇所の確認）。**内容**: `let <pattern> [: Type] = <expr>` の構文・整形・型検査・所有権検査。単一識別子 `let x = ...` は既存経路のまま保つ。**確認**: `let_destructuring_irrefutable_only`、`tsuzuri fmt` の往復一致、既存 fixture の IR が byte 一致すること、T1。
6. **変更**: 文書。**内容**: `values.md`・`pattern-matching.md`・`docs/language.md` の更新。**確認**: T2、`node scripts/check-docs.mjs <変更したページ>`。

## テスト計画

- `tests/match_exhaustiveness.rs`:
  - 受理（警告なし）: `\(x, y) -> x + y`、`\(Point { x }) -> x`、`for (k, v) in pairs do ...`、単一ケース `union UserId = UserId of i64` の `\(UserId id) -> id`、Phase 2 の `let (a, b) = (1, 2)`・`let Point { x, y } = p`・非 Copy 文字列フィールドの部分 move。
  - 拒否（2027 で `E1021`、2026 で `W1012`）: `\(Some x) -> x`、`\[a, b] -> a + b`、`\0 -> 1`、`\x when x > 0 -> x`、`for Some x in items do ()`、`Maybe { let! (Some x) = Some (Some 1); return x }`、`select` の `| recv (Some x) from ch -> x`。
  - Phase 2 拒否（全 edition で `E1021`）: `let Some x = Some 1`、`let [a, b] = [1, 2]`、`let 0 = n`。
  - 整形（Phase 2）: `let (a, b): (i64 * i64) = pair` が `tsuzuri fmt` で変わらないこと。

## ドキュメント

- `docs/language.md`: 「match と分解」「式と評価順序」「診断」。
- `_tsuzuri/language-reference/pattern-matching/pattern-matching.md`（「文脈による検査規則の違い」の表と `let` 分解の注記を更新）、`loops-and-conditionals/for-in.md`、`values-and-functions/lambda-expressions.md`、`values-and-functions/values.md`、`compiler/diagnostics.md`。

## 受け入れ条件

- [ ] edition 2027 でラムダ引数・`for...in`・計算式束縛・`select` 受信の反駁可能パターンが `E1021`（`refutable_binding_pattern`）になり、edition 2026 で `W1012` になる。
- [ ] Phase 2 で反駁不可能な `let <pattern> = <expr>` が動作し、反駁可能な `let Some x = ...` が `E1021` になる。
- [ ] 単一識別子 `let x = ...` や既存の反駁不可能パターンを使うプログラムの生成 IR が 1 byte も変わらない。
- [ ] `node scripts/gen-docs.mjs --check` と `node scripts/check-docs.mjs` が通る。

## 落とし穴

- `const def` 内の `let (a, b) = ...`（Phase 2）: `src/const_rules.rs` / `src/const_machine.rs` は `Match` を評価できるので動くが、検査と評価の両方で `LetPattern` の展開先が通ることをテストで確かめる。
- トップレベルの `let`（スクリプト形式の `Main.tz`）とブロック内の `let` の両方で同じ規則にすること。また `def f :: (i64 * i64) -> i64` の直後の `let f (a, b) = a + b`（関数実装の `let`）を誤ってパターン束縛と読まないこと（先行する未実装の `def f` があるかで既存の `parser.rs` が判別している）。

## 対象外

- `let mut (a, b) = ...`（個別に `let (a0, b0) = ...; let mut a = a0` と書く）。
- `let Some x = expr else ...`（Tsuzuri には関数からの命令形早期 `return` がないため、[NS06](NS06-if-let-conditional-binding.md) の `if let` で扱う）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 単一パターンの分解位置（ラムダ引数・`for...in`・計算式束縛・`select` 受信）の反駁可能パターンを、edition 2027 で `E1021`、edition 2026 で警告 `W1012` にする | 2026 パッケージの後方互換性を保ちつつ、既定の 2027 では `None` 等による実行時トラップの抜け穴を静的に塞ぐため | 要承認 |
| D2 | 警告コード `W1012` を D-55 の予約枠（`W1011`〜`W1015`）から割り当てる（`W1011` は [NS05](NS05-warn-unchecked-unwrap.md)） | 既存の `W1003`（到達不能アーム）とは意味が異なるため独立したコードにする | 要承認 |
| D3 | Phase 2 で不変 `let` に反駁不可能パターン分解を許し、`let mut` は単一識別子のままにする | タプル・レコード分解の利便性を高めつつ、部分的な可変性や構文の複雑化を避けるため | 要承認 |
| D4 | エラーコードは新規コードではなく既存の `E1021`（網羅性不足）の新しい kind `refutable_binding_pattern` とする | 「パターンがすべての値を受け止められない」という本質が `match` の網羅性不足と同じであり、D-51 の方針（既存コードで表せるなら kind で分ける）に沿うため | 既定案（実装者はこの案に従う） |
| D5 | 生成 IR の不一致時トラップブロック（`TrapKind::PatternMismatch`）自体は多重防御として残す | D-10（「網羅性検査を通過したコードであっても安全のため失敗時トラップを残す」）と同一方針 | 既定案（実装者はこの案に従う） |
| D6 | `tsuzuri fmt` で `let (a, b) = pair` の形を保つため、構文木では専用ノードを持ち型検査で `match_value` へ下げる | パーサーで直接 `ExprKind::Match` に脱糖するとフォーマッタが `match pair with | (a, b) -> ...` に書き換えてしまうため | 既定案（実装者はこの案に従う） |
