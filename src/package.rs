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
    /// The `[windows]` section (W09). Only the root package's section counts.
    pub windows: WindowsSettings,
    /// The `[macos]` section (M17). Only the root package's section counts.
    pub macos: MacosSettings,
    /// Where the `[package]` values are, for the diagnostics about them.
    pub spans: PackageSpans,
}

/// The spans of the quoted `[package]` values, quotes included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageSpans {
    pub name: Span,
    pub version: Span,
    /// The `namespace` value, or the `name` value when the namespace is derived from the name.
    pub namespace: Span,
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

/// The `[windows]` section of the root manifest (W09, W10).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowsSettings {
    pub subsystem: Option<crate::driver::Subsystem>,
    pub manifest: Option<PathBuf>,
    pub icon: Option<PathBuf>,
    pub version: Option<(String, [u16; 4])>,
    pub resources: Vec<PathBuf>,
}

impl WindowsSettings {
    pub fn has_resources(&self) -> bool {
        self.manifest.is_some()
            || self.icon.is_some()
            || self.version.is_some()
            || !self.resources.is_empty()
    }
}

/// The `[windows]` keys, in the order that the error for an unknown key lists them.
const WINDOWS_KEYS: &str = "subsystem, manifest, icon, version, or resources";

/// Parses a `[windows] version` (`MAJOR.MINOR.PATCH` or `MAJOR.MINOR.PATCH.BUILD`, each 0..=65535
/// without leading zeros). An omitted fourth part defaults to 0.
fn parse_windows_version(text: &str) -> Option<[u16; 4]> {
    let mut parts = [0u16; 4];
    let mut count = 0usize;
    for part in text.split('.') {
        if count == 4
            || part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        parts[count] = part.parse::<u16>().ok()?;
        count += 1;
    }
    matches!(count, 3 | 4).then_some(parts)
}

/// The `[macos]` section of the root manifest (M17; M15/M16 add bundle and signing keys).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MacosSettings {
    pub min_version: Option<(crate::target::MacosVersion, Span)>,
    /// `bundle_id` (M15): the `CFBundleIdentifier` of `--emit app`, which that output requires.
    pub bundle_id: Option<String>,
    /// `name`: `CFBundleName`.
    pub name: Option<String>,
    /// `version`: `CFBundleShortVersionString`.
    pub version: Option<String>,
    /// `build`: `CFBundleVersion`.
    pub build: Option<String>,
    /// `icon`: an `.icns` file, copied to `Contents/Resources` and named by `CFBundleIconFile`.
    pub icon: Option<PathBuf>,
    /// `resources`: files and directories copied to `Contents/Resources` under their own names.
    pub resources: Vec<PathBuf>,
    /// `embed`: Mach-O files and `.framework` directories copied to `Contents/Frameworks`.
    pub embed: Vec<PathBuf>,
    /// `info_plist`: a property list whose keys are added to the generated `Info.plist`.
    pub info_plist: Option<PathBuf>,
    /// `category`: `LSApplicationCategoryType`.
    pub category: Option<String>,
    /// `ui_element`: `LSUIElement`.
    pub ui_element: Option<bool>,
}

/// The `[macos]` keys, in the order that the error for an unknown key lists them.
const MACOS_KEYS: &str = "min_version, bundle_id, name, version, build, icon, resources, embed, info_plist, category, or ui_element";

/// A bundle identifier: letters, digits, `-` and `.`, without an empty component.
fn valid_bundle_id(text: &str) -> bool {
    text.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// A bundle version: one to three integers of 0 to 2147483647, separated by `.`.
fn valid_bundle_version(text: &str) -> bool {
    let mut parts = 0;
    text.split('.').all(|part| {
        parts += 1;
        !part.is_empty()
            && part.len() <= 10
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && part
                .parse::<u32>()
                .is_ok_and(|value| value <= i32::MAX as u32)
    }) && parts <= 3
}

/// An application category: `public.app-category.` and then lower-case letters and `-`.
fn valid_category(text: &str) -> bool {
    text.strip_prefix("public.app-category.")
        .is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        })
}

/// A `[macos]` path: relative, and with a last component, which names the copy in the bundle.
fn macos_path(path: String) -> Option<PathBuf> {
    relative_path(path).filter(|path| path.file_name().is_some())
}

/// The first path whose file name an earlier one of `paths` already has, with that name. Names that
/// differ only in ASCII case count as the same: the default macOS volume keeps one file for both.
fn repeated_file_name(paths: &[(PathBuf, Span)]) -> Option<(String, Span)> {
    let mut seen = BTreeSet::new();
    paths.iter().find_map(|(path, span)| {
        let name = path.file_name()?.to_string_lossy().into_owned();
        (!seen.insert(name.to_ascii_lowercase())).then_some((name, *span))
    })
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
    /// The entry, from its key through its closing `}`.
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
    use JsonStep::{Item, Key, Member};
    use serde_json::Value;
    if text.len() > MAX_PACKAGE_FILE_BYTES {
        return Err(Diagnostic::of(
            DiagnosticKind::LockfileTooLarge,
            "Tsuzuri.lock exceeds 1 MiB",
            Span::new(0, 0).in_source(source_id),
        ));
    }
    // Each error points at the key or value that it is about: the one at `path`, or the
    // innermost value around it when it is missing.
    let at = |path: &[JsonStep<'_>]| {
        (0..=path.len())
            .rev()
            .find_map(|length| json_span(text, &path[..length]))
            .unwrap_or_default()
    };
    let invalid = |detail: String, span: (usize, usize)| {
        Diagnostic::of(
            DiagnosticKind::InvalidLockfile,
            format!(
                "Tsuzuri.lock is not a valid lockfile ({detail}); restore it from version control or delete it and run tsuzuri fetch"
            ),
            Span::new(span.0, span.1).in_source(source_id),
        )
    };
    let value: Value = serde_json::from_str(text).map_err(|error| {
        let start = json_error_offset(text, &error);
        let end = text[start..]
            .chars()
            .next()
            .map_or(start, |character| start + character.len_utf8());
        invalid(error.to_string(), (start, end))
    })?;
    if let Some((key, span)) = duplicate_json_key(text) {
        return Err(invalid(format!("duplicate key '{key}'"), span));
    }
    let object = value
        .as_object()
        .ok_or_else(|| invalid("expected an object".into(), at(&[])))?;
    if let Some(key) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "format" | "packages"))
    {
        return Err(invalid(format!("unknown key '{key}'"), at(&[Key(key)])));
    }
    let format = object.get("format").and_then(Value::as_u64);
    if !matches!(format, Some(1 | 2)) {
        return Err(invalid(
            "format must be 1 or 2".into(),
            at(&[Member("format")]),
        ));
    }
    let packages = object
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            invalid(
                "packages must be an array".into(),
                at(&[Member("packages")]),
            )
        })?;
    let mut entries = BTreeMap::new();
    for (index, package) in packages.iter().enumerate() {
        let item = [Member("packages"), Item(index)];
        let fields = package
            .as_object()
            .ok_or_else(|| invalid("each package must be an object".into(), at(&item)))?;
        let member = |key: &str| at(&[item[0], item[1], Member(key)]);
        if let Some(key) = fields.keys().find(|key| {
            !matches!(key.as_str(), "name" | "git" | "rev" | "sha256")
                && (format == Some(1) || key.as_str() != "version")
        }) {
            return Err(invalid(
                format!("unknown package key '{key}'"),
                at(&[item[0], item[1], Key(key)]),
            ));
        }
        let field = |key: &str| {
            fields
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(format!("each package needs a string {key}"), member(key)))
        };
        let name = field("name")?;
        if namespace(name, Span::default()).is_err() {
            return Err(invalid(
                format!("invalid package name '{name}'"),
                member("name"),
            ));
        }
        let version = match fields.get("version") {
            None => None,
            Some(version) => Some(version.as_str().and_then(Version::parse).ok_or_else(|| {
                invalid(format!("invalid version of '{name}'"), member("version"))
            })?),
        };
        let entry = LockEntry {
            git: field("git")?.to_owned(),
            rev: field("rev")?.to_owned(),
            sha256: field("sha256")?.to_owned(),
            version,
        };
        if !valid_git_url(&entry.git) {
            return Err(invalid(
                format!("invalid git url of '{name}'"),
                member("git"),
            ));
        }
        if !valid_rev(&entry.rev) {
            return Err(invalid(format!("invalid rev of '{name}'"), member("rev")));
        }
        if !lowercase_hex(&entry.sha256, 64) {
            return Err(invalid(
                format!("invalid sha256 of '{name}'"),
                member("sha256"),
            ));
        }
        if entries.insert(name.to_owned(), entry).is_some() {
            return Err(invalid(
                format!("duplicate package '{name}'"),
                member("name"),
            ));
        }
    }
    Ok(entries)
}

/// The byte offset in `text` of the place that a `serde_json` syntax error names by line and
/// column (columns count bytes, and the place is the last byte read).
fn json_error_offset(text: &str, error: &serde_json::Error) -> usize {
    let line_start = text
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let mut offset = (line_start + error.column().saturating_sub(1)).min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
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
    if let Some((key, _)) = duplicate_json_key(text) {
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
/// that `serde_json` accepted (`serde_json` keeps only the last duplicate), and the byte range
/// of its second occurrence, quotes included.
fn duplicate_json_key(text: &str) -> Option<(String, (usize, usize))> {
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
                        return Some((key, (start, end + 1)));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// A step into a JSON value: the value of an object's member, the key of the member (the last
/// step only), or an array's item.
#[derive(Clone, Copy)]
enum JsonStep<'a> {
    Member(&'a str),
    Key(&'a str),
    Item(usize),
}

/// The byte range in `text`, JSON that `serde_json` accepted without duplicate keys, of the
/// value at `path` or of the key that a last `Key` step names. `None` when there is none.
fn json_span(text: &str, path: &[JsonStep<'_>]) -> Option<(usize, usize)> {
    let mut scan = JsonScan {
        bytes: text.as_bytes(),
        at: 0,
    };
    for (index, step) in path.iter().enumerate() {
        scan.space();
        match *step {
            JsonStep::Item(wanted) => {
                scan.eat(b'[')?;
                for _ in 0..wanted {
                    scan.value()?;
                    scan.space();
                    scan.eat(b',')?;
                }
            }
            JsonStep::Member(name) | JsonStep::Key(name) => {
                scan.eat(b'{')?;
                loop {
                    scan.space();
                    let start = scan.at;
                    scan.string()?;
                    let key = (start, scan.at);
                    scan.space();
                    scan.eat(b':')?;
                    if serde_json::from_str::<String>(&text[key.0..key.1]).ok()? == name {
                        if matches!(step, JsonStep::Key(_)) {
                            return (index + 1 == path.len()).then_some(key);
                        }
                        break;
                    }
                    scan.value()?;
                    scan.space();
                    scan.eat(b',')?;
                }
            }
        }
    }
    scan.value()
}

/// A cursor over valid JSON for [`json_span`].
struct JsonScan<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl JsonScan<'_> {
    fn space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Option<()> {
        (self.bytes.get(self.at) == Some(&byte)).then(|| self.at += 1)
    }

    /// Passes a string at the cursor.
    fn string(&mut self) -> Option<()> {
        self.eat(b'"')?;
        loop {
            match *self.bytes.get(self.at)? {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return Some(());
                }
                _ => self.at += 1,
            }
        }
    }

    /// Passes the value after the cursor and returns its range.
    fn value(&mut self) -> Option<(usize, usize)> {
        self.space();
        let start = self.at;
        match *self.bytes.get(self.at)? {
            b'"' => self.string()?,
            b'{' | b'[' => {
                let mut depth = 0usize;
                loop {
                    match *self.bytes.get(self.at)? {
                        b'"' => {
                            self.string()?;
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                self.at += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.at += 1;
                }
            }
            b'}' | b']' | b',' | b':' => return None,
            _ => {
                while self.bytes.get(self.at).is_some_and(|byte| {
                    !matches!(byte, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
                }) {
                    self.at += 1;
                }
            }
        }
        Some((start, self.at))
    }
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
            segment != "_" && crate::lexer::lex(segment, edition).is_ok_and(|tokens| {
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
    // Each diagnostic points at the body of the problem (a key, a value, or a section header),
    // or, when a token is missing, where it should be.
    let mut section = "";
    let mut sections = BTreeSet::new();
    let mut package: Option<Span> = None;
    let mut fields = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    let mut wasm = WasmSettings::default();
    let mut windows = WindowsSettings::default();
    let mut windows_keys = BTreeSet::new();
    let mut macos = MacosSettings::default();
    let mut native: Option<(crate::driver::LinkInputs, Span)> = None;
    let mut native_keys = BTreeSet::new();
    let mut macos_keys = BTreeSet::new();
    // The names that `icon` and `resources` take in `Contents/Resources`, and `embed` in `Contents/Frameworks`.
    let mut macos_resource_names: Vec<(PathBuf, Span)> = Vec::new();
    let mut macos_embed_names: Vec<(PathBuf, Span)> = Vec::new();
    let mut registry: Option<(Option<String>, Option<String>, Span)> = None;
    let mut capabilities: Option<(Option<CapabilitySet>, Span)> = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let mut cursor = Line::new(line, offset, source_id);
        offset += line.len();
        cursor.space();
        if cursor.rest.is_empty() || cursor.rest.starts_with('#') {
            continue;
        }
        if cursor.rest.starts_with('[') {
            let header = cursor.header();
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
            } else if cursor.take("[windows]") {
                Some("windows")
            } else if cursor.take("[macos]") {
                Some("macos")
            } else {
                cursor.take("[native]").then_some("native")
            };
            match next {
                Some(next) if sections.insert(next) => section = next,
                _ => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ManifestSectionOrder,
                        "expected [package] followed by optional [dependencies], [wasm], [native], [registry], [capabilities], [windows] and [macos], each once",
                        header,
                    ));
                }
            }
            match section {
                "package" => package = Some(header),
                "native" => native = Some((crate::driver::LinkInputs::default(), header)),
                "registry" => registry = Some((None, None, header)),
                "capabilities" => capabilities = Some((None, header)),
                _ => {}
            }
            cursor.finish()?;
            continue;
        }
        let (key, key_span) = cursor.key()?;
        cursor.expect("=")?;
        let at_key = |kind: DiagnosticKind, message: &str| Diagnostic::of(kind, message, key_span);
        match section {
            "package" => {
                if !matches!(key.as_str(), "name" | "version" | "namespace" | "edition") {
                    return Err(at_key(
                        DiagnosticKind::UnknownManifestKey,
                        "unknown package key; expected name, version, namespace, or edition",
                    ));
                }
                const FIELD: &str = "package fields must be nonempty and occur exactly once";
                let (value, value_span) = cursor.string()?;
                if value.is_empty() {
                    return Err(Diagnostic::of(
                        DiagnosticKind::InvalidPackageField,
                        FIELD,
                        value_span,
                    ));
                }
                if fields.insert(key, (value, value_span)).is_some() {
                    return Err(at_key(DiagnosticKind::InvalidPackageField, FIELD));
                }
            }
            "dependencies" => {
                namespace(&key, key_span)?;
                cursor.expect("{")?;
                let (form, form_span) = cursor.key()?;
                let (source, native, granted) = match form.as_str() {
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
                        return Err(Diagnostic::of(
                            DiagnosticKind::InvalidGitDependency,
                            GIT_FORM,
                            form_span,
                        ));
                    }
                    _ => {
                        return Err(Diagnostic::of(
                            DiagnosticKind::InvalidDependencyForm,
                            "expected a path, git, or version dependency",
                            form_span,
                        ));
                    }
                };
                if dependencies
                    .insert(
                        key,
                        Dependency {
                            source,
                            span: cursor.since(key_span.start),
                            native,
                            capabilities: granted,
                        },
                    )
                    .is_some()
                {
                    return Err(at_key(
                        DiagnosticKind::DuplicateDependency,
                        "duplicate package dependency",
                    ));
                }
                if dependencies.len() > 1024 {
                    return Err(at_key(
                        DiagnosticKind::TooManyDependencies,
                        "a package may have at most 1024 dependencies",
                    ));
                }
            }
            "wasm" => {
                let slot = match key.as_str() {
                    "max-memory" => &mut wasm.max_memory,
                    "stack-size" => &mut wasm.stack_size,
                    _ => {
                        return Err(at_key(
                            DiagnosticKind::UnknownManifestKey,
                            "unknown wasm key; expected max-memory or stack-size",
                        ));
                    }
                };
                let (text, value_span) = cursor.string()?;
                let size = parse_size(&text).ok_or_else(|| {
                    Diagnostic::of(DiagnosticKind::InvalidWasmSize, "WASM sizes must be a quoted byte count or a number followed by KiB, MiB, or GiB, such as \"64MiB\"", value_span)
                })?;
                if slot.replace(size).is_some() {
                    return Err(at_key(
                        DiagnosticKind::DuplicateManifestKey,
                        "wasm keys must occur at most once",
                    ));
                }
            }
            "registry" => {
                let (index, rev, _) = registry
                    .as_mut()
                    .expect("the [registry] header precedes its keys");
                let (value, value_span) = cursor.string()?;
                let (slot, valid, rule) = match key.as_str() {
                    "index" => (index, valid_git_url(&value), GIT_URL_RULE),
                    "rev" => (rev, valid_rev(&value), GIT_REV_RULE),
                    _ => {
                        return Err(at_key(
                            DiagnosticKind::UnknownManifestKey,
                            "unknown registry key; expected index or rev",
                        ));
                    }
                };
                if !valid {
                    return Err(Diagnostic::of(
                        DiagnosticKind::InvalidRegistrySetting,
                        rule,
                        value_span,
                    ));
                }
                if slot.replace(value).is_some() {
                    return Err(at_key(
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
                    return Err(at_key(
                        DiagnosticKind::UnknownManifestKey,
                        "unknown capabilities key; expected allow",
                    ));
                }
                let names = cursor.capability_names("allow")?;
                if allow.replace(names).is_some() {
                    return Err(at_key(
                        DiagnosticKind::DuplicateManifestKey,
                        "capabilities keys must occur at most once",
                    ));
                }
            }
            "native" => {
                const FORM: &str = "expected [native] keys link, libraries, search, frameworks, framework_search or rpath, each an array of strings on one line";
                if !matches!(
                    key.as_str(),
                    "link" | "libraries" | "search" | "frameworks" | "framework_search" | "rpath"
                ) || !native_keys.insert(key.clone())
                {
                    return Err(at_key(DiagnosticKind::InvalidNativeSection, FORM));
                }
                let values = cursor.string_array(FORM)?;
                let inputs = &mut native
                    .as_mut()
                    .expect("the [native] header precedes its keys")
                    .0;
                match key.as_str() {
                    "libraries" => {
                        if let Some((message, span)) = values.iter().find_map(|(name, span)| {
                            crate::driver::library_name_error(name).map(|message| (message, *span))
                        }) {
                            return Err(Diagnostic::of(
                                DiagnosticKind::InvalidNativeLibraryName,
                                message,
                                span,
                            ));
                        }
                        inputs.libraries = values.into_iter().map(|(name, _)| name).collect();
                    }
                    "frameworks" => {
                        if let Some((message, span)) = values.iter().find_map(|(name, span)| {
                            crate::driver::framework_name_error(name).map(|message| {
                                (
                                    message
                                        .replace("--framework AppKit", "frameworks = [\"AppKit\"]"),
                                    *span,
                                )
                            })
                        }) {
                            return Err(Diagnostic::of(
                                DiagnosticKind::InvalidNativeFrameworkName,
                                message,
                                span,
                            ));
                        }
                        inputs.frameworks = values.into_iter().map(|(name, _)| name).collect();
                    }
                    "rpath" => {
                        if let Some((message, span)) = values.iter().find_map(|(value, span)| {
                            crate::driver::rpath_error(value).map(|message| (message, *span))
                        }) {
                            return Err(Diagnostic::of(
                                DiagnosticKind::InvalidNativeRpath,
                                message,
                                span,
                            ));
                        }
                        inputs.rpaths = values.into_iter().map(|(value, _)| value).collect();
                    }
                    _ => {
                        let paths = values
                            .into_iter()
                            .map(|(value, span)| {
                                relative_path(value).ok_or_else(|| {
                                    Diagnostic::of(
                                        DiagnosticKind::InvalidNativePath,
                                        if key == "framework_search" {
                                            "link, search and framework_search paths must be nonempty relative paths"
                                        } else {
                                            "link and search paths must be nonempty relative paths"
                                        },
                                        span,
                                    )
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        match key.as_str() {
                            "link" => inputs.paths = paths,
                            "search" => inputs.search = paths,
                            "framework_search" => inputs.framework_search = paths,
                            _ => unreachable!(),
                        }
                    }
                }
            }
            "windows" => {
                if !matches!(
                    key.as_str(),
                    "subsystem" | "manifest" | "icon" | "version" | "resources"
                ) {
                    return Err(at_key(
                        DiagnosticKind::UnknownManifestKey,
                        &format!("unknown windows key; expected {WINDOWS_KEYS}"),
                    ));
                }
                if !windows_keys.insert(key.clone()) {
                    return Err(at_key(
                        DiagnosticKind::DuplicateManifestKey,
                        "windows keys must occur at most once",
                    ));
                }
                let invalid = |message: &str, span: Span| {
                    Diagnostic::of(DiagnosticKind::InvalidWindowsSetting, message, span)
                };
                const PATHS: &str = "windows paths must be nonempty relative paths";
                match key.as_str() {
                    "subsystem" => {
                        let (text, value_span) = cursor.string()?;
                        let value = match text.as_str() {
                            "console" => crate::driver::Subsystem::Console,
                            "windows" => crate::driver::Subsystem::Windows,
                            _ => {
                                return Err(invalid(
                                    "subsystem must be \"console\" or \"windows\"",
                                    value_span,
                                ));
                            }
                        };
                        windows.subsystem = Some(value);
                    }
                    "manifest" | "icon" => {
                        let (text, value_span) = cursor.string()?;
                        let path = relative_path(text).ok_or_else(|| invalid(PATHS, value_span))?;
                        if key == "manifest" {
                            windows.manifest = Some(path);
                        } else {
                            windows.icon = Some(path);
                        }
                    }
                    "version" => {
                        let (text, value_span) = cursor.string()?;
                        let Some(parts) = parse_windows_version(&text) else {
                            return Err(invalid(
                                "version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH.BUILD with each part from 0 to 65535",
                                value_span,
                            ));
                        };
                        windows.version = Some((text, parts));
                    }
                    "resources" => {
                        const FORM: &str = "resources must be an array of relative paths on one line, such as [\"dialogs.rc\"]";
                        let values =
                            cursor.string_array_of(DiagnosticKind::InvalidWindowsSetting, FORM)?;
                        let mut paths = Vec::with_capacity(values.len());
                        for (value, span) in values {
                            let path = relative_path(value).ok_or_else(|| invalid(PATHS, span))?;
                            let valid_ext = path.extension().is_some_and(|ext| {
                                ext.eq_ignore_ascii_case("rc") || ext.eq_ignore_ascii_case("res")
                            });
                            if !valid_ext {
                                return Err(invalid(
                                    "windows resources must be .rc or .res files",
                                    span,
                                ));
                            }
                            paths.push(path);
                        }
                        windows.resources = paths;
                    }
                    _ => unreachable!(),
                }
            }
            "macos" => {
                if !matches!(
                    key.as_str(),
                    "min_version"
                        | "bundle_id"
                        | "name"
                        | "version"
                        | "build"
                        | "icon"
                        | "resources"
                        | "embed"
                        | "info_plist"
                        | "category"
                        | "ui_element"
                ) {
                    return Err(at_key(
                        DiagnosticKind::UnknownManifestKey,
                        &format!("unknown macos key; expected {MACOS_KEYS}"),
                    ));
                }
                let invalid = |message: &str, span: Span| {
                    Diagnostic::of(DiagnosticKind::InvalidMacosSetting, message, span)
                };
                const PATHS: &str = "macos paths must be nonempty relative paths";
                match key.as_str() {
                    "ui_element" => {
                        cursor.space();
                        let at = cursor.here();
                        macos.ui_element = Some(if cursor.take("true") {
                            true
                        } else if cursor.take("false") {
                            false
                        } else {
                            return Err(invalid("ui_element must be true or false", at));
                        });
                    }
                    "resources" | "embed" => {
                        const FORM: &str = "resources and embed must be arrays of relative paths on one line, such as [\"assets/help\"]";
                        let values =
                            cursor.string_array_of(DiagnosticKind::InvalidMacosSetting, FORM)?;
                        let names = if key == "embed" {
                            &mut macos_embed_names
                        } else {
                            &mut macos_resource_names
                        };
                        let mut paths = Vec::new();
                        for (value, span) in values {
                            let path = macos_path(value).ok_or_else(|| invalid(PATHS, span))?;
                            names.push((path.clone(), span));
                            paths.push(path);
                        }
                        if key == "embed" {
                            macos.embed = paths;
                        } else {
                            macos.resources = paths;
                        }
                    }
                    _ => {
                        let (text, value_span) = cursor.string()?;
                        match key.as_str() {
                            "min_version" => {
                                let Some(value) = crate::target::MacosVersion::parse(&text) else {
                                    return Err(invalid(
                                        "min_version must be a macOS version 12.0 or later, such as \"13.0\"",
                                        value_span,
                                    ));
                                };
                                macos.min_version = Some((value, value_span));
                            }
                            "bundle_id" => {
                                if !valid_bundle_id(&text) {
                                    return Err(invalid(
                                        "bundle_id must be a reverse-DNS name of letters, digits, '-' and '.', such as \"dev.example.app\"",
                                        value_span,
                                    ));
                                }
                                macos.bundle_id = Some(text);
                            }
                            "name" => {
                                if text.is_empty()
                                    || text.contains('/')
                                    || text.chars().any(char::is_control)
                                {
                                    return Err(invalid(
                                        "name must be nonempty and must not contain '/' or control characters",
                                        value_span,
                                    ));
                                }
                                macos.name = Some(text);
                            }
                            "version" | "build" => {
                                if !valid_bundle_version(&text) {
                                    return Err(invalid(
                                        &format!(
                                            "{key} must be one to three integers separated by '.', such as \"1.2.3\""
                                        ),
                                        value_span,
                                    ));
                                }
                                if key == "version" {
                                    macos.version = Some(text);
                                } else {
                                    macos.build = Some(text);
                                }
                            }
                            "icon" => {
                                let path = macos_path(text).filter(|path| {
                                    path.extension().is_some_and(|extension| {
                                        extension.eq_ignore_ascii_case("icns")
                                    })
                                });
                                let Some(path) = path else {
                                    return Err(invalid(
                                        "icon must be a relative path to an .icns file",
                                        value_span,
                                    ));
                                };
                                macos_resource_names.push((path.clone(), value_span));
                                macos.icon = Some(path);
                            }
                            "info_plist" => {
                                macos.info_plist = Some(
                                    macos_path(text).ok_or_else(|| invalid(PATHS, value_span))?,
                                );
                            }
                            "category" => {
                                if !valid_category(&text) {
                                    return Err(invalid(
                                        "category must start with \"public.app-category.\", such as \"public.app-category.developer-tools\"",
                                        value_span,
                                    ));
                                }
                                macos.category = Some(text);
                            }
                            _ => unreachable!("the key was checked above"),
                        }
                    }
                }
                // After the value, as before: a repeated key with a malformed value reports the value.
                if !macos_keys.insert(key.clone()) {
                    return Err(at_key(
                        DiagnosticKind::DuplicateManifestKey,
                        "macos keys must occur at most once",
                    ));
                }
                if let Some((name, span)) = repeated_file_name(&macos_resource_names) {
                    return Err(invalid(
                        &format!("resources and icon must not repeat the file name '{name}'"),
                        span,
                    ));
                }
                if let Some((name, span)) = repeated_file_name(&macos_embed_names) {
                    return Err(invalid(
                        &format!("embed must not repeat the file name '{name}'"),
                        span,
                    ));
                }
            }
            _ => {
                return Err(at_key(
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
            package.unwrap_or(whole),
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
    let (name, name_span) = fields.remove("name").ok_or_else(missing)?;
    let (version, version_span) = fields.remove("version").ok_or_else(missing)?;
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
    let derived = namespace(&name, name_span)?;
    let (namespace, namespace_span) = match fields.remove("namespace") {
        Some((namespace, span)) if !valid_namespace(&namespace, edition) => {
            return Err(Diagnostic::of(
                DiagnosticKind::InvalidPackageNamespace,
                "package namespace must be identifiers joined by '::' such as \"Acme::Tools\", at most 16 segments and 255 bytes, that do not start with 'std' or a standard library module name",
                span,
            ));
        }
        // Namespaces are dotted inside the compiler, as module keys are.
        Some((namespace, span)) => (namespace.replace("::", "."), span),
        None => (derived, name_span),
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
        windows,
        macos,
        spans: PackageSpans {
            name: name_span,
            version: version_span,
            namespace: namespace_span,
        },
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

/// One line of a manifest, read from left to right.
struct Line<'a> {
    /// What is left of the line, without its line end.
    rest: &'a str,
    /// The offset in the manifest just past the line's text, before its line end.
    end: usize,
    /// The offset just past the line's last character that is not a space or a tab.
    content_end: usize,
    source: usize,
}

impl<'a> Line<'a> {
    /// `line` starts at `start` in the manifest and may end with its line end.
    fn new(line: &'a str, start: usize, source: usize) -> Self {
        let text = line.trim_end_matches(['\r', '\n']);
        Self {
            rest: text,
            end: start + text.len(),
            content_end: start + text.trim_end_matches([' ', '\t']).len(),
            source,
        }
    }

    /// The offset of the start of `rest`.
    fn offset(&self) -> usize {
        self.end - self.rest.len()
    }

    /// From `start` to the start of `rest`.
    fn since(&self, start: usize) -> Span {
        Span::new(start, self.offset()).in_source(self.source)
    }

    /// The next character after spaces and tabs, or the empty span just past the line's last
    /// character: what stands where a token is expected.
    fn here(&self) -> Span {
        let rest = self.rest.trim_start_matches([' ', '\t']);
        let start = (self.end - rest.len()).min(self.content_end);
        let width = rest.chars().next().map_or(0, char::len_utf8);
        Span::new(start, start + width).in_source(self.source)
    }

    /// The section header at the start of `rest`: through its first `]`, or to the end of the
    /// line without one.
    fn header(&self) -> Span {
        let start = self.offset();
        let end = self
            .rest
            .find(']')
            .map_or(self.content_end, |at| start + at + 1);
        Span::new(start, end).in_source(self.source)
    }

    fn error(&self, kind: DiagnosticKind, message: &str) -> Diagnostic {
        Diagnostic::of(kind, message, self.here())
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

    /// A bare key and its span.
    fn key(&mut self) -> Result<(String, Span), Diagnostic> {
        self.space();
        let end = self
            .rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            .count();
        if end == 0 {
            return Err(self.error(DiagnosticKind::ManifestSyntaxError, "expected a bare key"));
        }
        let start = self.offset();
        let key = self.rest[..end].to_owned();
        self.rest = &self.rest[end..];
        Ok((key, self.since(start)))
    }

    /// A quoted string and its span, quotes included.
    fn string(&mut self) -> Result<(String, Span), Diagnostic> {
        self.expect("\"")?;
        let start = self.offset() - 1;
        let mut value = String::new();
        while let Some(character) = self.rest.chars().next() {
            let at = self.offset();
            self.rest = &self.rest[character.len_utf8()..];
            match character {
                '"' => return Ok((value, self.since(start))),
                '\\' => {
                    let Some(escaped) = self.rest.chars().next() else {
                        return Err(Diagnostic::of(
                            DiagnosticKind::ManifestSyntaxError,
                            "unterminated string escape",
                            self.since(at),
                        ));
                    };
                    self.rest = &self.rest[escaped.len_utf8()..];
                    value.push(match escaped {
                        '"' | '\\' => escaped,
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            return Err(Diagnostic::of(
                                DiagnosticKind::ManifestSyntaxError,
                                "unsupported string escape; use \\\" \\\\ \\n \\r or \\t",
                                self.since(at),
                            ));
                        }
                    });
                }
                character if character.is_control() => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ManifestSyntaxError,
                        "unescaped control character in string",
                        self.since(at),
                    ));
                }
                character => value.push(character),
            }
        }
        Err(Diagnostic::of(
            DiagnosticKind::ManifestSyntaxError,
            "unterminated quoted string",
            self.since(start),
        ))
    }

    /// A one-line array of strings and their spans; `form` is the error for anything else.
    fn string_array(&mut self, form: &str) -> Result<Vec<(String, Span)>, Diagnostic> {
        self.string_array_of(DiagnosticKind::InvalidNativeSection, form)
    }

    /// [`Self::string_array`] for another section: `kind` is the kind of the error `form`.
    fn string_array_of(
        &mut self,
        kind: DiagnosticKind,
        form: &str,
    ) -> Result<Vec<(String, Span)>, Diagnostic> {
        if !self.take("[") {
            return Err(self.error(kind, form));
        }
        let mut values = Vec::new();
        while !self.take("]") {
            values.push(self.string()?);
            if !self.take(",") && !self.rest.trim_start().starts_with(']') {
                return Err(self.error(kind, form));
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
            let (name, span) = self.string()?;
            let Some(capability) = Capability::from_name(&name) else {
                return Err(Diagnostic::of(
                    DiagnosticKind::UnknownCapability,
                    format!(
                        "unknown capability '{name}'; expected one of {}",
                        Capability::NAMES
                    ),
                    span,
                ));
            };
            if names.contains(capability) {
                return Err(Diagnostic::of(
                    DiagnosticKind::DuplicateCapability,
                    format!("capability '{name}' is listed twice"),
                    span,
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
            Err(Diagnostic::of(
                DiagnosticKind::ManifestSyntaxError,
                "unexpected content after manifest field",
                Span::new(self.offset(), self.content_end).in_source(self.source),
            ))
        }
    }

    /// The rest of `{ path = "...", native = true, capabilities = [...] }` after `path`, through `}`.
    fn path_dependency(
        &mut self,
    ) -> Result<(DependencySource, bool, Option<CapabilitySet>), Diagnostic> {
        const KEYS: &str =
            "only the path, native, and capabilities keys are supported in a dependency";
        self.expect("=")?;
        let (path, path_span) = self.string()?;
        // Windows treats `/dir` and `C:dir` as non-absolute, but both escape the package root.
        let rooted = Path::new(&path).has_root()
            || matches!(
                Path::new(&path).components().next(),
                Some(Component::Prefix(_))
            );
        if path.is_empty() || path.contains('\0') || rooted {
            return Err(Diagnostic::of(
                DiagnosticKind::InvalidDependencyPath,
                "dependency path must be a nonempty relative path",
                path_span,
            ));
        }
        let mut native = false;
        let mut capabilities = None;
        if self.take(",") {
            let (key, key_span) = self.key()?;
            match key.as_str() {
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
                        let (key, key_span) = self.key()?;
                        match key.as_str() {
                            "capabilities" => capabilities = self.dependency_capabilities()?,
                            "native" => {
                                return Err(Diagnostic::of(
                                    DiagnosticKind::DuplicateManifestKey,
                                    "duplicate key 'native' in a dependency",
                                    key_span,
                                ));
                            }
                            _ => {
                                return Err(Diagnostic::of(
                                    DiagnosticKind::UnknownManifestKey,
                                    KEYS,
                                    key_span,
                                ));
                            }
                        }
                    }
                }
                "capabilities" => capabilities = self.dependency_capabilities()?,
                _ => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::UnknownManifestKey,
                        KEYS,
                        key_span,
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
        let (text, span) = self.string()?;
        let version = Version::parse(&text).ok_or_else(|| {
            Diagnostic::of(
                DiagnosticKind::InvalidRegistryDependency,
                VERSION_RULE,
                span,
            )
        })?;
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
        match self.key() {
            Ok((key, _)) if key == "capabilities" => self.dependency_capabilities(),
            Ok((_, span)) => Err(Diagnostic::of(kind, form, span)),
            Err(_) => Err(self.error(kind, form)),
        }
    }

    /// The rest of `{ git = "<url>", rev = "<commit>", capabilities = [...] }` after `git`, through `}`.
    fn git_dependency(&mut self) -> Result<(DependencySource, Option<CapabilitySet>), Diagnostic> {
        let kind = DiagnosticKind::InvalidGitDependency;
        if !self.take("=") {
            return Err(self.error(kind, GIT_FORM));
        }
        let (url, url_span) = self.string()?;
        if !valid_git_url(&url) {
            return Err(Diagnostic::of(kind, GIT_URL_RULE, url_span));
        }
        if !self.take(",") {
            return Err(self.error(kind, GIT_FORM));
        }
        match self.key() {
            Ok((key, _)) if key == "rev" => {}
            Ok((_, span)) => return Err(Diagnostic::of(kind, GIT_FORM, span)),
            Err(_) => return Err(self.error(kind, GIT_FORM)),
        }
        if !self.take("=") {
            return Err(self.error(kind, GIT_FORM));
        }
        let (rev, rev_span) = self.string()?;
        if !valid_rev(&rev) {
            return Err(Diagnostic::of(kind, GIT_REV_RULE, rev_span));
        }
        let capabilities = self.trailing_capabilities(kind, GIT_FORM)?;
        if !self.take("}") {
            return Err(self.error(kind, GIT_FORM));
        }
        Ok((DependencySource::Git { url, rev }, capabilities))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: &str = "[package]\nname = \"sample-app\"\nversion = \"0.1.0\"\n";

    /// `text` without the markers '«' and '»', and the byte range between them.
    fn marked(text: &str) -> (String, (usize, usize)) {
        let start = text.find('«').expect("the case marks its span");
        let end = text.find('»').expect("the case marks its span") - '«'.len_utf8();
        (text.replace(['«', '»'], ""), (start, end))
    }

    /// Each diagnostic of a manifest points at the body of the problem: the key, the value, or
    /// the section header, or where a missing token belongs; never at the start of the file.
    #[test]
    fn manifest_errors_point_at_their_cause() {
        let git = "https://example.org/a.git";
        let rev = "0123456789abcdef0123456789abcdef01234567";
        // Each case: the text after PACKAGE with the span between '«' and '»', the kind, and the
        // 1-based line and column of the span's start.
        let cases = [
            // Unknown sections and the order of sections.
            ("\n  «[other]» # x\n", "manifest_section_order", 5, 3),
            ("[wasm]\n«[wasm]»\n", "manifest_section_order", 5, 1),
            ("[native]\n«[package]»\n", "manifest_section_order", 5, 1),
            ("«[dependencies»\n", "manifest_section_order", 4, 1),
            // Unknown keys.
            ("  «foo» = 1\n", "unknown_manifest_key", 4, 3),
            (
                "[wasm]\n«memory» = \"64MiB\"\n",
                "unknown_manifest_key",
                5,
                1,
            ),
            ("[registry]\n«url» = \"x\"\n", "unknown_manifest_key", 5, 1),
            (
                "[capabilities]\n«deny» = []\n",
                "unknown_manifest_key",
                5,
                1,
            ),
            ("[windows]\n«foo» = \"bar\"\n", "unknown_manifest_key", 5, 1),
            (
                "[native]\nlink = [\"a\"]\n«bogus» = [\"x\"]\n",
                "invalid_native_section",
                6,
                1,
            ),
            (
                "[native]\nlink = []\n«link» = []\n",
                "invalid_native_section",
                6,
                1,
            ),
            // The shape and the rules of values.
            ("[native]\nlink = «\"»m\"\n", "invalid_native_section", 5, 8),
            (
                "[native]\nlink = [\"a\" «\"»b\"]\n",
                "invalid_native_section",
                5,
                13,
            ),
            (
                "[native]\nlink = [\"a\"«»\n",
                "invalid_native_section",
                5,
                12,
            ),
            (
                "[native]\nlibraries = [\"m\", «\"libm\"»]\n",
                "invalid_native_library_name",
                5,
                19,
            ),
            (
                "[native]\nsearch = [«\"/abs\"»]\n",
                "invalid_native_path",
                5,
                11,
            ),
            (
                "[windows]\nsubsystem = «\"bogus\"»\n",
                "invalid_windows_setting",
                5,
                13,
            ),
            (
                "[wasm]\nmax-memory = «\"64MB\"»\n",
                "invalid_wasm_size",
                5,
                14,
            ),
            (
                "[wasm]\nmax-memory = «6»7108864\n",
                "manifest_syntax_error",
                5,
                14,
            ),
            ("edition = «\"2030\"»\n", "unsupported_edition", 4, 11),
            (
                "namespace = «\"acme-tools\"»\n",
                "invalid_package_namespace",
                4,
                13,
            ),
            (
                "[registry]\nindex = «\"http://e.org/i.git\"»\n",
                "invalid_registry_setting",
                5,
                9,
            ),
            (
                "[capabilities]\nallow = «\"»fs\"\n",
                "invalid_capabilities",
                5,
                9,
            ),
            (
                "[capabilities]\nallow = [«1»]\n",
                "invalid_capabilities",
                5,
                10,
            ),
            (
                "[capabilities]\nallow = [\"fs\", «\"files\"»]\n",
                "unknown_capability",
                5,
                16,
            ),
            (
                "[capabilities]\nallow = [\"fs\", «\"fs\"»]\n",
                "duplicate_capability",
                5,
                16,
            ),
            // Keys that occur twice.
            ("«version» = \"2\"\n", "invalid_package_field", 4, 1),
            (
                "[wasm]\nstack-size = \"1MiB\"\n«stack-size» = \"2MiB\"\n",
                "duplicate_manifest_key",
                6,
                1,
            ),
            (
                "[windows]\nsubsystem = \"console\"\n «subsystem» = \"windows\"\n",
                "duplicate_manifest_key",
                6,
                2,
            ),
            (
                "[capabilities]\nallow = []\n«allow» = []\n",
                "duplicate_manifest_key",
                6,
                1,
            ),
            // The syntax of a line.
            (
                "edition = \"2026\" «x # y»   \n",
                "manifest_syntax_error",
                4,
                18,
            ),
            (
                "edition = \"2026\" «ここ»\n",
                "manifest_syntax_error",
                4,
                18,
            ),
            ("edition «\"»2026\"\n", "manifest_syntax_error", 4, 9),
            ("«=» \"2026\"\n", "manifest_syntax_error", 4, 1),
            ("edition = \"20«\\q»26\"\n", "manifest_syntax_error", 4, 14),
            ("edition = «\"2026»\n", "manifest_syntax_error", 4, 11),
            ("edition = \"20«\\»\n", "manifest_syntax_error", 4, 14),
            // Dependencies.
            (
                "[dependencies]\n«Bad_Name» = { path = \"a\" }\n",
                "invalid_package_name",
                5,
                1,
            ),
            (
                "[dependencies]\na = { path = «\"/abs\"» }\n",
                "invalid_dependency_path",
                5,
                14,
            ),
            (
                "[dependencies]\na = { path = \"a\" }\n«a» = { path = \"b\" }\n",
                "duplicate_dependency",
                6,
                1,
            ),
            (
                "[dependencies]\na = { «url» = \"x\" }\n",
                "invalid_dependency_form",
                5,
                7,
            ),
            (
                "[dependencies]\na = «\"»a\"\n",
                "manifest_syntax_error",
                5,
                5,
            ),
            (
                "[dependencies]\na = { «tag» = \"v1\" }\n",
                "invalid_git_dependency",
                5,
                7,
            ),
            (
                "[dependencies]\na = { path = \"a\", native = «y»es }\n",
                "invalid_dependency_native_flag",
                5,
                28,
            ),
            (
                "[dependencies]\na = { path = \"a\", «link» = true }\n",
                "unknown_manifest_key",
                5,
                19,
            ),
            (
                "[dependencies]\na = { path = \"a\", native = true, «native» = true }\n",
                "duplicate_manifest_key",
                5,
                34,
            ),
            (
                "[dependencies]\na = { path = \"a\"«»  \n",
                "manifest_syntax_error",
                5,
                17,
            ),
            (
                "[dependencies]\na = { version = «\"1.2\"» }\n",
                "invalid_registry_dependency",
                5,
                17,
            ),
            (
                "[dependencies]\na = { version = \"1.2.3\", «native» = true }\n",
                "invalid_registry_dependency",
                5,
                26,
            ),
        ];
        let cases = cases
            .into_iter()
            .map(|(text, kind, line, column)| (text.to_owned(), kind, line, column));
        let git_cases = [
            (
                format!("[dependencies]\na = {{ git = \"{git}\" «}}»\n"),
                5,
                41,
            ),
            (
                format!(
                    "[dependencies]\na = {{ git = «\"http://e.org/a.git\"», rev = \"{rev}\" }}\n"
                ),
                5,
                13,
            ),
            (
                format!("[dependencies]\na = {{ git = \"{git}\", rev = «\"main\"» }}\n"),
                5,
                48,
            ),
            (
                format!("[dependencies]\na = {{ git = \"{git}\", «branch» = \"main\" }}\n"),
                5,
                42,
            ),
        ];
        let git_cases = git_cases
            .into_iter()
            .map(|(text, line, column)| (text, "invalid_git_dependency", line, column));
        for (text, kind, line, column) in cases.chain(git_cases) {
            let (source, span) = marked(&format!("{PACKAGE}{text}"));
            let error = parse_manifest(&source, 5).unwrap_err();
            assert_eq!(error.kind, Some(kind), "{text}: {}", error.message);
            assert_eq!(error.span.source, Some(5), "{text}");
            assert_eq!((error.span.start, error.span.end), span, "{text}");
            assert_eq!(
                crate::diagnostic::location(&source, error.span.start),
                (line, column),
                "{text}"
            );
        }
        // A manifest without name or version: the [package] header, or the whole file without one.
        for (text, span) in [
            ("# about\n«[package]» # the package\nname = \"a\"\n", None),
            ("# only a comment\n", Some((0, 17))),
            ("", Some((0, 0))),
        ] {
            let (source, span) = span.map_or_else(|| marked(text), |span| (text.to_owned(), span));
            let error = parse_manifest(&source, 0).unwrap_err();
            assert_eq!(error.kind, Some("missing_package_field"), "{text}");
            assert_eq!((error.span.start, error.span.end), span, "{text}");
        }
        for (text, kind) in [
            ("«[registry]»\n", "missing_registry_index"),
            ("«[capabilities]»\n", "missing_capabilities_allow"),
        ] {
            let (source, span) = marked(&format!("{PACKAGE}{text}"));
            let error = parse_manifest(&source, 0).unwrap_err();
            assert_eq!(error.kind, Some(kind), "{text}");
            assert_eq!((error.span.start, error.span.end), span, "{text}");
        }
        // CRLF line ends do not move the columns.
        let crlf = "[package]\r\nname = \"a\"\r\nversion = \"1\"\r\n\t«foo» = 1\r\n";
        let (source, span) = marked(crlf);
        let error = parse_manifest(&source, 0).unwrap_err();
        assert_eq!((error.span.start, error.span.end), span);
        assert_eq!(
            crate::diagnostic::location(&source, error.span.start),
            (4, 2)
        );
    }

    /// The spans that later diagnostics use: each dependency's entry and the `[package]` values.
    #[test]
    fn manifest_keeps_the_spans_of_entries_and_package_values() {
        let source = "[package]\nname = \"sample-app\"\nversion = \"0.1.0\"\nnamespace = \"Acme::Tools\"\n[dependencies]\n  geo = { path = \"../geo\" } # local\n";
        let manifest = parse_manifest(source, 2).unwrap();
        let text = |span: Span| {
            assert_eq!(span.source, Some(2));
            &source[span.start..span.end]
        };
        assert_eq!(
            text(manifest.dependencies["geo"].span),
            "geo = { path = \"../geo\" }"
        );
        assert_eq!(text(manifest.spans.name), "\"sample-app\"");
        assert_eq!(text(manifest.spans.version), "\"0.1.0\"");
        assert_eq!(text(manifest.spans.namespace), "\"Acme::Tools\"");
        // A namespace derived from the name is at the name.
        let derived = parse_manifest(PACKAGE, 0).unwrap();
        assert_eq!(derived.spans.namespace, derived.spans.name);
    }

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
    fn native_frameworks_and_rpaths_parse_and_validate() {
        let source = format!(
            "{PACKAGE}[native]\nframeworks = [\"AppKit\", \"Metal\"]\nframework_search = [\"vendor/Frameworks\"]\nrpath = [\"@executable_path/../Frameworks\"]\n"
        );
        let (inputs, _) = parse_manifest(&source, 0).unwrap().native.unwrap();
        assert_eq!(inputs.frameworks, ["AppKit", "Metal"]);
        assert_eq!(
            inputs.framework_search,
            [PathBuf::from("vendor/Frameworks")]
        );
        assert_eq!(inputs.rpaths, ["@executable_path/../Frameworks"]);

        let bad_fw = parse_manifest(
            &format!("{PACKAGE}[native]\nframeworks = [\"AppKit.framework\"]\n"),
            0,
        )
        .unwrap_err();
        assert_eq!(
            bad_fw.kind,
            Some(DiagnosticKind::InvalidNativeFrameworkName.id())
        );

        let bad_rpath =
            parse_manifest(&format!("{PACKAGE}[native]\nrpath = [\"./rel\"]\n"), 0).unwrap_err();
        assert_eq!(
            bad_rpath.kind,
            Some(DiagnosticKind::InvalidNativeRpath.id())
        );
    }

    #[test]
    fn rejects_malformed_native_link_section() {
        let form = "expected [native] keys link, libraries, search, frameworks, framework_search or rpath, each an array of strings on one line";
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
                "expected [package] followed by optional [dependencies], [wasm], [native], [registry], [capabilities], [windows] and [macos], each once",
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
                "expected [package] followed by optional [dependencies], [wasm], [native], [registry], [capabilities], [windows] and [macos], each once",
            ),
            (
                "[windows]\nfoo = \"bar\"\n",
                "unknown_manifest_key",
                "unknown windows key; expected subsystem, manifest, icon, version, or resources",
            ),
            (
                "[windows]\nmanifest = \"\"\n",
                "invalid_windows_setting",
                "windows paths must be nonempty relative paths",
            ),
            (
                "[windows]\nicon = \"/abs/app.ico\"\n",
                "invalid_windows_setting",
                "windows paths must be nonempty relative paths",
            ),
            (
                "[windows]\nversion = \"01.2.3\"\n",
                "invalid_windows_setting",
                "version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH.BUILD with each part from 0 to 65535",
            ),
            (
                "[windows]\nversion = \"1.2.65536\"\n",
                "invalid_windows_setting",
                "version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH.BUILD with each part from 0 to 65535",
            ),
            (
                "[windows]\nversion = \"1.2\"\n",
                "invalid_windows_setting",
                "version must be MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH.BUILD with each part from 0 to 65535",
            ),
            (
                "[windows]\nresources = [\"notes.txt\"]\n",
                "invalid_windows_setting",
                "windows resources must be .rc or .res files",
            ),
            (
                "[windows]\nresources = [\"\"]\n",
                "invalid_windows_setting",
                "windows paths must be nonempty relative paths",
            ),
            (
                "[windows]\nversion = \"1.0.0\"\nversion = \"1.0.1\"\n",
                "duplicate_manifest_key",
                "windows keys must occur at most once",
            ),
            (
                "[windows]\nsubsystem = \"efi\"\n",
                "invalid_windows_setting",
                "subsystem must be \"console\" or \"windows\"",
            ),
            (
                "[windows]\nsubsystem = \"windows\"\nsubsystem = \"console\"\n",
                "duplicate_manifest_key",
                "windows keys must occur at most once",
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
        // Each case: the keys after PACKAGE with the error's span between '«' and '»', its message.
        for (keys, message) in [
            (
                "edition = «\"2030\"»\n",
                "unsupported edition '2030'; supported editions: 2026, 2027, 2028",
            ),
            (
                "edition = «\" 2026\"»\n",
                "unsupported edition ' 2026'; supported editions: 2026, 2027, 2028",
            ),
            ("edition = «2»026\n", "expected '\"'"),
            (
                "edition = «\"\"»\n",
                "package fields must be nonempty and occur exactly once",
            ),
            (
                "edition = \"2026\"\n«edition» = \"2026\"\n",
                "package fields must be nonempty and occur exactly once",
            ),
            (
                "«license» = \"MIT\"\n",
                "unknown package key; expected name, version, namespace, or edition",
            ),
        ] {
            let (source, span) = marked(&format!("{PACKAGE}{keys}"));
            let error = parse_manifest(&source, 7).unwrap_err();
            assert_eq!(error.code, "E0002", "{keys}");
            assert_eq!(error.message, message, "{keys}");
            assert_eq!(error.span.source, Some(7), "{keys}");
            assert_eq!((error.span.start, error.span.end), span, "{keys}");
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
