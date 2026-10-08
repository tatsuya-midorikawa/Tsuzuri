# G16: デバッガー体験（型の表示・PDB）

| 項目 | 内容 |
| --- | --- |
| ID | G16 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | G08, (G10), (G14) |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 不要（Phase 1 の決定はすべて既定案。Phase 2・3 は人間が求めた場合だけ着手する）。2026-10-08 に利用者が全 Phase の実装を求め、GUIDE D-41 で承認済み |
| 改善する劣位 | C/C++ 比: デバッガーの成熟度（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cc-に対する劣位点)）、C#/F# 比: 開発体験（[同](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | DWARF の型と cache: `src/llvm_debug.rs` の `DebugContext::ty`・`fields`・`layout`・`metadata`・`quote`。union の格納: `src/llvm.rs` の `union_layout`・`storage_layout`・`FunctionEmitter::payload_pointer`、`src/llvm_recursive.rs` の `node_type`・`recursive_header`・`recursive_payload`・`recursive_tag`・`empty_case`。list の node: `src/llvm.rs` の `FunctionEmitter::list_node_type`。テスト: `tests/debug_info.rs` の `debug_metadata_is_deterministic_and_keeps_source_types`、`tests/debug_info.mjs` の `execute`。VS Code: `vsc/src/workflow.ts` の `debugConfiguration`（`preRunCommands`）、`vsc/scripts/toolchain.mjs` の `resources`、`vsc/scripts/package.mjs` の `required` |
| 主な影響ファイル | `src/llvm_debug.rs`, `src/llvm.rs`（`FunctionEmitter::emit` の `debug_subprogram` 呼び出し）, `scripts/lldb/tsuzuri_lldb.py`（新規）, `tests/debug_info.rs`, `tests/debug_info.mjs`, `tests/debugger.mjs`（新規）, `tests/fixtures/debug_view/Main.tz`（新規）, `vsc/src/workflow.ts`, `vsc/scripts/toolchain.mjs`, `vsc/scripts/package.mjs`, `vsc/src/test/extension.test.ts`, `docs/architecture.md`, `_docs/tools/debugging.md`, `vsc/README.md`, `README.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

Visual Studio（C#）、rust-lldb（Rust）、GDB の pretty printer（C++）のように、LLDB で文字列・配列・list・Vec・record・union・
`Maybe`・`Result`・`Map`・`Set`・関数値を Tsuzuri の値として表示する。呼び出し履歴の関数名を Tsuzuri の名前にし、
step 実行が compiler の生成した helper と runtime に入らないことを保証する。

Phase 1（DWARF の改善、LLDB の Python formatter、VS Code での自動読み込み、LLDB の batch テスト）だけを実装する。
Phase 2（Test Explorer からのテストのデバッグ）と Phase 3（Windows の PDB と natvis）は設計方針だけを記し、人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- G08 が `_features/README.md` の状態欄で done であること。確認: `grep -n "^| G08 \|^| G16 " _features/README.md` で G08 の行が `done`。
- G10・G14 は着手条件にしない。G14 が done なら、formatter を toolchain 配布物へも入れる作業を G14 の手順に従って追加する（D8）。
- GUIDE §2.3 の基準コマンドが成功すること。加えて次の 3 つが成功すること（macOS arm64 で確認済み）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
TSUZURI_DWARFDUMP=/opt/homebrew/opt/llvm@21/bin/llvm-dwarfdump node tests/debug_info.mjs target/release/tsuzuri
lldb --version
lldb -b -o "script print(6 * 7)"
```

1 行目は `debug info: native/WASM -O0 verified` と `-O3` の 2 行を出す。3 行目は `42` を出す（LLDB の Python が有効）。
HEAD の Apple LLDB は `lldb-2103.0.34.103`。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `linkageName` を外しても LLDB の frame 表示が `tz.fn.Main.show` のまま、または `breakpoint set -n Main.show` が 0 location になる（D3）。
- DWARF 上の payload の offset を、生成 IR の格納位置（`union_layout`・`recursive_payload`・`list_node_type`）と一致させられない。
  IR の layout を変えて合わせたくなった場合も含む（Phase 1 は metadata だけを変え、命令列は変えない）。
- `-g` なしの IR（`llvm::emit_with_options`）が 1 byte でも変わる。
- `llvm-dwarfdump --verify` が native または wasm32 の `-O0`・`-O3` のどれかで失敗する。
- `thread step-in` が runtime（`tz.alloc` など）や生成 helper（`tz.apply.*`・`tz.drop.rec.*`・`tz.clone.*`）の中で止まる。
- CodeLLDB 1.12.3（`vsc/scripts/toolchain.mjs` の `debuggerVersion`）が `initCommands` の `command script import` で script を読めない。
- LLDB 同梱の `lldb` module 以外の Python package、新しい crate、既定の WASM import が必要になった。
- 「既存テストへの影響」に挙げていない既存テストの期待値を変える必要がある。

## 現状（HEAD `f8dc655` で確認）

- `src/llvm_debug.rs` の `DebugContext::new` は `DW_LANG_C`・DWARF version 4 の compile unit を出す。型は `DebugContext::ty` が
  `types: BTreeMap<Type, usize>` で一度だけ作り、名前は `Type::display`（`i64`、`string`、`[i64]`、`[|i64|]`、`Vec<i64>`、
  `(i64 * bool)`、`Main.Point`、`Maybe<i64>`）。採番は `Globals::next_metadata` だけで、IR は決定的。
- スカラーは `DIBasicType(name: "i64", ...)` だが、LLDB は encoding と大きさから C の組み込み型を選ぶので `(long) input = 40` と表示する。
- `fields` は string・utf8string・配列・共有配列参照を `data`・`length`、list を `head`・`length`、Vec を `data`・`length`・`capacity`、
  関数値と Task を `code`・`environment`・`clone`・`drop` として出す。`head` と関数値の 4 field は `Type::Reference(Type::Unit)` で、
  LLDB は 8 bit 符号なしへの pointer を C 文字列として読み、`head = "\`\xe4\xdfo..."`・`code = "؀D..."` のようなごみを表示する。
- 再帰しない union は field のない `DW_TAG_structure_type`（`(Main.Shape) shape = {}`）。再帰 union は `baseType: null` の pointer
  （`(void *) tree = 0x...`）。どちらも case 名と payload が見えない。
- `DebugContext::subprogram` は `name` に `CheckedFunction::qualified_name`（lambda と Task は親の名前を前置）、`linkageName` に symbol を出す。
  LLDB は linkage 名を優先し、frame を `tz.fn.Main.show` と表示する。単相化した std 関数は `Array.sum.$mono.2`（`src/polymorph.rs` が
  `{name}.$mono.{instance}` を付ける）。`wrapper` は export 用 `@tz_<name>`、`@tsuzuri_main`、`@main` に利用者関数と同じ `name` を付ける。
- 生成 helper（`tz.apply.*`、`tz.drop.rec.*`、`tz.clone.new.rec.*`、`tz.clone.step.rec.*`）と runtime には行情報がない。LLDB の既定
  （`target.process.thread.step-in-avoid-nodebug true`）で step-in は入らない。これを固定するテストはない。
- `vsc/src/workflow.ts` の `debugConfiguration` は `type: 'lldb'`、`sourceLanguages: ['c']`、macOS だけ
  `preRunCommands: ["target symbols add <program>.dwarf"]` を作る。`initCommands` と formatter はない。CodeLLDB は
  `vsc/scripts/toolchain.mjs` の `debuggerVersion = '1.12.3'` を `codelldb.vsix` として同梱する。`vsc/resources/` は `.gitignore` 対象で、
  `resources` 関数が毎回生成する。
- テストは `tests/debug_info.rs` の 2 件（`debug_options_are_opt_in_and_reject_header_output`、
  `debug_metadata_is_deterministic_and_keeps_source_types`）と `tests/debug_info.mjs`（fixture `tests/fixtures/debug_info/Main.tz`、
  `llvm-dwarfdump --verify`、native/WASM × `-O0`/`-O3`）。LLDB で値の表示を確かめるテストはない。
- WASM は `.debug_info` custom section を保持するだけで、表示はデバッガー次第。PDB／CodeView の出力はない。`-O3` では変数が消える場合がある。

### 再現（検証済み）

`/tmp/tz-g16/probe/Main.tz`（W1001 の警告が出るが build は成功する）:

```tsuzuri
union Shape = Empty | Circle of f64 | Rect of f64 * f64
union Tree = Leaf | Node of Tree * i64 * Tree

def show :: i64 -> i64
fn show input =
    let text = "h\u{e9}llo"
    let shape = Rect (1.5, 2.0)
    let tree = Node (Leaf, input, Leaf)
    let maybe = Maybe.Some input
    let values = [1, 2, 3]
    let chain = [|4, 5|]
    let total = Array.sum (&values)
    total + input

show 40
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri build /tmp/tz-g16/probe -g -O0 -o /tmp/tz-g16/probe/app
lldb -b -o "target symbols add /tmp/tz-g16/probe/app.dwarf" -o "breakpoint set -f Main.tz -l 13" -o run \
  -o "frame variable" /tmp/tz-g16/probe/app
```

HEAD の出力（抜粋）:

```text
frame #0: 0x... app`tz.fn.Main.show(input=40) at Main.tz:13:5
(long) input = 40
(string) text = {
  data = 0x0000000100013b9c
  length = 5
}
(Main.Shape) shape = {}
(void *) tree = 0x0000000100689650
(Maybe<i64>) maybe = {}
([i64]) values = {
  data = 0x000000016fdfe480
  length = 3
}
([|i64|]) chain = (head = "`\xe4\xdfo\U00000001", length = 2)
(long) total = 6
```

## 仕様

### 前提とする他チケットのインターフェース

なし。G14 の配布物への同梱は D8 のとおり G14 側の作業とし、Phase 1 は VS Code 拡張（VSIX）とリポジトリの script だけを対象にする。

### 表示（Phase 1、`-O0`）

型の識別は DWARF の型名（`Type::display`）と、Tsuzuri の識別子に現れない `$` を含む member 名で行う（D5・D6）。

| Tsuzuri の型 | DWARF（変更後） | LLDB の表示（formatter 読み込み後） |
| --- | --- | --- |
| 整数・浮動小数点・decimal・`char`・`utf8char`・`unit` | `DW_TAG_typedef`（名前は `Type::display`）→ `DIBasicType`。`char` は `DW_ATE_UTF` 16 bit、`utf8char` は `DW_ATE_UTF` 32 bit | `(i64) input = 40`、`(char) letter = u'A'`、`unit` は `()` |
| `bool` | 変更なし | `(bool) flag = true` |
| `string` | 変更なし（`data`・`length`） | `"héllo"`。UTF-16 を復号し、孤立 surrogate は U+FFFD |
| `utf8string` | 変更なし | `u8"abc"`。不正な列は U+FFFD |
| 配列・共有配列参照・`Vec<T>` | 変更なし | summary `length=3`（Vec は `length=1 capacity=4`）、子は `[0]`、`[1]`… |
| list | `head` を node struct（`next`・`value`）への pointer にする | summary `length=2`、子は `next` を辿った要素 |
| record・tuple | 変更なし | LLDB の既定（field 名と値） |
| 全 case が nullary の union | `DW_TAG_enumeration_type`（enumerator は case 名と tag） | `(Main.Color) color = Green`（formatter 不要） |
| payload を持つ union（`Maybe`・`Result` を含む） | struct に `$tag`（enumeration）と `$payload`（case 名を member 名とする `DW_TAG_union_type`） | `Rect(1.5, 2)`、`Some(40)`、`None`、`Error("bad")`。子は有効な case の payload だけ |
| 再帰 union | node struct（`next`・`drop`・`clone`・`$tag`・`$payload`）への pointer | null は空 case 名（`Leaf`）、それ以外は `Node(Leaf, 40, Leaf)` |
| `Map<K, V>`・`Set<K>` | 変更なし（record と `entries` の Vec） | summary `size=1`、子は `entries` の要素 |
| 関数値・Task | `code` を関数型への pointer、他の 3 field を型なし pointer にする | `<fn 名前>`（`code` の指す関数名。なければ symbol 名）、Task は `<task>` |

formatter を読み込まない LLDB・GDB でも、DWARF だけで scalar の型名、nullary union の case 名、`$tag` の case 名、関数値の関数名が読める。

### 関数名と step 実行

- `DISubprogram` の `name` は次の規則で作り、`linkageName` は出さない（D3）。利用者関数は `Main.show`。単相化した関数は
  `.$mono.<N>` を除いた `Array.sum`（すべての実体が同じ名前になり、`breakpoint set -n Array.sum` が全実体に当たる）。lambda と Task は
  `<親の名前>.lambda@<行>:<列>`・`<親の名前>.task@<行>:<列>`（位置は関数の `span` の開始）。callback 特殊化（`@tz.specialized.<N>`）は元の関数と同じ規則。
- `wrapper` が作る `@tz_<name>`・`@tsuzuri_main`・`@main` は symbol 名（先頭の `@` を除く）を `name` にし、`flags: DIFlagArtificial` を付ける。
- `let` と引数の束縛で局所変数の slot へ書く `store` の位置は、その局所変数の宣言位置にする（D7）。HEAD では関数本体の開始位置になり、
  `thread step-in` が 14 行目から 8 行目へ戻る（検証済み）。変更後は次の行（15 行目）で止まる。
- 生成 helper と runtime には従来どおり行情報を付けない。LLDB の既定で step-in はそれらに入らない。

### 数値・トラップ・native と WASM の差

- 命令列は変えない。変わるのは metadata と、束縛の `store` に付く `!dbg` の番号だけ。`-g` なしの IR は byte 単位で同一。
- DWARF の変更は wasm32 にも同じ規則で出す（pointer 32 bit、offset は `layout` の `wasm` 引数で計算する）。wasm32 で確認するのは
  `llvm-dwarfdump --verify` と custom section の存在だけ（D10）。
- `-O0` は上の表示・step を保証し、LLDB のテストで固定する。`-O3` は `llvm-dwarfdump --verify` と型 metadata の存在だけを確認する。
  変数が最適化で消えた場合、formatter は例外を出さず LLDB の既定の表示（`<variable not available>` など）に任せる。

### 診断

compiler の診断は追加しない。formatter は読み取りに失敗しても例外を LLDB へ伝えず、summary に次の文字列を返す。

| 条件 | summary |
| --- | --- |
| `data` や node の読み取りに失敗した | `<unreadable>` |
| `length` が負、または 2^32 以上 | `<invalid length N>`（子は 0 個） |
| `$tag` が enumerator のどれでもない | `<invalid tag N>`（子は 0 個） |

### 資源上限

- string の summary は先頭 1,024 code unit、utf8string は 1,024 byte で切り、`...` を付ける。
- 子の数は `min(length, max)`（`max` は LLDB が `num_children` へ渡す上限。既定は `target.max-children-count` の 256）。list は
  その数だけ `next` を辿り、null で止まる。
- union の summary の入れ子は 3 段まで。4 段目は `...`。

### 例

LLDB 用 fixture `tests/fixtures/debug_view/Main.tz`（新規）。次の内容は HEAD の compiler で build・実行できる（出力 `6`、W1001 の警告だけ）。

```tsuzuri
union Shape = Empty | Circle of f64 | Rect of f64 * f64
union Color = Red | Green
union Tree = Leaf | Node of Tree * i64 * Tree
record Point { x: i64, label: string }

def show :: i64 -> i64
fn show input =
    let text = "h\u{e9}llo"
    let bytes = u8"abc"
    let letter = 'A'
    let point = Point { x: input, label: "p" }
    let color = Green
    let shape = Rect (1.5, 2.0)
    let tree = Node (Leaf, input, Leaf)
    let maybe = Maybe.Some input
    let nothing: Maybe<i64> = Maybe.None
    let outcome: Result<i64, string> = Result.Error "bad"
    let values = [1, 2, 3]
    let chain = [|4, 5|]
    let growing = Vec.push (Vec.empty()) 7
    let table = Map.singleton 1 10
    let add = \offset -> input + offset
    let total = Array.sum (&values)
    total + add 0 - input

show 40
```

24 行目で止めて `frame variable` を実行したときの期待出力（実装後に有効。未検証。値は fixture の式から手で求めた）:

```text
(i64) input = 40
(string) text = "héllo"
(utf8string) bytes = u8"abc"
(char) letter = u'A'
(Main.Color) color = Green
(Main.Shape) shape = Rect(1.5, 2)
(Main.Tree) tree = Node(Leaf, 40, Leaf)
(Maybe<i64>) maybe = Some(40)
(Maybe<i64>) nothing = None
(Result<i64, string>) outcome = Error("bad")
([i64]) values = length=3 {
  [0] = 1
  [1] = 2
  [2] = 3
}
([|i64|]) chain = length=2 {
  [0] = 4
  [1] = 5
}
(Vec<i64>) growing = length=1 capacity=4 {
  [0] = 7
}
(i64) total = 6
```

`point` は `x = 40`、`label = "p"` の 2 行を持つ struct 表示、`table` は summary `size=1`（子は `entries` の要素）、`add` は `<fn ` で始まる summary になる。

見直し（2026-10-08）: 実装後の実際の出力（Apple LLDB lldb-2103 と CodeLLDB 1.12.3 の LLDB 22.1.8 で同じ）は上の期待とほぼ同じで、次だけが違う。
(1) 型を注釈しない整数リテラルの list と `Vec` は `i32` なので `([|i32|]) chain`・`(Vec<i32>) growing`・`(Map<i32, i32>) table = size=1 { [0] = (key = 1, value = 10) }`。
(2) `char` は D4 の見直しのとおり `(char) letter = 'A'`。(3) `point` は LLDB の 1 行の形 `(Main.Point) point = (x = 40, label = "p")`。
(4) fixture は wasm32 の module を作れるように 26 行目に `export def view :: i64 -> i64 = \input -> show input` を足した（1〜25 行目は上と同じなので
LLDB の行番号は変わらない。`show 40` は 28 行目）。(5) 8 行目の `frame variable` は未初期化の値を `<invalid length N>`・`<invalid tag N>`・`<unreadable>`
で表示し、`utf8string` は読めたメモリを 1,024 バイトまで表示する。

### Phase 2: テストのデバッグ（設計方針）

VS Code の Test Explorer に Debug profile を足し、`tsuzuri test <root> --index N` と同じ選択で単一テストを `-g` 付きで build して
CodeLLDB で起動する。test runner が各テストを別 process で実行するため、debug 用に 1 テストだけを実行する entry が要る。Phase 2 の着手時に設計を確定する。
見直し（2026-10-08）: D13 の設計（`tsuzuri test ROOT --index N -g -o PATH` と Testing ビューの Debug のプロファイル）で実装した。

### Phase 3: Windows（設計方針）

G10 の完了後、CodeView／PDB（Clang の `-gcodeview`、`lld-link /debug`）と natvis を追加する。natvis は Phase 1 の DWARF 形（`$tag`・`$payload`・
node struct）と同じ member 名を使う。
見直し（2026-10-08）: D14 の設計で実装した。IR を入力にする Clang は `-gcodeview` で module flag を足さないので、driver が `CodeView` の flag を
IR へ足す（`llvm::with_codeview`）。

## 設計

### データ構造

```rust
pub(super) struct DebugContext {
    sources: Vec<Source>,
    unit: usize,
    empty: usize,
    optimized: bool,
    wasm: bool,
    types: BTreeMap<Type, usize>,
    nodes: BTreeMap<Type, usize>,      // （新規）再帰 union と list の node struct。key は union 型または list 型
    opaque: Option<usize>,             // （新規）名前のない型なし pointer
    code: Option<usize>,               // （新規）関数値の `code` 用の関数型への pointer
    locations: BTreeMap<(usize, usize, usize), usize>,
}
```

`DebugContext::subprogram` と `Globals::debug_subprogram` に `artificial: bool` を足す。名前は `subprogram_name(module, function, file_line_column)`（新規、
非公開の関数）が作る。新しい metadata も `metadata` で `Globals::next_metadata` から採番し、cache は `BTreeMap` だけにする（決定性）。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| DWARF | `src/llvm_debug.rs` | `DebugContext` | `nodes`・`opaque`・`code`（新規）。`new` で空に初期化する |
| DWARF | `src/llvm_debug.rs` | `DebugContext::ty` | scalar（`bool` 以外）を typedef にする。union は `union_type`（新規）へ、list・関数値・Task は新しい member 形へ分ける |
| DWARF | `src/llvm_debug.rs` | `fields` | `Type::List` と `Type::Function(..) \| Type::Task(_)` の arm を消す（`ty` が直接作る）。他は変更なし |
| DWARF | `src/llvm_debug.rs` | `union_type`（新規）, `node_type`（新規）, `opaque_pointer`（新規）, `code_pointer`（新規） | 下の「生成 IR とランタイム」の形を作る |
| DWARF | `src/llvm_debug.rs` | `layout` | 変更なし（union・node の offset 計算に使う） |
| DWARF | `src/llvm_debug.rs` | `DebugContext::subprogram`, `subprogram_name`（新規） | `linkageName` を消し、名前の規則と `DIFlagArtificial` |
| DWARF | `src/llvm_debug.rs` | `wrapper`, `Globals::debug_subprogram` | `wrapper` は `artificial = true` を渡す |
| IR | `src/llvm.rs` | `FunctionEmitter::emit` | `debug_subprogram(..., false)` |
| IR | `src/llvm.rs` | `FunctionEmitter::bind_local` | 束縛の `store` を出す間だけ `current_span` を `local.span` にし、後で戻す |
| formatter | `scripts/lldb/tsuzuri_lldb.py`（新規） | `__lldb_init_module` ほか | D6 |
| VS Code | `vsc/src/workflow.ts` | `debugConfiguration` | `initCommands` の先頭に `command script import "<拡張>/resources/lldb/tsuzuri_lldb.py"` |
| VS Code | `vsc/scripts/toolchain.mjs` | `resources` | `scripts/lldb/tsuzuri_lldb.py` を `resources/lldb/` へ複製 |
| VS Code | `vsc/scripts/package.mjs` | `required` | `extension/resources/lldb/tsuzuri_lldb.py` を追加 |

### 生成 IR とランタイム

runtime・生成 helper・命令列は変えない。DWARF の形は次のとおり（`p` は pointer の大きさ、native 8・wasm32 4）。

- scalar: `!A = !DIBasicType(name: "i64", size: 64, encoding: DW_ATE_signed)` と
  `!B = !DIDerivedType(tag: DW_TAG_typedef, name: "i64", baseType: !A)`。`types` には `!B` を入れる。
- 型なし pointer: `!DIDerivedType(tag: DW_TAG_pointer_type, baseType: null, size: <8p>)`（名前なし、1 個だけ）。
- `code`: `!DISubroutineType(types: !{null})` への pointer（1 個だけ）。関数値と Task の `environment`・`clone`・`drop` は型なし pointer。
- list の node: `distinct !DICompositeType(tag: DW_TAG_structure_type, name: "[|i64|].node", ...)`。member は `next`（この node への pointer、offset 0）と
  `value`（要素型、offset は `p` を要素の align に切り上げた値）。LLVM の形は `FunctionEmitter::list_node_type` の `{ ptr, T }`。
- 全 case が nullary の union: `!DICompositeType(tag: DW_TAG_enumeration_type, name: "Main.Color", size: 32, align: 32, baseType: <i32>, elements: !{...})`、
  enumerator は `!DIEnumerator(name: "Red", value: 0)`。
- payload を持つ union: struct（名前は `Type::display`、大きさは従来の `layout`）の member `$tag`（offset 0、enumeration 名は `<型名>.$tag`）と
  `$payload`（`DW_TAG_union_type`、名前 `<型名>.$payload`、member は payload を持つ case ごとに case 名と payload 型、すべて offset 0）。
  `$payload` の offset は `union_layout` が `Common(T)` なら `4` を `T` の align に切り上げた値、`General(_)` なら 16。
- 再帰 union: 従来の pointer 型（名前は `Type::display`）の `baseType` を node struct `<型名>.node` にする。member は `next`・`drop`・`clone`
  （型なし pointer、offset 0・`p`・`2p`）、`$tag`（offset `3p`）、`$payload`（offset は `3p + 4` を payload 格納型の align に切り上げた値）。
  payload 格納型は `src/llvm.rs` の node 定義（`{ ptr, ptr, ptr, i32, {fields} }`）と同じで、`Common(T)` は `T`、`General(K)` は
  `[K x i128]`（align 16）。空 case（`empty_case`）は null pointer で表す。

formatter（`scripts/lldb/tsuzuri_lldb.py`、LLDB の `lldb` module だけを使う）の骨格:

```python
import lldb

TEXT_LIMIT = 1024
LENGTH_LIMIT = 1 << 32

def __lldb_init_module(debugger, _dict):
    run = debugger.HandleCommand
    run("type category define tsuzuri")
    run("type summary add -w tsuzuri -F tsuzuri_lldb.string_summary string")
    run("type summary add -w tsuzuri -F tsuzuri_lldb.utf8_summary utf8string")
    run('type summary add -w tsuzuri -s "()" unit')
    run('type synthetic add -w tsuzuri -l tsuzuri_lldb.SequenceProvider -x "^(ref )?\\[[^|].*\\]$|^Vec<.+>$"')
    run('type synthetic add -w tsuzuri -l tsuzuri_lldb.ListProvider -x "^\\[\\|.+\\|\\]$"')
    run('type synthetic add -w tsuzuri -l tsuzuri_lldb.CollectionProvider -x "^(Map\\.Map|Set\\.Set)<.+>$"')
    run("type synthetic add -w tsuzuri -l tsuzuri_lldb.UnionProvider --recognizer-function tsuzuri_lldb.is_union")
    run("type summary add -w tsuzuri -F tsuzuri_lldb.union_summary --recognizer-function tsuzuri_lldb.is_union")
    run("type summary add -w tsuzuri -F tsuzuri_lldb.function_summary --recognizer-function tsuzuri_lldb.is_function")
    run("type category enable tsuzuri")
```

- `is_union(sbtype, _dict)`: struct の最初の field が `$tag`、または pointer の指す struct の名前が `.node` で終わり `$tag` を持つ。
- `is_function(sbtype, _dict)`: field が順に `code`・`environment`・`clone`・`drop` の 4 個で、`code` が関数 pointer。
- 読み取りは `SBProcess.ReadMemory` と `SBError` で行い、失敗は「診断」の表の文字列にする。summary 関数は必ず `str` を返す。

### アルゴリズム

```text
union_summary(value, depth):
  node = value が pointer なら deref（null なら 空 case 名を返す）
  tag  = node.$tag の unsigned 値。enumerator になければ "<invalid tag N>"
  name = tag の enumerator 名
  payload = node.$payload の child のうち名前が name のもの。なければ name を返す（nullary）
  depth >= 3 なら name + "(...)"
  parts = payload が tuple（名前が "(" で始まる struct）なら各 child、でなければ [payload]
  各 part: union なら union_summary(part, depth + 1)、それ以外は part の summary、なければ value
  return name + "(" + ", ".join(parts) + ")"

空 case 名 = enumerator のうち $payload に同名 member がない最初のもの（empty_case と同じ「最初の nullary case」）
```

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked --test debug_info` の `running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。
LLDB の確認は macOS arm64 で行う。以下の `APP` は `/tmp/tz-g16/app`。

### 手順 1: ベースラインと fixture

- 変更: `tests/fixtures/debug_view/Main.tz`（新規、「例」の内容）。
- 内容: GUIDE §2.3 の基準コマンドを実行し、`-g` なしの IR を保存する（手順 12 で byte 比較する）。`-g` の IR を文字列で比較する既存テストがないことを確かめる。
- 確認: 次がすべて成功し、`run` は `6`、`cargo test` は `2 passed`。最後の `grep` は `tests/cache.mjs`・`tests/debug_info.mjs`・`tests/docgen.mjs`・
  `tests/wasm_threads.mjs` を出す（HEAD で確認済み）。この 4 つの `-g` の使い方を読み、DWARF の中身を固定文字列で比べている箇所があれば停止条件。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-g16
for o in -O0 -O3; do
  target/release/tsuzuri build tests/fixtures/debug_view --emit llvm $o -o /tmp/tz-g16/plain$o.ll
  target/release/tsuzuri build tests/fixtures/debug_view --target wasm32 --emit llvm $o -o /tmp/tz-g16/plain-wasm$o.ll
done
target/release/tsuzuri run tests/fixtures/debug_view
cargo test --locked --test debug_info
grep -ln '"-g"' tests/*.mjs
```

### 手順 2: 関数名（D3）

- 変更: `src/llvm_debug.rs` の `DebugContext::subprogram`・`subprogram_name`（新規）・`wrapper`・`Globals::debug_subprogram`、`src/llvm.rs` の `FunctionEmitter::emit`。
- 内容: `linkageName` を消す。名前は「関数名と step 実行」の規則。`.$mono.` 以降を除く処理は `split(".$mono.").next()`（`FunctionEmitter::emit` と同じ書き方）。
  lambda・Task の位置が取れない（`source` が `None`）場合は HEAD の名前に戻す。wrapper は `flags: DIFlagArtificial` を `spFlags` の前に出す。
- 確認: `cargo test --locked --test debug_info` が `3 passed`（`debug_subprogram_names_are_readable` を追加）。次の LLDB で `Breakpoint 1: where = app\`Main.show`
  が 1 location、frame が `app\`Main.show(input=40)`。違えば停止条件。

```sh
cargo build --release --locked
target/release/tsuzuri build tests/fixtures/debug_view -g -O0 -o /tmp/tz-g16/app
lldb -b -o "target symbols add /tmp/tz-g16/app.dwarf" -o "breakpoint set -n Main.show" -o run -o "bt 1" /tmp/tz-g16/app
```

### 手順 3: 束縛の位置（D7）

- 変更: `src/llvm.rs` の `FunctionEmitter::bind_local`。
- 内容: 束縛の `store` を出す間だけ `current_span` を `local.span` にし、直後に元へ戻す。命令の順序と数は変えない。
- 確認: `debug_binding_stores_use_declaration_lines` を追加して `4 passed`。次の LLDB で step-in 後の frame が `at Main.tz:15`。

```sh
lldb -b -o "target symbols add /tmp/tz-g16/app.dwarf" -o "breakpoint set -f Main.tz -l 14" -o run -o "thread step-in" -o "frame info" /tmp/tz-g16/app
```

### 手順 4: scalar と pointer（D4）

- 変更: `src/llvm_debug.rs` の `DebugContext::ty`・`opaque_pointer`（新規）・`code_pointer`（新規）・`fields`。
- 内容: `bool` 以外の scalar を typedef にし、`char`・`utf8char` を `DW_ATE_UTF` にする。関数値と Task の 4 field を `code_pointer`・`opaque_pointer` で作る。
- 確認: `debug_scalars_and_pointers_use_tsuzuri_shapes` を追加して `5 passed`。LLDB の `frame variable input letter add`（24 行目）が
  `(i64) input = 40`、`(char) letter = u'A'`、`add` に C 文字列のごみが出ない。

### 手順 5: list の node

- 変更: `src/llvm_debug.rs` の `DebugContext::ty`・`node_type`（新規）。
- 内容: `head` を `<型名>.node` への pointer にする。node は `nodes` に先に登録してから member を作る（`next` が自分を指すため）。
- 確認: `cargo test --locked --test debug_info` が `5 passed`。LLDB の `frame variable chain` の `head` が `([|i64|].node *)` になる。

### 手順 6: union（D5）

- 変更: `src/llvm_debug.rs` の `DebugContext::ty`・`union_type`（新規）・`node_type`。
- 内容: 「生成 IR とランタイム」の 3 形。case 名と payload は `module.unions[id].cases` と `TypeContext::union_payloads`、layout は
  `src/llvm.rs` の `union_layout`（子 module なので `layout` と同じくそのまま呼べる）。offset は `layout` と `next_multiple_of` だけで計算する。
- 確認: `debug_unions_expose_cases_and_payloads` と `debug_union_offsets_match_storage` を追加して `7 passed`。LLDB の
  `frame variable color shape.\$tag` が `Green` と `Rect` を出す。

### 手順 7: DWARF の E2E

- 変更: `tests/debug_info.mjs`。
- 内容: 既存のループの各 `optimization` で `tests/fixtures/debug_view` の object（native）と wasm を `-g` で作り、`--verify` と
  「E2E」の DWARF 項目を確かめる。
- 確認: `TSUZURI_DWARFDUMP=/opt/homebrew/opt/llvm@21/bin/llvm-dwarfdump node tests/debug_info.mjs target/release/tsuzuri` が従来の 2 行を出して成功する。

### 手順 8: formatter（D6）

- 変更: `scripts/lldb/tsuzuri_lldb.py`（新規）。
- 内容: 「生成 IR とランタイム」の骨格、「アルゴリズム」、「資源上限」、「診断」の表。
- 確認: `lldb -b -o "command script import scripts/lldb/tsuzuri_lldb.py" -o "type category list tsuzuri"` が `Category: tsuzuri (enabled)` を含む。

### 手順 9: LLDB の batch テスト（D9）

- 変更: `tests/debugger.mjs`（新規）。
- 内容: 「E2E」の LLDB 項目。`tests/debug_info.mjs` の `execute` と同じ形の helper を使う。
- 確認: `node tests/debugger.mjs target/release/tsuzuri` が `debugger: LLDB formatters, names and stepping verified` を出して成功する。

### 手順 10: VS Code

- 変更: `vsc/src/workflow.ts` の `debugConfiguration`、`vsc/scripts/toolchain.mjs` の `resources`、`vsc/scripts/package.mjs` の `required`、
  `vsc/src/test/extension.test.ts`。
- 内容: `initCommands: [`command script import ${JSON.stringify(context.asAbsolutePath('resources/lldb/tsuzuri_lldb.py'))}`, ...(configuration.initCommands ?? [])]`。
  `resources` は `mkdir(path.join(root, 'lldb'))` の後に `copyFile` する。installed テストのサンプルの breakpoint 行より前に
  `let label = "ok"` を足し、`variables` に `name === 'label' && value === '"ok"'` があることを確かめる。
- 確認: `cd vsc && npm run compile && npm run test:unit` が成功する。macOS arm64 で `npm run package` の後に `npm run test:installed` が成功する。

### 手順 11: 文書

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs _docs/tools/debugging.md` が成功する。

### 手順 12: 最終確認

- 確認: 次がすべて成功し、`cmp` は差分を出さない（`-g` なしの IR が不変）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cargo test --locked
for o in -O0 -O3; do
  target/release/tsuzuri build tests/fixtures/debug_view --emit llvm $o -o /tmp/tz-g16/after$o.ll && cmp /tmp/tz-g16/plain$o.ll /tmp/tz-g16/after$o.ll
  target/release/tsuzuri build tests/fixtures/debug_view --target wasm32 --emit llvm $o -o /tmp/tz-g16/after-wasm$o.ll && cmp /tmp/tz-g16/plain-wasm$o.ll /tmp/tz-g16/after-wasm$o.ll
done
TSUZURI_DWARFDUMP=/opt/homebrew/opt/llvm@21/bin/llvm-dwarfdump node tests/debug_info.mjs target/release/tsuzuri
node tests/debugger.mjs target/release/tsuzuri
node tests/cache.mjs target/release/tsuzuri
node tests/wasm_threads.mjs target/release/tsuzuri
git diff --check
```

## テスト計画

### Rust テスト

`tests/debug_info.rs` に追加する（合計 7 件）。IR は `llvm::emit_with_debug_info` で native と wasm32 の両方を出し、同じ入力で 2 回出して一致を確かめる。

- `debug_subprogram_names_are_readable`: `linkageName:` を含まない。`name: "Main.answer"` が 1 回、lambda が `name: "Main.answer.lambda@`、
  export wrapper が `name: "tz_answer"` と `DIFlagArtificial` を持つ。単相化した std 関数（`Array.sum (&values)` を使う）は `name: "Array.sum"` で `$mono` を含まない。
- `debug_binding_stores_use_declaration_lines`: 3 行の source（2 行目 `let first = input + 1`、3 行目 `let second = first * 2`）で、
  `DILocation(line: 3, column: 9` の id を `!dbg` に持つ `store i64` がある。id と行の対応は `!N = !DILocation(` の行から作る。
- `debug_scalars_and_pointers_use_tsuzuri_shapes`: `DW_TAG_typedef, name: "i64"`、`DW_TAG_typedef, name: "char"`、`DW_ATE_UTF`、
  `DISubroutineType(types: !{null})` を含み、`name: "ref unit"` を含まない（list と lambda を使う source）。
- `debug_unions_expose_cases_and_payloads`: `DW_TAG_enumeration_type, name: "Main.Color"`、`DIEnumerator(name: "Green", value: 1)`、
  `name: "$tag"`、`DW_TAG_union_type, name: "Main.Shape.$payload"`、member `name: "Rect"`、`name: "Main.Tree.node"`、`name: "[|i64|].node"`。
- `debug_union_offsets_match_storage`: `$payload` の member 行の `offset:`（bit）を確かめる。期待値は layout 規則から手で計算した値:
  `Main.Shape`（General、K = 1）は native・wasm32 とも 128。`Maybe<i64>`（Common）は 64。`Main.Tree.node` は native で `$tag` 192・`$payload` 256、
  wasm32 で `$tag` 96・`$payload` 128（payload は `{ ptr, i64, ptr }` で align 8）。
- 既存の `debug_metadata_is_deterministic_and_keeps_source_types` は変更しない。

### E2E

- `tests/debug_info.mjs`（DWARF）: `-O0`・`-O3` × native object・wasm32 で `llvm-dwarfdump --verify` が成功する。`--debug-info` に
  `DW_TAG_enumeration_type`、`DW_TAG_union_type`、`"$tag"`、`"Main.Tree.node"`、`DW_TAG_typedef` があり、`DW_AT_linkage_name` がない。
  `tools` は既存と同じく `TSUZURI_DWARFDUMP`・`TSUZURI_CLANG` で上書きできる。
- `tests/debugger.mjs`（新規、LLDB）: `TSUZURI_LLDB`（新規、既定 `lldb`）で LLDB を選ぶ。見つからなければ `lldb not found; install LLDB or set TSUZURI_LLDB`
  で失敗する（skip しない）。`build tests/fixtures/debug_view -g -O0` の実行ファイルに対して 1 回の batch を実行する。

```sh
lldb -b -o "command script import scripts/lldb/tsuzuri_lldb.py" -o "target symbols add APP.dwarf" \
  -o "breakpoint set -f Main.tz -l 8" -o "breakpoint set -f Main.tz -l 14" -o "breakpoint set -f Main.tz -l 24" \
  -o run -o "frame variable" -o continue -o "thread step-in" -o "frame info" -o continue -o "frame variable" \
  -o "image lookup -r -n '^Main\.show\.lambda@22:'" -o kill APP
```

  `target symbols add` は macOS だけ付ける。確かめること: 8 行目の `frame variable`（未初期化の値）で `Traceback` と `error:` が出ず、
  batch が spawn の timeout（120 秒）内に終わる。step-in 後の `frame info` が `Main.show(input=40) at Main.tz:15`。24 行目の出力が「例」の期待出力の
  各行を含み、`add = <fn ` と `table = size=1` を含む。`image lookup` が `1 match found`。期待値は fixture の式から求めた値で、compiler の出力から作らない。
- VS Code: `vsc/src/test/extension.test.ts` の installed テストに `label` の確認を足す（手順 10）。
- `-O3` と WASM は LLDB で確かめない（D10・D12）。`live == 0` と WASM import の確認は命令列を変えないので追加しない（既存の `tests/debug_info.mjs` の
  `WebAssembly.Module.imports` が空であることの確認は残る）。

### 既存テストへの影響

なし。`tests/debug_info.mjs` の `'"Main.answer"'` は利用者関数の `DW_AT_name` として残る。VS Code の installed テストの
`/calculate/` は `Main.calculate` に一致する。

### 性能

対象外。`-g` なしの IR は不変（手順 12）。`-g` の metadata は型ごとに一定数増えるだけで、計測はしない。

## ドキュメント

- `_docs/tools/debugging.md` の「ソースレベルのデバッグ」: formatter の読み込み（`command script import <repo>/scripts/lldb/tsuzuri_lldb.py`）、
  「表示」の表の要約、`-O3`・WASM の制限、関数名の規則。
- `vsc/README.md`: 「現状は DWARF に記録された低水準の型表示で、Tsuzuri 専用 pretty printer や完全な Tsuzuri 式評価はありません」を、
  formatter の自動読み込みと式評価が未対応であることの説明に置き換える。
- `docs/architecture.md` の「デバッグ情報」: DWARF の型名（`Type::display`）、scalar typedef、union の `$tag`・`$payload`・node の形、関数名の規則、
  formatter の場所と `vsc/resources/lldb/` への複製。
- `README.md`: 検証コマンドの `node tests/debug_info.mjs` の次に `node tests/debugger.mjs target/release/tsuzuri # lldb required; TSUZURI_LLDB overrides it`。
- `_docs/feature-status.md` と `_features/README.md` の G16 の状態（GUIDE §8）。

## 受け入れ条件

- [ ] `tests/debug_info.rs` の 7 件、`tests/debug_info.mjs`、`tests/debugger.mjs` が macOS arm64 で成功する。
- [ ] LLDB で「例」の期待出力が得られ、未初期化の値でも formatter が例外を出さない。
- [ ] frame と breakpoint に `Main.show` 形式の名前が使われ、`linkageName` を出さない。
- [ ] `thread step-in` が束縛の後で前の行へ戻らず、runtime と生成 helper に入らない。
- [ ] `-g` なしの IR が native・wasm32 × `-O0`・`-O3` で byte 単位で不変。
- [ ] VS Code の debug 起動で formatter が自動で読み込まれ、installed テストが成功する。
- [ ] DWARF の形と名前の規則が `docs/architecture.md` に書かれている。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 関数の先頭行で止めると局所変数は未初期化で、HEAD でも `point.x = 8174993624` のようなごみが出る。期待値の確認は 24 行目で行い、
  formatter は長さ・tag を必ず上限と照合する（8 行目の確認がこれを固定する）。
- macOS は `run` の前に `target symbols add <app>.dwarf` が要る。Linux は DWARF が実行ファイルにあるので付けない。
- `command script import` は file 名を module 名にする。登録文字列の `tsuzuri_lldb.` を file 名と一致させる。
- `--recognizer-function` は LLDB 15 以降。CodeLLDB は自分の LLDB を同梱するので、手順 10 で CodeLLDB の debug console に
  `script lldb.SBDebugger.GetVersionString()` を入力して版を確かめる。
- LLDB は `2.0` を `2` と表示する。期待値は `Rect(1.5, 2)`。
- 再帰 union の node は `nodes` に登録してから member を作る。順序を逆にすると `Tree` の payload から自分を辿って無限再帰する。
- `$payload` の offset を DWARF 側で推測して合わせない。`union_layout` と node 定義（`{ ptr, ptr, ptr, i32, {fields} }`）から計算し、
  `debug_union_offsets_match_storage` で native と wasm32 を固定する。
- `linkageName` を消しても symbol 表の `tz.fn.Main.show` は残る。`breakpoint set -n tz.fn.Main.show` も引き続き使える。

## 対象外

- Tsuzuri の式評価（watch 式の完全な評価）、逆実行、GDB の pretty printer（D11）。
- WASM のデバッガーでの表示（D10）、`-O3` での変数の保持、decimal の十進表示。
- Phase 2（Test Explorer からのデバッグ）と Phase 3（PDB・natvis）。
- DWARF の言語コードの変更（D2）。

## 決定事項

### D1: 範囲

- 決定: Phase 1 は DWARF の改善、LLDB の Python formatter、VS Code での自動読み込み、LLDB の batch テスト。Phase 2・3 は人間が求めた場合だけ着手する。
- 理由: Phase 1 だけで macOS と Linux の LLDB・CodeLLDB の表示が完結し、Phase 2 は test runner、Phase 3 は G10 に依存する。
- 見直し（2026-10-08）: 利用者の依頼「G16…の実装をすべて完遂して。…すべてのフェーズを完了させること」により Phase 1〜3 をすべて実装した。
  Phase 2 の設計は D13、Phase 3 の設計は D14 に記す。G10 は Windows の実行確認が手元でできないため blocked のままだが、Phase 3 は CI と同じ
  windows-msvc のコード経路を型検査し、手元の LLVM 21（`clang`・`lld-link`・`llvm-readobj`・`llvm-pdbutil`）で CodeView と PDB を確かめて実装した。
- 状態: 承認済み（2026-10-08、D-41）

### D2: 言語コードと union の表現

- 決定: `DW_LANG_C` のまま、union は C の enumeration・struct・union で表す。`DW_TAG_variant_part` は使わない。
- 理由: LLDB の C 型系は variant part を Rust 向けの限定的な扱いでしか読まず、GDB・CodeView への対応も揃わない。C の形はどのデバッガーでも読め、
  formatter がなくても case 名が見える。
- 状態: 既定案（実装者はこの案に従う）

### D3: 関数名

- 決定: `linkageName` を出さず、`name` を「関数名と step 実行」の規則で作る。wrapper は symbol 名と `DIFlagArtificial`。
- 理由: LLDB は linkage 名を優先して `tz.fn.Main.show` と表示する（検証済み）。C の compile unit で linkage 名を省くのは Clang の C と同じ形。
  単相化の実体を同じ名前にすると、C++ の template と同じく名前の breakpoint が全実体に当たる。
- 見直し（2026-10-08）: (1) HEAD のラムダ式の名前は「現状」の記述と違い `$lambda.6`（`module` が `$lambda` で、`name.starts_with("$lambda")` が
  成り立たなかった）。新しい規則は `module == "$lambda"` と `is_task` で判定する。(2) テストの本体（`$test.N`）は `<モジュール>.test@<行>:<列>`
  （テスト名の位置）にした（Phase 2 のため。テスト名は空白や引用符を含みうるので、名前の breakpoint に使える形を選んだ）。(3) 組み込み関数・
  組み込みメソッド・case のコンストラクター・export の bridge（モジュール `$builtin`・`$intrinsic`・`$case`・`$export`。トラップ報告が呼び出し元へ
  帰属させるのと同じ集合）は `DISubprogram` を持たない。HEAD ではこれらに行情報があり、`Vec.push (Vec.empty()) 7` の行で step-in すると
  `$builtin.Vec.empty.5` の中（同じ行）で止まった（検証済み）。(4) 実体のラムダと callback 特殊化の両方が同じ名前を持つので、
  `image lookup -r -n '^Main\.show\.lambda@22:'` は 2 件になる（テストは件数ではなく全件の名前を確かめる）。
- 状態: 承認済み（2026-10-08、D-41）

### D4: scalar の型名

- 決定: `bool` 以外の scalar を `Type::display` 名の typedef にし、`char`・`utf8char` の基底を `DW_ATE_UTF` にする。
- 理由: LLDB は基本型の名前を無視して C の型名を表示する（`(long) input`、`(unsigned short) letter = 65`。検証済み）。typedef 名は表示に使われる。
- 見直し（2026-10-08）: (1) LLDB はポインター型の名前も無視する（`(Main.Tree.node *) tree`。検証済み）ので、名前の付くポインター（参照 `ref T`、
  ハンドル、`Rc`・`Arc`、再帰 union）も名前のないポインターへの typedef にした。(2) LLDB は `DW_ATE_UTF` 16 bit を `U+0041 u'A'` と表示するので、
  formatter が `char` を Tsuzuri のリテラルの形 `'A'`、`utf8char` を `u8'é'` で表示する（「例」の `u'A'` から変更。文字列の `"héllo"`・`u8"abc"` と揃えた）。
  (3) dyn 値の `data`・`vtable` も型のないポインターにした（`ref unit` を出さないため）。
- 状態: 承認済み（2026-10-08、D-41）

### D5: DWARF の型名と union の member 名

- 決定: 型名は HEAD と同じ `Type::display`。union の member は `$tag`・`$payload`、補助の型は `<型名>.$tag`・`<型名>.$payload`・`<型名>.node`。
- 理由: `Type::display` は source と診断に現れる名前で、すでに決定的。`$` は Tsuzuri の識別子に現れないので record の field と衝突しない。
- 見直し提案: 旧版の「型名を D-03 の正規マングリング（`Main.Pair[i64,string]`）に合わせる」は採らなかった。D-03 名は IR の symbol 用で、
  利用者が読む名前ではない。D-03 名に揃える場合は formatter の正規表現と「例」の期待出力を変える。
- 見直し（2026-10-08）: std の `Map`・`Set` の `Type::display` は `Map<i32, i32>`（`type_display` がモジュールと同名の型を短くする）で、設計の
  `^(Map\.Map|Set\.Set)<.+>$` ではない。formatter は名前の接頭辞と member 名の組で型を見分ける recognizer にした（`[i64]` と `[i64; 4]` の区別は
  正規表現では書けないため）。すべての case が値を持たない Drop union（`General(0)`）は `$payload` を持たない。
- 状態: 承認済み（2026-10-08、D-41）

### D6: formatter の場所と読み込み

- 決定: 正本は `scripts/lldb/tsuzuri_lldb.py`。`vsc/scripts/toolchain.mjs` の `resources` が `vsc/resources/lldb/` へ複製し、`debugConfiguration` が
  `initCommands` で読み込む。型 category は `tsuzuri`。`sourceLanguages: ['c']` は変えない。
- 理由: `vsc/resources/` は生成物で `.gitignore` 対象。`initCommands` は target 作成前に実行され、利用者の `initCommands` より先に置けば上書きもできる。
- 見直し（2026-10-08）: launch の組み立ては `vsc/src/core.ts` の `lldbLaunch`（単体テスト付き）に移し、Phase 2 のテストのデバッグと共有した。
  Windows のパスは LLDB の引用符の中で `\` がエスケープになるので `/` にする。CodeLLDB 1.12.3 は LLDB 22.1.8 と Python 3.12 を同梱し、
  `command script import` で読めることを手元の VS Code の統合テスト（`label = "ok"`）と、その LLDB の batch（`tests/debugger.mjs`）で確かめた。
- 状態: 承認済み（2026-10-08、D-41）

### D7: 束縛の行情報

- 決定: 束縛の `store` の位置を局所変数の宣言位置にする。生成 helper と runtime に `DISubprogram` は付けない。
- 理由: HEAD では束縛の `store` が関数本体の開始位置を持ち、step が前の行へ戻る（検証済み）。行情報のない関数は LLDB の既定で step-in の対象外になる。
- 見直し（2026-10-08）: (1) 引数の `store` は位置を持たせず（Clang と同じく prologue の一部）、`DISubprogram` の `scopeLine` を出さない。
  関数は entry から `loop` ブロックへ分岐してから引数を束縛するので、LLVM 21 の `prologue_end` は束縛の前に置かれ、名前の breakpoint と step-in が
  `input=1344` のような未束縛の値で止まっていた（検証済み）。prologue が行 0 になると LLDB は `prologue_end` の後の行 0 を飛ばすので、
  `breakpoint set -n Main.show` が `Main.show(input=40) at Main.tz:8` で止まる（手順 2 の期待どおり）。命令列は変えない。(2) 閉包の捕捉の読み出しは、
  捕捉の span（局所変数の宣言）ではなく閉包の位置にする（`FunctionEmitter::debug_span`。`let add = \offset -> ...` の step が 7 行目へ戻っていた）。
  (3) C のランタイム（`task.c`・`cpu.c`・`io.c`・`os.c`・`trap.c`・`arguments.c`）は G08 では `-g` でコンパイルしていたが、`Array.sum` からの step-in が
  `tsuzuri_cpu_sum_i64`（task.c）で止まった（検証済み）ので、`stack.c` と同じくデバッグ情報なしにした。これで「step-in はランタイムで止まらない」が
  成り立ち、D15 の `-O3` の検証の失敗も根本から直る。
- 状態: 承認済み（2026-10-08、D-41）

### D8: 配布物

- 決定: Phase 1 は VSIX とリポジトリの script だけ。G14 が done になった後の toolchain への同梱は G14 の配置規則に従う別作業にする。
- 理由: G14 は todo で、配置先が決まっていない。
- 見直し（2026-10-08）: G14 は done。`scripts/toolchain/bundle.mjs` が配布物の `share/lldb/tsuzuri_lldb.py`（Phase 3 で `share/natvis/tsuzuri.natvis` も）へ
  複製し、`verify` と `smoke.mjs` が存在を確かめる。実行ファイルではないので `bin/` ではなく `share/` に置いた（`entries` が manifest に載せる）。
  `tsuzuri toolchain info` の行は増やさない（`distribution` の行で場所が分かり、既存のテストが行数を固定している）。
- 状態: 承認済み（2026-10-08、D-41）

### D9: LLDB テストの実行場所

- 決定: `tests/debugger.mjs` は開発者が macOS arm64（LLDB がある Linux でも可）で明示的に実行し、共有 CI には足さない。LLDB がなければ失敗する。
- 理由: CI の runner に LLDB と Python の導入を足すのは費用があり、黙って skip すると回帰を見逃す。
- 見直し（2026-10-08）: `TSUZURI_LLDB` に CodeLLDB の VSIX の `extension/lldb/bin/lldb`（LLDB 22.1.8）を渡しても成功する。VS Code の統合テスト
  （CI の macOS・Linux・Windows x64 で実行）は formatter の表示（`label = "ok"`）と Phase 2 の Debug のプロファイルも確かめる。
- 状態: 承認済み（2026-10-08、D-41）

### D10: WASM

- 決定: WASM は DWARF を同じ規則で出して `llvm-dwarfdump --verify` だけを確かめる。デバッガーでの表示は対象外。
- 理由: WASM の DWARF を読むデバッガー（ブラウザー拡張、wasmtime と LLDB）は formatter の仕組みが異なり、既定の WASM import も増やせない（D-18）。
- 状態: 既定案（実装者はこの案に従う）

### D11: GDB の pretty printer

- 決定: Phase 1 では作らない。需要が出たら同じ DWARF 形（`$tag`・`$payload`）を読む GDB 用 script を別チケットで追加する。
- 理由: 旧版の既定案（LLDB の後に需要を見て追加する）を維持する。VS Code 拡張は CodeLLDB を使う。
- 状態: 既定案（実装者はこの案に従う）

### D12: `-O3`

- 決定: `-O3` は DWARF の検証だけを行い、LLDB の表示は保証しない。
- 理由: 最適化で変数と行が消えるのは C/C++ と同じで、表示を保証するには最適化を変える必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D13: テストのデバッグ（Phase 2）

- 決定: `tsuzuri test ROOT --index N -g -o PATH` は、テスト N だけをルートにしたテストランナーをデバッグ情報付きで PATH にビルドし、実行しない
  （macOS は `PATH.dwarf`、Windows の MSVC リンクは `PATH` の拡張子を `.pdb` にした PDB）。ランナーはそのテストだけを持つので引数は `0`。標準出力に
  `<index> <module>.<name>: <program> 0`、`--json` では `{"type":"debug","index","module","name","program","arguments"}` を出す。`-g` と `-o` の片方だけ、
  `--index` が 1 つでない、`--list`・`--filter`・WASM との組み合わせは E2000。ランナーの C の入口（`test-runner.c`）とランタイムはデバッグ情報なしの
  別 object にし、`-g` でリンクする（step-in が `main` に入らない）。テストを呼ぶ `@tsuzuri_test_run` は `@main` と同じく `debug::wrapper` で artificial な
  `DISubprogram`（位置はテストの宣言）を持ち、呼び出しと `ret` に位置を付ける。出力は `publish_outputs` で原子的に置き、ソースとリンク入力を
  上書きしない。VS Code の Testing ビューに Debug のプロファイル（既定）を足し、選んだ 1 件をこのコマンドで `<root>/.tsuzuri/test/runner` に
  ビルドし、`lldbLaunch`（D6）で CodeLLDB を起動する。終了コード 0 で passed、それ以外は failed、終わる前に止めると skipped。2 件以上を選ぶと
  エラー（全件 skipped）にする。
- 理由: test runner は各テストを別 process で実行し、`run_tests` の build は一時ディレクトリで完結しているので、debugger が起動できる実行ファイルを
  残す入口が要る。`build` に `--test` を足すより、テストの選択（`--index`）を既に持つ `test` に寄せる方が CLI の意味が一つに決まる。1 件だけを
  ルートにすると build が速く、ランナーの引数が常に `0` になる。
- 見直し（2026-10-08、独立レビュー）: 当初は `@tsuzuri_test_run` にデバッグ情報がなく、`-O1` 以上ではテストの本体がそこへ inline されて LLVM が
  行表を捨てていた（`dsymutil` が "no debug symbols in executable"、本体のブレークポイントが pending のまま）。`@main` と同じ `debug::wrapper` で
  artificial な subprogram と呼び出しの位置を付けて直した。代案のうち「選んだテストを `noinline` にする」は最適化の結果を変え、「`-O0` に固定する・
  拒否する」は最適化したテストを調べられないので採らない。`-g` なしのランナーの IR は変わらない。副作用として、テストの最後の行から先へステップ
  実行するとテストの宣言の行（`tsuzuri_test_run`）で一度止まる（プログラムの `main` の wrapper と同じ）。以前は `dyld` などの機械語まで抜けていた。
- 状態: 承認済み（2026-10-08、D-41）

### D14: Windows の CodeView・PDB・natvis（Phase 3）

- 決定: Windows で MSVC のリンカーを使う Clang（`driver::msvc_linker`。配布物の `tsuzuri-clang` は MinGW の `ld.lld` でリンクし DWARF を CodeLLDB で
  読むので除く）では、driver が `-g` のネイティブの IR に `llvm::with_codeview` で `CodeView` の module flag を足す（Clang は IR の入力に module flag を
  足さない）。実行ファイルとテストランナーのリンクに `-Xlinker` で `/PDB:<一時>`、`/PDBALTPATH:<出力名>.pdb`、`/NATVIS:<一時>/tsuzuri.natvis` を渡し、
  PDB を `<出力の拡張子を .pdb にした名前>` の sidecar として公開する（PDB 名をキャッシュのキーに含める）。natvis は `src/runtime/tsuzuri.natvis`
  （埋め込み）を正本とし、配布物の `share/natvis/` にも置く。`string`・`utf8string`・`Vec<*>`・`Map<*,*>`・`Set<*>`・`Maybe<*>`・`Result<*,*>`・
  `Task<*>` を定義し、`$tag`・`$payload` を DWARF と同じ名前で参照する。DWARF は image に残す。
- 理由: CodeView の型と関数の名前は DWARF の metadata から作られるので Tsuzuri の名前のまま（`llvm-readobj --codeview`・`llvm-pdbutil` で確認）。
  `lld-link /debug` は PDB を作りつつ DWARF の section も image に残すので、CodeLLDB の経路を壊さない。`/PDBALTPATH` がないと image は一時ディレクトリの
  PDB を指す。natvis の型名のワイルドカードはテンプレートの引数にしか使えないので、配列 `[T]`・リスト `[|T|]`・利用者の union は natvis では
  一般に書けない（プログラムごとの natvis の生成は、手元で表示を確かめられないので見送った）。`-Wl,` はパスのカンマで分かれるので `-Xlinker` を使う。
  配布物の経路に CodeView を足さないのは、Zig のリンカーの CI での回帰の危険を避けるため。
- 状態: 承認済み（2026-10-08、D-41）

### D15: `-O3` の `llvm-dwarfdump --verify` の失敗の原因

- 決定: 根本原因は、C のランタイムを Clang の既定（macOS 15 以降と Linux では DWARF 5）でコンパイルしていたこと。macOS の `--emit object` は
  `llvm-link` で IR を結合するので、module flag の `Dwarf Version` が Max の規則で 5 になり、`-O3` で LLVM 21 の DWARF 5 が `DW_OP_convert` の基本型
  `DW_ATE_signed_32` などを `.debug_names` に索引しないまま出力して検証に失敗した（`task.c` だけを `clang -O3 -g` でコンパイルしても同じ失敗になり、
  `-gdwarf-4` では通ることを確かめた）。D7 (3) でランタイムをデバッグ情報なしにしたので、プログラムの DWARF は Tsuzuri の compile unit（DWARF 4）
  だけになり、`node tests/debug_info.mjs` は `-O0`・`-O3` の native と wasm32 で通る。
- 理由: ランタイムの DWARF の版を Tsuzuri に揃える案（`-gdwarf-4`）も試して通ったが、ステップ実行がランタイムに入らないこと（D7）と両立するのは
  デバッグ情報を出さない方だけだった。
- 状態: 承認済み（2026-10-08、D-41）

## 実装と検証（2026-10-08）

利用者の依頼（GUIDE D-41）により Phase 1〜3 をすべて実装した。作業は worktree `wt/g16`（`Phase7-5` @ `1182045` から）。決定の見直しは「決定事項」の
`見直し（2026-10-08）` と D13〜D15 にある。

### 実装した内容

- Phase 1（手順 1〜12）: `src/llvm_debug.rs` の DWARF の形（スカラーの typedef、`char`・`utf8char` の `DW_ATE_UTF`、名前の付くポインターの typedef、
  型のないポインターと関数ポインター、リストのノード `<型名>.node`、union の列挙型・`$tag`・`$payload`・再帰 union のノード）、`linkageName` のない
  関数名（単相化は同名、`lambda@`・`task@`・`test@`、ラッパーは `DIFlagArtificial`）、glue の `DISubprogram` なし、束縛の `store` の位置、閉包の捕捉の
  位置、`scopeLine` なし。driver は C のランタイムをデバッグ情報なしでコンパイルする（D7・D15）。formatter `scripts/lldb/tsuzuri_lldb.py`（recognizer で
  型を見分け、文字列・配列・スライス・`Vec`・リスト・union・`Map`・`Set`・文字・`unit`・関数値・Task を表示）。VS Code は `initCommands` で読み込み、
  VSIX の `resources/lldb/` と配布物の `share/lldb/` に同梱する。`tests/fixtures/debug_view`、`tests/debugger.mjs`（新規）、`tests/debug_info.rs`
  （5 件追加）、`tests/debug_info.mjs`（debug_view の DWARF の検査）。
- Phase 2（D13）: `tsuzuri test ROOT --index N -g -o PATH`（`driver::build_debug_runner`）、Testing ビューの Debug のプロファイル。
- Phase 3（D14）: `llvm::with_codeview`、MSVC リンクの PDB・natvis（`src/runtime/tsuzuri.natvis`、配布物の `share/natvis/`）、PDB 名のキャッシュのキー。

### 確認したコマンドと結果（macOS arm64、Apple M1 Max、rustc 1.98.1、LLVM 21.1.8、Node v20.19.6）

- `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`: 81 binaries、858 passed、0 failed（調整役の指示どおり、既存の
  `bounds_computation_syntax_and_expansion` の 2 MiB の stack 溢れを避けるため）。負荷の高い状態で一度 `driver::test_runner::tests::
  runner_rejects_malformed_indices_and_reaps_timed_out_children`（合格するテストの 1 秒の上限）が失敗したが、単独と再実行では成功した（既存の時間依存）。
- GUIDE §3.1 の回帰テスト（既定の stack）: `bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・
  `bounds_nested_builder_expansion_not_just_source_syntax`・`honors_the_exact_specialization_limit` がそれぞれ 1 passed。
- `cargo test --locked --test debug_info`: 8 passed（既存 2 + 手順 2〜6 の 5 + Phase 3 の 1）。`--test test_runner`: 9 passed（Phase 2 の 1 件を追加）。
  `--bin tsuzuri`: 11 passed（`rejects_ambiguous_or_unused_arguments` に Phase 2 の組み合わせを追加）。`--lib pdb_links`: 1 passed。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`: 成功。Windows の型検査（rustup の stable 1.96.1）:
  `cargo check --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` が成功。同じ clippy は既存の `src/lsp.rs`・`src/parser.rs` の
  `nonminimal_bool` だけで失敗し（base でも同じ）、`-A clippy::nonminimal_bool` では成功。
- `-g` なしの IR: base のコンパイラと比べ、`tests/fixtures/*` と `examples/*` の 76 プロジェクト × native・wasm32 × `-O0`・`-O3` × `--trap-info` の有無の
  608 ファイルがすべて byte 単位で同一（`debug_view` は同じ内容で base と比較）。
- `-g` の IR: 同じ 76 プロジェクト × native・wasm32 で 2 回の出力が同一（決定的）で、`clang -c -g` が受け付け、object の `llvm-dwarfdump --verify` が成功。
- `node tests/debug_info.mjs`（LLVM 21 を PATH の先頭に置く）: `-O0`・`-O3` の native と WASM が成功（`-O3` の既存の失敗を D15 で修正）。
- `node tests/debugger.mjs`: Apple LLDB lldb-2103 と、CodeLLDB 1.12.3 の LLDB 22.1.8（`TSUZURI_LLDB`）の両方で成功。`TSUZURI_LLDB` が無効なら
  `lldb not found; install LLDB or set TSUZURI_LLDB` で失敗する。
- `node tests/e2e.mjs`・`cache.mjs`・`examples.mjs`・`stack_overflow.mjs`・`ffi_extensions.mjs`・`wasm_memory.mjs`・`wasm_threads.mjs`: 成功。
- `sh scripts/check-runtime-includes.sh`: 34 files（`tsuzuri.natvis` で 1 増）。`node scripts/check-docs.mjs`（変更した 5 ページ）: 成功。
- VS Code（`vsc`）: `npm ci`、`npm run compile`（型検査・lint）、`npm run test:unit`（11 件。Phase 1・2 で 1 件ずつ追加）、`npm run toolchain`（キャッシュ済みの Zig と CodeLLDB）、
  `npm run test:toolchain`、`archive.mjs` と `smoke.mjs --discover`、`npm test`（VS Code 1.103.2。formatter による `label = "ok"` と Debug のテストの
  プロファイルを含む）、`npm run vsix`、`npm run test:installed` がすべて macOS arm64 で成功。
- Phase 3 の手元の確認: `with_codeview` の IR を `x86_64`・`aarch64-pc-windows-msvc` へコンパイルし、`llvm-readobj --codeview` が `Main.show`・`Main.Shape`・
  `$tag`・`$payload`・`[i64]` を示した。`lld-link /debug /pdb /pdbaltpath /natvis /force:unresolved` で PDB ができ、`llvm-pdbutil` が手続き 8 件と
  `Main.*` の型 15 件、natvis の名前付き stream を示し、image の DWARF も `--verify` を通った。Clang の MSVC driver は `-debug` と `-Xlinker` の 3 引数を
  そのまま渡し、自分の `-pdb:` を足さない（`-###` で確認）。

### 手元で確かめられないこと

- Windows で `link.exe` を使うリンク（PDB の書き出し、`/PDBALTPATH`、`/NATVIS` の埋め込み）と、Visual Studio・WinDbg での natvis の表示（`$tag`・
  `$payload` の参照を含む）。CI は配布物の MinGW の経路だけを実行するので、この経路は CI でも確かめていない。CI へ確認を足さなかったのは、bash の
  runner で Clang が MSVC の `link.exe` を見つけることに確信がないため。→ レビュー後に win32-x64 の CI へ確認を足した（下の「レビュー後の修正」）。
  Visual Studio・WinDbg での natvis の表示は引き続き確かめていない。
- Windows x64 の CodeLLDB のデバッグ（Phase 1 の formatter と Phase 2 の Debug のプロファイルの統合テスト）と Linux の LLDB は CI が確かめる。

### 性能

性能は主張しない。`-g` なしの IR は変わらない。

### 更新した文書

`_tsuzuri/language-reference/compiler/debugging.md`（新規）、`compiler/usage.md`、`compiler/option.md`、`index.md`、
`built-in-types-and-modules/test.md`、`README.md`、`docs/architecture.md`、`vsc/README.md`、`vsc/CHANGELOG.md`、CLI の `--help`。
`docs/language.md` はデバッグを扱わないので変更しない。

### 変更したファイル

`src/llvm_debug.rs`、`src/llvm.rs`、`src/driver.rs`、`src/test_runner.rs`、`src/main.rs`、`src/cache.rs`、`src/runtime/tsuzuri.natvis`（新規）、
`scripts/lldb/tsuzuri_lldb.py`（新規）、`scripts/toolchain/bundle.mjs`、`scripts/toolchain/smoke.mjs`、`tests/debug_info.rs`、`tests/debug_info.mjs`、
`tests/debugger.mjs`（新規）、`tests/fixtures/debug_view/Main.tz`（新規）、`tests/test_runner.rs`、`vsc/src/core.ts`、`vsc/src/workflow.ts`、
`vsc/src/testing.ts`、`vsc/src/extension.ts`、`vsc/src/test/core.test.ts`、`vsc/src/test/extension.test.ts`、`vsc/scripts/toolchain.mjs`、
`vsc/scripts/package.mjs`、`vsc/scripts/test-extension.mjs`、上の文書、この ticket。

### レビュー後の修正（2026-10-08）

`Phase7-5`（G17・G18 との merge 後の `d0fe562`）へ fast-forward してから、独立レビューの指摘を直した。

- `-O1` 以上のデバッグ用ランナーが行表を失う: `src/llvm.rs` の `@tsuzuri_test_run` を `debug::wrapper` に通し、デバッグ情報があるときだけ artificial な
  `DISubprogram`（名前 `tsuzuri_test_run`、位置はテストの宣言）と呼び出し・`ret` の位置を付けた（D13 の見直し）。修正前のコンパイラでは `-O2` の
  ランナーで本体の `breakpoint set -f Main.tz -l 12` が pending のまま最後まで実行された。修正後は `-O0`〜`-O3` で `llvm-dwarfdump --debug-line` に
  テストの行が残り、`--verify` が成功する。
- formatter の `OverflowError`: `SequenceProvider`（`CollectionProvider` も）と `ListProvider` の `get_child_at_index` は、新しい `child_at` で番地が
  `0 <= address < 2^64` のときだけ子を作り、作れなければ `None` を返す。`ListProvider` はノードをたどる処理の例外も捕まえる。子の名前の解析は
  `isdigit` を `isdecimal` にした（`[²]` で `int` が `ValueError` を出していた）。
- Windows の CI: `.github/workflows/vscode.yml` の最後に step「Windows PDB and natvis」（win32-x64 だけ、pwsh）を足した。LLVM の `clang.exe` を
  `TSUZURI_CLANG` にして `tests/fixtures/debug_view` を `-g -O0` でビルドし、`app.pdb` があること、`app.exe` が `6` を出すこと、image が `app.pdb` を指し
  一時の名前 `artifact.pdb` を含まないこと、PDB の byte に `Main.Shape`・`Main.show`・`tsuzuri.natvis` があること、`llvm-pdbutil dump --types` に
  `` `Main.Shape` `` があること（`llvm-pdbutil.exe` がなければ notice）を確かめる。ARM64 は CodeLLDB を同梱せず、MSVC ARM64 の toolset も確かめて
  いないので対象外。手元では pwsh 7.5 で step の本文を模擬し（tsuzuri を、macOS の実行ファイルと `lld-link` の PDB を置くスクリプトに置き換え）、
  成功と 3 つの失敗の経路を確かめた。`link.exe` での実際の結果は CI だけが確かめる。
- テスト: `tests/debug_info.rs` に `debug_test_runner_dispatch_is_an_artificial_subprogram`（native・wasm32 で、`-g` なしは `!dbg` がなく、`-g` は
  artificial な subprogram と呼び出しの位置を持ち、決定的）。`tests/debugger.mjs` に、`-O2` のランナーで本体の 2 行（`fib 20` と `assert`）の
  ブレークポイントが bind して止まること、`-O0` の `bt` の `tsuzuri_test_run at Main.tz:6:6`、番地が 2^64 を超える配列とリストの provider が `None` を
  返すこと（修正前の formatter では `OverflowError` の Traceback）を足した。
- 文書: `debugging.md`（`tsuzuri_test_run` とステップ実行、壊れたポインターの行）、`usage.md`（`-O1`〜`-O3`）、`docs/architecture.md`、`--help` の
  `test --index N -g -o PATH` に `[-O0|-O1|-O2|-O3]`。

確認（macOS arm64、LLVM 21.1.8 を PATH の先頭）:

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`: 成功。
- `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast`: 82 binaries、912 passed、0 failed。GUIDE §3.1 の 4 件は既定の stack でそれぞれ 1 passed。
  `--bin tsuzuri` 12、`--test debug_info` 9、`--test test_runner` 10 passed。
- `node tests/debug_info.mjs`: `-O0`・`-O3` の native と WASM が成功。`node tests/debugger.mjs`: Apple LLDB lldb-2103 と CodeLLDB 1.12.3 の LLDB 22.1.8 で成功。
- `-g` なしの IR: `debug_view`・`tasks` × native・wasm32 × `-O0`・`-O3` × `--trap-info` の有無の 16 組が、base `1182045` とも修正前の `d0fe562` とも
  byte 単位で同一。テストランナーの IR（`emit_test_runner_with`・`emit_test_runner`、native・wasm32、選択 3 通り、seed の有無、プロパティテストを含む
  4 ソース）の 72 ファイルも `d0fe562` と同一。
- `node scripts/check-docs.mjs`（`debugging.md`・`usage.md`）: 成功。VS Code 拡張は変更していない。

変更したファイル: `src/llvm.rs`、`src/main.rs`、`scripts/lldb/tsuzuri_lldb.py`、`tests/debug_info.rs`、`tests/debugger.mjs`、
`.github/workflows/vscode.yml`、上の文書、この ticket。
