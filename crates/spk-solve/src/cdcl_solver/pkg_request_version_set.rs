// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use resolvo::utils::VersionSet;
use spk_schema::ident::LocatedBuildIdent;
use spk_schema::Request;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub(crate) struct RequestVS(pub(crate) Request);

impl VersionSet for RequestVS {
    type V = LocatedBuildIdent;
}
