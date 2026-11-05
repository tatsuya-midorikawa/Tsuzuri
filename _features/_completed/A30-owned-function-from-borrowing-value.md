# A30: 借用を持つ関数値から `Owned.function` を作れてしまい、`Routine.go`・`WaitGroup.go`・`Time.after_func` で解放後の読み出しになる

| 項目 | 内容 |
| --- | --- |
| ID | A30 |
| 優先度 | P0 |
| 規模 | S |
| 依存 | TZ02, TZ05, TZ06 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-05（A28 の原因の調査中に発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（健全性の穴を塞ぐ。文書の「捕捉できるのは所有値だけ」の約束どおりに拒否する。今受理している危険なプログラムが `E1013` になる） |
| 改善する劣位 | 不具合（メモリ安全性）: `let g: unit -> unit = \() -> ... borrowed ...`（`borrowed` は局所の `ref`）のように、借用を捕捉した関数値を変数に置いてから `Owned.function g` に渡すと、検査を通る。それを `Routine.go` に渡すと、tzroutine が関数の終わりで解放された文字列を読み、ごみを出力するか `SIGSEGV` で落ちる。ラムダを `Owned.function` に直接書けば正しく `E1013`（`owned_function_captures_reference`）になる |
| 手本にする既存実装 | `src/closures.rs:128〜150`（`Owned.function` に直接書いたラムダの捕捉を型で検査し、`OwnedFunctionCapturesReference` を出す）と、`src/ownership.rs:3129〜3139`（tzroutine の本文 `Type::Once` の捕捉が loan を持てば `TaskCaptureRetainsBorrow`） |
| 主な影響ファイル | `src/ownership.rs`, `src/closures.rs`, `tests/user_drop.rs`, `tests/tzroutine.rs`, `tests/timers.rs`, `_tsuzuri/language-reference/built-in-types-and-modules/owned.md`, `_features/README.md` |

## 目的

`Owned.Function` が「捕捉した値を所有する関数」であるという約束（[Owned](../../_tsuzuri/language-reference/built-in-types-and-modules/owned.md) の「捕捉できるのは所有値だけ」）を、
引数がラムダ式でない場合にも守る。`Routine.go` などの、関数を別の tzroutine やタイマーへ渡す組み込み関数は、`Owned.Function` が借用を持たないことを前提にしているので、
約束が破れると、安全なコードだけで解放後の読み出しが起きる。

## 着手条件と停止条件

### 着手条件

- TZ02・TZ05・TZ06 が `_features/README.md` で done であること（HEAD で done）。
- GUIDE §2.3 の基準コマンドが成功し、「再現」を確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `Owned.function` に名前付きの関数（`Owned.function double`。`tests/user_drop.rs:429`）や、所有値だけを捕捉する関数値を渡すプログラムが拒否される。
- `own Owned.Function<…>` の引数をそのまま `Routine.go` に渡すプログラム（[Routine](../../_tsuzuri/language-reference/built-in-types-and-modules/routine.md) の `spawn` の例）が拒否される。
- `std/`・`examples/`・`tests/fixtures/` のプログラムが新しく拒否される（調べた範囲では、ラムダ式でない引数の `Owned.function` は `tests/user_drop.rs` の名前付き関数の 2 か所と `owned.md:189` だけ）。
- `Owned.function` を通らずに `Owned.Function` の値を作る経路（レコードの literal や更新）が見つかった。その経路も塞ぐかを決めてから進める。

## 現状（HEAD `5ee6b0f2` で確認）

- `src/closures.rs:128〜150`: `Owned.function` に直接書いたラムダ（`LambdaKind::Owned`）の捕捉に `Send` を要求し、参照を含む捕捉を
  `an owned function captures only owned values; {型} contains a reference`（`OwnedFunctionCapturesReference`、`E1013`）にする。検査は捕捉の型だけで、
  ラムダ式でない引数（変数・引数・式）には何もしない。
- 普通の関数値（`LambdaKind::Function`）は、局所の `ref` を捕捉できる。型 `unit -> unit` は、環境が借用を持つかを表さない（A28 の「現状」）。
- `src/ownership.rs` の借用検査は、関数値の `Value` に、捕捉した借用の loan を持たせる。`Owned.function g` の結果は、`Owned.Function` の型が `carries_loans`
  なので、`g` の loan を受け継ぐ。そのため `Owned.function g` を関数から返すと `borrow_outlives_block`（`borrowed value does not live long enough to leave this block`）で正しく拒否される。
- しかし `Routine.go`・`WaitGroup.go`・`Time.after_func` は `own Owned.Function<…>` を受け取って結果が `unit`（またはタイマー）で、`eval_call` は引数の loan を
  検査しない。loan は結果に渡らずに消え、借用の持ち主が先に解放される。tzroutine の本文の捕捉（`Type::Once`）なら同じ値は `TaskCaptureRetainsBorrow` で拒否される。
- 局所の `ref` の代わりに、外側の関数の引数 `f: unit -> unit` を `Owned.function f` に渡す形も通る（`f` は呼び出し側の借用を持ちうる）。
- `Rc` を捕捉する普通の関数値は作れない（`E1005`: `cannot capture Rc<i32> in a function value; ...`）ので、`Send` の側の穴は無い。

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64）

```text
def spawn :: i64 -> unit
fn spawn count =
    let text = String.repeat count (ref "abcdefgh")
    let borrowed = ref text
    let g: unit -> unit = \() -> {
        Time.sleep 200000000;
        do! IO.write_line $"{String.length borrowed} {borrowed}"
    }
    Routine.go (Owned.function g)

def main :: unit -> i32 = \() ->
    spawn 4
    0
```

`tsuzuri check` は何も報告しない。`tsuzuri run` を 3 回実行すると、3 回とも `32 剀Ҧ     攀Ҧ             32` のようなごみを出力する（解放した文字列を読んでいる）。
調査では、`String.repeat 64` の版が 3 回とも `E2005 … signal: 11 (SIGSEGV)` で落ちた。`borrowed` の代わりに所有の `text` を捕捉する版は
3 回とも `32 abcdefghabcdefghabcdefghabcdefgh` を出力する。

`tsuzuri check` が受理する同じ形（HEAD `5ee6b0f2` で確かめた）:

- `WaitGroup.go (ref group) (Owned.function g)`（`g` は `borrowed` を出力する `unit -> unit`）。
- `let _timer = Time.after_func (5 * Time.millisecond()) (Owned.function g)`。
- `fn spawn f = Routine.go (Owned.function f)`（`def spawn :: (unit -> unit) -> unit`）。

拒否される形（正しい）:

- `Routine.go (Owned.function (\() -> { do! IO.write_line $"{borrowed}" }))` → `error[E1013]: an owned function captures only owned values; ref string contains a reference`。
- `Owned.function g` を関数から返す → `error[E1013]: borrowed value does not live long enough to leave this block`。

`ErrGroup.go` と `Context.after_func` は同じ `own Owned.Function` を受け取るが、まだ確かめていない（手順 1）。

## 仕様

- `Owned.function x` は、`x` がラムダ式でないとき、`x` の値が借用を持たない（借用検査の `Value` の loan が空）ことを要求する。持てば `E1013`、kind は
  `owned_function_captures_reference`、message は `an owned function captures only owned values; this function value holds a borrow`（D2）。
  位置は `x`、借用の位置（loan の場所）を note で示す。
- 名前付きの関数（環境の無い関数）と、所有値だけを捕捉する関数値は、今と同じく受理する。
- 外側の関数の引数（`f: unit -> unit`）を `Owned.function f` に渡す形は拒否する。引数は呼び出し側の借用を持ちうる。所有の関数を受け取りたいときは、引数の型を
  `own Owned.Function<…>` にする（help に書く）。
- `Routine.go`・`WaitGroup.go`・`ErrGroup.go`・`Time.after_func`・`Context.after_func` は変えない（受け取る `Owned.Function` は、作るときの検査で借用を持たない）。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/ownership.rs` | `eval_call` の組み込み関数 `Owned.function`（`Builtin::OwnedFunction`）の引数 | 引数がラムダ式でなく、その `Value` の loan が空でなければ `OwnedFunctionCapturesReference` を返す |
| `src/diagnostic_kinds.rs` | `OwnedFunctionCapturesReference` | 既存の kind を使う。help（`own Owned.Function<…>` を受け取る）を足す |
| `src/explain/codes/E1013.md` | kind の説明 | ラムダ式でない引数の場合を足す（`tsuzuri explain` の検査があれば、それに合わせる） |
| `tests/user_drop.rs`・`tests/tzroutine.rs`・`tests/timers.rs` | 新しいテスト | 下の「テスト計画」 |
| `_tsuzuri/language-reference/built-in-types-and-modules/owned.md` | 「捕捉できるのは所有値だけ」 | 関数値を渡す場合の例と `E1013` を足す |

### アルゴリズム

`Owned.function` の呼び出しで、引数を評価した `Value` の `loans` を見る。ラムダ式の引数は `src/closures.rs` が型で検査済みなので、二重に報告しない（ラムダ式は
`closures::lower` の後で `LambdaKind::Owned` の閉包になる。どの形が「直接書いたラムダ」かは、下げた後の式で見分ける）。名前付きの関数の参照は loan を持たない。

## 実装手順

### 手順 1: ベースラインと残りの API の確認

- 変更: なし（`target/scratch/a30/` の program だけ）。
- 内容: GUIDE §2.3 の基準コマンド。「再現」と、`ErrGroup.go`・`Context.after_func` の同じ形を書いて `tsuzuri check` する。`Owned.Function` をレコードの literal で作れるか
  （`std/Owned.tz` のフィールドの可視性）を確かめる。
- 確認: 結果をこのチケットの末尾に追記した。停止条件の 4 つ目に当たれば停止する。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/user_drop.rs`（`Owned.function` の受理・拒否のテストの近く。429・479 行）、`tests/tzroutine.rs`、`tests/timers.rs`。
- 内容: 「テスト計画」の拒否のテスト（修正前は受理されるので失敗する）と、受理のテスト（名前付きの関数、所有値だけを捕捉する関数値、`own Owned.Function` の引数の `Routine.go`）。
- 確認: 拒否のテストだけが失敗する。

### 手順 3: 検査を足す

- 変更: `src/ownership.rs`、`src/diagnostic_kinds.rs`（help）。
- 確認: 手順 2 のテストが成功する。`cargo test --locked` の全件が成功する。

### 手順 4: 既存のプログラムが変わらないこと

- 確認: `tests/fixtures/`・`examples/` の `tsuzuri check --json` が修正の前後で一致する。`node scripts/check-docs.mjs` を `owned.md`・`routine.md`・`tzroutine.md`・`timer.md` に。

### 手順 5: ドキュメントと最終確認

- 「ドキュメント」の各ページ。T2。

## テスト計画

### Rust テスト

- 拒否（`E1013`、kind `owned_function_captures_reference`）:
  - `Routine.go (Owned.function g)`（`g` は局所の `ref` を捕捉する `unit -> unit`）。
  - `WaitGroup.go (ref group) (Owned.function g)`、`Time.after_func d (Owned.function g)`、手順 1 で確かめた `ErrGroup.go`・`Context.after_func`。
  - `fn spawn f = Routine.go (Owned.function f)`（引数の関数）。
  - `let o = Owned.function g` だけ（使わなくても、作った時点で拒否する）。
- 受理:
  - `Owned.function double`（名前付きの関数。`tests/user_drop.rs:429` の既存のテスト）。
  - 所有値（`string`・レコード）だけを捕捉する関数値 `g` の `Owned.function g` と、その `Routine.go`。
  - `def spawn :: own Owned.Function<unit, unit> -> unit = \work -> Routine.go work`（[Routine](../../_tsuzuri/language-reference/built-in-types-and-modules/routine.md) の例）。

### E2E

- 受理する形（所有値だけを捕捉する関数値の `Routine.go`）を `tests/tzroutine.mjs` で native の `-O0`・`-O3` に通し、出力が正しいこと。

### 既存テストへの影響

- 拒否が増えるのは、今まで危険だったプログラムだけ（手順 4 で確かめる）。

### 性能

- 影響なし（検査だけ）。

## ドキュメント

- `_tsuzuri/language-reference/built-in-types-and-modules/owned.md`（214 行の近くの「捕捉できるのは所有値だけ」）: 関数値を `Owned.function` に渡すとき、その関数値が借用を持てば `E1013` になることと、
  引数の関数を所有にしたいときは `own Owned.Function<…>` を受け取ることを、例と実行結果で書く。
- `_tsuzuri/language-reference/built-in-types-and-modules/routine.md`（7・19・47〜57 行）、`async-tasks-and-lazy/tzroutine.md`（201〜213 行）: 説明が直した後の振る舞いと一致するか確かめる。
- `src/explain/codes/E1013.md`: kind の説明。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] 「再現」のプログラムが `E1013`（`owned_function_captures_reference`）で拒否される。
- [ ] `WaitGroup.go`・`Time.after_func`・引数の関数の形と、手順 1 で確かめた形が拒否される。
- [ ] 名前付きの関数、所有値だけを捕捉する関数値、`own Owned.Function` の引数の `Routine.go` が受理され、正しく動く。
- [ ] `tests/fixtures/`・`examples/` の診断が変わらない。
- [ ] `check-docs.mjs` と T2 が成功する。

## 落とし穴

- `Routine.go` などの側で loan を検査すると、`own Owned.Function<…>` の引数をそのまま渡す正しいプログラムまで拒否しうる（引数の値の loan の扱いしだい）。作る側（`Owned.function`）で検査する（D1）。
- ラムダ式の引数は `src/closures.rs` がすでに報告する。同じ位置に 2 つの診断を出さない。
- A28（誤った拒否を減らす）と同時に入れるときは、A28 の「一時」「読み通し」がこの検査の `Value` を空にしないことを、拒否のテストで確かめる。

## 対象外

- 関数の型に、環境が借用を持つかどうかを表す印を足すこと（言語の変更になる）。
- `Owned.Function` の値を、借用を持つ関数値から作れるようにする別の API（借用の期間を持つ所有関数など）。

## 決定事項

### D1: 作るときに検査する

- 決定: `Owned.function x` で `x` の借用を拒否し、`Owned.Function` を受け取る組み込み関数は変えない。
- 理由: `Owned.Function` は「捕捉を所有する」という型の約束で、受け取る側はそれを前提にしている。作る経路を 1 つに保てば、受け取る側の API が増えても穴は開かない。
- 状態: 既定案（実装者はこの案に従う）。

### D2: 既存の kind と code を使う

- 決定: `E1013` の `owned_function_captures_reference` を使い、message はラムダ式の場合（`{型} contains a reference`）と区別して `this function value holds a borrow` にする。
- 理由: 利用者にとっては同じ規則（所有関数は借用を捕捉できない）である。新しい kind は診断の索引（`src/diagnostic_kinds.rs`、生成した文書）を増やすだけになる。
- 状態: 既定案。

## 実装と検証（2026-11-06）

### 手順 1: ベースラインと残りの API

`main` の最新の `ci.yml` は緑（基準）。HEAD `5ee6b0f2` と同じ `src/` の dev バイナリで、`target/scratch/a30/` の program を `tsuzuri check` した結果:

- 受理されていた（穴）: 「再現」の `Routine.go (Owned.function g)`（`tsuzuri run` は 2 回とも `32 釀Ŧ …` のようなごみを出力）、`ErrGroup.go group (Owned.function g)`（`g: unit -> Result<unit, string>` が `borrowed` を読む）、
  `Context.after_func ctx (Owned.function g)`、`Time.after_func`、引数の関数 `fn spawn f = Routine.go (Owned.function f)`、部分適用 `Owned.function (show (ref text))`、
  パイプライン `g |> Owned.function`、関数値にした `let make = Owned.function` からの `make g`、そして **`Owned.function` に直接書いたラムダが借用を持つ関数値を捕捉する形**
  `Owned.function (\() -> g ())`（捕捉の型 `unit -> unit` は参照を含まないので `src/closures.rs` は通す）。
- 塞がっていた: レコードの literal `Owned.Function { run: g }`・更新 `{ base with run = g }`・フィールドの読み出しはどれも `E1022`（表現が不透明）。停止条件の 4 つ目には当たらない。

### 手順 2〜3: テストと検査

- `src/ownership.rs`:
  - `eval_call`: 組み込み関数 `Owned.function` を直接呼ぶとき、第 1 引数を評価した `Value` の loan が空でなければ `owned_function_captures_reference`（`E1013`）。位置は引数、loan を親へたどった最初の借用の位置を note（`borrowed_here`。呼び出し側の借用なら `a parameter may hold a borrow of its caller's values`）、
    help は `capture an owned value, such as the owner itself or a copy, instead of a borrow; a function that receives work to run later takes an 'own Owned.Function<unit, unit>' parameter instead of a function value`（型は呼び出しの結果の型）。
    callee は値として評価しない（`Mutex.with_lock` と同じ形）。
  - `E::Binary` の `Pipe`: `g |> Owned.function` に同じ検査（D-59 のパイプラインを保つ）。
  - `eval_value`: `Owned.function` を関数値として評価すると `must_apply_directly`（`Owned.function must be applied directly, so that its argument can be checked for borrows: call it with the function value together`）。生成した組み込みの包み関数の引数は検査しない（`is_generated_parameter`。その呼び出し側がここで拒否される）。
  - `check_body`: `Owned.Function` 型の引数（と本体から見た捕捉）には呼び出し側の loan を与えない（`Type::is_owned_function`）。
- テスト: `tests/tzroutine.rs` の `an_owned_function_from_a_function_value_holds_no_borrow`（拒否 9 形・受理 7 形。message・kind・note・help を照合）、`tests/timers.rs` の `the_function_of_a_timer_holds_no_borrow`、`tests/user_drop.rs` の `owned_function_lambdas_only_borrow_their_captures` に拒否 1・受理 1。
  修正前は拒否の 3 テストだけが失敗することを確かめた（`cargo test --locked --no-fail-fast --test tzroutine --test user_drop --test timers`: 18/19・25/26・11/12）。修正後は 19・26・12 件すべて成功。

### 手順 4: 既存のプログラム

`tests/fixtures/*/`（164）と `examples/*/`（13）の `tsuzuri check --json` を修正の前（HEAD `5ee6b0f2` と同じ `src/` の dev バイナリ）と後で比べ、一致した。途中で一度 `std/ErrGroup.tz` の `start` が拒否された（下の判断 3）ので、それを直してから比べ直した。

### 手順 5: ドキュメントと T2

- T0: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。
- T1: `cargo nextest run --locked` 2936 件成功（4 skipped）。
- `node tests/tzroutine.mjs target/debug/tsuzuri`: 159 runs passed（native の `-O0`・`-O3`。所有値だけを捕捉する関数値の `Routine.go` と、`h |> Owned.function` を `own Owned.Function` の引数へ渡す program を足した）。
- `node tests/features.mjs target/debug/tsuzuri tzroutine`（13 件）・`context`（15 件。`errgroup_*` を含む）・`timers`（9 件）成功。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs` を `owned.md`・`routine.md`・`tzroutine.md`・`timer.md` に: 4 pages, 110 links, 33 checked examples, 60 native runs（O0/O3）。
- `node scripts/gen-docs.mjs --check`（19 blocks, 0 stale）・`node scripts/check-cli-docs.mjs`（4 checks, 0 failed）。診断の登録簿は変えていない（既存の kind だけを使う、D2）。
- 生成コードは変えていない（検査だけ）ので、WASM と性能の確認はしていない。

### チケットから外れた・細かくした判断

1. **直接書いたラムダも loan を見る。** チケットは「ラムダ式の引数は二重に報告しない」としていたが、所有のラムダが借用を持つ関数値を捕捉する形（`Owned.function (\() -> g ())`）は型の検査を通るので、同じ解放後の読み出しになる。
   ラムダ式の引数も loan を見て、message を `an owned function captures only owned values; a captured value holds a borrow` にした。参照の捕捉は型の検査（`src/closures.rs`）が先に報告して借用検査まで来ないので、同じ位置に 2 つの診断は出ない（`tsuzuri check --json` で 1 件を確認）。
2. **`Owned.function` を関数値にすることを拒否し、パイプラインは直接の呼び出しと同じに扱う。** 関数値にした `Owned.function` は生成した包み関数を通り、その引数は検査できない（`make g` の結果の loan は `Routine.go` で消える）。`Mutex.with_lock` と同じ `must_apply_directly` にした。
   `g |> Owned.function` は、`std`・`examples`・`tests/fixtures` に使う所がなく、D-59（パイプラインがどのモジュールでも働く）に従って受理し、同じ検査をする。
3. **`own Owned.Function` の引数は loan を持たない。** 検査を足すと、`std/ErrGroup.tz` の `start`（所有のラムダが `own Owned.Function` の引数 `f` を捕捉する）が拒否された。引数の値には呼び出し側の loan が付くためで（チケットの落とし穴の 1 つ目）、`tzroutine { Owned.call (ref f) () }` も以前から `task_capture_retains_borrow` で拒否されていた。
   `Owned.Function` は `Owned.function` でしか作れず（表現は不透明）、それが借用を拒否するようになったので、`check_body` はこの型の引数に loan を与えない。結果、`own Owned.Function<…>` の引数を所有のラムダや `tzroutine { }` で捕捉できるようになった（`owned.md` に例）。`ref Owned.Function<…>` は今までどおり借用。
4. help は `src/diagnostic_kinds.rs` ではなく診断を作る所に書いた（登録簿は kind・code・版だけを持ち、help の欄がない）。
5. `src/explain/` の kind ごとのページは足していない（ページを持つ kind は `tests/explain.rs` の `CURATED` に固定）。`E1013.md` の「What it covers」と「Fixes」に足した。

### 更新した文書

`_tsuzuri/language-reference/built-in-types-and-modules/owned.md`（API の表、インラインのラムダ、「捕捉できるのは所有値だけ」に関数値・引数・`own Owned.Function` の例と実行結果、まとめ）、`routine.md`・`async-tasks-and-lazy/tzroutine.md`（`Routine.go` の節に 1 文）、`src/explain/codes/E1013.md`、`docs/language.md`（`Owned.function` の説明と `E1013` の表）、`docs/architecture.md`（`Owned.Function` の段落）、`_features/README.md`。

### 変更したファイル

`src/ownership.rs`, `src/explain/codes/E1013.md`, `tests/tzroutine.rs`, `tests/timers.rs`, `tests/user_drop.rs`, `tests/tzroutine.mjs`, `_tsuzuri/language-reference/built-in-types-and-modules/owned.md`, `_tsuzuri/language-reference/built-in-types-and-modules/routine.md`, `_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`, `docs/language.md`, `docs/architecture.md`, `_features/README.md`, このチケット。

### レビューの指摘への対応（2026-11-06）

独立したレビューの 3 件の指摘に対応した。どれも、まず指摘の evidence を修正前の dev バイナリで再現し、回帰テストを書いてから直した。期待値は手で求めた値と、`main` のバイナリが受理して正しく実行するプログラムの結果から取った。

1. **`Owned.call` の結果から作る `Owned.function` が拒否される（誤検出）。** `let f = Owned.call (ref factory) ()` の `f` が `factory` の loan を持ち、`Owned.function f` が `E1013`（`this function value holds a borrow`、note は `ref factory`）だった。`main` は受理し、`tsuzuri run` は `hello` を出力する。
   - 修正: `eval_call` で、組み込み関数 `Owned.call` を最後まで適用した呼び出し（引数 2 つ以上）は、第 1 引数（`ref Owned.Function`）の loan を結果に入れない。第 2 引数の loan は今までどおり結果に入る。パイプライン `x |> Owned.call (ref f)` も同じにした（`E::Binary` で、右辺が引数 1 つの `Owned.call` の呼び出しなら callee の loan を足さない）。`Owned.call (ref f)` だけの部分適用は `f` を借用したまま。
   - 根拠と、壊す試み: `Owned.Function` は借用を持たない（A30）。その本体が環境を借用する結果を返せるかを、修正後のバイナリで次の形を試した。捕捉への参照を返す（`ref text`・`[ref text]`・`Maybe.Some (ref text)`・`(ref text, 1)`・`ref job.name`・`if … then s else ref text`・`id_ref (ref text)`・部分適用 `keep (ref text)`）、捕捉の参照を捕捉したクロージャを返す、捕捉した所有関数値への参照を返す（`ref inner`）はどれも `E1013`（`cannot return a reference to a local value`）。捕捉を捕捉するクロージャを返す形（`\() -> String.length (ref text)`）と、入れ子の `Owned.function` は `E1012`（`cannot move 'text' out of an owned function`）。`Copy<'t>` のジェネリックな `mk x = Owned.function (\() -> x)` を `'t = ref string` で使う形は、実体の検査で A30 の `E1013`。結果が環境を借用できる形は見つからなかったので、どの形にも loan を残していない。捕捉の値をコピーしたクロージャを返す形（`let n = String.length (ref text); \() -> n`）は新しく受理され、`Owned.drop` の後に呼んで `32` を出力する（`main` は `E1014` の誤検出）。
   - テスト: `tests/tzroutine.rs` の `the_result_of_owned_call_borrows_only_its_input`（指摘のプログラムとパイプラインの形を受理、コピーを捕捉したクロージャを受理、第 2 引数の借用が結果に残ることを `E1013` で、部分適用が `f` を借用することを `E1013` で、本体が環境の借用を返す 6 形が `E1012`/`E1013` であることを確かめる）。修正を外すと失敗することを確かめた（`git stash` で `src/ownership.rs` を戻して実行）。`tests/tzroutine.mjs` に指摘のプログラムとパイプラインの形を足し、`hello` が 2 行出ることを native の `-O0`・`-O3`（1 P と 4 P）で確かめる。
   - 修正後の evidence: 指摘のプログラムは `tsuzuri check` で診断なし、`tsuzuri run` は `hello`。
2. **直接書いたラムダが借用を持つ関数値を捕捉する場合の拒否が文書にない。** `owned.md` の「捕捉できるのは所有値だけ」に、`Owned.function (\n -> size n)`（`size` が `ref text` を捕捉）の例と message `a captured value holds a borrow`・note を足した（修正後のバイナリの出力を確認。`main` は受理する）。「Owned.call による借用呼び出し」に結果の借用の説明と `run=32` の例、まとめに 2 行を足した。`docs/language.md` の `E1013` の行と `Owned.function` の項、`Owned.call` の表、`docs/architecture.md` の `Owned.Function` の段落、`src/explain/codes/E1013.md` の Fixes（`from a lambda or function value whose captures hold no borrow`。1414 バイトで上限 1500 の内）を直した。
3. **ジェネリックな本体の実体で、直接書いたラムダの message が `this function value holds a borrow` になる（nit）。** 実体の検査の時点でラムダは `E::Closure` に持ち上がっている。`owned_function_holds_borrow` は、`E::Lambda` と、関数の `owned_captures` が真の `E::Closure`（`Owned.function` に直接書いたラムダだけがそうなる。`let rec` の関数は偽）をラムダとして扱う。テスト `an_owned_lambda_in_a_generic_instance_reports_its_capture`（`(Send<'t>, Copy<'t>) => … Routine.go (Owned.function (\() -> Channel.send ch x))` を `'t = unit -> unit` で使う形が `a captured value holds a borrow`。`let rec` の関数を渡す形は `this function value holds a borrow`）は修正前に失敗し、修正後に成功する。指摘の program の message も `a captured value holds a borrow` になった。

検証:

- T0: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。
- T1: `cargo nextest run --locked` 2938 件成功（4 skipped）。
- `node tests/tzroutine.mjs target/debug/tsuzuri`: 163 runs passed。`node tests/features.mjs target/debug/tsuzuri` の `tzroutine`（13 件）・`context`（15 件）・`timers`（9 件）成功。
- `tests/fixtures/*/`・`examples/*/`（176）と `tests/conformance` の 251 の `.tz` の `tsuzuri check --json` を `main` のバイナリと比べ、すべて一致。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs owned.md`: 1 page, 7 links, 7 checked examples, 14 native runs（O0/O3）。`cargo test --locked --test explain` 16 件成功。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/gen-docs.mjs --check`（19 blocks, 0 stale）・`TSUZURI_BIN=target/debug/tsuzuri node scripts/check-cli-docs.mjs`（4 checks, 0 failed）・`node scripts/check-guide-lite.mjs`（88 references, 0 failed）。
- 生成コードは変えていない（借用検査だけ）。

判断:

- 結果の loan を外すのは `Owned.call` を組み込み関数として直接呼ぶ（またはパイプラインで最後まで適用する）ときだけ。`let c = Owned.call` のように関数値にしてから呼ぶ形は今までどおり第 1 引数の loan を結果に入れる（保守的な誤検出が残るだけで、健全性は変わらない）。
- 第 1 引数が呼び出し側の `ref Owned.Function` の引数でも同じ。結果の型が参照を含み、loan がなければ、今までどおり静的なデータへの loan になる。
