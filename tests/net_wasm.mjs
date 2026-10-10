import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import dgram from "node:dgram";
import dns from "node:dns";
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
  server.once("error", reject);
  server.listen(0, "127.0.0.1", () => resolveListen(server));
});
const stop = server => new Promise(resolveStop => server.close(() => resolveStop()));
const pause = milliseconds => new Promise(resolvePause => setTimeout(resolvePause, milliseconds));

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
    for (const optimization of ["-O0", "-O3"]) {
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
        "tsuzuri_net.unwatch", "tsuzuri_net.watch", "tsuzuri_net.write",
      ]);

      const reported = [];
      let onReport = () => {};
      const loaded = await (await import(`${pathToFileURL(glue).href}?${optimization}`)).load(readFileSync(wasm), {
        imports: { "Main.report": port => { reported.push(port); onReport(Number(port)); } },
      });
      const call = (name, argument = 0n) => loaded.exports[name](BigInt(argument));

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
      assert.equal(await call("literals"), 1202n, "literals");
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
      await pause(20);
    }
    console.log("Net wasm: blocking and async sockets on Node.js through JSPI passed at -O0 and -O3");
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}
