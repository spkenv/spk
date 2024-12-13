// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;

/// A collection of recipes and build targets.
///
/// Workspaces are used to define and build many recipes
/// together, helping to produce complete environments
/// with shared compatibility requirements. Workspaces
/// can be used to determine the number and order of
/// packages to be built in order to efficiently satisfy
/// and entire set of requirements for an environment.
pub struct Workspace {
    /// Spec templates available in this workspace.
    ///
    /// A workspace may contain multiple recipes for a single
    /// package, and templates may also not have a package name
    /// defined inside.
    pub(crate) templates:
        HashMap<Option<spk_schema::name::PkgNameBuf>, Vec<spk_schema::SpecTemplate>>,
}

impl Workspace {
    pub fn builder() -> crate::builder::WorkspaceBuilder {
        crate::builder::WorkspaceBuilder::default()
    }
}
