# PM03: コンパクト文字列（Latin-1 の内部表現）

| 項目 | 内容 |
| --- | --- |
| ID | PM03 |
| 分類 | メモリ |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | PX01, PR05 |
| 関連 | A08, D02, E05, E13 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（`%tz.string` の内部表現の変更。承認前はどの手順にも着手しない）。D2〜D10 は既定案 |
| 手本にする既存実装 | 内容で形式を選ぶ生成: `src/runtime/string.ll` の `@tz.string.from_utf8` の `ascii_only` 分岐。単位の幅による読み分け: `src/llvm_hash.rs` の `hash_primitive`（`bits` が 16 か 8）。runtime 関数を抜き出す試験: `tests/strings.mjs` の guard harness（`runtimeGuards`）。確保の計数と `live == 0`: `tests/strings_runtime.c`、`tests/strings.mjs` の host の `tracked_alloc` |
| 主な影響ファイル | runtime: `src/runtime/string.ll`, `src/runtime/string_latin1.ll`（新規）, `src/runtime/string_scalar.ll`・`src/runtime/string_v128.ll`（PR05 が作る。改名だけ）, `src/runtime/display.ll`, `src/runtime/character.ll`。生成: `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_bulk.rs`, `src/llvm_display.rs`, `src/llvm_hash.rs`, `src/llvm_abi.rs`, `src/llvm_imports.rs`。変えない: `src/abi.rs`, `src/llvm_debug.rs`, `std/String.tz`。試験: `tests/strings.rs`, `tests/strings.mjs`, `tests/strings_runtime.c`, `tests/compact_strings.mjs`（新規）。計測: `benchmarks/strings/ascii_words/Main.tz`・`benchmarks/strings/latin1_csv/Main.tz`・`benchmarks/strings/cjk_lines/Main.tz`（新規）。文書: `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md` |
| 計測対象 | メモリ: 上の 3 workload の最大 RSS（PX01 の process suite の `execute`）、`cpp/utf16_scan`・`cpp/utf16_compare`・`cpp/utf16_validate`・`cpp/utf8_roundtrip`・`cpp/format_parse` の確保量（PX01 の tracked）。時間: 同じ cpp 5 種目と PR05 の `utf/*` |

## 目的

`string`（UTF-16 のコード単位列）の観測できる意味を一切変えずに、全コード単位が 0xFF 以下の文字列を 1 単位 1 byte（Latin-1 形式）で保存し、
ASCII 中心のテキストの文字列バッファを半分にする。JVM の Compact Strings、V8 の one-byte string と同じ方式である。
比較・連結・変換は Latin-1 同士なら読む byte 数が半分になり、速くなる見込みがある（計測で確かめるまで主張しない）。

Phase 1（表現と、runtime・生成の全操作のスカラー実装）だけを実装する。Phase 2（PR05 の 128-bit ベクトル版に Latin-1 の kernel を足す）は
人間が求めた場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- D1 が承認済みであること。承認前はどの手順にも着手しない。
- PX01 と PR05 が `_perfs/README.md` の状態欄で done であること。確認: `grep -n "| PX01 \|| PR05 \|| PM03 " _perfs/README.md`。
- PR05 の実装後の runtime が「前提とする他チケットのインターフェース」と一致すること。確認:
  `grep -n "^define" src/runtime/string.ll src/runtime/string_scalar.ll src/runtime/string_v128.ll src/runtime/utf8string.ll`。
- GUIDE §2.3 の基準コマンドと GUIDE §14 の性能チケットの共通手順を実行し、実装手順 1 のベースライン（基準のコンパイラ、IR、計測）を保存していること。

### 前提とする他チケットのインターフェース

- PX01（done が条件）: `node benchmarks/run-cpp.mjs <compiler> --metrics <dir>` が `<dir>/cpp.jsonl` に PX01 形式（JSON Lines、`schema: 1`）で
  時間と tracked の確保（`calls`・`bytes`）を書く。`node benchmarks/metrics.mjs process --project <P> ...` が `execute` variant の
  `peak_rss` を 9 標本で書き、9 回の stdout が同じことを確かめる。`node benchmarks/metrics.mjs report <after> --baseline <before>` が比較表を出す。
  名前や旗が違えば PX01 の実装に合わせてこの節と「計測手順」を直してから進む。
- PR05（done が条件）: `%tz.string = type { ptr, i64 }` と runtime 関数の名前・引数は HEAD のまま。`@tz.string.from_ascii`・`@tz.utf8string.from_ascii`・
  `@tz.string.equal`・`@tz.string.compare`・`@tz.string.from_utf8`・`@tz.utf8string.from_string`・`@tz.string.is_well_formed`・
  `@tz.string.to_well_formed`・`@tz.utf8string.equal`・`@tz.utf8string.compare` と計数関数 2 つが `src/runtime/string_scalar.ll` と
  `src/runtime/string_v128.ll` の両方に同名・同引数であり、どちらか一方だけが連結される。`@tz.string.copy`・`allocate`・`new`・`concat`・
  `decode_utf8`・`try_decode_utf8`・`decode_utf16`・`to_ascii` は `src/runtime/string.ll` に残る。PR05 の差分 harness（`tests/strings_simd.c`）は
  12 関数の名前を列挙する。
- E13（関連、着手条件にしない）: 生成バインディングは `"slice:string"`・`"buffer:string"` を UTF-16（`Uint16Array`）として読み書きする。
  PM03 は公開 ABI を変えないので E13 の表も変わらない（D7）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- PR05 の実装後の関数名・置き場所が上の前提と違い、「段ごとの変更」の runtime の行を当てはめられない。
- `%tz.string` の大きさ（16 bytes）、`storage_layout`、`src/abi.rs` の `Buffer::String`（`uint16_t`、幅 2）、E05・E13 の公開 ABI を変える必要が出た。
- 長さ語（`%tz.string` の欄 1）を読む箇所が「段ごとの変更」の表にないファイルで見つかった（表の網羅が崩れている）。
- 手順 4（消費側だけを変え、生成側はまだ UTF-16 だけを作る段）で、IR 以外の観測結果（stdout、trap、`live`）が一つでも変わった。
- 差分テストで HEAD の結果と一つでも違い、原因が HEAD 側の誤りに見える。HEAD の意味は変えない。
- 既存テストの期待値の変更が「既存テストへの影響」の一覧の外で必要になった。
- 既定の wasm32 の出力に import が増えた、または `memcpy`・`memset` の呼び出し（`llvm.memcpy` の lowering を含む）が必要になった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。
- 手順 13 の計測で `benchmarks/strings/cjk_lines` の最大 RSS か `utf/cjk` の時間が、before の最小〜最大の範囲を超えて悪化し、
  D5 の範囲で直せない。種目名や入力に依存する調整はしない。

## 現状と計測（HEAD `f8dc655`）

### 表現

- `string` は「ECMA-262 の String 値モデルに従う、所有する不変の UTF-16 コード単位列」（docs/language.md `## 型とメモリ`）。
  IR の固定ヘッダー（`src/llvm.rs` の `; Tsuzuri - deterministic LLVM IR` で始まる文字列）が `%tz.string = type { ptr, i64 }` を出す。
  欄 0 はデータ、欄 1 はコード単位数で、長さは常に `9007199254740991`（2^53 − 1）以下（`@tz.string.allocate` と `@tz.string.concat` が trap で保証）。
  したがって欄 1 の bit 63 は HEAD では常に 0 である。
- 1 単位は常に `i16`（2 bytes）。`@tz.string.allocate` は `mul i64 %length, 2` bytes を `@tz.alloc` で確保する。空文字列は `zeroinitializer`（null, 0）。
- `utf8string`（`%tz.utf8string`）は妥当な UTF-8 の byte 列で、PM03 は変えない。
- 公開 ABI: `src/abi.rs` の `Buffer::String` は `c_element` が `uint16_t`、`width` が 2。`src/llvm_abi.rs` の `wrapper` は host の `ref string` を
  複製せずに記述子へ包み（長さを `9007199254740991` 以下と検査）、`string` の結果を `%out` の `%tz.abi.buffer` へそのまま書く。
  `src/llvm_imports.rs` の `host_call` は `ref string` の引数を記述子のまま host へ渡し、`read_host_result` は host の結果を UTF-16 として受け取る。

### 2 bytes／単位を仮定している箇所（網羅。設計「段ごとの変更」の元）

| 種別 | ファイル | 関数・箇所 | 仮定 |
| --- | --- | --- | --- |
| 定数 | `src/llvm.rs` | `FunctionEmitter::string_constant` の `StringLiteral::Utf16` | `[N x i16]` の定数 |
| 定数の使用 | `src/llvm.rs` | `expression` の `TypedExprKind::String` | `@tz.string.new(ptr @tz.literal.K, i64 N)` |
| 定数の使用 | `src/llvm_frame.rs` | `emit_frame_inner` の `TypedExprKind::String`、`Frame::String`、`frame_test`、`heap_copy` | 定数を指す記述子（欄 1 = `text.len()`）、欄 1 と `length` の比較、`.new(ptr, i64 length)` による複製 |
| 索引 | `src/llvm.rs` | `expression` の `TypedExprKind::Index(string, index) if string.ty.is_string()` | 欄 1 で境界検査し、`getelementptr inbounds i16` と `load i16` |
| 長さ | `src/llvm.rs` | `expression` の `TypedExprKind::StringLength` | 欄 1 をそのまま返す |
| 演算子 | `src/llvm.rs` | `binary` の `left.ty.is_string()` | `@tz.string.concat`・`equal`・`compare` |
| 複製 | `src/llvm.rs` | `clone_value` の `Type::String \| Type::Utf8String` | `@tz.string.new(ptr, i64 欄1)` |
| builtin | `src/llvm.rs` | builtin の生成で `Builtin::CloneString`・`StringFromUtf8`・`Utf8StringFromString`・`StringIsWellFormed`・`StringToWellFormed` を扱う arm | `(ptr %p, i64 %n)` で runtime へ |
| builtin | `src/llvm_bulk.rs` | `string_buffer_builtin` の `StringToCodeUnits`・`StringFromCodeUnits`・`StringCompare` | `[i16u]` と `string` のバッファを複製せずに移す |
| Display | `src/llvm.rs` | `emit_display` の `Type::String`（借用は `@tz.string.new`）、数値（`@tz.string.from_ascii`）、`Type::Bool`・`Type::Unit`（`[4 x i16]` など） | UTF-16 の結果 |
| Parse | `src/llvm.rs` | `emit_parse` の数値（`@tz.string.to_ascii` の後に `tz_soft_parse(..., i64 %length, ...)`）、`Type::Bool`（`@tz.string.equal` と `{ ptr, i64 4 }`） | 欄 1 を長さとして C へ渡す |
| 出力 | `src/llvm.rs` | console の出力（`Type::String` で `@tz.utf8string.from_string`）、`Debug.print`（`%units = extractvalue %tz.string %text, 1`） | UTF-16 → UTF-8 |
| Hash | `src/llvm_hash.rs` | `hash_primitive` の `ty.is_string()` | 欄 1 を hash し、`i16` の単位を `hash_bits(.., 16, 2)` |
| 引用表示 | `src/llvm_display.rs` | `display_quoted` | `@tz.display.quote(ptr, i64 欄1, i32 kind)` |
| ABI | `src/llvm_abi.rs` | `wrapper`（引数と、`Buffer::of(result)` の結果を `%out` へ書く分岐） | host の UTF-16 をそのまま使い、結果をそのまま渡す |
| ABI | `src/llvm_imports.rs` | `host_call`（`Buffer::of(inner)` の引数）、`read_host_result` | 同上 |
| runtime | `src/runtime/string.ll` | `copy`・`allocate`・`new`・`from_ascii`・`concat`・`equal`・`compare`・`from_utf8`・`decode_utf16`・`utf8string.from_string`・`is_well_formed`・`to_well_formed`・`to_ascii` | `i16` の読み書き、`mul i64 %length, 2` |
| runtime | `src/runtime/display.ll` | `@tz.display.join`・`@tz.display.escape`・`@tz.display.quote` | `i16` の読み書き |
| runtime | `src/runtime/character.ll` | `@tz.character.display`（`[2 x i16]` と `@tz.string.new`）、`@tz.character.parse`（`%tz.string` を読む） | 同上 |
| DWARF | `src/llvm_debug.rs` | `Type::String => sequence(Type::Integer(16, false))` | debugger は `u16` の列として表示 |

`std/String.tz` は `text[index]`・`text.length`・`String.from_code_units` だけで書かれており、表現を直接は仮定しない（D6）。
`drop_value` は欄 0 を `@tz.free` するだけで、表現に依存しない。

### 計測済みの事実

- 2026-09-26（M1 Max、`-O3`）の `cpp/utf16_compare` の相対性能は 0.497 で、最速は JavaScript。V8 は Latin-1 の範囲の文字列を 1 byte で保存する。
- 長さ n の文字列は ASCII でも 2n bytes を確保する（`@tz.string.allocate`）。数値の Display も `@tz.string.from_ascii` で 1 桁 2 bytes を確保する。
- メモリ量（最大 RSS・確保量）の文字列 workload の記録はない。実装手順 1 で PX01 形式の before を取る。

### 再現

probe `/tmp/tz-pm03/literal/Main.tz`（2026-09-29 に `tsuzuri run` で `12` を確認）:

```tsuzuri
def probe :: i64 -> i64
fn probe n =
    let text = "hello"
    let wide = "h\u00e9llo\u3042"
    text.length + wide.length + n

probe 1
```

次のコマンドで、`@tz.literal.0 = private unnamed_addr constant [5 x i16] [i16 104, ...]`、`[6 x i16]` の `@tz.literal.1`、
定数を指す記述子（`insertvalue %tz.string zeroinitializer, ptr @tz.literal.0, 0`。`llvm_frame.rs` の frame の経路）が出る。
PM03 の後は `@tz.literal.0` が `[5 x i8] c"\68\65\6C\6C\6F"`、欄 1 が `-9223372036854775803` になり、`@tz.literal.1`（U+3042 を含む）は変わらない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri run /tmp/tz-pm03/literal
target/release/tsuzuri build /tmp/tz-pm03/literal --emit llvm -o /tmp/tz-pm03/literal.ll
grep -n "x i16\] \[\|x i8\] c\|ptr @tz.literal" /tmp/tz-pm03/literal.ll
```

## 目標と指標

目標は CI の合否条件にしない。専用の計測機で before と after を PX01 形式で記録して判断する（GUIDE §14）。

- G1: 全単位が 0xFF 以下の文字列のバッファを n bytes（HEAD は 2n）にする。ASCII 中心の workload で文字列の確保量を約半分にする。
- G2: 非 Latin-1 のテキスト（CJK、絵文字、孤立 surrogate）で確保量・最大 RSS・時間を悪化させない。
- G3: 文字列の種目の時間を悪化させない。Latin-1 同士の比較・連結は速くなる見込み（計測で確かめる）。

| 指標 | 単位・統計 | 対象 | 期待（計算値。計測で確かめる） |
| --- | --- | --- | --- |
| M1 確保量 | bytes・回（1 回の実行の合計。決定的） | PX01 の tracked: `cpp/utf16_scan`・`utf16_compare`・`utf16_validate`・`utf8_roundtrip`・`format_parse` | seed が偶数なら `make_text` は ASCII（`"Az09-_ \n"` の倍増）で、文字列の bytes が約半分。奇数（U+03A9 などを含む）は変化なし |
| M2 最大 RSS | KiB（9 標本の中央値、最小、最大） | `benchmarks/strings/*` の `execute`（native `-O3`） | `ascii_words`・`latin1_csv` は文字列バッファ分が半分。`cjk_lines` は変化なし |
| M3 時間 | ms（9 標本の中央値、最小、最大） | M1 の cpp 5 種目、PR05 の `utf/*` | Latin-1 の入力は改善か変化なし。`mixed`・`cjk`・`emoji` は広がりの範囲内 |
| M4 大きさ | bytes（固定値） | `benchmarks/strings/*` の `executable_bytes`、`wasm_bytes` | runtime の関数が増える分だけ増える（報告のみ） |

## 変えてはいけない意味

- 値: コード単位の列、`.length`、`text[i]`（型 `i16u`）、`String.to_code_units` の配列、`String.from_code_units` の結果、std の全関数の結果。
- 比較: `==`・`<` などは符号なしの単位値による辞書順で、短い方が先。`String.compare` は HEAD と同じ `-1`・`0`・`1` だけを返す（値が観測できる）。
- Hash: canonical な入力列（tag `0x53`、長さ、各単位を 16 bit・2 bytes として `hash_bits`）を byte 単位で同じにする。形式の違う同じ内容は同じ hash になる。
- 出力: `Display`・`to_string`・console 出力・`Debug.print`・引用表示の byte 列。孤立 surrogate の保持と UTF-8 出力時の trap。
- trap: 境界（`BoundsCheck`）、長さの上限 2^53 − 1（`StringConcatOverflow`・`AllocationSize`、`llvm.trap`）の条件と種類。`tests/strings_runtime.c` の trap 数 28。
- 評価順序、所有権と借用（どの式が値を消費するか）、`Copy` でないこと。確保の回数は観測できる意味ではないが、`live == 0` は保つ。
- 公開 ABI（`const uint16_t *` と単位数、`tsuzuri_string_buffer`）と E13 の記述子。
- IR の決定性（形式の選択は単位の値だけで決まる）、既定の wasm32 が import を持たないこと（D-18）、runtime の宣言の重複がないこと。

## 設計

### 表現（D1・D4）

- `%tz.string = type { ptr, i64 }` は変えない。欄 1 を「長さ語」と呼ぶ。bit 63 が Latin-1 の印、bit 0–62 が単位数 n（2^53 − 1 以下）。
- 印 0（UTF-16 形式）: HEAD と同じ。データは n 個の `i16`。
- 印 1（Latin-1 形式）: データは n 個の `i8` で、各 byte がそのまま単位の値（0–255）。n ≥ 1。
- 空文字列は `zeroinitializer`（印 0）だけ。Latin-1 を作る関数は n = 0 なら `zeroinitializer` を返す。
- 正準形を要求しない（D4）。UTF-16 形式が 0xFF 以下の単位だけを持ってもよい（host の借用入力、`Bool` の Display など）。全消費側は両形式を受ける。
- IR での分解: `%n = and i64 %w, 9223372036854775807`、`%latin1 = icmp slt i64 %w, 0`。Latin-1 の定数語は `(n as i64) | i64::MIN` を
  10 進で出す（n = 5 なら `-9223372036854775803`）。

### データ構造

```rust
// src/llvm.rs（すべて新規）
pub(super) const STRING_LATIN1: u64 = 1 << 63;
pub(super) const STRING_LENGTH_MASK: u64 = STRING_LATIN1 - 1; // 9223372036854775807
/// 空でなく、全単位が 0xFF 以下のときだけ byte 列を返す。
pub(super) fn latin1_units(units: &[u16]) -> Option<Vec<u8>>;
/// 長さ語の IR 定数（符号付き 10 進）。
pub(super) fn string_word(length: usize, latin1: bool) -> String;
/// リテラルの長さ語。`StringLiteral::Utf8` は `text.len()` のまま。
pub(super) fn literal_word(text: &StringLiteral) -> String;
impl FunctionEmitter<'_, '_> {
    /// 長さ語から (単位数, Latin-1 かの i1) を出す。
    fn string_parts(&mut self, word: &str) -> (String, String);
    /// 境界検査済みの index の単位を i16 で読む（分岐と phi）。
    fn string_unit(&mut self, data: &str, latin1: &str, index: &str) -> String;
}
// src/llvm_frame.rs: Frame::String { data: String, length: usize, latin1: bool }  // latin1 を追加
```

### runtime 関数（D2）

規約: `string` のデータを受ける runtime 関数は、長さ語を加工せずに `i64` で受け、関数の中で分解する。既存の名前は形式で振り分ける
入口（dispatcher）として残し、生成側の呼び出しは変えない。PR05 の UTF-16 の本体は `string_scalar.ll` と `string_v128.ll` の両方で
名前に `.utf16` を付けて改名する（本文は変えない）。新しい Latin-1・混在の kernel は `src/runtime/string_latin1.ll`（新規、スカラー）に置き、
`string.ll` を連結する条件と同じ条件で連結する。

| 関数 | 置き場所 | 引数 | Phase 1 の動作 |
| --- | --- | --- | --- |
| `@tz.string.allocate` | `string.ll` | n | 変更なし（UTF-16 を 2n bytes。guard harness の期待を保つ） |
| `@tz.string.allocate_latin1`（新規） | `string_latin1.ll` | n | n = 0 は `zeroinitializer`。n > 2^53 − 1 は `llvm.trap`。n bytes を確保し、語は n \| bit 63 |
| `@tz.string.copy_bytes`・`@tz.string.widen`・`@tz.string.narrow`（新規） | `string_latin1.ll` | (to, from, n) | `i8`→`i8`、`i8`→`zext`→`i16`、`i16`→`trunc`→`i8` の loop。`llvm.memcpy` は使わない |
| `@tz.string.new` | `string.ll` | (ptr, 語) | Latin-1 は `allocate_latin1` + `copy_bytes`。UTF-16 は HEAD |
| `@tz.string.from_ascii` | `string.ll` | (ptr, n) | Latin-1 を作る（`allocate_latin1` + `copy_bytes`）。PR05 の本体は `@tz.string.from_ascii.utf16` |
| `@tz.string.concat` | `string.ll` | (記述子, 記述子) | 「アルゴリズム」の concat |
| `@tz.string.equal`・`@tz.string.compare` | `string.ll` | (記述子, 記述子) | 形式の組で `.utf16`（PR05）、`equal_latin1`・`compare_latin1`、`equal_mixed`・`compare_mixed`（新規、`string_latin1.ll`）へ |
| `@tz.utf8string.from_string` | `string.ll` | (ptr, 語) | Latin-1 は `@tz.utf8string.from_latin1`（新規）、UTF-16 は `.utf16` |
| `@tz.string.is_well_formed`・`@tz.string.to_well_formed` | `string.ll` | (ptr, 語) | Latin-1 は `true`・`@tz.string.new(ptr, 語)`。UTF-16 は `.utf16` |
| `@tz.string.to_ascii` | `string.ll` | (dst, ptr, 語) | Latin-1 の分岐を足す（各 byte < 0x80。長さの検査は HEAD と同じ単位数で） |
| `@tz.string.from_utf8` | PR05 の 2 ファイル | (ptr, bytes) | 変更なし。全 ASCII の経路が `@tz.string.from_ascii` を呼ぶので Latin-1 になる（D3） |
| `@tz.string.to_utf16`（新規） | `string_latin1.ll` | 記述子（消費） | UTF-16 はそのまま。Latin-1 は `from_ascii.utf16(data, n)` の後に `@tz.free(data)` |
| `@tz.string.borrow_utf16`（新規） | `string_latin1.ll` | 記述子（借用） | UTF-16 はそのまま。Latin-1 は `from_ascii.utf16(data, n)`（呼び出し側が解放） |
| `@tz.string.from_units`（新規） | `string_latin1.ll` | (ptr, n)（`[i16u]` を消費） | 「アルゴリズム」の from_units（D6） |
| `@tz.display.join` | `display.ll` | (配列, kind) | 全部分が Latin-1 か空なら Latin-1、それ以外は UTF-16（Latin-1 の部分は `widen`） |
| `@tz.display.quote` | `display.ll` | (ptr, 語, kind) | 入力が Latin-1 なら出力も Latin-1。単位は `zext` して `@tz.display.escape` へ |
| `@tz.character.display`・`@tz.character.parse` | `character.ll` | i32・ptr | 0xFF 以下は 1 単位の Latin-1。parse は語を分解して読む |
| `@tz.string.copy`・`decode_utf16`・`decode_utf8`・`try_decode_utf8`・`utf8string.*` | 各所 | — | 変更なし（UTF-16 専用か、`string` を受けない） |

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 定数 | `src/llvm.rs` | `string_constant` | `latin1_units` が `Some` なら `[N x i8] c"..."`（`StringLiteral::Utf8` の分岐と同じ `\XX` の書式） |
| 定数 | `src/llvm.rs` | `expression` の `TypedExprKind::String` | `.new(ptr {name}, i64 {literal_word})` |
| frame | `src/llvm_frame.rs` | `emit_frame_inner`、`Frame::String`、`frame_test`、`heap_copy` | 記述子の欄 1 と `frame_test` の比較値と `heap_copy` の引数を `string_word(length, latin1)` にする。`Frame::String { .. } => {}` の arm は変更なし |
| 索引 | `src/llvm.rs` | `expression` の `TypedExprKind::Index(string, index) if string.ty.is_string()` | `string_parts` で境界検査、`string_unit` で読む（`utf8string` は従来どおり） |
| 長さ | `src/llvm.rs` | `TypedExprKind::StringLength` | `Type::String` のとき `and i64 {語}, 9223372036854775807` |
| 演算子・複製 | `src/llvm.rs` | `binary`、`clone_value`、builtin の `CloneString` などの arm | 変更なし（語をそのまま runtime へ渡す） |
| builtin | `src/llvm_bulk.rs` | `string_buffer_builtin` | `StringToCodeUnits` は入力を先に `@tz.string.to_utf16` へ通す。`StringFromCodeUnits` は既存の guard の後、`insertvalue` の代わりに `@tz.string.from_units(ptr, i64)` |
| Display | `src/llvm.rs` | `emit_display` | 変更なし（数値は `from_ascii` が Latin-1 を返す。`Bool`・`Unit` の定数は UTF-16 のまま、D8） |
| Parse | `src/llvm.rs` | `emit_parse` の数値 | `%length`（語）は `to_ascii` へそのまま渡し、`%units = and i64 %length, 9223372036854775807` を `tz_soft_parse` へ渡す |
| 出力 | `src/llvm.rs` | console の出力、`Debug.print` | 変更なし（`from_string` の dispatcher） |
| Hash | `src/llvm_hash.rs` | `hash_primitive` | `Type::String` は `string_parts` の単位数を hash し、`string_unit` の i16 を `hash_bits(.., 16, 2)`。`Utf8String` は変更なし |
| 引用表示 | `src/llvm_display.rs` | `display_quoted` | 変更なし（語を `quote` へ渡す。`Char`・`Utf8Char` は UTF-16 の一時領域で印 0） |
| ABI | `src/llvm_abi.rs` | `wrapper` | 引数は変更なし（host の長さは 2^53 − 1 以下と検査済みで印 0）。`Buffer::of(result)` の分岐で `*result == Type::String` なら先に `@tz.string.to_utf16` |
| ABI | `src/llvm_imports.rs` | `host_call` | `inner == Type::String` の引数は `@tz.string.borrow_utf16` の結果を渡し、呼び出しの後、元が Latin-1 のときだけそのデータを `@tz.free`（分岐）。`read_host_result` は変更なし |
| DWARF | `src/llvm_debug.rs` | `Type::String` の記述 | 変更なし（D9） |
| 連結 | `src/llvm.rs` | runtime の連結（`string.ll` を足す条件） | `string_latin1.ll` を同じ条件で足す |
| runtime | `src/runtime/*.ll` | 「runtime 関数」の表 | 表のとおり |
| std | `std/String.tz` | 全関数 | 変更なし（`from_code_units` が狭めるので `slice`・`split`・`trim` なども Latin-1 を返す） |

### 生成 IR とランタイム

索引（`text[i]`）の形。`utf8string` と配列の索引は変えない。

```llvm
  %w = extractvalue %tz.string %s, 1
  %n = and i64 %w, 9223372036854775807
  %valid = icmp ult i64 %i, %n                ; 失敗は従来どおり BoundsCheck の guard
  %latin1 = icmp slt i64 %w, 0
  br i1 %latin1, label %narrow, label %wide
narrow:
  %p8 = getelementptr inbounds i8, ptr %data, i64 %i
  %b = load i8, ptr %p8
  %u8 = zext i8 %b to i16
  br label %done
wide:
  %p16 = getelementptr inbounds i16, ptr %data, i64 %i
  %u16 = load i16, ptr %p16
  br label %done
done:
  %unit = phi i16 [ %u8, %narrow ], [ %u16, %wide ]
```

リテラル `"hello"` は `@tz.literal.K = private unnamed_addr constant [5 x i8] c"\68\65\6C\6C\6F"` と
`insertvalue %tz.string %v, i64 -9223372036854775803, 1` になる。形式は単位の値だけで決まるので IR は決定的。

### アルゴリズム

```text
concat(l, r):                       ; (ln, la) = parts(l.w)、(rn, ra) = parts(r.w)
  n = ln + rn                       ; HEAD と同じ溢れの trap。上限は allocate / allocate_latin1 が検査
  if (la or ln == 0) and (ra or rn == 0) and n > 0:
      out = allocate_latin1(n); copy_bytes(out, l.data, ln); copy_bytes(out + ln, r.data, rn)
  else:
      out = allocate(n)             ; UTF-16
      各側: Latin-1 なら widen(dst, data, len)、UTF-16 なら copy(dst, data, len)

equal(l, r):  ln != rn -> false
  両方 UTF-16 -> equal.utf16(l, r)、両方 Latin-1 -> equal_latin1(a, b, n)
  混在 -> equal_mixed(latin1 側, utf16 側, n)      ; byte を zext して i16 で比較。確保しない
compare(l, r):  同じ振り分け。左が UTF-16、右が Latin-1 なら 0 - compare_mixed(r, l)
  全版は HEAD と同じく -1 / 0 / 1 を返す（最初の違う単位を符号なしで比べ、なければ短い方が小さい）

from_units(data, n):                ; [i16u] の所有バッファを消費
  n == 0 -> zeroinitializer
  どれかの単位 > 0xFF -> { data, n }         ; UTF-16。複製しない（HEAD と同じ）
  out = allocate_latin1(n); narrow(out, data, n); free(data); out

hash(s):  state = word(OFFSET, 0x53); state = word(state, n)       ; n は分解後の単位数
  各 i: state = hash_bits(state, unit_i を i16 で, 16, 2)       ; 形式によらず同じ列
```

### Phase 分割

- Phase 1（このチケットの実装範囲）: 表現、上の全 runtime 関数（スカラー）、生成の全箇所、差分テスト、計測。単独で出荷できる。
- Phase 2（人間が求めた場合だけ）: `equal_latin1`・`compare_latin1`・`equal_mixed`・`compare_mixed`・`widen`・`narrow`・`from_units` の走査・
  `utf8string.from_latin1` の 128-bit ベクトル版を PR05 の `string_v128.ll` に、スカラー版を `string_scalar.ll` に移す（PR05 の D1・D2 の
  選択規則に従う）。`from_utf8` の計数関数に最大の scalar を返させ、非 ASCII の Latin-1 入力も Latin-1 にする。PR05 の差分 harness を広げる。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、`running N tests` の N を必ず見る（GUIDE §3.1）。
手順 4・5 は消費側だけを変え、Latin-1 形式はまだ作らない。ここで観測結果が一つでも変われば停止条件である。

### 手順 1: ベースライン

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、基準のコンパイラを保存する。「再現」の probe を置き、IR を保存する。
  `grep -n "call %tz.string @tz.string.from_ascii" src/runtime/string_scalar.ll src/runtime/string_v128.ll` で、PR05 の `from_utf8` の
  全 ASCII の経路が `from_ascii` を呼ぶことを確かめる（呼ばなければ手順 7 で両ファイルのその経路を `from_ascii` の呼び出しにする）。
- 確認: 次がすべて成功し、`run` は `12` を出す。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked && cp target/release/tsuzuri /tmp/tz-pm03/tsuzuri-before
target/release/tsuzuri run /tmp/tz-pm03/literal
target/release/tsuzuri build /tmp/tz-pm03/literal --emit llvm -o /tmp/tz-pm03/before.ll
target/release/tsuzuri build /tmp/tz-pm03/literal --target wasm32 --emit llvm -O3 -o /tmp/tz-pm03/before-wasm-O3.ll
cargo test --locked --test strings
node tests/strings.mjs target/release/tsuzuri
```

### 手順 2: 計測 workload と before

- 変更: `benchmarks/strings/ascii_words/Main.tz`・`benchmarks/strings/latin1_csv/Main.tz`・`benchmarks/strings/cjk_lines/Main.tz`（新規、D10）。
- 内容: 各 project はトップレベルの結果式で合計を表示する（probe と同じ形）。本体は実装時に書き、`tsuzuri check` と `run` で確かめる。
  - `ascii_words`: i = 0…199,999 の `"word-" + to_string i` を `Vec<string>` に持ち続け、最後に全要素の `.length` の和を返す。期待 `2088890`。
  - `latin1_csv`: 同じ i の行 `"caf\u00e9," + to_string i + ",na\u00efve"` を `String.join` で `"\n"` 区切りの 1 つの文字列にし、
    `String.split` で行と `","` の欄に分け、全欄を持ち続けて `.length` の和を返す。期待 `2888890`。
  - `cjk_lines`: `latin1_csv` の `café`・`naïve` を `\u6771\u4eac`・`\u5927\u962a` に替えたもの。期待 `1888890`。
  - 期待値の独立の参照: `python3 -c 'print(sum(len(f"word-{i}") for i in range(200000)), sum(9+len(str(i)) for i in range(200000)), sum(4+len(str(i)) for i in range(200000)))'`。
  - 「計測手順」の before を `/tmp/tz-pm03/tsuzuri-before` で取る。
- 確認: 3 つの `tsuzuri run` が期待値を出す。`target/perf/PM03-before/` に `cpp.jsonl`、`utf.jsonl`、process の jsonl がある。

### 手順 3: Latin-1 の kernel（まだ呼ばない）

- 変更: `src/runtime/string_latin1.ll`（新規）、`src/llvm.rs` の runtime の連結（`include_str!("runtime/string.ll")` を足す `if` の中）。
- 内容: 「runtime 関数」の表の新規 12 関数（`allocate_latin1`、`copy_bytes`、`widen`、`narrow`、`equal_latin1`、`equal_mixed`、`compare_latin1`、
  `compare_mixed`、`utf8string.from_latin1`、`to_utf16`、`borrow_utf16`、`from_units`）を `define internal ... nounwind` で書く。loop は `@tz.string.copy` と
  同じ形（phi と 1 単位ずつ）にし、`equal_latin1` だけ `@tz.string.equal` の `wide` と同じ 8 bytes 単位の比較を持つ。`to_utf16`・`borrow_utf16` は
  `@tz.string.from_ascii.utf16` を呼ぶので、この手順では `@tz.string.from_ascii` を呼び、手順 4 で差し替える。
- 確認: `grep -c "^define internal" src/runtime/string_latin1.ll` が `12`。`cargo test --locked --test strings` と
  `node tests/strings.mjs target/release/tsuzuri`（`cargo build --release --locked` の後）が成功し、宣言の重複の検査も通る。

### 手順 4: runtime の消費側（dispatcher）

- 変更: `src/runtime/string_scalar.ll`・`src/runtime/string_v128.ll`（改名だけ）、`src/runtime/string.ll`、`src/runtime/display.ll`、`src/runtime/character.ll`、
  `tests/strings.mjs` の guard harness、PR05 の `tests/strings_simd.c` の名前の列。
- 内容: PR05 の `from_ascii`・`equal`・`compare`・`utf8string.from_string`・`is_well_formed`・`to_well_formed` を両ファイルで `.utf16` 付きに改名し、
  `string.ll` に元の名前の dispatcher を置く。`from_ascii` の dispatcher はこの手順では `from_ascii.utf16` を呼ぶだけ（まだ UTF-16 を作る）。
  `new`・`concat`・`to_ascii`・`display.join`・`display.quote`・`character.parse` は長さ語を分解して両形式を読む。出力の形式の規則（Latin-1 を作る）は
  まだ入れない（`concat` は常に UTF-16 で出し、Latin-1 の側は `widen`）。guard harness は `runtimeGuards` が抜き出す関数に `concat` が呼ぶ
  `allocate_latin1`・`copy_bytes`・`widen` を加える（`copy_bytes`・`widen` は `@tz.string.copy` と同じく何もしない stub にする）。`guardCases`・`guardTraps` は変えない。
- 確認: `cargo build --release --locked` の後、`node tests/strings.mjs target/release/tsuzuri` が HEAD と同じ行（`4344 bounded batches and 28 strict ... traps`）を出す。
  PR05 の差分 harness が成功する。`target/release/tsuzuri run /tmp/tz-pm03/literal` が `12`。

### 手順 5: 生成の消費側

- 変更: `src/llvm.rs`（定数 2 つ、`string_word`、`string_parts`、`string_unit`、`TypedExprKind::Index` の文字列の arm、`StringLength`、`emit_parse`）、
  `src/llvm_hash.rs` の `hash_primitive`、`src/llvm_bulk.rs` の `StringToCodeUnits`、`src/llvm_abi.rs` の `wrapper`、`src/llvm_imports.rs` の `host_call`。
- 内容: 「段ごとの変更」の該当行。`StringFromCodeUnits` とリテラルはまだ変えない。`emit_parse` は `%length` を語の名前のまま `to_ascii` へ渡し、
  `%units` を `tz_soft_parse` へ渡す（`src/llvm.rs` の既存テストの `call i1 @tz.string.to_ascii(ptr %buffer, ptr %data, i64 %length)` を保つ）。
- 確認: `cargo test --locked` が成功する。`cargo build --release --locked` の後、`node tests/strings.mjs`・`node tests/primitives.mjs`・
  `node tests/features.mjs`（引数はいずれも `target/release/tsuzuri`）と GUIDE §3.1 の E05・Display・Parse の行の suite が成功する。

### 手順 6: 生成側 1（リテラル）

- 変更: `src/llvm.rs` の `latin1_units`・`literal_word`・`string_constant`・`TypedExprKind::String`、`src/llvm_frame.rs` の `Frame::String` と 4 箇所。
- 内容: 「段ごとの変更」の定数と frame の行。
- 確認: probe が `12` を出し、IR に `[5 x i8] c"\68\65\6C\6C\6F"` と `i64 -9223372036854775803` があり、`@tz.literal.1` は `[6 x i16]` のまま。
  `cargo test --locked` と手順 5 の suite が成功する。

### 手順 7: 生成側 2（runtime の出力規則）

- 変更: `src/runtime/string.ll`（`from_ascii`、`concat`）、`src/runtime/display.ll`、`src/runtime/character.ll`（`display`）、`src/llvm_bulk.rs` の
  `StringFromCodeUnits`、`tests/strings.mjs` の `runtimeBridges`。
- 内容: D3 の生成規則を入れる。`runtimeBridges` の `decode` は `@tz.string.from_utf8` の結果を `@tz.string.to_utf16` に通してから
  data と長さを返す（`tests/strings_runtime.c` は `uint16_t` で読むため）。
- 確認: 手順 5 の全 suite が成功し、`tests/strings.mjs` の `4344`・`28`・`live` が HEAD と同じ。

### 手順 8: Rust テスト

- 変更: `tests/strings.rs`、`src/llvm.rs` の test module。
- 内容: 「テスト計画」の Rust テスト。
- 確認: `cargo test --locked --test strings` が手順 1 の件数 + 5。`cargo test --locked --lib latin1` が `2 passed`。

### 手順 9: E2E

- 変更: `tests/compact_strings.mjs`、`tests/fixtures/compact_strings/Main.tz`（新規）。
- 内容: 「テスト計画」の E2E。`tests/strings.mjs` の native host（`tracked_alloc`、`live`）と WASM の読み込みを写す。
- 確認: `node tests/compact_strings.mjs target/release/tsuzuri` が `-O0`・`-O3` のそれぞれで `compact_strings O<n>: <N> cases, 2 traps on native/WASM` を出す。
  `TSUZURI_BASELINE=/tmp/tz-pm03/tsuzuri-before node tests/compact_strings.mjs target/release/tsuzuri` も成功する（基準との差分）。

### 手順 10: 全体の確認

- 変更: なし。
- 内容: GUIDE §3 の全検証。重い BigInt の suite は Node 24（`npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri`）。
- 確認: `cargo test --locked`、`cargo clippy --locked --all-targets -- -D warnings`、GUIDE §3 の全 E2E が成功する。

### 手順 11: 生成コードの確認

- 変更: なし。
- 内容・確認: 「生成コードの確認」の全項目。結果を完了報告に貼る。

### 手順 12: 計測

- 変更: なし。
- 内容・確認: 「計測手順」の after と before2。`report` の表を完了報告に貼る。M1 の偶数 seed の種目で bytes が減っていなければ、
  生成側のどこかが UTF-16 を作っている（落とし穴 13）。

### 手順 13: 文書

- 変更: 「ドキュメント」の表のファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md docs/benchmarks.md` と `git diff --check` が成功する。

## 計測手順

GUIDE §14 と PX01 の「before／after」に従う。数字は 9 標本の中央値と最小・最大（PX01 が集計）。生データは `target/perf/PM03-*` に残し Git に加えない。

### 環境

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
sysctl -n machdep.cpu.brand_string; sw_vers; clang --version | head -1; rustc --version; node --version; git rev-parse HEAD
```

### before・after・before2

`C` を before・before2 では `/tmp/tz-pm03/tsuzuri-before`、after では `target/release/tsuzuri` にし、`R` を `PM03-before`・`PM03-after`・
`PM03-before2` にして、この順に 3 回測る。旗の名前が PX01 の実装と違えば、前提の節とこの節を直してから測る。

```sh
node benchmarks/run-cpp.mjs "$C" --metrics target/perf/$R
node benchmarks/run-utf.mjs "$C" --metrics target/perf/$R
node benchmarks/metrics.mjs process "$C" --out target/perf/$R \
  --project benchmarks/strings/ascii_words --project benchmarks/strings/latin1_csv --project benchmarks/strings/cjk_lines
node benchmarks/metrics.mjs report target/perf/PM03-before2 --baseline target/perf/PM03-before
node benchmarks/metrics.mjs report target/perf/PM03-after --baseline target/perf/PM03-before
```

- before2 と before の比較で時間系に `改善`・`悪化` が出た metric は「判定できない」と報告し、改善を主張しない。
- M1 は決定的なので 1 標本でよい。偶数 seed の種目は文字列の bytes が約半分、奇数 seed は同じであることを確かめる。
- 確保量の差には `from_units` の狭め（D6）と `to_code_units` の広げの分を含む。内訳が要るときは「生成コードの確認」の IR で呼び出しを数える。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for t in native wasm32; do for o in 0 3; do
  target/release/tsuzuri build /tmp/tz-pm03/literal $([ $t = wasm32 ] && echo --target wasm32) --emit llvm -O$o -o /tmp/tz-pm03/after-$t-O$o.ll
done; done
grep -c 'x i8\] c"\\68\\65\\6C\\6C\\6F"' /tmp/tz-pm03/after-*-O0.ll
grep -c 'x i16\] \[i16 104, i16 233' /tmp/tz-pm03/after-*-O0.ll
grep -c '@memcpy\|@memset\|llvm.memcpy' /tmp/tz-pm03/after-wasm32-O3.ll
grep -c '^define internal %tz.string @tz.string.allocate_latin1(' /tmp/tz-pm03/after-native-O0.ll
```

- 期待: 1 行目と 2 行目の grep は各ファイル 1、3 行目は `0`（手順 1 の `before-wasm-O3.ll` も 0 であることを確かめる）、4 行目は `1`（重複なし）。
- 同じコマンドを 2 回実行し、`cmp` で IR が同一なこと（決定性）。
- `-O3` の `String.find` の本体（E2E の fixture を `--emit llvm -O3`）で、loop の中に `icmp slt i64 ..., 0` が残るか（形式の分岐が loop の外へ
  出たか）を記録する。報告だけで合否にしない（D5）。

## テスト計画

### 共通の源

- 独立の参照は JavaScript の文字列（ECMA-262 の UTF-16 コード単位の列で、`string` と同じモデル）。長さは `length`、単位は `charCodeAt`、
  順序は `<`、`isWellFormed()`、`Buffer.byteLength(s, "utf8")`、`indexOf`、`slice`、`split`・`join`。期待値を現在のコンパイラの出力から作らない。
- 形式の不変性: host の `ref string` は UTF-16 形式で届く。fixture の `canon`（`String.from_code_units (String.to_code_units (clone_string text))`）は
  同じ内容を Latin-1 形式（該当すれば）にする。各操作を UTF-16・Latin-1 の組み合わせで実行し、結果が一致することを確かめる。
- 乱数の入力: xorshift32（seed `0x9e3779b9`）で 1,000 個。長さは 60% が 0–16、30% が 17–64、10% が 65–600。各文字列は次の区分のいずれか:
  ASCII（0x00–0x7F）、Latin-1（0x00–0xFF、半分以上が 0x80–0xFF）、BMP（0x100–0xD7FF と 0xE000–0xFFFF）、surrogate（対、孤立の上位、孤立の下位）、
  混在（Latin-1 の列の先頭・中間・末尾のどこか 1 単位だけが 0x100 以上）。二項の操作は `(s[i], s[i+1])`、`(s, s)`、`(s, s + 一単位)`、接頭辞の組。
- 境界の入力: `""`、`"\u00ff"`、`"\u0100"`、`"\u00ff\u0100"`、`"\u0000"`、`"\u007f\u0080\u009f"`、ASCII 4096 単位、Latin-1 4097 単位、数字だけの列。

### Rust テスト

`tests/strings.rs`（`accepts` は native と wasm32 の IR を 2 回ずつ出して一致を見る）に足す。

| テスト（新規） | 検査 |
| --- | --- |
| `latin1_literals_use_byte_constants_and_flagged_words` | `"hello"` が `[5 x i8] c"\68\65\6C\6C\6F"` と `i64 -9223372036854775803`。`"h\u00e9"` が `[2 x i8] c"\68\E9"`。`"\u0100"` は `[1 x i16] [i16 256]`。`u8"hello"` は従来どおり `i64 5` |
| `string_index_and_length_mask_the_latin1_bit` | `fn unit_at text index = text[index]` の IR に `and i64` と `9223372036854775807`、`load i8`、`load i16` がある。`utf8string` 版には `9223372036854775807` がない |
| `string_hash_uses_masked_length_and_widened_units` | `string` の Hash の関数に `and i64` と `zext i8` がある |
| `code_unit_conversions_narrow_and_widen` | `String.from_code_units` が `@tz.string.from_units(`、`String.to_code_units` が `@tz.string.to_utf16(` を呼ぶ |
| `host_abi_widens_latin1_results_and_import_arguments` | `string` を返す export が `@tz.string.to_utf16(`、`ref string` を受ける型付き import の呼び出しが `@tz.string.borrow_utf16(` を含む |

`src/llvm.rs` の test module に足す: `latin1_units_accepts_only_nonempty_byte_ranges`（`&[]`→`None`、`&[0x41]`→`Some`、`&[0xFF]`→`Some(vec![255])`、
`&[0x100]`→`None`、`&[0x41, 0xD800]`→`None`）、`latin1_string_words_are_signed_decimal`（`string_word(5, true)` が `"-9223372036854775803"`、
`string_word(5, false)` が `"5"`、`string_word(9007199254740991, true)` が `"-9214364837600034817"`）。

### E2E

`tests/compact_strings.mjs`（新規）と `tests/fixtures/compact_strings/Main.tz`（新規）。`tests/strings.mjs` と同じく fixture を一時 root へ複製し、
native（生成した C host、`tracked_alloc` で各 case の後 `live == 0`）と wasm32（`WebAssembly.Module.imports` が空、memory が 16 MiB 以下）を
`-O0`・`-O3` で実行する。fixture の関数はすべて `export def`。本体は実装時に書き `tsuzuri check` で確かめる。

| 関数（新規） | 型 | 内容 | 期待（参照） |
| --- | --- | --- | --- |
| `unit_sum` | `ref string -> i64` | `canon text` の Σ (i + 1)·unit_i | JS の `charCodeAt` で同じ和 |
| `same_forms` | `ref string -> i64` | UTF-16 形式と `canon` の間で: 長さ、全単位、`==`（両向き）、`String.compare` = 0（両向き）、Hash、配列の Display（引用表示）、`String.is_well_formed` の一致を bit で返す | 全 bit が 1 |
| `equal_forms`・`compare_forms` | `ref string -> ref string -> i64` | UTF-16・Latin-1 の 4 組の `==`（bit）と `String.compare`（Σ (c + 1)·3^k） | JS の `===` と `<` |
| `concat_forms` | `ref string -> ref string -> string` | 4 組の連結を順に連結 | `(a + b).repeat(4)` |
| `slice_at` | `ref string -> i64 -> i64 -> string` | `String.slice` を `canon text` に | `text.slice(first, last)`（範囲内のみ） |
| `split_join` | `ref string -> ref string -> string` | `String.join sep (String.split sep (canon text))` | `text` |
| `find_forms` | `ref string -> ref string -> i64` | `String.find` を `canon` 同士で。`None` は `-1` | `indexOf` |
| `utf8_length` | `ref string -> i64` | `Utf8String.from_string` の `.length`（well-formed の入力だけ） | `Buffer.byteLength` |
| `digits_concat` | `i64 -> i64` | 2 つの `to_string n` を連結した `.length` | `2 * String(n).length`。native で `n = 123456789` の tracked bytes が `36`（HEAD は `72`） |
| `parse_digits` | `ref string -> i64` | `canon text` を `Parse` で `i64` に。失敗は `-1` | 数字だけの入力を JS `BigInt` |
| `unit_at` | `ref string -> i64 -> i64` | `(canon text)[index]` | `charCodeAt`。`index = length` と `-1` は trap（native は終了コード、WASM は例外） |
| `hash_value` | `ref string -> i64` | `canon text` の Hash | `TSUZURI_BASELINE` のときだけ、基準コンパイラの結果と一致 |

`TSUZURI_BASELINE=<compiler>` があれば、同じ fixture を基準コンパイラでも native `-O0` で构築し、全関数・全入力の結果が一致することを確かめる
（`digits_concat` の bytes だけは除く）。変数がなければこの段を skip し、その旨を 1 行出す。

### 既存テストへの影響

- `tests/strings.mjs`: guard harness の抜き出し対象と stub（手順 4）、`runtimeBridges` の `decode`（手順 7）。期待値（`guardCases`、`guardTraps`、
  `expected * 2n`、`4344`、`28`、`live == 0`）は変えない。
- PR05 の `tests/strings_simd.c`: 6 関数の名前を `.utf16` 付きにする。期待値は変えない。
- `src/llvm.rs` の IR テスト（`[4 x i16]` の `true`、`[3 x i16] [i16 65, i16 55357, i16 56832]`、`to_ascii(ptr %buffer, ptr %data, i64 %length)`）: 変化なし（D8 と手順 5）。
- それ以外: なし。

### 性能

「計測手順」のとおり。共有 CI に速度・メモリの合否を足さない。

## ドキュメント

| ファイル | 節 | 内容 |
| --- | --- | --- |
| `docs/language.md` | `## 型とメモリ` | `string` の格納形式は実装の詳細で、単位列・長さ・索引・比較・Hash・公開 ABI（UTF-16）は形式によらないこと |
| `docs/architecture.md` | `string` の表現（`Type::String` と `Type::Utf8String` の表現、`i16` の単位）を説明している節 | 長さ語の bit 63、両形式、runtime の規約と dispatcher、生成規則（D3）、ABI の境界の変換とその確保、DWARF の制限（D9） |
| `docs/benchmarks.md` | PX01 の `## 性能指標の記録` | PM03 の before／after（M1〜M4）と環境。計測しなかったものは書かない |
| `_perfs/README.md` | `## 一覧` | PM03 の状態（Phase 1 の完了時に done、Phase 2 は別に記録） |

## 受け入れ条件

- [ ] D1 が承認されている。
- [ ] 消費側だけの段（手順 4・5）で、既存の全 suite の出力が HEAD と同じだった。
- [ ] `tests/compact_strings.mjs` が native/WASM × `-O0`/`-O3` で成功し、`live == 0`、WASM の import が空。基準コンパイラとの差分も成功。
- [ ] 「生成コードの確認」の期待がすべて成り立ち、IR が決定的。
- [ ] 公開 ABI・`src/abi.rs`・E13 の表が変わっていない。
- [ ] M1〜M4 の before／after を PX01 形式で記録し、`report` の表を報告した。改善は計測したものだけを主張している。
- [ ] 「ドキュメント」の更新を終え、GUIDE §10 の完了の定義を満たす。

## 落とし穴

1. 長さ語を分解せずに長さとして使うと、Latin-1 の値では 2^63 以上になり、`icmp ult` の境界検査が常に通って範囲外を読む。
   検出: E2E の `unit_at` の trap と `same_forms`。`extractvalue %tz.string` を足すときは必ず `string_parts` を通す。
2. Hash で語そのものを hash すると、形式で hash が変わり Map・Set が壊れる。単位数を hash する（`same_forms` の Hash bit）。
3. `String.compare` の値は観測できる。混在の左右を入れ替えたときの符号の反転を忘れると `compare_forms` が失敗する。
4. 印の判定は `icmp slt i64 %w, 0`（符号付き）、長さの比較は分解後の `icmp ult`。取り違えやすい。
5. Latin-1 の定数語を `u64` の 10 進（`9223372036854775813`）で出さない。`string_word` だけを使い、常に符号付きで出す。
6. n = 0 の Latin-1 を作ると、空文字列に 2 つの表現ができ、`frame_test` や `== zeroinitializer` のような検査と食い違う。生成する関数は全て n = 0 を先に扱う。
7. `frame_test` は欄 1 を frame の長さと比べる。Latin-1 のリテラルで語ではなく `length` を比べると、frame の判定が外れて二重解放か漏れになる（`live` で検出）。
8. `@tz.display.escape` は surrogate のときだけ前後の単位を `i16` で読む。Latin-1 の単位は surrogate にならないので到達しないが、
   `quote` の Latin-1 分岐から `i16` の読みを足さない。`\u0000`・`\u007f`・`\u009f` を含む境界入力で確かめる。
9. 新しい copy loop を LLVM の loop idiom が `memcpy` に変えると、wasm32 に import が現れる。`@tz.string.copy` と同じ形・属性で書き、
   `tests/strings.mjs` と E2E の import の検査と「生成コードの確認」の grep で検出する。
10. `from_units` は狭めたときだけ元の `i16` バッファを解放する。UTF-16 のままなら同じバッファを返す。`to_utf16` は Latin-1 の元を解放する。
11. `borrow_utf16` の一時領域は、元が Latin-1 のときだけ解放する。UTF-16 のときは借用中の元のデータなので解放すると二重解放になる。
12. `emit_parse` の `tz_soft_parse` へ語を渡すと、Latin-1 の数字列の parse が失敗する（`parse_digits` で検出）。
13. 生成側の漏れ（例: PR05 の `from_utf8` が `from_ascii` を呼ばない、`display.join` が常に UTF-16）は正しいままなのでテストでは落ちない。
    M1 の bytes と `digits_concat` の tracked bytes で検出する。
14. debugger（lldb）は DWARF の記述どおり `u16` の列として表示するので、Latin-1 の値は崩れて見え、長さも巨大に見える（D9。文書に書く）。

## 対象外

- UTF-8 を既定の文字列にする変更、rope などの遅延連結、文字列の intern、読み出し時に UTF-16 を狭める正規化。
- ポインターの下位 bit・別欄・確保の header による印（D1 で不採用）。`utf8string` の表現の変更。
- 公開 ABI に 1 byte の文字列を足すこと（E13 の新しい記述子を含む）。debugger の pretty printer（G16）。
- Phase 2 のベクトル化（人間が求めた場合だけ）。

## 決定事項

### D1: 印の置き場所と形式

- 決定: 長さ語の bit 63 を Latin-1 の印にする。印 0 が UTF-16 形式（HEAD と同じ）、印 1 が 1 単位 1 byte の Latin-1 形式。空文字列は印 0 の `zeroinitializer` だけ。
  `%tz.string` の型と大きさ、公開 ABI は変えない。
- 理由: 長さは 2^53 − 1 以下なので bit 63 は空いており、判定は符号の検査 1 命令である。印 0 を UTF-16 にすると、host の借用入力・
  `[i16u]` から移したバッファ・HEAD の記述子が変換なしに正しい値のままになり、消費側だけを先に変える段階的導入（手順 4・5）ができる。
  ポインターの下位 bit は `[N x i8]` の定数の align 1 と衝突し、`@tz.free` と全 load に復元が要り、LLVM の別名解析を妨げる。別欄は
  文字列とそれを含む全ての値を 16 → 24 bytes にし、確保の header は host の借用入力と定数に付けられない。
- 状態: 要承認（承認前はどの手順にも着手しない）

### D2: runtime の規約

- 決定: 文字列のデータを受ける runtime 関数は長さ語をそのまま受ける。既存の名前は dispatcher として残し、PR05 の UTF-16 の本体は `.utf16` 付きに改名する。
  新しい kernel は `src/runtime/string_latin1.ll`。`@tz.string.allocate` は UTF-16 のまま。
- 理由: 生成側の呼び出し（`binary`、`clone_value`、builtin の表、console、`Debug.print`、`display_quoted`）が変わらず、変更箇所が runtime に集まる。
  PR05 の本文と選択規則をそのまま使える。`allocate` を保つと guard harness の期待値（`expected * 2n`）が変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D3: Latin-1 を作る場所

- 決定: 空でなく全単位が 0xFF 以下のリテラル、`from_ascii`（数値の Display、`from_utf8` の全 ASCII）、両側が Latin-1 か空の `concat`、
  Latin-1 の `new`・`to_well_formed`、`from_units`、0xFF 以下の `character.display`、全部分が Latin-1 か空の `display.join`、Latin-1 の `display.quote`。
  それ以外（host の入力と結果、`Bool`・`Unit` の Display、非 ASCII を含む `from_utf8`）は UTF-16 を作る。
- 理由: 追加の走査なしに形式が分かる場所だけで狭め、最も多い ASCII の経路（リテラル、数値、入力の UTF-8、std の分割）を覆う。
  非 ASCII の `from_utf8` は PR05 の計数関数の変更が要るので Phase 2。
- 状態: 既定案（実装者はこの案に従う）

### D4: 正準形を要求しない

- 決定: 0xFF 以下だけの UTF-16 形式も正しい値とする。等価・順序・Hash は内容で決め、形式を比べない。
- 理由: host の借用入力を複製せずに正準化できない。正準形を前提にした等価の近道（形式が違えば不等）は誤りになる。
- 状態: 既定案（実装者はこの案に従う）

### D5: 混在の扱い

- 決定: 混在の比較は確保せずに byte を `zext` して比べる。UTF-16 と Latin-1 の連結は UTF-16。索引は形式の分岐と phi で、loop の外への
  分岐の移動は LLVM に任せる（手で loop を複製しない）。
- 理由: 混在のために確保すると非 Latin-1 のテキストが遅くなる（G2）。手の loop 複製は std の Tsuzuri コードでは不可能。
- 状態: 既定案（実装者はこの案に従う）

### D6: `String.from_code_units` と `String.to_code_units`

- 決定: `from_code_units` は全単位が 0xFF 以下なら Latin-1 に狭め、元の配列を解放する。`to_code_units` は Latin-1 を UTF-16 に広げる。
- 理由: std の `slice`・`split`・`join`・`trim`・`replace`・`repeat` は `Vec<i16u>` を作って `from_code_units` する。狭めなければ CSV の分割の
  結果が全て UTF-16 になり G1 に届かない。走査と1 回の確保が増える時間は M3 で計測する。`to_code_units` の結果の型は `[i16u]` なので広げるほかない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 公開 ABI と E13

- 決定: 公開 ABI は UTF-16 のまま。export の `string` の結果は `to_utf16`、型付き import の `ref string` の引数は `borrow_utf16` で変換する。
  host の入力と結果は UTF-16 形式のまま使う。E13 に 1 byte の記述子を足さない。
- 理由: C・JavaScript の利用者と E13 の生成物を壊さない。変換の確保は Latin-1 の値が境界を越えるときだけで、文書に記す。
- 状態: 既定案（実装者はこの案に従う）

### D8: `Bool`・`Unit` の定数

- 決定: `emit_display` と `emit_parse` の `[4 x i16]`・`[5 x i16]`・`[2 x i16]` は UTF-16 のままにする。
- 理由: `src/llvm.rs` の既存の IR テストがこの定数を検査している。値は小さく、正準形を要求しない（D4）ので正しい。
- 状態: 既定案（実装者はこの案に従う）

### D9: DWARF

- 決定: `src/llvm_debug.rs` の `string` の記述は変えない。Latin-1 の値が debugger で正しく見えないことを docs/architecture.md に書き、G16 に委ねる。
- 理由: DWARF では印による形式の切り替えを表せず、debugger ごとの pretty printer が要る。
- 状態: 既定案（実装者はこの案に従う）

### D10: 計測の workload

- 決定: 最大 RSS は `benchmarks/strings/` の 3 project（ASCII、Latin-1、CJK）を PX01 の process suite で、確保量は既存の cpp 5 種目を
  PX01 の tracked で測る。他言語の対応種目は足さない。
- 理由: 文字列を持ち続ける workload は既存の種目になく、RSS で差が見えない。比較の種目を足すと C++・Rust・C#・JavaScript の実装が要る。
- 状態: 既定案（実装者はこの案に従う）
