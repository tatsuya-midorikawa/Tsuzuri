import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { storageBridges } from "./strings_bridges.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const scratch = join(root, "target", `utf-simd-${process.pid}`);
mkdirSync(scratch, { recursive: true });
const env = { ...process.env, TMPDIR: scratch };
function run(program, args) {
  const r = spawnSync(program, args, { cwd: root, env, encoding: "utf8", timeout: 600_000, maxBuffer: 256 * 1024 * 1024 });
  assert.equal(r.status, 0, `${program} ${args.join(" ")}\n${r.error ?? ""}\n${r.stdout}\n${r.stderr}`);
  return r.stdout;
}
const read = (file) => readFileSync(join(root, "src/runtime", file), "utf8");
let cpu = read("cpu.c");
for (const bits of [8, 16]) {
  const width = bits === 8 ? "BYTES" : "lanes";
  const pattern = new RegExp(`(#define TZ_CPU_UTF${bits}\\([\\s\\S]*?)(?=\\n#define)`);
  const original = cpu.match(pattern)[1];
  const instrumented = original
    .replace("uint64_t i = 0, count = 0; \\\n",
      bits === 8 ? "uint64_t i = 0, count = validate ? (UINT64_C(1) << 63) : 0; \\\n" :
        "uint64_t i = 0, count = 0; \\\n")
    .replace(`if (length - i >= ${width}) { \\\n`,
      `if (length - i >= ${width}) { \\\n ++test_cpu${bits}_probe_count; test_cpu${bits}_window = ${width}; \\\n`)
    .replace("            break; \\\n", `            ++test_cpu${bits}_scalar_count; \\\n            break; \\\n`);
  assert.notEqual(original, instrumented);
  cpu = cpu.replace(pattern, instrumented);
}
let cpuCounters = "";
for (const bits of [8, 16]) {
  cpuCounters += `static uint64_t test_cpu${bits}_probe_count, test_cpu${bits}_scalar_count, test_cpu${bits}_window;
uint64_t test_cpu${bits}_probes(void) { return test_cpu${bits}_probe_count; }
uint64_t test_cpu${bits}_starts(void) { return test_cpu${bits}_scalar_count; }
uint64_t test_cpu${bits}_width(void) { return test_cpu${bits}_window; }
`;
}
cpu = cpu.replace("#define TZ_CPU_UTF8(", cpuCounters + "#define TZ_CPU_UTF8(");
const cpuSource = join(scratch, "cpu-instrumented.c");
writeFileSync(cpuSource, cpu);
const scalar = read("string_scalar.ll");
const names = [...scalar.matchAll(/^define internal .* @([\w.]+)\(/gm)].map((m) => m[1]);
let reference = scalar;
for (const name of names) reference = reference.replaceAll(`@${name}(`, `@ref.${name.slice(3)}(`);
// PM03: the Latin-1 kernels of the compact string form have the same shape: a scalar reference and, where the
// vector runtime is selected, a generated vector file. The generator must reproduce both files exactly.
const latin1Scalar = read("string_latin1.ll");
const latin1Names = [...latin1Scalar.matchAll(/^define internal .* @([\w.]+)\(/gm)].map((m) => m[1]);
let latin1Reference = latin1Scalar;
for (const name of latin1Names) latin1Reference = latin1Reference.replaceAll(`@${name}(`, `@ref.${name.slice(3)}(`);
const generated = join(scratch, "generated");
mkdirSync(generated, { recursive: true });
run(process.env.TSUZURI_PYTHON ?? "python3", [join(root, "src/runtime/generate_utf.py"), generated]);
for (const file of ["string_v128.ll", "string_latin1_v128.ll"]) {
  const text = readFileSync(join(generated, file), "utf8");
  assert.ok(!text.includes("\r"), `${file} is written with LF on every host`);
  assert.equal(text, read(file), `${file} is what generate_utf.py writes`);
}
const latin1Bridges = (prefix, namespace) => `
define i32 @${prefix}_latin1_units(ptr %p, i64 %n) {
  %r = call i1 @${namespace}.string.is_latin1_units(ptr %p, i64 %n)
  %z = zext i1 %r to i32
  ret i32 %z
}
define void @${prefix}_latin1_widen(ptr %to, ptr %from, i64 %n) {
  call void @${namespace}.string.widen(ptr %to, ptr %from, i64 %n)
  ret void
}
define void @${prefix}_latin1_widen_in_place(ptr %p, i64 %n) {
  call void @${namespace}.string.widen_in_place(ptr %p, i64 %n)
  ret void
}
define void @${prefix}_latin1_narrow_in_place(ptr %p, i64 %n) {
  call void @${namespace}.string.narrow_in_place(ptr %p, i64 %n)
  ret void
}
define i32 @${prefix}_latin1_equal_mixed(ptr %bytes, ptr %units, i64 %n) {
  %r = call i1 @${namespace}.string.equal_mixed(ptr %bytes, ptr %units, i64 %n)
  %z = zext i1 %r to i32
  ret i32 %z
}
define i32 @${prefix}_latin1_compare_mixed(ptr %bytes, i64 %bn, ptr %units, i64 %un) {
  %r = call i32 @${namespace}.string.compare_mixed(ptr %bytes, i64 %bn, ptr %units, i64 %un)
  ret i32 %r
}
define i64 @${prefix}_latin1_utf8(ptr %p, i64 %n, ptr %out) {
  %r = call %tz.utf8string @${namespace}.utf8string.from_latin1(ptr %p, i64 %n)
  %d = extractvalue %tz.utf8string %r, 0
  %m = extractvalue %tz.utf8string %r, 1
  store ptr %d, ptr %out
  ret i64 %m
}
define i32 @${prefix}_latin1_to_ascii(ptr %to, ptr %from, i64 %n) {
  %r = call i1 @${namespace}.string.to_ascii_latin1(ptr %to, ptr %from, i64 %n)
  %z = zext i1 %r to i32
  ret i32 %z
}
define i64 @${prefix}_latin1_scan(ptr %p, i64 %n, ptr %max) {
  %r = call { i64, i32 } @${namespace}.string.utf8_scan(ptr %p, i64 %n)
  %u = extractvalue { i64, i32 } %r, 0
  %m = extractvalue { i64, i32 } %r, 1
  store i32 %m, ptr %max
  ret i64 %u
}
define void @${prefix}_latin1_decode(ptr %p, i64 %n, ptr %data) {
  call void @${namespace}.string.utf8_to_latin1(ptr %p, i64 %n, ptr %data)
  ret void
}
`;
const bridges = (prefix, namespace) => {
  let ir = "";
  ir += `define i32 @${prefix}_valid8(ptr %p, i64 %n) {\n %value = call i1 @${namespace}.string.validate_utf8(ptr %p, i64 %n)\n %result = zext i1 %value to i32\n ret i32 %result\n}\n`;
  for (const [name, implementation] of [["length8", "string.utf8_to_utf16_length"], ["length16", "utf8string.utf16_to_utf8_length"]]) {
    ir += `define i64 @${prefix}_${name}(ptr %p, i64 %n) {\n %result = call i64 @${namespace}.${implementation}(ptr %p, i64 %n)\n ret i64 %result\n}\n`;
  }
  for (const [name, type, implementation] of [["decode", "string", "string.from_utf8"], ["encode", "utf8string", "utf8string.from_string"], ["repair", "string", "string.to_well_formed"]]) {
    ir += `define i64 @${prefix}_${name}(ptr %p, i64 %n, ptr %out) {\n %value = call %tz.${type} @${namespace}.${implementation}(ptr %p, i64 %n)\n %data = extractvalue %tz.${type} %value, 0\n %size = extractvalue %tz.${type} %value, 1\n store ptr %data, ptr %out\n ret i64 %size\n}\n`;
  }
  ir += `define i32 @${prefix}_well(ptr %p, i64 %n) {\n %value = call i1 @${namespace}.string.is_well_formed(ptr %p, i64 %n)\n %result = zext i1 %value to i32\n ret i32 %result\n}\n`;
  for (const [bits, type] of [[8, "utf8string"], [16, "string"]]) {
    for (const operation of ["equal", "compare"]) {
      const result = operation === "equal" ? "i1" : "i32";
      ir += `define i32 @${prefix}_${operation}${bits}(ptr %a, i64 %an, ptr %b, i64 %bn) {\n %a0 = insertvalue %tz.${type} zeroinitializer, ptr %a, 0\n %a1 = insertvalue %tz.${type} %a0, i64 %an, 1\n %b0 = insertvalue %tz.${type} zeroinitializer, ptr %b, 0\n %b1 = insertvalue %tz.${type} %b0, i64 %bn, 1\n %value = call ${result} @${namespace}.${type}.${operation}(%tz.${type} %a1, %tz.${type} %b1)\n`;
      ir += operation === "equal" ? " %result = zext i1 %value to i32\n ret i32 %result\n}\n" : " ret i32 %value\n}\n";
    }
  }
  return ir;
};
try {
  for (const wasm of [false, true]) for (const simd of wasm ? [false, true] : [true]) {
    let selected = simd ? read("string_v128.ll") : scalar;
    selected = selected.replaceAll("@tz.string.decode_utf16(", "@test.decode_utf16(");
    selected = selected.replace(/^scalar_start:\n/gm, `scalar_start:
  %test_starts = load i64, ptr @test.scalar16_starts
  %test_next = add i64 %test_starts, 1
  store i64 %test_next, ptr @test.scalar16_starts
`);
    for (const [label, counter] of [["block_read", "utf_probes"], ["block_scalar_start", "utf_starts"]]) {
      selected = selected.replaceAll(`${label}:\n`, `${label}:
  %test_${counter} = load i64, ptr @test.${counter}
  %test_${counter}_next = add i64 %test_${counter}, 1
  store i64 %test_${counter}_next, ptr @test.${counter}
`);
    }
    if (!wasm) {
      const cpuNames = ["tz.string.validate_utf8", "tz.string.from_utf8"];
      if (process.arch === "x64" || process.arch === "ia32") cpuNames.push("tz.utf8string.from_string");
      for (const name of cpuNames)
        selected = selected.replaceAll(`@${name}(ptr`, `@${name}.portable(ptr`);
      selected += read("string_cpu.ll");
      if (cpuNames.includes("tz.utf8string.from_string")) selected += read("string_cpu_utf16.ll");
      let cpu16Test = read("string_cpu_utf16.ll").replace(
        "@tz.utf8string.from_string(ptr", "@tz.utf8string.from_string.cpu_test(ptr");
      if (cpuNames.includes("tz.utf8string.from_string"))
        cpu16Test = cpu16Test.replace("declare i64 @tsuzuri_cpu_utf16_to_utf8(ptr, i64, ptr)\n", "");
      else cpu16Test = cpu16Test.replaceAll("@tz.utf8string.from_string.portable(", "@tz.utf8string.from_string(");
      selected += cpu16Test;
    }
    let ir = "%tz.string = type { ptr, i64 }\n%tz.utf8string = type { ptr, i64 }\ndeclare void @llvm.trap()\n" +
      read("string.ll") + read("utf8string.ll") + selected +
      reference + bridges("v", "tz") + bridges("r", "ref") +
      read(simd ? "string_latin1_v128.ll" : "string_latin1.ll") + latin1Reference +
      latin1Bridges("v", "tz") + latin1Bridges("r", "ref") +
      "define void @test_drop(ptr %p) {\n call void @tz.free(ptr %p)\n ret void\n}\n" +
      read(wasm ? "heap-wasm.ll" : "heap-native.ll") + storageBridges();
    for (const [operation, type, implementation] of [["decode", "string", "string.from_utf8"], ["encode", "utf8string", "utf8string.from_string"]]) {
      ir += `define i64 @p_${operation}(ptr %p, i64 %n, ptr %out) {
  %value = call %tz.${type} @tz.${implementation}${!wasm && (operation === "decode" || process.arch === "x64" || process.arch === "ia32") ? ".portable" : ""}(ptr %p, i64 %n)
  %data = extractvalue %tz.${type} %value, 0
  %size = extractvalue %tz.${type} %value, 1
  store ptr %data, ptr %out
  ret i64 %size
}
`;
    }
    ir += `define i32 @p_valid8(ptr %p, i64 %n) {
  %value = call i1 @tz.string.validate_utf8${wasm ? "" : ".portable"}(ptr %p, i64 %n)
  %result = zext i1 %value to i32
  ret i32 %result
}
`;
    for (const counter of ["utf_probes", "utf_starts"]) {
      ir += `@test.${counter} = internal global i64 0
define i64 @test_${counter}() {
  %value = load i64, ptr @test.${counter}
  ret i64 %value
}
`;
    }
    if (!wasm) ir += `define i64 @c_encode(ptr %p, i64 %n, ptr %out) {
  %value = call %tz.utf8string @tz.utf8string.from_string.cpu_test(ptr %p, i64 %n)
  %data = extractvalue %tz.utf8string %value, 0
  %size = extractvalue %tz.utf8string %value, 1
  store ptr %data, ptr %out
  ret i64 %size
}
`;
    for (const operation of ["decode", "encode", "repair"]) {
      ir += `define i64 @test_${operation}(ptr %p, i64 %n, ptr %out) {\n %result = call i64 @v_${operation}(ptr %p, i64 %n, ptr %out)\n ret i64 %result\n}\n`;
    }
    ir += `
define i32 @test_well_formed(ptr %p, i64 %n) {
  %result = call i32 @v_well(ptr %p, i64 %n)
  ret i32 %result
}
define void @test_allocate(i64 %n) {
  %result = call %tz.string @tz.string.allocate(i64 %n)
  ret void
}
define void @test_concat(i64 %an, i64 %bn) {
  %a = insertvalue %tz.string zeroinitializer, i64 %an, 1
  %b = insertvalue %tz.string zeroinitializer, i64 %bn, 1
  %result = call %tz.string @tz.string.concat(%tz.string %a, %tz.string %b)
  ret void
}
`;
    ir = ir.replace("define internal ptr @tz.alloc(", "define internal ptr @test.base_alloc(");
    ir += `
@test.scalar16_starts = internal global i64 0
define i64 @test_scalar16_starts() {
  %value = load i64, ptr @test.scalar16_starts
  ret i64 %value
}
@test.decode16_calls = internal global i64 0
define internal { i32, i64 } @test.decode_utf16(ptr %source, i64 %length, i64 %index, i1 %validated) nounwind {
  %calls = load i64, ptr @test.decode16_calls
  %next = add i64 %calls, 1
  store i64 %next, ptr @test.decode16_calls
  %result = call { i32, i64 } @tz.string.decode_utf16(ptr %source, i64 %length, i64 %index, i1 %validated)
  ret { i32, i64 } %result
}
define i64 @test_decodes16() {
  %value = load i64, ptr @test.decode16_calls
  ret i64 %value
}
@test.alloc_calls = internal global i64 0
@test.alloc_bytes = internal global i64 0
define internal ptr @tz.alloc(i64 %size) {
  %calls = load i64, ptr @test.alloc_calls
  %next = add i64 %calls, 1
  store i64 %next, ptr @test.alloc_calls
  %bytes = load i64, ptr @test.alloc_bytes
  %total = add i64 %bytes, %size
  store i64 %total, ptr @test.alloc_bytes
  %p = call ptr @test.base_alloc(i64 %size)
  ret ptr %p
}
define i64 @test_calls() {
  %value = load i64, ptr @test.alloc_calls
  ret i64 %value
}
define i64 @test_bytes() {
  %value = load i64, ptr @test.alloc_bytes
  ret i64 %value
}
`;
    if (!wasm) ir = ir.replaceAll("@malloc(", "@tracked_alloc(").replaceAll("@free(", "@tracked_free(")
      .replaceAll("@realloc(", "@tracked_realloc(");
    const source = join(scratch, `${wasm ? "wasm" : "native"}-${simd}.ll`);
    writeFileSync(source, ir);
    for (const opt of [0, 3]) {
      const output = join(scratch, `${wasm ? "wasm" : "native"}-${simd}-O${opt}${wasm ? ".wasm" : process.platform === "win32" ? ".exe" : ""}`);
      const testFlags = [...(process.argv.includes("--quick") ? ["-DUTF_QUICK"] : []),
        ...(simd ? ["-DUTF_VECTOR"] : [])];
      const flags = [`-O${opt}`, ...testFlags,
        "-Wno-override-module", join(root, "tests/strings_simd.c"), source, "-o", output];
      if (wasm) flags.unshift("--target=wasm32-unknown-unknown", simd ? "-msimd128" : "-mno-simd128",
        "-mbulk-memory", "-ffreestanding", "-fno-builtin", "-nostdlib", "-Wl,--no-entry", "-Wl,--export-memory",
        "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216",
        ...["differential", "exhaustive", "guard_pages", "utf8_input", "utf16_input", "result_slot",
          "v_length8", "v_length16", "v_well", "v_repair", "v_encode", "test_drop", "storage_cases",
          "all_scalars", "runtime_trap_case", "runtime_trap_count", "storage_trap_case"].map((s) => `-Wl,--export=${s}`));
      else flags.push(cpuSource);
      run(clang, flags);
      if (wasm) {
        const module = new WebAssembly.Module(readFileSync(output));
        assert.deepEqual(WebAssembly.Module.imports(module), []);
        const api = new WebAssembly.Instance(module).exports;
        assert.equal(api.differential(), 0, `WASM simd=${simd} O${opt}`);
        assert.equal(api.storage_cases(), 0);
        assert.equal(api.all_scalars(), 4344);
        assert.equal(api.runtime_trap_count(), 28);
        for (const [name, count] of [["runtime_trap_case", 28], ["storage_trap_case", 6]])
          for (let i = 0; i < count; ++i) {
            const isolated = new WebAssembly.Instance(module).exports;
            assert.throws(() => isolated[name](i),
              (e) => e instanceof WebAssembly.RuntimeError && /unreachable/.test(e.message));
          }
        assert.equal(api.guard_pages(), 0);
        const p = api.utf8_input();
        const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
        for (const bytes of [[0xef,0xbb,0xbf], [0xf4,0x8f,0xbf,0xbf], [0xed,0xa0,0x80], [0xe0,0x80,0x80], [0xf4,0x90,0x80,0x80]]) {
          new Uint8Array(api.memory.buffer, p, bytes.length).set(bytes);
          let expected = -1n;
          try { expected = BigInt(decoder.decode(new Uint8Array(bytes)).length); } catch {}
          assert.equal(api.v_length8(p, BigInt(bytes.length)), expected);
        }
        if (!process.argv.includes("--quick")) {
          const tails = [0,0x7f,0x80,0x8f,0x90,0x9f,0xa0,0xbf,0xc0,0xff];
          const frame = new Uint8Array(48);
          const check = (bytes) => {
            new Uint8Array(api.memory.buffer, p, bytes.length).set(bytes);
            let expected = -1n;
            try { expected = BigInt(decoder.decode(bytes).length); } catch {}
            assert.equal(api.v_length8(p, BigInt(bytes.length)), expected);
          };
          for (let n = 1; n <= 4; ++n) for (let lead = 0; lead < 256; ++lead)
            for (let a = 0; a < (n > 1 ? 10 : 1); ++a)
              for (let b = 0; b < (n > 2 ? 10 : 1); ++b)
                for (let c = 0; c < (n > 3 ? 10 : 1); ++c) {
                  const sequence = new Uint8Array([lead,tails[a],tails[b],tails[c]]).subarray(0,n);
                  check(sequence);
                  for (const at of [0,13,15,16]) {
                    frame.fill(65); frame.set(sequence,at); check(frame);
                  }
                }
        }
        const u = api.utf16_input(), slot = api.result_slot();
        const encoder = new TextEncoder();
        const check16 = (values) => {
          new Uint16Array(api.memory.buffer, u, values.length).set(values);
          const text = String.fromCharCode(...values);
          assert.equal(api.v_well(u, BigInt(values.length)), Number(text.isWellFormed()));
          const repairedLength = Number(api.v_repair(u, BigInt(values.length), slot));
          const repairedPointer = new DataView(api.memory.buffer).getUint32(slot,true);
          const repaired = String.fromCharCode(...new Uint16Array(api.memory.buffer,repairedPointer,repairedLength));
          assert.equal(repaired,text.toWellFormed());
          api.test_drop(repairedPointer);
          if (text.isWellFormed()) {
            const encodedLength = Number(api.v_encode(u, BigInt(values.length), slot));
            const encodedPointer = new DataView(api.memory.buffer).getUint32(slot,true);
            assert.deepEqual(new Uint8Array(api.memory.buffer,encodedPointer,encodedLength),encoder.encode(text));
            api.test_drop(encodedPointer);
          } else assert.equal(api.v_length16(u, BigInt(values.length)),-1n);
        };
        for (const surrogate of process.argv.includes("--quick") ? [0xd800,0xdbff,0xdc00,0xdfff] : Array.from({ length:2048 },(_,i)=>0xd800+i))
          for (let at = 0; at < 32; ++at) {
            const values = new Array(40).fill(65); values[at] = surrogate; check16(values);
          }
        for (const at of [7,15,23]) {
          const values = new Array(40).fill(65); values[at] = 0xd83d; values[at+1] = 0xde00; check16(values);
        }
        if (process.argv.includes("--exhaustive") && simd && opt === 3) assert.equal(api.exhaustive(), 0);
        const objdump = process.env.TSUZURI_OBJDUMP ?? join(dirname(clang), "llvm-objdump");
        const assembly = run(objdump, ["-d", output]);
        assert.equal(/\b(?:v128|i8x16|i16x8|i32x4|i64x2|f32x4|f64x2)\./.test(assembly), simd);
      } else {
        run(output, []);
        for (const [prefix, count] of [["t", 28], ["s", 6]]) for (let i = 0; i < count; ++i) {
          const trapped = spawnSync(output, [`${prefix}${i}`], { env, encoding: "utf8" });
          assert.ok(["SIGILL", "SIGTRAP"].includes(trapped.signal) || (trapped.status >>> 0) === 0xc000001d,
            `native simd=${simd} O${opt} ${prefix}${i}: status=${trapped.status} signal=${trapped.signal} ${trapped.stderr}`);
        }
        const forced = spawnSync(output, [], { env: { ...env, TSUZURI_CPU_FORCE: "baseline" }, encoding: "utf8", timeout: 600_000 });
        assert.equal(forced.status, 0, forced.stderr);
        const features = BigInt(run(output, ["--features"]).trim());
        for (const [variant, bit] of [["sse4.2", 0n], ["avx2", 1n], ["avx512", 2n], ["sve", 16n], ["sve2", 17n], ["unknown", 63n]]) {
          const selected = spawnSync(output, ["--guard-only"], {
            env: { ...env, TSUZURI_CPU_FORCE: variant }, encoding: "utf8", timeout: 600_000,
          });
          if ((features & (1n << bit)) !== 0n) assert.equal(selected.status, 0, selected.stderr);
          else {
            assert.notEqual(selected.status, 0);
            assert.match(selected.stderr, /requested variant is unavailable or unknown/);
          }
        }
        if (process.argv.includes("--exhaustive") && opt === 3) run(output, ["--exhaustive"]);
        if (process.argv.includes("--asan")) {
          const asan = `${output}-asan`;
          // LLVM IR needs explicit sanitizer attributes; sanitizer flags alone only instrument C.
          const asanSource = `${source}-asan.ll`;
          writeFileSync(asanSource, ir.replace(/ nounwind((?: alwaysinline| noinline)*) \{/g,
            (_, inline = "") => ` nounwind${inline} sanitize_address {`));
          run(process.env.TSUZURI_ASAN_CLANG ?? clang, [`-O${opt}`, ...testFlags,
            "-fsanitize=address", "-fno-omit-frame-pointer", "-Wno-override-module",
            join(root, "tests/strings_simd.c"), asanSource, cpuSource, "-o", asan]);
          run(asan, []);
          run(asan, ["--guard-only"]);
        }
      }
      console.log(`UTF differential/guard ${wasm ? (simd ? "wasm simd128" : "wasm scalar") : "native + CPU baseline/auto"} O${opt}: passed`);
    }
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
