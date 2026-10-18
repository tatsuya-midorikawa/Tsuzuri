# G27 付録: 実測・想定出力・参照実装

[G27-build-time-kb.md](G27-build-time-kb.md) の付録。A 実測の再現、B `examples/polymorphism`（symbols だけの KB）、C `examples/point`（symbols と graph）、D `kb_shop`（全ファイル）、E 切り詰めの手計算、F 参照実装（Python）、G 計測の手順と増分の鍵、H Rust の型と `emit.rs` の足し方、I `E2009` のメッセージと各テスト。

**想定出力について。** B〜E のカードと D の行は、**実装前に Python の参照実装（付録 F）で作った想定出力**で、コンパイラの出力を写したものではない。B・C の入力は G26 の golden（`tests/golden/kb/polymorphism/symbols.jsonl`・`tests/golden/kb/point/{symbols,graph}.jsonl`。G26 の付録 H の H1・H2）そのもの、D の入力は G26 の規則（付録 B・D の決定表）で手計算した行である。実装の出力と食い違ったら、規則（本文の仕様）と照らしてどちらが正しいかを決めて報告する（停止条件 6）。コンパイラの現在の出力を黙って golden に写さない。

**行の ID について。** G26 の golden は `kb::collect_sources`（名前空間なし）の出力で、`tsuzuri` の CLI が `examples/point` を読むと、フォルダ名 `point` が名前空間 `Point` になる（本文「現状」の `folder_namespace`）。B・C のカードは golden の ID（`Main`・`Point.distance`）で作ってあり、CLI の出力ではない。D の `kb_shop` は `Tsuzuri.toml` が名前空間 `Acme::Shop` を宣言するので、CLI の出力と同じ ID になる。

## 付録 A: 実測の再現

環境: Apple 製 macOS、`rustc 1.98.1`、`target/release/tsuzuri`（9,465,008 byte。作業ツリー `9dd0fbbc`。ソースは `a2313d19` と同じ）、`python3` 3.14。作業ディレクトリは `target/tz-G27-work/`（`.gitignore` の `target/` の下）。**下の数値は負荷の低い時間帯の値**で、他のプロセスが動いていると 1.5〜2 倍になる（同じ機械で負荷平均 30 のとき、A-4 の作成は 0.33 ms/個）。結論（費用はファイルの個数。hard link は遅い）は負荷で変わらない。

### A-1: `check` の時間（9 回の中央値）

生成器 `gen_project.py`（`python3 gen_project.py 200 target/tz-G27-work/syn200`）:

```python
import os, sys
count = int(sys.argv[1])
root = sys.argv[2]
os.makedirs(root, exist_ok=True)
for i in range(count):
    name = f"M{i:04d}"
    lines = [f"record Item{i:04d} {{ id: i64, weight: i64 }}", ""]
    for j in range(4):
        callee = ""
        if i > 0:
            callee = f" + M{i-1:04d}.f{j}_{i-1:04d} n"
        export = "export " if j == 0 else ""
        lines.append(f"{export}def f{j}_{i:04d} :: i64 -> i64 = \\n -> n * {j + 1} + {i}{callee}")
    lines.append("")
    lines.append(f"def private{i:04d} :: ref Item{i:04d} -> i64 = \\item -> item.id + item.weight")
    lines.append("")
    lines.append(f'test "f0 of {name}" = assert (f0_{i:04d} 1 > 0)')
    with open(os.path.join(root, name + ".tz"), "w") as f:
        f.write("\n".join(lines) + "\n")
with open(os.path.join(root, "Main.tz"), "w") as f:
    f.write(f"def main :: unit -> i32 = \\() ->\n    do! IO.write_line (M{count-1:04d}.f0_{count-1:04d} 1)\n    0\n")
```

計時 `median_ms.py`（最初の 1 回は捨てる）: `python3 median_ms.py 9 target/release/tsuzuri check target/tz-G27-work/syn1000`。

```python
import statistics, subprocess, sys, time

runs = int(sys.argv[1])
times = []
for _ in range(runs + 1):
    start = time.perf_counter()
    subprocess.run(sys.argv[2:], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    times.append((time.perf_counter() - start) * 1000)
print(f"{statistics.median(times[1:]):.1f}")
```

| プロジェクト | ソース合計 | `check` の中央値 |
| --- | --- | --- |
| `gen_project.py 1`（`syn1`） | 422 B | 38.9 ms |
| `gen_project.py 200` | 84,167 B | 70.2 ms |
| `gen_project.py 1000` | 422,567 B | 445.7 ms |

`TSUZURI_TIME_PASSES=1 tsuzuri check syn1000` の内訳: `load` 195 ms・`check` 87 ms・`specialize` 35 ms。

### A-2: 実行ファイルの SHA-256

`cache::build_key` が使う `Sha256` を取り出した独立のプログラムで 9,465,008 byte を 49.5〜52.8 ms（約 190 MB/s）。`shasum -a 256 target/release/tsuzuri` も同じ桁（`python3 median_ms.py 9 shasum -a 256 target/release/tsuzuri`）。`frontend_cache::compiler_identity()` は `metadata` だけで読まない。

### A-3: 20,000 個のファイルを持つ `target/` の `check`

```text
mkdir -p target/tz-G27-work/scan/target/tz/kb/brief/L2
cp target/tz-G27-work/scan-empty/Main.tz target/tz-G27-work/scan/Main.tz     # def main :: unit -> i32 = \() -> 0
python3 -c "import os; d='target/tz-G27-work/scan/target/tz/kb/brief/L2'; [open(f'{d}/{i:05d}.md','w').write('x'*400) for i in range(20000)]"
python3 median_ms.py 15 target/release/tsuzuri check target/tz-G27-work/scan-empty      # 36.2 ms
python3 median_ms.py 15 target/release/tsuzuri check target/tz-G27-work/scan            # 55.3 ms
```

`collect_sources` が `target/` の下も `read_dir` し、各エントリの `file_type` を取るので 19 ms 増える（本文の D4）。Phase 1 の後は、`target/tz/kb/.tsuzuri-kb` を置くと `scan` の値が `scan-empty` に戻る（手順 4）。

### A-4: ファイルシステムの費用

`fsbench.py`（`python3 fsbench.py target/tz-G27-work/fsb-run`。ディレクトリは作って消す）:

```python
import os, shutil, sys, time
from concurrent.futures import ThreadPoolExecutor

root = sys.argv[1]
body = b"x" * 400

def write(path):
    with open(path, "wb") as f: f.write(body)

def create(directory, count, threads):
    os.makedirs(directory)
    paths = [os.path.join(directory, f"{i:05d}.md") for i in range(count)]
    start = time.perf_counter()
    if threads == 1:
        for p in paths: write(p)
    else:
        with ThreadPoolExecutor(threads) as pool: list(pool.map(write, paths))
    return (time.perf_counter() - start) * 1000

os.makedirs(root)
a = os.path.join(root, "a")
ms = create(a, 20000, 1)
print(f"create 20000 files (400 B), 1 thread: {ms:.0f} ms ({ms / 20000:.3f} ms/file)")
start = time.perf_counter(); os.rename(a, os.path.join(root, "a2")); print(f"rename directory: {(time.perf_counter() - start) * 1000:.1f} ms")
b = os.path.join(root, "b"); os.makedirs(b)
start = time.perf_counter()
for i in range(2000): os.link(os.path.join(root, "a2", f"{i:05d}.md"), os.path.join(b, f"{i:05d}.md"))
ms = (time.perf_counter() - start) * 1000; print(f"hard link 2000 files: {ms:.0f} ms ({ms / 2000:.3f} ms/file)")
start = time.perf_counter(); shutil.rmtree(os.path.join(root, "a2")); ms = (time.perf_counter() - start) * 1000
print(f"remove 20000 files: {ms:.0f} ms ({ms / 20000:.3f} ms/file)")
shutil.rmtree(b)
for threads in (1, 4, 8):
    d = os.path.join(root, f"t{threads}")
    ms = create(d, 3000, threads)
    print(f"create 3000 files, {threads} thread(s): {ms / 3000 * 1000:.0f} us/file")
    shutil.rmtree(d)
shutil.rmtree(root)
```

| 操作 | 低負荷の値 | 負荷平均 30 のとき |
| --- | --- | --- |
| 400 byte のファイル 20,000 個を作る（1 スレッド） | 4,186 ms（0.21 ms/個） | 6,590 ms（0.33 ms/個） |
| 3,000 個を 1・4・8 スレッドで作る | 290・98・90 µs/個 | 308・228・223 µs/個 |
| ディレクトリの rename | 6 ms | 22 ms |
| hard link | 0.9 ms/個（20,000 個で 17.9 s） | 0.87 ms/個 |
| 20,000 個を消す | 0.13〜0.2 ms/個 | 0.27 ms/個 |

結論: 並列の書き込みの速さは機械の負荷で変わる（低負荷で約 3 倍、高負荷で約 1.4 倍）。hard link は作成より遅く、再利用の道具にならない。KB の費用は「ファイルの個数 × 0.2〜0.3 ms ÷ 並列度」で見積もれる（1,000 モジュールの `kb_shop` 型で約 2,300 個 → 並列で 160〜270 ms）。

## 付録 B: `examples/polymorphism`（symbols だけの KB）

入力は G26 の付録 H2 の 19 行（`tests/golden/kb/polymorphism/symbols.jsonl`）だけで、graph・facts・tests は無い（G26 Phase 1 だけを出荷した KB）。`project` は `polymorphism`、`edition` は `2026`、警告 0。カードの置き場は `tests/golden/kb/polymorphism/brief/`（L2 のファイル名は ID の `stem`）。

規則の確かめ:

- L0 の `modules 2`（`module` 行）・`symbols 19`（全行）・`tests 0`。`Main.main` が無く、`Main.$entry` の行があるので `entry: top-level code of Main`。
- Modules の並びは公開の最上位の行の数の降順。`Main` は `record Point`・`def add`・`def score`・`instance` 3 つの 6、`Classes` は `class` 2 つの 2 なので `Main` が先。`$entry`（`public` が偽で名前が `$` で始まる）は数えない。
- Exports は無い（`export` が真の `def`・`const` が 0）。Dependencies・Tests・Effects は graph・facts が無いか 0 件なので、ブロックごと出ない。
- L1 の `Main` の `6 items, 6 public`: `record`・`def` 2・`instance` 3。`method` の行（`instance:Add<Main.Point>.add` の形）と `field` の行は最上位ではないので数えない。Instances の名前は `instance` 行の `name`（`Classes.Score<Point>`）。
- L2 のファイルは `FILE_KINDS` の行で、11 件（`class` 2・`method` 6・`record` 1・`def` 2）。`methods:` の行は class だけにある（instance の `method` は ID が `instance:Add<Main.Point>.add` のように instance の下にあり、`class` の子ではない）。

##### brief/L0.md（387 字 / 上限 2,400）

```text
# polymorphism
tz-kb/1 | edition 2026 | modules 2 | symbols 19 | tests 0
status: ok
entry: top-level code of Main

## Modules
- Main (Main.tz): record Point; def add, score; instance Add<Point>, Eq<Point>, Classes.Score<Point>
- Classes (Classes.tt): class Score, Size

next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl ('::' is '.' in file names)
```

手計算（行の字数＋改行 1）: 15＋58＋11＋30＋1＋11＋101＋42＋1＋117 ＝ 387。上限 2,400 に収まるので、落とす行はない。

##### brief/L1/Classes.md（103 字 / 上限 1,600）

```text
# Classes (Classes.tt)
2 items, 2 public | 0 tests

## Types
- class Score {score}
- class Size {size}
```

##### brief/L1/Main.md（258 字 / 上限 1,600）

```text
# Main (Main.tz)
6 items, 6 public | 0 tests

## Types
- record Point {x, y}

## Functions
- def add :: 'a -> 'a -> 'a
- def score :: Classes.Score<'a> => ref 'a -> i32

## Instances
- instance Add<Point>
- instance Eq<Point>
- instance Classes.Score<Point>
```

##### L2（11 件。ファイル名 → カード）

`brief/L2/Classes.Score.md`（80 字）:

```text
Classes.Score
class | public | Classes.tt:1
sig: class Score<'a>
methods: score
```

`brief/L2/Classes.Score.score.md`（100 字）:

```text
Classes.Score.score
method | public | Classes.tt:2
sig: def score :: ref 'a -> i32
params: arg1:ref
```

`brief/L2/Classes.Size.md`（77 字）:

```text
Classes.Size
class | public | Classes.tt:5
sig: class Size<'a>
methods: size
```

`brief/L2/Classes.Size.size.md`（97 字）:

```text
Classes.Size.size
method | public | Classes.tt:6
sig: def size :: ref 'a -> i64
params: arg1:ref
```

`brief/L2/Main.Point.md`（80 字）:

```text
Main.Point
record | public | Main.tz:1
sig: record Point
fields: x: i32, y: i32
```

`brief/L2/Main.add.md`（86 字）:

```text
Main.add
def | public | Main.tz:3
sig: def add :: 'a -> 'a -> 'a
params: x:own, y:own
```

`brief/L2/Main.score.md`（108 字）:

```text
Main.score
def | public | Main.tz:18
sig: def score :: Classes.Score<'a> => ref 'a -> i32
params: value:ref
```

`brief/L2/instance%3AAdd%3CMain.Point%3E.add.md`（ID `instance:Add<Main.Point>.add`。142 字）:

```text
instance:Add<Main.Point>.add
method | public | Main.tz:6
sig: def add :: Main.Point -> Main.Point -> Main.Point
params: left:copy, right:copy
```

`brief/L2/instance%3AClasses.Score%3CMain.Point%3E.score.md`（128 字）:

```text
instance:Classes.Score<Main.Point>.score
method | public | Main.tz:15
sig: def score :: ref Main.Point -> i32
params: point:ref
```

`brief/L2/instance%3AEq%3CMain.Point%3E.eq.md`（140 字）:

```text
instance:Eq<Main.Point>.eq
method | public | Main.tz:10
sig: def eq :: ref Main.Point -> ref Main.Point -> bool
params: left:ref, right:ref
```

`brief/L2/instance%3AEq%3CMain.Point%3E.ne.md`（140 字）:

```text
instance:Eq<Main.Point>.ne
method | public | Main.tz:11
sig: def ne :: ref Main.Point -> ref Main.Point -> bool
params: left:ref, right:ref
```

最長のカード（142 字）は上限 700 の 20%。`params:` の `arg1` は G26 の B3 が生成名（宣言に名前のない引数）に付ける `arg{i+1}`。

## 付録 C: `examples/point`（symbols と graph）

入力は G26 の付録 H1 の `tests/golden/kb/point/symbols.jsonl`（8 行）と `graph.jsonl`（6 行）。facts は空の行だけで、カードに効果・site の行は出ない。`project` は `point`、`edition` は `2026`、警告 0。

規則の確かめ:

- Dependencies は `Main -> Point`（`calls`・`uses_type` の `from` が `Main.$entry`、`to` が `Point.*`。`$entry` の行は KB の行なので辺の端として使う）。`Main` は公開の最上位の行が 0 なので `(no public items)`。
- L1 の `Point`: `callers 2` は `Point.distance` の `calls` の `from` が `Main.$entry` と `Point.hypotenuse` の 2 つ。`used by: Main`。
- Exports は `export def hypotenuse` だけ。署名の ` :: ` より後ろを写す。

##### brief/L0.md（416 字 / 上限 2,400）

```text
# point
tz-kb/1 | edition 2026 | modules 2 | symbols 8 | tests 0
status: ok
entry: top-level code of Main

## Modules
- Point (Point.tz): record Point; def distance, hypotenuse
- Main (Main.tz): (no public items)

## Exports
- Point.hypotenuse :: f64 -> f64 -> f64

## Dependencies
- Main -> Point

next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl ('::' is '.' in file names)
```

手計算: 8＋57＋11＋30＋1＋11＋59＋36＋1＋11＋40＋1＋16＋16＋1＋117 ＝ 416。

##### brief/L1/Main.md（58 字）

```text
# Main (Main.tz)
0 items, 0 public | 0 tests

uses: Point
```

##### brief/L1/Point.md（196 字）

```text
# Point (Point.tz)
3 items, 3 public | 0 tests

## Types
- record Point {x, y}

## Functions
- def distance :: Point -> f64 | callers 2
- export def hypotenuse :: f64 -> f64 -> f64

used by: Main
```

##### L2（3 件。ID の byte 順は `Point.Point` ＜ `Point.distance`。ファイルの順位は `export`＝0 の `hypotenuse` が先）

`brief/L2/Point.hypotenuse.md`（149 字）:

```text
Point.hypotenuse
def | public export | Point.tz:6
sig: export def hypotenuse :: f64 -> f64 -> f64
params: x:copy, y:copy
callees (1): Point.distance
```

`brief/L2/Point.Point.md`（141 字）:

```text
Point.Point
record | public | Point.tz:1
sig: record Point
fields: x: f64, y: f64
used by (3): Main.$entry, Point.distance, Point.hypotenuse
```

`brief/L2/Point.distance.md`（155 字）:

```text
Point.distance
def | public | Point.tz:3
sig: def distance :: Point -> f64
params: point:copy
callees (1): sqrt
callers (2): Main.$entry, Point.hypotenuse
```

## 付録 D: `kb_shop`（全ファイルの完全な実例）

`tests/fixtures/kb_shop/`（新規。手順 1）の入力と、`kb_shop` を `tsuzuri check tests/fixtures/kb_shop` したときに `tests/fixtures/kb_shop/target/tz/kb/` にできる全ファイルの**想定**（付録 F の参照実装で作った）。`tsuzuri fmt --check`・`check`・`test` が通る（警告は 1 件。`test --list` は 3 件）。G28 が使う `kb_shop_head`（このプロジェクトを編集した版）は G28 の付録にある。`tests/golden/kb/shop/` には、この節のファイルを同じ相対パスで置く（`.tsuzuri-kb` は機械ごとなので置かない）。

### D-1: 入力

`kb_shop/Tsuzuri.toml`（84 byte）:

```toml
[package]
name = "shop"
version = "0.1.0"
edition = "2026"
namespace = "Acme::Shop"
```

`kb_shop/Cart.tz`（702 byte）:

```tsuzuri
namespace Acme::Shop

/// One line of a cart: a product, how many, and its price in cents.
record Line { sku: string, quantity: i64, unit_price: i64 }

/// The price of one line in cents.
def line_total :: ref Line -> i64 = \line -> line.quantity * line.unit_price

/// The price of all lines in cents.
def total :: ref [Line] -> i64 = \lines ->
    let mut sum = 0
    let mut i = 0
    while i < lines.length do
        sum = sum + line_total (ref lines[i])
        i = i + 1
    sum

test "line total" = assert (line_total (ref Line { sku: "pen", quantity: 3, unit_price: 100 }) == 300)
test "empty cart" = assert (total (ref new [Line](0, i -> Line { sku: "x", quantity: 0, unit_price: 0 })) == 0)
```

`kb_shop/Report.tz`（131 byte）:

```tsuzuri
namespace Acme::Shop

using Acme::Shop

def print_total :: ref [Line] -> unit = \lines ->
    do! IO.write_line (Cart.total lines)
```

`kb_shop/Main.tz`（269 byte）:

```tsuzuri
namespace Acme::Shop

def main :: unit -> i32 = \() ->
    let lines = [Line { sku: "pen", quantity: 3, unit_price: 100 }]
    Report.print_total (ref lines)
    0

test "main runs" = assert (Cart.total (ref [Line { sku: "ink", quantity: 1, unit_price: 250 }]) == 250)
```

### D-2: `symbols.jsonl`（14 行。G26 の B3 の規則。`id` の byte 順）

```jsonl
{"id":"Acme::Shop::Cart","kind":"module","module":"Acme::Shop::Cart","name":"Cart","signature":"module Acme::Shop::Cart","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":0,"end":0,"line":1,"column":1}}
{"id":"Acme::Shop::Cart.Line","kind":"record","module":"Acme::Shop::Cart","name":"Line","signature":"record Line","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":"One line of a cart: a product, how many, and its price in cents.","deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":98,"end":102,"line":4,"column":8}}
{"id":"Acme::Shop::Cart.Line.quantity","kind":"field","module":"Acme::Shop::Cart","name":"quantity","signature":"quantity: i64","params":[],"result":"i64","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":118,"end":126,"line":4,"column":28}}
{"id":"Acme::Shop::Cart.Line.sku","kind":"field","module":"Acme::Shop::Cart","name":"sku","signature":"sku: string","params":[],"result":"string","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":105,"end":108,"line":4,"column":15}}
{"id":"Acme::Shop::Cart.Line.unit_price","kind":"field","module":"Acme::Shop::Cart","name":"unit_price","signature":"unit_price: i64","params":[],"result":"i64","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":133,"end":143,"line":4,"column":43}}
{"id":"Acme::Shop::Cart.line_total","kind":"def","module":"Acme::Shop::Cart","name":"line_total","signature":"def line_total :: ref Line -> i64","params":[{"name":"line","type":"ref Line","conv":"ref"}],"result":"i64","public":true,"export":false,"pure":false,"doc":"The price of one line in cents.","deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":192,"end":202,"line":7,"column":5}}
{"id":"Acme::Shop::Cart.total","kind":"def","module":"Acme::Shop::Cart","name":"total","signature":"def total :: ref [Line] -> i64","params":[{"name":"lines","type":"ref [Line]","conv":"ref"}],"result":"i64","public":true,"export":false,"pure":false,"doc":"The price of all lines in cents.","deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":307,"end":312,"line":10,"column":5}}
{"id":"Acme::Shop::Main","kind":"module","module":"Acme::Shop::Main","name":"Main","signature":"module Acme::Shop::Main","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":0,"end":0,"line":1,"column":1}}
{"id":"Acme::Shop::Main.main","kind":"def","module":"Acme::Shop::Main","name":"main","signature":"def main :: unit -> i32","params":[{"name":"arg1","type":"unit","conv":"copy"}],"result":"i32","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":26,"end":30,"line":3,"column":5}}
{"id":"Acme::Shop::Report","kind":"module","module":"Acme::Shop::Report","name":"Report","signature":"module Acme::Shop::Report","params":[],"result":null,"public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Report.tz","start":0,"end":0,"line":1,"column":1}}
{"id":"Acme::Shop::Report.print_total","kind":"def","module":"Acme::Shop::Report","name":"print_total","signature":"def print_total :: ref [Line] -> unit","params":[{"name":"lines","type":"ref [Line]","conv":"ref"}],"result":"unit","public":true,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Report.tz","start":44,"end":55,"line":5,"column":5}}
{"id":"test:Acme::Shop::Cart.empty cart","kind":"test","module":"Acme::Shop::Cart","name":"empty cart","signature":"test \"empty cart\"","params":[],"result":null,"public":false,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":595,"end":607,"line":19,"column":6}}
{"id":"test:Acme::Shop::Cart.line total","kind":"test","module":"Acme::Shop::Cart","name":"line total","signature":"test \"line total\"","params":[],"result":null,"public":false,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Cart.tz","start":492,"end":504,"line":18,"column":6}}
{"id":"test:Acme::Shop::Main.main runs","kind":"test","module":"Acme::Shop::Main","name":"main runs","signature":"test \"main runs\"","params":[],"result":null,"public":false,"export":false,"pure":false,"doc":null,"deprecated":null,"origin":"project","span":{"path":"Main.tz","start":170,"end":181,"line":8,"column":6}}
```

`main` の `arg1` は、引数が名前のない `()` のパターンなので G26 の B3 が付ける生成名（`arg{i+1}`）。G26 の実装が別の名前を付けたら、この行と L2 の `params: arg1:copy` を直す（停止条件 6 の A）。

### D-3: `graph.jsonl`（21 行。`rel`→`from`→`to` の byte 順）と `tests.jsonl`（3 行。`id` の byte 順。`index` は `tsuzuri test --list` の番号）

```jsonl
{"rel":"calls","from":"Acme::Shop::Cart.total","to":"Acme::Shop::Cart.line_total","args":["ref"],"span":{"path":"Cart.tz","start":434,"end":444,"line":14,"column":21}}
{"rel":"calls","from":"Acme::Shop::Main.main","to":"Acme::Shop::Report.print_total","args":["ref"],"span":{"path":"Main.tz","start":127,"end":145,"line":5,"column":5}}
{"rel":"calls","from":"Acme::Shop::Report.print_total","to":"Acme::Shop::Cart.total","args":["ref"],"span":{"path":"Report.tz","start":113,"end":123,"line":6,"column":24}}
{"rel":"calls","from":"Acme::Shop::Report.print_total","to":"IO.write_line","args":["copy"],"span":{"path":"Report.tz","start":98,"end":111,"line":6,"column":9}}
{"rel":"calls","from":"test:Acme::Shop::Cart.empty cart","to":"Acme::Shop::Cart.total","args":["ref"],"span":{"path":"Cart.tz","start":618,"end":623,"line":19,"column":29}}
{"rel":"calls","from":"test:Acme::Shop::Cart.line total","to":"Acme::Shop::Cart.line_total","args":["ref"],"span":{"path":"Cart.tz","start":515,"end":525,"line":18,"column":29}}
{"rel":"calls","from":"test:Acme::Shop::Main.main runs","to":"Acme::Shop::Cart.total","args":["ref"],"span":{"path":"Main.tz","start":192,"end":202,"line":8,"column":28}}
{"rel":"imports","from":"Acme::Shop::Report","to":"Acme::Shop","span":{"path":"Report.tz","start":22,"end":38,"line":3,"column":1}}
{"rel":"tested_by","from":"Acme::Shop::Cart.line_total","to":"test:Acme::Shop::Cart.empty cart","span":{"path":"Cart.tz","start":595,"end":607,"line":19,"column":6}}
{"rel":"tested_by","from":"Acme::Shop::Cart.line_total","to":"test:Acme::Shop::Cart.line total","span":{"path":"Cart.tz","start":492,"end":504,"line":18,"column":6}}
{"rel":"tested_by","from":"Acme::Shop::Cart.line_total","to":"test:Acme::Shop::Main.main runs","span":{"path":"Main.tz","start":170,"end":181,"line":8,"column":6}}
{"rel":"tested_by","from":"Acme::Shop::Cart.total","to":"test:Acme::Shop::Cart.empty cart","span":{"path":"Cart.tz","start":595,"end":607,"line":19,"column":6}}
{"rel":"tested_by","from":"Acme::Shop::Cart.total","to":"test:Acme::Shop::Main.main runs","span":{"path":"Main.tz","start":170,"end":181,"line":8,"column":6}}
{"rel":"uses_type","from":"Acme::Shop::Cart.line_total","to":"Acme::Shop::Cart.Line","span":{"path":"Cart.tz","start":210,"end":214,"line":7,"column":23}}
{"rel":"uses_type","from":"Acme::Shop::Cart.total","to":"Acme::Shop::Cart.Line","span":{"path":"Cart.tz","start":321,"end":325,"line":10,"column":19}}
{"rel":"uses_type","from":"Acme::Shop::Main.main","to":"Acme::Shop::Cart.Line","span":{"path":"Main.tz","start":72,"end":76,"line":4,"column":18}}
{"rel":"uses_type","from":"Acme::Shop::Report.print_total","to":"Acme::Shop::Cart.Line","span":{"path":"Report.tz","start":64,"end":68,"line":5,"column":25}}
{"rel":"uses_type","from":"test:Acme::Shop::Cart.empty cart","to":"Acme::Shop::Cart.Line","span":{"path":"Cart.tz","start":634,"end":638,"line":19,"column":45}}
{"rel":"uses_type","from":"test:Acme::Shop::Cart.empty cart","to":"Acme::Shop::Cart.Line","span":{"path":"Cart.tz","start":648,"end":652,"line":19,"column":59}}
{"rel":"uses_type","from":"test:Acme::Shop::Cart.line total","to":"Acme::Shop::Cart.Line","span":{"path":"Cart.tz","start":531,"end":535,"line":18,"column":45}}
{"rel":"uses_type","from":"test:Acme::Shop::Main.main runs","to":"Acme::Shop::Cart.Line","span":{"path":"Main.tz","start":209,"end":213,"line":8,"column":45}}
```

```jsonl
{"id":"test:Acme::Shop::Cart.empty cart","kind":"test","module":"Acme::Shop::Cart","name":"empty cart","index":1,"span":{"path":"Cart.tz","start":595,"end":607,"line":19,"column":6}}
{"id":"test:Acme::Shop::Cart.line total","kind":"test","module":"Acme::Shop::Cart","name":"line total","index":0,"span":{"path":"Cart.tz","start":492,"end":504,"line":18,"column":6}}
{"id":"test:Acme::Shop::Main.main runs","kind":"test","module":"Acme::Shop::Main","name":"main runs","index":2,"span":{"path":"Main.tz","start":170,"end":181,"line":8,"column":6}}
```

`tested_by` は、各 test から呼び出しを深さ 8 まで辿って届く `def`（G26 の A3）。`main runs` は `Cart.total` を呼び、`total` が `line_total` を呼ぶので、両方に届く。`Report.print_total`・`Main.main` はどの test からも届かない。

### D-4: `facts/*.jsonl` と `diag.jsonl`

G26 の Phase 3〜5 を出荷し、A20 が無い build（Level B。`effects_complete: false`・`alloc_complete: false`）の想定。`fact_modules()` は `Cart`・`Main`・`Report`（key の昇順）。各ファイルの行は `id` の byte 順。

`facts/Cart.jsonl`（4 行）:

```jsonl
{"id":"Acme::Shop::Cart.line_total","effects":[],"effect_sites":[],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
{"id":"Acme::Shop::Cart.total","effects":[],"effect_sites":[],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[{"span":{"path":"Cart.tz","start":450,"end":458,"line":14,"column":37},"kind":"bounds_check","proven":false}],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
{"id":"test:Acme::Shop::Cart.empty cart","effects":[],"effect_sites":[],"effects_complete":false,"copy_sites":[],"alloc_sites":[{"span":{"path":"Cart.tz","start":629,"end":694,"line":19,"column":40},"kind":"new_array"}],"alloc_complete":false,"trap_sites":[{"span":{"path":"Cart.tz","start":610,"end":701,"line":19,"column":21},"kind":"assert","proven":false}],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
{"id":"test:Acme::Shop::Cart.line total","effects":[],"effect_sites":[],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[{"span":{"path":"Cart.tz","start":507,"end":589,"line":18,"column":21},"kind":"assert","proven":false}],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
```

`facts/Main.jsonl`（2 行）:

```jsonl
{"id":"Acme::Shop::Main.main","effects":["io"],"effect_sites":[{"effect":"io","span":{"path":"Main.tz","start":127,"end":145,"line":5,"column":5},"via":["Acme::Shop::Report.print_total","IO.write_line"]}],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
{"id":"test:Acme::Shop::Main.main runs","effects":[],"effect_sites":[],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[{"span":{"path":"Main.tz","start":184,"end":268,"line":8,"column":20},"kind":"assert","proven":false}],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
```

`facts/Report.jsonl`（1 行）:

```jsonl
{"id":"Acme::Shop::Report.print_total","effects":["io"],"effect_sites":[{"effect":"io","span":{"path":"Report.tz","start":98,"end":111,"line":6,"column":9},"via":["IO.write_line"]}],"effects_complete":false,"copy_sites":[],"alloc_sites":[],"alloc_complete":false,"trap_sites":[],"ownership":[],"recursion":"none","loops":[],"specializations":0,"truncated":{}}
```

`diag.jsonl`（1 行。`tsuzuri check tests/fixtures/kb_shop --json` の 1 行の `path` を、ルートからの相対パスにしたもの。W1006 は入れない）:

```jsonl
{"severity":"warning","code":"W1001","message":"unused local 'i'; prefix it with '_' to silence this warning","path":"Cart.tz","span":{"start":643,"end":644,"line":19,"column":54,"end_line":19,"end_column":55}}
```

### D-5: `manifest.json` と `.tsuzuri-kb`

`source_hash` は §3 の式で、入力は `("", "Cart.tz")`・`("", "Main.tz")`・`("", "Report.tz")`・`("shop", "Tsuzuri.toml")`（ルートのソースの `package` は `None`、manifest の `package` は `shop`）の順。`modules[].source` は各ファイルの内容の SHA-256（`shasum -a 256 Cart.tz` と同じ）。`files` の値は、この付録の各ファイルの SHA-256。

```json
{
  "schema": "tz-kb/1",
  "compiler": "tsuzuri 0.1.0",
  "edition": "2026",
  "project": "shop",
  "version": "0.1.0",
  "complete": true,
  "source_hash": "1c4ce603d1ab50e2bbee3bc641a5909b52253d5c634b1a3d88871c43bb01b4c4",
  "counts": {
    "modules": 3,
    "symbols": 14,
    "public_symbols": 8,
    "edges": 21,
    "tests": 3,
    "benches": 0,
    "errors": 0,
    "warnings": 1,
    "l2_cards": 5,
    "l2_omitted": 0
  },
  "modules": [
    {"name":"Acme::Shop::Cart","key":"Cart","origin":"project","path":"Cart.tz","source":"7b8e394e141b45edcd80a35e7d1f06cbffeb28b4cf5aca35c4f4b192c5ed2750"},
    {"name":"Acme::Shop::Main","key":"Main","origin":"project","path":"Main.tz","source":"d7208bd0a04aed88790357d4a0edde9cbb0b5aa25a44ea69910d70e700a1ae65"},
    {"name":"Acme::Shop::Report","key":"Report","origin":"project","path":"Report.tz","source":"7c5a4d722b7b150685203cdf3ccb91a11ab3a14c9ea6b41d8e1ada3691f3c6cf"}
  ],
  "files": {
    "brief/L0.md": "a15e46f57395d14a2b09b42593785b3017590a49520cd0dc579dad7c012cbb0b",
    "brief/L1/Cart.md": "6d3cbaa72431b22eb9c1432f3bb9489903680720e2d61fb953b4491d01383fb7",
    "brief/L1/Main.md": "58c2eabc39ddd69125450107bbea7f7146a83af499aed70c875d077d37bf5235",
    "brief/L1/Report.md": "f50ab8fbc07fcc5447f3f725f3fa083846e28bdb102a9261f87d4adf3d7d1502",
    "brief/L2/Acme.Shop.Cart.Line.md": "4acccef323042c08a669040e3fe6898d20fe07a2179ee61a761f9c92fbc11fff",
    "brief/L2/Acme.Shop.Cart.line_total.md": "d51c9c8a026ea3860a85617c10297ac0a331c5a1da099e0f97c35a379b11092c",
    "brief/L2/Acme.Shop.Cart.total.md": "d5287b00b65dd434fd75aadfd29bbb9ca01011df22427a1c512870f00f919360",
    "brief/L2/Acme.Shop.Main.main.md": "32d82930cdcc82da72765478048b10cc1a0055830e93715e7a409c440d9ec86e",
    "brief/L2/Acme.Shop.Report.print_total.md": "6b4fc3b3a18e85ad5a6b6c7775fc6676f824d42e16bc04c50fe60c6685210ab9",
    "diag.jsonl": "e32d79302e1d911bc686580275821e910c60d0797525219d753278573d087f3c",
    "facts/Cart.jsonl": "7abc1c63b6b5dde5d567a4320888387b71dd47ae6da52f89dea63efcafb5a3c6",
    "facts/Main.jsonl": "bc651ca6231cbed5ff379da7be4cc8cdb32610f3e84af2fa5d667f38feb438ad",
    "facts/Report.jsonl": "d6119efa78736ed3bdbd7e726a680566e88a4c1eda291f4d691f389180f39d39",
    "graph.jsonl": "dd553345d0325da8471540ed23fd855e722fe279428cdfddad38b8577b359057",
    "symbols.jsonl": "6a53090ddea39cb4feb714b971e50927cc54fa2d61aba5185e1932358de6a75a",
    "tests.jsonl": "412f96bae1f9af2f9764d4abdcd3651f4896be65935e51d0a3a6501a1312e618"
  }
}
```

`counts` の数え方（§3）: `modules` 3、`symbols` 14（`symbols.jsonl` の行）、`public_symbols` 8（`Line`・`sku`・`quantity`・`unit_price`・`line_total`・`total`・`print_total`・`main`。`module` 行と `test` 行は含まない）、`edges` 21、`tests` 3、`warnings` 1、`l2_cards` 5（`Line`・`line_total`・`total`・`main`・`print_total`）、`l2_omitted` 0。

`.tsuzuri-kb`（機械ごと。2 行。stamp は 64 桁の 16 進で、コンパイラの版ごとに違う）:

```text
generated by tsuzuri kb
stamp 0000000000000000000000000000000000000000000000000000000000000000
```

### D-6: カード（L0 が 609 字、L1 が 299・174・189 字、L2 が 352・327・384・150・217 字）

L0 の手計算（行の字数＋改行 1）: 13＋58＋22＋29＋1＋11＋65＋39＋50＋1＋16＋59＋41＋1＋9＋22＋22＋1＋23＋8＋1＋117 ＝ 609。上限 2,400 に収まる。

`brief/L0.md`:

```text
# shop 0.1.0
tz-kb/1 | edition 2026 | modules 3 | symbols 14 | tests 3
status: ok, 1 warning
entry: Acme::Shop::Main.main

## Modules
- Acme::Shop::Cart (Cart.tz): record Line; def line_total, total
- Acme::Shop::Main (Main.tz): def main
- Acme::Shop::Report (Report.tz): def print_total

## Dependencies
- Acme::Shop::Main -> Acme::Shop::Cart, Acme::Shop::Report
- Acme::Shop::Report -> Acme::Shop::Cart

## Tests
- Acme::Shop::Cart: 2
- Acme::Shop::Main: 1

## Effects (functions)
- io: 2

next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl ('::' is '.' in file names)
```

`brief/L1/Cart.md`:

```text
# Acme::Shop::Cart (Cart.tz)
3 items, 3 public | 2 tests

## Types
- record Line {sku, quantity, unit_price}

## Functions
- def line_total :: ref Line -> i64 | callers 2
- def total :: ref [Line] -> i64 | callers 3

## Tests
- line total
- empty cart

used by: Acme::Shop::Main, Acme::Shop::Report
```

`brief/L1/Main.md`:

```text
# Acme::Shop::Main (Main.tz)
1 item, 1 public | 1 test

## Functions
- def main :: unit -> i32 | effects io

## Tests
- main runs

uses: Acme::Shop::Cart, Acme::Shop::Report
```

`brief/L1/Report.md`:

```text
# Acme::Shop::Report (Report.tz)
1 item, 1 public | 0 tests

## Functions
- def print_total :: ref [Line] -> unit | callers 1 | effects io

uses: Acme::Shop::Cart
used by: Acme::Shop::Main
```

`brief/L2/Acme.Shop.Cart.Line.md`（ID の `used by` は 7 件のうち先頭 5 件と `…2 more`）:

```text
Acme::Shop::Cart.Line
record | public | Cart.tz:4
sig: record Line
fields: sku: string, quantity: i64, unit_price: i64
doc: One line of a cart: a product, how many, and its price in cents.
used by (7): Acme::Shop::Cart.line_total, Acme::Shop::Cart.total, Acme::Shop::Main.main, Acme::Shop::Report.print_total, test:Acme::Shop::Cart.empty cart, …2 more
```

`brief/L2/Acme.Shop.Cart.line_total.md`:

```text
Acme::Shop::Cart.line_total
def | public | Cart.tz:7
sig: def line_total :: ref Line -> i64
params: line:ref
doc: The price of one line in cents.
callers (2): Acme::Shop::Cart.total, test:Acme::Shop::Cart.line total
tests (3): test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Cart.line total, test:Acme::Shop::Main.main runs
```

`brief/L2/Acme.Shop.Cart.total.md`（`sites: trap 1` は `bounds_check` の 1 件。`proven: false`）:

```text
Acme::Shop::Cart.total
def | public | Cart.tz:10
sig: def total :: ref [Line] -> i64
params: lines:ref
doc: The price of all lines in cents.
sites: trap 1
callees (1): Acme::Shop::Cart.line_total
callers (3): Acme::Shop::Report.print_total, test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs
tests (2): test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs
```

`brief/L2/Acme.Shop.Main.main.md`:

```text
Acme::Shop::Main.main
def | public | Main.tz:3
sig: def main :: unit -> i32
params: arg1:copy
effects: io
callees (1): Acme::Shop::Report.print_total
```

`brief/L2/Acme.Shop.Report.print_total.md`:

```text
Acme::Shop::Report.print_total
def | public | Report.tz:5
sig: def print_total :: ref [Line] -> unit
params: lines:ref
effects: io
callees (2): Acme::Shop::Cart.total, IO.write_line
callers (1): Acme::Shop::Main.main
```

`facts` が無い KB（G26 の Phase 3 より前）では、`Effects` のブロック・`| effects io`・`effects:`・`sites:` の行が消え、それ以外は同じ（L0 は 577 字、`Report.print_total` の L2 は 205 字になる）。

## 付録 E: 切り詰めの手計算

### E-1: L2 `Acme::Shop::Cart.total`（付録 D-6）を小さな上限で作る

全文は 384 字。行ごとの字数（改行を含む）: 23（ID）＋26（kind の行）＋36（`sig:`）＋18（`params:`）＋38（`doc:`）＋14（`sites:`）＋41（`callees`）＋111（`callers`）＋77（`tests`）＝ 384。優先度 0 は上の 3 行（85 字）、残りの 6 行が落とせる（`params` 1・`doc` 2・`sites` 2・`callees` 3・`callers` 3・`tests` 3）。落とす順は (優先度, ブロックの位置, 行の位置) が最大のもの、つまり優先度 3 の行を後ろから。

| 上限 | 手順 | 結果 |
| --- | --- | --- |
| 700 | 384 ≤ 700。何も落とさない | 384 字 |
| 330 | 384 > 330。`tests` を落とす: 384 − 77 ＋ 8（`…1 more` の行） ＝ 315 ≤ 330 | 315 字。最後の行が `…1 more` |
| 280 | 315 > 280。次に `callers` を落とす: 315 − 111 ＝ 204 ≤ 280（`…1 more` が `…2 more` に変わるだけで字数は同じ） | 204 字。`callees` までが残り、`…2 more` |
| 200 | 204 > 200。次に `callees` を落とす: 204 − 41 ＝ 163 ≤ 200 | 163 字。`sites:` までが残り、`…3 more` |
| 100 | 落とせる 6 行をすべて落とす: 85 ＋ 8 ＝ 93 ≤ 100 | 93 字（必須の 3 行と `…6 more`） |
| 40 | 必須の行も収まらない。先頭 39 字と `…` に切る（末尾の改行なし） | `'Acme::Shop::Cart.total\ndef | public | C…'`（40 字） |

上限 330 の結果:

```text
Acme::Shop::Cart.total
def | public | Cart.tz:10
sig: def total :: ref [Line] -> i64
params: lines:ref
doc: The price of all lines in cents.
sites: trap 1
callees (1): Acme::Shop::Cart.line_total
callers (3): Acme::Shop::Report.print_total, test:Acme::Shop::Cart.empty cart, test:Acme::Shop::Main.main runs
…1 more
```

上限 280 の結果:

```text
Acme::Shop::Cart.total
def | public | Cart.tz:10
sig: def total :: ref [Line] -> i64
params: lines:ref
doc: The price of all lines in cents.
sites: trap 1
callees (1): Acme::Shop::Cart.line_total
…2 more
```

上限 200 の結果:

```text
Acme::Shop::Cart.total
def | public | Cart.tz:10
sig: def total :: ref [Line] -> i64
params: lines:ref
doc: The price of all lines in cents.
sites: trap 1
…3 more
```

### E-2: 30 モジュールの L0（2,394 字 / 上限 2,400）

入力は合成プロジェクト `syn`（付録 F の `syn_rows.py`。付録 A-1 の `gen_project.py 30` と同じ構造）: モジュール `M0000`〜`M0029`、各モジュールに `record Item…`（フィールド 2）・公開の `def f0_…`〜`f3_…`（`f0_…` が `export`）・非公開の `def private…`・`test` 1 つ（1 モジュール 10 行で `symbols 300`）、`M{i}.f{j}_…` から `M{i-1}.f{j}_…` への `calls` 4 本（`i ≥ 1`。計 116 本）、依存は `M0001 -> M0000` …の 29 本。ブロックの優先度は `spread`（5 行ごとに 1 下げる）で、Modules は 1〜6、Exports は 2〜7、Dependencies（29 行）は 3〜8、Tests は 4〜9。全部を並べると 4,272 字なので、優先度の大きい行から落とし、上限に収まった時点で止める。残る行数は Modules 20（優先度 4 まで）・Exports 15（優先度 4 まで）・Dependencies 7（優先度 4 まで）・Tests 0。

```text
# syn
tz-kb/1 | edition 2026 | modules 30 | symbols 300 | tests 30
status: ok
entry: none (library)

## Modules
- M0000 (M0000.tz): record Item0000; def f0_0000, f1_0000, f2_0000, f3_0000
- M0001 (M0001.tz): record Item0001; def f0_0001, f1_0001, f2_0001, f3_0001
- M0002 (M0002.tz): record Item0002; def f0_0002, f1_0002, f2_0002, f3_0002
- M0003 (M0003.tz): record Item0003; def f0_0003, f1_0003, f2_0003, f3_0003
- M0004 (M0004.tz): record Item0004; def f0_0004, f1_0004, f2_0004, f3_0004
- M0005 (M0005.tz): record Item0005; def f0_0005, f1_0005, f2_0005, f3_0005
- M0006 (M0006.tz): record Item0006; def f0_0006, f1_0006, f2_0006, f3_0006
- M0007 (M0007.tz): record Item0007; def f0_0007, f1_0007, f2_0007, f3_0007
- M0008 (M0008.tz): record Item0008; def f0_0008, f1_0008, f2_0008, f3_0008
- M0009 (M0009.tz): record Item0009; def f0_0009, f1_0009, f2_0009, f3_0009
- M0010 (M0010.tz): record Item0010; def f0_0010, f1_0010, f2_0010, f3_0010
- M0011 (M0011.tz): record Item0011; def f0_0011, f1_0011, f2_0011, f3_0011
- M0012 (M0012.tz): record Item0012; def f0_0012, f1_0012, f2_0012, f3_0012
- M0013 (M0013.tz): record Item0013; def f0_0013, f1_0013, f2_0013, f3_0013
- M0014 (M0014.tz): record Item0014; def f0_0014, f1_0014, f2_0014, f3_0014
- M0015 (M0015.tz): record Item0015; def f0_0015, f1_0015, f2_0015, f3_0015
- M0016 (M0016.tz): record Item0016; def f0_0016, f1_0016, f2_0016, f3_0016
- M0017 (M0017.tz): record Item0017; def f0_0017, f1_0017, f2_0017, f3_0017
- M0018 (M0018.tz): record Item0018; def f0_0018, f1_0018, f2_0018, f3_0018
- M0019 (M0019.tz): record Item0019; def f0_0019, f1_0019, f2_0019, f3_0019
…10 more

## Exports
- M0000.f0_0000 :: i64 -> i64
- M0001.f0_0001 :: i64 -> i64
- M0002.f0_0002 :: i64 -> i64
- M0003.f0_0003 :: i64 -> i64
- M0004.f0_0004 :: i64 -> i64
- M0005.f0_0005 :: i64 -> i64
- M0006.f0_0006 :: i64 -> i64
- M0007.f0_0007 :: i64 -> i64
- M0008.f0_0008 :: i64 -> i64
- M0009.f0_0009 :: i64 -> i64
- M0010.f0_0010 :: i64 -> i64
- M0011.f0_0011 :: i64 -> i64
- M0012.f0_0012 :: i64 -> i64
- M0013.f0_0013 :: i64 -> i64
- M0014.f0_0014 :: i64 -> i64
…15 more

## Dependencies
- M0001 -> M0000
- M0002 -> M0001
- M0003 -> M0002
- M0004 -> M0003
- M0005 -> M0004
- M0006 -> M0005
- M0007 -> M0006
…22 more

## Tests
…30 more

next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl ('::' is '.' in file names)
```

字数の手計算: 行の字数の和 ＝ 2,394（`dump` の各行の字数＋改行 1 の合計。付録 F の `l0` が `len(text)` として返す値）。この例の読み方: Tests のブロックは行がすべて落ちても見出しと `…30 more` が残り、「テストが 30 個ある」ことが分かる。

### E-3: 1,000 モジュールの L0（2,388 字 / 上限 2,400）

同じ合成プロジェクトを 1,000 モジュールにしたもの（`symbols 10000`・`tests 1000`・依存 999 本）。残る行は Modules 20（`…980 more`）・Exports 15（`…985 more`）・Dependencies 6（`…993 more`）・Tests 0（`…1000 more`）で、2,388 字。テスト `l0_fits_for_1000_modules` はこの 4 つの `…N more` と 2,388 字を確かめる。`spread` が無い（ブロックの優先度だけ）と、Exports・Dependencies・Tests の行が全部落ちて Modules が 27 行だけ残る（`…973 more`・`…1000 more`・`…999 more`・`…1000 more`。2,369 字）ので、`spread` を入れた（決定 D13）。

### E-4: 優先度 0 の行の最大（`mandatory_lines_fit_every_limit`）

名前を 64 字、ID を 300 字とした最悪の入力でも、優先度 0 の行は上限に収まる。L2: ID 行 301 ＋ kind の行（`method | public export pure | ` ＋ clip 80 字の path ＋ `:` ＋ 行番号 10 桁 ＋ 改行 ＝ 122）＋ `sig:` の行（5 ＋ clip 120 ＋ 改行 ＝ 126）＝ 549 ≤ 700。L1: 見出しの行（`# ` ＋ ID 300 ＋ ` (` ＋ path 80 ＋ `)` ＋ 改行 ＝ 386）＋ 件数の行（約 60）≒ 446 ≤ 1,600。L0: 題名 100 ＋ 統計 100 ＋ 状態 40 ＋ `entry: ` ＋ ID 300 ＝ 308 ＋ `next:` 118 ≒ 670 ≤ 2,400。収まらない入力（ID が 1,000 字の場合）は規則 3（先頭 `上限 − 1` 字と `…`）になる。

## 付録 F: 参照実装（Python 3.9 以上）

B〜E のカードと D の `manifest.json` を作った実装。Rust の `brief.rs`・`manifest.rs`・`write.rs` は、これと**同じ出力**を返す（`cards_match_the_golden_files` が B〜D のカードと照合する）。行は D の JSON Lines を `json.loads` で読んだ辞書（キーは G26 の仕様のとおり）。`Kb(project, version, edition, symbols, graph, facts, warnings)` の `graph` は辺の辞書の一覧（無ければ `None`）、`facts` は `id` → 行（無ければ `None`）。

### F-1: `cards.py`（カードの規則）

```python
# Reference implementation of the brief-card rules of G27 (Phase 2). Python 3.9+.
ELLIPSIS = "\u2026"
LIMITS = {"L0": 2400, "L1": 1600, "L2": 700}
TOP_KINDS = ("record", "union", "class", "alias", "handle", "const", "def", "extern", "instance")
TYPE_KINDS = ("record", "union", "class", "alias", "handle")
FILE_KINDS = ("def", "const", "record", "union", "class", "method", "alias", "extern", "handle")
DEP_RELS = ("calls", "uses_type", "implements", "instance_of", "imports")
MEMBER_KIND = {"record": "field", "union": "case", "class": "method"}
NEXT = ("next: brief/L1/<key>.md, brief/L2/<id>.md, symbols.jsonl, graph.jsonl, facts/<key>.jsonl "
        "('::' is '.' in file names)")

def bytes_key(text): return text.encode("utf-8")
def norm(text): return " ".join(text.split())
def clip(text, limit): return text if len(text) <= limit else text[: limit - 1] + ELLIPSIS
def plural(n, one, many): return f"{n} {one if n == 1 else many}"
def after_colons(signature):
    i = signature.find(" :: ")
    return signature[i + 4:] if i >= 0 else signature
def first_line(doc):
    for line in (doc or "").splitlines():
        if line.strip(): return clip(norm(line), 100)
    return ""

class Card:
    """Blocks of (text, priority) lines. Priority 0 lines are never dropped; a larger number is dropped earlier."""
    def __init__(self, limit, gap="\n\n"): self.limit, self.gap, self.blocks = limit, gap, []
    def block(self, header, lines, spread=False):
        """spread: the line at position i of a list block with base priority p > 0 gets priority p + i // 5."""
        if spread: lines = [(text, p if p == 0 else p + i // 5) for i, (text, p) in enumerate(lines)]
        self.blocks.append((header, lines))
    def layout(self, dropped):
        parts = []
        for bi, (header, lines) in enumerate(self.blocks):
            if not lines: continue
            part = [header] if header is not None else []
            part += [text for li, (text, _) in enumerate(lines) if (bi, li) not in dropped]
            count = sum(1 for li in range(len(lines)) if (bi, li) in dropped)
            if count: part.append(f"{ELLIPSIS}{count} more")
            parts.append("\n".join(part))
        return self.gap.join(parts) + "\n"
    def render(self):
        dropped = set()
        while True:
            text = self.layout(dropped)
            if len(text) <= self.limit: return text
            candidates = [(priority, bi, li) for bi, (_, lines) in enumerate(self.blocks)
                          for li, (_, priority) in enumerate(lines) if priority > 0 and (bi, li) not in dropped]
            if not candidates: return text[: self.limit - 1] + ELLIPSIS
            dropped.add(max(candidates)[1:])

class Kb:
    def __init__(self, project, version, edition, symbols, graph=None, facts=None, warnings=0):
        self.project, self.version, self.edition, self.warnings = project, version, edition, warnings
        self.symbols = sorted(symbols, key=lambda r: bytes_key(r["id"]))
        self.graph, self.facts = graph, facts
        self.by_id = {r["id"]: r for r in self.symbols}
    def modules(self): return [r for r in self.symbols if r["kind"] == "module"]
    def rows_of(self, module_id): return [r for r in self.symbols if r["module"] == module_id and r["kind"] != "module" and not r["name"].startswith("$")]
    def children(self, parent_id, kind):
        prefix = parent_id + "."
        return sorted((r for r in self.symbols if r["kind"] == kind and r["id"].startswith(prefix) and "." not in r["id"][len(prefix):]),
                      key=lambda r: (r["span"]["start"], bytes_key(r["id"])))
    def count_of(self, rel, key, other, sid): 
        return sorted({e[other] for e in self.graph if e["rel"] == rel and e[key] == sid}, key=bytes_key)

def declaration_order(rows): return sorted(rows, key=lambda r: (r["span"]["start"], bytes_key(r["id"])))
def public_top(kb, module_id): return [r for r in kb.rows_of(module_id) if r["public"] and r["kind"] in TOP_KINDS]

def module_dependencies(kb):
    result = {}
    for e in kb.graph:
        if e["rel"] not in DEP_RELS: continue
        a, b = kb.by_id.get(e["from"]), kb.by_id.get(e["to"])
        if a is None or b is None or a["module"] == b["module"]: continue
        result.setdefault(a["module"], set()).add(b["module"])
    return result

def module_summary(kb, module_id):
    rows = public_top(kb, module_id)
    groups = []
    for kind in TOP_KINDS:
        names = [clip(r["name"], 32) for r in declaration_order(rows) if r["kind"] == kind]
        if names: groups.append(f"{kind} {', '.join(names)}")
    return clip("; ".join(groups), 100) if groups else "(no public items)"

def l0(kb, limit=None):
    card = Card(limit or LIMITS["L0"])
    modules = kb.modules()
    tests = [r for r in kb.symbols if r["kind"] == "test"]
    benches = [r for r in kb.symbols if r["kind"] == "bench"]
    title = "# " + clip(kb.project, 64) + (f" {clip(kb.version, 32)}" if kb.version else "")
    stats = f"tz-kb/1 | edition {kb.edition} | modules {len(modules)} | symbols {len(kb.symbols)} | tests {len(tests)}"
    if benches: stats += f" | benches {len(benches)}"
    status = "status: ok" + (f", {plural(kb.warnings, 'warning', 'warnings')}" if kb.warnings else "")
    main = next((r for r in kb.symbols if r["kind"] == "def" and r["name"] == "main"
                 and kb.by_id[r["module"]]["name"] == "Main"), None)
    top_level = next((r for r in kb.symbols if r["name"] == "$entry"), None)
    entry = (f"entry: {main['id']}" if main else
             f"entry: top-level code of {top_level['module']}" if top_level else "entry: none (library)")
    card.block(None, [(title, 0), (stats, 0), (status, 0), (entry, 0)])
    ordered = sorted(modules, key=lambda m: (-len(public_top(kb, m["id"])), bytes_key(m["id"])))
    card.block("## Modules", [(f"- {m['id']} ({clip(m['span']['path'], 80)}): {module_summary(kb, m['id'])}", 1) for m in ordered], spread=True)
    exports = [r for r in kb.symbols if r["export"] and r["kind"] in ("def", "const")]
    card.block("## Exports", [(f"- {r['id']} :: {clip(after_colons(norm(r['signature'])), 120)}", 2) for r in exports], spread=True)
    if kb.graph is not None:
        dependencies = module_dependencies(kb)
        card.block("## Dependencies", [(clip(f"- {m} -> {', '.join(sorted(ds, key=bytes_key))}", 200), 3)
                                       for m, ds in sorted(dependencies.items(), key=lambda kv: bytes_key(kv[0]))], spread=True)
    per_module = {}
    for t in tests: per_module[t["module"]] = per_module.get(t["module"], 0) + 1
    card.block("## Tests", [(f"- {m}: {n}", 4) for m, n in sorted(per_module.items(), key=lambda kv: (-kv[1], bytes_key(kv[0])))], spread=True)
    if kb.facts is not None:
        counts = {}
        for r in kb.symbols:
            if r["kind"] in ("def", "method") and r["id"] in kb.facts:
                for effect in kb.facts[r["id"]].get("effects", []): counts[effect] = counts.get(effect, 0) + 1
        card.block("## Effects (functions)", [(f"- {e}: {n}", 5) for e, n in sorted(counts.items())], spread=True)
    card.block(None, [(NEXT, 0)])
    return card.render()

def callers_of(kb, sid): return len({e["from"] for e in kb.graph if e["rel"] == "calls" and e["to"] == sid})

def l1(kb, module_id, limit=None):
    module = kb.by_id[module_id]
    card = Card(limit or LIMITS["L1"])
    rows = kb.rows_of(module_id)
    top = [r for r in rows if r["kind"] in TOP_KINDS]
    tests = [r for r in rows if r["kind"] in ("test", "bench")]
    card.block(None, [(f"# {module_id} ({clip(module['span']['path'], 80)})", 0),
                      (f"{plural(len(top), 'item', 'items')}, {sum(1 for r in top if r['public'])} public | "
                       f"{plural(sum(1 for r in tests if r['kind'] == 'test'), 'test', 'tests')}", 0)])
    def ordered(rs): return sorted(rs, key=lambda r: (not r["public"], r["span"]["start"], bytes_key(r["id"])))
    def priority(r): return 1 if r["public"] else 4
    def text(r):
        flag = "" if r["public"] else " [private]"
        if r["kind"] in MEMBER_KIND:
            names = ", ".join(k["name"] for k in kb.children(r["id"], MEMBER_KIND[r["kind"]]))
            return f"{r['kind']} {r['name']} {{{names}}}{flag}"
        line = clip(norm(r["signature"]), 140) + flag
        if kb.graph is not None and callers_of(kb, r["id"]): line += f" | callers {callers_of(kb, r['id'])}"
        fact = kb.facts.get(r["id"]) if kb.facts is not None else None
        if fact and fact.get("effects"): line += " | effects " + ",".join(fact["effects"])
        return line
    card.block("## Types", [(f"- {text(r)}", priority(r)) for r in ordered(r for r in top if r["kind"] in TYPE_KINDS)], spread=True)
    card.block("## Functions", [(f"- {text(r)}", priority(r)) for r in ordered(r for r in top if r["kind"] in ("const", "def", "extern"))], spread=True)
    card.block("## Instances", [(f"- instance {r['name']}", 2) for r in ordered(r for r in top if r["kind"] == "instance")], spread=True)
    card.block("## Tests", [(f"- {'bench ' if t['kind'] == 'bench' else ''}{t['name']}", 3) for t in declaration_order(tests)], spread=True)
    if kb.graph is not None:
        dependencies = module_dependencies(kb)
        uses = sorted(dependencies.get(module_id, ()), key=bytes_key)
        used_by = sorted((m for m, ds in dependencies.items() if module_id in ds), key=bytes_key)
        lines = []
        if uses: lines.append((clip("uses: " + ", ".join(uses), 200), 3))
        if used_by: lines.append((clip("used by: " + ", ".join(used_by), 200), 3))
        card.block(None, lines)
    return card.render()

def listing(label, items):
    shown = ", ".join(items[:5]) + (f", {ELLIPSIS}{len(items) - 5} more" if len(items) > 5 else "")
    return f"{label} ({len(items)}): {shown}"

def l2(kb, sid, limit=None):
    r = kb.by_id[sid]
    card = Card(limit or LIMITS["L2"])
    flags = ["public" if r["public"] else "private"] + (["export"] if r["export"] else []) + (["pure"] if r.get("pure") else [])
    lines = [(r["id"], 0), (f"{r['kind']} | {' '.join(flags)} | {clip(r['span']['path'], 80)}:{r['span']['line']}", 0),
             ("sig: " + clip(norm(r["signature"]), 120), 0)]
    if r["kind"] in ("def", "extern", "method", "const") and r.get("params"):
        lines.append(("params: " + ", ".join(f"{p['name']}:{p['conv']}" if p.get("name") else p["conv"] for p in r["params"]), 1))
    if r["kind"] in MEMBER_KIND:
        kids = kb.children(r["id"], MEMBER_KIND[r["kind"]])
        if kids:
            label = {"record": "fields", "union": "cases", "class": "methods"}[r["kind"]]
            lines.append((f"{label}: " + ", ".join(k["name"] if r["kind"] == "class" else norm(k["signature"]) for k in kids), 1))
    if first_line(r.get("doc")): lines.append(("doc: " + first_line(r["doc"]), 2))
    if r.get("deprecated"): lines.append(("deprecated: " + clip(norm(r["deprecated"]), 100), 1))
    fact = kb.facts.get(sid) if kb.facts is not None else None
    if fact:
        if fact.get("effects"): lines.append(("effects: " + ", ".join(fact["effects"]), 1))
        sites = [f"{name} {n}" for name, n in ((k, sum(1 for s in fact.get(k + "_sites", []) if not s.get("proven"))) for k in ("trap", "alloc", "copy")) if n]
        if sites: lines.append(("sites: " + ", ".join(sites), 2))
        if fact.get("recursion", "none") != "none": lines.append(("recursion: " + fact["recursion"], 2))
    if kb.graph is not None:
        pairs = []
        if r["kind"] in TYPE_KINDS:
            used = sorted({e["from"] for e in kb.graph if e["rel"] in ("uses_type", "implements", "instance_of") and e["to"] == sid}, key=bytes_key)
            pairs.append(("used by", used))
        else:
            pairs += [("callees", kb.count_of("calls", "from", "to", sid)), ("callers", kb.count_of("calls", "to", "from", sid)),
                      ("tests", kb.count_of("tested_by", "from", "to", sid))]
        lines += [(listing(label, items), 3) for label, items in pairs if items]
    card.block(None, lines)
    return card.render()

def l2_file_ids(kb):
    rows = [r for r in kb.symbols if r["kind"] in FILE_KINDS and not r["name"].startswith("$")]
    rank = lambda r: 0 if r["export"] else (1 if r["public"] else 2)
    return [r["id"] for r in sorted(rows, key=lambda r: (rank(r), bytes_key(r["id"])))[:256]]
```

### F-2: `kbwriter.py`（ファイル名・`source_hash`・`manifest.json`）

`cache::Sha256::field` は「タグの長さ（`u64` の little endian）・タグ・値の長さ・値」を足す。ここでは `hashlib.sha256` で同じ手順をとる。

```python
import hashlib, json, struct, re
from cards import *

def field(h, tag, data):
    h.update(struct.pack("<Q", len(tag.encode()))); h.update(tag.encode())
    h.update(struct.pack("<Q", len(data))); h.update(data)

def source_hash(inputs):
    """inputs: list of (package, path, kind, text) with kind 'source' or 'manifest'."""
    h = hashlib.sha256()
    field(h, "tz-kb-input", struct.pack("<I", 1))
    for package, path, kind, text in sorted(inputs, key=lambda t: (t[0].encode(), t[1].encode())):
        field(h, "package", package.encode()); field(h, "path", path.encode()); field(h, kind, text)
    return h.hexdigest()

def jstr(s): return json.dumps(s, ensure_ascii=False)
def jsonl(rows): return "".join(json.dumps(r, ensure_ascii=False, separators=(",", ":")) + "\n" for r in rows).encode()

def stem(text):
    out = []
    for part in text.split("::"):
        out.append("".join(c if re.fullmatch(r"[A-Za-z0-9_.]", c) else "".join(f"%{b:02X}" for b in c.encode()) for c in part))
    return ".".join(out)

def unique_names(ids):
    seen, names = {}, {}
    for i in sorted(ids, key=bytes_key):
        s = stem(i); low = s.lower()
        n = seen.get(low, 0); seen[low] = n + 1
        names[i] = s if n == 0 else f"{s}~{n}"
    return names

def manifest_text(kb, files, inputs_hash, modules, counts, complete=True):
    lines = ["{", '  "schema": "tz-kb/1",', '  "compiler": "tsuzuri 0.1.0",', f'  "edition": {jstr(kb.edition)},', f'  "project": {jstr(kb.project)},']
    if kb.version: lines.append(f'  "version": {jstr(kb.version)},')
    lines += [f'  "complete": {"true" if complete else "false"},', f'  "source_hash": "{inputs_hash}",', '  "counts": {']
    items = list(counts.items())
    for i, (k, v) in enumerate(items): lines.append(f'    "{k}": {v}' + ("," if i + 1 < len(items) else ""))
    lines.append("  },"); lines.append('  "modules": [')
    for i, m in enumerate(modules):
        lines.append("    " + json.dumps(m, ensure_ascii=False, separators=(",", ":")) + ("," if i + 1 < len(modules) else ""))
    lines.append("  ],"); lines.append('  "files": {')
    keys = sorted(files, key=bytes_key)
    for i, k in enumerate(keys): lines.append(f'    {jstr(k)}: "{hashlib.sha256(files[k]).hexdigest()}"' + ("," if i + 1 < len(keys) else ""))
    lines.append("  }"); lines.append("}")
    return "\n".join(lines) + "\n"
```

`manifest_text` の `files` は `manifest.json` と `.tsuzuri-kb` を除く全ファイル（相対パス → byte 列）。`unique_names` は同じディレクトリのファイル名の幹（小文字にして同じ名前が 2 つ以上なら、byte 順の 2 つ目以降に `~1`・`~2` を足す）。`stem` は `::` を `.` にし、`[A-Za-z0-9_.]` 以外を UTF-8 の byte ごとに `%XX` にする（`instance:Add<Main.Point>.add` → `instance%3AAdd%3CMain.Point%3E.add`）。

### F-3: `syn_rows.py`（付録 E の合成プロジェクト）

```python
from cards import *
def syn(count):
    syms, graph, facts = [], [], {}
    for i in range(count):
        m = f"M{i:04d}"
        path = m + ".tz"
        top = {"path": path, "start": 0, "end": 0, "line": 1, "column": 1}
        def row(id, kind, name, signature, line, start, public=True, export=False, params=None):
            return {"id": id, "kind": kind, "module": m, "name": name, "signature": signature, "params": params or [], "result": None,
                    "public": public, "export": export, "pure": False, "doc": None, "deprecated": None, "origin": "project",
                    "span": {"path": path, "start": start, "end": start + len(name), "line": line, "column": 1}}
        syms.append({"id": m, "kind": "module", "module": m, "name": m, "signature": "module " + m, "params": [], "result": None, "public": True,
                     "export": False, "pure": False, "doc": None, "deprecated": None, "origin": "project", "span": top})
        syms.append(row(f"{m}.Item{i:04d}", "record", f"Item{i:04d}", f"record Item{i:04d}", 1, 7))
        for f, t in (("id", "i64"), ("weight", "i64")):
            syms.append(row(f"{m}.Item{i:04d}.{f}", "field", f, f"{f}: {t}", 1, 20 + len(f)))
        for j in range(4):
            name = f"f{j}_{i:04d}"
            syms.append(row(f"{m}.{name}", "def", name, f"{'export ' if j == 0 else ''}def {name} :: i64 -> i64", 3 + j, 100 + j * 50, export=(j == 0)))
            if i > 0: graph.append({"rel": "calls", "from": f"{m}.{name}", "to": f"M{i-1:04d}.f{j}_{i-1:04d}", "args": ["copy"],
                                    "span": {"path": path, "start": 100 + j * 50, "end": 110 + j * 50, "line": 3 + j, "column": 40}})
        syms.append(row(f"{m}.private{i:04d}", "def", f"private{i:04d}", f"def private{i:04d} :: ref Item{i:04d} -> i64", 8, 400, public=False))
        syms.append({"id": f"test:{m}.f0 of {m}", "kind": "test", "module": m, "name": f"f0 of {m}", "signature": f'test "f0 of {m}"', "params": [],
                     "result": None, "public": False, "export": False, "pure": False, "doc": None, "deprecated": None, "origin": "project",
                     "span": {"path": path, "start": 500, "end": 510, "line": 10, "column": 6}})
    return Kb("syn", None, "2026", syms, graph, {})
```

使い方: `l0(syn(30))`（2,394 字）、`l0(syn(1000))`（2,388 字）。B・C の `l0`・`l1`・`l2` は、G26 の golden の JSON Lines を `Kb("point", None, "2026", symbols, graph, None)` のように渡す。

## 付録 G: 計測の手順と増分の鍵

### G-1: 着手の条件（手順 11 の最初）

```text
python3 gen_project.py 200  target/tz-G27-work/syn200      # 付録 A-1 の生成器
python3 gen_project.py 1000 target/tz-G27-work/syn1000
TSUZURI_TIME_PASSES=1 target/release/tsuzuri kb build target/tz-G27-work/syn200 --force 2>&1 >/dev/null | tail -1
```

最後の 1 行は `{"time_passes":1,"phases":[{"name":"load","ms":…},…],"total_ms":…}` で、同じ名前の区間は足し合わされる。`kb-model`・`kb-write`・`kb-facts`・`kb-brief`・`kb-publish` を 9 回の中央値で読む。**判定**: `kb-facts ÷ (kb-model ＋ kb-write ＋ kb-brief ＋ kb-publish) ≥ 0.10`（`kb-facts` は `kb-write` に入れ子なので分母に足さない）を、200 か 1,000 モジュールのどちらかで満たせば着手する。満たさなければ Phase 3 は出荷せず、数値を手順 16 の `docs/benchmarks.md` に書く（停止条件 8 の B）。`kb build --force` は新鮮でも作り直し、`--force` なしの 2 回目は何も書かない。

### G-2: 増分の効果（手順 13）

各入力（`syn1`・`syn200`・`syn1000`）で、次の 5 つを 9 回の中央値で測り（`median_ms.py`）、`docs/benchmarks.md` の節 `## KB の生成と増分（G27）` に表で書く（測定の条件: 機械・`rustc`・コミット・負荷平均・コマンド。閾値は置かない）。

| 列 | コマンド |
| --- | --- |
| `check`（KB なし） | `tsuzuri check P --no-kb` |
| `check`（KB を作る） | `rm -rf P/target; tsuzuri check P` |
| `check`（新鮮） | `tsuzuri check P`（2 回目） |
| `kb build --force` | `tsuzuri kb build P --force` |
| 1 モジュールを編集した後の `kb build` | `printf '\n// edit\n' >> P/M0500.tz` の後、`tsuzuri kb build P`（`--json` の `reused` も書く） |

### G-3: 増分の鍵（`fwd`・`rev`・`world`）と無効化の表

「モジュール」は key で数える。辺は、`graph.jsonl` の行で `from` と `to` が `symbols.jsonl` の行にあり、その `module`（ID）が違うものを、manifest の `modules` で key に直したもの。

```text
world  = SHA-256( 全 user モジュールを name の byte 順に: field("module", name), field("interface", interface) )
fwd(M) = M から届くモジュール（M を除く）。辺 = `calls`・`implements`・`instance_of`。
         class の `method` 行への `calls` は、その class の instance（`implements`・`instance_of` で class に結びつく行）の module にも届く
rev(M) = M に届くモジュール（逆向き）。M が「ジェネリック」のときだけ使う。
         ジェネリック = M の行（def・method・class・instance・record・union・alias）の `signature` に ' と英字が続く部分がある
K(M)   = SHA-256( field("tz-kb-facts", 1u32), field("stamp", stamp), field("edition", edition),
                  field("source", M.source), field("world", world),
                  D ∈ fwd(M) を name の byte 順に: field("callee", D.name), field("callee-source", D.source),
                  ジェネリックなら C ∈ rev(M) を name の byte 順に: field("caller", C.name), field("caller-source", C.source) )
```

`fwd`・`rev` はモジュールごとの幅優先探索で、サイクルのモジュールは互いの `fwd` に入る。4,096 モジュールを超えたら再利用しない。根拠: M の facts は、(1) M の本文、(2) 他の全モジュールの interface（修飾名と instance は大域。`FrontendDelta::must_recheck` と同じ規則。定数の初期化式・`@deprecated` の説明も interface に入る）、(3) 呼び先の**本文**（効果は呼び先の本文から決まる。interface だけでは足りない）、(4) ジェネリックなら呼び出し側の使い方（特殊化の数）で決まる。

| 変更 | 作り直すモジュール |
| --- | --- |
| 関数 `B.f` の本文だけ（コメントを含む。署名・型・定数は同じ） | `B`、`fwd` に `B` を含むモジュール（呼ぶ側すべて）、`B` がジェネリックなら `rev(B)` も |
| 署名・型・instance・定数の初期化式・`@deprecated`（公開か否かを問わない）の変更、モジュールの追加・削除 | **全モジュール**（`world` が変わる） |
| `Tsuzuri.toml` の `edition`、コンパイラの更新（stamp） | 全モジュール |
| 依存パッケージのソース | そのモジュールの変更として上の 2 行のどちらか |
| どのモジュールにも届かないモジュールの本文 | そのモジュールだけ |
| テストの追加・変更 | そのモジュールと、そのモジュールを `fwd` に含むモジュール |

## 付録 H: Rust の型の全フィールドと `emit.rs`

`serde` の derive は無い（依存は `serde_json` と `rustc_apfloat` だけ）。行は `serde_json::Value` から取り出す。

### H-1: 型

```rust
// src/kb/settings.rs
#[derive(Clone, Debug, Default)]
pub struct KbFlags { pub kb: bool, pub no_kb: bool, pub dir: Option<PathBuf> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KbMode { Default, On, Off }
#[derive(Clone, Debug)]
pub struct KbSettings { pub mode: KbMode, pub dir: Option<PathBuf> }

// src/kb/manifest.rs
pub struct Manifest {
    pub schema: String, pub compiler: String, pub edition: String, pub project: String, pub version: Option<String>,
    pub complete: bool, pub source_hash: String,
    pub counts: Vec<(&'static str, u64)>,            // §3 の順。失敗した解析では errors・warnings だけ
    pub modules: Vec<ModuleEntry>,                    // name の byte 順
    pub files: BTreeMap<String, String>,              // 相対パス → SHA-256（16 進）
}
pub struct ModuleEntry { pub name: String, pub key: String, pub origin: String, pub path: String, pub source: String }

// src/kb/write.rs
#[derive(Default)]
pub struct Files(pub BTreeMap<String, Vec<u8>>);
impl Files { pub fn put(&mut self, path: impl Into<String>, bytes: Vec<u8>); }

// src/kb/store.rs（Phase 2）。キーは G26 の仕様のまま。知らないキーは無視し、無いキーは既定値（空・None・false）にする
pub struct Span { pub path: String, pub start: usize, pub end: usize, pub line: usize, pub column: usize }
pub struct Param { pub name: String, pub ty: String, pub conv: String }                   // JSON のキーは "type"
pub struct Symbol { pub id: String, pub kind: String, pub module: String, pub name: String, pub signature: String,
    pub params: Vec<Param>, pub result: Option<String>, pub public: bool, pub export: bool, pub pure: bool,
    pub doc: Option<String>, pub deprecated: Option<String>, pub origin: String, pub span: Span }
pub struct Edge { pub rel: String, pub from: String, pub to: String, pub args: Option<Vec<String>>, pub span: Span }
pub struct TestRow { pub id: String, pub kind: String, pub module: String, pub name: String, pub index: usize, pub span: Span }
pub struct Site { pub kind: String, pub proven: bool, pub span: Span }                   // trap・alloc・copy の site（無いキーは false・空）
pub struct EffectSite { pub effect: String, pub span: Span, pub via: Vec<String> }
pub struct Fact { pub id: String, pub effects: Vec<String>, pub effect_sites: Vec<EffectSite>, pub effects_complete: bool,
    pub copy_sites: Vec<Site>, pub alloc_sites: Vec<Site>, pub alloc_complete: bool, pub trap_sites: Vec<Site>,
    pub recursion: String, pub specializations: u64, pub raw: serde_json::Value }          // raw は G28 の `show --json` が行をそのまま返すため
pub struct Meta { pub project: String, pub version: Option<String>, pub edition: String, pub complete: bool,
    pub source_hash: String, pub warnings: usize, pub modules: Vec<ModuleEntry> }
pub struct Diag { pub severity: String, pub code: Option<String>, pub message: String, pub path: Option<String>, pub raw: serde_json::Value }
pub struct KbData { pub meta: Meta, pub symbols: Vec<Symbol>, pub graph: Option<Vec<Edge>>, pub tests: Option<Vec<TestRow>>,
    pub facts: Option<BTreeMap<String, Fact>>, pub diag: Vec<Diag> }                      // None＝ファイルが無い（G26 の Phase が未出荷）
```

### H-2: `emit.rs`

```rust
pub fn emit(model: &KbModel) -> std::io::Result<Files> {
    let mut files = Files::default();
    files.put("symbols.jsonl", render(|out| model.write_symbols(out))?);        // G26 Phase 1
    // G26 Phase 2 が出荷されたら足す
    files.put("graph.jsonl", render(|out| model.write_graph(out))?);
    files.put("tests.jsonl", render(|out| model.write_tests(out))?);
    // G26 Phase 3〜5 が出荷されたら足す（行のあるモジュールだけ。`layout_modules`・`write_layout` も同じ形）
    let keys: Vec<String> = model.fact_modules().into_iter().map(str::to_owned).collect();
    let names = write::unique_names(&keys);                                      // key → 幹（`~N` を含む）
    let _facts = crate::timings::Phase::start("kb-facts");
    for key in &keys {
        files.put(format!("facts/{}.jsonl", names[key]), render(|out| model.write_facts(key, out))?);
    }
    Ok(files)
}
fn render(write: impl FnOnce(&mut Vec<u8>) -> std::io::Result<()>) -> std::io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    write(&mut buffer)?;
    Ok(buffer)
}
```

`kb-facts` の区間は `facts`・`layout` のループだけに掛ける（`_facts` のガードをその前で作り、ループの後で落とす）。`manifest` の `files` と `counts` は `Files` から作る（`counts.symbols` は `symbols.jsonl` の `\n` の数、`edges` は `graph.jsonl` の `\n` の数、`tests`・`benches` は `tests.jsonl` の `"kind":"test"` と `"kind":"bench"` の行の数）。

## 付録 I: `E2009` のメッセージと各テストの入力・期待

### I-1: `E2009` のメッセージ（固定。`{}` は値。スパンなし、`related`・`fixes` なし）

| kind | いつ | メッセージ |
| --- | --- | --- |
| `kb_missing` | 置き場がない（`verify`・照会） | `no KB at '{dir}'; run tsuzuri check or tsuzuri kb build` |
| `kb_stale` | 古い | `the KB at '{dir}' is stale ({reason}); run tsuzuri kb build` |
| `kb_broken` | manifest が不正、ファイルの欠落・不一致 | `the KB at '{dir}' is broken ({reason}); run tsuzuri kb build --force` |
| `kb_not_ours` | 置き場が KB でない・ソースを含む | `'{dir}' exists and is not a KB (no .tsuzuri-kb marker); choose another directory with --kb-dir` |
| `kb_incomplete` | `complete: false` | `the KB at '{dir}' is incomplete: the project has {n} errors; fix them and run tsuzuri check` |

`{reason}`: `kb_stale` は `sources changed`・`built by a different compiler`・`built with a different schema`、`kb_broken` は `manifest.json is not valid`・`{path} is missing`・`{path} does not match manifest.json`。`{n}` は manifest の `counts.errors`。`{dir}` は与えられたとおりの置き場。

例（テキスト、`--json`、自動の作成の失敗の通知）:

```text
<command line>:1:1: error[E2009]: the KB at 'target/tz/kb' is stale (sources changed); run tsuzuri kb build
{"severity":"error","code":"E2009","message":"the KB at 'target/tz/kb' is stale (sources changed); run tsuzuri kb build","path":"<command line>","span":{"start":0,"end":0,"line":1,"column":1,"end_line":1,"end_column":1}}
kb not updated: 'target/tz/kb' exists and is not a KB (no .tsuzuri-kb marker); choose another directory with --kb-dir
{"severity":"warning","code":"W2001","message":"kb not updated: 'target/tz/kb' exists and is not a KB (no .tsuzuri-kb marker); choose another directory with --kb-dir"}
```

### I-2: `tests/kb_build.rs`（Phase 1）の入力と期待

- `check_creates_the_kb_and_a_second_check_writes_nothing`: `kb_shop` を `check` → `target/tz/kb/` に `.tsuzuri-kb`・`manifest.json`（`complete: true`、`counts.warnings: 1`）・`symbols.jsonl`・`diag.jsonl`（1 行、`path` は `Cart.tz`）。2 回目の `check` の前後で全ファイルの更新時刻と inode が同じ。
- `editing_a_source_rebuilds_the_kb`: 関数を足す → `source_hash` が変わり、`symbols.jsonl` に ID が出る。
- `no_kb_env_and_loose_files`: `--no-kb`・`TSUZURI_KB=0` では `target` が無い。`Main.tz` も manifest もないルートの `check Physics.tz` は作らず、`--kb`・`TSUZURI_KB=1` では作る。
- `kb_flag_conflicts_and_bad_env_are_e2000`: `--kb --no-kb`・`TSUZURI_KB=2`・`--kb-dir ""`・`doc --kb` → `E2000`、終了コード 2（メッセージは §1 の表）。`kb_dir_flag_and_env_choose_the_directory`: `--kb-dir` が `TSUZURI_KB_DIR` に勝ち、相対パスはカレントから。
- `a_foreign_directory_is_never_overwritten`: マーカーのない `target/tz/kb/x` → 通知 1 回（テキストと `--json` の `W2001`）、終了コード 0、中身は無傷。`an_unwritable_target_warns_once`: `target` が通常ファイル → 通知 1 回、終了コード 0。
- `a_failed_analysis_writes_an_incomplete_kb`: 型エラーのあるプロジェクト → 終了コード 1、`complete: false`、`diag.jsonl` に `E1003`、`symbols.jsonl` なし。もう一度 `check` しても何も書かない。
- `the_kb_is_identical_in_two_directories_and_thread_counts`: 同じソースを別の 2 ルートに置き、`TSUZURI_THREADS=1` と `8` で `check` → `.` で始まらない全ファイルが同じ byte。`grep` で絶対パスと stamp が無い。
- `concurrent_checks_leave_a_valid_kb`: 8 プロセスを同時に `check`（ソースを変えた直後）→ マニフェストが読め、`target/tz` に `.tsuzuri-` で始まるものが残らない。
- `the_output_of_commands_does_not_depend_on_the_kb`: `kb_shop` とエラーのあるプロジェクトで `check`・`test`・`run` の標準出力・標準エラー・終了コードが `--no-kb` と同じ。
- `kb_dir_is_skipped_by_source_discovery`: `target/tz/kb/` に壊れた `.tz` を置く → マーカーがあれば無視、なければ `E0002`。`test_run_and_build_create_the_kb`: `test`・`run`・`build --emit llvm` でも作る。

### I-3: Phase 2〜4 のテストの入力・期待（名前は本文）

- `kb_build_verify_clean_export_round_trip`: `kb build kb_shop` → `kb: wrote 18 files to target/tz/kb`（付録 D-5 の `files` の 16 件＋ `manifest.json`＋ `.tsuzuri-kb`）→ 2 回目は `kb: up to date (target/tz/kb)`、`kb verify` は `kb: ok (14 symbols, 16 files, source 1c4ce603d1ab)`、`kb export --symbols` は D-2 と byte 同じ、`kb clean` で置き場が消える。
- `verify_reports_each_e2009_kind`: 置き場なし→`kb_missing`、`manifest.json` を壊す→`kb_broken`、`source_hash` を書き換える→`kb_stale`、マーカーを消す→`kb_missing`（「no .tsuzuri-kb marker」）、型エラーのあるプロジェクト→`kb_incomplete`。メッセージは I-1。
- `export_needs_exactly_one_flag`: フラグなし・`--symbols --graph` → `E2000`（終了コード 2）。
- `l2_files_are_capped_at_256`: 300 個の `export def` を持つプロジェクト → `brief/L2/` が 256 件、`counts.l2_cards: 256`・`l2_omitted: 44`。
- `std_kb_is_reused_and_old_generations_are_removed`: `kb build --std` を `TSUZURI_CACHE_DIR` を変えて 4 回（stamp の違う 4 つの `<id>/` を手で作る）→ 更新が新しい 3 つが残る。
- `memory_mode_writes_nothing`: `StdSource::Memory` の `KbView` が std の ID を引いても、キャッシュのディレクトリの全ファイルの（パス・サイズ・更新時刻）が変わらない。







