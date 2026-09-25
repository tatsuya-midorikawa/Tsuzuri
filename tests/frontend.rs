use tsuzuri::{analyze, analyze_modules, llvm};

#[test]
fn all_semantics_fixture_functions_lower_deterministically() {
    let source = include_str!("fixtures/Semantics.tz");
    let module = analyze(source).unwrap();
    let first = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let second = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("call %tz.closure @tz.fn.Main.select_function"));
    assert!(first.contains("@llvm.fptosi.sat.i64.f64"));
}

#[test]
fn deterministic_source_mutations_do_not_panic() {
    let originals = [
        "fn rec f(n: i64) -> i64 { if n == 0 { 1 } else { f(n - 1) } }",
        "record R { x: i64 } export fn f() -> i64 { let r = R { x: 1 }; r.x }",
        "fn f(x: [i64]) -> i64 { let a = [true, false]; if a[0] { x[1] } else { 0 } }",
        "fn id(x: i64) -> i64 { x } fn f() -> i64 { let g: fn(i64) -> i64 = id; 42 |> g }",
        "def add :: 'a -> 'a -> 'a\nfn add x y = x + y\ndef main :: i32\nfn main = add 20 22",
        "class C 'a { def f :: 'a -> i64 }\ninstance C bool { fn f x = 1 }\nC.f true",
        "def add :: Add 'a -> 'a -> 'a\nlet add = x -> y -> x + y\ndef main :: i32\nfn main = { let f = add 20; f 22 }",
        "def main :: string\nfn main = { let prefix = \"x\"; let f: string -> string = y -> prefix + y; f \"z\" }",
        "def f :: i64 -> [i32]\nfn f n = new [i32](n, i -> i as i32)",
        "def rec total :: i64 -> i64 -> i64\nfn rec total n sum = match n with | 0 -> sum | n when n > 0 -> total (n - 1) (sum + n) | _ -> sum",
        "let mut sum = 0\nfor i = 1 to 3 do { sum = sum + i as i64; }\nwhile sum < 8 do sum = sum + 1\nsum",
        "let f = fx (x, y) -> match x with | 0 | 1 -> y | _ -> x + y\nf (1, 2)",
        "def (|Even|_|) :: i64 -> bool\nfn (|Even|_|) n = n % 2 == 0\nmatch 2 with | Even as n when n > 0 -> n | _ -> 0",
        "union Maybe 'a = None | Some of 'a\nmatch Some (true, [|1|]) with | Some (true, [||]) -> 0 | Some (_, x :: _) -> x | Some (false, _) -> 1 | None -> 2",
        "def bump :: ref mut i64 -> i64 -> unit\nfn bump r n = deref r = deref r + n\nlet mut x = 1\nbump (ref mut x) 2\nbump &mut x 3\nlet t = ref x\nlet u: &&i64 = &t\nderef t + *t",
    ];
    let mut seed = 0x1357_2468_u32;
    for original in originals {
        for _ in 0..500 {
            let mut bytes = original.as_bytes().to_vec();
            for _ in 0..3 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let position = seed as usize % bytes.len();
                match seed % 3 {
                    0 => {
                        bytes.remove(position);
                    }
                    1 => {
                        bytes.insert(position, (32 + seed % 95) as u8);
                    }
                    _ => {
                        bytes[position] = (32 + seed % 95) as u8;
                    }
                }
            }
            let source = String::from_utf8(bytes).unwrap();
            if let Ok(module) = analyze(&source) {
                llvm::emit(&module, llvm::Entry::Library).unwrap();
            }
        }
    }
}

#[test]
fn warning_rendering_uses_source_path() {
    let helpers =
        "def pick :: bool -> i64\nfn pick b =\n    match b with\n    | _ -> 0\n    | true -> 1\n";
    let module =
        analyze_modules(&[("Helpers.tz", helpers), ("Main.tz", "Helpers.pick true")]).unwrap();
    let [warning] = module.warnings.as_slice() else {
        panic!("{:?}", module.warnings);
    };
    assert_eq!(warning.span.source, Some(0));
    assert_eq!(&helpers[warning.span.start..warning.span.end], "true");
    assert_eq!(
        warning.render_with_severity("warning", "Helpers.tz", helpers),
        "Helpers.tz:5:7: warning[W1003]: unreachable match arm; previous patterns already cover this arm\n  5 |     | true -> 1\n    |       ^"
    );
    assert!(
        warning
            .json_with_severity("warning", "Helpers.tz", helpers)
            .starts_with("{\"severity\":\"warning\",\"code\":\"W1003\",")
    );
}
