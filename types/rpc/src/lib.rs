use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{value::RawValue, Value};

// jsonrpc-v2 request object emulation
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct JsonRpcRequestObject {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    pub method: Box<str>,
    pub params: Option<InnerParams>,
    #[serde(deserialize_with = "JsonRpcRequestObject::deserialize_id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Option<Id>>,
}

fn default_jsonrpc() -> String {
    "2.0".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InnerParams {
    Value(Value),
    Raw(Box<RawValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Num(i64),
    Str(Box<str>),
    Null,
}

impl JsonRpcRequestObject {
    fn deserialize_id<'de, D>(deserializer: D) -> Result<Option<Option<Id>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(Option::deserialize(deserializer)?))
    }
}
