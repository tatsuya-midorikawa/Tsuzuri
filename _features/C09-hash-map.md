# C09: HashMap／HashSet

| 項目 | 内容 |
|---|---|
| ID | C09 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | A07, C02, C06 |
| 後続 | D08 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: 連想コンテナが挿入・削除 O(n) の順序付き Map だけ（C# `Dictionary`、Rust `HashMap`、C++ `unordered_map` 相当がない） |
| 主な影響ファイル | `std/HashMap.tz`（新規）, `std/HashSet.tz`（新規）, `src/stdlib.rs`, `src/check.rs`, `src/llvm_hash.rs`, `src/runtime/`, `docs/language.md`, `tests/hash_map.rs`（新規） |

## 目的

平均 O(1) の挿入・検索・削除を持つ連想コンテナを標準で提供する。
反復順序を決定的にしたまま、ハッシュ衝突を狙う入力（HashDoS）への耐性を持たせる。

## 現状

- `Map`／`Set`（`std/Map.tz`、`std/Set.tz`）は Vec を包む不透明 record で、`Ord` の二分探索を使う。検索 O(log n)、挿入・削除 O(n)。
  「HashMap・木構造・可変iterator・公開ABIは対象外です」（docs/language.md Map / Set）。
- 組み込み `Hash` と deriving（A07、`src/llvm_hash.rs`）は固定・鍵なしの little-endian FNV-1a 64-bit。±0・NaN・decimal cohort を正規化する（D-26）。
- `_docs/feature-status.md` の未対応範囲に HashMap がある。

## 仕様

### Phase 1（実装対象）

- std に常に非 Copy の不透明型 `HashMap<'key, 'value>`／`HashSet<'key>` を追加する。キー制約は `(Hash<'key>, Eq<'key>)`。
- API は Map/Set に揃える: `empty`、`with_capacity`、`length`、`is_empty`、`insert`、`remove`、`contains_key`／`contains`、`get`、`at`、`iter`、`fold`、`keys`、`values`、`to_array`。
  所有値を消費して更新値を返す形と、共有借用の読み取りは C06 と同じ。
- **反復順序は挿入順**とする（密な entry 配列 + 添字表）。削除は tombstone と償却再配置で残りの順序を保つ。これにより順序はハッシュ関数・seed に依存せず決定的になる。
- ハッシュ: コンテナ内部では、canonical な Hash のバイト列を鍵付きハッシュ（SipHash-1-3 など）で処理する。組み込み型と導出 instance はこの経路を使う。
  利用者の手書き `Hash` instance は出力値を鍵付きハッシュに通す（衝突耐性は弱くなるため文書化する）。
- seed: native は初回使用時に OS の乱数で決める。WASM は import を増やさず固定 seed とし、ホストが export 経由で seed を設定できる。
  seed は性能だけに影響し、観測できる結果（順序・値）には影響しない。
- キーの Eq の反射性検査（NaN のトラップ）は C06 と同じ規則にする。
- 容量・長さ・バイト数の overflow はトラップ。

### Phase 2（設計方針）

- SIMD によるグループ探索（F08）、借用キーでの検索（`ref string` キー）の最適化。

## 設計

- 実装は Tsuzuri の std と最小の builtin（鍵付きハッシュ、OS 乱数）で行う。コンパイラ登録の opaque record とアクセス制限は C06 の方式を再利用する。
- 鍵付きハッシュの builtin は `Hash` の canonical ストリームを共有し、既存の FNV-1a 出力（`Hash.hash`）の意味は変えない。
- OS 乱数は native runtime C（`getentropy`／`arc4random_buf`／`BCryptGenRandom`）で取得し、失敗時は固定 seed ではなく明示トラップにするか未決事項で決める。

## 実装手順

1. **表の実装**: 挿入順 entry と添字表、固定 seed。確認: `tests/hash_map.rs` で Map をモデルにした乱択操作列の結果一致。
2. **鍵付きハッシュ**: 組み込み型と導出 instance。確認: 既存 `Hash` の golden 値が不変。
3. **seed**: native の OS 乱数、WASM の固定 seed と設定 export。確認: 反復順序が seed に依存しない。
4. **所有権と解放**: 非 Copy キー・値、drop、`iter` の借用。確認: E2E の `live == 0`。

## テスト計画

- Rust: 型制約、借用規則、`E1022`（内部表現へのアクセス）。
- E2E: native/WASM × `-O0`/`-O3`、10 万件の乱択操作を独立した JavaScript 参照実装と照合、確保追跡。
- 性能: Map、C++ `unordered_map`、Rust `HashMap`、C# `Dictionary` と同条件で比較し、seed と入力分布を記録する。

## ドキュメント

- `docs/language.md` の Map / Set 節、`_docs/library-reference/map-set.md`、GUIDE D-07 の std 表。

## 受け入れ条件

- [ ] 平均 O(1) の操作と挿入順の決定的な反復を持つ HashMap／HashSet がある。
- [ ] seed が結果に影響せず、native では推測困難な seed を使う。
- [ ] 既存の Hash・Map・Set の意味が変わらない。

## 落とし穴

- 反復順序を bucket 順にすると seed によって結果が変わり、決定性を失う。
- tombstone が多い表の再配置を忘れると検索が劣化する。
- WASM に乱数 import を黙って追加しない（D-18）。

## 対象外

- 並行 HashMap、可変 iterator、公開 ABI。

## 未決事項

- **OS 乱数の取得失敗**: 既定案はトラップ。固定 seed への黙った置換はしない。
- **鍵付きハッシュ関数**: 既定案は SipHash-1-3。性能測定の結果で再検討する。
