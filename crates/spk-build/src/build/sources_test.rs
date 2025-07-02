// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io::Write;

use rstest::rstest;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident::build_ident;
use spk_schema::{GitSource, LocalSource, ScriptSource, SourceSpec, Spec, TarSource, v0};
use spk_storage::fixtures::*;

use super::{collect_sources, validate_source_changeset};

#[rstest]
fn test_validate_sources_changeset_nothing() {
    let res = validate_source_changeset(vec![], "/spfs");
    assert!(res.is_err());
}

#[rstest]
fn test_validate_sources_changeset_not_in_dir() {
    let res = validate_source_changeset(
        vec![spfs::tracking::Diff {
            path: "/file.txt".into(),
            mode: spfs::tracking::DiffMode::Changed(
                spfs::tracking::Entry::empty_file_with_open_perms(),
                spfs::tracking::Entry::empty_file_with_open_perms(),
            ),
        }],
        "/some/dir",
    );
    assert!(res.is_err());
}

#[rstest]
fn test_validate_sources_changeset_ok() {
    let res = validate_source_changeset(
        vec![spfs::tracking::Diff {
            path: "/some/dir/file.txt".into(),
            mode: spfs::tracking::DiffMode::Added(
                spfs::tracking::Entry::empty_file_with_open_perms(),
            ),
        }],
        "/some/dir",
    );
    assert!(res.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_sources_subdir(tmpdir: tempfile::TempDir) {
    let rt = spfs_runtime().await;

    // Create a small git working copy at tmpdir so this test does not depend on
    // this working copy using git.
    {
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&tmpdir)
            .output()
            .unwrap();
        tmpdir.path().join("file_a.txt").ensure();
        tmpdir.path().join("file_b.txt").ensure();
        let output = std::process::Command::new("git")
            .args(["add", "file_a.txt", "file_b.txt"])
            .current_dir(&tmpdir)
            .output()
            .unwrap();
        std::io::stderr().write_all(&output.stderr).unwrap();
        assert!(output.status.success());
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test User",
                "-c",
                "user.email=<testuser@invalid.invalid>",
                "commit",
                "--author",
                "Test User <testuser@invalid.invalid>",
                "-m",
                "test commit",
            ])
            .current_dir(&tmpdir)
            .output()
            .unwrap();
        std::io::stderr().write_all(&output.stderr).unwrap();
        assert!(output.status.success());
    }

    let tar_file = rt.tmpdir.path().join("archive.tar.gz");
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tar_file)
        .unwrap();
    let mut builder = tar::Builder::new(writer);
    builder.append_path("src/lib.rs").unwrap();
    builder.finish().unwrap();

    let tar_source = TarSource {
        tar: tar_file.to_string_lossy().to_string(),
        // purposefully add leading slash to make sure it doesn't fail
        subdir: Some("/archive/src".to_string()),
    };
    let git_source = GitSource {
        git: tmpdir.path().to_string_lossy().to_string(),
        subdir: Some("git_repo".to_string()),
        depth: 1,
        reference: String::new(),
    };
    let source_dir = rt.tmpdir.path().join("source");
    source_dir.join("file.txt").ensure();
    source_dir.join(".git/gitfile").ensure();
    let dir_source = LocalSource::new(source_dir).set_subdir("local");
    let source_file = rt.tmpdir.path().join("src").join("source_file.txt");
    source_file.ensure();
    let file_source = LocalSource::new(source_file).set_subdir("local");

    let dest_dir = rt.tmpdir.path().join("dest");
    let mut spec = v0::Spec::new("test-pkg/1.0.0/src".parse().unwrap());
    spec.sources = vec![
        SourceSpec::Git(git_source),
        SourceSpec::Tar(tar_source),
        SourceSpec::Local(file_source),
        SourceSpec::Local(dir_source),
    ];
    collect_sources(&Spec::from(spec), &dest_dir).unwrap();
    assert!(dest_dir.join("local").is_dir());
    assert!(dest_dir.join("git_repo").is_dir());
    assert!(dest_dir.join("archive/src").is_dir());
    assert!(dest_dir.join("archive/src/src/lib.rs").is_file());
    assert!(dest_dir.join("git_repo/file_a.txt").is_file());
    assert!(dest_dir.join("git_repo/file_b.txt").is_file());
    assert!(
        !dest_dir.join("local/.git").exists(),
        "should exclude git repo"
    );
    assert!(dest_dir.join("local/file.txt").is_file());
    assert!(dest_dir.join("local/source_file.txt").is_file());
}

#[rstest]
#[tokio::test]
async fn test_sources_environment(_tmpdir: tempfile::TempDir) {
    let rt = spfs_runtime().await;
    let mut spec = v0::Spec::new(build_ident!("sources-test/0.1.0/src"));
    let expected = [
        "SPK_PKG=sources-test/0.1.0/src",
        "SPK_PKG_NAME=sources-test",
        "SPK_PKG_VERSION=0.1.0",
        "SPK_PKG_BUILD=src",
        "SPK_PKG_VERSION_MAJOR=0",
        "SPK_PKG_VERSION_MINOR=1",
        "SPK_PKG_VERSION_PATCH=0",
        "SPK_PKG_VERSION_BASE=0.1.0",
        "",
    ]
    .join("\n");

    let out_file = rt.tmpdir.path().join("out.log");
    out_file.ensure();
    let script_source = ScriptSource::new([
        format!("echo SPK_PKG=${{SPK_PKG}} >> {out_file:?}"),
        format!("echo SPK_PKG_NAME=${{SPK_PKG_NAME}} >> {out_file:?}"),
        format!("echo SPK_PKG_VERSION=${{SPK_PKG_VERSION}} >> {out_file:?}"),
        format!("echo SPK_PKG_BUILD=${{SPK_PKG_BUILD}} >> {out_file:?}"),
        format!("echo SPK_PKG_VERSION_MAJOR=${{SPK_PKG_VERSION_MAJOR}} >> {out_file:?}"),
        format!("echo SPK_PKG_VERSION_MINOR=${{SPK_PKG_VERSION_MINOR}} >> {out_file:?}"),
        format!("echo SPK_PKG_VERSION_PATCH=${{SPK_PKG_VERSION_PATCH}} >> {out_file:?}"),
        format!("echo SPK_PKG_VERSION_BASE=${{SPK_PKG_VERSION_BASE}} >> {out_file:?}"),
    ]);
    let dest_dir = rt.tmpdir.path().join("dest");
    spec.sources = vec![SourceSpec::Script(script_source)];
    collect_sources(&Spec::from(spec), dest_dir).unwrap();

    let actual = std::fs::read_to_string(out_file).unwrap();
    assert_eq!(
        actual, expected,
        "should have access to package variables in sources script, want: {expected}, got: {actual}"
    );
}
