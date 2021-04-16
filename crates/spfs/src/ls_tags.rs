// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::config::load_config;
use super::storage::prelude::*;
use crate::Result;

/// List tags and tag directories based on a tag path.
///
/// For example, if the repo contains the following tags:
///     spi/stable/my_tag
///     spi/stable/other_tag
///     spi/latest/my_tag
/// Then ls_tags("spi") would return:
///     stable
///     latest
pub fn ls_tags<P: AsRef<relative_path::RelativePath>>(
    path: Option<P>,
) -> Result<Box<dyn Iterator<Item = String>>> {
    let config = load_config()?;
    let repo = config.get_repository()?;
    match path {
        Some(path) => Ok(repo.ls_tags(path.as_ref())?),
        None => Ok(repo.ls_tags(relative_path::RelativePath::new("/"))?),
    }
}
