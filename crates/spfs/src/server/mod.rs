// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Remote server implementations of the spfs repository

pub mod proto {
    tonic::include_proto!("spfs");
}
mod repository;
mod tag;

pub use repository::Repository;
pub use tag::TagService;
