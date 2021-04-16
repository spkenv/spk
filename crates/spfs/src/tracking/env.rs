use super::tag::TagSpec;
use crate::encoding;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./env_test.rs"]
mod env_test;

static ENV_SPEC_SEPARATOR: &str = "+";

/// One object specifier in an env spec
#[derive(Debug)]
pub enum EnvSpecItem {
    TagSpec(TagSpec),
    Digest(encoding::Digest),
}

impl std::fmt::Display for EnvSpecItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TagSpec(x) => x.fmt(f),
            Self::Digest(x) => x.fmt(f),
        }
    }
}

/// Specifies a complete runtime environment that
/// can be made up of multiple layers.
#[derive(Debug)]
pub struct EnvSpec {
    /// The ordered set of references that make up this environment.
    pub items: Vec<EnvSpecItem>,
}

impl EnvSpec {
    pub fn new(spec: &str) -> Result<Self> {
        Ok(Self {
            items: parse_env_spec(spec)?,
        })
    }
}

impl From<encoding::Digest> for EnvSpec {
    fn from(digest: encoding::Digest) -> Self {
        EnvSpec {
            items: vec![EnvSpecItem::Digest(digest)],
        }
    }
}

impl std::string::ToString for EnvSpec {
    fn to_string(&self) -> String {
        let items: Vec<_> = self.items.iter().map(|i| i.to_string()).collect();
        items.join(ENV_SPEC_SEPARATOR)
    }
}

/// Return the items identified in an environment spec string.
///
/// ```rust
/// use spfs::tracking::parse_env_spec;
/// let items = parse_env_spec("sometag~1+my-other-tag").unwrap();
/// let items: Vec<_> = items.into_iter().map(|i| i.to_string()).collect();
/// assert_eq!(items, vec!["sometag~1", "my-other-tag"]);
/// let items = parse_env_spec("3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====+my-tag").unwrap();
/// let items: Vec<_> = items.into_iter().map(|i| i.to_string()).collect();
/// assert_eq!(items, vec!["3YDG35SUMJS67N2QPQ4NQCYJ6QGKMEB5H4MHC76VRGMRWBRBLFHA====", "my-tag"]);
/// ```
pub fn parse_env_spec<S: AsRef<str>>(spec: S) -> Result<Vec<EnvSpecItem>> {
    let mut items = Vec::new();
    for layer in spec.as_ref().split(ENV_SPEC_SEPARATOR) {
        if let Ok(digest) = encoding::parse_digest(layer) {
            items.push(EnvSpecItem::Digest(digest));
            continue;
        }
        let tag_spec = TagSpec::parse(layer)?;
        items.push(EnvSpecItem::TagSpec(tag_spec));
    }

    if items.len() == 0 {
        return Err(Error::new("must specify at least one digest or tag"));
    }

    Ok(items)
}
