export async function loadPhysics(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Cannot load physics.wasm (${response.status}). Build the .tz example first.`);
  }
  const { instance } = await WebAssembly.instantiateStreaming(response);
  const api = instance.exports;
  if (typeof api.tz_next_position !== "function" || typeof api.tz_next_velocity !== "function") {
    throw new Error("The WASM module does not export the expected Tsuzuri physics ABI.");
  }
  return api;
}

export function stepBody(api, body, dt, width, height) {
  const x = body.x - body.radius;
  const y = body.y - body.radius;
  const extentX = width - 2 * body.radius;
  const extentY = height - 2 * body.radius;
  return {
    ...body,
    x: api.tz_next_position(x, body.vx, dt, extentX) + body.radius,
    y: api.tz_next_position(y, body.vy, dt, extentY) + body.radius,
    vx: api.tz_next_velocity(x, body.vx, dt, extentX),
    vy: api.tz_next_velocity(y, body.vy, dt, extentY),
  };
}

export function stepPositions(api, positions, velocities, dt, extent) {
  let positionPointer = 0, velocityPointer = 0, outputPointer = 0, resultPointer = 0;
  try {
    // wasm32 pointers at or above 2 GiB arrive as negative numbers.
    positionPointer = api.tsuzuri_alloc(BigInt(positions.length) * 8n) >>> 0;
    velocityPointer = api.tsuzuri_alloc(BigInt(velocities.length) * 8n) >>> 0;
    outputPointer = api.tsuzuri_alloc(16n) >>> 0;
    new Float64Array(api.memory.buffer, positionPointer, positions.length).set(positions);
    new Float64Array(api.memory.buffer, velocityPointer, velocities.length).set(velocities);
    api.tz_next_positions(outputPointer, positionPointer, BigInt(positions.length), velocityPointer, BigInt(velocities.length), dt, extent);
    const view = new DataView(api.memory.buffer);
    resultPointer = view.getUint32(outputPointer, true);
    const length = Number(view.getBigInt64(outputPointer + 8, true));
    return new Float64Array(api.memory.buffer, resultPointer, length).slice();
  } finally {
    api.tsuzuri_free(resultPointer);
    api.tsuzuri_free(outputPointer);
    api.tsuzuri_free(velocityPointer);
    api.tsuzuri_free(positionPointer);
  }
}
