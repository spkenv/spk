// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::DecisionBuilder;
use crate::{api, solve};

#[rstest]
fn test_request_default_component() {
    let spec: api::Spec = serde_yaml::from_str(
        r#"{
        pkg: parent,
        install: {
          requirements: [
            {pkg: dependency/1.0.0}
          ]
        }
    }"#,
    )
    .unwrap();
    let spec = std::sync::Arc::new(spec);
    let base = std::sync::Arc::new(super::State::default());

    let resolve_state = DecisionBuilder::new(spec.clone(), &base)
        .resolve_package(solve::solution::PackageSource::Spec(spec.clone()))
        .apply(&base);
    let request = resolve_state.get_merged_request("dependency").unwrap();
    assert!(
        request.pkg.components.contains(&api::Component::Run),
        "default run component should be injected when none specified"
    );

    let build_state = DecisionBuilder::new(spec, &base)
        .build_package(&solve::solution::Solution::new(None))
        .unwrap()
        .apply(&base);
    let request = build_state.get_merged_request("dependency").unwrap();
    assert!(
        request.pkg.components.contains(&api::Component::Run),
        "default run component should be injected when none specified"
    );
}
