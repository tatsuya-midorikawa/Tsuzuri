# PM03: コンパクト文字列（Latin-1 の内部表現）

| 項目 | 内容 |
| --- | --- |
| ID | PM03 |
| 分類 | メモリ |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | PX01, PR05 |
| 関連 | A08, D02, E05, E13 |
| 状態 | doing |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`）。2026-10-17 に HEAD 9dd0fbbc で再検討（「着手時の再検討」） |
| 承認 | 2026-10-17 の全フェーズ実装依頼で、D1（`%tz.string` の内部表現。印は bit 62 に改訂）・D2〜D10 の改訂と新設の D11〜D14 を含む全フェーズ（Phase 1・Phase 2）を承認済み。測定で効果を確認できない既定の切り替えは行わない（D11） |
| 手本にする既存実装 | 内容で形式を選ぶ生成: `src/runtime/string.ll` の `@tz.string.from_utf8` の `ascii_only` 分岐。単位の幅による読み分け: `src/llvm_hash.rs` の `hash_primitive`（`bits` が 16 か 8）。runtime 関数を抜き出す試験: `tests/strings.mjs` の guard harness（`runtimeGuards`）。確保の計数と `live == 0`: `tests/strings_runtime.c`、`tests/strings.mjs` の host の `tracked_alloc` |
| 主な影響ファイル | runtime: `src/runtime/string.ll`, `src/runtime/string_latin1.ll`（新規）, `src/runtime/string_scalar.ll`・`src/runtime/string_v128.ll`（PR05 が作る。改名だけ）, `src/runtime/display.ll`, `src/runtime/character.ll`。生成: `src/llvm.rs`, `src/llvm_frame.rs`, `src/llvm_bulk.rs`, `src/llvm_display.rs`, `src/llvm_hash.rs`, `src/llvm_abi.rs`, `src/llvm_imports.rs`。変えない: `src/abi.rs`, `src/llvm_debug.rs`, `std/String.tz`。試験: `tests/strings.rs`, `tests/strings.mjs`, `tests/strings_runtime.c`, `tests/compact_strings.mjs`（新規）。計測: `benchmarks/strings/ascii_words/Main.tz`・`benchmarks/strings/latin1_csv/Main.tz`・`benchmarks/strings/cjk_lines/Main.tz`（新規）。文書: `docs/language.md`, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md` |
| 計測対象 | メモリ: 上の 3 workload の最大 RSS（PX01 の process suite の `execute`）、`cpp/utf16_scan`・`cpp/utf16_compare`・`cpp/utf16_validate`・`cpp/utf8_roundtrip`・`cpp/format_parse` の確保量（PX01 の tracked）。時間: 同じ cpp 5 種目と PR05 の `utf/*` |

## 目的

`string`（UTF-16 のコード単位列）の観測できる意味を一切変えずに、全コード単位が 0xFF 以下の文字列を 1 単位 1 byte（Latin-1 形式）で保存し、
ASCII 中心のテキストの文字列バッファを半分にする。JVM の Compact Strings、V8 の one-byte string と同じ方式である。
比較・連結・変換は Latin-1 同士なら読む byte 数が半分になり、速くなる見込みがある（計測で確かめるまで主張しない）。

Phase 1（表現と、runtime・生成の全操作のスカラー実装）だけを実装する。Phase 2（PR05 の 128-bit ベクトル版に Latin-1 の kernel を足す）は
人間が求めた場合だけ着手する。

## 着手時の再検討（2026-10-17）

2026-10-17 の全フェーズ実装の依頼（Phase 1 と Phase 2。`要承認` の決定は承認済み）で PM03 を実装する。以下の「着手条件と停止条件」「現状と計測（HEAD `f8dc655`）」「設計」「実装手順」
「決定事項」は起票時の記録で、このセクションと食い違う箇所はこのセクションが優先する（旧 `_docs/` のパスは `_tsuzuri/language-reference/`、GUIDE §8.1）。
着手時の HEAD は `a2313d19`（main `9dd0fbbc` = 第2・第3フェーズ統合後 + 計画の足場）。偵察は baseline compiler（`target/perf/phase4-baseline/tsuzuri-before`、SHA-256 `5e1b9687…`）、
Apple M1 Max、macOS 27.0.1、Apple clang 21.0.0、rustc 1.98.1、Node 20.19.6（PX01 の runner は `~/.npm/_npx` の Node 24.21.0）で行った。
生データは worktree の `target/perf/PM03/`・`target/perf/PM03-before-*`（Git の外）にある。この段で製品コードは変えていない。
共有機（load average 6〜40）の時間は参考値にとどめ、判断は決定的な量（確保量・IR・文字列の値）を根拠にする。

### 要点

1. **起票時の D1（長さ語の bit 63）は PM01 の niche と衝突する。** PM01 D5 の `Niche::NegativeLength`（`src/llvm_layout.rs`）は、case が 2 つで片方が payload なしの union の payload が
   `string`・`utf8string`・配列のとき（`Maybe<string>` など）、タグを持たず payload そのものにし、payload なしの case を `{ ptr null, i64 -1 }`、tag の判定を `icmp slt i64 <長さ>, 0`
   にしている（`niche_tag`）。bit 63 を Latin-1 の印にすると Latin-1 の語は全部負（`"hello"` は `-9223372036854775803`）で、`Some "hello"` が `None` と読まれる。
   印は **bit 62** にする（語は UTF-16 が `n`、Latin-1 が `n | 1<<62`、`n ≤ 2^53 − 1`）。bit 63 は全ての string で 0 のままなので、`llvm_layout.rs`・DWARF の `$niche`（−1）・LLDB の `niche_case`・
   natvis の `$niche == -1`・`docs/architecture.md` の「どの文字列・配列の長さも負にならず」は変わらない（D1 改訂）。
2. **起票時の「2 bytes／単位を仮定している箇所」の表は網羅していなかった。** PM06/PM07（`append`・`concat_into`・`Frame::Buffer`・`concat_length`）、D07（補間・`tz.format.*`）、
   PM08（`integer.ll` の整数 parse）、`for ... in text`、PR05 の CPU 経路、G16 のデバッガ（LLDB の formatter・natvis）が抜けている（下の表と WP）。
3. **PR05 の runtime は「手で改名する 2 つの文書」ではない。** `string_v128.ll` は `generate_utf.py` が `string_scalar.ll` から決定的に生成する（scratch で再生成し、1,586 行が byte 一致することを確認した）。
   native の既定は `string_cpu.ll`（x86 では `string_cpu_utf16.ll` も）が `cpu.c` の kernel で `from_utf8`・`validate_utf8`・`from_string` を上書きする。
   HEAD の runtime 文書は編集せず、追加層と include 時の改名で入れる（D2 改訂）。
4. **既定の切り替えは測定が決める。** `TSUZURI_STRINGS=utf16|compact`（PR02 の `TSUZURI_RANGE_FLAGS` と同じ形）を最初の段で入れ、`utf16` は HEAD と byte 単位で同じ IR にする。
   既定を `compact` にするのは WP10 で基準（D11）を満たしたときだけで、満たさなければ opt-in で出荷する（PM05 D10 と同じ扱い）。
5. **D9（DWARF は変えず G16 に委ねる）は取り下げる。** G16 は実装済みで、`tests/debugger.mjs`（この機械で baseline が通る）が `(string) text = "h\u00e9llo"`・`Maybe<string>` などの表示を固定している。
   formatter と natvis が印を読めないと、Latin-1 になる ASCII の文字列が全部 `<invalid length …>` になる。
6. **文書の契約が変わる。** `docs/language.md` と `_tsuzuri/language-reference/built-in-types-and-modules/string.md` は `String.to_code_units`／`from_code_units` を
   「ヒープ上のバッファならポインターを移すだけで、コピーも新規確保もしない」と書く。Latin-1 の値ではこれが成り立たない（D14）。
7. **期待値の修正:** PM05 の WASM heap は 16 bytes のヘッダーと 32 bytes の最小ブロックを持ち、native の malloc も 16 bytes 刻みなので、payload が 16 bytes 以下の短い文字列は縮まない。
   「文字列バッファが約半分」は長い文字列の話で、`ascii_words`（`word-12345` 程度）の最大 RSS の減少は半分より小さい（数字は計測で確かめる）。

### 起票時の記述と現行のコード

| 起票時の記述 | 現行 | 影響 |
| --- | --- | --- |
| D1: 長さは 2^53 − 1 以下で bit 63 は空き。bit 63 を Latin-1 の印にする | PM01 の niche が負の長さを None に使う（要点 1）。baseline の probe（下の `niche`）の IR は `ret %tz.string { ptr null, i64 -1 }`、`%v3 = icmp slt i64 %v2, 0` と `select i1 %v3, i32 0, i32 1` | D1 を bit 62 に改訂。`llvm_layout.rs`・`llvm_debug.rs` の niche は変更しない。Latin-1 の語は正の `i64`（`n = 5` は `4611686018427387909`）なので、符号付き 10 進で出す落とし穴（旧 4・5）は消える |
| 「2 bytes／単位を仮定している箇所（網羅）」の 18 行 | 漏れ: `llvm_control.rs` の `for_each`（`for unit in text` は欄 1 を長さにし、`i16` の要素のアドレスを局所に束縛する）、`llvm_frame.rs` の `concat_length`・`frame_concat`（`concat_into(buffer, 128, ..)` と `[256 x i8]`）・`Frame::Buffer` の `frame_test`／`heap_copy`、`llvm.rs` の `StringLength` の連結の畳み込み、`string.ll` の `append`（A17。`tz.realloc(old = 2n, new = 2m)`）・`concat_into`（PM06）・`allocate.check`、`llvm_display.rs` の `interpolation`（`allocate` 1 回と `copy`・`tz.format.widen`・`fill.u16`・`scalars.utf16`）と `display_quoted` の `Utf8String` 経路、`integer.ll` の整数 parse 3 関数（`i16` を直接読む）、`character.ll`、`string_cpu*.ll`、`scripts/lldb/tsuzuri_lldb.py`、`src/runtime/tsuzuri.natvis` | 「段ごとの変更」の表は下の WP に差し替える。表の網羅は raw-word lint（D12）で機械的に確かめる |
| PR05: `string_scalar.ll`・`string_v128.ll`（10 関数 + 計数 2）の 6 関数を両方で手で `.utf16` に改名する。着手条件の grep は `src/runtime/utf8string.ll` も挙げる | `utf8string.ll` は HEAD にある（115 行。`copy`・`allocate`・`new`・`concat`・`append`・`concat_into`。PM06/PM07 が足した）。`string_scalar.ll` は 13 関数（`validate_utf8`・`from_ascii`×2・`equal`×2・`compare`×2・`from_utf8`・`from_string`・`is_well_formed`・`to_well_formed`・計数 2）、`string_v128.ll` はその生成物と helper（`compare.scalar`／`.bulk`・`from_ascii.scalar`／`.short`・`from_utf8.short`／`.scalar_failure`・`from_string.bulk`／`.short`・`write_utf16`・`utf8.block`・`utf16.block`） | 改名はしない（D2 改訂）。`string_v128.ll` を手で直さない |
| native は `string_scalar.ll` か `string_v128.ll` の一方 | native の既定（`cpu_dispatch`）は v128 に `string_cpu.ll`（`from_utf8`・`validate_utf8` を `cpu.c` の `tsuzuri_cpu_utf8_to_utf16`／`_utf8_validate` で上書き）、x86 ではさらに `string_cpu_utf16.ll`（`from_string`）。32 bytes 以上では `allocate(units)` の後に kernel が UTF-16 を直接書き、`from_ascii` を呼ばない | CPU 経路の ASCII は wrapper で Latin-1 にする（D3）。`cpu.c` は変えない（D13） |
| `emit_parse` の数値: `@tz.string.to_ascii` の後に `tz_soft_parse` | PM08: 整数は `tz.integer.parse`・`parse128`（`integer.ll`。`i16` を直接読み、長さは語のまま渡す）。`to_ascii` は float・decimal だけ（`tests/size.mjs` が両方を固定） | 整数の Latin-1 parse が要る（WP4 で `format_parse` の数値を見て、Latin-1 版の複製か短い入力の stack への widen かを選ぶ） |
| 数値の Display は `@tz.string.from_ascii` で 1 桁 2 bytes | PR04/PM08: `integer_text` が 40 bytes の stack に 1 byte ずつ後ろから書き、`from_ascii(ptr %text, i64 %length)` が 2 bytes に広げる。float・decimal も `tz_soft_format` の後に `from_ascii` | `from_ascii` が Latin-1 の入口（`format_parse` の確保が約 40 → 20 bytes/回。広げる loop が消える） |
| D9: DWARF の `string` は `u16` の列のまま。debugger は G16 に委ねる | G16 実装済み。`tsuzuri_lldb.py` の `length_of` は `0 <= length < 2^32` でなければ `<invalid length>`、`string_summary` は `2n` bytes を読む。natvis は `{data,[length]su}`。`tests/debugger.mjs`（baseline で 21 秒）が表示を固定する | D9 を取り下げ、WP6 で formatter・natvis を印に対応させる。DWARF の記述（`data: u16*`・`length: i64`）は変えない |
| 文書は `docs/language.md` の `## 型とメモリ` と `docs/architecture.md` の 1 節 | 更新する所は多い（下の WP10）。契約が変わる所: 所有権移送 API の表と「O(1) で移送」（`docs/language.md`、`string.md` の「所有権を移す変換」）、`docs/architecture.md` の niche の節・runtime 文書の表・補間・`from_ascii`・`%tz.string` の説明、`compiler/debugging.md` | D14 |
| 既存テストの期待値の変更は 3 か所 | 追加層（D2 改訂）では、既定（`utf16`）の間、既存テストの期待値は変わらない。生成側は公開名を呼び続けるので、`src/llvm.rs` の IR テスト（`call i1 @tz.integer.parse(ptr %data, i64 %length, ..)`・`to_ascii(ptr %buffer, ptr %data, i64 %length)` など）は compact でも通る。`tests/strings.mjs` はコンパイラの出す IR（既定の `utf16` では HEAD と同じ）を、`tests/utf_simd.mjs`・`tests/strings_simd.c` は HEAD の runtime 文書を直接読むので、guard harness・`runtimeBridges`・差分 harness は変わらない | 下の「既存テストへの影響」 |
| `EmitOptions` に足す前提はない | `EmitOptions { .. }` を書く行は 38 か所（16 ファイル。`driver.rs`・`llvm.rs` と 14 の tests）で、フィールドの追加は他チケットと衝突する | `Instrumentation.strings: Option<StringForm>`（`range_flags` と同じ）と環境変数と `emit_with_strings`（D11） |
| PX01: `run-cpp.mjs --metrics`・`metrics.mjs process` が使える | 動く（Node 24.21.0 で `--quick` を確認）。`run-cpp.mjs` は Node 24 以上が必須。`metrics.mjs process` は `execute` の `peak_rss` を出す。`benchmarks/apps/Main.tz`（PX02）に `tokenize`・`format_join`・`csv_aggregate`・`map_count`・`record_sort` があり、`String.join`・`split`・`to_string`・`Parse` を使う | M3 に apps の文字列系を足す（D10）。3 workload は起票時の設計で作れる（下。`Vec.empty()` は `()` の前に空白を入れない） |
| `check-runtime-includes.sh` の N が増える | HEAD は 50 files。新しい `.ll` は `.gitignore` の例外行（`!src/runtime/<name>.ll`）が要る | 新しい文書の数だけ増える（Phase 1 で 50 → 54 の見込み、Phase 2 で生成物が 1 つ増える） |
| WASM に `memcpy` を入れない | WASM の build は常に `-mbulk-memory`（`driver.rs`）で、PM05 の `heap-wasm.ll` は `llvm.memcpy`（`memory.copy`）を使う。loop idiom の `memcpy` は import にならない | 停止条件は「import が増える」だけに絞る（`WebAssembly.Module.imports` が空）。`llvm.memcpy` 自体は許す |
| `tests/strings.rs` の件数 + 5 | HEAD は 14 件。`string_vectors_are_target_opt_in_and_deterministic` が `<16 x i8>` の有無を native／WASM scalar／WASM simd128 で固定する | 新しい Rust テストは別 file（`tests/compact_strings.rs`）に置く。Phase 1 の scalar の kernel に vector 型を使わない |

### 偵察で確かめたこと（実コンパイラを変えない probe。2026-10-17）

- 起票時の `literal` probe（`"hello"` と `"h\u00e9llo\u3042"`）: baseline は `12`、`@tz.literal.0 = ... [5 x i16]`・`[6 x i16]`。起票時の記述のとおり。
- `niche` probe（要点 1）。baseline で `4` を出し、`Maybe<string>` は裸の `%tz.string`、None は `{ ptr null, i64 -1 }`、tag は `icmp slt i64 len, 0`。

  ```tsuzuri
  def pick :: i64 -> Maybe<string>
  fn pick n = if n > 0 then Maybe.Some "hello" else Maybe.None

  def size :: Maybe<string> -> i64
  fn size value = match value with
      | Some text -> text.length
      | None -> 0 - 1

  size (pick 1) + size (pick 0)
  ```

- `generate_utf.py` を scratch の複製で実行すると `string_v128.ll`（1,586 行）が byte 単位で再生成される。`string_v128.ll` を再生成せず手で直すと次の生成で消える。
- IR の棚卸し: examples と `tests/fixtures` の 108 project のうち 102 を baseline で `--emit llvm -O0` に出し（残り 6 は別の旗が要る）、string の欄 1 を読む関数と `i16` で string のデータを読み書きする関数を集計した。
  runtime 側は `tz.string.{concat, append, concat_into, equal, compare, from_utf8, from_ascii, to_ascii, to_well_formed, is_well_formed, decode_utf16, copy}`、
  `tz.display.{join, escape, quote}`、`tz.character.{display, parse}`、`tz.format.{scalars.utf16, fill.u16, widen}`、`tz.integer.{scan, parse, parse128}`、`tz.rec.clone`。
  生成側は builtin の橋渡し（`clone_string`・`display.string`・`parse.*`・`hash.string`・`display_quoted.*`）と、利用者の関数の `extractvalue %tz.string …, 1`（索引・長さ・`for`・補間）。
- `-O3` の形式の分岐: `text[i]` を足し合わせる loop を、HEAD の形と compact の形（要素ごとの印の test と 2 つの load の phi）で手書きし、clang 21 の `-O3`（arm64）に通した。
  印の test は loop の外に unswitch され、Latin-1 と UTF-16 の 2 つの loop がそれぞれ vector 化された（`target/perf/PM03/unswitch/`）。単純な loop では要素ごとの分岐の費用は出ない。呼び出しを含む loop は std で確かめる（WP9）。
- baseline の suite: `node tests/strings.mjs`（native/WASM × -O0/-O3）は 3,973 cases・75 traps・4,344 batches・28 strict traps で成功（41 秒）。`node tests/debugger.mjs` は成功（21 秒）。
  `sh scripts/check-runtime-includes.sh` は 50 files。
- PX01（baseline、`--quick`）: `cpp` の確保（calls／bytes）は `utf16_scan` 6／336、`utf16_compare` 7／464、`utf16_validate` 7／466、`utf8_roundtrip` 8／528、`format_parse` 16／624（quick の大きさ）。
  formal の大きさの値は WP1 で取り直す（PR05 の記録では formal の `utf8_roundtrip` が 32 calls／2,359,248 bytes など）。
- 3 workload の設計を baseline で確かめた（期待値は Python の参照と一致: `sum(len(f"word-{i}"))` ほか）。出力は `ascii_words` 2088890、`latin1_csv` 2888890、`cjk_lines` 1888890。
  `execute` の `peak_rss`（`--quick` の 1 標本）は `ascii_words` 11,354,112 bytes（`-O0`・`-O3`）、`latin1_csv` 41,467,904（`-O0`）・49,577,984（`-O3`）、`cjk_lines` 37,879,808（`-O0`）・37,863,424（`-O3`）。
- 大きさ（baseline、`-O3`）: native の hello 33,896 bytes、WASM の hello 6,833 bytes、`hello` の `--emit llvm` は 144,926 bytes（3,559 行）。runtime の IR が増える分は M4 で報告する。

### 改訂した決定事項

- **D1（改訂）**: 印は長さ語の bit 62。語は UTF-16 が `n`（HEAD と同じ）、Latin-1 が `n | 4611686018427387904`（`n ≥ 1`）で、`n ≤ 2^53 − 1`、bit 63 は常に 0（PM01 の niche）。空文字列は語 0 の `zeroinitializer` だけ。
  分解は `n = and w, 4611686018427387903`、Latin-1 は `and w, 4611686018427387904` が 0 でないこと。`%tz.string`・`storage_layout`・公開 ABI は変えない。
  理由: UTF-16 の語が HEAD と同じなので、host の借用入力・`[i16u]` から移したバッファが変換なしで正しい値のままになり、消費側だけを先に変える段階的導入ができる。bit 62 なら PM01 の niche が使う符号を侵さない。
  却下した案: bit 63 のままで niche の判定を `icmp eq i64 len, -1` に変える（共有の `niche_tag` を変える。配列も同じ niche を使い、PM02 が同じ file を触り、不変条件の文書も書き換わる）。
  語を `n << 1 | 印` にする（UTF-16 の語が HEAD と変わる）。ポインターの下位 bit・別欄・確保の header（起票時の理由のまま）。
- **D2（改訂）**: 規約（runtime 関数は長さ語をそのまま受け、中で分解する）と公開名（dispatcher）は起票時のとおり。実装は追加層にする。
  dispatcher と新しい kernel は新しい文書（`string_compact.ll`・`string_latin1.ll`・`display_compact.ll`・`integer_compact.ll`。Phase 2 は生成物の `string_latin1_v128.ll`）に置き、
  compact のときだけ、HEAD の公開関数の定義行（`new`・`concat`・`append`・`concat_into`・`to_ascii`・`from_ascii`・`equal`・`compare`・`from_utf8`・`utf8string.from_string`・`is_well_formed`・`to_well_formed`・
  `display.join`・`display.quote`・`integer.parse`・`integer.parse128` など。WP3 で確定）を include 時に `.utf16` へ改名して、元の名前を dispatcher が引き受ける。
  追加層は包む HEAD の文書と同じ条件で連結する（`integer.ll` だけが入って `string.ll` が入らない program があるため）。`utf16` では何も改名・追加しない。
  改名は定義行だけを対象にし、改名後の定義がちょうど 1 つ見つかることを単体テストで固定する（`.portable` の改名と同じ流儀）。HEAD の runtime 文書と raw harness（`strings_runtime.c`・`strings_simd.c`・`utf_simd.mjs`・`display_parse_runtime.c`）は変えない。
  `@tz.string.allocate` は UTF-16 のまま。
- **D3（改訂）**: Latin-1 を作る場所は、空でなく全単位が 0xFF 以下のリテラルと frame の定数、`from_ascii`（数値・float・decimal の Display、`from_utf8` の全 ASCII。短い経路・v128・CPU 経路のすべて）、
  両側が Latin-1 か空の `concat`・`append`・`concat_into`、Latin-1 の `new`・`to_well_formed`、`from_units`（`String.from_code_units` で全単位が 0xFF 以下）、全部分が Latin-1 の `display.join`、Latin-1 の `display.quote`、
  0xFF 以下の `character.display`、全部分が Latin-1 で fill も 0xFF 以下の補間（WP5 の最後）、Phase 2 の「全 scalar が 0xFF 以下の `from_utf8`」。
  作らない: host の入力と結果、`Bool`・`Unit` の定数（D8）、`arguments.c` の引数。
- **D4・D7・D8**: 変更なし。
- **D5**: 変更なし（混在の比較は確保せず、byte を `zext` して比べる。形式の分岐は LLVM の unswitch に任せる。偵察で確認）。Phase 2 で混在の vector kernel（`<8 x i8>` を `<8 x i16>` に広げて比べる）を足す。
- **D6（改訂）**: 狭め・広げの意味は起票時のとおり。実装は新規確保を避ける。`from_units` は走査で全単位が 0xFF 以下と確かめてから、所有する `i16` の buffer を前から `i8` に詰め、`tz.realloc(data, 2n, n)` で縮める。
  `to_utf16`（所有する Latin-1 を消費）は `tz.realloc(data, n, 2n)` の後に後ろから広げる。`borrow_utf16` は借用なので新規確保（呼び出しの後に解放）。縮小の `realloc` が全 allocator（system・small・wasm・host）で実際に安く済むかは WP5 で測り、
  copy になる heap では新規確保の方式にする。`std` の `slice`・`split`・`join` が測定で退行するなら、native の builtin にする案を別の決定として記録する（既定の判断 D11 の材料）。
- **D9（取り下げ）**: G16 のとおり formatter と natvis を印に対応させる（WP6）。DWARF の記述は変えない。
- **D10（改訂）**: 測定の設計は下の「測定計画の更新」。
- **D11（新）**: 形式は `TSUZURI_STRINGS=utf16|compact` で選ぶ（`utf16` は HEAD と byte 単位で同じ IR。不正な値は `E2000`、`TSUZURI_RANGE_FLAGS` と同じ流儀）。
  `Instrumentation.strings: Option<StringForm>`（`None` は環境変数）、`llvm::emit_with_strings`、`src/cache.rs` の鍵の一覧、`src/main.rs` の `--help` に 1 行。既定は WP10 まで `utf16`。
  `compact` を既定にする基準は「測定計画の更新」の「既定の判断」。満たさなければ opt-in で出荷し、結果を `docs/benchmarks.md` と本 ticket に記録する。`compact` が既定になった場合、`utf16` が文書化した fallback である。
- **D12（新）**: 不変条件: 全ての string の語で bit 63 は 0、空は語 0、Latin-1 の語は `n ≥ 1`。語の分解は `llvm_strings.rs` の helper（`string_parts`・`string_unit`・`string_length`）だけを通す。
  raw-word lint: compact の IR（corpus 全体）で `extractvalue %tz.string …, 1` の結果が、マスク・runtime の dispatcher・`insertvalue`・frame の等価検査以外（比較・加減算・添字・`getelementptr` など）で使われていれば失敗する（`tests/compact_strings.rs`）。
  `Maybe<string>`・`Maybe<[T]>` の niche と Latin-1 の `Some` の区別は E2E（`maybe_forms`）で固定する。
- **D13（新）**: Phase 2 の構造: Latin-1 の vector kernel は `string_latin1.ll`（scalar の参照。WASM の既定はこれ）から `generate_utf.py` が `string_latin1_v128.ll` を生成する。選択規則は PR05 D2 と同じ（native は常に vector、WASM は `wasm_simd128` のときだけ）。
  `string_scalar.ll`・`string_v128.ll`・`cpu.c` は変えない。`from_utf8` の計数は `@tz.string.utf8_to_utf16_length` の隣に、単位数と最大の scalar を返す `@tz.string.utf8_scan`（`{ i64 units, i32 max }`。不正は units = −1。
  最大が 0xFF を超えた時点で打ち切ってよい）を足す。wrapper は ASCII なら Latin-1 の copy、そうでなければ `utf8_scan` で `max ≤ 0xFF` なら Latin-1 の write（2-byte の lead が C2・C3 の窓は vector）、それ以外は HEAD の `from_utf8.utf16`（CPU 経路を含む）へ渡す。
  PR05 の差分 harness（`utf_simd.mjs`・`strings_simd.c`）を、Latin-1 の kernel と scalar の参照（`ref.`）の差分に広げる。生成物の再生成一致も検査する。
- **D14（新）**: 文書の契約: `String.to_code_units`／`from_code_units` は所有権を消費する変換のままで、「ポインターを移すだけ」は UTF-16 の値のときだけ成り立つ。Latin-1 の値は `to_code_units` が広げ、`from_code_units` が狭める
  （D6 の実装。O(n)）。値と trap の条件は変えない。`docs/language.md` の表と `string.md` の「所有権を移す変換」を同じ変更で直す。

### 作業の順序（コミット）

各 WP の後で tree はビルドでき、それまでのテストは通る。WP2 の後は `utf16` の IR が baseline と byte 同一であることを毎回確かめる（`target/perf/PM03/ir-identity.mjs`。corpus は examples と `tests/fixtures`、native／wasm32 × -O0／-O3 の `--emit llvm`）。

| WP | 内容 | 確認 |
| --- | --- | --- |
| WP0 | この節（docs のみ） | `git diff --check` |
| WP1 | baseline の固定と測定の土台。`benchmarks/strings/{ascii_words,latin1_csv,cjk_lines}/Main.tz`（下の設計）、before（cpp・utf・apps・process・build・size）を `target/perf/PM03-before/` に保存、corpus の IR と suite の出力を保存、`docs/benchmarks.md` の marker に before を記録 | 3 workload が `2088890`・`2888890`・`1888890`（Python の参照と一致）。`node benchmarks/metrics.mjs process` が `peak_rss` を 9 標本で出す |
| WP2 | 方針の配管（挙動を変えない）。`src/llvm_strings.rs`（新規。`StringForm`・環境変数の読み取り・語の定数と分解の helper・include 時の改名の helper）、`Instrumentation.strings`・`Globals.strings`、`llvm::emit_with_strings`、`src/cache.rs` の鍵、`src/main.rs` の `--help` | corpus の IR が baseline と byte 同一。単体テスト: 既定は `utf16`、不正値は `E2000`、`cache` の鍵に入る |
| WP3 | runtime の追加層（まだ生成側は呼ばない）。`string_latin1.ll`（scalar の kernel: `allocate_latin1`・`copy_bytes`・`widen`・`narrow`・Latin-1 同士と混在の `equal`／`compare`・Latin-1 → UTF-8・`from_units`・`to_utf16`・`borrow_utf16` ほか）、`string_compact.ll`・`display_compact.ll`・`integer_compact.ll`（dispatcher）、include 時の改名と選択、`.gitignore`、`abi.rs` の予約名の試験 | 新しい raw harness が Latin-1 の語を直接渡して全 kernel と dispatcher を JavaScript の参照と照合（境界・trap・`live == 0`）。`TSUZURI_STRINGS=compact` で既存 suite が（Latin-1 を作らないので）HEAD と同じ出力 |
| WP4 | 消費側（compact のとき両形式を読む。まだ Latin-1 を作らない）。`Index`・`StringLength`・`concat_length`・`hash_primitive`・`for_each`・`emit_parse`・`display_quoted`・補間の穴・`string_buffer_builtin`（`to_code_units`）・`llvm_abi.rs`（`to_utf16`）・`llvm_imports.rs`（`borrow_utf16`）・`Frame::String` の語 | compact で全 suite が HEAD と同じ出力。raw-word lint が 0 件 |
| WP5 | 生成側（compact のとき Latin-1 を作る）。リテラルと frame、`from_ascii`、`concat` 系、`from_units`、`display.join`／`quote`、`character.display`、補間 | M1: 偶数 seed の 5 種目の bytes が減り、奇数 seed は同じ。WP7 の形式の行列が成功 |
| WP6 | デバッガ。`tsuzuri_lldb.py`・`tsuzuri.natvis`・`compiler/debugging.md`（formatter なしで見える raw の `data`／`length` の注意） | `TSUZURI_STRINGS=compact node tests/debugger.mjs` が baseline と同じ表示で成功 |
| WP7 | テスト。`tests/compact_strings.rs`（IR の不変条件・lint・決定性・既定の `utf16`）、`tests/compact_strings.mjs`（形式の行列。native／WASM × -O0／-O3、`live == 0`、import が空、`TSUZURI_BASELINE` との差分）、`tests/compact_strings_modes.mjs`（既存 suite を `compact` で。`range_flags_audit.mjs` の形） | 各テストの件数（`running N tests`）を記録 |
| WP8 | Phase 2。`generate_utf.py` の拡張と `string_latin1_v128.ll`、選択規則、`utf8_scan` と非 ASCII の Latin-1 の `from_utf8`、混在の vector、`utf_simd.mjs`・`strings_simd.c` の拡張 | `node tests/utf_simd.mjs --quick` が native CPU baseline／auto と WASM scalar／simd128 × -O0／-O3 で成功。再生成して byte 一致 |
| WP9 | 測定と生成コードの確認（before／after／before2。D10） | `report` の表。IR／asm（unswitch の有無・`memcpy` の import が 0・`imports` が空・決定性）を記録 |
| WP10 | 既定の判断（D11 の基準）と文書（`docs/language.md`・`docs/architecture.md`・`README.md`・`_tsuzuri/language-reference/`・`docs/benchmarks.md`・この ticket の最終節）、`.github/workflows` の変更案を親へ | `node scripts/check-docs.mjs <変更したページ>`、`git diff --check` |
| WP11 | 全体の検証（full `cargo test`・clippy 1.99・fmt・全 node suite を両モードで）と完了処理（`状態 \| done`・`## 実装と検証`・`git mv`） | 完了報告 |

### 既存テストへの影響（先に宣言する）

- 既定が `utf16` の間（WP10 まで）: 期待値の変更はない。HEAD と IR が byte 単位で同じことをその証拠にする。
- `TSUZURI_STRINGS=compact` で Rust のテストを回したとき、IR の形を固定した試験のうち compact で形が変わるものは、`StringForm::Utf16` を明示して HEAD の形を固定する（期待値は緩めない。`RangeFlags::Off` を明示する試験と同じ流儀）。
  見込みは `tests/string_interpolation.rs` の「`@tz.string.allocate` が 1 回」（補間の確保が形式で分かれるとき）だけで、`[3 x i16]`・`[4 x i16]` の定数（非 Latin-1 のリテラル・`Bool`・書式指定の spec は UTF-16 のまま）と
  `tests/storage.rs`・`tests/display_parse.rs`・`tests/strings.rs` の `@tz.string.new` の有無は両モードで通ることを確かめる。compact の形は新しいテストが固定する。
- WP10 で既定を `compact` にする場合だけ: `tests/strings.mjs` の `runtimeGuards`（`allocate`・`concat` の抜き出しと stub）と `runtimeBridges` の `decode`（`@tz.string.to_utf16` を通す）のほか、
  runtime 文書の連結だけに依存する部分（`tests/llvm_handoff.rs` の連結の改行など、あれば）を compact の形に合わせる。値（`3973`・`75`・`4344`・`28`・`live == 0`）は変えない。
- それ以外の変更が要るときは、ここに足してから変える（GUIDE §13.1）。

### 測定計画の更新

- **3 workload の設計**（baseline で `check`・`run` を確かめ済み。WP1 で `benchmarks/strings/` に置く）。各 project は結果の式で合計を表示する。

  ```tsuzuri
  // ascii_words/Main.tz（期待 2088890）
  def words :: i64 -> Vec<string>
  fn words count =
      let mut output: Vec<string> = Vec.with_capacity count
      for index in 0 .. (count - 1) do output = Vec.push output ("word-" + to_string index)
      output

  def total :: ref [string] -> i64
  fn total items =
      let mut sum = 0
      for item in items do sum = sum + item.length
      sum

  let all = Vec.to_array (words 200000)
  total (ref all)
  ```

  `latin1_csv`（期待 2888890）は、`rows count = new [string](count, index -> "caf\u00e9," + to_string index + ",na\u00efve")` を `String.join "\n"` で 1 つの文字列にし、`String.split "\n"` で行、各行を `String.split ","` で欄に分け、
  全欄を `Vec<string>` に（`clone_string (ref part)` で）持ち続けて `total` で `.length` の和を返す。`cjk_lines`（期待 1888890）は `café`・`naïve` を `\u6771\u4eac`・`\u5927\u962a` に替えたもの。
  `cjk_lines` の欄のうち数字だけの欄は `split` の `from_code_units` で Latin-1 になるので、RSS は「変化なし」ではなく「悪化しない」で判定する（G2）。
- **指標**: M1（決定的）: `cpp` の 5 種目（seed 偶数・奇数）と `apps` の文字列系（`tokenize`・`format_join`・`csv_aggregate`・`map_count`・`record_sort`）の `alloc_calls`・`alloc_bytes`。
  M2: 3 workload の `execute` の `peak_rss`（9 標本の中央値・最小・最大。native `-O3`）。
  M3: 時間（参考値）: `cpp` の 5 種目、`utf/*` の 90 行、`apps` の文字列系、3 workload の wall。before／after／before2 の 3 回を交互に取り、before2 と before の差が出た metric は判定に使わない。
  M4: 大きさとビルド: `hello` と 3 workload の `executable_bytes`・`wasm_bytes`・`--emit llvm` の bytes、`build -O0`／`-O3` の時間（PX03 の `run-build.mjs`）。runtime の IR が増える分と、Phase 2 の vector の分は別に報告する。
- **既定の判断（D11）の基準**（`compact` を既定にするのは、次をすべて満たすときだけ）: (1) 全 suite が両モード・native／WASM × -O0／-O3 で成功し、`live == 0`・WASM の import が空・IR が決定的。
  (2) M1 は `cpp` で偶数 seed が減り奇数 seed が同じ（決定的）、apps の文字列系で bytes が減る。M2 は `ascii_words`・`latin1_csv` が before2 との範囲を超えて下がり、`cjk_lines` が before の最小〜最大を超えて悪化しない。
  (3) M3 は、cpp 5 種目・`utf/*`・apps の文字列系・3 workload のどれにも、9 組の `report` で `悪化` が残らない（改善は要求しない）。
  (4) `TSUZURI_STRINGS=utf16` が baseline と byte 同一の IR を出し、既定の変更が文書化されている。
  満たさない、または共有機の雑音で判定できないときは、既定を `utf16` のままにして結果を記録する。
- 正式な時間の比較は、静かな機械で親が最後に行う。この ticket の数値は日付・機械・commit・load average・標本数・生データの場所とともに `docs/benchmarks.md` の `<!-- phase4:PM03 -->` の中に書く。

### 受け入れ条件・停止条件の更新

- 受け入れ条件: 旧「D1 が承認されている」は承認済み（D1 は bit 62 に改訂）。追加: (a) `utf16` の IR が baseline と byte 同一（corpus 全体。既定を変える WP10 まで）、(b) `compact` で全 suite が両モードで成功し、
  `tests/debugger.mjs`・`tests/host_imports.mjs`・`tests/bindings*.mjs` を含む、(c) raw-word lint が 0 件で、bit 63 の立った語が観測されない、(d) Phase 2 の差分 harness が成功し、生成物が再生成と byte 一致、
  (e) `TSUZURI_BASELINE` との差分（`tests/compact_strings.mjs`）が成功、(f) 公開 ABI・`src/abi.rs`・E13 の表が変わっていない、(g) 「文書」の更新と D14 の契約の書き換え、(h) 既定の判断の記録。
- 停止条件: 「PR05 の関数名・置き場所が前提と違う」は本節の実測の一覧で置き換える。「手順 4 で観測結果が変わる」は WP4 の確認で読み替える。
  追加: `utf16` の IR が baseline と 1 byte でも違う（WP10 まで）／bit 63 の立った string の語を観測した／`compact` で `tests/debugger.mjs` が崩れ formatter で直せない／生成物が再生成と一致しない／
  Phase 2 の vector が scalar の参照と違う。apps の文字列系などで `悪化` が残り D5・D6 の範囲で直せない場合は、止めずに既定を `utf16` のままにして記録する（D11）。

### 他チケットとの関係（第4フェーズ）

- PM04（リスト）: `llvm_display.rs`・`llvm_hash.rs`・`llvm_frame.rs`・`llvm_control.rs`（`for_each` のリストの分岐の隣に文字列の分岐を足す）が重なる。PM03 の編集は helper（`llvm_strings.rs`）への 1〜3 行の呼び出しに抑える。統合順は PM04 → PM03。
- PM02（関数値）: `llvm_layout.rs` の niche を PM03 は触らない。`llvm_frame.rs` の `Frame` は別の arm。
- PB05（Cranelift）: compact の runtime 文書は `and`・`icmp`・`select`・`phi`・`i8` の load・`zext`／`trunc` と、Phase 2 の `<16 x i8>` を使う。既定が `utf16` の間は PB05 の入力は変わらない。PB05 の parity は `TSUZURI_STRINGS=compact` でも回す（統合後）。
- PB06・PR07・PR08: `src/main.rs`（`--help` の 1 行）と `src/cache.rs`（鍵の 1 行）だけが重なる。
- 取り込み済みのもの: PM01（niche）、PM05（`tz.realloc(old, new)` は byte 数。WASM の最小ブロック）、PM06/PM07（`concat_into`・`append`・`Frame::Buffer`）、PR04/PM08（`from_ascii`・`integer.ll`）、PR05（kernel・生成・harness）、
  PB01/PB03（runtime の IR は module ごとに文字列で連結され、事前ビルドは numeric・math・C のみ。native の実行ファイルは未到達の関数を剪定する）、G16（デバッガ）。

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
