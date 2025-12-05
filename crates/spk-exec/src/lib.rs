// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod error;
mod exec;

pub use error::{Error, Result};
pub use exec::{
    ConflictingPackagePair,
    ResolvedLayer,
    ResolvedLayers,
    pull_resolved_runtime_layers,
    pull_resolved_runtime_layers_with_reporter,
    resolve_runtime_layers,
    resolve_runtime_layers_with_reporter,
    setup_current_runtime,
    setup_runtime,
    setup_runtime_with_reporter,
    solution_to_resolved_runtime_layers,
};
