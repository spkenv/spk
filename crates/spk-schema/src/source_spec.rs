// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use relative_path::RelativePathBuf;
use serde::{Deserialize, Serialize};
use spk_schema_foundation::option_map::Stringified;

use crate::{Error, LintMessage, LintedItem, Lints, Result, Script, SourceSpecKey};

#[cfg(test)]
#[path = "./source_spec_test.rs"]
mod source_spec_test;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SpecKind {
    Local,
    Git,
    Tar,
    Script,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum SourceSpec {
    Local(LocalSource),
    Git(GitSource),
    Tar(TarSource),
    Script(ScriptSource),
}

impl Default for SourceSpec {
    fn default() -> Self {
        Self::Local(LocalSource {
            path: PathBuf::from("."),
            exclude: LocalSource::default_exclude(),
            filter: LocalSource::default_filter(),
            subdir: None,
        })
    }
}

impl SourceSpec {
    /// Optional directory under the main source folder to place these sources.
    pub fn subdir(&self) -> Option<RelativePathBuf> {
        match self {
            SourceSpec::Local(source) => source.subdir.as_ref().map(RelativePathBuf::from),
            SourceSpec::Git(source) => source.subdir.as_ref().map(RelativePathBuf::from),
            SourceSpec::Tar(source) => source.subdir.as_ref().map(RelativePathBuf::from),
            SourceSpec::Script(source) => source.subdir.as_ref().map(RelativePathBuf::from),
        }
    }

    /// Collect the represented sources files into the given directory.
    ///
    /// The base build environment should also be provided, in order to
    /// have variables like SPK_PACKAGE_VERSION available to collection scripts.
    pub fn collect(&self, dirname: &Path, env: &HashMap<String, String>) -> Result<()> {
        match self {
            SourceSpec::Local(source) => source.collect(dirname),
            SourceSpec::Git(source) => source.collect(dirname),
            SourceSpec::Tar(source) => source.collect(dirname),
            SourceSpec::Script(source) => source.collect(dirname, env),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
enum IdentKind {
    Path(PathBuf),
    Git(String),
    Tar(String),
    Script(Script),
}

impl IdentKind {
    pub fn get_path(&self) -> PathBuf {
        match self {
            IdentKind::Path(p) => p.clone(),
            IdentKind::Git(_) => PathBuf::new(),
            IdentKind::Tar(_) => PathBuf::new(),
            IdentKind::Script(_) => PathBuf::new(),
        }
    }

    pub fn get_git(&self) -> String {
        match self {
            IdentKind::Path(_) => String::from(""),
            IdentKind::Git(g) => g.clone(),
            IdentKind::Tar(_) => String::from(""),
            IdentKind::Script(_) => String::from(""),
        }
    }

    pub fn get_tar(&self) -> String {
        match self {
            IdentKind::Path(_) => String::from(""),
            IdentKind::Git(_) => String::from(""),
            IdentKind::Tar(t) => t.clone(),
            IdentKind::Script(_) => String::from(""),
        }
    }

    pub fn get_script(&self) -> Script {
        match self {
            IdentKind::Path(_) => Script::new(vec![""]),
            IdentKind::Git(_) => Script::new(vec![""]),
            IdentKind::Tar(_) => Script::new(vec![""]),
            IdentKind::Script(s) => s.clone(),
        }
    }
}

#[derive(Default, Debug)]
struct SourceSpecVisitor {
    identifier: Option<(SpecKind, IdentKind)>,
    exclude: Option<Vec<String>>,
    filter: Option<Vec<String>>,
    reference: Option<String>,
    depth: Option<u32>,
    subdir: Option<String>,
    lints: Vec<LintMessage>,
}

impl Lints for SourceSpecVisitor {
    fn lints(&mut self) -> Vec<LintMessage> {
        std::mem::take(&mut self.lints)
    }
}

impl From<SourceSpecVisitor> for SourceSpec {
    fn from(value: SourceSpecVisitor) -> Self {
        let (ident, val) = value
            .identifier
            .unwrap_or_else(|| (SpecKind::Local, IdentKind::Path(PathBuf::from("."))));

        match ident {
            SpecKind::Local => SourceSpec::Local(LocalSource {
                path: val.get_path(),
                exclude: value.exclude.unwrap_or(LocalSource::default_exclude()),
                filter: value.filter.unwrap_or(LocalSource::default_filter()),
                subdir: value.subdir,
            }),
            SpecKind::Git => SourceSpec::Git(GitSource {
                git: val.get_git(),
                reference: value.reference.unwrap_or(String::from("")),
                depth: value.depth.unwrap_or(default_git_clone_depth()),
                subdir: value.subdir,
            }),
            SpecKind::Tar => SourceSpec::Tar(TarSource {
                tar: val.get_tar(),
                subdir: value.subdir,
            }),
            SpecKind::Script => SourceSpec::Script(ScriptSource {
                script: val.get_script(),
                subdir: value.subdir,
            }),
        }
    }
}

impl<'de> Deserialize<'de> for SourceSpec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(SourceSpecVisitor::default())?
            .into())
    }
}

impl<'de> Deserialize<'de> for LintedItem<SourceSpec> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(deserializer
            .deserialize_map(SourceSpecVisitor::default())?
            .into())
    }
}

impl<'de> serde::de::Visitor<'de> for SourceSpecVisitor {
    type Value = SourceSpecVisitor;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a source spec")
    }

    fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "path" => {
                    self.identifier = Some((
                        SpecKind::Local,
                        IdentKind::Path(map.next_value::<PathBuf>()?),
                    ));
                }
                "git" => {
                    self.identifier = Some((
                        SpecKind::Git,
                        IdentKind::Git(map.next_value::<Stringified>()?.0),
                    ))
                }
                "tar" => {
                    self.identifier = Some((
                        SpecKind::Tar,
                        IdentKind::Tar(map.next_value::<Stringified>()?.0),
                    ))
                }
                "script" => {
                    self.identifier = Some((
                        SpecKind::Script,
                        IdentKind::Script(map.next_value::<Script>()?),
                    ))
                }
                "exclude" => {
                    self.exclude = Some(
                        map.next_value::<Vec<Stringified>>()?
                            .into_iter()
                            .map(|s| s.0)
                            .collect(),
                    )
                }
                "filter" => {
                    self.filter = Some(
                        map.next_value::<Vec<Stringified>>()?
                            .into_iter()
                            .map(|s| s.0)
                            .collect(),
                    )
                }
                "ref" => self.reference = Some(map.next_value::<Stringified>()?.0),
                "depth" => self.depth = Some(map.next_value::<u32>()?),
                "subdir" => self.subdir = Some(map.next_value::<Stringified>()?.0),
                unknown_key => {
                    self.lints
                        .push(LintMessage::UnknownSourceSpecKey(SourceSpecKey::new(
                            unknown_key,
                        )));

                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        Ok(self)
    }
}

/// Package source files in a local directory or file path.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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
            path: PathBuf::from("."),
            exclude: Self::default_exclude(),
            filter: Self::default_filter(),
            subdir: None,
        }
    }
}

impl LocalSource {
    /// Create a new local source for the given path.
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    /// Place the collected local sources into the given subdirectory in the source package.
    pub fn set_subdir<S: ToString>(mut self, subdir: S) -> Self {
        self.subdir = Some(subdir.to_string());
        self
    }
}

impl LocalSource {
    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        let mut rsync = std::process::Command::new("rsync");
        rsync.arg("--archive");
        let mut path = dunce::canonicalize(&self.path)
            .map_err(|err| Error::InvalidPath(self.path.clone(), err))?;
        if path.is_dir() {
            // if the source path is a directory then we require
            // a trailing '/' so that rsync doesn't create new subdirectories
            // in the destination folder
            rsync.arg("--recursive");
            path = path.join("")
        }
        // require a trailing '/' on destination also so that rsync doesn't
        // add additional levels to the resulting structure
        let dirname = dirname.join("");
        if std::env::var("SPK_DEBUG").is_ok() {
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
        rsync.args([&path, &dirname]);
        tracing::debug!(cmd = ?rsync, "running");
        rsync.current_dir(&dirname);
        match rsync
            .status()
            .map_err(|err| {
                Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                    "rsync",
                    err,
                    Some(dirname.to_owned()),
                ))
            })?
            .code()
        {
            Some(0) => Ok(()),
            code => Err(Error::String(format!(
                "rsync command failed with exit code {code:?}"
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
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
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
        git_cmd.arg(dirname);

        let mut submodule_cmd = std::process::Command::new("git");
        submodule_cmd.args(["submodule", "update", "--init", "--recursive"]);
        if git_supports_submodule_depth() {
            submodule_cmd.arg("--depth");
            submodule_cmd.arg(self.depth.to_string());
        }

        for mut cmd in vec![git_cmd, submodule_cmd].into_iter() {
            tracing::debug!(?cmd, "running");
            cmd.current_dir(dirname);
            match cmd
                .status()
                .map_err(|err| {
                    Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                        "git",
                        err,
                        Some(dirname.to_owned()),
                    ))
                })?
                .code()
            {
                Some(0) => (),
                code => {
                    return Err(Error::String(format!(
                        "git command failed with exit code {code:?}"
                    )))
                }
            }
        }
        Ok(())
    }
}

/// Package source files from a local or remote tar archive.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TarSource {
    pub tar: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

impl TarSource {
    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path) -> Result<()> {
        let tmpdir = tempfile::Builder::new()
            .prefix("spk-untar")
            .tempdir()
            .map_err(Error::TempDirError)?;
        let tarfile = relative_path::RelativePathBuf::from(&self.tar);
        let filename = tarfile.file_name().unwrap_or_default();
        let mut tarfile = tmpdir.path().join(filename);
        let re = regex::Regex::new("^https?://").unwrap();
        if re.is_match(&self.tar) {
            let mut wget = std::process::Command::new("wget");
            wget.arg(&self.tar);
            wget.current_dir(tmpdir.path());
            tracing::debug!(cmd=?wget, "running");
            match wget
                .status()
                .map_err(|err| {
                    Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                        "wget",
                        err,
                        Some(tmpdir.path().to_owned()),
                    ))
                })?
                .code()
            {
                Some(0) => (),
                code => {
                    return Err(Error::String(format!(
                        "wget command failed with exit code {code:?}"
                    )))
                }
            }
        } else {
            let tar_path = std::path::PathBuf::from(&self.tar);
            tarfile =
                dunce::canonicalize(&tar_path).map_err(|err| Error::InvalidPath(tar_path, err))?;
        }

        let mut cmd = std::process::Command::new("tar");
        cmd.arg("-xf");
        cmd.arg(&tarfile);
        cmd.current_dir(dirname);
        tracing::debug!(?cmd, "running");
        match cmd
            .status()
            .map_err(|err| {
                Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                    "tar",
                    err,
                    Some(dirname.to_owned()),
                ))
            })?
            .code()
        {
            Some(0) => Ok(()),
            code => Err(Error::String(format!(
                "tar command failed with exit code {code:?}"
            ))),
        }
    }
}

/// Package source files collected via arbitrary shell script.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ScriptSource {
    pub script: Script,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
}

impl ScriptSource {
    /// Create a new script source that executes the given lines of script.
    pub fn new<I, S>(script: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            script: Script::new(script),
            subdir: None,
        }
    }

    /// Place the collected local sources into the given subdirectory in the source package.
    pub fn set_subdir<S: ToString>(mut self, subdir: S) -> Self {
        self.subdir = Some(subdir.to_string());
        self
    }

    /// Collect the represented sources files into the given directory.
    pub fn collect(&self, dirname: &Path, env: &HashMap<String, String>) -> Result<()> {
        let mut bash = std::process::Command::new("bash");
        bash.arg("-ex"); // print each command, exit on failure
        bash.arg("-"); // read from stdin
        bash.stdin(std::process::Stdio::piped());
        bash.envs(env);
        bash.current_dir(dirname);

        tracing::debug!("running sources script");
        let mut child = bash.spawn().map_err(|err| {
            Error::ProcessSpawnError(spfs::Error::process_spawn_error(
                "bash",
                err,
                Some(dirname.to_owned()),
            ))
        })?;
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

        match child.wait().map_err(Error::ProcessWaitError)?.code() {
            Some(0) => Ok(()),
            code => Err(Error::String(format!(
                "source script failed with exit code {code:?}"
            ))),
        }
    }
}

pub fn git_supports_submodule_depth() -> bool {
    let v = git_version();
    match v {
        None => false,
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
    out.trim().split(' ').last().map(|s| s.to_string())
}

fn default_git_clone_depth() -> u32 {
    1
}

fn is_default_git_clone_depth(depth: &u32) -> bool {
    depth == &default_git_clone_depth()
}
