# Diagnostics

| Code | Meaning |
| --- | --- |
| `E1001`–`E1003` | 名前と型 <!-- rule: FIX-001 --> |
| <!-- rule: FIX-002 -->`E1012` | ムーブ後の使用 |

<!-- generated:diagnostic-kinds -->
| コード | kind |
| --- | --- |
| `E1012` | `assign_to_immutable_place`, `use_after_move` |
| `E1014` | `borrow_conflict` |
<!-- /generated:diagnostic-kinds -->

| Code | Condition |
| --- | --- |
| `E1012` | 後の節の表 |
