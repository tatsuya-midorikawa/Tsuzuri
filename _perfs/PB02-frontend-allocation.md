# PB02: フロントエンドの確保削減と高速化

| 項目 | 内容 |
| --- | --- |
| ID | PB02 |
| 分類 | ビルド速度 |
| 優先度 | P0 |
| 規模 | L |
| 依存 | PX03 |
| 関連 | PB04, PB06, G17, G07 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要。要承認: D9（Phase 2 の型の intern と arena。型検査全体の API を変える） |
| 手本にする既存実装 | 所有権を移して複製を避ける: `polymorph::specialize` の `std::mem::take(&mut function.constraints)`。生成済みの本体の差し替え: `closures::lower` の `module.functions[id].body = body`。借用で子を辿る: `TypedExpr::children`（`&Self` 版）。計測: PX03 の `benchmarks/run-build.mjs`・`benchmarks/build/generate.mjs`、PX01 の `TSUZURI_TIME_PASSES` と `node benchmarks/metrics.mjs report --baseline` |
| 主な影響ファイル | `src/polymorph.rs`（`specialize`・`Specializer`・`Specializer::instantiate`・`walk`）, `src/closures.rs`（`lower` と `children_mut` の呼び出し）, `src/constants.rs`（`children_mut` の呼び出し）, `src/check.rs`（`TypedExpr::children_mut` の削除、`TypedExpr::try_for_each_child_mut`（新規））, `src/ownership.rs`（`State`・`Checker`・`check_body`）, `tests/polymorphism.rs`（テスト 1 件追加）, `docs/benchmarks.md`, `docs/architecture.md`, `_perfs/README.md`（状態欄） |
| 計測対象 | PX03 の合成 1,000 モジュール（`check` と `build --emit llvm -O0`）の wall time・peak RSS・段階別時間。`sample` による top-of-stack の malloc／free／memmove の割合と、関数別の inclusive samples |

## 目的

コンパイラのフロントエンド（構文解析から型付き IR の完成まで）の確保と複製を減らし、単一スレッドでの `check` を速く、最大 RSS を小さくする。
並列化（PB04）・常駐サーバー（PB06）・増分（G17）の前に、1 スレッドあたりの仕事を減らす。PB04 が並列にする各段も、この変更で軽くなる。

実装者は Phase 1（S1〜S5）だけを実装する。Phase 2 は人間が依頼した場合だけ着手する（D1）。

## 着手条件と停止条件

### 着手条件

- PX03 が `_perfs/README.md` の状態欄で done であること。`benchmarks/build/generate.mjs` と `benchmarks/run-build.mjs` が存在し、
  PX01 の `TSUZURI_TIME_PASSES` が段の時間を出すこと。HEAD `f8dc655` ではどちらも未実装（`ls benchmarks/build` が失敗する）。
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
mkdir -p /tmp/tz-pb02
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir /tmp/tzperf/target
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tz-pb02/s1000
/usr/bin/time -l /tmp/tzperf/target/release/tsuzuri check /tmp/tz-pb02/s1000 2> /tmp/tz-pb02/before-check.time
/tmp/tzperf/target/release/tsuzuri check /tmp/tz-pb02/s1000 & pid=$!
sample $pid 10 1 -mayDie -file /tmp/tz-pb02/before-check.sample.txt; wait $pid
/tmp/tzperf/target/release/tsuzuri build /tmp/tz-pb02/s1000 --emit llvm -O0 -o /tmp/tz-pb02/s1000.ll --no-cache & pid=$!
sample $pid 10 1 -mayDie -file /tmp/tz-pb02/before-emit.sample.txt; wait $pid
```

- `before-check.time` の `maximum resident set size` が最大 RSS（bytes）、`real` が wall time。
- `sample` はプロセスの終了で止まる（`-mayDie` はシンボルを先に読む）。出力の `Call graph:` が inclusive samples、
  `Sort by top of stack, same collapsed (when >= 5):` が top of stack。
- 集計（top of stack の malloc／free／memmove の割合。5 未満の行は出力に載らないので、分母は載った行の合計）:

```sh
awk '/^Sort by top of stack/{f=1;next} f&&NF==0{exit} f{n=$NF; t+=n; if ($0 ~ /libsystem_malloc|_platform_mem(move|cpy|set)/) m+=n}
  END{printf "top %d, malloc/free/memmove %d (%.1f%%)\n", t, m, 100*m/t}' /tmp/tz-pb02/before-check.sample.txt
grep -E "polymorph::specialize|Specializer::instantiate|closures::lower|ownership::check_functions|children_mut|ownership::Checker" \
  /tmp/tz-pb02/before-check.sample.txt | sort -t' ' -k1,1 | head -40
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
- 資源上限: `MAX_SPECIALIZATIONS`（1,024 + `base_count`）、`MAX_CONSTRAINTS`（128）、`syntax::MAX_NESTING`（128）、`constants::inline` の
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
| S3 | `TypedExpr::children_mut` を `try_for_each_child_mut`（新規）に置き換え、3 か所の呼び出しを移して削除する | 節点ごとの `Vec` 確保 | `walk`・`lower_expression`・`constants::inline` の確保がなくなる |
| S4 | `ownership::State` の `locals` を `BTreeMap<usize, (&'a Local, Value)>` にする | 分岐ごとの `Local` の deep clone | `State` の複製が `Local` の `String`・`Type` を複製しない |
| S5 | `Specializer::requests`・`Specializer::functions` を `Vec::with_capacity(base_count)` で作る | `Vec<CheckedFunction>` の伸長ごとの memmove | 非 generic のプログラムでは再確保がなくなる |

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

struct Checker<'a> {
    state: State<'a>,
    // 他の field は変えない
}
```

`impl Checker<'_>` は `impl<'a> Checker<'a>` にし、`State` へ `Local` を入れる経路の式の引数を `&'a TypedExpr` にする。
`check_body` の `body: &TypedExpr` と `parameters: &[Local]` は `module: &'a CheckedModule` と同じ `'a` にする（どちらも `module.functions` の中）。

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

### Phase 分割

- Phase 1（この実装者の範囲）: S1〜S5。各施策を別々の手順で入れ、手順ごとに IR と診断の同一性を確かめる。
- Phase 2（人間が依頼した場合だけ。設計方針）:
  - P2-1: 所有権検査の 2 回目（`check_all`）を、単相化・閉包の変換で本体が変わった関数に限る。等価性の根拠がないので、
    debug build で全関数版と限定版の結果を比べる `debug_assert_eq!` を全テストで通してから release の経路を切り替える。
  - P2-2: 名前の intern（D6）。
  - P2-3: `TypedExpr::children` の 22 か所のうち再帰で呼ぶものを `try_for_each_child`（新規）へ（D5）。
  - P2-4: 関数 id・local id の `BTreeSet<usize>` を `Vec<u64>` の bit set（昇順の反復）へ。profile で挿入元を特定してから。
  - P2-5: 型の intern と型付き IR の arena（D9、要承認）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が 0 でないことを必ず見る（GUIDE §3.1）。「IR 同一」は手順 1 の `ir.sh` の after と before の `diff -r` が空であること。

### 手順 1: ベースラインを取る

- 変更: なし（scratch は `/tmp/tz-pb02/` だけ）。
- 内容: 変更前のコンパイラ 2 つ（stripped の `target/release/tsuzuri` と、シンボル付きの `/tmp/tzperf/target/release/tsuzuri`）を保存し、
  IR と診断の基準、stack-depth テスト、profile 3 回、PX03 の before を取る。`ir.sh` は次の内容で `/tmp/tz-pb02/ir.sh` に置く（commit しない）。

```sh
# usage: sh /tmp/tz-pb02/ir.sh <compiler> <out-dir>
compiler=$1; out=$2
mkdir -p "$out"
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for d in $(find examples tests/fixtures -mindepth 1 -maxdepth 1 -type d | sort) /tmp/tz-pb02/s1000; do
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
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir /tmp/tzperf/target
cp target/release/tsuzuri /tmp/tz-pb02/tsuzuri-before
cp /tmp/tzperf/target/release/tsuzuri /tmp/tz-pb02/tsuzuri-before-sym
node benchmarks/build/generate.mjs --modules 1000 --out /tmp/tz-pb02/s1000
sh /tmp/tz-pb02/ir.sh /tmp/tz-pb02/tsuzuri-before /tmp/tz-pb02/ir-before
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked --test tasks bounds_nested_task_syntax_and_types
for i in 1 2 3; do
  /tmp/tz-pb02/tsuzuri-before-sym check /tmp/tz-pb02/s1000 & pid=$!
  sample $pid 10 1 -mayDie -file /tmp/tz-pb02/before-check-$i.sample.txt; wait $pid
done
npx --yes --package=node@24 node benchmarks/run-build.mjs /tmp/tz-pb02/tsuzuri-before --out target/perf/PB02-before --sizes 200,1000
```

### 手順 2: `try_for_each_child_mut` と `polymorph::walk`（S3 前半）

- 変更: `src/check.rs` の `impl TypedExpr`、`src/polymorph.rs` の `walk`。
- 内容: 「データ構造」の `try_for_each_child_mut`（新規）を足す。`children_mut` の各 arm を同じ順で写す。`walk` を書き換える。
  `children_mut` はまだ残す。
- 確認: `cargo test --locked --test polymorphism` が成功（N > 0）。手順 1 の stack-depth 5 テストが成功。
  `cargo build --release --locked && sh /tmp/tz-pb02/ir.sh target/release/tsuzuri /tmp/tz-pb02/ir-after && diff -r /tmp/tz-pb02/ir-before /tmp/tz-pb02/ir-after && echo same`
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
sh /tmp/tz-pb02/ir.sh target/release/tsuzuri /tmp/tz-pb02/ir-after && diff -r /tmp/tz-pb02/ir-before /tmp/tz-pb02/ir-after && echo same
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
$N24 benchmarks/run-build.mjs /tmp/tz-pb02/tsuzuri-before --out target/perf/PB02-before2 --sizes 200,1000
$N24 benchmarks/metrics.mjs report target/perf/PB02-before2 --baseline target/perf/PB02-before
$N24 benchmarks/metrics.mjs report target/perf/PB02-after --baseline target/perf/PB02-before
```

- 標本: 各組で warm-up 1 回と 9 標本（PX03）。統計は中央値・最小・最大。
- 1 つ目の report で `wall_time` に `改善`・`悪化` が出たら停止条件。2 つ目の表を完了報告に貼り、`verdict` をそのまま使う。
- 生データは `target/perf/PB02-*/` に置き、Git に加えない（PX01 D2）。

### profile の比較（M4・M5）

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --locked --target-dir /tmp/tzperf/target
for i in 1 2 3; do
  /tmp/tzperf/target/release/tsuzuri check /tmp/tz-pb02/s1000 & pid=$!
  sample $pid 10 1 -mayDie -file /tmp/tz-pb02/after-check-$i.sample.txt; wait $pid
done
```

- 各ファイルに「現状と計測」の `awk` と `grep` をかけ、割合（M4）と関数別 samples（M5）の 3 回の中央値を before と並べる。
- LTO で inline された関数は `Call graph:` に出ない。出ない関数は「inline のため不明」と書き、呼び出し元の数で比べる。

## 生成コードの確認

このチケットは生成コードを変えない。確認は「変わらないこと」と「複製の経路が消えたこと」。

1. IR と診断: `diff -r /tmp/tz-pb02/ir-before /tmp/tz-pb02/ir-after` が空（native・wasm32 × `-O0`・`-O3`、`check` の出力と終了コード、
   合成 1,000 モジュールを含む）。
2. 複製の経路: 次の grep がすべて空。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -n "module.functions.clone()" src/polymorph.rs
grep -n "body.clone()" src/closures.rs
grep -rn "children_mut" src
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
| `docs/benchmarks.md` | `## ビルド速度`（PX03 が作る）の下に `### PB02 Phase 1 のフロントエンド確保削減`（新規） | 環境、run_id（`PB02-before`・`PB02-after`・`PB02-before2`）、`report --baseline` の表、profile の割合と関数別 samples の before／after（3 回の中央値）。改善を主張するのは `verdict` が `改善` の行だけ |
| `docs/architecture.md` | `## 性能設計の原則` | 「フロントエンドの段は `CheckedModule` を値で受け渡し、以後使わない本体は複製せず移す。子の走査は `try_for_each_child_mut` で確保しない」の 1 項目 |
| `_perfs/README.md` | 一覧の PB02 の状態欄 | Phase 1 の完了を記す（Phase 2 は未着手と書く） |

## 受け入れ条件

- [ ] S1〜S5 が入り、「生成コードの確認」の grep がすべて空。
- [ ] `diff -r /tmp/tz-pb02/ir-before /tmp/tz-pb02/ir-after` が空（IR・診断・終了コード）。
- [ ] `cargo test --locked` と手順 8 の E2E 6 suite が成功し、stack-depth の 4 テストと `honors_the_exact_specialization_limit` が変更なしで通る。
- [ ] `specializes_shared_non_generic_callee_once` が追加されている。
- [ ] `target/perf/PB02-before`・`PB02-after`・`PB02-before2` と、profile の before／after 各 3 回が記録され、`docs/benchmarks.md` に要約がある。
- [ ] `unsafe`・新しい crate・`Cargo.toml` の変更がない。
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
- profile の取り違え: `target/release/tsuzuri`（stripped）を `sample` するとシンボルが出ない。profile は `/tmp/tzperf/target` の binary、
  時間と RSS は stripped の `target/release/tsuzuri` で測る（PX03 の実行ファイルの大きさの metric を混ぜない）。
- `check` の 200 モジュールは 0.4 s 程度で `sample` の標本が少ない。profile は 1,000 モジュールだけで取る。
- `cargo test --locked <pattern>` は 0 件でも成功する。`running N tests` を確かめる。

## 対象外

- 並列化（PB04）、常駐（PB06）、関数単位の増分（PB07・G17）、LLVM 側の時間（PB03・PB05）。
- IR 文字列の検索（`str::contains`）と書式化（`fmt::write`）（D10）。
- `computation::expand`・`derive`・`constants` の構文木の複製（D8）、字句の token 列の再利用（D7）。
- Phase 2 の P2-1〜P2-5（人間が依頼した場合だけ。P2-5 は D9 の承認も要る）。

## 決定事項

### D1: Phase 分割と範囲

- 決定: Phase 1 は S1〜S5 だけ。Phase 2（P2-1〜P2-5）は人間が依頼した場合だけ着手する。
- 理由: S1〜S5 は局所的な変更で、IR と診断の同一性を手順ごとに確かめられる。Phase 2 は API を広く変えるか、等価性の根拠が要る。
- 状態: 既定案（実装者はこの案に従う）

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

- 決定: `TypedExpr::try_for_each_child_mut(&mut self, f: &mut dyn FnMut(&mut Self) -> Result<(), Diagnostic>) -> Result<(), Diagnostic>`（新規）。
  `children_mut` の 3 か所を移して `children_mut` を削除する。`children` の 22 か所は Phase 1 では変えない（P2-3 では同じ形の
  `try_for_each_child`（新規）を使う）。
- 理由: profile に出たのは `children_mut`。3 か所の呼び出しはどれも `Diagnostic` を返す再帰なので 1 つの型で足りる。`dyn` は instantiation を
  1 つにし、再帰 1 段あたりの frame を固定する（2 MiB の stack のテスト）。
- 状態: 既定案（実装者はこの案に従う）

### D6: 名前の intern（Phase 2 の P2-2）

- 決定: `src/intern.rs`（新規）に `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub(crate) struct Symbol(u32);` と
  `pub(crate) struct Interner { ids: std::collections::HashMap<Box<str>, Symbol>, names: Vec<Box<str>> }`（`intern(&mut self, &str) -> Symbol`、
  `text(&self, Symbol) -> &str`）を置く。持ち主は `check::Names`（解析 1 回につき 1 つ）。最初の対象は `format!("{module}.{name}")` で
  `String` を作って `names.functions` を引く箇所（`check.rs` の `names.functions[&format!(...)]`）。`syntax::Ident`・`TokenKind::Ident`・
  `check::Local` の `String` は変えない。
- 理由: `Symbol` に `Ord` を付けないことで、`BTreeMap` の key にして反復順（診断と IR の順）を変える誤りを型で防ぐ。`HashMap` は引くだけで反復しない。
  番号は初出順で決定的。`u32` はソースの大きさの上限（`E0003`）で溢れない。構文木の `String` まで変えると parser・formatter・LSP・docgen の全体に及ぶ。
- 状態: 既定案（Phase 2 は人間が依頼した場合だけ着手する）

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

- 決定: 型は `TypeId(u32)`（新規）で hash-consing し、構文木と型付き IR は標準ライブラリの `Vec` と添字で arena に置く。依存 crate は足さない。
- 理由: `Type` の clone／drop と比較を id の操作にでき、個別の `Box` の確保をなくせる。ただし型検査・単相化・所有権検査・IR 生成の API を広く変える。
- 状態: 要承認（承認前は P2-5 に着手しない）

### D10: IR 文字列の検索と書式化

- 決定: PB02 では扱わない。IR の生成・書き出し（`build_complete`、2026-09-29 の約 25%）は PB03 に委ね、PB02 は profile での割合の記録だけを行う。
- 理由: 目標の `check` は IR を作らない。`src/llvm.rs` の連結判定（`output.contains("@tz.rec.")` など）を生成時の記録に変えるには、
  ランタイムを呼ぶ全 emission 箇所に触れる必要があり、フロントエンドの確保削減とは別の変更になる。
- 状態: 既定案（実装者はこの案に従う）
- 見直し提案: PB03 の現行の記述に IR 文字列の検索の扱いがない。PB03 の詳細化で引き受けるかを調整する。

### D11: 所有権検査の `State` の借用

- 決定: `State<'a>` の `locals` を `BTreeMap<usize, (&'a Local, Value)>` にする。`Rc<Local>` は使わない。
- 理由: 検査する `Local` はすべて `module.functions` の引数か本体の束縛で、`Checker` の間ずっと生きる。借用なら分岐ごとの `State` の複製が
  `Local` を複製せず、確保も増えない。`Rc` は挿入ごとの確保が残る。
- 状態: 既定案（実装者はこの案に従う）
