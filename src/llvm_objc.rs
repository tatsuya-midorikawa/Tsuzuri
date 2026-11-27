//! The Objective-C lowering (M01, M02): class and selector references, the module flags of an
//! Objective-C image, retain and release, and the functions of the built-in module `ObjC`.
//!
//! The shapes follow what clang emits for `-fobjc-arc` on an arm64 Apple target (classrefs,
//! selrefs, `@llvm.compiler.used`, the six module flags). Retain and release are plain calls of
//! `objc_retain` and `objc_release`; no ObjCARC pass runs. A program that uses no Objective-C
//! type emits nothing from here.

use super::*;

/// The libobjc functions that the compiler calls, with their `declare` lines. The IR declares
/// each one once, and only when the program's own `extern` does not declare the symbol already.
const RUNTIME: [(&str, &str); 4] = [
    ("objc_retain", "declare ptr @objc_retain(ptr)"),
    ("objc_release", "declare void @objc_release(ptr)"),
    (
        "objc_retainAutoreleasedReturnValue",
        "declare ptr @objc_retainAutoreleasedReturnValue(ptr)",
    ),
    (
        "objc_opt_isKindOfClass",
        "declare i1 @objc_opt_isKindOfClass(ptr, ptr)",
    ),
];

/// The operand of the `clang.arc.attachedcall` bundle. It is only named, never called (M01 D6).
const ATTACHED_CALL_DECLARE: &str = "declare ptr @llvm.objc.retainAutoreleasedReturnValue(ptr)";
/// The bundle that makes the backend retain a +0 result right after the call (M01 D6).
pub(super) const ATTACHED_CALL: &str =
    " [ \"clang.arc.attachedcall\"(ptr @llvm.objc.retainAutoreleasedReturnValue) ]";

/// What the emitted functions use of Objective-C (M01).
#[derive(Default)]
pub(super) struct ObjcGlobals {
    /// Some emitted code holds, retains or releases an Objective-C object, or reads a class or
    /// selector reference: the module carries the Objective-C flags and links libobjc.
    pub(super) used: bool,
    /// The runtime names of the classes whose reference the code reads.
    classrefs: BTreeSet<String>,
    /// The selectors that the code reads, numbered in the order of their first use.
    selectors: BTreeMap<String, usize>,
    /// The libobjc functions that the code calls.
    runtime: BTreeSet<&'static str>,
    /// Some call carries the `clang.arc.attachedcall` bundle.
    attached_call: bool,
    /// The classes declared `@weak` whose reference the code reads, by runtime name (M17).
    weak_classes: BTreeSet<String>,
    /// Some emitted code sends a message (M02).
    msg_send: bool,
    /// The constant NSStrings of `ObjC.literal`, numbered in the order of their first use, by
    /// their UTF-16 code units (M02).
    cfstrings: BTreeMap<Vec<u16>, usize>,
}

impl ObjcGlobals {
    fn runtime(&mut self, symbol: &'static str) {
        debug_assert!(RUNTIME.iter().any(|(name, _)| *name == symbol));
        self.used = true;
        self.runtime.insert(symbol);
    }

    /// Marks a call that carries the attached-call bundle.
    pub(super) fn attached_call(&mut self) {
        self.used = true;
        self.attached_call = true;
    }

    /// Marks a call of `objc_msgSend` (M02).
    pub(super) fn msg_send(&mut self) {
        self.used = true;
        self.msg_send = true;
    }
}

/// The one declaration of `objc_msgSend`, as clang writes it (M02 D11).
const MSG_SEND_DECLARE: &str = "declare ptr @objc_msgSend(ptr, ptr, ...)";

/// M02 spec 5: the extension attribute of `ty` where a message passes or returns it in its own
/// width: `zeroext` for `bool` and the unsigned 8- and 16-bit integers, `signext` for the signed
/// ones. `None` for every other type, which a message passes as an `extern def` does.
pub(super) fn small_scalar_extension(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Bool | Type::Integer(8 | 16, false) => Some("zeroext"),
        Type::Integer(8 | 16, true) => Some("signext"),
        _ => None,
    }
}

/// The symbol of the class object that the Objective-C runtime knows as `runtime_name`.
pub(crate) fn class_symbol(runtime_name: &str) -> String {
    format!("OBJC_CLASS_$_{runtime_name}")
}

impl FunctionEmitter<'_, '_> {
    /// Reads the reference of the class `runtime_name`: its class object. A class declared
    /// `@weak` that this system does not have reports its symbol and traps (M17).
    pub(super) fn objc_classref(&mut self, runtime_name: &str) -> String {
        self.globals.objc.used = true;
        self.globals.objc.classrefs.insert(runtime_name.to_owned());
        let class = self.value(format!(
            "load ptr, ptr @tz.objc.classref.{runtime_name}, align 8"
        ));
        let weak = self
            .module
            .objc
            .classes
            .values()
            .any(|info| info.weak && info.runtime_name == runtime_name);
        if weak {
            self.globals
                .objc
                .weak_classes
                .insert(runtime_name.to_owned());
            let symbol = class_symbol(runtime_name);
            let present = self.value(format!("icmp ne ptr {class}, null"));
            let ok_block = self.label();
            let miss_block = self.label();
            self.branch(&present, &ok_block, &miss_block);
            self.begin(&miss_block);
            self.instruction(format!(
                "call void @tz.weak.missing(ptr @tz.weak.name.{symbol}, i64 {})",
                symbol.len()
            ));
            self.globals.weak_checked_symbols.insert(symbol);
            self.emit_trap(TrapKind::Assert);
            self.instruction("unreachable");
            self.begin(&ok_block);
        }
        class
    }

    /// Reads the reference of `selector`: the `SEL` that the runtime registered for it.
    pub(super) fn objc_selref(&mut self, selector: &str) -> String {
        self.globals.objc.used = true;
        let next = self.globals.objc.selectors.len();
        let number = *self
            .globals
            .objc
            .selectors
            .entry(selector.to_owned())
            .or_insert(next);
        self.value(format!(
            "load ptr, ptr @tz.objc.selref.{number}, align 8, !invariant.load !{{}}"
        ))
    }

    /// `objc_retain(object)`: one more strong reference; the result is the same object.
    pub(super) fn objc_retain(&mut self, object: &str) -> String {
        self.globals.objc.runtime("objc_retain");
        self.value(format!("call ptr @objc_retain(ptr {object})"))
    }

    /// `objc_release(object)`: gives up one strong reference. A null pointer, which a moved-out
    /// slot holds, is released as nothing.
    pub(super) fn objc_release(&mut self, object: &str) {
        self.globals.objc.runtime("objc_release");
        self.instruction(format!("call void @objc_release(ptr {object})"));
    }

    /// Retains a +0 result that came without the attached-call bundle: the result of a call
    /// whose canonical C type is `i64` (W14), after its `inttoptr`. The object stays in the
    /// autorelease pool, as with any plain retain.
    pub(super) fn objc_retain_autoreleased(&mut self, object: &str) -> String {
        self.globals
            .objc
            .runtime("objc_retainAutoreleasedReturnValue");
        self.value(format!(
            "call ptr @objc_retainAutoreleasedReturnValue(ptr {object})"
        ))
    }

    /// A direct call of a function of the built-in module `ObjC` (M01).
    pub(super) fn objc_builtin(
        &mut self,
        instance: &BuiltinInstance,
        argument: &TypedExpr,
        result: &Type,
    ) -> String {
        match instance.builtin {
            // The borrow is the address of the slot that holds the object.
            Builtin::ObjcRetain => {
                let borrowed = self.expression(argument);
                let object = self.value(format!("load ptr, ptr {borrowed}"));
                self.objc_retain(&object)
            }
            // A base class is the same object: no code.
            Builtin::ObjcUpcast | Builtin::ObjcUpcastOwned => self.expression(argument),
            Builtin::ObjcDowncast => {
                let borrowed = self.expression(argument);
                let object = self.value(format!("load ptr, ptr {borrowed}"));
                let Type::Handle(target, _) = &instance.types[1] else {
                    unreachable!("the checker validated the target of ObjC.downcast")
                };
                let pointer = match self.module.objc.runtime_name(target).map(str::to_owned) {
                    // Every object is an `ObjC.Object`.
                    None => self.objc_retain(&object),
                    Some(runtime_name) => {
                        let class = self.objc_classref(&runtime_name);
                        self.globals.objc.runtime("objc_opt_isKindOfClass");
                        let kind = self.value(format!(
                            "call i1 @objc_opt_isKindOfClass(ptr {object}, ptr {class})"
                        ));
                        let from = self.block.clone();
                        let retain_block = self.label();
                        let done_block = self.label();
                        self.branch(&kind, &retain_block, &done_block);
                        self.begin(&retain_block);
                        let retained = self.objc_retain(&object);
                        let from_retain = self.block.clone();
                        self.jump(&done_block);
                        self.begin(&done_block);
                        self.value(format!(
                            "phi ptr [ null, %{from} ], [ {retained}, %{from_retain} ]"
                        ))
                    }
                };
                self.nullable_from_c(result, &pointer)
            }
            Builtin::ObjcSel => {
                let selector = crate::check::constant_string(argument, &self.module.statics)
                    .expect("the checker validated the argument of ObjC.sel");
                self.objc_selref(&selector)
            }
            // M02 spec 7: a constant NSString object, which no retain or release changes.
            Builtin::ObjcLiteral => {
                let text = crate::check::constant_string(argument, &self.module.statics)
                    .expect("the checker validated the argument of ObjC.literal");
                self.globals.objc.used = true;
                let units: Vec<u16> = text.encode_utf16().collect();
                let next = self.globals.objc.cfstrings.len();
                let number = *self.globals.objc.cfstrings.entry(units).or_insert(next);
                format!("@tz.objc.cfstring.{number}")
            }
            _ => unreachable!("not a function of the module ObjC"),
        }
    }
}

/// Whether `line` declares the function `symbol`.
fn declares(line: &str, symbol: &str) -> bool {
    line.strip_prefix("declare ").is_some_and(|rest| {
        rest.find(" @").is_some_and(|at| {
            rest[at + 2..]
                .strip_prefix(symbol)
                .is_some_and(|after| after.starts_with('('))
        })
    })
}

/// The globals, declarations and module flags of the Objective-C uses that `globals.objc`
/// collected, appended to `output` (and to `globals.definitions` for the metadata) once every
/// function is emitted. Nothing for a program that uses no Objective-C.
pub(super) fn finish(globals: &mut Globals, output: &mut String) {
    let objc = std::mem::take(&mut globals.objc);
    if !objc.used {
        return;
    }
    let mut used = Vec::new();
    for runtime_name in &objc.classrefs {
        let symbol = class_symbol(runtime_name);
        let linkage = if objc.weak_classes.contains(runtime_name) {
            "extern_weak"
        } else {
            "external"
        };
        let _ = writeln!(output, "@\"{symbol}\" = {linkage} global [0 x i8]");
        let _ = writeln!(
            output,
            "@tz.objc.classref.{runtime_name} = internal global ptr @\"{symbol}\", section \"__DATA,__objc_classrefs,regular,no_dead_strip\", align 8"
        );
        used.push(format!("ptr @tz.objc.classref.{runtime_name}"));
    }
    let mut selectors: Vec<(&String, usize)> = objc
        .selectors
        .iter()
        .map(|(selector, number)| (selector, *number))
        .collect();
    selectors.sort_by_key(|(_, number)| *number);
    for (selector, number) in selectors {
        // A selector is ASCII identifiers and colons, so it needs no escape.
        let _ = writeln!(
            output,
            "@tz.objc.methname.{number} = private unnamed_addr constant [{} x i8] c\"{selector}\\00\", section \"__TEXT,__objc_methname,cstring_literals\", align 1",
            selector.len() + 1
        );
        let _ = writeln!(
            output,
            "@tz.objc.selref.{number} = internal externally_initialized global ptr @tz.objc.methname.{number}, section \"__DATA,__objc_selrefs,literal_pointers,no_dead_strip\", align 8"
        );
        used.push(format!("ptr @tz.objc.methname.{number}"));
        used.push(format!("ptr @tz.objc.selref.{number}"));
    }
    if !used.is_empty() {
        let _ = writeln!(
            output,
            "@llvm.compiler.used = appending global [{} x ptr] [{}], section \"llvm.metadata\"",
            used.len(),
            used.join(", ")
        );
    }
    // M02 spec 7: the constant NSStrings, in clang's layout: ASCII without a NUL as bytes
    // (flags 1992, the length in bytes), anything else as UTF-16 (flags 2000, the length in
    // units). The data ends in a zero unit that the length does not count.
    if !objc.cfstrings.is_empty() {
        let _ = writeln!(
            output,
            "@__CFConstantStringClassReference = external global [0 x i32]"
        );
    }
    let mut cfstrings: Vec<(&Vec<u16>, usize)> = objc
        .cfstrings
        .iter()
        .map(|(units, number)| (units, *number))
        .collect();
    cfstrings.sort_by_key(|(_, number)| *number);
    for (units, number) in cfstrings {
        let ascii = units.iter().all(|unit| (1..0x80).contains(unit));
        let (flags, length) = if ascii {
            let bytes: String = units.iter().map(|unit| format!("\\{unit:02X}")).collect();
            let _ = writeln!(
                output,
                "@tz.objc.cfstring.data.{number} = private unnamed_addr constant [{} x i8] c\"{bytes}\\00\", section \"__TEXT,__cstring,cstring_literals\", align 1",
                units.len() + 1
            );
            (1992, units.len())
        } else {
            let data: Vec<String> = units
                .iter()
                .chain(std::iter::once(&0))
                .map(|unit| format!("i16 {unit}"))
                .collect();
            let _ = writeln!(
                output,
                "@tz.objc.cfstring.data.{number} = private unnamed_addr constant [{} x i16] [{}], section \"__TEXT,__ustring\", align 2",
                units.len() + 1,
                data.join(", ")
            );
            (2000, units.len())
        };
        let _ = writeln!(
            output,
            "@tz.objc.cfstring.{number} = private global {{ ptr, i32, ptr, i64 }} {{ ptr @__CFConstantStringClassReference, i32 {flags}, ptr @tz.objc.cfstring.data.{number}, i64 {length} }}, section \"__DATA,__cfstring\", align 8"
        );
    }
    let mut declarations: Vec<&str> = RUNTIME
        .iter()
        .filter(|(symbol, _)| objc.runtime.contains(symbol))
        .filter(|(symbol, _)| !output.lines().any(|line| declares(line, symbol)))
        .map(|(_, declaration)| *declaration)
        .collect();
    if objc.msg_send && !output.lines().any(|line| declares(line, "objc_msgSend")) {
        declarations.push(MSG_SEND_DECLARE);
    }
    if objc.attached_call {
        declarations.push(ATTACHED_CALL_DECLARE);
    }
    for declaration in declarations {
        let _ = writeln!(output, "{declaration}");
    }
    // The flags of an Objective-C image, in clang's order. A `-g` build has a flags line
    // already, which takes them too: the module has one `!llvm.module.flags`.
    let mut flags = Vec::new();
    for flag in [
        "!{i32 1, !\"Objective-C Version\", i32 2}",
        "!{i32 1, !\"Objective-C Image Info Version\", i32 0}",
        "!{i32 1, !\"Objective-C Image Info Section\", !\"__DATA,__objc_imageinfo,regular,no_dead_strip\"}",
        "!{i32 1, !\"Objective-C Garbage Collection\", i8 0}",
        "!{i32 1, !\"Objective-C Class Properties\", i32 64}",
        "!{i32 1, !\"Objective-C Enforce ClassRO Pointer Signing\", i8 0}",
    ] {
        let id = globals.next_metadata;
        globals.next_metadata += 1;
        globals.definitions.push(format!("!{id} = {flag}"));
        flags.push(format!("!{id}"));
    }
    const FLAGS: &str = "!llvm.module.flags = !{";
    let existing = globals.definitions.iter_mut().find_map(|definition| {
        let start = definition.find(FLAGS)?;
        let end = start + definition[start..].find('}')?;
        Some((definition, end))
    });
    match existing {
        Some((definition, end)) => {
            definition.insert_str(end, &format!(", {}", flags.join(", ")));
        }
        None => globals
            .definitions
            .push(format!("{FLAGS}{}}}", flags.join(", "))),
    }
}

/// Whether `ir` is the IR of an Objective-C image: it carries the module flag that `finish`
/// writes. The driver links libobjc into such a program (M01 D11).
pub fn uses_objc(ir: &str) -> bool {
    ir.contains("!\"Objective-C Version\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_declaration_of_a_symbol() {
        assert!(declares("declare void @objc_release(i64)", "objc_release"));
        assert!(declares(
            "declare extern_weak ptr @objc_retain(ptr)",
            "objc_retain"
        ));
        assert!(!declares(
            "declare ptr @objc_retainAutoreleasedReturnValue(ptr)",
            "objc_retain"
        ));
        assert!(!declares(
            "  call void @objc_release(ptr %0)",
            "objc_release"
        ));
    }

    #[test]
    fn recognizes_an_objective_c_image() {
        assert!(uses_objc(
            "!0 = !{i32 1, !\"Objective-C Version\", i32 2}\n"
        ));
        assert!(!uses_objc("define i32 @main() {\n  ret i32 0\n}\n"));
    }
}
