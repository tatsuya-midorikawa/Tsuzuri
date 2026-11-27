//! Objective-C interop: the pure rules that the checker and the code generator share (M01, M02).
//!
//! An `extern class` is a handle type (`HandleKind::ObjcLocal` or `ObjcSync`); this module holds
//! what the type alone does not say: the class table, the shape of selectors and runtime names,
//! and the Cocoa method families that decide who owns a result.

use std::collections::BTreeMap;

use crate::diagnostic::Span;

/// The longest chain of base classes from a class to `ObjC.Object` (M01).
pub const MAX_CLASS_DEPTH: usize = 32;
/// The longest selector in bytes (M01). The macOS SDK has selectors above 255 bytes.
pub const MAX_SELECTOR_BYTES: usize = 1024;
/// The longest Objective-C runtime class name in bytes.
pub const MAX_CLASS_NAME_BYTES: usize = 255;

/// An `extern class` as the checker resolved it (M01), with its members (M02).
#[derive(Clone, Debug)]
pub struct ObjcClassInfo {
    /// `Module.Name`, the name of the handle type.
    pub name: String,
    /// The class's name in the Objective-C runtime.
    pub runtime_name: String,
    /// The qualified name of the declared base class; `None` when the base is `ObjC.Object`.
    pub base: Option<String>,
    pub sync: bool,
    pub trusted: bool,
    /// Declared `@weak`: the class may be missing at run time (M17).
    pub weak: bool,
    /// How many classes the chain has from this one to the root, this one included.
    pub depth: usize,
    pub public: bool,
    pub span: Span,
    /// The members that the class itself declares, in source order (M02).
    pub members: Vec<ObjcMemberInfo>,
}

/// An `extern category` as the checker resolved it (M02): a namespace of members of `class`.
#[derive(Clone, Debug)]
pub struct ObjcCategoryInfo {
    /// `Module.Name`.
    pub name: String,
    /// The qualified name of the class that the category extends.
    pub class: String,
    pub trusted: bool,
    pub public: bool,
    pub span: Span,
    pub members: Vec<ObjcMemberInfo>,
}

/// A member of an `extern class` or `extern category` (M02), as its declaration writes it. The
/// types are resolved where a call of the member is checked, with `Self` replaced by the class
/// that the call names.
#[derive(Clone, Debug)]
pub struct ObjcMemberInfo {
    pub name: String,
    /// The written selector, or the name of a member without arguments.
    pub selector: String,
    pub class_method: bool,
    /// The module of the declaration, where the member's types are resolved.
    pub module: String,
    /// The span of the name.
    pub span: Span,
    pub declaration: crate::syntax::ObjcMember,
}

/// How a message send is lowered (M02): what `HostImport::objc` holds for the function that
/// stands for one member called on one class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjcSend {
    pub selector: String,
    /// The runtime name of the class that receives a class method; `None` for an instance
    /// method, whose receiver is the first argument.
    pub class: Option<String>,
    /// The object result is a +1 reference (the alloc, copy, mutableCopy, new and init families,
    /// or `@returns_retained`); otherwise it is retained right after the send.
    pub retained: bool,
    /// The member as source code names it, `Class.member`, for diagnostics and the audit.
    pub display: String,
    /// The qualified name of the class or category that the call names, whose audit row counts
    /// the use.
    pub owner: String,
}

/// The method families of Cocoa's naming rules that change ownership (M02 D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Alloc,
    Copy,
    MutableCopy,
    New,
    Init,
}

/// The family that a selector's first part names, as clang's `ObjCMethodDecl::getMethodFamily`
/// reads it: leading underscores are skipped, and the family word ends the part or is followed
/// by a character that is not a lowercase letter. Whether the family applies (an object result;
/// `init` only for an instance method) is the caller's rule.
pub fn method_family(selector: &str) -> Option<Family> {
    let first = selector.split(':').next().unwrap_or("");
    let first = first.trim_start_matches('_');
    for (word, family) in [
        ("alloc", Family::Alloc),
        ("copy", Family::Copy),
        ("mutableCopy", Family::MutableCopy),
        ("new", Family::New),
        ("init", Family::Init),
    ] {
        if let Some(rest) = first.strip_prefix(word)
            && !rest.starts_with(|next: char| next.is_ascii_lowercase())
        {
            return Some(family);
        }
    }
    None
}

fn c_identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Whether `selector` is one identifier (`length`) or identifiers that each end in a colon
/// (`setTitle:`, `initWithBytes:length:encoding:`). The length limit is separate.
pub fn valid_selector(selector: &str) -> bool {
    match selector.strip_suffix(':') {
        None => c_identifier(selector),
        Some(parts) => parts.split(':').all(c_identifier),
    }
}

/// How many arguments a valid selector takes: its colons.
pub fn selector_arity(selector: &str) -> usize {
    selector.bytes().filter(|byte| *byte == b':').count()
}

/// Whether `name` can be an Objective-C runtime class name: a C identifier of at most 255 bytes.
pub fn valid_class_name(name: &str) -> bool {
    name.len() <= MAX_CLASS_NAME_BYTES && c_identifier(name)
}

/// The classes of a program by the qualified name of their handle type, and its categories.
#[derive(Clone, Debug, Default)]
pub struct ObjcClasses {
    pub classes: BTreeMap<String, ObjcClassInfo>,
    pub categories: BTreeMap<String, ObjcCategoryInfo>,
    /// Some type of the program names `ObjC.Object`, so object values exist without a class.
    pub object_used: bool,
}

impl ObjcClasses {
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.categories.is_empty()
    }

    /// Whether the program can hold an Objective-C object value at all: it declares a class or
    /// names `ObjC.Object`. The analyses of object values skip a program without either.
    pub fn has_objects(&self) -> bool {
        !self.classes.is_empty() || self.object_used
    }

    /// Whether `ancestor` is `class` itself or a class of its declared chain. `ObjC.Object` is
    /// an ancestor of every class.
    pub fn is_ancestor(&self, ancestor: &str, class: &str) -> bool {
        if ancestor == crate::syntax::OBJC_OBJECT {
            return true;
        }
        let mut current = Some(class);
        // The chain is finite: the checker rejected cycles and chains above the depth limit.
        for _ in 0..=MAX_CLASS_DEPTH {
            let Some(name) = current else {
                return false;
            };
            if name == ancestor {
                return true;
            }
            current = self.classes.get(name).and_then(|info| info.base.as_deref());
        }
        false
    }

    /// The runtime name of the class type `qualified`; `None` for `ObjC.Object`, which is no
    /// class of the runtime.
    pub fn runtime_name(&self, qualified: &str) -> Option<&str> {
        self.classes
            .get(qualified)
            .map(|info| info.runtime_name.as_str())
    }

    /// The class `qualified` and its bases, nearest first.
    pub fn chain<'a>(&'a self, qualified: &str) -> Vec<&'a ObjcClassInfo> {
        let mut chain = Vec::new();
        let mut current = self.classes.get(qualified);
        while let Some(info) = current {
            if chain.len() > MAX_CLASS_DEPTH {
                break;
            }
            chain.push(info);
            current = info.base.as_deref().and_then(|base| self.classes.get(base));
        }
        chain
    }

    /// The member `name` that `qualified` or one of its bases declares, nearest first.
    pub fn member<'a>(&'a self, qualified: &str, name: &str) -> Option<&'a ObjcMemberInfo> {
        self.chain(qualified)
            .into_iter()
            .find_map(|info| info.members.iter().find(|member| member.name == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_are_identifiers_with_colons() {
        for valid in [
            "length",
            "_private",
            "setTitle:",
            "initWithBytes:length:encoding:",
            "a1:b2:",
        ] {
            assert!(valid_selector(valid), "{valid}");
        }
        for invalid in [
            "",
            ":",
            "foo::",
            "foo:bar",
            "set Title:",
            "1st",
            "setTitle:x",
            "é",
            "a:b:c",
        ] {
            assert!(!valid_selector(invalid), "{invalid}");
        }
        assert_eq!(selector_arity("length"), 0);
        assert_eq!(selector_arity("setTitle:"), 1);
        assert_eq!(selector_arity("initWithBytes:length:encoding:"), 3);
    }

    #[test]
    fn families_follow_clang() {
        // The expectations are clang's rule (ObjCMethodDecl::getMethodFamily), written by hand.
        for (selector, family) in [
            ("alloc", Some(Family::Alloc)),
            ("allocWithZone:", Some(Family::Alloc)),
            ("_alloc", Some(Family::Alloc)),
            ("allocate", None),
            ("new", Some(Family::New)),
            ("newObject", Some(Family::New)),
            ("newer", None),
            ("copy", Some(Family::Copy)),
            ("copyWithZone:", Some(Family::Copy)),
            ("copying", None),
            ("mutableCopy", Some(Family::MutableCopy)),
            ("mutableCopyWithZone:", Some(Family::MutableCopy)),
            ("init", Some(Family::Init)),
            ("initWithFrame:", Some(Family::Init)),
            ("__init", Some(Family::Init)),
            ("initialize", None),
            ("initial:", None),
            ("init_x", Some(Family::Init)),
            ("init2", Some(Family::Init)),
            ("description", None),
            ("stringWithFormat:", None),
        ] {
            assert_eq!(method_family(selector), family, "{selector}");
        }
    }

    #[test]
    fn class_names_are_c_identifiers() {
        assert!(valid_class_name("NSWindow"));
        assert!(valid_class_name("_TtC4Main3Foo"));
        assert!(!valid_class_name("9x"));
        assert!(!valid_class_name(""));
        assert!(!valid_class_name("NS Window"));
        assert!(valid_class_name(&"a".repeat(255)));
        assert!(!valid_class_name(&"a".repeat(256)));
    }
}
