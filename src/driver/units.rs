//! A native executable built from codegen units (PB07). `llvm::split` cuts the final IR text into units; the objects
//! that the unit cache holds are read, the rest are compiled by Clang in parallel and stored, and the caller links
//! every object in the order of the units.
//!
//! The phases of `TSUZURI_TIME_PASSES` are `split` (the cut, on this thread), `units` (keys, cache reads and writes,
//! the files, on this thread's workers) and the wait for Clang as `tool.<clang>`. A worker never touches the
//! timings, so the phases are wall-clock time and do not overlap.

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use super::{
    Backend, BuildOptions, CODEGEN_UNITS_VARIABLE, Emit, OptLevel, Platform, Profile, Target,
    collect_message, driver_error_of, io_error, run_tool_untimed,
};
use crate::cache::{self, BuildCache, ToolIdentity};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::llvm::split::{SplitInput, Unit, split};

/// `TSUZURI_CODEGEN_UNITS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// The compiler decides: an eligible build of [`AUTO_MIN_BYTES`] of IR or more is split.
    Auto,
    /// Never: one module, as before PB07. This is the default.
    Single,
    /// Every eligible build, however small.
    Split,
}

/// What an `auto` build needs to be cut into units: the IR text of at least this many bytes. A unit costs a Clang
/// job of about 8 ms of CPU time and about as much for the declarations that it repeats, so the units spend about
/// three times the CPU time of one module, and a first build writes thousands of small files besides. On the machine
/// of the measurements (10 cores shared with other work, `docs/benchmarks.md`) the first build of 400 synthetic
/// modules (16.6 MB of IR) took 13% (without the cache) to 43% (with it) longer from units, and a rebuild after the
/// edit of one module took 31% less, so that the units pay back within two rebuilds; below that size they take more.
pub(super) const AUTO_MIN_BYTES: usize = 16_000_000;

pub(super) fn mode() -> Result<Mode, Diagnostic> {
    mode_of(env::var_os(CODEGEN_UNITS_VARIABLE).as_deref())
}

/// The mode that a value of the variable names; unset and empty are the default, one module.
pub(super) fn mode_of(value: Option<&OsStr>) -> Result<Mode, Diagnostic> {
    match value {
        None => Ok(Mode::Single),
        Some(value) => match value.to_str() {
            Some("" | "single") => Ok(Mode::Single),
            Some("auto") => Ok(Mode::Auto),
            Some("split") => Ok(Mode::Split),
            _ => Err(driver_error_of(
                DiagnosticKind::EnvironmentSettingInvalid,
                format!(
                    "invalid {CODEGEN_UNITS_VARIABLE} value '{}'; use 'auto', 'single' or 'split'",
                    value.to_string_lossy()
                ),
            )),
        },
    }
}

/// What a build knows about itself beyond its options when it decides whether to cut its module.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Facts {
    /// `--trap-info` numbered its sites so that an edit leaves the other files' numbers alone
    /// (`llvm::TrapNumbering::Stable`); a build that asks for trap information and has no such numbering is not cut.
    pub stable_trap_sites: bool,
    /// The wasm32 build checks its stack on entry, which `emit_wasm_build` makes.
    pub stack_checks: bool,
    /// The machine has a tool that makes symbols local (`localizer`), which an object that is made of units needs.
    pub can_localize: bool,
}

/// Whether a build can be cut into units at all (the table of PB07): a native executable or relocatable object, or a
/// wasm32 module that `wasm-ld` links, at `-O0`, that Clang builds on a machine with POSIX linking (for WebAssembly,
/// any host that is not Windows), for the machine it runs on. Everything else is built from one module. A macOS
/// object with debug information is one module as well: the relocatable link keeps a debug map of the unit objects
/// and drops their DWARF.
pub(super) fn eligible(
    options: BuildOptions,
    platform: Platform,
    profile: &Profile,
    facts: Facts,
) -> bool {
    let native = platform.unix() && !platform.windows() && !platform.cross();
    let kind = match (options.target, options.emit) {
        (Target::Native, Emit::Executable) => native,
        (Target::Native, Emit::Object) => {
            native && facts.can_localize && !(platform.mach_o() && options.debug_info)
        }
        (Target::Wasm32, Emit::Wasm) => !platform.windows() && !facts.stack_checks,
        _ => false,
    };
    kind && options.optimization == OptLevel::O0
        && options.backend == Backend::Llvm
        && (!options.trap_info || facts.stable_trap_sites)
        && !options.trap_return
        && !options.freestanding
        && !options.wasm_threads
        && matches!(profile, Profile::None)
}

/// Whether `mode` cuts an eligible build whose IR text has `bytes` bytes.
pub(super) fn wanted(mode: Mode, bytes: usize) -> bool {
    match mode {
        Mode::Single => false,
        Mode::Split => true,
        Mode::Auto => bytes >= AUTO_MIN_BYTES,
    }
}

/// What compiling the units takes: the Clang, its arguments up to the inputs, and where the objects go.
pub(super) struct Config<'a> {
    pub clang: OsString,
    /// `-x ir -Wno-override-module -O0 ...`: everything but `-c` and the inputs.
    pub args: Vec<OsString>,
    pub hint: String,
    pub directory: &'a Path,
    /// The identity of the Clang when the unit cache is enabled.
    pub identity: Option<&'a ToolIdentity>,
}

fn jobs() -> usize {
    match crate::parallel::thread_setting(env::var_os(crate::parallel::THREADS_VARIABLE).as_deref())
    {
        Ok(Some(threads)) => threads,
        _ => thread::available_parallelism().map_or(1, usize::from),
    }
}

/// `work` over `items` on up to `jobs` threads; the results come back in the order of the items.
fn parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = jobs().min(items.len());
    if threads <= 1 {
        return items.iter().map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let work = &work;
    let mut results: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= items.len() {
                            return done;
                        }
                        done.push((index, work(&items[index])));
                    }
                })
            })
            .collect();
        for worker in workers {
            for (index, result) in worker.join().expect("a unit worker does not panic") {
                results[index] = Some(result);
            }
        }
    });
    results
        .into_iter()
        .map(|result| result.expect("every item is done"))
        .collect()
}

fn object_name(index: usize) -> String {
    format!("u{index:05}.o")
}

/// What the unit cache says about one unit.
#[derive(Default)]
struct Lookup {
    key: Option<String>,
    /// The cached object, until it is written for the linker.
    object: Option<Vec<u8>>,
    /// The unit is in the cache and its object is written for the linker.
    hit: bool,
    /// The metadata of the entry when its time of use is to be written.
    touch: Option<serde_json::Value>,
    message: Option<String>,
}

/// Deletes the files that a build left in `directory` on several threads: thousands of small files take seconds to
/// delete one after another on macOS. The directory itself goes with the temporary directory that holds it.
pub(super) fn remove_files(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let paths: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .collect();
    parallel(&paths, |path| {
        let _ = fs::remove_file(path);
    });
}

/// The response file that names the objects to link, one quoted path per line (a link of thousands of objects does not
/// fit on a command line everywhere), as the argument `@<file>`.
pub(super) fn response_file(objects: &[PathBuf], directory: &Path) -> Result<OsString, Diagnostic> {
    let mut text = String::new();
    for object in objects {
        text.push('"');
        for character in object.to_string_lossy().chars() {
            if matches!(character, '"' | '\\') {
                text.push('\\');
            }
            text.push(character);
        }
        text.push_str("\"\n");
    }
    let path = directory.join("objects.rsp");
    fs::write(&path, text).map_err(|error| io_error("write the object list", &path, error))?;
    let mut argument = OsString::from("@");
    argument.push(&path);
    Ok(argument)
}

fn source_name(index: usize) -> String {
    format!("u{index:05}.ll")
}

/// Which bundle of the compile each unit belongs to: the largest units first, each to the bundle with the least text
/// so far, so that the processes end together. The order inside a bundle is the order of the units.
fn bundles(units: &[Unit], misses: &[usize], count: usize) -> Vec<Vec<usize>> {
    let count = count.min(misses.len()).max(1);
    let mut order: Vec<usize> = misses.to_vec();
    order.sort_by_key(|index| (std::cmp::Reverse(units[*index].text.len()), *index));
    let mut loads = vec![0usize; count];
    let mut bundles = vec![Vec::new(); count];
    for index in order {
        let lightest = (0..count)
            .min_by_key(|bundle| (loads[*bundle], *bundle))
            .expect("there is a bundle");
        loads[lightest] += units[index].text.len().max(1);
        bundles[lightest].push(index);
    }
    for bundle in &mut bundles {
        bundle.sort_unstable();
    }
    bundles.retain(|bundle| !bundle.is_empty());
    bundles
}

/// The objects of the units, in the order of the units, and the symbols that a relocatable object made of them makes local.
pub(super) struct Built {
    pub objects: Vec<PathBuf>,
    pub localize: Vec<String>,
}

/// `program` as a child that starts in another directory finds it. A name with a directory part (`./clang`,
/// `toolchain/bin/clang`) is relative to the directory of this process, and a command that has a `current_dir` looks
/// for it there; a bare name is searched in `PATH`, which the directory of the child does not change.
pub(super) fn anchored(program: &OsStr) -> OsString {
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() < 2 {
        return program.to_owned();
    }
    match env::current_dir() {
        Ok(directory) => directory.join(path).into_os_string(),
        Err(_) => program.to_owned(),
    }
}

/// Cuts `text` into units and returns the object of each, in the order of the units, or `None` when `text` has a
/// form that a unit cannot hold (the caller builds one module instead, which is always correct).
pub(super) fn build(
    text: &str,
    modules: &[&str],
    config: &Config<'_>,
    messages: &mut Vec<String>,
) -> Result<Option<Built>, Diagnostic> {
    let split_time = crate::timings::Phase::start("split");
    let units = match split(&SplitInput { ir: text, modules }) {
        Ok(units) => units,
        Err(_) => return Ok(None),
    };
    drop(split_time);

    let directory = config.directory.join("units");
    fs::create_dir_all(&directory)
        .map_err(|error| io_error("create the unit directory", &directory, error))?;
    let objects: Vec<PathBuf> = (0..units.len())
        .map(|index| directory.join(object_name(index)))
        .collect();

    let units_time = crate::timings::Phase::start("units");
    let cache = config.identity.and_then(|identity| {
        let root = cache::default_root()?;
        let prefix = cache::unit_key_prefix(&config.args, identity)?;
        match BuildCache::open(&root).and_then(|_| BuildCache::open(&root.join("units"))) {
            Ok(unit_cache) => Some((unit_cache.for_units(), prefix)),
            Err(error) => {
                messages.push(format!("unit cache disabled: {error}"));
                None
            }
        }
    });

    // The keys and the cache reads come first and the files are written after them: on macOS the opens of a process
    // are an order of magnitude slower while its other threads create files, so reading and writing in one pass cost
    // more than compiling the units.
    let indices: Vec<usize> = (0..units.len()).collect();
    let mut lookups = parallel(&indices, |&index| {
        let Some((unit_cache, prefix)) = &cache else {
            return Lookup::default();
        };
        let key = cache::unit_key(prefix, &units[index].text);
        match unit_cache.read(&key) {
            Ok(Some((mut artifact, due))) if artifact.files.len() == 1 => Lookup {
                object: artifact.files.remove("artifact").map(|file| file.bytes),
                touch: due,
                key: Some(key),
                ..Lookup::default()
            },
            Ok(_) => Lookup {
                key: Some(key),
                ..Lookup::default()
            },
            Err(error) => Lookup {
                key: Some(key),
                message: Some(format!("unit cache read failed; rebuilding: {error}")),
                ..Lookup::default()
            },
        }
    });
    // The objects of the hits, for the linker.
    let hits: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|index| lookups[*index].object.is_some())
        .collect();
    let restored = parallel(&hits, |&index| {
        fs::write(
            &objects[index],
            lookups[index].object.as_deref().unwrap_or_default(),
        )
    });
    for (index, result) in hits.into_iter().zip(restored) {
        let lookup = &mut lookups[index];
        lookup.object = None;
        match result {
            Ok(()) => lookup.hit = true,
            Err(error) => {
                lookup.touch = None;
                lookup.message = Some(format!("unit cache read failed; rebuilding: {error}"));
            }
        }
    }
    if let Some(message) = lookups.iter().find_map(|lookup| lookup.message.clone()) {
        messages.push(message);
    }
    let misses: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|index| !lookups[*index].hit)
        .collect();

    // The files that Clang reads.
    let written = parallel(&misses, |&index| {
        let path = directory.join(source_name(index));
        fs::write(&path, &units[index].text).map_err(|error| (path, error))
    });
    if let Some(Err((path, error))) = written.into_iter().find(Result::is_err) {
        return Err(io_error("write a unit of LLVM IR", &path, error));
    }
    drop(units_time);

    let mut compiled: Vec<(Vec<usize>, String)> = Vec::new();
    if !misses.is_empty() {
        let bundles = bundles(&units, &misses, jobs());
        let working = cache::real_path(&directory)
            .map_err(|error| io_error("resolve the unit directory", &directory, error))?;
        // The Clang runs in the unit directory, from where a relative path to it would not lead to it.
        let clang = anchored(&config.clang);
        let started = crate::timings::start();
        let outcomes: Vec<Result<String, Diagnostic>> = thread::scope(|scope| {
            let workers: Vec<_> = bundles
                .iter()
                .map(|bundle| {
                    let (config, working, clang) = (config, &working, &clang);
                    scope.spawn(move || {
                        let mut command = Command::new(clang);
                        command.current_dir(working).args(&config.args).arg("-c");
                        for index in bundle {
                            command.arg(source_name(*index));
                        }
                        run_tool_untimed(&mut command, &config.hint)
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().expect("a Clang worker does not panic"))
                .collect()
        });
        if started.is_some() {
            let stem = Path::new(&config.clang)
                .file_stem()
                .unwrap_or(config.clang.as_os_str())
                .to_string_lossy()
                .into_owned();
            crate::timings::finish(&format!("tool.{stem}"), started);
        }
        for (bundle, outcome) in bundles.into_iter().zip(outcomes) {
            compiled.push((bundle, outcome?));
        }
        for (bundle, _) in &compiled {
            for index in bundle {
                if !objects[*index].is_file() {
                    return Err(driver_error_of(
                        DiagnosticKind::ExternalToolFailed,
                        format!(
                            "Clang wrote no object for a unit of LLVM IR ({})",
                            units[*index].name
                        ),
                    ));
                }
            }
        }
        // A compile that printed something (a warning) is not stored: the next build would not say it again.
        let units_time = crate::timings::Phase::start("units");
        let mut stores: Vec<usize> = Vec::new();
        for (bundle, message) in &compiled {
            if message.trim().is_empty() {
                stores.extend(bundle.iter().copied());
            }
        }
        stores.sort_unstable();
        if let Some((unit_cache, _)) = &cache {
            // As with the reads of the cache, every object is read before the first entry is written.
            let read = parallel(&stores, |&index| {
                cache::read_files(&BTreeMap::from([(
                    "artifact".to_owned(),
                    objects[index].clone(),
                )]))
            });
            let prepared: Vec<_> = stores.iter().copied().zip(read).collect();
            let results = parallel(&prepared, |(index, files)| {
                let Some(key) = &lookups[*index].key else {
                    return Ok(false);
                };
                match files {
                    Ok(files) => unit_cache.store_files(key, files, &[]),
                    Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
                }
            });
            let stored_any = results.iter().any(|result| matches!(result, Ok(true)));
            if let Some(Err(error)) = results.into_iter().find(Result::is_err) {
                messages.push(format!("unit cache write failed: {error}"));
            }
            if stored_any && let Err(error) = unit_cache.evict_if_due(cache::UNIT_EVICT_INTERVAL_MS)
            {
                messages.push(format!("unit cache cleanup failed: {error}"));
            }
        }
        drop(units_time);
    }
    // The time of use of the entries that were read, written once all the reads are done. A failure only leaves the
    // entry looking older than it is.
    if let Some((unit_cache, _)) = &cache {
        let touches: Vec<_> = lookups
            .iter()
            .filter(|lookup| lookup.hit)
            .filter_map(|lookup| Some((lookup.key.as_ref()?, lookup.touch.as_ref()?)))
            .collect();
        if !touches.is_empty() {
            let units_time = crate::timings::Phase::start("units");
            parallel(&touches, |(key, metadata)| {
                unit_cache.touch(key, (*metadata).clone())
            });
            drop(units_time);
        }
    }
    for (_, message) in compiled {
        collect_message(messages, message);
    }
    let mut localize: Vec<String> = units
        .iter()
        .flat_map(|unit| unit.localize.iter().cloned())
        .collect();
    localize.sort_unstable();
    localize.dedup();
    Ok(Some(Built { objects, localize }))
}

/// The program that makes global symbols local in a relocatable object: Xcode's `nmedit` on macOS, an `objcopy` (GNU
/// or LLVM) elsewhere; `None` when the machine has none, and then an object is built from one unit.
pub(super) fn localizer(platform: Platform) -> Option<OsString> {
    let candidates: Vec<OsString> = if platform.mach_o() {
        vec![super::tool("TSUZURI_NMEDIT", "nmedit")]
    } else if let Some(tool) = env::var_os("TSUZURI_OBJCOPY") {
        vec![tool]
    } else {
        vec!["llvm-objcopy".into(), "objcopy".into()]
    };
    candidates.into_iter().find(|tool| {
        Command::new(tool)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .output()
            .is_ok()
    })
}

/// Makes `names` local in the relocatable object `object`. The units define what was `internal` as `hidden`, which the
/// link inside the object needs; left global, such a name would clash with the same name in another object of another
/// program that the host links with this one. Weak and other hidden symbols, which the runtime merges across objects on
/// purpose, are not touched.
pub(super) fn localize(
    tool: &OsStr,
    platform: Platform,
    names: &[String],
    object: &Path,
    directory: &Path,
    hint: &str,
    messages: &mut Vec<String>,
) -> Result<(), Diagnostic> {
    let mut list = String::new();
    for name in names {
        // A Mach-O symbol has the underscore that the C name does not.
        if platform.mach_o() {
            list.push('_');
        }
        list.push_str(name);
        list.push('\n');
    }
    let path = directory.join("localize.txt");
    fs::write(&path, list).map_err(|error| io_error("write the symbol list", &path, error))?;
    let edited = directory.join("localized.o");
    let mut command = Command::new(tool);
    if platform.mach_o() {
        command
            .arg("-R")
            .arg(&path)
            .arg(object)
            .arg("-o")
            .arg(&edited);
    } else {
        let mut option = OsString::from("--localize-symbols=");
        option.push(&path);
        command.arg(option).arg(object).arg(&edited);
    }
    collect_message(messages, super::run_tool(&mut command, hint)?);
    fs::rename(&edited, object).map_err(|error| io_error("replace the object", object, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{Cpu, Triple};

    #[test]
    fn codegen_units_mode_accepts_auto_single_split_and_rejects_others() {
        // The default is one module: unset and empty.
        assert_eq!(mode_of(None).unwrap(), Mode::Single);
        assert_eq!(mode_of(Some(OsStr::new(""))).unwrap(), Mode::Single);
        assert_eq!(mode_of(Some(OsStr::new("auto"))).unwrap(), Mode::Auto);
        assert_eq!(mode_of(Some(OsStr::new("single"))).unwrap(), Mode::Single);
        assert_eq!(mode_of(Some(OsStr::new("split"))).unwrap(), Mode::Split);
        let error = mode_of(Some(OsStr::new("x"))).unwrap_err();
        assert_eq!(error.code, "E2000");
        assert_eq!(
            error.message,
            "invalid TSUZURI_CODEGEN_UNITS value 'x'; use 'auto', 'single' or 'split'"
        );
        assert_eq!(
            mode_of(Some(OsStr::new("Single"))).unwrap_err().code,
            "E2000"
        );
    }

    #[test]
    fn remove_files_empties_a_directory_and_tolerates_a_missing_one() {
        let directory = crate::driver::TemporaryDirectory::new(&env::temp_dir()).unwrap();
        for index in 0..40 {
            fs::write(directory.path.join(format!("u{index:05}.o")), b"x").unwrap();
        }
        remove_files(&directory.path);
        assert_eq!(fs::read_dir(&directory.path).unwrap().count(), 0);
        remove_files(&directory.path.join("missing"));
        directory.close().unwrap();
    }

    #[test]
    fn auto_waits_for_enough_text() {
        assert!(!wanted(Mode::Single, usize::MAX));
        assert!(wanted(Mode::Split, 0));
        assert!(!wanted(Mode::Auto, AUTO_MIN_BYTES - 1));
        assert!(wanted(Mode::Auto, AUTO_MIN_BYTES));
    }

    #[test]
    fn split_eligible_matches_the_table() {
        use crate::llvm::Allocator;
        let base = BuildOptions {
            optimization: OptLevel::O0,
            ..BuildOptions::default()
        };
        let host = Platform::new(None);
        let ok = |options, platform: Platform, profile: &Profile| {
            eligible(options, platform, profile, Facts::default())
        };
        if cfg!(windows) {
            assert!(!ok(base, host, &Profile::None), "Windows builds one unit");
            return;
        }
        assert!(ok(base, host, &Profile::None));
        // Options that only change the compile or the link are no reason not to cut.
        for options in [
            BuildOptions {
                strip: true,
                ..base
            },
            BuildOptions {
                cpu: Cpu::Native,
                ..base
            },
            BuildOptions {
                cache: false,
                ..base
            },
            BuildOptions {
                allocator: Allocator::Small,
                ..base
            },
            BuildOptions {
                debug_info: true,
                ..base
            },
        ] {
            assert!(ok(options, host, &Profile::None), "{options:?}");
        }
        // Each row of the table, made false by itself.
        let ineligible = [
            (
                "-O1",
                BuildOptions {
                    optimization: OptLevel::O1,
                    ..base
                },
            ),
            (
                "-O2",
                BuildOptions {
                    optimization: OptLevel::O2,
                    ..base
                },
            ),
            (
                "-O3",
                BuildOptions {
                    optimization: OptLevel::O3,
                    ..base
                },
            ),
            (
                "-Os",
                BuildOptions {
                    optimization: OptLevel::Os,
                    ..base
                },
            ),
            (
                "--emit object without a localizer",
                BuildOptions {
                    emit: Emit::Object,
                    ..base
                },
            ),
            (
                "--emit llvm",
                BuildOptions {
                    emit: Emit::Llvm,
                    ..base
                },
            ),
            (
                "--emit shared",
                BuildOptions {
                    emit: Emit::Shared,
                    ..base
                },
            ),
            (
                "--emit bitcode",
                BuildOptions {
                    emit: Emit::Bitcode,
                    ..base
                },
            ),
            (
                "--emit header",
                BuildOptions {
                    emit: Emit::Header,
                    ..base
                },
            ),
            (
                "--backend fast",
                BuildOptions {
                    backend: Backend::Fast,
                    ..base
                },
            ),
            (
                "--trap-mode return",
                BuildOptions {
                    trap_return: true,
                    ..base
                },
            ),
            (
                "--freestanding",
                BuildOptions {
                    freestanding: true,
                    ..base
                },
            ),
        ];
        for (name, options) in ineligible {
            assert!(!ok(options, host, &Profile::None), "{name}");
        }
        // `--trap-info` is cut only when its sites have the numbering that keeps them stable.
        let traps = BuildOptions {
            trap_info: true,
            ..base
        };
        assert!(
            !ok(traps, host, &Profile::None),
            "--trap-info with dense sites"
        );
        assert!(
            eligible(
                traps,
                host,
                &Profile::None,
                Facts {
                    stable_trap_sites: true,
                    ..Facts::default()
                }
            ),
            "--trap-info with stable sites"
        );
        // A wasm32 module that wasm-ld links is cut; the builds that `emit_wasm_build` makes, wasm64, threads and a
        // module that is not linked are not.
        let wasm = BuildOptions {
            target: Target::Wasm32,
            emit: Emit::Wasm,
            ..base
        };
        assert!(ok(wasm, host, &Profile::None), "wasm32");
        // The tail-call feature only adds a flag to the compile of every unit (PM09), which the units share with the module.
        assert!(
            ok(
                BuildOptions {
                    wasm_tail_call: true,
                    ..wasm
                },
                host,
                &Profile::None
            ),
            "wasm32 with tail-call"
        );
        assert!(
            ok(
                BuildOptions {
                    wasm_simd: true,
                    ..wasm
                },
                host,
                &Profile::None
            ),
            "wasm32 with simd128"
        );
        assert!(
            !eligible(
                wasm,
                host,
                &Profile::None,
                Facts {
                    stack_checks: true,
                    ..Facts::default()
                }
            ),
            "wasm32 with stack checks"
        );
        // A native object is cut when the machine can make symbols local again, and a macOS object with debug
        // information is not.
        let object = BuildOptions {
            emit: Emit::Object,
            ..base
        };
        let localizing = Facts {
            can_localize: true,
            ..Facts::default()
        };
        assert!(
            !ok(object, host, &Profile::None),
            "object without a localizer"
        );
        assert!(eligible(object, host, &Profile::None, localizing), "object");
        let debug_object = BuildOptions {
            debug_info: true,
            ..object
        };
        assert_eq!(
            eligible(debug_object, host, &Profile::None, localizing),
            !host.mach_o(),
            "object with debug information"
        );
        assert!(
            !eligible(
                BuildOptions {
                    emit: Emit::Shared,
                    ..base
                },
                host,
                &Profile::None,
                localizing
            ),
            "shared library"
        );
        for (name, options) in [
            (
                "wasm32 threads",
                BuildOptions {
                    wasm_threads: true,
                    ..wasm
                },
            ),
            (
                "wasm64",
                BuildOptions {
                    target: Target::Wasm64,
                    ..wasm
                },
            ),
            (
                "wasm32 object",
                BuildOptions {
                    emit: Emit::Object,
                    ..wasm
                },
            ),
            (
                "wasm32 -O3",
                BuildOptions {
                    optimization: OptLevel::O3,
                    ..wasm
                },
            ),
            (
                "wasm32 fast backend",
                BuildOptions {
                    backend: Backend::Fast,
                    ..wasm
                },
            ),
        ] {
            assert!(!ok(options, host, &Profile::None), "{name}");
        }
        assert!(!ok(wasm, host, &Profile::Generate("profiles".into())));
        assert!(!ok(base, host, &Profile::Generate("profiles".into())));
        assert!(!ok(base, host, &Profile::Use("merged.profdata".into())));
        // A named target is built from one module, even the machine's own.
        let named = Platform::new(Triple::parse("x86_64-pc-windows-msvc"));
        assert!(!ok(base, named, &Profile::None));
    }

    #[test]
    fn bundles_balance_the_text_and_keep_unit_order() {
        let unit = |size: usize| Unit {
            name: String::new(),
            text: "x".repeat(size),
            localize: Vec::new(),
        };
        let units = vec![unit(10), unit(100), unit(1), unit(50), unit(60), unit(5)];
        let misses = [0, 1, 2, 3, 4, 5];
        let bundles = bundles(&units, &misses, 2);
        assert_eq!(bundles.len(), 2);
        let mut all: Vec<usize> = bundles.iter().flatten().copied().collect();
        all.sort_unstable();
        assert_eq!(all, misses);
        for bundle in &bundles {
            assert!(bundle.windows(2).all(|pair| pair[0] < pair[1]));
        }
        // The same input gives the same bundles.
        assert_eq!(bundles, super::bundles(&units, &misses, 2));
        // No more bundles than units, and none empty.
        assert_eq!(super::bundles(&units, &[3], 8), [[3]]);
    }

    #[test]
    fn parallel_returns_results_in_item_order() {
        let items: Vec<usize> = (0..257).collect();
        let doubled = parallel(&items, |item| item * 2);
        assert_eq!(
            doubled,
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
        assert!(parallel::<usize, usize>(&[], |item| *item).is_empty());
    }

    /// The Clang of a unit runs in the unit directory, so a path to it with a directory part (`TSUZURI_CLANG=./clang`)
    /// must not be left relative to the directory of the compiler; a bare name and an absolute path are as they are.
    #[test]
    fn a_tool_path_with_a_directory_is_anchored_to_the_directory_of_the_compiler() {
        let here = env::current_dir().unwrap();
        for relative in ["./clang-rel", "toolchain/bin/clang", "../clang"] {
            let anchored = anchored(OsStr::new(relative));
            assert_eq!(anchored, here.join(relative).into_os_string(), "{relative}");
            assert!(Path::new(&anchored).is_absolute(), "{relative}");
        }
        for unchanged in ["clang", "clang-21", ""] {
            assert_eq!(anchored(OsStr::new(unchanged)), OsString::from(unchanged));
        }
        let absolute = here.join("tools").join("clang");
        assert_eq!(anchored(absolute.as_os_str()), absolute.into_os_string());
    }
}
