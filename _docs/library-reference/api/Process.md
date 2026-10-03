# Process

## `Output`

```tsuzuri
record Output {
  code: i32
  signal: i32
  stdout: [ubyte]
  stderr: [ubyte]
}
```

How a finished process ended: its exit `code`, or -1 and the `signal` that ended it, and the bytes
it wrote to its standard output and its standard error.

## `run`

```tsuzuri
def run :: string -> [string] -> [ubyte] -> IO<Result.Result<Output, Os.Error>>
```

Runs a program and waits for it, with no shell in between: `args` are its arguments after its own name,
each passed as one argument whatever it contains, and `input` is written to its standard input, which is
then closed. A program name without "/" is looked up on PATH, and the child inherits the environment and
the working directory. Both output streams are collected, up to 2^30 bytes together, after which the child
is killed and the call is `Other`. Not available under `--wasm-host wasi`, where it is `Other`.

