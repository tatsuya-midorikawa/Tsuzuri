# A01: Alpha feature

| 項目 | 内容 |
| --- | --- |
| ID | A01 |
| 優先度 | P1 |
| 規模 | 全体 M（Phase 1: S、Phase 2: S） |
| 依存 | B02（done）、(C03)。C03 は弱い依存: 先に入れると速い。[X99](X99.md) は別件 |
| 後続 | D04 |
| 状態 | todo |
| 主な影響ファイル | `src/check.rs`・`src/new.rs`（新規）, `tests/alpha.rs`, `src/*.rs`, `tests/fixtures/storage/` |

## 実装者向け早見
早見の本文。 D-02 😀
## 着手条件と停止条件
### 停止条件
- `E1012` が変わったら止める。
## 仕様
共通。D-01〜D-03。
### Phase 1: 基本
Phase 1 の仕様。
### Phase 2: 拡張
Phase 2 の仕様。
### 共通の注意
どの Phase にも要る。
## 設計
設計の本文。
## 実装手順
前文。
### Phase 1: 土台
**手順 1: 準備。**
### 手順の補足
Phase 1 に属す。
### Phase 2: 仕上げ
**手順 2: 仕上げ。**
## テスト計画
`tests/alpha.rs`・suite `storage`。
## 受け入れ条件
- [ ] 全部。
