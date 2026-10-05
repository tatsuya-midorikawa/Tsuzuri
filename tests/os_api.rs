use std::collections::BTreeSet;

use tsuzuri::check::CheckedModule;
use tsuzuri::{analyze, analyze_modules, llvm};

fn module(source: &str) -> CheckedModule {
    analyze_modules(&[("Main.tz", source)])
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

/// The IR of both targets must be deterministic with deduplicated declarations.
fn ir(module: &CheckedModule, entry: llvm::Entry, wasm: bool) -> String {
    let ir = llvm::emit_target(module, entry, wasm).unwrap();
    assert_eq!(ir, llvm::emit_target(module, entry, wasm).unwrap());
    let declarations: Vec<_> = ir
        .lines()
        .filter(|line| line.starts_with("declare "))
        .collect();
    let unique: BTreeSet<_> = declarations.iter().collect();
    assert_eq!(unique.len(), declarations.len(), "{ir}");
    ir
}

/// The text of the function whose definition starts with `header`.
fn function<'a>(ir: &'a str, header: &str) -> &'a str {
    let start = ir.find(header).unwrap_or_else(|| panic!("no {header}"));
    let rest = &ir[start..];
    &rest[..rest.find("\n}\n").unwrap()]
}

#[test]
fn reserves_os_module_names() {
    for name in [
        "File", "Dir", "Path", "Env", "Time", "Random", "Os", "Process",
    ] {
        let path = format!("{name}.tz");
        let error = analyze_modules(&[("Main.tz", "0"), (&path, "def f :: i64\nfn f = 1")])
            .expect_err(name);
        assert_eq!(error.code, "E1011", "{name}: {}", error.message);
        assert!(
            error.message.contains("reserved for the standard library"),
            "{}",
            error.message
        );
    }
}

#[test]
fn os_primitives_are_private_to_std() {
    for name in [
        "Os.__read",
        "Os.__args",
        "Os.__write",
        "Os.__random",
        "Os.__clock",
        "Os.__sleep",
        "Os.__open",
        "Os.__handle",
        "Os.__close",
        "Os.__spawn",
    ] {
        let error = analyze(&format!("let call = {name}\n0")).expect_err(name);
        assert_eq!(error.code, "E1022", "{name}: {}", error.message);
        assert!(
            error
                .message
                .contains("operating-system primitives are private"),
            "{}",
            error.message
        );
    }
}

#[test]
fn os_api_signatures_type_check() {
    module(
        "def main :: IO<unit> =
    let read_bytes: string -> IO<Result<[ubyte], Os.Error>> = File.read_bytes
    let read_text: string -> IO<Result<string, Os.Error>> = File.read_text
    let write_bytes: string -> [ubyte] -> IO<Result<unit, Os.Error>> = File.write_bytes
    let write_text: string -> string -> IO<Result<unit, Os.Error>> = File.write_text
    let append_text: string -> string -> IO<Result<unit, Os.Error>> = File.append_text
    let remove_file: string -> IO<Result<unit, Os.Error>> = File.remove
    let list: string -> IO<Result<[string], Os.Error>> = Dir.list
    let create: string -> IO<Result<unit, Os.Error>> = Dir.create
    let remove_dir: string -> IO<Result<unit, Os.Error>> = Dir.remove
    let join: ref string -> ref string -> string = Path.join
    let parent: ref string -> Maybe<string> = Path.parent
    let file_name: ref string -> Maybe<string> = Path.file_name
    let extension: ref string -> Maybe<string> = Path.extension
    let args: unit -> IO<Result<[string], Os.Error>> = Env.args
    let variable: string -> IO<Result<Maybe<string>, Os.Error>> = Env.var
    let current: unit -> IO<Result<string, Os.Error>> = Env.current_dir
    let monotonic: unit -> IO<Result<i64, Os.Error>> = Time.monotonic_ns
    let unix: unit -> IO<Result<i64, Os.Error>> = Time.unix_ns
    let sleep: i64 -> IO<Result<unit, Os.Error>> = Time.sleep_ms
    let bytes: i64 -> IO<Result<[ubyte], Os.Error>> = Random.bytes
    let next: unit -> IO<Result<i64u, Os.Error>> = Random.next_u64
    let pcg: i64u -> i64u -> Random.Pcg = Random.pcg
    let next_u32: Random.Pcg -> (i32u * Random.Pcg) = Random.pcg_next_u32
    let next_pcg_u64: Random.Pcg -> (i64u * Random.Pcg) = Random.pcg_next_u64
    let open: string -> File.Mode -> IO<Result<File.Handle, Os.Error>> = File.open
    let read: File.Handle -> i64 -> IO<Result<[ubyte], Os.Error>> = File.read
    let write: File.Handle -> [ubyte] -> IO<Result<unit, Os.Error>> = File.write
    let flush: File.Handle -> IO<Result<unit, Os.Error>> = File.flush
    let close: File.Handle -> IO<Result<unit, Os.Error>> = File.close
    let modes = [File.Read, File.Write, File.Append, File.CreateNew]
    let describe: string -> IO<Result<File.Metadata, Os.Error>> = File.metadata
    let describe_link: string -> IO<Result<File.Metadata, Os.Error>> = File.link_metadata
    let walk: string -> IO<Result<[string], Os.Error>> = Dir.walk
    let run: string -> [string] -> [ubyte] -> IO<Result<Process.Output, Os.Error>> = Process.run
    let kinds = [File.Regular, File.Directory, File.Symlink, File.Other]
    let decode: [ubyte] -> Result<string, Os.Error> = Os.decode
    let names: ref [ubyte] -> Result<[string], Os.Error> = Os.split_names
    let number: ref [ubyte] -> i64 -> i64 = Os.decode_i64
    let message: ref Os.Error -> string = Os.message
    let encode: ref string -> Result<utf8string, Os.Error> = Os.encode
    let status: i64 -> Os.Error = Os.error_of_status
    let seeded: unit -> IO<HashMap<i64, string>> = HashMap.randomized
    let try_seeded: unit -> IO<Result<HashMap<i64, string>, Os.Error>> = HashMap.try_randomized
    let seeded_set: unit -> IO<HashSet<string>> = HashSet.randomized
    let try_seeded_set: unit -> IO<Result<HashSet<string>, Os.Error>> = HashSet.try_randomized
    do! IO.write_line \"typed\"
",
    );
}

#[test]
fn os_builtins_declare_runtime_only_when_reached() {
    let program = module(
        "def main :: IO<unit> =\n    let! bytes = File.read_bytes \"a\"\n    do! IO.write_line (Result.is_ok (&bytes))\n",
    );
    for wasm in [false, true] {
        let ir = ir(&program, llvm::Entry::Library, wasm);
        assert!(
            ir.contains("declare i64 @tsuzuri_os_read(ptr, i32, ptr, i64)\n"),
            "{ir}"
        );
        assert!(!ir.contains("@tsuzuri_os_write"), "{ir}");
        assert!(!ir.contains("@tsuzuri_os_args"), "{ir}");
        for line in ir.lines().filter(|line| line.contains("@tsuzuri_os_")) {
            for attribute in ["readnone", "readonly", "memory("] {
                assert!(!line.contains(attribute), "{line}");
            }
        }
    }
    // The pure parts of Path, Os, and Random.Pcg need no runtime at all.
    let pure = module(
        "export def parent_length :: i64\nfn parent_length =\n    let path = \"a/b\"\n    match Path.parent (&path) with\n    | Maybe.Some text -> text.length\n    | Maybe.None -> 0\n",
    );
    for wasm in [false, true] {
        let ir = ir(&pure, llvm::Entry::Library, wasm);
        assert!(!ir.contains("tsuzuri_os_"), "{ir}");
        assert!(!ir.contains("wasm-import-module"), "{ir}");
    }
}

#[test]
fn seeded_hash_containers_need_no_runtime_but_randomized_ones_do() {
    let seeded = module(
        "export def seeded :: i64\nfn seeded =\n    let map: HashMap<i64, i64> = HashMap.insert (HashMap.with_seed 7i64u) 1 2\n    HashMap.length (&map) + HashMap.longest_probe (&map)\n",
    );
    for wasm in [false, true] {
        let ir = ir(&seeded, llvm::Entry::Library, wasm);
        assert!(!ir.contains("tsuzuri_os_"), "{ir}");
        assert!(!ir.contains("wasm-import-module"), "{ir}");
    }
    let randomized = module(
        "def main :: IO<unit> =\n    let! map = HashMap.randomized ()\n    let map: HashMap<i64, i64> = map\n    do! IO.write_line (HashMap.length (&map))\n",
    );
    for wasm in [false, true] {
        let ir = ir(&randomized, llvm::Entry::Library, wasm);
        assert!(ir.contains("@tsuzuri_os_random"), "{ir}");
    }
}

#[test]
fn io_only_programs_keep_their_main() {
    let program = module("def main :: IO<unit> =\n    do! IO.write_line \"hi\"\n");
    let console = ir(&program, llvm::Entry::Console, false);
    assert!(!console.contains("tsuzuri_os_"), "{console}");
    assert!(console.contains("define i32 @main() {"), "{console}");
    assert!(!console.contains("%argc"), "{console}");
}

#[test]
fn args_entry_receives_argv() {
    let program = module(
        "def main :: IO<unit> =\n    let! args = Env.args ()\n    do! IO.write_line (Result.is_ok (&args))\n",
    );
    let console = ir(&program, llvm::Entry::Console, false);
    assert!(
        console.contains("define i32 @main(i32 %argc, ptr %argv) {"),
        "{console}"
    );
    assert!(
        console.contains("call void @tsuzuri_os_set_args(i32 %argc, ptr %argv)"),
        "{console}"
    );
    assert!(
        console.contains("declare void @tsuzuri_os_set_args(i32, ptr)\n"),
        "{console}"
    );
    // A library host calls `tsuzuri_os_set_args` itself.
    let library = ir(&program, llvm::Entry::Library, false);
    assert!(!library.contains("define i32 @main("), "{library}");
}

#[test]
fn pcg_is_opaque() {
    let io = analyze("IO { work: \\() -> 42 }").unwrap_err();
    let pcg = analyze("Random.Pcg { state: 1i64u, increment: 2i64u }").unwrap_err();
    assert_eq!(io.code, "E1022");
    assert_eq!(pcg.code, io.code, "{}", pcg.message);
    for source in [
        "let generator = Random.pcg 1i64u 2i64u\ngenerator.state",
        "let generator = Random.pcg 1i64u 2i64u\n{ generator with increment = 2i64u }.state",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1022", "{source}");
    }
}

#[test]
fn file_handles_are_opaque_numbers_that_actions_can_hold() {
    for source in [
        "File.Handle { id: 1 }",
        "let handle = File.Handle { id: 1 }\n0",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1022", "{source}");
    }
    // A handle is Copy, so one IO action can hold it and several can share it.
    module(
        "def twice :: File.Handle -> IO<unit> =
    \\handle -> IO {
        let! first = File.close handle
        let! second = File.close handle
        do! IO.write_line (Result.is_ok (&first))
        do! IO.write_line (Result.is_ok (&second))
    }
def main :: IO<unit> =
    let! opened = File.open \"a\" File.Read
    do! IO.write_line (Result.is_ok (&opened))
",
    );
    let program = module(
        "def main :: IO<unit> =\n    let! opened = File.open \"a\" File.Read\n    do! IO.write_line (Result.is_ok (&opened))\n",
    );
    for wasm in [false, true] {
        let ir = ir(&program, llvm::Entry::Library, wasm);
        assert!(
            ir.contains("declare i64 @tsuzuri_os_open(i32, ptr, i64)\n"),
            "{ir}"
        );
        assert!(!ir.contains("@tsuzuri_os_close"), "{ir}");
    }
}

#[test]
fn exit_code_entry_returns_the_value() {
    let coded = module("def main :: IO<i32> =\n    do! IO.write_line \"x\"\n    return 3i32\n");
    assert!(llvm::exit_code_entry(&coded));
    let text = ir(&coded, llvm::Entry::Console, false);
    let entry = function(&text, "define i32 @tsuzuri_main()");
    assert!(!entry.contains("ret i32 0"), "{entry}");
    assert!(entry.contains("ret i32 %"), "{entry}");
    for source in [
        "def main :: IO<i64> =\n    return 3\n",
        "def main :: IO<unit> =\n    do! IO.write_line \"x\"\n",
        "def main :: IO<i32u> =\n    return 3i32u\n",
    ] {
        let plain = module(source);
        assert!(!llvm::exit_code_entry(&plain), "{source}");
        let text = ir(&plain, llvm::Entry::Console, false);
        assert!(
            function(&text, "define i32 @tsuzuri_main()").contains("ret i32 0"),
            "{source}"
        );
    }
}
