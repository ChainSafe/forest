use std::collections::VecDeque;

use itertools::Itertools as _;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    jsonrpc_types::{Error, RequestParameters},
    openrpc_types::ParamStructure,
    util::Optional as _,
};

pub fn check_args<const N: usize>(names: [&str; N], optional: [bool; N]) {
    let duplicates = names.into_iter().duplicates().collect::<Vec<_>>();
    if !duplicates.is_empty() {
        panic!("duplicate param names: [{}]", duplicates.join(", "))
    }
    for (ix, (left, right)) in optional.into_iter().tuple_windows().enumerate() {
        if left && !right {
            panic!(
                "mandatory param `{}` follows optional param `{}` at index {}",
                names[ix + 1],
                names[ix],
                ix
            )
        }
    }
}

#[derive(Debug)]
pub struct Parser<'a> {
    params: Option<ParserInner>,
    /// What arguments do we expect to parse?
    argument_names: &'a [&'a str],
    /// How many times has the user called us so far?
    call_count: usize,
    has_errored: bool,
}

#[derive(Debug)]
enum ParserInner {
    ByPosition(VecDeque<Value>), // for O(1) pop_front
    ByName(serde_json::Map<String, Value>),
}

impl Drop for Parser<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() && !self.has_errored {
            assert!(
                self.call_count >= self.argument_names.len(),
                "`Parser` has unhandled parameters - did you forget to call `parse`?"
            );
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(
        params: Option<RequestParameters>,
        names: &'a [&'a str],
        calling_convention: ParamStructure,
    ) -> Result<Self, Error> {
        Self::_new(params, names, calling_convention).map_err(Into::into)
    }
    fn _new(
        params: Option<RequestParameters>,
        names: &'a [&'a str],
        calling_convention: ParamStructure,
    ) -> Result<Self, ParseError> {
        let params = match (params, calling_convention) {
            // ignore the calling convention if there are no arguments to parse
            (None, _) => None,
            (Some(params), _) if names.is_empty() && params.is_empty() => None,
            // mutually exclusive
            (Some(RequestParameters::ByPosition(_)), ParamStructure::ByName) => {
                return Err(ParseError::MustBeNamed)
            }
            (Some(RequestParameters::ByName(_)), ParamStructure::ByPosition) => {
                return Err(ParseError::MustBePositional)
            }
            // `parse` won't be called, so do additional checks here
            (Some(RequestParameters::ByPosition(it)), _) if names.is_empty() && !it.is_empty() => {
                return Err(ParseError::UnexpectedPositional(it.len()))
            }
            (Some(RequestParameters::ByName(it)), _) if names.is_empty() && !it.is_empty() => {
                return Err(ParseError::UnexpectedNamed(
                    it.into_iter().map(|(it, _)| it).collect(),
                ))
            }
            (Some(RequestParameters::ByPosition(it)), _) => {
                Some(ParserInner::ByPosition(VecDeque::from(it)))
            }
            (Some(RequestParameters::ByName(it)), _) => Some(ParserInner::ByName(it)),
        };

        Ok(Self {
            params,
            argument_names: names,
            call_count: 0,
            has_errored: false,
        })
    }
    fn error<T>(&mut self, e: ParseError<'a>) -> Result<T, ParseError<'a>> {
        self.has_errored = true;
        Err(e)
    }
    pub fn parse<T>(&mut self) -> Result<T, Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        self._parse().map_err(Into::into)
    }
    fn _parse<T>(&mut self) -> Result<T, ParseError<'a>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let index = self.call_count;
        self.call_count += 1;
        let name = match self.argument_names.get(index) {
            Some(it) => *it,
            None => panic!(
                "`Parser` was initialized with {} arguments, but `parse` was called {} times",
                self.argument_names.len(),
                self.call_count
            ),
        };
        let ty = std::any::type_name::<T>();
        let missing_parameter = ParseError::Missing { index, name, ty };
        let deserialize_error = |error| ParseError::Deser {
            index,
            name,
            ty,
            error,
        };
        let t = match &mut self.params {
            None => match T::optional() {
                true => T::unwrap_none(),
                false => self.error(missing_parameter)?,
            },
            Some(ParserInner::ByName(it)) => match it.remove(name) {
                Some(it) => match serde_json::from_value::<T>(it) {
                    Ok(it) => it,
                    Err(e) => self.error(deserialize_error(e))?,
                },
                None => match T::optional() {
                    true => T::unwrap_none(),
                    false => self.error(missing_parameter)?,
                },
            },
            Some(ParserInner::ByPosition(it)) => match it.pop_front() {
                Some(it) => match serde_json::from_value::<T>(it) {
                    Ok(it) => it,
                    Err(e) => self.error(deserialize_error(e))?,
                },
                None => match T::optional() {
                    true => T::unwrap_none(),
                    false => self.error(missing_parameter)?,
                },
            },
        };
        let final_arg = self.call_count >= self.argument_names.len();
        if final_arg {
            match self.params.take() {
                Some(ParserInner::ByName(it)) => match it.is_empty() {
                    true => {}
                    false => self.error(ParseError::UnexpectedNamed(
                        it.into_iter().map(|(k, _)| k).collect(),
                    ))?,
                },
                Some(ParserInner::ByPosition(it)) => match it.len() {
                    0 => {}
                    n => self.error(ParseError::UnexpectedPositional(n))?,
                },
                None => {}
            };
        }
        Ok(t)
    }
}

/// Broken out error type for writing tests
#[derive(Debug)]
enum ParseError<'a> {
    Missing {
        index: usize,
        name: &'a str,
        ty: &'a str,
    },
    Deser {
        index: usize,
        name: &'a str,
        ty: &'a str,
        error: serde_json::Error,
    },
    UnexpectedPositional(usize),
    UnexpectedNamed(Vec<String>),
    MustBeNamed,
    MustBePositional,
}

impl<'a> From<ParseError<'a>> for Error {
    fn from(value: ParseError<'a>) -> Self {
        match value {
            ParseError::Missing { index, name, ty } => Error::invalid_params(
                "missing required parameter",
                json!({
                    "index": index,
                    "name": name,
                    "type": ty
                }),
            ),
            ParseError::Deser {
                index,
                name,
                ty,
                error,
            } => Error::invalid_params(
                "error deserializing parameter",
                json!({
                    "index": index,
                    "name": name,
                    "type": ty,
                    "error": error.to_string()
                }),
            ),
            ParseError::UnexpectedPositional(n) => {
                Error::invalid_params("unexpected trailing arguments", json!({"count": n}))
            }
            ParseError::UnexpectedNamed(names) => {
                Error::invalid_params("unexpected named arguments", json!(names))
            }
            ParseError::MustBeNamed => {
                Error::invalid_params("this method only accepts arguments by-name", None)
            }
            ParseError::MustBePositional => {
                Error::invalid_params("this method only accepts arguments by-position", None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::from_value;

    #[test]
    fn optional() {
        // no params where optional
        let mut parser = Parser::_new(None, &["p0"], ParamStructure::Either).unwrap();
        assert_eq!(None::<i32>, parser._parse().unwrap());

        // positional optional
        let mut parser = Parser::_new(from_value!([]), &["opt"], ParamStructure::Either).unwrap();
        assert_eq!(None::<i32>, parser._parse().unwrap());

        // named optional
        let mut parser = Parser::_new(from_value!({}), &["opt"], ParamStructure::Either).unwrap();
        assert_eq!(None::<i32>, parser._parse().unwrap());

        // postional optional with mandatory
        let mut parser =
            Parser::_new(from_value!([0]), &["p0", "opt"], ParamStructure::Either).unwrap();
        assert_eq!(Some(0), parser._parse().unwrap());
        assert_eq!(None::<i32>, parser._parse().unwrap());

        // named optional with mandatory
        let mut parser = Parser::_new(
            from_value!({"p0": 0}),
            &["p0", "opt"],
            ParamStructure::Either,
        )
        .unwrap();
        assert_eq!(Some(0), parser._parse().unwrap());
        assert_eq!(None::<i32>, parser._parse().unwrap());
    }

    #[test]
    fn missing() {
        // missing only named
        let mut parser = Parser::_new(from_value!({}), &["p0"], ParamStructure::Either).unwrap();
        assert!(matches!(
            parser._parse::<i32>().unwrap_err(),
            ParseError::Missing { name: "p0", .. },
        ));

        // missing only positional
        let mut parser = Parser::_new(from_value!([]), &["p0"], ParamStructure::Either).unwrap();
        assert!(matches!(
            parser._parse::<i32>().unwrap_err(),
            ParseError::Missing { name: "p0", .. },
        ));

        // missing a named
        let mut parser = Parser::_new(
            from_value!({"p0": 0}),
            &["p0", "p1"],
            ParamStructure::Either,
        )
        .unwrap();
        assert_eq!(0, parser._parse::<i32>().unwrap());
        assert!(matches!(
            parser._parse::<i32>().unwrap_err(),
            ParseError::Missing { name: "p1", .. },
        ));

        // missing a positional
        let mut parser =
            Parser::_new(from_value!([0]), &["p0", "p1"], ParamStructure::Either).unwrap();
        assert_eq!(0, parser._parse::<i32>().unwrap());
        assert!(matches!(
            parser._parse::<i32>().unwrap_err(),
            ParseError::Missing { name: "p1", .. },
        ));
    }

    #[test]
    fn unexpected() {
        // named but expected none
        assert!(matches!(
            Parser::_new(from_value!({ "surprise": () }), &[], ParamStructure::Either).unwrap_err(),
            ParseError::UnexpectedNamed(it) if it == ["surprise"],
        ));

        // positional but expected none
        assert!(matches!(
            Parser::_new(from_value!(["surprise"]), &[], ParamStructure::Either).unwrap_err(),
            ParseError::UnexpectedPositional(1),
        ));

        // named after one
        let mut parser = Parser::_new(
            from_value!({ "p0": 0, "surprise": () }),
            &["p0"],
            ParamStructure::Either,
        )
        .unwrap();
        assert!(matches!(
            parser._parse::<i32>().unwrap_err(),
            ParseError::UnexpectedNamed(it) if it == ["surprise"]
        ));

        // positional after one
        let mut parser = Parser::_new(
            from_value!([1, "surprise"]),
            &["p0"],
            ParamStructure::Either,
        )
        .unwrap();
        assert!(matches!(
            parser._parse::<i32>().unwrap_err(),
            ParseError::UnexpectedPositional(1),
        ));
    }

    #[test]
    #[should_panic = "`Parser` was initialized with 0 arguments, but `parse` was called 1 times"]
    fn called_too_much() {
        let mut parser = Parser::_new(None, &[], ParamStructure::Either).unwrap();
        let _ = parser._parse::<()>();
        unreachable!()
    }

    #[test]
    #[should_panic = "`Parser` has unhandled parameters - did you forget to call `parse`?"]
    fn called_too_little() {
        Parser::_new(None, &["p0"], ParamStructure::Either).unwrap();
    }
}
