# Use of a private name from another module

## Why
A `private` declaration is visible only inside the module that declares it (`E1022`). Here `Array.min_index` is a helper inside the standard `Array` module, and only its public functions can be called from elsewhere.

## Bad
```tsuzuri
def smallest :: [i64] -> i64 = \xs -> Array.min_index xs
```

## Good
```tsuzuri
def smallest :: [i64] -> i64 = \xs -> Array.min xs
```
For your own module, remove `private` from the declaration or add a public function that wraps it.

## See also
- `private_type_in_public_api`
- `unknown_module_member`
