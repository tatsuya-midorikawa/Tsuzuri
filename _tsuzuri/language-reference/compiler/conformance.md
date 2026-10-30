# 仕様の規則と適合性テスト

言語とツールの規則のうち、テストで確かめているものには **規則 ID**（`OWN-012` のような `AREA-NNN`）が付いています。規則は英語の 1 文で `spec/rules/<AREA>.md` の表にあり、その規則を述べる文書の節と、守っているテスト（`tests/conformance/<AREA>/`）を指します。文書の該当する文の前には、表示されないマーカー `<!-- rule: OWN-012 -->` があります。

## この記事のポイント

- 規則 ID は領域（`LEX`・`SYN`・`TYP`・`OWN`・`EVL`・`NUM`・`STR`・`CTL`・`MOD`・`TSK`・`DIA`・`CLI`・`ABI`・`STD`・`EDN`）と、領域ごとの 3 桁の番号です。番号は再利用しません。意味の変わった規則は `retired` にして、新しい番号を付けます。
- 1 つの規則には 1 つ以上の適合性テストがあります。テストは小さなプログラムで、`Main.tz` の最後の `// rule:`・`// expect:` の行が、守る規則と期待（診断のコードと行、標準出力、トラップ、`tsuzuri test` の合格数、コマンドの出力）を書きます。
- `node scripts/conformance.mjs target/release/tsuzuri` が全テストを実行します（`--area`・`--rule`・`--filter` で絞れます）。標準出力とトラップは native の `-O0` と `-O3`、`tests-pass` は native と wasm32 の `-O0` と `-O3` で確かめます。
- `node scripts/check-spec.mjs` は、表・テスト・文書のマーカーの整合を、コンパイラを使わずに検査します。
- 規則は網羅ではありません。文書の文から、実機で確かめられたものを足していきます。

## 規則の表

`spec/rules/<AREA>.md` の表は `| ID | Rule | Doc | Tests | Status |` の 5 列です。

| 列 | 内容 |
| --- | --- |
| ID | `AREA-NNN`。領域の中で昇順 |
| Rule | `MUST`・`MUST NOT`・`MAY` を含む英語の 1 文（240 文字まで）。1 つの edition だけの規則は `In edition 2026, ` で始めます |
| Doc | その規則を述べる文書（`docs/*.md`・言語リファレンス・`README.md`）の節へのリンク 1 つ |
| Tests | その規則を指すテストの名前。`check-spec --write` が書きます |
| Status | `active` か `retired` |

## テストの書き方

1 つのファイルのテストは `tests/conformance/<AREA>/<ID>-<語>.tz`、複数のファイルが要るテストは同じ名前のディレクトリ（`Main.tz` を含む）です。本体の後に空行を置き、ディレクティブを並べます。

```text
let first = "data"
let second = first
first.length

// rule: OWN-001
// expect: check-error E1012 line=3
```

| 期待 | 合格の条件 |
| --- | --- |
| `check-ok` | `check` が誤りなく終わる |
| `check-error E1012 [line=N] [file=NAME] [kind=K]` | `check` が終了コード 1 で、そのコード（と行・ファイル・kind）の誤りを出す |
| `check-warning W1001 …` | 終了コード 0 で、その警告を出す |
| `stdout "…"` | `run` の `-O0` と `-O3` の標準出力がその文字列（JSON の文字列）と等しい |
| `trap kind=K [line=N] [stdout "…"]` | `run` がトラップし、標準エラーに `trap: <説明> at <場所>` の行がある |
| `tests-pass N` | `tsuzuri test --json` が N 件合格し、失敗がない（整数の折り返しの報告は `--allow-overflow` で警告にとどめます） |

`// cli: ARGS` を書くと、コンパイラを `ARGS`（`$DIR` はテストの作業ディレクトリ）で 1 回実行し、`exit N`・`stdout`・`stdout-has`・`stderr-has`・`stderr-lacks`・`stdout-empty`・`stderr-empty`・`stderr-lines N`・`file-has PATH "…"`・`file-lacks`・`deterministic`（同じ入力から同じ成果物）で確かめます。`// edition: 2026` は 1 つのファイルのテストにその edition のマニフェストを足し、`// targets:` は実行する組を絞ります。

## 実行と検査

```sh
node scripts/conformance.mjs target/release/tsuzuri
node scripts/conformance.mjs target/release/tsuzuri --rule OWN-012 --targets native
node scripts/check-spec.mjs --require-markers
node scripts/check-spec.mjs --coverage
```

`conformance.mjs` は、すべて合格なら終了コード 0、不合格があれば 1、ディレクティブの誤り・コンパイラがない・選んだテストがない・wasm32 のツールがない（`--targets native` で native だけにできます）ときは 2 です。出力は 1 テスト 1 行（`ok <名前> (<規則>)` か `FAIL …` と理由の行）で、`--json` で 1 行 1 JSON です。`check-spec.mjs` は、規則の形・順序・重複、文書へのリンク、テストのない規則、規則のないテストやマーカー、`Tests` 列のずれを報告し、誤りがあれば終了コード 1 です。

## カバレッジ

領域ごとの規則とテストの数です（`check-spec --write` が書き、`check-spec` が一致を検査します）。`Tested` はテストのある `active` の規則、`Tests` は最初の `// rule:` がその領域のテスト、`Marked` は文書にマーカーのある規則の数です。

<!-- generated:spec-coverage -->
| Area | Active | Retired | Tested | Tests | Marked |
| --- | ---: | ---: | ---: | ---: | ---: |
| LEX | 10 | 0 | 10 | 10 | 10 |
| SYN | 10 | 0 | 10 | 10 | 10 |
| TYP | 22 | 0 | 22 | 22 | 22 |
| OWN | 30 | 0 | 30 | 30 | 30 |
| EVL | 20 | 0 | 20 | 20 | 20 |
| NUM | 30 | 0 | 30 | 30 | 30 |
| STR | 10 | 0 | 10 | 10 | 10 |
| CTL | 26 | 0 | 26 | 26 | 26 |
| MOD | 10 | 0 | 10 | 10 | 10 |
| TSK | 10 | 0 | 10 | 10 | 10 |
| DIA | 10 | 0 | 10 | 10 | 10 |
| CLI | 10 | 0 | 10 | 10 | 10 |
| ABI | 10 | 0 | 10 | 10 | 10 |
| STD | 10 | 0 | 10 | 10 | 10 |
| EDN | 10 | 0 | 10 | 10 | 10 |
| Total | 228 | 0 | 228 | 228 | 228 |
<!-- /generated:spec-coverage -->

## 限界

- 規則は文書の全文ではありません。確かめた文だけを足すので、規則のない文書の文もあります。
- WebAssembly で確かめるのは `tests-pass` の規則だけです（`run` は native だけなので、wasm32 の標準出力とトラップは照合しません）。
- macOS と Linux で確かめています。Windows では、診断の `path` の区切り（`\`）は `/` にして比べますが、標準出力の改行と終了コードは確かめていません。
- 実行時間の合否の閾値はありません。

## まとめ

- 規則 ID は、規則・文書の節・テストを 1 つの表で結びます。
- テストは `tests/conformance/` の小さなプログラムで、最後の行に規則と期待を書きます。
- `conformance.mjs` が実行し、`check-spec.mjs` が表・テスト・マーカーの整合を検査します。

## 関連項目

- [コンパイラの使い方](usage.md)
- [診断メッセージとエラーコード](diagnostics.md)
- [言語リファレンスの目次](../index.md)
