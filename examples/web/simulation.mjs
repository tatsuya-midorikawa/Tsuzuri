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
