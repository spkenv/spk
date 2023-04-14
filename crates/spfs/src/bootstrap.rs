// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::prelude::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::resolve::{which, which_spfs};
use crate::{runtime, Error, Result};

#[cfg(test)]
#[path = "./bootstrap_test.rs"]
mod bootstrap_test;

/// Environment variable used to store the original value of HOME
/// when launching through certain shells (tcsh).
const SPFS_ORIGINAL_HOME: &str = "SPFS_ORIGINAL_HOME";

/// A command to be executed
#[derive(Debug, Clone)]
pub struct Command {
    pub executable: OsString,
    pub args: Vec<OsString>,
    /// A list of NAME=value pairs to set in the
    /// launched environment
    pub vars: Vec<(OsString, OsString)>,
}

impl Command {
    /// Turns this command into a synchronously runnable one
    pub fn into_std(self) -> std::process::Command {
        let mut cmd = std::process::Command::new(self.executable);
        cmd.args(self.args);
        cmd.envs(self.vars);
        cmd
    }

    /// Turns this command into an asynchronously runnable one
    pub fn into_tokio(self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(self.executable);
        cmd.args(self.args);
        cmd.envs(self.vars);
        cmd
    }

    /// Execute this command, replacing the current program.
    ///
    /// Upon success, this function will never return. Upon
    /// error, the current process' environment will have been updated
    /// to that of this command, and caution should be taken.
    #[cfg(target_os = "linux")]
    pub fn exec(self) -> Result<std::convert::Infallible> {
        tracing::debug!("{self:#?}");
        // ensure that all components of this command are utilized
        let Self {
            executable,
            args,
            vars,
        } = self;
        let exe = CString::new(executable.into_vec()).map_err(crate::Error::CommandHasNul)?;
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(exe);
        for arg in args.into_iter() {
            argv.push(CString::new(arg.into_vec()).map_err(crate::Error::CommandHasNul)?);
        }
        for (name, value) in vars {
            // set the environment to be inherited by the new process
            std::env::set_var(name, value);
        }
        nix::unistd::execv(&argv[0], argv.as_slice()).map_err(crate::Error::from)
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
    sync_time: Duration,
) -> Result<Command>
where
    E: Into<OsString>,
    A: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    build_spfs_enter_command(runtime, command, args, sync_time)
}

/// Return a command that initializes and runs an interactive shell
///
/// The returned command properly sets up and runs an interactive
/// shell session in the current runtime.
///
/// If `shell` is not specified, `$SHELL` will be read from the environment.
pub fn build_interactive_shell_command(
    rt: &runtime::Runtime,
    shell: Option<&str>,
) -> Result<Command> {
    let shell = find_best_shell(shell)?;
    match shell {
        Shell::Tcsh(tcsh) => Ok(Command {
            executable: tcsh.into(),
            args: vec![],
            vars: vec![
                (
                    SPFS_ORIGINAL_HOME.into(),
                    std::env::var_os("HOME").unwrap_or_default(),
                ),
                (
                    "HOME".into(),
                    rt.config
                        .csh_startup_file
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .as_os_str()
                        .to_owned(),
                ),
            ],
        }),

        Shell::Bash(bash) => Ok(Command {
            executable: bash.into(),
            args: vec![
                "--init-file".into(),
                rt.config.sh_startup_file.as_os_str().to_owned(),
            ],
            vars: vec![],
        }),
    }
}

/// Construct a bootstrapping command for initializing through the shell.
///
/// The returned command properly calls through a shell which sets up
/// the current runtime appropriately before calling the desired command.
///
/// If `shell` is not specified, `$SHELL` will be read from the environment.
pub fn build_shell_initialized_command<E, A, S>(
    runtime: &runtime::Runtime,
    shell: Option<&str>,
    command: E,
    args: A,
) -> Result<Command>
where
    E: Into<OsString>,
    A: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let shell = find_best_shell(shell)?;
    let startup_file = match shell.kind() {
        ShellKind::Bash => &runtime.config.sh_startup_file,
        ShellKind::Tcsh => &runtime.config.csh_startup_file,
    };

    let mut shell_args = vec![startup_file.into(), command.into()];
    shell_args.extend(args.into_iter().map(Into::into));

    Ok(Command {
        executable: shell.executable().into(),
        args: shell_args,
        vars: vec![],
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
        vars: vec![],
    })
}

fn build_spfs_enter_command<E, A, S>(
    rt: &runtime::Runtime,
    command: E,
    args: A,
    sync_time: Duration,
) -> Result<Command>
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
        "--sync-time-seconds".into(),
        sync_time.as_secs_f64().to_string().into(),
        "--metrics-in-env".into(),
        "--".into(),
        command.into(),
    ]);
    enter_args.extend(args.into_iter().map(Into::into));
    Ok(Command {
        executable: exe.into(),
        args: enter_args,
        vars: vec![],
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
#[derive(Debug)]
enum Shell {
    Bash(PathBuf),
    Tcsh(PathBuf),
}

impl Shell {
    fn kind(&self) -> ShellKind {
        match self {
            Self::Bash(_) => ShellKind::Bash,
            Self::Tcsh(_) => ShellKind::Tcsh,
        }
    }

    fn executable(&self) -> &Path {
        match self {
            Self::Bash(p) => p,
            Self::Tcsh(p) => p,
        }
    }

    fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        match path.file_name().map(OsStr::to_string_lossy) {
            Some(n) if n == ShellKind::Bash.as_ref() => Ok(Self::Bash(path.to_owned())),
            Some(n) if n == ShellKind::Tcsh.as_ref() => Ok(Self::Tcsh(path.to_owned())),
            Some(_) => Err(Error::new(format!("Unsupported shell: {path:?}"))),
            None => Err(Error::new(format!("Invalid shell path: {path:?}"))),
        }
    }
}

/// Looks for the most desired shell to use for bootstrapping.
///
/// If `shell` is not provided, read the value of `$SHELL` from the
/// environment.
///
/// In general, this strategy uses the value of SHELL before
/// searching for viable entries in PATH and then falling back
/// to whatever it can find listed in /etc/shells
fn find_best_shell(shell: Option<&str>) -> Result<Shell> {
    let shell = shell
        .map(|s| s.to_string())
        .or_else(|| std::env::var("SHELL").ok());

    let mut desired = None;
    if let Some(name) = shell {
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
