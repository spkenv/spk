use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "spec_test.rs"]
mod spec_test;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd, Deserialize, Serialize)]
pub struct Workspace {
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "glob_from_str")]
    pub recipes: Vec<glob::Pattern>,
}

mod glob_from_str {
    use serde::{Deserializer, Serialize, Serializer};

    pub fn serialize<S>(patterns: &Vec<glob::Pattern>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let patterns: Vec<_> = patterns.iter().map(|p| p.as_str()).collect();
        patterns.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<glob::Pattern>, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// Visits a serialized string, decoding it as a digest
        struct PatternVisitor;

        impl<'de> serde::de::Visitor<'de> for PatternVisitor {
            type Value = Vec<glob::Pattern>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a glob pattern")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut patterns = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(pattern) = seq.next_element()? {
                    let pattern = glob::Pattern::new(pattern).map_err(serde::de::Error::custom)?;
                    patterns.push(pattern);
                }
                Ok(patterns)
            }
        }
        deserializer.deserialize_seq(PatternVisitor)
    }
}
