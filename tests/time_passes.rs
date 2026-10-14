use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn compiler(arguments: &[&str], enabled: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command
        .args(arguments)
        .env("TSUZURI_CACHE_DIR", "")
        .env_remove("TSUZURI_TIME_PASSES");
    if let Some(value) = enabled {
        command.env("TSUZURI_TIME_PASSES", value);
    }
    command.output().unwrap()
}

fn hello() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hello")
}

fn artifact_bytes(path: &Path, coff: bool) -> Vec<u8> {
    let mut bytes = fs::read(path).unwrap();
    if coff {
        assert!(bytes.len() >= 20);
        assert!(matches!(
            u16::from_le_bytes(bytes[..2].try_into().unwrap()),
            0x14c | 0x8664 | 0xaa64
        ));
        assert_eq!(&bytes[16..18], &[0, 0]);
        let sections = usize::from(u16::from_le_bytes(bytes[2..4].try_into().unwrap()));
        assert!(sections > 0 && 20 + sections * 40 <= bytes.len());
        // The CI byte delta is precisely the native Clang COFF wall-clock timestamp.
        bytes[4..8].fill(0);
    }
    bytes
}

struct Temporary(PathBuf);

impl Temporary {
    fn new(name: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("tsuzuri-time-passes-{name}-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn phases(output: &Output) -> Vec<String> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    let report: serde_json::Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
    assert_eq!(report["time_passes"], 1);
    let phases = report["phases"].as_array().unwrap();
    let mut names = Vec::new();
    let mut sum = 0.0;
    for phase in phases {
        let name = phase["name"].as_str().unwrap();
        assert!(!names.iter().any(|previous| previous == name));
        assert!(
            [
                "load",
                "parse",
                "check",
                "specialize",
                "closures",
                "ownership",
                "emit",
                "cache"
            ]
            .contains(&name)
                || name.starts_with("tool."),
            "{name}"
        );
        let ms = phase["ms"].as_f64().unwrap();
        assert!(ms.is_finite() && ms >= 0.0);
        sum += ms;
        names.push(name.to_owned());
    }
    let total = report["total_ms"].as_f64().unwrap();
    assert!(sum <= total + 0.0005 * (phases.len() + 1) as f64);
    names
}

#[test]
fn time_passes_is_off_by_default() {
    for value in [None, Some("0"), Some("true")] {
        let output = compiler(&["check", hello()], value);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
    }
    let temporary = Temporary::new("missing");
    let missing = temporary.path().join("missing");
    let args = ["check", missing.to_str().unwrap()];
    let plain = compiler(&args, None);
    let timed = compiler(&args, Some("1"));
    assert_eq!(plain.status.code(), timed.status.code());
    assert_eq!(plain.stdout, timed.stdout);
    let stderr = String::from_utf8(timed.stderr).unwrap();
    let report = stderr.lines().last().unwrap();
    assert!(report.starts_with("{\"time_passes\":1,"));
    assert_eq!(
        stderr
            .strip_suffix(&format!("{report}\n"))
            .unwrap()
            .as_bytes(),
        plain.stderr
    );
}

#[test]
fn check_reports_frontend_phases_in_order() {
    assert_eq!(
        phases(&compiler(&["check", hello()], Some("1"))),
        [
            "load",
            "cache",
            "parse",
            "check",
            "specialize",
            "closures",
            "ownership"
        ]
    );
}

#[test]
fn build_reports_backend_phases_and_tools() {
    let temporary = Temporary::new("build");
    let output = temporary.path().join("hello");
    let names = phases(&compiler(
        &[
            "build",
            hello(),
            "-O0",
            "--no-cache",
            "-o",
            output.to_str().unwrap(),
        ],
        Some("1"),
    ));
    for name in ["emit", "cache"] {
        assert!(names.iter().any(|phase| phase == name));
    }
    assert!(names.iter().any(|name| name.starts_with("tool.")));
}

#[test]
fn time_passes_does_not_change_artifacts() {
    let temporary = Temporary::new("artifacts");
    let object_source = temporary.path().join("Object.tz");
    fs::write(&object_source, "export def answer :: i64\nfn answer = 42\n").unwrap();
    for (emit, target, wasi) in [
        ("llvm", "native", false),
        ("header", "native", false),
        ("object", "native", false),
        ("wasm", "wasm32", false),
        ("llvm", "x86_64-pc-windows-msvc", false),
        ("wasm", "wasm32", true),
        ("header", "x86_64-unknown-none", false),
    ] {
        for opt in ["-O0", "-O3"] {
            let output = temporary
                .path()
                .join(format!("{emit}-{target}-wasi-{wasi}-{opt}"));
            let source = if emit == "object" || target == "x86_64-unknown-none" {
                object_source.to_str().unwrap()
            } else {
                hello()
            };
            let args = [
                "build",
                source,
                "--emit",
                emit,
                opt,
                "--no-cache",
                "--target",
                target,
                "-o",
                output.to_str().unwrap(),
            ];
            let mut args = args.to_vec();
            if wasi {
                args.extend(["--wasm-host", "wasi"]);
            }
            if target == "x86_64-unknown-none" {
                args.extend(["--freestanding", "--allocator", "host"]);
            }
            let plain = compiler(&args, None);
            assert!(plain.status.success(), "{:?}", plain);
            let coff = cfg!(windows) && emit == "object" && target == "native";
            let bytes = artifact_bytes(&output, coff);
            let runs = if coff { 3 } else { 1 };
            for run in 0..runs {
                if runs > 1 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
                let enabled = if coff && run == 0 { None } else { Some("1") };
                let timed = compiler(&args, enabled);
                if enabled.is_some() {
                    assert!(phases(&timed).iter().any(|name| name == "emit"));
                } else {
                    assert!(timed.status.success(), "{timed:?}");
                    assert_eq!(plain.stderr, timed.stderr);
                }
                assert_eq!(plain.stdout, timed.stdout);
                let timed_bytes = artifact_bytes(&output, coff);
                if bytes != timed_bytes {
                    let offset = bytes
                        .iter()
                        .zip(&timed_bytes)
                        .position(|(plain, timed)| plain != timed)
                        .unwrap_or(bytes.len().min(timed_bytes.len()));
                    let start = offset.saturating_sub(8);
                    panic!(
                        "{emit} {target} {opt}: byte {offset}, lengths {} / {}; plain {:02x?}; timed {:02x?}",
                        bytes.len(),
                        timed_bytes.len(),
                        &bytes[start..bytes.len().min(offset + 24)],
                        &timed_bytes[start..timed_bytes.len().min(offset + 24)],
                    );
                }
            }
        }
    }
}
