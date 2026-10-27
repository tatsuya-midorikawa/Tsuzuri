# G23 付録: アルゴリズムと Phase 3・4 の手順（付録 F）

[G23 本体](G23-diagnostic-kinds-explain-hints.md) から移した部分です。内容は本体の一部として扱う。

## F.1 アルゴリズム（擬似コード）

```text
resolve(target):
    if target matches ^[EW][0-9]{4}$ and kinds_with_code(target) is not empty: return Code(that code)
    if DiagnosticKind::from_id(target) is Some(k): return Kind(k)
    return Err(suggest(target))

suggest(target):
    limit = 2 if len(target) >= 5 else 1
    candidates = []                                   // (distance, text)
    for each id in ALL ids and each code in codes():
        d = levenshtein(target, id)                   // 挿入・削除・置換が各 1。O(len^2)、長さは 48 以下
        if d <= limit or (len(target) >= 3 and id.starts_with(target)): candidates.push((d, id))
    if target.to_ascii_uppercase() is a code and target is not that code: candidates.push((0, upper))
    sort candidates by (distance, text); remove duplicates; take the first 5

annotate(set, inputs):                                // S6 の付ける順序と条件
    lexed = BTreeMap<usize, Vec<Token>>
    for diagnostic in set.diagnostics:
        if diagnostic.help is Some or diagnostic.kind is None: continue
        index = diagnostic.span.source.unwrap_or(0)   // Diagnostics::push と同じ規則（根は 0）
        input = inputs.get(index); if input is None or input.origin != User: continue
        candidates = TABLE entries whose kind == diagnostic.kind        // BTreeMap<&str, Vec<&Hint>> を LazyLock で 1 回作る
        if candidates is empty: continue
        tokens = lexed.entry(index).or_insert_with(|| lex_all(input.text, input.edition).0)
        for hint in candidates (表の順):
            if all(hint.message_has, |s| diagnostic.message.contains(s)) and (hint.patterns.is_empty() or any pattern matches):
                apply(hint, diagnostic); break

matches(pattern, tokens, text, span):
    anchor = position of the element that starts with '@' (exactly one)
    i = index of the first token with start >= span.start
    if tokens[i].start == span.start: i is the main token else: pseudo token (only when len(pattern) == 1)
    for j, element in pattern: t = tokens[i + j - anchor] (out of range -> false); compare(element, t, text)
```


## F.2 Phase 3: ヒント DB の手順

### 手順 3.1: 照合の機構（テスト先行）

- 変更: `tests/hints.rs`（新規。単体は `src/hints.rs` の `mod tests`）、`src/hints.rs`（新規）、`src/lib.rs`（`pub mod hints;`）。
- 内容: `src/hints.rs` の `mod tests` に先に書く: `matches_a_pattern_around_the_main_token`（`". @<int>"` 相当の `[".", "@<int>"]` が `pair.0` の `0` に合い、`pair.x` の `x` に合わない）、`a_pseudo_token_needs_a_single_element_pattern`、`every_message_fragment_must_be_present`、`an_existing_help_is_kept`、`std_sources_get_no_hints`（`ModuleOrigin::Std` の診断は変わらない）、`the_first_matching_hint_wins`。次に `Hint`・`HintFix`・`annotate`・`matches`（S6）を、表を空（`TABLE = &[]`）にして書く。`annotate` の `kind` ごとの索引は `std::sync::LazyLock<BTreeMap<&'static str, Vec<&'static Hint>>>`。
- 確認: `cargo test --locked hints` で `running N tests` の N が 6 以上で通る（`cargo test <pattern>` は 0 件でも成功するので件数を見る）。

### 手順 3.2: 表の 40 件

- 変更: `tests/hints.rs`、`src/hints.rs`。
- 内容: 先に `tests/hints.rs` を書く: `every_hint_matches_its_bad_source`（各 `bad` を `analyze_modules_all(&[("Main", bad)])` に与え、`TABLE` 内の同じ項目が付けた `help`（`Hint.help`）が、当該 kind の診断のどれかに付くこと。かつ、**その診断に先に合う別の項目がない**こと＝表の順序が自分を隠さない）、`every_good_source_is_accepted`、`hint_ids_are_unique_and_kinds_are_registered`、`hint_text_follows_the_style`（`help` は 160 文字以内、小文字始まり、ピリオドで終わらない。`title`・`id` の形）、`unrelated_diagnostics_get_no_help`（`let x: i64 = true` の `type_mismatch` に `help` が付かない）。次に付録 D の 40 件を、**付録 D の順に**そのまま `TABLE` に書く（`patterns` はそのまま。`message_has` は 1 つの部分文字列を要素 1 つの配列に）。`fix` は手順 3.4。
- 確認: `cargo test --locked --test hints` が緑（`running` の件数が 5 以上）。`pattern` が実トークンと合わないヒントは、**`patterns` だけ**を直す（`help`・`kind`・`bad`・`good` は直さない）。直し方は `lex_all` のトークンを `dbg!` で出して照合する。3 回直しても合わなければ停止条件 8。

### 手順 3.3: `analyze_inputs_with` への接続

- 変更: `src/lib.rs`、`tests/hints.rs`。
- 内容: 先に `help_reaches_the_json_and_the_library_result`（`tuple-dot-index` の `bad` を CLI の `check --json` に与え、診断の JSON に `"help":"read a tuple element by pattern matching, …"` が `kind` の次に付く。キーの順序は D-51）と `hints_are_deterministic`（同じ入力で 2 回、同じ結果）を書く。`analyze_inputs_with` の中身を `analyze_inputs_unannotated`（同じ引数・同じ戻り値）に移し、`analyze_inputs_with` は結果の `Err(mut set)` に `hints::annotate(&mut set, inputs)` をかけて返す形にする。
- 確認: `cargo test --locked` 緑（`baseline_diagnostics_are_unchanged` は `help`・`fixes` を除いて比べるので緑のまま）。

### 手順 3.4: ヒントの `fix`

- 変更: `src/hints.rs`、`tests/hints.rs`。
- 内容: 先に `fix_edits_replace_only_the_main_token`（`match-fat-arrow` の `bad` に付く `fixes` が 1 件、`applicability` が `likely`、`edits` が主トークン `=>` の `span`（`start`・`end`）を `->` にする 1 件）と `a_fix_removes_its_diagnostic`（`edits` を `bad` に適用して再解析すると、同じ kind・同じ位置の診断が出ない）を書く。付録 D で `fix` を持つ 7 件（`option-type`・`float-type-name`・`string-type-name`・`match-fat-arrow`・`true-false-case`・`false-case`・`void-value`。D.1 の fix 列が `（なし）` でないもの）に `HintFix` を足す。
- 確認: `cargo test --locked --test hints` 緑。

### 手順 3.5: G21 への公開と文書

- 変更: `_tsuzuri/language-reference/compiler/diagnostics.md`（「ヒント」の節: 何が付くか・`help` が既存を上書きしないこと・`fix` は `tsuzuri fix` が既定で適用しないこと）、`docs/language.md`、`README.md`、`docs/architecture.md`（`hints.rs` の行）、`docs/stability.md`（`help` の文言は保証の対象外）。`hints::TABLE` が `pub` で、`id`・`title`・`kind`・`bad`・`good`・`help` を G21 が読めることを `tests/hints.rs::table_is_public_for_the_guide` で確かめる（外部クレートとして `tsuzuri::hints::TABLE` を読める）。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md` が通る。

## F.3 Phase 4: LSP と文書の仕上げの手順

### 手順 4.1: hover

- 変更: `tests/lsp_sessions.mjs`、`src/lsp.rs`。
- 内容: 先に `tests/lsp_sessions.mjs` に、`E1012` を出すソースを `didOpen` し、診断の範囲の位置で `textDocument/hover` を呼んで、`contents.value` が `**error[E1012]** \`use_after_move\`` で始まり `tsuzuri explain use_after_move` を含み、診断のない位置の hover が従来と同じ（`null` か型の表示）であることを検査するケースを足す（`request`・`position`・`session` を使う。手本: 同ファイルの hover のケース）。**既存の `stale` のケース**（`const stale = … position(invalid, "true")` で `result` が `null` であることを検査する）は、`true` が `E1003` の診断の位置なので応答がブロックに変わる。位置を診断の外の `position(invalid, "bad")` に変え、`null` の検査は保つ（既存の期待値を変える唯一の箇所）。次に `Session::handle` の `"textDocument/hover" | "textDocument/definition" | "textDocument/documentSymbol"` の腕で、`let offset = position_offset(…)?;` の直後・`index.at(source_id, offset)` の前に、`method.ends_with("hover")` のときだけ、`state.diagnostics` から `kind` を持ち、`span.source.unwrap_or(state.project.root) == source_id` かつ `span.start <= offset < span.end`（空のスパンは `offset == start`）を満たす最大 3 件を集めてブロックを作る（S7）。`index.at` が `None` のとき、ブロックがあればブロックだけを `contents.value`・先頭の診断の範囲を `range` にして返し、なければ従来どおり `Null`。`entry` があるときは、`markdown` の先頭にブロックと `\n\n---\n\n` を足す（`range` は従来どおり `entry.span`）。
- 確認: `node tests/lsp_sessions.mjs target/release/tsuzuri`（`cargo build --release --locked` の後）と `cargo test --locked --test lsp` が緑。

### 手順 4.2: 文書の kind 一覧と一致テスト

- 変更: `tests/explain.rs`、`_tsuzuri/language-reference/compiler/diagnostics.md`。
- 内容: 先に `the_docs_list_every_kind`: `DiagnosticKind::ALL` から S7 の表（コードの昇順、各行 `| \`E1012\` | \`a\`, \`b\` |`、kind は id の昇順）を作り、`compiler/diagnostics.md` の `<!-- generated:diagnostic-kinds -->` と `<!-- /generated:diagnostic-kinds -->` の間と**完全に一致**することを検査する（失敗メッセージに期待のブロックを出す）。次に、「エラーコード」の表の下に、見出し `### kind の一覧` と、更新の手順（「`cargo test --locked --test explain the_docs_list_every_kind` の失敗メッセージのブロックを貼る」）と、マーカーで挟んだブロックを足す。
- 確認: `cargo test --locked --test explain the_docs_list_every_kind` が緑で `running 1 test`。`node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/diagnostics.md` が通る。

### 手順 4.3: 全体の確認と文書の仕上げ

- 内容: 「ドキュメント」の一覧がすべて済み、`README.md`・`docs/language.md`・言語リファレンスの kind・`explain`・ヒントの記述が食い違わない。`cargo test --locked`・`cargo clippy --all-targets -- -D warnings`・`cargo fmt --all -- --check`・`node tests/features.mjs target/release/tsuzuri` が緑。
- 確認: 受け入れ条件の全 Phase の欄。

