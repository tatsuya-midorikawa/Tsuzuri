use tsuzuri::{analyze, llvm};

fn emit(source: &str, wasm: bool) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
}

/// Both notations and the parenthesis-free argument forms must reach the same typed tree.
fn same_ir(sources: &[&str]) {
    for wasm in [false, true] {
        let expected = emit(sources[0], wasm);
        assert!(expected.contains(" @tz.fn.Main."), "{expected}");
        for source in &sources[1..] {
            assert_eq!(
                expected,
                emit(source, wasm),
                "wasm={wasm}\n{}\n{source}",
                sources[0]
            );
        }
    }
}

fn accepts(source: &str) {
    emit(source, false);
}

fn rejects(source: &str, code: &str, message: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    assert!(
        error.message.contains(message),
        "{source}\n{}",
        error.message
    );
}

#[test]
fn keyword_and_symbol_notations_lower_identically() {
    same_ir(&[
        "fn f(x: i64) -> i64 { let r = &x; *r }",
        "fn f(x: i64) -> i64 { let r = ref x; deref r }",
    ]);
    same_ir(&[
        "fn f() -> i64 { let mut x = 1; let r = &mut x; *r = *r + 2; x }",
        "fn f() -> i64 { let mut x = 1; let r = ref mut x; deref r = deref r + 2; x }",
    ]);
    same_ir(&[
        "fn f() -> i64 { let mut x = 1; let r = &mut x; let s = &mut *r; *s = 2; let t = &*r; *t }",
        "fn f() -> i64 { let mut x = 1; let r = ref mut x; let s = ref mut r; deref s = 2; let t = ref r; deref t }",
    ]);
    same_ir(&[
        "record P { x: i64, y: i64 }\nfn f(p: &P, xs: &[i64]) -> i64 { let a = &p.x; let b = &xs[1]; *a + *b }",
        "record P { x: i64, y: i64 }\nfn f(p: ref P, xs: ref [i64]) -> i64 { let a = ref p.x; let b = ref xs[1]; deref a + deref b }",
    ]);
    same_ir(&[
        "fn replace(text: &mut string) -> unit { *text = \"changed\"; }\nfn f() -> i64 { let mut s = \"x\"; replace(&mut s); s.length }",
        "fn replace(text: ref mut string) -> unit { deref text = \"changed\"; }\nfn f() -> i64 { let mut s = \"x\"; replace(ref mut s); s.length }",
    ]);
}

#[test]
fn implicit_arguments_lower_like_explicit_borrows() {
    let declarations = "def add :: &mut i64 -> i64 -> unit\nfn add r n = *r = *r + n\n\
                        def read :: &i64 -> i64\nfn read r = *r\n\
                        def value :: i64 -> i64\nfn value n = n\n";
    same_ir(&[
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut x = 1; add (&mut x) 2; let r = &mut x; add (&mut *r) 3; let v = read (&*r); add (&mut *r) v; let n = value (*r); read (&x) + n }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut x = 1; add (ref mut x) 2; let r = ref mut x; add (ref mut r) 3; let v = read ref r; add (ref mut r) v; let n = value (deref r); read ref x + n }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut x = 1; add x 2; let r = ref mut x; add r 3; let v = read r; add r v; let n = value r; read x + n }}"
        ),
    ]);
}

#[test]
fn implicit_arguments_accept_reference_returning_calls() {
    let declarations = "def loan :: ref mut i64 -> ref mut i64\nfn loan r = r\n\
                        def read :: ref i64 -> i64\nfn read r = deref r\n\
                        def value :: i64 -> i64\nfn value n = n\n";
    same_ir(&[
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut n = 3; let first = read (ref (loan (ref mut n))); let second = value (deref (loan (ref mut n))); first + second + n }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut n = 3; let first = read (loan n); let second = value (loan n); first + second + n }}"
        ),
    ]);
    accepts(
        "def mark :: ref mut i64 -> unit\nfn mark r = deref r = 2\n\
         let twice = r -> { mark r; mark r; }\nlet mut n = 1\ntwice n\nn",
    );
}

#[test]
fn implicit_arguments_cover_places_functions_and_pipelines() {
    let declarations = "def apply :: ('a -> 'b) -> 'a -> 'b\nfn apply f x = f x\n\
                        def add :: ref i64 -> i64 -> i64\nfn add r n = deref r + n\n";
    same_ir(&[
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let text = \"hello\"; let length = String.length; let count = apply length (ref text); let increment = add (ref count); increment (ref text |> length) }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let text = \"hello\"; let length = String.length; let count = apply length text; let increment = add count; increment (text |> length) }}"
        ),
    ]);
    let declarations = "record Box { text: string }\n";
    same_ir(&[
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let box = Box {{ text: \"record\" }}; let array = [\"array\"]; let list = [|\"list\"|]; String.length (ref box.text) + String.length (ref array[0]) + String.length (ref list[0]) }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let box = Box {{ text: \"record\" }}; let array = [\"array\"]; let list = [|\"list\"|]; String.length box.text + String.length array[0] + String.length list[0] }}"
        ),
    ]);
    same_ir(&[
        "def f :: i64\nfn f = { let text = \"\\u{1f600}\"; let bytes = u8\"\\u{1f600}\"; let copied = clone_string ref text; String.length ref text + Utf8String.length ref bytes + copied.length + text.length }",
        "def f :: i64\nfn f = { let text = \"\\u{1f600}\"; let bytes = u8\"\\u{1f600}\"; let copied = clone_string text; String.length text + Utf8String.length bytes + copied.length + text.length }",
    ]);
    same_ir(&[
        "def f :: i64\nfn f = { let text = \"lambda\"; (fx (value: ref string) -> value.length) (ref text) }",
        "def f :: i64\nfn f = { let text = \"lambda\"; (fx (value: ref string) -> value.length) text }",
    ]);
    let declarations = "class Size<'a> { def size :: ref 'a -> i64 }\n\
                        instance Size<string> { fn size text = text.length }\n";
    same_ir(&[
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let text = \"class\"; Size.size ref text }}"
        ),
        &format!("{declarations}def f :: i64\nfn f = {{ let text = \"class\"; Size.size text }}"),
    ]);
}

#[test]
fn implicit_arguments_preserve_inference_and_owned_parameters() {
    accepts(
        "def read :: ref 'a -> 'a\nfn read r = deref r\n\
         def relay :: 'a -> 'a\nfn relay x = read x\n\
         let n: i32 = relay 42\nn",
    );
    accepts(
        "def identity :: 'a -> 'a\nfn identity x = x\n\
         def narrow :: i32 -> [i32] -> i32\nfn narrow n values = n + values[0]\n\
         narrow (identity 20) (identity [22])",
    );
    rejects(
        "def take :: string -> i64\nfn take text = text.length\n\
         let text = \"owned\"\nlet n = take text\ntext.length + n",
        "E1012",
        "moved",
    );
    rejects(
        "def identity :: 'a -> 'a\nfn identity x = x\n\
         let mut n = 1\nlet r = ref mut n\nlet moved = identity r\nderef r",
        "E1012",
        "moved",
    );
    rejects("let n = 1\nlet r: ref i64 = n\n0", "E1003", "ref i64");
    rejects(
        "def read :: i64 -> i64\nfn read n = n\nlet n = 1\nread ref n",
        "E1003",
        "expected i64",
    );
}

#[test]
fn implicit_arguments_preserve_borrow_safety() {
    let mark = "def mark :: ref mut i64 -> unit\nfn mark r = deref r = 2\n";
    for source in [
        "let n = 1\nmark n\nn",
        "let mut n = 1\nlet shared = ref n\nmark shared\nn",
        "record Box { n: i64 }\nlet mut box = Box { n: 1 }\nmark box.n\n0",
        "let mut values = [1]\nmark values[0]\n0",
        "let mut values = [|1|]\nmark values[0]\n0",
        "let mut n = 1\nlet shared = ref n\nmark n\nderef shared",
    ] {
        rejects(&format!("{mark}{source}"), "E1014", "");
    }
    let both = "def both :: ref mut i64 -> ref mut i64 -> unit\nfn both left right = ()\n";
    for source in [
        "let mut n = 1\nboth n n\nn",
        "let mut n = 1\nlet r = ref mut n\nboth r r\nn",
    ] {
        rejects(&format!("{both}{source}"), "E1014", "live borrow");
    }
    rejects(
        "def take :: string -> i64\nfn take text = text.length\n\
         let text = \"owned\"\nlet r = ref text\ntake r",
        "E1012",
        "cannot move a non-Copy value out of a reference",
    );
    rejects(
        "def identity :: ref string -> ref string\nfn identity text = text\n\
         let escaped = { let text = \"local\"; identity text }\nescaped.length",
        "E1013",
        "does not live long enough",
    );
    rejects(
        "def add :: ref mut i64 -> i64 -> unit\nfn add r n = deref r = deref r + n\n\
         let mut n = 1\nlet later = add n\nlater 2\nn",
        "E1005",
        "cannot capture",
    );
    rejects(
        "def keep :: ref string -> i64 -> i64\nfn keep text n = text.length + n\n\
         let escaped = { let text = \"local\"; keep text }\nescaped 0",
        "E1013",
        "does not live long enough",
    );
    for argument in ["&r", "r"] {
        rejects(
            &format!(
                "def read :: ref ref i64 -> i64\nfn read r = deref deref r\ndef f :: i64\nfn f = {{ let n = 42; let r = ref n; read {argument} }}"
            ),
            "E1013",
            "nested borrowed values",
        );
    }
}

#[test]
fn prefix_arguments_need_no_parentheses() {
    let declarations = "def add :: &mut i64 -> i64 -> unit\nfn add r n = *r = *r + n\n\
                        def read :: &i64 -> i64\nfn read r = *r\n";
    same_ir(&[
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut x = 1; add (&mut x) 2; let r = &mut x; add (&mut *r) 3; let v = read (&*r); add (&mut *r) v; read (&x) }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut x = 1; add &mut x 2; let r = &mut x; add &mut *r 3; let v = read &*r; add &mut *r v; read &x }}"
        ),
        &format!(
            "{declarations}def f :: i64\nfn f = {{ let mut x = 1; add (ref mut x) 2; let r = ref mut x; add (ref mut r) 3; let v = read ref r; add (ref mut r) v; read ref x }}"
        ),
    ]);
    same_ir(&[
        "def pick :: i64 -> i64 -> i64\nfn pick a b = a - b\ndef f :: &i64 -> i64\nfn f r = pick (*r) 1",
        "def pick :: i64 -> i64 -> i64\nfn pick a b = a - b\ndef f :: &i64 -> i64\nfn f r = pick *r 1",
        "def pick :: i64 -> i64 -> i64\nfn pick a b = a - b\ndef f :: &i64 -> i64\nfn f r = pick (deref r) 1",
    ]);
    // Spaced or unspaced operators stay binary; only a tight prefix after whitespace is an argument.
    same_ir(&[
        "fn f(a: i64, b: i64) -> i64 { (a * b) + (a & b) }",
        "fn f(a: i64, b: i64) -> i64 { a*b + (a&b) }",
        "fn f(a: i64, b: i64) -> i64 { a * b + (a & b) }",
    ]);
    same_ir(&[
        "fn f(a: bool, b: bool) -> bool { a && b }",
        "fn f(a: bool, b: bool) -> bool { a &&b }",
    ]);
    // At the head of an expression a symbol prefix still covers the whole application.
    same_ir(&[
        "def get :: &i64 -> &i64\nfn get r = r\ndef f :: &i64 -> i64\nfn f r = *(get r)",
        "def get :: &i64 -> &i64\nfn get r = r\ndef f :: &i64 -> i64\nfn f r = *get r",
        "def get :: &i64 -> &i64\nfn get r = r\ndef f :: &i64 -> i64\nfn f r = deref (get r)",
    ]);
}

#[test]
fn keyword_forms_take_exactly_one_operand() {
    let mark = "def mark :: ref mut i64 -> i64 -> unit\nfn mark r n = deref r = n\n";
    rejects(
        &format!("{mark}let mut value = 0\nmark ref mut value 1\nvalue"),
        "E0002",
        "write '(ref mut value)'",
    );
    accepts(&format!(
        "{mark}let mut value = 0\nmark (ref mut value) 1\nvalue"
    ));
    let get = "def get :: ref i64 -> ref i64\nfn get r = ref r\n";
    rejects(
        &format!("{get}let x = 1\nderef get ref x"),
        "E0002",
        "write '(deref get)'",
    );
    accepts(&format!("{get}let x = 1\nderef (get ref x)"));
    accepts(&format!("{get}let x = 1\nlet r = ref x\nderef get(r)"));
    rejects("let x = 1\nlet r = ref x (x)\n0", "E0002", "'ref' takes");
    // The last argument, operators, casts, and assignment may follow a keyword form.
    accepts(
        "def replace :: ref mut string -> unit\nfn replace text = deref text = \"new\"\n\
         let mut s = \"old\"\nreplace ref mut s\nlet r = ref s\nlet n = 2i32\n(deref r).length + deref ref n as i64",
    );
}

#[test]
fn keyword_ref_reborrows_statically_known_references() {
    // `ref r` on `ref mut T` is a shared reborrow, so the exclusive reference stays usable.
    accepts(
        "def f :: i64\nfn f = { let mut x = 1; let r = ref mut x; let s = ref r; let v = deref s; deref r = v + 1; x }",
    );
    // A reborrow keeps one level: `ref rr` on `ref ref i64` is `&*rr`, still `ref ref i64`.
    rejects(
        "let v = 5\nlet r = ref v\nlet rr = &r\nlet back: ref i64 = ref rr\n0",
        "E1003",
        "expected i64, found ref i64",
    );
    accepts("let v = 5\nlet r = ref v\nlet rr = &r\nlet back: &&i64 = ref rr\n0");
    rejects(
        "let value = 1\nlet r = ref value\nlet m = ref mut r\n0",
        "E1014",
        "shared reference",
    );
    rejects(
        "let value = 1\nlet m = ref mut value\n0",
        "E1014",
        "'ref mut'",
    );
    // A type variable is a value type: `ref x` borrows `x: 'a` rather than reborrowing it.
    accepts(
        "def size :: ref 'a -> i64\nfn size r = 8\ndef f :: 'a -> i64\nfn f x = size ref x\ndef g :: i64\nfn g = f 1",
    );
    rejects(
        "def apply :: ('a -> 'b) -> 'a -> 'b\nfn apply f x = f x\nlet v = 1\nlet r = ref v\napply (p -> deref (ref p)) r",
        "E1015",
        "'ref' must know",
    );
    accepts(
        "def apply :: ('a -> 'b) -> 'a -> 'b\nfn apply f x = f x\nlet v = 1\napply (p -> deref (ref p)) v",
    );
    accepts(
        "def apply :: (ref i64 -> i64) -> ref i64 -> i64\nfn apply f x = f x\nlet v = 1\nlet r = ref v\napply (p -> deref (ref p)) r",
    );
}

#[test]
fn reference_types_accept_keywords_and_double_ampersand() {
    rejects("let x: ref ref mut i64 = 1\n0", "E1003", "ref ref mut i64");
    rejects("let x: &&mut i64 = 1\n0", "E1003", "ref ref mut i64");
    rejects("let x: & &i64 = 1\n0", "E1003", "ref ref i64");
    rejects(
        "let x: ref (i64 -> i64) = 1\n0",
        "E1003",
        "ref (i64 -> i64)",
    );
    rejects(
        "let x: ref mut [|ref string|] * ref Task<i64> = 1\n0",
        "E1003",
        "(ref mut [|ref string|] * ref Task<i64>)",
    );
    same_ir(&[
        "def f :: &mut [i64] -> &i64 -> i64\nfn f xs r = xs.length + *r",
        "def f :: ref mut [i64] -> ref i64 -> i64\nfn f xs r = xs.length + deref r",
    ]);
}

#[test]
fn diagnostics_guide_between_notations() {
    rejects(
        "def mark :: ref mut i64 -> unit\nfn mark r = deref r = 3\nlet mut value = 0\nmark mut value\nvalue",
        "E0002",
        "write 'ref mut x'",
    );
    rejects("let x = 3\nlet y = 2 *x\ny", "E1005", "'a * b'");
    rejects("let x = 3\nlet r = &x\nlet y = x &r\ny", "E1005", "'a & b'");
    let error = analyze("let x = 3\nlet r = &x\nlet y = x (*r)\ny").unwrap_err();
    assert_eq!(error.code, "E1005");
    assert!(!error.message.contains("spaces"), "{}", error.message);
    rejects(
        "let x = 3\nderef x",
        "E1005",
        "'deref' requires a reference",
    );
    rejects("let x = 3\n*x", "E1005", "dereference requires a reference");
}

#[test]
fn keyword_forms_work_in_pattern_arguments_and_guards() {
    let above = "def (|Above|_|) :: &i64 -> i64 -> bool\nfn (|Above|_|) limit n = n > *limit\n";
    same_ir(&[
        &format!(
            "{above}def f :: i64 -> i64\nfn f n = {{ let limit = 3; match n with | Above (&limit) -> 1 | _ -> 0 }}"
        ),
        &format!(
            "{above}def f :: i64 -> i64\nfn f n = {{ let limit = 3; match n with | Above (ref limit) -> 1 | _ -> 0 }}"
        ),
        &format!(
            "{above}def f :: i64 -> i64\nfn f n = {{ let limit = 3; match n with | Above limit -> 1 | _ -> 0 }}"
        ),
    ]);
    let positive = "def positive :: &i64 -> bool\nfn positive r = *r > 0\n";
    same_ir(&[
        &format!(
            "{positive}def classify :: i64 -> i64\nfn classify x\n    | positive (&x) -> 1\n    | otherwise -> 0"
        ),
        &format!(
            "{positive}def classify :: i64 -> i64\nfn classify x\n    | positive ref x -> 1\n    | otherwise -> 0"
        ),
        &format!(
            "{positive}def classify :: i64 -> i64\nfn classify x\n    | positive x -> 1\n    | otherwise -> 0"
        ),
    ]);
}

#[test]
fn ref_and_deref_are_reserved_words() {
    for source in [
        "let ref = 1\n0",
        "def deref :: i64 -> i64\nfn deref x = x\n0",
        "record R { ref: i64 }\n0",
    ] {
        rejects(source, "E0002", "expected an identifier");
    }
    accepts("let ref_count = 1\nlet dereference = 2\nlet refs = 3\nref_count + dereference + refs");
    let error =
        tsuzuri::analyze_modules(&[("ref.tz", "def one :: i64\nfn one = 1"), ("Main.tz", "0")])
            .expect_err("keyword module name");
    assert_eq!(error.code, "E1011", "{}", error.message);
}
