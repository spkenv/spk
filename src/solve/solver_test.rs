// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::{RequestEnum, Solver};
use crate::api;

#[rstest]
fn test_request_default_component() {
    let mut solver = Solver::new();
    solver
        .py_add_request(RequestEnum::String("python/3.7.3".into()))
        .unwrap();
    let state = solver.get_initial_state();
    let request = state
        .pkg_requests
        .get(0)
        .expect("solver should have a reqiest");
    assert_eq!(
        request.pkg.components,
        vec![api::Component::Run].into_iter().collect(),
        "solver should inject a default run component if not otherwise given"
    )
}
