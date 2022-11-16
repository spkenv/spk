// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build_spec;
mod embedded_package;
mod install_spec;
mod option;
mod spec;
mod test_spec;

pub(crate) use build_spec::UncheckedBuildSpec;
pub use build_spec::{BuildSpec, Script};
pub use embedded_package::EmbeddedPackage;
pub use install_spec::InstallSpec;
pub use option::{Inheritance, Opt};
pub use spec::Spec;
pub use test_spec::TestSpec;

pub type Recipe = Spec<spk_schema_ident::VersionIdent>;
pub type Package = Spec<spk_schema_ident::BuildIdent>;
