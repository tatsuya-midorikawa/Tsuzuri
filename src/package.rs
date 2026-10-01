use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

use crate::{
    diagnostic::{Diagnostic, Span},
    syntax::MAX_SOURCE_BYTES,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageId {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub namespace: String,
    pub dependencies: BTreeMap<String, Dependency>,
    pub wasm: WasmSettings,
}

/// `[wasm]` sizes in bytes. Builds read only the root package's section, and
/// command-line options take precedence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WasmSettings {
    pub max_memory: Option<u64>,
    pub stack_size: Option<u64>,
}

/// Parses a byte count with an optional binary `KiB`, `MiB`, or `GiB` suffix.
pub fn parse_size(text: &str) -> Option<u64> {
    let (digits, suffix) = text.split_at(
        text.find(|character: char| !character.is_ascii_digit())
            .unwrap_or(text.len()),
    );
    let scale: u64 = match suffix {
        "" => 1,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        _ => return None,
    };
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u64>().ok()?.checked_mul(scale)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub path: PathBuf,
    pub span: Span,
}

pub fn namespace(name: &str, span: Span) -> Result<String, Diagnostic> {
    if name.len() > 255
        || !name.split('-').all(|part| {
            part.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
    {
        return Err(Diagnostic::new(
            "E1011",
            "package names must be lowercase ASCII kebab-case (at most 255 bytes)",
            span,
        ));
    }
    Ok(name
        .split('-')
        .map(|part| {
            let mut segment = part.to_owned();
            segment[..1].make_ascii_uppercase();
            segment
        })
        .collect())
}

pub fn parse_manifest(source: &str, source_id: usize) -> Result<Manifest, Diagnostic> {
    let whole = Span::new(0, source.len()).in_source(source_id);
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic::new(
            "E1017",
            "package manifest exceeds 1 MiB",
            whole,
        ));
    }
    let mut section = "";
    let mut sections = BTreeSet::new();
    let mut fields = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut wasm = WasmSettings::default();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let span = Span::new(offset, offset + line.trim_end().len()).in_source(source_id);
        offset += line.len();
        let mut cursor = Line {
            rest: line.trim_end_matches(['\r', '\n']),
            span,
        };
        cursor.space();
        if cursor.rest.is_empty() || cursor.rest.starts_with('#') {
            continue;
        }
        if cursor.rest.starts_with('[') {
            let next = if section.is_empty() {
                cursor.take("[package]").then_some("package")
            } else if cursor.take("[dependencies]") {
                Some("dependencies")
            } else {
                cursor.take("[wasm]").then_some("wasm")
            };
            match next {
                Some(next) if sections.insert(next) => section = next,
                _ => {
                    return Err(cursor.error(
                        "expected [package] followed by optional [dependencies] and [wasm], each once",
                    ));
                }
            }
            cursor.finish()?;
            continue;
        }
        let key = cursor.key()?;
        cursor.expect("=")?;
        match section {
            "package" => {
                if !matches!(key.as_str(), "name" | "version") {
                    return Err(cursor.error("unknown package key; expected name or version"));
                }
                let value = cursor.string()?;
                if value.is_empty() || fields.insert(key, (value, span)).is_some() {
                    return Err(
                        cursor.error("package fields must be nonempty and occur exactly once")
                    );
                }
            }
            "dependencies" => {
                namespace(&key, span)?;
                cursor.expect("{")?;
                if cursor.key()? != "path" {
                    return Err(cursor.error("only local path dependencies are supported"));
                }
                cursor.expect("=")?;
                let path = cursor.string()?;
                // Windows treats `/dir` and `C:dir` as non-absolute, but both escape the package root.
                let rooted = Path::new(&path).has_root()
                    || matches!(
                        Path::new(&path).components().next(),
                        Some(Component::Prefix(_))
                    );
                if path.is_empty() || path.contains('\0') || rooted {
                    return Err(cursor.error("dependency path must be a nonempty relative path"));
                }
                cursor.expect("}")?;
                if dependencies
                    .insert(
                        key,
                        Dependency {
                            path: path.into(),
                            span,
                        },
                    )
                    .is_some()
                {
                    return Err(Diagnostic::new(
                        "E1011",
                        "duplicate package dependency",
                        span,
                    ));
                }
                if dependencies.len() > 1024 {
                    return Err(Diagnostic::new(
                        "E1017",
                        "a package may have at most 1024 dependencies",
                        span,
                    ));
                }
            }
            "wasm" => {
                let slot = match key.as_str() {
                    "max-memory" => &mut wasm.max_memory,
                    "stack-size" => &mut wasm.stack_size,
                    _ => {
                        return Err(
                            cursor.error("unknown wasm key; expected max-memory or stack-size")
                        );
                    }
                };
                let text = cursor.string()?;
                let size = parse_size(&text).ok_or_else(|| {
                    cursor.error("WASM sizes must be a quoted byte count or a number followed by KiB, MiB, or GiB, such as \"64MiB\"")
                })?;
                if slot.replace(size).is_some() {
                    return Err(cursor.error("wasm keys must occur at most once"));
                }
            }
            _ => return Err(cursor.error("expected [package] before fields")),
        }
        cursor.finish()?;
    }
    let missing = || Diagnostic::new("E0002", "[package] requires name and version", whole);
    let (name, span) = fields.remove("name").ok_or_else(missing)?;
    let (version, _) = fields.remove("version").ok_or_else(missing)?;
    Ok(Manifest {
        namespace: namespace(&name, span)?,
        name,
        version,
        dependencies,
        wasm,
    })
}

struct Line<'a> {
    rest: &'a str,
    span: Span,
}

impl Line<'_> {
    fn error(&self, message: &str) -> Diagnostic {
        Diagnostic::new("E0002", message, self.span)
    }

    fn space(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', '\t']);
    }

    fn take(&mut self, token: &str) -> bool {
        self.space();
        if let Some(rest) = self.rest.strip_prefix(token) {
            self.rest = rest;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), Diagnostic> {
        if self.take(token) {
            Ok(())
        } else {
            Err(self.error(&format!("expected '{token}'")))
        }
    }

    fn key(&mut self) -> Result<String, Diagnostic> {
        self.space();
        let end = self
            .rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            .count();
        if end == 0 {
            return Err(self.error("expected a bare key"));
        }
        let key = self.rest[..end].to_owned();
        self.rest = &self.rest[end..];
        Ok(key)
    }

    fn string(&mut self) -> Result<String, Diagnostic> {
        self.expect("\"")?;
        let mut value = String::new();
        while let Some(character) = self.rest.chars().next() {
            self.rest = &self.rest[character.len_utf8()..];
            match character {
                '"' => return Ok(value),
                '\\' => {
                    let Some(escaped) = self.rest.chars().next() else {
                        return Err(self.error("unterminated string escape"));
                    };
                    self.rest = &self.rest[escaped.len_utf8()..];
                    value.push(match escaped {
                        '"' | '\\' => escaped,
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            return Err(self
                                .error("unsupported string escape; use \\\" \\\\ \\n \\r or \\t"));
                        }
                    });
                }
                character if character.is_control() => {
                    return Err(self.error("unescaped control character in string"));
                }
                character => value.push(character),
            }
        }
        Err(self.error("unterminated quoted string"))
    }

    fn finish(&mut self) -> Result<(), Diagnostic> {
        self.space();
        if self.rest.is_empty() || self.rest.starts_with('#') {
            Ok(())
        } else {
            Err(self.error("unexpected content after manifest field"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: &str = "[package]\nname = \"sample-app\"\nversion = \"0.1.0\"\n";

    #[test]
    fn parses_manifest_comments_escapes_and_crlf() {
        let manifest = parse_manifest(&format!("# heading\r\n{PACKAGE}[dependencies]\r\ngeometry-core = {{ path = \"../geo#\\\"\\\\\\t\\n\\r\" }} # end\r\n"), 3).unwrap();
        assert_eq!(manifest.namespace, "SampleApp");
        assert_eq!(
            manifest.dependencies["geometry-core"].path,
            PathBuf::from("../geo#\"\\\t\n\r")
        );
        assert_eq!(manifest.dependencies["geometry-core"].span.source, Some(3));
    }

    #[test]
    fn parses_wasm_sizes_in_any_section_order() {
        for source in [
            format!(
                "{PACKAGE}[wasm]\nmax-memory = \"256MiB\"\nstack-size = \"65536\" # bytes\n[dependencies]\n"
            ),
            format!(
                "{PACKAGE}[dependencies]\n[wasm]\nstack-size = \"64KiB\"\nmax-memory = \"1GiB\"\n"
            ),
        ] {
            let wasm = parse_manifest(&source, 0).unwrap().wasm;
            assert_eq!(wasm.stack_size, Some(65536));
            assert!(matches!(wasm.max_memory, Some(268435456 | 1073741824)));
        }
        assert_eq!(
            parse_manifest(PACKAGE, 0).unwrap().wasm,
            WasmSettings::default()
        );
        for (text, size) in [
            ("0", Some(0)),
            ("16GiB", Some(1 << 34)),
            ("64MB", None),
            ("MiB", None),
            ("1.5GiB", None),
            (" 64MiB", None),
            ("18446744073709551615", Some(u64::MAX)),
            ("18446744073709551616", None),
            ("17179869184GiB", None),
        ] {
            assert_eq!(parse_size(text), size, "{text}");
        }
    }

    #[test]
    fn rejects_unknown_duplicate_and_malformed_fields() {
        for (suffix, code) in [
            ("name = \"other\"", "E0002"),
            ("command = \"anything\"", "E0002"),
            ("[other]", "E0002"),
            ("[package]", "E0002"),
            (
                "[dependencies]\nfoo = { git = \"https://example.org\" }",
                "E0002",
            ),
            (
                "[dependencies]\nfoo = { path = \"../foo\", version = \"1\" }",
                "E0002",
            ),
            (
                "[dependencies]\nfoo = { path = \"../foo\" }\nfoo = { path = \"../bar\" }",
                "E1011",
            ),
            ("[dependencies]\nBad_Name = { path = \"../foo\" }", "E1011"),
            ("[dependencies]\nfoo = { path = \"/absolute\" }", "E0002"),
            ("[dependencies]\nfoo = { path = \"\\u0041\" }", "E0002"),
            ("[wasm]\n[wasm]", "E0002"),
            ("[wasm]\n[package]", "E0002"),
            ("[wasm]\nmemory = \"64MiB\"", "E0002"),
            ("[wasm]\nmax-memory = 67108864", "E0002"),
            ("[wasm]\nmax-memory = \"64MB\"", "E0002"),
            (
                "[wasm]\nstack-size = \"1MiB\"\nstack-size = \"2MiB\"",
                "E0002",
            ),
        ] {
            let error = parse_manifest(&format!("{PACKAGE}{suffix}"), 5).unwrap_err();
            assert_eq!(error.code, code, "{suffix}: {error:?}");
            assert_eq!(error.span.source, Some(5));
        }
        for source in [
            "",
            "[package]\nname = \"app\"",
            "[package]\nversion = \"1\"",
            "[wasm]\n[package]\nname = \"app\"\nversion = \"1\"",
        ] {
            assert_eq!(parse_manifest(source, 0).unwrap_err().code, "E0002");
        }
        for name in ["", "A", "a_", "-a", "a-", "a--b", "a-1"] {
            assert_eq!(namespace(name, Span::default()).unwrap_err().code, "E1011");
        }
    }
}
