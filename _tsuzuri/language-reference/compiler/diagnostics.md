# 診断メッセージとエラーコード

コンパイラは、直せることを 1 回の実行でできるだけ集め、それぞれに安定したコードと位置を付けます。成功は終了コード 0、ソースやツールや実行の失敗は 1、引数の誤りは 2 です。警告は既定では終了コードを変えません。

コードの意味は、`src/diagnostic.rs`、`src/warnings.rs`、[言語仕様の診断](../../../docs/language.md#診断) で確認したものだけを載せてあります。

## この記事のポイント

- 人間向けの 1 件は `パス:行:列: 重要度[コード]: メッセージ` です。直す手がかりがある診断は、続けて関連する位置（`note:`）、直し方（`help:`）、機械的な修正案（`fix`）を出します。 <!-- rule: DIA-007 -->
- `--json` は stderr の 1 行 1 JSON です。配列にはまとめません。追加の情報は、値があるときだけキーが付きます。
- どの診断も、原因を表す安定した識別子 kind（`use_after_move` など）を持ちます。`tsuzuri explain <kind>` が原因と、誤りの例・直した例を出します。
- よくある誤りの形（`pair.0`、`=>` の腕、`println!` など）には、ヒントの表から `help` と修正案が付きます。
- 画面に出すのは 50 件まで、内部では重複を除いて 1000 件まで集めます。
- 字句エラーがあると構文解析へ進まず、構文エラーがあると型検査へ進みません。 <!-- rule: DIA-005 --> <!-- rule: DIA-006 -->
- コードの千の位が、構文、意味、ツールのどれかを大まかに分けます。

## 表示形式

位置は 1 始まりです。列は Unicode スカラー値の数で、タブも 1 文字です。スパンの `start` と `end` は、ファイル先頭からの UTF-8 バイトで、`end` は含みません。

テキスト形式では、問題が発生した行の内容と、該当する列を指すキャレット（`^`）を表示します。該当行が空行の場合は行の引用を表示しません。複数の診断メッセージの間には空行が挿入されます。

型が合わない足し算は、こう出ました。終了コードは 1 です。

```text
./Main.tz:1:35: error[E1003]: expected i64, found bool
  1 | def add :: i64 -> i64 = \n -> n + true
    |                                   ^
```

同じファイルの独立した式は、まとめて出ます。

```text
./Main.tz:2:17: error[E1003]: expected i64, found bool
  2 |     let a = n + true
    |                 ^

./Main.tz:3:21: error[E1003]: expected bool, found i32
  3 |     let b = false + 1
    |                     ^
```

<!-- rule: DIA-003 -->
警告も同じ形です。この `check` の終了コードは 0 でした。

```text
./Main.tz:2:9: warning[W1001]: unused local 'unused'; prefix it with '_' to silence this warning
  2 |     let unused = 1
    |         ^
  fix (certain): Prefix 'unused' with '_'
```

### 関連する位置・help・修正案

所有権、型の不一致、名前、呼び出し、網羅性、重複の診断の一部は、主の位置に続けて、原因になった別の位置と直し方を出します。ムーブした値を使うと、どこでムーブしたかを示します。

```text
def consume :: own string -> i64 = \s -> s.length

def main_value :: unit -> i64 = \_ ->
    let text = "hello"
    let a = consume text
    let b = consume text
    a + b

main_value ()
```

`check` の出力は次のとおりで、終了コードは 1 です。

```text
./Main.tz:6:21: error[E1012]: use of moved or partially moved value 'text'
  6 |     let b = consume text
    |                     ^
  note: value moved here
    --> ./Main.tz:5:21
  5 |     let a = consume text
    |                     ^
  help: borrow the value with 'ref text' or move it last
```

| 行 | 意味 |
| --- | --- |
| `  note: <説明>` と `    --> パス:行:列`、その行の引用 | 関連する位置。1 件の診断に 16 件まで。別のファイル（std を含む）の位置は、そのファイルの行と列で出します |
| `  help: <直し方>` | 1 行の直し方 |
| `  fix (<certain\|likely\|maybe>): <題>` | 機械的な修正案。8 件まで。`maybe` は提案で、そのまま正しいとは限りません |

網羅していない `match` は、足りない腕を足す修正案を持ちます。足す腕の本体は `unreachable ()` で、実際の処理に書き換える前提の `maybe` です。字下げはしないので、`match` の最後の腕の末尾に ` | Triangle _ -> unreachable ()` が付きます。足りない腕が複数あるときは、8 個までならまとめて 1 つの修正案にします（`Add the arms '| B -> unreachable () | C -> unreachable ()'`）。それより多いときや、腕として書けない形（レコード、配列の長さ、リスト、リテラル）が足りないときは、`help` だけです。

```text
./Main.tz:4:5: error[E1021]: match is not exhaustive; missing: Triangle _
  4 |     match s with
    |     ^
  help: add an arm for 'Triangle _', or end the match with a wildcard arm '| _ -> ...'
  fix (maybe): Add the arm '| Triangle _ -> unreachable ()'
```

### 未知の名前の候補

名前を解決できない 6 つの診断（`E1002` の `unknown value`・`module '…' has no function or union case`・`unknown function`、`E1004` の `unknown record, union, or type alias`、`E1007` の `unknown field`・`record '…' has no field`、`E1020` の `unknown union case or active pattern`）は、近い名前があると、それを `candidates` に入れ、`help` を `did you mean '…'?` にします。近さは編集距離（挿入・削除・置換・隣の 2 文字の入れ替えを 1 と数える）で、名前が 4 文字以下なら 1、5 文字以上なら 2 までです（2 文字未満の名前には探しません）。`unknown value` は、局所、同じモジュールの関数と定数、組み込み関数、ケース、（大文字で始まる名前なら）モジュール名と、モジュールを付ければ解決する名前（`length` に対する `Array.length`。距離 0 で先頭）を比べます。候補の `how` は、局所なら `local`、モジュールを付けるだけなら `qualify`、ほかは `similar` です。最も近い候補が 1 つに決まるとき（候補が 1 つか、先頭が 2 番目より近い）は、名前を置き換える `maybe` の修正案も付きます。近い名前がなければ、診断は以前と同じです。コード・メッセージ・主の位置は変わりません。

```text
./Main.tz:2:9: error[E1002]: unknown value 'totl'
  2 | let y = totl + 1
    |         ^
  help: did you mean 'total'?
  fix (maybe): Replace 'totl' with 'total'
```

追加の情報を持たない診断の出力は、主の位置の 3 行だけで、以前と同じです。どの診断が何を持つかは「エラーコード」の表にあります。`tsuzuri repl` は `note:` と `fix` を出しません（位置が REPL の生成するプログラムのものだからです）。

ソースコードの引用行は、エラー位置の列の前後を最大 120 文字に切り詰めて表示します。表示されるパスは、渡した入力からの相対パスであり、実際に問題が発生したモジュールのファイルパスを正確に示します。すべてのエラーを先頭の入力ファイル名にまとめてしまうことはありません。

引数のエラーは、ソースが無いのでパスが `<command line>` で、行と列は 1 です。

```text
<command line>:1:1: error[E2000]: missing .tz, .tt, or .tc input or project directory; use --help
```

## JSON の 1 行 1 オブジェクト

`--json` は、同じ診断を stderr へ 1 行ずつ出します。標準出力は、`run` のプログラム出力や `test` の結果に残します。

先の型エラーは、次の 1 行でした。

```text
{"severity":"error","code":"E1003","message":"expected i64, found bool","path":"./Main.tz","span":{"start":34,"end":38,"line":1,"column":35,"end_line":1,"end_column":39},"kind":"type_mismatch","expected":"i64","found":"bool"}
```

| フィールド | 意味 |
| --- | --- |
| <!-- rule: DIA-010 -->`severity` | `error`、`warning`、省略通知の `note` |
| `code` | `E1003` や `W1001`。省略通知には無い |
| `message` | 人間向けと同じ文。引用符と改行は JSON の文字列としてエスケープされる |
| `path` | ソースのパス。引数エラーは `<command line>` |
| `span.start` / `span.end` | UTF-8 バイトオフセット。`end` は排他 |
| `span.line` / `span.column` | 始まり。1 始まりの行と、Unicode スカラー値の列 |
| <!-- rule: DIA-002 -->`span.end_line` / `span.end_column` | 終わりの行と列 |

追加の情報は、5 つのキーの後ろに、次の順で、値があるときだけ付きます。`kind` はすべての診断が持ちます。

| フィールド | 意味 |
| --- | --- |
| `kind` | 診断の原因を表す安定した識別子（[kind と explain](#kind-と-explain)） |
| `related` | 関連する位置の配列。要素は `role`、`message`、`path`、`span`（主と同じ 6 キー）。位置を解決できない要素は省きます |
| `help` | 1 行の直し方 |
| `expected` / `found` | 型の不一致（`E1003`）の期待した型と実際の型。メッセージと同じ表示です |
| `candidates` | 候補の配列（`name`、`signature`、`how`）。現在の診断には付きません |
| `scope` | 見えるローカル変数の配列（`name`、`type`）。内側のスコープから、32 件まで |
| `fixes` | 修正案の配列。要素は `title`、`applicability`（`certain`・`likely`・`maybe`）、`edits`。`edits` の要素は `path`、`span`、`text` で、`span` の範囲（UTF-8 バイト、`end` は排他）を `text` に置き換えます。`start` と `end` が同じなら挿入です |

`role` は `moved_here`（ムーブした位置）、`borrowed_here`（借用を作った位置）、`borrow_used_here`（借用がまだ生きている理由になる使用）、`declared_here`（宣言）、`expected_because`（期待する型を決めた宣言）、`first_defined_here`（最初の定義）、`via_call`、`required_by`、`note` のどれかです。先のムーブの診断は、次の 1 行です。

```text
{"severity":"error","code":"E1012","message":"use of moved or partially moved value 'text'","path":"./Main.tz","span":{"start":157,"end":161,"line":6,"column":21,"end_line":6,"end_column":25},"kind":"use_after_move","related":[{"role":"moved_here","message":"value moved here","path":"./Main.tz","span":{"start":132,"end":136,"line":5,"column":21,"end_line":5,"end_column":25}}],"help":"borrow the value with 'ref text' or move it last"}
```

キーの名前と順序、`kind` と `role` と `applicability` の値は安定しています。`related` の `message`、`help`、修正案の `title` と `text` の文言は変わることがあります（[安定性と互換性の方針](../../../docs/stability.md#安定として保たないもの)）。

`fmt --check --json` だけ、形が少し違います。スパンは無く、パスと「整形されていない」ことだけです。終了コードは 1 でした。

```text
{"severity":"error","code":"E2000","message":"file is not formatted","path":"unformatted.tz","kind":"file_not_formatted"}
```

ツールがビルドのついでに出す警告、たとえばキャッシュを使えなかった通知は、`code` が `W2001`、`kind` が `tool_notice` で、スパンを持たない行になることがあります。`run --overflow-report --json` では、プログラムが成功したあと、整数の折り返しの報告の行が `W2001` の行になり、`kind` は `integer_overflow_wrapped`、`path` と `span` は折り返した演算の式です（メッセージの列は演算子の位置で、`span` は式全体です）（[整数の折り返しの報告](usage.md#整数の折り返しの報告)）。

```text
{"severity":"warning","code":"W2001","message":"integer overflow wrapped at ./Main.tz:1:45 (i32u add)","path":"./Main.tz","span":{"start":42,"end":47,"line":1,"column":43,"end_line":1,"end_column":48},"kind":"integer_overflow_wrapped"}
```
最後の一時ディレクトリを消せなかった、といった後始末の注意は、JSON モードでもプレーンテキストのままです。

`test --json` の結果は stderr ではなく stdout で、診断とは別のオブジェクトです。1 件は `type` が `test`、最後に `type` が `summary` の行が続きます。

```text
{"type":"test","index":0,"module":"Main","name":"adds numbers","status":"passed","duration_ms":234}
{"type":"summary","passed":1,"failed":0,"ignored":0,"duration_ms":792}
```

失敗した件には `failure` が足されます。`--list --json` は実行せず、`type`、`index`、`module`、`name`、`path`、UTF-16 の `range` を出します。

## kind と explain

コードは原因の大まかな分類で、同じ `E1002` が「未知の値」「モジュールにない関数」「型クラスにないメソッド」を表します。kind は、その中の 1 つの規則を表す識別子です。小文字と数字と `_` の 48 文字以内で、1 つの kind は 1 つのコードに属します。診断の `code` と重要度は kind から決まります。

| 例 | kind | コード |
| --- | --- | --- |
| ムーブした値の使用 | `use_after_move` | `E1012` |
| 生きている借用と競合するアクセス | `borrow_conflict` | `E1014` |
| 型の不一致 | `type_mismatch` | `E1003` |
| 未知の値 | `unknown_value` | `E1002` |
| モジュールにない関数 | `unknown_module_member` | `E1002` |

kind は `--json` の `kind` キーと、LSP の診断の `data.kind` に出ます。人間向けのテキスト出力には出しません。`tsuzuri explain <kind>` は原因（`## Why`）と、誤りの例（`## Bad`）・直した例（`## Good`）を出し、`tsuzuri explain <コード>` はそのコードの kind の一覧と直し方を出します（[コンパイラの使い方](usage.md#explain)）。全 kind は `tsuzuri explain --catalog` と、この記事の [kind の一覧](#kind-の一覧) にあります。

```text
$ tsuzuri explain use_after_move
use_after_move  E1012  error  since 0.1.0
Use of a moved value

## Why
Passing a value to a function that takes ownership, ...
```

kind は足すことはあっても、消したり、名前を変えたり、別のコードへ移したりしません。1 つの kind を 2 つに分けることはあります（元の kind は残りの条件に使い続けます）。説明の文言は保証しません（[安定性と互換性の方針](../../../docs/stability.md)）。

### ヒント

よくある誤りの形には、コンパイラのヒントの表から、直し方の `help` が付きます。ヒントは、診断の kind、メッセージの断片、診断の位置の前後の字句で選びます。たとえばタプルを `pair.0` で読むと、`E0002` の `expected an identifier` に次の `help` が付きます。

```text
./Main.tz:1:49: error[E0002]: expected an identifier
  1 | t :: (i64 * i64) -> i64 = \pair -> pair.0
    |                                         ^
  help: read a tuple element by pattern matching, as in 'match pair with | (first, _) -> first'
```

表は 40 件で、ほかに `(5: i64)` の型注釈、`Display.to_string`、`let new`、整数の `abs`、`return`、`fn f()`、`{` で始まる `union`・`for`・`while`、`println!`、`+=`、`::` での呼び出し、メソッド呼び出し、`struct`、`null`、`?`、`let ... in`、`and`、`pub`、`use`、`<T>`、`Option`、`int`・`float`・`str`、`=>` の腕、`|` のない腕、`True`・`False`、`void`、文字と文字列の引用符、`|x|` や `x =>` のラムダを扱います。

- ヒントは、検査に失敗した利用者のソースの診断にだけ付きます。標準ライブラリの診断と、成功した検査の警告には付きません。
- 位置の前後の字句で選ぶヒントは、診断が元から持つ一般的な `help`（たとえば未知の値の `check the spelling, ...`）を置き換えます。字句の条件がないヒントは、`help` のない診断にだけ付きます。
- 7 件のヒント（`Option`→`Maybe`、`float`→`f64`、`str`→`string`、`=>`→`->`、`True`→`true`、`False`→`false`、`void`→`()`）は、その字句を置き換える修正案も持ちます。修正案は `likely` か `maybe` で、`certain` にはしません。経験則なので、そのまま正しいとは限りません。

## 複数エラーと二次エラー

診断は、ソース、開始位置、終了位置、重要度、コード、メッセージで一意にします。同じ報告が 2 回出ることはありません。

画面に詳しく出すのは 50 件までです。内部では、重複を除いて 1000 件まで集めます。残りがあるときは、最後に件数だけを足します。

| 状況 | テキスト | JSON |
| --- | --- | --- |
| エラーが 50 件を超え、収集は 1000 件未満 | `error: 12 more errors not shown` | `{"severity":"note","message":"12 more errors not shown"}` |
| 収集上限の 1000 件に達した | `error: at least 950 more errors not shown` | `note` で、`at least` が付く |
| 警告だけの超過 | `warning: 12 more warnings not shown` | 同じ `note`。メッセージは warnings |

字句エラーがあるファイルは、構文解析へ進みません。構文エラーがあるプロジェクトは、型検査へ進みません。壊れた構文木を後ろの段階へ渡さないためです。回復は、括弧の外や、行頭の宣言の境界で行います。

関数のシグネチャが壊れているとき、その関数を呼んでいる側の二次エラーは抑えます。ほかの関数の検査は続けます。本体では、独立した部分式の型エラーを集め、壊れた値に依存する続きは抑えます。

所有権は、変数ごとに最初の不正な再利用を報告してから、ほかの変数を見ます。型が付いた関数なら、プロジェクトの別の場所に型エラーがあっても、ムーブ後の使用や借用の競合は報告します。借用の生存期間のような、型が全部決まってから見るエラーは、型エラーが消えた次の実行で初めて出ることがあります。1 回で全部出る保証はありません。

資源上限の `E1017`、診断が収集上限に達したとき、region や制約やコンピュテーション式の展開に失敗したときは、その関数本体の続きを打ち切ります。名前空間のように、続きを見ても安全でない段階では、そこまでに集めた診断を出して止まります。

エラーがあると、`build` と `run` は LLVM にもリンクにも進みません。既存の成果物は置き換えません。このとき警告は出しません。警告だけのときは、ファイルと位置の順で stderr へ出してから、次へ進みます。

```mermaid
flowchart TD
  lex["字句解析"] --> lexOk{"E0001 は無いか"}
  lexOk -->|ある| stopLex["構文解析へ進まない"]
  lexOk -->|無い| parse["構文解析"]
  parse --> parseOk{"E0002 や E0003 は無いか"}
  parseOk -->|ある| stopParse["型検査へ進まない"]
  parseOk -->|無い| types["型、名前、ABI"]
  types --> own["所有権と借用"]
  own --> gate{"エラーは残っているか"}
  gate -->|はい| stopGen["コード生成しない。終了コード 1"]
  gate -->|いいえ| warn{"警告があり deny か"}
  warn -->|はい| stopWarn["終了コード 1。生成しない"]
  warn -->|いいえ| gen["check 完了、または生成と実行"]
```

## エラーコード

エラーコードのプレフィックスと千の位が分類を表します。`E0`（`E0001`〜`E0004`）は字句解析と構文解析、`E1`（`E1001`〜`E1037`）は意味解析・型検査・所有権と借用などの言語仕様上のエラー、`E2`（`E2000`〜`E2009`）は CLI 引数・I/O・リンカー・実行時・依存の取得・edition の移行・知識グラフなどのツールと実行に関するエラーです。警告コードは `W1`（`W1001`〜`W1010`）およびツールの警告 `W2001`・`W2002` です。

| コード | 何を報告するか |
| --- | --- |
| <!-- rule: LEX-008 -->`E0001` | 字句解析 |
| `E0002` | 構文、AST の深さの上限、不正なビルダーの別名宣言（`@alias`）、`const` の置き場所の誤り（`const fn`・`const let`・`const and`・`const export def`、`@cpu` との併用）、ローカルな `let rec` の誤り（右辺がラムダ式でない、`let rec mut`、`let! rec`・`and! rec`、`@checked` を付けた `let rec`、`where` の `rec`、`use rec`）、マニフェストの `edition` が対応する値でないこと（`unsupported edition '2030'; supported editions: 2026`）。行頭の `#if` もここに落ちます |
| `E0003` | ソースファイルの大きさの上限（4 MiB） |
| `E0004` | edition 2027 が拒否する表記。素の `..` の範囲（`..=` か `..<`）、短い数値接尾辞（`86uy` は `86i8u`）、`&x`・`&mut x`・`*r`（`ref x`・`ref mut x`・`deref r`）、型の `&T`（`ref T`）、旧ラムダ `x -> e`（`\x -> e`）。kind は `*_rejected` の 6 種。修正案は付かず、edition 2026 の `W1007` の書き換えで移行する |
| `E1001`–`E1010` | 識別子名、名前の解決、型の不一致、演算子・関数呼び出し、レコードフィールド、公開 ABI、リテラル、型レイアウト |
| `E1011` | モジュール名、予約名、`std`、パッケージのパスと名前（同じ名前の別の取得元、git・registry のパッケージの許されない依存、互換の範囲が違う版の要求を含む）、シンボリックリンクの入力 |
| `E1012` | ムーブ後の使用、不正な代入、`let rec` の関数が借用している捕捉のムーブ（`cannot move 'text' out of a 'let rec' function, which borrows it`）。2027 の既定の読み取り引数から非 Copy 値を取り出す場合は `own` を案内します。ムーブ後の使用は、ムーブした位置（`moved_here`。ループの前の反復なら `in the previous iteration of the loop`）と `help` を持ちます |
| `E1013` | 所有者より長生きする借用（借用して捕捉した値を持つ `let rec` の関数を、その値の持ち主から返す場合を含む）、Task をまたぐ借用、Async の `let!` / `do!` / 開始をまたぐ借用、Task へ持ち込む `Rc` や `Sync` でない値の `Arc`、`Task.scope` で共有する `Sync` でない値、`Task.scope` や `Parallel.*` で共有する、借用を持った一時値、`Mutex.with_lock` の結果に含まれる借用、`Channel.send` の要素が持つ借用（`a channel item cannot hold a borrow`）、`Channel.bounded` の `Send` でない要素（`Channel.bounded` や `Mutex.create` を包んだジェネリック関数を `Send` でない型で使う場合も）。所有者より長生きする借用（ブロックの結果、代入、関数の戻り値、パターンと反復の束縛）は、所有者の宣言（`declared_here`）、借用した位置（`borrowed_here`）と `help` を持ちます |
| `E1014` | 借用の競合、不変な値への可変アクセス。借用の競合は、借用した位置（`borrowed_here`）、借用を生かしている後の使用（`borrow_used_here`）と `help` を持ちます。`let` で宣言した変数への可変アクセスは、その宣言（`declared_here`）と `help` を持ちます |
| `E1015` | 曖昧な型変数、無限型、多相の不正 |
| `E1016` | 型クラスとインスタンス。`Drop` を含む（std の型への利用者の `Drop` の instance は `E1016`。std のモジュールが自分で宣言した型には、std が書く） |
| `E1017` | コンパイラの資源上限。`tsuzuri fix` の 1 ラウンドの修正案が 20,000 件を超えた場合（`fix_candidate_limit`）と、`--overflow-report` の地点が 65,536 を超えた場合（`too_many_overflow_sites`）を含む |
| `E1018` | ファイル種別の規則違反（`.tc` 以外の `@alias` を含む）、未知のビルダー、必要なビルダー操作の未定義、ビルダー候補の曖昧・不在、GPU カーネルにできない構文（厳密な WGSL の `f32`・`f16`・64 bit の要素、緩い WGSL（`Gpu.init_relaxed`・`Gpu.map_relaxed`・`--emit wgsl-relaxed`）の `f64`・64 bit 整数・`bool` の要素・`f32`・`f16` から整数と `f32` と `f64` の間のキャスト、整数の除算と剰余を含む）、Vulkan 用の厳密な SPIR-V（`Gpu.map`・`--emit spirv`）の `f32` の `/`・`f64`・`f16`・`bool` の要素・整数の除算と剰余と `**`、緩い SPIR-V（`--emit spirv-relaxed`）の 64 bit 整数の要素と `f32` から整数へのキャスト、標準ライブラリからの不正な `export`、`.tt` と標準ライブラリの `bench` 宣言、`tsuzuri bench` 以外の出力でトップレベルのコード・`export def`・実行するテスト・`Drop` の実装から到達する `Bench.now` |
| `E1019` | `rec` の過不足、単独の `and` |
| `E1020` | パターンの形、OR パターンの束縛、共用体のペイロード。未知のケース・アクティブパターンは、近い名前の候補を持ちます（下の「未知の名前の候補」） |
| `E1021` | `match` と関数ガードの網羅性。足りないケースを示し、`help` を持ちます。`match ... with` の足りないケースが腕として書ける形（`_`、`true`・`false`、`()`、共用体のケース、それらのタプル）で 8 個以下なら、末尾にそれらの腕を足す `maybe` の修正案も持ちます |
| `E1022` | 他モジュールの `private`、公開 API からの private 型、不透明な標準ライブラリ型（`Map`、`Arena`、`Arena.Handle`、`Net.Address`、`Atomic`、`Mutex`、`Channel.Sender`、`Channel.Receiver` など）の直接構築・フィールド参照、std 専用の組み込み関数（`Os.__read`、`Net.__open`、`Arena.__next_id`、`Gpu.__open`、`Channel.__close_sender` など）の呼び出し |
| `E1023` | ループ外の `break` / `continue`、`finally` を越える脱出 |
| `E1024` | 型パラメーター、共用体と型エイリアスの名前、循環するエイリアス。共用体の重複したケースは、最初の定義（`first_defined_here`）を持ちます |
| `E1025` | `deriving` できない型クラスや要素、止まらない `Default`、`Encode`／`Decode` を導出しない型の `@json`、重なった `@json` の名前 |
| `E1026` | コンパイル時定数と `const def` のトラップ、循環、ステップ上限、`const def` の署名（値型でない引数や結果）と本体（使えない式）の規則違反、静的データの合計が 4 MiB を超える定数 |
| `E1027` | 条件付きインスタンスやスーパークラスの制約 |
| `E1028` | `dyn` にできない型クラス。理由はメッセージに入る |
| `E1029` | 純粋な宣言・関数値・class／instance の契約、または到達する処理の純粋性違反。宣言した呼び出し段階の引数を実装が途中までしか束縛しない場合も含む。`own` に参照・共有状態・利用者の `Drop` などを含む型を使った場合や、generic の特殊化で条件を満たさない場合も含む（[純粋関数](../values-and-functions/functions.md#純粋関数pure-def)） |
| `E1030` | 型つき穴 `?`・`?名前`（kind `typed_hole`）。穴ごとに 1 件で、期待される型（`expected`。決まらなければ `_`）、見える局所（`scope`）、その型の値を作れる候補（`candidates`。1 つの関数の先頭 16 個の穴だけ）と `help` を持ち、期待される型の局所が 1 つなら穴をそれに置き換える `maybe` の修正案を持ちます。穴は型の誤りとして回復するので、連鎖する誤りは出ません（[型つき穴](../types-and-type-inference/type-inference.md#型つき穴-)） |
| `E1031` | 効果の属性（`@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`）の表明を、推論した効果が破る。`extern` の呼び出しとたどれない呼び出しを含む。kind で 6 種に分かれる（[効果の表明](../values-and-functions/attributes.md#効果の表明no_allocno_copyno_trapterminates)） |
| `E1032` | 効果の属性の置き場の誤り。トップレベル `def` 以外・重複・`extern` の前・式の位置 |
| `E1033` | 契約の行（`@requires`・`@ensures`・`@old`）の誤り。置き場・`bool` でない条件・純粋でない条件・結果のラムダ・参照の結果・`const def` の契約（[契約](../values-and-functions/contracts.md)） |
| `E1034` | `tsuzuri prove`・`--contracts prove` が証明できない。反例・非対応の構文・時間切れ・ソルバーの失敗・ソルバーがない |
| `E1036` | マニフェストの権限の宣言（`[capabilities]`・依存の `capabilities`）が許さない権限を、パッケージのコードが使う。位置は使用の地点で、std の中の呼び出しの連鎖を添える（[権限](../organizing-tsuzuri/packages.md#権限capabilities)） |
| `E1037` | `@trusted` の誤り。`extern` でない宣言・未知の性質・重複した性質・`extern type` の性質・`@trusted` の重複（[信頼の印](../values-and-functions/attributes.md#trusted-extern-の信頼の印)） |
| `E2000` | 引数（`tsuzuri api-diff` の入力が 2 つでない場合を含む）、拡張子、組み合わせ、`--target` の名前が一覧にないこと（32-bit と対応する 5 環境以外を含む）、ビルド機で動かせない target の `run`・`test` に `TSUZURI_RUNNER` がないこと、その target の `test --coverage`・`test -g`・`bench`（[対応プラットフォーム](platforms.md#診断)）、`--allocator small` と WASM の target・`--trap-mode return`・`--freestanding` の組み合わせ（native の 64-bit のサイズ区分 allocator です）、`--wasm-feature net` の条件の不備（wasm32 の object・LLVM IR・WASM・bindings-js でない、`jspi` がない、threads や WASI との併用）、`net` なしの既定 wasm32 で OS API や `Net` のソケットに到達したビルド、`Async.block_on` なしで `Net` の非同期操作（`connect_async` など）に到達した native のビルド、Async 実行器に必要な opt-in の不足・threads との組み合わせ、`--freestanding` での `Mutex` と `Channel`（タスクのランタイムが要ります）、`--wasm-feature webgpu` の対象外の出力や threads・`--wasm-host`・JavaScript バインディングとの組み合わせ、`--wasm-feature tail-call` の対象外の出力（wasm32 と wasm64 の object・LLVM IR・WASM 以外）や `--backend fast` の wasm32 との組み合わせ、WGSL と SPIR-V の出力（`--emit wgsl`、`wgsl-relaxed`、`spirv`、`spirv-relaxed`）と `--target`・`-O`・`--cpu`・デバッグ・WASM 機能・`--allocator` との組み合わせ、環境変数 `TSUZURI_RANGE_FLAGS` が `off`・`proven`・`audit` のどれでもないこと（IR を出すコマンドだけ。`check` と `lsp` は読みません）、環境変数 `TSUZURI_STRINGS` が `utf16`・`compact` のどれでもないこと（同じく IR を出すコマンドだけ）、`tsuzuri watch` にコマンド名（`check`・`build`・`test`）がないこと、`tsuzuri serve` の引数（ディレクトリが 1 つでない、`--idle-timeout` が 1 から 86400 の整数でない、`--status` と `--stop` の同時指定）と設定（`TSUZURI_SERVER_DIR` が相対パスか空、ソケットのパスが長すぎる、Unix 以外）、`--no-server` を `check`・`build` 以外に付けること、`tsuzuri kg` の照会の ID が曖昧・未知なこと（`kg_ambiguous_id`・`kg_unknown_id`。終了コード 1。[知識グラフ](kg.md#id-の解決)）と `kg impact --git`・`test --affected` の git のリビジョンが不正・未知なこと（未知は `unknown_git_revision`。終了コード 1）、`tsuzuri kg diagram` の引数・ID の誤り・図に要る事実がないこと・図が大きすぎること（`diagram_*`。[図表の生成](diagrams.md#誤り)）、`tsuzuri mcp` の引数の誤り（`mcp_bad_arguments`）、`tsuzuri find` の引数と読めない問い合わせの型（`find_bad_arguments`）・どのモジュールにも当たらない `--module`（`find_unknown_module`。どちらも終了コード 2。[find](usage.md#find)） |
| `E2001` | ファイルの読み書き。ディレクトリを渡して `Main.tz` が無いときもこれです。`TSUZURI_RUNNER` のプログラムを起動できないときもこれで、メッセージにその名前が入ります。`tsuzuri serve` では、ソケットのディレクトリがほかの利用者に開かれている・シンボリックリンク・他人のものであること、同じプロジェクトのサーバーがすでに動いていること、ソケットの場所にソケットでないものがあること、`--status`・`--stop` の相手のサーバーがないこと、`--stop` の相手が要求を実行中で、終わるまで止まらないこともこれです |
| `E2002` | LLVM やリンカーの失敗。未解決シンボル、Windows の target で OS API に到達した場合、同梱ツールチェーンが扱わない組み合わせ（Windows 以外のホストからの MSVC のリンク、sysroot）を含む |
| `E2003` | 出力ファイルの保護。既存の成果物を壊さないための失敗 |
| `E2004` | 入口の要件。`Main.tz` が無い実行ファイル、export も `main` も無い WASM、`export def` の無いバインディングと共有ライブラリ、`export` が 1 つでない WGSL・SPIR-V の出力、`main` のシグネチャ違反、`def main` とトップレベル実行の併用、`Main.tz` 以外のモジュールでのトップレベル実行 |
| `E2005` | 実行時の異常終了。トラップ位置、0 以外の `def main`、スタック枯渇の推定、`tsuzuri bench` の失敗（トラップ、時間切れ、負の時間）を含む。契約の違反は kind `contract_requires`・`contract_ensures` で、位置は契約の行 |
| `E2006` | 言語内テストの失敗 |
| `E2007` | 依存の取得と検証の失敗。`Tsuzuri.lock` の欠落・古い項目・形式違反、ストアにない依存や内容が SHA-256 と違う依存、`git` の欠落・失敗、取得したリポジトリのシンボリックリンク・submodule・危険なパス・大文字小文字だけ違うパス、registry の index にない版、index の項目と違うパッケージ、`publish` で commit とディレクトリの内容が違うこと（[パッケージ](../organizing-tsuzuri/packages.md#git-依存と-tsuzurilock)） |
| `E2008` | `tsuzuri fix` の中止（修正案を適用すると診断が増える `fix_verification_failed`、実行中にファイルが変わった `fix_source_changed`、置き換えの失敗 `fix_write_failed`。どれもファイルは書かないか元に戻します）と、`tsuzuri fix --edition` による edition の移行の失敗。`--from` とマニフェストの版の食い違い、古い版への移行、移行後のソースが大きすぎること、マニフェストの更新や置き換えの失敗（[コンパイラの使い方](usage.md#fix)） |
| `E2010` | `tsuzuri mcp` が続けられない: 根がディレクトリでない `mcp_root_unusable`、クライアントとの入出力の失敗 `mcp_io_failed`、作業のスレッドを作れない `mcp_worker_failed`。クライアントのメッセージの誤りはこのコードではなく、JSON-RPC のエラーで答えて続けます（[MCP サーバー](mcp.md)） |
| `E2009` | 知識グラフ（`tsuzuri kg`）を読めない・置けない。置き場がない `kg_missing`、ソースやコンパイラが変わった `kg_stale`、`manifest.json` やファイルの欠落・不一致・グラフの不変条件の違反 `kg_broken`、置き場が知識グラフでない（マーカー `.tsuzuri-kg` がない）かソースを含む `kg_not_ours`、型検査の通らないプロジェクトの知識グラフ `kg_incomplete`。`check`・`build`・`run`・`test` が自動で書くときの失敗は、この診断ではなく警告 1 行（`W2001`、`kg not updated: …`）で、終了コードは変わりません（[知識グラフ](kg.md)） |

`E1001` から `E1010` は型検査・意味解析の基本的なエラーに割り当てられています。具体的には、`E1001` が識別子名の重複（1 つの `let rec` グループに同じ名前を 2 度書く場合を含む。関数のシグネチャと定義、レコード、レコードのフィールド、引数の重複は最初の定義 `first_defined_here` を持ちます）、`E1002` が未知の値・関数・定数・ケースの名前（未知の値は、見えるローカル変数の `scope` と `help` を持ちます）、`E1003` が型の不一致（expected ... found ...。`expected`・`found` を持ち、`let` の型注釈・関数の結果の型・名前付きの関数の引数の型が期待を決めたときは、その宣言 `expected_because` を持ちます。引数が足りない呼び出しには `help` が付きます）、`E1004` が曖昧な識別子や名前解決エラー（複数のビルダーが宣言する別名の曖昧さを含む）、`E1005` が関数呼び出しや演算子インスタンスの不備（関数値が捕捉できない値の捕捉を含み、型変数 `'a` を含む型の値を捕捉する `let rec` もここです。関数でない値の呼び出しには `help` が付きます）、`E1006` が関数適用の引数の個数の不一致（アクティブパターンの追加引数や `Owned.function` に渡すラムダの引数の数を含む。引数が多すぎるときは `help` が付きます）、`E1007` がレコードフィールドの不備（未知のフィールドは、レコードのフィールドを並べた `help` を持ちます）、`E1008` が公開 ABI や extern の制約違反、`E1009` が数値リテラルの不正、`E1010` が型レイアウトの制限です。詳細な理由は各診断メッセージで示されます。

### kind の一覧

コードごとの全 kind です。`tsuzuri explain --catalog` と同じ内容で、`tests/explain.rs` の `the_docs_list_every_kind` が登録簿との一致を検査します。kind を足したら、そのテストの失敗メッセージにある表を、次の 2 つのマーカーの間に貼ります。

<!-- generated:diagnostic-kinds -->
| コード | kind |
| --- | --- |
| `E0001` | `bigint_suffix_with_fraction`, `byte_literal_not_ascii`, `byte_suffix_on_interpolated_string`, `byte_suffix_on_unsupported_literal`, `char_literal_not_one_unit`, `comment_in_interpolation_hole`, `expected_digit`, `float_suffix_on_hex_or_binary_literal`, `format_field_out_of_range`, `format_precision_on_integer`, `format_type_needs_precision`, `interpolation_hole_spans_lines`, `invalid_format_spec`, `invalid_numeric_literal`, `invalid_string_escape`, `invalid_type_variable_start`, `invalid_unicode_scalar_escape`, `numeric_separator_misplaced`, `raw_newline_in_string`, `single_closing_brace_in_interpolation`, `unexpected_character`, `unterminated_block_comment`, `unterminated_interpolation_hole`, `unterminated_string_literal`, `utf8char_not_one_scalar` |
| `E0002` | `alias_name_on_same_line`, `angle_bracket_spacing`, `builder_alias_is_contextual_keyword`, `builder_alias_name_case`, `checked_let_rec`, `class_needs_one_type_parameter`, `computation_match_without_arms`, `computation_nesting_too_deep`, `const_export_order`, `constraint_line_indentation`, `cpu_attribute_on_const_def`, `definition_without_signature`, `do_bang_outside_task`, `duplicate_builder_alias`, `duplicate_capability`, `duplicate_cpu_target`, `duplicate_json_attribute`, `duplicate_manifest_key`, `dyn_with_type_arguments`, `empty_interpolation_hole`, `empty_region_list`, `expected_binding_end`, `expected_body_expression`, `expected_builder_alias`, `expected_computation_body`, `expected_cpu_target`, `expected_dyn_class`, `expected_expression`, `expected_expression_after_attribute`, `expected_identifier`, `expected_interpolation_close`, `expected_kind`, `expected_line_break`, `expected_link_name_string`, `expected_pattern`, `expected_single_type_argument`, `expected_token`, `expected_type_close`, `expected_union_case_name`, `expected_where_bindings`, `export_on_implementation`, `expression_nesting_too_deep`, `extern_type_with_body`, `extern_type_with_link_name`, `for_loop_needs_identifier`, `header_path_on_same_line`, `header_path_spacing`, `integer_literal_too_large`, `invalid_capabilities`, `invalid_dependency_form`, `invalid_dependency_native_flag`, `invalid_dependency_path`, `invalid_fixed_array_length`, `invalid_git_dependency`, `invalid_json_attribute`, `invalid_link_name_text`, `invalid_native_library_name`, `invalid_native_path`, `invalid_native_section`, `invalid_package_field`, `invalid_registry_dependency`, `invalid_registry_setting`, `invalid_wasm_size`, `lambda_without_parameters`, `length_parameter_type`, `let_function_needs_lambda`, `let_rec_needs_lambda`, `list_type_with_length`, `malformed_constraint_line`, `malformed_multi_case_active_pattern`, `malformed_new_collection`, `manifest_section_order`, `manifest_syntax_error`, `match_without_arms`, `misplaced_checked_attribute`, `misplaced_const_modifier`, `misplaced_cpu_attribute`, `misplaced_doc_comment`, `misplaced_json_attribute`, `misplaced_namespace_declaration`, `misplaced_region_list`, `misplaced_using_declaration`, `missing_capabilities_allow`, `missing_package_field`, `missing_registry_index`, `module_declaration_unsupported`, `mut_as_expression`, `mutable_let_rec`, `mutable_use_binding`, `negative_pattern_needs_number`, `pattern_nesting_too_deep`, `prefix_keyword_operand`, `range_takes_one_step`, `recursive_let_bang`, `recursive_where_binding`, `return_not_last_in_computation`, `return_outside_task_end`, `signature_uses_def`, `signature_uses_double_colon`, `signature_without_definition`, `slice_must_be_borrowed`, `slice_range_needs_end`, `slice_without_bounds`, `stray_and_member`, `syntax_nesting_too_deep`, `too_many_active_pattern_cases`, `too_many_dyn_classes`, `too_many_extern_link_names`, `too_many_function_parameters`, `too_many_interpolation_holes`, `too_many_lambda_parameters`, `too_many_tuple_elements`, `too_many_type_arguments`, `type_parameters_need_angle_brackets`, `typed_and_needs_body`, `unknown_capability`, `unknown_cpu_target`, `unknown_manifest_key`, `unsupported_edition`, `use_bang_in_and_group`, `use_bang_outside_computation`, `where_binding_indentation` |
| `E0003` | `source_too_large` |
| `E0004` | `bare_range_in_for_rejected`, `legacy_lambda_rejected`, `short_literal_suffix_rejected`, `symbolic_borrow_rejected`, `symbolic_deref_rejected`, `symbolic_reference_type_rejected` |
| `E1001` | `duplicate_and_binding`, `duplicate_derived_class`, `duplicate_export_name`, `duplicate_extern_declaration`, `duplicate_function_definition`, `duplicate_function_signature`, `duplicate_name` |
| `E1002` | `class_has_no_method`, `unknown_function`, `unknown_length_constant`, `unknown_module_member`, `unknown_union_case`, `unknown_value` |
| `E1003` | `active_recognizer_result`, `fixed_array_literal_length`, `format_spec_type_mismatch`, `format_spec_type_unknown`, `index_not_integer`, `type_mismatch` |
| `E1004` | `ambiguous_builder_alias`, `ambiguous_name`, `ambiguous_pattern_name`, `dyn_needs_type_class`, `empty_list_needs_annotation`, `expected_length`, `length_used_as_type`, `not_a_record_type`, `repeated_module_name`, `std_item_missing`, `type_argument_count`, `type_used_as_length`, `unknown_type_name` |
| `E1005` | `atomic_element_type`, `call_non_function`, `capture_exclusive_in_reusable_function`, `capture_rc_in_function_value`, `capture_sync_primitive_in_function_value`, `capture_unsendable_in_function_value`, `collection_element_exclusive_borrow`, `cpu_function_wide_vector_boundary`, `deref_non_reference`, `exclusive_slice_whole_array_parameter`, `fixed_array_init_annotation`, `fixed_slice_source`, `for_over_fixed_array`, `for_source_not_iterable`, `function_constraint_unresolvable`, `function_constraint_unsatisfied`, `index_non_indexable`, `invalid_length_constant`, `misplaced_mut_convention`, `misplaced_own_convention`, `mut_parameter_needs_edition_2027`, `new_on_fixed_array`, `no_instance`, `no_instance_for_operator`, `no_wider_integer`, `range_as_value`, `rec_function_captures_generic`, `record_update_non_record`, `shared_container_exclusive_borrow`, `simd_lane_count_mismatch`, `simd_operation_vector_type`, `slice_non_array`, `slice_of_fixed_array`, `std_representation_mismatch`, `unsized_slice_type_position`, `use_requires_drop` |
| `E1006` | `active_pattern_argument_count`, `active_recognizer_parameters`, `definition_parameter_count_mismatch`, `instance_method_parameter_count`, `owned_function_parameter_count`, `too_many_arguments_applied` |
| `E1007` | `field_access_non_record`, `missing_record_fields`, `record_has_no_field`, `unknown_field` |
| `E1008` | `conflicting_extern_declaration`, `export_active_recognizer`, `export_on_extern_declaration`, `extern_callback_argument`, `extern_callback_call_form`, `extern_declaration_constraints`, `invalid_extern_symbol`, `invalid_wasm_import_module`, `reserved_extern_symbol`, `unsupported_export_signature`, `unsupported_extern_signature` |
| `E1009` | `decimal_literal_out_of_range`, `float_literal_out_of_range`, `float_literal_type`, `integer_literal_out_of_range`, `integer_literal_type`, `invalid_bigint_digit`, `invalid_float_literal` |
| `E1010` | `fixed_array_too_long`, `infinite_recursive_layout`, `layout_nesting_limit`, `recursive_union_without_base_case`, `value_layout_too_large` |
| `E1011` | `ambiguous_package_root`, `conflicting_dependency_source`, `cyclic_package_dependency`, `dependency_kind_not_allowed`, `dependency_name_mismatch`, `dependency_path_not_relative`, `duplicate_dependency`, `duplicate_module`, `incompatible_dependency_versions`, `invalid_module_filename`, `invalid_module_name`, `invalid_package_name`, `invalid_package_namespace`, `invalid_publish_version`, `invalid_source_path`, `invalid_std_path`, `invalid_using_declaration`, `manifest_not_regular_file`, `module_namespace_collision`, `module_source_not_regular_file`, `publish_non_registry_dependency`, `reserved_package_namespace`, `reserved_std_namespace`, `source_filename_case_mismatch`, `source_outside_package_root`, `source_without_filename`, `symlink_not_followed` |
| `E1012` | `assign_to_immutable_place`, `drop_pass_exclusive_value`, `drop_replace_whole_value`, `move_out_of_collection_element`, `move_out_of_drop_type`, `move_out_of_owned_function`, `move_out_of_rec_function`, `move_out_of_reference_binding`, `update_drop_type_value`, `use_after_move` |
| `E1013` | `assignment_extends_borrow`, `borrow_across_async_boundary`, `borrow_field_without_region`, `borrow_of_temporary`, `borrow_outlives_binding`, `borrow_outlives_block`, `borrowed_union_payload`, `callback_contract_unsatisfied`, `channel_item_holds_borrow`, `const_temporary_borrow_escapes`, `dyn_holds_borrow_without_region`, `dyn_holds_exclusive_borrow`, `exclusive_borrow_behind_region`, `exclusive_borrow_field_shape`, `exclusive_field_regions_missing`, `exclusive_field_without_region`, `exclusive_region_reused`, `field_mixes_regions`, `invalid_region_names`, `misplaced_region_quantifier`, `multiple_regions_on_value`, `must_apply_directly`, `mutable_record_field`, `mutable_region_quantified_parameter`, `mutex_result_retains_borrow`, `named_region_in_function_type`, `named_region_in_local`, `nested_borrow_unsupported`, `owned_function_captures_reference`, `parallel_callback_borrows_environment`, `parallel_input_borrows_environment`, `parallel_result_retains_borrow`, `reference_region_count`, `reference_without_owner`, `region_already_declared`, `region_belongs_to_function`, `region_callback_stores_exclusive_input`, `region_count_mismatch`, `region_on_non_borrow`, `region_quantifier_target`, `result_region_mismatch`, `result_region_missing`, `result_region_without_input`, `self_referential_store`, `store_local_borrow_through_reference`, `store_region_mismatch`, `store_region_unnamed`, `task_capture_retains_borrow`, `task_requires_owned_value`, `task_requires_send`, `task_requires_sync`, `task_result_is_reference`, `task_result_retains_borrow`, `task_scope_shares_borrowed_environment`, `undeclared_region`, `union_payload_exclusive_borrow`, `unused_region` |
| `E1014` | `active_pattern_mutates_input`, `borrow_conflict`, `exclusive_slice_source`, `move_reborrowed_exclusive_reference`, `mutate_immutable_value`, `mutate_through_shared_reference`, `replace_exclusive_slice` |
| `E1015` | `ambiguous_polymorphic_use`, `concrete_integer_required`, `concrete_type_required`, `constructor_kind_mismatch`, `constructor_variable_kind_annotation`, `dyn_dispatch_result_type`, `dyn_of_arity`, `dyn_of_needs_expected_type`, `expected_type_constructor`, `extern_signature_not_concrete`, `extern_type_as_constructor`, `fixed_array_length_unknown`, `higher_kinded_alias`, `higher_order_kind_argument`, `infinite_inferred_type`, `instance_head_not_constructor`, `internal_compact_string_runtime`, `internal_rec_binding_not_lambda`, `internal_rec_group_unbound`, `internal_rec_not_bound_by_block`, `internal_unlowerable_ir`, `missing_type_arguments`, `polymorphic_entry_point`, `ref_operand_inferred_late`, `scalar_as_type_constructor`, `simd_vector_type_unknown`, `superclass_kind_mismatch`, `type_constructor_not_applied`, `type_variable_absent_from_signature`, `type_variable_kind_conflict` |
| `E1016` | `builtin_instance_override`, `class_constraint_arity`, `class_method_constraint`, `class_method_type_variable`, `class_without_methods`, `default_body_unknown_method`, `drop_instance_constraint`, `drop_instance_generic`, `drop_instance_target`, `explicit_drop_call`, `field_type_constraint`, `format_instance_target`, `instance_missing_method`, `instance_unknown_method`, `internal_effective_api_unavailable`, `missing_explicit_constraint`, `overlapping_instance`, `unknown_type_class` |
| `E1017` | `and_group_too_large`, `capability_analysis_limit`, `class_constraint_limit`, `contract_line_limit`, `deriving_expression_limit`, `deriving_instance_limit`, `effect_summary_limit`, `exhaustiveness_search_limit`, `fix_candidate_limit`, `function_constraint_limit`, `git_dependency_too_large`, `gpu_effect_analysis_limit`, `gpu_kernel_limit`, `header_path_too_long`, `instance_overlap_limit`, `instance_resolution_cycle`, `instance_resolution_limit`, `lockfile_too_large`, `loop_ownership_limit`, `manifest_too_large`, `module_discovery_limit`, `module_path_limit`, `namespace_path_too_long`, `or_pattern_alternatives_limit`, `package_graph_limit`, `pattern_alternatives_limit`, `polymorphic_constraint_limit`, `polymorphic_recursion_growth`, `polymorphic_type_expansion_limit`, `prove_limit`, `pure_callback_proof_limit`, `pure_definition_reach_limit`, `pure_value_validation_limit`, `qualified_name_too_long`, `recursive_type_changes_arguments`, `recursive_type_expansion_limit`, `registry_index_too_large`, `registry_resolution_limit`, `sequence_loop_depth_limit`, `specialization_limit`, `superclass_resolution_limit`, `too_many_dependencies`, `too_many_loop_exits`, `too_many_overflow_sites`, `too_many_raise_points`, `too_many_record_regions`, `too_many_region_parameters`, `too_many_trap_locations`, `type_alias_expansion_limit` |
| `E1018` | `alias_outside_builder_file`, `bench_clock_outside_bench`, `builder_missing_operation`, `builder_without_operations`, `cannot_infer_builder`, `class_outside_class_file`, `computation_without_builder`, `declaration_in_class_file`, `gpu_buffer_element_type`, `gpu_callback_not_known`, `gpu_kernel_effect`, `gpu_kernel_missing`, `gpu_kernel_recursion`, `gpu_kernel_signature`, `gpu_operation_not_direct`, `gpu_unsupported_construct`, `gpu_unsupported_operator`, `gpu_unsupported_type`, `implicit_task_needs_builder`, `no_builder_for_value`, `std_declares_test`, `std_exports_function`, `unknown_computation_builder` |
| `E1019` | `and_without_recursive_group`, `mismatched_recursive_group`, `missing_rec_marker` |
| `E1020` | `active_argument_not_value`, `active_pattern_case_count`, `active_pattern_case_name`, `array_pattern_on_fixed_array`, `case_pattern_missing_payload`, `case_pattern_payload_shape`, `case_pattern_unexpected_payload`, `duplicate_active_pattern_case`, `expression_in_pattern`, `multi_case_active_pattern_result_type`, `null_pattern`, `or_pattern_binding_mismatch`, `partial_active_pattern_case_count`, `qualified_pattern_binding`, `record_pattern_unknown_type`, `reserved_active_result_variable`, `unknown_case_or_active_pattern` |
| `E1021` | `non_exhaustive_match` |
| `E1022` | `opaque_type_representation`, `private_builder_operation`, `private_export`, `private_name_access`, `private_on_unsupported_declaration`, `private_std_primitive`, `private_type_in_public_api` |
| `E1023` | `jump_outside_loop` |
| `E1024` | `alias_argument_count`, `cyclic_type_alias`, `duplicate_type_parameter`, `duplicate_union_case`, `invalid_type_parameter_name`, `type_name_case`, `undeclared_length_parameter`, `undeclared_type_variable`, `union_case_name_case`, `unused_type_parameter` |
| `E1025` | `default_derive_recursive`, `derived_instance_invalid`, `deriving_unavailable`, `duplicate_json_name`, `json_name_without_deriving`, `underivable_class` |
| `E1026` | `const_aggregate_operator`, `const_assignment_target`, `const_borrow_not_allowed`, `const_call_depth_limit`, `const_callee_not_const`, `const_checked_operation`, `const_condition_not_bool`, `const_cycle`, `const_evaluation_trapped`, `const_expression_not_allowed`, `const_for_loop_form`, `const_function_value`, `const_limit_exceeded`, `const_new_collection_form`, `const_operation_not_allowed`, `const_partial_application`, `const_pipe_not_allowed`, `const_signature_invalid`, `const_static_data_too_large`, `const_unbuildable_type`, `const_unsupported_operation`, `constant_type_not_concrete` |
| `E1027` | `class_context_not_a_class`, `instance_check_failed`, `instance_context_variable`, `instance_missing_superclass`, `superclass_cycle`, `superclass_type_variable`, `undeclared_default_constraint`, `undeclared_instance_constraint`, `unknown_class_in_context` |
| `E1028` | `dyn_incompatible_class` |
| `E1029` | `impure_builtin_for_pure_contract`, `impure_callback_for_pure_contract`, `impure_operation_in_pure_def`, `impure_pure_function_value`, `pure_function_type_parameter_not_allowed`, `pure_implementation_partial_parameters`, `pure_method_mutable_reference`, `pure_on_non_function_type`, `pure_parameter_not_allowed`, `pure_returned_function_contract`, `pure_specialization_own_parameter`, `pure_value_captures_unknown_callback`, `pure_value_unprovable` |
| `E1030` | `typed_hole` |
| `E1031` | `effect_alloc_in_no_alloc`, `effect_copy_in_no_copy`, `effect_diverge_in_terminates`, `effect_extern_in_attribute`, `effect_trap_in_no_trap`, `effect_unknown_callee` |
| `E1032` | `effect_attribute_duplicate`, `effect_attribute_misplaced`, `effect_attribute_on_extern`, `effect_attribute_unavailable` |
| `E1033` | `contract_ensures_reference_result`, `contract_impure`, `contract_misplaced`, `contract_not_bool`, `contract_old_misplaced`, `contract_old_nested`, `contract_old_not_copy`, `contract_old_operand`, `contract_on_const`, `contract_result_lambda`, `contract_writes` |
| `E1034` | `prove_counterexample`, `prove_solver_error`, `prove_solver_missing`, `prove_unknown`, `prove_unsupported` |
| `E1036` | `capability_denied` |
| `E1037` | `trusted_duplicate_property`, `trusted_repeated`, `trusted_type_property`, `trusted_unknown_property`, `trusted_wrong_target` |
| `E2000` | `alloc_needs_counting_allocator`, `audit_usage`, `bindgen_annotation_error`, `bindgen_argument_error`, `cannot_derive_namespace`, `debug_info_without_source_map`, `diagram_bad_option`, `diagram_format_unsupported`, `diagram_needs_id`, `diagram_too_large`, `diagram_unavailable`, `diagram_unknown_id`, `diagram_unknown_kind`, `diagram_usage`, `diagram_wrong_id_kind`, `duplicate_link_input`, `duplicate_option`, `edition_option_with_manifest`, `environment_setting_invalid`, `fast_backend_unavailable`, `fast_backend_unsupported_option`, `file_not_formatted`, `find_bad_arguments`, `find_unknown_module`, `fix_bad_arguments`, `fix_edition_unsupported`, `fix_edition_with_applicability`, `fix_maybe_needs_dry_run`, `foreign_target_not_runnable`, `formatter_changed_semantics`, `formatter_invalid_output`, `guide_bad_arguments`, `guide_edition_unsupported`, `guide_manifest_missing`, `guide_unknown_std_module`, `guide_unknown_topic`, `index_out_of_range`, `input_count_invalid`, `input_extension_invalid`, `invalid_library_name`, `invalid_namespace_argument`, `invalid_publish_git_rev`, `invalid_publish_git_url`, `invalid_range_flag_setting`, `invalid_thread_setting`, `invalid_trap_source_map`, `kg_ambiguous_id`, `kg_unknown_id`, `link_input_limit`, `mcp_bad_arguments`, `missing_input`, `native_link_setting_not_allowed`, `net_async_needs_block_on`, `new_project_folder_not_empty`, `new_project_io_failure`, `no_sources_found`, `option_conflict`, `option_invalid_for_command`, `option_invalid_value`, `option_missing_value`, `option_requires_build`, `option_requires_option`, `option_unsupported_platform`, `output_extension_invalid`, `project_manifest_missing`, `prove_unknown_function`, `repl_command_usage`, `repl_declaration_not_allowed`, `repl_unknown_command`, `script_wrong_file_kind`, `trap_info_without_source_map`, `unknown_explain_target`, `unknown_git_revision`, `unknown_option`, `unknown_target`, `usage_error`, `wasm_net_needs_opt_in`, `wasm_os_api_needs_opt_in`, `wasm_test_runner_async` |
| `E2001` | `build_server_control_failed`, `build_server_start_failed`, `foreign_runner_not_startable`, `input_not_regular_file`, `input_not_utf8`, `input_path_not_utf8`, `input_too_large`, `io_failure`, `link_input_unreadable`, `repl_stream_failure`, `temp_directory_failed` |
| `E2002` | `bindgen_unreadable_ast`, `bindgen_unsupported_target`, `bitcode_symbol_not_kept`, `external_tool_failed`, `external_tool_not_runnable`, `external_tool_output_too_large`, `external_tool_read_failed`, `external_tool_wait_failed`, `fast_backend_unsupported_construct`, `invalid_coverage_counters`, `platform_unsupported_api`, `target_attributes_not_applicable`, `target_attributes_unreadable`, `toolchain_missing_feature` |
| `E2003` | `bindgen_output_not_generated`, `doc_filename_collision`, `doc_output_changed`, `doc_output_contains_sources`, `doc_output_invalid`, `doc_output_not_owned`, `doc_publication_failed`, `format_target_not_regular_file`, `output_has_source_extension`, `output_not_regular_file`, `output_overwrites_input`, `output_overwrites_link_input`, `output_publication_failed`, `source_changed_during_formatting` |
| `E2004` | `ambiguous_entry_point`, `builder_file_toplevel_code`, `entry_point_missing`, `entry_point_result_type`, `invalid_main_signature`, `spirv_kernel_count`, `toplevel_code_outside_main` |
| `E2005` | `benchmark_failed`, `contract_ensures`, `contract_requires`, `program_exited_nonzero`, `program_terminated`, `program_time_limit_exceeded` |
| `E2006` | `tests_failed` |
| `E2007` | `file_url_after_https`, `git_case_collision`, `git_command_failed`, `git_missing_manifest`, `git_non_regular_entry`, `git_source_not_utf8`, `git_tree_unreadable`, `git_unsafe_path`, `git_version_unsupported`, `invalid_lockfile`, `lockfile_checksum_mismatch`, `lockfile_entry_missing`, `lockfile_missing`, `lockfile_not_regular_file`, `lockfile_version_mismatch`, `package_files_unreadable`, `package_not_downloaded`, `package_store_unavailable`, `package_store_unusable`, `published_package_mismatch`, `registry_checksum_mismatch`, `registry_dependency_not_selected`, `registry_entry_mismatch`, `registry_index_invalid`, `registry_index_missing`, `registry_package_missing`, `registry_version_missing` |
| `E2008` | `fix_source_changed`, `fix_verification_failed`, `fix_write_failed`, `migration_edition_backwards`, `migration_edition_mismatch`, `migration_invalid_edit`, `migration_manifest_update_failed`, `migration_source_too_large`, `migration_write_failed` |
| `E2009` | `kg_broken`, `kg_incomplete`, `kg_missing`, `kg_not_ours`, `kg_stale` |
| `E2010` | `mcp_io_failed`, `mcp_root_unusable`, `mcp_worker_failed` |
| `W1001` | `unused_local` |
| `W1002` | `unused_private_function`, `unused_private_type` |
| `W1003` | `unreachable_match_arm` |
| `W1004` | `shadowed_local` |
| `W1005` | `deprecated_use` |
| `W1006` | `implicit_copy_of_large_value` |
| `W1007` | `bare_range_in_for`, `legacy_lambda`, `short_literal_suffix`, `symbolic_borrow`, `symbolic_deref`, `symbolic_reference_type` |
| `W1008` | `effect_attribute_on_const_def`, `effect_attribute_unchecked_generic` |
| `W1009` | `contract_always_true`, `contract_result_unused`, `contract_test_skipped` |
| `W1010` | `untrusted_extern` |
| `W2001` | `integer_overflow_wrapped`, `tool_notice` |
| `W2002` | `bindgen_skipped_declaration` |
<!-- /generated:diagnostic-kinds -->

### 確保数の診断

[Alloc](../built-in-types-and-modules/alloc.md) と [bench --allocs](../built-in-types-and-modules/bench.md#確保数を測る) は既存のコードを使います。

| コード | 条件・修正 |
| --- | --- |
| `E1018` | テスト以外のコード生成が `Alloc` に到達し、counting heap がない場合。`build`・`run` では `--allocator counting` を付けます。bench の時間用実行器では `tsuzuri bench --allocs` が助言です |
| `E1022` | std 以外から `Alloc.__counter` を呼んだ場合。`Alloc.snapshot`・`Alloc.since` を使います |
| `E2000` | `Alloc` に届く出力と `--allocator host`・`small`、counting とカーネル出力、`bench` 以外の `--allocs`。計数用の設定を選び直します |
| `E2005` | bench の計数中のトラップ・時間切れ・負の時間・負の確保数の差分、または不正な計数出力。`benchmark 'Main.name' allocation run: ...` と宣言の位置を報告します |

## 警告

| コード | いつ出るか | 消し方 |
| --- | --- | --- |
| <!-- rule: DIA-004 -->`W1001` | 未使用のローカル、引数、パターン束縛、自分の本体の中でしか使わない `let rec` の関数 | 名前を `_` か `_unused` にする。ムーブ、借用、ガード、クロージャの捕捉は使用です。`_` を付けた名前が同じ関数のローカルにもプログラムの関数にもないときは、その `certain` の修正案を持ち、`tsuzuri fix` が直します |
| `W1002` | 公開 API から到達しない private な関数、レコード、共用体、型エイリアス | 削除するか、公開する |
| `W1003` | 前のガード無しの節が覆っている、到達しない `match` の節 | 節を削除するか、前のパターンを狭くする |
| `W1004` | 同じ字句スコープでのシャドーイング | コマンドラインからは出せません。内部検査のときだけです |
| `W1005` | 別のパッケージや標準ライブラリが `@deprecated` にした `def`・`record`・`union` の使用 | 案内に従って別の宣言へ移ります。同じパッケージの中の使用には出ません（[ドキュメント コメント](../organizing-tsuzuri/documentation-comment.md#警告-w1005)）。関数の案内がちょうど `use Module.name instead` で、`name` が同じモジュールの公開関数で、宣言のシグネチャと `pure`・`const` が同じで、どちらも型引数と名前付きの region を持たないなら（型引数の推論した制約と region の契約はシグネチャに表れないため）、その名前に置き換える `likely` の修正案を持ちます（`tsuzuri fix --applicability likely`） |
| `W1006` | 配列やリストの暗黙のディープコピー | `--warn implicit-copy` のときだけ。`ref` で借りるか、`Array.copy` / `List.copy` で明示する |
| `W1009` | 契約の行が何も確かめない。常に真の条件（`contract_always_true`）、使わない結果の名前（`contract_result_unused`）、`--contracts test` で性質テストを作れない関数（`contract_test_skipped`） | 条件を書き直すか行を外す。結果を使わない行は `@ensures \_ -> ...` と書く |
| `W1007` | edition 2027 が拒否する表記（`E0004` と同じ 6 種）を edition 2026 のソースが使っている | `--warn canonical` のときだけ（ルートのパッケージのソース）。正準形への書き換えを修正案（`certain`、`&` の一時値と引数 `_` の旧ラムダは `likely`、参照の借用 `&r` はなし）として持ち、`tsuzuri fix --edition 2027` が当てる |
| `W1010` | `@trusted` のない `extern` の関数を使っている（`untrusted_extern`） | `--warn untrusted-extern` のとき（ルートのパッケージの宣言だけ）と `tsuzuri audit`（全パッケージ）。関数の契約を確かめて、宣言の前に `@trusted` を書く |
| `W1008` | 効果の属性が検査されない。インスタンスが 1 つもないジェネリック関数の属性（`effect_attribute_unchecked_generic`）と、`const def` の属性（実行時の本体だけを検査する。`effect_attribute_on_const_def`） | ジェネリック関数を使う型で呼ぶ。`const def` の属性は外すか、そのままにする（警告だけで、終了コードは 0） |
| `W2001` | キャッシュを使えなかった、プロファイルが合わなかった、`run --profile-generate` がプロファイルを書かなかった、などのツール警告。`run --overflow-report --json` の整数の折り返しの報告（`integer_overflow_wrapped`）もこのコード | ソースの警告ではありません。`--json` のときこのコードになります。プロファイルが合わないときは、`profile data does not match N function(s) in this build; they are optimized without profile data. Rebuild with --profile-generate and rerun the training workload` の 1 行です（[プロファイル誘導最適化](option.md#プロファイル誘導最適化)） |
| `W2002` | `tsuzuri bindgen` が C ヘッダーの宣言や整数のマクロを変換せずに省いた。理由はメッセージと、出力の `// skipped` 行にあります | 手で宣言するか、対応する型だけの C 関数で包みます。ポインターと長さの組は `--buffer` で変換できることがあります。位置はヘッダーの宣言やマクロの名前です |

`W1006` は、同じ位置がジェネリックの複数の具体化でコピーされても 1 件です。生成コードは、フラグの有無で変わりません。標準ライブラリの内部名と、コンパイラが作った束縛には `W1001` を出しません。

`W1005` は既定で有効で、フラグは要りません。メッセージは `'LegacyLib::Old.old' is deprecated: use LegacyLib::Old.next instead` の形で、案内が空なら `is deprecated; see its documentation for a replacement` です。`--deny-warnings` を付けると終了コードが 1 になるので、依存パッケージが宣言を非推奨にすると、その依存を使う側のビルドが失敗することがあります。警告の有無は互換性の保証の対象外です（[安定性と互換性の方針](../../../docs/stability.md#安定として保たないもの)）。

次の再使用は、`--warn implicit-copy` を付けた `check` で `W1006` になりました。フラグが無いと同じソースは警告ゼロで、終了コードはどちらも 0 です。

```text
copywarn/Main.tz:2:9: warning[W1006]: implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit
  2 | let b = a
    |         ^
```

`--deny-warnings` は、これらの警告が 1 つでもあれば終了コード 1 にします。重要度の文字列は `warning` のままです。警告コードを 1 つだけ消す指令は無いので、個別に直すか、未使用なら `_` を付けます。

`W2002` は `tsuzuri bindgen` だけが出す警告です。生成は続き、終了コードは 0 で、省いた宣言は出力にも `// skipped` 行として残ります。`bindgen` は `--deny-warnings` を受け付けません。

## LSP での表示

`tsuzuri lsp` は、同じ診断を `textDocument/publishDiagnostics` でエディターへ渡します。CLI のテキスト形式ではありません。

各要素は `range`、`severity`、`code`、`source`、`message` です。`source` は `tsuzuri` です。`severity` はエラーが 1、警告が 2 で、LSP の数値です。`range` は UTF-16 の行と桁で、0 始まりです。CLI の `--json` が 1 始まりの Unicode スカラー値なのと、ここが違います。

関連する位置のうちプロジェクトのファイルにあるものは `relatedInformation`（`location` と `message`）になります。std の中の位置は、エディターが開けないので入れません。`data.kind` は診断の kind、修正案を持つ診断は `data.fixes`（`title` と `applicability`）も持ちます。値のないキーは付きません。

診断の範囲にカーソルを置いた hover は、その診断（3 件まで）の重要度・コード・kind・メッセージと、`tsuzuri explain <kind>` の案内を出します。型や doc コメントの hover があるときは、その前に置き、`---` で区切ります。エラーのあるプログラムには型の hover がないので、診断の範囲の外では従来どおり何も出しません。

```text
**error[E1012]** `use_after_move`
use of moved or partially moved value 'text'
Run `tsuzuri explain use_after_move` for an explanation.
```

未保存のバッファも、開いているテキストで検査します。入力を読めないときも、同じオブジェクトを 1 件出します。閉じた文書に新しい診断を送り続けることはしません。

VS Code 拡張は、これを波線と問題パネルに出します。`textDocument/codeAction` のクイックフィックスは、診断の修正案です。修正案は、適用したテキストをもう一度検査し、その診断のコードの件数が減り、どのコードのエラーも増えないときだけ出します。ほかのエラーは残っていてもよく、適用後に検査が通ることまでは保証しません（警告の修正案なら警告も数えます。表示の上限の 50 件を超えた診断も数え、収集の上限の 1000 件に達したときは出しません）。現在、修正案を持つのは、未使用ローカル（`W1001`。名前に `_` を付ける `certain`）、`@deprecated` の案内が同じシグネチャの関数を名指しする使用（`W1005`。`likely`）、`E1021` の足りない腕（`maybe`）、[ヒント](#ヒント) の 7 件（`likely` か `maybe`）です。`certain` の修正案だけを優先のクイックフィックス（`isPreferred`）にします。`context.only` が `source`・`source.fixAll`・`source.fixAll.tsuzuri` を含むか、`only` がないときは、その文書の `certain` の修正案を `tsuzuri fix` と同じラウンドと検証でまとめて適用する `source.fixAll.tsuzuri`（題 `Fix all auto-fixable problems`）も返します（適用するものがなければ返しません）。CLI からは `tsuzuri fix` で適用できます（[コンパイラの使い方](usage.md#fix)）。

## まとめ

<!-- rule: DIA-001 -->
- テキストは `パス:行:列: error[E1003]: ...`、JSON は 1 行に `severity`、`code`、`message`、`path`、`span`、`kind` です。関連する位置、`help`、修正案は、値があるときだけ続きます。
- kind は原因の安定した識別子で、`tsuzuri explain` で説明を読めます。よくある誤りの形には、ヒントの `help` が付きます。
- 50 件を超えると件数だけを足し、1000 件で収集を止めます。重複は最初から 1 件です。
- 字句、構文、型の順に段階を分け、壊れた木から二次エラーを量産しないようにしています。
- `E0` は構文、`E1` は意味、`E2` はツールと実行、`W` は警告です。
- LSP は同じコードを、UTF-16 で 0 始まりの range として出します。関連する位置は `relatedInformation`、検証を通った修正案はクイックフィックスになります。

## 関連項目

- [コンパイラの使い方](usage.md)
- [コンパイラ オプション](option.md)
- [コンパイラ ディレクティブ](directives.md)
- [対応プラットフォームとクロスコンパイル](platforms.md)
- [所有権とムーブ](../ownership-and-memory/ownership.md)
- [言語仕様の診断](../../../docs/language.md#診断)
- [言語リファレンスの目次](../index.md)
