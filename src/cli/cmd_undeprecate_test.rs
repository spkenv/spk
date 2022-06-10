// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use itertools::Itertools;
use rstest::rstest;

//use spk::option_map;
//use spk::spec;
use spk::{api::PkgName, make_package, make_repo};
//use spk::make_repo;

use super::{change_deprecation_state, ChangeAction};

#[rstest]
fn test_undeprecate_without_prompt() {
    // Set up a repo with three package versions, with one build each,
    // two of which are deprecated
    let pkg_name = "my-pkg";
    let name1 = "my-pkg/1.0.0";
    let name2 = "my-pkg/1.0.1";
    let name3 = "my-pkg/1.0.2";

    let repo = make_repo!([
        {"pkg": name1, "deprecated": true},
        {"pkg": name2, "deprecated": false},
        {"pkg": name3, "deprecated": true}
    ]);

    let package_name = PkgName::from_str(pkg_name).unwrap();
    let repos = vec![("test".to_string(), repo)];

    // Debugging to check what's in the repo after the setup
    for (n, r) in &repos {
        let ps = r.list_packages().unwrap();
        println!(
            "{n} has pkgs: {}",
            ps.iter().map(ToString::to_string).join(", ")
        );
        let v = r.list_package_versions(&package_name).unwrap();
        println!(
            "repos has: {}",
            v.iter()
                .map(|ver| format!("{}", ver))
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    // Test undeprecating all the package versions and their builds
    // with the '--yes' flag to prevent it prompting.
    let packages = vec![name1.to_string(), name2.to_string(), name3.to_string()];
    let yes = true;
    let result = change_deprecation_state(ChangeAction::Undeprecate, &repos, &packages, yes);

    match result {
        Ok(r) => assert_eq!(r, 0),
        Err(e) => {
            // This should not happen. But it does and it produces this error:
            //
            //   "Spec must be published with no build, got my-pkg/1.0.0/3I42H3S6"
            //
            println!("{}", e);
            std::panic::panic_any(e);
        }
    }

    // None of the packages should be deprecated anymore, although one
    // was already not deprecated (undeprecated) before the test.
    for name in &[name1, name2, name3] {
        let ident = spk::api::parse_ident(name).unwrap();
        let (_, r) = &repos[0];
        let spec = r.read_spec(&ident).unwrap();
        println!("checking: {}", ident);
        assert!(!spec.deprecated);

        for b in r.list_package_builds(&ident).unwrap() {
            let bspec = r.read_spec(&b).unwrap();
            println!("checking: {}", b);
            assert!(!bspec.deprecated);
        }
    }
}
