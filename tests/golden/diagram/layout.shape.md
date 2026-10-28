# Layout of `Acme::Kb::Shapes.Shape` (native, 64-bit)

| item | value |
| --- | --- |
| kind | union |
| size (bytes) | 16 |
| align (bytes) | 8 |
| layout | common |
| tag bytes | 1 |
| niche | none |
| drop flag | no |

| case | payload |
| --- | --- |
| Circle | `f64` |
| Empty | - |

Native only. On wasm32 a pointer is 4 bytes and the `null_pointer` niche does not exist, so these numbers are not valid there for any type that holds a pointer.
