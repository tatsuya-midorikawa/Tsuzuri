# Editions

The same project name in two edition blocks is two projects, each checked in its edition.

<!--if edition=2026-->
```tsuzuri project=demo run=2026
def main :: unit -> i32 = \() ->
    do! IO.writeln "2026"
    0
```
<!--endif-->
<!--if edition=2027-->
```tsuzuri project=demo run=2027
def main :: unit -> i32 = \() ->
    do! IO.writeln "2027"
    0
```
<!--endif-->
