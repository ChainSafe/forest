// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{any::type_name, borrow::Cow};

pub fn short_type_name<T>() -> Cow<'static, str> {
    let prefix_pattern = lazy_regex::regex!(r#"(([^,:<>]+::)|\s)*"#);
    let n = type_name::<T>();
    prefix_pattern.replace_all(n, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::io::Result as IoResult;

    #[test]
    fn test_short_type_name() {
        assert_eq!(short_type_name::<String>(), "String");
        assert_eq!(short_type_name::<Option<String>>(), "Option<String>");
        assert_eq!(
            short_type_name::<IoResult<Option<String>>>(),
            "Result<Option<String>,Error>"
        );
        assert_eq!(short_type_name::<Cow<'static, str>>(), "Cow<str>");
        assert_eq!(
            short_type_name::<IoResult<Cow<'static, str>>>(),
            "Result<Cow<str>,Error>"
        );
    }
}
