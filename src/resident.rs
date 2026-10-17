//! What a process that runs many commands keeps between them (PB06): `tsuzuri watch` and
//! `tsuzuri serve`.
//!
//! Loading a project opens every source file, and on some machines an `open` costs 0.3 to 0.7 ms
//! (an on-access scan), which is half of the `check` of a thousand modules. A [`ResidentState`]
//! remembers the text of each source file with the [`FileStamp`] taken before it was read, and
//! gives the text back while the stamp is the same: a `stat` instead of an `open`. It remembers
//! the SHA-256 of a file the same way (every build key hashes the compiler's own binary, 9 MB).
//! It also pins the identity of the compiler that started the process, so that the cache files
//! of the frontend are never written under the identity of a binary that replaced it.
//!
//! The state is installed on the thread that runs a command by a [`Scope`], as a platform is
//! (`PlatformScope`): the code that reads a file asks here, and does what it always did when no
//! scope is active, so a command line that runs once is unchanged. Where a stamp cannot show every
//! write (not Unix), nothing is remembered.
//!
//! A text is remembered only after a successful read, so an error is always the error of a real
//! read. A stamp that was made less than [`RACY`] before the read could be the stamp of a later
//! write too, and such an entry is not used (see [`FileStamp::is_racy`]).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The longest timestamp granularity that is taken into account (a FAT file system's 2 s). A write
/// less than this long after the stamp's time can leave the stamp as it was.
pub const RACY: Duration = Duration::from_secs(2);
/// Whether every write to a file changes its stamp, so that a file whose stamp is the same need
/// not be read: on Unix the status-change time says so, and a program cannot set it back.
pub(crate) const STAMP_SHOWS_WRITES: bool = cfg!(unix);
/// The most that a state holds before it drops everything (the sources, the digests).
pub const MAX_RESIDENT_BYTES: u64 = 1 << 30;

/// What `lstat` says about a file: enough to tell, without reading it, that it was written. On Unix
/// that is the device, the inode, the size, the modification time and the status-change time, which
/// the kernel sets on every write, rename, `utimes` and `chmod`. Elsewhere it is the size and the
/// modification time, which a program can set back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
    /// A regular file, not a symbolic link or a directory.
    pub(crate) regular: bool,
    pub(crate) len: u64,
    pub(crate) modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64),
    #[cfg(unix)]
    changed: (i64, i64),
}

impl FileStamp {
    /// The stamp of the file at `path` itself (a symbolic link is not followed), or `None` when
    /// there is none.
    pub(crate) fn of(path: &Path) -> Option<Self> {
        fs::symlink_metadata(path)
            .ok()
            .map(|metadata| Self::from_metadata(&metadata))
    }

    pub(crate) fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            regular: metadata.is_file(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            identity: (metadata.dev(), metadata.ino()),
            #[cfg(unix)]
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }

    /// The time of the newest change that the stamp records.
    #[cfg(unix)]
    pub(crate) fn time(&self) -> SystemTime {
        let (seconds, nanoseconds) = self.changed;
        match (u64::try_from(seconds), u32::try_from(nanoseconds)) {
            (Ok(seconds), Ok(nanoseconds)) => UNIX_EPOCH + Duration::new(seconds, nanoseconds),
            _ => UNIX_EPOCH,
        }
    }

    #[cfg(not(unix))]
    pub(crate) fn time(&self) -> SystemTime {
        self.modified.unwrap_or(UNIX_EPOCH)
    }

    /// Whether a write after `read_at` could leave this stamp as it is. The time of a stamp has a
    /// granularity, so a read that is not at least [`RACY`] later than the stamp's time cannot rule
    /// out another write in the same tick (git calls such an entry racily clean).
    pub(crate) fn is_racy(&self, read_at: SystemTime) -> bool {
        read_at < self.time() + RACY
    }

    /// When a file that was read at `read_at` has to be read once more, if it has to be.
    pub(crate) fn verify_after(stamp: Option<Self>, read_at: SystemTime) -> Option<SystemTime> {
        stamp
            .filter(|stamp| stamp.is_racy(read_at))
            .map(|stamp| stamp.time() + RACY)
    }
}

/// A value that was computed from a file whose stamp was `stamp`, by a read that began at `read_at`.
struct Remembered<T> {
    stamp: FileStamp,
    read_at: SystemTime,
    value: T,
}

impl<T> Remembered<T> {
    /// Whether the value is still the file's: the stamp is the same, and the read was late enough
    /// for the stamp to show a later write.
    fn holds(&self, stamp: &FileStamp) -> bool {
        self.stamp == *stamp && !self.stamp.is_racy(self.read_at)
    }
}

/// The state of a process that stays.
#[derive(Default)]
pub struct ResidentState {
    sources: BTreeMap<PathBuf, Remembered<String>>,
    source_bytes: u64,
    digests: BTreeMap<PathBuf, Remembered<[u8; 32]>>,
    /// The identity of the compiler file that started the process (see `frontend_cache`).
    compiler: Option<[u8; 32]>,
    /// The texts that were given back without opening the file, and the files that were read.
    pub source_hits: u64,
    pub source_reads: u64,
    /// The digests that were given back without opening the file, and the files that were hashed.
    pub digest_hits: u64,
    pub digest_reads: u64,
}

impl ResidentState {
    /// The state of a process that stays: it pins the identity of the compiler that is running.
    pub fn new() -> Self {
        Self {
            compiler: crate::frontend_cache::running_compiler(),
            ..Self::default()
        }
    }

    /// What the state holds, in bytes (the texts and the digests).
    pub fn bytes(&self) -> u64 {
        self.source_bytes + 32 * self.digests.len() as u64
    }

    /// Drops everything when the state holds more than `MAX_RESIDENT_BYTES`; the pinned compiler
    /// stays. Returns whether it did.
    pub fn trim(&mut self) -> bool {
        if self.bytes() <= MAX_RESIDENT_BYTES {
            return false;
        }
        self.sources.clear();
        self.source_bytes = 0;
        self.digests.clear();
        true
    }

    /// Whether the compiler file is no longer the one that this process started from: it was
    /// replaced, rebuilt, or removed. A process that went on would write its caches and keys for
    /// a binary that it is not.
    pub fn compiler_changed(&self) -> bool {
        self.compiler
            .is_some_and(|pinned| crate::frontend_cache::running_compiler() != Some(pinned))
    }

    /// The identity of the compiler that started the process, hex-encoded.
    pub fn compiler_identity(&self) -> Option<String> {
        self.compiler.map(|identity| {
            identity
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        })
    }
}

thread_local! {
    /// The state of the command that this thread is running, if there is one.
    static ACTIVE: RefCell<Option<ResidentState>> = const { RefCell::new(None) };
}

/// Makes `state` the state of the commands that this thread runs until the guard is dropped, and
/// gives it back, with what it remembered, then.
#[must_use = "the state applies only while the scope is alive"]
pub struct Scope<'a> {
    state: &'a mut ResidentState,
    outer: Option<ResidentState>,
}

impl<'a> Scope<'a> {
    pub fn enter(state: &'a mut ResidentState) -> Self {
        let inner = std::mem::take(state);
        let outer = ACTIVE.with(|active| active.borrow_mut().replace(inner));
        Self { state, outer }
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let inner =
            ACTIVE.with(|active| std::mem::replace(&mut *active.borrow_mut(), self.outer.take()));
        *self.state = inner.unwrap_or_default();
    }
}

fn with<T>(operation: impl FnOnce(&mut ResidentState) -> T) -> Option<T> {
    ACTIVE.with(|active| active.borrow_mut().as_mut().map(operation))
}

/// Whether this thread runs its commands for a process that stays (`watch`, `serve`): such a
/// process never asks a build server to run a command.
pub fn is_active() -> bool {
    ACTIVE.with(|active| active.borrow().is_some())
}

/// The time a read begins, when a state is active and a stamp can show every write: `None`
/// otherwise, and then nothing is looked up or remembered.
pub(crate) fn clock() -> Option<SystemTime> {
    (STAMP_SHOWS_WRITES && is_active()).then(SystemTime::now)
}

/// The identity of the compiler that the active state pinned, if it pinned one.
pub(crate) fn pinned_compiler() -> Option<[u8; 32]> {
    with(|state| state.compiler).flatten()
}

/// The text that the active state remembers for the file at `path` whose `stat` is `metadata`,
/// if the file has not changed since it was read.
pub(crate) fn cached_source(path: &Path, metadata: &fs::Metadata) -> Option<String> {
    let stamp = FileStamp::from_metadata(metadata);
    with(|state| {
        let text = state
            .sources
            .get(path)
            .filter(|entry| entry.holds(&stamp))
            .map(|entry| entry.value.clone());
        if text.is_some() {
            state.source_hits += 1;
        }
        text
    })
    .flatten()
}

/// Remembers `text`, which was read from the file at `path` by a read that began at `read_at`
/// after `metadata` was taken.
pub(crate) fn remember_source(
    path: &Path,
    metadata: &fs::Metadata,
    read_at: SystemTime,
    text: &str,
) {
    let stamp = FileStamp::from_metadata(metadata);
    with(|state| {
        state.source_reads += 1;
        if state.bytes() + text.len() as u64 > MAX_RESIDENT_BYTES {
            state.sources.clear();
            state.source_bytes = 0;
        }
        let entry = Remembered {
            stamp,
            read_at,
            value: text.to_owned(),
        };
        state.source_bytes += text.len() as u64;
        if let Some(old) = state.sources.insert(path.to_owned(), entry) {
            state.source_bytes -= old.value.len() as u64;
        }
    });
}

/// The digest that the active state remembers for the file at `path`, whose stamp is `stamp`.
pub(crate) fn cached_digest(path: &Path, stamp: &FileStamp) -> Option<[u8; 32]> {
    with(|state| {
        let digest = state
            .digests
            .get(path)
            .filter(|entry| entry.holds(stamp))
            .map(|entry| entry.value);
        if digest.is_some() {
            state.digest_hits += 1;
        }
        digest
    })
    .flatten()
}

pub(crate) fn remember_digest(
    path: &Path,
    stamp: FileStamp,
    read_at: SystemTime,
    digest: [u8; 32],
) {
    with(|state| {
        state.digest_reads += 1;
        state.digests.insert(
            path.to_owned(),
            Remembered {
                stamp,
                read_at,
                value: digest,
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::TemporaryDirectory;
    #[cfg(unix)]
    use crate::{
        cache::{Sha256, file_digest},
        driver::read_source_text,
    };

    fn file(text: &str) -> (TemporaryDirectory, PathBuf) {
        let temporary = TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let path = temporary.path.join("Main.tz");
        fs::write(&path, text).unwrap();
        (temporary, path)
    }

    #[cfg(unix)]
    fn digest_of(bytes: &[u8]) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(bytes);
        hash.finalize()
    }

    #[test]
    fn nothing_is_active_outside_a_scope_or_on_another_thread() {
        assert!(clock().is_none());
        assert!(pinned_compiler().is_none());
        let mut state = ResidentState::new();
        let _scope = Scope::enter(&mut state);
        assert_eq!(clock().is_some(), STAMP_SHOWS_WRITES);
        assert!(pinned_compiler().is_some());
        std::thread::spawn(|| {
            assert!(clock().is_none());
            assert!(pinned_compiler().is_none());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_scope_takes_the_state_and_gives_it_back_also_when_the_command_panics() {
        let (_temporary, path) = file("text\n");
        let stamp = FileStamp::of(&path).unwrap();
        let mut state = ResidentState::new();
        {
            let _scope = Scope::enter(&mut state);
            remember_digest(&path, stamp, SystemTime::now(), [7; 32]);
        }
        assert_eq!(state.digest_reads, 1);
        assert!(state.compiler.is_some(), "the pinned compiler stays");
        assert!(clock().is_none());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scope = Scope::enter(&mut state);
            remember_digest(&path, stamp, SystemTime::now(), [8; 32]);
            panic!("a command panics");
        }));
        assert!(result.is_err());
        assert_eq!(state.digest_reads, 2);
        assert!(clock().is_none());
        // A scope inside a scope hides the outer state and restores it.
        let mut outer = ResidentState::new();
        let mut inner = ResidentState::new();
        let outer_scope = Scope::enter(&mut outer);
        {
            let _inner_scope = Scope::enter(&mut inner);
            remember_digest(&path, stamp, SystemTime::now(), [9; 32]);
        }
        remember_digest(&path, stamp, SystemTime::now(), [9; 32]);
        drop(outer_scope);
        assert_eq!((inner.digest_reads, outer.digest_reads), (1, 1));
    }

    #[cfg(unix)]
    #[test]
    fn a_text_comes_back_only_while_the_stamp_is_clean() {
        let (_temporary, path) = file("real\n");
        let metadata = fs::metadata(&path).unwrap();
        let stamp = FileStamp::from_metadata(&metadata);
        let mut state = ResidentState::new();
        let _scope = Scope::enter(&mut state);
        // Read within the granularity of the stamp: a later write could have the same stamp.
        remember_source(&path, &metadata, stamp.time(), "racy\n");
        assert_eq!(cached_source(&path, &metadata), None);
        remember_source(&path, &metadata, stamp.time() + RACY, "clean\n");
        assert_eq!(cached_source(&path, &metadata).as_deref(), Some("clean\n"));
        // Another file with the same name is another stamp.
        fs::remove_file(&path).unwrap();
        fs::write(&path, "real\n").unwrap();
        let other = fs::metadata(&path).unwrap();
        assert_eq!(cached_source(&path, &other), None);
    }

    #[cfg(unix)]
    #[test]
    fn read_source_text_does_not_open_a_file_whose_stamp_is_clean() {
        let (_temporary, path) = file("real\n");
        let metadata = fs::metadata(&path).unwrap();
        let stamp = FileStamp::from_metadata(&metadata);
        let mut state = ResidentState::new();
        {
            let _scope = Scope::enter(&mut state);
            // The first read is from the file, and a text that was read at once is racy.
            assert_eq!(read_source_text(&path).unwrap(), "real\n");
            assert_eq!(read_source_text(&path).unwrap(), "real\n");
            // A clean entry that is not what the file holds shows that the file is not opened.
            remember_source(&path, &metadata, stamp.time() + RACY, "planted\n");
            assert_eq!(read_source_text(&path).unwrap(), "planted\n");
            // A write changes the stamp, even with the same length and modification time.
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            fs::write(&path, "rewrit\n").unwrap();
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(modified)
                .unwrap();
            assert_eq!(read_source_text(&path).unwrap(), "rewrit\n");
            // Errors are those of a real read, every time.
            let missing = path.with_file_name("Missing.tz");
            let first = read_source_text(&missing).unwrap_err().message;
            assert_eq!(read_source_text(&missing).unwrap_err().message, first);
            let invalid = path.with_file_name("Invalid.tz");
            fs::write(&invalid, [0xff, 0xfe, b'\n']).unwrap();
            let first = read_source_text(&invalid).unwrap_err().message;
            assert!(first.contains("UTF-8"), "{first}");
            assert_eq!(read_source_text(&invalid).unwrap_err().message, first);
        }
        assert_eq!(state.source_hits, 1);
        // Without a state the file is read as it always was.
        assert_eq!(read_source_text(&path).unwrap(), "rewrit\n");
    }

    #[cfg(unix)]
    #[test]
    fn file_digest_does_not_open_a_file_whose_stamp_is_clean() {
        let (_temporary, path) = file("hashed\n");
        let stamp = FileStamp::from_metadata(&fs::metadata(&path).unwrap());
        let mut state = ResidentState::new();
        {
            let _scope = Scope::enter(&mut state);
            assert_eq!(file_digest(&path).unwrap(), digest_of(b"hashed\n"));
            remember_digest(&path, stamp, stamp.time() + RACY, [5; 32]);
            assert_eq!(file_digest(&path).unwrap(), [5; 32]);
            fs::write(&path, "hashes\n").unwrap();
            assert_eq!(file_digest(&path).unwrap(), digest_of(b"hashes\n"));
            assert!(file_digest(&path.with_file_name("Missing.tz")).is_err());
        }
        assert_eq!(state.digest_hits, 1);
        assert_eq!(file_digest(&path).unwrap(), digest_of(b"hashes\n"));
    }

    #[test]
    fn the_state_is_trimmed_over_the_limit_and_the_compiler_stays_pinned() {
        let (_temporary, path) = file("text\n");
        let metadata = fs::metadata(&path).unwrap();
        let mut state = ResidentState::new();
        assert!(!state.compiler_changed());
        assert!(state.compiler_identity().is_some_and(|hex| hex.len() == 64));
        let _scope = Scope::enter(&mut state);
        remember_source(&path, &metadata, SystemTime::now(), "text\n");
        drop(_scope);
        assert_eq!(state.bytes(), 5);
        assert!(!state.trim());
        state.source_bytes = MAX_RESIDENT_BYTES + 1;
        assert!(state.trim());
        assert_eq!(state.bytes(), 0);
        assert!(state.compiler.is_some());
        // A state that was not made for a process that stays has no compiler to compare with.
        assert!(!ResidentState::default().compiler_changed());
    }

    #[test]
    fn the_frontend_cache_uses_the_pinned_identity_inside_a_scope() {
        let mut state = ResidentState::new();
        state.compiler = Some([3; 32]);
        assert_ne!(crate::frontend_cache::compiler_identity(), Some([3; 32]));
        {
            let _scope = Scope::enter(&mut state);
            assert_eq!(crate::frontend_cache::compiler_identity(), Some([3; 32]));
        }
        assert_ne!(crate::frontend_cache::compiler_identity(), Some([3; 32]));
        // The pinned identity is not the running one, so the compiler counts as changed.
        assert!(state.compiler_changed());
    }
}
