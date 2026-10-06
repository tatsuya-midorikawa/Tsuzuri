// F13: --allocator host|counting and --freestanding, end to end.
// Usage: node tests/allocator.mjs target/release/tsuzuri
import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);
// Command-line errors end with status 2; errors found while building, with status 1.
function rejects(args, message, status = 2) {
  const result = execute(compiler, args, false);
  assert.equal(result.status, status, `${args.join(" ")}\n${result.stderr}`);
  assert.match(result.stderr, /error\[E2000\]/, args.join(" "));
  assert.ok(result.stderr.includes(message), `${args.join(" ")}\n${result.stderr}`);
}
const symbols = (object) => execute("nm", [object]).stdout;
const undefinedSymbols = (object) => execute("nm", ["-u", object]).stdout.split("\n").map((line) => line.trim().replace(/^U\s+/, "")).filter(Boolean);

const work = mkdtempSync(join(tmpdir(), "tsuzuri-allocator-"));
try {
  const abi = join(work, "abi");
  cpSync(join(root, "tests/fixtures/host_abi"), abi, { recursive: true });
  // Vec growth reaches @tz.realloc.
  const grow = join(work, "grow");
  mkdirSync(grow);
  writeFileSync(join(grow, "Grow.tz"), `export def grow :: i64 -> i64
fn grow count =
    let mut values: Vec<i64> = Vec.empty()
    for index in 1i64 .. count do values = Vec.push values index
    let result = Vec.to_array values
    Array.sum ref result
`);
  const library = join(grow, "Grow.tz");

  // A. Rejected combinations.
  const hostOutput = "--allocator host requires object, LLVM IR, header, or WebAssembly output";
  rejects(["build", abi, "--allocator", "host", "-o", join(work, "x")], hostOutput);
  rejects(["build", abi, "--emit", "wgsl", "--allocator", "host", "-o", join(work, "x.wgsl")], hostOutput);
  rejects(["build", abi, "--emit", "object", "--allocator", "pool", "-o", join(work, "x.o")], "allocator must be 'system', 'host', or 'counting'");
  rejects(["build", abi, "--target", "wasm32", "--wasm-feature", "threads", "--allocator", "host", "-o", join(work, "x.wasm")], "--allocator host cannot be combined with --wasm-feature threads");
  rejects(["run", abi, "--allocator", "system"], "--allocator is only valid with build");
  rejects(["build", abi, "--allocator", "counting", "-o", join(work, "x")], "--allocator counting requires object, LLVM IR, header, or WebAssembly output");
  rejects(["build", abi, "--emit", "object", "--freestanding", "-o", join(work, "x.o")], "--freestanding requires --allocator host");
  rejects(["build", abi, "--target", "wasm32", "--allocator", "host", "--freestanding", "-o", join(work, "x.wasm")], "--freestanding requires a native target");

  // B. --allocator system is the default: the same IR, header, object symbols, and WASM.
  const same = (args, file) => {
    const plain = join(work, `plain-${file}`), system = join(work, `system-${file}`);
    cli(["build", abi, ...args, "--no-cache", "-o", plain]);
    cli(["build", abi, ...args, "--allocator", "system", "--no-cache", "-o", system]);
    return [plain, system];
  };
  for (const [args, file] of [[["--emit", "llvm"], "abi.ll"], [["--emit", "header"], "abi.h"]]) {
    const [plain, system] = same(args, file);
    assert.equal(readFileSync(system, "utf8"), readFileSync(plain, "utf8"), file);
  }
  for (const optimization of ["-O0", "-O3"]) {
    const [plain, system] = same(["--emit", "object", optimization], `abi${optimization}.o`);
    assert.equal(symbols(system), symbols(plain), `object symbols ${optimization}`);
    const [plainWasm, systemWasm] = same(["--target", "wasm32", optimization], `abi${optimization}.wasm`);
    assert.deepEqual(readFileSync(systemWasm), readFileSync(plainWasm), `wasm ${optimization}`);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(systemWasm))), []);
  }

  // C. The host allocator's IR: deterministic, declared once, without the C library's allocator.
  const hostIr = join(work, "host.ll"), hostAgain = join(work, "host-again.ll");
  cli(["build", abi, "--emit", "llvm", "--allocator", "host", "-o", hostIr]);
  cli(["build", abi, "--emit", "llvm", "--allocator", "host", "-o", hostAgain]);
  const hostText = readFileSync(hostIr, "utf8");
  assert.equal(hostText, readFileSync(hostAgain, "utf8"));
  const declarations = hostText.match(/^declare .*$/gm);
  assert.equal(new Set(declarations).size, declarations.length);
  for (const declaration of ["declare ptr @tsuzuri_host_alloc(i64, i64)", "declare void @tsuzuri_host_free(ptr, i64, i64)", "declare ptr @tsuzuri_host_realloc(ptr, i64, i64, i64)"]) {
    assert.ok(hostText.includes(declaration), declaration);
  }
  assert.ok(!/@(malloc|free|realloc)\(/.test(hostText));

  // D. The header's prototypes compile as C and C++ (inside extern "C").
  const header = join(work, "lib.h");
  cli(["build", abi, "--emit", "header", "--allocator", "host", "-o", header]);
  for (const prototype of ["void *tsuzuri_host_alloc(uint64_t size, uint64_t align);", "void tsuzuri_host_free(void *ptr, uint64_t size, uint64_t align);", "void *tsuzuri_host_realloc(void *ptr, uint64_t old_size, uint64_t new_size, uint64_t align);"]) {
    assert.ok(readFileSync(header, "utf8").includes(prototype), prototype);
  }
  const include = join(work, "include.c");
  writeFileSync(include, '#include "lib.h"\nvoid *(*pick)(uint64_t, uint64_t) = tsuzuri_host_alloc;\n');
  execute(clang, ["-fsyntax-only", "-std=c11", `-I${work}`, include]);
  execute(clang, ["-fsyntax-only", "-x", "c++", "-std=c++17", `-I${work}`, include]);

  // E-G. Native objects with the C host of tests/allocator_host.c: counts, sizes, boundaries, failures.
  const runHost = (object, label) => {
    const binary = join(work, `${label}-host`);
    execute(clang, ["-std=c11", "-O1", `-I${work}`, join(root, "tests/allocator_host.c"), object, "-lm", "-o", binary]);
    const output = execute(binary, []).stdout.trim();
    const match = /^allocations=(\d+) frees=(\d+) live=0$/.exec(output);
    assert.ok(match && match[1] === match[2] && Number(match[1]) > 1024, `${label}: ${output}`);
    for (const failure of ["oom", "misaligned"]) {
      const result = execute(binary, [failure], false);
      assert.ok(result.status !== 0 || result.signal, `${label}: ${failure} must trap`);
    }
    return output;
  };
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(work, `host${optimization}.o`);
    cli(["build", abi, "--emit", "object", "--allocator", "host", optimization, "--no-cache", "-o", object]);
    const table = symbols(object);
    for (const name of ["tsuzuri_host_alloc", "tsuzuri_host_free"]) {
      assert.match(table, new RegExp(`\\bU _?${name}\\b`), `${name} stays undefined`);
    }
    assert.ok(!/\b_?malloc\b/.test(table), "no malloc");
    runHost(object, `host${optimization}`);
  }

  // Phase 2: --allocator counting on native objects.
  const growHeader = join(work, "grow.h");
  cli(["build", library, "--emit", "header", "--allocator", "counting", "-o", growHeader]);
  assert.ok(readFileSync(growHeader, "utf8").includes("void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats);"));
  const countingHost = join(work, "counting.c");
  writeFileSync(countingHost, `#include <assert.h>
#include <stdio.h>
#include "grow.h"
int main(void) {
    tsuzuri_allocation_stats stats;
    tsuzuri_alloc_stats(&stats);
    assert(stats.allocations == 0 && stats.frees == 0 && stats.live_bytes == 0 && stats.peak_bytes == 0);
    assert(tz_grow(100) == 5050);
    tsuzuri_alloc_stats(&stats);
    assert(stats.allocations > 0 && stats.allocations == stats.frees && stats.live_bytes == 0);
    assert(stats.peak_bytes >= 800);
    uint64_t peak = stats.peak_bytes;
    assert(tz_grow(10) == 55);
    tsuzuri_alloc_stats(&stats);
    assert(stats.peak_bytes == peak && stats.live_bytes == 0);
    printf("allocations=%llu peak=%llu\\n", (unsigned long long)stats.allocations, (unsigned long long)stats.peak_bytes);
    return 0;
}
`);
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(work, `counting${optimization}.o`);
    cli(["build", library, "--emit", "object", "--allocator", "counting", optimization, "--no-cache", "-o", object]);
    assert.match(symbols(object), /\bT _?tsuzuri_alloc_stats\b/);
    const binary = join(work, `counting${optimization}`);
    execute(clang, ["-std=c11", `-I${work}`, countingHost, object, "-lm", "-o", binary]);
    assert.match(execute(binary, []).stdout, /^allocations=\d+ peak=\d+\n$/);
  }

  // Phase 2: --allocator counting and --allocator host on WebAssembly.
  for (const optimization of ["-O0", "-O3"]) {
    // C08: Parallel.for_each_chunk runs at most 1,024 jobs, so one-element chunks of 100,000 values
    // (800,000 bytes) copy a callback chosen at run time 1,024 times, not once per chunk.
    const slices = join(work, `slices${optimization}.wasm`);
    cli(["build", join(root, "tests/fixtures/mutable_slices"), "--target", "wasm32", "--allocator", "counting", optimization, "--no-cache", "-o", slices]);
    const chunked = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(slices))).exports;
    const stats = chunked.tsuzuri_alloc(32n);
    assert.equal(chunked.tz_parallel_dynamic(100000n, 1n), 4999950000n + 100000n * 1000001n);
    chunked.tsuzuri_alloc_stats(stats);
    const [, , chunkedLive, chunkedPeak] = new BigUint64Array(chunked.memory.buffer, stats, 4);
    assert.ok(chunkedLive === 32n && chunkedPeak < 1000000n, `for_each_chunk ${optimization}: live ${chunkedLive} peak ${chunkedPeak}`);

    const counted = join(work, `counting${optimization}.wasm`);
    cli(["build", library, "--target", "wasm32", "--allocator", "counting", optimization, "--no-cache", "-o", counted]);
    const countedModule = new WebAssembly.Module(readFileSync(counted));
    assert.deepEqual(WebAssembly.Module.imports(countedModule), []);
    const api = new WebAssembly.Instance(countedModule).exports;
    const scratch = api.tsuzuri_alloc(32n);
    const read = () => {
      api.tsuzuri_alloc_stats(scratch);
      return [...new BigUint64Array(api.memory.buffer, scratch, 4)];
    };
    // The scratch buffer is itself one live allocation of 32 bytes.
    const [allocations, frees, live] = read();
    assert.deepEqual([allocations, frees, live], [1n, 0n, 32n]);
    assert.equal(api.tz_grow(100n), 5050n);
    const [after, freed, still, peak] = read();
    assert.ok(after > allocations && after - allocations === freed - frees && still === 32n && peak >= 832n, `${after} ${freed} ${still} ${peak}`);
    api.tsuzuri_free(scratch);

    const hosted = join(work, `host${optimization}.wasm`);
    cli(["build", library, "--target", "wasm32", "--allocator", "host", optimization, "--no-cache", "-o", hosted]);
    const hostedModule = new WebAssembly.Module(readFileSync(hosted));
    const imports = WebAssembly.Module.imports(hostedModule).map(({ module, name }) => `${module}.${name}`).sort();
    assert.deepEqual(imports, ["tsuzuri_heap.alloc", "tsuzuri_heap.free", "tsuzuri_heap.realloc"]);
    // A bump allocator above __heap_base that checks every size it is given back.
    let instance, next = 0, live2 = 0n, calls = { alloc: 0, free: 0, realloc: 0 };
    const blocks = new Map();
    const allocate = (size) => {
      const memory = instance.exports.memory;
      if (next === 0) next = (instance.exports.__heap_base.value + 15) & ~15;
      const address = next;
      next += Number((size + 15n) & ~15n);
      if (next > memory.buffer.byteLength) memory.grow(Math.ceil((next - memory.buffer.byteLength) / 65536));
      blocks.set(address, size);
      live2 += size;
      return address;
    };
    const release = (address, size) => {
      assert.equal(blocks.get(address), size);
      blocks.delete(address);
      live2 -= size;
    };
    instance = new WebAssembly.Instance(hostedModule, { tsuzuri_heap: {
      alloc(size, align) { assert.equal(align, 16n); assert.ok(size >= 16n); calls.alloc++; return allocate(size); },
      free(address, size, align) { assert.equal(align, 16n); calls.free++; release(address, size); },
      realloc(address, oldSize, newSize, align) {
        assert.equal(align, 16n);
        calls.realloc++;
        const moved = allocate(newSize);
        const bytes = new Uint8Array(instance.exports.memory.buffer);
        bytes.copyWithin(moved, address, address + Number(oldSize < newSize ? oldSize : newSize));
        release(address, oldSize);
        return moved;
      },
    } });
    assert.equal(instance.exports.tz_grow(100n), 5050n);
    assert.ok(calls.alloc > 0 && calls.realloc > 0 && calls.alloc === calls.free && live2 === 0n, JSON.stringify(calls));
  }

  // Phase 3: --freestanding objects reference no C library function.
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(work, `free${optimization}.o`);
    cli(["build", abi, "--emit", "object", "--allocator", "host", "--freestanding", optimization, "--no-cache", "-o", object]);
    // Freestanding C still provides memcpy, memmove, memset, and memcmp; the rest is the host's.
    const allowed = /^_?(tsuzuri_host_(alloc|free|realloc)|memcpy|memmove|memset|memcmp|bzero|__bzero)$/;
    const unexpected = undefinedSymbols(object).filter((name) => !allowed.test(name));
    assert.deepEqual(unexpected, [], `freestanding ${optimization} undefined symbols`);
    assert.equal(runHost(object, `free${optimization}`).split(" ")[0], runHost(join(work, `host${optimization}.o`), `again${optimization}`).split(" ")[0]);
  }
  // The header of a freestanding build is the ordinary host-allocator header.
  cli(["build", abi, "--emit", "header", "--allocator", "host", "--freestanding", "-o", join(work, "free.h")]);
  assert.equal(readFileSync(join(work, "free.h"), "utf8"), readFileSync(header, "utf8"));
  const freestanding = (name, source, message) => {
    const project = join(work, name);
    mkdirSync(project);
    writeFileSync(join(project, "Main.tz"), source);
    // A header holds no IR, so it is checked against the IR the object would hold.
    for (const [emit, extension] of [["object", "o"], ["llvm", "ll"], ["header", "h"]]) {
      rejects(["build", project, "--emit", emit, "--allocator", "host", "--freestanding", "-o", join(work, `${name}.${extension}`)], message, 1);
    }
  };
  freestanding("io", "IO { do! IO.write_line 42 }\n", "--freestanding cannot use the standard IO");
  freestanding("tasks", "export def total :: i64 -> i64\nfn total count =\n    let tasks = new [Task<i64>](4, index -> task { return index * count })\n    let results = Task.run (Task.parallel tasks)\n    Array.sum (ref results)\n", "--freestanding cannot use parallel tasks");
  freestanding("debug", "export def traced :: i64 -> i64\nfn traced value = Debug.trace value\n", "--freestanding cannot use Debug output");
  console.log("allocator: ok");
} finally {
  rmSync(work, { recursive: true, force: true });
}
