//! The value that a function reads, transforms or updates (its subject: a collection, a text, an option, a matrix, a
//! writer) is its LAST argument, so that a pipeline (`xs |> Array.f a`) and a composition (`Array.f a >> Array.g b`)
//! pass it on. AGENTS.md ("Standard library API conventions") states the rule and its exemptions.
//!
//! The rule is checked on the `def` lines of every `std/` module, on the compiler builtins of the collection and
//! text modules, and on the pipeline and composition forms. A new `std/` file must be classified in `MODULES`: as a
//! module whose functions take the subject last, a module with no function of two parameters, or an exemption with
//! its reason.

use std::collections::HashMap;

use tsuzuri::analyze;
use tsuzuri::check::{Builtin, BuiltinType, Type};
use tsuzuri::stdlib::SOURCES;

/// How the functions of a std module are checked.
enum Kind {
    /// A function that takes a value of one of the `heads` types takes one as its last parameter.
    Subject {
        /// A type name (`Vec` matches `Vec<'a>`, not `VecDeque`), `[]` for an array, `[|` for a list.
        heads: &'static [&'static str],
        /// A function that takes several values of the subject's type (a needle and a text) cannot be told apart
        /// by its types: the last parameter of its definition must have one of these names instead.
        names: &'static [&'static str],
        /// Functions that are a protocol or an entry point, not API, with the reason.
        except: &'static [(&'static str, &'static str)],
    },
    /// No function has more than one parameter, so there is no order to get wrong. A function of two parameters
    /// fails the test: classify the module again.
    SingleArgument,
    /// Not checked, for the reason given.
    Exempt(&'static str),
}

use Kind::{Exempt, SingleArgument, Subject};

const fn subject(heads: &'static [&'static str]) -> Kind {
    Subject {
        heads,
        names: &[],
        except: &[],
    }
}

const fn text(heads: &'static [&'static str], names: &'static [&'static str]) -> Kind {
    Subject {
        heads,
        names,
        except: &[],
    }
}

const HANDLE: &str =
    "a resource: its handle or path comes first, like the receiver of a method of .NET";
const BUILTIN_HANDLE: &str =
    "a handle that the compiler implements: it comes first, like the receiver of a method of .NET";
const DERIVED: &str = "the protocol of the code that `derive` generates";
const HARNESS: &str = "a test or benchmark entry point: the generator or setup comes before the body that the caller writes last";

/// Every file of `std/`, by module name. See AGENTS.md ("Standard library API conventions") for the rule and for
/// when a module may be exempt.
const MODULES: &[(&str, Kind)] = &[
    // `since` reads a Stats subject; snapshot and measure have no Stats input.
    ("Alloc", subject(&["Stats"])),
    ("Arena", subject(&["Arena"])),
    ("Array", subject(&["[]"])),
    ("Async", SingleArgument),
    ("Atomic", Exempt(BUILTIN_HANDLE)),
    ("Bench", Exempt(HARNESS)),
    ("BigInt", subject(&["BigInt"])),
    ("Cbor", SingleArgument),
    ("Channel", Exempt(BUILTIN_HANDLE)),
    ("Char", SingleArgument),
    ("Debug", SingleArgument),
    ("Dir", SingleArgument),
    ("Env", SingleArgument),
    ("Exception", SingleArgument),
    ("File", Exempt(HANDLE)),
    ("Format", subject(&["string"])),
    (
        "Gen",
        Subject {
            heads: &["Gen"],
            names: &[],
            except: &[
                ("check", HARNESS),
                ("check_where", HARNESS),
                ("for_all", HARNESS),
                ("for_all_cases", HARNESS),
                ("for_all_where", HARNESS),
            ],
        },
    ),
    ("Gpu", subject(&["Buffer"])),
    ("HashMap", subject(&["HashMap"])),
    ("HashSet", subject(&["HashSet"])),
    ("IO", subject(&["IO"])),
    (
        "Json",
        Subject {
            heads: &["Value", "Writer", "Reader", "Numeral", "Lexeme"],
            names: &[],
            except: &[("decode_field", DERIVED), ("case_index", DERIVED)],
        },
    ),
    ("List", subject(&["[|"])),
    ("Map", subject(&["Map"])),
    ("Math", SingleArgument),
    ("Matrix", subject(&["Matrix"])),
    ("MatrixView", subject(&["MatrixView", "Mut"])),
    ("Maybe", subject(&["Maybe"])),
    // A class and the records of its instances: no `def` line.
    ("Monoid", SingleArgument),
    ("Monoids", SingleArgument),
    ("Mutex", Exempt(BUILTIN_HANDLE)),
    ("Net", Exempt(HANDLE)),
    ("Os", subject(&["[ubyte]"])),
    ("Owned", Exempt(BUILTIN_HANDLE)),
    ("Parallel", SingleArgument),
    ("Path", subject(&["string"])),
    ("Process", Exempt(HANDLE)),
    ("Random", subject(&["Pcg"])),
    ("Regex", text(&["string", "utf8string"], &["text"])),
    ("Result", subject(&["Result"])),
    ("Seq", subject(&["Seq"])),
    ("Set", subject(&["Set"])),
    ("String", text(&["string", "[string]"], &["text", "parts"])),
    ("Tensor", subject(&["Tensor", "View"])),
    ("Test", Exempt(HARNESS)),
    ("Time", SingleArgument),
    ("Unicode", subject(&["string", "utf8string"])),
    ("Utf8Char", SingleArgument),
    (
        "Utf8String",
        text(&["utf8string", "[utf8string]"], &["text", "parts"]),
    ),
    ("Vec", subject(&["Vec"])),
];

/// Pure functions of a module that is exempt as a whole, or whose subject is a type variable: their parameter types.
const PINNED: &[(&str, &str, &[&str])] = &[
    ("Net", "parse_address", &["ref string"]),
    ("Net", "parse_ip", &["i64", "ref string"]),
    ("Json", "serialize_pretty", &["i64", "ref 'a"]),
];

/// The parameter types of the signature in `def name :: [Constraint =>] t1 -> t2 -> result`: every type before the
/// last arrow that is not inside a bracket (a function type in a parameter keeps its own arrows).
fn parameters(signature: &str) -> Vec<String> {
    let ty = signature.split_once("=>").map_or(signature, |(_, ty)| ty);
    let characters: Vec<char> = ty.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '-' && characters.get(index + 1) == Some(&'>') {
            if depth == 0 {
                parts.push(current.trim().to_owned());
                current.clear();
            } else {
                current.push_str("->");
            }
            index += 2;
            continue;
        }
        match character {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => depth -= 1,
            _ => {}
        }
        current.push(character);
        index += 1;
    }
    parts.push(current.trim().to_owned());
    parts.pop();
    parts
}

/// A public function of a module: the parameter types of its `def` line and the parameter names of its `fn` line.
struct Function {
    name: String,
    types: Vec<String>,
    names: Vec<String>,
}

/// `fn [rec|and] name a b =` as its name and its parameter names.
fn header(line: &str) -> Option<(&str, Vec<&str>)> {
    let line = line.strip_prefix("fn ")?;
    let line = line
        .strip_prefix("rec ")
        .or_else(|| line.strip_prefix("and "))
        .unwrap_or(line);
    let mut words = line.split_whitespace();
    let name = words.next()?;
    Some((name, words.take_while(|word| *word != "=").collect()))
}

/// The public functions of a module, in the order of their `def` lines.
fn functions(source: &str) -> Vec<Function> {
    let headers: HashMap<&str, Vec<&str>> = source.lines().filter_map(header).collect();
    source
        .lines()
        .filter_map(|line| {
            line.strip_prefix("pure ")
                .unwrap_or(line)
                .strip_prefix("def ")
        })
        .filter_map(|line| {
            let line = line
                .strip_prefix("rec ")
                .or_else(|| line.strip_prefix("and "))
                .unwrap_or(line);
            let (left, signature) = line.split_once(" :: ")?;
            let name = left.split_whitespace().next()?;
            let signature = signature.split(" = ").next().unwrap_or(signature);
            Some(Function {
                name: name.to_owned(),
                types: parameters(signature)
                    .into_iter()
                    .map(|ty| ty.strip_prefix("own ").unwrap_or(&ty).to_owned())
                    .collect(),
                names: headers
                    .get(name)
                    .map(|names| names.iter().map(|name| (*name).to_owned()).collect())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// A type without its borrow and regions: `ref mut { r } Mut<'a> { r }` is `Mut<'a> { r }`, `&(Maybe<'a>)` is
/// `Maybe<'a>`.
fn core(ty: &str) -> &str {
    let mut ty = ty.trim();
    loop {
        let rest = if let Some(rest) = ty.strip_prefix("own ") {
            rest
        } else if let Some(rest) = ty.strip_prefix("ref ") {
            rest
        } else if let Some(rest) = ty.strip_prefix("mut ") {
            rest
        } else if let Some(inner) = ty
            .strip_prefix("&(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            inner
        } else if let Some(rest) = ty.strip_prefix('&') {
            rest
        } else if ty.starts_with('{') {
            ty.split_once('}').map_or(ty, |(_, rest)| rest)
        } else {
            return ty;
        };
        ty = rest.trim_start();
    }
}

/// Whether `ty` is one of the subject types: the name of a `head` followed by a non-identifier character.
fn is_subject(ty: &str, heads: &[&str]) -> bool {
    let ty = core(ty);
    heads.iter().any(|head| match *head {
        "[]" => ty.starts_with('[') && !ty.starts_with("[|"),
        "[|" => ty.starts_with("[|"),
        head => ty.strip_prefix(head).is_some_and(|rest| {
            !rest.starts_with(|character: char| character.is_alphanumeric() || character == '_')
        }),
    })
}

fn is_protocol(name: &str) -> bool {
    name.starts_with(|character: char| character.is_uppercase())
}

fn module_of(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.split('.').next().unwrap_or(file)
}

fn source_of(module: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|(path, _)| module_of(path) == module)
        .map(|(_, text)| *text)
        .unwrap_or_else(|| panic!("std/{module} does not exist"))
}

/// The functions of a module that break the rule, and how many functions the rule applied to.
fn misplaced(
    module: &str,
    source: &str,
    heads: &[&str],
    names: &[&str],
    except: &[(&str, &str)],
) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut misplaced = Vec::new();
    for function in functions(source) {
        if is_protocol(&function.name) || except.iter().any(|(name, _)| *name == function.name) {
            continue;
        }
        let types = &function.types;
        if !types.iter().any(|ty| is_subject(ty, heads)) {
            continue;
        }
        checked += 1;
        let signature = format!("{module}.{} :: {}", function.name, types.join(" -> "));
        if !types.last().is_some_and(|ty| is_subject(ty, heads)) {
            misplaced.push(format!("{signature}  (the subject is not last)"));
        } else if !names.is_empty()
            && types.len() > 1
            && function.names.len() == types.len()
            && function
                .names
                .last()
                .is_some_and(|name| !names.contains(&name.as_str()))
        {
            misplaced.push(format!(
                "{signature}  (the last parameter `{}` must be named {names:?})",
                function.names.last().unwrap()
            ));
        }
    }
    (checked, misplaced)
}

const RULE: &str = "The value that a function reads, transforms or updates is its last argument (AGENTS.md, \
                    \"Standard library API conventions\")";

#[test]
fn pure_definitions_remain_subject_to_argument_order_checks() {
    let source = "pure def contains :: own i64 -> [i64] -> bool\nfn contains needle values = true\nprivate pure def hidden :: i64 -> i64\nfn hidden value = value\n";
    let definitions = functions(source);
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].name, "contains");
    assert_eq!(definitions[0].types, ["i64", "[i64]"]);
    assert_eq!(definitions[0].names, ["needle", "values"]);
}

#[test]
fn every_std_module_is_classified() {
    let unclassified: Vec<&str> = SOURCES
        .iter()
        .map(|(path, _)| *path)
        .filter(|path| !MODULES.iter().any(|(name, _)| *name == module_of(path)))
        .collect();
    assert!(
        unclassified.is_empty(),
        "{unclassified:?} must be classified in `MODULES` of tests/std_argument_order.rs: take the subject last, or \
         document why not. {RULE}"
    );
    for (name, _) in MODULES {
        assert!(
            SOURCES.iter().any(|(path, _)| module_of(path) == *name),
            "`MODULES` classifies std/{name}, which does not exist"
        );
    }
}

#[test]
fn the_modules_are_listed_once_and_in_order() {
    let names: Vec<&str> = MODULES.iter().map(|(name, _)| *name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        names, sorted,
        "list each module once, in alphabetical order"
    );
}

#[test]
fn every_exemption_gives_its_reason() {
    for (module, kind) in MODULES {
        if let Exempt(reason) = kind {
            assert!(
                reason.len() > 20,
                "{module}: say why its functions do not take the subject last"
            );
        }
    }
}

#[test]
fn every_function_takes_its_subject_last() {
    let mut failures = Vec::new();
    let mut checked_total = 0;
    for (module, kind) in MODULES {
        let Subject {
            heads,
            names,
            except,
        } = kind
        else {
            continue;
        };
        let (checked, misplaced) = misplaced(module, source_of(module), heads, names, except);
        checked_total += checked;
        failures.extend(misplaced);
        for (name, _) in *except {
            assert!(
                functions(source_of(module))
                    .iter()
                    .any(|function| function.name == *name),
                "{module}.{name} is excepted but does not exist"
            );
        }
    }
    assert!(
        checked_total > 400,
        "only {checked_total} functions were read: the signature reader is broken"
    );
    assert!(failures.is_empty(), "{RULE}:\n{}", failures.join("\n"));
}

#[test]
fn the_collections_are_read_in_full() {
    for (module, minimum) in [("Array", 100), ("List", 80), ("Vec", 40)] {
        let Some((
            _,
            Subject {
                heads,
                names,
                except,
            },
        )) = MODULES.iter().find(|(name, _)| *name == module)
        else {
            panic!("{module} is classified as a subject module")
        };
        let (checked, misplaced) = misplaced(module, source_of(module), heads, names, except);
        assert!(
            checked >= minimum,
            "{module}: only {checked} functions were read"
        );
        assert!(misplaced.is_empty(), "{misplaced:?}");
    }
}

#[test]
fn the_single_argument_modules_have_no_function_of_two_parameters() {
    let mut failures = Vec::new();
    for (module, kind) in MODULES {
        if !matches!(kind, SingleArgument) {
            continue;
        }
        for function in functions(source_of(module)) {
            if !is_protocol(&function.name) && function.types.len() > 1 {
                failures.push(format!(
                    "{module}.{} :: {}",
                    function.name,
                    function.types.join(" -> ")
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "classify these modules again in tests/std_argument_order.rs: take the subject last, or document why not. \
         {RULE}:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_pure_functions_of_the_exempt_modules_are_pinned() {
    for (module, name, expected) in PINNED {
        let function = functions(source_of(module))
            .into_iter()
            .find(|function| function.name == *name)
            .unwrap_or_else(|| panic!("{module}.{name} does not exist"));
        assert_eq!(function.types, *expected, "{module}.{name}. {RULE}");
    }
}

/// The compiler implements some functions of these modules itself: their schemes follow the rule too.
#[test]
fn the_builtins_take_the_subject_last() {
    fn core(ty: &BuiltinType) -> &BuiltinType {
        match ty {
            BuiltinType::Reference(inner, _) => core(inner),
            ty => ty,
        }
    }
    fn is_subject(module: &str, ty: &BuiltinType) -> bool {
        matches!(
            (module, core(ty)),
            ("Array", BuiltinType::Array(_) | BuiltinType::ArrayView(_))
                | ("List", BuiltinType::List(_))
                | ("Vec", BuiltinType::Vec(_))
                | ("String", BuiltinType::Concrete(Type::String))
                | ("Utf8String", BuiltinType::Concrete(Type::Utf8String))
        )
    }
    let mut checked = 0;
    let mut misplaced = Vec::new();
    for builtin in Builtin::ALL {
        let name = builtin.name();
        let Some((module, _)) = name.split_once('.') else {
            continue;
        };
        if !["Array", "List", "Vec", "String", "Utf8String"].contains(&module) {
            continue;
        }
        let parameters = builtin.scheme().parameters;
        if !parameters.iter().any(|ty| is_subject(module, ty)) {
            continue;
        }
        checked += 1;
        if !parameters.last().is_some_and(|ty| is_subject(module, ty)) {
            misplaced.push(format!("{name}: the subject is not last"));
        }
    }
    assert!(checked > 10, "only {checked} builtins were read");
    assert!(misplaced.is_empty(), "{RULE}:\n{}", misplaced.join("\n"));
}

#[test]
fn the_check_finds_a_subject_that_is_not_last() {
    let source = "def get :: ref Map<'k, 'v> -> 'k -> Maybe<'v>\n\
                  fn get map key = map\n\
                  def put :: 'k -> 'v -> Map<'k, 'v> -> Map<'k, 'v>\n\
                  fn put key value map = map\n\
                  def find :: ref string -> ref string -> Maybe<i64>\n\
                  fn find text needle = text\n\
                  def Bind :: Map<'k, 'v> -> ('v -> 'w) -> Map<'k, 'w>\n\
                  fn Bind map next = map\n\
                  def merge :: Map<'k, 'v> -> ref string -> ref string -> unit\n\
                  fn merge map first text = map\n";
    let (checked, found) = misplaced("Test", source, &["Map"], &[], &[]);
    assert_eq!(checked, 3, "{found:?}");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].starts_with("Test.get ::"), "{found:?}");
    assert!(found[1].starts_with("Test.merge ::"), "{found:?}");
    // Two strings cannot be told apart by their types: the name of the last parameter decides.
    let (checked, found) = misplaced("Test", source, &["string"], &["text"], &[]);
    assert_eq!(checked, 2, "{found:?}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("Test.find ::"), "{found:?}");
    // An exception is skipped.
    let (_, found) = misplaced(
        "Test",
        source,
        &["Map"],
        &[],
        &[("get", "protocol"), ("merge", "protocol")],
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_signature_reader_splits_at_the_arrows_outside_brackets() {
    assert_eq!(
        parameters("Copy<'a> => ('a -> 'state -> 'state) -> 'state -> ref ['a] -> 'state"),
        ["('a -> 'state -> 'state)", "'state", "ref ['a]"]
    );
    assert_eq!(
        parameters("i64 -> ref mut ['a..] -> (ref mut ['a..] * ref mut ['a..])"),
        ["i64", "ref mut ['a..]"]
    );
    assert_eq!(parameters("['a]"), Vec::<String>::new());
}

#[test]
fn the_type_reader_finds_the_subject_through_borrows_and_regions() {
    let maybe = ["Maybe"];
    assert!(is_subject("&(Maybe<'a>)", &maybe));
    assert!(is_subject("ref Maybe<i64>", &maybe));
    assert!(!is_subject("MaybeNot<'a>", &maybe));
    let view = ["MatrixView", "Mut"];
    assert!(is_subject("ref mut { r } Mut<'a> { r }", &view));
    assert!(is_subject("MatrixView<'a> { r }", &view));
    assert!(!is_subject("Matrix<'a>", &view));
    assert!(is_subject("ref mut ['a..]", &["[]"]));
    assert!(!is_subject("[|'a|]", &["[]"]));
    assert!(is_subject("ref [|i64|]", &["[|"]));
    assert!(is_subject("ref [string]", &["[string]"]));
    assert!(!is_subject("(Maybe<'a> * i64)", &maybe));
}

#[test]
fn the_function_reader_pairs_the_def_line_with_the_fn_line() {
    let source = "def rec find :: ref string -> ref string -> Maybe<i64>\n\
                  doc\n\
                  fn rec find needle text =\n\
                  \x20   0\n\
                  private def hidden :: ref string -> ref string -> bool\n\
                  def sub { r } :: i64 -> i64 -> ref string -> string\n\
                  fn sub first last text = text\n";
    let functions = functions(source);
    assert_eq!(functions.len(), 2);
    assert_eq!(functions[0].name, "find");
    assert_eq!(functions[0].names, ["needle", "text"]);
    assert_eq!(functions[1].name, "sub");
    assert_eq!(functions[1].types, ["i64", "i64", "ref string"]);
}

fn accepts(source: &str) {
    analyze(source).unwrap_or_else(|error| panic!("{source}\n{}: {}", error.code, error.message));
}

fn rejects(source: &str, codes: &[&str]) {
    let error = analyze(source).expect_err(source);
    assert!(
        codes.contains(&error.code),
        "{source}\n{}: {}",
        error.code,
        error.message
    );
}

/// The functions whose collection came first, called in that old order. Each is a type error now, whichever argument
/// clashes first.
#[test]
fn the_old_order_of_the_functions_that_moved_the_array_is_rejected() {
    let calls = [
        "Array.get (ref xs) 0",
        "Array.at (ref xs) 0",
        "Array.sub (ref xs) 0 1",
        "Array.set (ref mut xs) 0 1",
        "Array.fill (ref mut xs) 0 1 7",
        "Array.blit (ref xs) 0 (ref mut ys) 0 1",
        "Array.fold_back (\\x acc -> acc + x) (ref xs) 0",
        "Array.foldr_back (\\x acc -> acc + deref x) (ref xs) 0",
        "Array.fold_back2 (\\x y acc -> acc + x + y) (ref xs) (ref xs) 0",
        "Array.scan_back (\\x acc -> acc + x) (ref xs) 0",
        "Array.map_fold_back (\\x acc -> (x, acc)) (ref xs) 0",
        "Array.index_of (ref xs) (ref 1)",
        "Array.binary_search (ref xs) (ref 1)",
        "Array.update xs 0 (\\x -> x + 1)",
        "Array.swap xs 0 1",
        "Array.write (ref mut xs) 0 1",
        "Array.swap_in (ref mut xs) 0 1",
        "Array.split_at_mut (ref mut xs) 1",
        "List.fold_back (\\x acc -> acc + x) (ref zs) 0",
        "List.foldr_back (\\x acc -> acc + deref x) (ref zs) 0",
        "List.fold_back2 (\\x y acc -> acc + x + y) (ref zs) (ref zs) 0",
        "List.scan_back (\\x acc -> acc + x) (ref zs) 0",
        "List.map_fold_back (\\x acc -> (x, acc)) (ref zs) 0",
    ];
    for call in calls {
        let source = format!(
            "let mut xs = [1, 2, 3]\nlet mut ys = [0, 0, 0]\nlet zs = [|1, 2, 3|]\nlet result = {call}\n0"
        );
        let error = analyze(&source).expect_err(call);
        assert!(
            matches!(error.code, "E1003" | "E1005" | "E1014" | "E1015"),
            "{call}\n{}: {}",
            error.code,
            error.message
        );
    }
}

#[test]
fn the_new_order_type_checks() {
    let calls = [
        "Array.get 0 (ref xs)",
        "Array.at 0 (ref xs)",
        "Array.sub 0 1 (ref xs)",
        "Array.set 0 1 xs",
        "Array.fill 0 1 7 xs",
        "Array.blit 0 0 1 (ref xs) ys",
        "Array.fold_back (\\x acc -> acc + x) 0 (ref xs)",
        "Array.foldr_back (\\x acc -> acc + deref x) 0 (ref xs)",
        "Array.fold_back2 (\\x y acc -> acc + x + y) 0 (ref xs) (ref xs)",
        "Array.scan_back (\\x acc -> acc + x) 0 (ref xs)",
        "Array.map_fold_back (\\x acc -> (x, acc)) 0 (ref xs)",
        "Array.index_of (ref 1) (ref xs)",
        "Array.binary_search (ref 1) (ref xs)",
        "Array.write 0 1 xs",
        "Array.swap_in 0 1 xs",
        "Array.split_at_mut 1 xs",
        "List.fold_back (\\x acc -> acc + x) 0 (ref zs)",
        "List.foldr_back (\\x acc -> acc + deref x) 0 (ref zs)",
        "List.fold_back2 (\\x y acc -> acc + x + y) 0 (ref zs) (ref zs)",
        "List.scan_back (\\x acc -> acc + x) 0 (ref zs)",
        "List.map_fold_back (\\x acc -> (x, acc)) 0 (ref zs)",
    ];
    for call in calls {
        accepts(&format!(
            "let mut xs = [1, 2, 3]\nlet mut ys = [0, 0, 0]\nlet zs = [|1, 2, 3|]\nlet result = {call}\n0"
        ));
    }
    accepts("let result = Array.update 0 (\\x -> x + 1) [1, 2]\nresult[0]");
    accepts("let result = Array.swap 0 1 [1, 2]\nresult[0]");
}

/// The values that the calls below work on.
const PRELUDE: &str = "let map = HashMap.insert 1 10 (HashMap.empty())\n\
                       let ordered = Map.insert 1 10 (Map.empty())\n\
                       let hashed = HashSet.insert 1 (HashSet.empty())\n\
                       let sorted = Set.insert 1 (Set.empty())\n\
                       let arena: Arena<i64> = Arena.empty()\n\
                       let text = \"hello\"\n\
                       let bytes = u8\"hello\"\n\
                       let matrix = Matrix.of_array 2 2 [1, 2, 3, 4]\n\
                       let maybe = Maybe.Some 1\n\
                       let ok: Result<i64, string> = Result.Ok 1\n\
                       let re = Result.get (Regex.compile (ref \"a\"))\n\
                       let value = Json.Null\n";

/// The functions of the maps, sets, arenas, texts, matrices, options and results whose subject came first or in
/// the middle, called in that old order. The functions whose other arguments have the subject's own type (a regex
/// replacement and its text) are covered by the behavior tests, not here.
#[test]
fn the_old_order_of_the_other_modules_is_rejected() {
    accepts(&format!("{PRELUDE}0"));
    let calls = [
        "HashMap.contains_key (ref map) 1",
        "HashMap.get (ref map) 1",
        "HashMap.at (ref map) 1",
        "HashMap.insert map 2 20",
        "HashMap.remove map 1",
        "HashSet.contains (ref hashed) 1",
        "HashSet.insert hashed 2",
        "HashSet.remove hashed 1",
        "Map.contains_key (ref ordered) 1",
        "Map.get (ref ordered) 1",
        "Map.at (ref ordered) 1",
        "Map.insert ordered 2 20",
        "Map.remove ordered 1",
        "Set.contains (ref sorted) 1",
        "Set.insert sorted 2",
        "Set.remove sorted 1",
        "Arena.insert arena 5",
        "String.slice (ref text) 1 3",
        "String.sub (ref text) 1 2",
        "String.decode_at (ref text) 0",
        "String.repeat (ref text) 2",
        "Utf8String.slice (ref bytes) 1 3",
        "Utf8String.sub (ref bytes) 1 2",
        "Utf8String.decode_at (ref bytes) 0",
        "Utf8String.repeat (ref bytes) 2",
        "Matrix.at (ref matrix) 0 1",
        "Matrix.get (ref matrix) 0 1",
        "Matrix.row (ref matrix) 0",
        "Matrix.set matrix 0 1 9",
        "Maybe.bind maybe (\\x -> Maybe.Some (x + 1))",
        "Maybe.or_else maybe (\\() -> Maybe.Some 0)",
        "Result.bind ok (\\x -> Result.Ok (x + 1))",
        "Result.or_else ok (\\() -> Result.Ok 0)",
        "IO.bind (IO.pure 1) (\\x -> IO.pure (x + 1))",
        "Regex.find_at (ref re) (ref text) 0",
        "Json.write_bool (Json.writer ()) true",
        "Json.write_text (Json.writer ()) (ref text)",
        "Json.to_utf8string_pretty (ref value) 2",
        "Net.parse_ip (ref text) 80",
    ];
    for call in calls {
        let source = format!("{PRELUDE}let result = {call}\n0");
        let error = analyze(&source).expect_err(call);
        assert!(
            matches!(error.code, "E1002" | "E1003" | "E1005" | "E1014" | "E1015"),
            "{call}\n{}: {}",
            error.code,
            error.message
        );
    }
}

#[test]
fn the_new_order_of_the_other_modules_type_checks() {
    let calls = [
        "HashMap.contains_key 1 (ref map)",
        "HashMap.get 1 (ref map)",
        "HashMap.at 1 (ref map)",
        "HashMap.insert 2 20 map",
        "HashMap.remove 1 map",
        "HashSet.contains 1 (ref hashed)",
        "HashSet.insert 2 hashed",
        "HashSet.remove 1 hashed",
        "Map.contains_key 1 (ref ordered)",
        "Map.get 1 (ref ordered)",
        "Map.at 1 (ref ordered)",
        "Map.insert 2 20 ordered",
        "Map.remove 1 ordered",
        "Set.contains 1 (ref sorted)",
        "Set.insert 2 sorted",
        "Set.remove 1 sorted",
        "Arena.insert 5 arena",
        "String.slice 1 3 (ref text)",
        "String.sub 1 2 (ref text)",
        "String.decode_at 0 (ref text)",
        "String.repeat 2 (ref text)",
        "Utf8String.slice 1 3 (ref bytes)",
        "Utf8String.sub 1 2 (ref bytes)",
        "Utf8String.decode_at 0 (ref bytes)",
        "Utf8String.repeat 2 (ref bytes)",
        "Matrix.at 0 1 (ref matrix)",
        "Matrix.get 0 1 (ref matrix)",
        "Matrix.row 0 (ref matrix)",
        "Matrix.set 0 1 9 matrix",
        "Maybe.bind (\\x -> Maybe.Some (x + 1)) maybe",
        "Maybe.or_else (\\() -> Maybe.Some 0) maybe",
        "Result.bind (\\x -> Result.Ok (x + 1)) ok",
        "Result.or_else (\\() -> Result.Ok 0) ok",
        "IO.bind (\\x -> IO.pure (x + 1)) (IO.pure 1)",
        "Regex.find_at (ref re) 0 (ref text)",
        "Regex.replace_all (ref re) (ref text) (ref text)",
        "Json.write_bool true (Json.writer ())",
        "Json.write_text (ref text) (Json.writer ())",
        "Json.to_utf8string_pretty 2 (ref value)",
        "Net.parse_ip 80 (ref text)",
    ];
    for call in calls {
        accepts(&format!("{PRELUDE}let result = {call}\n0"));
    }
}

#[test]
fn pipelines_pass_the_collection_on() {
    for source in [
        "let xs = [3, 1, 2]\nlet top = xs |> Array.sort |> Array.rev |> Array.take 2\ntop[0]",
        "let xs = [1, 2, 3]\nlet middle = xs |> Array.sub 1 1\nmiddle[0]",
        "let xs = [1, 2, 3]\nxs |> Array.fold_back (\\x acc -> acc * 10 + x) 0",
        "let xs = [1, 2, 3]\nlet states = xs |> Array.scan_back (\\x acc -> acc + x) 0\nstates[0]",
        "let xs = [1, 2, 3]\nxs |> Array.get 1",
        "let result = [1, 2] |> Array.update 0 (\\x -> x + 1) |> Array.swap 0 1\nresult[0]",
        "let zs = [|1, 2, 3|]\nzs |> List.fold_back (\\x acc -> acc * 10 + x) 0",
        "let zs = [|3, 1, 2|]\nlet top = zs |> List.sort |> List.rev |> List.take 2\ntop.length",
        "let vs = Vec.empty() |> Vec.add 3 |> Vec.add 1 |> Vec.sort\nvs[0]",
    ] {
        accepts(source);
    }
}

/// The subject of the maps, sets, arenas, texts, matrices, options and results is last as well, so that they chain.
#[test]
fn pipelines_pass_the_other_subjects_on() {
    for source in [
        "let map = HashMap.empty() |> HashMap.insert 1 10 |> HashMap.insert 2 20 |> HashMap.remove 1\nHashMap.length (ref map)",
        "let map = Map.empty() |> Map.insert 1 10 |> Map.insert 2 20 |> Map.remove 1\nMap.length (ref map)",
        "let set = Set.empty() |> Set.insert 3 |> Set.insert 1 |> Set.remove 3\nSet.length (ref set)",
        "let set = HashSet.empty() |> HashSet.insert 3 |> HashSet.insert 1 |> HashSet.remove 3\nHashSet.length (ref set)",
        "let map = HashMap.insert 1 10 (HashMap.empty())\nmap |> HashMap.get 1",
        "let pair = Arena.empty() |> Arena.insert 7\nmatch pair with\n| (arena, handle) ->\n    let bumped = arena |> Arena.update handle (\\x -> x + 1)\n    Arena.length (ref bumped)",
        "let text = \"hello world\"\nlet word = text |> String.sub 6 5\nMaybe.get word",
        "let text = \"ab\"\ntext |> String.repeat 3",
        "let bytes = u8\"hello\"\nlet part = bytes |> Utf8String.slice 1 3\nMaybe.get part",
        "let matrix = Matrix.of_array 2 2 [1, 2, 3, 4]\nlet cell = matrix |> Matrix.get 1 1\nMaybe.get cell",
        "let matrix = Matrix.of_array 2 2 [1, 2, 3, 4] |> Matrix.set 0 0 9\nMatrix.rows (ref matrix)",
        "let found = Maybe.Some 3 |> Maybe.bind (\\x -> if x > 2 then Maybe.Some (x * 2) else Maybe.None)\nMaybe.get found",
        "let nothing: Maybe<i64> = Maybe.None\nlet found = nothing |> Maybe.or_else (\\() -> Maybe.Some 4)\nMaybe.get found",
        "let ok: Result<i64, string> = Result.Ok 3\nlet next = ok |> Result.bind (\\x -> Result.Ok (x + 1))\nResult.get next",
        "let json = Json.writer () |> Json.open_array\nlet written = json |> Result.bind (Json.write_bool true) |> Result.bind Json.close_array\nlet text = written |> Result.bind Json.finish\nResult.is_ok (ref text)",
    ] {
        accepts(source);
    }
}

/// The slice is lent last, so an argument before it may still read it; passed on by a pipeline, the slice is lent
/// first and the arguments after it cannot.
#[test]
fn an_argument_may_read_the_slice_that_is_lent_last() {
    accepts("let mut values = [1, 2, 3]\nArray.write 0 (values[1] * 10) values\nvalues[0]");
    accepts(
        "let mut values = [1, 2, 3]\nArray.write (values.length - 1) (values[1] * 10) (ref mut values)\nvalues[2]",
    );
    accepts("let mut values = [1, 2, 3]\nArray.swap_in 0 (values.length - 1) values\nvalues[0]");
    rejects(
        "let mut values = [1, 2, 3]\nvalues |> Array.write 0 (values[1] * 10)\nvalues[0]",
        &["E1014"],
    );
}

/// A mutable array passes on as an exclusive slice, also into a partial application that has a lambda.
#[test]
fn a_mutable_array_pipes_into_the_functions_that_write_it() {
    for source in [
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.set 1 9\nxs[1]",
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.write 1 9\nxs[1]",
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.swap_in 0 2\nxs[0]",
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.fill 1 2 7\nxs[1]",
        "let mut xs = [3, 1, 2]\nref mut xs |> Array.sort_in_place_with (\\a b -> a - b)\nxs[0]",
        "let mut xs = [3, 1, 2]\nref mut xs |> Array.sort_in_place_by (\\a -> -a)\nxs[0]",
        "let mut xs = [3, 1, 2]\nxs |> Array.set 0 5\nxs[0]",
    ] {
        accepts(source);
    }
    // The element types must agree.
    rejects(
        "let mut xs = [1, 2, 3]\nref mut xs |> Array.set 1 \"nine\"\n0",
        &["E1003"],
    );
}

/// A composition passes the collection that one function returns to the next, which borrows it. The element type is
/// not known where `Array.sort >> Array.rev` is written, so the borrow is settled once the function is checked.
#[test]
fn partial_applications_compose() {
    for source in [
        "let both = Array.sort >> Array.rev\nlet top = both [3, 1, 2]\ntop[0]",
        "let top = [3, 1, 2] |> (Array.sort >> Array.rev)\ntop[0]",
        "let both = Array.sort >> Array.rev >> Array.truncate 2\nlet top = [3, 1, 2] |> both\ntop[0]",
        "let sorted = Array.sort >> Array.truncate 2\nlet top = [3, 1, 2] |> sorted\ntop[0]",
        "let total = Array.map (\\x -> x * 2) >> Array.sum\n[1, 2, 3] |> total",
        "let both = Array.update_at 0 9 >> Array.swap 0 1\nlet changed = both [1, 2, 3]\nchanged[0]",
        "let both = List.sort >> List.rev\nlet top = both [|3, 1, 2|]\ntop.length",
        "let both = Vec.sort >> Vec.reverse\nlet top = both (Vec.of_array [3, 1, 2])\ntop[0]",
        "let both = HashMap.insert 1 10 >> HashMap.insert 2 20\nlet map = both (HashMap.empty())\nHashMap.length (ref map)",
        "let both = Set.insert 1 >> Set.remove 2\nlet set = both (Set.empty())\nSet.length (ref set)",
        "let shout = String.trim >> String.repeat 2\nlet text = \"  ab \"\ntext |> shout",
        "let chain = Maybe.bind (\\x -> Maybe.Some (x + 1)) >> Maybe.map (\\x -> x * 2)\nMaybe.get (chain (Maybe.Some 1))",
        "let chain = Result.bind (\\x -> Result.Ok (x + 1)) >> Result.map (\\x -> x * 2)\nlet ok: Result<i64, string> = Result.Ok 1\nResult.get (chain ok)",
    ] {
        accepts(source);
    }
    // The composition still cannot return a reference into the array that the first function made.
    for source in [
        "let first = Array.sort >> Array.minr\nlet xs = [3, 1, 2]\nlet smallest = first (ref xs)\n0",
        "let first = Array.sort >> Array.findr (\\x -> deref x > 1)\nlet xs = [3, 1, 2]\nlet found = first (ref xs)\n0",
        "let seed = 0\nlet last = Array.sort >> Array.foldr (\\acc x -> x) (ref seed)\nlet xs = [3, 1, 2]\nlet found = last (ref xs)\n0",
        "let first = List.sort >> List.minr\nlet xs = [|3, 1, 2|]\nlet smallest = first (ref xs)\n0",
    ] {
        rejects(source, &["E1013"]);
    }
}
