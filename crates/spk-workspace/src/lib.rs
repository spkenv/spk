// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! SPK workspaces are used to build a network of packages together.
//!
//! The [`WorkspaceFile`] is used to load [`Workspace`] configurations from
//! yaml files on disk.

#![deny(missing_docs)]

pub mod builder;
pub mod error;
mod file;
mod workspace;

pub use file::WorkspaceFile;
pub use workspace::{
    FindOrLoadPackageTemplateError, FindPackageTemplateError, FindPackageTemplateResult, Workspace,
};
