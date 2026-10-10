  // The sockets of the Net module under --wasm-feature net (E09 Phase 3): the imports of `tsuzuri_net`, on Node's net, dgram,
  // and dns. They keep the contract of src/runtime/net.c: a handle is a generation and a slot of a table, a status is
  // (kind << 32) | code, a timeout covers a whole call, 0 means "try once" (and PENDING says that the call would have
  // to wait), and a readiness wait or a connect completes an Async operation with `tsuzuri_async_complete`.
  // The calls that can wait are JSPI imports, so the module is suspended while Node does the work.
  function netImports(owner) {
    if (NODE_NET === undefined) throw new Error("Net sockets need Node.js (node:net, node:dgram, and node:dns); a browser has no raw TCP or UDP");
    const { net, dgram, dns, os } = NODE_NET;
    const errno = os.constants.errno;
    const codeNames = new Map(Object.entries(errno).map(([name, number]) => [number, name]));
    const status = (kind, code) => (BigInt(kind) << 32n) | BigInt(code >>> 0);
    const KINDS = { EACCES: 2, EPERM: 2, EADDRINUSE: 3, EINVAL: 4, EAFNOSUPPORT: 4, EADDRNOTAVAIL: 4, EMSGSIZE: 4, EINTR: 6 };
    const named = (name) => status(KINDS[name] ?? 7, errno[name] ?? 0);
    const failed = (error) => named(typeof error?.code === "string" && error.code in errno ? error.code : "EIO");
    const TIMED_OUT = named("ETIMEDOUT");
    const BAD_HANDLE = status(4, errno.EBADF);
    const INVALID = status(4, 0);
    const PENDING = status(7, 0);
    const RECORD = 20;
    const LIMIT = 1048576;
    const SEND_CHUNK = 65536n;

    // The table of open sockets: a handle is (generation << 32) | (slot + 1), as in net.c.
    const slots = [];
    const free = [];
    let generation = 0n;
    const slotOf = (handle) => Number(handle & 0xffffffffn) - 1;
    function register(entry) {
      const slot = free.length > 0 ? free.pop() : slots.length;
      generation = generation >= 2147483647n ? 1n : generation + 1n;
      entry.handle = (generation << 32n) | BigInt(slot + 1);
      slots[slot] = entry;
      return entry.handle;
    }
    function find(handle, kind) {
      if (handle <= 0n) return undefined;
      const entry = slots[slotOf(handle)];
      return entry !== undefined && entry.handle === handle && (kind === 0 || entry.kind === kind) ? entry : undefined;
    }
    function forget(entry) {
      slots[slotOf(entry.handle)] = undefined;
      free.push(slotOf(entry.handle));
    }
    const notify = (entry) => { for (const wake of [...entry.waiters]) wake(); };
    // Waits until the entry changes or the deadline (milliseconds on performance.now) passes: whether it changed.
    const sleep = (entry, deadline) => new Promise((resolve) => {
      let timer;
      const wake = () => { clearTimeout(timer); entry.waiters.delete(wake); resolve(true); };
      if (deadline !== Infinity) timer = setTimeout(() => { entry.waiters.delete(wake); resolve(false); }, Math.max(0, Math.ceil(deadline - performance.now())));
      entry.waiters.add(wake);
    });
    const deadlineOf = (timeout) => (timeout < 0n ? Infinity : performance.now() + Number(timeout));

    // Addresses: the scalars of the calls, and the 20-byte records of the results.
    const hex = (value) => value.toString(16);
    function hostOf(meta, high, low) {
      const lower = BigInt.asUintN(64, low);
      if (((meta >> 16n) & 1n) === 0n) return [24n, 16n, 8n, 0n].map((shift) => String((lower >> shift) & 255n)).join(".");
      const upper = BigInt.asUintN(64, high);
      return [48n, 32n, 16n, 0n].map((shift) => hex((upper >> shift) & 0xffffn)).concat([48n, 32n, 16n, 0n].map((shift) => hex((lower >> shift) & 0xffffn))).join(":");
    }
    const portOf = (meta) => Number(meta & 0xffffn);
    const familyOf = (meta) => (((meta >> 16n) & 1n) === 0n ? 4 : 6);
    function v6bytes(text) {
      let address = text.split("%")[0];
      if (address.includes(".")) {
        const colon = address.lastIndexOf(":");
        const [a, b, c, d] = address.slice(colon + 1).split(".").map(Number);
        address = `${address.slice(0, colon + 1)}${hex((a << 8) | b)}:${hex((c << 8) | d)}`;
      }
      const [head, rest] = address.split("::");
      const first = head === "" ? [] : head.split(":");
      const second = rest === undefined || rest === "" ? [] : rest.split(":");
      const groups = rest === undefined ? first : [...first, ...Array(8 - first.length - second.length).fill("0"), ...second];
      const bytes = new Uint8Array(16);
      groups.forEach((group, index) => {
        const value = Number.parseInt(group, 16);
        bytes[index * 2] = value >> 8;
        bytes[index * 2 + 1] = value & 255;
      });
      return bytes;
    }
    function record(text, port) {
      const bytes = new Uint8Array(RECORD);
      const version = net.isIP(text);
      if (version === 0) return bytes;
      bytes[0] = version === 4 ? 4 : 6;
      bytes[2] = port >> 8;
      bytes[3] = port & 255;
      if (version === 4) bytes.set(text.split(".").map(Number), 16);
      else bytes.set(v6bytes(text), 4);
      return bytes;
    }
    const names = (local, peer) => {
      const bytes = new Uint8Array(2 * RECORD);
      bytes.set(local, 0);
      bytes.set(peer, RECORD);
      return bytes;
    };

    // Results of the calls that return bytes go through a descriptor in the module's memory: a pointer and a length.
    function deliver(pointer, bytes) {
      let address = 0;
      let length = 0;
      if (bytes !== undefined && bytes.length > 0) [address, length] = writeBuffer(owner, "ubyte", bytes);
      const view = new DataView(memoryOf(owner));
      view.setUint32(pointer, address, true);
      view.setBigInt64(pointer + 8, BigInt(length), true);
    }
    const copyOf = (pointer, length) => Buffer.from(new Uint8Array(memoryOf(owner), pointer, Number(length)));

    // ---- Sockets ----
    function streamEntry(socket, family) {
      const entry = { kind: 1, socket, family, waiters: new Set(), chunks: [], queued: 0, ended: false, error: undefined, readShut: false, writeShut: false, names: undefined };
      socket.on("data", (chunk) => {
        if (entry.readShut) return;
        entry.chunks.push(chunk);
        entry.queued += chunk.length;
        if (entry.queued > LIMIT) socket.pause();
        notify(entry);
      });
      socket.on("end", () => { entry.ended = true; notify(entry); });
      socket.on("close", () => { entry.ended = true; notify(entry); });
      socket.on("error", (error) => { entry.error = error; notify(entry); });
      socket.on("drain", () => notify(entry));
      return entry;
    }
    const streamNames = (socket, peer) => names(record(socket.localAddress ?? "", socket.localPort ?? 0), peer);
    // Connects a new socket: the entry once it is connected, or a status.
    function connectSocket(meta, high, low, timeout) {
      const socket = new net.Socket();
      const entry = streamEntry(socket, familyOf(meta));
      const host = hostOf(meta, high, low);
      const port = portOf(meta);
      const settled = new Promise((resolve) => {
        const timer = timeout >= 0n ? setTimeout(() => { socket.destroy(); resolve(TIMED_OUT); }, Number(timeout)) : undefined;
        socket.once("connect", () => { clearTimeout(timer); entry.names = streamNames(socket, record(host, port)); resolve(0n); });
        socket.once("error", (error) => { clearTimeout(timer); resolve(failed(error)); });
        entry.cancel = () => { clearTimeout(timer); socket.destroy(); };
      });
      socket.connect({ host, port, family: familyOf(meta) });
      return { entry, settled };
    }
    function takeStream(entry, maximum) {
      if (entry.chunks.length > 0) {
        const joined = entry.chunks.length === 1 ? entry.chunks[0] : Buffer.concat(entry.chunks);
        const taken = joined.subarray(0, maximum);
        entry.chunks = taken.length < joined.length ? [joined.subarray(taken.length)] : [];
        entry.queued -= taken.length;
        if (entry.queued < LIMIT / 2) entry.socket.resume();
        return new Uint8Array(taken);
      }
      if (entry.ended || entry.readShut) return new Uint8Array(0);
      if (entry.error !== undefined) return failed(entry.error);
      return undefined;
    }
    function takeDatagram(entry, maximum) {
      const next = entry.messages.shift();
      if (next === undefined) return entry.error !== undefined ? failed(entry.error) : undefined;
      if (next.data.length > maximum) return named("EMSGSIZE");
      const bytes = new Uint8Array(RECORD + next.data.length);
      bytes.set(record(next.info.address, next.info.port), 0);
      bytes.set(next.data, RECORD);
      return bytes;
    }
    const readable = (entry) => (entry.kind === 1 ? entry.chunks.length > 0 || entry.ended || entry.readShut || entry.error !== undefined
      : entry.kind === 2 ? entry.queue.length > 0 : entry.messages.length > 0 || entry.error !== undefined);
    const writable = (entry) => entry.kind !== 1 || !entry.socket.writableNeedDrain || entry.socket.destroyed || entry.error !== undefined;

    // op 0 connects, 1 listens, 2 binds a UDP socket. A handle, or the negated status.
    async function open(pointer, operation, meta, high, low, timeout) {
      deliver(pointer, undefined);
      if (operation < 0 || operation > 2) return -INVALID;
      const host = hostOf(meta, high, low);
      const port = portOf(meta);
      const family = familyOf(meta);
      if (operation === 0) {
        if (timeout === 0n) return -TIMED_OUT;
        const { entry, settled } = connectSocket(meta, high, low, timeout);
        const outcome = await settled;
        if (outcome !== 0n) return -outcome;
        const handle = register(entry);
        deliver(pointer, entry.names);
        return handle;
      }
      if (operation === 1) {
        const server = net.createServer({ pauseOnConnect: true });
        const entry = { kind: 2, server, family, waiters: new Set(), queue: [], names: undefined };
        server.on("connection", (socket) => { entry.queue.push(socket); notify(entry); });
        const outcome = await new Promise((resolve) => {
          server.once("error", (error) => resolve(failed(error)));
          server.listen({ host, port, exclusive: true, ipv6Only: family === 6 }, () => resolve(0n));
        });
        if (outcome !== 0n) return -outcome;
        const bound = server.address();
        entry.names = names(record(bound.address, bound.port), new Uint8Array(RECORD));
        const handle = register(entry);
        deliver(pointer, entry.names);
        return handle;
      }
      const socket = dgram.createSocket(family === 6 ? { type: "udp6", ipv6Only: true } : { type: "udp4" });
      const entry = { kind: 3, socket, family, waiters: new Set(), messages: [], error: undefined, names: undefined };
      socket.on("message", (data, info) => { entry.messages.push({ data, info }); notify(entry); });
      socket.on("error", (error) => { entry.error = error; notify(entry); });
      const outcome = await new Promise((resolve) => {
        socket.once("error", (error) => resolve(failed(error)));
        socket.bind({ address: host, port, exclusive: true }, () => resolve(0n));
      });
      if (outcome !== 0n) {
        socket.close();
        return -outcome;
      }
      const bound = socket.address();
      entry.names = names(record(bound.address, bound.port), new Uint8Array(RECORD));
      const handle = register(entry);
      deliver(pointer, entry.names);
      return handle;
    }

    async function accept(pointer, handle, timeout) {
      deliver(pointer, undefined);
      const entry = find(handle, 2);
      if (entry === undefined) return -BAD_HANDLE;
      const deadline = deadlineOf(timeout);
      for (;;) {
        if (entry.queue.length > 0) {
          const socket = entry.queue.shift();
          const accepted = streamEntry(socket, entry.family);
          accepted.names = streamNames(socket, record(socket.remoteAddress ?? "", socket.remotePort ?? 0));
          socket.resume();
          const opened = register(accepted);
          deliver(pointer, accepted.names);
          return opened;
        }
        if (find(handle, 2) !== entry) return -BAD_HANDLE;
        if (timeout === 0n) return -PENDING;
        if (!(await sleep(entry, deadline))) return -TIMED_OUT;
      }
    }

    async function read(pointer, operation, handle, maximum, timeout) {
      deliver(pointer, undefined);
      if ((operation !== 0 && operation !== 1) || maximum < 1n || maximum > 16777216n) return INVALID;
      const kind = operation === 0 ? 1 : 3;
      const entry = find(handle, kind);
      if (entry === undefined) return BAD_HANDLE;
      const deadline = deadlineOf(timeout);
      for (;;) {
        const result = operation === 0 ? takeStream(entry, Number(maximum)) : takeDatagram(entry, Number(maximum));
        if (typeof result === "bigint") return result;
        if (result !== undefined) {
          deliver(pointer, result);
          return 0n;
        }
        if (find(handle, kind) !== entry) return BAD_HANDLE;
        if (timeout === 0n) return PENDING;
        if (!(await sleep(entry, deadline))) return TIMED_OUT;
      }
    }

    const sendDatagram = (entry, bytes, meta, high, low) => new Promise((resolve) => {
      entry.socket.send(bytes, portOf(meta), hostOf(meta, high, low), (error) => resolve(error ? failed(error) : 0n));
    });

    async function write(operation, handle, data, length, meta, high, low, timeout) {
      if ((operation !== 0 && operation !== 1) || length < 0n) return INVALID;
      const entry = find(handle, operation === 0 ? 1 : 3);
      if (entry === undefined) return BAD_HANDLE;
      const bytes = copyOf(data, length);
      if (operation === 1) return bytes.length > 65536 ? named("EMSGSIZE") : sendDatagram(entry, bytes, meta, high, low);
      if (entry.error !== undefined) return failed(entry.error);
      if (entry.writeShut || entry.socket.destroyed) return named("EPIPE");
      const written = new Promise((resolve) => entry.socket.write(bytes, (error) => resolve(error ? failed(error) : 0n)));
      if (timeout < 0n) return written;
      let timer;
      const late = new Promise((resolve) => { timer = setTimeout(() => resolve(TIMED_OUT), Number(timeout)); });
      return Promise.race([written, late]).finally(() => clearTimeout(timer));
    }

    // Sends without waiting: the bytes taken, or the negated status. Node queues what it takes and hands it to the system as
    // the system has room, so one call takes at most SEND_CHUNK bytes, and the caller waits (PENDING, then a drain) for the
    // rest: what is accepted but not yet with the system stays small, and `close` delivers it before it closes.
    function send(operation, handle, data, length, offset, meta, high, low) {
      if ((operation !== 0 && operation !== 1) || length < 0n || offset < 0n || offset > length) return -INVALID;
      const entry = find(handle, operation === 0 ? 1 : 3);
      if (entry === undefined) return -BAD_HANDLE;
      if (operation === 1) {
        if (length > 65536n) return -named("EMSGSIZE");
        entry.socket.send(copyOf(data, length), portOf(meta), hostOf(meta, high, low), (error) => { if (error) { entry.error = error; notify(entry); } });
        return length;
      }
      if (offset === length) return 0n;
      if (entry.error !== undefined) return -failed(entry.error);
      if (entry.writeShut || entry.socket.destroyed) return -named("EPIPE");
      if (entry.socket.writableNeedDrain) return -PENDING;
      const taken = length - offset > SEND_CHUNK ? SEND_CHUNK : length - offset;
      entry.socket.write(copyOf(data + Number(offset), taken));
      return taken;
    }

    function close(operation, handle) {
      if (operation < 0 || operation > 3) return INVALID;
      if (operation === 0) {
        const entry = find(handle, 0);
        if (entry === undefined) return BAD_HANDLE;
        forget(entry);
        if (entry.kind === 1) {
          // What the socket has accepted is still sent (a close of a descriptor does the same): the socket ends after it, and
          // what arrives meanwhile is dropped.
          entry.readShut = true;
          entry.socket.end(() => entry.socket.destroy());
        } else if (entry.kind === 2) {
          for (const pending of entry.queue) pending.destroy();
          entry.server.close();
        } else entry.socket.close();
        notify(entry);
        return 0n;
      }
      const entry = find(handle, 1);
      if (entry === undefined) return BAD_HANDLE;
      if (operation !== 2) entry.readShut = true;
      if (operation !== 1) {
        entry.writeShut = true;
        entry.socket.end();
      }
      notify(entry);
      return 0n;
    }

    function classify(code) {
      switch (codeNames.get(code)) {
        case "ETIMEDOUT": return 0n;
        case "ECONNREFUSED": return 1n;
        case "ECONNRESET": case "ECONNABORTED": case "EPIPE": return 2n;
        case "EADDRINUSE": return 3n;
        case "EADDRNOTAVAIL": return 4n;
        case "ENETUNREACH": case "EHOSTUNREACH": case "ENETDOWN": case "EHOSTDOWN": return 5n;
        default: return 6n;
      }
    }

    async function lookup(pointer, host, length, port) {
      deliver(pointer, undefined);
      if (length < 1n || length > 253n || port < 0n || port > 65535n) return INVALID;
      const bytes = new Uint8Array(memoryOf(owner), host, Number(length)).slice();
      if (bytes.includes(0)) return INVALID;
      try {
        const found = await dns.promises.lookup(new TextDecoder().decode(bytes), { all: true, verbatim: true });
        const records = [];
        for (const { address } of found) {
          const next = record(address, Number(port));
          if (next[0] !== 0 && records.length < 64 && !records.some((earlier) => earlier.every((byte, index) => byte === next[index]))) records.push(next);
        }
        if (records.length === 0) return status(1, 0);
        const all = new Uint8Array(records.length * RECORD);
        records.forEach((next, index) => all.set(next, index * RECORD));
        deliver(pointer, all);
        return 0n;
      } catch (error) {
        return error?.code === "ENOTFOUND" || error?.code === "ENODATA" ? status(1, 0) : status(7, 0);
      }
    }

    function namesOf(pointer, handle) {
      deliver(pointer, undefined);
      const entry = find(handle, 0);
      if (entry === undefined) return BAD_HANDLE;
      deliver(pointer, entry.names);
      return 0n;
    }

    // ---- Async operations: waits and connects that complete an operation of the executor ----
    const operations = new Map();
    // A completion is delivered after the call that started or ended the operation has returned.
    function finish(operation, value) {
      operations.delete(operation);
      if (!owner.alive || !owner.current()) return;
      enter(owner, () => owner.exports.tsuzuri_async_complete(operation, value));
      owner.wake?.();
      asyncHost?.schedule();
    }
    function later(operation, value) {
      const timer = setImmediate(() => finish(operation, value));
      operations.set(operation, () => clearImmediate(timer));
    }
    function watch(operation, handle, events, timeout) {
      if ((events !== 1 && events !== 2) || timeout < -1n) return later(operation, INVALID);
      if (timeout === 0n) return later(operation, TIMED_OUT);
      const entry = find(handle, 0);
      if (entry === undefined) return later(operation, BAD_HANDLE);
      const ready = () => (events === 1 ? readable(entry) : writable(entry));
      if (ready()) return later(operation, 0n);
      let timer;
      const check = () => {
        if (find(entry.handle, 0) === entry && !ready()) return;
        clearTimeout(timer);
        entry.waiters.delete(check);
        later(operation, 0n);
      };
      if (timeout > 0n) {
        timer = setTimeout(() => {
          entry.waiters.delete(check);
          finish(operation, TIMED_OUT);
        }, Number(timeout));
      }
      entry.waiters.add(check);
      operations.set(operation, () => { clearTimeout(timer); entry.waiters.delete(check); });
    }
    function connect(operation, meta, high, low, timeout) {
      if (timeout === 0n) return later(operation, -TIMED_OUT);
      if (timeout < -1n) return later(operation, -INVALID);
      const { entry, settled } = connectSocket(meta, high, low, timeout);
      operations.set(operation, () => entry.cancel());
      settled.then((outcome) => {
        if (!operations.has(operation)) {
          entry.socket.destroy();
          return;
        }
        if (outcome !== 0n) {
          entry.socket.destroy();
          finish(operation, -outcome);
          return;
        }
        finish(operation, register(entry));
      });
    }
    function unwatch(operation) {
      operations.get(operation)?.();
      operations.delete(operation);
    }

    // What can wait is a JSPI import; the rest returns at once.
    const suspending = (call) => new WebAssembly.Suspending(call);
    return Object.assign(Object.create(null), {
      resolve: suspending(lookup),
      open: suspending(open),
      accept: suspending(accept),
      read: suspending(read),
      write: suspending(write),
      close,
      classify,
      names: namesOf,
      send,
      watch,
      unwatch,
      connect,
    });
  }
