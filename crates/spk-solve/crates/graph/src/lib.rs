// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::fn_params_excessive_bools)]

mod error;
mod graph;

pub use error::{
    Error,
    GetCurrentResolveError,
    GetCurrentResolveResult,
    GetMergedRequestError,
    GetMergedRequestResult,
    Result,
};
pub use graph::{
    CachedHash,
    Change,
    Decision,
    Graph,
    GraphError,
    Node,
    Note,
    RequestPackage,
    RequestVar,
    SetOptions,
    SkipPackageNote,
    State,
    StepBack,
    DEAD_STATE,
    DUPLICATE_REQUESTS_COUNT,
    REQUESTS_FOR_SAME_PACKAGE_COUNT,
};
