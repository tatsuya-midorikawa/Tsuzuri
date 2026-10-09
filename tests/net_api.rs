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
