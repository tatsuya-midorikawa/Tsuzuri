//! A22: the noncanonical spellings that the parser records in `Program.noncanonical`, each with
//! the edits that rewrite it to the canonical form that every edition accepts. The records are
//! syntax only; the type check later drops the borrow rewrites whose operand is a reference.

use super::*;

impl Parser<'_> {
    fn push_noncanonical(&mut self, finding: Noncanonical) {
        if self.noncanonical.len() < crate::canonical::MAX_FINDINGS {
            self.noncanonical.push(finding);
        }
    }

    /// The last `..` of a `for` range: `..=` says the same.
    #[inline(never)]
    pub(super) fn record_bare_range(&mut self, span: Span) {
        self.push_noncanonical(Noncanonical {
            kind: NoncanonicalKind::BareRange,
            span,
            spelling: "..".into(),
            canonical: "..=".into(),
            level: FixLevel::Certain,
            edits: vec![TextEdit {
                span,
                text: "..=".into(),
            }],
        });
    }

    /// A numeric literal with a short suffix (`86uy`): the type name says the same.
    #[inline(never)]
    pub(super) fn record_short_suffix(&mut self, span: Span) {
        let Some(text) = self.source.get(span.start..span.end) else {
            return;
        };
        let suffix = crate::lexer::numeric_suffix(text);
        let Some(name) = crate::numeric::short_suffix_type(suffix) else {
            return;
        };
        let digits = &text[..text.len() - suffix.len()];
        self.push_noncanonical(Noncanonical {
            kind: NoncanonicalKind::ShortSuffix,
            span,
            spelling: text.to_owned(),
            canonical: format!("{digits}{name}"),
            level: FixLevel::Certain,
            edits: vec![TextEdit {
                span: Span {
                    start: span.end - suffix.len(),
                    ..span
                },
                text: name.into(),
            }],
        });
    }

    /// `&x`, `&mut x`, `*r` or `**r`, whose operand `value` the prefix parsed; `term` is the
    /// argument position (`f &x y`). `mutable` ends at the `mut` of `&mut`.
    #[inline(never)]
    pub(super) fn record_symbolic_prefix(
        &mut self,
        token: &Token,
        mutable: Option<Span>,
        term: bool,
        value: &Expr,
    ) {
        let (kind, spelling, canonical) = match (&token.kind, mutable) {
            (TokenKind::Ampersand, None) => (NoncanonicalKind::SymbolicBorrow, "&", "ref"),
            (TokenKind::Ampersand, Some(_)) => {
                (NoncanonicalKind::SymbolicBorrow, "&mut", "ref mut")
            }
            (TokenKind::Star, _) => (NoncanonicalKind::SymbolicDeref, "*", "deref"),
            (TokenKind::DoubleStar, _) => (NoncanonicalKind::SymbolicDeref, "**", "deref deref"),
            _ => return,
        };
        // `&mut` keeps its `mut`; only the symbol is replaced.
        let keyword = match token.kind {
            TokenKind::Ampersand => "ref",
            TokenKind::Star => "deref",
            _ => "deref deref",
        };
        let level = if kind == NoncanonicalKind::SymbolicBorrow && !is_place_path(value) {
            FixLevel::Likely
        } else {
            FixLevel::Certain
        };
        let at = |start: usize| Span {
            start,
            end: start,
            source: token.span.source,
        };
        let edits = if term {
            // An argument: the whole prefix form goes in parentheses.
            vec![
                TextEdit {
                    span: token.span,
                    text: format!("({keyword} "),
                },
                TextEdit {
                    span: at(value.span.end),
                    text: ")".into(),
                },
            ]
        } else if self.is_term_shaped(value) {
            vec![TextEdit {
                span: token.span,
                text: format!("{keyword} "),
            }]
        } else {
            // A keyword takes one term, so an application or operator operand is parenthesized.
            vec![
                TextEdit {
                    span: token.span,
                    text: format!("{keyword} "),
                },
                TextEdit {
                    span: at(value.span.start),
                    text: "(".into(),
                },
                TextEdit {
                    span: at(value.span.end),
                    text: ")".into(),
                },
            ]
        };
        self.push_noncanonical(Noncanonical {
            kind,
            span: mutable.map_or(token.span, |end| token.span.through(end)),
            spelling: spelling.into(),
            canonical: canonical.into(),
            level,
            edits,
        });
    }

    /// `&` or `&&` before a type: `ref` and `ref ref`.
    #[inline(never)]
    pub(super) fn record_symbolic_type(&mut self, token: &Token) {
        let (spelling, canonical) = if token.kind == TokenKind::AndAnd {
            ("&&", "ref ref")
        } else {
            ("&", "ref")
        };
        self.push_noncanonical(Noncanonical {
            kind: NoncanonicalKind::SymbolicReferenceType,
            span: token.span,
            spelling: spelling.into(),
            canonical: canonical.into(),
            level: FixLevel::Certain,
            edits: vec![TextEdit {
                span: token.span,
                text: format!("{canonical} "),
            }],
        });
    }

    /// `name -> body` without a backslash. A chain `x -> y -> e` and `x -> \y -> e` fold into one
    /// lambda, so they become `\x y -> e`; a parenthesized lambda body has no rewrite, since
    /// `\x -> (\y -> e)` would not fold (D11).
    #[inline(never)]
    pub(super) fn record_legacy_lambda(&mut self, name: &Ident, arrow: Span, body: &Expr) {
        let head = name.span.through(arrow);
        let slash = TextEdit {
            span: Span {
                end: name.span.start,
                ..name.span
            },
            text: "\\".into(),
        };
        let first = self.source[body.span.start..].chars().next();
        let mut finding = Noncanonical {
            kind: NoncanonicalKind::LegacyLambda,
            span: head,
            spelling: format!("{} ->", name.text),
            canonical: format!("\\{} ->", name.text),
            level: FixLevel::Certain,
            edits: vec![slash.clone()],
        };
        if let ExprKind::Lambda(parameters, _) = &body.kind {
            let inner: Vec<&str> = parameters
                .iter()
                .map(|(parameter, _)| parameter.text.as_str())
                .collect();
            let gap = |end: usize| TextEdit {
                span: Span {
                    start: name.span.end,
                    end,
                    source: name.span.source,
                },
                text: " ".into(),
            };
            let nested = self.noncanonical.iter().rposition(|record| {
                record.kind == NoncanonicalKind::LegacyLambda
                    && record.span.start == body.span.start
            });
            match (first, nested) {
                (Some('\\'), _) => {
                    finding.canonical = format!("\\{} {} ->", name.text, inner.join(" "));
                    finding.edits.push(gap(body.span.start + 1));
                }
                (_, Some(index)) => {
                    let nested = self.noncanonical.remove(index);
                    finding.span = name.span.through(nested.span);
                    finding.spelling = format!("{} -> {}", name.text, nested.spelling);
                    finding.canonical = format!("\\{} {}", name.text, &nested.canonical[1..]);
                    if nested.level == FixLevel::Unavailable {
                        finding.level = FixLevel::Unavailable;
                        finding.edits.clear();
                    } else {
                        finding.level = finding.level.max(nested.level);
                        finding.edits.push(gap(body.span.start));
                        finding
                            .edits
                            .extend(nested.edits.into_iter().filter(|edit| {
                                !(edit.text == "\\" && edit.span.start == body.span.start)
                            }));
                    }
                }
                _ => {
                    finding.canonical = format!("\\{} {} ->", name.text, inner.join(" "));
                    finding.level = FixLevel::Unavailable;
                    finding.edits.clear();
                }
            }
        }
        // `\_ -> e` reads `_` as a pattern and matches the argument; `_ -> e` binds it. They mean
        // the same, but only an unoptimized build tells them apart, so the rewrite is likely.
        if finding.level == FixLevel::Certain
            && finding
                .spelling
                .split(" -> ")
                .any(|part| part.trim_end_matches(" ->") == "_")
        {
            finding.level = FixLevel::Likely;
        }
        if finding.edits.len() > crate::diagnostic::MAX_EDITS {
            finding.level = FixLevel::Unavailable;
            finding.edits.clear();
        }
        self.push_noncanonical(finding);
    }

    /// Whether `expression` reads as one term after a keyword prefix: a name, a literal, a
    /// collection, a projection of a term, or anything in parentheses.
    fn is_term_shaped(&self, expression: &Expr) -> bool {
        match &expression.kind {
            ExprKind::Name(_)
            | ExprKind::QualifiedFunction(_)
            | ExprKind::TypeFunction(..)
            | ExprKind::Integer(..)
            | ExprKind::BigInt(_)
            | ExprKind::Float(..)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Utf8Char(_)
            | ExprKind::Bool(_)
            | ExprKind::Unit
            | ExprKind::Array(_)
            | ExprKind::List(_)
            | ExprKind::Interpolated(_) => true,
            ExprKind::Field(base, _)
            | ExprKind::Index(base, _)
            | ExprKind::Slice { value: base, .. }
            | ExprKind::Borrow(base, ..)
            | ExprKind::Dereference(base, _) => {
                self.is_group(expression) || self.is_term_shaped(base)
            }
            _ => self.is_group(expression),
        }
    }

    /// Whether `expression` is written in parentheses: its first token is a `(` whose `)` is
    /// its last token.
    fn is_group(&self, expression: &Expr) -> bool {
        let Ok(first) = self
            .tokens
            .binary_search_by_key(&expression.span.start, |token| token.span.start)
        else {
            return false;
        };
        if self.tokens[first].kind != TokenKind::LeftParen {
            return false;
        }
        let mut depth = 0usize;
        for token in &self.tokens[first..] {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return token.span.end == expression.span.end;
                    }
                }
                _ => {}
            }
            if token.span.start >= expression.span.end {
                break;
            }
        }
        false
    }
}

/// A place: a name and its fields, elements, slices and dereferences. Borrowing one means the
/// same with `ref` as with `&`; borrowing a temporary may not (`&[]` guides its element type).
fn is_place_path(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::Name(_) => true,
        ExprKind::Field(place, _)
        | ExprKind::Index(place, _)
        | ExprKind::Dereference(place, _)
        | ExprKind::Slice { value: place, .. } => is_place_path(place),
        _ => false,
    }
}
