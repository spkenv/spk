// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::env::{args_os, var_os};
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use miette::{bail, miette, Context, IntoDiagnostic, Result};
use nix::unistd::execv;
use spfs::encoding::Digest;
use spfs::prelude::*;
use spfs::storage::fallback::FallbackProxy;
use spfs::storage::fs::OpenFsRepository;
use spfs::storage::RepositoryHandle;
use spfs::tracking::EnvSpec;
use spfs::OsError;

const DEV_SHM: &str = "/dev/shm";
const ORIGIN: &str = "origin";
/// Variable to customize where applications are installed into,
/// defaults to `DEV_SHM`.
const ROOT_ENV_VAR: &str = "SPK_LAUNCHER_INSTALL_ROOT";
const RPM_TAG: &str = "rpm";
/// spfs tags are placed into a subdirectory
/// called this, below `SpfsTag::spfs_tag_prefix`.
const SPFS_TAG_SUBDIR: &str = "spk-launcher";

trait SpfsTag {
    /// SPFS tag prefix where to find runnable platforms.
    /// This also used as the name of the application in
    /// diagnostic messages.
    fn spfs_tag_prefix(&self) -> &OsStr;
    /// Env var name containing version to run.
    fn tag_env_var(&self) -> OsString;
    /// Env var name to set with executable path.
    fn bin_var(&self) -> OsString;
    /// Relative path (inside spfs platform) to binary to run.
    fn rel_bin_path(&self) -> PathBuf;
    /// Absolute path (from rpm) to binary to run.
    fn rpm_bin_path(&self) -> PathBuf;
    /// Path to install into.
    fn install_path(&self) -> PathBuf;
}

struct Dynamic<'a> {
    /// The executable name being wrapped, e.g., "spk"
    exe_name: &'a OsStr,
    /// The executable name, converted to uppercase.
    upper_case_exe_name: String,
}

impl<'a> SpfsTag for Dynamic<'a> {
    fn spfs_tag_prefix(&self) -> &'a OsStr {
        self.exe_name
    }

    fn tag_env_var(&self) -> OsString {
        format!("{}_BIN_TAG", self.upper_case_exe_name).into()
    }

    fn bin_var(&self) -> OsString {
        format!("{}_BIN_PATH", self.upper_case_exe_name).into()
    }

    fn rel_bin_path(&self) -> PathBuf {
        let mut buf = PathBuf::new();
        let lossy_exe_name = self.exe_name.to_string_lossy();
        buf.push("opt");
        buf.push(format!("{lossy_exe_name}.dist"));
        buf.push(self.exe_name);
        buf
    }

    fn rpm_bin_path(&self) -> PathBuf {
        let mut buf = PathBuf::from("/");
        buf.push(self.rel_bin_path());
        buf
    }

    fn install_path(&self) -> PathBuf {
        let mut buf = PathBuf::from(var_os(ROOT_ENV_VAR).unwrap_or_else(|| DEV_SHM.into()));
        buf.push(self.exe_name);
        buf
    }
}

impl<'a> Dynamic<'a> {
    fn new(exe_name: &'a OsStr) -> Self {
        Self {
            upper_case_exe_name: exe_name.to_string_lossy().to_uppercase(),
            exe_name,
        }
    }

    /// Ensure requested version of spk is installed.
    async fn check_or_install(
        &self,
        tag: &str,
        platform_digest: &Digest,
        local: Arc<OpenFsRepository>,
        remote: RepositoryHandle,
    ) -> Result<OsString> {
        let digest_string = platform_digest.to_string();
        let install_location = Path::new(self.install_path().as_os_str()).join(&digest_string);
        if !install_location.exists() {
            spfs::runtime::makedirs_with_perms(self.install_path(), 0o777)
                .into_diagnostic()
                .wrap_err("makedirs_with_perms")?;

            let tag_as_dirname = tag.to_string().replace('/', "-");

            let temp_dir = tempfile::Builder::new()
                .prefix(&tag_as_dirname)
                .tempdir_in(self.install_path())
                .into_diagnostic()
                .wrap_err("create temp working directory")?;

            let env_spec = EnvSpec::parse(tag).wrap_err("create env spec")?;

            // Ensure tag is sync'd local because `render_into_directory` operates
            // out of the local repo.
            let handle = local.clone().into();
            let syncer = spfs::Syncer::new(&remote, &handle)
                .with_policy(spfs::sync::SyncPolicy::LatestTags)
                .with_reporter(spfs::sync::ConsoleSyncReporter::default());
            let r = syncer.sync_env(env_spec).await.wrap_err("sync reference")?;
            let env_spec = r.env;

            let fallback = FallbackProxy::new(local, vec![remote]);

            spfs::storage::fs::Renderer::new(&fallback)
                .with_reporter(spfs::storage::fs::ConsoleRenderReporter::default())
                .render_into_directory(
                    env_spec,
                    temp_dir.path(),
                    spfs::storage::fs::RenderType::Copy,
                )
                .await
                .wrap_err("render spfs platform")?;

            let should_create_symlink = match std::fs::rename(temp_dir.path(), &install_location)
                .into_diagnostic()
                .wrap_err("rename into place")
            {
                Ok(_) => true,
                Err(err) => match err.downcast_ref::<std::io::Error>() {
                    // ErrorKind::DirectoryNotEmpty == 39; this is currently nightly-only.
                    Some(io_err) if io_err.os_error() == Some(39) => {
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
                    Some(io_err) if matches!(io_err.kind(), std::io::ErrorKind::AlreadyExists) => {
                        // Somebody put a file in the place where we wanted to
                        // put our new directory. This is an error situation.
                        bail!(
                            "a file unexpectedly appeared in the way: {}",
                            install_location.display()
                        )
                    }
                    _ => return Err(err),
                },
            };

            if should_create_symlink {
                let symlink_name = Path::new(self.install_path().as_os_str()).join(tag_as_dirname);
                // The symlink isn't required so any errors related to creating it
                // are ignored.
                let _ = if symlink_name.is_symlink() {
                    // Symlink already exists; therefore it is not pointing
                    // at the correct place.
                    std::fs::remove_file(&symlink_name)
                        .into_diagnostic()
                        .wrap_err("remove existing symlink")
                } else if !symlink_name.exists() {
                    Ok(())
                } else {
                    Err(miette!("symlink target exists"))
                }
                .and_then(|_| {
                    symlink(&digest_string, &symlink_name)
                        .into_diagnostic()
                        .wrap_err("create symlink")
                });
            }
        }

        Ok(install_location.join(self.rel_bin_path()).into_os_string())
    }

    async fn execute(&self) -> Result<()> {
        let bin_tag = var_os(self.tag_env_var()).unwrap_or_else(|| RPM_TAG.into());
        let args = args_os()
            .map(|os_string| CString::new(os_string.as_bytes()))
            .collect::<Result<Vec<_>, _>>()
            .into_diagnostic()
            .wrap_err("valid CStrings")?;
        if bin_tag == RPM_TAG {
            let bin = CString::new(AsRef::<OsStr>::as_ref(&self.rpm_bin_path()).as_bytes())
                .expect("valid CString");
            execv(&bin, args.as_slice())
                .into_diagnostic()
                .wrap_err_with(|| format!("execv({}, ...)", bin.to_string_lossy()))?;
            unreachable!();
        }

        let config = spfs::load_config().expect("loaded spfs config");
        let local_repo = config
            .get_opened_local_repository()
            .await
            .wrap_err("open local spfs repo")?;
        let remote_repo = config
            .get_remote(ORIGIN)
            .await
            .wrap_err("opened remote spfs repo")?;

        let spfs_tag = format!(
            "{}/{}/{}",
            self.spfs_tag_prefix().to_string_lossy(),
            SPFS_TAG_SUBDIR,
            bin_tag.to_string_lossy(),
        );
        match remote_repo.read_ref(&spfs_tag).await {
            Err(spfs::Error::UnknownReference(_)) => {
                bail!(
                    "Unable to resolve ${} == \"{}\"",
                    self.tag_env_var().to_string_lossy(),
                    bin_tag.to_string_lossy()
                );
            }
            Err(err) => bail!(err.to_string()),
            Ok(spfs::graph::Object::Platform(platform)) => {
                if platform.stack.is_empty() {
                    bail!("Unexpected empty platform stack");
                }

                let bin_path = self
                    .check_or_install(
                        &spfs_tag,
                        &platform.digest().wrap_err("get platform context")?,
                        local_repo.into(),
                        remote_repo,
                    )
                    .await
                    .wrap_err_with(|| {
                        format!(
                            "install requested version of {}",
                            self.spfs_tag_prefix().to_string_lossy()
                        )
                    })?;

                std::env::set_var(self.bin_var(), &bin_path);

                execv(
                    &CString::new(bin_path.into_vec())
                        .into_diagnostic()
                        .wrap_err_with(|| {
                            format!(
                                "convert {} bin path to CString",
                                self.spfs_tag_prefix().to_string_lossy()
                            )
                        })?,
                    args.as_slice(),
                )
                .into_diagnostic()
                .wrap_err("process replaced")?;
                unreachable!();
            }
            Ok(obj) => bail!("Expected platform object from spfs; found: {}", obj),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let application_name = Path::new(
        &args_os()
            .next()
            .ok_or_else(|| miette!("args missing"))
            .wrap_err("get application name")?,
    )
    .iter()
    .last()
    .ok_or_else(|| miette!("empty argv[0]?"))
    .wrap_err("get last component of argv[0])")?
    .to_owned();

    Dynamic::new(application_name.as_os_str())
        .execute()
        .await
        .wrap_err_with(|| format!("execute as {}", application_name.to_string_lossy()))
}
