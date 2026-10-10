# F10: 並行処理プリミティブ（Atomic・Mutex・Channel）

| 項目 | 内容 |
| --- | --- |
| ID | F10 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | F01, B07, (A13), (C10) |
| 後続 | なし（C10 の循環の注意書きだけが F10 の `Mutex` に依存する） |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | `96d7cbf`（main）。ブランチ `impl/f10-concurrency`、2026-10-10 |
| 承認 | 全フェーズと必要な判断への包括承認（D1、D10 を含む）。GUIDE の [D-44](../GUIDE.md#d-44-第2期の-4-チケットe09f09f10c11の確定) に記録した |
| 利用者向け仕様 | [Atomic](../../_tsuzuri/language-reference/built-in-types-and-modules/atomic.md)、[Mutex](../../_tsuzuri/language-reference/built-in-types-and-modules/mutex.md)、[Channel](../../_tsuzuri/language-reference/built-in-types-and-modules/channel.md)、[Task 式](../../_tsuzuri/language-reference/async-tasks-and-lazy/task.md#共有状態と-taskscope)、[言語仕様](../../docs/language.md#atomic--mutex) |

## 目的と実装範囲

構造化された fork/join（`Task.parallel`、`Parallel`）では表せない、進捗カウンター・共有キャッシュ・生産者と消費者のパイプラインを、データ競合なしに書けるようにする。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | `Atomic<'a>`、`Mutex<'a>`（`Mutex.with_lock`）、builtin `Task.scope`、組み込みクラス `Sync`・`AtomicValue`、`Arc<T>` の `Send` の規則（`T` が `Send` かつ `Sync`）、native と既定の wasm32 のランタイム、`--trap-mode return` の境界でのロック解放 |
| Phase 2 | `Channel`（容量付きの MPMC、drop で閉じる両端）、待ちの規則（手伝う、眠る、全員が待てばデッドロックのトラップ）、`--wasm-feature threads` の `Mutex` と `Channel`（Worker ごとのインスタンスのグローバルで、TLS の初期化は要らない） |

## 確定した API と意味

```text
Atomic.create / load / store / swap / compare_exchange / fetch_add|sub|and|or|xor / into_inner   (AtomicValue: i8..i64, i8u..i64u, bool)
Mutex.create :: Send<'a> => 'a -> Mutex<'a>
Mutex.with_lock :: Send<'b> => ref Mutex<'a> -> (ref mut 'a -> 'b) -> 'b
Mutex.into_inner :: Mutex<'a> -> 'a
Task.scope :: (Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]
Channel.bounded :: Send<'a> => i64 -> (Channel.Sender<'a> * Channel.Receiver<'a>)
Channel.send :: ref Channel.Sender<'a> -> 'a -> Result<unit, 'a>
Channel.recv :: ref Channel.Receiver<'a> -> Maybe<'a>
Channel.clone_sender :: ref Channel.Sender<'a> -> Channel.Sender<'a>
```

- 順序と表現: `Atomic` の操作は `seq_cst` の `atomicrmw`・`cmpxchg`・`load atomic`・`store atomic` 1 命令（AArch64 の `ldaddal`、x86-64 の `lock` 付き命令、wasm の `i64.atomic.rmw.add`）。セルは配列のバッファで、record をムーブしても動かない。`Mutex` のセルは `(i32u * 'a)` の 1 要素の配列で、先頭の語がロック（0、1、2）。
- `Sync` と `Arc`: `Sync` は `Atomic`・`Mutex<T: Send>`・`Channel` の両端・整数・文字列・配列・レコード・関数値・`Sync` な値の `Arc`。`Rc`、extern ハンドル、`Task`、排他参照、Copy でない `dyn`、`Owned.Function`、`Seq`、`Async`、GPU のハンドルは `Sync` でない。`Arc<T>` は `T` が `Send` かつ `Sync` のとき `Send` で、`Arc<Atomic<T>>`・`Arc<Mutex<T>>`・`Arc<Channel.Receiver<T>>` が、タスクの間で状態を共有する標準の形になった。
- `Mutex.with_lock` の中は、入れ子の `with_lock`、並列の開始、`Channel` の操作がトラップ（デッドロックのかわり）。結果は `Send` で、ロック中の値への借用を持てない（`E1013`）。ポイズンはない。
- `Channel`: 容量は 1 以上（以下はトラップ）。`send` は満杯なら待ち、受け手がいなければ要素を `Error` で返す。`recv` は空なら待ち、すべての `Sender` が drop され空なら `None`。最後の端が drop されるとき、残った要素を 1 回ずつ drop してブロックを解放する。
- 待ちの規則: 待つスレッドは、空いているワーカー（仕事を取っていないワーカー）がいなければ、最も古いグループの未着手の仕事を自分のスタックの上で走らせ（深さ 16 まで）、なければ眠る。グループを始めたスレッドは、公開する lock の中で最初の子どもを取り、結合を待つあいだは自分のグループの子どもだけを走らせる（native と `--wasm-feature threads` で同じ）。全スレッドが待ち、動かせる仕事もなければ、待ちを全員起こして `Tsuzuri runtime: deadlock: every task is waiting on a channel` を出し、assert のトラップにする。既定の wasm32 と CPU が 1 つの native は、子どもを `index` の順に 1 スレッドで走らせ、満たされない待ちはその場でトラップする。完了するかどうかはスレッド数に左右される（D10）。
- opt-in: 予約モジュール `Atomic`・`Mutex`・`Channel`（`RESERVED_MODULES` は 49 個）は、ソースに名前が現れたプログラムだけに読み込まれる。`Sync`・`AtomicValue` の組み込みクラスは `BUILTIN_CLASSES`（33 個）。使わないプログラムの IR はバイト単位で変わらない。
- 診断: `E1022`（両端の構築・フィールド参照・内部の close 関数）、`E1016`（利用者の std 型への `Drop`・`Sync` などへの instance）、`E1005`（両端と `Atomic`・`Mutex` の捕捉）、`E1013`（`Sync` でない共有、`Send` でない要素、借用を持つ結果や一時値）、`E2000`（`--freestanding` の `Mutex`・`Channel`）、`E1008`・`E1026`（export・extern・`const` の境界）。

## 決定事項の結果

| 決定 | 結果 |
| --- | --- |
| D1 名前 | 予定どおり。ただし `Mutex.with` は `with` が予約語でドットの後ろに書けないので **`Mutex.with_lock`**（`Bench.with_input` の先例）。構築は `create`（`new` は予約語）。 |
| D2 `Task.scope` | 予定どおり（完全適用だけの同期 builtin、1 子ども 1 要素のグループ）。一時値の借用（`Task.scope (ref 5) ...`）も使える。 |
| D3 表現 | 予定どおり（`Type` の variant を増やさない不透明な std record）。`Channel` の両端は `{ block: i64 }`。 |
| D4 複製と捕捉 | 予定どおり。`Atomic`・`Mutex` と、`Drop` を持つ両端は `Copy` でも `Capture` でもない。 |
| D5 順序 | 予定どおり `seq_cst`。 |
| D6 `Sync` | 変更あり: `Function` は型の上で `Sync`（C10 の `Arc<i64 -> i64>` を保つため）で、環境の安全は `Task.scope` と `Parallel.map_ref` の所有権の証明（`E1013`、`proven owned environments`）が担う。`Arc<T>` の `Send` の規則は C10 の「`T` が `Send`」から「`T` が `Send` かつ `Sync`」へ。`sync_here(Variable)` は偽で、制約は具体化のときに検査する。 |
| D7 `Mutex` と trap | 変更あり: `--trap-mode return`（E14）で、トラップ後に鍵が残り `tz_mutex_held` が古くなるのは許せないので、`task.c` と `trap.c` が弱い `tsuzuri_sync_hooks.abandon` を共有し、境界がトラップしたスレッドのロックを解放して状態を消す。ポイズンは持たない。 |
| D8 threads の `Mutex` | 変更あり: E2000 では出さず、実装した。TLS（`__wasm_init_tls`）の初期化ではなく、各 Worker が自分のインスタンスを持つことを使い、スレッドごとの状態を `address_space(1)` の WebAssembly のグローバルにした。ホストのグルーは変わらない。 |
| D9 既定の WASM | 予定どおり。`Channel` の操作はモジュールの IR で、満たされない待ちはトラップ。 |
| D10 `Channel` と待ち | 予定どおり。手伝いは「空きワーカーがいないとき」に限った（生産者が消費者を自分の上で走らせて詰まる逆転を避ける）。判定は、空きワーカーがいなければ未着手の仕事があっても出す（深さの上限で手伝えない待ちが残って止まるのを避ける）。 |
| D11 依存 B07 | Phase 2 は B07 の drop の機構を使う。std の型に `Drop` の instance は書けない（E1016）ので、std のモジュールが自分で宣言した型にだけ std が書けるようにした（`polymorph::validate_drop_instance`）。 |

その他の変更: `send` は `unit` ではなく `Result<unit, 'a>`（受け手が消えたとき要素を失わない）。`Channel` の操作は、待つかどうかにかかわらず `Mutex.with_lock` の中でトラップする（`tsuzuri_mutex_wait_ok`）。起こす側が、起こすスレッドの「走っている」数え上げを先に戻す（native。起こされる側が戻すと、その間に別のスレッドが全員待ちと誤判定する）。

## 実装の要点

- 型と登録: `Builtin::Atomic*`・`Mutex*`・`TaskScope`・`Channel*`（`sync_scheme`）、`Type::can_sync`・`can_send`・`can_capture`・`is_shared_cell`、`stdlib.rs`（`SOURCES`・`RESERVED_MODULES`・`OPT_IN`・`opaque_record`）、`polymorph.rs`（`E1022` の内部関数、std の `Drop`）。
- 生成: `src/llvm_sync.rs`（`Atomic`・`Mutex`・`Channel` の lowering。`Channel` のブロックは先頭 80 バイトとリング）、`src/llvm_parallel.rs`（`Task.scope`）、`src/llvm.rs`（ヘッダーの組み立て）。
- ランタイム: `src/runtime/task.c`（ロック、`busy`・`idle`・`waiters` の数え上げ、判定、手伝い）、`task-wasm-threads.c`（`epoch` で眠る。進展ごとのリセットの数え上げ）、`channel-wasm.ll`・`sync-wasm.ll`（既定の wasm32）、`task-windows.h`（`pthread_cond_init`・`signal` の shim）、`trap.c`（境界のフック）。
- 所有権: `src/ownership.rs` が、`Task.scope` と `Parallel.*` の共有引数の環境、`Mutex.with_lock` の結果が借用を持たないことを証明する。

## 検証で見つかった欠陥と修正

独立したレビューが、Phase 1 の次の 3 つを見つけ、それぞれ別のコミットで直した（修正前に失敗するテストつき）。

1. `Mutex.with_lock` の結果が、ロック中の値への借用を捕捉した関数値を返せた（解放後の読み出しとデータ競合）。結果の型が借用を持ちうるとき、`callback` の結果が借用を持たないと証明できなければ `E1013`。部分適用と関数値化も拒否する。
2. `Task.scope (ref (\x -> ... Rc.share r ...))` のように、借用を持つ一時値を共有すると、名前を付けたときだけ `E1013` だった。一時値が借用を 1 つでも持てば拒否する（`Parallel.map_ref` などの入力も同じ）。
3. 再帰した型の中の `Arc<Mutex<..>>` を持つ値を、クロージャが捕捉できなかった。`Arc` の中は `Sync` の判定に任せ、型が自分で持つセルだけを拒否する。

Phase 2 のレビューは、さらに次の欠陥を見つけ、それぞれ別のコミットで直した（修正前に失敗するテストつき）。

1. `Channel.send` が、借用を持つ関数値を受け取れた。`Sender<T>` は整数 1 つの record で、`carries_loans` に要素の型が見えず、`can_send(Function)` は真だったので、`producer` が戻ったあとの `data` を、受け手が読めた（解放後の読み出し）。`Channel.send` の直接呼びで、要素の loan が 1 つでもあれば `E1013`（引数の借用も、`ref` した局所値の借用も）。部分適用・関数値化・`|>` は、要素の型が借用を持ちうるとき `E1013`。`recv` が返す要素は、入れるときに検査済みなので loan を持たない（取り出した関数を別のチャンネルへ送れる）。
2. `Send<'a>` が型変数に対して真で、ジェネリック関数の中で満たされたと見なされ、使う型で再検査されなかった（`Channel.bounded`・`Mutex.create`・`Parallel.init` を包む関数、利用者が書いた `Send<'a>`）。`Rc` を運ぶチャンネルが作れ、非 atomic の計数が競合した。`can_send` は型変数と推論変数に偽を返し、制約が関数に残って、使う型ごとに検査される（`Sync` と同じ）。
3. `--wasm-feature threads` で、3 段のパイプラインが最少の 3 スレッドで、CPU が混んでいるときに、誤ってデッドロックのトラップになった。手伝いは「空きのスレッドがいないとき」だけだが、グループの終わりを待つスレッドを空きに数えておらず、`submit` が lock を放してから最初のアイテムを取るまでの間に、ワーカーがアイテムをすべて取ると、待ち始めたワーカーが最後の段を自分の下の段の上に積んで詰まった。結合を待つスレッドを `idle` に数えて直したが、この数え方は、次の回のレビューで別の誤判定を許すと分かり（下の 2.）、最初の子どもを公開と同じ lock の中で取る方式に置き換えた。
4. `--wasm-feature threads` のプールが、`Channel` を使わないプログラムでも遅くなった（`Task.scope` で 1,000,000 個の子ども、7 ワーカーの中央値が、Phase 1 の 360 ms から 2,216 ms）。アイテムが終わるたびに、`progress()` が lock の中で `epoch` を進めて全員を起こし、起こされたスレッドが同じ lock を奪い合い、眠る道は lock を 2 回取っていた。眠っているスレッドが待つのは、新しい仕事、グループの最後の完了、チャンネルの変化、競合したロックの解放だけなので、グループの途中のアイテムの完了は誰も起こさない。通知は lock を放してから送り、眠るスレッドは登録した `epoch` が動くまで眠り続ける（遅れた通知で二重に数えない）。アイドルのワーカーと結合を待つスレッドは、アイテムがないと判定した同じ lock の中で登録する。7 ワーカーで 233 ms、3 ワーカーで 199 ms（Phase 1 は 360 ms と 271 ms）。`tests/wasm_threads.mjs` は、5,000 個の子どもの scope で `epoch` が動く回数（2 回。以前は 5,001 回）を調べる。

その修正のレビューが、さらに次の欠陥を見つけ、それぞれ別のコミットで直した（修正前に失敗するテストつき）。

1. `can_send` と `sync_here` が、型変数に型引数を適用した型（高カインドの `'f<i64>`）と型構築子の部分適用を、既定の腕で「ただのデータ」として真と答えていた。`Send<'f<i64>>` が関数を書いた場所で満たされたと見なされ、`'f` に `Holder` が決まる使用箇所で検査されなかった（`Rc` を運ぶチャンネル、`Parallel.init`、`Mutex.create`、`Task.scope` の共有、利用者が書いた `Send<'f<i64>>`）。どちらも `Type` のすべての種類を列挙する `match` にして、`Type::is_unresolved` の型（型変数、推論変数、適用、部分適用）は偽にした。新しい種類は、判定を書かないとコンパイルできない。
2. `--wasm-feature threads` で、入れ子のグループを持つ有効なプログラムが、誤ってデッドロックのトラップになった（native は完了する）。外側の `Task.parallel [生産者, 消費者]` の生産者が、入れ子の `Task.parallel` の結果を、そのあとで送る形で、結合を待つスレッドが、自分のグループに未着手のアイテムがないと、ほかのグループの消費者を取って結合のフレームの上に載せた。消費者は、そのスレッド自身が結合のあとに送るトークンを待つので先へ進めず、全員が待って判定が出た。ワーカーが新しいグループから先に取る（グループを先頭に足していた）ので、外側のアイテムは最後まで残り、結合を待つスレッドが取りやすかった。native と同じ規則にした: `submit` はグループを公開する lock の中で最初のアイテムを取り、結合を待つあいだは自分のグループのアイテムだけを走らせ、グループは古い順に並べ、`idle` はワーカーだけを数える（前の回の 3. で足した、結合を待つスレッドを数える数え方は不要になった）。2 スレッドで 20,000,000 の入れ子は、修正前は 5 つのプールがすべてトラップし、修正後は完了する（スケジュールに左右されるので、テストは新しいプールを何度も作り、どのスケジュールでも通るように書いた）。
3. `--wasm-feature threads` で、host の `fail()` が済んだあとに `epoch` を読んだスレッドが、そのまま眠り続けることがあった（`fail()` は `failed` を立ててから `epoch` を進めて通知するだけで、`sleep_until_progress` は眠る前に `failed` を見ていなかった）。`epoch` を読んだあと、眠る前にも `check_failed()` を行う。`lock` の隙間は数命令で、決定的には再現できないため、テストは追加せず、順序の論証を `docs/architecture.md` と記録に残した。

## 検証

詳細は `_features/` ではなく、コミットの記録と `docs/architecture.md` にある。要点:

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked --no-fail-fast`（`tests/concurrency.rs`: Atomic・Mutex・Task.scope・Channel の型、拒否、IR）、`scripts/check-runtime-includes.sh`、`git diff --check`。
- `tests/features.mjs` の `concurrency` と `channel`（native と wasm32、`-O0`／`-O3`、トラップ、残存確保数 0、`TSUZURI_TSAN`・`TSUZURI_ASAN`）。
- `tests/sync_runtime.c`、`tests/channel_runtime.c`（1〜4 スレッド、条件変数だけで駆動、TSan と ASan）、`tests/tasks.mjs`（`tests/fixtures/channel_threads` を 1〜4 スレッドの native で）、`tests/trap_return_sync.mjs`（デッドロックを含む）。
- 本物の Worker: `tests/wasm_threads.mjs` と `tests/bindings_threads.mjs`（`Mutex` の競合、生産者と消費者、3 段、複数の生産者、作業キュー、ピンポン、デッドロックのトラップ）。
- Windows: `TSUZURI_WINDOWS_SDK` で `task.c`（`Task`・`Mutex`・`Channel` のプログラム）を MSVC のヘッダーでクロスコンパイルしてリンクし、両方の Windows ターゲットを `cargo check`。実行は CI のみ。
- `Atomic`・`Mutex`・`Channel` を使わないプログラムの IR が、ベースラインと同一であること（89 のフィクスチャ × 7 の構成）。

## 既知の限界

- `Channel` の完了は、スレッド数に左右される。待つタスクの数がスレッド数を超えるパイプラインは、判定のトラップになることがある（子ども 1 つに専用のスレッドは保証しない）。
- native は 1 つの mutex で全チャンネルの操作を守る。チャンネルが多く操作が非常に頻繁なプログラムでは、競合する。threads は、新しい仕事、グループの終わり、チャンネルの変化、競合したロックの解放のたびに、眠っている全員が起きる。
- 外部のスレッド（ホストが自分で作ったスレッド）は、チャンネルの操作と `Task` の呼び出しの間だけ数える。1 つのチャンネルは 1 回の呼び出しの木の中に閉じるので、これが誤判定を作る経路は見つかっていない。
- 同じ型の `Mutex`・`Channel` の循環（`Arc` の中の `Mutex` が別の `Arc` を指す）は作れて、サイクルコレクターがないので解放されない。循環の一方は `Arc.Weak` で持つ。
- ジェネリックな関数が `Mutex.with_lock` を包むとき、結果の型が借用を持ちうる型で使うと、callback の結果を証明できずに `E1013` になる。
- 関数値（を含む値）を `Channel.send` で送るときは、環境が借用を持たない関数を送る場所で作る。関数値を引数で受け取って送る関数は、呼び出し側が借用を持つかどうかを知れないので `E1013` になる（`Owned.Function` の引数も同じ）。`Task` の捕捉と同じ保守性である。
- WASM のトラップは、トラップしたインスタンスを捨てる合図で、`with_lock` のフラグはそのインスタンスに残る（グルーは作り直す）。
- Windows の実行、MSRV 1.85 での Windows ターゲット、ブラウザは、この環境では実行していない。
