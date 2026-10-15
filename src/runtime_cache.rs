use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::cache::{Sha256, read_regular};

const PREBUILT_ENVIRONMENT: &[&str] = &[
    "SDKROOT",
    "DEVELOPER_DIR",
    "MACOSX_DEPLOYMENT_TARGET",
    "INCLUDE",
    "LIB",
    "LIBPATH",
    "CPATH",
    "C_INCLUDE_PATH",
    "CPLUS_INCLUDE_PATH",
    "LIBRARY_PATH",
    "SOURCE_DATE_EPOCH",
    "TSUZURI_SYSROOT",
    "TSUZURI_ZIG",
    "TSUZURI_LLVM_CLANG",
];

#[derive(Default)]
pub(crate) struct RuntimeSymbols(BTreeSet<String>);

impl RuntimeSymbols {
    pub(crate) fn collect(source: &str) -> Self {
        let mut symbols = Self::default();
        symbols.extend(source);
        symbols
    }

    fn extend(&mut self, source: &str) {
        let bytes = source.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b';' => {
                    while index < bytes.len() && bytes[index] != b'\n' {
                        index += 1;
                    }
                }
                b'"' => {
                    index += 1;
                    while index < bytes.len() {
                        if bytes[index] == b'\\' {
                            index += 1;
                        } else if bytes[index] == b'"' {
                            index += 1;
                            break;
                        }
                        index += 1;
                    }
                }
                b'@' => {
                    index += 1;
                    let start = index;
                    while index < bytes.len()
                        && (bytes[index].is_ascii_alphanumeric()
                            || matches!(bytes[index], b'_' | b'.' | b'$' | b'-'))
                    {
                        index += 1;
                    }
                    if index > start {
                        self.0.insert(source[start..index].to_owned());
                    }
                }
                _ => index += 1,
            }
        }
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    pub(crate) fn prefix(&self, prefix: &str) -> bool {
        self.0
            .range(prefix.to_owned()..)
            .next()
            .is_some_and(|name| name.starts_with(prefix))
    }

    pub(crate) fn append(&mut self, output: &mut String, source: &str) {
        self.extend(source);
        output.push_str(source);
    }
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.hex()
}

fn symbol(header: &str) -> Option<&str> {
    header
        .split_once('@')?
        .1
        .split_once('(')
        .map(|(name, _)| name)
}

fn inline_math(name: &str) -> bool {
    matches!(
        name,
        "tz_math_sqrt_f32" | "tz_math_sqrt_f64" | "tz_math_fabs_f32" | "tz_math_fabs_f64"
    )
}

fn external_math(name: &str) -> bool {
    name.starts_with("tz_math_")
        && (name.ends_with("_f32") || name.ends_with("_f64"))
        && !inline_math(name)
}

fn declaration(header: &str, prefix: &str) -> String {
    let signature = header
        .strip_prefix(prefix)
        .expect("runtime definition prefix");
    let end = signature.rfind(')').expect("runtime function parameters");
    let (result, arguments) = signature[..end].split_once('(').expect("runtime signature");
    let arguments = arguments
        .split(", ")
        .map(|argument| argument.rsplit_once(" %").map_or(argument, |(ty, _)| ty))
        .collect::<Vec<_>>()
        .join(", ");
    format!("declare hidden {result}({arguments})\n")
}

pub(crate) fn numeric_declarations() -> &'static str {
    static DECLARATIONS: OnceLock<String> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        include_str!("runtime/numeric.ll")
            .lines()
            .filter(|line| line.starts_with("define weak hidden "))
            .map(|line| declaration(line, "define weak hidden "))
            .collect()
    })
}

pub(crate) fn math_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE.get_or_init(|| {
        include_str!("runtime/math.ll")
            .split_inclusive('\n')
            .map(|line| {
                if line.starts_with("define internal ") && symbol(line).is_some_and(external_math) {
                    // Hidden, non-interposable definitions still inline within this module.
                    line.replacen("define internal ", "define hidden ", 1)
                } else {
                    line.to_owned()
                }
            })
            .collect()
    })
}

pub(crate) fn math_declarations() -> &'static str {
    static DECLARATIONS: OnceLock<String> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        let mut output = String::new();
        let mut lines = include_str!("runtime/math.ll").lines();
        while let Some(line) = lines.next() {
            if !line.starts_with("define internal ") {
                continue;
            }
            let Some(name) = symbol(line) else { continue };
            if external_math(name) {
                output.push_str(&declaration(line, "define internal "));
            } else if inline_math(name) {
                // Keep these intrinsic wrappers in the caller, without generated attribute IDs.
                let end = line.rfind(')').expect("math parameters");
                output.push_str(&line[..=end]);
                output.push_str(" {\n");
                for body in lines.by_ref() {
                    output.push_str(body);
                    output.push('\n');
                    if body == "}" {
                        break;
                    }
                }
            }
        }
        for (name, ty) in [
            ("sqrt.f32", "float"),
            ("sqrt.f64", "double"),
            ("fabs.f32", "float"),
            ("fabs.f64", "double"),
        ] {
            output.push_str(&format!("declare {ty} @llvm.{name}({ty})\n"));
        }
        output
    })
}

/// Distribution objects are relocatable but only valid with the exact bundled toolchain.
pub(crate) fn prebuilt(
    unit: &str,
    source: &str,
    args: &[OsString],
    identity: Option<&crate::cache::ToolIdentity>,
) -> io::Result<Option<Vec<u8>>> {
    if ["TSUZURI_ZIG", "TSUZURI_LLVM_CLANG"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return Ok(None);
    }
    if crate::driver::resolve_tool("TSUZURI_CLANG", "clang").0 != crate::driver::ToolSource::Bundled
    {
        return Ok(None);
    }
    let Some(root) = crate::driver::distribution_root() else {
        return Ok(None);
    };
    let Some(identity) = identity else {
        return Ok(None);
    };
    load_prebuilt(&root, unit, source, args, identity)
}

fn load_prebuilt(
    root: &Path,
    unit: &str,
    source: &str,
    args: &[OsString],
    identity: &crate::cache::ToolIdentity,
) -> io::Result<Option<Vec<u8>>> {
    let directory = root.join("share/runtime");
    let manifest = match read_regular(&directory.join("manifest.json"), 4 * 1024 * 1024) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let value: serde_json::Value = serde_json::from_slice(&manifest)?;
    if value["format"] != 1
        || value["os"] != std::env::consts::OS
        || value["arch"] != std::env::consts::ARCH
    {
        return Ok(None);
    }
    let Some(environment) = value["environment"].as_object() else {
        return Ok(None);
    };
    if environment.len() != PREBUILT_ENVIRONMENT.len()
        || PREBUILT_ENVIRONMENT
            .iter()
            .any(|name| !environment.contains_key(*name))
    {
        return Ok(None);
    }
    for (name, expected) in environment {
        let current = std::env::var_os(name);
        let actual = match current.as_ref().map(|value| value.to_str()) {
            Some(None) => return Ok(None),
            Some(Some(value)) => Some(value),
            None => None,
        };
        if !(expected.is_null() || expected.is_string()) || actual != expected.as_str() {
            return Ok(None);
        }
    }
    let Some(tools) = value["tools"].as_object() else {
        return Ok(None);
    };
    let suffix = std::env::consts::EXE_SUFFIX;
    if !tools.contains_key(&format!("bin/tsuzuri-clang{suffix}"))
        || !tools.contains_key(&format!("bin/clang{suffix}"))
        || (unit == "native-c"
            && !(cfg!(windows) && std::env::consts::ARCH == "aarch64")
            && !tools.contains_key(&format!("zig/zig{suffix}")))
    {
        return Ok(None);
    }
    for (name, expected) in tools {
        if !safe_relative(name) {
            return Ok(None);
        }
        if !expected
            .as_str()
            .is_some_and(|expected| identity.matches_bundled_tool(name, expected))
        {
            return Ok(None);
        }
    }
    let args: Option<Vec<_>> = args.iter().map(|arg| arg.to_str()).collect();
    let Some(args) = args else { return Ok(None) };
    let Some(entries) = value["objects"].as_array() else {
        return Ok(None);
    };
    let hash = digest(source.as_bytes());
    for entry in entries {
        if entry["unit"] != unit
            || entry["source"] != hash
            || entry["args"] != serde_json::json!(args)
        {
            continue;
        }
        let Some(name) = entry["file"].as_str() else {
            return Ok(None);
        };
        if !safe_relative(name) || Path::new(name).components().count() != 1 {
            return Ok(None);
        }
        let bytes = read_regular(&directory.join(name), 256 * 1024 * 1024)?;
        if entry["size"].as_u64() != Some(bytes.len() as u64) || entry["sha256"] != digest(&bytes) {
            return Ok(None);
        }
        return Ok(Some(bytes));
    }
    Ok(None)
}

fn safe_relative(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && PathBuf::from(name)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_symbols_ignore_literals_comments_and_prefix_collisions() {
        let mut symbols = RuntimeSymbols::collect(
            "@data = constant [40 x i8] c\"@tz_soft_op(\\00@tz_math_sin_f64(\"\n\
             ; call void @tz.rec.fake()\n\
             call void @tz_soft_cast(ptr %a, ptr %b, i32 0, i32 0)\n\
             call void @tz.alloc.check(i64 1)\n",
        );
        assert!(symbols.contains("tz_soft_cast"));
        assert!(!symbols.contains("tz_soft_op"));
        assert!(!symbols.contains("tz.alloc"));
        assert!(symbols.prefix("tz.alloc"));
        assert!(!symbols.prefix("tz_math_"));
        assert!(!symbols.prefix("tz.rec."));
        let mut output = String::new();
        symbols.append(
            &mut output,
            "define void @tz.display.integer() { call void @tz_soft_format() }\n",
        );
        assert!(symbols.contains("tz_soft_format"));
    }

    #[test]
    fn runtime_declarations_follow_generated_sources() {
        let source = include_str!("runtime/numeric.ll");
        assert_eq!(
            numeric_declarations().lines().count(),
            source
                .lines()
                .filter(|line| line.starts_with("define weak hidden "))
                .count()
        );
        assert!(!numeric_declarations().contains('%'));
        assert!(!numeric_declarations().contains("@llvm.trap"));
        assert!(math_source().contains("define hidden double @tz_math_sin_f64"));
        assert!(math_declarations().contains("define internal noundef double @tz_math_sqrt_f64"));
        assert!(!math_declarations().contains("define internal double @tz_math_sin_f64"));
        assert!(math_declarations().contains("declare hidden double @tz_math_sin_f64"));
    }

    #[test]
    fn prebuilt_paths_cannot_escape_distribution() {
        for name in ["../object", "/object", "a/../object", r"a\object", ""] {
            assert!(!safe_relative(name), "{name}");
        }
        assert!(safe_relative("bin/clang"));
    }
}
