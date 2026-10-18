// The profile writer of a WebAssembly module that `tsuzuri build --target wasm32 --profile-generate DIR` instrumented.
//
//   import { writeProfile } from "./app.wasm.profile.mjs";
//   const { instance } = await WebAssembly.instantiate(await readFile("app.wasm"));
//   instance.exports.tz_run(1000n);   // the workload that the program should be tuned for
//   await writeProfile(instance);     // DIR/default_wasm_<pid>_<n>.profraw
//
// A WebAssembly module has no profile runtime, so it only counts in its memory. This file turns those counters into an LLVM
// raw profile (version 10, a 32-bit program), which `tsuzuri build --target wasm32 --profile-use DIR` merges with
// llvm-profdata. It reads the layout that Clang 21 instruments with, and refuses a module that another version built.

/** The directory that --profile-generate named; `writeProfile` writes there unless it is given another. */
export const defaultDirectory = __TSUZURI_PROFILE_DIRECTORY__;

const RAW_VERSION = 10n;
const RECORD_BYTES = 48;
const HEADER_BYTES = 16 * 8;
let written = 0;

/** The bytes of the raw profile that the counters of `instance` hold now. */
export function profileBytes(instance) {
  const { exports } = instance;
  const address = (name) => {
    const symbol = exports[name];
    if (!(symbol instanceof WebAssembly.Global)) {
      throw new Error(`the module does not export ${name}: build it with tsuzuri build --target wasm32 --profile-generate`);
    }
    return symbol.value;
  };
  if (!(exports.memory instanceof WebAssembly.Memory)) throw new Error("the module does not export its memory");
  const memory = new Uint8Array(exports.memory.buffer);
  const view = new DataView(exports.memory.buffer);
  const version = view.getBigUint64(address("__llvm_profile_raw_version"), true);
  if ((version & 0xffffffffn) !== RAW_VERSION) {
    throw new Error(`the module counts in raw profile version ${version & 0xffffffffn}; this writer supports version 10 (Clang 21)`);
  }
  const section = (name) => [address(`__start___llvm_prf_${name}`), address(`__stop___llvm_prf_${name}`)];
  const [counters, data, names] = [section("cnts"), section("data"), section("names")];
  if ((data[1] - data[0]) % RECORD_BYTES !== 0 || (counters[1] - counters[0]) % 8 !== 0) {
    throw new Error("the profile sections of the module do not have the layout of raw profile version 10 with 32-bit pointers");
  }
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
  let at = HEADER_BYTES;
  for (const [start, end] of [data, counters, names]) {
    bytes.set(memory.subarray(start, end), at);
    at += end - start;
  }
  return bytes;
}

/** Saves the profile of `instance` in `directory` (which is made if it is not there) and returns the path of the file. Node only. */
export async function writeProfile(instance, directory = defaultDirectory) {
  const bytes = profileBytes(instance);
  const [{ mkdir, writeFile }, { join }] = await Promise.all([import("node:fs/promises"), import("node:path")]);
  await mkdir(directory, { recursive: true });
  const path = join(directory, `default_wasm_${process.pid}_${written++}.profraw`);
  await writeFile(path, bytes);
  return path;
}
