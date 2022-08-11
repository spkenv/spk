// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Storage implementation which is a client of the built-in spfs server

mod database;
mod payload;
mod repository;
mod tag;

pub use repository::{Config, RpcRepository};
