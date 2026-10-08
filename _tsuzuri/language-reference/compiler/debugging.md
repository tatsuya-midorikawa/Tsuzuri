# デバッグ

`tsuzuri build -g` は、関数・行・局所変数・型の DWARF デバッグ情報を出します。LLDB に Tsuzuri 用の表示（formatter）を読み込むと、文字列・配列・リスト・`Vec`・union・`Maybe`・`Result`・`Map`・`Set`・関数値を Tsuzuri の値として表示します。VS Code 拡張は、この formatter をデバッグの開始時に自動で読み込みます。

このページの LLDB の出力は、macOS arm64 の Apple LLDB（lldb-2103）と、VS Code 拡張が同梱する CodeLLDB 1.12.3 の LLDB 22 で確かめたものです。

## この記事のポイント

- `-g`（`--debug-info`）でデバッグ情報付きにビルドします。macOS では DWARF が実行ファイルの隣の `<出力>.dwarf` に入ります。
- LLDB では `command script import <配布物>/share/lldb/tsuzuri_lldb.py` で formatter を読み込みます。VS Code では何もしなくても読み込まれます。
- 呼び出し履歴とブレークポイントには `Main.show` のような Tsuzuri の関数名を使います。
- ステップ実行は、ランタイムとコンパイラーが生成した補助関数に入りません。
- `tsuzuri test --index N -g -o PATH` で 1 件のテストをデバッグ用にビルドできます。VS Code では Testing ビューの **Debug** です。
- Windows で MSVC のリンカーを使うビルドは、実行ファイルの隣に PDB を書き、Visual Studio の natvis の表示を埋め込みます。
- 表示を保証するのは `-O0` のネイティブだけです。`-O3` では変数が消えることがあり、WebAssembly は DWARF を保持するだけです。

## デバッグ情報付きでビルドする

`-g` を付けて `-O0` でビルドします。`run` にも `-g` を付けられます。

```sh
tsuzuri build . -g -O0 -o app
```

Linux では DWARF が実行ファイルに入るので、そのまま `lldb app` で開けます。macOS では DWARF が `app.dwarf` に入るので、プロセスを起動する前に `target symbols add` で読み込みます。

```sh
lldb -o "target symbols add app.dwarf" app
```

## LLDB で Tsuzuri の値を表示する

formatter は Python の LLDB スクリプトで、LLDB に同梱の `lldb` モジュールだけを使います。LLDB 15 以降で動きます。置き場所は次のとおりです。

| 入手方法 | 場所 |
| --- | --- |
| 配布物（`tsuzuri toolchain info` の `distribution`） | `share/lldb/tsuzuri_lldb.py` |
| リポジトリ | `scripts/lldb/tsuzuri_lldb.py` |
| VS Code 拡張 | 拡張の `resources/lldb/tsuzuri_lldb.py`。デバッグの開始時に自動で読み込みます |

LLDB で読み込みます。毎回読み込むなら、同じ行を `~/.lldbinit` に書きます。

```sh
lldb -o "command script import ~/.local/share/tsuzuri/tsuzuri-0.1.0-darwin-arm64/share/lldb/tsuzuri_lldb.py" app
```

読み込むと、型カテゴリー `tsuzuri` が有効になります（`type category list tsuzuri` で確かめられます）。次のプログラムを `-g -O0` でビルドし、最後の行で止めます。

```tsuzuri run=6
union Shape = Empty | Circle of f64 | Rect of f64 * f64
union Tree = Leaf | Node of Tree * i64 * Tree

def show :: i64 -> i64
fn show input =
    let text = "h\u{e9}llo"
    let letter = 'A'
    let shape = Rect (1.5, 2.0)
    let tree = Node (Leaf, input, Leaf)
    let outcome: Result<i64, string> = Result.Error "bad"
    let values = [1, 2, 3]
    let add = \offset -> input + offset
    let total = Array.sum (ref values)
    total + add 0 - input + (if letter == 'A' then 0 else 1) + text.length - 5

show 40
```

`frame variable` の出力です（`shape`、`tree`、`outcome` は使わない局所変数なので、ビルドでは `W1001` の警告が出ます）。

```text
(i64) input = 40
(string) text = "héllo"
(char) letter = 'A'
(Main.Shape) shape = Rect(1.5, 2)
(Main.Tree) tree = Node(Leaf, 40, Leaf)
(Result<i64, string>) outcome = Error("bad")
([i64]) values = length=3 {
  [0] = 1
  [1] = 2
  [2] = 3
}
(i64 -> i64) add = <fn Main.show.lambda@12:15>
(i64) total = 6
```

### 表示の一覧

| Tsuzuri の型 | 表示 |
| --- | --- |
| 整数・浮動小数点・decimal | 型名と値。`(i64) input = 40`。decimal は格納形式の整数のまま表示します |
| `char`・`utf8char` | Tsuzuri のリテラルの形。`'A'`、`u8'é'` |
| `unit` | `()` |
| `bool` | `true`・`false` |
| `string`・`utf8string` | `"héllo"`、`u8"abc"`。改行などの制御文字、孤立したサロゲートは `\n`・`\u{d800}` のようなエスケープにします |
| 配列・スライス・`Vec` | `length=3`（`Vec` は `length=1 capacity=4`）と、要素 `[0]`、`[1]`… |
| リスト | `length=2` と、先頭から順の要素 `[0]`、`[1]`… |
| record・タプル | LLDB の既定の表示（フィールド名と値） |
| すべての case が値を持たない union | case 名。`Green` |
| 値を持つ case がある union（`Maybe`・`Result` を含む） | `Rect(1.5, 2)`、`Some(40)`、`None`、`Error("bad")`。子は有効な case の値だけ |
| 再帰する union | `Node(Leaf, 40, Leaf)`。値を持たない case はノードを持たず、その case 名を表示します |
| `Map`・`Set` | `size=1` と、キー順の要素 `[0]`、`[1]`… |
| 関数値 | `<fn Main.show.lambda@12:15>` のように呼び出す関数の名前 |
| `Task` | `<task>` |

LLDB は浮動小数点の `2.0` を `2` と表示します。formatter を読み込まない LLDB や GDB でも、DWARF だけで整数などの型名、値を持たない union の case 名、union の `$tag` の case 名が読めます。union の DWARF は、タグを `$tag`、値を `$payload`（case 名をメンバー名とする C の union）に持つ構造体です。`$` は Tsuzuri の識別子に使えないので、record のフィールドとは衝突しません。

### 読めない値と上限

関数の先頭で止めたときの局所変数のように、まだ値を書いていない場所も formatter は例外を出さずに表示します。

| 状況 | 表示 |
| --- | --- |
| メモリを読めない | `<unreadable>` |
| 長さが負、または 2^32 以上 | `<invalid length N>`（子なし） |
| union のタグがどの case でもない | `<invalid tag N>`（子なし） |
| 文字の値が U+10FFFF を超える | `<invalid character N>` |

`string` は先頭 1,024 コード単位、`utf8string` は 1,024 バイトで切り、`...` を付けます。子の数は LLDB の上限（`target.max-children-count`、既定 256）までです。union の入れ子は 3 段まで表示し、4 段目は `Wrap(...)` のように省きます。

## 関数の名前

デバッグ情報の関数名は、リンカーのシンボル名（`tz.fn.Main.show`）ではなく Tsuzuri の名前です。呼び出し履歴にこの名前が出て、`breakpoint set -n Main.show` で止められます。シンボル名でも止められます。

| 関数 | 名前 |
| --- | --- |
| 関数 | `Main.show` |
| ジェネリックな関数の各実体 | 元の関数と同じ `Array.sum`。名前のブレークポイントがすべての実体に当たります |
| ラムダ式 | `<外側の関数>.lambda@<行>:<列>`。例: `Main.show.lambda@12:15` |
| `task` | `<外側の関数>.task@<行>:<列>` |
| `test` の本体 | `<モジュール>.test@<行>:<列>`（テスト名の位置） |
| `export def` の公開関数・`main` などの入口 | シンボル名（`tz_view`、`main`、`tsuzuri_main`）。コンパイラーが作った関数として印が付きます |

## ステップ実行

- `let` の束縛は宣言の位置にあります。束縛の後で前の行へ戻りません。
- 関数のブレークポイントと関数へのステップインは、引数を束縛した後の本体の最初の行で止まります。
- ランタイム（C で書いたタスク・入出力などと、LLVM IR のメモリ管理）と、コンパイラーが生成した補助関数（組み込み関数のラッパー、case のコンストラクター、関数値の呼び出しアダプター、解放と複製）はデバッグ情報を持たないので、LLDB の既定の設定ではステップインで入りません。
- 標準ライブラリの関数（`Array.sum` など）にはデバッグ情報があり、ステップインで入ります。標準ライブラリのソースはコンパイラーに埋め込まれているので、デバッガーは `<プロジェクト>/std/Array.tz` のソースを見つけられず、逆アセンブルを表示します。

## テストをデバッグする

`tsuzuri test` に `--index N`、`-g`、`-o PATH` を付けると、テスト N だけを入れたテストランナーをデバッグ情報付きでビルドし、実行せずに終わります。ランナーは引数 `0` でそのテストを実行します。標準出力（`--json` なら `type` が `debug` の JSON）に、ランナーの絶対パスと引数が出ます。

```sh
tsuzuri test . --index 1 -g -o .tsuzuri/test/runner
lldb -o "command script import <配布物>/share/lldb/tsuzuri_lldb.py" -o "target symbols add .tsuzuri/test/runner.dwarf" -- .tsuzuri/test/runner 0
```

`target symbols add` は macOS だけです。テストの本体の関数名は `<モジュール>.test@<行>:<列>`（テスト名の位置）で、テストの本体に置いたブレークポイントで止まります。ランナーの入口（`main` と `tsuzuri_test_run`）はデバッグ情報を持たないので、ステップ実行で入りません。`assert` が失敗すると、デバッガーはトラップした位置で止まります。

VS Code 拡張では、Testing ビューでテストを 1 件選んで **Debug** を押すと、拡張がこのコマンドでランナーを `<プロジェクト>/.tsuzuri/test/runner` にビルドし、CodeLLDB で起動します。formatter と macOS の DWARF の読み込みは、プロジェクトのデバッグと同じです。ランナーが 0 で終われば成功、それ以外は失敗として結果に残ります。終わる前にデバッグを止めると、テストはスキップになります。

## Windows（PDB と natvis）

Windows で、MSVC のリンカー（`link.exe`）でリンクする Clang（配布物の `tsuzuri-clang` 以外。たとえば `TSUZURI_CLANG` に指定した LLVM の `clang`）を使うと、`-g` のネイティブのビルドは DWARF に加えて CodeView も出します。実行ファイルでは、リンカーが出力の拡張子を `.pdb` にした PDB（`app.exe` なら `app.pdb`）を隣に書きます。実行ファイルは PDB をディレクトリなしの名前で指すので、Visual Studio や WinDbg は隣の PDB を見つけます。PDB の関数と型の名前は DWARF と同じ Tsuzuri の名前（`Main.show`、`Main.Shape`、`$tag`）です。`tsuzuri test --index N -g -o PATH` のランナーにも PDB を書きます。

PDB には、Tsuzuri の値の表示を定義した natvis が埋め込まれます。natvis は `string`、`utf8string`、`Vec`、`Map`、`Set`、`Maybe`、`Result`、`Task` を Tsuzuri の値として表示します。natvis の型名のワイルドカードはテンプレートの引数にしか使えないので、配列、リスト、利用者の union には natvis の表示がありません。これらはデバッガーの既定の表示になり、union の `$tag` は case 名を表示します。

`--emit object` と `--emit llvm` の `-g` も CodeView を含みます。自分で MSVC のリンカーでリンクするときは、`/DEBUG` と `/NATVIS:<配布物>/share/natvis/tsuzuri.natvis` を付けます。

配布物と VS Code 拡張は MinGW の `ld.lld` でリンクするので、PDB を書かず、実行ファイルの DWARF を CodeLLDB と LLDB で読みます（このページの LLDB の表示）。

## 最適化と WebAssembly

- `-O3` でも DWARF は正しく、`llvm-dwarfdump --verify` を通ります。ただし最適化で変数や行が消えることがあります。消えた変数は LLDB の既定の表示（`<variable not available>` など）になります。
- `--target wasm32` の `-g` は DWARF を `.debug_info` などのカスタムセクションに残します。WebAssembly のデバッガーでの表示は対象外です。

## まとめ

- `-g -O0` でビルドし、LLDB で `command script import .../tsuzuri_lldb.py` を実行すると Tsuzuri の値で表示されます。VS Code では自動です。
- 関数名は `Main.show` の形で、ジェネリックな関数の実体は同じ名前です。
- ステップ実行は束縛で前の行へ戻らず、ランタイムと補助関数に入りません。
- `tsuzuri test --index N -g -o PATH` と VS Code の **Debug** で、1 件のテストをデバッガーで実行できます。
- Windows の MSVC のリンカーは、隣に PDB を書き、natvis を埋め込みます。

## 関連項目

- [コンパイラ オプション](option.md)
- [コンパイラの使い方](usage.md)
- [テスト](../built-in-types-and-modules/test.md)
- [WebAssembly への出力](webassembly.md)
- [言語リファレンスの目次](../index.md)
