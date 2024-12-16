// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
use crate::foundation::fixtures::*;
use crate::LintedItem;

#[rstest]
fn test_local_source_dir(tmpdir: tempfile::TempDir) {
    init_logging();
    let source_dir = tmpdir.path().join("source");
    let dest_dir = tmpdir.path().join("dest");
    {
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::File::create(source_dir.join("file.txt")).unwrap();
    }
    let spec = format!("{{path: {source_dir:?}}}");
    let source: LocalSource = serde_yaml::from_str(&spec).unwrap();
    source.collect(&dest_dir).unwrap();

    assert!(dest_dir.join("file.txt").exists());
}

#[rstest]
fn test_local_source_file(tmpdir: tempfile::TempDir) {
    init_logging();
    let source_dir = tmpdir.path().join("source");
    let dest_dir = tmpdir.path().join("dest");
    {
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::File::create(source_dir.join("file.txt")).unwrap();
    }
    let spec = format!("{{path: {:?}}}", source_dir.join("file.txt"));
    let source: LocalSource = serde_yaml::from_str(&spec).unwrap();
    source.collect(&dest_dir).unwrap();

    assert!(dest_dir.join("file.txt").exists());
}

#[rstest]
fn test_git_sources(tmpdir: tempfile::TempDir) {
    init_logging();
    let source_dir = tmpdir.path().join("source");
    let dest_dir = tmpdir.path().join("dest");
    {
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::File::create(source_dir.join("file.txt")).unwrap();
    }
    let spec = format!(
        "{{git: {:?}}}",
        std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
    );
    let source: GitSource = serde_yaml::from_str(&spec).unwrap();
    source.collect(&dest_dir).unwrap();

    assert!(dest_dir.join(".git").is_dir());
}

#[rstest]
fn test_tar_sources(tmpdir: tempfile::TempDir) {
    init_logging();
    let filename = tmpdir.path().join("archive.tar.gz");
    let mut tar_cmd = std::process::Command::new("tar");
    tar_cmd.arg("acf");
    tar_cmd.arg(&filename);
    tar_cmd.arg("src/lib.rs");
    tar_cmd.status().unwrap();

    let spec = format!("{{tar: {:?}}}", &filename);
    let source: TarSource = serde_yaml::from_str(&spec).unwrap();
    source.collect(tmpdir.path()).unwrap();

    assert!(tmpdir.path().join("src/lib.rs").is_file());
}

#[rstest]
fn test_script_sources(tmpdir: tempfile::TempDir) {
    init_logging();
    let spec = "{script: ['mkdir spk', 'touch spk/__init__.py']}".to_string();
    let source: ScriptSource = serde_yaml::from_str(&spec).unwrap();
    source.collect(tmpdir.path(), &Default::default()).unwrap();

    assert!(tmpdir.path().join("spk/__init__.py").exists());
}

#[rstest]
fn test_path_sources_lints(tmpdir: tempfile::TempDir) {
    init_logging();
    let source_dir = tmpdir.path().join("source");
    let dest_dir = tmpdir.path().join("dest");
    {
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(dest_dir).unwrap();
        std::fs::File::create(source_dir.join("file.txt")).unwrap();
    }
    let spec = format!("{{paths: {source_dir:?}}}");
    let source: LintedItem<SourceSpec> = serde_yaml::from_str(&spec).unwrap();

    assert_eq!(source.lints.len(), 1);
    for lint in source.lints.iter() {
        assert_eq!(lint.get_key(), "sources.paths");
    }
}

#[rstest]
fn test_git_sources_lints(tmpdir: tempfile::TempDir) {
    init_logging();
    let source_dir = tmpdir.path().join("source");
    let dest_dir = tmpdir.path().join("dest");
    {
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(dest_dir).unwrap();
        std::fs::File::create(source_dir.join("file.txt")).unwrap();
    }
    let spec = format!(
        "{{gitt: {:?}}}",
        std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
    );

    let source: LintedItem<SourceSpec> = serde_yaml::from_str(&spec).unwrap();

    assert_eq!(source.lints.len(), 1);
    for lint in source.lints.iter() {
        assert_eq!(lint.get_key(), "sources.gitt");
    }
}

#[rstest]
fn test_script_sources_lints() {
    init_logging();
    let spec = "{scripts: ['mkdir spk', 'touch spk/__init__.py']}".to_string();
    let source: LintedItem<SourceSpec> = serde_yaml::from_str(&spec).unwrap();

    assert_eq!(source.lints.len(), 1);
    for lint in source.lints.iter() {
        assert_eq!(lint.get_key(), "sources.scripts");
    }
}
