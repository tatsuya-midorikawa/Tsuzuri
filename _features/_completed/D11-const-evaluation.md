# D11: コンパイル時評価の拡張（const 関数・表の生成）

| 項目 | 内容 |
| --- | --- |
| ID | D11 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | D06, (A16) |
| 後続 | – |
| 状態 | done（Phase 1・2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。旧計画の基準は `f8dc655` |
| 実装の基準 | `053bced8`、2026-10-11 |
| 承認 | 2026-10-11 に、全フェーズと `要承認` の判断すべてへ包括承認（要承認だったのは旧 D10 の Phase 2 の静的データだけで、GUIDE D-28 の「static lifetime を導入しない」の変更を含む） |
| 利用者向け仕様 | [値（コンパイル時定数・`const def`・静的データ）](../../_tsuzuri/language-reference/values-and-functions/values.md#コンパイル時定数-const-と-literal)、[ライフタイムと region](../../_tsuzuri/language-reference/ownership-and-memory/lifetimes.md)、[診断コード](../../_tsuzuri/language-reference/compiler/diagnostics.md)、[言語仕様](../../docs/language.md#コンパイル時定数) |

## 目的と実装範囲

lookup table・CRC 表・係数表などを、実行時の初期化なしに関数で生成できるようにした（`const def`）。
評価は実行時と同じ数値・トラップの意味で、Rust の再帰を使わない明示スタックの評価器（`Machine`）が行う。
評価した値は D06 と同じく型付きリテラルになり、その値が平たいデータだけの定数は、読み出しと借用でバイナリの読み取り専用の 1 つの実体（静的データ）を指す。

| フェーズ | 実装 |
| --- | --- |
| Phase 1 | `const def`（構文、署名の値型、本体の網羅的な受理・拒否、明示スタックの評価器、初期化式からの呼び出し、依存の走査）。予算・呼び出しの深さ・値の入れ子の上限。テスト、E2E suite `const_functions`、文書 |
| Phase 2 | 平たいデータの定数の静的データ（`TypedExprKind::Static`、`@tz.static.N`）、所有権の静的な根、借用の escape、合計 4 MiB の上限。テスト、E2E suite `const_statics`、ベンチマーク `benchmarks/run-statics.mjs`、文書 |

新しい予約語・診断コード・環境変数・crate・`unsafe`・WASM の import はない。`FRONTEND_FORMAT` は 2 から 3 へ上げた（`const` の印を符号化する）。

## 確定した仕様

### Phase 1: `const def`

- 構文: `const` は `def` の署名だけに付け、`fn`・`and`・`let` は継承する。`private const def`、`export const def`、`const def rec` と `and`（署名の `const` を継承）を書ける。`const fn`・`const let`・`const and`・`const export def` は `E0002`。`@cpu` と `@literal` を `const def` に付けると `E0002`。
- 署名は具体的で、引数と結果が値型だけ（整数、binary 浮動小数点、decimal、`bool`、`unit`、`char`、`utf8char`、`string`、`utf8string`、`[T]`、固定長配列 `[T; N]`、タプル、レコード、union）。固定長配列は平たい値なので許す。型変数・制約・関数型・参照・Task・リスト・`Vec`・`dyn`・`Rc`・SIMD・ハンドル・利用者の `Drop` は `E1026`。
- 評価は型検査の後、所有権検査と特殊化の前（`constants::fold`）。`const def` の本体は、定数から呼ばれなくても検査する。実行時にも通常の関数として呼べて、IR は `const` のない関数と同じ。
- 予算は既存の `remaining`（1 << 20）を定数の評価と共有し、呼び出しの深さと値の入れ子は `MAX_NESTING`（128）。どの上限も上げず、超過は `E1026`。stack-depth の 3 テスト（`bounds_type_growing_polymorphic_recursion`、`bounds_recursive_and_flat_expression_depth`、`bounds_nested_builder_expansion_not_just_source_syntax`）と `RUST_MIN_STACK` の下限（2,130,000 以下）は変わらない。
- trap（ゼロ除算、`MIN / -1`、負の指数、範囲外の添字、step 0、負の長さ、どの arm にも一致しない `match`）は評価中に `E1026` でメッセージに種類が入る。

#### `Type` と `TypedExprKind` の振り分け

`const_rules.rs`（署名の `value_type`、本体の `validate_function`）と `const_machine.rs` の `match` は、`_ =>` を使わずに全 variant を名指しする（新しい variant は両方で判断が要る）。

| 対象 | 評価する | 拒否する（`E1026`） |
| --- | --- | --- |
| `Type` | `Integer`・`Binary`・`Decimal`・`Bool`・`Unit`・`Char`・`Utf8Char`・`String`・`Utf8String`・`Array`・`Tuple`・`FixedArray`（長さが具体的）・`Record`（利用者の `Drop` と不透明な std のレコードを除く）・`Union`（利用者の `Drop` を除く） | `Simd`・`List`・`Vec`・`Task`・`Handle`・`Function`・`Reference`・`Dyn`・`Shared`・`ArrayView`・`Length`・`Error`・`Variable`・`Infer`・`Partial`・`Application` |
| 値・変数 | `Int`・`Float`・`GenericInteger`・`GenericFloat`・`String`・`Bool`・`Unit`・`Local` | `Error` |
| 演算 | `Unary`・`Binary`（スカラーと文字列。短絡の `&&`・`\|\|` を含む）・`Cast`（数値どうし） | `Binary(Pipe)`、集成体への演算子（`StructuralCompare`・`StructuralHash`・`StructuralDisplay`）、decimal の演算、`Checked`（`@checked`） |
| 制御 | `Block`・`If`・`Match`・`While`・`ForRange`・`ForEach`（配列・`string`・`utf8string`）・`Break`・`Continue`・`Assign`（`let mut` の変数だけ） | `Try`・`RaisedException`・`Reraise` |
| データ | `Record`・`RecordUpdate`・`Array`・`Tuple`（固定長配列を含む）・`NewLiteral`・`NewArray`（ラムダか、引数 1 個の `const def`）・`Construct`・`Field`・`Index`・`Length`・`StringLength`・`UnionTag`・`UnionPayload`・場所の読み出しの基部の `Borrow(false)`・`BorrowOperand`・`Dereference` | `List`・`ListTail`・`NewList`・`Slice`・`Borrow(true)`・`Interpolated`・`CaseConstructor` |
| 呼び出し | `Call`（定数と `const def` の完全適用） | `Function`（`new [T](n, f)` の初期化子以外）・`GenericFunction`・`Method`・`TypeFunction`・`HostCall`・`Lambda`・`Closure`（同上）・`TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel`・`DynDispatch`・`Static` |

### Phase 2: 静的データ（設計の答え）

**1. 表面（どの使い方が静的データを指すか）。** 平たいデータの定数 `C` について、次は複製せず `Static(C)` を指す: `ref C`・`&C`・`ref` 引数へ渡す `C`（`Array.length C`、パイプの引数、文字列補間の穴）、`C[i]`、`C.length`、`C.field`、`ref C[a..b]`、固定長配列の `C` を `ref [T]` へ渡す `Array.sum Grid`、配列の `for x in C`。値としての使い方（`let t = C`、値の引数・戻り値、`match C`、`C == D`）は従来どおり独立した所有値の複製（IR は変わらない）。平たいデータは、整数・浮動小数点（`f16`〜`f128` と decimal を含む）・`bool`・`char`・`utf8char`・`unit`・それらだけのタプルとレコード（利用者の `Drop` なし）・固定長配列・payload を持たないか全 case が同じ payload 型の union、そして定数全体が配列 `[T]` のときのその要素（配列の配列は対象外）。型だけで決め、大きさでは決めない。`string`・`utf8string`・リスト・`Vec`・`needs_drop` な型を含む定数は対象外で、従来どおり（借用の escape は `E1013`）。診断コードは増やさない（`E1013`・`E1026` を再利用）。

**2. escape と所有権。** 静的データへの借用は、返す・`ref` フィールドを持つレコードへ格納する・ほかの関数へ渡す・クロージャーの本体で名前から読む、のどれも `E1013` にならない。タスクの本体が名前で読むのは捕捉を要さず可能で、借用を持つ変数の捕捉と借用を持つタスクの結果は、型の規則（`tasks require owned values`）のまま `E1013`。所有権検査は `Static(id)` を根 `STATIC_ROOT + 1 + id` の場所とし（ローカルの番号とも引数の根 `usize::MAX - id` とも重ならない）、書き込みも move もできず、その根への借用は読み取りだけで互いに衝突しない。`ref mut C` は場所でないので `E1013`、要素への代入は `E1012`、排他スライスは `E1014`。`check_result` と `store_through` は静的な根の借用を許す（タスクの結果を除く）。借用を返す関数の呼び出しで、どの引数も借用を貸さずに結果の型が参照を持つとき、結果に根 `STATIC_ROOT` の借用を 1 つ足す（返せる借用は引数の借用か静的データだけで、ローカルを返すクロージャーや関数は呼び出し側の前に `E1013` で止まるため、ダングリングを隠さない。`static_borrows_do_not_hide_a_dangling_reference` が検査）。`Copy`・`Send`・`Sync` は型の性質のままで、変わらない。名前付きの static lifetime や region の記法は足していない。

**3. 表現と LLVM。** 平たい値は `@tz.static.N = private unnamed_addr constant T v`、配列は要素を `@tz.static.N.data = private unnamed_addr constant [L x T] [...], align 16`（8 ビット整数は `c"..."`）に置く。借用の値は定数 `%tz.array { ptr @tz.static.N.data, i64 L }`（空の配列は `zeroinitializer`）で、ヘッダーのグローバルも読み込みもない。`N` は定数の宣言の番号で、定数ごとに 1 つ、使われたときだけ `Globals::statics` が定義する（順序は関数の出力順で決定的）。借用は drop も `free` も `realloc` もされない: 参照は drop しない、借用を所有値にする経路（`Array.copy`・`clone_value`・`Array.reverse`・`Array.append`・`Array.sort` など）はすべて確保する、`take` された `Static` の配列は `clone_value` の複製になる。`unnamed_addr` の統合（LLVM の `constmerge`）は番地を比べないので無害。WASM ではデータ領域に入り（import は増えず、threads でもワーカーが読む）、ヒープの上限は絶対番地 16 MiB のままなので、その分ヒープが狭まる（`--wasm-max-memory` が小さすぎれば wasm-ld がリンクで失敗する）。静的データは確保を要さないので `--freestanding`・カスタム allocator と無関係。trap の挙動（範囲外は同じ `index out of bounds`）、`--trap-info`、デバッグ情報、カバレッジは変わらない。

**4. 上限。** 静的データは合計 `MAX_STATIC_BYTES` = 4 MiB（4,194,304 バイト。LLVM の配置の大きさで数える）。超えると、超えた定数の宣言（id の昇順）に `E1026`。複製へ黙って戻ることはない。評価の予算（1,048,576）が先に尽きるので、ラムダで作る配列は 16 バイトの要素で約 20 万要素（約 3.2〜3.5 MiB。測定では 20 万個は通り、22 万個は予算で `E1026`）までで、4 MiB は複数の定数の合計か、より大きい要素でないと届かない（単体テスト `the_total_of_static_data_is_bounded` が境界を検査）。ノード数が多い定数の評価は従来どおり `remaining` に数える。コンパクトな表現（packed vector）は不要だった。

**5. 互換性と性能。** 静的データを使わないプログラムの IR は、ベースラインのコンパイラと同一（native と wasm32、`-O0` と `-O3`、fixture と examples の 358 ファイルで比べ、変わったのは `constants` fixture の 4 ファイルだけで、`Array.length (&Values)` が静的データの借用になったため）。測定は `docs/benchmarks.md` の「静的データ（D11 Phase 2）」: 256 要素の表を引く CRC-32 のループで、`CrcTable[...]` を直接読む形が、以前のコンパイラ（反復ごとに 1 KiB の表を作り直す）の 5.03〜5.06 倍の速さで、C の `static const` の表と同じ速さ（M1 Max、`-O3`）。ループの前に束縛する形は以前も同じ速さだった。

### GUIDE の更新が要る箇所（実装者から統合者への連絡）

- **D-28:** 「aggregate は既存 literal/frame/relocate を再利用し、専用 global template や Const 型付き命令を増やさない」は、平たいデータの定数だけ例外（`TypedExprKind::Static`・`@tz.static.N`）。「定数の直接借用は単一段階の完全適用・loan を持たない結果だけ。保存・返却・段階適用では明示 let 束縛を要求し、static lifetime を導入しない」は、平たいデータの定数の借用は静的データを指して escape できる（名前付きの static lifetime の記法は導入しない。`string` など平たくない定数は従来どおり）。上限の行へ「静的データ合計 4 MiB（`E1026`）、`const def` の呼び出し 128 段」を足す。
- **D-30:** 「D11 Phase 2（static データ。D-28 の変更）」を承認済み・実装済みへ。構文の行 `const def` は予約語を足さない（`const` の直後が `def`）ので D-15 へ移し、D-16 は診断コードの追加なし（`E1026`・`E0002`・`E1013`）。
- D-15・D-16 はコードの追加なし。`FRONTEND_FORMAT` は 3。

## 旧計画から外れた判断

| 旧計画 | 確定した判断と理由 |
| --- | --- |
| 評価器・規則を `src/constants.rs` に置く | `constants.rs` の子モジュールへ分けた: `src/const_machine.rs`（評価器）、`src/const_rules.rs`（署名と本体の規則）、`src/const_static.rs`（静的データ）。`constants.rs` を肥大させず、`match` の網羅を 1 か所に集めるため |
| 評価は特殊化後の整数リテラル | 評価は特殊化の前で、`GenericInteger`・`GenericFloat` が残る。`Machine` が `Checker::integer_literal` と `numeric::float_literal` で型に合わせて解決する |
| 文字列は値として運ぶだけ | `string`・`utf8string` の `for`・添字・`.length` を評価する（UTF-16 のコード単位、`utf8string` はバイト） |
| `const` の初期化式は定数と演算だけ | 呼び先が `const def` の名前か union の case 名なら呼べる（`area (Square 7)`）。定数のフィールド参照 `P.x` も書ける（従来は `E1026`）。添字は従来どおり `E1026` |
| 単項 `+`・`@checked`・パイプ・集成体の `==` の扱いは未定 | 単項 `+` は評価する。`@checked`・`\|>`・集成体への演算子は `E1026`（例外と呼び出しの形が定数式にないため） |
| 固定長配列の署名は未定 | 許す（平たい値）。本体の `xs[i]`・`xs.length` も評価する（レビューで抜けを直した） |
| `@cpu`・`@literal` との併用は未定 | どちらも `const def` に付けられず `E0002`（`@cpu` は専用のメッセージ） |
| Phase 2 の静的データは「static 領域と借用元」 | 型の `Static` の場所ではなく、`Borrow` の被演算子にしか現れない葉 `Static(id)` と、読み出しの `*(ref Static)` へ書き換える。ヘッダーのグローバルは作らず、借用の値は定数。ownership は静的な根（`STATIC_ROOT`）と、借用を貸さない呼び出しの結果への loan で表す。`Match.value` は書き換えない（値の複製のまま） |
| `_docs/feature-status.md` の更新 | そのファイルはない。状態は `_features/README.md`（統合者が更新する）と `_tsuzuri/language-reference/languages/strategy.md`（更新した） |
| 作業は `/tmp` の scratch | 実行環境が `/tmp` の書き込みを拒否するので、scratch は `target/scratch/`（git 管理外）に置いた。rubber-duck の agent 型がなかったので、Phase 2 の設計レビューは general-purpose の agent に依頼した |

## 受け入れ条件

- [x] `const def` の構文、署名・本体の検査、明示スタックの評価、初期化式からの呼び出し、依存の走査。上限は上げていない。
- [x] 評価した値が実行時の結果と一致する（`const_def_calls_evaluate_like_runtime`、E2E の `assert (f x == Value)`）。数値の意味（`unary`・`binary`・`cast`）は変えていない。
- [x] `const def` を使わない既存プログラムの IR は同一。`tests/constants.rs` の既存 8 件と suite `constants` の値は変わっていない。
- [x] stack-depth の 3 テストと `bounds_computation_syntax_and_expansion`（`RUST_MIN_STACK=2200000`）が通る。
- [x] Phase 2: 静的データの表面・所有権・LLVM・上限・互換性・性能を上の答えのとおり実装し、測定した。
- [x] 文書: `docs/language.md`・`docs/architecture.md`・`docs/benchmarks.md`・`README.md`・言語リファレンス（`values.md`・`lifetimes.md`・`keywords.md`・`functions.md`・`attributes.md`・`access-control.md`・`diagnostics.md`・`directives.md`・`strategy.md`）。`node scripts/check-docs.mjs` は 9 ページ、44 例、88 回の native 実行（`-O0`/`-O3`）で通る。

## 実装と検証（2026-10-11）

### 実装したファイル

- Phase 1: `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`（golden）・`src/frontend_cache.rs`・`src/docgen.rs`・`src/semantic.rs`・`src/check.rs`・`src/constants.rs`・`src/polymorph.rs`、新規 `src/const_machine.rs`・`src/const_rules.rs`。
- Phase 2: 新規 `src/const_static.rs`、`src/check.rs`（`Static`・`StaticData`・`CheckedModule::statics`、`fixed_slice_source`）・`src/constants.rs`（`fold` の書き換え、`temporary_borrows`）・`src/ownership.rs`・`src/llvm.rs`・`src/polymorph.rs`・`src/warnings.rs`。
- テスト: `tests/constants.rs`（8 件 → Phase 1 で 18 件 → 32 件）、`src/const_static.rs` の単体テスト 2 件、`tests/docs.rs`・`tests/lsp.rs`、`tests/features.mjs` の suite `const_functions`（19 件）と `const_statics`（52 件と WASM threads の検査）、fixture `tests/fixtures/const_functions/`・`tests/fixtures/const_statics/`。
- 測定: `benchmarks/run-statics.mjs`、`benchmarks/statics/`。

### 検証結果

| 検査 | 結果 |
| --- | --- |
| `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings` | 問題なし |
| `cargo test --locked --no-fail-fast -- --skip bounds_computation_syntax_and_expansion` | 93 個のテストバイナリで 1,106 件が成功、失敗 0（Phase 1 の後は 1,090 件、Phase 2 の後は 1,103 件、レビューの修正の後は 1,106 件） |
| `RUST_MIN_STACK=2200000 cargo test --locked --test computations bounds_computation_syntax_and_expansion` | 成功。`RUST_MIN_STACK=2130000` で成功、`2129687` で stack overflow（ベースラインの下限 2,129,687〜2,130,000 と同じ） |
| `node tests/features.mjs target/release/tsuzuri constants`・`const_functions`・`const_statics` | native と WASM の `-O0`/`-O3` で成功（8・19・52 件、`live == 0`、WASM の import なし、trap の種類、IR の決定性） |
| `const_statics` の変種 | `TSUZURI_ASAN=1`、`TSUZURI_TEST_ALLOCATOR=host`、`TSUZURI_TEST_WASM_SIMD=1`、`TSUZURI_TEST_CPU=native`、WASM threads（3 ワーカー）、`TSUZURI_CLANG` が clang 21.1.8・22.1.8・23.1.1 のそれぞれ（wasm32 `-O0` を含む）で成功 |
| IR の比較（fixture と examples、native と wasm32、`-O0` と `-O3`） | ベースラインの 358 ファイルのうち 354 が同一。変わった 4 ファイルは `constants` fixture（静的データの借用）。新しい fixture の IR は 8 ファイル増えた |
| `-g`、`--emit object`・`shared`・`header`・`bindings-js`、`--trap-info`、`tsuzuri test`・`--coverage` | 静的データを使うプログラムで成功（`-g` の task 付きの object は、clang と llvm-link の版を揃えたときだけ。版が違うと LLVM 23 の `f0x` 形式の浮動小数点を Apple clang 21 が読めない。静的データとは無関係） |
| `cargo check --locked --all-targets --target x86_64-pc-windows-msvc`・`aarch64-pc-windows-msvc` | 成功（rustup の stable ツールチェーン） |
| 測定（M1 Max） | `docs/benchmarks.md` |

実行できなかったもの: x86-64 と Linux と Windows での実行、wasm64 の実行（Node 24 がなく、Node 20 は table64 を読めない。wasm64 のビルドとリンクは成功）。

### レビュー後の修正

独立した code-review の agent が、ブランチ全体を検査した。所有権・LLVM・書き換えの領域に問題はなく、次の 4 点と弱いテストを直した（各修正にテストを足した）。

| 指摘 | 修正 |
| --- | --- |
| `new [T](n, f)` に `const def` を名前で渡すと、その本体が読む後ろの定数を走査せず、偽の循環 `E1026` | 依存の走査が、呼び出しだけでなく `Function(User(id))` の名前をたどる（`const_defs_passed_by_name_bring_their_constants_before_use`） |
| 固定長配列（`Tuple` の節点）を `const def` の本体で添字・`.length` できない | 評価器の 4 か所で `Tuple` を受ける（`const_defs_index_and_measure_fixed_length_arrays`） |
| `private const def` が定数からしか呼ばれないと、偽の警告 `W1002` | `fold` の前に定数から `const def` への辺を集め、警告の参照へ足す（`a_private_const_def_that_a_constant_calls_is_used`） |
| `@tz.static.` の `free` を調べる検査が空振り（`free` の被演算子は SSA 値） | 配列のデータは `{ ptr data, i64 }` のヘッダーでしか名指されないこと、store・atomicrmw・cmpxchg の番地が静的でないことを検査（`assert_static_data_is_only_read`、suite の `inspect`）。実行時は `tracked_free` と ASan が検査する |
| （4 チケットを統合したあとの review）`f16`・`f128` の定数の `+ - * /` が NaN を返すと、APFloat の符号（正、またはオペランドの符号）になり、実行時（`tz_soft_op`）と `Math.copysign` の結果が違った | `float_binary` が、`f16`・`f128` の NaN の結果を、実行時と同じ規則の NaN（`+`・`-` は左辺の符号、`*`・`/` は両辺の符号の排他的論理和、ペイロードなし）に置き換える。定数と実行時の符号が一致することを suite `constants` の `nan_signs` が確かめる（旧コンパイラでは失敗する）。`f32`・`f64` の NaN の符号は実行時に機械が決めるので、定数は NaN であることだけを決める（文書に書いた） |
| （同上）依存の走査が、定数ごとに到達する `const def` の本体を歩き直し、1 つの大きな `const def` を多数の定数が呼ぶソースで 2 乗に遅くなった（12,800 定数で 2.7 秒） | `const def` の本体が名指すものを関数ごとに 1 度だけ求め（`ConstDefWalk`）、定数ごとの走査は名前の列を、見つけた順のまま展開する（12,800 定数で 0.22 秒。診断の順序と結果は変わらない）。値として使う展開の予算が関数ごとであることは従来どおり（既知の限界の 1 つめ） |

### 文書と計測

- 文書: `docs/language.md`（「コンパイル時定数」に `const def` と「読み取り専用の静的データ」、診断表）、`docs/architecture.md`（モジュール表、定数の節、検証コマンド）、`docs/benchmarks.md`（「静的データ（D11 Phase 2）」）、`README.md`、言語リファレンス（`values-and-functions/values.md`・`functions.md`・`keywords.md`・`attributes.md`、`ownership-and-memory/lifetimes.md`、`organizing-tsuzuri/access-control.md`、`compiler/diagnostics.md`・`directives.md`、`languages/strategy.md`）。
- 計測: `node benchmarks/run-statics.mjs target/release/tsuzuri --baseline <ベースラインのコンパイラ> --runs 15`（M1 Max、macOS、Apple clang 21.0.0、4 Mi バイト）。3 回の thread CPU 時間の中央値（ns/バイト）は、現在の `in_place` が 2.626〜2.630、以前の `before_in_place` が 13.220〜13.277、C の `static const` の表が 2.633〜2.641。生成コードの確認（`in_place` のループは 7 命令で境界検査なし、以前は反復ごとに 64 個のベクトル store）と条件は `docs/benchmarks.md`。測定中のロードアベレージは 27〜34。速度の合否の閾値は置いていない。

## 既知の限界

- 値として使う箇所への展開の予算（1,048,576 ノード）は、従来どおり関数ごとに数える。大きな値を多くの関数が値として使うと、メモリは関数の数に比例して増える（150,000 要素の定数を 8 関数が値で使うと、`tsuzuri check` が約 1.3 GB）。静的データの読み出しと借用は複製しないので、この上限に数えない。プログラム全体の上限にすると、既存のプログラムを拒否しうるので変えていない。
- `match C`・`let t = C`・値の引数は複製のまま。`Match.value` は書き換えていない（小さい値の複製で、表の読み出しの形ではないため）。
- 静的データは定数ごとに 1 つで、値が同じ別の定数とは統合しない（`-O1` 以上では LLVM の `constmerge` が統合する）。
- 固定長配列の定数への明示の `ref Grid` を `Array.sum` のような総称の `ref [T]` 引数へ渡すと `E1003`（ローカルの固定長配列と同じ既存の制限）。`Array.sum Grid` と `ref Grid[a..b]` は書ける。
- 静的データを指す使い方は、型検査後の `fold` が名前から決める。`match` と値の使い方のほかに、評価前の借用（`const` の初期化式の中）は書けない（`E1026`）。
- 評価の予算が先に尽きるため、ラムダで作る配列の静的データは実際には約 3.2〜3.5 MiB（16 バイトの要素）までで、4 MiB の上限は複数の定数か大きい要素でしか届かない。
- WASM の静的データは、その分ヒープの範囲（絶対番地 16 MiB まで）を狭める。
- x86-64・Linux・Windows での実行、wasm64 の実行、ほかの表の大きさと型の測定は未実施。
