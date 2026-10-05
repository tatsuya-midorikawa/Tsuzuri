use tsuzuri::check::CheckedModule;
use tsuzuri::{analyze, llvm};

fn accepts(source: &str) -> CheckedModule {
    let module = analyze(source)
        .unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    module
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

const RESOURCE: &str = "extern def drop_log :: i64 -> unit\nrecord Resource { id: i64, name: string }\ninstance Drop<Resource> { fn drop value = drop_log value.id }\n";
const HANDLE: &str = "extern def drop_log :: i64 -> unit\nrecord Handle<'a> { id: i64, value: 'a }\ninstance Drop<Handle<'a>> { fn drop value = drop_log value.id }\n";
const RECORDS_AND_UNIONS: &str =
    "only records and unions declared in this program can implement Drop";
const EVERY_INSTANTIATION: &str = "a Drop instance must cover every instantiation; write every type parameter as a distinct type variable";

#[test]
fn accepts_drop_instances_for_records_and_unions() {
    accepts(&format!(
        "{RESOURCE}union Slot = Empty | Full of i64\ninstance Drop<Slot> {{ fn drop _value = drop_log 0 }}\nexport def both :: i64\nfn both =\n    let slot = Full 2\n    let resource = Resource {{ id: 1, name: \"a\" }}\n    resource.id + (match slot with | Full value -> value | Empty -> 0)\n"
    ));
}

#[test]
fn accepts_generic_drop_instance_covering_the_type() {
    accepts(&format!(
        "{HANDLE}export def handles :: i64\nfn handles =\n    let first = Handle {{ id: 4, value: 4 }}\n    let second = Handle {{ id: 5, value: \"five\" }}\n    first.id + second.id\n"
    ));
}

#[test]
fn rejects_drop_for_builtin_and_std_types() {
    for head in ["i64", "[i64]", "(i64 * i64)", "Option<'a>"] {
        rejects(
            &format!("instance Drop<{head}> {{ fn drop _value = () }}"),
            "E1016",
            RECORDS_AND_UNIONS,
        );
    }
}

#[test]
fn rejects_partial_or_repeated_type_arguments() {
    rejects(
        "record Handle<'a> { value: 'a }\ninstance Drop<Handle<i64>> { fn drop _value = () }",
        "E1016",
        EVERY_INSTANTIATION,
    );
    rejects(
        "record Pair<'a, 'b> { left: 'a, right: 'b }\ninstance Drop<Pair<'a, 'a>> { fn drop _value = () }",
        "E1016",
        EVERY_INSTANTIATION,
    );
}

#[test]
fn rejects_constrained_drop_instance() {
    rejects(
        "record Handle<'a> { value: 'a }\ninstance Eq<'a> => Drop<Handle<'a>> { fn drop _value = () }",
        "E1016",
        "Drop instances cannot have constraints; drop must work for every instantiation",
    );
}

#[test]
fn keeps_existing_copy_and_deriving_errors() {
    rejects(
        "record Resource { id: i64 }\ninstance Copy<Resource> {}",
        "E1016",
        "built-in instances and marker classes cannot be overridden",
    );
    rejects(
        "record Resource { id: i64 } deriving (Drop)",
        "E1025",
        "only Eq, Ord, Display, Hash and Default can be derived",
    );
}

#[test]
fn rejects_duplicate_drop_instance() {
    rejects(
        &format!("{RESOURCE}instance Drop<Resource> {{ fn drop _value = () }}"),
        "E1016",
        "overlapping instance for Drop<Main.Resource>",
    );
}

#[test]
fn rejects_direct_drop_calls() {
    let message = "'Drop.drop' runs automatically when a value is dropped; let the value go out of scope or pass it to a function that consumes it";
    rejects(
        &format!(
            "{RESOURCE}def close :: Resource -> unit\nfn close value =\n    let mut owned = value\n    Drop.drop (ref mut owned)\n"
        ),
        "E1016",
        message,
    );
    rejects(
        "record Twice { id: i64 }\ninstance Drop<Twice> { fn drop value = Drop.drop value }",
        "E1016",
        message,
    );
}

const COUNTER: &str = "extern def drop_log :: i64 -> unit\nrecord Counter { id: i64 }\ninstance Drop<Counter> { fn drop value = drop_log value.id }\ndef take :: Counter -> i64 = \\counter -> counter.id\n";

#[test]
fn drop_types_are_not_copy() {
    let twice =
        "export def twice :: i64\nfn twice =\n    let r = Counter { id: 1 }\n    take r + take r\n";
    let copy = COUNTER.replace(
        "instance Drop<Counter> { fn drop value = drop_log value.id }\n",
        "",
    );
    let module = accepts(&format!("{copy}{twice}"));
    let types = module.types();
    let counter = module
        .records
        .iter()
        .position(|record| record.name == "Main.Counter")
        .unwrap();
    assert!(tsuzuri::check::Type::Record(counter, Box::default()).is_copy(&types));
    rejects(
        &format!("{COUNTER}{twice}"),
        "E1012",
        "use of moved or partially moved value 'r'",
    );
}

#[test]
fn drop_types_cannot_be_captured_by_function_values() {
    let error = analyze(&format!(
        "{COUNTER}export def capture :: i64\nfn capture =\n    let r = Counter {{ id: 1 }}\n    let read = \\_ -> r.id\n    read ()\n"
    ))
    .expect_err("Drop types are not Capture");
    assert_eq!(error.code, "E1005", "{}", error.message);
    assert!(
        error.message.starts_with("cannot capture ")
            && error.message.contains(" in a reusable function;"),
        "{}",
        error.message
    );
}

#[test]
fn drop_types_can_be_sent_to_tasks() {
    accepts(&format!(
        "{COUNTER}export def sent :: i64\nfn sent =\n    let r = Counter {{ id: 7 }}\n    Task.run (task {{ return r.id }})\n"
    ));
}

#[test]
fn specializes_drop_once_per_concrete_type() {
    let module = accepts(&format!(
        "{HANDLE}def make :: 'a -> Handle<'a> = \\value -> Handle {{ id: 1, value: value }}\nexport def handles :: i64\nfn handles =\n    let first = make 4\n    let second = make \"five\"\n    let third = Handle {{ id: 6, value: 6 }}\n    first.id + second.id + third.id\n"
    ));
    let types = module.types();
    let mut names: Vec<_> = module
        .user_drops
        .keys()
        .map(|ty| ty.display(&types))
        .collect();
    names.sort();
    assert_eq!(names, ["Main.Handle<i32>", "Main.Handle<string>"]);
    for function in module.user_drops.values() {
        assert!(module.functions[*function].name.contains(".drop"));
    }
}

#[test]
fn programs_without_drop_have_no_user_drops() {
    let module = accepts(
        "record Point { x: i64, label: string }\nunion Shape = Empty | Dot of Point\nunion Chain = End | Link of i64 * Chain\nexport def shapes :: i64\nfn shapes =\n    let shape = Dot (Point { x: 1, label: \"a\" })\n    let chain = Link (2, Link (3, End))\n    let total = Task.run (task { return 4 })\n    total + (match shape with | Dot point -> point.x | Empty -> 0) + (match chain with | Link (value, _) -> value | End -> 0)\n",
    );
    assert!(module.user_drops.is_empty());
    assert!(module.records.iter().all(|record| !record.user_drop));
    assert!(module.unions.iter().all(|union| !union.user_drop));
}

const MOVE_OUT: &str = "cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead";
const MAKE: &str = "def make :: i64 -> Resource = \\id -> Resource { id: id, name: \"r\" }\n";

#[test]
fn rejects_moving_fields_out_of_drop_types() {
    rejects(
        &format!("{RESOURCE}def name_of :: Resource -> string\nfn name_of value = value.name\n"),
        "E1012",
        MOVE_OUT,
    );
    rejects(
        &format!(
            "{RESOURCE}{MAKE}export def temporary :: i64\nfn temporary =\n    let name = (make 1).name\n    name.length\n"
        ),
        "E1012",
        MOVE_OUT,
    );
    rejects(
        "extern def drop_log :: i64 -> unit\nunion Slot = Empty | Full of string\ninstance Drop<Slot> { fn drop _value = drop_log 1 }\ndef text_of :: Slot -> string\nfn text_of slot =\n    match slot with\n    | Full text -> text\n    | Empty -> \"\"\n",
        "E1012",
        MOVE_OUT,
    );
}

#[test]
fn allows_copy_fields_and_borrows_of_drop_types() {
    accepts(&format!(
        "{RESOURCE}{MAKE}def take :: Resource -> i64 = \\value -> value.id\nexport def reads :: i64\nfn reads =\n    let r = make 1\n    let length = String.length (ref r.name)\n    let first = (make 2).id\n    let slot = Full 3\n    let payload = match slot with | Full value -> value | Empty -> 0\n    r.id + length + first + payload + take r\nunion Slot = Empty | Full of i64\ninstance Drop<Slot> {{ fn drop _value = drop_log 1 }}\n"
    ));
}

#[test]
fn rejects_record_update_of_drop_types() {
    rejects(
        &format!(
            "{RESOURCE}def rename :: Resource -> Resource\nfn rename value = {{ value with name = \"c\" }}\n"
        ),
        "E1012",
        "cannot update a value whose type implements Drop; construct a new value instead",
    );
}

#[test]
fn rejects_replacing_the_whole_value_inside_drop() {
    rejects(
        "record Resource { id: i64, name: string }\ninstance Drop<Resource> { fn drop value = deref value = Resource { id: 0, name: \"\" } }",
        "E1012",
        "cannot replace the whole value inside Drop.drop; read or borrow its fields instead",
    );
    // Record fields are immutable everywhere, so a field assignment keeps its existing error.
    rejects(
        "record Resource { id: i64, name: string }\ninstance Drop<Resource> { fn drop value = value.id = 0 }",
        "E1012",
        "assignment replaces a mutable binding; record fields, array elements, and list elements are immutable",
    );
    accepts(
        "record Resource { id: i64, name: string }\ndef reset :: ref mut Resource -> unit = \\value -> deref value = Resource { id: 0, name: \"\" }\nexport def outside :: i64\nfn outside =\n    let mut r = Resource { id: 1, name: \"a\" }\n    reset (ref mut r)\n    r.id\n",
    );
    // A callee given the exclusive reference could replace the value just the same.
    let reset = "extern def drop_log :: i64 -> unit\nrecord Resource { id: i64, name: string }\ndef reset :: ref mut Resource -> unit = \\value -> deref value = Resource { id: 0, name: \"\" }\ndef apply :: 'a -> ('a -> unit) -> unit = \\x f -> f x\ndef read :: ref Resource -> i64 = \\value -> value.id\n";
    for body in [
        "reset value",
        "reset (ref mut (deref value))",
        "apply value reset",
        "{ let other = value; drop_log other.id }",
    ] {
        rejects(
            &format!("{reset}instance Drop<Resource> {{ fn drop value = {body} }}"),
            "E1012",
            "cannot pass the value on as 'ref mut' inside Drop.drop; read it or borrow it with 'ref' instead",
        );
    }
    accepts(&format!(
        "{reset}instance Drop<Resource> {{ fn drop value = drop_log (read value + read (ref (deref value))) }}"
    ));
}

/// The body of the LLVM function whose definition line contains `name`.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    let start = ir
        .lines()
        .position(|line| line.starts_with("define ") && line.contains(name))
        .unwrap_or_else(|| panic!("no definition of {name}"));
    let offset: usize = ir.lines().take(start).map(|line| line.len() + 1).sum();
    let body = &ir[offset..];
    &body[..body.find("\n}\n").unwrap()]
}

fn user_drop_symbol(module: &CheckedModule, ty: &str) -> String {
    let function = module
        .user_drops
        .iter()
        .find_map(|(key, function)| (key.display(&module.types()) == ty).then_some(*function))
        .unwrap_or_else(|| panic!("no user drop for {ty}"));
    format!("@tz.fn.{}", module.functions[function].qualified_name())
}

#[test]
fn drop_glue_calls_user_drop_before_fields() {
    let module = accepts(&format!(
        "{RESOURCE}export def scopes :: i64\nfn scopes =\n    let first = Resource {{ id: 1, name: \"a\" + \"b\" }}\n    first.id\n"
    ));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert!(ir.contains("%tz.record.Main.Resource = type { i64, %tz.string, i8 }"));
    let body = function_body(&ir, "@tz.fn.Main.scopes(");
    let symbol = user_drop_symbol(&module, "Main.Resource");
    let call = body
        .find(&format!("call i8 {symbol}(ptr "))
        .expect("the drop glue calls the user drop");
    let free = body
        .rfind("call void @tz.free(")
        .expect("the name field is freed");
    assert!(call < free, "{body}");
    // Moved-out storage is zero, so a cleared live flag skips the call.
    assert!(body[..call].contains("icmp ne i8 "), "{body}");
}

#[test]
fn recursive_drop_calls_user_drop_before_enqueue() {
    let module = accepts(
        "extern def drop_log :: i64 -> unit\nunion Counted = Stop | More of Counted\ninstance Drop<Counted> { fn drop _value = drop_log 1 }\nexport def chain :: i64\nfn chain =\n    let counted = More (More Stop)\n    match counted with | More _ -> 1 | Stop -> 0\n",
    );
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let body = function_body(&ir, "@\"tz.drop.rec.Main.Counted\"(");
    let symbol = user_drop_symbol(&module, "Main.Counted");
    let call = body
        .find(&format!("call i8 {symbol}(ptr "))
        .expect("the node helper calls the user drop");
    let enqueue = body
        .find("call void @tz.rec.enqueue(")
        .expect("the helper queues the child node");
    assert!(call < enqueue, "{body}");
    // Every case owns a node, so the nullary first case is not null.
    let construct = function_body(&ir, "@tz.fn.Main.chain(");
    assert_eq!(
        construct.matches("call ptr @tz.alloc(").count(),
        3,
        "{construct}"
    );
}

fn fingerprint(source: &str) -> String {
    tsuzuri::formatter::ast_fingerprint(tsuzuri::parser::parse(source).unwrap())
}

#[test]
fn use_bindings_bind_drop_values_in_blocks_and_computations() {
    let module = accepts(&format!(
        "{RESOURCE}{MAKE}export def scoped :: i64\nfn scoped =\n    use first = make 1\n    use second: Resource = make 2\n    first.id + second.id\nexport def braced :: i64\nfn braced = {{ use first = make 3; use _guard = make 4; first.id }}\nexport def computed :: i64\nfn computed =\n    let result = Option {{\n        use _held = make 5\n        let! value = Option.Some 10\n        return value + 1\n    }}\n    Option.default_value 0 result\nexport def bound :: i64\nfn bound =\n    let result = Option {{\n        use! resource = Option.Some (make 6)\n        return resource.id\n    }}\n    Option.default_value 0 result\nexport def spawned :: i64\nfn spawned =\n    let work = task {{\n        use! inner = task {{ return make 7 }}\n        return inner.id\n    }}\n    Task.run work\n"
    ));
    assert_eq!(module.user_drops.len(), 1);
    assert!(fingerprint("use x = 1\nx").contains("using: true"));
    assert_ne!(fingerprint("use x = 1\nx"), fingerprint("let x = 1\nx"));
}

#[test]
fn use_is_a_contextual_keyword() {
    accepts(
        "def use :: i64 -> i64\nfn use value = value + 1\nexport def call :: i64\nfn call =\n    let count = use 2\n    use count\n",
    );
}

#[test]
fn use_bindings_require_drop_values() {
    rejects(
        "def f :: i64\nfn f =\n    use x = 1\n    x\n",
        "E1005",
        "'use' needs a value whose type implements Drop; i64 does not, so bind it with 'let'",
    );
    rejects(
        "def keep :: 'a -> unit\nfn keep value =\n    use _held = value\n    ()\nexport def call :: unit\nfn call = keep 1l\n",
        "E1005",
        "no instance for Drop<i64>",
    );
    rejects(
        &format!("{RESOURCE}{MAKE}def f :: i64\nfn f =\n    use mut x = make 1\n    x.id\n"),
        "E0002",
        "a use binding cannot be mutable; bind the value with 'let mut' instead",
    );
    rejects(
        &format!(
            "{RESOURCE}{MAKE}def f :: Option<i64>\nfn f = Option {{\n    use! a = Option.Some (make 1)\n    and! b = Option.Some 2\n    return b\n}}\n"
        ),
        "E0002",
        "'use!' cannot start an and! group",
    );
    // The continuation after let! is a function value, so it cannot capture a Drop value.
    rejects(
        &format!(
            "{RESOURCE}{MAKE}def f :: Option<i64>\nfn f = Option {{\n    use held = make 1\n    let! value = Option.Some 2\n    return value + held.id\n}}\n"
        ),
        "E1005",
        "cannot capture Main.Resource in a reusable function",
    );
}

#[test]
fn owned_drop_releases_values_early() {
    let module = accepts(&format!(
        "{RESOURCE}{MAKE}export def early :: i64\nfn early =\n    let first = make 1\n    Owned.drop first\n    let second = make 2\n    second.id\n"
    ));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let symbol = user_drop_symbol(&module, "Main.Resource");
    let body = function_body(&ir, "tz.builtin.Owned.drop.");
    assert!(body.contains(&format!("call i8 {symbol}(ptr ")), "{body}");
    rejects(
        &format!(
            "{RESOURCE}{MAKE}export def twice :: i64\nfn twice =\n    let first = make 1\n    Owned.drop first\n    first.id\n"
        ),
        "E1012",
        "use of moved or partially moved value 'first'",
    );
}

const OWNED: &str = "export def owned :: i64\nfn owned =\n    let held = make 7\n    let add = Owned.function (\\amount -> amount + held.id)\n";

#[test]
fn owned_functions_keep_drop_captures_across_calls() {
    let module = accepts(&format!(
        "{RESOURCE}{MAKE}{OWNED}    Owned.call (ref add) 1 + Owned.call (ref add) 2\n"
    ));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    let apply = function_body(&ir, "@tz.apply.$lambda.");
    let call = apply
        .find("call i64 @tz.fn.$lambda.")
        .expect("the body is called");
    let branch = apply
        .find("br i1 %borrow")
        .expect("borrowed calls skip the drop");
    let drop = apply
        .find("call void @tz.env.drop.$lambda.")
        .expect("consuming calls drop the environment");
    assert!(call < branch && branch < drop, "{apply}");
    assert!(!ir.contains("@tz.env.clone.$lambda."), "{ir}");
    let call = function_body(&ir, "tz.builtin.Owned.call.");
    assert!(call.contains(", i1 true)"), "{call}");
    // Plain function values can also be owned.
    accepts(
        "def double :: i64 -> i64\nfn double value = value * 2\nexport def twice :: i64\nfn twice =\n    let owned = Owned.function double\n    Owned.call (ref owned) 21\n",
    );
}

#[test]
fn owned_functions_are_unique_opaque_values() {
    let prefix = format!("{RESOURCE}{MAKE}{OWNED}");
    rejects(
        &format!("{prefix}    let copy = add\n    Owned.call (ref add) 1\n"),
        "E1012",
        "use of moved or partially moved value 'add'",
    );
    rejects(
        &format!("{prefix}    let read = \\x -> Owned.call (ref add) x\n    read 1\n"),
        "E1005",
        "cannot capture Owned.Function<i64, i64> in a reusable function",
    );
    rejects(
        &format!("{prefix}    let run = add.run\n    run 1\n"),
        "E1022",
        "the representation of 'Owned.Function' is opaque",
    );
}

#[test]
fn owned_function_lambdas_only_borrow_their_captures() {
    let prefix =
        format!("{RESOURCE}{MAKE}export def owned :: i64\nfn owned =\n    let held = make 7\n");
    rejects(
        &format!(
            "{prefix}    let consume = Owned.function (\\amount -> {{ Owned.drop held; amount }})\n    Owned.call (ref consume) 1\n"
        ),
        "E1012",
        "cannot move 'held' out of an owned function; borrow it with 'ref' instead",
    );
    // A handle has no drop glue but is not Copy, so each call would close it again.
    let handle = "extern type Counter\nextern def counter_open :: i64 -> Counter\nextern def counter_value :: ref Counter -> i64\nextern def counter_close :: Counter -> unit\nexport def owned :: i64\nfn owned =\n    let counter = counter_open 1\n";
    rejects(
        &format!(
            "{handle}    let close = Owned.function (\\x -> {{ counter_close counter; x }})\n    Owned.call (ref close) 1\n"
        ),
        "E1012",
        "cannot move 'counter' out of an owned function; borrow it with 'ref' instead",
    );
    accepts(&format!(
        "{handle}    let read = Owned.function (\\x -> x + counter_value (ref counter))\n    Owned.call (ref read) 1\n"
    ));
    rejects(
        &format!("{prefix}    let add = Owned.function (\\x y -> x + y + held.id)\n    0\n"),
        "E1006",
        "Owned.function takes a lambda with one parameter; take a tuple or return another function",
    );
    rejects(
        &format!(
            "{prefix}    let borrowed = ref held\n    let add = Owned.function (\\x -> x + borrowed.id)\n    Owned.call (ref add) 1\n"
        ),
        "E1013",
        "an owned function captures only owned values; ref Main.Resource contains a reference",
    );
    // Only a lambda written as the direct argument follows the owned rules.
    rejects(
        &format!("{prefix}    let add = (\\x -> x + held.id) |> Owned.function\n    0\n"),
        "E1005",
        "cannot capture Main.Resource in a reusable function",
    );
    accepts(&format!(
        "{prefix}    let shape = Option.Some 3\n    let base = 10\n    let scale = \\x -> x * base\n    let inner = Owned.function (\\x -> scale x + held.id + String.length (ref held.name))\n    let outer = Owned.function (\\x -> match ref shape with | Option.Some size -> Owned.call (ref inner) x + size | Option.None -> 0)\n    Owned.call (ref outer) 1\n"
    ));
}
