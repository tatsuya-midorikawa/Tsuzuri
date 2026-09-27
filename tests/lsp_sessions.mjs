import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, realpathSync, writeFileSync, readFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = realpathSync(mkdtempSync(join(tmpdir(), "tsuzuri-lsp-")));

async function session(encoding) {
  const child = spawn(compiler, ["lsp"], { stdio: ["pipe", "pipe", "pipe"] });
  const messages = [];
  const waiters = [];
  let output = Buffer.alloc(0);
  let errors = "";
  let nextId = 1;
  child.stderr.on("data", (chunk) => { errors += chunk; });
  const exited = new Promise((resolve, reject) => { child.once("exit", (code) => resolve(code)); child.once("error", reject); });
  child.stdout.on("data", (chunk) => {
    output = Buffer.concat([output, chunk]);
    for (;;) {
      const headerEnd = output.indexOf("\r\n\r\n");
      if (headerEnd < 0) break;
      const length = /^Content-Length: (\d+)$/m.exec(output.subarray(0, headerEnd).toString());
      assert.ok(length, "stdout contains only framed messages");
      const end = headerEnd + 4 + Number(length[1]);
      if (output.length < end) break;
      const message = JSON.parse(output.subarray(headerEnd + 4, end).toString());
      output = output.subarray(end);
      const index = waiters.findIndex((waiter) => waiter.predicate(message));
      if (index < 0) messages.push(message);
      else { const [waiter] = waiters.splice(index, 1); clearTimeout(waiter.timer); waiter.resolve(message); }
    }
  });
  function wait(predicate) {
    const index = messages.findIndex(predicate);
    if (index >= 0) return Promise.resolve(messages.splice(index, 1)[0]);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { child.kill(); reject(new Error(`LSP response timeout\n${errors}`)); }, 30_000);
      waiters.push({ predicate, resolve, timer });
    });
  }
  function send(message) {
    const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }));
    child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    child.stdin.write(body);
  }
  function request(method, params = {}) {
    const id = nextId++;
    send({ id, method, params });
    return wait((message) => message.id === id);
  }
  const notify = (method, params) => send({ method, params });
  function position(source, needle) {
    const offset = source.indexOf(needle);
    assert.ok(offset >= 0, needle);
    const prefix = source.slice(0, offset).split("\n");
    return { line: prefix.length - 1, character: encoding === "utf-8" ? Buffer.byteLength(prefix.at(-1)) : prefix.at(-1).length };
  }
  const path = join(root, "Main.tz");
  const uri = pathToFileURL(path).href;
  const extra = join(root, "Extra.tz");
  const extraUri = pathToFileURL(extra).href;
  const original = "fn disk() -> i64 { 7 }";
  writeFileSync(path, original);
  writeFileSync(join(root, "Shapes.tz"), "record Point { x: i64 }");
  mkdirSync(join(root, "Geometry"), { recursive: true });
  const nestedUri = pathToFileURL(join(root, "Geometry/Point.tz")).href;
  writeFileSync(join(root, "Geometry/Point.tz"), "fn nested() -> i64 { 42 }");
  const invalid = "fn bad() -> i64 { let text = \"\u65e5\u{1f600}\"; true }";
  const valid = "def identity :: 'a -> 'a\nfn identity value = value\n"
    + "def read :: i64 -> i64\nfn read number = { let text = \"\u65e5\u{1f600}\"; let result = identity number; assert (text.length == 3); result }\n"
    + "def point :: Shapes.Point -> i64\nfn point value = value.x\n"
    + "fn nested_read() -> i64 { Geometry.Point.nested() }\n";
  try {
    const initialized = await request("initialize", { rootUri: pathToFileURL(root).href, capabilities: encoding === "utf-8" ? { general: { positionEncodings: ["utf-8"] } } : {} });
    assert.equal(initialized.result.capabilities.positionEncoding, encoding);
    assert.equal(initialized.result.capabilities.textDocumentSync.change, 1);
    notify("initialized", {});
    notify("textDocument/didOpen", { textDocument: { uri, languageId: "tsuzuri", version: 1, text: invalid } });
    const diagnosed = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === uri && message.params.version === 1);
    assert.equal(diagnosed.params.diagnostics[0].code, "E1003");
    assert.deepEqual(diagnosed.params.diagnostics[0].range.start, position(invalid, "true"));
    notify("textDocument/didChange", { textDocument: { uri, version: 2 }, contentChanges: [{ text: valid }] });
    const hovered = await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "number; assert") });
    assert.match(hovered.result.contents.value, /number: i64/);
    const cleared = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === uri && message.params.version === 2);
    assert.deepEqual(cleared.params.diagnostics, []);
    const definition = await request("textDocument/definition", { textDocument: { uri }, position: position(valid, "identity number") });
    assert.equal(definition.result.uri, uri);
    assert.deepEqual(definition.result.range.start, { line: 1, character: 3 });
    const record = await request("textDocument/definition", { textDocument: { uri }, position: position(valid, "Shapes.Point") });
    assert.equal(record.result.uri, pathToFileURL(join(root, "Shapes.tz")).href);
    const symbols = await request("textDocument/documentSymbol", { textDocument: { uri } });
    assert.deepEqual(symbols.result.map((symbol) => symbol.name), ["identity", "read", "point", "nested_read"]);
    const nestedDefinition = await request("textDocument/definition", { textDocument: { uri }, position: position(valid, "Geometry.Point.nested") });
    assert.equal(nestedDefinition.result.uri, nestedUri);
    notify("textDocument/didOpen", { textDocument: { uri: nestedUri, languageId: "tsuzuri", version: 1, text: "fn nested() -> i64 { true }" } });
    assert.equal((await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "identity number") })).result, null);
    notify("textDocument/didChange", { textDocument: { uri: nestedUri, version: 2 }, contentChanges: [{ text: "fn nested() -> i64 { 42 }" }] });
    assert.ok((await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "identity number") })).result);
    notify("textDocument/didOpen", { textDocument: { uri: extraUri, languageId: "tsuzuri", version: 1, text: "fn value() -> i64 { 42 }" } });
    const extraSymbols = await request("textDocument/documentSymbol", { textDocument: { uri: extraUri } });
    assert.equal(extraSymbols.result[0].name, "value");
    assert.equal(existsSync(extra), false);
    notify("textDocument/didChange", { textDocument: { uri, version: 3 }, contentChanges: [{ text: invalid }] });
    const stale = await request("textDocument/hover", { textDocument: { uri }, position: position(invalid, "true") });
    assert.equal(stale.result, null);
    const badUri = await request("textDocument/hover", { textDocument: { uri: "file://remote/Main.tz" }, position: { line: 0, character: 0 } });
    assert.equal(badUri.error.code, -32602);
    const badPosition = await request("textDocument/hover", { textDocument: { uri }, position: { line: 1000, character: 0 } });
    assert.equal(badPosition.error.code, -32602);
    notify("$/cancelRequest", { id: nextId });
    assert.equal((await request("textDocument/hover", { textDocument: { uri }, position: { line: 0, character: 0 } })).error.code, -32800);
    notify("textDocument/didClose", { textDocument: { uri: extraUri } });
    const closed = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === extraUri && message.params.version === undefined);
    assert.deepEqual(closed.params.diagnostics, []);
    assert.equal(readFileSync(path, "utf8"), original);
    assert.equal((await request("shutdown")).result, null);
    assert.equal((await request("textDocument/documentSymbol", { textDocument: { uri } })).error.code, -32600);
    notify("exit", {});
    child.stdin.end();
    assert.equal(await exited, 0, errors);
    assert.equal(errors, "");
    console.log(`lsp: ${encoding} session passed`);
  } finally {
    for (const waiter of waiters) clearTimeout(waiter.timer);
    if (child.exitCode === null && child.signalCode === null) child.kill();
  }
}

try { await session("utf-16"); await session("utf-8"); }
finally { rmSync(root, { recursive: true, force: true }); }
