// A JSON-RPC client of `tsuzuri lsp` for the language server's measurements and golden transcripts (PB08).
// Every server message is kept, in order, with the time it arrived (`t`, ms since the server started).
import { spawn } from "node:child_process";
import { pathToFileURL } from "node:url";

export function connect(compiler, { env = process.env } = {}) {
  const started = performance.now();
  const now = () => performance.now() - started;
  const child = spawn(compiler, ["lsp"], { stdio: ["pipe", "pipe", "pipe"], env });
  const messages = [];
  const listeners = [];
  let output = Buffer.alloc(0);
  let stderr = "";
  let nextId = 1;
  let exitInfo = null;
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  child.stdin.on("error", () => {});
  const exited = new Promise((resolve) => {
    child.once("exit", (code, signal) => {
      exitInfo = { code, signal, t: now() };
      for (const listener of listeners.splice(0)) {
        clearTimeout(listener.timer);
        listener.reject(new Error(`the server exited (${code ?? signal}) before the awaited message:\n${stderr}`));
      }
      resolve(exitInfo);
    });
  });
  child.stdout.on("data", (chunk) => {
    output = Buffer.concat([output, chunk]);
    for (;;) {
      const headerEnd = output.indexOf("\r\n\r\n");
      if (headerEnd < 0) return;
      const length = /Content-Length: *(\d+)/i.exec(output.subarray(0, headerEnd).toString());
      if (!length) throw new Error(`malformed header: ${output.subarray(0, headerEnd)}`);
      const end = headerEnd + 4 + Number(length[1]);
      if (output.length < end) return;
      const message = JSON.parse(output.subarray(headerEnd + 4, end).toString());
      output = output.subarray(end);
      Object.defineProperty(message, "t", { value: now(), enumerable: false });
      messages.push(message);
      for (let index = 0; index < listeners.length; index++) {
        if (listeners[index].predicate(message)) {
          const [listener] = listeners.splice(index, 1);
          clearTimeout(listener.timer);
          listener.resolve(message);
          index--;
        }
      }
    }
  });
  const send = (message) => {
    const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }));
    child.stdin.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
  };
  // A message that matches, already received at or after `from`, or the next one.
  const wait = (predicate, { from = 0, timeout = 600_000 } = {}) => {
    for (let index = from; index < messages.length; index++) if (predicate(messages[index])) return Promise.resolve(messages[index]);
    if (exitInfo) return Promise.reject(new Error(`the server has exited (${JSON.stringify(exitInfo)}):\n${stderr}`));
    return new Promise((resolve, reject) => {
      const listener = { predicate, resolve, reject };
      listener.timer = setTimeout(() => {
        listeners.splice(listeners.indexOf(listener), 1);
        reject(new Error(`no awaited message within ${timeout} ms:\n${stderr}`));
      }, timeout);
      listeners.push(listener);
    });
  };
  // `sent` is the time just before the request was written; `response` resolves to the response.
  const request = (method, params = {}) => {
    const id = nextId++;
    const sent = now();
    send({ id, method, params });
    return { id, sent, response: wait((message) => message.id === id && !message.method) };
  };
  const notify = (method, params) => send({ method, params });
  const initialize = async (root) => {
    const uri = pathToFileURL(root).href;
    const { response } = request("initialize", {
      processId: process.pid, rootUri: uri, workspaceFolders: [{ uri, name: "root" }], capabilities: {},
    });
    const result = await response;
    if (result.error) throw new Error(`initialize failed: ${JSON.stringify(result.error)}`);
    notify("initialized", {});
    return result;
  };
  const shutdown = async () => {
    if (exitInfo) return exitInfo;
    await Promise.race([request("shutdown", null).response, new Promise((resolve) => setTimeout(resolve, 120_000).unref())]);
    notify("exit", null);
    const timer = setTimeout(() => child.kill("SIGKILL"), 10_000);
    const info = await exited;
    clearTimeout(timer);
    return info;
  };
  return {
    child, messages, now, wait, request, notify, initialize, shutdown, exited,
    get stderr() { return stderr; }, get exitInfo() { return exitInfo; },
  };
}

export const uriOf = (path) => pathToFileURL(path).href;

// The `textDocument/publishDiagnostics` of `uri`, of document version `version` when it is given.
export const isDiagnostics = (uri, version) => (message) => message.method === "textDocument/publishDiagnostics"
  && message.params.uri === uri && (version === undefined || message.params.version === version);

// The LSP position (UTF-16 code units) of a JavaScript string offset.
export function position(text, offset) {
  const lines = text.slice(0, offset).split("\n");
  return { line: lines.length - 1, character: lines.at(-1).length };
}

export const endPosition = (text) => position(text, text.length);

// The median of `values` (the lower middle for an even count, like the prototypes); min and max.
export function stats(values) {
  const sorted = [...values].sort((left, right) => left - right);
  return { median: sorted[(sorted.length - 1) >> 1], min: sorted[0], max: sorted.at(-1) };
}
