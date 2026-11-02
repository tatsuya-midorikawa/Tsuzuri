# TZ04: `select` 式

| 項目 | 内容 |
| --- | --- |
| ID | TZ04 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | TZ03 |
| 後続 | TZ05（`Time.after` との組み合わせ）、TZ07（`Context.done`）、TZ09、TZ12 |
| 状態 | done（2026-11-02） |
| 起票 | 2026-11-01（D-61。HEAD `9a76d8ac` で確認） |
| 承認 | 承認済み（D-61: 構文の形）。決定事項 D1〜D7 は既定案 |
| 手本にする既存実装 | 構文: `src/parse_control.rs` の `match_expression`・`match_arms`（1148〜1220 行。腕の列、`when`、`->`、`body_expression`）。型: `src/check.rs` の `match_value`（腕の本体の型の合流）。所有権: `src/ownership.rs` の `match` の枝の合流。生成: `src/llvm.rs` の `match` の `switch` と、TZ03 の `Channel.send`／`recv` の生成（`alloca` の位置、状態の分岐） |
| 主な影響ファイル | `src/syntax.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/parser.rs`・`src/parse_control.rs`・`src/check.rs`（または新規 `src/select.rs`）・`src/ownership.rs`・`src/llvm.rs`（または新規 `src/llvm_select.rs`）・`src/formatter.rs`・`src/warnings.rs`・`src/effects.rs`・`src/pure_rules.rs`・`src/kg/effects.rs`・`src/kg_capture.rs`・`src/const_rules.rs`・`src/const_machine.rs`・`src/prove_smt.rs`・`src/diagnostic_kinds.rs`・`tests/select.rs`（新規）・`tests/fixtures/select/Main.tz`（新規）・`tests/tzroutine.mjs`・`tests/features.mjs`・`vsc/syntaxes/tsuzuri.tmLanguage.json`・文書 |

## 実装者向け早見

- 範囲: Go の `select`。複数の `Channel` の送受信を同時に待ち、準備のできた腕を一様乱数で 1 つ選ぶ。`default` の腕（待たない）、腕の `when`（Go の nil チャンネルで腕を無効にする書き方の代わり）。ランタイムは TZ01 の `tsuzuri_tz_select`。
- 最初に読む: ① 仕様 1〜4 ② `src/parse_control.rs` の `match_arms` ③ TZ01 の仕様 1.10 と付録 D-5 ④ TZ03 の仕様 3・4。
- 最初に書くテスト: `tests/select.rs` の `parses_select_arms` → `types_select`。E2E の `timeout_like`（TZ05 の前なので、時間の代わりに別の tzroutine が送る）。
- 触ってはいけない: `select` という名前の識別子を使う既存のコード（文脈キーワードの条件。仕様 1）。`match` の構文と意味。
- 完了の見分け方: 次が `a=3 b=2 quit` を出す（腕の選び方は乱数でも、受け取る順は各チャンネルの中で決まる）。

```tsuzuri
let a: Channel<i64> = Channel.make 0
let b: Channel<i64> = Channel.make 0
let quit: Channel<unit> = Channel.make 0
let a2 = Channel.share a
let b2 = Channel.share b
let quit2 = Channel.share quit
tzroutine {
    for i in 1 ..= 3 do
        Channel.send a2 (i as i64)
    for i in 1 ..= 2 do
        Channel.send b2 (i as i64)
    Channel.close quit2
}
let mut last_a = 0i64
let mut last_b = 0i64
let mut running = true
while running do
    select
    | recv x from a -> match x with | Maybe.Some v -> last_a = v | Maybe.None -> ()
    | recv y from b -> match y with | Maybe.Some v -> last_b = v | Maybe.None -> ()
    | recv _ from quit -> running = false
$"a={last_a} b={last_b} quit"
```

## 目的

複数のチャンネルを同時に待つ手段がないと、タイムアウト（TZ05）、取り消し（TZ07）、複数の入力の合流、待たない送受信の試行を書けない。Go の並行処理の書き方の中心にある `select` を、Tsuzuri の `match` と同じ見た目で入れる。

## 着手条件と停止条件

### 着手条件

- TZ03 が done（`grep -n "ChannelSend" src/check.rs`）。
- 共有の数を今の値から読む（`FRONTEND_FORMAT`、`ExprKind` の tag、`CONTEXTUAL_KEYWORDS` の長さ、`Builtin::ALL.len()`）。

### 停止条件

- **S1** `select` を識別子に使う既存のコード（`std/`、`tests/fixtures/`、`examples/`、文書の例）が、新しい構文で読めなくなる。`grep -rnw "select" std tests/fixtures examples _tsuzuri | grep -v "select_"` で確かめる。選択肢: A 仕様 1 の先読みを強める、B 止めて報告する。
- **S2** 所有権の検査で、送信の腕の値が「選ばれなかったときに drop される」ことと、腕の本体の合流を、既存の `match` の合流の仕組みで表せない。
- **S3** 生成 IR が非決定的、`-O0` と `-O3` で結果が違う（乱数の選び方の違いではなく、受け取った値・数が違う）。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `9a76d8ac` で確認）

- `select` は予約語でも文脈キーワードでもない（`grep -rnw select src/lexer.rs src/syntax.rs` に当たりがない）。
- `match` は `match e with` の後に、同じ桁の `| pattern [when guard] -> body` を並べる（`match_arms`、`src/parse_control.rs` 1148 行）。腕がなければ `E0002`（`MatchWithoutArms`）。
- 複数のチャンネルを同時に待つ API は、`DataChannel`（旧 `Channel`）にも、TZ03 の `Channel` にもない。TZ01 のランタイムに `tsuzuri_tz_select(tzr_scase *cases, int64_t n, int32_t block, int32_t *ok)` がある（腕は 72 バイト `{ ptr, ptr, i32, i32, [6 x i64] }`）。

## 変えてはいけない意味

- `select` を識別子として使う既存のコード（仕様 1 の条件に当たらないもの）。
- 評価順序: 腕を上から順に、各腕の中は `when` → チャンネル → 送る値の順に評価する。`when` が偽の腕のチャンネルと値は評価しない。
- 所有権: 送信の腕の値は、腕が選ばれなくても `select` に移される（選ばれなかった値は drop される。D3）。

## 仕様

### 1. 構文

```text
select_expr := "select" arm+
arm         := "|" head ["when" expr] "->" expr
head        := "recv" (identifier | "_") "from" expr
             | "send" expr "to" expr
             | "default"
```

- `select` は文脈キーワード（`CONTEXTUAL_KEYWORDS` に足す）。**`select` の直後の字句が `|` で、その次が識別子 `recv`・`send`・`default` のどれか**のときだけ `select` 式になる。ほかは識別子（`select xs`、`let select = 1` は今までどおり）。腕の `|` は次の行でよい（`match` と同じく、最初の腕の桁にそろえる）。
- `recv`・`send`・`default`・`from` は腕の中だけの文脈キーワード（予約語ではない）。`to` は既存のキーワード。
- 腕は 1 つ以上。`default` は 0 か 1 つ（2 つ以上は `E0002`、新しい kind `select_default_repeated`、`a select has at most one 'default' arm`）。`default` の位置は自由（Go と同じ）。
- 腕の頭の形が違う（`| x -> …` など）は `E0002`（新しい kind `select_arm_expected`、`a select arm starts with 'recv NAME from CHANNEL', 'send VALUE to CHANNEL', or 'default'`）。
- 腕の数の上限は 65,535。超えると `E1017`（新しい kind `select_arm_limit`）。
- 腕の本体は `match` の腕の本体と同じ（`body_expression`）。

```tsuzuri
select
| recv job from jobs when accepting -> handle job
| send report to reports -> ()
| default -> idle ()
```

### 2. 型

| 要素 | 規則 |
| --- | --- |
| `recv x from ch` の `ch` | `ref Channel<'a>`（呼び出しの引数と同じ暗黙の借用。`Channel<'a>` の場所を書けば借りる） |
| `x` | `Maybe<'a>`。`Some v` は受け取った、`None` は閉じて空。`_` なら受け取った値（`Some` のとき）を腕の本体の前に drop |
| `send v to ch` の `ch`・`v` | `ref Channel<'a>` と `'a`。`v` は TZ03 の仕様 3 の借用の検査を受ける（`E1013` の `channel_item_holds_borrow`） |
| `when g` | `bool`（`E1003`） |
| 腕の本体 | すべての腕（`default` を含む）の型が同じ。`select` 式の型はその型（`match` と同じ合流。違えば `E1003`） |
| チャンネルでない式 | `E1003`（`expected Channel<…>, found …`） |

### 3. 実行の意味

1. 腕を上から順に見る。`when` があれば評価し、偽ならその腕を **無効** にして、チャンネルと値を評価しない。有効な腕は、チャンネルの式（借用）、送信なら値の式（移す）の順に評価する。
2. 有効な腕の中で、すぐに完了できるもの（受信: 要素がある・送り手が待っている・閉じている、送信: 受け手が待っている・空きがある・閉じている）があれば、その中から **一様乱数で 1 つ** を選んで完了させる。
3. どれも完了できず `default` があれば、`default` の本体へ。
4. どれも完了できず `default` がなければ、有効な腕のどれかが完了するまで待つ。**有効な腕が 1 つもなく `default` もなければ、永久に待つ**（Go の nil チャンネルだけの `select`。全員が待てばデッドロックの判定）。
5. 選ばれなかった送信の腕の値を、腕の順に drop する。
6. 選ばれた腕が、閉じたチャンネルへの送信なら `Tsuzuri runtime: send on closed Channel` を書いてトラップ（`assert`。TZ03 の `send` と同じ）。
7. 選ばれた腕の本体を、`recv` なら `x` を束縛して実行する。

- デッドロックの判定で起こされた擬似 G（`main` など）は、`select` の位置でトラップする（待ちの理由 `select`）。
- 公平さ: どの準備のできた腕も、同じ確率で選ばれる（TZ01 の D8。再現用の種はない）。

### 4. 生成コード

```text
entry:   %cases = alloca [N x { ptr, ptr, i32, i32, [6 x i64] }]   ; N = 送受信の腕の数
         %arm_of = alloca [N x i32]                                  ; 有効な腕の添字 → 腕の番号
         %slot.i = alloca <腕 i の要素の型>                           ; 各送受信の腕に 1 つ
         %on.i = alloca i1                                           ; 送信の腕が有効か（drop のため）
         %ok = alloca i32
腕ごと:  （when）→ 有効なら cases[count] = { chan, %slot.i, send, 0, 0 }、arm_of[count] = i、count += 1、送信なら値を %slot.i へ store し %on.i = true
         %k = call i64 @tsuzuri_tz_select(ptr %cases, i64 %count, i32 <default がなければ 1>, ptr %ok)
         %k == -1 → default の本体へ。%k == -2 → トラップ（assert）
         %arm = load arm_of[%k]
         送信の腕 j ごと: %on.j かつ %arm != j なら %slot.j の値を drop
         switch i32 %arm → 腕の本体（recv: %ok で Some (load %slot.i) / None、send: %ok == 0 なら report(1) + トラップ）
```

- `alloca` はすべて関数の entry の block に置く（ループの中の `select` でスタックが伸びないように）。
- `N == 0`（`default` だけの `select`）は、ランタイムを呼ばずに `default` の本体へ。
- 宣言 `declare i64 @tsuzuri_tz_select(ptr, i64, i32, ptr)` は `intrinsics` へ。

## 設計

### AST と構文解析

| ファイル | 変更 |
| --- | --- |
| `src/syntax.rs` | `ExprKind::Select(Vec<SelectArm>)`（大きい field は `Box`。GUIDE-LITE §10 の stack の注意）。`pub struct SelectArm { head: SelectHead, guard: Option<Expr>, body: Expr, span: Span }`、`pub enum SelectHead { Recv { name: Ident, channel: Box<Expr> }, Send { value: Box<Expr>, channel: Box<Expr> }, Default }`（`name` は `_` のとき `text == "_"`）。`visit`・`visit_mut` に腕の各式。`CONTEXTUAL_KEYWORDS` に `"select"` |
| `src/syntax_codec.rs` | `ExprKind::Select` の tag（今の最大 + 1）。`SelectArm`・`SelectHead` の put／get（`MatchArm` と同じ書き方。`SelectHead` の tag は 0・1・2）。件数のテスト |
| `src/frontend_cache.rs` | `FRONTEND_FORMAT` + 1 |
| `src/parser.rs` | `primary` に `TokenKind::Ident(name) if name == "select" && self.select_ahead() => return self.select_expression()`。`select_ahead`: 次が `TokenKind::Pipe`、その次が `Ident("recv" \| "send" \| "default")` |
| `src/parse_control.rs` | `fn select_expression`: `enter()`、`take()`（`select`）、`select_arms()`。`select_arms` は `match_arms` と同じ桁の規則・`stop_at_arm`・`stop_at_arrow` で、頭を `recv`（`self.ident()` か `_`、`from` を `TokenKind::Ident("from")` で確認、`expression_inner(0, true, true)`）・`send`（値の式、`TokenKind::To`、チャンネルの式）・`default` で読み、`when`、`->`、`body_expression`。誤りは仕様 1 の kind |
| `src/formatter.rs` | `select` を `match` と同じ桁で出す（`\| recv x from ch ->`） |
| `src/diagnostic_kinds.rs` | `E0002` に `select_arm_expected`・`select_default_repeated`、`E1017` に `select_arm_limit`（G23 の手順。`since "0.1.0"` の群の (code, id) の順の位置）。`src/explain/` の kind の説明を足す（G23 の仕様 S1） |

### 型検査（`src/check.rs` の `composed_expression` から、新規 `src/select.rs` の `Checker::select` を呼ぶ）

- 各腕: `when` → `self.expression(guard, Some(&Type::Bool))`。`recv`: チャンネルを `self.expression(channel, None)` で検査し、`autoderef` してから `Channel<'a>`（std の record）であることを確かめ、`'a` を得る（違えば `E1003`）。束縛 `x : Maybe<'a>` を腕の scope に入れて本体を検査する。`send`: チャンネル → 値を `Some(&'a)` で検査。
- 本体の型の合流は `match_value` の腕の合流と同じ関数を使う（期待する型があればそれ、なければ最初の腕の型）。
- 結果は `TypedExprKind::Select { arms: Vec<TypedSelectArm>, default: Option<Box<TypedExpr>> }`、`TypedSelectArm { guard: Option<TypedExpr>, channel: TypedExpr, kind: TypedSelectKind, body: TypedExpr }`、`TypedSelectKind::Recv { local: Option<Local> } | Send { value: TypedExpr }`。`children`・`try_for_each_child`・`try_for_each_child_mut` に、腕の順（guard → channel → value → body）で足す。

### 所有権（`src/ownership.rs`）

- 腕の評価部分（guard・channel・value）は、上から順に通常の式として評価する（channel は借用、value は移す）。`value` は TZ03 の要素の借用の検査（`ChannelItemHoldsBorrow`）を受ける: `sent_item` と同じ判定を `TypedSelectKind::Send` の `value` に適用する。
- 本体は `match` の枝と同じく、それぞれ同じ入口の状態から検査して合流する（`default` も 1 つの枝）。`recv` の束縛は、その枝の新しい所有値。
- 移された送信の値は、どの枝に入っても「消費済み」（選ばれなかった値は生成コードが drop する）。

### 生成

- 仕様 4。新規 `src/llvm_select.rs`（`impl FunctionEmitter` の `fn select(&mut self, arms, default, ty) -> String`）を `src/llvm.rs` の `TypedExprKind::Select` の腕から呼ぶ。結果の値は、腕の本体の値を `phi` で集める（`match` の生成と同じ方法を使う。`grep -n "fn match_expression\|phi" src/llvm.rs` で既存の合流を探す）。

### 効果・ツール

| ファイル | 変更 |
| --- | --- |
| `src/effects.rs` | `node` の `K::Select` は、TZ03 の `ChannelSend`・`ChannelRecv` の効果の和（`select` の中の腕の効果は子が足す） |
| `src/pure_rules.rs`・`src/const_rules.rs`・`src/const_machine.rs`・`src/prove_smt.rs` | `K::Select` を拒否（TZ03 のチャンネルの操作と同じ診断） |
| `src/kg_capture.rs` | `K::Select` の各腕に、`Channel.recv`／`Channel.send` への `calls` の辺（図の並行の区間に入る） |
| `src/warnings.rs` | 使わない束縛（`recv x` の `x` を使わない）は既存の未使用の警告（`_x` で抑える） |
| `vsc/syntaxes/tsuzuri.tmLanguage.json` | `select` を文脈キーワードの色に。腕の `recv`・`send`・`default`・`from` は `select` の直後の腕の中だけ（正規表現で `^\s*\|\s*(recv|send|default)\b`） |

## 前提とする他チケットのインターフェース

- TZ01 の `tsuzuri_tz_select` と `tzr_scase` の配置（72 バイト）。TZ03 の `Channel.Channel`、要素の借用の検査、`tsuzuri_tz_report(1)`。

## 他チケットへ提供するインターフェース

- `ExprKind::Select`・`TypedExprKind::Select`。TZ05 の `Time.after`、TZ07 の `Context.done` は `Channel` を返すので、そのまま腕に書ける。

## 実装手順

1. **変更**: ブランチ `feat/tz04-select`、基準、S1 の `grep`。**確認**: `select` の既存の使用の一覧をメモ。
2. **変更**: AST・codec・構文解析・formatter・診断の kind。型検査は仮の拒否（TZ02 の手順 2 と同じ）。**確認**: `tests/select.rs` の `parses_select_arms`（3 種の頭、`when`、`default` の位置、`select` が識別子のままの 5 例: `select xs`・`let select = 1`・`f select`・`select |> f`・`match v with | A -> select` の次の行の `| B -> 0`）、`rejects_bad_arms`（`E0002` の 2 つの kind、`E1017`）、`formats_select`。
3. **変更**: 型検査。**確認**: 受理と拒否（仕様 2 の表。各拒否のコード）。
4. **変更**: 所有権。**確認**: 送信の値を `select` のあとで使うと `E1012`。借用を運ぶ値の送信で `E1013`。腕の本体ごとの移動の合流（`match` と同じ例）。
5. **変更**: 生成。**確認**: IR が決定的。`alloca` が entry にだけある（ループの中の `select` の IR を検査）。
6. **変更**: 効果・ツール。**確認**: `cargo nextest run --locked`。
7. **変更**: E2E。**確認**: `node tests/tzroutine.mjs target/debug/tsuzuri`、`node tests/features.mjs target/debug/tsuzuri select`。
8. **変更**: 文書と規則 ID。**確認**: 文書の検査一式。
9. **変更**: T2 と CI（`ci:full`）。

## テスト計画

### E2E（`tests/fixtures/select/Main.tz`、native の `-O0`・`-O3`、P 1 と 4）

| export／プログラム | 内容 | 期待 |
| --- | --- | --- |
| 早見の例 | 3 つのチャンネル | `a=3 b=2 quit` |
| `fairness` | 常に準備のある 2 つのチャンネル（2 つの tzroutine が送り続ける）から 10,000 回 | 両方が 3,000 回以上選ばれる（1 を返す） |
| `default_when_idle` | 何も準備がないとき `default` | `default` の値 |
| `send_and_recv` | 送信の腕と受信の腕を同じ `select` に（10,000 往復の中継） | 失いも重複もない合計 |
| `guards` | `when false` の腕は選ばれない。`when` の偽の腕のチャンネルの式が評価されない（式に `Atomic` を数える関数を置く） | 数えが 0 |
| `closed_recv` | 閉じたチャンネルの受信の腕は `None` で選ばれる | `None` を 1 回 |
| `drops_unchosen` | 選ばれない送信の腕の値（`Drop` を持つ利用者の型）が 1 回だけ drop される | drop の数 |
| `same_channel_twice` | 同じチャンネルを 2 つの受信の腕に | どちらかで 1 回受ける |
| `in_loop` | ループの中の `select` を 100,000 回 | スタックが伸びない（`deep` の上限の中で終わる）、合計が一致 |

- トラップ: 閉じたチャンネルへの送信の腕が選ばれる → `send on closed Channel`。
- デッドロック: 有効な腕のない `select`（すべて `when false`、`default` なし）を `main` で → `all tzroutines are asleep - deadlock! (1 waiting: select (no cases))`。

## ドキュメント

- 新規 `_tsuzuri/language-reference/async-tasks-and-lazy/select.md`（「select 式」）: この記事のポイント、基本（早見の例）、腕の 3 つの形、`when`（Go の nil チャンネルの代わり）、`default`（待たない試行）、評価順序と所有権（選ばれない送信の値は drop される。値を失いたくなければ `Channel.try_send` を使う）、公平さ、デッドロック、生成コードの費用（ロックを腕の数だけ取る）、Go との対応（`case v := <-ch` → `| recv v from ch`、`case v, ok := <-ch` → `Maybe`、`case ch <- v` → `| send v to ch`、`default`、nil チャンネル → `when`）、他の言語（Rust の `crossbeam::select!`、Kotlin の `select`）。
- `values-and-functions/keywords.md`（文脈キーワード `select`・`recv`・`send`・`default`・`from`）、`index.md`、`channel.md`（`select` へのリンク）、`docs/language.md`（節「select」）、`compiler/diagnostics.md`（生成の表）。
- `spec/rules/TSK.md`: `TSK-020`（A `select` MUST complete exactly one ready arm, or run `default` when none is ready.）、`TSK-021`（An arm whose `when` guard is false MUST NOT be chosen and its channel and value MUST NOT be evaluated.）、`TSK-022`（A send value of an arm that is not chosen MUST be dropped exactly once.）。適合性テスト。

## 受け入れ条件

- [ ] 仕様 1〜4 のとおりに動き、拒否は表のコードと kind で出る。
- [ ] `select` を識別子に使う既存のコードが変わらず読める（S1）。
- [ ] E2E が native の `-O0`・`-O3`、P 1 と 4 で通り、`live == 0`。
- [ ] 文書・規則 ID・診断の kind を更新し、検査が通る。

## 落とし穴

- **`select` の先読みは 2 字句**: `select` の次の `|` だけで判定すると、`match` の腕の本体の最後が `select` という変数のとき、次の腕を `select` の腕として読んでしまう。`recv`・`send`・`default` まで見る。
- **`when` の偽の腕の値を評価しない**: 生成コードで、値の式を `when` の分岐の中に置く。外に置くと副作用（drop・確保）が起きる。
- **`%on.j` を初期化する**: 無効の送信の腕の `%on.j` が未初期化だと、選ばれた腕によっては、未初期化の値を drop する。entry で `false` を store する。
- **`ok` の意味は腕の種類で違う**: 受信は 0 が「閉じて空」、送信は 0 が「閉じていた」（トラップ）。
- **同じチャンネルが 2 つの腕**: ランタイムはロックを 1 回だけ取る。生成コードは同じチャンネルを 2 回 `cases` に入れてよい。
- **腕の本体の中の `break`**: `select` は `match` と同じく、外のループの `break`・`continue` をそのまま通す（`select` 自体はループではない）。

## 対象外

- `DataChannel` の端（`Sender`・`Receiver`）を腕に書くこと（`E1003`。`select` は `Channel` だけ。TZ03 の仕様 9）。
- 時間の腕（`| after 100 ->` のような専用の構文）。`Time.after` のチャンネル（TZ05）を `recv` で待つ。
- 腕の頭の任意のパターン（`| recv (Maybe.Some x) from ch`）。`Maybe` の値を本体で `match` する。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | `match` と同じ `\|` の腕の列。頭は `recv x from ch`・`send v to ch`・`default` | `<-` の字句を足すと `a<-1`（`a < -1`）の読み方が変わる。語は文脈キーワードにする | 既定案 |
| D2 | `recv` の束縛は `Maybe<'a>` | Go の `v, ok`。ゼロ値を作らない | 既定案 |
| D3 | 送信の腕の値は評価時に移し、選ばれなければ drop | Go と同じく、全腕のオペランドを 1 回ずつ評価する。値を取り戻したい場合は `try_send` | 既定案 |
| D4 | nil チャンネルの代わりに腕の `when` | Tsuzuri に nil はない。`when` が偽の腕はオペランドを評価しない | 既定案 |
| D5 | 有効な腕がなく `default` もない `select` は永久に待つ | Go と同じ。デッドロックの判定が見つける | 既定案 |
| D6 | 準備のできた腕は一様乱数で選ぶ | Go と同じ。飢餓を防ぐ | 既定案 |
| D7 | 腕の数の上限 65,535（`E1017`） | ランタイムが並びを `uint16_t` に置く | 既定案 |

## 実装と検証（2026-11-02）

- **範囲**: 仕様 1〜4 をすべて実装した（1 つの PR。TZ03 の PR #81 のブランチの上）。共有の数は `FRONTEND_FORMAT` 16 → 17、`ExprKind` の tag 48、`CONTEXTUAL_KEYWORDS` 9 → 10（`select`）。S1 の `grep` の当たり（`std/Set.tz` の `select left right …`、`Simd.select`）はどれも `select` の直後が `|` でなく、読み方は変わらない。
- **構文**: `ExprKind::Select(Vec<SelectArm>)`、`SelectHead::{Recv, Send, Default}`（codec、`SelectArm::expressions`）。`parser.rs` の `select_ahead`（2 字句）、`parse_control.rs` の `select_expression`・`select_arms`。formatter は `match` と同じく腕をそろえる。診断の kind `select_arm_expected`・`select_default_repeated`（`E0002`）、`select_arm_limit`（`E1017`）と、前の 2 つの `src/explain/` のページ。
- **型と下ろし方**: `src/select.rs`。腕のチャンネルの操作を 1 つの葉 `TypedExprKind::SelectWait` にし、本体はその結果の組 `(番号, Maybe<'a> ...)` への `match` にした。所有権の合流・流れの解析・本体の生成は `match` のものを使う。
- **生成**: `src/llvm_select.rs`（`select_wait`）。`tzr_scase` の並びと番号の並びを入口の `alloca` に置き、`tsuzuri_tz_select` を呼ぶ。`TZR_SINGLE` の `tsuzuri_tz_select` を `tzroutine.c` に足した。
- **効果・ツール**: 効果（Atomic・trap、待つなら diverge）、`pure`・`const`・SMT の拒否、KG の `calls`（腕ごとに `Channel.send`・`Channel.recv`）、`warnings`、VS Code の文法。
- **チケットから外れた判断**:
  1. **型付きの木**: 設計の `TypedExprKind::Select { arms, default }`（本体を持つ新しい制御の節）ではなく、葉の `SelectWait` と既存の `match` にした。本体を持つ節を足すと、所有権の合流・流れ・last-use・範囲・const・生成など 19 のファイルが `match` と同じ扱いを要るが、組への `match` なら葉の 1 つで済み、`match` の検査と生成をそのまま使える（S2 も `match` の合流の仕組みで表せた）。
  2. **`default` に `when` は付けない**: 仕様 1 の文法は許すが、偽の `default` は「待つ」と同じで意味がない。`E0002`（`select_arm_expected`、`a 'default' arm takes no 'when' guard`）にした。
  3. **その場で作ったチャンネル**: 腕のチャンネルはほかの腕の評価と待ちの間ずっと借りるので、関数の戻り値などの一時の値は `E1013`（`borrow_of_temporary`、`bind the channel to a name first`）にした。場所と参照は書ける。
  4. **向きのあるハンドル**: TZ03 の Phase 2 の `ReceiveOnly`（受け取る腕）・`SendOnly`（送る腕）も腕に書ける。
  5. **大きな select**: 腕の情報は入口の `alloca` だが、65 個以上の腕はヒープに置く（72 バイト × 65,535 は tzroutine のスタックのガードを越えうる）。
  6. **既定の WebAssembly**: 仕様にはないが、spawn しないプログラムの `select` を `TZR_SINGLE` で動かす（`select_single` の suite）。待つべき `select` はトラップ。
  7. **`select_arm_limit` の説明のページ**は作らない（悪い例が 65,536 個の腕になる）。
- **検証**: `cargo fmt`・`cargo clippy --all-targets -- -D warnings`。`tests/select.rs` 6 件（3 つの頭と `when` と `default` の位置、`select` を識別子にする 5 例、`E0002` の 2 つの kind と `E1017`、formatter、型の受理と 7 つの拒否、送る値のムーブと借用、入口の `alloca`）。E2E: `features.mjs select`（native、`-O0`／`-O3`、P 1・4。早見の例 `321`、`fairness`（10,000 回で両方が 3,000 回以上）、`send_and_recv`、`guards`（数えが 0）、`closed_recv`、`drops_unchosen`（2 回）、`same_channel_twice`、`in_loop` 100,000 回、トラップ 2 つ）、`select_single`（native と wasm32、import なし、7 件とトラップ 2 つ）。適合性 `TSK-020`〜`022`（`conformance.mjs --area TSK` 22 件）、`check-spec --require-markers`、`gen-docs --check`、`check-cli-docs`、変更した全ページの `check-docs`。
- **文書**: 新規 `async-tasks-and-lazy/select.md`。`index.md`、`values-and-functions/keywords.md`、`built-in-types-and-modules/channel.md`、`compiler/diagnostics.md`・`conformance.md`（生成）、`docs/language.md`（節「select」と文脈キーワード）、`docs/architecture.md`（`### select（TZ04）`）、`README.md`、`spec/rules/TSK.md`、`vsc/syntaxes/tsuzuri.tmLanguage.json`。
