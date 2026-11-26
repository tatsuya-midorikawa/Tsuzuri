use tsuzuri::{analyze, llvm};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.into_data().message
}

fn emits(source: &str) -> [String; 2] {
    let module = analyze(source).unwrap_or_else(|error| {
        let data = error.into_data();
        panic!(
            "{source}\n{}: {} at {:?}",
            data.code,
            data.message,
            &source[data.span.start..]
        );
    });
    [false, true].map(|wasm| {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap(),
            "{source}: IR is deterministic"
        );
        assert!(
            llvm::duplicate_declarations(&ir).is_empty(),
            "{source}: declarations"
        );
        ir
    })
}

#[test]
fn shares_borrowed_slices_tuples_and_tensor_views_without_copying() {
    // 1. Single `ref [f64]`, tuple `ref (ref [f64] * ref [f64])`, and `ref Tensor.View<f64>`.
    let [_, wasm_ir] = emits(
        r#"let a = [1.0, 2.0, 3.0, 4.0]
let b = [10.0, 20.0, 30.0, 40.0]
let mut out1 = [0.0, 0.0, 0.0, 0.0]
Parallel.for_each_chunk_with 2 (ref a) (\src start chunk ->
    for i in 0i64 ..= (chunk.length - 1) do
        Array.write i (src[start + i] * 2.0) chunk) (ref mut out1)

let inputs = (ref a, ref b)
let mut out2 = [0.0, 0.0, 0.0, 0.0]
Parallel.for_each_chunk_with 2 (ref inputs) (\pair start chunk ->
    match pair with
    | (la, rb) ->
        for i in 0i64 ..= (chunk.length - 1) do
            Array.write i (la[start + i] + rb[start + i]) chunk) (ref mut out2)

let t = Tensor.of_array [2, 2] [5.0, 6.0, 7.0, 8.0]
let tv = Tensor.view (ref t)
let mut out3 = [0.0, 0.0, 0.0, 0.0]
Parallel.for_each_chunk_with 2 (ref tv) (\view start chunk ->
    for i in 0i64 ..= (chunk.length - 1) do
        let r = (start + i) / 2
        let c = (start + i) % 2
        Array.write i view[(r, c)] chunk) (ref mut out3)

out1[3] + out2[3] + out3[3]"#,
    );
    // Default WASM uses tsuzuri_task_parallel with no JSPI or host imports.
    assert!(wasm_ir.contains("@tsuzuri_task_parallel"));
    assert!(!wasm_ir.contains("@tsuzuri_tz_scope_run"));

    // 2. Runtime execution at -O0 and -O3 with `--allocator counting` proving 0 input-copy allocations!
    let prog = r#"def main :: unit -> i32 = \() ->
    let a = new [f64](16, \i -> (i as f64) + 1.0)
    let b = new [f64](16, \i -> (i as f64) * 10.0)
    let mut out = new [f64](16, \_ -> 0.0)
    let inputs = (ref a, ref b)
    let s0 = Alloc.snapshot ()
    Parallel.for_each_chunk_with 4 (ref inputs) (\pair start chunk ->
        match pair with
        | (la, rb) ->
            for i in 0i64 ..= (chunk.length - 1) do
                Array.write i (la[start + i] + rb[start + i]) chunk) (ref mut out)
    let u0 = Alloc.since s0

    // Matrix.mul_parallel with > 1 chunk (rows = 300, inner = 32, cols = 64 -> rows_per_chunk = 512,
    // so use rows = 600, inner = 64, cols = 64 -> rows_per_chunk = 1048576 / 4096 = 256 -> 3 chunks!)
    let m_left = Matrix.init 600 64 (\r c -> ((r + c) % 7) as i64)
    let m_right = Matrix.init 64 64 (\r c -> ((r * 3 + c) % 5) as i64)
    let s1 = Alloc.snapshot ()
    let prod = Matrix.mul_parallel (ref m_left) (ref m_right)
    let u1 = Alloc.since s1
    let seq = Matrix.mul (ref m_left) (ref m_right)
    assert (prod[(599, 63)] == seq[(599, 63)])
    // u1.allocs == 1: ONLY the 600x64 output buffer (307200 bytes), zero input copies, zero closure snapshots!
    do! IO.writeln ($"{u0.allocs}:{u1.allocs}:{u1.bytes}:{out[15] == 166.0}")
    0
"#;
    let dir = std::env::temp_dir().join(format!("tsuzuri-nc15-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(&main_tz, prog).unwrap();
    for opt in ["-O0", "-O3"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["run", "--no-cache", "--allocator", "counting", opt])
            .arg(&main_tz)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "nc15 {opt}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "0:2:614400:true",
            "nc15 {opt}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_aliasing_borrowed_closures_unsync_impure_and_partial_application() {
    // 1. Aliasing `shared` and `values` (direct, via tuple, and via Tensor.View) is E1014.
    rejects(
        r#"let mut xs = [1.0, 2.0, 3.0, 4.0]
Parallel.for_each_chunk_with 2 (ref xs) (\_ _ _ -> ()) (ref mut xs)
0"#,
        "E1014",
    );
    rejects(
        r#"let mut xs = [1.0, 2.0, 3.0, 4.0]
let ys = [5.0, 6.0, 7.0, 8.0]
let inputs = (ref xs, ref ys)
Parallel.for_each_chunk_with 2 (ref inputs) (\_ _ _ -> ()) (ref mut xs)
0"#,
        "E1014",
    );
    rejects(
        r#"let mut xs = [1.0, 2.0, 3.0, 4.0]
let tv = Tensor.borrow [2, 2] (ref xs)
Parallel.for_each_chunk_with 2 (ref tv) (\_ _ _ -> ()) (ref mut xs)
0"#,
        "E1014",
    );

    // 2. Unproven borrowed closure environment in `shared` (direct, record, and nested `ref (i64 -> i64)`) is E1013.
    let prelude = "let rc = Rc.new 5i64\nlet r = ref rc\nlet f = \\x -> { let again = Rc.share r; deref (Rc.get (ref again)) + x }\nlet mut out = [0, 0, 0, 0]\n";
    let msg1 = rejects(
        &format!(
            "{prelude}Parallel.for_each_chunk_with 2 (ref f) (\\fn_ref start chunk -> Array.write 0 ((deref fn_ref) start) chunk) (ref mut out)\n0"
        ),
        "E1013",
    );
    assert!(msg1.contains("Parallel.for_each_chunk_with"), "{msg1}");

    let msg2 = rejects(
        &format!(
            "record FnRef {{ r }} {{ rf: ref {{ r }} (i64 -> i64) }}\n{prelude}let h = FnRef {{ rf: ref f }}\nParallel.for_each_chunk_with 2 (ref h) (\\holder start chunk -> Array.write 0 ((deref holder.rf) start) chunk) (ref mut out)\n0"
        ),
        "E1013",
    );
    assert!(msg2.contains("Parallel.for_each_chunk_with"), "{msg2}");

    // Owned closure environment is accepted.
    emits(
        r#"let base = 10i64
let mut out = [0, 0, 0, 0]
Parallel.for_each_chunk_with 2 (ref (\x -> x + base)) (\fn_ref start chunk -> Array.write 0 ((deref fn_ref) start) chunk) (ref mut out)
out[0]"#,
    );

    // 3. Callback capturing an outer borrow directly is E1013.
    rejects(
        r#"let ys = [1, 2, 3, 4]
let r = ref ys
let s = 10
let mut out = [0, 0, 0, 0]
Parallel.for_each_chunk_with 2 (ref s) (\_ start chunk -> Array.write 0 r[start] chunk) (ref mut out)
0"#,
        "E1013",
    );

    // 4. Output element `'a` requires `Send<'a>`, so storing references in `chunk` is rejected (`E1013`).
    rejects(
        r#"let z = 0
let x = 99
let mut out = [ref z]
Parallel.for_each_chunk_with 1 (ref x) (\s _ chunk -> Array.write 0 s chunk) (ref mut out)
deref out[0]"#,
        "E1013",
    );

    // 5. `!Sync` shared (`Rc`, `ref mut`), `extern type local` in callback, `pure def`, partial application,
    // and keeping `Routine.scope` strict on `Tensor.View`.
    rejects(
        r#"let rc = Rc.new 1
let mut out = [0, 0]
Parallel.for_each_chunk_with 1 (ref rc) (\_ _ _ -> ()) (ref mut out)
0"#,
        "E1013",
    );
    rejects(
        r#"extern type local Handle
extern "h_use" def use_h :: Handle -> i64
extern "h_mk" def mk_h :: unit -> Handle
let s = 1
let mut out = [0, 0]
Parallel.for_each_chunk_with 1 (ref s) (\_ _ chunk -> Array.write 0 (use_h (mk_h ())) chunk) (ref mut out)
0"#,
        "E1013",
    );
    // Pure callback in `pure def` is a closed task operation (like `Parallel.for_each_chunk`);
    // impure or opaque callback in `pure def` is rejected with `E1029`.
    emits(
        r#"pure def par_double :: ref [i64] -> [i64]
fn par_double xs =
    let mut out = [0, 0]
    Parallel.for_each_chunk_with 1 xs (\s start chunk -> Array.write 0 (s[start] * 2) chunk) (ref mut out)
    out
par_double [20, 22]"#,
    );
    rejects(
        r#"pure def bad :: ref [i64] -> [i64]
fn bad xs =
    let mut out = [0, 0]
    Parallel.for_each_chunk_with 1 xs (\s start chunk -> { Debug.print s; Array.write 0 s[start] chunk }) (ref mut out)
    out
0"#,
        "E1029",
    );
    rejects(
        r#"let s = [1, 2]
let f = Parallel.for_each_chunk_with 1 (ref s) (\_ _ (_c: ref mut [i64..]) -> ())
0"#,
        "E1013",
    );
    rejects(
        r#"let t = Tensor.of_array [2] [1.0, 2.0]
let v = Tensor.view (ref t)
let _ = Routine.scope (ref v) 2 (\_ i -> i)
0"#,
        "E1013",
    );
}
