// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use variantly::Variantly;

use crate::ident::{
    PinnedRequest,
    PinnedValue,
    PkgRequest,
    PkgRequestOptions,
    PkgRequestWithOptions,
    VarRequest,
};
use crate::name::OptName;
use crate::spec_ops::Named;

/// Similar to [`super::PinnedRequest`] but using [`PkgRequestWithOptions`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Variantly)]
pub enum RequestWithOptions {
    Pkg(PkgRequestWithOptions),
    Var(VarRequest<PinnedValue>),
}

impl Named<OptName> for RequestWithOptions {
    fn name(&self) -> &OptName {
        match self {
            RequestWithOptions::Var(r) => &r.var,
            RequestWithOptions::Pkg(r) => r.pkg.name.as_opt_name(),
        }
    }
}

impl std::fmt::Display for RequestWithOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pkg(p) => p.pkg_request.fmt(f),
            Self::Var(v) => v.fmt(f),
        }
    }
}

impl From<PinnedRequest> for RequestWithOptions {
    fn from(r: PinnedRequest) -> Self {
        match r {
            PinnedRequest::Pkg(pr) => RequestWithOptions::Pkg(PkgRequestWithOptions {
                pkg_request: pr,
                options: Default::default(),
            }),
            PinnedRequest::Var(vr) => RequestWithOptions::Var(vr),
        }
    }
}

impl From<PkgRequest> for RequestWithOptions {
    fn from(r: PkgRequest) -> Self {
        RequestWithOptions::Pkg(PkgRequestWithOptions {
            pkg_request: r,
            options: PkgRequestOptions::default(),
        })
    }
}

impl From<PkgRequestWithOptions> for RequestWithOptions {
    fn from(r: PkgRequestWithOptions) -> Self {
        RequestWithOptions::Pkg(r)
    }
}

impl From<VarRequest<PinnedValue>> for RequestWithOptions {
    fn from(r: VarRequest<PinnedValue>) -> Self {
        RequestWithOptions::Var(r)
    }
}
