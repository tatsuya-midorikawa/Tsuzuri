import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { execFile, execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { loadPhysics, stepBody } from "../examples/web/simulation.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-examples-"));
const run = (program, args) => execFileSync(program, args, {
  cwd: root, encoding: "utf8", timeout: 90_000, stdio: ["ignore", "pipe", "pipe"],
});
let server;

try {
  assert.equal(run(compiler, ["run", "examples/hello/Main.tzr"]), "5050\n");
  assert.equal(run(compiler, ["run", "examples/functional/Main.tzr"]), "42\n");
  assert.equal(run(compiler, ["run", "examples/polymorphism"]), "42\n");
  assert.equal(run(compiler, ["run", "examples/currying"]), "42\n");
  assert.equal(Number(run(compiler, ["run", "examples/point"])), Math.sqrt(10 ** 2 + 20.5 ** 2));
  run(process.execPath, ["--check", "examples/web/app.mjs"]);

  const source = join(root, "examples/web/Physics.tzr");
  const wasm = join(temporary, "physics.wasm");
  const object = join(temporary, "physics.o");
  const header = join(temporary, "physics.h");
  run(compiler, ["build", source, "--target", "wasm32", "-o", wasm]);
  run(compiler, ["build", source, "--emit", "object", "-o", object]);
  run(compiler, ["build", source, "--emit", "header", "-o", header]);

  const native = join(temporary, process.platform === "win32" ? "native.exe" : "native");
  run(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O3", "examples/native/main.c",
    "-I", temporary, object, "-o", native, ...(process.platform === "win32" ? [] : ["-lm"])]);
  const expected = { x: 20, y: 230, vx: 120, vy: 90 };
  assert.deepEqual(JSON.parse(run(native, [])), expected);

  if (process.platform !== "win32") {
    const library = join(temporary, process.platform === "darwin" ? "physics.dylib" : "physics.so");
    run(clang, [process.platform === "darwin" ? "-dynamiclib" : "-shared", object, "-lm", "-o", library]);
    const desktop = run(process.env.PYTHON ?? "python3", ["examples/desktop/app.py", library, "--headless"]);
    assert.deepEqual(JSON.parse(desktop), expected);
  }

  const assets = new Map([
    ["/", ["text/html", readFileSync(join(root, "examples/web/index.html"))]],
    ["/app.mjs", ["text/javascript", readFileSync(join(root, "examples/web/app.mjs"))]],
    ["/simulation.mjs", ["text/javascript", readFileSync(join(root, "examples/web/simulation.mjs"))]],
    ["/physics.wasm", ["application/wasm", readFileSync(wasm)]],
  ]);
  server = createServer((request, response) => {
    const asset = assets.get(request.url);
    if (!asset) { response.writeHead(404); response.end("Not found"); return; }
    response.writeHead(200, { "Content-Type": asset[0] });
    response.end(asset[1]);
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const base = `http://127.0.0.1:${server.address().port}`;
  const page = await (await fetch(base)).text();
  assert.match(page, /id="game"/);
  const api = await loadPhysics(`${base}/physics.wasm`);
  await assert.rejects(loadPhysics(`${base}/missing.wasm`), /404/);

  for (const [position, velocity, dt, extent, nextPosition, nextVelocity] of [
    [9, 3, 1, 10, 8, -3],
    [1, -3, 1, 10, 2, 3],
    [9, 1000, 1, 10, 9, 1000],
    [3, -1000, 1, 10, 3, -1000],
    [10, -3, 0, 10, 10, -3],
    [0, -3, 0, 10, 0, 3],
  ]) {
    assert.equal(api.tz_next_position(position, velocity, dt, extent), nextPosition);
    assert.equal(api.tz_next_velocity(position, velocity, dt, extent), nextVelocity);
  }
  for (const args of [[0, 1, -1, 10], [0, NaN, 0, 10], [0, 1, 0, 0.5], [11, 1, 0, 10]]) {
    assert.throws(() => api.tz_next_position(...args), WebAssembly.RuntimeError);
  }
  let body = { x: 112, y: 62, vx: 120, vy: 90, radius: 12 };
  for (let frame = 0; frame < 600; frame++) {
    const before = { ...body };
    const next = stepBody(api, body, 1 / 60, 664, 384);
    assert.deepEqual(body, before, "the host adapter must not mutate the previous state");
    body = next;
  }
  assert.deepEqual(body, { x: expected.x + 12, y: expected.y + 12, vx: expected.vx, vy: expected.vy, radius: 12 });
  for (let index = 0; index < 32; index++) {
    let ball = { x: 20 + index * 7, y: 20 + index * 5, vx: index * 17 - 200, vy: index * 11 - 180, radius: 8 };
    for (let frame = 0; frame < 1000; frame++) {
      ball = stepBody(api, ball, 1 / 60, 960, 540);
      assert.ok(ball.x >= 8 && ball.x <= 952 && ball.y >= 8 && ball.y <= 532);
      assert.equal(Math.abs(ball.vx), Math.abs(index * 17 - 200));
      assert.equal(Math.abs(ball.vy), Math.abs(index * 11 - 180));
    }
  }
  if (process.env.TSUZURI_BROWSER) {
    const { stdout } = await promisify(execFile)(process.env.TSUZURI_BROWSER, [
      "--headless", "--disable-gpu", "--disable-background-networking",
      "--disable-extensions", "--no-first-run", "--no-default-browser-check",
      `--user-data-dir=${join(temporary, "browser-profile")}`,
      "--dump-dom", "--virtual-time-budget=3000", base,
    ], { timeout: 30_000, maxBuffer: 4 * 1024 * 1024 });
    assert.match(stdout, /Running\. UI and drawing stay in JavaScript\.|Paused\. All physics runs in LLVM-generated WASM\./);
    assert.doesNotMatch(stdout, /<button[^>]+id="pause"[^>]+disabled/);
    assert.doesNotMatch(stdout, /class="error"/);
    console.log("Browser: real Chrome loads the LLVM WASM module and enables the application");
  }
  console.log("Examples: console, higher-order functions, native C host, desktop ABI, HTTP/WASM loading, 32,600 game steps");
} finally {
  if (server?.listening) {
    server.closeAllConnections();
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
  rmSync(temporary, { recursive: true, force: true });
}
