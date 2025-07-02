// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema::Deprecate;
use spk_schema::ident::parse_version_ident;
use spk_solve_macros::make_repo;

use super::{ChangeAction, change_deprecation_state};

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
    let result = change_deprecation_state(ChangeAction::Deprecate, &repos, &packages, yes).await;

    match result {
        Ok(r) => assert_eq!(r, 0),
        Err(e) => {
            // This should not happen
            println!("{e}");
            std::panic::panic_any(e);
        }
    }

    // None of the packages should be undeprecated anymore, although
    // one was already deprecated before the test.
    for name in &[name1, name2, name3] {
        let ident = parse_version_ident(name).unwrap();
        let (_, r) = &repos[0];
        let recipe = r.read_recipe(&ident).await.unwrap();
        println!("checking: {ident}");
        assert!(recipe.is_deprecated());

        for b in r.list_package_builds(&ident).await.unwrap() {
            let spec = r.read_package(&b).await.unwrap();
            println!("checking: {b}");
            assert!(spec.is_deprecated());
        }
    }
}
