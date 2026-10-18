#![cfg(all(feature = "fast-backend", unix))]

//! Reference test of the fast debug backend (PB05): one function per operation and operand type,
//! compiled once by Clang from the IR text and once by the translator, and called on the same
//! boundary inputs by a C host. The operations must agree bit for bit: wrap-around, odd widths,
//! 128-bit integers, shifts, division, saturation, overflow flags, NaN, signed zero, rounding and
//! the lanes of vectors.
//!
//! Operations that the translator refuses (E2002) are dropped from the comparison, and the list of
//! dropped ones is pinned in `KNOWN_REFUSALS`, so a new gap and a closed gap both show.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use tsuzuri::backend::{self, Options};

fn clang() -> String {
    std::env::var("TSUZURI_CLANG").unwrap_or_else(|_| "clang".to_owned())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Int,
    Float,
}

struct Case {
    id: usize,
    group: String,
    label: String,
    kind: Kind,
    declarations: Vec<String>,
    body: String,
}

#[derive(Default)]
struct Suite {
    cases: Vec<Case>,
}

impl Suite {
    fn add(
        &mut self,
        group: &str,
        label: String,
        kind: Kind,
        declarations: Vec<String>,
        body: String,
    ) {
        let id = self.cases.len();
        self.cases.push(Case {
            id,
            group: group.to_owned(),
            label,
            kind,
            declarations,
            body,
        });
    }
}

/// The multiplier that folds the second half of a wide result into one 64-bit value.
const MIX: i64 = -7046029254386353131;

fn constant(width: u32, value: i128) -> String {
    if width == 1 {
        (if value & 1 == 1 { "true" } else { "false" }).to_owned()
    } else {
        value.to_string()
    }
}

// ---- integers ----

fn int_prelude(width: u32) -> String {
    match width {
        64 => "  %x = add i64 %a, 0\n  %y = add i64 %b, 0\n".to_owned(),
        128 => "  %ah = zext i64 %a to i128\n  %bh = zext i64 %b to i128\n  %as = shl i128 %ah, 64\n  %x = or i128 %as, %bh\n  %bs = shl i128 %bh, 64\n  %ak = xor i64 %a, 6148914691236517205\n  %akh = zext i64 %ak to i128\n  %y = or i128 %bs, %akh\n".to_owned(),
        _ => format!("  %x = trunc i64 %a to i{width}\n  %y = trunc i64 %b to i{width}\n"),
    }
}

/// Defines `%o`, the value as 64 bits.
fn int_fold(width: u32, value: &str) -> String {
    match width {
        64 => format!("  %o = add i64 {value}, 0\n"),
        128 => format!(
            "  %flo = trunc i128 {value} to i64\n  %fhs = lshr i128 {value}, 64\n  %fhi = trunc i128 %fhs to i64\n  %fmx = mul i64 %fhi, {MIX}\n  %o = add i64 %flo, %fmx\n"
        ),
        _ => format!("  %o = zext i{width} {value} to i64\n"),
    }
}

fn int_cases(suite: &mut Suite, width: u32) {
    let ty = format!("i{width}");
    let group = ty.clone();
    let prelude = int_prelude(width);
    let one = constant(width, 1);
    let add = |suite: &mut Suite,
               name: &str,
               declarations: Vec<String>,
               compute: String,
               result: &str| {
        let body = format!(
            "{prelude}{compute}{}  ret i64 %o\n",
            int_fold(width, result)
        );
        suite.add(
            &group,
            format!("{ty} {name}"),
            Kind::Int,
            declarations,
            body,
        );
    };
    for op in ["add", "sub", "mul", "and", "or", "xor"] {
        add(
            suite,
            op,
            vec![],
            format!("  %r = {op} {ty} %x, %y\n"),
            "%r",
        );
    }
    if width >= 2 {
        let amount = if width.is_power_of_two() {
            format!("  %s = and {ty} %y, {}\n", width - 1)
        } else {
            format!("  %s = urem {ty} %y, {width}\n")
        };
        for op in ["shl", "lshr", "ashr"] {
            add(
                suite,
                op,
                vec![],
                format!("{amount}  %r = {op} {ty} %x, %s\n"),
                "%r",
            );
        }
    }
    let divisor = format!("  %z = icmp eq {ty} %y, 0\n  %d = select i1 %z, {ty} {one}, {ty} %y\n");
    for op in ["udiv", "urem"] {
        add(
            suite,
            op,
            vec![],
            format!("{divisor}  %r = {op} {ty} %x, %d\n"),
            "%r",
        );
    }
    let minimum = if width == 128 {
        i128::MIN
    } else {
        -(1i128 << (width - 1))
    };
    let signed_divisor = format!(
        "{divisor}  %m = icmp eq {ty} %x, {min}\n  %n = icmp eq {ty} %d, {neg}\n  %bad = and i1 %m, %n\n  %e = select i1 %bad, {ty} {one}, {ty} %d\n",
        min = constant(width, minimum),
        neg = constant(width, -1)
    );
    for op in ["sdiv", "srem"] {
        add(
            suite,
            op,
            vec![],
            format!("{signed_divisor}  %r = {op} {ty} %x, %e\n"),
            "%r",
        );
    }
    add(
        suite,
        "select slt",
        vec![],
        format!("  %c = icmp slt {ty} %x, %y\n  %r = select i1 %c, {ty} %x, {ty} %y\n"),
        "%r",
    );
    for predicate in [
        "eq", "ne", "ugt", "uge", "ult", "ule", "sgt", "sge", "slt", "sle",
    ] {
        suite.add(
            &group,
            format!("{ty} icmp {predicate}"),
            Kind::Int,
            vec![],
            format!("{prelude}  %c = icmp {predicate} {ty} %x, %y\n  %o = zext i1 %c to i64\n  ret i64 %o\n"),
        );
    }
    // Conversions.
    if width < 64 {
        for op in ["sext", "zext"] {
            suite.add(
                &group,
                format!("{ty} {op} to i64"),
                Kind::Int,
                vec![],
                format!("{prelude}  %o = {op} {ty} %x to i64\n  ret i64 %o\n"),
            );
            suite.add(
                &group,
                format!("{ty} {op} to i128"),
                Kind::Int,
                vec![],
                format!(
                    "{prelude}  %w = {op} {ty} %x to i128\n{}  ret i64 %o\n",
                    int_fold(128, "%w")
                ),
            );
        }
    }
    if width == 128 {
        for target in [1u32, 8, 24, 32, 64] {
            suite.add(
                &group,
                format!("{ty} trunc to i{target}"),
                Kind::Int,
                vec![],
                format!(
                    "{prelude}  %t = trunc i128 %x to i{target}\n{}  ret i64 %o\n",
                    int_fold(target, "%t")
                ),
            );
        }
    }
    // Intrinsics: only on the widths of the machine, the ones that the compiler's IR uses.
    if !matches!(width, 8 | 16 | 32 | 64 | 128) {
        return;
    }
    let unary = |name: &str| format!("declare {ty} @llvm.{name}.{ty}({ty})");
    let flagged = |name: &str| format!("declare {ty} @llvm.{name}.{ty}({ty}, i1)");
    let binary = |name: &str| format!("declare {ty} @llvm.{name}.{ty}({ty}, {ty})");
    for name in ["ctpop", "bitreverse"] {
        add(
            suite,
            name,
            vec![unary(name)],
            format!("  %r = call {ty} @llvm.{name}.{ty}({ty} %x)\n"),
            "%r",
        );
    }
    if width >= 16 && width % 16 == 0 {
        add(
            suite,
            "bswap",
            vec![unary("bswap")],
            format!("  %r = call {ty} @llvm.bswap.{ty}({ty} %x)\n"),
            "%r",
        );
    }
    for name in ["ctlz", "cttz", "abs"] {
        add(
            suite,
            name,
            vec![flagged(name)],
            format!("  %r = call {ty} @llvm.{name}.{ty}({ty} %x, i1 false)\n"),
            "%r",
        );
    }
    for name in [
        "smin", "smax", "umin", "umax", "sadd.sat", "uadd.sat", "ssub.sat", "usub.sat",
    ] {
        add(
            suite,
            name,
            vec![binary(name)],
            format!("  %r = call {ty} @llvm.{name}.{ty}({ty} %x, {ty} %y)\n"),
            "%r",
        );
    }
    for name in ["fshl", "fshr"] {
        add(
            suite,
            name,
            vec![format!("declare {ty} @llvm.{name}.{ty}({ty}, {ty}, {ty})")],
            format!(
                "  %z = xor {ty} %x, %y\n  %r = call {ty} @llvm.{name}.{ty}({ty} %x, {ty} %y, {ty} %z)\n"
            ),
            "%r",
        );
    }
    for name in ["scmp", "ucmp"] {
        suite.add(
            &group,
            format!("{ty} {name}"),
            Kind::Int,
            vec![format!("declare i8 @llvm.{name}.i8.{ty}({ty}, {ty})")],
            format!("{prelude}  %r = call i8 @llvm.{name}.i8.{ty}({ty} %x, {ty} %y)\n  %o = zext i8 %r to i64\n  ret i64 %o\n"),
        );
    }
    for name in ["sadd", "uadd", "ssub", "usub", "smul", "umul"] {
        suite.add(
            &group,
            format!("{ty} {name}.with.overflow"),
            Kind::Int,
            vec![format!(
                "declare {{ {ty}, i1 }} @llvm.{name}.with.overflow.{ty}({ty}, {ty})"
            )],
            format!(
                "{prelude}  %s = call {{ {ty}, i1 }} @llvm.{name}.with.overflow.{ty}({ty} %x, {ty} %y)\n  %v = extractvalue {{ {ty}, i1 }} %s, 0\n  %f = extractvalue {{ {ty}, i1 }} %s, 1\n{}  %fz = zext i1 %f to i64\n  %fm = mul i64 %fz, 1311768467463790320\n  %p = add i64 %o, %fm\n  ret i64 %p\n",
                int_fold(width, "%v")
            ),
        );
    }
}

// ---- floating point ----

fn float_prelude(ty: &str) -> String {
    if ty == "double" {
        "  %x = bitcast i64 %a to double\n  %y = bitcast i64 %b to double\n".to_owned()
    } else {
        "  %xd = bitcast i64 %a to double\n  %yd = bitcast i64 %b to double\n  %x = fptrunc double %xd to float\n  %y = fptrunc double %yd to float\n".to_owned()
    }
}

/// Defines `%o`, the value's bits as 64 bits.
fn float_fold(ty: &str, value: &str) -> String {
    if ty == "double" {
        format!("  %o = bitcast double {value} to i64\n")
    } else {
        format!("  %ob = bitcast float {value} to i32\n  %o = zext i32 %ob to i64\n")
    }
}

fn float_cases(suite: &mut Suite, ty: &str) {
    let suffix = if ty == "double" { "f64" } else { "f32" };
    let group = ty.to_owned();
    let prelude = float_prelude(ty);
    let add = |suite: &mut Suite,
               name: &str,
               declarations: Vec<String>,
               compute: String,
               result: &str| {
        let body = format!("{prelude}{compute}{}  ret i64 %o\n", float_fold(ty, result));
        suite.add(
            &group,
            format!("{ty} {name}"),
            Kind::Float,
            declarations,
            body,
        );
    };
    for op in ["fadd", "fsub", "fmul", "fdiv", "frem"] {
        add(
            suite,
            op,
            vec![],
            format!("  %r = {op} {ty} %x, %y\n"),
            "%r",
        );
    }
    add(
        suite,
        "fneg",
        vec![],
        format!("  %r = fneg {ty} %x\n"),
        "%r",
    );
    for (name, predicate) in [("select olt", "olt"), ("select ugt", "ugt")] {
        add(
            suite,
            name,
            vec![],
            format!("  %c = fcmp {predicate} {ty} %x, %y\n  %r = select i1 %c, {ty} %x, {ty} %y\n"),
            "%r",
        );
    }
    for predicate in [
        "false", "oeq", "ogt", "oge", "olt", "ole", "one", "ord", "ueq", "ugt", "uge", "ult",
        "ule", "une", "uno", "true",
    ] {
        suite.add(
            &group,
            format!("{ty} fcmp {predicate}"),
            Kind::Float,
            vec![],
            format!("{prelude}  %c = fcmp {predicate} {ty} %x, %y\n  %o = zext i1 %c to i64\n  ret i64 %o\n"),
        );
    }
    for name in [
        "fabs",
        "sqrt",
        "floor",
        "ceil",
        "trunc",
        "rint",
        "nearbyint",
        "roundeven",
    ] {
        add(
            suite,
            name,
            vec![format!("declare {ty} @llvm.{name}.{suffix}({ty})")],
            format!("  %r = call {ty} @llvm.{name}.{suffix}({ty} %x)\n"),
            "%r",
        );
    }
    for name in ["copysign", "minimum", "maximum"] {
        // The payload and sign of a NaN that `minimum` or `maximum` returns differ between code
        // generators (the language only promises that a NaN propagates), so those results are
        // compared as one NaN.
        let canonical = if name == "copysign" {
            String::new()
        } else {
            format!(
                "  %nan = fcmp uno {ty} %r, %r\n  %q = select i1 %nan, {ty} 0x7FF8000000000000, {ty} %r\n"
            )
        };
        add(
            suite,
            name,
            vec![format!("declare {ty} @llvm.{name}.{suffix}({ty}, {ty})")],
            format!("  %r = call {ty} @llvm.{name}.{suffix}({ty} %x, {ty} %y)\n{canonical}"),
            if name == "copysign" { "%r" } else { "%q" },
        );
    }
    for name in ["fma", "fmuladd"] {
        add(
            suite,
            name,
            vec![format!(
                "declare {ty} @llvm.{name}.{suffix}({ty}, {ty}, {ty})"
            )],
            format!("  %r = call {ty} @llvm.{name}.{suffix}({ty} %x, {ty} %y, {ty} %x)\n"),
            "%r",
        );
    }
    for mask in [1023, 3, 516, 96, 144, 264, 448] {
        suite.add(
            &group,
            format!("{ty} is.fpclass {mask}"),
            Kind::Float,
            vec![format!("declare i1 @llvm.is.fpclass.{suffix}({ty}, i32)")],
            format!("{prelude}  %c = call i1 @llvm.is.fpclass.{suffix}({ty} %x, i32 {mask})\n  %o = zext i1 %c to i64\n  ret i64 %o\n"),
        );
    }
    for target in [8u32, 16, 32, 64] {
        for name in ["fptosi.sat", "fptoui.sat"] {
            suite.add(
                &group,
                format!("{ty} {name} to i{target}"),
                Kind::Float,
                vec![format!(
                    "declare i{target} @llvm.{name}.i{target}.{suffix}({ty})"
                )],
                format!(
                    "{prelude}  %r = call i{target} @llvm.{name}.i{target}.{suffix}({ty} %x)\n{}  ret i64 %o\n",
                    int_fold(target, "%r")
                ),
            );
        }
    }
    // Calls of the C library: arguments and results in floating-point registers.
    if ty == "double" {
        for name in ["pow", "atan2", "fmod"] {
            add(
                suite,
                &format!("call {name}"),
                vec![format!("declare double @{name}(double, double)")],
                format!("  %r = call double @{name}(double %x, double %y)\n"),
                "%r",
            );
        }
    } else {
        add(
            suite,
            "call sinf",
            vec!["declare float @sinf(float)".to_owned()],
            "  %r = call float @sinf(float %x)\n".to_owned(),
            "%r",
        );
        add(
            suite,
            "call powf",
            vec!["declare float @powf(float, float)".to_owned()],
            "  %r = call float @powf(float %x, float %y)\n".to_owned(),
            "%r",
        );
    }
    // Conversions from integers, and back where the value is in range.
    for source in [8u32, 16, 32, 64] {
        for (to_float, to_int, name) in [
            ("sitofp", "fptosi", "signed"),
            ("uitofp", "fptoui", "unsigned"),
        ] {
            let narrow = if source == 64 {
                "  %i = add i64 %a, 0\n".to_owned()
            } else {
                format!("  %i = trunc i64 %a to i{source}\n")
            };
            suite.add(
                &group,
                format!("{ty} {to_float} from i{source}"),
                Kind::Float,
                vec![],
                format!(
                    "{narrow}  %r = {to_float} i{source} %i to {ty}\n{}  ret i64 %o\n",
                    float_fold(ty, "%r")
                ),
            );
            // Only the sources that the type holds exactly come back through the integer.
            let exact = match ty {
                "float" => source <= 16,
                _ => source <= 32,
            };
            if exact {
                suite.add(
                    &group,
                    format!("{ty} {to_int} round trip i{source} ({name})"),
                    Kind::Float,
                    vec![],
                    format!(
                        "{narrow}  %f = {to_float} i{source} %i to {ty}\n  %r = {to_int} {ty} %f to i{source}\n{}  ret i64 %o\n",
                        int_fold(source, "%r")
                    ),
                );
            }
        }
    }
    if ty == "float" {
        suite.add(
            &group,
            "float fpext to double".to_owned(),
            Kind::Float,
            vec![],
            format!("{prelude}  %r = fpext float %x to double\n  %o = bitcast double %r to i64\n  ret i64 %o\n"),
        );
    } else {
        suite.add(
            &group,
            "double fptrunc to float".to_owned(),
            Kind::Float,
            vec![],
            format!(
                "{prelude}  %r = fptrunc double %x to float\n{}  ret i64 %o\n",
                float_fold("float", "%r")
            ),
        );
    }
}

// ---- vectors ----

fn vector_prelude(vector: &str) -> String {
    format!(
        "  %v0 = insertelement <2 x i64> undef, i64 %a, i32 0\n  %v1 = insertelement <2 x i64> %v0, i64 %b, i32 1\n  %ak = xor i64 %a, 6148914691236517205\n  %w0 = insertelement <2 x i64> undef, i64 %b, i32 0\n  %w1 = insertelement <2 x i64> %w0, i64 %ak, i32 1\n  %x = bitcast <2 x i64> %v1 to {vector}\n  %y = bitcast <2 x i64> %w1 to {vector}\n"
    )
}

/// Defines `%o`, the 128 bits of the vector folded into 64.
fn vector_fold(vector: &str, value: &str) -> String {
    format!(
        "  %vb = bitcast {vector} {value} to <2 x i64>\n  %vl0 = extractelement <2 x i64> %vb, i32 0\n  %vl1 = extractelement <2 x i64> %vb, i32 1\n  %vmx = mul i64 %vl1, {MIX}\n  %o = add i64 %vl0, %vmx\n"
    )
}

fn splat(lane: &str, lanes: u32, value: i64) -> String {
    let items: Vec<String> = (0..lanes).map(|_| format!("{lane} {value}")).collect();
    format!("<{}>", items.join(", "))
}

fn int_vector_cases(suite: &mut Suite, lanes: u32, bits: u32) {
    let lane = format!("i{bits}");
    let vector = format!("<{lanes} x {lane}>");
    let mask = format!("<{lanes} x i1>");
    let suffix = format!("v{lanes}i{bits}");
    let group = format!("vector {vector}");
    let prelude = vector_prelude(&vector);
    let add = |suite: &mut Suite,
               name: &str,
               declarations: Vec<String>,
               compute: String,
               result: &str| {
        let body = format!(
            "{prelude}{compute}{}  ret i64 %o\n",
            vector_fold(&vector, result)
        );
        suite.add(
            &group,
            format!("{vector} {name}"),
            Kind::Int,
            declarations,
            body,
        );
    };
    for op in ["add", "sub", "mul", "and", "or", "xor"] {
        add(
            suite,
            op,
            vec![],
            format!("  %r = {op} {vector} %x, %y\n"),
            "%r",
        );
    }
    let amounts = splat(&lane, lanes, i64::from(bits) - 1);
    for op in ["shl", "lshr", "ashr"] {
        add(
            suite,
            op,
            vec![],
            format!("  %s = and {vector} %y, {amounts}\n  %r = {op} {vector} %x, %s\n"),
            "%r",
        );
    }
    for predicate in ["eq", "ne", "ugt", "ult", "sgt", "sle"] {
        add(
            suite,
            &format!("icmp {predicate}"),
            vec![],
            format!("  %c = icmp {predicate} {vector} %x, %y\n  %r = sext {mask} %c to {vector}\n"),
            "%r",
        );
    }
    add(
        suite,
        "select",
        vec![],
        format!(
            "  %c = icmp slt {vector} %x, %y\n  %r = select {mask} %c, {vector} %x, {vector} %y\n"
        ),
        "%r",
    );
    for name in ["smin", "smax", "umin", "umax", "uadd.sat", "ssub.sat"] {
        add(
            suite,
            name,
            vec![format!(
                "declare {vector} @llvm.{name}.{suffix}({vector}, {vector})"
            )],
            format!("  %r = call {vector} @llvm.{name}.{suffix}({vector} %x, {vector} %y)\n"),
            "%r",
        );
    }
    for name in ["ctpop", "bitreverse"] {
        add(
            suite,
            name,
            vec![format!("declare {vector} @llvm.{name}.{suffix}({vector})")],
            format!("  %r = call {vector} @llvm.{name}.{suffix}({vector} %x)\n"),
            "%r",
        );
    }
    add(
        suite,
        "abs",
        vec![format!("declare {vector} @llvm.abs.{suffix}({vector}, i1)")],
        format!("  %r = call {vector} @llvm.abs.{suffix}({vector} %x, i1 false)\n"),
        "%r",
    );
    if bits >= 16 {
        add(
            suite,
            "bswap",
            vec![format!("declare {vector} @llvm.bswap.{suffix}({vector})")],
            format!("  %r = call {vector} @llvm.bswap.{suffix}({vector} %x)\n"),
            "%r",
        );
    }
    // Lanes: reverse, interleave, and an element chosen at run time.
    let reverse: Vec<String> = (0..lanes)
        .rev()
        .map(|index| format!("i32 {index}"))
        .collect();
    add(
        suite,
        "shufflevector reverse",
        vec![],
        format!(
            "  %r = shufflevector {vector} %x, {vector} %y, <{lanes} x i32> <{}>\n",
            reverse.join(", ")
        ),
        "%r",
    );
    let interleave: Vec<String> = (0..lanes / 2)
        .flat_map(|index| [index, lanes + index])
        .map(|index| format!("i32 {index}"))
        .collect();
    add(
        suite,
        "shufflevector interleave",
        vec![],
        format!(
            "  %r = shufflevector {vector} %x, {vector} %y, <{lanes} x i32> <{}>\n",
            interleave.join(", ")
        ),
        "%r",
    );
    add(
        suite,
        "extractelement insertelement variable",
        vec![],
        format!(
            "  %i = and i64 %a, {last}\n  %j = and i64 %b, {last}\n  %e = extractelement {vector} %x, i64 %i\n  %r = insertelement {vector} %y, {lane} %e, i64 %j\n",
            last = lanes - 1
        ),
        "%r",
    );
    add(
        suite,
        "store load",
        vec![],
        format!(
            "  %p = alloca {vector}, align 16\n  store {vector} %x, ptr %p, align 16\n  %q = getelementptr {lane}, ptr %p, i64 1\n  %e = load {lane}, ptr %q, align 1\n  %r = insertelement {vector} %y, {lane} %e, i32 0\n"
        ),
        "%r",
    );
    // A mask vector in memory is its bits in one integer; a variable index reads and writes one lane.
    add(
        suite,
        "mask store load",
        vec![],
        format!(
            "  %c = icmp ugt {vector} %x, %y\n  %p = alloca {mask}, align 8\n  store {mask} %c, ptr %p\n  %d = load {mask}, ptr %p\n  %r = select {mask} %d, {vector} %x, {vector} %y\n"
        ),
        "%r",
    );
    add(
        suite,
        "mask extractelement insertelement variable",
        vec![],
        format!(
            "  %c = icmp ult {vector} %x, %y\n  %i = and i64 %b, {last}\n  %e = extractelement {mask} %c, i64 %i\n  %f = xor i1 %e, true\n  %j = and i64 %a, {last}\n  %d = insertelement {mask} %c, i1 %f, i64 %j\n  %r = select {mask} %d, {vector} %x, {vector} %y\n",
            last = lanes - 1
        ),
        "%r",
    );
    for reduction in [
        "add", "mul", "and", "or", "xor", "smax", "smin", "umax", "umin",
    ] {
        suite.add(
            &group,
            format!("{vector} vector.reduce.{reduction}"),
            Kind::Int,
            vec![format!(
                "declare {lane} @llvm.vector.reduce.{reduction}.{suffix}({vector})"
            )],
            format!(
                "{prelude}  %r = call {lane} @llvm.vector.reduce.{reduction}.{suffix}({vector} %x)\n{}  ret i64 %o\n",
                int_fold(bits, "%r")
            ),
        );
    }
}

fn float_vector_cases(suite: &mut Suite, lanes: u32, ty: &str) {
    let suffix = if ty == "double" { "f64" } else { "f32" };
    let bits = if ty == "double" { 64 } else { 32 };
    let vector = format!("<{lanes} x {ty}>");
    let integers = format!("<{lanes} x i{bits}>");
    let mask = format!("<{lanes} x i1>");
    let intrinsic = format!("v{lanes}{suffix}");
    let group = format!("vector {vector}");
    let prelude = vector_prelude(&vector);
    let add = |suite: &mut Suite,
               name: &str,
               declarations: Vec<String>,
               compute: String,
               result: &str| {
        let body = format!(
            "{prelude}{compute}{}  ret i64 %o\n",
            vector_fold(&vector, result)
        );
        suite.add(
            &group,
            format!("{vector} {name}"),
            Kind::Float,
            declarations,
            body,
        );
    };
    for op in ["fadd", "fsub", "fmul", "fdiv"] {
        add(
            suite,
            op,
            vec![],
            format!("  %r = {op} {vector} %x, %y\n"),
            "%r",
        );
    }
    add(
        suite,
        "fneg",
        vec![],
        format!("  %r = fneg {vector} %x\n"),
        "%r",
    );
    for predicate in ["oeq", "olt", "ole", "une", "ord", "uno", "ugt"] {
        suite.add(
            &group,
            format!("{vector} fcmp {predicate}"),
            Kind::Float,
            vec![],
            format!(
                "{prelude}  %c = fcmp {predicate} {vector} %x, %y\n  %s = sext {mask} %c to {integers}\n{}  ret i64 %o\n",
                vector_fold(&integers, "%s")
            ),
        );
    }
    add(
        suite,
        "select",
        vec![],
        format!(
            "  %c = fcmp olt {vector} %x, %y\n  %r = select {mask} %c, {vector} %x, {vector} %y\n"
        ),
        "%r",
    );
    for name in ["fabs", "sqrt", "floor", "ceil"] {
        add(
            suite,
            name,
            vec![format!(
                "declare {vector} @llvm.{name}.{intrinsic}({vector})"
            )],
            format!("  %r = call {vector} @llvm.{name}.{intrinsic}({vector} %x)\n"),
            "%r",
        );
    }
    for name in ["copysign", "minimum", "maximum"] {
        // See float_cases: a NaN that `minimum` or `maximum` returns is compared as one NaN.
        let canonical = if name == "copysign" {
            String::new()
        } else {
            let nan = format!("{ty} 0x7FF8000000000000");
            let nans = vec![nan; lanes as usize].join(", ");
            format!(
                "  %nan = fcmp uno {vector} %r, %r\n  %q = select {mask} %nan, {vector} <{nans}>, {vector} %r\n"
            )
        };
        add(
            suite,
            name,
            vec![format!(
                "declare {vector} @llvm.{name}.{intrinsic}({vector}, {vector})"
            )],
            format!(
                "  %r = call {vector} @llvm.{name}.{intrinsic}({vector} %x, {vector} %y)\n{canonical}"
            ),
            if name == "copysign" { "%r" } else { "%q" },
        );
    }
    add(
        suite,
        "fma",
        vec![format!(
            "declare {vector} @llvm.fma.{intrinsic}({vector}, {vector}, {vector})"
        )],
        format!(
            "  %r = call {vector} @llvm.fma.{intrinsic}({vector} %x, {vector} %y, {vector} %x)\n"
        ),
        "%r",
    );
    add(
        suite,
        "extractelement insertelement variable",
        vec![],
        format!(
            "  %i = and i64 %a, {last}\n  %j = and i64 %b, {last}\n  %e = extractelement {vector} %x, i64 %i\n  %r = insertelement {vector} %y, {ty} %e, i64 %j\n",
            last = lanes - 1
        ),
        "%r",
    );
}

// ---- control flow, calls, memory and atomics ----

/// Folds a 64-bit expression list into `%o` and returns it.
fn control_cases(suite: &mut Suite) {
    let group = "control";
    let mut add = |label: &str, declarations: Vec<String>, body: String| {
        suite.add(group, label.to_owned(), Kind::Int, declarations, body);
    };
    add(
        "loop with phis",
        vec![],
        "entry:\n  %n = and i64 %a, 31\n  br label %loop\nloop:\n  %i = phi i64 [ 0, %entry ], [ %i2, %loop ]\n  %acc = phi i64 [ %b, %entry ], [ %acc2, %loop ]\n  %m = mul i64 %acc, 31\n  %x = xor i64 %m, %i\n  %acc2 = add i64 %x, %a\n  %i2 = add i64 %i, 1\n  %c = icmp ult i64 %i2, %n\n  br i1 %c, label %loop, label %done\ndone:\n  ret i64 %acc2\n".to_owned(),
    );
    add(
        "phis that swap",
        vec![],
        "entry:\n  %n = and i64 %a, 7\n  br label %loop\nloop:\n  %p = phi i64 [ %a, %entry ], [ %q, %loop ]\n  %q = phi i64 [ %b, %entry ], [ %p, %loop ]\n  %i = phi i64 [ 0, %entry ], [ %i2, %loop ]\n  %i2 = add i64 %i, 1\n  %c = icmp ult i64 %i2, %n\n  br i1 %c, label %loop, label %done\ndone:\n  %m = mul i64 %p, 1000003\n  %o = add i64 %m, %q\n  ret i64 %o\n".to_owned(),
    );
    let mut large = String::from("entry:\n");
    for index in 0..20 {
        let _ = writeln!(large, "  %xa{index} = add i64 %a, {index}");
        let _ = writeln!(large, "  %yb{index} = mul i64 %b, {}", index + 1);
        let (from_x, from_y) = if index == 0 {
            ("undef".to_owned(), "undef".to_owned())
        } else {
            (format!("%xv{}", index - 1), format!("%yv{}", index - 1))
        };
        let _ = writeln!(
            large,
            "  %xv{index} = insertvalue [20 x i64] {from_x}, i64 %xa{index}, {index}"
        );
        let _ = writeln!(
            large,
            "  %yv{index} = insertvalue [20 x i64] {from_y}, i64 %yb{index}, {index}"
        );
    }
    large.push_str("  %n = and i64 %a, 7\n  br label %loop\nloop:\n  %X = phi [20 x i64] [ %xv19, %entry ], [ %Y, %loop ]\n  %Y = phi [20 x i64] [ %yv19, %entry ], [ %X, %loop ]\n  %i = phi i64 [ 0, %entry ], [ %i2, %loop ]\n  %i2 = add i64 %i, 1\n  %c = icmp ult i64 %i2, %n\n  br i1 %c, label %loop, label %done\ndone:\n  %e0 = extractvalue [20 x i64] %X, 0\n  %e1 = extractvalue [20 x i64] %X, 19\n  %e2 = extractvalue [20 x i64] %Y, 7\n  %t0 = mul i64 %e0, 31\n  %t1 = add i64 %t0, %e1\n  %t2 = mul i64 %t1, 31\n  %o = add i64 %t2, %e2\n  ret i64 %o\n");
    add("a phi of twenty values that swap", vec![], large);

    let mut switch =
        String::from("entry:\n  %k = and i64 %a, 15\n  switch i64 %k, label %other [\n");
    for value in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13] {
        let _ = writeln!(switch, "    i64 {value}, label %c{}", value % 5);
    }
    switch.push_str("  ]\n");
    for target in 0..5 {
        let _ = write!(
            switch,
            "c{target}:\n  %r{target} = mul i64 %b, {}\n  %s{target} = add i64 %r{target}, %k\n  br label %join\n",
            target + 3
        );
    }
    switch.push_str("other:\n  %ro = xor i64 %a, %b\n  br label %join\njoin:\n  %o = phi i64 [ %s0, %c0 ], [ %s1, %c1 ], [ %s2, %c2 ], [ %s3, %c3 ], [ %s4, %c4 ], [ %ro, %other ]\n  ret i64 %o\n");
    add("switch with shared targets", vec![], switch);
    add(
        "switch on i8 with negative cases",
        vec![],
        "entry:\n  %k = trunc i64 %a to i8\n  switch i8 %k, label %d [\n    i8 -128, label %n128\n    i8 -1, label %n1\n    i8 0, label %z\n    i8 127, label %p127\n  ]\nn128:\n  br label %join\nn1:\n  %m1 = mul i64 %b, 7\n  br label %join\nz:\n  %m2 = add i64 %b, 11\n  br label %join\np127:\n  %m3 = xor i64 %b, -1\n  br label %join\nd:\n  %m4 = sub i64 %a, %b\n  br label %join\njoin:\n  %o = phi i64 [ 1, %n128 ], [ %m1, %n1 ], [ %m2, %z ], [ %m3, %p127 ], [ %m4, %d ]\n  ret i64 %o\n".to_owned(),
    );
    add(
        "calls through a table of functions",
        vec![
            "define internal i64 @ind0(i64 %a, i64 %b) {\n  %r = add i64 %a, %b\n  ret i64 %r\n}".to_owned(),
            "define internal i64 @ind1(i64 %a, i64 %b) {\n  %r = sub i64 %a, %b\n  ret i64 %r\n}".to_owned(),
            "define internal i64 @ind2(i64 %a, i64 %b) {\n  %r = mul i64 %a, %b\n  ret i64 %r\n}".to_owned(),
            "define internal i64 @ind3(i64 %a, i64 %b) {\n  %r = xor i64 %a, %b\n  ret i64 %r\n}".to_owned(),
            "@ind_table = internal constant [4 x ptr] [ptr @ind0, ptr @ind1, ptr @ind2, ptr @ind3]".to_owned(),
        ],
        "entry:\n  %i = and i64 %a, 3\n  %p = getelementptr [4 x ptr], ptr @ind_table, i64 0, i64 %i\n  %f = load ptr, ptr %p\n  %r = call i64 %f(i64 %a, i64 %b)\n  ret i64 %r\n".to_owned(),
    );
    add(
        "global that is read and written",
        vec!["@counter = internal global i64 5".to_owned()],
        "entry:\n  %v = load i64, ptr @counter\n  %n = add i64 %v, %a\n  store i64 %n, ptr @counter\n  %w = load i64, ptr @counter\n  %m = mul i64 %w, 3\n  %o = sub i64 %m, %b\n  ret i64 %o\n".to_owned(),
    );
}

fn call_cases(suite: &mut Suite) {
    let group = "calls";
    let mut add = |label: &str, declarations: Vec<String>, body: String| {
        suite.add(group, label.to_owned(), Kind::Int, declarations, body);
    };
    add(
        "a pair as argument and result",
        vec!["define internal { i64, i64 } @agg_swap({ i64, i64 } %p) {\n  %x = extractvalue { i64, i64 } %p, 0\n  %y = extractvalue { i64, i64 } %p, 1\n  %r0 = insertvalue { i64, i64 } undef, i64 %y, 0\n  %r1 = insertvalue { i64, i64 } %r0, i64 %x, 1\n  ret { i64, i64 } %r1\n}".to_owned()],
        "entry:\n  %p0 = insertvalue { i64, i64 } undef, i64 %a, 0\n  %p1 = insertvalue { i64, i64 } %p0, i64 %b, 1\n  %r = call { i64, i64 } @agg_swap({ i64, i64 } %p1)\n  %x = extractvalue { i64, i64 } %r, 0\n  %y = extractvalue { i64, i64 } %r, 1\n  %m = mul i64 %x, 1000003\n  %o = add i64 %m, %y\n  ret i64 %o\n".to_owned(),
    );
    add(
        "128-bit arguments and result",
        vec!["define internal i128 @agg_wide(i128 %x, i64 %k, i128 %y) {\n  %kk = zext i64 %k to i128\n  %m = mul i128 %x, %kk\n  %r = add i128 %m, %y\n  ret i128 %r\n}".to_owned()],
        format!(
            "{}  %r = call i128 @agg_wide(i128 %x, i64 %a, i128 %y)\n{}  ret i64 %o\n",
            int_prelude(128),
            int_fold(128, "%r")
        ),
    );
    add(
        "an array of twenty as argument and result",
        vec!["define internal [20 x i64] @agg_big([20 x i64] %v, i64 %k) {\n  %e0 = extractvalue [20 x i64] %v, 0\n  %e19 = extractvalue [20 x i64] %v, 19\n  %s = add i64 %e19, %k\n  %r0 = insertvalue [20 x i64] %v, i64 %s, 0\n  %r1 = insertvalue [20 x i64] %r0, i64 %e0, 19\n  ret [20 x i64] %r1\n}".to_owned()],
        {
            let mut text = String::from("entry:\n");
            for index in 0..20 {
                let from = if index == 0 { "undef".to_owned() } else { format!("%v{}", index - 1) };
                let _ = writeln!(text, "  %t{index} = add i64 %a, {index}");
                let _ = writeln!(text, "  %v{index} = insertvalue [20 x i64] {from}, i64 %t{index}, {index}");
            }
            text.push_str("  %r = call [20 x i64] @agg_big([20 x i64] %v19, i64 %b)\n  %f = extractvalue [20 x i64] %r, 0\n  %g = extractvalue [20 x i64] %r, 19\n  %h = extractvalue [20 x i64] %r, 10\n  %m = mul i64 %f, 31\n  %n = add i64 %m, %g\n  %p = mul i64 %n, 31\n  %o = add i64 %p, %h\n  ret i64 %o\n");
            text
        },
    );
    add(
        "twelve integers and ten doubles",
        vec!["define internal i64 @agg_many(i64 %a0, i64 %a1, i64 %a2, i64 %a3, i64 %a4, i64 %a5, i64 %a6, i64 %a7, i64 %a8, i64 %a9, i64 %a10, i64 %a11, double %d0, double %d1, double %d2, double %d3, double %d4, double %d5, double %d6, double %d7, double %d8, double %d9) {\n  %i1 = mul i64 %a1, 3\n  %i2 = mul i64 %a2, 5\n  %i3 = mul i64 %a3, 7\n  %i4 = mul i64 %a4, 11\n  %i5 = mul i64 %a5, 13\n  %i6 = mul i64 %a6, 17\n  %i7 = mul i64 %a7, 19\n  %i8 = mul i64 %a8, 23\n  %i9 = mul i64 %a9, 29\n  %i10 = mul i64 %a10, 31\n  %i11 = mul i64 %a11, 37\n  %s1 = add i64 %a0, %i1\n  %s2 = add i64 %s1, %i2\n  %s3 = add i64 %s2, %i3\n  %s4 = add i64 %s3, %i4\n  %s5 = add i64 %s4, %i5\n  %s6 = add i64 %s5, %i6\n  %s7 = add i64 %s6, %i7\n  %s8 = add i64 %s7, %i8\n  %s9 = add i64 %s8, %i9\n  %s10 = add i64 %s9, %i10\n  %s11 = add i64 %s10, %i11\n  %f1 = fmul double %d1, 2.0\n  %f2 = fmul double %d2, 3.0\n  %f3 = fmul double %d3, 4.0\n  %f4 = fmul double %d4, 5.0\n  %f5 = fmul double %d5, 6.0\n  %f6 = fmul double %d6, 7.0\n  %f7 = fmul double %d7, 8.0\n  %f8 = fmul double %d8, 9.0\n  %f9 = fmul double %d9, 10.0\n  %g1 = fadd double %d0, %f1\n  %g2 = fadd double %g1, %f2\n  %g3 = fadd double %g2, %f3\n  %g4 = fadd double %g3, %f4\n  %g5 = fadd double %g4, %f5\n  %g6 = fadd double %g5, %f6\n  %g7 = fadd double %g6, %f7\n  %g8 = fadd double %g7, %f8\n  %g9 = fadd double %g8, %f9\n  %gb = bitcast double %g9 to i64\n  %r = xor i64 %s11, %gb\n  ret i64 %r\n}".to_owned()],
        "entry:\n  %c2 = xor i64 %a, %b\n  %c3 = add i64 %a, %b\n  %c4 = mul i64 %a, 3\n  %c5 = sub i64 %b, %a\n  %c6 = shl i64 %a, 1\n  %c7 = lshr i64 %b, 1\n  %c8 = or i64 %a, 5\n  %c9 = and i64 %b, 255\n  %c10 = add i64 %c2, 17\n  %c11 = xor i64 %c3, 99\n  %f0 = sitofp i64 %a to double\n  %f1 = sitofp i64 %b to double\n  %f2 = sitofp i64 %c2 to double\n  %f3 = sitofp i64 %c3 to double\n  %f4 = sitofp i64 %c4 to double\n  %f5 = sitofp i64 %c5 to double\n  %f6 = sitofp i64 %c6 to double\n  %f7 = sitofp i64 %c7 to double\n  %f8 = sitofp i64 %c8 to double\n  %f9 = sitofp i64 %c9 to double\n  %r = call i64 @agg_many(i64 %a, i64 %b, i64 %c2, i64 %c3, i64 %c4, i64 %c5, i64 %c6, i64 %c7, i64 %c8, i64 %c9, i64 %c10, i64 %c11, double %f0, double %f1, double %f2, double %f3, double %f4, double %f5, double %f6, double %f7, double %f8, double %f9)\n  ret i64 %r\n".to_owned(),
    );
    add(
        "a vector argument and result",
        vec!["define internal <4 x i32> @agg_vec(<4 x i32> %v, i32 %k) {\n  %k0 = insertelement <4 x i32> undef, i32 %k, i32 0\n  %k1 = shufflevector <4 x i32> %k0, <4 x i32> undef, <4 x i32> zeroinitializer\n  %r = add <4 x i32> %v, %k1\n  ret <4 x i32> %r\n}".to_owned()],
        format!(
            "{}  %k = trunc i64 %b to i32\n  %r = call <4 x i32> @agg_vec(<4 x i32> %x, i32 %k)\n{}  ret i64 %o\n",
            vector_prelude("<4 x i32>"),
            vector_fold("<4 x i32>", "%r")
        ),
    );
    add(
        "a record of mixed fields as argument and result",
        vec!["define internal { double, i32, i8 } @agg_mix(double %d, { double, i32, i8 } %s, i8 %t) {\n  %f = extractvalue { double, i32, i8 } %s, 0\n  %i = extractvalue { double, i32, i8 } %s, 1\n  %c = extractvalue { double, i32, i8 } %s, 2\n  %f2 = fadd double %f, %d\n  %i2 = add i32 %i, 7\n  %c2 = xor i8 %c, %t\n  %r0 = insertvalue { double, i32, i8 } undef, double %f2, 0\n  %r1 = insertvalue { double, i32, i8 } %r0, i32 %i2, 1\n  %r2 = insertvalue { double, i32, i8 } %r1, i8 %c2, 2\n  ret { double, i32, i8 } %r2\n}".to_owned()],
        "entry:\n  %d = sitofp i64 %a to double\n  %i = trunc i64 %b to i32\n  %c = trunc i64 %a to i8\n  %t = trunc i64 %b to i8\n  %s0 = insertvalue { double, i32, i8 } undef, double %d, 0\n  %s1 = insertvalue { double, i32, i8 } %s0, i32 %i, 1\n  %s2 = insertvalue { double, i32, i8 } %s1, i8 %c, 2\n  %r = call { double, i32, i8 } @agg_mix(double %d, { double, i32, i8 } %s2, i8 %t)\n  %f = extractvalue { double, i32, i8 } %r, 0\n  %j = extractvalue { double, i32, i8 } %r, 1\n  %k = extractvalue { double, i32, i8 } %r, 2\n  %fb = bitcast double %f to i64\n  %jz = zext i32 %j to i64\n  %kz = zext i8 %k to i64\n  %m = mul i64 %fb, 31\n  %n = add i64 %m, %jz\n  %p = mul i64 %n, 31\n  %o = add i64 %p, %kz\n  ret i64 %o\n".to_owned(),
    );
}

fn memory_cases(suite: &mut Suite) {
    let group = "memory";
    let mut add = |label: String, declarations: Vec<String>, body: String| {
        suite.add(group, label, Kind::Int, declarations, body);
    };
    let memcpy = "declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)".to_owned();
    let memmove = "declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)".to_owned();
    let memset = "declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)".to_owned();
    // A buffer of 96 bytes that holds a pattern made of the inputs.
    let fill = |name: &str| {
        format!(
            "  %{name} = alloca [96 x i8], align 8\n  call void @llvm.memset.p0.i64(ptr %{name}, i8 90, i64 96, i1 false)\n  store i64 %a, ptr %{name}, align 8\n  %{name}1 = getelementptr i8, ptr %{name}, i64 8\n  store i64 %b, ptr %{name}1, align 1\n  %{name}2 = getelementptr i8, ptr %{name}, i64 19\n  %{name}x = xor i64 %a, %b\n  store i64 %{name}x, ptr %{name}2, align 1\n"
        )
    };
    // The twelve words of a buffer folded into `%o`.
    let hash = |name: &str| {
        let mut text = String::new();
        for index in 0..12 {
            let _ = writeln!(
                text,
                "  %{name}p{index} = getelementptr i64, ptr %{name}, i64 {index}"
            );
            let _ = writeln!(
                text,
                "  %{name}v{index} = load i64, ptr %{name}p{index}, align 8"
            );
        }
        let mut previous = format!("%{name}v0");
        for index in 1..12 {
            let _ = writeln!(text, "  %{name}m{index} = mul i64 {previous}, 31");
            let _ = writeln!(
                text,
                "  %{name}s{index} = add i64 %{name}m{index}, %{name}v{index}"
            );
            previous = format!("%{name}s{index}");
        }
        let _ = writeln!(text, "  %o = add i64 {previous}, 0");
        text
    };
    let offsets = "  %n = and i64 %a, 63\n  %so = and i64 %b, 15\n  %do0 = lshr i64 %a, 8\n  %do = and i64 %do0, 15\n";
    add(
        "memcpy of a variable length".to_owned(),
        vec![memcpy.clone(), memset.clone()],
        format!(
            "entry:\n{}  %dst = alloca [96 x i8], align 8\n  call void @llvm.memset.p0.i64(ptr %dst, i8 0, i64 96, i1 false)\n{offsets}  %sp = getelementptr i8, ptr %src, i64 %so\n  %dp = getelementptr i8, ptr %dst, i64 %do\n  call void @llvm.memcpy.p0.p0.i64(ptr %dp, ptr %sp, i64 %n, i1 false)\n{}  ret i64 %o\n",
            fill("src"),
            hash("dst")
        ),
    );
    add(
        "memmove inside one buffer".to_owned(),
        vec![memmove.clone(), memset.clone()],
        format!(
            "entry:\n{}{offsets}  %sp = getelementptr i8, ptr %buf, i64 %so\n  %dp = getelementptr i8, ptr %buf, i64 %do\n  call void @llvm.memmove.p0.p0.i64(ptr %dp, ptr %sp, i64 %n, i1 false)\n{}  ret i64 %o\n",
            fill("buf"),
            hash("buf")
        ),
    );
    add(
        "memset of a variable length".to_owned(),
        vec![memset.clone()],
        format!(
            "entry:\n{}{offsets}  %dp = getelementptr i8, ptr %buf, i64 %do\n  %v = trunc i64 %b to i8\n  call void @llvm.memset.p0.i64(ptr %dp, i8 %v, i64 %n, i1 false)\n{}  ret i64 %o\n",
            fill("buf"),
            hash("buf")
        ),
    );
    for size in [0, 1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64] {
        add(
            format!("memcpy of {size} bytes"),
            vec![memcpy.clone(), memset.clone()],
            format!(
                "entry:\n{}  %dst = alloca [96 x i8], align 8\n  call void @llvm.memset.p0.i64(ptr %dst, i8 0, i64 96, i1 false)\n  %so = and i64 %b, 15\n  %do0 = lshr i64 %a, 8\n  %do = and i64 %do0, 15\n  %sp = getelementptr i8, ptr %src, i64 %so\n  %dp = getelementptr i8, ptr %dst, i64 %do\n  call void @llvm.memcpy.p0.p0.i64(ptr %dp, ptr %sp, i64 {size}, i1 false)\n{}  ret i64 %o\n",
                fill("src"),
                hash("dst")
            ),
        );
        add(
            format!("memset of {size} bytes"),
            vec![memset.clone()],
            format!(
                "entry:\n{}  %do0 = lshr i64 %a, 8\n  %do = and i64 %do0, 15\n  %dp = getelementptr i8, ptr %buf, i64 %do\n  %v = trunc i64 %b to i8\n  call void @llvm.memset.p0.i64(ptr %dp, i8 %v, i64 {size}, i1 false)\n{}  ret i64 %o\n",
                fill("buf"),
                hash("buf")
            ),
        );
    }
    let record = "%t.rec = type { i8, i32, i64, [3 x i16], double }".to_owned();
    add(
        "fields of a record in an array".to_owned(),
        vec![record],
        "entry:\n  %p = alloca [4 x %t.rec], align 16\n  %i = urem i64 %a, 4\n  %f0 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 0\n  %t0 = trunc i64 %b to i8\n  store i8 %t0, ptr %f0\n  %f1 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 1\n  %t1 = trunc i64 %a to i32\n  store i32 %t1, ptr %f1\n  %f2 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 2\n  store i64 %b, ptr %f2\n  %g0 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 3, i64 0\n  %t3 = trunc i64 %a to i16\n  store i16 %t3, ptr %g0\n  %g1 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 3, i64 1\n  %t5 = trunc i64 %b to i16\n  store i16 %t5, ptr %g1\n  %g2 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 3, i64 2\n  store i16 %t3, ptr %g2\n  %j0 = and i64 %b, 1\n  %j = add i64 %j0, 1\n  %gj = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 3, i64 %j\n  %t6 = lshr i64 %a, 16\n  %t7 = trunc i64 %t6 to i16\n  store i16 %t7, ptr %gj\n  %f4 = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i, i32 4\n  %t4 = sitofp i64 %a to double\n  store double %t4, ptr %f4\n  %q = getelementptr [4 x %t.rec], ptr %p, i64 0, i64 %i\n  %s = load %t.rec, ptr %q\n  %e0 = extractvalue %t.rec %s, 0\n  %e1 = extractvalue %t.rec %s, 1\n  %e2 = extractvalue %t.rec %s, 2\n  %e3 = extractvalue %t.rec %s, 3\n  %e3a = extractvalue [3 x i16] %e3, 0\n  %e3b = extractvalue [3 x i16] %e3, 1\n  %e3c = extractvalue [3 x i16] %e3, 2\n  %e4 = extractvalue %t.rec %s, 4\n  %z0 = zext i8 %e0 to i64\n  %z1 = zext i32 %e1 to i64\n  %z2 = zext i16 %e3a to i64\n  %z3 = zext i16 %e3b to i64\n  %z4 = zext i16 %e3c to i64\n  %z5 = bitcast double %e4 to i64\n  %h0 = mul i64 %z0, 31\n  %h1 = add i64 %h0, %z1\n  %h2 = mul i64 %h1, 31\n  %h3 = add i64 %h2, %e2\n  %h4 = mul i64 %h3, 31\n  %h5 = add i64 %h4, %z2\n  %h6 = mul i64 %h5, 31\n  %h7 = add i64 %h6, %z3\n  %h8 = mul i64 %h7, 31\n  %h9 = add i64 %h8, %z4\n  %ha = mul i64 %h9, 31\n  %o = add i64 %ha, %z5\n  ret i64 %o\n".to_owned(),
    );
    add(
        "unaligned loads and stores".to_owned(),
        vec![],
        "entry:\n  %buf = alloca [32 x i8], align 8\n  %k = and i64 %a, 7\n  %p = getelementptr i8, ptr %buf, i64 %k\n  store i64 %b, ptr %p, align 1\n  %q = getelementptr i8, ptr %p, i64 3\n  %t = trunc i64 %a to i32\n  store i32 %t, ptr %q, align 1\n  %l1 = load i64, ptr %p, align 1\n  %r = getelementptr i8, ptr %p, i64 1\n  %l2 = load i32, ptr %r, align 1\n  %l3 = zext i32 %l2 to i64\n  %m = mul i64 %l1, 31\n  %o = add i64 %m, %l3\n  ret i64 %o\n".to_owned(),
    );
}

fn atomic_cases(suite: &mut Suite) {
    let group = "atomics";
    for width in [8u32, 16, 32, 64] {
        let ty = format!("i{width}");
        let align = width / 8;
        for operation in [
            "xchg", "add", "sub", "and", "nand", "or", "xor", "max", "min", "umax", "umin",
        ] {
            suite.add(
                group,
                format!("{ty} atomicrmw {operation}"),
                Kind::Int,
                vec![],
                format!(
                    "{}  %p = alloca {ty}, align 8\n  store {ty} %x, ptr %p, align {align}\n  %old = atomicrmw {operation} ptr %p, {ty} %y seq_cst\n  %new = load atomic {ty}, ptr %p seq_cst, align {align}\n  %m = mul {ty} %old, 31\n  %z = add {ty} %m, %new\n{}  ret i64 %o\n",
                    int_prelude(width),
                    int_fold(width, "%z")
                ),
            );
        }
        suite.add(
            group,
            format!("{ty} cmpxchg"),
            Kind::Int,
            vec![],
            format!(
                "{}  %p = alloca {ty}, align 8\n  store {ty} %x, ptr %p, align {align}\n  %q = xor i64 %a, %b\n  %q1 = and i64 %q, 1\n  %c = icmp eq i64 %q1, 0\n  %e = select i1 %c, {ty} %x, {ty} %y\n  %n = xor {ty} %y, 85\n  %r = cmpxchg ptr %p, {ty} %e, {ty} %n seq_cst seq_cst\n  %v = extractvalue {{ {ty}, i1 }} %r, 0\n  %f = extractvalue {{ {ty}, i1 }} %r, 1\n  %now = load atomic {ty}, ptr %p seq_cst, align {align}\n  %m = mul {ty} %v, 31\n  %z = add {ty} %m, %now\n{}  %fz = zext i1 %f to i64\n  %fm = mul i64 %fz, 1311768467463790320\n  %s = add i64 %o, %fm\n  ret i64 %s\n",
                int_prelude(width),
                int_fold(width, "%z")
            ),
        );
    }
}

/// A `[20 x i64]` named `%{name}v19`: element k is `seed + k`, or `seed * (k + 1)` when `scaled`.
fn large_value(name: &str, seed: &str, scaled: bool) -> String {
    let mut text = String::new();
    for index in 0..20 {
        let operation = if scaled {
            format!("mul i64 {seed}, {}", index + 1)
        } else {
            format!("add i64 {seed}, {index}")
        };
        let _ = writeln!(text, "  %{name}t{index} = {operation}");
        let from = if index == 0 {
            "undef".to_owned()
        } else {
            format!("%{name}v{}", index - 1)
        };
        let _ = writeln!(
            text,
            "  %{name}v{index} = insertvalue [20 x i64] {from}, i64 %{name}t{index}, {index}"
        );
    }
    text
}

fn value_cases(suite: &mut Suite) {
    let group = "values";
    let mut add = |label: &str, declarations: Vec<String>, body: String| {
        suite.add(group, label.to_owned(), Kind::Int, declarations, body);
    };
    let x = large_value("x", "%a", false);
    let y = large_value("y", "%b", true);
    add(
        "insertvalue keeps its operand",
        vec![],
        format!(
            "entry:\n{x}  %x2 = insertvalue [20 x i64] %xv19, i64 %b, 3\n  %e1 = extractvalue [20 x i64] %xv19, 3\n  %e2 = extractvalue [20 x i64] %x2, 3\n  %e3 = extractvalue [20 x i64] %x2, 4\n  %m = mul i64 %e1, 31\n  %n = add i64 %m, %e2\n  %p = mul i64 %n, 31\n  %o = add i64 %p, %e3\n  ret i64 %o\n"
        ),
    );
    add(
        "a load of a large value is a copy",
        vec![],
        format!(
            "entry:\n{x}  %p = alloca [20 x i64], align 8\n  store [20 x i64] %xv19, ptr %p, align 8\n  %v = load [20 x i64], ptr %p, align 8\n  %z = insertvalue [20 x i64] %xv19, i64 %b, 0\n  store [20 x i64] %z, ptr %p, align 8\n  %w = load [20 x i64], ptr %p, align 8\n  %e0 = extractvalue [20 x i64] %v, 0\n  %e1 = extractvalue [20 x i64] %w, 0\n  %m = mul i64 %e0, 31\n  %o = add i64 %m, %e1\n  ret i64 %o\n"
        ),
    );
    add(
        "select of large values",
        vec![],
        format!(
            "entry:\n{x}{y}  %c = icmp slt i64 %a, %b\n  %s = select i1 %c, [20 x i64] %xv19, [20 x i64] %yv19\n  %t = insertvalue [20 x i64] %s, i64 %a, 5\n  %e0 = extractvalue [20 x i64] %s, 5\n  %e1 = extractvalue [20 x i64] %t, 5\n  %e2 = extractvalue [20 x i64] %t, 19\n  %m = mul i64 %e0, 31\n  %n = add i64 %m, %e1\n  %p = mul i64 %n, 31\n  %o = add i64 %p, %e2\n  ret i64 %o\n"
        ),
    );
    add(
        "conditional edges into a large phi",
        vec![],
        format!(
            "entry:\n{x}{y}  %c = icmp slt i64 %a, %b\n  br i1 %c, label %join, label %other\nother:\n  %d = icmp eq i64 %a, 0\n  br i1 %d, label %join, label %third\nthird:\n  br label %join\njoin:\n  %v = phi [20 x i64] [ %xv19, %entry ], [ %yv19, %other ], [ %xv19, %third ]\n  %e = extractvalue [20 x i64] %v, 7\n  %f = extractvalue [20 x i64] %v, 19\n  %m = mul i64 %e, 31\n  %o = add i64 %m, %f\n  ret i64 %o\n"
        ),
    );
    add(
        "a switch into a large phi",
        vec![],
        format!(
            "entry:\n{x}{y}  %k = and i64 %a, 3\n  switch i64 %k, label %join [\n    i64 0, label %join\n    i64 1, label %mid\n    i64 2, label %mid2\n  ]\nmid:\n  %z = insertvalue [20 x i64] %yv19, i64 %b, 7\n  br label %join\nmid2:\n  br label %join\njoin:\n  %v = phi [20 x i64] [ %xv19, %entry ], [ %xv19, %entry ], [ %z, %mid ], [ %yv19, %mid2 ]\n  %e = extractvalue [20 x i64] %v, 7\n  %f = extractvalue [20 x i64] %v, 19\n  %m = mul i64 %e, 31\n  %o = add i64 %m, %f\n  ret i64 %o\n"
        ),
    );
    add(
        "a large record of doubles, integers and an array",
        vec!["%t.big = type { [10 x double], [10 x i32], i64 }".to_owned()],
        "entry:\n  %p = alloca %t.big, align 16\n  %d0 = getelementptr %t.big, ptr %p, i32 0, i32 0, i64 0\n  %d9 = getelementptr %t.big, ptr %p, i32 0, i32 0, i64 9\n  %i0 = getelementptr %t.big, ptr %p, i32 0, i32 1, i64 0\n  %i9 = getelementptr %t.big, ptr %p, i32 0, i32 1, i64 9\n  %l = getelementptr %t.big, ptr %p, i32 0, i32 2\n  %fa = sitofp i64 %a to double\n  %fb = sitofp i64 %b to double\n  %ta = trunc i64 %a to i32\n  %tb = trunc i64 %b to i32\n  store double %fa, ptr %d0\n  store double %fb, ptr %d9\n  store i32 %ta, ptr %i0\n  store i32 %tb, ptr %i9\n  store i64 %b, ptr %l\n  %v = load %t.big, ptr %p\n  %c = icmp slt i64 %a, %b\n  %s = select i1 %c, %t.big %v, %t.big zeroinitializer\n  %e0 = extractvalue %t.big %s, 0, 0\n  %e1 = extractvalue %t.big %s, 0, 9\n  %e2 = extractvalue %t.big %s, 1, 0\n  %e3 = extractvalue %t.big %s, 1, 9\n  %e4 = extractvalue %t.big %s, 2\n  %b0 = bitcast double %e0 to i64\n  %b1 = bitcast double %e1 to i64\n  %z2 = zext i32 %e2 to i64\n  %z3 = zext i32 %e3 to i64\n  %h0 = mul i64 %b0, 31\n  %h1 = add i64 %h0, %b1\n  %h2 = mul i64 %h1, 31\n  %h3 = add i64 %h2, %z2\n  %h4 = mul i64 %h3, 31\n  %h5 = add i64 %h4, %z3\n  %h6 = mul i64 %h5, 31\n  %o = add i64 %h6, %e4\n  ret i64 %o\n".to_owned(),
    );
    add(
        "qsort with a callback",
        vec![
            "declare void @qsort(ptr, i64, i64, ptr)".to_owned(),
            "define internal i32 @cmp64(ptr %p, ptr %q) {\n  %x = load i64, ptr %p\n  %y = load i64, ptr %q\n  %lt = icmp slt i64 %x, %y\n  %gt = icmp sgt i64 %x, %y\n  %l = zext i1 %lt to i32\n  %g = zext i1 %gt to i32\n  %r = sub i32 %g, %l\n  ret i32 %r\n}".to_owned(),
        ],
        {
            let mut text = String::from("entry:\n  %buf = alloca [8 x i64], align 8\n  %v2 = xor i64 %a, %b\n  %v3 = add i64 %a, %b\n  %v4 = sub i64 %a, %b\n  %v5 = mul i64 %a, 3\n  %v6 = mul i64 %b, 5\n  %v7 = or i64 %a, %b\n");
            for (index, value) in ["%a", "%b", "%v2", "%v3", "%v4", "%v5", "%v6", "%v7"].iter().enumerate() {
                let _ = writeln!(text, "  %s{index} = getelementptr [8 x i64], ptr %buf, i64 0, i64 {index}");
                let _ = writeln!(text, "  store i64 {value}, ptr %s{index}");
            }
            text.push_str("  call void @qsort(ptr %buf, i64 8, i64 8, ptr @cmp64)\n");
            let mut previous = String::from("0");
            for index in 0..8 {
                let _ = writeln!(text, "  %l{index} = load i64, ptr %s{index}");
                let _ = writeln!(text, "  %m{index} = mul i64 {previous}, 31");
                let _ = writeln!(text, "  %h{index} = add i64 %m{index}, %l{index}");
                previous = format!("%h{index}");
            }
            let _ = write!(text, "  %o = add i64 {previous}, 0\n  ret i64 %o\n");
            text
        },
    );
    add(
        "calls through a table in the data",
        vec![
            "define internal i64 @gi0(i64 %a, i64 %b) {\n  %r = add i64 %a, %b\n  ret i64 %r\n}".to_owned(),
            "define internal i64 @gi1(i64 %a, i64 %b) {\n  %r = sub i64 %a, %b\n  ret i64 %r\n}".to_owned(),
            "@gstr = internal constant [6 x i8] c\"hello\\00\"".to_owned(),
            "@gp = internal constant ptr getelementptr inbounds ([6 x i8], ptr @gstr, i64 0, i64 2)".to_owned(),
            "@gtab = internal constant [3 x { ptr, i64 }] [{ ptr, i64 } { ptr @gi0, i64 11 }, { ptr, i64 } { ptr @gi1, i64 22 }, { ptr, i64 } { ptr @gstr, i64 33 }]".to_owned(),
        ],
        "entry:\n  %i = urem i64 %a, 3\n  %pf = getelementptr [3 x { ptr, i64 }], ptr @gtab, i64 0, i64 %i, i32 0\n  %fp = load ptr, ptr %pf\n  %pn = getelementptr [3 x { ptr, i64 }], ptr @gtab, i64 0, i64 %i, i32 1\n  %n = load i64, ptr %pn\n  %isf = icmp ult i64 %i, 2\n  br i1 %isf, label %call, label %data\ncall:\n  %r1 = call i64 %fp(i64 %a, i64 %b)\n  br label %join\ndata:\n  %ch = load i8, ptr %fp\n  %r2 = zext i8 %ch to i64\n  br label %join\njoin:\n  %r = phi i64 [ %r1, %call ], [ %r2, %data ]\n  %q = load ptr, ptr @gp\n  %c = load i8, ptr %q\n  %cz = zext i8 %c to i64\n  %m = add i64 %r, %n\n  %m2 = mul i64 %m, 31\n  %o = add i64 %m2, %cz\n  ret i64 %o\n".to_owned(),
    );
    add(
        "thread-local storage with the intrinsic",
        vec![
            "@tl = internal thread_local global i64 7".to_owned(),
            "declare ptr @llvm.threadlocal.address.p0(ptr)".to_owned(),
        ],
        "entry:\n  %p = call ptr @llvm.threadlocal.address.p0(ptr @tl)\n  %v = load i64, ptr %p\n  %n = add i64 %v, %a\n  store i64 %n, ptr %p\n  %q = call ptr @llvm.threadlocal.address.p0(ptr @tl)\n  %w = load i64, ptr %q\n  %m = mul i64 %w, 3\n  %o = sub i64 %m, %b\n  ret i64 %o\n".to_owned(),
    );
    add(
        "thread-local storage by its name",
        vec!["@tl2 = internal thread_local global i64 9".to_owned()],
        "entry:\n  %v = load i64, ptr @tl2\n  %n = add i64 %v, %a\n  store i64 %n, ptr @tl2\n  %w = load i64, ptr @tl2\n  %m = mul i64 %w, 5\n  %o = sub i64 %m, %b\n  ret i64 %o\n".to_owned(),
    );
    add(
        "pointers and integers",
        vec![],
        "entry:\n  %p = inttoptr i64 %a to ptr\n  %q = ptrtoint ptr %p to i64\n  %t = ptrtoint ptr %p to i32\n  %u = ptrtoint ptr %p to i128\n  %r = inttoptr i64 %b to ptr\n  %c = icmp ult ptr %p, %r\n  %cz = zext i1 %c to i64\n  %tz = zext i32 %t to i64\n  %uh = lshr i128 %u, 64\n  %uht = trunc i128 %uh to i64\n  %s = select i1 %c, ptr %p, ptr %r\n  %sq = ptrtoint ptr %s to i64\n  %m0 = mul i64 %tz, 3\n  %m1 = mul i64 %cz, 5\n  %m2 = mul i64 %uht, 7\n  %m3 = mul i64 %sq, 11\n  %s1 = add i64 %q, %m0\n  %s2 = add i64 %s1, %m1\n  %s3 = add i64 %s2, %m2\n  %o = add i64 %s3, %m3\n  ret i64 %o\n".to_owned(),
    );
    for width in [8u32, 16, 32] {
        add(
            &format!("an index of i{width} that is negative"),
            vec!["declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)".to_owned()],
            format!(
                "entry:\n  %buf = alloca [64 x i32], align 16\n  call void @llvm.memset.p0.i64(ptr %buf, i8 0, i64 256, i1 false)\n  %mid = getelementptr [64 x i32], ptr %buf, i64 0, i64 32\n  %k0 = trunc i64 %a to i{width}\n  %k1 = and i{width} %k0, 7\n  %k = sub i{width} %k1, 4\n  %p = getelementptr i32, ptr %mid, i{width} %k\n  %tv = trunc i64 %b to i32\n  store i32 %tv, ptr %p, align 4\n  %kw = sext i{width} %k to i64\n  %q = getelementptr i32, ptr %mid, i64 %kw\n  %l = load i32, ptr %q, align 4\n  %nk = add i64 %kw, 1\n  %q2 = getelementptr i32, ptr %mid, i64 %nk\n  %l2 = load i32, ptr %q2, align 4\n  %z = zext i32 %l to i64\n  %z2 = zext i32 %l2 to i64\n  %m = mul i64 %z, 31\n  %n = add i64 %m, %z2\n  %o = add i64 %n, %kw\n  ret i64 %o\n"
            ),
        );
    }
    add(
        "constants of every kind",
        vec![],
        format!(
            "{}  %r1 = fadd double %x, 0xBFF8000000000000\n  %r2 = fmul double %r1, 0x7FF0000000000000\n  %r3 = fsub double %r2, 0x0000000000000001\n  %r4 = fadd double %r3, 1.5e+300\n  %s = fptrunc double %x to float\n  %s1 = fadd float %s, 0x3FF8000000000000\n  %s2 = fmul float %s1, 0x47EFFFFFE0000000\n  %sb = bitcast float %s2 to i32\n  %sz = zext i32 %sb to i64\n  %rb = bitcast double %r4 to i64\n  %o = xor i64 %rb, %sz\n  ret i64 %o\n",
            float_prelude("double")
        ),
    );
    add(
        "vectors loaded from constants",
        vec![
            "@vk = internal constant <16 x i8> <i8 1, i8 2, i8 3, i8 4, i8 5, i8 6, i8 7, i8 8, i8 9, i8 10, i8 11, i8 12, i8 13, i8 14, i8 15, i8 16>".to_owned(),
            "@vtext = internal constant [32 x i8] c\"abcdefghijklmnopqrstuvwxyz012345\"".to_owned(),
        ],
        format!("{}  %k = load <16 x i8>, ptr @vk, align 16\n  %s = load <16 x i8>, ptr getelementptr (i8, ptr @vtext, i64 5), align 1\n  %m = add <16 x i8> %x, %k\n  %n = xor <16 x i8> %m, %s\n  %t = select <16 x i1> <i1 true, i1 false, i1 true, i1 true, i1 false, i1 false, i1 true, i1 false, i1 true, i1 true, i1 false, i1 true, i1 false, i1 false, i1 true, i1 true>, <16 x i8> %n, <16 x i8> %y\n  %r = insertelement <16 x i8> %t, i8 99, i32 3\n{}  ret i64 %o\n", vector_prelude("<16 x i8>"), vector_fold("<16 x i8>", "%r")),
    );
}

fn generate() -> Suite {
    let mut suite = Suite::default();
    value_cases(&mut suite);
    control_cases(&mut suite);
    call_cases(&mut suite);
    memory_cases(&mut suite);
    atomic_cases(&mut suite);
    for width in [1, 7, 8, 16, 24, 32, 48, 64, 128] {
        int_cases(&mut suite, width);
    }
    float_cases(&mut suite, "float");
    float_cases(&mut suite, "double");
    for (lanes, bits) in [(16, 8), (8, 16), (4, 32), (2, 64)] {
        int_vector_cases(&mut suite, lanes, bits);
    }
    float_vector_cases(&mut suite, 4, "float");
    float_vector_cases(&mut suite, 2, "double");
    suite
}

// ---- the harness ----

const INTEGERS: [u64; 30] = [
    0,
    1,
    u64::MAX,
    2,
    3,
    7,
    0x7f,
    0x80,
    0xff,
    0x100,
    0x7fff,
    0x8000,
    0xffff,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
    0x1_0000_0000,
    i64::MAX as u64,
    i64::MIN as u64,
    0x0123_4567_89ab_cdef,
    0xfedc_ba98_7654_3210,
    0xdead_beef_cafe_babe,
    31,
    32,
    63,
    64,
    65,
    127,
    128,
    0x5555_5555_5555_5555,
];

fn floats() -> Vec<u64> {
    let values: [f64; 36] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.5,
        3.0,
        0.1,
        0.3,
        255.5,
        256.0,
        -128.5,
        127.5,
        65535.5,
        65536.0,
        2147483648.0,
        4294967296.0,
        9007199254740993.0,
        9223372036854775808.0,
        -9223372036854775808.0,
        18446744073709551616.0,
        1e10,
        -1e10,
        1e300,
        -1e300,
        4.9e-324,
        2.2250738585072014e-308,
        1.7976931348623157e308,
        3.4028235e38,
        3.5e38,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut bits: Vec<u64> = values.iter().map(|value| value.to_bits()).collect();
    bits.extend([
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0x7ff8_0000_dead_beef,
    ]);
    bits
}

fn render(cases: &[&Case]) -> String {
    let mut declarations = BTreeSet::new();
    for case in cases {
        declarations.extend(case.declarations.iter().cloned());
    }
    let mut text = String::new();
    for declaration in &declarations {
        let _ = writeln!(text, "{declaration}");
    }
    for case in cases {
        let _ = write!(
            text,
            "define i64 @c{}(i64 %a, i64 %b) {{\n{}}}\n",
            case.id, case.body
        );
    }
    text
}

/// What a case is called in an error that names the function.
fn refused_case(message: &str) -> Option<usize> {
    let rest = message.split(" in @c").nth(1)?;
    rest.split(|character: char| !character.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn host_program(cases: &[&Case]) -> String {
    let mut text = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\ntypedef int64_t (*fn)(int64_t, int64_t);\n",
    );
    for case in cases {
        let _ = writeln!(text, "extern int64_t c{}(int64_t, int64_t);", case.id);
    }
    let table = |values: &[u64]| {
        values
            .iter()
            .map(|value| format!("(int64_t)0x{value:x}ULL"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(
        text,
        "static const int64_t INT[] = {{{}}};",
        table(&INTEGERS)
    );
    let _ = writeln!(
        text,
        "static const int64_t FLT[] = {{{}}};",
        table(&floats())
    );
    let _ = writeln!(
        text,
        "static const struct {{ const char *name; fn f; int kind; }} cases[] = {{"
    );
    for case in cases {
        let _ = writeln!(
            text,
            "  {{\"{}\", c{}, {}}},",
            case.label,
            case.id,
            i32::from(case.kind == Kind::Float)
        );
    }
    text.push_str(
        "};\n\
int main(int argc, char **argv) {\n\
  for (unsigned i = 0; i < sizeof cases / sizeof cases[0]; i++) {\n\
    if (argc > 1 && strcmp(argv[1], cases[i].name) != 0) continue;\n\
    const int64_t *table = cases[i].kind ? FLT : INT;\n\
    unsigned count = cases[i].kind ? sizeof FLT / sizeof FLT[0] : sizeof INT / sizeof INT[0];\n\
    uint64_t hash = 1469598103934665603ULL;\n\
    for (unsigned x = 0; x < count; x++) for (unsigned y = 0; y < count; y++) {\n\
      int64_t r = cases[i].f(table[x], table[y]);\n\
      if (argc > 1) printf(\"%016llx %016llx -> %016llx\\n\", (unsigned long long)table[x], (unsigned long long)table[y], (unsigned long long)r);\n\
      hash = (hash ^ (uint64_t)r) * 1099511628211ULL;\n\
    }\n\
    if (argc <= 1) printf(\"%s %016llx\\n\", cases[i].name, (unsigned long long)hash);\n\
  }\n\
  return 0;\n\
}\n",
    );
    text
}

fn run(command: &mut Command) -> String {
    let output = command.output().expect("the command starts");
    assert!(
        output.status.success(),
        "{command:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn work_directory(label: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-fast-ops-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The operations that the translator does not take yet, by label. Each is a refusal (E2002) that
/// the translator reports; none of them may change a result silently.
const KNOWN_REFUSALS: &[&str] = &["i128 smul.with.overflow", "i128 umul.with.overflow"];

/// `TSUZURI_FAST_OPS_EXPORT=DIR` stops the test after the translation and writes what a comparison on
/// another machine needs (a Linux container, say): the C host, the IR of each group, and the object the
/// translator makes for the machine `TSUZURI_FAST_OPS_TARGET=TRIPLE` names (this machine if unset).
/// There, Clang compiles `host.c` and the `g*.ll`, the two programs are linked from `host.o` and the
/// `g*.llvm.o` or the `g*.fast.o`, and their outputs are compared.
fn export_for_another_machine(
    directory: &Path,
    kept: &[&Case],
    objects: &[(String, Vec<u8>, String)],
) {
    use tsuzuri::target::{Platform, parse_target};
    let platform = match std::env::var("TSUZURI_FAST_OPS_TARGET") {
        Ok(name) => Platform::new(parse_target(&name).expect("a target").1),
        Err(_) => Platform::HOST,
    };
    let options = Options {
        platform,
        threads: 2,
        verify: true,
    };
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(directory.join("host.c"), host_program(kept)).unwrap();
    for (index, (_, _, text)) in objects.iter().enumerate() {
        std::fs::write(directory.join(format!("g{index}.ll")), text).unwrap();
        let object = backend::compile(text, &options)
            .unwrap_or_else(|error| panic!("g{index} for the other machine: {}", error.message));
        std::fs::write(directory.join(format!("g{index}.fast.o")), object).unwrap();
    }
}

#[test]
fn operations_agree_with_clang() {
    if Command::new(clang()).arg("--version").output().is_err() {
        return;
    }
    let suite = generate();
    let options = Options {
        verify: true,
        ..Options::host(2)
    };
    let mut groups: Vec<String> = Vec::new();
    for case in &suite.cases {
        if !groups.contains(&case.group) {
            groups.push(case.group.clone());
        }
    }
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    let mut kept: Vec<&Case> = Vec::new();
    let mut objects: Vec<(String, Vec<u8>, String)> = Vec::new();
    for group in &groups {
        let mut members: Vec<&Case> = suite
            .cases
            .iter()
            .filter(|case| &case.group == group)
            .collect();
        let object = loop {
            let text = render(&members);
            match backend::compile(&text, &options) {
                Ok(object) => break object,
                Err(error) => {
                    let id = refused_case(&error.message)
                        .unwrap_or_else(|| panic!("{group}: {}\n{}", error.message, text));
                    assert_eq!(error.code, "E2002", "{}", error.message);
                    let position = members.iter().position(|case| case.id == id).unwrap();
                    dropped.insert(members[position].label.clone());
                    members.remove(position);
                }
            }
        };
        let text = render(&members);
        kept.extend(members);
        objects.push((group.clone(), object, text));
    }
    let known: BTreeSet<String> = KNOWN_REFUSALS
        .iter()
        .map(|label| (*label).to_owned())
        .collect();
    assert_eq!(
        dropped, known,
        "the operations that the translator refuses changed"
    );

    if let Some(export) = std::env::var_os("TSUZURI_FAST_OPS_EXPORT") {
        export_for_another_machine(Path::new(&export), &kept, &objects);
        return;
    }

    let directory = work_directory("native");
    let host_source = directory.join("host.c");
    std::fs::write(&host_source, host_program(&kept)).unwrap();
    let host = directory.join("host.o");
    run(Command::new(clang())
        .args(["-O1", "-c"])
        .arg(&host_source)
        .arg("-o")
        .arg(&host));
    let mut llvm_objects: Vec<PathBuf> = Vec::new();
    let mut fast_objects: Vec<PathBuf> = Vec::new();
    for (index, (_, object, text)) in objects.iter().enumerate() {
        let source = directory.join(format!("g{index}.ll"));
        std::fs::write(&source, text).unwrap();
        let llvm_object = directory.join(format!("g{index}.llvm.o"));
        run(Command::new(clang())
            .args(["-x", "ir", "-O0", "-c", "-Wno-override-module"])
            .arg(&source)
            .arg("-o")
            .arg(&llvm_object));
        llvm_objects.push(llvm_object);
        let fast_object = directory.join(format!("g{index}.fast.o"));
        std::fs::write(&fast_object, object).unwrap();
        fast_objects.push(fast_object);
    }
    let link = |objects: &[PathBuf], output: &Path| {
        run(Command::new(clang())
            .arg(&host)
            .args(objects)
            .arg("-lm")
            .arg("-o")
            .arg(output));
    };
    let llvm_program = directory.join("llvm");
    let fast_program = directory.join("fast");
    link(&llvm_objects, &llvm_program);
    link(&fast_objects, &fast_program);
    let expected = run(&mut Command::new(&llvm_program));
    let actual = run(&mut Command::new(&fast_program));
    let mut mismatches = Vec::new();
    let mut compared = 0;
    for (want, got) in expected.lines().zip(actual.lines()) {
        compared += 1;
        if want != got {
            let label = want.rsplit_once(' ').map_or(want, |(label, _)| label);
            let detail = |program: &Path| run(Command::new(program).arg(label));
            let (left, right) = (detail(&llvm_program), detail(&fast_program));
            let first = left
                .lines()
                .zip(right.lines())
                .find(|(l, r)| l != r)
                .map(|(l, r)| format!("clang {l} / fast {r}"))
                .unwrap_or_default();
            mismatches.push(format!("{label}: {first}"));
        }
    }
    assert_eq!(expected.lines().count(), actual.lines().count());
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        mismatches.is_empty(),
        "{} of {compared} operations differ from Clang:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    assert!(compared > 500, "only {compared} operations were compared");
}

// ---- WebAssembly (D13) ----

fn tool(variable: &str, default: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| default.to_owned())
}

/// Runs both modules of every case in Node and compares what each returns for the same inputs.
const WASM_HOST: &str = r#"
const fs = require('fs');
const [llvmFile, fastFile, specFile] = process.argv.slice(2);
const spec = JSON.parse(fs.readFileSync(specFile, 'utf8'));
const load = (file) => {
  let instance;
  const env = {
    fmod: (a, b) => a % b,
    fmodf: (a, b) => Math.fround(a % b),
    // The C library's qsort, which some cases call with a comparison function of the module.
    qsort: (base, count, size, cmp) => {
      const memory = new Uint8Array(instance.exports.memory.buffer);
      const compare = instance.exports.__indirect_function_table.get(cmp);
      const order = Array.from({ length: count }, (_, index) => index)
        .sort((a, b) => compare(base + a * size, base + b * size));
      const copy = memory.slice(base, base + count * size);
      order.forEach((from, to) => memory.set(copy.subarray(from * size, (from + 1) * size), base + to * size));
    },
  };
  // The other functions of the C library are the host's: the same for both modules, so that they agree.
  const library = new Proxy(env, {
    get: (target, name) => {
      if (name in target) return target[name];
      const single = name.endsWith('f') && typeof Math[name.slice(0, -1)] === 'function';
      const base = Math[single ? name.slice(0, -1) : name];
      if (typeof base !== 'function') throw new Error('unexpected import ' + String(name));
      return single ? (...values) => Math.fround(base(...values)) : base;
    },
  });
  instance = new WebAssembly.Instance(new WebAssembly.Module(fs.readFileSync(file)), { env: library });
  return instance.exports;
};
const [clang, fast] = [load(llvmFile), load(fastFile)];
const mismatches = [];
let compared = 0;
// The payload and the sign of a NaN are not specified by WebAssembly, and an engine's tiers differ in them,
// so the NaNs of a float result are all one value.
const quiet = (value) => {
  const unsigned = BigInt.asUintN(64, value);
  if (((unsigned >> 52n) & 0x7ffn) === 0x7ffn && (unsigned & 0xfffffffffffffn) !== 0n) return 'nan64';
  if (unsigned >> 32n === 0n && ((unsigned >> 23n) & 0xffn) === 0xffn && (unsigned & 0x7fffffn) !== 0n) return 'nan32';
  return String(value);
};
const outcome = (instance, name, x, y, float) => {
  try {
    const value = instance[name](x, y);
    return float ? quiet(value) : String(value);
  } catch (error) {
    return 'trap: ' + String(error.message).split('\n')[0];
  }
};
for (const c of spec.cases) {
  const table = (c.kind ? spec.flt : spec.int).map((hex) => BigInt.asIntN(64, BigInt('0x' + hex)));
  let first = null;
  for (const x of table) {
    for (const y of table) {
      const [want, got] = [outcome(clang, c.name, x, y, c.kind), outcome(fast, c.name, x, y, c.kind)];
      if (want !== got && first === null) first = `${x.toString(16)} ${y.toString(16)} -> clang ${want} / fast ${got}`;
    }
  }
  compared++;
  if (first !== null) mismatches.push(`${c.label}: ${first}`);
}
console.log(JSON.stringify({ compared, mismatches }));
"#;

/// The operations that the wasm translator does not take yet, by label (E2002 each).
const KNOWN_WASM_REFUSALS: &[&str] = &[
    "i128 smul.with.overflow",
    "i128 umul.with.overflow",
    "float fma",
    "double fma",
    "<4 x float> fma",
    "<2 x double> fma",
];

#[test]
fn wasm_operations_agree_with_clang() {
    let (clang, wasm_ld) = (clang(), tool("TSUZURI_WASM_LD", "wasm-ld"));
    let usable = |program: &str, flag: &str| Command::new(program).arg(flag).output().is_ok();
    let node = tool("TSUZURI_NODE", "node");
    if !usable(&clang, "--version") || !usable(&wasm_ld, "--version") || !usable(&node, "--version")
    {
        return;
    }
    // The runtime that a program has (the routines of the compiler runtime for 128-bit integers) is part of
    // each module, as it is in a build.
    let runtime = include_str!("../src/runtime/wasm.ll");
    let suite = generate();
    let mut groups: Vec<String> = Vec::new();
    for case in &suite.cases {
        if !groups.contains(&case.group) {
            groups.push(case.group.clone());
        }
    }
    let directory = work_directory("wasm");
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    let mut reasons: BTreeSet<String> = BTreeSet::new();
    let mut kept: Vec<&Case> = Vec::new();
    let (mut llvm_objects, mut fast_objects): (Vec<PathBuf>, Vec<PathBuf>) =
        (Vec::new(), Vec::new());
    for (index, group) in groups.iter().enumerate() {
        let mut members: Vec<&Case> = suite
            .cases
            .iter()
            .filter(|case| &case.group == group)
            .collect();
        let (object, text) = loop {
            let text = format!("{}\n{runtime}", render(&members));
            match backend::compile_wasm(&text, 2, true) {
                Ok(object) => break (object, text),
                Err(error) => {
                    let id = refused_case(&error.message)
                        .unwrap_or_else(|| panic!("{group}: {}", error.message));
                    assert_eq!(error.code, "E2002", "{}", error.message);
                    let position = members.iter().position(|case| case.id == id).unwrap();
                    dropped.insert(members[position].label.clone());
                    reasons.insert(
                        error
                            .message
                            .split(" in @c")
                            .next()
                            .unwrap_or_default()
                            .to_owned(),
                    );
                    members.remove(position);
                }
            }
        };
        kept.extend(members);
        let source = directory.join(format!("g{index}.ll"));
        std::fs::write(&source, &text).unwrap();
        let llvm_object = directory.join(format!("g{index}.llvm.o"));
        run(Command::new(&clang)
            .args([
                "--target=wasm32-unknown-unknown",
                "-mbulk-memory",
                "-mno-simd128",
            ])
            .args(["-x", "ir", "-O0", "-c", "-Wno-override-module"])
            .arg(&source)
            .arg("-o")
            .arg(&llvm_object));
        llvm_objects.push(llvm_object);
        let fast_object = directory.join(format!("g{index}.fast.o"));
        std::fs::write(&fast_object, object).unwrap();
        fast_objects.push(fast_object);
    }
    let known: BTreeSet<String> = KNOWN_WASM_REFUSALS
        .iter()
        .map(|label| (*label).to_owned())
        .collect();
    assert_eq!(
        dropped,
        known,
        "the operations that the wasm translator refuses changed; the reasons:\n{}",
        reasons.iter().cloned().collect::<Vec<_>>().join("\n")
    );
    let link = |objects: &[PathBuf], output: &Path| {
        run(Command::new(&wasm_ld)
            .args([
                "--no-entry",
                "--export-all",
                "--export-table",
                "--allow-undefined",
                "-z",
                "stack-size=1048576",
            ])
            .args(objects)
            .arg("-o")
            .arg(output));
    };
    let (llvm_module, fast_module) = (directory.join("llvm.wasm"), directory.join("fast.wasm"));
    link(&llvm_objects, &llvm_module);
    link(&fast_objects, &fast_module);

    let table = |values: &[u64]| {
        values
            .iter()
            .map(|value| format!("\"{value:x}\""))
            .collect::<Vec<_>>()
            .join(",")
    };
    let cases = kept
        .iter()
        .map(|case| {
            format!(
                "{{\"name\":\"c{}\",\"label\":\"{}\",\"kind\":{}}}",
                case.id,
                case.label,
                i32::from(case.kind == Kind::Float)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let spec = directory.join("spec.json");
    std::fs::write(
        &spec,
        format!(
            "{{\"int\":[{}],\"flt\":[{}],\"cases\":[{cases}]}}",
            table(&INTEGERS),
            table(&floats())
        ),
    )
    .unwrap();
    let host = directory.join("host.js");
    std::fs::write(&host, WASM_HOST).unwrap();
    let report = run(Command::new(&node)
        .arg(&host)
        .arg(&llvm_module)
        .arg(&fast_module)
        .arg(&spec));
    let _ = std::fs::remove_dir_all(&directory);
    let compared = report
        .split("\"compared\":")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|number| number.parse::<usize>().ok())
        .unwrap_or(0);
    assert!(
        report.contains("\"mismatches\":[]"),
        "operations differ from Clang on wasm32:\n{}",
        report.replace("\",\"", "\"\n\"")
    );
    assert!(compared > 500, "only {compared} operations were compared");
}
