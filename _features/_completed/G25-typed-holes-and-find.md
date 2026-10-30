# G25: 型つき穴 `?` と `tsuzuri find`（型でのシグネチャ検索）、未解決名の候補

| 項目 | 内容 |
| --- | --- |
| ID | G25 |
| 優先度 | P2 |
| 規模 | 全体 L（Phase 1: M、Phase 2: M、Phase 3: M、Phase 4: S）。Phase ごとに別の PR にする |
| 依存 | G22 の Phase 1（診断 v2 のデータモデル: `Detail`・`Candidate`・`ScopeEntry`・`Diagnostic::with_kind`・`with_help`・`fix`、JSON の `expected`・`candidates`・`scope`・`fixes`）。弱い依存: G24（`maybe` の fix を `tsuzuri fix --applicability maybe --dry-run` で見る。なくても動く）、G23（既存の診断の kind。G25 の新しい診断は最初から kind を持つ） |
| 後続 | G28（`kb find` は本チケットの `find::run` を呼ぶ）、G29（`tz_find`）、G21（ガイドの「関数を探す」） |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `9dd0fbbc` で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D14 はすべて承認済み |
| 改善する劣位 | Rust 比（rust-analyzer の型を手がかりにした補完）・Haskell 比（Hoogle の型検索）・Lean／Idris 比（型つき穴）: AI も人も、期待する型から使える関数・値を探す手段がなく、`unknown value 'length'` の診断は近い名前も示さない。D-50 の結論 (1)「コンパイラは事実を持つが診断・ツールに出さない」の「直す」（G22〜G25）の最後の部分 |
| 手本にする既存実装 | 式の追加（葉の式）: `ExprKind::Break`・`ExprKind::Continue` の全出現、`src/parser.rs` の `primary`・`space_argument`、`src/check.rs` の `Checker::value_expression`・`recover_expression`・`finish_recovery`。具体化と単一化: `src/polymorph.rs` の `Inference`（`fresh`・`resolve`・`unify`）・`substitute`・`Checker::function`・`Classes::holds`・`type_expression`、`src/check.rs` の `resolve_type_with_kinds`。索引の運び手: `src/semantic.rs` の `SemanticIndex`（`src/lib.rs` の `analyze_inputs_semantic`）。署名の構文解析: `src/parser.rs` の `signature`。CLI: `src/main.rs` の `api_diff_command`。テスト: `tests/diagnostics.rs`、`tests/api_diff.rs` |
| 主な影響ファイル | Phase 1: `src/syntax.rs`・`src/lexer.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/computation.rs`・`src/warnings.rs`・`src/check.rs`、新規 `tests/typed_holes.rs`、変更 `tests/modules.rs`（1 行）。Phase 2: 新規 `src/find.rs`、`src/check.rs`・`src/polymorph.rs`。Phase 3: `src/find.rs`・`src/parser.rs`・`src/semantic.rs`・`src/check.rs`・`src/main.rs`、新規 `tests/find.rs`。Phase 4: `src/find.rs`・`src/check.rs`・`src/control.rs`。文書は「ドキュメント」 |

## 方針の最新化（2026-10-30。実装前の再検討）

起票時（HEAD `a2313d19`）の前提と、G23・G28（D-60）・A18・A22 の実装後の現状が食い違う点を、実装前に次のとおり改めた（人間の包括許可による判断。D1〜D14 の趣旨は保つ）。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| 診断は `Diagnostic::new("E1030", …).with_kind("typed_hole")` | G23 の登録簿があり、診断は `Diagnostic::of(DiagnosticKind::…)` で作る | `DiagnosticKind::TypedHole`（`E1030`）・`FindBadArguments`・`FindUnknownModule`（`E2000`）を登録簿に足して `of` で出す。`explain` の `E1030` のページを足す |
| Phase 3 の `tsuzuri find` は `check` の子モジュールで型つきに照合し、`kb find` がそれを呼ぶ（D7〜D9） | D-60 で G28 の `kg find`（`src/kg/find.rs`）が先に出荷され、「G25 の `tsuzuri find` は出荷するときにこの実装を呼ぶ」と決まった（D-55 の「同じ実装」） | `tsuzuri find` は `src/main_find.rs` の前段で、照合と出力は `kg::find::search`・`render` を呼ぶ。std だけ（`KgView::new(None, StdSource::Cache)`）か、`--project DIR` でそのプロジェクトの知識グラフも。`parse_type`・`parse_type_query`・`SemanticIndex.find`・`Tier`・`Request`・`check::find::search` は作らない。結果は `kg find` と同じ（field は `ref Record -> T`、case は `Module.Union.Case`、制約は基本の型だけ確かめる、プロジェクトの非公開の宣言も出る）。問い合わせの型が読めないときは `kg find` と同じくコマンドラインの誤り（`E2000` の `find_bad_arguments`、終了コード 2。D-55 の「コマンドラインの語だけで決まる誤りは 2」）で、型名の解決の誤り（`E1004`）はない（照合が名前で行う）。`--project` のプロジェクトに誤りがあると知識グラフが不完全なので `E2009`（終了コード 1）で、本体の誤りがあっても探す仕様（付録 D.2 の 13）は採らない |
| 穴の候補は `Array.length (ref xs)`（付録 D.1 の 9） | A18 で edition 2027（マニフェストのない入力の既定）は無印の引数を借用にする。`xs` は `ref [i64]` | 候補は局所の型から作る規則のまま。edition 2026（ライブラリの `analyze_all`）は `(ref xs)`、2027 の CLI は `xs`。テストは両方を確かめる |
| 組み込み関数はすべて索引に入る | `Int.abs_diff` などは結果の型を型族（`Checker.families`）が決め、索引の型変数 `'b` は何にでも合う | 使い捨ての `Checker` に型族が残る組み込み関数は索引から外す |
| 数値リテラルの局所の型は `_` | 接尾辞のない整数リテラルの局所は既定（`i32`）を持つ開いた型。付録 D.3 の 1 は `total: i64` を期待 | `hole_type` は既定のある開いた型を既定（`i32`・`f64`）で表示する。期待は `total: i32`（言語の既定。`i64` は誤記） |
| 既存のテストへの影響は `tests/modules.rs` の 1 行（停止条件 2） | `?` を「未知の文字」の例に使うテストと、候補で変わるテストがほかにもある | 意味を保つ置き換えだけをした: `tests/diagnostics.rs`・`tests/strings.rs`・`src/hints.rs` の単体テストの未知の文字を `` ` ``（`E0001` のまま）に。ヒント `question-mark`（`c ? 1 : 2`）は `?` が穴になって `E0002 expected an expression`（`:` の位置）になったので、kind と形を付け替え、`tests/diagnostic_baseline/baseline.jsonl` のその行を新しい診断にした。`tests/diagnostic_kinds.rs` の比較は `candidates` の追加を許す。`tests/diagnostic_v2.rs` は D14（候補があれば `help` を置き換える）に合わせ、G22 の `help` は近い名前のない入力で確かめる。`src/server.rs` の単体テストは `main_find.rs` を `main_kg.rs` と同じく除く |
| Phase ごとに別の PR | AGENTS.md は 1 チケット 1 PR | 全 Phase を 1 つの PR にする |

## 実装者向け早見

- **作るもの**: (1) 式の位置の `?`／`?名前` ＝型つき穴。常に 1 件の `E1030`（kind `typed_hole`）で、期待される型・見える局所・（Phase 2 で）候補を診断に載せる。(2) 型から関数・値を探す `tsuzuri find`。(3) `E1002`・`E1004`・`E1007`・`E1020`（未解決の名前）に、近い名前の候補と `maybe` の fix。
- **最初に読む**: 「仕様」→「設計」→「実装手順」。コード: `src/check.rs` の `value_expression`・`recover_expression`・`name`・`qualified_function`、`src/polymorph.rs` の `Inference`・`substitute`・`Checker::function`、`src/parser.rs` の `primary`・`space_argument`・`signature`。
- **最初に書くテスト**: Phase 1 は `tests/typed_holes.rs` の `a_hole_is_one_error_and_does_not_cascade`（手順 2）。
- **触ってはいけない**: `TypedExprKind`（穴は型付き IR に入らない。誤りとして回復する）、所有権検査・IR の生成、`?` を `?` 演算子（エラー伝播）に使うこと（「採用しない改善」）。既存の診断のコード・位置・`message`（Phase 4 は `candidates`・`help`・`fixes` を足すだけ）。
- **停止の目印**: G22 Phase 1 が main にない（`grep -n "pub struct Candidate" src/diagnostic.rs` が 0 件）。穴を足すと既存テストが変わる（`tests/modules.rs` の 1 行を除く）。`tsuzuri fmt` が穴で「formatter would change program semantics」になる（手順 4 の規則を忘れている）。
- **Phase 1 の完了**: `?` が式の位置で読め、`tsuzuri check` が `E1030`（`expected`・`scope` つき）を 1 穴 1 件出し、連鎖する誤りを出さず、`tsuzuri fmt` が穴を保つ。
- **Phase 2 の完了**: 穴の診断に `candidates`（最大 10）が付き、自分自身は候補にならず、局所の値と引数が局所で埋まる関数が先に並ぶ。
- **Phase 3 の完了**: `tsuzuri find 'ref [i64] -> i64'` が `Array.length`・`Array.sum` を `exact` で出す。std だけでも動く。
- **Phase 4 の完了**: `unknown value 'cnt'` などの 6 つの診断に `candidates` と（候補が 1 つに決まるとき）`maybe` の fix が付く。
- **新しい名前**: 診断 `E1030`（kind `typed_hole`）、サブコマンド `find`、フラグ `--project`・`--result`・`--takes`・`--module`・`--limit`（`--json` は既存）。`E2000` の kind 2 個（`find_bad_arguments`・`find_unknown_module`）。D-55 の名前（`E1030`・`find`）は動かさない。`find` の `--project` 以下のフラグと kind は本チケットが決めた（GUIDE の台帳への追記を報告する）。

## 目的

型は、AI が「次に何を書くか」を絞る最も強い手がかりだが、今のコンパイラは「ここに `i64` が要る」という事実を診断に出さず、探索の手段もない。本チケットは次を満たす。

1. **穴**: 書きかけの式に `?`（または `?名前`）を置くと、コンパイラが「ここに入る型・見える局所・使える候補」を返す。穴は 1 件の誤りで、後続の推論に連鎖する誤りを出さない。
2. **検索**: `tsuzuri find` は、型（`ref [i64] -> i64`）から std と利用者の関数・値・field・case を探す。結果は決定的で、`ref` の有無の違いと、引数の数の違いを段階（`exact`／`borrow`／`partial`）で分ける。
3. **未解決名**: `unknown value 'cnt'` に、近い名前（編集距離）と、修飾すれば解決する名前（`length` → `Array.length`）の候補を付け、1 つに決まるときは `maybe` の fix を付ける。

HEAD で測った数字: `?` は字句の段階で `E0001`（`unexpected character '?'; identifiers use ASCII`）。標準ライブラリは 47 モジュール・374,185 bytes、`def`／`export def`／`private def` の行が 815（`grep -c "^def \|^export def \|^private def " std/*.tz` の合計）。組み込み関数は `Builtin::ALL` の 246 項目（`awk '/pub const ALL: &.static \[Self\] = &\[/{f=1;next} f&&/^    \];/{exit} f' src/check.rs | grep -c "Self::"` で数える。`#[cfg(test)]` の 3 項目を除くと 243）で、署名の表がなく、型は `Checker::builtin` が呼び出しごとに作る。名前の接頭辞ごとの組み込み関数の数は `grep -c 'Self::[A-Za-z0-9]* => "Math\.' src/check.rs` のように数える（`Math` 32・`Int` 29・`Simd` 20・`Vec` 17）。未解決名の診断は 4 つのコードに分かれる（「現状」2）。

**非目標**: `?` を `?` 演算子（エラー伝播）にすること。穴を実行時に使うこと（穴は常に誤り。コード生成に進まない）。型クラスのメソッド（`Display.display` のようなもの）の検索（索引に入れない。対象外）。証明（候補はヒントで、`check` が最終の判定）。

## 着手条件と停止条件

### 着手条件

- G22 の Phase 1 がマージ済み: `grep -n "pub struct Candidate" src/diagnostic.rs`・`grep -n "pub struct ScopeEntry" src/diagnostic.rs`・`grep -n "pub struct Detail" src/diagnostic.rs` が各 1 件。
- GUIDE §2.3 の基準（`cargo build --release --locked`・`cargo test --locked`）が通る。件数を控える（手順 1）。
- 読むもの: GUIDE の D-51・D-55、GUIDE §6.1・§6.2・§6.9・§11.1、`src/check.rs` の `Checker::expression`〜`finish_recovery`。承認は不要（新しい crate・`unsafe`・予約語・既定の WASM の import はない）。

### 停止条件

1. G22 Phase 1 が main にない。選択肢: A（推奨）G22 Phase 1 を先に実装する。B 本チケットで D-51 の型だけを足す（G22 と衝突するので、人間が許したときだけ）。
2. 穴を足すと、`tests/modules.rs` の 1 行以外の既存テストの期待値が変わる。選択肢: A 変更を戻して原因（字句・構文の分岐）を探す。
3. `TypedExprKind` に新しい variant が要る（穴が誤りとして回復しない箇所が見つかった）。選択肢: A その箇所の回復を直す（`recover_expression` を通す）。B 人間に報告する。**型付き IR に穴を足さない**。
4. 候補の探索が、1 つの穴で 1 秒より長くかかる（debug ビルドで 0.2 秒の検査に対して。付録 E）。選択肢: `MAX_ENTRIES_EXAMINED` を下げる。上限を黙って上げない。
5. 使い捨ての `Checker` で組み込み関数の型を作る（`Checker::builtin`）と panic する、または索引の構築が 50 ms（debug）より長くかかる（試作は 192 件で 5〜7 ms）。選択肢: panic する組み込み関数だけを索引から外す（文書に書く）。`names`・`signatures` は共有の参照なので、持ち主の `Checker` の状態が変わることはない。変わると見えたら停止して報告する。
6. `Classes::holds` が、利用者の `instance` を持つ型で panic する・誤る。選択肢: 制約の判定を、組み込みのクラスだけに限る。
7. 新しい予約語・crate・`unsafe`・既定の WASM の import・`?` 演算子が必要になった。同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

作業ディレクトリ `$WORK`（例: `/tmp/tz-G25`）に入力を作り、`tsuzuri` は `target/release/tsuzuri`。診断は標準エラーに出る。

**1. `?` は未知の文字**。`let x: i64 = ?` / `x` を `tsuzuri check q1 --json` すると（`?name`・`1 ? 2` も同じ）:

```text
{"severity":"error","code":"E0001","message":"unexpected character '?'; identifiers use ASCII","path":"q1/Main.tz","span":{"start":13,"end":14,"line":1,"column":14,"end_line":1,"end_column":15}}
```

字句の表は `src/lexer.rs` の `Lexer::symbol`（`'#' => Hash` の隣）で、`?` の腕がない。

**2. 未解決の名前の診断**（実測。`check <dir> --json` の `code`・`message`・主スパン）:

| 入力 | コード | message | 主スパン | 発行箇所（`src/`） |
| --- | --- | --- | --- | --- |
| `let y = cnt + 1`（`count` がある） | `E1002` | `unknown value 'cnt'` | 名前 | `check.rs` の `Checker::name` |
| `Array.lenght (ref xs)` | `E1002` | `module 'Array' has no function or union case 'lenght'` | メンバー名だけ（`lenght`） | `check.rs` の `value_expression`（`has no function or union case`） |
| `Aray.length (ref xs)` | `E1002` | `unknown value 'Aray'` | モジュール名 | 同上（`Checker::name`） |
| `a.b` の修飾した関数 | `E1002` | `unknown function '{name}'`（`{hint}` つき） | 名前 | `check.rs` の `Checker::qualified_function` |
| `let x: Strng = "a"`・`Pointt { x: 1 }` | `E1004` | `unknown record, union, or type alias 'Strng'` | 名前 | `check.rs`（`unknown record, union, or type alias` の 2 か所と、`unknown type class, record type, or union type`） |
| `p.zz` | `E1007` | `unknown field 'zz'` | field 名 | `check.rs`（`unknown field`） |
| `\| Circl r -> r` | `E1020` | `unknown union case or active pattern 'Circl'` | 名前 | `control.rs`（`unknown union case or active pattern`） |

**設計の種は未解決名を `E1004` と書いていたが、HEAD では値・関数・モジュールのメンバーは `E1002`、型は `E1004`、field は `E1007`、case は `E1020`**。本チケットの Phase 4 は上の 4 コード・6 つの診断を対象にする。メッセージ・コード・位置は変えない。

**3. 式の誤りの回復**。関数の本体は常に `checker.recovering = true` で検査される（`grep -n "checker.recovering = true" src/check.rs`）。`Checker::expression` が `Err` を返すと、`finish_recovery` → `recover_expression` が、誤りを `recovered` に積み、`TypedExpr::error(span)`（型 `Type::Error`）を返す。`Type::Error` は何とでも単一化でき、連鎖する誤りを出さない。したがって、穴は `value_expression` の 1 つの腕が `Err(E1030)` を返すだけでよく、型付き IR の variant は要らない（設計の種の `TypedExprKind::Hole` は不要）。

**4. 期待される型（`expected`）が分かる位置**（Phase 1 の試作を `cargo build` した複製で実測。付録 A の表）: 注釈つきの `let`・関数本体の末尾・呼び出しの引数・二項演算の右辺・`if` の枝・`match` の腕・record の field・注釈つきの配列の要素は分かり、注釈のない `let`・タプルの要素・無名関数の本体・`? && x` の左辺は分からない（`_`）。未確定の型は `Type::display` が `an undetermined type`、誤りの型が `an erroneous type` と書くので、診断では `_` に置き換える。見える局所は `self.scopes`（`Vec<BTreeMap<String, Local>>`）にあり、`match` の主語などコンパイラが作った局所（名前が `$match39` のように `$` で始まる）が混じる。

**5. 索引の材料**。`Names.functions: BTreeMap<String, NameInfo>`（`NameInfo { id, name, module, visibility }`）と `signatures: Vec<Scheme>`（`Scheme { signature: Signature { parameters, result }, variables, constraints, members }`）が、std と利用者のすべての宣言された関数の型を持つ。`Checker::function(id)` が `Scheme` の変数を新しい推論変数に置き換えて具体化する（`substitute`）。組み込み関数は `Builtin::ALL`（246）で、型は `Checker::builtin(builtin, span)` が返す（内部用のものは `E1022` で拒否する）。record の field は `TypeContext::record_fields`、union の case は `TypeContext::union_payloads`。`Classes::holds(name, ty, types)` は具体的な型がクラスを満たすかを返す。

**6. `fmt`**。`?` を読めるようにしただけでは `tsuzuri fmt` が「`formatter would change program semantics; please report a bug`」で失敗する（`?x` の `?` と `x` の間に空白を入れてしまい、穴の名前が消えるため。試作で実測）。`src/formatter.rs` の記号の間隔の規則（`matches!(previous.kind, At | Hash)` の行）に `Question` を足すと、`?x` は保たれ、`f   ?` は `f ?` に整形される。

**7. 既存テスト**: `tests/modules.rs`（`keeps_source_identity_for_lexing_parsing_types_and_recursive_layouts`）は字句の誤りの例に `fn value() -> i64 { ? }`（`E0001`）を使っている。穴を足すとこの入力は `E1030` になるので、字句の誤りの例を別の文字（バッククォート。`E0001` を実測）に変える。

## 変えてはいけない意味

- `?` を含まない入力の受理・拒否・診断・生成 IR は、1 byte も変わらない。`?` を含む入力は、以前 `E0001`（字句）で拒否され、今後は構文か `E1030` で拒否される（どちらも誤り。受理するプログラムは増えない）。
- 型付き IR（`TypedExprKind`）・所有権・コード生成・ABI・WASM の import。穴を含むプログラムは必ず `E1030` で止まり、コード生成に進まない。
- `E1002`・`E1004`・`E1007`・`E1020` のコード・message・主スパン（Phase 4 は `candidates`・`help`・`fixes` を足すだけ。G22 の「足すキーを持たない診断の JSON は 1 byte も変わらない」を守る）。
- `?` は `?` 演算子ではない（式の位置の値としてだけ）。予約語は足さない。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `?`・`?名前`（字句・構文・整形・符号化）と `E1030`（`expected`・`scope`） | M | G22 Phase 1 | 穴を書かない入力は変わらない。`candidates` なしでも診断は有用 |
| 2 | 穴の `candidates`（`src/find.rs` の探索と順位づけ）と `maybe` の fix | M | Phase 1 | 診断にキーが増えるだけ。探索が失敗・上限超過でも診断は Phase 1 のまま |
| 3 | `tsuzuri find`（型の検索）。組み込み関数の署名の索引 | M | Phase 2 の `find.rs` | 新しいサブコマンドだけ |
| 4 | 未解決名（`E1002`・`E1004`・`E1007`・`E1020`）の候補と `maybe` の fix | S | Phase 2（`src/find.rs` があること。Phase 3 には依存しない） | 既存の診断にキーが増えるだけ |

## 仕様

### Phase 1: 型つき穴 `?`

**構文**（`Parser::primary` の腕。式の位置だけ）:

```
hole = "?" [ ident ]     ident は `?` の直後（空白なし）の識別子。キーワードは名前にならない
```

| 入力 | 読み方 |
| --- | --- |
| `?` | 名前なしの穴 |
| `?foo`（空白なし） | 名前つきの穴 `foo`（名前は診断の `message` に出るだけ。ログ用） |
| `? foo`・`? 1`・`?(x)` | 名前なしの穴に引数を渡す式（試作で確認）。呼び出しの関数の位置の穴で、`expected` は `_`（付録 A.1 の行 27）。`foo` などの引数も検査される（未知の名前は `E1002`）。`?1`（空白なしの数）は `E0002` |
| `f ? x` | `f` に引数 2 つ（`?` と `x`） |
| `f ?x ?y` | 引数 2 つ（`Parser::space_argument` が `Question` を引数の先頭にする） |
| パターン・型・宣言の位置の `?` | 構文の誤り（既存の `E0002`。穴は式だけ） |

字句は `TokenKind::Question`（`Lexer::symbol` の `'?' => Question`）。**予約語は足さない**。全 edition が受理する（受理を増やす拡張）。AST は `ExprKind::Hole(Option<Ident>)`（補間の穴 `InterpolationHole`・`TypedHole` とは別の物。doc コメントで区別する）。

**検査**: `Checker::value_expression` の `ExprKind::Hole(name)` の腕が `Err(E1030)` を返す。回復（`finish_recovery`）が `TypedExpr::error` にするので、穴は 1 件の誤りで、型付き IR にも所有権にもコード生成にも進まない。穴の値は `Type::Error` で、使った先に連鎖する誤りは出ない。

| 項目 | 規則 |
| --- | --- |
| コード・kind | `E1030`・`typed_hole`。1 つの穴に 1 件 |
| 主スパン | `?`（名前つきは `?名前` の全体） |
| `message` | 型が分かる: `typed hole {label} of type '{T}'; replace it with an expression of that type`。分からない: `typed hole {label} of unknown type; replace it with an expression`。`{label}` は `'?'` か `'?foo'` |
| `help` | 分かる: `fill the hole with a value of type '{T}'`。分からない: `fill the hole with an expression; its type is not known here` |
| `expected`（`Detail`） | 分かる: `{T}`。分からない: `_` |
| `scope`（`Detail`） | 下の規則。最大 32 件 |
| `candidates`・`related`・`fixes` | Phase 1 はなし（Phase 2 が `candidates` と `maybe` の fix） |

**「型が分かる」**: `expected` が `Some` で、解決後（`self.inference.resolve`）が `Type::Infer(_)` 1 つだけではなく、`contains_error()` でもない。一部に未確定の型を含んでよい（`ref [_]`）。

**型の表示 `hole_type`**: 解決した型の `Infer` と `Error` を `Type::Handle("_".into())` に置き換え（`polymorph::map_type`。`pub(super)` にする）、`Type::display(&self.types)` で文字列にする。`an undetermined type`・`an erroneous type` は出さない。

**`scope` の規則**: (1) `self.scopes` の全フレームの局所のうち、`provenance == Provenance::User`、名前が `$` で始まらず、より内側のフレームの同名に隠されていないもの。(2) 型が分かるとき、型が期待型と単一化できる局所を先、残りを後に並べる（`Inference::default()` を新しく作り、`Infer`・`Error` を新しい変数に置き換えて `unify` の成否だけを見る）。各群は `Local.id` の昇順（宣言の順）。分からないときは全部を `Local.id` の昇順。(3) 先頭から 32 件で打ち切る。(4) 各要素は `ScopeEntry { name, ty: hole_type(local.ty) }`。

**期待される型が分かる位置**（実測。全表は付録 A）: 注釈つきの `let`・関数本体の末尾・呼び出しの引数・二項演算の右辺（左が既知）・`if` の枝・`match` の腕・record の field・注釈つきの配列の要素。分からない位置（`_`）: 注釈のない `let`・タプルの要素・無名関数の本体・`? && x` の左辺。この表は `Checker::expression` が渡す `expected` の結果で、本チケットは `expected` を渡す規則を変えない。

**例 1**（想定出力。`def f :: i64 -> i64 = \n -> n + ?` / `f 1`。1 行目）:

```text
{"severity":"error","code":"E1030","message":"typed hole '?' of type 'i64'; replace it with an expression of that type","path":"h3/Main.tz","span":{"start":32,"end":33,"line":1,"column":33,"end_line":1,"end_column":34},"kind":"typed_hole","help":"fill the hole with a value of type 'i64'","expected":"i64","scope":[{"name":"n","type":"i64"}]}
```

**例 2**（想定出力。`let x = ?foo` / `x`）: 型が分からない。

```text
{"severity":"error","code":"E1030","message":"typed hole '?foo' of unknown type; replace it with an expression","path":"h2/Main.tz","span":{"start":8,"end":12,"line":1,"column":9,"end_line":1,"end_column":13},"kind":"typed_hole","help":"fill the hole with an expression; its type is not known here","expected":"_"}
```

**例 3**（想定出力。人間向け。関数 `f` に穴 2 つと別の誤り 1 つ。穴の連鎖は出ない。D-51 の表示）:

```text
Main.tz:2:18: error[E1030]: typed hole '?' of type 'i64'; replace it with an expression of that type
  2 |     let a: i64 = ?
    |                  ^
  help: fill the hole with a value of type 'i64'

Main.tz:3:19: error[E1003]: expected bool, found i32
  3 |     let b: bool = 1
    |                   ^

Main.tz:4:17: error[E1030]: typed hole '?' of unknown type; replace it with an expression
  4 |     let c = a + ?
    |                 ^
  help: fill the hole with an expression; its type is not known here
```

### Phase 2: 穴の候補

**索引（`Vec<find::Entry>`。`Names.search_index` に最初の穴で 1 回だけ作る。`tsuzuri find` も同じ `find::build` を使う。作り方は設計）**。1 つの `Entry` は、宣言の 1 つの型つきの形を持つ。

| `kind` | 元 | `name` | `parameters` → `result` |
| --- | --- | --- | --- |
| `def` | `Names.functions` の関数（`names.constants` にない。名前に `$`・`__` を含まず、`Scheme` が poisoned でない） | `declaration_display(key)` | `Scheme.signature` |
| `const` | `names.constants` の関数（引数なし） | 同上 | `[]` → 結果 |
| `builtin` | `Builtin::ALL`（名前に `$`・`__` を含まず、`Checker::builtin` が `Ok` のもの） | `Builtin::name()` | `Checker::builtin` が返す型。`Infer` を `'a`・`'b`… に置き換える |
| `field` | `names.records` の各 record の各 field | `{type_display(key)}.{field}` | `[Record<'p…>]` → field の型（`value.field` と書く） |
| `case` | `names.cases` の各 case | `declaration_display(key)` | payload があれば `[payload]`、なければ `[]` → `Union<'p…>` |

`Entry` は、持ち主のモジュール、公開か（`public`）、`ModuleOrigin`、関数 id（`def`・`const` だけ）、型変数、制約（`(class, 型)` の並び）、署名の文字列 `text`（`Numeric<'a> => ref ['a] -> 'a`。制約がなければ `=>` を付けない）も持つ。見えるか（`public` または持ち主のモジュールが依頼者と同じ）は、索引ではなく使うときに判定する。

**候補の元**（`find::hole_candidates`）。期待型が「分かる」ときだけ探す。

| 元 | `how` | テキスト |
| --- | --- | --- |
| 局所の値（`scope` の全部） | `value` | 名前 |
| 局所の record の field（深さ 1） | `value` | `local.field` |
| 索引の `Entry`（見えるもの。**いま検査している関数自身を除く**） | 引数なし `value`、あり `call` | `名前` または `名前 引数…`（引数は下の規則） |

**`Entry` の候補**: 探索ごとに新しい `Inference::default()` を作り、`Entry` の型変数を新しい変数に置き換え（`polymorph::substitute`）、結果の型を期待型（`Infer`・`Error` を新しい変数に置き換えた写し）と `unify` する。通らない `Entry` は捨てる。

**引数の充填**（引数の型 `P` ごとに、`scope` の順で最初に合う局所。`P` は充填の途中の単一化の結果で解決した型）:

| `P` | 局所の型 | 書く文字列 |
| --- | --- | --- |
| `ref T`／`ref mut T` | 参照型ではなく、`T` と単一化できる（`ref mut` は局所が `let mut` のときだけ） | `(ref x)`／`(ref mut x)` |
| `ref T`／`ref mut T` | 同じ種類の参照型で、中身が `T` と単一化できる | `x` |
| それ以外 | `P` と単一化できる | `x` |
| 合う局所がない | — | `?` |

型が未確定（解決しても `Type::Infer` 1 つだけ。注釈のない整数リテラルの `let` の局所がその例）の局所は、引数を埋めるのに使わない（何にでも単一化できて偽の候補が増えるため。順位 0 の局所の値としては候補になる）。

合った局所の単一化は**取り込む**（次の引数は取り込んだ後の型で調べる。同じ局所を複数の引数に使ってよい）。取り込んだ後に `Entry` の制約（下）が成り立たなくなる局所は、合わなかったものとして次の局所を調べる。

**順位**（小さいほど先）:

| 順位 | 内容 |
| --- | --- |
| 0 | 局所の値 |
| 1 | `local.field` |
| 2 | 引数が 1 つ以上あり、すべて局所で埋まる |
| 3 | 引数の一部が局所で埋まる（残りは `?`） |
| 4 | 引数なし（定数・引数なしの case・引数なしの組み込み値） |
| 5 | 引数があり、どれも埋まらない（すべて `?`） |

同じ順位の中は、(1) `Entry` の宣言上の結果の型が型変数 1 つだけのもの（`'a`）を後に、(2) `?` の数の昇順、(3) `name`、(4) テキスト。テキストが同じ候補は 1 つにし、最大 10 件（D-51）。`Candidate { name: テキスト, signature, how }`。`signature` は `Entry` なら `{name} :: {text}`、局所なら `{名前}: {型}`、field は `{local}.{field}: {型}`。

**制約**: `Entry` の各制約の型が、探索の `Inference` で解決した後に、変数・`Infer`・`Error` を含まないとき、`Classes::holds_class(class, 型, types)` が真でなければ候補を捨てる（`Float<i64>` の `Array.sum_pairwise` を除く）。変数が残るものは調べない。

**`maybe` の fix**: 順位 0 の候補がちょうど 1 件のとき、`Fix { title: "Replace '?' with '{x}'", applicability: Maybe, edits: [Edit { span: 穴の span, text: x }] }` を付ける（名前つきの穴は title の `'?'` を `'?foo'` にする）。2 件以上のときは付けない。

**上限**: 1 つの関数（`Checker`）で探索する穴は先頭の 16 個（`MAX_HOLE_SEARCHES`。残りは `candidates` なし）。1 回の探索で調べる `Entry` は 5,000（`MAX_ENTRIES_EXAMINED`）。1 つのテキストは 200 文字まで（超えたら捨てる）。探索は誤りにならない（失敗しても Phase 1 の診断のまま）。

**例 4**（実測。試作の出力を整えたもの。`def f :: [i64] -> i64 = \xs -> ?` / `f [1, 2]`）。`candidates` の先頭 4 件と `scope`:

```text
"expected":"i64","candidates":[{"name":"Array.length (ref xs)","signature":"Array.length :: ref ['a] -> i64","how":"call"},{"name":"Array.product (ref xs)","signature":"Array.product :: Numeric<'a> => ref ['a] -> 'a","how":"call"},{"name":"Array.sum (ref xs)","signature":"Array.sum :: Numeric<'a> => ref ['a] -> 'a","how":"call"},{"name":"Parallel.sum (ref xs)","signature":"Parallel.sum :: Numeric<'a>, Add<'a>, Copy<'a>, Send<'a> => ref ['a] -> 'a","how":"call"}, ...],"scope":[{"name":"xs","type":"[i64]"}]
```

自分自身（`Main.f xs`）は候補に出ない。`Array.sum_pairwise (ref xs)` は `Float<i64>` を満たさないので出ない。`Int.count_ones xs` は `Integer<[i64]>` が成り立たないので、引数が `?` になる（順位 5）。

**例 5**（実測。`union Shape = Circle of f64 | Square of f64 | Empty` / `let s: Shape = ?`。局所なし）: `candidates` の名前は `Main.Empty`（順位 4）、`Main.Circle ?`・`Main.Square ?`（順位 5。結果の型が具体）の順で、`Array.fold ? ? ?` のような結果が型変数だけの関数は、これらの後に並ぶ。

### Phase 3: `tsuzuri find`

```text
tsuzuri find [--project DIR] [--module NAME] [--limit N] [--json] QUERY
tsuzuri find [--project DIR] [--module NAME] [--limit N] [--json] [--result TYPE] [--takes TYPE]...
```

| 引数 | 規則 |
| --- | --- |
| `QUERY` | 型の文字列 1 つ（シェルでは引用符で囲む）。`--result`・`--takes` とは同時に使えない |
| `--result TYPE` | 結果の型だけを指定する（引数は問わない）。1 回まで |
| `--takes TYPE` | 引数のどこかにある型（順序は問わない）。何回でも書ける |
| `--module NAME` | `Array`・`Geometry::Point` のように、持ち主のモジュールの表示名か、その最後の部分が `NAME` に等しい `Entry` だけ（組み込み関数は名前の `.` より前） |
| `--limit N` | 表示する件数。1〜100、既定 20 |
| `--project DIR` | `DIR` のプロジェクトの宣言も探す（`Project::load_with_overlays`）。省略すると std だけ（埋め込みの 47 モジュールのすべて。D-40 の opt-in モジュールも含む） |
| `--json` | 結果を 1 行 1 JSON で標準出力に出す |

**型の書き方**: 宣言の署名と同じ型の文法（`ref [i64] -> i64`・`(ref 'a -> bool) -> ref ['a] -> i64`・`Maybe<'a>`）に加え、`_` は任意の型（出現ごとに別の型）、`'a` は型変数（同じ名前は同じ型）。制約（`=>`）は書けない（構文の誤り）。型名は `Main` の中で書いたように解決する（std の型は `Maybe`・`Json.Value`、別モジュールの利用者の型は `Geometry::Point`）。`i64` だけの問い合わせは「引数なしの `i64`」を意味し、引数ありの関数は `partial` で出る。

**照合**。`Entry` の型を `(P1…Pn) → R`、問い合わせを `(Q1…Qm) → S` とする。両方の型変数と `_` を新しい推論変数に置き換えて単一化する（どちらの変数も束縛される。`'a -> 'a` は `i64 -> i64` にも `string -> string` にも合う）。単一化の後、`Entry` の制約のうち具体的な型になったものは `holds_class` が真でなければならない。

| 段 | 条件（上の段が成り立てば下の段は調べない） |
| --- | --- |
| `exact` | `n == m`。すべての `Pi ~ Qi` と `R ~ S` がそのまま単一化できる |
| `borrow` | `n == m`。各型の外側の `ref`／`ref mut` を 1 つずつ外した型が単一化できる |
| `partial` | `n >= m`。`R ~ S`（`ref` を外す）が単一化でき、各 `Qi`（`ref` を外す）が互いに別の `Pj` に、`j` の昇順で最初に単一化できるものと対応する。余る引数は `n - m` 個（`n == m` は引数の順序違い） |

`--result`・`--takes` の問い合わせは `exact`（`ref` を外さない）か `borrow`（外す）の 2 段だけ。結果を指定しなければ結果は問わず、`--takes` がなければ引数は問わない。

**並び順**: `(段, 余る引数の数, name, kind)`。`kind` の順は `def`・`const`・`builtin`・`field`・`case`。同じ `(name, kind)` は 1 件にする。`--limit` で切る前の全件を数える。`--module` と見える範囲（`--project` がなければ利用者の宣言はない。std の非公開の宣言は見えない）で絞った `Entry` の数を `searched` とする。

**出力**（人間向け。標準出力。1 件 1 行）: `{tier:<7} {name} :: {text}`。`partial` で余りが 1 つ以上なら行末に `  (+{n})`。0 件のときは `no matches` の 1 行（終了コード 0）。`--limit` で切ったときは最後に `... {n} more; raise --limit (at most 100)`。

**例 6**（実測。試作の出力。`tsuzuri find 'ref [i64] -> i64'` の先頭と末尾）:

```text
exact   Array.length :: ref ['a] -> i64
exact   Array.product :: Numeric<'a> => ref ['a] -> 'a
exact   Array.sum :: Numeric<'a> => ref ['a] -> 'a
exact   Parallel.sum :: Numeric<'a>, Add<'a>, Copy<'a>, Send<'a> => ref ['a] -> 'a
partial Array.at :: i64 -> ref ['a] -> ref 'a  (+1)
partial Array.count :: (ref 'a -> bool) -> ref ['a] -> i64  (+1)
...
partial Array.fold_ref :: ('state -> ref 'a -> 'state) -> 'state -> ref ['a] -> 'state  (+2)
partial Parallel.reduce :: Copy<'a>, Send<'a> => 'a -> ('a -> 'a -> 'a) -> ref ['a] -> 'a  (+2)
```

全体は 16 件（`exact` 4・`partial` 12）。`Array.sum_pairwise` は `Float<i64>` を満たさないので出ない。`tsuzuri find 'string -> i64'` の先頭は `borrow  String.char_count :: ref string -> i64` と `borrow  String.length :: ref string -> i64`（全体 8 件）。`tsuzuri find 'ref string -> string -> bool'` は `String.contains`・`String.ends_with`・`String.starts_with` の 3 件で、いずれも `borrow`。

**JSON**（1 件 1 行。キーの順は固定）:

```json
{"event":"result","tier":"exact","kind":"def","name":"Array.length","module":"Array","origin":"std","signature":"ref ['a] -> i64"}
{"event":"summary","query":"ref [i64] -> i64","matches":16,"shown":16,"limit":20,"searched":981}
```

`signature` は `Entry.text`（制約つき）。文字列のエスケープは `diagnostic::json_string`。`extra` は `partial` のときだけ（`"extra":1`）。`origin` は `std` か `user`。`summary.query` は QUERY の文字列、または `--result T --takes U` を連ねた文字列。0 件の JSON は `summary` の 1 行だけ（`matches` が 0）。`--limit` で切ったとき、JSON は `shown` が `matches` より小さい（`... more` の行は人間向けだけ）。`searched` の 981 は std のすべての公開された宣言の数（`def` 559・`builtin` 192・`case` 123・`field` 107。試作で数えた。付録 E）。

**終了コードと誤り**:

| 状況 | 出力先・内容 | 終了コード |
| --- | --- | --- |
| 1 件以上、または 0 件 | 標準出力 | 0 |
| 問い合わせの構文・型名の誤り | 通常の診断（`E0002`・`E1004`）を標準エラーに出す。パスは `<query>`、本文は問い合わせ | 1 |
| `--project` が読めない・宣言の段階の誤りで索引ができない | `check` と同じ診断を標準エラーに出す | 1 |
| 使い方の誤り | `E2000`（下の表）を標準エラーに出す | 2 |

使い方の誤り（`E2000`。`Span::default()`・パス `<command line>`。G24 の `fix_bad_arguments` と同じ作り）:

| 状況 | message | kind |
| --- | --- | --- |
| `QUERY` も `--result`・`--takes` もない、両方ある、`QUERY` が 2 つ以上 | `find takes one type query, or --result and --takes; usage: tsuzuri find [--project DIR] [--module NAME] [--limit N] [--json] QUERY` | `find_bad_arguments` |
| 未知のオプション（`-` で始まる語） | `unknown option '{option}' for find; supported: --project, --module, --limit, --result, --takes, --json` | `find_bad_arguments` |
| 値がない | `{option} needs a value` | `find_bad_arguments` |
| `--limit` が 1〜100 の整数でない | `--limit needs a number from 1 to 100` | `find_bad_arguments` |
| `--takes` 以外の同じオプションが 2 回 | `{option} specified more than once` | `find_bad_arguments` |
| `--module` の名前のモジュールがない | `no loaded module is named '{name}'` | `find_unknown_module` |

### Phase 4: 未解決名の候補

対象は、名前が解決できない 6 つの診断（コード・message・主スパンは変えない。発行箇所と比べる名前と候補の元の全表は付録 C）:

| # | 診断 | 比べる名前 | 候補の元（`pool`） |
| --- | --- | --- | --- |
| 1 | `E1002 unknown value 'n'` | `n` | 局所（利用者のもの。`$` で始まらない）・同じモジュールの関数と定数・修飾なしの組み込み関数・case の別名。`n` が大文字で始まるときはモジュール名（`names.modules` のキーの最後の部分）も。**修飾すると解決する名前**（他のモジュールの関数・case・組み込み関数で、最後の部分が `n` に等しいもの）は `Array.length` のように修飾して入れる |
| 2 | `E1002 module 'M' has no function or union case 'x'` | `x` | `M` の関数・case・組み込み関数の名前（修飾なし） |
| 3 | `E1002 unknown function 'n'` | `n` 全体 | 見える関数・case の `declaration_display` |
| 4 | `E1004 unknown record, union, or type alias 'n'` | `n` | record・union・型別名・handle の名前（別名表のキー）と primitive の型名（`numeric::NUMERIC_NAMES` と `bool`・`unit`・`string`・`utf8string`・`char`・`utf8char`・`bigint`） |
| 5 | `E1007 unknown field 'f'`・`record 'R' has no field 'f'`（3 か所） | `f` | その record の field 名 |
| 6 | `E1020 unknown union case or active pattern 'c'` | `c` | case の別名・active pattern の別名 |

**候補の選び方**（`find::suggest(target, pool)`。付録 B の検証済みのコードをそのまま使う）: 距離は Damerau-Levenshtein（挿入・削除・置換・隣の 2 文字の入れ替えが各 1）。`target` が 4 文字以下なら 1 以下、5 文字以上なら 2 以下を「近い」とする。`target` が 2 文字未満なら近い名前は探さない。修飾すると解決する名前は距離 0。`target` と同じ綴りの `text` は捨てる。並び順は `(距離, group, text)`（`group` は局所 0・関数と定数 1・case 2・module 3・型 4）。同じ `text` は 1 つ、最大 10 件。

**診断への付け方**（候補が 1 件以上のときだけ。0 件なら診断は変えない）:

| 項目 | 内容 |
| --- | --- |
| `candidates` | `Candidate { name: text, signature, how }`。`how`: 局所は `local`、距離 0 の修飾名は `qualify`、それ以外は `similar` |
| `signature` | 局所は `{名前}: {型}`。そのほかは種類の語: `function`・`constant`・`builtin`・`union case`・`field`・`module`・`type`・`active pattern` |
| `help` | 候補が 1 件、または先頭の `(距離, group)` が 2 番目より小さい: `did you mean '{先頭}'?`。そうでなければ `did you mean one of: '{1 番目}', '{2 番目}', '{3 番目}'?`（最大 3 つ）。G22 Phase 3 の `help` があれば置き換える（`with_help` は後勝ち。`scope` は変えない） |
| `fixes` | 上の「先頭が 1 つに決まる」とき 1 件: `Fix { title: "Replace '{target}' with '{先頭}'", applicability: Maybe, edits: [Edit { span: 置換先の span, text: 先頭 }] }`。置換先は、1・3・4・5・6 は名前の span、2 は `x`（メンバー名）の span。名前が `.` や `::` を含む（修飾名）診断 4 は fix を付けない |

**例 7**（想定出力。実装後に確かめる。比べる名前と候補の元は試作の `find::suggest` の検証済みの動きから）:

| 入力 | 診断 | 候補（`candidates`） | `help` | `fixes` |
| --- | --- | --- | --- | --- |
| `let total = 1` の後に `totl` | `E1002 unknown value 'totl'` | `total`（`local`・距離 1） | `did you mean 'total'?` | `Replace 'totl' with 'total'`（`maybe`） |
| `let count = 1` の後に `cnt` | `E1002 unknown value 'cnt'` | なし（`cnt` は 3 文字で、`count` は距離 2 > 1） | G22 Phase 3 の `help` のまま | なし |
| `length (ref xs)`（std に `length` を持つモジュールが 9 つある。`Arena`・`Array`・`HashMap`・`HashSet`・`List`・`Map`・`Set`・`String`・`Utf8String`。`Arena` は opt-in で、名前を書かなければ読まれない。組み込みの `Vec.length` もある） | `E1002 unknown value 'length'` | `Array.length`・`HashMap.length`・…（`qualify`・距離 0。最大 10 件） | `did you mean one of: 'Array.length', 'HashMap.length', 'HashSet.length'?` | なし（先頭が 1 つに決まらない） |
| `Array.lenght (ref xs)` | `E1002 module 'Array' has no function or union case 'lenght'` | `length`（`similar`・距離 1。入れ替え） | `did you mean 'length'?` | `Replace 'lenght' with 'length'`（`maybe`。置換先は `lenght` の span） |
| `record Point { x: i64 }` の後に `let p: Pointt = ...` | `E1004 unknown record, union, or type alias 'Pointt'` | `Point`（`similar`・距離 1） | `did you mean 'Point'?` | `Replace 'Pointt' with 'Point'`（`maybe`） |
| `\| Circl r -> r` | `E1020 unknown union case or active pattern 'Circl'` | `Circle`（距離 1） | `did you mean 'Circle'?` | `Replace 'Circl' with 'Circle'`（`maybe`） |

### 互換と決定性

- 決定性: 候補・索引・出力は、`BTreeMap` の順・文字列の順・上の整列だけに依る。乱数・時刻・スレッド数に依らない（穴の探索は関数ごとの `Checker` の中で、結果は関数の順に集める既存の仕組みに乗る）。
- 互換: `?` を含まない入力の診断は、Phase 4 の 6 つの診断の `candidates`・`help`・`fixes` が増える以外は 1 byte も変わらない。人間向けの描画には G22 の `help:`・`fix (maybe):` の行が増える。`message`・`code`・主スパンは変わらない。`docs/stability.md` の「診断の JSON の追加キーは互換」に当たる。
- 性能: 穴・未解決名の探索は、診断が出る失敗の経路だけで動く。成功するコンパイルの速度は変えない（索引は最初の穴で 1 回だけ作る）。`find` は std の全モジュールを読む（`check` の約 4 倍、約 0.16 秒。実測）。

## 設計

### 変更するファイル

| ファイル | 変更 | Phase |
| --- | --- | --- |
| `src/syntax.rs` | `TokenKind::Question`、`ExprKind::Hole(Option<Ident>)`（`visit` の葉の群に加える） | 1 |
| `src/lexer.rs` | `Lexer::symbol` に `'?' => Question` | 1 |
| `src/parser.rs` | `fn hole`、`primary` の腕、`space_argument` に `Question`。Phase 3: `parse_type`・`parse_type_query` | 1・3 |
| `src/formatter.rs` | 記号の間隔の規則に `Question`、`Canonical` の `Hole` の腕 | 1 |
| `src/syntax_codec.rs` | `ExprKind::Hole` の tag（`ExprKind` の次の空き番号。HEAD は 46）、`("ExprKind", N)` の件数 +1 | 1 |
| `src/frontend_cache.rs` | `FRONTEND_FORMAT` を 1 上げる（他のチケットが先に上げていればその次の値） | 1 |
| `src/computation.rs`・`src/warnings.rs` | 式の葉の群に `Hole(_)` | 1 |
| `src/check.rs` | `value_expression` の腕、`Checker::typed_hole`、`#[path = "find.rs"] pub mod find;`、`Names.search_index: OnceLock<Vec<find::Entry>>`、`Checker.function: Option<usize>`・`Checker.holes_searched: usize`、Phase 3 の呼び出し、Phase 4 の 6 か所 | 1〜4 |
| `src/polymorph.rs` | `map_type` を `pub(super)`、`Classes::holds_class`・`Classes::class_name` | 1・2 |
| `src/control.rs` | `E1020` の候補 | 4 |
| `src/semantic.rs` | `SemanticIndex.find: Option<find::Request>`・`found: Option<Result<find::Outcome, Diagnostic>>` | 3 |
| `src/find.rs`（新規） | 索引・照合・穴の探索・CLI・`suggest` | 2〜4 |
| `src/main.rs` | `find_command`・`HELP`・サブコマンドの分岐 | 3 |
| テスト | 新規 `tests/typed_holes.rs`・`tests/find.rs`・`tests/name_suggestions.rs`、変更 `tests/modules.rs`（1 行） | 1〜4 |

Phase 1 の字句・構文・整形・符号化・葉の群の変更は、試作の差分（付録 A.2。**そのまま使える**）。

### 型（`src/find.rs`）

付録 B.2 の検証済みのコードが定義と本体の全部。要点だけ:

```rust
pub const REQUESTER: &str = "Main";           // 問い合わせの型名を解決し、見えるかを決める視点
pub const MAX_ENTRIES_EXAMINED: usize = 5_000;
pub const MAX_HOLE_SEARCHES: usize = 16;      // 1 関数あたり
pub enum Kind { Def, Const, Builtin, Field, Case }          // `as_str`: def const builtin field case
pub struct Entry { pub kind, pub name, pub module, pub public, pub origin, pub id: Option<usize>,
                   pub variables, pub parameters, pub result, pub constraints: Vec<(usize, Type)>, pub text }
pub enum Tier { Exact, Borrow, Partial }                    // `as_str`
pub struct Request { pub parameters: Vec<TypeExpr>, pub result: Option<TypeExpr>, pub loose: bool, pub module: Option<String> }
pub struct Hit { pub tier, pub extra: usize, pub kind, pub name, pub module, pub origin, pub text }
pub struct Outcome { pub hits: Vec<Hit>, pub searched: usize }   // 整列済み・未切り詰め
pub(super) fn build(names, types, signatures, classes) -> Vec<Entry>
pub(super) fn search(request, names, types, signatures, classes, entries) -> Result<Outcome, Diagnostic>
pub(super) fn tier(entry, query, types, classes) -> Option<(Tier, usize)>
pub(super) fn instantiate(entry, search: &mut Inference) -> (Vec<Type>, Type, Vec<(usize, Type)>)
pub(super) fn constraints_hold(constraints, search, types, classes) -> bool
impl Checker<'_> {   // Phase 2。子モジュールから親の `Checker` の private なフィールドを使える
    pub(super) fn hole_candidates(&self, wanted: &Type, scope: &[(String, Type)], hole: Span) -> (Vec<Candidate>, Vec<Fix>)
}
```

CLI の型（Phase 3。`main.rs` が使う）:

```rust
pub struct Options { pub query: Option<String>, pub result: Option<String>, pub takes: Vec<String>,
                     pub module: Option<String>, pub project: Option<PathBuf>, pub limit: usize, pub json: bool }
pub struct Report { pub outcome: Outcome, pub query: String, pub limit: usize }
pub enum Failure { Usage(Diagnostic), Query { diagnostic: Diagnostic, text: String },
                   Load(SourceError),                                        // `--project` が読めない
                   Analysis { errors: DiagnosticSet, project: Project } }   // std だけの解析は失敗しない（`expect`）
pub fn parse_arguments(arguments: &[OsString]) -> Result<Options, Diagnostic>   // E2000 find_bad_arguments
pub fn run(options: &Options) -> Result<Report, Failure>                        // 出力はしない
pub fn render(report: &Report, json: bool) -> String                            // 標準出力に出す全文
```

### 処理

**穴の検査（Phase 1）**。`value_expression` の `ExprKind::Hole(name)` の腕は `return Err(self.typed_hole(name.as_ref(), expected, expression.span))`。`typed_hole` は次を順に行う: (1) `expected` を `self.inference.resolve` して「分かる」かを決める。(2) `scope` を作る（`self.scopes` を内側から外へ、`provenance == User`・名前が `$` で始まらず、見たことのない名前だけを集め、型が期待型と単一化できるものを先、各群を `Local.id` の昇順に並べ、32 件まで）。(3) `Diagnostic::new("E1030", message, span)` に `with_kind("typed_hole")`・`with_help`・`detail.expected`・`with_scope` を付ける（`expected_found` は使わない。`found` がない）。(4) Phase 2 なら候補を付ける。「単一化できる」は、`Inference::default()` を新しく作り、`Infer`・`Error` を新しい変数に置き換えた写し（試作の `import_type`。付録 B.3）で `unify` の成否だけを見る。

**穴の候補（Phase 2）**。`typed_hole` は、期待型が「分かる」かつ `self.holes_searched < MAX_HOLE_SEARCHES` のとき `self.holes_searched += 1` して `self.hole_candidates(wanted, &scope, span)` を呼び、返った `Vec<Candidate>` を `with_candidates`、`Vec<Fix>` を `fix` で診断に付ける。`hole_candidates` は、(1) 局所の値、(2) 局所の record の field、(3) `names.search_index.get_or_init(|| find::build(..))` の `Entry`（`visible_from(self.module)`、`entry.id == self.function` を除く）を調べ、順位・上限・制約の規則（仕様）で並べて 10 件にする。探索の失敗・上限超過は何も付けない。試作の本体は付録 B.3。`Checker.function` は、関数の本体を検査する `Checker::new` の直後（`checker.indexing = indexing;` の次の行）に `Some(id)` を入れる。入口の式の `Checker`（`id` がない）は `None` のまま。`holes_searched` は `Checker::new` で 0 にする。

**索引（Phase 2）**。`build` は付録 B.2。組み込み関数は、1 つごとに使い捨ての `Checker::new(REQUESTER, names, types, signatures, classes)` を作って `builtin(*builtin, Span::default())` を呼び（内部用のものは `E1022` で `Err`。除く）、`Infer(k)` を出現順に `'a`・`'b`… に置き換える。`names`・`types` は共有の参照なので、持ち主の `Checker` の状態は変わらない。

**照合と探索（Phase 3）**。`SemanticIndex` が運び手: 呼び出し側が `find = Some(request)` を入れて `analyze_inputs_semantic` を呼ぶ。`check_modules_collect` は、すべての署名が決まった後・関数の本体の検査の前（`let declaration_count = function_declarations.len();` の直前）に `semantic.find.take()` を見て、あれば `found = Some(find::search(...))` を入れる。引数 `semantic` は `mut semantic` にして `as_deref_mut()` で借りる。**本体の検査は止めない**（`find` は `check` と同じ時間がかかる。std だけで約 0.16 秒）。`found` が `Some` なら、その後の解析の誤りは捨てる（本体の誤りがあっても結果を出す）。`found` が `None` で解析が誤りなら `Failure::Analysis`。`run` の入力は、std だけのとき `("Main.tz", "")` の利用者ソース 1 つと `stdlib::SOURCES` の全部（`lib.rs` の `analyze_modules_with_semantics` と同じ並べ方）、`--project` のとき `Project::load_with_overlays(dir, &BTreeMap::new())` の `sources`（`lsp.rs` の `analyze` と同じ `SourceInput` の作り方）。問い合わせの型は `parse_type`／`parse_type_query` で読み（誤りは `Failure::Query`）、`_` を `name_wildcards` で一意の型変数にしてから `resolve_type_with_kinds(.., REQUESTER, names, &BTreeMap::new())` で解決する。`E2000` の `find_unknown_module` は `search` が返し、`run` は `diagnostic.code == "E2000"` を `Failure::Usage` にする。

**候補（Phase 4）**。`suggest` は付録 B.1（検証済み）。診断の付け方は仕様の表。6 か所のそれぞれで、`Err(Diagnostic)` を返す直前に `pool` を作り、候補があれば `with_candidates`・`with_help`・`fix` を呼ぶ（付録 C に発行箇所・`pool` の作り方・置換先の span を 1 行ずつ書いた）。`Names` のメソッド（`named_type` の `Choice::Missing`）は、`self` の別名表だけで `pool` を作る。

### 前提とする他チケットのインターフェース

| チケット | 使うもの | 無いとき |
| --- | --- | --- |
| G22 Phase 1 | `Diagnostic`（`Deref`・`DerefMut`。`detail` を直接書ける）、`Fix`・`Edit`・`Applicability`・`Candidate`・`ScopeEntry`・`Detail`、`with_kind`・`with_help`・`fix`・`with_candidates`・`with_scope` | 着手しない（停止条件 1） |
| G22 Phase 3 | `E1002`・`E1007` の `scope` と `help`（Phase 4 の `help` はそれを置き換える。`scope` は触らない） | 無くても動く（`help` は Phase 4 だけ） |
| G22 Phase 4 | `fixes` から LSP の quickfix。`maybe` の fix は `isPreferred: false` | 無くても動く（LSP に出ないだけ） |
| G23 | kind の登録簿。あれば `E1030` の `typed_hole`、`E2000` の `find_bad_arguments`・`find_unknown_module` を S1 の手順で登録し、発行は `Diagnostic::of(DiagnosticKind::…)`（`grep -c "Diagnostic::of(" src/check.rs` が 1 以上なら） | `with_kind("…")` で直接付ける |
| G24 | `tsuzuri fix --applicability maybe --dry-run` が `maybe` の fix を見せる（`fixes` を持つ診断をそのまま使う） | 無くても動く |
| G28 | `kb find` が `find::run` と `find::render` を呼ぶ | 本チケットは何も要しない |

## 実装手順

Phase ごとに別の PR にする。各手順は「テストを先に書き、失敗を確かめてから実装する」。検証のコマンドは README の `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 cargo test --locked`（Phase の終わりに `TSUZURI_THREADS=8` 付きでも 1 回）。試作（HEAD の複製の debug ビルド）は測定の後に削除した。実装では付録の検証済みコードを使う。

### Phase 1

1. **基準**。`cargo build --release --locked && RUST_MIN_STACK=4194304 cargo test --locked 2>&1 | grep "test result"` の件数を控える。着手条件の `grep` 3 つを実行する。`grep -n "FRONTEND_FORMAT" src/frontend_cache.rs` の値と、`src/syntax_codec.rs` の `("ExprKind", N)` と最大の tag を控える。
2. **テストを先に**。`tests/typed_holes.rs` に付録 D.1 の Phase 1 のテスト（8 件）を書く。`cargo test --locked --test typed_holes` が全部失敗する（`E0001`）ことを確かめる。
3. **字句・構文・整形・符号化・葉の群**。付録 A.2 の差分をそのまま当てる。tag は `ExprKind` の最大の tag + 1、`FRONTEND_FORMAT` は 1 上げる。`cargo build` が **網羅されていない `match`** で失敗するので、出た箇所（`check.rs` の `value_expression` を含む）に `Hole` を足す。`check.rs` の腕はこの段階では `ExprKind::Hole(_) => return Err(Diagnostic::new("E1030", "typed hole", expression.span))` の仮でよい。**確認**: `cargo build` が通る。`src/lexer.rs`・`src/formatter.rs` に付録 D.4 の単体テスト 2 件を足して通す。`cargo test --locked --lib` と `--test modules`（手順 5 の 1 行を除く）が基準と同じ。
4. **検査**。`typed_hole` と `hole_type` を仕様どおりに書く（骨格は付録 A.3・A.4。`map_type` を `pub(super)` にする。発行は `with_kind("typed_hole")`。G23 方式なら「前提」の表）。Phase 2 の `hole_candidates` の呼び出しは手順 9 まで書かない。**確認**: `cargo test --locked --test typed_holes` の Phase 1 の 8 件が通る。例 1〜3 が付録 D.1 の入力と一致する。
5. **既存テスト**。`tests/modules.rs` の `keeps_source_identity_for_lexing_parsing_types_and_recursive_layouts` の `fn value() -> i64 { ? }` を `` fn value() -> i64 { ` } `` に直す（`E0001` を実測済み）。**確認**: 全テストが基準 + 8 件（Phase 1 の新規）で通る。`printf 'let x = f   ?a ?\nx\n' > $WORK/f1/Main.tz && target/release/tsuzuri fmt $WORK/f1` が `let x = f ?a ?` に整形し、`fmt --check` が通る（std の既存の未整形の差は無関係なので std には当てない）。
6. **文書**（「ドキュメント」の Phase 1 の行）。**確認**: `node scripts/check-docs.mjs` を変えたページに当てて通る。

### Phase 2

7. **テストを先に**。付録 D.1 の Phase 2 のテスト（9 件）を書き、失敗を確かめる。
8. **索引**。`Classes::holds_class`・`class_name`、`src/find.rs`（付録 B.2 の `Kind`・`Entry`・`render`・`build`・`ground`・`instantiate`・`constraints_hold`。`Tier` 以降は手順 11 で）、`#[path = "find.rs"] pub mod find;`、`Names.search_index`、`Checker.function`（`checker.indexing = indexing;` の次の行で `Some(id)`）。**確認**: `cargo build` が通る。
9. **候補**。`hole_candidates`（付録 B.3）と `maybe` の fix、上限（`MAX_HOLE_SEARCHES`・`MAX_ENTRIES_EXAMINED`・200 文字）。**確認**: Phase 2 の 9 件が通る。例 4・5 の `candidates` が実測と一致する。`TSUZURI_THREADS=1` と `=8` で出力が同じ。全テストが通る。
10. **文書**。`node scripts/check-docs.mjs` が通る。Phase 2 の PR を出す。

### Phase 3

11. **問い合わせと照合**。`parse_type`・`parse_type_query`（付録 A.2 の `parse_type_query` に `parse_type` を足す。`parse_type` は 1 つの型を読み、後ろに何かあれば `E0002` の `unexpected input after the type`）と単体テスト 4 件。`SemanticIndex.find`・`found`、`check_modules_collect` の呼び出し、付録 B.2 の `Tier`・`Request`・`Hit`・`Outcome`・`tier`・`name_wildcards`・`module_matches`・`search`。**確認**: `cargo build` が通る。
12. **CLI**。`tests/find.rs`（付録 D.2 の 15 件）と `find::parse_arguments` の単体テスト 4 件を先に書き、`parse_arguments`・`run`・`render`、`main.rs` の `find_command`（`api_diff_command` の作り。`parse_arguments` の後に `invalid_thread_setting(json)` を呼び、`raw.first() == "find"` の分岐を `api-diff` の次に足す。`Failure::Usage` は `print_diagnostic(.., Path::new("<command line>"), "", json)` で終了コード 2（`diagnostic.code == "E2000"` の `find_unknown_module` も 2）、`Failure::Query` は `Path::new("<query>")` と問い合わせを本文にして終了コード 1、`Failure::Load(error)` は `print_diagnostic(&error.diagnostic, &error.path, "", json)` で終了コード 1、`Failure::Analysis` は `print_diagnostics(&errors, &project, json)` で終了コード 1）・`HELP`・`E2000` の kind を実装する。**確認**: `cargo build --release --locked && target/release/tsuzuri find 'ref [i64] -> i64'` の先頭 4 行が例 6 と一致する。`tsuzuri find --json 'string -> i64'` の末尾が `summary` の行。`tsuzuri find --module Arrey x` が終了コード 2。
13. **文書**。`node scripts/check-docs.mjs`、`cargo test --locked`（`TSUZURI_THREADS=8` 付きも）が通る。Phase 3 の PR を出す。

### Phase 4

14. **`suggest`**。付録 B.1 のコードと単体テスト 3 件を `src/find.rs` に入れる（`Pooled`・`Suggestion`・`distance`・`limit`・`suggest`・`best`）。`tests/name_suggestions.rs`（付録 D.3 の 10 件）を先に書く。
15. **6 か所**。付録 C の順に 1 か所ずつ（`pool` の作り方・置換先の span・`how`・`signature`）実装し、そのたびに該当のテストを通す。G22 Phase 3 が main にあれば、`help` の呼び出しを G22 の後ろに置く。**確認**: 既存の全テストが通る（`message`・`code`・`span` を変えていない）。`cnt`（近い名前なし）の診断の JSON が変更前と 1 byte も違わない（`candidates`・`help`・`fixes` のキーが増えない）。
16. **文書と最終確認**。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 cargo test --locked` と `TSUZURI_THREADS=8` 付き、`cargo build --release --locked`、`node scripts/check-docs.mjs` を変えたページに。`git status` が「変更するファイル」の表と一致する。Phase 4 の PR を出す。

## テスト計画

新規テストは合計 57 件（付録 D に入力と期待値）。数字は Phase の PR ごとの目安で、`cargo test --locked` の増分と一致させる。

| ファイル | テスト | 件数 | Phase |
| --- | --- | --- | --- |
| `tests/typed_holes.rs` | 穴の検査 8（連鎖しない・名前つき・独立した誤りとの共存・位置ごとの期待型・`scope` の規則・build しない・`fmt` が保つ・式以外の `?`） | 8 | 1 |
| `src/lexer.rs`・`src/formatter.rs` の単体 | `?` の字句、穴の間隔 | 2 | 1 |
| `tests/typed_holes.rs` | 候補 9（局所と順位・自分自身の除外・制約・`maybe` の fix 2 件・union の case・上限と決定性・16 個目以降・期待型が不明） | 9 | 2 |
| `tests/find.rs` | CLI 15（段・borrow・`_` と型変数・制約・`--result`/`--takes`・`--module`・`--limit`・JSON・0 件・問い合わせの誤り・使い方の誤り・`--project` の 3 件・決定性） | 15 | 3 |
| `src/parser.rs`・`src/find.rs`・`src/main.rs` の単体 | `parse_type`・`parse_type_query` 4、`name_wildcards`・`module_matches` 2、`find_arguments` 4 | 10 | 3 |
| `tests/name_suggestions.rs` | 6 つの診断・近い名前なし・曖昧・不変 | 10 | 4 |
| `src/find.rs` の単体 | `distance`・`suggest` 3 | 3 | 4 |

**既存テストへの影響**: (1) `tests/modules.rs` の字句の誤りの例 1 行（手順 5）。(2) `src/syntax_codec.rs` の `("ExprKind", N)` の件数 +1。(3) `src/frontend_cache.rs` の `FRONTEND_FORMAT` を固定値で照合するテストがあれば、その値。(4) Phase 4 は `message`・`code`・`span` を変えないので、`E1002`・`E1004`・`E1007`・`E1020` を `message` で照合する既存のテスト 8 件（`tests/diagnostics.rs`・`union_types.rs`・`alias.rs`・`local_recursion.rs`）は変わらない。人間向けの全文を照合するテストがあれば `help:` の行が増える（HEAD では見つからない。手順 15 の全テストで確かめる）。ほかの既存テストは変わらない（`?` を含まない入力の診断・IR は同じ）。

**性能**: 追加の測定は不要（成功するコンパイルの経路は変わらない）。`find` の実測は付録 E（std だけ: 索引 約 5 ms、照合 約 5 ms。debug ビルド。`check` 全体は約 0.16 秒）。CI の閾値は足さない。

## ドキュメント

| ファイル | 内容 | Phase |
| --- | --- | --- |
| `_tsuzuri/language-reference/types-and-type-inference/type-inference.md` | 節「型つき穴 `?`」: 書き方・`E1030` の内容（`expected`・`scope`・`candidates`）・例 1〜5・`maybe` の fix | 1・2 |
| `_tsuzuri/language-reference/values-and-functions/tokens.md`・`op-and-expressions.md` | `?` の字句（`?` と `?名前`）、式としての穴。`?` 演算子ではない | 1 |
| `_tsuzuri/language-reference/compiler/diagnostics.md` | `E1030` の行、`E1001`〜`E1028` の記述を `E1030` まで、Phase 4 の 6 つの診断の候補と `maybe` の fix、LSP の quickfix | 1・4 |
| `_tsuzuri/language-reference/compiler/usage.md`・`option.md` | `### find`（引数・段・出力・例 6・終了コード）、オプションの表。`find` という名前のパスは `tsuzuri build find` か `./find` と書く | 3 |
| `README.md` | CLI の表に `find`、`?` と `find` の短い節 | 1・3 |
| `docs/language.md` | 診断の表（`E1030`）、式の構文に穴、`find` の節 | 1・3 |
| `docs/architecture.md` | `src/find.rs` の行、`Names.search_index`、`SemanticIndex.find`/`found` | 2・3 |
| `docs/stability.md` | `find` の出力（人間向け・JSON のキー）と `E1030` の追加は互換 | 3 |

`node scripts/check-docs.mjs <変えたページ>` が通ること。例は `tsuzuri` の fence（`check` される）と、出力を見せる `text` の fence に分ける。未実装の Phase の記述は書かない（現在の実装だけ。計画は `_features/` へのリンク）。

## 受け入れ条件

- [ ] Phase 1: `?`・`?名前` が式の位置で読め、`tsuzuri check` が 1 穴 1 件の `E1030`（`expected`・`scope`・kind `typed_hole`・`help`）を出し、連鎖する誤りを出さず、`tsuzuri build` は何も作らず、`tsuzuri fmt` が穴を保つ。`?` を含まない入力の診断・IR は 1 byte も変わらない。
- [ ] Phase 2: 穴の `candidates`（最大 10。順位 0〜5）が付き、自分自身は出ず、制約を満たさないものは出ない。順位 0 が 1 件だけなら `maybe` の fix。17 個目以降の穴・期待型が不明な穴は `candidates` なし。`TSUZURI_THREADS` に依らず同じ出力。
- [ ] Phase 3: `tsuzuri find` が std と（`--project` で）利用者の `def`・`const`・組み込み関数・field・case を、`exact`／`borrow`／`partial` の段で探す。`--result`・`--takes`・`--module`・`--limit`・`--json`、終了コード 0／1／2 が仕様どおり。例 6 の出力が実測と一致する。
- [ ] Phase 4: 6 つの診断に、近い名前と修飾名の `candidates`・`help`、先頭が 1 つに決まるときの `maybe` の fix が付く。近い名前がなければ診断は 1 byte も変わらない。`message`・`code`・主スパンは不変。
- [ ] 新規テスト 57 件が通る。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked`（`TSUZURI_THREADS=8` 付きも）・`cargo build --release --locked`・`node scripts/check-docs.mjs` が通る。
- [ ] 文書（上の表）を更新し、`docs/language.md`・`README.md` と一致させた。GUIDE の台帳（D-55）に `find` のフラグ（`--project`・`--result`・`--takes`・`--module`・`--limit`）と `E2000` の kind 2 つ、`E1030` の kind を追記するよう報告した。

## 落とし穴

| # | 落とし穴 | 検出 |
| --- | --- | --- |
| 1 | `?` を読めるようにしただけで `tsuzuri fmt` が「formatter would change program semantics」で失敗する（`?x` の間に空白が入る）。間隔の規則に `Question` を足す | テスト 7 |
| 2 | `space_argument` に `Question` を足さないと `f ?x ?y` が引数 1 つになる | テスト 3 |
| 3 | `Type::display` は未確定を `an undetermined type`、誤りを `an erroneous type` と書く。診断には `_` を出す（`hole_type`） | テスト 4 |
| 4 | `scope` に `match` の主語などの生成された局所（`$match39`）が混じる。`provenance == User` と `$` で除く | テスト 5 |
| 5 | `Classes::holds` は未知のクラス名で panic する。`holds_class` を使い、具体的な型だけに限る | テスト 11 |
| 6 | 探索で `Inference` を共有すると、候補ごとの束縛が漏れる。`Entry` ごとに新しい `Inference::default()`、引数の試行は `clone` してから取り込む | テスト 9・11 |
| 7 | 引数の充填で、取り込んだ後に制約が壊れる局所（`Int.count_ones xs` の `Integer<[i64]>`）を採ると偽の候補が出る。取り込む前に `constraints_hold` で確かめる | テスト 11 |
| 8 | 結果が型変数 1 つだけの関数（`Array.fold ? ? ?`）が、同じ順位の具体的な候補（`Main.Circle ?`）より前に並ぶ。整列の第 1 キーを足す | テスト 14 |
| 9 | 並列の本体の検査で索引を作ると順序が揺れる。`OnceLock` で 1 回だけ作り、`BTreeMap` の順を保つ | テスト 15 |
| 10 | `check_modules_collect` の呼び出しを本体の検査の後に置くと、本体の誤りがある利用者のプロジェクトで結果が出ない。署名が決まった直後に置く | テスト 12 の後半 |
| 11 | `_` の問い合わせを `resolve_type_with_kinds` に渡すと `E1004`（`_` は型名として探される）。`name_wildcards` で変数にしてから渡す | 単体テスト |
| 12 | 組み込み関数の使い捨ての `Checker` の `module` を `IO`・`Os` 系の名前にすると、内部用の組み込み関数が `Ok` になって索引に入る。`REQUESTER`（`Main`）を使う | テスト 1 |
| 13 | Phase 4 の `help` は G22 Phase 3 の `help` を置き換える（候補があるときだけ）。`scope` は触らない。候補が 0 件なら何も呼ばない | テスト 10 |
| 14 | `FRONTEND_FORMAT`・tag は他のチケットも上げる。固定の数にせず、現在の値から 1 つ足す | 手順 1 の控え |

## 対象外

- 型クラスのメソッド・active pattern・`extern` 関数・計算式の builder の操作・属性の検索（索引は `def`・`const`・組み込み関数・field・case だけ）。
- 型の同型（型別名の展開を超えた意味的な等価）・高階型の部分適用の照合・ランク付けの学習。
- `find` の本体の検査を省く最適化（`check` と同じ時間がかかる。std だけで約 0.16 秒）。
- 穴を実行時に使うこと、`?` を `?` 演算子にすること、穴の自動補完（LSP の completion）。`maybe` の fix は G22 Phase 4 の quickfix 経由で LSP に出る。
- 2 つ以上の穴の同時の充填、`candidates` の再帰的な組み立て（引数が `?` のまま。候補の候補は出さない）。
- 利用者の宣言の型別名の中での照合、名前空間つきのモジュールの `using` による短縮名（候補は `declaration_display` の完全な名前）。
- `E1004` の「unknown type class, record type, or union type」（型クラスの位置）の候補。

## 決定事項

状態はすべて「既定案（承認済み。D-50 の包括許可）」。

- **D1 穴は誤りとして回復する**。`TypedExprKind::Hole` を足さない（設計の種の案との差）。`Err(E1030)` を返せば `recover_expression` が `Type::Error` にし、連鎖する誤りが出ない（試作で確かめた）。却下: 穴に新しい型変数を持たせて推論に参加させる案（`TypedExprKind` の全 variant と所有権・コード生成の全箇所の更新が要り、穴は常に誤りなので得るものがない）。
- **D2 診断**: コード `E1030`（D-55）、kind `typed_hole`、1 つの穴に 1 件。`expected`・`scope`・（Phase 2）`candidates`・`maybe` の fix。型は `hole_type` で `_` を使って表示する。却下: 穴ごとに `E1030` と別の「型が分からない」診断を分ける案（利用者に同じ対処で、コードが増えるだけ）。
- **D3 `?名前`**: 名前は `message` に出るだけで、束縛にも参照にもならない。`?` と名前の間に空白があれば名前なしの穴（`? foo` は穴への引数）。却下: 名前を `scope` に足して後の式から参照できるようにする案（穴は誤りで、名前の意味づけが要らない）。
- **D4 全 edition が受理**する（受理を増やす拡張）。予約語は足さない。`?` は `?` 演算子ではない（「採用しない改善」）。却下: 新しい edition だけが `?` を受理する案（GUIDE の edition の規則で、受理を増やす拡張は全 edition が受理する）。
- **D5 探索は誤りにならない**: 1 関数あたり 16 個の穴、1 回の探索で 5,000 個の `Entry`、`candidates` 10 件、テキスト 200 文字の上限。超えても Phase 1 の診断のまま。却下: 上限なし（穴を大量に書いたときの時間が読めない）。
- **D6 `maybe` の fix は順位 0 が 1 件のときだけ**。穴を勝手に埋めない（`tsuzuri fix` は `maybe` を `--dry-run` でしか適用しない。G24）。却下: 先頭の候補を常に fix にする案（関数の候補は意味が違うことが多い）。
- **D7 索引は 1 つ**（穴と `find` で共有）。種類は `def`・`const`・`builtin`・`field`・`case`。設計の種に合わせて case を含めた。型クラスのメソッドは対象外。組み込み関数は使い捨ての `Checker` で型を得る（署名の表を別に持たない）。却下: 組み込み関数の署名を手書きの表にする案（246 項目が実装とずれる）。`find` を std の `.tz` の字句だけで探す案（制約・型変数・別名を扱えない）。
- **D8 照合は型つき**: `Inference` を再利用し、問い合わせと `Entry` の両方の変数を束縛する単一化（`'a -> 'a` は `i64 -> i64` に合う）。段は `exact`・`borrow`・`partial`（引数の順序違いを含む）。`--result`・`--takes` は `exact`・`borrow` だけ。却下: 文字列の部分一致（Hoogle のような型の一致にならない）。問い合わせの変数を固定（剛体）にする案（`'a -> 'a` が `Debug.trace` にしか合わず、使い道が狭い）。
- **D9 `find` は `check` の途中で結果を取る**: `SemanticIndex` を運び手にし（`find`・`found`）、署名が決まった直後に探索する。`src/find.rs` は `check` の子モジュール（`Names`・`Scheme`・`Inference` の内部を使うため）。std は全 47 モジュールを読む。却下: `Names`・`Scheme` を `pub(crate)` にして `src/find.rs` を独立させる案（公開面が広がる）。本体の検査を省く案（`check_modules_collect` の途中打ち切りが要り、対象外にした）。
- **D10 `--project DIR` は明示**（暗黙に cwd のプロジェクトを探さない。出力が cwd に依らない）。設計の種にないフラグ。却下: cwd に `Main.tz` や `Tsuzuri.toml` があれば自動で含める案（同じコマンドの結果が場所で変わる）。
- **D11 終了コード**: 0 件でも 0、問い合わせ・解析の誤りは 1、使い方は 2。`E2000` の kind は `find_bad_arguments`・`find_unknown_module`。却下: 0 件を終了コード 1 にする案（誤りではなく検索の結果。`grep` と違い、AI が失敗と取り違える）。
- **D12 未解決名**: 設計の種は `E1004` と書いていたが、HEAD は値・関数・モジュールのメンバーが `E1002`、型が `E1004`、field が `E1007`、case が `E1020`。対象はこの 6 つの診断。`message`・`code`・主スパンは変えない。却下: コードを `E1004` に統一する案（既存の診断の意味を変える）。
- **D13 候補の選び方**: Damerau-Levenshtein（隣の入れ替えを 1 とする。optimal string alignment）、4 文字以下は 1、5 文字以上は 2、2 文字未満は探さない。修飾すると解決する名前は距離 0。fix は先頭が `(距離, group)` で 1 つに決まるときだけ（`maybe`）。却下: 距離の閾値を長さの割合にする案（短い名前で候補が増える）。先頭を常に fix にする案（`ac` が `aa`・`ab` に等しく近い。D6 と同じ理由）。
- **D14 `help` の共存**: Phase 4 の `help` は、候補があるときだけ G22 Phase 3 の `help` を置き換える。`scope` は触らない。却下: 2 つの `help` を連結する案（`help` は 1 行。D-51）。

## 付録

- [付録](G25-appendix.md): A 位置ごとの期待型・Phase 1 の試作の差分、B 検証済みのコード（B.1 `suggest`、B.2 索引と照合、B.3 穴の探索）、C 未解決名の発行箇所と `pool`、D テストの入力と期待値、E 実測。

## 実装と検証（2026-10-30）

- **範囲**: Phase 1〜4 をすべて実装した。冒頭の「方針の最新化」のとおり、G23・D-60・A18 の後の現状に合わせた。
- **Phase 1**: `TokenKind::Question`、`ExprKind::Hole(Option<Ident>)`（`syntax_codec` の tag 46、`("ExprKind", 47)`、`FRONTEND_FORMAT` 14 → 15）、`Parser::hole`・`space_argument`、整形（`Question` の後は元の間隔、`Canonical` の `Hole`）、`value_expression` の腕が `Checker::typed_hole`（`src/find.rs`）の `E1030` を返す。誤りの回復が `Type::Error` にするので、連鎖せず、型付き IR に入らない。
- **Phase 2**: `src/find.rs` の `Entry`・`build`（`Names.search_index: OnceLock`）・`instantiate`・`constraints_hold`・`hole_candidates`（順位 0〜5、型変数だけの結果を後ろ、10 件、200 文字、1 関数 16 個の穴、5,000 件）、`Classes::holds_class`、`Inference::numeric_default`、`Checker.function`・`holes_searched`。局所の record の field は参照の局所（`ref Record`）にも探す。
- **Phase 3**: `src/main_find.rs`（`parse_find_arguments`・`find_command`）、`kg::find::has_module`（`--module` の存在の確認。`module_named` を `search` と共有）、`HELP`、`main` の分岐。
- **Phase 4**: `find::distance`・`suggest`・`best`・`with_suggestions` と、`value_pool`・`member_pool`・`function_pool`・`field_pool`・`pattern_pool`・`Names::type_pool`。6 つの診断（`unknown_value`・`unknown_module_member`・`unknown_function`・`unknown_type_name`・`unknown_field`・`record_has_no_field`（2 か所）・`unknown_case_or_active_pattern`）に付けた。`message`・`code`・主スパンは変えない。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。新しいテスト 49 件: `tests/typed_holes.rs` 17（Phase 1 の 8・Phase 2 の 9）、`tests/name_suggestions.rs` 10、`tests/find.rs` 13（`find` と `kg find` の一致、誤りのあるプロジェクトを含む）、`src/find.rs` の単体 3、`src/main_find.rs` の単体 4、`src/lexer.rs` の単体 1、`tests/formatter.rs` 1。`cargo nextest run --locked` は全件成功（下の既存テストの直しの後）。`node scripts/check-docs.mjs`（変えた 7 ページ）成功。
- **実測**（debug ビルド、`check`）: 穴 1 つのプログラムは約 0.06〜0.13 秒（索引の構築を含む）。`tsuzuri find`（std だけ、std の知識グラフをキャッシュから読む）は約 0.04 秒、キャッシュが空のときは作るので約 0.6 秒。
- **文書**: `type-inference.md`（節「型つき穴 `?`」）、`tokens.md`、`op-and-expressions.md`、`diagnostics.md`（`E1030`、「未知の名前の候補」、`E2000` の kind、kind の一覧）、`usage.md`（`### find`、終了コード）、`option.md`、`kg.md`、`README.md`（節と CLI の表と検証の表）、`docs/language.md`、`docs/architecture.md`、`docs/stability.md`、`src/explain/codes/E1030.md` と `E1002`・`E1004`・`E1007`・`E1020` のページ、GUIDE の D-55 追補（`find` の kind と `typed_hole`）。
