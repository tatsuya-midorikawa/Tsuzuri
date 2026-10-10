  // The sockets of the Net module under --wasm-feature net (E09 Phase 3): the imports of `tsuzuri_net`, on Node's net, dgram,
  // and dns. They keep the contract of src/runtime/net.c: a handle is a generation and a slot of a table, a status is
  // (kind << 32) | code, a timeout covers a whole call, 0 means "try once" (and PENDING says that the call would have
  // to wait), and a readiness wait or a connect completes an Async operation with `tsuzuri_async_complete`.
  // The calls that can wait are JSPI imports, so the module is suspended while Node does the work; the try-once reads and
  // accepts (`try_read`, `try_accept`) are plain imports, because the executor of Async.start polls outside JSPI.
  // What a socket costs Node: it keeps the process alive only while the module waits on it (a blocking call, a watch, a
  // connect), so a socket that was never closed ends with the process as it does natively; the instance's sockets are all
  // destroyed when the instance is discarded (`owner.dispose`, called by `fail`), because nothing could close them then.
  function netImports(owner) {
    if (NODE_NET === undefined) throw new Error("Net sockets need Node.js (node:net, node:dgram, and node:dns); a browser has no raw TCP or UDP");
    const { net, dgram, dns, os, util } = NODE_NET;
    // The error numbers by name: those of `os.constants.errno`, then the names that libuv gives to errors that it reports and
    // the system lists no constant for (EHOSTDOWN, ENONET, ...), with libuv's number negated, which is the system's own where
    // the system has the error. The table is the glue's alone, so that an error which Node names always has a status.
    const errno = Object.assign(Object.create(null), os.constants.errno);
    for (const [number, [name]] of util.getSystemErrorMap()) if (number < 0 && !(name in errno)) errno[name] = -number;
    const codeNames = new Map(Object.entries(errno).map(([name, number]) => [number, name]));
    const status = (kind, code) => (BigInt(kind) << 32n) | BigInt(code >>> 0);
    const KINDS = { EACCES: 2, EPERM: 2, EADDRINUSE: 3, EINVAL: 4, EAFNOSUPPORT: 4, EADDRNOTAVAIL: 4, EMSGSIZE: 4, EINTR: 6 };
    const named = (name) => status(KINDS[name] ?? 7, errno[name] ?? 0);
    // The status of an exception: the error number that Node names, EPIPE for a stream that Node says is closed, EINVAL for
    // an argument that Node's own checks refuse (the other `ERR_` codes), and EIO for the rest.
    const CLOSED = new Set(["ERR_STREAM_DESTROYED", "ERR_STREAM_WRITE_AFTER_END", "ERR_STREAM_PREMATURE_CLOSE", "ERR_SOCKET_CLOSED"]);
    const failed = (error) => {
      const code = typeof error?.code === "string" ? error.code : "";
      return named(code in errno ? code : CLOSED.has(code) ? "EPIPE" : code.startsWith("ERR_") ? "EINVAL" : "EIO");
    };
    const TIMED_OUT = named("ETIMEDOUT");
    const BAD_HANDLE = status(4, errno.EBADF);
    const INVALID = status(4, 0);
    const PENDING = status(7, 0);
    // What a datagram send answers (negated) when Node has taken the datagram and has not yet said how it went: ACCEPTED plus the
    // token of that send, which `watch` takes (events 2 + 4 * token) and completes with the send's own status. No error number
    // is as large as ACCEPTED, and its kind is the last, so that the compiler's check of a status lets it through.
    const ACCEPTED = status(7, 16777216);
    const MAX_TOKEN = 268435455;
    const RECORD = 20;
    const LIMIT = 1048576;
    const SEND_CHUNK = 65536n;
    // What the host keeps for a module that does not read or accept: the kernel drops a datagram when its receive buffer is
    // full and holds a bounded queue of connections, and so does the glue (the kernel's own limits come on top).
    const DATAGRAM_BYTES = 1048576;
    const DATAGRAM_COUNT = 4096;
    const ACCEPT_QUEUE = 128;
    // Linux refuses a datagram to port 0 (EINVAL), Windows too (EADDRNOTAVAIL), and macOS sends it and nobody gets it;
    // Node refuses it by throwing, so the glue says EINVAL everywhere.
    const ZERO_PORT = named("EINVAL");

    // The exceptions of Node's calls (a port that Node refuses, a descriptor that ran out) become the status of the call.
    // They must never reach the module: JSPI would take them for a failure of the instance, and a RangeError for a stack
    // that is exhausted. A trap of the module itself, from a nested call of its allocator, is not ours and passes. A call
    // on an instance that was discarded meanwhile throws what discarded it, which ends the stack that is still running.
    function guard(call, otherwise) {
      return function (...args) {
        if (!owner.alive) throw owner.failure;
        try {
          return call(...args);
        } catch (error) {
          if (error instanceof WebAssembly.RuntimeError) throw error;
          return otherwise(error, args);
        }
      };
    }
    function guardAsync(call, otherwise) {
      return async function (...args) {
        if (!owner.alive) throw owner.failure;
        let value;
        try {
          value = await call(...args);
        } catch (error) {
          if (error instanceof WebAssembly.RuntimeError) throw error;
          value = otherwise(error, args);
        }
        if (!owner.alive) throw owner.failure;
        return value;
      };
    }

    // The table of open sockets: a handle is (generation << 32) | (slot + 1), as in net.c.
    const slots = [];
    const free = [];
    // The sockets that a call is still making: not in the table, but the instance's all the same.
    const making = new Set();
    // The datagram sends that Node has taken and not finished, by token: a send is over when its callback has run, and its
    // outcome is waited for by the operation that started it (`watch` with that token), which deletes the flight.
    const flights = new Map();
    let nextToken = 0;
    let generation = 0n;
    const slotOf = (handle) => Number(handle & 0xffffffffn) - 1;
    function register(entry) {
      const slot = free.length > 0 ? free.pop() : slots.length;
      generation = generation >= 2147483647n ? 1n : generation + 1n;
      entry.handle = (generation << 32n) | BigInt(slot + 1);
      slots[slot] = entry;
      making.delete(entry);
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
    // A handle keeps Node alive only while the module waits on it. `holds` counts the waits (and the creation, until the
    // handle is open); a new handle starts with one.
    const handleOf = (entry) => entry.socket ?? entry.server;
    function retain(entry) {
      if (entry.holds++ === 0 && !handleOf(entry).destroyed) handleOf(entry).ref();
    }
    function release(entry) {
      if (--entry.holds === 0 && !handleOf(entry).destroyed) handleOf(entry).unref();
    }
    // Waits until the entry changes or the deadline (milliseconds on performance.now) passes: whether it changed.
    const sleep = (entry, deadline) => new Promise((resolve) => {
      let timer;
      retain(entry);
      const wake = () => { clearTimeout(timer); entry.waiters.delete(wake); release(entry); resolve(true); };
      if (deadline !== Infinity) timer = setTimeout(() => { entry.waiters.delete(wake); release(entry); resolve(false); }, Math.max(0, Math.ceil(deadline - performance.now())));
      entry.waiters.add(wake);
    });
    const deadlineOf = (timeout) => (timeout < 0n ? Infinity : performance.now() + Number(timeout));
    // Destroys every socket of the instance: what a discarded instance leaves would stay bound and keep Node alive.
    function destroy(entry) {
      try {
        if (entry.kind === 1) entry.socket.destroy();
        else if (entry.kind === 2) {
          for (const pending of entry.queue) pending.socket.destroy();
          entry.queue.length = 0;
          entry.server.close();
        } else entry.socket.close();
      } catch {
        // Closed already.
      }
      notify(entry);
    }
    function dispose() {
      for (const cancel of [...operations.values()]) cancel();
      operations.clear();
      for (const entry of [...making, ...slots]) if (entry !== undefined) destroy(entry);
      making.clear();
      flights.clear();
      slots.length = 0;
      free.length = 0;
    }
    owner.dispose = dispose;

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

    // Results of the calls that return bytes go through a descriptor in the module's memory: a pointer and a length. A
    // pointer of the module arrives as a signed number, and one at or above 2 GiB (the heap may grow to 4 GiB) as a
    // negative one, so each pointer of a call is made unsigned first.
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
    // The entry of a stream. It starts with one hold, which its maker releases when it is open.
    function streamEntry(socket, family) {
      const entry = { kind: 1, socket, family, holds: 1, waiters: new Set(), chunks: [], queued: 0, ended: false, error: undefined, reported: false, readShut: false, writeShut: false, names: undefined };
      socket.on("data", (chunk) => {
        if (entry.readShut) return;
        entry.chunks.push(chunk);
        entry.queued += chunk.length;
        if (entry.queued > LIMIT) socket.pause();
        notify(entry);
      });
      socket.on("end", () => { entry.ended = true; notify(entry); });
      socket.on("close", () => { entry.ended = true; notify(entry); });
      socket.on("error", (error) => { entry.error ??= error; notify(entry); });
      socket.on("drain", () => notify(entry));
      return entry;
    }
    const streamNames = (socket, peer) => names(record(socket.localAddress ?? "", socket.localPort ?? 0), peer);
    // Connects a new socket: the entry once it is connected, or a status. The peer's half-close does not end our writing
    // (`allowHalfOpen`), as a received FIN does not natively.
    function connectSocket(meta, high, low, timeout) {
      const socket = new net.Socket({ allowHalfOpen: true });
      const entry = streamEntry(socket, familyOf(meta));
      const host = hostOf(meta, high, low);
      const port = portOf(meta);
      making.add(entry);
      const settled = new Promise((resolve) => {
        const done = (outcome) => { clearTimeout(timer); making.delete(entry); resolve(outcome); };
        const timer = timeout >= 0n ? setTimeout(() => { socket.destroy(); done(TIMED_OUT); }, Number(timeout)) : undefined;
        socket.once("connect", () => { entry.names = streamNames(socket, record(host, port)); done(0n); });
        socket.once("error", (error) => done(failed(error)));
        entry.cancel = () => { clearTimeout(timer); making.delete(entry); socket.destroy(); };
      });
      try {
        socket.connect({ host, port, family: familyOf(meta) });
      } catch (error) {
        making.delete(entry);
        socket.destroy();
        throw error;
      }
      return { entry, settled };
    }
    // What a stream gives a read: what has arrived, then the error that ended it (once: a reset that came while the module
    // was not reading is a reset, not a clean end), then the end.
    function takeStream(entry, maximum) {
      if (entry.chunks.length > 0) {
        const joined = entry.chunks.length === 1 ? entry.chunks[0] : Buffer.concat(entry.chunks);
        const taken = joined.subarray(0, maximum);
        entry.chunks = taken.length < joined.length ? [joined.subarray(taken.length)] : [];
        entry.queued -= taken.length;
        if (entry.queued < LIMIT / 2) entry.socket.resume();
        return new Uint8Array(taken);
      }
      if (entry.readShut) return new Uint8Array(0);
      if (entry.error !== undefined && !entry.reported) {
        entry.reported = true;
        return failed(entry.error);
      }
      if (entry.ended) return new Uint8Array(0);
      return undefined;
    }
    function takeDatagram(entry, maximum) {
      const next = entry.messages.shift();
      if (next === undefined) {
        if (entry.error === undefined) return undefined;
        const error = entry.error;
        entry.error = undefined;
        return failed(error);
      }
      entry.queued -= next.data.length;
      if (next.data.length > maximum) return named("EMSGSIZE");
      const bytes = new Uint8Array(RECORD + next.data.length);
      bytes.set(record(next.info.address, next.info.port), 0);
      bytes.set(next.data, RECORD);
      return bytes;
    }
    // Whether a call on the entry can go on now: it has something to read or accept, or can take a write.
    const readable = (entry) => (entry.kind === 1 ? entry.chunks.length > 0 || entry.ended || entry.readShut || (entry.error !== undefined && !entry.reported)
      : entry.kind === 2 ? entry.queue.length > 0 || entry.error !== undefined : entry.messages.length > 0 || entry.error !== undefined);
    // A datagram socket takes a send at any time (Node queues it); what a module waits for after one is that send's own end.
    const writable = (entry) => (entry.kind === 1 ? !entry.socket.writableNeedDrain || entry.socket.destroyed || entry.error !== undefined : true);

    // The connection that waits in the queue of a listener, as a handle, or the negated status of what went wrong with
    // the listener; undefined when none waits. A connection that is gone before it is taken is skipped, as natively. The
    // addresses of a queued connection are those that it had when it arrived: a peer that resets it meanwhile leaves no
    // peer name to ask for, and net.c, too, names the connection by what `accept` gave it.
    function takeConnection(pointer, entry) {
      while (entry.queue.length > 0) {
        const { socket, names } = entry.queue.shift();
        if (socket.destroyed) continue;
        const accepted = streamEntry(socket, entry.family);
        accepted.names = names;
        socket.resume();
        release(accepted);
        const opened = register(accepted);
        deliver(pointer, accepted.names);
        return opened;
      }
      if (entry.error === undefined) return undefined;
      const error = entry.error;
      entry.error = undefined;
      return -failed(error);
    }

    // op 0 connects, 1 listens, 2 binds a UDP socket. A handle, or the negated status.
    async function open(pointer, operation, meta, high, low, timeout) {
      pointer >>>= 0;
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
        if (!owner.alive) {
          entry.socket.destroy();
          return -INVALID;
        }
        release(entry);
        const handle = register(entry);
        deliver(pointer, entry.names);
        return handle;
      }
      if (operation === 1) {
        const server = net.createServer({ pauseOnConnect: true, allowHalfOpen: true });
        const entry = { kind: 2, server, family, holds: 1, waiters: new Set(), queue: [], error: undefined, names: undefined };
        making.add(entry);
        // A connection that is not taken yet is not read, but it may still fail, and that must not be an unhandled event.
        server.on("connection", (socket) => {
          socket.on("error", () => {});
          if (entry.queue.length >= ACCEPT_QUEUE) {
            socket.resetAndDestroy();
            return;
          }
          entry.queue.push({ socket, names: streamNames(socket, record(socket.remoteAddress ?? "", socket.remotePort ?? 0)) });
          notify(entry);
        });
        const outcome = await new Promise((resolve) => {
          const listening = () => { server.off("error", refused); resolve(0n); };
          const refused = (error) => { server.off("listening", listening); resolve(failed(error)); };
          server.once("error", refused);
          server.once("listening", listening);
          try {
            server.listen({ host, port, exclusive: true, ipv6Only: family === 6, backlog: ACCEPT_QUEUE });
          } catch (error) {
            refused(error);
          }
        });
        if (outcome !== 0n) {
          making.delete(entry);
          return -outcome;
        }
        // The instance was discarded meanwhile, and `dispose` has closed the listener.
        if (!owner.alive) return -INVALID;
        // An error after listening (a descriptor that ran out while accepting) is the next accept's.
        server.on("error", (error) => { entry.error = error; notify(entry); });
        const bound = server.address();
        entry.names = names(record(bound.address, bound.port), new Uint8Array(RECORD));
        release(entry);
        const handle = register(entry);
        deliver(pointer, entry.names);
        return handle;
      }
      const socket = dgram.createSocket(family === 6 ? { type: "udp6", ipv6Only: true } : { type: "udp4" });
      const entry = { kind: 3, socket, family, holds: 1, waiters: new Set(), messages: [], queued: 0, inflight: 0, closing: false, error: undefined, names: undefined };
      making.add(entry);
      // A datagram that finds the queue full is dropped, as a full receive buffer drops it.
      socket.on("message", (data, info) => {
        if (entry.messages.length >= DATAGRAM_COUNT || entry.queued + data.length > DATAGRAM_BYTES) return;
        entry.messages.push({ data, info });
        entry.queued += data.length;
        notify(entry);
      });
      socket.on("error", (error) => { entry.error ??= error; notify(entry); });
      const outcome = await new Promise((resolve) => {
        const listening = () => { socket.off("error", refused); resolve(0n); };
        const refused = (error) => { socket.off("listening", listening); resolve(failed(error)); };
        socket.once("error", refused);
        socket.once("listening", listening);
        try {
          socket.bind({ address: host, port, exclusive: true });
        } catch (error) {
          refused(error);
        }
      });
      if (outcome !== 0n) {
        making.delete(entry);
        socket.close();
        return -outcome;
      }
      if (!owner.alive) return -INVALID;
      const bound = socket.address();
      entry.names = names(record(bound.address, bound.port), new Uint8Array(RECORD));
      release(entry);
      const handle = register(entry);
      deliver(pointer, entry.names);
      return handle;
    }

    // The handle of a connection that waits, the negated status of what went wrong with the listener, or undefined.
    // The calls that wait are JSPI imports and so cannot run in a poll of the executor of Async.start, which is not
    // entered through `promising`; the async operations try once through `try_accept` and `try_read` instead, which answer
    // at once whether they have a result or not.
    function tryAccept(pointer, handle) {
      pointer >>>= 0;
      deliver(pointer, undefined);
      const entry = find(handle, 2);
      if (entry === undefined) return -BAD_HANDLE;
      return takeConnection(pointer, entry) ?? -PENDING;
    }

    async function accept(pointer, handle, timeout) {
      pointer >>>= 0;
      deliver(pointer, undefined);
      const entry = find(handle, 2);
      if (entry === undefined) return -BAD_HANDLE;
      const deadline = deadlineOf(timeout);
      for (;;) {
        const taken = takeConnection(pointer, entry);
        if (taken !== undefined) return taken;
        if (find(handle, 2) !== entry) return -BAD_HANDLE;
        if (timeout === 0n) return -PENDING;
        if (!(await sleep(entry, deadline))) return -TIMED_OUT;
      }
    }

    // What a read gives without waiting: the status, 0 with the bytes delivered, or undefined when it would have to wait.
    function readNow(pointer, operation, entry, maximum) {
      const result = operation === 0 ? takeStream(entry, Number(maximum)) : takeDatagram(entry, Number(maximum));
      if (typeof result === "bigint") return result;
      if (result === undefined) return undefined;
      deliver(pointer, result);
      return 0n;
    }

    function tryRead(pointer, operation, handle, maximum) {
      pointer >>>= 0;
      deliver(pointer, undefined);
      if ((operation !== 0 && operation !== 1) || maximum < 1n || maximum > 16777216n) return INVALID;
      const entry = find(handle, operation === 0 ? 1 : 3);
      if (entry === undefined) return BAD_HANDLE;
      return readNow(pointer, operation, entry, maximum) ?? PENDING;
    }

    async function read(pointer, operation, handle, maximum, timeout) {
      pointer >>>= 0;
      deliver(pointer, undefined);
      if ((operation !== 0 && operation !== 1) || maximum < 1n || maximum > 16777216n) return INVALID;
      const kind = operation === 0 ? 1 : 3;
      const entry = find(handle, kind);
      if (entry === undefined) return BAD_HANDLE;
      const deadline = deadlineOf(timeout);
      for (;;) {
        const result = readNow(pointer, operation, entry, maximum);
        if (result !== undefined) return result;
        if (find(handle, kind) !== entry) return BAD_HANDLE;
        if (timeout === 0n) return PENDING;
        if (!(await sleep(entry, deadline))) return TIMED_OUT;
      }
    }

    // Hands a datagram to Node, which sends it in a later turn and then says how it went: `done` gets the status. `inflight`
    // counts the sends that Node still has, because a datagram socket that is closed drops a send that has not left yet.
    function sendDatagram(entry, bytes, meta, high, low, done) {
      entry.inflight++;
      try {
        entry.socket.send(bytes, portOf(meta), hostOf(meta, high, low), (error) => {
          entry.inflight--;
          if (entry.closing && entry.inflight === 0) closeDatagram(entry);
          done(error ? failed(error) : 0n);
        });
      } catch (error) {
        entry.inflight--;
        throw error;
      }
    }
    function closeDatagram(entry) {
      try {
        entry.socket.close();
      } catch {
        // Closed already.
      }
    }

    async function write(operation, handle, data, length, meta, high, low, timeout) {
      data >>>= 0;
      if ((operation !== 0 && operation !== 1) || length < 0n) return INVALID;
      const entry = find(handle, operation === 0 ? 1 : 3);
      if (entry === undefined) return BAD_HANDLE;
      const bytes = copyOf(data, length);
      if (operation === 1) {
        if (bytes.length > 65536) return named("EMSGSIZE");
        // A blocking send waits for Node to say how it went.
        return portOf(meta) === 0 ? ZERO_PORT : new Promise((resolve) => sendDatagram(entry, bytes, meta, high, low, resolve));
      }
      if (entry.error !== undefined) return failed(entry.error);
      if (entry.writeShut || entry.socket.destroyed) return named("EPIPE");
      retain(entry);
      const written = new Promise((resolve) => entry.socket.write(bytes, (error) => resolve(error ? failed(error) : 0n)));
      if (timeout < 0n) return written.finally(() => release(entry));
      let timer;
      const late = new Promise((resolve) => { timer = setTimeout(() => resolve(TIMED_OUT), Number(timeout)); });
      return Promise.race([written, late]).finally(() => { clearTimeout(timer); release(entry); });
    }

    // Sends without waiting: the bytes taken, or the negated status. Node queues what it takes and hands it to the system as
    // the system has room, so one call takes at most SEND_CHUNK bytes, and the caller waits (PENDING, then a drain) for the
    // rest: what is accepted but not yet with the system stays small, and `close` delivers it before it closes.
    // A datagram is the system's to send in a later turn and a call cannot wait for it: the call hands it to Node, once, and
    // answers ACCEPTED plus a token (never the datagram again); the caller then waits for that token (`watch`), which completes
    // with how this very send went. So a send has its own result, a failed one leaves no datagram, and nothing is sent twice.
    function send(operation, handle, data, length, offset, meta, high, low) {
      data >>>= 0;
      if ((operation !== 0 && operation !== 1) || length < 0n || offset < 0n || offset > length) return -INVALID;
      const entry = find(handle, operation === 0 ? 1 : 3);
      if (entry === undefined) return -BAD_HANDLE;
      if (operation === 1) return startSend(entry, data, length, meta, high, low);
      if (offset === length) return 0n;
      if (entry.error !== undefined) return -failed(entry.error);
      if (entry.writeShut || entry.socket.destroyed) return -named("EPIPE");
      if (entry.socket.writableNeedDrain) return -PENDING;
      const taken = length - offset > SEND_CHUNK ? SEND_CHUNK : length - offset;
      entry.socket.write(copyOf(data + Number(offset), taken));
      return taken;
    }
    function startSend(entry, data, length, meta, high, low) {
      if (length > 65536n) return -named("EMSGSIZE");
      if (portOf(meta) === 0) return -ZERO_PORT;
      do nextToken = nextToken === MAX_TOKEN ? 1 : nextToken + 1; while (flights.has(nextToken));
      const token = nextToken;
      const flight = { handle: entry.handle, done: false, status: 0n, attached: undefined, abandoned: false };
      flights.set(token, flight);
      try {
        sendDatagram(entry, copyOf(data, length), meta, high, low, (outcome) => {
          flight.done = true;
          flight.status = outcome;
          if (flight.attached !== undefined) flight.attached();
          else if (flight.abandoned) flights.delete(token);
        });
      } catch (error) {
        flights.delete(token);
        throw error;
      }
      return -(ACCEPTED + BigInt(token));
    }
    // The wait of an async operation for the end of the send with `token` on the socket `handle`: it completes with the status
    // of that send (0, or why it failed), whatever happened to the handle meanwhile (a close waits for the send, and the send
    // has a result of its own). A wait that is cancelled or times out leaves the send going, and its flight is dropped when it ends.
    function watchSend(operation, handle, token, timeout) {
      const flight = flights.get(token);
      if (flight === undefined || flight.handle !== handle || flight.attached !== undefined) return later(operation, INVALID);
      if (flight.done) {
        flights.delete(token);
        return later(operation, flight.status);
      }
      if (timeout === 0n) {
        flight.abandoned = true;
        return later(operation, TIMED_OUT);
      }
      let timer;
      flight.attached = () => {
        clearTimeout(timer);
        flights.delete(token);
        later(operation, flight.status);
      };
      if (timeout > 0n) {
        timer = setTimeout(() => {
          flight.attached = undefined;
          flight.abandoned = true;
          finish(operation, TIMED_OUT);
        }, Number(timeout));
      }
      operations.set(operation, () => {
        clearTimeout(timer);
        flight.attached = undefined;
        flight.abandoned = true;
      });
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
          for (const pending of entry.queue) pending.socket.destroy();
          entry.server.close();
        } else if (entry.inflight > 0) {
          // A datagram socket that is closed drops the sends that Node has not finished: it closes after the last of them.
          entry.closing = true;
        } else closeDatagram(entry);
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

    // Whether a resolver may read a host as an IP address, so that the module's strict parser has to decide it: the rule of
    // `numeric_looking` in std/Net.tz, which this glue keeps on its own because the glue is the last thing before Node's
    // resolver. Node maps a host with IDNA before the system sees it, so a byte outside ASCII is refused too:
    // "１２７．０．０．１", "127。0。0。1", and "127.0.0.1" with a soft hyphen after it all reach getaddrinfo as 127.0.0.1.
    function readsAsAddress(bytes) {
      if (bytes.some((byte) => byte === 58 || byte === 37 || byte <= 32 || byte >= 127)) return true;
      const stop = bytes.length > 0 && bytes[bytes.length - 1] === 46 ? bytes.length - 1 : bytes.length;
      let start = stop;
      while (start > 0 && bytes[start - 1] !== 46) start--;
      const label = bytes.subarray(start, stop);
      if (label.length === 0) return false;
      return label.every((byte) => byte >= 48 && byte <= 57) || (label.length >= 2 && label[0] === 48 && (label[1] === 120 || label[1] === 88));
    }

    async function lookup(pointer, host, length, port) {
      pointer >>>= 0;
      host >>>= 0;
      deliver(pointer, undefined);
      if (length < 1n || length > 253n || port < 0n || port > 65535n) return INVALID;
      const bytes = new Uint8Array(memoryOf(owner), host, Number(length)).slice();
      if (readsAsAddress(bytes)) return INVALID;
      let found;
      try {
        found = await dns.promises.lookup(new TextDecoder().decode(bytes), { all: true, verbatim: true });
      } catch (error) {
        return error?.code === "ENOTFOUND" || error?.code === "ENODATA" ? status(1, 0) : status(7, 0);
      }
      const records = [];
      for (const { address } of found) {
        const next = record(address, Number(port));
        if (next[0] !== 0 && records.length < 64 && !records.some((earlier) => earlier.every((byte, index) => byte === next[index]))) records.push(next);
      }
      if (records.length === 0) return status(1, 0);
      const all = new Uint8Array(records.length * RECORD);
      records.forEach((next, index) => all.set(next, index * RECORD));
      if (!owner.alive) return INVALID;
      deliver(pointer, all);
      return 0n;
    }

    function namesOf(pointer, handle) {
      pointer >>>= 0;
      deliver(pointer, undefined);
      const entry = find(handle, 0);
      if (entry === undefined) return BAD_HANDLE;
      deliver(pointer, entry.names);
      return 0n;
    }

    // ---- Async operations: waits and connects that complete an operation of the executor ----
    const operations = new Map();
    // A completion is delivered after the call that started or ended the operation has returned. A trap of the module in it
    // has discarded the instance by then, and its failure reaches the module's callers (`settled()`, the next call); here
    // nobody could catch it.
    function finish(operation, value) {
      operations.delete(operation);
      if (!owner.alive || !owner.current()) return;
      try {
        enter(owner, () => owner.exports.tsuzuri_async_complete(operation, value));
      } catch {
        return;
      }
      owner.wake?.();
      asyncHost?.schedule();
    }
    function later(operation, value) {
      const timer = setImmediate(() => finish(operation, value));
      operations.set(operation, () => clearImmediate(timer));
    }
    // `events` 1 waits until the socket can read (or accept), 2 until it can write; 2 + 4 * token waits for the end of the datagram
    // send that `send` answered with that token, and completes with the status of that send.
    function watch(operation, handle, events, timeout) {
      const wanted = events & 3;
      const token = events >>> 2;
      if (events < 1 || (wanted !== 1 && wanted !== 2) || (token !== 0 && wanted !== 2) || timeout < -1n) return later(operation, INVALID);
      if (token !== 0) return watchSend(operation, handle, token, timeout);
      if (timeout === 0n) return later(operation, TIMED_OUT);
      const entry = find(handle, 0);
      if (entry === undefined) return later(operation, BAD_HANDLE);
      const ready = () => (wanted === 1 ? readable(entry) : writable(entry));
      if (ready()) return later(operation, 0n);
      let timer;
      retain(entry);
      const stop = () => {
        if (!entry.waiters.delete(check)) return;
        clearTimeout(timer);
        release(entry);
      };
      const check = () => {
        if (find(entry.handle, 0) === entry && !ready()) return;
        stop();
        later(operation, 0n);
      };
      if (timeout > 0n) {
        timer = setTimeout(() => {
          stop();
          finish(operation, TIMED_OUT);
        }, Number(timeout));
      }
      entry.waiters.add(check);
      operations.set(operation, stop);
    }
    function connect(operation, meta, high, low, timeout) {
      if (timeout === 0n) return later(operation, -TIMED_OUT);
      if (timeout < -1n) return later(operation, -INVALID);
      const { entry, settled } = connectSocket(meta, high, low, timeout);
      operations.set(operation, () => entry.cancel());
      settled.then((outcome) => {
        if (!operations.has(operation) || !owner.alive) {
          entry.socket.destroy();
          return;
        }
        if (outcome !== 0n) {
          entry.socket.destroy();
          finish(operation, -outcome);
          return;
        }
        release(entry);
        finish(operation, register(entry));
      });
    }
    function unwatch(operation) {
      operations.get(operation)?.();
      operations.delete(operation);
    }

    // What can wait is a JSPI import; the rest returns at once, and the try-once calls are among them, because an executor that
    // the host polls (Async.start) is not inside `promising`.
    const suspending = (call) => new WebAssembly.Suspending(call);
    const negated = (error) => -failed(error);
    return Object.assign(Object.create(null), {
      resolve: suspending(guardAsync(lookup, failed)),
      open: suspending(guardAsync(open, negated)),
      accept: suspending(guardAsync(accept, negated)),
      read: suspending(guardAsync(read, failed)),
      write: suspending(guardAsync(write, failed)),
      try_accept: guard(tryAccept, negated),
      try_read: guard(tryRead, failed),
      close: guard(close, failed),
      classify,
      names: guard(namesOf, failed),
      send: guard(send, negated),
      watch: guard(watch, (error, [operation]) => later(operation, failed(error))),
      unwatch: guard(unwatch, () => undefined),
      connect: guard(connect, (error, [operation]) => later(operation, -failed(error))),
    });
  }
