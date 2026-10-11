# D12: ランタイム IR と生成コードの intrinsic 宣言の重複（i32 の Int.min／Int.max／Int.leading_zeros）

| 項目 | 内容 |
| --- | --- |
| ID | D12 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | D04 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-10-07（言語リファレンス `_tsuzuri/language-reference/` の執筆中に発見。HEAD `5b896b7` で確認） |
| 実装の基準 | `aecdf75e`、2026-10-12 |
| 承認 | 不要（言語の意味は変えない。生成 IR を正しくする修正） |
| 改善する劣位 | 不具合: 数値を表示・書式化・解析するプログラムで、`i32` の `Int.min`・`Int.max` と、`i32`／`i32u` の `Int.leading_zeros` を使うとビルドが失敗する |
| 手本にする既存実装 | `src/llvm.rs` の `emit_program` で `math.ll` を連結した直後の、`declare` 行を名前で一度だけ残す filter（`BTreeSet`）。後段で宣言を足す前に存在を確かめる `src/llvm_traps.rs` の `declare i64 @write(` と `src/llvm_cpu.rs` の `declare i32 @tsuzuri_cpu_pick(i64)`。テスト: `tests/integers.rs` の `integer_bit_and_selection_intrinsics_are_typed_and_deterministic`、`tests/stdlib.rs` の `library`、`tests/integer_intrinsics.mjs` |
| 主な影響ファイル | `src/llvm.rs`, `src/driver.rs`, `tests/integers.rs`, `tests/stdlib.rs`, `tests/strings.rs`, `tests/display_parse.rs`, `tests/hash_map.rs`, `tests/os_api.rs`, `tests/string_interpolation.rs`, `tests/integer_intrinsics.mjs`, `tests/features.mjs`, `tests/control.mjs`, `tests/strings.mjs`, `tests/allocator.mjs`, `tests/math.mjs`, `docs/architecture.md`, `_tsuzuri/language-reference/built-in-types-and-modules/int.md`, `_features/README.md` |

## 目的

`Int.min`／`Int.max`／`Int.leading_zeros` を、数値の表示や文字列化と一緒に使えるようにする。
現在は、生成コードが宣言する LLVM intrinsic と、連結したランタイム IR（`src/runtime/numeric.ll`）が宣言する同じ intrinsic が 2 回現れ、
clang が `invalid redefinition of function` で失敗する。`IO.write_line (Int.min a b)` のようなごく普通のプログラムが、native・WASM の
どのターゲットでもビルドできない。接尾辞のない整数リテラルの既定の型は `i32` なので、`Int.min 3 5` を表示するだけのプログラムも該当する。

同時に、宣言の一意性を「行全体」で比べていて今回の重複を見逃した既存の検査を、「シンボル名」で比べる検査に改める。
GUIDE §10 の「intrinsic 宣言が重複しない」を、実際に LLVM が受け付ける条件で確かめられるようにする。

## 着手条件と停止条件

### 着手条件

- D04 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認: `grep -nE "^\| (D04|D12) " _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の再現（失敗すること）を確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `src/runtime/numeric.ll` や `math.ll` を手で編集したくなった。これらは `src/runtime/generate.py`・`generate_math.py` の生成物である
  （`docs/architecture.md`「`numeric.ll` は生成スクリプトから自動生成されるため、手動で直接編集してはなりません」）。
- 同じシンボル名で、戻り値や引数の型が違う `declare` が見つかった（どちらを残しても正しくない）。
- 同じシンボル名の `declare` と `define` が同じモジュールに現れた（このチケットは `declare` 同士の重複だけを直す）。
- 重複の無いプログラムの IR が、修正の前後で byte 単位で変わる（手順 6）。
- 既存テストの期待値を変えないと通らない（「既存テストへの影響」に書いた、一意性検査の方法の変更を除く）。

## 現状（HEAD `5b896b7` で確認）

- 生成コードの宣言: `src/llvm.rs` の `integer_builtin` は、`IntMin`・`IntMax` で `declare {ty} @llvm.{s|u}{min|max}.{ty}({ty}, {ty})`、
  `IntCountOnes`・`IntLeadingZeros`・`IntTrailingZeros` で `declare {ty} @llvm.{ctpop|ctlz|cttz}.{ty}(...)` を `self.intrinsics`
  （`BTreeSet<String>`）に入れる。`emit_program` はこの集合を `for intrinsic in intrinsics { writeln!(output, "{intrinsic}") }` で書き、
  そのあとでランタイム IR の本文を連結する。集合の中では重複しないが、ランタイム IR の宣言とは照合しない。
  `IntAbs`・`IntUnsignedAbs`・`IntClamp`・`IntAbsDiff` は `icmp` と `select` で書き、intrinsic を宣言しない。
- ランタイム IR の連結（同じく `emit_program`）: 出力が `@tz_soft_` を含むと `numeric.ll` を連結する。このとき取り除くのは
  `declare void @llvm.trap()\n` だけである。`@tz_math_` を含むと `math.ll` を連結し、その直後に `declare` 行を名前で一度だけ残す
  filter を全体にかける。したがって `math.ll` を連結したプログラムでは重複が消え、`numeric.ll` だけを連結したプログラムでは残る。
- `numeric.ll` が宣言するもの: `llvm.abs.i32`、`llvm.ctlz.i32`、`llvm.experimental.noalias.scope.decl`、`llvm.lifetime.end.p0`、
  `llvm.lifetime.start.p0`、`llvm.memcpy.p0.p0.i64`、`llvm.memset.p0.i64`、`llvm.scmp.i32.i32`、`llvm.smax.i32`、`llvm.smin.i32`、`llvm.trap`。
  `math.ll` が宣言するもの: `llvm.copysign.f32`／`.f64`、`llvm.fabs.f32`／`.f64`、`llvm.is.fpclass.f64`、`llvm.lifetime.end.p0`、
  `llvm.lifetime.start.p0`、`llvm.smax.i32`、`llvm.smin.i32`、`llvm.sqrt.f32`／`.f64`。確認:
  `grep -o "^declare [^@]*@[A-Za-z0-9_.]*" src/runtime/numeric.ll | sort -u`。
- 生成コードの宣言と `numeric.ll` の宣言が重なるのは `llvm.smin.i32`（`Int.min` の `i32`）、`llvm.smax.i32`（`Int.max` の `i32`）、
  `llvm.ctlz.i32`（`Int.leading_zeros` の `i32` と `i32u`。型名はどちらも `i32`）の 3 つである。`i32u` の `Int.min`／`Int.max` は
  `llvm.umin.i32`／`llvm.umax.i32` なので重ならない。
- `numeric.ll` を連結させる `@tz_soft_` の呼び出しは多い: トップレベルの結果式が数値のときの表示（`console_main` の `@tz_soft_format`）、
  数値の `Display`（`IO.write_line`・`to_string`）、文字列補間の書式指定（`src/llvm_display.rs` の `@tz_soft_format_spec`）、
  数値の解析（`@tz_soft_parse`）、f16／f128／decimal の演算、浮動小数点の canonical hash（`@tz_soft_hash_canonical`）など。
- LLVM は 2 つ目の宣言を、属性グループだけが違っていても受け付けない。生成コードは `declare i32 @llvm.smin.i32(i32, i32)`、
  `numeric.ll` は `declare i32 @llvm.smin.i32(i32, i32) #14` である。
- 既存のテストが見逃した理由:
  - `tests/integers.rs` の `integer_bit_and_selection_intrinsics_are_typed_and_deterministic` は全幅の `Int.*` を
    `Entry::Library` で出すが、数値の表示や文字列化を含まないので `numeric.ll` を連結しない。
  - 宣言の一意性検査はすべて行全体を比べる。`#14` の有無で行が違うと、同じシンボルでも重複と判定されない。該当箇所（12 か所）:
    `tests/stdlib.rs` の `library`、`tests/strings.rs`・`tests/display_parse.rs`・`tests/hash_map.rs`・`tests/os_api.rs`・
    `tests/string_interpolation.rs` の IR 補助関数、`tests/features.mjs`（`declarations are deduplicated`）、`tests/control.mjs`、
    `tests/strings.mjs`、`tests/allocator.mjs`、`tests/math.mjs`、`tests/integer_intrinsics.mjs`。
    確認: `grep -rn 'starts_with("declare ")\|new Set(declarations)' tests` が 12 行を出す。
  - `tests/integer_intrinsics.mjs` は export した関数を C と JavaScript から呼んで参照値と照合するだけで、数値を表示しない。
- 文書: `docs/architecture.md` は「intrinsic の外部宣言は組み込み関数と通常の式で共通化され、重複なく決定論的な順序で出力されます」とし、
  GUIDE §10 も「intrinsic 宣言が重複しない」を完了条件にしている。言語リファレンスの `int.md` には、この不具合を既知の不具合として注記している。

### 再現（検証済み。HEAD `5b896b7`、macOS arm64、Apple clang 21）

トップレベルの結果式（`Entry::Console`）:

```sh
mkdir -p /tmp/tz-d12/script && cd /tmp/tz-d12/script
printf 'let a: i32 = 5\nlet b: i32 = 9\nInt.min a b\n' > Main.tz
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri run .
```

```text
./Main.tz:1:1: error[E2002]: 'clang' failed (exit status: 1):
.../module.ll:13008:13: error: invalid redefinition of function 'llvm.smin.i32'
 13008 | declare i32 @llvm.smin.i32(i32, i32) #14
```

ライブラリ（`Entry::Library`。`--emit llvm` の出力そのものが不正な IR になる）:

```sh
mkdir -p /tmp/tz-d12/library && cd /tmp/tz-d12/library
printf 'export def digits :: i32 -> i64 = \\x ->\n    (to_string (Int.min x 3)).length\n' > Lib.tz
/Users/tmidorikawa/Documents/git/Tsuzuri/target/release/tsuzuri build Lib.tz --emit llvm -o lib.ll
grep -n '^declare i32 @llvm.smin.i32' lib.ll
```

```text
108:declare i32 @llvm.smin.i32(i32, i32)
12909:declare i32 @llvm.smin.i32(i32, i32) #14
```

同じ `Lib.tz` は `--target wasm32`、`--target wasm64`、`--emit object` のどれでも `E2002`（`invalid redefinition of function 'llvm.smin.i32'`）になる。

組み合わせごとの結果（`tsuzuri build Main.tz -o app --no-cache`）:

| プログラム | 結果 |
| --- | --- |
| `def main` で `Int.min a 9`（`i32`）を計算し、表示しない | 成功 |
| トップレベルの `Int.min a b`（`i32`） | `E2002`（`llvm.smin.i32`） |
| トップレベルの `Int.min 3 5`（接尾辞なし。既定の `i32`） | `E2002`（`llvm.smin.i32`） |
| トップレベルで `let c = Int.min a b` のあと `0` を返す | `E2002`（`0` の表示が `numeric.ll` を連結する） |
| `def main` で `do! IO.write_line (Int.min a 9)` | `E2002` |
| `def main` で `$"{m:>4}"`（`m = Int.min a 9`） | `E2002` |
| トップレベルの `Int.max a 7`（`i32`） | `E2002`（`llvm.smax.i32`） |
| トップレベルの `Int.leading_zeros a`（`i32u`） | `E2002`（`llvm.ctlz.i32`） |
| トップレベルの `Int.max a 7u`（`i32u`） | 成功（`llvm.umax.i32`） |
| トップレベルの `Int.min`（`i16`・`i64`） | 成功 |
| トップレベルの `Int.abs`・`Int.unsigned_abs`・`abs`・`Int.clamp`（`i32`） | 成功（`select` で書くため） |

## 仕様

言語の意味・構文・診断は変えない。生成するモジュールについて次を保証する。

- 1 つのモジュールの中で、同じシンボル名の `declare` は高々 1 つである。生成コードとランタイム IR が同じ関数を宣言するときは、先に現れた
  宣言だけを残す（生成コードの宣言はランタイム IR の本文より前に書かれるので、生成コードの宣言が残る）。
- 重複が無いモジュールの IR は、修正の前後で byte 単位で同じである。
- native・wasm32・wasm64、`--emit exe|object|llvm|wasm` のすべてに適用する。`--emit llvm` の出力も、そのまま LLVM に渡せる。
- intrinsic の属性は LLVM が名前から与えるので、属性グループ付きの宣言を落としても意味は変わらない。参照されなくなった
  `attributes #N = { ... }` はそのまま残してよい（LLVM は未使用の属性グループを受け付ける）。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/llvm.rs` | `unique_declarations`（新規、private） | `declare` 行をシンボル名で一度だけ残す。重複が無ければ入力の `String` をそのまま返す |
| `src/llvm.rs` | `duplicate_declarations`（新規、`pub`） | 2 回以上宣言されたシンボル名を、最初に現れた順の `Vec<String>` で返す。テストと `debug_assert!` が使う |
| `src/llvm.rs` | `emit_program` の末尾 | `Ok((simd::with_vector_alignment(output), globals.traps))` の直前で `output = unique_declarations(output)` を呼ぶ |
| `src/llvm.rs` | `emit_program` の `numeric.ll`・`math.ll` の分岐 | 変えない（D1）。既存の `llvm.trap` の除去と `math.ll` 後の filter はそのまま残す |
| `src/driver.rs` | IR を書き出す直前（`Emit::Llvm`・`Executable`・`Object`・`Wasm` の分岐の前） | `debug_assert!(llvm::duplicate_declarations(&text).is_empty(), ...)` を足す（D4） |
| `tests/*.rs`・`tests/*.mjs` | 一意性の検査 | 行全体ではなくシンボル名で比べる（D3） |

### アルゴリズム

`unique_declarations(ir: String) -> String`:

1. 1 回目の走査で、`declare ` で始まる行からシンボル名を取り出し、`BTreeSet<&str>` に入れる。すでにある名前に出会ったら「重複あり」とする。
   シンボル名は、行の最初の `@` の次から最初の `(` の直前までとする。`@"...` のように引用符で始まる名前は対象外として残す（ランタイム IR と
   生成コードは引用符付きの名前を使わない。見つけたら停止条件）。
2. 重複が無ければ `ir` をそのまま返す（byte 単位で同一。改行の正規化もしない）。
3. 重複があれば 2 回目の走査で、各 `declare` 行について名前が初出なら残し、既出なら捨てる。`declare` 以外の行は順序も内容も変えない。
   行の区切りは `split_inclusive('\n')` で保ち、末尾の改行の有無も入力どおりにする。

`duplicate_declarations(ir: &str) -> Vec<String>` は同じ取り出し規則で、2 回目以降に現れた名前を初出順に重複なく返す。
`declare` と `define` の衝突は数えない（停止条件の確認は手順 2 のテストで行う）。

計算量は行数に対して $O(n \log n)$ で、1 万行規模のモジュールでも無視できる。生成する IR の順序は入力の順序だけで決まるので決定的である。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインと再現

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」の 2 つを実行して失敗を確かめる。重複の無いプログラムの IR を比較用に保存する。
- 確認:

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
cargo test --locked
mkdir -p /tmp/tz-d12/base
for fixture in tests/fixtures/constants tests/fixtures/string_interpolation; do
  name=$(basename "$fixture")
  target/release/tsuzuri build "$fixture" --emit llvm -o "/tmp/tz-d12/base/$name.ll"
  target/release/tsuzuri build "$fixture" --target wasm32 --emit llvm -o "/tmp/tz-d12/base/$name-wasm.ll"
done
```

`/tmp/tz-d12/script` の `run` と `/tmp/tz-d12/library` の `grep` が「再現」のとおりになる。ならなければ停止する。

### 手順 2: 失敗するテストを先に足す

- 変更: `src/llvm.rs`（`duplicate_declarations` だけを先に足す）、`tests/integers.rs`。
- 内容: `integer_intrinsics_beside_number_formatting_declare_each_symbol_once`（新規）を足す。この時点では失敗する。
- 確認: `cargo test --locked --test integers integer_intrinsics_beside_number_formatting` が `1 failed`（`llvm.smin.i32` などを報告）。

### 手順 3: `unique_declarations` を入れる

- 変更: `src/llvm.rs`（`unique_declarations`、`emit_program` の末尾、`mod tests` の単体テスト）。
- 内容: 「アルゴリズム」どおりに実装し、単体テスト `unique_declarations_keep_the_first_and_leave_clean_ir_unchanged`（新規）を足す。
- 確認: `cargo test --locked --test integers` が全件成功する。`cargo test --locked --lib unique_declarations` が `1 passed`。

### 手順 4: 一意性の検査をシンボル名に改める

- 変更: `tests/stdlib.rs`・`tests/strings.rs`・`tests/display_parse.rs`・`tests/hash_map.rs`・`tests/os_api.rs`・`tests/string_interpolation.rs`
  の IR 補助関数、`tests/features.mjs`・`tests/control.mjs`・`tests/strings.mjs`・`tests/allocator.mjs`・`tests/math.mjs`・
  `tests/integer_intrinsics.mjs` の検査（「現状」の 12 か所）。
- 内容: Rust は `assert!(llvm::duplicate_declarations(&ir).is_empty(), "{ir}")`、JavaScript は
  `const names = declarations.map(line => line.match(/@([^\s(]+)\(/)[1]); assert.equal(new Set(names).size, names.length, ...)`。
  既存のメッセージ文字列は変えない。
- 確認: `cargo test --locked` と、6 つの JavaScript の検査（`node tests/<名前>.mjs target/release/tsuzuri`）が成功する。
  `grep -rn 'new Set(declarations)' tests` が 0 行になる。この段で新たに失敗するテストがあれば、それは別の重複であり停止条件（報告する）。

### 手順 5: `debug_assert!` を driver に足す

- 変更: `src/driver.rs`。
- 内容: IR を一時ファイルや出力へ書く直前に `debug_assert!`（D4）。release build の動作は変わらない。
- 確認: `cargo test --locked` が成功する（debug build のテストが driver を通る）。

### 手順 6: 重複の無いプログラムの IR が変わらないこと

- 変更: なし。
- 内容: 手順 1 で保存した IR と、修正後の IR を比べる。
- 確認: 次がすべて差分なしで終わる。

```sh
cargo build --release --locked
for fixture in tests/fixtures/constants tests/fixtures/string_interpolation; do
  name=$(basename "$fixture")
  target/release/tsuzuri build "$fixture" --emit llvm -o "/tmp/tz-d12/$name.ll"
  target/release/tsuzuri build "$fixture" --target wasm32 --emit llvm -o "/tmp/tz-d12/$name-wasm.ll"
  cmp "/tmp/tz-d12/base/$name.ll" "/tmp/tz-d12/$name.ll"
  cmp "/tmp/tz-d12/base/$name-wasm.ll" "/tmp/tz-d12/$name-wasm.ll"
done
```

### 手順 7: E2E

- 変更: `tests/integer_intrinsics.mjs`。
- 内容: 「E2E」の 2 つのブロックを足す。
- 確認: `node tests/integer_intrinsics.mjs target/release/tsuzuri` が成功する。「再現」の 2 つが成功し、`run` は `5` を出す。

### 手順 8: ドキュメントと最終確認

- 変更: `docs/architecture.md`、`_tsuzuri/language-reference/built-in-types-and-modules/int.md`、`_features/README.md`。
- 内容: 「ドキュメント」のとおり。
- 確認: GUIDE §3 の全体（`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked`）、
  `node tests/features.mjs target/release/tsuzuri`、`node tests/strings.mjs target/release/tsuzuri`、
  `node tests/control.mjs target/release/tsuzuri`、`node tests/allocator.mjs target/release/tsuzuri`、
  `node tests/math.mjs target/release/tsuzuri`、`node tests/integer_intrinsics.mjs target/release/tsuzuri`、
  `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/int.md`。

## テスト計画

### Rust テスト

| テスト | 内容 |
| --- | --- |
| `tests/integers.rs` の `integer_intrinsics_beside_number_formatting_declare_each_symbol_once`（新規） | 幅 8・16・32・64・128 × 符号の有無 × `integer_bit_and_selection_intrinsics_are_typed_and_deterministic` と同じ 11 の式について、(a) `def run :: {ty} -> {ty} -> string` / `fn run left right = to_string (({式}) as i64)` を `Entry::Library` の native と WASM で、(b) 同じ `run` をトップレベルの結果式で呼ぶプログラムを `Entry::Console` で出す。どれも `@tz_soft_format` を含むこと（`numeric.ll` を連結した条件で検査していること）と、`llvm::duplicate_declarations(&ir)` が空であることを確かめる。2 回出して IR が一致することも確かめる |
| `src/llvm.rs` の `unique_declarations_keep_the_first_and_leave_clean_ir_unchanged`（新規） | 重複の無い入力は `==` で同一（末尾改行なしの入力も含む）。`declare i32 @f(i32)` と `declare i32 @f(i32) #3` の 2 行は前者だけが残る。`define` 行・コメント行・空行・行の順序は変わらない |
| 既存の IR 補助関数（手順 4 の Rust の 6 ファイル） | 行全体ではなく `llvm::duplicate_declarations` で検査する |
| 既存の JavaScript の検査（手順 4 の 6 ファイル） | 行全体ではなく、`declare` 行から取り出したシンボル名の集合で検査する |

### E2E

`tests/integer_intrinsics.mjs` に 2 つのブロックを足す。期待値は既存の `reference` 関数（JavaScript の BigInt）で作り、コンパイラ出力から写さない。

| ブロック | 内容 |
| --- | --- |
| 表示するプログラム | `min`・`max`・`leading_zeros` × `i32`・`i32u`（ほかの幅も同じ表で回してよい）ごとに、`def main :: unit -> i32` で `do! IO.write_line (式)` と `do! IO.write_line ($"{式:>4}")` を出すプロジェクトを作り、`tsuzuri run <dir> -O0` と `-O3` の標準出力を参照値の行と照合する |
| 文字列化する WASM ライブラリ | 「再現」の `digits` と同じ形の export を、`min`・`max`・`leading_zeros` × `i32`・`i32u` で作り、`--target wasm32` でビルドして import なしでインスタンス化し、`to_string` の長さを参照値の桁数と照合する |

### 既存テストへの影響

一意性の検査の方法を、行全体からシンボル名に変える（手順 4）。期待値やメッセージは変えない。それ以外の期待値は変えない。

### 性能

なし。追加の走査は 1 モジュールにつき 1〜2 回の行走査で、計測の対象にしない。

## ドキュメント

GUIDE §8 の手順で、同じ PR で更新する。

- `docs/architecture.md` の「intrinsic の外部宣言は組み込み関数と通常の式で共通化され、重複なく決定論的な順序で出力されます」に、
  連結したランタイム IR（`numeric.ll`・`math.ll` など）の宣言も、シンボル名ごとに最初の 1 つだけを残すことを足す。同じ文書の「開発と検証」の
  「`Math` モジュールを使用する場合にのみ `numeric.ll` の後に `math.ll` が結合され、重複する LLVM intrinsic 宣言が安全に除去されます」も、
  宣言の重複の除去が `math.ll` の有無によらないことに直す。
- `_tsuzuri/language-reference/built-in-types-and-modules/int.md` の「既知の不具合（LLVM の intrinsic 宣言の重複）」の注記を削除する。
  ほかに不具合の注記や D12 へのリンクが残っていないことを `grep -rnE "D12-intrinsic|intrinsic 宣言の重複" _tsuzuri/language-reference` で
  確かめる（HEAD `7d86f66` で該当は `int.md` だけ）。
- `_features/README.md` の D12 の状態（GUIDE §10）。

## 受け入れ条件

- [x] 「再現」の 2 つと、組み合わせの表で `E2002` だった行が、native・wasm32・wasm64・`--emit object` で成功する。
- [x] 全幅の `Int.*` を数値の表示・文字列化と組み合わせても、`llvm::duplicate_declarations` が空である（Rust テスト）。
- [x] 一意性の検査がシンボル名で行われる（手順 4 の 12 か所。実装時は 20 か所。「実装と検証」の「現状との差」）。
- [x] 重複の無いプログラムの IR が byte 単位で変わらない（手順 6）。
- [x] `int.md` の既知の不具合の注記を削除し、`node scripts/check-docs.mjs` が成功する。
- [x] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `--emit llvm` と `--emit object` は `Entry::Library`、実行ファイルは `Entry::Console` で IR を作る。トップレベルの結果式の表示は
  `Entry::Console` にしか無いので、`--emit llvm` だけで再現を探すと見落とす。数値の文字列化を含むライブラリで再現する。
- 行全体の比較では、属性グループ（`#14`）の有無で重複を見逃す。必ずシンボル名で比べる。
- `math.ll` を連結したプログラムでは既存の filter が重複を消すので、`Math.*` を混ぜたテストでは再現しない。
- `numeric.ll`・`math.ll` は生成物で、手で直さない。宣言を消すために生成スクリプトを変えるのも、このチケットの範囲外である。
- `lines()` と `join("\n")` で組み直すと、末尾改行や行の内容が変わり、重複の無いプログラムの IR まで変わる。重複が無ければ入力をそのまま返す。
- `tsuzuri run` はビルドキャッシュを使う。キャッシュの鍵はコンパイラを含むので、再ビルドしたコンパイラでは古い成果物は使われない。
  それでも疑わしいときは `--no-cache` を付ける。

## 対象外

- ランタイム IR を別オブジェクトにしてリンク時に結合する構成への変更。
- `numeric.ll`・`math.ll` の宣言や生成スクリプトの変更。
- `declare` と `define` の衝突の自動解決（見つけたら停止条件）。
- `Int.abs` などを intrinsic に変えるといった、lowering の変更。

## 決定事項

### D1: 既存の 2 つの特別扱いは残す

- 決定: `numeric.ll` 連結前の `declare void @llvm.trap()\n` の除去と、`math.ll` 連結後の filter は変えず、最後に `unique_declarations` を足す。
- 理由: 現在ビルドできているプログラムの IR を byte 単位で変えないため。特別扱いの整理は、IR の差分を受け入れる別の変更で行う。
- 状態: 既定案（実装者はこの案に従う）

### D2: 先に現れた宣言を残す

- 決定: 同じシンボル名の `declare` は、モジュールの中で最初に現れたものを残す。
- 理由: 生成コードの宣言が常にランタイム IR の本文より前にあり、結果が決定的になる。intrinsic の属性は名前から決まるので、
  ランタイム IR 側の属性グループを落としても意味は変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D3: 一意性はシンボル名で検査する

- 決定: テストの一意性検査は `llvm::duplicate_declarations`（JavaScript では `@` から `(` までの名前）で行い、行全体では比べない。
- 理由: LLVM が拒否するのは同じ名前の 2 回目の宣言で、属性や引数属性の違いは区別されない。
- 状態: 既定案（実装者はこの案に従う）

### D4: 新しい診断は作らない

- 決定: 重複はコンパイラの不具合として扱い、利用者向けの診断コードは足さない。debug build だけ `debug_assert!` で検出する。
- 理由: 利用者のプログラムでは避けられない誤りで、報告されても利用者は直せない。release build の動作と性能を変えない。
- 状態: 既定案（実装者はこの案に従う）。実装済み。

D1〜D3 も既定案のとおり実装した（2026-10-12）。

## 実装と検証（2026-10-12）

実装の基準は `aecdf75e`（起票時の `5b896b7` から 242 コミット後）。手順 1〜8 をすべて行い、停止条件には当たらなかった。実装しなかったものは、対象外に書いたものだけである。

### 現状との差（GUIDE §0 手順 2）

- 一意性を行全体で比べる検査は、起票時の 12 か所から **20 か所**（Rust 13、JavaScript 7）に増えていた。増えたのは `tests/matrix.rs`・`tensor.rs`・`matrix_views.rs`（C11）、`net_api.rs`（E09）、`unicode.rs`・`regex.rs`（D09）、`f16_hardware.rs`・`f16_hardware.mjs`（D10）。意図（行全体でなくシンボル名で比べる）は変わらないので、20 か所すべてを改めた。`grep -rn 'new Set(declarations)' tests` は 0 行になり、`starts_with("declare ")` で残る 3 行（`net_api.rs` の `@tsuzuri_net_` の数え上げ、`unicode.rs`・`regex.rs` の「宣言が `llvm.` か C ライブラリの 3 つだけ」）は一意性の検査ではないので変えていない。
- `emit_program` は起票時より多くのランタイム（task・sync・channel・debug・display・format・unicode・各 heap・recursive など）を連結し、`emit_native_build`・`emit_wasm_build`・`traps::instrument`、driver の `windows_abi`・`with_wasm_heap_limit`・`with_wasi_host`・`with_net_imports`・`with_wasm_gpu_host` が、その後で IR を作り直す。`unique_declarations` は設計どおり `emit_program` の最後に置き、後段が足す宣言は driver の `debug_assert!` が debug build で検出する。この変更のテストと E2E に、後段が重複を足す例は無かった。
- `numeric.ll` が宣言する `llvm.smin.i32`・`llvm.smax.i32`・`llvm.ctlz.i32` の 3 つだけが、生成コードの宣言と重なる（起票時と同じ）。落とした宣言と残した宣言は、属性グループと引数属性が違うだけで型は同じである（`llvm.ctlz.i32` は生成コードが `(i32, i1)`、ランタイムが `(i32, i1 immarg) #13`）。

### 実装したファイル

- `src/llvm.rs`: `declared_symbol`・`repeats_a_declaration`・`unique_declarations`（private）、`duplicate_declarations`（`pub`）、`emit_program` の最後の呼び出し、単体テスト 2 件。
- `src/driver.rs`: IR を後段の変換まで済ませた直後の `debug_assert!`（`--emit header` と GPU kernel の出力は IR でないので除く）。
- Rust テスト: `tests/integers.rs`（新規 1 件と、11 の式を `SELECTION_EXPRESSIONS` に切り出し）、`stdlib.rs`・`strings.rs`・`display_parse.rs`・`hash_map.rs`・`os_api.rs`・`string_interpolation.rs`・`matrix.rs`・`tensor.rs`・`matrix_views.rs`・`net_api.rs`・`unicode.rs`・`regex.rs`・`f16_hardware.rs`（シンボル名の検査へ。不要になった `BTreeSet` の import も削除）。
- JavaScript: `tests/integer_intrinsics.mjs`（E2E の 2 ブロック）、`features.mjs`・`control.mjs`・`strings.mjs`・`allocator.mjs`・`math.mjs`・`f16_hardware.mjs`（シンボル名の検査へ。変数名は、`allocator.mjs`・`f16_hardware.mjs` に同名の `symbols` が既にあるので `declaredNames`）。
- 文書: `docs/architecture.md`（2 か所と整数 intrinsic の段落）、`_tsuzuri/language-reference/built-in-types-and-modules/int.md`、`_features/README.md`、このチケット（`_completed/` へ移動）。

### 検証結果

- 基準（変更前）: `cargo build --release --locked` と `RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast` が 97 バイナリ・1,196 件成功。`RUST_MIN_STACK` を付けない既定の 2 MiB では、既存の問題で `tests/computations.rs::bounds_computation_syntax_and_expansion` が stack overflow で止まる（G17・G18 の記録と同じ。この変更とは無関係）。「再現」の 2 つ（トップレベルの `Int.min a b` と `digits` のライブラリ）は記載どおり `E2002`・重複 2 行になった。
- 手順 2（修正前）: 新しい Rust テストは `["llvm.ctlz.i32", "llvm.smax.i32", "llvm.smin.i32"]` を報告して失敗した（予想した 3 シンボルと一致）。手順 3 の後は `--test integers` が 4 件成功、`--lib unique_declarations` が 1 件成功。
- 最終: `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`sh scripts/check-runtime-includes.sh`（43 ファイル）が成功。`RUST_MIN_STACK=4194304 cargo test --locked --no-fail-fast` は 97 バイナリで **1,199 件成功**（基準の 1,196 件 + 新しい 3 件）、失敗 0、無視 0。GUIDE §3.1 の 4 つの回帰テストは各 1 件（`running 1 test`）で成功。
- 手順 4・5: 変更した Rust の 13 ファイルは全件成功（`display_parse` 6、`f16_hardware` 15、`hash_map` 5、`matrix` 8、`matrix_views` 7、`net_api` 18、`os_api` 10、`regex` 6、`stdlib` 7、`string_interpolation` 11、`strings` 13、`tensor` 4、`unicode` 4）。debug build の driver の `debug_assert!` は、全テストが通る間に一度も発火しなかった。
- E2E（release を `cargo build --release --locked` した直後）: `control`（270 ケース）、`strings`（O0・O3 各 3,973 ケース）、`allocator`、`math`（786,051 参照）、`f16_hardware`（2,707,485 ケース）、`features`（**13,719 ケース**）、`integer_intrinsics` が成功（native と WASM の `-O0`・`-O3`、`live == 0`、WASM の import なし）。
- `tests/integer_intrinsics.mjs` の新しい 2 ブロック: 全幅 10 型の `Int.min`・`Int.max`・`Int.leading_zeros` を `IO.write_line` と `$"{…:>4}"` で表示するプログラムを `tsuzuri run` の `-O0`・`-O3` で実行し、BigInt の参照値と **960 行**が一致した。`i32`・`i32u` の `export def` が `(to_string (Int.min x 3)).length` などを返す WASM ライブラリを `-O0`・`-O3` で作り、import なしでインスタンス化して、BigInt の参照値と **3,786 ケース**が一致した。変更前のコンパイラでは、`shown-i32` が `invalid redefinition of function 'llvm.ctlz.i32'` で失敗する。
- 「組み合わせごとの結果」の表（14 のプログラムと `digits` のライブラリ）を、native の実行ファイル・`--emit object`・`--emit llvm`・wasm32・wasm64 の object と、`def main` を持つ 2 つの `--target wasm32`・`wasm64` で作り直した。変更前は表のとおり 8 つのプログラムとライブラリが失敗し（`Int.max a 7u`、`i16`・`i64`、`Int.abs`、`Int.unsigned_abs`、`Int.clamp` は成功）、変更後はすべて成功した。
- IR の一致（ベースラインは `aecdf75e` の release を別の worktree でビルドしたもの）: 「手順 6」の 2 fixture × native・wasm32 の 4 つは `cmp` が差分なし。`examples/` と `tests/fixtures/` の 104 ディレクトリ × native・wasm32 × `-O0`・`-O3` の 416 構成は、**366 構成の IR が byte 単位で一致**し、50 構成（トップレベルに `Main.tz` を持たない 10 ディレクトリの 40 構成と、WASM で使えない API を使う 4 つの fixture の wasm32 の 10 構成）は両者が同じ診断で失敗し、差は 0。
- 文書: `node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/int.md` が成功（12 リンク、5 例、native 10 回）。新しい例（`Int.min`・`Int.max`・`Int.leading_zeros` を文字列補間で表示して `3:9:28`）は、変更前のコンパイラでは `E2002` になる。
- 性能: 主張しない。追加の走査は 1 モジュールにつき 1 回（重複があるときだけ 2 回）の行走査で、参考として debug build の `duplicate_declarations` は約 56 万 byte の IR で約 1.5 ms だった。

### チケットから外れた判断

- 新しい Rust テストは、式ごとに 1 モジュールではなく、**型ごとに 11 式を 1 モジュールにまとめた**。型検査（`analyze` が 1 回約 220 ms）が支配的で、式ごとの版は debug build で 93 秒かかったため（まとめた版は約 25 秒）。モジュールの宣言は 11 式の宣言の和集合なので、重複の検出力は変わらず、失敗時は 3 シンボルをまとめて報告する（修正前の失敗は上のとおり）。Console の入口は `run` を呼ぶ形ではなく、トップレベルの結果式を数値のまま表示する形（`console_main` の `@tz_soft_format`）で検査した。
- `int.md` は、注記の削除に加えて、`Int.min`・`Int.max`・`Int.leading_zeros` を文字列補間で表示する実行例を足した（AGENTS.md の「既知の不具合の警告を実際の振る舞いの説明に置き換える」）。
- 一意性の検査を改める対象は 12 か所でなく 20 か所とした（上記）。

### 見つけた問題・残作業

- 既定の 2 MiB の stack で `bounds_computation_syntax_and_expansion` が止まる既存の問題は、この変更の範囲外のまま（全体の実行には `RUST_MIN_STACK=4194304` を付ける）。
- 属性が違うだけでなく**型が違う**同名の `declare` は、今の生成コードとランタイム IR には無い。将来それが現れると `unique_declarations` は先の宣言を残して後の宣言を黙って落とすので、`debug_assert!` では検出できない。型の比較までは、このチケットの対象外とした（停止条件は、手順 6 の IR の比較と今回の確認で見つからなかった）。
