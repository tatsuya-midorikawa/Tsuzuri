// The checksums of benchmarks/list-memory/Main.tz, from closed forms in n (BigInt, independent of the compiler).
export const workloads = ["bulk_new", "cons_build", "tail_consume", "clone_list", "index_walk", "string_list"];

// The sizes the runner measures. index_walk is quadratic on a node list, so it stops at 10,000.
export const sizes = {
  allocations: [1, 2, 3, 4, 8, 16, 1000, 1000000],
  indexAllocations: [1, 2, 3, 4, 8, 16, 1000, 10000],
  measured: [1, 16, 1000, 1000000],
  indexMeasured: [16, 1000, 10000],
  quick: [1, 3, 16, 1000],
  wasm: [1000, 100000, 400000, 600000, 1000000, 2000000],
};

export function reference(name, size) {
  const n = BigInt(size);
  switch (name) {
    case "bulk_new": case "cons_build": case "tail_consume": case "index_walk": return n * (n - 1n) / 2n;
    case "clone_list": return n * (n - 1n);
    case "string_list": return 3n * n;
    default: throw new Error(`unknown list-memory workload: ${name}`);
  }
}

// How many calls one sample makes, so that a sample of a small size is not below the resolution of the clock.
export function repeats(name, size, quick) {
  if (quick) return size >= 1000000 ? 1 : 3;
  if (name === "index_walk") return size >= 10000 ? 3 : size >= 1000 ? 100 : 5000;
  return size >= 1000000 ? 3 : size >= 1000 ? 200 : 20000;
}
