// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema::foundation::option_map;
use spk_schema::ident_ops::NormalizedTagStrategy;
use spk_schema::{Package, recipe};
use spk_storage::fixtures::*;
use spk_storage::{SpfsRepositoryHandle, export_package};

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
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    let filename = rt.tmpdir.path().join("deep/nested/path/archive.spk");
    let repo = match &*rt.tmprepo {
        spk_solve::RepositoryHandle::SPFS(repo) => SpfsRepositoryHandle::Normalized(repo),
        spk_solve::RepositoryHandle::SPFSWithVerbatimTags(repo) => {
            SpfsRepositoryHandle::Verbatim(repo)
        }
        spk_solve::RepositoryHandle::Mem(_) | spk_solve::RepositoryHandle::Runtime(_) => {
            panic!("only spfs repositories are supported")
        }
    };
    export_package::<NormalizedTagStrategy>(&[repo], &spec.ident().to_any_ident(), filename)
        .await
        .expect("export should create dirs as needed");
}
