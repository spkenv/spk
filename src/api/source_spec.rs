// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::io::Write;
use std::path::{Path, PathBuf};

use relative_path::RelativePathBuf;
use serde_derive::{Deserialize, Serialize};

use crate::{Error, Result};

#[cfg(test)]
#[path = "./source_spec_test.rs"]
mod source_spec_test;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourceSpec {
    Local(LocalSource),
    Git(GitSource),
    Tar(TarSource),
    Script(ScriptSource),
}

impl SourceSpec {
    /// Optional directory under the main source folder to place these sources.
    pub fn subdir(&self) -> Option<RelativePathBuf> {
        match self {
            SourceSpec::Local(source) => source.subdir.as_ref().map(|s| RelativePathBuf::from(s)),
            SourceSpec::Git(source) => source.subdir.as_ref().map(|s| RelativePathBuf::from(s)),
            SourceSpec::Tar(source) => source.subdir.as_ref().map(|s| RelativePathBuf::from(s)),
            SourceSpec::Script(source) => source.subdir.as_ref().map(|s| RelativePathBuf::from(s)),
        }
    }

    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        match self {
            SourceSpec::Local(source) => source.collect(dirname),
            SourceSpec::Git(source) => source.collect(dirname),
            SourceSpec::Tar(source) => source.collect(dirname),
            SourceSpec::Script(source) => source.collect(dirname),
        }
    }
}

/// Package source files in a local directory or file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSource {
    pub path: PathBuf,
    #[serde(
        default = "LocalSource::default_exclude",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub exclude: Vec<String>,
    #[serde(
        default = "LocalSource::default_filter",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub filter: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

impl Default for LocalSource {
    fn default() -> Self {
        Self {
            path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            exclude: Self::default_exclude(),
            filter: Self::default_filter(),
            subdir: None,
        }
    }
}

impl LocalSource {
    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        let mut rsync = std::process::Command::new("rsync");
        rsync.arg("--archive");
        let path = if self.path.is_dir() {
            // if the source path is a directory then we require
            // a trailing '/' so that rsync doesn't create new subdirectories
            // in the destination folder
            rsync.arg("--recursive");
            self.path.join("")
        } else {
            self.path.clone()
        };
        // require a trailing '/' on destination also so that rsync doesn't
        // add aditional levels to the resulting structure
        let dirname = dirname.join("");
        if let Ok(_) = std::env::var("SPK_DEBUG") {
            rsync.arg("--verbose");
        }
        for filter_rule in self.filter.iter() {
            rsync.arg("--filter");
            rsync.arg(filter_rule);
        }
        for exclusion in self.exclude.iter() {
            rsync.arg("--exclude");
            rsync.arg(exclusion);
        }
        rsync.args(&[&path, &dirname]);
        tracing::debug!(cmd = ?rsync, "running");
        rsync.current_dir(&dirname);
        match rsync.status()?.code() {
            Some(0) => Ok(()),
            code => Err(Error::String(format!(
                "rsync command failed with exit code {:?}",
                code
            ))),
        }
    }

    fn default_exclude() -> Vec<String> {
        vec![".git/".to_string(), ".svn/".to_string()]
    }

    fn default_filter() -> Vec<String> {
        vec![":- .gitignore".to_string()]
    }
}

/// Package source files from a remote git repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSource {
    pub git: String,
    #[serde(default, rename = "ref", skip_serializing_if = "String::is_empty")]
    pub reference: String,
    #[serde(
        default = "default_git_clone_depth",
        skip_serializing_if = "is_default_git_clone_depth"
    )]
    pub depth: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

impl GitSource {
    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        let mut git_cmd = std::process::Command::new("git");
        git_cmd.arg("clone");
        git_cmd.arg("--depth");
        git_cmd.arg(self.depth.to_string());
        if !self.reference.is_empty() {
            git_cmd.arg("-b");
            git_cmd.arg(&self.reference);
        }
        git_cmd.arg(&self.git);
        git_cmd.arg(&dirname);

        let mut submodule_cmd = std::process::Command::new("git");
        submodule_cmd.args(&["submodule", "update", "--init", "--recursive"]);
        if git_supports_submodule_depth() {
            submodule_cmd.arg("--depth");
            submodule_cmd.arg(self.depth.to_string());
        }

        for mut cmd in vec![git_cmd, submodule_cmd].into_iter() {
            tracing::debug!(?cmd, "running");
            cmd.current_dir(&dirname);
            match cmd.status()?.code() {
                Some(0) => (),
                code => {
                    return Err(Error::String(format!(
                        "git command failed with exit code {:?}",
                        code
                    )))
                }
            }
        }
        Ok(())
    }
}

/// Package source files from a local or remote tar archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TarSource {
    pub tar: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

impl TarSource {
    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        let tmpdir = tempdir::TempDir::new("spk-untar")?;
        let tarfile = relative_path::RelativePathBuf::from(&self.tar);
        let filename = tarfile.file_name().unwrap_or_default();
        let mut tarfile = tmpdir.path().join(filename);
        let re = regex::Regex::new("^https?://").unwrap();
        if re.is_match(&self.tar) {
            let mut wget = std::process::Command::new("wget");
            wget.arg(&self.tar);
            tracing::debug!(cmd=?wget, "running");
            match wget.status()?.code() {
                Some(0) => (),
                code => {
                    return Err(Error::String(format!(
                        "wget command failed with exit code {:?}",
                        code
                    )))
                }
            }
        } else {
            tarfile = std::path::PathBuf::from(&self.tar).canonicalize()?;
        }

        let mut cmd = std::process::Command::new("tar");
        cmd.arg("-xf");
        cmd.arg(&tarfile);
        cmd.current_dir(&dirname);
        tracing::debug!(?cmd, "running");
        match cmd.status()?.code() {
            Some(0) => Ok(()),
            code => {
                return Err(Error::String(format!(
                    "tar command failed with exit code {:?}",
                    code
                )))
            }
        }
    }
}

/// Package source files collected via arbitrary shell script.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptSource {
    pub script: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

impl ScriptSource {
    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        let mut bash = std::process::Command::new("bash");
        bash.arg("-ex"); // print each command, exit on failure
        bash.arg("-"); // read from stdin
        bash.stdin(std::process::Stdio::piped());
        bash.current_dir(dirname);

        tracing::debug!("running sources script");
        let mut child = bash.spawn()?;
        let stdin = match child.stdin.as_mut() {
            Some(s) => s,
            None => {
                return Err(Error::String(
                    "failed to get stdin handle for bash".to_string(),
                ))
            }
        };
        if let Err(err) = stdin.write_all(self.script.join("\n").as_bytes()) {
            return Err(Error::wrap_io("failed to write source script to bash", err));
        }

        match child.wait()?.code() {
            Some(0) => Ok(()),
            code => Err(Error::String(format!(
                "source script failed with exit code {:?}",
                code
            ))),
        }
    }
}

pub fn git_supports_submodule_depth() -> bool {
    let v = git_version();
    match v {
        None => return false,
        Some(v) => v.as_str() >= "2.0",
    }
}

pub fn git_version() -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("--version");

    let out = match cmd.output() {
        Err(_) => return None,
        Ok(out) => out,
    };

    // eg: git version 1.83.6
    let out = String::from_utf8_lossy(out.stdout.as_slice());
    out.trim().split(" ").last().map(|s| s.to_string())
}

fn default_git_clone_depth() -> u32 {
    1
}

fn is_default_git_clone_depth(depth: &u32) -> bool {
    depth == &default_git_clone_depth()
}
