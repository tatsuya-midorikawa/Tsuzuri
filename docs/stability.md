# 安定性と互換性の方針

コンパイラを更新しても利用者のソースが壊れないように、保つものと保たないものを先に決めておきます。このページは、安定として保つもの、保たないもの、言語版（edition）、非推奨にした宣言の扱い、既知の例外、公開 API の差分の確かめ方をまとめます。個々の機能の仕様は [言語仕様](language.md) と `_tsuzuri/language-reference/` にあり、実装の構造は [アーキテクチャ](architecture.md) にあります。

## 保証の単位

保証の単位は edition です。同じ edition で書いたソースは、コンパイラを更新しても同じ意味で受理されます。コンパイラの更新で変えてよいのは、バグの修正と、受理するプログラムを増やす拡張だけです。受理していたプログラムを拒否する変更は、新しい edition でだけ行います。

### 安定として保つもの

| 対象 | 保つこと |
| --- | --- |
| edition ごとの構文と意味 | 型、値、評価順序、所有権と借用、数値のオーバーフロー・丸め・NaN・トラップ。バグの修正は除きます |
| std の公開 API | 公開した宣言を削除せず、シグネチャも変えません。例外は [既知の例外](#既知の例外) です |
| 公開 ABI | `export def` の `tz_<name>` シンボル、`--emit header` が出す C ヘッダー、`tsuzuri_` で始まるシンボル（`--overflow-report` の WebAssembly が export する `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` を含む）、`extern` のリンク名と WebAssembly の import 名。`--trap-info` の `.trap.json` と `--overflow-report` の `.overflow.json` のキーも同じ扱いです（キーが増えることはあります） |
| 診断のコード・重要度・JSON のキー | コードの意味を変えず、別の条件へ再利用しません。`--json` の既存のキーを消したり改名したりしません（キーが増えることはあります）。追加の情報のキー（`kind`・`related`・`help`・`expected`・`found`・`candidates`・`scope`・`fixes`）は値があるときだけ出し、名前と順序、`related` の `role` と修正案の `applicability` の値を変えません（[診断](../_tsuzuri/language-reference/compiler/diagnostics.md#json-の-1-行-1-オブジェクト)） |
| 診断の kind | すべての診断が持つ `kind` の値（`use_after_move` など）。kind は足すことはあっても、消したり、名前を変えたり、別のコードへ移したりしません。1 つの kind を 2 つに分けるのは互換の変更で、元の kind は残りの条件に使い続けます。edition は kind を変えません（[kind と explain](../_tsuzuri/language-reference/compiler/diagnostics.md#kind-と-explain)） |
| `tsuzuri guide` | サブコマンドとオプションの名前、`--json` の schema `tz-guide/1` とキーの名前と順序（キーが増えることはあります）、終了コード（出力 0、`--agents`・`--llms-txt` のマニフェストがない 1、引数の誤り 2） |
| `tsuzuri fix --json` | `event` ごとのキー（`fix`・`file`・`summary`）の名前と順序、`status`・`reason`・`applicability`・`stop` の値を変えません（キーが増えることはあります）。終了コードは完了 0、中止 1、引数の誤り 2 です。`--applicability` の既定（`certain`）を変えません |
| `tsuzuri explain` の JSON | `--json` と `--catalog --json` のキーの名前と順序を変えません（キーが増えることはあります）。終了コードは成功 0、引数の誤りと不明な対象 2 です |
| `tsuzuri api-diff` の出力形式 | テキストの行の形と `--json` のキー（キーが増えることはあります） |
| `tsuzuri audit`（E15） | テキストの行の形、`--json` の schema `tz-audit/1` と行の `type`（`audit`・`package`・`extern`・`summary`）、キーの名前と順序（キーが増えることはあります。読み手は知らないキーを無視します）、終了コード（違反なし 0、`E1036` か `--deny-warnings` の `W1010` 1、引数の誤り 2）。互換のない変更は `tz-audit/2` にします |
| マニフェストの権限（E15） | `[capabilities]` の `allow` と依存の `capabilities` の書き方、権限の 9 個の名前（`env`・`ffi`・`fs`・`gpu`・`net`・`process`・`random`・`stdio`・`time`）。権限の宣言がないプロジェクトは検査しません。権限の名前は足すことはあっても、消したり意味を変えたりしません |
| 知識グラフの schema（facts の行は `tz-facts/1`。G26・D-60） | `nodes.jsonl`・`edges.jsonl`・`tests.jsonl`・`facts/`・`layout/` の行のキーを消さず・改名せず、行の意味を変えません。キーと、`kind`・`rel`・`event`・`conv`・`layout` の語彙を足すのは互換の変更です（読み手は知らないキーと語彙を無視します）。互換のない変更は schema の番号を上げます（[知識グラフの形式](../_tsuzuri/language-reference/compiler/kg-format.md)） |
| 知識グラフの置き場と `tsuzuri kg`（G27） | `target/tz/kg/` のファイル名、`manifest.json` の schema `tz-kg/1` とキー（`counts` の項目は増えることがあります）、`tsuzuri kg` のサブコマンド・オプション・`--json` のキー・終了コード、`E2009` の kind（[知識グラフ](../_tsuzuri/language-reference/compiler/kg.md)） |
| `tsuzuri find`（G25） | オプションの名前、テキストの行の形（`{段} {名前} :: {署名}`、`  (+n)`、`no matches`、`... N more; raise --limit (at most 100)`）、`--json` の `event`（`result`・`summary`）とキーの名前と順序（キーが増えることはあります）、段の名前（`exact`・`borrow`・`partial`）、終了コード（結果が 0 件でも 0、プロジェクトの誤り 1、引数の誤り 2）、`find_bad_arguments`・`find_unknown_module` の kind。どの宣言がどの段に当たるか、候補の並びの細部は照合の改良で変わることがあります |
| doc の例（G34） | `///` の中の ```` ```tsuzuri ```` のフェンスの記法: 印なし・`run=`（パーセント符号化と比べ方）・`test`・`fails=`・`ignore` の意味と、`# ` の隠し行。印は足すことはあっても、意味を変えません。`tsuzuri test --doc` の `--json` の `type`（`doctest`・`summary`）とキーの名前と順序、`--list --json` のキー（キーが増えることはあります）、終了コード（成功か 0 件 0、失敗・記法の誤り 1、引数の誤り 2）、`doc_example_invalid`（`E2011`）と `doc_test_failed`（`E2006`）の kind |
| 型つき穴（G25） | `?`・`?名前` は全 edition で `E1030`（kind `typed_hole`）の誤りで、プログラムを生成しません。`candidates` の中身と順序、`scope` の並べ方は改良で変わることがあります |
| `tsuzuri mcp`（G29） | ツールの名前、引数のキーと値、`structuredContent` のキー（キーは増えることがあります）、資源の URI、`E2010` と `mcp_*` の kind。`tools.json` のキーを消す・改名するのは互換のない変更です |
| `tsuzuri kg diagram`（G30） | `KIND` の 7 つの名前、`--format` の値（`mermaid`・`dot`）、`--json` の行のキー（`kind`・`format`・`text`・`nodes`・`folded`）、`DiagramOutput` のフィールド、`views/` のファイル名（`modules.mmd`・`types.mmd`・`concurrency.mmd`）、`E2000` の `diagram_*` の kind と終了コード |
| `tsuzuri kg impact` と `test --affected`（G28） | `impact --json` の行の `type` と、`change` の `severity`・`action`・`kind`・`name`・`old`・`new`・`note`、`tests` の `mode`・`affected`・`total`・`reason`・`paths`、`summary` の `breaking`・`non_breaking`・`info`・`omitted`。`--affected --json` の `mode`・`affected`・`total`・`base`・`reason`。`impact` の終了コードは差があっても 0。照会と git のリビジョンの `E2000` の kind `kg_ambiguous_id`・`kg_unknown_id`・`unknown_git_revision` |

### 安定として保たないもの

| 対象 | 理由 |
| --- | --- |
| LLVM IR の形、内部のシンボル名、内部の型の配置 | 最適化と実装のために変わります。`@deprecated` の有無では変わりません。edition による引数規約の違いは下記の例外です |
| ビルドキャッシュと構文解析キャッシュの形式 | コンパイラが変われば読み捨てます |
| 診断のメッセージの文言 | 読みやすくするために直します。コードと位置の意味は保ちます。`related` の `message`、`help`、修正案の `title` と `text` も同じです。どの診断が関連する位置や修正案を持つかも増えることがあります。ヒントの表（`help` とヒントの修正案を付ける規則）も増減します |
| `tsuzuri guide` の本文、`AGENTS.md`・`llms.txt` のひな形 | 文言・章立て・例・std の索引の行は直します（D-56） |
| `tsuzuri explain` の説明 | 題と本文の文言、例、専用の説明を持つ kind の範囲は直します |
| 知識グラフの brief カードの文言 | 行の並び・文言・優先度・切り詰め方は直します（上限の文字数は保ちます） |
| MCP の本文 | `content` の本文、ツールと資源の説明、`instructions` の文言は直します |
| `kg diagram` と `views/*.mmd` の図の本文 | Mermaid・DOT・Markdown の字句、色、ノードと辺の順序、ラベルの切り方の細部は直します（生成した Mermaid が Mermaid で読めることは保ちます） |
| 警告が出るかどうか | 新しい警告が増えることがあります。`--deny-warnings` を付けたビルドは、コンパイラの更新や、依存パッケージが宣言を非推奨にしたことで失敗することがあります。警告を出す・出さないと `--deny-warnings` の結果は、保証の対象外です。`tsuzuri test` が整数の折り返しを報告してテストを失敗にする（A23。`--allow-overflow` で報告だけ）のも、この扱いです。範囲の証明が強くなると報告される地点は減り、計装の対象が変わると増えることがあります |
| 実験的と書いた機能（GPU カーネルなど） | 仕様が固まるまで変わります |
| `tsuzuri api-diff` が見つける変更の範囲 | 宣言の署名だけを読むので、見つけられない変更があります（[公開 API の差分](#公開-api-の差分)） |
| std の関数が要る権限 | std の関数の本体が変わると、その権限も変わることがあります（権限は本体から導出します）。権限を新たに要るようになった std の関数を、権限を宣言したプロジェクトが使っていれば `E1036` になります。リリースノートで知らせます |

## edition

edition は、ソースを読むときの規則の版です。年の名前で呼びます。字句（予約語）と警告の既定に加え、2027 では型検査が通常の関数の引数の既定を決めます。

| edition | 予約語の追加 | 状態 |
| --- | --- | --- |
| `2026` | なし。最初の edition で、[言語仕様の予約語](language.md#ソースと宣言) の一覧が基準です | 既存 manifest の省略時の版です |
| `2027` | なし | 現在の最新（`Edition::LATEST`）です。通常の `def` の所有する具体型・裸の型変数・関数引数を既定で読み取りにし、`mut T` と射影への代入を使えます。同じ意味の書き方が複数あった 6 種類の表記（`for` の素の `..`、`86uy` のような短い接尾辞、`&x`・`&mut x`、`*r`、型の `&T`、旧ラムダ `x -> e`）を `E0004` で拒否します（A22。正準形 `..=`・`..<`・型名の接尾辞・`ref`・`deref`・`\x -> e` はどの edition も受理します） |

どの edition で読むかは、パッケージごと、ソースごとに決まります。`Tsuzuri.toml` の `[package]` に `edition = "2026"` と書きます（[パッケージ](../_tsuzuri/language-reference/organizing-tsuzuri/packages.md#package-セクション)）。

| ソース | edition |
| --- | --- |
| `edition` を書いたマニフェストのパッケージ | その値 |
| `edition` のないマニフェストのパッケージ | `2026` です。この既定は、新しい edition ができても変えません。変えると、既存のマニフェストが新しい予約語で壊れます |
| マニフェストのないプロジェクト、単独のファイル、`script`、`repl` | 最新の edition。スクリプトや文書の例は、コンパイラに追従します |
| 標準ライブラリ | 最新の edition です。全 edition で 1 つのコピーを共有します |
| `tsuzuri fmt` | 入力のディレクトリ（ファイルなら親）の `Tsuzuri.toml` の `edition` です。上位のディレクトリは探さず、マニフェストがなければ最新の edition です |

依存パッケージ（path、git、registry）も、自分の `Tsuzuri.toml` の `edition` で読まれます。edition の違うパッケージを 1 つのプログラムへ結合できます。

edition の差として許すのは、次の 4 つだけです。

- 予約語を増やす。
- 前の edition で非推奨にした構文を削除する。
- 警告の既定の有効・無効を変える。
- 通常の関数の引数を既定で共有借用にする（A18、2026-10-23 に承認された 2027 の例外）。

2027 の表記の拒否（A22）は 2 つ目の「前の edition で非推奨にした構文の削除」に当たります。edition 2026 では `--warn canonical` が同じ表記を `W1007` で報告し、正準形への書き換えを修正案として持ちます。`tsuzuri fix --edition 2027 --from 2026` はその `certain` の書き換えを先に当ててから A18 の移行をします。std・fixture・examples・文書の例は、この書き換えで生成 IR を変えずに正準形へ移しました（引数が `_` の旧ラムダ 6 か所だけは、`\_` がパターンとして空の `match` を作るので、LLVM の最適化前の IR が変わります。最適化後の機械語は同じです）。

引数規約とそれに伴う版別 API 以外では、両方の edition が受理するプログラムの意味と生成 IR は同じです。
2027 の無印の引数は明示的な `ref T` と同じ型になり、2026 の無印の引数とは所有権と ABI が異なる場合があります。
`own T` は全 edition で 2026 の値渡しを明示します。数値などの小さな具体型、`Task`、`dyn`、SIMD、可変束縛引数は値渡しのままです。
`const def`、入口の `main`、`extern`、instance のメソッドは契約が固定されているため補完しません。
2027 の `mut T` は有効な値型 `T` の排他借用です。可変な所有者か排他参照を根にした field・collection 要素への代入を許します。
Array／List の `map`・`mapi`・`iter`・`iteri`・`fold`・`fold_back` は 2027 では読み取り callback を使い、
`_owned` 入口は旧版の値 callback を保ちます。`Vec.push` は 2027 では排他更新して unit を返し、
旧版の消費する入口は `Vec.push_owned` です。これらも同日の全 Phase 承認による例外です。
明示した借用・呼び出しの評価順・数値の意味・トラップ・所有権検査の規則は変更しません。

### edition を追加するとき

新しい edition は、人間の承認を得て 1 つの変更で作ります。名前は、作成年以上で、最新の edition より大きい最小の年です。次を同時に更新します。

- `src/syntax.rs` の `Edition` の定数、`SUPPORTED`、`LATEST`
- `src/lexer.rs` の `EDITION_KEYWORDS`（その edition から予約する語）
- このページの edition の表と、`tests/editions.rs`

`MANIFEST_DEFAULT` は変えません。`LATEST` は移行が検証できるまで動かしません。
現在は checked migration による std・fixture・文書の移行後の 2027 が最新です。リリースに含めた edition は凍結し、差を足しません。
旧ソースは `tsuzuri fix --edition 2027 --from 2026 PATH` で、値渡しの署名と API を明示してから移行できます。
版を取らない Rust の `analyze*` 互換 API は 2026 のままです。新しい規約には明示 edition の解析 API を使います。

予約語を足す変更は、`Lexer::identifier` の `match` に足したうえで、予約を始める edition と一緒に `EDITION_KEYWORDS` へ登録します。そうしないと、旧 edition のパッケージが新しい予約語で壊れます。

予約語を識別子として書く構文（raw identifier）はありません。新しい edition のパッケージからは、旧 edition のパッケージが新しい予約語と同じ名前で公開した宣言を参照できません（`E0002`）。最初に予約語を足す edition を作るときに、raw identifier が要るかどうかを判断します。

## 非推奨の手順

宣言は、取り除く前に非推奨にします。

1. **印を付ける**: 宣言の `///` の説明に、`@deprecated` で始まる行を足します。残りの行が移行先の案内です（`@deprecated use LegacyLib::Old.next instead`）。`tsuzuri doc` と LSP のホバーは、この行も説明として表示します。警告の対象は `def`（`extern def` を除く）、`record`、`union` です。`const`、`type`、`class`、class のメソッド、`extern def` の `@deprecated` は、説明として表示されるだけです。
2. **警告する**: 別のパッケージ（標準ライブラリを含む）が使うと、警告 `W1005` が出ます。同じパッケージの中の使用には出さないので、移行の期間に自分のパッケージの `--deny-warnings` のビルドが失敗することはありません。標準ライブラリの宣言には、使うすべてのプロジェクトで出ます。
3. **取り除く**: 取り除く方法は、対象で決まります。
   - 標準ライブラリの公開 API は取り除きません。
   - パッケージの公開 API を取り除くことは、互換性のない変更です。`tsuzuri api-diff` で確かめ、registry の互換の範囲が変わる版（1.0.0 以降は MAJOR、それより前は `0.MINOR`）へ上げます。
   - 構文は、新しい edition でだけ削除します。

## 既知の例外

標準ライブラリの公開 API は削除しませんが、edition と無関係に、次の変更は利用者のソースを壊すことがあります。release notes で予告します。

- **std へのモジュールの追加**: 新しいモジュールの名前は予約されます。同じ名前の利用者のモジュールは `E1011` になります。
- **std の union への case の追加**: その union を網羅的に `match` するソースが `E1021` になります。case の追加は互換性のない変更として扱い、必要になったときに、edition ごとの切り分けを含めて別に承認を得ます。

edition ごとに std を複製することはしません。型と instance が edition の数だけ分かれ、パッケージの間で値を渡せなくなるからです。

## 公開 API の差分

`tsuzuri api-diff OLD NEW [--json] [--effective]` は、パッケージやプロジェクトの 2 つの版の公開 API を比べ、1 変更 1 行で出します。互換性のない変更があると、終了コードは 1 です。使い方と出力は [コンパイラの使い方](../_tsuzuri/language-reference/compiler/usage.md#api-diff) にあります。

- 互換性のない変更: 公開宣言の削除、署名の変更（型、制約、region、`export` と `const`（`const def`）の取り消し。別のモジュールの `const` の初期化式が `const def` を呼べるので、`const` を外すとそれが壊れます）、record への field の追加（構築式が壊れます）、union への case の追加（網羅的な `match` が壊れます）、既定のない class メソッドの追加、`deriving` の削除。
- 互換性を保つ変更: 宣言の追加、説明の変更、`@deprecated` の追加、`export` の追加。

`pure def` の `pure` を加えるのは契約の追加、取り消すのは非互換な契約の削除として報告します。
ただし、共有借用の補完で署名が変わる場合や、保証する呼び出し段階を変える場合は非互換です。
class メソッドの純粋性の追加も、既存 instance の義務が増えるため非互換です。
効果の属性（`@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`）を足すのは約束の追加で互換、外すのは約束の削除で非互換として報告します。関数を受け取りうる引数を持つ関数に足すのは、呼び出し側が渡す関数にも同じ属性を求めるので非互換です。型変数や長さパラメーター（`[i64; N]` の `N`。結果の型にだけあるものも）を含む関数に足すのも、呼び出し側が選ぶ型と長さのインスタンスごとに検査するので非互換です。属性は生成コードを変えません。
契約の行は、`@requires` を足す（呼ぶ側の義務が増える）・`@ensures` を外す（保証が減る）のを非互換、`@requires` を外す・`@ensures` を足すのを互換として報告します。

`extern` の宣言の `@trusted`（E15）は、人が確かめた印で、API には入れません。足しても外しても報告しません。外すと `--warn untrusted-extern` と `tsuzuri audit` の `W1010` が増えることがあります。

既定は宣言の署名の文字列を比べます。無印の引数を持つ通常の `def` では、edition の引数の既定の差も
保守的に breaking とします。型を解決しないので、scalar など実際には変わらない引数にも出る場合があります。
`--effective` は両方を依存込みで型検査し、alias 解決後の型、実際の引数規約、純粋性の呼び出し段階、
制約、region の対応を比べます。依存の読み込み・型検査に失敗した場合は診断で止まり、構文比較には戻しません。
このモードも、関数の本体の挙動・性能・`const` の値の互換性を証明するものではありません。
関数の本体の挙動、性能、型エイリアスの展開結果、その他の所有権の意味の変更は見つけられません。
`tsuzuri publish` は `api-diff` を呼びません。公開の前に、前の版のチェックアウトと比べてください。

```sh
git worktree add ../geometry-1.2.0 v1.2.0
tsuzuri api-diff ../geometry-1.2.0 .
```

## 現時点の制限

- LSP の補完が出す予約語と、VS Code 拡張の構文ハイライトは、edition を区別しません。現在の `2026` と `2027` では予約語の差はありません。
- `tsuzuri fmt` は、`Tsuzuri.toml` を入力のディレクトリ（ファイルなら親）でだけ探します。上位のディレクトリにあるマニフェストの `edition` は見ません。
- `W1005` は、非推奨の宣言の名前を書いた場所に出ます。型エイリアスを経由した使用は、エイリアスの定義の 1 件だけで、エイリアスを使う側には出ません。パターンの中の型注釈（`match` の腕の `| q: T ->`、ラムダの `\(p: T) ->`、`for` のパターン）に書いた名前には出ません。
