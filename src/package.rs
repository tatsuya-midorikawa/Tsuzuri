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
    /// The `[native]` link inputs, with paths still relative to the package root, and the span of its header.
    pub native: Option<(crate::driver::LinkInputs, Span)>,
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
    /// `native = true`: the root package lets this dependency's `[native]` link inputs into the build.
    pub native: bool,
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

/// Whether `text` can be a package's default namespace: dotted identifiers,
/// at most 16 segments and 255 bytes, whose first segment is not a standard
/// library module name.
pub fn valid_namespace(text: &str) -> bool {
    use crate::syntax::{Token, TokenKind};
    text.len() <= 255
        && text.split('.').count() <= 16
        && text
            .split('.')
            .next()
            .is_some_and(|first| !crate::stdlib::is_reserved_module(first))
        && text.split('.').all(|segment| {
            segment != "_" && segment != "Task" && crate::lexer::lex(segment).is_ok_and(|tokens| {
                matches!(
                    tokens.as_slice(),
                    [Token { kind: TokenKind::Ident(name), .. }, Token { kind: TokenKind::End, .. }]
                        if name == segment
                )
            })
        })
}

/// Creates a package in `directory`, which must be missing or empty: a
/// `Tsuzuri.toml` that names its default namespace, a `Main.tz` that declares
/// it, and a `.gitignore`. Returns the created files.
pub fn create_project(
    directory: &Path,
    namespace: Option<&str>,
) -> Result<Vec<PathBuf>, Diagnostic> {
    let error = |message: String| Diagnostic::new("E2000", message, Span::default());
    let resolved = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_owned());
    let folder = resolved
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let name = package_name(folder);
    let namespace = match namespace {
        Some(namespace) if valid_namespace(namespace) => namespace.to_owned(),
        Some(namespace) => {
            return Err(error(format!(
                "invalid namespace '{namespace}'; use dotted identifiers such as Acme.Tools, at most 16 segments and 255 bytes, that do not start with a standard library module name"
            )));
        }
        None => self::namespace(&name, Span::default())
            .ok()
            .filter(|namespace| valid_namespace(namespace))
            .ok_or_else(|| {
                error(format!(
                    "cannot derive a namespace from the folder name '{folder}'; pass --namespace"
                ))
            })?,
    };
    let io = |action: &str, path: &Path, cause: std::io::Error| {
        error(format!("cannot {action} '{}': {cause}", path.display()))
    };
    match std::fs::read_dir(directory) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(error(format!(
                    "'{}' is not empty; choose a new or empty folder so that no file is overwritten",
                    directory.display()
                )));
            }
        }
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(directory).map_err(|cause| io("create", directory, cause))?;
        }
        Err(cause) => return Err(io("read", directory, cause)),
    }
    let files = [
        (
            "Tsuzuri.toml",
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nnamespace = \"{namespace}\"\n"
            ),
        ),
        (
            "Main.tz",
            format!(
                "namespace {namespace}\n\ndef main :: IO<unit> =\n    do! IO.writeln \"Hello, Tsuzuri!\"\n\ntest \"adds numbers\" = assert (1 + 2 == 3)\n"
            ),
        ),
        (".gitignore", ".tsuzuri/\n".to_owned()),
    ];
    let mut created = Vec::new();
    for (file, text) in files {
        let path = directory.join(file);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut handle| std::io::Write::write_all(&mut handle, text.as_bytes()))
            .map_err(|cause| io("create", &path, cause))?;
        created.push(path);
    }
    Ok(created)
}

/// A kebab-case package name for a folder name such as `MyApp` or
/// `hello_world`, or `app` when the folder name has no usable letters.
fn package_name(folder: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut previous = ' ';
    for character in folder.chars() {
        if !character.is_ascii_alphanumeric() {
            previous = ' ';
            continue;
        }
        if previous == ' ' || character.is_ascii_uppercase() && previous.is_ascii_lowercase() {
            words.push(String::new());
        }
        if let Some(word) = words.last_mut() {
            word.push(character.to_ascii_lowercase());
        }
        previous = character;
    }
    let name = words
        .into_iter()
        .filter(|word| word.starts_with(|first: char| first.is_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join("-");
    if namespace(&name, Span::default()).is_ok() {
        name
    } else {
        "app".to_owned()
    }
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
    let mut native: Option<(crate::driver::LinkInputs, Span)> = None;
    let mut native_keys = BTreeSet::new();
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
            } else if cursor.take("[wasm]") {
                Some("wasm")
            } else {
                cursor.take("[native]").then_some("native")
            };
            match next {
                Some(next) if sections.insert(next) => section = next,
                _ => {
                    return Err(cursor.error(
                        "expected [package] followed by optional [dependencies], [wasm] and [native], each once",
                    ));
                }
            }
            if section == "native" {
                native = Some((crate::driver::LinkInputs::default(), span));
            }
            cursor.finish()?;
            continue;
        }
        let key = cursor.key()?;
        cursor.expect("=")?;
        match section {
            "package" => {
                if !matches!(key.as_str(), "name" | "version" | "namespace") {
                    return Err(
                        cursor.error("unknown package key; expected name, version, or namespace")
                    );
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
                let mut native = false;
                if cursor.take(",") {
                    if cursor.key()? != "native" {
                        return Err(cursor
                            .error("only the path and native keys are supported in a dependency"));
                    }
                    cursor.expect("=")?;
                    native = if cursor.take("true") {
                        true
                    } else if cursor.take("false") {
                        false
                    } else {
                        return Err(cursor.error("native must be true or false"));
                    };
                }
                cursor.expect("}")?;
                if dependencies
                    .insert(
                        key,
                        Dependency {
                            path: path.into(),
                            span,
                            native,
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
            "native" => {
                const FORM: &str = "expected [native] keys link, libraries or search, each an array of strings on one line";
                if !matches!(key.as_str(), "link" | "libraries" | "search")
                    || !native_keys.insert(key.clone())
                {
                    return Err(cursor.error(FORM));
                }
                let values = cursor.string_array(FORM)?;
                let inputs = &mut native
                    .as_mut()
                    .expect("the [native] header precedes its keys")
                    .0;
                match key.as_str() {
                    "libraries" => {
                        if let Some(message) = values
                            .iter()
                            .find_map(|name| crate::driver::library_name_error(name))
                        {
                            return Err(cursor.error(&message));
                        }
                        inputs.libraries = values;
                    }
                    _ => {
                        let paths = values
                            .into_iter()
                            .map(|value| {
                                relative_path(value).ok_or_else(|| {
                                    cursor.error(
                                        "link and search paths must be nonempty relative paths",
                                    )
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        if key == "link" {
                            inputs.paths = paths;
                        } else {
                            inputs.search = paths;
                        }
                    }
                }
            }
            _ => return Err(cursor.error("expected [package] before fields")),
        }
        cursor.finish()?;
    }
    let missing = || Diagnostic::new("E0002", "[package] requires name and version", whole);
    let (name, span) = fields.remove("name").ok_or_else(missing)?;
    let (version, _) = fields.remove("version").ok_or_else(missing)?;
    let derived = namespace(&name, span)?;
    let namespace = match fields.remove("namespace") {
        Some((namespace, span)) if !valid_namespace(&namespace) => {
            return Err(Diagnostic::new(
                "E1011",
                "package namespace must be dotted identifiers such as \"Acme.Tools\", at most 16 segments and 255 bytes, that do not start with a standard library module name",
                span,
            ));
        }
        Some((namespace, _)) => namespace,
        None => derived,
    };
    Ok(Manifest {
        namespace,
        name,
        version,
        dependencies,
        wasm,
        native,
    })
}

/// A nonempty path that stays under the package root: `[dependencies]` and `[native]` paths.
fn relative_path(path: String) -> Option<PathBuf> {
    // Windows treats `/dir` and `C:dir` as non-absolute, but both escape the package root.
    let rooted = Path::new(&path).has_root()
        || matches!(
            Path::new(&path).components().next(),
            Some(Component::Prefix(_))
        );
    (!path.is_empty() && !path.contains('\0') && !rooted).then(|| path.into())
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

    /// A one-line array of strings; `form` is the error for anything else.
    fn string_array(&mut self, form: &str) -> Result<Vec<String>, Diagnostic> {
        if !self.take("[") {
            return Err(self.error(form));
        }
        let mut values = Vec::new();
        while !self.take("]") {
            values.push(self.string()?);
            if !self.take(",") && !self.rest.trim_start().starts_with(']') {
                return Err(self.error(form));
            }
        }
        Ok(values)
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

    #[test]
    fn parses_native_link_section() {
        for source in [
            format!(
                "{PACKAGE}[native]\nlink = [\"host.o\", \"vendor/libutil.a\"]\nlibraries = [\"sqlite3\", \"m\"] # system\nsearch = [\"vendor\",]\n"
            ),
            format!(
                "{PACKAGE}[dependencies]\n[wasm]\nmax-memory = \"64MiB\"\n[native]\r\nsearch = [ \"vendor\" ]\r\nlibraries = [\"sqlite3\",\"m\"]\r\nlink = [\"host.o\" , \"vendor/libutil.a\"]\r\n"
            ),
        ] {
            let (inputs, span) = parse_manifest(&source, 4).unwrap().native.unwrap();
            assert_eq!(
                inputs.paths,
                [PathBuf::from("host.o"), PathBuf::from("vendor/libutil.a")]
            );
            assert_eq!(inputs.libraries, ["sqlite3", "m"]);
            assert_eq!(inputs.search, [PathBuf::from("vendor")]);
            assert_eq!(span.source, Some(4));
            assert_eq!(&source[span.start..span.end], "[native]");
        }
        let (inputs, _) = parse_manifest(&format!("{PACKAGE}[native]\nlink = []\n"), 0)
            .unwrap()
            .native
            .unwrap();
        assert!(inputs.is_empty());
        assert_eq!(parse_manifest(PACKAGE, 0).unwrap().native, None);
    }

    #[test]
    fn rejects_malformed_native_link_section() {
        let form = "expected [native] keys link, libraries or search, each an array of strings on one line";
        for (suffix, message) in [
            ("[native]\nlink = \"host.o\"", form),
            ("[native]\nlink = [\"a\"\n", form),
            ("[native]\nlink = [\"a\" \"b\"]", form),
            ("[native]\nflags = [\"-O2\"]", form),
            ("[native]\nlink = [\"a\"]\nlink = [\"b\"]", form),
            (
                "[native]\nlibraries = [\"libm\"]",
                "invalid library name 'libm'; pass the name without the 'lib' prefix or extension, for example -l sqlite3",
            ),
            (
                "[native]\nlink = [\"/abs/host.o\"]",
                "link and search paths must be nonempty relative paths",
            ),
            (
                "[native]\nsearch = [\"\"]",
                "link and search paths must be nonempty relative paths",
            ),
            (
                "[native]\n[native]",
                "expected [package] followed by optional [dependencies], [wasm] and [native], each once",
            ),
        ] {
            let error = parse_manifest(&format!("{PACKAGE}{suffix}"), 5).unwrap_err();
            assert_eq!(error.code, "E0002", "{suffix}");
            assert_eq!(error.message, message, "{suffix}");
            assert_eq!(error.span.source, Some(5));
        }
        // [native] needs [package] first, like every other section.
        assert_eq!(
            parse_manifest("[native]\nlink = [\"a\"]\n[package]", 0)
                .unwrap_err()
                .code,
            "E0002"
        );
    }

    #[test]
    fn parses_the_native_opt_in_of_a_dependency() {
        let manifest = parse_manifest(
            &format!(
                "{PACKAGE}[dependencies]\nplain = {{ path = \"plain\" }}\nhost-lib = {{ path = \"../host\", native = true }}\nquiet = {{ path = \"quiet\" , native = false }} # no\n"
            ),
            0,
        )
        .unwrap();
        assert!(!manifest.dependencies["plain"].native);
        assert!(manifest.dependencies["host-lib"].native);
        assert!(!manifest.dependencies["quiet"].native);
        assert_eq!(
            manifest.dependencies["host-lib"].path,
            PathBuf::from("../host")
        );
        for (entry, message) in [
            (
                "a = { path = \"a\", native = yes }",
                "native must be true or false",
            ),
            ("a = { path = \"a\", native }", "expected '='"),
            (
                "a = { path = \"a\", link = true }",
                "only the path and native keys are supported in a dependency",
            ),
            ("a = { path = \"a\", native = true", "expected '}'"),
            (
                "a = { path = \"a\", native = true, native = true }",
                "expected '}'",
            ),
        ] {
            let error =
                parse_manifest(&format!("{PACKAGE}[dependencies]\n{entry}\n"), 0).unwrap_err();
            assert_eq!(error.code, "E0002", "{entry}");
            assert_eq!(error.message, message, "{entry}");
        }
    }

    #[test]
    fn parses_an_explicit_namespace() {
        let manifest =
            parse_manifest(&format!("{PACKAGE}namespace = \"Acme.Tools\"\n"), 0).unwrap();
        assert_eq!(manifest.namespace, "Acme.Tools");
        assert_eq!(parse_manifest(PACKAGE, 0).unwrap().namespace, "SampleApp");
        let long = vec!["A"; 17].join(".");
        for value in [
            "acme-tools",
            "Acme..Tools",
            "Acme.",
            "IO.Extra",
            "match",
            "Task",
            "_",
            "1st",
            long.as_str(),
        ] {
            let source = format!("{PACKAGE}namespace = \"{value}\"\n");
            let error = parse_manifest(&source, 0).unwrap_err();
            assert_eq!(error.code, "E1011", "{value}: {}", error.message);
        }
        for suffix in ["namespace = \"\"", "namespace = \"A\"\nnamespace = \"B\""] {
            let error = parse_manifest(&format!("{PACKAGE}{suffix}\n"), 0).unwrap_err();
            assert_eq!(error.code, "E0002", "{suffix}");
        }
        assert!(valid_namespace("lower.case_1"));
    }

    #[test]
    fn creates_projects_that_declare_their_namespace() {
        for (folder, name) in [
            ("MyApp", "my-app"),
            ("hello_world", "hello-world"),
            ("ex1", "ex1"),
            ("2024 app", "app"),
            ("\u{65e5}\u{672c}", "app"),
        ] {
            assert_eq!(package_name(folder), name, "{folder}");
        }
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-new-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let created = create_project(&root.join("hello-world"), None).unwrap();
        assert_eq!(created.len(), 3);
        let manifest = parse_manifest(&std::fs::read_to_string(&created[0]).unwrap(), 0).unwrap();
        assert_eq!(
            (manifest.name.as_str(), manifest.namespace.as_str()),
            ("hello-world", "HelloWorld")
        );
        let main = std::fs::read_to_string(&created[1]).unwrap();
        assert!(main.starts_with("namespace HelloWorld\n\n"), "{main}");
        let again = create_project(&root.join("hello-world"), None).unwrap_err();
        assert!(again.message.contains("is not empty"), "{}", again.message);
        create_project(&root.join("other"), Some("Acme.Tools")).unwrap();
        assert!(
            std::fs::read_to_string(root.join("other/Tsuzuri.toml"))
                .unwrap()
                .contains("namespace = \"Acme.Tools\"")
        );
        for namespace in ["IO", "acme-tools", "A..B"] {
            assert!(
                create_project(&root.join("bad"), Some(namespace)).is_err(),
                "{namespace}"
            );
        }
        assert!(!root.join("bad").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
