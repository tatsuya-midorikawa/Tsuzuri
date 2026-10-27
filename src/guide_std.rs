//! The std index of `tsuzuri guide std` (G21 Phase 3).

/// `tsuzuri guide std`: one line per module.
pub(crate) fn render_modules() -> String {
    String::from("# std modules\n")
}

/// `tsuzuri guide std MODULE`: every function of one module; `None` for an unknown module.
pub(crate) fn render_module(module: &str) -> Option<String> {
    (module == "Array").then(|| String::from("# Array\n"))
}

/// `tsuzuri guide --topic std-all`: every function of every module.
pub(crate) fn render_all() -> String {
    String::from("# std index\n")
}
