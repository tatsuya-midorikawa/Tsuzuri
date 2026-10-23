use tsuzuri::check::{CheckedModule, Type};
use tsuzuri::syntax::{SourceKind, Visibility};
use tsuzuri::{analyze, formatter, llvm, parser};

fn accepted(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"))
}

fn rejected(source: &str, text: &str) {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, "E1029", "{source}\n{error:?}");
    assert!(error.message.contains(text), "{source}\n{error:?}");
}

fn ir(source: &str) -> String {
    let module = accepted(source);
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap());
    ir
}

#[test]
fn parses_pure_def_as_a_modifier_and_keeps_pure_as_an_identifier() {
    let program = parser::parse(
        "pure def a :: i64 -> i64 = \\x -> x\n\
         export pure def b :: i64 -> i64 = \\x -> x\n\
         private pure def c :: i64 -> i64 = \\x -> x\n\
         pure def rec even :: i64 -> bool = \\n -> n == 0 || odd (n - 1)\n\
         and odd :: i64 -> bool = \\n -> n != 0 && even (n - 1)\n\
         def pure :: i64 -> i64 = \\pure -> pure + 1\n",
    )
    .unwrap();
    assert!(program.functions[..5].iter().all(|function| function.pure));
    assert!(program.functions[1].exported);
    assert_eq!(program.functions[2].visibility, Visibility::Private);
    assert!(!program.functions[5].pure);
    accepted(
        "record P { pure: i64 }\ndef pure :: i64 -> i64 = \\pure -> { let p = P { pure: pure }; p.pure }\n",
    );
}

#[test]
fn rejects_misplaced_pure() {
    for source in [
        "pure fn f x = x",
        "pure export def f :: i64 -> i64 = \\x -> x",
        "const pure def f :: i64 -> i64 = \\x -> x",
        "pure const def f :: i64 -> i64 = \\x -> x",
    ] {
        assert_eq!(parser::parse(source).unwrap_err().code, "E0002", "{source}");
    }
}

#[test]
fn pure_function_values_keep_effects_and_call_stage_boundaries() {
    accepted(
        "pure def apply :: pure (i64 -> i64) -> i64 -> i64 = \\f value -> f value\n\
         pure def increment :: i64 -> i64 = \\value -> value + 1\n\
         def ordinary :: i64 -> i64 = \\value -> value + 2\n\
         export def answer :: i64 = apply increment 20 + apply ordinary 19 + apply (\\value -> value + 3) 18\n",
    );
    rejected(
        "pure def apply :: pure (i64 -> i64) -> i64 -> i64 = \\f value -> f value\n\
         def bad :: i64 = apply (\\value -> { Debug.print (ref value); value }) 1\n",
        "debug output",
    );
    rejected(
        "pure def apply :: pure (i64 -> i64) -> i64 -> i64 = \\f value -> f value\n\
         def bad :: (i64 -> i64) -> i64 = \\f -> apply f 1\n",
        "unknown ordinary callback",
    );
    accepted(
        "pure def apply :: pure (i64 -> i64) -> i64 -> i64 = \\f value -> f value\n\
         def weaken :: ref (i64 -> i64) -> i64 = \\f -> (*f) 20\n\
         pure def increment :: i64 -> i64 = \\value -> value + 1\n\
         export def answer :: i64 = { let f = increment; weaken (ref f) }\n",
    );
    assert!(
        analyze(
            "type F = i64 -> i64\n\
         def factory :: unit -> F = \\() -> \\value -> { Debug.print (ref value); value }\n\
         pure def apply :: pure (unit -> i64 -> i64) -> i64 = \\f -> f () 1\n\
         def bad :: i64 = apply factory\n",
        )
        .is_err()
    );
    accepted(
        "type F = i64 -> i64\n\
         pure def factory :: unit -> F = \\() -> \\value -> { Debug.print (ref value); value }\n",
    );
    rejected(
        "type F = i64 -> i64\n\
         pure def factory :: unit -> F = \\() -> \\value -> { Debug.print (ref value); value }\n\
         pure def bad :: unit -> i64 = \\() -> factory () 1\n",
        "pure",
    );
    rejected(
        "type F = i64 -> i64\n\
         def invoke :: ref (unit -> i64 -> i64) -> i64 = \\f -> (*f) () 1\n\
         pure def bad :: pure (unit -> F) -> i64 = \\factory -> invoke factory\n",
        "pure",
    );
    rejected(
        "type F = i64 -> i64\n\
         pure def invoke :: pure (unit -> F) -> i64 = \\f -> f () 1\n",
        "pure",
    );
    rejected(
        "type F = i64 -> i64\n\
         pure def factory :: unit -> F = \\() -> \\value -> { Debug.print (ref value); value }\n\
         def invoke :: (unit -> i64 -> i64) -> i64 = \\f -> f () 1\n\
         pure def bad :: unit -> i64 = \\() -> invoke factory\n",
        "pure",
    );
}

#[test]
fn weakening_a_pure_factory_keeps_the_closure_representation() {
    let emitted = ir("type F = i64 -> i64\n\
         pure def factory :: i64 -> F = \\offset -> { \\value -> offset + value }\n\
         def invoke :: own (i64 -> i64 -> i64) -> i64 -> i64 = \\f value -> f value (value + 1)\n\
         def borrowed :: ref (i64 -> i64 -> i64) -> i64 -> i64 = \\f value -> (*f) value (value + 1)\n\
         export def answer :: i64 = invoke factory 20 + borrowed factory 20\n");
    assert!(!emitted.contains("call void @tz_soft_cast"));
}

#[test]
fn class_and_instance_purity_are_contracts_even_when_unused() {
    accepted(
        "record R { value: i64 }\n\
         class Measure<'a> { pure def size :: 'a -> i64 }\n\
         instance Measure<R> { fn size item = item.value }\n\
         pure def answer :: R -> i64 = \\item -> Measure.size item\n",
    );
    rejected(
        "record R { value: i64 }\n\
         class Measure<'a> { pure def size :: 'a -> i64 }\n\
         instance Measure<R> { fn size item = { Debug.print (ref item.value); item.value } }\n",
        "debug output",
    );
    accepted(
        "record R { value: i64 }\n\
         instance Add<R> { pure fn add left right = R { value: left.value + right.value } }\n\
         pure def answer :: i64 = ((R { value: 20 }) + (R { value: 22 })).value\n",
    );
    rejected(
        "record R { value: i64 }\n\
         instance Add<R> { pure fn add left right = { Debug.print (ref left.value); left } }\n",
        "debug output",
    );
}

#[test]
fn borrowed_drop_data_is_readable_without_claiming_its_destructor_is_pure() {
    accepted(
        "record Ticket { id: i64 }\n\
         instance Drop<Ticket> { fn drop value = Debug.print (ref value.id) }\n\
         pure def read :: Ticket -> i64 = \\ticket -> ticket.id\n\
         pure def count :: [Ticket] -> i64 = \\tickets -> tickets.length\n",
    );
    rejected(
        "record Ticket { id: i64 }\n\
         instance Drop<Ticket> { fn drop value = Debug.print (ref value.id) }\n\
         pure def bad :: unit -> i64 = \\() -> (Ticket { id: 1 }).id\n",
        "user Drop",
    );
}

#[test]
fn io_construction_is_deferred_but_execution_and_eager_captures_are_not() {
    accepted(
        "pure def action :: unit -> IO<unit> = \\() -> IO.write_line \"deferred\"\n\
         pure def read :: unit -> IO<Maybe<string>> = \\() -> IO.read_line ()\n\
         pure def composed :: unit -> IO<unit> = \\() -> IO.map (\\value -> { Debug.print (ref value); () }) (IO.pure 1)\n",
    );
    rejected(
        "pure def bad :: unit -> IO<unit> = \\() -> { Debug.print (ref 1); IO.pure () }\n",
        "debug output",
    );
    assert!(analyze(
        "pure def bad :: unit -> unit = \\() -> { let action = IO.write_line \"not deferred\"; do! action; () }\n",
    ).is_err());
    rejected(
        "pure def bad :: unit -> i64 = \\() -> { let callback = \\n -> { Debug.print (ref n); n }; let action = IO.map callback (IO.pure 1); callback 2 }\n",
        "debug output",
    );
}

#[test]
fn cpu_specialized_pure_functions_keep_the_effect_contract() {
    accepted("@cpu [\"avx2\"] pure def increment :: i64 -> i64 = \\value -> value + 1\n");
    rejected(
        "@cpu [\"avx2\"] pure def bad :: i64 -> i64 = \\value -> { Debug.print (ref value); value }\n",
        "debug output",
    );
}

#[test]
fn formats_pure_and_own_and_round_trips_documentation() {
    let source = "/// length\npure def f::own string->i64=\\s->s.length\n";
    let formatted = formatter::format_source("Main.tz", source, SourceKind::Code)
        .unwrap()
        .formatted;
    assert!(formatted.contains("pure def f :: own string -> i64"));
    assert_eq!(
        formatted,
        formatter::format_source("Main.tz", &formatted, SourceKind::Code)
            .unwrap()
            .formatted
    );
    assert_eq!(
        formatter::ast_fingerprint(parser::parse(source).unwrap()),
        formatter::ast_fingerprint(parser::parse(&formatted).unwrap())
    );
}

#[test]
fn pure_def_parameters_are_shared_references_but_scalars_remain_values() {
    let module = accepted(
        "record Point { x: i64, y: i64 }\n\
         record Named { text: string }\n\
         pure def text :: string -> i64 = \\s -> s.length\n\
         pure def array :: [i64] -> i64 = \\xs -> xs.length\n\
         pure def maybe :: Maybe<[i64]> -> i64 = \\x -> 0\n\
         pure def named :: Named -> i64 = \\x -> x.text.length\n\
         pure def point :: Point -> i64 = \\x -> x.x\n\
         pure def scalar :: i64 -> f64 -> bool -> unit -> i64 = \\n f b () -> n\n\
         pure def explicit :: ref string -> i64 = \\s -> s.length\n",
    );
    for name in ["text", "array", "maybe", "named", "explicit"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap();
        assert!(
            matches!(function.signature.parameters[0], Type::Reference(_, false)),
            "{name}"
        );
    }
    for name in ["point", "scalar"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap();
        assert!(
            function
                .signature
                .parameters
                .iter()
                .all(|ty| !matches!(ty, Type::Reference(..)))
        );
    }
}

#[test]
fn pure_def_own_parameters_are_values_and_require_plain_data() {
    accepted(
        "record Point { x: i64 }\n\
         pure def f :: own string -> own [string] -> own Vec<i64> -> own Maybe<[i64]> -> own Point -> i64 = \\s a v m p -> s.length + a.length + p.x\n",
    );
    for (ty, declaration, text) in [
        ("ref string", "", "a reference"),
        ("Rc<string>", "", "an 'Rc' or 'Arc'"),
        ("Arc<string>", "", "an 'Rc' or 'Arc'"),
        ("Maybe<Rc<string>>", "", "an 'Rc' or 'Arc'"),
        (
            "Box",
            "record Box { item: Rc<string> }\n",
            "an 'Rc' or 'Arc'",
        ),
        (
            "Ticket",
            "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = () }\n",
            "a value with a user Drop",
        ),
        (
            "Counter",
            "record Counter {r} { value: ref mut {r} i64 }\n",
            "a reference",
        ),
        (
            "View",
            "record View {r} { value: ref {r} string }\n",
            "a reference",
        ),
    ] {
        rejected(
            &format!("{declaration}pure def f :: own {ty} -> i64 = \\x -> 0\n"),
            text,
        );
    }
    accepted("pure def f :: ref Rc<string> -> i64 = \\x -> 0\n");
    let module = accepted(
        "pure def f :: own string -> own [i64] -> string -> i64 = \\s xs text -> s.length + xs.length + text.length\n",
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .unwrap();
    assert!(matches!(function.signature.parameters[0], Type::String));
    assert!(matches!(function.signature.parameters[1], Type::Array(_)));
    assert!(matches!(
        function.signature.parameters[2],
        Type::Reference(..)
    ));
}

#[test]
fn generic_pure_specializations_preserve_contracts_and_plain_data_obligations() {
    let module = accepted(
        "pure def length :: ['a] -> i64 = \\xs -> xs.length\n\
         pure def identity :: own 'a -> 'a = \\value -> value\n\
         pure def plus :: Add<'a> => own 'a -> own 'a -> 'a = \\left right -> left + right\n\
         export def answer :: i64 = length [\"a\", \"bc\"] + (identity \"hello\").length + plus 20 15\n",
    );
    assert!(module.functions.iter().any(|function| {
        function.origin.pure
            && function.name.starts_with("identity.$mono.")
            && function.signature.parameters == [Type::String]
    }));
    rejected(
        "pure def inspect :: own 'a -> i64 = \\value -> 0\n\
         def bad :: unit -> i64 = \\() -> inspect (Rc.new 1)\n",
        "plain data",
    );
    rejected(
        "pure def unused :: 'a -> i64 = \\value -> { let n = 1; Debug.print (ref n); n }\n",
        "debug output",
    );
    rejected(
        "record R { value: i64 }\n\
         instance Add<R> { fn add left right = { Debug.print (ref left.value); left } }\n\
         pure def plus :: Add<'a> => own 'a -> own 'a -> 'a = \\left right -> left + right\n\
         def bad :: unit -> R = \\() -> plus (R { value: 1 }) (R { value: 2 })\n",
        "debug output",
    );
}

#[test]
fn pure_def_own_keeps_existing_ownership_rules() {
    let error = analyze(
        "pure def eat :: own string -> i64 = \\s -> s.length\n\
         def bad :: unit -> i64 = \\() -> { let s = \"x\"; eat s + s.length }\n",
    )
    .unwrap_err();
    assert_eq!(error.code, "E1012");
    accepted(
        "pure def bump :: own [i64] -> [i64] = \\xs -> Array.update_at 0 99 xs\n\
         export def run :: i64 = { let xs = [1, 2]; let ys = bump xs; xs[0] * 100 + ys[0] }\n",
    );
}

#[test]
fn inferred_pure_reads_have_ownership_guidance_but_explicit_refs_do_not() {
    for source in [
        "pure def f :: string -> string = \\text -> text\n",
        "record Named { text: string }\npure def f :: Named -> string = \\named -> named.text\n",
    ] {
        let error = analyze(source).unwrap_err();
        assert!(
            error.message.contains("read-only by default (pure def)"),
            "{error:?}"
        );
        assert!(error.message.contains("write 'own "));
        let explicit = source
            .replace(":: string", ":: ref string")
            .replace(":: Named", ":: ref Named");
        assert!(
            !analyze(&explicit)
                .unwrap_err()
                .message
                .contains("read-only by default")
        );
    }
}

#[test]
fn pure_def_signature_rules_are_enforced_even_when_unused() {
    for (source, text) in [
        ("pure def f :: (i64 -> i64) -> i64 = \\f -> 0", "function"),
        (
            "record Callback { f: i64 -> i64 }\npure def f :: Callback -> i64 = \\x -> 0",
            "function",
        ),
        (
            "pure def f :: ref mut i64 -> i64 = \\x -> deref x",
            "only shared",
        ),
        ("pure def f :: i64 -> i64 = \\mut x -> x", "mut"),
        ("pure def f :: own i64 -> i64 = \\mut x -> x", "mut"),
    ] {
        rejected(source, text);
    }
}

#[test]
fn pure_def_ir_matches_the_explicit_ref_version_without_new_attributes() {
    let source = "pure def f :: string -> [i64] -> i64 = \\s xs -> s.length + Array.sum xs\n\
                  export def run :: i64 -> i64 = \\n -> f (String.repeat n \"x\") [1, 2, 3]\n";
    let pure = ir(source);
    let normal = ir(&source.replace(
        "pure def f :: string -> [i64]",
        "def f :: ref string -> ref [i64]",
    ));
    assert_eq!(pure, normal);
    assert_eq!(
        ir("pure def f :: own string -> i64 = \\s -> s.length\nexport def run :: i64 = f \"x\"\n"),
        ir("def f :: string -> i64 = \\s -> s.length\nexport def run :: i64 = f \"x\"\n")
    );
}

#[test]
fn rejects_external_io_debug_extern_and_shared_state() {
    for (body, text) in [
        ("do! IO.write_line \"hi\"\nn", "outside world"),
        ("Debug.print (ref n)\nn", "debug output"),
        ("host_tick n", "extern function"),
        (
            "let a = Atomic.create 0\nAtomic.store (ref a) n\nAtomic.load (ref a)",
            "shared mutable state",
        ),
        (
            "let r = Rc.new \"abc\"\n(Rc.get (ref r)).length + n",
            "shared mutable state",
        ),
    ] {
        let body = body
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        rejected(
            &format!(
                "extern def host_tick :: i64 -> i64\npure def f :: i64 -> i64 = \\n ->\n{body}"
            ),
            text,
        );
    }
}

#[test]
fn permits_closed_tasks_and_parallelism_but_checks_every_callback() {
    accepted(
        "pure def parallel :: [i64] -> [i64] = \\xs -> Parallel.map (\\x -> x + 1) xs\n\
         pure def run :: i64 -> i64 = \\n -> { let t = task { n + 1 }; Task.run t }\n",
    );
    for source in [
        "pure def f :: i64 -> i64 = \\n -> { let t = task { Debug.print (ref n); n }; Task.run t }",
        "pure def f :: [i64] -> [i64] = \\xs -> Parallel.map (\\x -> { Debug.print (ref x); x }) xs",
    ] {
        rejected(source, "debug output");
    }
}

#[test]
fn rejects_user_drop_even_inside_owned_aggregates() {
    let prefix = "record Ticket { id: i64 }\ninstance Drop<Ticket> { fn drop value = do! IO.write_line \"drop\" }\n";
    for body in [
        "let t = Ticket { id: n }\nt.id",
        "let xs = [Ticket { id: n }]\nxs.length",
        "let m: Maybe<Ticket> = Maybe.Some (Ticket { id: n })\nn",
    ] {
        let body = body
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        rejected(
            &format!("{prefix}pure def f :: i64 -> i64 = \\n ->\n{body}"),
            "user Drop",
        );
    }
}

#[test]
fn follows_indirect_callbacks_recursive_cycles_and_unused_pure_roots() {
    for source in [
        "def effect :: i64 -> i64 = \\n -> { Debug.print (ref n); n }\npure def f :: [i64] -> [i64] = \\xs -> Array.map effect xs",
        "def effect :: i64 -> i64 = \\n -> { Debug.print (ref n); n }\npure def f :: i64 -> i64 = \\n -> { let g = effect; g n }",
        "def rec a :: i64 -> i64 = \\n -> if n == 0 then b n else a (n - 1)\nand b :: i64 -> i64 = \\n -> { Debug.print (ref n); a n }\npure def f :: i64 -> i64 = \\n -> a n",
        "pure def f :: i64 -> i64 = \\n -> { let g = \\x -> { Debug.print (ref x); x }; g n }",
    ] {
        rejected(source, "debug output");
    }
    accepted(
        "pure def rec a :: i64 -> bool = \\n -> n == 0 || b (n - 1)\nand b :: i64 -> bool = \\n -> n != 0 && a (n - 1)\n",
    );
}

#[test]
fn permits_local_mutation_allocation_std_closures_and_seeded_randomness() {
    accepted(
        "record Point { x: i64, y: i64 } deriving (Eq, Ord)\n\
         pure def sort :: [Point] -> i64 = \\xs -> (Array.sort xs).length\n\
         pure def squares :: i64 -> [i64] = \\n -> Array.init n (\\i -> i * i)\n\
         pure def local :: [i64] -> i64 = \\xs -> { let mut ys = Array.copy xs; ys = Array.update_at 0 9 ys; ys[0] }\n\
         pure def random :: i64 -> i64 = \\seed -> { let generator = Random.pcg (seed as i64u) 54i64u; match Random.pcg_next_u32 generator with | (value, _) -> value as i64 }\n",
    );
}

#[test]
fn pure_calls_with_traps_are_not_discarded_and_results_are_not_shared() {
    let source = "pure def f :: [i64] -> i64 -> i64 = \\xs i -> xs[i]\n\
                  export def run :: i64 -> i64 = \\i -> { let unused = f [1, 2] i; 0 }\n";
    let normal = source.replace("pure def f :: [i64]", "def f :: ref [i64]");
    assert_eq!(ir(source), ir(&normal));
    assert!(ir(source).contains("@llvm.trap"), "bounds trap must remain");
    let source = "pure def f :: i64 -> [i64] = \\n -> Array.init n (\\i -> i)\n\
                  export def run :: i64 -> i64 = \\n -> { let xs = f n; let ys = f n; let zs = Array.update_at 0 99 xs; ys[0] + zs[0] }\n";
    assert_eq!(ir(source), ir(&source.replace("pure def", "def")));
}

#[test]
fn pure_defs_reach_no_effectful_runtime_symbol() {
    use std::collections::{BTreeMap, BTreeSet};
    let module = accepted(include_str!("fixtures/pure_functions/Main.tz"));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let functions = ir
            .split("\n\n")
            .filter(|block| block.starts_with("define "))
            .map(|block| {
                let name = block
                    .lines()
                    .next()
                    .unwrap()
                    .split('@')
                    .nth(1)
                    .unwrap()
                    .split('(')
                    .next()
                    .unwrap();
                (name, block)
            })
            .collect::<BTreeMap<_, _>>();
        let mut pending = module
            .functions
            .iter()
            .filter(|function| function.origin.pure)
            .map(|function| format!("tz.fn.{}", function.qualified_name()))
            .collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            let body = functions
                .get(name.as_str())
                .unwrap_or_else(|| panic!("missing {name}"));
            for forbidden in [
                "@tsuzuri_io_",
                "@tsuzuri_os_",
                "@tsuzuri_net_",
                "@tsuzuri_async_",
                "@tsuzuri_host_",
                "@tsuzuri_debug_",
                "@tsuzuri_bench_",
                "@tsuzuri_gpu_",
                "@tz.async",
                "@tz.gpu",
                "@tz.mutex",
                "@tz.debug",
                "@tz.task",
                "atomicrmw",
                "cmpxchg",
                "fence ",
            ] {
                assert!(
                    !body.contains(forbidden),
                    "{name} reaches {forbidden}\n{body}"
                );
            }
            pending.extend(body.split('@').skip(1).filter_map(|reference| {
                let name = reference
                    .chars()
                    .take_while(|character| {
                        character.is_ascii_alphanumeric() || ".$_".contains(*character)
                    })
                    .collect::<String>();
                name.starts_with("tz.fn.").then_some(name)
            }));
        }
    }
}
