# F15: 逐次と同一の結果を保証するデータ並列（`Monoid` の還元・純粋性・提案だけの自動化）

| 項目 | 内容 |
| --- | --- |
| ID | F15 |
| 優先度 | P3 |
| 規模 | 全体 L（1〜3 週）。Phase 1: M（3〜5 日）、Phase 2: S（1〜2 日）、Phase 3: M（3〜5 日）、Phase 4: S（1 日。G26 の後） |
| 依存 | Phase 1・2: なし。Phase 3: A19 Phase 1（`src/pure_rules.rs` の効果の表と到達可能性。done が前提）。G22 は任意（`Fix`・`Applicability` の型。なければ同じ JSON の形を自前で出す）。Phase 4: G26 Phase 3（`facts/<Module>.jsonl` の行と `KbModel::fact_keys()`） |
| 後続 | G23（kind `monoid_float_instance` を登録簿に入れる）、G26（facts の `parallel` キー。Phase 4）、G35（`Parallel` の保証の表を生成するときは本チケットの仕様 §5 に合わせる） |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1… はすべて承認済み |
| 改善する劣位 | `Parallel.reduce`・`Parallel.sum` は、結合的でない演算や浮動小数点で逐次の `Array.reduce`・`Array.sum` とビットが違う（文書の注意書きだけ）。「結果が逐次と同じ」と言える条件が型で書けず、コールバックの純粋性による保証もテストと文書がない。並列化できる箇所を見つける手段もない。黙った自動並列化は採らず、**保証・検査・提案**を足す |
| 手本にする既存実装 | `std/Parallel.tz`（`Parallel.sum` が `Parallel.reduce` の薄い包み）、`std/Array.tz` の `fold`、`std/Json.tz` の `Encode`・`Decode`（std が持つ型クラスの instance）、`src/polymorph.rs` の `validate_format_instance`・`validate_drop_instance`（クラスごとの instance の検査）と `ClassEnv::validate_instance`（制約の失敗の診断）、`src/copies.rs`（`CheckedModule` を走査する報告。`--warn implicit-copy` の `copies::warnings`）、`src/trap.rs`（JSON を `write!` で書く報告） |
| 主な影響ファイル | Phase 1: `std/Monoid.tt`・`std/Monoids.tz`（新規）、`std/Array.tz`・`std/Parallel.tz`、`src/stdlib.rs`、`src/check.rs`（`Type::holds_float`）、`src/polymorph.rs`（E1039）、テスト（`tests/fixtures/monoid/`（新規）・`tests/features.mjs`・`tests/monoid.rs`（新規））、文書。Phase 2: `tests/fixtures/parallel_equivalence/`（新規）・`tests/features.mjs`、文書。Phase 3: `src/parallel_report.rs`（新規）、`src/pure_rules.rs`（A19。到達可能性の関数 1 つ）、`src/main.rs`、`src/lib.rs`、`src/driver.rs`、`tests/parallel_report.rs`（新規）、文書。Phase 4: `src/kb/facts.rs`・`src/kb/model.rs`（G26 が作る facts の書き出し）、`tests/kb.rs`（G26 のテスト） |

付録: [F15-appendix.md](F15-appendix.md)（A: `std/Monoid.tt`、B: `std/Monoids.tz`、C: `Array.tz`・`Parallel.tz` の追加、D: `src/stdlib.rs`、E: fixture `monoid`、F: fixture `parallel_equivalence`、G: `features.mjs` の suite、H: 実測と試作の結果、I: `monoid.md`（文書ページの全文）、J: `tests/monoid.rs`、K: 文書の変更の全文）。

## 方針の最新化（2026-10-31。実装前の再検討）

起票時（HEAD `a2313d19`）の前提と、A18・A20・A22（edition 2027）・G23・G26〜G30（D-60 の KG）・G34・G35 の実装後の現状が食い違う点を、次のとおり改めた（人間の包括許可による判断。D1〜D13 の趣旨は保つ）。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| 付録のソースは edition 2026 の表記 | manifest のない入力は edition 2027（素の `..` は `E0004`）。`Array.map_ref` は `Array.mapr` に改名済み | 付録 F の `0i64 .. (chunk.length - 1)` を `..=` に、`Array.map_ref` を `Array.mapr` に、付録 L2 の `rules` の 17 行目を `Array.mapr` に直した（期待値 `RULES_TEXT` も `Array.mapr can be Parallel.map_ref`）。付録 E・A〜C は 2027 のまま通った |
| `RESERVED_MODULES` は 53 → 55 | 54（`Alloc` が先に入った） | 56。`modules.md` の「53 個」と一覧は `Alloc` が抜けていたので、`Alloc`・`Monoid`・`Monoids` を足して 56 にした |
| E1039 は `Diagnostic::new(...).with_kind(...)` | G23 の登録簿（`Diagnostic::of`） | `DiagnosticKind::MonoidFloatInstance`（`monoid_float_instance`、`E1039`、`since "0.1.0"`）と `.with_help(...)`。説明ページ `src/explain/codes/E1039.md`、`diagnostics.md` の生成ブロックは `gen-docs.mjs` で更新 |
| G34・G35 は未実装なら何もしない | 実装済み | `Array.fold_monoid` に `@complexity O(n)`・`@since 0.1.0`・`@see Array.fold`・`@see Parallel.fold` と `run=6` の例、`Parallel.fold` に `@since`・`@see` と `run=10` の例。`FLOORS` に `Monoid`・`Monoids`（0）、`std_argument_order.rs` の `MODULES` に両方を `SingleArgument`（`def` 行がない）で足した。`Array` の計算量の表は `@since` を持つ関数ができたので「版」の列が増えた（生成） |
| A19 の窓口 `reachable_effect` は手順 9 で足す | A19 の後続（D-53）で `reachable_effect`・`builtin_impure` が既にある。`Effect::Builtin` は `(Builtin, Category)`、`Effect::Io` がある | 既存の窓口を使い、手順 9 は単体テスト 3 件だけを足した。停止条件 12 のとおり `reason` の対応だけを直した: `Builtin` の `{effect}` は L7 の名前の接頭辞の表ではなく A20 の分類（`effects::builtin_facts(..).effect` の D-53 の名前。動的ディスパッチは `dyn`）、`Io` に `the callback may execute an I/O action (effect: io)` を足した |
| `for_each_chunk` の本体も `reachable_effect` で判定 | A19 の `witness` は `ref mut` の引数を「不透明な入力」として `Unresolved` にするので、どの本体も純粋にならない | `pure_rules::reachable_effect_of_chunk_body` を足した（最後の引数＝排他チャンクだけを引数の検査から外し、本体と到達する関数は同じ規則で走査）。§5 の「自分のチャンクだけを書く」本体が `parallel_safe: true` になる |
| `Array.map` の呼び出しは std の `Array.map` の `Call` | edition 2027 は `Array.map f xs` を読み取りコールバックの `Array.mapr` に解決し、値を取る関数（`square`）を `Checker::callback_adapter` の `{ let f = square; \x -> f x }` で包む | 提案は「書かれた名前」で判定する（呼び先が `Array.mapr` でも、ソースが `Array.map` なら `Parallel.map`。要素の `Copy` を要求）。名前の直後が識別子の文字なら一致としない（`Array.map` と `Array.mapr`）。純粋性はこのアダプタ（捕捉 1 つを呼ぶだけの本体）を剥がしてから判定する |
| `Project::source_of` を足す | `Project::source_for_span` が同じ働き | `impl Sources for Project` は `source_for_span` を使う |
| `src/kb/facts.rs`・`model.rs`・`tests/kb.rs`（Phase 4） | KB は D-60 で KG（`src/kg/`）になった。facts の行は `src/kg/facts.rs` の `build`、キーは `src/kg/model.rs` の `FACT_KEYS` | `FactsRow.parallel`（`ParallelRow`）と `FACT_KEYS` の `parallel`（`specializations` の次、`truncated` の前。14 キー）、`kg-format.md` の行。テストは `tests/kg.rs` の `facts_rows_carry_parallel_entries`（ID は `Main.pure_total` の形） |
| `tests/parallel_report.rs` は fixture のディレクトリで `check Main.tz` | `Main.tz` のあるディレクトリはプロジェクトで、既定で `target/tz/kg/` を書く | テストの `run` は `TSUZURI_KG=0` で走らせる（ソースの木を汚さない。stderr が空の検査も保つ） |
| `--help \| grep -c "report parallel"` が 1 | — | 使い方の行に加え、オプションの説明の行を足したので 2 |

## 実装者向け早見

1. **最初に読む**: 「仕様」§1〜§3（`Monoid`・`Monoids`・同一の証明）と、付録 A〜G・I・J・K（検証済みの全文。**写して書き換えない**）。Phase 3 は §7（`--report parallel`）と「設計」の Phase 3、付録 `F15-appendix-report.md`。
2. **最初に書くテスト**: 付録 E の fixture `tests/fixtures/monoid/Main.tz` と付録 G の suite `monoid`（手順 2）。`std` を足す前は `error[E1016]: unknown type class 'Monoid'` で赤になるのが正しい。
3. **名前**: クラス `Monoid`（`std/Monoid.tt`。常時ロード）、newtype の module `Monoids`（`std/Monoids.tz`。opt-in なので **`Monoids.Sum`** のように修飾して書く）、`Array.fold_monoid`、`Parallel.fold`。診断コード `E1039`（kind `monoid_float_instance`）。CLI `--report parallel`（新しい `--report` フラグ。値は `parallel` だけ）。
4. **保証の核**: `Parallel.fold values` は `Parallel.reduce (Monoid.empty ()) (\left right -> Monoid.combine left right) values`。`Monoid` の法則（結合律・単位元。ビットまで）が成り立ち、`empty`・`combine` が外から見える作用を持たなければ、チャンクの分け方によらず `Array.fold_monoid values` と**ビット単位で同一**（仕様 §3 の証明と条件）。浮動小数点を含む型の `Monoid` は **E1039 で拒否**する。
5. **変えない**: `Parallel.*` の実行時の規則（チャンク数の式・結合順・WASM の逐次経路）、`Parallel.reduce`・`sum` の意味、`Array` の既存関数、`--report` を付けない `check` の出力（1 byte も変わらない）、新しい予約語・crate・`unsafe`・WASM の import。
6. **触る Rust は小さい**: Phase 1 は `src/stdlib.rs`（4 か所）、`src/check.rs`（`Type::holds_float` 1 つ）、`src/polymorph.rs`（`ClassEnv::normalize` に 1 か所）。Phase 3 が新規 `src/parallel_report.rs` と `main.rs` の配線。
7. **罠**: 「落とし穴」の 1〜5（opt-in は修飾して書く・`std/Monoid.tt` は opt-in にしない・非可換な `Affine` のテスト・`std` を変えたら毎回再ビルド）。
8. **Phase 1 の完了**: 手順 5 の確認と受け入れ条件 Phase 1（`monoid` が `90 cases passed`、`tsuzuri test tests/fixtures/monoid` が `11 passed`、`tests/monoid.rs` と `stdlib` の 3 件が緑、`parallel` が 36 件）。
9. **Phase 2 の完了**: suite `parallel_equivalence` が `48 cases passed`、`parallel.md` と `docs/language.md` に保証の表がある。
10. **Phase 3 の完了**: `tests/parallel_report.rs` が緑、`--report` なしの `check` の出力が不変、同じ入力の 2 回の報告が同一。**Phase 4** は G26 の後（なければ着手しない）。

## 目的

**問題**:
1. `Parallel.reduce`・`Parallel.sum` は、チャンク順に結合する（決定的）が、結合的でない演算や浮動小数点では `Array.reduce`・`Array.sum` とビットが違う（`docs/language.md` の注意書きだけ）。「この還元は逐次と同じ」と型で言える入口がない。
2. `Parallel.map`・`map_ref`・`init`・`for_each_chunk` は、コールバックが純粋なら逐次と同じ結果を返す構造だが、その保証は文書になく、チャンクの境界でのテストもない。
3. どの `Array.map`・`Array.sum` を `Parallel.*` に置き換えられるかを、人間も AI も手で探す。黙って自動で並列化すると、実行の順序・コスト・メモリの意味が見えなくなるので採らない。

**利用者の価値**: (1) 結合的な還元を `Monoid` として書けば、`Parallel.fold` が逐次と同じビットを返すと保証される。浮動小数点の誤用はコンパイルで止まる。(2) `Parallel.*` の保証が API ごとの表で読める（逐次と同一／決定的だが異なりうる／未規定）。(3) `tsuzuri check --report parallel` が、置き換えられる箇所と、既存の `Parallel.*` の呼び出しの純粋さを、決定的な出力で示す（提案だけで、自動では適用しない）。

**非目標**: 黙った自動並列化と提案の自動適用。浮動小数点の `Monoid`。実行時の規則（チャンク数・結合順・`Gpu.Auto`）の変更と新しい実行経路。結合律の検査（法則の検査は `Gen` のテスト）。`Task`・`Async` の解析。新しい予約語・crate・`unsafe`・WASM の import（D-50）。詳細は「対象外」。

## 着手条件と停止条件

### 着手条件

- Phase 1・2 に依存はない。Phase 3 は A19 Phase 1 が done であること（`ls src/pure_rules.rs` が存在し、`builtin_effect` がある）。Phase 4 は G26 Phase 3 が done であること（`facts/<Module>.jsonl` を書く `write_facts` がある）。
- ベースライン（GUIDE §2.3）: `cargo build --release --locked` と `cargo test --locked`。HEAD の実測: `node tests/features.mjs target/release/tsuzuri parallel` が `features: 36 cases passed`。
- 読むもの: `std/Parallel.tz`（2 行）、`std/Array.tz` の `fold`、`docs/language.md` の節「データ並列 API」と「多相関数と型クラス」、`_tsuzuri/language-reference/built-in-types-and-modules/parallel.md`、`src/stdlib.rs` の `OPT_IN`・`RESERVED_MODULES` と、その下の 3 つの単体テスト、`_features/GUIDE-LITE.md`、`_features/_completed/A19-pure-functions.md` の「検証のアルゴリズム」と「組み込み関数の分類」（Phase 3）。

### 停止条件

1. **`std/Monoid.tt`・`std/Monoids.tz` を足すと、`src/stdlib.rs` の単体テスト（`opt_in_modules_are_reached_only_through_their_names`・`every_std_module_type_checks_when_named`・`reserves_the_d07_table`）が赤になる**。規則: 常時ロードの std は opt-in を名前で使えない。opt-in の instance は自分の型か `names` のクラスだけ。付録 D の形は検証済みなので、写し間違いを疑う。直らなければ止めて報告する。
2. **付録 E の `monoid` suite のどれかが失敗する**（`Mismatch` の値 `9223372036854775807` が返る、または JS の参照と違う）。`Parallel.fold` の結合順の誤りの疑い。止めて報告する（期待値を実装に合わせて変えない）。
3. **既存の `parallel`（36 件）・`json`・`map_set`・`iteration_protocol` の suite が赤になる**。`std` の追加が既存の名前を壊した。止めて報告する。
4. **`ClassEnv::normalize` に E1039 を入れても、仕様 §4 の例 1〜3 が `E1039` にならない**（`class.name` が `Monoid.Monoid` と一致しない、`current.ty` が具象にならない）。対処: 一時的な `dbg!`（コミットしない）で `self.declarations[current.class].name` の実際の値を確かめて綴りを合わせる。それでも出なければ、`validate_instance` の入口にも同じ検査を足す（instance が見つからない制約用）。文面・span・kind は同じ。
5. **（Phase 3）A19 が未実装**（`src/pure_rules.rs` がない）。止める。Phase 1・2 だけを出荷し、Phase 3 を後回しにする。
6. **（Phase 3）同じ入力の報告が実行ごとに違う**（`HashMap` の反復順）。`BTreeMap`・整列で直すまで止める。
7. **（Phase 3）提案を適用したソースが型検査を通らない**（`tests/parallel_report.rs` の「適用して再検査」のテストが赤）。提案の条件（仕様 §7.3）が足りない。条件を足すまで止める（提案しないほうへ倒す）。
8. **（Phase 3）`--report` なしの `check` の出力が 1 byte でも変わる**。止めて直す。
9. **（Phase 4）G26 が未実装**。着手しない（本チケットの完了の条件にしない）。
10. **新しい std モジュール名 `Monoid`・`Monoids` は、`docs/stability.md` の「std へのモジュールの追加」の例外に当たる**（同名の利用者のモジュールは `E1011`）。止めなくてよいが、完了報告に「新しく予約した std モジュール名: `Monoid`・`Monoids`」と書き、release notes に載せる。
11. **（Phase 3）`the_basic_report_in_text_and_json` の 6:19 の提案が出ない**（呼び出し式の `span.start` から `call` の長さのテキストが `Array.map` と一致しない。`TypedExpr.span` の始まりが名前の先頭でない）。対処: `Call` の `callee.span` のテキストが `call` と一致するときだけ提案し、`span` は `callee.span` にする。それでも通らなければ止めて報告する。
12. **（Phase 3）A19 の `Effect`・`Scanned`・`builtin_effect` の形が、設計 3 の想定（`Effect::Builtin(&'static str, Category)`・`Host(String)`・`Concurrency`・`Dyn`・`UserDrop(String)`・`Unresolved`）と違う**。対処: A19 の実際の名前に合わせて §7.2 の `reason` の表の対応だけを直す（文言と `{effect}` の語は変えない）。合わせられなければ止めて報告する。

## 現状（HEAD `a2313d19` で確認）

作業木の HEAD は `9dd0fbbc`（`a2313d19` の子孫で、差は `docs/benchmarks.md` だけ）。実行は `target/release/tsuzuri`（HEAD）と、複製に付録 A〜D を置いて `cargo build --release --offline`（3 分 6 秒）したコンパイラで行った。

**コード**:
- `std/Parallel.tz`（2 行）: `def sum :: (Numeric<'a>, Add<'a>, Copy<'a>, Send<'a>) => ref ['a] -> 'a` と `fn sum values = Parallel.reduce (0i64 as 'a) (left -> right -> left + right) values`。`Parallel.init`・`map`・`map_ref`・`reduce`・`for_each_chunk` は組み込み関数（`src/check.rs` の `Builtin::ParallelInit`・`ParallelMap`・`ParallelMapRef`・`ParallelReduce`・`ParallelForEachChunk`。実装は `src/llvm_parallel.rs`）。
- `Parallel.reduce identity reducer input`: チャンク数 `k = min(1024, ceil(n / 4096))`（空なら 0）、チャンク `c` の開始 `(n / k) * c + ((n % k) * c) / k`。各チャンクは `identity` の複製から昇順に畳み込み、呼び出し元が `identity` からチャンク順に部分結果を結合する（`docs/language.md` の節「データ並列 API」。WASM は同じ順の逐次）。
- 型クラスは `.tt` に宣言し（`class Score<'a> { def score :: ref 'a -> i32 }`）、instance は `.tz` に書く。条件付き instance は `instance (Integer<'a>, Add<'a>) => Monoid<Sum<'a>> { ... }`。`src/stdlib.rs` の `SOURCES` に std の `.tt` はまだない（単体テストは `module_name("std/Classes.tt")` を受理する）。
- `--report` というフラグは `src/main.rs` にない（`grep -n '"--report"' src/main.rs` は 0 件）。`--warn` は `implicit-copy` だけ。`check` は成功しても何も出力しない（`Action::Check => Ok(Vec::new())`）。

**再現コマンドと実出力**（付録 H に試作の全結果）:

```sh
$ target/release/tsuzuri check q2     # [Monoids.Sum { value: 1.5 }, ...] に Parallel.fold（std を足した複製のコンパイラ）
q2/Main.tz:2:13: error[E1005]: no instance for Integer<f64>; define an instance or use a supported type
$ target/release/tsuzuri check q3     # record Mean { total: f64, count: i64 } に instance Monoid<Mean>
（出力なし。受理される → Phase 1 で E1039 にする）
$ target/release/tsuzuri check q1     # 修飾なしの Sum { value: 1 }
q1/Main.tz:1:15: error[E1004]: unknown record, union, or type alias 'Sum'
$ node tests/features.mjs <複製のコンパイラ> monoid
monoid: native/WASM at O0/O3 passed
features: 90 cases passed
$ node tests/features.mjs <複製のコンパイラ> parallel_equivalence
parallel_equivalence: native/WASM at O0/O3 passed
features: 48 cases passed
$ <複製のコンパイラ> test tests/fixtures/monoid
ok 1 - Main Sum<i64> obeys the laws
...（11 件）
11 passed; 0 failed; 0 ignored
```

**整数は折り返す**: `Sum<i32>` の `2147483647 + 1 + (-1)` は `2147483647`（トラップしない。`-O0`・`-O3`・WASM で同じ）。折り返しは結合的なので、整数の `Sum`・`Product` は逐次と同一になる。
**非可換なモノイドの必要**: `Parallel.reduce` の `combine` の引数を入れ替える変異は、`Sum`（可換）では差が出ず、非可換な `Affine`（`x -> scale * x + shift` の合成）で要素数 2 から差が出た。
**float の Parallel.sum**: `Parallel.sum` は決定的だが、`Array.sum` と違いうる。`(((i * 7919 + 13) % 1009) as f64) / 10.0 + 0.1` の列で、n = 4096・4097・12288 は一致し、**n = 8193 は一致しない**（`Array.sum` = `413670.50000000006`、`Parallel.sum` = `413670.5`。JavaScript の倍精度で同じ計算をして確かめた）。

## 変えてはいけない意味

1. **`Parallel.*` の実行時の規則**: チャンク数の式 `k = min(1024, ceil(n / 4096))`、チャンクの境界、結合の順、`identity` の複製、WASM の逐次経路、`Gpu.Auto` のコスト規則。`std/Parallel.tz` の `sum` は 1 文字も変えない。**新しい最適化の経路・ランタイム関数・組み込み関数を足さない**。
2. **`Array` の既存関数・`Parallel` の既存関数の型と意味**。`fold_monoid`・`fold` を足すだけ。
3. **`--report` を付けない `check`・ほかのサブコマンドの出力と終了コード**（1 byte も変わらない）。`--warn` の既存の値。
4. **既存の診断コードの意味**。浮動小数点の `Monoids.Sum<f64>` の `E1005` は、E1039 に変わる（新しい `Monoid` の制約だけで起きる。既存のプログラムは影響を受けない）。
5. **IR・ABI**: `Monoid`・`Monoids` を使わないプログラムの意味は不変。IR は連番の接尾辞（`$intrinsic.Display.display.<N>`）だけが `std` の宣言数で変わりうる（C12 の実測と同じ）。WASM の import を足さない。
6. **決定性**: 同じソースから同じ IR・同じ報告（Phase 3）。edition・予約語は変更なし（`Monoid`・`Monoids` は std のモジュール名の予約で、予約語ではない）。
7. **逐次のコードの実行**: `Array.fold_monoid` は `Array.fold` と同じ左畳み込み（`Monoid.empty ()` から、添字の昇順）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `Monoid`（`.tt`）・`Monoids`（opt-in）・`Array.fold_monoid`・`Parallel.fold`・E1039・テスト・文書 | M | なし | 追加だけ。既存の API・実行時の規則は不変。使わないプログラムの意味は同じ |
| 2 | 純粋性の保証: `Parallel.*` の保証の表（文書）と、純粋なコールバックでの逐次との同一を E2E で確かめる suite `parallel_equivalence` | S | なし（Phase 1 の後でも前でもよい） | 文書とテストだけ。Rust の変更なし |
| 3 | `tsuzuri check --report parallel`（提案だけ。`fixes` の `maybe`）と、呼び出し地点の `parallel_safe` | M | A19 Phase 1 | `--report` を付けたときだけ動く新しいフラグ。付けない `check` は不変。A19 がなければ出荷しない |
| 4 | G26 の facts の行に `parallel` キー（`parallel_safe`）を足す | S | G26 Phase 3 | facts にキーを足すだけ（読み手は知らないキーを無視する）。G26 Phase 3 がなければ着手しない |

## 仕様

### §1 `Monoid`（クラス。`std/Monoid.tt`、Phase 1）

```text
class Monoid<'m> {
    def empty :: unit -> 'm
    def combine :: 'm -> 'm -> 'm
}
```

呼び出しは `Monoid.empty ()`・`Monoid.combine left right`、制約は `Monoid<'m>`（無修飾でよい。クラス名は std で一意）。instance は `.tz` に書く。**`empty` は `let empty = \() -> ...` と書く**（`fn empty () = ...` は `E0002`）。

| 法則 | 内容（任意の `a`・`b`・`c` について、ビットまで同じ） |
| --- | --- |
| L1 結合律 | `combine (combine a b) c` と `combine a (combine b c)` が同じ値 |
| L2 左単位元 | `combine (empty ()) a` と `a` が同じ値 |
| L3 右単位元 | `combine a (empty ())` と `a` が同じ値 |

コンパイラは法則を検査できない。検査は `Gen` の性質テスト（付録 E の `laws`）で行う。利用者は自分の instance に同じテストを書く（文書に例を載せる）。

### §2 `Monoids`（newtype と instance。`std/Monoids.tz`。opt-in。Phase 1）

利用者は `Monoids.Sum { value: 1 }`・`Monoids.Sum<i64>` のように**修飾して**書く（無修飾は `E1004`）。`value` が中身。

| 型 | instance の文脈 | `empty` | `combine left right` | 備考 |
| --- | --- | --- | --- | --- |
| `Sum<'a>` | `Integer`・`Numeric`・`Add`・`Copy` | `0i64 as 'a` | `left.value + right.value` | 折り返す。整数だけ |
| `Product<'a>` | `Integer`・`Numeric`・`Mul`・`Copy` | `1i64 as 'a` | `left.value * right.value` | 折り返す |
| `MaxOf<'a>`（`value: Maybe<'a>`） | `Integer`・`Ord`・`Copy` | `Maybe.None` | 片方が `None` なら他方。両方 `Some` なら大きいほう（同じ値なら左） | 空は値なし |
| `MinOf<'a>`（`value: Maybe<'a>`） | 同上 | `Maybe.None` | 小さいほう（同じ値なら左） | |
| `BitAnd<'a>` | `Integer`・`Numeric`・`Bits`・`Copy` | `Bits.bit_not (0i64 as 'a)`（全ビット 1） | `Bits.bit_and` | |
| `BitOr<'a>` | 同上 | `0i64 as 'a` | `Bits.bit_or` | |
| `BitXor<'a>` | 同上 | `0i64 as 'a` | `Bits.bit_xor` | |
| `All`（`value: bool`） | なし | `true` | `left.value && right.value` | |
| `Any`（`value: bool`） | なし | `false` | `left.value \|\| right.value` | |

**浮動小数点の instance は作らない**: `Integer<'a>` の文脈が `f32`・`f64` を拒否し、さらに E1039（§4）が `Monoid` を浮動小数点を含む型に使うことを拒否する。理由は、浮動小数点の加算・乗算が結合的でないため、チャンクの分け方で結果が変わるから（`Parallel.sum` が `Array.sum` と違う実例が「現状」にある）。

### §3 `Array.fold_monoid` と `Parallel.fold`（Phase 1）

| 関数 | シグネチャ | 意味 |
| --- | --- | --- |
| `Array.fold_monoid` | `(Monoid<'m>, Copy<'m>) => ref ['m] -> 'm` | `Monoid.empty ()` から、添字の昇順に `state = Monoid.combine state value`。空なら `Monoid.empty ()` |
| `Parallel.fold` | `(Monoid<'m>, Copy<'m>, Send<'m>) => ref ['m] -> 'm` | `Parallel.reduce (Monoid.empty ()) (\left right -> Monoid.combine left right) values` |

**定理（逐次との同一）**: `Monoid<'m>` の instance が L1〜L3 を満たすなら、任意の入力 `x_0 … x_{n-1}` で `Parallel.fold` と `Array.fold_monoid` は**ビット単位で同じ値**を返す。
証明: `⊕` を `combine`、`e` を `empty ()` とする。`n = 0` なら両方 `e`。`n > 0` のとき、チャンクの境界 `0 = b_0 < … < b_k = n`（`k` は `min(1024, ceil(n/4096))`）について、チャンク `c` の部分結果は `P_c = (…((e ⊕ x_{b_c}) ⊕ x_{b_c+1}) …) ⊕ x_{b_{c+1}-1}`、`Parallel.reduce` の結果は `R = (…((e ⊕ P_0) ⊕ P_1) …) ⊕ P_{k-1}`。
L2 で `e ⊕ x = x`、L1 で括弧の付け替えが自由なので、`P_c = x_{b_c} ⊕ … ⊕ x_{b_{c+1}-1}`、`R = P_0 ⊕ … ⊕ P_{k-1} = x_0 ⊕ … ⊕ x_{n-1}`。`Array.fold_monoid` の結果 `(…(e ⊕ x_0) ⊕ …) ⊕ x_{n-1}` も L2 と L1 で同じ。証明は `k` と境界の値を使わない（4096・1024 の定数が将来変わっても成り立つ）。
**条件**: (a) 法則が成り立つこと（利用者の責任）。(b) 途中で `combine` がトラップしないこと（トラップする入力では、トラップすること自体は同じだが、どの要素のトラップが報告されるかは未規定）。(c) 浮動小数点を含まないこと（E1039 が拒否）。(d) `Monoid.empty`・`Monoid.combine` が外から見える作用（出力・共有状態への書き込み・時刻・乱数）を持たないこと。持つと `combine` を呼ぶ回数・順序・組み合わせが `Array.fold_monoid` と違う（`R` の括弧と、`e` を各チャンクと全体で足すこと）。**この条件の外では、結果の値も `Parallel.reduce` と同じく逐次と違いうる**。(d) は型検査では強制しない（決定事項 D13）。Phase 3 の `--report parallel`（§7）が `Parallel.fold` ごとの `parallel_safe` で、到達できる `Monoid` の関数が純粋と証明できるかを示す。

### §4 診断 `E1039`（Phase 1）

| 項目 | 内容 |
| --- | --- |
| コード・重要度 | `E1039`・error |
| kind | `monoid_float_instance`（G22 の `Diagnostic::with_kind`。G22 がなければ付けない） |
| 条件 | 制約 `Monoid<T>`（`T` は具象）の解決のとき、`T` が浮動小数点の数を**含む**: `Type::Binary(_)`（`f16`・`f32`・`f64`・`f128`）、`Type::Decimal(_)`（`d32`・`d64`・`d128`）、レーンが浮動小数点の `Type::Simd(_)` に、レコードのフィールド・union の payload・タプル・配列・リスト・`Vec`・`Maybe` の中身を辿って届く |
| 位置 | 制約の `span`（`Parallel.fold`・`Array.fold_monoid`・`Monoid.combine` を呼んだ地点。`std` の中ではない） |
| メッセージ | `no Monoid instance for {T}: it holds a floating-point number, whose operations are not associative, so Parallel.fold could differ from Array.fold_monoid; accumulate in integers (fixed point) or use Parallel.sum for a documented order`（`{T}` は `constraint.ty.display(types)`） |
| help（G22） | `accumulate in integers (fixed point), or use 'Parallel.sum' or 'Array.sum_pairwise' for floating-point sums` |
| 検査しないもの | 型変数を含む制約（`Array.fold_monoid` の本体）。使われない instance の宣言（使うと E1039） |

例（3 つ以上）:
1. `record Mean { total: f64, count: i64 }` に `instance Monoid<Mean>` を書き、`Array.fold_monoid (ref means)` を呼ぶ → E1039（`{T}` は `Main.Mean`）。
2. `Parallel.fold (ref [Monoids.Sum { value: 1.5 }, Monoids.Sum { value: 2.5 }])` → E1039（今は `E1005` の `no instance for Integer<f64>`）。
3. `record Pair { left: i64, right: f32 }` に instance を書いて `Parallel.fold` → E1039（入れ子の浮動小数点も同じ）。
4. 受理される: `record Fixed { micros: i64 }` の instance（整数だけ）、`Monoids.Sum<i64>`。

### §5 保証の表（Phase 2。`parallel.md` と `docs/language.md` に載せる）

| API | 結果 | 条件 |
| --- | --- | --- |
| `Parallel.fold` | `Array.fold_monoid` と**同一**（ビットまで） | `Monoid` の法則と、`empty`・`combine` に作用がないこと（§3）。浮動小数点を含む型は E1039 |
| `Parallel.map`・`Parallel.map_ref` | `Array.map`・`Array.map_ref` と**同一**（各要素の値と添字の順） | コールバックが**純粋**（§6）。純粋でなければ、副作用の回数・順序・実行するスレッドは**未規定** |
| `Parallel.init` | `new [T](n, f)` と**同一** | `f` が純粋 |
| `Parallel.for_each_chunk` | 各チャンクを先頭から順に処理する逐次のループと**同一** | `body` が純粋（自分のチャンクだけを書く。型が保証） |
| `Parallel.sum`（整数） | `Array.sum` と**同一** | 整数の加算は折り返しても結合的 |
| `Parallel.sum`（浮動小数点） | **決定的**（チャンクの規則で固定）だが `Array.sum` と**異なりうる** | 例: 上の `n = 8193`。同じ順序が要るなら `Array.sum_pairwise`・`Array.sum_kahan` |
| `Parallel.reduce` | **決定的**だが、結合律と単位元が成り立たなければ `Array.fold`・`Array.reduce` と異なる | 成り立つなら `Parallel.fold` と同じ |

どの API も、コールバックや `combine` がトラップする入力では、プログラムがトラップする点は同じだが、複数の要素がトラップするとき**どれが報告されるかは未規定**（既存の文書の「チャンク間の開始順序・完了順序は未規定」）。

### §6 純粋性と `parallel_safe`（Phase 2 の文書、Phase 3 の検査）

**純粋なコールバック**とは、A19 の規則（`pure def` と同じ効果の表と到達可能性）で、本体から到達できる関数に不純な源（外界・時計と乱数・並行・共有可変状態・GPU・動的ディスパッチ・利用者の `Drop`）がないもの。**関数値の呼び出し**（呼び先が静的に分からない）は、本チケットでは不純とみなす（`parallel_safe: false`、理由 `unknown_call`）。A19 の `pure def` は関数値の呼び出しを辺にも効果にもしないが、保証を壊しうるので保守的に倒す。
**`parallel_safe`**（呼び出し地点ごと）: その呼び出しのコールバックがすべて純粋と証明できるとき `true`。`Parallel.fold`・`Parallel.sum` は、`Monoid` の関数（std の instance は純粋）が純粋と証明できるとき `true`。`true` は「§5 の『同一』の条件のうち、純粋性が成り立つ」の意味で、`Parallel.reduce` の結合律までは含まない。

### §7 `tsuzuri check --report parallel`（Phase 3。出力は決定的）

**7.1 CLI**: `tsuzuri check [options] PATH --report parallel [--json]`。`--report` は新しいフラグで、値は `parallel` だけ。`check` 以外のサブコマンド（`build`・`run`・`test`・`fmt` を含む）に付けると引数エラー `--report is only valid with check`（`--check is only valid with fmt` と同じ形）。値がない・未知: `--report requires a report name; supported: parallel` と `unknown report 'X' for --report; supported: parallel`。2 回: `report parallel specified more than once`（文言は `--warn` の引数エラー `warn implicit-copy specified more than once` と同じ形。終了コードも同じ）。
`check` がエラーなら、診断だけを出して終了コード 1（報告は出さない）。成功なら、警告は従来どおり stderr に出し、**報告を stdout に**出して終了コード 0（提案があっても 0）。`--deny-warnings` で警告が失敗にしたときは、報告を出さずに終了コード 1。`--report` を付けない `check` は 1 byte も変わらない。

**7.2 出力**。位置は `path:line:column`（`path` は診断と同じ文字列。`tsuzuri check Main.tz` なら `./Main.tz`、`tsuzuri check demo` なら `demo/Main.tz`）。エントリは `(path の昇順（UTF-8 のバイト順）, 開始 offset, 終了 offset, kind（`suggestion` が `site` より先）)` の順。絶対パス・日時・環境の値を入れない。
`suggestion` の `span` は**呼び出しの名前**（`Array.map` の 9 バイト）の範囲、`site` の `span` は**呼び出し式の全体**（`Parallel.map (\value -> value * 2) values` の先頭から最後の引数の終わりまで）。

テキスト（1 行 1 件。`fix` の行は 2 スペースの字下げ）:

```text
parallel report: {suggestions} suggestions, {sites} sites
{path}:{line}:{column}: suggestion: {call} can be {replacement}: {why}
  fix (maybe): replace '{call}' with '{replacement}'
{path}:{line}:{column}: site: {call} callback is pure (parallel_safe: true)
{path}:{line}:{column}: site: {call} callback is not pure (parallel_safe: false): {reason}
```

JSON Lines（`--json`。stdout。1 行 1 オブジェクト。キーの順序は固定。値があるものだけ）: 最初の行が要約、続いてエントリ。

```text
{"schema":"tz-report-parallel/1","suggestions":2,"sites":2,"truncated":0}
{"kind":"suggestion","call":"Array.map","replacement":"Parallel.map","parallel_safe":true,"path":"./Main.tz","span":{"start":134,"end":143,"line":6,"column":19,"end_line":6,"end_column":28},"fixes":[{"title":"replace 'Array.map' with 'Parallel.map'","applicability":"maybe","edits":[{"path":"./Main.tz","span":{"start":134,"end":143,"line":6,"column":19,"end_line":6,"end_column":28},"text":"Parallel.map"}]}]}
```

キー: `kind`（`suggestion`・`site`）、`call`（呼び出しの名前）、`replacement`（`suggestion` だけ）、`parallel_safe`（bool）、`reason`（`site` で `parallel_safe` が `false` のときだけ）、`path`、`span`（診断の JSON と同じ 6 キー）、`fixes`（`suggestion` だけ。D-51 の `fixes` の形: `title`・`applicability`（常に `maybe`）・`edits`（`path`・`span`・`text`））。文字列のエスケープは `diagnostic::json_string`。`truncated` は上限（1,000 件）を超えて捨てた件数（0 のときも出す）。`site` の全キーの例は 7.5。

`reason`（`site` の `parallel_safe: false`）は、閉じた 8 つの文のどれか。原因・`reason` の文・`{effect}`（D-53 の効果の名前か、本チケットだけの `dyn`・`unresolved`・`unknown_call`・`analysis_limit`）の表は [付録 L7](F15-appendix-report.md#l7-reason-の表) にある（以下「§7.2 の表」はそれを指す）。

**7.3 提案の規則**（決定表。すべての条件を満たすときだけ `suggestion`。条件を満たさないものは何も出さない）:

| 呼び出し（`Array` の関数） | 置き換え先 | 追加の条件 | `{why}` |
| --- | --- | --- | --- |
| `Array.map f xs` | `Parallel.map` | `f` が純粋。要素型が `Copy` かつ `Send`、結果型が `Send`。`f` の捕捉が借用・`Atomic`・`Mutex` を含まない | `the callback is pure` |
| `Array.map_ref f xs` | `Parallel.map_ref` | 同上（要素の `Copy` は不要） | `the callback is pure` |
| `Array.fold_monoid xs` | `Parallel.fold` | 要素型が `Send`。呼び出しの特殊化から到達できる `Monoid` の関数が純粋 | `the result is the same by the laws of Monoid` |
| `Array.sum xs`（要素が整数型） | `Parallel.sum` | 要素型が整数（浮動小数点は**提案しない**: ビットが変わる） | `integer sums are the same in any order` |

共通の条件: (1) 呼び出しを含む関数が利用者のモジュールの関数（`FunctionOrigin.module == ModuleOrigin::User`）。(2) 呼び出し式（`TypedExpr` の `Call`）の `span.start` から `call` の長さのバイトの**ソースのテキスト**が `call`（`Array.map` の形）と一致する（別名・パイプ・括弧が入っていれば一致しないので提案しない）。その範囲が `suggestion` の `span`。(3) 入力の長さが静的に 4,096 未満と分かるなら提案しない（入力が配列リテラルで要素が 4,096 個未満、または `new [T](n, ..)` の `n` が整数リテラルで 4,096 未満。チャンクが 1 つで並列にならない）。長さが分からなければ提案する。(4) 同じ span・同じ `call` の重複（特殊化の複製）は 1 件にまとめる。(5) 入力・結果の要素型と、コールバックの捕捉の型が、関数値に届かない（`Parallel` が `E1013` にするものを提案しない）。
`site`（既存の `Parallel.init`・`map`・`map_ref`・`reduce`・`sum`・`fold`・`for_each_chunk` の呼び出し。利用者のモジュールのもの）は、条件なしに全部出し、`parallel_safe` は §6 で決める。

**7.4 上限**: エントリは 1,000 件まで（整列した後ろの超えた分を捨てて `truncated` に件数。要約の `suggestions`・`sites` は出した件数）。走査する式は関数ごとに 100,000 個まで（超えたらその関数を飛ばす。報告の失敗にしない）。A19 の到達可能性の上限（関数 4,096・式 2,097,152）を超えたら、そのコールバックは `parallel_safe: false`・理由 `analysis_limit`。

**7.5 例**（付録 H の実在のソースを使った**想定出力**。実装後に実出力で置き換える）。入力は `F15-appendix-report.md` の L1（`Main.tz`。16 行・484 バイト。`square`・`pure_total`・`traced`・`doubled`・`shown` の 5 つの関数）:

```text
$ tsuzuri check Main.tz --report parallel
parallel report: 2 suggestions, 2 sites
./Main.tz:6:19: suggestion: Array.map can be Parallel.map: the callback is pure
  fix (maybe): replace 'Array.map' with 'Parallel.map'
./Main.tz:7:5: suggestion: Array.sum can be Parallel.sum: integer sums are the same in any order
  fix (maybe): replace 'Array.sum' with 'Parallel.sum'
./Main.tz:13:21: site: Parallel.map callback is pure (parallel_safe: true)
./Main.tz:16:19: site: Parallel.map callback is not pure (parallel_safe: false): the callback reaches 'Debug.__print_string' (effect: debug)
```

同じ入力の `--json` の全 5 行は L4 の `BASIC_JSON`（`Array.map` は 134〜143、`Array.sum` は 162〜171、13 行目の `Parallel.map (\value -> value * 2) values` は 341〜382、16 行目の `Parallel.map (\value -> Debug.trace value) values` は 434〜483）。

10 行目の `Array.map (\value -> Debug.trace value) values` は、コールバックが不純なので何も出さない。`Array.map square values`（6 行目）は `square` が純粋なので提案する。`Debug.trace` は `Debug.print` を呼び、`Debug.print` が組み込み関数 `Debug.__print_string` に届くので、`reason` は組み込み関数の名前（A19 の `Effect::Builtin`）になる。

### §8 facts（Phase 4。G26）

G26 の facts の行（`facts/<Module>.jsonl`。D-52）に、キー `parallel` を足す（`specializations` の次・`truncated` の直前。G26 の規則で**常に出し**、提案も呼び出し地点もなければ `[]`。`KbModel::fact_keys()` に足し、G26 の golden を更新する）。要素は `{"call","kind","parallel_safe","reason"?,"span"}`（`kind` は `suggestion`・`site`。`span` は D-52 の `{"path","start","end","line","column"}`）。行は呼び出しを含む関数ごと。配列の上限は 100 件（D-52 の規則。超えたら `truncated` に足す）。Phase 3 の `parallel_report::build` の結果を、関数の ID ごとに分けて渡す（G26 の `Capture` と同じ形）。

### §9 互換性

- **`docs/stability.md`**: 診断コード `E1039` は新しい（意味の再利用なし）。新しい CLI フラグ `--report` は追加で互換。std の公開 API の追加（`Array.fold_monoid`・`Parallel.fold`・`Monoid`・`Monoids`）は互換だが、**新しい std モジュール名 `Monoid`・`Monoids` は予約され、同名の利用者のモジュールは `E1011` になる**（「既知の例外」。release notes に書く）。
- 既存のプログラム: `Monoid` というクラスを自分のモジュールに持つプログラムは、自モジュールが優先されて動く。ただし利用者のモジュール名 `Monoid`・`Monoids` は拒否される。IR・ABI・WASM・edition・予約語は「変えてはいけない意味」のとおり。

## 設計

### 設計 1: Phase 1（`Monoid` と `Parallel.fold`）

std の 4 ファイルと `src/stdlib.rs` の変更は付録 A〜D（手順 3）。Rust の変更は 2 か所だけで、`src/check.rs` の `impl Type`（`holds_host_state` の隣）に `holds_float`、`src/polymorph.rs` の `ClassEnv::normalize` の `while let Some((current, depth, finished)) = pending.pop()` の中、`let class = &self.declarations[current.class];` の直後に E1039（手順 4）。

**`Type::holds_float`**（`src/check.rs`。`holds_host_state` と同じ `types.reaches` を使う）:

```rust
    /// Whether a value of this type holds a floating-point number (F15): a binary or decimal float, or a SIMD vector of them.
    pub(crate) fn holds_float(&self, types: &TypeContext<'_>) -> bool {
        types.reaches(self, |ty| {
            ty.is_float() || matches!(ty, Type::Simd(vector) if vector.element().is_float())
        })
    }
```

`vector.element()` が `self` を取る（`SimdType` が `Copy` でなければ `.clone()` を付ける）。コンパイルが通る形に直すだけで、意味は変えない。

**E1039**（`src/polymorph.rs`。`normalize` のループの中。`class.name` はモジュール修飾の `Monoid.Monoid`）:

```rust
            if class.name == "Monoid.Monoid"
                && variables(&current.ty).is_empty()
                && current.ty.holds_float(types)
            {
                return Err(Diagnostic::new(
                    "E1039",
                    format!(
                        "no Monoid instance for {}: it holds a floating-point number, whose operations are not associative, so Parallel.fold could differ from Array.fold_monoid; accumulate in integers (fixed point) or use Parallel.sum for a documented order",
                        current.ty.display(types)
                    ),
                    current.span,
                ));
            }
```

この位置にするのは、`normalize` が、instance が見つかった制約（`Monoid<Sum<'a>>` が `Sum<f64>` に合う）では `validate_instance` を呼ばず、文脈の制約（`Integer<f64>`）へ進むため。E1039 は、外側の `Monoid<T>` を見る最初の機会で出す。G22 が入っていれば、`.with_kind("monoid_float_instance")` と `.with_help("accumulate in integers (fixed point), or use 'Parallel.sum' or 'Array.sum_pairwise' for floating-point sums")` をつなぐ。

### 設計 2: Phase 2（保証の表とテスト）

Rust の変更はない。付録 F の fixture と付録 G の suite `parallel_equivalence` を足し、`parallel.md`・`docs/language.md` に §5 の表を載せる（「ドキュメント」）。

### 設計 3: Phase 3（`--report parallel`）

**新規 `src/parallel_report.rs`**（`src/copies.rs` を手本にする。`src/lib.rs` に `pub mod parallel_report;`）: 定数 `MAX_ENTRIES = 1000`・`SMALL_INPUT = 4096`・`MAX_NODES_PER_FUNCTION = 100_000`、`EntryKind`（`Suggestion`・`Site`）、`Entry`（`kind`・`call`・`replacement`・`function`・`span`・`parallel_safe`・`reason`・`why`）、`Report { entries, truncated }`、トレイト `Sources`（`text(span)`・`path(span)`）、関数 `build(module, sources)`・`render_text`・`render_json` の**宣言の全文は [付録 L8](F15-appendix-report.md#l8-srcparallel_reportrs-の型と関数の宣言) をそのまま写す**。`Entry.function` は呼び出しを含む関数の `CheckedModule::functions` の添字（Phase 4 が使う）。`span` は suggestion が呼び出しの名前、site が呼び出し式の全体。`reason` は site で `parallel_safe == false` のとき、`why` は suggestion のとき。

**`build` の手順**:
1. `entries = BTreeSet<(usize /*source*/, usize /*start*/, usize /*end*/, EntryKind, &'static str)>` で重複を除く。
2. `module.functions` の各関数 `function`（`function.origin.module == ModuleOrigin::User`。生成された helper・ラムダも含む）について、`function.body` を明示のスタックで走査する（再帰しない。`TypedExpr::children()`）。式の数が `MAX_NODES_PER_FUNCTION` を超えたらその関数を飛ばす。
3. **呼び出しの検出**。式 `e` が次のどちらかのとき、`call` の名前と「コールバックの根」を決める（下の表）。
   - `TypedExprKind::Parallel(builtin, arguments)`（`src/check.rs` の `call_kind` が、完全に適用された `Parallel.init`・`map`・`map_ref`・`reduce`・`for_each_chunk` から作る）。`call` は `builtin.name()`（`"Parallel.map"` の形）、`Site`、`span` は `e.span`（呼び出し式の全体）。
   - `TypedExprKind::Call(callee, arguments)` で `callee.kind` が `TypedExprKind::Function(FunctionRef::User(id))`。`name = module.functions[id].qualified_name()` から、最初の `.$mono` 以降を落とす。`name` が `Parallel.sum`・`Parallel.fold` なら `Site`（`span` は `e.span`）。`Array.map`・`Array.map_ref`・`Array.fold_monoid`・`Array.sum` なら、手順 4〜7 の条件がすべて成り立つとき `Suggestion`（`span` は手順 7）。

   | 呼び出し | コールバックの根（`roots`） |
   | --- | --- |
   | `Parallel(ParallelInit, [length, f])` | `arguments[1]` |
   | `Parallel(ParallelMap, [f, input])`・`Parallel(ParallelMapRef, [f, input])` | `arguments[0]` |
   | `Parallel(ParallelReduce, [identity, reducer, input])` | `arguments[1]` |
   | `Parallel(ParallelForEachChunk, [size, body, view])` | `arguments[1]` |
   | `Parallel.sum`・`Parallel.fold` の `Call` | `module.functions[id].body` を走査して最初に見つかる `Parallel(ParallelReduce, args)` の `args[1]`（std の本体は `Parallel.reduce` を 1 回呼ぶだけ） |
   | `Array.map`・`Array.map_ref` の `Call` | `arguments[0]` |
   | `Array.fold_monoid` の `Call` | 関数そのもの（`Function(User(id))` として評価。本体が `Monoid.combine` の instance へ届く） |
   | `Array.sum` の `Call` | なし（整数の加算は純粋。`parallel_safe` は `true`） |

4. **コールバックの純粋性**（根がある呼び出しだけ）。根の式 `r` を次の表で上から順に評価し、最初に当たった行で決める。

   | `r` | 結果 |
   | --- | --- |
   | 呼び出しのコールバック以外の引数の型か `e.ty` が、関数型 `Type::Function(..)` に届く（`TypeContext::reaches` で探す。`Type::holds_float` と同じ形）。または `r` が `Closure(_, captures)` で、ある `captures[i].ty` が同じく届く | 不純。理由は §7.2 の表の「未知の関数値」 |
   | `Closure(id, _)`・`Function(FunctionRef::User(id))` | `pure_rules::reachable_effect(module, id)` が `Ok(None)` なら純粋、`Ok(Some(effect))` なら不純（理由は §7.2 の表）、`Err(LimitExceeded)` なら不純（理由は解析の上限） |
   | `Function(FunctionRef::Builtin(instance))` | `pure_rules::builtin_impure(instance.builtin)`（D-53 の窓口。A19 のままなら `builtin_effect(b).is_some()`）が偽なら純粋、真なら不純（理由は `Builtin(name, _)` の行。`name` は `instance.builtin.name()`） |
   | そのほか（局所変数の関数値） | 不純。理由は §7.2 の表の「未知の関数値」 |

   関数値の呼び出し（`Call` の callee が `Function`・`Closure` でない）は、A19 と同じく辺にも効果にもしない。**外から入る関数値**（捕捉・引数の型・結果の型・局所変数）は 1 行目と 4 行目が塞ぐので、`Array.map` のような std の高階関数が `Closure` を呼ぶ形（作った場所が到達する本体の中にある）は不純にならない。
5. **型の条件**（`Suggestion` だけ）。`input` は最後の引数。`Type::Reference(inner, _)` なら `inner` を取り、`Type::Array(element)` から要素型 `T` を得る（この形でなければ提案しない）。結果の要素型 `U` は、`Array.map`・`Array.map_ref` では `e.ty` の `Type::Array(U)`、`Array.fold_monoid` では `e.ty` 自身。
   - `Array.map`: `T.is_copy(types)`・`T.can_send(types)`・`U.can_send(types)`。
   - `Array.map_ref`: `T.can_send(types)`・`U.can_send(types)`。
   - `Array.fold_monoid`: `U.can_send(types)`。
   - `Array.sum`: `T` が `Type::Integer(..)`。
   - `Closure(_, captures)` のコールバック: どの `captures[i].ty` も、`Type::Reference(..)` に届かず（`reaches`）、`holds_cell(types)` でない。
6. **長さの条件**: `input` の式から `TypedExprKind::Borrow(inner, _)` を 1 つ剥がし、`TypedExprKind::Array(elements)` で `elements.len() < SMALL_INPUT`、または `TypedExprKind::NewArray(length, _)` で `length.kind` が `TypedExprKind::Int(n)` かつ `n < SMALL_INPUT` なら提案しない。局所変数を辿らない（長さが分からなければ提案する）。
7. **テキストの条件**: `start = e.span.start`、`end = start + call.len()`。`sources.text(e.span).get(start..end) == Some(call)` のときだけ提案し、`span` は `e.span` の `start`・`end` を置き換えたもの（`source` は同じ）。一致しなければ提案しない。
8. 整列して上限（1,000）で切る。`truncated` は捨てた件数。`site` の `parallel_safe`・`reason` は手順 4 の結果で、`suggestion` の `parallel_safe` は、根がなければ `true`、あれば手順 4 の結果。

**A19 から使う関数**（`src/pure_rules.rs`）:

```rust
/// The first impure source that the function `root` (an id in `module.functions`) reaches: A19's per-function scan and breadth-first search of `validate`, started from `root` instead of from the `pure def`s. `Err` when the limits (4,096 functions, 2,097,152 nodes) are exceeded.
pub(crate) fn reachable_effect(module: &CheckedModule, root: usize) -> Result<Option<Effect>, LimitExceeded>;
pub(crate) struct LimitExceeded;
```

A19 の実装が同じ働きの関数を持つならそれを使う。なければ手順 9 で足す（約 40 行。`validate` の手順 2・3 の本体を、根の集合を引数に取る内部関数へ切り出し、`validate` と `reachable_effect` の両方がそれを呼ぶ形にする。`validate` の出力は 1 byte も変えない）。あわせて `pub(crate) fn builtin_impure(builtin: Builtin) -> bool`（`builtin_effect(builtin).is_some()`）を足す。**この 2 つと `Effect` は D-53 の「純粋性の問い合わせの窓口」で、A20 の経路 A のあとも名前と形が変わらない**（`pure_rules.rs` の薄い関数として残り、中身は `effects::summarize` の `Witness`）。本チケットは `builtin_effect`・`Category` を直接呼ばない。`src/check.rs` の `mod pure_rules;` を `pub(crate) mod pure_rules;` に変えて `crate::parallel_report` から使う。`Effect::Builtin` の `&'static str` が `Builtin::name()` の文字列でなければ、`reachable_effect` の中で `name()` の文字列に直す（§7.2 の表の `{name}`）。

**`src/main.rs`**（手本は `--warn`。`warn_implicit_copy` の出現 7 か所を `grep -n warn_implicit_copy src/main.rs` で数える）: `Arguments` に `report_parallel: bool` を足し、`--report` を `--warn` と同じ形で解析する（引数エラーの文言は仕様 §7.1）。引数の検査に `if report_parallel && action != Action::Check { return Err("--report is only valid with check".into()); }`（`--check is only valid with fmt` の隣）。`HELP` の `tsuzuri check source.tz|source.tt|source.tc|directory [--json]` の行を `... [--json] [--report parallel]` にする。`check` の成功の後、`print_diagnostics(&warnings, &project, arguments.json)` と `deny_warnings` の失敗判定（`return ExitCode::FAILURE`）の**後**、`if arguments.action == Action::Test` の**前**に、`arguments.report_parallel` なら `tsuzuri::parallel_report::build(&module, &project)` を `render_json`（`arguments.json`）か `render_text` で **stdout** に `print!` する。単体テスト `parses_report_parallel`（`parses_warn_implicit_copy` の隣。付録 L5。手順 11）。
**`src/driver.rs`**: `Project::source_of(&self, span: Span) -> &SourceFile`（`let index = span.source.unwrap_or(self.root); self.sources.get(index).unwrap_or_else(|| &self.manifests[index - self.sources.len()])`。`source_for` は `self.source_of(error.span)` を返す形に直す）と、`impl parallel_report::Sources for Project`（`text` は `&self.source_of(span).text`、`path` は `self.source_of(span).path.to_string_lossy().into_owned()`）。行・桁は `diagnostic::location(source, offset)`（桁は文字数）、JSON の文字列は `diagnostic::json_string`、`span` の 6 キーの書き方は `src/trap.rs` の `write!` が手本。

### 設計 4: Phase 4（facts）

G26 の facts を書く関数（G26 Phase 3 が作る `write_facts`）に、`parallel_report::build` の結果を関数の ID ごとに整列して渡し、行に `parallel` を足す（仕様 §8）。G26 のテストに、付録 H の例（`pure_total` が `suggestion` 2 件、`shown` が `parallel_safe: false`）の行を足す。

## 前提とする他チケットのインターフェース

| チケット | 使うもの | 未実装のとき |
| --- | --- | --- |
| A19（Phase 3 の前提） | D-53 の窓口: `src/pure_rules.rs` の `reachable_effect`・`builtin_impure`・`Effect`（`Host(String)`・`Builtin(&'static str, Category)`・`Concurrency`・`Dyn`・`UserDrop(String)`・`Unresolved`）。A19 のままなら、その下の `builtin_effect(Builtin) -> Option<Category>`、関数ごとの局所の走査 `Scanned` と幅優先探索（「検証のアルゴリズム」の手順 2・3）から作る（手順 9）。A20 の経路 A のあとは窓口が残る | **Phase 3 を出荷しない**（停止条件 5）。Phase 1・2 は影響を受けない |
| G22（任意） | `Diagnostic::with_kind`・`with_help`、`Fix { title, applicability, edits }`・`Applicability::Maybe`・`Edit { span, text }`（D-51） | E1039 は `Diagnostic::new` のメッセージだけ（help の文は本文に含めてある）。報告の JSON は D-51 の `fixes` と同じ形を自前で書く（`Fix` の型は使わない） |
| G23 | kind の登録簿 | kind `monoid_float_instance` を登録する（`E1039` の 1 件）。G23 が未実装でも E1039 は動く |
| G26（Phase 4 の前提） | facts の行（D-52）と、その書き出し（Phase 3 の `write_facts`・`fact_keys()`） | Phase 4 を出荷しない |
| G34（Phase 3）・G35（Phase 2） | `tests/std_examples.rs` の `FLOORS`（`SOURCES` の全モジュールが要る）と `every_public_std_function_has_an_example`、`tests/std_tags.rs` の `TAGGED`。GUIDE-LITE §7 | 未実装: 何もしない。実装済み: `Monoid`・`Monoids` を `FLOORS` に 0 で足し（クラス・record・instance は `program.functions` に入らず数えられない）、`Array.fold_monoid`・`Parallel.fold` に G34 の形式の例を付け、`TAGGED` に `Array` があれば `Array.fold_monoid` に `@complexity O(n)`・`@see Array.fold` を付ける |

効果の名前は D-53 に固定されている（A20 が未実装でも使える）。提案は `maybe` なので G24 の `tsuzuri fix` は適用しない。

## 実装手順

共通の約束: 各手順の終わりで `cargo build --release --locked` が通り、その手順の「確認」が緑であること（`std` を変えた手順は `include_str!` で埋め込むので必ず再ビルドする）。確認が赤のまま次へ進まない。Rust を変えた手順の終わりには `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings`（README のとおり）も通す。付録のコード・fixture・文書は**そのまま写し、書き換えない**。

### Phase 1: `Monoid`・`Monoids`・`Parallel.fold`・E1039

### 手順 1: ベースライン
- 変更: なし。
- 内容: HEAD で `cargo build --release --locked` と `RUST_MIN_STACK=4194304 cargo test --locked` を通す。`cp target/release/tsuzuri target/baseline-tsuzuri`（手順 13 の「出力が 1 byte も変わらない」確認に使う。`target/` は git の管理外）。
- 確認（HEAD の実測値。違えば止めて報告）:
  - `node tests/features.mjs target/release/tsuzuri parallel` → `features: 36 cases passed`。
  - `grep -n "RESERVED_MODULES.len()" src/stdlib.rs` → `assert_eq!(RESERVED_MODULES.len(), 53);`。
  - `git ls-files | grep '\.ll$' | grep -vc '^src/runtime/'` → `0`（IR を固定する追跡ファイルはない）。
  - `node scripts/check-docs.mjs <ページ>`（`_tsuzuri/language-reference/` 以下）→ `built-in-types-and-modules/parallel.md`: `17 links, 4 checked examples, 8 native runs`、`built-in-types-and-modules/array.md`: `9 links, 11 checked examples, 22 native runs`、`index.md`: `114 links, 0 checked examples`、`compiler/usage.md`: `45 links, 5 checked examples, 4 native runs, 1 test projects`、`compiler/diagnostics.md`: `12 links, 0 checked examples`。

### 手順 2: テスト先行（fixture・suite・Rust のテスト）
- 変更: `tests/fixtures/monoid/Main.tz`（新規。付録 E）、`tests/features.mjs`（付録 G の suite `monoid` だけ。`arena: {` の直前）、`tests/monoid.rs`（新規。付録 J）。
- 内容: 付録の全文をそのまま写す。suite `parallel_equivalence` は手順 6 で足す。
- 確認: `node tests/features.mjs target/release/tsuzuri monoid` が失敗し、最初の診断が `error[E1016]: unknown type class 'Monoid'`（`std` にクラスがないので、赤が正しい）。`cargo test --locked --test monoid` が 4 件とも失敗する。

### 手順 3: `std` の 4 ファイルと `src/stdlib.rs`
- 変更: `std/Monoid.tt`・`std/Monoids.tz`（新規。付録 A・B）、`std/Array.tz`（`def fold` の直後に付録 C の `fold_monoid`）、`std/Parallel.tz`（末尾に付録 C の `fold`）、`src/stdlib.rs`（付録 D: `SOURCES` に 2 行、`RESERVED_MODULES` に `Monoid`・`Monoids`、`OPT_IN` に `Monoids`（`names` は `["Monoids"]`）、テスト `reserves_the_d07_table` の `53` を `55` に）。
- 内容: 写すだけ。`Monoids` の各 instance の文脈・`empty`・`combine` は表（仕様 §2）のとおりで、書き換えない。
- 確認:
  - `cargo build --release --locked`（約 3 分）。`cargo test --locked --lib stdlib` が緑（`opt_in_modules_are_reached_only_through_their_names`・`every_std_module_type_checks_when_named`・`reserves_the_d07_table` を含む）。
  - `node tests/features.mjs target/release/tsuzuri monoid` → `features: 90 cases passed`。`target/release/tsuzuri test tests/fixtures/monoid` → `11 passed; 0 failed; 0 ignored`。
  - `cargo test --locked --test monoid` → 3 件が緑で、`float_monoids_are_rejected_where_they_are_used` だけが赤（`E1005` か、受理されて `expect_err` が失敗する。E1039 は次の手順）。
  - `node tests/features.mjs target/release/tsuzuri parallel` → `features: 36 cases passed`（不変）。

### 手順 4: `Type::holds_float` と E1039
- 変更: `src/check.rs`（`holds_host_state` の隣に `holds_float`）、`src/polymorph.rs`（`ClassEnv::normalize` に E1039）。
- 内容: 設計 1 のコードを写し、`vector.element()` の取り方だけをコンパイルが通る形に直す。G22 が済んでいれば `.with_kind("monoid_float_instance")` と `.with_help(..)` をつなぐ。条件は仕様 §4（`class.name == "Monoid.Monoid"`・型変数なし・`holds_float`）。
- 確認:
  - `cargo build --release --locked`。`cargo test --locked --test monoid` が 4 件とも緑。
  - 実出力（付録 J の `MEAN` に `let means = [Mean { total: 1.5, count: 1 }]`・`let total = Array.fold_monoid (ref means)`・`total.count` を続けた `Main.tz` で `tsuzuri check Main.tz`。**想定**）: `./Main.tz:7:13: error[E1039]: no Monoid instance for Main.Mean: it holds a floating-point number, whose operations are not associative, so Parallel.fold could differ from Array.fold_monoid; accumulate in integers (fixed point) or use Parallel.sum for a documented order`。
  - `RUST_MIN_STACK=4194304 cargo test --locked` が全体で緑。`monoid` suite は 90 件のまま。

### 手順 5: 文書（Phase 1）
- 変更: 付録 I（`monoid.md`。新規）、付録 K の K1 の 1・2・4（`parallel.md`）・K2（`array.md`・`index.md`）・K3・K4・K5・K7 の Phase 1 の 2 か所。
- 内容: 付録のとおりに写す。`docs/language.md` と `README.md` は同じ変更で揃える（AGENTS.md）。
- 確認: `node scripts/check-docs.mjs` → `monoid.md`: `10 links, 2 checked examples, 4 native runs`、`parallel.md`: `19 links, 4 checked examples, 8 native runs`、`array.md`: `10 links, 11 checked examples, 22 native runs`、`index.md`: `115 links`、`diagnostics.md`: `12 links, 0 checked examples`。`grep -n "Parallel.fold" README.md docs/language.md` が 2 ファイルでヒット。**Phase 1 の完了**（受け入れ条件 Phase 1）。

### Phase 2: 保証の表とテスト

### 手順 6: fixture `parallel_equivalence` と suite
- 変更: `tests/fixtures/parallel_equivalence/Main.tz`（新規。付録 F）、`tests/features.mjs`（付録 G の suite `parallel_equivalence`）。
- 内容: 写す。Rust の変更はない。この fixture は `Monoid` を使わないので、Phase 1 より先に出荷してもよい。
- 確認: `node tests/features.mjs target/release/tsuzuri parallel_equivalence` → `features: 48 cases passed`。`parallel` は 36 のまま。

### 手順 7: 文書（Phase 2）
- 変更: 付録 K1 の 3（`parallel.md` に節「逐次と同じ結果になる条件」）。
- 内容: 写す。Phase 1 より先に出荷するなら、節の表の `fold` の行と、`Monoid` へのリンクを除く。
- 確認: `node scripts/check-docs.mjs` → `parallel.md`: `19 links, 6 checked examples, 12 native runs`（節の 2 つの例は `true` と `false`）。`docs/language.md` の保証の表（K3）が入っている。

### Phase 3: `--report parallel`（A19 が done であること）

### 手順 8: テスト先行（fixture と `tests/parallel_report.rs`）
- 変更: `tests/fixtures/parallel_report/basic/Main.tz`・`rules/Main.tz`・`error/Main.tz`（新規。`F15-appendix-report.md` の L1〜L3）、`tests/parallel_report.rs`（新規。L4）。
- 内容: 写す。`rules` は `Monoids` を使うので Phase 1 が入っていること（なければ 19〜20 行目の `sums` と 28〜29 行目の `folded` を fixture から外し、`RULES_TEXT` の該当 2 件と見出しの `3 suggestions, 2 sites` を直し、`the_suggested_edits_compile_and_leave_no_suggestions` の `4` を `3`、`7` を `5` に直す）。
- 確認: `cargo test --locked --test parallel_report` が 8 件とも失敗する（`--report` が未知のフラグ）。各 fixture の `(cd tests/fixtures/parallel_report/<名前> && ../../../../target/release/tsuzuri check Main.tz)` は `basic`・`rules` が出力なし（成功）、`error` が `./Main.tz:1:18: error[E1003]: expected i64, found bool`。

### 手順 9: `pure_rules::reachable_effect`（A19 の上）
- 前提の確認: `ls src/pure_rules.rs`（A20 の経路 A のあとも残る）、`grep -n "fn reachable_effect\|fn builtin_impure" src/pure_rules.rs`（D-53 の窓口。なければこの手順で足す。A19 のままなら足すときに `grep -n "fn builtin_effect\|enum Effect\|struct Scanned" src/pure_rules.rs` も見る）。`pure_rules.rs` がなければ停止条件 5、形が違えば停止条件 12。
- 変更: `src/pure_rules.rs`（`reachable_effect`・`LimitExceeded`。`validate` の手順 2・3 の本体を、根の集合を取る内部関数に切り出して両方から呼ぶ）、`src/check.rs`（`mod pure_rules;` を `pub(crate) mod pure_rules;` に）。
- 内容: 設計 3。`validate` の出力は 1 byte も変えない。
- 確認: A19 の既存テスト（`cargo test --locked --test pure_functions` と `src/pure_rules.rs` の単体テスト）が緑のまま。`src/pure_rules.rs` の `#[cfg(test)]` に 3 件（A19 の `honors_the_exact_specialization_limit` と同じ書き方）: `reachable_effect_finds_the_print_builtin`（`Debug.print` を呼ぶ関数を根にして `Ok(Some(Effect::Builtin("Debug.__print_string", _)))`）、`reachable_effect_accepts_a_pure_function`（`Ok(None)`）、`reachable_effect_reports_the_limit`（小さい `Limits` で `Err(LimitExceeded)`）。

### 手順 10: `Sources` と `parallel_report::build`
- 変更: `src/parallel_report.rs`（新規。`Sources`・`Entry`・`Report`・`build`。`render_*` は手順 11 まで `String::new()`）、`src/lib.rs`（`pub mod parallel_report;`）、`src/driver.rs`（`Project::source_of`、`impl Sources for Project`。`source_for` は `source_of` を呼ぶ形）。
- 内容: 設計 3 の手順 1〜8。
- 確認: `src/parallel_report.rs` の `#[cfg(test)]` の 2 件が緑。`builds_the_basic_report`（`include_str!("../tests/fixtures/parallel_report/basic/Main.tz")` を `analyze` し、1 ソースだけを返す `Sources` で `build`。`(kind, call, start, end, parallel_safe)` の 4 件が L4 の `BASIC_JSON` と同じ）と `builds_the_rules_report`（`rules` の 7 件の `(kind, call, 行)` が L4 の `RULES_TEXT` と同じ）。手順 8 のテストは手順 11 まで赤のまま、ほかは緑。

### 手順 11: 出力と CLI
- 変更: `src/parallel_report.rs`（`render_text`・`render_json`）、`src/main.rs`（`report_parallel`・解析・引数の検査・`HELP`・配線。単体テスト `parses_report_parallel` は付録 `F15-appendix-report.md` の L5）。
- 内容: 仕様 §7.1〜§7.2、設計 3 の `main.rs`・`driver.rs`。
- 確認: `cargo test --locked --test parallel_report` が 8 件とも緑。`cargo test --locked --bin tsuzuri parses_report_parallel` が緑。`target/release/tsuzuri --help | grep -c "report parallel"` が `1`。仕様 §7.5 の実出力が想定出力と一致する（違えば、実装を直し、期待値は直さない）。

### 手順 12: 文書（Phase 3）
- 変更: 付録 K6 の 4 つ（`usage.md`・`docs/language.md`・`docs/architecture.md`・`parallel.md` の文）と K7 の Phase 3 の文（`README.md`）。
- 確認: `node scripts/check-docs.mjs` → `parallel.md`: `19 links, 6 checked examples, 12 native runs`、`usage.md`: `45 links, 5 checked examples, 4 native runs, 1 test projects`（変更前と同じ）。

### 手順 13: Phase 3 の全体の確認
- 内容: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 cargo test --locked`。`--report` なしの出力が不変であることを、全 fixture で確かめる（bash）:

```bash
for d in tests/fixtures/*/; do
  diff <(target/baseline-tsuzuri check "$d" 2>&1) <(target/release/tsuzuri check "$d" 2>&1) || echo "changed: $d"
done
```

- 確認: `changed:` の行が 1 つも出ない（Phase 1 の std の追加で診断が変わる fixture があれば、`E1005` の文言の差を読み、`Monoid` に由来しなければ止めて報告）。`for` の出力が空。

### Phase 4: facts（G26 Phase 3 が done のとき）

### 手順 14: facts の `parallel` キー
- 前提の確認: `ls src/kb/facts.rs` と `grep -n "fn fact_keys" src/kb/*.rs`。なければ着手しない（停止条件 9）。
- 変更: `src/kb/model.rs`・`src/kb/facts.rs`（`parallel` を `truncated` の直前に足す）、`tests/kb.rs`。
- 内容: `parallel_report::build(&module, &project)` を 1 回呼び、`Entry.function` を G26 の根の写像で宣言に直し、行ごとに `[{"call","kind","parallel_safe","reason"?,"span"}]` を出す（仕様 §8）。100 件を超えたら `truncated.parallel`。
- 確認: G26 のテスト `facts_rows_carry_parallel_entries`（fixture `basic`）: `pure_total` の行に `suggestion` が 2 件、`doubled` に `site`（`parallel_safe: true`）が 1 件、`shown` に `site`（`false`）が 1 件、`square`・`traced` の行は `"parallel":[]`。`cargo test --locked` が緑。受け入れ条件がすべて満たされる。完了報告に、新しく予約した std モジュール名（`Monoid`・`Monoids`）を書く。

## テスト計画

| テスト | 場所 | 入力と期待値 |
| --- | --- | --- |
| E2E `monoid`（90 件） | `tests/features.mjs`・fixture `monoid`（付録 E・G） | native と WASM の `-O0`・`-O3`。参照は JS の BigInt のループ（要素型ごとの折り返しを含む）。`Sum`・`Product`・`MaxOf`・`MinOf`・ビット演算・`All`・`Any`・非可換な `Affine` を、要素数 0・1・4095・4096・4097・8193・12288・100000 で比べる |
| E2E `parallel_equivalence`（48 件） | 同上（付録 F・G） | `map`・`map_ref`・`init`・`for_each_chunk`・整数の `sum` が逐次と一致、浮動小数点の `sum` は n = 8193 で `Array.sum` と異なる（`413670.50000000006` と `413670.5`）ことを固定 |
| `tsuzuri test tests/fixtures/monoid` | fixture の `test` 宣言（付録 E） | `11 passed; 0 failed; 0 ignored`。法則（結合律・左右の単位元）を `Gen` の性質テストで、`Sum<i64>` と非可換な `Affine` に |
| `tests/monoid.rs`（4 件。付録 J） | `float_monoids_are_rejected_where_they_are_used`（8 ソース: `Mean`・`Pair`・`Optional`（`Maybe<f64>`）・`Money`（`d64`）・`Lanes`（`f32x4`）・`Monoids.Sum<f64>`・直接の `Monoid.combine`・`Monoids.MaxOf<f64>`。コード・メッセージ全文・位置）、`integer_monoids_and_unused_float_instances_are_accepted`（受理と IR の決定性）、`monoids_are_qualified_and_the_module_names_are_reserved`（`E1004`・`E1011`）、`law_tests_pass_at_o0_and_o3` | 期待値は仕様 §4 と付録 J |
| 単体 `src/stdlib.rs` の 3 件 | `opt_in_modules_are_reached_only_through_their_names`・`every_std_module_type_checks_when_named`・`reserves_the_d07_table`（55） | 緑。`reserves_the_d07_table` の期待値が **53 → 55**（既存テストの唯一の期待値の変更） |
| 変異 | 手順 3 の後、`std/Parallel.tz` の `fold` の `combine` の引数を `right left` に入れ替えて `monoid` suite を走らせる | 可換な `Sum` では通り、非可換な `Affine` の場合が失敗する（付録 H の試作では要素数 2 で `2 → -8`、4095 で `-9215219183495582855`）。確かめたら戻す |
| `tests/parallel_report.rs`（8 件。Phase 3。`F15-appendix-report.md` の L4） | 基本の報告（text・JSON の全文）・`rules` の提案・決定性・フラグなしの不変・エラー時に報告なし・フラグの誤り 4 種・提案を適用して再検査・1,000 件の上限 | 期待値は L4 の定数。提案を適用して再検査すると `suggestions` が 0・`sites` が 7 になる |
| 単体 `src/parallel_report.rs` の 2 件、`src/pure_rules.rs` の 3 件、`src/main.rs` の `parses_report_parallel` | 手順 10・9・11 | 手順の「確認」のとおり |
| G26 `facts_rows_carry_parallel_entries`（Phase 4） | `tests/kb.rs` | 手順 14 |

**既存テストへの影響**: 期待値が変わるのは `reserves_the_d07_table`（53 → 55）だけ。IR は、連番の接尾辞（`$intrinsic.Display.display.<N>`）だけが `std` の宣言数で変わる。IR を固定する追跡ファイルは HEAD にない（手順 1 の `git ls-files` が 0）ので、テストは壊れない。`--report` を付けない `check` の出力は手順 13 の `diff` で不変を確かめる。

## ドキュメント

更新するもの（全文と文言は付録 I・K・K7）:
- 新規 `_tsuzuri/language-reference/built-in-types-and-modules/monoid.md`（付録 I）と `index.md` のリンク。
- `parallel.md`（`fold` の行・保証の表・例 2 つ・関連項目）、`array.md`（`fold_monoid` の行）、`compiler/diagnostics.md`（`E1039` の行・範囲の文）、`compiler/usage.md`（`check` の `--report parallel`。Phase 3）。
- `docs/language.md`（データ並列 API の表の `fold`・節「逐次との同一と `Monoid`」・`--report parallel`。Phase 3）、`docs/architecture.md`（`OPT_IN` の列挙・`RESERVED_MODULES` の数 55・段落 `Monoid（F15）`・`src/parallel_report.rs` の行。Phase 3）、`README.md`（データ並列 API の項）。
- `docs/stability.md` は変えない（「std へのモジュールの追加」は既知の例外として既にある。完了報告に新しい予約名を書く）。`_features/GUIDE.md` の台帳（診断コード `E1039` の使用・kind・`--report`・std の名前 `Monoid`・`Monoids`・決定 D1〜D12）は統括者が更新する。
- 検証: `node scripts/check-docs.mjs` を、変更した `_tsuzuri` のページすべてに実行する（数は手順 5・7・12）。

## 受け入れ条件

**Phase 1**
- [ ] `std/Monoid.tt`・`std/Monoids.tz`・`fold_monoid`・`fold` が付録のとおり入り、`cargo test --locked`（`stdlib` の 3 件と `tests/monoid.rs` の 4 件を含む）が緑。
- [ ] `node tests/features.mjs target/release/tsuzuri monoid` が `features: 90 cases passed`、`tsuzuri test tests/fixtures/monoid` が `11 passed; 0 failed; 0 ignored`、既存の `parallel` が 36 件。
- [ ] E1039 が仕様 §4 の 8 ソースで出て（コード・メッセージ・位置）、整数の `Monoid`・使われない float の instance は受理される。
- [ ] `Monoid`・`Monoids` を使わないプログラムの `check` の出力と終了コードが不変（手順 13 の `diff`）。WASM の import・予約語・crate・`unsafe` が増えていない。
- [ ] 文書（手順 5 の `check-docs` の数）と `docs/language.md`・`README.md` が揃っている。
**Phase 2**
- [ ] suite `parallel_equivalence` が 48 件。`parallel.md` と `docs/language.md` に §5 の保証の表がある（手順 7 の数）。
**Phase 3**
- [ ] `tests/parallel_report.rs` の 8 件・単体テスト 6 件が緑。`--report` なしの出力が不変。同じ入力の 2 回の報告が同一（テスト `the_report_is_deterministic`）。
- [ ] 提案を適用したソースが `check` を通り、提案が 0 になる。`A19` の既存テストが緑のまま。
**Phase 4**
- [ ] G26 の `facts_rows_carry_parallel_entries` が緑（G26 がなければ対象外）。

## 落とし穴

1. **opt-in の std は修飾してしか書けない**（`Sum` は `E1004`、`Monoids.Sum`）。`std/Monoid.tt` は opt-in にしない（`Array.tz`・`Parallel.tz` が `Monoid` を使う。opt-in にすると `opt_in_modules_are_reached_only_through_their_names` が赤）。
2. **instance の `empty` は `let empty = \() -> ...`**（`fn empty () = ...` は `E0002`）。条件付き instance の文脈は `(Integer<'a>, Numeric<'a>, Add<'a>, Copy<'a>)` の形（足りないと `E1027`）。
3. **可換な `Sum` だけのテストは、`combine` の引数の入れ替えを見逃す**。非可換な `Affine` を必ず入れる（変異で確かめる）。
4. **`std` を変えたら `cargo build --release --locked` をやり直す**。やり直さないと `unknown type class 'Monoid'` のまま。
5. **`Gen.i64()`・`Gen.bool()` は引数のない関数の呼び出し**。`Gen.for_all_cases` の値は `Display` が要るので、`Maybe` の payload は性質の中で作る（付録 E のとおり）。
6. **E1039 は `normalize` に置く**。`validate_instance` に置くと、`Monoid<Sum<'a>>` が見つかる `Monoids.Sum<f64>` では呼ばれず、`E1005`（`Integer<f64>`）のままになる。
7. **Phase 3 の `Parallel(..)` の式は callee を持たない**ので、`site` の `span` は式の全体、`suggestion` の `span` は名前の範囲（`e.span.start` から `call.len()` バイト）。ソースのテキストと比べる条件を外さない（パイプの `xs |> Array.map f` は提案しない）。
8. **診断と同じ `path`**（`tsuzuri check Main.tz` なら `./Main.tz`）と、使用法のエラーの終了コード 2（`--warn` と同じ。`<command line>:1:1: error[E2000]: ...`）に合わせる。`tests/parallel_report.rs` は終了コードの値でなく、失敗とメッセージで確かめる。
9. **決定性**: エントリの集合は `BTreeMap`・整列済みの `Vec` で持つ。`HashMap` の反復順を出力に使わない（手順 13 の 2 回実行のテスト）。
10. **A19 の `validate` を変えない**。`reachable_effect` は内部関数を共有するだけで、A19 の診断の出力は 1 byte も変えない（`cargo test --locked --test pure_functions`）。

## 対象外

- 浮動小数点の `Monoid`（`Sum<f64>`）。結合律の静的な検査。黙った自動並列化と、提案の自動適用（`tsuzuri fix` は `maybe` を適用しない）。
- `Parallel.*` の実行時の規則（チャンク数・結合順・WASM の逐次経路・`Gpu.Auto`）の変更、新しい組み込み関数・ランタイム関数。
- `Task`・`Async` の並行性の解析。`List`・`Vec`・`Seq` の `fold_monoid`。`Map`・`Set` のモノイド。`--report` の `parallel` 以外の値。LSP のコードアクションへの提案の露出（G22・G24 の領分）。

## 決定事項

すべて**承認済み**（D-50: 人間の包括許可 2026-10-18）。

- **D1 `Monoid` は `std/Monoid.tt` の型クラス**。理由: 法則を持つ型を型で書け、E1039 のような検査を `ClassEnv` に置ける。却下: Rust の組み込みクラス（`polymorph.rs`・`Builtin` に特別扱いが増える）。却下: `empty` と `combine` を関数として渡す API（型で「法則がある」と言えず、`Parallel.fold` の同一の保証が文書だけになる）。
- **D2 newtype は opt-in の `Monoids` に置き、修飾して書く**。理由: 利用者の `Sum`・`Product`・`All`・`Any` の型名と衝突せず、使わないプログラムは何も読み込まない。却下: 無修飾の常時ロード名（利用者の型と衝突し、予約名が増える）。却下: `Monoid.Sum`（`.tt` のクラスと同じ名前空間に型を置けず、opt-in にもできない）。
- **D3 浮動小数点の `Monoid` は作らず、E1039 で拒否する**。理由: 浮動小数点の加算は結合的でなく、チャンクの分け方でビットが変わる（「現状」の実例）。却下: 許して文書で警告（保証が型で書けない）。却下: Kahan 付きの `Sum<f64>`（チャンクをまたぐと結合的でない）。却下: 警告にとどめる（保証が空洞化する）。
- **D4 E1039 は `ClassEnv::normalize` に置く**。理由: 具象な `Monoid<T>` の制約は必ず `normalize` を通る。`validate_instance` は instance が見つからないときだけ呼ばれる。却下: `validate_instance`（`Monoids.Sum<f64>` で呼ばれない）。却下: 型検査後に `Parallel.fold` の呼び出しを数える別の走査（直接の `Monoid.combine` を取りこぼす）。
- **D5 並列化は提案だけで、自動では適用しない**（`fixes` の `maybe`）。理由: 実行順序・コスト・メモリの意味が見えなくなる。却下: 黙った書き換え。却下: `certain` の fix（`tsuzuri fix` が性能の判断を機械的に決める）。
- **D6 純粋性は A19 の規則に従い、外から入る関数値だけを不純とする**。理由: A19 の「作った場所が到達する本体の中にある」という論拠が、捕捉・引数・結果の関数値だけ成り立たない。却下: A19 のまま（捕捉された関数値の本体を見ず、保証が破れる）。却下: 関数値の呼び出しを全部不純（`Array.map`・`Array.fold` の std の高階関数を使うだけで不純になり、報告が役に立たない）。
- **D7 `--report parallel` は新しいフラグ**（値は `parallel` だけ）。理由: 報告は stdout に出て終了コード 0 で、`--warn`（警告の有効化・stderr）と意味が違う。却下: `--warn parallel`。却下: 新しいサブコマンド（D-55 の割り当てを増やす）。却下: `check --json` の診断に混ぜる（診断の JSON を変えない約束）。
- **D8 `suggestion` の `span` は呼び出しの名前、`site` の `span` は呼び出し式の全体**。理由: `TypedExprKind::Parallel` は callee を持たず、置き換えは名前だけ。却下: 全部 callee の `span`。
- **D9 小さい入力（静的に 4,096 未満）と浮動小数点の `Array.sum` は提案しない**。理由: チャンクが 1 つで並列にならない／ビットが変わる。却下: 全部提案（人間が確かめる前に誤った提案が増える）。
- **D10 Phase 分割**: Phase 2 は Rust の変更なし（Phase 1 から独立）。Phase 3 は A19、Phase 4 は G26 に依存し、なければ出荷しない。却下: 1 つの Phase にまとめる（規模 L が 1 つの変更になり、A19・G26 の遅れで全体が止まる）。
- **D11 kind `monoid_float_instance` を G23 の登録簿に入れる**。G23 が未実装でも E1039 は動く。
- **D12 `--report` の報告は `tz-report-parallel/1` の JSON Lines（1 行 1 オブジェクト）**。却下: 1 つの JSON の配列（長い出力を行ごとに処理できない。D-52 の facts と同じ形に揃える）。
- **D13 条件 (d)（`empty`・`combine` に外から見える作用がない）は型検査では強制しない**。理由: 強制には instance の本体の効果を A19 の窓口で調べる必要があり、A19 が未実装の Phase 1 では書けない。代わりに §3 の条件に書き、Phase 3 の `parallel_safe` で見えるようにする。却下: 不純なら `Parallel.fold` を逐次に落とす（実行時の規則を変えない。非目標）。却下: 新しい診断（Phase 1 の範囲を超える）。

## 付録

- [F15-appendix.md](F15-appendix.md): A〜D（std と `src/stdlib.rs`）、E・F（fixture）、G（suite）、H（実測）、I（`monoid.md`）、J（`tests/monoid.rs`）、K（文書の変更の全文）。
- [F15-appendix-report.md](F15-appendix-report.md): Phase 3 の fixture 3 つ、`tests/parallel_report.rs`、`parses_report_parallel`、実装者が見直す点。

## 実装と検証（2026-10-31）

- **範囲**: Phase 1〜4 をすべて実装した（1 つの PR。G34・G35 のブランチの上）。冒頭の「方針の最新化」のとおり、起票後の変更（edition 2027・G23・G34・G35・A19 の窓口・D-60 の KG）に合わせた。
- **Phase 1**: `std/Monoid.tt`・`std/Monoids.tz`（付録 A・B のまま）、`Array.fold_monoid`（`std/Array.tz`。`@complexity O(n)`・`@since 0.1.0`・`@see`・例）、`Parallel.fold`（`std/Parallel.tz`。`Parallel.sum` は 1 文字も変えていない）、`src/stdlib.rs`（`SOURCES` 2 行、`RESERVED_MODULES` 54 → 56、`OPT_IN` の `Monoids`）。`Type::holds_float`（`src/check.rs`）と `ClassEnv::normalize` の `E1039`（`src/polymorph.rs`。`DiagnosticKind::MonoidFloatInstance`、help 付き）、`src/explain/codes/E1039.md`。テスト: fixture `tests/fixtures/monoid`、suite `monoid`（`tests/features.mjs`）、`tests/monoid.rs`（4 件）、`std_examples.rs` の `FLOORS` と `std_argument_order.rs` の `MODULES` に `Monoid`・`Monoids`。
- **Phase 2**: fixture `tests/fixtures/parallel_equivalence`（2027 の表記に直した）と suite `parallel_equivalence`。保証の表を `parallel.md`（節「逐次と同じ結果になる条件」と `true`・`false` の 2 例）と `docs/language.md`（「逐次との同一と `Monoid`」）に載せた。Rust の変更なし。
- **Phase 3**: `src/parallel_report.rs`（`build`・`entries`・`render_text`・`render_json`。提案・地点・純粋性・型と長さとテキストの条件・特殊化の複製の併合・1,000 件の上限・関数ごとの 100,000 式の上限）、`pure_rules::reachable_effect_of_chunk_body` と A19 の窓口の単体テスト 4 件、`impl Sources for Project`（`src/driver.rs`）、`src/main.rs` の `--report parallel`（解析・`--report is only valid with check`・`HELP`・stdout への出力・`parses_report_parallel`・`rejects_ambiguous_or_unused_arguments` に 3 件）。fixture `tests/fixtures/parallel_report/{basic,rules,error}` と `tests/parallel_report.rs`（8 件）。§7.5 の想定出力は実出力と一致した（`basic` の text と JSON の全文、`rules` の 7 件）。
- **Phase 4**: KG の facts の行に `parallel`（`src/kg/model.rs` の `ParallelRow`・`FACT_KEYS` 14 個、`src/kg/facts.rs` が `parallel_report::entries` を関数の宣言ごとに分け、100 件で `truncated.parallel`）。`tests/kg.rs` の `facts_rows_carry_parallel_entries`（`pure_total` に提案 2 件、`doubled` に `parallel_safe: true`、`shown` に `false` と理由、`square`・`traced` は `[]`）。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。`cargo nextest run --locked` は 2,806 件のうち 2,804 件成功・4 件 skip で、`kg_brief` の 2 件（`cards_match_the_golden_files`・`kg_build_verify_clean_export_round_trip`）だけが、facts の行に `"parallel":[]` が入った分で `tests/golden/kg/shop` と違った（§8 の「G26 の golden を更新する」）。golden の `facts/*.jsonl` と `manifest.json` を作り直し（差は `"parallel":[]` とハッシュだけ）、KG と報告のテスト（`kg`・`kg_brief`・`kg_build`・`kg_query`・`kg_impact`・`kg_std`・`parallel_report` と単体）115 件が成功。新しいテスト: `tests/monoid.rs` 4 件、`tests/parallel_report.rs` 8 件、`src/parallel_report.rs` 4 件、`src/pure_rules.rs` 4 件、`parses_report_parallel`、`tests/kg.rs` の `facts_rows_carry_parallel_entries`、`stdlib` の 9 件（`reserves_the_d07_table` は 56）。E2E（dev のバイナリ、native と WASM の `-O0`／`-O3`）: `monoid` 90 件、`parallel_equivalence` 48 件、`parallel` 36 件（不変）、`json` 165 件、`map_set` 26 件、`iteration_protocol` 26 件。`tsuzuri test tests/fixtures/monoid` は `-O0`・`-O3` とも `11 passed; 0 failed; 0 ignored`。変異（`Parallel.fold` の `combine right left`）で `affine_total` が要素数 4095 で落ち、`Sum` などの可換な場合は通ることを確かめて戻した。`tsuzuri test --doc std` は `-O0`・`-O3` とも `825 passed; 0 failed; 0 ignored`。`--report` なしの `check` の出力と終了コードは、全 fixture で HEAD のコンパイラと同じ（違ったのは新しい `monoid` だけ。HEAD には `Monoid` がない）。`node scripts/check-docs.mjs`（変更した 10 ページ）: `424 links, 55 checked examples, 102 native runs (O0/O3), 2 test projects, 2 generated blocks`。`node scripts/gen-docs.mjs --check` 0 stale、`node scripts/check-cli-docs.mjs` `4 checks, 0 failed`、`node tests/repo_kb.mjs` 78 件、`node scripts/check-guide-lite.mjs` 0 failed。
- **文書**: 新規 `built-in-types-and-modules/monoid.md`（付録 I に `Monoids` の表、検査される法則のテスト例、「作用を持つ instance」「引数の順序」を足した）。`parallel.md`・`array.md`・`index.md`・`union.md`・`organizing-tsuzuri/modules.md`（予約名 56 個）・`compiler/diagnostics.md`（`E1039`、生成ブロック）・`compiler/usage.md`（`check --report parallel` と実出力）・`compiler/option.md`（表 3 か所）・`compiler/kg-format.md`（`parallel`）、`docs/language.md`（データ並列 API の表・「逐次との同一と `Monoid`」・`--report parallel`・予約名・診断の表）、`docs/architecture.md`（`OPT_IN`・`RESERVED_MODULES` 56・節「Monoid（F15）」・`src/parallel_report.rs` と `src/kg/` の行）、`README.md`（データ並列 API の項・CLI の synopsis・オプション表）、`_features/GUIDE.md`（D-53 の窓口に `reachable_effect_of_chunk_body`）。`docs/stability.md` は変えない（「std へのモジュールの追加」の既知の例外）。
- **新しく予約した std モジュール名**: `Monoid`・`Monoids`（同名の利用者のモジュールは `E1011`。release notes に載せる）。
- **見つけたこと**: (1) `organizing-tsuzuri/modules.md` と `docs/language.md` の予約名の一覧に `Alloc` が抜けていた（本 PR で足した）。(2) edition 2027 の `Array.map f xs` は `Array.mapr` と読み取りコールバックのアダプタになるので、`Array.map` を名前で探す道具（KG の `calls` の `to` など）は `Array.mapr` を見る。報告は書かれた名前で判定する。(3) A19 の `witness` は `ref mut` の引数を持つ根を一律に `Unresolved` にするので、`for_each_chunk` の本体のような関数を根にする問い合わせは本 PR の `reachable_effect_of_chunk_body` が要る。
