// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::de::MapAccess;
use serde::Deserialize;
use spk_schema_foundation::name::OptNameBuf;
use spk_schema_foundation::option_map::Stringified;

use crate::option::PkgNameWithComponents;

#[derive(Debug)]
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
#[derive(Deserialize)]
#[serde(untagged)]
pub enum VariantSpecEntryKey {
    PkgOrOpt(PkgNameWithComponentsWithoutVersion),
    Opt(OptNameBuf),
}

#[derive(Default)]
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
