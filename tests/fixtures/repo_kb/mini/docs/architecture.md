# Mini architecture

| ファイル | 責務 |
|---|---|
| `src/check.rs` | 型検査 |
| `src/control.rs` / `recursion.rs` | 制御構文と再帰検査 |
| `src/runtime/string.ll` / `heap-*.ll` / `utf8string.ll` | ランタイム |
| `std/` / `src/missing.rs` | 標準ライブラリ |
