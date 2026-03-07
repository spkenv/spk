// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

pub use spk_build as build;
pub use spk_exec as exec;
pub use spk_schema as schema;
pub use spk_solve as solve;
pub use spk_storage as storage;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
