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

#[test]
fn reserves_net_module() {
    let error = analyze_modules(&[("Main.tz", "0"), ("Net.tz", "def f :: i64\nfn f = 1")])
        .expect_err("Net.tz");
    assert_eq!(error.code, "E1011", "{}", error.message);
    assert!(
        error.message.contains("reserved for the standard library"),
        "{}",
        error.message
    );
}

#[test]
fn resolve_leaves_address_text_to_the_strict_parser() {
    use std::path::Path;
    use std::process::Command;
    // A host that a system's resolver would read as an address in its own way ("127.1", "0x7f000001", "010.0.0.1",
    // "fe80::1%lo0") is decided by the strict parser alone, so the answer is the same on every system, and nothing
    // here asks a resolver or needs a network. tests/fixtures/net_resolve resolves a corpus that is all address text:
    // its first twelve hosts are what the strict parser reads, and every other one is InvalidInput.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/net_resolve");
    let corpus = std::fs::read_to_string(fixture.join("Corpus.tz")).unwrap();
    let hosts = corpus
        .lines()
        .filter(|line| line.starts_with("\t\""))
        .count();
    for needed in [
        "\"127.1\"",
        "\"0x7f.1\"",
        "\"0x7f000001\"",
        "\"2130706433\"",
        "\"1.2.3\"",
        "\"0177.0.0.1\"",
        "\"010.0.0.1\"",
        "\"0\"",
        "\"0x0\"",
        "\"4294967296\"",
        "\"fe80::1%lo0\"",
    ] {
        assert!(corpus.contains(needed), "the corpus has {needed}");
    }
    let root = std::env::temp_dir().join(format!("tsuzuri-net-resolve-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for optimization in ["-O0", "-O3"] {
        let executable = root.join(format!(
            "resolve{optimization}{}",
            std::env::consts::EXE_SUFFIX
        ));
        let built = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .arg("build")
            .arg(&fixture)
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
        let run = Command::new(&executable).output().unwrap();
        assert!(run.status.success());
        let output = String::from_utf8(run.stdout).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), hosts, "{optimization}\n{output}");
        assert_eq!(
            lines[..12],
            [
                "1: 127.0.0.1:80",
                "1: 0.0.0.0:80",
                "1: 255.255.255.255:80",
                "1: 192.0.2.7:80",
                "1: [::1]:80",
                "1: [::]:80",
                "1: [2001:db8::1]:80",
                "1: [2001:db8::1]:80",
                "1: [::ffff:c000:201]:80",
                "1: [1:2:3:4:5:6:7:8]:80",
                "1: [::1]:80",
                "1: [fe80::1]:80",
            ],
            "{optimization}"
        );
        for (index, line) in lines.iter().enumerate().skip(12) {
            assert_eq!(
                *line, "Unclassified InvalidInput 0",
                "host {index} of the corpus at {optimization}"
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn addresses_and_handles_are_opaque_copies() {
    // The records are not built outside the module, so no address holds a port that is not a port.
    for source in [
        "Net.Address { v6: false, high: 0i64u, low: 1i64u, port: 70000 }",
        "Net.TcpStream { id: 1, local: Net.Address { v6: false, high: 0i64u, low: 1i64u, port: 1 }, peer: Net.Address { v6: false, high: 0i64u, low: 1i64u, port: 1 } }",
        "Net.TcpListener { id: 1 }",
        "Net.UdpSocket { id: 1 }",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1022", "{source}");
    }
    for source in [
        "let address = Net.parse_ip (ref \"::1\") 1\nlet shown = Maybe.get address\nshown.port",
        "let address = Net.parse_ip (ref \"::1\") 1\nlet shown = Maybe.get address\n{ shown with port = 70000 }.high",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1022", "{source}");
    }
    // An address and a handle are Copy: one action holds them and several share them.
    module(
        "def twice :: Net.TcpStream -> IO<unit> =
    \\stream -> IO {
        let! first = Net.close stream
        let! second = Net.close stream
        do! IO.write_line (Result.is_ok (&first))
        do! IO.write_line (Result.is_ok (&second))
    }
def both :: Net.Address -> i64
fn both address = Net.port address + Net.port address
def listener :: Net.TcpListener -> Net.Address
fn listener value = Net.local_addr value
def again :: Net.UdpSocket -> (Net.Address * Net.Address)
fn again socket = (Net.udp_local_addr socket, Net.udp_local_addr socket)
",
    );
    // Addresses compare and hash, and print like the text that makes them.
    module(
        "def same :: Net.Address -> Net.Address -> bool
fn same left right = left == right
def key :: Net.Address -> i64u
fn key address = Hash.hash (&address)
def show :: Net.Address -> string
fn show address = to_string address
",
    );
}

#[test]
fn net_primitives_are_private_to_std() {
    for name in [
        "Net.__resolve",
        "Net.__open",
        "Net.__accept",
        "Net.__read",
        "Net.__write",
        "Net.__close",
        "Net.__classify",
        "Net.__watch",
        "Net.__unwatch",
        "Net.__connect",
        "Net.__names",
        "Net.__send",
    ] {
        let error = analyze(&format!("let call = {name}\n0")).expect_err(name);
        assert_eq!(error.code, "E1022", "{name}: {}", error.message);
        assert!(
            error
                .message
                .contains("network primitives are private to the standard Net module"),
            "{}",
            error.message
        );
    }
    let error = analyze("Net.__close 0i32 3").unwrap_err();
    assert_eq!(error.code, "E1022", "{}", error.message);
    let error = analyze("Net.__watch 1 1 1i32 -1").unwrap_err();
    assert_eq!(error.code, "E1022", "{}", error.message);
}

#[test]
fn types_the_async_api() {
    // Every async operation is an `Async` computation of the same result that its blocking twin has.
    module(
        "def main :: unit -> i32 = \\() ->
    let connect: Net.Address -> Maybe<i64> -> Async<Result<Net.TcpStream, Os.Error>> = Net.connect_async
    let accept: Net.TcpListener -> Maybe<i64> -> Async<Result<Net.TcpStream, Os.Error>> = Net.accept_async
    let read: Net.TcpStream -> i64 -> Maybe<i64> -> Async<Result<[ubyte], Os.Error>> = Net.read_async
    let write: Net.TcpStream -> [ubyte] -> Maybe<i64> -> Async<Result<unit, Os.Error>> = Net.write_async
    let recv_from: Net.UdpSocket -> i64 -> Maybe<i64> -> Async<Result<([ubyte] * Net.Address), Os.Error>> = Net.recv_from_async
    let send_to: Net.UdpSocket -> [ubyte] -> Net.Address -> Async<Result<unit, Os.Error>> = Net.send_to_async
    let bind: Net.Address -> Async<Result<Net.TcpListener, Os.Error>> = Net.bind_async
    let bind_udp: Net.Address -> Async<Result<Net.UdpSocket, Os.Error>> = Net.bind_udp_async
    let close: Net.TcpStream -> Async<Result<unit, Os.Error>> = Net.close_async
    let close_listener: Net.TcpListener -> Async<Result<unit, Os.Error>> = Net.close_listener_async
    let close_udp: Net.UdpSocket -> Async<Result<unit, Os.Error>> = Net.close_udp_async
    let shutdown: Net.TcpStream -> Net.Shutdown -> Async<Result<unit, Os.Error>> = Net.shutdown_async
    do! IO.write_line \"typed\"
    0
",
    );
    // A computation of several operations is an ordinary `Async` block.
    module(
        "def fetch :: Net.Address -> Async<i64>
fn fetch address = Async {
    match! Net.connect_async address (Maybe.Some 100) with
    | Result.Error _ -> return -1
    | Result.Ok stream ->
        let! _written = Net.write_async stream [1ubyte] (Maybe.Some 100)
        match! Net.read_async stream 8 (Maybe.Some 100) with
        | Result.Error _ -> return -2
        | Result.Ok bytes ->
            let! _closed = Net.close_async stream
            return bytes.length
}
def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))
    let! outcome = Async.block_on (Async.all [fetch address, fetch address])
    do! IO.write_line outcome[0]
    0
",
    );
}

#[test]
fn async_operations_declare_what_they_reach_and_need_the_reactor() {
    // `read_async` waits for readiness and tries to read; it never connects, and it hands the poller the function
    // that completes an operation.
    let reading = module(
        "def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))
    let! connected = Net.connect address Maybe.None
    match connected with
    | Result.Error _ -> do! IO.write_line \"none\"
    | Result.Ok stream ->
        let! count = Async.block_on (Async {
            match! Net.read_async stream 8 (Maybe.Some 10) with
            | Result.Error _ -> return -1
            | Result.Ok bytes -> return bytes.length
        })
        do! IO.write_line count
    0
",
    );
    let native = ir(&reading, llvm::Entry::Console, false);
    for declaration in [
        "declare void @tsuzuri_net_watch(ptr, i64, i64, i32, i64)\n",
        "declare void @tsuzuri_net_unwatch(i64)\n",
        "declare i32 @tsuzuri_async_post(i64, i64)\n",
        "declare i64 @tsuzuri_net_read(ptr, i32, i64, i64, i64)\n",
    ] {
        assert!(native.contains(declaration), "{declaration}\n{native}");
    }
    assert!(!native.contains("@tsuzuri_net_connect"), "{native}");
    assert!(!native.contains("@tsuzuri_net_send"), "{native}");
    assert!(
        llvm::uses_net(&native) && llvm::uses_net_async(&native) && llvm::uses_reactor(&native)
    );
    // The two completions that net.c posts go through the pointer that the call passes.
    assert!(
        native.contains("call void @tsuzuri_net_watch(ptr @tsuzuri_async_post, "),
        "{native}"
    );

    // A connect is its own primitive, which names its target and hands over the socket, and sends need no wait.
    let connecting = module(
        "def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))
    let! outcome = Async.block_on (Net.connect_async address Maybe.None)
    do! IO.write_line (Result.is_ok (ref outcome))
    0
",
    );
    let native = ir(&connecting, llvm::Entry::Console, false);
    for declaration in [
        "declare void @tsuzuri_net_connect(ptr, i64, i64, i64, i64, i64)\n",
        "declare i64 @tsuzuri_net_names(ptr, i64)\n",
    ] {
        assert!(native.contains(declaration), "{declaration}\n{native}");
    }
    assert!(!native.contains("@tsuzuri_net_watch"), "{native}");

    // Without Async.block_on nothing receives the completion: the checker accepts it, and the driver reports E2000.
    let unaided = module(
        "def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))
    let outcome = Async.run (Net.connect_async address Maybe.None)
    do! IO.write_line (Result.is_ok (ref outcome))
    0
",
    );
    let text = ir(&unaided, llvm::Entry::Console, false);
    assert!(
        llvm::uses_net_async(&text) && !llvm::uses_reactor(&text),
        "{text}"
    );

    // The blocking operations need neither the reactor nor the poller entry points, though Net loads Async.
    let blocking = module(
        "def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:9\"))
    let! opened = Net.connect address Maybe.None
    do! IO.write_line (Result.is_ok (ref opened))
    0
",
    );
    for wasm in [false, true] {
        let text = ir(&blocking, llvm::Entry::Library, wasm);
        assert!(!llvm::uses_net_async(&text), "{text}");
        assert!(!text.contains("tsuzuri_async"), "{text}");
        assert!(!text.contains("tsuzuri_net_watch"), "{text}");
    }
}

#[test]
fn types_the_phase_one_api() {
    module(
        "def main :: unit -> i32 = \\() ->
    let parse: ref string -> Maybe<Net.Address> = Net.parse_address
    let parse_ip: ref string -> i64 -> Maybe<Net.Address> = Net.parse_ip
    let text: Net.Address -> string = Net.address_text
    let ip: Net.Address -> string = Net.ip_text
    let port: Net.Address -> i64 = Net.port
    let v6: Net.Address -> bool = Net.is_ipv6
    let kind: Os.Error -> Net.ErrorKind = Net.error_kind
    let resolve: string -> i64 -> IO<Result<[Net.Address], Os.Error>> = Net.resolve
    let connect: Net.Address -> Maybe<i64> -> IO<Result<Net.TcpStream, Os.Error>> = Net.connect
    let read: Net.TcpStream -> i64 -> Maybe<i64> -> IO<Result<[ubyte], Os.Error>> = Net.read
    let write: Net.TcpStream -> [ubyte] -> Maybe<i64> -> IO<Result<unit, Os.Error>> = Net.write
    let shutdown: Net.TcpStream -> Net.Shutdown -> IO<Result<unit, Os.Error>> = Net.shutdown
    let close: Net.TcpStream -> IO<Result<unit, Os.Error>> = Net.close
    let local: Net.TcpStream -> Net.Address = Net.stream_local_addr
    let peer: Net.TcpStream -> Net.Address = Net.peer_addr
    let bind: Net.Address -> IO<Result<Net.TcpListener, Os.Error>> = Net.bind
    let accept: Net.TcpListener -> Maybe<i64> -> IO<Result<Net.TcpStream, Os.Error>> = Net.accept
    let bound: Net.TcpListener -> Net.Address = Net.local_addr
    let close_listener: Net.TcpListener -> IO<Result<unit, Os.Error>> = Net.close_listener
    let bind_udp: Net.Address -> IO<Result<Net.UdpSocket, Os.Error>> = Net.bind_udp
    let send_to: Net.UdpSocket -> [ubyte] -> Net.Address -> IO<Result<unit, Os.Error>> = Net.send_to
    let recv_from: Net.UdpSocket -> i64 -> Maybe<i64> -> IO<Result<([ubyte] * Net.Address), Os.Error>> = Net.recv_from
    let udp_bound: Net.UdpSocket -> Net.Address = Net.udp_local_addr
    let close_udp: Net.UdpSocket -> IO<Result<unit, Os.Error>> = Net.close_udp
    let sides = [Net.Read, Net.Write, Net.Both]
    do! IO.write_line \"typed\"
    0
",
    );
    // The bracket helpers take the action as a function of the handle.
    module(
        "def serve :: Net.TcpStream -> IO<i64>
fn serve stream = IO.pure (Net.port (Net.peer_addr stream))
def main :: unit -> i32 = \\() ->
    let with_connection: Net.Address -> Maybe<i64> -> (Net.TcpStream -> IO<i64>) -> IO<Result<i64, Os.Error>> = Net.with_connection
    let with_accepted: Net.TcpListener -> Maybe<i64> -> (Net.TcpStream -> IO<i64>) -> IO<Result<i64, Os.Error>> = Net.with_accepted
    let with_listener: Net.Address -> (Net.TcpListener -> IO<i64>) -> IO<Result<i64, Os.Error>> = Net.with_listener
    let with_udp: Net.Address -> (Net.UdpSocket -> IO<i64>) -> IO<Result<i64, Os.Error>> = Net.with_udp
    let address = Maybe.get (Net.parse_address (ref \"127.0.0.1:1\"))
    let! connected = with_connection address (Maybe.Some 5) serve
    let! listened = with_listener address (\\listener -> IO.pure (Net.port (Net.local_addr listener)))
    let! udp = with_udp address (\\socket -> IO.pure (Net.port (Net.udp_local_addr socket)))
    do! IO.write_line (Result.is_ok (&connected))
    do! IO.write_line (Result.is_ok (&listened))
    do! IO.write_line (Result.is_ok (&udp))
    let _accepting = with_accepted
    0
",
    );
}

#[test]
fn error_kind_match_is_exhaustive() {
    let all = "def label :: Net.ErrorKind -> i64
fn label kind =
    match kind with
    | Net.TimedOut -> 1
    | Net.ConnectionRefused -> 2
    | Net.ConnectionReset -> 3
    | Net.AddressInUse -> 4
    | Net.AddressNotAvailable -> 5
    | Net.Unreachable -> 6
    | Net.Unclassified -> 7
";
    module(all);
    let missing = all.replace("    | Net.Unclassified -> 7\n", "");
    let error = analyze_modules(&[("Main.tz", &missing)]).unwrap_err();
    assert_eq!(error.code, "E1021", "{}", error.message);
}

#[test]
fn declares_only_reached_net_builtins() {
    // The address functions are pure Tsuzuri: no runtime on any target.
    let pure = module(
        "export def port_of :: i64\nfn port_of =\n    let text = \"[::1]:8080\"\n    match Net.parse_address (&text) with\n    | Maybe.Some address -> Net.port address + String.length (&Net.address_text address)\n    | Maybe.None -> 0\n",
    );
    for wasm in [false, true] {
        let ir = ir(&pure, llvm::Entry::Library, wasm);
        assert!(!ir.contains("tsuzuri_net_"), "{ir}");
        assert!(!ir.contains("tsuzuri_os_"), "{ir}");
        assert!(!ir.contains("wasm-import-module"), "{ir}");
    }
    // Only the primitives that a program reaches are declared, and Net needs no operating-system runtime.
    let only_close = module(
        "export def closer :: i64\nfn closer =\n    let action = Net.close_listener\n    0\n",
    );
    for wasm in [false, true] {
        let ir = ir(&only_close, llvm::Entry::Library, wasm);
        assert!(!ir.contains("@tsuzuri_net_open"), "{ir}");
        assert!(!ir.contains("tsuzuri_os_"), "{ir}");
    }
    let connecting = module(
        "def main :: unit -> i32 = \\() ->\n    let address = Maybe.get (Net.parse_address (&\"127.0.0.1:9\"))\n    let! opened = Net.connect address Maybe.None\n    do! IO.write_line (Result.is_ok (&opened))\n    0\n",
    );
    for wasm in [false, true] {
        let ir = ir(&connecting, llvm::Entry::Library, wasm);
        assert!(
            ir.contains("declare i64 @tsuzuri_net_open(ptr, i32, i64, i64, i64, i64)\n"),
            "{ir}"
        );
        assert!(!ir.contains("@tsuzuri_net_accept"), "{ir}");
        assert!(!ir.contains("tsuzuri_os_"), "{ir}");
    }
}

#[test]
fn net_declarations_are_effectful() {
    let program = module(
        "def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (&\"127.0.0.1:9\"))
    let! opened = Net.connect address Maybe.None
    match opened with
    | Result.Error error -> do! IO.write_line (Net.error_kind error)
    | Result.Ok stream ->
        let! written = Net.write stream [1ubyte] Maybe.None
        let! received = Net.read stream 10 Maybe.None
        let! shut = Net.shutdown stream Net.Both
        let! closed = Net.close stream
        do! IO.write_line (Result.is_ok (&written))
        do! IO.write_line (Result.is_ok (&received))
        do! IO.write_line (Result.is_ok (&shut))
        do! IO.write_line (Result.is_ok (&closed))
    let! listener = Net.bind address
    match listener with
    | Result.Ok bound ->
        let! accepted = Net.accept bound Maybe.None
        do! IO.write_line (Result.is_ok (&accepted))
    | Result.Error _ -> do! IO.write_line \"x\"
    let! resolved = Net.resolve \"localhost\" 1
    do! IO.write_line (Result.is_ok (&resolved))
    0
",
    );
    for wasm in [false, true] {
        let ir = ir(&program, llvm::Entry::Library, wasm);
        let declared: Vec<_> = ir
            .lines()
            .filter(|line| line.contains("@tsuzuri_net_") && line.starts_with("declare "))
            .collect();
        assert_eq!(declared.len(), 7, "{declared:?}");
        for line in declared {
            for attribute in ["readnone", "readonly", "memory(", "wasm-import"] {
                assert!(!line.contains(attribute), "{line}");
            }
        }
    }
}

#[test]
fn brackets_close_the_handle_they_open() {
    let program = module(
        "def serve :: Net.TcpStream -> IO<i64>
fn serve stream = IO.pure 1
def main :: unit -> i32 = \\() ->
    let address = Maybe.get (Net.parse_address (&\"127.0.0.1:9\"))
    let! used = Net.with_connection address (Maybe.Some 5) serve
    do! IO.write_line (Result.is_ok (&used))
    0
",
    );
    let ir = ir(&program, llvm::Entry::Library, false);
    assert!(ir.contains("declare i64 @tsuzuri_net_open("), "{ir}");
    assert!(
        ir.contains("declare i64 @tsuzuri_net_close(i32, i64)\n"),
        "{ir}"
    );
}

#[test]
fn net_is_opt_in_and_costs_other_programs_nothing() {
    // A program that does not name Net neither loads it nor emits anything of it.
    let plain = module("def main :: unit -> i32 = \\() ->\n    do! IO.write_line 42\n    0\n");
    for wasm in [false, true] {
        let ir = ir(&plain, llvm::Entry::Library, wasm);
        assert!(!ir.contains("Net"), "{ir}");
        assert!(!ir.contains("tsuzuri_net_"), "{ir}");
    }
    // The module is reached through its name only.
    let error = analyze("let kind: ErrorKind = Net.Unclassified\n0").unwrap_err();
    assert!(
        matches!(error.code, "E1002" | "E1004" | "E1003"),
        "{}: {}",
        error.code,
        error.message
    );
}
