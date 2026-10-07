// End-to-end test of the thread-pool glue of `--emit bindings-js --wasm-feature threads` (E13 Phase 2).
// Node.js runs the browser glue through a small Web Worker adapter over node:worker_threads. With
// TSUZURI_BROWSER set to a Chrome or Chromium executable, the same glue also runs in that headless
// browser, served with and without the COOP/COEP headers. With TSUZURI_PLAYWRIGHT set to a
// playwright-core package directory, Playwright launches TSUZURI_BROWSER_ENGINE (webkit, chromium or
// firefox; default chromium) instead, from TSUZURI_BROWSER when it is set.
// usage: node tests/bindings_threads.mjs target/release/tsuzuri
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Worker as NodeWorker } from "node:worker_threads";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-bindings-threads-"));
const fixture = join(root, "fixture");
cpSync(join(repository, "tests/fixtures/bindings_threads"), fixture, { recursive: true });
const ISOLATION = "WASM threads need a cross-origin isolated page: serve it from HTTPS or localhost with 'Cross-Origin-Opener-Policy: same-origin' and 'Cross-Origin-Embedder-Policy: require-corp' (crossOriginIsolated is false)";

function run(args) {
  const result = spawnSync(compiler, args, { encoding: "utf8", timeout: 180_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
}

// The host functions every worker builds for itself; `data.counters` is shared by all of them.
writeFileSync(join(root, "host.mjs"), `export function createImports({ workerId, data }) {
  const counters = new Int32Array(data.counters);
  return {
    // Three participants meet before any returns: the tasks run on three threads at once.
    "Main.barrier": (value) => {
      Atomics.or(counters, 1, 1 << workerId);
      if (Atomics.add(counters, 0, 1) === 2) {
        Atomics.store(counters, 2, 1);
        Atomics.notify(counters, 2);
      } else {
        while (!Atomics.load(counters, 2)) if (Atomics.wait(counters, 2, 0, 30000) === "timed-out") return -1n;
      }
      return value;
    },
    "Main.worker_id": () => {
      Atomics.or(counters, 3, 1 << workerId);
      return BigInt(workerId);
    },
    // Fails only on helpers: an i32 result out of range, or an error of the host's own.
    "Main.narrow_on_helpers": (value) => {
      if (workerId === 0) return Number(value);
      if (data.mode === "throw") throw new Error(\`host failure on worker \${workerId}\`);
      return 2 ** 40;
    },
  };
}
`);

// A Web Worker on node:worker_threads, enough for the generated browser glue: module workers,
// postMessage with transfers, message and error events, and terminate.
writeFileSync(join(root, "adapter-worker.mjs"), `import { parentPort, workerData } from "node:worker_threads";
const listeners = new Set();
const queued = [];
let loaded = false;
globalThis.crossOriginIsolated = true;
globalThis.self = globalThis;
globalThis.addEventListener = (type, listener) => { if (type === "message") listeners.add(listener); };
globalThis.removeEventListener = (type, listener) => { listeners.delete(listener); };
globalThis.postMessage = (message, transfer) => parentPort.postMessage(message, transfer);
parentPort.on("message", (data) => {
  if (loaded) for (const listener of [...listeners]) listener({ data });
  else queued.push(data);
});
await import(workerData.url);
loaded = true;
for (const data of queued.splice(0)) for (const listener of [...listeners]) listener({ data });
`);
const adapter = pathToFileURL(join(root, "adapter-worker.mjs"));
class WebWorker {
  #worker;
  #listeners = { message: new Set(), error: new Set() };
  constructor(url, options) {
    assert.equal(options?.type, "module");
    this.#worker = new NodeWorker(adapter, { workerData: { url: String(url) } });
    this.#worker.on("message", (data) => this.#dispatch("message", { data }));
    this.#worker.on("error", (error) => this.#dispatch("error", { message: error.message, error }));
  }
  #dispatch(type, event) {
    for (const listener of [...this.#listeners[type]]) listener(event);
  }
  addEventListener(type, listener) {
    this.#listeners[type]?.add(listener);
  }
  removeEventListener(type, listener) {
    this.#listeners[type]?.delete(listener);
  }
  postMessage(message, transfer) {
    this.#worker.postMessage(message, transfer);
  }
  terminate() {
    return this.#worker.terminate();
  }
}

const sumOfSquares = (start, count) => { let total = 0n; for (let index = 0n; index < count; index++) total += (start + index) ** 2n; return total; };
const units = (text) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));

let server;
let browserProcess;
try {
  run(["build", fixture, "--target", "wasm32", "--wasm-feature", "threads", "--emit", "bindings-js", "-o", join(root, "threads.mjs")]);
  run(["build", fixture, "--target", "wasm32", "--wasm-feature", "threads", "--emit", "bindings-js", "-o", join(root, "again.mjs")]);
  assert.deepEqual(readFileSync(join(root, "threads.mjs")), readFileSync(join(root, "again.mjs")));
  const declarations = readFileSync(join(root, "threads.d.mts"), "utf8");
  assert.match(declarations, /squares\(arg0: tz_record_4Main_5Range\): Promise<bigint>;/);
  assert.match(declarations, /doubled\(arg0: Float64Array\): Promise<Float64Array>;/);
  assert.match(declarations, /importsModule: string \| URL;/);
  assert.doesNotMatch(declarations, /Borrowed|withBorrowed/);
  run(["build", fixture, "--target", "wasm32", "--emit", "bindings-js", "-o", join(root, "single.mjs")]);
  for (const optimization of ["-O0", "-O3"]) run(["build", fixture, "--target", "wasm32", "--wasm-feature", "threads", "--trap-info", optimization, "-o", join(root, `threads${optimization}.wasm`)]);
  run(["build", join(repository, "tests/fixtures/bindings"), "--target", "wasm32", "-o", join(root, "plain.wasm")]);

  globalThis.Worker = WebWorker;
  globalThis.crossOriginIsolated = true;
  const { load, TsuzuriTrap } = await import(pathToFileURL(join(root, "threads.mjs")).href);
  const importsModule = pathToFileURL(join(root, "host.mjs")).href;
  for (const optimization of ["-O0", "-O3"]) {
    const path = join(root, `threads${optimization}.wasm`);
    const bytes = readFileSync(path);
    const sites = JSON.parse(readFileSync(`${path}.trap.json`, "utf8")).sites;
    const counters = new SharedArrayBuffer(16);
    const words = new Int32Array(counters);
    const api = await load(bytes, { workers: 2, importsModule, importData: { counters }, sites });
    assert.ok(Object.isFrozen(api) && Object.isFrozen(api.exports));
    assert.equal(api.workerCount, 2);
    assert.equal(await api.exports.squares({ start: 1n, count: 100n }), sumOfSquares(1n, 100n));
    assert.equal(await api.exports.squares({ start: -5n, count: 0n }), 0n);
    assert.equal(await api.exports.concurrent(), 0n * 100n + 1n * 10n + 2n);
    // The coordinator (0) and both helpers (1, 2) met at the barrier.
    assert.equal(words[1], 0b111);
    assert.equal(typeof (await api.exports.workers_seen(256n)), "bigint");
    assert.equal(words[3] & ~0b111, 0);
    const input = Float64Array.of(1, 2.5, -0);
    const doubled = await api.exports.doubled(input);
    assert.ok(doubled instanceof Float64Array);
    assert.deepEqual([...doubled], [2, 5, -0]);
    assert.deepEqual([...input], [1, 2.5, -0]);
    const large = Float64Array.from({ length: 20000 }, (_, index) => index);
    assert.ok((await api.exports.doubled(large)).every((value, index) => value === 2 * index));
    assert.deepEqual(units(await api.exports.greet("a\ud800")), [97, 0xd800, 33]);
    // Argument errors reject before the pool sees the call.
    await assert.rejects(api.exports.divide(1, 2n), { name: "TypeError", message: "argument 0 of 'divide' must be a bigint" });
    await assert.rejects(api.exports.squares({ start: 1n }), { name: "TypeError", message: "argument 0 of 'squares' field 'count' must be a bigint" });
    await assert.rejects(api.exports.divide(1n), { name: "TypeError", message: "'divide' expects 2 arguments" });
    assert.equal(await api.exports.divide(7n, 2n), 3n);
    // Calls queue in the coordinator and resolve in order.
    assert.deepEqual(await Promise.all([api.exports.divide(9n, 3n), api.exports.squares({ start: 2n, count: 3n }), api.exports.divide(-7n, 2n)]), [3n, 29n, -3n]);
    // A trap stops the pool: the call rejects with the site, and the pool takes no more calls.
    await assert.rejects(api.exports.divide(1n, 0n), (error) => error instanceof TsuzuriTrap && error.trap.reason === "trap" && error.trap.kind === "integer division by zero" && error.trap.path.endsWith("Main.tz"));
    await assert.rejects(api.exports.divide(1n, 1n), { message: "the WASM thread pool stopped after a failure; load the module again" });
    await api.close();
    // A trap on a helper thread stops the coordinator too, with the helper's site.
    const failing = await load(bytes, { workers: 3, importsModule, importData: { counters: new SharedArrayBuffer(16) }, sites });
    await assert.rejects(failing.exports.worker_trap(64n), (error) => error instanceof TsuzuriTrap && error.trap.reason === "trap" && error.trap.kind === "assertion failed");
    await assert.rejects(failing.exports.divide(1n, 1n), /stopped after a failure/);
    await failing.close();
    // Without workers the coordinator runs every task itself: the count is the caller's choice.
    const alone = await load(new WebAssembly.Module(bytes), { workers: 0, importsModule, importData: { counters: new SharedArrayBuffer(16) } });
    assert.equal(alone.workerCount, 0);
    assert.equal(await alone.exports.squares({ start: 1n, count: 10n }), sumOfSquares(1n, 10n));
    await alone.close();
    await assert.rejects(alone.exports.squares({ start: 1n, count: 1n }), { message: "the WASM thread pool is closed" });
    // A host function's error on a helper reaches the call as that error, not as stack exhaustion
    // or an anonymous trap: the glue's RangeError for an import result, and the host's own Error.
    for (const [mode, expected] of [
      ["range", (error) => error instanceof RangeError && error.message === "result of import 'Main.narrow_on_helpers' is out of range for i32"],
      ["throw", (error) => error instanceof Error && !(error instanceof TsuzuriTrap) && /^host failure on worker [12]$/.test(error.message)],
    ]) {
      const hosted = await load(bytes, { workers: 2, importsModule, importData: { counters: new SharedArrayBuffer(16), mode }, sites });
      await assert.rejects(hosted.exports.helper_results(), expected, mode);
      await assert.rejects(hosted.exports.divide(1n, 1n), { message: "the WASM thread pool stopped after a failure; load the module again" });
      await hosted.close();
    }
    console.log(`bindings threads: ${optimization} passed on Web Workers over node:worker_threads`);
  }

  // The checks before any worker starts.
  const bytes = readFileSync(join(root, "threads-O3.wasm"));
  // A helper that fails right after its ready message (the coordinator releases the helpers before
  // it reports ready) still stops the pool: the failure listeners exist from the worker's start.
  class LateFailure {
    static injected = false;
    #inner;
    #messages = new Set();
    constructor(url, options) {
      this.#inner = new WebWorker(url, options);
      const helper = options?.name === "tsuzuri-helper";
      this.#inner.addEventListener("message", (event) => {
        for (const listener of [...this.#messages]) listener(event);
        if (helper && event.data?.kind === "ready" && !LateFailure.injected) {
          LateFailure.injected = true;
          const failed = { data: { kind: "failed", error: { type: "Error", message: "late helper failure" } } };
          for (const listener of [...this.#messages]) listener(failed);
        }
      });
    }
    addEventListener(type, listener) {
      if (type === "message") this.#messages.add(listener);
      else this.#inner.addEventListener(type, listener);
    }
    removeEventListener(type, listener) {
      if (type === "message") this.#messages.delete(listener);
      else this.#inner.removeEventListener(type, listener);
    }
    postMessage(message, transfer) {
      this.#inner.postMessage(message, transfer);
    }
    terminate() {
      return this.#inner.terminate();
    }
  }
  globalThis.Worker = LateFailure;
  const late = await load(bytes, { workers: 1, importsModule, importData: { counters: new SharedArrayBuffer(16) } });
  globalThis.Worker = WebWorker;
  assert.ok(LateFailure.injected);
  await assert.rejects(late.exports.divide(1n, 1n), (error) => error.message === "the WASM thread pool stopped after a failure; load the module again" && error.cause?.message === "late helper failure");
  await late.close();
  await assert.rejects(load(bytes, { workers: 1 }), { name: "TypeError", message: "missing import 'Main.barrier': pass importsModule, the URL of a module whose createImports returns the host functions" });
  await assert.rejects(load(bytes, { workers: 32, importsModule }), RangeError);
  await assert.rejects(load(readFileSync(join(root, "plain.wasm")), { importsModule }), { message: /^threads bindings need a module built with --wasm-feature threads/ });
  const single = await import(pathToFileURL(join(root, "single.mjs")).href);
  await assert.rejects(single.load(bytes), { message: "bindings do not support modules built with --wasm-feature threads; generate the bindings with --wasm-feature threads as well" });
  globalThis.crossOriginIsolated = false;
  await assert.rejects(load(bytes, { importsModule }), { name: "Error", message: ISOLATION });
  globalThis.crossOriginIsolated = true;
  console.log("bindings threads: load checks passed");

  // TypeScript 6.0.3 type-checks the declarations of the thread pool with a small consumer.
  const tsc = process.env.TSUZURI_TSC ?? join(repository, "vsc/node_modules/typescript/bin/tsc");
  if (existsSync(tsc)) {
    writeFileSync(join(root, "consumer.mts"), `import { load, TsuzuriTrap, type CreateImports, type ThreadBindings } from "./threads.mjs";
declare const bytes: ArrayBuffer;
export const createImports: CreateImports = ({ workerId }) => ({ "Main.barrier": (value) => value, "Main.worker_id": () => BigInt(workerId), "Main.narrow_on_helpers": (value) => Number(value) });
const api: ThreadBindings = await load(bytes, { importsModule: new URL("./host.mjs", import.meta.url), workers: 2 });
const total: bigint = await api.exports.squares({ start: 1n, count: 3n });
const doubled: Float64Array = await api.exports.doubled(Float64Array.of(1));
try {
  await api.exports.divide(1n, 0n);
} catch (error) {
  if (error instanceof TsuzuriTrap) void error.trap.reason;
}
void [total, doubled, api.workerCount];
await api.close();
// @ts-expect-error calls of a thread pool return promises
const direct: bigint = api.exports.divide(1n, 2n);
// @ts-expect-error the imports module is required when the module imports host functions
await load(bytes);
// @ts-expect-error a thread pool takes no Borrowed buffers
api.withBorrowed("f64", 1, () => 0);
void direct;
`);
    const typed = spawnSync(process.execPath, [tsc, "--noEmit", "--strict", "--module", "nodenext", "--moduleResolution", "nodenext", "--target", "es2022", join(root, "consumer.mts")], { encoding: "utf8", cwd: root });
    assert.equal(typed.status, 0, `${typed.stdout}\n${typed.stderr}`);
    console.log("bindings threads: tsc passed");
  } else {
    console.log("bindings threads: tsc skipped (run npm ci --prefix vsc or set TSUZURI_TSC)");
  }

  // A real browser, when one is named: COOP/COEP make the page cross-origin isolated.
  const browser = process.env.TSUZURI_BROWSER;
  const playwrightPath = process.env.TSUZURI_PLAYWRIGHT;
  if (!browser && !playwrightPath) {
    console.log("bindings threads: browser skipped (set TSUZURI_BROWSER to a Chrome or Chromium executable, or TSUZURI_PLAYWRIGHT to a playwright-core directory)");
  } else {
    cpSync(join(root, "threads-O3.wasm"), join(root, "threads.wasm"));
    cpSync(join(root, "threads-O3.wasm.trap.json"), join(root, "threads.wasm.trap.json"));
    writeFileSync(join(root, "index.html"), '<!doctype html><meta charset="utf-8"><title>bindings threads</title><script type="module" src="./page.mjs"></script>\n');
    writeFileSync(join(root, "page.mjs"), `import { load, TsuzuriTrap } from "./threads.mjs";
const report = (value) => fetch("/result", { method: "POST", body: JSON.stringify(value) });
try {
  const bytes = await (await fetch("./threads.wasm")).arrayBuffer();
  const { sites } = await (await fetch("./threads.wasm.trap.json")).json();
  const counters = typeof SharedArrayBuffer === "function" ? new SharedArrayBuffer(16) : undefined;
  const api = await load(bytes, { workers: 2, importsModule: new URL("./host.mjs", import.meta.url).href, importData: { counters }, sites });
  const results = { isolated: crossOriginIsolated, workerCount: api.workerCount };
  results.squares = String(await api.exports.squares({ start: 1n, count: 100n }));
  results.concurrent = String(await api.exports.concurrent());
  results.mask = new Int32Array(counters)[1];
  results.doubled = Array.from(await api.exports.doubled(Float64Array.of(1, 2.5)));
  results.greet = await api.exports.greet("a\\ud800");
  try { await api.exports.divide(1n, 0n); } catch (error) { results.trap = { isTrap: error instanceof TsuzuriTrap, trap: error.trap }; }
  try { await api.exports.divide(1n, 1n); } catch (error) { results.after = error.message; }
  await api.close();
  const hosted = await load(bytes, { workers: 2, importsModule: new URL("./host.mjs", import.meta.url).href, importData: { counters: new SharedArrayBuffer(16) }, sites });
  try { await hosted.exports.helper_results(); } catch (error) { results.helper = \`\${error.name}: \${error.message}\`; }
  await hosted.close();
  await report({ ok: true, results });
} catch (error) {
  await report({ ok: false, error: String(error?.message ?? error) });
}
`);
    const types = { ".html": "text/html", ".mjs": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };
    let resolveReport;
    server = createServer((request, response) => {
      if (request.method === "POST" && request.url === "/result") {
        let body = "";
        request.on("data", (chunk) => { body += chunk; });
        request.on("end", () => { response.end("ok"); resolveReport?.(JSON.parse(body)); });
        return;
      }
      const isolated = !request.url.startsWith("/plain/");
      const name = request.url.replace(/^\/plain/, "").split("?")[0].replace(/^\/$/, "/index.html").slice(1);
      let body;
      try {
        if (!/^[\w.-]+$/.test(name)) throw new Error("not served");
        body = readFileSync(join(root, name));
      } catch {
        response.writeHead(404);
        response.end();
        return;
      }
      const headers = { "Content-Type": types[extname(name)] ?? "application/octet-stream", "Cache-Control": "no-store" };
      if (isolated) Object.assign(headers, { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" });
      response.writeHead(200, headers);
      response.end(body);
    });
    server.listen(0, "127.0.0.1");
    await once(server, "listening");
    const base = `http://127.0.0.1:${server.address().port}`;
    const engine = process.env.TSUZURI_BROWSER_ENGINE ?? "chromium";
    const playwright = playwrightPath ? await import(pathToFileURL(join(resolve(playwrightPath), "index.mjs")).href) : undefined;
    let version = "";
    async function visit(path) {
      const reported = new Promise((resolveVisit, reject) => {
        resolveReport = resolveVisit;
        setTimeout(() => reject(new Error(`the browser did not report ${path} within 60 s`)), 60_000).unref();
      });
      if (playwright) {
        if (!playwright[engine]) throw new Error(`TSUZURI_BROWSER_ENGINE must be webkit, chromium or firefox, not '${engine}'`);
        const launched = await playwright[engine].launch(browser ? { executablePath: browser } : {});
        try {
          version = `${engine} ${launched.version()}`;
          await (await launched.newPage()).goto(`${base}${path}`);
          return await reported;
        } finally {
          await launched.close();
        }
      }
      const profile = join(root, `profile${path.replaceAll("/", "-")}`);
      mkdirSync(profile, { recursive: true });
      browserProcess = spawn(browser, ["--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check", "--disable-extensions", "--disable-background-networking", "--disable-component-update", `--user-data-dir=${profile}`, `${base}${path}`], { stdio: "ignore" });
      try {
        return await reported;
      } finally {
        if (browserProcess.exitCode === null && browserProcess.signalCode === null) {
          const exited = once(browserProcess, "exit");
          browserProcess.kill();
          await exited;
        }
        browserProcess = undefined;
      }
    }
    const isolated = await visit("/");
    assert.equal(isolated.ok, true, JSON.stringify(isolated));
    const { results } = isolated;
    assert.equal(results.isolated, true);
    assert.equal(results.workerCount, 2);
    assert.equal(BigInt(results.squares), sumOfSquares(1n, 100n));
    assert.equal(BigInt(results.concurrent), 12n);
    assert.equal(results.mask, 0b111);
    assert.deepEqual(results.doubled, [2, 5]);
    assert.deepEqual(units(results.greet), [97, 0xd800, 33]);
    assert.equal(results.trap.isTrap, true);
    assert.equal(results.trap.trap.kind, "integer division by zero");
    assert.equal(results.after, "the WASM thread pool stopped after a failure; load the module again");
    assert.equal(results.helper, "RangeError: result of import 'Main.narrow_on_helpers' is out of range for i32");
    const plain = await visit("/plain/");
    assert.deepEqual(plain, { ok: false, error: ISOLATION });
    if (!playwright) version = spawnSync(browser, ["--version"], { encoding: "utf8" }).stdout.trim();
    console.log(`bindings threads: browser passed (${version || browser}; isolated page on 2 workers, plain page refused)`);
  }
} finally {
  browserProcess?.kill();
  if (server?.listening) {
    server.closeAllConnections();
    server.close();
  }
  rmSync(root, { recursive: true, force: true });
}
