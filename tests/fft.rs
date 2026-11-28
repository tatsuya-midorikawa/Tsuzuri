//! NC09: `Fft` against independent references. Every test writes a project of one `Main.tz` (edition 2028) and
//! runs `tsuzuri test` on it. The expectations are the definition (an `O(n^2)` sum written in the test) and the
//! values that NumPy 2.5.1 gives; `python3 tests/fft_polynomial_reference.py fft` prints the block between
//! `reference:begin` and `reference:end`.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/fft")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn run_tz_test(body: &str) {
    let root = scratch_dir("suite");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Main.tz"),
        format!("{PRELUDE}\n{REFERENCE}\n{body}"),
    )
    .unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"fft-suite\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
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

const PRELUDE: &str = r#"using std

// The O(n^2) definition, with the angle reduced by (j k) mod n so that it stays accurate for a large n.
def naive_dft :: own bool -> ref [Complex<f64>] -> [Complex<f64>]
fn naive_dft inverse xs =
    let n = xs.length
    let sign = if inverse then 1.0 else -1.0
    new [Complex<f64>](n, \k ->
        let mut re = 0.0
        let mut im = 0.0
        for j in 0i64 ..< n do
            let angle = sign * 2.0 * Math.pi() * (((j * k) % n) as f64) / (n as f64)
            let c = Math.cos angle
            let s = Math.sin angle
            re = re + xs[j].re * c - xs[j].im * s
            im = im + xs[j].re * s + xs[j].im * c
        if inverse then Complex.of (re / (n as f64)) (im / (n as f64)) else Complex.of re im)

def wave :: own i64 -> [Complex<f64>]
fn wave n = new [Complex<f64>](n, \i -> Complex.of (Math.sin ((i * i + 1) as f64)) (Math.cos ((3 * i + 2) as f64)))

def wave_real :: own i64 -> [f64]
fn wave_real n = new [f64](n, \i -> Math.sin ((i * i + 1) as f64) + 0.25 * (i as f64))

def gap :: ref [Complex<f64>] -> ref [Complex<f64>] -> f64
fn gap left right =
    assert (left.length == right.length)
    let mut worst = 0.0
    for i in 0i64 ..< left.length do
        let d = Math.hypot (left[i].re - right[i].re) (left[i].im - right[i].im)
        if !(d <= worst) then worst = d
    worst

def gap_real :: ref [f64] -> ref [f64] -> f64
fn gap_real left right =
    assert (left.length == right.length)
    let mut worst = 0.0
    for i in 0i64 ..< left.length do
        let d = Math.abs (left[i] - right[i])
        if !(d <= worst) then worst = d
    worst

def complex_of :: Result<Tensor<Complex<f64>>, Fft.Error> -> [Complex<f64>]
fn complex_of result =
    match result with
    | Result.Ok tensor -> Array.copy (Tensor.as_array tensor)
    | Result.Error _ ->
        assert false
        let empty: [Complex<f64>] = []
        empty

def real_of :: Result<Tensor<f64>, Fft.Error> -> [f64]
fn real_of result =
    match result with
    | Result.Ok tensor -> Array.copy (Tensor.as_array tensor)
    | Result.Error _ ->
        assert false
        let empty: [f64] = []
        empty

def shape_of :: Result<Tensor<Complex<f64>>, Fft.Error> -> [i64]
fn shape_of result =
    match result with
    | Result.Ok tensor ->
        let view = Tensor.view tensor
        Array.copy (Tensor.shape view)
    | Result.Error _ ->
        assert false
        let empty: [i64] = []
        empty

def fails_with :: own Fft.Error -> Result<Tensor<Complex<f64>>, Fft.Error> -> bool
fn fails_with expected result =
    match result with
    | Result.Ok _ -> false
    | Result.Error problem -> problem == expected

def real_fails_with :: own Fft.Error -> Result<Tensor<f64>, Fft.Error> -> bool
fn real_fails_with expected result =
    match result with
    | Result.Ok _ -> false
    | Result.Error problem -> problem == expected

def same_bits :: ref [Complex<f64>] -> ref [Complex<f64>] -> bool
fn same_bits left right =
    if left.length != right.length then false
    else
        let mut same = true
        for i in 0i64 ..< left.length do
            if !(left[i].re == right[i].re && left[i].im == right[i].im) then same = false
            if Math.copysign 1.0 left[i].re != Math.copysign 1.0 right[i].re || Math.copysign 1.0 left[i].im != Math.copysign 1.0 right[i].im then same = false
        same
"#;

const REFERENCE: &str = r#"// reference:begin
def x6 :: unit -> [f64]
fn x6 _ = [(0.5), (-1.25), (2.0), (3.5), (-0.75), (1.0)]

def x7 :: unit -> [f64]
fn x7 _ = [(0.5), (-1.25), (2.0), (3.5), (-0.75), (1.0), (4.25)]

def h4 :: unit -> [Complex<f64>]
fn h4 _ = [Complex.of (1.0) (0.0), Complex.of (2.0) (-1.0), Complex.of (0.5) (3.0), Complex.of (-1.5) (0.25)]

def z5 :: unit -> [Complex<f64>]
fn z5 _ = [Complex.of (1.0) (2.0), Complex.of (-0.5) (0.25), Complex.of (3.0) (-1.0), Complex.of (0.75) (0.5), Complex.of (-2.0) (-2.0)]

def m34 :: unit -> [f64]
fn m34 _ = [(-7.0), (-6.0), (-4.17157287525381), (-1.803847577293368), (1.0), (4.180339887498949), (7.696938456699069), (11.520259177452136), (15.627416997969522), (20.0), (24.622776601683793), (29.4828726939094)]

def np_fft_z5 :: unit -> [Complex<f64>]
fn np_fft_z5 _ = [Complex.of (2.25) (-0.25), Complex.of (-1.5481569316180357) (-0.8853728350694794), Complex.of (6.930457806944216) (4.51947052619407), Complex.of (1.4322546227426267) (2.003071959743299), Complex.of (-4.064555498068807) (4.6128303491321105)]

def np_fft_z5_ortho :: unit -> [Complex<f64>]
fn np_fft_z5_ortho _ = [Complex.of (1.0062305898749053) (-0.11180339887498948), Complex.of (-0.6923568277870843) (-0.3959507689294131), Complex.of (3.0993949543042763) (2.0211686637753368), Complex.of (0.6405237395081659) (0.8958010131619477), Complex.of (-1.8177244784004736) (2.0629204458666974)]

def np_fft_z5_forward :: unit -> [Complex<f64>]
fn np_fft_z5_forward _ = [Complex.of (0.45) (-0.05), Complex.of (-0.30963138632360715) (-0.1770745670138959), Complex.of (1.3860915613888434) (0.903894105238814), Complex.of (0.28645092454852533) (0.4006143919486598), Complex.of (-0.8129110996137614) (0.9225660698264222)]

def np_ifft_z5 :: unit -> [Complex<f64>]
fn np_ifft_z5 _ = [Complex.of (0.45) (-0.05), Complex.of (-0.8129110996137614) (0.9225660698264222), Complex.of (0.28645092454852533) (0.4006143919486598), Complex.of (1.3860915613888434) (0.903894105238814), Complex.of (-0.30963138632360715) (-0.1770745670138959)]

def np_ifft_z5_ortho :: unit -> [Complex<f64>]
fn np_ifft_z5_ortho _ = [Complex.of (1.0062305898749053) (-0.11180339887498948), Complex.of (-1.8177244784004736) (2.0629204458666974), Complex.of (0.6405237395081659) (0.8958010131619477), Complex.of (3.0993949543042763) (2.0211686637753368), Complex.of (-0.6923568277870843) (-0.3959507689294131)]

def np_ifft_z5_forward :: unit -> [Complex<f64>]
fn np_ifft_z5_forward _ = [Complex.of (2.25) (-0.25), Complex.of (-4.064555498068807) (4.6128303491321105), Complex.of (1.4322546227426267) (2.003071959743299), Complex.of (6.930457806944216) (4.51947052619407), Complex.of (-1.5481569316180357) (-0.8853728350694794)]

def np_fft_z5_n8 :: unit -> [Complex<f64>]
fn np_fft_z5_n8 _ = [Complex.of (2.25) (-0.25), Complex.of (1.6464466094067263) (0.6464466094067263), Complex.of (-4.25) (2.25), Complex.of (5.414213562373095) (7.0), Complex.of (1.75) (-1.75), Complex.of (2.353553390593274) (1.3535533905932737), Complex.of (-3.75) (-0.25), Complex.of (2.585786437626905) (7.0)]

def np_fft_z5_n3 :: unit -> [Complex<f64>]
fn np_fft_z5_n3 _ = [Complex.of (3.5) (1.25), Complex.of (0.8325317547305482) (5.406088913245535), Complex.of (-1.3325317547305482) (-0.6560889132455352)]

def np_rfft_x6 :: unit -> [Complex<f64>]
fn np_rfft_x6 _ = [Complex.of (5.0) (0.0), Complex.of (-3.75) (-0.4330127018922185), Complex.of (3.5) (4.330127018922194), Complex.of (-1.5) (0.0)]

def np_rfft_x7 :: unit -> [Complex<f64>]
fn np_rfft_x7 _ = [Complex.of (9.25) (0.0), Complex.of (-0.7747577830243954) (1.4811393501427186), Complex.of (-1.1558724504646836) (9.118771056606715), Complex.of (-0.9443697665109216) (-0.9752515791581511)]

def np_rfft_x6_ortho :: unit -> [Complex<f64>]
fn np_rfft_x6_ortho _ = [Complex.of (2.041241452319315) (0.0), Complex.of (-1.5309310892394865) (-0.1767766952966366), Complex.of (1.4288690166235207) (1.7677669529663693), Complex.of (-0.6123724356957946) (0.0)]

def np_rfft_x6_n9 :: unit -> [Complex<f64>]
fn np_rfft_x6_n9 _ = [Complex.of (5.0) (0.0), Complex.of (-2.0951823537613388) (-3.598684656341857), Complex.of (-3.1549343528757348) (2.453180000908014), Complex.of (3.5) (4.330127018922193), Complex.of (1.5001167066370735) (-3.0414020824867345)]

def np_rfft_x6_n4 :: unit -> [Complex<f64>]
fn np_rfft_x6_n4 _ = [Complex.of (4.75) (0.0), Complex.of (-1.5) (4.75), Complex.of (0.25) (0.0)]

def np_irfft_h4 :: unit -> [f64]
fn np_irfft_h4 _ = [(0.75), (0.08931639747704082), (0.6547005383792515), (-0.08333333333333326), (-1.6547005383792515), (1.2440169358562925)]

def np_irfft_h4_n5 :: unit -> [f64]
fn np_irfft_h4_n5 _ = [(1.2000000000000002), (-0.039509499607937884), (0.9909717238462049), (-1.7617921170961417), (0.6103298928578749)]

def np_irfft_h4_n10 :: unit -> [f64]
fn np_irfft_h4_n10 _ = [(0.30000000000000004), (0.04658391036660286), (0.25233961112313896), (0.22505812174918027), (0.35522793779586054), (0.09999999999999992), (-0.9260483310457975), (-0.9194853127490963), (0.518480782126798), (1.0478432806333131)]

def np_irfft_h4_forward :: unit -> [f64]
fn np_irfft_h4_forward _ = [(4.5), (0.535898384862245), (3.9282032302755088), (-0.49999999999999956), (-9.928203230275509), (7.464101615137755)]

def np_hfft_h4 :: unit -> [f64]
fn np_hfft_h4 _ = [(4.5), (7.464101615137754), (-9.928203230275509), (-0.5000000000000001), (3.9282032302755088), (0.5358983848622457)]

def np_hfft_h4_n7 :: unit -> [f64]
fn np_hfft_h4_n7 _ = [(3.0), (10.477191254899537), (-7.605596009606888), (-6.384115284834382), (3.7584695490703828), (2.282551990999133), (1.471498499472216)]

def np_hfft_h4_ortho :: unit -> [f64]
fn np_hfft_h4_ortho _ = [(1.8371173070873839), (3.0472067242285474), (-4.053171996137779), (-0.2041241452319316), (1.6036822533546011), (0.21877959948235717)]

def np_ihfft_x6 :: unit -> [Complex<f64>]
fn np_ihfft_x6 _ = [Complex.of (0.8333333333333333) (-0.0), Complex.of (-0.625) (0.07216878364870308), Complex.of (0.5833333333333333) (-0.7216878364870323), Complex.of (-0.25) (-0.0)]

def np_ihfft_x7 :: unit -> [Complex<f64>]
fn np_ihfft_x7 _ = [Complex.of (1.3214285714285714) (-0.0), Complex.of (-0.11067968328919933) (-0.21159133573467406), Complex.of (-0.16512463578066908) (-1.3026815795152449), Complex.of (-0.13490996664441737) (0.13932165416545014)]

def np_fft2_m34 :: unit -> [Complex<f64>]
fn np_fft2_m34 _ = [Complex.of (95.1551833626657) (0.0), Complex.of (-18.520725185159527) (21.01894440656922), Complex.of (-19.604065000468545) (0.0), Complex.of (-18.520725185159527) (-21.01894440656922), Complex.of (-76.04072236015361) (56.58222768616538), Complex.of (3.1618698185080634) (-6.205734671135193), Complex.of (4.750444553293608) (-1.9303866343846519), Complex.of (6.873573992412896) (2.2247524673141292), Complex.of (-76.04072236015361) (-56.58222768616538), Complex.of (6.873573992412896) (-2.2247524673141292), Complex.of (4.750444553293608) (1.9303866343846519), Complex.of (3.1618698185080634) (6.205734671135193)]

def np_ifft2_m34 :: unit -> [Complex<f64>]
fn np_ifft2_m34 _ = [Complex.of (7.929598613555474) (0.0), Complex.of (-1.5433937654299605) (-1.7515787005474348), Complex.of (-1.6336720833723786) (0.0), Complex.of (-1.5433937654299605) (1.7515787005474348), Complex.of (-6.336726863346134) (-4.7151856405137815), Complex.of (0.2634891515423386) (0.5171445559279327), Complex.of (0.395870379441134) (0.16086555286538765), Complex.of (0.5727978327010746) (-0.1853960389428441), Complex.of (-6.336726863346134) (4.7151856405137815), Complex.of (0.5727978327010746) (0.1853960389428441), Complex.of (0.395870379441134) (-0.16086555286538765), Complex.of (0.2634891515423386) (-0.5171445559279327)]

def np_fft_m34_axis0 :: unit -> [Complex<f64>]
fn np_fft_m34_axis0 _ = [Complex.of (9.627416997969522) (0.0), Complex.of (18.18033988749895) (0.0), Complex.of (28.148142183129053) (0.0), Complex.of (39.199284294068164) (0.0), Complex.of (-15.313708498984761) (12.667714711989916), Complex.of (-18.090169943749473) (13.7002275366613), Complex.of (-20.331430404445243) (14.658205813900448), Complex.of (-22.305413512974134) (15.556079623613716), Complex.of (-15.313708498984761) (-12.667714711989916), Complex.of (-18.090169943749473) (-13.7002275366613), Complex.of (-20.331430404445243) (-14.658205813900448), Complex.of (-22.305413512974134) (-15.556079623613716)]

def np_fftn_m34_s53 :: unit -> [Complex<f64>]
fn np_fftn_m34_s53 _ = [Complex.of (55.955899068597525) (0.0), Complex.of (-13.53682403734448) (8.632370007916514), Complex.of (-13.53682403734448) (-8.632370007916514), Complex.of (-61.93570556210036) (-47.66119472703856), Complex.of (7.216681621553201) (7.9113765093803305), Complex.of (-3.282462864951798) (9.339952940279177), Complex.of (-8.971176160332927) (49.73226493061), Complex.of (-2.001652194860517) (-3.097211131584859), Complex.of (2.0331896637381197) (-3.8107392495608874), Complex.of (-8.971176160332927) (-49.73226493061), Complex.of (2.0331896637381197) (3.8107392495608874), Complex.of (-2.001652194860517) (3.097211131584859), Complex.of (-61.93570556210036) (47.66119472703856), Complex.of (-3.282462864951798) (-9.339952940279177), Complex.of (7.216681621553201) (-7.9113765093803305)]

def np_fftn_m34_axis0_s2 :: unit -> [Complex<f64>]
fn np_fftn_m34_axis0_s2 _ = [Complex.of (-6.0) (0.0), Complex.of (-1.819660112501051) (0.0), Complex.of (3.525365581445259) (0.0), Complex.of (9.716411600158768) (0.0), Complex.of (-8.0) (0.0), Complex.of (-10.180339887498949) (0.0), Complex.of (-11.86851133195288) (0.0), Complex.of (-13.324106754745504) (0.0)]

def np_fftn_m34_ortho :: unit -> [Complex<f64>]
fn np_fftn_m34_ortho _ = [Complex.of (27.468935364611625) (0.0), Complex.of (-5.346472835619468) (6.067646605607259), Complex.of (-5.659206102615719) (0.0), Complex.of (-5.346472835619468) (-6.067646605607259), Complex.of (-21.951065762004145) (16.33388219297814), Complex.of (0.912753195429092) (-1.791441291449649), Complex.of (1.3713352208072283) (-0.5572546215010173), Complex.of (1.9842298974071984) (0.6422307179420484), Complex.of (-21.951065762004145) (-16.33388219297814), Complex.of (1.9842298974071984) (-0.6422307179420484), Complex.of (1.3713352208072283) (0.5572546215010173), Complex.of (0.912753195429092) (1.791441291449649)]

def np_rfft2_m34 :: unit -> [Complex<f64>]
fn np_rfft2_m34 _ = [Complex.of (95.1551833626657) (0.0), Complex.of (-18.520725185159527) (21.01894440656922), Complex.of (-19.604065000468545) (0.0), Complex.of (-76.04072236015361) (56.58222768616538), Complex.of (3.1618698185080634) (-6.205734671135193), Complex.of (4.750444553293608) (-1.9303866343846519), Complex.of (-76.04072236015361) (-56.58222768616538), Complex.of (6.873573992412896) (-2.2247524673141292), Complex.of (4.750444553293608) (1.9303866343846519)]

def np_rfftn_m34_s46 :: unit -> [Complex<f64>]
fn np_rfftn_m34_s46 _ = [Complex.of (95.1551833626657) (0.0), Complex.of (-34.55576844391369) (-40.12164239193574), Complex.of (25.66246025672369) (8.632370007916515), Complex.of (-19.604065000468545) (0.0), Complex.of (-108.70848674610988) (-24.397537521650154), Complex.of (-0.22954676219221248) (59.73185709292273), Complex.of (-23.471498834864178) (-9.00159763943285), Complex.of (5.864953796295643) (7.003660608252016), Complex.of (46.360108319365395) (0.0), Complex.of (-9.998651519809304) (-19.549592844147227), Complex.of (12.499220246017437) (2.5414426162378887), Complex.of (-5.596743783964511) (0.0), Complex.of (-108.70848674610988) (24.397537521650154), Complex.of (20.342502785596302) (35.17474016881834), Complex.of (-29.562426226542804) (4.161642371273404), Complex.of (5.864953796295643) (-7.003660608252016)]

def np_rfftn_m34_axes10 :: unit -> [Complex<f64>]
fn np_rfftn_m34_axes10 _ = [Complex.of (95.15518336266568) (0.0), Complex.of (-18.52072518515953) (21.018944406569215), Complex.of (-19.604065000468538) (0.0), Complex.of (-18.52072518515953) (-21.018944406569215), Complex.of (-76.04072236015361) (56.58222768616538), Complex.of (3.161869818508066) (-6.205734671135193), Complex.of (4.750444553293605) (-1.93038663438465), Complex.of (6.873573992412897) (2.2247524673141292)]

def np_irfft2_f :: unit -> [f64]
fn np_irfft2_f _ = [(-6.999999999999998), (-5.999999999999998), (-4.17157287525381), (-1.8038475772933666), (1.0000000000000009), (4.18033988749895), (7.69693845669907), (11.520259177452136), (15.627416997969519), (19.999999999999996), (24.62277660168379), (29.482872693909396)]

def np_irfftn_f_s35 :: unit -> [f64]
fn np_irfftn_f_s35 _ = [(-6.273545059592088), (-4.651189352687054), (-4.28263418995894), (-2.309484981608308), (-1.4585668687007833), (-0.6007321216504029), (3.5258813410166887), (4.455263728056792), (7.9067007773780125), (9.110423796849068), (10.6553977795244), (16.214984232337322), (17.486793789647088), (21.94590796472464), (23.429982527329255)]

def c234 :: unit -> [Complex<f64>]
fn c234 _ = [Complex.of (0.0) (-3.0), Complex.of (1.0) (-2.0), Complex.of (2.0) (-1.0), Complex.of (3.0) (0.0), Complex.of (4.0) (1.0), Complex.of (0.0) (2.0), Complex.of (1.0) (3.0), Complex.of (2.0) (-3.0), Complex.of (3.0) (-2.0), Complex.of (4.0) (-1.0), Complex.of (0.0) (0.0), Complex.of (1.0) (1.0), Complex.of (2.0) (2.0), Complex.of (3.0) (3.0), Complex.of (4.0) (-3.0), Complex.of (0.0) (-2.0), Complex.of (1.0) (-1.0), Complex.of (2.0) (0.0), Complex.of (3.0) (1.0), Complex.of (4.0) (2.0), Complex.of (0.0) (3.0), Complex.of (1.0) (-3.0), Complex.of (2.0) (-2.0), Complex.of (3.0) (-1.0)]

def np_fftn_c234 :: unit -> [Complex<f64>]
fn np_fftn_c234 _ = [Complex.of (46.0) (-6.0), Complex.of (0.0) (4.0), Complex.of (-2.0) (2.0), Complex.of (-4.0) (0.0), Complex.of (8.160254037844386) (-8.598076211353316), Complex.of (-3.2320508075688776) (-5.06217782649107), Complex.of (2.5) (-11.330127018922193), Complex.of (-18.892304845413264) (12.06217782649107), Complex.of (-9.160254037844386) (-3.401923788646684), Complex.of (0.23205080756887675) (7.0621778264910695), Complex.of (2.5) (-2.6698729810778072), Complex.of (1.8923048454132632) (-0.06217782649106951), Complex.of (-4.0) (-4.0), Complex.of (10.0) (-14.0), Complex.of (0.0) (0.0), Complex.of (10.0) (-14.0), Complex.of (-2.5) (-2.6698729810778072), Complex.of (-5.107695154586737) (-2.0621778264910704), Complex.of (4.624355652982141) (-4.330127018922193), Complex.of (7.232050807568877) (-4.93782217350893), Complex.of (-2.5) (-11.330127018922193), Complex.of (-25.892304845413264) (10.06217782649107), Complex.of (-19.62435565298214) (4.330127018922193), Complex.of (3.7679491924311224) (-17.06217782649107)]

def np_fft_c234_axis1 :: unit -> [Complex<f64>]
fn np_fft_c234_axis1 _ = [Complex.of (7.0) (-4.0), Complex.of (5.0) (-1.0), Complex.of (3.0) (2.0), Complex.of (6.0) (-2.0), Complex.of (-0.901923788646684) (-3.3660254037844384), Complex.of (1.598076211353316) (0.9641016151377544), Complex.of (4.098076211353316) (-3.3660254037844384), Complex.of (-1.9641016151377544) (0.1339745962155614), Complex.of (-6.098076211353316) (-1.6339745962155614), Complex.of (-3.598076211353316) (-5.964101615137754), Complex.of (-1.098076211353316) (-1.6339745962155614), Complex.of (4.964101615137754) (1.8660254037844386), Complex.of (3.0) (4.0), Complex.of (6.0) (0.0), Complex.of (9.0) (-4.0), Complex.of (7.0) (-1.0), Complex.of (-1.9641016151377544) (0.1339745962155614), Complex.of (4.098076211353316) (3.6339745962155616), Complex.of (4.098076211353316) (-3.3660254037844384), Complex.of (-0.901923788646684) (-3.3660254037844384), Complex.of (4.964101615137754) (1.8660254037844386), Complex.of (-1.098076211353316) (5.366025403784438), Complex.of (-1.098076211353316) (-1.6339745962155614), Complex.of (-6.098076211353316) (-1.6339745962155614)]

def np_fftfreq_8 :: unit -> [f64]
fn np_fftfreq_8 _ = [(0.0), (1.25), (2.5), (3.75), (-5.0), (-3.75), (-2.5), (-1.25)]

def np_fftfreq_7 :: unit -> [f64]
fn np_fftfreq_7 _ = [(0.0), (0.5714285714285714), (1.1428571428571428), (1.7142857142857142), (-1.7142857142857142), (-1.1428571428571428), (-0.5714285714285714)]

def np_rfftfreq_8 :: unit -> [f64]
fn np_rfftfreq_8 _ = [(0.0), (1.25), (2.5), (3.75), (5.0)]

def np_rfftfreq_7 :: unit -> [f64]
fn np_rfftfreq_7 _ = [(0.0), (0.5714285714285714), (1.1428571428571428), (1.7142857142857142)]

def np_fftshift_35 :: unit -> [i64]
fn np_fftshift_35 _ = [13, 14, 10, 11, 12, 3, 4, 0, 1, 2, 8, 9, 5, 6, 7]

def np_ifftshift_35 :: unit -> [i64]
fn np_ifftshift_35 _ = [7, 8, 9, 5, 6, 12, 13, 14, 10, 11, 2, 3, 4, 0, 1]

def np_fftshift_35_axis1 :: unit -> [i64]
fn np_fftshift_35_axis1 _ = [3, 4, 0, 1, 2, 8, 9, 5, 6, 7, 13, 14, 10, 11, 12]

def np_ifftshift_35_axis0 :: unit -> [i64]
fn np_ifftshift_35_axis0 _ = [5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0, 1, 2, 3, 4]
// reference:end
"#;

#[test]
fn complex_transforms_match_the_naive_dft_for_every_kind_of_length() {
    run_tz_test(
        r#"// Powers of two (radix 4 and 2), lengths with factors 3 and 5 (mixed radix), and lengths with a larger prime
// factor (Bluestein): 1, 2, 3, 4, 7, 8, 12, 15, 16, 17, 64 and 100 are the lengths of the ticket.
test "fft and ifft against the definition" =
    for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 24, 25, 27, 30, 31, 32, 45, 60, 64, 75, 81, 100, 125, 127, 128, 243, 250, 256, 360, 1000, 1024, 1031] do
        let xs = wave n
        let t = Tensor.of_array [n] (Array.copy (ref xs))
        let forward = complex_of (Fft.fft (Tensor.view t))
        assert (gap (ref forward) (ref (naive_dft false (ref xs))) < 4e-14 * (n as f64))
        let inverse = complex_of (Fft.ifft (Tensor.view t))
        assert (gap (ref inverse) (ref (naive_dft true (ref xs))) < 4e-14)
        let spectrum = Tensor.of_array [n] forward
        let back = complex_of (Fft.ifft (Tensor.view spectrum))
        assert (gap (ref back) (ref xs) < 2e-14)

test "Parseval" =
    for n in [8, 15, 17, 100, 1031] do
        let xs = wave n
        let t = Tensor.of_array [n] (Array.copy (ref xs))
        let spectrum = complex_of (Fft.fft (Tensor.view t))
        let mut time_energy = 0.0
        let mut frequency_energy = 0.0
        for i in 0i64 ..< n do
            time_energy = time_energy + xs[i].re * xs[i].re + xs[i].im * xs[i].im
            frequency_energy = frequency_energy + spectrum[i].re * spectrum[i].re + spectrum[i].im * spectrum[i].im
        assert (Math.abs (time_energy - frequency_energy / (n as f64)) < 1e-12 * time_energy)
"#,
    );
}

#[test]
fn single_precision_keeps_its_accuracy_for_long_chirps() {
    run_tz_test(
        r#"// f32 against the f64 transform (itself checked against the definition). 8209 is a prime above 4096, where a
// chirp phase computed as k * k in f32 would lose its low bits; the roots are rounded once from f64 instead.
test "f32 against f64" =
    for n in [1, 2, 3, 5, 8, 12, 17, 100, 1000, 1024, 1031, 8209] do
        let xs = wave n
        let wide = Tensor.of_array [n] (Array.copy (ref xs))
        let narrow = Tensor.of_array [n] (new [Complex<f32>](n, \i -> Complex.of (xs[i].re as f32) (xs[i].im as f32)))
        let narrow_input = new [Complex<f64>](n, \i -> Complex.of ((xs[i].re as f32) as f64) ((xs[i].im as f32) as f64))
        let rounded = Tensor.of_array [n] narrow_input
        let expected = complex_of (Fft.fft (Tensor.view rounded))
        match Fft.fft (Tensor.view narrow) with
        | Result.Ok y ->
            let s = Tensor.as_array y
            let got = new [Complex<f64>](n, \i -> Complex.of (s[i].re as f64) (s[i].im as f64))
            let mut scale = 1.0
            for i in 0i64 ..< n do
                let size = Math.hypot expected[i].re expected[i].im
                if size > scale then scale = size
            assert (gap (ref got) (ref expected) < 2e-5 * scale)
            match Fft.ifft (Tensor.view y) with
            | Result.Ok back ->
                let b = Tensor.as_array back
                let returned = new [Complex<f64>](n, \i -> Complex.of (b[i].re as f64) (b[i].im as f64))
                assert (gap (ref returned) (ref xs) < 2e-5)
            | Result.Error _ -> assert false
        | Result.Error _ -> assert false
        assert ((Tensor.as_array wide).length == n)
"#,
    );
}

#[test]
fn norms_lengths_and_axes_match_numpy() {
    run_tz_test(
        r#"test "norm" =
    let z = Tensor.of_array [5] (z5 ())
    let v = Tensor.view z
    assert (gap (ref (complex_of (Fft.fft v))) (ref (np_fft_z5 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.fft_with (Fft.Options { axis: -1, n: Maybe.None, norm: Fft.Norm.Ortho }) v))) (ref (np_fft_z5_ortho ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.fft_with (Fft.Options { axis: 0, n: Maybe.None, norm: Fft.Norm.Forward }) v))) (ref (np_fft_z5_forward ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.ifft v))) (ref (np_ifft_z5 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.ifft_with (Fft.Options { axis: -1, n: Maybe.None, norm: Fft.Norm.Ortho }) v))) (ref (np_ifft_z5_ortho ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.ifft_with (Fft.Options { axis: -1, n: Maybe.None, norm: Fft.Norm.Forward }) v))) (ref (np_ifft_z5_forward ())) < 1e-14)

test "a longer n pads with zeros and a shorter n cuts" =
    let z = Tensor.of_array [5] (z5 ())
    let v = Tensor.view z
    assert (gap (ref (complex_of (Fft.fft_with (Fft.Options { axis: -1, n: Maybe.Some 8, norm: Fft.Norm.Backward }) v))) (ref (np_fft_z5_n8 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.fft_with (Fft.Options { axis: -1, n: Maybe.Some 3, norm: Fft.Norm.Backward }) v))) (ref (np_fft_z5_n3 ())) < 1e-14)

test "axes of a matrix and of a rank 3 tensor" =
    let m = Tensor.of_array [3, 4] (Array.map (\x -> Complex.of x 0.0) (m34 ()))
    let mv = Tensor.view m
    assert (gap (ref (complex_of (Fft.fft_axis 0 mv))) (ref (np_fft_m34_axis0 ())) < 1e-13)
    assert (gap (ref (complex_of (Fft.fft_axis (-2) mv))) (ref (np_fft_m34_axis0 ())) < 1e-13)
    let c = Tensor.of_array [2, 3, 4] (c234 ())
    let cv = Tensor.view c
    assert (gap (ref (complex_of (Fft.fft_axis 1 cv))) (ref (np_fft_c234_axis1 ())) < 1e-13)
    assert (shape_of (Fft.fft_axis 1 cv) == [2, 3, 4])
"#,
    );
}

#[test]
fn real_and_hermitian_transforms_match_the_definition_and_numpy() {
    run_tz_test(
        r#"test "rfft is the first half of fft" =
    for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 15, 16, 17, 30, 34, 64, 100, 127, 250, 1000, 2062] do
        let xs = wave_real n
        let as_complex = new [Complex<f64>](n, \i -> Complex.of xs[i] 0.0)
        let full = naive_dft false (ref as_complex)
        let t = Tensor.of_array [n] (Array.copy (ref xs))
        let half = complex_of (Fft.rfft (Tensor.view t))
        assert (half.length == n / 2 + 1)
        let expected = Array.sub 0 (n / 2 + 1) (ref full)
        assert (gap (ref half) (ref expected) < 4e-14 * (n as f64) * (n as f64))
        let spectrum = Tensor.of_array [n / 2 + 1] half
        let back = real_of (Fft.irfft_len n (Tensor.view spectrum))
        assert (gap_real (ref back) (ref xs) < 1e-13 * (n as f64))

test "rfft against NumPy" =
    let a = Tensor.of_array [6] (x6 ())
    let b = Tensor.of_array [7] (x7 ())
    let av = Tensor.view a
    assert (gap (ref (complex_of (Fft.rfft av))) (ref (np_rfft_x6 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.rfft (Tensor.view b)))) (ref (np_rfft_x7 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.rfft_axis 0 av))) (ref (np_rfft_x6 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.rfft_with (Fft.Options { axis: -1, n: Maybe.None, norm: Fft.Norm.Ortho }) av))) (ref (np_rfft_x6_ortho ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.rfft_with (Fft.Options { axis: -1, n: Maybe.Some 9, norm: Fft.Norm.Backward }) av))) (ref (np_rfft_x6_n9 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.rfft_with (Fft.Options { axis: -1, n: Maybe.Some 4, norm: Fft.Norm.Backward }) av))) (ref (np_rfft_x6_n4 ())) < 1e-14)

test "irfft against NumPy" =
    let h = Tensor.of_array [4] (h4 ())
    let hv = Tensor.view h
    assert (gap_real (ref (real_of (Fft.irfft hv))) (ref (np_irfft_h4 ())) < 1e-14)
    assert (gap_real (ref (real_of (Fft.irfft_len 5 hv))) (ref (np_irfft_h4_n5 ())) < 1e-14)
    assert (gap_real (ref (real_of (Fft.irfft_len 10 hv))) (ref (np_irfft_h4_n10 ())) < 1e-14)
    assert (gap_real (ref (real_of (Fft.irfft_with (Fft.Options { axis: 0, n: Maybe.None, norm: Fft.Norm.Forward }) hv))) (ref (np_irfft_h4_forward ())) < 1e-14)

test "hfft and ihfft against NumPy" =
    let h = Tensor.of_array [4] (h4 ())
    let hv = Tensor.view h
    assert (gap_real (ref (real_of (Fft.hfft hv))) (ref (np_hfft_h4 ())) < 1e-13)
    assert (gap_real (ref (real_of (Fft.hfft_len 7 hv))) (ref (np_hfft_h4_n7 ())) < 1e-13)
    assert (gap_real (ref (real_of (Fft.hfft_with (Fft.Options { axis: -1, n: Maybe.None, norm: Fft.Norm.Ortho }) hv))) (ref (np_hfft_h4_ortho ())) < 1e-13)
    let a = Tensor.of_array [6] (x6 ())
    let b = Tensor.of_array [7] (x7 ())
    assert (gap (ref (complex_of (Fft.ihfft (Tensor.view a)))) (ref (np_ihfft_x6 ())) < 1e-14)
    assert (gap (ref (complex_of (Fft.ihfft (Tensor.view b)))) (ref (np_ihfft_x7 ())) < 1e-14)
    // ihfft with the forward scaling is the conjugate of the unscaled rfft.
    let unscaled = complex_of (Fft.ihfft_with (Fft.Options { axis: -1, n: Maybe.None, norm: Fft.Norm.Forward }) (Tensor.view a))
    let direct = np_rfft_x6 ()
    assert (gap (ref unscaled) (ref (new [Complex<f64>](4, \i -> Complex.of direct[i].re (0.0 - direct[i].im)))) < 1e-14)
"#,
    );
}

#[test]
fn multi_dimensional_transforms_match_numpy() {
    run_tz_test(
        r#"test "fft2 and fftn" =
    let m = Tensor.of_array [3, 4] (Array.map (\x -> Complex.of x 0.0) (m34 ()))
    let mv = Tensor.view m
    assert (gap (ref (complex_of (Fft.fft2 mv))) (ref (np_fft2_m34 ())) < 1e-12)
    assert (gap (ref (complex_of (Fft.fftn mv))) (ref (np_fft2_m34 ())) < 1e-12)
    assert (gap (ref (complex_of (Fft.ifft2 mv))) (ref (np_ifft2_m34 ())) < 1e-13)
    assert (gap (ref (complex_of (Fft.ifftn mv))) (ref (np_ifft2_m34 ())) < 1e-13)
    let padded = Fft.fftn_with (Fft.NdOptions { axes: Maybe.Some [0, 1], shape: Maybe.Some [5, 3], norm: Fft.Norm.Backward }) mv
    assert (gap (ref (complex_of padded)) (ref (np_fftn_m34_s53 ())) < 1e-12)
    // Without axes, a shape of k lengths names the last k axes.
    let last_axes = Fft.fftn_with (Fft.NdOptions { axes: Maybe.None, shape: Maybe.Some [5, 3], norm: Fft.Norm.Backward }) mv
    assert (gap (ref (complex_of last_axes)) (ref (np_fftn_m34_s53 ())) < 1e-12)
    let one_axis = Fft.fftn_with (Fft.NdOptions { axes: Maybe.Some [0], shape: Maybe.Some [2], norm: Fft.Norm.Backward }) mv
    assert (shape_of one_axis == [2, 4])
    let one_axis_again = Fft.fftn_with (Fft.NdOptions { axes: Maybe.Some [-2], shape: Maybe.Some [2], norm: Fft.Norm.Backward }) mv
    assert (gap (ref (complex_of one_axis_again)) (ref (np_fftn_m34_axis0_s2 ())) < 1e-13)
    let ortho = Fft.fftn_with (Fft.NdOptions { axes: Maybe.None, shape: Maybe.None, norm: Fft.Norm.Ortho }) mv
    assert (gap (ref (complex_of ortho)) (ref (np_fftn_m34_ortho ())) < 1e-13)
    let c = Tensor.of_array [2, 3, 4] (c234 ())
    let spectrum = complex_of (Fft.fftn (Tensor.view c))
    assert (gap (ref spectrum) (ref (np_fftn_c234 ())) < 1e-12)
    let s = Tensor.of_array [2, 3, 4] spectrum
    assert (gap (ref (complex_of (Fft.ifftn (Tensor.view s)))) (ref (c234 ())) < 1e-13)
    let partial = Fft.ifftn_with (Fft.NdOptions { axes: Maybe.Some [1], shape: Maybe.None, norm: Fft.Norm.Backward }) (Tensor.view c)
    let forward_again = Tensor.of_array [2, 3, 4] (complex_of partial)
    assert (gap (ref (complex_of (Fft.fft_axis 1 (Tensor.view forward_again)))) (ref (c234 ())) < 1e-13)

test "rfft2, rfftn and their inverses" =
    let m = Tensor.of_array [3, 4] (m34 ())
    let mv = Tensor.view m
    assert (gap (ref (complex_of (Fft.rfft2 mv))) (ref (np_rfft2_m34 ())) < 1e-12)
    assert (gap (ref (complex_of (Fft.rfftn mv))) (ref (np_rfft2_m34 ())) < 1e-12)
    assert (shape_of (Fft.rfft2 mv) == [3, 3])
    let padded = Fft.rfftn_with (Fft.NdOptions { axes: Maybe.Some [0, 1], shape: Maybe.Some [4, 6], norm: Fft.Norm.Backward }) mv
    assert (shape_of (Fft.rfftn_with (Fft.NdOptions { axes: Maybe.Some [0, 1], shape: Maybe.Some [4, 6], norm: Fft.Norm.Backward }) mv) == [4, 4])
    assert (gap (ref (complex_of padded)) (ref (np_rfftn_m34_s46 ())) < 1e-12)
    // The last listed axis is the real one: axes [1, 0] halve axis 0.
    let turned = Fft.rfftn_with (Fft.NdOptions { axes: Maybe.Some [1, 0], shape: Maybe.None, norm: Fft.Norm.Backward }) mv
    assert (shape_of (Fft.rfftn_with (Fft.NdOptions { axes: Maybe.Some [1, 0], shape: Maybe.None, norm: Fft.Norm.Backward }) mv) == [2, 4])
    assert (gap (ref (complex_of turned)) (ref (np_rfftn_m34_axes10 ())) < 1e-12)
    let f = Tensor.of_array [3, 3] (np_rfft2_m34 ())
    let fv = Tensor.view f
    assert (gap_real (ref (real_of (Fft.irfft2 fv))) (ref (np_irfft2_f ())) < 1e-13)
    assert (gap_real (ref (real_of (Fft.irfftn fv))) (ref (m34 ())) < 1e-13)
    let odd = Fft.irfftn_with (Fft.NdOptions { axes: Maybe.Some [0, 1], shape: Maybe.Some [3, 5], norm: Fft.Norm.Backward }) fv
    assert (gap_real (ref (real_of odd)) (ref (np_irfftn_f_s35 ())) < 1e-13)
"#,
    );
}

#[test]
fn strided_windows_and_empty_axes() {
    run_tz_test(
        r#"test "a reversed, a permuted and a broadcast window" =
    let c = Tensor.of_array [2, 3, 4] (c234 ())
    let cv = Tensor.view c
    let reversed = Tensor.flip_axis 2 cv
    let copy = Tensor.to_tensor reversed
    assert (same_bits (ref (complex_of (Fft.fft reversed))) (ref (complex_of (Fft.fft (Tensor.view copy)))))
    let permuted = Tensor.permute [2, 0, 1] cv
    let permuted_copy = Tensor.to_tensor permuted
    assert (same_bits (ref (complex_of (Fft.fftn permuted))) (ref (complex_of (Fft.fftn (Tensor.view permuted_copy)))))
    assert (same_bits (ref (complex_of (Fft.fft_axis 0 permuted))) (ref (complex_of (Fft.fft_axis 0 (Tensor.view permuted_copy)))))
    let real = Tensor.of_array [3, 4] (m34 ())
    let real_view = Tensor.view real
    let real_reversed = Tensor.flip_axis 1 real_view
    let real_copy = Tensor.to_tensor real_reversed
    assert (same_bits (ref (complex_of (Fft.rfft real_reversed))) (ref (complex_of (Fft.rfft (Tensor.view real_copy)))))

test "a transform axis without elements is an error, other empty axes are not" =
    let empty: Tensor<Complex<f64>> = Tensor.of_array [0] []
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.fft (Tensor.view empty)))
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.ifft (Tensor.view empty)))
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.fftn (Tensor.view empty)))
    let rows: Tensor<Complex<f64>> = Tensor.of_array [0, 4] []
    assert (shape_of (Fft.fft (Tensor.view rows)) == [0, 4])
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.fft_axis 0 (Tensor.view rows)))
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.fft2 (Tensor.view rows)))
    let z = Tensor.of_array [5] (z5 ())
    let v = Tensor.view z
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.fft_with (Fft.Options { axis: -1, n: Maybe.Some 0, norm: Fft.Norm.Backward }) v))
    assert (fails_with (Fft.Error.InvalidLength (-3)) (Fft.fft_with (Fft.Options { axis: -1, n: Maybe.Some (-3), norm: Fft.Norm.Backward }) v))
    assert (fails_with (Fft.Error.InvalidAxis (1, 1)) (Fft.fft_axis 1 v))
    assert (fails_with (Fft.Error.InvalidAxis (-2, 1)) (Fft.ifft_axis (-2) v))
    assert (fails_with (Fft.Error.InvalidAxis (-2, 1)) (Fft.fft2 v))
    let m = Tensor.of_array [3, 4] (Array.map (\x -> Complex.of x 0.0) (m34 ()))
    let mv = Tensor.view m
    assert (fails_with (Fft.Error.RepeatedAxis (-1)) (Fft.fftn_with (Fft.NdOptions { axes: Maybe.Some [1, -1], shape: Maybe.None, norm: Fft.Norm.Backward }) mv))
    assert (fails_with (Fft.Error.LengthMismatch (2, 1)) (Fft.fftn_with (Fft.NdOptions { axes: Maybe.Some [0, 1], shape: Maybe.Some [4], norm: Fft.Norm.Backward }) mv))
    assert (fails_with (Fft.Error.LengthMismatch (2, 3)) (Fft.fftn_with (Fft.NdOptions { axes: Maybe.None, shape: Maybe.Some [4, 4, 4], norm: Fft.Norm.Backward }) mv))
    assert (fails_with (Fft.Error.EmptyAxis 1) (Fft.ifftn_with (Fft.NdOptions { axes: Maybe.Some [0, 1], shape: Maybe.Some [4, 0], norm: Fft.Norm.Backward }) mv))
    // A single term has no real samples to give back: 2 (1 - 1) = 0.
    let single = Tensor.of_array [1] [Complex.of 1.0 0.0]
    assert (real_fails_with (Fft.Error.EmptyAxis 0) (Fft.irfft (Tensor.view single)))
    assert (real_fails_with (Fft.Error.EmptyAxis 0) (Fft.hfft (Tensor.view single)))
    assert (real_fails_with (Fft.Error.EmptyAxis 0) (Fft.irfftn (Tensor.view single)))
    assert (gap_real (ref (real_of (Fft.irfft_len 1 (Tensor.view single)))) [1.0] == 0.0)
    let no_reals: Tensor<f64> = Tensor.of_array [0] []
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.rfft (Tensor.view no_reals)))
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.rfftn (Tensor.view no_reals)))
    assert (fails_with (Fft.Error.LengthMismatch (1, 0)) (Fft.rfftn_with (Fft.NdOptions { axes: Maybe.Some [], shape: Maybe.None, norm: Fft.Norm.Backward }) (Tensor.view no_reals)))
    let scalar = Tensor.scalar (Complex.of 2.0 1.0)
    assert (fails_with (Fft.Error.InvalidAxis (-1, 0)) (Fft.fft (Tensor.view scalar)))
    assert (same_bits (ref (complex_of (Fft.fftn (Tensor.view scalar)))) [Complex.of 2.0 1.0])
    assert ($"{Fft.Error.EmptyAxis 2}" == "axis 2 has no elements to transform")
"#,
    );
}

#[test]
fn frequencies_and_shifts_match_numpy() {
    run_tz_test(
        r#"test "fftfreq and rfftfreq" =
    assert (Tensor.to_array (Fft.fftfreq 0.1 8) == np_fftfreq_8 ())
    assert (Tensor.to_array (Fft.fftfreq 0.25 7) == np_fftfreq_7 ())
    assert (Tensor.to_array (Fft.rfftfreq 0.1 8) == np_rfftfreq_8 ())
    assert (Tensor.to_array (Fft.rfftfreq 0.25 7) == np_rfftfreq_7 ())
    assert (Tensor.to_array (Fft.fftfreq 2.0 1) == [0.0])

test "fftshift and ifftshift" =
    let a = Tensor.init [3, 5] (\i -> i)
    let av = Tensor.view a
    assert (Tensor.to_array (Fft.fftshift av) == np_fftshift_35 ())
    assert (Tensor.to_array (Fft.ifftshift av) == np_ifftshift_35 ())
    match (Fft.fftshift_axes [1] av, Fft.ifftshift_axes [0] av, Fft.fftshift_axes [-1] av) with
    | (Result.Ok shifted, Result.Ok unshifted, Result.Ok negative) ->
        assert (Tensor.to_array shifted == np_fftshift_35_axis1 ())
        assert (Tensor.to_array unshifted == np_ifftshift_35_axis0 ())
        assert (Tensor.to_array negative == np_fftshift_35_axis1 ())
    | _ -> assert false
    let shifted = Fft.fftshift av
    assert (Tensor.to_array (Fft.ifftshift (Tensor.view shifted)) == Tensor.to_array (Tensor.to_tensor av))
    match Fft.fftshift_axes [2] av with
    | Result.Error problem -> assert (problem == Fft.Error.InvalidAxis (2, 2))
    | Result.Ok _ -> assert false
    let flags = Tensor.of_array [3] [true, false, false]
    assert (Tensor.to_array (Fft.fftshift (Tensor.view flags)) == [false, true, false])
"#,
    );
}

#[test]
fn plans_are_reused_without_allocating() {
    run_tz_test(
        r#"test "execute gives the bits of fft and ifft" =
    for n in [1, 2, 16, 30, 17, 1031] do
        let xs = wave (3 * n)
        let t = Tensor.of_array [3, n] (Array.copy (ref xs))
        let v = Tensor.view t
        let direct: Result<Fft.Plan<f64>, Fft.Error> = Fft.plan n Fft.Direction.Direct
        let inverse: Result<Fft.Plan<f64>, Fft.Error> = Fft.plan n Fft.Direction.Inverse
        match (direct, inverse) with
        | (Result.Ok forward_plan, Result.Ok inverse_plan) ->
            assert (Fft.plan_length forward_plan == n)
            assert (same_bits (ref (complex_of (Fft.execute forward_plan v))) (ref (complex_of (Fft.fft v))))
            assert (same_bits (ref (complex_of (Fft.execute inverse_plan v))) (ref (complex_of (Fft.ifft v))))
        | _ -> assert false

test "plan_with carries the norm, and a bad length or shape is an error" =
    let z = Tensor.of_array [5] (z5 ())
    let v = Tensor.view z
    let ortho: Result<Fft.Plan<f64>, Fft.Error> = Fft.plan_with Fft.Norm.Ortho 5 Fft.Direction.Direct
    match ortho with
    | Result.Ok p ->
        assert (gap (ref (complex_of (Fft.execute p v))) (ref (np_fft_z5_ortho ())) < 1e-14)
        let other = Tensor.of_array [4] (h4 ())
        assert (fails_with (Fft.Error.LengthMismatch (5, 4)) (Fft.execute p (Tensor.view other)))
        let scalar = Tensor.scalar (Complex.of 2.0 1.0)
        assert (fails_with (Fft.Error.InvalidAxis (-1, 0)) (Fft.execute p (Tensor.view scalar)))
    | Result.Error _ -> assert false
    let none: Result<Fft.Plan<f64>, Fft.Error> = Fft.plan 0 Fft.Direction.Direct
    let negative: Result<Fft.Plan<f32>, Fft.Error> = Fft.plan (-4) Fft.Direction.Inverse
    match (none, negative) with
    | (Result.Error first, Result.Error second) -> assert (first == Fft.Error.InvalidLength 0 && second == Fft.Error.InvalidLength (-4))
    | _ -> assert false

test "execute_into and execute_in_place allocate nothing" =
    for n in [16, 30, 17] do
        let xs = wave n
        let t = Tensor.of_array [n] (Array.copy (ref xs))
        let expected = complex_of (Fft.fft (Tensor.view t))
        let made: Result<Fft.Plan<f64>, Fft.Error> = Fft.plan n Fft.Direction.Direct
        match made with
        | Result.Ok p ->
            let needed = Fft.plan_work_length p
            assert (if n == 17 then needed == 2 * 36 else needed == n)
            let mut work = new [Complex<f64>](needed, \_ -> Complex.of 0.0 0.0)
            let mut output = new [Complex<f64>](n, \_ -> Complex.of 0.0 0.0)
            let mut data = Array.append (ref xs) (ref xs)
            let start = Alloc.snapshot ()
            let first = Fft.execute_into p (ref xs) (ref mut work) (ref mut output)
            let second = Fft.execute_in_place p (ref mut work) (ref mut data)
            let used = Alloc.since start
            assert (used.allocs == 0 && used.reallocs == 0)
            match (first, second) with
            | (Result.Ok _, Result.Ok _) -> ()
            | _ -> assert false
            assert (same_bits (ref output) (ref expected))
            assert (same_bits (ref (Array.sub 0 n (ref data))) (ref expected))
            assert (same_bits (ref (Array.sub n n (ref data))) (ref expected))
            let mut short = new [Complex<f64>](needed - 1, \_ -> Complex.of 0.0 0.0)
            match Fft.execute_into p (ref xs) (ref mut short) (ref mut output) with
            | Result.Error problem -> assert (problem == Fft.Error.LengthMismatch (needed, needed - 1))
            | Result.Ok _ -> assert false
            match Fft.execute_into p (ref data) (ref mut work) (ref mut output) with
            | Result.Error problem -> assert (problem == Fft.Error.LengthMismatch (n, 2 * n))
            | Result.Ok _ -> assert false
            match Fft.execute_in_place p (ref mut work) (ref mut data[1 ..]) with
            | Result.Error problem -> assert (problem == Fft.Error.LengthMismatch (n, 2 * n - 1))
            | Result.Ok _ -> assert false
        | Result.Error _ -> assert false
"#,
    );
}

#[test]
fn parallel_transforms_give_the_bits_of_the_serial_ones() {
    run_tz_test(
        r#"// The batches are larger than one chunk of 16384 values, so several jobs run.
test "fft_parallel and ifft_parallel" =
    for shape in [[40, 1024], [70, 1000], [9, 2053], [3, 16384], [5, 1]] do
        let total = shape[0] * shape[1]
        let t = Tensor.of_array (Array.copy (ref shape)) (wave total)
        let v = Tensor.view t
        assert (same_bits (ref (complex_of (Fft.fft_parallel v))) (ref (complex_of (Fft.fft v))))
        assert (same_bits (ref (complex_of (Fft.ifft_parallel v))) (ref (complex_of (Fft.ifft v))))

test "fft2_parallel and ifft2_parallel" =
    for shape in [[2, 128, 256], [3, 130, 67], [1, 1, 40000], [1, 40000, 1], [4, 5, 6]] do
        let total = shape[0] * shape[1] * shape[2]
        let t = Tensor.of_array (Array.copy (ref shape)) (wave total)
        let v = Tensor.view t
        assert (same_bits (ref (complex_of (Fft.fft2_parallel v))) (ref (complex_of (Fft.fft2 v))))
        assert (same_bits (ref (complex_of (Fft.ifft2_parallel v))) (ref (complex_of (Fft.ifft2 v))))
    let matrix = Tensor.of_array [200, 300] (wave 60000)
    let matrix_view = Tensor.view matrix
    let turned = Tensor.permute [1, 0] matrix_view
    assert (same_bits (ref (complex_of (Fft.fft2_parallel turned))) (ref (complex_of (Fft.fft2 turned))))

test "parallel errors are the serial ones" =
    let empty: Tensor<Complex<f64>> = Tensor.of_array [3, 0] []
    assert (fails_with (Fft.Error.EmptyAxis 1) (Fft.fft_parallel (Tensor.view empty)))
    assert (fails_with (Fft.Error.EmptyAxis 1) (Fft.fft2_parallel (Tensor.view empty)))
    let no_rows: Tensor<Complex<f64>> = Tensor.of_array [0, 3] []
    assert (fails_with (Fft.Error.EmptyAxis 0) (Fft.ifft2_parallel (Tensor.view no_rows)))
    assert (shape_of (Fft.ifft_parallel (Tensor.view no_rows)) == [0, 3])
    let z = Tensor.of_array [5] (z5 ())
    assert (fails_with (Fft.Error.InvalidAxis (-2, 1)) (Fft.fft2_parallel (Tensor.view z)))
    let scalar = Tensor.scalar (Complex.of 2.0 1.0)
    assert (fails_with (Fft.Error.InvalidAxis (-1, 0)) (Fft.fft_parallel (Tensor.view scalar)))
"#,
    );
}
