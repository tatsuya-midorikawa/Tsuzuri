//! A native executable built from codegen units (PB07). `llvm::split` cuts the final IR text into units; the objects
//! that the unit cache holds are read, the rest are compiled by Clang in parallel and stored, and the caller links
//! every object in the order of the units.
//!
//! The phases of `TSUZURI_TIME_PASSES` are `split` (the cut, on this thread), `units` (keys, cache reads and writes,
//! the files, on this thread's workers) and the wait for Clang as `tool.<clang>`. A worker never touches the
//! timings, so the phases are wall-clock time and do not overlap.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use super::{
    Backend, BuildOptions, CODEGEN_UNITS_VARIABLE, Emit, OptLevel, Platform, Profile, Target,
    collect_message, driver_error, io_error, run_tool_untimed,
};
use crate::cache::{self, BuildCache, ToolIdentity};
use crate::diagnostic::Diagnostic;
use crate::llvm::split::{SplitInput, Unit, split};

/// `TSUZURI_CODEGEN_UNITS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// The compiler decides: an eligible build of [`AUTO_MIN_BYTES`] of IR or more is split.
    Auto,
    /// Never: one module, as before PB07.
    Single,
    /// Every eligible build, however small.
    Split,
}

/// What an `auto` build needs to be cut into units: the IR text of at least this many bytes. A unit costs a Clang
/// job of about 8 ms of CPU time whatever its size, so a small program is faster from one module; on the machine of
/// the measurements (10 cores, `docs/benchmarks.md`) a cold build of 60 to 100 synthetic modules (2.5 to 4.1 MB of IR)
/// is as fast either way, and the units are faster from there.
pub(super) const AUTO_MIN_BYTES: usize = 4 << 20;

pub(super) fn mode() -> Result<Mode, Diagnostic> {
    mode_of(env::var_os(CODEGEN_UNITS_VARIABLE).as_deref())
}

pub(super) fn mode_of(value: Option<&OsStr>) -> Result<Mode, Diagnostic> {
    match value {
        None => Ok(Mode::Auto),
        Some(value) => match value.to_str() {
            Some("" | "auto") => Ok(Mode::Auto),
            Some("single") => Ok(Mode::Single),
            Some("split") => Ok(Mode::Split),
            _ => Err(driver_error(
                "E2000",
                format!(
                    "invalid {CODEGEN_UNITS_VARIABLE} value '{}'; use 'auto', 'single' or 'split'",
                    value.to_string_lossy()
                ),
            )),
        },
    }
}

/// Whether a build can be cut into units at all (the table of PB07): a native executable at `-O0` that Clang
/// builds on a machine with POSIX linking, for the machine it runs on. Everything else is built from one module.
pub(super) fn eligible(options: BuildOptions, platform: Platform, profile: &Profile) -> bool {
    options.target == Target::Native
        && options.emit == Emit::Executable
        && options.optimization == OptLevel::O0
        && options.backend == Backend::Llvm
        && !options.debug_info
        && !options.trap_info
        && !options.trap_return
        && !options.freestanding
        && !options.wasm_threads
        && platform.unix()
        && !platform.windows()
        && !platform.cross()
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

/// Cuts `text` into units and returns the object of each, in the order of the units, or `None` when `text` has a
/// form that a unit cannot hold (the caller builds one module instead, which is always correct).
pub(super) fn build(
    text: &str,
    modules: &[&str],
    config: &Config<'_>,
    messages: &mut Vec<String>,
) -> Result<Option<Vec<PathBuf>>, Diagnostic> {
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

    // The keys and the cache reads, with the objects of the hits written for the linker.
    let indices: Vec<usize> = (0..units.len()).collect();
    let lookups = parallel(&indices, |&index| {
        let Some((unit_cache, prefix)) = &cache else {
            return (None, false, None);
        };
        let key = cache::unit_key(prefix, &units[index].text);
        match unit_cache.load(&key) {
            Ok(Some(mut artifact)) if artifact.files.len() == 1 => {
                let Some(file) = artifact.files.remove("artifact") else {
                    return (Some(key), false, None);
                };
                match fs::write(&objects[index], file.bytes) {
                    Ok(()) => (Some(key), true, None),
                    Err(error) => (
                        Some(key),
                        false,
                        Some(format!("unit cache read failed; rebuilding: {error}")),
                    ),
                }
            }
            Ok(_) => (Some(key), false, None),
            Err(error) => (
                Some(key),
                false,
                Some(format!("unit cache read failed; rebuilding: {error}")),
            ),
        }
    });
    if let Some(message) = lookups.iter().find_map(|(_, _, message)| message.clone()) {
        messages.push(message);
    }
    let misses: Vec<usize> = (0..units.len())
        .filter(|index| !lookups[*index].1)
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

    let mut stored_any = false;
    let mut compiled: Vec<(Vec<usize>, String)> = Vec::new();
    if !misses.is_empty() {
        let bundles = bundles(&units, &misses, jobs());
        let working = cache::real_path(&directory)
            .map_err(|error| io_error("resolve the unit directory", &directory, error))?;
        let started = crate::timings::start();
        let outcomes: Vec<Result<String, Diagnostic>> = thread::scope(|scope| {
            let workers: Vec<_> = bundles
                .iter()
                .map(|bundle| {
                    let (config, working) = (config, &working);
                    scope.spawn(move || {
                        let mut command = Command::new(&config.clang);
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
                    return Err(driver_error(
                        "E2002",
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
            let results = parallel(&stores, |&index| {
                let Some(key) = &lookups[index].0 else {
                    return Ok(false);
                };
                let paths = std::collections::BTreeMap::from([(
                    "artifact".to_owned(),
                    objects[index].clone(),
                )]);
                unit_cache.store_entry(key, &paths, &[])
            });
            stored_any = results.iter().any(|result| matches!(result, Ok(true)));
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
    let _ = stored_any;
    for (_, message) in compiled {
        collect_message(messages, message);
    }
    Ok(Some(objects))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{Cpu, Triple};

    #[test]
    fn codegen_units_mode_accepts_auto_single_split_and_rejects_others() {
        assert_eq!(mode_of(None).unwrap(), Mode::Auto);
        assert_eq!(mode_of(Some(OsStr::new(""))).unwrap(), Mode::Auto);
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
        if cfg!(windows) {
            assert!(
                !eligible(base, host, &Profile::None),
                "Windows builds one unit"
            );
            return;
        }
        assert!(eligible(base, host, &Profile::None));
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
        ] {
            assert!(eligible(options, host, &Profile::None), "{options:?}");
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
                "--emit object",
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
                "wasm32",
                BuildOptions {
                    target: Target::Wasm32,
                    emit: Emit::Wasm,
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
                "-g",
                BuildOptions {
                    debug_info: true,
                    ..base
                },
            ),
            (
                "--trap-info",
                BuildOptions {
                    trap_info: true,
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
            assert!(!eligible(options, host, &Profile::None), "{name}");
        }
        assert!(!eligible(base, host, &Profile::Generate("profiles".into())));
        assert!(!eligible(
            base,
            host,
            &Profile::Use("merged.profdata".into())
        ));
        // A named target is built from one module, even the machine's own.
        let named = Platform::new(Triple::parse("x86_64-pc-windows-msvc"));
        assert!(!eligible(base, named, &Profile::None));
    }

    #[test]
    fn bundles_balance_the_text_and_keep_unit_order() {
        let unit = |size: usize| Unit {
            name: String::new(),
            text: "x".repeat(size),
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
}
