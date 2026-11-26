use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use tsuzuri::llvm;
use tsuzuri::syntax::Edition;

fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/scratch/multi-index")
        .join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
}

fn ir_of(source: &str, edition: Edition) -> String {
    let module = tsuzuri::analyze_with_edition(source, edition)
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(
        ir,
        llvm::emit(&module, llvm::Entry::Library).unwrap(),
        "IR must be deterministic"
    );
    ir
}

#[test]
fn multi_index_ir_matches_explicit_tuple_array_and_sel_forms() {
    for (short, long) in [
        // 2D Tensor element read `t[i, j]` == `t[(i, j)]`
        (
            "def f :: ref Tensor<i64> -> i64 -> i64 -> i64 = \\t i j -> t[i, j]\nlet t = Tensor.init [3, 4] (\\i -> i)\nf (ref t) 1 2\n",
            "def f :: ref Tensor<i64> -> i64 -> i64 -> i64 = \\t i j -> t[(i, j)]\nlet t = Tensor.init [3, 4] (\\i -> i)\nf (ref t) 1 2\n",
        ),
        // 2D Matrix element read `m[i, j]` == `m[(i, j)]`
        (
            "def f :: ref Matrix<i64> -> i64 -> i64 -> i64 = \\m i j -> m[i, j]\nlet m = Matrix.init 3 4 (\\r c -> r + c)\nf (ref m) 1 2\n",
            "def f :: ref Matrix<i64> -> i64 -> i64 -> i64 = \\m i j -> m[(i, j)]\nlet m = Matrix.init 3 4 (\\r c -> r + c)\nf (ref m) 1 2\n",
        ),
        // 5D Tensor element read `t[a, b, c, d, e]` == `t[[a, b, c, d, e]]`
        (
            "def f :: ref Tensor<i64> -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 = \\t a b c d e -> t[a, b, c, d, e]\nlet t = Tensor.init [2, 2, 2, 2, 2] (\\i -> i)\nf (ref t) 0 1 0 1 0\n",
            "def f :: ref Tensor<i64> -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 = \\t a b c d e -> t[[a, b, c, d, e]]\nlet t = Tensor.init [2, 2, 2, 2, 2] (\\i -> i)\nf (ref t) 0 1 0 1 0\n",
        ),
        // HashMap<(i32 * i32), i64> `h[3, 4]` == `h[(3, 4)]` without implicit integer widening
        (
            "def f :: ref HashMap<(i32 * i32), i64> -> i64 = \\h -> h[3, 4]\nlet h = HashMap.insert (3i32, 4i32) 9i64 (HashMap.empty())\nf (ref h)\n",
            "def f :: ref HashMap<(i32 * i32), i64> -> i64 = \\h -> h[(3, 4)]\nlet h = HashMap.insert (3i32, 4i32) 9i64 (HashMap.empty())\nf (ref h)\n",
        ),
        // Tensor window `ref t[1..<3, ^0, ..]` == explicit `[Slice.Sel]`
        (
            "def f :: ref Tensor<i64> -> i64 = \\t -> (ref t[1..<3, ^0, ..])[0, 0]\nlet t = Tensor.init [4, 2, 2] (\\i -> i)\nf (ref t)\n",
            "def f :: ref Tensor<i64> -> i64 = \\t -> (ref t[[Slice.Sel.Span (Slice.Bound.Included (Slice.Pos.Front 1), Slice.Bound.Excluded (Slice.Pos.Front 3), 1), Slice.Sel.Item (Slice.Pos.Back 0), Slice.Sel.Whole]])[0, 0]\nlet t = Tensor.init [4, 2, 2] (\\i -> i)\nf (ref t)\n",
        ),
        // Matrix window `ref m[0..1, 2]` == explicit `(Slice.Sel * Slice.Sel)`
        (
            "def f :: ref Matrix<i64> -> i64 = \\m -> MatrixView.rows (ref m[0..1, 2])\nlet m = Matrix.init 3 4 (\\r c -> r + c)\nf (ref m)\n",
            "def f :: ref Matrix<i64> -> i64 = \\m -> MatrixView.rows (ref m[(Slice.Sel.Span (Slice.Bound.Included (Slice.Pos.Front 0), Slice.Bound.Included (Slice.Pos.Front 1), 1), Slice.Sel.Item (Slice.Pos.Front 2))])\nlet m = Matrix.init 3 4 (\\r c -> r + c)\nf (ref m)\n",
        ),
        // 1-component Tensor window `ref t[1..3]` == explicit `[Slice.Sel]`
        (
            "def f :: ref Tensor<i64> -> i64 = \\t -> (ref t[1..3])[0]\nlet t = Tensor.init [5] (\\i -> i)\nf (ref t)\n",
            "def f :: ref Tensor<i64> -> i64 = \\t -> (ref t[[Slice.Sel.Span (Slice.Bound.Included (Slice.Pos.Front 1), Slice.Bound.Included (Slice.Pos.Front 3), 1)]])[0]\nlet t = Tensor.init [5] (\\i -> i)\nf (ref t)\n",
        ),
        // Assignment `mt[i, j] = v` == `mt[(i, j)] = v`
        (
            "def f :: ref mut Tensor.Mut<i64> -> i64 -> i64 -> i64 -> unit = \\mt i j v -> mt[i, j] = v\nlet mut buf = [0, 1, 2, 3]\nlet mut mt = Tensor.borrow_mut [2, 2] (ref mut buf)\nf (ref mut mt) 0 1 9\nbuf[1]\n",
            "def f :: ref mut Tensor.Mut<i64> -> i64 -> i64 -> i64 -> unit = \\mt i j v -> mt[(i, j)] = v\nlet mut buf = [0, 1, 2, 3]\nlet mut mt = Tensor.borrow_mut [2, 2] (ref mut buf)\nf (ref mut mt) 0 1 9\nbuf[1]\n",
        ),
    ] {
        assert_eq!(
            ir_of(short, Edition::E2028),
            ir_of(long, Edition::E2028),
            "IR mismatch for:\n{short}\nvs\n{long}"
        );
    }
}

#[test]
fn accepts_user_sel_instances_chained_windows_and_matrix_only_windows() {
    // User container with `[Slice.Sel]` window instance
    tsuzuri::analyze_with_edition(
        "record GridView { r } { data: ref { r } [i64], off: i64 }\n\
         record Grid { data: [i64] }\n\
         instance Index<Grid, [Slice.Sel], GridView> {\n\
             fn index sels self =\n\
                 match Slice.resolve_sel self.data.length (ref sels[0]) with\n\
                 | Slice.Resolved.Index p -> GridView { data: ref self.data[0..], off: p }\n\
                 | Slice.Resolved.Slice (s, _, _) -> GridView { data: ref self.data[0..], off: s }\n\
                 | Slice.Resolved.InsertAxis -> GridView { data: ref self.data[0..], off: 0 }\n\
         }\n\
         let g = Grid { data: [10, 20, 30, 40] }\n\
         let v = ref g[1..<3, ..]\n\
         v.data[v.off]\n",
        Edition::E2028,
    )
    .unwrap();

    // Chained View windows (`returns_contents`) and parenthesized window element read
    tsuzuri::analyze_with_edition(
        "let t = Tensor.init [4, 4] (\\i -> i)\n\
         let w = ref t[1..3, ..][0..1, 1..2]\n\
         let x = (ref t[1..3, ..])[0, 1]\n\
         w[0, 0] + x\n",
        Edition::E2028,
    )
    .unwrap();

    // D11: a program naming only `Matrix` can still take a bracket window `ref m[0..<2, 1]`
    // and Phase 2 `m[^0, 1]`, while a non-`Matrix` program keeps its loaded std modules unchanged.
    tsuzuri::analyze_with_edition(
        "let m = Matrix.of_array 3 4 [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]\n\
         let sub = ref m[0..<2, 1]\n\
         sub[0, 0] + m[^0, 1]\n",
        Edition::E2028,
    )
    .unwrap();

    let non_matrix_plain = tsuzuri::stdlib::sources_for(["let xs = [1, 2, 3]\nxs[0]\n"]);
    let non_matrix_slice = tsuzuri::stdlib::sources_for(["let xs = [1, 2, 3]\nref xs[0..<2]\n"]);
    assert_eq!(non_matrix_plain.len(), non_matrix_slice.len());
}

#[test]
fn diagnostics_match_specification_section_8() {
    let check_err =
        |src: &str, ed: Edition, code: &str, kind: &str| -> tsuzuri::diagnostic::DiagnosticData {
            let err = tsuzuri::analyze_with_edition(src, ed)
                .unwrap_err()
                .into_data();
            assert_eq!(err.code, code, "{src}\n{err:?}");
            assert_eq!(err.kind, Some(kind), "{src}\n{err:?}");
            err
        };

    // Trailing comma -> E0002 expected_token
    check_err(
        "let t = Tensor.zeros [2, 2]\nt[0,]\n",
        Edition::E2028,
        "E0002",
        "expected_token",
    );
    // Unborrowed range/whole window -> E0002 slice_must_be_borrowed
    let d = check_err(
        "let t = Tensor.zeros [2, 2]\nt[0..1, 0]\n",
        Edition::E2028,
        "E0002",
        "slice_must_be_borrowed",
    );
    assert!(d.message.contains("index windows must be borrows"), "{d:?}");
    // All-`..` key -> E0002 slice_without_bounds
    check_err(
        "let t = Tensor.zeros [2, 2]\nref t[.., ..]\n",
        Edition::E2028,
        "E0002",
        "slice_without_bounds",
    );
    // Builtin 6 types with 2+ components -> E1005 builtin_index_one_component
    for init in [
        "let c = [1, 2, 3]",
        "let c: [i64; 3] = [1, 2, 3]",
        "let c = [|1, 2, 3|]",
        "let c: Vec<i64> = Vec.empty()",
        "let c = \"abc\"",
        "let c = u8\"abc\"",
    ] {
        check_err(
            &format!("{init}\nc[0, 1]\n"),
            Edition::E2028,
            "E1005",
            "builtin_index_one_component",
        );
    }
    // Slice.Axis in new bracket key -> E1003 index_component_type
    check_err(
        "let t: Tensor<i64> = Tensor.zeros [2, 2]\nref t[Slice.all(), 0]\n",
        Edition::E2028,
        "E1003",
        "index_component_type",
    );
    // Mask mixed with range -> E1003 index_component_type
    check_err(
        "let t: Tensor<i64> = Tensor.zeros [2, 2]\nlet mask = Tensor.of_array [2, 2] [true, false, true, false]\nref t[mask, 0..<2]\n",
        Edition::E2028,
        "E1003",
        "index_component_type",
    );
    // Non-i64 range bound -> E1003 type_mismatch with help
    let d = check_err(
        "let t: Tensor<i64> = Tensor.zeros [4, 4]\nlet a: i32 = 1\nref t[a..2, 0]\n",
        Edition::E2028,
        "E1003",
        "type_mismatch",
    );
    assert!(d.help.as_deref().unwrap_or("").contains("as i64"), "{d:?}");
    // Map<string, i64> with 2 components -> E1005 no_instance
    check_err(
        "let m: Map<string, i64> = Map.empty()\nm[\"a\", \"b\"]\n",
        Edition::E2028,
        "E1005",
        "no_instance",
    );
    // Tensor element read with `i: i32` -> E1005 no_instance (no implicit integer widening)
    check_err(
        "let t: Tensor<i64> = Tensor.zeros [2, 2]\nlet i: i32 = 0\nt[i, 0]\n",
        Edition::E2028,
        "E1005",
        "no_instance",
    );
    // `ref t[i, j]` on value-returning element read -> E1003 index_ref_on_value with range help
    let d = check_err(
        "let t: Tensor<i64> = Tensor.zeros [2, 2]\nref t[0, 1]\n",
        Edition::E2028,
        "E1003",
        "index_ref_on_value",
    );
    assert!(
        d.help.as_deref().unwrap_or("").contains("ref t[i, ..]"),
        "{d:?}"
    );
    // Type-detected window (`Slice.rest()`) without `ref` -> E1042 index_view_requires_ref
    check_err(
        "let t: Tensor<i64> = Tensor.zeros [2, 2]\nt[Slice.rest(), 0]\n",
        Edition::E2028,
        "E1042",
        "index_view_requires_ref",
    );
    // `ref mut t[...]` before NC20 -> E1042 index_view_not_writable
    check_err(
        "let mut t: Tensor<i64> = Tensor.zeros [2, 2]\nref mut t[0..1, ..]\n",
        Edition::E2028,
        "E1042",
        "index_view_not_writable",
    );
    // Temporary base `ref (mk 1)[0, ..]` -> E1013
    let err = tsuzuri::analyze_with_edition(
        "def mk :: i64 -> Tensor<i64> = \\n -> Tensor.zeros [n, n]\n\
         let w = ref (mk 2)[0, ..]\n\
         w[0]\n",
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err.code, "E1013", "{err:?}");
    // Mutating `Tensor.Mut` while a shared window borrowed from it is live -> E1014
    let err = tsuzuri::analyze_with_edition(
        "let mut buf = [1, 2, 3, 4]\n\
         let mut m = Tensor.borrow_mut [2, 2] (ref mut buf)\n\
         let w = ref m[0, ..]\n\
         m[0, 0] = 99\n\
         w[0]\n",
        Edition::E2028,
    )
    .unwrap_err();
    assert_eq!(err.code, "E1014", "{err:?}");

    // Matrix `m[0]`: E1003 without MatrixView, E1005 with MatrixView, both with row help (§4).
    let d_only = check_err(
        "let m = Matrix.init 2 2 (\\_ _ -> 0)\nm[0]\n",
        Edition::E2028,
        "E1003",
        "type_mismatch",
    );
    assert_eq!(
        d_only.help.as_deref(),
        Some("write 'ref m[i, ..]' for a row"),
        "{d_only:?}"
    );
    let d_with_mv = check_err(
        "let _a = Slice.all()\nlet m = Matrix.init 2 2 (\\_ _ -> 0)\nm[0]\n",
        Edition::E2028,
        "E1005",
        "no_instance",
    );
    assert_eq!(
        d_with_mv.help.as_deref(),
        Some("write 'ref m[i, ..]' for a row")
    );
}

#[test]
fn negative_literal_warnings_follow_d15() {
    // Range bound negative literal -> W1014 negative_literal_range_bound
    let m = tsuzuri::analyze_with_edition(
        "let t: Tensor<i64> = Tensor.zeros [4, 4]\nlet w = ref t[..-1, 0]\nw[0]\n",
        Edition::E2028,
    )
    .unwrap();
    assert_eq!(m.warnings.len(), 1);
    assert_eq!(m.warnings[0].code, "W1014");
    assert_eq!(m.warnings[0].kind, Some("negative_literal_range_bound"));

    // Window integer component negative literal -> W1014 negative_literal_index in all editions
    let m = tsuzuri::analyze_with_edition(
        "let t: Tensor<i64> = Tensor.zeros [4, 4]\nlet w = ref t[-1, ..]\nw[0]\n",
        Edition::E2028,
    )
    .unwrap();
    assert_eq!(m.warnings.len(), 1);
    assert_eq!(m.warnings[0].code, "W1014");
    assert_eq!(m.warnings[0].kind, Some("negative_literal_index"));

    for ed in [Edition::E2026, Edition::E2027, Edition::E2028] {
        let m = tsuzuri::analyze_with_edition(
            "let t: Tensor<i64> = Tensor.zeros [4, 4]\nlet w = ref t[Slice.rest(), -1]\nw[0]\n",
            ed,
        )
        .unwrap();
        assert_eq!(m.warnings.len(), 1, "{ed:?}");
        assert_eq!(m.warnings[0].code, "W1014");
        assert_eq!(m.warnings[0].kind, Some("negative_literal_index"));
    }

    // Instance-independent: custom user types taking `[Slice.Sel]` and `(Slice.Sel * Slice.Sel)`
    let m = tsuzuri::analyze_with_edition(
        "record GV { r } { p: ref { r } [i64] }\n\
         record G1 { xs: [i64] }\n\
         record G2 { xs: [i64] }\n\
         instance Index<G1, [Slice.Sel], GV> { fn index _ self = GV { p: ref self.xs[0..] } }\n\
         instance Index<G2, (Slice.Sel * Slice.Sel), GV> { fn index _ self = GV { p: ref self.xs[0..] } }\n\
         let g1 = G1 { xs: [1, 2] }\n\
         let g2 = G2 { xs: [1, 2] }\n\
         let v1 = ref g1[-1, ..]\n\
         let v2 = ref g2[Slice.rest(), -1]\n\
         v1.p[0] + v2.p[0]\n",
        Edition::E2028,
    )
    .unwrap();
    assert_eq!(m.warnings.len(), 2);
    assert!(
        m.warnings
            .iter()
            .all(|w| w.kind == Some("negative_literal_index"))
    );

    // Element mode keys (`t[-1, 0]`, `m[-1, 0]`, `h[-1, 0]`), `0 - 1`, variables, `-0`, `-0x0` do NOT warn!
    let clean = tsuzuri::analyze_with_edition(
        "let t: Tensor<f64> = Tensor.zeros [4, 4]\n\
         let m = Matrix.init 4 4 (\\_ _ -> 0.0)\n\
         let h = HashMap.insert (-1i64, 0i64) 3.0 (HashMap.empty())\n\
         let neg = -1\n\
         let w1 = ref t[0 - 1, ..]\n\
         let w2 = ref t[neg, ..]\n\
         let w3 = ref t[-0, -0x0..1]\n\
         t[-1, 0] + m[-1, 0] + h[-1, 0] + w1[0] + w2[0] + w3[0]\n",
        Edition::E2028,
    )
    .unwrap();
    assert!(clean.warnings.is_empty(), "{:?}", clean.warnings);
}

#[test]
fn a38_base_loan_guards_multi_axis_components() {
    let header = "\
record Grid { w: i64, data: [i64] }
instance Index<Grid, (i64 * i64), i64> {
    fn index k self =
        match k with
        | (r, c) -> self.data[r * self.w + c]
}
";
    // Reading base inside component is allowed (`g[g.w - 1, 0]`)
    tsuzuri::analyze_with_edition(
        &format!("{header}let g = Grid {{ w: 2, data: [10, 20, 30, 40] }}\ng[g.w - 1, 0]\n"),
        Edition::E2028,
    )
    .unwrap();

    // Assigning to base inside component -> E1014 borrow_conflict
    let err = tsuzuri::analyze_with_edition(
        &format!(
            "{header}let mut g = Grid {{ w: 2, data: [10, 20, 30, 40] }}\n\
             g[{{ g = Grid {{ w: 1, data: [0] }}; 0 }}, 0]\n"
        ),
        Edition::E2028,
    )
    .unwrap_err()
    .into_data();
    assert_eq!(err.code, "E1014");
    assert_eq!(err.kind, Some("borrow_conflict"));

    // Exclusively borrowing base inside component -> E1014 borrow_conflict
    let err = tsuzuri::analyze_with_edition(
        &format!(
            "{header}def touch :: ref mut Grid -> i64 = \\_ -> 0\n\
             let mut g = Grid {{ w: 2, data: [10, 20, 30, 40] }}\n\
             g[touch (ref mut g), 0]\n"
        ),
        Edition::E2028,
    )
    .unwrap_err()
    .into_data();
    assert_eq!(err.code, "E1014");
    assert_eq!(err.kind, Some("borrow_conflict"));
}

#[test]
fn allocation_counts_and_resolve_sel_edge_cases_run_cleanly() {
    let root = scratch_dir("alloc-and-sel");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Tsuzuri.toml"),
        "[package]\nname = \"alloc-and-sel\"\nversion = \"0.1.0\"\nedition = \"2028\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Main.tz"),
        "\
test \"element_reads_allocate_zero_and_windows_at_most_two\" =
    let t = Tensor.init [3, 4] (\\i -> i)
    let mark0 = Alloc.snapshot ()
    let v = t[1, 2] + t[^0, ^1]
    let d0 = Alloc.since mark0
    Test.equal 16 v
    Test.equal 0 d0.allocs

    let mark1 = Alloc.snapshot ()
    let w = ref t[0..1, 1..<3]
    let d1 = Alloc.since mark1
    Test.equal 1 w[0, 0]
    Test.equal 2 d1.allocs

test \"resolve_sel_and_empty_axes\" =
    let empty: Tensor<i64> = Tensor.zeros [0, 4]
    let w = ref empty[..<0, 1..2]
    let sh = Tensor.shape (ref w)
    assert (sh[0] == 0 && sh[1] == 2)

    let s1 = Slice.Sel.Span (Slice.Bound.Unbounded, Slice.Bound.Unbounded, -2)
    assert (Slice.resolve_sel 5 (ref s1) == Slice.Resolved.Slice (4, 3, -2))
    let s2 = Slice.Sel.Span (Slice.Bound.Included (Slice.Pos.Front 2), Slice.Bound.Included (Slice.Pos.Front 1), 1)
    assert (Slice.resolve_sel 5 (ref s2) == Slice.Resolved.Slice (2, 0, 1))
    let s3 = Slice.Sel.Span (Slice.Bound.Included (Slice.Pos.Front 1), Slice.Bound.Included (Slice.Pos.Front 2), -1)
    assert (Slice.resolve_sel 5 (ref s3) == Slice.Resolved.Slice (1, 0, -1))
",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(["test", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn formatter_is_idempotent_on_multi_index() {
    let source = "\
def demo :: ref Tensor<i64> -> i64
fn demo t =
    let a = t[1, 2]
    let b = t[^0, 1]
    let w = ref t[1 ..< 3, ^0, ..]
    let s = ref t[0 .. 2 .. 4, Slice.rest()]
    a + b + w[0, 0] + s[0, 0]
";
    use tsuzuri::formatter::format_source_with_edition;
    use tsuzuri::syntax::SourceKind;
    let once = format_source_with_edition("Main.tz", source, SourceKind::Code, Edition::E2028)
        .unwrap()
        .formatted;
    let twice = format_source_with_edition("Main.tz", &once, SourceKind::Code, Edition::E2028)
        .unwrap()
        .formatted;
    assert_eq!(once, twice);
}
