// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Package, SourceSpec};
use crate::{Error, Result};

/// Specifies some data object within the spk ecosystem.
///
/// All build-able and resolve-able types have a spec representation
/// that can be serialized and deserialized from a human-written
/// file or machine-managed persistent storage.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "api")]
#[enum_dispatch::enum_dispatch(PackageTemplate)]
pub enum Spec {
    #[serde(rename = "v0/package")]
    V0Package(super::v0::Spec),
}

impl<'de> Deserialize<'de> for Spec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_yaml::Mapping::deserialize(deserializer)?;
        let api_field = serde_yaml::Value::String(String::from("api"));
        // unfortunately, serde does not have a derive mechanism which
        // would allow us to specify a default enum variant for when
        // the 'api' field does not exist in a spec. This small setup will not
        // create as nice of error messages in some cases, but is the
        // best implementation that I could think of without adding a
        // non-trivial maintenance burden to the setup.
        let variant = value
            .remove(&api_field)
            .unwrap_or_else(|| serde_yaml::Value::String(String::from("v0/package")));
        match variant.as_str() {
            Some("v0/package") => Ok(Spec::V0Package(
                serde_yaml::from_value(value.into()).map_err(serde::de::Error::custom)?,
            )),
            Some(variant) => Err(serde::de::Error::custom(format!(
                "Unknown api variant: '{variant}'"
            ))),
            None => Err(serde::de::Error::custom(
                "Invalid value for field 'api', expected string type",
            )),
        }
    }
}

/// ReadSpec loads a package specification from a yaml file.
pub fn read_spec_file<P: AsRef<Path>>(filepath: P) -> Result<Spec> {
    let filepath = filepath.as_ref().canonicalize()?;
    let file = std::fs::File::open(&filepath)?;
    let mut spec: Spec = serde_yaml::from_reader(file)
        .map_err(|err| Error::InvalidPackageSpecFile(filepath.clone(), err))?;
    if let Some(spec_root) = filepath.parent() {
        match &mut spec {
            Spec::V0Package(spec) => {
                for source in spec.sources.iter_mut() {
                    if let SourceSpec::Local(source) = source {
                        source.path = spec_root.join(&source.path);
                    }
                }
            }
        }
    }

    Ok(spec)
}

/// Save the given spec to a file.
pub fn save_spec_file<P: AsRef<Path>>(filepath: P, spec: &Spec) -> crate::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(filepath)?;
    serde_yaml::to_writer(file, spec).map_err(Error::SpecEncodingError)?;
    Ok(())
}

impl AsRef<Spec> for Spec {
    fn as_ref(&self) -> &Spec {
        self
    }
}
