# OS API: ファイル・環境・時刻・乱数・プロセス

[ドキュメントのトップ](../README.md)

標準モジュールの `File`、`Dir`、`Path`、`Env`、`Time`、`Random`、`Process`、`Os` は、ファイル、ディレクトリ、環境変数、時計、乱数、子プロセスを扱います。OS に触れる操作はすべて [IO](io.md) のアクションで、ファイルがない、権限がないといった通常の失敗は例外ではなく、`Result.Result<T, Os.Error>` の `Error` として返します。メモリの確保失敗のような回復できない失敗は Result にせず、トラップします。

| モジュール | 内容 |
| --- | --- |
| `File` | ファイル全体の読み書き、ハンドル、メタデータ |
| `Dir` | 一覧、作成、削除、再帰的な走査 |
| `Path` | パス文字列の操作（OS に触れない純粋な関数） |
| `Env` | コマンドライン引数、環境変数、作業ディレクトリ |
| `Time` | 単調時計、壁時計、sleep |
| `Random` | OS の乱数と、決定的な PCG（PCG は純粋な関数） |
| `Process` | シェルを介さない子プロセスの実行 |
| `Os` | エラーの型と UTF-8 の変換 |

これらの名前は標準ライブラリ用に予約されています。同名の `File.tz` などを置くと `E1011` なので、以前に使っていたモジュールは改名してください。低水準の `Os.__read` などは標準モジュールの内部用で、ほかから参照すると `E1022` です。

## 実行されるまで何も起きない

OS に触れる関数は、呼んだだけでは何もしません。アクションを作っても OS には触れず、入口が返す IO の中で `let!` や `do!` が実行した時点で処理します。

```tsuzuri
def main :: IO<unit> =
    let _unused = File.write_text "never.txt" "not written"
    do! IO.write_line "done"
```

この例は `done` だけを表示し、`never.txt` は作りません。path やテキストの引数は所有値としてアクションへ渡すので、後でも必要な `string` は先に複製してください。

IO ブロックの `match` の腕では `do!` を使えません。結果を調べる `match` や `while` は名前付きの関数へ移し、ブロックには `let!` と `do!` を並べます。次の例の `save` のように、腕が別の IO アクションを返す形にすると、実行を分岐できます。一つの IO ブロックに文を並べ続けると展開の深さの上限（128）に達するので、25 文前後を目安に関数へ分けます。

## 読んで書く例

`input.txt` を読み、行数を `lines.txt` へ書きます。失敗は `Os.message` で文にして stderr へ出します。

```tsuzuri
def count_lines :: ref string -> i64
fn count_lines text =
    let mut lines = 0
    let mut index = 0
    while index < text.length do
        if text[index] == 10i16u then lines = lines + 1
        index = index + 1
    lines

def written :: Result.Result<unit, Os.Error> -> IO<unit>
fn written result =
    match result with
    | Result.Ok _ -> IO.write_line "wrote lines.txt"
    | Result.Error error -> IO.write_error_line ("write failed: " + Os.message (ref error))

def save :: Result.Result<string, Os.Error> -> IO<unit>
fn save loaded =
    match loaded with
    | Result.Ok text ->
        let count = count_lines (ref text)
        IO.bind (File.write_text "lines.txt" (to_string count + "\n")) written
    | Result.Error error -> IO.write_error_line ("read failed: " + Os.message (ref error))

def main :: IO<unit> =
    let! loaded = File.read_text "input.txt"
    do! save loaded
```

`File.read_text` の `IO<Result<string, Os.Error>>` を `let!` で束縛し、`save` が Result を `match` します。`main` は二文だけで、行を数える `while` も `match` も関数に置いています。

独立した `target/os-demo/Main.tz` を、入力のあるディレクトリで実行します。相対 path の起点は作業ディレクトリです。

```sh
mkdir -p target/os-demo-data && cd target/os-demo-data
printf 'one\ntwo\nthree\n' > input.txt
../../target/release/tsuzuri run ../os-demo
cat lines.txt
```

`wrote lines.txt` を表示し、`lines.txt` には `3` を書きます。`input.txt` がなければ stderr に `read failed: not found (os error 2)` を書きますが、入口は `IO<unit>` なので終了コードは 0 です。失敗を終了コードで知らせる方法は[終了コード](#終了コード)を参照してください。

## Os とエラー

失敗は `Os.Error` で表します。種類の `kind` と、システム自身のコードの `code` を持つレコードです。

| API | 契約 |
| --- | --- |
| `Os.Error` | `kind: Os.ErrorKind` と `code: i32`。`code` は native では `errno`、WASI では WASI の errno。Tsuzuri 自身が見つけた失敗（NUL を含む path など）は 0 |
| `Os.ErrorKind` | `NotFound`、`PermissionDenied`、`AlreadyExists`、`InvalidInput`、`InvalidEncoding`、`Interrupted`、`Other`。case の追加はメジャー版だけ |
| `Os.message error` | `ref Os.Error` から `not found (os error 2)` の形の文を作る。`code` が 0 なら `(os error ...)` を付けない |
| `Os.encode text` | `ref string` を UTF-8 の `Result<utf8string, Os.Error>` にする。孤立サロゲートは `InvalidEncoding` |
| `Os.decode bytes` | `[ubyte]` を UTF-8 として `Result<string, Os.Error>` にする。不正な UTF-8 は `InvalidEncoding` |

`Os.error_of_status`、`Os.split_names`、`Os.decode_i64` は、標準モジュールが runtime の戻り値を解く補助で、通常は呼びません。

種類で分岐するには、`kind` を `match` します。次の例は、設定ファイルがないときだけ既定値に進みます。

```tsuzuri
def settings :: Result.Result<string, Os.Error> -> string
fn settings result =
    match result with
    | Result.Ok text -> text
    | Result.Error error ->
        match error.kind with
        | Os.NotFound -> "defaults"
        | _ -> "cannot read settings: " + Os.message (ref error)

def main :: IO<unit> =
    let! loaded = File.read_text "settings.txt"
    do! IO.write_line (settings loaded)
```

## File

path と内容は `string` で、OS との境界では UTF-8 です。相対 path は作業ディレクトリが起点です。

### 全体の読み書き

| API | 結果 | 契約 |
| --- | --- | --- |
| `File.read_bytes path` | `IO<Result<[ubyte], Os.Error>>` | ファイル全体を読む |
| `File.read_text path` | `IO<Result<string, Os.Error>>` | 全体を UTF-8 として読む。BOM は残す。不正な UTF-8 は `InvalidEncoding` |
| `File.write_bytes path data` | `IO<Result<unit, Os.Error>>` | 作成または切り詰めて書く。途中で失敗すると書きかけのファイルが残る |
| `File.write_text path text` | `IO<Result<unit, Os.Error>>` | 同じく UTF-8 のテキストを書く。孤立サロゲートはファイルに触れる前に `InvalidEncoding` |
| `File.append_text path text` | `IO<Result<unit, Os.Error>>` | 末尾へ追記する。ファイルがなければ作る |
| `File.remove path` | `IO<Result<unit, Os.Error>>` | ファイルまたはシンボリックリンクを消す。リンク先は消さない |

`File.remove` にディレクトリを渡すと、Linux は `InvalidInput`（`EISDIR`）、macOS は `PermissionDenied`（`EPERM`）です。ディレクトリは `Dir.remove` で消します。

### ハンドル

大きなファイルを少しずつ読む、続けて書くといった処理にはハンドルを使います。

| API | 結果 | 契約 |
| --- | --- | --- |
| `File.open path mode` | `IO<Result<File.Handle, Os.Error>>` | ファイルを開く。開いているファイルをもう一度開いてもよく、ハンドルは別になる |
| `File.read handle count` | `IO<Result<[ubyte], Os.Error>>` | 最大 `count` バイトを読む。空配列がファイルの終わりで、`count` より短くても終わりとは限らない。`count` が負か 2^30 を超える場合と、`File.Read` で開いていないハンドルは `InvalidInput` |
| `File.write handle data` | `IO<Result<unit, Os.Error>>` | 全バイトを書く。`File.Read` で開いたハンドルは `InvalidInput` |
| `File.flush handle` | `IO<Result<unit, Os.Error>>` | ハンドルを検査する。Tsuzuri は書き込みをバッファしないので、`write` した内容はすでに OS に渡っている |
| `File.close handle` | `IO<Result<unit, Os.Error>>` | 閉じる。システムが失敗を返してもハンドルは無効になるので、二度閉じると `InvalidInput` |
| `File.with_open path mode body` | `IO<Result<'a, Os.Error>>` | 開いて `body handle` を実行し、`body` の結果が失敗でも閉じてから返す。値は close が失敗しない限り `body` の値 |

`mode` は `File.Mode` で、`File.Read` は既存のファイルを読み、`File.Write` は作成または切り詰め、`File.Append` は作成または末尾へ追記、`File.CreateNew` は存在してはならない新規作成です。

`File.Handle` は不透明な Copy の値で、実行時の世代付きの表を指します。閉じたハンドルが別のファイルと取り違えられることはなく、閉じた後の使用や二重の close は `InvalidInput`（`code` は `EBADF`）で、未定義動作にはなりません。ただし `Drop` 型ではないので、スコープを抜けても自動では閉じません。`File.close` を呼ぶか `File.with_open` を使ってください。閉じ忘れたハンドルはプロセスの終了で閉じます。開いたハンドルは close-on-exec で、`Process.run` の子プロセスへは渡りません。

次の例は、書き込みが失敗しても `File.close` を呼び、最初の失敗を返します。

```tsuzuri
def first_error :: Result.Result<unit, Os.Error> -> Result.Result<unit, Os.Error> -> Result.Result<unit, Os.Error>
fn first_error written closed =
    match written with
    | Result.Ok _ -> closed
    | Result.Error error -> Result.Error error

def write_and_close :: File.Handle -> IO<Result.Result<unit, Os.Error>>
fn write_and_close handle = IO {
    let! written = File.write handle [104ubyte, 105ubyte, 10ubyte]
    let! closed = File.close handle
    return first_error written closed
}

def start :: Result.Result<File.Handle, Os.Error> -> IO<Result.Result<unit, Os.Error>>
fn start opened =
    match opened with
    | Result.Ok handle -> write_and_close handle
    | Result.Error error -> IO.pure (Result.Error error)

def exit_code :: Result.Result<unit, Os.Error> -> IO<i32>
fn exit_code result =
    match result with
    | Result.Ok _ -> IO.pure 0i32
    | Result.Error error -> IO.bind (IO.write_error_line (Os.message (ref error))) (\() -> IO.pure 1i32)

def main :: IO<i32> =
    let! opened = File.open "out.bin" File.Write
    let! result = start opened
    return! exit_code result
```

`File.with_open` は同じ後始末を引き受けます。結果は二重の Result で、外側は open と close の失敗、内側は `body` 自身の結果です。

```tsuzuri
def header_text :: Result.Result<Result.Result<[ubyte], Os.Error>, Os.Error> -> string
fn header_text result =
    match result with
    | Result.Ok (Result.Ok bytes) -> "read " + to_string bytes.length + " bytes"
    | Result.Ok (Result.Error error) -> "read failed: " + Os.message (ref error)
    | Result.Error error -> "open or close failed: " + Os.message (ref error)

def main :: IO<unit> =
    let! result = File.with_open "data.bin" File.Read (\handle -> File.read handle 16)
    do! IO.write_line (header_text result)
```

### メタデータ

| API | 結果 | 契約 |
| --- | --- | --- |
| `File.metadata path` | `IO<Result<File.Metadata, Os.Error>>` | シンボリックリンクをたどった先の情報 |
| `File.link_metadata path` | `IO<Result<File.Metadata, Os.Error>>` | パス自身の情報。シンボリックリンクはたどらない |

`File.Metadata` は `kind: File.EntryKind`、`size: i64`、`modified_ns: i64` のレコードです。`modified_ns` は 1970-01-01 UTC からのナノ秒で、`i64` に収まらない時刻は `i64` の限界に丸めます。`File.EntryKind` は `Regular`、`Directory`、`Symlink`、`Other` で、`Symlink` は `File.link_metadata` からだけ返ります。`Other` はデバイス、ソケット、パイプなどです。壊れたリンクは `File.metadata` が `NotFound`、`File.link_metadata` が `Symlink` を返します。

存在だけを調べる API はありません。確認から使うまでの間に状態が変わりうるので、操作を直接実行して `Error` を処理するか、`File.metadata` の結果が `NotFound` かどうかで判断します。

## Dir

| API | 結果 | 契約 |
| --- | --- | --- |
| `Dir.list path` | `IO<Result<[string], Os.Error>>` | 名前だけを返す。`.` と `..` は含めず、UTF-8 のバイト順に整列する。名前が一つでも不正な UTF-8 なら呼び出し全体が `InvalidEncoding` |
| `Dir.create path` | `IO<Result<unit, Os.Error>>` | ディレクトリを一つ作る。親は作らず、既存なら `AlreadyExists` |
| `Dir.remove path` | `IO<Result<unit, Os.Error>>` | 空のディレクトリだけを消す。空でなければ `Other`（`ENOTEMPTY`）。シンボリックリンクは `File.remove` で消す |
| `Dir.walk path` | `IO<Result<[string], Os.Error>>` | 下にあるすべてを、`path` からの相対パス（`/` 区切り）で返す。各エントリの直後にその中身が続き、同じディレクトリの中は `Dir.list` の順。シンボリックリンクは載せるがたどらない。途中で失敗するとその失敗を返す |

整列はロケールや UTF-16 の順ではありません。たとえば U+FF61 は U+1F600 より前に並びます。`Dir.walk` は一部の結果だけを返すことはなく、読めないディレクトリが一つあれば全体が `PermissionDenied` などの失敗です。

```tsuzuri
def listing :: Result.Result<[string], Os.Error> -> string
fn listing result =
    match result with
    | Result.Ok names ->
        let newline = "\n"
        String.join (ref newline) (ref names)
    | Result.Error error -> "walk failed: " + Os.message (ref error)

def kind_text :: File.EntryKind -> string
fn kind_text kind =
    match kind with
    | File.Regular -> "file"
    | File.Directory -> "directory"
    | File.Symlink -> "symbolic link"
    | File.Other -> "other"

def meta_text :: Result.Result<File.Metadata, Os.Error> -> string
fn meta_text result =
    match result with
    | Result.Ok meta -> kind_text meta.kind + ", " + to_string meta.size + " bytes"
    | Result.Error error -> Os.message (ref error)

def main :: IO<unit> =
    let! walked = Dir.walk "docs"
    do! IO.write_line (listing walked)
    let! meta = File.metadata "README.md"
    do! IO.write_line (meta_text meta)
```

`docs/a.md` と `docs/sub/b.md` があれば、`a.md`、`sub`、`sub/b.md` の順に表示し、続けて `README.md` の種類と大きさを表示します。

## Path

`Path` は文字列の操作だけをする純粋な関数で、OS に触れません。どの対象でも import なしで動きます。区切りは `/` だけで、引数は `ref string` です。

| API | 結果 | 契約 |
| --- | --- | --- |
| `Path.join left right` | `string` | `/` で連結する。`left` が空なら `right`、`left` が `/` で終わるなら重ねない。`right` が絶対パスでも置き換えず、そのまま続ける |
| `Path.parent path` | `Option<string>` | 最後の名前と末尾の `/` を除く。`/` だけは残す。名前が一つだけの path、`/`、空文字列は `None` |
| `Path.file_name path` | `Option<string>` | 最後の名前。空文字列、`/`、`.`、`..` は `None` |
| `Path.extension path` | `Option<string>` | 最後の名前の、最後の `.` より後。名前が先頭の `.` だけを持つなら `None` |

| 入力 | 結果 |
| --- | --- |
| `Path.join "a" "b"` | `"a/b"` |
| `Path.join "a/" "b"` | `"a/b"` |
| `Path.join "" "b"` | `"b"` |
| `Path.join "a" "/etc"` | `"a//etc"` |
| `Path.parent "a/b/c"`、`"/a"`、`"a/b/"` | `Some "a/b"`、`Some "/"`、`Some "a"` |
| `Path.parent "a"`、`"/"`、`""` | `None` |
| `Path.file_name "a/b.txt"`、`"a/b/"` | `Some "b.txt"`、`Some "b"` |
| `Path.file_name "a/.."`、`"."`、`"/"`、`""` | `None` |
| `Path.extension "a.tar.gz"`、`"a."` | `Some "gz"`、`Some ""` |
| `Path.extension ".bashrc"`、`"a/b.d/c"` | `None` |

```tsuzuri run=reports/2026/summary.tar.gz%0Areports/2026%0Asummary.tar.gz%0Agz
def show :: Option.Option<string> -> string
fn show value =
    match value with
    | Option.Some text -> text
    | Option.None -> "(none)"

let directory = "reports/2026"
let name = "summary.tar.gz"
let path = Path.join (ref directory) (ref name)
let parent = show (Path.parent (ref path))
let file = show (Path.file_name (ref path))
let extension = show (Path.extension (ref path))
let newline = "\n"
let lines = [path, parent, file, extension]
String.join (ref newline) (ref lines)
```

`Path` は `.` や `..`、重複した `/`、シンボリックリンクを解決せず、path が特定のディレクトリの中にあるかも確かめません。サンドボックスではなく、あくまで文字列の整形です。

## Env

| API | 結果 | 契約 |
| --- | --- | --- |
| `Env.args ()` | `IO<Result<[string], Os.Error>>` | プログラム名（argv[0]）を除く引数。引数がない場合と、ホストが渡さない場合（ライブラリ、テストの実行ファイル）は空 |
| `Env.var name` | `IO<Result<Option<string>, Os.Error>>` | 環境変数の値。未設定は `Ok None`、空の値は `Some ""`。名前が空、`=` か NUL を含むと `InvalidInput`、値が UTF-8 でなければ `InvalidEncoding` |
| `Env.current_dir ()` | `IO<Result<string, Os.Error>>` | 作業ディレクトリ。システムが報告する形で返す（macOS の `/tmp` は `/private/tmp`） |

`tsuzuri run` はプログラムへ引数を渡しません。引数が必要なときは `tsuzuri build` で作った実行ファイルを起動します。次の例は引数の数、`HOME`、20 ミリ秒の sleep の実測を表示します。

```tsuzuri
def count_text :: Result.Result<[string], Os.Error> -> string
fn count_text args =
    match args with
    | Result.Ok names -> to_string names.length + " arguments"
    | Result.Error error -> Os.message (ref error)

def var_text :: Result.Result<Option.Option<string>, Os.Error> -> string
fn var_text value =
    match value with
    | Result.Ok (Option.Some text) -> "HOME=" + text
    | Result.Ok Option.None -> "HOME is not set"
    | Result.Error error -> Os.message (ref error)

def elapsed_text :: Result.Result<i64, Os.Error> -> Result.Result<i64, Os.Error> -> string
fn elapsed_text before after =
    match (before, after) with
    | (Result.Ok first, Result.Ok second) -> "slept " + to_string ((second - first) / 1000000) + " ms"
    | _ -> "clock failed"

def main :: IO<unit> =
    let! args = Env.args ()
    do! IO.write_line (count_text args)
    let! home = Env.var "HOME"
    do! IO.write_line (var_text home)
    let! before = Time.monotonic_ns ()
    let! _slept = Time.sleep_ms 20
    let! after = Time.monotonic_ns ()
    do! IO.write_line (elapsed_text before after)
```

```sh
./target/release/tsuzuri build target/env-demo -o target/env-demo-app
target/env-demo-app a b "c d"
```

出力は次のような形です。`HOME` と測定値は環境によって変わります。

```text
3 arguments
HOME=/Users/example
slept 21 ms
```

## Time

| API | 結果 | 契約 |
| --- | --- | --- |
| `Time.monotonic_ns ()` | `IO<Result<i64, Os.Error>>` | 戻らない時計のナノ秒。起点は未規定で、差だけに意味がある |
| `Time.unix_ns ()` | `IO<Result<i64, Os.Error>>` | 1970-01-01 UTC からのナノ秒。壁時計なので、時刻の調整で前後することがある。`i64` に収まらなければ `Other` |
| `Time.sleep_ms milliseconds` | `IO<Result<unit, Os.Error>>` | 少なくとも指定したミリ秒待つ。負は `InvalidInput`、0 は OS を呼ばず `Ok ()`。中断されたら残りの時間から再開する |

経過時間は `monotonic_ns` の差で測ります。`unix_ns` の差は時計の調整に影響されます。IO と同じく Task の中では実行できず、`sleep_ms` は入口のスレッドを止めます。

## Random

| API | 結果 | 契約 |
| --- | --- | --- |
| `Random.bytes count` | `IO<Result<[ubyte], Os.Error>>` | OS の乱数を `count` バイト返す。`count` が負か 2^30 を超えると `InvalidInput` |
| `Random.next_u64 ()` | `IO<Result<i64u, Os.Error>>` | OS の乱数 8 バイトをリトルエンディアンの数にしたもの |
| `Random.pcg seed sequence` | `Random.Pcg` | `i64u` の seed と stream 番号から PCG-XSH-RR 64/32 の生成器を作る（pcg-c-basic の `pcg32_srandom_r`）。純粋 |
| `Random.pcg_next_u32 generator` | `(i32u * Random.Pcg)` | 次の 32 bit と、進んだ生成器（`pcg32_random_r`） |
| `Random.pcg_next_u64 generator` | `(i64u * Random.Pcg)` | 二つの出力から作った 64 bit（先の出力が上位 32 bit）と、進んだ生成器 |

OS の乱数は native では `getentropy`、WASI では `random_get` から取ります。PCG は同じ seed と stream から、どの対象でも import なしで同じ列を返します。出力は pcg-c-basic の公開出力と一致することを検証していて、seed 42、stream 54 の最初の六つは 16 進で `a15c02b7 7b47f409 ba1d3330 83d2f293 bfa4784b cbed606e` です。

```tsuzuri run=2707161783%202068313097%203122475824
def draw :: i64 -> string
fn draw count =
    let mut generator = Random.pcg 42i64u 54i64u
    let mut text = ""
    let mut index = 0
    while index < count do
        match Random.pcg_next_u32 generator with
        | (value, next) ->
            if index > 0 then text = text + " "
            text = text + to_string value
            generator = next
        index = index + 1
    text

draw 3
```

先頭の `2707161783` は `a15c02b7` の十進表記です。生成器は値で、次の出力は戻り値の組の二つ目から受け取ります。PCG は統計的な乱数で、暗号用途には使えません。鍵やトークンには `Random.bytes` を使います。

OS の乱数で seed を作り、以降は PCG で進める形が典型です。同じ seed を記録すれば、同じ列を再現できます。

```tsuzuri
def first_word :: i64u -> string
fn first_word seed =
    match Random.pcg_next_u64 (Random.pcg seed 1i64u) with
    | (word, _) -> to_string word

def seeded :: Result.Result<i64u, Os.Error> -> string
fn seeded result =
    match result with
    | Result.Ok seed -> first_word seed
    | Result.Error error -> Os.message (ref error)

def main :: IO<unit> =
    let! seed = Random.next_u64 ()
    do! IO.write_line (seeded seed)
```

## Process

| API | 結果 | 契約 |
| --- | --- | --- |
| `Process.run program args input` | `IO<Result<Process.Output, Os.Error>>` | `program` を起動して終了まで待つ。シェルは介さない |

- `args` は `program` 自身の名前の後ろに渡す引数で、どの要素も内容にかかわらず一つの引数です。空白、引用符、`;` などは解釈されません。
- `input` のバイト列を標準入力へ書いて閉じます。子が読み切る前に終わっても、呼び出し側は SIGPIPE で落ちません。
- `/` を含まない `program` は PATH から探します。子は環境変数と作業ディレクトリを引き継ぎます。
- 標準出力と標準エラーを集めます。合計が 2^30 バイトを超えると子を終了させ、`Other` を返します。
- 起動できなければ `NotFound`（存在しない）や `PermissionDenied`（実行できない）です。空の `program`、NUL を含む `program` や引数は `InvalidInput`、孤立サロゲートは `InvalidEncoding` です。

`Process.Output` のフィールドは次のとおりです。

| フィールド | 型 | 内容 |
| --- | --- | --- |
| `code` | `i32` | 終了コード。シグナルで終わったときは -1 |
| `signal` | `i32` | 終わらせたシグナルの番号。シグナルでなければ 0 |
| `stdout` | `[ubyte]` | 標準出力のバイト列 |
| `stderr` | `[ubyte]` | 標準エラーのバイト列 |

```tsuzuri
def stdout_text :: ref [ubyte] -> string
fn stdout_text bytes =
    match Os.decode (Array.sub bytes 0 bytes.length) with
    | Result.Ok text -> text
    | Result.Error _ -> "<binary>"

def describe :: Result.Result<Process.Output, Os.Error> -> string
fn describe result =
    match result with
    | Result.Ok output ->
        if output.code == 0i32 then stdout_text (ref output.stdout)
        else "exit code " + to_string output.code + ", signal " + to_string output.signal
    | Result.Error error -> "cannot run: " + Os.message (ref error)

def main :: IO<unit> =
    let! result = Process.run "printf" ["%s", "one two; echo not-run"] []
    do! IO.write_line (describe result)
```

`one two; echo not-run` を一つの引数として表示します。`; echo not-run` は実行されません。

## エラーの対応規則

- 文字の境界は UTF-8 です。path、環境変数の名前、プロセスの引数、書き込むテキストに孤立サロゲートがあると、OS を呼ぶ前に `InvalidEncoding`（code 0）です。NUL を含む path や引数は `InvalidInput`（code 0）です。
- OS から返る名前や値（`Dir.list`、`Env.args`、`Env.var`、`Env.current_dir`、`File.read_text`）が不正な UTF-8 なら、その呼び出し全体が `InvalidEncoding` です。置換文字は使わず、Unicode の正規化もしません。バイト列のまま扱うには `File.read_bytes` を使います。
- native では `errno` を次のように対応させます。`code` には元の `errno` が入ります。

| `Os.ErrorKind` | native の errno と、Tsuzuri 自身が見つける条件 |
| --- | --- |
| `NotFound` | `ENOENT`、`ENOTDIR` |
| `PermissionDenied` | `EACCES`、`EPERM`、`EROFS` |
| `AlreadyExists` | `EEXIST` |
| `InvalidInput` | `EINVAL`、`ENAMETOOLONG`、`EISDIR`。範囲外の引数と NUL（code 0）、閉じたハンドルの使用（`EBADF`） |
| `InvalidEncoding` | `EILSEQ`。孤立サロゲートと不正な UTF-8（code 0） |
| `Interrupted` | `EINTR`。多くの呼び出しは再試行するので通常は返らない |
| `Other` | 上記以外。空でないディレクトリを消すときの `ENOTEMPTY` など |

- EINTR は、open、read、write、sleep、子プロセスの待機などで runtime が再試行します。close は、記述子の状態が不定で再試行すると別のファイルを閉じかねないので、EINTR を閉じ済みとして成功にします。
- WASI でも同じ分類を WASI の errno に適用します（`ENOENT` は 44、`EEXIST` は 20、`ENOTDIR` は 54、`ENOTEMPTY` は 55、`EBADF` は 8）。preopen の外を指す path は `NOTCAPABLE`（76）で `PermissionDenied` です。
- 存在確認の API はありません。`File.metadata` が `NotFound` を返すかを見るか、操作を直接実行します。
- path は正規化しません。`.`、`..`、重複した `/`、シンボリックリンクは OS にそのまま渡します。
- `File.write_bytes` と `File.write_text` は一時ファイルを経由せず、切り詰めてから書きます。途中の失敗で書きかけのファイルが残るので、原子的な置き換えにはなりません。rename に当たる API もありません。

## 終了コード

入口が `IO<i32>` なら、その `i32` の値がプロセスの終了コードになります。OS が見る終了ステータスは下位 8 ビットなので、`256` は 0 です。`IO<unit>` や `IO<i64>` など、ほかの型の入口は値を捨てて 0 で終了します。`Os.exit` のような途中終了の API はありません。

```tsuzuri
def code_of :: Result.Result<string, Os.Error> -> i32
fn code_of result =
    match result with
    | Result.Ok _ -> 0i32
    | Result.Error _ -> 2i32

def main :: IO<i32> =
    let! config = File.read_text "config.txt"
    return code_of config
```

`config.txt` が読めなければ終了コード 2 です。`tsuzuri run` は非 0 の終了コードを `E2005` で報告し、自身の終了ステータスは 1 です。

```text
error[E2005]: program exited with code 2
```

`run --json` では診断の `code` が `E2005` で、`message` は `program exited with code 2` です。`tsuzuri build` で作った実行ファイルは、値をそのまま終了コードにします。以前は `IO<i32>` の値を捨てて 0 で終了していたので、値を返している既存のプログラムは、その値が終了コードに変わります。

## 対象環境

| 対象 | OS API |
| --- | --- |
| macOS と Linux の native | 使えます。`run` と `build` で同じです |
| 既定の wasm32 と wasm64 | 使うビルドは `E2000` です |
| `--wasm-host wasi` の wasm32 | 使えます。WASI preview1 の import へ下げます |
| Windows の native | 使うビルドは `E2002` です。Windows での動作は未検証です |

既定の wasm の成果物には、OS 用のホスト import がありません。OS API に到達するビルドは、`wasm`、`llvm`、`object` のどの出力でも次の `E2000` で止まり、出力ファイルを書きません。

```text
error[E2000]: wasm output cannot use the File, Dir, Env, Time, Random, or Process operating-system APIs because the default wasm target has no host imports; build for the native target, use --wasm-host wasi, or keep to Path and Random.Pcg, which need no host
```

`tsuzuri check` は IR を作らないので、この診断は `build` のときに初めて出ます。`Path`、`Os` の純粋な補助関数、`Random.pcg` は OS に触れないので、既定の wasm でも import なしで動きます。

`--wasm-host wasi` は、標準入出力と OS API を `wasi_snapshot_preview1` の import へ下げ、到達した操作の import だけを出します。次の点が native と異なります。

- `Os.Error` の `code` は WASI の errno です。
- path は、ホストが渡した preopen の名前との最長一致で解決します。どれにも合わない相対 path は最初の preopen からの相対で、preopen の外を指す絶対 path は `PermissionDenied`（code 76）です。
- `Env.current_dir ()` は最初の preopen の名前（例: `/work`）を返します。
- `Process.run` は `Other`（code 52）です。
- 標準入出力も WASI（`fd_read`、`fd_write`）へ下がり、`tsuzuri_io` の import は出ません。
- 組み合わせは wasm32 の `wasm`、`llvm`、`object` 出力だけです。wasm64、`--emit header`、`--wasm-feature threads` との併用は `E2000` です。
- WASI preview2 とコンポーネントモデルは未対応です。

ビルドと実行の手順は [WASM ガイド](../guides/webassembly.md#os-api-と-wasi)を参照してください。同じプログラムの native と WASI の結果は、システムのエラーコードを除いて一致することを `node:wasi` で検証しています。

## 安全上の注意

- path はサンドボックスされません。相対 path は作業ディレクトリが起点で、`..` と絶対パスはそのまま通ります。`Path.join` も外へ出る path を防ぎません。信頼できない名前を使うときは、アプリケーション側で `/` や `..` を検査してください。WASI では、ホストが渡した preopen が境界になります。
- native で作る新しいファイルのモードは 0666 に umask を適用した値です。モードを変える API はありません。
- `File.metadata` で調べてから開くまでの間に、別のプロセスが対象を差し替えられます。検査の結果を安全の根拠にしないでください。
- `Process.run` はシェルを使いません。引数は引用やエスケープなしでそのまま一つの引数になります。ただし、起動したプログラムが `-` で始まる引数をオプションとして解釈するなど、受け取った後の扱いは変わりません。子は環境変数を引き継ぐので、秘密を環境変数に置いている場合は渡ります。`/` を含まないプログラム名は PATH から探すので、確実に特定のプログラムを実行するときは絶対パスを使います。
- `Random.pcg` は暗号用途に使えません。

## 関連項目

- [IO と標準入出力](io.md)
- [WebAssembly と JavaScript ホスト](../guides/webassembly.md#os-api-と-wasi)
- [所有権と借用](../language-reference/ownership.md)
- 標準ライブラリのソースと doc コメント: [Os](../../std/Os.tz)、[File](../../std/File.tz)、[Dir](../../std/Dir.tz)、[Path](../../std/Path.tz)、[Env](../../std/Env.tz)、[Time](../../std/Time.tz)、[Random](../../std/Random.tz)、[Process](../../std/Process.tz)
- 検証用のテスト: [OS API](../../tests/os.mjs)、[WASI の参照ホスト](../../tests/os-wasi-host.mjs)
