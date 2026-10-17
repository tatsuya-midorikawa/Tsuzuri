//! `--emit bitcode` (PR08): the target attributes of the IR, the bitcode of a build, and the hosts that link it.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::driver::Project;
use tsuzuri::driver::lto::{TargetAttributes, probe_target_attributes, stamp_target_attributes};
use tsuzuri::llvm::{Allocator, EmitOptions, Entry, F16Path};

fn clang() -> OsString {
    std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into())
}

/// A new directory for one test, which the test removes when it succeeds.
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-lto-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const INPUT: &str = r#"%tz.string = type { ptr, i64 }
declare void @llvm.trap()
declare void @h() #1

define internal i64 @f(i64 %x) nounwind {
entry:
  ret i64 %x
}
define i64 @tz_f(i64 %x) nounwind !dbg !7 {
entry:
  ret i64 %x
}
define internal void @g(ptr %p) local_unnamed_addr #0 {
entry:
  ret void
}
define internal i32 @"f.cpu.avx2"(i32 %x) nounwind "target-features"="+avx2" {
entry:
  ret i32 %x
}
define internal i32 @bare() {
entry:
  ret i32 0
}
attributes #0 = { nounwind "no-trapping-math"="true" "tune-cpu"="generic" }
attributes #1 = { nounwind }
"#;

#[test]
fn stamps_the_cpu_and_keeps_the_features_of_a_cpu_version() {
    let cpu = TargetAttributes::new("cpu-x", Some("tune-y"), Some("+a,+b"));
    // The default build copies the CPU and its tuning only.
    let stamped = stamp_target_attributes(INPUT, &cpu.without_features()).unwrap();
    assert_eq!(
        stamped,
        r#"%tz.string = type { ptr, i64 }
declare void @llvm.trap()
declare void @h() #1

define internal i64 @f(i64 %x) nounwind #2 {
entry:
  ret i64 %x
}
define i64 @tz_f(i64 %x) nounwind #2 !dbg !7 {
entry:
  ret i64 %x
}
define internal void @g(ptr %p) local_unnamed_addr #0 {
entry:
  ret void
}
define internal i32 @"f.cpu.avx2"(i32 %x) nounwind "target-features"="+avx2" #3 {
entry:
  ret i32 %x
}
define internal i32 @bare() #2 {
entry:
  ret i32 0
}
attributes #0 = { nounwind "no-trapping-math"="true" "target-cpu"="cpu-x" "tune-cpu"="tune-y" }
attributes #1 = { nounwind }
attributes #2 = { "target-cpu"="cpu-x" "tune-cpu"="tune-y" }
attributes #3 = { "target-cpu"="cpu-x" "tune-cpu"="tune-y" }
"#
    );
    // `--cpu native` copies the features too, except where a function names its own.
    let native = stamp_target_attributes(INPUT, &cpu).unwrap();
    assert!(native.contains(
        r#"attributes #0 = { nounwind "no-trapping-math"="true" "target-cpu"="cpu-x" "target-features"="+a,+b" "tune-cpu"="tune-y" }"#
    ));
    assert!(native.contains(
        r#"attributes #2 = { "target-cpu"="cpu-x" "target-features"="+a,+b" "tune-cpu"="tune-y" }"#
    ));
    assert!(
        native.contains(r#"attributes #3 = { "target-cpu"="cpu-x" "tune-cpu"="tune-y" }"#),
        "{native}"
    );
    assert!(native.contains(r#""f.cpu.avx2"(i32 %x) nounwind "target-features"="+avx2" #3 {"#));
    // The same input gives the same output, and a stamped module stays as it is.
    for attributes in [cpu.without_features(), cpu.clone()] {
        let once = stamp_target_attributes(INPUT, &attributes).unwrap();
        assert_eq!(once, stamp_target_attributes(INPUT, &attributes).unwrap());
        assert_eq!(once, stamp_target_attributes(&once, &attributes).unwrap());
    }
}

#[test]
fn stamps_a_module_without_definitions_and_without_a_tuning_cpu() {
    let declarations =
        "declare void @llvm.trap()\ndeclare i32 @f(i32) #0\nattributes #0 = { nounwind }\n";
    let attributes = TargetAttributes::new("x86-64", None, Some("+sse2"));
    assert_eq!(
        stamp_target_attributes(declarations, &attributes).unwrap(),
        declarations
    );
    assert_eq!(
        stamp_target_attributes("define void @f() {\nentry:\n  ret void\n}", &attributes).unwrap(),
        "define void @f() #0 {\nentry:\n  ret void\n}\nattributes #0 = { \"target-cpu\"=\"x86-64\" \"target-features\"=\"+sse2\" }\n"
    );
    // A group that names features of its own keeps them, and a group used with a function that has its own
    // gets none.
    let ir = "define void @a() #0 {\n}\ndefine void @b() #1 \"target-features\"=\"+avx2\" {\n}\nattributes #0 = { \"target-features\"=\"+own\" \"tune-cpu\"=\"old\" }\nattributes #1 = { nounwind }\n";
    assert_eq!(
        stamp_target_attributes(ir, &attributes).unwrap(),
        "define void @a() #0 {\n}\ndefine void @b() #1 \"target-features\"=\"+avx2\" {\n}\nattributes #0 = { \"target-cpu\"=\"x86-64\" \"target-features\"=\"+own\" }\nattributes #1 = { nounwind \"target-cpu\"=\"x86-64\" }\n"
    );
}

#[test]
fn stamps_a_definition_that_follows_a_closing_brace() {
    // The runtime text after a function may end without a newline, so the next `define` shares its line.
    let attributes = TargetAttributes::new("x86-64", None, None);
    assert_eq!(
        stamp_target_attributes(
            "define void @a() nounwind {\n  ret void\n}define internal void @b() nounwind {\n  ret void\n}\n",
            &attributes
        )
        .unwrap(),
        "define void @a() nounwind #0 {\n  ret void\n}define internal void @b() nounwind #0 {\n  ret void\n}\nattributes #0 = { \"target-cpu\"=\"x86-64\" }\n"
    );
}

#[test]
fn refuses_a_definition_it_cannot_stamp() {
    let attributes = TargetAttributes::new("x86-64", None, None);
    for (ir, why) in [
        ("define void @f()\n{\n}\n", "does not end with '{'"),
        ("define void f {\n}\n", "has no parameter list"),
    ] {
        let error = stamp_target_attributes(ir, &attributes).unwrap_err();
        assert_eq!(error.code, "E2002");
        assert!(
            error.message.starts_with(&format!(
                "cannot add target attributes to the LLVM IR: a definition {why}: "
            )),
            "{}",
            error.message
        );
    }
}

/// The groups of the `define` lines of `ir`, each with the text of its attribute list.
fn definition_attributes(ir: &str) -> Vec<(String, String)> {
    let groups: std::collections::BTreeMap<&str, &str> = ir
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("attributes #")?;
            let (number, list) = rest.split_once(" = {")?;
            Some((number, list))
        })
        .collect();
    ir.lines()
        // A definition may share the line of the `}` of the function before it.
        .map(|line| {
            line.strip_prefix('}')
                .filter(|rest| rest.starts_with("define "))
                .unwrap_or(line)
        })
        .filter(|line| line.starts_with("define "))
        .map(|line| {
            let name = line
                .split('@')
                .nth(1)
                .unwrap_or_default()
                .split('(')
                .next()
                .unwrap_or_default();
            let mut list = line.to_owned();
            for part in line.split(" #").skip(1) {
                let number: String = part.chars().take_while(char::is_ascii_digit).collect();
                if let Some(group) = groups.get(number.as_str()) {
                    list.push(' ');
                    list.push_str(group);
                }
            }
            (name.to_owned(), list)
        })
        .collect()
}

/// Checks the IR of a project: Clang reads it once it is stamped, every function has the CPU, and the
/// functions that name features keep them.
fn check_stamped(ir: &str, attributes: &TargetAttributes, directory: &Path, label: &str) {
    let stamped = stamp_target_attributes(ir, attributes)
        .unwrap_or_else(|error| panic!("{label}: {}", error.message));
    let input = directory.join("stamped.ll");
    let output = directory.join("parsed.ll");
    fs::write(&input, &stamped).unwrap();
    let result = Command::new(clang())
        .args([
            "-x",
            "ir",
            "-Wno-override-module",
            "-O0",
            "-Xclang",
            "-disable-llvm-passes",
            "-S",
            "-emit-llvm",
            "-o",
        ])
        .arg(&output)
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let parsed = fs::read_to_string(&output).unwrap();
    let before = definition_attributes(ir);
    let after = definition_attributes(&parsed);
    assert_eq!(before.len(), after.len(), "{label}");
    let cpu = format!("\"target-cpu\"=\"{}\"", attributes.cpu());
    for ((name, was), (_, now)) in before.iter().zip(&after) {
        assert!(now.contains(&cpu), "{label}: {name}: {now}");
        if let Some(at) = was.find("\"target-features\"=\"") {
            let own = &was[at..];
            let own = &own[..own[19..].find('"').unwrap() + 20];
            assert!(now.contains(own), "{label}: {name} lost {own}: {now}");
        }
    }
    assert_eq!(
        stamped,
        stamp_target_attributes(&stamped, attributes).unwrap(),
        "{label}"
    );
}

/// The IR that a native object build of the project at `path` writes, for a CPU that detects `levels`.
fn library_ir(path: &Path, levels: u8) -> Option<String> {
    let project = Project::load(path).ok()?;
    let module = project.analyze().ok()?;
    tsuzuri::llvm::emit_native_build_for(
        &module,
        EmitOptions {
            entry: Entry::Library,
            wasm: false,
            wasm_simd128: false,
            debug_output: false,
            runtime_objects: false,
            allocator: Allocator::System,
            f16: F16Path::Soft,
        },
        &[],
        None,
        false,
        levels,
    )
    .ok()
    .map(|output| output.ir)
}

#[test]
fn stamps_the_ir_of_every_fixture_and_example() {
    let directory = scratch("corpus");
    let generic = TargetAttributes::new("x86-64", Some("generic"), None);
    let native = TargetAttributes::new(
        "skylake-avx512",
        Some("generic"),
        Some("+cmov,+cx8,+fxsr,+mmx,+sse,+sse2,+x87,+avx512f"),
    );
    let mut projects: Vec<PathBuf> = ["examples", "tests/fixtures"]
        .iter()
        .flat_map(|base| fs::read_dir(root().join(base)).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    projects.sort();
    let (mut checked, mut versions) = (0, 0);
    for path in &projects {
        let label = path.file_name().unwrap().to_string_lossy().into_owned();
        // A CPU that detects the x86-64 levels, with the `@cpu` versions that name features of their own (F08).
        let Some(ir) = library_ir(path, 0b1110) else {
            continue;
        };
        let named = |ir: &str| {
            ir.lines()
                .filter(|line| {
                    line.starts_with("define ") && line.contains("\"target-features\"=\"")
                })
                .count()
        };
        versions += named(&ir);
        check_stamped(
            &ir,
            &native,
            &directory,
            &format!("{label} (x86-64, native)"),
        );
        check_stamped(&ir, &generic, &directory, &format!("{label} (x86-64)"));
        checked += 2;
        // AArch64 Linux has its own two levels.
        if named(&ir) > 0 {
            let ir = library_ir(path, 0b11_0000).unwrap();
            versions += named(&ir);
            check_stamped(&ir, &generic, &directory, &format!("{label} (AArch64)"));
            checked += 1;
        }
    }
    assert!(checked >= 200, "only {checked} modules were checked");
    assert!(
        versions >= 50,
        "only {versions} versions that name features"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn probes_the_target_attributes_of_clang() {
    let directory = scratch("probe");
    let flags = [OsString::from("-fPIC")];
    let attributes = probe_target_attributes(&clang(), &flags, &directory).unwrap();
    assert!(!attributes.cpu().is_empty());
    assert!(
        attributes
            .features()
            .is_some_and(|features| features.starts_with('+') || features.starts_with('-'))
    );
    assert_eq!(
        attributes,
        probe_target_attributes(&clang(), &flags, &directory).unwrap()
    );
    // A Clang that is not there is a tool failure.
    let error =
        probe_target_attributes("tsuzuri-no-such-clang".as_ref(), &flags, &directory).unwrap_err();
    assert_eq!(error.code, "E2002");
    assert!(
        error
            .message
            .ends_with("bitcode output requires Clang that emits LLVM IR; set TSUZURI_CLANG")
    );
    fs::remove_dir_all(directory).unwrap();
}
