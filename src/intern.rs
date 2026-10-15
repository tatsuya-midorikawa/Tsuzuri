use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Symbol(u32);

#[derive(Default)]
pub(super) struct Interner {
    ids: HashMap<Rc<str>, Symbol>,
    names: Vec<Rc<str>>,
}

impl Interner {
    pub(super) fn intern(&mut self, text: &str) -> Symbol {
        if let Some(symbol) = self.lookup(text) {
            return symbol;
        }
        let symbol = Symbol(u32::try_from(self.names.len()).expect("source limits bound symbols"));
        let text: Rc<str> = Rc::from(text);
        self.names.push(Rc::clone(&text));
        self.ids.insert(text, symbol);
        symbol
    }

    pub(super) fn lookup(&self, text: &str) -> Option<Symbol> {
        self.ids.get(text).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_follow_first_use_and_share_text_storage() {
        let mut names = Interner::default();
        let first = names.intern("Main");
        let second = names.intern("helper");
        assert_eq!(first, Symbol(0));
        assert_eq!(second, Symbol(1));
        assert_eq!(names.intern("Main"), first);
        assert_eq!(names.lookup("helper"), Some(second));
        assert_eq!(names.lookup("absent"), None);
        assert_eq!(names.names.len(), 2);
        assert!(Rc::ptr_eq(
            &names.names[0],
            names.ids.get_key_value("Main").unwrap().0
        ));
    }
}
