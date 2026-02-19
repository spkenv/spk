// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! SPK CLI command group 1: bake, completion, deprecation commands.

/// The `spk bake` command for creating embedded package variants.
pub mod cmd_bake;

/// Shell completion script generation.
pub mod cmd_completion;

/// The `spk deprecate` command for marking packages as deprecated.
pub mod cmd_deprecate;

/// The `spk undeprecate` command for removing deprecation marks.
pub mod cmd_undeprecate;
