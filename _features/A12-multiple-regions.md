# A12: レコード内の独立した複数 region と region 付き関数値型

| 項目 | 内容 |
|---|---|
| ID | A12 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | A09 |
| 後続 | A13, C08 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | Rust 比: 借用で表せるデータ構造の制限（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#rust-に対する劣位点)） |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/regions.rs`, `src/check.rs`, `src/ownership.rs`, `src/ownership_control.rs`, `src/closures.rs`, `docs/language.md`, `tests/borrowed_records.rs`, `tests/types_ownership.rs` |

## 目的

Rust の `struct Pair<'a, 'b> { left: &'a T, right: &'b U }` や `for<'a> fn(&'a T) -> &'a T` に相当する契約を書けるようにする。
寿命の異なる借用を一つの view にまとめる設計、region 契約を持つ関数を高階関数へ渡す設計を、view の分割や名前付き関数への置換なしに表現する。

## 現状

- A09 の完了範囲は、共有借用フィールドと、値全体に一つの region を割り当てる名前付き契約（`def first {r s}`、`ref {r} T`、`record View<'a> {r}`、`View<T> {r}`）まで。
- レコードに独立した二つ目の region を宣言すると `src/regions.rs` が `E1013 "records currently have one shared region; use separate views for independent regions"` を返す。
- 高階関数型の内部の region は `E1013 "named regions inside higher-order function types are not supported; use a named function with all parameters bound"`。
  alias・union・const・ローカル注釈への指定は `E1013 "named regions belong in function signatures or record declarations; ..."`。
- `CheckedFunction.region_sources`（`src/check.rs`）は結果 region の入力添字集合。直接の完全適用だけが無関係な入力の loan を戻り値から除き、関数値・部分適用は全入力の交差寿命を保守的に保持する（`src/ownership.rs`）。
- region は検査専用で、`Type` と LLVM 表現には残らない（A09 の設計）。

## 仕様

### Phase 1: レコードの複数 region（実装対象）

```text
record Pair<'a, 'b> {r s} { left: ref {r} 'a, right: ref {s} 'b }

def left_of {r s} :: Pair<'a, 'b> {r s} -> ref {r} 'a = \pair -> pair.left
```

- 宣言した各 region は一つ以上のフィールドで使う。未使用・未宣言の region は `E1013`。
- 使用位置の `Pair<i64, string> {r s}` は宣言順に対応させる。個数の不一致は `E1013`。
  region を省略した使用は、全 region を同一の交差寿命として扱う（A09 の単一 region と互換）。
- フィールドの読み出し `pair.left` は `r` に属する loan だけを持つ。レコード全体の move・Copy・捕捉は全 region の loan を保持する。
- 関数の結果 region は、引数レコードの一部の region に結び付けられる。本体の実 loan がその region 以外を指せば `E1013`。
- 意味・評価順序・LLVM 表現・公開 ABI は変えない。region は引き続き検査専用とする。

### Phase 2: region 付き関数値型（設計方針）

- 関数型の中で閉じた region 量化を表す（例: `{r} ref {r} string -> ref {r} string`）。rank-1 に限定し、関数値の呼び出し結果へ契約を反映する。
- 量化のない関数値型は従来どおり全入力の交差寿命を保持する。

## 設計

- `RecordDecl` と `CheckedRecord` にフィールドごとの region 添字を保存する。単一 region のレコードは region 1 個として同じ経路を通す。
- `ownership.rs` の値の loan を region 別の集合へ分ける。フィールド射影は該当 region だけを伝播し、合流・ループ固定点・捕捉・Task 拒否は全 region の和で扱う。
- region は単相化キーに含めない。型引数の具体化とは独立に検査する。
- 資源上限: レコードあたり region 16、関数あたり region 64。超過は `E1017`。

## 実装手順

1. **構文と宣言検査**: 複数 region の宣言・使用・個数照合を `parser.rs`／`regions.rs` に追加する。
   確認: `tests/borrowed_records.rs` に受理・拒否ケース（`E1013` の各メッセージ）を追加。
2. **単一 region の同値化**: 既存レコードを region 1 個の一般形へ移し、挙動を変えない。
   確認: `cargo test --locked --test borrowed_records --test types_ownership` が無変更で通る。
3. **region 別 loan**: フィールド射影・部分 move・match 分解で region 別に loan を伝播する。
   確認: 一方の region の所有者だけを move・置換できるケース、できないケース（`E1013`／`E1014`）。
4. **関数契約**: 結果 region を引数レコードの一部 region に結び付け、`region_sources` を拡張する。
   確認: 直接の完全適用と関数値経由の寿命の差を検査するテスト。
5. **Phase 2 の設計レビュー**: 関数型 region の構文と推論を別チケット段階として承認を得る。

## テスト計画

- Rust: 受理（入れ子・ジェネリックレコード・タプル内のレコード）、拒否（`E1012`、`E1013`、`E1014`、Task への送信）、上限 `E1017`。
- E2E: `tests/fixtures/borrowed_records/` に複数 region のケースを追加し、native/WASM × `-O0`/`-O3`、`live == 0`、IR の決定性を確認する。
- 生成 IR が単一 region 版と同一になること（region が IR に影響しないこと）を比較する。

## ドキュメント

- `docs/language.md`「名前付き Region」の未対応一覧と例、`_docs/language-reference/lifetimes.md`、`README.md` の未対応記述。

## 受け入れ条件

- [ ] 独立した複数 region を持つレコードの宣言・使用・フィールド読み出しが検査される。
- [ ] 省略時の意味が A09 と一致し、既存テストが変わらない。
- [ ] region の追加で生成 IR・公開 ABI が変わらない。
- [ ] Phase 2 の設計が承認され、未決事項に記録されている。

## 落とし穴

- 部分 move とフィールド別 loan を組み合わせると、合流で region ごとの和を取り忘れやすい。
- 捕捉・Task・コレクション格納では region を分けず全体を保持する。ここを緩めると use-after-free を許す。
- region を型の同一性に含めると単相化の数が増え、上限 1,024 を無駄に消費する。

## 対象外

- 排他借用フィールド（A13）、一時値の寿命延長、static region、Polonius 相当の一般化した推論。

## 未決事項

- **関数型 region の構文**: 既定案は前置量化 `{r} ref {r} T -> ref {r} T`。型変数 `'a` と混同しない表記を優先する。
- **省略時の意味**: 既定案は全 region を同一の交差寿命とみなす（互換優先）。
