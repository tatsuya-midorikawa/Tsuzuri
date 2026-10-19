// The profile writer of a WebAssembly module that `tsuzuri build --target wasm32 --profile-generate DIR` instrumented.
//
//   import { writeProfile } from "./app.wasm.profile.mjs";
//   const { instance } = await WebAssembly.instantiate(await readFile("app.wasm"));
//   instance.exports.tz_run(1000n);   // the workload that the program should be tuned for
//   await writeProfile(instance);     // a new file, DIR/default_wasm_<pid>_<n>.profraw
//
// A file holds what the module counted since the call before: once it is written, the counters of that instance start from zero
// again (a write that fails gives them back). Every call writes a file of its own and never replaces one. Each module has a
// writer of its own, each worker thread loads a copy of it, and all of them count their files from 0 and share the process id, so
// a name that is taken is skipped. Any number of calls, modules and threads therefore leave files that
// `tsuzuri build --target wasm32 --profile-use DIR` merges into exactly what was counted. `profileBytes(instance)` only reads: it
// holds everything counted since the last file and does not start the counters again, so a host that saves it more than once
// (a browser has no `writeProfile`) must keep the last bytes only, or the earlier counts are merged twice.
//
// A WebAssembly module has no profile runtime, so it only counts in its memory. This file turns those counters into an LLVM
// raw profile (version 10, a 32-bit program), which `tsuzuri build --target wasm32 --profile-use DIR` merges with
// llvm-profdata. It reads the layout that Clang 21 instruments with, and refuses a module that another version built.

/** The directory that --profile-generate named; `writeProfile` writes there unless it is given another. */
export const defaultDirectory = __TSUZURI_PROFILE_DIRECTORY__;

const RAW_VERSION = 10n;
const RECORD_BYTES = 48;
const HEADER_BYTES = 16 * 8;
const OUTSIDE = "the profile sections of the module are not inside its memory";
let written = 0;

/** Where the profile sections of a module are, after checking that they have the layout that this writer reads. */
function locate(exports) {
  const { memory } = exports;
  if (!(memory instanceof WebAssembly.Memory)) throw new Error("the module does not export its memory");
  const address = (name) => {
    const symbol = exports[name];
    if (!(symbol instanceof WebAssembly.Global)) {
      throw new Error(`the module does not export ${name}: build it with tsuzuri build --target wasm32 --profile-generate`);
    }
    // An i32 global reads as a signed number, and the sections of a module whose stack is 2 GiB or more are above 2^31.
    return symbol.value >>> 0;
  };
  const versionAt = address("__llvm_profile_raw_version");
  if (versionAt + 8 > memory.buffer.byteLength) throw new Error(OUTSIDE);
  const version = new DataView(memory.buffer).getBigUint64(versionAt, true);
  if ((version & 0xffffffffn) !== RAW_VERSION) {
    throw new Error(`the module counts in raw profile version ${version & 0xffffffffn}; this writer supports version 10 (Clang 21)`);
  }
  const section = (name) => [address(`__start___llvm_prf_${name}`), address(`__stop___llvm_prf_${name}`)];
  const [counters, data, names] = [section("cnts"), section("data"), section("names")];
  if ((data[1] - data[0]) % RECORD_BYTES !== 0 || (counters[1] - counters[0]) % 8 !== 0) {
    throw new Error("the profile sections of the module do not have the layout of raw profile version 10 with 32-bit pointers");
  }
  if ([counters, data, names].some(([start, end]) => end < start || end > memory.buffer.byteLength)) throw new Error(OUTSIDE);
  return { memory, version, counters, data, names };
}

/** The bytes of the raw profile that the counters of `instance` hold now. It only reads; `writeProfile` starts the counters again. */
export function profileBytes(instance) {
  return copy(locate(instance.exports));
}

function copy({ memory, version, counters, data, names }) {
  const padding = (8 - ((names[1] - names[0]) % 8)) % 8;
  const bytes = new Uint8Array(HEADER_BYTES + (data[1] - data[0]) + (counters[1] - counters[0]) + (names[1] - names[0]) + padding);
  const header = new DataView(bytes.buffer);
  [
    0xff6c70726f665281n, // the magic of a 32-bit program
    version,
    0n, // size of the binary ids
    BigInt((data[1] - data[0]) / RECORD_BYTES),
    0n, // padding before the counters
    BigInt((counters[1] - counters[0]) / 8),
    0n, // padding after the counters
    0n, // bitmap bytes
    0n, // padding after the bitmap bytes
    BigInt(names[1] - names[0]),
    BigInt((counters[0] - data[0]) >>> 0), // the difference of two 32-bit addresses, as a 32-bit program writes it
    0n, // bitmap delta
    BigInt(names[0]),
    0n, // virtual tables
    0n, // their names
    2n, // the last kind of value profile
  ].forEach((field, index) => header.setBigUint64(index * 8, field, true));
  const source = new Uint8Array(memory.buffer);
  let at = HEADER_BYTES;
  for (const [start, end] of [data, counters, names]) {
    bytes.set(source.subarray(start, end), at);
    at += end - start;
  }
  return bytes;
}

/** Adds the counts of a profile that could not be saved back to the counters, which have gone on counting since. */
function giveBack({ memory, counters, data }, bytes) {
  const saved = new DataView(bytes.buffer, HEADER_BYTES + (data[1] - data[0]), counters[1] - counters[0]);
  const view = new DataView(memory.buffer);
  for (let offset = 0; offset < saved.byteLength; offset += 8) {
    const at = counters[0] + offset;
    view.setBigUint64(at, view.getBigUint64(at, true) + saved.getBigUint64(offset, true), true);
  }
}

/**
 * Saves the profile of `instance` in `directory` (which is made if it is not there) and returns the path of the file. Node only.
 * The file is created exclusively: a name that exists is skipped for the next number, so that the profile of another module, of
 * a worker thread, or of an earlier process with the same id is never replaced. It holds what the module counted since the call
 * before, and the counters start from zero again. A write that fails leaves no half of a profile and gives the counts back.
 */
export async function writeProfile(instance, directory = defaultDirectory) {
  const found = locate(instance.exports);
  const [{ mkdir, open, unlink }, { join }] = await Promise.all([import("node:fs/promises"), import("node:path")]);
  await mkdir(directory, { recursive: true });
  for (;;) {
    const path = join(directory, `default_wasm_${process.pid}_${written++}.profraw`);
    let file;
    try {
      file = await open(path, "wx");
    } catch (error) {
      if (error.code === "EEXIST") continue;
      throw error;
    }
    // The counts are taken and the counters cleared in one step, so that what the module counts while the file is written, or
    // in a call that overlaps this one, is neither lost nor in two files.
    let bytes;
    try {
      bytes = copy(found);
      new Uint8Array(found.memory.buffer).fill(0, found.counters[0], found.counters[1]);
      await file.writeFile(bytes);
      await file.close();
    } catch (error) {
      if (bytes) giveBack(found, bytes);
      await file.close().catch(() => {});
      await unlink(path).catch(() => {});
      throw error;
    }
    return path;
  }
}
