// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod platform;
mod spec;
mod test_spec;
mod variant;
mod variant_spec;

pub use platform::{BuiltPlatform, Platform};
pub use spec::Spec;
pub use test_spec::TestSpec;
pub use variant::Variant;
pub use variant_spec::VariantSpec;

#[cfg(test)]
#[path = "./validators_test.rs"]
mod validators_test;
