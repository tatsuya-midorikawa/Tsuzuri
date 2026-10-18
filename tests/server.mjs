// `tsuzuri serve` end to end: a command that a server runs prints what the same command alone
// prints (exit code, standard output, standard error, artifacts), whatever the state of the sources;
// a server refuses what it must not run, and a client that is refused, or that finds anything else
// than a server, runs the command itself without a word.
// usage: node tests/server.mjs [target/release/tsuzuri]
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, unlinkSync, utimesSync, writeFileSync } from "node:fs";
import net from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
// A short name: a socket path has at most 103 bytes on macOS.
const root = realpathSync(mkdtempSync(join(tmpdir(), "tzs-")));
const srv = join(root, "s");
const env = { ...process.env, TSUZURI_CACHE_DIR: join(root, "c"), TSUZURI_SERVER_DIR: srv, TSUZURI_THREADS: "2" };
delete env.TSUZURI_TIME_PASSES;
const servers = new Set();
// The logs of every server that this test started, for the message of a failure.
const logs = [];
// A modification time that two writes can both be set to: the length and the time then stay the same.
const fixed = new Date(1_700_000_000_000);

const pause = (milliseconds) => new Promise((done) => setTimeout(done, milliseconds));

function run(args, options = {}) {
  const result = spawnSync(options.program ?? compiler, args, { cwd: options.cwd ?? root, env: { ...env, ...options.env }, encoding: "utf8", timeout: 180_000, maxBuffer: 64 << 20 });
  assert.ifError(result.error);
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

function runAsync(args, options = {}) {
  return new Promise((done, fail) => {
    const child = spawn(options.program ?? compiler, args, { cwd: options.cwd ?? root, env: { ...env, ...options.env }, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "", stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk) => { stdout += chunk; });
    child.stderr.setEncoding("utf8").on("data", (chunk) => { stderr += chunk; });
    child.on("error", fail);
    child.on("exit", (status) => done({ status, stdout, stderr }));
  });
}

// Writes a file and sets its modification time to the fixed one.
function save(path, text) {
  writeFileSync(path, text);
  utimesSync(path, fixed, fixed);
}

const socketOf = (project) => join(srv, `${createHash("sha256").update(realpathSync(project)).digest("hex").slice(0, 16)}.sock`);

// Starts `tsuzuri serve ARGS`; `log` is what it writes to standard error.
function startServer(args, options = {}) {
  const child = spawn(options.program ?? compiler, ["serve", ...args], { cwd: options.cwd ?? root, env: { ...env, ...options.env }, stdio: ["ignore", "pipe", "pipe"] });
  servers.add(child);
  const state = { child, log: "", exit: null };
  logs.push(state);
  child.stderr.setEncoding("utf8").on("data", (chunk) => { state.log += chunk; });
  child.stdout.resume();
  state.exited = new Promise((done) => child.on("close", (code, signal) => { state.exit = { code, signal }; servers.delete(child); done(state.exit); }));
  return state;
}

function status(project, options) {
  const result = run(["serve", "--status", project], options);
  assert.equal(result.status, 0, `serve --status: ${result.stderr}\n${logs.map((server) => `[${JSON.stringify(server.exit)}] ${server.log}`).join("\n")}`);
  return JSON.parse(result.stdout);
}

// Resolves with the first status when the server answers (a hang is a failure after 30 s: no time is asserted).
async function ready(server, project, options = {}) {
  const started = Date.now();
  while (Date.now() - started < 30_000) {
    if (server.exit !== null) throw new Error(`the server ended with ${JSON.stringify(server.exit)} before it was ready\n${server.log}`);
    const result = run(["serve", "--status", project], options);
    if (result.status === 0) {
      // The server's standard error arrives while the event loop runs.
      for (let waited = 0; waited < 5_000 && !server.log.includes("serve: listening"); waited += 20) await pause(20);
      return JSON.parse(result.stdout);
    }
    await pause(20);
  }
  throw new Error(`the server was not ready after 30 s\n${server.log}`);
}

async function exits(server, milliseconds = 30_000) {
  let timer;
  const late = new Promise((_, fail) => { timer = setTimeout(() => fail(new Error(`the server did not end after ${milliseconds} ms\n${server.log}`)), milliseconds); });
  try { return await Promise.race([server.exited, late]); } finally { clearTimeout(timer); }
}

async function stop(server, project, options = {}) {
  const stopped = run(["serve", "--stop", project], options);
  assert.equal(stopped.status, 0, stopped.stderr);
  assert.match(stopped.stdout, /^stopped the build server for /);
  assert.deepEqual(await exits(server), { code: 0, signal: null }, server.log);
  assert.ok(!existsSync(socketOf(project)), "the socket is gone");
}

// One connection by hand: the hello, and what follows it. The connection stays open until the server has
// answered and closed it, as a client's does: a server takes a request whose client closed its end for one
// that was abandoned (only a connection that sends nothing is closed at once).
function talk(socketPath, send) {
  return new Promise((done, fail) => {
    const socket = net.createConnection(socketPath);
    let data = "";
    const timer = setTimeout(() => { socket.destroy(); fail(new Error("no answer after 30 s")); }, 30_000);
    socket.setEncoding("utf8");
    socket.on("data", (chunk) => { data += chunk; });
    socket.on("error", (error) => { if (error.code !== "ECONNRESET" && error.code !== "EPIPE") { clearTimeout(timer); fail(error); } });
    socket.on("close", () => { clearTimeout(timer); const [hello, ...rest] = data.split("\n"); done({ hello, reply: rest.join("\n").trim() }); });
    socket.on("connect", () => { if (send !== null) socket.write(send); else socket.end(); });
  });
}

// Something that listens where a server does, and is not one: `behave(connection)`. With `answerOnce`
// it also ends the connection with that text after the first request line.
async function impostor(project, behave, answerOnce = null) {
  mkdirSync(srv, { recursive: true, mode: 0o700 });
  chmodSync(srv, 0o700);
  const path = socketOf(project);
  rmSync(path, { force: true });
  const fake = { connections: 0, requests: [], answerOnce };
  fake.server = net.createServer((connection) => {
    fake.connections += 1;
    let data = "";
    connection.on("error", () => {});
    connection.on("data", (chunk) => {
      data += chunk;
      if (data.includes("\n") && fake.requests.length === 0) {
        fake.requests.push(data.split("\n")[0]);
        if (fake.answerOnce !== null) connection.end(fake.answerOnce);
      }
    });
    behave(connection, fake);
  });
  await new Promise((done) => fake.server.listen(path, done));
  fake.close = async () => { await new Promise((done) => fake.server.close(done)); rmSync(path, { force: true }); };
  return fake;
}

const helloOf = (project, overrides = {}) => `${JSON.stringify({ protocol: 1, server: "tsuzuri", version: "0.1.0", pid: 1, root: realpathSync(project), ...overrides })}\n`;

function copyOf(name) {
  mkdirSync(join(root, "b"), { recursive: true });
  const copy = join(root, "b", name);
  copyFileSync(compiler, copy);
  chmodSync(copy, 0o755);
  return copy;
}

// What a refusal counted since `before`.
function refusalsSince(before, after) {
  const found = {};
  for (const [name, count] of Object.entries(after.refusals)) if (count !== (before.refusals[name] ?? 0)) found[name] = count - (before.refusals[name] ?? 0);
  return found;
}

try {
  const app = join(root, "app");
  mkdirSync(app);
  const main = join(app, "Main.tz");
  save(main, "42\n");
  const out = join(root, "o");
  mkdirSync(out);

  if (process.platform === "win32") {
    // No server without Unix domain sockets: `serve` says so, and every command runs by itself.
    const serve = run(["serve", app]);
    assert.equal(serve.status, 2);
    assert.match(serve.stderr, /error\[E2000\]: the build server needs Unix domain sockets/);
    const checked = run(["check", app]);
    assert.equal(checked.status, 0);
    assert.deepEqual(run(["check", app, "--no-server"]), checked);
    console.log("Server: Windows has no server; serve is E2000, and check and build run by themselves");
  } else {
    assert.ok(socketOf(app).length < 100, `the socket path is ${socketOf(app).length} bytes: set TMPDIR to a shorter directory`);

    // 0: what a client sends. An impostor takes the request of a real client and shows it.
    const spy = await impostor(app, (connection) => connection.write(helloOf(app)), "");
    // (The client runs apart from this process: an impostor of this process answers only while the event loop runs.)
    const asked = await runAsync(["check", app]);
    assert.equal(asked.status, 0);
    assert.equal(spy.requests.length, 1, "the client asked the socket that is named after the project");
    const template = JSON.parse(spy.requests[0]);
    await spy.close();
    assert.equal(template.protocol, 1);
    assert.equal(template.op, "run");
    assert.equal(template.cwd, root);
    assert.equal(template.compiler, realpathSync(compiler));
    assert.deepEqual(template.args, ["check", app]);
    assert.ok(template.env.some(([name, value]) => name === "TSUZURI_THREADS" && value === "2"));
    assert.ok(template.env.every((pair) => Array.isArray(pair) && pair.length === 2));
    assert.ok(!template.env.some(([name]) => name === "TERM_PROGRAM" || name === "EDITOR"), "only the variables that matter are sent");

    // 1: matching. A command that the server ran prints what the same command prints with --no-server.
    let server = startServer([app]);
    const first = await ready(server, app);
    assert.equal(first.server, "tsuzuri");
    assert.equal(first.root, realpathSync(app));
    assert.equal(first.compiler, realpathSync(compiler));
    assert.equal(first.compiler_sha256, createHash("sha256").update(readFileSync(compiler)).digest("hex"));
    assert.equal(statSync(srv).mode & 0o777, 0o700, "the directory is private");
    assert.equal(statSync(socketOf(app)).mode & 0o777, 0o600, "and so is the socket");
    assert.match(server.log, /^serve: listening on .*\.sock for /);

    // Runs `args` and says how many requests the server ran for it, and what it refused.
    const ask = (args, options) => {
      const before = status(app);
      const result = run(args, options);
      const after = status(app);
      return { result, served: after.served - before.served, refused: refusalsSince(before, after) };
    };
    const alone = (args, options) => run([...args, "--no-server"], options);

    const commands = [
      { args: ["check", app] },
      { args: ["check", app, "--json"] },
      { args: ["check", app, "--deny-warnings"] },
      { args: ["build", app, "--emit", "llvm", "-O0", "-o", join(out, "a.ll")], artifact: join(out, "a.ll") },
      { args: ["build", app, "--target", "wasm32", "-O0", "-o", join(out, "a.wasm")], artifact: join(out, "a.wasm") },
      { args: ["build", app, "--emit", "llvm", "-O3", "--no-cache", "-o", join(out, "n.ll")], artifact: join(out, "n.ll") },
      { args: ["build", app, "-O0", "--cpu", "generic", "-o", join(out, "a")], executable: join(out, "a") },
    ];
    // Options that other tickets added travel in the request like any other: bitcode (PR08), an instrumented
    // build and a profile that is not there (PR07), the fast backend (PB05). The first two need what a host may not
    // have, an LLVM tool or the profile runtime of Clang, and then fail alone with E2002 (LLVM or the linker failed):
    // such a command is left out, with a line that says why. It is not when CI, LLVM_PREFIX or TSUZURI_PGO_STRICT=1
    // say that the toolchain is there on purpose, and a failure of any other kind is a failure of this test. The
    // fast backend is left out when this tsuzuri was built without it, whatever the environment says.
    const strict = Boolean(process.env.CI || process.env.LLVM_PREFIX) || process.env.TSUZURI_PGO_STRICT === "1";
    save(main, "42\n");
    for (const [label, command] of [
      ["--emit bitcode", { args: ["build", app, "--emit", "bitcode", "-O0", "-o", join(out, "a.bc")], artifact: join(out, "a.bc") }],
      ["--profile-generate", { args: ["build", app, "--profile-generate", join(out, "prof"), "-O0", "-o", join(out, "p")], executable: join(out, "p") }],
      ["--backend fast", { args: ["build", app, "-O0", "--backend", "fast", "-o", join(out, "f")], executable: join(out, "f") }],
      ["--backend fast --emit object", { args: ["build", app, "-O0", "--backend", "fast", "--emit", "object", "-o", join(out, "f.o")], artifact: join(out, "f.o") }],
      ["--backend fast --target wasm32", { args: ["build", app, "--target", "wasm32", "-O0", "--backend", "fast", "-o", join(out, "f.wasm")], artifact: join(out, "f.wasm") }],
    ]) {
      const result = alone(command.args);
      if (result.status === 0) {
        commands.push(command);
        continue;
      }
      if (label.startsWith("--backend fast") && /built without the fast backend/.test(result.stderr)) {
        console.log(`Server: ${label} is not tested: this tsuzuri was built without the fast backend`);
        continue;
      }
      assert.ok(!strict && /error\[E2002\]/.test(result.stderr), `${label} fails alone${strict ? " where the toolchain is meant to be there" : ""}:\n${result.stderr}`);
      console.log(`Server: ${label} is not tested: it fails alone on this host (${result.stderr.split("\n")[0]})`);
    }
    commands.push({ args: ["build", app, "--profile-use", join(out, "none.profdata"), "-O0", "-o", join(out, "q")], executable: join(out, "q") });
    const hold = join(root, "Main.tz.hold");
    const extra = join(app, "Extra.tz");
    const states = [
      ["a valid program", () => save(main, "42\n")],
      ["an edit", () => save(main, "43\n")],
      ["an unknown name", () => save(main, "missing\n")],
      ["a text of the same length with the same modification time", () => save(main, "mixxing\n")],
      ["a syntax error in a new module", () => { save(main, "42\n"); save(extra, "def broken :: i64 = \n"); }],
      ["the new module removed", () => unlinkSync(extra)],
      ["no Main.tz", () => renameSync(main, hold)],
      ["Main.tz again", () => renameSync(hold, main)],
    ];
    // What a command produced: its output, and its artifact (an executable by what it prints: two links of
    // the same program can differ in the fields of the Mach-O file that name the link).
    const outcome = (command, result) => {
      const path = command.artifact ?? command.executable;
      const produced = path && existsSync(path) ? readFileSync(path) : null;
      let ran = null;
      if (command.executable && produced) {
        const program = spawnSync(command.executable, [], { encoding: "utf8", timeout: 60_000 });
        ran = { status: program.status, stdout: program.stdout };
      }
      return { ...result, artifact: command.executable ? produced !== null : produced, ran };
    };
    for (const [state, apply] of states) {
      apply();
      for (const command of commands) {
        const path = command.artifact ?? command.executable;
        const label = `${state}: ${command.args.join(" ")}`;
        if (path) rmSync(path, { force: true });
        const through = ask(command.args);
        assert.equal(through.served, 1, `${label}: the server ran it`);
        const served = outcome(command, through.result);
        if (path) rmSync(path, { force: true });
        assert.deepEqual(served, outcome(command, alone(command.args)), label);
      }
    }
    save(main, "42\n");

    // The same project, named another way: a relative path, a file of it, a trailing slash.
    for (const args of [["check", "app"], ["check", join(app, "Main.tz")], ["check", "app/"], ["build", "app", "-O0", "--emit", "llvm", "-o", join(out, "r.ll")]]) {
      const through = ask(args);
      assert.equal(through.served, 1, args.join(" "));
      assert.deepEqual(through.result, alone(args), args.join(" "));
    }
    // A symbolic link is not a project root (E1011): the server is not asked.
    symlinkSync(app, join(root, "applink"));
    const linked = ask(["check", "applink"]);
    assert.equal(linked.served, 0);
    assert.equal(linked.result.status, 1);
    assert.match(linked.result.stderr, /E1011/);

    // 1b: what the server remembers. When the sources are older than the window of 2 s, a request
    // opens no file (the counters of `status` say what the server's memory did).
    await pause(2_500);
    run(["check", app]);
    run(["check", app]);
    const before = status(app);
    run(["check", app]);
    const after = status(app);
    assert.ok(before.resident_bytes > 0, "the server holds the sources");
    assert.ok(after.source_hits > before.source_hits, "a request was answered from memory");
    assert.equal(after.source_reads, before.source_reads, "and opened no file");
    assert.equal(after.refused, 0);

    // 1c: a file that the server remembers and that then changes, in every way that leaves most of
    // its stat alone: the next request reads what is there.
    const stamp = join(app, "Main.tz.new");
    for (const [label, edit] of [
      ["the same length and modification time", () => save(main, "mixxing\n")],
      ["another length", () => save(main, "missing value\n")],
      ["a new file with the same size and time, renamed over it", () => { save(stamp, "mizzing\n"); renameSync(stamp, main); }],
      ["the first text again", () => save(main, "42\n")],
      ["gone", () => unlinkSync(main)],
      ["back", () => save(main, "42\n")],
    ]) {
      await pause(2_500);
      assert.equal(ask(["check", app]).served, 1);
      assert.equal(ask(["check", app]).served, 1);
      edit();
      const through = ask(["check", app, "--json"]);
      assert.equal(through.served, 1, label);
      assert.deepEqual(through.result, alone(["check", app, "--json"]), label);
    }
    save(main, "42\n");

    // 2: what a server refuses. The command then runs by itself, and prints what it prints alone.
    const refusedBy = (reason, args, options) => {
      const through = ask(args, options);
      assert.equal(through.served, 0, `${reason}: ${args.join(" ")}`);
      assert.deepEqual(through.refused, { [reason]: 1 }, `${args.join(" ")} (${JSON.stringify(options)})`);
      assert.deepEqual(through.result, alone(args, options), `${reason}: ${args.join(" ")}`);
    };
    // A setting that the compiler reads (TSUZURI_*, PATH, the SDK, ...) differs, or is there for one only.
    refusedBy("env", ["check", app], { env: { TSUZURI_THREADS: "3" } });
    refusedBy("env", ["check", app], { env: { PATH: `${root}:${process.env.PATH}` } });
    refusedBy("env", ["check", app], { env: { TSUZURI_FUTURE_SETTING: "1" } });
    refusedBy("env", ["check", app], { env: { MACOSX_DEPLOYMENT_TARGET: "10.15" } });
    refusedBy("env", ["build", app, "--emit", "llvm", "-O0", "-o", join(out, "e.ll")], { env: { LC_ALL: "de_DE.UTF-8" } });
    // One that nothing reads is not compared: the variables of a terminal differ between its windows.
    const unrelated = ask(["check", app], { env: { EXTRA_NOTHING_READS_THIS: "1", TERM_SESSION_ID: "another window" } });
    assert.equal(unrelated.served, 1);
    assert.deepEqual(unrelated.result, alone(["check", app]));
    // The working directory differs: the paths of the command line mean something else there.
    refusedBy("cwd", ["check", "."], { cwd: app });
    // Another compiler file: the server runs the one that it is.
    refusedBy("compiler", ["check", app], { program: copyOf("other") });

    // What is not asked of the server at all: other commands, --no-server, a server that is turned off.
    const counted = status(app).requests;
    for (const args of [["run", app], ["test", app], ["bench", app, "--list"], ["fmt", "--check", app], ["doc", app, "-o", join(out, "doc")], ["check", app, "--no-server"], ["build", app, "--no-server", "--emit", "llvm", "-o", join(out, "x.ll")]]) run(args);
    run(["check", app], { env: { TSUZURI_SERVER_DIR: "" } });
    run(["check", app], { env: { TSUZURI_SERVER_DIR: "relative/dir" } });
    assert.equal(status(app).requests, counted + 1, "only the status request itself was a request");

    // 3: protocol. The server answers what it cannot read, and goes on.
    const raw = async (line) => {
      const { hello, reply } = await talk(socketOf(app), line);
      const parsed = JSON.parse(hello);
      assert.deepEqual([parsed.protocol, parsed.server, parsed.root], [1, "tsuzuri", realpathSync(app)]);
      return reply === "" ? null : JSON.parse(reply);
    };
    const refusal = async (line) => { const reply = await raw(line); assert.equal(reply.ok, false, line?.slice?.(0, 80)); return reply.error; };
    const asRun = (overrides) => `${JSON.stringify({ ...template, ...overrides })}\n`;
    assert.equal(await refusal("not json\n"), "protocol");
    assert.equal(await refusal("[]\n"), "protocol");
    assert.equal(await refusal('{"protocol":2,"op":"status"}\n'), "protocol");
    assert.equal(await refusal('{"protocol":1,"op":"restart"}\n'), "protocol");
    assert.equal(await refusal(asRun({ args: [1] })), "protocol");
    assert.equal(await refusal(asRun({ env: [["A"]] })), "protocol");
    assert.equal(await refusal("x".repeat((1 << 20) + 1)), "too-large");
    assert.equal(await refusal(`${"x".repeat((1 << 20) + 1)}\n`), "too-large");
    assert.equal(await raw(null), null, "a connection that sends nothing is closed without a word");
    assert.equal(await refusal(asRun({ args: ["check", join(root, "o")] })), "root");
    assert.equal(await refusal(asRun({ compiler: "/nonexistent/tsuzuri" })), "compiler");
    assert.equal(await refusal(asRun({ cwd: app })), "cwd");
    assert.equal(await refusal(asRun({ env: [...template.env, ["TSUZURI_FUTURE_SETTING", "1"]] })), "env");
    assert.equal(await refusal(asRun({ env: [] })), "env");
    for (const args of [["run", app], ["test", app], ["fmt", app], ["lsp"], ["serve", app], ["watch", "check", app], ["check", app, "--no-server"], ["frobnicate", app], []]) {
      assert.equal(await refusal(asRun({ args })), "unsupported", JSON.stringify(args));
    }
    // `repl` is a path to the parser, which is a build of a project that is not this one.
    assert.equal(await refusal(asRun({ args: ["repl"] })), "root");
    // A request that is right is run, with the same output as the client's.
    const direct = await raw(asRun({}));
    assert.equal(direct.ok, true);
    assert.deepEqual({ status: direct.exit, stdout: direct.stdout, stderr: direct.stderr }, alone(["check", app]));
    const counters = status(app);
    assert.equal(counters.refusals.protocol, 6);
    assert.equal(counters.refusals["too-large"], 2);
    assert.equal(counters.refusals.unsupported, 9);
    assert.equal(counters.refusals.root, 2);
    assert.equal(ask(["check", app]).served, 1, "the server went on");

    // 4: concurrent clients: every command runs, one after the other.
    const racing = await Promise.all(Array.from({ length: 6 }, (_, index) => runAsync(["build", app, "-O0", "--emit", "llvm", "-o", join(out, `p${index}.ll`)])));
    for (const result of racing) assert.deepEqual(result, { status: 0, stdout: "", stderr: "" });
    const artifacts = Array.from({ length: 6 }, (_, index) => readFileSync(join(out, `p${index}.ll`)));
    for (const artifact of artifacts) assert.deepEqual(artifact, artifacts[0]);
    const racedStatus = status(app);
    assert.equal(racedStatus.refusals.busy, undefined, "six requests fit in the queue");
    assert.ok(racedStatus.served >= 6);

    await stop(server, app);
    assert.match(server.log, /serve: shutdown requested; stopping/);
    assert.match(server.log, /serve: request \d+ served \(exit 0\)/);
    assert.match(server.log, /serve: request \d+ refused: env \(TSUZURI_THREADS differs from the server environment\)/);
    assert.match(server.log, /serve: request \d+ refused: cwd \(/);
    assert.match(server.log, /serve: request \d+ refused: protocol \(/);

    // 5: something else at the place of the server. The client takes anything but a server's answer for
    // "no server", and runs the command itself.
    const expected = alone(["check", app]);
    const hello = helloOf(app);
    const impostors = [
      ["closes at once", (c) => c.destroy()],
      ["says no hello", (c) => c.end("hello there\n")],
      ["says a hello of protocol 2", (c) => c.end(helloOf(app, { protocol: 2 }))],
      ["says a hello of another root", (c) => c.end(helloOf(join(root, "o")))],
      ["says a hello of another server", (c) => c.end(helloOf(app, { server: "other" }))],
      ["says a hello and closes", (c) => c.end(hello)],
      ["refuses", (c) => c.end(`${hello}${JSON.stringify({ protocol: 1, ok: false, error: "busy", message: "full" })}\n`)],
      ["answers with a reply of another protocol", (c) => c.end(`${hello}${JSON.stringify({ protocol: 2, ok: true, exit: 0, stdout: "", stderr: "" })}\n`)],
      ["answers with an exit code beyond 255", (c) => c.end(`${hello}${JSON.stringify({ protocol: 1, ok: true, exit: 300, stdout: "", stderr: "" })}\n`)],
      ["answers with an output that is not text", (c) => c.end(`${hello}${JSON.stringify({ protocol: 1, ok: true, exit: 0, stdout: 1, stderr: "" })}\n`)],
      ["answers with a broken line", (c) => c.end(`${hello}{"protocol":1,"ok":tr`)],
      ["answers with bytes that are no text", (c) => c.end(Buffer.concat([Buffer.from(hello), Buffer.from([0xff, 0xfe, 0x0a])]))],
      ["answers with a line that is far too long", (c) => { c.write(hello); c.end("x".repeat(17 << 20)); }],
    ];
    for (const [label, behave] of impostors) {
      const fake = await impostor(app, behave);
      try {
        assert.deepEqual(await runAsync(["check", app]), expected, label);
        assert.ok(fake.connections >= 1, `${label}: the client tried the socket`);
      } finally {
        await fake.close();
      }
    }
    // An impostor that is only a file, a directory, or a link.
    mkdirSync(srv, { recursive: true });
    for (const make of [() => writeFileSync(socketOf(app), "not a socket"), () => mkdirSync(socketOf(app)), () => symlinkSync(join(root, "nowhere"), socketOf(app))]) {
      make();
      assert.deepEqual(run(["check", app]), expected);
      rmSync(socketOf(app), { recursive: true, force: true });
    }

    // 6: a server that was killed leaves its socket. Clients run by themselves, and the next server takes it.
    server = startServer([app]);
    await ready(server, app);
    assert.equal(ask(["check", app]).served, 1);
    server.child.kill("SIGKILL");
    await exits(server);
    assert.ok(existsSync(socketOf(app)), "a killed server cannot remove its socket");
    assert.deepEqual(run(["check", app]), expected);
    assert.deepEqual(run(["serve", "--status", app]).status, 1);
    server = startServer([app]);
    await ready(server, app);
    assert.equal(ask(["check", app]).served, 1);
    // 6b: a second server for the same project does not start, and does not disturb the first.
    const second = run(["serve", app]);
    assert.equal(second.status, 1);
    assert.match(second.stderr, /error\[E2001\]: a build server is already running for this project/);
    assert.equal(ask(["check", app]).served, 1);
    await stop(server, app);
    // 6c: a place that is no socket is not taken.
    writeFileSync(socketOf(app), "keep");
    const kept = run(["serve", app]);
    assert.equal(kept.status, 1);
    assert.match(kept.stderr, /E2001.*exists and is not a socket/);
    assert.equal(readFileSync(socketOf(app), "utf8"), "keep");
    rmSync(socketOf(app));

    // 7: the directory of the sockets is private, or there is no server.
    rmSync(srv, { recursive: true, force: true });
    mkdirSync(srv, { mode: 0o755 });
    chmodSync(srv, 0o755);
    const shared = run(["serve", app]);
    assert.equal(shared.status, 1);
    assert.match(shared.stderr, /error\[E2001\].*is open to other users \(mode 755\)/);
    assert.deepEqual(run(["check", app]), expected);
    chmodSync(srv, 0o700);
    server = startServer([app]);
    await ready(server, app);
    assert.equal(ask(["check", app]).served, 1);
    // A directory that others can enter is not one to trust, even after the server started in it.
    const servedBefore = status(app).served;
    chmodSync(srv, 0o755);
    assert.deepEqual(run(["check", app]), expected);
    assert.match(run(["serve", "--status", app]).stderr, /no build server is running/);
    chmodSync(srv, 0o700);
    assert.equal(status(app).served, servedBefore, "a client does not use a socket in a shared directory");
    await stop(server, app);
    rmSync(srv, { recursive: true });
    mkdirSync(join(root, "real"), { mode: 0o700 });
    symlinkSync(join(root, "real"), srv);
    const linkedDirectory = run(["serve", app]);
    assert.equal(linkedDirectory.status, 1);
    assert.match(linkedDirectory.stderr, /E2001.*not a file or a symbolic link/);
    assert.deepEqual(run(["check", app]), expected);
    unlinkSync(srv);
    // The setting itself: relative, empty, and too long for a socket path.
    const relative = run(["serve", app], { env: { TSUZURI_SERVER_DIR: "relative/dir" } });
    assert.equal(relative.status, 2);
    assert.match(relative.stderr, /E2000.*TSUZURI_SERVER_DIR must be an absolute path/);
    const off = run(["serve", app], { env: { TSUZURI_SERVER_DIR: "" } });
    assert.equal(off.status, 2);
    assert.match(off.stderr, /E2000.*turned off/);
    const long = run(["serve", app], { env: { TSUZURI_SERVER_DIR: join(root, "d".repeat(120)) } });
    assert.equal(long.status, 2);
    assert.match(long.stderr, /E2000.*too long for this system/);
    const disabled = run(["serve", "--status", app], { env: { TSUZURI_SERVER_DIR: "" } });
    assert.equal(disabled.status, 1);
    assert.match(disabled.stderr, /turned off/);

    // 8: lifetime. A server with nothing to do ends by itself, and one whose compiler was replaced ends.
    server = startServer([app, "--idle-timeout", "1"]);
    assert.deepEqual(await exits(server), { code: 0, signal: null }, server.log);
    assert.match(server.log, /serve: idle for 1 s; stopping/);
    assert.ok(!existsSync(socketOf(app)));

    const replaceable = copyOf("replaceable");
    const withCopy = { program: replaceable };
    server = startServer([app], withCopy);
    await ready(server, app, withCopy);
    const copied = ask(["check", app], withCopy);
    assert.equal(copied.served, 1);
    assert.deepEqual(copied.result, expected);
    copyFileSync(compiler, `${replaceable}.next`);
    chmodSync(`${replaceable}.next`, 0o755);
    renameSync(`${replaceable}.next`, replaceable);
    // The compiler that asks is the new file, at the same path: the server is the old one.
    assert.deepEqual(run(["check", app], withCopy), expected);
    assert.deepEqual(await exits(server), { code: 1, signal: null }, server.log);
    assert.match(server.log, /serve: the compiler binary changed; stopping \(start tsuzuri serve again\)/);
    assert.match(server.log, /refused: compiler-changed/);
    assert.ok(!existsSync(socketOf(app)));
    assert.deepEqual(run(["check", app], withCopy), expected, "and then nothing answers");

    // 8b: a server that runs a command still answers `status` and `--stop`, does not run what a client that
    // went away left in its queue, and ends when the command that runs ends. A Clang that waits for a file
    // holds a build in the middle (the identity of the tool is read on every build, so every build waits).
    const gate = join(root, "gate");
    mkdirSync(gate);
    const opened = join(gate, "open");
    const holding = join(gate, "clang");
    const realClang = process.env.TSUZURI_CLANG ?? spawnSync("sh", ["-c", "command -v clang"], { encoding: "utf8" }).stdout.trim();
    writeFileSync(holding, `#!/bin/sh\nwhile [ ! -e "$TZ_GATE" ]; do sleep 0.05; done\nexec "$TZ_REAL_CLANG" "$@"\n`);
    chmodSync(holding, 0o755);
    // Only TSUZURI_CLANG is compared; the others belong to the script, and are given to every process, so that
    // a client that runs a build itself waits at the gate too.
    const held = { env: { TSUZURI_CLANG: holding, TZ_GATE: opened, TZ_REAL_CLANG: realClang } };
    server = startServer([app], held);
    await ready(server, app, held);
    const waitFor = async (what, done) => {
      for (let waited = 0; waited < 30_000; waited += 25) {
        const current = status(app, held);
        if (done(current)) return current;
        await pause(25);
      }
      throw new Error(`${what} did not happen\n${server.log}`);
    };
    const build = (name) => runAsync(["build", app, "-O0", "--cpu", "generic", "-o", join(out, name)], held);
    const beforeHeld = status(app, held);
    const slowBuild = build("held");
    const running = await waitFor("a request that runs", (current) => current.running !== null);
    assert.equal(running.queued, 0);
    assert.equal(typeof running.running.request, "number");
    // One client waits and is interrupted; another waits.
    const doomed = spawn(compiler, ["check", app], { cwd: root, env: { ...env, ...held.env }, stdio: "ignore" });
    await waitFor("a request that waits", (current) => current.queued === 1);
    doomed.kill("SIGKILL");
    await new Promise((done) => doomed.on("exit", done));
    const patient = runAsync(["check", app], held);
    await waitFor("two requests that wait", (current) => current.queued === 2);
    writeFileSync(opened, "");
    assert.deepEqual(await slowBuild, { status: 0, stdout: "", stderr: "" });
    assert.deepEqual(await patient, expected);
    const afterHeld = status(app, held);
    assert.equal(afterHeld.abandoned, 1, "the request of the client that went away was not run");
    assert.equal(afterHeld.served - beforeHeld.served, 2, "the build and the check that waited");
    assert.equal(afterHeld.running, null);
    assert.equal(afterHeld.queued, 0);
    assert.match(server.log, /serve: request \d+ skipped: its client went away while it waited/);
    // `--stop` while a build runs: the answer is at once, and says what the server waits for; the build is not
    // cut off, and the server ends when it has ended.
    rmSync(opened);
    const lastBuild = build("held-again");
    await waitFor("a request that runs", (current) => current.running !== null);
    const stopping = run(["serve", "--stop", app], held);
    assert.equal(stopping.status, 1, stopping.stdout);
    assert.match(stopping.stderr, /error\[E2001\]: the build server for .* \(pid \d+\) is running request \d+ \(for \d+ s\) and stops when it ends/);
    assert.equal(server.exit, null, "the server goes on while the request runs");
    assert.notEqual(status(app, held).running, null);
    writeFileSync(opened, "");
    assert.deepEqual(await lastBuild, { status: 0, stdout: "", stderr: "" });
    assert.deepEqual(await exits(server), { code: 0, signal: null }, server.log);
    assert.match(server.log, /serve: shutdown requested; stopping/);
    assert.ok(!existsSync(socketOf(app)));
    // 9: TSUZURI_TIME_PASSES: the server's report is the report of the command, once, at the end.
    const timed = { env: { TSUZURI_TIME_PASSES: "1" } };
    server = startServer([app], timed);
    await ready(server, app, timed);
    const reportOf = (result) => {
      const lines = result.stderr.split("\n").filter((line) => line.startsWith('{"time_passes":1,'));
      assert.equal(lines.length, 1, result.stderr);
      assert.ok(result.stderr.trimEnd().endsWith(lines[0]), "the report is the last line");
      const report = JSON.parse(lines[0]);
      assert.equal(typeof report.total_ms, "number");
      return report.phases.map((phase) => phase.name);
    };
    const reported = ask(["check", app], timed);
    assert.equal(reported.served, 1);
    const phases = reportOf(reported.result);
    assert.ok(phases.includes("load") && phases.includes("check"), phases.join());
    assert.deepEqual(phases, reportOf(alone(["check", app], timed)), "the phases are the ones of the command alone");
    save(main, "missing\n");
    const failing = ask(["check", app], timed);
    assert.equal(failing.served, 1);
    assert.ok(!reportOf(failing.result).includes("specialize"), "the phases of an earlier request are not in this one");
    save(main, "42\n");
    // A client without the setting does not use a server that has it: the output would differ.
    const quiet = ask(["check", app]);
    assert.deepEqual(quiet.refused, { env: 1 });
    assert.deepEqual(quiet.result, expected);
    await stop(server, app, timed);

    // 10: the command line of serve and of --no-server.
    for (const args of [[], [app, join(root, "o")], [app, "--idle-timeout", "0"], [app, "--idle-timeout", "86401"], [app, "--idle-timeout", "x"], [app, "--idle-timeout"], [app, "--idle-timeout", "1", "--idle-timeout", "2"], [app, "-O0"], ["--status"], ["--stop", "--status", app], ["--status", app, "--idle-timeout", "5"]]) {
      const result = run(["serve", ...args]);
      assert.equal(result.status, 2, args.join(" "));
      assert.match(result.stderr, /error\[E2000\]/, args.join(" "));
    }
    const missing = run(["serve", join(root, "nowhere")]);
    assert.equal(missing.status, 1);
    assert.match(missing.stderr, /error\[E2001\]: cannot find the project of /);
    const noRoot = run(["serve", "--stop", join(root, "nowhere")]);
    assert.equal(noRoot.status, 1);
    const threads = run(["serve", app], { env: { TSUZURI_THREADS: "0" } });
    assert.equal(threads.status, 2);
    assert.match(threads.stderr, /TSUZURI_THREADS must be an integer from 1 to 32/);
    assert.match(run(["serve", "--stop", app]).stderr, /error\[E2001\]: no build server is running for /);
    for (const action of ["run", "test", "fmt", "bench"]) {
      const result = run([action, app, "--no-server"]);
      assert.equal(result.status, 2, action);
      assert.match(result.stderr, /--no-server is only valid with check or build/);
    }
    assert.match(run(["check", app, "--no-server", "--no-server"]).stderr, /no-server specified more than once/);

    // 11: the form of the strings (PM03). `TSUZURI_STRINGS` is read by the compiler, so the server compares it: a
    // server that has `compact` makes the Latin-1 form that a cold build with it makes, and a client that has
    // another form is refused and gets the build that it asked for.
    const strings = join(root, "strings");
    mkdirSync(strings);
    save(join(strings, "Main.tz"), `"caf\\u00e9".length + "abc".length\n`);
    const compact = { env: { TSUZURI_STRINGS: "compact" } };
    const stringsBuild = (name) => ["build", strings, "--emit", "llvm", "-O0", "-o", join(out, name)];
    const coldUtf16 = run([...stringsBuild("strings-utf16.ll"), "--no-server"]);
    const coldCompact = run([...stringsBuild("strings-cold.ll"), "--no-server"], compact);
    assert.deepEqual([coldUtf16.status, coldCompact.status], [0, 0], `${coldUtf16.stderr}${coldCompact.stderr}`);
    const utf16Ir = readFileSync(join(out, "strings-utf16.ll"), "utf8");
    const compactIr = readFileSync(join(out, "strings-cold.ll"), "utf8");
    assert.ok(compactIr.includes("@tz.string.allocate_latin1(") && !utf16Ir.includes("@tz.string.allocate_latin1("), "the form changes the IR");
    const compactServer = startServer([strings], compact);
    await ready(compactServer, strings, compact);
    const requested = (options, name) => {
      const before = status(strings, compact);
      const result = run(stringsBuild(name), options);
      const after = status(strings, compact);
      return { result, served: after.served - before.served, refused: refusalsSince(before, after) };
    };
    const served = requested(compact, "strings-served.ll");
    assert.equal(served.served, 1, "the server that has the form ran the request");
    assert.deepEqual(served.result, coldCompact);
    assert.equal(readFileSync(join(out, "strings-served.ll"), "utf8"), compactIr, "and made the IR of a cold build with that form");
    const other = requested({}, "strings-refused.ll");
    assert.equal(other.served, 0, "a client with the default form is not served by it");
    assert.deepEqual(other.refused, { env: 1 });
    assert.deepEqual(other.result, coldUtf16);
    assert.equal(readFileSync(join(out, "strings-refused.ll"), "utf8"), utf16Ir, "and gets its own build");
    await stop(compactServer, strings, compact);

    console.log("Server: runs equal the commands alone for every state of the sources and the memory that changes under them, refusals (env, cwd, compiler, root, unsupported, protocol), concurrent clients, impostors, stale sockets, private directories, idle and replaced compiler, time-pass reports, the form of the strings (TSUZURI_STRINGS) and usage errors passed");
  }
} finally {
  for (const child of servers) child.kill("SIGKILL");
  rmSync(root, { recursive: true, force: true });
}
