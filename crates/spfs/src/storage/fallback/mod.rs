// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

//! An spfs storage implementation that proxies one or more
//! existing repositories. The proxies secondary repositories
//! are only used to fetch missing objects and tags.

mod repository;
pub use repository::{Config, FallbackProxy};
