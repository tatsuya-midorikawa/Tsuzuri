import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PerformanceObserver, performance } from "node:perf_hooks";
import { availableParallelism } from "node:os";
import { Worker } from "node:worker_threads";

const wrap = (value) => BigInt.asUintN(64, value);
const multiplier = 6364136223846793005n, increment = 1442695040888963407n;
const mix = (value, salt) => wrap((value ^ (value >> 13n)) * multiplier + salt);
const step = (value, salt) => mix(value, wrap(salt + increment));
const factors = [17n, 3n, 29n, 7n, 61n, 11n, 83n, 5n, 47n, 19n, 101n, 31n, 53n, 23n, 97n, 13n];

function textAndMath(name, count, seed) {
  if (name === "format_parse") {
    for (let index = 0; index < count; ++index) {
      const text = step(seed, BigInt(index)).toString();
      seed = BigInt(text) ^ BigInt(text.length);
    }
    return seed;
  }
  if (name === "math_intrinsics") {
    let state = Number((seed & 65535n) + 1n) / 16;
    for (let index = 0; index < count; ++index) {
      state = Math.sqrt(Math.abs(state) + (index & 255));
      state = Math.floor(state * 16) / 16 + Math.ceil(state) / 1024;
    }
    return BigInt(Math.trunc(state * 1048576));
  }
  let text = count === 0 ? "" : (seed & 1n) === 0n ? "Az09-_ \n" : "A\0\u03A9\uD83D\uDE00\u4E2Dz\n";
  while (text.length < count) text = text.slice() + text;
  if (name === "utf16_compare") {
    const left = text.slice() + ((seed & 2n) === 0n ? "a" : "b"), right = text + ((seed & 4n) === 0n ? "a" : "b");
    return left === right ? 1n : left < right ? 2n : 4n;
  }
  let extra = 0;
  if (name === "utf16_validate") {
    text += (seed & 8n) === 0n ? "" : "\uD800";
    extra = Number(text.isWellFormed());
    text = text.toWellFormed();
  } else if (name === "utf8_roundtrip") {
    const bytes = new TextEncoder().encode(text);
    const restored = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    assert.equal(restored, text);
    extra = bytes.length;
    text = restored;
  }
  let total = extra;
  for (let index = 0; index < text.length; ++index) total += text.charCodeAt(index);
  return BigInt(total);
}

function array(count, seed, copy = false, computation = false) {
  const values = new BigUint64Array(count);
  for (let index = 0; index < count; ++index) values[index] = computation
    ? mix(BigInt(index), seed) : (BigInt(index) ^ seed) * multiplier + increment;
  const duplicate = copy ? values.slice() : null;
  let total = 0n;
  for (const value of values) total = wrap(total + (computation ? (value ^ (value >> 17n)) * (seed | 1n) : value));
  if (duplicate) for (const value of duplicate) total = wrap(total + value);
  return total;
}

function computation(name, count, seed) {
  if (name.startsWith("array_")) return array(count, seed, false, true);
  const values = name === "owned_capture" ? new BigUint64Array(256) : null;
  if (values) for (let index = 0; index < values.length; ++index) values[index] = mix(BigInt(index), seed);
  let state = seed;
  for (let remaining = count; remaining > 0; --remaining) {
    const salt = wrap(increment + BigInt(remaining));
    switch (name) {
      case "bind": state = mix(state, salt); break;
      case "checked": case "std_result": {
        const first = state ^ salt;
        state = (state & 7n) === 0n ? first : (first & 3n) === 0n ? mix(first, salt) : mix(first, salt) ^ salt;
        break;
      }
      case "delayed": state = wrap(mix(state, salt) + mix(state ^ 71n, salt) + mix(state ^ 113n, salt)); break;
      case "std_option": state = (state & 7n) === 0n ? state ^ salt : mix(state ^ salt, salt); break;
      case "std_option_owned": state = mix(state, salt) ^ ((state & 7n) === 0n ? 0n : 12n); break;
      case "owned_capture": state = mix(values[Number(state & 255n)], state); break;
      default: throw new Error(`Unknown computation: ${name}`);
    }
  }
  return state;
}

function closureCapture(count, seed) {
  const transform = (seed & 1n) === 0n ? (value) => step(value, seed) : (value) => step(value, seed ^ 71n);
  let state = seed;
  for (let index = 0; index < count; ++index) state = transform(state);
  return state;
}

function closureChurn(count, seed) {
  let state = seed;
  for (let index = 0; index < count; ++index) {
    const captured = state;
    const transform = (captured & 1n) === 0n ? (value) => step(value, captured) : (value) => step(value, captured ^ 71n);
    const copy = transform;
    state = copy(state);
    state = transform(state);
  }
  return state;
}

function kernel(family, name, count, seed) {
  assert.ok(Number.isSafeInteger(count) && count >= 0 && count <= 100000000);
  if (family === "cpp" && name === "task_sequence") {
    for (let index = 0; index < count; ++index) {
      const captured = seed;
      const task = () => captured;
      seed = mix(task(), increment);
    }
    return seed;
  }
  if (family === "cpp" && name === "task_sequential") {
    let total = 0n;
    for (let index = 0; index < 16; ++index) total = wrap(total + kernel("cpp", "integer_mix", count, seed ^ BigInt(index)));
    return total;
  }
  if (family === "cpp" && name === "task_parallel") {
    const parallelism = Math.min(16, availableParallelism());
    const workers = [];
    const pending = [];
    for (let first = 1; first < parallelism; ++first) {
      const worker = new Worker(new URL("./javascript-worker.mjs", import.meta.url), {
        workerData: { count, seed, first, stride: parallelism },
      });
      workers.push(worker);
      pending.push(new Promise((resolve, reject) => {
        let result;
        worker.once("message", (value) => { result = value; });
        worker.once("error", reject);
        worker.once("exit", (code) => code === 0 && typeof result === "bigint" ? resolve(result) : reject(new Error(`Worker exited with ${code}`)));
      }));
    }
    let total = 0n;
    for (let index = 0; index < 16; index += parallelism) total = wrap(total + kernel("cpp", "integer_mix", count, seed ^ BigInt(index)));
    return Promise.all(pending).then((values) => values.reduce((sum, value) => wrap(sum + value), total))
      .finally(() => Promise.all(workers.map((worker) => worker.terminate())));
  }
  if (family === "computations") return computation(name, count, seed);
  if (family === "cpp" && ["utf16_scan", "utf16_compare", "utf16_validate", "utf8_roundtrip", "format_parse", "math_intrinsics"].includes(name)) return textAndMath(name, count, seed);
  if (family === "cpp" && name === "integer_mix") {
    for (let index = 0; index < count; ++index) seed = mix(seed, increment);
    return seed;
  }
  if (name === "mandelbrot") {
    assert.ok(count <= 4096);
    if (!count) return 0n;
    const dx = 3 / count, dy = 2 / count, offset = Number(BigInt.asIntN(64, seed)) * 0.000001;
    let total = 0;
    for (let index = 0; index < count * count; ++index) {
      const cr = (index % count) * dx - 2 + offset, ci = Math.floor(index / count) * dy - 1;
      let real = 0, imaginary = 0, iterations = 0;
      while (iterations < 256 && real * real + imaginary * imaginary <= 4) {
        const next = real * real - imaginary * imaginary + cr;
        imaginary = 2 * real * imaginary + ci;
        real = next;
        ++iterations;
      }
      total += iterations;
    }
    return BigInt(total);
  }
  switch (name) {
    case "while_mix": case "for_mix": case "tail_mix": case "tail_if_mix": case "tail_builtin_mix":
      for (let remaining = count; remaining > 0; --remaining) seed = step(seed, BigInt(remaining));
      return seed;
    case "record_pipeline": {
      let state = { value: seed, remaining: count };
      while (state.remaining > 0) state = { value: step(state.value, BigInt(state.remaining)), remaining: state.remaining - 1 };
      return state.value;
    }
    case "match_dispatch": {
      let total = seed;
      for (let index = 0; index < count; ++index) total = wrap(total + factors[Number((seed + BigInt(index)) & 15n)] * (BigInt(index) + 1n));
      return total;
    }
    case "array_sum": return array(count, seed);
    case "array_copy": return array(count, seed, true);
    case "list_sum": {
      let head = null, tail = null;
      for (let index = 0; index < count; ++index) {
        const node = { value: wrap((BigInt(index) ^ seed) * multiplier + increment), next: null };
        if (tail) tail.next = node;
        else head = node;
        tail = node;
      }
      let total = 0n;
      for (let node = head; node; node = node.next) total = wrap(total + node.value);
      return total;
    }
    case "closure_capture": return closureCapture(count, seed);
    case "closure_churn": return closureChurn(count, seed);
    case "integer128_mix": {
      let state = (seed << 64n) | increment;
      for (let remaining = count; remaining > 0; --remaining) state = BigInt.asUintN(128, (state ^ (state >> 43n)) * multiplier + BigInt(remaining));
      return wrap(state ^ (state >> 64n));
    }
    case "float32_mix": case "float64_mix": {
      const round = name === "float32_mix" ? Math.fround : (value) => value;
      let state = round(Number(seed & 65535n) / 16 + 1);
      const factor = round(name === "float32_mix" ? 1.000001 : 1.0000001);
      for (let index = 0; index < count; ++index) state = round(round(state * factor) + (index & 7) / 16);
      return state >= 2 ** 64 ? (1n << 64n) - 1n : BigInt(Math.trunc(state));
    }
    default: throw new Error(`Unknown workload: ${family}/${name}`);
  }
}

const plan = JSON.parse(readFileSync(process.argv[2], "utf8"));
const input = new BigUint64Array(new SharedArrayBuffer(16));
const gc = [];
const observer = new PerformanceObserver((list) => {
  for (const entry of list.getEntries()) gc.push({ start: entry.startTime, duration: entry.duration });
});
observer.observe({ entryTypes: ["gc"] });
const workloads = [];
for (const job of plan.jobs) {
  for (const check of job.checks) {
    assert.equal((await kernel(job.family, job.name, check.size, BigInt(check.seed))).toString(16).padStart(16, "0"), check.checksum, `${job.family}/${job.name}`);
  }
  const measure = async (sample, repeats) => {
    input[0] = BigInt(job.size);
    input[1] = BigInt(sample.seed);
    const expected = BigInt(`0x${sample.checksum}`);
    const start = performance.now();
    for (let repeat = 0; repeat < repeats; ++repeat) {
      const value = kernel(job.family, job.name, Number(Atomics.load(input, 0)), Atomics.load(input, 1));
      const result = typeof value === "bigint" ? value : await value;
      if (result !== expected) throw new Error(`${job.family}/${job.name}: checksum mismatch`);
    }
    return { start, end: performance.now(), repeats, checksum: sample.checksum };
  };
  for (let warm = 0; warm < 3; ++warm) await measure(job.samples[0], 1);
  const calibrated = await measure(job.samples[0], 1);
  const repeats = plan.quick ? 1 : Math.min(10000, Math.ceil(20 / Math.max(0.001, calibrated.end - calibrated.start)));
  const raw = [];
  for (const sample of job.samples) {
    const timing = await measure(sample, repeats);
    await new Promise(setImmediate);
    const events = gc.filter((entry) => entry.start >= timing.start && entry.start < timing.end);
    raw.push({ wall_ms: (timing.end - timing.start) / repeats, repeats, checksum: sample.checksum,
      gc_events: events.length, gc_ms: events.reduce((total, entry) => total + entry.duration, 0) });
  }
  const reclaimStart = performance.now();
  global.gc?.();
  workloads.push({ family: job.family, name: job.name, checks: job.checks.length, raw,
    post_measurement_gc_ms: performance.now() - reclaimStart });
}
observer.disconnect();
console.log(JSON.stringify({ environment: {
  node: process.version, v8: process.versions.v8, clock: "performance.now monotonic wall time",
  integer_semantics: "BigInt with explicit 64/128-bit wrapping", explicit_gc: Boolean(global.gc),
  array_storage: "BigUint64Array; managed allocation and GC, not explicit native free",
  parallel_backend: "bounded worker_threads, creation and exit/join included; worker GC is not observed by the main-thread observer",
}, workloads }));