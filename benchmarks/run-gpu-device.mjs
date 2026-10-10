import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { cpus, loadavg, platform, release, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { createGpuImports } from "../src/runtime/webgpu.mjs";

// F09 Phase 2: the same Tsuzuri program on the CPU reference and on a WebGPU device, transfers included.
//   node benchmarks/run-gpu-device.mjs target/release/tsuzuri [--runs 9] [--json]
// Each call is `Gpu.map_relaxed device kernel (Gpu.from_array device values)` followed by `Gpu.to_array`: the host array
// is copied, uploaded, mapped, read back, and copied out. `light` is a cubic polynomial (3 multiplications and 3
// additions per lane), `heavy` is 64 chained multiply-adds per lane. Native needs wgpu-native 29 (TSUZURI_WEBGPU_LIBRARY
// names it); WASM needs Node.js 24 or newer (JSPI) and the `webgpu` binding (TSUZURI_WEBGPU_MODULE). The numbers are
// those of this machine on this day; they are not thresholds, and the CPU reference is one scalar thread.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const runsAt = process.argv.indexOf("--runs");
const runs = runsAt < 0 ? 9 : Number(process.argv[runsAt + 1] || 9);
const json = process.argv.includes("--json");
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-device-bench-"));
// Calls per measurement: the CPU reference takes microseconds at the small sizes, so it gets more calls.
const sizes = [
  { count: 1024, light: [20000, 50], heavy: [2000, 50] },
  { count: 16384, light: [2000, 30], heavy: [200, 30] },
  { count: 262144, light: [100, 10], heavy: [20, 10] },
  { count: 4194304, light: [10, 3], heavy: [3, 3] },
];
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const run = (program, args, env = {}) => {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 900000, maxBuffer: 8 * 1024 * 1024, env: { ...process.env, ...env } });
  if (result.error || result.status !== 0) throw new Error(`${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
};
const heavy = Array.from({ length: 64 }, (_, step) => `    let v${step} = ${step ? `v${step - 1}` : "value"} * 1.0001 + 0.0001;`).join("\n");
const prelude = (count, kernel) => `def light :: f32 -> f32
fn light value = ((value * 0.5 + 0.25) * value + 0.125) * value + 1.0

def heavy :: f32 -> f32
fn heavy value = {
${heavy}
    v63
}

def sample :: i32 -> f32
fn sample index = (index as f32) * 0.015625 + 0.5

def rounds :: ref Gpu.Device -> ref [f32] -> i64 -> f32
fn rounds device values iterations = {
    let mut total = 0.0f32;
    let mut round = 0i64;
    while round < iterations do {
        let result = Gpu.to_array (Gpu.map_relaxed device ${kernel} (Gpu.from_array device values));
        total = total + result[round % ${count}i64];
        round = round + 1
    };
    total
}
`;
const nativeSource = (count, kernel, [cpuRounds, gpuRounds]) => `${prelude(count, kernel)}
def main :: unit -> i32 = \\() ->
    let reference = Result.get (Gpu.request Gpu.CpuReference)
    let values = Gpu.to_array (Gpu.init_relaxed (&reference) ${count} sample)
    let! cpu_start = Time.monotonic_ns ()
    let cpu_total = rounds (&reference) (&values) ${cpuRounds}
    let! cpu_end = Time.monotonic_ns ()
    let! open_start = Time.monotonic_ns ()
    let device = Result.get (Gpu.request Gpu.WebGpu)
    let! open_end = Time.monotonic_ns ()
    let first_total = rounds (&device) (&values) 1
    let! first_end = Time.monotonic_ns ()
    let gpu_total = rounds (&device) (&values) ${gpuRounds}
    let! gpu_end = Time.monotonic_ns ()
    let cpu_check = rounds (&reference) (&values) 3
    let gpu_check = rounds (&device) (&values) 3
    do! IO.write_line (Result.get cpu_end - Result.get cpu_start)
    do! IO.write_line (Result.get open_end - Result.get open_start)
    do! IO.write_line (Result.get first_end - Result.get open_end)
    do! IO.write_line (Result.get gpu_end - Result.get first_end)
    do! IO.write_line (if Math.abs (cpu_check - gpu_check) <= 0.0001 * Math.abs cpu_check && cpu_total > 0.0 && gpu_total > 0.0 && first_total > 0.0 then 1 else 0)
    0
`;
const wasmSource = (count, kernel) => `${prelude(count, kernel)}
export def wasm_cpu :: i32 -> f32
fn wasm_cpu iterations = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let values = Gpu.to_array (Gpu.init_relaxed (&reference) ${count} sample);
    rounds (&reference) (&values) (iterations as i64)
}

export def wasm_open :: bool
fn wasm_open = Result.is_ok (&(Gpu.request Gpu.WebGpu))

export def wasm_gpu :: i32 -> f32
fn wasm_gpu iterations = {
    let reference = Result.get (Gpu.request Gpu.CpuReference);
    let values = Gpu.to_array (Gpu.init_relaxed (&reference) ${count} sample);
    let device = Result.get (Gpu.request Gpu.WebGpu);
    rounds (&device) (&values) (iterations as i64)
}
`;
const rows = [];
const summarize = (target, kernel, count, [cpuRounds, gpuRounds], samples) => ({
  target, kernel, lanes: count, cpu_calls: cpuRounds, gpu_calls: gpuRounds, runs,
  cpu_us_per_call: median(samples.map(s => s.cpu)), gpu_us_per_call: median(samples.map(s => s.gpu)),
  first_call_ms: median(samples.map(s => s.first)), open_ms: median(samples.map(s => s.open)),
  gpu_us_per_call_range: [Math.min(...samples.map(s => s.gpu)), Math.max(...samples.map(s => s.gpu))],
});
let adapter = "";
try {
  const library = process.env.TSUZURI_WEBGPU_LIBRARY;
  for (const kernel of ["light", "heavy"]) {
    for (const size of sizes) {
      const project = join(directory, `native-${kernel}-${size.count}`);
      mkdirSync(project);
      writeFileSync(join(project, "Main.tz"), nativeSource(size.count, kernel, size[kernel]));
      const program = join(project, `program${platform() === "win32" ? ".exe" : ""}`);
      run(compiler, ["build", project, "-O3", "-o", program]);
      const samples = [];
      for (let index = 0; index < runs; index++) {
        const [cpu, open, first, gpu, agreed] = run(program, [], library === undefined ? {} : { TSUZURI_WEBGPU_LIBRARY: library }).stdout.trim().split("\n").map(Number);
        if (agreed !== 1) throw new Error(`native ${kernel} ${size.count}: the device result disagrees with the CPU reference`);
        samples.push({ cpu: cpu / size[kernel][0] / 1e3, open: open / 1e6, first: first / 1e6, gpu: gpu / size[kernel][1] / 1e3 });
      }
      rows.push(summarize("native (wgpu-native)", kernel, size.count, size[kernel], samples));
    }
  }
  if (typeof WebAssembly.Suspending === "function") {
    const binding = await import(process.env.TSUZURI_WEBGPU_MODULE ?? pathToFileURL(resolve("target/webgpu-runtime/node_modules/webgpu/index.js")).href);
    Object.assign(globalThis, binding.globals);
    const create = () => binding.create(platform() === "darwin" ? ["backend=metal"] : []);
    const info = (await create().requestAdapter()).info;
    adapter = `${info.vendor} ${info.architecture} ${info.description}`.trim();
    for (const kernel of ["light", "heavy"]) {
      for (const size of sizes) {
        const project = join(directory, `wasm-${kernel}-${size.count}`);
        mkdirSync(project);
        writeFileSync(join(project, "Main.tz"), wasmSource(size.count, kernel));
        const wasm = join(project, "program.wasm");
        run(compiler, ["build", project, "--target", "wasm32", "--wasm-feature", "webgpu", "--wasm-max-memory", "256MiB", "-O3", "-o", wasm]);
        const module = new WebAssembly.Module(readFileSync(wasm));
        const [cpuRounds, gpuRounds] = size[kernel];
        const samples = [];
        for (let index = 0; index < runs; index++) {
          const host = createGpuImports(create(), () => instance.exports.memory);
          const instance = new WebAssembly.Instance(module, { tsuzuri_gpu: host.imports });
          const call = name => WebAssembly.promising(instance.exports[name]);
          const time = async action => { const start = performance.now(); const value = await action(); return [performance.now() - start, value]; };
          // The calls with 0 rounds build the input only; their time is subtracted.
          const [cpuBuild] = await time(() => call("tz_wasm_cpu")(0));
          const [cpuTotal] = await time(() => call("tz_wasm_cpu")(cpuRounds));
          const [open, opened] = await time(() => call("tz_wasm_open")());
          if (opened !== 1) throw new Error("the WebGPU device did not open");
          const [gpuBuild] = await time(() => call("tz_wasm_gpu")(0));
          const [firstTotal] = await time(() => call("tz_wasm_gpu")(1));
          const [gpuTotal] = await time(() => call("tz_wasm_gpu")(gpuRounds));
          samples.push({ cpu: (cpuTotal - cpuBuild) / cpuRounds * 1e3, open, first: firstTotal - gpuBuild, gpu: (gpuTotal - gpuBuild) / gpuRounds * 1e3 });
          await host.close();
        }
        rows.push(summarize("WASM (Dawn, JSPI)", kernel, size.count, size[kernel], samples));
      }
    }
  }
  const environment = { os: `${platform()} ${release()}`, cpu: cpus()[0]?.model, load_average_1m_at_end: loadavg()[0], node: process.version, adapter, library: library ?? "default search path", compiler_options: "-O3; WASM: wasm32 --wasm-feature webgpu --wasm-max-memory 256MiB" };
  if (json) console.log(JSON.stringify({ environment, rows }, null, 2));
  else {
    console.log(`environment: ${JSON.stringify(environment)}`);
    console.log("| target | kernel | lanes | CPU reference µs/call | WebGPU µs/call (min–max) | first call ms | device open ms |\n| --- | --- | ---: | ---: | ---: | ---: | ---: |");
    for (const row of rows) console.log(`| ${row.target} | ${row.kernel} | ${row.lanes} | ${row.cpu_us_per_call.toFixed(1)} | ${row.gpu_us_per_call.toFixed(1)} (${row.gpu_us_per_call_range.map(value => value.toFixed(1)).join("–")}) | ${row.first_call_ms.toFixed(1)} | ${row.open_ms.toFixed(1)} |`);
  }
} finally {
  rmSync(directory, { recursive: true, force: true });
}
