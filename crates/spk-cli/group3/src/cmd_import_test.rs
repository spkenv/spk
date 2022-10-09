use rstest::rstest;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_cli_common::Run;
use spk_schema::{recipe, Package};
use spk_storage::fixtures::*;

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
    spk_storage::export_package(spec.ident(), &filename)
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
    let result = super::Import {
        sync: spfs_cli_common::Sync {
            sync: true,
            resync: true,
            max_concurrent_manifests: 10,
            max_concurrent_payloads: 10,
        },
        files: vec![filename],
    }
    .run()
    .await;
    assert!(matches!(result, Ok(0)), "import should not fail");
}
