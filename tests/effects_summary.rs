//! A20 Phase 1: the effect summary of each function, inferred from the typed IR. Expected
//! summaries are written by hand from the language rules (D-53, appendix A and B of A20).

use tsuzuri::analyze;
use tsuzuri::check::CheckedModule;
use tsuzuri::check::effects::{self, Effect, EffectSet, Options, Scope, Unknown, Witness};

fn module(source: &str) -> CheckedModule {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message))
}

fn id(module: &CheckedModule, name: &str) -> usize {
    module
        .functions
        .iter()
        .position(|function| function.qualified_name() == name)
        .unwrap_or_else(|| panic!("no function {name}"))
}

fn all(module: &CheckedModule) -> effects::FunctionEffects {
    effects::summarize(module, Options::new(Scope::All, EffectSet::ALL)).unwrap()
}

const MAIN: &str = "def main :: unit -> i32 = \\() -> 0\n";

#[test]
fn summary_of_a_scalar_function_is_empty() {
    let module = module(&format!("def f :: i64 -> i64 = \\n -> n * 2 + 1\n{MAIN}"));
    let summaries = all(&module);
    let f = summaries.summary(id(&module, "Main.f"));
    assert_eq!(f.effects, EffectSet::EMPTY);
    assert_eq!(f.unknown, None);
    assert_eq!(f.calls_params, 0);
}

#[test]
fn io_debug_and_extern_propagate_through_calls() {
    let module = module(&format!(
        "extern \"abs\" def c_abs :: i32 -> i32\n\
         def shout :: unit -> IO<unit> = \\() -> IO.write_line 1\n\
         def trace :: i64 -> unit = \\n -> Debug.print (ref n)\n\
         def native :: i32 -> i32 = \\n -> c_abs n\n\
         def outer :: unit -> i32 = \\() ->\n    let _action = shout ()\n    trace 1\n    native 2i32\n{MAIN}"
    ));
    let summaries = all(&module);
    let outer = summaries.summary(id(&module, "Main.outer")).effects;
    for effect in [Effect::Io, Effect::Debug, Effect::Extern] {
        assert!(outer.contains(effect), "{effect:?} in {:?}", outer.names());
    }
    assert!(!outer.contains(Effect::Net));
    let shout = summaries.summary(id(&module, "Main.shout")).effects;
    assert!(shout.contains(Effect::Io));
    assert!(!shout.contains(Effect::Extern));
}

#[test]
fn mutual_recursion_shares_one_summary() {
    let module = module(&format!(
        "def rec even :: i64 -> bool = \\n -> if n == 0 then true else odd (n - 1)\n\
         and odd :: i64 -> bool = \\n -> if n == 0 then false else {{ Debug.print (ref n); even (n - 1) }}\n{MAIN}"
    ));
    let summaries = all(&module);
    let even = summaries.summary(id(&module, "Main.even"));
    let odd = summaries.summary(id(&module, "Main.odd"));
    assert_eq!(even.effects, odd.effects);
    assert!(even.effects.contains(Effect::Debug));
    assert!(even.effects.contains(Effect::Diverge));
}

#[test]
fn closures_passed_to_std_functions_are_edges() {
    let module = module(&format!(
        "def total :: ref [i64] -> i64 = \\xs -> Array.fold (\\sum x -> {{ Debug.print (ref x); sum + x }}) 0 xs\n{MAIN}"
    ));
    let summaries = all(&module);
    let total = summaries.summary(id(&module, "Main.total"));
    assert!(
        total.effects.contains(Effect::Debug),
        "{:?}",
        total.effects.names()
    );
    assert_eq!(total.unknown, None);
}

#[test]
fn open_function_that_calls_its_parameter_records_it() {
    let module = module(&format!(
        "record Box {{ run: i64 -> i64 }}\n\
         def apply :: (i64 -> i64) -> i64 -> i64 = \\f x -> f x\n\
         def through :: ref Box -> i64 -> i64 = \\b x -> b.run x\n{MAIN}"
    ));
    let summaries = all(&module);
    let apply = summaries.summary(id(&module, "Main.apply"));
    assert_eq!(apply.unknown, None);
    assert_eq!(apply.calls_params, 1);
    let through = summaries.summary(id(&module, "Main.through"));
    assert_eq!(through.unknown, Some(Unknown::ValueCall));
}

#[test]
fn closed_function_with_local_lambdas_is_known() {
    let module = module(&format!(
        "def twice :: i64 -> i64 = \\n ->\n    let f = \\x -> x * 2\n    f (f n)\n{MAIN}"
    ));
    let summaries = all(&module);
    let twice = summaries.summary(id(&module, "Main.twice"));
    assert_eq!(twice.unknown, None);
    assert!(!twice.effects.contains(Effect::Alloc));
}

#[test]
fn witness_chain_ends_in_a_direct_source() {
    let module = module(&format!(
        "def leaf :: i64 -> unit = \\n -> Debug.print (ref n)\n\
         def middle :: i64 -> unit = \\n -> leaf (n + 1)\n\
         def top :: i64 -> unit = \\n -> middle (n * 2)\n{MAIN}"
    ));
    let summaries = all(&module);
    let top = id(&module, "Main.top");
    let chain = summaries.chain(top, Effect::Debug);
    assert!(chain.len() >= 3, "{chain:?}");
    assert_eq!(chain[0].0, top);
    assert_eq!(chain[1].0, id(&module, "Main.middle"));
    assert_eq!(chain[2].0, id(&module, "Main.leaf"));
    let (last, _) = *chain.last().unwrap();
    assert!(matches!(
        summaries.witness(last, Effect::Debug),
        Some(Witness::Direct { .. })
    ));
    assert!(matches!(
        summaries.witness(top, Effect::Debug),
        Some(Witness::Via { .. })
    ));
}

#[test]
fn summaries_are_deterministic() {
    let source = format!(
        "def rec walk :: i64 -> i64 = \\n -> if n <= 0 then 0 else walk (n - 1) + Array.sum [n, n]\n\
         def text :: i64 -> string = \\n -> $\"{{n}}\"\n{MAIN}"
    );
    let first = module(&source);
    let second = module(&source);
    let (a, b) = (all(&first), all(&second));
    for id in 0..first.functions.len() {
        assert_eq!(a.summary(id), b.summary(id));
        assert_eq!(a.chain(id, Effect::Alloc), b.chain(id, Effect::Alloc));
    }
    assert_eq!(a.rows(&first), b.rows(&second));
}

#[test]
fn no_attribute_means_no_summary() {
    let module = module(&format!("def f :: i64 -> i64 = \\n -> n + 1\n{MAIN}"));
    assert!(!effects::needed(&module));
    assert!(module.effects.is_none());
}

#[test]
fn an_attribute_keeps_the_summary_on_the_module() {
    let module = module(&format!(
        "@no_copy\ndef f :: i64 -> i64 = \\n -> n + 1\n{MAIN}"
    ));
    assert!(effects::needed(&module));
    let summaries = module.effects.as_ref().expect("summary");
    assert!(summaries.computed.contains(Effect::Copy));
    assert!(!summaries.computed.contains(Effect::Alloc));
}
