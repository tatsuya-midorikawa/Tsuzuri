use tsuzuri::{analyze, llvm};

fn rejects(source: &str, code: &str) -> String {
    let error = analyze(source).expect_err(source);
    assert_eq!(error.code, code, "{source}\n{}", error.message);
    error.into_data().message
}

fn emits(source: &str) -> [String; 2] {
    let module = analyze(source).unwrap_or_else(|error| {
        panic!("{source}\n{}: {}", error.code, error.message);
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

const VEC_BUF: &str = "\
record Vec2Buf { items: [i64] }
instance Add<Vec2Buf> {
    fn add left right = Vec2Buf { items: [left.items[0] + right.items[0], left.items[1] + right.items[1]] }
}
instance Sub<Vec2Buf> {
    fn sub left right = Vec2Buf { items: [left.items[0] - right.items[0], left.items[1] - right.items[1]] }
}
instance Mul<Vec2Buf> {
    fn mul left right = Vec2Buf { items: [left.items[0] * right.items[0], left.items[1] * right.items[1]] }
}
instance Div<Vec2Buf> {
    fn div left right = Vec2Buf { items: [left.items[0] / right.items[0], left.items[1] / right.items[1]] }
}
instance Rem<Vec2Buf> {
    fn rem left right = Vec2Buf { items: [left.items[0] % right.items[0], left.items[1] % right.items[1]] }
}
instance Pow<Vec2Buf> {
    fn pow left right = Vec2Buf { items: [left.items[0] ** right.items[0], left.items[1] ** right.items[1]] }
}
instance Neg<Vec2Buf> {
    fn neg value = Vec2Buf { items: [-value.items[0], -value.items[1]] }
}
";

#[test]
fn noncopy_operators_borrow_without_moving() {
    // BigInt operands stay live across repeated expressions.
    emits(
        "let a = 100000000000000000000I\n\
         let b = 200000000000000000000I\n\
         let c = a + b\n\
         let d = (a - b) * c\n\
         let e = d / b + (c % a) + (-a) + (2I ** 3I)\n\
         BigInt.compare (ref a) (ref b) + BigInt.compare (ref c) (ref e)",
    );

    // Non-Copy user record: repeated use, self-operation `a + a`, nested temporaries, direct `Add.add`.
    emits(&format!(
        "{VEC_BUF}\
         let a = Vec2Buf {{ items: [10, 20] }}\n\
         let b = Vec2Buf {{ items: [3, 4] }}\n\
         let c = a + b\n\
         let same = a + a\n\
         let nested = (a + b) * (a - b) + (c / b) + (c % b) + (-a) + (a ** b)\n\
         let direct = Add.add a b\n\
         let neg_direct = Neg.neg a\n\
         a.items[0] + b.items[0] + c.items[0] + same.items[0] + nested.items[0] + direct.items[0] + neg_direct.items[0]"
    ));

    // First-class method value and partial application still work on non-Copy types via adapter.
    emits(&format!(
        "{VEC_BUF}\
         let f: Vec2Buf -> Vec2Buf -> Vec2Buf = Add.add\n\
         let r = f (Vec2Buf {{ items: [1, 2] }}) (Vec2Buf {{ items: [3, 4] }})\n\
         let add_one = Add.add (Vec2Buf {{ items: [5, 6] }})\n\
         let r2 = add_one (Vec2Buf {{ items: [7, 8] }})\n\
         r.items[0] + r2.items[1]"
    ));
}

#[test]
fn generic_arithmetic_monomorphizes_for_scalars_and_noncopy_types() {
    emits(&format!(
        "{VEC_BUF}\
         def double_plus :: Add<'a> => own 'a -> own 'a -> 'a\n\
         fn double_plus x y = (x + y) + x\n\
         let v = double_plus (Vec2Buf {{ items: [1, 2] }}) (Vec2Buf {{ items: [3, 4] }})\n\
         let n = double_plus 10 20\n\
         let b = double_plus 100I 200I\n\
         v.items[0] + n + BigInt.compare (ref b) (ref 400I)"
    ));
}

#[test]
fn borrowed_arithmetic_enforces_borrow_conflicts() {
    // Mutating the left operand inside the right operand conflicts with its shared borrow.
    rejects(
        &format!(
            "{VEC_BUF}\
             let mut a = Vec2Buf {{ items: [1, 2] }}\n\
             let b = Vec2Buf {{ items: [3, 4] }}\n\
             let c = a + {{ a = b; a }}\n\
             c.items[0]"
        ),
        "E1014",
    );

    // Reading via `+` while an exclusive borrow is live is E1014.
    rejects(
        &format!(
            "{VEC_BUF}\
             let mut a = Vec2Buf {{ items: [1, 2] }}\n\
             let b = Vec2Buf {{ items: [3, 4] }}\n\
             let r = ref mut a\n\
             let c = a + b\n\
             r.items.length + c.items[0]"
        ),
        "E1014",
    );
}

#[test]
fn string_concat_keeps_consuming_contract_and_scalars_keep_direct_ir() {
    // `string + string` still consumes its operands (no UAF with @tz.string.append).
    rejects(
        "let s = \"hello\"\nlet t = s + \" world\"\ns.length + t.length",
        "E1012",
    );

    // Scalar `Add.add` still compiles with value type and PR02 D10 direct lowering.
    let [native_ir, _] = emits(
        "def scalar_op :: i64 -> i64 -> i64\n\
         fn scalar_op a b = Add.add a b\n\
         scalar_op 20 22",
    );
    let body = native_ir
        .split("define ")
        .find(|block| block.contains("@tz.fn.Main.scalar_op("))
        .expect("scalar_op emitted");
    assert!(
        !body.contains("@tz.fn.$intrinsic.Add.add"),
        "PR02 D10 direct lowering must remain active for scalar Add.add:\n{body}"
    );
}

#[test]
fn drop_type_arithmetic_drops_locals_at_scope_end_and_temporaries_at_expression_end() {
    // User Drop type compiles cleanly for locals and nested temporary arithmetic expressions.
    emits(
        "record Guard { id: i64 }\n\
         instance Drop<Guard> { fn drop g = Debug.print (ref g.id) }\n\
         instance Add<Guard> { fn add a b = Guard { id: a.id + b.id } }\n\
         let a = Guard { id: 1 }\n\
         let b = Guard { id: 2 }\n\
         let c = (a + b) + (b + a)\n\
         a.id + b.id + c.id",
    );
}
