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

/// A command to be executed
pub struct Command {
    pub executable: OsString,
    pub args: Vec<OsString>,
}

impl Command {
    /// Turns this command into a synchronously runnable one
    pub fn into_std(self) -> std::process::Command {
        let mut cmd = std::process::Command::new(self.executable);
        cmd.args(self.args);
        cmd
    }

    /// Turns this command into an asynchronously runnable one
    pub fn into_tokio(self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(self.executable);
        cmd.args(self.args);
        cmd
    }
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Command")
            .field(&self.executable)
            .field(&self.args)
            .finish()
    }
}

/// Construct a bootstrap command.
///
/// The returned command properly calls through the relevant spfs
/// binaries and runs the desired command in an existing runtime.
pub fn build_command_for_runtime<E, A, S>(
    runtime: &runtime::Runtime,
    command: E,
    args: A,
) -> Result<Command>
where
    E: Into<OsString>,
    A: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    build_spfs_enter_command(runtime, command, args)
}

/// Return a command that initializes and runs an interactive shell
///
/// The returned command properly sets up and runs an interactive
/// shell session in the current runtime.
pub fn build_interactive_shell_command(rt: &runtime::Runtime) -> Result<Command> {
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
                return Ok(Command {
                    executable: expect.into(),
                    args: vec![
                        rt.config.csh_expect_file.clone().into(),
                        shell_path.into(),
                        rt.config.csh_startup_file.clone().into(),
                    ],
                });
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
    Ok(Command {
        executable: shell_path.into(),
        args: vec![
            "--init-file".into(),
            rt.config.sh_startup_file.as_os_str().to_owned(),
        ],
    })
}

/// Construct a bootstrapping command for initializing through the shell.
///
/// The returned command properly calls through a shell which sets up
/// the current runtime appropriately before calling the desired command.
pub fn build_shell_initialized_command<E, A, S>(
    runtime: &runtime::Runtime,
    command: E,
    args: A,
) -> Result<Command>
where
    E: Into<OsString>,
    A: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let desired_shell =
        std::env::var_os("SHELL").unwrap_or_else(|| which("bash").unwrap_or_default().into());
    let shell_name = std::path::Path::new(&desired_shell)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let startup_file = match shell_name.as_str() {
        "bash" | "sh" => &runtime.config.sh_startup_file,
        "tcsh" | "csh" => &runtime.config.csh_startup_file,
        _ => return Err(Error::NoSupportedShell),
    };

    let mut shell_args = vec![startup_file.into(), command.into()];
    shell_args.extend(args.into_iter().map(Into::into));

    Ok(Command {
        executable: desired_shell,
        args: shell_args,
    })
}

pub(crate) fn build_spfs_remount_command(rt: &runtime::Runtime) -> Result<Command> {
    let exe = match which_spfs("enter") {
        None => return Err(Error::MissingBinary("spfs-enter")),
        Some(exe) => exe,
    };

    let args = vec![
        "--remount".into(),
        "--runtime-storage".into(),
        rt.storage().address().to_string().into(),
        "--runtime".into(),
        rt.name().into(),
        "--".into(),
    ];
    Ok(Command {
        executable: exe.into(),
        args,
    })
}

fn build_spfs_enter_command<E, A, S>(rt: &runtime::Runtime, command: E, args: A) -> Result<Command>
where
    E: Into<OsString>,
    A: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let exe = match which_spfs("enter") {
        None => return Err(Error::MissingBinary("spfs-enter")),
        Some(exe) => exe,
    };

    let mut enter_args = vec![
        "--runtime-storage".into(),
        rt.storage().address().to_string().into(),
        "--runtime".into(),
        rt.name().into(),
        "--".into(),
        command.into(),
    ];
    enter_args.extend(args.into_iter().map(Into::into));
    Ok(Command {
        executable: exe.into(),
        args: enter_args,
    })
}

// fn find shell
// /etc/shells
