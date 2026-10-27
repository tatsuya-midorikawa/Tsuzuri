# Doc comment with nothing to document

## Why
A `///` doc comment documents the API declaration that follows it, such as a `def` signature or a `record`, `union`, `type`, `class`, `const`, or `extern` declaration. Before an implementation (`fn`), a plain `let`, a builder alias, or an expression it has nothing to attach to (`E0002`).

## Bad
```tsuzuri
def add_one :: i64 -> i64
/// Adds one.
fn add_one x = x + 1
```

## Good
```tsuzuri
/// Adds one.
def add_one :: i64 -> i64
fn add_one x = x + 1
```
Move the comment above the signature, or make it a plain `//` comment.
