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
        serde_yaml::from_str(&yaml).map_err(|err| SerdeError::new(yaml, err))
    }
}
