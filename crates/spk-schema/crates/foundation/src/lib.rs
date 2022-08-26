// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

pub mod env;
pub mod fixtures;
pub mod format;
mod from_yaml;
pub mod ident_build;
pub mod ident_component;
pub mod ident_ops;
pub mod name;
pub mod option_map;
pub mod spec_ops;
pub mod version;
pub mod version_range;

pub use fixtures::*;
pub use from_yaml::FromYaml;
