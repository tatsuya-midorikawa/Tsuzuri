import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { cpus, loadavg, platform, release, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

// F09 Phase 3: the measurements behind the cost rule of `Gpu.Auto` (src/runtime/gpu-vulkan.c, TZ_VK_AUTO_*).
//   node benchmarks/run-gpu-vulkan.mjs target/release/tsuzuri [--runs 3] [--json]
// One project per kernel size, run by `tsuzuri bench`: the same strict i32 kernel (`rounds` repetitions of a multiply-add-shift-xor
// step per lane) through `Gpu.map` on the CPU reference, on an explicit `Gpu.Vulkan` device, and on a `Gpu.Auto` device, for several
// lane counts. A call is `Gpu.map device kernel (Gpu.from_array device values)` on a host array that the bench prepared, and its
// result is dropped: the time includes the copy of the input, the upload, the dispatch, the wait, the readback, the allocation of
// the output, and its release, and nothing is resident between calls. Five benches per lane count:
//   reference  an explicit CpuReference device (the compiler knows the backend, so it specializes the loop)
//   vulkan     an explicit Vulkan device
//   auto       a Gpu.Auto device in a fresh process: its first calls run on the CPU until the saving that they would have made
//              pays the first use of the device (open, compile), so over the 12 calls of a bench it can still be on the CPU
//   fallback   a Gpu.Auto device with the Vulkan library disabled: the CPU path that Auto runs when it declines the device
//   copy       the copy of the input alone, which every device shares and which the estimates subtract
// `tsuzuri bench` runs each bench in a process of its own after a warm-up sample, so the numbers are of a warm device and a built
// pipeline; the cold costs (the first open and the first call of a kernel) are timed separately, in `--runs` processes. A second
// program per kernel asks a warm Auto device which backend it chooses in each cell. The Vulkan results must equal the CPU
// reference's (a checksum is compared). The numbers are those of this machine, under the load printed with them, on that day:
// they set heuristics for one kind of device, they are not thresholds, and nothing here passes or fails on speed.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const option = (name, fallback) => Number(process.argv[process.argv.indexOf(name) + 1] || fallback);
const runs = option("--runs", 3);
const json = process.argv.includes("--json");
const rounds = [1, 4, 16, 64, 256];
const counts = [1000, 10000, 100000, 1000000, 4000000];
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-gpu-vulkan-bench-"));
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const execute = (program, args, env = {}) => {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 3600000, maxBuffer: 64 * 1024 * 1024, env: { ...process.env, ...env } });
  if (result.error || result.status !== 0) throw new Error(`${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
};
// The environment of every run: the measured rule, not a threshold of someone's own.
const rule = { TSUZURI_GPU_AUTO_MIN_WORK: "" };

const prelude = steps => `def kernel :: i32 -> i32
fn kernel value = {
    let mut state = value;
${Array.from({ length: steps }, () => "    state = (state * 1664525 + 1013904223) ^ (Bits.ushr state 13);").join("\n")}
    state
}

def make :: i64 -> [i32]
fn make count = Array.init count (\\index -> (index as i32) * 7 + 3)

def checksum :: Gpu.Buffer<i32> -> i32
fn checksum buffer = {
    let values = Gpu.to_array buffer;
    values[0] + values[Array.length (&values) - 1]
}
`;
const step = (name, backend, call) => `def ${name}_step :: ref [i32] -> Gpu.Buffer<i32>
fn ${name}_step values = {
    let device = Result.get (Gpu.request Gpu.${backend});
    ${call}
}
`;
const mapped = "Gpu.map (&device) kernel (Gpu.from_array (&device) values)";
const benchSource = steps => `${prelude(steps)}
${step("reference", "CpuReference", mapped)}
${step("vulkan", "Vulkan", mapped)}
${step("auto", "Auto", mapped)}
${step("copy", "CpuReference", "Gpu.from_array (&device) values")}
${counts.flatMap(count => ["reference", "vulkan", "auto", "fallback", "copy"].map(name => `bench "${name} ${count}" = Bench.with_input (\\_ -> make ${count}) ${name === "fallback" ? "auto" : name}_step`)).join("\n")}
`;
// The warm Auto device's own choice in every cell: one explicit Vulkan call opens the device and builds the pipeline first. The
// checksums of the three devices' results are compared.
const decisionSource = steps => `${prelude(steps)}
def code :: Gpu.Backend -> i32
fn code backend = match backend with
    | Gpu.Vulkan -> 2
    | Gpu.WebGpu -> 1
    | _ -> 0

def main :: unit -> i32 = \\() ->
    let cpu = Result.get (Gpu.request Gpu.CpuReference)
    let vulkan = Result.get (Gpu.request Gpu.Vulkan)
    let auto = Result.get (Gpu.request Gpu.Auto)
    let warm = make 1024
    let _first = Gpu.map (&vulkan) kernel (Gpu.from_array (&vulkan) (&warm))
${counts.map((count, index) => `    let values${index} = make ${count}
    let expected${index} = checksum (Gpu.map (&cpu) kernel (Gpu.from_array (&cpu) (&values${index})))
    let device${index} = checksum (Gpu.map (&vulkan) kernel (Gpu.from_array (&vulkan) (&values${index})))
    let chosen${index} = checksum (Gpu.map (&auto) kernel (Gpu.from_array (&auto) (&values${index})))
    let served${index} = code (Gpu.last_backend ())
    do! IO.write_line ("${count} " + to_string served${index} + " " + to_string (expected${index} == device${index} && expected${index} == chosen${index}))`).join("\n")}
    0
`;
// Cold costs: the first request of the device and the first call of the kernel (shader module and pipeline), 1024 lanes.
const coldSource = steps => `${prelude(steps)}
def main :: unit -> i32 = \\() ->
    let! open_start = Time.monotonic_ns ()
    let vulkan = Result.get (Gpu.request Gpu.Vulkan)
    let! open_end = Time.monotonic_ns ()
    let values = make 1024
    let input = Gpu.from_array (&vulkan) (&values)
    let! first_start = Time.monotonic_ns ()
    let output = Gpu.map (&vulkan) kernel input
    let! first_end = Time.monotonic_ns ()
    let input2 = Gpu.from_array (&vulkan) (&values)
    let! second_start = Time.monotonic_ns ()
    let output2 = Gpu.map (&vulkan) kernel input2
    let! second_end = Time.monotonic_ns ()
    do! IO.write_line (to_string (Result.get open_end - Result.get open_start) + " " + to_string (Result.get first_end - Result.get first_start) + " " + to_string (Result.get second_end - Result.get second_start) + " " + to_string (checksum output + checksum output2))
    0
`;

const report = {
  machine: { platform: `${platform()} ${release()}`, cpu: cpus()[0]?.model, cores: cpus().length, load_before: loadavg().map(value => Number(value.toFixed(2))), compiler },
  cells: [],
  cold: [],
};
const project = (name, source) => {
  const path = join(directory, name);
  mkdirSync(path, { recursive: true });
  writeFileSync(join(path, "Main.tz"), source);
  return path;
};
for (const steps of rounds) {
  const benches = project(`bench-${steps}`, benchSource(steps));
  const decisions = project(`decisions-${steps}`, decisionSource(steps));
  // The weight of the kernel is the last field of its descriptor in the IR.
  const ir = join(decisions, "program.ll");
  execute(compiler, ["build", decisions, "--emit", "llvm", "-o", ir]);
  const descriptors = [...readFileSync(ir, "utf8").matchAll(/ptr @tz\.gpu\.kernel\.\d+\.spirv, i32 \d+, i32 (\d+) \}/g)].map(match => Number(match[1]));
  const weight = descriptors.at(-1);
  const times = {};
  for (const name of ["reference", "vulkan", "auto", "fallback", "copy"]) {
    const env = { ...rule, ...(name === "fallback" ? { TSUZURI_VULKAN_LIBRARY: "" } : {}) };
    const result = execute(compiler, ["bench", benches, "--filter", `${name} `, "--json"], env);
    for (const line of result.stdout.trim().split("\n")) {
      const entry = JSON.parse(line);
      if (entry.type !== "bench") continue;
      if (entry.status !== "passed") throw new Error(`bench ${entry.name}: ${entry.status}`);
      times[entry.name] = entry.median;
    }
  }
  const decisionProgram = join(decisions, "program");
  execute(compiler, ["build", decisions, "-O3", "-o", decisionProgram]);
  const decided = Object.fromEntries(execute(decisionProgram, [], rule).stdout.trim().split("\n").map(line => { const [count, served, equal] = line.split(" "); return [Number(count), { served: Number(served), equal: equal === "true" }]; }));
  const cold = project(`cold-${steps}`, coldSource(steps));
  const coldProgram = join(cold, "program");
  execute(compiler, ["build", cold, "-O3", "-o", coldProgram]);
  const samples = Array.from({ length: runs }, () => execute(coldProgram, [], rule).stdout.trim().split(" ").map(Number));
  report.cold.push({ rounds: steps, weight, open_ms: median(samples.map(sample => sample[0])) / 1e6, first_call_ms: median(samples.map(sample => sample[1])) / 1e6, second_call_ms: median(samples.map(sample => sample[2])) / 1e6 });
  for (const count of counts) {
    if (!decided[count].equal) throw new Error(`rounds ${steps}, ${count} lanes: the device results differ from the CPU reference`);
    const cell = { rounds: steps, weight, lanes: count };
    for (const name of ["reference", "vulkan", "auto", "fallback", "copy"]) cell[`${name}_ms`] = times[`${name} ${count}`];
    cell.served = decided[count].served === 2 ? "vulkan" : "cpu";
    cell.warm_auto_ms = cell.served === "vulkan" ? cell.vulkan_ms : cell.fallback_ms;
    cell.best = cell.vulkan_ms < cell.fallback_ms ? "vulkan" : "cpu";
    cell.regret_ms = Math.max(0, cell.warm_auto_ms - Math.min(cell.vulkan_ms, cell.fallback_ms));
    report.cells.push(cell);
  }
}
report.machine.load_after = loadavg().map(value => Number(value.toFixed(2)));

// Estimates, each from named cells, with the copy of the input (shared by every device) taken out, of the quantities that the
// cost rule of src/runtime/gpu-vulkan.c uses.
const cell = (steps, count) => report.cells.find(entry => entry.rounds === steps && entry.lanes === count);
const own = (entry, name) => (entry[`${name}_ms`] - entry.copy_ms) * 1e6;
const laneBytes = 8; // 4 bytes in and 4 bytes out
const perByte = (own(cell(1, 4000000), "vulkan") - own(cell(1, 1000000), "vulkan")) / (3000000 * laneBytes);
const heavy = cell(256, 4000000);
const perDeviceOp = Math.max(0, (own(heavy, "vulkan") - own(cell(1, 4000000), "vulkan")) / (4000000 * (heavy.weight - cell(1, 4000000).weight)));
const perOp = name => report.cells.filter(entry => entry.lanes >= 100000).map(entry => own(entry, name) / (entry.lanes * entry.weight));
const stats = values => ({ min: Number(Math.min(...values).toFixed(3)), median: Number(median(values).toFixed(3)), max: Number(Math.max(...values).toFixed(3)) });
report.estimates = {
  call_ns: Math.round(own(cell(1, 1000), "vulkan")),
  byte_ns: Number(perByte.toFixed(3)),
  device_ns_per_op: Number(perDeviceOp.toFixed(4)),
  fallback_ns_per_op: stats(perOp("fallback")),
  reference_ns_per_op: stats(perOp("reference")),
  open_ms: Number(median(report.cold.map(entry => entry.open_ms)).toFixed(1)),
  compile_ms: Number(median(report.cold.map(entry => entry.first_call_ms - entry.second_call_ms)).toFixed(1)),
};
const source = readFileSync(resolve(import.meta.dirname, "../src/runtime/gpu-vulkan.c"), "utf8");
report.constants_in_runtime = Object.fromEntries([...source.matchAll(/#define (TZ_VK_AUTO_\w+) ([0-9.]+)/g)].map(match => [match[1], Number(match[2])]));
report.regret_ms_total = Number(report.cells.reduce((total, entry) => total + entry.regret_ms, 0).toFixed(3));

// The CPU cost per operation that the rule should assume: for each candidate, the cells in which the warm rule (the other
// constants as they stand in the runtime) would pick the slower backend, and what that costs against the faster one. The
// candidate with the least total is the suggestion.
const constants = report.constants_in_runtime;
const warm = entry => constants.TZ_VK_AUTO_CALL_NS + entry.lanes * laneBytes * constants.TZ_VK_AUTO_BYTE_NS + entry.lanes * entry.weight * constants.TZ_VK_AUTO_GPU_NS_PER_OP;
report.scan = [];
for (let exponent = -2; exponent <= -0.3; exponent += 0.1) {
  const perOpCost = Number((10 ** exponent).toFixed(3));
  let total = 0;
  let wrong = 0;
  for (const entry of report.cells) {
    const device = warm(entry) * constants.TZ_VK_AUTO_MARGIN < entry.lanes * entry.weight * perOpCost;
    const picked = device ? entry.vulkan_ms : entry.fallback_ms;
    const regret = picked - Math.min(entry.vulkan_ms, entry.fallback_ms);
    if (regret > 0) wrong++;
    total += regret;
  }
  report.scan.push({ cpu_ns_per_op: perOpCost, regret_ms: Number(total.toFixed(3)), cells_wrong: wrong });
}
// The suggestion is the middle (geometric) of the plateau of candidates whose total is within a tenth of a millisecond or five
// percent of the least: one candidate's value is an accident of the cells measured, the plateau is what the data supports.
const least = Math.min(...report.scan.map(entry => entry.regret_ms));
const plateau = report.scan.filter(entry => entry.regret_ms <= least + Math.max(0.1, 0.05 * least));
report.plateau_cpu_ns_per_op = [plateau[0].cpu_ns_per_op, plateau.at(-1).cpu_ns_per_op];
report.suggested_cpu_ns_per_op = Number(Math.sqrt(plateau[0].cpu_ns_per_op * plateau.at(-1).cpu_ns_per_op).toFixed(3));

if (json) {
  console.log(JSON.stringify(report, null, 2));
} else {
  const pad = (value, width) => String(value).padStart(width);
  console.log(`machine: ${report.machine.platform}, ${report.machine.cpu}, ${report.machine.cores} cores`);
  console.log(`load average before ${report.machine.load_before.join(" ")}, after ${report.machine.load_after.join(" ")} (1, 5, 15 minutes); medians in ms, transfers included`);
  console.log("rounds weight    lanes reference  fallback    vulkan      auto      copy  warm Auto  best    regret");
  for (const entry of report.cells) {
    console.log(`${pad(entry.rounds, 6)} ${pad(entry.weight, 6)} ${pad(entry.lanes, 8)} ${pad(entry.reference_ms.toFixed(3), 9)} ${pad(entry.fallback_ms.toFixed(3), 9)} ${pad(entry.vulkan_ms.toFixed(3), 9)} ${pad(entry.auto_ms.toFixed(3), 9)} ${pad(entry.copy_ms.toFixed(3), 9)}  ${entry.served.padEnd(9)}  ${entry.best.padEnd(7)} ${entry.regret_ms.toFixed(3)}`);
  }
  console.log("cold (median of " + runs + " processes): " + report.cold.map(entry => `rounds ${entry.rounds}: open ${entry.open_ms.toFixed(1)} ms, first call ${entry.first_call_ms.toFixed(1)} ms, second ${entry.second_call_ms.toFixed(2)} ms`).join("; "));
  console.log("estimates: " + JSON.stringify(report.estimates));
  console.log("constants in the runtime: " + JSON.stringify(report.constants_in_runtime));
  console.log("least total regret by the CPU cost per operation the rule assumes: " + report.scan.map(entry => `${entry.cpu_ns_per_op}: ${entry.regret_ms} ms (${entry.cells_wrong} cells)`).join("; "));
  console.log(`suggested TZ_VK_AUTO_CPU_NS_PER_OP: ${report.suggested_cpu_ns_per_op} (the plateau of least regret is ${report.plateau_cpu_ns_per_op.join(" to ")})`);
  console.log(`regret of the warm Gpu.Auto against the better of fallback and vulkan in every cell: ${report.regret_ms_total} ms in total`);
}
