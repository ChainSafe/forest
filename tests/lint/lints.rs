use std::{fmt::Display, mem};

use ariadne::Color;
use proc_macro2::Span;
use syn::{
    parse_quote,
    spanned::Spanned,
    visit::{self, Visit},
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
    spans: Vec<Violation>,
}

impl<'ast> Visit<'ast> for NoTestsWithReturn {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if i.attrs.iter().any(|attr| attr == &parse_quote!(#[test])) {
            if let syn::ReturnType::Type(..) = i.sig.output {
                self.spans.push(
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
        mem::take(&mut self.spans)
    }

    const DESCRIPTION: &'static str = "`#[test]` functions are not allowed to have a return type";
    const NOTE: Option<&'static str> =
        Some("`assert`s and `unwrap`s provide better error messages in tests");
    const HELP: Option<&'static str> =
        Some("Remove the return type, and any `?` error propogations");
}
