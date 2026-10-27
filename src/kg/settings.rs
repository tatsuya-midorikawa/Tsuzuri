//! Whether `check`, `build`, `run` and `test` write the knowledge graph, and where (G27 §1, D-60):
//! the flags `--kg`, `--no-kg` and `--kg-dir`, then the environment (`TSUZURI_KG`,
//! `TSUZURI_KG_DIR`), then the default: a project writes its graph to `<root>/target/tz/kg`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use crate::driver::Project;

/// The KG flags of one command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KgFlags {
    pub kg: bool,
    pub no_kg: bool,
    pub dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KgMode {
    /// Write the graph of a project, not of a loose file.
    Default,
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KgSettings {
    pub mode: KgMode,
    pub dir: Option<PathBuf>,
}

fn usage(message: &str) -> Diagnostic {
    Diagnostic::of(DiagnosticKind::UsageError, message, Span::default())
}

/// The settings of the flags and the values of `TSUZURI_KG` and `TSUZURI_KG_DIR` (`E2000` for
/// both `--kg` and `--no-kg`, or a `TSUZURI_KG` other than `0`, `1` or empty).
pub fn resolve(
    flags: &KgFlags,
    kg_env: Option<&OsStr>,
    dir_env: Option<&OsStr>,
) -> Result<KgSettings, Diagnostic> {
    if flags.kg && flags.no_kg {
        return Err(usage("--kg and --no-kg cannot be used together"));
    }
    let environment = match kg_env.map(|value| value.to_str()) {
        None => None,
        Some(Some("")) => None,
        Some(Some("0")) => Some(KgMode::Off),
        Some(Some("1")) => Some(KgMode::On),
        Some(_) => {
            return Err(Diagnostic::of(
                DiagnosticKind::EnvironmentSettingInvalid,
                "TSUZURI_KG must be 0 or 1",
                Span::default(),
            ));
        }
    };
    let mode = if flags.kg {
        KgMode::On
    } else if flags.no_kg {
        KgMode::Off
    } else {
        environment.unwrap_or(KgMode::Default)
    };
    let dir = flags
        .dir
        .clone()
        .or_else(|| dir_env.filter(|value| !value.is_empty()).map(PathBuf::from));
    Ok(KgSettings { mode, dir })
}

/// Whether the root of `project` is a project: it has a manifest, or a regular `Main.tz`.
pub fn is_project(project: &Project) -> bool {
    project.root_package.is_some()
        || project.root_directory().is_some_and(|root| {
            std::fs::symlink_metadata(root.join("Main.tz")).is_ok_and(|metadata| metadata.is_file())
        })
}

impl KgSettings {
    /// The settings of a command that always writes the graph (`tsuzuri kg build`).
    pub fn explicit(dir: Option<PathBuf>) -> Self {
        Self {
            mode: KgMode::On,
            dir,
        }
    }

    /// Whether the command writes the graph of `project` (G27 §1, the decision table).
    pub fn wants(&self, project: &Project) -> bool {
        match self.mode {
            KgMode::On => true,
            KgMode::Off => false,
            KgMode::Default => is_project(project),
        }
    }

    /// Where the graph of `project` goes: `--kg-dir`, `TSUZURI_KG_DIR`, or `<root>/target/tz/kg`.
    pub fn directory(&self, project: &Project) -> PathBuf {
        match &self.dir {
            Some(dir) => dir.clone(),
            None => {
                // The root as the user wrote it: relative to the current directory when it is in it.
                let root = project
                    .root_directory()
                    .unwrap_or_else(|| PathBuf::from("."));
                let shown = std::env::current_dir()
                    .ok()
                    .and_then(|cwd| {
                        let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
                        let real = std::fs::canonicalize(&root).ok()?;
                        real.strip_prefix(&cwd).ok().map(Path::to_path_buf)
                    })
                    .unwrap_or(root);
                if shown.as_os_str().is_empty() || shown == Path::new(".") {
                    default_directory().to_owned()
                } else {
                    shown.join(default_directory())
                }
            }
        }
    }
}

/// `target/tz/kg`, relative to a project's root.
pub fn default_directory() -> &'static Path {
    Path::new("target/tz/kg")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(kg: bool, no_kg: bool) -> KgFlags {
        KgFlags {
            kg,
            no_kg,
            dir: None,
        }
    }

    #[test]
    fn flags_come_before_the_environment() {
        let on = Some(OsStr::new("1"));
        let off = Some(OsStr::new("0"));
        assert_eq!(
            resolve(&flags(true, false), off, None).unwrap().mode,
            KgMode::On
        );
        assert_eq!(
            resolve(&flags(false, true), on, None).unwrap().mode,
            KgMode::Off
        );
        assert_eq!(
            resolve(&flags(false, false), on, None).unwrap().mode,
            KgMode::On
        );
        assert_eq!(
            resolve(&flags(false, false), off, None).unwrap().mode,
            KgMode::Off
        );
        assert_eq!(
            resolve(&flags(false, false), Some(OsStr::new("")), None)
                .unwrap()
                .mode,
            KgMode::Default
        );
        assert_eq!(
            resolve(&flags(false, false), None, None).unwrap().mode,
            KgMode::Default
        );
        let both = resolve(&flags(true, true), None, None).unwrap_err();
        assert_eq!(both.code, "E2000");
        assert_eq!(both.message, "--kg and --no-kg cannot be used together");
        let bad = resolve(&flags(false, false), Some(OsStr::new("2")), None).unwrap_err();
        assert_eq!(bad.code, "E2000");
        assert_eq!(bad.message, "TSUZURI_KG must be 0 or 1");
    }

    #[test]
    fn the_flag_directory_comes_before_the_environment() {
        let mut with_dir = flags(false, false);
        with_dir.dir = Some(PathBuf::from("flag"));
        let chosen = resolve(&with_dir, None, Some(OsStr::new("env"))).unwrap();
        assert_eq!(chosen.dir, Some(PathBuf::from("flag")));
        let chosen = resolve(&flags(false, false), None, Some(OsStr::new("env"))).unwrap();
        assert_eq!(chosen.dir, Some(PathBuf::from("env")));
        let chosen = resolve(&flags(false, false), None, Some(OsStr::new(""))).unwrap();
        assert_eq!(chosen.dir, None);
    }
}
