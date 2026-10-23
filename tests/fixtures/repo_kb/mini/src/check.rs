use std::fmt;

/* outer /* nested "E1001" */ still a comment */
const RAW: &str = r#"E1004"#;
const NOTE: &str = "fn fake() { Diagnostic::new(\"E1002\") }";
const BRACE: char = '{';

pub struct Checker<'a> {
    source: &'a str,
}

impl<'a> Checker<'a> {
    pub fn value_expression(&self) -> Result<i64, Diagnostic> {
        let closer = '}';
        let _ = (closer, b"E1005");
        Err(Diagnostic::new("E1012", "use of moved value", 0))
    }

    fn private_helper(&self) -> &'a str {
        self.source
    }
}

pub trait Rule {
    fn check(&self) -> bool {
        warn("W1006")
    }
}

pub(crate) fn warn_copy() -> Diagnostic {
    Diagnostic::warning("W1006", "implicit copy", 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks() {
        assert_eq!(Checker { source: "" }.value_expression().unwrap_err().code, "E1999");
    }
}

pub fn after_tests() -> &'static str {
    "E1013"
}
