// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_ident::{PkgRequest, Satisfy};

use super::WhenBlock;
use crate::{BuildEnv, Package};

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ScriptBlock(Vec<ScriptBlockEntry>);

impl ScriptBlock {
    /// Reduce this script to a string, resolving all conditionals
    pub fn to_string<E, P>(&self, build_env: &E) -> String
    where
        E: BuildEnv<Package = P>,
        P: Package + Satisfy<PkgRequest>,
    {
        self.0
            .iter()
            .map(|block| block.to_string(build_env))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl<'de> serde::de::Deserialize<'de> for ScriptBlock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ScriptBlockVisitor;

        impl<'de> serde::de::Visitor<'de> for ScriptBlockVisitor {
            type Value = ScriptBlock;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a script block")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ScriptBlock(vec![ScriptBlockEntry::Simple(v.to_owned())]))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut entries = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(entry) = seq.next_element::<ScriptBlockEntry>()? {
                    entries.push(entry)
                }
                Ok(ScriptBlock(entries))
            }
        }

        deserializer.deserialize_any(ScriptBlockVisitor)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
#[serde(untagged)]
pub enum ScriptBlockEntry {
    Simple(String),
    Conditional {
        #[serde(rename = "do")]
        script: ScriptBlock,
        when: WhenBlock,
    },
}

impl ScriptBlockEntry {
    /// Reduce this script to a string, resolving all conditionals
    pub fn to_string<E, P>(&self, build_env: &E) -> String
    where
        E: BuildEnv<Package = P>,
        P: Package + Satisfy<PkgRequest>,
    {
        match self {
            Self::Simple(s) => s.clone(),
            Self::Conditional { script, when } => {
                if when.check_is_active(build_env).is_ok() {
                    script.to_string(build_env)
                } else {
                    String::new()
                }
            }
        }
    }
}
