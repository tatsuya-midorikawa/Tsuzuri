# TZ15: ランタイムの IR の断片を改行で終え、次の断片が閉じ括弧の行から始まらないようにする（`}define ...`）

| 項目 | 内容 |
| --- | --- |
| ID | TZ15 |
| 優先度 | P3 |
| 規模 | S |
| 依存 | TZ02, TZ10, TZ14 |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-06（TZ14 の作業中に発見。TZ14 のチケットの「レビューの指摘への対応」の「残したこと」の続き。`main` の `72f06862` と TZ14 のブランチの `2306260c` で確認） |
| 承認 | 不要（言語の意味は変えない。生成 IR の行の分け方と、それで正しく付くようになる属性・検査だけを変える） |
| 改善する劣位 | 潜在的な不具合: ランタイムの IR の断片のうち 4 個が改行で終わらないので、次の断片の最初の行がその `}` と同じ行に続く（`}define ...`・`}; ...`・`}@...`）。IR を行ごとに読む処理は、その行の関数を関数として見ない。今起きているのは、spawn するプログラムでその関数に `"probe-stack"` が付かないことと、その関数を呼ぶループに要らない横取りの検査が入ることである |
| 手本にする既存実装 | `src/llvm.rs` の `emit_program` で `CPU_KERNELS` の宣言の前に置かれた `if !output.ends_with('\n') { output.push('\n'); }`（「the runtime text above may end without a newline」）、`tests/tzroutine.rs` の `probes_the_stack_only_when_the_program_spawns`、`src/llvm_preempt.rs` の TZ14 の単体テスト `runtime_pieces_after_a_closing_brace_are_functions_of_their_own` |
| 主な影響ファイル | `src/runtime/character.ll`, `src/runtime/display.ll`, `src/runtime/debug.ll`, `src/runtime/recursive.ll`, `src/llvm.rs`（単体テスト）, `tests/tzroutine.rs`, `docs/architecture.md`, `_features/README.md` |

## 目的

ランタイムの IR の断片をつないだ IR で、どの関数の見出しも `define ` で始まる自分の行にあり、どの `}` も自分の行で関数を閉じるようにする。
生成 IR を行ごとに読む処理（TZ02 の `with_stack_probes`、TZ10 の `with_preemption_checks`、TZ14 の `with_thread_storage_noinline` など）は、`define ` で始まる行を関数の見出し、`}` だけの行を関数の終わりとみなす。
断片が改行で終わらないと、次の断片の最初の行が前の関数の `}` の後に続く。すると、これらの処理はその行の関数を見落とすか、前の関数の一部として読む。

## 着手条件と停止条件

### 着手条件

- TZ14 が `_features/README.md` で done であること（TZ14 の `runtime_definitions` の行の分け方とその単体テストは、このチケットの変更の後も同じ結果を出すはずの比較の相手になる）。
- GUIDE §2.3 の基準コマンドが成功していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- 手順 3 の IR の比較で、「仕様」に挙げた 4 種類のほかの違いが出る（横取りの検査が増える、`noinline` の付き方が変わる、など）。
- 断片の文字列を加工する処理（`src/llvm_strings.rs` の `StringForm::display_runtime`・`character_runtime` など）か、その単体テストが、末尾に改行が無いことに依っていて、意味を保ったまま直す方法が 1 つに決まらない。
- native と WASM、`-O0` と `-O3` で実行結果が違う。

## 現状（`main` の `72f06862` と TZ14 のブランチの `2306260c` で確認）

- `src/runtime/*.ll` の 38 個のうち、`character.ll`・`display.ll`・`debug.ll`・`recursive.ll` の 4 個は最後の `}` の後に改行が無い（`for f in src/runtime/*.ll; do [ -n "$(tail -c1 "$f")" ] && echo "$f"; done`）。
- `src/llvm.rs` の `emit_program` は、断片を `RuntimeSymbols::append`（`src/runtime_cache.rs`。`output.push_str(source)`）か `output.push_str(include_str!(...))` でそのままつなぐ。
  改行を足すのは、`CPU_KERNELS` の宣言の前と、`Mutex`・`Channel` の宣言の前の 2 か所だけである（どちらも「the runtime text above may end without a newline」）。
- `tests/fixtures` と `examples` の下で `Main.tz` を持つ 174 の project（`find tests/fixtures examples -maxdepth 3 -name Main.tz`）を `build --emit llvm --no-cache -O0` で出すと（既定の確保器と `--allocator small` で同じ）、26 個の IR に `}` の後に断片が続く行がある（26 個で合わせて 30 行）。前の関数と、続く行:

  | 前の関数（断片） | 続く行（断片） | 行の数 |
  | --- | --- | --- |
  | `@tz.display.quote`（`display.ll`）、`@tz.debug.write`（`debug.ll`）、`@tz.rec.clone`（`recursive.ll`） | `}; PM08: integers as text ...`（`integer.ll` の先頭の注釈） | 6・6・5 |
  | `@tz.rec.clone`（`recursive.ll`）、`@tz.debug.write`（`debug.ll`） | `}define internal %tz.closure @tz.closure.clone(...)`（`closure.ll`） | 4・1 |
  | `@tz.character.utf8`（`character.ll`） | `}define internal void @tz.string.copy(...)`（`string.ll`） | 4 |
  | `@tz.rec.clone`（`recursive.ll`） | `}define internal %tz.string @tz.display.join(...)`（`display.ll`） | 2 |
  | `@tz.rec.clone`（`recursive.ll`） | `}@tz.debug.newline = ...`（`debug.ll` の大域変数） | 1 |
  | `@tz.display.quote`（`display.ll`） | `}; Padding helpers of interpolated strings ...`（`format.ll` の先頭の注釈） | 1 |

- `src/llvm.rs` の `with_stack_probes`（TZ02）は、spawn するか `Parallel.*` を呼ぶネイティブのプログラム（`emit_program` の `spawning`）と spawn する WebAssembly のプログラムで動き、`define ` で始まり `{` で終わる行だけを見出しとみて `"probe-stack"="inline-asm"` を足す（Windows では足さない）。
  `}define ...` の行の関数には付かない。`docs/architecture.md` の TZ02 の段落は「`with_stack_probes` がすべての `define` に `"probe-stack"="inline-asm"` を付け」と書いている。
- `src/llvm_preempt.rs` の `definitions`（TZ10 の `with_preemption_checks` が使う）は、`define ` で始まる行を見出し、`}` だけの行を終わりとみる。`}define ...` の行の関数と、`}; ...` の注釈の後の最初の関数は前の関数の範囲に入り、関数として数えられない。
  そのため、その関数の呼び出しは宣言された関数の呼び出し（`Callee::Heavy`）に見える。また、その関数の本体の行は前の関数の行として読まれる（`thread_bound` の `@llvm.threadlocal.address` の検索も含む。今の IR では、TLS を読む関数はこの継ぎ目の後に無い）。
  TZ14 の `runtime_definitions` は、`}` の後に top-level の形が続く行を分けてから数えるので、この影響を受けない。
- 再現 1（`"probe-stack"` が付かない）: spawn し `Char` を表示する次のプログラムを、`build --emit llvm --no-cache`（`-O0`・`-O3`）で出した。

  ```tsuzuri
  def go :: i64 -> i64
  fn go n =
      tzroutine { () }
      n
  do! IO.write_line (to_string 'x')
  go 41
  ```

  1338 行目が `}define internal void @tz.string.copy(ptr %to, ptr %from, i64 %length) nounwind {` で、`"probe-stack"` が無い（`main` と `2306260c` で同じ）。`define ` で始まる 113 行は、すべて `"probe-stack"="inline-asm"` を持つ。
  `'x'` を `42` に替えたプログラムは `character.ll` を含まないので、同じ関数は `define internal void @tz.string.copy(...) nounwind "probe-stack"="inline-asm" {` と自分の行にある。実行結果は `x` と `41`。
- 再現 2（要らない横取りの検査）: 次のプログラムでは、`@tz.fn.Main.count` のループの後ろの辺の前に `call void @tz.preempt.poll()` が 1 つ入る（`-O0`・`-O3`。`main` と `2306260c` で同じ）。

  ```tsuzuri
  def go :: i64 -> i64
  fn go n =
      tzroutine { () }
      n
  def count :: i64 -> i64
  fn count n =
      let mut total = 0
      for _ in 0 ..< n do
          total = total + String.length (to_string 'x')
      total
  go (count 1000)
  ```

  ループから届く関数のうち、宣言された関数に見えるのは `}define` の後の `@tz.string.copy` だけで、それが重い呼び出しに数えられる。
  `to_string i`（`i` は整数）を足し、`Debug.print` で `}; PM08: ...` の継ぎ目を作った版でも、同じく検査が 1 つ入る（見落とすのは `integer.ll` の最初の関数）。
- 試作（commit していない）: 4 個の `.ll` の末尾に改行を足したコンパイラで比べた（手順 3 と同じ比較）。
  - 再現 1: 1338 行目が `}` と `define internal void @tz.string.copy(...) nounwind "probe-stack"="inline-asm" {` の 2 行に分かれることだけが違う（`define ` で始まる行は 114 行で、すべて `"probe-stack"` を持つ）。
  - 再現 2: `call void @tz.preempt.poll()` と `@tz.preempt.poll` の定義（`POLL_DEFINITION`）が消える。実行結果（`1000`。整数の版は `n` と、手で数えた 10 × 1 + 90 × 2 + 900 × 3 = 2890）は同じ。
  - 174 の project（既定の確保器と `--allocator small`、`-O0`）: 142 個は byte 単位で同じ、6 個は前後で同じ診断で build できない、26 個が違う。そのうち 23 個は行が分かれることだけが違う。
    `rc`・`recursive_types`（`Parallel.*` を呼ぶ）は、それに加えて `@tz.closure.clone` の見出しに `"probe-stack"="inline-asm"` が付く。
    `effects_sweep` には継ぎ目が無いが、空の行が 1 つ増える（`recursive.ll` の後に、改行で始まる確保器の断片 `host_abi::allocator()` が続く）。横取りの検査と `noinline` の付き方は、174 個のどれでも変わらない。

## 仕様

- ランタイムの IR の断片をつないだ IR に、`}` の後に同じ行で何かが続く行が無い（`grep -n '^}.'` が何も出さない）。
- 生成 IR の違いは次の 4 種類だけで、言語の意味・実行結果・診断は変わらない。
  1. `}` の後に続いていた行が、`}` の行と次の行に分かれる。
  2. `with_stack_probes` が動くプログラムで、分かれた行の関数の見出しに `"probe-stack"="inline-asm"` が付く（Windows では付かない）。
  3. spawn するネイティブのプログラムで、見落としていた関数を通してだけ重い呼び出しに数えられていたループの検査（`call void @tz.preempt.poll()`）が消える。検査が 1 つも残らなければ `POLL_DEFINITION` も消える。検査は増えない（見落としていた関数は、重い呼び出しに数えられていた宣言から、定義された関数になるだけである）。
  4. 次の断片が改行で始まるところに空の行が 1 つ増える（D1 の費用）。
- `--allocator small` の `tz_small_realloc` の ` noinline`（TZ10・TZ14）の付き方は変わらない。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/runtime/character.ll`・`display.ll`・`debug.ll`・`recursive.ll` | 末尾 | 改行を 1 つ足す（D1） |
| `src/llvm.rs` | 単体テスト（新規） | `src/runtime/` のすべての `.ll` が改行で終わることを確かめる |
| `tests/tzroutine.rs` | `probes_the_stack_only_when_the_program_spawns` の隣（新規） | 再現 1・2 を固定する（下の「テスト計画」） |
| `docs/architecture.md` | TZ02 の段落（`with_stack_probes`）と、TZ14 の `helpers_may_wait` の段落 | TZ14 の段落の「末尾に改行の無い断片がある」を直し、`runtime_definitions` の分け方は外から来る IR への備えとして残ると書く |

### アルゴリズム

コードの処理は変えない。断片のファイルが改行で終わるので、`push_str` でつないでも次の断片は自分の行から始まる。
新しい単体テストは `env!("CARGO_MANIFEST_DIR")` の `src/runtime` を `std::fs::read_dir` で読み、拡張子が `ll` のファイルがどれも `\n` で終わることを確かめる（ファイルの名前を並べ替えて報告し、結果を決定的にする）。
`DYN_DROP`・`DYN_CLONE`（`src/llvm.rs`）と `host_abi::allocator()` などの文字列の断片は、すでに改行で終わる。

## 実装手順

### 手順 1: 基準の IR を控える

- 変更: なし（`target/scratch/tz15/` に出力するだけ）。
- 内容: 着手時の HEAD のコンパイラで、「現状」の 174 の project の `build --emit llvm --no-cache -O0` を、既定の確保器と `--allocator small` で出して控える。
  `--target wasm32` でも同じく出す（build できないものは診断を控える）。再現 1・2 の IR を `-O0`・`-O3` で控え、`grep -n '^}.'` の行を数える。
- 確認: 「現状」の数（26 個、30 行）と比べ、違えば現在の値を記録して続ける（HEAD で断片が増減していることがある）。

### 手順 2: 失敗するテストを先に足す

- 変更: `src/llvm.rs` の単体テスト、`tests/tzroutine.rs`。
- 内容:
  - 単体テスト `runtime_pieces_end_with_a_newline`: 上の「アルゴリズム」のとおり。
  - 統合テスト `runtime_pieces_start_on_lines_of_their_own`: 再現 1 のプログラムの native の IR（`llvm::emit`、`Entry::Library` と `Entry::Console`）で、(a) `}` で始まり後に何か続く行が無い、
    (b) `define ` を含む行（行の途中も数える）がどれも `define ` で始まり、`cfg!(windows)` でなければ `"probe-stack"="inline-asm"` を持ち、その中に `@tz.string.copy(` の見出しがある、を確かめる。
    再現 2 のプログラムの native の IR に `@tz.preempt.poll` が無いことも確かめる（TZ10 の規則: ループから届く関数は、どれも定義された軽い関数か確保だけ）。
    期待値は IR の文法（`}` の後の top-level の形は前の関数に属さない）と、TZ02・TZ10 の規則から書く。コンパイラの出力を写さない。
- 確認: 単体テストは 4 個の名前を出して失敗し、統合テストは (a)・(b)・再現 2 の 3 つで失敗する。

### 手順 3: 4 個の `.ll` を改行で終える

- 変更: `src/runtime/character.ll`・`display.ll`・`debug.ll`・`recursive.ll`。
- 確認: 手順 2 のテストが成功する。`cargo test --locked --lib preempt`（TZ14 の `runtime_pieces_after_a_closing_brace_are_functions_of_their_own` を含む）、`cargo test --locked --lib llvm_strings`、`cargo test --locked --test tzroutine --test preemption --test allocator` が全件成功する。
  手順 1 の IR と新しい IR を比べ、違いが「仕様」の 4 種類だけであることを確かめる（`}` の後に続く行を 2 行に分けた古い IR と新しい IR の diff を読む）。違いのある project とその種類を、チケットの末尾に表で記録する。

### 手順 4: ドキュメントと最終確認

- `docs/architecture.md` の 2 つの段落を直す（`grep -n "probe-stack\|runtime_definitions" docs/architecture.md`）。
- T2。継ぎ目のあった project の suite を、native と WASM の `-O0`・`-O3` で回す: `node tests/features.mjs target/debug/tsuzuri <suite>`（`chars`・`string_interpolation`・`deriving`・`debug_output`・`recursive_types`・`rc`・`concurrency`）、
  `node tests/compact_strings.mjs`・`node tests/compact_strings_raw.mjs`・`node tests/user_drop.mjs`・`node tests/wasm_threads.mjs`（`concurrency_threads`）を `target/debug/tsuzuri` で。
  `TSUZURI_TEST_ALLOCATOR=small` でも `recursive_types`・`rc` を回す。`sh scripts/check-runtime-includes.sh`（`src/runtime/` を変えたので）。

## テスト計画

### Rust テスト

- `src/llvm.rs`: `runtime_pieces_end_with_a_newline`（新規）。
- `tests/tzroutine.rs`: `runtime_pieces_start_on_lines_of_their_own`（新規。再現 1・2）。
- 既存: `tests/tzroutine.rs` の `probes_the_stack_only_when_the_program_spawns`、`src/llvm_preempt.rs` の TZ14 の単体テスト（作った IR を使うので、変わらず成功するはず）。

### E2E

- 手順 4 の `features.mjs` の suite。

### 既存テストへの影響

- IR を byte 単位で固定しているテストは、試作の比較では見つかっていない（`grep -rn "tz.string.copy\|tz.closure.clone\|PM08" tests` で着手時に確かめる）。見つかれば、その期待値が「仕様」の 4 種類の違いのどれかであることを確かめてから直す。

### 性能

- 測らない。消えるのは要らない横取りの検査で、付くのは `"probe-stack"` の属性だけである（LLVM は、frame が probe の大きさ（既定で 4096 byte）より小さい関数には probe の命令を出さない）。
  手順 3 で、再現 2 の `-O3` の `count` のループから検査の load と分岐が消えることを `--emit asm` で確かめて記録する。

## ドキュメント

- `_tsuzuri/language-reference/`: 変更不要（利用者から見える振る舞いは変わらない）。
- `docs/architecture.md`: TZ02 の `with_stack_probes` の段落と、TZ14 の `helpers_may_wait` の段落。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] `src/runtime/` のすべての `.ll` が改行で終わり、単体テストがそれを保つ。
- [ ] 再現 1 のプログラムの IR で、`@tz.string.copy` の見出しが自分の行にあり、`"probe-stack"="inline-asm"` を持つ（Windows を除く）。
- [ ] 再現 2 のプログラムの IR に横取りの検査が無い。
- [ ] 手順 3 の IR の比較の違いが「仕様」の 4 種類だけで、チケットに記録されている。
- [ ] T2 が成功する。

## 落とし穴

- `with_preemption_checks` が足す `@tz.preempt.poll` の定義（`POLL_DEFINITION`）は `with_stack_probes` の後に足されるので、`"probe-stack"` を持たない。「すべての `define` が `"probe-stack"` を持つ」を確かめるテストは、検査の入らないプログラムを使うか、この関数を除く。
- Windows では `with_stack_probes` が属性を足さない（`__chkstk`）。テストは `cfg!(windows)` で期待値を分ける（既存の `probes_the_stack_only_when_the_program_spawns` と同じ）。
- `define ` で始まる行だけを見る処理は、`src/llvm_split.rs`・`src/llvm_traps.rs`・`src/llvm_facts.rs`・`src/llvm_cpu.rs`・`src/llvm_abi.rs`・`src/driver.rs` などにもある（`grep -n 'starts_with("define ")' src/*.rs`）。
  この変更で、それらにも見落としていた関数が見えるようになる。`--emit llvm` の出力に現れない処理（object を作るときの分割など）は、手順 1・3 の IR の比較では確かめられないので、手順 4 の E2E で確かめる。
- `.ll` を開いて保存するエディターの設定（末尾の改行を消すもの）で、改行がまた消えることがある。単体テストがそれを捕まえる。

## 対象外

- TZ14 の `runtime_definitions` の行の分け方を消すこと。この変更の後、ランタイムの断片はそれを使わなくなるが、単体テストで固定した保守的な備えとして残す。
- `with_stack_probes` と `definitions` に、TZ14 の行の分け方を入れること（D1 の「採らない案」）。
- 断片をつなぐ所（`RuntimeSymbols::append` と `push_str` の各所）を 1 つの関数にまとめること。

## 決定事項

### D1: 4 個の `.ll` の末尾に改行を足し、すべての `.ll` が改行で終わることをテストで保つ

- 決定: 原因である 4 個のファイルを直し、`src/runtime/` のすべての `.ll` が改行で終わることを単体テストで保つ。断片をつなぐコードは変えない。
- 理由: 原因そのものを直すので、IR を行ごとに読むすべての処理に一度に効く（`define ` で始まる行を見る処理は `src/` に 20 か所以上ある）。新しい断片を足しても、テストが改行の欠けを捕まえる。
- 費用: 次の断片が改行で始まるところに空の行が 1 つ増える（試作の比較では、174 個のうち `effects_sweep` だけ）。意味は変わらない。
- 採らない案:
  - (a) つなぐ側で、出力が改行で終わらないときだけ改行を足す（`RuntimeSymbols::append` と、断片を `push_str` で足す十数か所）。空の行は増えないが、断片を足す所が 2 種類に分かれていて、新しい所を足すと漏れうる。
  - (b) `with_stack_probes` と `definitions` に TZ14 の行の分け方を入れる。この 2 つの処理だけが直り、ほかの行ごとの処理には継ぎ目が残る。
- 状態: 既定案（実装者はこの案に従う）。手順 3 の比較で「仕様」のほかの違いが出たら見直す（停止条件）。
