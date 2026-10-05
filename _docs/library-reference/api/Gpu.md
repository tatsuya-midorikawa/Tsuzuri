# Gpu

## `Backend`

```tsuzuri
union Backend =
  | CpuReference
  | WebGpu
  | Vulkan
  | Cuda
  | Metal
  | Auto
```

## `Error`

```tsuzuri
union Error =
  | Unavailable
```

## `Device`

```tsuzuri
record Device {
  backend: Backend
}
```

## `Buffer`

```tsuzuri
record Buffer<'a> {
  values: ['a]
}
```

## `request`

```tsuzuri
def request :: Backend -> Result<Device, Error>
```

## `backend`

```tsuzuri
def backend :: ref Device -> Backend
```

## `init`

```tsuzuri
def init :: ref Device -> i64 -> (i32 -> 'a) -> Buffer<'a>
```

## `map`

```tsuzuri
def map :: Copy<'a> => ref Device -> ('a -> 'b) -> Buffer<'a> -> Buffer<'b>
```

## `from_array`

```tsuzuri
def from_array :: Copy<'a> => ref Device -> ref ['a] -> Buffer<'a>
```

## `to_array`

```tsuzuri
def to_array :: Buffer<'a> -> ['a]
```

