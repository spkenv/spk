// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use variantly::Variantly;

use crate::ident::{
    PinnableValue,
    PkgRequest,
    PkgRequestOptions,
    PkgRequestWithOptions,
    Request,
    VarRequest,
};
use crate::name::OptName;
use crate::spec_ops::Named;

/// Similar to [`super::Request`] but using [`PkgRequestWithOptions`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Variantly)]
pub enum RequestWithOptions {
    Pkg(PkgRequestWithOptions),
    Var(VarRequest<PinnableValue>),
}

impl Named<OptName> for RequestWithOptions {
    fn name(&self) -> &OptName {
        match self {
            RequestWithOptions::Var(r) => &r.var,
            RequestWithOptions::Pkg(r) => r.pkg_request.pkg.name.as_opt_name(),
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

impl From<Request> for RequestWithOptions {
    fn from(r: Request) -> Self {
        match r {
            Request::Pkg(pr) => RequestWithOptions::Pkg(PkgRequestWithOptions {
                pkg_request: pr,
                options: Default::default(),
            }),
            Request::Var(vr) => RequestWithOptions::Var(vr),
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

impl From<VarRequest<PinnableValue>> for RequestWithOptions {
    fn from(r: VarRequest<PinnableValue>) -> Self {
        RequestWithOptions::Var(r)
    }
}
