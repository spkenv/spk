// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

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
    let shell = find_best_shell()?;
    match shell {
        Shell::Tcsh { tcsh, expect } => Ok(Command {
            executable: expect.into(),
            args: vec![
                rt.config.csh_expect_file.clone().into(),
                tcsh.into(),
                rt.config.csh_startup_file.clone().into(),
            ],
        }),

        Shell::Bash(bash) => Ok(Command {
            executable: bash.into(),
            args: vec![
                "--init-file".into(),
                rt.config.sh_startup_file.as_os_str().to_owned(),
            ],
        }),
    }
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
    let shell = find_best_shell()?;
    let startup_file = match shell.kind() {
        ShellKind::Bash => &runtime.config.sh_startup_file,
        ShellKind::Tcsh => &runtime.config.csh_startup_file,
    };

    let mut shell_args = vec![startup_file.into(), command.into()];
    shell_args.extend(args.into_iter().map(Into::into));

    Ok(Command {
        executable: shell.executable().into(),
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

    let mut enter_args = Vec::new();

    // Capture the current $TMPDIR value here before it is lost when running
    // privileged process spfs-enter.
    if let Some(tmpdir_value_for_child_process) = std::env::var_os("TMPDIR") {
        tracing::trace!(
            ?tmpdir_value_for_child_process,
            "capture existing value for $TMPDIR (build_spfs_enter_command)"
        );

        enter_args.extend(["--tmpdir".into(), tmpdir_value_for_child_process]);
    }

    enter_args.extend([
        "--runtime-storage".into(),
        rt.storage().address().to_string().into(),
        "--runtime".into(),
        rt.name().into(),
        "--".into(),
        command.into(),
    ]);
    enter_args.extend(args.into_iter().map(Into::into));
    Ok(Command {
        executable: exe.into(),
        args: enter_args,
    })
}

/// The set of supported shells which spfs can run under
enum ShellKind {
    Bash,
    Tcsh,
}

impl AsRef<str> for ShellKind {
    fn as_ref(&self) -> &str {
        match self {
            Self::Bash => "bash",
            Self::Tcsh => "tcsh",
        }
    }
}

/// A supported shell that exists on this system
enum Shell {
    Bash(PathBuf),
    Tcsh { tcsh: PathBuf, expect: PathBuf },
}

impl Shell {
    fn kind(&self) -> ShellKind {
        match self {
            Self::Bash(_) => ShellKind::Bash,
            Self::Tcsh { .. } => ShellKind::Tcsh,
        }
    }

    fn executable(&self) -> &Path {
        match self {
            Self::Bash(p) => p,
            Self::Tcsh { tcsh, .. } => tcsh,
        }
    }

    fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        match path.file_name().map(OsStr::to_string_lossy) {
            Some(n) if n == ShellKind::Bash.as_ref() => Ok(Self::Bash(path.to_owned())),
            Some(n) if n == ShellKind::Tcsh.as_ref() => {
                let expect = which("expect").ok_or_else(|| {
                    Error::new("Cannot run tcsh without expect, and 'expect' was not found in PATH")
                })?;
                Ok(Self::Tcsh {
                    tcsh: path.to_owned(),
                    expect,
                })
            }
            Some(_) => Err(Error::new(format!("Unsupported shell: {path:?}"))),
            None => Err(Error::new(format!("Invalid shell path: {path:?}"))),
        }
    }
}

/// Looks for the most desired shell to use for bootstrapping.
///
/// In general, this strategy uses the value of SHELL before
/// searching for viable entries in PATH and then falling back
/// to whatever it can find listed in /etc/shells
fn find_best_shell() -> Result<Shell> {
    let mut desired = None;
    if let Ok(name) = std::env::var("SHELL") {
        if Path::new(&name).is_absolute() {
            desired = Some(PathBuf::from(name));
        } else {
            desired = which(name);
        }
    }

    if let Some(path) = desired {
        if let Ok(shell) = Shell::from_path(path) {
            return Ok(shell);
        }
    }

    for kind in &[ShellKind::Bash, ShellKind::Tcsh] {
        if let Some(path) = which(kind) {
            if let Ok(shell) = Shell::from_path(path) {
                return Ok(shell);
            }
        }
    }

    if let Ok(shells) = std::fs::read_to_string("/etc/shells") {
        for candidate in shells.split('\n') {
            let path = Path::new(candidate.trim());
            if let Ok(shell) = Shell::from_path(path) {
                return Ok(shell);
            }
        }
    }

    Err(Error::NoSupportedShell)
}
