// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use chrono::prelude::*;

use crate::tracking;

#[cfg(test)]
#[path = "./prune_test.rs"]
mod prune_test;

/// Specifies a range of conditions for pruning tags out of a repository.
#[derive(Debug, Default)]
pub(crate) struct PruneParameters {
    pub prune_if_older_than: Option<DateTime<Utc>>,
    pub keep_if_newer_than: Option<DateTime<Utc>>,
    pub prune_if_version_more_than: Option<u64>,
    pub keep_if_version_less_than: Option<u64>,
}

impl PruneParameters {
    pub fn is_empty(&self) -> bool {
        let Self {
            // keep params are irrelevant unless prune options are specified
            keep_if_newer_than: _,
            keep_if_version_less_than: _,
            prune_if_older_than,
            prune_if_version_more_than,
        } = self;

        prune_if_older_than.is_none() && prune_if_version_more_than.is_none()
    }

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

        false
    }
}
