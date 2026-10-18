# A23 付録: 実測・試作・出力例・fixture

[A23](A23-integer-overflow-intent.md) の本文が参照する長い表と実測をまとめる。測定は 2026-10-18、HEAD `a2313d19`（作業ツリーは同じコードの `9dd0fbbc`。差は `docs/benchmarks.md` だけ）、Apple M1 Max、macOS、Apple clang 21.0.0、Node v20.19.6。
コンパイラのリポジトリでは `cargo build`・`cargo test` を実行せず、複製（`rsync` で `target/` と `.git` を除く）に測定専用の試作を足して `cargo build --release` を 1 回だけ行った。**試作は破棄した**（採用する設計は本文。試作は測定のためだけで、行数と範囲は付録 A.1）。

## 付録 A: 実測

### A.1 試作（測定専用。リポジトリには入れない）

- 新規 `src/llvm_a23.rs`（約 230 行）。`FunctionEmitter` に `a23_binary`・`a23_negate`・`a23_power` を足し、`binary`・`tail_arguments`・`emit_expression_mode` の `Unary` の腕・`binary` の `Power` の前から呼ぶ（変更は合わせて約 30 行）。`emit_program_in` の先頭で状態を消し、末尾で `declare i64 @write(i32, ptr, i64)` を足す。`RangeFacts` の区間の解析は、環境変数があるときだけ `crate::ranges::analyze(.., true)` で有効にする。
- 環境変数 `TZ_A23_STATS=1`: 整数の `+ - * ** neg` を 1 つ出すたびに、`<origin>|<op>|<型>|<proven|unproven>` の件数を数え、`emit_program_in` の終わりに `A23STAT <key> <件数>` として標準エラーへ書く。`proven` は `RangeFacts::integer_flags` が、その型の意味（符号付きは `nsw`、符号なしは `nuw`）で溢れないと示したこと。`origin` は `ModuleOrigin::Std` なら `std`、`reported` の述語（本文の仕様 2）を満たす関数なら `user`、それ以外（`$intrinsic` の `Add.add` の本体、依存パッケージの関数、`reported` を満たさない生成関数）は `other`。
- 環境変数 `TZ_A23=report`: 範囲の証明で溢れないと示せない `user` の演算の直前に、`checked_integer_arithmetic` の溢れの旗を求め、真なら地点ごとの 1 byte の旗を `atomicrmw xchg` し、初めてなら `write(2, ...)` で `tz-a23: integer overflow wrapped in <関数> at <ソース番号>:<開始>-<終了> (<型> <演算>)` を 1 行書く。元の `add`・`sub`・`mul` の命令はそのまま残す。`**` は数えるだけで報告しない。`tail_arguments` の地点は呼び出し全体の span を使ってしまう（本文の設計は引数の span を使う）。
- `TZ_A23_ALL=1`: 全関数を根にして `--emit llvm` する（std の未参照の関数は型検査後に消えているので、足しても std の全関数は出ない。測定には使わなかった）。
- `test_runner.rs` の `execute_test`: 標準エラーに `tz-a23:` があれば、成功したテストを `integer overflow wrapped` で失敗にする（Phase 3 の様子見）。

試作の結果は本文の「現状」と「決定事項」の根拠にだけ使った。**試作の出力形式（`tz-a23:`、バイト範囲）は採用しない**。

### A.2 静的な数: 整数の `+ - *`（と `-x`・`**`）の地点と、範囲の証明

数えたのは、LLVM IR を出すときに 1 回ずつ書かれた演算の数（特殊化された本体と、既知のクロージャを持つ作業関数の本体は、出すたびに数えるので、同じソースの演算が複数回数えられる）。`proven` は、その型の意味で溢れないと示せたもの。実装が数える「一意な地点」とは違う。

| 対象 | プロジェクト数 | 地点（出した数） | 証明できた | 割合 |
| --- | --- | --- | --- | --- |
| `examples/` の利用者のコード | 9（`Main.tz` のある 9 つ。`examples/web/Physics.tz` は浮動小数点だけで 0 件） | 25（`+` 18・`-` 3・`*` 4） | 0 | 0.0% |
| `tests/fixtures/` の利用者のコード（`--emit llvm` で出せた 93 ディレクトリ。出せなかった 7 は `arrays`・`bindgen`・`coverage`・`lists`・`llvm_handoff`・`property`・`storage`） | 93 | 3,317（`+` 2,189・`-` 371・`*` 746・`-x` 9・`**` 2） | 368 | 11.1% |
| 言語リファレンスの例（`^```tsuzuri` の塊を `check-docs.mjs` と同じ規則でプロジェクトにまとめ、`--emit llvm` で出せた 551 個） | 551（561 のうち 10 は出せなかった） | 284（`+` 197・`-` 25・`*` 62） | 15 | 5.3% |
| `std`（上の fixture が到達した std の関数。出した数の重みつき。重複を含む） | 93 | 3,685（`+` 1,885・`-` 1,249・`*` 549・`-x` 2） | 405 | 11.0% |

利用者のコードの演算ごとの内訳（`tests/fixtures/`）:

| 演算 | 地点 | 証明できた | 割合 |
| --- | --- | --- | --- |
| `+` | 2,189 | 226 | 10.3% |
| `-` | 371 | 113 | 30.5% |
| `*` | 746 | 28 | 3.8% |
| `-x`（符号付き） | 9 | 1 | 11.1% |
| `**` | 2 | 0 | 0.0% |

読み取り:

- 利用者のコードの 89〜100% の演算は、HEAD の範囲の証明では溢れないと示せない。演算子ごとの静的な警告は、演算の 89〜100% に出る。**静的な lint を採らない根拠**（決定事項 D2）。
- 証明できるのは、リテラルの演算、`&&& 255` や `% 10` で値域が決まるもの、`if a < 1000 && a > 0 then a + 1` のような条件で絞れたもの、拡張してから掛けるもの、不変の `let` の値だけ。可変の累積（`s = s + i`）、再帰の引数（`n - 1`、`total + x`）、関数の引数どうしの演算は、証明できない。
- 同じ性質を `examples/` の 9 プロジェクトで確かめた最小の例（`target/release/tsuzuri` の `TZ_A23_STATS` を足した試作）: `(a &&& 255) + 1`・`100 + 200`・`if a < 1000 && a > 0 then a + 1 else 0`・`a % 10 + 5`・`(a as i64) * 1000`・`(a as i32) + 1`（`a: i8u`）は `proven`、`for i in 0 .. 9 do s = s + i` の `s + i` は `unproven`。

### A.3 動的な数: 実行して折り返したもの

試作の報告（`TZ_A23=report`）で、既存の利用者のコードを実行した。

| 対象 | 実行した数 | 折り返した | 内訳 |
| --- | --- | --- | --- |
| 言語リファレンスの `run=` つきの例 | 561 プロジェクトのうち `run=` のあるもの | 2 | `matrix-view.md` の 3 つめの例と `matrix.md` の 7 つめの例。どちらも `Matrix.fold (\total x -> total * 31 + x)` の**意図した**ハッシュ混ぜ |
| 言語リファレンスの `test` を含む例（10 プロジェクト） | 10 | 1 | `built-in-types-and-modules/gen.md` の `project=props`。`Gen.for_all (Gen.pair (Gen.i64()) (Gen.i64())) (\pair -> ... a + b == b + a)` が、乱数の `i64` で溢れる |
| `tests/fixtures/coverage`・`property`・`wasm_memory`（`tsuzuri test`） | 3 プロジェクト | `property` の 1 テスト | `Props pair commutes`（`Gen.pair (Gen.i32()) (Gen.i32())` の `a + b == b + a`）が、基準では成功、報告つきでは失敗。`property` の他のテストは基準でも失敗する（反例の報告を確かめるための意図した失敗）。`coverage` は基準の 15 成功 1 失敗のまま、`wasm_memory` は 2 成功のまま |

`tsuzuri test` の既定を有効にする Phase 3 で、このとおりに変わるものが「既存テストへの影響」（付録 C）。

### A.4 計装の費用（1 つの workload、1 台。速度の主張ではない）

`def hot :: i64 -> i64 = \n -> let mut s = 0; for i in 0 .. n do s = s + (i ^^^ (i >>> 3)) * 3 - (i &&& 7); s` を `hot 1000000000` で呼ぶ（地点は `+`・`*`・`-` の 3 つ。どれも証明できない）。`-O3` の native。3 回ずつ。出力は同じ（`1513211202258126784`）。

| ビルド | user 秒（3 回） |
| --- | --- |
| 既定 | 0.53・0.56・0.55 |
| `TZ_A23=report`（試作） | 1.00・1.04・1.03 |

約 1.9 倍。既定のビルドはベクトル化されるが、計装つきはされない。**診断用のモードであり、配布ビルドには使わない**（文書に書く）。性能の主張はしない（共有 CI に速度の閾値を置かない）。

### A.5 WASM: import を増やさずに地点を読めること（手書きの IR で確認）

計装と同じ形（`llvm.sadd.with.overflow.i32` の旗で分岐し、地点ごとの 1 byte に `store atomic i8 1, ... monotonic`）を手で書き、`clang -x ir --target=wasm32-unknown-unknown -mbulk-memory`、`wasm-ld --no-entry --stack-first --export=...` でリンクして Node で実行した。

- 1 つの関数 `tsuzuri_overflow_hit(i32) -> i32`（`load atomic i8 ... monotonic` を `zext`）と `tsuzuri_overflow_count() -> i32` を export した。`WebAssembly.Module.imports(module)` は `[]`。`memory` は `wasm-ld` の既定で export される（ポインターを読ませないので使わない）。
- `add(1, 2)` では `hits = [0,0,0]`、`add(2147483647, 1)` は `-2147483648` を返し `hits = [0,1,0]`。
- `i128` の `sadd.with.overflow`・`usub.with.overflow` と `i64` の `smul.with.overflow` は、`src/runtime/wasm.ll`（`__multi3` を含む `i128` の libcall の定義。WASM の出力には常に連結される）を足せば、`-O0`・`-O3` でリンクでき、import は `[]`、旗は `[1, 1, 1, 0]`（3 地点とも記録）。`wasm.ll` なしでは `undefined symbol: __multi3` でリンクに失敗する（既存の `Int.checked_mul` の 64 bit も同じ）。**計装は `src/runtime/wasm.ll` を連結する既存の経路の上でだけ使う**。
- `atomicrmw` は WASM（`atomics` なし）では libcall になりうるので、WASM の地点では `store atomic ... monotonic` だけを使う（`xchg` は native だけ）。

### A.6 結果が変わらないこと

`tests/fixtures/overflow_report` 相当のプログラム（本付録 E）と、`i32u`・`i8`・`i128` の折り返しを含む 9 値のプログラムを、既定のビルドと計装つきのビルドで `-O0`・`-O3` の native に実行した。出力はビット単位で同じ（試作。報告の行を除く）。

## 付録 B: 整数の演算を書く箇所の全一覧（HEAD）

利用者が書いた整数の算術から、LLVM の `add`・`sub`・`mul` を出す箇所は次の 4 つだけ。内部の増分（ループの添字）や std・ランタイムの手書き IR は含めない。

| # | ファイル・関数 | 出すもの | Phase 2 の扱い |
| --- | --- | --- | --- |
| 1 | `src/llvm.rs` の `FunctionEmitter::binary`（`words` を決める `arithmetic_flags` の直前） | `x + y`・`x - y`・`x * y`。`Add.add x y`・`Sub.sub x y`・`Mul.mul x y` の直接の呼び出し（PR02 D10。`call` が `binary` を呼ぶ）も、この関数を通る | 計装する |
| 2 | `src/llvm.rs` の `FunctionEmitter::tail_arguments`（native だけ。WASM は `self.expression` を直接呼ぶので #1 を通る） | 末尾呼び出しの引数の `n - 1`・`acc + x`。ラッチへ遅らせる `add`・`sub` の直前に、オペランドを写した時点で旗を求める | 計装する（旗を求める位置と同じ。引数の span を使う） |
| 3 | `src/llvm_exception.rs` の `FunctionEmitter::power`（`checked == false`。`binary` の `Power` から呼ぶ。`@checked` の `**` は `checked == true`） | `x ** y` の二乗法の `mul` | 計装する（地点は `**` の式 1 つ。ループの中の `mul` はどれも同じ地点に報告する） |
| 4 | `src/llvm.rs` の `FunctionEmitter::emit_expression_mode` の `TypedExprKind::Unary` の腕（`UnaryOp::Negate`、整数） | `-x` = `sub iN 0, x` | 計装する（符号付きだけ。符号なしの `-x` は型検査が `E1005` で拒否する） |

計装しないもの: `@checked` の中（`FunctionEmitter::checked`。型検査が各演算を `TypedExprKind::Checked` に包む。`src/exceptions.rs` の `spec_expression`・`checked_kind`）、`Int.*` の組み込み関数の本体（`integer_builtin`）、`for` の範囲ループの内部の増分（`audit_increment` の対象）、SIMD のレーン演算（`simd_binary`）、浮動小数点・decimal の演算。上の 4 箇所以外で `src/llvm*.rs` が出す `add`・`sub`・`mul`（`grep -c 'format!("\(add\|sub\|mul\) ' src/llvm*.rs` が 0 でないファイルのうち）は、`i64` の長さ・添字・ポインターの計算と、`src/llvm_bulk.rs`・`llvm_parallel.rs`・`llvm_display.rs`・`llvm_io.rs`・`llvm_shared.rs`・`llvm_sync.rs`・`llvm_hash.rs`・`llvm_frame.rs`・`llvm_abi.rs`・`llvm_control.rs`・`llvm_imports.rs`・`llvm_simd.rs` のランタイムの手書き IR で、利用者の算術ではないので計装しない。

## 付録 C: 既存テスト・文書への影響

### C.1 Phase 1（`Int.wrapping_*`）

| 対象 | 変わること |
| --- | --- |
| `tests/integer_intrinsics.mjs` | `operations` に 4 つ、`reference` に 4 つの腕を足す（期待値は BigInt）。既存の期待値は変わらない |
| `Builtin::ALL` を数えるテスト | HEAD に件数を固定するテストはない（`grep -rn "Builtin::ALL" tests src` で確かめる）。あれば 4 を足す |
| 既存のプログラムの IR | 変わらない（`Int.wrapping_*` を呼ばないプログラムは同じ。手順 1 の基準と 1 byte も違わない） |

### C.2 Phase 2（`--overflow-report`）

既存の期待値が変わるものは**ない**。フラグを付けないビルドの IR・object・`.wasm` は 1 byte も変わらない（手順 1 で保存した基準とのバイト比較を、手順ごとの確認にする）。

### C.3 Phase 3（`tsuzuri test` の既定で報告を有効にし、折り返したテストを失敗にする）

| 対象 | 変わること | 直し方 |
| --- | --- | --- |
| `tests/fixtures/property/Props.tz` の `test "pair commutes"` | 成功 → 失敗（`Gen.pair (Gen.i32()) (Gen.i32())` の `a + b == b + a` が溢れる） | 乱数の `i32` の可換性の意図を保つため、`Int.wrapping_add a b == Int.wrapping_add b a` に書き換える。`tests/property.rs` の期待（`pair commutes` が成功すること）を使うテストは、書き換えた後で緑になる。**期待値は変えない** |
| `_tsuzuri/language-reference/built-in-types-and-modules/gen.md` の `project=props` の `addition commutes` | 成功 → 失敗 | 同じく `Int.wrapping_add a b == Int.wrapping_add b a` に直し、本文に「乱数の整数どうしの足し算は溢れるので、折り返すことを `Int.wrapping_add` で書く」を 1 文足す。`node scripts/check-docs.mjs` を通す |
| `_tsuzuri/language-reference/built-in-types-and-modules/matrix-view.md` と `matrix.md` の `run=` つきの例 | `test` ではなく `run` で実行されるので変わらない。ただし `\total x -> total * 31 + x` は意図した折り返しなので、`Int.wrapping_add (Int.wrapping_mul total 31) x` に書き換える（手順 17。決定事項 D13） | 書き換えても `run=` の期待値は同じ |
| 上に挙げた以外の `tsuzuri test` を使うテスト | 実行して調べる（手順 17）。`grep -rln "tsuzuri.*test\|run_tests" tests` で一覧を取り、`cargo test --locked` の失敗から、折り返しで失敗したものを列挙する | テストの**ソース**の算術を `Int.wrapping_*` に直す。期待値や報告の機構を変えて通さない |
| A21 が済んでいる木の `tests/fixtures/contracts/Main.tz` の `wrap_add`（`ls tests/fixtures/contracts` で確かめる） | `tsuzuri test --contracts test`・`--overflow-report` で走らせると、`@ensures \r -> r == a + b` と本体 `a + b`（`i8u`）の折り返しが報告される（`tests/features.mjs` の `contracts` は `build` で走るので今は変わらない） | 意図した折り返しなので、`@ensures \r -> r == Int.wrapping_add a b` と本体 `Int.wrapping_add a b` に直す。`features.mjs` の期待値 44 は変わらない。fixture に `--allow-overflow` を付けて通さない。この 1 件は停止条件 9 の「5 件」に数えない |

測定した範囲（`tests/fixtures/coverage`・`property`・`wasm_memory` と、言語リファレンスの `test` を含む 10 プロジェクト）では、成功から失敗に変わるのは上の 2 つだけだった。ほかの Rust のテストが埋め込むソース（`tests/test_runner.rs`・`tests/coverage.rs`・`tests/property.rs`）は、複製に試作を入れても `cargo test` を実行できなかったので測っていない。手順 17 で測る。

## 付録 D: 出力の例（Phase 2 と 3 の実装後の想定出力）

以降、実装前の出力は「（想定出力）」と書く。値は付録 E の fixture を手計算（Python と JavaScript の BigInt）で確かめた値と、試作の動きから作った。

### D.1 native の `tsuzuri run`（想定出力）

付録 E の fixture に `def main` を足したものではなく、C のホストが各 export を呼ぶ形（付録 E.3）。ホストの標準出力は値、標準エラーは報告。ディレクトリ `.` で `tsuzuri build . --overflow-report --emit llvm -O0 -o out.ll` した IR を `clang host.c out.ll` でリンクして実行した標準エラー（想定出力。パスはビルドに渡した入力のとおり）:

```text
warning: integer overflow wrapped at ./Main.tz:1:49 (i32 add)
warning: integer overflow wrapped at ./Main.tz:2:53 (i32u sub)
warning: integer overflow wrapped at ./Main.tz:3:45 (i8 mul)
warning: integer overflow wrapped at ./Main.tz:4:40 (i64 neg)
warning: integer overflow wrapped at ./Main.tz:5:49 (i16 pow)
warning: integer overflow wrapped at ./Main.tz:6:36 (i128 add)
warning: integer overflow wrapped at ./Main.tz:8:101 (i32 add)
```

7 行。`./Main.tz:8:93`（`n - 1`）は、`tail32` の引数が `3` と `2` で `n` が 1 まで下がるだけなので報告されない。`./Main.tz:10`（`masked` の `+ 1`）は、範囲の証明で溢れないと示せるので、そもそも計装されない（地点の表に載らない）。

### D.2 `tsuzuri run --json`（想定出力）

プログラムの標準エラーのうち、`warning: integer overflow wrapped at ` で始まる行は、プログラムが終わったあとに 1 行ずつ次の JSON に変えて、`tsuzuri` の標準エラーへ書く（ほかの行はそのまま）。

```json
{"severity":"warning","code":"W2001","message":"integer overflow wrapped at ./Main.tz:1:49 (i32 add)","path":"./Main.tz","span":{"start":48,"end":53,"line":1,"column":49,"end_line":1,"end_column":54}}
```

（`span` の形は `trap.json` の `side_table` と同じ 6 キー。`start`・`end` は UTF-8 のバイトオフセット、`end` は排他。）

### D.3 `tsuzuri test`（想定出力）

既定（折り返したテストは失敗）:

```text
ok 1 - Main adds
not ok 2 - Main hashes
  failure: integer overflow wrapped at ./Main.tz:12:5 (i32 mul)
  warning: integer overflow wrapped at ./Main.tz:12:5 (i32 mul)
  warning: integer overflow wrapped at ./Main.tz:12:13 (i32 add)

1 passed; 1 failed; 0 ignored
./Main.tz:10:6: error[E2006]: 1 test failed
```

`--allow-overflow`（失敗にせず、`ok` の行のあとに報告を出す）:

```text
ok 1 - Main adds
ok 2 - Main hashes
  warning: integer overflow wrapped at ./Main.tz:12:5 (i32 mul)
  warning: integer overflow wrapped at ./Main.tz:12:13 (i32 add)

2 passed; 0 failed; 0 ignored
```

`--json`（1 行ずつ。`overflow` は折り返しを報告したテストだけが持つ。メッセージは `warning: ` なし）:

```json
{"type":"test","index":1,"module":"Main","name":"hashes","status":"failed","failure":"integer overflow wrapped at ./Main.tz:12:5 (i32 mul)","output":"warning: integer overflow wrapped at ./Main.tz:12:5 (i32 mul)\nwarning: integer overflow wrapped at ./Main.tz:12:13 (i32 add)\n","overflow":["integer overflow wrapped at ./Main.tz:12:5 (i32 mul)","integer overflow wrapped at ./Main.tz:12:13 (i32 add)"],"duration_ms":4}
```

### D.4 WASM の `.overflow.json`（想定出力）

`tsuzuri build . --target wasm32 --overflow-report -O3 -o Main.wasm` が `Main.wasm.overflow.json` を書く（`--trap-info` の `Main.wasm.trap.json` と同じ置き場の規則）。付録 E の fixture では 8 地点。地点の番号は出す順なので、テストは `span.line`・`span.column` で引く:

```json
{"version":1,"sites":[{"id":0,"operation":"add","type":"i32","path":"./Main.tz","span":{"start":48,"end":53,"line":1,"column":49,"end_line":1,"end_column":54},"message":"integer overflow wrapped at ./Main.tz:1:49 (i32 add)"},{"id":1,"operation":"sub","type":"i32u","path":"./Main.tz","span":{"start":106,"end":111,"line":2,"column":53,"end_line":2,"end_column":58},"message":"integer overflow wrapped at ./Main.tz:2:53 (i32u sub)"}]}
```

（2 地点だけ示す。実際は `sites` が 8 要素。1 行で出す。キーの順は固定。）

### D.5 IR の抜粋（native の 1 地点。想定出力）

`export def add32 :: i32 -> i32 -> i32 = \a b -> a + b` の `a + b`（`--overflow-report`、`-O0`）。元の `add` の命令はそのまま残り、その直前に検査が入る。

```llvm
  %v8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v6, i32 %v7)
  %v9 = extractvalue { i32, i1 } %v8, 0
  %v10 = extractvalue { i32, i1 } %v8, 1
  %v11 = call i1 @llvm.expect.i1(i1 %v10, i1 false)
  br i1 %v11, label %b2, label %b4
b2:
  %v12 = atomicrmw xchg ptr getelementptr inbounds (i8, ptr @tsuzuri_overflow_hits, i64 0), i8 1 monotonic
  %v13 = icmp eq i8 %v12, 0
  br i1 %v13, label %b3, label %b4
b3:
  call void @tz.overflow.report(ptr @tz.overflow.message.0, i64 62)
  br label %b4
b4:
  %v14 = add i32 %v6, %v7
```

（番号は例。`b2`・`b3`・`b4` と `%vN` は `FunctionEmitter::label`・`value` が決める。`%v9` は使われない。）

## 付録 E: E2E の fixture と期待値

### E.1 `tests/fixtures/overflow_report/Main.tz`（全文。11 行。1 行 1 関数なので、地点は `span.line` で引ける）

```tsuzuri
export def add32 :: i32 -> i32 -> i32 = \a b -> a + b
export def sub32u :: i32u -> i32u -> i32u = \a b -> a - b
export def mul8 :: i8 -> i8 -> i8 = \a b -> a * b
export def neg64 :: i64 -> i64 = \a -> -a
export def pow16 :: i16 -> i16 -> i16 = \a b -> a ** b
def sum128 :: i128 -> i128 = \a -> (a <<< 64) + (a <<< 64)
export def wide64 :: i64 -> i64 = \a -> (sum128 (a as i128)) as i64
def rec accumulate :: i32 -> i32 -> i32 = \n total -> if n == 0 then total else accumulate (n - 1) (total + 1000000000)
export def tail32 :: i32 -> i32 = \n -> accumulate n 0
export def masked :: i32 -> i32 = \a -> (a &&& 255) + 1
export def saturated :: i32 -> i32 -> i32 = \a b -> Int.saturating_add a b
```

HEAD で `tsuzuri check` が通り、`--target wasm32` の `-O0`・`-O3` で import が 0 であることと、次の表の値を確かめた（Node の BigInt の参照と一致）。

### E.2 入力と期待値（独立した参照: JavaScript の BigInt）

| export | 入力 | 期待値 | 折り返す | 報告される地点（行:列 型 演算） |
| --- | --- | --- | --- | --- |
| `add32` | `2147483647, 1` | `-2147483648` | はい | `1:49 i32 add` |
| `add32` | `1, 2` | `3` | いいえ | なし |
| `sub32u` | `0, 1` | `4294967295` | はい | `2:53 i32u sub` |
| `sub32u` | `5, 3` | `2` | いいえ | なし |
| `mul8` | `100, 100` | `16` | はい | `3:45 i8 mul` |
| `mul8` | `3, 4` | `12` | いいえ | なし |
| `neg64` | `-9223372036854775808` | `-9223372036854775808` | はい | `4:40 i64 neg` |
| `neg64` | `5` | `-5` | いいえ | なし |
| `pow16` | `200, 2` | `-25536` | はい | `5:49 i16 pow` |
| `pow16` | `3, 2` | `9` | いいえ | なし |
| `wide64` | `4611686018427387904` | `0` | はい（`i128` の和が `2^127`） | `6:36 i128 add` |
| `wide64` | `1` | `0` | いいえ（和は `2^65`） | なし |
| `tail32` | `3` | `-1294967296` | はい（3 回目の足し算） | `8:101 i32 add` |
| `tail32` | `2` | `2000000000` | いいえ | なし |
| `masked` | `1000` | `233` | いいえ | 地点ではない（範囲の証明で溢れない） |
| `saturated` | `2147483647, 1` | `2147483647` | いいえ（飽和） | 地点ではない |

`tail32` の `n - 1`（`8:93 i32 sub`）は地点だが、入力が 3 と 2 なので折り返さない。地点は全部で 8（`tsuzuri_overflow_count() == 8`）: 1:49・2:53・3:45・4:40・5:49・6:36・8:93・8:101。`sub32u` は JavaScript から見ると符号つきの `i32` なので、期待値は `>>> 0` で比べる。

### E.3 native のホスト（`tests/fixtures/overflow_report/host.c`）

各 export を、折り返す入力で 1 回ずつ、折り返さない入力で 1 回ずつ、表の順に呼び、結果を標準出力へ `printf` する（標準エラーは触らない）。`tests/integer_intrinsics.mjs` と同じく、`tsuzuri build <fixture> --emit llvm [--overflow-report] -O<n> -o x.ll` の IR を `clang -O<n> -Wno-override-module host.c x.ll` でリンクして実行する。

期待する標準出力（計装の有無によらず同じ）:

```text
-2147483648 3
4294967295 2
16 12
-9223372036854775808 -5
-25536 9
0 0
-1294967296 2000000000
233 2147483647
```

期待する標準エラー（`--overflow-report`、`-O0` と `-O3`）は付録 D.1 の 7 行（パスはビルドに渡したパス）。`--overflow-report` なしでは標準エラーは空。

## 付録 F: 言語リファレンスに足す例（`node scripts/check-docs.mjs` を通る形）

`int.md` の「検査、飽和、累乗、拡大乗算」の表に `Int.wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg` の行を足し、次の例を「在庫を型の端で留める」の例のあとに足す（値は手計算。`Int.wrapping_add 127i8 1i8` は `-128i8`、`Int.wrapping_sub 0i8u 1i8u` は `255i8u`、`Int.wrapping_mul 100i8 100i8` は 10000 mod 256 = 16、`Int.wrapping_neg (-128i8)` は `-128i8`）:

```tsuzuri run=-128:255:16:-128
let a = Int.wrapping_add 127i8 1i8
let b = Int.wrapping_sub 0i8u 1i8u
let c = Int.wrapping_mul 100i8 100i8
let d = Int.wrapping_neg (-128i8)
assert (a == -128i8)
$"{a as i64}:{b as i64}:{c as i64}:{d as i64}"
```

実行結果:

```text
-128:255:16:-128
```

（この例は Phase 1 の手順 4 で、実装後の `tsuzuri run` の出力と一致することを確かめてから足す。出力が違うときは、ここの手計算ではなく実装の誤りを疑う。）

## 付録 G: 設計の骨格（Rust。名前と型はこのとおり）

### G.1 `src/overflow.rs`

```rust
use std::cell::RefCell;
use std::rc::Rc;

use crate::diagnostic::{json_string, location, Span};

pub const MAX_SITES: usize = 65_536;
pub const MAX_PRINTED: u32 = 100;
pub const REPORT_PREFIX: &str = "warning: integer overflow wrapped at ";
pub const STOPPED_LINE: &str = "warning: integer overflow reports stop after 100 sites";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operation { Add, Subtract, Multiply, Negate, Power }

impl Operation {
    /// `add`・`sub`・`mul`・`neg`・`pow`.
    pub fn name(self) -> &'static str { /* match */ }
}

/// One arithmetic site of the report. The derived order makes the table deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site { pub source: usize, pub start: usize, pub end: usize,
                  pub operation: Operation, pub bits: u16, pub signed: bool }

impl Site {
    pub fn type_name(&self) -> String { /* "i32", "i8u" */ }
    pub fn span(&self) -> Span { Span::new(self.start, self.end).in_source(self.source) }
    /// `integer overflow wrapped at ./Main.tz:12:5 (i32 add)`.
    pub fn message(&self, source: &Source) -> String { /* location(&source.text, self.start) */ }
}

/// A source file of the program, by the index that `Span::source` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source { pub path: String, pub text: String,
                    /// Whether the report covers the arithmetic of this file: the root package's own sources.
                    pub reported: bool }

pub(crate) struct ScopeData { pub sources: Vec<Source>, sites: RefCell<Vec<Site>> }

impl ScopeData {
    pub fn reports(&self, source: Option<usize>) -> bool { /* sources.get(source?)?.reported */ }
    pub fn publish(&self, sites: Vec<Site>) { *self.sites.borrow_mut() = sites; }
}

thread_local! { static ACTIVE: RefCell<Option<Rc<ScopeData>>> = const { RefCell::new(None) }; }

/// While it is alive, native and WASM IR emission on this thread instruments the arithmetic that
/// the report covers (A23). Dropping it restores the previous scope.
#[must_use = "the report applies only while the scope is alive"]
pub struct Scope { data: Rc<ScopeData>, previous: Option<Rc<ScopeData>> }

impl Scope {
    pub fn enter(sources: Vec<Source>) -> Self;     // ACTIVE.replace(Some(data.clone()))
    /// The sites of the most recent emission, in number order.
    pub fn sites(&self) -> Vec<Site>;
    pub fn sources(&self) -> &[Source];
}
impl Drop for Scope { /* ACTIVE.set(self.previous.take()) */ }

pub(crate) fn active() -> Option<Rc<ScopeData>>;

/// `{"version":1,"sites":[{"id":0,"operation":"add","type":"i32","path":"./Main.tz","span":{...},"message":"..."}]}` + `\n`.
pub fn side_table(sites: &[Site], sources: &[Source]) -> String;

/// The message lines of a program's standard error, without the prefix `warning: `.
pub fn report_lines(stderr: &str) -> Vec<&str>;      // lines that start with REPORT_PREFIX
```

`side_table` の JSON のキーの順: `version`・`sites`。要素: `id`・`operation`・`type`・`path`・`span`（`start`・`end`・`line`・`column`・`end_line`・`end_column`）・`message`。1 行で出し、末尾に `\n`。文字列は `diagnostic::json_string`。`span` の `line`・`column` は開始、`end_line`・`end_column` は `location(text, end)`（`trap::side_table` と同じ）。

### G.2 `Globals` と emission の接続（`src/llvm.rs`）

1. `struct Globals` に `overflow: Option<overflow_emit::Report>` を足し、`Default` で `None`。
2. `emit_program_in` の `Globals { ... }` の組み立てに `overflow: crate::overflow::active().map(overflow_emit::Report::new),` を足す（`range_flags,` の隣）。
3. `FunctionEmitter::emit` の `crate::ranges::analyze(self.module, self.function, self.globals.range_flags != RangeFlags::Off)` を `self.globals.range_flags != RangeFlags::Off || self.globals.overflow.is_some()` にする（区間の解析を計装のために有効にする）。
4. `struct FunctionEmitter` に `overflow_counted: bool` を足し、`FunctionEmitter::new` で `globals.overflow.as_ref().is_some_and(|report| report.covers(function))`（`coverage` の `counted` と同じ形）で決める。`covers` は仕様 2 の述語 `reported`。
5. `emit_program_in` の `if let Some(plan) = instrumentation.coverage { ... }` のブロックの**直後**（`for global in globals.definitions` の前）に、`if let Some(report) = &globals.overflow { output.push_str(&report.runtime(wasm)?); report.publish(); }` を足す。`runtime(wasm)` は付録 D.5 の IR（地点の数 `N` が確定した最後に出す）を返す。`N > MAX_SITES` なら `E1017` を返す。
6. `emit_program_in` の最後の `unique_declarations(output)` の前に、native で計装があるなら `output.push_str("declare i64 @write(i32, ptr, i64)\n")`（`unique_declarations` が重複を落とす。綴りは `src/llvm_traps.rs` と一字一句同じ）。

### G.3 `src/llvm_overflow.rs`（型。`overflow_check` の手順は本文の設計）

```rust
use super::*;
use crate::overflow::{self, Operation, ScopeData, Site};

pub(super) struct Report {
    scope: Rc<ScopeData>,
    index: BTreeMap<Site, usize>,
    order: Vec<Site>,
    limit: usize,                 // overflow::MAX_SITES（単体テストは小さい値）
    exceeded: bool,
}
impl Report {
    pub(super) fn new(scope: Rc<ScopeData>) -> Self;
    pub(super) fn covers(&self, function: &CheckedFunction) -> bool;     // 仕様 2 の reported(function)
    /// The number of `site`, a new one if the emission has not met it. `None` past the limit.
    fn number(&mut self, site: Site) -> Option<usize>;
    pub(super) fn runtime(&self, wasm: bool) -> Result<String, Diagnostic>;  // 付録 D.5
    pub(super) fn publish(&self);                                        // scope.publish(self.order.clone())
}

impl FunctionEmitter<'_, '_> {
    /// Writes the check of one integer operation before the operation itself (A23).
    /// `proven`: the range proof shows that it cannot overflow in the sense of its type.
    pub(super) fn overflow_check(&mut self, operation: Operation, ty: &Type, span: Span,
                                 proven: bool, left: &str, right: &str);
}
```

`src/llvm.rs` に `#[path = "llvm_overflow.rs"] mod overflow_emit;` を足す（`use super::*;`）。

### G.4 `RangeFacts::negation_flags`（`src/ranges_intervals.rs`。証明の規則は足さず、既存の `compute` の `Unary Negate` と同じ式を公開する）

```rust
/// The overflow flags of `-operand`: none unless it is an integer of at most 64 bits.
pub(crate) fn negation_flags(&mut self, module: &CheckedModule, operand: &TypedExpr) -> Flags {
    let Some(ty) = integer(&operand.ty) else { return Flags::default() };
    let value = self.value(module, operand, ty, 0);
    interval::arithmetic(Operation::Subtract, ty, Interval::point(0), value).flags
}
```

### G.5 実行時の IR（`Report::runtime(wasm)`。付録 D.5 の形）

- 共通: `@tsuzuri_overflow_hits = internal global [N x i8] zeroinitializer, align 1`（`N` は地点の数。0 でもよい）。
- **WASM**: `define i32 @tsuzuri_overflow_count() nounwind { ret i32 N }` と、`define i32 @tsuzuri_overflow_hit(i32 %index) nounwind`（`%index` が `N` 未満なら `getelementptr inbounds i8, ptr @tsuzuri_overflow_hits, i32 %index` から `load atomic i8 ... monotonic, align 1` を `zext i32` して返す。そうでなければ 0）。
- **native**: 地点ごとに `@tz.overflow.message.I = private unnamed_addr constant [L x i8] c"..."`（`\XX` の 16 進エスケープ。`@tz.trap.message.N` と同じ書き方）、`@tz.overflow.reported = internal global i32 0`、`@tz.overflow.stopped = private unnamed_addr constant [..] c"warning: integer overflow reports stop after 100 sites\0A"`、`define internal void @tz.overflow.report(ptr %message, i64 %length) cold nounwind`（`%n = atomicrmw add ptr @tz.overflow.reported, i32 1 monotonic`。`%n < 100` なら `@tz.overflow.write(%message, %length)`。`%n == 100` なら `@tz.overflow.write(@tz.overflow.stopped, len)`。それ以外は何もしない）、`define internal void @tz.overflow.write(ptr %data, i64 %length) cold nounwind`（`@tz.trap.write` と同じループ: `write(2, ...)` を、書ききるまで繰り返す）。native の `tsuzuri_overflow_*` の export は作らない（決定事項 D4）。
- どの関数も `nounwind`。IR の名前は `@tz.overflow.*`（内部）と `@tsuzuri_overflow_*`（WASM の export）。

### G.6 driver（`src/driver.rs`）の変更 1〜7

1. `BuildOptions` に `pub overflow_report: bool`（既定 `false`）を足す。`BuildOptions { .. }` を全 field で書く箇所は `src/cache.rs`・`src/repl.rs`・`src/driver.rs` の単体テスト（`grep -n "BuildOptions {" src tests` で一覧。大半は `..BuildOptions::default()` か `..options`）。足りないと言われた箇所に `overflow_report: false` を足す。`repl` は `false`。
2. `BuildOptions::validate` に、本文の仕様 5 の CLI の表の `E2000` を足す（`--overflow-report` と `Emit::Header`・カーネル・バインディング、`--freestanding`）。
3. `Project::overflow_sources(&self) -> Vec<overflow::Source>`（`pub`）: `self.sources` の各ファイルについて `Source { path: source.path.to_string_lossy().into_owned(), text: source.text.clone(), reported: source.origin == ModuleOrigin::User && self.dependency_package(source).is_none() }`。順序と添字は `self.sources` と同じ（`Span::source` の添字）。
4. `build_complete`: `PlatformScope::enter(platform)` の直後に `let overflow = options.overflow_report.then(|| overflow::Scope::enter(project.overflow_sources()));`。IR を出す `if/else if` の連鎖が終わった直後に `let overflow_sites = overflow.as_ref().map(overflow::Scope::sites).unwrap_or_default();`。戻り値を `Result<(Vec<String>, Vec<TrapSite>, Vec<overflow::Site>), Diagnostic>` に広げる（呼び出し元は `build_linked` と `run_process` の 2 つ）。
5. WASM の sidecar: `pub fn overflow_sidecar_path(output: &Path) -> PathBuf`（`trap_sidecar_path` の隣。`.overflow.json` を足す）。`options.overflow_report && options.target.is_wasm()` のとき、`trap` の sidecar と同じ扱い（`staged` を一時ディレクトリへ書き、`cache_paths` に `"overflow"` を足し、`protect_sources`・`protect_links`・`publish_outputs` の `sidecars` に足す）。内容は `overflow::side_table(&overflow_sites, &project.overflow_sources())`。
6. WASM のリンク: `if options.trap_info { linker.arg("--export=tsuzuri_trap_site"); }` の隣に `if options.overflow_report { linker.args(["--export=tsuzuri_overflow_count", "--export=tsuzuri_overflow_hit"]); }`。
7. `run_process`: `build_complete` の戻り値から `overflow_sites` を受け、`json` かつ成功のとき `overflow::json_lines(&stderr, &overflow_sites, &project.overflow_sources())`（新規の関数。`src/driver.rs` の中でよい）で標準エラーを変換して書く。変換の規則は本文の仕様 4。

## 付録 H: Rust テストの一覧（名前・入力・期待値）

`tests/integer_wrapping.rs` は LLVM 不要（`tests/integers.rs` の `integer_builtin_signatures_and_constraints` の形: 受理は `analyze(source).unwrap()`、拒否は `analyze(source).expect_err(source)` の `error.code` を照合）。`tests/overflow_report.rs` の補助関数 `emit(source, wasm, reported)` は、`overflow::Scope::enter` で `Main.tz` を報告する 1 ソースとして、`analyze` → `llvm::emit_target(&module, Entry::Library, wasm)` を呼ぶ（std のソースは scope に入れない）。

| # | ファイル::関数 | 入力 | 期待値 |
| --- | --- | --- | --- |
| H1 | `tests/integer_wrapping.rs::wrapping_functions_exist_for_every_width_and_sign` | 10 型 × 4 関数 | すべて受理（`Int.wrapping_neg` は符号付き 5 型） |
| H2 | `tests/integer_wrapping.rs::wrapping_neg_rejects_unsigned_integers` | `Int.wrapping_neg 5i32u` | `E1005` |
| H3 | `tests/integer_wrapping.rs::wrapping_functions_reject_mixed_float_and_wrong_arity_arguments` | `Int.wrapping_add 5i32 6i8`・`Int.wrapping_add 5.0 6.0`・`Int.wrapping_add 5i32` | `E1003`・`E1005`・`E1003` |
| H4 | `tests/integer_wrapping.rs::wrapping_calls_are_inline_instructions` | `export def f :: i32 -> i32 -> i32 = \a b -> Int.wrapping_add a b`（`sub`・`mul`・`neg` も同様）。関数値は `let g = Int.wrapping_add 1i32` | `@tz.fn.Main.f` の本体に `add i32 `、`@tz.builtin.Int.wrapping_add` の呼び出しなし、` nsw`・` nuw` なし。関数値の IR には `define internal i32 @tz.builtin.Int.wrapping_add.i32` がある |
| H5 | `tests/overflow_report.rs::unproven_arithmetic_is_instrumented_and_proven_arithmetic_is_not` | `add32`（`a + b`）と `masked`（`(a &&& 255) + 1`） | IR に `llvm.sadd.with.overflow.i32` が 1 回宣言、`@tsuzuri_overflow_hits = internal global [1 x i8]`、scope の地点が 1 つ |
| H6 | `…::each_operation_width_and_sign_uses_the_intrinsic_of_its_sense` | `i8..i64`・`i8u..i64u` × `add`・`sub`・`mul`、`i128`・`i128u` の `add`・`sub`・`mul`、符号付きの `neg`、`**` | `llvm.{s,u}{add,sub,mul}.with.overflow.iN`。`i128`・`i128u` の `add`・`sub` は `llvm.{s,u}{add,sub}.with.overflow.i128`、`i128` の `mul` は intrinsic なしの手書き。`neg` は `llvm.ssub.with.overflow.iN`、`**` は `mul`。`declare` は 1 つずつ |
| H7 | `…::sites_are_shared_by_span_operation_and_type` | 総称関数を `i32` と `i64` で使う。`i32` で 2 回使う | 2 地点（型ごとに 1 地点）。`i32` の 2 回目は同じ番号 |
| H8 | `…::tail_call_arguments_are_sites_at_their_own_span` | 付録 E の `accumulate`（`n - 1` と `total + 1000000000`） | 2 地点（`8:93`・`8:101`）。span はその引数の式 |
| H9 | `…::checked_wrapping_saturating_and_shift_forms_are_not_sites` | `@checked a + b`・`Int.wrapping_add`・`Int.checked_add`・`Int.saturating_add`・`a <<< 1`・`a as i8` | 0 地点、`@tsuzuri_overflow_hits` は `[0 x i8]` |
| H10 | `…::std_dependency_and_generated_code_are_not_reported` | 2 ソース。`reported: false` のソースの関数、`Array.sum` を呼ぶだけの関数、`$intrinsic` の `Add.add` 本体 | 0 地点 |
| H11 | `…::ir_without_a_scope_is_unchanged` | 同じ源をスコープ内で 1 回出して drop したあとに出す | スコープなしの IR と同じ。`tsuzuri_overflow` を含まない |
| H12 | `…::emission_with_a_scope_is_deterministic` | 同じ源を 2 回 | 同じ IR・同じ表 |
| H13 | `…::native_ir_reports_through_write_and_wasm_ir_exports_the_hits` | native と wasm | native: `@tz.overflow.report`・`declare i64 @write(i32, ptr, i64)` が 1 回・`atomicrmw xchg i8`。wasm: `define i32 @tsuzuri_overflow_count()`・`@tsuzuri_overflow_hit`・`store atomic i8 1`・`@write` なし・`atomicrmw` なし |
| H14 | `…::the_message_names_the_line_column_type_and_operation` | `Site::message`。UTF-8 の識別子や日本語のコメントのあとの列 | `integer overflow wrapped at ./Main.tz:1:49 (i32 add)`。列は文字数 |
| H15 | `…::run_json_turns_report_lines_into_warning_objects` | `tsuzuri run --overflow-report --json` | 付録 D.2 の 1 行（`W2001`・`path`・`span`） |
| H16 | `…::run_without_json_relays_the_report_unchanged` | `tsuzuri run --overflow-report` | 標準エラーが生の行のまま |
| H17 | `…::test_fails_a_test_that_wraps_and_names_the_site` | 2 テスト（1 つは `127i8 + 1i8` を関数の引数経由で） | 終了コード 1、`not ok 2 - Main …`、`failure: integer overflow wrapped at ./Main.tz:L:C (i8 add)` |
| H18 | `…::allow_overflow_keeps_the_test_passing_and_reports_it` | 同じ | 終了コード 0、`ok 2`、`  warning: integer overflow wrapped at ...` |
| H19 | `…::library_run_tests_defaults_to_no_report` | `driver::run_tests` と `TestOptions::default()` | 折り返すテストが成功 |
| H20 | `…::a_failing_test_keeps_its_own_failure_reason` | 折り返してから `assert false` | `failure` は `assert` の理由 |
| H21 | `…::property_tests_report_overflow_inside_the_property` | `Gen.for_all (Gen.pair (Gen.i32()) (Gen.i32())) (\p -> match p with \| (a, b) -> a + b == b + a)` | 失敗（`i32 add`）。`Int.wrapping_add` に直すと成功 |
| H22 | `…::intentional_wrapping_with_int_wrapping_passes` | `Int.wrapping_mul total 31` | 成功、`overflow` なし |
| H23 | `…::reports_survive_a_long_standard_error` | 折り返したあと 100 KB の `Debug.print` | 失敗のまま（行が末尾から落ちない） |
| H24 | `…::wasm_tests_report_overflow_like_native_tests` | `--target wasm32` の `tsuzuri test` | native と同じ `failure` |
| H25 | `…::test_command_text_and_json_show_the_report` | CLI（`env!("CARGO_BIN_EXE_tsuzuri")`） | 付録 D.3 の text と JSON（`overflow` キー） |
| H26 | `src/overflow.rs` の単体（`message_uses_character_columns`・`side_table_has_the_fixed_key_order`・`json_lines_matches_messages_and_leaves_other_lines`・`scope_restores_the_previous_scope`） | 手順 6 | 4 件 |
| H27 | `src/llvm_overflow.rs` の単体 `number_stops_at_the_limit` | `Report` の `limit` を 2 にして 3 地点 | 3 つめは `None`、`exceeded` が真、2 回目の同じ地点は同じ番号 |
| H28 | `src/main.rs` の引数のテスト `rejects_ambiguous_or_unused_arguments` | 手順 10・16 | 受理・拒否 |
| H29 | `src/driver.rs` の単体（`validate_rejects_the_overflow_report_with_freestanding_and_header_output`・`overflow_sidecar_path_follows_the_trap_sidecar`） | 手順 9 | 2 件 |

## 付録 I: 決定事項の理由と、却下した案の詳細

本文の「決定事項」の表（決定・却下した案）の補足。D1〜D15 はすべて承認済み。「種」は `ticket-seeds.md` の A23 の節。

- **D1（旗で分岐し、元の演算をそのまま書く）**: 値が計装の有無で変わらないことが構造から言える。末尾呼び出しのラッチ（演算を遅らせる `tail_arguments`）は、オペランドを写した時点で検査し、遅らせた演算に触れずに済む。却下した `with.overflow` の値を結果に使う案は、フラグなしの出力との差が大きい。
- **D2（静的な lint を作らない）**: 利用者のコードの演算の 89〜100% は証明できず、演算の 89〜100% が警告になって大量の注釈を生む。範囲の証明を強くして減らすのは、証明の規則を変える別の仕事（PR02 の範囲）。動的な報告は、実際に折り返した演算だけを示す。両端が必ず溢れる式だけの診断は、新しい診断コードが要り、`const` の畳み込みとの整合も要る（本文の対象外）。
- **D3（地点の表は emission が作る）**: `coverage::plan` のように型付き IR を先に走査する計画は作らない。証明の可否は emission の途中の状態（`RangeFacts` の `bind`・`enter_condition`）にしか無く、事前の走査では「どれが計装されるか」を決められない。emission が作る表は、定義によって emission と一致する。事前に全候補を数える案は、旗の配列が大きくなり、計装された地点の表と一致しない。
- **D4（報告の経路）**: native はどの終わり方（`exit`・トラップ）でも途中までの報告が残る（`@tz.trap.write` と同じ作法）。WASM は import を足せないので export が唯一の道で、`tsuzuri_trap_site` と同じ作法。関数で 1 地点ずつ読む形は、`memory` の export・ポインターの幅（wasm64 は i64）・バイト列の読み出しの規則を要らなくする。種の `tsuzuri_overflow_sites()`（ポインターを返す 1 つの関数）は、`memory` の export とポインターの幅の扱いがホストに移る。native で終了時に記録を書き出す案は、`atexit` や `llvm.global_dtors` が要り、`--freestanding`・Windows・トラップで落ちる。種からの変更は報告の「逸脱」に書く。
- **D5（1 回・100 地点）**: 熱いループの中の地点が毎回書くと、報告そのものが実行を埋める。環境変数は D-55 に無く、必要になったら足せる。件数を数えて終了時にまとめる案は、トラップで落ちると出ない。
- **D6（報告する関数の範囲）**: 直せるのはルートパッケージのコードだけ。依存の意図した折り返しでテストが失敗しては導入できない。std は意味が契約で定まっている。テストの本体の算術も利用者のコード。
- **D7（`wrapping_shl`・`wrapping_shr` を足さない）**: 4 つは、報告の下で「これは意図した折り返し」と書ける唯一の書き方（演算子は報告され、組み込み関数は報告されない）。`wrapping_shl`・`wrapping_shr` は、`<<<`・`>>>` が量を幅で切る既存の定義と同じ結果で、どの報告のモードでも演算子と違いがなく、D-50 の調査結論(3)（同じ意味に複数の書き方）を増やすだけ。種からの変更は報告の「逸脱」に書く。
- **D8（inline）**: AGENTS.md「同じ意味の組み込み関数と演算子が別の性能経路を選ばない」。`-O0` でも演算子と同じ命令。PR02 の D10 と同じ判断（`Add.add` を inline する）。
- **D9（scope）**: `EmitOptions` の field を足すと 24 のテストが壊れる（D-45 の G15 が `PlatformScope` を選んだ理由）。emission の入口は 6 つあり、scope なら driver の 1 か所で済む。`overflow::Scope` は、driver から emission へ「報告する」ことと、ソースの表（パス・本文・報告するか）と、結果の地点の表を運ぶ。
- **D10（`W2001` の再利用）**: すべての JSON の診断はコードを持つので、`code` のない行は既存の読み手を壊す。実行時の報告はコンパイルの診断ではないので新しいコードを作らない（D-50・D-55）。`W2001` は「ツール警告」で、すでにスパンなしで使われている。種の「コードなし」からの変更は報告の「逸脱」に書く。
- **D11（CLI は既定で報告）**: 意図の表明がない折り返しは潜在バグ。移行は `Int.wrapping_*` に直すだけで、手段が用意されている（Phase 1）。ライブラリ API の既定を変えると、既存の呼び出し側（テスト）が壊れる。警告だけの既定は誰も見ない。完全にオフにする手段は、意図の表明を使わない逃げ道になる。
- **D12（コンパイル時の評価）**: 実行時に演算が残らない。報告するには、新しい診断コードか、評価器（`constants.rs`）の変更が要る。将来のチケットにする。
- **D13（文書の例の書き換え）**: 文書の例は、意図の表明の書き方を示す場所。`run` では報告されないが、`test` に移したときに失敗しないようにしておく。
- **D14（設定を作らない）**: 設定が増えると「報告されない書き方」が増え、意図の表明（`Int.wrapping_*`）の意味が薄れる。D-55 に無い名前を足さない。
- **D15（診断コードも `kind` も作らない）**: 実行時の報告はコンパイルの診断ではない（D10）。`kind` は G23 が登録簿を作るときに決める。
