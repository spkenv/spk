use super::resolve::{resolve_overlay_dirs, which};
use super::status::{active_runtime, compute_runtime_manifest};
use crate::{runtime, Result};
use std::ffi::{OsStr, OsString};

#[cfg(test)]
#[path = "./bootstrap_test.rs"]
mod bootstrap_test;

/// Construct a bootstrap command.
///
/// The returned command properly calls through the relevant spfs
/// binaries and runs the desired command in an existing runtime.
pub fn build_command_for_runtime(
    runtime: runtime::Runtime,
    command: OsString,
    args: &mut Vec<OsString>,
) -> Result<(OsString, Vec<OsString>)> {
    match which("spfs") {
        None => Err("'spfs' not found in PATH".into()),
        Some(spfs_exe) => {
            let mut spfs_args = vec![
                spfs_exe.as_os_str().to_owned(),
                "init-runtime".into(),
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
pub fn build_interactive_shell_cmd() -> Result<Vec<OsString>> {
    let rt = active_runtime()?;
    let shell_path = std::env::var("SHELL").unwrap_or("<not-set>".to_string());
    let shell_name = std::path::Path::new(shell_path.as_str())
        .file_name()
        .unwrap_or_else(|| OsStr::new("bash"));

    match shell_name.to_str() {
        Some("tcsh") => match which("expect") {
            None => {
                tracing::error!("'expect' command not found in PATH, falling back to bash");
            }
            Some(expect) => {
                return Ok(vec![
                    expect.as_os_str().to_owned(),
                    rt.csh_expect_file.into(),
                    shell_path.into(),
                    rt.csh_startup_file.into(),
                ]);
            }
        },
        Some("bash") => (),
        _ => {
            tracing::warn!(
                "current shell not supported ({:?}) - using bash",
                shell_name
            );
        }
    }

    let shell_path = "/usr/bin/bash";
    Ok(vec![
        shell_path.into(),
        "--init-file".into(),
        rt.sh_startup_file.into(),
    ])
}

/// Construct a boostrapping command for initializing through the shell.
///
/// The returned command properly calls through a shell which sets up
/// the current runtime appropriately before calling the desired command.
pub fn build_shell_initialized_command(
    command: OsString,
    args: &mut Vec<OsString>,
) -> Result<Vec<OsString>> {
    let runtime = active_runtime()?;
    let default_shell = which("bash").unwrap_or_default();
    let desired_shell = std::env::var_os("SHELL").unwrap_or_else(|| default_shell.into());
    let shell_name = std::path::Path::new(&desired_shell)
        .file_name()
        .unwrap_or_else(|| OsStr::new("bash"))
        .to_string_lossy()
        .to_string();
    let startup_file = match shell_name.as_str() {
        "bash" | "sh" => runtime.sh_startup_file.clone(),
        "tcsh" | "csh" => runtime.csh_startup_file.clone(),
        _ => return Err("No supported shell found, or no support for current shell".into()),
    };

    let mut cmd = vec![desired_shell, startup_file.into(), command];
    cmd.append(args);
    Ok(cmd)
}

fn build_spfs_enter_command(
    rt: runtime::Runtime,
    command: &mut Vec<OsString>,
) -> Result<(OsString, Vec<OsString>)> {
    let config = crate::load_config()?;
    let exe = match which("spfs-enter") {
        None => return Err("'spfs-enter' not found in PATH".into()),
        Some(exe) => exe,
    };

    let mut args = vec![];

    let overlay_dirs = resolve_overlay_dirs(&rt)?;
    for dirpath in overlay_dirs {
        args.push("-d".into());
        args.push(dirpath.into());
    }

    if rt.is_editable() {
        args.push("-e".into());
        args.push("-t".into());
        args.push(format!("size={}", config.filesystem.tmpfs_size).into());
    }

    tracing::debug!("computing runtime manifest");
    let manifest = compute_runtime_manifest(&rt)?;

    tracing::debug!("finding files that should be masked");
    for node in manifest.walk_abs("/spfs") {
        if !node.entry.kind.is_mask() {
            continue;
        }
        args.push("-m".into());
        args.push(node.path.to_path("").into());
    }

    args.push("--".into());
    args.append(command);
    Ok((exe.into(), args))
}
