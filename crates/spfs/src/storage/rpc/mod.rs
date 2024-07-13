// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Storage implementation which is a client of the built-in spfs server

mod database;
mod payload;
mod repository;
mod tag;

pub use repository::{Config, Params, RpcRepository};
