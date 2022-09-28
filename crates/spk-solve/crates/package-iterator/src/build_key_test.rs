// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use itertools::Itertools;
use rstest::rstest;
use spk_schema::foundation::opt_name;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::spec_ops::PackageOps;
use spk_schema::spec;

use super::{
    BuildKey,
    BuildKeyEntry,
    BuildKeyExpandedVersionRange,
    BuildKeyVersionNumber,
    BuildKeyVersionNumberPiece,
};

// For making test case results
fn make_tag_part(pieces: Vec<&str>) -> Option<Vec<BuildKeyVersionNumberPiece>> {
    if pieces.is_empty() {
        None
    } else {
        let mut tmp: Vec<BuildKeyVersionNumberPiece> = vec![];
        for value in pieces {
            match value.parse::<u32>() {
                Ok(number) => tmp.push(BuildKeyVersionNumberPiece::Number(number)),
                Err(_) => tmp.push(BuildKeyVersionNumberPiece::Text(value.to_string())),
            }
        }
        Some(tmp)
    }
}

// For making test case results
#[allow(clippy::too_many_arguments)]
fn make_expanded_version_range_part(
    version: &str,
    max_digits: Vec<u32>,
    max_plus_epsilon: bool,
    max_post: Vec<&str>,
    max_pre: Vec<&str>,
    max_notags: bool,
    min_digits: Vec<u32>,
    min_plus_epsilon: bool,
    min_post: Vec<&str>,
    min_pre: Vec<&str>,
    min_notags: bool,
) -> BuildKeyExpandedVersionRange {
    let post_max = make_tag_part(max_post);
    let pre_max = make_tag_part(max_pre);
    let post_min = make_tag_part(min_post);
    let pre_min = make_tag_part(min_pre);

    BuildKeyExpandedVersionRange {
        max: BuildKeyVersionNumber {
            digits: max_digits
                .iter()
                .map(|n| BuildKeyVersionNumberPiece::Number(*n))
                .collect::<Vec<BuildKeyVersionNumberPiece>>(),
            plus_epsilon: max_plus_epsilon,
            posttag: post_max,
            notags: max_notags,
            pretag: pre_max,
        },
        min: BuildKeyVersionNumber {
            digits: min_digits
                .iter()
                .map(|n| BuildKeyVersionNumberPiece::Number(*n))
                .collect::<Vec<BuildKeyVersionNumberPiece>>(),
            plus_epsilon: min_plus_epsilon,
            posttag: post_min,
            notags: min_notags,
            pretag: pre_min,
        },
        tie_breaker: BuildKeyExpandedVersionRange::generate_tie_breaker(version),
    }
}

#[rustfmt::skip]
#[rstest]
#[case("1.2.3",              make_expanded_version_range_part("1.2.3",              vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec![], true))]
#[case("1.2.3.4",            make_expanded_version_range_part("1.2.3.4",            vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], true, vec![1, 2, 3, 4], false, vec![],          vec![], true))]
#[case("1.2.3-r.1",          make_expanded_version_range_part("1.2.3-r.1",          vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec!["r", "1"], false))]
#[case("1.2.3+r.2",          make_expanded_version_range_part("1.2.3+r.2",          vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], true, vec![1, 2, 3],    false, vec!["r", "2"],  vec![], false))]
#[case("1.2.3-a.4+r.6",      make_expanded_version_range_part("1.2.3-a.4+r.6",      vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], true, vec![1, 2, 3],    false, vec!["r", "6"],  vec!["a", "4"], false))]
#[case("~1.2.3",             make_expanded_version_range_part("~1.2.3",             vec![1, 3],                         false, vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec![], true))]
#[case("~1.2.3.1",           make_expanded_version_range_part("~1.2.3.1",           vec![1, 2, 4],                      false, vec![], vec![], true, vec![1, 2, 3, 1], false, vec![],          vec![], true))]
#[case(">=1.2.3",            make_expanded_version_range_part(">=1.2.3",            vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec![], true))]
#[case(">=1.2.3,<1.2.5",     make_expanded_version_range_part(">=1.2.3,<1.2.5",     vec![1, 2, 5],                      false, vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec![], true))]
#[case(">=1.2.3+r.2,<1.2.5", make_expanded_version_range_part(">=1.2.3+r.2,<1.2.5", vec![1, 2, 5],                      false, vec![], vec![], true, vec![1, 2, 3],    false, vec!["r", "2"],  vec![], false))]
#[case(">=2.3.4.1,<2.3.5",   make_expanded_version_range_part(">=2.3.4.1,<2.3.5",   vec![2, 3, 5],                      false, vec![], vec![], true, vec![2, 3, 4, 1], false, vec![],          vec![], true))]
#[case("1.*",                make_expanded_version_range_part("1.*",                vec![2],                            false, vec![], vec![], true, vec![1, 0],       false, vec![],          vec![], true))]
#[case("<1.2.3",             make_expanded_version_range_part("<1.2.3",             vec![1, 2, 3],                      false, vec![], vec![], true, vec![0, 0, 0],    false, vec![],          vec![], true))]
#[case("=1.2.3",             make_expanded_version_range_part("=1.2.3",             vec![1, 2, 3],                      true,  vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec![], true))]
#[case("^1.2.3",             make_expanded_version_range_part("^1.2.3",             vec![2],                            false, vec![], vec![], true, vec![1, 2, 3],    false, vec![],          vec![], true))]
// These ones appear in the function's comments
#[case("~2.3.4-r.1",         make_expanded_version_range_part("~2.3.4-r.1",         vec![2, 4],                         false, vec![], vec![], true, vec![2, 3, 4],    false, vec![],          vec!["r", "1"], false))]
#[case("~2.3.4",             make_expanded_version_range_part("~2.3.4",             vec![2, 4],                         false, vec![], vec![], true, vec![2, 3, 4],    false, vec![],          vec![], true))]
#[case("~2.3.4+r.2",         make_expanded_version_range_part("~2.3.4+r.2",         vec![2, 4],                         false, vec![], vec![], true, vec![2, 3, 4],    false, vec!["r", "2"],  vec![], false))]
fn test_parse_value_to_build_key_extended_version_range(
    #[case] vrange: &str,
    #[case] expected: BuildKeyExpandedVersionRange,
) {
    println!("ver: {}", vrange);
    let maxmin = BuildKeyExpandedVersionRange::parse_from_range_value(vrange).unwrap();
    assert_eq!(maxmin, expected)
}

#[rstest]
// This doesn't parse because the comma makes it look like 2 distinct
// version numbers and then 'test' isn't a valid version
// major.minor.patch number
#[should_panic]
#[case("25.0.8-alpha.0,test.1",
       BuildKeyExpandedVersionRange {
           max: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               notags: true,
               pretag: Some(vec![]),
           },
           min: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               notags: true,
               pretag: Some(vec![]),
           },
           tie_breaker: BuildKeyExpandedVersionRange::generate_tie_breaker("25.0.8-alpha.0,test.1")
       }
)]
// A version with a build. This doesn't directly parse as a
// BuildKeyExpandedVersionRange because of the build digest.
// That would need to be removed before trying to make a
// BuildKeyExpandedVersionRange from it, see below in
// test_generating_build_key() for an example of that.
#[should_panic]
#[case("4.1.0/DIGEST",
       BuildKeyExpandedVersionRange {
           max: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               notags: true,
               pretag: Some(vec![]),
           },
           min: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               notags: true,
               pretag: Some(vec![]),
           },
           tie_breaker: BuildKeyExpandedVersionRange::generate_tie_breaker("4.1.0/DIGEST")
       }
)]
// This doesn't parse because the characters after the first '/' are
// removed and then 'somepkg' isn't a valid version major.minor.patch
// number
#[should_panic]
#[case("somepkg/4.1.0/DIGEST",
       BuildKeyExpandedVersionRange {
           max: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               notags: true,
               pretag: Some(vec![]),
           },
           min: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               notags: true,
               pretag: Some(vec![]),
           },
           tie_breaker: BuildKeyExpandedVersionRange::generate_tie_breaker("somepkg/4.1.0/DIGEST")
       }
)]
fn test_parse_value_to_build_key_extended_version_range_invalid(
    #[case] vrange: &str,
    #[case] expected: BuildKeyExpandedVersionRange,
) {
    // These are expected to fail with panics during the parsing
    // in this call. When the parsing is changed to allow these values,
    // this test will have to be updated.
    println!("ver: {}", vrange);
    let maxmin = BuildKeyExpandedVersionRange::parse_from_range_value(vrange).unwrap();
    assert_eq!(maxmin, expected)
}

// Test that lists of version range string values sort correctly based
// on their maxmin tuples (for ordering between build key components).
//
// This tests if the values would order as expected if they were the
// values of build options for a set of builds, within a version being
// considered by the solver. The sorting is the same kind of thing the
// SortedBuildPackageIterators do.
#[rstest]
#[case(vec!["~9.3.1", "~6.3.1", "~4.8.2"], vec!["~9.3.1", "~6.3.1", "~4.8.2"])]
// Version ranges that include pre and post releases.  Not a
// pre-release should be smaller than a non-pre-release
#[case(vec!["~1.2.3", "~1.2.3+r.2", "~1.2.3-r.3", "~1.2.3", "~1.2.3+r.1", "~1.2.3-r.2"], vec!["~1.2.3+r.2", "~1.2.3+r.1", "~1.2.3", "~1.2.3", "~1.2.3-r.3", "~1.2.3-r.2"])]
// This one needs the tie_breaker hash third entry in the tuple to
// sort consistent between rust and python because because "1.2.3" and
// ">=1.2.3" have identical keys up to the tie-breaker.
#[case(vec!["=1.2.3", "1.2.3", ">=1.2.3", "~1.2.3"], vec!["1.2.3", ">=1.2.3", "~1.2.3", "=1.2.3"])]
// These values are used in the function's comments. Note: this is
// also a test that involves tags.
#[case(vec!["~2.3.4-r.1", "~2.3.4", "~2.3.4+r.2", "~2.3.4-r.2", "~2.3.4+r.1"], vec!["~2.3.4+r.2", "~2.3.4+r.1", "~2.3.4", "~2.3.4-r.2", "~2.3.4-r.1"])]
fn test_build_piece_ordering(#[case] values: Vec<&str>, #[case] expected: Vec<&str>) {
    // Test all the permutations
    for perm in values.iter().permutations(values.len()) {
        // Turn the perm(utation) into the same type as the expected
        // value before sorting
        let mut sample: Vec<&str> = perm.iter().map(|s| **s).collect::<Vec<&str>>();
        sample.sort_by_cached_key(|v| {
            let bevr = BuildKeyExpandedVersionRange::parse_from_range_value(v).unwrap();
            println!("{v} => {bevr}");
            bevr
        });
        sample.reverse();
        assert_eq!(sample, expected)
    }
}

// Test making build keys
#[rstest]
fn test_generating_build_key() {
    // Set up a binary package build spec
    let a_build = spec!({"pkg": "testpackage/1.0.0/TESTTEST"});

    // Set up some resolved build options
    let name1 = opt_name!("alib").to_owned();
    let name2 = opt_name!("somevar").to_owned();
    let name3 = opt_name!("notinthisbuild").to_owned();
    let name4 = opt_name!("apkg").to_owned();
    let name5 = opt_name!("versionbuild").to_owned();

    let value1 = "1.2.3".to_string();
    let value2 = "something".to_string();
    // value3 is left out deliberately to exercise unset value processing
    let value4 = ">1".to_string();
    // This is not a valid version, unless the build digest is stripped off
    let value5 = "4.1.0/DIGEST".to_string();

    let mut resolved_options: OptionMap = OptionMap::default();
    resolved_options.insert(name1.clone(), value1);
    // value3 is left out deliberately to exercise unset value processing
    resolved_options.insert(name2.clone(), value2.clone());
    resolved_options.insert(name4.clone(), value4);
    resolved_options.insert(name5.clone(), value5);

    // Generate the build's key based on the ordering of option names
    let ordering = vec![name1, name2, name3, name4, name5];
    let key = BuildKey::new(a_build.ident(), &ordering, &resolved_options);

    // Expected build key structure for this ordering and build options:
    // "alib", "somevalue", "notinthisbuild", "apkg", "versionbuild" build digest
    //  1.2.3,  something,     notset,         >1,    4.1.0/DIGEST,  TESTTEST
    let expected = BuildKey::Binary(vec![
        // 1.2.3
        BuildKeyEntry::ExpandedVersion(make_expanded_version_range_part(
            "1.2.3",
            vec![u32::MAX, u32::MAX, u32::MAX],
            false,
            vec![],
            vec![],
            true,
            vec![1, 2, 3],
            false,
            vec![],
            vec![],
            true,
        )),
        // something
        BuildKeyEntry::Text(value2),
        // notset
        BuildKeyEntry::NotSet,
        //  >1
        BuildKeyEntry::ExpandedVersion(make_expanded_version_range_part(
            ">1",
            vec![u32::MAX, u32::MAX, u32::MAX],
            false,
            vec![],
            vec![],
            true,
            vec![1],
            true,
            vec![],
            vec![],
            true,
        )),
        // This will have the build digest removed and should be treated as
        // if it was 4.1.0
        // "4.1.0/DIGEST" -> 4.1.0
        BuildKeyEntry::ExpandedVersion(make_expanded_version_range_part(
            "4.1.0",
            vec![u32::MAX, u32::MAX, u32::MAX],
            false,
            vec![],
            vec![],
            true,
            vec![4, 1, 0],
            false,
            vec![],
            vec![],
            true,
        )),
        // build digest as a string, it is always the last entry
        BuildKeyEntry::Text("TESTTEST".to_string()),
    ]);

    assert_eq!(key, expected)
}

#[rstest]
fn test_generating_build_key_src_build() {
    // Set up a src package build spec
    let a_build = spec!({"pkg": "testpackage/1.0.0/src"});

    // Set up some resolved build options
    let name1 = opt_name!("alib").to_owned();
    let name2 = opt_name!("somevar").to_owned();
    let name3 = opt_name!("notinthisbuild").to_owned();
    let name4 = opt_name!("apkg").to_owned();

    let value1 = "1.2.3".to_string();
    let value2 = "something".to_string();
    // value3 is left out deliberately to exercise unset value processing
    let value4 = "1.0.0,<1.5".to_string();

    let mut resolved_options: OptionMap = OptionMap::default();
    resolved_options.insert(name1.clone(), value1);
    // value3 is left out deliberately to exercise unset value processing
    resolved_options.insert(name2.clone(), value2);
    resolved_options.insert(name4.clone(), value4);

    // Generate the build's key based on the ordering of option
    // names. Note: because this is a source build it won't use any of
    // the ordering or option names in the key generation
    let ordering = vec![name1, name2, name3, name4];
    let key = BuildKey::new(a_build.ident(), &ordering, &resolved_options);

    // Expected build key structure
    let expected = BuildKey::Src;

    assert_eq!(key, expected)
}
