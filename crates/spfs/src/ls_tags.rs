// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use futures::Stream;

use super::config::get_config;
use super::storage::prelude::*;
use crate::storage::EntryType;
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
pub async fn ls_tags<P: AsRef<relative_path::RelativePath>>(
    path: Option<P>,
) -> Pin<Box<dyn Stream<Item = Result<EntryType>>>> {
    let repo = match get_config() {
        Ok(c) => match c.get_repository().await {
            Ok(repo) => repo,
            Err(err) => return Box::pin(futures::stream::once(async { Err(err) })),
        },
        Err(err) => return Box::pin(futures::stream::once(async { Err(err) })),
    };
    match path {
        Some(path) => repo.ls_tags(path.as_ref()),
        None => repo.ls_tags(relative_path::RelativePath::new("/")),
    }
}
