/* W36: the C side of tests/c_layout_by_value.mjs. Every function takes or returns a struct by
 * value with the C ABI of the host compiler, checks each field against values written here, and
 * returns values that the Tsuzuri fixture checks. The structs and prototypes are those of the
 * reference set. */
#define TSUZURI_BY_VALUE_HOST
#include "c_layout_by_value_reference.c"

#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int64_t tz_run_takes(void);
int64_t tz_run_results(void);
int64_t tz_run_round_trips(void);
int64_t tz_run_counting(void);

/* The layout of each struct, as W21 documents it for the matching record. */
_Static_assert(sizeof(BvRgb) == 3 && sizeof(BvB5) == 5 && sizeof(BvB7) == 7 && sizeof(BvB9) == 9, "bytes");
_Static_assert(sizeof(BvPacked5) == 5 && sizeof(BvPackedF2) == 8 && sizeof(BvQuadP2) == 16, "packed");
_Static_assert(sizeof(BvVec3) == 12 && sizeof(BvF5) == 20 && sizeof(BvI3) == 12 && sizeof(BvCS) == 4, "odd");
_Static_assert(sizeof(BvDF) == 16 && sizeof(BvCD) == 16 && sizeof(BvIC) == 8 && sizeof(BvLI) == 16, "padded");
_Static_assert(sizeof(BvRectD) == 32 && sizeof(BvL3) == 24 && sizeof(BvHX) == 16, "large");

#define TAG(value) ((void *)(uintptr_t)(value))
static const uint64_t TAG_A = 0x1122334455667788u;
static const uint64_t TAG_B = 0xfedcba9876543210u;

/* Scalar helpers of the fixture (not part of the reference set). */
void *bv_make_tag(long long value) { return TAG((uint64_t)value); }
long long bv_tag_value(void *tag) { return (long long)(uintptr_t)tag; }

static int same_bits32(float value, uint32_t bits) { return memcmp(&value, &bits, 4) == 0; }
static int same_bits64(double value, uint64_t bits) { return memcmp(&value, &bits, 8) == 0; }
static float float_of(uint32_t bits) { float value; memcpy(&value, &bits, 4); return value; }
static double double_of(uint64_t bits) { double value; memcpy(&value, &bits, 8); return value; }

/* ---- arguments: the fixture passes these exact values ---- */
int bv_take_point(BvPoint v) { assert(v.x == -5 && v.y == 7); return 1; }
int bv_take_pointf(BvPointF v) { assert(v.x == 1.5f && v.y == -2.25f); return 2; }
int bv_take_rect(BvRect v) { assert(v.left == -1 && v.top == 2 && v.right == 2147483647 && v.bottom == -2147483647 - 1); return 3; }
int bv_take_rgb(BvRgb v) { assert(v.r == 255 && v.g == 0 && v.b == 128); return 4; }
int bv_take_p2d(BvP2D v) { assert(v.x == 0.1 && v.y == -1e300); return 5; }
int bv_take_vec3(BvVec3 v) { assert(v.x == 1.0f && v.y == 2.0f && v.z == 3.5f); return 6; }
int bv_take_f1(BvF1 v) { assert(v.f == -7.75f); return 7; }
int bv_take_d1(BvD1 v) { assert(v.d == 6.02214076e23); return 8; }
int bv_take_s1(BvS1 v) { assert(v.s == -32768); return 9; }
int bv_take_c1(BvC1 v) { assert(v.c == -128); return 10; }
int bv_take_di(BvDI v) { assert(v.d == 2.5 && v.i == -9223372036854775807LL - 1); return 11; }
int bv_take_df(BvDF v) { assert(v.d == -0.5 && v.f == 4.25f); return 12; }
int bv_take_b5(BvB5 v) { assert(v.b[0] == 1 && v.b[1] == 2 && v.b[2] == 3 && v.b[3] == 4 && v.b[4] == 255); return 13; }
int bv_take_b7(BvB7 v) { for (int i = 0; i < 7; i++) assert(v.b[i] == 10 * (i + 1)); return 14; }
int bv_take_b9(BvB9 v) { for (int i = 0; i < 9; i++) assert(v.b[i] == 200 + i); return 15; }
int bv_take_s3(BvS3 v) { assert(v.a == 65535 && v.b == 0 && v.c == 32768); return 16; }
int bv_take_packed5(BvPacked5 v) { assert(v.tag == 200 && v.value == -123456789); return 17; }
int bv_take_packedf2(BvPackedF2 v) { assert(v.a == 0.5f && v.b == -0.25f); return 18; }
int bv_take_quad(BvQuad v) { assert(v.v[0] == 1.0f && v.v[1] == -2.0f && v.v[2] == 3.0f && v.v[3] == -4.0f); return 19; }
int bv_take_quadp2(BvQuadP2 v) { assert(v.a[0] == 5.0f && v.a[1] == 6.0f && v.a[2] == 7.0f && v.b == 8.5f); return 20; }
int bv_take_nested3(BvNested3 v) { assert(v.p.x == 9.0f && v.p.y == 10.0f && v.z == 11.5f); return 21; }
int bv_take_f5(BvF5 v) { assert(v.a == 1.0f && v.b == 2.0f && v.c == 3.0f && v.d == 4.0f && v.e == 5.5f); return 22; }
int bv_take_l3(BvL3 v) { assert(v.a == 1 && v.b == -2 && v.c == 9223372036854775807LL); return 23; }
int bv_take_hx(BvHX v) { assert((uintptr_t)v.h == TAG_A && v.x == -9); return 24; }
int bv_take_h1(BvH1 v) { assert((uintptr_t)v.h == TAG_B); return 25; }
int bv_take_h2(BvH2 v) { assert((uintptr_t)v.h == TAG_A && v.g == NULL); return 26; }
int bv_take_rectd(BvRectD v) { assert(v.x == 10.5 && v.y == -20.25 && v.w == 640.0 && v.h == 480.0); return 27; }
int bv_take_d3(BvD3 v) { assert(v.a == 1.25 && v.b == 2.5 && v.c == -3.75); return 28; }
int bv_take_range(BvRange v) { assert(v.location == 18446744073709551615ULL && v.length == 42); return 29; }
int bv_take_cd(BvCD v) { assert(v.c == -1 && v.d == 1024.5); return 30; }
int bv_take_ic(BvIC v) { assert(v.a == 123456789 && v.b == -77); return 31; }
int bv_take_li(BvLI v) { assert(v.a == -5000000000LL && v.b == 77); return 32; }
int bv_take_i3(BvI3 v) { assert(v.a == 11 && v.b == -22 && v.c == 33); return 33; }
int bv_take_fi(BvFI v) { assert(v.f == 2.5f && v.i == -3); return 34; }
int bv_take_ffd(BvFFD v) { assert(v.a == 1.5f && v.b == 2.5f && v.c == 3.5); return 35; }
int bv_take_dff(BvDFF v) { assert(v.a == -1.5 && v.b == -2.5f && v.c == -3.5f); return 36; }
int bv_take_fffi(BvFFFI v) { assert(v.a == 0.25f && v.b == 0.5f && v.c == 0.75f && v.d == 1000000); return 37; }
int bv_take_cs(BvCS v) { assert(v.a == 127 && v.b == -300); return 38; }
int bv_take_box(BvBox v) { assert(v.origin.x == 1 && v.origin.y == 2 && v.size.x == 30 && v.size.y == 40); return 39; }
int bv_take_line(BvLine v) { assert(v.from.x == 0.5f && v.from.y == 1.5f && v.to.x == 2.5f && v.to.y == 3.5f); return 40; }

/* ---- results: every field follows from `seed`, as the fixture expects ---- */
BvPoint bv_ret_point(int seed) { BvPoint v = { seed, -seed - 1 }; return v; }
BvPointF bv_ret_pointf(int seed) { BvPointF v = { (float)seed + 0.5f, -(float)seed }; return v; }
BvRect bv_ret_rect(int seed) { BvRect v = { seed, seed + 1, seed + 2, -seed }; return v; }
BvRgb bv_ret_rgb(int seed) { BvRgb v = { (unsigned char)seed, 255, 128 }; return v; }
BvP2D bv_ret_p2d(int seed) { BvP2D v = { (double)seed + 0.125, -(double)seed * 1e10 }; return v; }
BvVec3 bv_ret_vec3(int seed) { BvVec3 v = { (float)seed, (float)seed * 2.0f, (float)seed * 3.0f }; return v; }
BvF1 bv_ret_f1(int seed) { BvF1 v = { (float)seed - 0.5f }; return v; }
BvD1 bv_ret_d1(int seed) { BvD1 v = { (double)seed / 4.0 }; return v; }
BvS1 bv_ret_s1(int seed) { BvS1 v = { (short)(-seed) }; return v; }
BvC1 bv_ret_c1(int seed) { BvC1 v = { (signed char)(-seed) }; return v; }
BvDI bv_ret_di(int seed) { BvDI v = { (double)seed + 0.5, (long long)seed * 1000000000000LL }; return v; }
BvDF bv_ret_df(int seed) { BvDF v = { -(double)seed, (float)seed + 0.25f }; return v; }
BvB5 bv_ret_b5(int seed) { BvB5 v; for (int i = 0; i < 5; i++) v.b[i] = (unsigned char)(seed + i); return v; }
BvB7 bv_ret_b7(int seed) { BvB7 v; for (int i = 0; i < 7; i++) v.b[i] = (unsigned char)(seed * 2 + i); return v; }
BvB9 bv_ret_b9(int seed) { BvB9 v; for (int i = 0; i < 9; i++) v.b[i] = (unsigned char)(250 - seed - i); return v; }
BvS3 bv_ret_s3(int seed) { BvS3 v = { (unsigned short)seed, 65535, (unsigned short)(seed * 1000) }; return v; }
BvPacked5 bv_ret_packed5(int seed) { BvPacked5 v; v.tag = (unsigned char)(seed + 100); v.value = -seed * 1000003; return v; }
BvPackedF2 bv_ret_packedf2(int seed) { BvPackedF2 v; v.a = (float)seed * 0.5f; v.b = -(float)seed * 0.25f; return v; }
BvQuad bv_ret_quad(int seed) { BvQuad v = { { (float)seed, (float)seed + 1.0f, (float)seed + 2.0f, (float)seed + 3.0f } }; return v; }
BvQuadP2 bv_ret_quadp2(int seed) { BvQuadP2 v; v.a[0] = (float)seed; v.a[1] = -(float)seed; v.a[2] = (float)seed * 2.0f; v.b = (float)seed + 0.5f; return v; }
BvNested3 bv_ret_nested3(int seed) { BvNested3 v = { { (float)seed, (float)seed + 1.0f }, (float)seed + 2.5f }; return v; }
BvF5 bv_ret_f5(int seed) { BvF5 v = { (float)seed, (float)seed + 1.0f, (float)seed + 2.0f, (float)seed + 3.0f, (float)seed + 4.5f }; return v; }
BvL3 bv_ret_l3(int seed) { BvL3 v = { seed, (long long)seed * -4000000000LL, (long long)seed + 1 }; return v; }
BvHX bv_ret_hx(int seed) { BvHX v = { TAG(TAG_A + (uint64_t)seed), -seed }; return v; }
BvH1 bv_ret_h1(int seed) { BvH1 v = { TAG(TAG_B - (uint64_t)seed) }; return v; }
BvH2 bv_ret_h2(int seed) { BvH2 v = { TAG(TAG_A), seed == 0 ? NULL : TAG((uint64_t)seed) }; return v; }
BvRectD bv_ret_rectd(int seed) { BvRectD v = { (double)seed, (double)seed + 0.5, (double)seed * 100.0, -(double)seed }; return v; }
BvD3 bv_ret_d3(int seed) { BvD3 v = { (double)seed + 0.25, (double)seed + 0.5, (double)seed + 0.75 }; return v; }
BvRange bv_ret_range(int seed) { BvRange v = { 18446744073709551615ULL - (unsigned long long)seed, (unsigned long long)seed * 3 }; return v; }
BvCD bv_ret_cd(int seed) { BvCD v; memset(&v, 0xAA, sizeof v); v.c = (signed char)(-seed); v.d = (double)seed * 1.5; return v; }
BvIC bv_ret_ic(int seed) { BvIC v; memset(&v, 0xAA, sizeof v); v.a = seed * 1000; v.b = (signed char)(-seed); return v; }
BvLI bv_ret_li(int seed) { BvLI v; memset(&v, 0xAA, sizeof v); v.a = (long long)seed * -7000000000LL; v.b = seed; return v; }
BvI3 bv_ret_i3(int seed) { BvI3 v = { seed, seed * -2, seed * 3 }; return v; }
BvFI bv_ret_fi(int seed) { BvFI v = { (float)seed + 0.75f, -seed }; return v; }
BvFFD bv_ret_ffd(int seed) { BvFFD v = { (float)seed, (float)seed + 0.5f, (double)seed + 0.25 }; return v; }
BvDFF bv_ret_dff(int seed) { BvDFF v = { (double)seed - 0.5, (float)seed, -(float)seed }; return v; }
BvFFFI bv_ret_fffi(int seed) { BvFFFI v = { (float)seed, (float)seed * 2.0f, (float)seed * 4.0f, seed * -9 }; return v; }
BvCS bv_ret_cs(int seed) { BvCS v; memset(&v, 0xAA, sizeof v); v.a = (signed char)seed; v.b = (short)(seed * -100); return v; }
BvBox bv_ret_box(int seed) { BvBox v = { { seed, seed + 1 }, { seed * 10, seed * 20 } }; return v; }
BvLine bv_ret_line(int seed) { BvLine v = { { (float)seed, (float)seed + 0.5f }, { -(float)seed, (float)seed * 8.0f } }; return v; }

/* ---- round trips ---- */
BvPointF bv_echo_pointf(BvPointF v) {
    /* seed 1: a quiet NaN with a payload and -0.0, by their bits. Otherwise the swapped fields. */
    if (v.x == 1.0f && v.y == 1.0f) { BvPointF bits = { float_of(0x7fc12345u), float_of(0x80000000u) }; return bits; }
    BvPointF swapped = { v.y, v.x };
    return swapped;
}
BvP2D bv_echo_p2d(BvP2D v) {
    if (v.x == 1.0 && v.y == 1.0) { BvP2D bits = { double_of(0x7ff8000000abcdefull), double_of(0x8000000000000000ull) }; return bits; }
    BvP2D swapped = { v.y, v.x };
    return swapped;
}
int bv_check_bits_pointf(BvPointF v) { return same_bits32(v.x, 0x7fc12345u) && same_bits32(v.y, 0x80000000u); }
int bv_check_bits_p2d(BvP2D v) { return same_bits64(v.x, 0x7ff8000000abcdefull) && same_bits64(v.y, 0x8000000000000000ull); }
BvRectD bv_scale_rectd(BvRectD v, double factor) {
    BvRectD scaled = { v.x * factor, v.y * factor, v.w * factor, v.h * factor };
    v.x = v.y = v.w = v.h = -1.0;
    return scaled;
}
BvL3 bv_rotate_l3(BvL3 v) {
    BvL3 rotated = { v.b, v.c, v.a };
    v.a = v.b = v.c = -1;
    return rotated;
}
BvRect bv_offset_rect(BvRect v, BvPoint by) {
    BvRect moved = { v.left + by.x, v.top + by.y, v.right + by.x, v.bottom + by.y };
    return moved;
}

/* ---- a callee writes to its copies ---- */
int bv_scribble_l3(BvL3 a, BvL3 b) {
    assert(a.a == 100 && a.b == 200 && a.c == 300);
    assert(b.a == 100 && b.b == 200 && b.c == 300);
    /* Two arguments made from one value are two copies. */
    a.a = 1; a.b = 2; a.c = 3;
    assert(b.a == 100 && b.b == 200 && b.c == 300);
    b.a = -1; b.b = -2; b.c = -3;
    assert(a.a == 1 && a.b == 2 && a.c == 3);
    return 41;
}
int bv_scribble_small(BvPoint a, BvRgb b, BvRect c) {
    assert(a.x == 3 && a.y == 4);
    assert(b.r == 9 && b.g == 8 && b.b == 7);
    assert(c.left == 1 && c.top == 2 && c.right == 3 && c.bottom == 4);
    a.x = a.y = -1; b.r = b.g = b.b = 0; c.left = c.top = c.right = c.bottom = -1;
    return 42 + a.x + b.r + c.left + 2;
}

/* ---- registers over the whole call ---- */
int bv_five_then_rect(long long a, long long b, long long c, long long d, long long e, BvRect r, long long f) {
    assert(a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6);
    assert(r.left == 10 && r.top == 20 && r.right == 30 && r.bottom == 40);
    return 43;
}
int bv_seven_doubles_then_p2d(double a, double b, double c, double d, double e, double f, double g, BvP2D p, double h) {
    assert(a == 1.0 && b == 2.0 && c == 3.0 && d == 4.0 && e == 5.0 && f == 6.0 && g == 7.0 && h == 8.0);
    assert(p.x == 0.5 && p.y == -0.5);
    return 44;
}
int bv_six_then_small(long long a, long long b, long long c, long long d, long long e, long long f,
                      BvPoint p, BvPacked5 q, BvPointF r) {
    assert(a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6);
    assert(p.x == 7 && p.y == 8);
    assert(q.tag == 9 && q.value == 10);
    assert(r.x == 11.0f && r.y == 12.0f);
    return 45;
}
int bv_sse_exhausted(double a, double b, double c, double d, double e, double f, double g,
                     BvPointF p, BvPointF q, double h, BvPoint r) {
    assert(a == 1.0 && b == 2.0 && c == 3.0 && d == 4.0 && e == 5.0 && f == 6.0 && g == 7.0 && h == 8.0);
    assert(p.x == 1.5f && p.y == 2.5f && q.x == 3.5f && q.y == 4.5f);
    assert(r.x == -1 && r.y == -2);
    return 46;
}
BvL3 bv_sret_counts(long long a, long long b, long long c, long long d, BvRect r) {
    assert(a == 1 && b == 2 && c == 3 && d == 4);
    BvL3 v = { r.left + a, r.top + b, (long long)r.right * r.bottom + c + d };
    return v;
}
long long bv_many(int a, BvPoint p, double b, BvPointF f, BvRect r, long long c, BvVec3 v, BvRgb g, BvP2D d) {
    assert(a == 1 && p.x == 2 && p.y == 3 && b == 4.5 && f.x == 5.5f && f.y == 6.5f);
    assert(r.left == 7 && r.top == 8 && r.right == 9 && r.bottom == 10 && c == 11);
    assert(v.x == 12.5f && v.y == 13.5f && v.z == 14.5f);
    assert(g.r == 15 && g.g == 16 && g.b == 17 && d.x == 18.5 && d.y == 19.5);
    return 4700000000LL;
}
double bv_hfa_spill(BvRectD a, BvRectD b, BvRectD c, BvP2D d, double e) {
    assert(a.x == 1.0 && a.y == 2.0 && a.w == 3.0 && a.h == 4.0);
    assert(b.x == 5.0 && b.y == 6.0 && b.w == 7.0 && b.h == 8.0);
    assert(c.x == 9.0 && c.y == 10.0 && c.w == 11.0 && c.h == 12.0);
    assert(d.x == 13.0 && d.y == 14.0 && e == 15.0);
    return 48.5;
}
long long bv_gpr_spill(long long a, long long b, long long c, long long d, long long e, long long f, long long g,
                       BvRect r, int z) {
    assert(a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6 && g == 7);
    assert(r.left == 8 && r.top == 9 && r.right == 10 && r.bottom == 11 && z == 12);
    return 49;
}
int bv_stack_structs(BvPoint a, BvPoint b, BvPoint c, BvPoint d, BvPoint e, BvRect f, BvPointF g) {
    assert(a.x == 1 && a.y == 2 && b.x == 3 && b.y == 4 && c.x == 5 && c.y == 6 && d.x == 7 && d.y == 8);
    assert(e.x == 9 && e.y == 10 && f.left == 11 && f.top == 12 && f.right == 13 && f.bottom == 14);
    assert(g.x == 15.5f && g.y == 16.5f);
    return 50;
}
int bv_small_on_stack(long long a, long long b, long long c, long long d, long long e, long long f, long long g, long long h,
                      BvPoint p, BvRgb q, BvS1 r, BvC1 s, int z) {
    assert(a == 1 && b == 2 && c == 3 && d == 4 && e == 5 && f == 6 && g == 7 && h == 8);
    assert(p.x == 9 && p.y == -10 && q.r == 11 && q.g == 12 && q.b == 255 && r.s == -13 && s.c == -14 && z == 15);
    return 51;
}
int bv_float_stack(float a, float b, float c, float d, float e, float f, float g, float h,
                   float i, BvPointF p, float j, BvVec3 v, BvF1 w) {
    assert(a == 1.0f && b == 2.0f && c == 3.0f && d == 4.0f && e == 5.0f && f == 6.0f && g == 7.0f && h == 8.0f);
    assert(i == 9.5f && p.x == 10.5f && p.y == 11.5f && j == 12.5f);
    assert(v.x == 13.5f && v.y == 14.5f && v.z == 15.5f && w.f == 16.5f);
    return 52;
}

int main(void) {
    int64_t failed = tz_run_takes();
    if (failed != 0) { fprintf(stderr, "by-value argument check %lld failed\n", (long long)failed); return 1; }
    failed = tz_run_results();
    if (failed != 0) { fprintf(stderr, "by-value result check %lld failed\n", (long long)failed); return 2; }
    failed = tz_run_round_trips();
    if (failed != 0) { fprintf(stderr, "by-value round trip check %lld failed\n", (long long)failed); return 3; }
    failed = tz_run_counting();
    if (failed != 0) { fprintf(stderr, "by-value register counting check %lld failed\n", (long long)failed); return 4; }
    return 0;
}
