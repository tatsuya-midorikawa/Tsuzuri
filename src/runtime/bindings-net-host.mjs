// The Node modules behind the sockets of a module that was built with --wasm-feature net (E09 Phase 3). A host without
// them, such as a browser, which has no raw TCP or UDP, cannot instantiate the module: `netImports` says so.
const NODE_NET = TABLE.net === true
  ? await Promise.all([import("node:net"), import("node:dgram"), import("node:dns"), import("node:os"), import("node:util")]).then(
    ([net, dgram, dns, os, util]) => ({ net: net.default ?? net, dgram: dgram.default ?? dgram, dns: dns.default ?? dns, os: os.default ?? os, util: util.default ?? util }),
    () => undefined,
  )
  : undefined;
