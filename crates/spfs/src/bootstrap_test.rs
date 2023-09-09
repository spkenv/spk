// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsString;
use std::process::Command;

use rstest::rstest;

use super::build_shell_initialized_command;
use crate::fixtures::*;
use crate::resolve::which;
use crate::runtime;

#[rstest(
    shell,
    startup_script,
    startup_cmd,
    case("bash", "test.sh", "echo hi; export TEST_VALUE='spfs-test-value'"),
    case("tcsh", "test.csh", "echo hi; setenv TEST_VALUE 'spfs-test-value'")
)]
#[tokio::test]
#[serial_test::serial] // env and config manipulation must be reliable
async fn test_shell_initialization_startup_scripts(
    shell: &str,
    startup_script: &str,
    startup_cmd: &str,
    tmpdir: tempfile::TempDir,
) {
    init_logging();
    let shell_path = match which(shell) {
        Some(path) => path,
        None => {
            println!("{shell} not available on this system");
            return;
        }
    };
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FsRepository::create(&root)
            .await
            .unwrap(),
    );
    let storage = runtime::Storage::new(repo);

    let mut rt = runtime::Storage::create_transient_runtime(&storage)
        .await
        .unwrap();
    rt.set_runtime_dir(tmpdir.path()).await.unwrap();

    let setenv = |cmd: &mut std::process::Command| {
        cmd.env("SPFS_RUNTIME", rt.name());
        cmd.env("SPFS_STORAGE_ROOT", &root);
        cmd.env("SPFS_DEBUG", "1");
        cmd.env("SHELL", &shell_path);
    };

    let tmp_startup_dir = tmpdir.path().join("startup.d");
    std::fs::create_dir(&tmp_startup_dir).unwrap();
    rt.ensure_startup_scripts(None).unwrap();
    for startup_script in &[&rt.config.sh_startup_file, &rt.config.csh_startup_file] {
        let mut cmd = Command::new("sed");
        cmd.arg("-i");
        cmd.arg(format!(
            "s|/spfs/etc/spfs/startup.d|{}|",
            tmp_startup_dir.to_string_lossy()
        ));
        cmd.arg(startup_script);
        setenv(&mut cmd);
        println!("{:?}", cmd.output().unwrap());
    }

    std::fs::write(tmp_startup_dir.join(startup_script), startup_cmd).unwrap();

    std::env::set_var("SHELL", &shell_path);

    match crate::Shell::find_best(None).unwrap() {
        crate::Shell::Bash(_) if shell == "tcsh" => {
            // Test will fail because we weren't able to
            // find the shell we are trying to test
            return;
        }
        crate::Shell::Tcsh(_) if shell == "bash" => {
            // Test will fail because we weren't able to
            // find the shell we are trying to test
            return;
        }
        _ => {}
    }

    let cmd = build_shell_initialized_command(&rt, None, "printenv", vec!["TEST_VALUE"]).unwrap();
    let mut cmd = cmd.into_std();
    setenv(&mut cmd);
    println!("{cmd:?}");
    let out = cmd.output().unwrap();
    println!("{out:?}");
    assert!(out.stdout.ends_with("spfs-test-value\n".as_bytes()));
}

#[rstest(shell, case("bash"), case("tcsh"))]
#[tokio::test]
#[serial_test::serial] // env and config manipulation must be reliable
async fn test_shell_initialization_no_startup_scripts(shell: &str, tmpdir: tempfile::TempDir) {
    init_logging();
    let shell_path = match which(shell) {
        Some(path) => path,
        None => {
            println!("{shell} not available on this system");
            return;
        }
    };
    let root = tmpdir.path().to_string_lossy().to_string();
    let repo = crate::storage::RepositoryHandle::from(
        crate::storage::fs::FsRepository::create(&root)
            .await
            .unwrap(),
    );
    let storage = runtime::Storage::new(repo);

    let mut rt = storage.create_transient_runtime().await.unwrap();
    rt.set_runtime_dir(tmpdir.path()).await.unwrap();

    let setenv = |cmd: &mut std::process::Command| {
        cmd.env("SPFS_STORAGE_ROOT", &root);
        cmd.env("SPFS_RUNTIME", rt.name());
        cmd.env("SPFS_DEBUG", "1");
        cmd.env("SHELL", &shell_path);
    };

    let tmp_startup_dir = tmpdir.path().join("startup.d");
    std::fs::create_dir(&tmp_startup_dir).unwrap();
    rt.ensure_startup_scripts(None).unwrap();
    for startup_script in &[&rt.config.sh_startup_file, &rt.config.csh_startup_file] {
        let mut cmd = Command::new("sed");
        cmd.arg("-i");
        cmd.arg(format!(
            "s|/spfs/etc/spfs/startup.d|{}|",
            tmp_startup_dir.display()
        ));
        cmd.arg(startup_script);
        setenv(&mut cmd);
        println!("{:?}", cmd.output().unwrap());
    }

    std::env::set_var("SHELL", &shell_path);
    let cmd = build_shell_initialized_command(&rt, None, "echo", Option::<OsString>::None).unwrap();
    let mut cmd = cmd.into_std();
    setenv(&mut cmd);
    println!("{cmd:?}");
    let out = cmd.output().unwrap();
    assert_eq!(out.stdout, "\n".as_bytes());
}

#[cfg(unix)]
#[rstest(shell, case("bash"), case("tcsh"))]
#[tokio::test]
#[serial_test::serial] // env manipulation must be reliable
async fn test_find_alternate_bash(shell: &str, tmpdir: tempfile::TempDir) {
    init_logging();
    let original_path = std::env::var("PATH").unwrap_or_default();
    let original_shell = std::env::var("SHELL").unwrap_or_default();
    std::env::set_var("PATH", tmpdir.path());
    std::env::set_var("SHELL", shell);

    let tmp_shell = tmpdir.path().join(shell);
    make_exe(&tmp_shell);

    let found = super::Shell::find_best(None).expect("should find a shell");
    let expected = tmp_shell.as_os_str().to_os_string();
    assert!(found.executable() == expected, "should find shell in PATH");

    std::env::set_var("PATH", original_path);
    std::env::set_var("SHELL", original_shell);
}

#[cfg(unix)]
fn make_exe(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let file = std::fs::File::create(path).unwrap();
    drop(file);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
