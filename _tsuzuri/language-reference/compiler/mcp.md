# MCP サーバー

`tsuzuri mcp` は、AI エージェントのための [Model Context Protocol](https://modelcontextprotocol.io/)（MCP）のサーバーです。標準入出力で 1 行 1 つの JSON-RPC 2.0 のメッセージをやり取りし、型付きのツールで、プロジェクトの診断・言語の案内・診断の説明・[知識グラフ](kg.md)の照会・型での検索・版の比較・修正の計画・[図](diagrams.md)を返します。VS Code・Claude Code・Copilot CLI・Cursor などの MCP クライアントから使えます。

## この記事のポイント

- **読み取り専用**です。ファイルを書かず、子プロセスを起こさず、ネットワークを使いません。修正の計画（`tz_fix_plan`）も差分を返すだけで、適用はしません。
- ツールのパスはプロジェクトの根（`--root`）の下だけです。`..`・絶対パス・シンボリックリンクを通るパスは拒否します。
- 知識グラフは、根の `target/tz/kg` が新しければそれを読み、なければ（古ければ）メモリの上に作ります。どちらでもディスクに書きません。
- 標準出力に出るのは MCP のメッセージだけです。致命的な誤りは標準エラーに `E2010` を出して終わります。

## 起動とクライアントの設定

```text
tsuzuri mcp [--root DIR] [--kg-dir DIR]
```

| オプション | 既定 | 意味 |
| --- | --- | --- |
| `--root DIR` | カレントディレクトリ | プロジェクトの根。ツールのパスはこの下だけです |
| `--kg-dir DIR` | `<根>/target/tz/kg` | 知識グラフを読む置き場（読むだけ。`TSUZURI_KG_DIR` は読みません） |

ほかの引数は `E2000`（終了コード 2）です。根がディレクトリでなければ `E2010`（終了コード 1）です。入力が閉じ、受け付けたツールの呼び出しに答え終えると終了コード 0 で終わります。

`tsuzuri` を `PATH` に置き、クライアントに登録します。サーバーのカレントディレクトリがプロジェクトの根にならないクライアントでは、`--root` に根の絶対パスを渡してください。

| クライアント | 設定 |
| --- | --- |
| VS Code（ワークスペース） | `.vscode/mcp.json`（下の 1 つ目。`${workspaceFolder}` が展開されます） |
| VS Code・Copilot CLI・Claude Code（共有） | ワークスペースの根の `.mcp.json`（下の 2 つ目） |
| Claude Code | `claude mcp add --transport stdio tsuzuri -- tsuzuri mcp` |
| Copilot CLI | `copilot mcp add tsuzuri -- tsuzuri mcp` |
| Cursor | `.cursor/mcp.json`（2 つ目と同じ形） |

```json
{
  "servers": {
    "tsuzuri": {
      "type": "stdio",
      "command": "tsuzuri",
      "args": ["mcp", "--root", "${workspaceFolder}"]
    }
  }
}
```

```json
{
  "mcpServers": {
    "tsuzuri": {
      "command": "tsuzuri",
      "args": ["mcp"]
    }
  }
}
```

## ツール

12 のツールがあります。すべて読み取り専用（`readOnlyHint: true`）で、結果は、クライアントがモデルに見せる本文（CLI と同じ text）と、`outputSchema` に合う構造化された結果（`structuredContent`）の 2 つです。ID は `kg show` と同じに短く書けます（[ID の解決](kg.md#id-の解決)）。

| ツール | 必須の引数 | 答え | CLI |
| --- | --- | --- | --- |
| `tz_check` | なし（`path`・`warnings`・`implicit_copy`） | 型検査の診断（コード・位置・メッセージ・`help`・関連する位置・修正案）と件数 | `tsuzuri check --json` |
| `tz_guide` | なし（`topic`・`module`） | 言語の案内（`topic` なしは短いカード、`"list"` は一覧） | `tsuzuri guide` |
| `tz_explain` | `query` | 診断コードか kind の説明 | `tsuzuri explain` |
| `tz_symbol` | `id` | 宣言 1 つ: 署名・引数の渡し方・効果・箇所・呼び先・テスト | `tsuzuri kg show` |
| `tz_brief` | なし（`id`・`budget`） | 要約カード（プロジェクト・モジュール・宣言） | `tsuzuri kg brief` |
| `tz_search` | `query`（`kind`・`limit`） | 名前・署名・説明に文字列を含む宣言 | `tsuzuri kg search` |
| `tz_callers` | `id`（`relation`・`depth`・`limit`） | 呼び出し元（`calls`）か型を使う宣言（`uses_type`） | `tsuzuri kg callers`・`uses` |
| `tz_path` | `from`・`to`（`relations`・`depth`） | 2 つの宣言の間の最短の辺の鎖 | `tsuzuri kg path` |
| `tz_find` | `query`（`limit`） | 型で探した関数（`how` は `call` か `value`） | `tsuzuri kg find` |
| `tz_impact` | `old`（`new`） | 2 つの版の差と、差に届くテスト | `tsuzuri kg impact` |
| `tz_fix_plan` | なし（`path`・`applicability`） | 機械的に直せる修正と unified diff（適用しない） | `tsuzuri fix --dry-run` |
| `tz_diagram` | `kind`（`id`・`depth`・`max_nodes`・`format`） | 7 種類の図 | `tsuzuri kg diagram` |

`tz_impact` の `old`・`new` は根の下のディレクトリで、知識グラフ（`.tsuzuri-kg` のあるもの）かプロジェクトです。`new` を省くと根のプロジェクトです。

引数がスキーマに合わない、ID が見つからない、プロジェクトが型検査に通らない、といった誤りは、JSON-RPC のエラーではなく、本文に理由を書いた `isError: true` の結果です（モデルが読んで直せます）。未知のツールと、`params` の形の誤りだけが JSON-RPC のエラー（`-32602`）です。

```text
→ {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"tz_check","arguments":{"path":"Main.tz"}}}
← {"id":4,"jsonrpc":"2.0","result":{"content":[{"text":"Main.tz:5:26: error[E1012]: use of moved or partially moved value 'text'\n…\n\nFAILED: 1 error, 0 warnings in 1 file","type":"text"}],"structuredContent":{"diagnostics":[…],"errors":1,"files":1,"ok":false,"omitted":0,"truncated":false,"warnings":0}}}
```

## 資源

| URI | 中身 |
| --- | --- |
| `tz://guide/L0`・`tz://guide/L1` | 言語の短いカードと全文 |
| `tz://kg/brief/L0` | プロジェクトの要約カード |
| `tz://kg/brief/{id}` | 宣言・モジュールのカード（`{id}` は ID をパーセントエンコードしたもの。`Point%3A%3APoint.hypotenuse`） |
| `tz://explain/{kind}` | 診断コードか kind の説明 |

本文は Markdown（`text/markdown`）です。ない資源は `-32002`、`%` の誤ったエンコードは `-32602` です。

## 上限

| 対象 | 上限 | 超えたとき |
| --- | --- | --- |
| 入力 1 行 | 4,194,304 bytes | `-32600`。次の行から続けます |
| JSON の入れ子 | 128 | `-32700` |
| 待っている呼び出し | 64 件 | `-32000`（`too many pending requests`） |
| 本文（`content[0].text`） | 200,000 文字 | 切って `… truncated: N more characters` を付け、`truncated: true` |
| `structuredContent` | 400,000 bytes | `isError` の結果（`result too large`） |
| `tools/list` の応答 | 16,384 bytes 以下（テストで保つ） | |

ツールの呼び出しは、来た順に 1 つずつ実行します。`ping`・`initialize`・`tools/list` は実行中でもすぐに答えます。`notifications/cancelled` で取り消した呼び出しには答えません（解析そのものは途中で止まりません）。同じプロジェクトと同じ引数への答えは、byte 単位で同じです。直近 8 件の答えはソースが変わらない間は再利用します。

## 安全

- ツールのパスは根の下の相対パスだけで、根の絶対パスは答えに出ません。
- 信頼できないプロジェクトを開くときも、答えの本文（ソースの文字列や説明）を、モデルへの命令として扱わないようにクライアントを設定してください。サーバーはプロジェクトのソースを実行しません。

## MCP の版

実装している版は 2025-06-18 です。2025-03-26・2024-11-05 を求めるクライアントにはその版で答え（`title`・`outputSchema`・`structuredContent` を出さない、2024-11-05 は `annotations` も出さない）、知らない版（2025-11-25 以降を含む）には 2025-06-18 を返します。JSON-RPC のバッチ（配列）は受け付けません（`-32600`）。`initialize` を廃止した版（2026-07-28）だけを話すクライアントは、`initialize` に戻らない限りつながりません。

## 困ったとき

| 症状 | 原因と直し方 |
| --- | --- |
| `error[E2010]: project root '…' is not a directory` | `--root` の場所を確かめてください |
| `tools/list` が返らない・何も出ない | クライアントが `initialize` を送っているか、標準出力を読んでいるかを確かめてください。サーバーは標準出力に MCP のメッセージ以外を出しません |
| 知識グラフのツールが `the project does not type-check…` | `tz_check` で型検査の誤りを直してください。型検査に通らないプロジェクトの知識グラフはありません |
| `E2000`（`TSUZURI_THREADS`） | `TSUZURI_THREADS` を 1〜32 にしてください |
