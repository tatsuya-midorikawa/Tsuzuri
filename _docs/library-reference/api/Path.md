# Path

## `join`

```tsuzuri
def join :: ref string -> ref string -> string
```

Joins two paths with "/". An empty left path gives the right one, and an absolute right path
is only appended: nothing here is a sandbox.

## `parent`

```tsuzuri
def parent :: ref string -> Option<string>
```

The path without its last name, without trailing "/" except for a lone "/".
"a" and "/" and "" have none.

## `file_name`

```tsuzuri
def file_name :: ref string -> Option<string>
```

The last name of a path, or `None` for "", "/", ".", and "..".

## `extension`

```tsuzuri
def extension :: ref string -> Option<string>
```

What follows the last "." of the last name; a name that starts with its only "." has none.

