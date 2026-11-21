# E20: 利用者の共用体と同じ名前の std の組み込みモジュールの関数を、`std::` を付けて書いても共用体のケースとして探す

| 項目 | 内容 |
| --- | --- |
| ID | E20 |
| 優先度 | P3（利用者が `Channel`・`Mutex`・`Atomic`・`DataChannel` などの組み込みモジュールと同じ名前の共用体を宣言したときだけ。`std::` を付けて std の関数を呼べないので、名前を変えるしかない） |
| 規模 | S〜M（`src/check.rs` の値の名前の解決、回帰テスト、`E1040` の修正案の確かめ（E18 D2・E19）への影響の確認） |
| 依存 | E19（done） |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-22（E19 の PR のレビューで見つかった、E19 より前からある不具合。`main` の `69645da3` と E19 のブランチで再現） |
| 承認 | 不要（`std::` で修飾した名前を std のモジュールに解決する、名前の解決の直し） |
| 改善する劣位 | 不具合: `union DataChannel = Open \| Closed` を宣言したモジュールで `std::DataChannel.bounded 3` と書くと、`E1002: union 'Main.DataChannel' has no case 'bounded'` になる |
| 手本にする既存実装 | std の `.tz` の関数（`union Path` の隣の `std::Path.join` は std の関数に解決する）と、組み込みの `Routine`（`union Routine` の隣の `std::Routine.scope` も解決する） |
| 主な影響ファイル | `src/check.rs`, `tests/removed_features.rs`（または名前の解決のテスト） |

## 観測

各プログラムは `Main.tz` だけ（マニフェストなし、edition 2027）。

| 共用体 | 書いた呼び出し | 結果 |
| --- | --- | --- |
| `union DataChannel = Open \| Closed` | `std::DataChannel.bounded 3`（型注釈 `std::DataChannel.Sender<i64> * …`） | `E1002: union 'Main.DataChannel' has no case 'bounded'` |
| 同上 | `std::DataChannel.clone_sender sender` | `E1002: … has no case 'clone_sender'` |
| `union Channel = Open \| Closed` | `std::Channel.make 1` | `E1002: union 'Main.Channel' has no case 'make'` |
| `union Mutex = Locked \| Free` | `std::Mutex.create 1` | `E1002: union 'Main.Mutex' has no case 'create'`（起票時の表は `new` と書いたが、`Mutex.new` という関数はない。共用体のない `std::Mutex.new` は `module 'std::Mutex' has no function or union case 'new'`） |
| `union Atomic = A \| B` | `std::Atomic.create 1` | `E1002: union 'Main.Atomic' has no case 'create'`（同上、`new` はない） |
| `union Path = Here \| There` | `std::Path.join "a" "b"` | 通る（std の `.tz` の関数） |
| `union Routine = Idle \| Busy` | `std::Routine.scope (ref base) 4 (…)` | 通る |

型の位置の `std::DataChannel.Sender<i64>` は std の型に解決する（上の表の 1 行目の型注釈はエラーにならない）。裸の `DataChannel.bounded` が利用者の共用体のケースとして探されるのは「プログラムの定義が勝つ」規則どおりでよいが、`std::` を付けた名前は std の組み込みに解決するべき。

## E18・E19 との関係

E18 の D2 と E19 は、`E1040` の確かな修正案の置き換え（`DataChannel.bounded`・`std::DataChannel.recv` など）を、`Names::names_std_module`（モジュールの解決）とローカルの束縛で確かめる。利用者の共用体 `DataChannel` は、モジュールでもローカルの束縛でもないので確かめを通り、修正案は裸の `DataChannel.bounded` になるが、直したプログラムは上の `E1002` になる（`tsuzuri fix` は、修正案が `E1002` を増やすので `E2008` で止まり、どのファイルも書かない。意味を変えて書くことはない）。このチケットで `std::` の修飾を直したら、`names_std_module` か `removed_written`・`data_channel_spellings` が、同じ名前の共用体（と型）も「裸の名前を取るもの」として扱い、`std::` を付けるようにする。

## 進め方

1. 値の位置の `Module.member` の解決（`value_expression` の `ExprKind::Field`、`qualified_builtin`、共用体のケースの探索の順序）で、`std::` で修飾した名前は利用者の共用体のケースを探さず、std のモジュール・組み込みに解決する。`std::Path.join`・`std::Routine.scope` が通る経路と比べて、組み込みのモジュール（`Channel`・`DataChannel`・`Mutex`・`Atomic` など）だけが違う理由を確かめる。
2. 裸の `M.member`（`std::` なし）は、今までどおり利用者の共用体のケースが先。
3. E18 D2・E19 の確かめに、同じ名前の利用者の共用体（値の位置で裸の名前を取るもの）を足し、そのときは `std::` を付ける。
4. 回帰テスト: 上の表のそれぞれ、`E1040` の修正案（`Channel.bounded`・`Channel.recv` の書き換え）が同じ名前の共用体の隣で `std::` を付け、直したプログラムが通ること。

## 受け入れ条件

- [ ] 上の表の `std::` を付けた呼び出しが、std の関数に解決して通る。
- [ ] 裸の `M.member` の解決（利用者の共用体のケースが先）は変わらない。
- [ ] 同じ名前の共用体の隣の `E1040` の確かな修正案が、直したプログラムが通る書き方になる。

## 実装と検証（2026-11-22）

`main` `fe5c68bf` で着手した。

- 原因: `case_reference` は、`Module.member` が見える関数（std の `.tz` の関数を含む）を名指すときは共用体のケースを探さないが、組み込み関数（`DataChannel.bounded`・`Channel.make`・`Mutex.create`・`Atomic.create`）は関数の表にないので `Names::case_path` に進む。`case_path` の最後の分岐は、モジュール（std の `DataChannel` など）にその名前のケースがないとき、参照元の同名の共用体のケースへ戻っていた。書いた名前が `std::DataChannel` でも戻っていたのが不具合。
- 直し（`src/check.rs`）: その分岐を、書いた名前がモジュールの名前そのもの（裸）のときだけにした。`std::M.member` は `None` を返し、呼び出し側が std の組み込み関数を見つける。裸の `M.member` は今までどおり参照元の共用体のケースが先（「プログラムの定義が勝つ」）。参照元の共用体に取られて std の組み込み関数に届かない裸の名前は、`E1002` の文に `write 'std::Mutex.create' for the standard library's function` を足す（std のケースのときの `for the standard library's case` と同じ形）。
- E18 D2・E19: 値の位置の裸の置き換えの確かめを `Checker::takes_bare_module`（同じ名前のローカルの束縛か、参照元の同名の共用体）にまとめた。自前の `union DataChannel` があるモジュールの `Channel.bounded`・旧い `Channel.recv` の修正案は `std::DataChannel.bounded`・`std::DataChannel.recv` になり、直したプログラムが通る（以前は裸の名前で、`tsuzuri fix` は `E2008` で止まった）。
- テスト: `tests/modules.rs`（上の表の `std::` の呼び出しがすべて通ること、`std::Path.join` も、裸の `Mutex.create` の `E1002` と案内）、`tests/removed_features.rs` の `a_union_named_like_the_replacement_module_makes_the_fix_say_std`。
- 文書: `union.md`、`compiler/diagnostics.md`（`E1040`）、`docs/language.md`、`docs/architecture.md`。
