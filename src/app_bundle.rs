//! macOS application bundles (M15): `tsuzuri build --emit app -o Name.app`.
//!
//! The bundle is the native executable of the same options as `--emit exe`, put at
//! `Name.app/Contents/MacOS/Name` next to an `Info.plist` that this module writes, `PkgInfo`, the
//! `icon` and `resources` of `[macos]` in `Contents/Resources` and its `embed` in `Contents/Frameworks`.
//! The whole is assembled in a temporary directory beside the output, signed ad hoc from the inside
//! out (the linker's signature names the executable `artifact` and does not bind `Info.plist`), and
//! then swapped in, so a build that fails leaves the previous bundle as it was.
//!
//! Nothing but the executable is cached: the bundle is assembled and signed on every build.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::check::{CheckedModule, ModuleOrigin};
use crate::diagnostic::Diagnostic;
use crate::diagnostic_kinds::DiagnosticKind;
use crate::driver::{
    self, BuildOptions, LinkInputs, Profile, Project, TemporaryDirectory, driver_error_of, io_error,
};
use crate::package::MacosSettings;
use crate::plist::{self, Dictionary, Key, Value};

/// The deepest directory nesting of a `resources` or `embed` entry that is copied.
const MAX_TREE_DEPTH: usize = 64;

/// How a bundle is signed. M15 signs ad hoc (the identity `-`); M16 adds the Developer ID identity,
/// the entitlements, the hardened runtime and the timestamp here, and [`sign`] passes them on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignPlan {
    /// The argument of `codesign --sign`.
    pub identity: String,
}

impl SignPlan {
    pub fn ad_hoc() -> Self {
        Self {
            identity: "-".to_owned(),
        }
    }
}

/// The keys of `Info.plist` that the compiler writes, with the `[macos]` key that each comes from;
/// `None` is a value that has no key (the executable's name, the constants, the linked minimum version).
const GENERATED_KEYS: [(&str, Option<&str>); 12] = [
    ("CFBundleExecutable", None),
    ("CFBundleIconFile", Some("icon")),
    ("CFBundleIdentifier", Some("bundle_id")),
    ("CFBundleInfoDictionaryVersion", None),
    ("CFBundleName", Some("name")),
    ("CFBundlePackageType", None),
    ("CFBundleShortVersionString", Some("version")),
    ("CFBundleVersion", Some("build")),
    ("LSApplicationCategoryType", Some("category")),
    ("LSMinimumSystemVersion", None),
    ("LSUIElement", Some("ui_element")),
    ("NSHighResolutionCapable", None),
];

fn missing_bundle_id() -> Diagnostic {
    driver_error_of(
        DiagnosticKind::MacosBundleIdMissing,
        "'--emit app' needs [macos] bundle_id in the root Tsuzuri.toml",
    )
}

/// The dictionary of `Contents/Info.plist`: the generated keys, and the keys of the `info_plist` file
/// `extra` (its name for messages, and its top-level dictionary) as they are. A key that both would give
/// is an error whether or not `[macos]` sets it, so one place decides each value.
pub fn info_plist(
    executable: &str,
    settings: &MacosSettings,
    minimum_system: &str,
    extra: Option<(&str, &Dictionary)>,
) -> Result<Dictionary, Diagnostic> {
    let bundle_id = settings
        .bundle_id
        .as_deref()
        .ok_or_else(missing_bundle_id)?;
    let mut entries = Dictionary::new();
    let mut set = |key: &str, value: Value| {
        entries.insert(Key::new(key), value);
    };
    set("CFBundleExecutable", Value::string(executable));
    set("CFBundleIdentifier", Value::string(bundle_id));
    set("CFBundleInfoDictionaryVersion", Value::string("6.0"));
    set("CFBundlePackageType", Value::string("APPL"));
    set("LSMinimumSystemVersion", Value::string(minimum_system));
    set("NSHighResolutionCapable", Value::Bool(true));
    if let Some(name) = &settings.name {
        set("CFBundleName", Value::string(name.as_str()));
    }
    if let Some(version) = &settings.version {
        set(
            "CFBundleShortVersionString",
            Value::string(version.as_str()),
        );
    }
    if let Some(build) = &settings.build {
        set("CFBundleVersion", Value::string(build.as_str()));
    }
    if let Some(name) = settings.icon.as_deref().and_then(Path::file_name) {
        set("CFBundleIconFile", Value::string(name.to_string_lossy()));
    }
    if let Some(category) = &settings.category {
        set(
            "LSApplicationCategoryType",
            Value::string(category.as_str()),
        );
    }
    if let Some(ui_element) = settings.ui_element {
        set("LSUIElement", Value::Bool(ui_element));
    }
    let Some((file, extra)) = extra else {
        return Ok(entries);
    };
    for (key, value) in extra {
        if let Some((_, source)) = GENERATED_KEYS
            .iter()
            .find(|(generated, _)| *generated == key.as_str())
        {
            let key = key.as_str();
            let message = match source {
                Some(setting) if entries.contains_key(&Key::new(key)) => format!(
                    "Info.plist key '{key}' comes from both [macos] {setting} and '{file}'; remove one"
                ),
                Some(setting) => format!(
                    "Info.plist key '{key}' belongs to [macos] {setting}; set it there and remove it from '{file}'"
                ),
                None => format!(
                    "Info.plist key '{key}' is written by '--emit app'; remove it from '{file}'"
                ),
            };
            return Err(driver_error_of(
                DiagnosticKind::MacosBundleConflict,
                message,
            ));
        }
        entries.insert(key.clone(), value.clone());
    }
    Ok(entries)
}

/// The identifier of the dSYM of a bundle, as `dsymutil` names it.
fn dsym_identifier(bundle_id: &str) -> String {
    format!("com.apple.xcode.dsym.{bundle_id}")
}

/// The dictionary of `Name.app.dSYM/Contents/Info.plist`: the seven keys that `dsymutil` writes for an
/// executable inside a bundle, with the bundle's versions (`1.0` and `1` without them).
pub fn dsym_info_plist(settings: &MacosSettings) -> Result<Dictionary, Diagnostic> {
    let bundle_id = settings
        .bundle_id
        .as_deref()
        .ok_or_else(missing_bundle_id)?;
    let text = |value: &str| Value::string(value);
    Ok(Dictionary::from([
        (Key::new("CFBundleDevelopmentRegion"), text("English")),
        (
            Key::new("CFBundleIdentifier"),
            Value::string(dsym_identifier(bundle_id)),
        ),
        (Key::new("CFBundleInfoDictionaryVersion"), text("6.0")),
        (Key::new("CFBundlePackageType"), text("dSYM")),
        (Key::new("CFBundleSignature"), text("????")),
        (
            Key::new("CFBundleShortVersionString"),
            text(settings.version.as_deref().unwrap_or("1.0")),
        ),
        (
            Key::new("CFBundleVersion"),
            text(settings.build.as_deref().unwrap_or("1")),
        ),
    ]))
}

/// The minimum macOS version of a thin 64-bit little-endian Mach-O file: the `minos` of its
/// `LC_BUILD_VERSION` (or the version of an older linker's `LC_VERSION_MIN_MACOSX`), which holds
/// `major.minor.patch` as `xxxx.yy.zz`. A fat file, a 32-bit file and anything else is `None`.
pub fn mach_o_min_version(bytes: &[u8]) -> Option<(u32, u32, u32)> {
    const MH_MAGIC_64: u32 = 0xfeed_facf;
    const LC_VERSION_MIN_MACOSX: u32 = 0x24;
    const LC_BUILD_VERSION: u32 = 0x32;
    let word = |offset: usize| -> Option<u32> {
        let slice = bytes.get(offset..offset.checked_add(4)?)?;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    };
    if word(0)? != MH_MAGIC_64 {
        return None;
    }
    let commands = word(16)?;
    let mut offset = 32usize;
    for _ in 0..commands {
        let command = word(offset)?;
        let size = word(offset + 4)? as usize;
        // A command that the file does not hold whole is not read.
        if size < 8 || bytes.len() < offset.checked_add(size)? {
            return None;
        }
        let version = match command {
            // cmd, cmdsize, platform, minos, sdk, ntools
            LC_BUILD_VERSION => Some(word(offset + 12)?),
            // cmd, cmdsize, version, sdk
            LC_VERSION_MIN_MACOSX => Some(word(offset + 8)?),
            _ => None,
        };
        if let Some(version) = version {
            return Some((version >> 16, (version >> 8) & 0xff, version & 0xff));
        }
        offset = offset.checked_add(size)?;
    }
    None
}

/// `LSMinimumSystemVersion` of a version: `12.0`, `13.1.2`, `27.2`.
pub fn minimum_system_version((major, minor, patch): (u32, u32, u32)) -> String {
    if patch == 0 {
        format!("{major}.{minor}")
    } else {
        format!("{major}.{minor}.{patch}")
    }
}

/// Whether `bytes` are an icns file: the magic `icns` and then the length of the file, big endian.
pub fn valid_icns(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && &bytes[..4] == b"icns"
        && u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize == bytes.len()
}

/// Whether a file that starts with `prefix` is Mach-O code: thin 64-bit, or a fat file.
pub fn mach_o_magic(prefix: &[u8]) -> bool {
    matches!(
        prefix.get(..4),
        Some([0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe] | [0xca, 0xfe, 0xba, 0xbf])
    )
}

fn invalid_input(message: String) -> Diagnostic {
    driver_error_of(DiagnosticKind::MacosBundleInputInvalid, message)
}

/// A `[macos]` path as the manifest wrote it, for messages.
fn shown(relative: &Path) -> String {
    relative.display().to_string()
}

fn unreadable(relative: &Path, error: io::Error) -> Diagnostic {
    driver_error_of(
        DiagnosticKind::IoFailure,
        format!(
            "cannot read macOS bundle input '{}': {error}",
            shown(relative)
        ),
    )
}

/// One `resources` or `embed` entry, or the icon: where it is and its name in the bundle.
struct Entry {
    source: PathBuf,
    relative: PathBuf,
    name: OsString,
}

/// What the manifest names, read and checked before the executable is built.
struct Inputs {
    /// `icon` and `resources`, for `Contents/Resources`.
    resources: Vec<Entry>,
    /// `embed`, for `Contents/Frameworks`.
    embed: Vec<Entry>,
    /// The name of the `info_plist` file and its top-level dictionary.
    extra: Option<(String, Dictionary)>,
}

impl Inputs {
    fn read(root: &Path, settings: &MacosSettings) -> Result<Self, Diagnostic> {
        let entry = |relative: &Path| -> Entry {
            Entry {
                source: root.join(relative),
                relative: relative.to_owned(),
                name: relative
                    .file_name()
                    .expect("the manifest checked the file name")
                    .to_owned(),
            }
        };
        let mut resources = Vec::new();
        if let Some(icon) = &settings.icon {
            let entry = entry(icon);
            let bytes = fs::read(&entry.source).map_err(|error| unreadable(icon, error))?;
            if !valid_icns(&bytes) {
                return Err(invalid_input(format!(
                    "'{}' is not an icns file: expected the 'icns' magic and its length",
                    shown(icon)
                )));
            }
            resources.push(entry);
        }
        for relative in &settings.resources {
            let entry = entry(relative);
            let metadata =
                fs::metadata(&entry.source).map_err(|error| unreadable(relative, error))?;
            if !metadata.is_file() && !metadata.is_dir() {
                return Err(invalid_input(format!(
                    "'{}' in resources is neither a file nor a directory",
                    shown(relative)
                )));
            }
            resources.push(entry);
        }
        let mut embed = Vec::new();
        for relative in &settings.embed {
            let entry = entry(relative);
            let metadata =
                fs::metadata(&entry.source).map_err(|error| unreadable(relative, error))?;
            let code = if metadata.is_dir() {
                relative.extension() == Some(OsStr::new("framework"))
            } else if metadata.is_file() {
                let mut prefix = Vec::new();
                fs::File::open(&entry.source)
                    .and_then(|file| file.take(4).read_to_end(&mut prefix))
                    .map_err(|error| unreadable(relative, error))?;
                mach_o_magic(&prefix)
            } else {
                false
            };
            if !code {
                return Err(invalid_input(format!(
                    "'{}' in embed is neither a Mach-O file nor a .framework directory; put it in resources",
                    shown(relative)
                )));
            }
            embed.push(entry);
        }
        let extra = match &settings.info_plist {
            Some(relative) => {
                let mut bytes = Vec::new();
                fs::File::open(root.join(relative))
                    .and_then(|file| {
                        file.take(plist::MAX_BYTES as u64 + 1)
                            .read_to_end(&mut bytes)
                    })
                    .map_err(|error| unreadable(relative, error))?;
                let entries = plist::read_dictionary(&bytes).map_err(|reason| {
                    invalid_input(format!(
                        "'{}' is not a property list that info_plist can add: {reason}",
                        shown(relative)
                    ))
                })?;
                Some((shown(relative), entries))
            }
            None => None,
        };
        Ok(Self {
            resources,
            embed,
            extra,
        })
    }

    fn sources(&self) -> impl Iterator<Item = &Entry> {
        self.resources.iter().chain(&self.embed)
    }
}

/// Copies the bytes of a file and whether it is executable (`0o755` or `0o644`). Extended attributes,
/// ACLs and times are not copied: `com.apple.FinderInfo`, which `std::fs::copy` carries over, makes
/// `codesign` refuse the bundle.
fn copy_file(source: &Path, destination: &Path) -> io::Result<()> {
    let mut input = fs::File::open(source)?;
    let executable = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            input.metadata()?.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            false
        }
    };
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        output.set_permissions(fs::Permissions::from_mode(if executable {
            0o755
        } else {
            0o644
        }))?;
    }
    let _ = executable;
    Ok(())
}

/// Whether the symbolic link at `inside` (the directories from the root of the copied tree to the link)
/// with the target `target` stays in the tree: a relative path that never climbs above the root.
fn link_stays_inside(inside: &Path, target: &Path) -> bool {
    let mut depth = inside.components().count();
    for component in target.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// Copies the file or directory `entry` to `destination`. The top-level path is followed when it is a
/// symbolic link; inside a directory, a link is copied as the same link when it is relative and stays in
/// the tree, and is an error otherwise, as is anything that is not a file, a directory or a link.
fn copy_entry(entry: &Entry, destination: &Path) -> Result<(), Diagnostic> {
    let metadata =
        fs::metadata(&entry.source).map_err(|error| unreadable(&entry.relative, error))?;
    if metadata.is_dir() {
        copy_tree(entry, &entry.source, destination, Path::new(""), 0)
    } else {
        copy_file(&entry.source, destination).map_err(|error| unreadable(&entry.relative, error))
    }
}

fn copy_tree(
    entry: &Entry,
    source: &Path,
    destination: &Path,
    inside: &Path,
    depth: usize,
) -> Result<(), Diagnostic> {
    let failed = |error: io::Error| unreadable(&entry.relative.join(inside), error);
    if depth > MAX_TREE_DEPTH {
        return Err(invalid_input(format!(
            "'{}' nests directories deeper than {MAX_TREE_DEPTH} levels",
            shown(&entry.relative)
        )));
    }
    fs::create_dir(destination).map_err(failed)?;
    // The order of a directory listing is the file system's; the copy is the same for any order, and the
    // sort makes the first error the same one on every machine.
    let mut names = Vec::new();
    for item in fs::read_dir(source).map_err(failed)? {
        names.push(item.map_err(failed)?.file_name());
    }
    names.sort();
    for name in names {
        let from = source.join(&name);
        let to = destination.join(&name);
        let within = inside.join(&name);
        let failed = |error: io::Error| unreadable(&entry.relative.join(&within), error);
        let kind = fs::symlink_metadata(&from).map_err(failed)?.file_type();
        if kind.is_symlink() {
            let target = fs::read_link(&from).map_err(failed)?;
            if !link_stays_inside(inside, &target) {
                return Err(invalid_input(format!(
                    "the symbolic link '{}' points outside '{}'; a bundle keeps only relative links that stay inside the copied directory",
                    shown(&entry.relative.join(&within)),
                    shown(&entry.relative)
                )));
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &to).map_err(failed)?;
            #[cfg(not(unix))]
            return Err(failed(io::Error::other(
                "symbolic links are copied only on macOS",
            )));
        } else if kind.is_dir() {
            copy_tree(entry, &from, &to, &within, depth + 1)?;
        } else if kind.is_file() {
            copy_file(&from, &to).map_err(failed)?;
        } else {
            return Err(invalid_input(format!(
                "'{}' is not a file, a directory or a symbolic link",
                shown(&entry.relative.join(&within))
            )));
        }
    }
    Ok(())
}

const CODESIGN_HINT: &str = "macOS application bundles are signed with codesign from /usr/bin or the Xcode Command Line Tools";

/// Signs the bundle from the inside out: each of `nested` (the Mach-O files and frameworks in
/// `Contents/Frameworks`), then `bundle`. `--deep` is not used: Apple advises against it for signing, and
/// it would hide the order. The identifier of the bundle is the `CFBundleIdentifier` of its `Info.plist`.
pub fn sign(plan: &SignPlan, bundle: &Path, nested: &[PathBuf]) -> Result<(), Diagnostic> {
    for path in nested.iter().map(PathBuf::as_path).chain([bundle]) {
        let mut command = Command::new("codesign");
        command
            .args(["--force", "--sign"])
            .arg(&plan.identity)
            .arg(path);
        // What codesign prints when it succeeds ("replacing existing signature") is not a notice of the build.
        driver::run_tool(&mut command, CODESIGN_HINT)?;
    }
    Ok(())
}

/// The top-level dictionary of the `Info.plist` of an existing bundle or dSYM, if it can be read.
fn existing_info(directory: &Path) -> Option<Dictionary> {
    let path = directory.join("Contents").join("Info.plist");
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .and_then(|file| {
            file.take(plist::MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .ok()?;
    plist::read_dictionary(&bytes).ok()
}

fn string_of<'a>(entries: &'a Dictionary, key: &str) -> Option<&'a str> {
    match entries.get(&Key::new(key)) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Whether there is something at `path` that the build will replace, after checking that it may: a
/// directory, not a symbolic link, whose `Info.plist` satisfies `owned`. A marker file cannot record the
/// owner, because any file that is added to `Contents` breaks the signature.
fn replaceable(
    path: &Path,
    owned: impl Fn(&Dictionary) -> bool,
    what: &str,
) -> Result<bool, Diagnostic> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error("inspect output", path, error)),
    };
    let ours = metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && existing_info(path).is_some_and(|entries| owned(&entries));
    if ours {
        Ok(true)
    } else {
        Err(driver_error_of(
            DiagnosticKind::AppOutputNotOwned,
            format!(
                "'{}' exists and is not {what}; remove it or choose another output",
                file_name_of(path)
            ),
        ))
    }
}

/// The paths that the build replaces, and the checks that it may replace what is there now.
struct Outputs {
    bundle: PathBuf,
    /// `Name.app.dSYM` of a `-g` build.
    dsym: Option<PathBuf>,
    /// `Name.app.trap.json` of a `--trap-info` build.
    traps: Option<PathBuf>,
}

impl Outputs {
    /// Refuses a bundle or dSYM that this build did not make before, and one that holds an input of the
    /// build. Returns whether each of the three outputs exists.
    #[allow(clippy::too_many_arguments)]
    fn check(
        &self,
        project: &Project,
        links: &LinkInputs,
        inputs: &Inputs,
        root: &Path,
        settings: &MacosSettings,
        bundle_id: &str,
        executable: &str,
    ) -> Result<[bool; 3], Diagnostic> {
        let bundle = replaceable(
            &self.bundle,
            |entries| {
                string_of(entries, "CFBundleIdentifier") == Some(bundle_id)
                    && string_of(entries, "CFBundleExecutable") == Some(executable)
            },
            &format!("a bundle that tsuzuri built for '{bundle_id}'"),
        )?;
        let dsym_id = dsym_identifier(bundle_id);
        let dsym = match &self.dsym {
            Some(path) => replaceable(
                path,
                |entries| {
                    string_of(entries, "CFBundlePackageType") == Some("dSYM")
                        && string_of(entries, "CFBundleIdentifier") == Some(dsym_id.as_str())
                },
                &format!("the dSYM of '{bundle_id}'"),
            )?,
            None => false,
        };
        let traps = match &self.traps {
            Some(path) => {
                driver::protect_sources(project, path)?;
                driver::protect_links(links, path)?;
                fs::symlink_metadata(path).is_ok()
            }
            None => false,
        };
        for directory in [
            bundle.then_some(&self.bundle),
            self.dsym.as_ref().filter(|_| dsym),
        ]
        .into_iter()
        .flatten()
        {
            let directory = fs::canonicalize(directory)
                .map_err(|error| io_error("resolve output", directory, error))?;
            let holds =
                |path: &Path| fs::canonicalize(path).is_ok_and(|path| path.starts_with(&directory));
            let source = project
                .sources
                .iter()
                .chain(&project.manifests)
                .any(|source| source.origin == ModuleOrigin::User && holds(&source.path));
            let input = inputs.sources().any(|entry| holds(&entry.source))
                || settings
                    .info_plist
                    .as_ref()
                    .is_some_and(|path| holds(&root.join(path)));
            if source || input {
                return Err(driver_error_of(
                    DiagnosticKind::OutputOverwritesInput,
                    format!(
                        "refusing to replace '{}': it holds {} of this build; choose a different -o path",
                        file_name_of(&directory),
                        if source {
                            "a source or manifest"
                        } else {
                            "a [macos] input"
                        }
                    ),
                ));
            }
            if links.paths.iter().any(|path| holds(path)) {
                return Err(driver_error_of(
                    DiagnosticKind::OutputOverwritesLinkInput,
                    format!(
                        "output '{}' would overwrite a link input; choose a different -o path",
                        self.bundle.display()
                    ),
                ));
            }
        }
        Ok([bundle, dsym, traps])
    }
}

/// Moves each staged output to its place, first moving what is there into `temporary`. When a move
/// fails, the outputs that were already replaced are put back, so either all of them are new or all are
/// as they were.
fn publish(
    items: &[(PathBuf, &Path, bool)],
    temporary: &mut TemporaryDirectory,
) -> Result<(), Diagnostic> {
    let mut published: Vec<(&Path, PathBuf, bool)> = Vec::new();
    let result = (|| {
        for (index, (staged, output, previous)) in items.iter().enumerate() {
            let backup = temporary.path.join(format!("previous-{index}"));
            if *previous {
                fs::rename(output, &backup)
                    .map_err(|error| io_error("back up output", output, error))?;
            }
            if let Err(error) = fs::rename(staged, output) {
                // The slot of this item is empty now: put its own backup back before the others.
                published.push((output, backup, *previous));
                return Err(io_error("publish output", output, error));
            }
            published.push((output, backup, *previous));
        }
        Ok(())
    })();
    let Err(error) = result else {
        return Ok(());
    };
    for (index, (output, backup, previous)) in published.into_iter().enumerate().rev() {
        let discarded = temporary.path.join(format!("discarded-{index}"));
        let removed = match fs::rename(output, &discarded) {
            Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
        let restored = removed.and_then(|()| {
            if previous {
                fs::rename(&backup, output)
            } else {
                Ok(())
            }
        });
        if let Err(restore_error) = restored {
            temporary.keep();
            return Err(driver_error_of(
                DiagnosticKind::OutputPublicationFailed,
                format!(
                    "output publication failed ({}) and '{}' could not be restored ({restore_error}); recovery files remain in '{}'",
                    error.message,
                    output.display(),
                    temporary.path.display()
                ),
            ));
        }
    }
    Err(error)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    fs::write(path, bytes).map_err(|error| io_error("write bundle file", path, error))
}

fn create_directories(path: &Path) -> Result<(), Diagnostic> {
    fs::create_dir_all(path).map_err(|error| io_error("create bundle directory", path, error))
}

/// `tsuzuri build --emit app`: builds the executable of `options` and the bundle `output` around it.
pub fn build_app(
    module: &CheckedModule,
    project: &Project,
    output: &Path,
    options: BuildOptions,
    links: &LinkInputs,
    profile: &Profile,
) -> Result<Vec<String>, Diagnostic> {
    options.validate()?;
    if let Some(error) = driver::app_output_error(options.emit, output) {
        return Err(error);
    }
    let executable = driver::app_executable_name(output)
        .expect("app_output_error checked the name")
        .to_owned();
    let settings = &project.macos;
    let bundle_id = settings
        .bundle_id
        .as_deref()
        .ok_or_else(missing_bundle_id)?;
    // `[macos]` belongs to the root package, and its paths are relative to that package's directory.
    let root = project
        .root_package
        .as_ref()
        .map(|package| package.root.clone())
        .ok_or_else(missing_bundle_id)?;
    let inputs = Inputs::read(&root, settings)?;
    let extra = inputs
        .extra
        .as_ref()
        .map(|(file, entries)| (file.as_str(), entries));
    // The conflicts of `info_plist` are reported before anything is built; the version comes later.
    info_plist(&executable, settings, "0", extra)?;

    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create output directory", parent, error))?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| io_error("resolve output directory", parent, error))?;
    let bundle_name = output.file_name().expect("the output names a bundle");
    let outputs = Outputs {
        bundle: parent.join(bundle_name),
        dsym: options.debug_info.then(|| {
            let mut name = bundle_name.to_owned();
            name.push(".dSYM");
            parent.join(name)
        }),
        traps: options
            .trap_info
            .then(|| driver::trap_sidecar_path(&parent.join(bundle_name))),
    };
    outputs.check(
        project,
        links,
        &inputs,
        &root,
        settings,
        bundle_id,
        &executable,
    )?;

    // The executable is built outside the staged bundle: its sidecars (`.dwarf`, `.trap.json`) come next
    // to it, and a file that is not code in `Contents/MacOS` breaks the signature.
    let mut temporary = TemporaryDirectory::new(&parent)?;
    let build = temporary.path.join("build");
    create_directories(&build)?;
    let program = build.join("program");
    let contents = Path::new("Contents");
    let final_program = outputs
        .bundle
        .join(contents)
        .join("MacOS")
        .join(&executable);
    let messages = driver::build_bundle_executable(
        module,
        project,
        &program,
        &final_program,
        options.bundle_executable(),
        links,
        profile,
    )?;

    let image = fs::read(&program).map_err(|error| io_error("read executable", &program, error))?;
    let minimum = mach_o_min_version(&image).ok_or_else(|| {
        driver_error_of(
            DiagnosticKind::ToolchainMissingFeature,
            "the linked executable has no LC_BUILD_VERSION, so the LSMinimumSystemVersion of the bundle is unknown; link with a Clang whose linker records the minimum macOS version",
        )
    })?;
    drop(image);
    let info = info_plist(
        &executable,
        settings,
        &minimum_system_version(minimum),
        extra,
    )?;

    let staged_bundle = temporary.path.join(bundle_name);
    let staged_contents = staged_bundle.join(contents);
    let code = staged_contents.join("MacOS");
    create_directories(&code)?;
    let staged_program = code.join(&executable);
    fs::rename(&program, &staged_program)
        .map_err(|error| io_error("place executable", &staged_program, error))?;
    write_file(
        &staged_contents.join("Info.plist"),
        &plist::write(&Value::Dict(info)),
    )?;
    write_file(&staged_contents.join("PkgInfo"), b"APPL????")?;
    if !inputs.resources.is_empty() {
        let directory = staged_contents.join("Resources");
        create_directories(&directory)?;
        for entry in &inputs.resources {
            copy_entry(entry, &directory.join(&entry.name))?;
        }
    }
    let mut nested = Vec::new();
    if !inputs.embed.is_empty() {
        let directory = staged_contents.join("Frameworks");
        create_directories(&directory)?;
        for entry in &inputs.embed {
            let destination = directory.join(&entry.name);
            copy_entry(entry, &destination)?;
            nested.push(destination);
        }
    }

    let mut staged = Vec::new();
    let staged_dsym = match &outputs.dsym {
        Some(_) => {
            let mut name = bundle_name.to_owned();
            name.push(".dSYM");
            let directory = temporary.path.join(name);
            let symbols = directory.join(contents).join("Resources").join("DWARF");
            create_directories(&symbols)?;
            let mut flat = program.clone().into_os_string();
            flat.push(".dwarf");
            let symbols = symbols.join(&executable);
            fs::rename(&flat, &symbols)
                .map_err(|error| io_error("place debug symbols", &symbols, error))?;
            write_file(
                &directory.join(contents).join("Info.plist"),
                &plist::write(&Value::Dict(dsym_info_plist(settings)?)),
            )?;
            Some(directory)
        }
        None => None,
    };
    let staged_traps = outputs
        .traps
        .as_ref()
        .map(|_| driver::trap_sidecar_path(&program));

    sign(&SignPlan::ad_hoc(), &staged_bundle, &nested)?;

    // What is there now may have changed while the executable was built.
    let [bundle, dsym, traps] = outputs.check(
        project,
        links,
        &inputs,
        &root,
        settings,
        bundle_id,
        &executable,
    )?;
    staged.push((staged_bundle, outputs.bundle.as_path(), bundle));
    if let (Some(directory), Some(path)) = (staged_dsym, &outputs.dsym) {
        staged.push((directory, path.as_path(), dsym));
    }
    if let (Some(file), Some(path)) = (staged_traps, &outputs.traps) {
        staged.push((file, path.as_path(), traps));
    }
    publish(&staged, &mut temporary)?;
    temporary.close()?;
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_stay_inside_the_copied_tree() {
        let inside = |directory: &str, target: &str| {
            link_stays_inside(Path::new(directory), Path::new(target))
        };
        assert!(inside("", "A"));
        assert!(inside("Versions", "A"));
        assert!(inside("", "Versions/Current/Foo"));
        assert!(inside("Versions/A", "../B/Foo"));
        assert!(inside("Versions/A", "../../Foo"));
        assert!(!inside("Versions/A", "../../../Foo"));
        assert!(!inside("", "../Foo"));
        assert!(!inside("Versions", "/usr/lib/libz.dylib"));
        // A link that leaves and comes back is refused: the name outside the tree is not ours.
        assert!(!inside("", "../Foo.framework/Foo"));
    }

    #[test]
    fn reads_the_version_of_an_older_load_command() {
        // LC_VERSION_MIN_MACOSX 10.15.4, after one other command.
        let mut bytes = Vec::new();
        for word in [0xfeed_facfu32, 0x0100_000c, 0, 2, 2, 32, 0, 0] {
            bytes.extend(word.to_le_bytes());
        }
        for word in [0x1bu32, 16, 0, 0] {
            bytes.extend(word.to_le_bytes());
        }
        for word in [0x24u32, 16, (10 << 16) | (15 << 8) | 4, 0] {
            bytes.extend(word.to_le_bytes());
        }
        assert_eq!(mach_o_min_version(&bytes), Some((10, 15, 4)));
        assert_eq!(minimum_system_version((10, 15, 4)), "10.15.4");
    }
}
