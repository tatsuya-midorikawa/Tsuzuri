# A30: 借用を持つ関数値から `Owned.function` を作れてしまい、`Routine.go`・`WaitGroup.go`・`Time.after_func` で解放後の読み出しになる

| 項目 | 内容 |
| --- | --- |
| ID | A30 |
| 優先度 | P0 |
| 規模 | S |
| 依存 | TZ02, TZ05, TZ06 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-05（A28 の原因の調査中に発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（健全性の穴を塞ぐ。文書の「捕捉できるのは所有値だけ」の約束どおりに拒否する。今受理している危険なプログラムが `E1013` になる） |
| 改善する劣位 | 不具合（メモリ安全性）: `let g: unit -> unit = \() -> ... borrowed ...`（`borrowed` は局所の `ref`）のように、借用を捕捉した関数値を変数に置いてから `Owned.function g` に渡すと、検査を通る。それを `Routine.go` に渡すと、tzroutine が関数の終わりで解放された文字列を読み、ごみを出力するか `SIGSEGV` で落ちる。ラムダを `Owned.function` に直接書けば正しく `E1013`（`owned_function_captures_reference`）になる |
| 手本にする既存実装 | `src/closures.rs:128〜150`（`Owned.function` に直接書いたラムダの捕捉を型で検査し、`OwnedFunctionCapturesReference` を出す）と、`src/ownership.rs:3129〜3139`（tzroutine の本文 `Type::Once` の捕捉が loan を持てば `TaskCaptureRetainsBorrow`） |
| 主な影響ファイル | `src/ownership.rs`, `src/closures.rs`, `tests/user_drop.rs`, `tests/tzroutine.rs`, `tests/timers.rs`, `_tsuzuri/language-reference/built-in-types-and-modules/owned.md`, `_features/README.md` |

## 目的

`Owned.Function` が「捕捉した値を所有する関数」であるという約束（[Owned](../_tsuzuri/language-reference/built-in-types-and-modules/owned.md) の「捕捉できるのは所有値だけ」）を、
引数がラムダ式でない場合にも守る。`Routine.go` などの、関数を別の tzroutine やタイマーへ渡す組み込み関数は、`Owned.Function` が借用を持たないことを前提にしているので、
約束が破れると、安全なコードだけで解放後の読み出しが起きる。

## 着手条件と停止条件

### 着手条件

- TZ02・TZ05・TZ06 が `_features/README.md` で done であること（HEAD で done）。
- GUIDE §2.3 の基準コマンドが成功し、「再現」を確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `Owned.function` に名前付きの関数（`Owned.function double`。`tests/user_drop.rs:429`）や、所有値だけを捕捉する関数値を渡すプログラムが拒否される。
- `own Owned.Function<…>` の引数をそのまま `Routine.go` に渡すプログラム（[Routine](../_tsuzuri/language-reference/built-in-types-and-modules/routine.md) の `spawn` の例）が拒否される。
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
  - `def spawn :: own Owned.Function<unit, unit> -> unit = \work -> Routine.go work`（[Routine](../_tsuzuri/language-reference/built-in-types-and-modules/routine.md) の例）。

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
