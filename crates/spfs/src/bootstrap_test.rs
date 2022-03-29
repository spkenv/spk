// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{build_interactive_shell_cmd, build_shell_initialized_command};
use crate::{resolve::which, runtime};
use std::{ffi::OsString, process::Command};

use crate::fixtures::*;

#[rstest(
    shell,
    startup_script,
    startup_cmd,
    case("bash", "test.sh", "export TEST_VALUE='spfs-test-value'"),
    case("tcsh", "test.csh", "setenv TEST_VALUE 'spfs-test-value'")
)]
#[serial_test::serial] // env and config manipulation must be reliable
fn test_shell_initialization_startup_scripts(
    shell: &str,
    startup_script: &str,
    startup_cmd: &str,
    tmpdir: tempdir::TempDir,
) {
    init_logging();
    let shell_path = match which(shell) {
        Some(path) => path,
        None => {
            println!("{shell} not available on this system");
            return;
        }
    };

    let storage = runtime::Storage::new(tmpdir.path()).unwrap();
    let rt = storage.create_runtime().unwrap();

    let setenv = |cmd: &mut std::process::Command| {
        cmd.env("SPFS_STORAGE_RUNTIMES", rt.root().parent().unwrap());
        cmd.env("SPFS_RUNTIME", rt.name());
        cmd.env("SPFS_DEBUG", "1");
        cmd.env("SHELL", &shell_path);
    };

    let tmp_startup_dir = tmpdir.path().join("startup.d");
    std::fs::create_dir(&tmp_startup_dir).unwrap();
    for startup_script in &[&rt.sh_startup_file, &rt.csh_startup_file] {
        let mut cmd = Command::new("sed");
        cmd.arg("-i");
        cmd.arg(format!(
            "s|/spfs/etc/spfs/startup.d|{}|",
            tmp_startup_dir.to_string_lossy()
        ));
        cmd.arg(startup_script);
        setenv(&mut cmd);
        println!("{cmd:?}");
        println!("{:?}", cmd.output().unwrap());
    }

    std::fs::write(tmp_startup_dir.join(startup_script), startup_cmd).unwrap();

    std::env::set_var("SHELL", &shell_path);
    std::env::set_var("SPFS_RUNTIME", &rt.name());

    let mut config = crate::Config::load().unwrap();
    config.storage.runtimes = Some(rt.root().parent().unwrap().to_owned());
    config.make_current().unwrap();

    let args = build_shell_initialized_command(
        OsString::from("printenv"),
        &mut vec![OsString::from("TEST_VALUE")],
    )
    .unwrap();
    let mut cmd = Command::new(args.get(0).unwrap());
    cmd.args(args[1..].iter());
    setenv(&mut cmd);
    println!("{cmd:?}");
    let out = cmd.output().unwrap();
    rt.delete().unwrap();
    println!("{out:?}");
    assert!(out.stdout.ends_with("spfs-test-value\n".as_bytes()));
}

#[rstest(shell, case("bash"), case("tcsh"))]
#[serial_test::serial] // env and config manipulation must be reliable
fn test_shell_initialization_no_startup_scripts(shell: &str, tmpdir: tempdir::TempDir) {
    let shell_path = match which(shell) {
        Some(path) => path,
        None => {
            println!("{shell} not available on this system");
            return;
        }
    };

    let storage = runtime::Storage::new(tmpdir.path()).unwrap();
    let rt = storage.create_runtime().unwrap();

    let setenv = |cmd: &mut std::process::Command| {
        cmd.env("SPFS_STORAGE_RUNTIMES", rt.root().parent().unwrap());
        cmd.env("SPFS_RUNTIME", rt.name());
        cmd.env("SPFS_DEBUG", "1");
        cmd.env("SHELL", &shell_path);
    };

    let tmp_startup_dir = std::fs::create_dir(tmpdir.path().join("startup.d")).unwrap();
    for startup_script in &[&rt.sh_startup_file, &rt.csh_startup_file] {
        let mut cmd = Command::new("sed");
        cmd.arg("-i");
        cmd.arg(format!("s|/spfs/etc/spfs/startup.d|{tmp_startup_dir:?}|"));
        cmd.arg(startup_script);
        setenv(&mut cmd);
        println!("{:?}", cmd.output().unwrap());
    }

    std::env::set_var("SHELL", &shell_path);
    std::env::set_var("SPFS_RUNTIME", &rt.name());

    let mut config = crate::Config::load().unwrap();
    config.storage.runtimes = Some(rt.root().parent().unwrap().to_owned());
    config.make_current().unwrap();

    let args = build_shell_initialized_command(OsString::from("echo"), &mut Vec::new()).unwrap();
    let mut cmd = Command::new(args.get(0).unwrap());
    cmd.args(args[1..].iter());
    setenv(&mut cmd);
    let out = cmd.output().unwrap();
    assert_eq!(out.stdout, "\n".as_bytes());
}

#[rstest(shell, case("bash"), case("tcsh"))]
#[serial_test::serial] // env manipulation must be reliable
fn test_find_alternate_bash(shell: &str, tmpdir: tempdir::TempDir) {
    init_logging();
    let original_path = std::env::var("PATH").unwrap_or_default();
    let original_shell = std::env::var("SHELL").unwrap_or_default();
    std::env::set_var("PATH", tmpdir.path());
    std::env::set_var("SHELL", shell);

    let rt = runtime::Runtime::new(&tmpdir).unwrap();

    let tmp_shell = tmpdir.path().join(shell);
    make_exe(&tmp_shell);
    make_exe(&tmpdir.path().join("expect")); // for tcsh

    let cmd = build_interactive_shell_cmd(&rt).unwrap();
    let expected = tmp_shell.as_os_str().to_os_string();
    assert!(cmd.contains(&expected), "should find shell in PATH");

    std::env::set_var("PATH", original_path);
    std::env::set_var("SHELL", original_shell);
}

fn make_exe(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let file = std::fs::File::create(&path).unwrap();
    drop(file);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
