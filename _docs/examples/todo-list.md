# 状態を更新する TODO リスト

[実装例の一覧](README.md) · [ドキュメントのトップ](../README.md)

項目の追加・完了・削除を処理し、最後に一覧を表示するアプリケーションです。データと操作を一つのモジュール、入力するコマンド列を Main に分けます。

## 完成時の動作

3 件を追加し、1 件目を完了、2 件目を削除します。存在しない ID の完了要求はエラーになりますが、現在の一覧は維持します。

```text
Error: Item not found.
[x] 1: Write docs
[ ] 3: Review code
```

この例はコマンド列を順に適用するインメモリーのコンソールアプリです。対話入力、保存、日時、ユーザー管理は含みません。同じ状態更新関数をホストの入力処理に接続できますが、Board をそのまま C / WASM ABI に公開することはできません。

## ファイル構成

```text
target/todo-list/
    Tasks.tz
    Main.tz
```

Tasks は通常のモジュール名です。予約された組み込み名前空間 Task とは別で、このアプリは並列タスクを使いません。

## データと更新処理

次が `Tasks.tz` の全内容です。apply は Board を消費しますが、成功・失敗のどちらでも更新後の Board と操作結果の組を返します。

```tsuzuri project=todo file=Tasks.tz
record Item { title: string, completed: bool }
record Board { next_id: i64, items: Map<i64, Item> }
union Command = AddItem of string | CompleteItem of i64 | RemoveItem of i64

def empty :: Board
fn empty = Board { next_id: 1, items: Map.empty() }

def apply :: Board -> ref Command -> Board * Result<unit, string>
fn apply board command =
    match command with
    | AddItem title ->
        let cleaned = String.trim (ref title)
        if cleaned.length == 0 then (board, Result.Error "Title must not be empty.")
        elif board.next_id == 9223372036854775807 then (board, Result.Error "No IDs remaining.")
        else
            let identifier = board.next_id
            let items = Map.insert board.items identifier (Item { title: cleaned, completed: false })
            (Board { next_id: identifier + 1, items: items }, Result.Ok ())
    | CompleteItem identifier ->
        if !(Map.contains_key (ref board.items) identifier) then (board, Result.Error "Item not found.")
        else
            let current = Map.at (ref board.items) identifier
            let replacement = Item { title: clone_string (ref current.title), completed: true }
            let next_id = board.next_id
            let items = Map.insert board.items identifier replacement
            (Board { next_id: next_id, items: items }, Result.Ok ())
    | RemoveItem identifier ->
        if !(Map.contains_key (ref board.items) identifier) then (board, Result.Error "Item not found.")
        else
            let next_id = board.next_id
            let items = Map.remove board.items identifier
            (Board { next_id: next_id, items: items }, Result.Ok ())

def render :: ref Board -> string
fn render board =
    if Map.is_empty (ref board.items) then "(empty)"
    else Map.fold (ref board.items) "" (fx report identifier item ->
        let separator = if report.length == 0 then "" else "\n"
        let marker = if item.completed then "[x] " else "[ ] "
        report + separator + marker + to_string (deref identifier) + ": " + clone_string (ref item.title))

test "returns the unchanged board for a blank title" =
    let command = AddItem " \t "
    match apply (empty()) (ref command) with
    | (board, result) ->
        assert (Result.is_error ref result)
        assert (board.next_id == 1 && Map.is_empty (ref board.items))

test "preserves existing items after an unknown ID" =
    let add = AddItem "Keep this"
    let missing = CompleteItem 99
    match apply (empty()) (ref add) with
    | (board, _) ->
        match apply board (ref missing) with
        | (unchanged, result) ->
            assert (Result.is_error ref result)
            assert (unchanged.next_id == 2)
            assert (render (ref unchanged) == "[ ] 1: Keep this")

test "completing an item twice is valid" =
    let add = AddItem "Review"
    let complete = CompleteItem 1
    match apply (empty()) (ref add) with
    | (board, _) ->
        match apply board (ref complete) with
        | (completed, _) ->
            match apply completed (ref complete) with
            | (unchanged, result) ->
                assert (Result.is_ok ref result)
                assert (render (ref unchanged) == "[x] 1: Review")

test "rejects removal from an empty board" =
    let command = RemoveItem 1
    match apply (empty()) (ref command) with
    | (board, result) ->
        assert (Result.is_error ref result)
        assert (render (ref board) == "(empty)")
```

Board が成功値の内側にしかない `Result<Board, string>` だと、Error のとき元の Board を返せません。ここでは失敗後にもアプリを継続したいので、状態と成否を別々に返します。

## コマンドを実行する

次が同じディレクトリの `Main.tz` です。

```tsuzuri project=todo file=Main.tz run=Error%3A%20Item%20not%20found.%0A%5Bx%5D%201%3A%20Write%20docs%0A%5B%20%5D%203%3A%20Review%20code
def main :: string
fn main =
    let commands = [
        Tasks.AddItem "Write docs",
        Tasks.AddItem "Ship release",
        Tasks.AddItem "Review code",
        Tasks.CompleteItem 1,
        Tasks.RemoveItem 2,
        Tasks.CompleteItem 99
    ]
    let mut board = Tasks.empty()
    let mut errors = ""
    for command in commands do
        match Tasks.apply board (ref command) with
        | (updated, result) ->
            board = updated
            match result with
            | Result.Ok _ -> ()
            | Result.Error message -> errors = errors + "Error: " + message + "\n"
    errors + Tasks.render (ref board)
```

commands の要素は string を含む非 Copy 値です。通常の for は要素を借用して読むので、apply も command を共有参照で受け取ります。配列から所有値を取り出そうとはしません。

## 実行する

リポジトリルートから、二つのソースを含むディレクトリを指定します。

```sh
./target/release/tsuzuri check target/todo-list
./target/release/tsuzuri run target/todo-list
./target/release/tsuzuri test target/todo-list
```

Main からは Tasks.empty / apply / render と修飾します。別モジュールの関数を使うために export は必要ありません。

## 状態と所有権の流れ

Board は非 Copy の Map を所有します。各操作へ move し、戻ってきた Board を次の反復の前に再代入します。失敗した分岐も元の Board を返すので、エラーの後に一覧を表示できます。

CompleteItem は現在の Item を借用し、タイトルを複製した置換値を作ります。その借用が終わってから Map を消費して更新します。Map.at の参照を保持したまま Map.insert しないことが重要です。

RemoveItem は Map の所有更新で、削除する項目のタイトルも解放されます。削除した ID は再利用せず、新規項目の番号だけを増やします。最大 ID に達した場合は折り返さず、新規追加を拒否します。

## ルールと制限

空タイトルは ASCII whitespace を trim してから判定します。Unicode の空白をすべて取り除く API ではありません。存在しない ID の完了・削除は失敗、すでに完了した項目の再完了は成功です。

Map は ID 順に列挙し、検索は O(log n)、挿入・削除は O(n) です。ここでは小さい一覧を想定しています。公開の Board レコードをホストの入力から無検証で構築する設計にはせず、運用では状態の生成経路を管理してください。

## 発展させるとき

期限や優先度は Item のフィールド、操作は Command の case として追加できます。コマンドを増やしたとき、match の網羅性検査が処理の追加漏れを検出します。

ファイル保存や画面からの操作はホストへ置きます。永続化する場合は、ID の採番と保存失敗時の状態の扱いも別途決めます。

## 関連項目

- [union とパターンマッチ](../language-reference/unions.md)
- [所有権と部分 move](../language-reference/ownership.md)
- [Map API](../library-reference/map-set.md)
- [モジュールと可視性](../language-reference/modules-and-packages.md)
