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
| 起票 | 2026-09-29（未レビュー） |
| 主な影響ファイル | `src/llvm.rs`（`storage_layout`・`union_layout`・値の構築と分解）, `src/llvm_frame.rs`, `src/llvm_recursive.rs`, `src/llvm_debug.rs`, `docs/architecture.md`, `tests/union_types.rs`, `tests/primitives.mjs` |

## 目的

union・Option・Result・レコード・タプルの値を、Rust と同等以上に小さく表現する。
値が小さいほど、配列やコレクションに多数を格納するときのメモリ量とキャッシュミスが減り、コピーも速くなります。

## 現状と計測

`src/llvm.rs` の `storage_layout` と `union_layout`（2026-09-29）:

- ペイロードのない union（列挙型）はタグ `i32`（4 bytes）。
- 全ケースのペイロードが同じ LLVM 型の union は `{ i32 タグ, ペイロード }`。`Option<i64>` は 16 bytes、`Option<string>` は 24 bytes、`Option<ref T>` は 16 bytes です。
- ペイロードの型が異なる union は、16 bytes のヘッダーと 16 bytes 単位のペイロード領域（`16 + 16 * n` bytes、アラインメント 16）です。例えば `Result<i64, string>` は 32 bytes。
  Rust では `Option<&T>` は 8 bytes（null を None に使う niche）、`Option<String>` と `Result<i64, String>` は 24 bytes です。
- レコード・タプルは宣言順の C と同じ配置で、フィールドの並べ替えはしません。`{ a: bool, b: i64, c: bool, d: i64 }` は 32 bytes（並べ替えれば 24 bytes）。
- 再帰 union は所有ノードへのポインター（8 bytes）で、最初のペイロードなしのケースを null で表します（D-24）。これは niche の一種で、既に実装済みです。
- 内部のレコード・union の LLVM 表現は安定 ABI ではありません（docs/language.md 公開 ABI）。公開 ABI のレコードは E05 の正規化で別に扱います。

## 目標と指標

- 目標: 代表的な型（`Option<i64>`、`Option<ref T>`、`Option<string>`、`Result<i64, string>`、JSON のような多ケースの union、フィールドの多いレコード）の値のサイズが、Rust の同等の型以下。
- 指標: 型ごとのサイズ、それらを大量に格納する種目（PX02）の最大 RSS と時間。

## 施策

### Phase 1: タグと領域

- タグの幅をケース数に合わせる（256 以下なら `i8`）。
- ペイロードの型が異なる union のヘッダーを 16 bytes から、タグの幅と最大ペイロードのアラインメントで決まる最小の大きさにし、ペイロード領域も 16 bytes 単位ではなく最大ペイロードのサイズにする。

### Phase 2: niche

- ペイロードのないケースが一つで、もう一つのケースのペイロードに使われないビット列がある場合、タグを持たずにそのビット列で表す。
  候補: 参照（null）、再帰ノード（既存）、関数値のコードポインター（null）、`bool`（2 以上）、`utf8char`（0x10FFFF より大きい値）、長さ（負の値）、列挙型の未使用のタグ。
- `Option<ref T>` は 8 bytes、`Option<string>` は 16 bytes になる。

### Phase 3: フィールドの並べ替え

- レコード・タプル・union のペイロードで、アラインメントの大きい順にフィールドを並べ替えて余白を減らす。並べ替えは決定的な規則（アラインメントの降順、同じなら宣言順）にする。
- 公開 ABI の正規化された構造体（E05）と DWARF の表示（G08）は、宣言順の名前を保って対応させる。

## 意味・安全性の保持

- 評価順序はソースの記述順のままにする（並べ替えるのは記憶上の配置だけ）。フィールドの初期化・解放の順序も変えない。
- パターン照合・構造比較（A11）・Hash（A07 の canonical な順序）・Display は宣言順の意味を保つ。
- 借用（`ref record.field`）のアドレスとアラインメントが正しいこと。niche を使う型では、値の中を指す参照が不正なビット列を観測しないこと。

## 検証

- 型ごとのサイズを確認する Rust テスト（決定的な IR の型定義）。
- 全 E2E（union、再帰型、Option／Result、Map／Set、Vec、Task の捕捉）を native/WASM × `-O0`/`-O3` で実行し、確保追跡 `live == 0`。
- DWARF のテスト（`tests/debug_info.mjs`）で、フィールド名と値の表示が変わらないこと。
- 大量の Option／Result を格納する種目で最大 RSS と時間を記録する。

## 受け入れ条件

- [ ] Phase 1〜3 の各段で、対象の型のサイズが縮み、全テストが通る。
- [ ] 公開 ABI と DWARF の表示が変わらない。
- [ ] 最大 RSS と時間の変化を実測し、記録している。

## リスク

- niche を使う表現では、タグの読み出しが比較に置き換わる。分岐が増えて遅くなる型がないか計測する。
- `General` のペイロード領域は現在メモリ経由（spill）で扱われている。領域の型の変更で読み書きの生成を誤りやすい。

## 対象外

- 値のビット単位の詰め込み（`bool` の配列を 1 bit ずつにするなど、要素のアドレスを変える変更）。

## 未決事項

- **並べ替えの既定**: 既定案は既定で並べ替える（内部表現は ABI ではないため）。デバッグ時に宣言順が必要なら DWARF のオフセットで対応する。
