import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import dgram from "node:dgram";
import dns from "node:dns";
import { once } from "node:events";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import net from "node:net";
import os, { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// E2E for the sockets of Net on WebAssembly (E09 Phase 3): a module built with --wasm-feature jspi --wasm-feature net imports
// `tsuzuri_net`, and the generated JavaScript glue implements those imports on node:net, node:dgram, and node:dns, so the
// blocking API and the async operations both work, end to end, on loopback, against Node peers on ephemeral ports. It needs
// JSPI (Node.js 24 or newer) and is skipped without it. No result depends on how long something took.
//   node tests/net_wasm.mjs target/release/tsuzuri
// It runs from the repository whatever the directory it is started in (the CI starts it in vsc/): the compiler and the
// tools that the environment names by a path are made absolute first, then the working directory moves to the repository.
const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(repository, "target/release/tsuzuri"));
for (const name of ["TSUZURI_CLANG", "TSUZURI_WASM_LD"]) {
  if (/[\\/]/.test(process.env[name] ?? "")) process.env[name] = resolve(process.env[name]);
}
process.chdir(repository);
const root = mkdtempSync(join(tmpdir(), "tsuzuri-net-wasm-"));
const errno = os.constants.errno;

function execute(args, success = true) {
  const result = spawnSync(compiler, args, { encoding: "utf8", timeout: 240_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const pattern = count => Buffer.from(Array.from({ length: count }, (_, index) => index % 256));
const listen = handler => new Promise((resolveListen, reject) => {
  const server = net.createServer(handler);
  server.open = new Set();
  server.on("connection", socket => { server.open.add(socket); socket.once("close", () => server.open.delete(socket)); });
  server.once("error", reject);
  server.listen(0, "127.0.0.1", () => resolveListen(server));
});
// Stops a peer's listener and ends what is still open to it, so that a connection that the program never closed (a failure of
// the host, or a case that ends first) does not keep `close` waiting.
const stop = server => new Promise(resolveStop => {
  server.close(() => resolveStop());
  for (const socket of server.open ?? []) socket.destroy();
});
const pause = milliseconds => new Promise(resolvePause => setTimeout(resolvePause, milliseconds));
// A wait for what a peer should see is bounded, so that a host that never does it fails the case instead of hanging the run.
function within(promise, what, milliseconds = 20_000) {
  let timer;
  const late = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`timed out waiting for ${what}`)), milliseconds); });
  return Promise.race([promise, late]).finally(() => clearTimeout(timer));
}
// How long a call of the program may take in the cases that depend on a datagram or connection peer.
const callWait = 60_000;
// A Node datagram peer that collects what a program sends it, up to a fence: a datagram that the peer sends to itself once the
// program's call has returned. A loopback socket receives datagrams in the order in which they were sent, so when the fence is
// read everything that the program sent has been, however many datagrams a send made, and no case waits for time to pass.
async function collector() {
  const socket = dgram.createSocket("udp4");
  const messages = [];
  const fenced = Promise.withResolvers();
  socket.on("message", message => { if (message.toString() === "fence") fenced.resolve(); else messages.push(Buffer.from(message)); });
  await new Promise((resolveBind, reject) => { socket.once("error", reject); socket.bind(0, "127.0.0.1", resolveBind); });
  const port = socket.address().port;
  return {
    socket, port, messages,
    texts: () => messages.map(message => message.toString()).sort(),
    fence: async () => {
      socket.send("fence", port, "127.0.0.1");
      await within(fenced.promise, "the fence behind the program's datagrams");
    },
  };
}
// A case of the host's own behavior: its failure is noted and the next case still runs, so that one run shows every failure.
const failures = [];
let level = "";
async function check(name, body) {
  try {
    await body();
  } catch (error) {
    failures.push(`${name} ${level}`);
    console.error(`Net wasm: FAIL ${name} ${level}\n${error?.stack ?? error}`);
  }
}

try {
  const fixture = join(root, "fixture");
  cpSync("tests/fixtures/net_wasm", fixture, { recursive: true });
  const features = ["--wasm-feature", "jspi", "--wasm-feature", "net"];
  // The names that reach the host's resolver: the glue calls dns.promises.lookup, which is watched here.
  const lookups = [];
  const lookup = dns.promises.lookup;
  dns.promises.lookup = function (host, ...rest) {
    lookups.push(host);
    return lookup.call(this, host, ...rest);
  };
  // The datagram sockets and the listeners that the glue makes (a listener has `pauseOnConnect`, which the peers' own do not),
  // in the order they were made, so that a test can reach the program's own sockets.
  const made = { sockets: [], servers: [] };
  const createSocket = dgram.createSocket;
  dgram.createSocket = function (...rest) {
    const socket = createSocket.apply(this, rest);
    made.sockets.push(socket);
    return socket;
  };
  const createServer = net.createServer;
  net.createServer = function (...rest) {
    const server = createServer.apply(this, rest);
    if (rest[0]?.pauseOnConnect) made.servers.push(server);
    return server;
  };

  // The opt-in is explicit: without it a build that reaches a socket stays E2000 and the default module has no import.
  const plain = join(root, "plain");
  mkdirSync(plain, { recursive: true });
  writeFileSync(join(plain, "Main.tz"), `extern def report :: i64 -> unit
export def probe :: i64 -> i64
fn probe port =
    let address = Maybe.get (Net.parse_ip (ref "127.0.0.1") port)
    let! connected = Net.connect address Maybe.None
    if Result.is_ok (ref connected) then 1 else 0
`);
  for (const [args, message] of [
    [["--target", "wasm32"], /wasm32 output cannot use the Net socket API because the default wasm32 target has no host imports/],
    [["--target", "wasm32", "--wasm-feature", "jspi"], /wasm32 output cannot use the Net socket API/],
    [["--target", "wasm32", "--wasm-host", "wasi"], /wasm32 output cannot use the Net socket API/],
    [["--target", "wasm32", "--wasm-feature", "net"], /--wasm-feature net requires --wasm-feature jspi/],
    [["--target", "wasm32", ...features, "--wasm-feature", "threads"], /--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host/],
    [["--target", "wasm32", ...features, "--wasm-host", "wasi"], /--wasm-feature jspi cannot be combined with --wasm-feature threads or --wasm-host/],
    [["--target", "wasm64", ...features], /--wasm-feature net requires wasm32/],
  ]) {
    const refused = execute(["build", plain, ...args, "-o", join(root, "refused.wasm")], false);
    assert.ok(refused.status === 1 || refused.status === 2, `${args.join(" ")}\n${refused.stderr}`);
    assert.match(refused.stderr, /error\[E2000\]/, args.join(" "));
    assert.match(refused.stderr, message, args.join(" "));
  }
  const native = execute(["build", plain, "--wasm-feature", "net", "-o", join(root, "native")], false);
  assert.match(native.stderr, /error\[E2000\]: --wasm-feature net requires wasm32/);
  execute(["check", fixture]);

  if (typeof WebAssembly.Suspending !== "function") {
    console.log("Net wasm: JSPI checks skipped (WebAssembly.Suspending needs Node.js 24 or newer)");
  } else {
    // A module that reaches no socket gets the glue of JSPI alone: no Node.js modules, no `tsuzuri_net` import, and no export
    // of an allocator that only the sockets need, so the module and its glue load together.
    {
      const pure = join(root, "pure");
      mkdirSync(pure, { recursive: true });
      writeFileSync(join(pure, "Main.tz"), `export def probe :: i64 -> i64
fn probe port =
    match Net.parse_ip (ref "127.0.0.1") port with
    | Maybe.Some address -> Net.port address
    | Maybe.None -> 0 - 1
`);
      const wasm = join(root, "pure.wasm");
      const glue = join(root, "pure.mjs");
      execute(["build", pure, "--target", "wasm32", ...features, "-o", wasm]);
      execute(["build", pure, "--target", "wasm32", ...features, "--emit", "bindings-js", "-o", glue]);
      assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(wasm))), []);
      const text = readFileSync(glue, "utf8");
      for (const part of ["netImports", "NODE_NET", "node:net", "tsuzuri_net"]) assert.ok(!text.includes(part), part);
      const loaded = await (await import(pathToFileURL(glue).href)).load(readFileSync(wasm));
      assert.equal(await loaded.exports.probe(8080n), 8080n, "probe");
    }
    for (const optimization of ["-O0", "-O3"]) {
      level = optimization;
      const wasm = join(root, `net${optimization}.wasm`);
      const glue = join(root, `net${optimization}.mjs`);
      execute(["build", fixture, "--target", "wasm32", ...features, optimization, "-o", wasm]);
      execute(["build", fixture, "--target", "wasm32", ...features, optimization, "--emit", "bindings-js", "-o", glue]);

      // Only what the program reaches is imported, from `tsuzuri_net`, beside the host's own `report` and the reactor's JSPI imports.
      const module = new WebAssembly.Module(readFileSync(wasm));
      const imports = WebAssembly.Module.imports(module).map(({ module: namespace, name }) => `${namespace}.${name}`).sort();
      assert.deepEqual(imports, [
        "tsuzuri.Main.report", "tsuzuri_async.clock", "tsuzuri_async.wait", "tsuzuri_net.accept", "tsuzuri_net.classify", "tsuzuri_net.close",
        "tsuzuri_net.connect", "tsuzuri_net.names", "tsuzuri_net.open", "tsuzuri_net.read", "tsuzuri_net.resolve", "tsuzuri_net.send",
        "tsuzuri_net.try_accept", "tsuzuri_net.try_read", "tsuzuri_net.unwatch", "tsuzuri_net.watch", "tsuzuri_net.write",
      ]);

      const reported = [];
      let onReport = () => {};
      const instantiate = async () => (await import(`${pathToFileURL(glue).href}?${optimization}`)).load(readFileSync(wasm), {
        imports: { "Main.report": port => { reported.push(port); onReport(Number(port)); } },
      });
      const loaded = await instantiate();
      const call = (name, argument = 0n) => loaded.exports[name](BigInt(argument));
      // A case that a broken glue could leave stuck inside a call runs on an instance of its own, so that it cannot hold the
      // cases after it, and is bounded.
      const alone = (name, argument = 0n) => within((async () => (await instantiate()).exports[name](BigInt(argument)))(), `${name} to finish`, callWait);

      // The echo of 1024 bytes, through the blocking API and through the async operations.
      {
        const received = [];
        const server = await listen(socket => { socket.on("data", data => { received.push(data); socket.write(data); }); socket.on("end", () => socket.end()); socket.on("error", () => {}); });
        const port = server.address().port;
        assert.equal(await call("echo", port), 130560n, "echo");
        assert.equal(await call("echo_async", port), 130560n, "echo_async");
        assert.deepEqual(Buffer.concat(received), Buffer.concat([pattern(1024), pattern(1024)]));
        await stop(server);
      }

      // The program listens and a Node client connects: the greeting arrives, the answer goes back, and both sides end.
      for (const [name, expected] of [["serve_once", 6542n], ["serve_async", 542n]]) {
        let reply = "";
        let ended;
        const clientEnded = new Promise(resolveEnded => { ended = resolveEnded; });
        onReport = port => {
          const client = net.connect({ host: "127.0.0.1", port }, () => client.write("hello\n"));
          client.on("data", data => { reply += data; });
          client.on("end", () => ended());
          client.on("error", () => ended());
        };
        assert.equal(await call(name), expected, name);
        await clientEnded;
        assert.equal(reply, "HELLO\n", name);
      }

      // Datagrams: a Node peer sends to the port that the program reports, and gets an answer; two sockets of one program.
      {
        const peer = dgram.createSocket("udp4");
        const answer = new Promise(resolveAnswer => peer.once("message", message => resolveAnswer(message.toString())));
        onReport = port => peer.send("ping", port, "127.0.0.1");
        assert.equal(await call("udp_once"), 41n, "udp_once");
        assert.equal(await answer, "pong");
        peer.close();
        assert.equal(await call("udp_async"), 31n, "udp_async");
      }

      // Name resolution: a name goes to the host's resolver, and address text (even what a system would read as an address
      // in its own way) is decided by the strict parser in the module and never reaches it.
      lookups.length = 0;
      assert.equal(await call("literals"), 2002n, "literals");
      assert.deepEqual(lookups, [], "no address text reached dns.lookup");
      assert.ok((await call("lookup")) >= 1n, "localhost resolves");
      assert.deepEqual(lookups, ["localhost"], "a name reached dns.lookup");
      // A refused connection, and a stream that stays good after a timeout.
      {
        const gone = await listen(() => {});
        const goneServerPort = gone.address().port;
        await stop(gone);
        assert.equal(await call("refused", goneServerPort), 2n, "refused");
        const late = await listen(socket => { socket.on("error", () => {}); setTimeout(() => socket.write("x"), 400); });
        assert.equal(await call("quiet", late.address().port), 101n, "quiet");
        await stop(late);
        const slow = await listen(socket => { socket.on("error", () => {}); setTimeout(() => socket.write("y"), 400); });
        assert.equal(await call("cancel", slow.address().port), 9901n, "cancel");
        await stop(slow);
        const idle = await listen(socket => { socket.on("error", () => {}); });
        assert.equal(await call("lifecycle", idle.address().port), BigInt(errno.EBADF), "lifecycle");
        await stop(idle);
      }

      // Thirty clients and one server in one program, all in one executor.
      assert.equal(await call("crowd"), 30030n, "crowd");

      // IPv6 where the machine has it, a datagram that does not fit, and transfers bigger than one read or one write.
      {
        const v6 = await new Promise(resolveListen => {
          const server = net.createServer(socket => { socket.on("data", data => socket.write(data)); socket.on("end", () => socket.end()); socket.on("error", () => {}); });
          server.once("error", () => resolveListen(undefined));
          server.listen(0, "::1", () => resolveListen(server));
        });
        if (v6) {
          assert.equal(await call("echo6", v6.address().port), 130560n, "echo6");
          await stop(v6);
        } else {
          console.log("Net wasm: skip: ::1 (this machine has no IPv6 loopback)");
        }
        assert.equal(await call("datagrams"), BigInt(errno.EMSGSIZE) * 10000n + 200n, "datagrams");
        const big = await listen(socket => { socket.on("error", () => {}); socket.end(pattern(3145728)); });
        assert.equal(await call("bigread", big.address().port), 401080320n, "bigread");
        await stop(big);
        let total = 0;
        let sum = 0;
        let finished;
        const everything = new Promise(resolveAll => { finished = resolveAll; });
        const slow = await listen(socket => {
          socket.pause();
          socket.on("data", chunk => { total += chunk.length; for (const byte of chunk) sum += byte; if (total === 4194304) finished(); });
          socket.on("error", () => {});
          setTimeout(() => socket.resume(), 200);
        });
        assert.equal(await call("bigwrite", slow.address().port), 1n, "bigwrite");
        await everything;
        assert.equal(total, 4194304);
        assert.equal(sum, (4194304 / 256) * 32640);
        await stop(slow);
      }

      // ---- What the host's sockets do that a program cannot see, and what must not break it ----

      // A discarded instance leaves no socket behind. A trap with a listener, a datagram socket, and a connection open: the
      // listener refuses, both ports can be bound again at once, the peer sees its connection end, and the next call (on a new
      // instance) works.
      await check("trap_with_sockets", async () => {
        const ended = Promise.withResolvers();
        const server = await listen(socket => { socket.on("error", () => {}); socket.on("close", () => ended.resolve()); socket.resume(); });
        const ports = [];
        onReport = port => ports.push(port);
        await assert.rejects(call("trap_with_sockets", server.address().port), error => error.name === "TsuzuriTrap");
        await within(ended.promise, "the peer's connection to the discarded instance to end");
        assert.equal(ports.length, 2, "trap_with_sockets reported its ports");
        await stop(server);
        const refusal = net.connect({ host: "127.0.0.1", port: ports[0] });
        const [refused] = await once(refusal, "error");
        assert.equal(refused.code, "ECONNREFUSED", "the listener of the discarded instance");
        const rebound = net.createServer(() => {});
        await new Promise((resolveListen, reject) => { rebound.once("error", reject); rebound.listen(ports[0], "127.0.0.1", resolveListen); });
        await stop(rebound);
        const datagram = dgram.createSocket("udp4");
        await new Promise((resolveBind, reject) => { datagram.once("error", reject); datagram.bind(ports[1], "127.0.0.1", resolveBind); });
        datagram.close();
        assert.equal(await call("refused", ports[0]), 2n, "a new instance after the trap");
      });

      // A program that ends with sockets that it never closed lets Node end by itself, as the process end closes them natively.
      await check("leak", async () => {
        const script = join(root, "leak.mjs");
        writeFileSync(script, `import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
const [glue, wasm] = process.argv.slice(2);
const loaded = await (await import(pathToFileURL(glue).href)).load(readFileSync(wasm), { imports: { "Main.report": () => {} } });
console.log("leaked " + await loaded.exports.leak(0n));
`);
        const child = spawnSync(process.execPath, [script, glue, wasm], { encoding: "utf8", timeout: 60_000 });
        assert.ifError(child.error);
        assert.equal(child.status, 0, `${child.stdout}\n${child.stderr}`);
        assert.equal(child.stdout, "leaked 7\n");
      });

      // A datagram to port 0 (which Node refuses by throwing) is a failure of that call, through both sends.
      await check("zero_port", async () => {
        assert.equal(await call("zero_port"), BigInt(errno.EINVAL) * 1000n + BigInt(errno.EINVAL), "zero_port");
      });

      // The peer's half-close does not end the program's writing: a request, a half-close, and then the answer.
      await check("half_close", async () => {
        let reply = "";
        const answered = Promise.withResolvers();
        onReport = port => {
          const client = net.connect({ host: "127.0.0.1", port }, () => client.end("request"));
          client.on("data", data => { reply += data; });
          client.on("end", () => answered.resolve());
          client.on("error", () => answered.resolve());
        };
        assert.equal(await call("half_close"), 71n, "half_close");
        await within(answered.promise, "the answer after the half-close");
        assert.equal(reply, "ANSWER", "the answer after the peer's half-close");
      });

      // A datagram that the system refuses (one byte over the largest) fails that send only, and the same socket still receives.
      await check("datagram_failure", async () => {
        assert.equal(await call("datagram_failure"), BigInt(errno.EMSGSIZE) * 10000n + 100n + 2n, "datagram_failure");
      });

      // A reset that comes while the program is not reading is a reset, then the end: the peer resets the first connection
      // (after the program's side has seen it) and connects a second one, which ends the program's wait.
      await check("reset_idle", async () => {
        const second = [];
        let flow;
        onReport = port => {
          flow = (async () => {
            const server = made.servers.at(-1);
            const arrived = once(server, "connection");
            const first = net.connect({ host: "127.0.0.1", port });
            first.on("error", () => {});
            const connected = once(first, "connect");
            const [theirs] = await within(arrived, "the program's listener to see a connection");
            await within(connected, "the connection to be made");
            const gone = new Promise(resolveGone => theirs.once("close", resolveGone));
            first.resetAndDestroy();
            await within(gone, "the program's side of the reset connection to close");
            const next = net.connect({ host: "127.0.0.1", port });
            next.on("error", () => {});
            second.push(next);
          })();
          flow.catch(() => {});
        };
        try {
          assert.equal(await call("reset_idle"), 301n, "reset_idle");
          await flow;
        } finally {
          for (const socket of second) socket.destroy();
        }
      });

      // What a program does not read is bounded: a datagram socket keeps 1 MiB and 4096 datagrams (a full buffer drops
      // the rest, as a kernel's does), a listener keeps 128 connections and resets the rest. Datagrams are handed to the
      // glue's own handler, so the count does not depend on what a system's buffers hold.
      for (const [size, count, kept] of [[1400, 5000, Math.floor(1048576 / 1400)], [1, 6000, 4096]]) {
        await check(`udp_flood of ${size} bytes`, async () => {
          made.sockets.length = 0;
          onReport = () => {
            const socket = made.sockets.at(-1);
            for (let index = 0; index < count; index++) socket.emit("message", Buffer.alloc(size, index % 256), { address: "127.0.0.1", port: 9, family: "IPv4", size });
          };
          assert.equal(await call("udp_flood"), BigInt(kept), `udp_flood of ${count} datagrams of ${size} bytes`);
        });
      }
      await check("accept_flood", async () => {
        made.servers.length = 0;
        const ports = [];
        const clients = [];
        const queued = [];
        let flow;
        onReport = port => {
          ports.push(port);
          if (ports.length < 2) return;
          flow = (async () => {
            const flooded = made.servers.at(-2);
            const everyone = Promise.withResolvers();
            flooded.on("connection", socket => { queued.push(socket); if (queued.length === 200) everyone.resolve(); });
            for (let batch = 0; batch < 5; batch++) {
              // Forty at a time, each batch connected before the next, so that the system's own queue never overflows (its
              // dropped connection requests would only be retried a second later).
              const connecting = [];
              for (let index = 0; index < 40; index++) {
                const client = net.connect({ host: "127.0.0.1", port: ports[0] });
                client.on("error", () => {});
                clients.push(client);
                connecting.push(new Promise(resolveConnect => { client.once("connect", resolveConnect); client.once("error", resolveConnect); }));
              }
              await within(Promise.all(connecting), "a batch of clients to connect");
            }
            await within(everyone.promise, "the listener to see 200 connections");
            // A failure of the listener (a descriptor that ran out) and of a connection that waits: no unhandled event.
            queued[0].emit("error", new Error("reset while it waits"));
            flooded.emit("error", Object.assign(new Error("too many open files"), { code: "EMFILE" }));
            const gate = net.connect({ host: "127.0.0.1", port: ports[1] });
            gate.on("error", () => {});
            clients.push(gate);
          })();
          flow.catch(() => {});
        };
        try {
          assert.equal(await call("accept_flood"), 128n * 1000n + BigInt(errno.EMFILE), "accept_flood");
          await flow;
        } finally {
          for (const client of clients) client.destroy();
        }
      });

      // Under Async.start the host polls the executor outside JSPI, where a call that suspends the stack is an error, so the
      // operations that try once (a read, an accept, a receive) must not use one.
      await check("Async.start", async () => {
        assert.ok(loaded.async, "the executor of Async.start is the host's");
        const results = [];
        const peers = [];
        const sender = dgram.createSocket("udp4");
        const started = async (name, argument, expected, what) => {
          await call(name, argument);
          await within(loaded.async.settled(), `${what} to finish`);
          assert.deepEqual(results.splice(0), expected, `${what} under Async.start`);
        };
        let greeter;
        try {
          onReport = value => {
            if (value >= 1000000) results.push(value - 1000000);
            else {
              const peer = net.connect({ host: "127.0.0.1", port: value });
              peer.on("error", () => {});
              peers.push(peer);
            }
          };
          greeter = await listen(socket => { socket.on("error", () => {}); socket.write("hello"); });
          await started("started_read", greeter.address().port, [5], "read_async");
          await started("started_accept", 0, [1], "accept_async");
          onReport = value => {
            if (value >= 1000000) results.push(value - 1000000);
            else sender.send("ping", value, "127.0.0.1");
          };
          await started("started_recv", 0, [4], "recv_from_async");
        } finally {
          for (const peer of peers) peer.destroy();
          sender.close();
          if (greeter) await stop(greeter);
        }
      });

      // A datagram send hands exactly one datagram to the system and has a result of its own, however it waits. A peer counts what
      // arrives for one async send, one blocking send, twelve async sends of the same bytes in a row, fifty at once from one
      // executor (a hundred distinct bytes each), and an end marker: 65 sends that said Ok, and 65 datagrams.
      await check("udp_counts", async () => {
        const peer = await collector();
        try {
          assert.equal(await alone("udp_counts", peer.port), 65n, "udp_counts: the sends that said Ok");
          await peer.fence();
        } finally {
          peer.socket.close();
        }
        const texts = peer.texts();
        const times = word => texts.filter(item => item === word).length;
        assert.deepEqual([times("one"), times("blocking"), times("same"), times("end")], [1, 1, 12, 1], `datagrams per send: ${texts.filter(item => item.length < 100).join(" ")}`);
        const burst = peer.messages.filter(message => message.length === 100);
        assert.deepEqual(burst.map(message => message[0]).sort((a, b) => a - b), Array.from({ length: 50 }, (_, index) => index), "the fifty datagrams of the burst, each once");
        assert.ok(burst.every(message => message.subarray(1).every(byte => byte === 85)), "the burst's datagrams arrived whole");
        assert.equal(peer.messages.length, 65, "a datagram for each send and no other");
      });

      // An echo server of recv_from_async and send_to_async: fifty requests, and each answered once.
      await check("udp_echo", async () => {
        const peer = await collector();
        onReport = port => { for (let index = 0; index < 50; index++) peer.socket.send(`request ${index}`, port, "127.0.0.1"); };
        try {
          assert.equal(await alone("udp_echo", 50), 50n, "udp_echo: the answers that were sent");
          await peer.fence();
        } finally {
          peer.socket.close();
        }
        assert.deepEqual(peer.texts(), Array.from({ length: 50 }, (_, index) => `request ${index}`).sort(), "each request answered once");
      });

      // A close right after a send does not cancel it, and the send still says how it went: after a send that has ended (async,
      // blocking), and with the close racing the async send in one executor.
      await check("udp_close_race", async () => {
        const peer = await collector();
        try {
          assert.equal(await alone("udp_close_race", peer.port), 6n, "udp_close_race: the calls that said Ok");
          await peer.fence();
        } finally {
          peer.socket.close();
        }
        assert.deepEqual(peer.texts(), ["after-async", "after-blocking", "racing-close"], "each datagram arrived, once");
      });

      // The `udp` case of the native async fixture (tests/fixtures/net_async, run_udp) through the glue: the same lines that the
      // native program prints, which are the numbers of this system.
      await check("udp_lines", async () => {
        const timedOut = `TimedOut Other ${errno.ETIMEDOUT}`;
        assert.deepEqual((await alone("udp_lines")).split("\n"), [
          "exchange: 31 1", `quiet: ${timedOut}`, `vanished: ${timedOut}`,
          `truncated: Unclassified InvalidInput ${errno.EMSGSIZE} then datagram:2 then datagram:0`, `closed: ok ok Unclassified InvalidInput ${errno.EBADF}`,
        ]);
      });

      // A connection that its peer reset before the program accepted it still names the peer: the glue keeps the addresses that the
      // connection had when it arrived (a reset connection has no peer name to ask for later), as the address that `accept` gives
      // natively.
      await check("peer_after_reset", async () => {
        made.servers.length = 0;
        const ports = [];
        const open = [];
        let clientPort = 0;
        let flow;
        onReport = port => {
          ports.push(port);
          if (ports.length < 2) return;
          flow = (async () => {
            const queued = made.servers.at(-2);
            const arrived = new Promise(resolveArrived => queued.once("connection", resolveArrived));
            const client = net.connect({ host: "127.0.0.1", port: ports[0] });
            client.on("error", () => {});
            open.push(client);
            await within(new Promise(resolveConnect => client.once("connect", resolveConnect)), "the client to connect");
            await within(arrived, "the program's listener to see the connection");
            clientPort = client.localPort;
            const gone = new Promise(resolveGone => client.once("close", resolveGone));
            client.resetAndDestroy();
            await within(gone, "the reset client to close");
            const gate = net.connect({ host: "127.0.0.1", port: ports[1] });
            gate.on("error", () => {});
            open.push(gate);
          })();
          flow.catch(() => {});
        };
        try {
          const outcome = await alone("peer_after_reset");
          await flow;
          assert.ok(clientPort > 0, "the client had a port");
          assert.equal(outcome, BigInt(clientPort) * 10n + 1n, "the accepted connection's peer is the client that reset it");
        } finally {
          for (const socket of open) socket.destroy();
        }
      });

      // The glue's table of error names is the system's and libuv's: libuv names errors that the system's constants lack
      // (EHOSTDOWN). A connect that fails with each name is classified as natively (`refused` answers the index of the kind).
      await check("error_names", async () => {
        const connect = net.Socket.prototype.connect;
        try {
          for (const [code, expected] of [
            ["ETIMEDOUT", 1n], ["ECONNREFUSED", 2n], ["ECONNRESET", 3n], ["ECONNABORTED", 3n], ["EPIPE", 3n], ["EADDRINUSE", 4n], ["EADDRNOTAVAIL", 5n],
            ["ENETUNREACH", 6n], ["EHOSTUNREACH", 6n], ["ENETDOWN", 6n], ["EHOSTDOWN", 6n], ["EACCES", 7n], ["EWHATEVER", 7n],
          ]) {
            net.Socket.prototype.connect = function () {
              process.nextTick(() => this.destroy(Object.assign(new Error(`connect ${code}`), { code })));
              return this;
            };
            assert.equal(await call("refused", 1), expected, `a connect that fails with ${code}`);
          }
        } finally {
          net.Socket.prototype.connect = connect;
        }
      });

      // An address at or above 2 GiB reaches the host as a negative number: with two gibibytes of capacity held, the buffers of
      // the calls are up there. Built once, at -O3, with a heap that can hold them, where the machine has the memory.
      if (optimization === "-O3" && (process.env.TSUZURI_NET_WASM_HIGH === "0" || os.totalmem() < 8 * 2 ** 30)) {
        console.log("Net wasm: skip: high_echo (needs 8 GiB of memory, and TSUZURI_NET_WASM_HIGH not 0)");
      } else if (optimization === "-O3") {
        await check("high_echo", async () => {
          const high = join(root, "net-high.wasm");
          execute(["build", fixture, "--target", "wasm32", ...features, optimization, "--wasm-max-memory", "3GiB", "-o", high]);
          const heavy = await (await import(`${pathToFileURL(glue).href}?${optimization}`)).load(readFileSync(high), {
            imports: { "Main.report": () => {} },
          });
          const server = await listen(socket => { socket.on("data", data => socket.write(data)); socket.on("end", () => socket.end()); socket.on("error", () => {}); });
          assert.equal(await heavy.exports.high_echo(BigInt(server.address().port)), 130562n, "high_echo");
          await stop(server);
        });
      }
      await pause(20);
    }
    assert.deepEqual(failures, [], `Net wasm cases that failed: ${failures.join(", ")}`);
    console.log("Net wasm: blocking and async sockets on Node.js through JSPI passed at -O0 and -O3");
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}
