# G28 付録: 実例・参照実装・テストの詳細

[G28-kb-query-and-impact.md](G28-kb-query-and-impact.md) の付録。A 実測と `kb_shop_head`、B 照会の全出力、C `impact` の全出力、D 参照実装（Python）、E Rust の型の全フィールド、F 各テストの入力・期待と測定の手順。

**出力の素性**: HEAD には `kb` コマンドがないので、B・C の出力は実コンパイラの出力ではなく、**参照実装（D）が KB の行から作った想定出力**である。KB の行は G27 の付録 D（`kb_shop`）の手書きの行で、`kb_shop_head` の行も同じ手順で作った。コンパイラの診断・`api-diff`・`test`・`git` の出力は HEAD の `target/release/tsuzuri` と git 2.55.0 の実測。B・C の各ブロックは D のスクリプトの出力と 1 byte ずつ照合した。facts の値（`effects`・`*_sites`・`recursion`）と `main` の引数名 `arg1` は G26 の仕様から手で導いた値なので、G26 の実装が違う行を出したら、期待値を G26 の出力から作り直す（実装者が直すのは期待値のファイルだけ）。

## 付録 A: 実測と `kb_shop_head`

### A-1: 実測（HEAD `9dd0fbbc`。`target/release/tsuzuri` 9,465,008 byte、`rustc 1.98.1`）

測定した機械は他の作業で load average が 40〜55 だった。**時間は 1.2〜6 倍に膨らむので、傾向だけを読む**（G27 の付録 A は静かなときの値）。

| 測ったもの | 方法 | 結果 |
| --- | --- | --- |
| `check`（KB なし）。1／200／1,000 モジュール | G27 付録 A-1 の `gen_project.py`・`median_ms.py` | 39／70／446 ms（G27 付録 A の表） |
| `api-diff` と `check`（`shop`。5 回の中央値） | `python3 median_ms.py 5 target/release/tsuzuri api-diff shop shop-head`、`… check shop` | 44.0 ms、38.2 ms |
| 合成 KB の読み込み。`symbols.jsonl` と `graph.jsonl` の全行を Python の `json` で解析（5 回の中央値） | `python3 kbread_bench.py 200`、`… 1000`（下のスクリプト） | 200 モジュール（symbols 2,000 行 586,200 B、graph 796 行 116,216 B）8.0 ms。1,000 モジュール（symbols 10,000 行 2,931,000 B、graph 3,996 行 583,416 B）47.1 ms |
| 同じ KB で `callers --depth 8`（逆引き表を作った後の幅優先） | 同上 | 8 個の ID を 0.011 ms |

読み方: 1,000 モジュールの KB を**遅い Python で読み切る時間（47 ms）が `check` の 446 ms より短い**。Rust の読み手はさらに速いので、照会は KB を読む設計で足りる。

`kbread_bench.py`（20 行）: G27 付録 F-3 の `syn(n)` の行を `json.dumps(row, ensure_ascii=False, separators=(",", ":"))` で 1 行ずつ `symbols.jsonl`・`graph.jsonl` に書き、全行を `json.loads` で読み直す時間（5 回の中央値）と、graph の `to`→`from` の表を作って `M0000.f0_0000` から深さ 8 まで幅優先にたどる時間を測る。

`test --affected` が頼る既存の動き（`target/release/tsuzuri test shop …`。警告は標準エラー）。`--list` の番号が `tests.jsonl` の `index`（0 始まり）と一致し、実行の出力は 1 始まりの番号で、選ばれなかったテストは `ignored` に数えられる:

```text
$ tsuzuri test shop --list
0 Cart.line total
1 Cart.empty cart
2 Main.main runs
$ tsuzuri test shop --index 1
ok 2 - Cart empty cart

1 passed; 0 failed; 2 ignored
$ tsuzuri test shop --index 0 --index 2
ok 1 - Cart line total
ok 3 - Main main runs

2 passed; 0 failed; 1 ignored
```

### A-2: `tests/fixtures/kb_shop_head/`（Phase 3）

`kb_shop`（G27 付録 D-1）の編集版。`Tsuzuri.toml` は `kb_shop` と同じ（`name = "shop"`、`version = "0.1.0"`、`namespace = "Acme::Shop"`。バージョンを変えないので `api-diff` の末尾に `note:` が付く）。変えたもの: (1) `Line` に `tax: i64`（field の追加）、(2) `line_total` が `IO.write_line line.quantity` を呼び、`+ line.tax`（効果 `io` の追加）、(3) `discount` の追加（`let rate = 5` が W1001）、(4) `Report.print_total` の引数が `ref [Line]` から `[Line]`（署名と所有権の変更）、`Main` の呼び出しを合わせる。`fmt --check`（終了コード 0）・`check`（警告 2 件）・`test`（3 件通る）を実測した。

`Tsuzuri.toml`:

```toml
[package]
name = "shop"
version = "0.1.0"
edition = "2026"
namespace = "Acme::Shop"
```

`Cart.tz`:

```text
namespace Acme::Shop

/// One line of a cart: a product, how many, and its price in cents.
record Line { sku: string, quantity: i64, unit_price: i64, tax: i64 }

/// The price of one line in cents.
def line_total :: ref Line -> i64 = \line ->
    do! IO.write_line line.quantity
    line.quantity * line.unit_price + line.tax

/// The price of all lines in cents.
def total :: ref [Line] -> i64 = \lines ->
    let mut sum = 0
    let mut i = 0
    while i < lines.length do
        sum = sum + line_total (ref lines[i])
        i = i + 1
    sum

/// The price of all lines after a percentage discount.
def discount :: ref [Line] -> i64 -> i64 = \lines percent ->
    let rate = 5
    total lines * (100 - percent) / 100

test "line total" = assert (line_total (ref Line { sku: "pen", quantity: 3, unit_price: 100, tax: 0 }) == 300)
test "empty cart" = assert (total (ref new [Line](0, i -> Line { sku: "x", quantity: 0, unit_price: 0, tax: 0 })) == 0)
```

`Report.tz`:

```text
namespace Acme::Shop

using Acme::Shop

def print_total :: [Line] -> unit = \lines ->
    do! IO.write_line (Cart.total (ref lines))
```

`Main.tz`:

```text
namespace Acme::Shop

def main :: unit -> i32 = \() ->
    let lines = [Line { sku: "pen", quantity: 3, unit_price: 100, tax: 0 }]
    Report.print_total lines
    0

test "main runs" = assert (Cart.total (ref [Line { sku: "ink", quantity: 1, unit_price: 250, tax: 0 }]) == 250)
```

`check` の警告（標準エラー。`i` の警告は `kb_shop` では 19 行だった）:

```text
Cart.tz:22:9: warning[W1001]: unused local 'rate'; prefix it with '_' to silence this warning
Cart.tz:26:54: warning[W1001]: unused local 'i'; prefix it with '_' to silence this warning
```

`api-diff kb_shop kb_shop_head`（実測。終了コード 1。`--json` は同じ内容の 3 行の `change` と、`{"type":"summary","breaking":2,"non_breaking":1,"omitted":0,"old_version":"0.1.0","new_version":"0.1.0","compatible_versions":true}`）:

```text
breaking: added field Acme::Shop::Cart.Line.tax: `tax: i64` (constructions of the record without it stop compiling)
breaking: changed def Acme::Shop::Report.print_total: `def print_total :: ref [Line] -> unit` => `def print_total :: [Line] -> unit`
non-breaking: added def Acme::Shop::Cart.discount: `def discount :: ref [Line] -> i64 -> i64`
summary: 2 breaking, 1 non-breaking
note: version 0.1.0 is compatible with 0.1.0, so registry dependents that accept 0.1.0 would take it; raise the major version (the minor version before 1.0.0) for a breaking change
```

### A-3: git の実測（`--git REV` の手順 1〜6）

`$WORK` は作業場（`gitdemo` は `kb_shop` の 4 ファイルを 1 コミット `base` にした使い捨てのリポジトリ）。ハッシュは環境ごとに違う。出力は実測。`[exit N]` は終了コードの注記。

```text
$ git -C gitdemo rev-parse --show-toplevel
$WORK/gitdemo
$ git -C gitdemo rev-parse --verify --quiet --end-of-options HEAD^{commit}
be3b94c9dddbe82d5c5bc775edb9b561edc5b012                                    [exit 0]
$ git -C gitdemo rev-parse --verify --quiet --end-of-options nope^{commit}
                                                                            [exit 1。出力なし]
$ git -C "$HOME" rev-parse --show-toplevel
fatal: not a git repository (or any of the parent directories): .git        [exit 128]
$ git -c core.hooksPath=$WORK/wt-demo/no-hooks -C gitdemo worktree add --detach $WORK/wt-demo/checkout be3b94c9dddbe82d5c5bc775edb9b561edc5b012
Preparing worktree (detached HEAD be3b94c)
HEAD is now at be3b94c base
$ git -C gitdemo worktree list
$WORK/gitdemo          be3b94c [master]
$WORK/wt-demo/checkout be3b94c (detached HEAD)
$ git -C gitdemo worktree remove --force $WORK/wt-demo/checkout
$ git -C gitdemo worktree list
$WORK/gitdemo be3b94c [master]
```

前後で `git status --short` と `git stash list` は同じ（実測）。ほかに確かめたこと:

- **フック**: `.git/hooks/post-checkout`（`echo ran > "$PWD/hook-ran.txt"`）を置くと、`git worktree add` は新しい worktree に `hook-ran.txt` を作る。`-c core.hooksPath=<空のディレクトリ>` を付けると作らない。
- **残った登録**: `git worktree add` の後でそのディレクトリを消す（`Drop` が走らない状況の再現）と、`git worktree list` に `… (detached HEAD) prunable` と出る。別のパスへの `git worktree add` は成功し、`git worktree remove --force <消えたパス>` は終了コード 0 で登録を消す。**`worktree prune` は全 worktree に効く**ので呼ばない。
- **`failure_line`**: 空でないディレクトリへの `worktree add` は終了コード 128、標準エラーは `Preparing worktree (detached HEAD be3b94c)` と `fatal: '$WORK/wt-exists' already exists`。`failure_line` は `fatal:` の行を返す。

## 付録 B: 照会の全出力

### B-0: 使う KB

`tests/fixtures/kb_shop`（G27 付録 D）の KB。ファイルは G27 の付録 D-2〜D-11。symbols は 14 行（module 3、record `Cart.Line` と field 3、def 4＝`line_total`・`total`・`main`・`print_total`、test 3。`test` の 3 行だけ `public` が偽）。graph は 21 辺（`calls` 7、`imports` 1、`tested_by` 5、`uses_type` 8）。`calls` と `tested_by`（`Acme::Shop::` と `test:Acme::Shop::` を略した。`IO.write_line` は graph の `to` にだけ出る ID）:

```text
calls     Cart.total -> Cart.line_total          Cart.tz:14:21  ref
calls     Main.main -> Report.print_total        Main.tz:5:5    ref
calls     Report.print_total -> Cart.total       Report.tz:6:24 ref
calls     Report.print_total -> IO.write_line    Report.tz:6:9  copy
calls     test:Cart.empty cart -> Cart.total     Cart.tz:19:29  ref
calls     test:Cart.line total -> Cart.line_total Cart.tz:18:29 ref
calls     test:Main.main runs -> Cart.total      Main.tz:8:28   ref
tested_by Cart.line_total -> test:Cart.empty cart, test:Cart.line total, test:Main.main runs
tested_by Cart.total -> test:Cart.empty cart, test:Main.main runs
```

facts: `Main.main` と `Report.print_total` が `io`、ほかは効果なし。`Cart.total` に trap site が 1 つ。

### B-1: ID の解決（§2 の実例）

コマンドは `tsuzuri kb show TEXT --path tests/fixtures/kb_shop`。「決まった ID」は `show` の 1 行目に出る。

| `TEXT` | 段 | 結果 |
| --- | --- | --- |
| `Acme::Shop::Cart.total` | 1 | `Acme::Shop::Cart.total` |
| `Line` | 2（`.Line` で終わる 1 件） | `Acme::Shop::Cart.Line` |
| `main` | 2 | `Acme::Shop::Main.main` |
| `Cart` | 2（`::Cart` で終わる 1 件） | `Acme::Shop::Cart`（module） |
| `line total` | 2 | `test:Acme::Shop::Cart.line total` |
| `Acme::Shop::Cart.l` | 5（`Acme::Shop::Cart.l` で始まる 1 件。大文字の `Line` は別） | `Acme::Shop::Cart.line_total` |
| `t` | 5 で 3 件 | 曖昧 |
| `totl` | どの段にもない | 未知（候補 1 件） |
| `zzz` | どの段にもない | 未知（候補 0 件） |

エラー（標準エラー。`--json` のときは 1 行の JSON）:

```text
$ tsuzuri kb show t --path tests/fixtures/kb_shop
<command line>:1:1: error[E2000]: ambiguous id 't'; candidates: test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Cart.line total, test:Acme::Shop::Main.main runs     [exit 1]
$ tsuzuri kb show totl --path tests/fixtures/kb_shop
<command line>:1:1: error[E2000]: unknown id 'totl'; did you mean: Acme::Shop::Cart.total     [exit 1]
$ tsuzuri kb show zzz --path tests/fixtures/kb_shop
<command line>:1:1: error[E2000]: unknown id 'zzz'; use tsuzuri kb search TEXT to find ids     [exit 1]
$ tsuzuri kb show "" --path tests/fixtures/kb_shop
<command line>:1:1: error[E2000]: id must not be empty     [exit 2]
```

`--json` のときは `{"severity":"error","code":"E2000","message":"ambiguous id 't'; candidates: …","path":"<command line>","span":{"start":0,"end":0,"line":1,"column":1,"end_line":1,"end_column":1}}` の 1 行（`api-diff` の使い方の誤りと同じ形）。

`IO.writ_line`（`IO.write_line` の綴り違い）→ 名前 `writ_line`（9 字。距離 2 まで）で、graph にだけある `IO.write_line`（名前 `write_line`、距離 1）が候補: `unknown id 'IO.writ_line'; did you mean: IO.write_line`。なお `IO.write_lin` は段 5（前方一致）で `IO.write_line` に決まる。

### B-2: テキストの出力

コマンドはすべて `--path tests/fixtures/kb_shop` 付き。

`tsuzuri kb callers Acme::Shop::Cart.line_total`:

```text
callers of Acme::Shop::Cart.line_total (depth 1)
1 Acme::Shop::Cart.total  Cart.tz:14:21  args ref
1 test:Acme::Shop::Cart.line total  Cart.tz:18:29  args ref
summary: 2 callers, depth 1
```

`tsuzuri kb callers Acme::Shop::Cart.line_total --depth 3`（`via` は 1 つ手前の ID。位置はその ID との辺）:

```text
callers of Acme::Shop::Cart.line_total (depth 3)
1 Acme::Shop::Cart.total  Cart.tz:14:21  args ref
1 test:Acme::Shop::Cart.line total  Cart.tz:18:29  args ref
2 Acme::Shop::Report.print_total  Report.tz:6:24  via Acme::Shop::Cart.total
2 test:Acme::Shop::Cart.empty cart  Cart.tz:19:29  via Acme::Shop::Cart.total
2 test:Acme::Shop::Main.main runs  Main.tz:8:28  via Acme::Shop::Cart.total
3 Acme::Shop::Main.main  Main.tz:5:5  via Acme::Shop::Report.print_total
summary: 6 callers, depth 3
```

`tsuzuri kb callees Acme::Shop::Main.main --depth 3`（`IO.write_line` は graph にだけある ID）:

```text
callees of Acme::Shop::Main.main (depth 3)
1 Acme::Shop::Report.print_total  Main.tz:5:5  args ref
2 Acme::Shop::Cart.total  Report.tz:6:24  via Acme::Shop::Report.print_total
2 IO.write_line  Report.tz:6:9  via Acme::Shop::Report.print_total
3 Acme::Shop::Cart.line_total  Cart.tz:14:21  via Acme::Shop::Cart.total
summary: 4 callees, depth 3
```

`tsuzuri kb uses Acme::Shop::Cart.Line`:

```text
uses of Acme::Shop::Cart.Line
uses_type Acme::Shop::Cart.line_total  Cart.tz:7:23
uses_type Acme::Shop::Cart.total  Cart.tz:10:19
uses_type Acme::Shop::Main.main  Main.tz:4:18
uses_type Acme::Shop::Report.print_total  Report.tz:5:25
uses_type test:Acme::Shop::Cart.empty cart  Cart.tz:19:45
uses_type test:Acme::Shop::Cart.empty cart  Cart.tz:19:59
uses_type test:Acme::Shop::Cart.line total  Cart.tz:18:45
uses_type test:Acme::Shop::Main.main runs  Main.tz:8:45
summary: 8 uses
```

`tsuzuri kb tests-for Acme::Shop::Cart.line_total`:

```text
tests for Acme::Shop::Cart.line_total
test:Acme::Shop::Cart.empty cart  Cart.tz:19:6
test:Acme::Shop::Cart.line total  Cart.tz:18:6
test:Acme::Shop::Main.main runs  Main.tz:8:6
summary: 3 tests
```

`tsuzuri kb search total`（行頭の番号は連番。JSON の `rank` は順位: `total` は名前が一致で 1、`line_total`・`print_total`・test `line total` は名前に含むで 3）:

```text
search 'total' (project)
1 Acme::Shop::Cart.total  def  def total :: ref [Line] -> i64
2 Acme::Shop::Cart.line_total  def  def line_total :: ref Line -> i64
3 Acme::Shop::Report.print_total  def  def print_total :: ref [Line] -> unit
4 test:Acme::Shop::Cart.line total  test  test "line total"
summary: 4 matches
```

（`main runs` の test は名前に `total` を含まないので出ない。）

`tsuzuri kb show Acme::Shop::Cart.total`:

```text
Acme::Shop::Cart.total
def | public | Cart.tz:10:5
signature: def total :: ref [Line] -> i64
doc: The price of all lines in cents.
params:
  lines  ref  ref [Line]
result: i64
effects: none
sites: trap 1, alloc 0, copy 0
callees (1):
  Acme::Shop::Cart.line_total  Cart.tz:14:21  args ref
callers (3):
  Acme::Shop::Report.print_total  Report.tz:6:24  args ref
  test:Acme::Shop::Cart.empty cart  Cart.tz:19:29  args ref
  test:Acme::Shop::Main.main runs  Main.tz:8:28  args ref
tests (2):
  test:Acme::Shop::Cart.empty cart  Cart.tz:19:6
  test:Acme::Shop::Main.main runs  Main.tz:8:6
```

`tsuzuri kb show Acme::Shop::Cart.Line`（record。`fields` は宣言順。graph の `used by`）:

```text
Acme::Shop::Cart.Line
record | public | Cart.tz:4:8
signature: record Line
doc: One line of a cart: a product, how many, and its price in cents.
fields:
  sku: string
  quantity: i64
  unit_price: i64
used by (8):
  Acme::Shop::Cart.line_total  uses_type  Cart.tz:7:23
  Acme::Shop::Cart.total  uses_type  Cart.tz:10:19
  Acme::Shop::Main.main  uses_type  Main.tz:4:18
  Acme::Shop::Report.print_total  uses_type  Report.tz:5:25
  test:Acme::Shop::Cart.empty cart  uses_type  Cart.tz:19:45
  test:Acme::Shop::Cart.empty cart  uses_type  Cart.tz:19:59
  test:Acme::Shop::Cart.line total  uses_type  Cart.tz:18:45
  test:Acme::Shop::Main.main runs  uses_type  Main.tz:8:45
```

`tsuzuri kb show Acme::Shop::Main.main`（`doc` がないので行がない。引数名 `arg1` は G26 の `unit` 引数の仮定）:

```text
Acme::Shop::Main.main
def | public | Main.tz:3:5
signature: def main :: unit -> i32
params:
  arg1  copy  unit
result: i32
effects: io
sites: trap 0, alloc 0, copy 0
callees (1):
  Acme::Shop::Report.print_total  Main.tz:5:5  args ref
```

`tsuzuri kb show IO.write_line`（graph にだけある ID。symbols にないので 2 行と `callers` だけ）:

```text
IO.write_line
symbol: not in this KB
callers (1):
  Acme::Shop::Report.print_total  Report.tz:6:9  args copy
```

### B-3: `--json` の行

`kb callers Acme::Shop::Cart.line_total --json`（`span` は G26 の形、`args` は深さ 1 だけ）:

```json
{"type":"edge","depth":1,"id":"Acme::Shop::Cart.total","via":null,"sites":1,"span":{"path":"Cart.tz","start":434,"end":444,"line":14,"column":21},"args":["ref"]}
{"type":"edge","depth":1,"id":"test:Acme::Shop::Cart.line total","via":null,"sites":1,"span":{"path":"Cart.tz","start":515,"end":525,"line":18,"column":29},"args":["ref"]}
{"type":"summary","count":2,"depth":1}
```

`kb tests-for Acme::Shop::Cart.line_total --json` は `{"type":"test","id":"test:Acme::Shop::Cart.empty cart","span":{"path":"Cart.tz","start":595,"end":607,"line":19,"column":6}}` の形の 3 行（テキストと同じ順）と `{"type":"summary","count":3}`。

`kb search total --json`（`rank` は順位。`test` は `name` に含む＝3）:

```json
{"type":"match","rank":1,"id":"Acme::Shop::Cart.total","kind":"def","signature":"def total :: ref [Line] -> i64"}
{"type":"match","rank":3,"id":"Acme::Shop::Cart.line_total","kind":"def","signature":"def line_total :: ref Line -> i64"}
{"type":"match","rank":3,"id":"Acme::Shop::Report.print_total","kind":"def","signature":"def print_total :: ref [Line] -> unit"}
{"type":"match","rank":3,"id":"test:Acme::Shop::Cart.line total","kind":"test","signature":"test \"line total\""}
{"type":"summary","count":4}
```

`kb uses Acme::Shop::Cart.Line --json` は `{"type":"use","rel":"uses_type","from":"Acme::Shop::Cart.line_total","span":{"path":"Cart.tz","start":210,"end":214,"line":7,"column":23}}` の形の 8 行（テキストの 8 行と同じ順）と `{"type":"summary","count":8}`。

`kb show Acme::Shop::Main.main --json`（`symbol` は symbols の行、`facts` は facts の行そのまま、`callees`・`callers`・`tests`・`used_by` は graph の行）:

```json
{"type":"symbol","symbol":{"id":"Acme::Shop::Main.main","kind":"def","module":"Acme::Shop::Main","name":"main","signature":"def main :: unit -> i32","params":[{"name":"arg1","type":"unit","conv":"copy"}],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":26,"end":30,"line":3,"column":5}},"facts":{"id":"Acme::Shop::Main.main","effects":["io"],"effect_sites":[{"effect":"io","span":{"path":"Main.tz","start":127,"end":145,"line":5,"column":5},"via":["Acme::Shop::Report.print_total","IO.write_line"]}],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}},"callees":[{"rel":"calls","from":"Acme::Shop::Main.main","to":"Acme::Shop::Report.print_total","args":["ref"],"span":{"path":"Main.tz","start":127,"end":145,"line":5,"column":5}}],"callers":[],"tests":[],"used_by":[]}
```

`kb show IO.write_line --json`:

```json
{"type":"symbol","symbol":null,"facts":null,"callees":[],"callers":[{"rel":"calls","from":"Acme::Shop::Report.print_total","to":"IO.write_line","args":["copy"],"span":{"path":"Report.tz","start":98,"end":111,"line":6,"column":9}}],"tests":[],"used_by":[]}
```

`kb brief Acme::Shop::Cart.total --json`（`text` は `brief/L2/Acme.Shop.Cart.total.md` と同じ）:

```json
{"type":"brief","level":"L2","id":"Acme::Shop::Cart.total","text":"Acme::Shop::Cart.total\ndef | public | Cart.tz:10\nsig: def total :: ref [Line] -> i64\nparams: lines:ref\ndoc: The price of all lines in cents.\nsites: trap 1\ncallees (1): Acme::Shop::Cart.line_total\ncallers (3): Acme::Shop::Report.print_total, test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs\ntests (2): test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs\n"}
```

### B-4: `--budget` と graph のない KB

`kb brief --budget 450`（`--budget` なしは `brief/L0.md`＝609 文字そのまま。450 は 439 文字。ブロックの行を後ろから落とし `…N more`。`Modules` は 3 行のうち後ろ 1 行を落とした）:

```text
# shop 0.1.0
tz-kb/1 | edition 2026 | modules 3 | symbols 14 | tests 3
status: ok, 1 warning
entry: Acme::Shop::Main.main

## Modules
- Acme::Shop::Cart (Cart.tz): record Line; def line_total, total
- Acme::Shop::Main (Main.tz): def main
…1 more

## Dependencies
…2 more

## Tests
…2 more

## Effects (functions)
…1 more

next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl ('::' is '.' in file names)
```

`kb show Acme::Shop::Cart.total --budget 330`（`used by`・`tests`・`callers` の項目から落ち、見出しは残る。321 文字）:

```text
Acme::Shop::Cart.total
def | public | Cart.tz:10:5
signature: def total :: ref [Line] -> i64
doc: The price of all lines in cents.
params:
  lines  ref  ref [Line]
result: i64
effects: none
sites: trap 1, alloc 0, copy 0
callees (1):
  Acme::Shop::Cart.line_total  Cart.tz:14:21  args ref
callers (3):
tests (2):
…5 more
```

`kb callers Acme::Shop::Cart.line_total --depth 3 --budget 160`（見出しと `summary` は残り、項目は後ろから落ちる）:

```text
callers of Acme::Shop::Cart.line_total (depth 3)
1 Acme::Shop::Cart.total  Cart.tz:14:21  args ref
…5 more
summary: 6 callers, depth 3
```

`graph.jsonl` がない KB（G26 の Phase 2 が未出荷のコンパイラが作ったもの。テストは `kb_shop` の KB から `graph.jsonl` と `tests.jsonl` を消し、`--no-refresh` で読む）での `kb callers Acme::Shop::Cart.total`:

```text
callers of Acme::Shop::Cart.total (depth 1)
note: this KB has no graph.jsonl
summary: 0 callers, depth 1
```

`--json` は `{"type":"note","message":"this KB has no graph.jsonl"}` と `{"type":"summary","count":0,"depth":1}` の 2 行。

## 付録 C: `impact` の全出力

終了コードは全場面で 0。ハッシュ `3f9a1c7d2b04` は例（テストは `git rev-parse` の先頭 12 桁に置き換えて照合する）。ID は G26 の綴り。

### C-1: 場面 A（`kb_shop` → `kb_shop_head`）

`tsuzuri kb impact --git <kb_shop をコミットした REV> --path tests/fixtures/kb_shop_head`（`--from tests/fixtures/kb_shop --path tests/fixtures/kb_shop_head` なら 1 行目は `impact: tests/fixtures/kb_shop -> working tree`）:

```text
impact: git 3f9a1c7d2b04 -> working tree
breaking: added field Acme::Shop::Cart.Line.tax: `tax: i64` (constructions of the record without it stop compiling)
breaking: added effect io on Acme::Shop::Cart.line_total (reached by 4 functions and 3 tests)
breaking: added effect io on Acme::Shop::Cart.total (reached by 3 functions and 2 tests)
breaking: changed def Acme::Shop::Report.print_total: `def print_total :: ref [Line] -> unit` => `def print_total :: [Line] -> unit`
non-breaking: added def Acme::Shop::Cart.discount: `def discount :: ref [Line] -> i64 -> i64`
info: new warning W1001 at Cart.tz:22: unused local 'rate'; prefix it with '_' to silence this warning
tests: 3 of 3 affected
  test:Acme::Shop::Cart.empty cart
  test:Acme::Shop::Cart.line total
  test:Acme::Shop::Main.main runs
summary: 4 breaking, 1 non-breaking, 1 info
```

各行の出どころ:

| 行 | 決定表 | 理由 |
| --- | --- | --- |
| `added field …Line.tax` | symbols 5 | 公開の record に field。`api-diff` と同じ文字列 |
| `added effect io on …line_total (reached by 4 functions and 3 tests)` | facts 1 | `effects` が `[]`→`["io"]`、`public`。head の `calls` を逆向きに深さ 8 まで: 関数 `total`・`discount`・`print_total`・`main`、test 3 つ（`discount` は head で `total` を呼ぶ） |
| `added effect io on …total (reached by 3 functions and 2 tests)` | facts 1 | 関数 `print_total`・`discount`・`main`、test `empty cart`・`main runs` |
| `changed def …print_total` | symbols 7 | `signature` が違う（`conv` も `ref`→`own` だが、行 8 は `signature` が同じときだけ）。`api-diff` と同じ文字列 |
| `added def …discount` | symbols 6 | 公開の関数の追加。`api-diff` と同じ文字列 |
| `new warning W1001 at Cart.tz:22: …` | 警告 | base は `(W1001, unused local 'i'…, Cart.tz)` が 1 つ、head は `i`（26 行に動いた）と `rate`。多重集合の差は `rate` だけ |
| `tests: 3 of 3 affected` | tests の最後の行 | `Cart`・`Main`・`Report` の 3 つとも `source` が違い、変更モジュールの test と、そこへ届く test がすべて |

`--json`（`api-diff` と同じキーの `change` の行、`tests`、`summary`）:

```json
{"type":"impact","base":"git 3f9a1c7d2b04","head":"working tree"}
{"type":"change","severity":"breaking","action":"added","kind":"field","name":"Acme::Shop::Cart.Line.tax","old":null,"new":"tax: i64","note":"constructions of the record without it stop compiling"}
{"type":"change","severity":"breaking","action":"added","kind":"effect","name":"Acme::Shop::Cart.line_total","old":null,"new":"io","note":"reached by 4 functions and 3 tests"}
{"type":"change","severity":"breaking","action":"added","kind":"effect","name":"Acme::Shop::Cart.total","old":null,"new":"io","note":"reached by 3 functions and 2 tests"}
{"type":"change","severity":"breaking","action":"changed","kind":"def","name":"Acme::Shop::Report.print_total","old":"def print_total :: ref [Line] -> unit","new":"def print_total :: [Line] -> unit","note":null}
{"type":"change","severity":"non-breaking","action":"added","kind":"def","name":"Acme::Shop::Cart.discount","old":null,"new":"def discount :: ref [Line] -> i64 -> i64","note":null}
{"type":"change","severity":"info","action":"new","kind":"warning","name":"W1001","old":null,"new":"Cart.tz:22: unused local 'rate'; prefix it with '_' to silence this warning","note":null}
{"type":"tests","mode":"some","affected":["test:Acme::Shop::Cart.empty cart","test:Acme::Shop::Cart.line total","test:Acme::Shop::Main.main runs"],"total":3,"reason":null}
{"type":"summary","breaking":4,"non_breaking":1,"info":1,"omitted":0}
```

`mode` が `all` のとき `affected` は `[]` で、`reason` に理由が入る（場面 E の JSON）。

### C-2: 影響を受けるテストの場面（symbols・facts の変更なし）

`kb_shop` の KB と、**1 つのファイルの末尾に `\n// touched\n` を足した**プロジェクトの KB（コメントなのでシンボル・graph・facts は同じで、そのファイルの `modules[].source` と `source_hash` だけが違う。`check` は通る）。

場面 B（`Main.tz`）: 変更モジュールは `Main`。`Main` の test と、`Main` の関数へ届く test（なし）だけ。

```text
impact: git 3f9a1c7d2b04 -> working tree
tests: 1 of 3 affected
  test:Acme::Shop::Main.main runs
summary: 0 breaking, 0 non-breaking, 0 info
```

場面 C（`Cart.tz`）: `Cart` の test 2 つと、`Cart.total` を呼ぶ `Main.main runs`。

```text
impact: git 3f9a1c7d2b04 -> working tree
tests: 3 of 3 affected
  test:Acme::Shop::Cart.empty cart
  test:Acme::Shop::Cart.line total
  test:Acme::Shop::Main.main runs
summary: 0 breaking, 0 non-breaking, 0 info
```

場面 D（`Report.tz`）: `print_total` を呼ぶのは `main` だけで、`main` を呼ぶ test はない。

```text
impact: git 3f9a1c7d2b04 -> working tree
tests: 0 of 3 affected
summary: 0 breaking, 0 non-breaking, 0 info
```

場面 E（head の KB の `compiler` が `tsuzuri 0.2.0`。ほかは base と同じ）:

```text
impact: git 3f9a1c7d2b04 -> working tree
info: skipped facts (the two KBs were built by different compilers or one has no facts)
tests: all 3 (the compiler changed)
summary: 0 breaking, 0 non-breaking, 1 info
```

```json
{"type":"impact","base":"git 3f9a1c7d2b04","head":"working tree"}
{"type":"change","severity":"info","action":"skipped","kind":"facts","name":"","old":null,"new":null,"note":"the two KBs were built by different compilers or one has no facts"}
{"type":"tests","mode":"all","affected":[],"total":3,"reason":"the compiler changed"}
{"type":"summary","breaking":0,"non_breaking":0,"info":1,"omitted":0}
```

場面 F（head の KB に `graph.jsonl` がなく、`Cart` の `source` が違う）:

```text
impact: git 3f9a1c7d2b04 -> working tree
tests: all 3 (the KB has no graph)
summary: 0 breaking, 0 non-breaking, 0 info
```

場面 G（head に公開の関数 `extra0000`〜`extra1002` の 1,003 個を足す。`MAX_LISTED_CHANGES` は 1,000。先頭の 1,000 行は `non-breaking: added def Acme::Shop::Cart.extraNNNN: \`def extraNNNN :: i64 -> i64\`` で、末尾の 7 行だけを示す。`summary` と `omitted` は全件を数える）:

```text
non-breaking: added def Acme::Shop::Cart.extra0999: `def extra0999 :: i64 -> i64`
note: 3 more changes not shown
tests: 3 of 3 affected
  test:Acme::Shop::Cart.empty cart
  test:Acme::Shop::Cart.line total
  test:Acme::Shop::Main.main runs
summary: 0 breaking, 1003 non-breaking, 0 info
```

JSON の最後の行は `{"type":"summary","breaking":0,"non_breaking":1003,"info":0,"omitted":3}`、`change` の行は 1,000 本。

### C-3: `tsuzuri test --affected`

`tsuzuri test edits/E1 --affected shop`（`shop` は `kb_shop`）。編集 `E1`〜`E5` は `kb_shop` の 1 行を直したもの（付録 F-6）。1 行目の `affected:` は実行の前に標準出力へ出る。落ちたテストは HEAD のコンパイラで実測した。

| 編集 | `affected:` の行 | 落ちるテスト（実測） |
| --- | --- | --- |
| E1 `Cart.tz` の `line.quantity * line.unit_price` を `+` に | `affected: 3 of 3 tests (shop)` | `Cart line total`・`Main main runs` |
| E2 `Main.tz` の期待値 `== 250` を `== 251` に | `affected: 1 of 3 tests (shop)` | `Main main runs` |
| E3 `Cart.tz` の `let mut sum = 0` を `1` に | `affected: 3 of 3 tests (shop)` | `Cart empty cart`・`Main main runs` |
| E4 `Report.tz` の `Cart.total lines` を `Cart.total lines + 0` に | `affected: 0 of 3 tests (shop)`（何も実行せず終了コード 0） | なし |
| E5 `Main.tz` の戻り値 `0` を `1` に | `affected: 1 of 3 tests (shop)` | なし |

`affected:` の行の `3 of 3` は場面 C、`1 of 3` は場面 B、`0 of 3` は場面 D と同じ計算（編集した 1 ファイルの `source` だけが違う）。`E2` は `--index 2` で実行される（A-1 の実測: `ok 3 - Main main runs`）。`--json` の行:

```json
{"type":"affected","mode":"some","affected":1,"total":3,"base":"shop","reason":null}
{"type":"affected","mode":"all","affected":3,"total":3,"base":"shop","reason":"the compiler changed"}
```

## 付録 D: 参照実装（Python 3.9 以上）

B・C の出力はこの 3 つのファイルが作った。`cards.py` は G27 の付録 F-1（`Card`・`norm`・`clip`・`first_line`・`plural`・`l0`・`l1`・`l2`・`Kb`）。作業場の 1 つのディレクトリに `cards.py`・`kbload.py`・`query_ref.py`・`impact_ref.py` を置く。KB のディレクトリは G27 の付録 D-2〜D-11 のブロックを同名のファイルに書いたもの。使い方:

```python
from kbload import load, diag_tuples
from query_ref import show_text, calls_text, resolve
import impact_ref
base, base_manifest, base_diag = load("kbout/base")      # (Kb, manifest の dict, diag の行)
head, head_manifest, head_diag = load("kbout/head")
print(show_text(base, "Acme::Shop::Cart.total"), end="")
print(impact_ref.impact_text(base, head, base_manifest, head_manifest, diag_tuples(base_diag), diag_tuples(head_diag), "git 3f9a1c7d2b04", "working tree"), end="")
```

実装との対応: `resolve`→`query::resolve`、`osa`→`suggest::osa_distance`、`walk`→`callers`・`callees` の幅優先、`show_text`・`*_text`→`query::*`、`symbol_changes`・`fact_changes`・`diag_changes`→`impact::diff` の②、`affected_tests`→④、`sort_key`→③、`impact_text`→`render_text`（`render_json` は C-1 の JSON の行。`note` が空なら `null`）。`resolve` は「many」の全件を返し、メッセージは先頭 10 件と `, …N more` にする。

### D-1: `kbload.py`

```python
import json, os
from cards import *

def load(directory):
    """Reads a KB directory (the files of G27's appendix D) into (Kb, manifest dict, diag rows)."""
    def rows(path):
        full = os.path.join(directory, path)
        return [json.loads(line) for line in open(full, encoding="utf-8")] if os.path.exists(full) else None
    manifest = json.load(open(os.path.join(directory, "manifest.json"), encoding="utf-8"))
    symbols, graph, tests = rows("symbols.jsonl"), rows("graph.jsonl"), rows("tests.jsonl")
    facts = None
    fdir = os.path.join(directory, "facts")
    if os.path.isdir(fdir):
        facts = {}
        for name in sorted(os.listdir(fdir)):
            for row in rows("facts/" + name): facts[row["id"]] = row
    diag = rows("diag.jsonl") or []
    kb = Kb(manifest["project"], manifest.get("version"), manifest["edition"], symbols, graph, facts, warnings=manifest["counts"].get("warnings", 0))
    kb.tests = tests
    return kb, manifest, diag

def diag_tuples(diag):
    return [(d["code"], d["message"], d["path"], d["span"]["line"]) for d in diag if d.get("severity") == "warning"]
```

### D-2: `query_ref.py`

```python
# G28 reference: ID resolution and the text of the query commands (the JSON rows are the examples of section B-3). Python 3.9+; cards.py is G27's appendix F-1.
import re
from cards import *

b = bytes_key

# ---------- ID resolution ----------
def osa(a, c):
    d = [[0] * (len(c) + 1) for _ in range(len(a) + 1)]
    for i in range(len(a) + 1): d[i][0] = i
    for j in range(len(c) + 1): d[0][j] = j
    for i in range(1, len(a) + 1):
        for j in range(1, len(c) + 1):
            cost = 0 if a[i - 1] == c[j - 1] else 1
            d[i][j] = min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost)
            if i > 1 and j > 1 and a[i - 1] == c[j - 2] and a[i - 2] == c[j - 1]:
                d[i][j] = min(d[i][j], d[i - 2][j - 2] + 1)
    return d[len(a)][len(c)]

def universe(kb):
    return sorted({r["id"] for r in kb.symbols} | {e["to"] for e in (kb.graph or [])}, key=b)

def resolve(kb, text):
    ids = universe(kb)
    if text in ids: return ("one", text)
    low = text.lower()
    stages = [
        [i for i in ids if i.endswith("::" + text) or i.endswith("." + text)],
        [i for i in ids if i.lower() == low],
        [i for i in ids if i.lower().endswith("::" + low) or i.lower().endswith("." + low)],
        [i for i in ids if i.startswith(text)],
        [i for i in ids if i.lower().startswith(low)],
    ]
    for stage in stages:
        stage = sorted(set(stage), key=b)
        if len(stage) == 1: return ("one", stage[0])
        if len(stage) > 1: return ("many", stage)                       # the message lists 10 and counts the rest as "…N more"
    last = lambda text: re.split(r"\.|::", text)[-1].lower()
    limit = 1 if len(last(text)) <= 4 else 2
    cand = sorted(((osa(last(text), last(i)), i) for i in ids), key=lambda t: (t[0], b(t[1])))
    return ("none", [i for d, i in cand if d <= limit][:5])

def site(e): return f"{e['span']['path']}:{e['span']['line']}:{e['span']['column']}"

def listing(header, items, footer, limit, gap="\n"):
    c = Card(limit, gap)
    c.block(None, [(h, 0) for h in header])
    c.block(None, [(t, 1) for t in items])
    c.block(None, [(f, 0) for f in footer])
    return c.render()

# ---------- callers / callees ----------
def walk(kb, start, depth, forward):
    seen, rows, frontier = {start}, [], [start]
    for d in range(1, depth + 1):
        nxt = []
        for node in sorted(frontier, key=b):
            edges = [e for e in kb.graph if e["rel"] == "calls" and e["from" if forward else "to"] == node]
            groups = {}
            for e in edges: groups.setdefault(e["to" if forward else "from"], []).append(e)
            for other in sorted(groups, key=b):
                if other in seen: continue
                seen.add(other); nxt.append(other)
                es = sorted(groups[other], key=lambda e: (e["span"]["path"], e["span"]["start"]))
                rows.append({"depth": d, "id": other, "edge": es[0], "sites": len(es), "via": None if d == 1 else node})
        frontier = nxt
    rows.sort(key=lambda r: (r["depth"], b(r["id"])))
    return rows

def args_text(e): return "args " + ",".join(e.get("args", [])) if e.get("args") else ""

def calls_text(kb, sid, depth, forward, limit=2000):
    rows = walk(kb, sid, depth, forward)
    items = []
    for r in rows:
        t = f"{r['depth']} {r['id']}  {site(r['edge'])}"
        if r["depth"] == 1 and args_text(r["edge"]): t += "  " + args_text(r["edge"])
        if r["sites"] > 1: t += f"  +{r['sites'] - 1} sites"
        if r["via"]: t += f"  via {r['via']}"
        items.append(t)
    word = "callees" if forward else "callers"
    head = [f"{word} of {sid} (depth {depth})"]
    foot = [f"summary: {plural(len(rows), word[:-1], word)}, depth {depth}"]
    return listing(head, items, foot, limit)

def uses_text(kb, tid, limit=2000):
    es = sorted([e for e in kb.graph if e["rel"] in ("uses_type", "implements", "instance_of") and e["to"] == tid],
                key=lambda e: (b(e["rel"]), b(e["from"]), e["span"]["start"]))
    items = [f"{e['rel']} {e['from']}  {site(e)}" for e in es]
    return listing([f"uses of {tid}"], items, [f"summary: {plural(len(es), 'use', 'uses')}"], limit)

def tests_for_text(kb, sid, limit=2000):
    es = sorted([e for e in kb.graph if e["rel"] == "tested_by" and e["from"] == sid], key=lambda e: b(e["to"]))
    items = [f"{e['to']}  {site(e)}" for e in es]
    return listing([f"tests for {sid}"], items, [f"summary: {plural(len(es), 'test', 'tests')}"], limit)

def search_text(kb, text, limit=2000):
    low = text.lower(); rows = []
    for r in kb.symbols:
        name = r["name"].lower(); fl = first_line(r.get("doc")).lower()
        if name == low: rank = 1
        elif name.startswith(low): rank = 2
        elif low in name: rank = 3
        elif low in r["id"].lower(): rank = 4
        elif low in norm(r["signature"]).lower(): rank = 5
        elif low and low in fl: rank = 6
        else: continue
        rows.append((rank, b(r["id"]), r))
    rows.sort(key=lambda t: (t[0], t[1]))
    items = [f"{n} {r['id']}  {r['kind']}  {clip(norm(r['signature']), 100)}" for n, (rank, _, r) in enumerate(rows, 1)]
    return listing([f"search '{text}' (project)"], items, [f"summary: {plural(len(rows), 'match', 'matches')}"], limit)

# ---------- show ----------
def show_text(kb, sid, limit=3000):
    r = kb.by_id.get(sid)
    if r is None:                                     # an id that only the graph names, such as IO.write_line
        lines = [(sid, 0), ("symbol: not in this KB", 0)]
    else:
        vis = "public" if r["public"] else "private"
        flags = [vis] + (["export"] if r["export"] else []) + (["pure"] if r.get("pure") else [])
        lines = [(r["id"], 0), (f"{r['kind']} | {' '.join(flags)} | {r['span']['path']}:{r['span']['line']}:{r['span']['column']}", 0),
                 ("signature: " + clip(norm(r["signature"]), 160), 0)]
        if first_line(r.get("doc")): lines.append(("doc: " + first_line(r["doc"]), 1))
        if r["kind"] in ("def", "extern", "method", "const") and r.get("params"):
            lines.append(("params:", 1))
            for p in r["params"]:
                lines.append((f"  {p['name'] or '_'}  {p['conv']}  {p['type']}", 1))
            lines.append((f"result: {r['result']}", 1))
        for kind, label in (("field", "fields"), ("case", "cases"), ("method", "methods")):
            if (r["kind"], kind) in (("record", "field"), ("union", "case"), ("class", "method")):
                kids = sorted(kb.children(r["id"], kind), key=lambda k: (k["span"]["start"], k["id"]))
                if kids:
                    lines.append((f"{label}:", 1))
                    for k in kids: lines.append((f"  {norm(k['signature'])}", 1))
        f = kb.facts.get(sid) if kb.facts is not None else None
        if f is not None:
            lines.append(("effects: " + (", ".join(f.get("effects", [])) or "none"), 1))
            counted = {k: sum(1 for s in f.get(k + "_sites", []) if not s.get("proven")) for k in ("trap", "alloc", "copy")}
            proven = sum(1 for s in f.get("trap_sites", []) if s.get("proven"))
            t = f"sites: trap {counted['trap']}, alloc {counted['alloc']}, copy {counted['copy']}"
            if proven: t += f" ({proven} proven trap {'site' if proven == 1 else 'sites'} not counted)"
            lines.append((t, 1))
            if f.get("recursion", "none") != "none": lines.append(("recursion: " + f["recursion"], 1))
    if kb.graph is not None:
        def group(label, es, fmt):
            if not es: return
            lines.append((f"{label} ({len(es)}):", 2))
            for e in es: lines.append((fmt(e), 3))
        edges = lambda rel, end: [e for e in kb.graph if e["rel"] in rel and e[end] == sid]
        out = sorted(edges(("calls",), "from"), key=lambda e: (b(e["to"]), e["span"]["start"]))
        inn = sorted(edges(("calls",), "to"), key=lambda e: (b(e["from"]), e["span"]["start"]))
        tst = sorted(edges(("tested_by",), "from"), key=lambda e: b(e["to"]))
        usd = sorted(edges(("uses_type", "implements", "instance_of"), "to"), key=lambda e: (b(e["from"]), b(e["rel"]), e["span"]["start"]))
        if r is not None:
            group("callees", out, lambda e: f"  {e['to']}  {site(e)}" + (f"  {args_text(e)}" if args_text(e) else ""))
        group("callers", inn, lambda e: f"  {e['from']}  {site(e)}" + (f"  {args_text(e)}" if args_text(e) else ""))
        if r is not None:
            group("tests", tst, lambda e: f"  {e['to']}  {site(e)}")
            group("used by", usd, lambda e: f"  {e['from']}  {e['rel']}  {site(e)}")
    c = Card(limit, "\n"); c.block(None, lines)
    return c.render()
```

### D-3: `impact_ref.py`

```python
# G28 reference: the rules of `kb impact`. Python 3.9+; cards.py is G27's appendix F-1.
import re
from cards import *
b = bytes_key

MEMBERS = ("field", "case", "method")
MAX_DEPTH = 8

def canon(sig):
    names = {}
    def sub(m):
        return names.setdefault(m.group(0), f"'t{len(names)}")
    return re.sub(r"'[A-Za-z_][A-Za-z0-9_]*", sub, norm(sig))

def parent_id(r): return r["id"].rsplit(".", 1)[0]
def conv_of(r): return [p["conv"] for p in r.get("params", [])]

def shown(r): return norm(r["signature"])

class Change:
    def __init__(self, cls, action, kind, name, old=None, new=None, note=""):
        self.cls, self.action, self.kind, self.name, self.old, self.new, self.note = cls, action, kind, name, old, new, note
    def message(self):
        if self.kind == "effect":
            return f"{self.action} effect {self.old or self.new} on {self.name}" + (f" ({self.note})" if self.note else "")
        if self.kind == "warning":
            return f"{self.action} warning {self.name} at {self.old or self.new}"
        text = f"{self.action} {self.kind}" + (f" {self.name}" if self.name else "")
        if self.action == "deprecated" and self.new: text += f": {self.new}"
        elif self.action == "undeprecated" and self.old: text += f": {self.old}"
        elif self.action in ("deprecated", "undeprecated"): pass
        elif self.old is not None and self.new is not None: text += f": `{self.old}` => `{self.new}`"
        elif self.old is not None or self.new is not None: text += f": `{self.old if self.old is not None else self.new}`"
        if self.note: text += f" ({self.note})"
        return text

def symbol_changes(base, head):
    out = []
    old = {r["id"]: r for r in base.symbols if r["kind"] not in ("module",)}
    new = {r["id"]: r for r in head.symbols if r["kind"] not in ("module",)}
    for sid, r in old.items():
        if sid in new: continue
        if r["kind"] in MEMBERS and parent_id(r) not in new: continue      # reported with the removed parent
        note = {"field": "uses of the field stop compiling", "case": "uses of the case stop compiling",
                "method": "calls and instances of the method stop compiling"}.get(r["kind"], "")
        out.append(Change("breaking" if r["public"] else "non-breaking", "removed", r["kind"], sid, shown(r), None, note))
    for sid, r in new.items():
        if sid in old: continue
        if r["kind"] in MEMBERS and parent_id(r) not in old: continue      # reported with the added parent
        in_class = r["kind"] == "method" and new[parent_id(r)]["kind"] == "class"
        if r["kind"] == "field" and r["public"]: out.append(Change("breaking", "added", "field", sid, None, shown(r), "constructions of the record without it stop compiling"))
        elif r["kind"] == "case" and r["public"]: out.append(Change("breaking", "added", "case", sid, None, shown(r), "exhaustive matches over the union stop compiling"))
        elif in_class and r["public"]: out.append(Change("breaking", "added", "method", sid, None, shown(r), "existing instances do not define the method"))
        else: out.append(Change("non-breaking", "added", r["kind"], sid, None, shown(r)))
    for sid, o in old.items():
        n = new.get(sid)
        if n is None: continue
        api = o["public"]
        if canon(o["signature"]) != canon(n["signature"]):
            out.append(Change("breaking" if api else "non-breaking", "changed", n["kind"], sid, shown(o), shown(n)))
        elif conv_of(o) != conv_of(n):
            out.append(Change("breaking" if api else "non-breaking", "changed", "conv", sid, ",".join(conv_of(o)), ",".join(conv_of(n)), "ownership passing of an argument changed"))
        if o["public"] != n["public"]:
            out.append(Change("non-breaking" if n["public"] else "breaking", "changed", "visibility", sid, "public" if o["public"] else "private", "public" if n["public"] else "private"))
        for key in ("export", "pure"):
            if bool(o.get(key)) != bool(n.get(key)):
                out.append(Change("non-breaking" if n.get(key) else "breaking", "changed", key, sid, str(bool(o.get(key))).lower(), str(bool(n.get(key))).lower()))
        if o.get("deprecated") is None and n.get("deprecated") is not None: out.append(Change("non-breaking", "deprecated", n["kind"], sid, None, n["deprecated"]))
        if o.get("deprecated") is not None and n.get("deprecated") is None: out.append(Change("non-breaking", "undeprecated", n["kind"], sid, o["deprecated"], None))
        if (o.get("doc") or "") != (n.get("doc") or "") and (o.get("deprecated") is None) == (n.get("deprecated") is None):
            out.append(Change("non-breaking", "documented", n["kind"], sid))
    # implemented classes of a public type (a derive is an implements edge too)
    def implemented(kb): return {(e["from"], e["to"]) for e in (kb.graph or []) if e["rel"] == "implements"}
    if base.graph is not None and head.graph is not None:
        for frm, to in sorted(implemented(base) - implemented(head), key=lambda t: (b(t[0]), b(t[1]))):
            if frm in head.by_id and head.by_id[frm]["public"]: out.append(Change("breaking", "removed", "implements", f"{frm}.{to}"))
        for frm, to in sorted(implemented(head) - implemented(base), key=lambda t: (b(t[0]), b(t[1]))):
            out.append(Change("non-breaking", "added", "implements", f"{frm}.{to}"))
    # order of fields and cases
    for sid, o in old.items():
        n = new.get(sid)
        if n is None or o["kind"] not in ("record", "union"): continue
        kind = "field" if o["kind"] == "record" else "case"
        names = lambda kb: [r["name"] for r in sorted(kb.children(sid, kind), key=lambda r: (r["span"]["start"], r["id"]))]
        was, now = names(base), names(head)
        common_was = [x for x in was if x in now]; common_now = [x for x in now if x in was]
        if common_was != common_now:
            out.append(Change("breaking" if o["public"] else "non-breaking", "changed", "order", sid, ", ".join(common_was), ", ".join(common_now), "the order of fields and cases is part of the layout and of derived comparisons"))
    return out

def counted(f, key): return sum(1 for s in f.get(key, []) if not s.get("proven"))

def reach(head, sid):
    reverse = {}
    for e in head.graph or []:
        if e["rel"] == "calls": reverse.setdefault(e["to"], set()).add(e["from"])
    seen, frontier = {sid}, {sid}
    for _ in range(MAX_DEPTH):
        nxt = {c for n in frontier for c in reverse.get(n, ()) if c not in seen}
        seen |= nxt; frontier = nxt
        if not frontier: break
    seen.discard(sid)
    tests = sum(1 for i in seen if i.startswith("test:"))
    return len(seen) - tests, tests

def fact_changes(base, head, base_manifest, head_manifest):
    out = []
    if base.facts is None or head.facts is None or base_manifest["compiler"] != head_manifest["compiler"]:
        return [Change("info", "skipped", "facts", "", None, None, "the two KBs were built by different compilers or one has no facts")]
    for sid, nf in sorted(head.facts.items(), key=lambda t: b(t[0])):
        of = base.facts.get(sid)
        row = head.by_id.get(sid)
        if of is None or row is None or row["kind"] not in ("def", "method", "extern"): continue
        public = row["public"]
        for e in sorted(set(nf.get("effects", [])) - set(of.get("effects", []))):
            fn, tn = reach(head, sid) if head.graph is not None else (0, 0)
            note = f"reached by {plural(fn, 'function', 'functions')} and {plural(tn, 'test', 'tests')}" if fn + tn else ""
            out.append(Change("breaking" if public else "info", "added", "effect", sid, None, e, note))
        for e in sorted(set(of.get("effects", [])) - set(nf.get("effects", []))):
            out.append(Change("non-breaking" if public else "info", "removed", "effect", sid, e, None))
        for key in ("trap_sites", "alloc_sites", "copy_sites"):
            a, c = counted(of, key), counted(nf, key)
            if a != c: out.append(Change("info", "increased" if c > a else "decreased", key, sid, str(a), str(c)))
        if of.get("recursion", "none") != nf.get("recursion", "none"):
            out.append(Change("info", "changed", "recursion", sid, of.get("recursion", "none"), nf.get("recursion", "none")))
    return out

def diag_changes(base_diag, head_diag):
    out = []
    bset, hset = list(base_diag), list(head_diag)
    key = lambda d: (d[0], d[1], d[2])
    remaining = [key(d) for d in bset]
    for d in hset:
        if key(d) in remaining: remaining.remove(key(d))
        else: out.append(Change("info", "new", "warning", d[0], None, f"{d[2]}:{d[3]}: {d[1]}"))
    hk = [key(d) for d in hset]
    for d in bset:
        if key(d) in hk: hk.remove(key(d))
        else: out.append(Change("info", "fixed", "warning", d[0], f"{d[2]}:{d[3]}: {d[1]}", None))
    return out

def changed_modules(base_manifest, head_manifest):
    old = {m["name"]: m["source"] for m in base_manifest["modules"]}
    new = {m["name"]: m["source"] for m in head_manifest["modules"]}
    return {name for name in old.keys() | new.keys() if old.get(name) != new.get(name)}

def std_instance_in(kb, modules):
    """An instance of a class that the project does not declare (operator overloads call it without a call edge)."""
    classes = {e["from"]: e["to"] for e in (kb.graph or []) if e["rel"] == "instance_of"}
    return any(r["kind"] == "instance" and r["module"] in modules and classes.get(r["id"]) not in kb.by_id for r in kb.symbols)

def affected_tests(base, head, base_manifest, head_manifest):
    """Returns (test ids, None), or (None, reason) when every test must run."""
    if head.graph is None: return None, "the KB has no graph"
    if (base_manifest["compiler"], base_manifest["schema"]) != (head_manifest["compiler"], head_manifest["schema"]):
        return None, "the compiler changed"
    if base_manifest["source_hash"] == head_manifest["source_hash"]: return [], None
    modules = changed_modules(base_manifest, head_manifest)
    if not modules: return None, "the manifest or a dependency changed"
    if std_instance_in(base, modules) or std_instance_in(head, modules): return None, "an instance of a std class changed"
    reverse = {}
    for e in head.graph:
        if e["rel"] == "calls": reverse.setdefault(e["to"], set()).add(e["from"])
    # a call of a class method reaches the same method of every instance of the class
    instances = {}
    for e in head.graph:
        if e["rel"] == "instance_of": instances.setdefault(e["to"], []).append(e["from"])
    for e in head.graph:
        if e["rel"] != "calls": continue
        for cls, members in instances.items():
            method = e["to"][len(cls) + 1:] if e["to"].startswith(cls + ".") else ""
            if method and "." not in method:
                for instance in members: reverse.setdefault(f"{instance}.{method}", set()).add(e["from"])
    seen = {r["id"] for r in head.symbols if r["module"] in modules and r["kind"] != "module"}
    frontier = set(seen)
    def expand(front): return {c for n in front for c in reverse.get(n, ()) if c not in seen}
    for _ in range(MAX_DEPTH):
        frontier = expand(frontier); seen |= frontier
        if not frontier: break
    else:
        if expand(frontier): return None, f"callers deeper than {MAX_DEPTH} calls"
    tests = {i for i in seen if i.startswith("test:")}
    tests |= {r["id"] for r in head.symbols if r["kind"] == "test" and r["module"] in modules}
    removed = {r["id"] for r in base.symbols if r["id"] not in head.by_id}
    if base.graph is not None:
        tests |= {e["from"] for e in base.graph if e["rel"] == "calls" and e["to"] in removed and e["from"].startswith("test:")}
    return sorted((t for t in tests if t in head.by_id), key=b), None

ORDER = {"breaking": 0, "non-breaking": 1, "info": 2}
ACTIONS = ["removed", "added", "changed", "deprecated", "undeprecated", "documented", "increased", "decreased", "new", "fixed", "skipped"]
MAX_LISTED_CHANGES = 1000

def sort_key(c):
    return (ORDER[c.cls], b(c.name), b(c.kind), ACTIONS.index(c.action), c.old is not None, b(c.old or ""), c.new is not None, b(c.new or ""))

def compare(base, head, base_manifest, head_manifest, base_diag, head_diag):
    """Returns (changes in output order, test ids or None, reason)."""
    changes = symbol_changes(base, head) + fact_changes(base, head, base_manifest, head_manifest) + diag_changes(base_diag, head_diag)
    changes.sort(key=sort_key)
    tests, reason = affected_tests(base, head, base_manifest, head_manifest)
    return changes, tests, reason

def counts(changes): return [sum(1 for c in changes if c.cls == cls) for cls in ("breaking", "non-breaking", "info")]

def impact_text(base, head, base_manifest, head_manifest, base_diag, head_diag, base_label, head_label):
    changes, tests, reason = compare(base, head, base_manifest, head_manifest, base_diag, head_diag)
    lines = [f"impact: {base_label} -> {head_label}"]
    for c in changes[:MAX_LISTED_CHANGES]: lines.append(f"{c.cls}: {c.message()}")
    if len(changes) > MAX_LISTED_CHANGES: lines.append(f"note: {len(changes) - MAX_LISTED_CHANGES} more changes not shown")
    total = sum(1 for r in head.symbols if r["kind"] == "test")
    if tests is None: lines.append(f"tests: all {total} ({reason})")
    else:
        lines.append(f"tests: {len(tests)} of {total} affected")
        lines += [f"  {t}" for t in tests]
    n_break, n_non, n_info = counts(changes)
    lines.append(f"summary: {n_break} breaking, {n_non} non-breaking, {n_info} info")
    return "\n".join(lines) + "\n"
```

## 付録 E: Rust の型の全フィールド

名前と型は変えない。`serde` の derive はない（行は G27 の `store.rs` の型で持つ）。診断は `Diagnostic::new("E2000", message, Span::default())` を `print_diagnostic(&d, Path::new("<command line>"), "", json)` で出す（`api_diff_usage` と同じ）。

```rust
// src/kb/store.rs（G27 の付録 H-1 への追加）
pub struct Meta { /* G27 の全フィールド */ pub schema: String, pub compiler: String }    // manifest の同名キー。open が入れる
impl KbData {                                       // 索引（BTreeMap・Vec の位置）は open と from_parts が 1 回作る
    pub fn symbol(&self, id: &str) -> Option<&Symbol>;                      // G27
    pub fn fact(&self, id: &str) -> Option<&Fact>;
    pub fn edges_from(&self, rel: &str, id: &str) -> Vec<&Edge>;           // graph の行の順。graph が None なら空
    pub fn edges_to(&self, rel: &str, id: &str) -> Vec<&Edge>;
    pub fn children(&self, parent_id: &str, kind: &str) -> Vec<&Symbol>;  // (span.start, id) の順
    pub fn all_ids(&self) -> &[String];                                     // symbols の id ∪ graph の to。byte 順・重複なし
    pub fn test_index(&self, id: &str) -> Option<usize>;                    // tests.jsonl の index。tests が None なら None
    pub fn module_key(&self, id: &str) -> Option<&str>;                     // meta.modules の name → key
}
impl KbView { /* 同名のメソッド。project を先に、std を後に引く。all_ids は 2 つの和（byte 順） */ }

// src/kb/suggest.rs
pub fn osa_distance(a: &str, b: &str) -> usize;                       // chars() 単位。隣接の入れ替えを 1 とする
pub fn last_name(id: &str) -> &str;                                   // 最後の `.` か `::` の後ろ
pub fn suggestions(ids: &[String], text: &str) -> Vec<String>;        // §2 の未知の規則。5 件まで

// src/kb/query.rs
pub enum Resolution { One(String), Many(Vec<String>), Unknown(Vec<String>) }   // Many は全件（byte 順）、Unknown は候補（5 件まで）
pub struct Options { pub budget: Option<usize>, pub depth: usize }              // depth の既定は 1
pub enum BriefTarget { Project, Module(String), Id(String) }
pub struct Output { pub text: String, pub rows: Vec<String> }                    // rows は JSON Lines の各行（改行なし）
pub enum QueryError {
    Ambiguous { text: String, candidates: Vec<String>, more: usize },            // candidates は 10 件まで、more は残りの数
    Unknown { text: String, suggestions: Vec<String> },
    Empty(&'static str),                                                          // "id must not be empty" か "search text must not be empty"
}
impl QueryError {
    pub fn diagnostic(&self) -> Diagnostic;                // E2000。message は §2・B-1 の文言。Ambiguous の末尾は ", …{more} more"
    pub fn exit_code(&self) -> u8;                         // Ambiguous・Unknown は 1、Empty は 2
}
pub fn resolve(view: &KbView, text: &str, modules: bool) -> Resolution;           // modules が真なら module の ID と key だけが対象
pub fn brief(view: &KbView, target: &BriefTarget, options: &Options) -> Result<Output, QueryError>;
pub fn show(view: &KbView, id: &str, options: &Options) -> Result<Output, QueryError>;
pub fn callers(view: &KbView, id: &str, options: &Options) -> Result<Output, QueryError>;   // callees・uses・tests_for も同じ形
pub fn search(view: &KbView, text: &str, std: bool, options: &Options) -> Result<Output, QueryError>;

// src/kb/impact.rs
pub const MAX_LISTED_CHANGES: usize = crate::api_diff::MAX_LISTED_CHANGES;            // 1,000
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity { Breaking, NonBreaking, Info }                                       // as_str: breaking・non-breaking・info
#[derive(Clone, PartialEq, Eq)]
pub struct Change { pub severity: Severity, pub action: &'static str, pub kind: String, pub name: String,
                    pub old: Option<String>, pub new: Option<String>, pub note: String }
impl Change { pub fn message(&self) -> String; pub fn json(&self) -> String }           // D-3 の `Change.message`、C-1 の `change` の行
pub enum Tests { Some(Vec<String>), All(String) }                                       // All の中身は理由。Some は byte 順
pub struct Impact { pub base: String, pub head: String, pub changes: Vec<Change>, pub tests: Tests, pub total_tests: usize }
pub fn diff(base: &KbData, head: &KbData, base_label: &str, head_label: &str) -> Impact;
impl Impact {
    pub fn render_text(&self) -> String;     // D-3 の impact_text
    pub fn render_json(&self) -> String;     // C-1 の JSON の行（キーの順は固定）
    pub fn counts(&self) -> (usize, usize, usize);                                       // 全件の (breaking, non-breaking, info)
}

// src/kb/git.rs
pub struct Checkout { temporary: TemporaryDirectory, top: PathBuf, checkout: PathBuf, relative: PathBuf, sha: String }
impl Checkout {
    pub fn create(root: &Path, rev: &str) -> Result<Self, Diagnostic>;   // §5 の git の扱い 1〜5。E2000・E2001
    pub fn project_root(&self) -> PathBuf;                                 // {checkout}/{relative}
    pub fn sha12(&self) -> &str;
}
impl Drop for Checkout { fn drop(&mut self); }                              // worktree remove --force（失敗は無視）。temporary はその後に落ちる

// src/kb/command.rs（G27 のファイルへの追加）
pub const COMMANDS: &[&str] = &["build", "verify", "clean", "export", "brief", "show", "callers", "callees", "uses", "tests-for", "search"];  // Phase 2 で "find"、Phase 3 で "impact" を末尾に足す
pub struct Common { pub path: PathBuf, pub kb_dir: Option<PathBuf>, pub no_refresh: bool, pub json: bool }
pub enum Query { Brief(BriefTarget), Show(String), Callers(String), Callees(String), Uses(String), TestsFor(String), Search { text: String, std: bool } }
pub struct Parsed { pub common: Common, pub options: query::Options, pub query: Query }
pub fn parse_query(command: &str, arguments: &[OsString]) -> Result<Parsed, Diagnostic>;       // E2000（終了コード 2）
pub fn run_query(parsed: &Parsed) -> ExitCode;                       // load_for_tests → open → query。QueryError・E2009 は print_diagnostic
pub enum ImpactSource { Git(String), From { from: PathBuf, to: Option<PathBuf> } }
pub struct ImpactArguments { pub source: ImpactSource, pub common: Common }
pub fn parse_impact(arguments: &[OsString]) -> Result<ImpactArguments, Diagnostic>;
pub fn run_impact(arguments: &ImpactArguments) -> ExitCode;          // 常に 0（エラーは 1・2）
pub struct Affected { pub label: String, pub reason: Option<String>, pub indices: Vec<usize>, pub total: usize }   // reason があれば全テスト
pub fn select_affected(project: &Project, value: &str, kb_dir: Option<&Path>, json: bool) -> Result<Affected, Diagnostic>;
impl Affected { pub fn render_line(&self) -> String; pub fn render_json(&self) -> String }   // C-3 の `affected:` の行と JSON
```

`src/main.rs`: `kb` の分岐（G27）は `raw[1]` が `find` なら `find_command(&raw[2..])`、`COMMANDS` の中なら `parse_query`・`run_query`（`impact` は `parse_impact`・`run_impact`）、ほかは §1 の `unknown kb command`。Phase 4 は §6 の「実装の場所」（`Arguments.affected`、`run_test_action` での `select_affected`）。

## 付録 F: 各テストの入力・期待と測定の手順

### F-1: symbols の決定表（`each_row_of_the_symbols_table`。D-3 の `symbol_changes` が出した行）

行の作り方: `module` は ID の最初の `.` の前（`P`）、`public` は真、`export`・`pure` は偽、`doc`・`deprecated` は `null`、`params` は空（`conv` を指定する行は引数 `a0` … の型 `i64`）、`span.start` は 0（field・case は書いた順に 10・20）。記号: `R`＝`P.R` `record R`、`R.a`＝`P.R.a` `a: i64`、`U`＝`P.U` `union U`、`U.C`＝`P.U.C` `C(i64)`、`K`＝`P.K` `class K<'a>`、`K.m`＝`P.K.m` `def m :: ref 'a -> i64`、`f`＝`P.f` `def f :: i64 -> i64`。期待は `impact` の 1 行（`—` は行が出ない）。

| # | base | head | 期待 |
| --- | --- | --- | --- |
| 1 | R、R.a、R.b | なし | `breaking: removed record P.R: \`record R\`` だけ（field の行は出ない） |
| 2a | R、R.a、R.b | R、R.a | `breaking: removed field P.R.b: \`b: i64\` (uses of the field stop compiling)` |
| 2b | U、U.C、U.D（`D(i64)`） | U、U.C | `breaking: removed case P.U.D: \`D(i64)\` (uses of the case stop compiling)` |
| 2c | K、K.m | K | `breaking: removed method P.K.m: \`def m :: ref 'a -> i64\` (calls and instances of the method stop compiling)` |
| 2d | f | なし | `breaking: removed def P.f: \`def f :: i64 -> i64\`` |
| 3a | 非公開の `P.g`（`def g :: i64 -> i64`） | なし | `non-breaking: removed def P.g: \`def g :: i64 -> i64\`` |
| 3b | 非公開の test `test:P.t`（`test "t"`） | なし | `non-breaking: removed test test:P.t: \`test "t"\`` |
| 4 | なし | R、R.a、R.b | `non-breaking: added record P.R: \`record R\`` だけ |
| 5a | R、R.a | R、R.a、R.c | `breaking: added field P.R.c: \`c: i64\` (constructions of the record without it stop compiling)` |
| 5b | U、U.C | U、U.C、U.D | `breaking: added case P.U.D: \`D(i64)\` (exhaustive matches over the union stop compiling)` |
| 5c | K | K、K.m | `breaking: added method P.K.m: \`def m :: ref 'a -> i64\` (existing instances do not define the method)` |
| 6a | なし | f | `non-breaking: added def P.f: \`def f :: i64 -> i64\`` |
| 6b | 非公開の `record Q` | 同じ＋非公開の field `P.Q.z`（`z: i64`） | `non-breaking: added field P.Q.z: \`z: i64\`` |
| 6c | `instance:K<P.R>`（`instance K<P.R>`） | 同じ＋method `instance:K<P.R>.m`（`def m :: ref P.R -> i64`） | `non-breaking: added method instance:K<P.R>.m: \`def m :: ref P.R -> i64\`` |
| 7a | f | `def f :: i64 -> i64 -> i64` | `breaking: changed def P.f: \`def f :: i64 -> i64\` => \`def f :: i64 -> i64 -> i64\`` |
| 7b | 非公開の f | 同じ変更（非公開） | `non-breaking: changed def P.f: …`（7a と同じ文字列） |
| 7c | `def f :: 'a -> 'b -> 'a` | `def f :: 'x -> 'y -> 'x` | — |
| 8 | f（引数の `conv` が `ref`） | f（`own`） | `breaking: changed conv P.f: \`ref\` => \`own\` (ownership passing of an argument changed)` |
| 9a | 非公開の f | 公開の f | `non-breaking: changed visibility P.f: \`private\` => \`public\`` |
| 9b | 公開の f | 非公開の f | `breaking: changed visibility P.f: \`public\` => \`private\`` |
| 10a | `export` の f | f | `breaking: changed export P.f: \`true\` => \`false\`` |
| 10b | f | `export` の f | `non-breaking: changed export P.f: \`false\` => \`true\`` |
| 11 | `pure` の f | f | `breaking: changed pure P.f: \`true\` => \`false\`` |
| 12a | f | `deprecated` が `"use g"` の f | `non-breaking: deprecated def P.f: use g` |
| 12b | f | `deprecated` が `""` の f | `non-breaking: deprecated def P.f` |
| 12c | `deprecated` が `"use g"` の f | f | `non-breaking: undeprecated def P.f: use g` |
| 13 | `doc` が `Old.` の f | `doc` が `New.` の f | `non-breaking: documented def P.f` |
| 14a | R、R.a（10）、R.b（20） | R、R.a（20）、R.b（10） | `breaking: changed order P.R: \`a, b\` => \`b, a\` (the order of fields and cases is part of the layout and of derived comparisons)` |
| 14b | 非公開の `union V`、`V.C`（10）、`V.D`（20） | `V.C`（20）、`V.D`（10） | `non-breaking: changed order P.V: \`C, D\` => \`D, C\` (the order of fields and cases is part of the layout and of derived comparisons)` |
| 15 | R、graph に `implements` `P.R`→`Eq` | R のみ | `breaking: removed implements P.R.Eq` |
| 16 | R | R、graph に `implements` `P.R`→`Eq` | `non-breaking: added implements P.R.Eq` |

### F-2: facts（`effects_added_to_a_public_function_are_breaking` ほか）

`P.f`・`P.g`・`P.h` は公開の `def`、`P.k` は非公開の `def`、`test:P.t` は test。facts の行は `effects`・`trap_sites`（`{"kind","proven"}`）・`alloc_sites`・`copy_sites`・`recursion`。両方の KB の `manifest.compiler` は同じ。graph の既定は `P.g`→`P.f`、`P.h`→`P.g`、`test:P.t`→`P.h`（すべて `calls`）。

| # | 変更（base→head） | 期待 |
| --- | --- | --- |
| X1 | `P.f` の `effects` が `[]`→`["io"]` | `breaking: added effect io on P.f (reached by 2 functions and 1 test)` |
| X2 | 同じで graph が空 | `breaking: added effect io on P.f`（note なし） |
| X3 | `P.k` に `io`。graph は `P.g`→`P.k` だけ | `info: added effect io on P.k (reached by 1 function and 0 tests)` |
| X4a | `P.f` の `io` を除く | `non-breaking: removed effect io on P.f` |
| X4b | `P.k` の `io` を除く | `info: removed effect io on P.k` |
| X5 | `P.f` の `trap_sites` が `[proven]`→`[proven, unproven]` | `info: increased trap_sites P.f: \`0\` => \`1\``（`proven` は数えない） |
| X6 | `P.f` の `alloc_sites` が 2→1、`copy_sites` が 0→3 | `info: decreased alloc_sites P.f: \`2\` => \`1\`` と `info: increased copy_sites P.f: \`0\` => \`3\`` |
| X7 | `P.f` の `recursion` が `none`→`self` | `info: changed recursion P.f: \`none\` => \`self\`` |
| X8 | `test:P.t` の `effects` が `[]`→`["io"]` | —（test は比べない） |
| X9a | head に facts がない | `info: skipped facts (the two KBs were built by different compilers or one has no facts)` |
| X9b | head の `manifest.compiler` が `tsuzuri 0.2.0` | 同上 |

### F-3: 影響を受けるテスト（`tests_table_rows`・`dispatch_reaches_instance_methods`）

土台: モジュール `P`（`P.f`、`test:P.t`）と `Q`（`test:Q.u`。`P.f` を呼ぶ）。`manifest.modules` の `source` は `P`＝`1`、`Q`＝`2`（head で変えるものだけ書く）。

| # | head の状態 | 期待（`tests:` の行と以降） |
| --- | --- | --- |
| T1 | `graph.jsonl` がない | `tests: all 2 (the KB has no graph)` |
| T2 | `compiler` が違う | `tests: all 2 (the compiler changed)` |
| T3 | `schema` が `tz-kb/2` | 同上 |
| T4 | `source_hash` が同じ | `tests: 0 of 2 affected` |
| T5 | `source_hash` だけ違い、`modules` は同じ | `tests: all 2 (the manifest or a dependency changed)` |
| T6 | `P` の `source` が違う | `tests: 2 of 2 affected`、`  test:P.t`、`  test:Q.u` |
| T6b | `Q` の `source` が違う | `tests: 1 of 2 affected`、`  test:Q.u` |
| T7 | class `P.K`（method `m`）、instance `instance:K<Q.R>`（graph の `instance_of` で `P.K`）の method `instance:K<Q.R>.m` が `Q`、`test:T.t`（モジュール `T`）が `P.K.m` を呼ぶ。`Q` だけ違う | `tests: 1 of 1 affected`、`  test:T.t` |
| T8 | base で `test:T.t`→`P.gone`。head は `P.gone` がなく `test:T.t`→`Q.alt`。`P` だけ違う（`T` は同じ） | `tests: 1 of 1 affected`、`  test:T.t`（規則 (c)） |
| T9 | 鎖: `P0.a0`〜`P8.a8`、`Pi.ai` が `P(i-1).a(i-1)` を呼ぶ。`test:P9.t`（モジュール `P9`）が `a7` を呼ぶ。`P0` だけ違う | `tests: 1 of 1 affected`、`  test:P9.t` |
| T10 | T9 で test が `a8` を呼ぶ | `tests: all 1 (callers deeper than 8 calls)` |
| T11 | 鎖を `a0`〜`a9` に伸ばし、test が `a9` を呼ぶ | 同上 |
| T12 | `Q` の `instance:Eq<Q.R>`（graph の `instance_of` で `Eq`。`Eq` は symbols にない）と method `instance:Eq<Q.R>.eq`。`test:T.t` は `Q.R` を使うだけ。`Q` だけ違う | `tests: all 1 (an instance of a std class changed)`（演算子のオーバーロードは `calls` の辺なしに instance を呼ぶ。T7 の user の class は辺で届く） |

### F-4: 引数の拒否（`rejects_ambiguous_or_unused_arguments` に足す行。`E2000`。メッセージは標準エラーの `error[E2000]:` の後ろ）

使い方の行（`共通` ＝ ` [--path PATH] [--kb-dir DIR] [--no-refresh] [--json]`）: `usage: tsuzuri kb brief [ID | --project | --module M] [--budget N]共通`、`… kb show ID [--budget N]共通`、`… kb callers ID [--depth N] [--budget N]共通`（`callees` も）、`… kb uses TYPE [--budget N]共通`、`… kb tests-for ID [--budget N]共通`、`… kb search TEXT [--std] [--budget N]共通`、`… kb impact (--git REV | --from PATH [--to PATH])共通`。

| 引数 | メッセージ | 終了 |
| --- | --- | --- |
| `kb show`、`kb show a b` | `usage: tsuzuri kb show ID [--budget N] [--path PATH] [--kb-dir DIR] [--no-refresh] [--json]` | 2 |
| `kb show X --budget 99`、`--budget x` | `--budget must be an integer from 100 to 100000` | 2 |
| `kb show X --budget` | `--budget needs a value` | 2 |
| `kb callers X --depth 0`、`9`、`x` | `--depth must be an integer from 1 to 8` | 2 |
| `kb show X --depth 2` | `--depth is only valid with kb callers or kb callees` | 2 |
| `kb show X --std` | `--std is only valid with kb search, kb build, or kb clean` | 2 |
| `kb show X --module M`、`kb impact … --budget 200`、`kb show X --git HEAD` | `--module is only valid with kb brief`、`--budget is not valid with kb impact`、`--git is only valid with kb impact` | 2 |
| `kb brief X --project` | `kb brief takes one of ID, --project, or --module` | 2 |
| `kb show X --json --json` | `--json specified more than once` | 2 |
| `kb show X --bogus` | `unknown option '--bogus'; use --help` | 2 |
| `kb nope` | `unknown kb command 'nope'; use build, verify, clean, export, brief, show, callers, callees, uses, tests-for, or search`（Phase 1 の時点） | 2 |
| `kb show ""` | `id must not be empty` | 2 |
| `kb search ""` | `search text must not be empty` | 2 |
| `kb impact`、`kb impact --git HEAD --from x`、`kb impact --to x` | `usage: tsuzuri kb impact (--git REV \| --from PATH [--to PATH]) [--path PATH] [--kb-dir DIR] [--no-refresh] [--json]` | 2 |
| `kb impact --git -x`、`--git ""` | `invalid git revision '-x'`（空は `''`） | 2 |
| `kb impact --git nope` | `unknown git revision 'nope'` | 1 |
| `test --affected x --filter y`、`--index 0` | `--affected cannot be combined with --filter or --index` | 2 |
| `test --affected x --no-kb` | `--affected needs the KB; remove --no-kb or TSUZURI_KB=0` | 2 |
| `bench --affected x` | `--affected is only valid with test` | 2 |

### F-5: git のテスト（`tests/kb_impact.rs`）

リポジトリは一時ディレクトリに `git init`（`-c user.name=t -c user.email=t@t` でコミット。`GIT_*` は子へ渡らない）。`TMPDIR`（Windows は `TEMP`）をテスト用のディレクトリにして、`kb impact` が起こす一時ディレクトリを数える。

- `worktree_is_removed_on_success_and_on_failure`: (1) `kb_shop` をコミットし、作業ツリーを `kb_shop_head` にして `kb impact --git HEAD` が 0。(2) head が型エラーのプロジェクトで 1。どちらも終了後に `git worktree list` が 1 行で、`TMPDIR` に `.tsuzuri-*` が残らない。
- `git_status_is_unchanged`: 前後で `git status --porcelain=v1 -uall`・`git stash list`・`git branch --list`・`git rev-parse HEAD`・`git for-each-ref` が同じ。
- `hooks_do_not_run`: `.git/hooks/post-checkout`（`#!/bin/sh` と `echo ran > <テスト用ディレクトリの絶対パス>/marker`）を置いて `kb impact --git HEAD` を実行し、`marker` が作られない。
- `unknown_revision_and_not_a_repository`: `--git nope` が 1（`unknown git revision 'nope'`）。リポジトリの外の一時ディレクトリのプロジェクトで `--git HEAD` が 1（`'{root}' is not inside a git repository`）。
- `project_directory_missing_at_revision`: リポジトリの下の `app/` を後からコミットし、`--git HEAD~1 --path app` が 1（`the project directory 'app' does not exist at git revision 'HEAD~1' ({12 桁})`）。

### F-6: 差分テスト（`affected_tests_cover_every_failing_test`）と編集

C-3 の編集 `E1`〜`E5` を `kb_shop` の 1 ファイルの文字列置換として持つ（`E1`: `Cart.tz` の `line.quantity * line.unit_price` を `line.quantity + line.unit_price`、`E2`: `Main.tz` の `== 250` を `== 251`、`E3`: `Cart.tz` の `let mut sum = 0` を `let mut sum = 1`、`E4`: `Report.tz` の `(Cart.total lines)` を `(Cart.total lines + 0)`、`E5`: `Main.tz` の戻り値 `    0` を `    1`）。各編集で: (1) 編集したコピーで `tsuzuri test` を実行し、`not ok N - …` の `N - 1` を落ちた番号の集合にする。(2) `tsuzuri test <コピー> --affected <kb_shop> --list` が出す番号の集合を選ばれた集合にする。(3) 落ちた集合 ⊆ 選ばれた集合。(4) 選ばれた集合の大きさが C-3 の表（3・1・3・0・1）と同じ。編集を足したら期待の大きさも足す。`affected_with_no_changes_runs_nothing`（`--affected` に自分自身: `affected: 0 of 3 tests (…)`、何も実行せず 0）、`affected_all_when_the_manifest_changes`（`Tsuzuri.toml` の `version` を変えたコピー: `affected: all 3 tests (the manifest or a dependency changed)`）、`affected_value_is_a_directory_a_kb_or_a_revision`（VALUE に `kb_shop` の KB のディレクトリ・プロジェクトのディレクトリ・git のリビジョン `HEAD~1` を与えて同じ `affected:` の行。ラベルは与えた値。`./name` はパス、`refs/heads/name` はリビジョンとして読む）。

### F-7: 性能の測り方（`docs/benchmarks.md` の節 `## KB の照会と impact（G28）`）

合否は置かない。G27 付録 A-1 の `gen_project.py` で 200・1,000 モジュールのプロジェクト `A` と、1 モジュールの本体を変えた `B` を作り、`check A`・`check B` で KB を作ってから、`median_ms.py 7` の中央値を表にする（load average も書く）: `kb show ID --path A --no-refresh`、`kb callers ID --depth 8 --path A --no-refresh`、`kb impact --from A --path B --no-refresh`、`kb impact --git HEAD~1 --path B`（チェックアウトと KB の作成を含む）、比較に `check A`・`api-diff A B`。

<!-- G28-APPENDIX-END -->

