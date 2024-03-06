//! A transcription of types from the [`OpenRPC` Specification](https://spec.open-rpc.org/).
//!
//! > When quoted, the specification will appear as blockquoted text, like so.

use std::collections::HashMap;

use itertools::Itertools as _;
use schemars::schema::Schema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenRPC {
    /// > REQUIRED.
    /// > The available methods for the API.
    /// > While it is required, the array may be empty (to handle security filtering, for example).
    pub methods: Methods,
    /// > An element to hold various schemas for the specification.
    pub components: Components,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]

pub struct Components {
    pub schemas: HashMap<String, Schema>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(transparent)]
pub struct Methods {
    inner: Vec<Method>,
}

#[derive(Debug, Clone, Error)]
#[error("{}", .0)]
pub struct MethodsError(String);

impl Methods {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn just(method: Method) -> Self {
        Self {
            inner: vec![method],
        }
    }
    pub fn new(methods: impl IntoIterator<Item = Method>) -> Result<Self, MethodsError> {
        let inner = methods.into_iter().collect::<Vec<_>>();
        let duplicates = inner
            .iter()
            .map(|it| &it.name)
            .duplicates()
            .collect::<Vec<_>>();
        match duplicates.is_empty() {
            true => Ok(Self { inner }),
            false => Err(MethodsError(format!(
                "the following method names are duplicated: [{}]",
                duplicates.iter().join(", ")
            ))),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Method> {
        self.inner.iter()
    }
}

impl IntoIterator for Methods {
    type Item = Method;

    type IntoIter = std::vec::IntoIter<Method>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Methods {
    type Item = &'a Method;

    type IntoIter = std::slice::Iter<'a, Method>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'de> Deserialize<'de> for Methods {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(Vec::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Method {
    /// > REQUIRED.
    /// > The cannonical name for the method.
    /// > The name MUST be unique within the methods array.
    pub name: String,
    pub params: Params,
    #[serde(default)]
    pub param_structure: ParamStructure,
    /// > The description of the result returned by the method.
    /// > If defined, it MUST be a Content Descriptor or Reference Object.
    /// > If undefined, the method MUST only be used as a notification.
    pub result: Option<ContentDescriptor>,
}

/// > The expected format of the parameters.
/// > As per the JSON-RPC 2.0 specification,
/// > the params of a JSON-RPC request object may be an array, object, or either
/// > (represented as by-position, by-name, and either respectively).
/// > When a method has a paramStructure value of by-name,
/// > callers of the method MUST send a JSON-RPC request object whose params field is an object.
/// > Further, the key names of the params object MUST be the same as the contentDescriptor.names for the given method.
/// > Defaults to "either".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ParamStructure {
    ByName,
    ByPosition,
    #[default]
    Either,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentDescriptor {
    /// > REQUIRED.
    /// > Name of the content that is being described.
    /// > If the content described is a method parameter assignable by-name, this field SHALL define the parameter’s key (ie name).
    pub name: String,
    /// > REQUIRED.
    /// > Schema that describes the content.
    ///
    /// > The Schema Object allows the definition of input and output data types.
    /// > The Schema Objects MUST follow the specifications outline in the JSON Schema Specification 7 Alternatively,
    /// > any time a Schema Object can be used, a Reference Object can be used in its place.
    /// > This allows referencing definitions instead of defining them inline.
    ///
    /// > This object MAY be extended with Specification Extensions.
    pub schema: Schema,
    /// > Determines if the content is a required field. Default value is false.
    #[serde(default)]
    pub required: bool,
}

/// > REQUIRED.
/// > A list of parameters that are applicable for this method.
/// > The list MUST NOT include duplicated parameters and therefore require name to be unique.
/// > The list can use the Reference Object to link to parameters that are defined by the Content Descriptor Object.
/// > All optional params (content descriptor objects with “required”: false) MUST be positioned after all required params in the list.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(transparent)]
pub struct Params {
    inner: Vec<ContentDescriptor>,
}

#[derive(Debug, Clone, Error)]
#[error("{}", .0)]
pub struct ParamListError(String);

impl Params {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn just(param: ContentDescriptor) -> Self {
        Self { inner: vec![param] }
    }
    pub fn new(
        params: impl IntoIterator<Item = ContentDescriptor>,
    ) -> Result<Self, ParamListError> {
        let params = params.into_iter().collect::<Vec<_>>();
        let duplicates = params
            .iter()
            .map(|it| it.name.as_str())
            .duplicates()
            .collect::<Vec<_>>();
        if !duplicates.is_empty() {
            return Err(ParamListError(format!(
                "The following parameter names are duplicated: [{}]",
                duplicates.join(", ")
            )));
        }
        if let Some((first_opt_ix, first_opt_param)) =
            params.iter().enumerate().find(|(_, it)| !it.required)
        {
            let late_mandatory_params = params
                .iter()
                .enumerate()
                .filter(|(ix, it)| it.required && *ix > first_opt_ix)
                .map(|(_, it)| it.name.clone())
                .collect::<Vec<_>>();

            if !late_mandatory_params.is_empty() {
                return Err(ParamListError(
                    String::from("Mandatory parameters may not follow optional parameters,")
                        + &format!(
                            "but the optional parameter {} is followed by [{}]",
                            first_opt_param.name,
                            late_mandatory_params.join(", ")
                        ),
                ));
            }
        };
        Ok(Self { inner: params })
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ContentDescriptor> {
        self.inner.iter()
    }
}

impl IntoIterator for Params {
    type Item = ContentDescriptor;

    type IntoIter = std::vec::IntoIter<ContentDescriptor>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = &'a ContentDescriptor;

    type IntoIter = std::slice::Iter<'a, ContentDescriptor>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'de> Deserialize<'de> for Params {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(Vec::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
