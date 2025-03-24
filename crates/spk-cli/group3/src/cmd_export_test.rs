// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spfs::prelude::*;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_schema::foundation::option_map;
use spk_schema::{Package, recipe};
use spk_storage::SpfsRepositoryHandle;
use spk_storage::fixtures::*;

#[rstest]
#[tokio::test]
async fn test_export_works_with_missing_builds() {
    let rt = spfs_runtime().await;

    let spec = recipe!(
        {
            "pkg": "spk-export-test/0.0.1",
            "build": {
                "options": [
                    {"var": "color"},
                ],
                "script": "touch /spfs/file.txt",
            },
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (blue_spec, _) = BinaryPackageBuilder::from_recipe(spec.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {"color" => "blue"}, &*rt.tmprepo)
        .await
        .unwrap();
    let (red_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {"color" => "red"}, &*rt.tmprepo)
        .await
        .unwrap();

    // Now that these two builds are created, remove the `spk/pkg` tags for one
    // of them. The publish is still expected to succeed; it should publish
    // the remaining valid build.
    let repo = match &*rt.tmprepo {
        spk_storage::RepositoryHandle::SPFS(spfs) => {
            for spec in [
                format!("{}", blue_spec.ident().build()),
                format!("{}/build", blue_spec.ident().build()),
                format!("{}/run", blue_spec.ident().build()),
            ] {
                let tag = spfs::tracking::TagSpec::parse(format!(
                    "spk/pkg/spk-export-test/0.0.1/{spec}",
                ))
                .unwrap();
                spfs.remove_tag_stream(&tag).await.unwrap();
            }
            SpfsRepositoryHandle::Normalized(spfs)
        }
        _ => panic!("only implemented for spfs repos"),
    };

    let filename = rt.tmpdir.path().join("archive.spk");
    filename.ensure();
    spk_storage::export_package(
        &[repo],
        red_spec
            .ident()
            .clone()
            .to_version_ident()
            .to_any_ident(None),
        &filename,
    )
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
            "tags/spk/pkg/spk-export-test".to_string(),
            "tags/spk/pkg/spk-export-test/0.0.1".to_string(),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}",
                red_spec.ident().build()
            ),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}.tag",
                red_spec.ident().build()
            ),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}/build.tag",
                red_spec.ident().build()
            ),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}/run.tag",
                red_spec.ident().build()
            ),
            "tags/spk/spec".to_string(),
            "tags/spk/spec/spk-export-test".to_string(),
            "tags/spk/spec/spk-export-test/0.0.1".to_string(),
            "tags/spk/spec/spk-export-test/0.0.1.tag".to_string(),
            format!(
                "tags/spk/spec/spk-export-test/0.0.1/{}.tag",
                red_spec.ident().build()
            ),
        ]
    );
}
