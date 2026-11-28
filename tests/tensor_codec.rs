//! `TensorCodec` against independent references (NC11).
//!
//! `tests/fixtures/tensor_codec/reference.json` holds the bytes that NumPy, zlib and zipfile write and the
//! values that they read, and Safetensors files built from the format description
//! (`tests/fixtures/tensor_codec/generate.py` writes it). Each test here turns a group of those cases into a
//! Tsuzuri test program and runs it with `tsuzuri test`. `tests/tensor_io.mjs` checks the other direction (what
//! Tsuzuri writes, NumPy reads) with a live Python.

use std::fmt::Write as _;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;
use tsuzuri::analyze;

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/tensor_codec/reference.json")).unwrap()
}

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/tensor_codec")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

/// Runs the `test` blocks of `source` and returns how many ran.
fn run_tz_test(prefix: &str, source: &str) -> usize {
    let root = scratch_dir(prefix);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Main.tz"), source).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"tensor-codec-suite\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "tsuzuri test failed in {}:\nstdout:\n{stdout}\nstderr:\n{stderr}",
        root.display()
    );
    // The C compiler says nothing about the code that the std modules generate.
    assert!(!stderr.contains("warning:"), "{stderr}");
    let _ = std::fs::remove_dir_all(&root);
    stdout
        .lines()
        .filter(|line| line.starts_with("ok "))
        .count()
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
        .collect()
}

/// `    let name: [ubyte] = [1ubyte, ...]`.
fn bytes_let(name: &str, bytes: &[u8]) -> String {
    let items: Vec<String> = bytes.iter().map(|byte| format!("{byte}ubyte")).collect();
    format!("    let {name}: [ubyte] = [{}]\n", items.join(", "))
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn list(value: &Value) -> &Vec<Value> {
    value.as_array().unwrap()
}

fn shape_of(value: &Value) -> String {
    let extents: Vec<String> = list(value)
        .iter()
        .map(|extent| extent.as_i64().unwrap().to_string())
        .collect();
    format!("[{}]", extents.join(", "))
}

/// The type of the parts of an element: a complex number is compared by its real and imaginary parts.
fn part_type(ty: &str) -> &str {
    match ty {
        "Complex<f32>" => "f32",
        "Complex<f64>" => "f64",
        other => other,
    }
}

/// Lines that check `tensor` (a `Tensor<ty>`) against the shape and the values of a case.
fn check_tensor(output: &mut String, indent: &str, ty: &str, shape: &Value, values: &Value) {
    let _ = writeln!(output, "{indent}let window = Tensor.view (ref tensor)");
    let _ = writeln!(output, "{indent}let shape: [i64] = {}", shape_of(shape));
    let _ = writeln!(
        output,
        "{indent}assert (Array.copy (Tensor.shape (ref window)) == shape)"
    );
    let Some(values) = values.as_array() else {
        return;
    };
    let items: Vec<&str> = values.iter().map(text).collect();
    let part = part_type(ty);
    let _ = writeln!(
        output,
        "{indent}let expected: [{part}] = [{}]",
        items.join(", ")
    );
    if part == ty {
        let _ = writeln!(
            output,
            "{indent}assert (Array.copy (Tensor.as_array (ref tensor)) == expected)"
        );
    } else {
        let _ = writeln!(
            output,
            "{indent}let elements = Tensor.as_array (ref tensor)"
        );
        let _ = writeln!(
            output,
            "{indent}let parts = new [{part}](elements.length * 2, \\index -> if index % 2 == 0 then elements[index / 2].re else elements[index / 2].im)"
        );
        let _ = writeln!(output, "{indent}assert (parts == expected)");
    }
}

/// A match arm pattern for an error case name.
fn error_pattern(name: &str) -> String {
    match name {
        "InvalidMagic" | "TruncatedData" | "ShapeOverflow" => format!("TensorCodec.{name}"),
        other => format!("TensorCodec.{other} _"),
    }
}

/// `match <expression> with` arms that accept only the named errors (`A|B` accepts either).
fn expect_error(output: &mut String, indent: &str, expression: &str, errors: &str) {
    let _ = writeln!(output, "{indent}match {expression} with");
    for error in errors.split('|') {
        let _ = writeln!(
            output,
            "{indent}| Result.Error ({}) -> ()",
            error_pattern(error)
        );
    }
    let _ = writeln!(output, "{indent}| _ -> assert false");
}

const OTHER_TYPE: &[(&str, &str)] = &[("f64", "i64"), ("i64", "f64")];

fn another_type(ty: &str) -> &'static str {
    OTHER_TYPE
        .iter()
        .find(|(from, _)| *from == ty)
        .map_or("f64", |(_, to)| to)
}

#[test]
fn npy_files_of_numpy_decode_and_encode_byte_for_byte() {
    let reference = reference();
    let cases = list(&reference["npy"]);
    assert!(cases.len() >= 50, "{}", cases.len());
    // A few programs, so that each one stays small enough to compile quickly.
    let mut total = 0;
    for (chunk_index, chunk) in cases.chunks(12).enumerate() {
        let mut source = String::new();
        for case in chunk {
            let ty = text(&case["type"]);
            let _ = writeln!(source, "test \"npy {}\" =", text(&case["name"]));
            source.push_str(&bytes_let("file", &hex_bytes(text(&case["hex"]))));
            source.push_str(&bytes_let(
                "canonical",
                &hex_bytes(text(&case["canonical"])),
            ));
            let _ = writeln!(
                source,
                "    let decoded: Result<Tensor<{ty}>, TensorCodec.Error> = TensorCodec.decode_npy (ref file)"
            );
            let _ = writeln!(source, "    match decoded with");
            let _ = writeln!(source, "    | Result.Error _ -> assert false");
            let _ = writeln!(source, "    | Result.Ok tensor ->");
            check_tensor(&mut source, "        ", ty, &case["shape"], &case["values"]);
            let _ = writeln!(
                source,
                "        assert (Result.get (TensorCodec.encode_npy (ref window)) == canonical)"
            );
            let _ = writeln!(
                source,
                "    match TensorCodec.decode_npy_raw (ref file) with"
            );
            let _ = writeln!(source, "    | Result.Error _ -> assert false");
            let _ = writeln!(
                source,
                "    | Result.Ok raw -> assert (Result.get (TensorCodec.encode_npy_raw (ref raw)) == canonical)"
            );
            let other = another_type(ty);
            let _ = writeln!(
                source,
                "    let wrong: Result<Tensor<{other}>, TensorCodec.Error> = TensorCodec.decode_npy (ref file)"
            );
            expect_error(&mut source, "    ", "wrong", "DtypeMismatch");
            source.push('\n');
        }
        total += run_tz_test(&format!("npy{chunk_index}"), &source);
    }
    assert_eq!(total, cases.len());
}

#[test]
fn windows_encode_as_numpy_saves_the_same_views() {
    let reference = reference();
    let views = &reference["views"];
    let mut source = String::from("test \"views\" =\n");
    for name in [
        "base",
        "permuted_2_0_1",
        "flipped_axis_1",
        "narrowed_axis_2",
        "row_1",
    ] {
        source.push_str(&bytes_let(name, &hex_bytes(text(&views[name]))));
    }
    source.push_str(
        r##"    let decoded: Result<Tensor<f32>, TensorCodec.Error> = TensorCodec.decode_npy (ref base)
    let tensor = Result.get decoded
    let window = Tensor.view (ref tensor)
    let permuted = Tensor.permute [2, 0, 1] (ref window)
    assert (Result.get (TensorCodec.encode_npy (ref permuted)) == permuted_2_0_1)
    let flipped = Tensor.flip_axis 1 (ref window)
    assert (Result.get (TensorCodec.encode_npy (ref flipped)) == flipped_axis_1)
    let narrowed = Tensor.narrow 2 1 2 (ref window)
    assert (Result.get (TensorCodec.encode_npy (ref narrowed)) == narrowed_axis_2)
    let row = Tensor.index_axis 0 1 (ref window)
    assert (Result.get (TensorCodec.encode_npy (ref row)) == row_1)
    // A `Raw` keeps the dtype, and `of_raw` gives the same tensor back.
    let raw = TensorCodec.to_raw (ref permuted)
    assert (raw.dtype == TensorCodec.F32 && raw.shape == [4, 2, 3] && raw.data.length == 96)
    let again: Result<Tensor<f32>, TensorCodec.Error> = TensorCodec.of_raw (ref raw)
    let rebuilt = Result.get again
    assert (Result.get (TensorCodec.encode_npy (Tensor.view (ref rebuilt))) == permuted_2_0_1)
    let wrong: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.of_raw (ref raw)
    match wrong with
    | Result.Error (TensorCodec.DtypeMismatch _) -> ()
    | _ -> assert false

test "raw that does not agree" =
    let short = TensorCodec.Raw { dtype: TensorCodec.F64, shape: [2], data: [1ubyte, 2ubyte] }
    let typed: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.of_raw (ref short)
    match typed with
    | Result.Error (TensorCodec.InvalidHeader _) -> ()
    | _ -> assert false
    match TensorCodec.encode_npy_raw (ref short) with
    | Result.Error (TensorCodec.InvalidHeader _) -> ()
    | _ -> assert false
    let negative = TensorCodec.Raw { dtype: TensorCodec.U8, shape: [-1], data: [] }
    match TensorCodec.encode_npy_raw (ref negative) with
    | Result.Error TensorCodec.ShapeOverflow -> ()
    | _ -> assert false
    let huge = TensorCodec.Raw { dtype: TensorCodec.U8, shape: [4611686018427387904, 4], data: [] }
    match TensorCodec.encode_npz [("x", huge)] with
    | Result.Error TensorCodec.ShapeOverflow -> ()
    | _ -> assert false
"##,
    );
    assert_eq!(run_tz_test("views", &source), 2);
}

#[test]
fn hostile_npy_input_is_an_error_and_never_a_trap() {
    let reference = reference();
    let cases = list(&reference["npy_errors"]);
    assert!(cases.len() >= 30, "{}", cases.len());
    let mut source = String::new();
    for case in cases {
        let error = text(&case["error"]);
        let _ = writeln!(source, "test \"npy error {}\" =", text(&case["name"]));
        source.push_str(&bytes_let("file", &hex_bytes(text(&case["hex"]))));
        expect_error(
            &mut source,
            "    ",
            "TensorCodec.inspect_npy (ref file)",
            error,
        );
        expect_error(
            &mut source,
            "    ",
            "TensorCodec.decode_npy_raw (ref file)",
            error,
        );
        let _ = writeln!(
            source,
            "    let typed: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.decode_npy (ref file)"
        );
        expect_error(&mut source, "    ", "typed", error);
        source.push('\n');
    }
    // Every prefix of a valid file, and a file with any one byte of its header changed, is read without a trap.
    let good = hex_bytes(text(&list(&reference["npy"])[0]["hex"]));
    source.push_str("test \"npy prefixes and changed bytes\" =\n");
    source.push_str(&bytes_let("file", &good));
    source.push_str(
        r##"    let mut failures = 0
    for cut in 0i64 ..< file.length do
        match TensorCodec.decode_npy_raw (ref file[0 ..< cut]) with
        | Result.Ok _ -> failures = failures + 1
        | Result.Error _ -> ()
    assert (failures == 0)
    let mut read = 0
    for position in 0i64 ..< 128 do
        for change in [1ubyte, 32ubyte, 128ubyte] do
            let changed = new [ubyte](file.length, \index -> if index == position then file[index] ^ change else file[index])
            match TensorCodec.decode_npy_raw (ref changed) with
            | Result.Ok _ -> read = read + 1
            | Result.Error _ -> ()
    // Changing a space of the padding to another blank leaves the file valid; nothing else matters here.
    assert (read < 128 * 3)
"##,
    );
    assert_eq!(run_tz_test("npy-errors", &source), cases.len() + 1);
}

fn check_entries(output: &mut String, entries: &Value) {
    let names: Vec<String> = list(entries)
        .iter()
        .map(|entry| format!("{:?}", text(&entry["name"])))
        .collect();
    let _ = writeln!(
        output,
        "        assert (TensorCodec.archive_names (ref archive) == [{}])",
        names.join(", ")
    );
    for (index, entry) in list(entries).iter().enumerate() {
        let ty = text(&entry["type"]);
        let name = format!("{:?}", text(&entry["name"]));
        let _ = writeln!(
            output,
            "        let found{index}: Result<Tensor<{ty}>, TensorCodec.Error> = TensorCodec.archive_get {name} (ref archive)"
        );
        let _ = writeln!(output, "        match found{index} with");
        let _ = writeln!(output, "        | Result.Error _ -> assert false");
        let _ = writeln!(output, "        | Result.Ok tensor ->");
        check_tensor(
            output,
            "            ",
            ty,
            &entry["shape"],
            &entry["values"],
        );
        let other = another_type(ty);
        let _ = writeln!(
            output,
            "        let wrong{index}: Result<Tensor<{other}>, TensorCodec.Error> = TensorCodec.archive_get {name} (ref archive)"
        );
        expect_error(
            output,
            "        ",
            &format!("wrong{index}"),
            "DtypeMismatch",
        );
        let _ = writeln!(
            output,
            "        match TensorCodec.archive_raw {name} (ref archive) with"
        );
        let _ = writeln!(
            output,
            "        | Result.Ok raw -> assert (raw.shape == {})",
            if list(&entry["shape"]).is_empty() {
                "(Array.sub 0 0 [0])".to_owned()
            } else {
                shape_of(&entry["shape"])
            }
        );
        let _ = writeln!(output, "        | Result.Error _ -> assert false");
        let _ = writeln!(
            output,
            "        match TensorCodec.archive_inspect {name} (ref archive) with"
        );
        let _ = writeln!(
            output,
            "        | Result.Ok header -> assert (TensorCodec.dtype_size (ref header.dtype) > 0)"
        );
        let _ = writeln!(output, "        | Result.Error _ -> assert false");
    }
    expect_error(
        output,
        "        ",
        "TensorCodec.archive_raw \"no such tensor\" (ref archive)",
        "EntryNotFound",
    );
}

#[test]
fn npz_archives_of_numpy_and_zipfile_decode() {
    let reference = reference();
    let cases = list(&reference["npz"]);
    assert_eq!(cases.len(), 3);
    let mut total = 0;
    for case in cases {
        let mut source = String::new();
        let _ = writeln!(source, "test \"npz {}\" =", text(&case["name"]));
        source.push_str(&bytes_let("file", &hex_bytes(text(&case["hex"]))));
        source.push_str("    match TensorCodec.decode_npz file with\n");
        source.push_str("    | Result.Error _ -> assert false\n");
        source.push_str("    | Result.Ok archive ->\n");
        check_entries(&mut source, &case["entries"]);
        total += run_tz_test(&format!("npz-{}", text(&case["name"])), &source);
    }
    assert_eq!(total, 3);
}

/// Tests of archives that must fail: at the index (`decode`) or when the entry `a` is taken out.
fn archive_error_tests(label: &str, decode: &str, cases: &[Value]) -> String {
    let mut source = String::new();
    for case in cases {
        let error = text(&case["error"]);
        let _ = writeln!(source, "test \"{label} error {}\" =", text(&case["name"]));
        source.push_str(&bytes_let("file", &hex_bytes(text(&case["hex"]))));
        if text(&case["stage"]) == "index" {
            expect_error(&mut source, "    ", &format!("{decode} file"), error);
        } else {
            let _ = writeln!(source, "    match {decode} file with");
            let _ = writeln!(source, "    | Result.Error _ -> assert false");
            let _ = writeln!(source, "    | Result.Ok archive ->");
            if error == "DtypeMismatch" {
                let _ = writeln!(
                    source,
                    "        let typed: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.archive_get \"a\" (ref archive)"
                );
                expect_error(&mut source, "        ", "typed", error);
            } else {
                expect_error(
                    &mut source,
                    "        ",
                    "TensorCodec.archive_raw \"a\" (ref archive)",
                    error,
                );
                let _ = writeln!(
                    source,
                    "        let typed: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.archive_get \"a\" (ref archive)"
                );
                expect_error(&mut source, "        ", "typed", error);
            }
        }
        source.push('\n');
    }
    source
}

#[test]
fn hostile_npz_input_is_an_error_and_never_a_trap() {
    let reference = reference();
    let cases = list(&reference["npz_errors"]);
    assert!(cases.len() >= 18, "{}", cases.len());
    let mut source = archive_error_tests("npz", "TensorCodec.decode_npz", cases);
    let good = hex_bytes(text(&list(&reference["npz"])[1]["hex"]));
    // The deflated archive of NumPy with one byte changed at a time, in its first entry and in its index.
    source.push_str("test \"npz changed bytes\" =\n");
    source.push_str(&bytes_let("file", &good));
    source.push_str(
        r##"    let mut read = 0
    for step in 0i64 ..< 240 do
        let position = if step < 120 then step * 3 else file.length - 1 - (step - 120) * 5
        let changed = new [ubyte](file.length, \index -> if index == position then file[index] ^ 85ubyte else file[index])
        match TensorCodec.decode_npz changed with
        | Result.Error _ -> ()
        | Result.Ok archive ->
            let names = TensorCodec.archive_names (ref archive)
            for index in 0i64 ..< names.length do
                match TensorCodec.archive_raw (ref names[index]) (ref archive) with
                | Result.Ok _ -> read = read + 1
                | Result.Error _ -> ()
    assert (read >= 0)
"##,
    );
    assert_eq!(run_tz_test("npz-errors", &source), cases.len() + 1);
}

#[test]
fn deflate_streams_of_zlib_inflate_and_round_trip() {
    let reference = reference();
    let cases = list(&reference["deflate"]);
    assert_eq!(cases.len(), 6);
    let mut source = String::new();
    for case in cases {
        let name = text(&case["name"]);
        let length = case["length"].as_i64().unwrap();
        let crc = case["crc32"].as_i64().unwrap();
        let _ = writeln!(source, "test \"deflate {name}\" =");
        let raw = case["hex"].as_str().map(hex_bytes);
        if let Some(raw) = &raw {
            source.push_str(&bytes_let("plain", raw));
            source.push_str("    assert ((TensorCodec.crc32 (ref plain) as i64) == ");
            let _ = writeln!(source, "{crc})");
            source.push_str("    let packed = TensorCodec.deflate (ref plain)\n");
            let _ = writeln!(
                source,
                "    assert (Result.get (TensorCodec.inflate {length} (ref packed)) == plain)"
            );
            // The stream is deterministic.
            source.push_str("    assert (TensorCodec.deflate (ref plain) == packed)\n");
        }
        for (label, stream) in case["streams"].as_object().unwrap() {
            let stream = hex_bytes(text(stream));
            source.push_str(&bytes_let(&format!("stream_{label}"), &stream));
            let _ = writeln!(
                source,
                "    match TensorCodec.inflate {length} (ref stream_{label}) with"
            );
            let _ = writeln!(source, "    | Result.Error _ -> assert false");
            let _ = writeln!(
                source,
                "    | Result.Ok data -> assert (data.length == {length} && (TensorCodec.crc32 (ref data) as i64) == {crc}{})",
                if raw.is_some() {
                    " && data == plain"
                } else {
                    ""
                }
            );
            if length > 0 {
                // One byte less than the stream holds is refused, and so is a stream cut in half.
                let _ = writeln!(
                    source,
                    "    match TensorCodec.inflate {} (ref stream_{label}) with",
                    length - 1
                );
                let _ = writeln!(
                    source,
                    "    | Result.Error (TensorCodec.CorruptData _) -> ()"
                );
                let _ = writeln!(source, "    | _ -> assert false");
                if stream.len() > 6 {
                    let _ = writeln!(
                        source,
                        "    match TensorCodec.inflate {length} (ref stream_{label}[0 ..< {}]) with",
                        stream.len() / 2
                    );
                    let _ = writeln!(source, "    | Result.Ok _ -> assert false");
                    let _ = writeln!(source, "    | Result.Error _ -> ()");
                }
            }
        }
        source.push('\n');
    }
    // Arbitrary bytes and changed streams are read to an end without a trap, and never give more than the limit.
    let text_stream = hex_bytes(text(&cases[2]["streams"]["dynamic"]));
    source.push_str("test \"inflate arbitrary bytes\" =\n");
    source.push_str(&bytes_let("stream", &text_stream));
    source.push_str(
        r##"    let mut state = 12345
    let mut longest = 0
    for round in 0i64 ..< 400 do
        state = (state * 1103515245 + 12345) % 2147483648
        let position = state % stream.length
        let bit = (state / 65536) % 8
        let changed = new [ubyte](stream.length, \index -> if index == position then stream[index] ^ ((1 <<< bit) as ubyte) else stream[index])
        match TensorCodec.inflate 1000 (ref changed) with
        | Result.Ok data -> if data.length > longest then longest = data.length
        | Result.Error _ -> ()
        let noise = new [ubyte](1 + round % 40, \index -> ((state / (index + 1)) % 256) as ubyte)
        match TensorCodec.inflate 1000 (ref noise) with
        | Result.Ok data -> if data.length > longest then longest = data.length
        | Result.Error _ -> ()
    assert (longest <= 1000)
"##,
    );
    assert_eq!(run_tz_test("deflate", &source), cases.len() + 1);
}

#[test]
fn safetensors_files_from_the_format_description_decode() {
    let reference = reference();
    let cases = list(&reference["safetensors"]);
    let mut source = String::new();
    for case in cases {
        let _ = writeln!(source, "test \"safetensors {}\" =", text(&case["name"]));
        source.push_str(&bytes_let("file", &hex_bytes(text(&case["hex"]))));
        // The same header is read without taking the bytes.
        let first = &list(&case["entries"])[0];
        let _ = writeln!(
            source,
            "    match TensorCodec.inspect_safetensors {:?} (ref file) with",
            text(&first["name"])
        );
        let _ = writeln!(
            source,
            "    | Result.Ok header -> assert (header.shape == {} && header.data_offset > 8)",
            shape_of(&first["shape"])
        );
        let _ = writeln!(source, "    | Result.Error _ -> assert false");
        source.push_str("    match TensorCodec.decode_safetensors file with\n");
        source.push_str("    | Result.Error _ -> assert false\n");
        source.push_str("    | Result.Ok archive ->\n");
        let pairs: Vec<String> = list(&case["metadata"])
            .iter()
            .map(|pair| format!("({:?}, {:?})", text(&pair[0]), text(&pair[1])))
            .collect();
        let _ = writeln!(
            source,
            "        assert (TensorCodec.archive_metadata (ref archive) == [{}])",
            pairs.join(", ")
        );
        check_entries(&mut source, &case["entries"]);
        source.push('\n');
    }
    // What Tsuzuri writes, it reads back: every dtype, the metadata, and the layout rules of the format.
    source.push_str(
        r##"test "safetensors written and read back" =
    let wide = Tensor.of_array [2] [1.5, -2.5]
    let narrow = Tensor.of_array [3] [1ubyte, 2ubyte, 3ubyte]
    let middle = Tensor.of_array [2, 2] [1i32, 2i32, 3i32, 4i32]
    let flags = Tensor.of_array [3] [true, false, true]
    let tensors = [("narrow", TensorCodec.to_raw (Tensor.view (ref narrow))), ("wide", TensorCodec.to_raw (Tensor.view (ref wide))), ("middle", TensorCodec.to_raw (Tensor.view (ref middle))), ("flags", TensorCodec.to_raw (Tensor.view (ref flags)))]
    let file = Result.get (TensorCodec.encode_safetensors_with [("k", "v \"quoted\" é")] (ref tensors))
    // The header length is a multiple of 8, and the larger element types come first.
    assert (file[0] % 8ubyte == 0ubyte)
    let wide_place = Result.get (TensorCodec.inspect_safetensors "wide" (ref file))
    let middle_place = Result.get (TensorCodec.inspect_safetensors "middle" (ref file))
    let narrow_place = Result.get (TensorCodec.inspect_safetensors "narrow" (ref file))
    let flags_place = Result.get (TensorCodec.inspect_safetensors "flags" (ref file))
    assert (wide_place.data_offset % 8 == 0 && middle_place.data_offset == wide_place.data_offset + 16)
    assert (narrow_place.data_offset == middle_place.data_offset + 16 && flags_place.data_offset == narrow_place.data_offset + 3)
    assert (flags_place.data_offset + 3 == file.length)
    let archive = Result.get (TensorCodec.decode_safetensors file)
    assert (TensorCodec.archive_metadata (ref archive) == [("k", "v \"quoted\" é")])
    let back: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.archive_get "wide" (ref archive)
    assert (Tensor.to_array (Result.get back) == [1.5, -2.5])
    let flags_back: Result<Tensor<bool>, TensorCodec.Error> = TensorCodec.archive_get "flags" (ref archive)
    assert (Tensor.to_array (Result.get flags_back) == [true, false, true])

test "safetensors names and dtypes that cannot be written" =
    let t = Tensor.of_array [1] [1.0f32]
    let complex = Tensor.of_array [1] [Complex.of 1.0f32 2.0f32]
    match TensorCodec.encode_safetensors [("a", TensorCodec.to_raw (Tensor.view (ref t))), ("a", TensorCodec.to_raw (Tensor.view (ref t)))] with
    | Result.Error (TensorCodec.InvalidHeader _) -> ()
    | _ -> assert false
    match TensorCodec.encode_safetensors [("__metadata__", TensorCodec.to_raw (Tensor.view (ref t)))] with
    | Result.Error (TensorCodec.InvalidHeader _) -> ()
    | _ -> assert false
    match TensorCodec.encode_safetensors [("c", TensorCodec.to_raw (Tensor.view (ref complex)))] with
    | Result.Error (TensorCodec.UnsupportedDtype _) -> ()
    | _ -> assert false
"##,
    );
    assert_eq!(run_tz_test("safetensors", &source), cases.len() + 2);
}

#[test]
fn hostile_safetensors_input_is_an_error_and_never_a_trap() {
    let reference = reference();
    let cases = list(&reference["safetensors_errors"]);
    assert!(cases.len() >= 25, "{}", cases.len());
    let source = archive_error_tests("safetensors", "TensorCodec.decode_safetensors", cases);
    assert_eq!(run_tz_test("safetensors-errors", &source), cases.len());
}

#[test]
fn tables_of_text_of_numpy_decode() {
    let reference = reference();
    let cases = list(&reference["csv"]);
    let mut source = String::new();
    for case in cases {
        let ty = text(&case["type"]);
        let _ = writeln!(source, "test \"csv {}\" =", text(&case["name"]));
        source.push_str(&bytes_let("file", text(&case["text"]).as_bytes()));
        source.push_str(&bytes_let("delimiter", text(&case["delimiter"]).as_bytes()));
        source.push_str(&bytes_let("comment", text(&case["comment"]).as_bytes()));
        let _ = writeln!(
            source,
            "    let options = TensorCodec.CsvOptions {{ delimiter: Maybe.get (Utf8String.from_bytes delimiter), skip_rows: {}, comment: Maybe.get (Utf8String.from_bytes comment) }}",
            case["skip_rows"].as_i64().unwrap()
        );
        let _ = writeln!(
            source,
            "    let table = Maybe.get (Utf8String.from_bytes file)"
        );
        let _ = writeln!(
            source,
            "    let decoded: Result<Tensor<{ty}>, TensorCodec.Error> = TensorCodec.decode_csv options (ref table)"
        );
        let _ = writeln!(source, "    match decoded with");
        let _ = writeln!(source, "    | Result.Error _ -> assert false");
        let _ = writeln!(source, "    | Result.Ok tensor ->");
        check_tensor(&mut source, "        ", ty, &case["shape"], &case["values"]);
        if case["values"].is_null() {
            // `nan,inf,-inf,NaN` and `Infinity,-Infinity,+1.5,1E3`.
            source.push_str(
                r##"        let values = Tensor.as_array (ref tensor)
        assert (Math.is_nan values[0] && values[1] == 1.0 / 0.0 && values[2] == -1.0 / 0.0 && Math.is_nan values[3])
        assert (values[4] == 1.0 / 0.0 && values[5] == -1.0 / 0.0 && values[6] == 1.5 && values[7] == 1000.0)
"##,
            );
        }
        source.push('\n');
    }
    source.push_str(
        r##"test "csv errors and forms" =
    let ragged: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.csv ()) u8"1,2\n3\n"
    match ragged with
    | Result.Error (TensorCodec.CsvParseError (line, _)) -> assert (line == 2)
    | _ -> assert false
    let word: Result<Tensor<i64>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.csv ()) u8"1,2\n\n3,x\n"
    match word with
    | Result.Error (TensorCodec.CsvParseError (line, _)) -> assert (line == 3)
    | _ -> assert false
    let hole: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.csv ()) u8"1,,3\n"
    match hole with
    | Result.Error (TensorCodec.CsvParseError (line, _)) -> assert (line == 1)
    | _ -> assert false
    let none: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.csv ()) u8"# only a comment\n\n"
    let empty = Result.get none
    let empty_window = Tensor.view (ref empty)
    assert (Array.copy (Tensor.shape (ref empty_window)) == [0, 0])
    let flags: Result<Tensor<bool>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.tsv ()) u8"true\tfalse"
    assert (Tensor.to_array (Result.get flags) == [true, false])
    // What `encode_csv` writes, `decode_csv` reads to the same bits, for every form of rank.
    let values = Tensor.of_array [2, 3] [0.1, -0.0, 1.0e300, 5.0e-324, 1.0 / 0.0, 123456789.125]
    let window = Tensor.view (ref values)
    let written = Result.get (TensorCodec.encode_csv (TensorCodec.csv ()) (ref window))
    let again: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.csv ()) (ref written)
    let back = Result.get again
    assert (TensorCodec.to_raw (Tensor.view (ref back)) == TensorCodec.to_raw (ref window))
    let vector = Tensor.of_array [3] [1, 2, 3]
    assert (Result.get (TensorCodec.encode_csv (TensorCodec.csv ()) (Tensor.view (ref vector))) == u8"1\n2\n3\n")
    let scalar = Tensor.scalar 7
    assert (Result.get (TensorCodec.encode_csv (TensorCodec.csv ()) (Tensor.view (ref scalar))) == u8"7\n")
    let spaced = TensorCodec.CsvOptions { delimiter: u8"", skip_rows: 0, comment: u8"//" }
    let matrix = Tensor.of_array [2, 2] [1, 2, 3, 4]
    assert (Result.get (TensorCodec.encode_csv spaced (Tensor.view (ref matrix))) == u8"1 2\n3 4\n")
    let loose: Result<Tensor<i64>, TensorCodec.Error> = TensorCodec.decode_csv (TensorCodec.CsvOptions { delimiter: u8"", skip_rows: 0, comment: u8"//" }) u8"  1 \t 2  // one\n3   4\n"
    assert (Tensor.to_array (Result.get loose) == [1, 2, 3, 4])
    let cube = Tensor.of_array [1, 1, 1] [1]
    match TensorCodec.encode_csv (TensorCodec.csv ()) (Tensor.view (ref cube)) with
    | Result.Error (TensorCodec.UnsupportedRank rank) -> assert (rank == 3)
    | _ -> assert false
"##,
    );
    assert_eq!(run_tz_test("csv", &source), cases.len() + 1);
}

#[test]
fn archives_written_by_tsuzuri_read_back_with_every_option() {
    let source = r##"test "npz written and read back" =
    let weights = Tensor.init [40, 25] (\index -> to_float (index % 9) * 0.5)
    let labels = Tensor.of_array [3] [1i64, -2i64, 3i64]
    let complex = Tensor.of_array [2] [Complex.of 1.5 (-2.0), Complex.of 0.0 4.0]
    let halves = Tensor.of_array [2] [1.5f16, -0.25f16]
    let empty: Tensor<f32> = Tensor.zeros [0, 3]
    let tensors = [("weights", TensorCodec.to_raw (Tensor.view (ref weights))), ("labels", TensorCodec.to_raw (Tensor.view (ref labels))), ("複素", TensorCodec.to_raw (Tensor.view (ref complex))), ("halves", TensorCodec.to_raw (Tensor.view (ref halves))), ("empty", TensorCodec.to_raw (Tensor.view (ref empty)))]
    let mut sizes: Vec<i64> = Vec.empty()
    for compress in [false, true] do
        for zip64 in [false, true] do
            let file = Result.get (TensorCodec.encode_npz_with (TensorCodec.NpzOptions { compress: compress, zip64: zip64 }) (ref tensors))
            // The archive is the same bytes each time.
            assert (Result.get (TensorCodec.encode_npz_with (TensorCodec.NpzOptions { compress: compress, zip64: zip64 }) (ref tensors)) == file)
            sizes = Vec.push_owned file.length sizes
            let archive = Result.get (TensorCodec.decode_npz file)
            assert (TensorCodec.archive_names (ref archive) == ["weights", "labels", "複素", "halves", "empty"])
            let first: Result<Tensor<f64>, TensorCodec.Error> = TensorCodec.archive_get "weights" (ref archive)
            assert (Result.get first == weights)
            let second: Result<Tensor<i64>, TensorCodec.Error> = TensorCodec.archive_get "labels" (ref archive)
            assert (Result.get second == labels)
            let third: Result<Tensor<Complex<f64>>, TensorCodec.Error> = TensorCodec.archive_get "複素" (ref archive)
            let parts = Tensor.to_array (Result.get third)
            assert (parts[0].re == 1.5 && parts[0].im == -2.0 && parts[1].re == 0.0 && parts[1].im == 4.0)
            let fourth: Result<Tensor<f16>, TensorCodec.Error> = TensorCodec.archive_get "halves" (ref archive)
            assert (Tensor.to_array (Result.get fourth) == [1.5f16, -0.25f16])
            let fifth: Result<Tensor<f32>, TensorCodec.Error> = TensorCodec.archive_get "empty" (ref archive)
            let nothing = Result.get fifth
            let window = Tensor.view (ref nothing)
            assert (Array.copy (Tensor.shape (ref window)) == [0, 3])
            // A stored entry can be found in place; a deflated one cannot.
            match TensorCodec.archive_inspect "weights" (ref archive) with
            | Result.Ok header -> assert ((header.data_offset < 0) == compress && header.shape == [40, 25])
            | Result.Error _ -> assert false
    // Deflating helps, and the ZIP64 records cost a fixed number of bytes.
    assert (sizes[2] < sizes[0] / 4 && sizes[1] > sizes[0] && sizes[1] - sizes[0] < 400)
    assert ((Result.get (TensorCodec.encode_npz (ref tensors))).length == sizes[0])
    assert ((Result.get (TensorCodec.encode_npz_compressed (ref tensors))).length == sizes[2])

test "npz names that cannot be written" =
    let t = Tensor.of_array [1] [1.0]
    match TensorCodec.encode_npz [("a", TensorCodec.to_raw (Tensor.view (ref t))), ("a", TensorCodec.to_raw (Tensor.view (ref t)))] with
    | Result.Error (TensorCodec.InvalidArchive _) -> ()
    | _ -> assert false
    match TensorCodec.encode_npz [("", TensorCodec.to_raw (Tensor.view (ref t)))] with
    | Result.Error (TensorCodec.InvalidArchive _) -> ()
    | _ -> assert false
    let none: [(string * TensorCodec.Raw)] = []
    let archive = Result.get (TensorCodec.decode_npz (Result.get (TensorCodec.encode_npz (ref none))))
    assert ((TensorCodec.archive_names (ref archive)).length == 0)

test "every element type has its dtype" =
    let bools: [bool] = []
    let bytes: [i8] = []
    let shorts: [i16] = []
    let ints: [i32] = []
    let longs: [i64] = []
    let ubytes: [i8u] = []
    let ushorts: [i16u] = []
    let uints: [i32u] = []
    let ulongs: [i64u] = []
    let halves: [f16] = []
    let singles: [f32] = []
    let doubles: [f64] = []
    let small: [Complex<f32>] = []
    let large: [Complex<f64>] = []
    let names = [TensorCodec.dtype_name (TensorCodec.element_dtype (ref bools)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref bytes)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref shorts)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref ints)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref longs)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref ubytes)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref ushorts)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref uints)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref ulongs)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref halves)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref singles)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref doubles)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref small)), TensorCodec.dtype_name (TensorCodec.element_dtype (ref large))]
    assert (names == ["bool", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64", "float16", "float32", "float64", "complex64", "complex128"])
    assert (Display.display (ref TensorCodec.Complex128) == "complex128")
    assert (Display.display (ref (TensorCodec.DtypeMismatch ("float64", "int8"))) == "expected dtype float64, found int8")
"##;
    assert_eq!(run_tz_test("written", source), 3);
}

fn rejects(source: &str, code: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
}

#[test]
fn the_primitives_and_the_representations_stay_inside_the_std_modules() {
    // The byte and mapping primitives are private to their modules.
    rejects(
        "let values = [1.5]\nlet bytes: [ubyte] = TensorCodec.__to_bytes (ref values)\nbytes.length\n",
        "E1022",
    );
    rejects(
        "let bytes = [0ubyte]\nlet values: [f64] = TensorCodec.__of_bytes 0 0 (ref bytes)\nvalues.length\n",
        "E1022",
    );
    rejects(
        "let owner = 1\nlet slice: ref [f64] = TensorIO.__mmap_slice 4096 8 (ref owner)\nslice.length\n",
        "E1022",
    );
    rejects("Os.__unmap 4096 8\n", "E1022");
    // An archive, a mapped tensor and the proof of an element type cannot be made or taken apart.
    rejects(
        "let archive = TensorCodec.Archive { bytes: [], entries: [], metadata: [], zip: true }\n1\n",
        "E1022",
    );
    rejects(
        "let proof: TensorCodec.ElementType<string> = TensorCodec.ElementType { dtype: TensorCodec.U8 }\n1\n",
        "E1022",
    );
    rejects(
        "def address :: ref TensorIO.MappedTensor -> i64\nfn address mapped = mapped.data\n1\n",
        "E1022",
    );
}

#[test]
fn the_element_class_has_no_instances_outside_the_std_module() {
    // A type that holds a pointer would otherwise be read out of mapped bytes by `TensorIO.view`.
    rejects(
        r##"record Name { text: string }
instance TensorElement.TensorElement<Name> {
    fn element_type _unit = TensorCodec.ElementType { dtype: TensorCodec.U64 }
    fn to_le_bytes _values = []
    fn of_le_bytes _offset _count _bytes = []
}
1
"##,
        "E1022",
    );
}

#[test]
fn a_window_of_a_mapped_file_cannot_outlive_the_mapping() {
    // Returned out of the function that owns the mapping.
    rejects(
        r##"def leak :: own TensorIO.MappedTensor -> Tensor.View<f64>
fn leak mapped = TensorIO.view (ref mapped)
1
"##,
        "E1013",
    );
    // Used after the mapping has been moved away.
    rejects(
        r##"def consume :: own TensorIO.MappedTensor -> i64
fn consume _mapped = 0

def misuse :: own TensorIO.MappedTensor -> i64
fn misuse mapped =
    let window: Tensor.View<f64> = TensorIO.view (ref mapped)
    let gone = consume mapped
    gone + Tensor.count (ref window)
1
"##,
        "E1014",
    );
    // Carried out of the scoped form.
    rejects(
        r##"def window_of :: own string -> IO<Result<Tensor.View<f64>, TensorIO.Error>>
fn window_of path = TensorIO.with_mmap_npy path (\mapped -> TensorIO.view mapped)
1
"##,
        "E1013",
    );
}
