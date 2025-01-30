// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use resolvo::utils::VersionSet;
use spk_schema::ident::{LocatedBuildIdent, PkgRequest};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub(crate) struct PkgRequestVS(pub(crate) PkgRequest);

impl VersionSet for PkgRequestVS {
    type V = LocatedBuildIdent;
}
