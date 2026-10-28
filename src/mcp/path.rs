//! The paths of the tools (G29 "パスの閉じ込め"): relative to the project root, never through a
//! symbolic link or out of the root, and never shown with the root's absolute path.

use std::path::{Path, PathBuf};

use super::backend::ToolError;

const MAX_PATH_CHARS: usize = 4096;

/// What a path must name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    /// A directory or a `.tz`, `.tt` or `.tc` file.
    Source,
    Directory,
}

/// The path that `arg` names under `root` (already canonical), not canonicalized.
pub fn resolve_path(root: &Path, arg: &str, wanted: Wanted) -> Result<PathBuf, ToolError> {
    let fail = |message: String| Err(ToolError(message));
    if arg.chars().count() > MAX_PATH_CHARS {
        return fail("path is too long".into());
    }
    let bytes = arg.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if arg.contains('\0') || arg.contains('\\') || arg.starts_with('/') || drive {
        return fail(format!(
            "path must be relative to the project root: '{arg}'"
        ));
    }
    let parts: Vec<&str> = arg
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return fail(format!("path must not contain '..': '{arg}'"));
    }
    let mut path = root.to_path_buf();
    for part in &parts {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Err(_) => return fail(format!("no such file or directory: '{arg}'")),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return fail(format!("path goes through a symbolic link: '{arg}'"));
            }
            Ok(_) => {}
        }
    }
    let real = crate::cache::real_path(&path)
        .map_err(|_| ToolError(format!("no such file or directory: '{arg}'")))?;
    if !real.starts_with(root) {
        return fail(format!("path escapes the project root: '{arg}'"));
    }
    let source = path
        .extension()
        .is_some_and(|extension| matches!(extension.to_str(), Some("tz" | "tt" | "tc")))
        && path.is_file();
    match wanted {
        Wanted::Source if path.is_dir() || source => Ok(path),
        Wanted::Source => fail(format!(
            "path is neither a directory nor a .tz, .tt or .tc file: '{arg}'"
        )),
        Wanted::Directory if path.is_dir() => Ok(path),
        Wanted::Directory => fail(format!("path is not a directory: '{arg}'")),
    }
}

/// `path` relative to `root` with `/` between the parts, or `.` for the root and a path outside.
pub fn relative_text(root: &Path, path: &Path) -> String {
    let real = crate::cache::real_path(path).unwrap_or_else(|_| path.to_path_buf());
    match real.strip_prefix(root).or_else(|_| path.strip_prefix(root)) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".into(),
        Ok(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => ".".into(),
    }
}

/// `message` without the root's absolute path: `{root}/` and `{root}\` go, the root alone is `.`.
pub fn scrub(message: &str, root: &Path) -> String {
    let root = root.to_string_lossy();
    if root.is_empty() {
        return message.to_owned();
    }
    message
        .replace(&format!("{root}/"), "")
        .replace(&format!("{root}\\"), "")
        .replace(root.as_ref(), ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "tsuzuri-mcp-path-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Main.tz"), "0\n").unwrap();
        std::fs::write(root.join("notes.txt"), "x\n").unwrap();
        crate::cache::real_path(&root).unwrap()
    }

    #[test]
    fn resolve_path_table() {
        let root = root();
        let error = |arg: &str| resolve_path(&root, arg, Wanted::Source).unwrap_err().0;
        assert_eq!(resolve_path(&root, "", Wanted::Source).unwrap(), root);
        assert_eq!(resolve_path(&root, ".", Wanted::Source).unwrap(), root);
        assert_eq!(
            resolve_path(&root, "./src//", Wanted::Source).unwrap(),
            root.join("src")
        );
        assert_eq!(
            resolve_path(&root, "Main.tz", Wanted::Source).unwrap(),
            root.join("Main.tz")
        );
        assert_eq!(error(&"a".repeat(4097)), "path is too long");
        for arg in ["/x", "C:x", "a\\b", "a\0b"] {
            assert_eq!(
                error(arg),
                format!("path must be relative to the project root: '{arg}'"),
                "{arg:?}"
            );
        }
        for arg in ["../x", "a/../b", ".."] {
            assert_eq!(error(arg), format!("path must not contain '..': '{arg}'"));
        }
        assert_eq!(error("missing"), "no such file or directory: 'missing'");
        assert_eq!(
            error("notes.txt"),
            "path is neither a directory nor a .tz, .tt or .tc file: 'notes.txt'"
        );
        assert_eq!(
            resolve_path(&root, "Main.tz", Wanted::Directory)
                .unwrap_err()
                .0,
            "path is not a directory: 'Main.tz'"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("src"), root.join("link")).unwrap();
            assert_eq!(
                error("link/x.tz"),
                "path goes through a symbolic link: 'link/x.tz'"
            );
            std::os::unix::fs::symlink(std::env::temp_dir(), root.join("out")).unwrap();
            assert_eq!(error("out"), "path goes through a symbolic link: 'out'");
        }
    }

    #[test]
    fn scrub_removes_the_root() {
        let root = Path::new("/r");
        assert_eq!(
            scrub("cannot inspect source '/r/Main.tz': no", root),
            "cannot inspect source 'Main.tz': no"
        );
        assert_eq!(scrub("in /r", root), "in .");
        assert_eq!(
            scrub("C:\\r\\x and /r\\y", Path::new("/r")),
            "C:\\r\\x and y"
        );
        assert_eq!(relative_text(root, Path::new("/r/a/b.tz")), "a/b.tz");
        assert_eq!(relative_text(root, Path::new("/r")), ".");
        assert_eq!(relative_text(root, Path::new("/elsewhere")), ".");
    }
}
