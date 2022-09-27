// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema::foundation::spec_ops::PackageOps;
use spk_schema::recipe;
use spk_storage::{export_package, fixtures::*};

use crate::{BinaryPackageBuilder, BuildSource};

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
