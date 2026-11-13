# TZ15: ランタイムの IR の断片を改行で終え、次の断片が閉じ括弧の行から始まらないようにする（`}define ...`）

| 項目 | 内容 |
| --- | --- |
| ID | TZ15 |
| 優先度 | P3 |
| 規模 | S |
| 依存 | TZ02, TZ10, TZ14 |
| 後続 | – |
| 状態 | done（2026-11-08。4 個の `.ll` を改行で終え、単体テストがすべての `.ll` の末尾の改行を保つ。IR の違いは「仕様」の 5 種類だけ。5 種類目の wasm32 のスタック検査は第1回のレビューで足した） |
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
- 生成 IR の違いは次の 5 種類だけで、言語の意味・実行結果・診断は変わらない（5 は第1回のレビューで足した）。
  1. `}` の後に続いていた行が、`}` の行と次の行に分かれる。
  2. `with_stack_probes` が動くプログラムで、分かれた行の関数の見出しに `"probe-stack"="inline-asm"` が付く（Windows では付かない）。
  3. spawn するネイティブのプログラムで、見落としていた関数を通してだけ重い呼び出しに数えられていたループの検査（`call void @tz.preempt.poll()`）が消える。検査が 1 つも残らなければ `POLL_DEFINITION` も消える。検査は増えない（見落としていた関数は、重い呼び出しに数えられていた宣言から、定義された関数になるだけである）。
  4. 次の断片が改行で始まるところに空の行が 1 つ増える（D1 の費用）。
  5. wasm32 で `with_stack_checks` が動く build（spawn するモジュール、`--wasm-feature threads`、2 GiB を超える `--wasm-max-memory`。`tsuzuri test --target wasm32` も 2 GiB を超えるとき）で、分かれた行の関数の入口に `call void @tz.stack.check()`（`--trap-info` では `call void @tz.stack.check(i32 %tz.context)`）が 1 つ入る。「全関数のエントリーブロックに検査を入れる」（`docs/architecture.md`）とおりになるだけで、検査が抜けていた関数は線形メモリに frame を作らないか、作っても余白の 4096 バイトに収まる。
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

- [x] `src/runtime/` のすべての `.ll` が改行で終わり、単体テストがそれを保つ。
- [x] 再現 1 のプログラムの IR で、`@tz.string.copy` の見出しが自分の行にあり、`"probe-stack"="inline-asm"` を持つ（Windows を除く）。
- [x] 再現 2 のプログラムの IR に横取りの検査が無い。
- [x] 手順 3 の IR の比較の違いが「仕様」の 5 種類だけで、チケットに記録されている（5 種類目は第1回のレビューで足した）。
- [x] T2 が成功する。

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

## 着手時の再検討（2026-11-08、HEAD b3dd7f1c）

HEAD b3dd7f1c の dev ビルド（`target/scratch/base/tsuzuri`）で「現状」を確かめ直しました。結果は `target/scratch/audit-TZ15/baseline.md` にあります。
この節は、それ以前の節（とくに「実装手順」「テスト計画」「落とし穴」）より優先します。

### 停止条件の扱い

- このチケットの停止条件（GUIDE §13）は、作業を止めるのではなく、「健全さを保ち、今受け入れている安全なプログラムをすべて受け入れ続ける、いちばん保守的な案を採り、その判断をこのチケットに記録して続ける」に読み替えます。
- テストの期待値を弱めて通すことはしません。

### 変わっていないこと

- 改行で終わらない `.ll` は、今も 38 個のうち `character.ll`・`debug.ll`・`display.ll`・`recursive.ll` の 4 個です。
- 再現 1（`-O0`・`-O3`）:
  - 1338 行目が `}define internal void @tz.string.copy(...) nounwind {` です。
  - `define ` で始まる 113 行は、すべて `"probe-stack"` を持ちます。
  - 実行結果は `x`・`41` です。
- 再現 2（`-O0`・`-O3`）:
  - 329 行目に同じ継ぎ目があります。
  - `@tz.fn.Main.count` に検査が 1 つ入ります。
  - 実行結果は `1000` です。
- 整数と `Debug.print` を使う版は、`}; PM08` の継ぎ目で検査が 1 つ入ります（実行結果は `2890`）。
- 主な関数の新しい位置:
  - `src/llvm.rs`: `emit_program` 1449、`with_stack_probes` 1555、断片の連結 2273〜2525。改行を足す 2 か所は 2292〜2300 と 2315〜2319 です。
  - `src/runtime_cache.rs`: `RuntimeSymbols::append` 87。
  - `src/llvm_preempt.rs`: `definitions` 249、`with_preemption_checks` 564、`runtime_definitions` 783、TZ14 の単体テスト 1215。
  - `tests/tzroutine.rs`: `probes_the_stack_only_when_the_program_spawns` 507。
  - `src/llvm_strings.rs`: `display_runtime` 168、`character_runtime` 184、`layered` 414。
- `src/llvm_strings.rs` の加工は、末尾に改行が無いことに依りません。
  - UTF-16 の形は、文字列を借りたまま返します。
  - compact の形は、`layered` が改行の無いときだけ足すので、出力は変わりません。
  - よって停止条件 2 には当たりません。
- IR を byte 単位で固定するテストはありません。`tz.string.copy`・`tz.closure.clone`・`PM08` に一致するのは、合成した IR、注釈、継ぎ目を通らない呼び出しの検査だけです。

### 数の更新

- ネイティブ（174 の project、`-O0`）:
  - 既定の確保器でも `--allocator small` でも、168 個が build できます（6 個は同じ診断で拒否されます）。
  - 継ぎ目のある project は 26 個、行は 31 行です（起票時は 30 行）。
  - 増えた 1 行は、伸びた `generic_records` の fixture によるものです。`@tz.rec.clone` → `}define ... @tz.closure.clone` が 5 個になりました。
- `--target wasm32`:
  - 158 個が build でき、18 個・22 行に継ぎ目があります。
  - 表に無い種類が 1 つあります: `@tz.rec.clone` → `}; Standalone WASM has no host thread imports ...`（`task-wasm.ll` の先頭の注釈。`rc`・`recursive_types`・`user_drop`）。
  - 18 個とも `with_stack_probes` が動かないので、違いは行の分割だけのはずです。
- D1 の空の行は、ネイティブの `effects_sweep` のほかに、wasm32 の `concurrency` にも増えます（`recursive.ll` の後に、改行で始まる `host_abi::allocator()` が続くため）。
- ネイティブで新たに `"probe-stack"` が付く見出しは、`rc`・`recursive_types` の `@tz.closure.clone` だけの見込みです。
- 検査が減りうるのは、spawn する `concurrency` です。今は 2 個で、`}; PM08` の後にある `integer.ll` の最初の関数を見落としています。

- 継ぎ目の種類ごとの数（ネイティブ、既定の確保器と `--allocator small` で同じ）:

  | 前の関数（断片） | 続く行（断片） | project の数 |
  | --- | --- | --- |
  | `@tz.display.quote` | `}; PM08: ...`（`integer.ll` の先頭の注釈） | 6 |
  | `@tz.debug.write` | `}; PM08: ...` | 6 |
  | `@tz.rec.clone` | `}; PM08: ...` | 5 |
  | `@tz.rec.clone` | `}define internal %tz.closure @tz.closure.clone(...)` | 5（`debug_view`・`generic_records`・`last_use`・`rc`・`recursive_types`） |
  | `@tz.character.utf8` | `}define internal void @tz.string.copy(...)` | 4（`chars`・`compact_strings`・`compact_strings_raw`・`string_interpolation`） |
  | `@tz.rec.clone` | `}define internal %tz.string @tz.display.join(...)` | 2（`deriving`・`llvm_handoff/tree`） |
  | `@tz.rec.clone` | `}@tz.debug.newline = ...` | 1（`alloc_contracts`） |
  | `@tz.display.quote` | `}; Padding helpers ...`（`format.ll` の先頭の注釈） | 1（`compact_strings`） |
  | `@tz.debug.write` | `}define internal %tz.closure @tz.closure.clone(...)` | 1（`capabilities/probe`） |

### 計画の変更

1. 既存の回避策は、コードを残して注釈だけ直します。
   - 継ぎ目を前提にしたコードは、`runtime_definitions` のほかにもあります:
     - `src/llvm_traps.rs` の `ir_lines`（380）と `with_frame_pointers`（438 の `}define ` の枝）。
     - `src/driver/lto.rs` の `definition_start`（228）。
     - `src/llvm_split.rs` の `parse_function`（`}` の直後から読み続ける）。
   - 「対象外」の方針に合わせて、これらのコードは残します。
   - 次の注釈は「ランタイムの断片は改行で終わらない」と書いているので直します:
     - `src/llvm_preempt.rs` の `runtime_definitions` と `JOINED_PROGRAM` の doc
     - `src/llvm_traps.rs:376-379`
     - `src/driver/lto.rs:226-227`
     - `src/llvm.rs:2292-2293`・2316
     - `tests/lto.rs:157`
     - `tests/llvm_handoff.rs` の `TREE` の doc と 2 つのテストの注釈
     - `tests/llvm_handoff.mjs:128-130`
2. `tests/llvm_handoff.rs` の `the_trap_instrumentation_reads_runtime_parts_that_share_a_line` は、TZ15 の後は継ぎ目を通りません。そこで、合成した `}define ...` の IR で `ir_lines`・`with_frame_pointers` を確かめる単体テストを `src/llvm_traps.rs` に 1 つ足します。残す備えを、テストで守られた状態に保つためです。
3. `docs/architecture.md` で直す段落を広げます。
   - TZ02（1107 行）と TZ14（1104 行）の段落に加えて、§フロントエンドのトラップの走査の段落（774 行）を直します。
   - PR08 の段落（224 行）の `}define` の記述は、外から来る IR への備えと分かるように書きます。
4. 統合テスト `runtime_pieces_start_on_lines_of_their_own` の形を変えます。
   - 理由:
     - `llvm::emit` は `TSUZURI_STRINGS` を読みます。
     - `targets.yml` の PM03 の job は、Rust のテストをすべて compact の形で回します。
     - compact の形では、再現 1・2 に継ぎ目がありません（`layered` が改行を足すため）。
     - そのため、修正の前でもテストが通ってしまいます。
   - 変更:
     - 再現 1・2 は、`llvm::emit_with_strings(..., StringForm::Utf16)` で形を固定します（`tests/string_interpolation.rs:33` と同じ書き方）。
     - 3 つ目の例として、`Debug.print 1` と `to_string i` のループを足します。どちらの形でも `}; PM08` の継ぎ目が出ます。
     - この例で、(a) と横取りの検査が無いことを、両方の形で確かめます。
     - 「すべての `define` が `"probe-stack"` を持つ」の確認からは、`@tz.preempt.poll` と `@tz.trap.*` を除きます。
5. 確認のコマンドを直します。
   - 手順 3 の `cargo test --locked --lib llvm_strings` は 0 件です（module は `llvm::strings`）。`cargo test --locked --lib llvm::strings::` に替え、`running N tests` を確かめます。
   - 手順 4 に `cargo test --locked --test llvm_handoff --test lto` と、`node tests/llvm_handoff.mjs target/debug/tsuzuri`（`tree`・`tree_channel`）を足します。
6. 手順 1・3 の比較の基準は、HEAD b3dd7f1c の上の数（ネイティブ 26 個・31 行、wasm32 18 個・22 行）です。
   - 手順 1 の IR は、監査が base のコンパイラで出した `target/scratch/audit-TZ15/survey/{default,small,wasm}/` を使います（同じ HEAD、同じコマンド）。
   - 予想される違い: 全体で行の分割（種類 1）。ネイティブの `rc`・`recursive_types` の `@tz.closure.clone` の見出しに `"probe-stack"`（種類 2）。spawn する `concurrency` の検査（今は 2 個）が減りうる（種類 3）。空の行が、ネイティブの `effects_sweep` と wasm32 の `concurrency` に 1 つずつ（種類 4）。
   - それ以外の違いが出たら、上の「停止条件の扱い」に従います。

### 補足

- `--trap-info` の IR:
  - `with_stack_probes` の後に `ir_lines` が行を分け直すので、今でも `^}.` の行はありません。
  - それでも `rc` の `@tz.closure.clone` には `"probe-stack"` がありません。
  - よって、この build でも「仕様」の 2 の違いが出ます。
- `with_generated_section`（`src/llvm_preempt.rs:834`。実行ファイルだけ、`src/driver.rs:4261`）も `definitions` を使います。`src/llvm_cpu.rs` の `@cpu` の本体の取り出し（`}` で始まる行で本体を終える）も、`--emit llvm` に現れない行ごとの処理です。
  - 継ぎ目の後の関数は、ソースの関数を直接呼ばず、TLS も読みません。そのため、置き場所は変わらない見込みです。
  - `--emit llvm` には現れないので、手順 4 の E2E（`tests/preemption.rs`、`concurrency`）で確かめます。
- `with_stack_probes` は、新しく見える関数の入口のブロックにある定数の `alloca` も先頭へ寄せます（Windows でも）。
  - 今回の関数（`@tz.closure.clone`・`@tz.string.copy`・`@tz.display.join`）の入口には `alloca` が無いので、違いは出ません。
- 空の行と `"probe-stack"` で IR の文字列が変わるので、ランタイムと codegen の単位の cache の鍵は一度だけ変わります（害はありません）。
- `runtime_pieces_end_with_a_newline` は `src/runtime` を再帰せずに読み、拡張子 `ll` のファイルだけを見ます（`math-include/`・`musl/` は下位のディレクトリです）。

## 決定事項

### D1: 4 個の `.ll` の末尾に改行を足し、すべての `.ll` が改行で終わることをテストで保つ

- 決定: 原因である 4 個のファイルを直し、`src/runtime/` のすべての `.ll` が改行で終わることを単体テストで保つ。断片をつなぐコードは変えない。
- 理由: 原因そのものを直すので、IR を行ごとに読むすべての処理に一度に効く（`define ` で始まる行を見る処理は `src/` に 20 か所以上ある）。新しい断片を足しても、テストが改行の欠けを捕まえる。
- 費用: 次の断片が改行で始まるところに空の行が 1 つ増える（試作の比較では、174 個のうち `effects_sweep` だけ）。意味は変わらない。
- 採らない案:
  - (a) つなぐ側で、出力が改行で終わらないときだけ改行を足す（`RuntimeSymbols::append` と、断片を `push_str` で足す十数か所）。空の行は増えないが、断片を足す所が 2 種類に分かれていて、新しい所を足すと漏れうる。
  - (b) `with_stack_probes` と `definitions` に TZ14 の行の分け方を入れる。この 2 つの処理だけが直り、ほかの行ごとの処理には継ぎ目が残る。
- 状態: 既定案（実装者はこの案に従う）。手順 3 の比較で「仕様」のほかの違いが出たら見直す（停止条件）。

## 実装と検証（2026-11-08）

ブランチ `fix/tz15-runtime-ir-newline`（`main` の `b3dd7f1c` から）。手順 1〜4 をすべて実装し、「着手時の再検討」の計画の変更をすべて採りました。

### 実装した手順

- 手順 1: 監査が同じ HEAD・同じコマンドで出した IR（`target/scratch/audit-TZ15/survey/{default,small,wasm}/`、`baseline.md`）を基準にしました。数は「着手時の再検討」のとおりです（ネイティブ 26 個・31 行、wasm32 18 個・22 行）。
- 手順 2: 失敗するテストを先に足しました。
  - `src/llvm.rs` の `runtime_pieces_end_with_a_newline`: `src/runtime` を再帰せずに読み、拡張子 `ll` のファイルがどれも `\n` で終わることを確かめます（名前を並べ替えて報告）。修正の前は `["character.ll", "debug.ll", "display.ll", "recursive.ll"]` を出して失敗しました。
  - `tests/tzroutine.rs` の `runtime_pieces_start_on_lines_of_their_own`: 再現 1・2 を `llvm::emit_with_strings(..., StringForm::Utf16)` で、`Entry::Library` と `Entry::Console` の両方で出します。(a) `}` の後に何か続く行が無い、(b) `define ` を含む行がどれも `define ` で始まり、`cfg!(windows)` でなければ `"probe-stack"="inline-asm"` を持つ（`@tz.preempt.poll` と `@tz.trap.*` を除く）、その中に `@tz.string.copy(` の見出しがある、再現 2 の IR に `@tz.preempt.poll` が無い、を確かめます。3 つ目の例（`Debug.print (ref 1)` と `to_string i` のループ）は UTF-16 と compact の両方で (a)・(b) と検査が無いことを確かめます。修正の前は、(a)（`}define internal void @tz.string.copy(...)`）、(b)（同じ行）、再現 2 の検査、3 つ目の例の `}; PM08: ...` と検査で、それぞれ失敗することを確かめました（各確認を一時的に外して 1 つずつ実行。3 つ目の例は先に回る UTF-16 で失敗し、compact でも同じ継ぎ目と検査が出ることは監査の `repro2int.compact.ll` で確かめました）。
  - `src/llvm_traps.rs` の `a_definition_after_a_closing_brace_is_a_line_of_its_own`: 合成した `}define ...`・`}; note` の IR で、`ir_lines` が閉じ括弧と残りを 2 行に分け、`with_frame_pointers` が `}define` の行にも `"frame-pointer"="all"` を入れることを確かめます（残す備えのテスト。修正の前から成功します）。
- 手順 3: `character.ll`・`display.ll`・`debug.ll`・`recursive.ll` の末尾に改行を 1 つ足しました。断片をつなぐコードは変えていません（D1）。
- 手順 4: 「ランタイムの断片は改行で終わらない」と書いていた注釈と文書を直しました（下の「変更したファイル」）。継ぎ目を前提にしたコード（`runtime_definitions`、`ir_lines`、`with_frame_pointers` の `}define ` の枝、`definition_start`、`parse_function`、`src/llvm.rs` の改行の確認 2 か所）は、別の形でつないだ IR への備えとして残しました。

### 手順 3 の IR の比較

新しいコンパイラ（このブランチの dev ビルド）で、監査と同じコマンド（`build --emit llvm --no-cache -O0`。`find tests/fixtures examples -maxdepth 3 -name Main.tz` の 174 個）で出し、古い IR の `}` の後に続く行を 2 行に分けてから `diff` で比べ、違いを「仕様」の 4 種類に分類しました（作業用のスクリプト `target/scratch/tz15/compare.py` で。commit していません）。分類できない違いは、どの構成でも 0 行でした。この 3 つの構成ではスタック検査（`with_stack_checks`）が動かないので、種類 5 は出ません。スタック検査の動く wasm32 の構成は、第1回のレビューで種類 5 を見つけて調べ直しました（下の「レビューの指摘への対応（第1回）」の表）。

| 構成 | byte 単位で同じ | 違う | 拒否（前後で同じ診断） | 種類 1（分かれた行） | 種類 2（`"probe-stack"`） | 種類 3（検査） | 種類 4（空の行） | 新しい IR の `^}.` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 既定の確保器 | 141 | 27 | 6 | 26 個・31 行 | 2（`rc`・`recursive_types` の `@tz.closure.clone`） | 0 | 1（`effects_sweep`） | 0 |
| `--allocator small` | 141 | 27 | 6 | 26 個・31 行 | 2（同じ） | 0 | 1（`effects_sweep`） | 0 |
| `--target wasm32` | 139 | 19 | 16 | 18 個・22 行 | 0 | 0 | 1（`concurrency`） | 0 |

違いのある project と種類（ネイティブは既定の確保器と `--allocator small` で同じ。数は行の数）:

| project | ネイティブ | wasm32 |
| --- | --- | --- |
| `alloc_contracts` | 1: 2 | 1: 1 |
| `capabilities/probe` | 1: 1 | – |
| `cbor` | 1: 1 | 1: 1 |
| `chars` | 1: 1 | 1: 1 |
| `compact_strings` | 1: 2 | 1: 2 |
| `compact_strings_raw` | 1: 2 | 1: 2 |
| `concurrency` | 1: 1 | 4: 1 |
| `concurrency_threads` | 1: 1 | – |
| `debug_output` | 1: 1 | – |
| `debug_view` | 1: 1 | 1: 1 |
| `deriving` | 1: 2 | 1: 2 |
| `effects_sweep` | 4: 1 | – |
| `generic_records` | 1: 1 | 1: 1 |
| `json` | 1: 1 | 1: 1 |
| `kg_basic` | 1: 1 | – |
| `last_use` | 1: 1 | 1: 1 |
| `list_chunks` | 1: 1 | 1: 1 |
| `llvm_handoff/tree` | 1: 2 | 1: 2 |
| `llvm_handoff/tree_channel` | 1: 1 | 1: 1 |
| `parallel_report/basic` | 1: 1 | – |
| `pure_functions` | 1: 1 | – |
| `rc` | 1: 1、2: 1 | 1: 1 |
| `recursive_types` | 1: 1、2: 1 | 1: 1 |
| `string_interpolation` | 1: 1 | 1: 1 |
| `text_class_stages` | 1: 1 | – |
| `user_drop` | 1: 1 | 1: 1 |
| `value_layout` | 1: 1 | 1: 1 |

- 横取りの検査と `noinline` の付き方は、どの project でも変わりませんでした。spawn する `concurrency` の検査は前後とも 2 個です（監査は減りうるとしていましたが、`integer.ll` の最初の関数は検査のあるループの重さを決めていませんでした）。`--allocator small` の `tz_small_realloc` の ` noinline` も同じです。
- 種類 2 の 2 つは `define internal %tz.closure @tz.closure.clone(%tz.closure %value) nounwind "probe-stack"="inline-asm" {` です。`concurrency` の `define ` の 1,140 行のうち `"probe-stack"` が無いのは `@tz.preempt.poll` だけです。
- ほかの構成の抜き取り（`deriving`・`rc`・`compact_strings`・`concurrency`・`alloc_contracts`）: `--allocator host` は種類 1 と、`rc` の種類 2（`@tz.closure.clone`）だけ。`--allocator counting` は種類 1 と種類 4（`alloc_contracts`・`concurrency`・`deriving`・`rc` に 1 行ずつ。`recursive.ll` の後に改行で始まる確保器の断片が続く）だけ。`--trap-info` は前から `^}.` が 0 行で、違いは `rc` の種類 2（`@tz.closure.clone`）だけ。`.trap.json` は 5 個とも byte 単位で同じです。
- 再現の IR（`-O0`・`-O3` で同じ）:
  - 再現 1: 1338 行目が `}` と `define internal void @tz.string.copy(...) nounwind "probe-stack"="inline-asm" {` の 2 行に分かれることだけが違います。`define ` で始まる 114 行は、すべて `"probe-stack"` を持ちます。実行結果は `x`・`41` です。
  - 再現 2: 継ぎ目が分かれ、`@tz.string.copy` に `"probe-stack"` が付き、`call void @tz.preempt.poll()` と `POLL_DEFINITION`（`@tsuzuri_tz_preempt_pending`・`@tsuzuri_tz_preempt_check` の宣言を含む）が消えます。`define ` の 65 行はすべて `"probe-stack"` を持ちます。実行結果は `1000` です。
  - 3 つ目の例（`Debug.print 1` と `to_string i`）: `}; PM08: ...` が分かれ、検査と `POLL_DEFINITION` が消えます。`define ` の 86 行はすべて `"probe-stack"` を持ちます。実行結果は `1`・`2890`（手で数えた 10 × 1 + 90 × 2 + 900 × 3）です。

### 生成コードの確認（性能の節）

`--emit asm` という出力の種類は無いので、`-O3` の実行ファイル（macOS arm64）を `llvm-objdump -d --no-show-raw-insn` で読みました。速さは測っていません。

| プログラム | 基準（`b3dd7f1c`） | このブランチ |
| --- | --- | --- |
| 再現 2 | `count` を inline した `_tz.fn.Main.$entry`（7,004 命令）で、ループは展開され、`ldr w8, [x19, #0x50]`（`tsuzuri_tz_preempt_pending` の load）と `cbnz` の組が 999 個、冷たい `bl _tsuzuri_tz_preempt_check` が 999 個 | 入口は `_main`（36 命令）に inline され、`count 1000` は定数 `1000` に畳まれる。`tsuzuri_tz_preempt_pending` の load も `_tsuzuri_tz_preempt_check` の呼び出しも 0 個。実行ファイルは 106,264 → 73,160 byte |
| 3 つ目の例 | `_main`（13,758 命令）に旗の load が 500 個、`bl _tsuzuri_tz_preempt_check` が 500 個 | 旗の load も呼び出しも 0 個。LLVM は検査の消えたループを別の形に展開し（`_tz.fn.Main.$entry` が 26,201 命令、`@tz.integer.digits` の呼び出しが 502 → 1,001 個）、実行ファイルは 139,304 → 188,824 byte |

どちらも検査の load と分岐がループから消えることを確かめました。3 つ目の例の大きさの増え方は、回数が定数 1000 のループを LLVM が展開した結果で、検査が消えたことで展開の判断が変わったものです。

### 実行したコマンドと結果

- T0: `cargo fmt --all -- --check`（成功）、`cargo clippy --all-targets -- -D warnings`（警告なし）。
- 手順 2 の後（修正の前）の T1: `cargo nextest run --locked --no-fail-fast` で 2,983 件のうち 2,981 件が成功し、失敗は新しい 2 件（`llvm::tests::runtime_pieces_end_with_a_newline`、`tzroutine::runtime_pieces_start_on_lines_of_their_own`）だけでした（4 件 skip）。
- 手順 3 の確認:
  - `cargo test --locked --lib -- runtime_pieces_end_with_a_newline a_definition_after_a_closing_brace_is_a_line_of_its_own preempt llvm::strings::`: `running 19 tests`、19 件成功（`preempt` は 10 件、`llvm::strings::` は 7 件。再検討のとおり `llvm_strings` の絞り込みは 0 件なので使っていません）。
  - `cargo test --locked --test tzroutine --test preemption --test allocator --test llvm_handoff --test lto`: 23・6・10・13・20 件、すべて成功。
  - `TSUZURI_STRINGS=compact cargo test --locked --test tzroutine`（PM03 の job と同じ形）: 23 件すべて成功。
- 最後の T1（手順 4 の後）: `cargo nextest run --locked --no-fail-fast` で 2,983 件すべて成功（4 件 skip）。
- T2（`cargo build --locked` の dev のバイナリ。`features.mjs` は native と WASM の `-O0`・`-O3` を回します）:
  - `node tests/features.mjs target/debug/tsuzuri <suite>`: `chars`（19 件）、`string_interpolation`（82 件）、`deriving`（209 件）、`debug_output`（3 件）、`recursive_types`（23 件）、`rc`（39 件。WASM のスレッドの `Arc` を含む）、`concurrency`（15 件）。どれも native と WASM の `-O0`・`-O3` で成功。
  - `TSUZURI_TEST_ALLOCATOR=small` で `recursive_types`（23 件）・`rc`（39 件）: 成功。
  - `node tests/compact_strings.mjs`（12 の build の行列が一致）、`node tests/compact_strings_raw.mjs`、`node tests/user_drop.mjs`（native と WASM の `-O0`・`-O3`）、`node tests/wasm_threads.mjs`、`node tests/llvm_handoff.mjs`（`tree`・`tree_channel` を含む）: すべて成功（どれも `target/debug/tsuzuri`）。
  - `sh scripts/check-runtime-includes.sh`: 60 個のファイルが追跡されていて成功。
  - `node scripts/check-guide-lite.mjs`（88 件、失敗 0）、`node scripts/check-links.mjs --into _features/_completed/TZ15-runtime-ir-pieces-newline.md --into _features/README.md`（15 件、壊れたもの 0）。
  - `node scripts/check-docs.mjs` は、言語リファレンスのページを変えていないので回していません。診断の登録簿と std のタグも変えていないので、`gen-docs.mjs` も要りません。

### チケットから外れた判断

- 実装の時点では、停止条件に当たる事態は起きませんでした（調べた構成では IR の違いは「仕様」の 4 種類だけ、`llvm_strings` の加工は末尾の改行に依らない、native と WASM・`-O0` と `-O3` の結果は同じ）。第1回のレビューで、調べていなかったスタック検査の動く wasm32 の構成に 5 種類目の違いが見つかりました。停止条件の読み替えに従って、検査が入るほうを採り（契約どおりで、検査を足すほうが保守的です）、「仕様」に足しました。
- 再検討のとおり、統合テストの再現 1・2 は `StringForm::Utf16` に固定し、3 つ目の例を足しました。`analyze` は edition 2026 なので、3 つ目の例は `Debug.print 1` でなく `Debug.print (ref 1)` と書きました（edition 2026 の `Debug.print` は `ref` を取ります。継ぎ目は同じ `debug.ll` → `integer.ll` です）。
- 「すべての `define` が `"probe-stack"` を持つ」の確認は、`@tz.preempt.poll` と `@tz.trap.*` を除きます（再検討の 4）。`define ` を含む行は、行の途中のものも数えます。
- 性能の節の「`--emit asm`」は、その出力の種類が無いので、`-O3` の実行ファイルの逆アセンブルで代えました（上の「生成コードの確認」）。
- 監査が「減りうる」とした `concurrency` の検査は減りませんでした（2 個のまま）。「仕様」の 3 は「消える」ことを許すだけなので、食い違いではありません。
- テストの名前 `the_trap_instrumentation_reads_runtime_parts_that_share_a_line`（`tests/llvm_handoff.rs`）は変えず、注釈で TZ15 の前の状況だと書きました。継ぎ目の処理そのものは、`src/llvm_traps.rs` の新しい単体テストが合成した IR で確かめます。

### 文書

- `docs/architecture.md`: TZ02 の `with_stack_probes` の段落（見出しとみる行と、`"probe-stack"` が付かない関数は probe の後で足す関数だけになったこと。第1回のレビューで、native の `--emit llvm` のほかの出力で後から足す関数も並べました）、TZ14 の `helpers_may_wait` の段落（`runtime_definitions` の分け方は備えとして残す）、§フロントエンドのトラップの走査の段落（TZ15 からは継ぎ目が無く、`ir_lines` は備え）、PR08 の `stamp_target_attributes` と B10 の `with_frame_pointers` の `}define` の記述（備えと分かるように）。
- `_tsuzuri/language-reference/`・`docs/language.md`・`README.md`: 変更不要（利用者から見える振る舞いは変わりません。`grep -rn "TZ15\|probe-stack\|tz.string.copy" _tsuzuri docs/language.md README.md` で、関係する記述が無いことを確かめました）。
- `_features/README.md`: TZ15 の 2 つの行を done にし、`_completed/` へのリンクにしました。`_features/_completed/TZ14-thread-storage-across-parallel-waits.md` の TZ15 へのリンクも直しました。

### 変更したファイル

- `src/runtime/character.ll`・`display.ll`・`debug.ll`・`recursive.ll`（末尾の改行）
- `src/llvm.rs`（単体テスト `runtime_pieces_end_with_a_newline`、改行の確認 2 か所の注釈）
- `src/llvm_traps.rs`（単体テスト `a_definition_after_a_closing_brace_is_a_line_of_its_own`、`ir_lines` の注釈）
- `src/llvm_preempt.rs`（`runtime_definitions` と `JOINED_PROGRAM` の注釈）
- `src/driver/lto.rs`（`definition_start` の注釈）
- `tests/tzroutine.rs`（統合テスト `runtime_pieces_start_on_lines_of_their_own`）
- `tests/lto.rs`・`tests/llvm_handoff.rs`・`tests/llvm_handoff.mjs`（注釈）
- `docs/architecture.md`
- `_features/README.md`、`_features/_completed/TZ14-thread-storage-across-parallel-waits.md`、このチケット（`_features/_completed/` へ移動）

### レビューの指摘への対応（第1回）

6 件の指摘をすべて再現し、どれも実際の問題でした（1 は記録の漏れ、2・4 は文書の言い過ぎ、3・5・6 は古い注釈）。コードの処理は変えていません。

1. **[minor] wasm32 のスタック検査が、`}` の後に続いていた関数にも入る（「仕様」に無い 5 種類目の違い）**: 再現しました。
   - 再現 1 のプログラムを `--target wasm32 --wasm-feature jspi` で出すと、`call void @tz.stack.check` は基準の 104 個からこのブランチの 105 個になります。増えたのは `@tz.string.copy` の入口の 1 つです。
   - `with_stack_checks` は `define ` で始まり `{` で終わる行を見出しとみるので、TZ15 より前は `}define ...` の行の関数に検査が入っていませんでした。
   - 停止条件の読み替えに従って、検査が入るほうを採りました。`docs/architecture.md` の「全関数のエントリーブロックに検査を入れる」とおりになり、検査を足すほうが保守的です。
   - 「仕様」に種類 5 を足し、「状態」・受け入れ条件・`_features/README.md` の 2 つの行・「チケットから外れた判断」を直しました。
   - `docs/architecture.md` のスタック検査の段落に、見出しとみる行と TZ15 の前後の違いを書きました。
   - 前の抜けに害が無かった理由は、`-O0` の wasm32 の object を逆アセンブルして確かめました。`@tz.string.copy`・`@tz.closure.clone` は `__stack_pointer` を動かしません。`@tz.display.join` は 16 バイトの frame を作りますが、呼び出し元の検査が残す 4096 バイトの余白に収まり、そこから呼ぶ関数はそれぞれ検査します。
     - 指摘は「これらの関数は線形メモリに frame を持たない」としていました。`@tz.display.join` には当てはまらないので、文書ではこの理由で書きました。
   - 回帰テスト: `tests/tzroutine.rs` の `wasm_stack_checks_reach_the_function_after_each_runtime_piece`。
     - 2 つの構成で確かめます。spawn するモジュール（`--wasm-feature jspi`、`StackBounds::Switched`）と、`--wasm-max-memory 3GiB`（`StackBounds::Main`）です。
     - CLI の `--emit llvm` に `TSUZURI_STRINGS=utf16` を渡します。compact の形では修正の前から継ぎ目が無いので、PM03 の job でも形を固定するためです。
     - (a) `}` の後に何か続く行が無い、(b) `@tz.string.copy` の見出しが `define ` で始まる、(c) 見出しの次が `entry:` と `  call void @tz.stack.check()` である、を確かめます。spawn するほうでは、見出しが `"probe-stack"="inline-asm"` を持つことも確かめます。
     - 4 個の `.ll` を `b3dd7f1c` の内容に戻すと、このテストは失敗します（下の「実行したコマンド」）。
   - スタック検査の動く構成で、174 個の project を調べ直しました（`build --emit llvm --no-cache -O0`。基準と新しいコンパイラで出し、`target/scratch/tz15/compare2.py` で分類）。分類できない違いは、どの構成でも 0 行でした。

     | 構成 | byte 単位で同じ | 違う | 拒否（前後で同じ診断） | 種類 1（分かれた行） | 種類 4（空の行） | 種類 5（スタック検査） | 新しい IR の `^}.` |
     | --- | --- | --- | --- | --- | --- | --- | --- |
     | `--target wasm32 --wasm-max-memory 3GiB` | 139 | 19 | 16 | 18 個・22 行 | 1（`concurrency`） | 9 | 0 |
     | 同じ構成に `--trap-info` | 148 | 10 | 16 | 0（`ir_lines` が前から分けていた） | 1（`concurrency`） | 9（`@tz.stack.check(i32 %tz.context)`） | 0 |
     | `--target wasm32 --wasm-feature jspi` | 139 | 19 | 16 | 18 個・22 行 | 1（`concurrency`） | 0（wasm32 で build できる fixture は spawn しない） | 0 |

     - 種類 2・3 は、どの構成でも 0 です。
     - 種類 5 が出る 9 個は、`chars`・`compact_strings`・`compact_strings_raw`・`string_interpolation`（`@tz.string.copy`）、`debug_view`・`generic_records`・`last_use`（`@tz.closure.clone`）、`deriving`・`llvm_handoff/tree`（`@tz.display.join`）です。どれも検査が 1 つ増えるだけです。
     - `--wasm-feature threads` は `--emit llvm` を受け付けません。そこで、`--emit object` の object の `@tz.stack.check` への再配置を数えました。`debug_view` は 95 → 96、`deriving` は 675 → 676 です。`chars` は 150 のままで、object も byte 単位で同じでした。
   - 実行結果: `export` した関数（`to_string 'x'` と `to_string [1, 2, 3]` の長さに引数を足す）を `--target wasm32 --wasm-max-memory 3GiB` で build し、Node で呼びました。引数が 5 のとき、基準とこのブランチの `-O0`・`-O3` の 4 つとも 15 でした（手で数えた 1 + 9 + 5）。IR の検査は 76 → 77 個です。
2. **[minor] `docs/architecture.md` の「`"probe-stack"` が付かないのは `@tz.preempt.poll` と `@tz.trap.*` だけ」は、wasm32 では正しくない**: 再現しました。
   - 再現 1 の wasm32 の IR では、`"probe-stack"` の無い見出しが 17 個あります。`@tz.stack.check`、`wasm.ll` の i128 の補助関数、`tzroutine-jspi.ll` の関数（`@tsuzuri_tz_entry` など）です。
   - TZ02 の段落を書き直しました。今の文は次のとおりです。
     - `with_stack_probes` に渡る IR の関数にはすべて付き、付かないのは probe の後で足す関数です。
     - native の `--emit llvm` では、それは `@tz.preempt.poll`・`@tz.trap.*` です。
     - wasm32 の spawn するモジュールでは、`@tz.stack.check`、i128 の補助関数、`tzroutine-jspi.ll` の関数、`--trap-info` の `@tz.trap.report`・`@tsuzuri_trap_site` にも付きません。
     - native の実行ファイル・object・shared・bitcode では、F08 の `@cpu` の振り分けの関数にも付きません。
     - どれも TZ15 の前から同じです。
   - `tests/tzroutine.rs` の `runtime_pieces_start_on_lines_of_their_own` の注釈も、「この native の IR では」と範囲を書きました（テストは native の IR だけを見るので、確認の内容は変えていません）。
3. **[minor] `tests/fixtures/llvm_handoff/tree/Main.tz` の注釈が、`recursive.ll` は改行で終わらないと書いたまま**: 再現しました（このブランチの IR に `^}.` の行は 0 行で、基準には 2 行）。
   - 注釈を「TZ15 より前は……」の形に直し、今はすべての断片が改行で終わると書きました。
   - 注釈の行の数は変えていないので、ソースの位置は変わりません。
4. **[minor] 2 と同じ文が、wasm32 と `@cpu` の振り分けの関数でも正しくない**: 2 と同じ直しで対応しました。
   - `src/llvm_cpu.rs` の振り分けの関数は、`emit_native` の中で `emit_program` の後（`cpu::multiversion`）に足され、見出しに `"probe-stack"` を持ちません。
   - これは native の `--emit llvm` には現れません（`emit_native_build_numbered` を使うのは実行ファイル・object・shared・bitcode だけです）。
5. **[minor] 3 と同じ fixture の注釈と、`tree_channel/Main.tz` の「改行を関数と一緒に失う」**: 再現しました。
   - `tests/fixtures/llvm_handoff/tree_channel/Main.tz` の注釈も「TZ15 より前は……失いました」に直しました。行の数は同じです。
6. **[minor] `tests/llvm_handoff.rs` の `defined` と `context_without_parameter` の注釈が、ランタイムの断片が `}` の後に続くと書いたまま**: 再現しました（`rg` で 2 件）。
   - 2 つとも「TZ15 より前は続くことがありました。今は改行で終わり、分け方は別の形でつないだ IR への備えとして残します」に直しました。コードは変えていません。

実行したコマンド（第1回の対応）:

- 修正の前の確認: 4 個の `.ll` を `b3dd7f1c` の内容に戻して `cargo test --locked --test tzroutine wasm_stack_checks` を回しました。`running 1 test` で失敗し、`}` の後に続く行 `"}define internal void @tz.string.copy(ptr %to, ptr %from, i64 %length) nounwind {"` を報告しました。戻した後は成功します。
- T0: `cargo fmt --all -- --check`（成功）、`cargo clippy --all-targets -- -D warnings`（警告なし）。
- T1: `cargo nextest run --locked --no-fail-fast` で 2,984 件すべて成功しました（新しいテストの 1 件が増えました。4 件 skip）。
- `TSUZURI_STRINGS=compact cargo test --locked --test tzroutine --test llvm_handoff`（PM03 の job と同じ形）: 24 件と 13 件で、すべて成功しました。
- node の suite（`cargo build --locked` の dev のバイナリ）: `node tests/tzroutine_wasm.mjs`（JSPI の tzroutine、`-O0`・`-O3`）、`node tests/wasm_memory.mjs`（2 GiB を超えるメモリとスタック検査、`-O0`・`-O3`）、`node tests/llvm_handoff.mjs`（`tree`・`tree_channel` を含む）、`node tests/wasm_threads.mjs`。すべて成功しました。
- `node scripts/check-guide-lite.mjs`（88 件、失敗 0）、`node scripts/check-links.mjs --into _features/_completed/TZ15-runtime-ir-pieces-newline.md --into _features/README.md`（壊れたもの 0）。
- 言語リファレンスのページ・`docs/language.md`・`README.md` は変えていません（利用者から見える振る舞いは変わりません）。

変更したファイル（第1回の対応）: `tests/tzroutine.rs`（回帰テストと `emitted_ir_with`、native のテストの注釈）、`tests/llvm_handoff.rs`・`tests/fixtures/llvm_handoff/tree/Main.tz`・`tests/fixtures/llvm_handoff/tree_channel/Main.tz`（注釈）、`docs/architecture.md`（TZ02 の段落とスタック検査の段落）、`_features/README.md`、このチケット。
