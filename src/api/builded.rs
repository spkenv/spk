// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::Build;

/// Some item with an associated build
pub trait Builded {
    fn build(&self) -> &Build;

    /// Replace the associated build, returning the previous value.
    fn set_build(&mut self, build: Build);
}
