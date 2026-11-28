// Independent references for the `random` and `random_tensor` suites of tests/features.mjs (NC10).
//
// The generator is modelled with BigInt from the published algorithms: PCG-XSL-RR 128/64 (pcg-c's
// pcg_setseq_128_xsl_rr_64) and the seed mixing of NumPy's SeedSequence. The model is checked below against
// numbers that NumPy 2.5.1 printed (`numpy.random.PCG64(seed).random_raw()`, `SeedSequence.spawn`,
// `Generator.random`, `Generator.binomial`, `Generator.poisson`), so the expected values of the cases do not come
// from std/Random.tz. The distributions follow the papers (Leva 1992; Marsaglia and Tsang 2000; Jöhnk 1964;
// Kachitvichyanukul and Schmeiser 1988; Hörmann 1993) in JavaScript doubles. Their logarithms come from V8, not
// from the musl routines of the compiler, so real results are compared with a relative tolerance and integer
// results exactly (a rejection test would need a tie within one ulp to differ).
import assert from "node:assert/strict";

const MASK32 = 0xffffffffn;
const MASK64 = (1n << 64n) - 1n;
const MASK128 = (1n << 128n) - 1n;
const MULTIPLIER = 47026247687942121848144207491837523525n;
const signed = (value) => BigInt.asIntN(64, value);

// ---- SeedSequence ----

const seedWords = (value) => (value >> 32n === 0n ? [value & MASK32] : [value & MASK32, value >> 32n]);

function seedPool(entropy, spawnKey) {
  const words = [...entropy];
  if (spawnKey.length > 0) while (words.length < 4) words.push(0n);
  words.push(...spawnKey);
  let constant = 0x43b0d7e5n;
  const hashMix = (value) => {
    value ^= constant;
    constant = (constant * 0x931e8875n) & MASK32;
    value = (value * constant) & MASK32;
    return value ^ (value >> 16n);
  };
  const mix = (x, y) => {
    const result = (0xca01f9ddn * x - 0x4973f715n * y) & MASK32;
    return result ^ (result >> 16n);
  };
  const pool = [0n, 0n, 0n, 0n];
  for (let i = 0; i < 4; i++) pool[i] = hashMix(i < words.length ? words[i] : 0n);
  for (let source = 0; source < 4; source++) {
    for (let target = 0; target < 4; target++) {
      if (source !== target) pool[target] = mix(pool[target], hashMix(pool[source]));
    }
  }
  for (let source = 4; source < words.length; source++) {
    for (let target = 0; target < 4; target++) pool[target] = mix(pool[target], hashMix(words[source]));
  }
  return pool;
}

// generate_state(4, uint64): eight 32-bit outputs, paired low word first.
function seedState(pool) {
  let constant = 0x8b51f9ddn;
  const words = [];
  for (let i = 0; i < 8; i++) {
    let value = pool[i % 4] ^ constant;
    constant = (constant * 0x58f38dedn) & MASK32;
    value = (value * constant) & MASK32;
    words.push(value ^ (value >> 16n));
  }
  return [0, 1, 2, 3].map((i) => words[2 * i] | (words[2 * i + 1] << 32n));
}

export class SeedSequence {
  constructor(entropy, spawnKey = []) {
    this.entropy = entropy;
    this.spawnKey = spawnKey;
    this.spawned = 0n;
    this.pool = seedPool(entropy, spawnKey);
  }
  static of(values) {
    return new SeedSequence(values.flatMap(seedWords));
  }
  spawn(count) {
    const children = [];
    for (let i = 0n; i < BigInt(count); i++) children.push(new SeedSequence(this.entropy, [...this.spawnKey, ...seedWords(this.spawned + i)]));
    this.spawned += BigInt(count);
    return children;
  }
  state() {
    return seedState(this.pool);
  }
}

// ---- PCG-XSL-RR 128/64 ----

export class Pcg64 {
  constructor(sequence) {
    const [high, low, streamHigh, streamLow] = sequence.state();
    this.increment = ((((streamHigh << 64n) | streamLow) << 1n) | 1n) & MASK128;
    this.state = 0n;
    this.step();
    this.state = (this.state + ((high << 64n) | low)) & MASK128;
    this.step();
  }
  static seeded(seed) {
    return new Pcg64(SeedSequence.of([BigInt.asUintN(64, seed)]));
  }
  step() {
    this.state = (this.state * MULTIPLIER + this.increment) & MASK128;
  }
  nextU64() {
    this.step();
    const folded = (this.state >> 64n) ^ (this.state & MASK64);
    const rotation = this.state >> 122n;
    return ((folded >> rotation) | (folded << (64n - rotation))) & MASK64;
  }
  nextF64() {
    return Number(this.nextU64() >> 11n) / 9007199254740992;
  }
  // Lemire's method on the full 64-bit product.
  range(low, high) {
    const span = BigInt.asUintN(64, high - low);
    let product = this.nextU64() * span;
    if ((product & MASK64) < span) {
      const threshold = ((1n << 64n) - span) % span;
      while ((product & MASK64) < threshold) product = this.nextU64() * span;
    }
    return signed(low + (product >> 64n));
  }
  uniform(low, high) {
    return low + (high - low) * this.nextF64();
  }
  // Leva, "A fast normal random number generator", ACM TOMS 18 (1992).
  standardNormal() {
    for (;;) {
      const u = this.nextF64();
      const v = 1.7156 * (this.nextF64() - 0.5);
      const x = u - 0.449871;
      const y = Math.abs(v) + 0.386595;
      const q = x * x + y * (0.196 * y - 0.25472 * x);
      if (u > 0 && (q < 0.27597 || (q <= 0.27846 && v * v <= -4 * u * u * Math.log(u)))) return v / u;
    }
  }
  normal(loc, scale) {
    return loc + scale * this.standardNormal();
  }
  exponential(scale) {
    return scale * (0 - Math.log(1 - this.nextF64()));
  }
  // Marsaglia and Tsang, "A simple method for generating gamma variables", ACM TOMS 26 (2000).
  gammaUnit(shape) {
    if (shape < 1) {
      const boosted = this.gammaUnit(shape + 1);
      return boosted * Math.pow(this.nextF64(), 1 / shape);
    }
    const d = shape - 1 / 3;
    const c = 1 / Math.sqrt(9 * d);
    for (;;) {
      const x = this.standardNormal();
      const t = 1 + c * x;
      if (t <= 0) continue;
      const v = t * t * t;
      const u = this.nextF64();
      if (u < 1 - 0.0331 * (x * x) * (x * x)) return d * v;
      if (Math.log(u) < 0.5 * (x * x) + d * (1 - v + Math.log(v))) return d * v;
    }
  }
  gamma(shape, scale) {
    return this.gammaUnit(shape) * scale;
  }
  beta(a, b) {
    if (a <= 1 && b <= 1) {
      for (;;) {
        const u = this.nextF64();
        const v = this.nextF64();
        const x = Math.pow(u, 1 / a);
        const y = Math.pow(v, 1 / b);
        if (x + y <= 1 && u + v > 0) {
          if (x + y > 0) return x / (x + y);
          let logX = Math.log(u) / a;
          let logY = Math.log(v) / b;
          const largest = Math.max(logX, logY);
          logX -= largest;
          logY -= largest;
          return Math.exp(logX - Math.log(Math.exp(logX) + Math.exp(logY)));
        }
      }
    }
    const x = this.gammaUnit(a);
    const y = this.gammaUnit(b);
    return x / (x + y);
  }
  chisquare(df) {
    return 2 * this.gammaUnit(df / 2);
  }
  laplace(loc, scale) {
    for (;;) {
      const u = this.nextF64();
      if (u >= 0.5) return loc - scale * Math.log(2 - u - u);
      if (u > 0) return loc + scale * Math.log(u + u);
    }
  }
  lognormal(mean, sigma) {
    return Math.exp(this.normal(mean, sigma));
  }
  standardT(df) {
    const z = this.standardNormal();
    const g = this.gammaUnit(df / 2);
    return (Math.sqrt(df / 2) * z) / Math.sqrt(g);
  }
  fDist(dfnum, dfden) {
    const above = this.gammaUnit(dfnum / 2);
    const below = this.gammaUnit(dfden / 2);
    return (2 * above * dfden) / (2 * below * dfnum);
  }
  bernoulli(p) {
    return this.nextF64() < p;
  }
  binomialInversion(n, p) {
    const q = 1 - p;
    const qn = Math.exp(n * Math.log(q));
    const np = n * p;
    const bound = Math.trunc(Math.min(n, np + 10 * Math.sqrt(np * q + 1)));
    let x = 0;
    let px = qn;
    let u = this.nextF64();
    while (u > px) {
      x++;
      if (x > bound) {
        x = 0;
        px = qn;
        u = this.nextF64();
      } else {
        u -= px;
        px = ((n - x + 1) * p * px) / (x * q);
      }
    }
    return x;
  }
  // BTPE: Kachitvichyanukul and Schmeiser, "Binomial random variate generation", CACM 31 (1988), steps 0 to 6.
  binomialBtpe(n, r) {
    const q = 1 - r;
    const nrq = n * r * q;
    const fm = n * r + r;
    const m = Math.floor(fm);
    const p1 = Math.floor(2.195 * Math.sqrt(nrq) - 4.6 * q) + 0.5;
    const xm = m + 0.5;
    const xl = xm - p1;
    const xr = xm + p1;
    const c = 0.134 + 20.5 / (15.3 + m);
    let a = (fm - xl) / (fm - xl * r);
    const laml = a * (1 + a / 2);
    a = (xr - fm) / (xr * q);
    const lamr = a * (1 + a / 2);
    const p2 = p1 * (1 + 2 * c);
    const p3 = p2 + c / laml;
    const p4 = p3 + c / lamr;
    const tail = (value) => {
      const squared = value * value;
      return (13680 - (462 - (132 - (99 - 140 / squared) / squared) / squared) / squared) / value / 166320;
    };
    for (;;) {
      const u = this.nextF64() * p4;
      let v = this.nextF64();
      let y;
      if (u <= p1) return Math.floor(xm - p1 * v + u);
      if (u <= p2) {
        const x = xl + (u - p1) / c;
        v = v * c + 1 - Math.abs(m - x + 0.5) / p1;
        if (v > 1) continue;
        y = Math.floor(x);
      } else if (u <= p3) {
        y = Math.floor(xl + Math.log(v) / laml);
        if (y < 0 || v === 0) continue;
        v = v * (u - p2) * laml;
      } else {
        y = Math.floor(xr - Math.log(v) / lamr);
        if (y > n || v === 0) continue;
        v = v * (u - p3) * lamr;
      }
      const k = Math.abs(y - m);
      if (k > 20 && k < nrq / 2 - 1) {
        const rho = (k / nrq) * ((k * (k / 3 + 0.625) + 0.16666666666666666) / nrq + 0.5);
        const t = (-k * k) / (2 * nrq);
        const logV = Math.log(v);
        if (logV < t - rho) return y;
        if (logV > t + rho) continue;
        const x1 = y + 1;
        const f1 = m + 1;
        const z = n + 1 - m;
        const w = n - y + 1;
        const limit = xm * Math.log(f1 / x1) + (n - m + 0.5) * Math.log(z / w) + (y - m) * Math.log((w * r) / (x1 * q))
          + tail(f1) + tail(z) + tail(x1) + tail(w);
        if (logV > limit) continue;
        return y;
      }
      const s = r / q;
      const scaled = s * (n + 1);
      let f = 1;
      if (m < y) for (let i = m + 1; i <= y; i++) f *= scaled / i - s;
      else if (m > y) for (let i = y + 1; i <= m; i++) f /= scaled / i - s;
      if (v > f) continue;
      return y;
    }
  }
  binomial(n, p) {
    if (n === 0 || p === 0) return 0;
    if (p <= 0.5) return p * n <= 30 ? this.binomialInversion(n, p) : this.binomialBtpe(n, p);
    const q = 1 - p;
    return n - (q * n <= 30 ? this.binomialInversion(n, q) : this.binomialBtpe(n, q));
  }
  // PTRS: Hörmann, "The transformed rejection method for generating Poisson random variables" (1993).
  poisson(lam) {
    if (lam === 0) return 0;
    if (lam < 10) {
      const limit = Math.exp(-lam);
      let count = 0;
      let product = 1;
      for (;;) {
        product *= this.nextF64();
        if (product > limit) count++;
        else return count;
      }
    }
    const logGamma = (x) => {
      const a = [8.333333333333333e-02, -2.777777777777778e-03, 7.936507936507937e-04, -5.952380952380952e-04, 8.417508417508418e-04,
        -1.917526917526918e-03, 6.410256410256410e-03, -2.955065359477124e-02, 1.796443723688307e-01, -1.39243221690590e+00];
      if (x === 1 || x === 2) return 0;
      const n = x < 7 ? Math.trunc(7 - x) : 0;
      let x0 = x + n;
      const x2 = (1 / x0) * (1 / x0);
      let gl0 = a[9];
      for (let k = 8; k >= 0; k--) gl0 = gl0 * x2 + a[k];
      let gl = gl0 / x0 + 0.5 * 1.8378770664093453 + (x0 - 0.5) * Math.log(x0) - x0;
      for (let k = 1; k <= n; k++) {
        gl -= Math.log(x0 - 1);
        x0 -= 1;
      }
      return gl;
    };
    const slam = Math.sqrt(lam);
    const loglam = Math.log(lam);
    const b = 0.931 + 2.53 * slam;
    const a = -0.059 + 0.02483 * b;
    const invalpha = 1.1239 + 1.1328 / (b - 3.4);
    const vr = 0.9277 - 3.6224 / (b - 2);
    for (;;) {
      const u = this.nextF64() - 0.5;
      const v = this.nextF64();
      const us = 0.5 - Math.abs(u);
      const k = Math.floor(((2 * a) / us + b) * u + lam + 0.43);
      if (us >= 0.07 && v <= vr) return k;
      if (k < 0 || (us < 0.013 && v > us)) continue;
      if (Math.log(v) + Math.log(invalpha) - Math.log(a / (us * us) + b) <= -lam + k * loglam - logGamma(k + 1)) return k;
    }
  }
  geometric(p) {
    const trials = Math.ceil(Math.log(1 - this.nextF64()) / Math.log1p(-p));
    return Math.max(1, trials);
  }
}

// ---- The model against NumPy 2.5.1 ----

const numpyRaw = {
  0: [11749869230777074271n, 4976686463289251617n, 755828109848996024n, 304881062738325533n, 15002187965291974971n, 16837368535893154894n],
  1: [9441442522235856127n, 17532960557476522086n, 2659275481604167885n, 17499493567006797778n, 5752274989370667689n, 7808994663829368904n],
  42: [14276969152011380360n, 8095878257575067585n, 15838336090824644132n, 12864169557245331597n, 1737265434024182251n, 17997055833233904524n],
  4294967296: [16412783775159424549n, 10277383025879800780n, 14774146505460541886n, 17644565593934502278n, 1081258858114640722n, 4360823125038362322n],
  "18446744073709551615": [12544278110101001871n, 15593249672699323225n, 136562751618339402n, 16501869284920798641n, 2378988675683841820n, 5881532399743467424n],
};
for (const [seed, expected] of Object.entries(numpyRaw)) {
  const generator = Pcg64.seeded(BigInt(seed));
  assert.deepEqual(expected.map(() => generator.nextU64()), expected, `PCG64(${seed}).random_raw`);
}
// SeedSequence(entropy).generate_state(4, uint64)
assert.deepEqual(SeedSequence.of([42n]).state(), [11465652750463011511n, 15382171918060459190n, 9018504550953525431n, 3703499796004394495n]);
assert.deepEqual(SeedSequence.of([1n, 2n, 3n]).state(), [12997252459554536576n, 4810247055756232647n, 1222713447732757573n, 10992519347067621260n]);
assert.deepEqual(SeedSequence.of([(1n << 40n) + 7n, 5n]).state(), [9777067634972100749n, 18392096798471181453n, 7862492495128036046n, 10701036481666473693n]);
assert.deepEqual(SeedSequence.of([]).state(), [15793235383387715774n, 12390638538380655177n, 2361836109651742017n, 3188717715514472916n]);
assert.deepEqual(SeedSequence.of([0n]).state(), SeedSequence.of([]).state());
{
  // parent = SeedSequence([7, 2**63 + 11]); parent.spawn(3); parent.spawn(2); children[1].spawn(2)
  const parent = SeedSequence.of([7n, (1n << 63n) + 11n]);
  const children = parent.spawn(3);
  assert.deepEqual(children.map((child) => child.state()), [
    [9467426937815421579n, 17274181305795733317n, 5385912142169205373n, 15697225709272289533n],
    [14835283281034365071n, 3689496229629122584n, 14171601595520173601n, 11289829423028649271n],
    [15287538432996742217n, 1974489374459617234n, 1660183082763424996n, 13304653758203774397n],
  ]);
  assert.deepEqual(parent.spawn(2).map((child) => child.state()), [
    [14487776950770993554n, 614341224978773801n, 9130451947184798615n, 13921495007098111200n],
    [10966134542246220462n, 3372343651952155706n, 7704953427768063727n, 11539541966683601391n],
  ]);
  assert.deepEqual(children[1].spawn(2).map((child) => child.state()), [
    [10462788738530288591n, 6918442967145667235n, 2513088580779623796n, 4737267928676636920n],
    [13007891437525253046n, 10120536594390247973n, 1233990705112142248n, 14733555809767741508n],
  ]);
  const third = new Pcg64(children[2]);
  assert.deepEqual([0, 1, 2].map(() => third.nextU64()), [3805353529435280501n, 4490927405843939139n, 13565916614012661666n]);
}
{
  // Generator(PCG64(42)).random(4), and binomial and poisson from Generator(PCG64(seed)).
  const generator = Pcg64.seeded(42n);
  assert.deepEqual([0, 1, 2, 3].map(() => generator.nextF64()), [0.7739560485559633, 0.4388784397520523, 0.8585979199113825, 0.6973680290593639]);
  const draws = (seed, count, draw) => {
    const source = Pcg64.seeded(seed);
    return Array.from({ length: count }, () => draw(source));
  };
  assert.deepEqual(draws(7n, 12, (g) => g.binomial(20, 0.3)), [7, 9, 8, 4, 5, 8, 1, 8, 8, 6, 5, 5]);
  assert.deepEqual(draws(7n, 12, (g) => g.binomial(1000, 0.4)), [400, 391, 386, 374, 396, 405, 398, 406, 397, 405, 427, 385]);
  assert.deepEqual(draws(8n, 12, (g) => g.binomial(1000, 0.85)), [859, 855, 834, 843, 856, 847, 848, 842, 853, 882, 843, 853]);
  assert.deepEqual(draws(9n, 8, (g) => g.binomial(1e12, 0.25)), [250000126847, 250000019777, 250000030827, 249999617862, 250000548159, 249999217606, 250000998312, 249999726654]);
  assert.deepEqual(draws(7n, 12, (g) => g.poisson(3.5)), [4, 1, 4, 5, 3, 1, 1, 5, 0, 2, 1, 4]);
  assert.deepEqual(draws(7n, 12, (g) => g.poisson(250)), [256, 264, 240, 265, 241, 238, 250, 236, 255, 248, 256, 250]);
  assert.deepEqual(draws(7n, 12, (g) => g.poisson(10)), [13, 8, 13, 8, 7, 10, 7, 11, 11, 10, 12, 9]);
}

// ---- The `random` suite (tests/fixtures/random) ----

const wrap = (total, value) => signed(total * 31n + BigInt(value));
const nthRaw = (generator, index) => {
  let value = 0n;
  for (let i = 0; i <= index; i++) value = generator.nextU64();
  return signed(value);
};
const nth = (draw, index) => {
  let value;
  for (let i = 0; i <= index; i++) value = draw();
  return value;
};

export const cases = [];
const seeds = [0n, 1n, 42n, -1n, 4294967296n, 0x123456789abcdefn];
for (const seed of seeds) {
  for (const index of [0, 1, 5, 40]) cases.push(["raw_at", [seed, BigInt(index)], nthRaw(Pcg64.seeded(seed), index)]);
}

const entropies = [[], [0n], [1n, 2n, 3n], [(1n << 40n) + 7n, 5n], [-1n, -1n, -1n], [7n, (1n << 63n) + 11n], [0n, 0n], [0n, 0n, 0n]];
for (const entropy of entropies) {
  const [a = 0n, b = 0n, c = 0n] = entropy;
  for (const index of [0, 2]) {
    const sequence = SeedSequence.of(entropy.map((value) => BigInt.asUintN(64, value)));
    cases.push(["sequence_raw_at", [BigInt(entropy.length), signed(a), signed(b), signed(c), BigInt(index)], nthRaw(new Pcg64(sequence), index)]);
  }
}

for (const [a, b] of [[7n, (1n << 63n) + 11n], [0n, 0n], [42n, 1n]]) {
  for (const [first, second] of [[3, 2], [1, 0], [0, 4], [5, 5]]) {
    for (let child = 0; child < first + second; child++) {
      const parent = SeedSequence.of([a, b]);
      const children = [...parent.spawn(first), ...parent.spawn(second)];
      cases.push(["spawn_raw_at", [signed(a), signed(b), BigInt(first), BigInt(second), BigInt(child), 1n], nthRaw(new Pcg64(children[child]), 1)]);
    }
  }
  for (const child of [0, 1, 2]) {
    for (const grandchild of [0, 1]) {
      const picked = SeedSequence.of([a, b]).spawn(3)[child];
      cases.push(["grandchild_raw_at", [signed(a), signed(b), BigInt(child), BigInt(grandchild), 0n], nthRaw(new Pcg64(picked.spawn(2)[grandchild]), 0)]);
    }
  }
}
for (const seed of [0n, 42n]) cases.push(["parent_raw_after_spawn", [seed, 3n], nthRaw(Pcg64.seeded(seed), 3)]);

for (const seed of seeds) {
  for (const index of [0, 3, 30]) {
    const generator = Pcg64.seeded(seed);
    cases.push(["unit_scaled_at", [seed, BigInt(index)], nth(() => generator.nextU64() >> 11n, index)]);
  }
}

const i64Min = -(1n << 63n);
const i64Max = (1n << 63n) - 1n;
const ranges = [[0n, 1n], [0n, 2n], [-5n, 5n], [0n, 6n], [10n, 13n], [0n, i64Max], [i64Min, i64Max], [-3n, 1n << 62n], [0n, 1000000007n],
  [i64Min, i64Min + 3n], [i64Max - 1n, i64Max], [0n, (1n << 63n) - 25n]];
for (const [low, high] of ranges) {
  for (const seed of [1n, 42n]) {
    for (const index of [0, 7]) {
      const generator = Pcg64.seeded(seed);
      cases.push(["range_at", [low, high, seed, BigInt(index)], nth(() => generator.range(low, high), index)]);
    }
    const generator = Pcg64.seeded(seed);
    let total = 0n;
    for (let i = 0; i < 300; i++) total = wrap(total, generator.range(low, high));
    cases.push(["range_mix", [low, high, seed, 300n], total]);
  }
}

// [kind, a, b]: the kinds of `draw_real` in the fixture.
const reals = [
  [0, 0, 0], [1, -1, 3], [1, 1e10, 1e10 + 1], [1, -1e300, 1e300], [2, 0, 0], [3, 10, 2.5], [3, -4, 0], [4, 2, 0], [4, 0, 0],
  [5, 2.5, 1.5], [5, 0.3, 2], [5, 1, 1], [5, 100, 0.01], [5, 0.01, 1], [6, 2, 5], [6, 0.5, 0.5], [6, 0.2, 0.9], [6, 1, 3], [6, 1, 1],
  [6, 0.001, 0.002], [6, 30, 0.5], [7, 3, 0], [7, 0.5, 0], [8, -3, 1.5], [8, 2, 0], [9, 0.5, 0.75], [10, 5, 0], [10, 0.7, 0], [11, 4, 12],
  [11, 0.8, 3],
];
const drawReal = (generator, kind, a, b) => [
  () => generator.nextF64(), () => generator.uniform(a, b), () => generator.standardNormal(), () => generator.normal(a, b),
  () => generator.exponential(a), () => generator.gamma(a, b), () => generator.beta(a, b), () => generator.chisquare(a),
  () => generator.laplace(a, b), () => generator.lognormal(a, b), () => generator.standardT(a), () => generator.fDist(a, b),
][kind];
for (const [kind, a, b] of reals) {
  for (const seed of [1n, 42n]) {
    for (const index of [0, 3, 17]) {
      const generator = Pcg64.seeded(seed);
      const expected = nth(drawReal(generator, kind, a, b), index);
      assert.ok(Number.isFinite(expected), `distribution ${kind}(${a}, ${b}) is finite`);
      cases.push(["real_close_at", [BigInt(kind), a, b, seed, BigInt(index), expected], 1n]);
    }
  }
  const generator = Pcg64.seeded(7n);
  const draw = drawReal(generator, kind, a, b);
  let total = 0;
  for (let i = 0; i < 500; i++) total += draw();
  cases.push(["real_sum_close", [BigInt(kind), a, b, 7n, 500n, total, signed(generator.nextU64())], 1n]);
}

// [kind, n, p]: 0 binomial, 1 poisson (rate p), 2 geometric, 3 bernoulli.
const counts = [
  [0, 20, 0.3], [0, 1000, 0.4], [0, 1000, 0.85], [0, 1e12, 0.25], [0, 5, 0.5], [0, 1, 0.999], [0, 100000, 0.5], [0, 61, 0.5], [0, 60, 0.5],
  [0, 0, 0.5], [0, 9, 0], [0, 9, 1], [0, 300, 0.1], [0, 300, 0.9], [0, 2 ** 40, 1e-9], [0, 2 ** 40, 0.5],
  [1, 0, 3.5], [1, 0, 250], [1, 0, 10], [1, 0, 9.999], [1, 0, 1e6], [1, 0, 0.01], [1, 0, 0], [1, 0, 1e15],
  [2, 0, 0.5], [2, 0, 0.01], [2, 0, 1], [2, 0, 1e-9], [2, 0, 0.34], [2, 0, 0.999999],
  [3, 0, 0.3], [3, 0, 0], [3, 0, 1],
];
const drawCount = (generator, kind, n, p) => [
  () => generator.binomial(n, p), () => generator.poisson(p), () => generator.geometric(p), () => (generator.bernoulli(p) ? 1 : 0),
][kind];
for (const [kind, n, p] of counts) {
  for (const seed of [1n, 42n]) {
    for (const index of [0, 9]) {
      const generator = Pcg64.seeded(seed);
      cases.push(["count_at", [BigInt(kind), BigInt(n), p, seed, BigInt(index)], BigInt(nth(drawCount(generator, kind, n, p), index))]);
    }
  }
  const generator = Pcg64.seeded(7n);
  const draw = drawCount(generator, kind, n, p);
  let total = 0n;
  for (let i = 0; i < 400; i++) total = wrap(total, draw());
  cases.push(["count_mix", [BigInt(kind), BigInt(n), p, 7n, 400n], wrap(total, signed(generator.nextU64()))]);
}

export const traps = [
  ["trap_range_empty", [3n]], ["trap_range_empty", [i64Min]], ["trap_spawn_negative", [-1n]],
  ["trap_real", [0n, 2, 2]], ["trap_real", [0n, 3, 1]], ["trap_real", [0n, NaN, 1]], ["trap_real", [0n, 0, Infinity]], ["trap_real", [0n, -1e308, 1e308]],
  ["trap_real", [3n, 0, -1]], ["trap_real", [3n, NaN, 1]], ["trap_real", [3n, 0, Infinity]], ["trap_real", [3n, 0, NaN]], ["trap_real", [3n, Infinity, 1]],
  ["trap_real", [4n, -1, 0]], ["trap_real", [4n, Infinity, 0]], ["trap_real", [4n, NaN, 0]],
  ["trap_real", [5n, 0, 1]], ["trap_real", [5n, -1, 1]], ["trap_real", [5n, 1, -1]], ["trap_real", [5n, Infinity, 1]], ["trap_real", [5n, 1, NaN]],
  ["trap_real", [6n, 0, 1]], ["trap_real", [6n, 1, 0]], ["trap_real", [6n, 1, Infinity]], ["trap_real", [6n, NaN, 1]],
  ["trap_real", [7n, 0, 0]], ["trap_real", [7n, -2, 0]], ["trap_real", [7n, Infinity, 0]],
  ["trap_real", [8n, 0, -1]], ["trap_real", [8n, NaN, 1]],
  ["trap_real", [9n, 0, -1]], ["trap_real", [9n, Infinity, 1]],
  ["trap_real", [10n, 0, 0]], ["trap_real", [10n, NaN, 0]],
  ["trap_real", [11n, 0, 1]], ["trap_real", [11n, 1, 0]], ["trap_real", [11n, 1, Infinity]],
  ["trap_count", [0n, -1n, 0.5]], ["trap_count", [0n, 5n, 1.5]], ["trap_count", [0n, 5n, -0.1]], ["trap_count", [0n, 5n, NaN]],
  ["trap_count", [1n, 0n, -1]], ["trap_count", [1n, 0n, 1e19]], ["trap_count", [1n, 0n, NaN]], ["trap_count", [1n, 0n, Infinity]],
  ["trap_count", [2n, 0n, 0]], ["trap_count", [2n, 0n, 1.5]], ["trap_count", [2n, 0n, NaN]], ["trap_count", [2n, 0n, -0.5]],
  ["trap_count", [3n, 0n, 1.1]], ["trap_count", [3n, 0n, -0.1]], ["trap_count", [3n, 0n, NaN]],
];

// ---- The `random_tensor` suite (tests/fixtures/random_tensor) ----
//
// A tensor is the scalar draws in row-major order (the fixture compares them bit for bit), so the model only has
// the functions that `Random` does not have: the Fisher and Yates order, the weighted choice, Dirichlet vectors and
// multivariate normal vectors.

const mixAll = (start, values) => values.reduce((total, value) => wrap(total, value), BigInt(start));
const mixGenerator = (total, generator) => wrap(total, signed(generator.nextU64()));

// Element i is swapped with one of the elements from i on.
function shuffledIndices(generator, count) {
  const order = Array.from({ length: count }, (_, index) => index);
  for (let index = 0; index < count - 1; index++) {
    const other = Number(generator.range(BigInt(index), BigInt(count)));
    [order[index], order[other]] = [order[other], order[index]];
  }
  return order;
}

function choiceWeighted(generator, count, replace, weights) {
  const running = [];
  let total = 0;
  let last = 0;
  weights.forEach((weight, index) => {
    total += weight;
    running.push(total);
    if (weight > 0) last = index;
  });
  if (replace) {
    return Array.from({ length: count }, () => {
      const point = generator.nextF64() * total;
      const found = running.findIndex((sum) => sum > point);
      return found < 0 ? last : found;
    });
  }
  // Exponential clocks of rate `weight`: the order of the clocks is the order of draws without replacement.
  const clocks = [];
  weights.forEach((weight, index) => {
    if (weight > 0) clocks.push([(0 - Math.log(1 - generator.nextF64())) / weight, index]);
  });
  clocks.sort((left, right) => left[0] - right[0] || left[1] - right[1]);
  return clocks.slice(0, count).map(([, index]) => index);
}

function dirichlet(generator, alpha, rows) {
  const k = alpha.length;
  const values = [];
  if (Math.max(...alpha) < 0.1) {
    const rest = new Array(k).fill(0);
    for (let index = k - 2; index >= 0; index--) rest[index] = rest[index + 1] + alpha[index + 1];
    for (let row = 0; row < rows; row++) {
      let remaining = 1;
      for (let index = 0; index < k - 1; index++) {
        const share = generator.beta(alpha[index], rest[index]);
        values.push(remaining * share);
        remaining *= 1 - share;
      }
      values.push(remaining);
    }
    return values;
  }
  for (let row = 0; row < rows; row++) {
    const samples = alpha.map((parameter) => generator.gamma(parameter, 1));
    const total = samples.reduce((sum, sample) => sum + sample, 0);
    values.push(...samples.map((sample) => sample / total));
  }
  return values;
}

// The lower Cholesky factor, row by row (Cholesky and Banachiewicz).
function cholesky(cov, n) {
  const lower = new Array(n * n).fill(0);
  for (let i = 0; i < n; i++) {
    for (let j = 0; j <= i; j++) {
      let sum = cov[i * n + j];
      for (let k = 0; k < j; k++) sum -= lower[i * n + k] * lower[j * n + k];
      lower[i * n + j] = i === j ? Math.sqrt(sum) : sum / lower[j * n + j];
    }
  }
  return lower;
}

function multivariateNormal(generator, mean, cov, rows) {
  const n = mean.length;
  const lower = cholesky(cov, n);
  const values = [];
  for (let row = 0; row < rows; row++) {
    const normals = mean.map(() => generator.standardNormal());
    for (let i = 0; i < n; i++) {
      let value = mean[i];
      for (let j = 0; j <= i; j++) value += lower[i * n + j] * normals[j];
      values.push(value);
    }
  }
  return values;
}

export const tensorCases = [];
for (const [kind, a, b] of reals.filter(([kind]) => kind >= 1)) {
  for (const [rows, cols] of [[3, 4], [1, 1], [0, 5], [7, 2]]) {
    tensorCases.push(["real_tensor_is_scalar_draws", [BigInt(kind), a, b, BigInt(rows), BigInt(cols), 42n], 1n]);
  }
}
for (const [kind, n, p] of counts) {
  for (const extent of [0, 1, 5]) tensorCases.push(["count_tensor_is_scalar_draws", [BigInt(kind), BigInt(n), p, BigInt(extent), 42n], 1n]);
}
for (const [low, high] of ranges) {
  for (const [rows, cols] of [[4, 5], [1, 0]]) {
    const generator = Pcg64.seeded(11n);
    const values = Array.from({ length: rows * cols }, () => generator.range(low, high));
    tensorCases.push(["integers_mix", [low, high, BigInt(rows), BigInt(cols), 11n], mixGenerator(mixAll(0, values), generator)]);
  }
}
for (const count of [0, 1, 2, 3, 10, 257]) {
  for (const seed of [1n, 42n]) {
    const generator = Pcg64.seeded(seed);
    const order = shuffledIndices(generator, count);
    assert.deepEqual([...order].sort((x, y) => x - y), order.map((_, index) => index));
    tensorCases.push(["permutation_mix", [BigInt(count), seed], mixGenerator(mixAll(count, order), generator)]);
  }
}
for (const [rows, cols] of [[5, 3], [1, 4], [4, 1], [0, 3], [3, 0], [9, 9]]) {
  for (const axis of [0, 1, -1, -2]) {
    const generator = Pcg64.seeded(5n);
    const along = (axis + 2) % 2;
    const order = shuffledIndices(generator, along === 0 ? rows : cols);
    const values = [];
    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < cols; col++) values.push(along === 0 ? order[row] * cols + col : row * cols + order[col]);
    }
    tensorCases.push(["shuffle_mix", [BigInt(rows), BigInt(cols), BigInt(axis), 5n], mixGenerator(mixAll(0, values), generator)]);
  }
  // Axis 0 of the transposed window is axis 1 of the buffer.
  const generator = Pcg64.seeded(6n);
  const order = shuffledIndices(generator, cols);
  const values = [];
  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < cols; col++) values.push(row * cols + order[col]);
  }
  tensorCases.push(["shuffle_transposed_mix", [BigInt(rows), BigInt(cols), 6n], mixGenerator(mixAll(0, values), generator)]);
}
for (const seed of [1n, 42n]) {
  const generator = Pcg64.seeded(seed);
  const order = shuffledIndices(generator, 4);
  const values = [];
  for (let a = 0; a < 2; a++) {
    for (let i = 0; i < 4; i++) {
      for (let c = 0; c < 3; c++) values.push(a * 12 + order[i] * 3 + c);
    }
  }
  tensorCases.push(["shuffle_cube_mix", [seed], mixGenerator(mixAll(0, values), generator)]);
}

const weightSets = [[0.1, 0.2, 0.3, 0.4], [0, 5, 0, 1, 2, 0], [1], [1e-300, 1, 1e300], new Array(12).fill(3)];
weightSets.forEach((weights, kind) => {
  const positive = weights.filter((weight) => weight > 0).length;
  for (const replace of [1, 0]) {
    for (const count of [0, 1, 2, 3, 6, 12, 40].filter((count) => replace === 1 || count <= positive)) {
      for (const seed of [1n, 42n]) {
        const generator = Pcg64.seeded(seed);
        const picked = choiceWeighted(generator, count, replace === 1, weights);
        assert.ok(picked.every((index) => weights[index] > 0), "a slice of weight 0 is never drawn");
        if (replace === 0) assert.equal(new Set(picked).size, count, "draws without replacement are distinct");
        const values = picked.flatMap((index) => [2 * index, 2 * index + 1]);
        tensorCases.push(["choice_mix", [BigInt(count), BigInt(replace), BigInt(kind), seed], mixGenerator(mixAll(count, values), generator)]);
      }
    }
  }
});

const alphaSets = [[1, 2, 3], [0.05, 0.02, 0.01, 0.09], [5], [0.5, 0.5], [0.09, 30]];
alphaSets.forEach((alpha, kind) => {
  for (const seed of [1n, 42n]) {
    const rows = 6;
    const generator = Pcg64.seeded(seed);
    const values = dirichlet(generator, alpha, rows);
    for (let row = 0; row < rows; row++) {
      const total = values.slice(row * alpha.length, (row + 1) * alpha.length).reduce((sum, value) => sum + value, 0);
      assert.ok(Math.abs(total - 1) < 1e-12, "a Dirichlet vector sums to 1");
    }
    for (const index of [0, alpha.length - 1, values.length - 1]) {
      tensorCases.push(["dirichlet_close_at", [BigInt(kind), BigInt(rows), seed, BigInt(index), values[index]], 1n]);
    }
    tensorCases.push(["dirichlet_generator_after", [BigInt(kind), BigInt(rows), seed], mixGenerator(0n, generator)]);
  }
});

const normalSets = [
  [[1, -1], [2, 0.5, 0.5, 1]],
  [[0, 10, -5], [4, 2, 0.6, 2, 2, 0.5, 0.6, 0.5, 3]],
  [[2.5], [0.25]],
];
normalSets.forEach(([mean, cov], kind) => {
  for (const seed of [1n, 42n]) {
    const rows = 5;
    const generator = Pcg64.seeded(seed);
    const values = multivariateNormal(generator, mean, cov, rows);
    for (const index of [0, mean.length - 1, values.length - 1]) {
      tensorCases.push(["mvn_close_at", [BigInt(kind), BigInt(rows), seed, BigInt(index), values[index]], 1n]);
    }
    // The fixture draws `[rows, 2]` vectors here.
    const twice = Pcg64.seeded(seed);
    multivariateNormal(twice, mean, cov, rows * 2);
    tensorCases.push(["mvn_generator_after", [BigInt(kind), BigInt(rows), seed], mixGenerator(0n, twice)]);
  }
});
// Shapes that do not fit are ShapeMismatch (1); a covariance that is not symmetric positive definite is
// NotPositiveDefinite (2).
[0n, 1n, 2n, 2n, 1n, 1n, 1n].forEach((code, kind) => tensorCases.push(["mvn_error", [BigInt(kind)], code]));
tensorCases.push(["empty_shapes_draw_nothing", [42n], 1n], ["empty_shapes_draw_nothing", [0n], 1n]);

export const tensorTraps = [
  ["trap_real_tensor", [1n, 2, 2]], ["trap_real_tensor", [1n, 0, Infinity]], ["trap_real_tensor", [3n, 0, -1]], ["trap_real_tensor", [3n, NaN, 1]],
  ["trap_real_tensor", [4n, -1, 0]], ["trap_real_tensor", [5n, 0, 1]], ["trap_real_tensor", [5n, 1, -1]], ["trap_real_tensor", [6n, 0, 1]],
  ["trap_real_tensor", [6n, 1, 0]], ["trap_real_tensor", [7n, 0, 0]], ["trap_real_tensor", [8n, 0, -1]], ["trap_real_tensor", [9n, 0, -1]],
  ["trap_real_tensor", [9n, Infinity, 1]], ["trap_real_tensor", [10n, 0, 0]], ["trap_real_tensor", [11n, 0, 1]], ["trap_real_tensor", [11n, 1, 0]],
  ["trap_count_tensor", [0n, -1n, 0.5]], ["trap_count_tensor", [0n, 1n, 2]], ["trap_count_tensor", [1n, 0n, -1]], ["trap_count_tensor", [1n, 0n, 1e19]],
  ["trap_count_tensor", [2n, 0n, 0]], ["trap_count_tensor", [2n, 0n, 1.5]], ["trap_count_tensor", [3n, 0n, 2]], ["trap_count_tensor", [3n, 0n, NaN]],
  ["trap_count_tensor", [4n, 5n, 0]], ["trap_count_tensor", [4n, 5n, -2]],
  ["trap_negative_extent", [-1n]], ["trap_permutation", [-1n]], ["trap_shuffle_axis", [2n]], ["trap_shuffle_axis", [-3n]],
  ["trap_choice", [0n, 0]], ["trap_choice", [1n, 0]], ["trap_choice", [2n, 0]], ["trap_choice", [3n, 0]], ["trap_choice", [4n, 0]],
  ["trap_choice", [5n, NaN]], ["trap_choice", [5n, Infinity]], ["trap_choice", [7n, 0]],
  ["trap_dirichlet", [0n, 0]], ["trap_dirichlet", [1n, 0]], ["trap_dirichlet", [2n, 0]], ["trap_dirichlet", [3n, Infinity]], ["trap_dirichlet", [3n, NaN]],
];
