use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SUFFIX: &str = std::env::consts::EXE_SUFFIX;

fn scratch(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("tsuzuri-toolchain-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    // The compiler reports Windows paths without the verbatim `\\?\` prefix.
    let directory = fs::canonicalize(directory).unwrap();
    let plain = directory
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
        .map(PathBuf::from);
    plain.unwrap_or(directory)
}

/// Copies the compiler to `root/bin` with empty bundled tools and, optionally, a manifest.
fn install(root: &Path, manifest: bool) -> PathBuf {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let compiler = bin.join(format!("tsuzuri{SUFFIX}"));
    fs::copy(env!("CARGO_BIN_EXE_tsuzuri"), &compiler).unwrap();
    if manifest {
        fs::write(
            root.join("manifest.json"),
            format!("{{\"id\":\"0123456789abcdef{}\"}}", "0".repeat(48)),
        )
        .unwrap();
    }
    for name in ["tsuzuri-clang", "wasm-ld"] {
        fs::write(bin.join(format!("{name}{SUFFIX}")), "").unwrap();
    }
    compiler
}

/// Runs `toolchain info` (plus `extra` arguments) without tool variables and with an empty `PATH`.
fn info(
    compiler: &Path,
    path: &Path,
    extra: &[&str],
    environment: &[(&str, &Path)],
) -> (i32, String, String) {
    let mut command = Command::new(compiler);
    command
        .args(["toolchain", "info"])
        .args(extra)
        .env("PATH", path);
    for variable in [
        "TSUZURI_CLANG",
        "TSUZURI_WASM_LD",
        "TSUZURI_LLVM_LINK",
        "TSUZURI_DSYMUTIL",
    ] {
        command.env_remove(variable);
    }
    for (name, value) in environment {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

#[test]
fn reports_path_tools_without_distribution() {
    let root = scratch("path");
    let empty = root.join("empty");
    fs::create_dir(&empty).unwrap();
    let (code, stdout, _) = info(Path::new(env!("CARGO_BIN_EXE_tsuzuri")), &empty, &[], &[]);
    assert_eq!(code, 0);
    let lines = lines(&stdout);
    assert_eq!(lines[0], format!("tsuzuri {}", env!("CARGO_PKG_VERSION")));
    assert_eq!(lines[1], "distribution: none");
    assert_eq!(lines[2], "TSUZURI_CLANG: path clang (not found)");
    assert_eq!(lines[6], "node: path node (not found)");
    assert_eq!(lines.len(), 7);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn prefers_bundled_tools_over_path() {
    let root = scratch("bundled");
    let compiler = install(&root.join("dist"), true);
    let path = root.join("path");
    fs::create_dir(&path).unwrap();
    fs::write(path.join(format!("clang{SUFFIX}")), "").unwrap();
    let (code, stdout, _) = info(&compiler, &path, &[], &[]);
    assert_eq!(code, 0);
    let dist = root.join("dist");
    let lines = lines(&stdout);
    assert_eq!(
        lines[1],
        format!("distribution: {} (id 0123456789ab)", dist.display())
    );
    assert_eq!(
        lines[2],
        format!(
            "TSUZURI_CLANG: bundled {} (unavailable)",
            dist.join("bin")
                .join(format!("tsuzuri-clang{SUFFIX}"))
                .display()
        )
    );
    assert_eq!(lines[4], "TSUZURI_LLVM_LINK: path llvm-link (not found)");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn environment_overrides_bundled_tools() {
    let root = scratch("environment");
    let compiler = install(&root.join("dist"), true);
    let fake = root.join("fake-clang");
    fs::write(&fake, "").unwrap();
    let (code, stdout, _) = info(&compiler, &root, &[], &[("TSUZURI_CLANG", &fake)]);
    assert_eq!(code, 0);
    let lines = lines(&stdout);
    assert_eq!(
        lines[2],
        format!("TSUZURI_CLANG: env {} (unavailable)", fake.display())
    );
    assert!(
        lines[3].starts_with("TSUZURI_WASM_LD: bundled "),
        "{}",
        lines[3]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ignores_bundle_without_manifest() {
    let root = scratch("unmarked");
    let compiler = install(&root.join("dist"), false);
    let empty = root.join("empty");
    fs::create_dir(&empty).unwrap();
    let (code, stdout, _) = info(&compiler, &empty, &[], &[]);
    assert_eq!(code, 0);
    let lines = lines(&stdout);
    assert_eq!(lines[1], "distribution: none");
    assert_eq!(lines[2], "TSUZURI_CLANG: path clang (not found)");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn follows_symlinked_compiler() {
    let root = scratch("symlink");
    let compiler = install(&root.join("dist"), true);
    let links = root.join("links");
    fs::create_dir(&links).unwrap();
    std::os::unix::fs::symlink(&compiler, links.join("tsuzuri")).unwrap();
    let (code, stdout, _) = info(&links.join("tsuzuri"), &links, &[], &[]);
    assert_eq!(code, 0);
    assert_eq!(
        lines(&stdout)[2],
        format!(
            "TSUZURI_CLANG: bundled {} (unavailable)",
            root.join("dist/bin/tsuzuri-clang").display()
        )
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn keeps_other_toolchain_arguments_unchanged() {
    let root = scratch("arguments");
    for extra in ["--json", "extra"] {
        let (code, stdout, stderr) = info(
            Path::new(env!("CARGO_BIN_EXE_tsuzuri")),
            &root,
            &[extra],
            &[],
        );
        assert_eq!(code, 2, "{stdout}");
        assert!(stderr.contains("E2000"), "{stderr}");
    }
    fs::remove_dir_all(root).unwrap();
}
