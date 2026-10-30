# G36: 言語全体の差分ファズ（プログラム生成型。コンパイル時評価を参照実装に）

| 項目 | 内容 |
| --- | --- |
| ID | G36 |
| 優先度 | P2 |
| 規模 | L（Phase 1: L、Phase 2: M、Phase 3: M）。Phase は順に積む（Phase 2 は 1、Phase 3 は 2 の上に足す） |
| 依存 | [D11](D11-const-evaluation.md)（done。`const def` とコンパイル時評価 `src/const_machine.rs`）。Phase 3 の native の漏れの検査だけが、任意で [F14](F14-structural-performance-assertions.md)（done、Phase 1・2・3。`Alloc.snapshot`／`Alloc.since`、`run`・実行ファイルでの `--allocator counting`）を使う。F14 が無くても Phase 3 は完成する |
| 後続 | なし。型検査・コード生成・最適化を変える全チケットの回帰検出に使う |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `main` の `9dd0fbbc` で、差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D12 はすべて承認済み |
| 改善する劣位 | 言語全体を横断して意味を機械で突き合わせる道具が無い。既存の差分ファズは A17（最後の使用）・PR02（整数の範囲証明）・B09（ローカルな `let rec`）の局所だけを見る。`const def` の文書は「同じ入力なら、コンパイラが評価した値と実行時の結果は一致します」と約束するが、HEAD の検査は固定の 19 件（`tests/features.mjs` の suite `const_functions`）だけ。型の組み合わせ（record・union・`Maybe`・配列・ループ）を通る整数演算、native と wasm32 と `-O0`／`-O3` の差、所有権つきの値の解放の漏れは、乱数で広く当てる手段が無い |
| 手本にする既存実装 | 差分ファズの形: `tests/last_use_fuzz.mjs`（位置引数・`spawnSync`・被覆の JSON）、`tests/integer_ranges_fuzz.mjs`（`generate`・`evaluate`・`wrap`・`runWasm`・`build`）、`tests/local_recursion_fuzz.mjs`（`worker` の並列・`execute`）。`const def` の規則: `src/const_rules.rs` の `validate_function`・`check_signature`・`value_type`、評価器 `src/const_machine.rs` の `Machine`・`within`・`advance`、`tests/fixtures/const_functions/Main.tz`。wasm32 を Node で動かす: `tests/features.mjs` の `boundedWasmCall`、`tests/allocator.mjs`（`--allocator counting` の `tsuzuri_alloc_stats` を JS から読む）。CI: `.github/workflows/targets.yml` の段「Last-use differential fuzz smoke」「Integer range proofs (audit mode and differential fuzz)」 |
| 主な影響ファイル | （新規）`tests/progfuzz.mjs`。変更: `.github/workflows/targets.yml`、`README.md`、`docs/architecture.md`、`_tsuzuri/language-reference/loops-and-conditionals/for-in.md`（KB1 の既知のバグの注意）。コンパイラのソース・既存のテストは変えない |

付録（`tests/progfuzz.mjs` の検証済みの全文、実測、`const def` の受理の実測、KB1、`.tz` の例）は [G36-appendix.md](G36-appendix.md)。

## 方針の最新化（2026-10-31。実装前の再検討）

起票時（HEAD `a2313d19`）の前提と、A18・A22・A23・F14 の実装後の現状が食い違う点を、実装前と実装中に次のとおり改めた（人間の包括許可による判断。D1〜D12 の趣旨は保つ）。付録 A は起票時の試作の全文のまま残し、出荷した `tests/progfuzz.mjs` との差はこの表だけにある。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| マニフェストのない入力は edition 2026 で読まれ、`for` の素の `..` は両端を含む（付録 A の `render` は `lo .. hi`・`lo .. step .. hi` を書く） | A18・A22 の後、マニフェストのない入力は edition 2027（`LATEST`）で読まれ、`for` の最後の素の `..` は `E0004`。付録 A のままでは、step 5 の表の `for` の 9 行がすべて `E0004` で落ちる（実測） | 生成器は edition 2027 の正準形を書く。範囲の最後の演算子を `..=` にし、30% を半開の `..<` にする（`for` の節点の `op`。評価器は `..<` で終端を除き、変異 `loop-exclusive` は両者を入れ替える。被覆のタグ `loop_half_open` を `REQUIRED_TAGS.values` に足して 33 個）。刻みの印は素の `..` のまま（2027 で正準）。意味の表に `..<` の 10 行（A22 の付録 F の列挙から手で和を求めた）を足して 126 行。マニフェストで 2026 に固定する案は採らない（既定の edition を検査しなくなる）。`..<` の抽選が乱数の列をずらすので、変異の本数・最小化の数は起票時と変わる（「実装と検証」）。列を変える前に、付録 A のままの step 0〜4 が付録 B9 の数（122・65・67・99・164・37・27・16、750 → 20 節点・109 回）を 1 つも違えずに再現することを確かめ、写しが正しいことを確認した |
| 所有権の補助関数の無印の `Vec<i32>`・`string` は値渡し（move） | A18: edition 2027 の `def` の無印の引数のうち、drop が要る型は読み取り（`ref T`）に補われる | 受け取って消費する補助関数（`h_grow`・`h_pop`・`h_drop`・`h_arr`・`h_cat`・`h_take`）の引数を `own T` と書く（全 edition で受理し、2026 の無印と同じ意味）。読み取りの補助関数は従来どおり `ref T`。`const def`・`main` は補完の対象外なので、values の生成器は変えない |
| `Vec.push x v` は Vec を消費して返す（`v = Vec.push x v`） | edition 2027 の組み込み `Vec.push` は対象を排他借用してその場で更新し、`unit` を返す（`BORROWING_APIS`。消費する形は全 edition の `Vec.push_owned`）。付録 A のままでは `E1003 expected Vec<i32>, found unit`（実測） | `run` の本体の `push`・`loop`・`qpush`・`qpushs` は 2027 の文 `Vec.push e v`、`h_grow` は `Vec.push_owned` の入れ子にして、2 つの契約の両方を検査する。`h_sum` の範囲は `0i64 ..< (Vec.length v)`、`loop` は `0i64 ..= N`。描画だけの変更なので owned の乱数の列は変わらない（変異の本数 27・16 は起票時と同じ） |
| KB1 の再現は `0i64 .. 1i64` | KB1 は今も再現する（`..`・`..=`・`..<`・刻みつきのどれでも `E0002`。素の `..` でも `E0004` より先に `E0002`） | `KNOWN_BUGS` の再現を 2027 の `..=` で書く。停止条件 4 には当たらない。for-in.md の注意も `..=` の例で書いた |
| F14 は todo で、native の漏れの検査は無効のまま | F14 は done（`Alloc.snapshot`・`Alloc.since`、`run`・`build` の `--allocator counting`） | 付録 A のままで `supportsAlloc` が真になり、owned の native の経路も `<値> leaked 0` を検査する（停止条件 9 には当たらない）。「Alloc module がない」の 1 行は出ない |
| 最小化の打ち切り（600 秒）は自己検査でも同じ | 共有の機械の負荷が高いとき（load average 約 49、8 コア）、step 6 の最小化が 600 秒で打ち切られ、`shift-saturating` の保存が 76 節点（上限 60）で落ちた。同じ予算（60 回）を打ち切りなしで回すと 50 節点（30 秒） | 打ち切りの秒数を変数 `minimizeSeconds` に出し、自己検査（step 4・6・9）は回数の予算だけで止める（結果が機械の負荷で変わらない）。fuzz の実行は従来どおり 600 秒でも止める |
| A23 の整数の折り返しの報告 | 折り返しで失敗するのは `tsuzuri test` だけ（`--overflow-report` は opt-in）。`build`・`run`・定数評価は報告しない（A23 D12） | 影響なし。progfuzz は `build` して実行ファイルを直接動かす |
| `targets.yml` は 4 つのホスト | 3 つのホスト（darwin-arm64・linux-x64・linux-arm64） | 同じ YAML の段を 3 ホストに入れた |
| `docs/architecture.md` の `node tests/integer_ranges_fuzz.mjs target/release/tsuzuri 30 1 3` の行の後にコマンドを足す | その行は今の architecture.md にない（コマンドの一覧は README にある） | architecture.md には段落だけを足し、コマンドは README の「テストスイートの実行」と「変更の種類別」の表に足した |
| 手元の検証は `target/release/tsuzuri` | AGENTS.md: 共有の機械では dev プロファイル（opt-level 1）のビルドで足りる | 手元は `target/debug/tsuzuri` で確かめた。CI は従来どおり release |

## 実装者向け早見

- 作るもの: `tests/progfuzz.mjs`（1 ファイル）。乱数（seed）から型つきの Tsuzuri のプログラムを作り、(a) コンパイル時評価（`const` の初期化式）、(b) native `-O0`、(c) native `-O3`、(d) wasm32 `-O0`、(e) wasm32 `-O3` で動かし、JS の BigInt の評価器の値と突き合わせる。`--self-test` が、評価器・乱数・最小化・変異の検出を確かめる。
- **コードは付録 A にある。** 実測で緑にした全文なので、写して使う。位相の印（`// <<< phase N` … `// >>> phase N`）の間が位相 N の行。発明しない。迷ったら付録 A が正。
- 最初に読む（この順）: 付録 A の先頭 → `tests/last_use_fuzz.mjs`（200 行）→ `_tsuzuri/language-reference/values-and-functions/values.md` の「コンパイル時に評価する関数 (`const def`)」→ `src/const_rules.rs` の `validate_function`。
- 最初に動かすもの: 手順 1.3 で `node tests/progfuzz.mjs --self-test` が「評価器が表の 116 行すべてに一致する」と言うこと。手順 1.4 で、表がコンパイラの 3 つの経路（const・o0・o3）で一致すること。
- 触ってはいけない: コンパイラのソース（`src/`）・既存のテスト・既存の fuzz スクリプト・`.github/workflows/windows.yml`。`/tmp` を使わない（作業は `target/progfuzz/`、子プロセスの `TMPDIR` もそこ）。
- 実行コマンド: `node tests/progfuzz.mjs [コンパイラ] [件数] [最初の seed] [深さ]`（既存の 3 本と同じ位置引数。種の `--count --seed` ではない）。smoke は `node tests/progfuzz.mjs target/release/tsuzuri 200 1 4`（実測 2 分 24 秒）。失敗の再現は `… 1 <seed> <深さ>`。
- 既知のバグ **KB1**: `xs[ … for i in a .. b do … ]`（添字の中の範囲 `for`）は構文エラー `E0002`。生成器は添字の式を括弧の外の `let` に束縛して避ける（付録 A の `render` の `index`）。`--self-test` が KB1 の再現を確かめ、直ったら「回避を消せ」と言う。直すのは G36 ではない。
- 診断コードを足さない。新しい名前は環境変数 `TSUZURI_PROGFUZZ_*` と出力の `target/progfuzz/` だけ（「仕様」）。
- Phase の完了の見分け方: Phase 1 = `--self-test` が通り、`… 200 1 4` が `zero differences`（経路は const・o0・o3）。Phase 2 = 経路に wasm0・wasm3 が加わり、故意に壊した評価器（`TSUZURI_PROGFUZZ_MUTANT=div-floor`）で `target/progfuzz/failures/seed-N-o0/` が 3 ファイル（`Main.tz`・`original.txt`・`info.txt`）できる。Phase 3 = `TSUZURI_PROGFUZZ_KIND=owned … 100 1 4` が通り、`--self-test` の step 9 が通る。
- 迷ったら: 「決定事項」D1〜D12 → 付録 A。止める条件は「着手条件と停止条件」。

## 目的

HEAD の言語は、同じ式を 2 つの別の実装で評価できる。コンパイラは LLVM のコードを作り、`const` の初期化式は `src/const_machine.rs` がコンパイル時に評価する。文書は両者が一致すると約束する（`values.md`: 「同じ入力なら、コンパイラが評価した値と実行時の結果は一致します」）。さらに native と WASM、`-O0` と `-O3` が同じ意味を持つことも約束する（`basic-types.md`: 「整数の折り返し、ゼロ除算のトラップ、浮動小数点の丸めは、native と WASM で意味を変えない契約です」）。この約束を、型の組み合わせと制御構造を乱数で変えながら大量に検査する道具が無い。

G36 は、第 3 の実装として **JS の BigInt の評価器**（生成器と同じ構文木を評価。`BigInt.asIntN`／`asUintN` で幅を折り返す）を置き、5 つの経路との差を探す。コンパイル時評価はコンパイラの中で LLVM と独立した実装なので、参照実装の 1 つとして使う（経路 a）。3 つの実装のうち 1 つだけが違えば、違うものが誤りの候補になる。

試作（付録 A）で実測した事実（付録 B）:

- 200 本・8,000 回の実行（5 経路 × 8 入力）が 2 分 24 秒で終わり、差は 0。300 本（depth 5）の別の実行は 12,000 回で差 0。
- **最初の実行（300 本）で、本物のバグ KB1 が 18 本に出た**（全経路で `E0002`）。最小化で 1,500 節点が 72 節点に縮み、原因は `src/parser.rs` の `index_or_slice` が立てる旗 `stop_at_slice_dotdot` が角括弧の中のブロックにも残ること（付録 D）。
- 手計算の意味の表（116 行）は、評価器とコンパイラの 5 経路のすべてで一致した。ゼロ除算と最小値 `/ -1` は `E1026`（コンパイル時）・トラップ（実行時）で、生成器は作らない。
- 所有権のプログラム（`Vec`・`string`・`Vec<string>` の move・借用・drop）300 本・9,600 回で、wasm32 の `--allocator counting` の漏れの検査まで含めて差 0。

利用者の価値: (1) 最適化・型検査・コード生成の変更が、言語の意味を壊したときに、最小化された再現つきで CI が落ちる。(2) 文書の約束（折り返し・シフト量のマスク・範囲 `for` の端・`const def` と実行時の一致）が、手の例でなく乱数で検査される。(3) 退行しても結果が同じで時間だけが変わる種類は対象外（F14 の領分）。

非目標: 診断の検査（構文・型の誤りの生成）。浮動小数点・文字列（値の側）・クロージャ・ジェネリクス・trait・タスク・SIMD・GPU の生成。`i128`。Windows の CI。KB1 の修正。コンパイラの速度の測定。

## 着手条件と停止条件

### 着手条件

- D11 が done（`const def`・`const` の初期化式の評価。HEAD にある）。F14 は不要（Phase 3 の native の漏れの検査だけが使い、無ければ飛ばす）。
- Node 20 以上（`availableParallelism`、BigInt）。CI は Node 24。外部の npm の部品を足さない。
- ベースライン（手順 1.1）: `tests/progfuzz.mjs` が無いこと、既存の fuzz が動くこと、`target/release/tsuzuri` で `build --target wasm32 --no-cache` が通ること。
- 読む箇所: 「実装者向け早見」の 4 つ。`values.md` の「評価中のエラーと上限」（`E1026` の文面）。`op-and-expressions.md` の「ビット演算子とシフト演算子」。`for-in.md` の「整数範囲の反復」。

### 停止条件

止めて報告する。選択肢を添える。

1. **手順 1.3**: `--self-test` の「評価器と表」が食い違う。→ 表の手計算の誤り。その行を文書（`basic-types.md`・`op-and-expressions.md`・`for-in.md`）で確かめ直す。直せなければ止める。
2. **手順 1.4**: 表の行がコンパイラの経路（const・o0・o3・wasm0・wasm3）のどれかと食い違う。→ 文書か実装の誤り。選択肢 A（文書が実装を支持するなら表の行を直す）、B（実装の誤りなら、新しい不具合のチケットを起こして報告し、その行を付録 D の形で `KNOWN_BUGS` に記録して、行は表に残したまま進める）。
3. **手順 1.6・1.9**: smoke（`200 1 4`）が KB1 以外の差・コンパイルエラーを出す。→ 最小化（Phase 2 以降）か手で縮める。選択肢 A（コンパイラの不具合: 新しいチケット + `KNOWN_BUGS` + **構文を避けるだけ**の回避。意味を変える回避はしない）、B（生成器の誤り: 生成器を直す。評価器が `Trap` を投げるのは生成器の誤り）。
4. **手順 1.7**: KB1 が再現しない（パーサが直った）。→ 止めずに、`render` の `index` を `xs[(e as i64u) % ((xs).length as i64u) as i64]` の直接の形に戻し、`KNOWN_BUGS` の項目を消す。文書の注意（for-in.md）も消す。
5. smoke の所要時間が、実装者の機械でジョブ数 4 のとき 6 分を超える。→ 件数を 100 に下げて報告する（CI の `timeout-minutes` と合わせる）。
6. `def main :: [string] -> i32` が受理されない、または `args.length` が 0 でない。→ 入力を不透明にする方法（「設計」）を変える必要がある。止める。
7. wasm32 の `lib` の `.wasm` に import が現れる（`WebAssembly.Module.imports` が空でない）。→ `lib` の文面が `IO` を使っていないか見る。止める。
8. 変異（`MUTANTS`）のどれかが 200 本の seed で答えを変えない。→ 生成器がその演算を作っていない。止める。
9. **Phase 3**: F14 が実装済みなのに `supportsAlloc` が偽。→ F14 の名前（`Alloc.snapshot`・`Alloc.since`・`allocs`・`frees`）が違う。止めて F14 の完了報告と照合する。F14 が未実装なら、native の漏れの検査は無効のまま正常。
10. 差が 1 つのホスト（例: linux-arm64）にだけ出る。→ 本物の発見。最小化の保存を残し、ホスト名とコマンドを添えて報告する。止めずに他を進めてよい。

## 現状（HEAD `a2313d19` で確認）

調べたコマンドと実出力（`target/release/tsuzuri`）。

```sh
ls tests/progfuzz.mjs
# ls: tests/progfuzz.mjs: No such file or directory
head -3 tests/last_use_fuzz.mjs | tail -1
# // node tests/last_use_fuzz.mjs [compiler] [programs] [first seed] [depth]
grep -n "mulberry32\|1664525" tests/last_use_fuzz.mjs tests/integer_ranges_fuzz.mjs tests/local_recursion_fuzz.mjs | cut -c1-90
# （last_use は線形合同 1664525、残り 2 本は mulberry32）
```

- 既存の差分ファズ 3 本はどれも `node tests/<名前>.mjs [コンパイラ] [件数] [最初の seed] [深さ]` の位置引数で、失敗は例外（終了コード 1）、成功は 1 行（`last_use_fuzz` は続けて被覆の JSON）を出す。CI の段は `targets.yml` の「Last-use differential fuzz smoke」（`40 1 4`）と「Integer range proofs …」（`20 1 3`）。`windows.yml` には last_use の段だけがある。
- `integer_ranges_fuzz.mjs` は wasm32 を `new WebAssembly.Instance(new WebAssembly.Module(readFileSync(wasm))).exports` で読み、`exports.tz_f(…)` を呼ぶ（`export def f` は `tz_f` になる）。i64 は BigInt、それ以外は Number で渡す。
- `const def` が受理する範囲（`src/const_rules.rs`）: 署名は具体的で、引数と結果が「値型」（整数・浮動小数点・`bool`・`unit`・`char`・`string`・配列・固定長配列・タプル・record・union）。本体はスカラーの演算・`as`・`if`・`match`・`while`・範囲／配列／文字列の `for`・`break`／`continue`・`let mut` への再代入・record／タプル／配列／union の構築・`xs[i]`・`xs.length`・`new [T](n, \i -> …)`・`const def` の完全適用。拒否は std の関数・`Vec`・借用・ラムダ・パイプ・`@checked`・補間・タプル／配列への演算子。**43 件の本体で実測した表が付録 C**。
- 評価の上限（実測。付録 C）: 呼び出しの深さ 128 で `E1026` `const evaluation exceeds the compiler limit of 128 nested const def calls; …`。300 万回のループで `E1026` `const evaluation exceeds the compiler limit; split or simplify the constants`。予算は 1,048,576（`values.md`）。
- 演算子（`op-and-expressions.md`）: シフトは `<<<`（左）と `>>>`（右。符号付きは算術、符号なしは論理）、量は型の幅でマスクする。`<<`・`>>` は関数合成（種の記述の `<< >>` は誤り）。`&`・`|`・`^`・`~` はビット演算、`as` は整数の縮小で下位ビットを残す。
- 範囲 `for`（`for-in.md`）: `for i in 0 .. 3 do` は両端を含む。刻みを省くと 1、負にすると降順。次の値が型の範囲を外れるとその場で止まる（`125i8 .. 2i8 .. 127i8` は 125 と 127）。刻み 0 は実行時トラップ。
- wasm32 の出力の形（実測）: `export def run :: i64 -> i64` のライブラリは import が空で、`tz_run` を export する。`def main` と `IO.write_line` があるプログラムは import `tsuzuri_io.write` を持ち、`tz_run` を export しない（`run` は `export` が要る）。`tsuzuri build <dir> --target wasm32 -O3 --no-cache` は小さなプログラムで 0.35 秒。native の `tsuzuri run`／`build` は 0.5〜0.7 秒。`--no-cache` を付けないと、最初のビルドは数秒かかる（ランタイムの用意）。
- `def main :: [string] -> i32` は引数が `[string]`（`Env.args` と同じ並び）。引数なしで実行すると `args.length` は 0。コンパイラはこの 0 を知らない（不透明）。
- KB1（実測。付録 D）: `xs[({ let mut a: i64 = 0i64; for i in 0i64 .. 1i64 do { a = (a + i); () }; a })]` は `error[E0002]: expected 'do' after the enumerable expression`。
- 試作（付録 A）で 5 経路が 300 本・12,000 回一致し、所有権のプログラムが 300 本・9,600 回一致した。実測は付録 B。

## 変えてはいけない意味

- コンパイラ・ランタイム・std・既存のテスト・既存の fuzz・`windows.yml` は変えない。`cargo test` の件数は変わらない。
- 診断コード・kind は足さない。新しい CLI のフラグ・サブコマンドは足さない（`tsuzuri` の外の Node のスクリプト）。
- 外部の npm の部品を足さない（Node の標準モジュールだけ）。
- すべての生成物と一時ファイルは `target/` の下（既定は `target/progfuzz/`）。`/tmp` を使わない。子プロセスの `TMPDIR`／`TMP`／`TEMP` はそのプログラムの作業ディレクトリ。
- 生成するプログラムは**トラップしない**（ゼロ除算・最小値 `/ -1`・添字の範囲外・刻み 0 を作らない）。評価器がトラップを検出したら生成器の誤りで、実行は例外で落ちる。
- 一致を弱めない: 浮動小数点・NaN・符号付きゼロ・fast-math を持ち込まない（生成しない）。比較は文字列の完全一致。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 生成器・評価器・描画・意味の表・`--self-test`・経路 const／o0／o3・smoke の CI・文書 | L | D11 | 新しいファイルと CI の 1 段だけ。コンパイラは変わらない。wasm が無くても、3 経路の差分ファズとして完結する |
| 2 | wasm32 の経路 wasm0／wasm3・最小化・失敗の保存（`target/progfuzz/failures/`）・変異の通しの検査 | M | Phase 1 | 経路と最小化を足すだけ。Phase 1 の出力の形（成功の行・失敗の行）は変わらず、経路の数が増える |
| 3 | 所有権のプログラム（`TSUZURI_PROGFUZZ_KIND=owned`）・wasm32 の漏れの検査・（F14 があれば）native の漏れの検査・owned の smoke の CI・文書 | M | Phase 2 | 既定（`values`）の動作は変わらない。`owned` を指定したときだけ働く。F14 が無くても完結する |

## 仕様

### コマンドと環境変数

```text
node tests/progfuzz.mjs [コンパイラ] [件数] [最初の seed] [深さ]
node tests/progfuzz.mjs --self-test [コンパイラ]
```

| 引数 | 既定 | 規則 |
| --- | --- | --- |
| コンパイラ | `target/release/tsuzuri` | `resolve` する |
| 件数 | 20 | 正の整数 |
| 最初の seed | 1 | 0 以上。seed は `最初の seed` から `件数` 個。1 つの seed が 1 つのプログラムを決める |
| 深さ | 4 | 1〜8。関数の本体の式の深さの上限 |

範囲外・整数でない引数は `assert` で落ちる（`usage: node tests/progfuzz.mjs [compiler] [programs] [first seed] [depth 1..8]`）。

| 環境変数 | 既定 | 意味（位相） |
| --- | --- | --- |
| `TSUZURI_PROGFUZZ_PATHS` | `const,o0,o3`（Phase 2 から `const,o0,o3,wasm0,wasm3`。`owned` は const を除く） | 動かす経路のコンマ区切り |
| `TSUZURI_PROGFUZZ_JOBS` | `min(4, availableParallelism())` | 同時に調べるプログラムの数 |
| `TSUZURI_PROGFUZZ_DIR` | `target/progfuzz` | 作業と保存の場所。`--self-test` は `<DIR>/self-test` だけを使い、終わると消す（`failures/` は消さない） |
| `TSUZURI_PROGFUZZ_KEEP` | 未設定 | `1` で、各プログラムのディレクトリ（`seed-N/exe`・`const`・`lib`）を消さない |
| `TSUZURI_PROGFUZZ_MINIMIZE` | 未設定 | `0` で最小化しない（Phase 2） |
| `TSUZURI_PROGFUZZ_BUDGET` | 150 | 1 件の最小化で使うコンパイラの実行の上限（Phase 2） |
| `TSUZURI_PROGFUZZ_KIND` | `values` | `owned` で所有権のプログラム（Phase 3） |
| `TSUZURI_PROGFUZZ_MUTANT` | 空 | 評価器を故意に誤らせる名前（`MUTANTS`）。検査の検査用 |
| `TSUZURI_PROGFUZZ_INJECT_LEAK` | 未設定 | `--self-test` 専用。wasm の漏れの測定に 16 byte の確保を足す |

### 出力と終了コード

成功（標準出力 2 行）:

```text
progfuzz values: 200 programs (0 discarded for their size, 0 constant evaluations over budget), 8000 executions on const, o0, o3, wasm0, wasm3, seed 1, depth 4; zero differences
{"array_index":164,"array_length":84,"call":93, …}
```

2 行目は、生成した構文の被覆（タグ → 本数。キーの辞書順）。件数が 100 以上のとき、`REQUIRED_TAGS[kind]` の全タグが 1 以上であることを `assert` する（生成器が構文を作らなくなったら落ちる）。打ち切った本数が全体の 4 分の 1 を超えても `assert` で落ちる。

失敗（標準エラー）。経路ごとに 1 行、最小化できたときはその 1 行、最後に件数。終了コード 1:

```text
seed 1034 o0: compile …/seed-1034/exe/Main.tz:6:669: error[E0002]: expected 'do' after the enumerable expression
  minimized from 1500 to 72 nodes in 150 compiler runs: …/target/progfuzz/failures/seed-1034-o0
1 of 300 programs differ
```

```text
seed 2 o0: value input 2 (actual -55522, expected 9223372036854720285)
```

| 種類（`kind`） | いつ |
| --- | --- |
| `compile` | `build` が失敗した（診断の先頭 1 行が出る） |
| `run` | 実行ファイルが 0 以外・シグナルで終わった。wasm の呼び出しが `trap` した |
| `timeout` | 実行が 60 秒（ビルドは 120 秒）を超えた |
| `value` | 出力の行数か値が評価器と違う（`input N` は最初に違う入力の添字） |
| `limit`（差に数えない） | const 経路のビルドが `E1026` の「compiler limit」「nested const def calls」で止まった。評価の予算の超過は値の誤りではない |

`--self-test` の失敗は `assert` の例外（終了コード 1）。成功の最後の行は `progfuzz self test: ok`。

保存（Phase 2）: 失敗した最初の経路のプログラムを最小化し、`<DIR>/failures/seed-<seed>-<経路>/` に書く。

| ファイル | 内容 |
| --- | --- |
| `Main.tz` | 最小化したプログラム（その経路の文面。`tsuzuri run <dir>` や `build --target wasm32` にそのまま渡せる） |
| `original.txt` | 最小化の前の文面。`.tz` にしないのは、`tsuzuri run <dir>` がディレクトリの `.tz` をすべてモジュールとして読む（`original.tz` があると `E1011`）ため。保存先の `.tz` は `Main.tz` だけ（step 6 が確かめる） |
| `info.txt` | 6 行: seed・深さ・経路・種類／入力／期待値／実際／節点数の変化とコンパイラの実行回数／再現コマンド（付録 E に実物） |

### 生成する言語

**型**: 整数 `i8 i16 i32 i64 i8u i16u i32u i64u`、`bool`、タプル（2〜3 要素）、record `R0..`（フィールド 1〜4）、union `U0..`（case 2〜4。payload は無し／スカラー／スカラー 2 つのタプル／record）、`Maybe<T>`（T はスカラーか 2 要素のタプル）、配列 `[T]`（T はスカラー。長さは 1〜4 で、0 の配列を作らない）。型名は `R<i>`・`U<i>`、union の case は `K<i>_<j>`（`Rc` のような予約名と衝突しない）、関数は `f<n>`（`rec` で始まる名前を避ける。付録の試作で `recfuel` が `E1019` になった）、`run`。

**文面**（EBNF 風。`( … )` は括弧そのもの。すべての 2 項演算と式を括弧で包み、優先順位と結合性を使わない）:

```text
program  = { typedecl } { function } entry [ main ]
typedecl = "record" R "{" f ":" type { "," f ":" type } "}"
         | "union" U "=" K [ "of" type ] { "|" K [ "of" type ] }
type     = int | "bool" | R | U | "(" type "*" type { "*" type } ")" | "Maybe<" type ">" | "[" type "]"
function = "const def" [ " rec" ] name "::" type { "->" type } NL "fn" [ " rec" ] name param { param } "=" expr NL
entry    = ( "const def" | "export const def" ) "run :: i64 -> i64" NL "fn run k =" expr NL
expr     = lit | name
         | "(" expr op expr ")"                  ; op = + - * & | ^ / % <<< >>> と 比較 と && ||
         | "(-" expr ")" | "(~" expr ")" | "(!" expr ")" | "(" expr "as" int ")"
         | "(if" expr "then" expr "else" expr ")"
         | "({ let" name ":" type "=" expr ";" expr "})"
         | "(match" expr "with" { "|" pat "->" expr } ")"     ; 整数・bool・タプル・union・Maybe
         | "(" expr ")" "." f | "(" expr "," expr { "," expr } ")" | "(" R "{" f ":" expr … "})"
         | "({" expr "with" f "=" expr "})" | "(" K expr ")" | K | "(Maybe.Some" expr ")"
         | "({ let" n ":" type "= Maybe.None;" n "})"
         | "[" expr { "," expr } "]" | "(new [" type "](" len "," "\" i "->" expr "))"
         | "({ let" a ":" "[" type "]" "=" expr ";" "let" j ":" "i64u = ((" expr ") as i64u);" a "[(" j "% ((" a ".length) as i64u)) as i64] })"
         | "(" name expr { expr } ")"
         | "({ let mut" acc ":" type "=" expr ";" "for" i "in" expr [ ".." expr ] ".." expr "do {" [ "if" expr "then" ("break"|"continue") "else ();" ] acc "=" expr "; () };" acc "})"
         | "({ let mut" acc ":" type "=" expr ";" "let mut" c ": i32 = 0i32; while" cond "do {" c "= (" c "+ 1i32);" [ exit ] acc "=" expr "; () };" acc "})"
main     = ( "def main :: [string] -> i32 = \args ->" … )       ; 「実行経路と観測」
```

リテラルは、型の接尾辞つきで書き、負の値は括弧で包む（`(-5i8)`。`run (-3i64)` のように呼び出しの引数でも）。`match` の整数のパターンは接尾辞つきの値（負の値は括弧なし `-128i8`）。1 つの関数は 1 つの式。複数行にするのは、型宣言・`const def` の署名・`fn`・`main` の行だけ。

**節点の表**（付録 A の `render` と `evaluate` が全節点を扱う。32 種）:

| 節点 `t` | 意味（評価器） | 描画 |
| --- | --- | --- |
| `lit` | 値 | `5i32`・`(-5i32)`・`true` |
| `var` | 環境の値 | 名前 |
| `bin` | `+ - * & \| ^`。結果を `wrap` | `(a op b)` |
| `div` | `/` は 0 へ丸め、`%` は被除数の符号。除数 0・最小値 `/ -1` は `Trap` | `(a / b)` |
| `shift` | `<<<`: `wrap(a << (c & (bits-1)))`、`>>>`: `a >> (c & (bits-1))`（BigInt の `>>` は符号付きで算術、符号なしで論理）。`c` は `BigInt.asUintN(bits, c)` に直してからマスク | `(a <<< c)` |
| `neg` `not` `lnot` | `wrap(-a)`・`wrap(~a)`・`!a` | `(-a)` `(~a)` `(!a)` |
| `cast` | `wrap(型, a)`（ビットを保つ） | `(a as T)` |
| `cmp` | 正規化した値の比較（符号なしは符号なしの順序） | `(a < b)` |
| `logic` | 短絡 `&&` `\|\|` | `(a && b)` |
| `if` `let` | 選ばれた側だけ評価。`let` は型注釈つき | 上の EBNF |
| `matchint` | 上の arm から順に。リテラル、ガードつき 1 本、`_` | `(match s with \| 5i32 -> … \| m when (m < 3i32) -> … \| _ -> …)` |
| `matchbool` `matchtuple` `matchuni` `matchmaybe` | 構造の分解 | `(match s with \| true -> … \| false -> …)` ほか |
| `field` `tuple` `record` `update` | 構築と読み出し | `(r).f0`・`(a, b)`・`(R0 { f0: … })`・`({ r with f0 = … })` |
| `ctor` `some` `none` | union の case・`Maybe`。`none` は型注釈つきの `let` で束縛 | `(K0_1 e)`・`(Maybe.Some e)` |
| `array` `newarray` | 配列リテラル・`new [T](n, \i -> …)`（`i` は `i64`） | `[a, b]`・`(new [i32](3, \i0 -> …))` |
| `index` `len` | `a[(e as i64u) % len]` は常に範囲内。`len` は `i64` | 添字は括弧の外の `let` に束縛（KB1 の回避） |
| `call` | 関数の呼び出し。引数を左から | `(f0 a b)` |
| `for` | 範囲 `for`。評価の規則は下の表 | |
| `while` | カウンター `c < n`（`n` は 1〜6）と任意の条件で回る。`break`／`continue` つき | |

**意味の決定表**（`evaluate` の規則。**116 行の表 `TABLE`** が根拠で、付録 A にあり、`--self-test` が評価器と 5 経路のコンパイラの両方で確かめる）:

| 項目 | 規則 |
| --- | --- |
| 折り返し | `+ - *`・単項 `-`・`~` と `& \| ^` は、型の幅で折り返す（`i8` の `127 + 1` は `-128`、`i8u` の `0 - 1` は `255`）。最小値の `-x` は最小値 |
| 除算・剰余 | 0 へ丸める。`-7 / 2 = -3`、`-7 % 2 = -1`、`7 % -2 = 1`。符号なしの `255i8u / 2 = 127` |
| シフト | 量は値と同じ型。`count & (bits-1)` でマスク（負の量も、`bits` 以上の量も）。`1i8 <<< 9i8 = 2`、`1i32 <<< -1i32` は `<<< 31`。`>>>` は符号付きで算術（`-128i8 >>> 1i8 = -64`）、符号なしで論理 |
| `as` | 整数どうしはビットを保つ。`300i32 as i8 = 44`、`-1i8 as i8u = 255`、`-1i8 as i64u = 18446744073709551615` |
| 比較 | 符号なしは符号なしの順序（`200i8u > 100i8u`、`18446744073709551615i64u > 0i64u`） |
| 範囲 `for` | `lo .. hi` は両端を含む。刻み（既定 1。負で降順）。`i = lo` から、昇順なら `i <= hi`・降順なら `i >= hi` の間回り、`i + step` が型の範囲を外れたらその場で止まる。`lo > hi`（昇順）は 0 回。刻み 0 は `Trap`（作らない）。`125i8 .. 2i8 .. 127i8` は 125, 127。`250i8u .. 255i8u` は 6 回で終わる |
| `break`／`continue` | `for`／`while` の本体の先頭で `if cond then break else ()`。`continue` はその回の `acc = …` を飛ばす |
| 配列 | 長さは 1〜4。`xs[(e as i64u) % (xs.length as i64u) as i64]` は常に範囲内 |
| 再帰 | `rec` の関数は第 1 引数が燃料 `fuel: i64`。本体は `if fuel <= 0 then base else step`。`step` の自己呼び出しは `fuel - 1`。外からの呼び出しの燃料は 0〜4 の定数。1 つの関数の自己呼び出しは最大 2 か所 |

**トラップの回避**（生成時）: 除算の除数 `d` は `let g = d; (a / (if (g == 0 \|\| g == -1) then 1 else g))`（符号なしは `g == 0` だけ）か、0 と -1 を除いたリテラル。添字は上の形。ループの範囲は定数か `lit + (x & 3)` で 8 回以内。`for` の刻みは 0 でない定数。評価器が `Trap`（除数 0・最小値 `/ -1`・刻み 0・未束縛の名前）を投げたら、生成器の誤りで実行が落ちる（最小化の候補では、その候補を捨てる）。

**上限**: 評価器は 1 回の `run k` で 20,000 ステップ（節点の評価とループの 1 周）を超えたら `Limit` を投げ、そのプログラムを打ち切る（数えて除く）。定数評価の予算 1,048,576 とのあいだに余裕がある（300 本で超過 0）。入力は 8 個: `0, 1, -1, 2^63-1, -2^63`、乱数の 64 ビット 2 個、`0..999` の 1 個。

### `const def` の受理（生成器との対応）

生成する構文はすべて `const def` が受理する（付録 C の「受理」）。したがって**どのプログラムにも const 経路が使える**（const 専用の部分集合を作らない）。関数はすべて `const def`（再帰は `const def rec` と `fn rec`）にして、実行時の経路ではふつうの関数として動かす（文書: `const def` は普通の関数でもある）。

| 構文 | const での扱い | 生成器 |
| --- | --- | --- |
| `Array.get`・`Vec`・std の関数・`ref`・ラムダ（`new` の外）・`==` の集成値・補間・パイプ・`@checked` | `E1026` で拒否 | 作らない（添字は `xs[…]` と `%`） |
| 0 除算・最小値 `/ -1`・範囲外の添字 | `E1026`（`const division trapped on zero or signed overflow` ほか） | 作らない |
| 評価の予算の超過・呼び出し 128 段 | `E1026`（`compiler limit`／`nested const def calls`） | まれに起きたら `limit` として差に数えない |

### 実行経路と観測

| 経路 | ファイル | 手順 | 観測 |
| --- | --- | --- | --- |
| `const` | `const/Main.tz`: `const C<j>: i64 = run <入力>` を 8 個と、それを `to_string` で出す `main` | `build -O0 --no-cache` して実行 | 8 行の標準出力 |
| `o0` `o3` | `exe/Main.tz`: `main` が `run (<入力> + z)`（`z = args.length` は実行時に 0）を出す | `build -O0`／`-O3 --no-cache` して、**実行ファイルを直接**実行（タイムアウトで確実に殺せる） | 8 行 |
| `wasm0` `wasm3`（Phase 2） | `lib/Main.tz`: `export const def run` だけ（`main` なし） | `build --target wasm32 -O0`／`-O3 --no-cache`。子の Node が `tz_run(BigInt)` を 8 回呼ぶ | 8 行（`trap …` の行は `run` の差） |

native の入力が `+ z` なのは、`run` が定数の引数で呼ばれると `-O3` が全体を畳み込むため。wasm は JS から渡すので不透明。const 経路の `main` は `run` を呼ばない（`--self-test` が IR で確かめる: `main` の本体に `@tz.fn.Main.` への `call` が無い）。

### 失敗の最小化（Phase 2）

`minimize(program, stillFails, { budget, seconds })`: 構文木を保ったまま縮める貪欲法。候補は次の順（`candidates`）: 関数は `run` から先に、節点は親から先に。(1) 節点を同じ型の 0（整数）・`false`/`true`（bool）に置換、(2) 節点を同じ型の子に置換、(3) `let` を本体に置換（束縛を代入）、(4) `matchint` の arm の削除・ガードの削除、(5) ループの `exit`・`extra`・`step` の削除、(6) リテラルを 0 側へ（半分・±1）。候補ごとに `prune`（呼ばれない関数を捨てる）→ 重み（節点数、同じなら整数リテラルの絶対値の和）が減ること → `valid`（評価器が `Trap`／`Limit` を投げない）→ `stillFails`（その経路で同じ種類の失敗。`compile` は同じエラーコード）を調べ、最初に通った候補を採って先頭から繰り返す。`budget`（コンパイラの実行の回数。既定 150）か `seconds`（600）で止める。入力は、最初に違った 1 つに絞る。実測は付録 B（1,500 → 72 節点）。

### 所有権のプログラム（Phase 3）

`TSUZURI_PROGFUZZ_KIND=owned`。プログラムは**ステップの列**（付録 A の `generateOwned`）で、固定の補助関数 10 個（`OWNED_HELPERS`）と `run k` の本体（`let`・再代入・`for`・`while` の行）からなる。生成器が生きている変数を追跡し、move 済みの変数を使わない。

| 変数の種類 | 型 | ステップ |
| --- | --- | --- |
| `vec` | `Vec<i32>` | `newvec`（`Vec.empty()`／`Vec.with_capacity 4`／`Vec.of_array [..]`）、`push`、`grow`（`h_grow` で move して戻す）、`pop`（`h_pop`）、`loop`（`for` で push）、`whilepop`、`len`／`sum`（借用）、`clone`（`Vec.clone (ref v)`）、`drop`（`h_drop v` で move）、`arr`（`h_arr v` で `Vec.to_array`）、`branch`（`v = if c then h_grow … else h_pop …`）、`cdrop`（`if c then h_drop v else 定数`。**条件つきの move**。以後 `v` を使わない）、`mdrop`（`match` の arm で move） |
| `str` | `string` | `newstr`、`append`（`s + "…"`）、`slen`（借用）、`take`（move）、`cat`（2 つを move して 1 つ作る） |
| `q` | `Vec<string>` | `newq`、`qpush`（リテラル）、`qpushs`（`str` を move して入れる）、`qpop`（取り出した文字列を drop）、`qclear`、`qlen` |

`run` の値は、束縛したスカラーと最後まで生きている変数の長さを `acc * 31 + x` で混ぜた `i64`。ASCII だけの文字列（長さは UTF-16 の単位と同じ）。**漏れの検査**: wasm は `--allocator counting` でビルドし、各呼び出しの後で `tsuzuri_alloc_stats` の `live_bytes` が呼び出し前（統計用の 32 byte の確保だけ）と同じことを確かめる（`<値> leaked 0`）。native は F14 の `Alloc.snapshot ()`／`Alloc.since` で `allocs - frees` を求める（`<値> leaked 0`）。F14 が無いコンパイラでは `supportsAlloc` が偽になり、native は値だけを比べる（標準出力に 1 行の注意が出る）。

### 既知のバグ KB1

付録 D。生成器の `index` の描画が回避する。`KNOWN_BUGS` の項目（再現の `.tz` と、期待するエラー文面）を、`--self-test` が毎回確かめる。新しい不具合を見つけたら、同じ形で項目を足す。

### 互換性

`docs/stability.md` が約束する対象（診断のコード・JSON・edition・公開 API）に触れない。新しい名前は環境変数 `TSUZURI_PROGFUZZ_*` と出力先 `target/progfuzz/` だけ。WASM の import・IR・ABI は変わらない（検査するだけ）。

## 設計

### `tests/progfuzz.mjs` の構成（付録 A の順。位相は印）

| 部分 | 関数・定数 | 位相 |
| --- | --- | --- |
| 設定 | 引数、`allPaths`、`paths`、`kind`、`generator`、`wantedLines`、`mutant` | 1（wasm・owned の行は 2・3） |
| 乱数 | `xorshift(seed)`: `next`・`below`・`pick`・`chance`・`word`。xorshift64\*（seed を splitmix64 で混ぜる）。`KNOWN_RANDOM` が 3 つの seed の最初の 3 語を持つ（別の実装で求めた値） | 1 |
| 型 | `INTS`・`BOOL`・`low`・`high`・`wrap`・`fits`・`keyOf`・`showTy`・`pool`（型の端と小さい値のリスト） | 1 |
| 生成器 | `generate(seed, maxDepth)`: 型の宣言 → 補助関数 2〜5 個（25% が `rec`）→ `run`。内部に `gen`・`genInt`・`genBool`・`aggregate`・`division`・`shift`・`matchInt`・`consume`・`loopNode`・`loopBounds`・`callNode`・`selfCall`・`toI64`・`mix` | 1 |
| 描画 | `render`・`source(program, tail)`（`tail` は `run`／`const`／`lib`） | 1 |
| 評価器 | `evaluate(program, input)`・`Trap`・`Limit`・`check`。`mutant` による故意の誤り 6 種 | 1 |
| 所有権 | `generateOwned`・`replay`・`sourceOwned`・`evaluateOwned`・`OWNED_HELPERS` | 3 |
| 実行 | `exec`・`runPath`・`compare`・`writeSources`・`wasmCall` | 1（wasm は 2、`wasmOwnedCall` は 3） |
| 最小化 | `positions`・`directChildren`・`replaceAt`・`mapNodes`・`substitute`・`weightOf`・`prune`・`valid`・`candidates`・`minimize`・`minimizeFailure` | 2（owned の行は 3） |
| 表 | `TABLE`（116 行）・`tableProgram` | 1 |
| 本体 | `checkProgram`・`fuzz`・`report`・`REQUIRED_TAGS` | 1（`supportsAlloc` と owned の行は 3） |
| 自己検査 | `selfTesting`（step 0〜9）・`MUTANTS`・`KNOWN_BUGS`・`KNOWN_RANDOM` | 1（step 4・6 は 2、step 9 は 3） |

### 生成のアルゴリズム（付録 A の `generate`）

1. `xorshift(seed)` を作る。型の宣言: record 0〜2 個、union 0〜2 個。
2. 補助関数 `helpers = 2 + below(4)` 個。各関数の引数 1〜3 個と結果の型は `anyType(1)`（スカラー 45%、タプル 14%、record 12%、union 10%、`Maybe` 8%、配列 8%）。25% は `rec`（先頭に `fuel: i64`）。本体は `gen(結果の型, 引数, maxDepth)`。`gen` は型で振り分け、`d <= 0` なら葉（変数・型の違う変数からの `as`・リテラル）。整数は重み付きで算術 22・ビット 8・除算 7・シフト 8・単項 4・`as` 8・`if` 6・`let` 5・`match` 5・ループ 6・呼び出し 6・自己呼び出し 3・分解 8 から選ぶ。リテラルは 60% が `pool(型)`（型の端・小さい値）。`rec` の関数は、`if fuel <= 0 then base else step` の形で、自己呼び出しが 0 回なら強制的に 1 回入れる。
3. `run`: 入力 `k` から 2〜4 個の型つきの値を `(k >>> n) as T` で切り出し、補助関数の全部を 1 回ずつ呼び、結果を `toI64`（型ごとに分解して `acc * 31 + x` で混ぜる）で `i64` にして足し、さらに生成した項を 1 つ足す。
4. ループは 2 重まで（`ctx.loops`）。範囲は `loopBounds` が 7 種から選ぶ: 小さい昇順・空・刻みつき・型の最大値で終わる・降順（符号付き）・型の最小値から（符号付き）・`lo + (x & 3)` の動的な上端。
5. 入力 8 個を決めて返す。被覆のタグ（`for_loop`・`shift_count_over_width` …）を `program.tags` に集める。

### 入力を不透明にする（設計の理由）

native の `main` は `run (<入力> + z)` を呼ぶ。`z` は `args.length` で、`tsuzuri` が作った実行ファイルを引数なしで動かすと 0。コンパイラは 0 を知らないので、`-O3` は `run` を畳み込めない。定数のまま呼ぶと、-O3 が LLVM の定数畳み込みで全体を解いて、実行時の命令を検査しない。

### 並列と資源

`fuzz` は seed の待ち行列を `jobs` 個のワーカーが消費する。各プログラムは `seed-N/` の下の 3 つのディレクトリ（`exe`・`const`・`lib`）に文面を書き、経路を順に動かし、差が無ければディレクトリを消す。子プロセスはすべて `exec`（`spawn`、標準出力と標準エラーを集め、タイムアウトで `SIGKILL`）。`--no-cache` を付けるのは、並列のビルドが同じキャッシュを書かないため。

### 段ごとの変更表

| 手順 | ファイル | 変更 |
| --- | --- | --- |
| 1.2 | `tests/progfuzz.mjs`（新規） | 設定・乱数・型・`KNOWN_RANDOM`・`selfTesting` の step 0 |
| 1.3 | 同 | 評価器・`TABLE`・`tableProgram`・step 1 |
| 1.4 | 同 | `render`・`source`・`exec`・`runPath`（const・o0・o3）・`compare`・`writeSources`・step 5 |
| 1.5 | 同 | `generate`・step 2・3 |
| 1.6 | 同 | `checkProgram`・`fuzz`・`report`・`REQUIRED_TAGS` |
| 1.7 | 同 | step 7・8・`KNOWN_BUGS` |
| 1.8 | `.github/workflows/targets.yml`・`README.md`・`docs/architecture.md`・`for-in.md` | CI の 1 段・コマンドの 1 行・段落・KB1 の注意 |
| 2.1〜2.3 | `tests/progfuzz.mjs` | wasm の経路・最小化・失敗の保存・step 4・6 |
| 2.4 | `targets.yml`・`README.md` | smoke が wasm を含む旨・成果物のアップロード |
| 3.1〜3.3 | `tests/progfuzz.mjs` | owned の生成・漏れの測定（wasm・native）・step 9 |
| 3.4 | `targets.yml`・`README.md`・`docs/architecture.md` | owned の smoke・文書 |

## 前提とする他チケットのインターフェース

- **D11（done）**: `const def`・`const` の初期化式（`const C0: i64 = run 0i64`）。使う面は付録 C に実測がある。
- **F14（todo。Phase 3 の native の漏れの検査だけが使う）**: `Alloc.snapshot : unit -> Alloc.Stats`、`Alloc.since : Alloc.Stats -> Alloc.Stats`、`Stats` のフィールド `allocs`・`frees`（i64）、`tsuzuri run --allocator counting`・`tsuzuri build --allocator counting`（実行ファイル）。F14 の設計（D1〜D4）で固定済みの名前。未実装のとき: `supportsAlloc()`（付録 A）が、`Alloc` を使う小さなプログラムを `run --allocator counting` して、成功して `0` が出ない限り偽にし、native の漏れの検査を無効にする（wasm の漏れの検査は今の HEAD で動く）。F14 の試作のコンパイラで、native の検査が 30 本・差 0 で動くことを実測した（付録 B8）。

## 実装手順

試作の全文（付録 A）が、各手順の「内容」の正である。手順は、全文を位相の印と節の見出しコメントで切り出して、上から順に足す。節の見出しは `// ---- … ----` のコメント。位相 2・3 の印の領域（上から 2a〜2i・3a〜3s）と手順の対応は付録 F の表。切り出しに迷ったら、位相 1 の版（付録 F の `strip`）を一度に置いて、手順 1.2〜1.7 の「確認」を順に動かしてよい（後の手順の部分が入っていても、各「確認」の行は出る。行が増えるだけ）。位相 2・3 も同じ（その位相の版を置いて、手順の「確認」を順に動かす）。

### Phase 1

#### 手順 1.1: ベースライン

- 変更: なし。
- 内容: `node --version`（20 以上）。`ls tests/progfuzz.mjs`（無い）。`node tests/last_use_fuzz.mjs target/release/tsuzuri 5 1 4` が `last-use fuzz: 5 programs …` を出す。付録 D の再現の `.tz` を作業ディレクトリ（`target/progfuzz-try/`）に置き、`target/release/tsuzuri build target/progfuzz-try -O0 --no-cache -o target/progfuzz-try/x` が `error[E0002]: expected 'do' after the enumerable expression`（終了コード 1）。`export def run :: i64 -> i64` だけの小さなプログラムを `build --target wasm32 -O3 --no-cache` して、`WebAssembly.Module.imports` が空で `tz_run` がある。
- 確認: 上のすべてが期待どおり。

#### 手順 1.2: 骨格と乱数の既知の値（テスト先行）

- 変更: `tests/progfuzz.mjs`（新規）。
- 内容: 付録 A から、先頭のコメント・`import`・引数と環境変数（位相 1 の行だけ。位相 2・3 の印の間は入れない）・`xorshift`・型（`INTS` から `pool` まで）・`lit`・`variable`・`KNOWN_RANDOM` を写す。`selfTesting` は `KNOWN_RANDOM` を確かめる step 0 だけにし、`savedPaths` の行は後で使うので置く。ファイルの末尾は `if (selfTest) await selfTesting(); else throw new Error("fuzz() is added in step 1.6");`。設定の `let generator = generate;` は、`generate` がまだ無いので `let generator = null;` と書く（手順 1.5 で直す）。`wantedLines`・`mutant`・`mutantName`・`outputRoot`・`STEP_LIMIT`・`exeSuffix`・`jobs` は付録 A のとおり。
- 確認: `node tests/progfuzz.mjs --self-test` が `self test: xorshift64* gives the known words for 3 seeds` を出して終了コード 0（`selfTesting` の最後に `progfuzz self test: ok` を置いてよい）。KAT の値を 1 つ書き換えると落ちること。

#### 手順 1.3: 評価器と意味の表（テスト先行）

- 変更: `tests/progfuzz.mjs`。
- 内容: `Trap`・`Limit`・`check`・`evaluate`（`mutant` の分岐ごと。位相 3 の 2 行は入れない）、`TABLE`（116 行）、`tableProgram` を付録 A の位置に写す。`selfTesting` に step 1（評価器と表）を足す。
- 確認: `node tests/progfuzz.mjs --self-test` が `self test: the evaluator matches all 116 lines of the semantics table`。表の 1 行（例: `bin i8 + 127 1 -128` の結果）を書き換えると、`evaluator, table line "…"` で落ちる。
- 停止条件 1。

#### 手順 1.4: 描画・実行・表のコンパイラでの検証

- 変更: `tests/progfuzz.mjs`。
- 内容: `patternLiteral`・`literalText`・`render`・`int64Text`・`source`（位相 3 の行は入れない）、`exec`・`runPath`（位相 2・3 の領域を入れない。`const`・`o0`・`o3` だけ）・`compare`・`writeSources`（位相 3 の行を入れない）を写す。`allPaths`・`paths` は const・o0・o3。`selfTesting` に step 5 を足す。
- 確認: `node tests/progfuzz.mjs --self-test` が `self test: the compiler matches the semantics table on const, o0, o3`。数秒。
- 停止条件 2。

#### 手順 1.5: 生成器・同じ seed は同じプログラム・変異（テスト先行の続き）

- 変更: `tests/progfuzz.mjs`。
- 内容: `generate`（位相 1 の全体）と、`selfTesting` の前の `MUTANTS`（変異の名前 6 つの配列。位相 3 の `OWNED_MUTANTS` は入れない）を写す。`let generator = null;` を `let generator = generate;` に直す。`selfTesting` に step 2（40 個の seed が 40 個の違うプログラムで、2 回目も同じ）と step 3（`MUTANTS` の 6 つが、200 個の seed で答えを変える。`mutantSets` は位相 1 の 1 行だけ）を足す。
- 確認: `--self-test` が step 2・3 の 7 行を出す（変異ごとの本数は付録 B9 と同じ数: 122・65・67・99・164・37）。
- 停止条件 8。

#### 手順 1.6: 本体と smoke

- 変更: `tests/progfuzz.mjs`。
- 内容: `coverage`・`checkProgram`（位相 2・3 の行を入れない）・`fuzz`・`REQUIRED_TAGS.values`・`report` を写す。ファイル末尾を `if (selfTest) await selfTesting(); else report(await fuzz());` に直す。
- 確認: `node tests/progfuzz.mjs target/release/tsuzuri 20 1 4` が 1 行目 `progfuzz values: 20 programs (0 discarded for their size, 0 constant evaluations over budget), 480 executions on const, o0, o3, seed 1, depth 4; zero differences` と、2 行目の被覆の JSON を出す（約 12 秒）。`… 200 1 4` が `zero differences`・4,800 回（約 2 分 20 秒）。`TSUZURI_PROGFUZZ_MUTANT=div-floor node tests/progfuzz.mjs target/release/tsuzuri 5 1 4` が `seed 2 o0: value input 2 …` のような行を出して終了コード 1（Phase 2 の前は最小化がないので、差のある seed の行と `N of 5 programs differ` だけ）。
- 停止条件 3・5。

#### 手順 1.7: 定数の評価が本当にコンパイル時であること・既知のバグ

- 変更: `tests/progfuzz.mjs`。
- 内容: `KNOWN_BUGS`（KB1）を写し、`selfTesting` に step 7（const の `main` の IR に `@tz.fn.Main.` への `call` が無く、`exe` の `main` には `run` の `call` がある）と step 8（KB1 が今も `E0002` になる）を足す。
- 確認: `node tests/progfuzz.mjs --self-test` が最後に `progfuzz self test: ok`（位相 1 の版で約 4 秒）。
- 停止条件 4。

#### 手順 1.8: CI と文書（Phase 1）

- 変更: `.github/workflows/targets.yml`・`README.md`・`docs/architecture.md`・`for-in.md`（「ドキュメント」の表）。
- 内容: 「ドキュメント」の Phase 1 の行。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/loops-and-conditionals/for-in.md`。`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/targets.yml'))"`（YAML として読める）。

#### 手順 1.9: Phase 1 の全体の確認

- 内容: `node tests/progfuzz.mjs --self-test target/release/tsuzuri`、`node tests/progfuzz.mjs target/release/tsuzuri 200 1 4`、`cargo test --locked` の件数が手順 1.1 の前と同じ（Rust を変えていない）。完了報告（GUIDE §13.2）を末尾に書く。
- 確認: すべて通る。

### Phase 2

#### 手順 2.1: wasm32 の経路（テスト先行）

- 変更: `tests/progfuzz.mjs`。
- 内容: 位相 2 の印の行（`allPaths.push("wasm0", "wasm3")`、`wasmCall`、`runPath` の wasm の領域）を足す。
- 確認: `--self-test` の step 5 が `… on const, o0, o3, wasm0, wasm3`。`node tests/progfuzz.mjs target/release/tsuzuri 12 1 4` が 5 経路で `zero differences`。
- 停止条件 7。

#### 手順 2.2: 最小化の部品（テスト先行）

- 変更: `tests/progfuzz.mjs`。
- 内容: 位相 2 の最小化の領域（`SKIP` から `errorCode` まで。`weightOf`・`prune`・`candidates` の位相 3 の行は入れない）と `minimizeBudget` を足す。`selfTesting` に step 4（コンパイラ無しの性質「量が型の幅以上のシフトを含む」で、750 節点が 30 以下に縮む）を足す。
- 確認: `--self-test` が `self test: the minimizer cut 750 nodes to 20 in 109 tries`（数は seed 固定なので同じ）。

#### 手順 2.3: 失敗の保存

- 変更: `tests/progfuzz.mjs`。
- 内容: `tailOf`・`minimizeFailure`（位相 2 の 1 つの領域）と、`checkProgram` の中で `minimizeFailure` を呼ぶ 1 行（位相 2 の印の間）を足す。`selfTesting` に step 6（`div-floor` と `shift-saturating` の変異を故意に入れた評価器で、最小化して保存できる）を足す。
- 確認: `--self-test` が全部通る（約 72 秒）。`TSUZURI_PROGFUZZ_MUTANT=div-floor TSUZURI_PROGFUZZ_PATHS=o0 node tests/progfuzz.mjs target/release/tsuzuri 5 1 4` が終了コード 1 で、`target/progfuzz/failures/seed-2-o0/` と `seed-5-o0/` に `Main.tz`・`original.txt`・`info.txt` ができ、`Main.tz` が 10 行前後（実測 11 行と 9 行）で、`target/release/tsuzuri run target/progfuzz/failures/seed-2-o0` が動く（差のある値 `-1` を出して終了コード 0）。`target/progfuzz/failures` を消す。

#### 手順 2.4: CI と文書（Phase 2）

- 変更: `targets.yml`・`README.md`（「ドキュメント」の Phase 2 の行）。
- 確認: YAML として読める。smoke が 5 経路になった旨が README にある。

#### 手順 2.5: Phase 2 の全体の確認

- 内容: `--self-test`、`… 200 1 4`（5 経路・約 2 分半）。
- 確認: すべて通る。完了報告。

### Phase 3

#### 手順 3.1: 所有権のプログラム（テスト先行）

- 変更: `tests/progfuzz.mjs`。
- 内容: 位相 3 の領域のうち次を足す。(a) 設定の 3 つの領域: `kind` の読み込み、`kind === "owned"` のときの `paths` の絞り込み、`generator`・`nativeLeaks`・`wantedLines` の領域（`nativeLeaks` は偽のままなので、`leaked` の検査はまだ働かない）。(b) `source`・`evaluate` の中の位相 3 の行。(c) 所有権の節（`OWNED_HELPERS` から `evaluateOwned` まで。`sourceOwned` の `nativeLeaks` の分岐を含む）。(d) `writeSources`・`weightOf`・`prune`・`candidates` の中の位相 3 の行。(e) `REQUIRED_TAGS.owned`。(f) `OWNED_MUTANTS` と、`selfTesting` の `mutantSets.push` の 1 行。
- 確認: `TSUZURI_PROGFUZZ_KIND=owned TSUZURI_PROGFUZZ_PATHS=o0,o3 node tests/progfuzz.mjs target/release/tsuzuri 12 1 3` の 1 行目が `progfuzz owned: 12 programs (0 discarded for their size, 0 constant evaluations over budget), 192 executions on o0, o3, seed 1, depth 3; zero differences`。`--self-test` が全部通り、step 3 に `owned-pop-ignored`（27）と `owned-clone-aliased`（16）の行が加わる。

#### 手順 3.2: wasm32 の漏れの検査

- 変更: `tests/progfuzz.mjs`。
- 内容: `wasmOwnedCall`、`runPath` の wasm の領域の中の位相 3 の行（`--allocator counting` を付け、`wasmOwnedCall` を使う）、`supportsAlloc`（step 9 が呼ぶので、ここで置く。`Alloc` を使う小さなプログラムを `run --allocator counting` して、成功して `0` が出なければ偽）、`selfTesting` の step 9（owned の seed、変異の検出と最小化、漏れの測定の検査: 注入なしで `leaked 0`、注入ありで 16 の倍数）を足す。
- 確認: `TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/release/tsuzuri 100 1 4` が `… on o0, o3, wasm0, wasm3 … zero differences`（約 1 分 10 秒）。`--self-test` が全部通る（約 81 秒）。

#### 手順 3.3: native の漏れの検査（F14 があるとき）

- 変更: `tests/progfuzz.mjs`。
- 内容: `fuzz` の `kind === "owned"` の領域（`supportsAlloc()` の結果を `nativeLeaks` に入れ、偽なら注意を 1 行出す）と、`runPath` の native の領域の `--allocator counting` の行（`nativeLeaks` が真で owned のとき）を足す。`sourceOwned` の `nativeLeaks` の分岐は、手順 3.1 で `sourceOwned` ごと入っている。
- 確認: F14 が無い HEAD: 実行の最初に `progfuzz: the compiler has no Alloc module, so only the wasm paths check for leaks` が 1 行出て、結果は手順 3.2 と同じ。F14 の実装済みのコンパイラ: その行が出ず、`o0`・`o3` の出力が `<値> leaked 0` の形で検査され、差 0。

#### 手順 3.4: CI と文書（Phase 3）

- 変更: `targets.yml`・`README.md`・`docs/architecture.md`（「ドキュメント」の Phase 3 の行）。
- 確認: YAML として読める。

#### 手順 3.5: Phase 3 の全体の確認

- 内容: `--self-test`、`… 200 1 4`、`TSUZURI_PROGFUZZ_KIND=owned … 100 1 4`、`cargo test --locked` の件数が同じ。完了報告。
- 確認: すべて通る。

## テスト計画

### `node tests/progfuzz.mjs --self-test`（コンパイラに依存しない step を先に）

| step | 位相 | 入力 | 期待値 |
| --- | --- | --- | --- |
| 0 | 1 | seed 1・2・123456789 の `xorshift(seed).next()` の最初の 3 語 | `KNOWN_RANDOM`（Python の整数を 64 ビットで切って別に求めた値）と一致 |
| 1 | 1 | `TABLE` の 116 行を `evaluate` | 手計算の値（`wrap(I64, …)`）と一致 |
| 2 | 1 | seed 1〜40 を 2 回生成 | 同じ seed は同じ文面、40 個が相異 |
| 3 | 1・3 | 変異 6 + 2 つ。seed 1〜200（depth 4） | どの変異も 1 本以上で答えが変わる |
| 4 | 2 | 「量が型の幅以上のシフトがある」という性質で最小化 | 節点数 ≦ 30 かつ元の 10 分の 1 以下 |
| 5 | 1・2 | 表をプログラムにして、`paths` の各経路で実行 | 116 行すべて一致 |
| 6 | 2 | 評価器を `div-floor`・`shift-saturating` に変えて 60 本以内に差を見つける | `value` の差を見つけ、最小化して保存（節点数 ≦ 60）。保存した `Main.tz` に `def main`。保存したディレクトリの `.tz` は `Main.tz` だけ |
| 7 | 1 | seed 7（depth 3）の const の文面と run の文面を `--emit llvm` | const の `main` の本体に `@tz.fn.Main.` への `call` が無い。run の `main` に `run` への `call` がある |
| 8 | 1 | `KNOWN_BUGS` の各項目をビルド | 失敗し、診断に期待の文面（KB1: `error[E0002]: expected 'do' after the enumerable expression`） |
| 9 | 3 | owned の seed 1〜40・変異 `owned-pop-ignored`・漏れの測定 | 同じ seed は同じ文面。変異を検出して 12 ステップ以下に最小化。注入なしで `leaked 0`、注入ありで 16 ずつ増える |

### 差分ファズの実行（CI と手元）

| 名前 | コマンド | 期待値 |
| --- | --- | --- |
| smoke（values） | `node tests/progfuzz.mjs target/release/tsuzuri 200 1 4` | `zero differences`・8,000 回（Phase 1 は 4,800 回）・約 2 分半 |
| smoke（owned） | `TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/release/tsuzuri 100 1 4` | `zero differences`・3,200 回・約 1 分 10 秒 |
| 拡張（手で） | `node tests/progfuzz.mjs target/release/tsuzuri 20000 1 5`、`TSUZURI_PROGFUZZ_KIND=owned … 5000 1 5` | 差 0（時間は 4 時間ほど。CI には置かない） |

### 既存のテストへの影響

なし。根拠: 新しいファイル 1 つと、文書・CI の追記だけで、コンパイラ・既存のテスト・既存の fuzz を変えない。`cargo test --locked` の件数は手順 1.1 と同じ。`node scripts/check-docs.mjs` は `for-in.md` を通す。

## ドキュメント

| 対象 | 更新 | 位相 |
| --- | --- | --- |
| `README.md` | 「検証と性能測定」の「テストスイートの実行」の、`node tests/integer_ranges_fuzz.mjs …` の行の直後に 2 行: `node tests/progfuzz.mjs target/release/tsuzuri 200 1 4`（5 経路。説明は 1 文: 型つきの乱数のプログラムを const・native・wasm32 で動かして BigInt の評価器と突き合わせる。引数は `[コンパイラ] [件数] [最初の seed] [深さ]`）と `node tests/progfuzz.mjs --self-test target/release/tsuzuri`。Phase 2 で経路の説明を直し、Phase 3 で `TSUZURI_PROGFUZZ_KIND=owned` の行を足す | 1・2・3 |
| `docs/architecture.md` | `tests/last_use_fuzz.mjs` を説明する段落の直後に、G36 の段落（評価器・5 経路・最小化・自己検査・既知のバグ KB1）。同ファイルの `node tests/integer_ranges_fuzz.mjs target/release/tsuzuri 30 1 3` の行の後に `node tests/progfuzz.mjs target/release/tsuzuri 50 1 4` | 1・3 |
| `_tsuzuri/language-reference/loops-and-conditionals/for-in.md` | 「整数範囲の反復」の節に、KB1 の注意（`> [!WARNING]`）: 「`xs[…]` の角括弧の中に、範囲の `for` を含む式を書くと `E0002`（`expected 'do' after the enumerable expression`）になる。式を `let` に束縛してから添字にする」。直ったら消す（G36 の `--self-test` が知らせる） | 1 |
| `.github/workflows/targets.yml` | 下の YAML | 1・2・3 |
| `docs/language.md`・`_tsuzuri/language-reference/` の他のページ | 更新不要（利用者に見える動作は変わらない）。PR の説明に書く | — |
| `_features/README.md`・`GUIDE.md`・台帳 | 統括者が更新する（報告の項目に書く） | — |

CI の段（`targets.yml`。`benchmark-correctness-${{ matrix.host }}` を上げる `actions/upload-artifact@v7` の段の直後で、コメント `# Other tickets' end-to-end steps for these four hosts go here, before the Rust tests …` の直前。このコメントは、長い段を `Rust tests` の前の最後にまとめて、遅いランナーが他の結果を失わせないようにする指示で、段は 6 分前後かかる）:

```yaml
      - name: Whole-language differential fuzz (G36)
        if: ${{ !cancelled() && steps.bundle.outcome == 'success' }}
        timeout-minutes: 30
        run: |
          export TSUZURI_CLANG="$LLVM_PREFIX/bin/clang"
          export TSUZURI_WASM_LD="${TSUZURI_WASM_LD:-$LLVM_PREFIX/bin/wasm-ld}"
          node tests/progfuzz.mjs --self-test target/release/tsuzuri
          node tests/progfuzz.mjs target/release/tsuzuri 200 1 4
          TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/release/tsuzuri 100 1 4
      - uses: actions/upload-artifact@v7
        if: ${{ failure() }}
        with:
          name: progfuzz-failures-${{ matrix.host }}
          path: target/progfuzz/failures
          if-no-files-found: ignore
```

Phase 1 では `--self-test` と `… 200 1 4` の 2 行、Phase 3 で 3 行目を足す。`windows.yml` には足さない（`.exe`・CRLF・トラップの終了コードの検証が Windows の緑でしか確かめられない。後続の課題）。

検証: `node scripts/check-docs.mjs _tsuzuri/language-reference/loops-and-conditionals/for-in.md`。

## 受け入れ条件

### Phase 1

- [ ] `node tests/progfuzz.mjs --self-test target/release/tsuzuri` が通る（step 0・1・2・3・5・7・8。約 4 秒）。
- [ ] `node tests/progfuzz.mjs target/release/tsuzuri 200 1 4` が const・o0・o3 で `zero differences`。
- [ ] 表 116 行が評価器とコンパイラの 3 経路で一致する。
- [ ] 生成器の被覆（`REQUIRED_TAGS.values` の 32 個）が 200 本で全部 1 以上。
- [ ] `targets.yml` の 1 段・README・architecture・for-in.md の KB1 の注意がある。`cargo test --locked` の件数が変わらない。
- [ ] `/tmp` を使っていない（`grep -n "/tmp" tests/progfuzz.mjs` が 0 件）。

### Phase 2

- [ ] 5 経路で `… 200 1 4` が `zero differences`。step 4・6 を含む `--self-test` が通る（約 72 秒）。
- [ ] 故意に壊した評価器の失敗が `target/progfuzz/failures/seed-N-経路/` に 3 ファイルで保存され、`Main.tz` が `tsuzuri run` でその差を再現する。
- [ ] CI の段が成果物（`target/progfuzz/failures`）を失敗時にアップロードする。

### Phase 3

- [ ] `TSUZURI_PROGFUZZ_KIND=owned … 100 1 4` が `zero differences`。`REQUIRED_TAGS.owned` の 25 個が全部 1 以上（100 本以上のとき）。
- [ ] `--self-test` の step 9 が通る（約 81 秒）。F14 が無いコンパイラで native の漏れの検査が無効になる旨の 1 行が出る。
- [ ] F14 があるコンパイラで native の漏れの検査が有効になり、差 0。
- [ ] 全体の `--self-test`・values の smoke・owned の smoke が CI の 1 段に入っている。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| 名前が `rec` で始まる関数（`recfuel`）は `fn rec` の組と誤読されて `E1019`。関数名は `f<n>`、変数は `v`・`a`・`i`・`g`・`ix` の接頭辞 | 試作で再現した。`tsuzuri check` |
| union の case に予約名（例: `Rc`）を使うと `E1001` | case は `K<i>_<j>` |
| `Vec.empty ()`（空白つき）は `E1006`。`Vec.empty()` と書く | 手順 3.1 の実行 |
| 負の値を呼び出しの引数に直接書くと `run -3i64` が `run - 3i64`（引き算）になる | `render` は負のリテラルを括弧で包む |
| `Maybe.None` は型が決まらないと `E1015`。`let n: Maybe<T> = Maybe.None` で束縛する | `render` の `none` |
| wasm32 の `.wasm` に `IO.write_line` があると import が増え、空の import でインスタンス化できない | `lib` の文面には `main` を置かない（手順 2.1） |
| i64 を JS に渡すときは BigInt。`tz_run(5)` は `TypeError` | `wasmCall` は `BigInt(input)` |
| `tsuzuri run` は子の実行ファイルを残すので、`SIGKILL` が親だけを殺し、無限ループのプログラムが残る | native は `build` してから実行ファイルを直接動かす |
| 並列のビルドが同じキャッシュを書く | `--no-cache` を付ける（付録 B の実測で、`--no-cache` の wasm32 のビルドは 0.1〜0.35 秒） |
| Windows の標準出力が CRLF | `exec` が `\r\n` を `\n` にする（Windows の CI は G36 の範囲外） |
| 警告（`unused local`）が標準エラーに出る | 比較は標準出力と終了コード。標準エラーは `compile`・`run` の診断にだけ使う |
| `for` の範囲を `xs[…]` の中に書くと KB1 | `index` の描画（付録 A）。`--self-test` の step 8 |
| 最小化の候補が評価器で `Trap` を投げる（例: 除数の保護を外した候補） | `valid` が捨てる。`stillFails` は `valid` の後にだけ呼ばれる |
| 乱数を JS の `Math.random`・32 ビットの演算で書くと、seed がホストで違う | BigInt の xorshift64\*。step 0 が既知の値を確かめる |

## 対象外

- 構文・型の誤りの生成と診断の検査。`tsuzuri check` の検査。
- `i128`・浮動小数点・decimal・`char`・文字列（値の側）・クロージャ（`new` の初期化以外）・ジェネリクス・trait・タスク・`Rc`・SIMD・GPU・`--wasm-feature`・wasm64・`--allocator host`／`small`・`-O1`／`-O2`・native 以外の target・`--trap-mode return`。
- 浮動小数点の `NaN`・符号付きゼロの検査（既存の `tests/float*.mjs` 系の領分）。
- Windows の CI への追加（検証できない）。
- KB1 の修正。**新しい不具合のチケットを起こす**（報告に書く）。
- 実行時間・確保数の測定（F14・`tsuzuri bench` の領分）。
- 生成するプログラムの数・深さの CI 上での長時間の拡張（手で実行する）。

## 決定事項

### D1: 引数は既存の fuzz と同じ位置引数 `[コンパイラ] [件数] [最初の seed] [深さ]`

決定: 3 本の既存の fuzz と README・CI の書き方に合わせる。却下: 種の `--count 200 --seed 1`（既存の流儀と違い、README の 3 行・CI の 2 段と並べると読み分けが要る）。`--self-test` だけが先頭の引数で切り替わる。環境変数は付加的な設定だけ。状態: 承認済み。

### D2: 参照は JS の BigInt の評価器。コンパイル時評価は「被験者」であり第 2 の実装

決定: 評価器は構文木を評価する（`BigInt.asIntN`／`asUintN`）。const 経路は、LLVM と独立なコンパイラ内の実装として、同じ値を出すかを見る。3 者のうち 1 者だけが違えば、それが誤りの候補。却下: const の値を期待値にする（const 自身の誤りを見逃す）。コンパイラの出力から期待値を作る（独立した参照にならない）。状態: 承認済み。

### D3: 構文木から文面を作る。すべて括弧で包んだ 1 行の式

決定: 優先順位・結合性・レイアウトを使わず、意味だけを検査する。優先順位は別の検査の領分（既存のパーサのテスト）。却下: 文面を直接作る（評価器が文面を読む必要が出る）。インデントのレイアウトを作る（意味の検査に関係しない構文の経路を増やす）。状態: 承認済み。

### D4: 全関数を `const def` にして、どのプログラムにも const 経路を使う

決定: 生成する構文は `const def` の受理の範囲の内側（付録 C）。実行時の経路でも同じ文面を動かす（文書: `const def` は普通の関数）。却下: const 専用の部分集合を別に作る（経路ごとの文面が増え、const の被覆が狭まる）。状態: 承認済み。

### D5: トラップを作らない。作ったら生成器の誤り

決定: 除数は `let` に束縛して 0／-1 を 1 に替える。添字は `% 長さ`。評価器の `Trap` は例外で落とす。トラップの意味は、既存の `tests/features.mjs`（`traps`）が検査している。却下: トラップも比較する（native はシグナル、wasm は `RuntimeError`、const は `E1026` で、比較の定義が経路ごとに違う）。状態: 承認済み。

### D6: 入力は 8 個。native は `args.length` で不透明にする

決定: 境界 5 個（`0`・`1`・`-1`・`2^63-1`・`-2^63`）+ 乱数 3 個（64 ビット 2 個と `0..999` の 1 個）。native の `main` は `run (入力 + z)`。却下: 定数の引数（`-O3` が全部畳み込む）、標準入力から読む（解析のコードが増え、被験者が増える）、C の driver（`clang` が要る）。状態: 承認済み。

### D7: 位相の印つきの 1 つのファイル

決定: `tests/progfuzz.mjs` を 1 つに保つ（種の指定）。位相は `// <<< phase N`／`// >>> phase N` で示し、`strip`（付録 F）で位相ごとの版が作れて、それぞれが動く（実測）。却下: ファイル分割（既存の fuzz と形が違い、共有の定数が増える）。状態: 承認済み。

### D8: 最小化は構文木のまま縮める貪欲法。予算つき

決定: 候補は構文を保つ置換だけ。コンパイラの実行は 150 回・600 秒まで。同じ種類の失敗が続くものだけ採る。却下: 文面の行・文字の削除（構文が壊れ、別の失敗に化ける）。最小化なし（実測で 1,500 節点の差は読めない）。状態: 承認済み。

### D9: 既知のバグは「構文を避ける回避」と「再現の自己検査」で扱う（KB1）

決定: 回避は構文だけを避け、意味を変えない。再現を `KNOWN_BUGS` に持ち、直ったら自己検査が落ちて回避を消させる。却下: ファズを赤のまま置く（CI の他の段の価値を下げる）。範囲 `for` を生成しない（`for` の被覆が減り、バグの再現も消える）。状態: 承認済み。

### D10: 所有権のプログラムはステップの列。補助関数は固定

決定: 生成器が生きている変数を追跡する。条件つきの move（`cdrop`・`mdrop`）は、その後その変数を使わない。却下: Phase 1 の構文木に借用・move を載せる（借用検査の規則を生成器に写すことになり、無効なプログラムが増える）。状態: 承認済み。

### D11: 漏れの検査は wasm の `--allocator counting`（今ある機能）を主とし、native は F14 があれば足す

決定: wasm は `tsuzuri_alloc_stats` の `live_bytes` が呼び出し前後で同じことを見る（`tests/allocator.mjs` と同じ読み方）。native は F14 の `Alloc` で `allocs - frees`。却下: F14 を必須にする（Phase 3 が F14 の完了まで止まる）。状態: 承認済み。

### D12: CI は `targets.yml` の 1 段だけ。Windows には足さない。拡張は手で

決定: 4 つのホストで `--self-test`・values 200・owned 100（約 6 分）。失敗時に `target/progfuzz/failures` をアップロード。却下: `windows.yml`（検証できない）、拡張の `20000` を CI に置く（時間）。状態: 承認済み。

## 付録

[G36-appendix.md](G36-appendix.md): A `tests/progfuzz.mjs` の全文、B 実測、C `const def` の受理の実測、D KB1、E `.tz` の例、F 位相ごとのファイルを作る手順。

## 実装と検証（2026-10-31）

- **範囲**: Phase 1〜3 をすべて実装した。成果は 1 つのファイル `tests/progfuzz.mjs`（1,761 行。位相の印つき）で、土台は付録 A の全文、差は冒頭の「方針の最新化」の表のとおり（edition 2027 の正準形・`own`・2027 の `Vec.push` と `Vec.push_owned`・`..<` の生成と表の 10 行・自己検査の最小化を回数だけで止める `minimizeSeconds`）。コンパイラ・既存のテスト・既存の fuzz・`windows.yml` は変えていない（Rust の変更なし）。全 Phase を 1 つのブランチ・PR にした（AGENTS.md の 1 チケット 1 PR）。
- **Phase 1**: 設定・`xorshift`（3 seed の既知の語）・型・生成器（範囲の `for` の終端の演算子 `op` は `..=` か `..<`）・edition 2027 の描画・評価器（`..<` と変異 6 種）・126 行の `TABLE` と `tableProgram`・`exec`・`runPath`（const・o0・o3）・`compare`・`writeSources`・`checkProgram`・`fuzz`・`report`・`REQUIRED_TAGS.values`（33 個）・`KNOWN_BUGS`（KB1）・自己検査の step 0・1・2・3・5・7・8。
- **Phase 2**: wasm0・wasm3 の経路（`wasmCall`。`lib` の `.wasm` は import なし）、構文木の最小化（`SKIP`〜`errorCode`）、失敗の保存（`tailOf`・`minimizeFailure`）、自己検査の step 4・6。
- **Phase 3**: `TSUZURI_PROGFUZZ_KIND=owned`（`OWNED_HELPERS` 10 個・`generateOwned`・`replay`・`sourceOwned`・`evaluateOwned`）、wasm32 の漏れの検査（`--allocator counting`・`wasmOwnedCall`）、native の漏れの検査（F14 の `Alloc`。`supportsAlloc` は真）、`REQUIRED_TAGS.owned`（25 個）、`OWNED_MUTANTS`、自己検査の step 9。
- **検証**（`target/debug/tsuzuri`（dev、opt-level 1）、Apple Silicon 8 コア、Node v20.17.0。共有の機械で他のセッションが並行し、load average は 12〜49 だったので、時間は参考値）:
  - `node tests/progfuzz.mjs --self-test target/debug/tsuzuri`: `progfuzz self test: ok`（989 秒）。表の 126 行が評価器と const・o0・o3・wasm0・wasm3 で一致した。変異が答えを変えた本数（200 本中）は shift-saturating 123・mul-unwrapped 56・div-floor 74・shr-logical 95・cast-saturate 176・loop-exclusive 41・owned-pop-ignored 27・owned-clone-aliased 16。最小化の検査は 1,084 → 21 節点（140 回）。step 6 は div-floor を seed 6 で検出して 47 節点、shift-saturating を seed 1 で検出して 50 節点に縮めて保存した。step 7・8（const の `main` がユーザー関数を呼ばない・KB1 が再現する）と step 9（owned の seed、owned-pop-ignored を seed 2 で検出して 2 ステップ、漏れの測定が注入なしで 0・注入ありで 16 ずつ）も通った。
  - 付録 F の `strip` で作った Phase 1 だけの版（1,190 行）の `--self-test` が通った（6.5 秒）。Phase 1・2 の版（1,396 行）は `node --check` が通る。
  - 付録 A のままの版: step 0〜4 が付録 B9 と同じ数を出し、step 5 が edition 2027 の `E0004`（素の `..`）で落ちた（「方針の最新化」の根拠）。
  - `node tests/progfuzz.mjs target/debug/tsuzuri 200 1 4`: `progfuzz values: 200 programs (0 discarded for their size, 0 constant evaluations over budget), 8000 executions on const, o0, o3, wasm0, wasm3, seed 1, depth 4; zero differences`（179 秒）。被覆の 33 タグはすべて 1 以上（`loop_half_open` 92、最少は `loop_from_type_edge` 15）。
  - `TSUZURI_PROGFUZZ_KIND=owned node tests/progfuzz.mjs target/debug/tsuzuri 100 1 4`: `progfuzz owned: 100 programs (0 discarded for their size, 0 constant evaluations over budget), 3200 executions on o0, o3, wasm0, wasm3, seed 1, depth 4; zero differences`（83 秒）。native の経路も `<値> leaked 0` を検査した（F14 があるので注意の行は出ない）。25 タグはすべて 1 以上。
  - `TSUZURI_PROGFUZZ_MUTANT=div-floor TSUZURI_PROGFUZZ_PATHS=o0 node tests/progfuzz.mjs target/debug/tsuzuri 10 1 4`: 終了コード 1、`3 of 10 programs differ`（176 秒）。`target/progfuzz/failures/seed-6-o0`・`seed-7-o0`・`seed-9-o0` に `Main.tz`・`original.txt`・`info.txt` ができた（342 → 27、542 → 20、559 → 39 節点。いずれも 150 回。`Main.tz` は 15・12・15 行）。`tsuzuri run target/progfuzz/failures/seed-7-o0` は `1`（0 へ丸める剰余）を出して終了コード 0 で、誤らせた評価器の期待値 `-1798002995847787663`（床の剰余）との差を再現した。新しい乱数の列では seed 1〜5 に床と 0 への丸めが違う除算がないので、手順 2.3 の `5 1 4` は差 0 になる（変異の最初の検出は seed 6）。
  - `node scripts/check-docs.mjs _tsuzuri/language-reference/loops-and-conditionals/for-in.md`（`TSUZURI_BIN=target/debug/tsuzuri`）: 1 ページ・10 リンク・13 例・native 26 回、成功。for-in.md の注意の 2 つの書き方（`xs[…]` の中の `for` が `E0002`、`let` に束縛すると `20`）は手で確かめた。
  - `node tests/repo_kb.mjs`: 78 件成功。`node scripts/check-guide-lite.mjs`: 86 件の参照、失敗 0。`targets.yml` は YAML として読める。`grep -n "/tmp" tests/progfuzz.mjs` は 0 件。Rust を変えていないので `cargo fmt`・`cargo clippy`・`cargo test` は実行していない（件数は変わらない）。
- **文書**: `README.md`（「テストスイートの実行」に 3 行、「変更の種類別」の表の 3 行に `progfuzz.mjs`）、`docs/architecture.md`（G36 の段落）、`docs/language.md`（整数範囲の節に KB1 の 1 文）、`for-in.md`（「整数範囲の反復」に KB1 の既知の不具合の注意）、`.github/workflows/targets.yml`（段「Whole-language differential fuzz (G36)」と、失敗時の `target/progfuzz/failures` のアップロード）、`_features/README.md` の G36 の 2 行、付録の冒頭の注記（出荷版との差）。ほかの言語リファレンスのページは、利用者に見える動作が変わらないので更新していない。
- **見つけた不具合**: 新しいものはない（smoke の values 200 本・owned 100 本で差 0）。KB1（付録 D。`src/parser.rs` の `stop_at_slice_dotdot`）は edition 2027 の `..=`・`..<` でも再現する。修正は G36 の範囲外。
- **後続**: (1) KB1 を直す不具合のチケット（直ると `--self-test` の step 8 が失敗し、`render` の `index` の回避・`KNOWN_BUGS` の項目・for-in.md と language.md の注意の削除を求める）。(2) Windows の CI（`.exe`・CRLF・トラップの終了コード。D12）。(3) 手で回す拡張（`20000 1 5`、owned の `5000 1 5`）は未実行。(4) release ビルドの CI の所要時間は、最初の `targets.yml` の実行で確かめる（試作の実測は約 6 分。段の上限は 30 分）。
