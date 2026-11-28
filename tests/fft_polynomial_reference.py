"""Prints the NumPy reference values that tests/fft.rs and tests/polynomial.rs embed (NC09).

Run `python3 tests/fft_polynomial_reference.py fft` or `... polynomial` with NumPy 2.x and paste the
output between the `// reference:begin` and `// reference:end` lines of the test file. The values are IEEE 754
doubles printed with `repr`, so the Tsuzuri literals read back the same bits.
"""

import sys

import numpy as np


def real(values):
    return "[" + ", ".join(f"({float(v)!r})" for v in np.asarray(values).ravel()) + "]"


def cplx(values):
    return "[" + ", ".join(f"Complex.of ({float(v.real)!r}) ({float(v.imag)!r})" for v in np.asarray(values).ravel()) + "]"


def ints(values):
    return "[" + ", ".join(str(int(v)) for v in np.asarray(values).ravel()) + "]"


def emit(name, kind, text):
    print(f"def {name} :: unit -> {kind}")
    print(f"fn {name} _ = {text}")
    print()


def fft():
    x6 = np.array([0.5, -1.25, 2.0, 3.5, -0.75, 1.0])
    x7 = np.array([0.5, -1.25, 2.0, 3.5, -0.75, 1.0, 4.25])
    h = np.array([1 + 0j, 2 - 1j, 0.5 + 3j, -1.5 + 0.25j])
    z = np.array([1 + 2j, -0.5 + 0.25j, 3 - 1j, 0.75 + 0.5j, -2 - 2j])
    m = np.arange(12, dtype=float).reshape(3, 4) ** 1.5 - 7.0
    f = np.fft.rfft2(m)
    emit("x6", "[f64]", real(x6))
    emit("x7", "[f64]", real(x7))
    emit("h4", "[Complex<f64>]", cplx(h))
    emit("z5", "[Complex<f64>]", cplx(z))
    emit("m34", "[f64]", real(m))
    emit("np_fft_z5", "[Complex<f64>]", cplx(np.fft.fft(z)))
    emit("np_fft_z5_ortho", "[Complex<f64>]", cplx(np.fft.fft(z, norm="ortho")))
    emit("np_fft_z5_forward", "[Complex<f64>]", cplx(np.fft.fft(z, norm="forward")))
    emit("np_ifft_z5", "[Complex<f64>]", cplx(np.fft.ifft(z)))
    emit("np_ifft_z5_ortho", "[Complex<f64>]", cplx(np.fft.ifft(z, norm="ortho")))
    emit("np_ifft_z5_forward", "[Complex<f64>]", cplx(np.fft.ifft(z, norm="forward")))
    emit("np_fft_z5_n8", "[Complex<f64>]", cplx(np.fft.fft(z, n=8)))
    emit("np_fft_z5_n3", "[Complex<f64>]", cplx(np.fft.fft(z, n=3)))
    emit("np_rfft_x6", "[Complex<f64>]", cplx(np.fft.rfft(x6)))
    emit("np_rfft_x7", "[Complex<f64>]", cplx(np.fft.rfft(x7)))
    emit("np_rfft_x6_ortho", "[Complex<f64>]", cplx(np.fft.rfft(x6, norm="ortho")))
    emit("np_rfft_x6_n9", "[Complex<f64>]", cplx(np.fft.rfft(x6, n=9)))
    emit("np_rfft_x6_n4", "[Complex<f64>]", cplx(np.fft.rfft(x6, n=4)))
    emit("np_irfft_h4", "[f64]", real(np.fft.irfft(h)))
    emit("np_irfft_h4_n5", "[f64]", real(np.fft.irfft(h, n=5)))
    emit("np_irfft_h4_n10", "[f64]", real(np.fft.irfft(h, n=10)))
    emit("np_irfft_h4_forward", "[f64]", real(np.fft.irfft(h, norm="forward")))
    emit("np_hfft_h4", "[f64]", real(np.fft.hfft(h)))
    emit("np_hfft_h4_n7", "[f64]", real(np.fft.hfft(h, n=7)))
    emit("np_hfft_h4_ortho", "[f64]", real(np.fft.hfft(h, norm="ortho")))
    emit("np_ihfft_x6", "[Complex<f64>]", cplx(np.fft.ihfft(x6)))
    emit("np_ihfft_x7", "[Complex<f64>]", cplx(np.fft.ihfft(x7)))
    emit("np_fft2_m34", "[Complex<f64>]", cplx(np.fft.fft2(m)))
    emit("np_ifft2_m34", "[Complex<f64>]", cplx(np.fft.ifft2(m)))
    emit("np_fft_m34_axis0", "[Complex<f64>]", cplx(np.fft.fft(m, axis=0)))
    emit("np_fftn_m34_s53", "[Complex<f64>]", cplx(np.fft.fftn(m, s=(5, 3), axes=(0, 1))))
    emit("np_fftn_m34_axis0_s2", "[Complex<f64>]", cplx(np.fft.fftn(m, s=(2,), axes=(0,))))
    emit("np_fftn_m34_ortho", "[Complex<f64>]", cplx(np.fft.fftn(m, norm="ortho")))
    emit("np_rfft2_m34", "[Complex<f64>]", cplx(f))
    emit("np_rfftn_m34_s46", "[Complex<f64>]", cplx(np.fft.rfftn(m, s=(4, 6), axes=(0, 1))))
    emit("np_rfftn_m34_axes10", "[Complex<f64>]", cplx(np.fft.rfftn(m, axes=(1, 0))))
    emit("np_irfft2_f", "[f64]", real(np.fft.irfft2(f)))
    emit("np_irfftn_f_s35", "[f64]", real(np.fft.irfftn(f, s=(3, 5), axes=(0, 1))))
    c3 = (np.arange(24, dtype=float).reshape(2, 3, 4) % 5) + 1j * (np.arange(24, dtype=float).reshape(2, 3, 4) % 7 - 3)
    emit("c234", "[Complex<f64>]", cplx(c3))
    emit("np_fftn_c234", "[Complex<f64>]", cplx(np.fft.fftn(c3)))
    emit("np_fft_c234_axis1", "[Complex<f64>]", cplx(np.fft.fft(c3, axis=1)))
    emit("np_fftfreq_8", "[f64]", real(np.fft.fftfreq(8, 0.1)))
    emit("np_fftfreq_7", "[f64]", real(np.fft.fftfreq(7, 0.25)))
    emit("np_rfftfreq_8", "[f64]", real(np.fft.rfftfreq(8, 0.1)))
    emit("np_rfftfreq_7", "[f64]", real(np.fft.rfftfreq(7, 0.25)))
    a = np.arange(15).reshape(3, 5)
    emit("np_fftshift_35", "[i64]", ints(np.fft.fftshift(a)))
    emit("np_ifftshift_35", "[i64]", ints(np.fft.ifftshift(a)))
    emit("np_fftshift_35_axis1", "[i64]", ints(np.fft.fftshift(a, axes=1)))
    emit("np_ifftshift_35_axis0", "[i64]", ints(np.fft.ifftshift(a, axes=0)))


def polynomial():
    P = np.polynomial.polynomial
    C = np.polynomial.chebyshev
    L = np.polynomial.legendre
    a = np.array([1.5, -2.0, 0.25, 3.0])
    b = np.array([-0.5, 4.0, 1.0])
    xs = np.array([-1.5, -0.25, 0.0, 0.5, 2.0])
    emit("pa", "[f64]", real(a))
    emit("pb", "[f64]", real(b))
    emit("xs5", "[f64]", real(xs))
    emit("np_polyval_a", "[f64]", real(P.polyval(xs, a)))
    emit("np_polyadd", "[f64]", real(P.polyadd(a, b)))
    emit("np_polysub", "[f64]", real(P.polysub(a, b)))
    emit("np_polymul", "[f64]", real(P.polymul(a, b)))
    q, r = P.polydiv(a, b)
    emit("np_polydiv_q", "[f64]", real(q))
    emit("np_polydiv_r", "[f64]", real(r))
    emit("np_polypow3", "[f64]", real(P.polypow(b, 3)))
    emit("np_polyder", "[f64]", real(P.polyder(a)))
    emit("np_polyder2", "[f64]", real(P.polyder(a, 2)))
    emit("np_polyint", "[f64]", real(P.polyint(a)))
    emit("np_polyint_k", "[f64]", real(P.polyint(a, k=[2.5])))
    emit("np_polyfromroots", "[f64]", real(P.polyfromroots([-2.0, 0.5, 3.0])))
    emit("np_polyvander", "[f64]", real(P.polyvander(xs, 3)))
    x = np.linspace(-1.0, 1.0, 9)
    y = np.exp(x) + 0.5 * x * x
    w = np.linspace(0.5, 2.5, 9)
    emit("fit_x", "[f64]", real(x))
    emit("fit_y", "[f64]", real(y))
    emit("fit_w", "[f64]", real(w))
    emit("np_polyfit3", "[f64]", real(P.polyfit(x, y, 3)))
    emit("np_polyfit3_w", "[f64]", real(P.polyfit(x, y, 3, w=w)))
    emit("np_chebfit4", "[f64]", real(C.chebfit(x, y, 4)))
    emit("np_legfit4", "[f64]", real(L.legfit(x, y, 4)))
    cc = np.array([0.5, -1.0, 2.0, 0.25, -0.75])
    emit("cc5", "[f64]", real(cc))
    emit("np_chebval", "[f64]", real(C.chebval(xs, cc)))
    emit("np_legval", "[f64]", real(L.legval(xs, cc)))
    roots = P.polyroots([6.0, -5.0, -2.0, 1.0])
    emit("np_roots_real", "[f64]", real(roots))
    cr = P.polyroots([5.0, -6.0, 4.0, -2.0, 1.0])
    emit("np_roots_complex", "[Complex<f64>]", cplx(cr))


if __name__ == "__main__":
    {"fft": fft, "polynomial": polynomial}[sys.argv[1]]()
