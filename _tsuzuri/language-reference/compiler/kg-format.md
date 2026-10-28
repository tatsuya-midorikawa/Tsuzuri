# 知識グラフの形式

コンパイラは、型検査と所有権解析で得た事実（宣言・呼び出し・型の使用・効果・トラップ箇所・確保箇所・複製箇所・所有権の出来事・再帰・ループ・特殊化・型のレイアウト）を、**知識グラフ**（KG）として機械が読める形に出せます。KG は型付きの有向グラフです。宣言が**ノード**、宣言どうしの関係が**辺**で、ノードの追加の事実は**性質の表**に入ります。同じソースと同じコンパイラからは、どの機械でも byte 単位で同じ内容ができます。

## この記事のポイント

- ノードは `nodes.jsonl` の 1 行、辺は `edges.jsonl` の 1 行です。テスト・事実・レイアウトは、ノードの ID で引く性質の表（`tests.jsonl`・`facts/<モジュール>.jsonl`・`layout/<モジュール>.jsonl`）です。
- ID は名前空間つきの完全名で、位置に依存しません（`Acme::Shop::Cart.total`、`test:Acme::Shop::Main.adds`）。
- `contains` の辺がモジュールから宣言、record から field、union から case、class と instance から method へ伸び、モジュールを根とする森（木の集まり）を作ります。
- 事実は `check` の解析だけから作り、LLVM IR を使いません。`effects_complete`・`alloc_complete` は、効果の要約（A20）が入るまで `false` です。
- ファイルへの書き出しと `tsuzuri kg` のコマンドは計画中です（[G27](../../../_features/G27-build-time-kb.md)）。今はライブラリの `tsuzuri::kg::collect` で作れます。

## 共通の規則

- UTF-8 の JSON Lines です。1 行に 1 つの JSON オブジェクトで、行は `\n` で終わります。行がなければ 0 byte です。`:` と `,` の後に空白はありません。
- 文字列のエスケープは診断の JSON（`check --json`）と同じです。数値は 0 以上の 10 進の整数だけです。値のないものは `null`、空の配列は `[]` です。
- キーの順序は下の各表のとおりに固定です。読み手は知らないキーを無視してください（キーが増えることがあります）。
- `span` は `{"path","start","end","line","column"}` です。`path` はプロジェクトの根からの相対パスで `/` 区切り、`start`・`end` は UTF-8 の byte 位置（`end` は含みません）、`line`・`column` は `start` の行と列（文字単位）で、どちらも 1 から数えます。
- 行の並び: `nodes`・`tests`・`facts`・`layout` は `id` の byte 順、`edges` は `rel`・`from`・`to`・`span.path`・`span.start`・`span.end` の順です。まったく同じ行は 1 行にします。
- 事実の 1 つのキーの配列は 100 件までです。超えた分は捨て、捨てた件数を `truncated` に入れます。1 関数の走査は 1,048,576 ノードまでで、超えたら `truncated` に `{"nodes":1}` が入ります。
- 型検査が通らないプロジェクトの KG は作りません（部分的な KG はありません）。

## ID

| 対象 | ID | 例 |
| --- | --- | --- |
| モジュール | 名前空間を含む完全名（`::` 区切り） | `Acme::Kg::Geometry::Metrics` |
| `def`・`const`・`extern`・record・union・型別名・class・`extern type` | `<モジュールの ID>.<名前>` | `Acme::Kg::Shapes.area` |
| field・case | `<型の ID>.<名前>` | `Acme::Kg::Shapes.Point.x` |
| class のメソッド | `<class の ID>.<名前>` | `Acme::Kg::Traits.Sized.size` |
| instance | `instance:<class の ID><<型>>` | `instance:Acme::Kg::Traits.Sized<Acme::Kg::Shapes.Point>` |
| instance のメソッド | `<instance の ID>.<名前>` | `instance:Acme::Kg::Traits.Sized<Acme::Kg::Shapes.Point>.size` |
| test・bench | `test:<モジュールの ID>.<名前>`・`bench:…` | `test:Acme::Kg::Main.total adds` |
| トップレベルのコード | `<モジュールの ID>.$entry` | `Acme::Kg::Main.$entry` |
| アクティブパターン | `<モジュールの ID>.(\|A\|B\|)` | `Main.(\|Even\|Odd\|)` |
| 組み込みのクラス・関数（辺の `to` だけ） | 名前 | `Eq`・`sqrt`・`assert` |
| std の関数（辺の `to` だけ） | `<std のモジュール>.<名前>` | `Debug.print`・`Array.map` |

同じ ID が 2 つ以上あるとき（同じ名前のテスト）は、ソースの位置の順に 2 つ目から `#2`・`#3` を付けます（`test:Main.same`・`test:Main.same#2`）。

## `nodes.jsonl`

| キー | 型 | 内容 |
| --- | --- | --- |
| `id` | 文字列 | 上の ID |
| `kind` | 文字列 | `module`・`def`・`const`・`record`・`union`・`case`・`field`・`class`・`method`・`instance`・`alias`・`test`・`bench`・`extern`・`handle`（`extern type`） |
| `module` | 文字列 | 宣言のモジュールの ID |
| `name` | 文字列 | 宣言の名前（test は名前の文字列、instance は `Class<型>`） |
| `signature` | 文字列 | 書かれたとおりの署名（`tsuzuri doc` と同じ描画）。`def score :: Classes.Score<'a> => ref 'a -> i32` |
| `params` | 配列 | 引数ごとの `{"name","type","conv"}`。`conv` は `own`（所有権を受け取る）・`ref`・`ref_mut`・`copy`（Copy の値渡し）。解決した型で決めるので、edition 2027 の読み取りの既定の引数は `ref` です。`type` には `own`・`mut` の印を含めません |
| `result` | 文字列か `null` | 結果の型（field・case・const・型別名は値の型） |
| `public`・`export`・`pure` | 真偽 | 公開か、`export def` か、`pure def` か |
| `doc`・`deprecated` | 文字列か `null` | 説明（`///`）と `@deprecated` の注記 |
| `origin` | 文字列 | `project` か `dependency:<パッケージ名>` |
| `span` | object | 名前の位置（モジュールは先頭のファイルの `0..0`、`$entry` は式全体） |

## `edges.jsonl`

キーは `rel`・`from`・`to`・`args`（`calls` だけ）・`span` です。

| `rel` | `from` | `to` | `span` |
| --- | --- | --- | --- |
| `contains` | 親（モジュール・record・union・class・instance） | 子の宣言 | 子の名前 |
| `calls` | 呼び出しを含む関数・テスト・`$entry` | 呼び先（ノード、std の関数、組み込み関数、class のメソッド） | 呼び先の式。`args` は実引数ごとの `conv`。関数を値として渡しただけなら `[]` |
| `uses_type` | 型名の使用を囲む最も内側の宣言 | record・union・class・`extern type` | 型名 |
| `implements` | 型（instance の型、`deriving` の型） | class | class 名 |
| `instance_of` | instance | class | class 名 |
| `imports` | モジュール | `using` の名前空間（書かれたまま） | `using` の宣言 |
| `tested_by` | テストから 8 段の呼び出し以内で届く関数（1 つのテストにつき 200 件まで） | テスト | テストの名前 |

`from` はつねにこの KG のノードです（`implements` の `from` だけは型で、利用者の型でなければノードではありません）。`to` は、この KG のノードか、KG の外の端点（std の関数、組み込み、名前空間）です。`contains` の辺は、モジュール以外のどのノードにもちょうど 1 本入ります。演算子のオーバーロードが呼ぶ instance のメソッドは `calls` に出ません。

## `tests.jsonl`

キーは `id`・`kind`（`test`・`bench`）・`module`・`name`・`index`（`tsuzuri test --index`・`bench --index` の番号）・`span` です。

## `facts/<モジュール>.jsonl`

ファイル名はモジュールの key（`Geometry.Metrics`）です。関数的なノード（`def`・`extern`・`test`・`bench`・instance の `method`・`$entry`）に 1 行ずつあります。ジェネリック関数は、宣言 1 つに対して特殊化したすべての実体の事実を集めます。

| キー | 内容 |
| --- | --- |
| `id` | ノードの ID |
| `effects` | 効果の名前（辞書順）。効果の要約（A20）が入るまでは `debug`・`io`・`os`・`net`・`task`・`atomic`・`rc`・`gpu`・`extern`・`user_drop` だけで、呼び出しを通して伝わります |
| `effect_sites` | 効果ごとに `{"effect","span","via"}`。`via` は、効果を直接持つ関数までの最短の呼び出しの鎖（名前のある関数の ID、近い順）。100 件を超える鎖は先頭の 100 件で、残りの数を `truncated` の `via` に足します |
| `effects_complete` | 効果の要約が全部の効果を計算したか（今は `false`） |
| `copy_sites` | 暗黙の複製 `{"span","type","kind","cost"}`（`--warn implicit-copy` の `W1006` と同じ位置） |
| `alloc_sites` | 明示の確保 `{"span","kind"}`（`new_array`・`new_list`・`new_literal`） |
| `alloc_complete` | 確保の全部か（今は `false`） |
| `trap_sites` | トラップしうる箇所 `{"span","kind","proven"}`。`kind` は `assert`・`unreachable`・`integer_division_by_zero`・`integer_division_overflow`・`bounds_check`・`range_step_zero`・`overflow`。`proven` は範囲の証明か定数で起きないと示せたもの |
| `ownership` | 所有権の出来事 `{"var","event","to","span"}`。`event` は `move`・`borrow`・`borrow_mut`・`reborrow`・`drop`・`copy`。`to` は引数として渡した呼び先（ないときはキーがありません）。`drop` は `start` と `end` が同じで、ブロックの終わりを指します |
| `recursion` | `none`・`self`・`self_tail`（自分を末尾でだけ呼ぶ）・`mutual` |
| `loops` | `{"kind","span"}`。`kind` は `while`・`for_range`・`for_each` |
| `specializations` | ジェネリック関数の特殊化した実体の数 |
| `truncated` | 配列を切った件数（キー名 → 件数） |

## `layout/<モジュール>.jsonl`

利用者の record・union の型ごとに 1 行です（型引数のある型は、使われた実体ごと）。サイズと整列はネイティブ 64 bit の**下限と上限**で、実測ではありません。

| キー | 内容 |
| --- | --- |
| `id`・`kind`・`type` | 型の ID（型引数つき）、`record` か `union`、型の表示 |
| `size`・`align` | `[下限,上限]`（byte） |
| `fields`・`reordered` | record だけ。宣言順の `{"name","type","slot"}`（`slot` はメモリ上の位置）と、並べ替えたか |
| `layout`・`tag_bytes`・`niche`・`cases` | union だけ。`enum`・`common`・`general`・`niche`・`node`（再帰する union）、タグの byte 数、niche の種類（`negative_length`・`null_pointer`・`null_descriptor`。WebAssembly には `null_pointer` の niche がありません）、case ごとの `{"name","payload"}` |
| `drop_flag` | 利用者の `Drop` の生存フラグを持つか |

## 限界

- `uses_type` は言語サーバーの索引に載る使用（署名・instance の頭・構築式・注釈）だけです。型別名の使用と、制約の class 名は出ません。
- 所有権の出来事は、ジェネリック関数の抽象の本体から 1 回だけ集めます。Copy が要る型変数の値は移動になりません。ラムダの本体の中の出来事は出ません。
- 効果は、関数の値を通した呼び出しを辿りません。
- `trap_sites` の `proven` は区間の証明を使わない保守的な判定で、生成コードが検査を省く箇所でも `false` のことがあります。

## ライブラリから作る

Rust のライブラリとして、プロジェクトを 1 回解析して KG を作れます。

```rust
let project = tsuzuri::driver::Project::load(std::path::Path::new("."))?;
let graph = tsuzuri::kg::collect(&project).expect("the project type-checks");
graph.write_nodes(&mut std::io::stdout())?;
graph.write_edges(&mut std::io::stdout())?;
for module in graph.fact_modules() {
    graph.write_facts(module, &mut std::io::stdout())?;
}
```
