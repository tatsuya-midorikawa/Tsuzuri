# Two default arms in a select

## Why
`default` is the arm that a `select` takes when no other arm can go on at once, so a select has at most one (`E0002`). It may stand anywhere among the arms.

## Bad
```tsuzuri
let jobs: Channel<i64> = Channel.make 1
select
| recv _ from jobs -> 1
| default -> 0
| default -> 2
```

## Good
```tsuzuri
let jobs: Channel<i64> = Channel.make 1
select
| recv _ from jobs -> 1
| default -> 0
```

## See also
- `select_arm_expected`
