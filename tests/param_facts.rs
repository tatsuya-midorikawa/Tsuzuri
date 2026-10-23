use tsuzuri::{analyze, llvm};

fn emit(source: &str, wasm: bool) -> String {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
    assert_eq!(
        ir,
        llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
    );
    ir
}

fn function<'a>(ir: &'a str, name: &str) -> &'a str {
    let symbol = format!("@tz.fn.Main.{name}(");
    let start = ir.find(&symbol).unwrap_or_else(|| panic!("{symbol}\n{ir}"));
    let start = ir[..start].rfind("define internal ").unwrap();
    &ir[start..start + ir[start..].find("\n}\n").unwrap() + 3]
}

fn header<'a>(ir: &'a str, name: &str) -> &'a str {
    function(ir, name).lines().next().unwrap()
}

#[test]
fn pointer_attributes_and_portable_layout_bounds() {
    let source = r#"
record Pair<'a> { flag: bool, value: 'a }
record Empty { }
record Cleanup { value: i64 }
instance Drop<Cleanup> { fn drop value = () }
union Chain = End | Link of (i64 * Chain)
union Color = Red | Green | Blue
union Shape = Circle of f64 | Rect of f64 * f64 | Blank
union Huge = Big of i128 | Little of i8 | Missing
union Lane8 = Eight of f32x8 | Byte of i8 | Absent8
def shared :: ref i64 -> i64 = \value -> deref value
def change :: ref mut i64 -> i64 -> unit = \value next -> deref value = next
def pair :: ref Pair<i128> -> i128 = \value -> value.value
def nested :: ref (ref i64) -> i64 = \value -> deref (deref value)
def zero :: ref [i64; 0] -> i64 = \value -> value.length
def empty :: ref Empty -> i64 = \value -> 0
def wide :: ref f64x4 -> i64 = \value -> 0
def cleanup :: ref Cleanup -> i64 = \value -> value.value
def chain :: ref Chain -> i64 = \value -> 0
def color :: ref Color -> i64 = \value -> 0
def shape :: ref Shape -> i64 = \value -> 0
def maybe_text :: ref Maybe<string> -> i64 = \value -> 0
def huge :: ref Huge -> i64 = \value -> 0
def lanes :: ref Lane8 -> i64 = \value -> 0
def dyn_pointer :: ref dyn Display -> i64 = \value -> 0
def function_pointer :: ref (i64 -> i64) -> i64 = \value -> 0
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        assert!(header(&ir, "shared").contains(
            "ptr noalias nocapture noundef nonnull readonly align 8 dereferenceable(8) %arg0"
        ));
        assert!(
            header(&ir, "change")
                .contains("ptr noalias nocapture noundef nonnull align 8 dereferenceable(8) %arg0")
        );
        assert!(header(&ir, "pair").contains("align 8 dereferenceable(24)"));
        assert!(!header(&ir, "nested").contains("noalias"));
        assert!(header(&ir, "nested").contains("align 4 dereferenceable(4)"));
        assert!(header(&ir, "zero").contains("(ptr %arg0)"));
        assert!(header(&ir, "empty").contains("(ptr %arg0)"));
        assert!(header(&ir, "wide").contains("align 8 dereferenceable(32)"));
        assert!(header(&ir, "cleanup").contains("align 8 dereferenceable(16)"));
        assert!(header(&ir, "chain").contains("align 4 dereferenceable(4)"));
        // A union is a tag of its own width and a payload area at the largest payload alignment
        // (PM01); an `i128` payload only guarantees the 8-byte alignment of older LLVM. An option of
        // a string has no tag: it is the 16-byte string.
        assert!(header(&ir, "color").contains("align 1 dereferenceable(1)"));
        assert!(header(&ir, "shape").contains("align 8 dereferenceable(24)"));
        assert!(header(&ir, "maybe_text").contains("align 8 dereferenceable(16)"));
        assert!(header(&ir, "huge").contains("align 8 dereferenceable(24)"));
        assert!(header(&ir, "lanes").contains("align 8 dereferenceable(40)"));
        assert!(header(&ir, "dyn_pointer").contains("align 4 dereferenceable(8)"));
        assert!(header(&ir, "function_pointer").contains("align 4 dereferenceable(8)"));
    }
}

#[test]
fn returned_and_stored_loans_and_callbacks_do_not_get_nocapture() {
    let source = r#"
record View {r} { value: ref {r} i64 }
def identity :: ref i64 -> ref i64 = \value -> value
def view {r} :: ref {r} i64 -> View {r} = \value -> View { value: value }
def store {r s} :: ref {r} i64 -> ref mut {s} View {r} -> unit =
    \value target -> deref target = View { value: value }
def callback :: (ref i64 -> i64) -> ref i64 -> i64 = \f value -> f value
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        for name in ["identity", "view", "store", "callback"] {
            assert!(!header(&ir, name).contains("nocapture"), "{name}");
        }
        assert!(!function(&ir, "callback").contains("!alias.scope"));
        assert!(!header(&ir, "store").contains("memory("));
        // A pointer-containing value is not unique merely because its descriptor slot is unique.
        assert!(
            header(&ir, "store").contains("ptr noundef nonnull align 4 dereferenceable(4) %arg1")
        );
    }
}

#[test]
fn interior_mutable_and_hidden_state_are_not_readonly_or_shared_noalias() {
    let source = r#"
record Nested { cells: [Atomic<i64>] }
def atomic :: ref Atomic<i64> -> i64 = \value -> Atomic.load value
def nested :: ref Nested -> i64 = \value -> Atomic.load (ref value.cells[0])
def mutex :: ref Mutex<i64> -> i64 = \value -> Mutex.with_lock value (\inner -> deref inner)
def counted :: ref Arc<i64> -> i64 = \value -> Arc.strong_count value
def closure :: ref (i64 -> i64) -> i64 = \value -> (deref value) 1
def channel :: ref Channel.Sender<i64> -> i64 = \value -> { let sent = Channel.send value 1; 1 }
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        for name in ["atomic", "nested", "mutex", "counted", "closure", "channel"] {
            let header = header(&ir, name);
            assert!(header.contains("noundef nonnull"), "{header}");
            assert!(!header.contains("readonly"), "{header}");
            assert!(!header.contains("noalias"), "{header}");
            assert!(!header.contains("memory("), "{header}");
            assert!(!function(&ir, name).contains("!tbaa"), "{name}");
        }
    }
}

#[test]
fn descriptor_abi_scopes_and_scalar_tbaa_are_proven_together() {
    let source = r#"
def update :: ref mut [i64..] -> ref [i64] -> i64 -> unit =
    \output input index -> Array.write index input[index] output
def same :: ref [i64] -> ref [i64] -> i64 = \left right -> left[0] + right[0]
def bytes :: ref mut [ubyte..] -> ref [ubyte] -> unit =
    \output input -> Array.write 0 input[0] output
def different :: ref mut i64 -> ref f64 -> unit = \output input -> deref output = (deref input) as i64
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        assert!(
            header(&ir, "update").contains("(%tz.array %arg0, %tz.array %arg1, i64 noundef %arg2)")
        );
        let update = function(&ir, "update");
        assert!(update.contains("!alias.scope"));
        assert!(update.contains("!noalias"));
        assert!(update.contains("!tbaa"));
        assert!(update.contains("@llvm.experimental.noalias.scope.decl"));
        assert!(update.lines().any(|line| line.contains("call ")
            && line.contains("Array.write")
            && line.contains("!alias.scope")
            && line.contains("!noalias")));
        assert!(!update.contains("!nonnull"));
        assert!(!header(&ir, "update").contains("memory("));
        assert!(!function(&ir, "same").contains("!noalias"));
        assert!(!function(&ir, "same").contains("!alias.scope"));
        assert!(function(&ir, "same").contains("!tbaa"));
        assert!(!function(&ir, "bytes").contains("!tbaa"));
        assert!(function(&ir, "bytes").contains("!noalias"));
        assert!(ir.contains("!\"i64\""));
        assert!(ir.contains("!\"double\""));
        assert!(!ir.contains("!\"i64u\""));
    }
}

#[test]
fn function_memory_effects_and_defined_scalars_fail_closed() {
    let source = r#"
extern def observe :: i64 -> i64
def add :: i64 -> i64 -> i64 = \a b -> a + b
def read :: ref i64 -> i64 = \value -> deref value
def write :: ref mut i64 -> i64 -> unit = \value next -> deref value = next
def exchange :: ref mut i64 -> i64 -> i64 =
    \value next -> { let old = deref value; deref value = next; old }
def divide :: i64 -> i64 -> i64 = \a b -> a / b
def host :: i64 -> i64 = \value -> observe value
def allocated :: i64 -> [i64] = \count -> new [i64](count, \index -> index)
def union_result :: bool -> Maybe<i64> = \flag -> if flag then Maybe.Some 1 else Maybe.None
def rec self :: i64 -> i64 -> i64 = \n total ->
    if n == 0 then total else self (n - 1) (total + n)
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        assert!(header(&ir, "add").starts_with("define internal noundef i64"));
        assert!(header(&ir, "add").contains("i64 noundef %arg0"));
        assert!(header(&ir, "add").contains("memory(none)"));
        assert!(header(&ir, "read").contains("memory(argmem: read)"));
        assert!(header(&ir, "write").contains("memory(argmem: write)"));
        assert!(header(&ir, "exchange").contains("memory(argmem: readwrite)"));
        assert!(!header(&ir, "divide").starts_with("define internal noundef"));
        assert!(!header(&ir, "host").contains("memory("));
        assert!(!header(&ir, "allocated").contains("memory("));
        assert!(function(&ir, "allocated").contains("call nonnull ptr @tz.alloc"));
        assert!(!header(&ir, "union_result").starts_with("define internal noundef"));
        assert!(!header(&ir, "self").contains("memory("));
        assert!(function(&ir, "self").contains("phi i64"));
        assert!(!function(&ir, "self").contains("!alias.scope"));
    }
}

#[test]
fn traps_and_unproven_termination_are_not_memory_free() {
    let source = r#"
def step :: i64 -> unit = \amount -> for _ in 1 .. amount .. 2 do ()
def assertion :: bool -> unit = \condition -> assert condition
def never :: unit -> unit = \unused -> unreachable ()
def checked :: i64 -> unit = \value -> { let result = @checked (value + 1); () }
def looping :: unit -> unit = \unused -> while true do ()
def conditional :: bool -> unit = \condition -> if condition then assert false else ()
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        for name in [
            "step",
            "assertion",
            "never",
            "checked",
            "looping",
            "conditional",
        ] {
            assert!(
                !header(&ir, name).contains("memory("),
                "{}",
                header(&ir, name)
            );
        }
        assert!(function(&ir, "step").contains("@llvm.trap"));
        assert!(function(&ir, "checked").contains("@llvm.trap"));
        assert!(function(&ir, "looping").contains("br label"));
    }
}

#[test]
fn public_wrappers_do_not_acquire_internal_facts() {
    let source = r#"
export def sum :: ref [i64] -> i64 = \values -> if values.length == 0 then 0 else values[0]
export def next :: i64 -> i64 = \value -> value + 1
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        for name in ["sum", "next"] {
            let wrapper = ir
                .lines()
                .find(|line| line.starts_with("define ") && line.contains(&format!("@tz_{name}(")))
                .unwrap();
            for fact in [
                "noalias",
                "nocapture",
                "nonnull",
                "noundef",
                "readonly",
                "memory(",
            ] {
                assert!(!wrapper.contains(fact), "{wrapper}");
            }
        }
    }
}

#[test]
fn overlapping_union_payloads_and_scalar_spill_reloads() {
    let source = r#"
union Mixed = Word of i64 | Real of f64
def mixed :: ref Mixed -> i64 = \value -> match value with
    | Mixed.Word number -> number
    | Mixed.Real number -> number as i64
def scalar :: ref i64 -> i64 = \value -> deref value
"#;
    for wasm in [false, true] {
        let ir = emit(source, wasm);
        assert!(!function(&ir, "mixed").contains("!tbaa"));
        assert!(!function(&ir, "mixed").contains("!alias.scope"));
        assert!(function(&ir, "scalar").contains("!nonnull"));
        assert!(function(&ir, "scalar").contains("!tbaa"));
    }
}

#[test]
fn captured_workers_and_task_bodies_keep_their_existing_contracts() {
    let module = analyze(
        "def make :: ref i64 -> bool -> (unit -> i64) = \\value flag -> \
         if flag then \\unused -> deref value else \\unused -> deref value\n\
             def work :: i64 -> Task<i64> = \\value -> task { return value + 1 }\n",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let mut excluded = 0;
        for header in ir.lines().filter(|line| {
            line.starts_with("define internal ")
                && (line.contains("@tz.fn.$lambda.") || line.contains("@tz.fn.$task."))
        }) {
            excluded += 1;
            for fact in [
                "noalias",
                "nocapture",
                "nonnull",
                "noundef",
                "readonly",
                "memory(",
            ] {
                assert!(!header.contains(fact), "{header}");
            }
        }
        assert!(excluded >= 2);
        assert!(ir.contains("@tz.fn.$lambda."));
        assert!(ir.contains("@tz.fn.$task."));
    }
}
