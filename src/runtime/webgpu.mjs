export async function createWebGpu(gpu = globalThis.navigator?.gpu) {
  if (!gpu?.requestAdapter) throw new Error("WebGPU is unavailable; no CPU fallback was selected");
  let adapter = await gpu.requestAdapter();
  if (!adapter || adapter.limits.maxComputeInvocationsPerWorkgroup < 256) {
    throw new Error("WebGPU adapter with 256-invocation workgroups is unavailable");
  }
  let device = await adapter.requestDevice();
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

  function bytesFor(count) {
    if (!Number.isSafeInteger(count) || count < 0 || count > 2147483647 || count * 4 > device.limits.maxStorageBufferBindingSize || count * 4 > device.limits.maxBufferSize) {
      throw new RangeError("GPU buffer length exceeds the kernel or device limit");
    }
    return Math.max(count * 4, 4);
  }

  function own(buffer, count) {
    const handle = { buffer, count, destroy() { if (live.delete(handle)) buffer.destroy(); } };
    live.add(handle);
    return handle;
  }

  function take(handle) {
    if (!live.delete(handle)) throw new Error("GPU buffer is consumed or belongs to another device");
    return handle.buffer;
  }

  function dispatch(program, mode, count, input) {
    if (program.device !== device) throw new Error("GPU kernel belongs to another device");
    const size = bytesFor(count);
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
        if (groups > device.limits.maxComputeWorkgroupsPerDimension) throw new RangeError("GPU dispatch exceeds the device workgroup limit");
        if (groups > 0) pass.dispatchWorkgroups(groups);
        pass.end();
        device.queue.submit([encoder.finish()]);
        await device.queue.onSubmittedWorkDone();
        return own(output, count);
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
    prepare(source) {
      return checked(async () => {
        const shader = device.createShaderModule({ code: source });
        const diagnostics = await shader.getCompilationInfo();
        const errors = diagnostics.messages.filter(message => message.type === "error");
        if (errors.length) throw new Error(errors.map(message => `${message.lineNum}:${message.linePos}: ${message.message}`).join("\n"));
        const map = await device.createComputePipelineAsync({ layout: "auto", compute: { module: shader, entryPoint: "map_main" } });
        const init = await device.createComputePipelineAsync({ layout: "auto", compute: { module: shader, entryPoint: "init_main" } });
        return { device, map, init };
      });
    },
    fromArray(values) {
      if (!(values instanceof Int32Array || values instanceof Uint32Array)) throw new TypeError("strict GPU buffers require Int32Array or Uint32Array");
      const size = bytesFor(values.length);
      return checked(() => {
        const buffer = device.createBuffer({ size, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST });
        if (values.byteLength) device.queue.writeBuffer(buffer, 0, values);
        return own(buffer, values.length);
      });
    },
    init(program, count) { return dispatch(program, "init", count); },
    map(program, buffer) { return dispatch(program, "map", buffer.count, buffer); },
    toArray(handle) {
      const buffer = take(handle);
      return checked(async () => {
        let readback;
        try {
          if (!handle.count) return new Uint32Array();
          readback = device.createBuffer({ size: handle.count * 4, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
          const encoder = device.createCommandEncoder();
          encoder.copyBufferToBuffer(buffer, 0, readback, 0, handle.count * 4);
          device.queue.submit([encoder.finish()]);
          await readback.mapAsync(GPUMapMode.READ);
          const result = new Uint32Array(readback.getMappedRange().slice(0));
          readback.unmap();
          return result;
        } finally {
          readback?.destroy();
          buffer.destroy();
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
