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
fn user_defined_index_and_index_mut_work_with_temporary_and_borrowed_keys() {
    emits(
        "record Grid { data: [i64] }\n\
         instance Index<Grid, i64, i64> {\n\
             fn index k self = self.data[deref k]\n\
         }\n\
         instance Index<Grid, string, i64> {\n\
             fn index k self = self.data[k.length]\n\
         }\n\
         let g = Grid { data: [10, 20, 30, 40] }\n\
         let key = \"ab\"\n\
         g[1] + g[key] + g[\"abc\"] + key.length",
    );

    // Phase 2: IndexMut with user container (temporary + reusable non-Copy keys) and `Tensor.Mut` / `MatrixView.Mut`.
    emits(
        "let mut buf = [1, 2, 3, 4]\n\
         let mut tm = Tensor.borrow_mut [2, 2] (ref mut buf)\n\
         tm[[1, 0]] = 9\n\
         let t = Tensor.update (\\m -> m[[0, 1]] = 7) (Tensor.zeros [2, 2])\n\
         let mut mbuf = [1, 2, 3, 4]\n\
         let mut mm = MatrixView.of_array_mut 2 2 (ref mut mbuf)\n\
         mm[(0, 1)] = 5\n\
         t[[0, 1]] + buf[2] + mbuf[1]",
    );

    // Custom container IndexMut runtime dispatch & key borrowing (-O0 and -O3).
    let custom_mut = "\
record Grid { data: [i64] }
instance Index<Grid, string, i64> {
    fn index k self = self.data[k.length]
}
instance IndexMut<Grid, string, i64> {
    fn index_mut k v self =
        let mut arr = Array.copy (ref self.data)
        Array.write k.length v (ref mut arr)
        self.data = arr
}
def main :: unit -> i32 = \\() ->
    let mut g = Grid { data: [10, 20, 30, 40] }
    let key = \"a\"
    g[key] = 42
    g[\"abc\"] = 99
    let a = g[key]
    let b = g[\"abc\"]
    let c = g[\"\"]
    let d = g[\"ab\"]
    do! IO.writeln ($\"{a}:{b}:{c}:{d}:{key.length}\")
    0
";
    let dir = std::env::temp_dir().join(format!("tsuzuri-nc04-custom-mut-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(&main_tz, custom_mut).unwrap();
    for opt in ["-O0", "-O3"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["run", "--no-cache", opt])
            .arg(&main_tz)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "custom IndexMut {opt}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "42:99:10:30:1",
            "custom IndexMut {opt}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);

    // Conflicting access to container while borrowed (shared or exclusive) is rejected with E1014.
    let grid_defs = "\
record Grid { data: [i64] }
instance Index<Grid, string, i64> {
    fn index k self = self.data[k.length]
}
instance IndexMut<Grid, string, i64> {
    fn index_mut k v self =
        let mut arr = Array.copy (ref self.data)
        Array.write k.length v (ref mut arr)
        deref self = Grid { data: arr }
}
";
    rejects(
        &format!(
            "{grid_defs}\
             let mut g = Grid {{ data: [1, 2, 3] }}\n\
             let r = ref g\n\
             g[\"a\"] = 5\n\
             r.data[0]"
        ),
        "E1014",
    );
    rejects(
        &format!(
            "{grid_defs}\
             let mut g = Grid {{ data: [1, 2, 3] }}\n\
             let r = ref mut g\n\
             let _ = g[\"a\"]\n\
             deref r = Grid {{ data: [4, 5, 6] }}\n\
             0"
        ),
        "E1014",
    );
}

#[test]
fn std_collections_support_index_with_temporary_keys_and_borrowed_views() {
    // Tensor coordinate + Slice view indexing (temporary [Slice.Axis] key does not trigger E1013).
    emits(
        "let t = Tensor.init [4, 4] (\\i -> i)\n\
         let sub = t[[Slice.step 1 4 2, Slice.rev()]]\n\
         let elem = t[[2, 3]]\n\
         let v_elem = sub[[0, 1]]\n\
         elem + v_elem",
    );

    // Matrix, MatrixView, Map, HashMap indexing (non-Copy string keys are borrowed, not moved).
    emits(
        "let m = Matrix.of_array 2 3 [1, 2, 3, 4, 5, 6]\n\
         let mv = m[(Slice.range 0 2, Slice.every 2)]\n\
         let mp = Map.empty() |> Map.insert \"tea\" 3 |> Map.insert \"cake\" 5\n\
         let hm = HashMap.empty() |> HashMap.insert \"tea\" 7\n\
         let k = \"tea\"\n\
         m[(1, 2)] + mv[(0, 1)] + mp[k] + hm[k] + mp[\"cake\"] + k.length",
    );
}

#[test]
fn index_view_inherits_container_lifetime_and_rejects_key_escape() {
    // Returning a borrowed View from an Index call cannot outlive the container.
    let message = rejects(
        "def leak { r } :: ref { r } Tensor<i64> -> Tensor.View<i64> { r }\n\
         fn leak _outer =\n    \
         let local = Tensor.init [3, 3] (\\i -> i)\n    \
         local[[Slice.all(), Slice.rev()]]\n\
         0",
        "E1013",
    );
    assert!(message.contains("does not live long enough"), "{message}");

    // Moving the container while an indexed view is still live is E1014.
    let message = rejects(
        "let t = Tensor.init [3, 3] (\\i -> i)\n\
         let sub = t[[Slice.all(), Slice.rev()]]\n\
         let moved = t\n\
         sub[[0, 0]] + (Tensor.as_array (ref moved)).length",
        "E1014",
    );
    assert!(
        message.starts_with("access conflicts with a live borrow"),
        "{message}"
    );

    // An Index instance that tries to escape a borrow of the KEY instead of the container is E1013.
    rejects(
        "record Box { v: i64 }\n\
         instance Index<Box, string, ref string> {\n\
             fn index k _self = k\n\
         }\n\
         0",
        "E1013",
    );
}

#[test]
fn index_rejects_overlaps_builtin_overrides_missing_instances_and_class_name_collisions() {
    // Overlapping (Self, Key) pair is E1016.
    rejects(
        "record R { x: i64 }\n\
         instance Index<R, i64, i64> { fn index _k self = self.x }\n\
         instance Index<R, i64, bool> { fn index _k _self = true }\n\
         0",
        "E1016",
    );

    // Overriding a built-in indexable type is E1016.
    rejects(
        "instance Index<[i64], string, i64> { fn index _k _self = 0 }\n0",
        "E1016",
    );

    // Unsupported key on a multi-instance Index type gives E1005; single-instance hints E1003.
    rejects("let t = Tensor.of_array [2] [1, 2]\nt[\"bad\"]", "E1005");
    rejects("let m = Matrix.of_array 2 2 [1, 2, 3, 4]\nm[0]", "E1003");

    // Non-indexable type without Index instances gives E1005 (IndexNonIndexable).
    rejects("let x = 42\nx[0]", "E1005");

    // Reserved builtin class names `Index` and `IndexMut` cannot be redeclared as records.
    rejects("record Index { x: i64 }\n0", "E1001");
    rejects("record IndexMut { x: i64 }\n0", "E1001");
}

#[test]
fn index_view_on_a_temporary_base_is_rejected() {
    let check_temp_e1013 = |source: &str, expected_base_prefix: &str| {
        let error = analyze(source).expect_err(source).into_data();
        assert_eq!(error.code, "E1013", "{source}\n{}", error.message);
        assert_eq!(error.kind, Some("borrow_of_temporary"), "{source}");
        assert!(
            error.message.starts_with("borrow requires a local place"),
            "{source}\n{}",
            error.message
        );
        let help = error.help.as_deref().unwrap_or("");
        assert!(
            help.contains("bind the value with 'let' first"),
            "{source}\nhelp={help}"
        );
        assert!(
            source[error.span.start..].starts_with(expected_base_prefix)
                || source[error.span.start..].starts_with(&format!("({expected_base_prefix}")),
            "{source}\nspan starts at: {:?}",
            &source[error.span.start..]
        );
    };

    // Reproducer 1: temporary Tensor sliced into a View.
    check_temp_e1013(
        "def mk :: i64 -> Tensor<i64>\n\
         fn mk n = Tensor.init [4] (\\i -> i + n)\n\
         let v = (mk 100)[[Slice.range 1 3]]\n\
         Tensor.count (ref v)",
        "mk 100",
    );

    // Reproducer 3-a: first step of a chain on a temporary Tensor.
    check_temp_e1013(
        "def mk :: i64 -> Tensor<i64>\n\
         fn mk n = Tensor.init [2, 1] (\\i -> i + n)\n\
         let x = (mk 100)[[Slice.range 0 2]][[1, 0]]\n\
         x",
        "mk 100",
    );

    // Reproducer 3-b: temporary Matrix sliced into a MatrixView.
    check_temp_e1013(
        "def mk :: i64 -> Matrix<i64>\n\
         fn mk n = Matrix.init 3 3 (\\r c -> r * 3 + c + n)\n\
         let mv = (mk 100)[(Slice.range 1 3, Slice.all())]\n\
         mv[(0, 0)]",
        "mk 100",
    );

    // Reproducer 3-c: user Index returning `ref string` on a temporary.
    check_temp_e1013(
        "record Grid { names: [string] }\n\
         instance Index<Grid, i64, ref string> {\n\
             fn index k self = ref self.names[deref k]\n\
         }\n\
         def mk :: i64 -> Grid\n\
         fn mk n = Grid { names: Array.init 3 (\\i -> $\"name-{i + n}\") }\n\
         let s = (mk 100)[1]\n\
         s.length",
        "mk 100",
    );

    // Reproducer 3-d: region-bearing user record borrowing its own field on a temporary.
    check_temp_e1013(
        "record R { r } { own_data: [i64], b: ref { r } [i64] }\n\
         instance Index<R, i64, R> {\n\
             fn index _k x = R { own_data: [], b: ref x.own_data[0..] }\n\
         }\n\
         def mk { r } :: i64 -> ref { r } [i64] -> R { r }\n\
         fn mk n e = R { own_data: Array.init 6 (\\i -> i + n), b: e }\n\
         let e = [1, 2, 3]\n\
         let z = (mk 100 (ref e))[0]\n\
         z.b[0]",
        "mk 100",
    );

    // Reproducer 3-g: region-less Output carrying a reference (`(ref string * i64)`).
    check_temp_e1013(
        "record Grid { names: [string] }\n\
         instance Index<Grid, i64, (ref string * i64)> {\n\
             fn index k self = (ref self.names[deref k], deref k)\n\
         }\n\
         def mk :: i64 -> Grid\n\
         fn mk n = Grid { names: Array.init 3 (\\i -> $\"name-{i + n}\") }\n\
         let p = (mk 100)[1]\n\
         match p with | (s, i) -> s.length + i",
        "mk 100",
    );

    // Reproducer 3-h: region-less Output carrying a closure (`unit -> i64`).
    check_temp_e1013(
        "record Grid { names: [string] }\n\
         instance Index<Grid, i64, (unit -> i64)> {\n\
             fn index k self =\n\
                 let i = deref k\n\
                 let f = \\() -> String.length self.names[i]\n\
                 f\n\
         }\n\
         def mk :: i64 -> Grid\n\
         fn mk n = Grid { names: Array.init 3 (\\i -> $\"name-{i + n}\") }\n\
         let f = (mk 100)[1]\n\
         f ()",
        "mk 100",
    );
}

#[test]
fn index_view_keeps_a_loan_of_its_base_place() {
    // Reproducer 2: Tensor.Mut indexed with a user key into a View keeps a shared loan of `m`.
    rejects(
        "record Key { k: i64 }\n\
         instance Index<Tensor.Mut<i64>, Key, Tensor.View<i64>> {\n\
             fn index _k self = Tensor.freeze self\n\
         }\n\
         let mut data = [1, 2, 3, 4]\n\
         let mut m = Tensor.borrow_mut [4] (ref mut data)\n\
         let v = m[Key { k: 0 }]\n\
         Tensor.write [0] 99 (ref mut m)\n\
         Tensor.count (ref v)",
        "E1014",
    );

    // Reproducer 3-e: replacing a local region-bearing base while its view is live is E1014.
    rejects(
        "record R { r } { own_data: [i64], b: ref { r } [i64] }\n\
         instance Index<R, i64, R> {\n\
             fn index _k x = R { own_data: [], b: ref x.own_data[0..] }\n\
         }\n\
         def mk { r } :: i64 -> ref { r } [i64] -> R { r }\n\
         fn mk n e = R { own_data: Array.init 6 (\\i -> i + n), b: e }\n\
         let e = [1, 2, 3]\n\
         let mut base = mk 100 (ref e)\n\
         let z = base[0]\n\
         base = mk 900 (ref e)\n\
         z.b[0] + base.own_data[0]",
        "E1014",
    );

    // Reproducer 3-f: overlapping view and `ref mut m` in a single call is E1014.
    rejects(
        "record Key { k: i64 }\n\
         instance Index<Tensor.Mut<i64>, Key, Tensor.View<i64>> {\n\
             fn index _k self = Tensor.freeze self\n\
         }\n\
         def overlap_copy :: Tensor.View<i64> -> ref mut Tensor.Mut<i64> -> unit\n\
         fn overlap_copy src dst =\n\
             let part = Tensor.narrow 0 0 4 (ref src)\n\
             let mut tail = Tensor.narrow_mut 0 1 4 dst\n\
             Tensor.copy_from (ref part) (ref mut tail)\n\
         let mut data = [1, 2, 3, 4, 5]\n\
         let mut m = Tensor.borrow_mut [5] (ref mut data)\n\
         overlap_copy m[Key { k: 0 }] (ref mut m)\n\
         data[0]",
        "E1014",
    );

    // D2: moving a local View while a sub-view taken from it is still live is E1014.
    rejects(
        "let t = Tensor.init [4] (\\i -> i)\n\
         let v = Tensor.view (ref t)\n\
         let s = v[[Slice.at 0]]\n\
         let moved = v\n\
         Tensor.count (ref s) + Tensor.count (ref moved)",
        "E1014",
    );
}

#[test]
fn std_view_chains_on_temporary_views_stay_accepted() {
    // Chained Tensor.View and MatrixView slices on temporary views, value reads on temporaries,
    // and reference parameters.
    emits(
        "def mk :: i64 -> Tensor<i64>\n\
         fn mk n = Tensor.init [4] (\\i -> i + n)\n\
         def mm :: i64 -> Matrix<i64>\n\
         fn mm n = Matrix.init 2 2 (\\r c -> r * 2 + c + n)\n\
         def from_ref { r } :: ref { r } Tensor<i64> -> Tensor.View<i64> { r }\n\
         fn from_ref r = r[[Slice.at 0]]\n\
         let t = Tensor.init [4, 3] (\\i -> i + 100)\n\
         let row = t[[Slice.range 1 3]][[Slice.at 1]]\n\
         let elem = t[[Slice.range 1 3]][[1, 2]]\n\
         let m = Matrix.init 3 3 (\\r c -> r * 3 + c + 100)\n\
         let mv = m[(Slice.range 1 3, Slice.all())][(Slice.all(), Slice.at 2)]\n\
         let r_view = from_ref (ref t)\n\
         row[[0]] + elem + mv[(0, 0)] + (mk 100)[[2]] + (mm 100)[(1, 1)] + r_view[[0]]",
    );

    // Chained slices inside generic functions (`infer_copy_all` + monomorphized check).
    emits(
        "def first_row { r } :: ref Tensor.View<'a> { r } -> Tensor.View<'a> { r }\n\
         fn first_row v = v[[Slice.range 0 1]][[Slice.at 0]]\n\
         def corner :: Copy<'a> => ref Tensor<'a> -> 'a\n\
         fn corner t = t[[Slice.range 1 3]][[0, 0]]\n\
         let t = Tensor.init [4, 3] (\\i -> i + 100)\n\
         let v = Tensor.view (ref t)\n\
         let row = first_row (ref v)\n\
         $\"{Tensor.to_array (Tensor.to_tensor (ref row))} {corner (ref t)}\"",
    );
}

#[test]
fn index_instances_on_std_types_need_a_key_of_this_program() {
    let rejects_orphan = |source: &str| {
        let error = analyze(source).expect_err(source).into_data();
        assert_eq!(error.code, "E1016", "{source}\n{}", error.message);
        assert_eq!(error.kind, Some("orphan_instance"), "{source}");
        assert!(
            error
                .message
                .ends_with("needs a type declared in this program as its self or key type"),
            "{source}\n{}",
            error.message
        );
        assert!(
            source[error.span.start..].starts_with("Index"),
            "{source}\nspan={:?}",
            &source[error.span.start..]
        );
    };

    rejects_orphan(
        "instance Index<Tensor.Mut<i64>, [Slice.Axis], Tensor.View<i64>> {\n\
             fn index _axes self = Tensor.freeze self\n\
         }\n0",
    );
    rejects_orphan(
        "instance IndexMut<Tensor<i64>, i64, i64> {\n\
             fn index_mut _k _v _self = ()\n\
         }\n0",
    );
    rejects_orphan(
        "instance Index<(i64 * i64), i64, i64> {\n\
             fn index _k _self = 0\n\
         }\n0",
    );

    // User Key type on std Tensor is accepted.
    emits(
        "record Key { i: i64 }\n\
         instance Copy<'a> => Index<Tensor<'a>, Key, 'a> {\n\
             fn index k self =\n\
                 let v = Tensor.view self\n\
                 deref (Tensor.at [k.i] (ref v))\n\
         }\n\
         let t = Tensor.of_array [3] [10, 20, 30]\n\
         t[Key { i: 1 }]",
    );

    // Built-in indexable override keeps `builtin_instance_override`.
    let builtin_err = analyze("instance Index<[i64], string, i64> { fn index _k _self = 0 }\n0")
        .unwrap_err()
        .into_data();
    assert_eq!(builtin_err.code, "E1016");
    assert_eq!(builtin_err.kind, Some("builtin_instance_override"));

    // Overlap between user instance and `std/Map.tz` reports at the user's `Index` span with
    // `declared_here` pointing to `std/Map.tz`.
    let map_src = "record K { v: i64 }\n\
                   instance Index<Map<K, i64>, K, i64> {\n\
                       fn index _k _self = 0\n\
                   }\n0";
    let overlap_err = analyze(map_src).unwrap_err().into_data();
    assert_eq!(overlap_err.code, "E1016");
    assert_eq!(overlap_err.kind, Some("overlapping_instance"));
    assert!(
        map_src
            .get(overlap_err.span.start..)
            .is_some_and(|tail| tail.starts_with("Index<Map<K, i64>")),
        "expected user span, got {:?}",
        overlap_err.span
    );
    assert!(
        overlap_err
            .related
            .iter()
            .any(|rel| rel.role == tsuzuri::diagnostic::RelatedRole::DeclaredHere),
        "expected DeclaredHere related note: {:?}",
        overlap_err.related
    );
}

fn run_main(tag: &str, source: &str) -> String {
    let dir = std::env::temp_dir().join(format!("tsuzuri-a38-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main_tz = dir.join("Main.tz");
    std::fs::write(&main_tz, source).unwrap();
    let mut first = None;
    for opt in ["-O0", "-O3"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
            .args(["run", "--no-cache", opt])
            .arg(&main_tz)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{tag} {opt}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        if let Some(prev) = &first {
            assert_eq!(&text, prev, "{tag}: -O0 vs -O3 mismatch");
        } else {
            first = Some(text);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    first.unwrap()
}

#[test]
fn index_evaluates_the_base_before_the_key() {
    let prog = "\
record Foo { v: i64 }
instance Index<Foo, i64, i64> {
    fn index k self = self.v + deref k
}
def say :: string -> i64 -> i64 = \\label val ->
    do! IO.writeln ($\"{label}\")
    val
def mk :: unit -> Foo = \\() ->
    do! IO.writeln \"base\"
    Foo { v: 100 }
def arr :: unit -> [i64] = \\() ->
    do! IO.writeln \"arr\"
    [10, 20, 30]
def pick { r } :: ref mut { r } Foo -> ref mut { r } Foo = \\foo ->
    do! IO.writeln \"pick\"
    foo
def say_range :: i64 -> i64 -> Slice.Axis = \\a b ->
    do! IO.writeln \"slice1\"
    Slice.range a b
def say_slice :: i64 -> Slice.Axis = \\i ->
    do! IO.writeln \"slice2\"
    Slice.at i
def main :: unit -> i32 = \\() ->
    let a = (mk ())[say \"key\" 1]
    let mut foo = Foo { v: 200 }
    let b = (pick (ref mut foo))[say \"pkey\" 2]
    let t = Tensor.init [3, 2] (\\i -> i + 10)
    let sub = t[[say_range 0 2]][[say_slice 1]]
    let c = (arr ())[say \"akey\" 1]
    do! IO.writeln ($\"{a}:{b}:{sub[[0]]}:{c}\")
    0
";
    let out = run_main("eval-read", prog);
    assert_eq!(
        out.trim(),
        "base\nkey\npick\npkey\nslice1\nslice2\narr\nakey\n101:202:12:20"
    );
}

#[test]
fn index_mut_evaluates_the_right_side_first() {
    let prog = "\
record Foo { v: i64 }
instance IndexMut<Foo, i64, i64> {
    fn index_mut k x self =
        do! IO.writeln \"body\"
        deref self = Foo { v: (deref k) * 100 + x }
}
def say :: string -> i64 -> i64 = \\label val ->
    do! IO.writeln ($\"{label}\")
    val
def pick { r } :: ref mut { r } Foo -> ref mut { r } Foo = \\foo ->
    do! IO.writeln \"pick\"
    foo
def pick_array { r } :: ref mut { r } [i64] -> ref mut { r } [i64] = \\xs ->
    do! IO.writeln \"pick_arr\"
    xs
def main :: unit -> i32 = \\() ->
    let mut foo = Foo { v: 0 }
    (pick (ref mut foo))[say \"key\" 1] = say \"rhs\" 7
    let v1 = foo.v
    let mut xs = [1, 2, 3]
    (deref (pick_array (ref mut xs)))[say \"akey\" 1] = say \"arhs\" 9
    foo[0] = say \"const_key_rhs\" 5
    let v2 = foo.v
    foo[say \"const_rhs_key\" 2] = 8
    let v3 = foo.v
    let mut i = 0
    foo[i] = { i = 2; 7 }
    let v4 = foo.v
    do! IO.writeln ($\"{v1}:{xs[1]}:{v2}:{v3}:{v4}\")
    0
";
    let out = run_main("eval-write", prog);
    assert_eq!(
        out.trim(),
        "rhs\npick\nkey\nbody\narhs\npick_arr\nakey\nconst_key_rhs\nbody\nconst_rhs_key\nbody\nbody\n107:9:5:208:207"
    );

    // Moving local in RHS read by key becomes E1012 (`use_after_move`), matching built-in `xs[s.length] = s`.
    let check_e1012 = |source: &str, key_snip: &str| {
        let err = analyze(source).expect_err(source).into_data();
        assert_eq!(err.code, "E1012", "{source}\n{}", err.message);
        assert_eq!(err.kind, Some("use_after_move"), "{source}");
        assert!(
            source[err.span.start..].starts_with(key_snip),
            "{source}\nspan={:?}",
            &source[err.span.start..]
        );
        assert!(
            err.related
                .iter()
                .any(|r| r.role == tsuzuri::diagnostic::RelatedRole::MovedHere),
            "{source}\nrelated={:?}",
            err.related
        );
    };

    check_e1012(
        "record Boxed { items: [string] }\n\
         instance IndexMut<Boxed, i64, string> {\n\
             fn index_mut _k _v _self = ()\n\
         }\n\
         let mut b = Boxed { items: [] }\n\
         let s = \"abc\"\n\
         b[s.length] = s\n\
         0",
        "s.length",
    );

    check_e1012(
        "record Pair { n: i64, tag: string }\n\
         record PairBox { v: i64 }\n\
         instance IndexMut<PairBox, i64, Pair> {\n\
             fn index_mut _k _v _self = ()\n\
         }\n\
         let mut b = PairBox { v: 0 }\n\
         let x = Pair { n: 1, tag: \"hi\" }\n\
         b[x.n] = x\n\
         0",
        "x.n]",
    );

    check_e1012(
        "record Pair { n: i64, tag: string }\n\
         record PairBox { v: i64 }\n\
         instance IndexMut<PairBox, (i64 * i64), Pair> {\n\
             fn index_mut _k _v _self = ()\n\
         }\n\
         let mut b = PairBox { v: 0 }\n\
         let x = Pair { n: 1, tag: \"hi\" }\n\
         b[(x.n, 0)] = x\n\
         0",
        "x.n, 0",
    );

    // Key tuple moving `x` while RHS reads `x.n` is now accepted because RHS runs first!
    emits(
        "record Pair { n: i64, tag: string }\n\
         record IBox { v: i64 }\n\
         instance IndexMut<IBox, (Pair * i64), i64> {\n\
             fn index_mut _k x self = deref self = IBox { v: x }\n\
         }\n\
         let mut b = IBox { v: 0 }\n\
         let x = Pair { n: 42, tag: \"ok\" }\n\
         b[(x, 0)] = x.n\n\
         b.v",
    );
}

#[test]
fn index_key_reassigning_a_mutable_base_reference() {
    let prog = "\
record Foo { v: i64 }
instance Index<Foo, i64, i64> {
    fn index k self = self.v + deref k
}
instance IndexMut<Foo, i64, i64> {
    fn index_mut k x self = deref self = Foo { v: (deref k) * 10 + x }
}
def main :: unit -> i32 = \\() ->
    let a = Foo { v: 1 }
    let b = Foo { v: 7 }
    let mut r = ref a
    let read_val = (deref r)[{ r = ref b; 0 }]
    let mut m = Foo { v: 2 }
    let mut rm = ref mut m
    (deref rm)[rm.v - 1] = 5
    do! IO.writeln ($\"{read_val}:{r.v}:{m.v}\")
    0
";
    let out = run_main("d9-mut-ref", prog);
    assert_eq!(out.trim(), "1:7:15");

    // D9: reassigning `r` inside the key of `IndexMut` is E1014 (`borrow_conflict`).
    let err = analyze(
        "record Foo { v: i64 }\n\
         instance IndexMut<Foo, i64, i64> {\n\
             fn index_mut _k x self = deref self = Foo { v: x }\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         let mut other = Foo { v: 9 }\n\
         let mut r = ref mut foo\n\
         (deref r)[{ r = ref mut other; 0 }] = 5\n\
         foo.v + other.v",
    )
    .unwrap_err()
    .into_data();
    assert_eq!(err.code, "E1014");
    assert_eq!(err.kind, Some("borrow_conflict"));
}

#[test]
fn index_key_cannot_write_its_base() {
    let check_e1014 = |source: &str, key_access: &str| {
        let err = analyze(source).expect_err(source).into_data();
        assert_eq!(err.code, "E1014", "{source}\n{}", err.message);
        assert_eq!(err.kind, Some("borrow_conflict"), "{source}");
        assert!(
            source[err.span.start..].starts_with(key_access),
            "{source}\nspan={:?}",
            &source[err.span.start..]
        );
        assert!(
            err.related
                .iter()
                .any(|r| r.role == tsuzuri::diagnostic::RelatedRole::BorrowedHere),
            "{source}\nrelated={:?}",
            err.related
        );
    };

    // 1. Read key assigns to base.
    check_e1014(
        "record Foo { v: i64 }\n\
         instance Index<Foo, i64, i64> {\n\
             fn index k self = self.v + deref k\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         foo[{ foo = Foo { v: 5 }; 0 }]",
        "foo = Foo",
    );

    // 2. Read key exclusively borrows base.
    check_e1014(
        r#"record Foo { v: i64 }
instance Index<Foo, i64, i64> {
    fn index k self = self.v + deref k
}
def bump :: ref mut Foo -> i64 = \f ->
    deref f = Foo { v: 50 }
    0
let mut foo = Foo { v: 1 }
foo[bump (ref mut foo)]"#,
        "(ref mut foo)",
    );

    // 3. Read key moves non-Copy base.
    check_e1014(
        "record Foo { s: string }\n\
         instance Index<Foo, i64, i64> {\n\
             fn index k self = self.s.length + deref k\n\
         }\n\
         def consume :: own Foo -> i64 = \\f -> f.s.length\n\
         let foo = Foo { s: \"abc\" }\n\
         foo[consume foo]",
        "foo]",
    );

    // 4. Write target key assigns to base.
    check_e1014(
        "record Foo { v: i64 }\n\
         instance IndexMut<Foo, i64, i64> {\n\
             fn index_mut _k x self = deref self = Foo { v: x }\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         foo[{ foo = Foo { v: 5 }; 0 }] = 7\n\
         foo.v",
        "foo = Foo",
    );

    // 5. Target is `ref mut Foo` value (`let r = ref mut foo`) and key assigns `*r` (write & read).
    check_e1014(
        "record Foo { v: i64 }\n\
         instance IndexMut<Foo, i64, i64> {\n\
             fn index_mut _k x self = deref self = Foo { v: self.v * 100 + x }\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         let r = ref mut foo\n\
         r[{ deref r = Foo { v: 5 }; 0 }] = 7\n\
         foo.v",
        "deref r = Foo",
    );

    check_e1014(
        "record Foo { v: i64 }\n\
         instance Index<Foo, i64, i64> {\n\
             fn index k self = self.v + deref k\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         let r = ref mut foo\n\
         r[{ deref r = Foo { v: 5 }; 0 }]",
        "deref r = Foo",
    );
}

#[test]
fn index_key_may_read_its_base() {
    emits(
        "record Foo { v: i64 }\n\
         instance Index<Foo, i64, i64> {\n\
             fn index k self = self.v + deref k\n\
         }\n\
         instance IndexMut<Foo, i64, i64> {\n\
             fn index_mut k x self = deref self = Foo { v: (deref k) + x }\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         let a = foo[foo.v]\n\
         foo[foo.v - 1] = 3\n\
         let t = Tensor.init [4] (\\i -> i + 10)\n\
         let last = t[[Tensor.count (Tensor.view (ref t)) - 1]]\n\
         a + foo.v + last",
    );

    // Direct place borrow of `foo.v` in `foo[foo.v] = 3` still lives through the call and conflicts
    // with the exclusive borrow of `foo`.
    rejects(
        "record Foo { v: i64 }\n\
         instance IndexMut<Foo, i64, i64> {\n\
             fn index_mut _k x self = deref self = Foo { v: x }\n\
         }\n\
         let mut foo = Foo { v: 1 }\n\
         foo[foo.v] = 3\n\
         foo.v",
        "E1014",
    );
}

#[test]
fn index_temporary_base_is_dropped_on_early_exit_in_key() {
    emits(
        r#"record Foo { tag: string }
instance Index<Foo, i64, i64> {
    fn index k self = self.tag.length + deref k
}
def mk :: i64 -> Foo
fn mk n = Foo { tag: $"item-{n}" }
def probe :: i64 -> i64 -> i64
fn probe n k =
    let huge = 9223372036854775807
    match (try (mk n)[@checked (k + huge)] with | e -> e) with
    | Result.Ok v -> v
    | Result.Error _ -> -1
probe 5 1"#,
    );
}

#[test]
fn index_simple_base_keeps_the_ir() {
    use tsuzuri::check::TypedExprKind;
    fn has_generated_index_local(expr: &tsuzuri::check::TypedExpr) -> bool {
        match &expr.kind {
            TypedExprKind::Block { bindings, result } => {
                bindings.iter().any(|(local, val)| {
                    local.name.starts_with("$index.") || has_generated_index_local(val)
                }) || has_generated_index_local(result)
            }
            TypedExprKind::Call(callee, args) => {
                has_generated_index_local(callee) || args.iter().any(has_generated_index_local)
            }
            TypedExprKind::Binary(_, a, b)
            | TypedExprKind::Assign(a, b)
            | TypedExprKind::Index(a, b) => {
                has_generated_index_local(a) || has_generated_index_local(b)
            }
            TypedExprKind::Unary(_, a)
            | TypedExprKind::Field(a, _)
            | TypedExprKind::Borrow(a, _)
            | TypedExprKind::BorrowOperand(a)
            | TypedExprKind::Dereference(a) => has_generated_index_local(a),
            _ => false,
        }
    }

    let source = "\
record Foo { v: i64 }
record Holder { f: Foo }
instance Index<Foo, i64, i64> {
    fn index k self = self.v + deref k
}
instance IndexMut<Foo, i64, string> {
    fn index_mut _k _s _self = ()
}
def mk :: i64 -> Foo
fn mk n = Foo { v: n }
def check_simple :: Foo -> Holder -> ref Foo -> i64 -> string -> i64
fn check_simple a h r i s =
    let mut m = Foo { v: 1 }
    m[i] = s
    m[0] = $\"x-{i}\"
    a[i] + h.f[i] + r[i] + (mk i)[0]
check_simple (Foo { v: 1 }) (Holder { f: Foo { v: 2 } }) (ref (Foo { v: 3 })) 0 \"hi\"
";
    let module = analyze(source).unwrap();
    let func = module
        .functions
        .iter()
        .find(|f| f.name.ends_with("check_simple"))
        .unwrap();
    assert!(
        !has_generated_index_local(&func.body),
        "expected zero $index.* locals in check_simple, got body: {:?}",
        func.body
    );
}
