import assert from "node:assert/strict";

// Independent references of benchmarks/heap/Main.tz in wrapping 64-bit unsigned BigInt arithmetic.
export const wrap = (value) => BigInt.asUintN(64, value);
export const hex = (value) => wrap(value).toString(16).padStart(16, "0");
const step = (state) => wrap((state ^ (state >> 13n)) * 6364136223846793005n + 1442695040888963407n);

export function steadyChurn(slots, rounds, spread, seed) {
  const bound = BigInt(spread), count = BigInt(slots);
  let state = wrap(seed);
  // A block is its salt and its length: its first element is the salt, its last the salt plus length - 1.
  const rows = [];
  for (let index = 0; index < slots; ++index) {
    state = step(state);
    rows.push([state, 1n + (state >> 40n) % bound]);
  }
  for (let round = 0; round < rounds; ++round) {
    state = step(state);
    rows[Number((state >> 33n) % count)] = [state, 1n + (state >> 20n) % bound];
  }
  let total = 0n;
  for (const [salt, length] of rows) total = wrap(total * 31n + salt + wrap(salt + length - 1n) + length);
  return total;
}

export function vecGrowth(count, rounds, seed) {
  let total = 0n;
  for (let round = 0; round < rounds; ++round) {
    for (let index = 0; index < count; ++index) total = wrap(total * 31n + wrap(seed + BigInt(index) + BigInt(round)));
  }
  return total;
}

export function checkReference() {
  // One block of one element in one slot, never replaced: salt, salt + 1 - 1 and the length 1 are summed.
  assert.equal(steadyChurn(1, 0, 1, 0n), wrap(2n * step(0n) + 1n));
  assert.equal(vecGrowth(0, 5, 9n), 0n);
  assert.equal(vecGrowth(3, 1, 10n), (10n * 31n + 11n) * 31n + 12n);
}
