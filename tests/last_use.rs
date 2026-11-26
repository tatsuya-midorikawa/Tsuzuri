use tsuzuri::{analyze, llvm};

fn body<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split("\n\n")
        .find(|block| {
            block.starts_with("define ")
                && block
                    .lines()
                    .next()
                    .unwrap()
                    .contains(&format!("@tz.fn.Main.{name}("))
        })
        .unwrap_or_else(|| panic!("missing {name}"))
}

fn allocations(source: &str, name: &str, expected: usize) {
    allocations_by_target(source, name, [expected; 2]);
}

fn allocations_by_target(source: &str, name: &str, expected: [usize; 2]) {
    let module = analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    for (wasm, expected) in [false, true].into_iter().zip(expected) {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            body(&ir, name)
                .replace("call nonnull ptr @tz.alloc", "call ptr @tz.alloc")
                .matches("call ptr @tz.alloc")
                .count(),
            expected,
            "{}",
            body(&ir, name)
        );
        assert_eq!(
            ir,
            llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap()
        );
    }
}

const PROBE: &str = r"
def branch_reads :: [i64] -> [i64] = \xs -> if xs.length > 2 then Array.__set 0 1 xs else xs
def loop_update :: [i64] -> [i64] = \xs ->
    let mut ys = xs
    for i in 0 .. 3 do
        ys = Array.__set i i ys
    ys
def read_twice :: [i64] -> i64 = \xs ->
    let ys = Array.__set 0 9 xs
    xs[0] + ys[0]
def invariant_loop :: [i64] -> i64 = \xs ->
    let mut total = 0
    for i in 0 .. 3 do
        let ys = Array.__set 0 i xs
        total = total + ys[0]
    total + xs[1]
def build_pair :: [i64] -> [i64] -> ([i64] * [i64]) = \a b ->
    let size = a.length
    if size > 1 then (a, b) else (b, a)
def match_result :: Result<[i64], string> -> i64 = \r ->
    match r with
    | Result.Ok xs -> xs[0]
    | Result.Error e -> e.length
def wrap :: [i64] -> Result<[i64], string> = \xs -> Result.Ok xs
def match_temp :: i64 -> i64 = \n ->
    match wrap (new [i64](3, \i -> i + n)) with
    | Result.Ok xs -> xs[0]
    | Result.Error _ -> 0
";

#[test]
fn appendix_b_exact_allocation_counts() {
    for (name, expected) in [
        ("branch_reads", 0),
        ("loop_update", 0),
        ("read_twice", 1),
        ("invariant_loop", 1),
        ("build_pair", 0),
        ("match_result", 0),
        ("match_temp", 1),
    ] {
        allocations(PROBE, name, expected);
    }
}

#[test]
fn moves_the_last_straight_line_read() {
    allocations(
        r"def f :: [i64] -> i64 = \xs -> { let ys = Array.__set 0 9 xs; let zs = Array.__set 0 8 xs; ys[0] + zs[0] }",
        "f",
        1,
    );
}

#[test]
fn copies_when_a_later_argument_reads_the_local() {
    allocations(
        r"def put :: [i64] -> i64 -> [i64] = \ys v -> Array.__set 0 v ys
def f :: [i64] -> [i64] = \xs -> put xs (xs[1] + 1)",
        "f",
        1,
    );
}

#[test]
fn moves_when_an_earlier_argument_reads_the_local() {
    allocations(
        r"def f :: [i64] -> [i64] = \xs -> Array.__set 0 (xs[1] + 1) xs",
        "f",
        0,
    );
}

#[test]
fn copies_a_borrowed_local() {
    allocations(
        r"def f :: [i64] -> i64 = \xs -> { let r = ref xs; let ys = Array.__set 0 9 xs; r[0] + ys[0] }",
        "f",
        1,
    );
}

#[test]
fn closure_capture_copies_and_the_later_read_moves() {
    // Native Windows keeps the documented heap fallback for local binding initializers.
    allocations_by_target(
        r"def f :: [i64] -> i64 = \xs -> { let g = \i -> xs[i]; let ys = Array.__set 0 9 xs; g 0 + ys[0] }",
        "f",
        [if cfg!(windows) { 2 } else { 1 }, 1],
    );
}

#[test]
fn moves_the_last_read_in_each_branch() {
    allocations(PROBE, "branch_reads", 0);
}

#[test]
fn recursive_array_accumulators_have_no_clone_in_the_lifted_worker() {
    let module = analyze(
        r"def fill :: i64 -> [i64] -> [i64] = \n values ->
    let rec go = \i xs -> if i == n then xs else go (i + 1) (Array.__set 0 i xs)
    go 0 values",
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        let functions: std::collections::BTreeMap<_, _> = ir
            .split("\n\n")
            .filter(|block| block.starts_with("define "))
            .map(|block| {
                (
                    block
                        .lines()
                        .next()
                        .unwrap()
                        .split('@')
                        .nth(1)
                        .unwrap()
                        .split('(')
                        .next()
                        .unwrap(),
                    block,
                )
            })
            .collect();
        let mut pending = vec!["tz.fn.Main.fill"];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name) {
                continue;
            }
            let function = functions[name];
            assert!(!function.contains("ptr @tz.alloc("), "{function}");
            for line in function.lines().filter(|line| line.contains("call ")) {
                let Some(target) = line
                    .split('@')
                    .nth(1)
                    .and_then(|call| call.split('(').next())
                else {
                    continue;
                };
                if target.starts_with("tz.specialized.") || target.starts_with("tz.fn.Main.") {
                    pending.push(target);
                }
            }
        }
        assert!(seen.len() >= 2);
    }
}

#[test]
fn moves_a_self_update() {
    allocations(
        r"def f :: [i64] -> [i64] = \xs -> { let mut ys = xs; ys = Array.__set 0 1 ys; ys }",
        "f",
        0,
    );
}

#[test]
fn copies_a_read_before_a_partial_assignment() {
    allocations(
        r"def f :: [i64] -> i64 = \xs -> { let mut a = xs; let b = Array.__set 0 9 a; Array.write 0 3 (ref mut a[0..]); a[0] + b[0] }",
        "f",
        1,
    );
}

#[test]
fn moves_a_self_update_in_a_loop() {
    allocations(PROBE, "loop_update", 0);
}

#[test]
fn while_and_nested_loop_fixed_points_preserve_self_updates() {
    allocations(
        r"def f :: [i64] -> i64 -> [i64] = \xs n ->
    let mut ys = xs
    let mut i = 0
    while i < n do
        i = i + 1
        for j in 0 .. 2 do
            if j == 1 then continue else ()
            ys = Array.__set 0 i ys
        if i == 2 then continue else ()
        ys = Array.__set 1 i ys
    ys",
        "f",
        0,
    );
}

#[test]
fn short_circuit_skips_or_transfers_without_losing_later_reads() {
    allocations(
        r"def consume :: [i64] -> bool = \xs -> xs.length == 0
def f :: bool -> [i64] -> bool = \c xs -> c || consume xs",
        "f",
        0,
    );
    allocations(
        r"def consume :: [i64] -> bool = \xs -> xs.length == 0
def f :: bool -> [i64] -> bool = \c xs ->
    let taken = c || consume xs
    taken && xs.length == 0",
        "f",
        1,
    );
}

#[test]
fn copies_a_loop_invariant_read() {
    allocations(PROBE, "invariant_loop", 1);
}

#[test]
fn copies_before_a_break_when_the_local_is_read_after_the_loop() {
    allocations(
        r"def f :: [i64] -> i64 = \xs ->
    let mut total = 0
    for i in 0 .. 3 do
        let ys = Array.__set 0 i xs
        total = total + ys[0]
        if i == 2 then break else ()
    total + xs[1]",
        "f",
        1,
    );
}

#[test]
fn moves_a_loop_local() {
    allocations(
        r"def f :: i64 -> i64 = \n ->
    let mut total = 0
    for i in 0 .. n do
        let xs = new [i64](2, \_k -> i)
        let ys = Array.__set 0 i xs
        total = total + ys[0]
    total",
        "f",
        1,
    );
}

#[test]
fn continue_keeps_the_head_live() {
    allocations(
        r"def f :: [i64] -> i64 = \xs ->
    let mut total = 0
    for i in 0 .. 3 do
        let ys = Array.__set 0 i xs
        total = total + ys[0]
        if i % 2 == 0 then continue else ()
    total",
        "f",
        1,
    );
}

#[test]
fn break_in_a_later_argument_preserves_the_old_value() {
    allocations(
        r"def put :: [i64] -> i64 -> [i64] = \ys v -> Array.__set 0 v ys
def f :: [i64] -> [i64] = \xs ->
    let mut ys = xs
    for i in 0 .. 3 do
        ys = put ys (if i == 2 then { break; i } else i)
    ys",
        "f",
        1,
    );
}

#[test]
fn break_in_an_earlier_argument_leaves_the_array_unmoved() {
    allocations(
        r"def f :: [i64] -> [i64] = \xs ->
    let mut ys = xs
    for i in 0 .. 3 do
        ys = Array.__set 0 (if i == 2 then { break; i } else i) ys
    ys",
        "f",
        0,
    );
}

#[test]
fn guard_falls_through_keeps_the_local_live() {
    allocations(
        r"def f :: Maybe<[i64]> -> i64 = \m ->
    match m with
    | Maybe.Some xs when xs[0] > 10 -> xs[0]
    | Maybe.Some xs -> xs[1]
    | Maybe.None -> 0",
        "f",
        0,
    );
}

#[test]
fn an_explicit_guard_read_cannot_move_the_aliased_subject() {
    allocations(
        r"def accepts :: Maybe<[i64]> -> bool = \m ->
    match m with | Maybe.Some xs -> xs[0] >= 0 | Maybe.None -> false
def f :: Maybe<[i64]> -> i64 = \m ->
    match m with
    | Maybe.Some xs when accepts m -> xs[0] + xs[2]
    | Maybe.Some xs -> xs[1]
    | Maybe.None -> 0",
        "f",
        3,
    );
}

#[test]
fn moves_in_each_arm() {
    allocations(
        r"def f :: i64 -> [i64] -> [i64] = \n xs ->
    match n with
    | 0 -> Array.__set 0 9 xs
    | _ -> xs",
        "f",
        0,
    );
}

#[test]
fn match_arm_reads_after_a_binding() {
    allocations(
        r"def f :: [i64] -> i64 = \xs ->
    match xs with
    | bound -> { let ys = Array.__set 0 9 xs; ys[0] + bound[0] }",
        "f",
        1,
    );
}

#[test]
fn moves_a_payload_out_of_a_temporary() {
    allocations(PROBE, "match_temp", 1);
}

#[test]
fn copies_fields_nested_inside_a_union_payload() {
    allocations(
        r"def f :: Maybe<([i64] * i64)> -> [i64] = \value ->
    match value with
    | Maybe.Some (items, _) -> items
    | Maybe.None -> []",
        "f",
        1,
    );
}

#[test]
fn moves_a_payload_out_of_a_last_use_local() {
    allocations(PROBE, "match_result", 0);
}

#[test]
fn copies_a_payload_when_the_local_is_used_after_the_match() {
    allocations(
        r"def f :: Maybe<[i64]> -> i64 = \m ->
    let first = match m with | Maybe.Some xs -> xs[0] | Maybe.None -> 0
    let second = match m with | Maybe.Some xs -> xs[1] | Maybe.None -> 0
    first + second",
        "f",
        1,
    );
}

#[test]
fn copies_overlapping_bindings() {
    allocations(
        r"def f :: Maybe<[i64]> -> i64 = \m ->
    match m with
    | Maybe.Some xs as whole -> xs[0] + (match whole with | Maybe.Some ys -> ys[1] | Maybe.None -> 0)
    | Maybe.None -> 0",
        "f",
        3,
    );
}

#[test]
fn copies_a_payload_reached_through_an_element() {
    allocations(
        r"def f :: [Maybe<[i64]>] -> i64 = \rows ->
    match rows[0] with | Maybe.Some xs -> xs[0] | Maybe.None -> 0",
        "f",
        1,
    );
}

#[test]
fn user_drop_can_still_observe_the_unmoved_record_and_union_payloads() {
    allocations(
        r"extern def drop_log :: i64 -> unit
record Bag { items: [i64] }
instance Drop<Bag> { fn drop value = drop_log value.items.length }
def f :: Bag -> i64 = \bag -> match bag with | { items = xs } -> xs[0]",
        "f",
        1,
    );
    allocations(
        r"extern def drop_log :: i64 -> unit
union Bag = Empty | Full of [i64]
instance Drop<Bag> { fn drop value = drop_log (match value with | Full xs -> xs.length | Empty -> 0) }
def f :: Bag -> i64 = \bag -> match bag with | Full xs -> xs[0] | Empty -> 0",
        "f",
        1,
    );
}

#[test]
fn copies_all_bindings_of_a_guarded_arm_until_the_guard_passes() {
    allocations(
        r"def f :: Maybe<[i64]> -> i64 = \m ->
    match m with
    | Maybe.Some xs when xs[0] > 100 -> xs[1]
    | Maybe.Some xs -> xs[0] + xs[1]
    | Maybe.None -> 0",
        "f",
        0,
    );
}

#[test]
fn borrowed_and_foreach_sources_are_never_transferred() {
    allocations(
        r"def f :: [[i64]] -> i64 = \rows ->
    let mut total = 0
    for row in rows do
        let kept = row
        total = total + kept[0]
    total",
        "f",
        1,
    );
}

#[test]
fn pm10_b1_b2_b3_b4_borrow_lifetimes() {
    // B1: borrow that ends at a direct call allows a later last-use move (0 copies).
    allocations(
        r"def f :: [i64] -> [i64] = \xs -> { let _n = Array.length (ref xs); xs }",
        "f",
        0,
    );
    // B1 with slice write: only the initial `new` allocates (1 allocation).
    allocations(
        r"def f :: i64 -> [i64] = \n -> { let mut out = new [i64](n, \i -> i); Array.write 0 7 (ref mut out[0..]); out }",
        "f",
        1,
    );
    // B2: consuming read in a later argument of the SAME call copies because the earlier borrow is live at the call.
    allocations(
        r"def pair_sum :: ref [i64] -> [i64] -> i64 = \r ys -> r[0] + ys[0]
def f :: [i64] -> i64 = \xs -> pair_sum (ref xs) (Array.__set 0 9 xs)",
        "f",
        1,
    );
    // B3: local window bound with `let` whose only uses are B1 calls ends before the root's final move.
    allocations(
        r"record View { r } { data: ref { r } [i64] }
def make_view { r } :: ref { r } [i64] -> View { r } = \xs -> View { data: xs }
def read_view :: ref View -> i64 = \v -> v.data[0]
def f :: [i64] -> [i64] = \xs -> { let v = make_view (ref xs); let _x = read_view (ref v); xs }",
        "f",
        0,
    );
    // B3 negative: a window with a user Drop runs at scope exit, so the root must still copy.
    allocations(
        r"extern def sink :: i64 -> unit
record Guard { r } { data: ref { r } [i64] }
instance Drop<Guard> { fn drop g = sink g.data[0] }
def make_guard { r } :: ref { r } [i64] -> Guard { r } = \xs -> Guard { data: xs }
def f :: [i64] -> [i64] = \xs -> { let _g = make_guard (ref xs); Array.__set 0 9 xs }",
        "f",
        1,
    );
    // B4 negative: stashing a borrow through an owned record holding `ref mut` or through an exclusive slice keeps the copy.
    allocations(
        r"record View { r } { data: ref { r } [i64] }
record Slot { q r } { dst: ref mut { q } View { r } }
def stash { q r } :: ref { r } [i64] -> Slot { q r } -> unit = \src slot -> *slot.dst = View { data: src }
def f :: [i64] -> i64 = \xs ->
    let fallback = [0, 0]
    let mut v = View { data: ref fallback }
    let slot = Slot { dst: ref mut v }
    stash (ref xs) slot
    let ys = Array.__set 0 9 xs
    v.data[0] + ys[0]",
        "f",
        1,
    );
    allocations(
        r"record View { r } { data: ref { r } [i64] }
def put { r } :: ref { r } [i64] -> ref mut [View { r }..] -> unit = \src views -> Array.write 0 (View { data: src }) views
def f { r } :: [i64] -> [View { r }] -> i64 = \xs init_views ->
    let mut views = init_views
    put (ref xs) (ref mut views[0..])
    let ys = Array.__set 0 9 xs
    views[0].data[0] + ys[0]",
        "f",
        1,
    );
    // Edition 2027+ A39 field exclusive borrow followed by returning the Copy record moves `b` (0 copies).
    let ed27 = tsuzuri::analyze_with_edition(
        "record Boxed { xs: [i64] }\ndef f :: own Boxed -> Boxed = \\b -> { let mut m = b; Array.write 0 7 (ref mut m.xs[0..]); m }\n0",
        tsuzuri::syntax::Edition::E2027,
    )
    .unwrap();
    for wasm in [false, true] {
        let ir = llvm::emit_target(&ed27, llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            body(&ir, "f")
                .replace("call nonnull ptr @tz.alloc", "call ptr @tz.alloc")
                .matches("call ptr @tz.alloc")
                .count(),
            0
        );
    }
}
