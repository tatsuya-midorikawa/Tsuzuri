use tsuzuri::{analyze, diagnostic::Diagnostic, llvm};

/// Checks `source`, emits it twice to check determinism, and returns the native IR.
fn accepts(source: &str) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

fn diagnostic(source: &str) -> Diagnostic {
    analyze(source).expect_err(source)
}

fn rejects(source: &str, code: &str) {
    let error = diagnostic(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

fn rejects_with(source: &str, code: &str, message: &str) {
    let error = diagnostic(source);
    assert_eq!(
        (error.code, error.message.as_str()),
        (code, message),
        "{source}"
    );
}

/// The body of the `define` of `@tz.fn.Main.{name}`.
fn function<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .find(&format!("@tz.fn.Main.{name}("))
        .unwrap_or_else(|| panic!("{name}\n{ir}"));
    let body = &ir[start..];
    &body[..body.find("\n}").unwrap()]
}

#[test]
fn fixed_array_types_resolve_and_display() {
    for source in [
        "def f :: [i64; 4]\nfn f = [1, 2, 3, 4]",
        "record R { xs: [i64; 0] }",
        "let xs: [i64; 1] = [1]\nxs[0]",
        "def f :: [[i64; 2]; 3] -> [i64; 2]\nfn f m = m[2]",
        "type Vec3 = [f64; 3]\ndef f :: Vec3 -> f64\nfn f v = v[0]",
    ] {
        accepts(source);
    }
    rejects_with(
        "def f :: [i64; 3] -> [i64; 4]\nfn f v = v",
        "E1003",
        "expected [i64; 4], found [i64; 3]",
    );
    rejects_with(
        "def f :: [i64; 3] -> [i64]\nfn f v = v",
        "E1003",
        "expected [i64], found [i64; 3]",
    );
}

#[test]
fn fixed_array_lengths_are_bounded_literals() {
    accepts("def f :: [i64; 0] -> i64\nfn f v = v.length");
    accepts("def f :: [i64; 1024] -> i64\nfn f v = v[1023]");
    let length =
        "a fixed array length must be an integer literal without a suffix, for example '[f64; 3]'";
    for source in [
        "def f :: [i64; -1] -> i64\nfn f v = 0",
        "def f :: [i64; 4i64] -> i64\nfn f v = 0",
        "def f :: [i64; 0x4] -> i64\nfn f v = 0",
        "def f :: [i64; (4)] -> i64\nfn f v = 0",
    ] {
        rejects_with(source, "E0002", length);
    }
    rejects_with(
        "def f :: [|i64; 2|] -> i64\nfn f v = 0",
        "E0002",
        "list types use '[|T|]', without a length; use '[T; N]' for a fixed-length array",
    );
    rejects_with(
        "def f :: [i64; 1025] -> i64\nfn f v = 0",
        "E1010",
        "fixed array length 1025 exceeds 1024 elements; use '[T]' for larger arrays",
    );
}

#[test]
fn fixed_array_layout_limit() {
    accepts("def f :: [[i64; 1024]; 8] -> i64\nfn f m = m[7][1023]");
    rejects("def f :: [[i64; 1024]; 9] -> i64\nfn f m = 0", "E1010");
    accepts("record R { m: [[i64; 1024]; 8] }");
    rejects_with(
        "record R { m: [[i64; 1024]; 9] }",
        "E1010",
        "value layout exceeds 65536 bytes; use smaller value types",
    );
    // A generic value meets the limit where it is built with its concrete type.
    rejects(
        "def pass :: 'a -> 'a\nfn pass x = x\ndef f :: i64\nfn f =\n    let big: [[i64; 1024]; 9] = FixedArray.init (_i -> FixedArray.init (_j -> 0))\n    0",
        "E1010",
    );
    rejects_with(
        "record Node { next: [Node; 2] }",
        "E1010",
        "recursive value layout must pass through a union with a finite alternative",
    );
    accepts(
        "union Tree = Leaf | Node of [Tree; 2]\ndef rec leaves :: ref Tree -> i64\nfn rec leaves tree = match tree with\n    | Leaf -> 1\n    | Node children -> leaves children[0] + leaves children[1]",
    );
    rejects(
        "def f :: i64\nfn f =\n    let mut x = 1i64\n    let v: [ref mut i64; 1] = [ref mut x]\n    0",
        "E1005",
    );
}

#[test]
fn fixed_array_literals_need_the_expected_length() {
    accepts("def f :: [i64; 3]\nfn f = [1, 2, 3]");
    rejects_with(
        "def f :: [i64; 3]\nfn f = [1, 2]",
        "E1003",
        "expected 3 elements for '[i64; 3]', found 2",
    );
    // Without an expected fixed-length array, a literal stays `[T]`.
    accepts(
        "def g :: [i64] -> i64\nfn g xs = xs.length\ndef f :: i64\nfn f =\n    let xs = [1i64, 2]\n    g xs",
    );
    rejects_with(
        "def f :: [i64; 2]\nfn f = new [1, 2]",
        "E1005",
        "'new' creates a heap array or list; remove 'new' to create a fixed-length array value",
    );
    accepts("def f :: [ubyte; 0]\nfn f = []");
    rejects("def f :: [i64; 2]\nfn f = [1, true]", "E1003");
}

#[test]
fn fixed_array_indexing_and_length() {
    let ir = accepts(
        "record P { pos: [i64; 3] }\ndef at :: [i64; 3] -> i64 -> i64\nfn at v i = v[i] + v.length\ndef field :: P -> i64 -> i64\nfn field p i = p.pos[i]\ndef grid :: [[i64; 2]; 2] -> i64 -> i64 -> i64\nfn grid m i j = m[i][j]\ndef through :: ref [i64; 3] -> i64 -> i64\nfn through v i = v[i]\ndef constant :: [i64; 3] -> i64\nfn constant v = v[0] + v[2]",
    );
    assert!(
        function(&ir, "at").contains("getelementptr inbounds [3 x i64], ptr"),
        "{ir}"
    );
    assert!(function(&ir, "at").contains("icmp ult i64 %"), "{ir}");
    assert!(
        !function(&ir, "constant").contains("icmp ult i64 %"),
        "{ir}"
    );
    rejects_with(
        "def f :: [i64; 2] -> i64\nfn f v = { v[0] = 1; 0 }",
        "E1012",
        "assignment replaces a mutable binding; record fields, array elements, and list elements are immutable",
    );
}

#[test]
fn fixed_arrays_copy_or_move_by_element() {
    let ir =
        accepts("def twice :: [i64; 2] -> i64\nfn twice v =\n    let copy = v\n    copy[0] + v[1]");
    assert!(!function(&ir, "twice").contains("@tz.alloc"), "{ir}");
    rejects(
        "def f :: [string; 2] -> i64\nfn f v =\n    let moved = v\n    v[0].length + moved[1].length",
        "E1012",
    );
    accepts("def f :: [string; 2] -> Task<i64>\nfn f v = task { return v[0].length }");
    rejects(
        "def f :: [ref string; 1] -> Task<i64>\nfn f v = task { return v[0].length }",
        "E1013",
    );
}

#[test]
fn fixed_arrays_in_generic_and_record_types() {
    let ir = accepts(
        "def pass :: 'a -> 'a\nfn pass x = x\ndef f :: f64\nfn f =\n    let a: [f64; 3] = [1.0, 2.0, 3.0]\n    let b: [f64; 4] = [1.0, 2.0, 3.0, 4.0]\n    (pass a)[0] + (pass b)[3]",
    );
    assert_eq!(
        ir.matches("define internal [3 x double] @tz.fn.Main.pass")
            .count()
            + ir.matches("define internal [4 x double] @tz.fn.Main.pass")
                .count(),
        2,
        "{ir}"
    );
    accepts("def f :: Maybe<[f64; 3]>\nfn f = Some [1.0, 2.0, 3.0]");
    accepts(
        "record Particle<'a> { pos: ['a; 3], id: i64 }\ndef f :: f64\nfn f =\n    let p = Particle { pos: [1.0, 2.0, 3.0], id: 7 }\n    p.pos[2]",
    );
    accepts("const ORIGIN: [i64; 2] = [0, 0]\ndef f :: i64\nfn f = ORIGIN[1]");
}

#[test]
fn fixed_array_init_takes_the_length_from_the_expected_type() {
    accepts(
        "record R { v: [i64; 4] }\ndef squares :: [i64; 4]\nfn squares = FixedArray.init (\\i -> i * i)\ndef take :: [f64; 2] -> f64\nfn take v = v[1]\ndef f :: f64\nfn f =\n    let r = R { v: FixedArray.init (\\i -> i) }\n    take (FixedArray.init (\\i -> i as f64)) + (r.v[3] + squares()[3]) as f64",
    );
    rejects_with(
        "def f :: i64\nfn f = { let xs = FixedArray.init (\\i -> i); 0 }",
        "E1015",
        "cannot determine the length of 'FixedArray.init'; annotate the result type, for example '[i64; 4]'",
    );
    rejects_with(
        "def f :: [i64]\nfn f = FixedArray.init (\\i -> i)",
        "E1005",
        "'FixedArray.init' creates a fixed-length array; annotate a type such as '[i64; 4]'",
    );
    // A function value of FixedArray.init is instantiated per array type.
    let ir = accepts(
        "def build :: ((i64 -> i64) -> [i64; 2]) -> [i64; 2]\nfn build make = make (\\i -> i)\ndef wide :: ((i64 -> i64) -> [i64; 3]) -> [i64; 3]\nfn wide make = make (\\i -> i)\ndef f :: i64\nfn f = (build FixedArray.init)[1] + (wide FixedArray.init)[2]",
    );
    assert_eq!(
        ir.matches("define internal [2 x i64] @tz.fn.$builtin.FixedArray.init")
            .count(),
        1,
        "{ir}"
    );
    assert_eq!(
        ir.matches("define internal [3 x i64] @tz.fn.$builtin.FixedArray.init")
            .count(),
        1,
        "{ir}"
    );
}

#[test]
fn fixed_arrays_slice_into_shared_borrows() {
    let ir = accepts(
        "def sum :: ref [i64] -> i64\nfn sum xs =\n    let mut total = 0i64\n    for x in xs do total = total + x\n    total\ndef f :: [i64; 5] -> ref [i64; 5] -> i64\nfn f v r = sum v + sum (ref v[1..3]) + sum r + Array.length v\ndef load :: [f32; 4] -> f32\nfn load v =\n    let lanes: f32x4 = Simd.load v 0\n    Simd.extract lanes 3",
    );
    assert!(
        function(&ir, "f").contains("insertvalue %tz.array zeroinitializer, ptr"),
        "{ir}"
    );
    let named = "slicing a fixed-length array requires a named value; bind it with 'let' first";
    rejects_with(
        "def mk :: [i64; 3]\nfn mk = [1, 2, 3]\ndef g :: ref [i64] -> i64\nfn g xs = xs.length\ndef f :: i64\nfn f = g (mk())",
        "E1005",
        named,
    );
    rejects_with(
        "def mk :: [i64; 3]\nfn mk = [1, 2, 3]\ndef f :: i64\nfn f = (ref (mk())[0..1]).length",
        "E1005",
        named,
    );
    rejects(
        "def g :: [i64] -> i64\nfn g xs = xs.length\ndef f :: [i64; 2] -> i64\nfn f v = g v",
        "E1003",
    );
    rejects(
        "def g :: ref mut [i64] -> i64\nfn g xs = 0\ndef f :: i64\nfn f =\n    let mut v: [i64; 2] = [1, 2]\n    g (ref mut v)",
        "E1003",
    );
    rejects_with(
        "def f :: i64\nfn f =\n    let mut v: [i64; 2] = [1, 2]\n    let part = ref mut v[0..1]\n    0",
        "E1005",
        "a fixed-length array has no exclusive slices; replace the whole value through 'let mut', or use '[T]'",
    );
}

#[test]
fn fixed_arrays_reject_patterns_iteration_and_exports() {
    rejects_with(
        "def f :: [i64; 2] -> i64\nfn f v = match v with\n    | [a, b] -> a + b",
        "E1020",
        "fixed-length arrays cannot be destructured by patterns; index the elements instead",
    );
    rejects_with(
        "def f :: [i64; 2] -> i64\nfn f v =\n    let mut s = 0i64\n    for x in v do s = s + x\n    s",
        "E1005",
        "a fixed-length array is not enumerable; iterate 'for i in 0 .. xs.length - 1' and index it",
    );
    rejects("export def f :: [i64; 2] -> i64\nfn f v = v[0]", "E1008");
    rejects(
        "def f :: [i64; 2] -> [i64; 2] -> bool\nfn f a b = a == b",
        "E1005",
    );
}

#[test]
fn runtime_fixture_lowers_for_both_targets() {
    let module = analyze(include_str!("fixtures/fixed_arrays/Main.tz")).unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(ir.contains("[4 x i64]"), "{ir}");
    }
}

#[test]
fn length_parameters_generalize_functions() {
    let ir = accepts(
        "def total :: [i64; N] -> i64\nfn total v =\n    let mut s = 0i64\n    for i in 0i64 .. (v.length - 1) do s = s + v[i]\n    s\ndef zeros :: [i64; M]\nfn zeros = FixedArray.init (\\_i -> 0)\ndef f :: i64\nfn f =\n    let z: [i64; 5] = zeros()\n    total [1, 2, 3] + total [4, 5] + total z",
    );
    assert_eq!(
        ir.matches("define internal i64 @tz.fn.Main.total").count(),
        3,
        "{ir}"
    );
    // The first literal decides N, so the second has the wrong length.
    rejects_with(
        "def same :: [i64; N] -> [i64; N] -> i64\nfn same a b = 0\ndef f :: i64\nfn f = same [1, 2] [1, 2, 3]",
        "E1003",
        "expected 2 elements for '[i64; 2]', found 3",
    );
    rejects_with(
        "def same :: [i64; N] -> [i64; N] -> i64\nfn same a b = 0\ndef f :: [i64; 2] -> [i64; 3] -> i64\nfn f a b = same a b",
        "E1003",
        "expected [i64; 2], found [i64; 3]",
    );
    rejects_with(
        "def f :: [i64; N]\nfn f = [1, 2]",
        "E1003",
        "expected N elements for '[i64; N]', found 2",
    );
}

#[test]
fn length_parameters_on_records_unions_and_aliases() {
    accepts(
        "record Grid<'a, const N: i64> { cells: ['a; N], id: i64 }\nunion Shape<const N: i64> = Empty | Poly of [f64; N]\ntype Row<const N: i64> = [i64; N]\ndef first :: Grid<'a, N> -> 'a\nfn first g = g.cells[0]\ndef corners :: Shape<N> -> i64\nfn corners s = match s with\n    | Empty -> 0\n    | Poly points -> points.length\ndef row :: Row<3> -> i64\nfn row r = r[2]\ndef f :: f64\nfn f =\n    let g = Grid { cells: [7.5, 8.5], id: 4 }\n    let s: Shape<4> = Poly [1.0, 2.0, 3.0, 4.0]\n    first g + (corners s + row [1, 2, 3]) as f64",
    );
    rejects_with(
        "record Grid<'a, const N: i64> { cells: ['a; N] }\ndef f :: Grid<f64, f64> -> i64\nfn f g = 0",
        "E1004",
        "expected a length, found the type 'f64'",
    );
    rejects_with(
        "record Grid<'a, const N: i64> { cells: ['a; N] }\ndef f :: Grid<3, 3> -> i64\nfn f g = 0",
        "E1004",
        "expected a type, found the length 3; a length belongs in '[T; N]' or a length parameter 'const N: i64'",
    );
    rejects_with(
        "record Grid<'a> { cells: ['a; N] }",
        "E1024",
        "length 'N' is not declared by record 'Grid'; add 'const N: i64' in angle brackets after the name, or define an 'i64' constant 'N'",
    );
    rejects_with(
        "record Grid<const N: i64> { size: i64 }",
        "E1024",
        "length parameter N is not used by any field; remove it or add a field that mentions it",
    );
    rejects_with(
        "record Grid<const N: i32> { cells: [i64; N] }",
        "E0002",
        "a length parameter has type 'i64', as in 'const N: i64'",
    );
    rejects(
        "record Grid<'a, const N: i64> { cells: ['a; N] }\ndef f :: Grid<f64, 2000> -> i64\nfn f g = 0",
        "E1010",
    );
}

#[test]
fn integer_constants_name_lengths() {
    accepts("const SIZE: i64 = 3\ndef f :: [f64; SIZE] -> f64\nfn f v = v[2]\nf [1.0, 2.0, 3.0]");
    rejects_with(
        "const BIG: i64 = 5000\ndef f :: [f64; BIG] -> i64\nfn f v = 0",
        "E1010",
        "fixed array length 5000 exceeds 1024 elements; use '[T]' for larger arrays",
    );
    rejects_with(
        "const SUM: i64 = 1 + 2\ndef f :: [f64; SUM] -> i64\nfn f v = 0",
        "E1005",
        "constant 'SUM' cannot be a length; a length constant has type 'i64' and an integer literal value",
    );
    // A private constant used only as a length is used.
    let module =
        analyze("private const SIZE: i64 = 2\ndef f :: [i64; SIZE] -> i64\nfn f v = v[1]").unwrap();
    assert!(
        module
            .warnings
            .iter()
            .all(|warning| warning.code != "W1002"),
        "{:?}",
        module.warnings
    );
}

/// A16 Phase 2: scalar fixed-length arrays are C array fields of exported records.
#[test]
fn exported_records_hold_scalar_fixed_arrays() {
    use std::{fs, path::Path, process::Command};
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-fixed-abi-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("project")).unwrap();
    fs::write(
        root.join("project/Main.tz"),
        "record Sample { values: [f64; 3], flags: [bool; 2], small: [i8; 2], id: i64 }\n\
         export def total :: ref Sample -> f64\n\
         fn total sample = sample.values[0] + sample.values[1] + sample.values[2] + (if sample.flags[1] then 100.0 else 0.0) + (sample.small[0] as f64) + (sample.small[1] as f64) * 10.0\n\
         export def make :: i64 -> Sample\n\
         fn make id = Sample { values: FixedArray.init (\\i -> (i + id) as f64), flags: [true, false], small: [-1, 2], id: id }\n",
    )
    .unwrap();
    let project = tsuzuri::driver::Project::load(&root.join("project/Main.tz")).unwrap();
    let module = project.analyze().unwrap();
    let header = tsuzuri::llvm::header(&module);
    assert!(
        header.contains(
            "    double values[3];\n    int32_t flags[2];\n    int32_t small[2];\n    int64_t id;\n"
        ),
        "{header}"
    );
    fs::write(root.join("api.h"), &header).unwrap();
    let native = root.join("native.ll");
    fs::write(
        &native,
        tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, false).unwrap(),
    )
    .unwrap();
    let host = root.join("host.c");
    fs::write(
        &host,
        "#include <assert.h>\n#include \"api.h\"\n\
         _Static_assert(sizeof(tz_record_4Main_6Sample) == 48, \"layout\");\n\
         int main(void) {\n\
             tz_record_4Main_6Sample sample = { { 1.5, 2.5, 3.0 }, { 0, 1 }, { -1, 2 }, 9 }, made;\n\
             assert(tz_total(&sample) == 126.0);\n\
             tz_make(&made, 4);\n\
             assert(made.values[0] == 4.0 && made.values[2] == 6.0);\n\
             assert(made.flags[0] == 1 && made.flags[1] == 0 && made.small[0] == -1 && made.small[1] == 2 && made.id == 4);\n\
             return 0;\n\
         }\n",
    )
    .unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    for optimization in [0, 3] {
        let executable = root.join(format!("native-{optimization}"));
        let output = Command::new(&clang)
            .args([
                "-std=c11",
                "-Wno-override-module",
                "-Wall",
                "-Wextra",
                "-Werror",
            ])
            .arg(format!("-O{optimization}"))
            .arg(&host)
            .arg(&native)
            .args(["-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(&executable).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let wasm = root.join(format!("module-{optimization}.wasm"));
        tsuzuri::driver::build(
            &module,
            &project,
            &wasm,
            tsuzuri::driver::BuildOptions {
                target: tsuzuri::driver::Target::Wasm32,
                emit: tsuzuri::driver::Emit::Wasm,
                optimization,
                ..Default::default()
            },
        )
        .unwrap();
        let script = root.join("host.mjs");
        fs::write(
            &script,
            "import assert from 'node:assert/strict';\n\
             import {readFileSync} from 'node:fs';\n\
             const api = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(process.argv[2]))).exports;\n\
             const sample = api.tsuzuri_alloc(48n), made = api.tsuzuri_alloc(48n);\n\
             let view = new DataView(api.memory.buffer);\n\
             [1.5, 2.5, 3].forEach((value, index) => view.setFloat64(sample + 8 * index, value, true));\n\
             view.setInt32(sample + 24, 0, true); view.setInt32(sample + 28, 1, true);\n\
             view.setInt32(sample + 32, -1, true); view.setInt32(sample + 36, 2, true);\n\
             view.setBigInt64(sample + 40, 9n, true);\n\
             assert.equal(api.tz_total(sample), 126);\n\
             api.tz_make(made, 4n);\n\
             view = new DataView(api.memory.buffer);\n\
             assert.deepEqual([0, 1, 2].map((index) => view.getFloat64(made + 8 * index, true)), [4, 5, 6]);\n\
             assert.deepEqual([24, 28, 32, 36].map((offset) => view.getInt32(made + offset, true)), [1, 0, -1, 2]);\n\
             assert.equal(view.getBigInt64(made + 40, true), 4n);\n",
        )
        .unwrap();
        let output = Command::new("node")
            .arg(&script)
            .arg(&wasm)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(Path::new(&root).exists());
    fs::remove_dir_all(root).unwrap();
}
