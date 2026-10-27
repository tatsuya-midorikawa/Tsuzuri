//! The base revision of `kg impact --git REV` (G28 §5): a detached worktree of the commit in a
//! temporary directory outside the project (a checkout under `target/` would be read as sources),
//! with the hooks off, and removed again whatever happens.

use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use crate::fetch::{failure_line, git_command};

pub struct Checkout {
    temporary: Option<crate::driver::TemporaryDirectory>,
    top: PathBuf,
    checkout: PathBuf,
    project: PathBuf,
    sha: String,
}

fn usage(message: String) -> Diagnostic {
    Diagnostic::of(DiagnosticKind::UsageError, message, Span::default())
}

fn failure(message: String) -> Diagnostic {
    Diagnostic::of(DiagnosticKind::IoFailure, message, Span::default())
}

impl Checkout {
    /// Checks out `rev` of the repository that holds `root`, and finds the same project in it.
    pub fn create(root: &Path, rev: &str) -> Result<Self, Diagnostic> {
        if rev.is_empty() || rev.starts_with('-') {
            return Err(usage(format!("invalid git revision '{rev}'")));
        }
        let output = git_command()
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|error| failure(format!("cannot run git: {error}")))?;
        if !output.status.success() {
            return Err(failure(format!(
                "'{}' is not inside a git repository",
                root.display()
            )));
        }
        let top = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        let output = git_command()
            .arg("-C")
            .arg(&top)
            .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
            .arg(format!("{rev}^{{commit}}"))
            .output()
            .map_err(|error| failure(format!("cannot run git: {error}")))?;
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || sha.len() < 12 {
            return Err(Diagnostic::of(
                DiagnosticKind::UnknownGitRevision,
                format!("unknown git revision '{rev}'"),
                Span::default(),
            ));
        }
        let temporary = crate::driver::TemporaryDirectory::new(&std::env::temp_dir())?;
        let real =
            crate::cache::real_path(&temporary.path).unwrap_or_else(|_| temporary.path.clone());
        let hooks = real.join("no-hooks");
        std::fs::create_dir_all(&hooks)
            .map_err(|error| failure(format!("cannot run git: {error}")))?;
        let checkout = real.join("checkout");
        let output = git_command()
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.display()))
            .arg("-C")
            .arg(&top)
            .args(["worktree", "add", "--detach"])
            .arg(&checkout)
            .arg(&sha)
            .output()
            .map_err(|error| failure(format!("cannot run git: {error}")))?;
        if !output.status.success() {
            return Err(failure(format!(
                "cannot run git: {}",
                failure_line(&output.stderr)
            )));
        }
        let mut made = Self {
            temporary: Some(temporary),
            top: top.clone(),
            checkout: checkout.clone(),
            project: PathBuf::new(),
            sha,
        };
        let real_root = crate::cache::real_path(root).unwrap_or_else(|_| root.to_owned());
        let real_top = crate::cache::real_path(&top).unwrap_or(top);
        let relative = real_root
            .strip_prefix(&real_top)
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let project = checkout.join(&relative);
        if !project.is_dir() {
            return Err(failure(format!(
                "the project directory '{}' does not exist at git revision '{rev}' ({})",
                relative.display(),
                made.sha12()
            )));
        }
        made.project = project;
        Ok(made)
    }

    /// The project's root in the checkout.
    pub fn project_root(&self) -> &Path {
        &self.project
    }

    pub fn sha12(&self) -> &str {
        &self.sha[..12]
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = git_command()
            .arg("-C")
            .arg(&self.top)
            .args(["worktree", "remove", "--force"])
            .arg(&self.checkout)
            .output();
        drop(self.temporary.take());
    }
}
