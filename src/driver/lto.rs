//! `--emit bitcode` (PR08): the parts of the build that give a host's LTO link bitcode it can inline.
//!
//! A host that links the bitcode with `-flto=thin` can inline the program's functions into its own, but LLVM
//! inlines a function only into a caller whose target attributes accept the callee's. Clang writes
//! `"target-cpu"` and `"target-features"` on every C function it compiles and none on the functions of the
//! LLVM IR that this compiler writes, so the IR gets the attributes that Clang gives a C function compiled
//! with the same flags ([`probe_target_attributes`], [`stamp_target_attributes`]).

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::diagnostic::Diagnostic;

use super::{ToolSource, driver_error, io_error, resolve_tool, run_tool};

/// What Clang writes on a C function compiled with the flags of a build: the CPU it compiles for, the CPU it
/// tunes for where it differs, and the features it enables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetAttributes {
    cpu: String,
    tune: Option<String>,
    features: Option<String>,
}

impl TargetAttributes {
    pub fn new(cpu: &str, tune: Option<&str>, features: Option<&str>) -> Self {
        Self {
            cpu: cpu.to_owned(),
            tune: tune.map(str::to_owned),
            features: features.map(str::to_owned),
        }
    }

    pub fn cpu(&self) -> &str {
        &self.cpu
    }

    pub fn tune(&self) -> Option<&str> {
        self.tune.as_deref()
    }

    pub fn features(&self) -> Option<&str> {
        self.features.as_deref()
    }

    /// The attributes without the features: the CPU and its tuning decide what the code is for, and the
    /// features that Clang lists for a CPU are that CPU's own.
    pub fn without_features(&self) -> Self {
        Self {
            features: None,
            ..self.clone()
        }
    }

    /// The attributes in the text of an attribute list, if it names a CPU.
    fn parse(list: &str) -> Option<Self> {
        Some(Self {
            cpu: attribute_value(list, "target-cpu")?.to_owned(),
            tune: attribute_value(list, "tune-cpu").map(str::to_owned),
            features: attribute_value(list, "target-features").map(str::to_owned),
        })
    }

    /// The attributes as an attribute group writes them, with `features` in place of this one's, in the order
    /// LLVM prints them.
    fn render(&self, features: Option<&str>) -> String {
        let mut text = format!("\"target-cpu\"=\"{}\"", self.cpu);
        if let Some(features) = features {
            text.push_str(&format!(" \"target-features\"=\"{features}\""));
        }
        if let Some(tune) = &self.tune {
            text.push_str(&format!(" \"tune-cpu\"=\"{tune}\""));
        }
        text
    }
}

/// The value of the string attribute `key` in the text of an attribute list.
fn attribute_value<'a>(list: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\"=\"");
    let start = list.find(&marker)? + marker.len();
    let length = list[start..].find('"')?;
    Some(&list[start..start + length])
}

/// `list` without the string attribute `key` and the space before it.
fn without_attribute(list: &str, key: &str) -> String {
    let marker = format!("\"{key}\"=\"");
    let Some(start) = list.find(&marker) else {
        return list.to_owned();
    };
    let value = start + marker.len();
    let end = list[value..]
        .find('"')
        .map_or(list.len(), |at| value + at + 1);
    format!("{}{}", list[..start].trim_end(), &list[end..])
}

/// The attributes that Clang gives a C function compiled with `flags`, read from the LLVM IR of an empty one
/// (`clang -S -emit-llvm` in `directory`).
pub fn probe_target_attributes(
    clang: &OsStr,
    flags: &[OsString],
    directory: &Path,
) -> Result<TargetAttributes, Diagnostic> {
    let source = directory.join("probe.c");
    let output = directory.join("probe.ll");
    fs::write(&source, "void tsuzuri_probe(void) {}\n")
        .map_err(|error| io_error("write probe source", &source, error))?;
    let mut command = Command::new(clang);
    command
        .args(["-std=c11", "-S", "-emit-llvm"])
        .args(flags)
        .arg(&source)
        .arg("-o")
        .arg(&output);
    run_tool(
        &mut command,
        "bitcode output requires Clang that emits LLVM IR; set TSUZURI_CLANG",
    )?;
    let ir =
        fs::read_to_string(&output).map_err(|error| io_error("read probe IR", &output, error))?;
    probed_attributes(&ir).ok_or_else(|| {
        driver_error(
            "E2002",
            "cannot read target attributes from Clang output; set TSUZURI_CLANG to a Clang that emits LLVM IR",
        )
    })
}

/// The attributes of `@tsuzuri_probe` in the IR of the probe.
fn probed_attributes(ir: &str) -> Option<TargetAttributes> {
    let definition = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains("@tsuzuri_probe("))?;
    let group = group_references(&definition[parameters_end(definition)?..])
        .into_iter()
        .next()?;
    let prefix = format!("attributes #{group} = {{");
    let line = ir.lines().find(|line| line.starts_with(&prefix))?;
    TargetAttributes::parse(&line[prefix.len()..])
}

/// The byte after the closing parenthesis of the parameter list of a `define` line.
fn parameters_end(line: &str) -> Option<usize> {
    let at = line.find('@')? + 1;
    let name = if line[at..].starts_with('"') {
        line[at + 1..].find('"')? + 2
    } else {
        line[at..].find('(')?
    };
    let open = at + name;
    if line.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let (mut depth, mut quoted) = (0usize, false);
    for (offset, byte) in line[open..].bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'(' if !quoted => depth += 1,
            b')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The attribute groups (`#<n>`) that the tail of a `define` line names, outside its strings.
fn group_references(tail: &str) -> Vec<u32> {
    let bytes = tail.as_bytes();
    let mut groups = Vec::new();
    let mut quoted = false;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'#' if !quoted && index > 0 && bytes[index - 1] == b' ' => {
                let digits = bytes[index + 1..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
                if let Ok(number) = tail[index + 1..index + 1 + digits].parse() {
                    groups.push(number);
                }
            }
            _ => {}
        }
    }
    groups
}

/// Where a new attribute group goes in the tail of a `define` line: before its first metadata attachment
/// (` !dbg !12`), else before the ` {` that ends it.
fn group_insertion(tail: &str) -> usize {
    let bytes = tail.as_bytes();
    let mut quoted = false;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b' ' if !quoted && bytes.get(index + 1) == Some(&b'!') => return index,
            _ => {}
        }
    }
    let end = tail.trim_end();
    end.len() - 1 - usize::from(end.ends_with(" {"))
}

/// The number of an `attributes #<n> = {` line.
fn group_number(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("attributes #")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[digits..]
        .starts_with(" = {")
        .then(|| rest[..digits].parse().ok())
        .flatten()
}

/// Where a definition starts on a line of the IR: at its beginning, or after the `}` of the function before it
/// (the runtime text that follows a function may end without a newline).
fn definition_start(line: &str) -> Option<usize> {
    if line.starts_with("define ") {
        Some(0)
    } else if line.starts_with("}define ") {
        Some(1)
    } else {
        None
    }
}

/// Whether the text of a `define` line or an attribute group names `"target-features"`.
fn has_features(text: &str) -> bool {
    text.contains("\"target-features\"=\"")
}

/// Gives every function that `ir` defines the target attributes of `attributes`, so that a host built with
/// the same Clang finds them compatible with its own and can inline them (D3):
///
/// - A function with no attribute group of its own gets a new group of the attributes. One that names groups
///   (`numeric.ll` and `math.ll` do) gets the attributes in those, in place of any they have.
/// - A function that names its own `"target-features"` (the versions of a `@cpu` function do, F08) keeps
///   them: LLVM lets the features of an attribute group win over the ones written on the definition, so such a
///   function gets the CPU and its tuning only.
///
/// Whether the features are among `attributes` is the caller's choice (`--cpu native` copies them, the
/// default build does not: they are what the CPU implies, and a host may spell them differently).
/// The result is the same for the same input, and stamping it again changes nothing.
pub fn stamp_target_attributes(
    ir: &str,
    attributes: &TargetAttributes,
) -> Result<String, Diagnostic> {
    let mut highest = None;
    // The groups that definitions name, and whether one of those definitions has features of its own.
    let mut used: BTreeMap<u32, bool> = BTreeMap::new();
    for line in ir.lines() {
        if let Some(number) = group_number(line) {
            highest = highest.max(Some(number));
        } else if let Some(start) = definition_start(line) {
            let tail = &line[definition_end(line, start)?..];
            let own = has_features(tail);
            for group in group_references(tail) {
                *used.entry(group).or_default() |= own;
            }
        }
    }
    let with_features = highest.map_or(0, |number| number + 1);
    let without_features = with_features + 1;
    let (mut needs_with, mut needs_without) = (false, false);
    let mut output = String::with_capacity(ir.len() + ir.len() / 64);
    for line in ir.lines() {
        if let Some(start) = definition_start(line) {
            let end = definition_end(line, start)?;
            let tail = &line[end..];
            if group_references(tail).is_empty() {
                let group = if has_features(tail) {
                    needs_without = true;
                    without_features
                } else {
                    needs_with = true;
                    with_features
                };
                let at = end + group_insertion(tail);
                output.push_str(&line[..at]);
                output.push_str(&format!(" #{group}"));
                output.push_str(&line[at..]);
            } else {
                output.push_str(line);
            }
        } else if let Some(number) = group_number(line)
            && let Some(&own_in_definition) = used.get(&number)
        {
            let list = &line[line.find('{').map_or(0, |at| at + 1)..];
            let list = list.trim_end();
            let list = list.strip_suffix('}').unwrap_or(list).trim();
            // A group that has features keeps them, and one that a definition with features uses has none to add.
            let own = attribute_value(list, "target-features");
            let features = own.or(attributes.features().filter(|_| !own_in_definition));
            let mut rest = without_attribute(&without_attribute(list, "target-cpu"), "tune-cpu");
            if own.is_some() {
                rest = without_attribute(&rest, "target-features");
            }
            let stamped = attributes.render(features);
            let rest = rest.trim();
            if rest.is_empty() {
                output.push_str(&format!("attributes #{number} = {{ {stamped} }}"));
            } else {
                output.push_str(&format!("attributes #{number} = {{ {rest} {stamped} }}"));
            }
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    if needs_with {
        let group = attributes.render(attributes.features());
        output.push_str(&format!("attributes #{with_features} = {{ {group} }}\n"));
    }
    if needs_without {
        let group = attributes.render(None);
        output.push_str(&format!("attributes #{without_features} = {{ {group} }}\n"));
    }
    Ok(output)
}

/// The byte after the parameter list of the definition that starts at `start` of `line`, or the error that
/// stops the stamp.
fn definition_end(line: &str, start: usize) -> Result<usize, Diagnostic> {
    let unexpected = |why: &str| {
        driver_error(
            "E2002",
            format!(
                "cannot add target attributes to the LLVM IR: a definition {why}: {}",
                line.chars().take(120).collect::<String>()
            ),
        )
    };
    let end = parameters_end(&line[start..]).ok_or_else(|| unexpected("has no parameter list"))?;
    if !line.trim_end().ends_with('{') {
        return Err(unexpected("does not end with '{'"));
    }
    Ok(start + end)
}

/// Whether `head`, the first bytes of a file, begin LLVM bitcode: the raw magic, or the wrapper that Clang
/// writes for a Darwin target.
pub fn is_bitcode(head: &[u8]) -> bool {
    head.starts_with(b"BC\xC0\xDE") || head.starts_with(&[0xDE, 0xC0, 0x17, 0x0B])
}

/// Fails unless `artifact` is bitcode. A Clang that does not take `-flto=thin` for LLVM IR (the launcher of an
/// older distribution drops it) writes an object, which a host's LTO link would not read as bitcode.
pub fn check_bitcode(artifact: &Path, clang: &OsStr) -> Result<(), Diagnostic> {
    let mut head = [0u8; 4];
    let read = fs::File::open(artifact)
        .and_then(|mut file| file.read(&mut head))
        .map_err(|error| io_error("read bitcode", artifact, error))?;
    if is_bitcode(&head[..read]) {
        return Ok(());
    }
    Err(driver_error(
        "E2002",
        format!(
            "'{}' did not write LLVM bitcode for --emit bitcode; compiling LLVM IR with -x ir -flto=thin -c must give bitcode, so set TSUZURI_CLANG to a Clang that does",
            Path::new(clang).display()
        ),
    ))
}

/// The name of the function that a `define` or `declare` line (from its keyword) names.
fn function_name(line: &str) -> Option<&str> {
    let rest = &line[line.find('@')? + 1..];
    Some(rest[..rest.find('(')?].trim_matches('"'))
}

/// Which of `names` the functions of `ir` define and which they declare.
fn functions_among<'a>(
    ir: &str,
    names: &BTreeSet<&'a str>,
) -> (BTreeSet<&'a str>, BTreeSet<&'a str>) {
    let (mut defined, mut declared) = (BTreeSet::new(), BTreeSet::new());
    for line in ir.lines() {
        let (set, start) = if let Some(start) = definition_start(line) {
            (&mut defined, start)
        } else if line.starts_with("declare ") {
            (&mut declared, 0)
        } else {
            continue;
        };
        if let Some(name) = function_name(&line[start..]).and_then(|name| names.get(name)) {
            set.insert(*name);
        }
    }
    (defined, declared)
}

/// Keeps the public entry points that only the C runtime defines external through the merge of the runtime into
/// the bitcode (D5). `llvm-link --internalize` makes every symbol that the second file alone defines internal,
/// except those on `@llvm.used`. `exports` are the names a host may call (the ones of a shared library), and those
/// that `ir` does not define come from the runtime (`tsuzuri_async_post` does): `ir` gets a declaration of each
/// and an `@llvm.used` list of them, so the runtime's definition stays external, as it is in an object.
///
/// The result is the names it kept, for [`check_kept_exports`].
pub fn keep_runtime_exports(
    ir: &mut String,
    exports: &[String],
) -> Result<Vec<String>, Diagnostic> {
    let names: BTreeSet<&str> = exports.iter().map(String::as_str).collect();
    let (defined, declared) = functions_among(ir, &names);
    let kept: Vec<String> = names
        .difference(&defined)
        .map(|name| (*name).to_owned())
        .collect();
    if kept.is_empty() {
        return Ok(kept);
    }
    if ir.contains("@llvm.used") {
        return Err(driver_error(
            "E2002",
            "cannot keep the C runtime's entry points in the bitcode: the LLVM IR already has an @llvm.used list",
        ));
    }
    if !ir.ends_with('\n') {
        ir.push('\n');
    }
    for name in kept.iter().filter(|name| !declared.contains(name.as_str())) {
        ir.push_str(&format!("declare void @{name}()\n"));
    }
    let entries: Vec<String> = kept.iter().map(|name| format!("ptr @{name}")).collect();
    ir.push_str(&format!(
        "@llvm.used = appending global [{} x ptr] [{}], section \"llvm.metadata\"\n",
        kept.len(),
        entries.join(", ")
    ));
    Ok(kept)
}

/// Fails unless the IR in `merged`, which `llvm-link --internalize` wrote, still defines each of `kept` with
/// external linkage and default visibility. An `llvm-link` that does not honour `@llvm.used` would hide the entry
/// points that [`keep_runtime_exports`] named, and a host's link would not find them.
pub fn check_kept_exports(merged: &Path, kept: &[String]) -> Result<(), Diagnostic> {
    if kept.is_empty() {
        return Ok(());
    }
    let ir =
        fs::read_to_string(merged).map_err(|error| io_error("read merged IR", merged, error))?;
    let names: BTreeSet<&str> = kept.iter().map(String::as_str).collect();
    let mut external = BTreeSet::new();
    for line in ir.lines() {
        let Some(start) = definition_start(line) else {
            continue;
        };
        let line = &line[start..];
        let Some(name) = function_name(line).and_then(|name| names.get(name)) else {
            continue;
        };
        let head = &line[..line.find('@').unwrap_or_default()];
        if !head
            .split_whitespace()
            .any(|word| matches!(word, "internal" | "private" | "hidden"))
        {
            external.insert(*name);
        }
    }
    let lost: Vec<&str> = names.difference(&external).copied().collect();
    if lost.is_empty() {
        return Ok(());
    }
    Err(driver_error(
        "E2002",
        format!(
            "llvm-link did not keep {} as a public symbol of the bitcode, so a host could not call it; llvm-link must come from the same LLVM as Clang: set TSUZURI_LLVM_LINK",
            lost.join(", ")
        ),
    ))
}

/// The `llvm-link` that merges the C runtime into the bitcode: `TSUZURI_LLVM_LINK`, a distribution's `bin/`,
/// the directory of the Clang (the LLVM it belongs to, so the IR that llvm-link writes is one Clang reads),
/// then `PATH`.
pub fn llvm_link_tool() -> OsString {
    let (source, tool) = resolve_tool("TSUZURI_LLVM_LINK", "llvm-link");
    if source != ToolSource::Path {
        return tool;
    }
    let clang = resolve_tool("TSUZURI_CLANG", "clang").1;
    beside(&clang, "llvm-link").map_or(tool, PathBuf::into_os_string)
}

/// The tool `name` in the directory of `clang`, or of the Clang that a link of it leads to.
fn beside(clang: &OsStr, name: &str) -> Option<PathBuf> {
    let invoked = crate::cache::invoked_path(clang).ok()?;
    let resolved = crate::cache::real_path(&invoked).ok();
    [resolved.as_deref(), Some(invoked.as_path())]
        .into_iter()
        .flatten()
        .filter_map(Path::parent)
        .map(|directory| directory.join(format!("{name}{}", env::consts::EXE_SUFFIX)))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_attributes_of_a_list_and_removes_a_key() {
        let list = r#" noinline nounwind "frame-pointer"="non-leaf" "target-cpu"="apple-m1" "target-features"="+a,+b" "tune-cpu"="apple-m5" "#;
        let attributes = TargetAttributes::parse(list).unwrap();
        assert_eq!(
            attributes,
            TargetAttributes::new("apple-m1", Some("apple-m5"), Some("+a,+b"))
        );
        assert_eq!(
            attributes.render(attributes.features()),
            r#""target-cpu"="apple-m1" "target-features"="+a,+b" "tune-cpu"="apple-m5""#
        );
        assert_eq!(
            attributes.without_features().render(None),
            r#""target-cpu"="apple-m1" "tune-cpu"="apple-m5""#
        );
        assert!(TargetAttributes::parse(" nounwind ").is_none());
        assert_eq!(
            without_attribute(r#"nounwind "a"="1" "b"="2,3" "c"="4""#, "b"),
            r#"nounwind "a"="1" "c"="4""#
        );
        assert_eq!(without_attribute("nounwind", "b"), "nounwind");
    }

    #[test]
    fn finds_the_parameter_list_the_groups_and_the_insertion_point() {
        let line = r#"define internal i32 @"tz.fn.f(x)"(ptr dereferenceable(8) %0, i32 %1) #3 nounwind "alloc-family"="a #9" !dbg !7 {"#;
        let end = parameters_end(line).unwrap();
        assert_eq!(
            &line[end..],
            r#" #3 nounwind "alloc-family"="a #9" !dbg !7 {"#
        );
        assert_eq!(group_references(&line[end..]), [3]);
        let tail = &line[end..];
        assert_eq!(&tail[group_insertion(tail)..], " !dbg !7 {");
        let plain = "define i64 @tz_add(i64 %arg0, i64 %arg1) nounwind {";
        let tail = &plain[parameters_end(plain).unwrap()..];
        assert_eq!(&tail[group_insertion(tail)..], " {");
        let bare = "define i32 @main() {";
        let tail = &bare[parameters_end(bare).unwrap()..];
        assert_eq!(&tail[group_insertion(tail)..], " {");
        assert_eq!(group_number("attributes #12 = { nounwind }"), Some(12));
        assert_eq!(group_number("attributes #x = {"), None);
        assert_eq!(group_number("declare void @f() #12"), None);
        assert!(parameters_end("define void @f").is_none());
    }

    #[test]
    fn recognizes_bitcode_files() {
        assert!(is_bitcode(b"BC\xC0\xDE\x35\x14"));
        assert!(is_bitcode(&[0xDE, 0xC0, 0x17, 0x0B, 0, 0]));
        assert!(!is_bitcode(b"\xCF\xFA\xED\xFE"));
        assert!(!is_bitcode(b"\x7fELF"));
        assert!(!is_bitcode(b"BC"));
        assert!(!is_bitcode(b""));
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = env::temp_dir().join(format!("tsuzuri-lto-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn finds_llvm_link_beside_clang() {
        let directory = scratch("beside");
        let other = scratch("elsewhere");
        let suffix = env::consts::EXE_SUFFIX;
        fs::write(directory.join(format!("clang{suffix}")), "").unwrap();
        fs::write(directory.join(format!("llvm-link{suffix}")), "").unwrap();
        fs::write(other.join(format!("clang{suffix}")), "").unwrap();
        let found = beside(
            directory.join(format!("clang{suffix}")).as_os_str(),
            "llvm-link",
        );
        assert_eq!(
            found.map(|path| path.file_name().unwrap().to_owned()),
            Some(OsString::from(format!("llvm-link{suffix}")))
        );
        for clang in ["clang", "missing"] {
            assert_eq!(
                beside(
                    other.join(format!("{clang}{suffix}")).as_os_str(),
                    "llvm-link"
                ),
                None
            );
        }
        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn checks_the_head_of_the_artifact() {
        let directory = scratch("magic");
        let artifact = directory.join("artifact");
        fs::write(&artifact, b"BC\xC0\xDE rest").unwrap();
        assert!(check_bitcode(&artifact, OsStr::new("clang")).is_ok());
        fs::write(&artifact, b"\xCF\xFA\xED\xFE object").unwrap();
        let error = check_bitcode(&artifact, OsStr::new("tsuzuri-clang")).unwrap_err();
        assert_eq!(error.code, "E2002");
        assert!(
            error
                .message
                .starts_with("'tsuzuri-clang' did not write LLVM bitcode"),
            "{}",
            error.message
        );
        fs::write(&artifact, b"").unwrap();
        assert!(check_bitcode(&artifact, OsStr::new("clang")).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn keeps_what_the_runtime_alone_defines() {
        let mut ir = "define i32 @tz_add(i32 %a, i32 %b) {\n  ret i32 0\n}define weak void @tsuzuri_free(ptr %p) {\n  ret void\n}\ndeclare void @tsuzuri_alloc_stats()\n".to_owned();
        let exports = names(&["tsuzuri_async_post", "tsuzuri_free", "tz_add"]);
        let kept = keep_runtime_exports(&mut ir, &exports).unwrap();
        assert_eq!(kept, ["tsuzuri_async_post"]);
        assert!(
            ir.ends_with(
                "declare void @tsuzuri_async_post()\n@llvm.used = appending global [1 x ptr] [ptr @tsuzuri_async_post], section \"llvm.metadata\"\n"
            ),
            "{ir}"
        );
        // A definition of the program, even one that follows a `}`, is not the runtime's to keep.
        assert_eq!(ir.matches("@tsuzuri_free").count(), 1);
    }

    #[test]
    fn keeps_a_declared_function_without_declaring_it_again() {
        let mut ir = "declare i32 @tsuzuri_async_post(i64, i64)\ndefine void @f() {\n  ret void\n}"
            .to_owned();
        let exports = names(&["tsuzuri_async_post", "tsuzuri_cpu_x", "tsuzuri_async_post"]);
        let kept = keep_runtime_exports(&mut ir, &exports).unwrap();
        assert_eq!(kept, ["tsuzuri_async_post", "tsuzuri_cpu_x"]);
        assert_eq!(ir.matches("declare i32 @tsuzuri_async_post").count(), 1);
        assert!(!ir.contains("declare void @tsuzuri_async_post"), "{ir}");
        assert!(ir.contains("declare void @tsuzuri_cpu_x()\n"), "{ir}");
        assert!(
            ir.ends_with("@llvm.used = appending global [2 x ptr] [ptr @tsuzuri_async_post, ptr @tsuzuri_cpu_x], section \"llvm.metadata\"\n"),
            "{ir}"
        );
        // Between the end of the definition and the new text there is a line break.
        assert!(ir.contains("}\ndeclare void @tsuzuri_cpu_x()"), "{ir}");
    }

    #[test]
    fn leaves_an_ir_alone_that_defines_every_export() {
        let original = "define i32 @tz_add(i32 %a, i32 %b) {\n  ret i32 0\n}\n";
        let mut ir = original.to_owned();
        assert!(
            keep_runtime_exports(&mut ir, &names(&["tz_add"]))
                .unwrap()
                .is_empty()
        );
        assert!(keep_runtime_exports(&mut ir, &[]).unwrap().is_empty());
        assert_eq!(ir, original);
    }

    #[test]
    fn refuses_an_ir_that_has_its_own_used_list() {
        let mut ir =
            "@llvm.used = appending global [0 x ptr] [], section \"llvm.metadata\"\n".to_owned();
        let error = keep_runtime_exports(&mut ir, &names(&["tsuzuri_async_post"])).unwrap_err();
        assert_eq!(error.code, "E2002");
        assert!(error.message.contains("@llvm.used"), "{}", error.message);
        // Nothing to keep needs no list, so the IR's own stays.
        assert!(keep_runtime_exports(&mut ir, &[]).unwrap().is_empty());
    }

    #[test]
    fn finds_a_kept_function_that_the_merge_hid() {
        let directory = scratch("merged");
        let merged = directory.join("combined.ll");
        let kept = names(&["tsuzuri_async_post"]);
        let check = |text: &str| {
            fs::write(&merged, text).unwrap();
            check_kept_exports(&merged, &kept)
        };
        assert!(check("define weak i32 @tsuzuri_async_post(i64 %0, i64 %1) local_unnamed_addr #3 {\n  ret i32 0\n}\n").is_ok());
        assert!(
            check(
                "define dso_local i32 @\"tsuzuri_async_post\"(i64 %0, i64 %1) {\n  ret i32 0\n}\n"
            )
            .is_ok()
        );
        for hidden in [
            "define internal i32 @tsuzuri_async_post(i64 %0, i64 %1) {\n  ret i32 0\n}\n",
            "define private i32 @tsuzuri_async_post(i64 %0, i64 %1) {\n  ret i32 0\n}\n",
            "define weak hidden i32 @tsuzuri_async_post(i64 %0, i64 %1) {\n  ret i32 0\n}\n",
            "declare i32 @tsuzuri_async_post(i64, i64)\n",
            "define i32 @tsuzuri_async_posted() {\n  ret i32 0\n}\n",
            "",
        ] {
            let error = check(hidden).unwrap_err();
            assert_eq!(error.code, "E2002");
            assert!(
                error.message.contains("tsuzuri_async_post")
                    && error.message.contains("TSUZURI_LLVM_LINK"),
                "{}",
                error.message
            );
        }
        // Nothing kept, nothing to read.
        assert!(check_kept_exports(&directory.join("missing.ll"), &[]).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }
}
