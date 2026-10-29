use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

use crate::capabilities::{Capability, CapabilitySet};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use crate::syntax::Edition;

/// The largest `Tsuzuri.toml`, `Tsuzuri.lock` and registry index file. Sources have their own
/// limit, `syntax::MAX_SOURCE_BYTES`.
pub const MAX_PACKAGE_FILE_BYTES: usize = 1024 * 1024;

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
    /// The language edition that the package's sources are read with (G19).
    pub edition: Edition,
    pub dependencies: BTreeMap<String, Dependency>,
    pub wasm: WasmSettings,
    /// The `[native]` link inputs, with paths still relative to the package root, and the span of its header.
    pub native: Option<(crate::driver::LinkInputs, Span)>,
    /// The `[registry]` index. `tsuzuri fetch` reads only the root package's section.
    pub registry: Option<Registry>,
    /// The `allow` of `[capabilities]` and the span of its header (E15). Only the root
    /// package's section counts.
    pub capabilities: Option<(CapabilitySet, Span)>,
}

/// A registry: a git repository whose `index/<name>.json` files list the
/// published versions of each package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    pub index: String,
    /// The index commit to resolve against; the repository's `HEAD` when absent.
    pub rev: Option<String>,
    /// The `[registry]` header.
    pub span: Span,
}

/// A package version `MAJOR.MINOR.PATCH`. As a requirement it accepts itself and
/// every later compatible version: the same `MAJOR` from 1.0.0 on, and the same
/// `0.MINOR` before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

pub const VERSION_RULE: &str = "versions are written as MAJOR.MINOR.PATCH in decimal without leading zeros, such as \"1.2.3\"; ranges, operators, and pre-release tags are not supported";
pub const VERSION_FORM: &str = "registry dependencies are written as { version = \"1.2.3\" }";

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.').map(|part| {
            (!part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == "0" || !part.starts_with('0')))
            .then(|| part.parse::<u64>().ok())
            .flatten()
        });
        let version = Self {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next()??,
        };
        parts.next().is_none().then_some(version)
    }

    /// Whether `other` is in this version's compatibility range.
    pub fn compatible(self, other: Self) -> bool {
        self.series() == other.series()
    }

    /// The compatibility range of this version: its major version from 1.0.0 on,
    /// and `0.minor` before that. Compatible versions have the same series.
    pub fn series(self) -> (u64, u64) {
        (self.major, if self.major == 0 { self.minor } else { 0 })
    }

    /// Whether this version satisfies `requirement`: compatible and not older.
    pub fn satisfies(self, requirement: Self) -> bool {
        requirement.compatible(self) && self >= requirement
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
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

/// Where a dependency comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencySource {
    /// A directory relative to the declaring package's root.
    Path(PathBuf),
    /// One commit of a git repository. `tsuzuri fetch` downloads it into the
    /// package store and records it in `Tsuzuri.lock`.
    Git { url: String, rev: String },
    /// A version requirement that `tsuzuri fetch` resolves in the registry index.
    Registry(Version),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub source: DependencySource,
    pub span: Span,
    /// `native = true`: the root package lets this dependency's `[native]` link inputs into the build.
    pub native: bool,
    /// `capabilities = [...]`: what the declaring package lets this dependency use (E15).
    pub capabilities: Option<CapabilitySet>,
}

pub const GIT_FORM: &str =
    "git dependencies are written as { git = \"https://...\", rev = \"<40 lowercase hex>\" }";
pub const GIT_URL_RULE: &str = "git urls must start with https:// or file:/// and must not contain credentials, spaces, '?', or '#'";
pub const GIT_REV_RULE: &str =
    "rev must be a full 40-character lowercase hex commit id; branches and tags are not supported";

/// A git url that a manifest or `Tsuzuri.lock` may name: `https://host/...` or
/// `file:///...`, at most 2048 printable ASCII bytes without `@` (credentials),
/// `?`, `#`, or `\`.
pub fn valid_git_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .filter(|rest| !rest.starts_with('/'))
        .or_else(|| url.strip_prefix("file:///"));
    rest.is_some_and(|rest| !rest.is_empty())
        && url.len() <= 2048
        && url.bytes().all(|byte| {
            (0x21..=0x7e).contains(&byte) && !matches!(byte, b'@' | b'?' | b'#' | b'\\')
        })
}

/// A full SHA-1 commit id in lowercase hex.
pub fn valid_rev(rev: &str) -> bool {
    lowercase_hex(rev, 40)
}

fn lowercase_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// A git or registry package recorded in `Tsuzuri.lock`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    pub git: String,
    pub rev: String,
    /// The [`content_sha256`] of the package.
    pub sha256: String,
    /// The version that `tsuzuri fetch` selected for a registry package.
    pub version: Option<Version>,
}

/// Reads `Tsuzuri.lock`: a JSON object `{"format": 1 or 2, "packages": [...]}`
/// whose entries have exactly the keys `name`, `git`, `rev`, and `sha256`, and in
/// format 2 optionally `version`. Key order and whitespace are free; unknown or
/// duplicate keys and names are errors.
pub fn parse_lock(text: &str, source_id: usize) -> Result<BTreeMap<String, LockEntry>, Diagnostic> {
    use serde_json::Value;
    let span = Span::new(0, 0).in_source(source_id);
    if text.len() > MAX_PACKAGE_FILE_BYTES {
        return Err(Diagnostic::of(
            DiagnosticKind::LockfileTooLarge,
            "Tsuzuri.lock exceeds 1 MiB",
            span,
        ));
    }
    let invalid = |detail: String| {
        Diagnostic::of(
            DiagnosticKind::InvalidLockfile,
            format!(
                "Tsuzuri.lock is not a valid lockfile ({detail}); restore it from version control or delete it and run tsuzuri fetch"
            ),
            span,
        )
    };
    let value: Value = serde_json::from_str(text).map_err(|error| invalid(error.to_string()))?;
    if let Some(key) = duplicate_json_key(text) {
        return Err(invalid(format!("duplicate key '{key}'")));
    }
    let object = value
        .as_object()
        .ok_or_else(|| invalid("expected an object".into()))?;
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "format" | "packages"))
    {
        return Err(invalid(format!("unknown key '{key}'")));
    }
    let format = object.get("format").and_then(Value::as_u64);
    if !matches!(format, Some(1 | 2)) {
        return Err(invalid("format must be 1 or 2".into()));
    }
    let packages = object
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("packages must be an array".into()))?;
    let mut entries = BTreeMap::new();
    for package in packages {
        let fields = package
            .as_object()
            .ok_or_else(|| invalid("each package must be an object".into()))?;
        if let Some(key) = fields.keys().find(|key| {
            !matches!(key.as_str(), "name" | "git" | "rev" | "sha256")
                && (format == Some(1) || key.as_str() != "version")
        }) {
            return Err(invalid(format!("unknown package key '{key}'")));
        }
        let field = |key: &str| {
            fields
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(format!("each package needs a string {key}")))
        };
        let name = field("name")?;
        if namespace(name, span).is_err() {
            return Err(invalid(format!("invalid package name '{name}'")));
        }
        let version = match fields.get("version") {
            None => None,
            Some(version) => Some(
                version
                    .as_str()
                    .and_then(Version::parse)
                    .ok_or_else(|| invalid(format!("invalid version of '{name}'")))?,
            ),
        };
        let entry = LockEntry {
            git: field("git")?.to_owned(),
            rev: field("rev")?.to_owned(),
            sha256: field("sha256")?.to_owned(),
            version,
        };
        if !valid_git_url(&entry.git) {
            return Err(invalid(format!("invalid git url of '{name}'")));
        }
        if !valid_rev(&entry.rev) {
            return Err(invalid(format!("invalid rev of '{name}'")));
        }
        if !lowercase_hex(&entry.sha256, 64) {
            return Err(invalid(format!("invalid sha256 of '{name}'")));
        }
        if entries.insert(name.to_owned(), entry).is_some() {
            return Err(invalid(format!("duplicate package '{name}'")));
        }
    }
    Ok(entries)
}

/// The canonical `Tsuzuri.lock`: two-space indentation, LF line ends, one final
/// newline, and packages in name order with the keys `name`, `version` (registry
/// packages only), `git`, `rev`, `sha256`. It is format 1 without registry
/// packages, so lockfiles of git dependencies keep their bytes, and 2 with them.
pub fn render_lock(entries: &BTreeMap<String, LockEntry>) -> String {
    use crate::diagnostic::json_string;
    let format = if entries.values().any(|entry| entry.version.is_some()) {
        2
    } else {
        1
    };
    let mut text = format!("{{\n  \"format\": {format},\n  \"packages\": [");
    for (index, (name, entry)) in entries.iter().enumerate() {
        text += if index == 0 { "\n" } else { ",\n" };
        text += &format!("    {{\n      \"name\": {},\n", json_string(name));
        if let Some(version) = entry.version {
            text += &format!("      \"version\": \"{version}\",\n");
        }
        text += &format!(
            "      \"git\": {},\n      \"rev\": {},\n      \"sha256\": {}\n    }}",
            json_string(&entry.git),
            json_string(&entry.rev),
            json_string(&entry.sha256)
        );
    }
    text += if entries.is_empty() {
        "]\n}\n"
    } else {
        "\n  ]\n}\n"
    };
    text
}

/// One published version in a registry index file `index/<name>.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub git: String,
    pub rev: String,
    pub sha256: String,
    /// The version requirements of the package's `[dependencies]`.
    pub dependencies: BTreeMap<String, Version>,
}

/// Reads the registry index file of package `name`:
/// `{"name": "<name>", "versions": [{"version", "git", "rev", "sha256", "dependencies"}, ...]}`.
/// Every key is required, unknown and duplicate keys are errors, and each version
/// occurs once. An error is the detail for the diagnostic.
pub fn parse_index(name: &str, text: &str) -> Result<BTreeMap<Version, IndexEntry>, String> {
    use serde_json::Value;
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if let Some(key) = duplicate_json_key(text) {
        return Err(format!("duplicate key '{key}'"));
    }
    let object = value.as_object().ok_or("expected an object")?;
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "name" | "versions"))
    {
        return Err(format!("unknown key '{key}'"));
    }
    if object.get("name").and_then(Value::as_str) != Some(name) {
        return Err(format!("name must be \"{name}\""));
    }
    let mut versions = BTreeMap::new();
    for version in object
        .get("versions")
        .and_then(Value::as_array)
        .ok_or("versions must be an array")?
    {
        let fields = version
            .as_object()
            .ok_or("each version must be an object")?;
        if let Some(key) = fields.keys().find(|key| {
            !matches!(
                key.as_str(),
                "version" | "git" | "rev" | "sha256" | "dependencies"
            )
        }) {
            return Err(format!("unknown version key '{key}'"));
        }
        let field = |key: &str| {
            fields
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("each version needs a string {key}"))
        };
        let number = Version::parse(field("version")?)
            .ok_or_else(|| format!("invalid version '{}'", field("version").unwrap_or_default()))?;
        let entry = IndexEntry {
            git: field("git")?.to_owned(),
            rev: field("rev")?.to_owned(),
            sha256: field("sha256")?.to_owned(),
            dependencies: fields
                .get("dependencies")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("version {number} needs a dependencies object"))?
                .iter()
                .map(|(dependency, requirement)| {
                    let requirement = requirement.as_str().and_then(Version::parse);
                    match requirement {
                        Some(requirement) if namespace(dependency, Span::default()).is_ok() => {
                            Ok((dependency.clone(), requirement))
                        }
                        _ => Err(format!(
                            "invalid dependency '{dependency}' of version {number}"
                        )),
                    }
                })
                .collect::<Result<_, _>>()?,
        };
        if !valid_git_url(&entry.git) {
            return Err(format!("invalid git url of version {number}"));
        }
        if !valid_rev(&entry.rev) {
            return Err(format!("invalid rev of version {number}"));
        }
        if !lowercase_hex(&entry.sha256, 64) {
            return Err(format!("invalid sha256 of version {number}"));
        }
        if entry.dependencies.len() > 1024 {
            return Err(format!("version {number} has more than 1024 dependencies"));
        }
        if versions.insert(number, entry).is_some() {
            return Err(format!("duplicate version {number}"));
        }
    }
    Ok(versions)
}

/// One version's entry for `index/<name>.json`, as `tsuzuri publish` prints it.
pub fn render_index_entry(version: Version, entry: &IndexEntry) -> String {
    use crate::diagnostic::json_string;
    let dependencies: Vec<_> = entry
        .dependencies
        .iter()
        .map(|(name, requirement)| format!("    {}: \"{requirement}\"", json_string(name)))
        .collect();
    format!(
        "{{\n  \"version\": \"{version}\",\n  \"git\": {},\n  \"rev\": {},\n  \"sha256\": {},\n  \"dependencies\": {}\n}}\n",
        json_string(&entry.git),
        json_string(&entry.rev),
        json_string(&entry.sha256),
        if dependencies.is_empty() {
            "{}".to_owned()
        } else {
            format!("{{\n{}\n  }}", dependencies.join(",\n"))
        }
    )
}

/// The SHA-256 of a package's files, keyed by `/`-separated paths relative to the
/// package root: its `Tsuzuri.toml` and every `.tz`, `.tt`, and `.tc` source.
pub fn content_sha256<Bytes: AsRef<[u8]>>(files: &BTreeMap<String, Bytes>) -> String {
    let mut hash = crate::cache::Sha256::new();
    hash.field("tsuzuri-package", b"1");
    for (path, bytes) in files {
        hash.field("path", path.as_bytes());
        hash.field("bytes", bytes.as_ref());
    }
    hash.hex()
}

/// The first key that occurs twice in one object of `text`, which must be JSON
/// that `serde_json` accepted (`serde_json` keeps only the last duplicate).
fn duplicate_json_key(text: &str) -> Option<String> {
    let mut objects: Vec<Option<BTreeSet<String>>> = Vec::new();
    let mut expect_key = false;
    let mut characters = text.char_indices();
    while let Some((start, character)) = characters.next() {
        match character {
            '{' => {
                objects.push(Some(BTreeSet::new()));
                expect_key = true;
            }
            '[' => {
                objects.push(None);
                expect_key = false;
            }
            '}' | ']' => {
                objects.pop();
                expect_key = false;
            }
            ',' => expect_key = matches!(objects.last(), Some(Some(_))),
            '"' => {
                let mut escaped = false;
                let mut end = start;
                for (index, character) in characters.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        end = index;
                        break;
                    }
                }
                if std::mem::take(&mut expect_key) {
                    let key: String = serde_json::from_str(&text[start..=end]).ok()?;
                    if let Some(Some(keys)) = objects.last_mut()
                        && !keys.insert(key.clone())
                    {
                        return Some(key);
                    }
                }
            }
            _ => {}
        }
    }
    None
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
        return Err(Diagnostic::of(
            DiagnosticKind::InvalidPackageName,
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

/// Whether `text` can be a package's default namespace: identifiers joined by
/// `::`, at most 16 segments and 255 bytes, whose first segment is neither
/// the std namespace nor a standard library module name. A segment that
/// `edition` reserves is not an identifier.
pub fn valid_namespace(text: &str, edition: Edition) -> bool {
    use crate::syntax::{Token, TokenKind};
    text.len() <= 255
        && text.split("::").count() <= 16
        && text.split("::").next().is_some_and(|first| {
            first != crate::stdlib::NAMESPACE && !crate::stdlib::is_reserved_module(first)
        })
        && text.split("::").all(|segment| {
            segment != "_"
            && segment != "Task"
            && crate::lexer::lex(segment, edition).is_ok_and(|tokens| {
                matches!(
                    tokens.as_slice(),
                    [Token { kind: TokenKind::Ident(name), .. }, Token { kind: TokenKind::End, .. }]
                        if name == segment
                )
            })
        })
}

/// Creates a package in `directory`, which must be missing or empty: a
/// `Tsuzuri.toml` that names its default namespace and the latest language
/// edition, a `Main.tz` that declares the namespace, and a `.gitignore`.
/// Returns the created files.
/// Creates a project in an empty or missing `directory`: `Tsuzuri.toml`, `Main.tz`, and
/// `.gitignore`, and with `agent_files`, `AGENTS.md` and `llms.txt` for AI agents (G21).
pub fn create_project(
    directory: &Path,
    namespace: Option<&str>,
    agent_files: bool,
) -> Result<Vec<PathBuf>, Diagnostic> {
    create_project_with(directory, namespace, agent_files, Edition::NEW_PROJECT)
}

/// `create_project` whose manifest declares `edition` (`tsuzuri new --edition`, A22).
pub fn create_project_with(
    directory: &Path,
    namespace: Option<&str>,
    agent_files: bool,
    edition: Edition,
) -> Result<Vec<PathBuf>, Diagnostic> {
    let error =
        |kind: DiagnosticKind, message: String| Diagnostic::of(kind, message, Span::default());
    let resolved = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_owned());
    let folder = resolved
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let name = package_name(folder);
    let namespace = match namespace {
        Some(namespace) if valid_namespace(namespace, edition) => namespace.to_owned(),
        Some(namespace) => {
            return Err(error(DiagnosticKind::InvalidNamespaceArgument, format!(
                "invalid namespace '{namespace}'; use identifiers joined by '::' such as Acme::Tools, at most 16 segments and 255 bytes, that do not start with 'std' or a standard library module name"
            )));
        }
        None => self::namespace(&name, Span::default())
            .ok()
            .filter(|namespace| valid_namespace(namespace, edition))
            .ok_or_else(|| {
                error(DiagnosticKind::CannotDeriveNamespace, format!(
                    "cannot derive a namespace from the folder name '{folder}'; pass --namespace"
                ))
            })?,
    };
    let io = |action: &str, path: &Path, cause: std::io::Error| {
        error(
            DiagnosticKind::NewProjectIoFailure,
            format!("cannot {action} '{}': {cause}", path.display()),
        )
    };
    match std::fs::read_dir(directory) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(error(
                    DiagnosticKind::NewProjectFolderNotEmpty,
                    format!(
                        "'{}' is not empty; choose a new or empty folder so that no file is overwritten",
                        directory.display()
                    ),
                ));
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
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\nnamespace = \"{namespace}\"\n"
            ),
        ),
        (
            "Main.tz",
            format!(
                "namespace {namespace}\n\ndef main :: unit -> i32 = \\() ->\n    do! IO.writeln \"Hello, Tsuzuri!\"\n    0\n\ntest \"adds numbers\" = assert (1 + 2 == 3)\n"
            ),
        ),
        (".gitignore", ".tsuzuri/\ntarget/\n".to_owned()),
    ];
    let values = crate::guide::Values {
        name: &name,
        namespace: &namespace,
        edition,
        version: env!("CARGO_PKG_VERSION"),
    };
    let agents = [
        ("AGENTS.md", crate::guide::agents_md(&values)),
        ("llms.txt", crate::guide::llms_txt(&values)),
    ];
    let files = files
        .into_iter()
        .chain(agents.into_iter().filter(|_| agent_files));
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
    if source.len() > MAX_PACKAGE_FILE_BYTES {
        return Err(Diagnostic::of(
            DiagnosticKind::ManifestTooLarge,
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
    let mut registry: Option<(Option<String>, Option<String>, Span)> = None;
    let mut capabilities: Option<(Option<CapabilitySet>, Span)> = None;
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
            } else if cursor.take("[registry]") {
                Some("registry")
            } else if cursor.take("[capabilities]") {
                Some("capabilities")
            } else {
                cursor.take("[native]").then_some("native")
            };
            match next {
                Some(next) if sections.insert(next) => section = next,
                _ => {
                    return Err(cursor.error(DiagnosticKind::ManifestSectionOrder,
                        "expected [package] followed by optional [dependencies], [wasm], [native], [registry] and [capabilities], each once",
                    ));
                }
            }
            if section == "native" {
                native = Some((crate::driver::LinkInputs::default(), span));
            }
            if section == "registry" {
                registry = Some((None, None, span));
            }
            if section == "capabilities" {
                capabilities = Some((None, span));
            }
            cursor.finish()?;
            continue;
        }
        let key = cursor.key()?;
        cursor.expect("=")?;
        match section {
            "package" => {
                if !matches!(key.as_str(), "name" | "version" | "namespace" | "edition") {
                    return Err(cursor.error(
                        DiagnosticKind::UnknownManifestKey,
                        "unknown package key; expected name, version, namespace, or edition",
                    ));
                }
                let value = cursor.string()?;
                if value.is_empty() || fields.insert(key, (value, span)).is_some() {
                    return Err(cursor.error(
                        DiagnosticKind::InvalidPackageField,
                        "package fields must be nonempty and occur exactly once",
                    ));
                }
            }
            "dependencies" => {
                namespace(&key, span)?;
                cursor.expect("{")?;
                let (source, native, granted) = match cursor.key()?.as_str() {
                    "path" => cursor.path_dependency()?,
                    "git" => {
                        let (source, granted) = cursor.git_dependency()?;
                        (source, false, granted)
                    }
                    "version" => {
                        let (source, granted) = cursor.registry_dependency()?;
                        (source, false, granted)
                    }
                    "rev" | "branch" | "tag" => {
                        return Err(cursor.error(DiagnosticKind::InvalidGitDependency, GIT_FORM));
                    }
                    _ => {
                        return Err(cursor.error(
                            DiagnosticKind::InvalidDependencyForm,
                            "expected a path, git, or version dependency",
                        ));
                    }
                };
                if dependencies
                    .insert(
                        key,
                        Dependency {
                            source,
                            span,
                            native,
                            capabilities: granted,
                        },
                    )
                    .is_some()
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::DuplicateDependency,
                        "duplicate package dependency",
                        span,
                    ));
                }
                if dependencies.len() > 1024 {
                    return Err(Diagnostic::of(
                        DiagnosticKind::TooManyDependencies,
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
                        return Err(cursor.error(
                            DiagnosticKind::UnknownManifestKey,
                            "unknown wasm key; expected max-memory or stack-size",
                        ));
                    }
                };
                let text = cursor.string()?;
                let size = parse_size(&text).ok_or_else(|| {
                    cursor.error(DiagnosticKind::InvalidWasmSize, "WASM sizes must be a quoted byte count or a number followed by KiB, MiB, or GiB, such as \"64MiB\"")
                })?;
                if slot.replace(size).is_some() {
                    return Err(cursor.error(
                        DiagnosticKind::DuplicateManifestKey,
                        "wasm keys must occur at most once",
                    ));
                }
            }
            "registry" => {
                let (index, rev, _) = registry
                    .as_mut()
                    .expect("the [registry] header precedes its keys");
                let value = cursor.string()?;
                let (slot, valid, rule) = match key.as_str() {
                    "index" => (index, valid_git_url(&value), GIT_URL_RULE),
                    "rev" => (rev, valid_rev(&value), GIT_REV_RULE),
                    _ => {
                        return Err(cursor.error(
                            DiagnosticKind::UnknownManifestKey,
                            "unknown registry key; expected index or rev",
                        ));
                    }
                };
                if !valid {
                    return Err(cursor.error(DiagnosticKind::InvalidRegistrySetting, rule));
                }
                if slot.replace(value).is_some() {
                    return Err(cursor.error(
                        DiagnosticKind::DuplicateManifestKey,
                        "registry keys must occur at most once",
                    ));
                }
            }
            "capabilities" => {
                let (allow, _) = capabilities
                    .as_mut()
                    .expect("the [capabilities] header precedes its keys");
                if key != "allow" {
                    return Err(cursor.error(
                        DiagnosticKind::UnknownManifestKey,
                        "unknown capabilities key; expected allow",
                    ));
                }
                let names = cursor.capability_names("allow")?;
                if allow.replace(names).is_some() {
                    return Err(cursor.error(
                        DiagnosticKind::DuplicateManifestKey,
                        "capabilities keys must occur at most once",
                    ));
                }
            }
            "native" => {
                const FORM: &str = "expected [native] keys link, libraries or search, each an array of strings on one line";
                if !matches!(key.as_str(), "link" | "libraries" | "search")
                    || !native_keys.insert(key.clone())
                {
                    return Err(cursor.error(DiagnosticKind::InvalidNativeSection, FORM));
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
                            return Err(
                                cursor.error(DiagnosticKind::InvalidNativeLibraryName, &message)
                            );
                        }
                        inputs.libraries = values;
                    }
                    _ => {
                        let paths = values
                            .into_iter()
                            .map(|value| {
                                relative_path(value).ok_or_else(|| {
                                    cursor.error(
                                        DiagnosticKind::InvalidNativePath,
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
            _ => {
                return Err(cursor.error(
                    DiagnosticKind::ManifestSectionOrder,
                    "expected [package] before fields",
                ));
            }
        }
        cursor.finish()?;
    }
    let missing = || {
        Diagnostic::of(
            DiagnosticKind::MissingPackageField,
            "[package] requires name and version",
            whole,
        )
    };
    let registry = match registry {
        None => None,
        Some((Some(index), rev, span)) => Some(Registry { index, rev, span }),
        Some((None, _, span)) => {
            return Err(Diagnostic::of(
                DiagnosticKind::MissingRegistryIndex,
                "[registry] requires index = \"https://...\"",
                span,
            ));
        }
    };
    let capabilities = match capabilities {
        None => None,
        Some((Some(allow), span)) => Some((allow, span)),
        Some((None, span)) => {
            return Err(Diagnostic::of(
                DiagnosticKind::MissingCapabilitiesAllow,
                "[capabilities] requires allow = [\"fs\", ...]",
                span,
            ));
        }
    };
    let (name, span) = fields.remove("name").ok_or_else(missing)?;
    let (version, _) = fields.remove("version").ok_or_else(missing)?;
    let edition = match fields.remove("edition") {
        None => Edition::MANIFEST_DEFAULT,
        Some((value, span)) => Edition::parse(&value).ok_or_else(|| {
            Diagnostic::of(
                DiagnosticKind::UnsupportedEdition,
                format!(
                    "unsupported edition '{value}'; supported editions: {}",
                    Edition::supported_names()
                ),
                span,
            )
        })?,
    };
    let derived = namespace(&name, span)?;
    let namespace = match fields.remove("namespace") {
        Some((namespace, span)) if !valid_namespace(&namespace, edition) => {
            return Err(Diagnostic::of(
                DiagnosticKind::InvalidPackageNamespace,
                "package namespace must be identifiers joined by '::' such as \"Acme::Tools\", at most 16 segments and 255 bytes, that do not start with 'std' or a standard library module name",
                span,
            ));
        }
        // Namespaces are dotted inside the compiler, as module keys are.
        Some((namespace, _)) => namespace.replace("::", "."),
        None => derived,
    };
    Ok(Manifest {
        namespace,
        edition,
        name,
        version,
        dependencies,
        wasm,
        native,
        registry,
        capabilities,
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
    fn error(&self, kind: DiagnosticKind, message: &str) -> Diagnostic {
        Diagnostic::of(kind, message, self.span)
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
            Err(self.error(
                DiagnosticKind::ManifestSyntaxError,
                &format!("expected '{token}'"),
            ))
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
            return Err(self.error(DiagnosticKind::ManifestSyntaxError, "expected a bare key"));
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
                        return Err(self.error(
                            DiagnosticKind::ManifestSyntaxError,
                            "unterminated string escape",
                        ));
                    };
                    self.rest = &self.rest[escaped.len_utf8()..];
                    value.push(match escaped {
                        '"' | '\\' => escaped,
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            return Err(self.error(
                                DiagnosticKind::ManifestSyntaxError,
                                "unsupported string escape; use \\\" \\\\ \\n \\r or \\t",
                            ));
                        }
                    });
                }
                character if character.is_control() => {
                    return Err(self.error(
                        DiagnosticKind::ManifestSyntaxError,
                        "unescaped control character in string",
                    ));
                }
                character => value.push(character),
            }
        }
        Err(self.error(
            DiagnosticKind::ManifestSyntaxError,
            "unterminated quoted string",
        ))
    }

    /// A one-line array of strings; `form` is the error for anything else.
    fn string_array(&mut self, form: &str) -> Result<Vec<String>, Diagnostic> {
        if !self.take("[") {
            return Err(self.error(DiagnosticKind::InvalidNativeSection, form));
        }
        let mut values = Vec::new();
        while !self.take("]") {
            values.push(self.string()?);
            if !self.take(",") && !self.rest.trim_start().starts_with(']') {
                return Err(self.error(DiagnosticKind::InvalidNativeSection, form));
            }
        }
        Ok(values)
    }

    /// A one-line array of capability names after `key =` (E15): `allow` or `capabilities`.
    fn capability_names(&mut self, key: &str) -> Result<CapabilitySet, Diagnostic> {
        let form = format!(
            "{key} must be an array of capability names on one line, such as [\"fs\", \"time\"]"
        );
        if !self.take("[") {
            return Err(self.error(DiagnosticKind::InvalidCapabilities, &form));
        }
        let mut names = CapabilitySet::EMPTY;
        while !self.take("]") {
            self.space();
            if !self.rest.starts_with('"') {
                return Err(self.error(DiagnosticKind::InvalidCapabilities, &form));
            }
            let name = self.string()?;
            let Some(capability) = Capability::from_name(&name) else {
                return Err(self.error(
                    DiagnosticKind::UnknownCapability,
                    &format!(
                        "unknown capability '{name}'; expected one of {}",
                        Capability::NAMES
                    ),
                ));
            };
            if names.contains(capability) {
                return Err(self.error(
                    DiagnosticKind::DuplicateCapability,
                    &format!("capability '{name}' is listed twice"),
                ));
            }
            names.insert(capability);
            if !self.take(",") && !self.rest.trim_start().starts_with(']') {
                return Err(self.error(DiagnosticKind::InvalidCapabilities, &form));
            }
        }
        Ok(names)
    }

    /// `, capabilities = [...]` after the source keys of a dependency, if present (E15).
    fn dependency_capabilities(&mut self) -> Result<Option<CapabilitySet>, Diagnostic> {
        self.expect("=")?;
        self.capability_names("capabilities").map(Some)
    }

    fn finish(&mut self) -> Result<(), Diagnostic> {
        self.space();
        if self.rest.is_empty() || self.rest.starts_with('#') {
            Ok(())
        } else {
            Err(self.error(
                DiagnosticKind::ManifestSyntaxError,
                "unexpected content after manifest field",
            ))
        }
    }

    /// The rest of `{ path = "...", native = true, capabilities = [...] }` after `path`, through `}`.
    fn path_dependency(
        &mut self,
    ) -> Result<(DependencySource, bool, Option<CapabilitySet>), Diagnostic> {
        self.expect("=")?;
        let path = self.string()?;
        // Windows treats `/dir` and `C:dir` as non-absolute, but both escape the package root.
        let rooted = Path::new(&path).has_root()
            || matches!(
                Path::new(&path).components().next(),
                Some(Component::Prefix(_))
            );
        if path.is_empty() || path.contains('\0') || rooted {
            return Err(self.error(
                DiagnosticKind::InvalidDependencyPath,
                "dependency path must be a nonempty relative path",
            ));
        }
        let mut native = false;
        let mut capabilities = None;
        if self.take(",") {
            match self.key()?.as_str() {
                "native" => {
                    self.expect("=")?;
                    native = if self.take("true") {
                        true
                    } else if self.take("false") {
                        false
                    } else {
                        return Err(self.error(
                            DiagnosticKind::InvalidDependencyNativeFlag,
                            "native must be true or false",
                        ));
                    };
                    // Only `capabilities` may follow `native`.
                    if self.take(",") {
                        match self.key()?.as_str() {
                            "capabilities" => capabilities = self.dependency_capabilities()?,
                            "native" => {
                                return Err(self.error(
                                    DiagnosticKind::DuplicateManifestKey,
                                    "duplicate key 'native' in a dependency",
                                ));
                            }
                            _ => {
                                return Err(self.error(
                                    DiagnosticKind::UnknownManifestKey,
                                    "only the path, native, and capabilities keys are supported in a dependency",
                                ));
                            }
                        }
                    }
                }
                "capabilities" => capabilities = self.dependency_capabilities()?,
                _ => {
                    return Err(self.error(
                        DiagnosticKind::UnknownManifestKey,
                        "only the path, native, and capabilities keys are supported in a dependency",
                    ));
                }
            }
        }
        self.expect("}")?;
        Ok((DependencySource::Path(path.into()), native, capabilities))
    }

    /// The rest of `{ version = "1.2.3", capabilities = [...] }` after `version`, through `}`.
    fn registry_dependency(
        &mut self,
    ) -> Result<(DependencySource, Option<CapabilitySet>), Diagnostic> {
        if !self.take("=") {
            return Err(self.error(DiagnosticKind::InvalidRegistryDependency, VERSION_FORM));
        }
        let version = Version::parse(&self.string()?)
            .ok_or_else(|| self.error(DiagnosticKind::InvalidRegistryDependency, VERSION_RULE))?;
        let capabilities =
            self.trailing_capabilities(DiagnosticKind::InvalidRegistryDependency, VERSION_FORM)?;
        if !self.take("}") {
            return Err(self.error(DiagnosticKind::InvalidRegistryDependency, VERSION_FORM));
        }
        Ok((DependencySource::Registry(version), capabilities))
    }

    /// The optional `, capabilities = [...]` of a git or registry dependency; any other key is
    /// the dependency form's error.
    fn trailing_capabilities(
        &mut self,
        kind: DiagnosticKind,
        form: &str,
    ) -> Result<Option<CapabilitySet>, Diagnostic> {
        if !self.take(",") {
            return Ok(None);
        }
        if self.key().ok().as_deref() != Some("capabilities") {
            return Err(self.error(kind, form));
        }
        self.dependency_capabilities()
    }

    /// The rest of `{ git = "<url>", rev = "<commit>", capabilities = [...] }` after `git`, through `}`.
    fn git_dependency(&mut self) -> Result<(DependencySource, Option<CapabilitySet>), Diagnostic> {
        if !self.take("=") {
            return Err(self.error(DiagnosticKind::InvalidGitDependency, GIT_FORM));
        }
        let url = self.string()?;
        if !valid_git_url(&url) {
            return Err(self.error(DiagnosticKind::InvalidGitDependency, GIT_URL_RULE));
        }
        if !self.take(",") || self.key().ok().as_deref() != Some("rev") || !self.take("=") {
            return Err(self.error(DiagnosticKind::InvalidGitDependency, GIT_FORM));
        }
        let rev = self.string()?;
        if !valid_rev(&rev) {
            return Err(self.error(DiagnosticKind::InvalidGitDependency, GIT_REV_RULE));
        }
        let capabilities =
            self.trailing_capabilities(DiagnosticKind::InvalidGitDependency, GIT_FORM)?;
        if !self.take("}") {
            return Err(self.error(DiagnosticKind::InvalidGitDependency, GIT_FORM));
        }
        Ok((DependencySource::Git { url, rev }, capabilities))
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
            manifest.dependencies["geometry-core"].source,
            DependencySource::Path(PathBuf::from("../geo#\"\\\t\n\r"))
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
                "expected [package] followed by optional [dependencies], [wasm], [native], [registry] and [capabilities], each once",
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
            manifest.dependencies["host-lib"].source,
            DependencySource::Path(PathBuf::from("../host"))
        );
        for (entry, message) in [
            (
                "a = { path = \"a\", native = yes }",
                "native must be true or false",
            ),
            ("a = { path = \"a\", native }", "expected '='"),
            (
                "a = { path = \"a\", link = true }",
                "only the path, native, and capabilities keys are supported in a dependency",
            ),
            ("a = { path = \"a\", native = true", "expected '}'"),
            (
                "a = { path = \"a\", native = true, native = true }",
                "duplicate key 'native' in a dependency",
            ),
            (
                "a = { path = \"a\", native = true, capabilities_extra = [\"ffi\"] }",
                "only the path, native, and capabilities keys are supported in a dependency",
            ),
        ] {
            let error =
                parse_manifest(&format!("{PACKAGE}[dependencies]\n{entry}\n"), 0).unwrap_err();
            assert_eq!(error.code, "E0002", "{entry}");
            assert_eq!(error.message, message, "{entry}");
        }
    }

    #[test]
    fn parses_the_capabilities_section() {
        use crate::capabilities::Capability::*;
        let text = format!(
            "{PACKAGE}\n[dependencies]\nfscache = {{ path = \"../fscache\", capabilities = [\"fs\"] }}\nhost-lib = {{ path = \"../host\", native = true, capabilities = [\"ffi\"] }}\nremote = {{ git = \"https://example.org/remote.git\", rev = \"0123456789abcdef0123456789abcdef01234567\", capabilities = [] }}\nregistered = {{ version = \"1.0.0\", capabilities = [\"net\", \"time\"] }}\nplain = {{ path = \"../plain\" }}\n\n[capabilities]\nallow = [\"fs\", \"net\", \"stdio\", \"time\"]\n"
        );
        let manifest = parse_manifest(&text, 0).unwrap();
        let names = |set: CapabilitySet| set.iter().collect::<Vec<_>>();
        let (allow, span) = manifest.capabilities.unwrap();
        assert_eq!(names(allow), [Fs, Net, Stdio, Time]);
        assert_eq!(&text[span.start..span.end], "[capabilities]");
        let granted = |name: &str| manifest.dependencies[name].capabilities.map(names);
        assert_eq!(granted("fscache"), Some(vec![Fs]));
        assert_eq!(granted("host-lib"), Some(vec![Ffi]));
        assert!(manifest.dependencies["host-lib"].native);
        assert_eq!(granted("remote"), Some(vec![]));
        assert_eq!(granted("registered"), Some(vec![Net, Time]));
        assert_eq!(granted("plain"), None);
        assert!(parse_manifest(PACKAGE, 0).unwrap().capabilities.is_none());
        let empty = parse_manifest(&format!("{PACKAGE}[capabilities]\nallow = []\n"), 0).unwrap();
        assert_eq!(empty.capabilities.unwrap().0, CapabilitySet::EMPTY);
    }

    #[test]
    fn rejects_malformed_capabilities() {
        for (text, kind, message) in [
            (
                "[capabilities]\nallow = []\n[capabilities]\nallow = []\n",
                "manifest_section_order",
                "expected [package] followed by optional [dependencies], [wasm], [native], [registry] and [capabilities], each once",
            ),
            (
                "[capabilities]\n",
                "missing_capabilities_allow",
                "[capabilities] requires allow = [\"fs\", ...]",
            ),
            (
                "[capabilities]\ndeny = []\n",
                "unknown_manifest_key",
                "unknown capabilities key; expected allow",
            ),
            (
                "[capabilities]\nallow = []\nallow = [\"fs\"]\n",
                "duplicate_manifest_key",
                "capabilities keys must occur at most once",
            ),
            (
                "[capabilities]\nallow = \"fs\"\n",
                "invalid_capabilities",
                "allow must be an array of capability names on one line, such as [\"fs\", \"time\"]",
            ),
            (
                "[capabilities]\nallow = [1]\n",
                "invalid_capabilities",
                "allow must be an array of capability names on one line, such as [\"fs\", \"time\"]",
            ),
            (
                "[dependencies]\na = { path = \"a\", capabilities = \"fs\" }\n",
                "invalid_capabilities",
                "capabilities must be an array of capability names on one line, such as [\"fs\", \"time\"]",
            ),
            (
                "[capabilities]\nallow = [\"files\"]\n",
                "unknown_capability",
                "unknown capability 'files'; expected one of env, ffi, fs, gpu, net, process, random, stdio, time",
            ),
            (
                "[capabilities]\nallow = [\"fs\", \"fs\"]\n",
                "duplicate_capability",
                "capability 'fs' is listed twice",
            ),
            (
                "[dependencies]\na = { path = \"a\", capabilities = [], native = true }\n",
                "manifest_syntax_error",
                "expected '}'",
            ),
            (
                "[dependencies]\na = { git = \"https://example.org/a.git\", rev = \"0123456789abcdef0123456789abcdef01234567\", native = true }\n",
                "invalid_git_dependency",
                GIT_FORM,
            ),
            (
                "[dependencies]\na = { version = \"1.0.0\", native = true }\n",
                "invalid_registry_dependency",
                VERSION_FORM,
            ),
        ] {
            let error = parse_manifest(&format!("{PACKAGE}{text}"), 5).unwrap_err();
            assert_eq!(error.code, "E0002", "{text}");
            assert_eq!(error.kind, Some(kind), "{text}");
            assert_eq!(error.message, message, "{text}");
            assert_eq!(error.span.source, Some(5), "{text}");
        }
    }

    #[test]
    fn parses_dependency_capabilities_for_every_source_kind() {
        use crate::capabilities::Capability::*;
        for (entry, native, granted) in [
            ("{ path = \"a\", capabilities = [\"fs\"] }", false, vec![Fs]),
            (
                "{ path = \"a\", native = true, capabilities = [\"ffi\", \"fs\"] }",
                true,
                vec![Ffi, Fs],
            ),
            (
                "{ path = \"a\", native = false , capabilities = [ ] }",
                false,
                vec![],
            ),
            (
                "{ git = \"https://example.org/a.git\", rev = \"0123456789abcdef0123456789abcdef01234567\", capabilities = [\"net\"] }",
                false,
                vec![Net],
            ),
            (
                "{ version = \"2.1.0\", capabilities = [\"time\", \"env\"] }",
                false,
                vec![Env, Time],
            ),
        ] {
            let manifest =
                parse_manifest(&format!("{PACKAGE}[dependencies]\na = {entry}\n"), 0).unwrap();
            let dependency = &manifest.dependencies["a"];
            assert_eq!(dependency.native, native, "{entry}");
            assert_eq!(
                dependency
                    .capabilities
                    .map(|set| set.iter().collect::<Vec<_>>()),
                Some(granted),
                "{entry}"
            );
        }
    }

    #[test]
    fn parses_an_explicit_namespace() {
        let manifest =
            parse_manifest(&format!("{PACKAGE}namespace = \"Acme::Tools\"\n"), 0).unwrap();
        assert_eq!(manifest.namespace, "Acme.Tools");
        assert_eq!(parse_manifest(PACKAGE, 0).unwrap().namespace, "SampleApp");
        let long = vec!["A"; 17].join("::");
        for value in [
            "acme-tools",
            "Acme.Tools",
            "Acme::::Tools",
            "Acme:Tools",
            "Acme::",
            "IO::Extra",
            "Maybe",
            "std",
            "std::Tools",
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
        let latest = Edition::LATEST;
        assert!(valid_namespace("lower::case_1", latest));
        assert!(valid_namespace("Std", latest) && valid_namespace("Acme::std", latest));
    }

    #[test]
    fn parses_edition_and_defaults_missing_edition() {
        let edition =
            |keys: &str| parse_manifest(&format!("{PACKAGE}{keys}"), 0).map(|m| m.edition);
        assert_eq!(edition("edition = \"2026\"\n"), Ok(Edition::E2026));
        assert_eq!(edition(""), Ok(Edition::MANIFEST_DEFAULT));
        assert_eq!(
            edition("# edition = \"2030\"\n"),
            Ok(Edition::MANIFEST_DEFAULT)
        );
        // The key may stand anywhere in [package], before or after the others.
        let before = "[package]\nedition = \"2026\"\nname = \"app\"\nversion = \"1\"\n";
        assert_eq!(parse_manifest(before, 0).unwrap().edition, Edition::E2026);
        // Each case: the keys after PACKAGE, the zero-based line of the error, its message.
        for (keys, line, message) in [
            (
                "edition = \"2030\"\n",
                0,
                "unsupported edition '2030'; supported editions: 2026, 2027",
            ),
            (
                "edition = \" 2026\"\n",
                0,
                "unsupported edition ' 2026'; supported editions: 2026, 2027",
            ),
            ("edition = 2026\n", 0, "expected '\"'"),
            (
                "edition = \"\"\n",
                0,
                "package fields must be nonempty and occur exactly once",
            ),
            (
                "edition = \"2026\"\nedition = \"2026\"\n",
                1,
                "package fields must be nonempty and occur exactly once",
            ),
            (
                "license = \"MIT\"\n",
                0,
                "unknown package key; expected name, version, namespace, or edition",
            ),
        ] {
            let error = parse_manifest(&format!("{PACKAGE}{keys}"), 7).unwrap_err();
            assert_eq!(error.code, "E0002", "{keys}");
            assert_eq!(error.message, message, "{keys}");
            assert_eq!(error.span.source, Some(7), "{keys}");
            let start = PACKAGE.len() + keys.lines().take(line).map(|l| l.len() + 1).sum::<usize>();
            assert_eq!(error.span.start, start, "{keys}");
        }
        // The edition also reads the namespace: a segment is an identifier of that edition.
        let source = format!("{PACKAGE}edition = \"2026\"\nnamespace = \"Acme::Tools\"\n");
        assert_eq!(parse_manifest(&source, 0).unwrap().namespace, "Acme.Tools");
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
        let created = create_project(&root.join("hello-world"), None, true).unwrap();
        assert_eq!(created.len(), 5);
        assert!(created[3].ends_with("AGENTS.md") && created[4].ends_with("llms.txt"));
        // `target/` holds the build output and the knowledge graph (G27).
        assert_eq!(
            std::fs::read_to_string(&created[2]).unwrap(),
            ".tsuzuri/\ntarget/\n"
        );
        let manifest = parse_manifest(&std::fs::read_to_string(&created[0]).unwrap(), 0).unwrap();
        assert_eq!(
            (manifest.name.as_str(), manifest.namespace.as_str()),
            ("hello-world", "HelloWorld")
        );
        assert_eq!(manifest.edition, Edition::LATEST);
        assert!(
            std::fs::read_to_string(&created[0])
                .unwrap()
                .contains(&format!("edition = \"{}\"\n", Edition::LATEST))
        );
        let main = std::fs::read_to_string(&created[1]).unwrap();
        assert!(main.starts_with("namespace HelloWorld\n\n"), "{main}");
        // The template's `main` is an entry point that the checker accepts.
        let module = crate::driver::Project::load(&root.join("hello-world"))
            .unwrap()
            .analyze()
            .unwrap();
        assert!(crate::llvm::main_entry(&module), "{main}");
        let again = create_project(&root.join("hello-world"), None, true).unwrap_err();
        assert!(again.message.contains("is not empty"), "{}", again.message);
        create_project(&root.join("other"), Some("Acme::Tools"), false).unwrap();
        assert!(
            std::fs::read_to_string(root.join("other/Tsuzuri.toml"))
                .unwrap()
                .contains("namespace = \"Acme::Tools\"")
        );
        assert!(
            std::fs::read_to_string(root.join("other/Main.tz"))
                .unwrap()
                .starts_with("namespace Acme::Tools\n\n")
        );
        for namespace in ["IO", "acme-tools", "A::::B", "Acme.Tools"] {
            assert!(
                create_project(&root.join("bad"), Some(namespace), false).is_err(),
                "{namespace}"
            );
        }
        assert!(!root.join("bad").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
