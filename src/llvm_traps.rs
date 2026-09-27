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
}

struct Callable<'a> {
    name: &'a str,
    open: usize,
    close: usize,
}

fn callable(line: &str, definition: bool) -> Option<Callable<'_>> {
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

fn quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
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
    if function.contains("builtin.assert") {
        TrapKind::Assert
    } else if function.contains("builtin.unreachable") {
        TrapKind::MatchFailure
    } else if function == "@tz.alloc" || function == "@tz.realloc" {
        TrapKind::AllocationFailure
    } else if function.contains("concat") {
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

#[derive(Default)]
struct Sites {
    bases: BTreeMap<(usize, usize, usize), u32>,
    sites: BTreeMap<u32, TrapSite>,
}

impl Sites {
    fn base(&mut self, span: Span, kinds: &BTreeSet<TrapKind>) -> Result<u32, Diagnostic> {
        let key = (span.source.unwrap_or(0), span.start, span.end);
        let next = u32::try_from(self.bases.len())
            .ok()
            .and_then(|count| count.checked_mul(TrapKind::ALL.len() as u32))
            .and_then(|base| base.checked_add(1))
            .ok_or_else(|| Diagnostic::new("E1017", "too many trap locations", span))?;
        let base = *self.bases.entry(key).or_insert(next);
        for kind in kinds {
            let id = base
                .checked_add(*kind as u32)
                .ok_or_else(|| Diagnostic::new("E1017", "too many trap locations", span))?;
            self.sites.entry(id).or_insert(TrapSite {
                id,
                kind: *kind,
                span,
            });
        }
        Ok(base)
    }
}

pub(super) fn instrument(
    ir: String,
    module: &CheckedModule,
    marks: Marks,
    sources: &[TrapSource<'_>],
    wasm: bool,
) -> Result<EmitOutput, Diagnostic> {
    if sources.is_empty() {
        return Err(Diagnostic::new(
            "E2000",
            "trap information requires a source map",
            Span::default(),
        ));
    }
    let mut functions: BTreeMap<String, Function> = BTreeMap::new();
    let mut current = None;
    for raw in ir.lines() {
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
            "@tsuzuri_test_count",
            "@tsuzuri_test_run",
            "@tsuzuri_task_parallel",
            "@tsuzuri_task_parallel_results",
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
    let mut sites = Sites::default();
    let mut output = String::with_capacity(ir.len());
    let mut function = String::new();
    let mut sequence = 0usize;
    for raw in ir.lines() {
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
                    let _ = writeln!(output, "  {value} = add i32 %tz.context, {}", kind as u32);
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
    let trap_sites: Vec<_> = sites.sites.into_values().collect();
    if wasm {
        output.push_str("\n@tz.trap.latest = internal global i32 0\ndefine internal void @tz.trap.report(i32 %site) cold nounwind {\nentry:\n  store volatile i32 %site, ptr @tz.trap.latest\n  ret void\n}\ndefine i32 @tsuzuri_trap_site() {\nentry:\n  %site = load volatile i32, ptr @tz.trap.latest\n  ret i32 %site\n}\n");
        crate::trap::side_table(&trap_sites, sources)?;
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
        output.push_str("define internal void @tz.trap.report(i32 %site) cold nounwind {\nentry:\n  switch i32 %site, label %done [\n");
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
        output.push_str("done:\n  ret void\n}\ndefine internal void @tz.trap.write(ptr %data, i64 %length) cold nounwind {\nentry:\n  br label %loop\nloop:\n  %offset = phi i64 [0, %entry], [%next, %advance]\n  %done = icmp eq i64 %offset, %length\n  br i1 %done, label %exit, label %write\nwrite:\n  %pointer = getelementptr inbounds i8, ptr %data, i64 %offset\n  %remaining = sub i64 %length, %offset\n  %written = call i64 @write(i32 2, ptr %pointer, i64 %remaining)\n  %progress = icmp sgt i64 %written, 0\n  br i1 %progress, label %advance, label %exit\nadvance:\n  %next = add i64 %offset, %written\n  br label %loop\nexit:\n  ret void\n}\n");
    }
    Ok(EmitOutput {
        ir: output,
        trap_sites,
    })
}
