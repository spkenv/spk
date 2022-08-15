// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_spec::recipe;
use spk_spec_ops::PackageOps;
use spk_storage::{export_package, fixtures::*, import_package};

use crate::{BinaryPackageBuilder, BuildSource};

#[rstest]
#[tokio::test]
async fn test_archive_io() {
    let rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "spk-archive-test/0.0.1",
            "build": {"script": "touch /spfs/file.txt"},
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(&*rt.tmprepo)
        .await
        .unwrap();
    let filename = rt.tmpdir.path().join("archive.spk");
    filename.ensure();
    export_package(spec.ident(), &filename)
        .await
        .expect("failed to export");
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
    import_package(&filename)
        .await
        .expect("failed to import package");
}

#[rstest]
#[tokio::test]
async fn test_archive_create_parents() {
    let rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "spk-archive-test/0.0.1",
            "build": {"script": "touch /spfs/file.txt"},
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(&*rt.tmprepo)
        .await
        .unwrap();
    let filename = rt.tmpdir.path().join("deep/nested/path/archive.spk");
    export_package(spec.ident(), filename)
        .await
        .expect("export should create dirs as needed");
}
