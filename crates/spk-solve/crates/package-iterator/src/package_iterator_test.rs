// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::Arc;

use rstest::rstest;
use spk_schema::foundation::name::PkgName;
use spk_schema::foundation::option_map;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::{recipe, spec, Package, Spec};
use spk_solve::{make_build, make_repo};

use super::{BuildIterator, PackageIterator, RepositoryPackageIterator, SortedBuildIterator};

#[rstest]
#[tokio::test]
async fn test_solver_sorted_build_iterator_sort_by_option_values() {
    // Test what happens when build have different options - this is
    // for build key generation and sorting coverage

    let package_name = "vnp3";
    let recipe = recipe!(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                "options": [
                    {"var": "tuesday/debug"},
                    {"var": "cmake/3.0"},
                    {"var": "same"},
                    {"var": "zlib"},
                ],
                "variants": [{"python": "2.7", "tuesday": "today", "zlib": "1.0"}],
            },
        }
    );
    let alt_recipe = recipe!(
        {
            "pkg": "vnp3/2.0.0",
            "build": {
                "options": [
                    {"pkg": "gcc/6"},
                    {"var": "cheese/3.0"},
                    {"var": "same"},
                ],
                "variants": [
                    {"gcc": "6.3.1"},
                    {"gcc": "9.3.1"},
                ],
            },
        }
    );
    let python2 = make_build!({"pkg": "python/2.7.7"});
    let gcc6 = make_build!({"pkg": "gcc/6.3.1"});
    let gcc9 = make_build!({"pkg": "gcc/9.3.1"});
    let src_spec = spec!(
        {
            "pkg": "vnp3/2.0.0/src",
            "build": {
                "options": [
                    {"var": "tuesday/debug"},
                    {"var": "cmake/3.0"},
                    {"var": "same"},
                    {"var": "zlib"},
                ],
                "variants": [{"python": "2.7", "tuesday": "today", "zlib": "1.0"}],
            },
        }
    );

    let options1 = option_map! {
        "cmake" => "1.0",
        "tuesday" => "debug",
        "zlib" => "something",
        "same" => "value",
    };
    let options2 = option_map! {
        "cmake" => "1.0",
        "tuesday" => "release",
        "zlib" => "something",
        "same" => "value",
    };
    let options3 = option_map! {
        "cmake" => "apples",
        "tuesday" => "alphabet",
        "zlib" => "2.0",
        "same" => "value",
        "cheese" => "2.0",
    };
    let options_s = OptionMap::default();
    let options_a = option_map! {
        "python" => "2.7",
        "cheese" => "4.0",
        "zlib" => "1.0",
        "same" => "value",
    };
    let options_b = option_map! {
        "cheese" => "4.0",
        "zlib" => "3.0",
        "same" => "value",
    };

    // A package with the first spec
    let build = make_build!(recipe, [python2], options1);
    // A package with the first spec - with tuesday set to a
    // different value
    let build_tuesday = make_build!(recipe, [python2], options2);
    // A package with the first spec - but cmake and zlib set
    // to a different types, and cheese added
    let build_diff_types = make_build!(recipe, [python2], options3);
    // A first spec /src build package - no options that matter
    let src_build = make_build!(src_spec, [], options_s);
    // A package with the second spec - different dependencies,
    // some new options, and some overlapping options
    let alt_build = make_build!(alt_recipe, [gcc6], options_a);
    // A different package with the second spec - higher gcc value
    // that the previous one
    let alt_build_higher = make_build!(alt_recipe, [gcc9], options_b);

    let repo = make_repo!([
        build,
        build_tuesday,
        build_diff_types,
        src_build,
        alt_build,
        alt_build_higher,
    ]);
    repo.publish_recipe(&recipe).await.unwrap();

    // Set up a way of identifying the builds in the expected order.
    // Doing this by options because it's easier to see and update
    // than build digests are. Note these values are a combination of
    // the spec used in the build and the options given to the build.
    let expected_order_by_options: Vec<HashMap<&str, &str>> = vec![
        // Highest gcc value will be first
        HashMap::from([("cheese", "4.0"), ("gcc", "~9.3.1"), ("same", "value")]),
        // Then the next highest gcc value
        HashMap::from([("cheese", "4.0"), ("gcc", "~6.3.1"), ("same", "value")]),
        // Highest tuesday value is release
        HashMap::from([
            ("cmake", "1.0"),
            ("same", "value"),
            ("tuesday", "release"),
            ("zlib", "something"),
        ]),
        // Then the next highest tuesday value is debug
        HashMap::from([
            ("cmake", "1.0"),
            ("same", "value"),
            ("tuesday", "debug"),
            ("zlib", "something"),
        ]),
        // Lowest cmake with a string value will be after all cmake version values
        HashMap::from([
            ("cmake", "apples"),
            ("same", "value"),
            ("tuesday", "alphabet"),
            ("zlib", "2.0"),
        ]),
        // /src builds are last
        HashMap::from([
            ("cmake", "3.0"),
            ("same", ""),
            ("tuesday", "debug"),
            ("zlib", ""),
        ]),
    ];

    let pkg_name = PkgName::new(package_name).unwrap();

    let mut rp_iterator = RepositoryPackageIterator::new(pkg_name.to_owned(), vec![Arc::new(repo)]);
    while let Some((_pkg, builds)) = rp_iterator.next().await.unwrap() {
        // This runs the test, by sorting the builds
        let mut iterator = SortedBuildIterator::new(OptionMap::default(), builds)
            .await
            .unwrap();

        // The rest of this is checking the test results
        let mut sorted_builds: Vec<Arc<Spec>> = Vec::new();
        while let Some(hm) = iterator.next().await.unwrap() {
            for (build, _) in hm.values() {
                sorted_builds.push(Arc::clone(build));
            }
        }

        for i in 0..sorted_builds.len() {
            let b = &sorted_builds[i];
            let options = b.option_values();

            for (n, v) in options.iter() {
                println!("{i} {} {n}={v}", b.ident());
                let expected = &expected_order_by_options[i];
                let expected_v = match expected.get(&(*n)[..]) {
                    Some(value) => {
                        println!("expected: {}={}", n, value);
                        *value
                    }
                    None => "",
                };

                // Is the value what it should be for this option of this build in the order
                assert_eq!(v, expected_v);
            }
        }
    }
}
