// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use tokio_stream::StreamExt;

use crate::encoding::Encodable;
use crate::fixtures::*;
use crate::graph::Manifest;
use crate::tracking;

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_read_write_manifest(
    #[case]
    #[future]
    repo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    let dir = tmpdir.path();
    let repo = repo.await;
    std::fs::File::create(dir.join("file.txt")).unwrap();
    let manifest = Manifest::from(&tracking::compute_manifest(&dir).await.unwrap());
    let expected = manifest.digest().unwrap();
    repo.write_object(&manifest.into())
        .await
        .expect("failed to write manifest");

    std::fs::write(dir.join("file.txt"), "newrootdata").unwrap();
    let manifest2 = Manifest::from(&tracking::compute_manifest(dir).await.unwrap());
    repo.write_object(&manifest2.into()).await.unwrap();

    let digests: crate::Result<Vec<_>> = repo
        .find_digests(crate::graph::DigestSearchCriteria::All)
        .collect()
        .await;
    let digests = digests.unwrap();
    assert!(digests.contains(&expected));
}

#[rstest]
#[case::fs(tmprepo("fs"))]
#[case::tar(tmprepo("tar"))]
#[cfg_attr(feature = "server", case::rpc(tmprepo("rpc")))]
#[tokio::test]
async fn test_manifest_parity(
    #[case]
    #[future]
    repo: TempRepo,
    tmpdir: tempfile::TempDir,
) {
    init_logging();

    let dir = tmpdir.path();
    let repo = repo.await;

    std::fs::create_dir(dir.join("dir")).unwrap();
    std::fs::write(dir.join("dir/file.txt"), "").unwrap();
    let expected = tracking::compute_manifest(&dir).await.unwrap();
    let storable = Manifest::from(&expected);
    let digest = storable.digest().unwrap();
    repo.write_object(&storable.into())
        .await
        .expect("failed to store manifest object");
    let out = repo
        .read_manifest(digest)
        .await
        .expect("stored manifest was not written");
    let actual = out.to_tracking_manifest();
    let mut diffs = tracking::compute_diff(&expected, &actual);
    diffs.retain(|d| !d.mode.is_unchanged());

    for diff in diffs.iter() {
        println!("{diff}, {:#?}", diff.mode);
    }
    assert!(diffs.is_empty(), "Should read out the way it went in");
}
