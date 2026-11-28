// NC09: the time of one planned transform under four kernel policies, and of the serial and parallel batches.
//
// usage: node benchmarks/run-fft.mjs <compiler> [--runs N] [--numpy]
//
// 1. Kernel policies. The policies are copies of std/Fft.tz as user modules with the choice of radices and of the
//    convolution length of Bluestein's algorithm edited (A is std/Fft.tz unchanged):
//      A  radices 4, 2, 3, 5; the convolution length is the smallest 2^a 3^b 5^c  (what std/Fft.tz does)
//      B  radices 2, 3, 5 (no radix 4)
//      C  radix 2 only and a power-of-two convolution length (the first plan of NC09, "Phase 1a")
//      D  radices 4, 2, 3, 5 and a power-of-two convolution length
//    Each transform is `execute_in_place` of a plan, forward and back, so the time has no allocation and no planning.
// 2. benchmarks/fft/Main.tz: `fft` against `fft_parallel` and `fft2` against `fft2_parallel`.
// 3. With --numpy, the same transforms with numpy.fft (pocketfft, one thread), for context only.
//
// Every program runs N times (default 5) and the fastest time of each line is printed. The script edits the source by
// exact text, so it stops when std/Fft.tz no longer has the lines it replaces.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const compiler = resolve(args.find((arg) => !arg.startsWith("--")) ?? join(root, "target/release/tsuzuri"));
const runs = args.includes("--runs") ? Number(args[args.indexOf("--runs") + 1]) : 5;

function execute(program, programArgs) {
  const result = spawnSync(program, programArgs, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${programArgs.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

const source = readFileSync(join(root, "std/Fft.tz"), "utf8");
const cut = (text, part) => { assert.ok(text.includes(part), `std/Fft.tz no longer contains:\n${part}`); return text.replace(part, ""); };
const radix4 = `    while rest % 4 == 0 do
        factors[count] = 4
        count = count + 1
        rest = rest / 4
`;
const radix35 = `    while rest % 3 == 0 do
        factors[count] = 3
        count = count + 1
        rest = rest / 3
    while rest % 5 == 0 do
        factors[count] = 5
        count = count + 1
        rest = rest / 5
`;
const smoothStart = source.indexOf("private def next_smooth :: own i64 -> i64");
const smoothEnd = source.indexOf("// perm[i] is the slot of input element");
assert.ok(smoothStart >= 0 && smoothEnd > smoothStart, "std/Fft.tz no longer has next_smooth before build_perm");
const powerOfTwo = (text) => text.replace(source.slice(smoothStart, smoothEnd), "private def next_smooth :: own i64 -> i64\nfn next_smooth least = doubled_to least 1\n\n");
const policies = [
  ["FftA", "A radix 4/2/3/5, smooth chirp", source],
  ["FftB", "B radix 2/3/5, smooth chirp", cut(source, radix4)],
  ["FftC", "C radix 2 only, 2^k chirp", powerOfTwo(cut(cut(source, radix4), radix35))],
  ["FftD", "D radix 4/2/3/5, 2^k chirp", powerOfTwo(source)],
];
const lengths = [256, 1024, 4096, 65536, 1000, 3600, 10000, 1031, 10007];
const one = ([module, label]) => `        let forward${module}: Result<${module}.Plan<f64>, ${module}.Error> = ${module}.plan n ${module}.Direction.Direct
        let inverse${module}: Result<${module}.Plan<f64>, ${module}.Error> = ${module}.plan n ${module}.Direction.Inverse
        match (forward${module}, inverse${module}) with
        | (Result.Ok forward, Result.Ok inverse) ->
            let mut data = wave n
            let mut work = new [Complex<f64>](${module}.plan_work_length forward, \\_ -> Complex.of 0.0 0.0)
            let! start = now ()
            for _round in 0i64 ..< rounds do
                let _a = ${module}.execute_in_place forward (ref mut work) (ref mut data)
                let _b = ${module}.execute_in_place inverse (ref mut work) (ref mut data)
            let! stop = now ()
            do! IO.writeln ($"${label} n={n} {(stop - start) / (2 * rounds)} ns/transform")
        | _ -> ()
`;
const kernels = `def wave :: own i64 -> [Complex<f64>]
fn wave n = new [Complex<f64>](n, \\i -> Complex.of (Math.sin ((i * i + 1) as f64)) (Math.cos ((3 * i + 2) as f64)))

def now :: unit -> IO<i64>
fn now _ = IO {
    let! stamp = Time.monotonic_ns ()
    return (match stamp with
        | Result.Ok value -> value
        | Result.Error _ -> 0)
}

def main :: unit -> i32 = \\() ->
    for n in [${lengths.join(", ")}] do
        let rounds = if n <= 2048 then 20000 elif n <= 16384 then 2000 else 100
${policies.map(one).join("")}    0
`;
const numpy = `import time
import numpy as np

def best(f, rounds):
    out = 1e99
    for _ in range(${runs}):
        start = time.perf_counter_ns()
        for _ in range(rounds):
            f()
        out = min(out, (time.perf_counter_ns() - start) / rounds)
    return out

def wave(shape):
    i = np.arange(int(np.prod(shape)))
    return (np.sin(i * i + 1.0) + 1j * np.cos(3.0 * i + 2.0)).reshape(shape)

for n in [${lengths.join(", ")}]:
    x = wave((n,))
    rounds = 20000 if n <= 2048 else 2000 if n <= 16384 else 100
    print(f"numpy fft n={n} {best(lambda: np.fft.fft(x), rounds):.0f} ns/transform (a new array and the call per transform)")
for shape in [(4096, 1024), (64, 65536), (100000, 16), (512, 1000), (200, 10007)]:
    x = wave(shape)
    print(f"numpy fft {list(shape)}: {best(lambda: np.fft.fft(x), 1) / 1000:.0f} us")
for shape in [(1024, 1024), (2048, 2048), (300, 500), (16, 256, 256), (1009, 1013)]:
    x = wave(shape)
    print(f"numpy fft2 {list(shape)}: {best(lambda: np.fft.fft2(x), 1) / 1000:.0f} us")
`;

// The fastest of `runs` runs for every line that ends in a time; the other words of the line are its key.
function fastest(program) {
  const best = new Map();
  for (let run = 0; run < runs; run++) {
    for (const line of execute(program, []).split("\n")) {
      const single = /^(.*) (\d+) ns\/transform$/.exec(line);
      const pair = /^(.*): serial (\d+) us, parallel (\d+) us, same (\w+)$/.exec(line);
      if (single) best.set(single[1], Math.min(best.get(single[1]) ?? Infinity, Number(single[2])));
      else if (pair) {
        const old = best.get(pair[1]) ?? [Infinity, Infinity, true];
        best.set(pair[1], [Math.min(old[0], Number(pair[2])), Math.min(old[1], Number(pair[3])), old[2] && pair[4] === "true"]);
      } else if (line.startsWith("processors")) best.set(line, null);
    }
  }
  for (const [key, value] of best) {
    if (value === null) console.log(key);
    else if (Array.isArray(value)) console.log(`${key}: serial ${value[0]} us, parallel ${value[1]} us, x${(value[0] / value[1]).toFixed(2)}, same ${value[2]}`);
    else console.log(`${key} ${value} ns/transform`);
  }
}

const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-fft-bench-"));
try {
  writeFileSync(join(temporary, "Tsuzuri.toml"), '[package]\nname = "fft-kernels"\nversion = "0.1.0"\nedition = "2028"\n');
  for (const [module, , text] of policies) writeFileSync(join(temporary, `${module}.tz`), text);
  writeFileSync(join(temporary, "Main.tz"), kernels);
  const kernelProgram = join(temporary, "kernels");
  execute(compiler, ["build", temporary, "-O3", "-o", kernelProgram]);
  fastest(kernelProgram);
  const batchProgram = join(temporary, "batches");
  execute(compiler, ["build", join(root, "benchmarks/fft"), "-O3", "-o", batchProgram]);
  fastest(batchProgram);
  if (args.includes("--numpy")) process.stdout.write(execute("python3", ["-c", numpy]));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
