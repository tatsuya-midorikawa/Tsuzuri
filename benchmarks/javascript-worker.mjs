import { parentPort, workerData } from "node:worker_threads";

const { count, seed, first, stride } = workerData;
let total = 0n;
for (let index = first; index < 16; index += stride) {
  let state = seed ^ BigInt(index);
  for (let remaining = count; remaining > 0; --remaining) {
    state = BigInt.asUintN(64, (state ^ (state >> 13n)) * 6364136223846793005n + 1442695040888963407n);
  }
  total = BigInt.asUintN(64, total + state);
}
parentPort.postMessage(total);