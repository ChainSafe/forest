use std::{fmt::Display, mem};

use ariadne::Color;
use proc_macro2::Span;
use syn::{
    parse_quote,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
    BinOp, Expr, ItemFn, Macro, ReturnType, Token,
};

/// A linting violation
pub struct Violation {
    pub span: Span,
    pub message: Option<String>,
    pub color: Option<Color>,
}

impl Violation {
    pub fn new(span: impl Spanned) -> Self {
        Self {
            span: span.span(),
            message: None,
            color: None,
        }
    }
    pub fn with_message(mut self, msg: impl Display) -> Self {
        self.message = Some(msg.to_string());
        self
    }
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

pub trait Lint {
    /// Finish linting a single file.
    ///
    /// The linter must not retain any [`Span`]s after this is called.
    fn flush(&mut self) -> Vec<Violation>;

    const DESCRIPTION: &'static str;
    const NOTE: Option<&'static str> = None;
    const HELP: Option<&'static str> = None;
}

//////////
// Linters
//////////

#[derive(Default)]
pub struct NoTestsWithReturn {
    violations: Vec<Violation>,
}

impl<'ast> Visit<'ast> for NoTestsWithReturn {
    fn visit_item_fn(&mut self, i: &'ast ItemFn) {
        if i.attrs.iter().any(|attr| attr == &parse_quote!(#[test])) {
            if let ReturnType::Type(..) = i.sig.output {
                self.violations.push(
                    Violation::new(&i.sig.output)
                        .with_message("not allowed to have a return type")
                        .with_color(Color::Red),
                )
            }
        }
        visit::visit_item_fn(self, i)
    }
}

impl Lint for NoTestsWithReturn {
    fn flush(&mut self) -> Vec<Violation> {
        mem::take(&mut self.violations)
    }

    const DESCRIPTION: &'static str = "`#[test]` functions are not allowed to have a return type";
    const NOTE: Option<&'static str> =
        Some("`assert`s and `unwrap`s provide better error messages in tests");
    const HELP: Option<&'static str> =
        Some("Remove the return type, and any `?` error propogations");
}

#[derive(Default)]
pub struct SpecializedAssertions {
    violations: Vec<Violation>,
}

impl<'ast> Visit<'ast> for SpecializedAssertions {
    fn visit_macro(&mut self, i: &'ast Macro) {
        if i.path.is_ident("assert") {
            if let Ok(exprs) = i.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
                if let Some(Expr::Binary(binary)) = exprs.first() {
                    match binary.op {
                        BinOp::Eq(_) => self.violations.push(
                            Violation::new(i)
                                .with_message("should be `assert_eq(..)`")
                                .with_color(Color::Red),
                        ),
                        BinOp::Ne(_) => self.violations.push(
                            Violation::new(i)
                                .with_message("should be `assert_ne(..)`")
                                .with_color(Color::Red),
                        ),
                        _ => {}
                    }
                }
            }
        }
        visit::visit_macro(self, i)
    }
}

impl Lint for SpecializedAssertions {
    fn flush(&mut self) -> Vec<Violation> {
        mem::take(&mut self.violations)
    }

    const DESCRIPTION: &'static str = "`assert!(..)` that should use a more specialized macro";
    const NOTE: Option<&'static str> = Some("specialized macros provides better error messages ");
}
