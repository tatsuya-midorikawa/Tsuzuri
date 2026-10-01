# G12: LSP の拡張（補完・rename・参照・整形・inlay hints）

| 項目 | 内容 |
| --- | --- |
| ID | G12 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | G07, G20 |
| 後続 | A15 Phase 2, G13 |
| 状態 | done（Phase 1） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要 |
| 改善する劣位 | Rust 比: 開発ツールの成熟度（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)）、C#/F# 比: IDE 支援が発展途上（[同](../../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | 要求の処理: `src/lsp.rs` の `Session::handle` の hover・definition・documentSymbol を処理する arm（`Session::refresh`、`PositionMapper::offset`・`range`、`SemanticIndex::at`）。索引の収集: `src/semantic.rs` の `collect`・`SemanticIndex::symbol`・`type_entry`・`type_target`。整形の安全検査: `src/formatter.rs` の `format_source`（`ast_fingerprint` の一致）。テスト: `tests/lsp.rs` の `semantic_index_preserves_source_types_and_definitions`、`tests/lsp_sessions.mjs` の `session`・`request`・`position` |
| 主な影響ファイル | `src/lsp.rs`, `src/semantic.rs`, `src/check.rs`（名前の使用位置の side table だけ）, `src/control.rs`（record pattern・case pattern の使用位置だけ）, `src/lib.rs`（変更なし。`analyze_modules_with_semantics` を使う）, `src/formatter.rs`・`src/lexer.rs`（変更なし。`format_source`・`lex_all` を呼ぶ）, `tests/lsp.rs`, `tests/lsp_sessions.mjs`, `vsc/README.md`（`vsc/package.json`・`vsc/src/extension.ts` は変更なし。D8）, `README.md`, `_docs/tools/editor-tools.md`, `docs/architecture.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

rust-analyzer、Roslyn、Ionide（F#）と同じく、参照検索・rename・symbol 検索・補完・signature help・semantic tokens・安全な
code action・LSP 経由の整形を提供する。応答は決定的で、UTF-8／UTF-16 のどちらの位置単位でも同じ内容を返す。
rename と code action は、編集後のソースを内部で再解析し、意味が変わらないことを確かめてから返す（D5）。

実装者は Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G07 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認: `grep -nE "^\| (G07|G12|G20) " _features/README.md`。
- G20 は Phase 1 の着手条件にしない（D1）。HEAD の G20 は todo。G20 の完了後は、回復した関数の型が同じ `SemanticIndex` に入るだけで、
  G12 の要求処理は変えない。
- 承認は不要（新しい crate・言語の意味・既定の WASM import を変えない）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（既存の LSP テスト 3 種）が成功していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 名前の使用位置を集めるために `TypedExprKind`・`ExprKind`・`PatternKind`・`TypeExprKind` の variant や field を変える必要が出た
  （D3 の side table で足りない）。
- 索引の収集に単相化後・closure lowering 後の情報、または `src/llvm.rs` の情報が要る。
- rename・code action の再解析検査（D5）が、意味の同じ編集を拒否する（偽陽性）例が既存 fixture で見つかり、規則を緩めたくなった。
- 既存テストの期待値を変える必要がある（`tests/lsp.rs` の 6 件、`tests/lsp_sessions.mjs` の既存 assert、`initialize` 応答の既存キーの値）。
  capability へのキーの追加だけは除く。
- 新しい crate（`lsp-types`、`tower-lsp` など）、`unsafe`、解析スレッドの追加、全量同期（`"change": 1`）の変更が必要になった。
- `vsc/src/extension.ts` の client 側コード（middleware・独自要求）を変えないと機能が動かない。
- 1 要求の処理に project 全体の解析が 2 回より多く要る（`refresh` の 1 回と D5 の検証の 1 回まで）。
- UTF-16 の位置計算を `PositionMapper` の外に書きたくなった。
- G17・PB06 の解析キャッシュやビルドサーバーがないと応答が成り立たない（Phase 1 はこれらに依存しない。D9）。

## 現状（HEAD `f8dc655` で確認）

### サーバー（`src/lsp.rs`）

- 枠組み: `read_message`・`write_message`（`serde_json`）。上限は `MAX_HEADER`（8192 bytes）と `MAX_MESSAGE`（16 MiB）。`serve` が reader
  スレッドと `$/cancelRequest`（`-32800` `request cancelled`）を扱う。
- `Session::handle` の `initialize` は次を返す。`capabilities.general.positionEncodings` に `"utf-8"` があれば `PositionEncoding::Utf8`、
  なければ `Utf16`。

```json
{"capabilities": {"positionEncoding": "utf-16", "textDocumentSync": {"openClose": true, "change": 1}, "hoverProvider": true, "definitionProvider": true, "documentSymbolProvider": true}, "serverInfo": {"name": "Tsuzuri", "version": "0.1.0"}}
```

- 扱うメソッドは `initialize`、`initialized`、`shutdown`、`exit`、`textDocument/didOpen`・`didChange`・`didClose`、
  `workspace/didChangeWatchedFiles`、`textDocument/hover`・`definition`・`documentSymbol`、`$/cancelRequest`。ほかは `_` arm で
  `-32601` `method not found: <method>`。初期化前は `-32002`、shutdown 後は `-32600`、引数の誤りは `invalid` による `-32602`。
- `didChange` は全量同期だけ（`only full-document synchronization is supported`）。1 ファイル `crate::syntax::MAX_SOURCE_BYTES`（1 MiB）、
  開いた buffer の合計 32 MiB・1024 個まで。
- `Session::schedule` は 200 ms 後の再解析を予約する。`Session::refresh` は `Project::load_with_overlays` で project を読み、
  `crate::analyze_inputs_indexed_all(&inputs, Some(&mut semantic))` で全体を解析する。成功なら `ProjectState::index` に索引を入れ、
  失敗なら `None`（hover・definition は `null`。`tests/lsp_sessions.mjs` の `stale` がこれを確かめる）。
- 診断の JSON は `range`・`severity`（1 か 2）・`code`・`source: "tsuzuri"`・`message` だけで、`data` はない。
- hover・definition・documentSymbol の arm は、予約中なら `refresh` し、path から source id を引き、`PositionMapper::offset`（失敗は
  `-32602` `position is outside the document or splits a Unicode character`）と `SemanticIndex::at` で項目を選ぶ。std の定義
  （`ModuleOrigin::Std` の source）への definition は `null`。

### 索引（`src/semantic.rs`）

- `SemanticIndex { entries, symbols, documentation }`。`check::check_modules_indexed_all` の最後で `semantic::collect(modules, &functions,
  &names, types)` が作る。単相化前の型を持つ。
- `SemanticEntry { span, detail, target, priority }` の priority は、ローカル 0、式・型式 1、宣言（`SemanticIndex::symbol`）2。
  `SemanticIndex::at` は最小の span、同じ大きさなら小さい priority を選ぶ。
- `DocumentSymbol { name, kind, span, selection }` の kind は extern・関数・test 12、record 23、union 10、型別名 26、const 14、
  class 5、method 6。
- ローカルと式の項目は `ModuleOrigin::User` の関数だけ。`target` は `TypedExprKind::Local` がローカルの `Local::span`、
  `TypedExprKind::Function(FunctionRef::User(id))`・`GenericFunction(id, _)` が `functions[id].span`、`TypedExprKind::Record(_)` が
  `type_target`。フィールドアクセス・union case・型別名には target がない。参照の一覧と、ローカルの有効範囲はない。
- 型付き木は名前の span を落とす。`ExprKind::Field(Box<Expr>, Ident)` は `TypedExprKind::Field(Box<TypedExpr>, usize)` に、
  `ExprKind::Record { name, fields }` の `Vec<(Ident, Expr)>` は `TypedExprKind::Record(Vec<(usize, TypedExpr)>)` になる。
  `PatternKind::Record(Option<Ident>, Vec<(Ident, Pattern)>)` は `src/control.rs` で `TypedExprKind::Field` の段に下ろされる。
  `resolve_type` は型別名を展開するので、`type_entry` は別名の使用を展開後の型として記録する。

### そのほか

- 整形: `src/formatter.rs` の `format_source(_path, source, kind) -> Result<FormatResult, Diagnostic>` は整形前後の `ast_fingerprint` の
  一致を確かめ、不一致は `E2000`。`FormatResult { changed, formatted }`。
- 警告の修正候補: `src/warnings.rs` の `unused_locals` が `W1001` `unused local '{name}'; prefix it with '_' to silence this warning`
  を `Local::span` に出す。`_` で始まるローカルは報告しない。
- 字句: `src/lexer.rs` の `lex_all(source) -> (Vec<Token>, Vec<Diagnostic>)` は構文エラーのあるソースでも token 列を返す。
  `Token { kind, span }`、`TokenKind::Ident(String)`・`Dot`・`LeftParen`・`RightParen`・`Comma`・`DocComment(String)`。
- VS Code: `vsc/src/extension.ts` が `vscode-languageclient` 9.0.1 の `LanguageClient` で `tsuzuri lsp` を起動する。client は server の
  capability から機能を有効にするので、client のコード変更は要らない。`vsc/package.json` は `grammars`（`source.tsuzuri`）と `snippets` を持ち、
  `semanticTokenScopes` はない。
- 文書: `_docs/tools/editor-tools.md` の「現在の capability」表は `completion、rename、semanticTokens | 未提供`。
- テスト: `tests/lsp.rs` の 6 件（`windows_file_uris_round_trip_drive_letters_and_canonical_paths`、
  `semantic_docs_follow_function_and_type_definition_targets`、`semantic_index_preserves_source_types_and_definitions`、
  `invalid_semantic_snapshots_are_not_returned`、`overlays_replace_disk_and_include_unsaved_files`、`server_lifecycle_and_protocol_errors_are_framed`）。
  `src/lsp.rs` の単体テスト `framing_roundtrips_and_rejects_invalid_inputs`・`positions_preserve_utf8_utf16_crlf_and_eof`。
  `tests/lsp_sessions.mjs` は `session("utf-16")` と `session("utf-8")` を順に走らせ、`request`・`notify`・`wait`・`position(source, needle)` を持つ。

### 再現（コードで確認）

`textDocument/references` などの要求は `Session::handle` の `_` arm に落ち、次の error を返す。capability も広告していない。

```json
{"jsonrpc": "2.0", "id": 2, "error": {"code": -32601, "message": "method not found: textDocument/references"}}
```

## 仕様

### Phase 分割

Phase 1（実装対象）は次の 8 機能を、この優先順で一つずつ実装する（手順 3–10）。各機能は単独で出荷でき、途中で止めても
広告した capability はすべて動く。documentSymbol・hover・definition は変えない（既存テストの期待値を保つ）。

| 順 | 機能 | LSP メソッド | `initialize` に足すキー | データ源 |
| --- | --- | --- | --- | --- |
| 1 | 参照検索・文書内の強調 | `textDocument/references`, `textDocument/documentHighlight` | `"referencesProvider": true`, `"documentHighlightProvider": true` | `SemanticIndex::definitions`・`references`（新規）、関数の宣言 head（A2） |
| 2 | 名前変更 | `textDocument/prepareRename`, `textDocument/rename` | `"renameProvider": {"prepareProvider": true}` | 1 と同じ、D5 の再解析 |
| 3 | workspace symbol | `workspace/symbol` | `"workspaceSymbolProvider": true` | 既存の `SemanticIndex::symbols`、`SourceFile::name` |
| 4 | 補完 | `textDocument/completion` | `"completionProvider": {"triggerCharacters": ["."], "resolveProvider": false}` | `lex_all`、`definitions`・`scopes`・`receivers`（新規）、`doc_for`、D6 |
| 5 | signature help | `textDocument/signatureHelp` | `"signatureHelpProvider": {"triggerCharacters": [" ", "("], "retriggerCharacters": [","]}` | `lex_all`、`Definition::parameters`（新規）、D6 |
| 6 | semantic tokens（全体） | `textDocument/semanticTokens/full` | `"semanticTokensProvider": {"legend": {"tokenTypes": TOKEN_TYPES, "tokenModifiers": TOKEN_MODIFIERS}, "full": true, "range": false}` | `definitions`・`references`、D6 |
| 7 | code action（安全な修正だけ） | `textDocument/codeAction` | `"codeActionProvider": {"codeActionKinds": ["quickfix"]}` | `ProjectState::diagnostics`（新規）、2 の rename 処理 |
| 8 | 整形 | `textDocument/formatting` | `"documentFormattingProvider": true` | `format_source` |

Phase 2（設計方針。人間が求めた場合だけ）: inlay hint（`let` の推論型、暗黙の借用、A15 の `copies::sites`）、期待型による補完の順位付け、
G20 の部分索引を使う壊れた関数内の補完、code action の追加（A03 の不足例からの match 節、`W1002` の private 削除、`W1003` の節の削除、
`E1019` の `rec`）、型別名・class・method の rename、`semanticTokens/range`・`delta`、増分同期（G17・PB06 と合わせる）、コメント内の補完の抑止。

### 前提とする他チケットのインターフェース

- G07（done）: `Session`・`ProjectState`・`PositionMapper`・`SemanticIndex` の現行の契約（「現状」のとおり）。
- G20（todo。着手条件ではない。D1）: 完了後は `analyze_inputs_indexed_all` が errors と部分的な索引を同時に返し得る。これに備え、rename・
  code action の可否は `ProjectState::index.is_some()` ではなく、`ProjectState::diagnostics`（新規）に `Severity::Error` がないことで判断する。
- A15（todo）: Phase 2 の inlay hint が `copies::sites` を使う。Phase 1 は使わない。
- D07（todo）: 文字列補間の穴の名前は実際の source 位置を持つので、1–6 は特別な処理なしで穴の中にも働く（D07 は semantic tokens を G12 に委ねている）。
- G17・PB06: 使わない（D9）。

### 共通の規則

- 位置: 入力の `position` は `PositionMapper::offset`、出力の範囲は `PositionMapper::range` だけで変換する。識別子は ASCII
  （`src/lexer.rs` の `identifier` は ASCII 英数字と `_`）なので長さは両単位で同じだが、同じ行の前にある非 ASCII 文字で列がずれる。
- 出現: `Definition::span` か `Reference::span`（どちらも識別子だけ）。位置 `p` では `start <= p < end` の出現を優先し、なければ
  `end == p` の出現（識別子の直後のカーソル）を選ぶ（`SemanticIndex::occurrence_at`（新規））。
- 索引がないとき（解析の失敗、project を読めない）: 1・2・3・7 は D6 を使わない。1 は `null`、2 は拒否（「診断」表）、3 はその project を
  飛ばす、7 は `[]`。4・5・6 は D6 の直前の成功索引を使う。8 は索引を使わない。
- std の定義（`ModuleOrigin::Std` の source）: references・highlight の結果に含めず、rename を拒否する。補完・signature help・
  semantic tokens（`defaultLibrary`）には使う。
- 順序: 位置の列は（source id、start）の昇順。source id は `Project::sources` の順。`WorkspaceEdit` の `changes` は `serde_json::Map`
  （既定は BTreeMap）なので URI 順になる。

### 要求ごとの規則

1. references: `context.includeDeclaration` が true なら宣言の出現を含める。出現がなければ `null`。結果は `Location[]`。
   documentHighlight は同じ文書の出現だけで、`kind` は宣言と代入先（`TypedExprKind::Assign` の左辺の `Local`）が 3、ほかは 2。
2. prepareRename: `{"range": <出現>, "placeholder": "<name>"}`。対象はローカル（引数・パターン束縛を含む）、利用者の関数（`export` なし）、
   const、record、union、union case、フィールド（D4）。rename: `newName` を検査し、全出現を置換する `{"changes": {uri: TextEdit[]}}` を作り、
   D5 の検証に通ったときだけ返す。`newName` が元と同じなら `{"changes": {}}`。衝突の事前検査: 同じモジュールの同じ名前空間
   （値: 関数・const・case。型: record・union・型別名・class）、同じ record のフィールド、同じ union の case、ローカルはどれかの出現の
   位置で `newName` のローカルが見えている（`scopes`）場合。事前検査を通っても D5 が失敗すれば拒否する。
3. workspace/symbol: `query` と名前を ASCII 小文字にして部分一致（空の `query` は全件）。解析済みの全 project（予約中は先に `refresh`）の
   `ModuleOrigin::User` の source だけ。項目は `{"name", "kind", "location": {"uri", "range": <symbol.span>}, "containerName": <SourceFile::name>}`。
4. completion: 位置が `TokenKind::String`・`TokenKind::DocComment` の token の中なら `{"isIncomplete": false, "items": []}`。
   カーソル直前の `Ident`（入力中の断片。なくてもよい）の前が `Dot` で、その前に空白なしの `Ident (Dot Ident)*` があればメンバー文脈:
   (a) 連結した名前が project の `SourceFile::name` と一致すれば、そのモジュールの定義（private は同じモジュールのときだけ）と、
   その名前で始まる子モジュールの次の区切り。(b) 最後の区切りの出現が union なら、その case。(c) ほかは `receivers` のうち
   `span.end` が `Dot` の start と等しい最長のものの record のフィールド。どれでもなければ空。
   メンバー文脈でなければ名前文脈: 位置を含む `scopes` のローカル（同名は定義の start が大きい方だけ）、同じモジュールの関数・const・
   record・union・case・型別名・class、project と std のモジュール名の先頭区切り、`KEYWORDS`（新規）。
   項目は `{"label", "kind", "detail", "sortText": "<g>_<label>"}` に、doc comment があれば `"documentation": {"kind": "markdown", "value": ...}`
   （`SemanticIndex::doc_for(definition.span)`）。g はローカル 0、同じモジュール 1、モジュール・メンバー 2、予約語 3。接頭辞での絞り込みは
   しない（client が行う）。500 件を超えたら（g、label）順の先頭 500 件と `"isIncomplete": true`。
5. signatureHelp: `lex_all` の token を位置から後ろへ読み、括弧の深さ 0 で行頭・`=`・`->`・`;`・`{`・`,`・二項演算子・予約語（`ref`・`mut`・
   `deref` を除く）に当たるか、対応のない `(` に当たるまで戻る。`(` の直前に空白なしで識別子があれば互換形式 `f(x, y)` で、引数番号は
   深さ 0 の `,` の数。そうでなければ区間の先頭の `Ident (Dot Ident)*` が head で、引数番号は head の後の深さ 0 の項の数（`ref`・`ref mut`・
   `deref`・`&`・`&mut`・`*` は次の項と合わせて一つ。位置が項の末尾に接していればその項の番号）。head の最後の区切りの出現が
   `Function`・`Extern` でなければ `null`。結果は `{"signatures": [{"label", "parameters": [{"label": "<name>: <type>"}, ...]}], "activeSignature": 0, "activeParameter": n}`。
   label は D13。n が引数の数以上でもそのまま返す（client は強調しない）。
6. semanticTokens/full: 文書の全出現（D6 のときは写せた出現だけ）を start 順に並べ、同じ start は一つにする。修飾名の前の区切り
   （`Geometry.Point.nested` の `Geometry`・`Point`）は `namespace`。凡例は D7。`data` は LSP の相対形式（行差、列差、長さ、種類、
   修飾子の bit）で、列と長さは `PositionMapper::position` で求める。索引も D6 もなければ `{"data": []}`。
7. codeAction: `ProjectState::diagnostics` のうち範囲が要求の `range` と交わる `W1001` ごとに、そのローカルを `_<name>` へ rename する
   action を一つ作る（D10）。`context.diagnostics` は信用しない。`context.only` があり `quickfix` を含まなければ `[]`。項目は
   `{"title": "Prefix '<name>' with '_'", "kind": "quickfix", "diagnostics": [<公開した診断の JSON>], "isPreferred": true, "edit": <WorkspaceEdit>}`。
   D5 に通らない action は出さない。
8. formatting: `format_source(&path.to_string_lossy(), text, kind)`（kind は `SourceKind::from_extension`）。`changed` なら文書全体を置換する
   TextEdit 一つ、変化なしなら `[]`、失敗（構文エラー・`E2000`）なら `null`。`options`（`tabSize` など）は使わず、CLI の `tsuzuri fmt` と同じ結果にする。

### 診断

Tsuzuri の診断コードは追加しない。要求の拒否は JSON-RPC の error で返す（D11）。メッセージは英語、小文字で始め、直し方を含む。

| コード | 条件 | メッセージ | 対象 |
| --- | --- | --- | --- |
| `-32602` | `newName` が識別子一つでない、または 256 bytes を超える | `'<new>' is not a valid identifier; use letters, digits and '_' and avoid keywords` | rename |
| `-32602` | 位置が文書外・文字の途中（既存） | `position is outside the document or splits a Unicode character` | 位置を取る全要求 |
| `-32803` | 位置に名前がない | `no renamable name at this position; place the cursor on a name` | prepareRename・rename |
| `-32803` | errors がある（D1） | `cannot rename while the project has errors; fix the errors first` | prepareRename・rename |
| `-32803` | std の定義 | `cannot rename '<name>' because it is defined in the standard library` | prepareRename・rename |
| `-32803` | `export` 付きの関数 | `cannot rename exported function '<name>' because its export name is part of the ABI; change the export manually` | prepareRename・rename |
| `-32803` | D4 の対象外の種類 | `renaming <kind> names is not supported yet; rename '<name>' manually` | prepareRename・rename |
| `-32803` | 事前検査の衝突 | `renaming '<old>' to '<new>' conflicts with <kind> '<new>'; choose another name` | rename |
| `-32803` | 出現が 10,000 を超える | `rename would change more than 10000 locations; rename in smaller steps` | rename |
| `-32803` | D5 の検証の失敗 | `rename would change the meaning of the program (<detail>); choose another name or rename manually` | rename |

`<kind>` は `type alias`・`class`・`method`・`extern function`・`module` など `SymbolKind` の英語名。`<detail>` は再解析で増えた最初の診断の
`<code>: <message>`、または `a name would refer to a different definition`。

### 資源上限

references・highlight は整列後の先頭 10,000 件で打ち切る。rename は 10,000 出現を超えたら拒否。completion 500 件、workspace/symbol 256 件、
`newName` 256 bytes。1 要求あたりの project 全体の解析は `refresh` の 1 回と D5 の 1 回まで。既存の上限（`MAX_MESSAGE`、`MAX_SOURCE_BYTES`、
buffer の合計 32 MiB・1024 個）は変えない。

## 設計

### データ構造

```rust
// src/semantic.rs（既存 field は変えず、4 field を足す）
pub struct SemanticIndex {
    pub entries: Vec<SemanticEntry>,
    pub symbols: Vec<DocumentSymbol>,
    pub documentation: Vec<(Span, String)>,
    pub definitions: Vec<Definition>,  // （新規）
    pub references: Vec<Reference>,    // （新規）(span.source, span.start) 順
    pub scopes: Vec<LocalScope>,       // （新規）
    pub receivers: Vec<(Span, usize)>, // （新規）record 型の式の span と、その record の definitions 添字
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind { Module, Record, Union, Case, Field, Alias, Class, Method, Function, Extern, Const, Parameter, Local } // （新規）

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role { Declaration, Write, Read } // （新規）

#[derive(Clone, Debug)]
pub struct Definition {           // （新規）
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,               // 名前の識別子だけ
    pub module: String,           // ModuleInput::name
    pub container: Option<usize>, // Field・Case: record・union の definitions 添字
    pub detail: String,           // hover と同じ書式（"x: i64"、"def Main.read :: i64 -> i64"）
    pub public: bool,             // Visibility::Public
    pub exported: bool,           // FunctionDecl::exported・SignatureDecl::exported
    pub parameters: Vec<String>,  // Function・Extern: "number: i64"
    pub result: String,           // Function・Extern: 結果型の表示。ほかは空
}

#[derive(Clone, Copy, Debug)]
pub struct Reference { pub span: Span, pub definition: usize, pub role: Role } // （新規）

#[derive(Clone, Copy, Debug)]
pub struct LocalScope { pub definition: usize, pub visible: Span } // （新規）
```

```rust
// src/check.rs（型付き木は変えず、落ちる名前の span を side table に集める）
#[derive(Clone, Copy, Debug)]
pub(crate) enum NameTarget { Record(usize), Union(usize), Field(usize, usize), Case(usize, usize) } // （新規）
// Checker<'a> に足す field
indexing: bool,                                // （新規）semantic が Some のときだけ true
pub(crate) name_uses: Vec<(Span, NameTarget)>, // （新規）

// src/lsp.rs
struct ProjectState { project: Project, index: Option<SemanticIndex>, mappers: Vec<PositionMapper>, diagnostics: Vec<Diagnostic> } // diagnostics（新規）
// Session に足す field: good: BTreeMap<PathBuf, ProjectState>（新規。D6）
const KEYWORDS: [&str; 41] = ["fn", "def", /* identifier の予約語を同じ順で */ "false"]; // （新規）
const TOKEN_TYPES: [&str; 11] = ["namespace", "struct", "enum", "enumMember", "property", "type", "interface", "method", "function", "variable", "parameter"]; // （新規）
const TOKEN_MODIFIERS: [&str; 3] = ["declaration", "readonly", "defaultLibrary"]; // （新規）
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 検査 | `src/check.rs` | `Checker` | `indexing`・`name_uses`。`check_modules_indexed_all` が checker を作る箇所で `indexing = semantic.is_some()` |
| 検査 | `src/check.rs` | `Checker::field_access` | `Type::Record(id, _)` の arm で `(field.span, Field(id, index))` |
| 検査 | `src/check.rs` | `ExprKind::Record` の arm、`Checker::record_literal` | record 名の最後の区切りに `Record(id)`、各フィールドの `Ident` に `Field(id, index)` |
| 検査 | `src/check.rs` | `Checker::record_update` | 各フィールドの `Ident` に `Field(id, index)` |
| 検査 | `src/check.rs` | `Names::case` で case を得る箇所、`Checker::case_reference` の呼び出し元 | case 名の `Ident` に `Case(union, case)`、修飾の union 名に `Union(union)` |
| 検査 | `src/control.rs` | `pattern_alternatives` の `PatternKind::Record`・`PatternKind::Apply` | record 名・フィールド・case に同じ規則 |
| 検査 | `src/check.rs` | `check_modules_indexed_all` | 関数の loop で `ModuleOrigin::User` の checker の `name_uses` を集め、`semantic::collect` の引数に足す |
| 索引 | `src/semantic.rs` | `collect`、`type_entry` | 定義・参照・scope・receiver を作る（A1） |
| 索引 | `src/semantic.rs` | `SemanticIndex::occurrence_at`・`occurrences`（新規） | 位置の出現（`at` と同じ走査）と、定義の全出現 |
| LSP | `src/lsp.rs` | `Session::handle` の `initialize` | 手順ごとにキーを足す。既存キーの値は変えない |
| LSP | `src/lsp.rs` | `Session::refresh`、`Session::schedule` | `diagnostics` を保存。`schedule` で外した状態が索引を持てば `good` へ移し、成功した `refresh` で `good` から消す |
| LSP | `src/lsp.rs` | `Session::document`（新規） | 既存 arm の「予約中なら `refresh`、path から source id」を関数にし、既存 arm と新 arm が共有する |
| LSP | `src/lsp.rs` | `function_heads`・`references`・`rename_edits`・`verify_edits`・`completion`・`signature_help`・`semantic_tokens`・`code_actions`・`format_document`・`StaleMap`（すべて新規） | 要求ごとの処理。`handle` の match に arm を足す |
| client | `vsc/` | — | 変更なし（D8） |

### アルゴリズム

- A1 索引（`collect`）: 宣言から `Definition` を作る（record・フィールド・union・case・型別名・const・関数・extern・class・method）。関数と const は
  `(module, name)` で `CheckedFunction` と結ぶ（const も関数として検査される。`names.constants`）。`types.records[id]`・`types.unions[id]` は
  `CheckedRecord::name`・`CheckedUnion::name`（`Module.Name`）で定義へ結び、フィールド・case は宣言順の添字で結ぶ。ローカルは既存の `locals` の
  走査で定義にし（関数・lambda の引数は `Parameter`）、`TypedExprKind::Local` を `Read`、`Assign` の左辺を `Write` の参照にする。同じ arm の
  alternatives の同名の束縛は、最初を定義、ほかを `Declaration` の参照にする。関数・const の参照は `TypedExprKind::Function(FunctionRef::User(id))`・
  `GenericFunction(id, _)` の span の末尾 `name.len()` bytes。型式は `type_entry` で、`TypeExprKind::Named` は span の末尾、`Apply` は head の
  `Ident` を record・union の参照にする。`name_uses` はそのまま参照にする。`Provenance::Generated` の名前は飛ばす。scope は束縛の終わりから、
  block・lambda・`for`・match の arm・関数本体の終わりまで。receiver は参照型（`Type::Reference`）を剥がして `Type::Record` になる式。
  最後に参照を整列し、同じ span の重複を除く。
- A2 関数の宣言 head: 定義のモジュールの source を `lex_all` し、`Def`・`Fn`・`And`（直後の `Rec` を読み飛ばす）と、列 0 の `Let` の直後の
  `Ident(name)` を `Declaration` の出現に足す（`def` と `fn` の 2 か所に名前がある形式のため）。
- A3 D5 の検証（`verify_edits`）: 編集を overlay に適用した source で project を解析する。(1) 成功し、`W1001` 以外の警告の code の多重集合が
  変わらず、`W1001` の数が増えない。(2) 旧索引の各出現（A2 を含む）を編集で写した start に新索引の出現があり、その定義の start も写した旧定義の
  start と一致する。(3) 出現の総数が同じ。
- A4 `StaleMap`（D6）: 旧テキストと新テキストの共通接頭辞 `p` と、重ならない共通接尾辞 `s`（どちらも char 境界）を求める。旧 offset `o` は
  `o <= p` なら `o`、`o >= old.len() - s` なら `o + new.len() - old.len()`、ほかは写せない。span は両端が写せるときだけ使う。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。`tests/lsp.rs` の件数は手順ごとに累計で書く。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: 既存の LSP テストと stack-depth の 3 テストを走らせる。検証済みサンプル（「テスト計画」の P1・P2）を `/tmp/tz-g12/p1`・`p2` に置く。
- 確認: 次がすべて成功する。`--test lsp` は `6 passed`、lib の 2 テストと stack-depth の 3 テストはそれぞれ `1 passed`、`check` は P1 が
  `W1001` 一つ（`Warn.tz:1:24`）、P2 が診断なしで終了コード 0。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked --test lsp
cargo test --locked --lib framing_roundtrips_and_rejects_invalid_inputs
cargo test --locked --lib positions_preserve_utf8_utf16_crlf_and_eof
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
node tests/lsp_sessions.mjs target/release/tsuzuri
target/release/tsuzuri check /tmp/tz-g12/p1
target/release/tsuzuri check /tmp/tz-g12/p2
```

### 手順 2: 名前の使用位置の side table

- 変更: `src/check.rs` の `Checker`・`field_access`・`record_literal`・`ExprKind::Record` の arm・`record_update`・case を解決する箇所・
  `check_modules_indexed_all`、`src/control.rs` の `pattern_alternatives`、`src/semantic.rs` の `collect` の引数（まだ使わない）。
- 内容: 「段ごとの変更」の検査の行。push は `self.indexing` のときだけ、`Provenance::Generated` の `Ident` は push しない。新しい再帰や helper は足さない。
- 確認: `cargo test --locked` が成功する。手順 1 の stack-depth 3 テストが成功する。`cargo test --locked --test lsp` が `6 passed`
  （`semantic_index_preserves_source_types_and_definitions` が索引ありとなしの IR の一致を確かめる）。

### 手順 3: 定義・参照・scope・receiver の索引

- 変更: `src/semantic.rs`（データ構造、`collect`、`type_entry`、`occurrence_at`・`occurrences`）、`tests/lsp.rs`。
- 内容: A1。P3（`def adder :: i64 -> i64 -> i64` と `fn adder n = \x -> x + n`）は `/tmp/tz-work-G12/c` で `check` 済み。
- 確認: `cargo test --locked --test lsp` が `10 passed`（索引テスト 4 件）。

### 手順 4: references・documentHighlight

- 変更: `src/lsp.rs` の `Session::document`・`function_heads`・`references`、`handle` の arm、`initialize` の 2 キー。`tests/lsp.rs` に
  helper `scripted`（新規）を足す。`scripted(files, encoding, requests) -> Vec<Value>` は一時ディレクトリにファイルを書き、initialize・
  didOpen・要求・shutdown・exit を `write_message` で並べて `serve` に渡し、応答を id 順に返す（`server_lifecycle_and_protocol_errors_are_framed` の形）。
- 内容: 共通の規則と要求 1。既存の hover・definition・documentSymbol の arm も `Session::document` を使うが、応答は変えない。
- 確認: `cargo test --locked --test lsp` が `11 passed`。`cargo test --locked --lib function_heads_find_def_fn_and_and` が `1 passed`。

### 手順 5: prepareRename・rename

- 変更: `src/lsp.rs` の `rename_edits`・`verify_edits`、arm、`initialize` のキー、`ProjectState::diagnostics`。
- 内容: 要求 2、「診断」表、A3。`verify_edits` は `refresh` と同じ入力の組み立て（`crate::SourceInput`）を使い、overlay に編集後のテキストを入れる。
- 確認: `cargo test --locked --test lsp` が `14 passed`。

### 手順 6: workspace/symbol

- 変更: `src/lsp.rs` の arm とキー。
- 内容: 要求 3。
- 確認: `cargo test --locked --test lsp` が `15 passed`。

### 手順 7: completion と直前の成功索引

- 変更: `src/lsp.rs` の `Session::good`・`schedule`・`refresh`・`StaleMap`・`completion`・`KEYWORDS`、arm とキー。
- 内容: 要求 4 と D6。`schedule` は `self.projects.remove(&directory)` の戻り値が索引を持てば `good` に入れる。
- 確認: `cargo test --locked --test lsp` が `16 passed`。`--lib keywords_match_the_lexer` と `--lib stale_map_keeps_prefix_and_shifts_suffix` が各 `1 passed`。
  再ビルド後の `node tests/lsp_sessions.mjs target/release/tsuzuri` が成功する（既存の `stale` の `null` が変わらない）。

### 手順 8: signatureHelp

- 変更: `src/lsp.rs` の `signature_help`、arm とキー。
- 内容: 要求 5 と D13。
- 確認: `cargo test --locked --test lsp` が `17 passed`。

### 手順 9: semanticTokens/full

- 変更: `src/lsp.rs` の `semantic_tokens`・`TOKEN_TYPES`・`TOKEN_MODIFIERS`、arm とキー。
- 内容: 要求 6 と D7。
- 確認: `cargo test --locked --test lsp` が `18 passed`。

### 手順 10: codeAction

- 変更: `src/lsp.rs` の `code_actions`、arm とキー。
- 内容: 要求 7 と D10。rename の処理（`rename_edits`・`verify_edits`）を再利用する。
- 確認: `cargo test --locked --test lsp` が `19 passed`。

### 手順 11: formatting と capability の全体

- 変更: `src/lsp.rs` の `format_document`、arm とキー。
- 内容: 要求 8。最後に `initialize` の応答全体を固定するテストを足す。
- 確認: `cargo test --locked --test lsp` が `21 passed`。

### 手順 12: 実セッション

- 変更: `tests/lsp_sessions.mjs`。`session` の framing・`wait`・`request`・`notify`・`position` を `connect(encoding)`（新規）へ出す（既存 assert は不変）。
  `features(encoding)`（新規）を足し、`session("utf-16")`・`session("utf-8")` の後に `features("utf-16")`・`features("utf-8")` を走らせる。
- 内容: 「E2E」の S1–S10。
- 確認: `cargo build --release --locked && node tests/lsp_sessions.mjs target/release/tsuzuri` が終了コード 0。

### 手順 13: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/editor-tools.md _docs/feature-status.md` が成功する。

### 手順 14: 最終確認

- 変更: なし。
- 確認: GUIDE §2.3 の基準コマンド、`cargo test --locked`、手順 1 の stack-depth 3 テスト、`node tests/lsp_sessions.mjs target/release/tsuzuri`
  が成功する。VS Code で `vsc/` の拡張から P1 を開き、参照検索・rename・補完・semantic highlight・quick fix・整形が動くことを目で確かめる。

## テスト計画

### 共通の源

P1（`/tmp/tz-work-G12/a` で `check` 済み。`Warn.tz:1:24` の `W1001` だけ）:

```tsuzuri
def read :: i64 -> i64
fn read number = { let text = "日😀"; let result = number + text.length; result }
def point :: Shapes.Point -> i64
fn point value = value.x
def twice :: i64 -> i64
fn twice n = read (read n)
```

`Shapes.tz` は `record Point { x: i64 }`、`Warn.tz` は `fn warn() -> i64 { let spare = 1; 2 }`。

P2（`/tmp/tz-work-G12/b/Main.tz` で `check` 済み。診断なし）:

```tsuzuri
record Point { x: i64 }
union Shape = Circle of i64 | Empty
const LIMIT: i64 = 3
def make :: i64 -> Point
fn make n = Point { x: n }
def get :: Point -> i64
fn get p = match p with
    | Point { x = v } -> v + LIMIT
def area :: Shape -> i64
fn area s = match s with
    | Circle r -> r
    | Empty -> 0
def moved :: Point -> Point
fn moved p = { p with x = 2 }
```

期待値は手で数えた出現（`str::find`・`position(source, needle)` で位置を求める）で、コンパイラの出力から写さない。
P1 の 2 行目は `日`（UTF-16 で 1、UTF-8 で 3）と `😀`（2 と 4）を含むので、その後ろの列は UTF-8 が 4 大きい。

### Rust テスト

`tests/lsp.rs`（索引は `analyze_modules_with_semantics`、要求は `scripted`）:

| テスト | 内容 |
| --- | --- |
| `index_links_locals_and_parameters` | P1: `number` の出現が引数の宣言と 1 か所の使用、`result` が宣言と使用、`text` の scope が束縛の後から block の終わりまで |
| `index_links_records_fields_cases_and_constants` | P2: `x` の出現 4（宣言・リテラル `x:`・パターン `x =`・更新 `x =`）、`Point` 7、`Circle` 2、`LIMIT` 2 |
| `index_records_receivers_for_field_completion` | P1: `value.x` の `value` の span が `Point` の receiver |
| `index_follows_lambda_captures` | P3: `def adder :: i64 -> i64 -> i64` と `fn adder n = \x -> x + n`。`n` の出現が 2、`x` が 2 |
| `references_and_highlights_cover_def_and_fn_heads` | P1: `read` の references が `def read`・`fn read`・`read (read n)` の 2 つ（計 4）。highlight の kind（宣言 3、使用 2） |
| `rename_rewrites_fields_cases_and_records` | P2: `x`→`y` が 4 edit、`Circle`→`Round` が 2、`Point`→`Pt` が 7、`LIMIT`→`MAX` が 2 |
| `rename_rejects_std_export_conflicts_and_errors` | 「診断」表の各行（`export def answer :: i64 = 42`（`/tmp/tz-work-G12/d` で `check` 済み）、`match`・`a b` の `newName`、`result`→`number` の衝突、型エラーのある project、`fn z() -> f64 { Math.zero() }`（`/tmp/tz-work-G12/e` で `check` 済み）の `zero`）を code と message で確かめる |
| `rename_is_verified_by_reanalysis` | `def helper :: i64 -> i64`・`fn helper n = n + 1`・`def f :: i64 -> i64`・`fn f value = helper value` で `helper`→`value` が D5 で拒否され、message が `rename would change the meaning of the program (` で始まる |
| `workspace_symbols_filter_by_query_across_files` | P1 で `query: "PO"` が `point`（Main）と `Point`（Shapes）の 2 件、URI 順、`containerName` が `Main`・`Shapes` |
| `completion_offers_fields_members_locals_and_keywords` | S5 と同じ項目、`Shapes.` の後が `Point` だけ、`read` の本体の名前文脈に `number`・`text`・`read`・`Shapes`・`match` がある |
| `signature_help_counts_curried_arguments` | S6。`twice` の本体の `read (` の直後は `activeParameter` 0 |
| `semantic_tokens_encode_declarations_and_references` | S7 の `Shapes.tz` の `data` |
| `code_action_prefixes_unused_locals` | S8 の edit。`context.only: ["refactor"]` では `[]` |
| `formatting_matches_the_library_formatter` | 整形前の `fn warn() -> i64 {  let _spare = 1;  2 }` への edit の `newText` が `tsuzuri::formatter::format_source` の `formatted` と一致し、範囲が文書全体 |
| `capabilities_are_advertised_exactly` | `initialize` の応答全体が「Phase 分割」表のキーと既存キーだけ |

`src/lsp.rs` の単体テスト: `function_heads_find_def_fn_and_and`（`def rec`・`and`・列 0 の `let` を含む token 列）、`keywords_match_the_lexer`
（`KEYWORDS` の各語を `lex_all` すると `TokenKind::Ident` でない token 一つ、`identifier` の予約語と同数）、`stale_map_keeps_prefix_and_shifts_suffix`
（`"ab日cd"` → `"ab日XYZcd"` で offset 0–5 が同じ、`cd` が 3 bytes ずれ、編集区間は写せない）。

### E2E

`tests/lsp_sessions.mjs` の `features(encoding)`。P1 を新しい一時 root に書き、initialize（utf-8 のときは `positionEncodings: ["utf-8"]`）と
3 ファイルの didOpen の後に次を送る。範囲は `{"start": {"line": l, "character": a}, "end": {"line": l, "character": b}}` を `l:a-b` と略す。

| 番号 | 要求 | 期待（UTF-16） | 期待（UTF-8） |
| --- | --- | --- | --- |
| S1 | references、`number =` の位置、`includeDeclaration: true` | Main の `1:8-14`、`1:50-56` | `1:8-14`、`1:54-60` |
| S2 | documentHighlight、`result =` の位置 | `1:41-47` kind 3、`1:72-78` kind 2 | `1:45-51`、`1:76-82` |
| S3 | rename、`result` → `total` | Main に `1:41-47`・`1:72-78` の `"total"` | `1:45-51`・`1:76-82` |
| S4 | rename、Shapes の `x` → `y` | Shapes `0:15-16`、Main `3:23-24` の `"y"` | 同じ |
| S5 | didChange で 3 行目を `fn point value = value.` にし、completion `3:23` | `{"isIncomplete": false, "items": [{"label": "x", "kind": 5, "detail": "x: i64", "sortText": "2_x"}]}` | 同じ |
| S6 | didChange で P1 に戻し、signatureHelp、`n)` の位置 | `{"signatures": [{"label": "read (number: i64) -> i64", "parameters": [{"label": "number: i64"}]}], "activeSignature": 0, "activeParameter": 0}` | 同じ |
| S7 | semanticTokens/full、Shapes | `{"data": [0, 7, 5, 1, 1, 0, 8, 1, 4, 1]}` | 同じ |
| S8 | semanticTokens/full、Main を復号した 1 行目 | `[1,3,4,8,1] [1,8,6,10,1] [1,23,4,9,1] [1,41,6,9,1] [1,50,6,10,0] [1,59,4,9,0] [1,72,6,9,0]` | 列が `41→45`・`50→54`・`59→63`・`72→76` |
| S9 | codeAction、Warn `0:0-37`、`only: ["quickfix"]` | 1 件。`diagnostics` は publishDiagnostics で受けた `W1001` と同じ object、edit は Warn の `0:23-28` の `"_spare"` | 同じ |
| S10 | formatting、Warn | `[]`（整形済み） | 同じ |

最後に既存と同じく shutdown・exit の終了コード 0 と stderr が空であることを確かめる。S8 の復号は `data` の相対値を足し合わせる 10 行の helper で行う。

### 既存テストへの影響

なし。`initialize` の応答にキーが増えるが、既存テストは個別のキーだけを見る。`tests/lsp_sessions.mjs` の `session` は helper の移動だけで assert は変えない。

### 性能

閾値は置かない。手順 14 で 200 ファイル（各 50 関数）の生成 project の references・rename・completion の応答時間（`refresh` を含む）を 9 回の中央値で完了報告に書く（G17・PB06 の比較基準）。

## ドキュメント

- `_docs/tools/editor-tools.md`: 冒頭の「補完やリファクタリングは行いません」と「現在の capability」表（`completion、rename、semanticTokens` の未提供行を
  8 機能の行に置き換え）、「root と位置の単位」の後の注意書き。rename の拒否規則と D6 の挙動を短く書く。
- `README.md`: 「補完・rename・LSP経由の整形はまだ提供しません」を現在の提供範囲に直す。
- `vsc/README.md`: 機能一覧に Phase 1 の 8 機能。
- `docs/architecture.md`: `src/lsp.rs`・`src/semantic.rs` の責務に、定義と参照の索引、`name_uses`、D5 の検証、D6 の直前の成功索引。
- `_docs/feature-status.md` の G12 行と `_features/README.md` の状態欄（Phase 1 完了）。

## 受け入れ条件

- [ ] 8 機能が両エンコーディングで S1–S10 と Rust テストの期待どおりに動く。
- [ ] rename と code action は D5 の検証に通った編集だけを返し、「診断」表の拒否を返す。
- [ ] `initialize` は実装した機能のキーだけを広告し、既存キーの値は変わらない。
- [ ] 既存の LSP テスト（`tests/lsp.rs` 6 件、`session` の 2 回）と stack-depth の 3 テストが変更なしで成功する。
- [ ] 新しい crate、`unsafe`、既定の WASM import を追加していない。`vsc/` のコードは変えていない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 型付き式の span は修飾や引数を含む。関数参照は末尾 `name.len()` bytes、`TypeExprKind::Apply` は head の `Ident` を使う。`Geometry.Point.nested()` で確かめる。
- `def name :: T` と `fn name ...` の 2 か所に名前がある。A2 を忘れると、D5 がすべての関数の rename を拒否する（(3) の数が合わない）。
- or パターンの各 alternative は別の `Local` を持つ。一つにまとめないと rename が片方だけ変え、D5 が拒否する。
- lambda の捕捉が別の `Local` id を持つ場合、外側の定義と結ばないと P3 のテストが失敗する。そのときは `(name, span)` で外側の定義へ結ぶ。
- derive・computation builder・`$entry`・`$lambda` の生成名（`Provenance::Generated`）を出現にすると、rename が生成コードを書き換えようとする。
- checker に再帰や大きな局所変数を足すと 2 MiB stack のテストが落ちる。push だけにし、上限や stack を上げない。
- UTF-16 の semantic tokens の列差を bytes で計算すると S8 が UTF-16 で失敗する。必ず `PositionMapper::position` を通す。
- hover・definition に D6 を使うと既存の `stale` の `null` が変わる。D6 は completion・signatureHelp・semantic tokens だけ。
- client の `context.diagnostics` から edit を作ると、古い診断や改ざんされた診断で誤った編集を返す。サーバーの `ProjectState::diagnostics` だけを使う。
- `serde_json` の `preserve_order` feature を有効にすると `changes` の順序が変わる。Cargo の feature を変えない。
- VS Code は snippets（`vsc/snippets/tsuzuri.json`）と LSP の予約語を両方表示する。重複は許容する（D8）。

## 対象外

- Phase 2 の全項目（「Phase 分割」）、デバッグアダプター（G16）、関数の抽出などの自動リファクタリング、project root をまたぐ rename、
  ファイル名の変更（`workspace/willRenameFiles`）、call hierarchy・type hierarchy・implementation・folding range、pull 型の診断。

## 決定事項

### D1: G20 との関係と errors のある project

- 決定: G20 は Phase 1 の着手条件にしない。errors のある project では、references・rename・workspace symbol・code action は索引を使わず
  （「共通の規則」）、completion・signature help・semantic tokens は D6 を使う。可否は `ProjectState::diagnostics` の `Severity::Error` の有無で判断する。
- 理由: 1–8 は HEAD の成功時の索引だけで作れる。G20 の完了を待つと P1 の優先度の機能がすべて止まる。
- 見直し提案: `_features/README.md` と metadata の依存欄を `G07, (G20)` にする（人間が判断。このチケットでは metadata を変えない）。
- 状態: 既定案（実装者はこの案に従う）

### D2: Phase 1 の範囲と順序

- 決定: 「Phase 分割」表の 8 機能をこの順で実装する。旧版の Phase 2–4 の signatureHelp・completion・codeAction・semantic tokens を Phase 1 に入れ、
  inlay hint は Phase 2 に残す。
- 理由: 利用者の優先度の順。各機能は同じ索引の上に独立して載り、途中で止めても広告と実装が一致する。inlay hint は A15 の `copies::sites` を待つ。
- 状態: 既定案（実装者はこの案に従う）

### D3: 名前の使用位置の集め方

- 決定: 型付き木は変えない。ローカル・関数・const・型名は型付き木と `type_entry` から、フィールド・record 名・case は checker の `name_uses` から、
  関数の宣言 head は LSP 側の token 走査（A2）から集める。
- 理由: `TypedExprKind` の variant を変えると全 match の変更と stack の危険がある。取り逃しは D5 が拒否に変えるので、意味を変える rename にはならない。
- 状態: 既定案（実装者はこの案に従う）

### D4: rename の対象

- 決定: ローカル、利用者の関数、const、record、union、union case、フィールドだけ。`export` 付きの関数、std、型別名、class、method、extern、test、
  モジュールは拒否する（旧未決事項「export 名の rename」は拒否で確定）。
- 理由: export 名は ABI の一部で、エディターから黙って変えない。型別名は `resolve_type` が展開するため索引に使用位置がない。
- 見直し提案: 旧版は型別名を Phase 1 に含めていた。Phase 2 で `NameTarget::Alias`（新規）を足して対応する。
- 状態: 既定案（実装者はこの案に従う）

### D5: 意味を変えないことの検証

- 決定: rename と code action の編集は、適用後の project を再解析し、A3 の 3 条件（成功、警告の code の多重集合、出現と定義の対応の保存）を
  満たすときだけ返す。旧版の「名前以外の AST が一致」はこの条件で具体化する。
- 理由: 名前が変わるので `ast_fingerprint` は使えない。束縛の対応が同じなら、名前以外を変えない編集の意味は同じ。
- 状態: 既定案（実装者はこの案に従う）

### D6: 直前の成功索引

- 決定: project root ごとに直前の成功状態を `Session::good` に一つ持ち、completion・signature help・semantic tokens だけが `StaleMap`（A4）で写して使う。
- 理由: 入力中のソースは多くの時点で解析に失敗する。共通の接頭辞・接尾辞だけを信用すれば、写した位置は旧索引と同じ名前を指す。
- 状態: 既定案（実装者はこの案に従う）

### D7: semantic tokens の凡例

- 決定: 種類は `TOKEN_TYPES` の 11 個（`SymbolKind` から namespace・struct・enum・enumMember・property・type・interface・method・function・
  variable・parameter へ写す）、修飾子は `declaration`（宣言）・`readonly`（const）・`defaultLibrary`（std の定義）。token は名前の出現だけで、
  予約語・リテラル・コメント・演算子は TextMate 文法に任せる。
- 理由: LSP の標準名だけなら VS Code の既定の対応で色が付き、client 側の設定が要らない。
- 状態: 既定案（実装者はこの案に従う）

### D8: VS Code 拡張

- 決定: `vsc/src/extension.ts`・`vsc/package.json`・TextMate 文法・snippets は変えない。
- 理由: `vscode-languageclient` 9.0.1 は server の capability から機能を有効にする。標準の token 名なので `semanticTokenScopes` も要らない。
- 状態: 既定案（実装者はこの案に従う）

### D9: 性能の方針

- 決定: 変更ごとの全体の再解析（HEAD の `refresh`）を保ち、要求ごとの追加の解析は D5 の 1 回だけ。G17・PB06 の解析キャッシュには依存しない。
- 理由: Phase 1 を独立に出荷するため。応答時間は「性能」で記録し、G17・PB06 が改善する。
- 状態: 既定案（実装者はこの案に従う）

### D10: code action の範囲

- 決定: Phase 1 は `W1001` の `_` 前置だけ。診断はサーバーが公開したものだけを使う。
- 理由: rename の処理と D5 をそのまま使え、修正が一意で安全。ほかの修正（`E1019` など）は編集の形を検証してから Phase 2 で足す。
- 状態: 既定案（実装者はこの案に従う）

### D11: 拒否の返し方

- 決定: rename の拒否は `-32803`（RequestFailed）、引数の誤りは既存の `-32602`。整形の失敗は error ではなく `null`。
- 理由: VS Code は rename の error message を利用者に表示する。保存時の整形で構文エラーごとに通知を出さない。
- 状態: 既定案（実装者はこの案に従う）

### D12: 補完の項目

- 決定: 要求 4 の書式・群・500 件の上限。接頭辞の絞り込みと `completionItem/resolve` はしない。
- 理由: client の fuzzy 照合と整合し、応答が決定的になる。
- 状態: 既定案（実装者はこの案に従う）

### D13: signature help の表示

- 決定: label は `<name> (<p1>: <T1>) ... -> <R>`（引数なしは `<name> () -> <R>`）。引数名は `FunctionDecl::parameters` の名前、足りない分は
  `arg<i>`（1 始まり）。parameter の label は文字列で、trigger は空白と `(`、retrigger は `,`。
- 理由: 引数名が一意なので文字列 label が label 内で一意に定まり、UTF-16 の offset 計算が要らない。空白の適用がカリー化の段を表す。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-01）

Phase 1 の 8 機能（手順 1–14）を実装した。Phase 2（inlay hint など）は対象外のまま。着手時の HEAD は `1e4c2ba`（G20 は done）。

### 実装

- `src/check.rs`・`src/control.rs`: `NameTarget` と `Checker::indexing`・`name_uses`。record 名・フィールド・case・`Union.Case` の修飾を
  `note_name`／`note_case`／`note_case_path`（いずれも `#[inline(never)]`）で記録する。索引ありの解析だけが push する。
- `src/semantic.rs`: `Definition`・`Reference`・`Role`・`SymbolKind`・`LocalScope`・`receivers` と `occurrence_at`・`occurrences`。
  宣言の定義を先に作る `define_declarations` と、本体ごとの `index_function` に分けた。
- `src/lib.rs`: `analyze_inputs_semantic`（新規。`analyze_inputs_indexed_all` を呼んだ後に `SemanticIndex::retain_spelled`）。
- `src/lsp.rs`: 8 メソッド、`Session::document`・`good`（D6）・`ProjectState::diagnostics`、`StaleMap`、`function_heads`（A2）、
  `verify_edits`（D5）、`call_context`（signature help の token 走査）、`semantic_tokens`、`format_document`。
- `tests/lsp.rs`（21 件。Windows 専用の 1 件を含めると 22 件）、`src/lsp.rs` の単体テスト 3 件、`tests/lsp_sessions.mjs` の `connect`・`features`（S1–S10）。

### 決定事項への追記（チケットから外れた判断）

- `NameTarget::Local(usize)` を足した。checker は OR パターンの 2 つ目以降の alternative に 1 つ目と同じ `Local` を使うので（`match_value`）、
  「別の Local」を前提にした A1 の規則では 2 つ目の束縛名が索引から落ちる。その名前を `Declaration` の参照として記録する。
- 定数の参照は `Call(Function, [])` で、callee の span が `Span::default()` だった（`polymorph.rs` の `Checker::function`）。この形は外側の span を使う。
- 型式の参照は、宣言（record・union・型別名・const・関数・extern・class の method・instance）に加え、関数本体の `let` の型注釈・`as`・`new` の型も
  集める（本体の型注釈は hover の項目を足さない）。型別名の使用は、末尾の区切りが record／union の名前と一致しないので参照にしない（D4）。
- 索引の全参照を、ソースの該当範囲が定義名と一致するものだけに絞る（`retain_spelled`）。生成コードの span による誤った参照を除く安全策。
  `analyze_inputs_indexed_all` 内で行うと、その frame が parser の再帰の下にあるため `bounds_type_growing_polymorphic_recursion` が 2 MiB の stack で
  overflow した。別関数 `analyze_inputs_semantic` に出して解消した（上限・stack は変えていない）。
- `def`・`fn` の head（A2）は `lsp.rs` の `analyze`（`refresh` と D5 の共通処理）で索引へ足す。`function_heads` は括弧の深さ 0 の token だけを見る
  （PR #3 のレビュー対応。class・instance の `{}` 内の method が同名のモジュール関数の宣言として数えられ、参照・rename が誤った span を含んでいた）。
- completion のメンバー文脈 (b) は、union の出現が入力中で索引にないとき、同じモジュール（修飾があればそのモジュール）の同名の union を名前で引く。
  signature help の head も同様に名前で引く。どちらも別モジュールの `private` な宣言は引かない（PR #3 のレビュー対応）。
  `(` の直後のように `(` の中に head がないときは、外側の呼び出しの入力中の引数として数える（`read (|` が `read` の 0 番）。
- `scripted`（`tests/lsp.rs`）は通知も含むメッセージ列を受け取る形にした（completion の D6 の試験で `didChange` を挟むため）。

### 確認

- `cargo test --locked --test lsp`: 21 passed（レビュー対応の `stale_fallbacks_hide_private_declarations_of_other_modules` を含む。修正前の
  `src/lsp.rs` では失敗することを確かめた）。`cargo test --locked --lib lsp::`: 5 passed（新規 3 件）。
- `cargo test --locked`: 全 56 suite 成功。`cargo fmt --all -- --check`・`cargo clippy --all-targets --locked -- -D warnings`: 成功。
- stack の回帰 3 件（`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・
  `bounds_nested_builder_expansion_not_just_source_syntax`）: 各 1 passed。
- `node tests/lsp_sessions.mjs target/release/tsuzuri`: utf-16／utf-8 の既存 session と features（S1–S10）の 4 つが成功。
- 生成 IR・ランタイム・既定の WASM import は変更なし（`semantic_index_preserves_source_types_and_definitions` が索引ありとなしの IR の一致を確認）。
- 性能（合否にしない）: 200 ファイル × 50 関数の生成 project、release build、didChange 直後（`refresh` を含む）の 9 回の中央値で
  references 390 ms、rename 888 ms（D5 の再解析を含む）、completion 393 ms。VS Code の統合試験を並行して実行していた時の値で、性能の主張はしない。

### 残作業

- VS Code での目視確認（手順 14 の後半）は未実施。`vsc` の `npm test` は hover・outline・定義・診断の段まで成功し、次の
  `vscode.executeCompletionItemProvider` で応答が返らず止まった。新しい capability をすべて隠す proxy を挟んだ場合も、HEAD の拡張機能（一時的な
  worktree）でも同じ段で止まるため、この変更ではなく作業機の状態（load average 約 37、login shell の起動 15 秒で VS Code の shell 環境の解決が
  時間切れ）によると判断した。LSP の通信を記録し、サーバーがすべての要求に応答していることを確かめた。負荷の低い環境か CI で
  `npm test`・`npm run test:installed` を再実行する。
- D1 の見直し提案（依存欄を `G07, (G20)` にする）は、G20 が done になったため不要になった。
- コミットは作っていない。
