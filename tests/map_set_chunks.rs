//! Table-model property tests for chunked ordered containers, at both optimization levels.

use std::process::Command;

#[test]
fn property_tests_pass_at_o0_and_o3() {
    for level in ["-O0", "-O3"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["test", "tests/fixtures/map_set_chunks", level])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{level}\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("5 passed; 0 failed; 0 ignored"),
            "{level}\n{stdout}"
        );
    }
}
