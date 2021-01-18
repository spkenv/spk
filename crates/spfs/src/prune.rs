use chrono::prelude::*;

use crate::{storage, tracking, Result};
use std::collections::HashSet;

#[cfg(test)]
#[path = "./prune_test.rs"]
mod prune_test;

/// Specifies a range of conditions for pruning tags out of a repository.
#[derive(Debug, Default)]
pub struct PruneParameters {
    pub prune_if_older_than: Option<DateTime<Utc>>,
    pub keep_if_newer_than: Option<DateTime<Utc>>,
    pub prune_if_version_more_than: Option<u64>,
    pub keep_if_version_less_than: Option<u64>,
}

impl PruneParameters {
    pub fn should_prune(&self, spec: &tracking::TagSpec, tag: &tracking::Tag) -> bool {
        if let Some(keep_if_version_less_than) = self.keep_if_version_less_than {
            if spec.version() < keep_if_version_less_than {
                return false;
            }
        }
        if let Some(keep_if_newer_than) = self.keep_if_newer_than {
            if tag.time > keep_if_newer_than {
                return false;
            }
        }

        if let Some(prune_if_version_more_than) = self.prune_if_version_more_than {
            if spec.version() > prune_if_version_more_than {
                return true;
            }
        }
        if let Some(prune_if_older_than) = self.prune_if_older_than {
            if tag.time < prune_if_older_than {
                return true;
            }
        }

        return false;
    }
}

pub fn get_prunable_tags(
    tags: &mut impl storage::TagStorage,
    params: PruneParameters,
) -> Result<HashSet<tracking::Tag>> {
    let mut to_prune = HashSet::new();
    for res in tags.iter_tag_streams() {
        let (spec, stream) = res?;
        let mut version = 0;
        for tag in stream {
            let versioned_spec = tracking::build_tag_spec(spec.org(), spec.name(), version)?;
            if params.should_prune(&versioned_spec, &tag) {
                to_prune.insert(tag);
            }
            version += 1;
        }
    }

    Ok(to_prune)
}

pub fn prune_tags(
    tags: &mut impl storage::TagStorage,
    params: PruneParameters,
) -> Result<HashSet<tracking::Tag>> {
    let to_prune = get_prunable_tags(tags, params)?;
    for tag in to_prune.iter() {
        tags.remove_tag(tag)?;
    }
    Ok(to_prune)
}
