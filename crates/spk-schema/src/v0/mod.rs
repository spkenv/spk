// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod platform;
mod requirements;
mod spec;
mod test_spec;
mod variant;
mod variant_spec;

pub use platform::Platform;
pub use requirements::Requirements;
pub use spec::Spec;
pub use spec::{LintedSpec, Spec};
pub use test_spec::TestSpec;
pub use variant::Variant;
pub use variant_spec::VariantSpec;
