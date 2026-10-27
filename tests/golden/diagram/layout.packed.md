# Layout of Acme::Kb::Shapes.Packed (native, 64-bit)

| item | value |
| --- | --- |
| kind | record |
| size (bytes) | 16 |
| align (bytes) | 8 |
| reordered | yes |
| drop flag | no |

| slot | field | type | size | align | offset |
| --- | --- | --- | --- | --- | --- |
| 0 | n | i64 | 8 | 8 | 0 |
| 1 | a | i8 | 1 | 1 | 8 |
| 2 | b | i8 | 1 | 1 | 9 |
| - | (tail padding) | - | 6 | - | 10 |

Native only. On wasm32 a pointer is 4 bytes and the `null_pointer` niche does not exist, so these numbers are not valid there for any type that holds a pointer.
