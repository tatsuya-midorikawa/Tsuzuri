# G29 付録 1/2（MCP サーバー: 仕様・JSON）

[G29-mcp-server.md](G29-mcp-server.md) の付録。付録 A〜D を置く。付録 E〜H は [G29-appendix-impl.md](G29-appendix-impl.md)。JSON は、**公式の JSON Schema（`schema.json`）と公式の TypeScript SDK のクライアントで検証した**（付録 A・F・G）。想定出力は「（想定出力）」と書く。

## 付録 A: MCP 仕様の抜き書きと検証

### A.1 取得

2026-10-18（JST）に取得した。実装する版は **2025-06-18**（`schema.ts` の `LATEST_PROTOCOL_VERSION = "2025-06-18"`）。

| 内容 | URL（2026-10-18 に取得） |
| --- | --- |
| 仕様の入口 | https://modelcontextprotocol.io/specification/2025-06-18 |
| メッセージ（要求・応答・通知・`_meta`） | https://modelcontextprotocol.io/specification/2025-06-18/basic |
| ライフサイクル（初期化・版の交渉・終了） | https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle |
| transport（stdio・Streamable HTTP） | https://modelcontextprotocol.io/specification/2025-06-18/basic/transports |
| ping・取り消し・進捗 | https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/ping、`…/cancellation`、`…/progress` |
| ツール | https://modelcontextprotocol.io/specification/2025-06-18/server/tools |
| 資源 | https://modelcontextprotocol.io/specification/2025-06-18/server/resources |
| ページ分割 | https://modelcontextprotocol.io/specification/2025-06-18/server/utilities/pagination |
| 変更点 | https://modelcontextprotocol.io/specification/2025-06-18/changelog |
| 型の正本 | `https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/main/schema/2025-06-18/schema.ts`（42,430 bytes、SHA-256 `7d7f4cc6f3adb76e3fd5e1eee6bab8b1e183cfaf719da66d2aa5661737ca9539`）と同じ場所の `schema.json`（108,234 bytes、SHA-256 `af845e7e5b9d27107d1690f0936022546177a1403e63ffb11470135b296a2e01`） |

### A.2 本チケットが従う規則

| 規則（2025-06-18。MUST・SHOULD は仕様の語） | 出典 | 本チケットの扱い |
| --- | --- | --- |
| stdio: メッセージは改行で区切り、**メッセージの中に改行を入れない**（MUST NOT） | transports | `serde_json::to_string` の 1 行 + `\n` |
| サーバーは stdout に **MCP のメッセージ以外を書かない**（MUST NOT）。stderr にログを書いてよい（MAY） | transports | stdout は JSON の行だけ（手順 9 の静的な検査）。stderr は `E2010` だけ |
| 終了: クライアントが stdin を閉じ、サーバーの終了を待つ（SHOULD） | lifecycle | EOF → 残りの応答を出して終了 0 |
| 要求の `id` は文字列か整数で、`null` でなく（MUST）、同じセッションで再利用しない（MUST。クライアントの責務） | basic | `null`・小数は `-32600`。実行中・待ちと同じ `id` は `-32600` |
| 応答は要求と同じ `id`。`result` か `error` のどちらか一方（MUST）。エラーコードは整数 | basic | 応答の組み立て |
| 通知は `id` を持たず、受け手は応答しない（MUST NOT） | basic | 通知にはエラーも返さない |
| `initialize` が最初のやり取り（MUST）。要求に `protocolVersion`・`capabilities`・`clientInfo` | lifecycle | `protocolVersion` だけを検査する（D2） |
| 版の交渉: 対応する版なら**同じ版**を返し（MUST）、そうでなければ**対応するほかの版**（SHOULD 最新）を返す | lifecycle | `2025-06-18`・`2025-03-26`・`2024-11-05` は同じ値、ほかは `2025-06-18` |
| クライアントは `initialize` の応答の前に `ping` 以外の要求を送らない（SHOULD NOT） | lifecycle | `Fresh` の状態で `tools/*` は `-32600` |
| `tools` を提供するサーバーは `tools` の capability を宣言する（MUST）。`listChanged` は通知を出すか | tools | `{"tools":{"listChanged":false}}` |
| 交渉した版を守り、交渉した機能だけを使う（MUST） | lifecycle | 版による出し分けの表 |
| `tools/list` は `cursor`・`nextCursor` のページ分割に対応でき、**不正なカーソルは `-32602`**（SHOULD） | tools・pagination | ページ分割せず、`cursor` はすべて `-32602` |
| `Tool`: `name`・`description`・`inputSchema`（JSON Schema・`type: "object"`）。`title`・`outputSchema`・`annotations` は任意。クライアントは `annotations` を信頼しない（MUST） | tools・schema.ts | `tools.json` |
| ツールの結果: `content`（配列）、`isError`、`structuredContent`。**`outputSchema` があれば、サーバーは合う `structuredContent` を返す（MUST）** | tools | テスト `tool_results_conform_to_output_schemas` |
| 構造化した結果は、**同じ JSON を `TextContent` にも入れる（SHOULD）** | tools | **従わない**（D4。`text` は CLI の text 出力） |
| 誤りの 2 種類: プロトコルの誤り（未知のツール・不正な引数・サーバーの誤り）と、`isError: true`（API の失敗・入力データの不正・業務ロジックの誤り） | tools | D5 |
| サーバーは、ツールの入力を検証し、アクセス制御し、呼び出しを制限し、出力を無害にする（MUST） | tools | 検証器・パスの閉じ込め・待ち 64 件・上限 |
| `ping`: 受け手は**すぐに**空の `result` で答える（MUST） | ping | 主スレッドで即答 |
| `notifications/cancelled`: 実行中の要求だけを指す。`initialize` は取り消せない（MUST NOT）。受け手は処理を止め・応答を返さない（SHOULD）。未知・終了済みは無視してよい（MAY） | cancellation | 結果を捨てる。待ちは外す。ほかは無視 |
| 進捗: `_meta.progressToken` があれば通知してよい（MAY） | progress | 通知しない |
| 資源: `resources` を宣言（MUST）。`resources/list`・`read`・`templates/list`。見つからない資源は `-32002` | resources | Phase 3 |
| JSON-RPC の**バッチは削除された**（2025-06-18 の変更点 1。2025-03-26 は受信に対応する MUST） | changelog | `-32600`（D3） |

### A.3 新しい版（2026-10-18 に変更点のページを読んだ）

| 版 | 変更点（抜粋） | 本チケットの扱い |
| --- | --- | --- |
| 2025-11-25 | icons・tasks（実験）・`$schema`・URL 形式の elicitation・サンプリングのツール、ツール名の指針（`A-Za-z0-9_.-`、1〜128 文字）、入力の検証の誤りを `isError` にする、JSON Schema 2020-12 を既定の方言に。`Tool` の差は `icons`・`execution`・`$schema` の追加だけ（`schema.ts` の差分で確認） | 対応しない（D2）。`tz_*` の名前は指針に合い、検証の誤りは `isError` で、既に整合する |
| 2026-07-28 | `initialize` と `notifications/initialized` を廃止（stateless。各要求の `_meta` に版・capabilities）、`server/discover`、`ping` の削除、結果に `resultType`、`tools/list`・`prompts/list`・`resources/list`・`resources/read`・`resources/templates/list` に `ttlMs`・`cacheScope`、資源の「見つからない」を `-32602` に。stdio の後方互換: クライアントは先に `server/discover` を送り、`DiscoverResult` なら modern、**modern のエラーでなければ（`-32601` を含む）legacy と見て `initialize` に戻る** | 対応しない（D2）。`server/discover` は `-32601`（初期化の前でも）。dual-era のクライアントは `initialize` に戻る。modern だけのクライアントは接続できない（相互運用の表で `Modern × Legacy` は失敗） |

### A.4 検証の方法

付録 C の JSON は、`schema.json` の `definitions` に対して、Python の `jsonschema`（4.26.0。Draft 7）で検証した（再現は下のコマンド）。ツールの `inputSchema`・`outputSchema` は Draft 2020-12 のメタスキーマで検証し、`Tool` の定義にも合う。付録 F の SDK のクライアントも同じ JSON を受理した。

```python
import json
from jsonschema import Draft7Validator

official = json.load(open("schema.json"))            # 上の URL
def check(definition, instance):
    schema = {"$ref": f"#/definitions/{definition}", "definitions": official["definitions"]}
    errors = list(Draft7Validator(schema).iter_errors(instance))
    assert not errors, errors[0].message

check("InitializeRequest", {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {...}})
check("InitializeResult", result_of_initialize)       # 応答は result の中身を検証し、
check("JSONRPCResponse", {"jsonrpc": "2.0", "id": 1, "result": result_of_initialize})  # 外側も検証する
check("CallToolResult", tool_call_result)
```

`id: null` の誤りの応答（付録 C の `error.parse`・`error.batch`）は、JSON-RPC 2.0 の規則（読めない要求への応答の `id` は `null`）で、`JSONRPCError` の定義（`id` は必須の文字列か整数）には合わない。HTTP の transport の節も「`id` のない JSON-RPC のエラー」を許す。**この 2 件だけが `schema.json` に合わない（意図した逸脱）**。

## 付録 B: `src/mcp/tools.json`（全文）

そのまま `src/mcp/tools.json` に写す（11 要素。順序も変えない）。検証済み（付録 A.4。各要素が `Tool` の定義に合い、全キーワードが `SUPPORTED_KEYWORDS` の中）。

```json
[
  {
    "name": "tz_guide",
    "title": "Tsuzuri language guide",
    "description": "Return the Tsuzuri language guide that matches this compiler: syntax, ownership rules, numeric rules, common mistakes and the standard library index. Omit topic for the short card; pass topic \"list\" to see every topic.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "topic": {
          "type": "string",
          "maxLength": 64,
          "description": "Guide topic. Omit for the short card; \"list\" lists the topics."
        },
        "module": {
          "type": "string",
          "maxLength": 128,
          "description": "With topic \"std\": the standard library module whose page to return, for example Array."
        }
      },
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "topic": {
          "type": "string"
        },
        "text": {
          "type": "string"
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "topic",
        "text",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Tsuzuri language guide",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_check",
    "title": "Type-check a Tsuzuri project",
    "description": "Type-check a project (or one file) without building it and return every diagnostic with its code, position and message, plus help, related locations and fixes when the compiler has them. Run it after every edit.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "path": {
          "type": "string",
          "maxLength": 4096,
          "description": "File or directory to check, relative to the project root. Default: the project root, which needs Main.tz. For a library without Main.tz pass one of its .tz files."
        },
        "warnings": {
          "type": "boolean",
          "default": true,
          "description": "Include warnings. false returns errors only."
        },
        "implicit_copy": {
          "type": "boolean",
          "default": false,
          "description": "Also report W1006, the implicit copies of arrays and lists."
        }
      },
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "ok": {
          "type": "boolean"
        },
        "files": {
          "type": "integer",
          "minimum": 0
        },
        "errors": {
          "type": "integer",
          "minimum": 0
        },
        "warnings": {
          "type": "integer",
          "minimum": 0
        },
        "omitted": {
          "type": "integer",
          "minimum": 0
        },
        "diagnostics": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "severity": {
                "type": "string",
                "enum": [
                  "error",
                  "warning"
                ]
              },
              "code": {
                "type": "string"
              },
              "message": {
                "type": "string"
              },
              "path": {
                "type": "string"
              },
              "span": {
                "type": "object",
                "properties": {
                  "start": {
                    "type": "integer",
                    "minimum": 0
                  },
                  "end": {
                    "type": "integer",
                    "minimum": 0
                  },
                  "line": {
                    "type": "integer",
                    "minimum": 1
                  },
                  "column": {
                    "type": "integer",
                    "minimum": 1
                  },
                  "end_line": {
                    "type": "integer",
                    "minimum": 1
                  },
                  "end_column": {
                    "type": "integer",
                    "minimum": 1
                  }
                },
                "required": [
                  "start",
                  "end",
                  "line",
                  "column",
                  "end_line",
                  "end_column"
                ]
              }
            },
            "required": [
              "severity",
              "code",
              "message",
              "path",
              "span"
            ]
          }
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "ok",
        "files",
        "errors",
        "warnings",
        "omitted",
        "diagnostics",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Type-check a Tsuzuri project",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_explain",
    "title": "Explain a diagnostic",
    "description": "Explain a compiler diagnostic: why it is reported, a failing example and the fix. query is a code such as E1012 or W1006, or a kind such as use_after_move.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "minLength": 1,
          "maxLength": 64,
          "description": "Diagnostic code (E1012) or kind (use_after_move)."
        }
      },
      "required": [
        "query"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "code": {
          "type": "string"
        },
        "kind": {
          "type": "string"
        },
        "title": {
          "type": "string"
        },
        "since": {
          "type": "string"
        },
        "text": {
          "type": "string"
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "code",
        "title",
        "text",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Explain a diagnostic",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_symbol",
    "title": "Show a declaration",
    "description": "Show one declaration from the knowledge base: signature, parameter conventions (own, ref, ref mut), effects, trap, alloc and copy sites, callees and tests. id is a knowledge-base id such as Main.sum_to or Array.map.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "minLength": 1,
          "maxLength": 512,
          "description": "Knowledge-base id of a module, function, type, class, field or test."
        }
      },
      "required": [
        "id"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "symbol": {
          "type": "object",
          "properties": {
            "id": {
              "type": "string"
            },
            "kind": {
              "type": "string"
            },
            "name": {
              "type": "string"
            }
          },
          "required": [
            "id",
            "kind",
            "name"
          ]
        },
        "facts": {
          "type": [
            "object",
            "null"
          ]
        },
        "callees": {
          "type": "array",
          "items": {
            "type": "string"
          }
        },
        "tests": {
          "type": "array",
          "items": {
            "type": "string"
          }
        },
        "callers": {
          "type": "integer",
          "minimum": 0
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "symbol",
        "facts",
        "callees",
        "tests",
        "callers",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Show a declaration",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_brief",
    "title": "Summary card of a project, module or item",
    "description": "Return a short summary card: the whole project (omit id), one module (id such as Main) or one function or type. Use it to orient yourself before reading code.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "minLength": 1,
          "maxLength": 512,
          "description": "Knowledge-base id of a module, function or type. Omit for the whole project."
        },
        "budget": {
          "type": "integer",
          "minimum": 200,
          "maximum": 200000,
          "default": 6000,
          "description": "Largest answer in characters; lower-priority lines are replaced by an 'N more' line."
        }
      },
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "id": {
          "type": [
            "string",
            "null"
          ]
        },
        "level": {
          "type": "string",
          "enum": [
            "L0",
            "L1",
            "L2"
          ]
        },
        "text": {
          "type": "string"
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "id",
        "level",
        "text",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Summary card of a project, module or item",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_search",
    "title": "Search declarations by text",
    "description": "Find declarations by text in names, signatures and documentation. Returns knowledge-base ids to pass to tz_symbol, tz_brief or tz_callers.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "minLength": 1,
          "maxLength": 256,
          "description": "Text to look for."
        },
        "kind": {
          "type": "string",
          "enum": [
            "module",
            "def",
            "const",
            "record",
            "union",
            "case",
            "field",
            "class",
            "method",
            "instance",
            "alias",
            "test",
            "bench",
            "extern",
            "handle"
          ],
          "description": "Only declarations of this kind."
        },
        "limit": {
          "type": "integer",
          "minimum": 1,
          "maximum": 50,
          "default": 20,
          "description": "Most matches to return."
        }
      },
      "required": [
        "query"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string"
        },
        "matches": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "id": {
                "type": "string"
              },
              "kind": {
                "type": "string"
              },
              "signature": {
                "type": "string"
              }
            },
            "required": [
              "id",
              "kind",
              "signature"
            ]
          }
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "query",
        "matches",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Search declarations by text",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_callers",
    "title": "Callers of a function, users of a type",
    "description": "List the call sites that call a function (relation calls) or the declarations that use a type (relation uses_type), up to depth levels of callers. Each row has the caller id, the position and the argument conventions.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "minLength": 1,
          "maxLength": 512,
          "description": "Knowledge-base id of a function (relation calls) or a type (relation uses_type)."
        },
        "relation": {
          "type": "string",
          "enum": [
            "calls",
            "uses_type"
          ],
          "default": "calls"
        },
        "depth": {
          "type": "integer",
          "minimum": 1,
          "maximum": 8,
          "default": 1,
          "description": "Levels of callers to follow."
        },
        "limit": {
          "type": "integer",
          "minimum": 1,
          "maximum": 200,
          "default": 50,
          "description": "Most rows to return."
        }
      },
      "required": [
        "id"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "id": {
          "type": "string"
        },
        "relation": {
          "type": "string",
          "enum": [
            "calls",
            "uses_type"
          ]
        },
        "depth": {
          "type": "integer",
          "minimum": 1
        },
        "callers": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "rel": {
                "type": "string"
              },
              "from": {
                "type": "string"
              },
              "to": {
                "type": "string"
              },
              "span": {
                "type": "object"
              },
              "depth": {
                "type": "integer",
                "minimum": 1
              }
            },
            "required": [
              "rel",
              "from",
              "to",
              "span",
              "depth"
            ]
          }
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "id",
        "relation",
        "depth",
        "callers",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Callers of a function, users of a type",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_find",
    "title": "Search declarations by type",
    "description": "Search the declarations visible in the project by type, for example 'ref [i64] -> i64'. _ matches any type and 'a is a type variable. Returns the best candidates first.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "minLength": 1,
          "maxLength": 256,
          "description": "A Tsuzuri type, for example \"ref [i64] -> i64\"."
        },
        "limit": {
          "type": "integer",
          "minimum": 1,
          "maximum": 10,
          "default": 10,
          "description": "Most candidates to return (the search never returns more than 10)."
        }
      },
      "required": [
        "query"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string"
        },
        "candidates": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "name": {
                "type": "string"
              },
              "signature": {
                "type": "string"
              },
              "how": {
                "type": "string",
                "enum": [
                  "call",
                  "value",
                  "type",
                  "module"
                ]
              }
            },
            "required": [
              "name",
              "signature",
              "how"
            ]
          }
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "query",
        "candidates",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Search declarations by type",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_impact",
    "title": "Impact of a change",
    "description": "Compare two versions of a project and report what the change touches: signature and parameter-convention changes, effect changes, new warnings and the tests that reach the changed code. old and new are directories inside the project root: a project or a knowledge base.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "old": {
          "type": "string",
          "minLength": 1,
          "maxLength": 4096,
          "description": "Directory of the earlier version, relative to the project root."
        },
        "new": {
          "type": "string",
          "maxLength": 4096,
          "description": "Directory of the later version, relative to the project root. Default: the project root."
        }
      },
      "required": [
        "old"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "old": {
          "type": "string"
        },
        "new": {
          "type": "string"
        },
        "records": {
          "type": "array",
          "items": {
            "type": "object"
          }
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "old",
        "new",
        "records",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Impact of a change",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_fix_plan",
    "title": "Plan machine-applicable fixes",
    "description": "Plan the fixes that the compiler can apply mechanically for the diagnostics of a project and return the edits and a unified diff. It never changes a file: apply the diff or the edits yourself.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "path": {
          "type": "string",
          "maxLength": 4096,
          "description": "File or directory, relative to the project root. Default: the project root."
        },
        "applicability": {
          "type": "string",
          "enum": [
            "certain",
            "likely",
            "maybe"
          ],
          "default": "certain",
          "description": "certain: meaning-preserving rewrites only. likely: also rewrites that a person should review. maybe: also suggestions that need a decision."
        }
      },
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "applicability": {
          "type": "string",
          "enum": [
            "certain",
            "likely",
            "maybe"
          ]
        },
        "rounds": {
          "type": "integer",
          "minimum": 0
        },
        "stop": {
          "type": "string",
          "enum": [
            "fixed_point",
            "round_limit"
          ]
        },
        "fixes": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "code": {
                "type": "string"
              },
              "kind": {
                "type": "string"
              },
              "applicability": {
                "type": "string",
                "enum": [
                  "certain",
                  "likely",
                  "maybe"
                ]
              },
              "title": {
                "type": "string"
              },
              "round": {
                "type": "integer",
                "minimum": 1
              },
              "edits": {
                "type": "array",
                "items": {
                  "type": "object",
                  "properties": {
                    "path": {
                      "type": "string"
                    },
                    "span": {
                      "type": "object"
                    },
                    "text": {
                      "type": "string"
                    }
                  },
                  "required": [
                    "path",
                    "span",
                    "text"
                  ]
                }
              }
            },
            "required": [
              "code",
              "applicability",
              "title",
              "round",
              "edits"
            ]
          }
        },
        "dropped": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "code": {
                "type": "string"
              },
              "applicability": {
                "type": "string",
                "enum": [
                  "certain",
                  "likely",
                  "maybe"
                ]
              },
              "title": {
                "type": "string"
              },
              "reason": {
                "type": "string"
              }
            },
            "required": [
              "code",
              "applicability",
              "title",
              "reason"
            ]
          }
        },
        "diff": {
          "type": "string"
        },
        "remaining_errors": {
          "type": "integer",
          "minimum": 0
        },
        "remaining_warnings": {
          "type": "integer",
          "minimum": 0
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "applicability",
        "rounds",
        "stop",
        "fixes",
        "dropped",
        "diff",
        "remaining_errors",
        "remaining_warnings",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Plan machine-applicable fixes",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  },
  {
    "name": "tz_diagram",
    "title": "Generate a diagram",
    "description": "Generate a Mermaid (or DOT) diagram from the knowledge base: module dependencies, types, the call graph of a function, the ownership timeline of a function, the concurrency topology, the memory layout of a type, or the state transitions of a union.",
    "inputSchema": {
      "type": "object",
      "properties": {
        "kind": {
          "type": "string",
          "enum": [
            "module",
            "types",
            "calls",
            "ownership",
            "concurrency",
            "layout",
            "transitions"
          ],
          "description": "Which diagram."
        },
        "id": {
          "type": "string",
          "minLength": 1,
          "maxLength": 512,
          "description": "calls and ownership: a function id. layout and transitions: a type id. module, types and concurrency: an optional module id to focus on."
        },
        "depth": {
          "type": "integer",
          "minimum": 1,
          "maximum": 6,
          "default": 2,
          "description": "calls: levels of callees to draw."
        },
        "max_nodes": {
          "type": "integer",
          "minimum": 5,
          "maximum": 200,
          "default": 60,
          "description": "Most nodes before the diagram folds modules into single nodes."
        },
        "format": {
          "type": "string",
          "enum": [
            "mermaid",
            "dot"
          ],
          "default": "mermaid"
        }
      },
      "required": [
        "kind"
      ],
      "additionalProperties": false
    },
    "outputSchema": {
      "type": "object",
      "properties": {
        "kind": {
          "type": "string"
        },
        "format": {
          "type": "string",
          "enum": [
            "mermaid",
            "dot",
            "markdown"
          ]
        },
        "text": {
          "type": "string"
        },
        "nodes": {
          "type": "integer",
          "minimum": 0
        },
        "truncated": {
          "type": "boolean"
        }
      },
      "required": [
        "kind",
        "format",
        "text",
        "nodes",
        "truncated"
      ]
    },
    "annotations": {
      "title": "Generate a diagram",
      "readOnlyHint": true,
      "destructiveHint": false,
      "idempotentHint": true,
      "openWorldHint": false
    }
  }
]
```

## 付録 C: 往復の例

クライアントの行は、クライアントが書く自然な順序のまま（キーの順序に意味はない）。**サーバーの行は、キーが辞書順の 1 行**で、テストの期待値は `Value` 同士で比べる。各例は付録 A.4 の方法で検証済み。

### C.1 初期化から `tools/list` まで

**initialize の要求（クライアント → サーバー）**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"roots":{"listChanged":true},"sampling":{},"elicitation":{}},"clientInfo":{"name":"ExampleClient","title":"Example Client Display Name","version":"1.0.0"}}}
```

**initialize の応答（Phase 1・2 の時点。resources は Phase 3 で足す）**（サーバー → クライアント）

```json
{"id":1,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"instructions":"Read-only Tsuzuri compiler tools. After you edit a .tz, .tt or .tc file, call tz_check and fix every error before anything else; each diagnostic has its code, position and, when the compiler knows, a help line. The other tools (see tools/list) answer questions about the language and the project without reading files. No tool changes the project.","protocolVersion":"2025-06-18","serverInfo":{"name":"tsuzuri","title":"Tsuzuri compiler","version":"0.1.0"}}}
```

**initialized の通知（応答なし）**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

**ping の要求**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":2,"method":"ping"}
```

**ping の応答**（サーバー → クライアント）

```json
{"id":2,"jsonrpc":"2.0","result":{}}
```

**tools/list の要求**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":3,"method":"tools/list"}
```


`tools/list` の応答は、`{"id":3,"jsonrpc":"2.0","result":{"tools":[<付録 B の 11 要素を順に>]}}` の 1 行（`2025-06-18` のとき）。**1 行が 14,411 bytes（改行を含む）で、その SHA-256 は `3a71b0f8176da5d463b0ef3d026c89999f4d100101bf0c714d53ecc2d9a55ef9`**（`tools.json` を変えたら、この値と上限の数字を更新する）。2025-03-26 では各要素から `title` と `outputSchema` を除き、2024-11-05 ではさらに `annotations` を除く。

**initialize の応答（Phase 3 以降。`resources` の capability が付く）**（サーバー → クライアント）

```json
{"id":1,"jsonrpc":"2.0","result":{"capabilities":{"resources":{},"tools":{"listChanged":false}},"instructions":"Read-only Tsuzuri compiler tools. After you edit a .tz, .tt or .tc file, call tz_check and fix every error before anything else; each diagnostic has its code, position and, when the compiler knows, a help line. The other tools (see tools/list) answer questions about the language and the project without reading files. No tool changes the project.","protocolVersion":"2025-06-18","serverInfo":{"name":"tsuzuri","title":"Tsuzuri compiler","version":"0.1.0"}}}
```


### C.2 `tools/call`

**tools/call tz_check の要求**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"tz_check","arguments":{"path":"Main.tz"}}}
```

**tools/call tz_check の応答（型検査に失敗。HEAD の `check --json` と同じ 5 キーの診断）**（サーバー → クライアント）

```json
{"id":4,"jsonrpc":"2.0","result":{"content":[{"text":"Main.tz:5:26: error[E1012]: use of moved or partially moved value 'text'\n  5 |     let second = consume text\n    |                          ^\n\nFAILED: 1 error, 0 warnings in 1 file","type":"text"}],"structuredContent":{"diagnostics":[{"code":"E1012","message":"use of moved or partially moved value 'text'","path":"Main.tz","severity":"error","span":{"column":26,"end":147,"end_column":30,"end_line":5,"line":5,"start":143}}],"errors":1,"files":1,"ok":false,"omitted":0,"truncated":false,"warnings":0}}}
```

**tools/call tz_check の応答（成功。警告が 1 件）**（サーバー → クライアント）

```json
{"id":5,"jsonrpc":"2.0","result":{"content":[{"text":"Main.tz:2:9: warning[W1001]: unused local 'spare'; prefix it with '_' to silence this warning\n  2 |     let spare = 1\n    |         ^\n\nOK: 0 errors, 1 warning in 1 file","type":"text"}],"structuredContent":{"diagnostics":[{"code":"W1001","message":"unused local 'spare'; prefix it with '_' to silence this warning","path":"Main.tz","severity":"warning","span":{"column":9,"end":45,"end_column":14,"end_line":2,"line":2,"start":40}}],"errors":0,"files":1,"ok":true,"omitted":0,"truncated":false,"warnings":1}}}
```

**tools/call tz_check の要求（引数の型が違う）**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"tz_check","arguments":{"path":5}}}
```

**tools/call の応答（引数の検証の失敗。ツールの実行の誤りなので `isError: true`）**（サーバー → クライアント）

```json
{"id":6,"jsonrpc":"2.0","result":{"content":[{"text":"invalid arguments: /path: must be a string","type":"text"}],"isError":true}}
```

**tools/call の応答（未知のツール。プロトコルの誤り）**（サーバー → クライアント）

```json
{"error":{"code":-32602,"message":"unknown tool: tz_nothing"},"id":7,"jsonrpc":"2.0"}
```


ほかの 10 ツールの呼び出しと結果は付録 D。

### C.3 版の交渉

**initialize の要求（未対応の新しい版 2026-07-28）**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"ExampleClient","version":"1.0.0"}}}
```

**initialize の応答（要求が未対応なら `2025-06-18` を返す）**（サーバー → クライアント）

```json
{"id":1,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"instructions":"Read-only Tsuzuri compiler tools. After you edit a .tz, .tt or .tc file, call tz_check and fix every error before anything else; each diagnostic has its code, position and, when the compiler knows, a help line. The other tools (see tools/list) answer questions about the language and the project without reading files. No tool changes the project.","protocolVersion":"2025-06-18","serverInfo":{"name":"tsuzuri","title":"Tsuzuri compiler","version":"0.1.0"}}}
```

**initialize の応答（要求が `2025-03-26`。`serverInfo.title` は付けない）**（サーバー → クライアント）

```json
{"id":1,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"instructions":"Read-only Tsuzuri compiler tools. After you edit a .tz, .tt or .tc file, call tz_check and fix every error before anything else; each diagnostic has its code, position and, when the compiler knows, a help line. The other tools (see tools/list) answer questions about the language and the project without reading files. No tool changes the project.","protocolVersion":"2025-03-26","serverInfo":{"name":"tsuzuri","version":"0.1.0"}}}
```


### C.4 誤りと通知

**JSON として読めない行（`id` は null。JSON-RPC 2.0 の規則）**（サーバー → クライアント）

```json
{"error":{"code":-32700,"message":"parse error: expected value at line 1 column 1"},"id":null,"jsonrpc":"2.0"}
```

**JSON の配列（バッチ）**（サーバー → クライアント）

```json
{"error":{"code":-32600,"message":"invalid request: batch requests are not supported"},"id":null,"jsonrpc":"2.0"}
```

**initialize の前の tools/list**（サーバー → クライアント）

```json
{"error":{"code":-32600,"message":"server not initialized: send initialize first"},"id":3,"jsonrpc":"2.0"}
```

**未知のメソッド（2026-07-28 のクライアントが送る `server/discover` もこれ）**（サーバー → クライアント）

```json
{"error":{"code":-32601,"message":"method not found: server/discover"},"id":1,"jsonrpc":"2.0"}
```

**notifications/cancelled（応答なし）**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":4,"reason":"User requested cancellation"}}
```


`error.parse` の `message` は `parse error: ` の後に `serde_json` のメッセージ（`}` だけの行なら `expected value at line 1 column 1`）が続く。文言は版で変わりうるので、テストは接頭辞だけを見る。

### C.5 資源（Phase 3）

**resources/list の応答（Phase 3。ガイドと KB が有効なとき）**（サーバー → クライアント）

```json
{"id":10,"jsonrpc":"2.0","result":{"resources":[{"description":"Short guide to the language that matches this compiler.","mimeType":"text/markdown","name":"guide-l0","title":"Tsuzuri language card","uri":"tz://guide/L0"},{"description":"Full guide to the language that matches this compiler.","mimeType":"text/markdown","name":"guide-l1","title":"Tsuzuri language guide","uri":"tz://guide/L1"},{"description":"Summary card of the whole project.","mimeType":"text/markdown","name":"brief-l0","title":"Project brief","uri":"tz://kb/brief/L0"}]}}
```

**resources/templates/list の応答**（サーバー → クライアント）

```json
{"id":11,"jsonrpc":"2.0","result":{"resourceTemplates":[{"description":"{id} is a knowledge-base id, percent-encoded.","mimeType":"text/markdown","name":"brief","title":"Brief card of a module, function or type","uriTemplate":"tz://kb/brief/{id}"},{"description":"{kind} is a diagnostic code such as E1012 or a kind such as use_after_move.","mimeType":"text/markdown","name":"explain","title":"Explanation of a diagnostic","uriTemplate":"tz://explain/{kind}"}]}}
```

**resources/read の要求**（クライアント → サーバー）

```json
{"jsonrpc":"2.0","id":12,"method":"resources/read","params":{"uri":"tz://kb/brief/Point.hypotenuse"}}
```

**resources/read の応答（本文は例のために短くした）**（サーバー → クライアント）

```json
{"id":12,"jsonrpc":"2.0","result":{"contents":[{"mimeType":"text/markdown","text":"# Point.hypotenuse\nexport def hypotenuse :: f64 -> f64 -> f64\n","uri":"tz://kb/brief/Point.hypotenuse"}]}}
```

**resources/read の応答（存在しない資源。2025-06-18 の規則で `-32002`）**（サーバー → クライアント）

```json
{"error":{"code":-32002,"data":{"uri":"tz://explain/E9999"},"message":"resource not found: tz://explain/E9999"},"id":13,"jsonrpc":"2.0"}
```


## 付録 D: ツールごとの例（想定出力）

`tz_check` は付録 C.2（HEAD の実出力）。次の 10 ツールの値は**想定出力**で、`examples/point` の KB から**手で作った**。依存チケットの実装後に、実際の出力で置き換える。このファイルが `tests/fixtures/mcp/tool-examples.json` になり、`FakeBackend` が返し、`tool_results_conform_to_output_schemas` が `outputSchema` で検証する（各例は付録 A.4 の方法で検証済み）。`tz_check` の例は `tests/mcp.rs` に直接書く（付録 C.2）。

```json
[
 {
  "tool": "tz_guide",
  "title": "tz_guide（引数なし。本文は説明のために短くした）",
  "arguments": {},
  "text": "# Tsuzuri card (L0)\nBorrow with 'ref x' (shared) or 'ref mut x' (exclusive). A value moved into a call is gone: using it again is E1012.\n",
  "structured": {
   "topic": "card",
   "text": "# Tsuzuri card (L0)\nBorrow with 'ref x' (shared) or 'ref mut x' (exclusive). A value moved into a call is gone: using it again is E1012.\n",
   "truncated": false
  }
 },
 {
  "tool": "tz_explain",
  "title": "tz_explain（G23 の説明ページ。Bad の例は実際に E1012 を出し、Good の例は通ることを HEAD で確かめた）",
  "arguments": {
   "query": "E1012"
  },
  "text": "code: E1012\ntitle: use of a moved value\nsince: 0.1.0\n\n## Why\nA value that was moved into a call, a 'let' or a return is no longer owned by its old name. Using the name again is an error.\n\n## Bad\n```tsuzuri\ndef consume :: string -> i64 = \\text -> text.length\n\ndef run :: string -> i64 = \\text ->\n    let first = consume text\n    let second = consume text\n    first + second\n```\n\n## Good\n```tsuzuri\ndef consume :: ref string -> i64 = \\text -> text.length\n\ndef run :: string -> i64 = \\text ->\n    let first = consume (ref text)\n    let second = consume (ref text)\n    first + second\n```\n\n## See also\nE1013, E1014\n",
  "structured": {
   "code": "E1012",
   "kind": "use_after_move",
   "title": "use of a moved value",
   "since": "0.1.0",
   "text": "code: E1012\ntitle: use of a moved value\nsince: 0.1.0\n\n## Why\nA value that was moved into a call, a 'let' or a return is no longer owned by its old name. Using the name again is an error.\n\n## Bad\n```tsuzuri\ndef consume :: string -> i64 = \\text -> text.length\n\ndef run :: string -> i64 = \\text ->\n    let first = consume text\n    let second = consume text\n    first + second\n```\n\n## Good\n```tsuzuri\ndef consume :: ref string -> i64 = \\text -> text.length\n\ndef run :: string -> i64 = \\text ->\n    let first = consume (ref text)\n    let second = consume (ref text)\n    first + second\n```\n\n## See also\nE1013, E1014\n",
   "truncated": false
  }
 },
 {
  "tool": "tz_symbol",
  "title": "tz_symbol（想定出力。`examples/point` の KB。G26・G28 の実装後に実際の出力で置き換える）",
  "arguments": {
   "id": "Point.hypotenuse"
  },
  "text": "Point.hypotenuse  def  export\nexport def hypotenuse :: f64 -> f64 -> f64\nparams: x: f64 (copy), y: f64 (copy)\ncallees: Point.distance\ntests: none\ncallers: 0",
  "structured": {
   "symbol": {
    "id": "Point.hypotenuse",
    "kind": "def",
    "module": "Point",
    "name": "hypotenuse",
    "signature": "export def hypotenuse :: f64 -> f64 -> f64",
    "params": [
     {
      "name": "x",
      "type": "f64",
      "conv": "copy"
     },
     {
      "name": "y",
      "type": "f64",
      "conv": "copy"
     }
    ],
    "result": "f64",
    "public": true,
    "export": true,
    "pure": false,
    "doc": "",
    "deprecated": null,
    "origin": "project",
    "span": {
     "path": "Point.tz",
     "start": 135,
     "end": 145,
     "line": 6,
     "column": 12
    }
   },
   "facts": null,
   "callees": [
    "Point.distance"
   ],
   "tests": [],
   "callers": 0,
   "truncated": false
  }
 },
 {
  "tool": "tz_brief",
  "title": "tz_brief（想定出力。関数のカード L2）",
  "arguments": {
   "id": "Point.hypotenuse",
   "budget": 700
  },
  "text": "Point.hypotenuse  def  export\nexport def hypotenuse :: f64 -> f64 -> f64\ncalls: Point.distance",
  "structured": {
   "id": "Point.hypotenuse",
   "level": "L2",
   "text": "Point.hypotenuse  def  export\nexport def hypotenuse :: f64 -> f64 -> f64\ncalls: Point.distance",
   "truncated": false
  }
 },
 {
  "tool": "tz_search",
  "title": "tz_search（想定出力）",
  "arguments": {
   "query": "distance",
   "limit": 5
  },
  "text": "2 matches for 'distance'\nPoint.distance  def  def distance :: Point -> f64\nPoint.hypotenuse  def  export def hypotenuse :: f64 -> f64 -> f64",
  "structured": {
   "query": "distance",
   "matches": [
    {
     "id": "Point.distance",
     "kind": "def",
     "signature": "def distance :: Point -> f64"
    },
    {
     "id": "Point.hypotenuse",
     "kind": "def",
     "signature": "export def hypotenuse :: f64 -> f64 -> f64"
    }
   ],
   "truncated": false
  }
 },
 {
  "tool": "tz_callers",
  "title": "tz_callers（想定出力。graph.jsonl の行に `depth` を足したもの）",
  "arguments": {
   "id": "Point.distance"
  },
  "text": "Point.hypotenuse  Point.tz:7:5  args: own",
  "structured": {
   "id": "Point.distance",
   "relation": "calls",
   "depth": 1,
   "callers": [
    {
     "rel": "calls",
     "from": "Point.hypotenuse",
     "to": "Point.distance",
     "args": [
      "own"
     ],
     "span": {
      "path": "Point.tz",
      "start": 181,
      "end": 212,
      "line": 7,
      "column": 5
     },
     "depth": 1
    }
   ],
   "truncated": false
  }
 },
 {
  "tool": "tz_find",
  "title": "tz_find（想定出力。G25 の候補）",
  "arguments": {
   "query": "f64 -> f64 -> f64"
  },
  "text": "Point.hypotenuse  export def hypotenuse :: f64 -> f64 -> f64  (call)",
  "structured": {
   "query": "f64 -> f64 -> f64",
   "candidates": [
    {
     "name": "Point.hypotenuse",
     "signature": "export def hypotenuse :: f64 -> f64 -> f64",
     "how": "call"
    }
   ],
   "truncated": false
  }
 },
 {
  "tool": "tz_impact",
  "title": "tz_impact（想定出力。`records` の中身は G28 が決める。ここは説明のための例）",
  "arguments": {
   "old": "target/tz/kb-before"
  },
  "text": "signature changed (breaking): Point.hypotenuse\n  old: export def hypotenuse :: f64 -> f64 -> f64\n  new: export def hypotenuse :: f64 -> f64 -> f64 -> f64\ntests reaching it: none",
  "structured": {
   "old": "target/tz/kb-before",
   "new": ".",
   "records": [
    {
     "change": "signature",
     "id": "Point.hypotenuse",
     "breaking": true,
     "old": "export def hypotenuse :: f64 -> f64 -> f64",
     "new": "export def hypotenuse :: f64 -> f64 -> f64 -> f64"
    }
   ],
   "truncated": false
  }
 },
 {
  "tool": "tz_fix_plan",
  "title": "tz_fix_plan（想定出力。G24 の `--dry-run` と同じ計算。W1001 の fix は G24 Phase 2 が足す。ソースは `a`（付録 G の `g29-warn`）の `Main.tz`）",
  "arguments": {
   "path": "Main.tz"
  },
  "text": "--- a/Main.tz\n+++ b/Main.tz\n@@ -1,5 +1,5 @@\n def scale :: i64 -> i64 = \\n ->\n-    let spare = 1\n+    let _spare = 1\n     n * 2\n \n scale 21\nwould apply 1 fix in 1 file",
  "structured": {
   "applicability": "certain",
   "rounds": 1,
   "stop": "fixed_point",
   "fixes": [
    {
     "code": "W1001",
     "applicability": "certain",
     "title": "prefix the unused local 'spare' with '_'",
     "round": 1,
     "edits": [
      {
       "path": "Main.tz",
       "span": {
        "start": 40,
        "end": 45,
        "line": 2,
        "column": 9,
        "end_line": 2,
        "end_column": 14
       },
       "text": "_spare"
      }
     ]
    }
   ],
   "dropped": [],
   "diff": "--- a/Main.tz\n+++ b/Main.tz\n@@ -1,5 +1,5 @@\n def scale :: i64 -> i64 = \\n ->\n-    let spare = 1\n+    let _spare = 1\n     n * 2\n \n scale 21\n",
   "remaining_errors": 0,
   "remaining_warnings": 0,
   "truncated": false
  }
 },
 {
  "tool": "tz_diagram",
  "title": "tz_diagram（想定出力。形式は G30 の付録 D の point.module.mmd）",
  "arguments": {
   "kind": "module"
  },
  "text": "flowchart TD\n    n1[\"Main\"]\n    n2[\"Point\"]\n    n1 -->|\"×2\"| n2\n",
  "structured": {
   "kind": "module",
   "format": "mermaid",
   "text": "flowchart TD\n    n1[\"Main\"]\n    n2[\"Point\"]\n    n1 -->|\"×2\"| n2\n",
   "nodes": 2,
   "truncated": false
  }
 }
]
```

