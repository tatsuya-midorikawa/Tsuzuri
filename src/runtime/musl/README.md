# Portable Math Sources

Selected unmodified math sources from musl 1.2.5:
https://musl.libc.org/releases/musl-1.2.5.tar.gz

SHA-256: `a9a118bbe84d8764da0ea0d28b3ab3fae8477fc7e4085d90102b8596fc7c75e4`

COPYRIGHT and source notices retain the upstream licenses. Tsuzuri's adapter
headers are in `../math-include/`. `../generate_math.py` compiles the sources
without FMA, fast math, host libm, or excess precision and links a portable IR
module. The generator needs Clang and llvm-link; Cargo builds only include the
checked-in IR. No user code depends on musl's C ABI or a system musl installation.

Verified regeneration: Apple Clang 21 with LLVM 21 `llvm-link`. Newer LLVM
versions change literal syntax and intrinsic signatures.
Set `TSUZURI_CLANG` and `TSUZURI_LLVM_LINK` explicitly when regenerating.

`../math.c` refines ordinary finite binary64 atan2 with compensated division,
range reduction to |r| <= 1/8, and a degree-31 series. The unmodified upstream
atan2 handles special values and extreme ratios. Its original ordinary path
exceeded the 1-ulp contract in reference testing.