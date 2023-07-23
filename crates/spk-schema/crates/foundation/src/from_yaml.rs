// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use format_serde_error::SerdeError;

/// Convert a serde_yaml::Error (0.9) into a SerdeError.
pub struct SerdeYamlError(pub serde_yaml::Error);

impl From<SerdeYamlError> for format_serde_error::ErrorTypes {
    fn from(err: SerdeYamlError) -> Self {
        // Until format_serde_error supports serde_yaml 0.9, we need to make
        // use of ErrorTypes::Custom, using code similar to how serde_yaml 0.8
        // is implemented.
        let location = err.0.location();
        Self::Custom {
            error: Box::new(err.0),
            line: location.as_ref().map(|l| l.line()),
            column: location.as_ref().map(|l| l.column() - 1),
        }
    }
}

/// A type that can be deserialized from a yaml document
pub trait FromYaml: Sized {
    /// Deserialize the given yaml as an instance of this type
    fn from_yaml<S: Into<String>>(yaml: S) -> Result<Self, SerdeError>;
}

impl<T> FromYaml for T
where
    T: serde::de::DeserializeOwned,
{
    fn from_yaml<S: Into<String>>(yaml: S) -> Result<Self, SerdeError> {
        let yaml = yaml.into();
        serde_yaml::from_str(&yaml).map_err(|err| SerdeError::new(yaml, SerdeYamlError(err)))
    }
}
