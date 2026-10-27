//! The inputs that decide a knowledge graph (G27 §3, §4, D-60): the hash of the project's sources
//! and manifests, and the stamp of the running compiler.

use crate::cache::Sha256;
use crate::check::ModuleOrigin;
use crate::driver::Project;

/// The version of the graph's format; part of the stamp.
pub const KG_FORMAT: u32 = 1;

/// The SHA-256 of the inputs of `project`: its user sources (those of its dependencies included)
/// and its manifests, by package name and relative path. Std is the compiler's, and the stamp
/// covers it; thread counts and warning options are not inputs.
pub fn source_hash(project: &Project) -> [u8; 32] {
    let mut inputs: Vec<(&str, String, &str, &[u8])> = project
        .sources
        .iter()
        .filter(|source| source.origin == ModuleOrigin::User)
        .map(|source| (source, "source"))
        .chain(project.manifests.iter().map(|source| (source, "manifest")))
        .map(|(source, kind)| {
            (
                source
                    .package
                    .as_ref()
                    .map_or("", |package| package.name.as_str()),
                source.relative_path.to_string_lossy().replace('\\', "/"),
                kind,
                source.text.as_bytes(),
            )
        })
        .collect();
    inputs.sort_by(|a, b| (a.0.as_bytes(), a.1.as_bytes()).cmp(&(b.0.as_bytes(), b.1.as_bytes())));
    let mut hash = Sha256::new();
    hash.field("tz-kg-input", &KG_FORMAT.to_le_bytes());
    for (package, path, kind, text) in inputs {
        hash.field("package", package.as_bytes());
        hash.field("path", path.as_bytes());
        hash.field(kind, text);
    }
    hash.finalize()
}

/// The identity of the running compiler and of the graph's format: it differs between two
/// builds of the compiler, so a graph that another build wrote is rebuilt. It differs between
/// machines too, so it lives in the marker, not in `manifest.json`.
pub fn stamp() -> Option<[u8; 32]> {
    let compiler = crate::frontend_cache::compiler_identity()?;
    let mut hash = Sha256::new();
    hash.field("kg-format", &KG_FORMAT.to_le_bytes());
    hash.field("compiler", &compiler);
    Some(hash.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The SHA-256 of `bytes` in lowercase hexadecimal.
pub fn digest(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hex(&hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_match_the_sha256_reference() {
        // `printf 'abc' | shasum -a 256`
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }
}
