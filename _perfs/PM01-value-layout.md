# PM01: 値のレイアウトの最適化（union・niche・フィールド順）

| 項目 | 内容 |
| --- | --- |
| ID | PM01 |
| 分類 | メモリ |
| 優先度 | P1 |
| 規模 | L |
| 依存 | PX01 |
| 関連 | A02, A04, A16, G08, G16, E05 |
| 状態 | todo |
| 起票 | 2026-09-29。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1・D2（Phase 1: タグの幅と General の領域）, D5（Phase 2: niche）, D7（Phase 3a: レコードの並べ替え）, D10（Phase 3b: タプルの並べ替え） |
| 手本にする既存実装 | 再帰 union の null による表現（`src/llvm_recursive.rs` の `recursive_tag`・`recursive_construct`、GUIDE §9 D-24）。`UnionLayout` で分岐する値の分解（`src/llvm.rs` の `payload_value`・`drop_value`）。宣言順の C 配置を内部表現と別に計算する公開 ABI（`src/llvm_abi.rs` の `record_layout`・`read_host_record`・`write_host_record`） |
| 主な影響ファイル | `src/llvm.rs`（`UnionLayout`・`union_layout`・`storage_layout`・`llvm_type`・`emit_program`・`construct_value`・`union_tag`・`payload_value`・`payload_pointer`・`drop_value`・`clone_value`・`emit_expression_mode`・`emit_place`・`sequence_next`・`io_entry`）, `src/llvm_task.rs`（`parallel_result_tasks`）, `src/llvm_debug.rs`（`layout`・`fields`・`ty`）, `src/llvm_frame.rs`（`field_types`・`frame_value`、Phase 3a）, `src/llvm_abi.rs`（`read_host_record`・`write_host_record`、Phase 3a）, `src/llvm_control.rs`（確認のみ）, `tests/value_layout.rs`（新規）, `tests/union_types.rs`, `tests/fixtures/value_layout/Main.tz`（新規）, `tests/features.mjs`, `tests/debug_info.mjs`, `benchmarks/layout/Main.tz`（新規）, `docs/architecture.md`, `docs/benchmarks.md`, `_perfs/README.md` |
| 計測対象 | 型ごとの値の大きさとアラインメント（bytes、native と wasm32）。`benchmarks/layout/Main.tz`（新規）の最大 RSS（bytes）。`benchmarks/computations/Main.tz` の `ce_std_option`・`direct_std_option`・`ce_std_result`・`direct_std_result`・`ce_std_option_owned`・`direct_std_option_owned` の時間 |

## 目的

union・`Maybe`・`Result`・レコードの値を小さくし、配列やコレクションに多数を格納するときのメモリ量・キャッシュミス・コピー量を減らす。
目安は Rust の同等の型の大きさ以下（`## 目標と指標` の表）。変えるのは内部表現だけで、言語の意味・診断・公開 ABI は変えない。

作業は三段に分け、各段を単独で出荷できるようにする。

- Phase 1: union のタグの幅をケース数に合わせ、ペイロードの LLVM 型が異なる union（`UnionLayout::General`）の 16 bytes 単位の領域をやめる。タグの幅だけ（手順 3 まで）でも出荷できる。
- Phase 2: ケースが二つで一方だけがペイロードを持つ union（`Maybe` の形）に niche を使う。
- Phase 3a: レコードのフィールドをアラインメントの降順に並べ替えて余白を減らす。Phase 3b（タプルと union のペイロードのタプル）は Phase 3a の計測を見て判断する。

実装者は Phase 1 だけを行う。Phase 2 以降は、人間が指示し、その段の決定事項が承認された場合だけ着手する。

## 着手条件と停止条件

- 依存: PX01 が `done`（`_perfs/README.md` の一覧の状態欄で確認）。
- 承認: 着手する段の決定事項が承認済み（Phase 1 は D1・D2、Phase 2 は D5、Phase 3a は D7、Phase 3b は D10）。内部表現を変える施策は承認を得てから実装する（`_perfs/README.md` の運用ルール）。
- 基準線: GUIDE §2.3 の手順で、HEAD の `cargo build --release --locked` と `cargo test --locked` が通ることを確かめる。その後、変更前のコンパイラで `## 計測手順` を `P=phase1-before` で実行する。
- 共通の規則: GUIDE §6（変更パターン別チェックリスト）、§7（テスト）、§10（完了の定義）、§11（小さいモデルへの追加指示）、§14（性能チケットの共通手順）。

### 前提とする他チケットのインターフェース

- PX01: 計測結果の記録形式（日付・計測機・コミット・コンパイラの版・生データの保存先）が `docs/benchmarks.md` に定まっている。PX01 が最大 RSS や確保量を記録する仕組みを加えていれば、`/usr/bin/time -l` の代わりに使ってよい。最大 RSS の定義は同じ（macOS の maximum resident set size、bytes）。
- PM02（関数値の表現）: この ticket は `%tz.closure` の形に依存しない（`storage_layout` の値を使うだけ）。関数値は niche の対象にしない（D5）。

### 停止条件（止めて報告する）

1. 承認されていない段の変更が必要になった。
2. E1010 の判定（`src/check.rs` の `Layouts`）、`src/llvm_frame.rs` の `stack_size`、`tests/union_types.rs` の `enforces_union_layout_limits` の期待値を変えたくなった（D4）。
3. `### 既存テストへの影響` に挙げていない既存の期待値が変わる。特に `tests/debug_info.mjs`、`tests/recursive_types.rs` の再帰ノードの型、`tests/gpu.rs`。
4. `## 目標と指標` の表の型、または表にない型が、その段の前より大きくなる（`sizes-native.txt`・`sizes-wasm32.txt` の差分で確認）。
5. union のタグを `union_tag`・`construct_value` を通さずに読み書きする箇所で、幅を正しく扱えないもの（ランタイムの `.ll`・C が union の値を受け渡すなど）が見つかった。
6. Phase 2: null の参照、または長さが負の `string`・`utf8string`・配列を作りうる経路（ランタイム、ホストの import、ゼロ埋めしたメモリを値として読む箇所）が見つかった。
7. 構造比較・Hash・Display が値のバイト列を直接使っている（`memcmp` など。パディングと並べ替えで結果が変わる）。
8. Phase 3a: レコードのフィールドを添字の直書きで扱うランタイム・生成コードで、`field_slot`（新規）を通せないものがある。レコードの宣言が std 由来かを判定する手段がない。公開 ABI の変換がレコードをフィールドごとでなくバイト列として写している。
9. `unsafe`、新しい crate、WASM の既定の import が必要になった。スタックや入れ子の深さの上限を上げたくなった。
10. `## 計測手順` の時間で、変更後の値が変更前の 3 回の最小〜最大の範囲より遅い種目がある。既定の表現にせず、結果を記録して判断を仰ぐ（`_perfs/README.md` の運用ルール）。

## 現状と計測（HEAD `f8dc655`）

### union の表現

`src/llvm.rs` の `union_layout` が `UnionLayout` を選び、`storage_layout` が大きさとアラインメントを返す（native を仮定し、ポインターは 8 bytes）。`storage_layout` を呼ぶのは `storage_layout` 自身と `union_layout` だけで、用途は `General` の領域の大きさ。

- `UnionLayout::Enum`: 全ケースがペイロードなし。値はタグそのもので、型定義は `= type i32` の別名。
- `UnionLayout::Common(Type)`: 全ペイロードの LLVM 型が同じ。`{ i32, T }`。
- `UnionLayout::General(usize)`: それ以外。`{ i32, [K x i128] }` で、K は最大ペイロードの大きさを 16 で切り上げた数。
- 再帰 union は所有ノードへのポインター（8 bytes）で、最初のペイロードなしのケースを null で表す（GUIDE §9 D-24。既存の niche）。ノードの型（`src/llvm_recursive.rs` の `node_type`）は添字 3 に `i32` のタグを持つ。
- タグを書くのは `construct_value`（`insertvalue {llvm} zeroinitializer, i32 {case_id}, 0`）。型定義は `emit_program`（`{{ i32, {} }}` と `{{ i32, [{count} x i128] }}`）。
- タグを読むのは `union_tag`（値は `extractvalue … , 0`、場所は `load i32, ptr {slot}`、再帰は `recursive_tag`）と `case_switch`（`switch i32`）。`src/llvm_control.rs` の `selector`・`switch_plan`・`match_expression` はこれらを経由する。
  例外は `src/llvm.rs` の `sequence_next`（`extractvalue … , 0` の後に `icmp eq i32 {tag}, 1`）と `src/llvm_task.rs` の `parallel_result_tasks`（`icmp eq i32 {tag}, {error_case}`）で、非再帰の union のタグを直接読む。
- union の比較・Hash・Display は `src/derive.rs` の `instances` が作るパターン照合を通るので、タグは `case_switch` 経由で読まれる。`src/llvm_hash.rs` の `hash_primitive` の `tag` は型の識別子で、union のタグではない。
- ランタイムの `{ i32, i64 }`（`@tz.string.decode_utf8`・`@tz.string.try_decode_utf8` の戻り値。`src/llvm_bulk.rs`・`src/llvm_abi.rs` が使う）は UTF-8 の復号の結果で、union ではない。

### レコードとタプルの表現

- レコードは宣言順の LLVM 構造体（例: `%tz.record.Main.Mixed = type { i1, i64, i1, i64 }`）、タプルは無名の構造体 `{ T1, T2 }`。並べ替えはない。
- 公開 ABI: エクスポートの引数・結果に union は使えない（E1008。`export def pick :: Color -> i64` と `export def first :: Maybe<i64> -> i64` で確認）。スカラーだけのレコードは使え、ホスト側の C の構造体は `src/llvm_abi.rs` の `record_layout`・`header_types` が宣言順で別に作り、`read_host_record`・`write_host_record` が内部のレコードとの間を変換する。
- GPU: `src/gpu.rs` は buffer の要素に `bool` 以外のスカラーだけを許す（"GPU operations require a concrete buffer result" の検査の直後）。ユーザーのレコードは WGSL に渡らない。

### 大きさの計算は 4 か所にある

| 関数 | 用途 | union の式（HEAD） | この ticket |
| --- | --- | --- | --- |
| `src/llvm.rs` の `storage_layout` | `General` の領域の大きさ | Enum 4、General `16 + 16 * K` | 新しい配置に合わせる |
| `src/llvm_debug.rs` の `layout` | DWARF の大きさ（`wasm` のときポインター 4） | 同上 | 新しい配置に合わせる |
| `src/check.rs` の `Layouts`（`union`・`record`・`size`） | E1010 の 64 KiB 上限 | ペイロードありは 16 とペイロードを 16 に切り上げた和、なしは 8 | 変えない（D4） |
| `src/llvm_frame.rs` の `stack_size` | スタックに置くリテラルの見積もり | 同上（フィールドごとに 16 に切り上げ） | 変えない（D4） |

`docs/language.md` は E1010 の上限を保守的な見積もりと定めている。新しい配置は全段でこの見積もり以下になる（Phase 1 はタグが 4 bytes 以下で領域の単位が 16 bytes 以下、Phase 2 はペイロードの大きさそのもの、Phase 3a は余白を減らすだけ）。

### 計測（2026-09-29、作業機での参考値）

`benchmarks/layout/Main.tz`（新規。内容は `## 計測手順`）を HEAD のコンパイラで `--emit llvm -O0` にし、`## 計測手順` の手順 2 で畳み込んだ値（bytes、大きさ / アラインメント）。生成 IR に `target datalayout` はないので、手順 2 がデータレイアウトを与える。

| 型 | LLVM 型（HEAD） | native | wasm32 |
| --- | --- | ---: | ---: |
| `Color`（ペイロードなし 3 ケース） | `i32` | 4 / 4 | 4 / 4 |
| `Maybe<bool>` | `{ i32, i1 }` | 8 / 4 | 8 / 4 |
| `Small`（`bool`・`i8`・なし） | `{ i32, [1 x i128] }` | 32 / 16 | 32 / 16 |
| `Maybe<i64>` | `{ i32, i64 }` | 16 / 8 | 16 / 8 |
| `Maybe<ref Point>` | `{ i32, ptr }` | 16 / 8 | 8 / 4 |
| `Maybe<string>` | `{ i32, %tz.string }` | 24 / 8 | 24 / 8 |
| `Result<i64, string>` | `{ i32, [1 x i128] }` | 32 / 16 | 32 / 16 |
| `Shape`（`f64`・`f64 * f64`・なし） | `{ i32, [1 x i128] }` | 32 / 16 | 32 / 16 |
| `Mixed { a: bool, b: i64, c: bool, d: i64 }` | `{ i1, i64, i1, i64 }` | 32 / 8 | 32 / 8 |
| `Tagged { tag: i8, value: i64, ok: bool }` | `{ i8, i64, i1 }` | 24 / 8 | 24 / 8 |
| `%tz.closure`（参考） | `{ ptr, ptr, ptr, ptr }` | 32 / 8 | 16 / 4 |

- 最大 RSS（`-O3`、`/usr/bin/time -l`、9 回）: 最小・中央値・最大とも 165,904,384 bytes。出力は `16166714`。
- 配列の要素の合計は 188,000,000 bytes（`## 目標と指標`）で、RSS はそれより約 22 MB 小さい。全要素が `None` の `Maybe<string>` の配列（24,000,000 bytes）が書き込まれず、ページが常駐しなかった可能性がある（未確認。`## 落とし穴` 5）。
- 時間: `_perfs/README.md` の現状（2026-09-26）では `std_option` 0.873、`std_result` 0.95〜0.99、`std_option_owned` 約 0.09（確保の条件がそろっていない）。この詳細化では再計測していない。
- JSON のような多ケースの union は、非再帰の `General` の代表として `Shape`・`Small` で測る。自分自身を含む JSON の値は再帰 union（ノード）で、ノードの配置はこの ticket の対象外（D1）。

## 目標と指標

目標は計画値で、共有 CI の合否条件にしない。表は D1・D2・D5・D8 の規則から算出した native の大きさ（bytes）。
wasm32 では `Maybe<ref Point>` が 8・8・4・4 になり、他の行は native と同じ。Rust の列は既知の値で、`## 計測手順` の手順 5 で確かめる。
Tsuzuri の `string` は UTF-16 の `{ ptr, i64 }`（16 bytes）で、Rust の `String`（24 bytes）とは表現が違う。

| 型 | HEAD | Phase 1 | Phase 2 | Phase 3a | Rust（参考） |
| --- | ---: | ---: | ---: | ---: | ---: |
| `Color` | 4 | 1 | 1 | 1 | 1 |
| `Maybe<bool>` | 8 | 2 | 2 | 2 | 1 |
| `Small` | 32 | 2 | 2 | 2 | 2 |
| `Maybe<i64>` | 16 | 16 | 16 | 16 | 16 |
| `Maybe<ref Point>` | 16 | 16 | 8 | 8 | 8 |
| `Maybe<string>` | 24 | 24 | 16 | 16 | 24 |
| `Result<i64, string>` | 32 | 24 | 24 | 24 | 24 |
| `Shape` | 32 | 24 | 24 | 24 | 24 |
| `Mixed` | 32 | 32 | 32 | 24 | 24 |
| `Tagged` | 24 | 24 | 24 | 16 | 16 |
| `benchmarks/layout` の配列の合計 | 188,000,000 | 133,000,000 | 125,000,000 | 109,000,000 | – |

配列の合計は、要素数 1,000,000 の 8 個の配列（`Color`・`Result<i64, string>`・`Maybe<string>`・`Mixed`・`Shape`・`Small`・`Maybe<bool>`・`Tagged`）の要素の大きさの和。

指標:

1. 値の大きさ: 型ごとの大きさとアラインメント（bytes、native・wasm32）。`## 計測手順` の手順 2 の `sizes-native.txt`・`sizes-wasm32.txt`。決定的なので 1 回でよい。
2. 最大 RSS: `benchmarks/layout` の `-O3` の実行ファイルの maximum resident set size（bytes）。9 回の中央値と最小〜最大を、配列の合計と並べて記録する。
3. 時間: `benchmarks/run-computations.mjs` の `ce_std_option`・`direct_std_option`・`ce_std_result`・`direct_std_result`・`ce_std_option_owned`・`direct_std_option_owned`。runner が出す値を 3 回分記録する。タグの `zext` や niche の比較で遅くならないことの確認に使う。
4. 確保量: PX01 が確保量を記録する仕組みを加えていれば、`benchmarks/layout` の値を記録する。ない場合は配列の合計で代える。

## 変えてはいけない意味

- 評価順序: union の値はペイロードの式を評価してから作る（現在と同じ）。レコードのリテラルと `with` の更新は、フィールドの式をソースの記述順に評価し、物理の位置へ入れるだけ。
- 解放・複製: レコードは論理順（宣言順）にフィールドを解放・複製する。union はペイロードのあるケースだけペイロードを解放・複製し、niche の `None` では何もしない。
- 所有権と借用: 借用したフィールド・ペイロード（`ref record.field` など）のアドレスは、値が動かない間は一定。アラインメントは LLVM の構造体の型が保証する。niche の union のペイロードへの参照は union の場所そのもの。
- 観測できる動作: パターン照合、構造比較（A11）、Hash（A07 の canonical な順序）、Display の文字列、エラーとトラップは変わらない。niche のビット列（null、長さ `-1`）はユーザーのコードから観測できない。
- 数値: 算術を変えないので、整数の折り返し・飽和・丸め・NaN・符号付きゼロは影響を受けない。タグは符号なしとして扱う（`zext`）。
- 診断: E1010 の判定と文言（D4）、公開 ABI の規則（E1008）は変えない。
- 公開 ABI: ホストから見える C の構造体（`record_layout`・`header_types`）は宣言順のまま。
- 決定性: 型定義と添字は入力だけで決まる（安定ソート。`HashMap` の反復順に依存しない）。同じ入力から同じ IR を出し、native と wasm32 で型定義の文字列は同じ。
- WASM: 既定の import を増やさない。

## 設計

### 方針

- 配置の判断は `union_layout` と `record_slots`（新規）の二か所だけで行い、他の箇所はその結果に従う。
- 非再帰の union のタグは `union_tag` だけが読み、`construct_value` だけが書く。`union_tag` は読んだタグを `zext` で `i32` にして返すので、消費側（`case_switch`・`selector`・`switch_plan`）は変えない（D3）。
- レコードの意味は論理順（`record_fields` の順）で決まる。`src/derive.rs` の `instances` もフィールドを論理順に読む式を作る。物理の添字は LLVM の命令を書く直前に `field_slot`（新規）で求める（D9）。
- 各段の後で全テストが通り、計測を記録してから次の段に進む。

### データ構造

`tag_bytes`（新規）、`UnionLayout::General { unit, units }`（`General(usize)` を置き換える）、`UnionLayout::Niche`（新規）、`Niche`（新規）、`record_slots`（新規）、`field_slot`（新規）。すべて `src/llvm.rs` に置く。

```rust
/// Bytes of the stored tag of a non-recursive union with `cases` cases.
fn tag_bytes(cases: usize) -> usize {
    match cases {
        0..=256 => 1,
        257..=65_536 => 2,
        _ => 4,
    }
}

enum UnionLayout {
    /// Every case is nullary, so the value is its tag.
    Enum,
    /// Every payload has the LLVM type of this one: `{ tag, T }`.
    Common(Type),
    /// Payloads of different LLVM types share `{ tag, [units x i(8 * unit)] }`;
    /// `unit` is the largest payload alignment in bytes.
    General { unit: usize, units: usize },
    /// Phase 2: the nullary case is a payload bit pattern that no value uses.
    Niche { payload: Type, niche: Niche, none: usize, some: usize },
}

/// Phase 2.
enum Niche {
    /// A single-pointer reference; `null` is the nullary case.
    NullPointer,
    /// A `{ ptr, i64 }` string, UTF-8 string, or array; a negative length is the nullary case.
    NegativeLength,
}

/// Phase 3a: physical slot of each declared field of a concrete record.
fn record_slots(id: usize, arguments: &[Type], module: &CheckedModule) -> Vec<usize>;

/// Phase 3a: a method next to `union_tag`; tuples return `logical` until Phase 3b.
fn field_slot(&self, aggregate: &Type, logical: usize) -> usize;
```

- ケース数は `module.types().union_payloads(id, arguments).len()`。`union_payloads` はケースごとに一つの `Option<Type>` を返す（`union_layout` の `.flatten()` がこれを前提にしている）。
- `none`・`some` は宣言順のケース番号。タグ `t` は `tag_bytes` の値で、LLVM の型は `i{8t}`。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| 1 | `src/llvm.rs` | `tag_bytes`（新規） | D1 の式。単体テスト `tag_bytes_follow_case_count`（新規） |
| 1 | `src/llvm.rs` | `UnionLayout`・`union_layout` | `General { unit, units }` を D2 の式で作る。doc コメントを新しい形にする |
| 1 | `src/llvm.rs` | `storage_layout` | Enum は `(t, t)`、Common は `(t, t)` とペイロードの aggregate、General は `### アルゴリズム` の式 |
| 1 | `src/llvm.rs` | `emit_program` | 型定義を `i{8t}`・`{ i{8t}, T }`・`{ i{8t}, [units x i{8 unit}] }` にする。enum の別名は構造体の定義より前（既存の順序のまま） |
| 1 | `src/llvm.rs` | `construct_value` | タグの定数を `i{8t}` で書く。General のスピルの型を新しい型にする |
| 1 | `src/llvm.rs` | `union_tag` | 値・場所とも `i{8t}` で読み、`zext` して `i32` を返す。再帰の分岐は変えない |
| 1 | `src/llvm.rs` | `payload_value`・`payload_pointer`・`drop_value`・`clone_value` | 添字 1 の GEP とスピルの形はそのまま。`i32` のタグの直書きがあれば `union_tag` に置き換える |
| 1 | `src/llvm.rs` | `sequence_next` | 非再帰の union のタグの読み出し（`extractvalue … , 0` と `icmp eq i32 {tag}, 1`）を `union_tag` に置き換える |
| 1 | `src/llvm_task.rs` | `parallel_result_tasks` | 非再帰の分岐のタグの読み出しを `union_tag` に置き換える |
| 1 | `src/llvm_debug.rs` | `layout`・`ty` | Enum は `(t, t)`、Common はタグを符号なし `8t` bit の整数として aggregate、General は `### アルゴリズム` の式。タグのメンバーの型とペイロードのメンバーの offset を新しい配置から出す |
| 1 | `src/llvm_control.rs` | `selector`・`switch_plan`・`match_expression` | 変更なし。`union_tag`・`recursive_tag` 経由であることを確かめる |
| 1 | `src/llvm_recursive.rs` | `node_type`・`recursive_tag`・`recursive_construct` | 変更なし（ノードのタグは `i32`。D1） |
| 1 | `src/check.rs`・`src/llvm_frame.rs` | `Layouts`・`stack_size` | 変更なし（D4） |
| 2 | `src/llvm.rs` | `UnionLayout::Niche`（新規）・`Niche`（新規）・`union_layout` | D5 の判定を Enum・Common・General より先に行う |
| 2 | `src/llvm.rs` | `llvm_type`・`storage_layout`・`emit_program` | Niche はペイロードの LLVM 型と `(size, align)` を使い、型定義を出さない（D6） |
| 2 | `src/llvm.rs` | `construct_value` | `Some x` は `x` の値そのもの、`None` は D6 の定数 |
| 2 | `src/llvm.rs` | `union_tag` | `icmp eq ptr … , null` または長さの `icmp slt i64 … , 0` から `select` で `none`・`some` の番号（`i32`）を返す。場所は GEP と `load` で同じ判定をする |
| 2 | `src/llvm.rs` | `payload_value`・`payload_pointer` | 値そのもの・場所そのもの |
| 2 | `src/llvm.rs` | `drop_value`・`clone_value` | `case_switch` 経由のまま。`None` の側では何もしない（複製は定数を返す） |
| 2 | `src/llvm_debug.rs` | `layout`・`ty` | Niche は union の名前の `DW_TAG_typedef` でペイロードの型を指す |
| 3a | `src/llvm.rs` | `record_slots`（新規）・`field_slot`（新規） | D8 の規則。std のレコードと、小さくならないレコードは恒等 |
| 3a | `src/llvm.rs` | `emit_program`・`storage_layout` | レコードの型定義と大きさを物理順で作る |
| 3a | `src/llvm.rs` | `emit_expression_mode`・`emit_place`・`clone_value`・`drop_value`・`io_entry`・`sequence_next` | レコードのフィールドの `extractvalue`・`insertvalue`・`getelementptr` の添字を `field_slot` にする。反復は論理順のまま |
| 3a | `src/llvm_frame.rs` | `field_types`・`frame_value` | フレームに置くリテラルの集約へ書く添字を `field_slot` にする |
| 3a | `src/llvm_abi.rs` | `read_host_record`・`write_host_record` | 内部のレコード側の添字だけ `field_slot`。`record_layout`・`header_types`・`type_definitions` は宣言順のまま |
| 3a | `src/llvm_debug.rs` | `layout`・`fields`・`ty` | メンバーは論理順、offset は物理の配置から、大きさは物理順の aggregate |
| 3a | 手順 10 の監査で見つけた箇所 | レコードのパターン・更新などを LLVM にする関数 | 添字を `field_slot` にする |

### 生成 IR

Phase 1 の `benchmarks/layout` の型定義（HEAD の形は `## 現状と計測`）:

```llvm
%"tz.union.Main.Color" = type i8
%"tz.union.Main.Small" = type { i8, [1 x i8] }
%"tz.union.Main.Shape" = type { i8, [2 x i64] }
%"tz.union.Maybe.Maybe[i64]" = type { i8, i64 }
%"tz.union.Maybe.Maybe[bool]" = type { i8, i1 }
%"tz.union.Maybe.Maybe[string]" = type { i8, %tz.string }
%"tz.union.Maybe.Maybe[ref[Main.Point]]" = type { i8, ptr }
%"tz.union.Result.Result[i64,string]" = type { i8, [2 x i64] }
```

Phase 1 のタグの読み出し（`union_tag` の値の分岐と `case_switch`）:

```llvm
%t = extractvalue %"tz.union.Main.Shape" %shape, 0
%tag = zext i8 %t to i32
switch i32 %tag, label %done [ i32 0, label %circle
                               i32 1, label %rect ]
```

Phase 2 では `Maybe[string]`・`Maybe[ref[Main.Point]]` の型定義がなくなり、値はペイロードの型になる。`{none}`・`{some}` は `Niche` のケース番号:

```llvm
store %tz.string { ptr null, i64 -1 }, ptr %slot
%len = extractvalue %tz.string %o, 1
%is_none = icmp slt i64 %len, 0
%tag = select i1 %is_none, i32 {none}, i32 {some}
%is_null = icmp eq ptr %r, null
%ref_tag = select i1 %is_null, i32 {none}, i32 {some}
```

Phase 3a の型定義。`Mixed` の `field_slot` は a→2、b→0、c→3、d→1、`Tagged` は tag→1、value→0、ok→2。`Point` は小さくならないので恒等:

```llvm
%tz.record.Main.Mixed = type { i64, i64, i1, i1 }
%tz.record.Main.Tagged = type { i64, i8, i1 }
%tz.record.Main.Point = type { i64, i64 }
```

### アルゴリズム

```text
tag_bytes(n) = 1 if n <= 256, 2 if n <= 65_536, 4 otherwise

union_layout(U, arguments):                          # recursive unions never reach here
    cases = union_payloads(U, arguments)             # one Option<Type> per case
    if Phase 2 and niche(cases) = (payload, niche, none, some): return Niche
    payloads = flatten(cases)
    if payloads is empty: return Enum
    if every llvm_type(p) equals llvm_type(payloads[0]): return Common(payloads[0])
    unit  = max(storage_layout(p).align for p in payloads)    # 1, 2, 4, 8, or 16
    bytes = max(storage_layout(p).size for p in payloads)
    return General { unit, units: max(1, ceil(bytes / unit)) }

size and align of General with tag t:
    offset = round_up(t, unit)
    align  = max(t, unit)
    size   = round_up(offset + unit * units, align)

niche(cases):
    require len(cases) == 2, exactly one nullary case, and payload P on the other
    P = Reference in the single-pointer form      -> NullPointer
    P = String, Utf8String, or Array               -> NegativeLength
    otherwise                                      -> none

record_slots(R):
    if R is declared in a std module: return identity
    fields = record_fields(R)                        # declared (logical) order
    order  = stable sort of 0..n by storage_layout(fields[i]).align, descending
    if size(order) >= size(identity): return identity
    slots[order[p]] = p for every physical position p
    return slots
```

`unit` が 2 の冪でない場合は `debug_assert!` で止める（現在の型では起きない）。`size(order)` は `storage_layout` の aggregate をその順で計算した値。

## 実装手順

各手順の後で木がビルドでき、挙げたテストが通る状態にする。Phase 1 は手順 1〜7、Phase 2 は手順 8〜9、Phase 3a は手順 10〜12。
次のコマンドを「全体の確認」と呼ぶ（全部が通ること）。

```sh
cargo test --locked
cargo build --release --locked
for s in e2e features primitives tasks computations debug_info display_parse host_imports gpu; do
  npx --yes --package=node@24 node tests/$s.mjs target/release/tsuzuri
done
```

1. 変更前の計測と workload の追加
   - 変更: `benchmarks/layout/Main.tz`（新規）。
   - 内容: `## 計測手順` の workload を置き、手順 1〜5 を `P=phase1-before` で実行する（コンパイラは未変更）。`## 生成コードの確認` のコマンドも `phase1-before` で実行し、比べる数を残す。
   - 確認: `target/release/tsuzuri run benchmarks/layout -O0` が `16166714` を出力する。`target/perf/PM01/phase1-before/sizes-native.txt` に `32 16 %"tz.union.Main.Shape" = type { i32, [1 x i128] }` の行がある。
2. タグの幅の関数
   - 変更: `src/llvm.rs` の `tag_bytes`（新規）と単体テスト `tag_bytes_follow_case_count`（新規）。
   - 内容: D1 の式。この手順ではまだ呼ばない。
   - 確認: `cargo test --locked --lib tag_bytes_follow_case_count` が `running 1 test` で通る。
3. タグの幅（Enum・Common・General のタグ）
   - 変更: `src/llvm.rs` の `storage_layout`・`emit_program`・`construct_value`・`union_tag`・`sequence_next`、`src/llvm_task.rs` の `parallel_result_tasks`、`src/llvm_debug.rs` の `layout`・`ty`。
   - 内容: D1・D3。General の領域はこの手順では `[K x i128]` のままで、タグだけ `i{8t}` にする。
   - 確認: `cargo test --locked --test union_types` の失敗が IR の文字列の期待値（`i32` のタグ）だけであることを確かめ、期待値を D1 の規則から手で書き直す。再実行で全部通る。`cargo build --release --locked && node tests/e2e.mjs target/release/tsuzuri && node tests/debug_info.mjs target/release/tsuzuri` が通る。この時点で出荷してもよい。
4. タグの直接の読み書きの監査
   - 変更: 見つかった箇所だけ。
   - 内容: `grep -n "load i32, ptr\|store i32 \|icmp [a-z]* i32 {tag\|i32 {case" src/llvm*.rs` の各行を、再帰ノード（`src/llvm_recursive.rs` と各関数の再帰の分岐）、union 以外、手順 3 で直した箇所のどれかに分類し、PR の説明に書く。
   - 確認: 分類できない行が残らない。残ったら停止条件 5。
5. General の領域
   - 変更: `src/llvm.rs` の `UnionLayout`・`union_layout`・`storage_layout`・`emit_program`・`construct_value`・`payload_value`・`drop_value`・`clone_value`、`src/llvm_debug.rs` の `layout`・`ty`。
   - 内容: D2。`General(count)` を `General { unit, units }` にし、コンパイラが指摘する全ての match を直す。
   - 確認: `grep -n "x i128\] }}\|16 + 16 \* count\|16 + count \* 16" src/llvm.rs src/llvm_debug.rs` が何も出さない。`cargo test --locked --test union_types` が通る（期待値は D2 で書き直す）。`cargo build --release --locked && node tests/debug_info.mjs target/release/tsuzuri` が通る。
6. Phase 1 のテスト
   - 変更: `tests/value_layout.rs`（新規）、`tests/fixtures/value_layout/Main.tz`（新規）、`tests/features.mjs`（suite `value_layout`（新規）、GUIDE §7.4）。
   - 内容: `## テスト計画` の Phase 1 の項目。
   - 確認: `cargo test --locked --test value_layout` が `running 3 tests` で通る。`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri value_layout` が native・wasm32 × `-O0`・`-O3` で通る。
7. Phase 1 の全体の確認・計測・文書
   - 変更: `docs/architecture.md`、`docs/benchmarks.md`、`_perfs/README.md`。
   - 内容: 全体の確認、`## 計測手順`（`P=phase1-after`）、`## 生成コードの確認` の Phase 1 の項目、`## ドキュメント`。
   - 確認: 全体の確認が通る。`sizes-native.txt` が `## 目標と指標` の Phase 1 の列と一致し、`diff target/perf/PM01/phase1-before/sizes-native.txt target/perf/PM01/phase1-after/sizes-native.txt` に大きくなった型がない。
8. niche（Phase 2。D5 の承認後）
   - 変更: `## 設計` の段 2 の行。
   - 内容: D5・D6。
   - 確認: `cargo test --locked --test value_layout` が `running 5 tests` で通り、`cargo test --locked --test union_types` が通る（`Maybe` の IR の期待値は D6 で書き直す）。`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri value_layout && node tests/debug_info.mjs target/release/tsuzuri` が通る。
9. Phase 2 の全体の確認・計測・文書
   - 変更: `docs/architecture.md`、`docs/benchmarks.md`、`_perfs/README.md`。
   - 内容: 手順 7 と同じ（`P=phase2-after`、Phase 2 の列）。`## テスト計画` の E2E (g) の Hash の出力を Phase 1 の後のコンパイラと比べる。
   - 確認: 手順 7 と同じ。Hash の出力の `diff` が空。
10. 恒等の並べ替えで全箇所を経由させる（Phase 3a。D7 の承認後）
    - 変更: `src/llvm.rs` の `record_slots`（この手順では常に恒等）・`field_slot` と、`## 設計` の段 3a の行の全箇所。
    - 内容: レコードのフィールドの添字を書く全箇所を `field_slot` に通す。箇所は `grep -n "record_fields" src/llvm*.rs` と、型付き IR のフィールドの読み出し・レコードのリテラル・`with` の更新・パターンを LLVM にする箇所（GUIDE §5 のデータ構造、§6 のチェックリスト）から集める。std のレコードを判定する手段（`src/stdlib.rs` が std として読み込むモジュール）を確かめる。
    - 確認: `## 生成コードの確認` の「恒等の並べ替えの確認」が `identical` を出す。
11. 並べ替えを有効にする
    - 変更: `record_slots` に D8 の規則を入れる。
    - 内容: `## テスト計画` の Phase 3a の項目を加える。
    - 確認: `cargo test --locked --test value_layout` が `running 7 tests` で通る。`cargo build --release --locked && node tests/features.mjs target/release/tsuzuri value_layout && node tests/debug_info.mjs target/release/tsuzuri` が通る。`grep -ln "export def" tests/*.mjs` の suite（公開 ABI を使うもの）が通る。
12. Phase 3a の全体の確認・計測・文書
    - 変更: `docs/architecture.md`、`docs/benchmarks.md`、`_perfs/README.md`。
    - 内容・確認: 手順 7 と同じ（`P=phase3a-after`、Phase 3a の列）。

Phase 3b は D10 の承認の後に手順を立てる。

## 計測手順

GUIDE §14 に従う。コマンドはリポジトリの最上位で zsh か bash で実行し、生データは `target/perf/PM01/$P/` に置く。
`P` は `phase1-before`・`phase1-after`・`phase2-after`・`phase3a-after`。次の段の変更前は前の段の変更後を使う。計測中は他の重い処理を止める。

### 手順 0: workload

`benchmarks/layout/Main.tz`（新規）。既存の構文だけで書いてあり、HEAD のコンパイラで `run -O0` と `-O3` の実行ファイルがどちらも `16166714` を出力することを確かめた。
トップレベルのプログラムは wasm32 の実行ファイルにできない（E2004）ので、wasm32 は手順 2 の大きさだけを測る。

```tsuzuri
union Color = Red | Green | Blue

union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty

union Small = Flag of bool | Code of i8 | Nothing

record Mixed { a: bool, b: i64, c: bool, d: i64 }

record Tagged { tag: i8, value: i64, ok: bool }

record Point { x: i64, y: i64 }

def code :: Color -> i64 = \c ->
    match c with
    | Red -> 1
    | Green -> 2
    | Blue -> 3

def area :: Shape -> f64 = \shape ->
    match shape with
    | Circle r -> r * r * 3.0
    | Rect (w, h) -> w * h
    | Empty -> 0.0

def small :: Small -> i64 = \s ->
    match s with
    | Flag b -> if b then 1 else 0
    | Code c -> if c == 3 then 3 else 0
    | Nothing -> 0

def first_x :: Maybe<ref Point> -> i64 = \o ->
    match o with
    | Some q -> q.x
    | None -> 0

let n = 1000000
let colors = new [Color](n, \i -> if i % 3 == 0 then Red else Green)
let results = new [Result<i64, string>](n, \i -> if i % 2 == 0 then Ok 1 else Ok 2)
let options = new [Maybe<string>](n, \i -> if i < 0 then Some "x" else None)
let mixed = new [Mixed](n, \i -> Mixed { a: i % 2 == 0, b: 1, c: false, d: 2 })
let shapes = new [Shape](n, \i -> if i % 2 == 0 then Circle 1.0 else Rect (1.0, 2.0))
let smalls = new [Small](n, \i -> if i % 2 == 0 then Flag true else Code 3)
let flags = new [Maybe<bool>](n, \i -> if i % 2 == 0 then Some true else None)
let tagged = new [Tagged](n, \i -> Tagged { tag: 1, value: 5, ok: i % 2 == 0 })
let p = Point { x: 7, y: 8 }
let fallback: Maybe<i64> = Some 41
let mut total = first_x (Some (ref p)) + Maybe.default_value 0 fallback
for c in colors do
    total = total + code c
for r in results do
    match r with
    | Ok v -> total = total + v
    | Error _ -> total = total + 0
for o in options do
    match o with
    | Some _ -> total = total + 1
    | None -> total = total + 0
for m in mixed do
    total = total + m.b + m.d
for s in shapes do
    total = total + to_int (area s)
for s in smalls do
    total = total + small s
for f in flags do
    match f with
    | Some b -> total = total + (if b then 1 else 0)
    | None -> total = total + 0
for t in tagged do
    total = total + t.value
total
```

期待値の計算: `first_x` 7、`fallback` 41、colors 333,334×1＋666,666×2＝1,666,666、results 500,000×1＋500,000×2＝1,500,000、options 0、mixed 3×1,000,000＝3,000,000、shapes 500,000×3＋500,000×2＝2,500,000、smalls 500,000×1＋500,000×3＝2,000,000、flags 500,000、tagged 5×1,000,000＝5,000,000。和は 16,166,714。

### 手順 1: 環境の記録

```sh
P=phase1-before
D=target/perf/PM01/$P
mkdir -p $D
{ git rev-parse --short HEAD; git status --short | wc -l; sw_vers; sysctl -n machdep.cpu.brand_string; clang --version | head -1; rustc --version; node --version; } > $D/env.txt
```

### 手順 2: 値の大きさ（決定的。1 回）

```sh
OPT=/opt/homebrew/opt/llvm@21/bin/opt
for target in native wasm32; do
  if [ $target = native ]; then
    DL='e-m:o-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-n32:64-S128-Fn32'
  else
    DL='e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-i128:128-n32:64-S128-ni:1:10:20'
  fi
  target/release/tsuzuri build benchmarks/layout --target $target --emit llvm -O0 -o $D/layout-$target.ll
  grep -E '^%[^ ]+ = type' $D/layout-$target.ll | grep -v opaque > $D/types-$target.txt
  {
    echo "target datalayout = \"$DL\""
    cat $D/types-$target.txt
    n=0
    while IFS= read -r line; do
      name=${line%% = type*}
      n=$((n+1))
      echo "define i64 @size.$n() {"
      echo "  ret i64 ptrtoint (ptr getelementptr ($name, ptr null, i32 1) to i64)"
      echo "}"
      echo "define i64 @align.$n() {"
      echo "  ret i64 ptrtoint (ptr getelementptr ({ i1, $name }, ptr null, i32 0, i32 1) to i64)"
      echo "}"
    done < $D/types-$target.txt
  } > $D/sizes-$target.ll
  paste -d' ' <($OPT -S -passes=instcombine $D/sizes-$target.ll | grep 'ret i64' | awk '{print $3}' | paste -d' ' - -) $D/types-$target.txt > $D/sizes-$target.txt
done
grep -E 'tz\.(union|record)' $D/sizes-native.txt
```

- 各行は「大きさ アラインメント 型定義」。Phase 2 以後は niche の union の行がなくなる（D6）ので、その大きさはペイロードの型の行（`%tz.string` は 16、参照は `ptr` で 8）で読む。
- データレイアウトは LLVM 21 の `aarch64-apple-macosx` と `wasm32-unknown-unknown` のもの。別の計測機では `echo | clang -x c - -S -emit-llvm -o - | grep datalayout` の値を native に使う。

### 手順 3: 最大 RSS（9 回）

```sh
target/release/tsuzuri build benchmarks/layout -O3 -o $D/layout
$D/layout
for k in 1 2 3 4 5 6 7 8 9; do
  /usr/bin/time -l $D/layout 2>&1 >/dev/null | awk '/maximum resident set size/ {print $1}'
done > $D/rss.txt
sort -n $D/rss.txt | sed -n '1p;5p;9p'
```

最初の実行は warm-up を兼ねて `16166714` を出力する。`sed` の 3 行が最小・中央値・最大。

### 手順 4: 時間（3 回）

```sh
for k in 1 2 3; do node benchmarks/run-computations.mjs target/release/tsuzuri > $D/computations-$k.txt; done
grep -h "std_option\|std_result" $D/computations-*.txt
```

出力の形は `docs/benchmarks.md` の computations の節を見る。6 種目の値を 3 回分、表にする。

### 手順 5: Rust の参考値（一度だけ）

`target/perf/PM01/rust_sizes.rs` に次を置き、`rustc -O -o target/perf/PM01/rust_sizes target/perf/PM01/rust_sizes.rs && target/perf/PM01/rust_sizes` を実行する。

```rust
use std::mem::size_of;

#[allow(dead_code)]
enum Color { Red, Green, Blue }
#[allow(dead_code)]
enum Small { Flag(bool), Code(i8), Nothing }
#[allow(dead_code)]
enum Shape { Circle(f64), Rect(f64, f64), Empty }
#[allow(dead_code)]
struct Mixed { a: bool, b: i64, c: bool, d: i64 }
#[allow(dead_code)]
struct Tagged { tag: i8, value: i64, ok: bool }
#[allow(dead_code)]
struct Point { x: i64, y: i64 }

fn main() {
    println!("Color {}", size_of::<Color>());
    println!("Option<bool> {}", size_of::<Option<bool>>());
    println!("Small {}", size_of::<Small>());
    println!("Option<i64> {}", size_of::<Option<i64>>());
    println!("Option<&Point> {}", size_of::<Option<&Point>>());
    println!("Option<String> {}", size_of::<Option<String>>());
    println!("Result<i64, String> {}", size_of::<Result<i64, String>>());
    println!("Shape {}", size_of::<Shape>());
    println!("Mixed {}", size_of::<Mixed>());
    println!("Tagged {}", size_of::<Tagged>());
}
```

### 手順 6: 記録

`docs/benchmarks.md` の「値のレイアウト（PM01）」の節に、段ごとの型の大きさ（native・wasm32）、RSS の中央値と最小〜最大、配列の合計、時間の 6 種目、計測機・OS・コミット・Clang・rustc・Node の版、生データの場所を書く。実装済みの段と計画の段を分けて書く。

## 生成コードの確認

```sh
D=target/perf/PM01/phase1-after
target/release/tsuzuri build benchmarks/layout --emit llvm -O0 -o $D/layout-O0.ll
target/release/tsuzuri build benchmarks/layout --emit llvm -O3 -o $D/layout-O3.ll
grep -E '^%"?tz\.(union|record)' $D/layout-O0.ll
grep -c 'zext i8 ' $D/layout-O0.ll
grep -c 'load i8, ptr' $D/layout-O3.ll
/opt/homebrew/opt/llvm@21/bin/llvm-objdump -d --no-show-raw-insn $D/layout > $D/layout.s
grep -c 'ldrb' $D/layout.s
```

`$D/layout` は `## 計測手順` の手順 3 で作った実行ファイル。段ごとに見るもの:

- Phase 1: 型定義が `### 生成 IR` の Phase 1 の形で、`[1 x i128]` がない。`zext i8` が 1 以上。`-O3` の IR の `load i8, ptr` とアセンブリの `ldrb` が `phase1-before` より多い（`colors` の要素を 1 byte で読む）。多くならない場合は、畳み込みで読み出しが消えたのかを IR で確かめて記録する。
- Phase 2: `-O0` の IR に `Maybe[string]`・`Maybe[ref[Main.Point]]` の型定義がない。`first_x` に `icmp eq ptr`、options のループに `icmp slt i64` がある。
- Phase 3a: 型定義が `### 生成 IR` の Phase 3a の形。`-O0` の IR で `m.b` の読み出しが添字 0、`m.d` が添字 1、`t.value` が添字 0（`extractvalue` または `getelementptr … i32 0, i32 N`）。

### 恒等の並べ替えの確認（手順 10）

変更の前に最初の 2 行を実行し、手順 10 の変更と `cargo build --release --locked` の後に残りを実行する。

```sh
mkdir -p target/perf/PM01/ir-before target/perf/PM01/ir-after
cp target/release/tsuzuri target/perf/PM01/tsuzuri-before
for d in tests/fixtures/*/ benchmarks/layout/ benchmarks/computations/; do
  n=$(echo "$d" | tr '/' '_')
  target/perf/PM01/tsuzuri-before build "$d" --emit llvm -O0 -o "target/perf/PM01/ir-before/$n.ll" 2>/dev/null
  target/release/tsuzuri build "$d" --emit llvm -O0 -o "target/perf/PM01/ir-after/$n.ll" 2>/dev/null
done
diff -r target/perf/PM01/ir-before target/perf/PM01/ir-after && echo identical
```

`Main.tz` のない fixture は両方とも失敗してファイルができないので、比較から外れる。

## テスト計画

### Rust テスト

- `src/llvm.rs` の `tag_bytes_follow_case_count`（新規、Phase 1）: ケース数 1→1、256→1、257→2、65,536→2、65,537→4。
- `tests/value_layout.rs`（新規）。IR は `tests/union_types.rs` の `lowers_layouts_constructors_and_tag_switches` と同じ補助関数で得る。期待する型定義の行は `### 生成 IR` の規則から手で書く。
  - `union_tags_use_the_smallest_width`（新規、Phase 1）: `Color` が `= type i8`、`Maybe<i64>` が `{ i8, i64 }`。Rust で生成した 300 ケースの union（299 個のペイロードなしと `of i64` 一つ）が `{ i16, i64 }`。
  - `general_unions_size_the_area_by_payload_alignment`（新規、Phase 1）: `Shape` が `{ i8, [2 x i64] }`、`Small` が `{ i8, [1 x i8] }`、`Result<i64, string>` が `{ i8, [2 x i64] }`、`i128` と `bool` のペイロードを持つ union が `{ i8, [1 x i128] }`。
  - `recursive_union_nodes_keep_i32_tags`（新規、Phase 1）: `tests/recursive_types.rs` が検査している再帰ノードの型の行が変わらない（同じソースと期待値を写す）。
  - `option_like_unions_use_niches`（新規、Phase 2）: `Maybe<string>`・`Maybe<utf8string>`・`Maybe<[i64]>`・`Maybe<ref Point>` の型定義がなく、関数の引数の型がペイロードの型（`%tz.string`・`%tz.utf8string`・`%tz.array`・`ptr`）。`Option<i64>` は `{ i8, i64 }` のまま。ペイロードなし二つと `of string` の 3 ケースの union は niche にならず `{ i8, %tz.string }`。
  - `nested_options_use_one_niche`（新規、Phase 2）: `Maybe<Maybe<string>>` の外側が `{ i8, %tz.string }`。
  - `records_are_sorted_by_alignment`（新規、Phase 3a）: `Mixed` が `{ i64, i64, i1, i1 }`、`Tagged` が `{ i64, i8, i1 }`、`Point` が `{ i64, i64 }`、小さくならない `{ a: i64, b: bool }` が宣言順の `{ i64, i1 }`。型引数を 3 つ持つジェネリックのレコードで、`<bool, i64, bool>` の具体化が `{ i64, i1, i1 }`、`<i64, bool, bool>` が宣言順。
  - `record_order_is_deterministic`（新規、Phase 3a）: 同じソースを 2 回 IR にして一致する。
- 既存の `tests/union_types.rs` の `enforces_union_layout_limits` は期待値を変えずに通る（E1010 の拒否の例を含む）。新しい診断はない。

### E2E

`tests/fixtures/value_layout/Main.tz`（新規）と `tests/features.mjs` の suite `value_layout`（新規、GUIDE §7.4）。native・wasm32 × `-O0`・`-O3`、`live == 0`、WASM の import が空。
トップレベルのプログラムはそのままでは wasm32 にできない（E2004）ので、既存の suite と同じ形（IO の main か `export def`）にする。期待値は下の計算で手で書き、コンパイラの出力から写さない。

- (a) Phase 1: `benchmarks/layout/Main.tz` の計算を `n = 1000` にしたもの。期待値 `16214`（`first_x` 7、`fallback` 41、colors 334×1＋666×2＝1,666、results 500×1＋500×2＝1,500、options 0、mixed 3,000、shapes 500×3＋500×2＝2,500、smalls 500×1＋500×3＝2,000、flags 500、tagged 5,000 の和）。
- (b) Phase 1: 256 ケースの union（`V0`〜`V254` はペイロードなし、`V255 of i64`）で `V0`・`V127`・`V128`・`V255 42` を作り、ペイロードなしはケース番号、`V255` はペイロードを返す関数で合計して `297`。300 ケースの union（`W0`〜`W298` はペイロードなし、`W299 of i64`）で `W128`・`W255`・`W256`・`W298`・`W299 7` から同様に `944`。128 番以上が正しい枝に行くこと（`zext`）を確かめる。
- (c) Phase 1: `bool`・`i8`・`char`・`i16`・`i32`・`i64`・`f64`・`i128`・`string`・`[i64]`・関数値・`i64 * bool` のタプルをペイロードに持つ一つの union の値を配列に入れ、各値から整数を取り出して合計する（期待値は入れた値から手で計算する）。文字列・配列・関数値の解放を `live == 0` で確かめる。
- (d) Phase 2: `Maybe<string>` の `Some ""`・`Some "abc"`・`None` の長さの和 `3`。`Maybe<utf8string>` も同じ値で（リテラルの書き方は GUIDE §12）。`Maybe<[i64]>` の `Some []`・`Some [1, 2]`・`None` で `2`。`Maybe<ref Point>` の `Some`（`x = 7`）と `None` で `7`。`Maybe<Maybe<string>>` の `Some (Some "ab")`・`Some None`・`None` をそれぞれ 10・20・30 に写した和 `60`。
- (e) Phase 2: `Maybe<string>` を持つレコード・配列の複製と解放（`live == 0`）。`match` でペイロードを借りて長さを読む。
- (f) Phase 3a: `Mixed { a: true, b: 11, c: false, d: 22 }` の各フィールドの読み出し（1・11・0・22）、`{ m with d = 33 }` の後の `d`、可変のフィールドへの代入、借りたフィールドを関数に渡す、`new [Mixed]` の配列の合計。`Tagged { tag: 3, value: 44, ok: true }` も同じ。
- (g) Phase 2・3a: `deriving` した Eq・Ord・Hash・Display を持つ union とレコード（`Mixed` と同じ形）で、比較の結果と Display の文字列を `docs/language.md` の表示形式から手で書く。Hash の値は期待値に入れず、同じ fixture を段の変更前のコンパイラでも実行して、出力の `diff` が空であることを確かめる。
- (h) Phase 3a: 公開 ABI。スカラーだけのレコード `{ a: i8, b: i64, c: i8 }`（並べ替え後 `{ i64, i8, i8 }`）を受け取って返す `export def` を、E05 のテスト（`_features/_completed/E05-host-abi-buffers.md` のテスト計画）と同じ方法でホストから呼び、各フィールドが保たれる。

### 既存テストへの影響

- Phase 1: `tests/union_types.rs` の `lowers_layouts_constructors_and_tag_switches`（型定義・`insertvalue … i32 N, 0`・タグの読み出しの文字列）と、該当すれば `switch_plans_read_payload_projections`。`grep -n "type { i32\|= type i32\|i128\]" tests/*.rs tests/*.mjs` に一致する `tests/features.mjs`・`tests/gpu.rs`・`tests/types_ownership.rs` の行のうち、非再帰の union の型を検査しているものだけ。期待値は D1・D2 の規則から手で書き直す。
- Phase 2: 上に加え、`Maybe` の形（`Maybe<string>`・参照など）の IR を検査する行。
- Phase 3a: 並べ替えで小さくなるレコードの型定義を検査する行。
- 変わらないもの: `enforces_union_layout_limits`、`tests/recursive_types.rs`、`tests/debug_info.mjs`、全 E2E の出力。変わったら停止条件 2・3。

### 性能

計測は `## 計測手順` だけで行い、テストに時間・RSS・大きさの閾値を入れない。

## ドキュメント

- `docs/architecture.md`: union の表現（`UnionLayout` の各形）、再帰ノードのタグ、DWARF の union、レコードの配置（Phase 3a）の記述。`grep -n "i128\]\|UnionLayout\|DW_TAG\|宣言順" docs/architecture.md` で箇所を探し、段ごとに実装済みの形だけを書く。
- `docs/benchmarks.md`: 「値のレイアウト（PM01）」の節（新規）。`## 計測手順` の手順 6 の内容。
- `docs/language.md`: 変更しない（E1010 の見積もりと公開 ABI の規則は同じ。D4）。
- `_perfs/README.md`: 「メモリと成果物サイズ」の値の表現の箇条（ペイロードの型が異なる union の 16 bytes 単位の領域）を段ごとに更新し、状態を運用ルールどおりに変える。
- `_docs/`: 変更しない（利用者に見える機能はない）。

## 受け入れ条件

- [ ] 着手した段の決定事項（Phase 1 は D1・D2、Phase 2 は D5、Phase 3a は D7）の承認を得ている。
- [ ] 段ごとに `sizes-native.txt`・`sizes-wasm32.txt` が `## 目標と指標` の列と一致し、大きくなった型がない。
- [ ] `cargo test --locked` が通り、`tests/value_layout.rs` がその段までのテスト（Phase 1 は 3、Phase 2 まで 5、Phase 3a まで 7）を実行している。
- [ ] `node tests/features.mjs target/release/tsuzuri value_layout` が native・wasm32 × `-O0`・`-O3` で通り、`live == 0`、WASM の import が空。
- [ ] `## 実装手順` の全体の確認がすべて通り、期待値の変更は `### 既存テストへの影響` に挙げたものだけ。
- [ ] `tests/debug_info.mjs` が期待値を変えずに通る。
- [ ] E1010 の判定、`stack_size`、公開 ABI の C の構造体が変わらない。
- [ ] 最大 RSS と時間を `## 計測手順` で計測し、`docs/benchmarks.md` に記録した。時間が悪化した種目があれば判断を仰いだ。
- [ ] `## 生成コードの確認` の形を確かめた。
- [ ] `docs/architecture.md` が実装済みの形だけを説明している。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

1. opaque pointer では、`i8` のタグの場所に `load i32, ptr %slot` を書いても LLVM の検証に通り、パディングを含む 4 bytes を黙って読む。`insertvalue`・`extractvalue` の型の食い違いはビルドで失敗するが、`load`・`store` は失敗しない。手順 4 の監査を省かない。
2. タグは `zext` で広げる。`sext` では 128〜255 番のケースが負になり、`switch` が別の枝へ行く。E2E の (b) で 128 番以上を必ず通す。`i8 200` のような定数の表記は LLVM で有効。
3. enum の別名（`= type i8`）は構造体の型定義より前に出す（既存の規則。別名は前方参照できない）。
4. `storage_layout` はポインターを 8 bytes と見積もるので、wasm32 の `General` の領域は必要より大きいことがある。正しさには影響しない。target ごとに変えると型定義の文字列が target で変わるので、この ticket では変えない。
5. 最大 RSS は書き込まれないページを数えない。HEAD の `benchmarks/layout` は配列の合計 188,000,000 bytes に対して RSS 165,904,384 bytes だった。Phase 2 の `None`（長さ `-1`）はゼロでないので、`Maybe<string>` の配列が常駐するようになり、値が小さくなっても RSS が増えて見えることがある。RSS は配列の合計と並べて記録し、増えた場合はこの理由かを IR の `store` の値で確かめる。
6. Phase 2 では `zeroinitializer` は `None` ではない（`%tz.string` のゼロは `Some ""`）。`None` は必ず D6 の定数で作り、ゼロ埋めしたメモリを union の値として使わない。参照の niche ではゼロ（null）が `None` になる。
7. `Maybe<Maybe<string>>` で niche を二重に使わない。内側だけが niche で、外側は `{ i8, %tz.string }`。
8. `General` のペイロードはメモリ経由（スピル）で読み書きする。領域の型を変えるときは、スピルの `alloca` の型と GEP の型を同じ新しい型にする。
9. ジェネリックのレコードは型引数でアラインメントが変わる。順序は具体化ごとに計算し、型の名前（`canonical_type`）は変えない。
10. 同じ型のフィールドの添字を取り違えても型の検証には通る（`Mixed` の `b` と `d`）。E2E の値はフィールドごとに違う値にする（`b = 11`、`d = 22`）。
11. レコードのリテラルの評価順はソースの記述順のまま。物理順に評価し直さない。
12. DWARF のメンバーは論理順に並べ、offset だけを物理の配置にする。メンバーを物理順に並べるとデバッガーの表示順が変わる。
13. `cargo test --locked <pattern>` は一致するテストが 0 個でも成功する。`running N tests` の N を確かめる。
14. Node 20 は重い suite で V8 が異常終了することがある。`npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri` を使う。
15. テストのプロジェクトは一つずつ別の一時ディレクトリに置く（E03 の再帰的なソースの探索）。
16. 新しい関数を深く再帰させない。`storage_layout` と `record_slots` の再帰は型の入れ子の深さまで。デバッグビルドのテストは 2 MiB のスタックで動く。

## 対象外

- 値のビット単位の詰め込み（`bool` の配列を 1 bit ずつにするなど、要素のアドレスを変える変更）。
- `bool`・`utf8char`・列挙型の未使用のタグ・関数値のコードポインターの niche（D5 の見直し提案）。関数値の表現は PM02。
- ペイロードを持つケースが二つ以上の union の niche（Rust の `Result<i64, String>` の形）。
- 再帰 union のノードの配置とタグの幅（D1）。
- E1010 の見積もりを実際の大きさに合わせる変更（D4）。
- wasm32 に合わせた `storage_layout`（落とし穴 4）。
- std のレコードの並べ替え（D8。計測の後に個別に判断する）。
- タプルと union のペイロードのタプルの並べ替え（Phase 3b。D10 の承認後）。
- 文字列（PM03）、リスト（PM04）、allocator（PM05）の表現。

## 決定事項

### D1: タグの幅

- 決定: 非再帰の union のタグは、ケース数 n が 256 以下なら `i8`、65,536 以下なら `i16`、それより多ければ `i32`（`tag_bytes`）。ケース番号は宣言順の 0 始まりのまま。`i1` は使わない。再帰 union のノードのタグ（`node_type` の `i32`）は変えない。
- 理由: 列挙型と小さいペイロードの union が 3〜30 bytes 縮む。`i1` はメモリ上の大きさが `i8` と同じで、0・1 以外のビット列の扱いに注意が要る。再帰ノードは先頭に 3 つのポインターがあり、ペイロードのアラインメントが 8 ならタグを縮めてもノードの大きさは変わらない。
- 状態: 要承認（承認前は Phase 1 に着手しない）。内部表現の変更（`_perfs/README.md` の運用ルール）。承認のときに GUIDE §9 D-05 の記述と矛盾しないかも確かめる。

### D2: General の領域

- 決定: `{ i{8t}, [units x i{8 unit}] }`。`unit` は最大のペイロードのアラインメント（`storage_layout` の値）、`units` は最大のペイロードの大きさを `unit` で切り上げた数（最小 1）。タグは添字 0、領域は添字 1 のまま。値は現在と同じく `zeroinitializer` から作る。
- 理由: 添字の位置を保つので、`payload_pointer` の `getelementptr inbounds {type}, ptr {slot}, i32 0, i32 1` とスピルの形が変わらない。領域の要素の整数型がアラインメントを持つので、他の構造体に入っても正しく並ぶ。native と wasm32 で同じ型定義になる。
- 状態: 要承認（D1 と同時に承認を受ける。承認前は Phase 1 に着手しない）

### D3: タグは `union_tag` に集め、`i32` で返す

- 決定: 非再帰の union のタグは `union_tag` だけが読み、読んだ直後に `zext` で `i32` にする（`i32` のタグはそのまま）。`sequence_next` と `parallel_result_tasks` の直接の読み出しは `union_tag` に置き換える。タグを書くのは `construct_value` だけ。
- 理由: 消費側（`case_switch` の `switch i32`、`selector`、`switch_plan`、比較の定数）を変えずに済み、変更が最小になる。`-O3` では LLVM が `zext` と比較を狭い命令に畳む（`## 生成コードの確認` で確かめる）。
- 状態: 既定案（実装者はこの案に従う）

### D4: E1010 の見積もりと `stack_size` は変えない

- 決定: `src/check.rs` の `Layouts`、`src/llvm_frame.rs` の `stack_size`、`docs/language.md` の上限の記述を変えない。
- 理由: 両者は `docs/language.md` の保守的な見積もりで、新しい配置は全段でこの見積もり以下になる。変えると受理されるプログラムが増え、言語の見える動作が変わる。
- 状態: 既定案（実装者はこの案に従う）。見積もりを実際の大きさに合わせる変更は、別の承認事項として対象外にする。

### D5: niche の対象（Phase 2）

- 決定: 再帰でなく、ケースがちょうど二つで、一方がペイロードなし、他方のペイロード P が次のどれかの union だけを niche にする。std の `Maybe` に限らず、この形のユーザーの union にも使う。
  - P が一つのポインターで表される `Type::Reference`（16 bytes の配列の共有参照の形は除く）: null をペイロードなしのケースにする（`Niche::NullPointer`）。
  - P が `Type::String`・`Type::Utf8String`・`Type::Array(_)`: 長さ（添字 1）が負の値をペイロードなしのケースにする（`Niche::NegativeLength`）。書く値は `-1`、判定は `icmp slt i64 %len, 0`。
  - P 自体が niche の union（`Maybe<Maybe<string>>` の内側など）なら、外側は Phase 1 の表現にする。
- 理由: 空の文字列・配列のデータのポインターが null にならないことは確かめていない（ランタイムを含む全経路の監査が要る）ので、ポインターの null は使わない。長さは常に 0 以上で、負の値は作られない。参照は生きている場所を指すので null にならない。効果の大きい `Maybe<string>`（24 → 16）と `Maybe<ref T>`（16 → 8）を最小の変更で得る。
- 状態: 要承認（承認前は Phase 2 に着手しない）。デバッガーでの `Maybe` の見え方も変わる（D6）。
- 見直し提案: 元の案の候補のうち、`bool`・`utf8char`・列挙型の未使用のタグ・関数値のコードポインターは Phase 2 から外した。前の三つは縮む量が 1〜3 bytes でペイロードの LLVM 型を変える必要があり、関数値は PM02 が表現を変えるため。Phase 2 の計測の後、必要なら別の段として承認を求める。

### D6: niche の union の LLVM 型と値

- 決定: niche の union は名前付きの型（`%"tz.union…"`）を持たず、`llvm_type` はペイロードの LLVM 型（`ptr`・`%tz.string`・`%tz.utf8string`・`%tz.array`）を返す。`emit_program` は型定義を出さない。`Some x` の値は `x` そのもの、`None` は定数（`ptr null`、`%tz.string { ptr null, i64 -1 }` など）。`union_tag` は `select` でケース番号を返し、`payload_value` は値そのもの、`payload_pointer` は union の場所そのもの。DWARF は union の名前の `DW_TAG_typedef` でペイロードの型を指す。
- 理由: LLVM の名前付き構造体は名前で区別されるので、同じ本体の別の型を作るとペイロードとの間で詰め替えが要る。ペイロードの型をそのまま使えば、構築と分解に命令が要らない。`%A = type %B` の形の別名にも頼らない。
- 状態: 既定案（実装者はこの案に従う）

### D7: フィールドの並べ替えの既定（旧・未決事項）

- 決定: 既定で並べ替える。最適化レベル（`-O0`〜`-O3`）と target（native・wasm32）に関係なく同じ順序にする。DWARF はメンバーを宣言順に並べ、物理の offset で対応する。並べ替えを止めるオプションは作らない。
- 理由: 内部のレコードの LLVM 表現は安定 ABI ではない（`docs/language.md` の公開 ABI）。レベルや target で順序を変えると、IR の差とデバッグのずれが増える。
- 状態: 要承認（承認前は Phase 3a に着手しない）

### D8: 並べ替えの規則と対象

- 決定: 具体的なレコードの型ごと（ジェネリックは具体化ごと）に、フィールドを `storage_layout` のアラインメントの降順で安定ソートする（同じなら宣言順）。宣言順より大きさが小さくなる場合だけ採用し、そうでなければ宣言順のまま。除外するのは std のモジュールで宣言されたレコードだけ。公開 ABI のレコードも並べ替え、ホスト側の C の構造体（`record_layout`・`header_types`）は宣言順のまま、`read_host_record`・`write_host_record` の内部側の添字だけを `field_slot` にする。
- 理由: 小さくならないレコード（`Point { x: i64, y: i64 }` など）の IR を変えないので、既存の期待値とデバッグの見え方が保たれる。std のレコードには生成コードやランタイムが特別に扱うもの（E05 の buffer の判定 `src/abi.rs`、GPU の buffer `src/gpu.rs`、`io_entry`）があり、Phase 3a では個別に監査しない。ホストから見えるのは `record_layout` が作る C の構造体で、内部のレコードではない。GPU の buffer の要素はスカラーに限られ、SIMD の型はレコードではないので、ほかに外から見える配置はない。
- 状態: 既定案（実装者はこの案に従う）

### D9: 意味は論理順（宣言順）で決める

- 決定: `record_fields` の順を論理順とし、評価・解放・複製の順、パターン照合、`deriving` の比較・Hash・Display、DWARF のメンバーの順、公開 ABI の C の構造体はすべて論理順のままにする。物理の添字は LLVM の `extractvalue`・`insertvalue`・`getelementptr` を書く直前に `field_slot` で求める。
- 理由: 観測できる動作が配置に依存しないことを、コードの構造で保証する。
- 状態: 既定案（実装者はこの案に従う）

### D10: タプルと union のペイロードのタプルの並べ替え（Phase 3b）

- 決定: Phase 3a では行わない。行う場合は `field_slot` のタプルの分岐を有効にし、`Type::Tuple` を扱う箇所（`src/llvm.rs` の `llvm_type`・`canonical_type`・`storage_layout`・`clone_value`・`drop_value`、`src/llvm_compare.rs` の `structural_compare`、`src/llvm_hash.rs` の `structural_hash`、`src/llvm_display.rs` の `structural_display`・`display_quoted`、`src/llvm_debug.rs` の `fields`・`layout`、`src/llvm_frame.rs` の `field_types`・`stack_size`、`src/llvm_bulk.rs` の `vector_builtin`）を監査する。ランタイムの集約（`@tz.string.decode_utf8` の `{ i32, i64 }`）と直接結び付くタプルは除外する。
- 理由: タプルは無名の構造体で、ランタイムの戻り値と添字で対応する箇所がある。元の案（タプルと union のペイロードも並べ替える）は保ち、レコードでの計測を見てから判断する。
- 状態: 要承認（承認前は Phase 3b に着手しない）

### D11: 計測の workload

- 決定: `benchmarks/layout/Main.tz` を追加し、全段で値の大きさと最大 RSS の計測に使う。runner は作らず、コマンドは `## 計測手順` のとおりにする。時間は既存の `benchmarks/run-computations.mjs` を使う。
- 理由: 全段で同じプログラムを使えば差をそのまま比べられる。実アプリ型の workload は PX02 の範囲。
- 状態: 既定案（実装者はこの案に従う）
