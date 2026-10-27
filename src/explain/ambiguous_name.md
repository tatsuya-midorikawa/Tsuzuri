# Ambiguous name

## Why
The name is declared in more than one module that is visible here, for example in two namespaces opened with `using`, or in several std modules, and nothing says which one is meant (`E1004`). The message lists the candidates.

## Bad
```tsuzuri
def f :: Error -> i64 = \_e -> 1
```

## Good
```tsuzuri
def f :: IO.Error -> i64 = \_e -> 1
```
Qualify the name with its module.

## See also
- `unknown_type_name`
