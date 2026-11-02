# A select arm of another shape

## Why
Each arm of a `select` starts with one of three heads: `recv NAME from CHANNEL` (`NAME` may be `_`), `send VALUE to CHANNEL`, or `default` (`E0002`). A `when` guard may follow a `recv` or `send` head, not `default`. A select has no patterns: to look at what a receive got, match the `Maybe` in the arm's body.

## Bad
```tsuzuri
let jobs: Channel<i64> = Channel.make 1
select
| recv job from jobs -> job
| Maybe.Some x -> x
```

## Good
```tsuzuri
let jobs: Channel<i64> = Channel.make 1
select
| recv job from jobs -> match job with | Maybe.Some x -> x | Maybe.None -> 0
| default -> 0
```

## See also
- `select_default_repeated`
- `match_without_arms`
