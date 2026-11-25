use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/range-family")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

#[test]
fn inclusive_slice_end_of_i64u_max_traps() {
    let root = scratch_dir("u64-end");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"u64-end\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "test \"valid_u64_inclusive_end\" =\n\
         \x20   let xs = [10, 20, 30, 40, 50]\n\
         \x20   let all = ref xs[0 ..= 4i64u]\n\
         \x20   assert (all.length == 5 && all[4] == 50)\n",
    )
    .unwrap();
    let ok = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // `u64::MAX` (`18446744073709551615i64u`) must trap rather than wrapping `-1 + 1` to `0`.
    std::fs::write(
        root.join("Main.tz"),
        "export def bad :: [i64] -> i64 = \\xs -> (ref xs[0 ..= 18446744073709551615i64u]).length\n\
         test \"u64_max_traps\" =\n\
         \x20   let xs = [10, 20, 30, 40, 50]\n\
         \x20   let _ = bad xs\n\
         \x20   assert true\n",
    )
    .unwrap();
    let trapped = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !trapped.status.success(),
        "expected u64::MAX inclusive slice end to trap"
    );
    let stdout = String::from_utf8_lossy(&trapped.stdout);
    assert!(stdout.contains("not ok 1"), "{stdout}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn plain_index_of_i64u_keeps_its_ir() {
    let m_u64 = tsuzuri::analyze("def get :: [i64] -> i64u -> i64\nfn get xs i = xs[i]\n").unwrap();
    let m_cast =
        tsuzuri::analyze("def get :: [i64] -> i64u -> i64\nfn get xs i = xs[i as i64]\n").unwrap();
    let ir_u64 = tsuzuri::llvm::emit(&m_u64, tsuzuri::llvm::Entry::Library).unwrap();
    let ir_cast = tsuzuri::llvm::emit(&m_cast, tsuzuri::llvm::Entry::Library).unwrap();
    assert_eq!(ir_u64, ir_cast);
}
