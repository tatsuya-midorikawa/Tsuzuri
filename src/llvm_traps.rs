use super::*;
use crate::trap::{TrapKind, TrapSite, TrapSource};

#[derive(Default)]
pub(super) struct Marks {
    pub instructions: BTreeMap<usize, (Span, Option<TrapKind>)>,
    pub sources: BTreeMap<String, (Span, bool)>,
}

impl Marks {
    pub fn source(&mut self, ir: &str, function: &CheckedFunction) {
        for line in ir.lines().filter(|line| line.starts_with("define ")) {
            if let Some(call) = callable(line, true) {
                let inherited = function.origin.module == ModuleOrigin::Std
                    || matches!(
                        function.module.as_str(),
                        "$builtin" | "$intrinsic" | "$to_string" | "$case"
                    )
                    || call.name.starts_with("@tz.apply.")
                    || call.name.starts_with("@tz.env.");
                self.sources
                    .insert(call.name.into(), (function.span, inherited));
            }
        }
    }
}

#[derive(Debug)]
pub struct EmitOutput {
    pub ir: String,
    pub trap_sites: Vec<TrapSite>,
    /// Whether the sites have a stable numbering (`TrapNumbering::Stable`), which a build from codegen units needs.
    pub stable_sites: bool,
}

pub(super) struct Callable<'a> {
    pub name: &'a str,
    pub open: usize,
    pub close: usize,
}

pub(super) fn callable(line: &str, definition: bool) -> Option<Callable<'_>> {
    let bytes = line.as_bytes();
    let mut cursor = if definition {
        line.find('@')?
    } else {
        line.find("call ")? + 5
    };
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            cursor = quoted_end(bytes, cursor)?;
        } else if matches!(bytes[cursor], b'@' | b'%') {
            let start = cursor;
            cursor += 1;
            if bytes.get(cursor) == Some(&b'"') {
                cursor = quoted_end(bytes, cursor)?;
            } else {
                while bytes.get(cursor).is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$' | b'-')
                }) {
                    cursor += 1;
                }
            }
            let end = cursor;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'(') {
                let open = cursor;
                let mut depth = 1;
                cursor += 1;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'"' => {
                            cursor = quoted_end(bytes, cursor)?;
                            continue;
                        }
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(Callable {
                                    name: &line[start..end],
                                    open,
                                    close: cursor,
                                });
                            }
                        }
                        _ => {}
                    }
                    cursor += 1;
                }
                return None;
            }
        } else {
            cursor += 1;
        }
    }
    None
}

pub(super) fn quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => return Some(cursor + 1),
            b'\\' => cursor += 1,
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn append_context(line: &str, call: &Callable<'_>, context: &str) -> String {
    let separator = if line[call.open + 1..call.close].trim().is_empty() {
        ""
    } else {
        ", "
    };
    let mut output = format!("{}{separator}i32 {context})", &line[..call.close]);
    let suffix = &line[call.close + 1..];
    let mut cursor = 0;
    while cursor < suffix.len() {
        if suffix.as_bytes()[cursor] == b'#'
            && suffix
                .as_bytes()
                .get(cursor + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            cursor += 1;
            while suffix
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_digit)
            {
                cursor += 1;
            }
        } else {
            let character = suffix[cursor..].chars().next().unwrap();
            output.push(character);
            cursor += character.len_utf8();
        }
    }
    output
}

fn marked<'a>(line: &'a str, marks: &Marks) -> (&'a str, Option<(Span, Option<TrapKind>)>) {
    let Some((line, marker)) = line.rsplit_once(", !tz.site !") else {
        return (line, None);
    };
    (
        line,
        marker
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|id| marks.instructions.get(&id).copied()),
    )
}

fn runtime_kind(function: &str) -> TrapKind {
    if function == "@tz.stack.check" {
        TrapKind::StackOverflow
    } else if function.contains("builtin.$requires") {
        TrapKind::ContractRequires
    } else if function.contains("builtin.$ensures") {
        TrapKind::ContractEnsures
    } else if function.contains("builtin.assert") || function.starts_with("@tz.mutex.") {
        TrapKind::Assert
    } else if function.contains("builtin.unreachable") {
        TrapKind::MatchFailure
    } else if matches!(
        function,
        "@tz.alloc"
            | "@tz.realloc"
            | "@tz.alloc.base"
            | "@tz.realloc.base"
            | "@tz.heap.alloc.unlocked"
            | "@tz.heap.realloc.unlocked"
    ) {
        TrapKind::AllocationFailure
    } else if function.contains("concat") || function.ends_with(".append") {
        TrapKind::StringConcatOverflow
    } else if function.contains("debug.write") {
        TrapKind::DebugOutput
    } else if function.contains("decode")
        || function.contains("character")
        || function.contains("Utf8Char")
    {
        TrapKind::Encoding
    } else if function.contains(".allocate") || function.contains(".from_ascii") {
        TrapKind::AllocationSize
    } else if function.starts_with("@tz.unicode.") {
        TrapKind::BoundsCheck
    } else {
        TrapKind::NumericRuntime
    }
}

#[derive(Default)]
struct Function {
    calls: BTreeSet<String>,
    indirect: bool,
    kinds: BTreeSet<TrapKind>,
}

/// How the sites of a program are numbered. A site is `1 + n * kinds + kind` for the location `n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapNumbering {
    /// `n` counts the distinct locations in the order that they first appear in the whole program.
    Dense,
    /// `n` is a number that the path of the source file and the position inside it give (PB07 Phase 2), so that an
    /// edit in one file does not renumber the sites of another, and the report is one function for each module that
    /// has sites, with one that dispatches on the module.
    Stable,
}

/// The most files that a stable numbering gives sites to, and the most locations in each of them. A program with more
/// is numbered densely (and built from one unit). The files are as many as keep every number a positive `i32`: the
/// wasm32 export `tsuzuri_trap_site` returns one, which the host compares with the side table as JavaScript reads it.
const STABLE_LOCATIONS: u64 = 1 << 14;
const STABLE_SOURCES: u64 = i32::MAX as u64 / (TrapKind::ALL.len() as u64 * STABLE_LOCATIONS);
/// How many site numbers each file owns.
const STABLE_SPAN: u32 = (TrapKind::ALL.len() as u32) * (STABLE_LOCATIONS as u32);

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash ^ (hash >> 32)
}

/// The base of every location of a stable numbering, and the file that owns each block of numbers.
struct Assignment {
    bases: BTreeMap<(usize, usize, usize), u32>,
    owners: BTreeMap<u32, usize>,
}

/// Gives every location a base: its file takes the block of numbers that the hash of its path names (the next free one
/// when two paths name the same, in the order of the paths), and the location the place in the block that the hash of
/// its position names (the next free one, in the order of the positions). The numbers of a file then depend on
/// the paths of the other files and on its own positions, and on nothing else. The `fallback` location, which the
/// runtime's traps report, has the first place of its file whatever its position: an edit of the entry function
/// must not change the code of the runtime.
fn assign_stable(
    locations: &BTreeSet<(usize, usize, usize)>,
    sources: &[TrapSource<'_>],
    fallback: (usize, usize, usize),
) -> Option<Assignment> {
    let mut by_source: BTreeMap<usize, BTreeSet<(usize, usize)>> = BTreeMap::new();
    for &(source, start, end) in locations {
        by_source.entry(source).or_default().insert((start, end));
    }
    if by_source.len() as u64 > STABLE_SOURCES {
        return None;
    }
    let mut order: Vec<(&str, usize)> = by_source
        .keys()
        .map(|&index| (sources.get(index).map_or("", |source| source.path), index))
        .collect();
    order.sort_unstable();
    let mut taken = BTreeSet::new();
    let mut assignment = Assignment {
        bases: BTreeMap::new(),
        owners: BTreeMap::new(),
    };
    for (path, index) in order {
        let mut block = fnv(path.bytes()) % STABLE_SOURCES;
        while !taken.insert(block) {
            block = (block + 1) % STABLE_SOURCES;
        }
        assignment.owners.insert(block as u32, index);
        let spans = &by_source[&index];
        if spans.len() as u64 > STABLE_LOCATIONS {
            return None;
        }
        let mut used = BTreeSet::new();
        if fallback.0 == index && spans.contains(&(fallback.1, fallback.2)) {
            used.insert(0);
        }
        for &(start, end) in spans {
            let place = if (index, start, end) == fallback {
                0
            } else {
                let mut place = fnv((start as u64)
                    .to_le_bytes()
                    .into_iter()
                    .chain((end as u64).to_le_bytes()))
                    % STABLE_LOCATIONS;
                while !used.insert(place) {
                    place = (place + 1) % STABLE_LOCATIONS;
                }
                place
            };
            let base = 1 + TrapKind::ALL.len() as u64 * (block * STABLE_LOCATIONS + place);
            assignment
                .bases
                .insert((index, start, end), u32::try_from(base).ok()?);
        }
    }
    Some(assignment)
}

enum SiteNumbers {
    Dense,
    /// The first pass of a stable numbering: it only notes the locations.
    Collecting(BTreeSet<(usize, usize, usize)>),
    Stable(Assignment),
}

struct Sites {
    numbers: SiteNumbers,
    bases: BTreeMap<(usize, usize, usize), u32>,
    sites: BTreeMap<u32, TrapSite>,
}

impl Sites {
    fn new(numbers: SiteNumbers) -> Self {
        Self {
            numbers,
            bases: BTreeMap::new(),
            sites: BTreeMap::new(),
        }
    }

    fn base(&mut self, span: Span, kinds: &BTreeSet<TrapKind>) -> Result<u32, Diagnostic> {
        let key = (span.source.unwrap_or(0), span.start, span.end);
        let base = match &mut self.numbers {
            SiteNumbers::Collecting(locations) => {
                locations.insert(key);
                return Ok(0);
            }
            SiteNumbers::Stable(assignment) => *assignment
                .bases
                .get(&key)
                .expect("the first pass of a stable numbering saw every location"),
            SiteNumbers::Dense => {
                let next = u32::try_from(self.bases.len())
                    .ok()
                    .and_then(|count| count.checked_mul(TrapKind::ALL.len() as u32))
                    .and_then(|base| base.checked_add(1))
                    .ok_or_else(|| {
                        Diagnostic::of(
                            DiagnosticKind::TooManyTrapLocations,
                            "too many trap locations",
                            span,
                        )
                    })?;
                *self.bases.entry(key).or_insert(next)
            }
        };
        for kind in kinds {
            let id = base.checked_add(*kind as u32).ok_or_else(|| {
                Diagnostic::of(
                    DiagnosticKind::TooManyTrapLocations,
                    "too many trap locations",
                    span,
                )
            })?;
            self.sites.entry(id).or_insert(TrapSite {
                id,
                kind: *kind,
                span,
            });
        }
        Ok(base)
    }
}

/// The lines of `ir`. The emitter appends a runtime part right behind the last line of the text before it, and a
/// part that follows a function without a line break of its own (`}define ...`) shares the line of that function's
/// closing brace. LLVM reads it, but the instrumentation works line by line: it would take the function for the part
/// of the one before it. The brace and the rest are the two lines that they are.
fn ir_lines(ir: &str) -> impl Iterator<Item = &str> {
    ir.lines()
        .flat_map(|line| match line.strip_prefix('}') {
            Some(rest) if !rest.is_empty() => [Some("}"), Some(rest)],
            _ => [Some(line), None],
        })
        .flatten()
}

pub(super) fn instrument(
    ir: String,
    module: &CheckedModule,
    marks: Marks,
    sources: &[TrapSource<'_>],
    wasm: bool,
    raise: bool,
) -> Result<EmitOutput, Diagnostic> {
    instrument_numbered(
        ir,
        module,
        marks,
        sources,
        wasm,
        raise,
        TrapNumbering::Dense,
    )
}

pub(super) fn instrument_numbered(
    ir: String,
    module: &CheckedModule,
    marks: Marks,
    sources: &[TrapSource<'_>],
    wasm: bool,
    raise: bool,
    numbering: TrapNumbering,
) -> Result<EmitOutput, Diagnostic> {
    if sources.is_empty() {
        return Err(Diagnostic::of(
            DiagnosticKind::TrapInfoWithoutSourceMap,
            "trap information requires a source map",
            Span::default(),
        ));
    }
    let mut functions: BTreeMap<String, Function> = BTreeMap::new();
    let mut current = None;
    for raw in ir_lines(&ir) {
        let (line, marker) = marked(raw, &marks);
        if line.starts_with("define ") {
            if let Some(call) = callable(line, true) {
                functions.entry(call.name.into()).or_default();
                current = Some(call.name.to_owned());
            }
        } else if line == "}" {
            current = None;
        } else if let (Some(function), Some(call)) = (&current, callable(line, false)) {
            let entry = functions.get_mut(function).unwrap();
            if call.name == "@llvm.trap" {
                entry.kinds.insert(
                    marker
                        .and_then(|(_, kind)| kind)
                        .unwrap_or_else(|| runtime_kind(function)),
                );
            } else if call.name.starts_with('%') {
                entry.indirect = true;
            } else {
                entry.calls.insert(call.name.into());
            }
        }
    }
    let kinds: BTreeSet<_> = functions
        .values()
        .flat_map(|function| function.kinds.iter().copied())
        .collect();
    for function in functions.values_mut().filter(|function| function.indirect) {
        function.kinds.extend(&kinds);
    }
    loop {
        let updates: Vec<_> = functions
            .iter()
            .map(|(name, function)| {
                let inherited: BTreeSet<_> = function
                    .calls
                    .iter()
                    .filter_map(|callee| functions.get(callee))
                    .flat_map(|callee| callee.kinds.iter().copied())
                    .collect();
                (name.clone(), inherited)
            })
            .collect();
        let mut changed = false;
        for (name, inherited) in updates {
            let function = functions.get_mut(&name).unwrap();
            let previous = function.kinds.len();
            function.kinds.extend(inherited);
            changed |= function.kinds.len() != previous;
        }
        if !changed {
            break;
        }
    }
    let mut external: BTreeSet<_> = module
        .functions
        .iter()
        .filter(|function| function.exported)
        .map(|function| format!("@tz_{}", function.name))
        .collect();
    external.extend(
        [
            "@main",
            "@tsuzuri_main",
            "@tsuzuri_arguments",
            "@tsuzuri_test_count",
            "@tsuzuri_test_run",
            "@tsuzuri_task_parallel",
            "@tsuzuri_task_parallel_results",
            "@tsuzuri_mutex_lock",
            "@tsuzuri_mutex_unlock",
            "@tsuzuri_mutex_parallel_ok",
            "@tsuzuri_alloc",
            "@tsuzuri_free",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let managed: BTreeSet<_> = functions
        .keys()
        .filter(|name| {
            !external.contains(*name)
                && !name.starts_with("@__")
                && !name.starts_with("@tz.task.item.")
                && !name.starts_with("@tz.parallel.chunk.")
        })
        .cloned()
        .collect();
    let fallback = module
        .entry
        .map(|id| module.functions[id].body.span)
        .or_else(|| module.functions.first().map(|function| function.span))
        .unwrap_or_default();
    let stable = numbering == TrapNumbering::Stable;
    // The text with the sites of `sites` in it. A stable numbering reads the program twice: the first pass notes the
    // locations, which decide the numbers, and the second writes them.
    let rewrite = |sites: &mut Sites| -> Result<String, Diagnostic> {
        let mut output = String::with_capacity(ir.len());
        let mut function = String::new();
        let mut sequence = 0usize;
        for raw in ir_lines(&ir) {
            let (line, marker) = marked(raw, &marks);
            if line.starts_with('!')
                && line
                    .split_once(" = ")
                    .and_then(|(id, _)| id.strip_prefix('!')?.parse::<usize>().ok())
                    .is_some_and(|id| marks.instructions.contains_key(&id))
            {
                continue;
            }
            if line.starts_with("define ") {
                // A local name only has to differ inside its function, and a count of the whole program would
                // change in every function after an edit.
                if stable {
                    sequence = 0;
                }
                if let Some(call) = callable(line, true) {
                    function = call.name.into();
                    if managed.contains(call.name) {
                        let _ = writeln!(output, "{}", append_context(line, &call, "%tz.context"));
                        continue;
                    }
                }
            } else if line == "}" {
                function.clear();
            }
            if let Some(call) = callable(line, false) {
                let source = marks.sources.get(&function);
                let inherited =
                    managed.contains(&function) && source.is_none_or(|(_, inherited)| *inherited);
                let span = marker
                    .map(|(span, _)| span)
                    .or_else(|| source.map(|(span, _)| *span))
                    .unwrap_or(fallback);
                if call.name == "@llvm.trap" {
                    let kind = marker
                        .and_then(|(_, kind)| kind)
                        .unwrap_or_else(|| runtime_kind(&function));
                    let site = if inherited {
                        let value = format!("%tz.trap.{sequence}");
                        sequence += 1;
                        let _ =
                            writeln!(output, "  {value} = add i32 %tz.context, {}", kind as u32);
                        value
                    } else {
                        (sites.base(span, &BTreeSet::from([kind]))? + kind as u32).to_string()
                    };
                    let _ = writeln!(output, "  call void @tz.trap.report(i32 {site})");
                } else if managed.contains(call.name)
                    || call.name.starts_with('%')
                        && !matches!(
                            function.as_str(),
                            "@tsuzuri_task_parallel" | "@tsuzuri_task_parallel_results"
                        )
                {
                    let context = if inherited {
                        "%tz.context".into()
                    } else {
                        sites
                            .base(
                                span,
                                functions
                                    .get(call.name)
                                    .map_or(&kinds, |function| &function.kinds),
                            )?
                            .to_string()
                    };
                    let _ = writeln!(output, "{}", append_context(line, &call, &context));
                    continue;
                }
            }
            output.push_str(line);
            output.push('\n');
        }
        Ok(output)
    };
    let assignment = if stable {
        let mut collecting = Sites::new(SiteNumbers::Collecting(BTreeSet::new()));
        rewrite(&mut collecting)?;
        let SiteNumbers::Collecting(locations) = collecting.numbers else {
            unreachable!("the first pass collects");
        };
        assign_stable(
            &locations,
            sources,
            (fallback.source.unwrap_or(0), fallback.start, fallback.end),
        )
    } else {
        None
    };
    // A program with more files or locations than a stable numbering holds is numbered densely.
    let stable = assignment.is_some();
    let owners = assignment
        .as_ref()
        .map(|assignment| assignment.owners.clone());
    let mut sites = Sites::new(match assignment {
        Some(assignment) => SiteNumbers::Stable(assignment),
        None => SiteNumbers::Dense,
    });
    let mut output = rewrite(&mut sites)?;
    let trap_sites: Vec<_> = sites.sites.into_values().collect();
    if wasm {
        output.push_str("\n@tz.trap.latest = internal global i32 0\ndefine internal void @tz.trap.report(i32 %site) cold nounwind {\nentry:\n  store volatile i32 %site, ptr @tz.trap.latest\n  ret void\n}\ndefine i32 @tsuzuri_trap_site() {\nentry:\n  %site = load volatile i32, ptr @tz.trap.latest\n  ret i32 %site\n}\n");
        crate::trap::side_table(&trap_sites, sources)?;
    } else if stable && !raise {
        if !output.contains("declare i64 @write(") {
            output.push_str("\ndeclare i64 @write(i32, ptr, i64)\n");
        }
        // The report of a file is a function of its own, which goes with the file's other functions, and the
        // report that the code calls picks the file's function by the block of numbers the site is in.
        let file_modules = source_modules(module);
        let owners = owners.expect("a stable numbering has its blocks");
        let mut blocks: BTreeMap<u32, Vec<&TrapSite>> = BTreeMap::new();
        for site in &trap_sites {
            blocks
                .entry((site.id - 1) / STABLE_SPAN)
                .or_default()
                .push(site);
        }
        let mut names = BTreeMap::new();
        for (block, group) in &blocks {
            let name = match file_modules.get(&owners[block]) {
                Some(module) => module.clone(),
                None => format!("s{block:x}"),
            };
            for site in group {
                let text = format!("{}\n", site.message(sources)?);
                let bytes = text
                    .bytes()
                    .map(|byte| format!("\\{byte:02X}"))
                    .collect::<String>();
                let _ = writeln!(
                    output,
                    "@tz.trap.message.{} = private unnamed_addr constant [{} x i8] c\"{bytes}\"",
                    site.id,
                    text.len()
                );
            }
            let _ = write!(
                output,
                "define internal void @tz.trap.report.{name}(i32 %site) cold nounwind {{\nentry:\n  switch i32 %site, label %done [\n"
            );
            for site in group {
                let _ = writeln!(output, "    i32 {}, label %site{}", site.id, site.id);
            }
            output.push_str("  ]\n");
            for site in group {
                let length = site.message(sources)?.len() + 1;
                let _ = writeln!(
                    output,
                    "site{}:\n  call void @tz.trap.write(ptr @tz.trap.message.{}, i64 {length})\n  br label %done",
                    site.id, site.id
                );
            }
            output.push_str("done:\n  ret void\n}\n");
            names.insert(*block, name);
        }
        let _ = write!(
            output,
            "define internal void @tz.trap.report(i32 %site) cold nounwind {{\nentry:\n  %offset = sub i32 %site, 1\n  %block = udiv i32 %offset, {STABLE_SPAN}\n  switch i32 %block, label %done [\n"
        );
        for block in names.keys() {
            let _ = writeln!(output, "    i32 {block}, label %block{block}");
        }
        output.push_str("  ]\n");
        for (block, name) in &names {
            let _ = writeln!(
                output,
                "block{block}:\n  call void @tz.trap.report.{name}(i32 %site)\n  br label %done"
            );
        }
        output.push_str("done:\n  ret void\n}\n");
        output.push_str(TRAP_WRITE);
    } else {
        if !output.contains("declare i64 @write(") {
            output.push_str("\ndeclare i64 @write(i32, ptr, i64)\n");
        }
        for site in &trap_sites {
            let text = format!("{}\n", site.message(sources)?);
            let bytes = text
                .bytes()
                .map(|byte| format!("\\{byte:02X}"))
                .collect::<String>();
            let _ = writeln!(
                output,
                "@tz.trap.message.{} = private unnamed_addr constant [{} x i8] c\"{bytes}\"",
                site.id,
                text.len()
            );
        }
        // Sites are `1 + n * kinds + kind`; inside a trap boundary the report returns to the host.
        let boundary = if raise {
            output.push_str("declare void @tsuzuri_trap_raise(i32, i32)\n");
            format!(
                "  %offset = sub i32 %site, 1\n  %kind = urem i32 %offset, {}\n  call void @tsuzuri_trap_raise(i32 %site, i32 %kind)\n",
                TrapKind::ALL.len()
            )
        } else {
            String::new()
        };
        let _ = write!(
            output,
            "define internal void @tz.trap.report(i32 %site) cold nounwind {{\nentry:\n{boundary}  switch i32 %site, label %done [\n"
        );
        for site in &trap_sites {
            let _ = writeln!(output, "    i32 {}, label %site{}", site.id, site.id);
        }
        output.push_str("  ]\n");
        for site in &trap_sites {
            let length = site.message(sources)?.len() + 1;
            let _ = writeln!(
                output,
                "site{}:\n  call void @tz.trap.write(ptr @tz.trap.message.{}, i64 {length})\n  br label %done",
                site.id, site.id
            );
        }
        output.push_str("done:\n  ret void\n}\n");
        output.push_str(TRAP_WRITE);
    }
    Ok(EmitOutput {
        ir: output,
        trap_sites,
        stable_sites: stable && !raise,
    })
}

/// The function that writes a site's message to the standard error, one `write` after another.
const TRAP_WRITE: &str = "define internal void @tz.trap.write(ptr %data, i64 %length) cold nounwind {\nentry:\n  br label %loop\nloop:\n  %offset = phi i64 [0, %entry], [%next, %advance]\n  %done = icmp eq i64 %offset, %length\n  br i1 %done, label %exit, label %write\nwrite:\n  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset\n  %remaining = sub i64 %length, %offset\n  %written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)\n  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %advance, label %exit\nadvance:\n  %next = add i64 %offset, %written\n  br label %loop\nexit:\n  ret void\n}\n";

/// The module of each source file that has functions, by the index of the file.
fn source_modules(module: &CheckedModule) -> BTreeMap<usize, String> {
    let mut modules = BTreeMap::new();
    for function in &module.functions {
        if let Some(source) = function.span.source
            && !function.module.starts_with('$')
        {
            modules
                .entry(source)
                .or_insert_with(|| function.module.clone());
        }
    }
    modules
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The last place of the last block gives the largest numbers of a stable numbering; with every kind they are
    /// still positive `i32` values, which the wasm32 export `tsuzuri_trap_site` returns.
    #[test]
    fn the_largest_stable_site_is_a_positive_i32() {
        let path = (0..)
            .map(|index| format!("dir{index}/Main.tz"))
            .find(|path| fnv(path.bytes()) % STABLE_SOURCES == STABLE_SOURCES - 1)
            .unwrap();
        let start = (0usize..)
            .find(|&start| {
                let end = start as u64 + 1;
                fnv((start as u64)
                    .to_le_bytes()
                    .into_iter()
                    .chain(end.to_le_bytes()))
                    % STABLE_LOCATIONS
                    == STABLE_LOCATIONS - 1
            })
            .unwrap();
        let sources = [TrapSource {
            path: &path,
            text: "",
        }];
        let location = (0, start, start + 1);
        let assignment = assign_stable(
            &BTreeSet::from([location]),
            &sources,
            (0, usize::MAX, usize::MAX),
        )
        .unwrap();
        let largest = assignment.bases[&location]
            + TrapKind::ALL.iter().map(|kind| *kind as u32).max().unwrap();
        assert_eq!(
            u64::from(largest),
            TrapKind::ALL.len() as u64 * STABLE_SOURCES * STABLE_LOCATIONS
        );
        assert!(i32::try_from(largest).is_ok(), "{largest}");
    }
}
