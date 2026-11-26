//! M17: `[macos] min_version` manifest parsing, precedence, and Clang invocation flags.

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::diagnostic::DiagnosticKind;
use tsuzuri::driver::{MacosChoice, Project, effective_macos_version_with};
use tsuzuri::package::parse_manifest;
use tsuzuri::target::{MacosVersion, Platform, Triple};

const PACKAGE: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";

#[test]
fn manifest_min_version_parses() {
    for (raw, expected_str, expected_triple) in [
        ("12", "12.0", "arm64-apple-macosx12.0.0"),
        ("13.0", "13.0", "arm64-apple-macosx13.0.0"),
        ("14.1.2", "14.1.2", "arm64-apple-macosx14.1.2"),
    ] {
        let src = format!("{PACKAGE}[macos]\nmin_version = \"{raw}\"\n");
        let manifest = parse_manifest(&src, 0).expect(raw);
        let (ver, _) = manifest.macos.min_version.expect("min_version set");
        assert_eq!(ver.to_string(), expected_str);
        assert_eq!(ver.clang_triple(), expected_triple);
    }

    for bad in ["11.0", "13.x", "", "13.0.0.1", "13.", ".0"] {
        let src = format!("{PACKAGE}[macos]\nmin_version = \"{bad}\"\n");
        let err = parse_manifest(&src, 0).unwrap_err();
        assert_eq!(err.code, "E0002", "{bad}");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::InvalidMacosSetting.id()),
            "{bad}"
        );
    }

    let unknown = parse_manifest(&format!("{PACKAGE}[macos]\nfoo = \"13.0\"\n"), 0).unwrap_err();
    assert_eq!(unknown.kind, Some(DiagnosticKind::UnknownManifestKey.id()));

    let dup = parse_manifest(
        &format!("{PACKAGE}[macos]\nmin_version = \"13.0\"\nmin_version = \"14.0\"\n"),
        0,
    )
    .unwrap_err();
    assert_eq!(dup.kind, Some(DiagnosticKind::DuplicateManifestKey.id()));

    let two_sections = parse_manifest(
        &format!("{PACKAGE}[macos]\nmin_version = \"13.0\"\n[macos]\n"),
        0,
    )
    .unwrap_err();
    assert_eq!(
        two_sections.kind,
        Some(DiagnosticKind::ManifestSectionOrder.id())
    );
    assert!(
        two_sections.message.contains("[macos]"),
        "{}",
        two_sections.message
    );
}

#[test]
fn dependency_sections_are_not_read() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("tsuzuri-m17-dep-{}-{stamp}", std::process::id()));
    fs::create_dir_all(root.join("app")).unwrap();
    fs::create_dir_all(root.join("dep")).unwrap();
    fs::write(
        root.join("app/Tsuzuri.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\ndep = { path = \"../dep\" }\n",
    )
    .unwrap();
    fs::write(
        root.join("app/Main.tz"),
        "def main :: unit -> i32 = \\() -> Dep::Lib.val ()\n",
    )
    .unwrap();
    fs::write(
        root.join("dep/Tsuzuri.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n[macos]\nmin_version = \"99.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("dep/Lib.tz"),
        "def val :: unit -> i32 = \\() -> 0i32\n",
    )
    .unwrap();

    let project = Project::load(&root.join("app")).expect("project loads");
    assert_eq!(project.macos.min_version, None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn effective_version_resolution() {
    let named_macos = Platform::new(Some(Triple::Aarch64AppleDarwin));
    let linux = Platform::new(Some(Triple::X86_64LinuxGnu));
    let v13 = MacosVersion::parse("13.0").unwrap();

    // Non-Mach-O always resolves to None.
    assert_eq!(
        effective_macos_version_with(linux, Some(v13), Some(OsStr::new("banana"))).unwrap(),
        MacosChoice::None
    );

    // 1. Manifest overrides environment.
    assert_eq!(
        effective_macos_version_with(named_macos, Some(v13), Some(OsStr::new("14.2"))).unwrap(),
        MacosChoice::Explicit(v13)
    );

    // 2. Environment applies when manifest is unset, and validates >= 12.0 on named target.
    assert_eq!(
        effective_macos_version_with(named_macos, None, Some(OsStr::new("14.2"))).unwrap(),
        MacosChoice::FromEnvironment(Some(MacosVersion::parse("14.2").unwrap()))
    );
    for bad_env in ["banana", "11.0"] {
        let err =
            effective_macos_version_with(named_macos, None, Some(OsStr::new(bad_env))).unwrap_err();
        assert_eq!(err.code, "E2000");
        assert_eq!(
            err.kind,
            Some(DiagnosticKind::InvalidMacosDeploymentTarget.id())
        );
    }

    // 3. Default 12.0 when neither is set or environment is empty.
    assert_eq!(
        effective_macos_version_with(named_macos, None, None).unwrap(),
        MacosChoice::Default
    );
    assert_eq!(
        effective_macos_version_with(named_macos, None, Some(OsStr::new(""))).unwrap(),
        MacosChoice::Default
    );
}

#[test]
fn effective_version_reaches_clang() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root: PathBuf =
        std::env::temp_dir().join(format!("tsuzuri-m17-clang-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();

    let fake_src = root.join("fake_clang.rs");
    let fake_bin = root.join(if cfg!(windows) {
        "fake-clang.exe"
    } else {
        "fake-clang"
    });
    fs::write(
        &fake_src,
        r#"
use std::io::Write;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("clang version 22.0.0-fake");
        return;
    }
    if let Ok(log) = std::env::var("FAKE_CLANG_LOG") {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap();
        writeln!(f, "clang {}", args.join(" ")).unwrap();
    }
    if let Some(pos) = args.iter().position(|a| a == "-o") {
        if let Some(out) = args.get(pos + 1) {
            std::fs::write(out, b"fake-out").unwrap();
        }
    }
}
"#,
    )
    .unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    assert!(
        Command::new(&rustc)
            .arg(&fake_src)
            .arg("-o")
            .arg(&fake_bin)
            .status()
            .unwrap()
            .success()
    );

    let proj = root.join("proj");
    fs::create_dir_all(&proj).unwrap();
    fs::write(
        proj.join("Main.tz"),
        "def main :: unit -> i32 = \\() -> (40.0 + 2.0) as i32\n",
    )
    .unwrap();
    fs::write(
        proj.join("Tsuzuri.toml"),
        "[package]\nname = \"proj\"\nversion = \"0.1.0\"\n[macos]\nmin_version = \"13.0\"\n",
    )
    .unwrap();

    let log_file = root.join("clang.log");
    let cache_dir = root.join("cache");
    let build_for = |target: &str, out_name: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args([
                "build",
                "--target",
                target,
                proj.to_str().unwrap(),
                "-o",
                root.join(out_name).to_str().unwrap(),
            ])
            .env("TSUZURI_CLANG", &fake_bin)
            .env("TSUZURI_CACHE_DIR", &cache_dir)
            .env("FAKE_CLANG_LOG", &log_file)
            .env_remove("MACOSX_DEPLOYMENT_TARGET")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    build_for("aarch64-apple-darwin", "mac13");
    let mac13_log = fs::read_to_string(&log_file).unwrap();
    assert!(
        mac13_log
            .lines()
            .all(|line| line.contains("--target=arm64-apple-macosx13.0.0")),
        "{mac13_log}"
    );

    // Changing `min_version` invalidates both build and runtime object caches and passes 14.2.0.
    let count_13 = mac13_log.lines().count();
    fs::write(
        proj.join("Tsuzuri.toml"),
        "[package]\nname = \"proj\"\nversion = \"0.1.0\"\n[macos]\nmin_version = \"14.2\"\n",
    )
    .unwrap();
    build_for("aarch64-apple-darwin", "mac14");
    let mac14_log = fs::read_to_string(&log_file).unwrap();
    assert!(mac14_log.lines().count() > count_13, "{mac14_log}");
    assert!(
        mac14_log
            .lines()
            .skip(count_13)
            .all(|line| line.contains("--target=arm64-apple-macosx14.2.0")),
        "{mac14_log}"
    );

    // Linux target ignores `[macos]` completely.
    let before_linux = mac14_log.lines().count();
    build_for("x86_64-unknown-linux-gnu", "linux");
    let linux_log = fs::read_to_string(&log_file).unwrap();
    for line in linux_log.lines().skip(before_linux) {
        assert!(!line.contains("macosx"), "{line}");
    }

    let _ = fs::remove_dir_all(root);
}
