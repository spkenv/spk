// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::{export_package, import_package};
use crate::{
    build::{self, BuildVariant},
    fixtures::*,
};

#[rstest]
fn test_archive_io() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "spk-archive-test/0.0.1",
            "build": {"script": "touch /spfs/file.txt"},
        }
    );
    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    let spec = build::BinaryPackageBuilder::from_spec(spec, BuildVariant::Default)
        .with_source(build::BuildSource::LocalPath(".".into()))
        .build()
        .unwrap();
    let filename = rt.tmpdir.path().join("achive.spk");
    filename.ensure();
    export_package(&spec.pkg, &filename).expect("failed to export");
    let mut actual = Vec::new();
    let mut tarfile = tar::Archive::new(std::fs::File::open(&filename).unwrap());
    for entry in tarfile.entries().unwrap() {
        let filename = entry.unwrap().path().unwrap().to_string_lossy().to_string();
        if filename.contains('/') && !filename.contains("tags") {
            // ignore specific object data for this test
            continue;
        }
        actual.push(filename);
    }
    actual.sort();
    assert_eq!(
        actual,
        vec![
            "VERSION".to_string(),
            "objects".to_string(),
            "payloads".to_string(),
            "renders".to_string(),
            "tags".to_string(),
            "tags/spk".to_string(),
            "tags/spk/pkg".to_string(),
            "tags/spk/pkg/spk-archive-test".to_string(),
            "tags/spk/pkg/spk-archive-test/0.0.1".to_string(),
            "tags/spk/pkg/spk-archive-test/0.0.1/3I42H3S6".to_string(),
            "tags/spk/pkg/spk-archive-test/0.0.1/3I42H3S6.tag".to_string(),
            "tags/spk/pkg/spk-archive-test/0.0.1/3I42H3S6/build.tag".to_string(),
            "tags/spk/pkg/spk-archive-test/0.0.1/3I42H3S6/run.tag".to_string(),
            "tags/spk/spec".to_string(),
            "tags/spk/spec/spk-archive-test".to_string(),
            "tags/spk/spec/spk-archive-test/0.0.1".to_string(),
            "tags/spk/spec/spk-archive-test/0.0.1.tag".to_string(),
            "tags/spk/spec/spk-archive-test/0.0.1/3I42H3S6.tag".to_string(),
        ]
    );
    crate::HANDLE
        .block_on(import_package(&filename))
        .expect("failed to import package");
}

#[rstest]
fn test_archive_create_parents() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "spk-archive-test/0.0.1",
            "build": {"script": "touch /spfs/file.txt"},
        }
    );
    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    let spec = build::BinaryPackageBuilder::from_spec(spec, BuildVariant::Default)
        .with_source(build::BuildSource::LocalPath(".".into()))
        .build()
        .unwrap();
    let filename = rt.tmpdir.path().join("deep/nested/path/archive.spk");
    export_package(&spec.pkg, filename).expect("export should create dirs as needed");
}
