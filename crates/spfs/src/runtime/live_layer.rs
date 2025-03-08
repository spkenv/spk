// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Definition and persistent storage of runtimes.

use std::fmt::Display;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::spec_api_version::SpecApiVersion;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./live_layer_test.rs"]
mod live_layer_test;

/// Data needed to bind mount a path onto an /spfs backend that uses
/// overlayfs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BindMount {
    /// Path to the source dir, or file, to bind mount into /spfs at
    /// the destination.
    #[serde(alias = "bind")]
    pub src: PathBuf,
    /// Where to attach the dir, or file, inside /spfs
    pub dest: String,
}

impl BindMount {
    /// Checks the bind mount is valid for use in /spfs with the given parent directory
    pub(crate) fn validate(&self, parent: PathBuf) -> Result<()> {
        if !self.src.starts_with(parent.clone()) {
            return Err(Error::String(format!(
                "Bind mount is not valid: {} is not under the live layer's directory: {}",
                self.src.display(),
                parent.display()
            )));
        }

        if !self.src.exists() {
            return Err(Error::String(format!(
                "Bind mount is not valid: {} does not exist",
                self.src.display()
            )));
        }

        Ok(())
    }
}

impl Display for BindMount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.src.display(), self.dest)
    }
}

/// The kinds of contents that can be part of a live layer
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum LiveLayerContents {
    /// A directory or file that will be bind mounted over /spfs
    BindMount(BindMount),
}

/// Data needed to add a live layer onto an /spfs overlayfs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct LiveLayer {
    /// The api format version of the live layer data
    pub api: SpecApiVersion,
    /// The contents that the live layer will put into /spfs
    pub contents: Vec<LiveLayerContents>,
}

impl LiveLayer {
    /// Returns a list of the BindMounts in this LiveLayer
    pub fn bind_mounts(&self) -> Vec<BindMount> {
        self.contents
            .iter()
            .map(|c| match c {
                LiveLayerContents::BindMount(bm) => bm.clone(),
            })
            .collect::<Vec<_>>()
    }

    /// Updates the live layer's contents entries using given parent
    /// directory. This will error if the resulting paths do not exist.
    ///
    /// This should be called before validate()
    fn set_parent(&mut self, parent: PathBuf) -> Result<()> {
        let mut new_contents = Vec::new();

        for entry in self.contents.iter() {
            let new_entry = match entry {
                LiveLayerContents::BindMount(bm) => {
                    let full_path = match parent.join(bm.src.clone()).canonicalize() {
                        Ok(abs_path) => abs_path.clone(),
                        Err(err) => {
                            return Err(Error::InvalidPath(parent.join(bm.src.clone()), err));
                        }
                    };

                    LiveLayerContents::BindMount(BindMount {
                        src: full_path,
                        dest: bm.dest.clone(),
                    })
                }
            };

            new_contents.push(new_entry);
        }
        self.contents = new_contents;

        Ok(())
    }

    /// Validates the live layer's contents are under the given parent
    /// directory and accessible by the current user.
    ///
    /// This should be called after set_parent()
    fn validate(&self, parent: PathBuf) -> Result<()> {
        for entry in self.contents.iter() {
            match entry {
                LiveLayerContents::BindMount(bm) => bm.validate(parent.clone())?,
            }
        }
        Ok(())
    }

    /// Sets the live layer's parent directory, which updates its
    /// contents, and then validates its contents.
    pub fn set_parent_and_validate(&mut self, parent: PathBuf) -> Result<()> {
        let abs_parent = match parent.canonicalize() {
            Ok(abs_path) => abs_path.clone(),
            Err(err) => return Err(Error::InvalidPath(parent.clone(), err)),
        };

        self.set_parent(parent.clone())?;
        self.validate(abs_parent)
    }
}

impl Display for LiveLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.api, self.contents)
    }
}
