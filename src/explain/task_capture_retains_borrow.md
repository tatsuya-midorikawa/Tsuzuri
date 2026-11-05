# Borrow captured by a tzroutine

## Why
A `tzroutine { ... }` captures only owned values: a captured value that holds a borrow is rejected (`E1013`). That includes a closure that captured a borrowed parameter, because the closure's environment holds that borrow.

## Bad
```tsuzuri
def report :: ref [i64] -> unit = \values ->
    let size = \_ -> values.length
    tzroutine {
        let _count = size ()
    }
```

## Good
```tsuzuri
def report :: ref [i64] -> unit = \values ->
    let size = values.length
    tzroutine {
        let _count = size
    }
```
Compute what the tzroutine needs before it starts, and let the tzroutine capture the owned result.

## See also
- `task_requires_owned_value`
- `task_result_retains_borrow`
