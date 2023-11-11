// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use miette::Diagnostic;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Diagnostic, Debug, Error)]
#[diagnostic(
    url(
        "https://getspk.io/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkNameError(#[from] spk_schema::foundation::name::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkStorageError(#[from] spk_storage::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkValidatorsError(#[from] spk_schema::validators::Error),
    #[error(transparent)]
    #[diagnostic(forward(0))]
    SpkVersionRangeError(#[from] spk_schema::foundation::version_range::Error),
    #[error("Error: {0}")]
    String(String),
}
