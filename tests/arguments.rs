//! The command-line arguments of `def main :: Array<string> -> i32` (src/runtime/arguments.c).
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-arguments-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn the_runtime_splits_windows_command_lines_and_decodes_utf8() {
    let root = scratch("runtime");
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let executable = root.join(format!("arguments-runtime{}", std::env::consts::EXE_SUFFIX));
    let built = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-O1"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/arguments_runtime.c"
        ))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&executable).output().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn executables_receive_their_command_line_arguments() {
    let root = scratch("program");
    fs::write(
        root.join("Main.tz"),
        "def main :: Array<string> -> i32 = \\args ->\n    let separator = \"][\"\n    do! IO.write_line (\"[\" + String.join (ref separator) (ref args) + \"]\")\n    args.length as i32\n",
    )
    .unwrap();
    for optimization in ["-O0", "-O3"] {
        let executable = root.join(format!(
            "program{optimization}{}",
            std::env::consts::EXE_SUFFIX
        ));
        let built = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("build")
            .arg(&root)
            .arg(optimization)
            .arg("--no-cache")
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        // The value of `main` is the exit code, and the program name is not an argument.
        let run = Command::new(&executable)
            .args(["a", "b c", "", "\u{65e5}\u{672c}"])
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(4));
        assert_eq!(
            String::from_utf8(run.stdout).unwrap(),
            "[a][b c][][\u{65e5}\u{672c}]\n"
        );
        let none = Command::new(&executable).output().unwrap();
        assert_eq!(none.status.code(), Some(0));
        assert_eq!(String::from_utf8(none.stdout).unwrap(), "[]\n");
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let raw = Command::new(&executable)
                .arg(std::ffi::OsStr::from_bytes(b"a\xff\xe0\x80b"))
                .output()
                .unwrap();
            assert_eq!(raw.status.code(), Some(1));
            assert_eq!(
                String::from_utf8(raw.stdout).unwrap(),
                "[a\u{fffd}\u{fffd}\u{fffd}b]\n"
            );
        }
    }
    // `tsuzuri run` passes no arguments and reports a nonzero exit code.
    fs::write(
        root.join("Main.tz"),
        "def main :: unit -> i32 = \\() ->\n    do! IO.write_line \"ran\"\n    3\n",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .arg("run")
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(String::from_utf8(run.stdout).unwrap(), "ran\n");
    let stderr = String::from_utf8(run.stderr).unwrap();
    assert!(
        stderr.contains("E2005") && stderr.contains("program exited with code 3"),
        "{stderr}"
    );
    fs::remove_dir_all(root).unwrap();
}
