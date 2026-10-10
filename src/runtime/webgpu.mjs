// A RangeError that stands for a kernel or device limit (status 3 of the language runtime). Any other RangeError, such as
// a pointer outside the module's memory or a failed allocation, is a failure (status 4).
const limitError = message => new RangeError(message, { cause: "limit" });

// `options.features` lists the device features to require, such as "shader-f16" (F09 Phase 2).
export async function createWebGpu(gpu = globalThis.navigator?.gpu, options = {}) {
  if (!gpu?.requestAdapter) throw new Error("WebGPU is unavailable; no CPU fallback was selected");
  let adapter = await gpu.requestAdapter();
  if (!adapter || adapter.limits.maxComputeInvocationsPerWorkgroup < 256) {
    throw new Error("WebGPU adapter with 256-invocation workgroups is unavailable");
  }
  const features = options.features ?? [];
  for (const feature of features) {
    if (!adapter.features?.has(feature)) throw new Error(`WebGPU adapter lacks the required feature ${feature}`, { cause: "feature" });
  }
  let device = await adapter.requestDevice(features.length ? { requiredFeatures: features } : undefined);
  const info = Object.fromEntries(["vendor", "architecture", "device", "description"].map(key => [key, adapter.info?.[key] ?? ""]));
  const live = new Set();
  let pending = Promise.resolve();
  let closed = false;

  function checked(operation) {
    const job = pending.then(async () => {
      if (closed) throw new Error("WebGPU runtime is closed");
      device.pushErrorScope("out-of-memory");
      device.pushErrorScope("validation");
      let value;
      let failure;
      try { value = await operation(); } catch (error) { failure = error; }
      const validation = await device.popErrorScope();
      const allocation = await device.popErrorScope();
      if (failure || validation || allocation) {
        value?.destroy?.();
        throw failure ?? new Error(validation?.message ?? allocation.message);
      }
      return value;
    });
    pending = job.then(() => {}, () => {});
    return job;
  }

  function bytesFor(count, kind) {
    const bytes = count * (kind === "f16" ? 2 : 4);
    if (!Number.isSafeInteger(count) || count < 0 || count > 2147483647 || bytes > device.limits.maxStorageBufferBindingSize || bytes > device.limits.maxBufferSize) {
      throw limitError("GPU buffer length exceeds the kernel or device limit");
    }
    return Math.max(Math.ceil(bytes / 4) * 4, 4);
  }

  function own(buffer, count, kind) {
    const handle = { buffer, count, kind, destroy() { if (live.delete(handle)) buffer.destroy(); } };
    live.add(handle);
    return handle;
  }

  function take(handle) {
    if (!live.delete(handle)) throw new Error("GPU buffer is consumed or belongs to another device");
    return handle.buffer;
  }

  function dispatch(program, mode, count, input) {
    if (program.device !== device) throw new Error("GPU kernel belongs to another device");
    if (input && input.kind !== program.input) throw new TypeError("GPU buffer element type does not match the kernel input");
    const size = bytesFor(count, program.output);
    const inputBuffer = input ? take(input) : undefined;
    return checked(async () => {
      let output;
      let uniform;
      try {
        output = device.createBuffer({ size, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
        uniform = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
        device.queue.writeBuffer(uniform, 0, new Uint32Array([count, 0, 0, 0]));
        const pipeline = program[mode];
        const entries = [{ binding: 1, resource: { buffer: output } }, { binding: 2, resource: { buffer: uniform } }];
        if (inputBuffer) entries.unshift({ binding: 0, resource: { buffer: inputBuffer } });
        const bindings = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
        const encoder = device.createCommandEncoder();
        const pass = encoder.beginComputePass();
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindings);
        const groups = Math.ceil(count / 256);
        if (groups > device.limits.maxComputeWorkgroupsPerDimension) throw limitError("GPU dispatch exceeds the device workgroup limit");
        if (groups > 0) pass.dispatchWorkgroups(groups);
        pass.end();
        device.queue.submit([encoder.finish()]);
        await device.queue.onSubmittedWorkDone();
        return own(output, count, program.output);
      } catch (error) {
        output?.destroy();
        throw error;
      } finally {
        uniform?.destroy();
        inputBuffer?.destroy();
      }
    });
  }

  return {
    info,
    // `options.float` must be "relaxed" for a kernel from `--emit wgsl-relaxed` (F09): its results follow
    // WGSL's floating-point rules, so the host never accepts it by accident.
    prepare(source, options = {}) {
      const header = /^\/\/ tsuzuri-gpu float=relaxed input=(f32|f16|i32|u32) output=(f32|f16|i32|u32)\n/.exec(source);
      if (source.startsWith("// tsuzuri-gpu") && !header) return Promise.reject(new Error("unsupported tsuzuri-gpu WGSL header"));
      if (header && options.float !== "relaxed") return Promise.reject(new Error('relaxed floating-point WGSL requires prepare(source, { float: "relaxed" })'));
      const kind = type => (type === "f32" || type === "f16" ? type : "u32");
      const input = header ? kind(header[1]) : "u32";
      const output = header ? kind(header[2]) : "u32";
      if (source.includes("\nenable f16;\n") && !device.features?.has("shader-f16")) {
        return Promise.reject(new Error('this kernel needs the shader-f16 device feature: create the runtime with createWebGpu(gpu, { features: ["shader-f16"] })'));
      }
      return checked(async () => {
        const shader = device.createShaderModule({ code: source });
        const diagnostics = await shader.getCompilationInfo();
        const errors = diagnostics.messages.filter(message => message.type === "error");
        if (errors.length) throw new Error(errors.map(message => `${message.lineNum}:${message.linePos}: ${message.message}`).join("\n"));
        const map = await device.createComputePipelineAsync({ layout: "auto", compute: { module: shader, entryPoint: "map_main" } });
        const init = await device.createComputePipelineAsync({ layout: "auto", compute: { module: shader, entryPoint: "init_main" } });
        return { device, map, init, input, output };
      });
    },
    // `Uint16Array` holds the bits of f16 lanes (the buffer of a kernel that needs the `shader-f16` device feature).
    fromArray(values) {
      const kind = values instanceof Float32Array ? "f32" : values instanceof Uint16Array ? "f16" : values instanceof Int32Array || values instanceof Uint32Array ? "u32" : undefined;
      if (!kind) throw new TypeError("GPU buffers require Int32Array, Uint32Array, Float32Array, or Uint16Array");
      const size = bytesFor(values.length, kind);
      return checked(() => {
        const buffer = device.createBuffer({ size, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC });
        if (values.byteLength % 4) {
          const bytes = new Uint8Array(size);
          bytes.set(new Uint8Array(values.buffer, values.byteOffset, values.byteLength));
          device.queue.writeBuffer(buffer, 0, bytes);
        } else if (values.byteLength) {
          device.queue.writeBuffer(buffer, 0, values);
        }
        return own(buffer, values.length, kind);
      });
    },
    init(program, count) { return dispatch(program, "init", count); },
    map(program, buffer) { return dispatch(program, "map", buffer.count, buffer); },
    toArray(handle) {
      const buffer = take(handle);
      const ElementArray = handle.kind === "f32" ? Float32Array : handle.kind === "f16" ? Uint16Array : Uint32Array;
      return checked(async () => {
        let readback;
        try {
          if (!handle.count) return new ElementArray();
          const size = bytesFor(handle.count, handle.kind);
          readback = device.createBuffer({ size, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
          const encoder = device.createCommandEncoder();
          encoder.copyBufferToBuffer(buffer, 0, readback, 0, size);
          device.queue.submit([encoder.finish()]);
          await readback.mapAsync(GPUMapMode.READ);
          const result = new ElementArray(readback.getMappedRange().slice(0, handle.count * ElementArray.BYTES_PER_ELEMENT));
          readback.unmap();
          return result;
        } finally {
          readback?.destroy();
          buffer.destroy();
        }
      });
    },
    // One kernel run on bytes, for the language runtime (F09 Phase 2): uploads `input` (the bytes of `count` lanes,
    // or undefined for init), dispatches, waits, and reads `outputBytes` bytes back. Buffers are padded to a
    // multiple of 4 bytes, so 2-byte f16 lanes work. Nothing outlives the call.
    run(program, mode, count, input, outputBytes) {
      if (program.device !== device) return Promise.reject(new Error("GPU kernel belongs to another device"));
      const inputSize = input ? Math.ceil(input.byteLength / 4) * 4 : 0;
      const outputSize = Math.max(Math.ceil(outputBytes / 4) * 4, 4);
      const groups = Math.ceil(count / 256);
      if (!Number.isSafeInteger(count) || count < 0 || count > 2147483647 || Math.max(inputSize, outputSize) > Math.min(device.limits.maxStorageBufferBindingSize, device.limits.maxBufferSize) || groups > device.limits.maxComputeWorkgroupsPerDimension) {
        return Promise.reject(limitError("GPU buffer length exceeds the kernel or device limit"));
      }
      if (count === 0) return Promise.resolve(new Uint8Array());
      return checked(async () => {
        const created = [];
        const make = (size, usage) => { const buffer = device.createBuffer({ size, usage }); created.push(buffer); return buffer; };
        try {
          const output = make(outputSize, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
          const uniform = make(16, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
          const readback = make(outputSize, GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST);
          device.queue.writeBuffer(uniform, 0, new Uint32Array([count, 0, 0, 0]));
          const pipeline = program[mode === 0 ? "map" : "init"];
          const entries = [{ binding: 1, resource: { buffer: output } }, { binding: 2, resource: { buffer: uniform } }];
          if (input) {
            const upload = make(inputSize, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST);
            const padded = new Uint8Array(inputSize);
            padded.set(input);
            device.queue.writeBuffer(upload, 0, padded);
            entries.unshift({ binding: 0, resource: { buffer: upload } });
          }
          const bindings = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
          const encoder = device.createCommandEncoder();
          const pass = encoder.beginComputePass();
          pass.setPipeline(pipeline);
          pass.setBindGroup(0, bindings);
          pass.dispatchWorkgroups(groups);
          pass.end();
          encoder.copyBufferToBuffer(output, 0, readback, 0, outputSize);
          device.queue.submit([encoder.finish()]);
          await readback.mapAsync(GPUMapMode.READ);
          const result = new Uint8Array(readback.getMappedRange().slice(0, outputBytes));
          readback.unmap();
          return result;
        } finally {
          for (const buffer of created) buffer.destroy();
        }
      });
    },
    async close() {
      await pending;
      closed = true;
      for (const handle of live) handle.destroy();
      device.destroy();
      device = undefined;
      adapter = undefined;
      gpu = undefined;
    },
  };
}

// The imports `tsuzuri_gpu.open` and `tsuzuri_gpu.run` of a module built with `--wasm-feature webgpu` (F09 Phase 2).
// They are JSPI suspending functions over a WebGPU provider (`navigator.gpu`, or the Dawn binding of Node.js), so the
// module's exports must be wrapped with WebAssembly.promising. `getMemory()` returns the module's memory once it
// exists. The statuses are those of the native runtime: 0 success, 1 unavailable, 2 unsupported (a missing feature, or
// a call without a kernel), 3 limit exceeded, 4 failed; a failure also logs one line to the console, which a status
// of 2 or more makes the module follow with a trap. Set `options.debug` to log why `open` reports 1 or 2, and each dispatch.
export function createGpuImports(gpu, getMemory, options = {}) {
  if (typeof WebAssembly.Suspending !== "function") throw new Error("WebAssembly.Suspending is missing: use an engine with JavaScript Promise Integration");
  const report = message => console.error(`tsuzuri gpu: ${message}`);
  const programs = new Map();
  let runtime;
  let opening;
  let features = 0;
  const open = async (backend, needed) => {
    if (backend !== 1) return 1;
    opening ??= createWebGpu(gpu, { features: needed & 1 ? ["shader-f16"] : [] }).then(
      created => { runtime = created; features = needed; return 0; },
      error => { if (options.debug) report(`webgpu: ${error.message}`); return error.cause === "feature" ? 2 : 1; },
    );
    const status = await opening;
    return status === 0 && needed & ~features ? 2 : status;
  };
  const run = async (backend, mode, flags, lanes, wgsl, wgslLength, spirv, spirvLength, input, count, output) => {
    // The guest pointers are i32, so an address from 2 GiB up (a module with a large --wasm-max-memory) arrives negative.
    wgsl >>>= 0;
    input >>>= 0;
    output >>>= 0;
    if (backend !== 1 || !runtime) {
      report("webgpu: no device is open; a WebGPU device comes from Gpu.request Gpu.WebGpu");
      return 1;
    }
    if (!wgsl || wgslLength <= 0) {
      report("webgpu: this call has no WGSL kernel: a strict Gpu.map or Gpu.init runs on a WebGPU device only with i32 or i32u lanes; use Gpu.map_relaxed or Gpu.init_relaxed for f32 or f16");
      return 2;
    }
    count = Number(count);
    if (options.debug && count > 0) report(`webgpu: ${mode ? "init" : "map"} ${count} lanes, kinds 0x${lanes.toString(16).padStart(4, "0")}`);
    try {
      const lane = kind => (kind === 3 ? 2 : 4);
      const inputBytes = mode === 0 ? count * lane(lanes & 255) : 0;
      const outputBytes = count * lane((lanes >> 8) & 255);
      const size = getMemory().buffer.byteLength;
      if (wgsl + wgslLength > size || input + inputBytes > size || output + outputBytes > size) throw new Error("a buffer of the call lies outside the memory of the module");
      const key = `${wgsl}:${wgslLength}`;
      if (!programs.has(key)) {
        const source = new TextDecoder().decode(new Uint8Array(getMemory().buffer, wgsl, wgslLength));
        programs.set(key, runtime.prepare(source, flags & 1 ? { float: "relaxed" } : {}));
      }
      const program = await programs.get(key);
      const memory = getMemory();
      const bytes = mode === 0 ? new Uint8Array(memory.buffer.slice(input, input + inputBytes)) : undefined;
      const result = await runtime.run(program, mode, count, bytes, outputBytes);
      new Uint8Array(getMemory().buffer, output, result.byteLength).set(result);
      return 0;
    } catch (error) {
      report(`webgpu: ${error.message}`);
      return error?.cause === "limit" ? 3 : 4;
    }
  };
  return {
    imports: { open: new WebAssembly.Suspending(open), run: new WebAssembly.Suspending(run) },
    // The functions behind the imports, for a host that drives them without a module (the tests call them with the
    // signed i32 pointers that a module passes).
    functions: { open, run },
    async close() { await runtime?.close(); runtime = undefined; },
  };
}
