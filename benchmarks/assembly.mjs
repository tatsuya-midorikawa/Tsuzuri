// The comparison of two assembly listings that benchmarks/run-range-flags.mjs makes.

// Each target's own comment syntax, which skips quoted strings: `;` and `//` on AArch64 (where `#` starts an immediate and
// `@` is an operand character), `#` on x86 (AT&T; `@` names relocations and symbol types).
const comments = { arm64: /("(?:[^"\\]|\\.)*")|\s*(?:;|\/\/).*$/g, x86_64: /("(?:[^"\\]|\\.)*")|\s*#.*$/g };

// The lines of an assembly listing in order. Comments and blank lines are the only text that is ignored: operands,
// immediates, labels, directives and data all take part in the comparison.
export function assemblyLines(assembly, target) {
  const comment = comments[target];
  if (!comment) throw new Error(`unknown assembly target: ${target}`);
  return assembly.split("\n").map((line) => line.replace(comment, "$1").trim()).filter(Boolean);
}

// The length of the shortest edit script (lines inserted plus lines deleted) from `a` to `b`: Myers' O(ND) algorithm, so
// the same lines in another order are a difference, and similar listings are cheap to compare.
export function difference(a, b) {
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) ++start;
  let endA = a.length, endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) { --endA; --endB; }
  const n = endA - start, m = endB - start;
  if (n === 0 || m === 0) return n + m;
  const frontier = new Int32Array(2 * (n + m) + 3), middle = n + m + 1;
  for (let d = 0; d <= n + m; ++d) {
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && frontier[middle + k - 1] < frontier[middle + k + 1]) ? frontier[middle + k + 1] : frontier[middle + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[start + x] === b[start + y]) { ++x; ++y; }
      frontier[middle + k] = x;
      if (x >= n && y >= m) return d;
    }
  }
}
