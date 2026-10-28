# Ownership of `Main.demo`

## text

```mermaid
stateDiagram-v2
    direction LR
    state "Owned" as s0
    [*] --> s0
    state "Borrowed" as s1
    s0 --> s1 : L10 borrow → Main.peek
    state "Owned" as s2
    s1 --> s2 : release
    state "Moved" as s3
    s2 --> s3 : L11 move → Main.consume
    s3 --> [*]
    classDef moved fill:#ffebe9,stroke:#cf222e
    classDef borrowed fill:#fff8c5,stroke:#9a6700
    class s3 moved
    class s1 borrowed
```

## label

```mermaid
stateDiagram-v2
    direction LR
    state "Owned" as s0
    [*] --> s0
    state "Borrowed" as s1
    s0 --> s1 : L13 borrow → Main.peek
    state "Owned" as s2
    s1 --> s2 : release
    state "Dropped" as s3
    s2 --> s3 : L14 drop
    s3 --> [*]
    classDef moved fill:#ffebe9,stroke:#cf222e
    classDef borrowed fill:#fff8c5,stroke:#9a6700
    class s3 moved
    class s1 borrowed
```

| # | line | var | event | to | diagnostic role |
| --- | --- | --- | --- | --- | --- |
| 1 | 10 | text | borrow | Main.peek | borrowed_here |
| 2 | 11 | text | move | Main.consume | moved_here |
| 3 | 13 | label | borrow | Main.peek | borrowed_here |
| 4 | 14 | label | drop | - | - |
