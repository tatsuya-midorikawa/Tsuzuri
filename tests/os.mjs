import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
  chmodSync, createWriteStream, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, realpathSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync,
} from "node:fs";
import os, { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// E2E for the standard File, Dir, Env, Time, Random, Path, and Os modules (E08). Native programs run
// as real executables in per-case directories; expectations come from Node's fs and os modules, and the
// PCG reference is an independent BigInt implementation checked against the published pcg-c-basic output.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const root = mkdtempSync(join(tmpdir(), "tsuzuri-os-"));
const errno = os.constants.errno;
const optimizations = ["-O0", "-O3"];

function execute(program, args, options = {}, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024, ...options });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

// Helpers every program shares. Matching on results lives here, in pure functions, because an
// IO block's own loops and matches are computations.
const prelude = `
def unit_text :: Result<unit, Os.Error> -> string
fn unit_text result =
    match result with
    | Result.Ok _ -> "ok"
    | Result.Error error -> Os.message (ref error)

def text_text :: Result<string, Os.Error> -> string
fn text_text result =
    match result with
    | Result.Ok text -> "ok:" + text
    | Result.Error error -> Os.message (ref error)

def digits :: ref [ubyte] -> string
fn digits bytes =
    let mut out = ""
    let mut index = 0
    while index < bytes.length do
        if index > 0 then out = out + ","
        out = out + to_string (bytes[index] as i64)
        index = index + 1
    out

def bytes_text :: Result<[ubyte], Os.Error> -> string
fn bytes_text result =
    match result with
    | Result.Ok bytes -> "ok:" + digits (ref bytes)
    | Result.Error error -> Os.message (ref error)

def names_text :: Result<[string], Os.Error> -> string
fn names_text result =
    match result with
    | Result.Ok names ->
        let separator = "|"
        "ok:" + to_string names.length + ":" + String.join (ref separator) (ref names)
    | Result.Error error -> Os.message (ref error)

def option_text :: Result<Maybe<string>, Os.Error> -> string
fn option_text result =
    match result with
    | Result.Ok (Maybe.Some value) -> "some:" + value
    | Result.Ok Maybe.None -> "none"
    | Result.Error error -> Os.message (ref error)

def length_text :: Result<[ubyte], Os.Error> -> string
fn length_text result =
    match result with
    | Result.Ok bytes -> "length:" + to_string bytes.length
    | Result.Error error -> Os.message (ref error)

def say :: string -> IO<unit>
fn say text = IO.write_line text
`;

const builds = new Map();
/** Writes \`source\` as Main.tz and returns a function that builds it once per optimization. */
function program(name, source) {
  const directory = join(root, `project-${name}`);
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, "Main.tz"), source + prelude);
  return optimization => {
    const key = `${name}${optimization}`;
    if (!builds.has(key)) {
      const executable = join(directory, `program${optimization}`);
      execute(compiler, ["build", directory, optimization, "-o", executable]);
      builds.set(key, executable);
    }
    return builds.get(key);
  };
}

/** A fresh case directory for one run of one optimization. */
function scratch(name, optimization) {
  const directory = join(root, `case-${name}${optimization}`);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });
  return directory;
}

function run(executable, cwd, { args = [], env = {}, status = 0 } = {}) {
  const result = execute(executable, args, { cwd, env: { ...process.env, ...env } }, false);
  assert.equal(result.status, status, `${executable}\n${result.stdout}\n${result.stderr}`);
  return result;
}

const message = (text, code) => (code ? `${text} (os error ${code})` : text);
const lines = result => result.stdout.split("\n").slice(0, -1);
const bytesText = buffer => `ok:${[...buffer].join(",")}`;

try {
  // 1. Whole-file round trips; an action that is built but never run leaves no file; the IO and OS runtimes link together.
  const files = program("files", `
def main :: IO<unit> =
    let _lazy = File.write_text "lazy.txt" "never"
    let! first = File.write_text "keep.txt" "h\\u00e9llo\\n"
    do! say (unit_text first)
    let! bytes = File.read_bytes "keep.txt"
    do! say (bytes_text bytes)
    let! appended = File.append_text "keep.txt" "w\\u00f6rld"
    do! say (unit_text appended)
    let! text = File.read_text "keep.txt"
    do! say (text_text text)
    let! raw = File.write_bytes "raw.bin" [0ubyte, 1ubyte, 255ubyte, 10ubyte]
    do! say (unit_text raw)
    let! gone = File.write_text "gone.txt" "x"
    do! say (unit_text gone)
    let! removed = File.remove "gone.txt"
    do! say (unit_text removed)
    let! again = File.read_bytes "gone.txt"
    do! say (bytes_text again)
    let! twice = File.remove "gone.txt"
    do! say (unit_text twice)
    let! empty = File.write_text "empty.txt" ""
    do! say (unit_text empty)
    let! none = File.read_bytes "empty.txt"
    do! say (bytes_text none)
    let! created = File.append_text "appended.txt" "one"
    do! say (unit_text created)
`);
  for (const optimization of optimizations) {
    const cwd = scratch("files", optimization);
    assert.deepEqual(lines(run(files(optimization), cwd)), [
      "ok", bytesText(Buffer.from("h\u00e9llo\n")), "ok", "ok:h\u00e9llo", "w\u00f6rld", "ok", "ok", "ok",
      message("not found", errno.ENOENT), message("not found", errno.ENOENT), "ok", "ok:", "ok",
    ]);
    assert.equal(readFileSync(join(cwd, "keep.txt"), "utf8"), "h\u00e9llo\nw\u00f6rld");
    assert.deepEqual([...readFileSync(join(cwd, "raw.bin"))], [0, 1, 255, 10]);
    assert.equal(readFileSync(join(cwd, "appended.txt"), "utf8"), "one");
    assert.ok(!existsSync(join(cwd, "lazy.txt")), "an IO value that never runs touches nothing");
    assert.ok(!existsSync(join(cwd, "gone.txt")));
  }

  // 2-6. Failure kinds and their errno codes.
  const failures = program("failures", `
def main :: IO<unit> =
    let! missing = File.read_text "missing.txt"
    do! say (text_text missing)
    let! locked = File.read_bytes "locked.txt"
    do! say (bytes_text locked)
    let! binary = File.read_text "binary.bin"
    do! say (text_text binary)
    let! raw = File.read_bytes "binary.bin"
    do! say (bytes_text raw)
    let! directory = File.read_bytes "folder"
    do! say (bytes_text directory)
    let! nul = File.read_bytes "a\\0b"
    do! say (bytes_text nul)
    let! surrogate = File.write_text "\\uD800" "x"
    do! say (unit_text surrogate)
    let! bad_text = File.write_text "bad-text.txt" "\\uD800"
    do! say (unit_text bad_text)
    let! nul_write = File.write_text "a\\0b" "x"
    do! say (unit_text nul_write)
    let! missing_parent = File.write_text "nowhere/x.txt" "x"
    do! say (unit_text missing_parent)
    let! over_folder = File.write_text "folder" "x"
    do! say (unit_text over_folder)
    let! unlink_folder = File.remove "folder"
    do! say (unit_text unlink_folder)
`);
  const rootUser = process.getuid?.() === 0;
  for (const optimization of optimizations) {
    const cwd = scratch("failures", optimization);
    writeFileSync(join(cwd, "locked.txt"), "secret");
    chmodSync(join(cwd, "locked.txt"), 0o000);
    writeFileSync(join(cwd, "binary.bin"), Buffer.from([0xff, 0xfe, 0x41]));
    mkdirSync(join(cwd, "folder"));
    const output = lines(run(failures(optimization), cwd));
    assert.equal(output[0], message("not found", errno.ENOENT));
    if (rootUser) console.log("OS: skipping the permission case because the tests run as root");
    else assert.equal(output[1], message("permission denied", errno.EACCES));
    assert.equal(output[2], "invalid encoding");
    assert.equal(output[3], "ok:255,254,65");
    assert.equal(output[4], message("invalid input", errno.EISDIR));
    assert.equal(output[5], "invalid input");
    assert.equal(output[6], "invalid encoding");
    assert.equal(output[7], "invalid encoding");
    assert.equal(output[8], "invalid input");
    assert.equal(output[9], message("not found", errno.ENOENT));
    assert.equal(output[10], message("invalid input", errno.EISDIR));
    // unlink(2) on a directory is EPERM on macOS and EISDIR on Linux.
    assert.equal(output[11], process.platform === "darwin" ? message("permission denied", errno.EPERM) : message("invalid input", errno.EISDIR));
    assert.deepEqual(readdirSync(cwd).sort(), ["binary.bin", "folder", "locked.txt"], "failed calls create nothing");
    chmodSync(join(cwd, "locked.txt"), 0o600);
  }

  // 7-8. Directories, byte-order listing, and symbolic links.
  const directories = program("directories", `
def main :: IO<unit> =
    let! first = Dir.create "made"
    do! say (unit_text first)
    let! second = Dir.create "made"
    do! say (unit_text second)
    let! nested = Dir.create "none/inner"
    do! say (unit_text nested)
    let! listed = Dir.list "listing"
    do! say (names_text listed)
    let! empty = Dir.list "made"
    do! say (names_text empty)
    let! missing = Dir.list "absent"
    do! say (names_text missing)
    let! file_list = Dir.list "listing/a"
    do! say (names_text file_list)
    let! full = Dir.remove "listing"
    do! say (unit_text full)
    let! removed = Dir.remove "made"
    do! say (unit_text removed)
    let! absent = Dir.remove "made"
    do! say (unit_text absent)
    let! link_text = File.read_text "target/link"
    do! say (text_text link_text)
    let! link_names = Dir.list "target"
    do! say (names_text link_names)
    let! unlinked = File.remove "target/link"
    do! say (unit_text unlinked)
    let! survivors = Dir.list "target"
    do! say (names_text survivors)
    let! through_link = Dir.remove "dirlink"
    do! say (unit_text through_link)
`);
  const utf8Order = names => [...names].sort((left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right)));
  for (const optimization of optimizations) {
    const cwd = scratch("directories", optimization);
    mkdirSync(join(cwd, "listing"));
    for (const name of ["b", "a", "A", "ab", "\u00e9", "\uff61", "\u{1f600}", "z.txt"]) writeFileSync(join(cwd, "listing", name), "");
    mkdirSync(join(cwd, "target"));
    writeFileSync(join(cwd, "target", "a.txt"), "payload");
    symlinkSync("a.txt", join(cwd, "target", "link"));
    mkdirSync(join(cwd, "realdir"));
    symlinkSync("realdir", join(cwd, "dirlink"));
    // The expectation comes from what the file system stored: HFS+ would normalize the accent.
    const sorted = utf8Order(readdirSync(join(cwd, "listing")));
    assert.ok(sorted.indexOf("\uff61") < sorted.indexOf("\u{1f600}"), "U+FF61 sorts before U+1F600 by UTF-8 bytes, unlike UTF-16 order");
    const output = lines(run(directories(optimization), cwd));
    assert.deepEqual(output.slice(0, 3), ["ok", message("already exists", errno.EEXIST), message("not found", errno.ENOENT)]);
    assert.equal(output[3], `ok:${sorted.length}:${sorted.join("|")}`);
    assert.equal(output[4], "ok:0:");
    assert.equal(output[5], message("not found", errno.ENOENT));
    assert.equal(output[6], message("not found", errno.ENOTDIR));
    assert.equal(output[7], message("other", errno.ENOTEMPTY));
    assert.equal(output[8], "ok");
    assert.equal(output[9], message("not found", errno.ENOENT));
    assert.equal(output[10], "ok:payload");
    assert.equal(output[11], "ok:2:a.txt|link");
    assert.equal(output[12], "ok");
    assert.equal(output[13], "ok:1:a.txt");
    assert.equal(output[14], message("not found", errno.ENOTDIR));
    assert.equal(readFileSync(join(cwd, "target", "a.txt"), "utf8"), "payload", "removing a link keeps its target");
    assert.ok(existsSync(join(cwd, "realdir")));
  }
  if (process.platform === "linux") {
    // A name that is not UTF-8 makes the whole listing InvalidEncoding (macOS cannot create such a name).
    const invalidNames = program("invalid-names", `
def main :: IO<unit> =
    let! listed = Dir.list "invalid-names"
    do! say (names_text listed)
`);
    for (const optimization of optimizations) {
      const cwd = scratch("invalid-names", optimization);
      const invalid = join(cwd, "invalid-names");
      mkdirSync(invalid);
      writeFileSync(Buffer.concat([Buffer.from(`${invalid}/`), Buffer.from([0x66, 0xff])]), "");
      assert.deepEqual(lines(run(invalidNames(optimization), cwd)), ["invalid encoding"]);
    }
  }

  // 9. Arguments, environment, and the working directory.
  const environment = program("environment", `
def main :: IO<unit> =
    let! args = Env.args ()
    do! say (names_text args)
    let! set = Env.var "TZ_E08"
    do! say (option_text set)
    let! unset = Env.var "TZ_E08_UNSET"
    do! say (option_text unset)
    let! equals = Env.var "A=B"
    do! say (option_text equals)
    let! empty = Env.var ""
    do! say (option_text empty)
    let! directory = Env.current_dir ()
    do! say (text_text directory)
`);
  for (const optimization of optimizations) {
    const cwd = scratch("environment", optimization);
    const result = run(environment(optimization), cwd, { args: ["\u03b1", "", "b c"], env: { TZ_E08: "v\u00e4rde" } });
    assert.deepEqual(lines(result), ["ok:3:\u03b1||b c", "some:v\u00e4rde", "none", "invalid input", "invalid input", `ok:${realpathSync(cwd)}`]);
    assert.deepEqual(lines(run(environment(optimization), cwd, { env: { TZ_E08: "" } })).slice(0, 2), ["ok:0:", "some:"]);
  }

  // 10-11. Clocks and operating-system randomness.
  const timing = program("timing", `
def clocks_text :: Result<i64, Os.Error> -> Result<i64, Os.Error> -> string
fn clocks_text before after =
    match (before, after) with
    | (Result.Ok first, Result.Ok second) -> "elapsed:" + to_string (second - first)
    | _ -> "failed"

def unix_text :: Result<i64, Os.Error> -> string
fn unix_text result =
    match result with
    | Result.Ok value -> "unix:" + to_string value
    | Result.Error error -> Os.message (ref error)

def main :: IO<unit> =
    let! before = Time.monotonic_ns ()
    let! slept = Time.sleep_ms 50
    let! after = Time.monotonic_ns ()
    let! unix = Time.unix_ns ()
    let! negative = Time.sleep_ms (-1)
    let! zero = Time.sleep_ms 0
    do! say (clocks_text before after)
    do! say (unit_text slept)
    do! say (unix_text unix)
    do! say (unit_text negative)
    do! say (unit_text zero)
`);
  const random = program("random", `
def pair_text :: Result<[ubyte], Os.Error> -> Result<[ubyte], Os.Error> -> string
fn pair_text first second =
    match (first, second) with
    | (Result.Ok left, Result.Ok right) ->
        "lengths:" + to_string left.length + "," + to_string right.length + " same:" + to_string (digits (ref left) == digits (ref right))
    | _ -> "failed"

def word_text :: Result<i64u, Os.Error> -> string
fn word_text result =
    match result with
    | Result.Ok _ -> "word"
    | Result.Error error -> Os.message (ref error)

def main :: IO<unit> =
    let! none = Random.bytes 0
    do! say (bytes_text none)
    let! first = Random.bytes 1000
    let! second = Random.bytes 1000
    do! say (pair_text first second)
    let! negative = Random.bytes (-1)
    do! say (bytes_text negative)
    let! huge = Random.bytes 1073741825
    do! say (bytes_text huge)
    let! word = Random.next_u64 ()
    do! say (word_text word)
    let! large = Random.bytes 1048576
    do! say (length_text large)
`);
  // C09: maps and sets keyed by a seed from the operating system behave like any other, and a flood of
  // keys that collide under the fixed mixing does not pile up in one cluster.
  const randomMaps = program("randommaps", `
def fill :: HashMap<i64, i64> -> i64 -> HashMap<i64, i64>
fn fill start count =
    let mut map = start
    let mut index = 0
    while index < count do
        map = HashMap.insert map (index * 7919 - 5000) index
        index = index + 1
    map

def found :: ref HashMap<i64, i64> -> i64 -> i64
fn found map count =
    let mut total = 0
    let mut index = 0
    while index < count do
        let key = index * 7919 - 5000
        if HashMap.contains_key (ref map) key && deref (HashMap.at (ref map) key) == index then total = total + 1
        index = index + 1
    total

def map_text :: HashMap<i64, i64> -> string
fn map_text start =
    let map = fill start 2000
    let sum = HashMap.fold (\\total _key value -> total + deref value) 0 (ref map)
    $"len:{HashMap.length (ref map)} found:{found (ref map) 2000} sum:{sum}"

def tried_text :: Result<HashMap<i64, i64>, Os.Error> -> string
fn tried_text made =
    match made with
    | Result.Ok map -> map_text map
    | Result.Error error -> Os.message (ref error)

def set_text :: HashSet<string> -> string
fn set_text start =
    let mut set = start
    let mut index = 0
    while index < 500 do
        set = HashSet.insert set (to_string index)
        index = index + 1
    let mut present = 0
    index = 0
    while index < 1000 do
        let key = to_string index
        if HashSet.contains_ref (ref set) (ref key) then present = present + 1
        index = index + 1
    $"len:{HashSet.length (ref set)} present:{present}"

def flood_mix :: i64u -> i64
fn flood_mix hash =
    let first = (hash ^ (hash >>> 33)) * 0xff51afd7ed558ccd
    let second = (first ^ (first >>> 33)) * 0xc4ceb9fe1a85ec53
    (second ^ (second >>> 33)) as i64

def colliding_keys :: i64 -> Vec<i64>
fn colliding_keys count =
    let mut keys = Vec.with_capacity count
    let mut candidate = 0
    while Vec.length (ref keys) < count do
        if ((flood_mix (Hash.hash (ref candidate))) & 2047) == 0 then keys = Vec.push keys candidate
        candidate = candidate + 1
    keys

def flood_text :: HashMap<i64, i64> -> string
fn flood_text start =
    let keys = colliding_keys 600
    let mut map = start
    let mut index = 0
    while index < Vec.length (ref keys) do
        map = HashMap.insert map keys[index] index
        index = index + 1
    if HashMap.length (ref map) == 600 && HashMap.longest_probe (ref map) <= 64 then "flood ok" else "flood bad"

def main :: IO<unit> =
    let! made = HashMap.randomized ()
    do! say (map_text made)
    let! tried = HashMap.try_randomized ()
    do! say (tried_text tried)
    let! set = HashSet.randomized ()
    do! say (set_text set)
    let! flooded = HashMap.randomized ()
    do! say (flood_text flooded)
`);
  const randomMapLines = ["len:2000 found:2000 sum:1999000", "len:2000 found:2000 sum:1999000", "len:500 present:500", "flood ok"];
  for (const optimization of optimizations) {
    const cwd = scratch("timing", optimization);
    const output = lines(run(timing(optimization), cwd));
    assert.ok(BigInt(output[0].slice("elapsed:".length)) >= 50_000_000n, output[0]);
    assert.equal(output[1], "ok");
    const difference = BigInt(output[2].slice("unix:".length)) - BigInt(Date.now()) * 1_000_000n;
    assert.ok(difference < 60_000_000_000n && difference > -60_000_000_000n, output[2]);
    assert.deepEqual(output.slice(3), ["invalid input", "ok"]);
    assert.deepEqual(lines(run(random(optimization), cwd)), [
      "ok:", "lengths:1000,1000 same:false", "invalid input", "invalid input", "word", "length:1048576",
    ]);
    assert.deepEqual(lines(run(randomMaps(optimization), cwd)), randomMapLines);
  }

  // 12. PCG-XSH-RR 64/32 against the published pcg-c-basic demonstration output and a BigInt reference.
  const mask = (1n << 64n) - 1n;
  const pcg = (seed, sequence) => {
    const increment = ((sequence << 1n) | 1n) & mask;
    const step = state => (state * 6364136223846793005n + increment) & mask;
    let state = step(0n);
    state = step((state + seed) & mask);
    return () => {
      const old = state;
      state = step(old);
      const shifted = Number((((old >> 18n) ^ old) >> 27n) & 0xffffffffn);
      const rotation = Number(old >> 59n);
      return ((shifted >>> rotation) | (shifted << ((-rotation) & 31))) >>> 0;
    };
  };
  const published = [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e];
  const reference = pcg(42n, 54n);
  assert.deepEqual(published.map(() => reference()), published, "the reference matches pcg-c-basic");
  const generator = program("pcg", `
def words :: unit -> string
fn words _unit =
    let mut state = Random.pcg 42i64u 54i64u
    let mut out = ""
    let mut index = 0
    while index < 6 do
        match Random.pcg_next_u32 state with
        | (value, next) ->
            out = out + to_string value + " "
            state = next
        index = index + 1
    out

def longs :: unit -> string
fn longs _unit =
    let mut state = Random.pcg 42i64u 54i64u
    let mut out = ""
    let mut index = 0
    while index < 3 do
        match Random.pcg_next_u64 state with
        | (value, next) ->
            out = out + to_string value + " "
            state = next
        index = index + 1
    out

def zero :: unit -> string
fn zero _unit =
    match Random.pcg_next_u32 (Random.pcg 0i64u 0i64u) with
    | (value, _) -> to_string value

def main :: IO<unit> =
    do! say (words ())
    do! say (longs ())
    do! say (zero ())
`);
  for (const optimization of optimizations) {
    const output = lines(run(generator(optimization), scratch("pcg", optimization)));
    const again = pcg(42n, 54n);
    const wide = [0, 1, 2].map(() => (BigInt(again()) << 32n) | BigInt(again()));
    assert.equal(output[0], `${published.join(" ")} `);
    assert.equal(output[1], `${wide.join(" ")} `);
    assert.equal(output[2], String(pcg(0n, 0n)()));
  }

  // 14. An IO<i32> entry returns its value as the exit code.
  for (const [type, value, status] of [["i32", "3i32", 3], ["i32", "0i32", 0], ["i32", "256i32", 0], ["i64", "5", 0], ["unit", "()", 0]]) {
    const name = `exit-${type}-${value}`;
    const code = program(name, `def main :: IO<${type}> =\n    do! say "done"\n    return ${value}\n`);
    for (const optimization of optimizations) {
      assert.equal(run(code(optimization), scratch(name, optimization), { status }).stdout, "done\n");
    }
  }
  {
    const directory = join(root, "project-exit-i32-3i32");
    const failed = execute(compiler, ["run", directory], {}, false);
    assert.equal(failed.status, 1);
    assert.match(failed.stderr, /E2005.*program exited with code 3/s);
    const json = execute(compiler, ["run", directory, "--json"], {}, false);
    const diagnostic = JSON.parse(json.stderr.trim());
    assert.equal(diagnostic.code, "E2005");
    assert.match(diagnostic.message, /program exited with code 3/);
    assert.equal(execute(compiler, ["run", join(root, "project-exit-i32-0i32")]).stdout, "done\n");
  }

  // 15. The default wasm targets reject the operating-system APIs and write no output.
  for (const emit of ["wasm", "llvm", "object"]) {
    for (const optimization of optimizations) {
      const output = join(root, `rejected-${emit}${optimization}`);
      const result = execute(compiler, ["build", join(root, "project-files"), "--target", "wasm32", "--emit", emit, optimization, "-o", output], {}, false);
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, /E2000.*wasm output cannot use the File, Dir, Env, Time, Random, or Process operating-system APIs/s);
      assert.ok(!existsSync(output));
    }
  }
  // A map keyed by an operating-system seed is one of those APIs; a map keyed by a given seed is not.
  for (const optimization of optimizations) {
    const output = join(root, `rejected-randommaps${optimization}`);
    const result = execute(compiler, ["build", join(root, "project-randommaps"), "--target", "wasm32", optimization, "-o", output], {}, false);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /E2000.*Random/s);
    assert.ok(!existsSync(output));
  }

  // 13. Path and Os are pure: they build for wasm32 without imports and agree with native.
  const pathText = `
def show :: Maybe<string> -> string
fn show value =
    match value with
    | Maybe.Some text -> "Some " + text
    | Maybe.None -> "None"

def copy :: ref string -> string
fn copy text = Maybe.get (String.slice text 0 text.length)

def joined :: string -> string -> string
fn joined left right = Path.join (ref left) (ref right)

def report :: unit -> string
fn report _unit =
    let mut out = ""
    out = out + joined "a" "b" + "\\n" + joined "a/" "b" + "\\n" + joined "" "b" + "\\n" + joined "a" "/etc" + "\\n"
    for path in ["a/b/c", "/a", "a", "/", "", "a/b/"] do
        out = out + "parent " + copy path + " -> " + show (Path.parent path) + "\\n"
    for path in ["a/b.txt", "a/..", ".", "/", "", "a/b/"] do
        out = out + "name " + copy path + " -> " + show (Path.file_name path) + "\\n"
    for path in ["a.tar.gz", ".bashrc", "a.", "a/b.d/c"] do
        out = out + "ext " + copy path + " -> " + show (Path.extension path) + "\\n"
    let permission = Os.error_of_status ((2i64 <<< 32) | 13i64)
    out = out + Os.message (ref permission) + "\\n"
    let plain = Os.error_of_status (7i64 <<< 32)
    out + Os.message (ref plain) + "\\n"

def digest :: string -> i64
fn digest text =
    let mut hash = 0
    let mut index = 0
    while index < text.length do
        hash = (hash * 131 + (text[index] as i64)) % 1000000007
        index = index + 1
    hash
`;
  const expectedPaths = [
    "a/b", "a/b", "b", "a//etc",
    "parent a/b/c -> Some a/b", "parent /a -> Some /", "parent a -> None", "parent / -> None", "parent  -> None", "parent a/b/ -> Some a",
    "name a/b.txt -> Some b.txt", "name a/.. -> None", "name . -> None", "name / -> None", "name  -> None", "name a/b/ -> Some b",
    "ext a.tar.gz -> Some gz", "ext .bashrc -> None", "ext a. -> Some ", "ext a/b.d/c -> None",
    "permission denied (os error 13)", "other",
  ].join("\n") + "\n";
  const paths = program("paths", `${pathText}\ndef main :: IO<unit> = IO.write (report ())\n`);
  const exported = join(root, "project-paths-wasm");
  mkdirSync(exported, { recursive: true });
  writeFileSync(join(exported, "Main.tz"), `${pathText}\nexport def path_digest :: i64 -> i64\nfn path_digest n = digest (report ()) + n\n`);
  let expectedDigest = 0n;
  for (let index = 0; index < expectedPaths.length; index++) expectedDigest = (expectedDigest * 131n + BigInt(expectedPaths.charCodeAt(index))) % 1000000007n;
  for (const optimization of optimizations) {
    assert.equal(run(paths(optimization), scratch("paths", optimization)).stdout, expectedPaths);
    const wasm = join(root, `paths${optimization}.wasm`);
    execute(compiler, ["build", exported, "--target", "wasm32", optimization, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(new WebAssembly.Instance(module).exports.tz_path_digest(0n), expectedDigest);
  }

  // 17. File.Handle: streaming reads and writes, misuse errors, stale handles, and no leaked descriptors.
  const handles = program("handles", `
def opened_text :: Result<File.Handle, Os.Error> -> string
fn opened_text result =
    match result with
    | Result.Ok _ -> "opened"
    | Result.Error error -> Os.message (ref error)

def with_handle :: Result<File.Handle, Os.Error> -> (File.Handle -> IO<string>) -> IO<string>
fn with_handle opened next =
    match opened with
    | Result.Ok handle -> next handle
    | Result.Error error -> IO.pure (Os.message (ref error))

def writer :: File.Handle -> IO<string>
fn writer handle = IO {
    let! first = File.write handle [104ubyte, 195ubyte]
    let! second = File.write handle [169ubyte, 108ubyte, 108ubyte, 111ubyte]
    let! flushed = File.flush handle
    let! closed = File.close handle
    let! stale = File.write handle [1ubyte]
    let! again = File.close handle
    return unit_text first + "," + unit_text second + "," + unit_text flushed + "," + unit_text closed + "," + unit_text stale + "," + unit_text again
}

def appender :: File.Handle -> IO<string>
fn appender handle = IO {
    let! added = File.write handle [33ubyte]
    let! closed = File.close handle
    return unit_text added + "," + unit_text closed
}

def rec drain :: File.Handle -> i64 -> IO<string>
fn rec drain handle total = IO {
    let! chunk = File.read handle 4
    return! match chunk with
        | Result.Ok bytes -> if bytes.length == 0 then IO.pure ("total:" + to_string total) else drain handle (total + bytes.length)
        | Result.Error error -> IO.pure (Os.message (ref error))
}

def drain_all :: File.Handle -> IO<string>
fn drain_all handle = IO.bind (drain handle 0) (\\text -> IO.map (\\closed -> text + "," + unit_text closed) (File.close handle))

def read_checks :: File.Handle -> IO<string>
fn read_checks handle = IO {
    let! negative = File.read handle (-1)
    let! huge = File.read handle 1073741825
    let! zero = File.read handle 0
    let! wrong = File.write handle [1ubyte]
    let! part = File.read handle 2
    let! closed = File.close handle
    return bytes_text negative + "," + bytes_text huge + "," + bytes_text zero + "," + unit_text wrong + "," + bytes_text part + "," + unit_text closed
}

def write_checks :: File.Handle -> IO<string>
fn write_checks handle = IO {
    let! wrong = File.read handle 1
    let! closed = File.close handle
    return bytes_text wrong + "," + unit_text closed
}

def reread :: File.Handle -> IO<string>
fn reread handle = IO.bind (File.read handle 2) (\\bytes -> IO.map (\\closed -> bytes_text bytes + "," + unit_text closed) (File.close handle))

def after_close :: File.Handle -> IO<string>
fn after_close first = IO {
    let! closed = File.close first
    let! second = File.open "data.txt" File.Read
    let! stale = File.read first 4
    let! live = with_handle second reread
    return unit_text closed + "," + bytes_text stale + "," + live
}

def main :: IO<unit> =
    let! created = File.open "stream.txt" File.Write
    let! a = with_handle created writer
    do! say a
    let! appending = File.open "stream.txt" File.Append
    let! b = with_handle appending appender
    do! say b
    let! reading = File.open "stream.txt" File.Read
    let! c = with_handle reading drain_all
    do! say c
    let! existing = File.open "stream.txt" File.CreateNew
    do! say (opened_text existing)
    let! fresh = File.open "fresh.txt" File.CreateNew
    let! d = with_handle fresh (\\handle -> IO.map unit_text (File.close handle))
    do! say d
    let! missing = File.open "missing.txt" File.Read
    do! say (opened_text missing)
    let! folder = File.open "folder" File.Read
    do! say (opened_text folder)
    let! locked = File.open "folder/x/y" File.Write
    do! say (opened_text locked)
    let! data = File.open "data.txt" File.Read
    let! e = with_handle data read_checks
    do! say e
    let! output = File.open "output.txt" File.Write
    let! f = with_handle output write_checks
    do! say f
    let! earlier = File.open "data.txt" File.Read
    let! g = with_handle earlier after_close
    do! say g
`);
  for (const optimization of optimizations) {
    const cwd = scratch("handles", optimization);
    mkdirSync(join(cwd, "folder"));
    writeFileSync(join(cwd, "data.txt"), "abcdef");
    const output = lines(run(handles(optimization), cwd));
    assert.deepEqual(output, [
      `ok,ok,ok,ok,${message("invalid input", errno.EBADF)},${message("invalid input", errno.EBADF)}`,
      "ok,ok",
      "total:7,ok",
      message("already exists", errno.EEXIST),
      "ok",
      message("not found", errno.ENOENT),
      message("invalid input", errno.EISDIR),
      message("not found", errno.ENOENT),
      `invalid input,invalid input,ok:,${message("invalid input", errno.EBADF)},ok:97,98,ok`,
      `${message("invalid input", errno.EBADF)},ok`,
      `ok,${message("invalid input", errno.EBADF)},ok:97,98,ok`,
    ]);
    assert.equal(readFileSync(join(cwd, "stream.txt"), "utf8"), "h\u00e9llo!");
    assert.equal(readFileSync(join(cwd, "fresh.txt")).length, 0);
  }

  const descriptors = program("descriptors", `
def count :: unit -> IO<i64>
fn count _unit = IO {
    let! where = Env.var "TZ_FD_DIR"
    return! match where with
        | Result.Ok (Maybe.Some path) -> IO.map length_of (Dir.list path)
        | _ -> IO.pure (-1)
}

def length_of :: Result<[string], Os.Error> -> i64
fn length_of result =
    match result with
    | Result.Ok names -> names.length
    | Result.Error _ -> -1

def failed :: Result<unit, Os.Error> -> i64
fn failed result =
    match result with
    | Result.Ok _ -> 0
    | Result.Error _ -> 1

def rec churn :: i64 -> i64 -> IO<i64>
fn rec churn remaining failures =
    if remaining <= 0 then IO.pure failures
    else IO.bind (File.open "data.txt" File.Read) (\\opened ->
        match opened with
        | Result.Ok handle -> IO.bind (File.close handle) (\\closed -> churn (remaining - 1) (failures + failed closed))
        | Result.Error _ -> churn (remaining - 1) (failures + 1))

def both :: File.Handle -> File.Handle -> File.Handle -> IO<i64>
fn both a b c = IO {
    let! during = count ()
    let! x = File.close a
    let! y = File.close b
    let! z = File.close c
    return during + failed x + failed y + failed z
}

def hold :: Result<File.Handle, Os.Error> -> Result<File.Handle, Os.Error> -> Result<File.Handle, Os.Error> -> IO<i64>
fn hold a b c =
    match (a, b, c) with
    | (Result.Ok x, Result.Ok y, Result.Ok z) -> both x y z
    | _ -> IO.pure (-1)

def scoped_text :: Result<Result<unit, Os.Error>, Os.Error> -> string
fn scoped_text result =
    match result with
    | Result.Ok (Result.Ok _) -> "ok"
    | Result.Ok (Result.Error error) -> "inner:" + Os.message (ref error)
    | Result.Error error -> "outer:" + Os.message (ref error)

def scoped_bytes :: Result<Result<[ubyte], Os.Error>, Os.Error> -> string
fn scoped_bytes result =
    match result with
    | Result.Ok (Result.Ok bytes) -> "bytes:" + to_string bytes.length
    | Result.Ok (Result.Error error) -> "inner:" + Os.message (ref error)
    | Result.Error error -> "outer:" + Os.message (ref error)

def main :: IO<unit> =
    let! before = count ()
    let! failures = churn 1500 0
    let! after = count ()
    do! say ("churn:" + to_string failures + " same:" + to_string (before == after))
    let! a = File.open "data.txt" File.Read
    let! b = File.open "data.txt" File.Read
    let! c = File.open "data.txt" File.Read
    let! during = hold a b c
    let! settled = count ()
    do! say ("hold:" + to_string (during - before) + " same:" + to_string (before == settled))
    let! scoped = File.with_open "scoped.txt" File.Write (\\handle -> File.write handle [97ubyte])
    do! say (scoped_text scoped)
    let! inner = File.with_open "scoped.txt" File.Write (\\handle -> File.read handle 1)
    do! say (scoped_bytes inner)
    let! outer = File.with_open "nope.txt" File.Read (\\handle -> File.read handle 1)
    do! say (scoped_bytes outer)
    let! reread = File.with_open "scoped.txt" File.Read (\\handle -> File.read handle 8)
    do! say (scoped_bytes reread)
    let! last = count ()
    do! say ("scoped same:" + to_string (before == last))
`);
  const fdDirectory = process.platform === "linux" ? "/proc/self/fd" : "/dev/fd";
  for (const optimization of optimizations) {
    const cwd = scratch("descriptors", optimization);
    writeFileSync(join(cwd, "data.txt"), "abcdef");
    const output = lines(run(descriptors(optimization), cwd, { env: { TZ_FD_DIR: fdDirectory } }));
    assert.deepEqual(output, [
      "churn:0 same:true", "hold:3 same:true", "ok", `inner:${message("invalid input", errno.EBADF)}`,
      `outer:${message("not found", errno.ENOENT)}`, "bytes:0",
      "scoped same:true",
    ]);
    // The write through with_open reached the file; the later truncating open emptied it.
    assert.equal(readFileSync(join(cwd, "scoped.txt")).length, 0);
  }

  // 19. File.metadata and File.link_metadata against Node's stat, including clamped kinds and failures.
  const metadataProgram = program("metadata", `
def kind_text :: File.EntryKind -> string
fn kind_text kind =
    match kind with
    | File.Regular -> "file"
    | File.Directory -> "dir"
    | File.Symlink -> "link"
    | File.Other -> "other"

def meta_text :: Result<File.Metadata, Os.Error> -> string
fn meta_text result =
    match result with
    | Result.Ok meta -> kind_text meta.kind + " " + to_string meta.size + " " + to_string meta.modified_ns
    | Result.Error error -> Os.message (ref error)

def main :: IO<unit> =
    let! file = File.metadata "data.txt"
    do! say (meta_text file)
    let! folder = File.metadata "folder"
    do! say (meta_text folder)
    let! link = File.metadata "link"
    do! say (meta_text link)
    let! raw = File.link_metadata "link"
    do! say (meta_text raw)
    let! broken = File.metadata "broken"
    do! say (meta_text broken)
    let! raw_broken = File.link_metadata "broken"
    do! say (meta_text raw_broken)
    let! missing = File.metadata "missing"
    do! say (meta_text missing)
    let! nul = File.metadata "a\\0b"
    do! say (meta_text nul)
    let! surrogate = File.metadata "\\uD800"
    do! say (meta_text surrogate)
    let! pipe = File.metadata "pipe"
    do! say (meta_text pipe)
    let! stamped = File.metadata "stamped.txt"
    do! say (meta_text stamped)
`);
  const prepareMetadata = cwd => {
    writeFileSync(join(cwd, "data.txt"), "hello");
    mkdirSync(join(cwd, "folder"));
    symlinkSync("data.txt", join(cwd, "link"));
    symlinkSync("gone.txt", join(cwd, "broken"));
    execute("mkfifo", [join(cwd, "pipe")]);
    writeFileSync(join(cwd, "stamped.txt"), "abc");
    utimesSync(join(cwd, "stamped.txt"), 1700000000, 1700000000.25);
  };
  const statLine = (path, kind, follow = true) => {
    const stat = (follow ? statSync : lstatSync)(path, { bigint: true });
    return `${kind} ${stat.size} ${stat.mtimeNs}`;
  };
  for (const optimization of optimizations) {
    const cwd = scratch("metadata", optimization);
    prepareMetadata(cwd);
    const output = lines(run(metadataProgram(optimization), cwd));
    assert.deepEqual(output, [
      statLine(join(cwd, "data.txt"), "file"), statLine(join(cwd, "folder"), "dir"), statLine(join(cwd, "link"), "file"),
      statLine(join(cwd, "link"), "link", false), message("not found", errno.ENOENT), statLine(join(cwd, "broken"), "link", false),
      message("not found", errno.ENOENT), "invalid input", "invalid encoding", statLine(join(cwd, "pipe"), "other"),
      "file 3 1700000000250000000",
    ]);
  }

  // 20. Dir.walk against a reference walk: pre-order, UTF-8 byte order, symbolic links listed but never followed.
  const walks = program("walks", `
def main :: IO<unit> =
    let! everything = Dir.walk "root"
    do! say (names_text everything)
    let! empty = Dir.walk "root/empty"
    do! say (names_text empty)
    let! missing = Dir.walk "absent"
    do! say (names_text missing)
    let! file = Dir.walk "root/a.txt"
    do! say (names_text file)
    let! locked = Dir.walk "locked"
    do! say (names_text locked)
    let! partial = Dir.walk "partial"
    do! say (names_text partial)
`);
  const prepareWalk = cwd => {
    const root = join(cwd, "root");
    mkdirSync(join(root, "b", "y"), { recursive: true });
    mkdirSync(join(root, "empty"));
    mkdirSync(join(root, "\u00e9"));
    for (const path of ["a.txt", "b/x", "b/y/z.txt", "\u00e9/inner", "\uff61", "\u{1f600}", "c"]) writeFileSync(join(root, path), "");
    symlinkSync("..", join(root, "loop"));
    symlinkSync("b", join(root, "dirlink"));
    mkdirSync(join(cwd, "locked"));
    chmodSync(join(cwd, "locked"), 0o000);
    mkdirSync(join(cwd, "partial", "ok"), { recursive: true });
    writeFileSync(join(cwd, "partial", "ok", "file"), "");
    mkdirSync(join(cwd, "partial", "zdeny"));
    chmodSync(join(cwd, "partial", "zdeny"), 0o000);
  };
  const walkReference = (directory, prefix = "") => readdirSync(directory)
    .sort((left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right)))
    .flatMap(name => {
      const relative = `${prefix}${name}`, stat = lstatSync(join(directory, name));
      return [relative, ...(stat.isDirectory() ? walkReference(join(directory, name), `${relative}/`) : [])];
    });
  const restoreWalk = cwd => {
    chmodSync(join(cwd, "locked"), 0o700);
    chmodSync(join(cwd, "partial", "zdeny"), 0o700);
  };
  for (const optimization of optimizations) {
    const cwd = scratch("walks", optimization);
    prepareWalk(cwd);
    const expected = walkReference(join(cwd, "root"));
    assert.ok(expected.includes("loop") && !expected.some(name => name.startsWith("loop/")), "links are not followed");
    assert.ok(expected.indexOf("\uff61") < expected.indexOf("\u{1f600}"), "byte order, not UTF-16 order");
    const output = lines(run(walks(optimization), cwd));
    assert.equal(output[0], `ok:${expected.length}:${expected.join("|")}`);
    assert.equal(output[1], "ok:0:");
    assert.equal(output[2], message("not found", errno.ENOENT));
    assert.equal(output[3], message("not found", errno.ENOTDIR));
    if (rootUser) console.log("OS: skipping the unreadable directory cases because the tests run as root");
    else {
      assert.equal(output[4], message("permission denied", errno.EACCES));
      assert.equal(output[5], message("permission denied", errno.EACCES));
    }
    restoreWalk(cwd);
  }

  // 21. Process.run: no shell, exact arguments and streams, exit codes and signals, and no hanging on large data.
  const processes = program("processes", `
def text_of :: ref [ubyte] -> string
fn text_of bytes =
    match Os.decode (Array.sub bytes 0 bytes.length) with
    | Result.Ok text -> text
    | Result.Error _ -> "<binary>"

def output_text :: Result<Process.Output, Os.Error> -> string
fn output_text result =
    match result with
    | Result.Ok output -> "code=" + to_string output.code + " signal=" + to_string output.signal + " out=[" + text_of (ref output.stdout) + "] err=[" + text_of (ref output.stderr) + "]"
    | Result.Error error -> Os.message (ref error)

def sizes_text :: Result<Process.Output, Os.Error> -> string
fn sizes_text result =
    match result with
    | Result.Ok output -> "code=" + to_string output.code + " out=" + to_string output.stdout.length + " err=" + to_string output.stderr.length
    | Result.Error error -> Os.message (ref error)

def same_text :: Result<Process.Output, Os.Error> -> [ubyte] -> string
fn same_text result input =
    match result with
    | Result.Ok output -> "code=" + to_string output.code + " same=" + to_string (Array.equal (ref input) (ref output.stdout)) + " length=" + to_string output.stdout.length
    | Result.Error error -> Os.message (ref error)

def big_input :: unit -> [ubyte]
fn big_input _unit = new [ubyte](1048576, index -> ((index * 7 + index / 256) % 256) as ubyte)

def run_say :: string -> [string] -> [ubyte] -> IO<unit>
fn run_say program args input = IO.bind (Process.run program args input) (\\result -> say (output_text result))

def count :: unit -> IO<i64>
fn count _unit = IO {
    let! where = Env.var "TZ_FD_DIR"
    return! match where with
        | Result.Ok (Maybe.Some path) -> IO.map length_of (Dir.list path)
        | _ -> IO.pure (-1)
}

def length_of :: Result<[string], Os.Error> -> i64
fn length_of result =
    match result with
    | Result.Ok names -> names.length
    | Result.Error _ -> -1

def rec spawn_many :: i64 -> i64 -> IO<i64>
fn rec spawn_many remaining failures =
    if remaining <= 0 then IO.pure failures
    else IO.bind (Process.run "true" [] []) (\\result ->
        match result with
        | Result.Ok _ -> spawn_many (remaining - 1) failures
        | Result.Error _ -> spawn_many (remaining - 1) (failures + 1))

def main :: IO<unit> =
    let nothing: [ubyte] = []
    do! run_say "/bin/sh" ["-c", "for a; do printf '[%s]' \\"$a\\"; done", "sh", "a b", "; touch pwned", "", "$HOME", "\\u00e9"] nothing
    do! run_say "cat" [] [104ubyte, 105ubyte]
    do! run_say "/bin/sh" ["-c", "printf out; printf err >&2"] nothing
    do! run_say "/bin/sh" ["-c", "exit 7"] nothing
    do! run_say "/bin/sh" ["-c", "kill -9 $$"] nothing
    do! run_say "/bin/sh" ["-c", "kill -15 $$"] nothing
    do! run_say "definitely-not-a-program" [] nothing
    do! run_say "./plain.txt" [] nothing
    do! run_say "a\\0b" [] nothing
    do! run_say "/bin/sh" ["x\\0y"] nothing
    do! run_say "/bin/sh" ["\\uD800"] nothing
    do! run_say "" [] nothing
    do! run_say "/bin/sh" ["-c", "printf %s \\"$TZ_E08\\""] nothing
    do! run_say "/bin/sh" ["-c", "printf %s \\"$(pwd -P)\\""] nothing
    let! echoed = Process.run "cat" [] (big_input ())
    do! say (same_text echoed (big_input ()))
    let! ignored = Process.run "/bin/sh" ["-c", "exit 0"] (big_input ())
    do! say (sizes_text ignored)
    let! large = Process.run "/bin/sh" ["-c", "head -c 5000000 /dev/zero"] nothing
    do! say (sizes_text large)
    let! both = Process.run "/bin/sh" ["-c", "head -c 300000 /dev/zero; head -c 300000 /dev/zero >&2"] nothing
    do! say (sizes_text both)
    do! run_say "/bin/sh" ["-c", "head -c 1073741825 /dev/zero"] nothing
    do! run_say "/bin/sh" ["-c", "yes"] nothing
    let! before = count ()
    let! failures = spawn_many 300 0
    let! after = count ()
    do! say ("spawned failures:" + to_string failures + " same:" + to_string (before == after))
`);
  for (const optimization of optimizations) {
    const cwd = scratch("processes", optimization);
    writeFileSync(join(cwd, "plain.txt"), "not a program");
    chmodSync(join(cwd, "plain.txt"), 0o644);
    const output = lines(run(processes(optimization), cwd, { env: { TZ_E08: "v\u00e4rde", TZ_FD_DIR: process.platform === "linux" ? "/proc/self/fd" : "/dev/fd" } }));
    assert.deepEqual(output, [
      "code=0 signal=0 out=[[a b][; touch pwned][][$HOME][\u00e9]] err=[]",
      "code=0 signal=0 out=[hi] err=[]",
      "code=0 signal=0 out=[out] err=[err]",
      "code=7 signal=0 out=[] err=[]",
      "code=-1 signal=9 out=[] err=[]",
      "code=-1 signal=15 out=[] err=[]",
      message("not found", errno.ENOENT),
      message("permission denied", errno.EACCES),
      "invalid input", "invalid input", "invalid encoding", "invalid input",
      "code=0 signal=0 out=[v\u00e4rde] err=[]",
      `code=0 signal=0 out=[${realpathSync(cwd)}] err=[]`,
      "code=0 same=true length=1048576",
      "code=0 out=0 err=0",
      "code=0 out=5000000 err=0",
      "code=0 out=300000 err=300000",
      // The output of both streams together stops at 2^30 bytes: the child is killed and the call fails, even for a child that never stops writing.
      message("other", errno.EFBIG),
      message("other", errno.EFBIG),
      "spawned failures:0 same:true",
    ]);
    assert.ok(!existsSync(join(cwd, "pwned")), "an argument is never run as shell code");
  }

  // 18. The same programs on the WASI host (--wasm-host wasi), run by Node's WASI with the case directory preopened as /work.
  // Native and WASI runs of one program must agree on every result except the system's own error codes.
  const wasiHost = resolve("tests/os-wasi-host.mjs");
  const wasiModules = new Map();
  const wasiBuild = (name, optimization) => {
    const key = `${name}${optimization}`;
    if (!wasiModules.has(key)) {
      const output = join(root, `wasi-${name}${optimization}.wasm`);
      execute(compiler, ["build", join(root, `project-${name}`), "--target", "wasm32", "--wasm-host", "wasi", optimization, "-o", output]);
      wasiModules.set(key, output);
    }
    return wasiModules.get(key);
  };
  const runWasi = (name, optimization, cwd, { args = [], env = {}, input, status = 0 } = {}) => {
    const result = execute(process.execPath, ["--no-warnings", wasiHost, wasiBuild(name, optimization), "/work", cwd, ...args], {
      env: { ...process.env, TZ_WASI_ENV: JSON.stringify(env) }, input,
    }, false);
    assert.equal(result.status, status, `${name}${optimization}\n${result.stdout}\n${result.stderr}`);
    return result;
  };
  const withoutCodes = list => list.map(line => line.replace(/ \(os error \d+\)/g, ""));
  const wasiCode = { ENOENT: 44, EEXIST: 20, EISDIR: 31, ENOTDIR: 54, ENOTEMPTY: 55, EBADF: 8 };
  /** Names, types, link targets, and contents under a directory, for comparing two runs. */
  const tree = (directory, prefix = "") => readdirSync(directory).sort().flatMap(name => {
    const path = join(directory, name), label = `${prefix}${name}`, stat = lstatSync(path);
    if (stat.isSymbolicLink()) return [`${label} -> ${readlinkSync(path)}`];
    if (stat.isDirectory()) {
      try { return [`${label}/`, ...tree(path, `${label}/`)]; } catch { return [`${label}/ unreadable`]; }
    }
    // A named pipe would block a read, so only regular files are read.
    if (!stat.isFile()) return [`${label}:special`];
    try { return [`${label}:${readFileSync(path).toString("hex")}`]; } catch { return [`${label}:unreadable`]; }
  });
  const fixtures = {
    files: () => {},
    failures: cwd => {
      writeFileSync(join(cwd, "locked.txt"), "secret");
      chmodSync(join(cwd, "locked.txt"), 0o000);
      writeFileSync(join(cwd, "binary.bin"), Buffer.from([0xff, 0xfe, 0x41]));
      mkdirSync(join(cwd, "folder"));
    },
    directories: cwd => {
      mkdirSync(join(cwd, "listing"));
      for (const name of ["b", "a", "A", "ab", "\u00e9", "\uff61", "\u{1f600}", "z.txt"]) writeFileSync(join(cwd, "listing", name), "");
      mkdirSync(join(cwd, "target"));
      writeFileSync(join(cwd, "target", "a.txt"), "payload");
      symlinkSync("a.txt", join(cwd, "target", "link"));
      mkdirSync(join(cwd, "realdir"));
      symlinkSync("realdir", join(cwd, "dirlink"));
    },
    handles: cwd => {
      mkdirSync(join(cwd, "folder"));
      writeFileSync(join(cwd, "data.txt"), "abcdef");
    },
    metadata: prepareMetadata,
    walks: prepareWalk,
  };
  const programs = { files, failures, directories, handles, metadata: metadataProgram, walks };
  // Modification times differ between two fresh directories, so only the stamped file keeps its.
  const withoutTimes = list => list.map(line => line.replace(/^(file|dir|link|other) (\d+) \d+$/, "$1 $2"));
  for (const [name, prepare] of Object.entries(fixtures)) {
    for (const optimization of optimizations) {
      const nativeDirectory = scratch(`native-${name}`, optimization), wasiDirectory = scratch(`wasi-${name}`, optimization);
      prepare(nativeDirectory);
      prepare(wasiDirectory);
      const native = lines(run(programs[name](optimization), nativeDirectory));
      const wasi = lines(runWasi(name, optimization, wasiDirectory));
      assert.deepEqual(withoutTimes(withoutCodes(wasi)), withoutTimes(withoutCodes(native)), `${name}${optimization}`);
      assert.deepEqual(tree(wasiDirectory), tree(nativeDirectory), `${name}${optimization} leaves the same files`);
      if (name === "files") {
        assert.equal(wasi[8], message("not found", wasiCode.ENOENT));
        assert.equal(wasi[11], "ok:");
      }
      if (name === "directories") {
        assert.equal(wasi[1], message("already exists", wasiCode.EEXIST));
        assert.equal(wasi[6], message("not found", wasiCode.ENOTDIR));
        assert.equal(wasi[7], message("other", wasiCode.ENOTEMPTY));
      }
      if (name === "handles") assert.equal(wasi[0], `ok,ok,ok,ok,${message("invalid input", wasiCode.EBADF)},${message("invalid input", wasiCode.EBADF)}`);
      if (name === "failures") chmodSync(join(wasiDirectory, "locked.txt"), 0o600);
      if (name === "failures") chmodSync(join(nativeDirectory, "locked.txt"), 0o600);
      if (name === "metadata") {
        assert.equal(wasi[0], statLine(join(wasiDirectory, "data.txt"), "file"), "WASI reports the modification time");
        assert.equal(wasi[10], "file 3 1700000000250000000");
      }
      if (name === "walks") {
        restoreWalk(wasiDirectory);
        restoreWalk(nativeDirectory);
      }
    }
  }
  for (const optimization of optimizations) {
    const cwd = scratch("wasi-environment", optimization);
    const result = runWasi("environment", optimization, cwd, { args: ["\u03b1", "", "b c"], env: { TZ_E08: "v\u00e4rde" } });
    assert.deepEqual(lines(result), ["ok:3:\u03b1||b c", "some:v\u00e4rde", "none", "invalid input", "invalid input", "ok:/work"]);
    const timed = lines(runWasi("timing", optimization, cwd));
    assert.ok(BigInt(timed[0].slice("elapsed:".length)) >= 50_000_000n, timed[0]);
    assert.equal(timed[1], "ok");
    const difference = BigInt(timed[2].slice("unix:".length)) - BigInt(Date.now()) * 1_000_000n;
    assert.ok(difference < 60_000_000_000n && difference > -60_000_000_000n, timed[2]);
    assert.deepEqual(timed.slice(3), ["invalid input", "ok"]);
    assert.deepEqual(lines(runWasi("random", optimization, cwd)), [
      "ok:", "lengths:1000,1000 same:false", "invalid input", "invalid input", "word", "length:1048576",
    ]);
    assert.deepEqual(lines(runWasi("randommaps", optimization, cwd)), randomMapLines);
    const published = lines(runWasi("pcg", optimization, cwd));
    assert.equal(published[0], "2707161783 2068313097 3122475824 2211639955 3215226955 3421331566 ");
    assert.equal(runWasi("paths", optimization, cwd).stdout, expectedPaths);
  }

  // A process cannot be started from WASI.
  program("spawn-wasi", `
def main :: IO<unit> =
    let! result = Process.run "true" [] []
    do! say (match result with
        | Result.Ok _ -> "started"
        | Result.Error error -> Os.message (ref error))
`);
  for (const optimization of optimizations) {
    assert.equal(runWasi("spawn-wasi", optimization, scratch("wasi-spawn", optimization)).stdout, "other (os error 52)\n");
  }

  // The exit code of an IO<i32> entry reaches the WASI host through proc_exit, and only that program imports it.
  const wasiImports = (name, optimization) => WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(wasiBuild(name, optimization)))).map(({ module, name: field }) => `${module}.${field}`).sort();
  for (const optimization of optimizations) {
    const three = runWasi("exit-i32-3i32", optimization, scratch("wasi-exit", optimization), { status: 3 });
    assert.equal(three.stdout, "done\n");
    for (const name of ["exit-i32-0i32", "exit-i32-256i32", "exit-i64-5", "exit-unit-()"]) {
      assert.equal(runWasi(name, optimization, scratch("wasi-exit", optimization)).stdout, "done\n");
    }
    assert.deepEqual(wasiImports("exit-i32-3i32", optimization), ["wasi_snapshot_preview1.fd_write", "wasi_snapshot_preview1.proc_exit"]);
    assert.deepEqual(wasiImports("exit-unit-()", optimization), ["wasi_snapshot_preview1.fd_write"]);
  }

  // Standard input and output use the same host, with the contract of the native runtime.
  const wasiIo = program("wasi-io", `
def main :: IO<unit> =
    let! _prompt = IO.write "Name: "
    let! line = IO.read_line ()
    do! IO.write_line (Maybe.default_value "<eof>" line)
    do! IO.write_error_line "note"
`);
  for (const optimization of optimizations) {
    for (const [input, line] of [
      ["hello\r\n", "hello"], ["", "<eof>"], ["\n", ""], ["\r\n", ""], ["tail", "tail"], ["tail\r", "tail\r"],
      ["x".repeat(131_072) + "\u65e5\ud83d\ude00\n", "x".repeat(131_072) + "\u65e5\ud83d\ude00"],
      ["h\u00e9llo\0\u65e5\u672c\ud83d\ude00\r\n", "h\u00e9llo\0\u65e5\u672c\ud83d\ude00"],
    ]) {
      const result = runWasi("wasi-io", optimization, scratch("wasi-io", optimization), { input });
      assert.equal(result.stdout, `Name: ${line}\n`);
      assert.equal(result.stderr, "note\n");
    }
    for (const bytes of [Buffer.from([0x80, 10]), Buffer.from([0xed, 0xa0, 0x80, 10])]) {
      const result = execute(process.execPath, ["--no-warnings", wasiHost, wasiBuild("wasi-io", optimization), "/work", scratch("wasi-io", optimization)], { input: bytes }, false);
      assert.notEqual(result.status, 0, "invalid UTF-8 input traps like the native runtime");
    }
    assert.deepEqual(wasiImports("wasi-io", optimization), ["wasi_snapshot_preview1.fd_read", "wasi_snapshot_preview1.fd_write"]);
  }

  // The option and its errors; without a host the default output keeps its tsuzuri_io imports.
  const wasiProject = join(root, "project-wasi-io");
  const build = (...extra) => execute(compiler, ["build", wasiProject, ...extra, "-o", join(root, "wasi-cli-output")], {}, false);
  for (const [extra, expected] of [
    [["--target", "wasm32", "--wasm-host", "nope"], "unknown wasm host 'nope'; the supported host is wasi"],
    [["--target", "wasm32", "--wasm-host", "wasi", "--wasm-host", "wasi"], "--wasm-host specified more than once"],
    [["--wasm-host", "wasi"], "--wasm-host wasi requires wasm32 object or WASM output"],
    [["--target", "wasm64", "--wasm-host", "wasi"], "--wasm-host wasi requires wasm32 object or WASM output"],
    [["--target", "wasm32", "--emit", "llvm", "--wasm-host", "wasi"], "--wasm-host wasi requires wasm32 object or WASM output"],
    [["--target", "wasm32", "--emit", "header", "--wasm-host", "wasi"], "--wasm-host wasi requires wasm32 object or WASM output"],
    [["--target", "wasm32", "--wasm-feature", "threads", "--emit", "object", "--wasm-host", "wasi"], "--wasm-host wasi cannot be combined with --wasm-feature threads"],
  ]) {
    const result = build(...extra);
    assert.equal(result.status, 2, `${extra}\n${result.stderr}`);
    assert.ok(result.stderr.includes(expected), `${extra}\n${result.stderr}`);
  }
  assert.ok(!existsSync(join(root, "wasi-cli-output")), "a rejected option writes nothing");
  assert.ok(execute(compiler, ["run", wasiProject, "--wasm-host", "wasi"], {}, false).stderr.includes("--wasm-host is only valid with build"));
  // LLVM IR keeps the tsuzuri_io imports of the default output: the WASI runtime is compiled in only for objects and WASM.
  const imports = ir => ir.split("\n").filter(line => line.startsWith("declare") && line.includes("@tsuzuri_io_"));
  const plain = join(root, "wasi-plain.ll");
  execute(compiler, ["build", wasiProject, "--target", "wasm32", "--emit", "llvm", "-o", plain]);
  assert.equal(imports(readFileSync(plain, "utf8")).length, 2);
  assert.ok(imports(readFileSync(plain, "utf8")).every(line => line.includes('"wasm-import-module"="tsuzuri_io"')));
  const hostedObject = join(root, "wasi-hosted.o");
  execute(compiler, ["build", join(root, "project-files"), "--target", "wasm32", "--emit", "object", "--wasm-host", "wasi", "-o", hostedObject]);
  assert.deepEqual([...readFileSync(hostedObject).subarray(0, 4)], [0, 0x61, 0x73, 0x6d]);
  for (const optimization of optimizations) {
    const plainModule = join(root, `wasi-default${optimization}.wasm`);
    execute(compiler, ["build", wasiProject, "--target", "wasm32", optimization, "-o", plainModule]);
    assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(plainModule))).map(({ module, name: field }) => `${module}.${field}`).sort(), ["tsuzuri_io.read_line", "tsuzuri_io.write"]);
  }

  // A file of unknown size (a FIFO) exercises the growing read loop.
  const fifoReader = program("fifo", `
def sum_text :: Result<[ubyte], Os.Error> -> string
fn sum_text result =
    match result with
    | Result.Ok data ->
        let mut sum = 0
        let mut index = 0
        while index < data.length do
            sum = (sum * 31 + (data[index] as i64)) % 1000000007
            index = index + 1
        "ok:" + to_string data.length + ":" + to_string sum
    | Result.Error error -> Os.message (ref error)

def main :: IO<unit> =
    let! bytes = File.read_bytes "pipe"
    do! say (sum_text bytes)
`);
  for (const optimization of optimizations) {
    const cwd = scratch("fifo", optimization);
    execute("mkfifo", [join(cwd, "pipe")]);
    const payload = Buffer.alloc(300_000);
    for (let index = 0; index < payload.length; index++) payload[index] = (index * 7 + (index >> 8)) & 255;
    let sum = 0n;
    for (const byte of payload) sum = (sum * 31n + BigInt(byte)) % 1000000007n;
    const child = spawn(fifoReader(optimization), [], { cwd, stdio: ["ignore", "pipe", "inherit"] });
    let stdout = "";
    child.stdout.on("data", chunk => { stdout += chunk; });
    const finished = new Promise(resolveChild => child.once("close", resolveChild));
    const writer = createWriteStream(join(cwd, "pipe"));
    await new Promise((resolveWrite, rejectWrite) => { writer.once("error", rejectWrite); writer.end(payload, resolveWrite); });
    assert.equal(await finished, 0);
    assert.equal(stdout, `ok:${payload.length}:${sum}\n`);
  }

  // 16. Allocation tracking plus ASan and UBSan: the runtime's own temporaries and every owned result are freed.
  const tracked = program("tracked", `
def size_text :: Result<File.Metadata, Os.Error> -> string
fn size_text result =
    match result with
    | Result.Ok meta -> "size:" + to_string meta.size
    | Result.Error error -> Os.message (ref error)

def count_text :: Result<[string], Os.Error> -> string
fn count_text result =
    match result with
    | Result.Ok names -> "walked:" + to_string names.length
    | Result.Error error -> Os.message (ref error)

def output_length_text :: Result<Process.Output, Os.Error> -> string
fn output_length_text result =
    match result with
    | Result.Ok output -> "ran:" + to_string output.stdout.length
    | Result.Error error -> Os.message (ref error)

def handle_text :: Result<File.Handle, Os.Error> -> IO<string>
fn handle_text opened =
    match opened with
    | Result.Error error -> IO.pure (Os.message (ref error))
    | Result.Ok handle -> IO {
        let! wrote = File.write handle [1ubyte, 2ubyte, 3ubyte]
        let! closed = File.close handle
        let! again = File.open "s.bin" File.Read
        return! match again with
            | Result.Error error -> IO.pure (Os.message (ref error))
            | Result.Ok reader -> IO {
                let! bytes = File.read reader 100
                let! finished = File.close reader
                return unit_text wrote + "," + unit_text closed + "," + length_text bytes + "," + unit_text finished
            }
    }

def first_part :: unit -> IO<unit>
fn first_part _unit = IO {
    let! written = File.write_text "t.txt" "tracked \\u65e5\\u672c"
    let! read = File.read_text "t.txt"
    let! bytes = File.read_bytes "t.txt"
    let! created = Dir.create "tracked-dir"
    let! listed = Dir.list "."
    let! args = Env.args ()
    do! say (unit_text written)
    do! say (text_text read)
    do! say (bytes_text bytes)
    do! say (unit_text created)
    do! say (names_text listed)
    do! say (names_text args)
}

def second_part :: unit -> IO<unit>
fn second_part _unit = IO {
    let! home = Env.var "TZ_E08"
    let! cwd = Env.current_dir ()
    let! random = Random.bytes 300
    let! missing = File.read_text "nope"
    let! removed = File.remove "t.txt"
    let! dir_removed = Dir.remove "tracked-dir"
    do! say (option_text home)
    do! say (text_text cwd)
    do! say (length_text random)
    do! say (text_text missing)
    do! say (unit_text removed)
    do! say (unit_text dir_removed)
}

def third_part :: unit -> IO<unit>
fn third_part _unit = IO {
    let! streamed = File.open "s.bin" File.Write
    let! streams = handle_text streamed
    let! described = File.metadata "."
    let! walked = Dir.walk "."
    let! ran = Process.run "cat" ["-"] [104ubyte, 105ubyte]
    do! say streams
    do! say (size_text described)
    do! say (count_text walked)
    do! say (output_length_text ran)
}

def main :: IO<unit> = IO {
    do! first_part ()
    do! second_part ()
    do! third_part ()
}
`);
  tracked(optimizations[0]);
  const irPath = join(root, "tracked.ll");
  execute(compiler, ["build", join(root, "project-tracked"), "--emit", "llvm", "-o", irPath]);
  writeFileSync(irPath, readFileSync(irPath, "utf8").replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc"));
  const harness = join(root, "tracked-host.c");
  writeFileSync(harness, `#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
extern int32_t tsuzuri_main(void);
extern void tsuzuri_os_set_args(int32_t, char **);
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
// The runtime's own temporaries are counted as well: os.c is compiled with these in place of the C library's.
static int64_t temporaries;
void *tz_test_malloc(size_t size) { void *value = malloc(size); if (value) temporaries++; return value; }
void *tz_test_calloc(size_t count, size_t size) { void *value = calloc(count, size); if (value) temporaries++; return value; }
void *tz_test_realloc(void *pointer, size_t size) {
    void *value = realloc(pointer, size);
    if (value && !pointer) temporaries++;
    return value;
}
void tz_test_free(void *pointer) { if (pointer) temporaries--; free(pointer); }
char *tz_test_strdup(const char *text) { char *value = strdup(text); if (value) temporaries++; return value; }
int main(int argc, char **argv) {
    tsuzuri_os_set_args(argc, argv);
    for (int index = 0; index < 64; index++) {
        assert(tsuzuri_main() == 0);
        assert(live == 0);
        assert(temporaries == 0);
    }
    return 0;
}
`);
  const runtime = join(root, "os-tracked.c");
  const original = readFileSync("src/runtime/os.c", "utf8");
  assert.ok(original.includes("#include <dirent.h>"));
  writeFileSync(runtime, original.replace("#include <dirent.h>", `#include <stdlib.h>
#include <string.h>
#define malloc tz_test_malloc
#define calloc tz_test_calloc
#define realloc tz_test_realloc
#define free tz_test_free
#define strdup tz_test_strdup
void *tz_test_malloc(size_t);
void *tz_test_calloc(size_t, size_t);
void *tz_test_realloc(void *, size_t);
void tz_test_free(void *);
char *tz_test_strdup(const char *);
#include <dirent.h>`));
  for (const optimization of optimizations) {
    const executable = join(root, `tracked${optimization}`);
    execute(clang, [optimization, "-fsanitize=address,undefined", "-fno-omit-frame-pointer", "-Wno-override-module", irPath, runtime, "src/runtime/io.c", harness, "-lm", "-o", executable]);
    const result = run(executable, scratch("tracked", optimization), { args: ["one", "two"], env: { TZ_E08: "tracked-value" } });
    assert.equal(lines(result).length, 16 * 64);
  }
  console.log("OS: native and wasm32 File, Dir, Path, Env, Time, Random, exit codes, and allocation tracking at -O0 and -O3 passed");
} finally {
  rmSync(root, { recursive: true, force: true });
}
