// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use super::{Spec, SpecRecipe};
use crate::Result;

/// Some item that has an associated package name
#[enum_dispatch::enum_dispatch]
pub trait Named {
    /// The name of the associated package
    fn name(&self) -> &super::PkgName;
}

impl<T: Named> Named for Arc<T> {
    fn name(&self) -> &super::PkgName {
        (**self).name()
    }
}

impl<T: Named> Named for &T {
    fn name(&self) -> &super::PkgName {
        (**self).name()
    }
}

/// Can be rendered into a recipe.
#[enum_dispatch::enum_dispatch]
pub trait Template: Named + Sized {
    type Output: super::Recipe;

    /// Load this template from a file on disk
    fn from_file(path: &std::path::Path) -> Result<Self>;

    /// Save this template to a file on disk
    ///
    /// If this file already exists, it will be overwritten
    fn to_file(&self, path: &std::path::Path) -> Result<()>;

    /// Render this template with the provided values.
    fn render(&self, options: &super::OptionMap) -> Result<Self::Output>;
}
