// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! An SPK workspace groups the recipe spec files in a directory tree so
//! that spk commands can find them by package name, version, or path.
//!
//! A [`WorkspaceFile`] (`workspace.spk.yaml`) declares which spec files
//! belong to the workspace via glob patterns, and is loaded into a
//! [`Workspace`] for resolution. When no workspace file is present,
//! callers typically fall back to a virtual workspace scoped to the
//! current directory.

#![deny(missing_docs)]

pub mod builder;
pub mod error;
mod file;
mod workspace;

pub use file::WorkspaceFile;
pub use workspace::{
    FindOrLoadPackageTemplateError,
    FindPackageTemplateError,
    FindPackageTemplateResult,
    Workspace,
};
