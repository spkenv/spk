// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_ident::Ident;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid package spec for {0}: {1}")]
    InvalidPackageSpec(Ident, #[source] serde_yaml::Error),
    #[error("Invalid repository metadata: {0}")]
    InvalidRepositoryMetadata(#[source] serde_yaml::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SPFS(#[from] spfs::Error),
    #[error(transparent)]
    SpkIdentBuildError(#[from] spk_foundation::ident_build::Error),
    #[error(transparent)]
    SpkIdentComponentError(#[from] spk_foundation::ident_component::Error),
    #[error(transparent)]
    SpkNameError(#[from] spk_foundation::name::Error),
    #[error(transparent)]
    SpkSpecError(#[from] spk_spec::Error),
    #[error(transparent)]
    SpkValidatorsError(#[from] spk_validators::Error),
    #[error("Error: {0}")]
    String(String),
}

impl From<String> for Error {
    fn from(err: String) -> Error {
        Error::String(err)
    }
}
