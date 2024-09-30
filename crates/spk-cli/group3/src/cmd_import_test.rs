// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_cli_common::Run;
use spk_schema::foundation::option_map;
use spk_schema::ident_ops::NormalizedTagStrategy;
use spk_schema::{recipe, Package};
use spk_storage::fixtures::*;
use spk_storage::SpfsRepositoryHandle;

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
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    let digest = spec.ident().build();

    let filename = rt.tmpdir.path().join("archive.spk");
    filename.ensure();
    let repo = match &*rt.tmprepo {
        spk_solve::RepositoryHandle::SPFS(repo) => SpfsRepositoryHandle::Normalized(repo),
        spk_solve::RepositoryHandle::SPFSWithVerbatimTags(repo) => {
            SpfsRepositoryHandle::Verbatim(repo)
        }
        spk_solve::RepositoryHandle::Mem(_) | spk_solve::RepositoryHandle::Runtime(_) => {
            panic!("only spfs repositories are supported")
        }
    };
    spk_storage::export_package::<NormalizedTagStrategy>(&[repo], spec.ident().to_any(), &filename)
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
            format!("tags/spk/pkg/spk-archive-test/0.0.1/{digest}"),
            format!("tags/spk/pkg/spk-archive-test/0.0.1/{digest}.tag"),
            format!("tags/spk/pkg/spk-archive-test/0.0.1/{digest}/build.tag"),
            format!("tags/spk/pkg/spk-archive-test/0.0.1/{digest}/run.tag"),
            "tags/spk/spec".to_string(),
            "tags/spk/spec/spk-archive-test".to_string(),
            "tags/spk/spec/spk-archive-test/0.0.1".to_string(),
            "tags/spk/spec/spk-archive-test/0.0.1.tag".to_string(),
            format!("tags/spk/spec/spk-archive-test/0.0.1/{digest}.tag"),
        ]
    );
    let result = super::Import {
        sync: spfs_cli_common::Sync {
            sync: false,
            resync: true,
            check: false,
            max_concurrent_manifests: 10,
            max_concurrent_payloads: 10,
            progress: None,
        },
        files: vec![filename],
    }
    .run()
    .await;
    assert!(matches!(result, Ok(0)), "import should not fail");
}
