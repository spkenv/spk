// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use anyhow::{anyhow, bail, Context, Result};
use nix::unistd::execv;
use spfs::prelude::*;
use std::env::{args_os, var_os};
use std::ffi::{CString, OsString};
use std::os::unix::{
    ffi::{OsStrExt, OsStringExt},
    fs::symlink,
};
use std::path::Path;
use tempdir::TempDir;

use spfs::encoding::Digest;
use spfs::storage::RepositoryHandle;
use spfs::tracking::EnvSpec;

const ORIGIN: &str = "origin";
const RPM_TAG: &str = "rpm";
/// spfs tags are placed into a subdirectory
/// called this, below `SpfsTag::spfs_tag_prefix`.
const SPFS_TAG_SUBDIR: &str = "spk-launcher";

trait SpfsTag {
    /// SPFS tag prefix where to find runnable platforms.
    /// This also used as the name of the application in
    /// diagnostic messages.
    fn spfs_tag_prefix() -> &'static str;
    /// Env var name containing version to run.
    fn tag_env_var() -> &'static str;
    /// Env var name to set with executable path.
    fn bin_var() -> &'static str;
    /// Relative path (inside spfs platform) to binary to run.
    fn rel_bin_path() -> &'static str;
    /// Absolute path (from rpm) to binary to run.
    fn rpm_bin_path() -> &'static str;
    /// Path to install into.
    fn install_path() -> &'static str;
}

struct Spawn;

impl SpfsTag for Spawn {
    fn spfs_tag_prefix() -> &'static str {
        "spawn"
    }

    fn tag_env_var() -> &'static str {
        "SPAWN_BIN_TAG"
    }

    fn bin_var() -> &'static str {
        "SPAWN_BIN_PATH"
    }

    fn rel_bin_path() -> &'static str {
        "opt/spawn.dist/spawn"
    }

    fn rpm_bin_path() -> &'static str {
        "/opt/spawn.dist/spawn"
    }

    fn install_path() -> &'static str {
        "/dev/shm/spawn"
    }
}

struct Spk;

impl SpfsTag for Spk {
    fn spfs_tag_prefix() -> &'static str {
        "spk"
    }

    fn tag_env_var() -> &'static str {
        "SPK_BIN_TAG"
    }

    fn bin_var() -> &'static str {
        "SPK_BIN_PATH"
    }

    fn rel_bin_path() -> &'static str {
        "opt/spk.dist/spk"
    }

    fn rpm_bin_path() -> &'static str {
        "/opt/spk.dist/spk"
    }

    fn install_path() -> &'static str {
        "/dev/shm/spk"
    }
}

/// Ensure requested version of spk is installed.
async fn check_or_install<S>(
    tag: &str,
    platform_digest: &Digest,
    local: &mut RepositoryHandle,
    remote: &RepositoryHandle,
) -> Result<OsString>
where
    S: SpfsTag,
{
    let digest_string = platform_digest.to_string();
    let install_location = Path::new(S::install_path()).join(&digest_string);
    if !install_location.exists() {
        spfs::runtime::makedirs_with_perms(S::install_path(), 0o777)?;

        let tag_as_dirname = tag.to_string().replace('/', "-");

        let temp_dir = TempDir::new_in(S::install_path(), &tag_as_dirname)
            .context("create temp working directory")?;

        // Ensure tag is sync'd local because `render_into_directory` operates
        // out of the local repo.
        spfs::sync_ref(tag, remote, local)
            .await
            .context("sync reference")?;

        let env_spec = EnvSpec::new(tag).context("create env spec")?;
        spfs::render_into_directory(&env_spec, temp_dir.path())
            .await
            .context("render spfs platform")?;

        let should_create_symlink = match std::fs::rename(temp_dir.path(), &install_location)
            .context("rename into place")
        {
            Ok(_) => true,
            Err(err) => match err.downcast_ref::<std::io::Error>() {
                // ErrorKind::DirectoryNotEmpty == 39; this is currently nightly-only.
                Some(io_err) if io_err.raw_os_error() == Some(39) => {
                    // It is extremely unlikely for this directory to suddenly
                    // exist unless it was another copy of this program racing
                    // to create it. Therefore, if it exists now, assume it is
                    // valid and that the symlink was also created.
                    // If we clobber the existing directory we could interfere
                    // with whatever concurrent process that created it.
                    // Our temp installation will be cleaned up when `temp_dir`
                    // is dropped.
                    false
                }
                _ => return Err(err),
            },
        };

        if should_create_symlink {
            let symlink_name = Path::new(S::install_path()).join(tag_as_dirname);
            // The symlink isn't required so any errors related to creating it
            // are ignored.
            let _ = if symlink_name.is_symlink() {
                // Symlink already exists; therefore it is not pointing
                // at the correct place.
                std::fs::remove_file(&symlink_name).context("remove existing symlink")
            } else if !symlink_name.exists() {
                Ok(())
            } else {
                Err(anyhow!("symlink target exists"))
            }
            .and_then(|_| symlink(&digest_string, &symlink_name).context("create symlink"));
        }
    }

    Ok(install_location.join(S::rel_bin_path()).into_os_string())
}

async fn execute<S>() -> Result<()>
where
    S: SpfsTag,
{
    let bin_tag = var_os(S::tag_env_var()).unwrap_or_else(|| RPM_TAG.into());
    let args = args_os()
        .map(|os_string| CString::new(os_string.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .context("valid CStrings")?;
    if bin_tag == RPM_TAG {
        execv(
            &CString::new(S::rpm_bin_path()).expect("valid CString"),
            args.as_slice(),
        )
        .context("process replaced")?;
        unreachable!();
    }

    let config = spfs::load_config().expect("loaded spfs config");
    let mut local_repo: RepositoryHandle = config
        .get_repository()
        .await
        .context("opened local spfs repo")?
        .into();
    let remote_repo = config
        .get_remote(ORIGIN)
        .await
        .context("opened remote spfs repo")?;

    let spfs_tag = format!(
        "{}/{}/{}",
        S::spfs_tag_prefix(),
        SPFS_TAG_SUBDIR,
        bin_tag.to_string_lossy(),
    );
    match remote_repo.read_ref(&spfs_tag).await {
        Err(spfs::Error::UnknownReference(_)) => {
            bail!(
                "Unable to resolve ${} == \"{}\"",
                S::tag_env_var(),
                bin_tag.to_string_lossy()
            );
        }
        Err(err) => bail!(err.to_string()),
        Ok(spfs::graph::Object::Platform(platform)) => {
            if platform.stack.is_empty() {
                bail!("Unexpected empty platform stack");
            }

            let bin_path = check_or_install::<S>(
                &spfs_tag,
                &platform.digest().context("get platform context")?,
                &mut local_repo,
                &remote_repo,
            )
            .await
            .with_context(|| format!("install requested version of {}", S::spfs_tag_prefix()))?;

            std::env::set_var(S::bin_var(), &bin_path);

            execv(
                &CString::new(bin_path.into_vec()).with_context(|| {
                    format!("convert {} bin path to CString", S::spfs_tag_prefix())
                })?,
                args.as_slice(),
            )
            .context("process replaced")?;
            unreachable!();
        }
        Ok(obj) => bail!("Expected platform object from spfs; found: {}", obj),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let application_name = Path::new(
        &args_os()
            .next()
            .ok_or_else(|| anyhow!("args missing"))
            .context("get application name")?,
    )
    .iter()
    .last()
    .ok_or_else(|| anyhow!("empty argv[0]?"))
    .context("get last component of argv[0])")?
    .to_owned();

    match application_name {
        x if x == "spk" || x == "spk-launcher" => execute::<Spk>().await.context("execute as spk"),
        x if x == "spawn" => execute::<Spawn>().await.context("execute as spawn"),
        x => bail!("Unhandled application name: {}", x.to_string_lossy()),
    }
}
