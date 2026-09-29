# C09: HashMap／HashSet

| 項目 | 内容 |
| --- | --- |
| ID | C09 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | A07, C02, C06 |
| 後続 | D08 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要（std 名 `HashMap`／`HashSet` は GUIDE D-30 の仮割り当てを使う）。Phase 2 は要承認: D11（seed 付きハッシュと乱数源。E08 `Random` とホストの seed 設定） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: 連想コンテナが挿入・削除 O(n) の順序付き Map だけ（C# `Dictionary`、Rust `HashMap`、C++ `unordered_map` 相当がない） |
| 手本にする既存実装 | 不透明な std record と Vec の上の実装: `std/Map.tz`（`private record Entry`、`record Map`、`lower_bound` の反射性 `assert`、`insert` の `Vec.swap`／`Vec.pop`／`Vec.push` による代表値の保持、`fold`、`iter_from`／`iter`）、`std/Set.tz`。登録: `src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`opaque_record`。検査: `tests/map_set.rs` の `ordered_containers_are_opaque_noncopy_owned_values`。E2E: `tests/fixtures/map_set/Main.tz` と `tests/features.mjs` の `map_set` suite（JavaScript `Map` の参照と `inspect`） |
| 主な影響ファイル | `std/HashMap.tz`（新規）, `std/HashSet.tz`（新規）, `src/stdlib.rs`（`SOURCES`, `RESERVED_MODULES`, `opaque_record`, テスト `reserves_the_d07_table`）, `tests/hash_map.rs`（新規）, `tests/fixtures/hash_map/Main.tz`（新規）, `tests/features.mjs`（suite `hash_map`（新規））, `docs/language.md`（`### Map / Set` の直後に `### HashMap / HashSet`（新規））, `docs/architecture.md`, `_docs/library-reference/hash-map.md`（新規）, `_docs/library-reference/README.md`, `_docs/library-reference/map-set.md`, `_docs/library-reference/api/`（`tsuzuri doc` で再生成）, `_docs/README.md`, `_docs/feature-status.md`, `_features/README.md`, `_features/GUIDE.md`（D-07 の表と D-30 の行） |

## 目的

平均 O(1) の挿入・検索・削除を持つ連想コンテナ `HashMap<'key, 'value>` と集合 `HashSet<'key>` を標準で提供する。
反復順序はハッシュ関数に依存させず、挿入と削除の列だけで決まる決定的な順序にする。
C06 の `Map`／`Set`（検索 O(log n)、挿入・削除 O(n)）を置き換えるのではなく並べて置く。キーの順序が必要なら `Map`、速さが必要なら `HashMap` を使う。

HashDoS（衝突を狙った入力）への耐性は Phase 1 では持たない（D10）。鍵付きハッシュと乱数 seed は Phase 2（D11、要承認）で扱う。
実装者は Phase 1 だけを実装する。Phase 2 は人間が求め、D11 が承認された場合だけ着手する。

## 着手条件と停止条件

### 着手条件

- A07・C02・C06 が `_features/README.md` の状態欄で done であること（HEAD `f8dc655` で 3 件とも done）。
  確認: `grep -nE "^\| (A07|C02|C06) " _features/README.md`。
- Phase 1 に承認は要らない。std 名 `HashMap`／`HashSet` は GUIDE D-30 の仮割り当てで、実装手順 12 で D-07 の表へ移す。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（HashMap を使わない IR と 4 つの上限テスト）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- std のソースだけでは書けず、`src/check.rs`・`src/llvm.rs`・`src/polymorph.rs` など `src/stdlib.rs` 以外のコンパイラの変更が要る
  （新しい `Builtin`、新しい `Type` の variant、借用規則の緩和、ランタイム C の変更）。
- 手順 1 で保存した HashMap を使わないプログラムの IR が、byte 単位で変わった。
- `honors_the_exact_specialization_limit`、stack-depth の 3 テスト（`bounds_type_growing_polymorphic_recursion`、
  `bounds_recursive_and_flat_expression_depth`、`bounds_nested_builder_expansion_not_just_source_syntax`）のどれかが失敗する。
  または上限・stack サイズを上げたくなった。
- `src/stdlib.rs` の `reserves_the_d07_table` の件数（20 → 22）以外の既存テストの期待値を変える必要がある。
- 既存の利用者 fixture・benchmark・文書のサンプルが `HashMap`／`HashSet` をモジュール名や型名に使っていて、予約で `E1011` になる
  （HEAD では該当なし。確認: `grep -rn "HashMap\|HashSet" tests benchmarks examples _docs --include=*.tz`）。
- E2E が JavaScript の参照と一致しない。参照の側を実装の出力へ合わせてはいけない。
- `unsafe`、新しい crate、既定の WASM import、`Hash.hash` の出力（D-26 の canonical ストリームと FNV-1a）の変更が必要になった。

## 現状（HEAD `f8dc655` で確認）

- `Map`（`std/Map.tz`）は `private record Entry<'key, 'value> { key: 'key, value: 'value }` と
  `record Map<'key, 'value> { entries: Vec<Entry<'key, 'value>> }`。キー昇順の密な Vec を `lower_bound` で二分探索する。
  `lower_bound` は最初に `assert (Eq.eq key key)` で問い合わせキーの反射性（NaN）を検査し、比較のたびに格納キーの反射性も検査する。
  `insert` は既存キーなら `Vec.swap` で末尾へ寄せ、`Vec.pop` で取り出して `Entry { key: old.key, value: value }` を push し直す（最初の代表キーを保持）。
  新しいキーと `remove` は `Vec.swap` の繰り返しで shift するので O(n)。`Set`（`std/Set.tz`）も同じ形。
- 不透明性: `src/stdlib.rs` の `opaque_record` が `"Map.Map" | "Map.Entry" | "Set.Set" | ...` を列挙し、`src/check.rs` の
  `record_storage` が定義モジュールの外からの構築・field・pattern・update を `E1022`
  「the representation of '{}' is opaque; use its module API」で拒否する。
- std の読み込み: `src/stdlib.rs` の `SOURCES`（`include_str!` の一覧）と `RESERVED_MODULES`（20 件。テスト `reserves_the_d07_table` が
  件数を固定する）。`embedded_sources_use_reserved_flat_modules` は std のファイルが予約名であることを、
  `std_definitions_never_shadow_builtin_functions` は std の関数名が `Builtin` の名前と重ならないことを検査する。
  GUIDE D-07 により、未使用の std 関数は IR に出ない（型検査は常に行う）。
- `Hash`: `src/polymorph.rs` の `BUILTIN_CLASSES` にある組み込みクラスで、`Hash.hash :: ref 'a -> i64u`。組み込み型の instance は
  `src/check.rs` の `Builtin::Hash`（`$builtin.hash`）と `src/llvm_hash.rs`、導出 instance は `src/derive.rs`（`hash_start`、`mix` が
  `Builtin::HashMix`＝`$builtin.hash_mix` を呼ぶ）。鍵なし FNV-1a 64-bit（offset `14695981039346656037`）で、±0・NaN・decimal cohort を
  正規化する（GUIDE D-26）。
- FNV-1a の下位 k bit は入力 byte の下位 k bit だけで決まる。そのため `Hash.hash` をそのまま 2 の冪の表の添字に使うと、
  8 の倍数のキーがすべて同じ位置へ集まる（下の再現）。C09 では固定の混合関数を必ず通す（D5）。
- Vec: `Vec.with_capacity`、`Vec.push`、`Vec.pop`（`(残り, Option<T>)`）、`Vec.set`（既存位置の置換）、`Vec.swap`、`Vec.length` がある。
  容量は 0 から初回 4、以後倍増。負の容量、長さ・容量・byte 数の overflow、確保失敗はトラップ（`_docs/library-reference/vec.md`）。
- `Map`・`Set` は Vec を持つので非 Copy・`needs_drop`。`export def bad :: Map<i64, i64>` は `E1008`（`tests/map_set.rs`）。
- 試作（`/tmp/tz-work-C09/proto3/`。利用者モジュール `HashMap.tz`・`HashSet.tz` として D1〜D9 の案を書いたもの）は release コンパイラで
  `run` が成功した。ここで次を確かめた: `(Hash<'key>, Eq<'key>)` 制約の generic 関数、`match map with | HashMap { entries = entries, slots = slots } -> ...`
  による所有 record の分解、`i64u` 文脈の `0xff51afd7ed558ccd`、`>>>`、`Vec.set`、`HashMap.HashMap<'key, unit>` と `()`。
- 同じ試作の前版で、削除を墓標（`Vec<Option.Option<Entry<...>>>`）にして `at` を `match ref map.entries[index] with` で書くと
  `E1013`「a borrowed value cannot outlive its pattern or iteration binding」になった。`ref 'value` を返す API は密な entry 配列でしか書けない（D3）。

### 再現（検証済み）

`/tmp/tz-work-C09/lowbits/Main.tz` に次を置き、`target/release/tsuzuri run /tmp/tz-work-C09/lowbits` を実行すると `31` を出力する
（8 から 248 までの 8 の倍数 31 個すべてで、`Hash.hash` の下位 3 bit が 0 のものと一致する）。

```tsuzuri
let zero = 0
let base = Hash.hash (ref zero) & 7
let mut index = 1
let mut same = 0
while index < 32 do
    let key = index * 8
    if (Hash.hash (ref key) & 7) == base then same = same + 1
    index = index + 1
same
```

## 仕様

### 段階

- Phase 1（実装対象）: std の不透明 record `HashMap<'key, 'value>`／`HashSet<'key>`。密な entry 配列と開番地法の添字表、固定の混合関数（D1〜D9, D12, D13）。
  コンパイラの変更は `src/stdlib.rs` の登録だけ。
- Phase 2（設計方針。要承認。Phase 1 では実装しない）: seed 付きの鍵付きハッシュ（D11）、借用キーでの検索（`ref 'key`）、F08 の SIMD によるグループ探索。

### 前提とする他チケットのインターフェース

- Phase 1 は完了済みの A07（`deriving (Eq, Hash)`）、C02（Vec API）、C06（不透明 std record の登録）だけを使う。未完了チケットの interface は使わない。
- Phase 2 だけ: E08 の `Random` が OS の乱数から `i64u` を返す関数を提供すること（関数名は E08 の決定に従う）。
  WASM でホストが seed を設定する経路は D-18 の opt-in とし、E08 または E13 が決める。

### 他チケットへの提供インターフェース

- D08: JSON object を `HashMap<string, _>` で表すなら、`HashMap.fold`／`HashMap.iter` の順序（D2）で出力すれば native と WASM で同じ文字列になる。
- C10: arena のハンドルが `Eq` と `Hash` の instance を持てば（C10 の D5）、そのままキーに使える。

### 型

- `HashMap<'key, 'value>`（std モジュール `HashMap` の record `HashMap.HashMap`）と `HashSet<'key>`（`HashSet.HashSet`）。利用者は `Map<'k, 'v>` と同じく無修飾で書ける。
- 常に非 Copy・`needs_drop`（Vec を持つため）。`can_capture`・`can_send`・参照の保持の可否は要素型に従う（C06 の `Map` と同じ。通常の record の規則）。
- 公開 ABI の署名には使えない（`E1008`。`Map` と同じ）。
- 内部表現は定義モジュールの外から見えない（`E1022`）。

### API

利用者から見た型は次のとおり（C06 の表記に合わせ、型変数は std のソースと同じ `'key`・`'value`・`'state`）。
`empty` は `Map.empty` と同じく `HashMap.empty()` と呼ぶ。

```text
HashMap.empty         : HashMap<'key, 'value>
HashMap.with_capacity : i64 -> HashMap<'key, 'value>
HashMap.length        : ref HashMap<'key, 'value> -> i64
HashMap.is_empty      : ref HashMap<'key, 'value> -> bool
HashMap.insert        : (Hash<'key>, Eq<'key>) => HashMap<'key, 'value> -> 'key -> 'value -> HashMap<'key, 'value>
HashMap.remove        : (Hash<'key>, Eq<'key>) => HashMap<'key, 'value> -> 'key -> HashMap<'key, 'value>
HashMap.contains_key  : (Hash<'key>, Eq<'key>) => ref HashMap<'key, 'value> -> 'key -> bool
HashMap.get           : (Hash<'key>, Eq<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> 'key -> Option.Option<'value>
HashMap.at            : (Hash<'key>, Eq<'key>) => ref HashMap<'key, 'value> -> 'key -> ref 'value
HashMap.to_array      : (Copy<'key>, Copy<'value>) => ref HashMap<'key, 'value> -> [('key * 'value)]
HashMap.keys          : Copy<'key> => ref HashMap<'key, 'value> -> ['key]
HashMap.values        : Copy<'value> => ref HashMap<'key, 'value> -> ['value]
HashMap.fold          : ref HashMap<'key, 'value> -> 'state -> ('state -> ref 'key -> ref 'value -> 'state) -> 'state
HashMap.iter          : ref HashMap<'key, 'value> -> Seq.Seq<(ref 'key * ref 'value)>

HashSet.empty         : HashSet<'key>
HashSet.with_capacity : i64 -> HashSet<'key>
HashSet.length        : ref HashSet<'key> -> i64
HashSet.is_empty      : ref HashSet<'key> -> bool
HashSet.insert        : (Hash<'key>, Eq<'key>) => HashSet<'key> -> 'key -> HashSet<'key>
HashSet.remove        : (Hash<'key>, Eq<'key>) => HashSet<'key> -> 'key -> HashSet<'key>
HashSet.contains      : (Hash<'key>, Eq<'key>) => ref HashSet<'key> -> 'key -> bool
HashSet.to_array      : Copy<'key> => ref HashSet<'key> -> ['key]
HashSet.fold          : ref HashSet<'key> -> 'state -> ('state -> ref 'key -> 'state) -> 'state
HashSet.iter          : ref HashSet<'key> -> Seq.Seq<ref 'key>
```

- `insert`: キーがなければ末尾へ追加する。等しいキー（`Eq.eq`）があれば、格納済みのキー（最初の代表）を保ち、古い値を解放して新しい値を置く。
  渡した新しいキーは解放する。entry の位置は変わらない。
- `remove`: キーがなければ map をそのまま返す。あれば entry（キーと値）を解放する。順序への影響は D2。
- `contains_key`／`get`／`at`／`remove` の問い合わせキーは値として受け取り、内部で借用して比較し、呼び出しの終わりに解放する（C06 と同じ）。
- `at` はキーがなければトラップする。`get` は値を複製して返すので `Copy<'value>` を要する。非 Copy の値は `at` で借用する。
- `with_capacity n` は `n` 件を再確保なしで挿入できる容量を確保する。`n == 0` は `empty` と同じで確保しない。
- 集合演算（`union` など）、`singleton`、所有値を取り出す `pop`、縮小は Phase 1 に含めない（D9）。

### 反復順序

- `fold`・`iter`・`keys`・`values`・`to_array` は entry 配列の添字順に走査する。順序は挿入と削除の列だけで決まり、ハッシュ値・表の大きさ・
  target（native／WASM）・最適化段階に依存しない（D2）。
- 新しいキーは末尾に付く。既存キーへの `insert` は位置を変えない。
- `remove` は swap-remove: 削除した位置へ末尾の entry を移し、長さを 1 減らす。したがって削除があると「挿入順」ではなくなる。
  例: キー `a b c d` の順に挿入して `b` を削除すると `a d c`。
- `HashSet` の順序は内部の `HashMap<'key, unit>` と同じ。

### 評価順序・所有権・借用

- 引数は言語の通常の順序（左から右）で評価する。keyed な操作は内部で次の順に利用者の instance を呼ぶ:
  1. `Eq.eq key key`（反射性。false ならトラップ）、2. `Hash.hash key`、3. 探査中、格納済みの混合ハッシュが等しい entry ごとに
  `Eq.eq (格納キー) (問い合わせキー)`。表の再構築は格納済みのハッシュを使い、利用者の instance を呼ばない。
- `insert`／`remove` は map を消費して新しい map を返す。消費後の map の使用は `E1012`。
- `at` の結果は map の共有借用で、借用中の `insert`／`remove`／move は `E1014`、関数の外への返却は `E1013`（C06 と同じ）。
- `fold` の関数と `iter` の要素はキーと値の共有借用を受け取る。キーの排他借用は提供しない（キーを変えると表が壊れるため）。
- 解放の順序（置換時の古い値と新しいキー、削除時のキーと値）は規定しない。B07 の前は観測できない。
- 関数値による捕捉と Task の drop は通常の record と同じで、複製は独立した storage を持つ（C06 の `map_capture`・`map_task_drop` と同じ）。

### 数値・トラップ・native と WASM の差

- 浮動小数点のキー: `-0.0` と `0.0` は `Eq` で等しく、`Hash` も正規化するので同じ entry になり、最初に挿入した方を代表キーとして保つ。
  NaN は `Eq.eq key key` が false なので、keyed な操作はすべてトラップする（空の map への問い合わせも含む）。
- 格納キーの反射性は検査しない。格納されるキーは挿入時に必ず検査を通り、`Eq` は純粋なので結果は変わらない（C06 は `Map.singleton` が
  検査なしで格納できるため比較ごとに検査している。`HashMap` には検査なしの格納経路がない）。
- 混合関数は `i64u` の通常の演算（`*` は折り返し）と `>>>`、`as i64`（bit 保持）だけを使う。表の添字は `hash & mask`（`mask = size - 1`）で、
  負のハッシュでも `[0, size)` に入る。
- トラップ: `at` の欠けたキー、非反射的なキー、`with_capacity` の負の引数と 2^60 を超える引数、確保失敗と Vec の overflow。
- native と WASM で結果（値・順序・トラップの有無）は同じ。異なるのは使えるメモリだけ（wasm32 は 16 MiB の heap）。

### 診断

新しい診断コードは足さない。既存の検査がそのまま働くことをテストで確かめる。

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E1022` | 定義モジュールの外で `HashMap.HashMap`・`HashMap.Entry`・`HashSet.HashSet` を構築・field 参照・pattern・update する | `the representation of 'HashMap.HashMap' is opaque; use its module API`（既存の形式） | その式・pattern |
| `E1005` | キー型に `Hash` か `Eq` の instance がない。`get`・`keys`・`values`・`to_array` で `Copy` がない | `no instance for Hash<Key>; define an instance or use a supported type`（既存の形式） | 呼び出し |
| `E1012` | `insert`／`remove` に渡した map を再び使う | 既存（変更しない） | 再使用の位置 |
| `E1014` | `at` の借用が生きている間に map を更新・move する | 既存（変更しない） | 更新の位置 |
| `E1013` | `at` の結果を関数の外へ返す、排他参照を値に格納する | 既存（変更しない） | 返却・格納の位置 |
| `E1008` | 公開 ABI の署名に `HashMap`／`HashSet` を使う | 既存（変更しない） | 署名 |
| `E1011` | 利用者のモジュール名が `HashMap`／`HashSet` | 既存（`reserved for the standard library` を含む） | そのソース |

### 資源上限

| 項目 | 規則 | 超えたとき |
| --- | --- | --- |
| 添字表の大きさ | 2 の冪で 8 以上。空の map は 0（確保なし） | – |
| 負荷率 | 件数 × 2 ≤ 表の大きさ。新しいキーの挿入で超えるなら先に表を倍にする | – |
| `with_capacity n` | `0 ≤ n ≤ 2^60`（1152921504606846976） | トラップ |
| 件数 | メモリで決まる。native は malloc、wasm32 は 16 MiB の heap | 確保失敗でトラップ |
| 1 entry の大きさ | `Entry { hash: i64, key, value }` の record の大きさと、添字表の 2〜4 slot（1 slot 8 bytes） | – |

計算量（ハッシュが一様に散る場合の期待値。`Hash.hash` と `Eq.eq` の費用、たとえば文字列の長さ n に比例する費用は別）:

| 操作 | 期待 | 最悪（全キーが衝突） |
| --- | --- | --- |
| `insert`（新しいキー） | 償却 O(1)（Vec と表の倍増を含む） | O(n) |
| `insert`（既存キー）、`contains_key`、`get`、`at` | O(1) | O(n) |
| `remove` | O(1)（後方シフトは clusters の長さ） | O(n) |
| `length`、`is_empty`、`empty` | O(1) | O(1) |
| `fold`、`iter` の全走査、`keys`、`values`、`to_array` | O(n) | O(n) |
| `with_capacity n` | O(n)（表を -1 で埋める） | O(n) |

### 例

新 API（実装後に有効。未検証）。結果は `154433`: 挿入順 `10 20 30 40`、`10` の置換で位置は変わらず、`20` の削除で末尾の `40` が 2 番目へ移り、
順序は `10→5, 40→4, 30→3` になる。`((0 × 100 + 15) × 100 + 44) × 100 + 33 = 154433`。

```tsuzuri
let mut map: HashMap<i64, i64> = HashMap.empty()
map = HashMap.insert map 10 1
map = HashMap.insert map 20 2
map = HashMap.insert map 30 3
map = HashMap.insert map 40 4
map = HashMap.insert map 10 5
map = HashMap.remove map 20
HashMap.fold (ref map) 0 (\total key value -> total * 100 + deref key + deref value)
```

新 API（実装後に有効。未検証）。拒否とトラップの例（期待する結果を右に書く）:

```text
record Key { name: string } deriving (Eq)
HashMap.insert (HashMap.empty()) (Key { name: "a" }) 1                          -> E1005（Hash<Key> がない）
let map = HashMap.insert (HashMap.empty()) 1 "x"
HashMap.get (ref map) 1                                                          -> E1005（Copy<string> がない）
let map: HashMap<i64, i64> = HashMap.HashMap { entries: Vec.empty(), slots: Vec.empty() }   -> E1022
let map = HashMap.insert (HashMap.empty()) 1 "x"
let value = HashMap.at (ref map) 1
let next = HashMap.remove map 1
value.length                                                                     -> E1014
HashMap.insert (HashMap.empty()) (0.0 / 0.0) 1                                   -> 実行時トラップ
let map: HashMap<f64, i64> = HashMap.empty()
HashMap.contains_key (ref map) (0.0 / 0.0)                                       -> 実行時トラップ
```

## 設計

### データ構造

std のソース（新規）の record。試作で動作を確かめた形で、std に置いた後の動作は未検証。

```tsuzuri
private record Entry<'key, 'value> { hash: i64, key: 'key, value: 'value }
record HashMap<'key, 'value> { entries: Vec<Entry<'key, 'value>>, slots: Vec<i64> }
```

```tsuzuri
record HashSet<'key> { map: HashMap.HashMap<'key, unit> }
```

不変条件（実装とテストの両方でこの番号を使う）:

- I1: `slots` の長さは 0 か、8 以上の 2 の冪。長さ 0 なら `entries` も空。
- I2: `slots` が空でなければ `2 * Vec.length entries <= Vec.length slots`。したがって空き slot が必ずあり、探査は止まる。
- I3: `[0, Vec.length entries)` の各添字は `slots` にちょうど一回現れ、残りの slot は `-1`。
- I4: `entries[i].hash == hash_of (ref entries[i].key)`（混合後の値）。
- I5: 線形探査の連続性: 添字 i を持つ slot を s とすると、`entries[i].hash & mask` から s までの slot（巡回）はすべて空でない。
- I6: 利用者の `Hash` と `Eq` が整合していれば、`Eq` で等しいキーを持つ entry は二つない。

`src/stdlib.rs` の変更（Rust）:

```rust
// SOURCES（アルファベット順。"std/Gpu.tz" の後、"std/IO.tc" の前）
("std/HashMap.tz", include_str!("../std/HashMap.tz")),
("std/HashSet.tz", include_str!("../std/HashSet.tz")),
// RESERVED_MODULES（"Set" の後）
"HashMap",
"HashSet",
// opaque_record の matches! に追加
"HashMap.HashMap" | "HashMap.Entry" | "HashSet.HashSet"
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std | `std/HashMap.tz`（新規） | `Entry`（private）、`HashMap`、公開関数 14 個 | 「アルゴリズム」の擬似コードどおり。補助関数 `mix`・`hash_of`・`table_size`・`empty_slots`・`probe`・`find`・`locate`・`rebuild`・`unlink`・`iter_from`（すべて新規・`private`） |
| std | `std/HashSet.tz`（新規） | `HashSet`、公開関数 10 個 | `HashMap.HashMap<'key, unit>` への委譲。`insert` は `()` を値にする。`iter` は `Seq.map` で組の第 1 要素を取り出す |
| 登録 | `src/stdlib.rs` | `SOURCES`, `RESERVED_MODULES`, `opaque_record` | 上の Rust 片のとおり |
| 登録のテスト | `src/stdlib.rs` | `reserves_the_d07_table` | `assert_eq!(RESERVED_MODULES.len(), 20)` を 22 へ（既存テストの正当な変更） |
| 検査 | `src/check.rs` | `record_storage`, `Builtin::Hash`, `Builtin::HashMix` | 変更なし（`record_storage` は `opaque_record` を引くだけ） |
| ハッシュ | `src/llvm_hash.rs`, `src/derive.rs` | canonical ストリームと FNV-1a | 変更なし（`Hash.hash` の値を変えない） |
| LLVM・所有権 | `src/llvm.rs`, `src/ownership.rs`, `src/polymorph.rs` | – | 変更なし。std の generic 関数として既存の特殊化・drop・clone を通る |
| ランタイム | `src/runtime/` | – | 変更なし（Vec の `@tz.alloc`／`@tz.realloc`／`@tz.free` だけを使う） |
| テスト | `tests/hash_map.rs`（新規） | 「テスト計画」の 5 関数 | – |
| E2E | `tests/fixtures/hash_map/Main.tz`（新規）, `tests/features.mjs` | suite `hash_map`（新規）、参照の補助 `insertionTable`（新規） | – |

### 生成 IR とランタイム

- 新しい `%tz.*` 型・ランタイム記号・WASM import はない。`HashMap<'key, 'value>` は通常の record で、LLVM 型は二つの Vec 記述子の struct になる。
- 関数は使われた型ごとに `@tz.fn.HashMap.insert...` などへ特殊化される（`@tz.fn.Map.lower_bound...` と同じ命名）。GUIDE D-07 により、
  `HashMap` を使わないプログラムの IR には何も出ない。手順 1 の IR と byte 単位で一致することを確かめる。
- 検索（`probe`・`find`・`hash_of`・`mix`・`contains_key`・`get`・`at`）は確保しない。確保は `insert`（Vec の伸長と `rebuild`）と
  `with_capacity` だけ。`inspect` で検査する（テスト計画）。

### アルゴリズム

`HashMap.tz` の本体。`slots[p]` は `ref` の Vec の要素の読み取り、`slots[p] = x` は `slots = Vec.set slots p x` を表す。

```text
mix(h: i64u) -> i64                          // MurmurHash3 の fmix64。固定（D5）
  h = (h ^ (h >>> 33)) * 0xff51afd7ed558ccd
  h = (h ^ (h >>> 33)) * 0xc4ceb9fe1a85ec53
  return (h ^ (h >>> 33)) as i64

hash_of(key: ref 'key) -> i64
  assert Eq.eq key key                       // NaN などをトラップ（D7）
  return mix(Hash.hash key)

probe(map: ref, key: ref, hash) -> slot      // 前提: slots が空でない（I2 で必ず止まる）
  mask = len(slots) - 1; p = hash & mask
  loop
    i = slots[p]
    if i < 0 || (entries[i].hash == hash && Eq.eq (ref entries[i].key) key) then return p
    p = (p + 1) & mask

find(map: ref, key: ref) -> i64              // entry の添字、なければ -1
  hash = hash_of key                         // 空の map でも先に検査する
  if len(slots) == 0 then return -1
  return slots[probe(map, key, hash)]

locate(slots: ref, hash, index) -> slot      // 値 index を持つ slot。index = -1 なら最初の空き slot
  p = hash & mask; while slots[p] != index do p = (p + 1) & mask; return p

table_size(n) -> i64
  assert (n >= 0 && n <= 1152921504606846976)
  size = 8; while size < 2 * n do size = size * 2; return size

rebuild(entries: ref, size) -> slots         // 格納済みの hash だけを使う
  slots = empty_slots(size)                  // Vec.with_capacity size に -1 を size 回 push
  for i in 0 .. len(entries): slots[locate(slots, entries[i].hash, -1)] = i

insert(map, key, value)
  hash = hash_of(ref key); count = len(entries)
  found = if len(slots) == 0 then -1 else slots[probe(ref map, ref key, hash)]
  match map with HashMap { entries = entries, slots = slots } ->
  if found >= 0 then                         // 位置を変えずに値だけ置換（C06 の insert と同じ手順）
    (rest, Some old) = Vec.pop (Vec.swap entries found (count - 1))
    entries = Vec.swap (Vec.push rest (Entry { hash, key: old.key, value })) found (count - 1)
  else
    if 2 * (count + 1) > len(slots) then slots = rebuild(ref entries, if len(slots) == 0 then 8 else len(slots) * 2)
    slots[locate(slots, hash, -1)] = count
    entries = Vec.push entries (Entry { hash, key, value })

remove(map, key)
  hash = hash_of(ref key); count = len(entries)
  if count == 0 then return map
  slot = probe(ref map, ref key, hash); index = slots[slot]
  if index < 0 then return map
  match map with HashMap { entries = entries, slots = slots } ->
  slots = unlink(ref entries, slots, slot)
  if index != count - 1 then slots[locate(slots, entries[count - 1].hash, count - 1)] = index
  (rest, _removed) = Vec.pop (Vec.swap entries index (count - 1))   // _removed の key と value を解放

unlink(entries: ref, slots, hole) -> slots   // 後方シフト削除。墓標を作らない（D4）
  p = (hole + 1) & mask
  while slots[p] >= 0 do
    ideal = entries[slots[p]].hash & mask
    if ((p - ideal) & mask) >= ((p - hole) & mask) then slots[hole] = slots[p]; hole = p
    p = (p + 1) & mask
  slots[hole] = -1

with_capacity(n)
  size = table_size(n)                       // 負と上限超えをここでトラップ
  if n == 0 then empty() else HashMap { entries: Vec.with_capacity n, slots: empty_slots(size) }
```

- `remove` の `unlink` は entry 配列を動かす前に行う（`entries[slots[p]].hash` を正しい位置から読むため）。末尾の entry の slot の付け替えは
  `unlink` の後に行う（`unlink` で末尾の entry の slot が動いている場合があるため、`locate` で探し直す）。
- `probe` の比較はまず格納済みの混合ハッシュを比べ、等しいときだけ `Eq.eq` を呼ぶ。
- 定数の由来: `0xff51afd7ed558ccd` と `0xc4ceb9fe1a85ec53` は MurmurHash3 の fmix64 の乗数。docs/language.md に固定値として書く。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。std のソースは tab ではなく 4 空白で字下げしてよい（`std/Vec.tz` と同じ）。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行する。`HashMap` を使わない検証済みのサンプルを `/tmp/tz-c09/baseline/Main.tz` に置き、IR を保存する。

```tsuzuri
let mut map: Map<i64, i64> = Map.empty()
map = Map.insert map 2 20
map = Map.insert map 1 10
Map.fold (ref map) 0 (\total key value -> total * 100 + deref key + deref value)
```

- 確認: `run` は `1122`。IR は `@tz.fn.Map.insert` を含み `HashMap` を含まない。4 つのテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
target/release/tsuzuri run /tmp/tz-c09/baseline
target/release/tsuzuri build /tmp/tz-c09/baseline --emit llvm -o /tmp/tz-c09/before.ll
target/release/tsuzuri build /tmp/tz-c09/baseline --emit llvm -O3 -o /tmp/tz-c09/before-O3.ll
cargo test --locked --test polymorphism honors_the_exact_specialization_limit
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
```

### 手順 2: モジュールの登録と骨格

- 変更: `std/HashMap.tz`（新規）、`src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES`・`opaque_record`・`reserves_the_d07_table`）、`tests/hash_map.rs`（新規）。
- 内容: `Entry`・`HashMap` の record、`empty`・`length`・`is_empty`、private の `mix`・`hash_of` を書く。「データ構造」の Rust 片どおりに登録する。
  `RESERVED_MODULES` には `"HashSet"` も今足す（ソースがまだなくても予約できる）。テストファイルに「テスト計画」の
  `hash_containers_are_opaque_noncopy_owned_values`（この段階では `HashMap.empty()` と `HashMap.length` だけを使う入力と E1022・E1008 の行）、
  `hash_module_names_are_reserved`、`unused_hash_containers_emit_no_code`、`hash_containers_leave_the_specialization_budget_to_users` を足す。
- 確認: `cargo test --locked --lib stdlib` が成功する（`reserves_the_d07_table`、`embedded_sources_use_reserved_flat_modules`、
  `std_definitions_never_shadow_builtin_functions` を含む）。`cargo test --locked --test hash_map` が `4 passed`。
  手順 1 の 4 テストが成功する。`cargo build --release --locked` の後、手順 1 と同じ `build` の出力が `/tmp/tz-c09/before.ll`・`before-O3.ll` と
  `cmp` で一致する。

### 手順 3: 検索と挿入

- 変更: `std/HashMap.tz`、`tests/hash_map.rs`。
- 内容: `table_size`・`empty_slots`・`probe`・`find`・`locate`・`rebuild` と、公開の `insert`・`contains_key`・`get`・`at` を「アルゴリズム」どおりに書く。
  所有 record の分解は `match map with | HashMap { entries = entries, slots = slots } -> ...`（試作で確認済み）。3 制約の組
  `(Hash<'key>, Eq<'key>, Copy<'value>)` は `std/Parallel.tz` の `map` と同じ書き方。
  `hash_lookup_and_updates_use_only_required_constraints` を足し、opaque のテストに E1012 の行を足す。
- 確認: `cargo test --locked --test hash_map` が `5 passed`。`cargo test --locked --lib stdlib` が成功する。

### 手順 4: 削除

- 変更: `std/HashMap.tz`、`tests/hash_map.rs`。
- 内容: `unlink` と `remove`。`unlink` を entry 配列の swap より先に行い、末尾の entry の slot を `locate` で探し直して付け替える。
  `hash_lookup_and_updates_use_only_required_constraints` に `remove` と `E1014` の行を足す。
- 確認: `cargo test --locked --test hash_map` が `5 passed`。

### 手順 5: 走査と容量

- 変更: `std/HashMap.tz`、`tests/hash_map.rs`。
- 内容: `fold`・`iter_from`／`iter`（`std/Map.tz` の `Seq.defer` の形を写す）・`keys`・`values`・`to_array`（`new [...](length map, index -> ...)`）・
  `with_capacity`。テストの受理側に各 API の行を足す。
- 確認: `cargo test --locked --test hash_map` が `5 passed`。

### 手順 6: HashSet

- 変更: `std/HashSet.tz`（新規）、`src/stdlib.rs`（`SOURCES`）、`tests/hash_map.rs`。
- 内容: `record HashSet<'key> { map: HashMap.HashMap<'key, unit> }` と 10 関数。`insert` は `HashMap.insert set.map key ()`、`iter` は
  `Seq.map (HashMap.iter (ref set.map)) (\pair -> match pair with | (key, _unit) -> key)`（試作で確認済み）。他の std モジュールは修飾して書く（D-07）。
  opaque と受理・拒否のテストに `HashSet` の行を足す。
- 確認: `cargo test --locked --test hash_map` が `5 passed`。`cargo test --locked --lib stdlib` が成功する。

### 手順 7: E2E fixture と suite

- 変更: `tests/fixtures/hash_map/Main.tz`（新規）、`tests/features.mjs`（`insertionTable`・`hashOps`（新規）と suite `hash_map`）。
- 内容: 「テスト計画」の E2E のとおり。fixture のディレクトリには `Main.tz` だけを置く（E03 の再帰探索）。
- 確認: 次が成功し、全ケースが native／WASM × `-O0`／`-O3` で通り、`live == 0`、WASM の import が空。Node 20 が V8 の
  `RepresentationChangerError` で落ちたら Node 24 で実行する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
node tests/features.mjs target/release/tsuzuri hash_map
npx --yes --package=node@24 node tests/features.mjs target/release/tsuzuri hash_map
```

### 手順 8: 既存の suite と全体

- 変更: なし（失敗したら原因を直す。期待値は変えない）。
- 確認: 次がすべて成功する。手順 2 と同じく baseline の IR が `cmp` で一致する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node tests/features.mjs target/release/tsuzuri map_set
node tests/features.mjs target/release/tsuzuri deriving
node tests/features.mjs target/release/tsuzuri
```

### 手順 9: 文書

- 変更: 「ドキュメント」の全ファイル。
- 内容: `target/release/tsuzuri doc std -o _docs/library-reference/api` で API の snapshot を再生成する（`_docs/contributing.md` の手順）。
  `_docs/library-reference/hash-map.md`（新規）の実行可能サンプルは `tsuzuri run=154433` の形で「例」を載せる。
- 確認: `node scripts/check-docs.mjs _docs/library-reference/hash-map.md _docs/library-reference/map-set.md _docs/library-reference/README.md _docs/README.md _docs/feature-status.md`
  が成功する。`git diff --check` が空。

### 手順 10: 最終確認

- 変更: `_features/README.md` の状態、`_docs/feature-status.md` の C09 行、GUIDE D-07 の表と D-30 の行。
- 確認: 手順 8 のコマンドと手順 1 の 4 テストが成功する。`grep -n "HashMap" _features/GUIDE.md` が D-07 の表の行だけを出す。
  受け入れ条件をすべて満たす。

## テスト計画

### Rust テスト

`tests/hash_map.rs`（新規）。`tests/map_set.rs` の形（`analyze` と `llvm::emit_target(&module, llvm::Entry::Library, wasm)`）を写す。

- `hash_containers_are_opaque_noncopy_owned_values`:
  受理: `let map: HashMap<i64, string> = HashMap.empty()\nlet set = HashSet.insert (HashSet.empty()) "key"\nHashMap.length (&map) + HashSet.length (&set)`。
  native と WASM の IR を 2 回出して一致。
  `E1022`: `HashMap.HashMap { entries: Vec.empty(), slots: Vec.empty() }` の構築、`map.entries.length`、`{ map with slots = Vec.empty() }`、
  `match map with | HashMap.HashMap { entries = storage } -> ...`、`match set with | { map = inner } -> ...`。
  `E1012`: `let map = HashMap.insert (HashMap.empty()) 1 2\nlet other = map\nHashMap.length (&map) + HashMap.length (&other)`。
  `E1008`: `export def bad :: HashMap<i64, i64>\nfn bad = HashMap.empty()`。
- `hash_module_names_are_reserved`: `analyze_modules(&[("HashMap", "")])` と `analyze_modules(&[("HashSet", "")])` が `E1011`
  （`tests/tasks.rs` の `Task` の行と同じ形）。
- `hash_lookup_and_updates_use_only_required_constraints`:
  受理（各行を解析し、native と WASM の IR を出す）: string キーと string 値の `insert`・`at`・`fold`、`get` の Copy 値、
  `record Key { name: string } deriving (Eq, Hash)` のキー、`remove` の後の `contains_key`、`keys`・`values`・`to_array`、`with_capacity 0` と `with_capacity 10`、
  `HashSet` の `insert`・`remove`・`contains`・`fold`・`to_array`・`iter` を `for` で回す行、`ref string` を値に持つ map、Task を値に持つ map の `remove`。
  拒否: `record Key { name: string } deriving (Eq)` をキーに `insert`（`E1005`、Hash がない）、`deriving (Hash)` だけのキー（`E1005`、Eq がない）、
  `HashMap.get` の値が string（`E1005`）、`HashMap.keys` のキーが string（`E1005`）、`HashSet.to_array` のキーが string（`E1005`）、
  `at` の借用中の `remove`（`E1014`）、`fn bad() -> &string { let map = HashMap.insert (HashMap.empty()) 1 "x"; HashMap.at (&map) 1 }`（`E1013`）。
- `unused_hash_containers_emit_no_code`: 手順 1 のサンプルの IR（native と WASM）が `HashMap` と `HashSet` を含まない。`HashMap.insert` を 1 回使う
  プログラムの IR は `@tz.fn.HashMap.insert` を含み、`@tz.fn.HashSet.` を含まない。
- `hash_containers_leave_the_specialization_budget_to_users`: `honors_the_exact_specialization_limit` と同じ生成（`id` と 1,024 個の `R{index}`・`f{index}`）で
  受理、1,025 個目で `E1017`。std に HashMap／HashSet の generic 関数が増えても利用者の予算を消費しないことを、C09 の変更の近くで検出する。

### E2E

fixture `tests/fixtures/hash_map/Main.tz`（新規）、`tests/features.mjs` の suite `hash_map`（新規。GUIDE §7.4）。期待値は JavaScript の参照で計算し、
コンパイラの出力から写さない。suite の既定の実行で native／WASM × `-O0`／`-O3`、各ケースの後に `live == 0`、WASM の import が空であることを確かめる。

参照の補助（`orderedReferences` の近くに置く。新規）。D2 の swap-remove を再現する:

```javascript
function insertionTable() {
  const keys = [], values = [], index = new Map();
  return {
    keys, values, get: (key) => index.get(key),
    set(key, value) {
      const at = index.get(key);
      if (at === undefined) { index.set(key, keys.length); keys.push(key); values.push(value); } else values[at] = value;
    },
    remove(key) {
      const at = index.get(key);
      if (at === undefined) return;
      const last = keys.length - 1;
      keys[at] = keys[last]; values[at] = values[last]; index.set(keys[at], at);
      keys.pop(); values.pop(); index.delete(key);
    },
  };
}

function hashOps(count, seed, set) {
  const table = insertionTable(), wrap = (value) => BigInt.asIntN(64, value);
  let state = seed, acc = 0n;
  for (let step = 0n; step < count; step++) {
    state = wrap(state * 6364136223846793005n + 1442695040888963407n);
    const r = BigInt.asUintN(64, state) >> 33n, key = (r >> 3n) % 4099n * 8n - 16000n, op = r % 8n;
    if (op < 4n) table.set(key, step);
    else if (op < 6n) table.remove(key);
    else { const at = table.get(key); acc = wrap(acc * 31n + (at === undefined ? 7n : set ? 1n : table.values[at])); }
  }
  return table.keys.reduce((total, key, position) => wrap(total * 31n + key * 7n + (set ? 0n : table.values[position])),
    wrap(acc * 1000003n + BigInt(table.keys.length)));
}
```

fixture の `hash_ops` は新 API（実装後に有効。未検証）で、参照と同じ操作列を実行する。`hash_set_ops` は同じ形で `HashSet<i64>` を使い、
検索は含めば `1`、なければ `7` を足し、最後の fold は `total * 31 + deref key * 7`。

```tsuzuri
export def hash_ops :: i64 -> i64 -> i64
fn hash_ops count seed =
    let mut map: HashMap<i64, i64> = HashMap.empty()
    let mut state = seed
    let mut acc = 0
    let mut step = 0
    while step < count do
        state = state * 6364136223846793005 + 1442695040888963407
        let r = state >>> 33
        let key = (r >>> 3) % 4099 * 8 - 16000
        let op = r % 8
        if op < 4 then map = HashMap.insert map key step
        else if op < 6 then map = HashMap.remove map key
        else acc = acc * 31 + (if HashMap.contains_key (ref map) key then deref (HashMap.at (ref map) key) else 7)
        step = step + 1
    HashMap.fold (ref map) (acc * 1000003 + HashMap.length (ref map)) (\total key value -> total * 31 + deref key * 7 + deref value)
```

| export | 型 | 内容 | 期待値 |
| --- | --- | --- | --- |
| `hash_ops` | `i64 -> i64 -> i64` | 上のとおり | `hashOps(count, seed, false)`。count は `0 1 2 7 8 9 100 1000 100000`、seed は `1 2 -3` |
| `hash_set_ops` | `i64 -> i64 -> i64` | `HashSet<i64>` で同じ操作列 | `hashOps(count, seed, true)`。同じ組 |
| `hash_strings` | `i64 -> i64` | `HashMap<string, string>` へ `insert (to_string (index % 9)) (to_string index)` を count 回、`remove "3"`、結果は `length * 100000 + fold (total * 7 + value.length)` | `insertionTable` で同じ操作をして計算。count は `0 1 9 12 20 257` |
| `hash_collisions` | `i64 -> i64` | `record Bucketed { value: i64 } deriving (Eq)` と `instance Hash<Bucketed> { fn hash key = (key.value % 3) as i64u }`（試作で確認した形）。count 個を挿入、`index % 3 == 0` を削除、残りの `at` の和 × 1000 + 長さ | count 40 で `507026`（試作と手計算で一致）。count `0 1 3 100 1000` は参照で計算 |
| `hash_iter_order` | `i64` | map に `5 6 7 8` を挿入して `6` を削除し `for` で `digits * 10 + key`、set に `1 2 3` を挿入して `1` を削除し同様。`map * 100 + set` | `58732`（`587` と `32`） |
| `first_representative` | `bool` | `-0.0` の後に `0.0` を挿入。キーは 1 個で `1.0 / key < 0.0`、値は後の方。`HashSet` も同じ | `1` |
| `owned_record_keys` | `i64` | `record Key { name: string } deriving (Eq, Hash)` をキーに `get` | `42` |
| `hash_capture` | `i64` | `map_capture` と同じ形（関数値で捕捉して 2 回呼ぶ） | `84` |
| `hash_borrowed_values` | `i64` | 値に `ref string`（`"borrowed"`）を格納して `at` | `8` |
| `hash_task_drop` | `i64` | 値に Task を格納して `remove`、長さを返す | `0` |
| `hash_capacity` | `i64 -> i64` | `with_capacity count` に count 個を挿入し長さ | count（`0 1 7 8 9 1000`） |

トラップ（`traps`）: `hash_at_missing`（空の map の `at`）、`hash_nan_insert`（NaN を `insert`）、`hash_nan_query_empty`（空の map へ NaN で `contains_key`）、
`hash_set_nan`（`HashSet.insert` に NaN）、`hash_capacity_negative`（`with_capacity (-1)`）、`hash_capacity_huge`（`with_capacity 1152921504606846977`）。

`inspect(ir)`: `map_set` の `inspect` を写し、`/^define internal [^\n]*@tz\.fn\.HashMap\.(?:mix|hash_of|probe|find|contains_key|get|at)[^\n]*\{([\s\S]*?)^\}/gm`
の一致が 1 個以上あり、どの本体も `@tz.(alloc|realloc)(` を含まないことを確かめる。

### 既存テストへの影響

- `src/stdlib.rs` の `reserves_the_d07_table`: `RESERVED_MODULES.len()` が 20 から 22 になる（予約の追加による正当な変更）。
- それ以外の期待値は変わらない。`HashMap` を使わないプログラムの IR は変わらない（手順 2・8 で確認）。

### 性能

- 速度の主張はしない。計測する場合は PX01・PX02 の方法で、`Map` と `HashMap` の同じ操作列（`hash_ops` の生成器、100,000 操作）、C++ `std::unordered_map`、
  Rust `HashMap`、C# `Dictionary` を比べ、入力分布と「seed なし」を記録する。CI に速度の合否条件を足さない。

## ドキュメント

- `docs/language.md`: `### Map / Set` の直後に `### HashMap / HashSet`（新規）。API の表、反復順序（D2 の swap-remove と例）、キーの制約と NaN、
  混合関数の定数（D5）、負荷率と容量（D6）、HashDoS への耐性がないこと（D10）。`### Map / Set` の「HashMap・木構造・可変iterator・公開ABIは対象外です」
  から HashMap を外す。
- `docs/architecture.md`: 「**順序付きコンテナ:**」の段落の後に「**ハッシュコンテナ:**」の段落を足す（opaque 標準 record、密な entry 配列、
  線形探査と後方シフト削除、固定の混合関数、コンパイラ変更は登録だけ）。
- `_docs/library-reference/hash-map.md`（新規）: 作成・更新・借用・走査の表、順序の例（`tsuzuri run=154433`）、Map との使い分け、HashDoS の注意。
- `_docs/library-reference/README.md`: 一覧の表に `[HashMap / HashSet](hash-map.md)`、API の表に `HashMap`・`HashSet` の行（`api/HashMap.md`・`api/HashSet.md`）。
- `_docs/library-reference/map-set.md`: 「HashMap や木構造としての実装ではありません」「HashMap、直接の公開 ABI は未対応です」を `hash-map.md` への案内に直す。
- `_docs/library-reference/api/`: `tsuzuri doc std` で再生成（手で編集しない）。
- `_docs/README.md`: ライブラリの一覧に `hash-map.md` を足す。
- `_docs/feature-status.md`: C09 の行を実装済みにし、未対応範囲の一覧から HashMap を外す。
- `_features/README.md`: C09 の状態。`_features/GUIDE.md`: D-07 の std の表に `HashMap`／`HashSet`（C09）を足し、D-30 の表から該当行を消す。

## 受け入れ条件

- [ ] `HashMap`／`HashSet` と「API」の全関数が std にあり、内部表現は外から `E1022` で守られる。
- [ ] 反復順序が D2 のとおりで、`hash_ops`・`hash_set_ops`・`hash_strings` が JavaScript の参照と native／WASM × `-O0`／`-O3` で一致する。
- [ ] 全ケースで `live == 0`、WASM の import が空、6 つのトラップがトラップする。
- [ ] 検索の関数本体が確保しない（`inspect`）。
- [ ] `HashMap` を使わないプログラムの IR が手順 1 と byte 単位で一致する。
- [ ] `Hash.hash` の値、`Map`／`Set` の意味、既存テストの期待値（`reserves_the_d07_table` を除く）が変わらない。
- [ ] `honors_the_exact_specialization_limit` と stack-depth の 3 テストが成功する。
- [ ] `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`、`node tests/features.mjs target/release/tsuzuri` が成功する。
- [ ] 「ドキュメント」の更新が終わり、`node scripts/check-docs.mjs` が成功する。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `Hash.hash` をそのまま `& mask` に使うと、8 の倍数などのキーが同じ slot 列へ集まり、探査が O(n) になる（「再現」）。結果は正しいままなので
  テストでは気づきにくい。必ず `mix` を通し、`hash_collisions` 以外の E2E が遅すぎないことを見る。
- 削除を `Option` の墓標にすると `at` が `E1013` で書けない。密な entry 配列と swap-remove を守る。
- `remove` で entry 配列を先に swap すると、`unlink` が読む `entries[slots[p]].hash` が別の entry のものになる。順序は「`unlink` → 末尾の slot の付け替え →
  swap と pop」。削除するのが末尾の entry（`index == count - 1`）なら付け替えをしない。
- 巡回の距離は `(p - ideal) & mask` で計算する。`%` は負の被除数で負を返しうる。
- 表の倍増の判定は新しいキーの挿入だけで行う。既存キーの置換で倍増すると容量が無駄に増える。`rebuild` は格納済みの `hash` を使い、
  `Hash.hash` を呼び直さない（利用者の instance の呼び出し回数が増え、D7 の評価順序も変わる）。
- `find`／`remove` は空の map でも先に `hash_of` を呼ぶ。空の map を先に判定すると NaN の問い合わせがトラップしなくなる（`hash_nan_query_empty`）。
- `table_size` の `2 * n` は、`n` の範囲を `assert` してから計算する。検査なしだと巨大な `n` で i64 が折り返し、ループが止まらない。
- std のソースでは他の std モジュールを修飾して書く（`Option.Some`、`Seq.defer`、`HashSet.tz` の中の `HashMap.HashMap`）。修飾し忘れると利用者の
  同名の型を拾わない代わりに解決に失敗する（GUIDE D-07）。
- std の generic 関数は使われたときだけ特殊化される。非 generic の std の関数や既存 std から `HashMap` を呼ぶと、全プログラムで特殊化が起き、
  利用者の 1,024 の予算を消費する。`honors_the_exact_specialization_limit` が失敗したら停止条件。
- `cargo test --locked <filter>` は一致 0 件でも成功する。`running N tests` を見る。
- E2E の期待値をコンパイラの出力から写さない。参照と合わないときは、D2 の順序（swap-remove）を参照と実装の両方で読み直す。
- Node 20.19.6 は重い BigInt の suite で V8 の `RepresentationChangerError` を出すことがある。Node 24 で実行する。

## 対象外

- 並行 HashMap、可変 iterator（値の排他借用）、公開 ABI（`E1008` のまま）。
- 集合演算（`HashSet.union`・`intersect`・`difference`）、`singleton`、`pop`、`retain`、縮小（`shrink_to_fit`）。`fold` と `insert` で書ける。
- 借用キーでの検索（`ref 'key`）、SIMD のグループ探索（F08）、HashDoS への耐性（D10・D11）。
- `Hash.hash` の出力（FNV-1a と D-26 の canonical ストリーム）の変更。
- `for (key, value) in map` の専用構文（`HashMap.iter` と C07 の反復で足りる）。

## 決定事項

### D1: 実装方式

- 決定: std の不透明 record（C06 と同じ）を Tsuzuri のソース `std/HashMap.tz`・`std/HashSet.tz` で書く。コンパイラの変更は `src/stdlib.rs` の登録だけで、
  新しい `Type` の variant・`Builtin`・ランタイム関数は足さない。
- 理由: C06 で確立した方式で、所有権・drop・clone・特殊化を既存の経路で共有できる。試作で必要な構文がすべて動いた。
- 状態: 既定案（実装者はこの案に従う）

### D2: 反復順序

- 決定: 反復は entry 配列の添字順。新しいキーは末尾、既存キーの置換は位置を変えない、削除は swap-remove（末尾の entry が削除位置へ移る）。
- 理由: 順序がハッシュ関数・表の大きさ・target に依存せず決定的になり、D11 で seed を入れても順序は変わらない。Rust の `indexmap` の `swap_remove` と同じ意味。
- 状態: 既定案（実装者はこの案に従う）

### D3: 削除の方式

- 決定: swap-remove。起票時の案（墓標と償却再配置で残りの順序を保つ）と、順序を保つ shift 削除は採らない。
- 理由: 墓標は `Vec<Option.Option<Entry<...>>>` を要し、`at` の `ref 'value` を返す `match ref ...` が `E1013` になった（「現状」）。shift 削除は O(n) で、
  C06 の劣位をそのまま残す。swap-remove は O(1) で、順序の規則も一文で書ける。
- 見直し提案: 挿入順を削除後も保つ必要が出たら、順序を保つ `HashMap.shift_remove`（O(n)）を別 API として足す。
- 状態: 既定案（実装者はこの案に従う）。起票時の案からの変更として報告済み。

### D4: 添字表

- 決定: `Vec<i64>` の開番地法。`-1` が空、それ以外は entry の添字。線形探査、大きさは 2 の冪、削除は後方シフト（墓標なし）。entry に混合後のハッシュを保存する。
- 理由: 墓標がないので削除が続いても検索が劣化せず、再配置の規則が要らない。保存したハッシュで再構築と後方シフトが利用者の instance を呼ばずにでき、
  比較の前にハッシュで候補を絞れる。
- 状態: 既定案（実装者はこの案に従う）

### D5: ハッシュ関数

- 決定: `mix(Hash.hash key)`。`mix` は MurmurHash3 の fmix64（乗数 `0xff51afd7ed558ccd`、`0xc4ceb9fe1a85ec53`、shift 33）で固定。seed は使わない。
- 理由: `Hash.hash`（FNV-1a）は下位 bit が入力の下位 bit だけで決まり、2 の冪の表では衝突が集中する（「再現」）。fmix64 は全 bit を下位へ拡散する。
  seed なしなら WASM の import も OS 乱数も要らず（D-18）、同じ入力で同じ性能になる。`Hash.hash` の既存の値は変えない。
- 状態: 既定案（実装者はこの案に従う）

### D6: 容量と伸長

- 決定: 最大負荷率 1/2（`2 * 件数 ≤ 表の大きさ`）。空の map は表を持たず、最初の挿入で 8、以後は倍増。`with_capacity n` は `n` 件を再確保なしで
  入れられる大きさ（8 以上の 2 の冪で `2n` 以上）と `Vec.with_capacity n`。`n` が負か 2^60 を超えるとトラップ。縮小はしない。
- 理由: 線形探査は負荷 1/2 で失敗探索の期待探査数が約 2.5 に収まる。2^60 の上限は `2 * n` と倍増の計算が i64 で折り返さない範囲で、実際の確保は Vec の検査が止める。
- 状態: 既定案（実装者はこの案に従う）

### D7: キーの制約と反射性

- 決定: keyed な操作は `(Hash<'key>, Eq<'key>)` を要する。すべての keyed な操作は最初に `Eq.eq key key` を `assert` する（空の map でも）。
  格納キーの検査はしない。等しいキーを挿入したときは最初の代表キーを保つ。
- 理由: C06（GUIDE D-28）と同じ NaN の規則。HashMap には検査なしで格納する経路（`Map.singleton` に当たるもの）がないので、格納キーは必ず検査済み。
- 状態: 既定案（実装者はこの案に従う）

### D8: Eq と Hash が整合しない instance

- 決定: 利用者の instance の契約とする（等しいキーは等しい `Hash.hash` を返す）。破った場合の検索結果・重複は未規定だが、メモリ安全で、探査は I2 により必ず止まり、
  トラップも追加しない。文書に書く。
- 理由: C06 の `Ord` の一貫性と同じ扱い（GUIDE D-28）。検出には全件比較が要る。
- 状態: 既定案（実装者はこの案に従う）

### D9: API の範囲

- 決定: 「API」の 24 関数。`get`・`keys`・`values`・`to_array` だけが `Copy` を要する。集合演算・`singleton`・`pop`・縮小は含めない。
- 理由: 起票時の一覧に従い、C06 と同じ所有と借用の形にそろえる。含めない API は `fold` と `insert` で書け、後から足しても互換性を壊さない。
- 状態: 既定案（実装者はこの案に従う）

### D10: HashDoS

- 決定: Phase 1 は衝突を狙った入力への耐性を持たない。混合関数も `Hash.hash` も公開の固定関数なので、攻撃者は全キーを衝突させられ、操作は最悪 O(n) になる。
  文書（docs/language.md と `hash-map.md`）に明記し、信頼できない入力のキーには `Map` を勧める。
- 理由: 耐性には推測できない seed が要り、native の OS 乱数と WASM の import（D-18）が必要になる。起票時の目的からの変更として報告済み。
- 状態: 既定案（実装者はこの案に従う）

### D11: Phase 2 の seed 付きハッシュ

- 決定: Phase 2 で SipHash-1-3 などの鍵付きハッシュを、`Hash` の canonical ストリームへ鍵を与えて計算する経路として足す。seed は E08 の `Random`
  （native は OS 乱数。取得失敗はトラップし、固定 seed へ黙って置き換えない）、WASM ではホストの明示的な opt-in で設定する。順序は D2 のままで seed に依存しない。
- 理由: 新しいホスト機能と `Builtin`（鍵付きハッシュ）を要し、D-18・D-30 に触れる。
- 状態: 要承認（承認前は Phase 2 に着手しない）

### D12: HashSet の表現

- 決定: `record HashSet<'key> { map: HashMap.HashMap<'key, unit> }`。すべての関数を `HashMap` へ委譲する。
- 理由: 探査・削除・順序の実装を一つにできる。`unit` の値は大きさを持たないか小さく、試作で動いた。
- 状態: 既定案（実装者はこの案に従う）

### D13: モジュール名の予約

- 決定: `HashMap`・`HashSet` を `RESERVED_MODULES` に足し、GUIDE D-30 の仮割り当てを D-07 の表へ移す。
- 理由: std のモジュール名は予約する規則（GUIDE D-07）。HEAD のテスト・例・文書に同名の利用者モジュールはない。
- 状態: 既定案（実装者はこの案に従う）
