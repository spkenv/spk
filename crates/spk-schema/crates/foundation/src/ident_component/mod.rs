// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod component_spec;
mod error;
pub mod parsing;

pub use component_spec::{Component, ComponentSet};
pub use error::{Error, Result};
