# タスク、並列実行、失敗の伝播

[ドキュメントのトップ](../README.md)

`Task<T>` は、所有値を捕捉した cold な一回実行の計算です。作成時には開始せず、スレッドのハンドルや JavaScript の Promise でもありません。F# の通常の task と開始時点が異なります。

## 作成と実行

```tsuzuri run=42
def next :: i64 -> Task<i64> = \value -> task { return value + 1 }

let work = task {
    let! first = next 19
    let! second = next 21
    do! task { assert (first == 20); }
    return first + second
}
Task.run work
```

`Task.run` はタスクを消費し、完了まで同期的に実行します。`let!` は別のタスクを一回実行して値を取り出します。let! を並べた場合は順次実行です。

`return!` は最後に別のタスクを実行してその結果を返し、`do!` は Task の unit 結果を受け取ります。return は末尾に置きます。task では最後の通常式を結果にすることもでき、空本体は `Task<unit>` です。

## 並列タスク

```tsuzuri run=14
let jobs = new [Task<i64>](4, index -> task { return index * index })
let results = Task.run (Task.parallel jobs)
Array.sum ref results
```

`Task.parallel :: [Task<T>] -> Task<[T]>` は配列を消費して新しい遅延タスクを返します。本体の実行・完了順序は未規定ですが、結果配列は入力順です。空入力なら空配列を返します。

タスク配列の構築や捕捉の評価順は通常どおりです。結果型の異なる仕事は一つの配列へ混在させられません。ユーザー向けの detach、スレッド ID、共有状態は提供しません。

## Result を返す並列計算

```tsuzuri run=42
def job :: i64 -> Task<Result<i64, string>> = \value -> task {
    if value == 2 { return Result.Error "stopped" }
    else { return Result.Ok (value * 2) }
}

let jobs = new [Task<Result<i64, string>>](4, index -> job index)
let result = Task.run (Task.parallel_results jobs)
match result with
| Result.Ok values -> Array.sum ref values
| Result.Error error -> if error == "stopped" then 42 else 0
```

`Task.parallel_results` の型は `[Task<Result<T, E>>] -> Task<Result<[T], E>>` です。全件 Ok なら入力順の配列を返し、空入力は `Ok []` です。

Error を検出したら、その index 以降の未配布タスクを開始せず、すでに開始したタスクは完了まで待ちます。返却する Error は完了順ではなく、入力 index が最小のものです。未開始タスクの捕捉値、未採用の結果、一時バッファは解放します。

これは開始済みタスクの強制停止ではありません。外部キャンセルトークン、unwind、トラップからの回復はなく、通常の Task.parallel は引き続き全件を実行します。

## 所有権と寿命

Task は常に非 Copy です。一つのタスクを二度実行できず、タスク配列の要素を添字で取り出して複製することもできません。繰り返し実行したい処理は毎回新しい Task を返す関数にします。

未実行の Task を捨てると、本体は実行せず捕捉値だけを解放します。一回実行の Task は別の Task を捕捉できますが、通常の再利用可能なクロージャーは Task を捕捉できません。

捕捉値と結果は借用を保持できません。配列、レコード、関数環境の中に隠れた参照も対象です。所有値を捕捉した後に、タスク内部で一時的に借用することはできます。関数値を捕捉するときは、その環境が借用を保持しないと証明できる必要があります。

## native の実行方式

POSIX では pthread、Windows の実装では Win32 の常駐ワーカープールを使います。複数の仕事が必要になるまでプールを起動せず、以後は再利用します。0 件・1 件では追加 worker を起動しません。

追加 worker はランタイム全体で最大 `min(オンライン CPU 数, 32) - 1` です。呼び出し元も処理を進め、入れ子では自分のグループを優先して進めます。CPU 数を取得できなければ追加 worker なしで実行します。

戻る時点ではグループの callback と結果公開が完了しています。常駐 OS スレッド自体は再利用のため待機し、呼び出しごとに破棄するわけではありません。短い仕事では確保・コピー・同期の費用が上回る可能性があります。

## WASM と失敗

WASM は既定で import 不要の逐次経路です。明示的な threads feature と適切な Worker ホストを使う場合に限り並列実行します。結果順序、所有権、Result のエラー選択の契約は同じです。

Task.run はいずれのターゲットでも同期呼び出しです。UI スレッドを非ブロッキングにしたり、非同期 I/O のイベントループへ自動接続したりしません。

メモリ不足、トラップ、スレッド基盤の失敗は Error に変換しません。失敗時の巻き戻し、解放、兄弟の停止は保証されず、非停止タスクがあれば完了待ちも終わりません。

## 関連項目

- [通常のコンピュテーション式](computation-expressions.md)
- [所有権と Capture](ownership.md)
- [正式な実行バックエンドの契約](../../docs/language.md#実行バックエンドと失敗)
