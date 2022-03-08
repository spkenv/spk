// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use tokio_stream::StreamExt;

use crate::graph::Manifest;
use crate::{encoding::Encodable, tracking};

use crate::fixtures::*;

#[rstest(
    repo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_read_write_manifest(#[future] repo: TempRepo, tmpdir: tempdir::TempDir) {
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

    let digests: crate::Result<Vec<_>> = repo.iter_digests().collect().await;
    let digests = digests.unwrap();
    assert!(digests.contains(&expected));
}

#[rstest(
    repo,
    case::fs(tmprepo("fs")),
    case::tar(tmprepo("tar")),
    case::rpc(tmprepo("rpc"))
)]
#[tokio::test]
async fn test_manifest_parity(#[future] repo: TempRepo, tmpdir: tempdir::TempDir) {
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
    let actual = out.unlock();
    let mut diffs = tracking::compute_diff(&expected, &actual);
    diffs = diffs
        .into_iter()
        .filter(|d| !d.mode.is_unchanged())
        .collect();

    for diff in diffs.iter() {
        println!("{diff}, {:?}", diff.entries);
    }
    assert!(diffs.is_empty(), "Should read out the way it went in");
}
