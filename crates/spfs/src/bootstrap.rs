// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use super::resolve::{which, which_spfs};
use crate::{runtime, Error, Result};

#[cfg(test)]
#[path = "./bootstrap_test.rs"]
mod bootstrap_test;

/// Construct a bootstrap command.
///
/// The returned command properly calls through the relevant spfs
/// binaries and runs the desired command in an existing runtime.
pub fn build_command_for_runtime(
    runtime: &runtime::Runtime,
    command: OsString,
    args: &mut Vec<OsString>,
) -> Result<(OsString, Vec<OsString>)> {
    match which_spfs("init") {
        None => Err(Error::MissingBinary("spfs-init")),
        Some(spfs_init_exe) => {
            let mut spfs_args = vec![
                spfs_init_exe.as_os_str().to_owned(),
                "--runtime-dir".into(),
                runtime.root().into(),
                "--".into(),
                command,
            ];
            spfs_args.append(args);
            build_spfs_enter_command(runtime, &mut spfs_args)
        }
    }
}

/// Return a command that initializes and runs an interactive shell
///
/// The returned command properly sets up and runs an interactive
/// shell session in the current runtime.
pub fn build_interactive_shell_cmd(rt: &runtime::Runtime) -> Result<Vec<OsString>> {
    let mut shell_path = std::path::PathBuf::from(
        std::env::var("SHELL").unwrap_or_else(|_| "<not-set>".to_string()),
    );
    let shell_name = shell_path
        .file_name()
        .unwrap_or_else(|| OsStr::new("bash"))
        .to_os_string();

    if !shell_path.is_absolute() {
        shell_path = match which(shell_name.to_string_lossy()) {
            None => {
                tracing::error!(
                    "'{}' not found in PATH, falling back to /usr/bin/bash",
                    shell_name.to_string_lossy()
                );
                std::path::PathBuf::from("/usr/bin/bash")
            }
            Some(path) => path,
        }
    }

    if let Some("tcsh") = shell_name.to_str() {
        match which("expect") {
            None => {
                tracing::error!("'expect' command not found in PATH, falling back to bash");
            }
            Some(expect) => {
                return Ok(vec![
                    expect.as_os_str().to_owned(),
                    rt.csh_expect_file.as_os_str().to_owned(),
                    shell_path.into(),
                    rt.csh_startup_file.as_os_str().to_owned(),
                ]);
            }
        }
    }

    match shell_name.to_str() {
        Some("bash") => (),
        _ => {
            tracing::warn!(
                "shell not supported ({:?}) - trying bash instead",
                shell_name
            );
            shell_path = PathBuf::from("/usr/bin/bash");
        }
    }
    Ok(vec![
        shell_path.into(),
        "--init-file".into(),
        rt.sh_startup_file.as_os_str().to_owned(),
    ])
}

/// Construct a boostrapping command for initializing through the shell.
///
/// The returned command properly calls through a shell which sets up
/// the current runtime appropriately before calling the desired command.
pub fn build_shell_initialized_command(
    runtime: &runtime::Runtime,
    command: OsString,
    args: &mut Vec<OsString>,
) -> Result<Vec<OsString>> {
    let desired_shell =
        std::env::var_os("SHELL").unwrap_or_else(|| which("bash").unwrap_or_default().into());
    let shell_name = std::path::Path::new(&desired_shell)
        .file_name()
        .unwrap_or_else(|| OsStr::new("bash"))
        .to_string_lossy()
        .to_string();
    let startup_file = match shell_name.as_str() {
        "bash" | "sh" => &runtime.sh_startup_file,
        "tcsh" | "csh" => &runtime.csh_startup_file,
        _ => return Err(Error::NoSupportedShell),
    };

    let mut cmd = vec![desired_shell, startup_file.into(), command];
    cmd.append(args);
    Ok(cmd)
}

pub(crate) fn build_spfs_remount_command(
    rt: &runtime::Runtime,
) -> Result<(OsString, Vec<OsString>)> {
    let exe = match which_spfs("enter") {
        None => return Err(Error::MissingBinary("spfs-enter")),
        Some(exe) => exe,
    };

    Ok((
        exe.into(),
        vec!["--remount".into(), rt.root().into(), "--".into()],
    ))
}

fn build_spfs_enter_command(
    rt: &runtime::Runtime,
    command: &mut Vec<OsString>,
) -> Result<(OsString, Vec<OsString>)> {
    let exe = match which_spfs("enter") {
        None => return Err(Error::MissingBinary("spfs-enter")),
        Some(exe) => exe,
    };

    let mut args = vec![rt.root().into(), "--".into()];
    args.append(command);
    Ok((exe.into(), args))
}
