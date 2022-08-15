// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{change_deprecation_state, ChangeAction};
use spk::make_repo;

#[rstest]
#[tokio::test]
async fn test_deprecate_without_prompt() {
    // Set up a repo with three package versions, with one build each,
    // two of which are undeprecated
    let name1 = "my-pkg/1.0.0";
    let name2 = "my-pkg/1.0.1";
    let name3 = "my-pkg/1.0.2";

    let repo = make_repo!([
        {"pkg": name1, "deprecated": false},
        {"pkg": name2, "deprecated": true},
        {"pkg": name3, "deprecated": false}
    ]);

    let repos = vec![("test".to_string(), repo)];

    // Test deprecating all the package versions and their builds
    // with the '--yes' flag to prevent it prompting.
    let packages = vec![name1.to_string(), name2.to_string(), name3.to_string()];
    let yes = true;
    let comment = Some("test".to_string());
    let result =
        change_deprecation_state(ChangeAction::Deprecate, &repos, &packages, yes, &comment).await;

    match result {
        Ok(r) => assert_eq!(r, 0),
        Err(e) => {
            // This should not happen
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }

    // None of the packages should be undeprecated anymore, although
    // one was already deprecated before the test.
    for name in &[name1, name2, name3] {
        let ident = spk::api::parse_ident(name).unwrap();
        let (_, r) = &repos[0];
        let spec = r.read_spec(&ident).await.unwrap();
        println!("checking: {}", ident);
        assert!(spec.deprecated);

        for b in r.list_package_builds(&ident).await.unwrap() {
            let bspec = r.read_spec(&b).await.unwrap();
            println!("checking: {}", b);
            assert!(bspec.deprecated);
        }
    }
}
