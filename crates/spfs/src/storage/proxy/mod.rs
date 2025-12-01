// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! An spfs storage implementation that proxies one or more
//! existing repositories. The proxies secondary repositories
//! are only used to fetch missing objects and tags.

mod repository;
pub use repository::{Config, ProxyRepository};
pub(crate) use repository::{
    find_tags_in_namespace,
    iter_tag_streams_in_namespace,
    ls_tags_in_namespace,
    payload_size,
    read_tag_in_namespace,
};

use crate::storage::Repository;

/// An abstraction for reusing logic across "proxy-like" repositories.
pub(crate) trait ProxyRepositoryExt {
    fn include_secondary_tags(&self) -> bool;
    fn primary(&self) -> impl Repository;
    fn secondary(&self) -> &[crate::storage::RepositoryHandle];
}

impl<T> ProxyRepositoryExt for &T
where
    T: ProxyRepositoryExt + ?Sized,
{
    #[inline]
    fn include_secondary_tags(&self) -> bool {
        (**self).include_secondary_tags()
    }

    #[inline]
    fn primary(&self) -> impl Repository {
        (**self).primary()
    }

    #[inline]
    fn secondary(&self) -> &[crate::storage::RepositoryHandle] {
        (**self).secondary()
    }
}
