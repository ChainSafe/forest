// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
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

/// A custom linter.
///
/// Names of linters should describe the codebase state that the lint "wants".
/// e.g [`NoTestsWithReturn`]: "there are no tests with return values".
pub trait Lint {
    /// Finish linting a single file.
    ///
    /// The linter must not retain any [`Span`]s after this is called.
    fn flush(&mut self) -> Vec<Violation>;

    /// The top-level explanation of the lint, as a declaration.
    const DESCRIPTION: &'static str;
    /// Why this lint exists.
    const NOTE: Option<&'static str> = None;
    /// What code changes the user should make.
    const HELP: Option<&'static str> = None;
}

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

#[cfg(test)]
#[track_caller]
fn should_lint<T: Default + for<'ast> Visit<'ast> + Lint>(file: syn::File) {
    let mut linter = T::default();
    linter.visit_file(&file);
    assert!(!linter.flush().is_empty())
}

#[cfg(test)]
#[track_caller]
fn should_not_lint<T: Default + for<'ast> Visit<'ast> + Lint>(file: syn::File) {
    let mut linter = T::default();
    linter.visit_file(&file);
    assert!(linter.flush().is_empty())
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
        if i.attrs
            .iter()
            .any(|attr| attr == &parse_quote!(#[test]) || attr == &parse_quote!(#[tokio::test]))
        {
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

#[test]
fn no_tests_with_return() {
    should_lint::<NoTestsWithReturn>(parse_quote! {
        #[test]
        fn foo() -> Bar {
            todo!()
        }
    });
    should_not_lint::<NoTestsWithReturn>(parse_quote! {
        #[test]
        fn foo() {}
        fn bar() -> Bar {}
    });
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
    const NOTE: Option<&'static str> = Some("specialized macros provides better error messages");
}

#[test]
fn specialized_assertions() {
    should_lint::<SpecializedAssertions>(parse_quote! {
        assert!(1 != 2);
    });
    should_lint::<SpecializedAssertions>(parse_quote! {
        assert!(1 == 1, "these should be equal");
    });
    should_not_lint::<SpecializedAssertions>(parse_quote! {
        assert_ne!(1, 2);
    });
    should_not_lint::<SpecializedAssertions>(parse_quote! {
        assert_eq!(1, 2, "these should be equal");
    });
}
