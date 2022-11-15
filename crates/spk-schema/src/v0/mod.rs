// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build_spec;
mod embedded_packages_list;
mod install_spec;
mod option;
mod spec;
mod test_spec;

pub(crate) use build_spec::UncheckedBuildSpec;
pub use build_spec::{BuildSpec, Script};
pub use embedded_packages_list::EmbeddedPackagesList;
pub use install_spec::InstallSpec;
pub use option::{Inheritance, Opt};
pub use spec::Spec;
pub use test_spec::TestSpec;
