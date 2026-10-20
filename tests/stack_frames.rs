//! PM09: stack frames of literals, string buffers and closure environments (lifetime markers on literals, per-function budget).

use tsuzuri::{analyze, llvm};

fn emit(source: &str, wasm: bool) -> String {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    let first = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        first,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    first
}

fn body_of<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split_once(&format!("@tz.fn.Main.{name}("))
        .unwrap_or_else(|| panic!("missing {name}"))
        .1
        .split("\n}")
        .next()
        .unwrap()
}

fn function_ir(source: &str, name: &str) -> String {
    body_of(&emit(source, false), name).to_owned()
}

/// The names of the allocas of type `ty` in `body`, in order.
fn allocas(body: &str, ty: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(" = alloca ")?;
            (rest == format!("{ty}, align 16")).then(|| name.to_owned())
        })
        .collect()
}

/// The lines of block `label` up to the next label, the label excluded.
fn block<'a>(body: &'a str, label: &str) -> Vec<&'a str> {
    body.lines()
        .skip_while(|line| *line != format!("{label}:"))
        .skip(1)
        .take_while(|line| line.starts_with(' ') || !line.ends_with(':'))
        .collect()
}

fn marker(kind: &str, bytes: usize, address: &str) -> String {
    format!("  call void @llvm.lifetime.{kind}.p0(i64 {bytes}, ptr {address})")
}

/// The labels `(yes, no)` of the branch on the test of the frame at `address`, whose value still
/// holds the frame when the test is true.
fn frame_test(body: &str, address: &str) -> (String, String) {
    let lines: Vec<_> = body.lines().collect();
    let test = lines
        .iter()
        .position(|line| line.contains("icmp eq ptr") && line.ends_with(&format!(", {address}")))
        .unwrap_or_else(|| panic!("no frame test of {address}\n{body}"));
    let branch = lines[test..]
        .iter()
        .find(|line| line.trim_start().starts_with("br i1"))
        .unwrap();
    let labels: Vec<_> = branch
        .split("label %")
        .skip(1)
        .map(|label| label.trim_end_matches([',', ' ']).to_owned())
        .collect();
    (labels[0].clone(), labels[1].clone())
}

/// Checks the start and the end of the frame at `address`: one start, which is the first line of the
/// body that uses the frame, and one end, in the branch of its test that holds the frame (and not in
/// the branch that frees the value).
fn assert_bracketed(body: &str, address: &str, bytes: usize) {
    let start = marker("start", bytes, address);
    let end = marker("end", bytes, address);
    let lines: Vec<_> = body.lines().collect();
    assert_eq!(
        lines.iter().filter(|line| **line == start).count(),
        1,
        "{body}"
    );
    assert_eq!(
        lines.iter().filter(|line| **line == end).count(),
        1,
        "{body}"
    );
    let loop_label = lines.iter().position(|line| *line == "loop:").unwrap();
    let first_use = lines[loop_label..]
        .iter()
        .find(|line| line.contains(&format!("ptr {address}")))
        .unwrap();
    assert_eq!(
        *first_use, start,
        "the lifetime starts before the first use\n{body}"
    );
    let (yes, no) = frame_test(body, address);
    assert!(block(body, &yes).contains(&end.as_str()), "{body}");
    assert!(
        !block(body, &no)
            .iter()
            .any(|line| line.contains("lifetime")),
        "{body}"
    );
}

const TOTAL: &str = "def total :: i64 -> i64
fn total n = {
    let a = [|n, n + 1, n + 2|];
    let b = List.length a;
    let c = [|b, b, b, b|];
    List.length c + b
}";

#[test]
fn brackets_binding_literals_with_lifetime_markers() {
    let body = function_ir(TOTAL, "total");
    // A chunk of `i64` is a 24-byte header, then 8 bytes for each slot.
    for (ty, bytes) in [
        ("{ ptr, i64, i64, [3 x i64] }", 24 + 3 * 8),
        ("{ ptr, i64, i64, [4 x i64] }", 24 + 4 * 8),
    ] {
        let found = allocas(&body, ty);
        assert_eq!(found.len(), 1, "{body}");
        assert_bracketed(&body, &found[0], bytes);
    }
    assert_eq!(body.matches("@llvm.lifetime.").count(), 4, "{body}");
}

#[test]
fn brackets_temporary_literals_and_loop_literals() {
    // The two literals of the branches are never alive together: one pair of markers each.
    let body = function_ir(
        "def f :: i64 -> i64
         fn f n =
             if n > 0 then
                 let a = [|n, n, n, n|]
                 List.length a + n
             else
                 let b = [|1, 2, 3, n|]
                 List.length b - n",
        "f",
    );
    let found = allocas(&body, "{ ptr, i64, i64, [4 x i64] }");
    assert_eq!(found.len(), 2, "{body}");
    for address in &found {
        assert_bracketed(&body, address, 24 + 4 * 8);
    }
    assert_eq!(body.matches("@llvm.lifetime.").count(), 4, "{body}");
    // A temporary operand has a frame too: the iterated literal.
    let body = function_ir(
        "def f :: i64 -> i64
         fn f n = {
             let mut total = 0;
             for v in [|n, 2, 3|] do total = total + v;
             total
         }",
        "f",
    );
    let found = allocas(&body, "{ ptr, i64, i64, [3 x i64] }");
    assert_eq!(found.len(), 1, "{body}");
    assert_bracketed(&body, &found[0], 24 + 3 * 8);
    // A literal in a loop keeps its storage in the entry block; each iteration has its own pair.
    let body = function_ir(
        "def rec count :: i64 -> i64 -> i64
         fn rec count n acc =
             if n == 0 then acc
             else
                 let xs = [|n, n, n|]
                 count (n - 1) (acc + List.length xs)",
        "count",
    );
    let (entry, rest) = body.split_once("\nloop:\n").unwrap();
    let found = allocas(entry, "{ ptr, i64, i64, [3 x i64] }");
    assert_eq!(found.len(), 1, "{body}");
    assert!(!rest.contains("alloca"), "{body}");
    assert_bracketed(&body, &found[0], 24 + 3 * 8);
    assert!(rest.contains("br label %loop"), "{body}");
}

#[test]
fn leaves_the_frames_that_one_value_may_hold_without_markers() {
    // A frame test compares the address of a value with the address of each frame, and LLVM gives
    // two allocas whose lifetimes are disjoint one slot: the test of the first frame would pass for a
    // value in the second. So the frames of the arms of an `if` or a `match`, and the frames of a
    // mutable binding, whose value an assignment drops early, have no markers.
    for (source, ty, count) in [
        (
            "def f :: i64 -> i64
             fn f n = {
                 let a = if n > 0 then [|n, n|] else [|1, 2|];
                 List.length a
             }",
            "{ ptr, i64, i64, [2 x i64] }",
            2,
        ),
        (
            "def f :: i64 -> i64
             fn f n = {
                 let a = match n with | 0 -> [|n, n|] | 1 -> [|1, 2|] | _ -> [|3, n|];
                 List.length a
             }",
            "{ ptr, i64, i64, [2 x i64] }",
            3,
        ),
        (
            "def f :: i64 -> i64
             fn f n = {
                 let mut a = [|n, n|];
                 a = [|1, n|];
                 List.length a
             }",
            "{ ptr, i64, i64, [2 x i64] }",
            1,
        ),
        (
            "def f :: i64 -> i64
             fn f n = {
                 let a = if n > 0 then [n, n] else [1, 2];
                 a.length
             }",
            "[2 x i64]",
            2,
        ),
    ] {
        let body = function_ir(source, "f");
        assert_eq!(allocas(&body, ty).len(), count, "{body}");
        assert!(!body.contains("@llvm.lifetime."), "{body}");
    }
    // The count of such frames returns to zero afterwards: the binding after them has its markers.
    let body = function_ir(
        "def f :: i64 -> i64
         fn f n = {
             let mut a = [|n, n|];
             a = [|1, n|];
             let b = if n > 0 then [|n, n|] else [|1, 2|];
             let c = [|n, n, n|];
             List.length a + List.length b + List.length c
         }",
        "f",
    );
    let found = allocas(&body, "{ ptr, i64, i64, [3 x i64] }");
    assert_eq!(found.len(), 1, "{body}");
    assert_bracketed(&body, &found[0], 24 + 3 * 8);
    assert_eq!(body.matches("@llvm.lifetime.").count(), 2, "{body}");
    // The one arm of a `match` is the one frame of its value.
    let body = function_ir(
        "def f :: i64 -> i64
         fn f n = {
             let a = match (n, n) with | (x, y) -> [|x, y|];
             List.length a
         }",
        "f",
    );
    let found = allocas(&body, "{ ptr, i64, i64, [2 x i64] }");
    assert_eq!(found.len(), 1, "{body}");
    assert_bracketed(&body, &found[0], 24 + 2 * 8);
}

#[test]
fn keeps_the_markers_of_locals_that_an_if_chooses_between() {
    // Both locals start before the `if` and neither ends before it, so LLVM keeps them apart.
    let body = function_ir(
        "def f :: i64 -> i64
         fn f n = {
             let a = [|n, n|];
             let b = [|1, n|];
             let c = if n > 0 then a else b;
             List.length c
         }",
        "f",
    );
    let found = allocas(&body, "{ ptr, i64, i64, [2 x i64] }");
    assert_eq!(found.len(), 2, "{body}");
    for address in &found {
        assert_eq!(
            body.matches(&marker("start", 24 + 2 * 8, address)).count(),
            1,
            "{body}"
        );
    }
}

#[test]
fn leaves_the_frames_of_simd_vectors_without_markers() {
    // `layout_bound` gives a vector the portable alignment of at most 8, where wasm32 aligns a
    // 16-byte vector to 16: the alloca of `[1 x <4 x i32>]` is 48 bytes and its marker would say 40.
    // No frame that holds a vector, itself or in a record, has markers, on either target.
    for wasm in [false, true] {
        let ir = emit(
            "record Masked { small: i8, mask: mask32x4, vector: i32x4 }
             def vector_list :: i64 -> i64
             fn vector_list n = { let v: i32x4 = Simd.splat (n as i32); let xs = [|v|]; List.length xs }
             def vector_array :: i64 -> i64
             fn vector_array n = { let v: i32x4 = Simd.splat (n as i32); let xs = [v, v]; xs.length }
             def masked :: i64 -> i64
             fn masked n = {
                 let v: i32x4 = Simd.splat (n as i32);
                 let mask = Simd.eq v v;
                 let xs = [Masked { small: 1i8, mask: mask, vector: v }];
                 xs.length
             }
             def plain :: i64 -> i64
             fn plain n = { let xs = [|n, n|]; List.length xs }",
            wasm,
        );
        for (name, ty) in [
            ("vector_list", "{ ptr, i64, i64, [1 x <4 x i32>] }"),
            ("vector_array", "[2 x <4 x i32>]"),
            ("masked", "[1 x %tz.record.Main.Masked]"),
        ] {
            let body = body_of(&ir, name);
            assert_eq!(allocas(body, ty).len(), 1, "{body}");
            assert!(!body.contains("@llvm.lifetime."), "{body}");
        }
        // A literal without a vector in the same module has them.
        let plain = body_of(&ir, "plain");
        assert_eq!(plain.matches("@llvm.lifetime.").count(), 2, "{plain}");
    }
}

#[test]
fn leaves_buffers_and_environments_without_markers_and_skips_unsettled_layouts() {
    for wasm in [false, true] {
        let ir = emit(
            "def f :: string -> i64
             fn f a = { let joined = a + \"xyz\"; joined.length + joined[0] as i64 }
             def g :: i64 -> i64
             fn g n = { let a = n + 1; let b = n * 2; let add = \\i -> i + a + b; add 1 + add 2 }
             def wide :: i128 -> i64
             fn wide n = { let values = [|n, n + 1|]; List.length values }",
            wasm,
        );
        // The buffers and the environments are heap values on a Windows host. They have allocas and
        // no markers: an alloca that has them keeps LLVM's inliner from adding its own around the
        // body of an inlined function, and these frames are released on paths that carry no `end`.
        if !(cfg!(windows) && !wasm) {
            let f = body_of(&ir, "f");
            assert_eq!(allocas(f, "[256 x i8]").len(), 1, "{f}");
            assert!(!f.contains("@llvm.lifetime."), "{f}");
            let g = body_of(&ir, "g");
            assert_eq!(allocas(g, "{ i64, i64 }").len(), 1, "{g}");
            assert!(!g.contains("@llvm.lifetime."), "{g}");
        }
        // LLVM versions disagree about the alignment of an `i128`, so its list has no markers.
        let wide = body_of(&ir, "wide");
        assert_eq!(
            allocas(wide, "{ ptr, i64, i64, [2 x i128] }").len(),
            1,
            "{wide}"
        );
        assert!(!wide.contains("@llvm.lifetime."), "{wide}");
    }
}

#[test]
fn lifetime_markers_do_not_consume_trap_sites() {
    let module = analyze(TOTAL).unwrap();
    let sources = [tsuzuri::trap::TrapSource {
        path: "Main.tz",
        text: TOTAL,
    }];
    let markers = |ir: &str| -> Vec<String> {
        ir.lines()
            .filter(|line| line.contains("call void @llvm.lifetime."))
            .map(str::to_owned)
            .collect()
    };
    for wasm in [false, true] {
        let options = llvm::EmitOptions {
            entry: llvm::Entry::Library,
            wasm,
            debug_output: false,
            runtime_objects: false,
            wasm_simd128: false,
            allocator: llvm::Allocator::System,
            f16: llvm::F16Path::Soft,
        };
        let plain = llvm::emit_with_options(&module, options).unwrap();
        assert_eq!(markers(&plain).len(), 4);
        // The instrumentation leaves the markers as they are: no site, no context argument.
        let traced = llvm::emit_with_trap_info(&module, options, &sources).unwrap();
        assert_eq!(markers(&traced.ir), markers(&plain));
        assert!(!traced.ir.contains("!tz.site"));
        // With line tables the markers have no location, while the calls around them do.
        let debugged = llvm::emit_with_debug_info(&module, options, &sources, false, true).unwrap();
        assert_eq!(markers(&debugged.ir), markers(&plain));
        assert!(
            debugged
                .ir
                .lines()
                .any(|line| line.contains("call void @tz.free(") && line.contains("!dbg")),
            "{}",
            debugged.ir
        );
    }
}

#[test]
fn declares_lifetime_intrinsics_once_with_runtime_ir() {
    // The numeric and the math runtime of a module declare the intrinsics too, and a module can declare each once.
    let frames = "let values = [|n, n + 1|]; List.length values";
    let half = "let half = (n as f16) + (n as f16)";
    for (what, body) in [
        ("no runtime", frames.to_owned()),
        ("numeric", format!("{half}; {frames} + (half as i64)")),
        ("math", format!("{frames} + (Math.sin (n as f64) as i64)")),
        (
            "numeric and math",
            format!("{half}; {frames} + (half as i64) + (Math.sin (n as f64) as i64)"),
        ),
    ] {
        let source = format!("def f :: i64 -> i64\nfn f n = {{ {body} }}");
        for wasm in [false, true] {
            let ir = emit(&source, wasm);
            assert!(ir.contains("call void @llvm.lifetime.start.p0("), "{what}");
            let names: Vec<_> = ir
                .lines()
                .filter(|line| line.starts_with("declare "))
                .map(|line| line.split_once('@').unwrap().1.split('(').next().unwrap())
                .collect();
            for kind in ["start", "end"] {
                let name = format!("llvm.lifetime.{kind}.p0");
                assert_eq!(
                    names.iter().filter(|declared| **declared == name).count(),
                    1,
                    "{what}, wasm {wasm}"
                );
            }
            let unique: std::collections::BTreeSet<_> = names.iter().collect();
            assert_eq!(names.len(), unique.len(), "{what}, wasm {wasm}: {names:?}");
        }
    }
}

fn list_literal(count: usize) -> String {
    let values: Vec<_> = (0..count).map(|value| format!("{value}l")).collect();
    format!("[|{}|]", values.join(", "))
}

/// The function `f` that binds each literal in turn and then adds their lengths.
fn bindings(literals: &[usize]) -> String {
    let mut body = String::new();
    for (index, count) in literals.iter().enumerate() {
        body.push_str(&format!("let l{index} = {}; ", list_literal(*count)));
    }
    let lengths: Vec<_> = (0..literals.len())
        .map(|index| format!("List.length l{index}"))
        .collect();
    format!(
        "def f :: i64 -> i64\nfn f n = {{ {body}{} + n }}",
        lengths.join(" + ")
    )
}

// A list literal counts 16 + 16 bytes for each `i64` element (a node of 16 bytes and the element
// rounded up to 16), so a literal of `n` elements counts 32 * n bytes.
#[test]
fn keeps_literals_on_the_stack_within_the_function_budget() {
    // 5 literals of 600 elements count 19,200 bytes each: the first three (57,600 bytes) fit in the
    // 65,536 bytes of a function, and a fourth would make 76,800.
    for wasm in [false, true] {
        let ir = emit(&bindings(&[600; 5]), wasm);
        let body = body_of(&ir, "f");
        assert_eq!(
            allocas(body, "{ ptr, i64, i64, [600 x i64] }").len(),
            3,
            "{body}"
        );
        // Each of the two other literals is a list that is built at once, one chunk on the heap.
        assert_eq!(body.matches("call ptr @tz.alloc(").count(), 2, "{body}");
    }
}

#[test]
fn budget_boundary_is_inclusive() {
    for wasm in [false, true] {
        // Two literals of 1,024 elements count 32,768 bytes each, which is the whole budget: both stay on
        // the stack, and a literal of one more element (32 bytes) does not.
        let ir = emit(&bindings(&[1024, 1024, 1]), wasm);
        let body = body_of(&ir, "f");
        assert_eq!(
            allocas(body, "{ ptr, i64, i64, [1024 x i64] }").len(),
            2,
            "{body}"
        );
        assert!(
            allocas(body, "{ ptr, i64, i64, [1 x i64] }").is_empty(),
            "{body}"
        );
        assert_eq!(body.matches("call ptr @tz.alloc(").count(), 1, "{body}");
        // One element more than the budget has room for (32,800 bytes after 32,768) goes to the heap.
        let ir = emit(&bindings(&[1024, 1025]), wasm);
        let body = body_of(&ir, "f");
        assert_eq!(
            allocas(body, "{ ptr, i64, i64, [1024 x i64] }").len(),
            1,
            "{body}"
        );
        assert!(
            allocas(body, "{ ptr, i64, i64, [1025 x i64] }").is_empty(),
            "{body}"
        );
        assert_eq!(body.matches("call ptr @tz.alloc(").count(), 1, "{body}");
    }
}

#[test]
fn literals_buffers_and_environments_share_the_function_budget() {
    let concatenation = |name: &str, suffix: &str| {
        format!("let {name} = (if n > 0 then \"ab\" else \"abc\") + \"{suffix}\"; ")
    };
    let lengths = ["j1", "j2", "j3"]
        .map(|name| format!("{name}.length"))
        .join(" + ");
    for wasm in [false, true] {
        // The buffers and the environments are heap values on a Windows host.
        if cfg!(windows) && !wasm {
            continue;
        }
        // Two literals of 1,016 elements count 65,024 bytes. Of three buffers of 256 bytes the first
        // two make 65,536 bytes, and the third does not fit.
        let literals = [1016, 1016]
            .iter()
            .enumerate()
            .map(|(index, count)| format!("let l{index} = {}; ", list_literal(*count)))
            .collect::<String>();
        let source = format!(
            "def f :: i64 -> i64\nfn f n = {{ {literals}{}{}{}List.length l0 + List.length l1 + {lengths} }}",
            concatenation("j1", "x"),
            concatenation("j2", "y"),
            concatenation("j3", "z"),
        );
        let ir = emit(&source, wasm);
        let body = body_of(&ir, "f");
        assert_eq!(
            allocas(body, "{ ptr, i64, i64, [1016 x i64] }").len(),
            2,
            "{body}"
        );
        assert_eq!(allocas(body, "[256 x i8]").len(), 2, "{body}");
        assert_eq!(body.matches(".concat_into(").count(), 2, "{body}");
        assert_eq!(body.matches(".concat(").count(), 1, "{body}");
        // The buffers come first here: 768 bytes, a literal of 32,512 (33,280 in all), and the second
        // literal would make 65,792.
        let source = format!(
            "def f :: i64 -> i64\nfn f n = {{ {}{}{}{literals}List.length l0 + List.length l1 + {lengths} }}",
            concatenation("j1", "x"),
            concatenation("j2", "y"),
            concatenation("j3", "z"),
        );
        let ir = emit(&source, wasm);
        let body = body_of(&ir, "f");
        assert_eq!(allocas(body, "[256 x i8]").len(), 3, "{body}");
        assert_eq!(
            allocas(body, "{ ptr, i64, i64, [1016 x i64] }").len(),
            1,
            "{body}"
        );
        assert_eq!(body.matches("call ptr @tz.alloc(").count(), 1, "{body}");
    }
}
