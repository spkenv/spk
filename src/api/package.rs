// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

pub trait Package {
    /// The name of this package
    fn name(&self) -> &super::Name {
        &self.ident().name
    }

    /// The full identifier for this package
    ///
    /// This includes the version and optional build
    fn ident(&self) -> &super::Ident;
}
