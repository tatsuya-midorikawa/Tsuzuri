# A23: 整数の折り返しの意図 — `Int.wrapping_*`・オーバーフロー報告 `--overflow-report`・`tsuzuri test` の既定

| 項目 | 内容 |
| --- | --- |
| ID | A23 |
| 優先度 | P2（D-50 の表） |
| 規模 | 全体 L（D-50 の表は M。Phase 2 の driver・WASM・test の配線を数え直して L にした）。Phase 1: S（1〜2 日）、Phase 2: L（1〜2 週）、Phase 3: M（3〜5 日） |
| 依存 | [PR02](../../_perfs/_completed/PR02-integer-ranges.md)（done。範囲の証明 `RangeFacts::integer_flags` と `RangeFlags` の `audit` の流儀を使う）。[G22](G22-diagnostic-v2.md) は**弱い依存**: 本チケットは新しい診断コードを作らず、実行時の報告だけを足すので、G22 を待たずに実装する。G22 が決める JSON のキーの順序（D-51）には、`run --json` の 1 行（`W2001`。付録 D.2）を従わせる |
| 後続 | なし。[A20](A20-effect-summary-and-checked-attributes.md) の `@no_trap` とは無関係（折り返しは trap ではない）。[G26](G26-facts-model.md) の `trap_sites` に折り返しの地点は入れない |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは同じコードの `9dd0fbbc`。差は `docs/benchmarks.md` だけ） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D15 はすべて承認済み |
| 改善する劣位 | Rust 比（`overflow-checks`・`wrapping_add`）: 整数の `+ - *` は常に黙って折り返し、意図した折り返し（ハッシュ・乱数）と間違い（在庫の加算）を、ソースでも実行でも区別できない。`Int.wrapping_pow` はあるが `wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg` がない。D-50 の調査結論(1)（範囲の事実を診断にもツールにも出さない）と(2)（署名や契約として固定できない）に当たる |
| 手本にする既存実装 | 検査してから元の演算を書く計装: `src/llvm_ranges.rs` の `arithmetic_flags`・`audit`・`audit_increment`。溢れの旗: `src/llvm.rs` の `FunctionEmitter::checked_integer_arithmetic`。範囲の証明: `src/ranges_intervals.rs` の `RangeFacts::integer_flags`、`src/interval.rs` の `Flags`。関数の述語: `src/coverage.rs` の `counted`。テストの実行: `src/test_runner.rs` の `execute_test`・`run_captured`・`read_tail`、`src/runtime/test-runner.mjs`。メッセージと `write(2)`: `src/llvm_traps.rs` の `instrument`、`src/trap.rs` の `TrapSite::message`・`side_table`。WASM の export: `src/llvm_traps.rs` の `@tsuzuri_trap_site` と `src/driver.rs` の `trap_sidecar_path`。scope: `src/target.rs` の `PlatformScope`。組み込み関数: `src/check.rs` の `Builtin::IntWrappingPow` と `src/llvm.rs` の `FunctionEmitter::integer_builtin`。inline: `src/call_specialization.rs` の `intrinsic_operation`（PR02 D10）。テスト: `tests/integer_intrinsics.mjs`・`integer_ranges.rs`・`range_flags_audit.mjs`・`test_runner.rs`・`coverage.rs` |
| 主な影響ファイル | Phase 1: `src/check.rs`・`src/llvm.rs`、`tests/integer_wrapping.rs`（新規）。Phase 2: `src/overflow.rs`・`src/llvm_overflow.rs`（新規）、`src/llvm.rs`・`src/llvm_exception.rs`・`src/ranges_intervals.rs`・`src/driver.rs`・`src/main.rs`、`tests/overflow_report.rs`・`tests/overflow_report.mjs`・`tests/fixtures/overflow_report/`（新規）。Phase 3: `src/test_runner.rs`・`src/runtime/test-runner.mjs`・`src/main.rs`・`src/driver.rs`、`tests/fixtures/property/Props.tz`。文書は「ドキュメント」の表 |

## 方針の最新化（2026-10-29。実装前の再検討）

起票時（HEAD `a2313d19`）の前提と、A19〜A22・G23 の実装後の現状が食い違う点を、実装前に次のとおり改めた（人間の包括許可による判断。D1〜D15 の趣旨は保つ）。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| `Int.wrapping_add x y` の完全な適用は `callee` が `FunctionRef::Builtin` の呼び出しで、関数値のときだけ `@tz.builtin.Int.wrapping_add.i32` が登録される（手順 3 ⑥・H4） | `closures::lower` が組み込み関数の参照をすべて `$builtin` のラッパー（`@tz.fn.$builtin.Int.wrapping_add.N`）に置き換えるので、完全な適用もラッパーの呼び出しになる | D8 の inline を 2 か所で行う: ラッパーの直接の呼び出し（`call_specialization::wrapping_builtin` がラッパーを見分ける）と、ラッパー自身の本体（`FunctionRef::Builtin` の完全な適用）。関数値はラッパーで、その本体も同じ 1 命令になり、`@tz.builtin.Int.wrapping_*` は出ない。H4 の関数値の期待をそのように改める |
| D15: 新しい `kind` を作らない（G23 の登録簿が決める） | G23 の登録簿が実装済みで、すべての診断が `kind` を持ち、`W2001` の行は `"kind":"tool_notice"` で終わる。`E1017` の上限にも `kind` が要る | 登録簿の規則に従って `kind` を 2 つ足す: `W2001` の `integer_overflow_wrapped`（`run --json` の報告の行）と `E1017` の `too_many_overflow_sites`。新しい診断コードは作らない（D15 の本体）。`run --json` の行のキーは G22・G23 の順の `severity`・`code`・`message`・`path`・`span`・`kind`（付録 D.2 に `kind` が加わる）。打ち切りの行は `path`・`span` なしの `tool_notice` |
| 例・fixture の短い接尾辞（`100y`・`3000000000u`） | A22 で edition 2027 が最新になり、マニフェストのない入力・文書の例は短い接尾辞を `E0004` で拒否する | 文書・テスト・fixture の例は型名の接尾辞（`100i8`・`3000000000i32u`）で書く。`int.md` の接尾辞の表が A22 の後も短い接尾辞を無条件に載せていたので、A22 のブランチで直した |
| 末尾呼び出しの引数の地点は `8:93`（`n - 1`）・`8:101`（付録 D.1・E.2。手計算） | 括弧で囲んだ式の span は括弧を含む（ほかの診断と同じ）。`argument.span` は `(n - 1)` の `(` から始まる | 地点の span はコンパイラの式の span のまま（`8:92`・`8:100`）。付録 D.1・E.2 の期待を改める（地点の数・値・どの地点が折り返すかは同じ） |
| A21 の契約は機能として使わない | A21 の `--contracts prove` の SMT 変換は、未知の組み込み関数の呼び出しを「未対応」にする。付録 C.3 で `wrap_add` を `Int.wrapping_add` に直すと証明できなくなる | `prove_smt.rs` に `Int.wrapping_add`・`sub`・`mul` を `bvadd`・`bvsub`・`bvmul`、`wrapping_neg` を `bvneg` として足す（演算子と同じ項） |
| 分類表（A19・A20・G26）は実装済みなら足す | `pure_rules::builtin_effect`・`effects::builtin_facts`・`kg::effects::builtin_effect` がある。`effects` の単体テストが組み込み関数の数（254）と効果なしの数（163）を固定している | 4 つを `IntSaturatingAdd` と同じ「効果なし・トラップなし」に分類し、固定した数を 258・167 にする（`IntWrappingPow` は負の指数でトラップするので、それとは別の行） |
| 付録 D.5: 地点の byte を定数式 `getelementptr inbounds (i8, ptr @tsuzuri_overflow_hits, i64 N)` で書く | `--backend fast` の翻訳器は IR の字句を読む | 地点の byte のアドレスは `getelementptr` の命令で求める（`--backend fast` が読む形。値は同じ） |
| 付録 G.2 の 3: 計装があれば全関数で区間の解析を有効にする | 区間の解析は旗のためだけで、報告しない関数には要らない | `overflow_counted` の関数だけ有効にする（報告しない関数の IR は同じで、解析が減る） |
| WASM の sidecar を `trap` と同じに扱う | ビルドキャッシュの成果物の名前は `artifact`・`traps`・`dwarf`・`pdb`・`profile` の許可表で、1 項目のファイルは 3 個まで | 許可表に `overflow` を足し、上限を 4 個にする（WASM の `traps`・`overflow`・`profile` と `artifact`） |
| メッセージの行と列は演算の式の始まり（仕様 4） | `a + b + c` の内側の `a + b` と外側は始まりが同じで、型と演算も同じなので、2 つの地点のメッセージが同じになる。`run --json` はメッセージから地点を引くので、外側だけが折り返しても内側の `span` を付けうる（PR #67 のレビュー） | メッセージの列は**演算子の位置**（`+`・`-`・`*`・`**`、`-x` の `-`。演算子のない `Add.add a b` は式の始まり）にする。地点ごとに一意になる。`span`（JSON・sidecar）は式全体のまま。地点の表を引くときは、各地点のメッセージを 1 回だけ作る（`overflow::LineIndex`） |
| native の末尾呼び出しの遅延（`tail_arguments`）は `binary_operation` が展開した補助関数の演算も呼び出し側で検査する | 利用者の補助関数（`def plus a b = a + b`）の演算が、呼び出し側の地点と方針（依存パッケージの関数も）で報告される（PR #67 のレビュー） | 報告があるときは `$intrinsic` 以外の補助関数を展開しない（呼び出しのまま。その本体が自分の地点で報告する）。報告のないビルドの IR は変わらない |
| 付録 C.3 の既知の影響は 2 件 | 言語リファレンス・`README.md`・ガイド（`guide/workflow.md` の `Gen.i64()` の `double n`）に、`tsuzuri test` で折り返す例がある | 手順 17 と同じく、例のソースを `Int.wrapping_*` か `Gen.range` に直す（期待値は変えない） |

## 実装者向け早見

- **最初に読むもの**（この順）: `src/llvm_ranges.rs`（全体。157 行）→ `src/llvm.rs` の `FunctionEmitter::binary`・`tail_arguments`・`checked_integer_arithmetic`・`integer_builtin` → `src/llvm_traps.rs` の末尾（`@tz.trap.write` と `@tsuzuri_trap_site`）→ `src/test_runner.rs` の `execute_test`・`run_captured` → 本チケットの「仕様」と「設計」。付録は [A23-appendix.md](A23-appendix.md)。
- **着手前の基準**: `cargo build --release --locked && cargo test --locked`。基準の IR を手順 1 で保存する（Phase 2・3 の各手順で、フラグなしの出力が 1 byte も変わらないことをこれと比べる）。
- **最初に書くテスト**: Phase 1 は `tests/integer_intrinsics.mjs` に `wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg` の 4 つを足す（赤: `E1002 module 'Int' has no function`）。Phase 2 は `tests/overflow_report.rs::unproven_arithmetic_is_instrumented_and_proven_arithmetic_is_not`。Phase 3 は `tests/overflow_report.rs::test_fails_a_test_that_wraps_and_names_the_site`。
- **触ってはいけないもの**: `src/ranges.rs`・`src/ranges_intervals.rs`・`src/interval.rs` の**証明の規則**（`negation_flags` を 1 つ足すだけ）、`TSUZURI_RANGE_FLAGS` の意味、`@checked` の lowering（`src/llvm_exception.rs` の `checked`）、`TrapKind`（新しい種類を足さない）、`Int.checked_*`・`saturating_*`・`abs` の意味、`EmitOptions`（field を足さない。24 個のテストが壊れる。D-45 の G15 と同じ理由で scope を使う。決定事項 D9）。
- **Phase の完了の見分け方**: Phase 1 = `Int.wrapping_add 127i8 1i8` が `-128i8` で、4 つの関数が `-O0`・`-O3`・native・WASM で BigInt の参照と全 10 型（5 幅 × 符号）で一致し、演算子と同じ命令になる。Phase 2 = 付録 E の fixture で、native は付録 D.1 の 7 行を標準エラーに出し、WASM は import が 0 のまま `tsuzuri_overflow_hit` で 7 地点が立ち、フラグなしの出力は基準と 1 byte も違わない。Phase 3 = `tsuzuri test` が折り返したテストを `integer overflow wrapped at ...` で失敗にし、`--allow-overflow` で成功に戻る。既存テストの影響は付録 C のとおりだけ。
- **やってはいけない近道**: 証明の規則を変えて報告を減らす。計装で演算の値を変える（検査は元の演算の**前**に書く）。WASM に import を足す。報告が出ないテストの期待値を書き換えて通す。

## 目的

Tsuzuri の整数の `+`・`-`・`*`・単項 `-`・`**` は、溢れると**黙って折り返す**（`2147483647 + 1` は `-2147483648`）。これは言語の意味で、変えない（`docs/stability.md` の「数値のオーバーフロー」）。困るのは、**書いた人の意図が実行にも道具にも残らない**ことである。

```text
$ cat Main.tz
def mix :: i32u -> i32u -> i32u = \a b -> a + b
Debug.print (to_string (mix 3000000000u 3000000000u))
$ tsuzuri run Main.tz
1705032704
```

在庫の加算なら間違い、ハッシュなら意図で、どちらも出力は同じ。意図を書く API は `Int.wrapping_pow` と、検査して `None` を返す `Int.checked_*`、飽和する `Int.saturating_*`、例外にする `@checked` が揃っているのに、**折り返すことを名前で書く `+`・`-`・`*`・`-x` の版がない**（付録 A の現状: 下の「現状」）。

本チケットが足すもの:

1. **Phase 1（S）**: `Int.wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg`。折り返すことを名前で書く。演算子と同じ命令になる。
2. **Phase 2（L）**: `--overflow-report`。範囲の証明で溢れないと示せない整数の演算に検査を足し、溢れたら**報告して、折り返した値で続ける**。意味は変えない。native は標準エラーへ `warning: integer overflow wrapped at ./Main.tz:12:5 (i32 add)` を 1 地点につき 1 回（最大 100 地点）、WASM は import を足さずに、export した関数で地点を読む。
3. **Phase 3（M）**: `tsuzuri test` は既定で報告を有効にし、**折り返しを報告したテストを失敗にする**（`--allow-overflow` で報告だけ）。意図した折り返しは `Int.wrapping_*` で書く。

HEAD で測った数字（付録 A。測定は破棄した試作による。再現手順は付録 A.1）:

- **静的な lint は成り立たない**: 範囲の証明で溢れないと示せた整数の演算は、`examples/` の利用者のコードで 25 件中 0（0.0%）、`tests/fixtures/` で 3,317 件中 368（11.1%）、言語リファレンスの例で 284 件中 15（5.3%）。演算子ごとの警告は演算の 89〜100%（付録 A.2）に出るので採らない（決定事項 D2）。
- **動的な報告なら、意図した折り返しだけが見つかる**: 言語リファレンスの 561 の例のうち実行時に折り返したのは 2 つで、どちらも `Matrix.fold (\total x -> total * 31 + x)` の意図したハッシュ混ぜ。`tests/fixtures/property` の `pair commutes` と `gen.md` の `addition commutes` は、乱数の整数の足し算で溢れる（付録 C.3）。
- **費用**: 地点が 3 つの 10 億回のループで、`-O3` の user 時間が 0.55 秒から 1.03 秒（約 1.9 倍。1 つの workload・1 台。付録 A.4）。診断用で、配布ビルドには使わない。

非目標: 演算子の意味を変えない（`--overflow-report` は値をビット単位で同じに保つ）。コンパイル時の評価（定数の畳み込み・`const def`）の折り返しは報告しない（D12）。std・依存パッケージ・コンパイラが作った補助関数・SIMD のレーン演算・シフト・`as` の cast・ビット演算は報告しない。検査付きの算術を既定にしない。マニフェストの設定・環境変数・`--no-overflow-report`・新しい診断コード・`TrapKind`・予約語を作らない（D14）。GPU のカーネル（`gpu.rs`）は計装しない。

## 着手条件と停止条件

### 着手条件

- GUIDE §2.3 の基準コマンド（`cargo build --release --locked`・`cargo test --locked`）が成功する状態から始める。失敗があれば、自分の変更と無関係であることを記録する。
- PR02 が `done` であること（`grep -nE "^\| PR02 " _perfs/README.md`）。`src/llvm_ranges.rs` と `src/ranges_intervals.rs` の `integer_flags` が HEAD にあること。
- **Phase の順に**実装し、Phase ごとに独立にマージできる。指示がなければ Phase 1 だけを実装する。Phase 3 は Phase 2 が済んでいること。
- 読むもの: GUIDE の D-45（本チケットは構文・edition を変えない）・D-51（`W2001` の行の形）・D-55、`docs/stability.md`、`docs/architecture.md` の「性能設計の原則」、AGENTS.md。変更前の IR は手順 1 で保存する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と選択肢を報告する（GUIDE §13）。

1. **フラグなしの出力が変わった**: `--overflow-report` を付けない `build`・`run`（`tsuzuri test` を除く）の IR・object・`.wasm` が、手順 1 で保存した基準と 1 byte でも違う（`Int.wrapping_*` を使わないプログラムで）。
2. **値が変わった**: 計装つきと計装なしで、付録 E の fixture や既存の E2E のプログラムの結果が、`-O0`・`-O3`・native・WASM のどれかで違う。選択肢: A（計装の位置を直す。検査は元の演算の前で、演算の値を計装の結果に差し替えない）、B（その演算を計装の対象から外し、理由を決定事項に書く）。
3. **WASM に import が増えた**、または `--overflow-report` の WASM のモジュールが `src/runtime/wasm.ll` なしでリンクを求める（付録 A.5: `__multi3`）。選択肢: A（`wasm.ll` を連結する既存の経路を保つ）、B（その幅・演算だけ手書きの旗にする）。`atomicrmw` を WASM の地点に使う案は採らない（libcall になる）。
4. **証明の規則を変えたくなった**（`ranges.rs`・`interval.rs`）: 報告を減らすための変更は許さない。止めて報告する。
5. **既存のテストの期待値を変えないと通らない**: 付録 C に列挙したもの（`tests/fixtures/property/Props.tz` の 1 テストと `gen.md` の例。Phase 3 だけ）を除く。
6. **`emit_program` の 2 回目の emission**（`Drain::DepthFirst`）で地点の表が 1 回目と食い違い、IR と表が合わない。選択肢: A（`Globals` ごとに表を作り、最後の emission の表を scope へ publish する。付録 G.2 のとおり）、B（止めて報告する）。
7. **報告する地点が 65,536 を超えた**: `E1017`。上限を上げない。
8. **Windows（MSVC）で `@write` の shim（`windows_abi`）が `declare i64 @write(i32, ptr, i64)` を見つけない**（宣言の綴りが違う）。綴りを `src/llvm_traps.rs` と一字一句同じにする。それでも通らなければ停止する。
9. **Phase 3 で、付録 C に無い既存テストが 5 件を超えて失敗した**。選択肢: A（テストのソースの算術を `Int.wrapping_*` に直す。報告の機構は変えない）、B（5 件を超えるなら停止し、既定の変更を保留するかを報告する）。
10. **`Int.wrapping_*` の inline 呼び出し（手順 3）が、`call` の他の分岐（コールバックの特殊化、`borrowed_call`）と衝突し、結果が変わる**。inline を外して `integer_builtin` の本体だけにし（`-O0` では呼び出しが残る）、報告する。
11. 新しい予約語、`unsafe`、新しい crate、新しい診断コード、既定の WASM の import が必要に見える。または、同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `a2313d19` で確認）

### `Int` モジュール

`Int` は組み込みのモジュールで（std の `.tz` はない。`src/stdlib.rs` の `RESERVED_MODULES` に `"Int"`）、公開関数は `src/check.rs` の `Builtin` の 26 項目（`grep -c 'Self::Int[A-Za-z]* => "Int\.' src/check.rs` が 26）。内訳は選択・絶対値 6、ビット 8、`checked_*` 7（`checked_pow` を含む）、`saturating_*` 3、`wrapping_pow` 1、`widening_mul` 1。折り返しを名前で書けるのは `Int.wrapping_pow` だけで、`wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg` は存在しない（実行結果）:

```text
$ cat Main.tz
let a = 2147483647
let d = Int.wrapping_add a 1
Debug.print $"{d}"
$ tsuzuri run .
./Main.tz:2:13: error[E1002]: module 'Int' has no function or union case 'wrapping_add'
  2 | let d = Int.wrapping_add a 1
    |             ^
```

演算子は黙って折り返す（HEAD の実行結果。`def` の中の演算は実行時に評価される）:

```text
def mix :: i32u -> i32u -> i32u = \a b -> a + b          # mix 3000000000u 3000000000u = 1705032704
def sub1 :: i32u -> i32u = \a -> a - 1u                  # sub1 0u = 4294967295
def mul8 :: i8 -> i8 -> i8 = \a b -> a * b               # mul8 100y 100y = 16
def neg32 :: i32 -> i32 = \a -> -a                       # neg32 (-2147483647 - 1) = -2147483648
def big :: i128 -> i128 -> i128 = \a b -> a + b          # big (2^127 - 1) 1 = -(2^127)
def pw :: i32 -> i32 -> i32 = \a b -> a ** b             # pw 3 40 = 689956897
```

トップレベルの `let n = m + 1`（`m` が定数）は、コンパイル時に畳み込まれて黙って折り返し、実行時の演算が残らない（`-2147483648` を得る）。関数の中の演算は実行時に残る。

### 整数の算術を出す箇所は 4 つだけ（付録 B）

利用者が書いた整数の算術から LLVM の `add`・`sub`・`mul` を書く箇所は、`FunctionEmitter::binary`、`tail_arguments`（native の末尾呼び出しの引数。ラッチへ遅らせる `add`・`sub`）、`llvm_exception.rs` の `power`（`**`）、`emit_expression_mode` の `Unary` の腕（`-x`）の 4 つ。`Add.add x y`・`Sub.sub x y`・`Mul.mul x y` の直接呼び出しは、PR02 の D10 で `FunctionEmitter::call` が `binary` を呼ぶので `binary` を通る。`@checked` の中の演算は、型検査が `TypedExprKind::Checked` に包み（`src/exceptions.rs` の `spec_expression`・`checked_kind`）、`FunctionEmitter::checked` が検査つきで書くので、`binary` を通らない。したがって `binary` に計装を足せば「`@checked` の内側でないもの」が自然に満たされ、`Int.wrapping_*` は組み込み関数なので `binary` を通らない。

### PR02 の範囲の証明と `audit` の形

- `src/llvm_ranges.rs` の `RangeFlags`（`Off`・`Proven`・`Audit`。`TSUZURI_RANGE_FLAGS`）。`arithmetic_flags` は `binary` と `tail_arguments` から呼ばれ、`Audit` では**元の演算の前に**検査（`checked_integer_arithmetic` の旗が真なら `TrapKind::Overflow` で trap）を書く。本チケットの計装は、この位置に、trap の代わりに「記録して続ける」を書く。
- 証明は `self.ranges.integer_flags(module, operator, left, right)`（`src/ranges_intervals.rs`）が `Flags { nsw, nuw }` を返す。**その型の意味**で選ぶ: 符号付きは `nsw`、符号なしは `nuw`。`i128` と 64 bit を超える型は常に「証明なし」。
- 区間の解析は `crate::ranges::analyze(module, function, intervals)` の `intervals` が真のときだけ働く。HEAD は `self.globals.range_flags != RangeFlags::Off` のとき。本チケットは「`|| 計装あり`」を足す。付録 A.2 の試作は、この入口をそのまま使って数えた。

### テストの実行と結果の回収

- `tsuzuri test` は**テストごとに別のプロセス**を起動する（`src/test_runner.rs` の `execute_test`: 実行ファイルに添字を渡す）。標準エラーの末尾 64 KiB（`CAPTURED_STDERR`）を `TestResult.output` に持ち、失敗したテストだけ表示する。WASM のテストは `src/runtime/test-runner.mjs`（Node）が 1 テストずつ実行する（`tests.wasm` と添字を受け取る）。
- `tsuzuri run` は `trap_info: true` でビルドし、プログラムの標準エラーを中継する（`--json` のときは終わるまで集めて、成功なら生のまま書く）。`E2005` の位置は、標準エラーの `trap: ... at ./Main.tz:L:C` の行を、ビルドが返す `TrapSite::message` と突き合わせて求める。ツールの警告は `{"severity":"warning","code":"W2001","message":...}`（スパンなし）。
- 診断の行の位置は `./Main.tz:1:29` の形（`Project::with_trap_sources` の `source.path.to_string_lossy()`。実行したときのパスの綴りのまま）。

### WASM

- 既定の WASM は import なし。トラップの位置は `@tsuzuri_trap_site`（IR に書く `define i32 @tsuzuri_trap_site()`）を `wasm-ld --export=tsuzuri_trap_site` で export し、ホストが読む（`src/runtime/trap-boundary.mjs`・`bindings-core.mjs`）。`--trap-info` は `<出力>.trap.json` を隣に書く（`trap_sidecar_path`）。
- import を増やさずに地点を読めることは、手書きの IR で確かめた（付録 A.5）。`i128` の add・sub と `i64` の `smul.with.overflow` は `src/runtime/wasm.ll`（`__multi3` を含む `i128` の libcall の定義。WASM の出力には常に連結される）が要る。

### 既知の影響（Phase 3）

`tsuzuri test` を既定で有効にすると、HEAD の既存の例のうち成功から失敗に変わるのは、測った範囲では 2 つだけ: `tests/fixtures/property/Props.tz` の `pair commutes` と、`gen.md` の `addition commutes`（付録 A.3・C.3）。

## 変えてはいけない意味

| # | 不変にするもの | 確かめ方 |
| --- | --- | --- |
| 1 | `--overflow-report` を付けない `build`・`run` の IR・object・`.wasm` | 手順 1 の基準との**バイト比較**（`Int.wrapping_*` を使わないプログラム。native・wasm32・`-O0`・`-O3`） |
| 2 | 計装つきと計装なしの**実行結果**（値・トラップ・終了コード） | 付録 E の fixture と、`tests/fixtures/control`・`primitives`・`stdlib`・`tasks` の結果の比較 |
| 3 | 整数の `+ - * ** -x` が折り返すこと、`@checked`・`Int.checked_*`・`saturating_*`・`abs`・`widening_mul` の意味 | 既存の `tests/integer_intrinsics.mjs`・`tests/control.mjs` |
| 4 | 範囲の証明の規則と `TSUZURI_RANGE_FLAGS` の意味（`off`・`proven`・`audit`） | `tests/integer_ranges.rs`・`tests/range_flags_audit.mjs` |
| 5 | 診断コードと `TrapKind`（新しいものを足さない）。診断の JSON の既存のキー | `tests/diagnostics.rs` |
| 6 | WASM の import（既定は 0） | `WebAssembly.Module.imports(module)` が `[]` |
| 7 | `driver::run_tests`（ライブラリ API）の既定の挙動: `TestOptions::default()` は報告を有効にしない | `tests/test_runner.rs`・`tests/coverage.rs`・`tests/property.rs` が期待値を変えずに通る |
| 8 | 生成 IR は決定的。intrinsic 宣言は 1 つずつ | `llvm::duplicate_declarations` が空。IR を 2 回出して同じ |

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `Int.wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg`（std の追加。inline 呼び出し）。文書 | S | なし | 追加だけで、既存のプログラムの IR は変わらない。意図を書く手段が先に揃う |
| 2 | `--overflow-report`（`build`・`run`）: 計装、native の標準エラーへの報告、WASM の export と sidecar、`run --json` の変換。文書と E2E | L | Phase 1（文書の移行先として。コードの依存はない） | フラグを付けなければ出力は 1 byte も変わらない。診断用に単独で使える |
| 3 | `tsuzuri test` の既定の報告と失敗、`--allow-overflow`。既存の例・テストの直し。文書 | M | Phase 2 | `test` だけの変更。ライブラリ API の既定は変えず、CLI の既定だけを変える |

## 仕様

### 1. 追加する関数（Phase 1）

| 関数 | シグネチャ | 意味 | 生成する命令（型 `iN`） |
| --- | --- | --- | --- |
| `Int.wrapping_add` | `Integer<'a> => 'a -> 'a -> 'a` | `x + y` を幅で折り返す（常に成功） | `add iN %x, %y` |
| `Int.wrapping_sub` | `Integer<'a> => 'a -> 'a -> 'a` | `x - y` を幅で折り返す | `sub iN %x, %y` |
| `Int.wrapping_mul` | `Integer<'a> => 'a -> 'a -> 'a` | `x * y` を幅で折り返す | `mul iN %x, %y` |
| `Int.wrapping_neg` | `SignedInteger<'a> => 'a -> 'a` | `-x` を折り返す（`MIN` は `MIN`）。符号なしの `-x` が型検査で拒否されるのと同じく、符号付きだけ | `sub iN 0, %x` |

- `'a` は 8・16・32・64・128 bit の符号付き・符号なし（`Integer` の制約）。引数の型をそのまま結果に使う。暗黙の変換はない。部分適用して関数値にできる（`let f = Int.wrapping_add 1i8`）。
- どれも**フラグ（`nsw`・`nuw`）を付けない**。範囲の証明は使わない。トラップしない。確保しない。
- 符号なしの否定は `Int.wrapping_sub 0i32u x` と書く（文書に書く）。
- 拒否（診断コードで照合）: 幅の違う引数 `Int.wrapping_add 5i32 6i8` は `E1003`。浮動小数点 `Int.wrapping_add 5.0 6.0` は `E1005`（`no instance for Integer<f64>`）。符号なしの `Int.wrapping_neg 5i32u` は `E1005`（`no instance for SignedInteger<i32u>`）。引数の数が違うのは `E1003`。
- `Int.wrapping_shl`・`wrapping_shr` は**足さない**（決定事項 D7）。シフトは `<<<`・`>>>` が、量を幅で切って（`bits - 1` との `and`）既に定義された操作で、折り返しの報告の対象でもない。
- **inline**（決定事項 D8）: 完全に適用した呼び出し `Int.wrapping_add x y` は、`@tz.builtin.Int.wrapping_add.i32` を呼ばずに、呼び出し側へ `add i32 %x, %y` を直接書く（PR02 D10 の `Add.add` と同じ。`-O0` でも演算子と同じ命令になる）。関数値として使うとき（部分適用・高階関数の引数）は、`integer_builtin` が書く関数の本体（同じ 1 命令）を使う。

### 2. 報告の対象（Phase 2）

**報告する演算**（地点）は、次の表の「する」だけ。

| 構文・位置 | 型 | 計装 | 地点の span |
| --- | --- | --- | --- |
| `x + y`・`x - y`・`x * y` | 整数 8〜128 bit | **する**（範囲の証明で溢れないと示せないとき） | 式全体（`binary` を呼ぶ時点の `current_span`） |
| `-x` | 符号付き整数 | する（`x == MIN` で報告） | 式全体 |
| `x ** y` | 整数 | する。**地点は `**` の式 1 つ**（二乗法のループの中のどの `mul` が溢れても、同じ地点に報告する）。範囲の証明はない（常に計装） | 式全体 |
| `Add.add x y`・`Sub.sub x y`・`Mul.mul x y` の直接呼び出し | 整数 | する（PR02 D10。`binary` を通る） | 呼び出し式全体 |
| 末尾呼び出しの引数の `n - 1`・`acc + x`（`tail_arguments`） | 整数 | する | **その引数の式の span**（呼び出し全体ではない） |
| `@checked` の中 | | しない（すでに検査されている） | — |
| `Int.wrapping_*`・`checked_*`・`saturating_*`・`widening_mul`・`abs`・`unsigned_abs`・`abs_diff` | | しない（契約が名前に書いてある） | — |
| `<<<`・`>>>`・`&&&`・`\|\|\|`・`^^^`・`~~~`、`as` の cast | | しない（折り返しではなく定義された操作） | — |
| `for` の範囲ループの内部の増分 | | しない（内部。PR02 の `audit_increment` の対象） | — |
| 浮動小数点・decimal・`BigInt`・SIMD のレーン | | しない | — |
| コンパイル時の評価（トップレベルの `let` の畳み込み、`const def`、`const`） | | しない（実行時に存在しない。D12） | — |
| std の関数・依存パッケージの関数・ベンチ・コンパイラが作った補助関数（モジュール名が `$` で始まる関数。`$lambda`・`$task` を除く） | | しない | — |
| 利用者の関数・利用者のテスト・ラムダと `task` が持ち上げた関数 | | **する** | — |

**どの関数を報告するか**（`FunctionEmitter` の `overflow_counted`。本体の emission の前に 1 回だけ決める。`coverage::counted` の述語と同じ形で、**テストを含む**）:

```text
reported(function) =
    function.origin.module == ModuleOrigin::User
    && function.origin.bench.is_none()
    && (!function.module.starts_with('$') || function.module in ["$lambda", "$task"])
    && (function.origin.provenance == Provenance::User || function.origin.parent.is_some())
    && !matches!(function.body.kind, HostCall(..) | DynDispatch { .. })
    && scope.reports(function.span.source)          // その span のソースが「報告する」ソース
```

`scope.reports(source)` は、`Scope` の `sources[source].reported`（driver が決める: `origin == User` かつルートパッケージのソース。依存パッケージ・std は偽）。`function.span.source` が `None`、または scope にないときは偽。特殊化された複本（`$mono`）は、定義した側の `span.source` を持つので、依存パッケージの総称関数をルートが具体化しても報告しない。

**証明されたもの**は計装しない: 整数 `+ - *` は `RangeFacts::integer_flags(...)` の旗を**その型の意味**で読み（符号付きは `nsw`、符号なしは `nuw`）、真なら溢れないので何も書かない。`-x` は新しい `RangeFacts::negation_flags(module, operand)` を同じ意味で読む。`i128` と `**` は常に計装する。

### 3. 地点（サイト）と表

- 地点の同一性: `Site { source, start, end, operation, bits, signed }`（`operation` は `Add`・`Subtract`・`Multiply`・`Negate`・`Power`）。同じ地点を何度 emission しても（`$mono` の複本、`borrowed` の作業関数、`@cpu` の複数の版）、**同じ番号**になる。型が違う（`i32` と `i64` の複本）、演算が違う、span が違う地点は別の番号。
- 番号は **emission で初めて計装した順**（`0` から）。同じソースと同じコンパイラから同じ表ができる（決定的）。`Globals` ごとに表を作り、2 回目の emission（`Drain::DepthFirst`）では作り直す。最後に scope へ publish する。
- 上限 `overflow::MAX_SITES = 65_536`。超えたら `E1017`（`integer overflow report needs more than 65536 sites; build without --overflow-report`、位置なし）。

### 4. 報告の形

- **メッセージ**（英語・小文字始まり・末尾にピリオドなし）: `integer overflow wrapped at {path}:{line}:{column} ({type} {operation})`。`path` は `trap:` の行と同じ綴り（`Project` のソースの `path.to_string_lossy()`）、`line`・`column` は span の開始（`diagnostic::location` の 1 始まり。列は文字数）、`type` は `i8`・`i16`・`i32`・`i64`・`i128`・`i8u`・`i16u`・`i32u`・`i64u`・`i128u`、`operation` は `add`・`sub`・`mul`・`neg`・`pow`。例: `integer overflow wrapped at ./Main.tz:12:5 (i32 add)`。
- **native**: 地点が**初めて**溢れたとき、`warning: ` + メッセージ + `\n` を標準エラーへ 1 回の `write` で書く（定数の文字列 `@tz.overflow.message.N`）。地点ごとの 1 byte の旗を `atomicrmw xchg i8 1 monotonic` で立て、0 → 1 のときだけ書く。100 地点を書いたあと、101 地点目で 1 回だけ `warning: integer overflow reports stop after 100 sites\n` を書き、以後は書かない。終了コード・標準出力は変わらない。
- **WASM**: 書かない（import がない）。地点が溢れたとき、地点ごとの 1 byte に `store atomic i8 1, ... monotonic` する。export は 2 つ: `tsuzuri_overflow_count() -> i32`（地点の数）と `tsuzuri_overflow_hit(i32) -> i32`（地点 `i` が溢れたなら 1、範囲外の `i` は 0）。ホストは任意の時点で読める。地点の説明は sidecar `<出力>.overflow.json`（`trap_sidecar_path` と同じ置き場。付録 D.4）。旗は 1 度立つと下ろさない（新しいインスタンスで 0 に戻る）。
- **`tsuzuri run --json`**: プログラムが成功で終わったあと、標準エラーのうち `warning: integer overflow wrapped at ` で始まる行を、1 行ずつ付録 D.2 の JSON（コードは `W2001`。D10）に変えて `tsuzuri` の標準エラーへ書く（ほかの行はそのまま）。`path`・`span` は地点の表とメッセージの完全一致で照合して付ける（`E2005` の位置を求める方法と同じ）。`warning: integer overflow reports stop after 100 sites` の行は `path`・`span` なしの `W2001`。失敗（`E2005`）のときは変換しない（`message` の中の生の標準エラーに残る）。`--json` でない `run` は標準エラーをそのまま中継する。

### 5. CLI（Phase 2・3）

| 入力 | 結果 |
| --- | --- |
| `tsuzuri build X --overflow-report [...]` | 計装する。`-O0`〜`-O3`・native・wasm32・wasm64・`--emit exe|object|llvm|shared|wasm` |
| `tsuzuri run X --overflow-report` | 計装し、native の報告が標準エラーに出る |
| `tsuzuri test X` | **既定で計装し、折り返したテストを失敗にする**（Phase 3）。`--overflow-report` は冗長だが受理する |
| `tsuzuri test X --allow-overflow` | 計装し、折り返したテストを成功のまま、警告を出す（Phase 3） |
| `--overflow-report` を `check`・`doc`・`fmt`・`bench` に（`parse_arguments` の `Action`） | `E2000`: `--overflow-report is only valid with build, run, or test` |
| `--overflow-report` を `lsp`・`script`・`repl`・`api-diff`・`bindgen`・`new`・`fetch`・`publish` に | 各コマンドの既存のオプション検査が拒否する（本チケットは変えない。`lsp` は `lsp takes no paths or build options`、`script` は `SCRIPT_OPTIONS`） |
| `--allow-overflow` を `test` 以外に | `E2000`: `--allow-overflow is only valid with test` |
| `--allow-overflow` を `test --index N -g -o PATH`（デバッグ用のランナー）に | `E2000`: `--allow-overflow cannot be combined with -g; a debug runner only builds` |
| `--overflow-report --freestanding` | `E2000`: `--overflow-report cannot be combined with --freestanding: the report is written through the C library` |
| `--overflow-report --emit header` / カーネル（`wgsl`・`spirv` 系）/ バインディング（`bindings-*`） | `E2000`: `--overflow-report is not valid for header, kernel, or bindings output` |
| 同じフラグを 2 回 | `E2000`: `overflow-report specified more than once`（`--allow-overflow` も同様に `allow-overflow specified more than once`） |

`tsuzuri test` が既定で計装するのは CLI の規則であり、ライブラリ API の `driver::run_tests` の `TestOptions::default()` は**計装しない**（既存のテストが期待を変えずに通るため）。`--allow-overflow` は D-55 に無い新しい名前（台帳へ追記が必要）。

### 6. `tsuzuri test` の規則（Phase 3）

1. 各テストの実行（native は実行ファイル、WASM は Node のランナー）の標準エラーを、**折り返しの行**（`warning: integer overflow wrapped at ` で始まる行）で走査する（`run_captured` が 64 KiB の末尾とは別に、その行を最大 101 行（100 地点 + 打ち切りの行）集める。長い出力で折り返しの行が末尾から落ちても、テストの合否が変わらないようにする）。
2. テストの終了が成功（終了コード 0）で、折り返しの行が 1 つ以上あり、`--allow-overflow` がないとき、テストを**失敗**にする。`failure` は最初の行から `warning: ` を除いたメッセージ（例: `integer overflow wrapped at ./Main.tz:12:5 (i32 mul)`）。2 つ以上あっても `failure` は 1 つめだけで、すべての行は `output`（標準エラーの末尾）に残る。
3. テストがほかの理由で失敗した（トラップ・`assert`・タイムアウト）ときは、`failure` をそのままにする。
4. `--allow-overflow` のとき、折り返しの行があっても成功のまま。テキストの出力は `ok` の行のあとに `  warning: ` + メッセージを 1 行ずつ出す。
5. JSON の 1 行（`{"type":"test",...}`）に、折り返しの行があるテストだけ `"overflow":[メッセージ, ...]`（`warning: ` なし）を足す。キーの位置は `failure`・`output` のあと、`duration_ms` の前（付録 D.3）。既存のキーは変えない。
6. 概要の行 `N passed; M failed; K ignored`、終了コード、`E2006` は、失敗に数えたテストを含めて既存のとおり。
7. `tsuzuri test --coverage` は、失敗に数えたテストのカウンターを使わない（`merge_coverage` の既存の規則: 失敗したテストは数えない）。
8. `tsuzuri test --index N -g -o PATH`（デバッグ用のランナー）と `tsuzuri bench` は計装しない。

### 7. 資源上限と決定性

- 地点 65,536（`E1017`）。メッセージ 1 つは `path` の長さに比例（1 行 4,096 byte を超えるパスは `path` を省略せず、そのまま。`write` は 1 回で書ききれなければループ）。
- 生成 IR は決定的。地点の番号・メッセージ・`@tsuzuri_overflow_hits` の大きさは入力だけで決まる。`HashMap` の反復順に依存しない（`BTreeMap`）。
- 計装を足す解析は、各式の `RangeFacts` の既存の資源上限（`STEP_BUDGET`・`NODE_BUDGET`）の下で動く。上限を超えて証明が失われたら、計装が増えるだけで、意味は変わらない（PR02 の「証明がないことは旗を失う」と同じ向き）。

### 8. 互換性（`docs/stability.md`）

- std に関数を足すのは互換（既知の例外の「std へのモジュールの追加」ではなく、既存モジュールへの関数の追加）。edition の差はない（全 edition に同じ）。
- CLI のフラグ・WASM の export・sidecar の追加は互換。`tsuzuri run --json` の標準エラーの行が、`warning: integer overflow wrapped ...` を JSON にするのは、**フラグを付けた場合だけ**。`--json` の既存のキーは変えない（足すだけ）。
- IR・内部シンボル（`@tz.overflow.*`・`@tsuzuri_overflow_hits`）は保証の対象外。WASM の export 名 `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` と、sidecar のキーは、`tsuzuri_trap_site`・`.trap.json` と同じ扱い（`tsuzuri_` で始まるシンボルは公開 ABI。`docs/stability.md` の表）。

## 設計

### 新規ファイル

| ファイル | 内容 |
| --- | --- |
| `src/overflow.rs` | `Operation`・`Site`・`Source`・`Scope`・メッセージと sidecar の生成。LLVM に依存しない（テストが LLVM なしで書ける） |
| `src/llvm_overflow.rs` | `Report`（emission ごとの地点の表）と `FunctionEmitter::overflow_check`・実行時の IR の文字列（`#[path = "llvm_overflow.rs"] mod overflow_emit;` を `src/llvm.rs` に足す。`use super::*;`） |

`src/lib.rs` に `pub mod overflow;` を足す（`pub mod target;` の隣）。

### Rust の骨格は付録 G

フィールド名・型・シグネチャは [付録 G](A23-appendix.md#付録-g-設計の骨格rust名前と型はこのとおり) にある。実装者はそれをそのまま写して始める。

| 付録 | 内容 |
| --- | --- |
| G.1 | `src/overflow.rs`: `Operation`・`Site`・`Source`・`Scope`・`side_table`・`report_lines`、定数 `MAX_SITES`（65,536）・`MAX_PRINTED`（100）・`REPORT_PREFIX`・`STOPPED_LINE` |
| G.2 | `src/llvm.rs` の `Globals`・`FunctionEmitter::emit`・`emit_program_in` への接続（6 つ） |
| G.3 | `src/llvm_overflow.rs` の `Report`（`covers`・`number`・`runtime`・`publish`）と `overflow_check` の宣言 |
| G.4 | `RangeFacts::negation_flags`（`src/ranges_intervals.rs`） |
| G.5 | 実行時の IR（`Report::runtime`。native と WASM） |
| G.6 | driver（`src/driver.rs`）の変更 1〜7 |

### `overflow_check`（`src/llvm_overflow.rs`。`Report` の型は付録 G.3）

`FunctionEmitter::overflow_check(&mut self, operation: Operation, ty: &Type, span: Span, proven: bool, left: &str, right: &str)` は、検査を元の演算の**前**に書く。手順（番号つき）:

1. `proven`、または `!self.overflow_counted`、または `ty` が整数でない、または `span.source` が `None` なら、何も書かずに戻る。
2. `let site = Site { source, start: span.start, end: span.end, operation, bits, signed }`。`report.number(site)` で番号 `index` を得る（`None` は上限超過: `exceeded = true` にして戻る。`runtime` が `E1017` を返す）。
3. 算術の名前: `Add`→`add`、`Subtract`→`sub`、`Multiply`・`Power`→`mul`、`Negate`→`sub`（左を `"0"`、右を `left` の値にする。`Negate` は `left` に被演算子を渡す）。`let (_, overflow) = self.checked_integer_arithmetic(bits, signed, name, a, b);`（値は使わない）。
4. `self.intrinsics.insert("declare i1 @llvm.expect.i1(i1, i1)".into())`。`let likely = self.value(format!("call i1 @llvm.expect.i1(i1 {overflow}, i1 false)"));`。
5. `wrapped = self.label()`・`done = self.label()`。`self.branch(&likely, &wrapped, &done); self.begin(&wrapped);`
6. **WASM**（`self.globals.wasm`）: `self.instruction(format!("store atomic i8 1, ptr getelementptr inbounds (i8, ptr @tsuzuri_overflow_hits, i64 {index}) monotonic, align 1"))`。
7. **native**: `let old = self.value(format!("atomicrmw xchg ptr getelementptr inbounds (i8, ptr @tsuzuri_overflow_hits, i64 {index}), i8 1 monotonic"));`、`let fresh = self.value(format!("icmp eq i8 {old}, 0"));`、`report = self.label()`、`self.branch(&fresh, &report, &done); self.begin(&report);`、`self.instruction(format!("call void @tz.overflow.report(ptr @tz.overflow.message.{index}, i64 {length})"))`（`length` はメッセージ全体（`warning: ` と `\n` を含む）の byte 数。`Report` が `Site::message` から決めて覚える）。
8. `self.jump(&done); self.begin(&done);`

**呼び出し点**（4 つ。付録 B）:

| 位置 | 呼び出し |
| --- | --- |
| `FunctionEmitter::binary`、`let words = if matches!(operator, Add \| Subtract \| Multiply) && !left.ty.is_float() {` の**直前** | 整数の `Add`・`Subtract`・`Multiply`: `let proven = self.ranges_proven(operator, left, right);` → `self.overflow_check(op, &left.ty, self.current_span, proven, &lhs, &rhs);`。`ranges_proven` は `integer_flags` の旗をその型の意味で読む補助関数（`overflow_check` と同じファイル） |
| `FunctionEmitter::binary`、`if operator == Power { return self.power(...) }` の**直前** | `Power`（整数のみ。浮動小数点は呼ばない）: `self.overflow_check(Operation::Power, ...)` ではなく、**`power` の中**に入れる（二乗法の `mul` ごとに旗を求めるため）。次の段落 |
| `FunctionEmitter::tail_arguments`、`let words = self.arithmetic_flags(...)` の**直前** | `self.current_span` を `argument.span` に一時的に差し替えて同じ呼び出し（呼び出し後に戻す） |
| `FunctionEmitter::emit_expression_mode` の `TypedExprKind::Unary(operator, operand)` の腕、`UnaryOp::Negate` で `operand.ty` が符号付き整数のとき、`self.value(match operator {...})` の**直前** | `proven` は `self.ranges.negation_flags(self.module, operand)` の `nsw` |

`power`（`src/llvm_exception.rs`）の変更: `checked == false` の `multiplied` の腕（`emitter.value(format!("mul {llvm} {left}, {right}"))`）の**前**に、`emitter.overflow_check(Operation::Power, ty, span, false, left, right)`（`span` は `power` を呼んだ時点の `self.current_span` を関数の先頭で `let span = self.current_span;` と取る）。`checked == true`（`@checked`）の腕には書かない。

`negation_flags` は付録 G.4、実行時の IR は G.5（付録 D.5 が出力の抜粋）、driver の変更は G.6。driver は `BuildOptions.overflow_report`・`validate` の `E2000`・`Project::overflow_sources`・`build_complete` の `Scope`・WASM の sidecar と `--export`・`run_process` の変換の 7 つ。

### 前提とする他チケットのインターフェース

| チケット | 使う名前・形 | 未実装のときの代替 |
| --- | --- | --- |
| [G22](G22-diagnostic-v2.md)（D-51） | JSON の 1 行のキーの順序 `severity`・`code`・`message`・`path`・`span`（`path`・`span` は足すだけで互換）。`kind` は付けない（G23 の登録簿が決める） | G22 がなくても同じ形で出せる（既存の 5 キーの流儀） |
| [A19](A19-pure-functions.md)・[A20](A20-effect-summary-and-checked-attributes.md)（D-53）・[G26](G26-facts-model.md) | 機能としては使わない。`trap` 効果にも `trap_sites` にも折り返しは入らない（`Int.wrapping_*` は `alloc`・`trap` を持たない純粋な演算）。ただし分類表（`builtin_effect`・`builtin_facts`）が実装済みなら、手順 3 の「分類表」で 4 つを足す | 表がなければ足すものはない |
| PR02（done） | `RangeFacts::integer_flags`・`Flags`・`RangeFlags` | — |

## 実装手順

各手順の最後で `cargo build --release --locked` と `cargo test --locked` が緑であること。手順を飛ばさない。

### Phase 1

#### 手順 1: ベースライン

- 変更: なし。
- 内容: `cargo build --release --locked && cargo test --locked` の件数と、`node tests/integer_intrinsics.mjs target/release/tsuzuri` が出す `BigInt cases` の件数を控える。基準のコンパイラを `mkdir -p target/tz-A23/before && cp target/release/tsuzuri target/tz-A23/tsuzuri-before` で保存する。次の 14 個の入力を `target/tz-A23/tsuzuri-before build <入力> --emit llvm -O0 -o target/tz-A23/before/<名前>.ll` と、`--target wasm32` を付けた同じコマンド（`<名前>.wasm32.ll`）で出して保存する: `examples/` の `computations`・`control`・`currying`・`functional`・`hello`・`io`・`point`・`polymorphism`・`tasks`（残りの `desktop`・`gpu`・`native`・`web` は `--emit llvm` で build できない）、`tests/fixtures/control`・`primitives`・`stdlib`・`tasks`、付録 E.1 の `Main.tz` を `target/tz-A23/fixture/Main.tz` に保存したもの。付録 A.5 の手書き IR の確認を HEAD の `clang`・`wasm-ld` で 1 回再現する。`grep -rn "Int.wrapping_add\|wrapping_neg" tests _tsuzuri docs std src` が 0 件であることを確かめる。
- 確認: すべて通る。基準の `.ll` が 28 個できている。`grep` は 0 件。

#### 手順 2: 失敗するテストを先に書く

- 変更: `tests/integer_intrinsics.mjs`、`tests/integer_wrapping.rs`（新規）。
- 内容: `tests/integer_intrinsics.mjs` の `operations` に `"wrapping_add"`・`"wrapping_sub"`・`"wrapping_mul"`・`"wrapping_neg"` を足し、`reference` に `case "wrapping_add": return wrap(left + right); case "wrapping_sub": return wrap(left - right); case "wrapping_mul": return wrap(left * right); case "wrapping_neg": return wrap(-left);` を足す。`unary` の配列に `"wrapping_neg"` を足し、符号なしを除く条件（`["abs", "unsigned_abs", "checked_neg"].includes(operation)`）に `"wrapping_neg"` を足す。`tests/integer_wrapping.rs`（LLVM 不要。`tests/integers.rs` の `integer_builtin_signatures_and_constraints` の形: 受理は `analyze(source).unwrap()`、拒否は `analyze(source).expect_err(source)` の `error.code` を照合）に次を書く（ソースは本文の仕様の拒否の例）:
  - `wrapping_functions_exist_for_every_width_and_sign`（10 型 × 4 関数を受理。`Int.wrapping_neg` は符号付き 5 型）
  - `wrapping_neg_rejects_unsigned_integers`（`E1005`）
  - `wrapping_functions_reject_mixed_float_and_wrong_arity_arguments`（`E1003`・`E1005`）
  - `wrapping_calls_are_inline_instructions`（`export def f :: i32 -> i32 -> i32 = \a b -> Int.wrapping_add a b` の `@tz.fn.Main.f` の本体に `add i32 ` があり、`@tz.builtin.Int.wrapping_add` の呼び出しがなく、` nsw`・` nuw` がない。`sub`・`mul`・`neg` も同様。関数値として使うプログラム `let g = Int.wrapping_add 1i32` の IR には `define internal i32 @tz.builtin.Int.wrapping_add.i32` がある）
- 確認: `cargo test --locked --test integer_wrapping` は `running 4 tests` で 4 件失敗（`E1002`）。`node tests/integer_intrinsics.mjs target/release/tsuzuri` は失敗する。

#### 手順 3: `Int.wrapping_*` の実装

- 変更: `src/check.rs`、`src/llvm.rs`。
- 内容: ① `src/check.rs` の `enum Builtin` の `IntWrappingPow,` の直後に `IntWrappingAdd,`・`IntWrappingSub,`・`IntWrappingMul,`・`IntWrappingNeg,`。② `Builtin::ALL`（`&'static [Self]`）の `Self::IntWrappingPow,` の直後に同じ 4 つ。③ `Builtin::name` に `Self::IntWrappingAdd => "Int.wrapping_add"`・`Self::IntWrappingSub => "Int.wrapping_sub"`・`Self::IntWrappingMul => "Int.wrapping_mul"`・`Self::IntWrappingNeg => "Int.wrapping_neg"` の 4 行。④ `Builtin::scheme`（`Self::IntMin | Self::IntMax | ... | Self::IntWideningMul => {` の or パターン）に 4 つを足し、`parameters` の 1 引数の腕（`Self::IntAbs | ... | Self::IntCheckedNeg => vec![a()]`）に `Self::IntWrappingNeg` を、`class` の `SignedInteger` の腕に `Self::IntWrappingNeg` を足す（結果の型は既定の `_ => a()`）。⑤ `src/llvm.rs` の `integer_builtin` に腕を足す: `IntWrappingAdd | IntWrappingSub | IntWrappingMul` は `self.value(format!("{instruction} {ty} %arg0, %arg1"))`（`instruction` は `add`・`sub`・`mul`）、`IntWrappingNeg` は `self.value(format!("sub {ty} 0, %arg0"))`。⑥ `FunctionEmitter::call` の、`Builtin::DynOf`・`Builtin::FixedArrayInit` の特殊な呼び出しの直後に、`Int.wrapping_*` の完全な適用を inline する分岐を足す: `callee` が `TypedExprKind::Function(FunctionRef::Builtin(instance))` で `instance.builtin` が上の 4 つのどれかで、引数の数が（`neg` は 1、ほかは 2）なら、`let lhs = self.expression(&arguments[0]);`（2 引数なら `rhs` も。左から右）として、`add`・`sub`・`mul`・`sub 0` の 1 命令を `self.value(...)` で書いて返す。型は `self.ty(&arguments[0].ty)`。`@tz.builtin.Int.wrapping_*` の関数は、関数値として使われたときだけ `Builtins` に登録される（`call` の既存の経路。inline したときは登録しない）。
- 分類表: `grep -rn "IntWrappingPow" src` の全ヒットのうち上で触れていない腕（A19 の `pure_rules::builtin_effect`・A20 の `effects::builtin_facts`・G26 の分類表が実装済みならその腕）にも、4 つを `IntWrappingPow` と同じ分類（純粋・効果なし）で足す。wildcard なしの `match` なので、足さないとコンパイルが通らない。A19 の単体テストの純粋の個数は 4 増える。
- 確認: `cargo test --locked --test integer_wrapping` が `running 4 tests` で 4 件通る。`cargo build --release --locked && node tests/integer_intrinsics.mjs target/release/tsuzuri` が通り、`integer intrinsics O0: N BigInt cases` の `N` が HEAD の 263182 より増える（`O3` も同じ値。増えた値は実行して控え、手計算しない）。手順 1 の基準と、`Int.wrapping_*` を使わないプログラムの IR が 1 byte も違わない。
- 停止条件 10 に注意。

#### 手順 4: 文書（Phase 1）

- 変更: 「ドキュメント」の表の Phase 1 の行（`int.md`・`docs/language.md`・`docs/architecture.md`）。
- 内容: 表のとおりに更新する。`int.md` の例は付録 F。
- 確認: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/int.md` が通る。`cargo test --locked` が通る。`grep -rn "wrapping_shl\|wrapping_shr" _tsuzuri docs README.md` が 0 件（実装しない関数を書かない）。

### Phase 2

#### 手順 5: テスト先行

- 変更: `tests/overflow_report.rs`（新規）、`src/overflow.rs`（骨格だけ）、`src/lib.rs`。
- 内容: `src/lib.rs` に `pub mod overflow;`。`src/overflow.rs` に型と `todo!()` の本体を置き（コンパイルが通る最小。付録 G.1）、`tests/overflow_report.rs` に付録 H の H5〜H14（10 件。名前・入力・期待値はそこ。補助関数 `emit` も付録 H の冒頭）を書く。`src/llvm_overflow.rs` の単体テスト `number_stops_at_the_limit`（H27）は手順 7 で足す。
- 確認: `cargo test --locked --test overflow_report` は `running 10 tests` で全件失敗（`todo!()`）。`cargo test --locked` のほかのテストは通る。

#### 手順 6: `src/overflow.rs`

- 変更: `src/overflow.rs`。
- 内容: 付録 G.1 の骨格を実装する。`message` は `location(&source.text, self.start)`（行・列）と `source.path`。`side_table` は付録 G.1 のキーの順。`report_lines`（`warning: integer overflow wrapped at ` で始まる行を、`REPORT_PREFIX` を除いて返す）と `json_lines`（`run --json` 用。仕様 4）を `src/overflow.rs` に置く（`driver` から呼ぶ。LLVM 不要でテストできる）。単体テスト（H26）: `message_uses_character_columns`・`side_table_has_the_fixed_key_order`・`json_lines_matches_messages_and_leaves_other_lines`・`scope_restores_the_previous_scope`。
- 確認: `cargo test --locked overflow::` が `running 4 tests` で通る。

#### 手順 7: 計装（`overflow_check`）

- 変更: `src/llvm_overflow.rs`（新規）、`src/llvm.rs`、`src/llvm_exception.rs`、`src/ranges_intervals.rs`。
- 内容: 本文の `overflow_check` と呼び出し点、付録 G.2〜G.4 のとおり。順に: ① `negation_flags` を足す（G.4）。② `src/llvm_overflow.rs` を作り、`Report`・`overflow_check`・`ranges_proven` を実装する（G.3）。`Report::runtime` は手順 8 まで空の文字列を返す。③ `Globals.overflow`・`FunctionEmitter.overflow_counted` と 5 つの接続を足す（G.2 の 1〜4）。④ 4 つの呼び出し点に `overflow_check` を足す。⑤ 単体テスト `number_stops_at_the_limit`（H27）を足す。
- 確認: `cargo test --locked --test overflow_report` の H5〜H10 が通る（地点の数と intrinsic。`runtime` に依る H11〜H13 の一部は手順 8）。`cargo test --locked` が通る。手順 1 の基準 IR（28 個の `.ll`）との比較: `--overflow-report` なし（scope なし）の IR が 1 byte も変わらない。

#### 手順 8: 実行時の IR・publish・`E1017`

- 変更: `src/llvm_overflow.rs`、`src/llvm.rs`。
- 内容: `Report::runtime(wasm)` を付録 G.5 と D.5 のとおりに実装する。`emit_program_in` で `runtime` を `output` へ足し、`publish` し、native で `declare i64 @write(...)` を足す（G.2 の 5・6）。`exceeded` のとき `E1017`。
- 確認: `cargo test --locked --test overflow_report` が `running 10 tests` で全件通り、単体テスト `number_stops_at_the_limit` が通る。`cargo test --locked` が通る。付録 E の fixture を `clang` と `wasm-ld` で手動で動かし、付録 D.1 の 7 行と WASM の `hit` を確かめる（まだ CLI はない。テストは `Scope` を直接使う）。

#### 手順 9: driver

- 変更: `src/driver.rs`。
- 内容: 付録 G.6 の 1〜7。`validate` の `E2000`。`Project::overflow_sources`。`build_complete` の scope・戻り値・sidecar・WASM の `--export`。`run_process` の変換。
- 確認: `cargo test --locked` が通る（`BuildOptions` を全 field で書く箇所は `overflow_report: false` を足してある）。`driver` の単体テストを足す: `validate_rejects_the_overflow_report_with_freestanding_and_header_output`・`overflow_sidecar_path_follows_the_trap_sidecar`。

#### 手順 10: CLI

- 変更: `src/main.rs`。
- 内容: `parse_arguments` に `--overflow-report`（`overflow_report` の bool。重複は `overflow-report specified more than once`）を足し、ループのあとの検査の列（`--trap-info` と同じ形）に `if overflow_report && !matches!(action, Action::Build | Action::Run | Action::Test) { return Err("--overflow-report is only valid with build, run, or test".into()); }` を足す。`BuildOptions { overflow_report, .. }`。`README.md` の CLI の表に行を足す。`--allow-overflow` は Phase 3。
- 確認: `src/main.rs` のテスト `rejects_ambiguous_or_unused_arguments` に、`check`・`fmt`・`doc`・`bench` での拒否、`--freestanding`・`--emit header`・`--emit bindings-js`・`--emit wgsl` との組み合わせの拒否、重複の拒否、`build`・`run`・`test` での受理を足す。`cargo test --locked --bin tsuzuri` が通る。

#### 手順 11: `run --json`

- 変更: `src/driver.rs`（`run_process`）。
- 内容: 仕様 4 のとおり。成功したとき、`--json` の標準エラーを `overflow::json_lines(...)` で変換して書く。
- 確認: `tests/overflow_report.rs` に `run_json_turns_report_lines_into_warning_objects`（付録 D.2 の形。`W2001`・`path`・`span` の 6 キー）・`run_without_json_relays_the_report_unchanged` を足して通る。

#### 手順 12: E2E

- 変更: `tests/overflow_report.mjs`（新規）、`tests/fixtures/overflow_report/Main.tz`・`host.c`（新規。付録 E）、`README.md`・`docs/architecture.md` の検証コマンド一覧。
- 内容: `tests/overflow_report.mjs` は、fixture を `-O0`・`-O3` で検査する。① native: `--overflow-report` なし／ありの `--emit llvm` を `clang` でホストとリンクして実行し、標準出力が付録 E.3 と同じ、標準エラーが空（なし）／付録 D.1 の 7 行（あり）。② wasm32: `--overflow-report` で build し、`imports` が `[]`、`tsuzuri_overflow_count() == 8`、sidecar の `sites` が 8 要素（付録 E.2 のとおり）、折り返す入力のあとの `tsuzuri_overflow_hit` が 7 地点で 1・`8:93`（`n - 1`）で 0、折り返さない入力だけの新しいインスタンスは全地点 0、値は計装なしの wasm と同じ。③ `--overflow-report` なしの wasm に `tsuzuri_overflow_count` がない。
- 確認: `node tests/overflow_report.mjs target/release/tsuzuri` が通る（出力に `-O0`・`-O3` と件数を出す）。`cargo build --release --locked` の直後に実行する。

#### 手順 13: 文書（Phase 2）

- 変更: 「ドキュメント」の表の Phase 2 の行。
- 内容: 表のとおりに更新する。
- 確認: 変更した各ページで `node scripts/check-docs.mjs <ページ>` が通る。`cargo test --locked` が通る。

### Phase 3

#### 手順 14: `TestOptions`・`TestResult` と判定

- 変更: `src/test_runner.rs`、`src/driver.rs`（`pub use`）。
- 内容: `pub struct OverflowTests { pub fail: bool, pub sources: Vec<overflow::Source> }`（`Clone, Debug`）と `TestOptions.overflow: Option<OverflowTests>`（既定 `None`）。`TestResult` に `pub overflow: Vec<String>` を足す（`TestResult` を組み立てる箇所は `run_with_timeout` の 1 か所と、`grep -n "TestResult {" src tests` の一覧）。`run_captured` の `Captured` に `overflow: Vec<String>` を足し、`read_tail` が読みながら行頭が `REPORT_PREFIX` または `STOPPED_LINE` の行を最大 101 行集める（1 行 4,096 byte まで）。`execute_test`: 終了が成功で、`overflow` が空でなく、`options.overflow` の `fail` が真なら `failure = Some(最初の行から "warning: " を除いたもの)`。`TestResult.output` は失敗したテストだけ（既存）。`overflow` の行は `TestResult.overflow` に、`warning: ` を除いて入れる。`build_runner`: `options.overflow` が `Some` なら、emission の前に `overflow::Scope::enter(sources.clone())`（`build_runner` の中で持つ）。
- 確認: `tests/overflow_report.rs` に付録 H の H17〜H23 を足して通す（`Gen.for_all` の `a + b` が `i32` で溢れる例、折り返したあとに 100 KB を標準エラーへ書く例を含む）。`cargo test --locked` が通る。

#### 手順 15: WASM のランナー

- 変更: `src/test_runner.rs`、`src/runtime/test-runner.mjs`。
- 内容: WASM の `build_runner` で、`options.overflow` が `Some` のとき、`overflow::side_table(&scope.sites(), &sources)` を `<temporary>/overflow.json` へ書き、`wasm-ld` に `--export=tsuzuri_overflow_count`・`--export=tsuzuri_overflow_hit` を足し、`Runner.arguments` に JSON のパスを `tests.wasm` の次へ足す（`node script.mjs tests.wasm overflow.json <index>`）。`test-runner.mjs`: `process.argv.length` が 4（表なし）または 5（表あり）を受け付け、表があり、`tsuzuri_test_run` が 0 を返したあと、`tsuzuri_overflow_count()` 回 `tsuzuri_overflow_hit(i)` を読み、1 の地点ごとに `warning: ` + `sites[i].message` + `\n` を標準エラーへ書く（地点の番号の順）。100 地点を超える分は書かず、`warning: integer overflow reports stop after 100 sites` を 1 回書く。表がなければ何もしない（既存の呼び出しと同じ）。
- 確認: `tests/overflow_report.rs` に `wasm_tests_report_overflow_like_native_tests`（`--target wasm32` の `tsuzuri test`。native と同じ `failure` の文字列）を足して通る。`tests/test_runner.rs` の既存のテストが期待を変えずに通る。

#### 手順 16: CLI と出力

- 変更: `src/main.rs`。
- 内容: `--allow-overflow`（重複は `allow-overflow specified more than once`）。`test` 以外で `E2000`（仕様 5）、`-g`・`-o` のデバッグ用のランナーと同時で `E2000`。`run_test_action` は `TestOptions { overflow: Some(OverflowTests { fail: !arguments.allow_overflow, sources: project.overflow_sources() }), .. }`。出力: 失敗は既存の経路（`failure:` と標準エラー）。成功で `overflow` が空でないときは `ok` の行のあとに `  warning: ` + メッセージ。JSON の 1 行に `"overflow":[...]`（仕様 6 の 5）。`README.md` の CLI の表に `--allow-overflow`。
- 確認: `src/main.rs` の引数のテストに `--allow-overflow` の拒否と受理を足す。`tests/overflow_report.rs` の CLI のテスト（`env!("CARGO_BIN_EXE_tsuzuri")`）に `test_command_text_and_json_show_the_report` を足して通る（付録 D.3 の形）。

#### 手順 17: 既存のテスト・文書の影響を直す

- 変更: `tests/fixtures/property/Props.tz`、`_tsuzuri/language-reference/built-in-types-and-modules/gen.md`・`matrix.md`・`matrix-view.md`、手順のなかで見つかったテスト。
- 内容: 付録 C.3 のとおり。まず `cargo test --locked` を実行し、折り返しで失敗したテストを列挙する（`failure: integer overflow wrapped`）。**テストのソースの算術を `Int.wrapping_*` に直す**。期待値・報告の機構・`--allow-overflow` の使用で通さない。`Props.tz` の `pair commutes` は `Int.wrapping_add a b == Int.wrapping_add b a`。`gen.md` の `addition commutes` も同じに直し、「乱数の整数どうしの足し算は溢れるので、折り返すことを `Int.wrapping_add` で書く」を 1 文足す。`matrix.md`・`matrix-view.md` の `\total x -> total * 31 + x` は `Int.wrapping_add (Int.wrapping_mul total 31) x` に書き換える（決定事項 D13。`run=` の期待値は変わらない）。付録 C.3 に無いテストが 5 件を超えたら停止条件 9。A21 が済んでいる木では、`tests/fixtures/contracts/Main.tz` の `wrap_add` も付録 C.3 の表のとおりに `Int.wrapping_add` へ直す（付録 C.3 にあるので 5 件に数えない）。
- 確認: `cargo test --locked` が緑。`node scripts/check-docs.mjs` を変更したページ（と、`grep -rln '^test "' _tsuzuri/language-reference` の 5 ページ）で通す。`node tests/examples.mjs target/release/tsuzuri` が通る。

#### 手順 18: 文書と最終確認

- 変更: 「ドキュメント」の表の Phase 3 の行。
- 内容: 表のとおりに更新する。
- 確認: 上の各ページで `node scripts/check-docs.mjs`。`cargo build --release --locked && cargo test --locked && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`。native と WASM の `-O0`・`-O3` の E2E: `node tests/overflow_report.mjs target/release/tsuzuri`・`node tests/integer_intrinsics.mjs target/release/tsuzuri`・`node tests/features.mjs target/release/tsuzuri`。`TSUZURI_RANGE_FLAGS=audit node tests/overflow_report.mjs target/release/tsuzuri`（`audit` と同時に使っても値が変わらない）。完了報告を書く（GUIDE §13.2）。

## テスト計画

### Rust テスト（名前・入力・期待値の全表は [付録 H](A23-appendix.md#付録-h-rust-テストの一覧名前入力期待値)）

| Phase | ファイル | 件数 | 付録 H の行 | 書く手順 |
| --- | --- | --- | --- | --- |
| 1 | `tests/integer_wrapping.rs`（新規） | 4 | H1〜H4 | 手順 2（赤）・3（緑） |
| 2 | `tests/overflow_report.rs`（新規）: IR・地点・scope | 10 | H5〜H14 | 手順 5（赤）・7・8（緑） |
| 2 | `tests/overflow_report.rs`: `tsuzuri run --json` | 2 | H15・H16 | 手順 11 |
| 2 | 単体: `src/overflow.rs` 4 件、`src/llvm_overflow.rs` 1 件、`src/driver.rs` 2 件、`src/main.rs` の `rejects_ambiguous_or_unused_arguments` への追加 | 7 + 追加 | H26〜H29 | 手順 6・7・9・10 |
| 3 | `tests/overflow_report.rs`: `tsuzuri test` | 9 | H17〜H25 | 手順 14・15・16 |

### E2E（`tests/overflow_report.mjs`。独立した参照は JavaScript の BigInt）

付録 E のとおり。native と WASM の `-O0`・`-O3`、`live == 0` の検査は不要（確保を足さない。ただし `tests/features.mjs` の suite は変えない）。値の期待は BigInt の `asIntN`・`asUintN` で作る。標準エラーの行の位置は付録 E.2 の表の `行:列`（fixture の本文から数えた値）。

### 既存テストへの影響

| Phase | 影響 | 根拠 |
| --- | --- | --- |
| 1 | `tests/integer_intrinsics.mjs` の件数が増える（4 演算ぶん）。それ以外の期待値は変わらない | 追加だけ。`Int.wrapping_*` を使わないプログラムの IR は同じ（手順 3 の確認） |
| 2 | **なし** | フラグなしの IR・object・`.wasm` は 1 byte も変わらない（手順 1 の基準とのバイト比較を各手順の確認にした）。`BuildOptions` に field を足すだけで、既存のテストは `..BuildOptions::default()` を使う |
| 3 | `tests/fixtures/property/Props.tz` の `pair commutes`・`gen.md` の `addition commutes`・`matrix*.md` の 2 つの例（書き換える）。ほかに折り返して失敗する既存のテストが見つかったら、そのソースを直す | 付録 A.3・C.3。`driver::run_tests` の既定は変えないので、`TestOptions::default()` を使う既存のテストは変わらない |

### 決定性・資源上限・性能

IR を 2 回出して同じ（`emission_with_a_scope_is_deterministic`）。`llvm::duplicate_declarations` が空（`declare i64 @write(i32, ptr, i64)` と `declare i1 @llvm.expect.i1(i1, i1)` が 1 つずつ）。地点の上限は単体テスト。性能の主張はしない（付録 A.4 は実装後に同じ workload で再測定し、3 倍を超えて違うときだけ文書の「約 1.9 倍」を直す。共有 CI に速度の閾値を入れない）。

## ドキュメント

手順 4・13・18 は、次の表の自分の Phase の行を更新する。新しい例は付録 F のように、`run=` の期待値を実装の出力から確かめて足す。

| Phase | ページ（`_tsuzuri/language-reference/` は `LR/` と略す） | 更新 |
| --- | --- | --- |
| 1 | `LR/built-in-types-and-modules/int.md` | 「この記事のポイント」の 4 通りの説明に「折り返すことを名前で書く `Int.wrapping_*`」を足す。mermaid の図に `overflow --> wrapping["Int.wrapping_*"] --> wrapped2["同じ幅で折り返す（意図の表明）"]` の枝を足す。「検査、飽和、累乗、拡大乗算」の表に 4 行（失敗するときの列は「失敗しない。折り返す」）。「他の言語との比較」の表の「通常の `+`」に「意図した折り返しは `Int.wrapping_*`」を足す。付録 F の例（実装後の `tsuzuri run` の出力と一致することを確かめてから）。符号なしの否定を `Int.wrapping_sub 0 x` と書くことを 1 文 |
| 1 | `docs/language.md`・`docs/architecture.md` | `Int` の関数の表（`wrapping_pow`・`checked_pow` の行の近く）に 4 行。architecture（`@checked` の段落の近く）に「`Int.wrapping_*` は `add`・`sub`・`mul` 1 命令で、呼び出しは `FunctionEmitter::call` が inline する」を 2 文 |
| 2 | `LR/compiler/usage.md` | `### run`・`### build` に `--overflow-report`。新しい節 `## 整数の折り返しの報告`（仕様 2 の表、メッセージの形、native の標準エラーと WASM の export の読み方（Node の短い例は `text` の抜粋にする）、費用（約 1.9 倍。1 つの workload・1 台）、診断用であること、定数の畳み込みは報告しないこと） |
| 2 | `LR/compiler/webassembly.md` | `## トラップと trap.json` の近くに export `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` と `.overflow.json` |
| 2 | `LR/compiler/diagnostics.md`・`LR/built-in-types-and-modules/int.md` | `W2001` に実行時の報告の行（`run --json`）を 1 文。`int.md` に検出の節（`--overflow-report`） |
| 2 | `LR/values-and-functions/op-and-expressions.md` | 算術演算子が折り返すことの注に `Int.wrapping_*` へのリンク（`grep -n "折り返" LR/values-and-functions/op-and-expressions.md` で場所を探す） |
| 2 | `docs/language.md`・`docs/architecture.md`・`README.md`・`docs/stability.md` | CLI の表（`--overflow-report`）、計装の位置と `Scope`、WASM の export、検証コマンド一覧に `tests/overflow_report.mjs`。stability: `tsuzuri_overflow_*` を公開 ABI の `tsuzuri_` のシンボルに含めることを 1 文 |
| 3 | `LR/built-in-types-and-modules/test.md` | `## 実行する` に既定の報告（失敗の例は付録 D.3）、`## JSON` に `overflow` キー、`## プロパティテスト` に整数の例の注意、移行の手順（`Int.wrapping_*` に直す。`--allow-overflow` は一時的な逃げ道）、ライブラリ API の既定が変わらないこと |
| 3 | `LR/compiler/usage.md`・`LR/compiler/diagnostics.md` | `### test` に `--allow-overflow` |
| 3 | `LR/built-in-types-and-modules/gen.md`・`matrix.md`・`matrix-view.md` | 手順 17 の書き換え（D13） |
| 3 | `docs/language.md`・`docs/architecture.md`・`README.md`・`docs/stability.md` | `tsuzuri test` の既定と CLI の表（`--allow-overflow`）。stability: `tsuzuri test` の既定が変わることを「警告が出るかどうか」の保証の外として 1 文 |
| — | `_features/README.md`・台帳 | 統括者が更新する（`--allow-overflow` を D-55 に足す） |

検証: 変更した各ページで `node scripts/check-docs.mjs _tsuzuri/language-reference/<ページ>.md`。

## 受け入れ条件

### Phase 1

- [ ] `Int.wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg` が全幅・全符号で BigInt の参照と一致する（native・WASM、`-O0`・`-O3`）。
- [ ] `wrapping_neg` は符号付きだけ。幅の違い・浮動小数点・引数の数の誤りは `E1003`・`E1005`。
- [ ] 完全な適用は `add`・`sub`・`mul`・`sub 0` を inline し、旗を付けない。
- [ ] `Int.wrapping_*` を使わないプログラムの IR・object・`.wasm` が基準と 1 byte も違わない。
- [ ] 文書（`int.md`・`docs/language.md`・`docs/architecture.md`）を更新し、`check-docs` が通る。

### Phase 2

- [ ] `--overflow-report` なしの出力が基準と 1 byte も違わない（native・wasm32・`-O0`・`-O3`）。
- [ ] 付録 E の fixture で、native の標準エラーが付録 D.1 の 7 行、WASM の import が 0 で `tsuzuri_overflow_hit` が 7 地点で 1・1 地点で 0、sidecar の 8 地点。値は計装なしと同じ。
- [ ] 範囲の証明できる演算・`@checked`・`Int.wrapping_*`・std・依存・生成された補助関数は地点にならない。
- [ ] `tsuzuri run --json` が報告の行を `W2001` の JSON に変える。
- [ ] 上限 65,536 で `E1017`。IR は決定的で、宣言は重複しない。
- [ ] `check`・`doc`・`fmt`・`bench` での `--overflow-report` と、`--freestanding`・header・カーネル・バインディングとの組み合わせは `E2000`。
- [ ] 文書を更新し、`check-docs` が通る。

### Phase 3

- [ ] `tsuzuri test` が折り返したテストを失敗にし、`failure` に地点を示す。`--allow-overflow` で成功に戻り、警告を出す。JSON に `overflow` キー。
- [ ] ライブラリ API `driver::run_tests` の既定は報告しない。
- [ ] 長い標準エラーでも報告の行が落ちない。WASM のテストも同じ文字列。
- [ ] 付録 C.3 の既存の例・テストが `Int.wrapping_*` に直されて緑。期待値と報告の機構を変えずに通した。
- [ ] `cargo build --release --locked && cargo test --locked && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings` が緑。native と WASM の `-O0`・`-O3` の E2E が緑。

## 落とし穴

1. **旗を型の意味で読む**: `integer_flags` は `nsw` と `nuw` の両方を返す。`i32` の `x + y` で `nuw` が偽でも、符号付きの意味では溢れないことがある。符号付きは `nsw`、符号なしは `nuw` だけを見る。検出: `unproven_arithmetic_is_instrumented...` に `i32` の `(a &&& 255) + 1`（符号付きで証明される）を入れる。
2. **`tail_arguments` の span**: 呼び出し全体の span を使うと、同じ呼び出しの 2 つの引数が 1 地点になり、行・列も呼び出しの先頭を指す（試作はこの誤りを出した）。`argument.span` に差し替える。検出: `tail_call_arguments_are_sites_at_their_own_span`。
3. **検査のあとで演算の値を使わない**: 検査は `checked_integer_arithmetic` の旗だけを使い、元の `add`・`sub`・`mul` を書く（`i128` の `mul` は `checked_integer_arithmetic` が手書きで、値も別に `mul i128` で求めている）。値を差し替えると、デッドコードの除去や `nsw` の付与との相互作用でフラグなしの出力がずれる。検出: 値の比較（付録 E）。
4. **WASM で `atomicrmw` を使わない**: `atomics` なしの wasm32 では libcall（`__atomic_*`）になり、リンクに失敗する。WASM は `store atomic ... monotonic` だけ。検出: `native_ir_reports_through_write_and_wasm_ir_exports_the_hits` の `atomicrmw` なし。
5. **`@write` の宣言の綴り**: `declare i64 @write(i32, ptr, i64)` を `src/llvm_traps.rs` と一字一句同じにする（Windows の `windows_abi` が文字列で探して shim に置き換える。`unique_declarations` が重複を落とす）。検出: `llvm::duplicate_declarations`。
6. **`Drain::DepthFirst` の 2 回目**: `Globals` を作り直すので、地点の表も作り直す。scope への publish は最後の emission だけで、`emit_program_in` が `Err` を返したときはしない。
7. **`reported` の `span.source`**: `CheckedFunction.span.source` が `None` の関数（コンパイラが作ったもの）は報告しない。検出: H10。
8. **`TestResult.output` は失敗したテストだけ**（既存の規則）: `--allow-overflow` で成功したテストの警告は `TestResult.overflow` に持つ。`run --json` の変換は成功のときだけ（失敗の `E2005` は標準エラーを `message` に入れる既存の経路で、変換すると二重になる）。
9. **`TestOptions` を全 field で書くのは `src/main.rs` の 1 か所だけ**（ほかは `..Default::default()`）。足りないと言われた箇所に `overflow: None`。
10. **折り返すテストを通さない**: 乱数の整数どうしの `+ - *` は折り返す（`Gen.for_all`）。直すのは**テストのソース**（`Int.wrapping_*`、または `Gen.range` で範囲を絞る）で、報告を弱めて通さない。トップレベルの `let n = m + 1` は畳み込まれて報告されないので、E2E のプログラムは関数の引数を使う（付録 E）。

## 対象外

- 演算子の既定の意味の変更・検査付き算術の既定化・マニフェストの設定・環境変数・`--no-overflow-report`（D14）。
- 静的な lint と、両端が必ず溢れる式の編集時の診断（新しい診断コードが要る。将来のチケット）。
- コンパイル時の評価の折り返しの報告（D12）。std・依存パッケージの内部、SIMD・シフト・cast の報告。GPU のカーネルの計装。
- `Int.wrapping_shl`・`wrapping_shr`・`wrapping_abs`・`overflowing_*`（`(値, 溢れたか)` の組を返す形）。
- native の `tsuzuri_overflow_count`・`tsuzuri_overflow_hit` の export と C ヘッダーへの出力。`test -g`・`bench` の計装。LSP（編集中の診断）への反映。

## 決定事項

D1〜D15 はすべて**承認済み**。理由と、却下した案の詳しい理由は [付録 I](A23-appendix.md#付録-i-決定事項の理由と却下した案の詳細)。実装者は蒸し返さない。

| # | 決定 | 却下した案 |
| --- | --- | --- |
| D1 | 計装は、元の `add`・`sub`・`mul`・`sub 0` の**前**に溢れの旗で分岐して記録し、元の演算をそのまま書く（`audit` と同じ形） | `with.overflow` の値を結果に使う案（`i128` の `mul` の手書きと値が二重になり、ラッチの構造を変える） |
| D2 | 静的な lint（演算子ごとの警告）を作らない。根拠は付録 A.2（利用者のコードの演算の 89〜100% は証明できない） | 証明できない演算ごとの警告。両端が必ず溢れる式だけの診断（新しい診断コードが要る） |
| D3 | 地点の表は emission が作る（初めて計装した順に番号を振り、最後に scope へ publish）。事前の計画は作らない | 全候補の地点を事前に数える案（「計装された地点」の表と一致しない） |
| D4 | 報告の経路: native は IR が定数のメッセージを `write(2)` で書く。WASM は import なしで地点ごとの 1 byte に記録し、`tsuzuri_overflow_count`・`tsuzuri_overflow_hit` を export する。native に export は作らない | 種の `tsuzuri_overflow_sites()`（`memory` とポインターの幅がホストに移る）。終了時に書き出す案（`--freestanding`・Windows・トラップで落ちる）。WASM の import（D-50 が禁じる） |
| D5 | 1 地点につき 1 回、100 地点で打ち切り（101 地点目で 1 行）。環境変数なし | すべての発生を書く案。終了時にまとめる案。`TSUZURI_OVERFLOW_REPORT_MAX`（D-55 に無い名前） |
| D6 | 報告するのはルートパッケージの利用者の関数（ラムダ・`task` を含む）とテスト。std・依存・生成された補助関数・ベンチは報告しない | 依存も報告する案（利用者が直せない）。テストを除く案（`Gen.for_all` の整数の可換性が出ない） |
| D7 | `Int.wrapping_add`・`wrapping_sub`・`wrapping_mul`・`wrapping_neg` を足す。`wrapping_shl`・`wrapping_shr` は足さない。`wrapping_neg` は符号付きだけ | Rust との対称性のための `wrapping_shl`・`wrapping_shr`。符号なしの `wrapping_neg`（`-x` が符号なしで拒否される規則と食い違う） |
| D8 | `Int.wrapping_*` の完全な適用は、呼び出し側へ 1 命令を inline する（`-O0` でも演算子と同じ命令）。関数値には `integer_builtin` の本体を使う | 本体の関数だけ（`-O0` で呼び出しが残る） |
| D9 | 配線は `overflow::Scope`（スレッドローカル）で行い、`EmitOptions` に field を足さない | `EmitOptions` の field（24 テストが壊れる）。新しい `emit_*`（入口が 6 つ） |
| D10 | `run --json` は `W2001` を再利用し、`path` と 6 キーの `span` を足す | 種の「コードなし」の行（読み手が `code` を前提にする）。新しい `W` コード |
| D11 | `tsuzuri test` の CLI は既定で報告し、折り返したテストを失敗にする（`--allow-overflow` で報告だけ）。`driver::run_tests` の `TestOptions::default()` は報告しない | 警告だけの既定。完全にオフにする手段 |
| D12 | コンパイル時の評価（トップレベルの `let` の畳み込み・`const def`・`const`）の折り返しは報告しない | 評価器で折り返したら警告する案（新しい診断コードが要る） |
| D13 | 文書のハッシュ混ぜの例（`matrix.md`・`matrix-view.md`）は `Int.wrapping_*` に書き換える（`run=` の期待値は同じ） | 書き換えない（`test` に移すと失敗する書き方が残る） |
| D14 | マニフェストの設定・環境変数・`--no-overflow-report` を作らない | 設定で報告を切る案（意図の表明の意味が薄れる） |
| D15 | 新しい診断コードも `kind` も作らない（`W2001` の行に `kind` を付けない。G23 の登録簿が決める） | 新しい `W` コード（D10） |

## 付録

実測・試作・出力例・fixture・Rust の骨格・テストの一覧・決定の理由は [A23-appendix.md](A23-appendix.md) にある。A: 実測（静的な数・動的な数・計装の費用・WASM の確認）。B: 整数の演算を書く箇所の全一覧。C: 既存テスト・文書への影響。D: 出力の例（native・`run --json`・`test`・WASM の sidecar・IR）。E: E2E の fixture と期待値。F: 言語リファレンスに足す例。G: 設計の骨格（Rust）。H: Rust テストの一覧。I: 決定事項の理由と却下した案の詳細。

## 実装と検証（2026-10-29）

- **範囲**: Phase 1〜3 をすべて実装した。冒頭の「方針の最新化」のとおり、A19〜A22・G23 の実装後の現状に合わせた。基盤は A22 のブランチ（`feat/a22-canonical-forms`）の上。
- **Phase 1**: `Builtin::IntWrappingAdd`・`IntWrappingSub`・`IntWrappingMul`・`IntWrappingNeg`（`Integer<'a> => 'a -> 'a -> 'a`、`neg` は `SignedInteger<'a> => 'a -> 'a`）。`integer_builtin` の腕（`add`・`sub`・`mul`・`sub 0`）と、`FunctionEmitter::call` の inline（`$builtin` のラッパーの直接の呼び出しを `call_specialization::wrapping_builtin` が見分け、ラッパー自身の本体も同じ命令）。旗は付けない。`pure_rules`・`effects::builtin_facts`・`kg::effects` は効果なし、`prove_smt` は `bvadd`・`bvsub`・`bvmul`・`bvneg`。`tests/integer_intrinsics.mjs` は 4 演算を足して `301969 BigInt cases`（`-O0`・`-O3` とも）。`Int.wrapping_*` を使わない fixture と examples の IR（native と wasm32、`-O0`。252 件）は実装前と 1 byte も同じ。
- **Phase 2**: `src/overflow.rs`（`Operation`・`Site`・`Source`・`Scope`・`side_table`・`report_lines`・`json_lines`）、`src/llvm_overflow.rs`（`Report`・`covers`・`number`・`runtime`・`publish`・`ranges_proven`・`overflow_check`）、`RangeFacts::negation_flags`。計装は `binary`・`tail_arguments`（引数の span）・単項 `-`・`power`（`**` の式 1 地点）の 4 か所で、元の演算の前に旗で分岐し、値に触れない。driver は `BuildOptions.overflow_report`・`validate` の `E2000` 2 つ・`Project::overflow_sources`・`build_complete` の scope と地点（戻り値の型 `Built`）・`overflow_sidecar_path` と WASM の 2 つの export・`run --json` の変換。ビルドキャッシュは成果物 `overflow` を受け付ける（1 項目 4 ファイルまで）。CLI は `--overflow-report`（`build`・`run`・`test`）。`DiagnosticKind` は `integer_overflow_wrapped`（`W2001`）と `too_many_overflow_sites`（`E1017`）の 2 つ。
- **Phase 3**: `TestOptions.overflow: Option<OverflowTests { fail, sources }>`（既定 `None`）、`TestResult.overflow`、`read_tail` が末尾 64 KiB とは別に報告の行を最大 101 行（1 行 4,096 byte まで）集め、`execute_test` が成功したテストを最初の報告で失敗にする。WASM のランナーは地点の表を `test-runner.mjs` の引数（`tests.wasm overflow.json index`）に渡し、成功の後に `tsuzuri_overflow_hit` を読んで同じ行を書く。CLI の `tsuzuri test` は既定で `fail: true`、`--allow-overflow` で `fail: false`（`ok` の行の後に `  warning: `）、JSON の `overflow` キー。`-g` のデバッグ用のランナーと `bench` は計装しない。
- **既存のテストと文書の直し（付録 C.3 と手順 17）**: 期待値と報告の機構は変えず、テストのソースの算術を `Int.wrapping_*` に直した。付録 C.3 のもの: `tests/fixtures/property/Props.tz` の `pair commutes`、`gen.md` の `addition commutes`（1 文を足した）、`matrix.md`・`matrix-view.md` のハッシュ（`run=` の期待値は同じ）、`tests/fixtures/contracts/Main.tz` の `wrap_add`（`features.mjs` の `contracts` は 13 件のまま通る）。付録 C.3 に無いもの 3 件（停止条件 9 の 5 件以内）: `tests/fixtures/property/Bounds.tz` の `-x != x`（最小値で折り返すことを確かめる性質。`Int.wrapping_neg`）、`tests/fixtures/map_set_chunks/Main.tz` の線形合同法 `next_state`（`Int.wrapping_add (Int.wrapping_mul ...)`）、`tests/contracts_property.rs` の `area` の `w + h`（任意の `i64` の和。`Int.wrapping_add`）。文書の例では `README.md` のプロパティテストと `guide/workflow.md` の `Gen.i64()` の `double n`（`Gen.range (-1000) 1000` に。D-56 の文字数の上限内）も直した。
- **レビュー（PR #67）**: メッセージの列を演算子の位置にした（`a + b + c` の 2 地点を区別する。付録 D.1 の行は `1:51`・`2:55`・`3:47`・`4:40`・`5:51`・`6:47`・`8:107` になる）。報告のあるビルドでは末尾呼び出しの引数の利用者の補助関数を展開しない。WASM のランナーは失敗したテストでも折り返した地点を書く（native と同じ `TestResult.overflow`）。CLI のテストはパスを `Path::new(".").join("Main.tz")` で作る（Windows は `.\Main.tz`）。
- **付録との差**: 付録 D.1・E.2 の末尾呼び出しの地点は `8:92`・`8:100`（括弧を含む式の span）。付録 D.2 の JSON の行は最後に `"kind":"integer_overflow_wrapped"` を持つ。付録 D.5 の地点の byte のアドレスは `getelementptr` の命令。H4 の関数値は `$builtin` のラッパーの本体の 1 命令。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。新しいテスト: `tests/integer_wrapping.rs` 4、`tests/overflow_report.rs` 21（H5〜H25）、`src/overflow.rs` の単体 4、`src/llvm_overflow.rs` の単体 1、`src/driver.rs` の単体 3、`src/main.rs` の `overflow_report_and_allow_overflow_arguments`。`node tests/overflow_report.mjs`（native と wasm32 の `-O0`・`-O3`、付録 E の値・標準エラー・`tsuzuri_overflow_hit`・地点の表）は `TSUZURI_RANGE_FLAGS` の `off`・`proven`・`audit` のどれでも同じ結果。`TSUZURI_CODEGEN_UNITS=split` の native と wasm32 でも報告は同じ。`--overflow-report` を付けない IR は実装前と 1 byte も同じ（252 件）。
- **費用の再測定（付録 A.4 と同じ workload。1 台・3 回）**: `-O3` の native、実装の `--overflow-report` で user 0.36・0.36・0.37 秒 → 0.73・0.75・0.75 秒（約 2.0 倍。出力は同じ `1513211202258126784`）。試作の約 1.9 倍との差は 3 倍以内なので、文書の「約 1.9 倍」は変えない。性能の主張はしない。
