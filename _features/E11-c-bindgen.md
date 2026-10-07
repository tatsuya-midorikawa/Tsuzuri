# E11: C ヘッダーからの extern 生成

| 項目 | 内容 |
| --- | --- |
| ID | E11 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E12 |
| 後続 | – |
| 状態 | done（Phase 1 と Phase 2） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（サブコマンド `bindgen` と警告 `W2002` の確定。GUIDE D-30 の仮割り当て） |
| 改善する劣位 | C/C++ 比: 既存の C/C++ コードをそのまま取り込めない（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 手本にする既存実装 | サブコマンドと出力保護: `src/main.rs` の `Action::Doc`・`parse_arguments` の `Some("doc")` 分岐、`src/driver.rs` の `document`・`protect_documentation`・`protect_source`。外部ツールの起動: `src/driver.rs` の `tool("TSUZURI_CLANG", "clang")`・`run_tool`・`driver_error`。ハッシュ: `src/cache.rs` の `Sha256`（`new`・`update`・`hex`）。ABI の逆写像の基準: `tsuzuri build <src> --emit header` の出力（`src/llvm_imports.rs`、`src/abi.rs` の `scalar_record`）。C とリンクする E2E: `tests/host_imports.mjs` |
| 主な影響ファイル | `src/main.rs`, `src/lib.rs`, `src/driver.rs`, `src/bindgen.rs`（新規）, `tests/bindgen.rs`（新規）, `tests/bindgen.mjs`（新規）, `tests/fixtures/bindgen/`（新規: `basic/`・`skipped/`・`include/`・`names/`）, `docs/language.md`, `docs/architecture.md`, `README.md`, `_docs/tools/command-line.md`, `_docs/guides/native-interop.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

既存の C ライブラリのヘッダーから、E12 のリンク名付き `extern def`・定数・スカラーレコード・型別名を生成し、手書きの宣言と shim を減らす。
Rust の bindgen、C# の ClangSharp／P/Invoke 生成、Zig の `@cImport` に相当する。

変換は「ABI が一致すると証明できる形だけを生成し、それ以外は理由付きで報告する」。推測で生成した宣言が C 側と食い違うと、
型検査を通ったまま実行時に値が壊れるため、迷う形はすべて `W2002` で省く。

実装者は Phase 1 だけを実装する。Phase 2（「対象外」の先送り項目）は人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- E12 Phase 1 の「リンク名」（`extern "symbol" def name :: ...`）が `_features/README.md` の状態欄で done であること。
  確認: `grep -n "^| E12 " _features/README.md`。E12 のほかの項目（`--link`・`extern type`・コールバック）は前提にしない。
- D1 が承認済みであること。承認前はどの手順にも着手しない。
- GUIDE §2.3 の基準コマンドが成功すること。
- `clang --version`（または `TSUZURI_CLANG`）が LLVM 17 以上を示すこと。build と同じ要件で、追加のツールは要らない。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- E12 で確定したリンク名の構文が「前提とする他チケットのインターフェース」と違う。生成テキストを合わせる箇所は `render_function`（新規）だけなので、
  差分を示して指示を待つ。
- 生成した `extern "..." def` が E12 の検査（予約接頭辞、同じシンボルの型違い）で拒否され、その規則が本チケットの名前規則（D6）と矛盾する。
- clang の JSON に「設計 / アルゴリズム」が使うフィールド（`kind`・`name`・`loc`・`range`・`inner`・`type.qualType`・`isImplicit`・`storageClass`・
  `inline`・`variadic`・`previousDecl`・`tagUsed`・`completeDefinition`・`isBitfield`・`value`）が無い版に当たった。
- `serde_json` の feature 追加（`preserve_order`・`unbounded_depth`）、新しい crate（`serde` の derive、`clang-sys`、`bindgen` など）、`unsafe` が必要になった。
- 生成テキストが `tsuzuri fmt --check` を通らず、formatter の変更なしには直せない。
- round-trip E2E で C から直接呼んだ値と Tsuzuri 経由の値が違う。対応表（D3）を変えて回避しない。
- 既存テストの期待値（IR・診断コード・メッセージ・`--help` 以外の CLI 出力）を変える必要がある。
- LLP64（Windows）や 32-bit target への対応を求められた（D8。G10 の後に別途扱う）。

## 現状（HEAD `f8dc655` で確認）

- CLI: `src/main.rs` の `enum Action` は `Lsp`・`Check`・`Doc`・`Build`・`Run`・`Fmt`・`Test` だけ。`parse_arguments` は未知の先頭語を
  `_ => Action::Build` とし、`tsuzuri bindgen x.h` は `bindgen` を入力パスとして扱う。`HELP` 定数に `bindgen` はない。
- extern: `extern def now :: unit -> i64` の native 名は `tsuzuri_host_<module>_<name>` に固定（docs/language.md「ホスト関数のインポート」）。
  既存の C 関数は shim なしでは呼べず、リンク名は E12 で入る。
- ABI（`tsuzuri build q/Lib.tz --emit header` で確認した C prototype）:
  - `i32 -> i32 -> i32` は `int32_t f(int32_t, int32_t)`、`unit -> unit` は `void f(void)`、`i8u -> i16 -> bool -> f32` は
    `float f(uint32_t, int32_t, int32_t)`、`i16u -> i32u -> i64u -> i64u` は `uint64_t f(uint32_t, uint32_t, uint64_t)`。
  - `ref Point -> f64` は `double f(const tz_record_... *)`。レコードの値引数も const ポインターで渡り、レコード結果は
    `void f(tz_record_... *out, ...)` になる。C の構造体の値渡し・値返しとは ABI が違う。
  - レコードの一時 struct はフィールドを正規化する。`record N { a: i8, b: bool, c: i32 }` は `int32_t a; int32_t b; int32_t c;` になる。
    32/64-bit の整数と `f32`/`f64` のフィールドだけが C の自然な配置と一致する。
  - 狭い整数の結果は `src/llvm_imports.rs` で `trunc i32 ... to iN`、bool の結果は `icmp ne i32 ..., 0` で受ける。
- 名前（`target/release/tsuzuri check` で確認）: `extern def Foo`・`extern def type_`・`extern def _hidden`・`const red: i32`・
  `const RED: i32`・`record point { x: f64 }`・`record R { type_: i32 }` は通る。`extern def sqrt`・`extern def abs` は
  `E1001 duplicate or reserved name`（`src/check.rs` の `duplicate`）。`type cint = i32` は
  `E1024 a type alias name must start with an uppercase ASCII letter`。`const A: i32 = -2147483648`・`const C: i32u = 4294967295`・
  `const MIN64: i64 = -9223372036854775808` は通る。
- 生成物の形: 先頭の `//` コメント 2 行・空行・`const`・`record ... { a: i8, b: bool, c: i32 }`・`extern def` の行は `tsuzuri fmt --check` で
  変更されない（本体の `n (&v)` だけが `n (& v)` に整形された）。
- 依存: `Cargo.toml` の `[dependencies]` は `rustc_apfloat = "0.2"` と `serde_json = "1.0"`（feature 指定なし）。したがって
  `serde_json::Value` のオブジェクトはキー順を保存しない（`BTreeMap`）。既定の再帰上限は 128。
- ハッシュ: `src/cache.rs` の `pub struct Sha256` に `new`・`update`・`hex` があり、`pub mod cache` で公開済み。
- 診断: `W2002` は GUIDE の仮割り当て表で「bindgen で変換できない C 宣言の省略」（E11）。`src/diagnostic.rs` に
  `Diagnostic::warning`・`render_with_severity`・`json` がある。

### 再現（2026-09-29 に検証済み。Apple clang 21.0.0・Homebrew clang 21.1.8、`clang -dumpmachine` は `arm64-apple-darwin27.0.0`）

```sh
mkdir -p /tmp/tz-work-E11/h && cd /tmp/tz-work-E11/h
printf '#define LIMIT 42\ntypedef int cint;\nenum color { RED, GREEN = 5, BLUE };\nstruct point { double x; double y; };\nint add(int a, int b);\nvoid logf2(const char *fmt, ...);\nstatic inline int twice(int v) { return v * 2; }\ndouble dist(struct point p);\n' > s.h
clang -x c -Xclang -ast-dump=json -fsyntax-only s.h > s.json
grep -c '"file"' s.json
```

観察した事実（実装の前提）:

- `"file"` は 1 回だけ出る。clang は位置の `file` を「直前に出した位置とファイルが変わったとき」だけ書く。宣言ごとの所属ファイルは出力順に追跡しないと分からない。
- `#define LIMIT 42` は AST に現れない。`logf2` は `"variadic": true`、`twice` は `"storageClass": "static"` と `"inline": true` を持つ。
- `RED`・`BLUE` の `EnumConstantDecl` は `inner` を持たない（C の規則で 0 と前の値 + 1）。`GREEN = 5` は `inner[0]` が `"kind": "ConstantExpr"` で `"value": "5"`（文字列）。
- 引数の型は文字列 `type.qualType`（例 `"const pt_t *"`・`"enum color"`・`"const int"`・`"char"`）。ポインターの先の typedef は展開されない。
  配列引数 `int arr[4]` は `"int *"` になる。
- `#include <stdio.h>`・`<math.h>`・`<stdlib.h>`・`<string.h>` を含むヘッダーの JSON は 4,417,769 bytes、入れ子の深さは 29。

## 仕様

### 前提とする他チケットのインターフェース

- E12 Phase 1 のリンク名: `extern "symbol" def name :: T1 -> ... -> R`。native では `symbol` をそのままシンボル名にし（Mach-O の `_` は
  リンカーが付ける）、Tsuzuri 側の名前 `name` とは独立。引数・結果の ABI は E05/E06 のまま（「現状」の prototype）。
  E12 は `tz_`・`tsuzuri_` で始まるシンボルと、同じシンボルへの型の違う宣言を拒否する。
- E12 の `--link`・`extern type`・コールバックは使わない。E2E は `--emit object` と clang でリンクする（`tests/host_imports.mjs` と同じ）。

### CLI

```text
tsuzuri bindgen HEADER -o OUTPUT.tz [--include-dir DIR]... [--json]
```

- `HEADER` はちょうど一つ。通常ファイルでなければ `E2001`。拡張子は問わない。`--` の後は既存の規則どおりパスとして読む。
- `-o`/`--output` は必須で、拡張子は `.tz`。モジュール名は既存の規則どおり出力のファイル名で決まるので `--module` は設けない（D2）。
- `--include-dir DIR` は複数指定でき、指定順に clang の `-I DIR` になる。任意の clang 引数は渡せない（D7）。
- `--json` は診断を JSON で出す。そのほかのオプション（`--target`・`-O*`・`--emit`・`--cpu`・`-g`・`--no-cache`・`--deny-warnings`・`--check`・
  `--filter`・`--list`・`--index` など）は `E2000`。
- clang の起動は次の 3 回だけ（`tool("TSUZURI_CLANG", "clang")`）。`HEADER` は `fs::canonicalize` した絶対パスで渡し、その文字列を所属判定に使う。

```text
<clang> --version                       # 1 行目を先頭コメントへ
<clang> -dumpmachine                    # LP64 の確認（D8）と先頭コメント
<clang> -x c -std=gnu17 -fsyntax-only -Xclang -ast-dump=json [-I DIR]... <HEADER の絶対パス>
```

- 成功時は出力を書き、`W2002` を stderr に出して終了コード 0。stdout には何も出さない。エラー時は出力を書かない。

### 変換する宣言（Phase 1）

`TranslationUnitDecl` の `inner` の要素のうち、所属ファイルが `HEADER` で `isImplicit` でないものを、配列の順に扱う（所属の追跡は「アルゴリズム」）。

| clang の `kind` | 条件 | 生成 |
| --- | --- | --- |
| `FunctionDecl` | `previousDecl` がない（最初の宣言）。`storageClass` がない、または `"extern"`。`inline` がない。`variadic` がない。引数・結果がすべて対応表で変換できる | `extern "c_name" def tz_name :: P1 -> ... -> R`。引数なしは `unit -> R` |
| `FunctionDecl` | `previousDecl` がある（再宣言） | 何も出さない。報告しない |
| `EnumDecl` | 名前の有無を問わない | 各 `EnumConstantDecl` を `const NAME: i32 = value`。値は `inner[0]` の `ConstantExpr` の `value`、なければ直前の値 + 1（最初は 0） |
| `RecordDecl` | `tagUsed` が `"struct"`、名前がある、`completeDefinition` が true、全フィールドがレコード列で変換でき `isBitfield` がない | `record TzName { field: T, ... }`（1 行） |
| `RecordDecl` | `completeDefinition` がない（前方宣言） | 何も出さない。報告しない |
| `TypedefDecl` | 型が対応表の「引数」列のスカラーに解決される | `type TzName = T` |
| `TypedefDecl` | それ以外（struct・enum・ポインター・関数型など） | 何も出さない。報告しない（関数の型は解決後の型で書くので別名は不要） |
| `VarDecl` | すべて | 省略して `W2002`（global variable） |
| そのほか | `StaticAssertDecl`・`EmptyDecl` など | 何も出さない |

`#define` は clang の AST に現れないので Phase 1 では扱わず、報告もしない（D9）。

### 型の対応表（D3）

型の文字列は `type.desugaredQualType` があればそれ、なければ `type.qualType`。値として渡る型は先頭の `const `・`volatile ` を取り除き、
typedef 表（全ファイルの `TypedefDecl`）にある名前なら最大 32 段まで展開する。展開後の文字列の完全一致で次の表を引き、
表にない文字列はすべて変換不可とする（推測しない）。`long` を 64-bit とするのは LP64 だけを受け付けるため（D8）。

| C の型（展開後） | 引数 | 結果 | レコードのフィールド |
| --- | --- | --- | --- |
| `void` | ×（引数が 0 個なら `unit`） | `unit` | × |
| `_Bool` | `bool` | `i8u`（D5） | × |
| `char` | × | × | × |
| `signed char` / `unsigned char` | `i8` / `i8u` | `i8` / `i8u` | × |
| `short` / `unsigned short` | `i16` / `i16u` | `i16` / `i16u` | × |
| `int` / `unsigned int` | `i32` / `i32u` | `i32` / `i32u` | `i32` / `i32u` |
| `long`, `long long` / `unsigned long`, `unsigned long long` | `i64` / `i64u` | `i64` / `i64u` | `i64` / `i64u` |
| `float` / `double` | `f32` / `f64` | `f32` / `f64` | `f32` / `f64` |
| `enum TAG`（D3 の条件を満たす enum） | `i32` | `i32` | `i32` |
| `const struct TAG *`（TAG が本ヘッダーから生成したレコード） | `ref TzName` | × | × |
| そのほか（`long double`・`__int128`・`_Float16`・`_Complex`・vector・そのほかのポインター・配列・関数ポインター・値の struct/union） | × | × | × |

- `const struct TAG *` の判定: 値の型と同じく `const ` を外す前の文字列を使い、`const X *` の `X` が typedef なら展開して `struct TAG` になるものも含める。
  `const` のない `struct TAG *` は書き込みの可能性があるので変換しない。
- enum の条件: 全定数が i32 に収まり、`EnumDecl` に `fixedUnderlyingType` がなく、`inner` に `"kind": "PackedAttr"` がない。
- レコードのフィールドが 32/64-bit だけなのは、E05 の一時 struct が狭い整数と bool を `int32_t` に正規化するため（「現状」）。

### 名前の規則（D6）

- C の名前が `[A-Za-z_][A-Za-z0-9_]*` でなければ（clang 拡張の `$`・Unicode 識別子）省略して `W2002`。
- 関数・レコードのフィールド: snake_case。小文字または数字の直後の大文字の前に `_` を入れ、全体を ASCII 小文字にする
  （`glClearColor` → `gl_clear_color`、`SDL_Init` → `sdl_init`、`XMLParse` → `xmlparse`、`add` → `add`）。
- レコード・型別名: PascalCase。`_` で分割して空の部分を捨て、各部分の先頭を大文字にして連結する（`point` → `Point`、`point_t` → `PointT`、
  `SDL_Rect` → `SDLRect`）。先頭が英大文字にならなければ `C` を前に付ける。
- enum の定数: C の名前のまま。
- 変換後の名前が予約語（`src/lexer.rs` の `identifier` の一覧）、`_`、または関数名で `src/check.rs` の `Builtin` のうち `Builtin::name` が `.` を
  含まないもの（`sqrt`・`floor`・`ceil`・`abs`）なら、末尾に `_` を付ける（`type` → `type_`、`sqrt` → `sqrt_`）。
- 値の名前（関数・定数）と型の名前（レコード・型別名）はそれぞれ一つの名前空間とし、既に出した名前と衝突した後続の宣言は省略して `W2002`。
- シンボル（C の名前）が `tz_` または `tsuzuri_` で始まる関数は省略して `W2002`。

### 出力（D4）

```text
// Generated by tsuzuri bindgen. Do not edit.
// header: <HEADER のファイル名> sha256=<ヘッダーの bytes の SHA-256、小文字 hex 64 桁>
// clang: <clang --version の 1 行目>
// target: <clang -dumpmachine の出力>

<宣言。ヘッダーの順に 1 宣言 1 行。空行を挟まない>
// skipped <C の名前>: <理由>
```

- 省略した宣言は、その位置に `// skipped` の行を残す。コメントに入る文字列（ファイル名・clang の版・target・型の文字列）は、
  0x20–0x7E 以外の文字を `?` に置き換える（改行による生成コードへの注入を防ぐ）。
- 最後は改行 1 個。時刻・絶対パス・環境変数を含めない。同じ clang・同じヘッダー・同じ `--include-dir` なら byte 単位で同じ出力になる。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `W2002` | 宣言を省略した | `skipped C declaration '<C の名前>': <理由>; declare it by hand or wrap it in a C function with a supported signature` | ヘッダーの `loc.offset` から `loc.tokLen` bytes |
| `E2000` | CLI の誤り | `bindgen requires exactly one header path` / `bindgen requires -o OUTPUT.tz` / `bindgen output must have the .tz extension` / `bindgen accepts only -o, --include-dir, and --json` | なし |
| `E2001` | ヘッダーが通常ファイルでない・読めない、出力を書けない | `the header must be a regular file`、そのほかは `io_error` の既存形式 | なし |
| `E2002` | clang を起動できない・失敗した（ヘッダーの構文エラーを含む） | `run_tool` の既存形式。hint は `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` | なし |
| `E2002` | AST が大きすぎる・読めない、target が LP64 でない | `clang AST output exceeds 268435456 bytes; bind a smaller header that includes only the declarations you need` / `clang produced an unreadable AST (<serde_json のエラー>); use LLVM/Clang 17+ or bind a simpler header` / `bindgen supports only 64-bit LP64 targets (Linux and macOS); '<triple>' is not supported` | なし |
| `E2003` | 出力保護 | `bindgen output must not overwrite the header` / `the output must not be a symlink, directory, or special file` / `refusing to overwrite '<path>': it does not start with the tsuzuri bindgen marker; delete it or choose another output` | なし |

`W2002` の理由（この文字列を使う。判定はこの順で、最初に当たったものだけを出す）:

1. 関数: `function has internal linkage (static)` → `inline function has no guaranteed external definition` → `variadic function` →
   `function has no prototype` →
   `name is not an ASCII C identifier` → `symbol uses the reserved tz_ or tsuzuri_ prefix` → `parameter <n> has unsupported type '<qualType>'`（n は 1 始まり）→
   `result has unsupported type '<qualType>'` → `name '<Tsuzuri 名>' collides with an earlier declaration`。
2. struct・union: `C union` → `anonymous struct; give the struct a tag` → `bit-field '<field>'` → `field '<field>' has unsupported type '<qualType>'` → 衝突。
3. enum の定数: `enumerator value <v> does not fit in i32` → 衝突。変数: `global variable`。

### 資源上限

- clang の stdout は 268,435,456 bytes（256 MiB）まで。超えたら読み取りをやめて `E2002`。stdout と stderr は分けて受け取る。
- JSON の入れ子は `serde_json` の既定上限 128 のまま（HEAD の標準ヘッダーは深さ 29）。超えると `serde_json` のエラーを含む `E2002`。
- typedef の展開は 32 段まで。超えた型は変換不可。

### 例

新構文（E12 の実装後に有効。未検証）。`tests/fixtures/bindgen/basic/sample.h`（新規）:

```c
#include <stdint.h>
#define SAMPLE_LIMIT 42
typedef int32_t sample_count;
enum sample_mode { SAMPLE_FAST, SAMPLE_SLOW = 5, SAMPLE_BEST };
struct sample_point { double x; double y; int32_t tag; };
int32_t sample_add(int32_t a, int32_t b);
double sample_norm(const struct sample_point *p);
uint8_t sample_low_byte(uint64_t value);
_Bool sample_is_even(int64_t value);
void sample_reset(void);
enum sample_mode sample_next(enum sample_mode mode);
int sample_log(const char *format, ...);
static inline int sample_twice(int v) { return v * 2; }
```

期待する出力（`tests/fixtures/bindgen/basic/expected.tz`（新規）。先頭 4 行のうち clang と target の行はテストで正規化する）:

```text
// Generated by tsuzuri bindgen. Do not edit.
// header: sample.h sha256=<sample.h の SHA-256>
// clang: <clang --version の 1 行目>
// target: <clang -dumpmachine>

type SampleCount = i32
const SAMPLE_FAST: i32 = 0
const SAMPLE_SLOW: i32 = 5
const SAMPLE_BEST: i32 = 6
record SamplePoint { x: f64, y: f64, tag: i32 }
extern "sample_add" def sample_add :: i32 -> i32 -> i32
extern "sample_norm" def sample_norm :: ref SamplePoint -> f64
extern "sample_low_byte" def sample_low_byte :: i64u -> i8u
extern "sample_is_even" def sample_is_even :: i64 -> i8u
extern "sample_reset" def sample_reset :: unit -> unit
extern "sample_next" def sample_next :: i32 -> i32
// skipped sample_log: variadic function
// skipped sample_twice: function has internal linkage (static)
```

`W2002` は `sample_log` と `sample_twice` の 2 件。`SAMPLE_LIMIT` は出ない。

## 設計

### データ構造

```rust
// src/bindgen.rs（新規）。clang も I/O も使わない純関数だけを置く
pub const MARKER: &str = "// Generated by tsuzuri bindgen. Do not edit.";
pub const MAX_AST_BYTES: usize = 256 * 1024 * 1024;

pub struct HeaderInfo<'a> {
    pub path: &'a str,          // clang に渡した絶対パス。所属判定に使う
    pub file_name: &'a str,     // 先頭コメント用
    pub sha256: &'a str,
    pub clang_version: &'a str,
    pub target: &'a str,
}

pub struct Skipped { pub name: String, pub reason: String, pub offset: usize, pub length: usize }
pub struct Generated { pub text: String, pub skipped: Vec<Skipped> }

pub fn generate(ast: &serde_json::Value, header: &HeaderInfo) -> Result<Generated, String>;
pub fn lp64_target(triple: &str) -> bool;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar { Bool, I8, I8u, I16, I16u, I32, I32u, I64, I64u, F32, F64 }
enum CType { Void, Scalar(Scalar), RecordRef(String) } // 変換不可は None

struct Tables {
    typedefs: BTreeMap<String, String>, // typedef 名 → 型の文字列
    enums: BTreeMap<String, bool>,      // "enum TAG" → D3 の条件を満たすか
    records: BTreeMap<String, String>,  // TAG → 生成するレコード名（本ヘッダーの変換可能な struct だけ）
}
```

`generate` の `Err` は AST の形が想定外（`inner` が配列でないなど）の場合だけで、driver が `E2002`
（`clang produced an unreadable AST (...)`）にする。`Skipped` は driver が `Diagnostic::warning("W2002", ...)` に変換する。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| CLI | `src/main.rs` | `HELP` | usage に `tsuzuri bindgen header.h -o Module.tz [--include-dir DIR]... [--json]` を足す |
| CLI | `src/main.rs` | `enum Action`, `Arguments` | `Action::Bindgen`（新規）、`Arguments::include_dirs: Vec<PathBuf>`（新規） |
| CLI | `src/main.rs` | `parse_arguments` | 先頭の `lsp` 判定と同じ形で `bindgen` を先に分岐し、専用の短いループで `-o`/`--output`・`--include-dir`・`--json`・`--` だけを読む |
| CLI | `src/main.rs` | `main`, `run_action` | `Action::Fmt` と同じく project の読み込み前に `driver::bindgen` を呼ぶ。警告は `print_with_severity("warning", ...)` に HEADER のパスと本文を渡す。`run_action` には `Action::Bindgen => unreachable!("bindgen runs before compilation")` |
| lib | `src/lib.rs` | モジュール一覧 | `pub mod bindgen;` |
| driver | `src/driver.rs` | `bindgen`（新規） | ヘッダーの検査と読み込み、`Sha256`、clang の 3 回の起動（stdout・stderr を分ける）、`lp64_target`、`serde_json::from_slice`、`generate`、出力保護、書き込み。戻り値は `(String /* ヘッダー本文 */, Vec<Diagnostic>)` |
| driver | `src/driver.rs` | `protect_bindgen_output`（新規） | 拡張子 `.tz`、HEADER と同一でない（`fs::canonicalize` で比較）、symlink・ディレクトリ・特殊ファイルでない、既存なら 1 行目が `MARKER` |
| 変換 | `src/bindgen.rs`（新規） | `generate`, `lp64_target`, 下請け | 「アルゴリズム」 |
| テスト | `tests/bindgen.rs`（新規）、`tests/bindgen.mjs`（新規）、`tests/fixtures/bindgen/`（新規） | — | 「テスト計画」 |

コンパイラ本体（lexer・parser・check・llvm）は変更しない。生成物は E12 実装後の通常のソースとして既存の経路で検査・コード生成される。

### 生成 IR とランタイム

なし。bindgen はテキストを出すだけで、IR・ランタイム・WASM import を変えない。生成したモジュールの IR は E12 のリンク名付き extern の IR になる。

### アルゴリズム

```text
generate(ast, header):
  nodes = ast["inner"]（配列でなければ Err）
  # 1. 所属ファイルの追跡。clang は file を変化時にしか書かないので、全ノードを clang の出力順に訪ねる
  current = None; owned = []
  for node in nodes:
      visit_loc(node["loc"])                 # spellingLoc → expansionLoc の順。無ければ loc 自身
      file = current                         # 宣言の所属 = loc の後の current（マクロ展開なら展開位置）
      visit_range(node["range"])             # begin → end
      visit_children(node)                   # array_filler → inner の順に再帰（各子も loc → range → 子）
      owned.push(file == header.path && !node.isImplicit)
  visit_loc(x): if x["file"] is string: current = x["file"]
  # serde_json の Object はキー順を保たないので、上の順序はコードで固定する。Value の反復順に頼らない
  # 2. 表（全ファイル）: typedefs、enums。records は owned かつ変換可能な struct だけ（後方参照に備えて先に作る）
  # 3. 出力（owned だけ、配列の順）: 宣言表・型の対応表・名前の規則を適用し、行または // skipped 行と Skipped を足す
  text = MARKER + 先頭コメント 3 行 + 空行 + 行の連結 + 改行
```

- 引数の `ParmVarDecl` は `FunctionDecl` の `inner` のうち `kind == "ParmVarDecl"` のもの。結果の型は `type.qualType`（例 `"int (int, int)"`・
  `"void (int) __attribute__((noreturn))"`）の最初の `(` より前を trim した文字列。ただし最初の `(` の直後が `*` か `^` なら関数ポインター・
  block を返す宣言（`"int (*(int))(void)"`）なので結果を変換不可にする。最初の `(` から対応する `)` までが `()` なら prototype のない宣言として省略する。
- enum の値は `i64` で数え、i32 に収まらない定数は省略する（後続の暗黙値は省略した値 + 1 から数える）。
- `lp64_target`: triple の最初の `-` までが `x86_64`・`aarch64`・`arm64` のいずれかで、triple が `windows`・`mingw`・`cygwin` を含まないとき true。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインと E12 の確認

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。「再現」のコマンドで clang の JSON の性質を手元の clang で確かめる。E12 のリンク名で libc の関数を
  直接呼べることを確かめる（`/tmp/tz-e11/link/Main.tz` に `extern "labs" def c_labs :: i64 -> i64` と `c_labs (-5)` を書く）。
- 確認: `grep -c '"file"' s.json` が `1`。`target/release/tsuzuri run /tmp/tz-e11/link` が `5` を出す。出なければ停止条件（E12 未完）。

### 手順 2: モジュールの骨組みと `lp64_target`

- 変更: `src/bindgen.rs`（新規）、`src/lib.rs`、`tests/bindgen.rs`（新規）。
- 内容: `MARKER`・`MAX_AST_BYTES`・`HeaderInfo`・`Skipped`・`Generated`・`lp64_target` と、空の出力（先頭コメントだけ）を返す `generate`。
- 確認: `cargo test --locked --test bindgen` が `running 1 test`・`1 passed`（`accepts_only_lp64_targets`）。

### 手順 3: 所属ファイルの追跡

- 変更: `src/bindgen.rs` の `generate` と追跡の下請け、`tests/bindgen.rs`。
- 内容: 「アルゴリズム」1 を実装する。`serde_json::Value` のオブジェクトを `iter()` で回さず、`loc`→`range`→`array_filler`→`inner` を名前で引く。
- 確認: `cargo test --locked --test bindgen tracks_the_file_of_each_declaration` が `1 passed`。

### 手順 4: 型の対応表と関数

- 変更: `src/bindgen.rs`（`Scalar`・`CType`・`Tables::typedefs`・関数の出力）、`tests/bindgen.rs`。
- 内容: 対応表（D3）、typedef の展開、結果型の取り出し、`_Bool` の結果（D5）、関数の省略理由の前半（static・inline・variadic・prototype）。
  出力行を作る関数は `render_function`（新規）の一つにまとめる（E12 の構文変更の影響をここに閉じる）。
- 確認: `cargo test --locked --test bindgen` が `running 5 tests`・`5 passed`（手順 2・3 と `maps_scalar_parameters_and_results`・
  `bool_results_become_i8u`・`rejects_unprototyped_and_function_pointer_results`）。

### 手順 5: enum・typedef・struct

- 変更: `src/bindgen.rs`（`Tables::enums`・`Tables::records`、enum 定数・型別名・レコードの出力、`ref` 引数）、`tests/bindgen.rs`。
- 内容: 「変換する宣言」の表の enum・struct・union・typedef の行。レコードは表を先に作ってから出力する（後方参照）。
- 確認: `cargo test --locked --test bindgen` が `running 8 tests`（`numbers_enum_constants_like_c`・`resolves_typedef_chains_and_stops_after_32`・
  `generates_scalar_records_and_ref_parameters` を追加）。

### 手順 6: 名前・省略・コメント

- 変更: `src/bindgen.rs`、`tests/bindgen.rs`。
- 内容: 名前の規則（D6）、衝突、予約接頭辞、`W2002` の理由の順序、`// skipped` 行、コメント文字列の置換。
- 確認: `cargo test --locked --test bindgen` が `running 12 tests`（`converts_names`・`reports_skip_reasons_in_order`・`sanitizes_comment_text`・
  `output_is_deterministic` を追加）。

### 手順 7: driver

- 変更: `src/driver.rs` の `bindgen`（新規）・`protect_bindgen_output`（新規）。
- 内容: ヘッダーを `fs::symlink_metadata` で検査して読み、`Sha256::new`・`update`・`hex` でハッシュする。`--version` と `-dumpmachine` は
  `run_tool` で実行する。AST の実行は `spawn` し、stderr を別 thread で読みながら stdout を `take(MAX_AST_BYTES as u64 + 1)` で読む
  （pipe の詰まりを防ぐ）。上限を超えたら子プロセスを kill して `E2002`。非 0 終了は `run_tool` と同じ文言の `E2002`。
  `serde_json::from_slice` → `generate` → `protect_bindgen_output` → 書き込みの順。
- 確認: `cargo build --locked` が成功する。`cargo test --locked --lib` が成功する。

### 手順 8: CLI

- 変更: `src/main.rs` の `HELP`・`Action`・`Arguments`・`parse_arguments`・`main`・`run_action`。
- 内容: 「段ごとの変更」の CLI の行。既存のサブコマンドの解析経路は変えない。
- 確認: `grep -rn "Usage:" tests/ | head` で `--help` の全文を比較するテストがないことを確かめる（あれば停止条件）。
  `cargo build --release --locked && target/release/tsuzuri bindgen /tmp/tz-work-E11/h/s.h -o /tmp/tz-e11/S.tz` が `W2002` を 2 件
  （`logf2`・`twice`）出して成功し、`/tmp/tz-e11/S.tz` に `extern "add" def add :: i32 -> i32 -> i32` の行がある。

### 手順 9: fixture と E2E

- 変更: `tests/fixtures/bindgen/{basic,include,skipped,names}/`（新規）、`tests/bindgen.mjs`（新規）。
- 内容: 「テスト計画 / E2E」の全ケース。golden は手で書き、C の意味から導く（コンパイラの出力を貼らない）。
- 確認: `cargo build --release --locked && node tests/bindgen.mjs target/release/tsuzuri` が成功する。

### 手順 10: 実ヘッダーでの確認（テストには同梱しない）

- 変更: なし。
- 内容: macOS は `$(xcrun --show-sdk-path)/usr/include/zlib.h` と `math.h`、Linux は `/usr/include/zlib.h` と `/usr/include/math.h` を変換する。
- 確認: 各出力に `target/release/tsuzuri check` と `fmt --check` が成功する。`W2002` の件数と代表的な理由を完了報告に書く。

### 手順 11: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md README.md _docs/tools/command-line.md _docs/guides/native-interop.md _docs/feature-status.md`
  が成功する。

### 手順 12: 最終確認

- 変更: なし。
- 確認: `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`、
  `node tests/bindgen.mjs target/release/tsuzuri`、`node tests/host_imports.mjs target/release/tsuzuri` がすべて成功する（GUIDE §10）。

## テスト計画

### Rust テスト

`tests/bindgen.rs`（新規）。入力は `serde_json::json!` で書いた最小の AST（clang を起動しない）。`HeaderInfo` の `path` は `"/h/main.h"`。

| テスト | 確かめること |
| --- | --- |
| `accepts_only_lp64_targets` | true: `arm64-apple-darwin27.0.0`・`x86_64-unknown-linux-gnu`・`aarch64-unknown-linux-gnu`。false: `x86_64-pc-windows-msvc`・`x86_64-w64-mingw32`・`i686-pc-linux-gnu`・`wasm32-unknown-unknown`・`armv7-unknown-linux-gnueabihf` |
| `tracks_the_file_of_each_declaration` | `loc.file` のない宣言は直前の file を継ぐ。`range.begin.file` と子の `loc.file` の変化も後続に効く。`spellingLoc` が別ファイルで `expansionLoc` が本ヘッダーなら出力する。`isImplicit` は出さない |
| `maps_scalar_parameters_and_results` | 対応表の全行（`const int` の `const` 除去、`"int (void)"` が `unit -> i32`） |
| `bool_results_become_i8u` | `_Bool (_Bool)` が `bool -> i8u` |
| `rejects_unprototyped_and_function_pointer_results` | `"int ()"` は `function has no prototype`、`"int (*(int))(void)"` は `result has unsupported type` |
| `numbers_enum_constants_like_c` | `A, B = 5, C` → 0・5・6。`"-3"` の負値。`4294967295` は省略し、次の暗黙値は 4294967296 から数えてそれも省略 |
| `resolves_typedef_chains_and_stops_after_32` | 3 段の typedef が `i64` になる。33 段は変換不可 |
| `generates_scalar_records_and_ref_parameters` | `const struct p *` と typedef 経由の `const p_t *` が `ref P`。`struct p *`・値の `struct p`・`signed char` のフィールド・bit-field・union・無名 struct は省略 |
| `converts_names` | D6 の例すべて、`type` → `type_`、`sqrt` → `sqrt_`、`a$b` の省略、`tz_x` の省略、`fooBar` と `foo_bar` の後者の省略 |
| `reports_skip_reasons_in_order` | 理由ごとに 1 宣言。`Skipped` の `reason`・`offset`・`length` と `// skipped` 行が一致する |
| `sanitizes_comment_text` | `file_name` の改行・非 ASCII が `?` になる |
| `output_is_deterministic` | 同じ入力で 2 回の `text` が一致し、`/h/` を含まない |

### E2E

`tests/bindgen.mjs`（新規。引数はコンパイラのパス、clang は `TSUZURI_CLANG` か `clang`。`tests/host_imports.mjs` の形）。各ケースは一時ディレクトリへ
fixture をコピーして行う。

- golden（4 fixture）: 出力の 3・4 行目を `// clang: <clang>`・`// target: <target>` に置き換えて `expected.tz` と完全一致を比べる。
  2 行目のハッシュは `node:crypto` の SHA-256 で独立に計算して比べる。
  - `basic/`: 「例」のヘッダー。
  - `include/`: `main.h` が `a` を宣言し、`other.h`（`b` と `#define DECLARE_F int f_from_macro(void);`）を include し、`c`、`DECLARE_F`、`e` を宣言する。
    出力は `a`・`c`・`f_from_macro`・`e` で、`b` を含まない。
  - `skipped/`: `W2002` の理由ごとに 1 宣言。`--json` の stderr を行ごとに `JSON.parse` し、`code == "W2002"` の件数とメッセージを比べる。
  - `names/`: 予約語・組み込み名・camelCase・衝突。
- 決定性: 同じ入力で 2 回実行し byte 一致。
- 生成物の検査: 4 つの出力に `tsuzuri check` と `tsuzuri fmt --check` が成功する。
- round-trip: `basic/` の出力を `Sample.tz` として `Main.tz`（新規。`export def add_case :: i32` などで各 extern を呼ぶ）と同じ project に置き、
  `-O0` と `-O3` で `tsuzuri build <dir> --emit object` し、`sample.c`（新規。C の実装）・`host.c`（新規）と clang でリンクして実行する。
  `host.c` は各 `tz_*_case()` の値を、同じ引数で C から直接呼んだ値と C の意味から手で書いた値（`sample_low_byte(0x1234)` は `0x34`、
  `sample_is_even(-4)` は 1、`sample_next(SAMPLE_SLOW)` は `SAMPLE_BEST`、`sample_norm(&(struct sample_point){3, 4, 0})` は 5.0）の両方と比べ、
  `sample_reset` の呼び出し回数も数える。終了コード 0 を期待する。
- CLI: `-o` なし・`-o x.txt`・`--target wasm32`・ヘッダー 2 個 → `E2000`。ヘッダーがディレクトリ → `E2001`。`TSUZURI_CLANG=/nonexistent` と
  構文エラーのヘッダー → `E2002`。生成物でない既存の `.tz`・symlink・ヘッダー自身への出力 → `E2003` で、既存ファイルの内容が変わらない。
  生成済みの出力への上書きは成功する。
- WASM は対象外（生成物は native のリンク名を前提にする）。確保を伴わないので `live == 0` の検査は要らない。

### 既存テストへの影響

なし。`HELP` の文言だけが変わる。

### 性能

目標を設けない。手順 10 で実ヘッダーの変換時間を参考値として完了報告に書く。

## ドキュメント

- `docs/language.md`: 「ホスト関数のインポート」に bindgen の段落（対応表の要約、`W2002`、LP64 限定）。診断コードの表に `W2002`。
- `docs/architecture.md`: ファイルの責務の表に `src/bindgen.rs`。
- `README.md`: `## CLI` に `tsuzuri bindgen`。
- `_docs/tools/command-line.md`: `## コマンド` に bindgen、`## ツールと環境変数` に bindgen も `TSUZURI_CLANG` を使うこと。
- `_docs/guides/native-interop.md`: `## ホスト関数をインポートする` に生成の手順と例（実行しない `sh`・`text` の fence）。
- `_docs/feature-status.md` の E11 行と `_features/README.md` の E11 の状態。

## 受け入れ条件

- [x] `tsuzuri bindgen` が C ヘッダーから E12 のリンク名付き extern・定数・レコード・型別名を生成し、shim なしで C 関数を呼べる（round-trip が native の `-O0`/`-O3` で成功）。
- [x] 変換できない宣言を黙って捨てず、`W2002` と `// skipped` 行で理由を示す。対応表にない型を推測で変換しない。
- [x] 出力が決定的で、`tsuzuri check` と `fmt --check` を通る。golden 4 件が一致する（Phase 2 の `extended/` を加えて 5 件）。
- [x] 対象ヘッダー以外（include 先）の宣言を出さない。
- [x] 出力保護（`E2003`）と CLI の誤り（`E2000`）・ツールの失敗（`E2002`）が仕様どおり。
- [x] 新しい crate・serde_json の feature・`unsafe` を足していない。
- [x] GUIDE §10 の完了の定義を満たす（`_features/README.md` の状態欄・`_completed/` への移動・GUIDE の台帳はコーディネーターが行う）。

## 落とし穴

- clang の JSON は `file` を変化時にしか書かない。`loc.file` だけを見ると、include の後の宣言を別ファイルと誤る。`include/` の golden で検出する。
- `serde_json::Value` のオブジェクトはキー順を保たない。`for (k, v) in obj` で位置を訪ねると順序が壊れる。キーを名前で引く順序をコードで固定する。
- `run_tool` は stdout と stderr を連結する。AST の JSON に警告が混ざるので使わない。stderr を読まずに stdout だけ読むと、警告の多いヘッダーで pipe が詰まり止まる。
- `desugaredQualType` は最上位の typedef しか展開しない（`const pt_t *` は展開されない）。typedef 表で自分で展開する。
- 関数型の文字列の最初の `(` は、関数ポインターを返す宣言では引数の括弧ではない。`(*`・`(^` を判定しないと結果を `int` と誤る。
- `int f();` は C17 では prototype なし。`unit -> i32` にすると引数のある定義を誤って呼ぶ。
- `_Bool` の結果を `bool` にすると、Tsuzuri は i32 全体を `icmp ne` し、C が保証しない上位 bit を読む（D5）。
- レコードの値引数と結果は E05 では const ポインターと out ポインターで、C の値渡し・値返しと一致しない。`ref` 引数以外にしない。
- plain `char` の符号は target で違う（Linux の aarch64 は unsigned）。`long` は LLP64 で 32-bit。どちらも推測しない。
- Homebrew の clang は macOS の SDK を見つけられず `#include <stdint.h>` で失敗することがある。`E2002` の本文を確かめ、`TSUZURI_CLANG=/usr/bin/clang`
  か `SDKROOT` を設定する。E2E はほかの suite と同じ clang を使う。
- 先頭コメントの clang の版と target は機械ごとに違う。golden では正規化し、ハッシュ行は独立に計算して比べる。ハッシュは include 先を含まない。
- ファイル名や clang の出力に改行があると、生成コードへ任意の行を注入できる。コメント文字列は必ず置換する。
- リンク名に Mach-O の `_` を付けない（E12 の落とし穴と同じ）。
- `-std=gnu17` を変えると `_Bool` の綴り（C23 では `bool`）が変わり、対応表が外れる。変えない。

## 対象外

- C++ ヘッダー（`extern "C"` 以外）、マクロ関数、インライン関数の本体、`#define` の定数（Phase 2 で `-E -dD` の出力から整数リテラルだけを読む案）。
- ポインターと長さの組の buffer への変換（利用者の注釈で明示する案。推測しない）、不透明 struct ポインターの E12 `extern type` への変換、
  コールバック（E12 Phase 2）。
- 値渡し・値返しの struct、union、bit-field、無名 struct の typedef、global variable。
- Windows（LLP64）・32-bit target・WASM 向けの生成（G10 の後に別チケットで扱う）。
- 任意の clang 引数・`--define`・`--module`。

## 決定事項

### D1: サブコマンドと警告コード

- 決定: `tsuzuri bindgen` を新しいサブコマンドとし、省略の警告を `W2002`（GUIDE D-30 の仮割り当て）にする。
- 理由: 生成は build と入出力が違い（入力が C ヘッダー、出力がソース）、既存のサブコマンドへ混ぜると `E2003` の規則が衝突する。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: CLI の形

- 決定: `tsuzuri bindgen HEADER -o OUTPUT.tz [--include-dir DIR]... [--json]`。`-o` は必須で `.tz` だけ。`--module` は設けない。
- 理由: モジュール名はファイル名で決まる（`HELP` の「Each source file is one module named after its filename」）ので、`--module` は `-o` と矛盾し得る。
  `--` は既存の CLI で「以降はパス」を意味するため、旧案の `-- <clang 引数>` は使えない。
- 状態: 既定案（実装者はこの案に従う）
- 見直し提案: 旧案と依頼文の `--module Name` を削った。必要になれば「出力のファイル名と一致しなければ `E2000`」として足せる。

### D3: 型の対応と判定方法

- 決定: 「型の対応表」のとおり。clang の型文字列を typedef 展開後に完全一致で引き、表にないものは変換不可。enum は i32 に収まり固定の基底型と
  packed がないものだけ `i32`。struct は本ヘッダーの名前付きで 32/64-bit のスカラーだけのものを `record` にし、`const struct TAG *` の引数だけ `ref` にする。
- 理由: 型文字列は clang の版で表記が揺れ得るが、完全一致なら揺れは「省略」になり、誤った ABI にはならない。E05 の ABI（「現状」の prototype）と
  C の自然な ABI が一致する形だけを選んだ。
- 状態: 既定案（実装者はこの案に従う）

### D4: 出力の形と保護

- 決定: 「出力」の形。先頭行は `MARKER`、宣言はヘッダーの順に 1 行ずつ、省略は `// skipped` 行。既存ファイルは 1 行目が `MARKER` のときだけ上書きする。
- 理由: ヘッダーの順は利用者が対応を追いやすく、決定的。marker は `protect_documentation` の `.tsuzuri-docs` と同じ考え方で、手書きのソースを守る。
- 状態: 既定案（実装者はこの案に従う）

### D5: `_Bool` の結果

- 決定: `_Bool` の引数は `bool`、結果は `i8u`（0 か 1）。
- 理由: E05 は bool の結果を i32 全体の `icmp ne` で受けるが、C の ABI は下位 8 bit しか保証しない。`i8u` なら `trunc` で受けるので正しい。
  引数は Tsuzuri が 0/1 の i32 を渡すので C 側で正しく読める。
- 状態: 既定案（実装者はこの案に従う）

### D6: 名前の規則

- 決定: 「名前の規則」のとおり（関数とフィールドは snake_case、型は PascalCase、定数はそのまま、予約語と組み込み名は `_` を付ける、衝突は後続を省略）。
- 理由: リンク名が C のシンボルを保持するので、Tsuzuri 名の変換で呼び出し先は変わらない。型別名は大文字始まりが必須（`E1024`）。
- 状態: 既定案（実装者はこの案に従う）

### D7: ヘッダーの解析方法

- 決定: 設定済みの clang を `-x c -std=gnu17 -fsyntax-only -Xclang -ast-dump=json` で起動し、既存の `serde_json` で読む。手書きの C パーサーは作らない。
- 理由: 実ヘッダーはプリプロセッサ・条件コンパイル・システムヘッダーに依存し、部分的なパーサーは誤った宣言を黙って作る。clang は build で既に必須で、
  新しい依存が要らない。任意の clang 引数を許すと target や ABI を変えられて対応表の前提が崩れるので、`-I` だけにする。
- 状態: 既定案（実装者はこの案に従う）

### D8: 対応する target

- 決定: `clang -dumpmachine` が LP64（x86_64・aarch64・arm64 の Linux と macOS）のときだけ生成し、それ以外は `E2002`。Windows は G10 の完了後に
  MSVC の呼び出し規約と型幅を検証して別途扱う（旧「未決事項」の既定案を維持）。
- 理由: `long` と `unsigned long` の幅が target で変わり、生成物を別 target で使うと ABI が壊れる。
- 状態: 既定案（実装者はこの案に従う）

### D9: `#define` の定数

- 決定: Phase 1 では生成しない。報告もしない。
- 理由: clang の JSON AST に現れない（「再現」で確認）。`-dM -E` は定義位置を持たず対象ヘッダーで絞れない。
- 状態: 既定案（実装者はこの案に従う）

### D10: 対象の範囲

- 決定: 生成するのは `HEADER` 自身の宣言だけ。`--include-dir` は include の解決にだけ使う。
- 理由: include 先まで生成するとシステムヘッダーの宣言で出力が膨大になり、別の生成物と名前が重複する。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-07）

D1 の承認（サブコマンド `tsuzuri bindgen` と警告 `W2002`）と「全 Phase を実装する」指示を受け、Phase 1（手順 1–12）を実装した。
着手時の HEAD は `2ee813f`（ブランチ `wt/e11`。チケットの確認時は `f8dc655` で、その後 E12 が完了している）。手順 1 で E12 のリンク名
（`extern "labs" def c_labs :: i64 -> i64` の `c_labs (-5)` が `5`）と、Clang の JSON の性質（`"file"` が 1 回）を手元の Apple clang 21 で確かめた。

### Phase 1: 実装

- `src/bindgen.rs`（新規）: CLI の解析（`Arguments`・`parse_arguments`）、`MARKER`・`MAX_AST_BYTES`・`HeaderInfo`・`Skipped`・`Generated`・`generate`・
  `lp64_target`。所属ファイルの追跡（`Tracker`）、Clang の型表記の解析（`tokenize`・`TypeParser`・`function_shape`）、typedef 展開と表（`Tables`）、
  名前の規則、出力（`Emitter`・`render_function`）。Clang も I/O も使わない。
- `src/bindgen_driver.rs`（新規。`src/driver.rs` の子モジュールで、`test_runner.rs` と同じ `#[path]` の形）: `driver::bindgen`（ヘッダーの検査・読み込み・
  `Sha256`、Clang の 3 回の起動、`lp64_target`、`serde_json::from_slice`、`generate`、出力保護、書き込み）、`capture`（stdout は `MAX_AST_BYTES` まで、
  stderr は別スレッドで読む）、`publish`（同じディレクトリの一時ディレクトリに書いて rename。直前にも保護を検査）、`protect_bindgen_output`。
- `src/main.rs`: `HELP`（usage 行と説明）、`main` の `bindgen` 分岐と `bindgen_command`（W2002 は `DiagnosticSet` で位置順・50 件まで表示）。
  `src/lib.rs` に `pub mod bindgen;`、`src/driver.rs` に子モジュールの宣言 3 行。
- `src/check.rs`: `pub fn reserved_type_name`（record・extern type・型別名の予約名の判定を一つにし、既存の 3 か所の条件をこれに置き換えた。振る舞いは同じ）。
  `src/abi.rs`: `pub const RESERVED_SYMBOL_PREFIXES`（`link_name_error` の接頭辞の一覧を公開。振る舞いは同じ）。bindgen はどちらも直接使い、規則を複製しない。
- テスト: `tests/bindgen.rs`（新規 15 件）、`src/bindgen.rs` の単体テスト 3 件、`tests/bindgen.mjs`（新規）、`tests/fixtures/bindgen/{basic,include,skipped,names}/`（新規）。
- 文書: `docs/language.md`（「C ヘッダーからの生成」、診断表の `W2002`）、`docs/architecture.md`（ファイル表、不変条件の **bindgen**、検証コマンド）、
  `README.md`（特徴、ホスト連携、`tsuzuri bindgen` の節、CLI 書式、テストスイート）、言語リファレンスの `compiler/usage.md`（`### bindgen`、流れ図）、
  `compiler/native-interop.md`（「C ヘッダーから extern を生成する」。「未実装」の節を置き換えた）、`compiler/diagnostics.md`（`W2002`）、`compiler/option.md`、
  `languages/why-tsuzuri.md`・`languages/strategy.md`（計画中の注記と E11 へのリンクを実際の振る舞いに置き換えた）。

### Phase 1: 決定事項への追記（チケットから外れた判断）

1. **CLI は局所に足した。** `Action::Bindgen` と `Arguments::include_dirs` は作らず、`new` と同じく `main` の先頭で `bindgen_command` へ分岐する。解析は
   `bindgen::parse_arguments`（ライブラリにあり `tests/bindgen.rs` で検査）。E13・E10 が同時に `src/main.rs` の解析を変えるので、マージしやすくするため。
   driver の I/O も同じ理由で子モジュールに置いた。出力のファイル名が英大文字で始まる ASCII 識別子でなければ `E2000` にした（モジュール名になるため。
   そのままでは `tsuzuri check` が `E1011`）。出力の親ディレクトリは `build` と同じく作る。
2. **E12 の実際の規則に合わせた。** E12 は `tz_`・`tsuzuri`（`tsuzuri_` ではない）・`__` で始まるシンボル、`RESERVED_HOST_SYMBOLS`（`malloc`・`write`・`putchar` など）、
   255 bytes を超えるシンボルを拒否する。理由は `symbol uses the reserved tz_, tsuzuri, or __ prefix`・`symbol is reserved by the Tsuzuri runtime`・
   `symbol is longer than 255 bytes`。`render_function` の構文（`extern "symbol" def name :: ...`）はチケットの想定どおりだった。
3. **予約名。** 修飾なしの組み込み関数は今は 12 個（`sqrt`・`floor`・`ceil`・`abs`・`to_float`・`to_int`・`assert`・`clone_string`・`unreachable`・`to_string`・
   `not`・`ignore`）で、`Builtin::ALL` から引く。予約語は lexer で字句解析して判定する（一覧を複製しない）。型名は `check::reserved_type_name`
   （`Vec`・`Array`・`Task`・組み込みクラス名など。`record Vec` は `E1001`）に当たれば `_` を付ける。enum の定数も予約語・組み込み名なら `_` を付ける（`match` → `match_`）。
4. **library builtin の再宣言。** Clang は `sqrt` などを最初の言及で暗黙の `FunctionDecl`（`isImplicit`）として宣言し、ヘッダーの宣言の `previousDecl` は
   それを指す。「`previousDecl` があれば何も出さない」だけでは `math.h` の関数がほぼ全部消えたので、暗黙の宣言を指す場合は最初の宣言として扱う。
5. **enum の値。** 初期化式の型が `int` でないとき（`X = 1u`、`Y = -1L`）、Clang は `ConstantExpr` を `ImplicitCastExpr`（`IntegralCast`）で包む。
   「`inner[0]` が `ConstantExpr` でなければ直前 + 1」では `1u` が 0 になるので、cast を辿り、変換先の型で折り返した値を使う。読めない初期化式の定数と、
   そこから数える暗黙の値は `enumerator value is not an integer constant in the clang AST` で省く。
6. **型表記の照合。** 文字列の完全一致の代わりに、Clang の表記を小さな解析器（指定子、`const`・`volatile`・`restrict`・nullability の修飾子、ポインター、
   関数ポインター）で読み、typedef を展開してから表と完全一致で照合する。`const struct TAG *` の判定（ポインターの先の typedef を含む）と `const T *const` を
   正しく扱うため。`desugaredQualType` は表記を読めないとき（`typeof` など）だけ使う。`AlignedAttr`・`PackedAttr` を持つ typedef は展開しない
   （desugar すると属性が消え、配置が変わったフィールドを `i64` と誤るため）。読めない表記はすべて変換不可。
   （レビュー指摘 3 で `desugaredQualType` は使わなくなり、属性は許可リストで判定するようになった。「レビュー指摘の修正」を参照）
7. **追加の省略理由**（ABI が一致しない形を推測で生成しないため）: asm ラベルや overloadable で `mangledName` が C の名前（Mach-O は `_` 付き）と異なる関数
   （macOS の `__DARWIN_ALIAS` のように同じシンボルになる asm ラベルは生成する）、`noreturn` 以外の型属性（`ms_abi`・`preserve_most` などの呼び出し規約。
   Clang は属性を型の前にも後にも書く）、`pass_object_size` の引数、packed・aligned・`#pragma pack` の struct、alignment 属性付きのフィールド、
   フィールドのない struct、ASCII でないフィールド名、snake_case にすると衝突するフィールド。理由は `src/bindgen.rs` の文字列のまま、言語リファレンスに一覧がある。
8. **絶対パスを出さない。** 匿名の tag の表記 `struct (unnamed struct at /abs/path:1:2)` は `struct (unnamed struct)` にしてから理由に入れる。
   `typedef struct { ... } name;` の匿名 struct は typedef の名前と位置で報告する。`typedef enum { ... } name;` の匿名 enum は Clang の表記どおり `enum name`
   として表に入れ、引数の `name` を `i32` にする
   （同じ名前の enum の tag があれば変換しない。無名の struct・union の typedef は展開しない。レビュー指摘 2 の修正）。
9. **clang の失敗の hint。** ヘッダーのコンパイルの失敗は `fix the header, or pass the directories that it includes with --include-dir`。
   `install LLVM/Clang 17+ or set TSUZURI_CLANG to its executable` は Clang を起動できないときと `--version`・`-dumpmachine` の失敗だけ。
10. **golden。** `expected.tz` の 2 行目は `sha256=<sha256>` で、E2E は `node:crypto` で計算したハッシュと比べてから置き換える（3・4 行目と同じ扱い）。

### Phase 2: 実装（コーディネーターが指定した (a)–(d)）

「対象外」の先送り項目のうち、次の 4 つを具体化して実装した。Windows（LLP64）・32-bit・C++ ヘッダー・値渡しの struct・union・bit-field・大域変数は
対象外のまま（G10 が blocked）。変更は `src/bindgen.rs`（`Extras`・`Failure`・`generate_with`・`BufferAnnotation`・`ConsumeAnnotation`・`ParameterName`、
マクロの読み取り、ハンドル・コールバック・buffer の対応）、`src/bindgen_driver.rs`（4 回目の Clang の起動）、`src/main.rs`（`HELP`）、テスト、文書。

**(a) `#define` の整数定数。**
- 入力は 4 回目の Clang の起動 `clang -x c -std=gnu17 -E -dD [-I DIR]... HEADER`（AST と同じ上限と stderr の扱い）。D9 の理由は「`-dM -E` は定義位置を持たない」
  だったが、`-dD` は定義をソース上の位置に残すので、行マーカーから所属ファイルと行が分かる。
- 所属と位置: 行マーカー `# N "FILE" FLAGS` が次の出力行のファイルと行番号を与え、以後の各出力行が次のソース行になる（Clang は短い空白を空行で、
  長い空白を行マーカーで埋め、継続行は 1 行に結合して空行を足す。手で確かめた）。ファイル名の C エスケープ（`\\`・`\"`・`\t`・`\n`・`\ooo`）を戻し、
  ヘッダーの絶対パスとバイト列で比べる。`#define` と `#undef` を出力順に適用した最終状態のうち、最後の定義がヘッダーにある object-like マクロだけを対象にし、
  位置はヘッダーのバイト列の行頭表（`\r\n`・`\n`・`\r`）と、行内の `define` の後の名前から求める。宣言とはヘッダー内の byte offset でマージし、
  値の名前空間の衝突もこの順で後続を省く。
- 値: 置換列を `(`・`)`・`-`・preprocessing number に字句分割し（そのほかの字句、`--`、65 個以上の字句は対象外）、外側の括弧と単項マイナスを外すと
  1 つの整数リテラルになるものだけを読む。浮動小数点（`.`、10 進の `e`、16 進の `p`）は対象外。10・16・8・2 進（2 進は gnu17 の拡張）と、
  接尾辞 `u`・`l`・`ll` の組み合わせ（大文字小文字。`lL` は不正）。型は C17 6.4.4.1 の候補列を LP64 の幅で引く（接尾辞なしの 10 進は `int`・`long`、
  それ以外の進数は `int`・`unsigned int`・`long`・`unsigned long`、`u` は `unsigned int`・`unsigned long` など）。`int` → `i32`、`unsigned int` → `i32u`、
  `long`・`long long` → `i64`、`unsigned long`・`unsigned long long` → `i64u`。単項マイナスはその型で計算する（`(-1u)` は `4294967295` の `i32u`、
  `-2147483648` は `long` の `-2147483648`）。
- 報告（`W2002`）: 64 bit に収まらない値、`u` なしで符号付き 64 bit に収まらない 10 進数（C17 では型がない。Clang の拡張の `unsigned long long` を推測しない）、
  不正なリテラル（`09`・`0x`・`0b2`・`1lL`）、変換しない接尾辞（`1f`・`1wb`）、ASCII でない名前、名前の衝突。それ以外のマクロ（式、文字列、文字、
  別のマクロ名、関数形式、空の置換列）は何も出さない（include guard や設定のマクロを報告しないため）。名前は C の名前のまま（予約語・組み込み名は `_` を付ける）。

**(b) 不透明な struct のハンドル。** 規則と、それが E12 のハンドルの意味（所有・Copy・ABI）で健全である理由:
- 対象は、名前付きの struct の tag のうち、翻訳単位のどこにも（入れ子の宣言を含めて）定義（`completeDefinition`）がなく、最初の宣言（`previousDecl` なし）が
  ヘッダーにあるもの。その宣言の位置に `extern type Name`（PascalCase。型の名前空間で衝突を検査）を出す。定義のある struct（`z_stream`、一部の欄を公開する
  zlib の `gzFile_s` を含む）は呼び出し側が確保・参照しうるのでハンドルにしない。最初の宣言が include 先の tag も出さない（D10）。
- 引数の `struct X *`・`const struct X *` は共有借用 `ref X`。E12 の `ref H` はハンドルの値そのもの（ポインター）を渡すので ABI は一致する。Tsuzuri は
  ハンドルを参照外ししないので、C がポインターの先を書き換えても Tsuzuri の別名の規則は破れず、呼び出しはハンドルを消費も複製もしない。
- 結果の `struct X *` は所有する `X`。ハンドルは drop glue を持たない（E12 D3）ので、借用されたポインターを返す関数でも Tsuzuri が勝手に解放することはない。
  結果の `const struct X *` は借用の結果で、E12 は借用の結果を ABI に許さないので省く。
- `--consume FUNCTION:PARAMETER`（新規）を付けた引数は値の `X`（ムーブ）で、その後の使用は `E1012`。E12 の規約（解放は close の extern へ値で渡す）を
  生成物でも使えるようにするための注釈で、C の宣言には所有の情報がないので推測しない。
- コールバックの引数の `struct X *` は `ref X`（E12 が許す形。C が渡すポインターをコールバックが所有するとは限らない）。
- 変換しない: `X **`（出力引数。E12 に `ref mut H` の ABI はない）、struct のフィールド（E12 は ABI record のフィールドにハンドルを許さない）、`volatile`。
  NULL は区別できない（E12 に `Option<H>` がない）が、Tsuzuri はハンドルを参照外ししないので、NULL の扱いは C の API の契約に従う（文書化した）。

**(c) コールバック。** 関数ポインターの引数（typedef 経由と nullability を含む）`R (*)(P1, ...)` は、`P` がコールバックの引数の表（スカラー、`int` の enum、
不透明な struct のポインター → `ref X`）、`R` が結果の表（スカラー、`void` → `unit`）にあるとき、E12 の静的コールバックの型 `(P1 -> ... -> R)`
（引数なしは `(unit -> R)`）になる。C から渡る `_Bool` の引数は `i8u` にした（D5 の逆向き: E12 の wrapper は `bool` の引数を 32 bit 全体の `icmp ne` で読むが、
C の呼び出し側は下位 8 bit しか保証しない。狭い整数は wrapper が `trunc` するので正しい）。`_Bool` の結果は wrapper が 0/1 を返し C が下位 8 bit を読むので `bool`。
`void *`（文脈引数）、可変長引数、プロトタイプなし、ポインターの結果、入れ子の関数ポインター、block、呼び出し規約などの属性、関数ポインターへのポインターは変換しない。

**(d) ポインターと長さの組 `--buffer FUNCTION:POINTER:LENGTH`。** E05/E12 の import の buffer の ABI は `ref [T]` を「要素へのポインター、`int64_t` の要素数」
の 2 引数で渡す（`src/llvm_imports.rs` の `host_call`）。C の `(const T *p, size_t n)` と完全に一致するのは、ポインターが `const`（`volatile` でない）要素への
ポインターで、要素が Tsuzuri の buffer と同じ表現（`long`・`long long` = `[i64]`、`double` = `[f64]`、`unsigned char`・`char`・`void` = `[ubyte]`）、長さが
直後の 64-bit 整数（`long`・`long long`・`unsigned long`（`size_t`）・`unsigned long long`。要素数は非負の i64 なので `size_t` でもビット列は同じ）のとき。
要素数かバイト数か（`void` はバイト数）、本当に読み取りだけかは宣言から分からないので、利用者が注釈で指定したときだけ生成する（引数は C の名前か 1 始まりの位置）。
`const` のないポインター（ホストが共有借用の配列へ書き込める）、長さが直後にない、長さが 64 bit でない、同じ表現の buffer がない要素（`int`・`float`・
`signed char`・`uint16_t` など）は `W2002`。注釈の関数・引数がない、同じ引数を 2 つの注釈が指す（ポインターと長さが同じ引数を含む）は `E2000`（終了コード 1。
注釈の形の誤りは引数の解析で終了コード 2）。`ref string`・`ref utf8string` は C のどの型と組かを宣言から決められないので生成しない（`const char *` は
`[ubyte]` で、`Utf8String.to_bytes` で渡せる）。空の配列は NULL と長さ 0 で渡ることがある（文書化）。

注釈を指定すると、出力の 5 行目に `// options: --buffer ... --consume ...` を記録する（再生成に要るため。パスを含まない）。

### Phase 2: 決定事項への追記

1. **マクロは既定で生成する**（オプトインにしない）。D9（Phase 1 は生成しない）を Phase 2 で改めたので、golden `basic/expected.tz` に
   `const SAMPLE_LIMIT: i32 = 42` が加わった（チケットの「`SAMPLE_LIMIT` は出ない」は Phase 1 だけの記述）。
2. 「整数に見えるマクロ」は「外側の括弧と単項マイナスを外すと 1 つの preprocessing number で、浮動小数点でないもの」と定義した。`(1 << 4)` のような式は黙って無視する。
3. `--consume` は (b) を E12 の所有の規約と両立させるために足した注釈で、指示にない追加。注釈がなければ引数は借用なので、解放関数を生成しても Tsuzuri が何かを
   二重に解放することはない（解放後の使用を型で防げないだけ）。
4. 2 進のリテラル（gnu17 の拡張）も受け付けた。
5. Clang の起動は 4 回（`--version`、`-dumpmachine`、AST、`-E -dD`）になった（レビュー指摘 4 の修正で `-E -dM` を足して 5 回）。

### 確認（Apple M1 Max、macOS 27、Apple clang 21.0.0（`arm64-apple-darwin27.0.0`）、rustc 1.98.1、Node v20.19.6。6 エージェントで共有した機械）

- `cargo test --locked --test bindgen`: Phase 1 で 15 passed、Phase 2 で 22 passed。`cargo test --locked --lib bindgen`: 3 → 5 passed（型表記の解析、関数型の分割、
  enum の値の変換、行マーカーと行頭表、マクロの値）。期待値はすべて C の規則から手で書いた（`0755` = 493、`(-1u)` = 4294967295 など）。
- `RUST_MIN_STACK=4194304 cargo test --locked`: Phase 1 の後 754 passed・0 failed（72 binaries）、Phase 2 の後 763 passed・0 failed（72 binaries）。
  GUIDE §3.1 の 4 テスト（`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・
  `bounds_nested_builder_expansion_not_just_source_syntax`・`honors_the_exact_specialization_limit`）は既定の stack で各 1 passed。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（Homebrew の clippy 1.98）が成功。Windows の型検査
  `PATH=~/.cargo/bin:$PATH CARGO_TARGET_DIR=target/wincheck cargo clippy --locked --all-targets --target x86_64-pc-windows-msvc -- -D warnings`（rustup の
  clippy 1.96.1）は、変更していない `src/lsp.rs:806`・`src/parser.rs:1956-1957` の `nonminimal_bool` 3 件で失敗する（既存。clippy 1.98 では出ない）。
  `-A clippy::nonminimal_bool` を足すと成功し、bindgen の変更による警告はない。bindgen の変更に OS 依存のコードはない（`same_file` と
  `TemporaryDirectory` は既存の cfg 付きの実装を使う）ので aarch64-pc-windows-msvc は実行していない。
- `cargo build --release --locked && node tests/bindgen.mjs target/release/tsuzuri`: 6 行（golden 5 件・決定性・`check`・`fmt --check`・`--json` の W2002 27 件と
  位置、basic の往復 `-O0`／`-O3`、extended の往復 `-O0`／`-O3`（IR の `malloc`・`free`・`realloc` を追跡関数に置き換えて 1000 回ごとに `live == 0`、
  object のリンク、`ext_live_counters() == 0`）、CLI と出力保護）。`node tests/host_imports.mjs`（2 行）、`node tests/ffi_extensions.mjs`（5 行）も成功。
  WASM は対象外（生成物は native のリンク名を前提にする。チケットのテスト計画どおり）。
- `node scripts/check-docs.mjs` で変更した 6 ページ（`compiler/usage.md`・`native-interop.md`・`diagnostics.md`・`option.md`・`languages/strategy.md`・
  `why-tsuzuri.md`）が成功（80 links、7 checked examples。生成した `Sample.tz` を例のプロジェクトとして型検査する）。文書の `counter.h`・`stats.h`・マクロの表の
  出力は実際に生成して確かめた。
- 実ヘッダー（テストに同梱しない。`$(xcrun --show-sdk-path)/usr/include`、release、負荷の高い共有機での参考値）:
  - `zlib.h`（zlib 1.2.12）: 0.34 s。生成 42 行（`const` 35、`extern type InternalState` 1、`extern def` 6）、`W2002` 78 件（引数 1 の型 65、引数 2 の型 6、
    結果の型 3、フィールドの型 3、可変長引数 1）。`z_streamp`・`gzFile`（定義のある struct）・`const char *`・`Bytef *` の関数が大半。
  - `math.h`: 0.13–0.21 s。生成 132 行（`extern def` 111、`const` 17、`type` 2、`record` 2）、`W2002` 117 件（引数 1 の型 59。主に `long double` と
    `_Float16`、`tz_`・`tsuzuri`・`__` の接頭辞 29、inline 19、引数 2・3 の型 8、大域変数 1、フィールド 1）。`sqrt`・`ceil`・`floor` は `sqrt_` などになる。
  - どちらの出力も `tsuzuri check` と `tsuzuri fmt --check` が成功した。`stdio.h`・`stdlib.h`・`string.h` は macOS では `_stdio.h` などの include だけなので
    0 宣言（D10 どおり）。
- 性能は主張しない。

### レビュー指摘の修正（2026-10-08）

統合ブランチへのマージ後の独立レビューの指摘 4 件を `wt/e11` で直した。方針は「確かでなければ `W2002` で省く」。各指摘は修正前の実装で
Clang から再現し（誤った出力になることを確かめ）、修正後は Rust のテストと golden `skipped` で省かれることを確かめた。

1. **配置を変える属性を持つ enum（`src/bindgen.rs` の `HARMLESS_ATTRIBUTES`・`layout_attribute`）。** enum は `PackedAttr` だけを見ていたので、
   `enum __attribute__((aligned(8))) E`（`AlignedAttr`）や `__attribute__((mode(QI)))`（`ModeAttr`。1 byte の enum）を `i32` にし、record のフィールドの
   オフセットと引数・結果の幅を黙って誤っていた。拒否リスト `LAYOUT_ATTRIBUTES` を、配置と呼び出し規約を変えないと分かっている属性の許可リスト
   `HARMLESS_ATTRIBUTES`（`Annotate`・`Availability`・`AvailableOnlyInDefaultEvalMethod`・`Deprecated`・`EnumExtensibility`・`FlagEnum`・`MayAlias`・
   `ObjCBoxable`・`ObjCBridge`・`ObjCBridgeMutable`・`ObjCBridgeRelated`・`SwiftAttr`・`SwiftBridgedTypedef`・`SwiftName`・`SwiftNewType`・`SwiftPrivate`・
   `Unavailable`・`Unused`・`Used`・`Visibility`。macOS SDK のヘッダーに現れる属性を調べて決めた）に置き換え、enum・typedef・struct・フィールドが
   それ以外の属性（`Aligned`・`Packed`・`Mode`・`MaxFieldAlignment`（`#pragma pack`）・`RandomizeLayout`、未知の属性）を 1 つでも持てば、その型を変換しない。
   enum の定数は値が型によらないので出し、その enum 型の引数・結果・フィールドを省く。理由は属性を示す形にした
   （`struct has __attribute__((packed)), which may change its layout`、`field 'b' has __attribute__((aligned)), which may change its layout`）。
2. **無名の struct の typedef と同じ名前の tag（`Tables::new`・`unnamed_tag`）。** Clang は `typedef struct { int small; } S;` の型を `struct S` と表記する
   ので、`S` を展開すると無関係の tag `struct S`（tag と typedef 名は C の別の名前空間）の record やハンドルに解決し、`ref S` の配置やハンドルを誤っていた。
   typedef の型ノード（`MAX_TYPE_NESTING` 段まで）に名前のない `ownedTagDecl` の struct・union があれば、その typedef を展開不可にした（`const S *` の引数は
   `W2002`、`struct S *` は従来どおり tag の型）。無名の enum の typedef は従来どおり `enum S` として表に入れるが、同じ名前の enum の tag が（定義・前方宣言、
   struct の中の宣言を含めて）あれば、tag とともに変換しない。struct の中の tag は C ではファイルスコープなので、`struct outer { enum E { BIG = 0x7fffffffffff } e; };`
   と `typedef enum { SMALL } E;` で `enum E` を `i32` にしていた（8 byte の enum。レビュー後の調査で見つけて同時に直した。tag の収集を全ノードの走査
   `visit_all` に変えた）。無名の union は、名付ける typedef の名前と位置で報告する。
3. **`typeof` の desugar（`node_type`・`Tables::c_type`）。** 読めない表記（`typeof` など）を `desugaredQualType` で読み直していたので、
   `__typeof__((wide_aligned)0)`（`aligned(16)` の typedef）のフィールドを `i64` とし、オフセットを誤っていた（desugar は typedef とともに属性を落とす）。
   `desugaredQualType` を一切使わないようにした（`node_type` は `qualType` だけを返し、`Typedef` から `desugared` を除いた）。読めない表記は、フィールド・
   引数・結果・typedef の展開のどこでも変換しない。`typedef __typeof__(sizeof(0)) my_size_t;` のような typedef も変換しなくなる（安全側）。
4. **`#pragma push_macro`／`pop_macro`（`header_macros`・`define_line`、`src/bindgen_driver.rs`）。** `clang -E -dD` は両 pragma を出力しない（空行になる）ので、
   `pop_macro` で戻された値を誤って出していた（C では 1 の `LEVEL` を `const LEVEL: i32 = 2`）。5 回目の Clang の起動
   `clang -x c -std=gnu17 -E -dM [-I DIR]... HEADER`（ヘッダーの終わりのマクロ表。`-dD` と同じ printer なので置換列は同じ文字列になる）と照合し、
   最後の定義が object-like で置換列が一致するマクロだけを生成する。一致しない・消えた・関数形式になったマクロは、`-dD` の置換列か最終の置換列が
   整数リテラルに見えれば `W2002`（`the macro does not end with this #define (#pragma push_macro and pop_macro can restore another value)`。位置は `-dD` が示す
   最後の `#define`）、そうでなければ従来どおり何も出さない。pragma に現れる名前を除く案は採らなかった（`-dD` には pragma 自体が現れず、`_Pragma` や
   include 先の pragma もあるので、最終状態を Clang 自身に問う方が確実）。
5. **同時に直したもの。** `returns_twice`（`ReturnsTwiceAttr`。`setjmp`・`vfork` の類）の関数を省く（`function returns twice (returns_twice), which a Tsuzuri
   call cannot follow`。生成する呼び出しには `returns_twice` が付かず、2 度目の戻りでレジスターの値が壊れうる）。関数型の後ろの `__attribute__` 以外の語
   （AArch64 の SME の `__arm_streaming`・`__arm_inout("za")` など）は以前から省いていたが、理由の属性名が `'?'` だったので語をそのまま示す。

テスト: `tests/bindgen.rs` に 4 件（`converts_only_types_whose_attributes_keep_the_layout`・`does_not_read_unnamed_struct_typedefs_as_tags`・
`never_reads_desugared_types`・`generates_a_macro_only_where_its_final_definition_agrees`。JSON AST と `-dD`／`-dM` の出力を手で書く）と
`splits_function_types` の 1 assert を足し、golden `skipped/skipped.h` に指摘ごとのケース（`aligned`・`mode`・`flag_enum` の enum、`randomize_layout`、
`struct tagged` と無名の `tagged`、struct の中の enum の tag、`typeof`、`returns_twice`、`push_macro`）を足した（`// skipped` 27 → 37 行。
`tests/bindgen.mjs` の `--json` の件数も 37）。

確認（同じ機械）:
- `cargo test --locked --test bindgen` 26 passed（22 → 26）、`cargo test --locked --lib bindgen` 5 passed。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings` が成功。Windows の型検査（x86_64-pc-windows-msvc、rustup の clippy 1.96.1）は
  Phase 2 と同じ既存の `nonminimal_bool` 3 件だけで失敗し、`-A clippy::nonminimal_bool` で成功（OS 依存のコードは変えていない）。
- `cargo build --release --locked && node tests/bindgen.mjs target/release/tsuzuri` の 6 行が成功（`-O0`／`-O3` の往復を含む）。
  `node scripts/check-docs.mjs _tsuzuri/language-reference/compiler/native-interop.md` が成功（10 links、4 checked examples）。
- 実ヘッダーの結果は Phase 2 と同じ: `zlib.h` 生成 42 行・`W2002` 78 件、`math.h` 生成 132 行・`W2002` 117 件（`float_t`・`double_t` の
  `AvailableOnlyInDefaultEvalMethodAttr` は許可リストにあるので `type FloatT = f32` などのまま）。どちらも `tsuzuri check` と `fmt --check` が成功。
- 文書: `docs/language.md`（「C ヘッダーからの生成」の属性・無名の tag・`typeof`・`-E -dM`・`returns_twice`）、`docs/architecture.md`（**bindgen:**）、
  言語リファレンスの `compiler/native-interop.md`（「対応する形」と「マクロの定数」）。

### 既知の制限と残作業

- Linux での実行は確かめていない（macOS arm64 だけ）。`-dumpmachine` が Linux の LP64 なら受け付け、`mangledName` の `_` 接頭辞は apple・darwin の triple だけで仮定する。
- Windows（LLP64）・32-bit・C++ ヘッダー・値渡しと値返しの struct・union・bit-field・大域変数・無名 struct の typedef は対象外（G10 の後）。
- 入れ子の struct や固定長配列のフィールド（A16 Phase 2 で ABI record に許された `[T; N]`）を持つ struct は変換しない（フィールドの表を 32/64-bit のスカラーに限った）。
- マクロは置換列が 1 つの整数リテラルのものだけで、別のマクロの参照（`#define A B`）や式は評価しない。struct の中で宣言された enum・struct の tag は生成しない。
- NULL のハンドルは区別できない。`void *` の文脈引数を持つコールバック（C の多くのコールバック API）は E12 の静的コールバックで表せない。
- `W2002` の表示は既存の警告と同じく 50 件までで、残りは件数だけ（出力の `// skipped` 行にはすべて残る）。
- コミット: Phase 1 `9d80edc`、Phase 2 `aa1e829`、レビュー指摘の修正は「レビュー指摘の修正」節を含むコミット。`_features/README.md`・`_completed/` への移動・GUIDE（D-30 の `W2002` と `tsuzuri bindgen` の確定、D-16）は
  コーディネーターが行う。
