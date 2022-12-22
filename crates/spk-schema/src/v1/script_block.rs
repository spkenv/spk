// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};
use spk_schema_ident::{PkgRequest, Satisfy};

use super::WhenBlock;
use crate::BuildEnv;

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ScriptBlock(Vec<ScriptBlockEntry>);

impl ScriptBlock {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Reduce this script to a string, resolving all conditionals
    pub fn to_string<E>(&self, build_env: &E) -> String
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest>,
    {
        // NOTE(rbottriell): the argument here must be a reference, since
        // there is a recursive call and without it, the compiler will try
        // to infinitely generate more instances of this generic function
        // with increasingly more references until it reaches the configured
        // limit
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

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(untagged)]
pub enum ScriptBlockEntry {
    Simple(String),
    Conditional(ConditionalScriptBlockEntry),
}

impl ScriptBlockEntry {
    /// Reduce this script to a string, resolving all conditionals
    pub fn to_string<E>(&self, build_env: &E) -> String
    where
        E: BuildEnv,
        E::Package: Satisfy<PkgRequest>,
    {
        // NOTE(rbottriell): the argument here must be a reference, since
        // there is a recursive call and without it, the compiler will try
        // to infinitely generate more instances of this generic function
        // with increasingly more references until it reaches the configured
        // limit
        match self {
            Self::Simple(s) => s.clone(),
            Self::Conditional(entry) => {
                if entry.when.check_is_active(build_env).is_enabled_for_any() {
                    entry.script.to_string(build_env)
                } else {
                    entry.else_script.to_string(build_env)
                }
            }
        }
    }
}

impl<'de> serde::de::Deserialize<'de> for ScriptBlockEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ScriptBlockEntryVisitor;

        impl<'de> serde::de::Visitor<'de> for ScriptBlockEntryVisitor {
            type Value = ScriptBlockEntry;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string or conditional script block")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ScriptBlockEntry::Simple(v.to_owned()))
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let deserializer = serde::de::value::MapAccessDeserializer::new(map);
                ConditionalScriptBlockEntry::deserialize(deserializer)
                    .map(ScriptBlockEntry::Conditional)
            }
        }

        deserializer.deserialize_any(ScriptBlockEntryVisitor)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct ConditionalScriptBlockEntry {
    when: WhenBlock,
    #[serde(rename = "do")]
    script: ScriptBlock,
    #[serde(
        default,
        rename = "else",
        skip_serializing_if = "ScriptBlock::is_empty"
    )]
    else_script: ScriptBlock,
}
