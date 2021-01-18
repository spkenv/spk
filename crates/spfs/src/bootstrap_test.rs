use rstest::{fixture, rstest};

use super::build_shell_initialized_command;
use crate::{resolve::which, runtime};
use std::process::Command;

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-").expect("failed to create dir for test")
}

#[rstest(
    shell,
    startup_cmd,
    case("bash", "export TEST_VALUE='spfs-test-value'"),
    case("tcsh", "setenv TEST_VALUE 'spfs-test-value'")
)]
#[tokio::test]
async fn test_shell_initialization_startup_scripts(
    shell: &str,
    startup_cmd: &str,
    tmpdir: tempdir::TempDir,
) {
    let shell_path = match which(shell) {
        Some(path) => path,
        None => {
            println!("{} not available on this system", shell);
            return;
        }
    };

    let storage = runtime::Storage::new(tmpdir.path()).unwrap();
    let rt = storage.create_runtime().unwrap();

    std::env::set_var("SPFS_RUNTIME", rt.root);
    std::env::set_var("SHELL", shell_path);

    let tmp_startup_dir = std::fs::create_dir(tmpdir.path().join("startup.d")).unwrap();
    for startup_script in &[rt.sh_startup_file, rt.csh_startup_file] {
        let mut cmd = Command::new("sed");
        cmd.arg("-i");
        cmd.arg(format!(
            "s|/spfs/etc/spfs/startup.d|{}|",
            tmp_startup_dir.strpath
        ));
        cmd.arg(startup_script);
        println!("{}", cmd.output().unwrap());
    }

    std::fs::write(tmp_startup_dir.join("test.csh"), startup_cmd).unwrap();
    std::fs::write(tmp_startup_dir.join("test.sh"), startup_cmd).unwrap();

    let args = build_shell_initialized_command("printenv", vec!["TEST_VALUE"]).unwrap();
    let mut cmd = Command::new(args[0]);
    cmd.args(args[1..]);
    let out = cmd.output().unwrap();
    assert!(out.stout.endswith("spfs-test-value\n"));
}

#[rstest(shell, case("bash"), case("tcsh"))]
#[tokio::test]
async fn test_shell_initialization_no_startup_scripts(shell: &str, tmpdir: tempdir::TempDir) {
    let shell_path = match which(shell) {
        Some(path) => path,
        None => {
            println!("{} not available on this system", shell);
            return;
        }
    };

    let storage = runtime::Storage::new(tmpdir.path()).unwrap();
    let rt = storage.create_runtime().unwrap();

    std::env::set_var("SPFS_RUNTIME", rt.root);
    std::env::set_var("SHELL", shell_path);

    let tmp_startup_dir = std::fs::create_dir(tmpdir.path().join("startup.d")).unwrap();
    for startup_script in &[rt.sh_startup_file, rt.csh_startup_file] {
        let mut cmd = Command::new("sed");
        cmd.arg("-i");
        cmd.arg(format!(
            "s|/spfs/etc/spfs/startup.d|{}|",
            tmp_startup_dir.strpath
        ));
        cmd.arg(startup_script);
        println!("{}", cmd.output().unwrap());
    }

    let args = build_shell_initialized_command("echo").unwrap();
    let mut cmd = Command::new(args[0]);
    cmd.args(args[1..]);
    let out = cmd.output().unwrap();
    assert_eq!(out.stdout, "\n");
}
