//! Function multiversioning for `@cpu` (F08 Phase 3). A native build compiles a marked
//! function once more for each CPU level it names, with that level's LLVM target features, and
//! replaces it with a stub that runs the version `tsuzuri_cpu_pick` (`src/runtime/cpu.c`) picks.

use super::*;
use crate::syntax::CPU_TARGETS;

/// The define-line attribute of a `@cpu` function: `"tz-cpu"="<levels>:<function id>"`.
pub(super) const MARK: &str = "\"tz-cpu\"=";

/// The LLVM target features of a level of `CPU_TARGETS`.
fn features(level: u8) -> &'static str {
    match level {
        1 => "+sse4.2",
        2 => "+avx2",
        3 => "+avx512f,+avx512bw,+avx512cd,+avx512dq,+avx512vl",
        4 => "+sve",
        _ => "+sve2",
    }
}

fn level_name(level: u8) -> &'static str {
    CPU_TARGETS
        .iter()
        .find(|(_, target)| *target == level)
        .map_or("baseline", |(name, _)| name)
}

/// The levels `src/runtime/cpu.c` detects where this compiler runs: x86-64 outside Windows,
/// and AArch64 Linux. Elsewhere a `@cpu` function keeps only its portable version.
pub(super) fn host_levels() -> u8 {
    if cfg!(all(target_arch = "x86_64", not(windows))) {
        0b1110
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        0b11_0000
    } else {
        0
    }
}

/// `@name` or `@"name"` as the quoted name of its version `suffix`.
fn versioned(name: &str, suffix: &str) -> String {
    let inner = &name[1..];
    let inner = inner
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(inner);
    format!("@\"{inner}.cpu{suffix}\"")
}

/// The call on an instruction line, if any.
fn call_of(line: &str) -> Option<traps::Callable<'_>> {
    line.contains("call ")
        .then(|| traps::callable(line, false))
        .flatten()
}

/// Replaces the functions marked by `FunctionEmitter::emit` with their versions for the
/// `levels` (bit `L` for level `L`) they name. The versions that run with other instruction
/// sets pass vectors wider than 128 bits in other registers, so every function that such a
/// version calls with one gets a version of the same level too.
pub(super) fn multiversion(
    ir: String,
    module: &CheckedModule,
    levels: u8,
) -> Result<String, Diagnostic> {
    if !ir.contains(MARK) {
        return Ok(ir);
    }
    let mut bodies = BTreeMap::new();
    let mut start = None;
    let mut offset = 0;
    for line in ir.split_inclusive('\n') {
        if line.starts_with("define ") {
            start = Some(offset);
        }
        offset += line.len();
        if line.starts_with('}')
            && let Some(begin) = start.take()
            && let Some(callable) = traps::callable(&ir[begin..offset], true)
        {
            bodies.insert(callable.name, &ir[begin..offset]);
        }
    }
    let wide = simd::wide_types(&ir);
    // Each marked function's levels and the functions that need versions with it.
    let mut roots = BTreeMap::new();
    let mut members: BTreeMap<&str, u8> = BTreeMap::new();
    for (&name, &text) in &bodies {
        let define = text.lines().next().unwrap_or_default();
        let Some((_, mark)) = define.split_once(MARK) else {
            continue;
        };
        let (marked, id) = mark
            .trim_start_matches('"')
            .split_once('"')
            .and_then(|(mark, _)| mark.split_once(':'))
            .and_then(|(marked, id)| Some((marked.parse::<u8>().ok()?, id.parse::<usize>().ok()?)))
            .expect("FunctionEmitter::emit writes the levels and the function id");
        let chosen = marked & levels;
        roots.insert(name, chosen);
        if chosen == 0 {
            continue;
        }
        let mut pending = vec![name];
        let mut seen = BTreeSet::from([name]);
        while let Some(function) = pending.pop() {
            for line in bodies[function].lines().skip(1) {
                let Some(call) = call_of(line) else {
                    continue;
                };
                if !simd::holds_wide_vector(&line[..=call.close], &wide) {
                    continue;
                }
                if !bodies.contains_key(call.name) {
                    return Err(Diagnostic::new(
                        "E1005",
                        "a '@cpu' function cannot pass a SIMD vector wider than 128 bits through a function value or an extern: its versions pass such vectors in different registers; call a named Tsuzuri function",
                        module.functions[id].span,
                    ));
                }
                if seen.insert(call.name) {
                    pending.push(call.name);
                    if call.name != name {
                        *members.entry(call.name).or_default() |= chosen;
                    }
                }
            }
        }
    }
    // The version of each function at each level, for renaming calls in other versions.
    let has_version = |name: &str, level: u8| {
        roots
            .get(name)
            .or_else(|| members.get(name))
            .is_some_and(|levels| levels & (1 << level) != 0)
    };
    let mut output = String::with_capacity(ir.len() * 2);
    let mut choices = String::new();
    let mut metadata = String::new();
    let mut next_metadata = ir
        .lines()
        .filter_map(|line| {
            line.strip_prefix('!')?
                .split_once(" = ")?
                .0
                .parse::<usize>()
                .ok()
        })
        .max()
        .map_or(0, |id| id + 1);
    let mut start = None;
    for line in ir.split_inclusive('\n') {
        if line.starts_with("define ") {
            start = Some(output.len());
        }
        output.push_str(line);
        if !line.starts_with('}') {
            continue;
        }
        let Some(begin) = start.take() else {
            continue;
        };
        let text = output.split_off(begin);
        let Some(callable) = traps::callable(&text, true) else {
            output.push_str(&text);
            continue;
        };
        let name = callable.name;
        let Some(&chosen) = roots.get(name) else {
            output.push_str(&text);
            continue;
        };
        let baseline = versioned(name, ".baseline");
        if chosen == 0 {
            output.push_str(&version(&text, name, name, None, &|_, _| None));
            continue;
        }
        let choice = versioned(name, "");
        let _ = writeln!(choices, "{choice} = internal global i32 -1");
        // The stub gets a subprogram of its own, so the portable version, which keeps the
        // original one, inlines into it with an inline location.
        let debug = text
            .lines()
            .next()
            .and_then(|define| define.split_once(" !dbg !"))
            .and_then(|(_, id)| id.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|id| {
                let prefix = format!("!{id} = distinct !DISubprogram(");
                ir.lines().find(|line| line.starts_with(&prefix))
            })
            .map(|subprogram| {
                let (_, fields) = subprogram.split_once(" = ").unwrap();
                let fields = match fields.split_once(", retainedNodes: !") {
                    Some((head, tail)) => format!(
                        "{head}{}",
                        tail.trim_start_matches(|c: char| c.is_ascii_digit())
                    ),
                    None => fields.to_owned(),
                };
                let line = fields
                    .split_once(" line: ")
                    .and_then(|(_, line)| line.split(|c: char| !c.is_ascii_digit()).next())
                    .unwrap_or("0");
                let ids = (next_metadata, next_metadata + 1);
                next_metadata += 2;
                let _ = writeln!(
                    metadata,
                    "!{} = {fields}\n!{} = !DILocation(line: {line}, scope: !{})",
                    ids.0, ids.1, ids.0
                );
                ids
            });
        output.push_str(&stub(&text, name, &choice, chosen, &baseline, debug));
        output.push_str(&version(&text, name, &baseline, None, &|callee, _| {
            (callee == name).then(|| baseline.clone())
        }));
        for level in (1..8).filter(|level| chosen & (1 << level) != 0) {
            output.push_str(&version_at(&text, name, level, &wide, &has_version));
        }
    }
    // The runtime text at the end may lack a final newline; the driver finds `cpu.c`'s use by a
    // line that starts with the declaration.
    if !output.ends_with('\n') {
        output.push('\n');
    }
    for (&name, &chosen) in &members {
        for level in (1..8).filter(|level| chosen & (1 << level) != 0) {
            output.push_str(&version_at(bodies[name], name, level, &wide, &has_version));
        }
    }
    output.push_str(&choices);
    output.push_str(&metadata);
    if !output.contains("declare i32 @tsuzuri_cpu_pick(i64)") && !choices.is_empty() {
        output.push_str("declare i32 @tsuzuri_cpu_pick(i64)\n");
    }
    Ok(output)
}

/// The version of `name` for `level`: its calls that pass wide vectors, and its calls to
/// itself, go to the versions of the same level.
fn version_at(
    text: &str,
    name: &str,
    level: u8,
    wide: &BTreeSet<&str>,
    has_version: &dyn Fn(&str, u8) -> bool,
) -> String {
    let suffix = format!(".{}", level_name(level));
    version(
        text,
        name,
        &versioned(name, &suffix),
        Some(level),
        &|callee, line| {
            ((callee == name || simd::holds_wide_vector(line, wide)) && has_version(callee, level))
                .then(|| versioned(callee, &suffix))
        },
    )
}

/// `text`, the function `name`, renamed to `renamed` without the `@cpu` mark. A version for a
/// level has that level's target features and no debug information, which belongs to the
/// portable version. `rename` maps a callee and its call line to the callee to call instead.
fn version(
    text: &str,
    name: &str,
    renamed: &str,
    level: Option<u8>,
    rename: &dyn Fn(&str, &str) -> Option<String>,
) -> String {
    let mut output = String::with_capacity(text.len());
    let mut lines = text.split_inclusive('\n');
    let define = lines.next().unwrap_or_default();
    let mut define = define.replacen(name, renamed, 1);
    if let Some((head, mark)) = define.split_once(&format!(" {MARK}")) {
        let tail = mark
            .trim_start_matches('"')
            .split_once('"')
            .map_or("", |(_, tail)| tail);
        define = format!("{head}{tail}");
    }
    if let Some(level) = level {
        define = without_debug(&define, " !dbg !");
        let body = define
            .trim_end()
            .strip_suffix('{')
            .unwrap_or(&define)
            .trim_end();
        define = format!("{body} \"target-features\"=\"{}\" {{\n", features(level));
    }
    output.push_str(&define);
    for line in lines {
        if level.is_some()
            && (line.contains("@llvm.dbg.declare(") || line.contains("@llvm.dbg.value("))
        {
            continue;
        }
        let line = if level.is_some() {
            without_debug(line, ", !dbg !")
        } else {
            line.to_owned()
        };
        match call_of(&line).and_then(|call| {
            let replacement = rename(call.name, &line[..=call.close])?;
            let at = call.name.as_ptr() as usize - line.as_ptr() as usize;
            Some((at, call.name.len(), replacement))
        }) {
            Some((at, length, replacement)) => {
                output.push_str(&line[..at]);
                output.push_str(&replacement);
                output.push_str(&line[at + length..]);
            }
            None => output.push_str(&line),
        }
    }
    output
}

/// `line` without each `marker` and the metadata number after it.
fn without_debug(line: &str, marker: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(marker) {
        output.push_str(&rest[..at]);
        rest = rest[at + marker.len()..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    output.push_str(rest);
    output
}

/// The function that keeps the name of the marked function: it calls the version that
/// `tsuzuri_cpu_pick` picks once for the CPU and `TSUZURI_CPU_FORCE`. `debug` is the stub's
/// subprogram and the location of its calls.
fn stub(
    text: &str,
    name: &str,
    choice: &str,
    levels: u8,
    baseline: &str,
    debug: Option<(usize, usize)>,
) -> String {
    let define = text.lines().next().unwrap_or_default();
    let callable = traps::callable(define, true).expect("a define line names its function");
    let at = callable.name.as_ptr() as usize - define.as_ptr() as usize;
    let result = define[..at]
        .strip_prefix("define internal ")
        .unwrap_or(&define[..at])
        .trim();
    let parameters = &define[callable.open + 1..callable.close];
    let (subprogram, location) = debug.map_or_else(
        || (String::new(), String::new()),
        |(subprogram, location)| {
            (
                format!(" !dbg !{subprogram}"),
                format!(", !dbg !{location}"),
            )
        },
    );
    let mut stub = format!(
        "define internal {result} {name}({parameters}) nounwind{subprogram} {{\n\
         entry:\n  \
         %tz.choice = load atomic i32, ptr {choice} monotonic, align 4\n  \
         %tz.known = icmp sge i32 %tz.choice, 0\n  \
         br i1 %tz.known, label %tz.dispatch, label %tz.pick\n\
         tz.pick:\n  \
         %tz.picked = call i32 @tsuzuri_cpu_pick(i64 {levels}){location}\n  \
         store atomic i32 %tz.picked, ptr {choice} monotonic, align 4\n  \
         br label %tz.dispatch\n\
         tz.dispatch:\n  \
         %tz.level = phi i32 [ %tz.choice, %entry ], [ %tz.picked, %tz.pick ]\n  \
         switch i32 %tz.level, label %tz.level0 [\n"
    );
    let chosen: Vec<u8> = (1..8).filter(|level| levels & (1 << level) != 0).collect();
    for level in &chosen {
        let _ = writeln!(stub, "    i32 {level}, label %tz.level{level}");
    }
    stub.push_str("  ]\n");
    for level in chosen.into_iter().chain([0]) {
        let callee = if level == 0 {
            baseline.to_owned()
        } else {
            versioned(name, &format!(".{}", level_name(level)))
        };
        let _ = writeln!(stub, "tz.level{level}:");
        if result == "void" {
            let _ = writeln!(
                stub,
                "  tail call void {callee}({parameters}){location}\n  ret void"
            );
        } else {
            let _ = writeln!(
                stub,
                "  %tz.result{level} = tail call {result} {callee}({parameters}){location}\n  ret {result} %tz.result{level}"
            );
        }
    }
    stub.push_str("}\n\n");
    stub
}
