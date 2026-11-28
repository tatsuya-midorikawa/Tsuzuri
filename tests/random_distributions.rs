//! NC10: `Random.Pcg64`, `Random.SeedSequence`, the scalar distributions, and the opt-in `RandomTensor`.
//!
//! Expected values are independent of `std/Random.tz`: raw outputs are the numbers that NumPy 2.5.1 prints
//! (`numpy.random.PCG64(seed).random_raw()`, `SeedSequence.spawn`), moments and probabilities are the textbook
//! formulas of each distribution (sample statistics must be within four standard errors for a fixed seed), and
//! `tests/features.mjs` (`random`, `random_tensor`) compares every sample with a BigInt model.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::capabilities;
use tsuzuri::driver::{AnalysisKind, Project};
use tsuzuri::parser;
use tsuzuri::syntax::Visibility;

fn scratch_dir(prefix: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/random_distributions")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn write_project(root: &std::path::Path, source: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"random-suite\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
}

fn run_tz_test(source: &str) {
    let root = scratch_dir("suite");
    write_project(&root, source);
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        out.status.success(),
        "tsuzuri test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The statistics helpers shared by the test programs: sample mean and variance against the mean `mu`, the
/// variance `variance` and the excess kurtosis `kurtosis` of the distribution (the standard error of a sample
/// variance is `variance * sqrt((kurtosis + 2) / n)`), and the share of samples below a bound against a probability.
const STATISTICS: &str = r#"
def moments_ok :: own f64 -> own f64 -> own f64 -> own i64 -> own i64u -> own (Random.Pcg64 -> (f64 * Random.Pcg64)) -> bool
fn moments_ok mu variance kurtosis count seed draw =
    let mut state = Random.pcg64 seed
    let mut first = 0.0
    let mut second = 0.0
    for _ in 0 ..< count do
        let (sample, next) = draw state
        state = next
        let centered = sample - mu
        first = first + centered
        second = second + centered * centered
    let n = count as f64
    let mean = first / n
    let spread = (second - first * first / n) / (n - 1.0)
    Math.abs mean <= 4.0 * Math.sqrt (variance / n) && Math.abs (spread - variance) <= 4.0 * variance * Math.sqrt ((kurtosis + 2.0) / n)

def share_ok :: own f64 -> own f64 -> own i64 -> own i64u -> own (Random.Pcg64 -> (f64 * Random.Pcg64)) -> bool
fn share_ok bound probability count seed draw =
    let mut state = Random.pcg64 seed
    let mut below = 0
    for _ in 0 ..< count do
        let (sample, next) = draw state
        state = next
        if sample < bound then below = below + 1
    let n = count as f64
    Math.abs ((below as f64) / n - probability) <= 4.0 * Math.sqrt (probability * (1.0 - probability) / n)

def as_real :: own (i64 * Random.Pcg64) -> (f64 * Random.Pcg64)
fn as_real pair =
    let (value, next) = pair
    (value as f64, next)
"#;

#[test]
fn pcg64_and_seed_sequence_reproduce_the_numpy_streams() {
    // numpy.random.PCG64(seed).random_raw(3); PCG64(SeedSequence([1, 2, 3])).random_raw(3);
    // SeedSequence([7, 2**63 + 11]).spawn(3)[2] -> PCG64(child).random_raw(3); Generator(PCG64(42)).random(4).
    run_tz_test(
        r#"
def three :: own Random.Pcg64 -> [i64u]
fn three generator =
    let (a, first) = Random.pcg64_next_u64 generator
    let (b, second) = Random.pcg64_next_u64 first
    let (c, _) = Random.pcg64_next_u64 second
    [a, b, c]

test "pcg64 is numpy.random.PCG64" =
    assert (three (Random.pcg64 0i64u) == [11749869230777074271i64u, 4976686463289251617i64u, 755828109848996024i64u])
    assert (three (Random.pcg64 1i64u) == [9441442522235856127i64u, 17532960557476522086i64u, 2659275481604167885i64u])
    assert (three (Random.pcg64 42i64u) == [14276969152011380360i64u, 8095878257575067585i64u, 15838336090824644132i64u])
    assert (three (Random.pcg64 4294967296i64u) == [16412783775159424549i64u, 10277383025879800780i64u, 14774146505460541886i64u])
    assert (three (Random.pcg64 18446744073709551615i64u) == [12544278110101001871i64u, 15593249672699323225i64u, 136562751618339402i64u])

test "seed_sequence is numpy.random.SeedSequence" =
    let sequence = Random.seed_sequence [1i64u, 2i64u, 3i64u]
    assert (three (Random.pcg64_from_seed (ref sequence)) == [12368030237656201616i64u, 2047858591075935483i64u, 9435677134316251571i64u])
    // One seed is the sequence of that seed, and no integers are one zero.
    let single = Random.seed_sequence [42i64u]
    assert (three (Random.pcg64_from_seed (ref single)) == three (Random.pcg64 42i64u))
    let none: [i64u] = []
    let empty = Random.seed_sequence none
    assert (three (Random.pcg64_from_seed (ref empty)) == three (Random.pcg64 0i64u))

test "spawned children are the children of numpy" =
    let (children, parent) = Random.seed_sequence_spawn 3 (Random.seed_sequence [7i64u, 9223372036854775819i64u])
    assert (children.length == 3)
    assert (three (Random.pcg64_from_seed (ref children[2])) == [3805353529435280501i64u, 4490927405843939139i64u, 13565916614012661666i64u])
    // The parent keeps its own stream, and what it spawns next differs from every earlier child.
    let again = Random.seed_sequence [7i64u, 9223372036854775819i64u]
    assert (three (Random.pcg64_from_seed (ref parent)) == three (Random.pcg64_from_seed (ref again)))
    let (later, _) = Random.seed_sequence_spawn 2 parent
    let firsts = [
        (three (Random.pcg64_from_seed (ref children[0])))[0], (three (Random.pcg64_from_seed (ref children[1])))[0],
        (three (Random.pcg64_from_seed (ref children[2])))[0], (three (Random.pcg64_from_seed (ref later[0])))[0],
        (three (Random.pcg64_from_seed (ref later[1])))[0], (three (Random.pcg64_from_seed (ref again)))[0]]
    assert (Array.length (Array.distinct firsts) == 6)
    let (nobody, _) = Random.seed_sequence_spawn 0 (Random.seed_sequence [1i64u])
    assert (nobody.length == 0)

test "next_f64 is Generator.random" =
    let (a, first) = Random.pcg64_next_f64 (Random.pcg64 42i64u)
    let (b, second) = Random.pcg64_next_f64 first
    let (c, third) = Random.pcg64_next_f64 second
    let (d, _) = Random.pcg64_next_f64 third
    assert (a == 0.7739560485559633 && b == 0.4388784397520523 && c == 0.8585979199113825 && d == 0.6973680290593639)
"#,
    );
}

#[test]
fn spawned_streams_are_uncorrelated() {
    // The sample correlation of two independent streams of n uniform numbers is within 4 / sqrt(n) of 0, and the
    // share of equal bits of their outputs is within four standard errors of one half.
    let source = format!(
        "{STATISTICS}{}",
        r#"
def correlation :: own i64 -> own Random.Pcg64 -> own Random.Pcg64 -> f64
fn correlation count left right =
    let mut a = left
    let mut b = right
    let mut sum_x = 0.0
    let mut sum_y = 0.0
    let mut sum_xx = 0.0
    let mut sum_yy = 0.0
    let mut sum_xy = 0.0
    for _ in 0 ..< count do
        let (x, next_a) = Random.pcg64_next_f64 a
        let (y, next_b) = Random.pcg64_next_f64 b
        a = next_a
        b = next_b
        sum_x = sum_x + x
        sum_y = sum_y + y
        sum_xx = sum_xx + x * x
        sum_yy = sum_yy + y * y
        sum_xy = sum_xy + x * y
    let n = count as f64
    (n * sum_xy - sum_x * sum_y) / Math.sqrt ((n * sum_xx - sum_x * sum_x) * (n * sum_yy - sum_y * sum_y))

def equal_bits :: own i64 -> own Random.Pcg64 -> own Random.Pcg64 -> i64
fn equal_bits count left right =
    let mut a = left
    let mut b = right
    let mut equal = 0
    for _ in 0 ..< count do
        let (x, next_a) = Random.pcg64_next_u64 a
        let (y, next_b) = Random.pcg64_next_u64 b
        a = next_a
        b = next_b
        equal = equal + 64 - Int.count_ones (x ^ y)
    equal

test "children of one parent, a child and its parent, and neighbouring seeds" =
    let count = 20000
    let bound = 4.0 / Math.sqrt (count as f64)
    let (children, parent) = Random.seed_sequence_spawn 4 (Random.seed_sequence [2026i64u])
    for left in 0 ..< 4 do
        for right in (left + 1) ..< 4 do
            assert (Math.abs (correlation count (Random.pcg64_from_seed (ref children[left])) (Random.pcg64_from_seed (ref children[right]))) <= bound)
        assert (Math.abs (correlation count (Random.pcg64_from_seed (ref children[left])) (Random.pcg64_from_seed (ref parent))) <= bound)
    assert (Math.abs (correlation count (Random.pcg64 1i64u) (Random.pcg64 2i64u)) <= bound)
    // 64 * count fair bits: the standard error of the count of equal bits is sqrt(64 * count) / 2.
    let bits = (64 * count) as f64
    let equal = equal_bits count (Random.pcg64_from_seed (ref children[0])) (Random.pcg64_from_seed (ref children[1]))
    assert (Math.abs ((equal as f64) - bits / 2.0) <= 4.0 * Math.sqrt bits / 2.0)
"#
    );
    run_tz_test(&source);
}

#[test]
fn scalar_distributions_have_the_moments_of_their_formulas() {
    let source = format!(
        "{STATISTICS}{}",
        r#"
test "uniform, range and unit" =
    assert (moments_ok 0.5 (1.0 / 12.0) (-1.2) 40000 11i64u (\g -> Random.pcg64_next_f64 g))
    assert (moments_ok 1.0 (16.0 / 12.0) (-1.2) 40000 12i64u (\g -> Random.pcg64_uniform (-1.0) 3.0 g))
    // The integers 10 .. 19: mean 14.5, variance (10^2 - 1) / 12, excess kurtosis -1.2 * (10^2 + 1) / (10^2 - 1).
    assert (moments_ok 14.5 8.25 (-1.2242424242424241) 40000 13i64u (\g -> as_real (Random.pcg64_next_i64_range 10 20 g)))

test "every value of a range comes out equally often" =
    // Chi-square with 9 degrees of freedom: 27.88 is the 0.999 quantile.
    let mut state = Random.pcg64 14i64u
    let mut seen = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    for _ in 0 ..< 50000 do
        let (value, next) = Random.pcg64_next_i64_range (-3) 7 state
        state = next
        seen[value + 3] = seen[value + 3] + 1
    let mut statistic = 0.0
    for slot in 0 ..< 10 do
        let difference = (seen[slot] as f64) - 5000.0
        statistic = statistic + difference * difference / 5000.0
    assert (statistic < 27.88)

test "normal, lognormal and laplace" =
    assert (moments_ok 0.0 1.0 0.0 40000 21i64u (\g -> Random.pcg64_standard_normal g))
    assert (moments_ok 10.0 6.25 0.0 40000 22i64u (\g -> Random.pcg64_normal 10.0 2.5 g))
    // exp(m + s^2 / 2), (exp(s^2) - 1) exp(2m + s^2), exp(4s^2) + 2exp(3s^2) + 3exp(2s^2) - 6 for m = 0.5, s = 0.25.
    assert (moments_ok 1.7010573018484008 0.18662090474627 1.095931274730182 40000 23i64u (\g -> Random.pcg64_lognormal 0.5 0.25 g))
    assert (moments_ok (-3.0) 4.5 3.0 40000 24i64u (\g -> Random.pcg64_laplace (-3.0) 1.5 g))

test "exponential, gamma, chi-square and beta" =
    assert (moments_ok 2.0 4.0 6.0 40000 31i64u (\g -> Random.pcg64_exponential 2.0 g))
    assert (moments_ok 3.75 5.625 2.4 40000 32i64u (\g -> Random.pcg64_gamma 2.5 1.5 g))
    assert (moments_ok 0.6 1.2 20.0 40000 33i64u (\g -> Random.pcg64_gamma 0.3 2.0 g))
    assert (moments_ok 1.0 1.0 6.0 40000 34i64u (\g -> Random.pcg64_gamma 1.0 1.0 g))
    assert (moments_ok 3.0 6.0 4.0 40000 35i64u (\g -> Random.pcg64_chisquare 3.0 g))
    // a / (a + b), ab / ((a + b)^2 (a + b + 1)), 6((a - b)^2 (a + b + 1) - ab(a + b + 2)) / (ab(a + b + 2)(a + b + 3)).
    assert (moments_ok (2.0 / 7.0) (10.0 / 392.0) (-0.12) 40000 36i64u (\g -> Random.pcg64_beta 2.0 5.0 g))
    assert (moments_ok 0.5 0.125 (-1.5) 40000 37i64u (\g -> Random.pcg64_beta 0.5 0.5 g))
    assert (moments_ok 0.4 (0.24 / 3.5) (-0.9494949494949495) 40000 38i64u (\g -> Random.pcg64_beta 1.0 1.5 g))

test "student t and F" =
    // df / (df - 2) and 6 / (df - 4) for df = 10.
    assert (moments_ok 0.0 1.25 1.0 40000 41i64u (\g -> Random.pcg64_standard_t 10.0 g))
    // d2 / (d2 - 2), 2 d2^2 (d1 + d2 - 2) / (d1 (d2 - 2)^2 (d2 - 4)) and
    // 12 (d1 (5 d2 - 22)(d1 + d2 - 2) + (d2 - 4)(d2 - 2)^2) / (d1 (d2 - 6)(d2 - 8)(d1 + d2 - 2)) for d1 = 5, d2 = 20.
    assert (moments_ok (20.0 / 18.0) (18400.0 / 25920.0) (169848.0 / 19320.0) 40000 42i64u (\g -> Random.pcg64_f_dist 5.0 20.0 g))

test "bernoulli, binomial, poisson and geometric" =
    assert (moments_ok 0.3 0.21 (-1.2380952380952381) 40000 51i64u (\g -> match Random.pcg64_bernoulli 0.3 g with
        | (hit, next) -> (if hit then 1.0 else 0.0, next)))
    // np, npq and (1 - 6pq) / (npq).
    assert (moments_ok 6.0 4.2 (-0.06190476190476191) 40000 52i64u (\g -> as_real (Random.pcg64_binomial 20 0.3 g)))
    assert (moments_ok 400.0 240.0 (-0.0018333333333333333) 40000 53i64u (\g -> as_real (Random.pcg64_binomial 1000 0.4 g)))
    assert (moments_ok 850.0 127.5 0.0018431372549019608 40000 54i64u (\g -> as_real (Random.pcg64_binomial 1000 0.85 g)))
    assert (moments_ok 3.5 3.5 (1.0 / 3.5) 40000 55i64u (\g -> as_real (Random.pcg64_poisson 3.5 g)))
    assert (moments_ok 250.0 250.0 0.004 40000 56i64u (\g -> as_real (Random.pcg64_poisson 250.0 g)))
    assert (moments_ok 12.0 12.0 (1.0 / 12.0) 40000 57i64u (\g -> as_real (Random.pcg64_poisson 12.0 g)))
    // 1 / p, (1 - p) / p^2 and 6 + p^2 / (1 - p).
    assert (moments_ok 5.0 20.0 6.05 40000 58i64u (\g -> as_real (Random.pcg64_geometric 0.2 g)))
    assert (moments_ok 1.25 0.3125 9.2 40000 59i64u (\g -> as_real (Random.pcg64_geometric 0.8 g)))
"#
    );
    run_tz_test(&source);
}

#[test]
fn scalar_distributions_have_the_probabilities_of_their_formulas() {
    // The share of samples below a bound against the distribution function (computed apart from the compiler:
    // erfc for the normal, the series of the incomplete gamma function, exact sums for binomial and Poisson,
    // closed forms for the others).
    let source = format!(
        "{STATISTICS}{}",
        r#"
test "normal" =
    assert (share_ok (-0.5) 0.30853753872598694 100000 61i64u (\g -> Random.pcg64_standard_normal g))
    assert (share_ok 1.0 0.6826894921370859 100000 62i64u (\g -> match Random.pcg64_standard_normal g with
        | (z, next) -> (Math.abs z, next)))
    // |z| > 2 and |z| > 3, as the share of -|z| below -2 and -3.
    assert (share_ok (-2.0) 0.04550026389635843 200000 63i64u (\g -> match Random.pcg64_standard_normal g with
        | (z, next) -> (0.0 - Math.abs z, next)))
    assert (share_ok (-3.0) 0.0026997960632601913 400000 64i64u (\g -> match Random.pcg64_standard_normal g with
        | (z, next) -> (0.0 - Math.abs z, next)))

test "gamma family" =
    assert (share_ok 0.01 0.27924099635901495 100000 65i64u (\g -> Random.pcg64_gamma 0.3 1.0 g))
    assert (share_ok 0.5 0.8138118046743926 100000 66i64u (\g -> Random.pcg64_gamma 0.3 1.0 g))
    assert (share_ok 1.0 0.15085496391539038 100000 67i64u (\g -> Random.pcg64_gamma 2.5 1.0 g))
    assert (share_ok 8.0 0.8437643724222773 100000 68i64u (\g -> Random.pcg64_gamma 2.5 2.0 g))
    assert (share_ok 2.0 0.4275932955291201 100000 69i64u (\g -> Random.pcg64_chisquare 3.0 g))
    // An exponential sample is above its mean with probability 1 / e.
    assert (share_ok 2.0 (1.0 - 0.36787944117144233) 100000 70i64u (\g -> Random.pcg64_exponential 2.0 g))
    assert (share_ok 0.1 0.20483276469913345 100000 71i64u (\g -> Random.pcg64_beta 0.5 0.5 g))
    assert (share_ok 0.2 0.34464 100000 72i64u (\g -> Random.pcg64_beta 2.0 5.0 g))

test "laplace, lognormal, t and F" =
    assert (share_ok (-4.5) 0.18393972058572117 100000 73i64u (\g -> Random.pcg64_laplace (-3.0) 1.5 g))
    assert (share_ok (Math.exp 0.5) 0.5 100000 74i64u (\g -> Random.pcg64_lognormal 0.5 0.75 g))
    assert (share_ok 1.0 0.8044988905221148 100000 75i64u (\g -> Random.pcg64_standard_t 3.0 g))
    assert (share_ok 1.0 0.5981224279835391 100000 76i64u (\g -> Random.pcg64_f_dist 2.0 10.0 g))

test "binomial, poisson and geometric" =
    assert (share_ok 390.5 0.2703196252901869 100000 77i64u (\g -> as_real (Random.pcg64_binomial 1000 0.4 g)))
    assert (share_ok 4.5 0.23750777887760163 100000 78i64u (\g -> as_real (Random.pcg64_binomial 20 0.3 g)))
    assert (share_ok 840.5 0.19922104731174603 100000 79i64u (\g -> as_real (Random.pcg64_binomial 1000 0.85 g)))
    assert (share_ok 240.5 0.2762274601578622 100000 80i64u (\g -> as_real (Random.pcg64_poisson 250.0 g)))
    assert (share_ok 2.5 0.32084719886213414 100000 81i64u (\g -> as_real (Random.pcg64_poisson 3.5 g)))
    assert (share_ok 9.5 0.24239216167051264 100000 82i64u (\g -> as_real (Random.pcg64_poisson 12.0 g)))
    assert (share_ok 3.5 0.488 100000 83i64u (\g -> as_real (Random.pcg64_geometric 0.2 g)))
"#
    );
    run_tz_test(&source);
}

#[test]
fn samples_stay_in_the_range_of_their_distribution() {
    run_tz_test(
        r#"
test "bounds" =
    let mut state = Random.pcg64 91i64u
    for _ in 0 ..< 20000 do
        let (unit, a) = Random.pcg64_next_f64 state
        assert (unit >= 0.0 && unit < 1.0)
        let (uniform, b) = Random.pcg64_uniform 5.0 5.5 a
        assert (uniform >= 5.0 && uniform < 5.5)
        let (waiting, c) = Random.pcg64_exponential 3.0 b
        assert (waiting >= 0.0 && Math.is_finite waiting)
        let (shape, d) = Random.pcg64_gamma 0.2 1.0 c
        assert (shape >= 0.0 && Math.is_finite shape)
        let (share, e) = Random.pcg64_beta 0.3 0.4 d
        assert (share >= 0.0 && share <= 1.0)
        let (successes, f) = Random.pcg64_binomial 50 0.9 e
        assert (successes >= 0 && successes <= 50)
        let (events, g) = Random.pcg64_poisson 40.0 f
        assert (events >= 0)
        let (trials, h) = Random.pcg64_geometric 0.05 g
        assert (trials >= 1)
        let (integer, i) = Random.pcg64_next_i64_range (-9223372036854775807 - 1) 9223372036854775807 h
        assert (integer < 9223372036854775807)
        let (normal, j) = Random.pcg64_standard_normal i
        assert (Math.is_finite normal)
        let (positive, k) = Random.pcg64_lognormal 0.0 1.0 j
        assert (positive > 0.0)
        state = k

test "degenerate parameters" =
    let rng = Random.pcg64 5i64u
    let (a, _) = Random.pcg64_normal 7.5 0.0 rng
    let (b, _) = Random.pcg64_exponential 0.0 rng
    let (c, _) = Random.pcg64_gamma 3.0 0.0 rng
    let (d, _) = Random.pcg64_laplace 2.0 0.0 rng
    let (e, _) = Random.pcg64_lognormal 0.0 0.0 rng
    let (f, _) = Random.pcg64_binomial 0 0.5 rng
    let (g, _) = Random.pcg64_binomial 30 0.0 rng
    let (h, _) = Random.pcg64_binomial 30 1.0 rng
    let (i, _) = Random.pcg64_poisson 0.0 rng
    let (j, _) = Random.pcg64_geometric 1.0 rng
    let (k, _) = Random.pcg64_next_i64_range 4 5 rng
    assert (a == 7.5 && b == 0.0 && c == 0.0 && d == 2.0 && e == 1.0)
    assert (f == 0 && g == 0 && h == 30 && i == 0 && j == 1 && k == 4)
    // A geometric number that does not fit `i64` is the largest `i64`.
    let (huge, _) = Random.pcg64_geometric 1e-300 rng
    assert (huge == 9223372036854775807)
"#,
    );
}

/// The seeded functions of `std/Random.tz` are declared `pure`, a `pure def` may call them, and a program that
/// only uses them touches no capability (the `random` capability is the operating system's entropy).
#[test]
fn seeded_functions_are_pure_and_need_no_capability() {
    let random = parser::parse(include_str!("../std/Random.tz")).unwrap();
    let mut checked = 0;
    for function in &random.functions {
        let name = function.name.text.as_str();
        if function.visibility != Visibility::Public
            || !(name.starts_with("pcg64") || name.starts_with("seed_sequence"))
        {
            continue;
        }
        assert_eq!(function.pure, name != "pcg64_from_os", "{name}");
        checked += 1;
    }
    assert_eq!(checked, 23);

    let root = scratch_dir("pure");
    write_project(
        &root,
        r#"pure def sample :: own i64u -> f64
fn sample seed =
    let (children, _) = Random.seed_sequence_spawn 2 (Random.seed_sequence [seed])
    let (z, a) = Random.pcg64_standard_normal (Random.pcg64_from_seed (ref children[1]))
    let (g, b) = Random.pcg64_gamma 0.5 2.0 a
    let (n, c) = Random.pcg64_binomial 1000 0.4 b
    let (k, d) = Random.pcg64_poisson 250.0 c
    let (r, _) = Random.pcg64_next_i64_range 0 10 d
    z + g + ((n + k + r) as f64)

def main :: unit -> i32 = \() -> if sample 1i64u == sample 1i64u then 0i32 else 1i32
"#,
    );
    let project = Project::load(&root).unwrap();
    let module = project
        .analyze_cached(false, AnalysisKind::Program)
        .unwrap_or_else(|errors| {
            panic!(
                "{}",
                errors
                    .first()
                    .map_or(String::new(), |error| error.message.clone())
            )
        });
    let analysis = capabilities::analyze(&module, &project.source_infos()).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let root_sites: Vec<_> = analysis
        .sites
        .iter()
        .filter(|(package, _)| package.is_none())
        .map(|(_, site)| site.callee.clone())
        .collect();
    assert!(root_sites.is_empty(), "{root_sites:?}");
}

const WASM_CALL: &str = "const fs = require('node:fs');\n\
    WebAssembly.instantiate(fs.readFileSync(process.argv[1]), {}).then(({ instance }) => {\n\
      console.log(String(instance.exports.tz_checksum(0n)));\n\
    }).catch((error) => { console.error(error); process.exit(1); });";

/// Builds `source` (which exports `checksum :: own i64 -> i64` and ends with `checksum 0`) natively and for
/// wasm32 at `-O0` and `-O3`, and returns the four results.
fn four_results(source: &str) -> Vec<String> {
    let mut results = Vec::new();
    for opt in ["-O0", "-O3"] {
        let root = scratch_dir("bits");
        write_project(&root, source);
        let native = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["run", opt, root.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            native.status.success(),
            "native {opt} failed:\n{}",
            String::from_utf8_lossy(&native.stderr)
        );
        results.push(String::from_utf8_lossy(&native.stdout).trim().to_owned());
        let wasm_path = root.join("out.wasm");
        let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                opt,
                "--target",
                "wasm32-unknown-unknown",
                root.to_str().unwrap(),
                "-o",
                wasm_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "wasm build {opt} failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let run = Command::new("node")
            .args(["-e", WASM_CALL, wasm_path.to_str().unwrap()])
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            run.status.success(),
            "wasm run {opt} failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
        results.push(String::from_utf8_lossy(&run.stdout).trim().to_owned());
    }
    results
}

/// `fold` mixes the exact value of a finite `f64` (its fraction and exponent from `Math.frexp`) into a number.
const FOLD: &str = r#"
def fold :: own i64 -> own f64 -> i64
fn fold total value =
    let (fraction, exponent) = Math.frexp value
    let mantissa = (fraction * 9007199254740992.0) as i64
    Int.wrapping_add (Int.wrapping_mul (Int.wrapping_add (Int.wrapping_mul total 1000003) mantissa) 31) (exponent as i64)
"#;

#[test]
fn seeded_scalar_samples_are_bit_identical_on_native_and_wasm_at_o0_and_o3() {
    let source = format!(
        "{FOLD}{}",
        r#"
export def checksum :: own i64 -> i64
fn checksum start =
    let mut state = Random.pcg64 20261128i64u
    let mut total = start
    for round in 0 ..< 400 do
        let shape = 0.05 + (round as f64) * 0.11
        let (a, s1) = Random.pcg64_next_f64 state
        let (b, s2) = Random.pcg64_uniform (-2.5) 7.25 s1
        let (c, s3) = Random.pcg64_standard_normal s2
        let (d, s4) = Random.pcg64_normal 1.5 0.75 s3
        let (e, s5) = Random.pcg64_exponential 0.3 s4
        let (f, s6) = Random.pcg64_gamma shape 1.7 s5
        let (g, s7) = Random.pcg64_beta shape 0.6 s6
        let (h, s8) = Random.pcg64_beta 0.4 0.9 s7
        let (i, s9) = Random.pcg64_chisquare shape s8
        let (j, s10) = Random.pcg64_laplace 0.25 2.0 s9
        let (k, s11) = Random.pcg64_lognormal 0.1 0.9 s10
        let (l, s12) = Random.pcg64_standard_t (shape + 0.5) s11
        let (m, s13) = Random.pcg64_f_dist (shape + 1.0) 6.0 s12
        let (n, s14) = Random.pcg64_binomial (round * 7 + 1) 0.37 s13
        let (o, s15) = Random.pcg64_poisson (shape * 9.0) s14
        let (p, s16) = Random.pcg64_geometric 0.013 s15
        let (q, s17) = Random.pcg64_next_i64_range (-1000) 1000 s16
        let (r, s18) = Random.pcg64_bernoulli 0.5 s17
        state = s18
        total = fold (fold (fold (fold (fold (fold total a) b) c) d) e) f
        total = fold (fold (fold (fold (fold (fold (fold total g) h) i) j) k) l) m
        total = Int.wrapping_add (Int.wrapping_mul total 31) (n + o * 3 + p * 5 + q * 7 + (if r then 11 else 13))
    let (bits, _) = Random.pcg64_next_u64 state
    Int.wrapping_add total (bits as i64)

checksum 0
"#
    );
    let results = four_results(&source);
    assert!(
        results.iter().all(|result| result == &results[0]) && !results[0].is_empty(),
        "native -O0, wasm -O0, native -O3, wasm -O3: {results:?}"
    );
}

// ---- Phase 2: the opt-in `RandomTensor` ----

#[test]
fn random_tensor_generates_the_example_of_the_ticket() {
    run_tz_test(
        r#"
test "a tensor of normal numbers from a seeded generator" =
    let rng = Random.pcg64 42i64u
    let (samples, rng) = RandomTensor.normal 0.0 1.0 [100, 10] rng
    let view = Tensor.view (ref samples)
    assert (Array.copy (Tensor.shape (ref view)) == [100, 10])
    // The tensor is 1000 scalar draws, and the generator continues after them.
    let mut state = Random.pcg64 42i64u
    for index in 0 ..< 1000 do
        let (z, next) = Random.pcg64_normal 0.0 1.0 state
        state = next
        assert ((Tensor.as_array (ref samples))[index] == z)
    let (after_tensor, _) = Random.pcg64_next_u64 rng
    let (after_scalars, _) = Random.pcg64_next_u64 state
    assert (after_tensor == after_scalars)
    // Two halves are one whole.
    let (top, middle) = RandomTensor.normal 0.0 1.0 [50, 10] (Random.pcg64 42i64u)
    let (bottom, _) = RandomTensor.normal 0.0 1.0 [50, 10] middle
    assert (Array.append (Tensor.as_array (ref top)) (Tensor.as_array (ref bottom)) == Tensor.to_array samples)
"#,
    );
}

#[test]
fn dirichlet_and_multivariate_normal_have_the_moments_of_their_formulas() {
    run_tz_test(
        r#"
// The sample mean and variance of column `column` of `[rows, k]` values against the moments of its distribution.
def column_ok :: own i64 -> own i64 -> own f64 -> own f64 -> own f64 -> ref [f64] -> bool
fn column_ok k column mu variance kurtosis values =
    let rows = values.length / k
    let mut first = 0.0
    let mut second = 0.0
    for row in 0 ..< rows do
        let centered = values[row * k + column] - mu
        first = first + centered
        second = second + centered * centered
    let n = rows as f64
    let spread = (second - first * first / n) / (n - 1.0)
    Math.abs (first / n) <= 4.0 * Math.sqrt (variance / n) && Math.abs (spread - variance) <= 4.0 * variance * Math.sqrt ((kurtosis + 2.0) / n)

def rows_sum_to_one :: own i64 -> ref [f64] -> bool
fn rows_sum_to_one k values =
    let mut ok = true
    for row in 0 ..< values.length / k do
        let mut total = 0.0
        for column in 0 ..< k do
            if !(values[row * k + column] >= 0.0) then ok = false
            total = total + values[row * k + column]
        if Math.abs (total - 1.0) > 1e-12 then ok = false
    ok

test "dirichlet by gamma numbers" =
    // Component i is Beta(a_i, a_0 - a_i): mean a_i / a_0, variance a_i (a_0 - a_i) / (a_0^2 (a_0 + 1)).
    let alpha = Tensor.of_array [3] [1.0, 2.0, 3.0]
    let (drawn, _) = RandomTensor.dirichlet (Tensor.view alpha) [200, 200] (Random.pcg64 101i64u)
    let view = Tensor.view (ref drawn)
    assert (Array.copy (Tensor.shape (ref view)) == [200, 200, 3])
    let values = Tensor.as_array (ref drawn)
    assert (rows_sum_to_one 3 values)
    assert (column_ok 3 0 (1.0 / 6.0) 0.01984126984126984 1.2 values)
    assert (column_ok 3 1 (1.0 / 3.0) 0.031746031746031744 (-0.375) values)
    assert (column_ok 3 2 0.5 0.03571428571428571 (-0.6666666666666666) values)

test "dirichlet by beta numbers when every parameter is small" =
    let alpha = Tensor.of_array [3] [0.05, 0.02, 0.03]
    let (drawn, _) = RandomTensor.dirichlet (Tensor.view alpha) [40000] (Random.pcg64 102i64u)
    let values = Tensor.as_array (ref drawn)
    assert (rows_sum_to_one 3 values)
    assert (column_ok 3 0 0.5 0.22727272727272727 (-1.9354838709677418) values)
    assert (column_ok 3 1 0.2 0.14545454545454542 0.3456221198156681 values)
    assert (column_ok 3 2 0.3 0.1909090909090909 (-1.1630458635066927) values)

test "multivariate normal" =
    // Sample means within 4 sqrt(s_ii / n), and sample covariances within 4 sqrt((s_ii s_jj + s_ij^2) / n).
    let mean = Tensor.of_array [2] [1.0, -1.0]
    let cov = Tensor.of_array [2, 2] [2.0, 0.5, 0.5, 1.0]
    match RandomTensor.multivariate_normal (Tensor.view mean) (Tensor.view cov) [40000] (Random.pcg64 103i64u) with
    | Result.Error _ -> assert false
    | Result.Ok (drawn, _) ->
        let values = Tensor.as_array (ref drawn)
        assert (values.length == 80000)
        let n = 40000.0
        let mut sum_x = 0.0
        let mut sum_y = 0.0
        let mut sum_xx = 0.0
        let mut sum_yy = 0.0
        let mut sum_xy = 0.0
        for row in 0 ..< 40000 do
            let x = values[row * 2] - 1.0
            let y = values[row * 2 + 1] + 1.0
            sum_x = sum_x + x
            sum_y = sum_y + y
            sum_xx = sum_xx + x * x
            sum_yy = sum_yy + y * y
            sum_xy = sum_xy + x * y
        assert (Math.abs (sum_x / n) <= 4.0 * Math.sqrt (2.0 / n))
        assert (Math.abs (sum_y / n) <= 4.0 * Math.sqrt (1.0 / n))
        let var_x = (sum_xx - sum_x * sum_x / n) / (n - 1.0)
        let var_y = (sum_yy - sum_y * sum_y / n) / (n - 1.0)
        let cov_xy = (sum_xy - sum_x * sum_y / n) / (n - 1.0)
        assert (Math.abs (var_x - 2.0) <= 4.0 * Math.sqrt ((2.0 * 2.0 + 2.0 * 2.0) / n))
        assert (Math.abs (var_y - 1.0) <= 4.0 * Math.sqrt ((1.0 * 1.0 + 1.0 * 1.0) / n))
        assert (Math.abs (cov_xy - 0.5) <= 4.0 * Math.sqrt ((2.0 * 1.0 + 0.5 * 0.5) / n))

test "multivariate normal reports shapes and covariances that do not fit" =
    let two = Tensor.of_array [2] [0.0, 0.0]
    let three = Tensor.of_array [3] [0.0, 0.0, 0.0]
    let identity = Tensor.of_array [2, 2] [1.0, 0.0, 0.0, 1.0]
    let indefinite = Tensor.of_array [2, 2] [1.0, 2.0, 2.0, 1.0]
    match RandomTensor.multivariate_normal (Tensor.view three) (Tensor.view identity) [4] (Random.pcg64 1i64u) with
    | Result.Error (Linalg.Error.ShapeMismatch (left, right)) -> assert (left == [3] && right == [2, 2])
    | _ -> assert false
    match RandomTensor.multivariate_normal (Tensor.view two) (Tensor.view indefinite) [4] (Random.pcg64 1i64u) with
    | Result.Error Linalg.Error.NotPositiveDefinite -> assert true
    | _ -> assert false
    // An identity covariance gives the standard normal numbers themselves, shifted by the mean.
    match RandomTensor.multivariate_normal (Tensor.view two) (Tensor.view identity) [3] (Random.pcg64 1i64u) with
    | Result.Ok (drawn, _) ->
        let (plain, _) = RandomTensor.standard_normal [3, 2] (Random.pcg64 1i64u)
        assert (Tensor.to_array drawn == Tensor.to_array plain)
    | Result.Error _ -> assert false
"#,
    );
}

#[test]
fn permutations_shuffles_and_weighted_choices_are_uniform_or_weighted() {
    run_tz_test(
        r#"
// The chi-square statistic of counts against equal expectations.
def chi_square :: own f64 -> ref [i64] -> f64
fn chi_square expected seen =
    let mut statistic = 0.0
    for index in 0 ..< seen.length do
        let difference = (seen[index] as f64) - expected
        statistic = statistic + difference * difference / expected
    statistic

test "every order of four elements is equally likely" =
    // 24 orders, 48000 draws: chi-square with 23 degrees of freedom, 49.73 is the 0.999 quantile.
    let mut state = Random.pcg64 201i64u
    let mut slots = Array.create 64 0
    for _ in 0 ..< 48000 do
        let (order, next) = RandomTensor.permutation 4 state
        state = next
        let values = Tensor.as_array (ref order)
        let slot = values[0] * 16 + values[1] * 4 + values[2]
        slots[slot] = slots[slot] + 1
    let used = Array.filter (\count -> count > 0) (ref slots)
    assert (used.length == 24)
    assert (chi_square 2000.0 (ref used) < 49.73)

test "every order of three elements is equally likely" =
    // 6 orders, 30000 draws: chi-square with 5 degrees of freedom, 20.52 is the 0.999 quantile.
    let mut state = Random.pcg64 202i64u
    let mut slots = Array.create 9 0
    for _ in 0 ..< 30000 do
        let (order, next) = RandomTensor.permutation 3 state
        state = next
        let values = Tensor.as_array (ref order)
        let slot = values[0] * 3 + values[1]
        slots[slot] = slots[slot] + 1
    let used = Array.filter (\count -> count > 0) (ref slots)
    assert (used.length == 6)
    assert (chi_square 5000.0 (ref used) < 20.52)

test "shuffle_axis moves whole slices in the order of permutation" =
    let mut data = Array.init 12 (\index -> index)
    let mut window = Tensor.borrow_mut [4, 3] (ref mut data)
    let after_shuffle = RandomTensor.shuffle_axis 0 (Random.pcg64 9i64u) (ref mut window)
    let (order, after_permutation) = RandomTensor.permutation 4 (Random.pcg64 9i64u)
    let positions = Tensor.to_array order
    for row in 0 ..< 4 do
        for col in 0 ..< 3 do assert (data[row * 3 + col] == positions[row] * 3 + col)
    let (left, _) = Random.pcg64_next_u64 after_shuffle
    let (right, _) = Random.pcg64_next_u64 after_permutation
    assert (left == right)
    // Along the other axis the columns move and each row keeps its elements.
    let mut wide = Array.init 12 (\index -> index)
    let mut columns = Tensor.borrow_mut [3, 4] (ref mut wide)
    let _next = RandomTensor.shuffle_axis 1 (Random.pcg64 9i64u) (ref mut columns)
    for row in 0 ..< 3 do
        for col in 0 ..< 4 do assert (wide[row * 4 + col] == row * 4 + positions[col])

test "weighted choice with replacement follows the weights" =
    // Weights 1 : 2 : 3 : 4 that do not sum to 1. Chi-square with 3 degrees of freedom: 16.27 is the 0.999 quantile.
    let values = Tensor.of_array [4] [0, 1, 2, 3]
    let weights = Tensor.of_array [4] [0.5, 1.0, 1.5, 2.0]
    let (picked, _) = RandomTensor.choice_weighted 40000 true (Tensor.view weights) (Random.pcg64 203i64u) (Tensor.view values)
    let drawn = Tensor.as_array (ref picked)
    let mut seen = [0, 0, 0, 0]
    for index in 0 ..< drawn.length do seen[drawn[index]] = seen[drawn[index]] + 1
    let mut statistic = 0.0
    for slot in 0 ..< 4 do
        let expected = 40000.0 * ((slot + 1) as f64) / 10.0
        let difference = (seen[slot] as f64) - expected
        statistic = statistic + difference * difference / expected
    assert (statistic < 16.27)

test "weighted choice without replacement draws from what is left" =
    // P(first = i, second = j) = p_i p_j / (1 - p_i) for the 12 ordered pairs: chi-square with 11 degrees of freedom,
    // 31.26 is the 0.999 quantile.
    let values = Tensor.of_array [4] [0, 1, 2, 3]
    let weights = Tensor.of_array [4] [1.0, 2.0, 3.0, 4.0]
    let mut state = Random.pcg64 204i64u
    let mut seen = Array.create 16 0
    for _ in 0 ..< 40000 do
        let (picked, next) = RandomTensor.choice_weighted 2 false (Tensor.view weights) state (Tensor.view values)
        state = next
        let pair = Tensor.as_array (ref picked)
        assert (pair[0] != pair[1])
        seen[pair[0] * 4 + pair[1]] = seen[pair[0] * 4 + pair[1]] + 1
    let mut statistic = 0.0
    for first in 0 ..< 4 do
        for second in 0 ..< 4 do
            if first != second then
                let p = ((first + 1) as f64) / 10.0
                let q = ((second + 1) as f64) / 10.0
                let expected = 40000.0 * p * q / (1.0 - p)
                let difference = (seen[first * 4 + second] as f64) - expected
                statistic = statistic + difference * difference / expected
    assert (statistic < 31.26)
    // All slices without replacement are a reordering, and slices of weight 0 never come.
    let sparse = Tensor.of_array [4] [0.0, 2.0, 0.0, 1.0]
    let (both, _) = RandomTensor.choice_weighted 2 false (Tensor.view sparse) (Random.pcg64 205i64u) (Tensor.view values)
    assert (Array.sort (Tensor.to_array both) == [1, 3])

test "weighted choice takes whole slices along the first axis" =
    let table = Tensor.of_array [3, 2] [10, 11, 20, 21, 30, 31]
    let weights = Tensor.of_array [3] [0.0, 0.0, 7.0]
    let (picked, _) = RandomTensor.choice_weighted 2 true (Tensor.view weights) (Random.pcg64 206i64u) (Tensor.view table)
    let view = Tensor.view (ref picked)
    assert (Array.copy (Tensor.shape (ref view)) == [2, 2])
    assert (Tensor.to_array picked == [30, 31, 30, 31])
"#,
    );
}

/// `Random` stays free of the opt-in modules: its source names none of them, a program that names `Random` alone
/// emits no `Tensor`, `Linalg` or `RandomTensor` code, and `RandomTensor` is emitted only when it is named.
#[test]
fn random_tensor_is_opt_in_and_random_does_not_reach_it() {
    let random = include_str!("../std/Random.tz");
    for name in ["Tensor", "Linalg", "Complex", "Matrix"] {
        assert!(!random.contains(name), "std/Random.tz names {name}");
    }
    let emit = |source: &str| -> String {
        let root = scratch_dir("optin");
        write_project(&root, source);
        let ir_path = root.join("out.ll");
        let build = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                "--emit",
                "llvm",
                root.to_str().unwrap(),
                "-o",
                ir_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let ir = std::fs::read_to_string(&ir_path).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        ir
    };
    let scalar = emit(
        "let (z, next) = Random.pcg64_standard_normal (Random.pcg64 1i64u)\nlet (n, _) = Random.pcg64_binomial 100 0.5 next\nz + (n as f64)\n",
    );
    assert!(scalar.contains("@tz.fn.Random.pcg64_standard_normal"));
    for module in ["Tensor", "Linalg", "RandomTensor", "Complex"] {
        assert!(
            !scalar.contains(&format!("@tz.fn.{module}.")),
            "a program that names only Random emits {module}"
        );
    }
    let tensor = emit(
        "let t = Tensor.of_array [2] [1, 2]\nlet tv = Tensor.view (ref t)\nTensor.sum (ref tv)\n",
    );
    assert!(!tensor.contains("@tz.fn.RandomTensor."));
    assert!(!tensor.contains("@tz.fn.Linalg."));
    let named = emit(
        "let (t, _) = RandomTensor.normal 0.0 1.0 [2, 2] (Random.pcg64 1i64u)\nTensor.to_array t\n",
    );
    assert!(named.contains("@tz.fn.RandomTensor.normal"));
}

#[test]
fn seeded_tensors_are_bit_identical_on_native_and_wasm_at_o0_and_o3() {
    let source = format!(
        "{FOLD}{}",
        r#"
def fold_all :: own i64 -> ref [f64] -> i64
fn fold_all start values =
    let mut total = start
    for index in 0 ..< values.length do total = fold total values[index]
    total

def mix_all :: own i64 -> ref [i64] -> i64
fn mix_all start values =
    let mut total = start
    for index in 0 ..< values.length do total = Int.wrapping_add (Int.wrapping_mul total 31) values[index]
    total

export def checksum :: own i64 -> i64
fn checksum start =
    let rng = Random.pcg64 20261128i64u
    let (a, r1) = RandomTensor.uniform (-3.0) 9.0 [7, 5] rng
    let (b, r2) = RandomTensor.standard_normal [4, 3, 2] r1
    let (c, r3) = RandomTensor.normal 2.0 0.5 [33] r2
    let (d, r4) = RandomTensor.exponential 1.25 [33] r3
    let (e, r5) = RandomTensor.gamma 0.35 2.0 [33] r4
    let (f, r6) = RandomTensor.gamma 7.5 0.1 [33] r5
    let (g, r7) = RandomTensor.beta 0.6 0.3 [33] r6
    let (h, r8) = RandomTensor.beta 4.0 1.5 [33] r7
    let (i, r9) = RandomTensor.chisquare 2.5 [33] r8
    let (j, r10) = RandomTensor.laplace 0.0 1.0 [33] r9
    let (k, r11) = RandomTensor.lognormal 0.25 0.5 [33] r10
    let (l, r12) = RandomTensor.standard_t 3.5 [33] r11
    let (m, r13) = RandomTensor.f_dist 3.0 7.0 [33] r12
    let (n, r14) = RandomTensor.integers (-50) 50 [33] r13
    let (o, r15) = RandomTensor.binomial 500 0.3 [33] r14
    let (p, r16) = RandomTensor.poisson 42.0 [33] r15
    let (q, r17) = RandomTensor.geometric 0.07 [33] r16
    let (r, r18) = RandomTensor.permutation 33 r17
    let alpha = Tensor.of_array [4] [0.7, 1.3, 2.9, 0.2]
    let (s, r19) = RandomTensor.dirichlet (Tensor.view alpha) [11] r18
    let small = Tensor.of_array [3] [0.03, 0.06, 0.01]
    let (t, r20) = RandomTensor.dirichlet (Tensor.view small) [11] r19
    let mean = Tensor.of_array [3] [0.5, -2.0, 4.0]
    let cov = Tensor.of_array [3, 3] [4.0, 2.0, 0.6, 2.0, 2.0, 0.5, 0.6, 0.5, 3.0]
    let mut total = start
    total = fold_all total (Tensor.as_array (ref a))
    total = fold_all total (Tensor.as_array (ref b))
    total = fold_all total (Tensor.as_array (ref c))
    total = fold_all total (Tensor.as_array (ref d))
    total = fold_all total (Tensor.as_array (ref e))
    total = fold_all total (Tensor.as_array (ref f))
    total = fold_all total (Tensor.as_array (ref g))
    total = fold_all total (Tensor.as_array (ref h))
    total = fold_all total (Tensor.as_array (ref i))
    total = fold_all total (Tensor.as_array (ref j))
    total = fold_all total (Tensor.as_array (ref k))
    total = fold_all total (Tensor.as_array (ref l))
    total = fold_all total (Tensor.as_array (ref m))
    total = mix_all total (Tensor.as_array (ref n))
    total = mix_all total (Tensor.as_array (ref o))
    total = mix_all total (Tensor.as_array (ref p))
    total = mix_all total (Tensor.as_array (ref q))
    total = mix_all total (Tensor.as_array (ref r))
    total = fold_all total (Tensor.as_array (ref s))
    total = fold_all total (Tensor.as_array (ref t))
    match RandomTensor.multivariate_normal (Tensor.view mean) (Tensor.view cov) [11] r20 with
    | Result.Error _ -> -1
    | Result.Ok (u, r21) ->
        total = fold_all total (Tensor.as_array (ref u))
        let weights = Tensor.of_array [33] (Tensor.to_array d)
        let (v, r22) = RandomTensor.choice_weighted 20 true (Tensor.view weights) r21 (Tensor.view n)
        let (w, r23) = RandomTensor.choice_weighted 20 false (Tensor.view weights) r22 (Tensor.view n)
        total = mix_all (mix_all total (Tensor.as_array (ref v))) (Tensor.as_array (ref w))
        let mut cells = Tensor.to_array o
        let mut window = Tensor.borrow_mut [11, 3] (ref mut cells)
        let r24 = RandomTensor.shuffle_axis 0 r23 (ref mut window)
        total = mix_all total (ref cells)
        let (bits, _) = Random.pcg64_next_u64 r24
        Int.wrapping_add total (bits as i64)

checksum 0
"#
    );
    let results = four_results(&source);
    assert!(
        results.iter().all(|result| result == &results[0])
            && !results[0].is_empty()
            && results[0] != "-1",
        "native -O0, wasm -O0, native -O3, wasm -O3: {results:?}"
    );
}

/// Every generator of `std/RandomTensor.tz` is declared `pure`; `shuffle_axis` writes through its window and is not.
#[test]
fn random_tensor_generators_are_pure() {
    let module = parser::parse(include_str!("../std/RandomTensor.tz")).unwrap();
    let mut public = 0;
    for function in &module.functions {
        if function.visibility != Visibility::Public {
            continue;
        }
        assert_eq!(
            function.pure,
            function.name.text != "shuffle_axis",
            "{}",
            function.name.text
        );
        public += 1;
    }
    assert_eq!(public, 21);
}
