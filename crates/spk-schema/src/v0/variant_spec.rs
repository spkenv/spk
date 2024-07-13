// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use serde::de::MapAccess;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::name::OptNameBuf;
use spk_schema_foundation::option_map::Stringified;

use crate::option::PkgNameWithComponents;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PkgNameWithComponentsWithoutVersion(pub PkgNameWithComponents);

impl TryFrom<PkgNameWithComponents> for PkgNameWithComponentsWithoutVersion {
    type Error = &'static str;

    fn try_from(pkg_name: PkgNameWithComponents) -> Result<Self, Self::Error> {
        if pkg_name.default.is_some() {
            Err("version is not allowed")
        } else {
            Ok(Self(pkg_name))
        }
    }
}

impl<'de> Deserialize<'de> for PkgNameWithComponentsWithoutVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        PkgNameWithComponents::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

/// Accept either a package name that might specify components, or an option name.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum VariantSpecEntryKey {
    PkgOrOpt(PkgNameWithComponentsWithoutVersion),
    Opt(OptNameBuf),
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VariantSpec {
    pub entries: Vec<(VariantSpecEntryKey, Stringified)>,
}

impl<'de> Deserialize<'de> for VariantSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct VariantSpecVisitor;

        impl<'de> serde::de::Visitor<'de> for VariantSpecVisitor {
            type Value = VariantSpec;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a variant specification")
            }

            fn visit_map<M>(self, mut access: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut entries = Vec::with_capacity(access.size_hint().unwrap_or(0));

                while let Some((key, value)) =
                    access.next_entry::<VariantSpecEntryKey, Stringified>()?
                {
                    entries.push((key, value));
                }

                Ok(VariantSpec { entries })
            }
        }

        deserializer.deserialize_map(VariantSpecVisitor)
    }
}

impl Serialize for VariantSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
