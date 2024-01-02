// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{DigestStrategy, EncodingFormat, HeaderBuilder};
use crate::fixtures::*;
use crate::graph::{ObjectKind, Platform};
use crate::prelude::*;

#[rstest]
fn test_digest_with_salting() {
    // the digest based on legacy encoding for a platform could easily
    // collide with eight null bytes.
    let legacy_platform = Platform::builder()
        .with_header(|h| h.with_digest_strategy(DigestStrategy::Legacy))
        .build()
        .digest()
        .unwrap();
    let nulls_digest = [0, 0, 0, 0, 0, 0, 0, 0].as_slice().digest().unwrap();
    assert_eq!(legacy_platform, nulls_digest);

    // the newer digest method adds the kind and salt to make
    // such cases less likely
    let salted_platform = Platform::builder()
        .with_header(|h| h.with_digest_strategy(DigestStrategy::WithKindAndSalt))
        .build()
        .digest()
        .unwrap();
    assert_ne!(salted_platform, nulls_digest);
}

#[rstest]
fn test_digest_with_encoding() {
    // check that two objects with the same digest strategy
    // can be saved with two different encoding methods and
    // still yield the same result
    let legacy_platform = Platform::builder()
        .with_header(|h| h.with_encoding_format(EncodingFormat::Legacy))
        .build()
        .digest()
        .unwrap();
    let flatbuf_platform = Platform::builder()
        .with_header(|h| h.with_encoding_format(EncodingFormat::FlatBuffers))
        .build()
        .digest()
        .unwrap();
    assert_eq!(legacy_platform, flatbuf_platform);
}

#[rstest]
#[case::legacy(EncodingFormat::Legacy)]
#[case::flatbuf(EncodingFormat::FlatBuffers)]
#[tokio::test]
async fn test_encoding_round_trip(
    #[case] encoding_format: EncodingFormat,
    #[future] tmprepo: TempRepo,
) {
    // check that each encoding format can save and load back
    // the same object data

    init_logging();
    let tmprepo = tmprepo.await;

    let mut manifest = generate_tree(&tmprepo).await;
    manifest.set_header(
        HeaderBuilder::new(ObjectKind::Manifest)
            .with_encoding_format(encoding_format)
            .build(),
    );
    // generate tree stores the object using the current configured
    // digest and encoding format, so we will store it again in the
    // format that is being tested
    let storable = manifest.to_graph_manifest();
    let digest = storable.digest().unwrap();
    tmprepo.remove_object(digest).await.unwrap();
    tmprepo.write_object(&storable).await.unwrap();

    let loaded = tmprepo.read_manifest(digest).await.unwrap();
    assert_eq!(
        loaded.header().encoding_format().unwrap(),
        encoding_format,
        "should retain config encoding format"
    );
    let result = loaded.to_tracking_manifest();
    let mut diffs = crate::tracking::compute_diff(&manifest, &result);
    diffs.retain(|d| !d.mode.is_unchanged());
    tracing::info!("Diffs:");
    for diff in diffs.iter() {
        tracing::info!("  {diff}");
    }
    assert!(
        diffs.is_empty(),
        "should generate, save and reload manifest with no changes to content"
    );

    let second = result.to_graph_manifest();
    assert_eq!(
        second.digest().unwrap(),
        digest,
        "save, load and convert should have no effect on digest"
    );
}
