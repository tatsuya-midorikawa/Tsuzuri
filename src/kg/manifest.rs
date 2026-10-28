//! `manifest.json` of a knowledge graph and its freshness (G27 §3, §4, D-60). The writer lays the
//! file out by hand (2-space indentation, one count, module and file per line); readers parse it
//! with `serde_json`.

use std::collections::BTreeMap;
use std::path::Path;

use super::write::{MANIFEST, MARKER, marker_stamp};
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span, json_string};

/// The schema of a graph's directory.
pub const SCHEMA: &str = "tz-kg/1";

/// The most bytes of `manifest.json` that a reader reads.
pub const MANIFEST_LIMIT: u64 = 16 << 20;

/// `tsuzuri <version>`: the same on every machine.
pub fn compiler() -> String {
    format!("tsuzuri {}", env!("CARGO_PKG_VERSION"))
}

/// One module of the graph: its ID, key, origin, source path and the SHA-256 of that source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleEntry {
    pub name: String,
    pub key: String,
    pub origin: String,
    pub path: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub schema: String,
    pub compiler: String,
    pub edition: String,
    pub project: String,
    pub version: Option<String>,
    pub complete: bool,
    pub source_hash: String,
    /// In the order of §3; a failed analysis has only `errors` and `warnings`.
    pub counts: Vec<(String, u64)>,
    /// By `name`.
    pub modules: Vec<ModuleEntry>,
    /// Every file but `manifest.json` and the per-machine `.`-files, by path, with its SHA-256.
    pub files: BTreeMap<String, String>,
}

impl Manifest {
    pub fn count(&self, key: &str) -> Option<u64> {
        self.counts
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
    }

    pub fn render(&self) -> String {
        let mut lines = vec![
            "{".to_owned(),
            format!("  \"schema\": {},", json_string(&self.schema)),
            format!("  \"compiler\": {},", json_string(&self.compiler)),
            format!("  \"edition\": {},", json_string(&self.edition)),
            format!("  \"project\": {},", json_string(&self.project)),
        ];
        if let Some(version) = &self.version {
            lines.push(format!("  \"version\": {},", json_string(version)));
        }
        lines.push(format!("  \"complete\": {},", self.complete));
        lines.push(format!(
            "  \"source_hash\": {},",
            json_string(&self.source_hash)
        ));
        lines.push("  \"counts\": {".to_owned());
        for (index, (key, value)) in self.counts.iter().enumerate() {
            let comma = if index + 1 < self.counts.len() {
                ","
            } else {
                ""
            };
            lines.push(format!("    {}: {value}{comma}", json_string(key)));
        }
        lines.push("  },".to_owned());
        lines.push("  \"modules\": [".to_owned());
        for (index, module) in self.modules.iter().enumerate() {
            let comma = if index + 1 < self.modules.len() {
                ","
            } else {
                ""
            };
            lines.push(format!(
                "    {{\"name\":{},\"key\":{},\"origin\":{},\"path\":{},\"source\":{}}}{comma}",
                json_string(&module.name),
                json_string(&module.key),
                json_string(&module.origin),
                json_string(&module.path),
                json_string(&module.source)
            ));
        }
        lines.push("  ],".to_owned());
        lines.push("  \"files\": {".to_owned());
        for (index, (path, digest)) in self.files.iter().enumerate() {
            let comma = if index + 1 < self.files.len() {
                ","
            } else {
                ""
            };
            lines.push(format!(
                "    {}: {}{comma}",
                json_string(path),
                json_string(digest)
            ));
        }
        lines.push("  }".to_owned());
        lines.push("}".to_owned());
        lines.join("\n") + "\n"
    }

    pub fn parse(text: &str) -> Result<Manifest, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|error| error.to_string())?;
        let object = value.as_object().ok_or("not an object")?;
        let string = |key: &str| {
            object
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("missing '{key}'"))
        };
        let counts = object
            .get("counts")
            .and_then(serde_json::Value::as_object)
            .ok_or("missing 'counts'")?
            .iter()
            .map(|(key, value)| {
                value
                    .as_u64()
                    .map(|count| (key.clone(), count))
                    .ok_or_else(|| format!("count '{key}' is not a number"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let modules = object
            .get("modules")
            .and_then(serde_json::Value::as_array)
            .ok_or("missing 'modules'")?
            .iter()
            .map(|module| {
                let field = |key: &str| {
                    module
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .ok_or_else(|| format!("module without '{key}'"))
                };
                Ok(ModuleEntry {
                    name: field("name")?,
                    key: field("key")?,
                    origin: field("origin")?,
                    path: field("path")?,
                    source: field("source")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let files = object
            .get("files")
            .and_then(serde_json::Value::as_object)
            .ok_or("missing 'files'")?
            .iter()
            .map(|(path, digest)| {
                if !plain_path(path) {
                    return Err(format!("file '{path}' is not a plain relative path"));
                }
                digest
                    .as_str()
                    .map(|digest| (path.clone(), digest.to_owned()))
                    .ok_or_else(|| format!("file '{path}' has no digest"))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        Ok(Manifest {
            schema: string("schema")?,
            compiler: string("compiler")?,
            edition: string("edition")?,
            project: string("project")?,
            version: object
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            complete: object
                .get("complete")
                .and_then(serde_json::Value::as_bool)
                .ok_or("missing 'complete'")?,
            source_hash: string("source_hash")?,
            counts,
            modules,
            files,
        })
    }
}

/// Whether `path` is a path that a writer lists: `/`-separated names of `[A-Za-z0-9_.%~-]`, none
/// of them empty, `.` or `..`. A reader never leaves the graph's directory.
pub fn plain_path(path: &str) -> bool {
    path.split('/').all(|name| {
        !name.is_empty()
            && name != "."
            && name != ".."
            && name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'%' | b'~' | b'-')
            })
    })
}

/// The state of a graph's directory (G27 §4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Freshness {
    /// No directory.
    Missing,
    /// A directory without a valid marker: not a graph that the compiler wrote.
    NotOurs,
    /// A marker, but `manifest.json` cannot be read or parsed, or names another schema family.
    Broken(String),
    /// Written for other sources, another schema version or another compiler.
    Stale(&'static str),
    /// Up to date, for sources that do not type-check (with the number of errors).
    Incomplete(u64),
    /// A reader's graph was replaced by a writer after the reader opened it.
    Replaced,
    Fresh,
}

/// Reads the manifest of the graph at `dir`, if it has a valid marker and a readable manifest.
pub fn read(dir: &Path) -> Result<Manifest, Freshness> {
    match std::fs::symlink_metadata(dir) {
        Err(_) => return Err(Freshness::Missing),
        Ok(metadata) if !metadata.is_dir() => return Err(Freshness::NotOurs),
        Ok(_) => {}
    }
    if marker_stamp(dir).is_none() {
        return Err(Freshness::NotOurs);
    }
    let bytes = crate::cache::read_regular(&dir.join(MANIFEST), MANIFEST_LIMIT)
        .map_err(|_| Freshness::Broken("manifest.json is not valid".into()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| Freshness::Broken("manifest.json is not valid".into()))?;
    let manifest = Manifest::parse(&text)
        .map_err(|_| Freshness::Broken("manifest.json is not valid".into()))?;
    if !manifest.schema.starts_with("tz-kg/") {
        return Err(Freshness::Broken("manifest.json is not valid".into()));
    }
    Ok(manifest)
}

/// Whether the graph at `dir` was written for `source_hash` by this compiler (`stamp`).
pub fn inspect(dir: &Path, source_hash: &str, stamp: &str) -> Freshness {
    match read(dir) {
        Ok(manifest) => judge(&manifest, marker_stamp(dir).as_deref(), source_hash, stamp),
        Err(freshness) => freshness,
    }
}

/// Whether `manifest`, with the stamp of its marker (`marker`), was written for `source_hash` by
/// this compiler (`stamp`). A reader judges the manifest and the marker of the publication that it
/// read (`store::Snapshot`), since another one can replace the graph after `inspect`.
pub fn judge(
    manifest: &Manifest,
    marker: Option<&str>,
    source_hash: &str,
    stamp: &str,
) -> Freshness {
    if manifest.schema != SCHEMA {
        return Freshness::Stale("built with a different schema");
    }
    if manifest.compiler != compiler() || marker != Some(stamp) {
        return Freshness::Stale("built by a different compiler");
    }
    if manifest.source_hash != source_hash {
        return Freshness::Stale("sources changed");
    }
    if !manifest.complete {
        return Freshness::Incomplete(manifest.count("errors").unwrap_or(0));
    }
    Freshness::Fresh
}

/// `E2009` `kg_missing`: no graph at `dir` (`marker` when a directory without the marker is there).
pub fn missing(dir: &Path, marker: bool) -> Diagnostic {
    let detail = if marker {
        format!(" (no {MARKER} marker)")
    } else {
        String::new()
    };
    Diagnostic::of(
        DiagnosticKind::KgMissing,
        format!(
            "no KG at '{}'{detail}; run tsuzuri check or tsuzuri kg build",
            dir.display()
        ),
        Span::default(),
    )
}

/// `E2009` `kg_stale`.
pub fn stale(dir: &Path, reason: &str) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::KgStale,
        format!(
            "the KG at '{}' is stale ({reason}); run tsuzuri kg build",
            dir.display()
        ),
        Span::default(),
    )
}

/// `E2009` `kg_broken`.
pub fn broken(dir: &Path, reason: &str) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::KgBroken,
        format!(
            "the KG at '{}' is broken ({reason}); run tsuzuri kg build --force",
            dir.display()
        ),
        Span::default(),
    )
}

/// `E2009` `kg_incomplete`: the graph was written for sources that do not type-check.
pub fn incomplete(dir: &Path, errors: u64) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::KgIncomplete,
        format!(
            "the KG at '{}' is incomplete: the project has {errors} errors; fix them and run tsuzuri check",
            dir.display()
        ),
        Span::default(),
    )
}

/// The diagnostic of a graph that is not `Fresh`.
pub fn diagnostic(dir: &Path, freshness: &Freshness) -> Option<Diagnostic> {
    match freshness {
        Freshness::Fresh => None,
        Freshness::Missing => Some(missing(dir, false)),
        Freshness::NotOurs => Some(super::write::not_ours(dir)),
        Freshness::Broken(reason) => Some(broken(dir, reason)),
        Freshness::Stale(reason) => Some(stale(dir, reason)),
        Freshness::Incomplete(errors) => Some(incomplete(dir, *errors)),
        Freshness::Replaced => Some(Diagnostic::of(
            DiagnosticKind::KgStale,
            format!(
                "the KG at '{}' was replaced while it was read; run the command again",
                dir.display()
            ),
            Span::default(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manifest {
        Manifest {
            schema: SCHEMA.into(),
            compiler: "tsuzuri 0.1.0".into(),
            edition: "2026".into(),
            project: "shop".into(),
            version: Some("0.1.0".into()),
            complete: true,
            source_hash: "ab".repeat(32),
            counts: vec![("modules".into(), 3), ("nodes".into(), 14)],
            modules: vec![ModuleEntry {
                name: "Acme::Shop::Cart".into(),
                key: "Cart".into(),
                origin: "project".into(),
                path: "Cart.tz".into(),
                source: "cd".repeat(32),
            }],
            files: BTreeMap::from([("nodes.jsonl".into(), "ef".repeat(32))]),
        }
    }

    #[test]
    fn renders_the_fixed_layout_and_parses_it_back() {
        let manifest = sample();
        let text = manifest.render();
        assert!(
            text.starts_with("{\n  \"schema\": \"tz-kg/1\",\n  \"compiler\": \"tsuzuri 0.1.0\",\n")
        );
        assert!(text.contains("  \"counts\": {\n    \"modules\": 3,\n    \"nodes\": 14\n  },\n"));
        assert!(text.contains("    {\"name\":\"Acme::Shop::Cart\",\"key\":\"Cart\",\"origin\":\"project\",\"path\":\"Cart.tz\",\"source\":\""));
        assert!(text.ends_with("  }\n}\n"));
        let parsed = Manifest::parse(&text).unwrap();
        assert_eq!(parsed, manifest);
        let mut without_version = sample();
        without_version.version = None;
        assert!(!without_version.render().contains("\"version\""));
        assert_eq!(
            Manifest::parse(&without_version.render()).unwrap(),
            without_version
        );
    }

    /// A manifest that lists a path out of the graph's directory is not a manifest.
    #[test]
    fn listed_paths_stay_in_the_directory() {
        for path in [
            "nodes.jsonl",
            "facts/Acme.Shop.Cart.jsonl",
            "brief/L2/Main.b~1.md",
            "brief/L2/test%3AMain.f-0123456789abcdef.md",
        ] {
            assert!(plain_path(path), "{path}");
        }
        for path in [
            "",
            "../secret.jsonl",
            "facts/../../secret.jsonl",
            "/etc/passwd",
            "facts//Main.jsonl",
            "facts/./Main.jsonl",
            "facts\\Main.jsonl",
            "C:secret.jsonl",
            "facts/Main.jsonl/",
        ] {
            assert!(!plain_path(path), "{path}");
            let mut manifest = sample();
            manifest.files.insert(path.into(), "00".repeat(32));
            assert!(Manifest::parse(&manifest.render()).is_err(), "{path}");
        }
    }
}
