// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use super::{MissingProviderContext, missing_file_provider_hint};

#[test]
fn suggests_origin_local_fallback_for_split_runtime_objects() {
    let hint = missing_file_provider_hint(MissingProviderContext {
        in_a_runtime: true,
        skipped_unknown_objects: true,
    })
    .expect("expected hint text");
    assert!(hint.contains("--origin-local-fallback"));
}

#[test]
fn omits_fallback_hint_when_lookup_cannot_be_influenced_by_repo_selection() {
    assert!(
        missing_file_provider_hint(MissingProviderContext {
            in_a_runtime: false,
            skipped_unknown_objects: true
        })
        .is_none()
    );
    assert!(
        missing_file_provider_hint(MissingProviderContext {
            in_a_runtime: true,
            skipped_unknown_objects: false
        })
        .is_none()
    );
}
