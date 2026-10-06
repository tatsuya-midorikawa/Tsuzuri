import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, realpathSync, writeFileSync, readFileSync, existsSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = realpathSync(mkdtempSync(join(tmpdir(), "tsuzuri-lsp-")));

function assertFileUri(uri, path) {
  const actual = statSync(fileURLToPath(uri), { bigint: true });
  const expected = statSync(path, { bigint: true });
  assert.deepEqual([actual.dev, actual.ino], [expected.dev, expected.ino]);
}

function connect(encoding) {
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
  function wait(predicate, description) {
    const index = messages.findIndex(predicate);
    if (index >= 0) return Promise.resolve(messages.splice(index, 1)[0]);
    return new Promise((resolve, reject) => {
      const timeout = process.platform === "win32" ? 120_000 : 30_000;
      const timer = setTimeout(() => {
        child.kill();
        reject(new Error(`Timed out waiting for ${description}\nReceived: ${JSON.stringify(messages)}\n${errors}`));
      }, timeout);
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
    return wait((message) => message.id === id, `${method} response (id ${id})`);
  }
  const notify = (method, params) => send({ method, params });
  function position(source, needle) {
    const offset = source.indexOf(needle);
    assert.ok(offset >= 0, needle);
    const prefix = source.slice(0, offset).split("\n");
    return { line: prefix.length - 1, character: encoding === "utf-8" ? Buffer.byteLength(prefix.at(-1)) : prefix.at(-1).length };
  }
  return { child, exited, waiters, wait, request, notify, position, get errors() { return errors; }, get nextId() { return nextId; } };
}

async function session(encoding) {
  const connection = connect(encoding);
  const { child, exited, waiters, wait, request, notify, position } = connection;
  const path = join(root, "Main.tz");
  const mainUri = pathToFileURL(path).href;
  const uri = process.platform === "win32"
    ? mainUri.replace(/^file:\/\/\/([A-Za-z]):/, (_, drive) => `file:///${drive === drive.toLowerCase() ? drive.toUpperCase() : drive.toLowerCase()}:`)
    : mainUri;
  const extra = join(root, "Extra.tz");
  const extraUri = pathToFileURL(extra).href;
  const original = "fn disk() -> i64 { 7 }";
  writeFileSync(path, original);
  writeFileSync(join(root, "Shapes.tz"), "record Point { x: i64 }");
  mkdirSync(join(root, "Geometry"), { recursive: true });
  const nestedUri = pathToFileURL(join(root, "Geometry/Point.tz")).href;
  writeFileSync(join(root, "Geometry/Point.tz"), "fn nested() -> i64 { 42 }");
  const invalid = "fn bad() -> i64 { let text = \"\u65e5\u{1f600}\"; true }";
  const valid = "/// Keeps the value.\ndef identity :: 'a -> 'a\nfn identity value = value\n"
    + "def read :: i64 -> i64\nfn read number = { let text = \"\u65e5\u{1f600}\"; let result = identity number; assert (text.length == 3); result }\n"
    + "def point :: Shapes.Point -> i64\nfn point value = value.x\n"
    + "fn nested_read() -> i64 { Geometry::Point.nested() }\n";
  try {
    const initialized = await request("initialize", { rootUri: pathToFileURL(root).href, capabilities: encoding === "utf-8" ? { general: { positionEncodings: ["utf-8"] } } : {} });
    assert.equal(initialized.result.capabilities.positionEncoding, encoding);
    assert.equal(initialized.result.capabilities.textDocumentSync.change, 1);
    notify("initialized", {});
    notify("textDocument/didOpen", { textDocument: { uri, languageId: "tsuzuri", version: 1, text: invalid } });
    const diagnosed = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === uri && message.params.version === 1, "diagnostics for Main.tz version 1");
    assert.equal(diagnosed.params.diagnostics[0].code, "E1003");
    assert.deepEqual(diagnosed.params.diagnostics[0].range.start, position(invalid, "true"));
    notify("textDocument/didChange", { textDocument: { uri, version: 2 }, contentChanges: [{ text: valid }] });
    const hovered = await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "number; assert") });
    assert.match(hovered.result.contents.value, /number: i64/);
    const documented = await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "identity number") });
    assert.match(documented.result.contents.value, /Keeps the value\./);
    const cleared = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === uri && message.params.version === 2, "diagnostics for Main.tz version 2");
    assert.deepEqual(cleared.params.diagnostics, []);
    const definition = await request("textDocument/definition", { textDocument: { uri }, position: position(valid, "identity number") });
    assert.equal(definition.result.uri, uri);
    assert.deepEqual(definition.result.range.start, position(valid, "identity value"));
    const record = await request("textDocument/definition", { textDocument: { uri }, position: position(valid, "Shapes.Point") });
    assertFileUri(record.result.uri, join(root, "Shapes.tz"));
    const symbols = await request("textDocument/documentSymbol", { textDocument: { uri } });
    assert.deepEqual(symbols.result.map((symbol) => symbol.name), ["identity", "read", "point", "nested_read"]);
    const nestedDefinition = await request("textDocument/definition", { textDocument: { uri }, position: position(valid, "Geometry::Point.nested") });
    assertFileUri(nestedDefinition.result.uri, join(root, "Geometry/Point.tz"));
    const shapesUri = pathToFileURL(join(root, "Shapes.tz")).href;
    writeFileSync(join(root, "Shapes.tz"), "record Point { x: bool }");
    notify("workspace/didChangeWatchedFiles", { changes: [{ uri: shapesUri, type: 2 }] });
    assert.equal((await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "identity number") })).result, null);
    writeFileSync(join(root, "Shapes.tz"), "record Point { x: i64 }");
    notify("workspace/didChangeWatchedFiles", { changes: [{ uri: shapesUri, type: 2 }] });
    assert.ok((await request("textDocument/hover", { textDocument: { uri }, position: position(valid, "identity number") })).result);
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
    notify("$/cancelRequest", { id: connection.nextId });
    assert.equal((await request("textDocument/hover", { textDocument: { uri }, position: { line: 0, character: 0 } })).error.code, -32800);
    notify("textDocument/didClose", { textDocument: { uri: extraUri } });
    const closed = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === extraUri && message.params.version === undefined, "diagnostics for closed Extra.tz");
    assert.deepEqual(closed.params.diagnostics, []);
    assert.equal(readFileSync(path, "utf8"), original);
    assert.equal((await request("shutdown")).result, null);
    assert.equal((await request("textDocument/documentSymbol", { textDocument: { uri } })).error.code, -32600);
    notify("exit", {});
    child.stdin.end();
    assert.equal(await exited, 0, connection.errors);
    assert.equal(connection.errors, "");
    console.log(`lsp: ${encoding} session passed`);
  } finally {
    for (const waiter of waiters) clearTimeout(waiter.timer);
    if (child.exitCode === null && child.signalCode === null) child.kill();
  }
}

const range = (line, start, end) => ({ start: { line, character: start }, end: { line, character: end } });

/** Decodes relative semantic tokens into absolute [line, start, length, type, modifiers] rows. */
function decode(data) {
  const rows = [];
  let line = 0;
  let start = 0;
  for (let index = 0; index < data.length; index += 5) {
    line += data[index];
    start = data[index] === 0 ? start + data[index + 1] : data[index + 1];
    rows.push([line, start, ...data.slice(index + 2, index + 5)]);
  }
  return rows;
}

async function features(encoding) {
  const connection = connect(encoding);
  const { child, exited, waiters, wait, request, notify, position } = connection;
  const directory = realpathSync(mkdtempSync(join(tmpdir(), "tsuzuri-lsp-features-")));
  const main = "def read :: i64 -> i64\nfn read number = { let text = \"\u65e5\u{1f600}\"; let result = number + text.length; result }\n"
    + "def point :: Shapes.Point -> i64\nfn point value = value.x\ndef twice :: i64 -> i64\nfn twice n = read (read n)\n";
  const shapes = "record Point { x: i64 }\n";
  const warn = "fn warn() -> i64 { let spare = 1; 2 }\n";
  const files = { "Main.tz": main, "Shapes.tz": shapes, "Warn.tz": warn };
  const uris = {};
  const shift = encoding === "utf-8" ? 4 : 0;
  try {
    for (const [name, text] of Object.entries(files)) {
      writeFileSync(join(directory, name), text);
      uris[name] = pathToFileURL(join(directory, name)).href;
    }
    const initialized = await request("initialize", { rootUri: pathToFileURL(directory).href, capabilities: encoding === "utf-8" ? { general: { positionEncodings: ["utf-8"] } } : {} });
    assert.equal(initialized.result.capabilities.renameProvider.prepareProvider, true);
    notify("initialized", {});
    for (const [name, text] of Object.entries(files)) {
      notify("textDocument/didOpen", { textDocument: { uri: uris[name], languageId: "tsuzuri", version: 1, text } });
    }
    const document = (name) => ({ uri: uris[name] });
    const references = await request("textDocument/references", { textDocument: document("Main.tz"), position: position(main, "number ="), context: { includeDeclaration: true } });
    assert.deepEqual(references.result, [{ uri: uris["Main.tz"], range: range(1, 8, 14) }, { uri: uris["Main.tz"], range: range(1, 50 + shift, 56 + shift) }]);
    const highlights = await request("textDocument/documentHighlight", { textDocument: document("Main.tz"), position: position(main, "result =") });
    assert.deepEqual(highlights.result, [{ range: range(1, 41 + shift, 47 + shift), kind: 3 }, { range: range(1, 72 + shift, 78 + shift), kind: 2 }]);
    const renamed = await request("textDocument/rename", { textDocument: document("Main.tz"), position: position(main, "result ="), newName: "total" });
    assert.deepEqual(renamed.result, { changes: { [uris["Main.tz"]]: [{ range: range(1, 41 + shift, 47 + shift), newText: "total" }, { range: range(1, 72 + shift, 78 + shift), newText: "total" }] } });
    const field = await request("textDocument/rename", { textDocument: document("Shapes.tz"), position: position(shapes, "x:"), newName: "y" });
    assert.deepEqual(field.result, { changes: { [uris["Main.tz"]]: [{ range: range(3, 23, 24), newText: "y" }], [uris["Shapes.tz"]]: [{ range: range(0, 15, 16), newText: "y" }] } });
    notify("textDocument/didChange", { textDocument: { uri: uris["Main.tz"], version: 2 }, contentChanges: [{ text: main.replace("= value.x", "= value.") }] });
    const completed = await request("textDocument/completion", { textDocument: document("Main.tz"), position: { line: 3, character: 23 } });
    assert.deepEqual(completed.result, { isIncomplete: false, items: [{ label: "x", kind: 5, detail: "x: i64", sortText: "2_x" }] });
    notify("textDocument/didChange", { textDocument: { uri: uris["Main.tz"], version: 3 }, contentChanges: [{ text: main }] });
    const signature = await request("textDocument/signatureHelp", { textDocument: document("Main.tz"), position: position(main, "n)") });
    assert.deepEqual(signature.result, { signatures: [{ label: "read (number: i64) -> i64", parameters: [{ label: "number: i64" }] }], activeSignature: 0, activeParameter: 0 });
    const shapeTokens = await request("textDocument/semanticTokens/full", { textDocument: document("Shapes.tz") });
    assert.deepEqual(shapeTokens.result, { data: [0, 7, 5, 1, 1, 0, 8, 1, 4, 1] });
    const mainTokens = await request("textDocument/semanticTokens/full", { textDocument: document("Main.tz") });
    assert.deepEqual(decode(mainTokens.result.data).filter((row) => row[0] === 1), [
      [1, 3, 4, 8, 1], [1, 8, 6, 10, 1], [1, 23, 4, 9, 1], [1, 41 + shift, 6, 9, 1],
      [1, 50 + shift, 6, 10, 0], [1, 59 + shift, 4, 9, 0], [1, 72 + shift, 6, 9, 0],
    ]);
    const published = await wait((message) => message.method === "textDocument/publishDiagnostics" && message.params.uri === uris["Warn.tz"] && message.params.diagnostics.length > 0, "W1001 for Warn.tz");
    const actions = await request("textDocument/codeAction", { textDocument: document("Warn.tz"), range: range(0, 0, 37), context: { diagnostics: [], only: ["quickfix"] } });
    assert.equal(actions.result.length, 1);
    assert.equal(actions.result[0].title, "Prefix 'spare' with '_'");
    assert.deepEqual(actions.result[0].diagnostics, published.params.diagnostics);
    assert.deepEqual(actions.result[0].edit, { changes: { [uris["Warn.tz"]]: [{ range: range(0, 23, 28), newText: "_spare" }] } });
    const formatted = await request("textDocument/formatting", { textDocument: document("Warn.tz"), options: { tabSize: 4, insertSpaces: true } });
    assert.deepEqual(formatted.result, []);
    for (const [name, text] of Object.entries(files)) assert.equal(readFileSync(join(directory, name), "utf8"), text);
    assert.equal((await request("shutdown")).result, null);
    notify("exit", {});
    child.stdin.end();
    assert.equal(await exited, 0, connection.errors);
    assert.equal(connection.errors, "");
    console.log(`lsp: ${encoding} features passed`);
  } finally {
    for (const waiter of waiters) clearTimeout(waiter.timer);
    if (child.exitCode === null && child.signalCode === null) child.kill();
    rmSync(directory, { recursive: true, force: true });
  }
}

try { await session("utf-16"); await session("utf-8"); await features("utf-16"); await features("utf-8"); }
finally { rmSync(root, { recursive: true, force: true }); }
