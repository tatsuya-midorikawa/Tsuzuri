//! File watching for `tsuzuri watch` (PB06): polling with `stat`, no new crate.
//!
//! A [`Watcher`] remembers what the last run could have read: the user sources and the manifests
//! that `Project::load` found, each with the text that was read and the [`FileStamp`] taken just
//! before the read. [`Watcher::wait_for_change`] polls every [`POLL`]:
//!
//! - A file whose stamp changed is read again, and it counts only when its content differs, so a
//!   `touch` or a save without a change does not start a run.
//! - Every [`DISCOVER_EVERY`] polls the directories are scanned again (no file is opened) for
//!   added and removed sources and manifests. Where a stamp cannot show every write (not Unix),
//!   every file is read again then.
//! - A file that was written in the last [`RACY`](crate::resident::RACY) before it was read could
//!   be written again with the same stamp, because timestamps have a granularity. It is read
//!   again once, after the window has passed.
//! - A compiler file that was replaced (rebuilt) ends the wait at once, without a change in the
//!   project: the process runs an image that is no longer on disk, and a run now would show the
//!   result of the old compiler to somebody who waits for the new one's.
//!
//! Opening a file is the expensive part of loading a project on some machines (an on-access scan
//! makes it 0.3 to 0.7 ms), and a stamp is a `stat`, so a project of a thousand modules costs a
//! few milliseconds per poll instead of a second of reading.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::check::ModuleOrigin;
use crate::driver::{self, Project, SourceError};
use crate::resident::{FileStamp, STAMP_SHOWS_WRITES};

/// How often the stamps of the watched files are compared.
pub const POLL: Duration = Duration::from_millis(100);
/// Every this many polls (1 s) the directories are scanned for added and removed files.
pub const DISCOVER_EVERY: u32 = 10;

/// A watched file as the last run could have read it.
#[derive(Debug)]
struct Entry {
    stamp: Option<FileStamp>,
    text: String,
    /// Until this time the stamp does not prove that the file is as it was read: the file is read
    /// again then.
    verify_after: Option<SystemTime>,
}

/// The files that one run reads, and the directories that decide which files that is.
#[derive(Debug)]
struct Observation {
    files: BTreeMap<PathBuf, Entry>,
    /// The package roots, scanned for added and removed files.
    roots: Vec<PathBuf>,
    /// The root of the project itself, whose `Tsuzuri.lock` counts.
    project_root: PathBuf,
    /// What the scan found before the project was read, or why it could not scan.
    discovered: Result<BTreeSet<PathBuf>, String>,
    /// Why the project cannot be loaded, when it cannot (the text of its first error).
    failure: Option<String>,
}

/// Why [`Watcher::wait_for_change`] returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// A file that a run reads changed, and the snapshot for the next run is taken.
    Changed,
    /// The compiler file that the active state pinned was replaced.
    CompilerReplaced,
}

/// Watches the project at `input` (the path that `check`, `build` or `test` takes).
pub struct Watcher {
    input: PathBuf,
    /// Whether the project is read for `tsuzuri test`, which also reads the tests' sources.
    tests: bool,
    observation: Observation,
    polls: u32,
}

impl Watcher {
    /// Takes the first snapshot of the project. Take it before the first run: a save during the
    /// run then shows as a difference and starts another run.
    pub fn new(input: &Path, tests: bool) -> Self {
        Self {
            input: input.to_owned(),
            tests,
            observation: observe(input, tests, None),
            polls: 0,
        }
    }

    /// Returns when something that a run reads has changed since the last snapshot, and takes the
    /// snapshot for the next run; or when the compiler file that the active state pinned was
    /// replaced, which is looked at first at every poll.
    pub fn wait_for_change(&mut self) -> Wait {
        loop {
            std::thread::sleep(POLL);
            if crate::resident::pinned_compiler_replaced() {
                return Wait::CompilerReplaced;
            }
            if self.poll(SystemTime::now()) {
                self.observation = observe(&self.input, self.tests, Some(&self.observation));
                self.polls = 0;
                return Wait::Changed;
            }
        }
    }

    /// One poll at `now`: whether the project differs from the snapshot.
    fn poll(&mut self, now: SystemTime) -> bool {
        self.poll_with(now, !STAMP_SHOWS_WRITES)
    }

    /// `poll`, reading every file at the scan when `read_all` (where a stamp does not show every
    /// write).
    fn poll_with(&mut self, now: SystemTime, read_all: bool) -> bool {
        self.polls = self.polls.wrapping_add(1);
        let scan = self.polls % DISCOVER_EVERY == 0;
        if let Some(failure) = &self.observation.failure {
            // Nothing is watched but the answer to loading: it is looked up again at each scan.
            if !scan {
                return false;
            }
            let again = observe(&self.input, self.tests, Some(&self.observation));
            return again.failure.as_ref() != Some(failure);
        }
        let mut changed = Vec::new();
        for (path, entry) in &self.observation.files {
            let stamp = FileStamp::of(path);
            if stamp != entry.stamp
                || stamp.is_some_and(|stamp| !stamp.regular)
                || entry.verify_after.is_some_and(|time| now >= time)
                || (read_all && scan)
            {
                changed.push(path.clone());
            }
        }
        if scan
            && discover(&self.observation.roots, &self.observation.project_root)
                != self.observation.discovered
        {
            return true;
        }
        let mut absorbed = Vec::new();
        for path in changed {
            let entry = &self.observation.files[&path];
            match fs::read(&path) {
                Ok(bytes) if bytes == entry.text.as_bytes() => {
                    let stamp = FileStamp::of(&path);
                    // The stamp is that of a regular file, or the file was replaced by a link.
                    if stamp.is_some_and(|stamp| !stamp.regular) {
                        return true;
                    }
                    absorbed.push((path, stamp));
                }
                _ => return true,
            }
        }
        for (path, stamp) in absorbed {
            if let Some(entry) = self.observation.files.get_mut(&path) {
                entry.stamp = stamp;
                entry.verify_after = FileStamp::verify_after(stamp, now);
            }
        }
        false
    }
}

/// The directory that `Project::load` takes the sources of `input` from.
fn project_directory(input: &Path) -> PathBuf {
    let entry = if fs::symlink_metadata(input).is_ok_and(|metadata| metadata.is_dir()) {
        input.join("Main.tz")
    } else {
        input.to_owned()
    };
    entry
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn failure_text(error: &SourceError) -> String {
    format!(
        "{}: {}: {}",
        error.path.display(),
        error.diagnostic.code,
        error.diagnostic.message
    )
}

/// The sources and manifests that exist under `roots`, found by the walk that `Project::load`
/// makes (hidden entries and the limits are the same). Nothing is opened.
fn discover(roots: &[PathBuf], project_root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut found = BTreeSet::new();
    for root in roots {
        let sources = driver::collect_sources(root, roots).map_err(|error| failure_text(&error))?;
        found.extend(sources);
        let manifest = root.join("Tsuzuri.toml");
        if fs::symlink_metadata(&manifest).is_ok() {
            found.insert(manifest);
            // A lockfile belongs to the root manifest, which is there whenever the lockfile is.
            let lock = root.join("Tsuzuri.lock");
            if root == project_root && fs::symlink_metadata(&lock).is_ok() {
                found.insert(lock);
            }
        }
    }
    Ok(found)
}

/// Reads the project at `input` and stamps what it read. `previous` tells where to look for the
/// files before the project is read.
fn observe(input: &Path, tests: bool, previous: Option<&Observation>) -> Observation {
    let started = SystemTime::now();
    let directory = project_directory(input);
    let (roots, project_root) = previous.map_or_else(
        || (vec![directory.clone()], directory.clone()),
        |previous| (previous.roots.clone(), previous.project_root.clone()),
    );
    // The stamps come before the read: a write between them shows as a different stamp.
    let discovered = discover(&roots, &project_root);
    let mut stamps: BTreeMap<PathBuf, Option<FileStamp>> = BTreeMap::new();
    let known = discovered.iter().flatten();
    let before = previous
        .into_iter()
        .flat_map(|previous| previous.files.keys());
    for path in known.chain(before) {
        stamps
            .entry(path.clone())
            .or_insert_with(|| FileStamp::of(path));
    }
    let loaded = if tests {
        Project::load_for_tests(input)
    } else {
        Project::load(input)
    };
    let project = match loaded {
        Ok(project) => project,
        Err(error) => {
            return Observation {
                files: BTreeMap::new(),
                roots: vec![directory.clone()],
                project_root: directory,
                discovered,
                failure: Some(failure_text(&error)),
            };
        }
    };
    let sources = project
        .sources
        .iter()
        .filter(|source| source.origin == ModuleOrigin::User);
    let mut files = BTreeMap::new();
    for source in sources.chain(&project.manifests) {
        let (stamp, verify_after) = match stamps.get(&source.path) {
            Some(stamp) => (*stamp, FileStamp::verify_after(*stamp, started)),
            // Not found before the read, so the stamp may be newer than the text: read it again.
            None => (FileStamp::of(&source.path), Some(started)),
        };
        files.insert(
            source.path.clone(),
            Entry {
                stamp,
                text: source.text.clone(),
                verify_after,
            },
        );
    }
    let mut roots: BTreeSet<PathBuf> = project
        .manifests
        .iter()
        .filter(|manifest| manifest.relative_path == Path::new("Tsuzuri.toml"))
        .filter_map(|manifest| manifest.path.parent().map(Path::to_path_buf))
        .collect();
    if roots.is_empty() {
        roots.insert(directory.clone());
    }
    let project_root = project
        .root_package
        .as_ref()
        .map_or(directory, |package| package.root.clone());
    Observation {
        files,
        roots: roots.into_iter().collect(),
        project_root,
        discovered,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::TemporaryDirectory;
    use crate::resident::RACY;
    use std::fs::File;

    fn project() -> (TemporaryDirectory, PathBuf) {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("app");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Main.tz"), "42\n").unwrap();
        (temporary, root)
    }

    /// A poll as the clock `now` shows it, `count` times.
    fn polls(watcher: &mut Watcher, now: SystemTime, count: u32) -> bool {
        (0..count).any(|_| watcher.poll(now))
    }

    fn set_modified(path: &Path, time: SystemTime) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }

    #[test]
    fn the_snapshot_holds_the_user_sources_and_manifests_but_not_std() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("app");
        fs::create_dir(&root).unwrap();
        crate::package::create_project(&root, Some("App"), false).unwrap();
        fs::write(root.join("Extra.tz"), "let x = 1\n").unwrap();
        fs::write(root.join("notes.txt"), "not a source").unwrap();
        let watcher = Watcher::new(&root, false);
        assert_eq!(watcher.observation.failure, None);
        let names: Vec<_> = watcher
            .observation
            .files
            .keys()
            .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, ["Extra.tz", "Main.tz", "Tsuzuri.toml"]);
        assert_eq!(watcher.observation.roots.len(), 1);
        assert_eq!(
            watcher.observation.project_root,
            watcher.observation.roots[0]
        );
        let (_temporary, root) = project();
        let watcher = Watcher::new(&root, false);
        assert_eq!(
            watcher.observation.files[&root.join("Main.tz")].text,
            "42\n"
        );
        assert_eq!(watcher.observation.roots, std::slice::from_ref(&root));
    }

    #[test]
    fn a_rewrite_with_the_same_length_and_time_changes_the_stamp_on_unix() {
        let (_temporary, root) = project();
        let path = root.join("Main.tz");
        let before = FileStamp::of(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, "43\n").unwrap();
        set_modified(&path, modified);
        let after = FileStamp::of(&path).unwrap();
        assert_eq!(after.len, before.len);
        assert_eq!(after.modified, before.modified);
        // Where the status-change time is in the stamp, the write shows without reading.
        assert_eq!(after != before, STAMP_SHOWS_WRITES);
        assert!(FileStamp::of(&root.join("missing")).is_none());
    }

    #[test]
    fn racy_means_a_read_less_than_the_granularity_after_the_stamp() {
        let (_temporary, root) = project();
        let stamp = FileStamp::of(&root.join("Main.tz")).unwrap();
        let time = stamp.time();
        assert!(stamp.is_racy(time));
        assert!(stamp.is_racy(time + RACY - Duration::from_millis(1)));
        assert!(!stamp.is_racy(time + RACY));
        assert_eq!(
            FileStamp::verify_after(Some(stamp), time),
            Some(time + RACY)
        );
        assert_eq!(FileStamp::verify_after(Some(stamp), time + RACY), None);
        assert_eq!(FileStamp::verify_after(None, time), None);
    }

    #[test]
    fn a_touch_is_absorbed_and_an_edit_is_found_on_the_next_poll() {
        let (_temporary, root) = project();
        let path = root.join("Main.tz");
        // Old enough that the file is not racy.
        let old = SystemTime::now() - Duration::from_secs(3600);
        set_modified(&path, old);
        let mut watcher = Watcher::new(&root, false);
        let later = SystemTime::now() + RACY * 2;
        assert!(!polls(&mut watcher, later, 3));
        // A touch changes the stamp but not the text: it is not a change, and the new stamp is kept.
        set_modified(&path, old + Duration::from_secs(60));
        assert!(!watcher.poll(later));
        assert_eq!(watcher.observation.files[&path].stamp, FileStamp::of(&path));
        assert!(!polls(&mut watcher, later, 3));
        // The same length, and the time of before: a change that only the status-change time shows.
        fs::write(&path, "43\n").unwrap();
        set_modified(&path, old + Duration::from_secs(60));
        assert_eq!(watcher.poll(later), STAMP_SHOWS_WRITES);
    }

    #[test]
    fn reading_every_file_at_the_scan_finds_a_write_that_the_stamp_hides() {
        let (_temporary, root) = project();
        let path = root.join("Main.tz");
        let mut watcher = Watcher::new(&root, false);
        let later = SystemTime::now() + RACY * 2;
        // The first poll reads the file once more, which it was written a moment before the snapshot.
        assert!(!watcher.poll(later));
        // The text of the snapshot is not the file's, and the stamp is: as on a file system whose
        // stamp a program can set back.
        watcher.observation.files.get_mut(&path).unwrap().text = "41\n".into();
        assert!(!polls(&mut watcher, later, DISCOVER_EVERY - 2));
        assert!(watcher.poll_with(later, true));
        let mut watcher = Watcher::new(&root, false);
        assert!(!watcher.poll(later));
        watcher.observation.files.get_mut(&path).unwrap().text = "41\n".into();
        // `poll` reads every file at the scan only where a stamp cannot show every write.
        assert_eq!(
            polls(&mut watcher, later, 3 * DISCOVER_EVERY),
            !STAMP_SHOWS_WRITES
        );
    }

    #[test]
    fn a_racy_file_is_read_once_after_its_window() {
        let (_temporary, root) = project();
        let path = root.join("Main.tz");
        let mut watcher = Watcher::new(&root, false);
        let entry = watcher.observation.files.get_mut(&path).unwrap();
        let time = entry.stamp.unwrap().time();
        // A file written a moment before it was read is read again after the window ...
        entry.verify_after = Some(time + RACY);
        // ... and what it holds then is compared: a different text is a change.
        entry.text = "41\n".into();
        assert!(!watcher.poll(time + RACY - Duration::from_millis(1)));
        assert!(watcher.poll(time + RACY));
        // The same text clears the mark.
        let mut watcher = Watcher::new(&root, false);
        watcher
            .observation
            .files
            .get_mut(&path)
            .unwrap()
            .verify_after = Some(time + RACY);
        assert!(!watcher.poll(time + RACY));
        assert_eq!(watcher.observation.files[&path].verify_after, None);
    }

    #[test]
    fn additions_and_removals_are_found_at_the_scan() {
        let (_temporary, root) = project();
        let mut watcher = Watcher::new(&root, false);
        let later = SystemTime::now() + RACY * 2;
        assert!(!polls(&mut watcher, later, DISCOVER_EVERY));
        let extra = root.join("Extra.tz");
        fs::write(&extra, "let x = 1\n").unwrap();
        // Not before the scan, which is every DISCOVER_EVERY polls.
        assert!(!polls(&mut watcher, later, DISCOVER_EVERY - 1));
        assert!(watcher.poll(later));
        watcher.observation = observe(&root, false, Some(&watcher.observation));
        watcher.polls = 0;
        assert!(watcher.observation.files.contains_key(&extra));
        assert!(!polls(&mut watcher, later, DISCOVER_EVERY));
        // A file removed is found at once: its stamp is gone.
        fs::remove_file(&extra).unwrap();
        assert!(watcher.poll(later));
        watcher.observation = observe(&root, false, Some(&watcher.observation));
        watcher.polls = 0;
        // A hidden file, a file of another kind and a new directory of sources are as `load` sees them.
        fs::write(root.join(".Hidden.tz"), "let x = 1\n").unwrap();
        fs::write(root.join("notes.txt"), "text").unwrap();
        assert!(!polls(&mut watcher, later, DISCOVER_EVERY));
        fs::create_dir(root.join("Deep")).unwrap();
        fs::write(root.join("Deep").join("Inner.tz"), "let x = 1\n").unwrap();
        assert!(polls(&mut watcher, later, DISCOVER_EVERY));
    }

    #[test]
    fn a_project_that_cannot_be_loaded_is_watched_for_the_fix() {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("late");
        let mut watcher = Watcher::new(&root, false);
        let failure = watcher.observation.failure.clone().unwrap();
        assert!(failure.contains("E2001"), "{failure}");
        let later = SystemTime::now() + RACY * 2;
        // The same failure is no change, however long it lasts ...
        assert!(!polls(&mut watcher, later, 3 * DISCOVER_EVERY));
        // ... and the directory that appears is, at the next scan.
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Main.tz"), "42\n").unwrap();
        assert!(!polls(&mut watcher, later, DISCOVER_EVERY - 1));
        assert!(watcher.poll(later));
        let next = observe(&root, false, Some(&watcher.observation));
        assert_eq!(next.failure, None);
        assert!(next.files.contains_key(&root.join("Main.tz")));
    }

    #[cfg(unix)]
    #[test]
    fn a_source_replaced_by_a_link_is_a_change_even_when_the_text_is_the_same() {
        let (_temporary, root) = project();
        let path = root.join("Main.tz");
        let mut watcher = Watcher::new(&root, false);
        let later = SystemTime::now() + RACY * 2;
        let copy = root.parent().unwrap().join("Elsewhere.tz");
        fs::write(&copy, "42\n").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&copy, &path).unwrap();
        assert!(watcher.poll(later));
    }

    #[test]
    fn tests_read_the_project_without_an_entry() {
        let (_temporary, root) = project();
        fs::write(
            root.join("Specs.tz"),
            "test \"one\" = Test.is_true (1 == 1)\n",
        )
        .unwrap();
        fs::remove_file(root.join("Main.tz")).unwrap();
        // Without Main.tz a program has no entry, and the tests of a project need none.
        assert!(Watcher::new(&root, false).observation.failure.is_some());
        let watcher = Watcher::new(&root, true);
        assert_eq!(watcher.observation.failure, None);
        assert!(
            watcher
                .observation
                .files
                .contains_key(&root.join("Specs.tz"))
        );
    }

    #[test]
    fn a_replaced_compiler_ends_the_wait_without_a_change_in_the_project() {
        use crate::resident::{ResidentState, Scope};
        let (_temporary, root) = project();
        let mut watcher = Watcher::new(&root, false);
        // The state started from a compiler that is not the file on disk.
        let mut state = ResidentState::pinned_to([9; 32]);
        let started = std::time::Instant::now();
        let waited = {
            let _scope = Scope::enter(&mut state);
            watcher.wait_for_change()
        };
        assert_eq!(waited, Wait::CompilerReplaced);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_change_of_the_project_ends_the_wait_while_the_compiler_is_the_one_that_runs() {
        use crate::resident::{ResidentState, Scope};
        let (_temporary, root) = project();
        let mut watcher = Watcher::new(&root, false);
        // Another length: the stamp shows it whatever the granularity of the file system.
        fs::write(root.join("Main.tz"), "4242\n").unwrap();
        let mut state = ResidentState::new();
        let waited = {
            let _scope = Scope::enter(&mut state);
            watcher.wait_for_change()
        };
        assert_eq!(waited, Wait::Changed);
        assert_eq!(
            watcher.observation.files[&root.join("Main.tz")].text,
            "4242\n",
            "the snapshot for the next run is taken"
        );
    }
}
