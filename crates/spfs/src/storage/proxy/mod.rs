// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! An spfs storage implementation that proxies one or more
//! existing repositories. The proxies secondary repositories
//! are only used to fetch missing objects and tags.

mod repository;
pub use repository::{Config, ProxyRepository};
