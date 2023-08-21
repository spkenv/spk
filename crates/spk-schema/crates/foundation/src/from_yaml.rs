// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use format_serde_error::SerdeError;

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
        serde_yaml::from_str(&yaml).map_err(|err| {
            let location = err.location();

            SerdeError::new(
                yaml,
                // Until format_serde_error supports serde_yaml 0.9, we need to
                // make use of ErrorTypes::Custom, using code similar to how
                // serde_yaml 0.8 is implemented.
                (
                    Box::new(err) as Box<dyn std::error::Error>,
                    location.as_ref().map(|l| l.line()),
                    location.as_ref().map(|l| l.column() - 1),
                ),
            )
        })
    }
}
