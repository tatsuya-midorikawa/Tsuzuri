# Task that captures a borrow

## Why
A `task { ... }` captures only owned values: a captured value that holds a borrow is rejected (`E1013`). That includes a closure that captured a borrowed parameter, because the closure's environment holds that borrow.

## Bad
```tsuzuri
def total :: ref [i64] -> i64 = \values ->
    let size = \_ -> values.length
    Task.run (task { return size () })
```

## Good
```tsuzuri
def total :: ref [i64] -> i64 = \values ->
    let size = values.length
    Task.run (task { return size })
```
Compute what the task needs before it starts, and let the task capture the owned result.

## See also
- `task_requires_owned_value`
- `task_result_retains_borrow`
