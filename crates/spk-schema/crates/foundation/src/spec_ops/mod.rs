// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod component_ops;
mod error;
mod file_matcher;
mod has_build;
mod named;
mod versioned;

pub use component_ops::ComponentOps;
pub use error::{Error, Result};
pub use file_matcher::FileMatcher;
pub use has_build::HasBuild;
pub use named::Named;
pub use versioned::{HasVersion, Versioned};

pub mod prelude {
    pub use super::{ComponentOps, HasBuild, HasVersion, Named, Versioned};
}
