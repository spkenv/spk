// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod embedded_package;
mod spec;

pub use embedded_package::EmbeddedPackage;
pub use spec::Spec;

pub type Recipe = Spec<spk_schema_ident::VersionIdent>;
pub type Package = Spec<spk_schema_ident::BuildIdent>;
