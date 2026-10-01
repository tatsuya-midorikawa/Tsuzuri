import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const wasmLd = process.env.TSUZURI_WASM_LD ?? "wasm-ld";
const MiB = 1024 * 1024;
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-wasm-memory-"));

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 120000 });
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);

// Independent of the compiler: limits of the module's own memory, in 64 KiB pages.
function definedMemory(bytes) {
  let position = 8;
  const leb = () => { let value = 0, shift = 0, byte; do { byte = bytes[position++]; value += (byte & 127) * 2 ** shift; shift += 7; } while (byte & 128); return value; };
  while (position < bytes.length) {
    const id = bytes[position++], size = leb(), end = position + size;
    if (id === 5) {
      assert.equal(leb(), 1);
      const flags = bytes[position++], minimum = leb();
      return { flags, minimum, maximum: flags & 1 ? leb() : null };
    }
    position = end;
  }
  return null;
}

function load(path) {
  const module = new WebAssembly.Module(readFileSync(path));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  return module;
}

// Each call gets a fresh instance, so the heap starts empty right after the stack and static data.
function checkLimit(module, limit, label) {
  for (const length of [32n * BigInt(MiB), BigInt(limit - 2 * MiB)]) {
    const api = new WebAssembly.Instance(module).exports;
    assert.equal(api.tz_bytes_length(length), length, `${label}: ${length}`);
    assert.ok(api.memory.buffer.byteLength <= limit, label);
  }
  assert.throws(() => new WebAssembly.Instance(module).exports.tz_bytes_length(BigInt(limit - MiB)), WebAssembly.RuntimeError, label);
}

const heapLines = (limit) => [
  `  %fits = icmp ule i64 %size, ${limit - 32}`,
  // Above 2 GiB, %begin + %needed can wrap in i32, so the heap compares the room left.
  limit > 2 ** 31 ? `  %room = sub i32 ${limit}, %begin\n  %within = icmp ule i32 %needed, %room` : `  %within = icmp ule i32 %end, ${limit}`,
  `  %fits = icmp ule i64 %new_size, ${limit - 32}`,
];
const occurrences = (text, line) => text.split(`${line}\n`).length - 1;

// The runtime heap alone, with the limit substituted independently of the compiler.
function checkHeapBoundaries(limit, optimization) {
  let heap = readFileSync(resolve("src/runtime/heap-wasm.ll"), "utf8");
  for (const [before, after] of heapLines(16 * MiB).map((line, index) => [line, heapLines(limit)[index]])) {
    assert.equal(occurrences(heap, before), 1, before);
    heap = heap.replace(`${before}\n`, `${after}\n`);
  }
  assert.doesNotMatch(heap, /16777184|16777216/);
  const input = join(directory, `heap-${limit}.ll`);
  const object = join(directory, `heap-${limit}-${optimization}.o`), output = join(directory, `heap-${limit}-${optimization}.wasm`);
  writeFileSync(input, `declare void @llvm.trap()\n${heap}\ndefine ptr @allocate(i64 %size) {\n  %value = call ptr @tz.alloc(i64 %size)\n  ret ptr %value\n}\n`);
  execute(clang, ["--target=wasm32", `-O${optimization}`, "-Wno-override-module", "-c", input, "-o", object]);
  execute(wasmLd, [object, "--no-entry", "--export=allocate", "--export=__heap_base", "--export-memory", `--max-memory=${limit}`, "-o", output]);
  const module = load(output);
  const fresh = () => new WebAssembly.Instance(module).exports;
  const api = fresh();
  const base = (api.__heap_base.value + 15) & -16;
  assert.equal(api.allocate(BigInt(limit - base - 16)) >>> 0, base + 16);
  assert.equal(api.memory.buffer.byteLength, limit);
  assert.throws(() => api.allocate(1n), WebAssembly.RuntimeError);
  // The first request fails the size check; the second passes it but its block ends past the limit
  // (near 4 GiB, %begin + %needed would wrap to a small address that passes an %end comparison).
  assert.throws(() => fresh().allocate(BigInt(limit - 31)), WebAssembly.RuntimeError);
  assert.throws(() => fresh().allocate(BigInt(limit - 32)), WebAssembly.RuntimeError);
}

const MAX_WASM32 = 4294901760;
const deepSource = `def rec deep :: i64 -> i64 = \\n ->
    let values = [${Array.from({ length: 1000 }, (_, index) => `n + ${index}`).join(", ")}]
    if n == 0 then 0 else values[deep (n - 1) % 1000]

export def deep_call :: i64 -> i64 = \\n -> deep n

export def frame :: i64 -> i64 = \\seed ->
    let values = [${Array.from({ length: 8000 }, (_, index) => `seed + ${index}`).join(", ")}]
    values[seed % 8000]
`;

try {
  const root = join(directory, "project");
  cpSync(resolve("tests/fixtures/wasm_memory"), root, { recursive: true });
  for (const optimization of [0, 3]) {
    const build = (name, ...options) => {
      const output = join(directory, `${name}-${optimization}.wasm`);
      cli(["build", root, "--target", "wasm32", `-O${optimization}`, "--no-cache", ...options, "-o", output]);
      return output;
    };
    const omitted = build("default");
    const explicit = build("explicit", "--wasm-max-memory", "16MiB", "--wasm-stack-size", "1MiB");
    assert.deepEqual(readFileSync(explicit), readFileSync(omitted));
    const defaults = definedMemory(readFileSync(omitted));
    assert.equal(defaults.maximum, 256);
    // Stack (16 pages) plus at most one page of static data keeps the 62 MiB request below 64 MiB.
    assert.ok(defaults.minimum <= 17, `${defaults.minimum}`);
    load(omitted);

    const large = build("large", "--wasm-max-memory", "64MiB");
    assert.deepEqual(readFileSync(build("large-again", "--wasm-max-memory", "64MiB")), readFileSync(large));
    assert.deepEqual(definedMemory(readFileSync(large)), { flags: 1, minimum: defaults.minimum, maximum: 1024 });
    checkLimit(load(large), 64 * MiB, `64 MiB O${optimization}`);

    const stack = build("stack", "--wasm-stack-size", "4MiB", "--wasm-max-memory", "64MiB");
    assert.deepEqual(definedMemory(readFileSync(stack)), { flags: 1, minimum: defaults.minimum + 48, maximum: 1024 });
    assert.equal(new WebAssembly.Instance(load(stack)).exports.tz_bytes_length(1024n), 1024n);

    const largest = build("largest", "--wasm-max-memory", "2GiB");
    assert.deepEqual(definedMemory(readFileSync(largest)), { flags: 1, minimum: defaults.minimum, maximum: 32768 });
    assert.equal(new WebAssembly.Instance(load(largest)).exports.tz_bytes_length(32n * BigInt(MiB)), 32n * BigInt(MiB));
    const near4 = build("near-4-gib", "--wasm-max-memory", "4194240KiB");
    assert.deepEqual(definedMemory(readFileSync(near4)), { flags: 1, minimum: defaults.minimum, maximum: 65535 });
    assert.equal(new WebAssembly.Instance(load(near4)).exports.tz_bytes_length(32n * BigInt(MiB)), 32n * BigInt(MiB));

    for (const options of [[], ["--trap-info"], ["-g"]]) {
      const output = join(directory, `limit-${optimization}${options.join("")}.ll`);
      cli(["build", root, "--target", "wasm32", "--emit", "llvm", "--wasm-max-memory", "64MiB", `-O${optimization}`, "--no-cache", ...options, "-o", output]);
      const text = readFileSync(output, "utf8");
      if (options.length === 0) {
        cli(["build", root, "--target", "wasm32", "--emit", "llvm", "--wasm-max-memory", "64MiB", `-O${optimization}`, "--no-cache", "-o", `${output}.again`]);
        assert.equal(readFileSync(`${output}.again`, "utf8"), text);
      }
      const lines = text.split("\n");
      for (const line of heapLines(64 * MiB)) assert.equal(lines.filter((entry) => entry === line).length, 1, `${options} ${line}`);
      for (const line of heapLines(16 * MiB)) assert.equal(lines.filter((entry) => entry === line).length, 0, `${options} ${line}`);
      assert.ok(!text.includes("@tz.stack.check"), `${options}`);
    }
    const nearIr = join(directory, `near-4-gib-${optimization}.ll`);
    cli(["build", root, "--target", "wasm32", "--emit", "llvm", "--wasm-max-memory", "4194240KiB", `-O${optimization}`, "--no-cache", "-o", nearIr]);
    const nearText = readFileSync(nearIr, "utf8");
    for (const line of heapLines(MAX_WASM32)) assert.equal(occurrences(nearText, line), 1, line);
    const definitions = nearText.split("\n").filter((line) => line.startsWith("define ")).length;
    // Every definition except the check itself and the appended i128 helpers starts with a check.
    const helpers = readFileSync(resolve("src/runtime/wasm.ll"), "utf8").split("\n").filter((line) => line.startsWith("define ")).length;
    assert.equal(occurrences(nearText, "  call void @tz.stack.check()"), definitions - 1 - helpers);

    const object = join(directory, `object-${optimization}.o`), linked = join(directory, `object-${optimization}.wasm`);
    cli(["build", root, "--target", "wasm32", "--emit", "object", "--wasm-max-memory", "64MiB", `-O${optimization}`, "--no-cache", "-o", object]);
    execute(wasmLd, ["--no-entry", "--stack-first", "-z", "stack-size=1048576", "--max-memory=67108864", "--export=tz_bytes_length", object, "-o", linked]);
    checkLimit(load(linked), 64 * MiB, `object O${optimization}`);

    for (const limit of [64 * MiB, 2048 * MiB, 2048 * MiB + 65536, MAX_WASM32]) checkHeapBoundaries(limit, optimization);

    const deepRoot = join(directory, "deep");
    mkdirSync(deepRoot, { recursive: true });
    writeFileSync(join(deepRoot, "Main.tz"), deepSource);
    const deepBuild = (name, ...options) => {
      const output = join(directory, `deep-${name}-${optimization}.wasm`);
      cli(["build", deepRoot, "--target", "wasm32", `-O${optimization}`, "--no-cache", ...options, "-o", output]);
      return new WebAssembly.Instance(load(output)).exports;
    };
    // The check follows the frame allocation: a 64,000-byte frame leaves under 4096 bytes of a 64 KiB stack.
    assert.throws(() => deepBuild("tight", "--wasm-max-memory", "4194240KiB", "--wasm-stack-size", "64KiB").tz_frame(7n), WebAssembly.RuntimeError);
    assert.equal(deepBuild("unchecked", "--wasm-max-memory", "2GiB", "--wasm-stack-size", "64KiB").tz_frame(7n), 14n);
    const checked = deepBuild("checked", "--wasm-max-memory", "4194240KiB");
    assert.equal(checked.tz_frame(7n), 14n);
    // With memory grown to the limit, a stack that wraps below address 0 could land in mapped memory.
    checked.memory.grow(MAX_WASM32 / 65536 - checked.memory.buffer.byteLength / 65536);
    const top = new Uint8Array(checked.memory.buffer, MAX_WASM32 - MiB, MiB).fill(0x5a);
    assert.throws(() => checked.tz_deep_call(200n), WebAssembly.RuntimeError);
    assert.ok(top.every((value) => value === 0x5a));

    const failing = execute(compiler, ["test", root, "--target", "wasm32", `-O${optimization}`], false);
    assert.equal(failing.status, 1, `${failing.stdout}\n${failing.stderr}`);
    assert.match(failing.stdout, /not ok 2 - Main allocates 32 MiB/);
    assert.match(failing.stdout, /1 passed; 1 failed; 0 ignored/);
    const passing = execute(compiler, ["test", root, "--target", "wasm32", "--wasm-max-memory", "64MiB", `-O${optimization}`]);
    assert.match(passing.stdout, /2 passed; 0 failed; 0 ignored/);
    console.log(`WASM memory O${optimization}: default output unchanged; 64 MiB, 4 MiB stack, 2 GiB, 4 GiB - 64 KiB, IR, object, heap boundaries, stack checks and tests honor the limit`);
  }

  const memoryRange = "--wasm-max-memory must be a multiple of 64 KiB and at most 4 GiB - 64 KiB on wasm32";
  const invalid = [
    [["build", root, "--target", "wasm32", "--wasm-max-memory", "64MB"], "--wasm-max-memory must be a byte count or a number followed by KiB, MiB, or GiB, such as 67108864 or 64MiB"],
    [["build", root, "--target", "wasm32", "--wasm-max-memory", "100000"], memoryRange],
    [["build", root, "--target", "wasm32", "--wasm-max-memory", "4GiB"], memoryRange],
    [["build", root, "--target", "wasm64", "--wasm-max-memory", "17GiB"], "--wasm-max-memory must be a multiple of 64 KiB and at most 16 GiB on wasm64"],
    [["build", root, "--target", "wasm32", "--wasm-max-memory", "1MiB"], "--wasm-max-memory must be at least the stack size plus 64 KiB; raise the memory limit or lower --wasm-stack-size"],
    [["build", root, "--target", "wasm32", "--wasm-stack-size", "1000"], "--wasm-stack-size must be a multiple of 16 bytes and at least 64 KiB"],
    [["run", root, "--wasm-max-memory", "64MiB"], "--wasm-max-memory and --wasm-stack-size are only valid with build or test"],
    [["build", root, "--wasm-max-memory", "64MiB"], "--wasm-max-memory requires wasm32 or wasm64 object, LLVM IR, or WASM output"],
    [["build", root, "--target", "wasm32", "--emit", "object", "--wasm-stack-size", "2MiB"], "--wasm-stack-size requires WASM output; link object and LLVM IR output with wasm-ld -z stack-size"],
    [["test", root, "--wasm-max-memory", "64MiB"], "--wasm-max-memory and --wasm-stack-size require --target wasm32 or wasm64"],
  ];
  for (const [args, message] of invalid) {
    const result = execute(compiler, args, false);
    assert.notEqual(result.status, 0, args.join(" "));
    assert.ok(result.stderr.includes(`error[E2000]: ${message}`), `${args.join(" ")}\n${result.stderr}`);
  }

  // Only the root manifest's [wasm] applies, after the command line and only where each option applies.
  const packaged = join(directory, "packaged");
  cpSync(resolve("tests/fixtures/wasm_memory"), packaged, { recursive: true });
  mkdirSync(join(packaged, "helper"));
  writeFileSync(join(packaged, "helper", "Tsuzuri.toml"), "[package]\nname = \"helper\"\nversion = \"1.0.0\"\n[wasm]\nmax-memory = \"1GiB\"\n");
  const manifest = (wasm) => writeFileSync(join(packaged, "Tsuzuri.toml"), `[package]\nname = "app"\nversion = "1.0.0"\n[dependencies]\nhelper = { path = "helper" }\n${wasm}`);
  const packagedBuild = (name, ...options) => {
    const output = join(directory, `packaged-${name}.wasm`);
    cli(["build", packaged, "--target", "wasm32", "--no-cache", ...options, "-o", output]);
    return readFileSync(output);
  };
  manifest("[wasm]\nmax-memory = \"64MiB\"\nstack-size = \"2MiB\"\n");
  const fromManifest = packagedBuild("manifest");
  assert.equal(definedMemory(fromManifest).maximum, 1024);
  assert.equal(definedMemory(packagedBuild("override", "--wasm-max-memory", "128MiB")).maximum, 2048);
  cli(["build", packaged, "--target", "wasm32", "--emit", "object", "--no-cache", "-o", join(directory, "packaged.o")]);
  cli(["build", packaged, "--emit", "llvm", "--no-cache", "-o", join(directory, "packaged-native.ll")]);
  assert.match(cli(["test", packaged, "--target", "wasm32"]).stdout, /2 passed; 0 failed; 0 ignored/);
  manifest("");
  assert.deepEqual(packagedBuild("options", "--wasm-max-memory", "64MiB", "--wasm-stack-size", "2MiB"), fromManifest);
  for (const [wasm, code, message] of [
    ["max-memory = \"100000\"", "E2000", "--wasm-max-memory must be a multiple of 64 KiB and at most 4 GiB - 64 KiB on wasm32 (after applying the root package's [wasm])"],
    ["stack-size = \"16MiB\"", "E2000", "--wasm-max-memory must be at least the stack size plus 64 KiB; raise the memory limit or lower --wasm-stack-size (after applying the root package's [wasm])"],
    ["max-memory = \"64MB\"", "E0002", "WASM sizes must be a quoted byte count or a number followed by KiB, MiB, or GiB, such as \"64MiB\""],
  ]) {
    manifest(`[wasm]\n${wasm}\n`);
    const result = execute(compiler, ["build", packaged, "--target", "wasm32", "--no-cache", "-o", join(directory, "invalid.wasm")], false);
    assert.notEqual(result.status, 0, wasm);
    assert.ok(result.stderr.includes(`Tsuzuri.toml:1:1: error[${code}]: ${message}`), `${wasm}\n${result.stderr}`);
  }
  console.log("WASM memory: root manifest [wasm] matches the options, yields to them, skips dependencies and reports invalid sizes");

  // 70,000 UTF-16 units of static data cannot fit beside a 64 KiB stack in 128 KiB.
  const crowded = join(directory, "crowded");
  mkdirSync(crowded);
  writeFileSync(join(crowded, "Main.tz"), `export def size :: i64 -> i64 = \\seed -> seed + "${"a".repeat(70000)}".length\n`);
  const linkFailure = execute(compiler, ["build", crowded, "--target", "wasm32", "-O0", "--no-cache", "--wasm-max-memory", "128KiB", "--wasm-stack-size", "64KiB", "-o", join(directory, "crowded.wasm")], false);
  assert.notEqual(linkFailure.status, 0);
  assert.match(linkFailure.stderr, /error\[E2002\]/);
  assert.ok(linkFailure.stderr.includes("; if the memory limit is too small, raise --wasm-max-memory or lower --wasm-stack-size"), linkFailure.stderr);
  console.log(`WASM memory: ${invalid.length} invalid option uses report E2000 and an undersized link reports E2002 with the memory hint`);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
