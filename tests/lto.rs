//! `--emit bitcode` (PR08): the target attributes of the IR, the bitcode of a build, and the hosts that link it.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::collections::BTreeSet;
#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::sync::OnceLock;

use tsuzuri::diagnostic::Diagnostic;
use tsuzuri::driver::lto::{TargetAttributes, stamp_target_attributes};
#[cfg(unix)]
use tsuzuri::driver::lto::{is_bitcode, llvm_link_tool, probe_target_attributes};
use tsuzuri::driver::{self, BuildOptions, Emit, Project};
#[cfg(unix)]
use tsuzuri::driver::{Cpu, OptLevel};
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

const ADD: &str = "export def add :: i64 -> i64 -> i64\nfn add x y = x + y\n";

/// Exports that take a slice and return an array, so the program allocates.
#[cfg(unix)]
const BUFFERS: &str = r#"export def add :: i64 -> i64 -> i64
fn add x y = x + y

export def sum :: ref [i64] -> i64
fn sum values = Array.sum values

export def bytes :: i64 -> [ubyte]
fn bytes count = new [ubyte](count, index -> index as ubyte)
"#;

/// Division traps on zero, which `--trap-mode return` hands to the host (E14).
#[cfg(unix)]
const TRAPS: &str = r#"export def div :: i64 -> i64 -> i64
fn div a b = a / b

export def sum_to :: i64 -> i64
fn sum_to count =
    let values = new [i64](count, \i -> i)
    Array.sum (ref values)
"#;

/// A program that waits on the clock of the reactor (`tsuzuri_async_post` is its host entry) and one that the host
/// drives (`tsuzuri_async_poll` and `tsuzuri_async_complete`), without a host function of its own.
#[cfg(unix)]
const ASYNC: &str = r#"def span :: i64 -> Async<i64>
fn span delay = Async {
    let! start = Async.now ()
    do! Async.sleep delay
    let! finish = Async.now ()
    return finish - start
}

export def timers :: i64 -> i64
fn timers delay =
    let! spans = Async.block_on (Async.all [span delay, span (delay * 2)])
    if spans[0] >= delay && spans[1] >= delay * 2 then 1 else 0

export def started :: i64 -> unit
fn started n =
    do! Async.start (Async {
        do! Async.sleep n
        ()
    })
"#;

fn write(directory: &Path, name: &str, text: &str) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, text).unwrap();
    path
}

/// Builds the project at `source` into `output` through the library, as `tsuzuri build` does.
fn build(source: &Path, output: &Path, options: BuildOptions) -> Result<(), Diagnostic> {
    let project = Project::load(source).unwrap();
    let module = project.analyze().unwrap();
    driver::build(&module, &project, output, options).map(drop)
}

fn bitcode_options() -> BuildOptions {
    BuildOptions {
        emit: Emit::Bitcode,
        cache: false,
        ..BuildOptions::default()
    }
}

#[test]
#[cfg(windows)]
fn windows_builds_no_bitcode_yet() {
    let directory = scratch("windows");
    let source = write(&directory, "Main.tz", ADD);
    let error = build(&source, &directory.join("add.bc"), bitcode_options()).unwrap_err();
    assert_eq!(error.code, "E2000");
    assert!(
        error.message.contains("not supported on Windows"),
        "{}",
        error.message
    );
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
const ADD_TOTAL: &str = "4999950000\n";

#[cfg(unix)]
const ADD_HOST: &str = r#"#include <stdint.h>
#include <stdio.h>

int64_t tz_add(int64_t x, int64_t y);

int main(void) {
    int64_t total = 0;
    for (int64_t i = 0; i < 100000; i++) total = tz_add(total, i);
    printf("%lld\n", (long long)total);
    return 0;
}
"#;

#[cfg(unix)]
const ADD_HOST_CPP: &str = r#"#include <cstdint>
#include <cstdio>

extern "C" std::int64_t tz_add(std::int64_t x, std::int64_t y);

int main() {
    std::int64_t total = 0;
    for (std::int64_t i = 0; i < 100000; i++) total = tz_add(total, i);
    std::printf("%lld\n", static_cast<long long>(total));
    return 0;
}
"#;

#[cfg(unix)]
const TASKS_HOST: &str = r#"#include <stdint.h>
#include <stdio.h>

int64_t tz_task_parallel(void);
int64_t tz_parallel_sum(int64_t count);
int64_t tz_sequence(int64_t n);

int main(void) {
    printf("%lld %lld %lld\n", (long long)tz_task_parallel(), (long long)tz_parallel_sum(10), (long long)tz_sequence(5));
    return 0;
}
"#;

/// Allocates and frees arrays through the program's own heap functions; `-DCOUNTING` also reads the counts of
/// `--allocator counting`.
#[cfg(unix)]
const BUFFERS_HOST: &str = r#"#include <stdint.h>
#include <stdio.h>

typedef struct { uint8_t *ptr; int64_t len; } bytes_t;
int64_t tz_add(int64_t x, int64_t y);
int64_t tz_sum(const int64_t *values, int64_t length);
void tz_bytes(bytes_t *out, int64_t count);
void tsuzuri_free(void *ptr);
#ifdef COUNTING
typedef struct { uint64_t allocations, frees, live_bytes, peak_bytes; } stats_t;
void tsuzuri_alloc_stats(stats_t *stats);
#endif

int main(void) {
    int checked = 0;
    for (int round = 0; round < 100; round++) {
        bytes_t bytes;
        tz_bytes(&bytes, 257);
        checked += bytes.len == 257 && bytes.ptr[255] == 255 && bytes.ptr[256] == 0;
        tsuzuri_free(bytes.ptr);
    }
    int64_t values[] = {1, 2, 3};
    printf("%d %lld %lld\n", checked, (long long)tz_add(40, 2), (long long)tz_sum(values, 3));
#ifdef COUNTING
    stats_t stats;
    tsuzuri_alloc_stats(&stats);
    printf("%d %llu\n", stats.allocations >= 100 && stats.allocations == stats.frees, (unsigned long long)stats.live_bytes);
#endif
    return 0;
}
"#;

/// The host of `--allocator host` owns every block of the program and sees each one freed.
#[cfg(unix)]
const HOST_ALLOCATOR_HOST: &str = r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct { uint8_t *ptr; int64_t len; } bytes_t;
void tz_bytes(bytes_t *out, int64_t count);
void tsuzuri_free(void *ptr);

static uint64_t live, allocations;
void *tsuzuri_host_alloc(uint64_t size, uint64_t align) { (void)align; live += size; allocations++; return malloc(size); }
void tsuzuri_host_free(void *ptr, uint64_t size, uint64_t align) { (void)align; live -= size; free(ptr); }
void *tsuzuri_host_realloc(void *ptr, uint64_t old_size, uint64_t new_size, uint64_t align) { (void)align; live += new_size; live -= old_size; return realloc(ptr, new_size); }

int main(void) {
    int checked = 0;
    for (int round = 0; round < 100; round++) {
        bytes_t bytes;
        tz_bytes(&bytes, 257);
        checked += bytes.len == 257 && bytes.ptr[255] == 255 && bytes.ptr[256] == 0;
        tsuzuri_free(bytes.ptr);
    }
    printf("%d %d %llu\n", checked, allocations >= 100, (unsigned long long)live);
    return 0;
}
"#;

#[cfg(unix)]
const TRAPS_HOST: &str = r#"#include <stdint.h>
#include <stdio.h>

typedef struct { uint32_t site; uint32_t kind; } trap_info;
int64_t tz_div(int64_t a, int64_t b);
int32_t tsuzuri_try_div(trap_info *trap, int64_t *result, int64_t a, int64_t b);
int64_t tz_sum_to(int64_t count);

int main(void) {
    trap_info trap;
    int64_t result = 0;
    int ok = tsuzuri_try_div(&trap, &result, 84, 2);
    int64_t quotient = result;
    int trapped = tsuzuri_try_div(&trap, &result, 1, 0);
    printf("%d %lld %d %lld %lld\n", ok, (long long)quotient, trapped != 0, (long long)tz_div(9, 3), (long long)tz_sum_to(100));
    return 0;
}
"#;

/// The host of `tests/fixtures/async_reactor`: each operation that the program starts completes 20 ms later, on a thread of
/// the host's own, through `tsuzuri_async_post`, which only the C runtime defines.
#[cfg(unix)]
const REACTOR_HOST: &str = r#"#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int32_t tsuzuri_async_post(int64_t operation, int64_t value);
int64_t tz_completed(int64_t n);

struct job { int64_t operation, value; };

static void *complete_later(void *argument) {
    struct job job = *(struct job *)argument;
    free(argument);
    usleep(20000);
    if (tsuzuri_async_post(job.operation, job.value) != 1) abort();
    return NULL;
}

void tsuzuri_host_Main_start_operation(int64_t operation, int64_t request) {
    struct job *job = malloc(sizeof *job);
    pthread_t thread;
    if (!job) abort();
    job->operation = operation;
    job->value = request * 10;
    if (pthread_create(&thread, NULL, complete_later, job) != 0) abort();
    pthread_detach(thread);
}

void tsuzuri_host_Main_cancel_operation(int64_t operation) { (void)operation; }
void tsuzuri_host_Main_report(int64_t tag, int64_t value) { (void)tag; (void)value; }

int main(void) {
    printf("%lld %d\n", (long long)tz_completed(4), (int)tsuzuri_async_post(5, 0));
    return 0;
}
"#;

/// Whether `tool` starts and answers `--version`.
#[cfg(unix)]
fn runs(tool: &OsStr) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A test that needs `reason` skips on a developer's machine and fails on CI, which installs every tool.
#[cfg(unix)]
fn unavailable(reason: &str) {
    assert!(std::env::var_os("CI").is_none(), "{reason}");
    eprintln!("skipped: {reason}");
}

/// A tool of the LLVM that `llvm-link` comes from: the one beside it, else the one on `PATH`.
#[cfg(unix)]
fn llvm_tool(name: &str) -> Option<OsString> {
    let link = llvm_link_tool();
    let beside = Path::new(&link)
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(|directory| directory.join(name).into_os_string());
    let found = beside
        .into_iter()
        .chain([OsString::from(name)])
        .find(|tool| runs(tool));
    if found.is_none() {
        unavailable(&format!(
            "{name} was not found: put the tools of the LLVM that Clang comes from next to Clang or on PATH"
        ));
    }
    found
}

/// The flags that make a link read bitcode: ThinLTO, and LLD on Linux, because GNU ld cannot read it.
#[cfg(unix)]
fn lto_flags() -> Vec<&'static str> {
    let mut flags = vec!["-O3", "-flto=thin"];
    if cfg!(target_os = "linux") {
        flags.push("-fuse-ld=lld");
    }
    flags
}

/// Whether the Clang of the build links with ThinLTO here; it is asked once.
#[cfg(unix)]
fn thinlto_link_works() -> bool {
    static WORKS: OnceLock<Result<(), String>> = OnceLock::new();
    let outcome = WORKS.get_or_init(|| {
        let directory = scratch("thinlto-probe");
        let source = write(&directory, "probe.c", "int main(void) { return 0; }\n");
        let result = Command::new(clang())
            .args(lto_flags())
            .arg(&source)
            .arg("-o")
            .arg(directory.join("probe"))
            .output();
        let outcome = match result {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(String::from_utf8_lossy(&output.stderr).into_owned()),
            Err(error) => Err(error.to_string()),
        };
        let _ = fs::remove_dir_all(directory);
        outcome
    });
    if let Err(reason) = outcome {
        unavailable(&format!("this Clang cannot link with -flto=thin: {reason}"));
    }
    outcome.is_ok()
}

/// Links `inputs` into `directory/name` with `compiler`, which must be the Clang that built them.
#[cfg(unix)]
fn link(
    compiler: &OsStr,
    directory: &Path,
    name: &str,
    inputs: &[&Path],
    lto: bool,
    extra: &[&str],
) -> PathBuf {
    let program = directory.join(name);
    let mut command = Command::new(compiler);
    if lto {
        command.args(lto_flags());
    } else {
        command.arg("-O3");
    }
    let result = command
        .args(inputs)
        .arg("-o")
        .arg(&program)
        .args(extra)
        .arg("-lm")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    program
}

#[cfg(unix)]
fn run(program: &Path) -> String {
    let output = Command::new(program).output().unwrap();
    assert!(
        output.status.success(),
        "{}: {} {}",
        program.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The symbols of a bitcode file, object, or executable as `(type, name)`, without the underscore of Mach-O.
#[cfg(unix)]
fn symbols(nm: &OsStr, file: &Path) -> Vec<(char, String)> {
    let output = Command::new(nm).arg(file).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().rev();
            let name = fields.next()?;
            let kind = fields.next()?.chars().next()?;
            let name = if cfg!(target_vendor = "apple") {
                name.strip_prefix('_').unwrap_or(name)
            } else {
                name
            };
            Some((kind, name.to_owned()))
        })
        .collect()
}

/// The names that `file` defines with the given symbol types.
#[cfg(unix)]
fn defined(nm: &OsStr, file: &Path, kinds: &str) -> BTreeSet<String> {
    symbols(nm, file)
        .into_iter()
        .filter(|(kind, _)| kinds.contains(*kind))
        .map(|(_, name)| name)
        .collect()
}

/// The names that `file` defines with external linkage, of any kind (strong, weak, in the code or in the data).
#[cfg(unix)]
fn externals(nm: &OsStr, file: &Path) -> BTreeSet<String> {
    symbols(nm, file)
        .into_iter()
        .filter(|(kind, _)| kind.is_ascii_uppercase() && *kind != 'U')
        .map(|(_, name)| name)
        .collect()
}

#[test]
#[cfg(unix)]
fn a_c_host_inlines_the_bitcode_but_not_an_object() {
    let Some(nm) = llvm_tool("llvm-nm") else {
        return;
    };
    if !thinlto_link_works() {
        return;
    }
    let directory = scratch("inline");
    let source = write(&directory, "Main.tz", ADD);
    let (bitcode, object) = (directory.join("add.bc"), directory.join("add.o"));
    build(&source, &bitcode, bitcode_options()).unwrap();
    build(
        &source,
        &object,
        BuildOptions {
            emit: Emit::Object,
            ..bitcode_options()
        },
    )
    .unwrap();
    assert!(is_bitcode(&fs::read(&bitcode).unwrap()));
    assert!(!is_bitcode(&fs::read(&object).unwrap()));
    let host = write(&directory, "host.c", ADD_HOST);
    let called = link(
        &clang(),
        &directory,
        "called",
        &[&host, &object],
        false,
        &[],
    );
    let inlined = link(
        &clang(),
        &directory,
        "inlined",
        &[&host, &bitcode],
        true,
        &[],
    );
    assert_eq!(run(&called), ADD_TOTAL);
    assert_eq!(run(&inlined), ADD_TOTAL);
    // The host's loop calls `tz_add` through the object, and has no call left after ThinLTO imported it.
    assert!(defined(&nm, &called, "Tt").contains("tz_add"));
    assert!(
        !symbols(&nm, &inlined)
            .iter()
            .any(|(_, name)| name == "tz_add"),
        "{:?}",
        symbols(&nm, &inlined)
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn a_cpp_host_inlines_the_bitcode_too() {
    let Some(nm) = llvm_tool("llvm-nm") else {
        return;
    };
    if !thinlto_link_works() {
        return;
    }
    let clang = clang();
    let compiler = Path::new(&clang).with_file_name("clang++").into_os_string();
    if !runs(&compiler) {
        unavailable(&format!("{} is not installed", compiler.to_string_lossy()));
        return;
    }
    let directory = scratch("cpp");
    let source = write(&directory, "Main.tz", ADD);
    let bitcode = directory.join("add.bc");
    build(&source, &bitcode, bitcode_options()).unwrap();
    let host = write(&directory, "host.cpp", ADD_HOST_CPP);
    let inlined = link(
        &compiler,
        &directory,
        "inlined",
        &[&host, &bitcode],
        true,
        &[],
    );
    assert_eq!(run(&inlined), ADD_TOTAL);
    assert!(
        !symbols(&nm, &inlined)
            .iter()
            .any(|(_, name)| name == "tz_add")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn the_c_runtime_is_merged_into_the_bitcode_and_hidden() {
    let (Some(nm), Some(_)) = (llvm_tool("llvm-nm"), llvm_tool("llvm-link")) else {
        return;
    };
    if !thinlto_link_works() {
        return;
    }
    let directory = scratch("runtime");
    let source = root().join("tests/fixtures/tasks/Main.tz");
    let (bitcode, object) = (directory.join("tasks.bc"), directory.join("tasks.o"));
    build(&source, &bitcode, bitcode_options()).unwrap();
    build(
        &source,
        &object,
        BuildOptions {
            emit: Emit::Object,
            ..bitcode_options()
        },
    )
    .unwrap();
    // The program's exports are the only strong symbols: the tasks, the CPU detection, and the rest of the C
    // runtime are inside, and a second library built the same way would not collide with them.
    let module = Project::load(&source).unwrap().analyze().unwrap();
    let exports: BTreeSet<String> = module
        .functions
        .iter()
        .filter(|function| function.exported)
        .map(|function| format!("tz_{}", function.name))
        .collect();
    assert!(exports.len() >= 30, "{exports:?}");
    assert_eq!(defined(&nm, &bitcode, "TDBR"), exports);
    let weak = defined(&nm, &bitcode, "WV");
    assert!(
        weak.iter()
            .all(|name| name.starts_with("tz_soft_") || name.starts_with("tsuzuri_")),
        "{weak:?}"
    );
    let host = write(&directory, "host.c", TASKS_HOST);
    let linked = link(
        &clang(),
        &directory,
        "from-object",
        &[&host, &object],
        false,
        &["-pthread"],
    );
    let inlined = link(
        &clang(),
        &directory,
        "from-bitcode",
        &[&host, &bitcode],
        true,
        &["-pthread"],
    );
    assert_eq!(run(&linked), "42 285 11\n");
    assert_eq!(run(&inlined), "42 285 11\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn cpu_kernels_agree_through_bitcode_and_an_object() {
    if llvm_tool("llvm-link").is_none() || !thinlto_link_works() {
        return;
    }
    let directory = scratch("kernels");
    let source = root().join("tests/fixtures/cpu_kernels/Main.tz");
    let exports: Vec<String> = fs::read_to_string(&source)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("export def ")?.split_whitespace().next())
        .map(str::to_owned)
        .collect();
    assert!(exports.len() >= 24, "{exports:?}");
    let mut text = String::from("#include <stdint.h>\n#include <stdio.h>\n");
    for name in &exports {
        text.push_str(&format!("int64_t tz_{name}(int64_t pattern);\n"));
    }
    text.push_str("int main(void) {\n    for (int64_t pattern = 0; pattern < 5; pattern++) {\n");
    for name in &exports {
        text.push_str(&format!(
            "        printf(\"{name} %lld %lld\\n\", (long long)pattern, (long long)tz_{name}(pattern));\n"
        ));
    }
    text.push_str("    }\n    return 0;\n}\n");
    let host = write(&directory, "host.c", &text);
    // The `@cpu` versions of F08 name their own target features: a link that merged the attributes of the
    // functions would run an instruction that this machine lacks, or give other results.
    let (bitcode, object) = (directory.join("kernels.bc"), directory.join("kernels.o"));
    build(&source, &bitcode, bitcode_options()).unwrap();
    build(
        &source,
        &object,
        BuildOptions {
            emit: Emit::Object,
            ..bitcode_options()
        },
    )
    .unwrap();
    let linked = link(
        &clang(),
        &directory,
        "from-object",
        &[&host, &object],
        false,
        &["-pthread"],
    );
    let inlined = link(
        &clang(),
        &directory,
        "from-bitcode",
        &[&host, &bitcode],
        true,
        &["-pthread"],
    );
    let expected = run(&linked);
    assert_eq!(expected.lines().count(), exports.len() * 5);
    assert_eq!(run(&inlined), expected);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn every_allocator_works_through_bitcode() {
    if llvm_tool("llvm-link").is_none() || !thinlto_link_works() {
        return;
    }
    let directory = scratch("allocators");
    let source = write(&directory, "Main.tz", BUFFERS);
    let host = write(&directory, "host.c", BUFFERS_HOST);
    let host_allocator = write(&directory, "allocator.c", HOST_ALLOCATOR_HOST);
    // The counting allocator's statistics and the host's own allocator both see every block freed.
    let cases: [(&str, Allocator, &Path, &[&str], &str); 4] = [
        ("system", Allocator::System, &host, &[], "100 42 6\n"),
        ("small", Allocator::Small, &host, &[], "100 42 6\n"),
        (
            "counting",
            Allocator::Counting,
            &host,
            &["-DCOUNTING"],
            "100 42 6\n1 0\n",
        ),
        ("host", Allocator::Host, &host_allocator, &[], "100 1 0\n"),
    ];
    for (name, allocator, main, extra, expected) in cases {
        let bitcode = directory.join(format!("{name}.bc"));
        let options = BuildOptions {
            allocator,
            ..bitcode_options()
        };
        build(&source, &bitcode, options)
            .unwrap_or_else(|error| panic!("{name}: {}", error.message));
        let program = link(&clang(), &directory, name, &[main, &bitcode], true, extra);
        assert_eq!(run(&program), expected, "{name}");
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn a_trap_returns_to_a_host_that_links_the_bitcode() {
    let Some(nm) = llvm_tool("llvm-nm") else {
        return;
    };
    if llvm_tool("llvm-link").is_none() || !thinlto_link_works() {
        return;
    }
    let directory = scratch("traps");
    let source = write(&directory, "Main.tz", TRAPS);
    let bitcode = directory.join("traps.bc");
    build(
        &source,
        &bitcode,
        BuildOptions {
            trap_return: true,
            ..bitcode_options()
        },
    )
    .unwrap();
    let strong = defined(&nm, &bitcode, "TDBR");
    for name in [
        "tz_div",
        "tz_sum_to",
        "tsuzuri_try_div",
        "tsuzuri_try_sum_to",
    ] {
        assert!(strong.contains(name), "{name} is missing: {strong:?}");
    }
    let host = write(&directory, "host.c", TRAPS_HOST);
    let program = link(
        &clang(),
        &directory,
        "traps",
        &[&host, &bitcode],
        true,
        &["-pthread"],
    );
    assert_eq!(run(&program), "0 42 1 3 4950\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn bitcode_is_the_same_bytes_every_time() {
    if llvm_tool("llvm-link").is_none() {
        return;
    }
    let directory = scratch("determinism");
    for source in [
        write(&directory, "Main.tz", BUFFERS),
        root().join("tests/fixtures/tasks/Main.tz"),
    ] {
        let name = source.parent().unwrap().file_name().unwrap().to_owned();
        let first = directory.join(format!("{}-1.bc", name.to_string_lossy()));
        let second = directory.join(format!("{}-2.bc", name.to_string_lossy()));
        build(&source, &first, bitcode_options()).unwrap();
        build(&source, &second, bitcode_options()).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
    }
    fs::remove_dir_all(directory).unwrap();
}

/// Runs `tsuzuri build --emit bitcode` on `source` with `arguments` and `environment`, which must fail, and returns its
/// stderr. The failed build leaves no output behind.
#[cfg(unix)]
fn build_failure(
    source: &Path,
    output: &Path,
    arguments: &[&str],
    environment: &[(&str, &OsStr)],
) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
    command
        .arg("build")
        .arg(source)
        .args(["--emit", "bitcode", "--no-cache", "-o"])
        .arg(output)
        .args(arguments);
    for (name, value) in environment {
        command.env(name, value);
    }
    let result = command.output().unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
    String::from_utf8_lossy(&result.stderr).into_owned()
}

#[test]
#[cfg(unix)]
fn a_clang_that_drops_lto_is_an_error() {
    let directory = scratch("without-lto");
    // The bundled launcher compiled IR without `-flto=thin` for a while: it wrote a native object instead.
    let wrapper = script(
        &directory,
        "clang-without-lto",
        "n=$#\nwhile [ $n -gt 0 ]; do a=$1; shift; n=$((n - 1)); [ \"$a\" = -flto=thin ] || set -- \"$@\" \"$a\"; done\nexec \"$REAL_CLANG\" \"$@\"",
    );
    let source = write(&directory, "Main.tz", ADD);
    let real = clang();
    let message = build_failure(
        &source,
        &directory.join("add.bc"),
        &[],
        &[
            ("TSUZURI_CLANG", wrapper.as_os_str()),
            ("REAL_CLANG", real.as_os_str()),
        ],
    );
    assert!(message.contains("error[E2002]"), "{message}");
    assert!(
        message.contains("did not write LLVM bitcode for --emit bitcode"),
        "{message}"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn a_missing_llvm_link_names_the_variable() {
    let directory = scratch("no-llvm-link");
    let missing = directory.join("no-such-llvm-link");
    let message = build_failure(
        &root().join("tests/fixtures/tasks/Main.tz"),
        &directory.join("tasks.bc"),
        &[],
        &[("TSUZURI_LLVM_LINK", missing.as_os_str())],
    );
    assert!(
        message.contains("error[E2002]: cannot execute"),
        "{message}"
    );
    assert!(
        message.contains("bitcode with the C runtime requires llvm-link matching Clang; set TSUZURI_LLVM_LINK, or put llvm-link next to Clang"),
        "{message}"
    );
    fs::remove_dir_all(directory).unwrap();
}

/// The host of the WebAssembly tests: `run` calls the exported function in a loop.
#[cfg(unix)]
const WASM_ADD_HOST: &str = r#"#include <stdint.h>

int64_t tz_add(int64_t x, int64_t y);

int64_t run(int64_t n) {
    int64_t total = 0;
    for (int64_t i = 0; i < n; i++) total = tz_add(total, i);
    return total;
}
"#;

/// `run` of the SIMD fixture calls its `transform` once.
#[cfg(unix)]
const WASM_SIMD_HOST: &str = r#"#include <stdint.h>

int64_t tz_transform(int64_t count, int32_t seed);

int64_t run(int64_t n) { return tz_transform(n, 5); }
"#;

/// Instantiates the module without imports, and prints its imports and `run(n)`.
#[cfg(unix)]
const NODE_RUN: &str = "const m = new WebAssembly.Module(require('fs').readFileSync(process.argv[1])); const i = new WebAssembly.Instance(m, {}); console.log(JSON.stringify({imports: WebAssembly.Module.imports(m), result: String(i.exports.run(BigInt(process.argv[2])))}));";

/// What the WebAssembly tests need besides Clang: a Clang with the WebAssembly backend, `wasm-ld`, and Node.
#[cfg(unix)]
struct WasmTools {
    wasm_ld: OsString,
}

#[cfg(unix)]
fn wasm_tools() -> Option<WasmTools> {
    let clang = clang();
    let beside = Path::new(&clang).with_file_name("wasm-ld").into_os_string();
    let wasm_ld = std::env::var_os("TSUZURI_WASM_LD")
        .into_iter()
        .chain([beside, OsString::from("wasm-ld")])
        .find(|tool| runs(tool));
    let Some(wasm_ld) = wasm_ld else {
        unavailable(
            "wasm-ld was not found: set TSUZURI_WASM_LD or put it next to Clang or on PATH",
        );
        return None;
    };
    if !runs(OsStr::new("node")) {
        unavailable("node is not installed");
        return None;
    }
    let directory = scratch("wasm-probe");
    let source = write(
        &directory,
        "probe.c",
        "int tsuzuri_probe(void) { return 0; }\n",
    );
    let compiled = Command::new(&clang)
        .args(["--target=wasm32-unknown-unknown", "-c"])
        .arg(&source)
        .arg("-o")
        .arg(directory.join("probe.o"))
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(directory);
    if !compiled.status.success() {
        unavailable(&format!(
            "this Clang has no WebAssembly backend: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
        return None;
    }
    Some(WasmTools { wasm_ld })
}

/// Links `inputs` with `host` into `directory/name.wasm`, which exports only `run`, with `wasm-ld`'s LTO or not.
#[cfg(unix)]
fn link_wasm(
    tools: &WasmTools,
    directory: &Path,
    name: &str,
    inputs: &[&Path],
    lto: bool,
) -> PathBuf {
    let module = directory.join(format!("{name}.wasm"));
    let mut linker = OsString::from("--ld-path=");
    linker.push(&tools.wasm_ld);
    let mut command = Command::new(clang());
    command
        .args(["--target=wasm32-unknown-unknown", "-O3", "-nostdlib"])
        .arg(&linker)
        .args(["-Wl,--no-entry", "-Wl,--export=run"]);
    if lto {
        command.arg("-flto=thin");
    }
    let result = command
        .args(inputs)
        .arg("-o")
        .arg(&module)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    module
}

/// The output of Node for `run(n)`: `{"imports":[],"result":"..."}`.
#[cfg(unix)]
fn run_wasm(module: &Path, n: u64) -> String {
    let output = Command::new("node")
        .args(["-e", NODE_RUN])
        .arg(module)
        .arg(n.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        module.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// The lines of the disassembly of `module` that contain one of `needles`.
#[cfg(unix)]
fn disassembly_matches(objdump: &OsStr, module: &Path, needles: &[&str]) -> usize {
    let output = Command::new(objdump)
        .args(["-d", "--no-show-raw-insn"])
        .arg(module)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| needles.iter().any(|needle| line.contains(needle)))
        .count()
}

#[test]
#[cfg(unix)]
fn wasm_ld_inlines_wasm32_bitcode_and_the_module_has_no_import() {
    let Some(objdump) = llvm_tool("llvm-objdump") else {
        return;
    };
    let Some(tools) = wasm_tools() else {
        return;
    };
    let directory = scratch("wasm");
    let source = write(&directory, "Main.tz", ADD);
    let (bitcode, object) = (directory.join("add.bc"), directory.join("add.o"));
    let wasm = BuildOptions {
        target: driver::Target::Wasm32,
        ..bitcode_options()
    };
    build(&source, &bitcode, wasm).unwrap();
    build(
        &source,
        &object,
        BuildOptions {
            emit: Emit::Object,
            ..wasm
        },
    )
    .unwrap();
    assert!(is_bitcode(&fs::read(&bitcode).unwrap()));
    // The bitcode of a program is the same bytes every time, as for a native target.
    let again = directory.join("again.bc");
    build(&source, &again, wasm).unwrap();
    assert_eq!(fs::read(&bitcode).unwrap(), fs::read(&again).unwrap());
    let host = write(&directory, "host.c", WASM_ADD_HOST);
    let called = link_wasm(&tools, &directory, "called", &[&host, &object], false);
    let inlined = link_wasm(&tools, &directory, "inlined", &[&host, &bitcode], true);
    let expected = r#"{"imports":[],"result":"4999950000"}"#;
    assert_eq!(run_wasm(&called, 100_000), expected);
    assert_eq!(run_wasm(&inlined, 100_000), expected);
    // The module of the object has `tz_add` and calls it; the LTO link inlined it and dropped it.
    assert!(disassembly_matches(&objdump, &called, &["tz_add"]) > 0);
    assert_eq!(disassembly_matches(&objdump, &inlined, &["tz_add"]), 0);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn wasm32_bitcode_keeps_its_simd_through_the_link() {
    let Some(objdump) = llvm_tool("llvm-objdump") else {
        return;
    };
    let Some(tools) = wasm_tools() else {
        return;
    };
    let directory = scratch("wasm-simd");
    let source = root().join("tests/fixtures/wasm_simd/Main.tz");
    let host = write(&directory, "host.c", WASM_SIMD_HOST);
    let simd_instructions = ["v128.", "i32x4.", "i64x2.", "i8x16.", "i16x8."];
    let mut counts = Vec::new();
    // The host is built without -msimd128 either way: the functions of the bitcode carry the feature themselves.
    for simd in [false, true] {
        let bitcode = directory.join(format!("simd-{simd}.bc"));
        let options = BuildOptions {
            target: driver::Target::Wasm32,
            wasm_simd: simd,
            ..bitcode_options()
        };
        build(&source, &bitcode, options).unwrap();
        let module = link_wasm(
            &tools,
            &directory,
            &format!("simd-{simd}"),
            &[&host, &bitcode],
            true,
        );
        // sum of ((i + 5) * 3 + 7) for i < 100000
        assert_eq!(
            run_wasm(&module, 100_000),
            r#"{"imports":[],"result":"15002050000"}"#,
            "simd {simd}"
        );
        counts.push(disassembly_matches(&objdump, &module, &simd_instructions));
    }
    assert_eq!(counts[0], 0, "no SIMD without --wasm-feature simd128");
    assert!(counts[1] > 0, "the link dropped the SIMD of the bitcode");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn a_host_thread_posts_to_the_reactor_of_the_bitcode() {
    let Some(nm) = llvm_tool("llvm-nm") else {
        return;
    };
    if llvm_tool("llvm-link").is_none() || !thinlto_link_works() {
        return;
    }
    let directory = scratch("reactor");
    let source = root().join("tests/fixtures/async_reactor/Main.tz");
    let (bitcode, object) = (directory.join("reactor.bc"), directory.join("reactor.o"));
    build(&source, &bitcode, bitcode_options()).unwrap();
    build(
        &source,
        &object,
        BuildOptions {
            emit: Emit::Object,
            ..bitcode_options()
        },
    )
    .unwrap();
    // Only the C runtime defines `tsuzuri_async_post`, and the host calls it from its own threads: the merge of the
    // runtime must not hide it, as it hides the rest of the runtime.
    let external = externals(&nm, &bitcode);
    for name in [
        "tsuzuri_async_post",
        "tsuzuri_async_poll",
        "tsuzuri_async_complete",
    ] {
        assert!(
            external.contains(name),
            "{name} is not external: {external:?}"
        );
    }
    assert!(
        !external.contains("tsuzuri_async_wait"),
        "the rest of the reactor stays inside the bitcode: {external:?}"
    );
    let host = write(&directory, "host.c", REACTOR_HOST);
    let linked = link(
        &clang(),
        &directory,
        "from-object",
        &[&host, &object],
        false,
        &["-pthread"],
    );
    let inlined = link(
        &clang(),
        &directory,
        "from-bitcode",
        &[&host, &bitcode],
        true,
        &["-pthread"],
    );
    // 4 * 10 * 1000 + 5 * 10 from the two operations that the host completes later; a post of an unknown id is refused.
    assert_eq!(run(&linked), "40050 0\n");
    assert_eq!(run(&inlined), "40050 0\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn the_bitcode_defines_every_symbol_that_a_shared_library_exports() {
    let Some(nm) = llvm_tool("llvm-nm") else {
        return;
    };
    if llvm_tool("llvm-link").is_none() {
        return;
    }
    let directory = scratch("exports");
    // A project of its own for each source, so that a module is not named like one of the standard library.
    let project = |name: &str, text: &str| {
        let folder = directory.join(name);
        fs::create_dir(&folder).unwrap();
        write(&folder, "Main.tz", text)
    };
    let cases = [
        (
            "async",
            project("async", ASYNC),
            bitcode_options(),
            vec!["tsuzuri_async_post", "tsuzuri_async_poll", "tz_timers"],
        ),
        (
            "hello",
            root().join("examples/hello/Main.tz"),
            bitcode_options(),
            vec!["tsuzuri_main", "tz_answer"],
        ),
        (
            "counting",
            project("counting", BUFFERS),
            BuildOptions {
                allocator: Allocator::Counting,
                ..bitcode_options()
            },
            vec!["tsuzuri_alloc_stats", "tsuzuri_alloc", "tz_bytes"],
        ),
        (
            "trap-return",
            project("trap-return", TRAPS),
            BuildOptions {
                trap_return: true,
                ..bitcode_options()
            },
            vec!["tsuzuri_try_div", "tz_div"],
        ),
    ];
    for (name, source, options, expected) in cases {
        let bitcode = directory.join(format!("{name}.bc"));
        let shared = directory.join(format!("{name}.shared"));
        build(&source, &bitcode, options)
            .unwrap_or_else(|error| panic!("{name}: {}", error.message));
        build(
            &source,
            &shared,
            BuildOptions {
                emit: Emit::Shared,
                ..options
            },
        )
        .unwrap_or_else(|error| panic!("{name}: {}", error.message));
        // The public C ABI of the shared library, which `shared_exports` lists, is what the bitcode must define too.
        let exported: BTreeSet<String> = externals(&nm, &shared)
            .into_iter()
            .filter(|symbol| symbol.starts_with("tz_") || symbol.starts_with("tsuzuri_"))
            .collect();
        let kept = externals(&nm, &bitcode);
        for symbol in expected {
            assert!(
                exported.contains(symbol),
                "{name}: {symbol} in {exported:?}"
            );
        }
        let lost: Vec<_> = exported.difference(&kept).collect();
        assert!(lost.is_empty(), "{name}: the bitcode lost {lost:?}");
    }
    fs::remove_dir_all(directory).unwrap();
}

/// A script that `tsuzuri` runs as Clang: `text` is the body of a `sh` script.
#[cfg(unix)]
fn script(directory: &Path, name: &str, text: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = write(directory, name, &format!("#!/bin/sh\n{text}\n"));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
#[cfg(unix)]
fn a_failed_final_compile_names_llvm_link_only_when_it_ran() {
    let directory = scratch("final-compile");
    // Every run of Clang goes to the real one, except the compile that writes the bitcode (the one with -flto=thin).
    let wrapper = script(
        &directory,
        "clang-fails-final",
        "for a in \"$@\"; do\n  if [ \"$a\" = -flto=thin ]; then echo 'fake clang: the final compile fails' >&2; exit 1; fi\ndone\nexec \"$REAL_CLANG\" \"$@\"",
    );
    let real = clang();
    let mut environment = vec![
        ("TSUZURI_CLANG", wrapper.as_os_str()),
        ("REAL_CLANG", real.as_os_str()),
    ];
    let hint = "the IR was merged by llvm-link, which must come from the same LLVM as Clang: set TSUZURI_LLVM_LINK";
    // llvm-link merges the C runtime of a native program only: a program without one and WebAssembly never run it.
    let source = write(&directory, "Main.tz", ADD);
    for (name, arguments) in [("native", &[][..]), ("wasm32", &["--target", "wasm32"][..])] {
        let message = build_failure(
            &source,
            &directory.join(format!("{name}.bc")),
            arguments,
            &environment,
        );
        assert!(message.contains("error[E2002]"), "{name}: {message}");
        assert!(
            message.contains("fake clang: the final compile fails"),
            "{name}: {message}"
        );
        assert!(!message.contains("llvm-link"), "{name}: {message}");
    }
    let Some(link) = llvm_tool("llvm-link") else {
        return;
    };
    environment.push(("TSUZURI_LLVM_LINK", link.as_os_str()));
    let message = build_failure(
        &root().join("tests/fixtures/tasks/Main.tz"),
        &directory.join("tasks.bc"),
        &[],
        &environment,
    );
    assert!(
        message.contains("fake clang: the final compile fails"),
        "{message}"
    );
    assert!(message.contains(hint), "{message}");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(unix)]
fn bitcode_takes_the_options_of_an_object() {
    if llvm_tool("llvm-link").is_none() || !thinlto_link_works() {
        return;
    }
    let directory = scratch("options");
    let source = write(&directory, "Main.tz", BUFFERS);
    let host = write(&directory, "host.c", BUFFERS_HOST);
    // `-g` is linked with the host's default DWARF 5, which only warns (see native-interop.md).
    let cases = [
        (
            "debug",
            BuildOptions {
                debug_info: true,
                ..bitcode_options()
            },
        ),
        (
            "native",
            BuildOptions {
                cpu: Cpu::Native,
                ..bitcode_options()
            },
        ),
        (
            "O0",
            BuildOptions {
                optimization: OptLevel::O0,
                ..bitcode_options()
            },
        ),
        (
            "Oz",
            BuildOptions {
                optimization: OptLevel::Oz,
                ..bitcode_options()
            },
        ),
        (
            "trap-info",
            BuildOptions {
                trap_info: true,
                ..bitcode_options()
            },
        ),
    ];
    for (name, options) in cases {
        let bitcode = directory.join(format!("{name}.bc"));
        build(&source, &bitcode, options)
            .unwrap_or_else(|error| panic!("{name}: {}", error.message));
        assert!(is_bitcode(&fs::read(&bitcode).unwrap()), "{name}");
        let program = link(
            &clang(),
            &directory,
            name,
            &[&host, &bitcode],
            true,
            &["-pthread"],
        );
        assert_eq!(run(&program), "100 42 6\n", "{name}");
    }
    fs::remove_dir_all(directory).unwrap();
}
