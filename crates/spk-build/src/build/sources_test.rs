// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident::ident;
use spk_schema::{v0, GitSource, LocalSource, ScriptSource, SourceSpec, Spec, TarSource};
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
            mode: spfs::tracking::DiffMode::Changed(Default::default(), Default::default()),
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
            mode: spfs::tracking::DiffMode::Added(Default::default()),
        }],
        "/some/dir",
    );
    assert!(res.is_ok());
}

#[rstest]
#[tokio::test]
async fn test_sources_subdir(_tmpdir: tempfile::TempDir) {
    let rt = spfs_runtime().await;

    let tar_file = rt.tmpdir.path().join("archive.tar.gz");
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
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
        git: std::env::current_dir()
            .unwrap()
            // Now that we're in a sub-crate, to find the root of the git
            // project we need to pop two directories.
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_string_lossy()
            .to_string(),
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
    let mut spec = v0::Spec::new("test-pkg".parse().unwrap());
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
    assert!(dest_dir.join("git_repo/crates/spk/src/cli.rs").is_file());
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
    let mut spec = v0::Spec::new(ident!("sources-test/0.1.0/src"));
    let expected = vec![
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
        "should have access to package variables in sources script"
    );
}
