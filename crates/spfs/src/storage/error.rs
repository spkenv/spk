// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use super::TagNamespaceBuf;

#[derive(Debug, miette::Diagnostic, thiserror::Error)]
#[diagnostic()]
pub enum OpenRepositoryError {
    #[error("Repository not initialized")]
    #[diagnostic(
        code("spfs::storage::fs::not_initialized"),
        help("run `spfs init repo {}` to establish a repository", path.display())
    )]
    PathNotInitialized {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("Could not validate repository version")]
    FsMigration(#[from] super::fs::migrations::MigrationError),

    #[error("Repository requires a newer version of spfs [version: {repo_version:?}]")]
    VersionIsTooNew { repo_version: semver::Version },
    #[error("Repository is for an older version of spfs [version: {repo_version:?}]")]
    #[diagnostic(help(
        "If the repo is not shared, consider using `spfs migrate`, or contact your system administrator"
    ))]
    VersionIsTooOld { repo_version: semver::Version },

    #[error("Invalid url query string")]
    #[diagnostic(code("spfs::storage::invalid_query"))]
    InvalidQuery {
        #[source_code]
        address: String,
        #[label = "this portion of the address"]
        query_span: miette::SourceSpan,
        #[source]
        source: serde_qs::Error,
    },
    #[error("This repository type requires query parameters")]
    #[diagnostic(code("spfs::storage::missing_query"))]
    MissingQuery {
        #[source_code]
        address: String,
        #[label = "should be followed by '?<query>'"]
        query_span: miette::SourceSpan,
    },

    #[error("Failed to create/validate target directory: {0:?}")]
    CouldNotCreateTarParent(std::path::PathBuf),
    #[error("Failed to open tar archive")]
    FailedToOpenArchive {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to close/write tar archive")]
    FailedToCloseArchive {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to unpack the tar archive for use")]
    FailedToUnpackArchive {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to fetch the spfs config")]
    #[diagnostic(help("Resolve any issues that appear when running `spfs config`"))]
    FailedToLoadConfig {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Failed to open sub-repository")]
    #[diagnostic(help(
        "The desired repository is a combination of others, and at least one failed"
    ))]
    FailedToOpenPartial {
        source: Box<dyn miette::Diagnostic + Send + Sync>,
    },

    #[error("Invalid repository address")]
    InvalidTransportAddress {
        #[source_code]
        address: String,
        source: tonic::transport::Error,
    },
    #[error("Failed to establish a connection")]
    FailedToConnect {
        #[from]
        source: tonic::transport::Error,
    },
    #[error("Pinned repository is read only")]
    RepositoryIsPinned,

    #[error("Failed to set tag namespace '{tag_namespace}'")]
    FailedToSetTagNamespace {
        tag_namespace: TagNamespaceBuf,
        source: Box<dyn miette::Diagnostic + Send + Sync>,
    },
}

impl OpenRepositoryError {
    pub fn invalid_query(address: &url::Url, source: serde_qs::Error) -> Self {
        let address = address.to_string();
        let query_location = address.find('?').unwrap_or_default() + 1; // 1-based
        let source_start = miette::SourceOffset::from_location(&address, 1, query_location);
        let source_length =
            miette::SourceOffset::from_location(&address, 1, address.len() - query_location + 2);
        Self::InvalidQuery {
            address,
            query_span: miette::SourceSpan::new(
                source_start,
                source_length.offset() - source_start.offset(),
            ),
            source,
        }
    }

    pub fn missing_query(address: &url::Url) -> Self {
        let address = address.to_string();
        let source_start = miette::SourceOffset::from_location(&address, 1, 1);
        let source_length = miette::SourceOffset::from_location(&address, 1, address.len());
        Self::MissingQuery {
            address,
            query_span: miette::SourceSpan::new(
                source_start,
                source_length.offset() - source_start.offset(),
            ),
        }
    }
}
