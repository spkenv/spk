// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::resolve_stack_to_layers;
use crate::{encoding, graph, prelude::*};

use crate::fixtures::*;

#[rstest]
#[tokio::test]
async fn test_stack_to_layers_dedupe(#[future] tmprepo: TempRepo) {
    let repo = tmprepo.await;
    let layer = graph::Layer::new(encoding::EMPTY_DIGEST.into());
    let platform = graph::Platform::new(vec![layer.clone(), layer.clone()].into_iter()).unwrap();
    let stack = vec![layer.digest().unwrap(), platform.digest().unwrap()];
    repo.write_object(&graph::Object::Layer(layer))
        .await
        .unwrap();
    repo.write_object(&graph::Object::Platform(platform))
        .await
        .unwrap();
    let resolved = resolve_stack_to_layers(stack.into_iter(), Some(&repo))
        .await
        .unwrap();
    assert_eq!(resolved.len(), 1, "should deduplicate layers in resolve");
}
