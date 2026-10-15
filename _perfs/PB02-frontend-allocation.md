# PB02: フロントエンドの確保削減と高速化

| 項目 | 内容 |
| --- | --- |
| ID | PB02 |
| 分類 | ビルド速度 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | PX03 |
| 関連 | PB04, PB06, G17, G07 |
| 状態 | doing（全 Phase 実装・最終等価性検証済み、source freeze。D9 は型 graph arena と AST 移動へ設計変更。正式性能測定は統合担当） |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 利用者が全 Phase と設計判断を承認済み。下記の現行コードとの照合を優先する |
| 手本にする既存実装 | 所有権を移して複製を避ける: `polymorph::specialize` の `std::mem::take(&mut function.constraints)`。生成済みの本体の差し替え: `closures::lower` の `module.functions[id].body = body`。借用で子を辿る: `TypedExpr::children`（`&Self` 版）。計測: PX03 の `benchmarks/run-build.mjs`・`benchmarks/build/generate.mjs`、PX01 の `TSUZURI_TIME_PASSES` と `node benchmarks/metrics.mjs report --baseline` |
| 主な影響ファイル | `src/polymorph.rs`（`specialize`・`Specializer`・`Specializer::instantiate`・`walk`）, `src/closures.rs`（`lower` と `children_mut` の呼び出し）, `src/constants.rs`（`children_mut` の呼び出し）, `src/check.rs`（`TypedExpr::children_mut` の削除、`TypedExpr::try_for_each_child_mut`（新規））, `src/ownership.rs`（`State`・`Checker`・`check_body`）, `tests/polymorphism.rs`（テスト 1 件追加）, `docs/benchmarks.md`, `docs/architecture.md`, `_perfs/README.md`（状態欄） |
| 計測対象 | PX03 の合成 1,000 モジュール（`check` と `build --emit llvm -O0`）の wall time・peak RSS・段階別時間。`sample` による top-of-stack の malloc／free／memmove の割合と、関数別の inclusive samples |

## 目的

コンパイラのフロントエンド（構文解析から型付き IR の完成まで）の確保と複製を減らし、単一スレッドでの `check` を速く、最大 RSS を小さくする。
並列化（PB04）・常駐サーバー（PB06）・増分（G17）の前に、1 スレッドあたりの仕事を減らす。PB04 が並列にする各段も、この変更で軽くなる。

今回の依頼は Phase 1（S1〜S5）と Phase 2（P2-1〜P2-5）の全体です。

## 着手時の現行コードとの照合（2026-10-14）

- PX01・PX03 は実装済みです。合成入力は現行生成器の 139,010 行を基準とし、
  旧 124,003 行との速度比較はしません。作業ファイルは `target/perf/` に置きます。
- 特殊化上限は GUIDE D-41 の 65,536 です。既存上限・制約伝播順・関数番号を保ちます。
  template は現在、名前・origin・署名・型引数も参照されますが、移動後の本体を読む経路はありません。
  `requires_rec` の境界は移動前の関数数を保存します。
- 子の種類は Try・Interpolated・ArrayView・local rec などが増えています。
  現在の `children` / `children_mut` の全順序を基準にします。backend の並行作業が
  `children()` に依存するため API を残し、確保しない訪問 API を追加します。
- S4 は `ownership_control.rs` の状態・loop / try flow も対象です。
  引数・束縛の宣言を本体ごとの registry に借用し、`State<'a>` が `&'a Local` を保持します。
  evaluator の一時 capture / view 式には宣言の lifetime を要求せず、`Checker<'a, 'run>` が
  宣言と出力先の lifetime を分けます。loan / moved / merge の意味は変えません。
- P2-1: 抽象検査は Copy 制約を推論する検査であり、具体化後の loan・capture・region 契約の
  証明とは等価ではありません。全関数の再検査を省く旧案は健全性の根拠がなく採用しません。
  代替は `ownership_free` による構造上の証明です。すべての子が primitive scalar で、
  literal / Local / Unary / Cast / Binary / If だけの式は移動・loan・capture を作れません。
  release の具体検査だけを省き、debug は従来の検査の診断・制約が空と assert します。
  call / borrow / 束縛 / 閉包 / aggregate は省略しません。抽象検査は全関数で維持します。
- P2-2: `Names` に解析単位の `Interner` と検索専用の `(Symbol, Symbol) -> function id`
  辞書を実装しました。own-module の名前解決・entry・結果推論は qualified String を作りません。
  文字列は `Rc<str>` を名前列と検索辞書で共有します。既存の順序付き辞書・可視性・
  cross-module 解決は維持し、Symbol に Ord は付けません。
- P2-3: 読み取り専用の訪問も確保しない API を追加します。既存 `children()` は互換用です。
- P2-4: local id は外部 loan の `usize::MAX - id` と共存します。全 BTreeSet を密な bitset に
  変えると巨大確保になります。密な lambda free-local id だけを昇順 bitset にしました。
  ownership の使用集合は中間の counting map を作らず直接収集します。
- P2-5: `Type` は既に named arguments / Partial / Dyn を Box にして四 word を保ちます。
  arena / TypeId 全移行は checker・所有権・LLVM の全 API を変更し、並行 backend と衝突します。
  immutable 型述語の graph に private TypeId / hash-consed Vec arena を実装し、
  AST の全移行は本体移動・allocation-free 訪問に置き換える設計変更として D9 を確定しました。
  public Type / AST の全面移行を実装済みとは扱わず、測定と適用範囲を明記します。
- baseline の `bounds_computation_syntax_and_expansion` は親の初回実行が既存 CI 設定を
  省いたため既定 stack で溢れました。元 HEAD を独立に展開して
  `RUST_MIN_STACK=4194304` で再実行し成功しました（GUIDE D-45 / D-46 の既存標準）。
  current も同じ標準で成功し、source / expansion / specialization の上限は変えていません。
- PR01 / PM06 / PM07 と numeric / UTF の並行変更は意図して LLVM IR を変えます。
  PB02 の等価性は同じ baseline LLVM を使う比較、または型付き frontend 構造と診断の比較で
  分離し、全 current compiler 対 baseline の IR 差を PB02 の差と混同しません。

## 着手条件と停止条件

### 着手条件

- PX03 が `_perfs/README.md` の状態欄で done であること。`benchmarks/build/generate.mjs` と `benchmarks/run-build.mjs` が存在し、
  PX01 の `TSUZURI_TIME_PASSES` が段の時間を出すこと。実装着手時にはいずれも実装済みです。
  確認: `grep -n "PX01\|PX03\|PB02" _perfs/README.md`、`ls benchmarks/build/generate.mjs benchmarks/run-build.mjs`。
- GUIDE §2.3 の基準コマンドが成功していること。
- 実装手順 1 のベースライン（全 fixture の IR と診断の保存、stack-depth テスト、before の profile と PX03 の計測）を取り終えていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `unsafe`、新しい crate（`smallvec`・`bumpalo`・`typed-arena`・`string-interner` など）、`Cargo.toml` の変更が必要になった。
- 実装手順 1 で保存した IR（`--emit llvm` の `-O0`・`-O3`、native と `--target wasm32`）か診断（コード・メッセージ・位置・順序）が
  1 byte でも変わった。期待値を更新して通すことはしない。
- stack-depth の 4 テスト（`parser::tests::bounds_recursive_and_flat_expression_depth`、
  `tests/polymorphism.rs::bounds_type_growing_polymorphic_recursion`、`tests/computations.rs::bounds_nested_builder_expansion_not_just_source_syntax`、
  `tests/tasks.rs::bounds_nested_task_syntax_and_types`）か `honors_the_exact_specialization_limit` が失敗した。上限・stack を上げたくなった。
- S1 で、instantiate 済みの非 generic template の `body` を後から読む箇所が見つかった（D2 の前提が崩れる）。
- S4 で、`State` に入れる `Local` がモジュールから借用できない（Checker が `Local` をその場で作る）箇所があり、clone か `Rc` が要る。
- before の profile で malloc／free／memmove の割合が 30% 未満、または `polymorph::specialize` が上位 10 関数に入らない
  （2026-09-29 の計測と形が違い、S1〜S5 の優先度の根拠が崩れる）。
- before と before2（同じコンパイラの再計測）の比較で wall time に `改善`・`悪化` が出た（環境の揺れが大きく、効果を判定できない）。

## 現状と計測（HEAD `f8dc655`）

### 計測済みの事実（2026-09-29、合成 1,000 モジュール・124,003 行、Apple M1 Max、macOS 27.0）

- `check` 2.12 s、IR の出力（`build --emit llvm -O0`）2.58 s。コンパイラの最大 RSS は 1.12 GB（1 行あたり約 9 KB）。
  200 モジュール（24,803 行）では `check` 0.39 s・237 MB。処理は単一スレッドで、1 秒あたり約 5 万行。
- 最適化を残したシンボル付きのビルドを 1 ms 間隔で `sample` した結果（IR 出力の実行、約 1,770 サンプル）:
  - 解析（`Project::analyze_all`）が約 73%、IR の生成と書き出し（`build_complete`）が約 25%。
  - top of stack の 57% が malloc／free／memmove。
  - 目立った箇所: 単相化（`polymorph::specialize`）での `CheckedFunction` の丸ごとの複製、所有権検査の 2 回の実行
    （`ownership::infer_copy_all` と `ownership::check_all`。どちらも `check_functions`。合計でおよそ 400 サンプル）、
    `TypedExpr::children_mut`（呼び出しごとに子の `Vec` を確保）、`BTreeSet<usize>` への挿入、`Type` の clone／drop、
    IR 文字列の書式化（`fmt::write`）、IR 文字列の検索（`str::contains`）。
- この合成プロジェクトは 2026-09-29 の一時的な生成器で作った。PX03 の `generate.mjs` の 1,000 モジュールと行数が違う場合は、
  PX03 の生成器の before を基準にし、上の数値とは直接比べない。

### 解析の順序（コードで確認）

`driver::Project::analyze_all` → `analyze_inputs_all`（`src/lib.rs`。字句・構文解析は `lexer::lex` と `parser::parse_with_source_all`）→
`check::check_modules_indexed_all`。後者の終盤は次の順で、段の間で `CheckedModule` を値で受け渡す。

1. 各関数の本体に `computation::expand`、全体に `constants::fold`。
2. `ownership::infer_copy_all(&module)` = `check_functions(module, true)`。誤りがあればここで返る。
3. `polymorph::specialize(module, ...)`。
4. `closures::lower(module)`。
5. `ownership::check_all(&module)` = `check_functions(module, false)`、`gpu::validate_calls`、`diagnostics.check()`。

### 複製と確保の発生源（コードで確認）

| 箇所 | 現状 | 影響 |
| --- | --- | --- |
| `polymorph::specialize` | `Specializer { templates: module.functions.clone(), ... }`。`module.functions` は最後まで保持し、`CheckedModule` を作るときに drop | 型付き IR 全体の複製 1 回と drop 1 回 |
| `Specializer::instantiate` | 先頭で `self.templates[id].clone()`。非 generic の template は `(id, Vec::new())` の 1 回だけ要求される（`Specializer::request` の `keys` で重複を除く） | 全関数の本体をもう 1 回複製。終盤は `module.functions`・`templates`・`functions` の 3 つが同時に生きる |
| `Specializer` の template 参照 | `self.templates[id]` の読み出しは `instantiate` のほか `parameters.len()` と `type_parameters` だけ（`body` は読まない） | S1 の前提（D2） |
| `closures::lower` | `let mut body = module.functions[id].body.clone();` の後 `module.functions[id].body = body;` | 全関数の本体の複製と drop が 1 回ずつ |
| `TypedExpr::children_mut` | `Vec<&mut Self>` を作って返す。呼び出しは `polymorph::walk`（`specialize` の依存の収集と `expression_types`）、`closures.rs`、`constants.rs` の 3 か所 | 節点ごとに 1 回の確保 |
| `TypedExpr::children` | `Vec<&Self>` を返す。呼び出しは 11 ファイルに 22 か所 | Phase 2 の候補（D5） |
| `ownership::State` | `locals: BTreeMap<usize, (Local, Value)>`。`check_body` は `parameter.clone()` を入れ、分岐（`if`・`match`・ループ）で `self.state.clone()` | 分岐ごとに、見えている全 `Local`（`name: String`・`ty: Type`）を deep clone |
| 字句 | `lexer::lex` の token 列は `Vec::new()` から伸ばす。識別子は `TokenKind::Ident(name.to_owned())` | 1 識別子 1 確保。profile の上位には出ていない（D7） |
| `syntax::Ident`・`check::Local` | `text: String`・`name: String` を持つ。`check::Type::Variable(String)` | 名前の intern は Phase 2（D6） |

補足（`grep -c 'clone()'`、行数）: `src/check.rs` 129、`src/polymorph.rs` 123、`src/computation.rs` 93、`src/ownership.rs` 31、
`src/closures.rs` 29、`src/constants.rs` 6、`src/derive.rs` 2。`src/computation.rs` の `body.clone()` は計算式の展開でだけ動き、
合成プロジェクトには計算式がない（D8）。`src/llvm.rs` のテスト以外で `.contains(` を含む行は 25（旧記述の「41」は再現しない）。
IR 文字列の検索と書式化は `check` では動かない（D10）。

`Cargo.toml` は `[profile.release]` に `lto = "thin"`・`codegen-units = 1`・`strip = true`、`[lints.rust]` に `unsafe_code = "forbid"`。
依存は `rustc_apfloat`・`serde_json`（Windows だけ `same-file`）。

### 再現（profile の取り方）

`strip = true` の release ではシンボルが消えるので、環境変数で strip だけを外した別の target ディレクトリで build する（最適化は同じ）。
`sample` の書式は `sample <pid> [duration [samplingInterval]] [-mayDie] [-file <filename>]`（2026-09-29 に `sample` の usage で確認。間隔は ms）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p target/perf/tz-pb02
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir target/perf/tzperf/target
node benchmarks/build/generate.mjs --modules 1000 --out target/perf/tz-pb02/s1000
/usr/bin/time -l target/perf/tzperf/target/release/tsuzuri check target/perf/tz-pb02/s1000 2> target/perf/tz-pb02/before-check.time
target/perf/tzperf/target/release/tsuzuri check target/perf/tz-pb02/s1000 & pid=$!
sample $pid 10 1 -mayDie -file target/perf/tz-pb02/before-check.sample.txt; wait $pid
target/perf/tzperf/target/release/tsuzuri build target/perf/tz-pb02/s1000 --emit llvm -O0 -o target/perf/tz-pb02/s1000.ll --no-cache & pid=$!
sample $pid 10 1 -mayDie -file target/perf/tz-pb02/before-emit.sample.txt; wait $pid
```

- `before-check.time` の `maximum resident set size` が最大 RSS（bytes）、`real` が wall time。
- `sample` はプロセスの終了で止まる（`-mayDie` はシンボルを先に読む）。出力の `Call graph:` が inclusive samples、
  `Sort by top of stack, same collapsed (when >= 5):` が top of stack。
- 集計（top of stack の malloc／free／memmove の割合。5 未満の行は出力に載らないので、分母は載った行の合計）:

```sh
awk '/^Sort by top of stack/{f=1;next} f&&NF==0{exit} f{n=$NF; t+=n; if ($0 ~ /libsystem_malloc|_platform_mem(move|cpy|set)/) m+=n}
  END{printf "top %d, malloc/free/memmove %d (%.1f%%)\n", t, m, 100*m/t}' target/perf/tz-pb02/before-check.sample.txt
grep -E "polymorph::specialize|Specializer::instantiate|closures::lower|ownership::check_functions|children_mut|ownership::Checker" \
  target/perf/tz-pb02/before-check.sample.txt | sort -t' ' -k1,1 | head -40
```

- 関数名は `tsuzuri::polymorph::specialize::h<hash>` の形に demangle される。関数ごとの数は、`Call graph:` の各行で名前の直前の数
  （inclusive）を最初に現れた行（最も浅い呼び出し）から読む。

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before／before2／after を記録して判断する（PX01 の before／after）。

- G1（長期、Phase 1 + 2）: 124K 行の `check` を 0.5 s 以下、最大 RSS を 250 MB 以下にする（単一スレッド）。
- G2（Phase 1）: 型付き IR の丸ごとの複製をなくし、`specialize` の間に同時に生きる型付き本体を減らす。診断と IR は 1 byte も変えない。
- G3（Phase 1）: 小さなプロジェクト（`examples/hello`、200 モジュール）の時間を悪化させない。

| 指標 | 単位・統計 | 対象 | 期待（計算による見込み。計測で確かめる） |
| --- | --- | --- | --- |
| M1 `check` の時間 | s（9 標本の中央値・最小・最大、`wall_time`） | PX03 の合成 1,000・200 モジュール、`examples/hello` | 1,000 で減る。量は主張しない |
| M2 最大 RSS | bytes（9 標本の中央値、`peak_rss`） | M1 と同じ、IR 出力も | 非 generic の関数では、`specialize` 終盤に同時に生きる本体が 3 組（`module.functions`・`templates`・`functions`）から 1 組になる |
| M3 段の時間 | ms（中央値、`phase_time` の `specialize`・`closures`・`ownership`・`check`・`total`） | 合成 1,000 モジュールの `check` | `specialize`（S1・S3・S5）、`closures`（S2・S3）、`ownership`（S4）が減る。`parse`・`load` は変わらない |
| M4 確保の割合 | %（top of stack の malloc／free／memmove ÷ 載った行の合計、1 回の `sample`） | 合成 1,000 モジュールの `check` | before（2026-09-29 は 57%）より下がる |
| M5 関数別 samples | 回（inclusive、1 回の `sample`） | `polymorph::specialize`・`Specializer::instantiate`・`closures::lower`・`ownership::check_functions` | 各関数の数が下がる。`TypedExpr::children_mut` は消える |
| M6 IR 出力の時間 | s（中央値） | 合成 1,000 モジュールの `build --emit llvm -O0` | 解析部分の減少分だけ減る。IR 生成（`build_complete`）自体は変えない |

型付き IR の丸ごとの複製の回数（1 回の解析あたり、非 generic の関数）: `specialize` 2 回 + `closures::lower` 1 回 = 3 回 → 0 回。
generic の template は具体化ごとに 1 回の複製が残る（型の置換で本体を書き換えるため。D2）。

## 変えてはいけない意味

- 診断: コード・メッセージ・位置・順序・件数。`ownership::infer_copy_all` が誤りで先に返る挙動と、`Diagnostics` の上限（`is_full`）も含む。
- 生成 IR: native と `--target wasm32`、`-O0` と `-O3` の `--emit llvm` の出力を byte 単位で保つ。関数の番号（`Specializer::requests` の順）、
  `{name}.$mono.{instance}` の名前、`FunctionOrigin::generated` の値、`recursion::check_specialized` に渡す `requires_rec` を変えない。
- 子の訪問順: `try_for_each_child_mut`（新規）は、削除する `children_mut` が返していた順とまったく同じ順で子を訪ねる。
  `walk` の順は `specialize` の依存 `calls` の順、したがって継承する制約の順と `function.constraints` の順を決める。
- 所有権検査の結果: move・借用・loan の判定と `State::merge` の意味を変えない。`State` の中身の型だけを変える（S4）。
- 資源上限: `MAX_SPECIALIZATIONS`（65,536、GUIDE D-41）、`MAX_CONSTRAINTS`（128）、`syntax::MAX_NESTING`（128）、`constants::inline` の
  `remaining`・`depth` の課金（置き換えの前に `charge`）をそのまま保つ。stack-depth のテストは 2 MiB の stack のまま通す。
- 決定性: 新しい `HashMap` を反復しない。順序を持つ集合は `BTreeMap`／`BTreeSet` のまま。
- 実行時の意味（overflow・丸め・NaN・符号付きゼロ・評価順序・trap・所有権）は IR が同一なので変わらない。WASM の import も変わらない。
- `unsafe_code = "forbid"` を保ち、crate を足さない。

## 設計

### 方針

1. 以後使わない値は複製せず移す（`std::mem::take`・`std::mem::replace`）。
2. 持ち主より短く使う値は借用する（`&'a Local`）。
3. 子の列を集めて返さず、その場で訪ねる（`&mut dyn FnMut`）。
4. profile の上位にない箇所は変えない（D7・D8・D10）。

### Phase 1 の施策（優先順）

| 施策 | 内容 | 根拠（現状の表） | 期待する効果 |
| --- | --- | --- | --- |
| S1 | `specialize` が `module.functions` を `templates` へ移し、`instantiate` が非 generic の template の本体を移す | `templates: module.functions.clone()`、`self.templates[id].clone()` | 本体の複製 2 回と drop 1 回がなくなる。最大 RSS が下がる |
| S2 | `closures::lower` が本体を `std::mem::replace` で取り出す | `module.functions[id].body.clone()` | 本体の複製と drop が 1 回ずつなくなる |
| S3 | `TypedExpr::children_mut` を `try_for_each_child_mut` に置き換え、現行のすべての利用を移して削除する | 節点ごとの `Vec` 確保 | frontend の mutable 子 Vec の確保がなくなる |
| S4 | `ownership::State` の `locals` を `BTreeMap<usize, (&'a Local, Value)>` にする | 分岐ごとの `Local` の deep clone | `State` の複製が `Local` の `String`・`Type` を複製しない |
| S5 | `Specializer::requests`・`Specializer::functions` を `Vec::with_capacity(base_count)` で作る | `Vec<CheckedFunction>` の伸長ごとの memmove | 既知の根の数までの vector 伸長による再確保を減らす |

### データ構造

`src/check.rs` の `impl TypedExpr`（`children_mut` と同じ場所に置き、`children_mut` は削除する）:

```rust
/// Visits the direct children in exactly the order `children` returns them.
pub(crate) fn try_for_each_child_mut(
    &mut self,
    f: &mut dyn FnMut(&mut Self) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    use TypedExprKind::*;
    match &mut self.kind {
        Unary(_, value) | Borrow(value, _) /* ...children_mut と同じ arm... */ => f(value),
        Binary(_, a, b) | Assign(a, b) /* ... */ => {
            f(a)?;
            f(b)
        }
        // Vec を作っていた arm は、同じ順の for 文で f を呼ぶ。
        _ => Ok(()),
    }
}
```

- arm の集合と順は、削除前の `children_mut` の本体を 1 arm ずつ写す。`Match` は `value`、各 arm の各 alternative の `steps`
  （`PatternStep::expression_mut`）→ `bindings` の値、`guard`、`body` の順。`Call` は `callee` の後に `arguments`。
- `dyn` にする理由は D5（1 つの instantiation、recursion 1 段あたりの frame を固定）。

呼び出し側の形（`src/polymorph.rs` の `walk`）:

```rust
fn walk(
    expression: &mut TypedExpr,
    f: &mut impl FnMut(&mut TypedExpr) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    expression.try_for_each_child_mut(&mut |child| walk(child, f))?;
    f(expression)
}
```

`closures::lower_expression` の `_ =>` arm は `expression.try_for_each_child_mut(&mut |child| lower_expression(child, functions, unions, generated, origin))`、
`constants::inline` は `expression.try_for_each_child_mut(&mut |child| inline(child, values, remaining, depth + 1))` にする。

`src/ownership.rs`:

```rust
#[derive(Clone, Default)]
struct State<'a> {
    locals: BTreeMap<usize, (&'a Local, Value)>,
    // aliases・moved・generic_moves は変えない
}

struct Checker<'a, 'run> {
    declarations: BTreeMap<usize, &'a Local>,
    state: State<'a>,
    // module / callback 契約は 'a、診断 / CopyRead 出力は 'run
}
```

`check_body` は借用した引数 iterator と `&'a TypedExpr` から宣言 registry を一度集めます。
入れ子の lambda の宣言は別 checker が集めます。evaluator の式は一時合成式を扱うため短い借用のままです。
`State` に挿入する Local は registry から取り、名前・型を branch snapshot ごとに複製しません。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 型付き IR | `src/check.rs` | `TypedExpr::children_mut` | 削除し、`TypedExpr::try_for_each_child_mut`（新規）を足す（S3） |
| 型付き IR | `src/check.rs` | `TypedExpr::children`、`TypedExpr::error` | 変更なし（`error` は子 module の `polymorph`・`closures` から見える private 関数で、置き場の値に使う） |
| 単相化 | `src/polymorph.rs` | `specialize` | `let templates = std::mem::take(&mut module.functions);` を `Specializer` の構築の前に置き、`original_count`・`base_count`・根の `(id, span)` の列をそこから作る。`requires_rec` の `module.functions.len()` は `original_count` に（S1）。`requests`・`functions` を `Vec::with_capacity(base_count)` に（S5） |
| 単相化 | `src/polymorph.rs` | `Specializer::instantiate` | 非 generic（`type_parameters.is_empty()`）なら `std::mem::replace(&mut template.body, TypedExpr::error(span))` で本体を取り、`CheckedFunction { body, ..template.clone() }`。generic は従来どおり `clone`（S1） |
| 単相化 | `src/polymorph.rs` | `walk` | `try_for_each_child_mut` を使う（S3）。`expression_types` は `walk` 経由なので変更なし |
| 閉包 | `src/closures.rs` | `lower` | `module.functions[id].body.clone()` を `std::mem::replace(&mut module.functions[id].body, TypedExpr::error(span))` に（S2） |
| 閉包 | `src/closures.rs` | `lower_expression` の `_ =>` arm | `try_for_each_child_mut`（S3） |
| 定数 | `src/constants.rs` | `inline` | `try_for_each_child_mut`（S3）。`charge` と `depth + 1` の順は変えない |
| 所有権 | `src/ownership.rs` | `State`、`Checker`、`check_body`、`State` へ入れる経路の関数 | S4。`parameter.clone()` を `parameter` に |
| テスト | `tests/polymorphism.rs` | `specializes_shared_non_generic_callee_once`（新規） | テスト計画 |
| 変更なし | `src/lexer.rs`・`src/parser.rs`・`src/computation.rs`・`src/derive.rs`・`src/llvm*.rs`・`src/call_specialization.rs` | — | D7・D8・D10 |

### 全 Phase の実装結果

| 施策 | 実装結果 |
| --- | --- |
| S1 | templates と非 generic 本体の所有権移動。根・bench・rec 境界を移動前に保存 |
| S2 | 閉包変換へ本体を移動し、完了後に戻す |
| S3 | mutable 子 Vec を廃止。現在増えている `closures_rec`・`const_static`・`gpu_devices` も移行 |
| S4 | Local 宣言を借用した State、registry、loop / try flow の lifetime 伝播 |
| S5 | `base_count` による requests / functions の事前確保 |
| P2-1 | primitive scalar 式だけを構造で証明して具体検査を省略。debug で全検査と照合 |
| P2-2 | 解析単位の名前 intern、own-module の検索時の qualified String 廃止 |
| P2-3 | owned frontend の readonly 走査を allocation-free 訪問へ移行。公開 `children()` は互換用に残す |
| P2-4 | lambda free-local id の昇順 bitset、ownership 使用集合の直接収集 |
| P2-5 | immutable 型述語に TypeId / hash-consed Vec arena。root / 宣言は借用、generated type の子は ID 投影。AST arena は本体移動 / allocation-free 訪問へ設計変更（D9） |

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が 0 でないことを必ず見る（GUIDE §3.1）。「IR 同一」は手順 1 の `ir.sh` の after と before の `diff -r` が空であること。

### 手順 1: ベースラインを取る

- 変更: なし（scratch は `target/perf/tz-pb02/` だけ）。
- 内容: 変更前のコンパイラ 2 つ（stripped の `target/release/tsuzuri` と、シンボル付きの `target/perf/tzperf/target/release/tsuzuri`）を保存し、
  IR と診断の基準、stack-depth テスト、profile 3 回、PX03 の before を取る。`ir.sh` は次の内容で `target/perf/tz-pb02/ir.sh` に置く（commit しない）。

```sh
# usage: sh target/perf/tz-pb02/ir.sh <compiler> <out-dir>
compiler=$1; out=$2
mkdir -p "$out"
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for d in $(find examples tests/fixtures -mindepth 1 -maxdepth 1 -type d | sort) target/perf/tz-pb02/s1000; do
  n=$(echo "$d" | tr '/' '_')
  "$compiler" check "$d" > "$out/$n-check.txt" 2>&1; echo "exit=$?" >> "$out/$n-check.txt"
  for o in -O0 -O3; do
    "$compiler" build "$d" --emit llvm $o --cpu generic --no-cache -o "$out/$n$o.ll" > "$out/$n$o.txt" 2>&1
    echo "exit=$?" >> "$out/$n$o.txt"
    "$compiler" build "$d" --target wasm32 --emit llvm $o --no-cache -o "$out/$n$o-wasm.ll" > "$out/$n$o-wasm.txt" 2>&1
    echo "exit=$?" >> "$out/$n$o-wasm.txt"
  done
done
```

- 確認: 次がすべて成功し、4 つの stack-depth テストと `honors_the_exact_specialization_limit` がそれぞれ `1 passed`。`ir-before` に
  `.ll` と `.txt` がそろう。「現状と計測」の集計で malloc／free／memmove の割合と上位関数を 3 回分記録する（停止条件の 30% を確かめる）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir target/perf/tzperf/target
cp target/release/tsuzuri target/perf/tz-pb02/tsuzuri-before
cp target/perf/tzperf/target/release/tsuzuri target/perf/tz-pb02/tsuzuri-before-sym
node benchmarks/build/generate.mjs --modules 1000 --out target/perf/tz-pb02/s1000
sh target/perf/tz-pb02/ir.sh target/perf/tz-pb02/tsuzuri-before target/perf/tz-pb02/ir-before
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test tasks bounds_nested_task_syntax_and_types
for i in 1 2 3; do
  target/perf/tz-pb02/tsuzuri-before-sym check target/perf/tz-pb02/s1000 & pid=$!
  sample $pid 10 1 -mayDie -file target/perf/tz-pb02/before-check-$i.sample.txt; wait $pid
done
npx --yes --package=node@24 node benchmarks/run-build.mjs target/perf/tz-pb02/tsuzuri-before --out target/perf/PB02-before --sizes 200,1000
```

### 手順 2: `try_for_each_child_mut` と `polymorph::walk`（S3 前半）

- 変更: `src/check.rs` の `impl TypedExpr`、`src/polymorph.rs` の `walk`。
- 内容: 「データ構造」の `try_for_each_child_mut`（新規）を足す。`children_mut` の各 arm を同じ順で写す。`walk` を書き換える。
  `children_mut` はまだ残す。
- 確認: `cargo test --locked --test polymorphism` が成功（N > 0）。手順 1 の stack-depth 5 テストが成功。
  `cargo build --release --locked && sh target/perf/tz-pb02/ir.sh target/release/tsuzuri target/perf/tz-pb02/ir-after && diff -r target/perf/tz-pb02/ir-before target/perf/tz-pb02/ir-after && echo same`
  が `same`（IR 同一）。

### 手順 3: 残りの `children_mut` を移して削除する（S3 後半）

- 変更: `src/closures.rs` の `lower_expression`、`src/constants.rs` の `inline`、`src/check.rs` の `TypedExpr::children_mut`（削除）。
- 内容: 2 か所を `try_for_each_child_mut` に移す。`constants::inline` は `charge` の後に子を辿る順を保つ。
- 確認: `grep -rn "children_mut" src` が空。`cargo test --locked` が成功。stack-depth 5 テストが成功。IR 同一。

### 手順 4: `specialize` と `instantiate` の移動（S1）とテスト

- 変更: `src/polymorph.rs` の `specialize`・`Specializer::instantiate`、`tests/polymorphism.rs`。
- 内容: 「段ごとの変更」の 2 行のとおり。先に `grep -n "templates\[" src/polymorph.rs` の全行が `instantiate`・`parameters.len()`・
  `type_parameters` の読み出しだけであることを確かめる（`body` を読む行があれば停止条件）。テスト計画の
  `specializes_shared_non_generic_callee_once`（新規）を足す。
- 確認: `grep -n "module.functions.clone()" src/polymorph.rs` が空。`cargo test --locked --test polymorphism` が成功（新テストを含む）。
  `cargo test --locked` が成功。IR 同一。

### 手順 5: 事前確保（S5）

- 変更: `src/polymorph.rs` の `specialize`（`Specializer` の構築）。
- 内容: `requests: Vec::with_capacity(base_count)`・`functions: Vec::with_capacity(base_count)`。他の `Vec::new()` は変えない（D7）。
- 確認: `cargo test --locked --test polymorphism` が成功。IR 同一。

### 手順 6: `closures::lower` の移動（S2）

- 変更: `src/closures.rs` の `lower`。
- 内容: `let span = module.functions[id].body.span;` の後、`std::mem::replace(&mut module.functions[id].body, TypedExpr::error(span))` で取り出す。
  `lower_expression` が `functions[id].body` を読まないこと（HEAD の `closures.rs` で `.body` に触れるのは `lower` の 2 行だけ）を
  `grep -n "\.body" src/closures.rs` で確かめる。
- 確認: `grep -n "body.clone()" src/closures.rs` が空。`cargo test --locked` が成功。`node tests/computations.mjs target/release/tsuzuri` が成功。IR 同一。

### 手順 7: 所有権検査の `State` の借用（S4）

- 変更: `src/ownership.rs` の `State`・`Checker`・`check_body` と、`State::locals` へ入れる経路の関数。
- 内容: 「データ構造」のとおり。`grep -n "locals" src/ownership.rs` の全行を確かめ、`(Local, Value)` を `(&'a Local, Value)` に、
  `parameter.clone()`・`local.clone()` を借用に変える。`closed` の `BTreeMap<usize, bool>` は別物なので変えない。
- 確認: `cargo test --locked --test types_ownership --test borrow_syntax --test tasks --test currying` が成功。`cargo test --locked` が成功。
  stack-depth 5 テストが成功。IR 同一（診断の `.txt` を含む）。

### 手順 8: 全体の確認

- 変更: なし。
- 内容: 全テストと E2E を流す。
- 確認: 次がすべて成功し、IR 同一。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo test --locked
cargo build --release --locked
for s in e2e features examples computations tasks control; do
  npx --yes --package=node@24 node tests/$s.mjs target/release/tsuzuri || echo "FAIL $s"
done
sh target/perf/tz-pb02/ir.sh target/release/tsuzuri target/perf/tz-pb02/ir-after && diff -r target/perf/tz-pb02/ir-before target/perf/tz-pb02/ir-after && echo same
```

### 手順 9: after の計測と profile

- 変更: なし。
- 内容: 「計測手順」のとおり。
- 確認: `target/perf/PB02-before`・`PB02-after`・`PB02-before2` がそろい、`report` が throw しない。after の profile 3 回を記録している。

### 手順 10: 文書と状態

- 変更: `docs/benchmarks.md`、`docs/architecture.md`、`_perfs/README.md`。
- 内容: 「ドキュメント」のとおり。
- 確認: `node scripts/check-docs.mjs docs/benchmarks.md docs/architecture.md` が成功。`git diff --check` が空。

## 計測手順

### 環境と条件

PX01 の「環境と条件」と PX03 の「計測手順」に従う（電源接続、他の重い処理なし、`git status --porcelain` が空の tree）。
完了報告に `sysctl -n machdep.cpu.brand_string`、`sw_vers -productVersion`、`rustc --version`、`clang --version | head -1`、
`node --version`、`git rev-parse --short HEAD` を書く（2026-09-29 は Apple M1 Max、macOS 27.0、rustc 1.98.1）。

### 時間と最大 RSS（M1・M2・M3・M6）

PX03 の variant `check` と `emit-llvm` が対象。順序は before → after → before2（PX01 の before／after）。1 run は約 17 分の見込み。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
N24="npx --yes --package=node@24 node"
$N24 benchmarks/run-build.mjs target/release/tsuzuri --out target/perf/PB02-after --sizes 200,1000
$N24 benchmarks/run-build.mjs target/perf/tz-pb02/tsuzuri-before --out target/perf/PB02-before2 --sizes 200,1000
$N24 benchmarks/metrics.mjs report target/perf/PB02-before2 --baseline target/perf/PB02-before
$N24 benchmarks/metrics.mjs report target/perf/PB02-after --baseline target/perf/PB02-before
```

- 標本: 各組で warm-up 1 回と 9 標本（PX03）。統計は中央値・最小・最大。
- 1 つ目の report で `wall_time` に `改善`・`悪化` が出たら停止条件。2 つ目の表を完了報告に貼り、`verdict` をそのまま使う。
- 生データは `target/perf/PB02-*/` に置き、Git に加えない（PX01 D2）。

### profile の比較（M4・M5）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir target/perf/tzperf/target
for i in 1 2 3; do
  target/perf/tzperf/target/release/tsuzuri check target/perf/tz-pb02/s1000 & pid=$!
  sample $pid 10 1 -mayDie -file target/perf/tz-pb02/after-check-$i.sample.txt; wait $pid
done
```

- 各ファイルに「現状と計測」の `awk` と `grep` をかけ、割合（M4）と関数別 samples（M5）の 3 回の中央値を before と並べる。
- LTO で inline された関数は `Call graph:` に出ない。出ない関数は「inline のため不明」と書き、呼び出し元の数で比べる。

## 生成コードの確認

このチケットは生成コードを変えない。確認は「変わらないこと」と「複製の経路が消えたこと」。

1. IR と診断: 同じ baseline LLVM を使う `check-frontend-equivalence.mjs --cli-matrix` が一致（native・wasm32 × `-O0`・`-O3`、CLI の出力と終了コード、
   合成 1,000 モジュールを含む）。
2. 複製の経路: 次の grep がすべて空。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -n "module.functions.clone()" src/polymorph.rs
grep -n "body.clone()" src/closures.rs
grep -rn '\.children_mut(' src
grep -n "(Local, Value)" src/ownership.rs
```

3. `Specializer::instantiate` の `clone` は generic の分岐と `..template.clone()` の 2 か所だけ（`grep -n "template.clone()" src/polymorph.rs`）。

## テスト計画

### Rust テスト

`tests/polymorphism.rs` に 1 件足す（`accepts` を使い、IR を 2 回出して一致を確かめる）。

- `specializes_shared_non_generic_callee_once`（新規）: 非 generic の `helper` を、2 つの型で具体化される generic の `apply` と
  非 generic の `run` の両方から呼ぶ。S1 で本体を移した非 generic の template が 2 回目に読まれないことの回帰。
  assert: `module.functions` のうち `name` が `helper` で終わるものが 1 つ、`apply.$mono.` を含むものが 2 つ。
  次のソースは 2026-09-29 に `target/release/tsuzuri check` と `build --emit llvm` で確認した（`@tz.fn.Main.helper`、
  `@tz.fn.Main.apply.$mono.2`、`@tz.fn.Main.apply.$mono.3` が出る。確認時の引数名 `value` は W1001 の指示どおり `_value` にした）。

```tsuzuri
def helper :: i64 -> i64
fn helper x = x + 1

def apply :: 'a -> i64 -> i64
fn apply _value n = helper n

def run :: i64 -> i64
fn run n = apply true n + apply 2 n + helper n
```

既存の回帰の要: stack-depth の 4 テストと `honors_the_exact_specialization_limit`（手順 1）、`tests/types_ownership.rs` の
`accepts_moves_borrows_partial_moves_and_local_mutation`・`rejects_use_after_move_borrow_conflicts_and_escaping_references`（S4）、
`tests/computations.rs`（`constants`・`closures` を通る）。

### E2E

新しい E2E は足さない（IR が同一なので実行結果も同一）。手順 8 の 6 suite と、`ir.sh` による IR・診断の同一性が意味の保持の確認。

### 既存テストへの影響

なし。期待値を変える必要が出たら停止条件。

### 性能

「計測手順」のとおり。速度の合否閾値を CI に足さない（AGENTS.md）。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/benchmarks.md` | `## ビルド速度` の PB02 節 | 全 Phase の実装範囲、private 型 graph arena と public AST 維持の設計変更を明記。環境、run_id、`report --baseline` の表。統合担当が正式測定し、改善を主張するのは `verdict` が `改善` の行だけ |
| `docs/architecture.md` | `## 性能設計の原則` | 「フロントエンドの段は `CheckedModule` を値で受け渡し、以後使わない本体は複製せず移す。子の走査は `try_for_each_child_mut` で確保しない」の 1 項目 |
| `_perfs/README.md` | 一覧の PB02 の状態欄 | 全 Phase の実装と D9 の代替、正式性能測定の状態を統合担当が記す |

## 受け入れ条件

- [x] S1〜S5 が入り、「生成コードの確認」の grep がすべて空。
- [x] baseline LLVM を使った 121 project の型付き構造・診断・native / WASM IR が一致。CLI 四構成の終了コード・診断 484 outputs、成功時の IR 386 outputs も一致。
- [ ] `cargo test --locked` と手順 8 の E2E 6 suite が成功し、stack-depth の 4 テストと `honors_the_exact_specialization_limit` が変更なしで通る。
- [x] `specializes_shared_non_generic_callee_once` が追加されている。
- [ ] `target/perf/PB02-before`・`PB02-after`・`PB02-before2` と、profile の before／after 各 3 回が記録され、`docs/benchmarks.md` に要約がある。
- [x] PB02 に `unsafe`・新しい crate・`Cargo.toml` の変更がない。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- 訪問順のずれ: `try_for_each_child_mut` の arm を「整理」して順を変えると、`specialize` の `calls` の順が変わり、制約の順と IR が変わる。
  `children_mut` の本体を 1 arm ずつ写し、`ir.sh` の diff で検出する。
- 置き場の値: `std::mem::replace` で残す `TypedExpr::error(span)` を後から読むと、`Type::Error` が伝わって別の誤りになる。
  非 generic の template と `closures::lower` の対象の本体は、取り出した後に読まれないことを grep で確かめてから移す。
- `CheckedFunction { body, ..template.clone() }` を 1 式で書き、`body` の式の中で `std::mem::replace` すると、借用の順が読みにくい。
  本体を先に `let` で取り出してから struct を作る。generic の分岐で `..template.clone()` を使うと本体を 2 回複製するので、generic は `template.clone()` だけにする。
- `specialize` は `module.types()` で `module` を借用したまま `Specializer` を作る。`std::mem::take(&mut module.functions)` は
  `Specializer` の構築より前に `let templates = ...` として行う（同じ struct 式の中では借用が衝突する）。
- `module.functions.len()` は take の後 0 になる。`requires_rec` の比較は take の前に取った `original_count` を使う
  （`templates` は生成した helper の push で伸びるので、`templates.len()` でもない）。
- S4 の lifetime: `impl Checker<'_>` のまま `State<'a>` を持たせると推論が通らない。`impl<'a> Checker<'a>` にして、式を受ける関数の引数を
  `&'a TypedExpr` にする。一時的に作った式を検査している箇所があれば停止条件（clone や `Rc` で逃げない）。
- 再帰の frame: `try_for_each_child_mut` は 1 段ごとに frame を 1 つ足す。`&mut dyn FnMut` にして instantiation を 1 つにし、
  `try_for_each_child_mut` に局所変数を置かない。stack-depth のテストが落ちたら停止条件。
- profile の取り違え: `target/release/tsuzuri`（stripped）を `sample` するとシンボルが出ない。profile は `target/perf/tzperf/target` の binary、
  時間と RSS は stripped の `target/release/tsuzuri` で測る（PX03 の実行ファイルの大きさの metric を混ぜない）。
- `check` の 200 モジュールは 0.4 s 程度で `sample` の標本が少ない。profile は 1,000 モジュールだけで取る。
- `cargo test --locked <pattern>` は 0 件でも成功する。`running N tests` を確かめる。

## 対象外

- 並列化（PB04）、常駐（PB06）、関数単位の増分（PB07・G17）、LLVM 側の時間（PB03・PB05）。
- IR 文字列の検索（`str::contains`）と書式化（`fmt::write`）（D10）。
- `computation::expand`・`derive`・`constants` の構文木の複製（D8）、字句の token 列の再利用（D7）。
- public Type / AST / LLVM API の全面移行（D9 の設計変更）。private 型 graph arena を含む P2-1〜P2-5 は今回の範囲です。

## 決定事項

### D1: Phase 分割と範囲

- 決定: 利用者の承認により S1〜S5 と P2-1〜P2-5 をすべて評価・実装します。健全性を証明できない省略や
  大規模な speculative API 移行は、上記の局所代替と明示した制約に置き換えます。
- 理由: S1〜S5 は局所的な変更で、IR と診断の同一性を手順ごとに確かめられる。Phase 2 は API を広く変えるか、等価性の根拠が要る。
- 状態: 全 Phase 承認済み・実装済み。D9 は private 型 arena と AST 移動への設計変更

### D2: 単相化での本体の移動

- 決定: `specialize` は `module.functions` を `std::mem::take` で `templates` に移す。`instantiate` は非 generic の template の本体を
  `std::mem::replace` で取り出し、generic の template は従来どおり複製する。
- 理由: 非 generic の template は `(id, Vec::new())` として `keys` で 1 回だけ具体化され、以後 `templates[id]` から読むのは
  `parameters.len()`・`type_parameters` だけ（HEAD で確認）。generic の template は型ごとに置換した本体が要るので複製は残る。
  共有した本体と置換表で表す案（旧施策）は、所有権検査・閉包の変換が具体型の本体を前提にするため Phase 1 では採らない。
- 状態: 既定案（実装者はこの案に従う）

### D3: `closures::lower` での本体の移動

- 決定: `std::mem::replace` で本体を取り出し、変換後に戻す。
- 理由: `lower_expression` は `functions` に生成関数を push するが、`functions[id].body` を読まない（HEAD で `.body` に触れるのは `lower` の 2 行だけ）。
- 状態: 既定案（実装者はこの案に従う）

### D4: `Type` の複製と `Rc<Type>`

- 決定: Phase 1 では `Rc<Type>` を入れない。`Type` の複製は S1〜S4 で本体ごと移すことと、`Local` を借用することで減らす。
  個々の `ty.clone()` を `&Type` に変える作業は、profile で `Type` の clone／drop が上位に残った場合に P2 として扱う。
- 理由: `expression_types` などが `&mut Type` を書き換えるので、`Rc<Type>` は全箇所で `Rc::make_mut` が要り、置換で新しい型を作る経路の確保は減らない。
  複製の大部分は本体ごとの複製（S1・S2）と `State` の複製（S4）から来る。
- 状態: 既定案（実装者はこの案に従う）

### D5: 子の訪問の API

- 決定: `TypedExpr::try_for_each_child_mut<'a>(&'a mut self, f: &mut dyn FnMut(&'a mut Self) -> Result<(), Diagnostic>) -> Result<(), Diagnostic>`。
  すべての mutable 子 Vec の利用を移して `children_mut` を削除しました。readonly frontend には
  `try_for_each_child` を追加しました。LLVM の並行利用と公開 API の互換のため `children()` は残します。
- 理由: profile に出たのは `children_mut`。呼び出しは `Diagnostic` を返す再帰・worklist なので 1 つの型で足りる。`dyn` は instantiation を
  1 つにし、再帰 1 段あたりの frame を固定する（2 MiB の stack のテスト）。
- 状態: 既定案（実装者はこの案に従う）

### D6: 名前の intern（Phase 2 の P2-2）

- 決定: `check` の子 module `src/intern.rs` に `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub(super) struct Symbol(u32);` と
  `pub(super) struct Interner { ids: std::collections::HashMap<Rc<str>, Symbol>, names: Vec<Rc<str>> }`（`intern(&mut self, &str) -> Symbol`、
  `lookup(&self, &str) -> Option<Symbol>`）を置く。持ち主は `check::Names`（解析 1 回につき 1 つ）。最初の対象は `format!("{module}.{name}")` で
  `String` を作って `names.functions` を引く箇所（`check.rs` の `names.functions[&format!(...)]`）。`syntax::Ident`・`TokenKind::Ident`・
  `check::Local` の `String` は変えない。
- 理由: `Symbol` に `Ord` を付けないことで、`BTreeMap` の key にして反復順（診断と IR の順）を変える誤りを型で防ぐ。`HashMap` は引くだけで反復しない。
  番号は初出順で決定的。`u32` はソースの大きさの上限（`E0003`）で溢れない。構文木の `String` まで変えると parser・formatter・LSP・docgen の全体に及ぶ。
- 状態: 実装済み。`Rc<str>` は両 container で一つの文字列確保を共有するために使用

### D7: token 列の再利用と事前確保

- 決定: `lexer::lex` の token 列の再利用はしない。`Vec::with_capacity` は S5 の 2 か所だけに足す。
- 理由: 2026-09-29 の profile の上位に字句はない。token 列はファイルごとに parser へ移るので、再利用には API の変更が要る。
  長さを正確に知らない場所での事前確保は、効果を測れない。
- 状態: 既定案（実装者はこの案に従う）

### D8: 計算式の展開・derive・定数の構文木の複製

- 決定: Phase 1 では変えない。PX03 の合成プロジェクトに計算式・deriving はなく、profile の上位にも出ていない。
  計算式の多いプロジェクト（`benchmarks/computations` など）の profile で `computation::` が 5% 以上を占めた場合に、別の perf チケットとして起票する。
- 理由: 測れない改善は入れない（AGENTS.md）。`src/computation.rs` の `body.clone()` は展開の意味（残りの本体の複製）に関わる。
- 状態: 既定案（実装者はこの案に従う）

### D9: 型の intern と arena（Phase 2 の P2-5）

- 2026-10-14 の再開決定: 原案未達のまま停止せず、immutable 型述語の走査に private
  `TypeId(u32)` / hash-consed Vec arena を実装しました。宣言と root は借用し、生成した generic
  field 型だけを所有し、その子は arena ID から投影します。visited は ID に結び付けます。
  public AST / Type / LLVM API は変えません。原案の AST 全移行は、本体の移動と allocation-free
  訪問に置き換える設計変更として確定しました。意味比較と割当経路の regression、matched cost で検証します。
- 原案の受け入れ条件: `TypeId(u32)` による型の hash-consing と、構文木 / 型付き IR の
  Vec-backed arena への全面移行でした。単なるフィールド借用だけでは足りないため、型 graph の
  hash-consing / ID visited / 子投影を実装し、残る AST の表現変更は S1〜S3 に統合しました。
- 決定: `src/type_arena.rs` の TypeId / hash-consed Vec arena を `stored_all_closed` / `reaches`
  が使用します。hash collision は Type の正確な比較で解決し、辞書は反復しません。
  型の初出順、述語の呼出順、closed 境界、cycle で再訪した root の述語評価を維持します。
  leaf / root reject / closed root は arena 自体を確保しません。
- 理由: current `Type` はすでに四 word で、可変の型置換を多数の API が直接受けます。全面移行には
  checker・所有権・LLVM と public IR の表現変更が必要で、今回親が所有する backend と衝突します。
  public AST / IR を arena へ移すだけでは、generic 置換のための更新契約・公開 Box tree の互換を
  自動では解決できません。一方、非 generic の本体複製 3 回は移動でゼロになり、typed child Vec も
  消えています。IR の大規模 API 再設計を目的化せず、この実際の経路除去を新しい AST 条件とします。
- 受け入れ条件: (1) private TypeId / hash-consed Vec arena と ID visited が実装される、
  (2) nongeneric / root の Type を clone しない、(3) generated type は owner 一つと ID 子投影で走査する、
  (4) 正確な述語順・closed / cycle・hash collision・frontend / IR / 診断の等価性を回帰で確認する、
  (5) 非 generic typed 本体の deep clone と mutable child Vec がゼロ、
  (6) matched before / after / before2 で追加 metadata の cost も評価する。
- 検証済みの型格納量: depth 64 の reference walker は Box clone 経路 2,144 個、
  borrowed arena は Type allocation 0 個・65 IDs。generated root は owner 一つ・64 子投影で、
  子の Type を複製しません。これは Type payload の数で、HashMap / Vec metadata の割当を含む RSS ではありません。
- 数値条件: G1 の 124K 行 0.5秒 / 250MB は長期目標で P2-5 の即時合否条件ではありません。
  個別速度の改善率は正式測定まで主張しません。設計を変えた事実を「全 AST arena 実装」とは呼びません。
- 状態: 再定義した実装条件と最終全 corpus 比較は完了。親の正式 after / before2 測定へ引き継ぎます。

### D10: IR 文字列の検索と書式化

- 決定: PB02 では扱わない。IR の生成・書き出し（`build_complete`、2026-09-29 の約 25%）は PB03 に委ね、PB02 は profile での割合の記録だけを行う。
- 理由: 目標の `check` は IR を作らない。`src/llvm.rs` の連結判定（`output.contains("@tz.rec.")` など）を生成時の記録に変えるには、
  ランタイムを呼ぶ全 emission 箇所に触れる必要があり、フロントエンドの確保削減とは別の変更になる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し提案: PB03 の現行の記述に IR 文字列の検索の扱いがない。PB03 の詳細化で引き受けるかを調整する。

### D11: 所有権検査の `State` の借用

- 決定: `State<'a>` の `locals` を `BTreeMap<usize, (&'a Local, Value)>` にする。`Rc<Local>` は使わない。
- 理由: evaluator は一時 capture / view 式も合成しますが、Local の宣言は関数の引数か束縛に存在します。
  宣言 registry を一度作り、状態は宣言を借用します。分岐 snapshot は Local の名前・型を複製しません。
  registry の一回の走査・辞書確保は増えるため、net wall / RSS の利益は正式測定の結果に従います。
- 状態: 実装済み。宣言と診断出力の lifetime は分離済み

## 実装検証の記録（2026-10-14）

### 正しさ

- `cargo check --locked --lib` 成功。
- frontend に絞った 8 suite（polymorphism / computations / types_ownership / borrow_syntax /
  tasks / currying / borrowed_records / control）: 159 tests 成功。
- scalar 再検査省略追加後に ownership / borrow / control を再実行: 52 tests 成功。
- allocation-free の child 順序・mutable early error、freeze の opaque / Channel / Atomic / Mutex、
  scalar 分類の借用・call・束縛除外: 3 unit tests 成功。interner の初出順・確保共有: 1 test 成功。
- parser depth、type-growing recursion、nested builder、task depth、exact specialization limit は
  既定 stack で成功。`bounds_computation_syntax_and_expansion` は既存 CI の
  `RUST_MIN_STACK=4194304` で current / 元 HEAD とも成功しました。
  元 HEAD の独立再実行は `target/perf/PB02-baseline-stack/`、実行結果は同所の `result.log` にあります。
- 初期 mutable visitor による nested builder の既定 stack overflow は、dispatch を直接
  iterator return にし Match helper を分けて解消しました。上限・stack の引き上げはしていません。

### PB02 だけの厳密な等価性

```sh
node benchmarks/check-frontend-equivalence.mjs target/perf/PB02-equivalence --baseline-ref=a621fada69bf27f425bfbb012cc7d0a699721db6
node benchmarks/check-frontend-equivalence.mjs target/perf/PB02-equivalence --reuse --cli-matrix
```

baseline HEAD の source / std / manifests を project 内に展開し、PB02 frontend だけを overlay します。
両側の LLVM / runtime は baseline のままです。所有権の PM07 proof-root 拡張だけを
staging の after で baseline の規則に戻し、別 stream の intentional IR 差を除外します。
元の辞書・関数・capture・診断の Debug 構造と順序も byte 比較します。

121 projects（examples / fixtures、所有権・名前解決・multi-error 順序・特殊化の診断 12 件、
合成 200・1,000 モジュール）の構造・診断が完全一致し、
成功した 99 projects の library IR は native / WASM の計 198 outputs が完全一致しました。
CLI matrix は保存済みの before / after binary を再利用し、native / WASM × `-O0` / `-O3` の
IR・終了コード・stdout / stderr を比較しました。484 configurations すべてで終了コード・診断が一致し、
成功した 386 configurations の IR も完全一致しました。残る 98 configurations は baseline と同じ失敗です。
指定した出力ファイル名だけを正規化し、診断コード・本文・入力の位置・順序は変えず比較します。
`comparison.json` に tested source SHA-256 を保存しました。tested frontend source は current と同一で、
その後の ownership unit test の追加だけは release binary に含まれません。
IR 期待値・診断期待値の更新はありません。統合 compiler が baseline と異なる backend IR を出すことは、
この PB02 単独比較の失敗を意味しません。

### 性能と引き継ぎ

`benchmarks/run-frontend.mjs` は cache を無効にし、hello / 合成 200 / 合成 1,000 の
check と native LLVM emission の wall / CPU / RSS / 段別時間を記録します。
warmup 1 回、測定 9 回、毎回の診断・IR 同一性を確認します。project 内の report file だけを使います。
quick smoke の hello / 合成 10 × check / emission は成功しましたが、正式な速度の証拠とは扱いません。

```sh
node benchmarks/run-frontend.mjs target/perf/PB02-equivalence/tsuzuri-before --out target/perf/PB02-before --sizes 200,1000
node benchmarks/run-frontend.mjs target/perf/PB02-equivalence/tsuzuri-after --out target/perf/PB02-after --sizes 200,1000
node benchmarks/run-frontend.mjs target/perf/PB02-equivalence/tsuzuri-before --out target/perf/PB02-before2 --sizes 200,1000
node benchmarks/metrics.mjs report --baseline target/perf/PB02-before target/perf/PB02-after
node benchmarks/metrics.mjs report --baseline target/perf/PB02-before target/perf/PB02-before2
```

numeric 全数検査の 6 worker と並行検証が CPU を使用しているため、親の指示で正式測定は開始していません。
統合担当が負荷解消後の中央 benchmark・full cargo / fmt / clippy / docs を所有します。
本体 / Local の deep clone と child Vec の経路除去は実装と等価性で確認済みですが、
P2-2 の辞書と S4 registry の追加 cost も含め、速度・RSS の改善値は結果が出るまで主張しません。

PB02 自体は syntax / semantics / CLI / diagnostics / limit を変更しません。
language-reference の利用者向け契約変更はありません。`languages/strategy.md` の性能設計には
private TypeId / hash-consed Vec arena の実装と、public Type / AST 全移行ではないことを追記しました。
この prose-only 変更の docs gate は baseline build 測定終了後に統合担当が実施します。
PR01 の LLVM facts と PM07 の copy proof に関する
利用者向け性能説明、および中央 architecture / README / benchmark 文書は統合担当に引き継ぎました。
ticket は移動せず、commit / push はしていません。

### P2-5 完了時の追加検証・source freeze

最終 candidate の Type arena SHA-256:
`1d91ddaea04da5163ecf0f3352acca758badc687f1e3eb784ace8d7e58ae74fd`。
baseline の binary は再ビルドせず `--refresh-after --cli-matrix` で frontend 側だけを更新しました。
最終 121 projects の frontend / diagnostics、584 IR、484 CLI diagnostic / exit comparisons は全一致です。
typed source の SHA と current が一致することも確認しました。PM07 の隔離は前述のとおりです。

追加 Type arena tests は 4 件成功（hash collision、borrowed / owned canonicalization、ID 子投影、
reference walker との述語順 / cycle / closed 境界、Box clone 経路数を含む）。
最終 ownership / borrow / control / param_facts は計 61 tests 成功、allocation / freeze / scalar は 3 tests 成功。
parser / growing-type / nested-builder / task depth は既定 stack のまま各 1 test 成功です。
exact specialization は integration のフィルタではなく
`cargo test --locked --lib honors_the_exact_specialization_limit --quiet` で実際の 1 test の成功を確認しました。
`cargo clippy --locked --all-targets -- -D warnings` と scoped rustfmt は成功しました。
Windows x64 / ARM64 は arena 追加後の rustup stable `cargo check --locked --lib --target ...` が両方成功です。

実装を source freeze しました。所有する release refresh / cargo / cross-check はすべて終了済みです。
最終 candidate の optional quick 再実行は、親の build-before 計測開始の通知を受けて停止しました。
全 corpus の証拠は完成しており、旧 quick の結果を final arena の証拠とは混同しません。
正式 frontend before（9 samples）は親が
`target/perf/phase2-formal/frontend-before/frontend.jsonl` に保存済みで、after / before2 は統合担当が実施します。
Type payload の Box 経路削減は検証済みですが、arena metadata の追加 cost を含む wall / RSS の判定は
この matched measurement の結果に従います。

### CI 用の限定実行と Windows 型検査

以下の限定実行を `targets.yml` の macOS/Linux に接続しました。Windows では cross 型検査と通常の Rust 回帰を用い、この isolation runner は実行しません。
fresh quick の実コマンドはローカルで 397.74 秒、22 project・88 CLI 比較・60 IR 出力で差分 0、frontend smoke も成功しました。
commit 後の HEAD 同士を比較して等価性が自明にならないよう、元の baseline commit を fetch し、
`--baseline-ref=<SHA>` で指定します。`--reuse` は保存した baseline revision を保ちます。
`--quick` は hello、子訪問の 8 fixtures、12 件の診断、合成 10 モジュールの 22 projects に限定します。
CLI matrix の 88 configurations が一致することを既存の isolated binary で確認しました。
quick の結果は `comparison-quick.json` とし、全 corpus の `comparison.json` は上書きしません。

```sh
git fetch --no-tags --depth=1 origin a621fada69bf27f425bfbb012cc7d0a699721db6
RUST_MIN_STACK=4194304 LC_ALL=C node benchmarks/check-frontend-equivalence.mjs target/perf/PB02-ci-equivalence --baseline-ref=a621fada69bf27f425bfbb012cc7d0a699721db6 --quick --cli-matrix
RUST_MIN_STACK=4194304 LC_ALL=C node benchmarks/run-frontend.mjs target/release/tsuzuri --out target/perf/PB02-ci-benchmark --quick
```

fresh preparation は full corpus で、quick equivalence は `--reuse` で検証済みです。
frontend quick smoke の既存記録は `target/perf/PB02-runner-smoke/projects.json` にあり、
使用 compiler は `target/perf/phase2-baseline/tsuzuri-before` です。
上記の fresh / pinned / quick の組合せと CI 上の所要時間は、まだ end-to-end では検証していません。
smoke の 5 分予算は candidate `target/release/tsuzuri` の build を含まず、既存 release build step を前提とします。

保存済みの同一 host 用 baseline / candidate CLI と snapshot helper がある場合は、
次の実行がどちらの compiler もビルドせず quick 比較します。
fresh CI の `target/` にはこれらの binary はないため、`--reuse` の前に適合する artifact の復元が必要です。
baseline binary を配布しない CI では、既存 full cargo test に含まれる Type arena reference walker、
allocation-free visitor、specialization の回帰を gate とし、archived compiler 間の byte 比較と同一だとは呼びません。

```sh
node benchmarks/check-frontend-equivalence.mjs target/perf/PB02-equivalence --reuse --quick --cli-matrix --baseline-ref=a621fada69bf27f425bfbb012cc7d0a699721db6
```

中央 CI の equivalence step 上限は `timeout-minutes: 25`、benchmark smoke は
`timeout-minutes: 5` です。workflow への接続と CI 実行の検証は統合担当が行います。
equivalence の binary preparation は release build を各側一回だけ、各 subprocess の上限は 600 秒です。
これは全体の上限ではなく、cold runner で 25 / 5 分以内に成功するという保証はありません。
RSS benchmark とこの isolation runner は macOS / Linux 用です。CI smoke に速度の合否閾値は設けません。

Windows x64 / ARM64 の `cargo check --locked --all-targets` はいずれも成功しました。
Homebrew の rustc 1.98.1 に Windows std がないため、installed targets を持つ rustup stable
1.96.1 の cargo / rustc を明示して確認しました。Windows での実行結果を保証するものではありません。

```sh
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo check --locked --all-targets --target x86_64-pc-windows-msvc
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo check --locked --all-targets --target aarch64-pc-windows-msvc
```
