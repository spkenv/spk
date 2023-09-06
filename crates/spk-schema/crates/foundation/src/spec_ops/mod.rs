// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod component_ops;
mod env_name;
mod error;
mod file_matcher;
mod has_build;
mod has_location;
mod named;
mod versioned;

pub use component_ops::ComponentOps;
pub use env_name::EnvName;
pub use error::{Error, Result};
pub use file_matcher::FileMatcher;
pub use has_build::HasBuild;
pub use has_location::HasLocation;
pub use named::Named;
pub use versioned::{HasVersion, Versioned};

pub mod prelude {
    pub use super::{ComponentOps, EnvName, HasBuild, HasLocation, HasVersion, Named, Versioned};
}
