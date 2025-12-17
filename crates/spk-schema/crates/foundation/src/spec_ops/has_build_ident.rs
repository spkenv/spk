// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use crate::ident::BuildIdent;

/// Some item that has an associated build id
pub trait HasBuildIdent {
    fn build_ident(&self) -> &BuildIdent;
}

impl<T: HasBuildIdent> HasBuildIdent for Arc<T> {
    fn build_ident(&self) -> &BuildIdent {
        (**self).build_ident()
    }
}

impl<T: HasBuildIdent> HasBuildIdent for Box<T> {
    fn build_ident(&self) -> &BuildIdent {
        (**self).build_ident()
    }
}

impl<T: HasBuildIdent> HasBuildIdent for &T {
    fn build_ident(&self) -> &BuildIdent {
        (**self).build_ident()
    }
}
