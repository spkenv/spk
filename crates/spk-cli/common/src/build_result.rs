// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::foundation::format::FormatIdent;
use spk_schema::{BuildIdent, OptionMap};

/// Details on a single build artifact.
#[derive(Debug)]
pub enum BuildArtifact {
    /// A source build
    Source(BuildIdent),
    /// A binary build and its variant index and options
    Binary(BuildIdent, usize, OptionMap),
}

impl std::fmt::Display for BuildArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildArtifact::Source(ident) => write!(f, "{}", ident.format_ident()),
            BuildArtifact::Binary(ident, variant_index, options) => write!(
                f,
                "{} variant {variant_index}, {options}",
                ident.format_ident()
            ),
        }
    }
}

/// The result(s) of a build operation.
#[derive(Debug, Default)]
pub struct BuildResult {
    /// Each of the builds that were created.
    ///
    /// The first element of the tuple describes what the input was, such as
    /// the filename of a spec file.
    pub artifacts: Vec<(String, BuildArtifact)>,
}

impl BuildResult {
    /// Return if the result is empty.
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    /// Iterate over the artifacts.
    pub fn iter(&self) -> impl Iterator<Item = &(String, BuildArtifact)> {
        self.artifacts.iter()
    }

    /// Append a new artifact to the result.
    pub fn push(&mut self, input: String, output: BuildArtifact) {
        self.artifacts.push((input, output));
    }
}
