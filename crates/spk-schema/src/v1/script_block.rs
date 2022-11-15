// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use serde::{Deserialize, Serialize};

use super::WhenBlock;

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize)]
pub struct ScriptBlock(Vec<ScriptBlockEntry>);

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

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ScriptBlockEntry {
    Simple(String),
    Conditional {
        #[serde(rename = "do")]
        script: ScriptBlock,
        when: WhenBlock,
    },
}
