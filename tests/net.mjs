import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import dgram from "node:dgram";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import net from "node:net";
import os, { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// E2E for the standard Net module (E09): address parsing and printing, blocking TCP and UDP sockets, name
// resolution, the handle table, and the runtime's allocation. Peers are Node sockets on 127.0.0.1 with ephemeral
// ports ("::1" when the machine has it); nothing leaves the machine, no port is fixed, and no pass or fail
// depends on how long something took. A child that runs for 60 seconds is killed as hung, which is not a limit
// that a test measures. Expectations come from Node's own net, dgram, URL, and os modules.
//   node tests/net.mjs target/release/tsuzuri [address|resolve|sockets|async|runtime|alloc]
// It runs from the repository whatever the directory it is started in (the CI starts it in vsc/): the compiler and the
// tools that the environment names by a path are made absolute first, then the working directory moves to the repository.
// The blocks "alloc" and "runtime" also build with AddressSanitizer and UndefinedBehaviorSanitizer (and, with
// TSUZURI_TSAN=1, ThreadSanitizer); TSUZURI_NO_SANITIZERS=1 leaves them out, for a clang whose sanitizer runtime does not
// start (the Homebrew LLVM 21 of the macOS CI: an empty AddressSanitizer program hangs there).
const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(repository, "target/release/tsuzuri"));
for (const name of ["TSUZURI_CLANG", "TSUZURI_WASM_LD"]) {
  if (/[\\/]/.test(process.env[name] ?? "")) process.env[name] = resolve(process.env[name]);
}
process.chdir(repository);
const selected = process.argv[3];
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sanitized = process.env.TSUZURI_NO_SANITIZERS !== "1";
const threadSanitized = sanitized && process.env.TSUZURI_TSAN === "1";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-net-"));
// The error numbers by name: errno values, or the numbers of Winsock on Windows, which no Node module reports.
const errno = process.platform === "win32"
  ? {
    EBADF: 10009, ECONNREFUSED: 10061, ETIMEDOUT: 10060, ECONNRESET: 10054, ECONNABORTED: 10053, EPIPE: 10058, EADDRINUSE: 10048,
    EADDRNOTAVAIL: 10049, EMSGSIZE: 10040, ENETUNREACH: 10051, EHOSTUNREACH: 10065, ENETDOWN: 10050, EINVAL: 10022, EACCES: 10013, EMFILE: 10024,
  }
  : os.constants.errno;
const optimizations = ["-O0", "-O3"];
const wanted = name => selected === undefined || selected === name;

function execute(program, args, options = {}, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 64 * 1024 * 1024, ...options });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

/** A copy of tests/fixtures/<name> in the temporary directory, which `change` may edit. */
function fixture(name, change) {
  const directory = join(root, `fixture-${name}`);
  rmSync(directory, { recursive: true, force: true });
  cpSync(join("tests", "fixtures", name), directory, { recursive: true });
  change?.(directory);
  return directory;
}

const executables = new Map();
function build(directory, optimization, extra = []) {
  const key = `${directory}${optimization}${extra.join(" ")}`;
  if (!executables.has(key)) {
    const executable = join(directory, `program${optimization}${executables.size}${process.platform === "win32" ? ".exe" : ""}`);
    execute(compiler, ["build", directory, optimization, ...extra, "-o", executable]);
    executables.set(key, executable);
  }
  return executables.get(key);
}

/**
 * Runs a socket program: the case name, the peer's port, and the peer's host go to its standard input (`repeat`
 * times, for a program that runs the case over and over), and `onLine` sees each line it prints as it prints it, so
 * that a peer can answer.
 */
function run(executable, caseName, port = 0, host = "127.0.0.1", onLine = () => {}, { repeat = 1, env = {} } = {}) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(executable, [], { stdio: ["pipe", "pipe", "pipe"], env: { ...process.env, ...env } });
    let stdout = "";
    let stderr = "";
    let pending = "";
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`${caseName} hung\n${stdout}\n${stderr}`));
    }, 60_000);
    child.stdout.on("data", chunk => {
      stdout += chunk;
      pending += chunk;
      for (let newline = pending.indexOf("\n"); newline >= 0; newline = pending.indexOf("\n")) {
        const line = pending.slice(0, newline);
        pending = pending.slice(newline + 1);
        try { onLine(line); } catch (error) { reject(error); }
      }
    });
    child.stderr.on("data", chunk => { stderr += chunk; });
    child.stdin.on("error", () => {});
    child.once("error", error => { clearTimeout(timer); reject(error); });
    child.once("close", (status, signal) => {
      clearTimeout(timer);
      resolveRun({ status, signal, stdout, stderr, lines: stdout.split("\n").slice(0, -1) });
    });
    child.stdin.end(`${caseName}\n${port}\n${host}\n`.repeat(repeat));
  });
}

function listen(handler, host = "127.0.0.1") {
  return new Promise((resolveListen, reject) => {
    const server = net.createServer(handler);
    server.once("error", reject);
    server.listen(0, host, () => resolveListen(server));
  });
}

const stop = server => new Promise(resolveStop => server.close(() => resolveStop()));
const bindDatagram = (type = "udp4", host = "127.0.0.1") => new Promise((resolveBind, reject) => {
  const socket = dgram.createSocket(type);
  socket.once("error", reject);
  socket.bind(0, host, () => resolveBind(socket));
});
const closed = socket => new Promise(resolveClosed => (socket.closed ? resolveClosed() : socket.once("close", resolveClosed)));
// The peer of the case "accept_reset": a connection that it resets at once, then a good one that says "good", then a datagram to
// the gate socket of the program, which accepts only after that: the connection is reset before the accept in every run.
async function resetThenGood(listenerPort, gatePort) {
  const doomed = net.connect({ host: "127.0.0.1", port: listenerPort });
  doomed.on("error", () => {});
  await new Promise(resolveConnect => doomed.once("connect", resolveConnect));
  const reset = closed(doomed);
  doomed.resetAndDestroy();
  await reset;
  const good = net.connect({ host: "127.0.0.1", port: listenerPort });
  good.on("error", () => {});
  await new Promise(resolveConnect => good.once("connect", resolveConnect));
  good.write("good");
  const gate = dgram.createSocket("udp4");
  await new Promise(resolveSend => gate.send("go", gatePort, "127.0.0.1", resolveSend));
  gate.close();
  return good;
}
const socketText = (host, port) => (host.includes(":") ? `[${host}]:${port}` : `${host}:${port}`);
const pattern = count => Buffer.from(Array.from({ length: count }, (_, index) => index % 256));
const patternSum = count => (count / 256) * 32640;
const ok = ["ok"];

// Failures print as "<Net.ErrorKind> <Os.ErrorKind> <errno>"; the numbers are this system's own.
const failure = (kind, osKind, code) => `${kind} ${osKind} ${code}`;
const invalid = failure("Unclassified", "InvalidInput", 0);
const stale = failure("Unclassified", "InvalidInput", errno.EBADF);
// The code of the failure that the async fixtures make up for themselves: no system has an error number like it (Linux has
// 99, EADDRNOTAVAIL, and another system may have any small number), so it is Unclassified everywhere.
const synthetic = 2147418113;

// Address text that a system's resolver reads in its own way ("127.1", "0x7f000001", "fe80::1%lo0"): `Net.resolve` decides
// such a host with the strict parser alone and never asks the system, so the answer does not depend on the system. A host
// is address text when it holds ':' or '%', a space or control character, or its last label (a final dot aside) is all
// digits or starts with "0x" (the same rule as `numeric_looking` in std/Net.tz).
const looksNumeric = host => {
  if (/[:%\u0000-\u0020\u007f]/.test(host)) return true;
  const label = host.replace(/\.$/, "").split(".").pop();
  return /^[0-9]+$/.test(label) || /^0[xX]/.test(label);
};
// The line that the program prints for such a host at port 80: the one address the strict parser reads (with Node's own
// reading of the text as the reference), or an InvalidInput.
const strictLine = host => {
  let shown;
  if (host.includes(":")) shown = !host.includes("%") && net.isIPv6(host) ? `${new URL(`http://[${host}]/`).hostname}:80` : undefined;
  else shown = net.isIPv4(host) ? `${host}:80` : undefined;
  return shown === undefined ? invalid : `1: ${shown}`;
};
// A Tsuzuri string literal for any text without a NUL: control characters and non-ASCII are escaped.
const tzString = text => JSON.stringify(text).replace(/[\u007f-\uffff]/g, symbol => `\\u${symbol.charCodeAt(0).toString(16).padStart(4, "0")}`);
// The hand-written hosts of tests/fixtures/net_resolve/Corpus.tz and a seeded sweep of forms around them: dotted numbers in
// every base `inet_aton` knows, numbers that do not fit, trailing and leading dots, IPv6 with zones and brackets and
// ports, and addresses with spaces or control characters around them. Every host of it is address text.
function resolveCorpus() {
  const written = readFileSync("tests/fixtures/net_resolve/Corpus.tz", "utf8");
  const classic = [...written.matchAll(/^\t"(.*)"[,]?$/gm)].map(match => JSON.parse(`"${match[1]}"`));
  assert.ok(classic.length >= 80, `the corpus file has ${classic.length} hosts`);
  for (const host of classic) assert.ok(looksNumeric(host), `${JSON.stringify(host)} is address text`);
  let state = 0x1f2e3d4c;
  const random = bound => {
    state = (Math.imul(state ^ (state >>> 15), 0x2c1b3c6d) + 0x297a2d39) >>> 0;
    state ^= state >>> 12;
    return (state >>> 0) % bound;
  };
  const pick = list => list[random(list.length)];
  const numbers = ["0", "1", "7", "9", "10", "100", "127", "255", "256", "0x7f", "0X7F", "0x", "0xff", "0xFFFFFFFF", "00", "01", "010", "0177", "08", "0255", "65535", "65536", "16777215", "16777216", "2130706433", "4294967295", "4294967296", "99999999999999999999", "", "1e2", "+1", "-1", "0b1", "1_0"];
  const finals = ["0", "1", "7", "127", "255", "256", "0x7f", "0X0", "0x", "0xg", "00", "010", "08", "65536", "2130706433", "4294967296", "0x7f000001"];
  const octet = () => String(random(256));
  const hex = () => Array.from({ length: 1 + random(4) }, () => pick([..."0123456789abcdefABCDEF"])).join("");
  const dotted = () => {
    const parts = Array.from({ length: random(5) }, () => pick(numbers));
    parts.push(pick(finals));
    if (random(10) === 0) parts.splice(random(parts.length), 0, "");
    return `${random(10) === 0 ? "." : ""}${parts.join(".")}${random(6) === 0 ? "." : ""}`;
  };
  const colons = () => {
    const groups = Array.from({ length: 1 + random(9) }, hex);
    const at = random(groups.length + 1);
    const text = random(3) === 0 ? groups.join(":") : `${groups.slice(0, at).join(":")}::${groups.slice(at).join(":")}`;
    const tail = random(5) === 0 ? `:${octet()}.${octet()}.${octet()}.${octet()}` : "";
    const zone = random(3) === 0 ? pick(["%eth0", "%1", "%", "%lo0", "%en0"]) : "";
    const full = `${text}${tail}${zone}`;
    return random(8) === 0 ? pick([`[${full}]`, `[${full}]:80`, `${full}:80`]) : full;
  };
  const spaced = () => {
    const base = `${octet()}.${octet()}.${octet()}.${octet()}`;
    const edge = pick(["", " ", "\t", "\n", "\r", "\u0001", "\u007f", "\u000b", " x", " 80", "\r\n"]);
    return random(3) === 0 ? `${edge}${base}` : `${base}${edge}`;
  };
  const generated = new Set(classic);
  while (generated.size < classic.length + 800) {
    const host = pick([dotted, dotted, dotted, colons, colons, spaced])();
    if (host.length > 0 && host.length <= 253 && looksNumeric(host)) generated.add(host);
  }
  return [...generated];
}
// The net_resolve project with that corpus, the lines that it must print, and how to write the corpus into another copy.
function resolveLiterals() {
  const hosts = resolveCorpus();
  const literalLines = hosts.map(strictLine);
  assert.ok(literalLines.filter(line => line !== invalid).length >= 25, "the corpus has addresses that the strict parser reads");
  assert.ok(literalLines.filter(line => line === invalid).length >= 600, "the corpus has address text that it refuses");
  for (const text of ["127.1", "0x7f.1", "0x7f000001", "2130706433", "1.2.3", "0177.0.0.1", "010.0.0.1", "0", "0x0", "4294967296", "fe80::1%lo0"]) {
    assert.equal(strictLine(text), invalid, text);
    assert.ok(hosts.includes(text), `the corpus has ${text}`);
  }
  const write = directory => writeFileSync(join(directory, "Corpus.tz"), `def hosts :: unit -> [string]\nfn hosts _unit = [\n${hosts.map(text => `\t${tzString(text)}`).join(",\n")}\n]\n`);
  return { literals: fixture("net_resolve", write), literalLines, write };
}

try {
  if (wanted("address")) {
    // A1: the parser and the printer, on every target, against Node's own reading of the same text.
    const classic = readFileSync("tests/fixtures/net_address/Corpus.tz", "utf8");
    const inputs = [...classic.matchAll(/^\t"(.*)"[,]?$/gm)].map(match => JSON.parse(`"${match[1]}"`));
    assert.equal(inputs.length, 22);
    const expectedTable = [
      "127.0.0.1:80", "0.0.0.0:0", "[::1]:443", "[2001:db8::1]:8080", "[2001:db8::1:0:0:1]:1", "[2001:db8:0:1:1:1:1:1]:1",
      "[::ffff:c000:201]:53", "[::]:65535", ...Array(14).fill("none"),
    ];
    // What Node's net module and URL serializer say about an address: the reference the corpus is compared with.
    const reference = text => {
      let host;
      let portText;
      let v6;
      if (text.startsWith("[")) {
        const close = text.indexOf("]");
        if (close < 0 || text[close + 1] !== ":") return "none";
        host = text.slice(1, close);
        portText = text.slice(close + 2);
        v6 = true;
      } else {
        const parts = text.split(":");
        if (parts.length !== 2) return "none";
        [host, portText] = parts;
        v6 = false;
      }
      if (!/^(0|[1-9][0-9]{0,4})$/.test(portText) || Number(portText) > 65535) return "none";
      if (v6) {
        if (host.includes("%") || !net.isIPv6(host)) return "none";
        return `${new URL(`http://[${host}]/`).hostname}:${portText}`;
      }
      return net.isIPv4(host) ? `${host}:${portText}` : "none";
    };
    for (const [index, input] of inputs.entries()) assert.equal(reference(input), expectedTable[index], `the reference reads ${JSON.stringify(input)}`);

    // A deterministic corpus of near misses and near hits, from a seeded generator.
    let state = 0x9e3779b9;
    const random = bound => {
      state = (Math.imul(state ^ (state >>> 15), 0x2c1b3c6d) + 0x297a2d39) >>> 0;
      state ^= state >>> 12;
      return (state >>> 0) % bound;
    };
    const pick = list => list[random(list.length)];
    const hex = () => Array.from({ length: 1 + random(4) }, () => pick([..."0123456789abcdefABCDEF"])).join("");
    const ports = ["0", "1", "80", "443", "8080", "65535", "22", "1024", "9", "65535", "65536", "99999", "100000", "080", "00", "+80", "8 0", "", "x", "5e3", "-1"];
    const decimal = () => (random(12) === 0 ? pick(["256", "01", "1a", "", "300", "0255", "00", "-1"]) : String(random(256)));
    const v4 = () => Array.from({ length: random(14) === 0 ? pick([3, 5]) : 4 }, decimal).join(".");
    const v6 = () => {
      const tail = random(5) === 0;
      const total = tail ? 6 : 8;
      const written = pick([total, total, random(total), random(total), total - 1]);
      const groups = Array.from({ length: written }, hex);
      let text;
      if (written === total && random(8) !== 0) text = groups.join(":");
      else {
        const at = random(written + 1);
        text = `${groups.slice(0, at).join(":")}::${groups.slice(at).join(":")}`;
      }
      return tail ? `${text}${text.endsWith(":") ? "" : ":"}${v4()}` : text;
    };
    const generated = new Set(inputs);
    while (generated.size < 3200) {
      let text;
      switch (random(5)) {
        case 0: text = `${v4()}:${pick(ports)}`; break;
        case 1: case 2: text = `[${v6()}]:${pick(ports)}`; break;
        case 3: text = random(2) ? `[${v6()}${pick(["%eth0", "%1", "%"])}]:${pick(ports)}` : `${v6()}:${pick(ports)}`; break;
        default: {
          const base = pick([`${v4()}:80`, `[${v6()}]:80`, "[::1]:80", "127.0.0.1:80"]);
          const characters = [...base];
          for (let edit = 1 + random(3); edit > 0; edit--) {
            const at = random(characters.length + 1);
            const operation = random(3);
            const symbol = pick([..."0123456789abcfABCF:.[]% x\u00e9-+"]);
            if (operation === 0) characters.splice(at, 0, symbol);
            else if (operation === 1) characters.splice(at, 1);
            else characters.splice(at, 1, symbol);
          }
          text = characters.join("");
        }
      }
      if (!text.includes("\0")) generated.add(text);
    }
    const corpus = [...generated];
    const escape = text => JSON.stringify(text).replace(/[\u007f-\uffff]/g, symbol => `\\u${symbol.charCodeAt(0).toString(16).padStart(4, "0")}`);
    const expected = corpus.map(reference);
    assert.ok(expected.filter(line => line !== "none").length > 300, "the corpus has accepted addresses");
    assert.ok(expected.filter(line => line === "none").length > 1000, "the corpus has rejected addresses");
    const project = fixture("net_address", directory => writeFileSync(join(directory, "Corpus.tz"), `def inputs :: unit -> [string]\nfn inputs _unit = [\n${corpus.map(text => `\t${escape(text)}`).join(",\n")}\n]\n`));
    const facts = ["192.0.2.7|8080|false|192.0.2.7:8080", "2001:db8::7|0|true|[2001:db8::7]:0", "none", "none", "none", "none", "255.255.255.255|65535|false|255.255.255.255:65535", "true false"];
    const wanted_output = `${expected.join("\n")}\n${facts.join("\n")}\n`;
    for (const optimization of optimizations) {
      const native = execute(build(project, optimization), []);
      assert.equal(native.stdout, wanted_output, `native ${optimization}`);
      const wasmPath = join(root, `address${optimization}.wasm`);
      execute(compiler, ["build", project, "--target", "wasm32", optimization, "-o", wasmPath]);
      const module = new WebAssembly.Module(readFileSync(wasmPath));
      // The address functions are plain Tsuzuri: all the import is the standard output.
      assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name }) => `${module}.${name}`), ["tsuzuri_io.write"]);
      const chunks = [];
      let instance;
      instance = new WebAssembly.Instance(module, { tsuzuri_io: {
        write(destination, pointer, length) {
          assert.equal(destination, 1);
          chunks.push(Buffer.from(new Uint8Array(instance.exports.memory.buffer, pointer, Number(length))));
          return 0;
        },
      } });
      assert.equal(instance.exports.tsuzuri_main(), 0);
      assert.equal(Buffer.concat(chunks).toString(), wanted_output, `wasm32 ${optimization}`);
    }
    console.log(`Net addresses: ${corpus.length} inputs agree with Node on native and wasm32 at -O0 and -O3`);
  }

  if (wanted("resolve")) {
    // T9b: a host that a system's resolver would read as an address in its own way is decided by the strict parser alone
    // ("127.1" is not 127.0.0.1): the same answer on every system, and none of these asks a resolver. Nothing here needs
    // a socket, so this block runs on every platform of the CI.
    const { literals, literalLines } = resolveLiterals();
    for (const optimization of optimizations) {
      const outcome = execute(build(literals, optimization), []);
      assert.equal(outcome.stdout, `${literalLines.join("\n")}\n`, `net_resolve ${optimization}`);
    }
    console.log(`Net resolve: ${literalLines.length} address texts that a system's resolver reads in its own ways are decided by the strict parser alone at -O0 and -O3`);
  }

  if (wanted("sockets")) {
    const project = fixture("net_sockets");
    const v6Server = await listen(() => {}, "::1").then(server => server, () => undefined);
    if (v6Server) await stop(v6Server);
    else console.log("Net: skip: ::1 (this machine has no IPv6 loopback)");

    // The default wasm32 output has no host to open a socket with: it is refused outright, while `check` is fine.
    const connecting = join(root, "connecting");
    mkdirSync(connecting, { recursive: true });
    writeFileSync(join(connecting, "Main.tz"), `def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref "127.0.0.1:9"))
    let! connected = Net.connect address Maybe.None
    do! IO.write_line (Result.is_ok (ref connected))
    0
`);
    for (const optimization of optimizations) {
      for (const extra of [[], ["--wasm-host", "wasi"], ["--wasm-feature", "simd128"]]) {
        const refused = execute(compiler, ["build", connecting, "--target", "wasm32", optimization, ...extra, "-o", join(root, "sockets.wasm")], {}, false);
        assert.equal(refused.status, 1, refused.stderr);
        assert.match(refused.stderr, /error\[E2000\]/);
        assert.match(refused.stderr, /wasm32 output cannot use the Net socket API because the default wasm32 target has no host imports/);
      }
      const llvm = execute(compiler, ["build", connecting, "--target", "wasm32", "--emit", "llvm", optimization, "-o", join(root, "sockets.ll")], {}, false);
      assert.match(llvm.stderr, /error\[E2000\]/);
    }
    execute(compiler, ["check", connecting]);
    execute(compiler, ["check", project]);

    for (const optimization of optimizations) {
      const exe = build(project, optimization);

      // T1 and T10: the echo of 1024 bytes over IPv4 and IPv6.
      for (const host of ["127.0.0.1", "::1"]) {
        if (host === "::1" && !v6Server) continue;
        const received = [];
        const peers = [];
        const remotePorts = [];
        const server = await listen(socket => {
          peers.push(socket);
          remotePorts.push(socket.remotePort);
          socket.on("data", data => { received.push(data); socket.write(data); });
          socket.on("error", () => {});
        }, host);
        const port = server.address().port;
        const result = await run(exe, "echo", port, host);
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.signal, null);
        assert.deepEqual(result.lines, [`local=${socketText(host, remotePorts[0])}`, `peer=${socketText(host, port)}`, "ok", `1024 ${patternSum(1024)}`, "ok"]);
        assert.deepEqual(Buffer.concat(received), pattern(1024));
        await closed(peers[0]);
        await stop(server);
      }

      // T2: the program is the server; its peer address is the Node client's, and the end of its sending reaches Node.
      {
        let client;
        let clientPort;
        let reply = "";
        let ended = false;
        const result = await run(exe, "server", 0, "127.0.0.1", line => {
          const found = /^port=(\d+)$/.exec(line);
          if (!found) return;
          client = net.connect({ host: "127.0.0.1", port: Number(found[1]) }, () => {
            clientPort = client.localPort;
            client.write("hello\n");
          });
          client.on("data", data => { reply += data; });
          client.on("end", () => { ended = true; });
          client.on("error", () => {});
        });
        await closed(client);
        assert.equal(result.status, 0, result.stderr);
        assert.equal(reply, "HELLO\n");
        assert.ok(ended, "shutdown of the write side reached the peer as the end of the stream");
        const port = /^port=(\d+)$/.exec(result.lines[0])[1];
        assert.deepEqual(result.lines, [`port=${port}`, `peer=127.0.0.1:${clientPort}`, `local=127.0.0.1:${port}`, "got=6 542", "ok ok", "rest=0 0", "ok ok"]);
      }

      // T3: the peer sends three bytes and ends.
      {
        const server = await listen(socket => { socket.on("error", () => {}); socket.end("abc"); });
        const result = await run(exe, "eof", server.address().port);
        assert.deepEqual(result.lines, ["abc", "eof", "again:0", "ok"]);
        await stop(server);
      }

      // T4: a port that nothing listens on.
      {
        const server = await listen(() => {});
        const port = server.address().port;
        await stop(server);
        const result = await run(exe, "refused", port);
        assert.deepEqual(result.lines, [failure("ConnectionRefused", "Other", errno.ECONNREFUSED)]);
      }

      // T5: a read that times out leaves the stream good, so the same handle writes next.
      {
        let data = "";
        let peer;
        const server = await listen(socket => { peer = socket; socket.on("data", chunk => { data += chunk; }); socket.on("error", () => {}); });
        const result = await run(exe, "timeout", server.address().port);
        assert.deepEqual(result.lines, [failure("TimedOut", "Other", errno.ETIMEDOUT), "ok", "ok"]);
        await closed(peer);
        assert.equal(data, "x");
        await stop(server);
      }

      // T6: a peer that resets the connection is an error, and no signal ends the program.
      {
        const server = await listen(socket => { socket.on("error", () => {}); socket.resetAndDestroy(); });
        const result = await run(exe, "reset", server.address().port);
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.signal, null, "SIGPIPE does not end the program");
        assert.equal(result.lines.length, 2);
        assert.match(result.lines[0], new RegExp(`^ConnectionReset Other (${errno.ECONNRESET}|${errno.EPIPE})$`));
        assert.equal(result.lines[1], "ok");
        await stop(server);
      }

      // T6 for a connection that the program accepted.
      {
        let client;
        const result = await run(exe, "server_reset", 0, "127.0.0.1", line => {
          const found = /^port=(\d+)$/.exec(line);
          if (!found) return;
          // The reset comes after the program has accepted and written, which is the first thing that reaches the client.
          client = net.connect({ host: "127.0.0.1", port: Number(found[1]) });
          client.once("data", () => client.resetAndDestroy());
          client.on("error", () => {});
        });
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.signal, null, "SIGPIPE does not end the program");
        assert.match(result.lines[1], new RegExp(`^ConnectionReset Other (${errno.ECONNRESET}|${errno.EPIPE})$`));
        assert.deepEqual([result.lines[2], result.lines[3]], ["ok", "ok"]);
      }

      // T7: a datagram comes in with its source, and its bytes go back reversed to that source.
      {
        const udp = await bindDatagram();
        const reply = new Promise(resolveReply => udp.once("message", message => resolveReply(message)));
        const payload = Buffer.from(Array.from({ length: 100 }, (_, index) => index + 1));
        const result = await run(exe, "udp", 0, "127.0.0.1", line => {
          const found = /^port=(\d+)$/.exec(line);
          if (found) udp.send(payload, Number(found[1]), "127.0.0.1");
        });
        const back = await reply;
        const port = /^port=(\d+)$/.exec(result.lines[0])[1];
        assert.deepEqual([...back], [...payload].reverse());
        assert.deepEqual(result.lines, [`port=${port}`, `from=127.0.0.1:${udp.address().port} bytes=100`, "ok", "ok"]);
        udp.close();
      }

      // T8: a datagram longer than the request is an error and is gone; the next one is whole.
      {
        const udp = await bindDatagram();
        const result = await run(exe, "truncate", 0, "127.0.0.1", line => {
          const found = /^port=(\d+)$/.exec(line);
          if (!found) return;
          udp.send(Buffer.alloc(100, 7), Number(found[1]), "127.0.0.1", () => udp.send("ok", Number(found[1]), "127.0.0.1"));
        });
        const port = /^port=(\d+)$/.exec(result.lines[0])[1];
        assert.deepEqual(result.lines, [`port=${port}`, failure("Unclassified", "InvalidInput", errno.EMSGSIZE), "next:ok", "ok"]);
        udp.close();
      }

      // T9: name resolution.
      {
        const result = await run(exe, "resolve");
        assert.equal(result.lines.length, 10, result.stdout);
        assert.equal(result.lines[0], "1: 127.0.0.1:8");
        assert.equal(result.lines[1], "1: [::1]:9");
        const local = result.lines[2].split(": ")[1].split(" ").slice(0);
        assert.match(result.lines[2], /^localhost \d+:/);
        assert.ok(result.lines[2].includes("127.0.0.1:80") || result.lines[2].includes("[::1]:80"), result.lines[2]);
        assert.ok(local.length >= 1);
        assert.deepEqual(result.lines.slice(3, 8), Array(5).fill(invalid));
        assert.equal(result.lines[8], failure("Unclassified", "InvalidEncoding", 0));
        assert.equal(result.lines[9], "1: 127.0.0.1:0");
      }

      // T9b is the block "resolve" above: the address texts that a system's resolver reads in its own ways.

      // T12: an address that is taken, and one that no interface has.
      {
        const server = await listen(() => {});
        const result = await run(exe, "inuse", server.address().port);
        assert.deepEqual(result.lines, [failure("AddressInUse", "AlreadyExists", errno.EADDRINUSE), failure("AddressNotAvailable", "InvalidInput", errno.EADDRNOTAVAIL)]);
        await stop(server);
      }

      // The waits that nothing ends but the timeout.
      {
        const result = await run(exe, "waits");
        const timedOut = failure("TimedOut", "Other", errno.ETIMEDOUT);
        assert.deepEqual(result.lines, [timedOut, "ok", timedOut, "ok"]);
      }

      // V1: what the API refuses before asking the system.
      {
        const server = await listen(socket => { socket.on("error", () => {}); socket.on("data", () => {}); });
        const result = await run(exe, "validate", server.address().port);
        assert.deepEqual(result.lines, [...Array(5).fill(invalid), invalid, invalid, invalid, "ok", invalid, invalid, "ok", "ok"]);
        await stop(server);
      }

      // T11: closing is final; a stale handle is refused even after its slot has a new owner; brackets close.
      {
        const connections = [];
        const server = await listen(socket => {
          // `socket.closed` turns true a tick before "close" is emitted, so wait for the event itself.
          const entry = { socket, closes: 0 };
          entry.done = new Promise(resolveDone => socket.on("close", () => { entry.closes++; resolveDone(); }));
          connections.push(entry);
          socket.on("error", () => {});
          socket.write("G");
        });
        const result = await run(exe, "lifecycle", server.address().port);
        assert.equal(result.status, 0, result.stderr);
        assert.deepEqual(result.lines, ["ok", stale, stale, stale, stale, "greeting:G", stale, "ok", `bracket ${stale}`, "with=true", "leaked"]);
        // Five connections; the program closed four of them, and the process end closed the one it kept.
        assert.equal(connections.length, 5);
        await Promise.all(connections.map(entry => entry.done));
        assert.deepEqual(connections.map(entry => entry.closes), [1, 1, 1, 1, 1]);
        await stop(server);
      }

      // A big write waits for room while the peer reads slowly; the peer gets every byte of it.
      {
        let total = 0;
        let sum = 0;
        let finished;
        const everything = new Promise(resolveAll => { finished = resolveAll; });
        const server = await listen(socket => {
          socket.pause();
          socket.on("data", chunk => { total += chunk.length; for (const byte of chunk) sum += byte; if (total === 8388608) finished(); });
          socket.on("error", () => {});
          setTimeout(() => socket.resume(), 200);
        });
        const result = await run(exe, "bigwrite", server.address().port);
        assert.deepEqual(result.lines, ["ok", "ok", "ok"]);
        await everything;
        assert.equal(total, 8388608);
        assert.equal(sum, patternSum(8388608));
        await stop(server);
      }

      // A read that asks for more than the stack buffer takes.
      {
        const server = await listen(socket => { socket.on("error", () => {}); socket.end(pattern(3145728)); });
        const result = await run(exe, "bigread", server.address().port);
        assert.deepEqual(result.lines, [`3145728 ${patternSum(3145728)}`, "ok"]);
        await stop(server);
      }

      // Both ends in one process: the whole round trip, the error kinds, and a UDP exchange, with no peer.
      {
        const result = await run(exe, "self");
        assert.deepEqual(result.lines, [
          "pattern: ok 1024 130560", "addresses: true true", "reply: ok ok done", "end: read:0",
          `closed: ok ok then ${stale}`, `taken: ${failure("AddressInUse", "AlreadyExists", errno.EADDRINUSE)}`,
          `unavailable: ${failure("AddressNotAvailable", "InvalidInput", errno.EADDRNOTAVAIL)}`,
          `closed listener: ok, connect: ${failure("ConnectionRefused", "Other", errno.ECONNREFUSED)}`, "rebound: bound",
          `udp: ok udp@true ok empty:0 ${failure("TimedOut", "Other", errno.ETIMEDOUT)}`, "resolve: 1: 127.0.0.1:7",
        ]);
      }

      // The listener's SO_REUSEADDR, IPV6_V6ONLY, and FD_CLOEXEC.
      {
        const result = await run(exe, "reuse");
        assert.deepEqual(result.lines, ["ok ok", "ok", "rebound ok"]);
        const cloexec = await run(exe, "cloexec");
        assert.deepEqual(cloexec.lines, ["same", "connected"]);
        if (v6Server) {
          const dual = await run(exe, "dual");
          assert.deepEqual(dual.lines, ["both ok", "ok"]);
        }
      }
    }
    console.log("Net sockets: blocking TCP and UDP, timeouts, resets, resolution, and the handle lifecycle passed at -O0 and -O3");
  }

  if (wanted("async")) {
    const project = fixture("net_async");
    const v6Server = await listen(() => {}, "::1").then(server => server, () => undefined);
    if (v6Server) await stop(v6Server);

    // A program that starts a Net async operation needs the reactor of Async.block_on natively (E2000), and the default
    // wasm32 output has no host to open a socket with.
    const unaided = join(root, "unaided");
    mkdirSync(unaided, { recursive: true });
    writeFileSync(join(unaided, "Main.tz"), `def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref "127.0.0.1:9"))
    let outcome = Async.run (Net.connect_async address Maybe.None)
    do! IO.write_line (Result.is_ok (ref outcome))
    0
`);
    for (const optimization of optimizations) {
      const refused = execute(compiler, ["build", unaided, optimization, "-o", join(root, "unaided")], {}, false);
      assert.equal(refused.status, 1, refused.stderr);
      assert.match(refused.stderr, /error\[E2000\]: the Net async operations \(connect_async, accept_async, read_async, \.\.\.\) complete through the reactor of Async\.block_on/);
      const wasm = execute(compiler, ["build", unaided, "--target", "wasm32", optimization, "-o", join(root, "unaided.wasm")], {}, false);
      assert.match(wasm.stderr, /error\[E2000\]: wasm32 output cannot use the Net socket API/);
    }
    execute(compiler, ["check", unaided]);

    for (const optimization of optimizations) {
      const exe = build(project, optimization);

      // The echo of 1024 bytes over IPv4 and IPv6: the addresses, the half-close, the end of the stream, and the stale handle.
      for (const host of ["127.0.0.1", "::1"]) {
        if (host === "::1" && !v6Server) continue;
        const received = [];
        const server = await listen(socket => {
          socket.on("data", data => { received.push(data); socket.write(data); });
          socket.on("end", () => socket.end());
          socket.on("error", () => {});
        }, host);
        const result = await run(exe, "echo", server.address().port, host);
        assert.equal(result.status, 0, result.stderr);
        assert.equal(result.signal, null);
        assert.deepEqual(result.lines, ["peer=true local_port=true", "ok", `1024 ${patternSum(1024)}`, "ok rest=0 0", `ok stale=${stale}`]);
        assert.deepEqual(Buffer.concat(received), pattern(1024));
        await stop(server);
      }

      // The program accepts a connection of the Node client, answers, and both sides end.
      {
        let client;
        let reply = "";
        let ended = false;
        const result = await run(exe, "server", 0, "127.0.0.1", line => {
          const found = /^port=(\d+)$/.exec(line);
          if (!found) return;
          client = net.connect({ host: "127.0.0.1", port: Number(found[1]) }, () => client.write("hello\n"));
          client.on("data", data => { reply += data; });
          client.on("end", () => { ended = true; });
          client.on("error", () => {});
        });
        await closed(client);
        assert.equal(result.status, 0, result.stderr);
        assert.equal(reply, "HELLO\n");
        assert.ok(ended, "the shutdown of the write side reached the peer as the end of the stream");
        const port = /^port=(\d+)$/.exec(result.lines[0])[1];
        assert.deepEqual(result.lines, [`port=${port}`, "got=6 542", "ok ok", "rest=0 0", "ok ok"]);
      }

      // A connection that its peer resets before the accept takes it is no failure of the accept (Windows reports it from accept as
      // WSAECONNRESET, macOS drops it, Linux hands it over and fails its first read): the next connection is accepted. The peer makes
      // a connection and resets it, makes a good one, and only then sends a datagram to the gate socket that the program waits on
      // before it accepts, so the connection is reset before the accept in every run.
      {
        let peer;
        const result = await run(exe, "accept_reset", 0, "127.0.0.1", line => {
          const found = /^ports=(\d+) (\d+)$/.exec(line);
          if (found) peer = resetThenGood(Number(found[1]), Number(found[2]));
        });
        const good = await peer;
        good.destroy();
        assert.equal(result.status, 0, result.stderr);
        assert.match(result.lines[0], /^ports=\d+ \d+$/);
        assert.deepEqual(result.lines.slice(1), ["gate: datagram:2", "accept: ok read:4"]);
      }

      // Forty clients connect at once to a server in the same executor; sixty-four reads wait together; reads that
      // nothing satisfies are cancelled again and again; closing a socket ends the waits on it.
      assert.deepEqual((await run(exe, "crowd")).lines, ["crowd: server=40 clients=40"]);
      assert.deepEqual((await run(exe, "waits")).lines, ["waits: sum=2016 sent=64", "closed=128"]);
      assert.deepEqual((await run(exe, "cancel")).lines, [
        `cancel: error ${failure("Unclassified", "Other", synthetic)}`, "after: ok read:1", "again: 97", "rounds: 200", "last: 2 243", "closed",
      ]);
      assert.deepEqual((await run(exe, "close_wait")).lines, [`read: error ${stale}`, `accept: error ${stale}`, "done"]);

      // Timeouts are for the whole operation and leave the socket good.
      const timedOut = failure("TimedOut", "Other", errno.ETIMEDOUT);
      assert.deepEqual((await run(exe, "timeouts")).lines, [`read: ${timedOut}`, `accept: ${timedOut}`, "late: 7 1", "closed"]);
      assert.deepEqual((await run(exe, "validate")).lines, [...Array(7).fill(invalid), "ok", invalid, "closed"]);

      // The peer sends three bytes and ends.
      {
        const server = await listen(socket => { socket.on("error", () => {}); socket.end("abc"); });
        const result = await run(exe, "eof", server.address().port);
        assert.deepEqual(result.lines, ["3 294", "again: read:0 read:0", "ok"]);
        await stop(server);
      }

      // A port that nothing listens on.
      {
        const server = await listen(() => {});
        const port = server.address().port;
        await stop(server);
        const result = await run(exe, "refused", port);
        assert.deepEqual(result.lines, [failure("ConnectionRefused", "Other", errno.ECONNREFUSED)]);
      }

      // A big write waits for room again and again while the peer reads slowly; the peer gets every byte of it.
      {
        let total = 0;
        let sum = 0;
        let finished;
        const everything = new Promise(resolveAll => { finished = resolveAll; });
        const server = await listen(socket => {
          socket.pause();
          socket.on("data", chunk => { total += chunk.length; for (const byte of chunk) sum += byte; if (total === 8388608) finished(); });
          socket.on("error", () => {});
          setTimeout(() => socket.resume(), 200);
        });
        const result = await run(exe, "bigwrite", server.address().port);
        assert.deepEqual(result.lines, ["write: ok ok"]);
        await everything;
        assert.equal(total, 8388608);
        assert.equal(sum, patternSum(8388608));
        await stop(server);
      }

      // A read in pieces of 64 KiB.
      {
        const server = await listen(socket => { socket.on("error", () => {}); socket.end(pattern(3145728)); });
        const result = await run(exe, "bigread", server.address().port);
        assert.deepEqual(result.lines, [`read: 3145728 ${patternSum(3145728)} ok`]);
        await stop(server);
      }

      // Datagrams, a half-close, and every operation on a closed handle: no peer is needed.
      assert.deepEqual((await run(exe, "udp")).lines, [
        "exchange: 31 1", `quiet: ${timedOut}`, `vanished: ${timedOut}`, `truncated: ${failure("Unclassified", "InvalidInput", errno.EMSGSIZE)} then datagram:2 then datagram:0`, `closed: ok ok ${stale}`,
      ]);
      assert.deepEqual((await run(exe, "shutdown")).lines, [`ok read:0 ok read:3 ${failure("ConnectionReset", "Other", errno.EPIPE)}`, "closed"]);
      assert.deepEqual((await run(exe, "stale")).lines, Array(9).fill(stale));
    }
    console.log("Net async: connect, accept, read, write, datagrams, timeouts, cancellation, and closing under Async.block_on passed at -O0 and -O3");
  }

  if (wanted("alloc")) {
    // The error classes by this system's errno, which only the runtime knows (Net.error_kind asks it).
    const classify = join(root, "classify.c");
    writeFileSync(classify, `#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
extern int64_t tsuzuri_net_classify(int32_t);
void *tsuzuri_alloc(int64_t size) { return malloc((size_t)size); }
void tsuzuri_free(void *pointer) { free(pointer); }
int main(int argc, char **argv) {
    for (int index = 1; index + 1 < argc; index += 2) {
        int64_t got = tsuzuri_net_classify((int32_t)atoi(argv[index]));
        if (got != atoi(argv[index + 1])) {
            printf("code %s: %lld, expected %s\\n", argv[index], (long long)got, argv[index + 1]);
            return 1;
        }
    }
    return 0;
}
`);
    const classes = [
      ["ETIMEDOUT", 0], ["ECONNREFUSED", 1], ["ECONNRESET", 2], ["ECONNABORTED", 2], ["EPIPE", 2], ["EADDRINUSE", 3], ["EADDRNOTAVAIL", 4],
      ["ENETUNREACH", 5], ["EHOSTUNREACH", 5], ["ENETDOWN", 5], ["EINVAL", 6], ["EACCES", 6], ["EBADF", 6], ["ENOENT", 6], ["EINTR", 6], ["EMFILE", 6],
    ];
    const pairs = [["0", 6], ["-1", 6], ["100000", 6], ...classes.map(([name, value]) => [String(errno[name]), value])].flat().map(String);
    const classifier = join(root, "classify");
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "src/runtime/net.c", classify, "-o", classifier]);
    execute(classifier, pairs);

    // Allocation tracking plus ASan and UBSan: every owned result and the runtime's own table are freed on every path.
    // A result may be freed by the poller thread of the async operations, so the counters are atomic, and a run ends only
    // when that thread has gone and (for the async cases) every descriptor that the run opened is closed.
    const trackedIr = (name, instrument = "", change = undefined) => {
      const irPath = join(root, `tracked-${name}${instrument}.ll`);
      execute(compiler, ["build", fixture(name, change), "--emit", "llvm", "-o", irPath]);
      let text = readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
      // The functions of the IR carry the attribute that makes a sanitizer look at them.
      if (instrument) text = text.replace(/ nounwind(?=[^{}\n]* \{)/g, ` nounwind sanitize_${instrument}`);
      writeFileSync(irPath, text);
      return irPath;
    };
    const harness = join(root, "tracked-host.c");
    writeFileSync(harness, `#undef NDEBUG
#include <assert.h>
#include <fcntl.h>
#include <netdb.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
extern int32_t tsuzuri_main(void);
static _Atomic uint64_t live;
void *tracked_alloc(uint64_t size) {
    uint64_t *value = malloc((size_t)size + 16);
    assert(value);
    value[0] = size;
    value[1] = UINT64_C(0x51a110ca7e);
    atomic_fetch_add(&live, size);
    return value + 2;
}
void tracked_free(void *pointer) {
    if (!pointer) return;
    uint64_t *value = (uint64_t *)pointer - 2;
    assert(value[1] == UINT64_C(0x51a110ca7e) && atomic_load(&live) >= value[0]);
    atomic_fetch_sub(&live, value[0]);
    value[1] = 0;
    free(value);
}
void *tracked_realloc(void *pointer, uint64_t size) {
    if (!pointer) return tracked_alloc(size);
    uint64_t previous = ((uint64_t *)pointer)[-2];
    void *next = tracked_alloc(size);
    memcpy(next, pointer, (size_t)(previous < size ? previous : size));
    tracked_free(pointer);
    return next;
}
// The runtime's own memory is counted as well: net.c is compiled with these in place of the C library's.
static _Atomic int64_t temporaries;
void *tz_test_malloc(size_t size) { void *value = malloc(size); if (value) atomic_fetch_add(&temporaries, 1); return value; }
void *tz_test_realloc(void *pointer, size_t size) {
    void *value = realloc(pointer, size);
    if (value && !pointer) atomic_fetch_add(&temporaries, 1);
    return value;
}
void tz_test_free(void *pointer) { if (pointer) atomic_fetch_sub(&temporaries, 1); free(pointer); }
// Every call that net.c makes to getaddrinfo is counted, and so is every one that the system answered as a numeric
// address: a host that got past Net.resolve as address text (the checks of std/Net.tz come before the runtime's).
static _Atomic int64_t resolver_calls;
static _Atomic int64_t numeric_hits;
int tz_test_getaddrinfo(const char *node, const char *service, const struct addrinfo *hints, struct addrinfo **result) {
    atomic_fetch_add(&resolver_calls, 1);
    int status = getaddrinfo(node, service, hints, result);
    if (status == 0 && hints != NULL && (hints->ai_flags & AI_NUMERICHOST) != 0) atomic_fetch_add(&numeric_hits, 1);
    return status;
}
static int open_descriptors(void) {
    int count = 0;
    for (int descriptor = 0; descriptor < 1024; descriptor++) {
        if (fcntl(descriptor, F_GETFD) != -1) count++;
    }
    return count;
}
// The poller of the async operations ends soon after the last wait, and frees what it has then.
static void settle(int descriptors) {
    for (int attempt = 0; attempt < 2000; attempt++) {
        if (atomic_load(&temporaries) == 0 && (descriptors < 0 || open_descriptors() == descriptors)) return;
        struct timespec pause = {0, 5 * 1000000L};
        nanosleep(&pause, NULL);
    }
    fprintf(stderr, "runtime memory left: %lld, descriptors %d (expected %d)\\n", (long long)atomic_load(&temporaries), open_descriptors(), descriptors);
    abort();
}
int main(void) {
    const char *text = getenv("TZ_ITERATIONS");
    int iterations = text ? atoi(text) : 1;
    int descriptors = getenv("TZ_CHECK_DESCRIPTORS") ? open_descriptors() : -1;
    for (int index = 0; index < iterations; index++) {
        assert(tsuzuri_main() == 0);
        assert(atomic_load(&live) == 0);
        settle(descriptors);
    }
    if (atomic_load(&numeric_hits) != 0) {
        fprintf(stderr, "the system read %lld hosts as addresses that the checks of Net.resolve let through\\n", (long long)atomic_load(&numeric_hits));
        abort();
    }
    const char *calls = getenv("TZ_RESOLVER_CALLS");
    if (calls && atomic_load(&resolver_calls) != (int64_t)atoi(calls) * iterations) {
        fprintf(stderr, "calls to getaddrinfo: %lld, expected %lld\\n", (long long)atomic_load(&resolver_calls), (long long)atoi(calls) * iterations);
        abort();
    }
    return 0;
}
`);
    const runtime = join(root, "net-tracked.c");
    const original = readFileSync("src/runtime/net.c", "utf8");
    const marker = "#include <arpa/inet.h>";
    assert.ok(original.includes(marker));
    writeFileSync(runtime, original.replace(marker, `#include <stdlib.h>
#include <string.h>
#define malloc tz_test_malloc
#define realloc tz_test_realloc
#define free tz_test_free
#define getaddrinfo tz_test_getaddrinfo
void *tz_test_malloc(size_t);
void *tz_test_realloc(void *, size_t);
void tz_test_free(void *);
${marker}`));
    const iterations = 12;
    const looping = (optimization, ir, sanitizers = sanitized ? "address,undefined" : "") => {
      const executable = join(root, `tracked-${sanitizers.replace(",", "-") || "plain"}-${basename(ir)}${optimization}`);
      execute(clang, [optimization, ...(sanitizers ? [`-fsanitize=${sanitizers}`] : []), "-fno-omit-frame-pointer", "-Wno-override-module", ir, runtime, "src/runtime/os.c", "src/runtime/io.c", "src/runtime/async.c", harness, "-lm", "-pthread", "-o", executable]);
      return executable;
    };
    const irPath = trackedIr("net_sockets");
    const resolving = resolveLiterals();
    const resolveIr = trackedIr("net_resolve", "", resolving.write);
    const sanitizerEnv = { ASAN_OPTIONS: "detect_stack_use_after_return=1", TSAN_OPTIONS: "halt_on_error=1" };
    const track = async (executable, caseName, port, lines, onLine, env = {}) => {
      const result = await run(executable, caseName, port, "127.0.0.1", onLine, { repeat: iterations, env: { TZ_ITERATIONS: String(iterations), ...sanitizerEnv, ...env } });
      assert.equal(result.status, 0, `${caseName}\n${result.stderr}\n${result.stdout.slice(-400)}`);
      assert.equal(result.lines.length, lines * iterations, `${caseName}\n${result.stdout.slice(-400)}`);
      return result;
    };
    for (const optimization of optimizations) {
      const executable = looping(optimization, irPath);
      // T1: an echo server that lives through all the runs.
      {
        const server = await listen(socket => { socket.on("data", data => socket.write(data)); socket.on("error", () => {}); });
        await track(executable, "echo", server.address().port, 5);
        await stop(server);
      }
      // T2: a new client for the port that each run prints.
      {
        const clients = [];
        await track(executable, "server", 0, 7, line => {
          const found = /^port=(\d+)$/.exec(line);
          if (!found) return;
          const client = net.connect({ host: "127.0.0.1", port: Number(found[1]) }, () => client.write("hello\n"));
          client.on("data", () => {});
          client.on("error", () => {});
          clients.push(client);
        });
        await Promise.all(clients.map(closed));
        assert.equal(clients.length, iterations);
      }
      // T7 and T8: datagrams for the port that each run prints.
      {
        const udp = await bindDatagram();
        udp.on("message", () => {});
        await track(executable, "udp", 0, 4, line => {
          const found = /^port=(\d+)$/.exec(line);
          if (found) udp.send(Buffer.alloc(300, 1), Number(found[1]), "127.0.0.1");
        });
        await track(executable, "truncate", 0, 4, line => {
          const found = /^port=(\d+)$/.exec(line);
          if (found) udp.send(Buffer.alloc(100, 7), Number(found[1]), "127.0.0.1", () => udp.send("ok", Number(found[1]), "127.0.0.1"));
        });
        udp.close();
      }
      // The paths that fail, wait, or end: refused, timed out, end of stream, validation, resolution, and a big read.
      {
        const quiet = await listen(socket => { socket.on("error", () => {}); socket.on("data", () => {}); });
        const ending = await listen(socket => { socket.on("error", () => {}); socket.end("abc"); });
        const big = await listen(socket => { socket.on("error", () => {}); socket.end(pattern(3145728)); });
        const gone = await listen(() => {});
        const gonePort = gone.address().port;
        await stop(gone);
        await track(executable, "refused", gonePort, 1);
        await track(executable, "timeout", quiet.address().port, 3);
        await track(executable, "eof", ending.address().port, 4);
        await track(executable, "bigread", big.address().port, 2);
        await track(executable, "validate", quiet.address().port, 13);
        // "localhost" is the only host of this case that asks the system (a probe for an address, then the lookup); the
        // numeric ones, which the strict parser decides, do not. Nor does any of the corpus of address text.
        await track(executable, "resolve", 0, 10, undefined, { TZ_RESOLVER_CALLS: "2" });
        await track(looping(optimization, resolveIr), "resolve", 0, resolving.literalLines.length, undefined, { TZ_RESOLVER_CALLS: "0" });
        await track(executable, "waits", 0, 4);
        await track(executable, "reuse", 0, 3);
        await Promise.all([quiet, ending, big].map(stop));
      }
    }
    console.log(`Net allocation: ${iterations} runs of each case ${sanitized ? "with ASan and UBSan" : "without sanitizers"} at -O0 and -O3 freed every result and the handle table`);

    // The async cases add the poller thread and the mailbox: after every run the thread is gone, its memory is freed, and
    // the descriptors are those of the start. With TSUZURI_TSAN=1 the same cases run under ThreadSanitizer, whose runs
    // need the IR functions to carry the sanitizer attribute as well.
    const asyncCases = async (executable, env) => {
      const tracked = (caseName, port, lines, onLine) => track(executable, caseName, port, lines, onLine, { TZ_CHECK_DESCRIPTORS: "1", ...env });
      const echoing = await listen(socket => { socket.on("data", data => socket.write(data)); socket.on("end", () => socket.end()); socket.on("error", () => {}); });
      await tracked("echo", echoing.address().port, 5);
      await stop(echoing);
      const clients = [];
      await tracked("server", 0, 5, line => {
        const found = /^port=(\d+)$/.exec(line);
        if (!found) return;
        const client = net.connect({ host: "127.0.0.1", port: Number(found[1]) }, () => client.write("hello\n"));
        client.on("data", () => {});
        client.on("error", () => {});
        clients.push(client);
      });
      await Promise.all(clients.map(closed));
      assert.equal(clients.length, iterations);
      const peers = [];
      await tracked("accept_reset", 0, 3, line => {
        const found = /^ports=(\d+) (\d+)$/.exec(line);
        if (found) peers.push(resetThenGood(Number(found[1]), Number(found[2])));
      });
      for (const good of await Promise.all(peers)) good.destroy();
      assert.equal(peers.length, iterations);
      for (const [name, lines] of [["crowd", 1], ["waits", 2], ["cancel", 6], ["close_wait", 3], ["timeouts", 4], ["validate", 10], ["udp", 5], ["shutdown", 2], ["stale", 9]]) {
        await tracked(name, 0, lines);
      }
      const ending = await listen(socket => { socket.on("error", () => {}); socket.end("abc"); });
      await tracked("eof", ending.address().port, 3);
      const big = await listen(socket => { socket.on("error", () => {}); socket.end(pattern(3145728)); });
      await tracked("bigread", big.address().port, 1);
      const gone = await listen(() => {});
      const gonePort = gone.address().port;
      await stop(gone);
      await tracked("refused", gonePort, 1);
      await Promise.all([ending, big].map(stop));
    };
    const asyncIr = trackedIr("net_async");
    for (const optimization of optimizations) await asyncCases(looping(optimization, asyncIr), {});
    console.log(`Net async allocation: ${iterations} runs of each case ${sanitized ? "with ASan and UBSan" : "without sanitizers"} at -O0 and -O3 freed every result, the mailbox, the poller's memory, and every descriptor`);
    if (threadSanitized) {
      const instrumented = trackedIr("net_async", "thread");
      for (const optimization of optimizations) await asyncCases(looping(optimization, instrumented, "thread"), {});
      console.log("Net async threads: the same cases under ThreadSanitizer at -O0 and -O3 reported nothing");
    }
  }

  if (wanted("runtime")) {
    // The poller thread, the connect that a wait owns, closing and cancelling, without the compiler (tests/net_runtime.c):
    // plain, with ASan and UBSan, and (TSUZURI_TSAN=1) with ThreadSanitizer (none of them with TSUZURI_NO_SANITIZERS=1).
    const configurations = [["", []]];
    if (sanitized) configurations.push(["address-undefined", ["-fsanitize=address,undefined"]]);
    if (threadSanitized) configurations.push(["thread", ["-fsanitize=thread"]]);
    for (const [name, flags] of configurations) {
      for (const optimization of ["-O1", "-O3"]) {
        const executable = join(root, `net-runtime-${name}${optimization}`);
        execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", optimization, "-g", "-fno-omit-frame-pointer", ...flags, "-pthread", "tests/net_runtime.c", "src/runtime/net.c", "-o", executable]);
        const result = execute(executable, [], { env: { ...process.env, ASAN_OPTIONS: "detect_stack_use_after_return=1", TSAN_OPTIONS: "halt_on_error=1" } });
        assert.match(result.stdout, /net runtime: waits, timeouts, close, unwatch, connect, cancel, and threads that churn sockets passed/);
      }
    }
    console.log(`Net runtime: the poller, connects that a wait owns, close, cancel, and churn from four threads passed (${configurations.map(([name]) => name || "plain").join(", ")})`);
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}
