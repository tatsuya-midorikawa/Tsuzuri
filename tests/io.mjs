import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { closeSync, mkdtempSync, openSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-io-"));
const source = join(root, "Main.tz");
function execute(program, args, options = {}, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024, ...options });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function host(module, lines = [], { readFailure = false, writeFailure = false, invalid } = {}) {
  let instance;
  let reads = 0;
  const writes = [[], []];
  instance = new WebAssembly.Instance(module, { tsuzuri_io: {
    read_line(out) {
      const line = lines[reads++];
      const bytes = line === undefined ? Buffer.alloc(0) : Buffer.from(line);
      const pointer = bytes.length ? instance.exports.tsuzuri_alloc(BigInt(bytes.length)) : 0;
      new Uint8Array(instance.exports.memory.buffer, pointer, bytes.length).set(bytes);
      const view = new DataView(instance.exports.memory.buffer);
      view.setUint32(out, pointer, true);
      view.setBigInt64(out + 8, BigInt(bytes.length), true);
      if (invalid) return invalid(view, out);
      return readFailure ? 2 : line === undefined ? 1 : 0;
    },
    write(destination, pointer, length) {
      assert.ok(destination === 1 || destination === 2);
      if (writeFailure && destination === 1) return 1;
      writes[destination - 1].push(Buffer.from(new Uint8Array(instance.exports.memory.buffer, pointer, Number(length))));
      return 0;
    },
  } });
  return {
    instance,
    get reads() { return reads; },
    get stdout() { return Buffer.concat(writes[0]).toString(); },
    get stderr() { return Buffer.concat(writes[1]).toString(); },
  };
}

function interactive(args) {
  return new Promise((resolveChild, reject) => {
    const child = spawn(compiler, args, { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    let answered = false;
    const timer = setTimeout(() => {
      child.kill();
      reject(new Error("IO prompt was not visible before reading stdin"));
    }, 30_000);
    child.stdout.on("data", chunk => {
      stdout += chunk;
      if (!answered && stdout.includes("Name: ")) {
        answered = true;
        child.stdin.end("interactive\n");
      }
    });
    child.stderr.on("data", chunk => { stderr += chunk; });
    child.once("error", error => { clearTimeout(timer); reject(error); });
    child.once("close", code => {
      clearTimeout(timer);
      try {
        assert.equal(code, 0, stderr);
        assert.ok(answered);
        assert.equal(stdout, "Name: interactive\n");
        assert.equal(stderr, "note\n");
        resolveChild();
      } catch (error) { reject(error); }
    });
  });
}

try {
  writeFileSync(source, `let _unused = IO.write_line "must not run"
let _unread = IO.read_line ()
IO {
    do! IO.write "Name: "
    let! line = IO.read_line ()
    do! IO.write_line (Option.default_value "<eof>" line)
    do! IO.write_error_line "note"
}
`);
  const input = "hello\0\u65e5\u672c\ud83d\ude00\r\n";
  const expected = `Name: ${input.slice(0, -2)}\n`;
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(root, `io${optimization}`);
    execute(compiler, ["build", source, optimization, "-o", executable]);
    const native = execute(executable, [], { input });
    assert.equal(native.stdout, expected);
    assert.equal(native.stderr, "note\n");
    for (const [input, line] of [
      ["", "<eof>"], ["\n", ""], ["\r\n", ""], ["tail", "tail"], ["tail\r", "tail\r"],
      ["x".repeat(131_072) + "\u65e5\ud83d\ude00\n", "x".repeat(131_072) + "\u65e5\ud83d\ude00"],
    ]) {
      assert.equal(execute(executable, [], { input }).stdout, `Name: ${line}\n`);
    }
    for (const extra of [[], ["--json"]]) {
      const cli = execute(compiler, ["run", source, optimization, ...extra], { input });
      assert.equal(cli.stdout, expected);
      assert.equal(cli.stderr, "note\n");
    }
    const wasm = join(root, `io${optimization}.wasm`);
    execute(compiler, ["build", source, "--target", "wasm32", optimization, "--trap-info", "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module).map(({ module, name }) => `${module}.${name}`).sort(), ["tsuzuri_io.read_line", "tsuzuri_io.write"]);
    assert.throws(() => new WebAssembly.Instance(module));
    const wasmHost = host(module, Array(1000).fill(input.slice(0, -2)));
    assert.equal(wasmHost.reads, 0);
    assert.equal(wasmHost.stdout + wasmHost.stderr, "");
    assert.equal(wasmHost.instance.exports.tsuzuri_main(), 0);
    assert.equal(wasmHost.reads, 1);
    assert.equal(wasmHost.stdout, expected);
    assert.equal(wasmHost.stderr, "note\n");
    const memorySize = wasmHost.instance.exports.memory.buffer.byteLength;
    for (let index = 1; index < 1000; index++) assert.equal(wasmHost.instance.exports.tsuzuri_main(), 0);
    assert.equal(wasmHost.instance.exports.memory.buffer.byteLength, memorySize);
    for (const invalid of [
      () => -1,
      () => 3,
      (view, out) => { view.setBigInt64(out + 8, -1n, true); return 0; },
      (view, out) => { view.setBigInt64(out + 8, 1n, true); return 0; },
      (view, out) => { view.setUint32(out, view.byteLength - 1, true); view.setBigInt64(out + 8, 8n, true); return 0; },
    ]) {
      const bad = host(module, [], { invalid });
      assert.throws(() => bad.instance.exports.tsuzuri_main(), WebAssembly.RuntimeError);
    }
    console.log(`IO: native/WASM ${optimization} stdin, stdout, stderr, cold actions and CLI passed`);
  }
  await interactive(["run", source, "-O0"]);
  await interactive(["run", source, "-O3", "--json"]);

  const header = join(root, "io.h");
  const harness = join(root, "host.c");
  writeFileSync(harness, '#include "io.h"\nint main(void) { return tsuzuri_main(); }\n');
  execute(compiler, ["build", source, "--emit", "header", "-o", header]);
  assert.match(readFileSync(header, "utf8"), /int32_t tsuzuri_main\(void\);/);
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(root, `io${optimization}.o`);
    const linked = join(root, `linked${optimization}`);
    execute(compiler, ["build", source, "--emit", "object", optimization, "-o", object]);
    execute(clang, [optimization, object, harness, "-lm", "-o", linked]);
    assert.equal(execute(linked, [], { input }).stdout, expected);
  }

  writeFileSync(harness, `#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "io.h"
static uint64_t live;
void *tracked_alloc(uint64_t size) {
    uint64_t *value = malloc((size_t)size + 16);
    assert(value);
    value[0] = size;
    value[1] = UINT64_C(0x51a110ca7e);
    live += size;
    return value + 2;
}
void tracked_free(void *pointer) {
    if (!pointer) return;
    uint64_t *value = (uint64_t *)pointer - 2;
    assert(value[1] == UINT64_C(0x51a110ca7e) && live >= value[0]);
    live -= value[0];
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
int main(void) {
    for (int index = 0; index < 256; index++) {
        assert(tsuzuri_main() == 0);
        assert(live == 0);
    }
    return 0;
}
`);
  const irPath = join(root, "tracked.ll");
  execute(compiler, ["build", source, "--emit", "llvm", "-o", irPath]);
  writeFileSync(irPath, readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc"));
  for (const optimization of ["-O0", "-O3"]) {
    const tracked = join(root, `tracked${optimization}`);
    execute(clang, [optimization, "-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-Wno-override-module", irPath, "src/runtime/io.c", harness, "-lm", "-o", tracked]);
    assert.equal(execute(tracked, [], { input: input.repeat(256) }).stdout, expected.repeat(256));
  }

  writeFileSync(source, `IO {
    for _index in [0, 1, 2, 3, 4] do {
        let! line = IO.read_line ()
        do! IO.write_line (Option.default_value "<eof>" line)
    }
}
`);
  for (const optimization of ["-O0", "-O3"]) {
    assert.equal(execute(compiler, ["run", source, optimization], { input: "first\r\n\nlast" }).stdout, "first\n\nlast\n<eof>\n<eof>\n");
  }

  writeFileSync(source, `IO {
    let! input = IO.try_read_line ()
    do! IO.write_error_line (match input with
        | Result.Ok Option.None -> "eof"
        | Result.Ok (Option.Some text) -> "line:" + text
        | Result.Error IO.InvalidEncoding -> "encoding"
        | Result.Error _ -> "read-failed")
    let! invalid = IO.try_write "\\uD800"
    do! IO.write_error_line (match invalid with | Result.Error IO.InvalidEncoding -> "encoding" | _ -> "unexpected")
    let! output = IO.try_write "ok"
    do! IO.write_error_line (match output with | Result.Error IO.WriteFailed -> "write-failed" | _ -> "written")
}
`);
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(root, `errors${optimization}`);
    execute(compiler, ["build", source, optimization, "-o", executable]);
    for (const bytes of [Buffer.from([0x80, 10]), Buffer.from([0xc0, 0xaf, 10]), Buffer.from([0xed, 0xa0, 0x80, 10]), Buffer.from([0xf0, 0x90])]) {
      const result = execute(executable, [], { input: bytes });
      assert.equal(result.stdout, "ok");
      assert.equal(result.stderr, "encoding\nencoding\nwritten\n");
    }
    assert.equal(execute(executable, [], { input: "" }).stderr, "eof\nencoding\nwritten\n");
    const writeOnly = openSync(join(root, "write-only"), "w");
    const readOnly = openSync(source, "r");
    try {
      const result = execute(executable, [], { stdio: [writeOnly, readOnly, "pipe"] });
      assert.equal(result.stderr, "read-failed\nencoding\nwrite-failed\n");
    } finally {
      closeSync(writeOnly);
      closeSync(readOnly);
    }
    const wasm = join(root, `errors${optimization}.wasm`);
    execute(compiler, ["build", source, "--target", "wasm32", optimization, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    for (const [lines, options, expected] of [
      [[], {}, "eof\nencoding\nwritten\n"],
      [[Buffer.from([0x80])], {}, "encoding\nencoding\nwritten\n"],
      [[], { readFailure: true, writeFailure: true }, "read-failed\nencoding\nwrite-failed\n"],
    ]) {
      const result = host(module, lines, options);
      assert.equal(result.instance.exports.tsuzuri_main(), 0);
      assert.equal(result.stderr, expected);
    }
  }

  writeFileSync(source, `def announce :: i64 -> IO<i64>
fn announce value = IO { do! IO.write value; return value + 1 }
IO {
    let! first = IO.bind (IO.pure 2) announce
    let! second = IO.bind (announce 3) IO.pure
    let! third = IO.map (value -> value + 1) (IO.pure 3)
    for number in [first, second, third] do do! IO.write number
    let! left = IO.pure 5
    and! right = IO.pure 6
    do! IO.write_line (left + right)
}
`);
  for (const optimization of ["-O0", "-O3"]) {
    assert.equal(execute(compiler, ["run", source, optimization]).stdout, "2334411\n");
    const wasm = join(root, `bind${optimization}.wasm`);
    execute(compiler, ["build", source, "--target", "wasm32", optimization, "-o", wasm]);
    const result = host(new WebAssembly.Module(readFileSync(wasm)));
    assert.equal(result.instance.exports.tsuzuri_main(), 0);
    assert.equal(result.stdout, "2334411\n");
  }
  writeFileSync(source, "IO { let! line = IO.read_line (); do! IO.write_line (Option.default_value \"\" line) }\n");
  const trapped = execute(compiler, ["run", source, "--json"], { input: Buffer.from([0xff, 10]) }, false);
  assert.equal(trapped.status, 1);
  assert.equal(JSON.parse(trapped.stderr.trim()).code, "E2005");

  writeFileSync(join(root, "Gate.tc"), `union Value<'a> = Stop of string | Next of 'a
def Return :: 'a -> Value<'a>
fn Return value = Next value
def Bind :: Value<'a> -> ('a -> Value<'b>) -> Value<'b>
fn Bind value next = match value with | Stop message -> Stop message | Next payload -> next payload
def Zero :: Value<unit>
fn Zero = Next ()
def Delay :: (unit -> Value<'a>) -> (unit -> Value<'a>)
fn Delay body = body
def Run :: (unit -> Value<'a>) -> Value<'a>
fn Run body = body ()
def forwarded :: Option<i64> -> Option<i64>
fn forwarded source =
  let! value = source
  return value + 1
`);
  writeFileSync(join(root, "Computed.tt"), `class Computed<'a> {
  def selected :: 'a -> Option<i64>
  def defaulted :: 'a -> Option<i64>
  fn defaulted _input =
    let! value = Some 41
    return value + 1
}
`);
  writeFileSync(join(root, "Helpers.tz"), `private def advance :: i64 -> Option<i64>
fn advance input =
  let! value = Some input
  return value + 1
def identity :: Capture<'a> => Option<'a> -> Option<'a>
fn identity source =
  let! value = source
  return value
def curried :: i64 -> Option<i64> -> Option<i64>
fn curried offset = source ->
  let! value = source
  return value + offset
def implemented_with_let :: i64 -> Option<i64>
let implemented_with_let = input ->
  let! value = Some input
  return value + 1
def rec repeated :: i64 -> Option<i64>
fn rec repeated count =
  let! value = Some count
  if value == 0 then return 42
  else return! repeated (value - 1)
def echo :: Capture<'a> => IO<Option<'a>> -> IO<Option<'a>>
fn echo source =
  let! option = source
  let! value = option
  return value
instance Computed<i64> {
  fn selected input =
    let! value = Some input
    return value + 1
}
def functions :: i64
fn functions =
  let transform: i64 -> Option<i64> = \\input ->
    let! value = Some input
    return value + 1
  let add_one = curried 1
  Option.get (advance 41) + Option.get (identity (Some 42)) +
    Option.get (add_one (Some 41)) + Option.get (implemented_with_let 41) +
    Option.get (repeated 2) + Option.get (transform 41) +
    Option.get (Computed.selected 41i64) + Option.get (Computed.defaulted 0i64) +
    Option.get (Gate.forwarded (Some 41))
`);
  const implicitCases = [
    ["read", `fn main =
    let! line = IO.read_line ()
    let! value = line
    do! IO.write_line value
    do! IO.write_line "done"
`, "hello\ndone\n", ""],
    ["result", `def gather :: IO<Result<Option<string>, IO.Error>>
fn gather =
    let! result = IO.try_read_line ()
    let! option = result
    let! text = option
    return text
fn main =
    let! result = gather()
    do! IO.write_line (match result with
        | Ok (Some text) -> text
        | Ok None -> "eof"
        | Error _ -> "failed")
`, "hello\n", "eof\n"],
    ["custom", `def gather :: IO<Gate.Value<Option<unit>>>
fn gather =
    let! line = IO.read_line ()
    let source = if Option.is_none (ref line) then Gate.Stop "stopped" else Gate.Next line
    let! option = source
    let! text = option
    do! IO.write_line text
    return ()
fn main =
    let! result = gather()
    do! IO.write_line (match result with | Gate.Stop message -> message | Gate.Next _ -> "done")
`, "hello\ndone\n", "stopped\n"],
    ["drop", `fn main =
    let! line = IO.read_line ()
    let! text = line
    return new [text]
`, "", ""],
    ["functions", `fn main =
    let! line = IO.read_line ()
    let! option = Helpers.echo (IO.pure line)
    let! text = option
    do! IO.write_line text
    do! IO.write_line (Helpers.functions())
  `, "hello\n378\n", ""],
  ];
  for (const [name, text, successOutput, eofOutput] of implicitCases) {
    writeFileSync(source, text);
    for (const optimization of ["-O0", "-O3"]) {
      const executable = join(root, `implicit-${name}${optimization}`);
      execute(compiler, ["build", source, optimization, "-o", executable]);
      assert.equal(execute(executable, [], { input: "hello\n" }).stdout, successOutput, name);
      assert.equal(execute(executable, [], { input: "" }).stdout, eofOutput, `${name} EOF`);
      const wasm = `${executable}.wasm`;
      execute(compiler, ["build", source, "--target", "wasm32", optimization, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      for (const [lines, expectedOutput] of [[["hello"], successOutput], [[], eofOutput]]) {
        const result = host(module, lines);
        assert.equal(result.reads, 0);
        assert.equal(result.instance.exports.tsuzuri_main(), 0);
        assert.equal(result.reads, 1);
        assert.equal(result.stdout, expectedOutput, `${name} WASM ${optimization}`);
      }
      if (name === "result") {
        assert.equal(execute(executable, [], { input: Buffer.from([0xff, 10]) }).stdout, "failed\n");
        const failed = host(module, [], { readFailure: true });
        assert.equal(failed.instance.exports.tsuzuri_main(), 0);
        assert.equal(failed.stdout, "failed\n");
      }
      if (name === "drop") {
        execute(compiler, ["build", source, "--emit", "llvm", "-o", irPath]);
        writeFileSync(irPath, readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc"));
        const tracked = join(root, `implicit-tracked${optimization}`);
        execute(clang, [optimization, "-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-Wno-override-module", irPath, "src/runtime/io.c", harness, "-lm", "-o", tracked]);
        execute(tracked, [], { input: "hello\n".repeat(256) });
        execute(tracked, [], { input: "" });
      }
    }
  }
  console.log("Implicit computations: IO/Option/Result/custom builders, short circuiting, preserved errors and owned results passed on native/WASM -O0/-O3");
  console.log("IO: EOF/CRLF/long lines, encoding/errors, ABI guards, composition, interactive prompts, object linking and ASan/UBSan zero-live allocations passed");
} finally {
  rmSync(root, { recursive: true, force: true });
}
