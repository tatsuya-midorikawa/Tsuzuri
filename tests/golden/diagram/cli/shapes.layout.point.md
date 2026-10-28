# Layout of Shapes::Shapes.Point (native, 64-bit)

| item | value |
| --- | --- |
| kind | record |
| size (bytes) | 16 |
| align (bytes) | 8 |
| reordered | no |
| drop flag | no |

| slot | field | type | size | align | offset |
| --- | --- | --- | --- | --- | --- |
| 0 | x | `f64` | 8 | 8 | 0 |
| 1 | y | `f64` | 8 | 8 | 8 |
| - | (tail padding) | - | 0 | - | 16 |

Native only. On wasm32 a pointer is 4 bytes and the `null_pointer` niche does not exist, so these numbers are not valid there for any type that holds a pointer.
