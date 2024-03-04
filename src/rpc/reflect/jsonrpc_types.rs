//! A transcription of types from the [`JSON-RPC 2.0` Specification](https://www.jsonrpc.org/specification).
//!
//! > When quoted, the specification will appear as blockquoted text, like so.

use std::{
    borrow::Cow,
    fmt::{self, Display},
    ops::RangeInclusive,
};

use serde::{
    de::{Error as _, IntoDeserializer as _, Unexpected, Visitor},
    Deserialize, Serialize,
};
use serde_json::{Map, Number, Value};

/// A `JSON-RPC 2.0` request object.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// > A String specifying the version of the JSON-RPC protocol.
    /// > MUST be exactly "2.0".
    pub jsonrpc: V2,
    /// > A String containing the name of the method to be invoked.
    /// > Method names that begin with the word rpc followed by a period character
    /// > (U+002E or ASCII 46) are reserved for rpc-internal methods and extensions
    /// > and MUST NOT be used for anything else.
    pub method: String,
    /// > A Structured value that holds the parameter values to be used during the
    /// > invocation of the method.
    /// > This member MAY be omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<RequestParameters>,
    /// > An identifier established by the Client that MUST contain a String,
    /// > Number, or NULL value if included.
    /// > If it is not included it is assumed to be a notification.
    /// > The value SHOULD normally not be Null and Numbers SHOULD NOT contain fractional parts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            jsonrpc: V2,
            method: String,
            #[serde(default, deserialize_with = "deserialize_some")]
            params: Option<Option<RequestParameters>>,
            #[serde(default, deserialize_with = "deserialize_some")]
            pub id: Option<Option<Id>>,
        }
        let Helper {
            jsonrpc,
            method,
            params,
            id,
        } = Helper::deserialize(deserializer)?;
        Ok(Self {
            jsonrpc,
            method,
            params: match params {
                Some(Some(params)) => Some(params),
                // Be lenient in what we accept
                // Some(None) => return Err(D::Error::custom("`params` may not be `null`")),
                Some(None) => None,
                None => None,
            },
            id: match id {
                Some(Some(id)) => Some(id),
                Some(None) => Some(Id::Null),
                None => None,
            },
        })
    }
}

/// A witness of the literal string "2.0"
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct V2;

impl<'de> Deserialize<'de> for V2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match &*Cow::<str>::deserialize(deserializer)? {
            "2.0" => Ok(Self),
            other => Err(D::Error::invalid_value(Unexpected::Str(other), &"2.0")),
        }
    }
}

impl Serialize for V2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("2.0")
    }
}

/// > If present, parameters for the rpc call MUST be provided as a Structured value.
/// > Either by-position through an Array or by-name through an Object.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)] // TODO(aatifsyed): manually implement Deserialize for a better error message
pub enum RequestParameters {
    /// > params MUST be an Array, containing the values in the Server expected order.
    ByPosition(Vec<Value>),
    /// > params MUST be an Object, with member names that match the Server
    /// > expected parameter names.
    /// > The absence of expected names MAY result in an error being generated.
    /// > The names MUST match exactly, including case, to the method's expected parameters.
    ByName(Map<String, Value>),
}

impl RequestParameters {
    pub fn len(&self) -> usize {
        match self {
            RequestParameters::ByPosition(it) => it.len(),
            RequestParameters::ByName(it) => it.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        match self {
            RequestParameters::ByPosition(it) => it.is_empty(),
            RequestParameters::ByName(it) => it.is_empty(),
        }
    }
}

/// See [`Request::id`].
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Id {
    String(String),
    Number(Number),
    Null,
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct IdVisitor;

        impl<'de> Visitor<'de> for IdVisitor {
            type Value = Id;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string, a number, or null")
            }

            fn visit_i64<E>(self, value: i64) -> Result<Id, E> {
                Ok(Id::Number(value.into()))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Id, E> {
                Ok(Id::Number(value.into()))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Id, E> {
                Ok(Number::from_f64(value).map_or(Id::Null, Id::Number))
            }

            fn visit_str<E>(self, value: &str) -> Result<Id, E>
            where
                E: serde::de::Error,
            {
                self.visit_string(String::from(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Id, E> {
                Ok(Id::String(value))
            }

            fn visit_none<E>(self) -> Result<Id, E> {
                Ok(Id::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Id, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_unit<E>(self) -> Result<Id, E> {
                Ok(Id::Null)
            }
        }

        deserializer.deserialize_any(IdVisitor)
    }
}

/// A `JSON-RPC 2.0` response object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// > A String specifying the version of the JSON-RPC protocol.
    /// > MUST be exactly "2.0".
    pub jsonrpc: V2,
    /// > "result":
    /// >
    /// > This member is REQUIRED on success.
    /// > This member MUST NOT exist if there was an error invoking the method.
    /// > The value of this member is determined by the method invoked on the Server.
    /// >
    /// > "error":
    /// >
    /// > This member is REQUIRED on error.
    /// > This member MUST NOT exist if there was no error triggered during invocation.
    pub result: Result<Value, Error>,
    /// > This member is REQUIRED.
    /// > It MUST be the same as the value of the id member in the Request Object.
    /// > If there was an error in detecting the id in the Request object
    /// > (e.g. Parse error/Invalid Request), it MUST be Null.
    pub id: Id,
}

#[derive(Serialize, Deserialize)]
struct RawResponseDeSer {
    jsonrpc: V2,
    #[serde(default, deserialize_with = "deserialize_some")]
    result: Option<Option<Value>>,
    #[serde(default)]
    error: Option<Error>,
    id: Id,
}
/// Distinguish between absent and present but null.
///
/// See <https://github.com/serde-rs/serde/issues/984#issuecomment-314143738>
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::de::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

impl Serialize for Response {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let Self {
            jsonrpc,
            result,
            id,
        } = self.clone();
        let helper = match result {
            Ok(result) => RawResponseDeSer {
                jsonrpc,
                result: Some(Some(result)),
                error: None,
                id,
            },
            Err(error) => RawResponseDeSer {
                jsonrpc,
                result: None,
                error: Some(error),
                id,
            },
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let RawResponseDeSer {
            jsonrpc,
            error,
            result,
            id,
        } = RawResponseDeSer::deserialize(deserializer)?;
        match (result, error) {
            (Some(ok), None) => Ok(Response {
                jsonrpc,
                result: Ok(ok.unwrap_or_default()),
                id,
            }),
            (None, Some(err)) => Ok(Response {
                jsonrpc,
                result: Err(err),
                id,
            }),
            (Some(_), Some(_)) => Err(D::Error::custom(
                "only ONE of `error` and `result` may be present",
            )),
            (None, None) => Err(D::Error::custom("must have an `error` or `result` member")),
        }
    }
}

/// A `JSON-RPC 2.0` error object.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// > A Number that indicates the error type that occurred.
    /// > This MUST be an integer.
    ///
    /// See the associated constants for error types defined by the specification.
    pub code: i64,
    /// > A String providing a short description of the error.
    /// > The message SHOULD be limited to a concise single sentence.
    pub message: String,
    /// > A Primitive or Structured value that contains additional information about the error.
    /// > This may be omitted.
    /// > The value of this member is defined by the Server
    /// > (e.g. detailed error information, nested errors etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

macro_rules! error_code_and_ctor {
    (
        $(
            $(#[doc = $doc:literal])*
            $const_name:ident / $ctor_name:ident = $number:literal;
        )*
    ) => {
        $(
            $(#[doc = $doc])*
            pub const $const_name: i64 = $number;
        )*

        $(
            #[doc = concat!("Convenience method for creating a new error with code [`Self::", stringify!($const_name), "`]")]
            pub fn $ctor_name(message: impl Display, data: impl Into<Option<Value>>) -> Self {
                Self::new(Self::$const_name, message, data)
            }
        )*
    };
}

impl Error {
    error_code_and_ctor! {
            /// > Invalid JSON was received by the server. An error occurred on the server while parsing the JSON text.
            PARSE_ERROR / parse_error = -32700;
            /// > The JSON sent is not a valid Request object.
            INVALID_REQUEST / invalid_request = -32600;
            /// > The method does not exist / is not available.
            METHOD_NOT_FOUND / method_not_found = -32601;
            /// > Invalid method parameter(s).
            INVALID_PARAMS / invalid_params = -32602;
            /// > Internal JSON-RPC error.
            INTERNAL_ERROR / internal_error = -32603;

    }

    /// > Reserved for implementation-defined server-errors.
    pub const SERVER_ERROR_RANGE: RangeInclusive<i64> = -32099..=-32000;

    /// Convenience method for creating a new error.
    pub fn new(code: i64, message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self {
            code,
            message: message.to_string(),
            data: data.into(),
        }
    }
}

impl<'de> Deserialize<'de> for Error {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            code: i64,
            message: String,
            #[serde(default, deserialize_with = "deserialize_some")]
            data: Option<Option<Value>>,
        }
        let Helper {
            code,
            message,
            data,
        } = Helper::deserialize(deserializer)?;
        Ok(Self {
            code,
            message,
            data: match data {
                Some(Some(value)) => Some(value),
                Some(None) => Some(Value::Null),
                None => None,
            },
        })
    }
}

/// A response to a [`MaybeBatchedRequest`].
pub enum MaybeBatchedResponse {
    Single(Response),
    Batch(Vec<Response>),
}

/// > To send several Request objects at the same time, the Client MAY send an Array filled with Request objects.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum MaybeBatchedRequest {
    Single(Request),
    Batch(Vec<Request>),
}

impl<'de> Deserialize<'de> for MaybeBatchedRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Err(
                D::Error::custom("expected a request object, or an array of request objects"),
            ),
            it @ Value::Array(_) => Ok(Self::Batch(
                Vec::<Request>::deserialize(it.into_deserializer()).map_err(D::Error::custom)?,
            )),
            it @ Value::Object(_) => Ok(Self::Single(
                Request::deserialize(it.into_deserializer()).map_err(D::Error::custom)?,
            )),
        }
    }
}
