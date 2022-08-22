// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
//! Main entry points and utilities for command line interface and interaction.

use anyhow::Result;

/// Trait all cli commands must implement to be runnable.
#[async_trait::async_trait]
pub trait Run {
    async fn run(&mut self) -> Result<i32>;
}

/// Trait all cli commands must implement to provide a list of the
/// "request" equivalent values from their command lines. This may be
/// expanded in future to include other groupings of arguments.
pub trait CommandArgs {
    /// Get a string list of the important positional arguments for
    /// the command that may help distinguish it from another instance
    /// of the same command, or different spk command. If there are no
    /// positional arguments, this will return an empty list.
    ///
    /// Most commands will return a list of their requests or package
    /// names, but search terms and filepaths may be returned by some
    /// commands.
    fn get_positional_args(&self) -> Vec<String>;
}
