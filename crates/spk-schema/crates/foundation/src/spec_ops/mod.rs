// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod component_ops;
mod env_name;
mod error;
mod file_matcher;
mod has_build;
mod has_build_ident;
mod has_location;
mod named;
mod versioned;

pub use component_ops::{ComponentFileMatchMode, ComponentOps};
pub use env_name::EnvName;
pub use error::{Error, Result};
pub use file_matcher::FileMatcher;
pub use has_build::HasBuild;
pub use has_build_ident::HasBuildIdent;
pub use has_location::HasLocation;
pub use named::Named;
pub use versioned::{HasVersion, Versioned, WithVersion};

pub mod prelude {
    pub use super::{ComponentOps, EnvName, HasBuild, HasLocation, HasVersion, Named, Versioned};
}
