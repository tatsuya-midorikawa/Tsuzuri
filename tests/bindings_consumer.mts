// Type-checks the declarations that `--emit bindings-js` writes for tests/fixtures/bindings: correct
// calls compile and the marked mistakes are rejected. tests/bindings.mjs copies this file next to
// the generated bindings.mjs and bindings.d.mts and runs tsc --strict on it.
import { load, TsuzuriTrap, type Bindings, type Imports, type TrapSite, type tz_handle_4Main_7Counter, type tz_record_4Main_6Sample } from "./bindings.mjs";

declare const bytes: Uint8Array;
declare const sites: readonly TrapSite[];

const counters = new Map<tz_handle_4Main_7Counter, bigint>();
const imports: Imports = {
  "Main.now": () => 37n,
  "Main.host_scale": (values: Float64Array, factor: number) => values.map((value) => value * factor),
  "Main.host_greeting": (name: string) => `hello ${name}`,
  "Main.host_fail": (value: bigint) => value,
  "Main.host_note": (text: string) => { void text; },
  "Main.host_flags": (flags) => ({ flags: { ...flags, tiny: flags.tiny - 1 }, amount: 2.5 }),
  "Main.host_mix": (small: number, large: bigint, flag: boolean, narrow: number) => (flag ? large : BigInt(small + narrow)),
  e13_counter_new: (start) => {
    const handle = (counters.size + 1) as tz_handle_4Main_7Counter;
    counters.set(handle, start);
    return handle;
  },
  e13_counter_add: (handle, amount) => (counters.get(handle) ?? 0n) + amount,
  e13_counter_free: (handle) => counters.get(handle) ?? 0n,
  e13_apply: (callback, value) => callback(callback(value)),
  e13_visit: (callback, handle) => (callback(handle, 10) ? 1n : 0n),
};

const api: Bindings = await load(bytes, { imports, sites });
const sum: bigint = api.exports.add(40n, 2n);
const text: string = api.exports.copy_text("a\ud800");
const copied: BigInt64Array = api.exports.copy_values(BigInt64Array.of(1n));
const sample: tz_record_4Main_6Sample = api.exports.update({ flags: { tiny: -1, wide: 1, flag: true }, amount: 1.5 });
const window = api.exports.make_window(5);
const first: number | undefined = window.values[0];
const total: number = api.withBorrowed("f64", 3, (buffer) => {
  buffer.view().set([1, 2, 3]);
  return api.exports.sum_float(buffer);
});
const handle: tz_handle_4Main_7Counter = api.exports.pass_through(1 as tz_handle_4Main_7Counter);
try {
  api.exports.divide(1n, 0n);
} catch (error) {
  if (error instanceof TsuzuriTrap) {
    const reason: "trap" | "stack" = error.trap.reason;
    void reason;
  }
}
void [sum, text, copied, sample, first, total, handle];

// @ts-expect-error i64 arguments are bigint
api.exports.add(1, 2n);
// @ts-expect-error string arguments are strings
api.exports.copy_text(1);
// @ts-expect-error a Borrowed<Float64Array> is not a BigInt64Array buffer
api.withBorrowed("f64", 1, (buffer) => api.exports.copy_values(buffer));
// @ts-expect-error handles are branded numbers
api.exports.pass_through(1);
// @ts-expect-error records need every field
api.exports.update({ flags: { tiny: -1, wide: 1, flag: true } });
// @ts-expect-error an unknown kind of borrowed buffer
api.withBorrowed("i32", 1, () => 0);
// @ts-expect-error the host imports are required
await load(bytes);
// @ts-expect-error every import must be present
const partial: Imports = { "Main.now": () => 1n };
// @ts-expect-error import results are checked: host_scale returns a Float64Array
const wrong: Imports = { ...imports, "Main.host_scale": () => [1, 2] };
void [partial, wrong];
