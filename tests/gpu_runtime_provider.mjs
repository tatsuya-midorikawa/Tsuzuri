// A WebGPU provider that needs no adapter, for tests/gpu_runtime.mjs (F09 Phase 2 review fixes): the kernel of `init_main`
// writes the number of each lane (an identity kernel), so the WASM host (src/runtime/webgpu.mjs) can be driven, with the
// pointers and the statuses of a module, without a GPU. `failOnBuffer` makes a buffer fail with a RangeError that is not
// a limit.
export function fakeProvider({ failOnBuffer = false } = {}) {
  globalThis.GPUBufferUsage ??= { MAP_READ: 1, COPY_SRC: 4, COPY_DST: 8, UNIFORM: 64, STORAGE: 128 };
  globalThis.GPUMapMode ??= { READ: 1 };
  const limits = { maxComputeInvocationsPerWorkgroup: 256, maxStorageBufferBindingSize: 134217728, maxBufferSize: 268435456, maxComputeWorkgroupsPerDimension: 65535 };
  const device = {
    limits,
    features: new Set(),
    pushErrorScope() {},
    async popErrorScope() { return null; },
    createBuffer({ size }) {
      if (failOnBuffer) throw new RangeError("Array buffer allocation failed");
      return { data: new Uint8Array(size), destroy() {}, async mapAsync() {}, getMappedRange() { return this.data.buffer; }, unmap() {} };
    },
    queue: {
      writeBuffer(buffer, offset, data) { buffer.data.set(new Uint8Array(data.buffer, data.byteOffset, data.byteLength), offset); },
      submit(commands) { for (const execute of commands) execute(); },
      async onSubmittedWorkDone() {},
    },
    createShaderModule: () => ({ async getCompilationInfo() { return { messages: [] }; } }),
    async createComputePipelineAsync({ compute }) { return { entry: compute.entryPoint, getBindGroupLayout: () => ({}) }; },
    createBindGroup: ({ entries }) => ({ entries }),
    createCommandEncoder() {
      let group;
      let copy;
      return {
        beginComputePass: () => ({ setPipeline() {}, setBindGroup(index, value) { group = value; }, dispatchWorkgroups() {}, end() {} }),
        copyBufferToBuffer(from, fromOffset, to, toOffset, size) { copy = { from, to, size }; },
        finish: () => () => {
          const binding = number => group.entries.find(entry => entry.binding === number).resource.buffer;
          const count = new Uint32Array(binding(2).data.buffer, 0, 1)[0];
          const output = new Int32Array(binding(1).data.buffer);
          for (let index = 0; index < count; index++) output[index] = index;
          copy.to.data.set(copy.from.data.subarray(0, copy.size));
        },
      };
    },
    destroy() {},
  };
  const adapter = { limits, features: new Set(), info: {}, async requestDevice() { return device; } };
  return { async requestAdapter() { return adapter; } };
}
