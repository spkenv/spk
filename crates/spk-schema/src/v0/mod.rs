// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod spec;
mod test_spec;
mod variant;

pub use spec::Spec;
pub use test_spec::TestSpec;
pub use variant::Variant;

#[cfg(test)]
#[path = "./validators_test.rs"]
mod validators_test;
