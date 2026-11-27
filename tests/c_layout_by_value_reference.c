/* W36: the reference set of C structs passed and returned by value.
 *
 * This file is plain C without any #include, so that Clang can lower it for every target without
 * a sysroot: `clang --target=<triple> -O1 -S -emit-llvm -ffreestanding` prints one `declare` per
 * function below, and that line is the independent reference for the C ABI of the target.
 * `tests/c_layout_by_value.mjs` compares it with the `declare` that Tsuzuri emits for the same
 * prototype in `tests/fixtures/c_layout_by_value/Main.tz`. `tests/c_layout_by_value_host.c` includes
 * this file for the struct definitions and the prototypes, and defines the functions.
 *
 * Only types whose size is the same on every supported target appear (no `long`): `signed char`
 * and `unsigned char` (i8, i8u), `short` (i16), `int` (i32), `long long` (i64), `float`, `double`
 * and `void *`. Scalar parameters are `int`, `long long`, `float` and `double` only.
 */
#ifndef TSUZURI_BY_VALUE_REFERENCE
#define TSUZURI_BY_VALUE_REFERENCE

typedef struct BvPoint { int x, y; } BvPoint;                              /* 8: POINT */
typedef struct BvPointF { float x, y; } BvPointF;                          /* 8: D2D1_POINT_2F */
typedef struct BvRect { int left, top, right, bottom; } BvRect;            /* 16: RECT */
typedef struct BvRgb { unsigned char r, g, b; } BvRgb;                     /* 3 */
typedef struct BvP2D { double x, y; } BvP2D;                               /* 16: NSPoint, NSSize */
typedef struct BvVec3 { float x, y, z; } BvVec3;                           /* 12 */
typedef struct BvF1 { float f; } BvF1;                                     /* 4 */
typedef struct BvD1 { double d; } BvD1;                                    /* 8 */
typedef struct BvS1 { short s; } BvS1;                                     /* 2 */
typedef struct BvC1 { signed char c; } BvC1;                               /* 1 */
typedef struct BvDI { double d; long long i; } BvDI;                       /* 16 */
typedef struct BvDF { double d; float f; } BvDF;                           /* 16 */
typedef struct BvB5 { unsigned char b[5]; } BvB5;                          /* 5 */
typedef struct BvB7 { unsigned char b[7]; } BvB7;                          /* 7 */
typedef struct BvB9 { unsigned char b[9]; } BvB9;                          /* 9 */
typedef struct BvS3 { unsigned short a, b, c; } BvS3;                      /* 6 */
#pragma pack(push, 1)
typedef struct BvPacked5 { unsigned char tag; int value; } BvPacked5;     /* 5, unaligned field */
typedef struct BvPackedF2 { float a, b; } BvPackedF2;                      /* 8, aligned fields */
#pragma pack(pop)
typedef struct BvQuad { float v[4]; } BvQuad;                              /* 16 */
#pragma pack(push, 2)
typedef struct BvQuadP2 { float a[3]; float b; } BvQuadP2;                 /* 16 */
#pragma pack(pop)
typedef struct BvNested3 { BvPointF p; float z; } BvNested3;               /* 12 */
typedef struct BvF5 { float a, b, c, d, e; } BvF5;                         /* 20 */
typedef struct BvL3 { long long a, b, c; } BvL3;                           /* 24 */
typedef struct BvHX { void *h; int x; } BvHX;                              /* 16 */
typedef struct BvH1 { void *h; } BvH1;                                     /* 8 */
typedef struct BvH2 { void *h; void *g; } BvH2;                            /* 16 */
typedef struct BvRectD { double x, y, w, h; } BvRectD;                     /* 32: NSRect */
typedef struct BvD3 { double a, b, c; } BvD3;                              /* 24 */
typedef struct BvRange { unsigned long long location, length; } BvRange;  /* 16: NSRange */
typedef struct BvCD { signed char c; double d; } BvCD;                     /* 16 */
typedef struct BvIC { int a; signed char b; } BvIC;                        /* 8 */
typedef struct BvLI { long long a; int b; } BvLI;                          /* 16 */
typedef struct BvI3 { int a, b, c; } BvI3;                                 /* 12 */
typedef struct BvFI { float f; int i; } BvFI;                              /* 8 */
typedef struct BvFFD { float a, b; double c; } BvFFD;                      /* 16 */
typedef struct BvDFF { double a; float b, c; } BvDFF;                      /* 16 */
typedef struct BvFFFI { float a, b, c; int d; } BvFFFI;                    /* 16 */
typedef struct BvCS { signed char a; short b; } BvCS;                      /* 4 */
typedef struct BvBox { BvPoint origin; BvPoint size; } BvBox;              /* 16, nested */
typedef struct BvLine { BvPointF from; BvPointF to; } BvLine;              /* 16, nested HFA */

int bv_take_point(BvPoint v);          BvPoint bv_ret_point(int seed);
int bv_take_pointf(BvPointF v);        BvPointF bv_ret_pointf(int seed);
int bv_take_rect(BvRect v);            BvRect bv_ret_rect(int seed);
int bv_take_rgb(BvRgb v);              BvRgb bv_ret_rgb(int seed);
int bv_take_p2d(BvP2D v);              BvP2D bv_ret_p2d(int seed);
int bv_take_vec3(BvVec3 v);            BvVec3 bv_ret_vec3(int seed);
int bv_take_f1(BvF1 v);                BvF1 bv_ret_f1(int seed);
int bv_take_d1(BvD1 v);                BvD1 bv_ret_d1(int seed);
int bv_take_s1(BvS1 v);                BvS1 bv_ret_s1(int seed);
int bv_take_c1(BvC1 v);                BvC1 bv_ret_c1(int seed);
int bv_take_di(BvDI v);                BvDI bv_ret_di(int seed);
int bv_take_df(BvDF v);                BvDF bv_ret_df(int seed);
int bv_take_b5(BvB5 v);                BvB5 bv_ret_b5(int seed);
int bv_take_b7(BvB7 v);                BvB7 bv_ret_b7(int seed);
int bv_take_b9(BvB9 v);                BvB9 bv_ret_b9(int seed);
int bv_take_s3(BvS3 v);                BvS3 bv_ret_s3(int seed);
int bv_take_packed5(BvPacked5 v);      BvPacked5 bv_ret_packed5(int seed);
int bv_take_packedf2(BvPackedF2 v);    BvPackedF2 bv_ret_packedf2(int seed);
int bv_take_quad(BvQuad v);            BvQuad bv_ret_quad(int seed);
int bv_take_quadp2(BvQuadP2 v);        BvQuadP2 bv_ret_quadp2(int seed);
int bv_take_nested3(BvNested3 v);      BvNested3 bv_ret_nested3(int seed);
int bv_take_f5(BvF5 v);                BvF5 bv_ret_f5(int seed);
int bv_take_l3(BvL3 v);                BvL3 bv_ret_l3(int seed);
int bv_take_hx(BvHX v);                BvHX bv_ret_hx(int seed);
int bv_take_h1(BvH1 v);                BvH1 bv_ret_h1(int seed);
int bv_take_h2(BvH2 v);                BvH2 bv_ret_h2(int seed);
int bv_take_rectd(BvRectD v);          BvRectD bv_ret_rectd(int seed);
int bv_take_d3(BvD3 v);                BvD3 bv_ret_d3(int seed);
int bv_take_range(BvRange v);          BvRange bv_ret_range(int seed);
int bv_take_cd(BvCD v);                BvCD bv_ret_cd(int seed);
int bv_take_ic(BvIC v);                BvIC bv_ret_ic(int seed);
int bv_take_li(BvLI v);                BvLI bv_ret_li(int seed);
int bv_take_i3(BvI3 v);                BvI3 bv_ret_i3(int seed);
int bv_take_fi(BvFI v);                BvFI bv_ret_fi(int seed);
int bv_take_ffd(BvFFD v);              BvFFD bv_ret_ffd(int seed);
int bv_take_dff(BvDFF v);              BvDFF bv_ret_dff(int seed);
int bv_take_fffi(BvFFFI v);            BvFFFI bv_ret_fffi(int seed);
int bv_take_cs(BvCS v);                BvCS bv_ret_cs(int seed);
int bv_take_box(BvBox v);              BvBox bv_ret_box(int seed);
int bv_take_line(BvLine v);            BvLine bv_ret_line(int seed);

/* Round trips: the argument and the result of one call, and the bits of a NaN and of -0.0. */
BvPointF bv_echo_pointf(BvPointF v);
BvP2D bv_echo_p2d(BvP2D v);
BvRectD bv_scale_rectd(BvRectD v, double factor);
BvL3 bv_rotate_l3(BvL3 v);
BvRect bv_offset_rect(BvRect v, BvPoint by);
int bv_check_bits_pointf(BvPointF v);
int bv_check_bits_p2d(BvP2D v);

/* A callee may write to the copy that it receives in memory. */
int bv_scribble_l3(BvL3 a, BvL3 b);
int bv_scribble_small(BvPoint a, BvRgb b, BvRect c);

/* How registers are counted over a whole call. On System V a struct that does not fit in the
   registers that are left goes to memory as a whole, and the scalars after it still use registers. */
int bv_five_then_rect(long long a, long long b, long long c, long long d, long long e, BvRect r, long long f);
int bv_seven_doubles_then_p2d(double a, double b, double c, double d, double e, double f, double g, BvP2D p, double h);
int bv_six_then_small(long long a, long long b, long long c, long long d, long long e, long long f,
                      BvPoint p, BvPacked5 q, BvPointF r);
int bv_sse_exhausted(double a, double b, double c, double d, double e, double f, double g,
                     BvPointF p, BvPointF q, double h, BvPoint r);
BvL3 bv_sret_counts(long long a, long long b, long long c, long long d, BvRect r);
long long bv_many(int a, BvPoint p, double b, BvPointF f, BvRect r, long long c, BvVec3 v, BvRgb g, BvP2D d);
/* AAPCS64: an HFA that does not fit in d0-d7 and an aggregate that does not fit in x0-x7 go to the stack. */
double bv_hfa_spill(BvRectD a, BvRectD b, BvRectD c, BvP2D d, double e);
long long bv_gpr_spill(long long a, long long b, long long c, long long d, long long e, long long f, long long g,
                       BvRect r, int z);
/* Windows x64: the fifth and later arguments are on the stack. */
int bv_stack_structs(BvPoint a, BvPoint b, BvPoint c, BvPoint d, BvPoint e, BvRect f, BvPointF g);
/* Small structs after every integer register is used: each is on the stack. */
int bv_small_on_stack(long long a, long long b, long long c, long long d, long long e, long long f, long long g, long long h,
                      BvPoint p, BvRgb q, BvS1 r, BvC1 s, int z);

#ifndef TSUZURI_BY_VALUE_HOST
/* Naming every function makes Clang print its `declare`. */
typedef void (*BvAny)(void);
BvAny const bv_reference[] = {
    (BvAny)bv_take_point, (BvAny)bv_ret_point, (BvAny)bv_take_pointf, (BvAny)bv_ret_pointf,
    (BvAny)bv_take_rect, (BvAny)bv_ret_rect, (BvAny)bv_take_rgb, (BvAny)bv_ret_rgb,
    (BvAny)bv_take_p2d, (BvAny)bv_ret_p2d, (BvAny)bv_take_vec3, (BvAny)bv_ret_vec3,
    (BvAny)bv_take_f1, (BvAny)bv_ret_f1, (BvAny)bv_take_d1, (BvAny)bv_ret_d1,
    (BvAny)bv_take_s1, (BvAny)bv_ret_s1, (BvAny)bv_take_c1, (BvAny)bv_ret_c1,
    (BvAny)bv_take_di, (BvAny)bv_ret_di, (BvAny)bv_take_df, (BvAny)bv_ret_df,
    (BvAny)bv_take_b5, (BvAny)bv_ret_b5, (BvAny)bv_take_b7, (BvAny)bv_ret_b7,
    (BvAny)bv_take_b9, (BvAny)bv_ret_b9, (BvAny)bv_take_s3, (BvAny)bv_ret_s3,
    (BvAny)bv_take_packed5, (BvAny)bv_ret_packed5, (BvAny)bv_take_packedf2, (BvAny)bv_ret_packedf2,
    (BvAny)bv_take_quad, (BvAny)bv_ret_quad, (BvAny)bv_take_quadp2, (BvAny)bv_ret_quadp2,
    (BvAny)bv_take_nested3, (BvAny)bv_ret_nested3, (BvAny)bv_take_f5, (BvAny)bv_ret_f5,
    (BvAny)bv_take_l3, (BvAny)bv_ret_l3, (BvAny)bv_take_hx, (BvAny)bv_ret_hx,
    (BvAny)bv_take_h1, (BvAny)bv_ret_h1, (BvAny)bv_take_h2, (BvAny)bv_ret_h2,
    (BvAny)bv_take_rectd, (BvAny)bv_ret_rectd, (BvAny)bv_take_d3, (BvAny)bv_ret_d3,
    (BvAny)bv_take_range, (BvAny)bv_ret_range, (BvAny)bv_take_cd, (BvAny)bv_ret_cd,
    (BvAny)bv_take_ic, (BvAny)bv_ret_ic, (BvAny)bv_take_li, (BvAny)bv_ret_li,
    (BvAny)bv_take_i3, (BvAny)bv_ret_i3, (BvAny)bv_take_fi, (BvAny)bv_ret_fi,
    (BvAny)bv_take_ffd, (BvAny)bv_ret_ffd, (BvAny)bv_take_dff, (BvAny)bv_ret_dff,
    (BvAny)bv_take_fffi, (BvAny)bv_ret_fffi, (BvAny)bv_take_cs, (BvAny)bv_ret_cs,
    (BvAny)bv_take_box, (BvAny)bv_ret_box, (BvAny)bv_take_line, (BvAny)bv_ret_line,
    (BvAny)bv_echo_pointf, (BvAny)bv_echo_p2d, (BvAny)bv_scale_rectd, (BvAny)bv_rotate_l3,
    (BvAny)bv_offset_rect, (BvAny)bv_check_bits_pointf, (BvAny)bv_check_bits_p2d,
    (BvAny)bv_scribble_l3, (BvAny)bv_scribble_small,
    (BvAny)bv_five_then_rect, (BvAny)bv_seven_doubles_then_p2d, (BvAny)bv_six_then_small,
    (BvAny)bv_sse_exhausted, (BvAny)bv_sret_counts, (BvAny)bv_many, (BvAny)bv_hfa_spill,
    (BvAny)bv_gpr_spill, (BvAny)bv_stack_structs, (BvAny)bv_small_on_stack,
};
#endif

#endif
