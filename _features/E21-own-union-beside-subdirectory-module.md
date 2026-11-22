# E21: 自分の共用体と同じ名前のモジュールがサブディレクトリにあると、無修飾の `Union.Case` が自分の共用体のケースに戻らない

| 項目 | 内容 |
| --- | --- |
| ID | E21 |
| 優先度 | P3（自分のモジュールに宣言した共用体と同じ名前のモジュールが、`using` などで見えるサブディレクトリにあるときだけ。同じ名前のモジュールがルートにあれば通るので、置き場所で受け付けが変わる） |
| 規模 | S（`src/check.rs` の `Names::case_path` の `Module.Union.Case` の分岐、回帰テスト） |
| 依存 | E20（done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（E20 の PR のレビューで見つかった、E20 より前からある振る舞い。`main` の `fe5c68bf` と E20 のブランチの両方で再現） |
| 承認 | 不要（モジュールの置き場所で名前の解決が変わらないようにする直し） |
| 改善する劣位 | 不具合: 自分の `union Tools` のケース `Tools.Hammer` が、同じ名前のモジュール `Tools` がルートにあれば自分の共用体のケースになり、サブディレクトリ（`Lib/Tools.tz`、`using Lib`）にあると `E1002: module 'Tools' has no function or union case 'Hammer'` になる |
| 手本にする既存実装 | `Names::case_path` の最後の分岐（ルートのモジュールにケースがないとき、無修飾なら自分の同名の共用体へ戻る。E20 で無修飾だけにした） |
| 主な影響ファイル | `src/check.rs`, `tests/modules.rs` |

## 観測

`Tsuzuri.toml` の `namespace = "App"` のプロジェクトで、`Lib/Tools.tz`（`namespace Lib`、関数 `helper` だけ）と、次の `Main.tz`:

```text
namespace App
using Lib

union Tools = Hammer | Saw

def main :: unit -> i32 = \() ->
    let a = Tools.Hammer
    let x = match a with
        | Tools.Hammer -> 1
        | Tools.Saw -> 2
    do! IO.writeln $"{x + Tools.helper ()}"
    0
```

`tsuzuri check` は `7:19: error[E1002]: module 'Tools' has no function or union case 'Hammer'` を出す。`Tools.tz` をルートに置いて `using Lib` を消すと通り、`2` を出す。

原因: ルートのモジュールのキー（`Tools`）には `.` がないので `case_path` の最後の分岐が自分の共用体へ戻るが、サブディレクトリのモジュールのキー（`Lib.Tools`）は `Module.Union.Case` の分岐に入り、`union_case(requester, "Lib", "Tools", …)` が `None` を返して終わる。

## 進め方

1. `case_path` の `Module.Union.Case` の分岐で、`union_case` が `None` を返し、書いた名前が無修飾（`.` が 1 つで `:` がない）で、書いた頭がモジュールのキーの最後の部分と同じで、自分のモジュールに同名の共用体があるときは、最後の分岐と同じく自分の共用体のケースへ戻る。
2. 回帰テスト: 観測のプログラムが通り、ルートに置いたときと同じ結果になること。`Lib::Tools.Hammer` のように名前空間を付けた書き方は戻らないこと（E20 の規則）。

## 受け入れ条件

- [ ] 観測のプログラムが通り、`tsuzuri run` が `2` を出す。
- [ ] ルートに置いたときの振る舞いと、名前空間を付けた書き方の振る舞い（E20）が変わらない。
