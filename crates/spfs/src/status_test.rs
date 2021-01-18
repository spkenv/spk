use std::process::Command;

use rstest::{fixture, rstest};

use crate::resolve::which;

#[rstest]
#[tokio::test]
async fn test_runtime_file_removal(tmpdir: tempdir::TempDir) {
    if let None = which("spfs") {
        println!("spfs must be installed for this test");
    }

    let script = tmpdir.path().join("script.sh");
    let filename = "/spfs/message.txt";
    let base_tag = "test/file_removal_base";
    let top_tag = "test/file_removal_top";
    script.write(
        vec![
            format!(
                "spfs run - bash -c 'echo hello > {} && spfs commit layer -t {}'",
                filename, base_tag
            ),
            format!(
                "spfs run -e {} -- bash -c 'rm {} && spfs commit platform -t {}'",
                base_tag, filename, top_tag
            ),
            format!("spfs run {} -- test ! -f {}", top_tag, filename),
        ]
        .join("\n"),
    );
    let cmd = Command::new("bash");
    cmd.arg("-ex");
    cmd.arg(script);
    assert_eq!(cmd.status().unwrap(), 0);
}

#[rstest]
#[tokio::test]
async fn test_runtime_dir_removal(tmpdir: tempdir::TempDir) {
    if let None = which("spfs") {
        println!("spfs must be installed for this test");
    }

    let script = tmpdir.path().join("script.sh");
    let dirpath = "/spfs/dir1/dir2/dir3";
    let to_remove = "/spfs/dir1/dir2";
    let to_remain = "/spfs/dir1";
    let base_tag = "test/dir_removal_base";
    let top_tag = "test/dir_removal_top";
    script.write(
        vec![
            format!("spfs run - bash -c 'mkdir -p {} && spfs commit layer -t {}'", dirpath,
            base_tag),
            format!("spfs run -e {} -- bash -c 'rm -r {} && spfs commit platform -t {}'", base_tag,
            to_remove,
            top_tag),
            format!("spfs run {} -- test ! -d {}", top_tag,
            to_remove)
            format!("spfs run {} -- test -d {}", top_tag,
            to_remain)
        ]
        .join("\n"),
    );
    let cmd = Command::new("bash");
    cmd.arg("-ex");
    cmd.arg(script);
    assert_eq!(cmd.status().unwrap(), 0);
}

#[rstest]
#[tokio::test]
async fn test_runtime_recursion() {
    if let None = which("spfs") {
        println!("spfs must be installed for this test");
    }
    let mut cmd = Command::new("spfs");
    cmd.args(&[
        "run",
        "",
        "--",
        "sh",
        "-c",
        "spfs edit --off; spfs run - -- echo hello",
    ]);
    let out = cmd.output().unwrap();
    assert_eq!(out.stdout, "hello\n");
}

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-").expect("failed to create dir for test")
}
