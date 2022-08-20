// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use crate::foundation::option_map::OptionMap;
use crate::foundation::spec_ops::Named;
use crate::Result;

/// Can be rendered into a recipe.
#[enum_dispatch::enum_dispatch]
pub trait Template: Named + Sized {
    type Output: super::Recipe;

    /// Identify the location of this template on disk
    fn file_path(&self) -> &Path;

    /// Render this template with the provided values.
    fn render(&self, options: &OptionMap) -> Result<Self::Output>;
}

pub trait TemplateExt: Template {
    /// Load this template from a file on disk
    fn from_file(path: &Path) -> Result<Self>;
}
