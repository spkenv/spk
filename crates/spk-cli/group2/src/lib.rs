// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! SPK CLI command group 2: listing, creation, and publishing commands.

/// The `spk ls` command for listing packages.
pub mod cmd_ls;

/// The `spk new` command for creating new package specs.
pub mod cmd_new;

/// The `spk num-variants` command for counting build variants.
pub mod cmd_num_variants;

/// The `spk publish` command for publishing packages to repositories.
pub mod cmd_publish;

/// The `spk remove` command for removing packages from repositories.
pub mod cmd_remove;

/// The `spk stats` command for repository statistics.
pub mod cmd_stats;
