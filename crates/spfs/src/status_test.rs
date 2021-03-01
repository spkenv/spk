use std::io::Write;
use std::process::Command;

use rstest::rstest;

fixtures!();

#[rstest]
#[tokio::test]
async fn test_runtime_file_removal(tmpdir: tempdir::TempDir, spfs_binary: std::path::PathBuf) {
    let script = tmpdir.path().join("script.sh");
    let filename = "/spfs/message.txt";
    let base_tag = "test/file_removal_base";
    let top_tag = "test/file_removal_top";
    let lines: Vec<String> = vec![
        format!(
            "{:?} run - -- bash -c 'echo hello > {} && {:?} commit layer -t {}'",
            &spfs_binary, filename, &spfs_binary, base_tag
        ),
        format!(
            "{:?} run -e {} -- bash -c 'rm {} && {:?} commit platform -t {}'",
            &spfs_binary, base_tag, filename, &spfs_binary, top_tag
        ),
        format!("{:?} run {} -- tree /spfs", &spfs_binary, top_tag),
        format!(
            "{:?} run {} -- test ! -f {}",
            &spfs_binary, top_tag, filename
        ),
    ];
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&script)
        .unwrap()
        .write_all(lines.join("\n").as_bytes())
        .unwrap();
    let mut cmd = Command::new("bash");
    cmd.arg("-ex");
    cmd.arg(script);
    assert_eq!(cmd.status().unwrap().code(), Some(0));
}

#[rstest]
#[tokio::test]
async fn test_runtime_dir_removal(tmpdir: tempdir::TempDir, spfs_binary: std::path::PathBuf) {
    let script = tmpdir.path().join("script.sh");
    let dirpath = "/spfs/dir1/dir2/dir3";
    let to_remove = "/spfs/dir1/dir2";
    let to_remain = "/spfs/dir1";
    let base_tag = "test/dir_removal_base";
    let top_tag = "test/dir_removal_top";
    let lines: Vec<String> = vec![
        format!(
            "{:?} run - -- bash -c 'mkdir -p {} && {:?} commit layer -t {}'",
            &spfs_binary, dirpath, &spfs_binary, base_tag
        ),
        format!(
            "{:?} run -e {} -- bash -c 'rm -r {} && {:?} commit platform -t {}'",
            &spfs_binary, base_tag, to_remove, &spfs_binary, top_tag
        ),
        format!("{:?} run {} -- tree /spfs", &spfs_binary, top_tag),
        format!(
            "{:?} run {} -- test ! -d {}",
            &spfs_binary, top_tag, to_remove
        ),
        format!(
            "{:?} run {} -- test -d {}",
            &spfs_binary, top_tag, to_remain
        ),
    ];
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&script)
        .unwrap()
        .write_all(lines.join("\n").as_bytes())
        .unwrap();
    let mut cmd = Command::new("bash");
    cmd.arg("-ex");
    cmd.arg(script);
    cmd.env("SHELL", "bash");
    assert_eq!(cmd.status().unwrap().code(), Some(0));
}

#[rstest]
#[tokio::test]
async fn test_runtime_recursion(spfs_binary: std::path::PathBuf) {
    let mut cmd = Command::new(&spfs_binary);
    cmd.args(&["run", "", "--", "sh", "-c"]);
    cmd.arg(format!(
        "{:?} edit --off; {:?} run - -- echo hello",
        &spfs_binary, &spfs_binary
    ));
    let out = cmd.output().unwrap();
    assert!(String::from_utf8_lossy(out.stdout.as_slice()).ends_with("hello\n"));
}
