// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! # Custom lints
//!
//! A simple, syntactical custom linting framework for forest, to unconditionally
//! forbid certain constructs.
//!
//! Excessive custom lints can be a codebase hazard, so careful consideration is
//! required for what to lint.
//!
//! Out of scope for the current design:
//! - Any conditionality.
//!   We intentionally don't support any `#[allow(..)]`-type gating.
//! - Resolved types, modules.
//! - Cross-file scope.
//!
//! ## Alternative designs.
//!
//! [`clippy`](https://github.com/rust-lang/rust-clippy/) can handle all of the
//! "out of scope" points above.
//! But is a lot more heavyweight, as is the similar project [`dylint`](https://github.com/trailofbits/dylint).
//!
//! If we need more functionality, we should consider porting.
//!
//! ## Technical overview
//!
//! - We parse `rustc`'s Makefile-style dependency files to know which source files
//!   to lint.
//!   This means that new, e.g. `examples/...` artifacts don't need special handling.
//! - We use [`syn`] to parse source files into an Abstract Syntax Tree.
//!   These are inputs to the custom linters, which are run on each file.
//!   Linters return [`proc_macro2::Span`]s to point to lint violations.
//! - We use [`ariadne`] to format violations into pretty `rustc`-style error
//!   messages.
//!   This involves converting [`proc_macro2::Span`]s to utf-8 character offsets
//!   into the file.

mod lints;

use std::{fs, ops::Range};

use ariadne::{Color, ReportKind, Source};
use cargo_metadata::camino::{Utf8Path, Utf8PathBuf};
use lints::{Lint, Violation};
use proc_macro2::{LineColumn, Span};
use syn::visit::Visit;
use tracing::{debug, info, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, util::SubscriberInitExt as _};

#[test]
fn lint() {
    let _guard = tracing_subscriber::fmt()
        .without_time()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::DEBUG.into())
                .from_env()
                .unwrap(),
        )
        .set_default();
    LintRunner::new()
        .run::<lints::NoTestsWithReturn>()
        .run::<lints::SpecializedAssertions>()
        .run_comment_linter()
        .finish();
}

#[must_use = "you must drive the runner to completion"]
struct LintRunner {
    files: Cache,
    num_violations: usize,
}

impl LintRunner {
    /// Performs source file discovery and parsing.
    pub fn new() -> Self {
        info!("collecting source files...");

        // The initial implementation here tried to ask `cargo` and `rustc`
        // what the source files were, but it was flaky.
        //
        // So just go for a simple globbing of well-known directories.

        let files = [
            concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/**/*.rs"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/**/*.rs"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/benches/**/*.rs"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples/**/*.rs"),
        ]
        .into_iter()
        .map(glob::glob)
        .flat_map(|it| {
            it.expect("patterns above are valid")
                .map(|it| it.expect("couldn't compare globbed path with pattern"))
        })
        .map(|path| {
            debug!(?path, "import file");
            // skip files we can't read or aren't syntactically valid
            let path = Utf8PathBuf::from_path_buf(path).unwrap();
            let s = fs::read_to_string(&path).expect("couldn't read file");
            let s = SourceFile::try_from(s).expect("couldn't parse file");
            (path, s)
        })
        .collect::<Cache>();

        info!(num_source_files = files.map.len());

        Self {
            files,
            num_violations: 0,
        }
    }

    /// Run the given linter.
    ///
    /// This prints out any messages, and updates the internal failure count.
    pub fn run<T: for<'a> Visit<'a> + Default + Lint>(mut self) -> Self {
        info!("running {}", std::any::type_name::<T>());
        let mut linter = T::default();
        let mut all_violations = vec![];
        for (path, SourceFile { linewise, ast, .. }) in self.files.map.iter() {
            linter.visit_file(ast);
            for Violation {
                span,
                message,
                color,
            } in linter.flush()
            {
                let mut label = ariadne::Label::new((path, span2span(linewise, span)));
                if let Some(message) = message {
                    label = label.with_message(message)
                }
                if let Some(color) = color {
                    label = label.with_color(color)
                }
                all_violations.push(label)
            }
        }
        let num_violations = all_violations.len();
        let auto = Utf8PathBuf::new(); // ariadne figures out the file label if it doesn't have one
        let mut builder = ariadne::Report::build(ReportKind::Error, (&auto, 0..1))
            .with_labels(all_violations)
            .with_message(T::DESCRIPTION);
        if let Some(help) = T::HELP {
            builder.set_help(help)
        }
        if let Some(note) = T::NOTE {
            builder.set_note(note)
        }
        match num_violations {
            _none @ 0 => {}
            _mid @ 1..=20 => {
                builder.finish().print(&self.files).unwrap();
            }
            _many => {
                builder
                    .with_config(ariadne::Config::default().with_compact(true))
                    .finish()
                    .print(&self.files)
                    .unwrap();
            }
        }
        self.num_violations += num_violations;
        self
    }
    /// Panics with an appropriate error message on failure
    pub fn finish(self) {
        match self.num_violations {
            0 => {
                println!("no violations found in {} files", self.files.map.len());
            }
            nonzero => {
                panic!(
                    "found {} violations in {} files",
                    nonzero,
                    self.files.map.len()
                );
            }
        }
    }
}

impl LintRunner {
    /// Special case comments because:
    /// - They operate on a concrete syntax tree.
    ///   (This is because `rustc`'s lexer diregards comments).
    /// - We get byteoffset spans from [`rowan`](https://docs.rs/rowan/latest/rowan),
    ///   not char-offset spans.
    pub fn run_comment_linter(mut self) -> Self {
        use ra_ap_syntax::{AstNode as _, AstToken as _, ast};
        use regex_automata::{Anchored, Input, meta::Regex};
        info!("linting comments");
        let mut all_violations = vec![];
        let finder = Regex::new("(TODO)|(XXX)|(FIXME)").unwrap();
        let checker = Regex::new(r"TODO\(.*\): https://github.com/").unwrap();
        for (path, SourceFile { plaintext, .. }) in self.files.map.iter() {
            for comment in
                ra_ap_syntax::SourceFile::parse(plaintext, ra_ap_syntax::Edition::Edition2021)
                    .tree()
                    .syntax() // downcast from AST to untyped syntax tree
                    .descendants_with_tokens() // comments are tokens
                    .filter_map(|it| it.into_token().and_then(ast::Comment::cast))
            {
                let haystack = comment.text();
                for found in finder.find_iter(haystack) {
                    if !checker.is_match(
                        Input::new(comment.text())
                            .range(found.start()..)
                            .anchored(Anchored::Yes),
                    ) {
                        let byte_offset_of_comment_in_file =
                            usize::from(comment.syntax().text_range().start());
                        let byte_offset_of_todo_in_comment = found.start();
                        let byte_offset_needle =
                            byte_offset_of_comment_in_file + byte_offset_of_todo_in_comment;
                        let char_offset = plaintext
                            .char_indices()
                            .enumerate()
                            .find_map(|(char_offset, (byte_offset_haystack, _char))| {
                                (byte_offset_haystack == byte_offset_needle).then_some(char_offset)
                            })
                            .unwrap();
                        all_violations.push(
                            ariadne::Label::new((
                                path,
                                char_offset..char_offset + (haystack[found.range()].len()),
                            ))
                            .with_color(Color::Red),
                        );
                    }
                }
            }
        }
        let num_violations = all_violations.len();
        let auto = Utf8PathBuf::new(); // ariadne figures out the file label if it doesn't have one
        let builder = ariadne::Report::build(ReportKind::Error, (&auto, 0..1))
            .with_labels(all_violations)
            .with_message("TODOs must have owners and tracking issues")
            .with_help("Change these to be `TODO(<owner>): https://github.com/ChainSafe/forest/issues/<issue>");
        match num_violations {
            0 => {}
            _ => {
                builder
                    .with_config(ariadne::Config::default().with_compact(true))
                    .finish()
                    .print(&self.files)
                    .unwrap();
            }
        }
        self.num_violations += num_violations;
        self
    }
}

struct SourceFile {
    plaintext: String,
    /// For formatting.
    linewise: ariadne::Source,
    /// Abstract syntax tree.
    ast: syn::File,
}

impl TryFrom<String> for SourceFile {
    type Error = syn::Error;

    fn try_from(plaintext: String) -> Result<Self, Self::Error> {
        Ok(Self {
            ast: syn::parse_file(&plaintext)?,
            linewise: ariadne::Source::from(plaintext.clone()),
            plaintext,
        })
    }
}

/// Stores all the files for repeated linting and formatting into pretty reports
struct Cache {
    map: ahash::HashMap<Utf8PathBuf, SourceFile>,
}

impl<Id> ariadne::Cache<Id> for &Cache
where
    Id: AsRef<str>,
{
    type Storage = String;

    fn fetch(&mut self, id: &Id) -> Result<&Source<Self::Storage>, impl std::fmt::Debug> {
        fn id_not_found_error(id: impl AsRef<str>) -> Box<dyn std::fmt::Debug> {
            Box::new(format!("{} not in cache", id.as_ref()))
        }

        self.map
            .get(Utf8Path::new(&id))
            .map(|SourceFile { linewise, .. }| linewise)
            .ok_or_else(|| id_not_found_error(id))
    }

    fn display<'a>(&self, id: &'a Id) -> Option<impl std::fmt::Display + 'a> {
        Some(Box::new(id.as_ref()))
    }
}

impl FromIterator<(Utf8PathBuf, SourceFile)> for Cache {
    fn from_iter<T: IntoIterator<Item = (Utf8PathBuf, SourceFile)>>(iter: T) -> Self {
        Self {
            map: iter.into_iter().collect(),
        }
    }
}

fn span2span(text: &Source, span: Span) -> Range<usize> {
    coord2offset(text, span.start())..coord2offset(text, span.end())
}

fn coord2offset(text: &Source, coord: LineColumn) -> usize {
    let line = text.line(coord.line - 1).expect("line is past end of file");
    assert!(coord.column <= line.len(), "column is past end of line");
    line.offset() + coord.column
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[should_panic = "found 3 violations in 1 files"]
    fn should_lint_bad_comments() {
        LintRunner {
            files: Cache::from_iter([(
                Utf8PathBuf::from("test.rs"),
                SourceFile::try_from(String::from(
                    "
                    // TODO
                    const _: () = {};
                    fn foo() {
                        /* FIXME */
                    }
                    // XXX: a comment left by David
                    mod bar;
                    ",
                ))
                .unwrap(),
            )]),
            num_violations: 0,
        }
        .run_comment_linter()
        .finish();
    }

    #[test]
    fn should_not_lint_good_comments() {
        LintRunner {
            files: Cache::from_iter([(
                Utf8PathBuf::from("test.rs"),
                SourceFile::try_from(String::from(
                    "// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/1234",
                ))
                .unwrap(),
            )]),
            num_violations: 0,
        }
        .run_comment_linter()
        .finish();
    }
}
