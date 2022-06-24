// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use itertools::Itertools;

use super::{
    BuildKey, BuildKeyEntry, BuildKeyExpandedVersionRange, BuildKeyVersionNumber,
    BuildKeyVersionNumberPiece,
};
use crate::api::parse_ident;
use crate::api::OptionMap;
use crate::api::Spec;

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
    min_digits: Vec<u32>,
    min_plus_epsilon: bool,
    min_post: Vec<&str>,
    min_pre: Vec<&str>,
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
            pretag: pre_max,
        },
        min: BuildKeyVersionNumber {
            digits: min_digits
                .iter()
                .map(|n| BuildKeyVersionNumberPiece::Number(*n))
                .collect::<Vec<BuildKeyVersionNumberPiece>>(),
            plus_epsilon: min_plus_epsilon,
            posttag: post_min,
            pretag: pre_min,
        },
        tie_breaker: BuildKeyExpandedVersionRange::generate_tie_breaker(version),
    }
}

#[rustfmt::skip]
#[rstest]
#[case("1.2.3",              make_expanded_version_range_part("1.2.3",              vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![1, 2, 3],    false, vec![],          vec![]))]
#[case("1.2.3.4",            make_expanded_version_range_part("1.2.3.4",            vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![1, 2, 3, 4], false, vec![],          vec![]))]
#[case("1.2.3-r.1",          make_expanded_version_range_part("1.2.3-r.1",          vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![1, 2, 3],    false, vec![],          vec!["r", "1"]))]
#[case("1.2.3+r.2",          make_expanded_version_range_part("1.2.3+r.2",          vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![1, 2, 3],    false, vec!["r", "2"],  vec![]))]
#[case("1.2.3-a.4+r.6",      make_expanded_version_range_part("1.2.3-a.4+r.6",      vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![1, 2, 3],    false, vec!["r", "6"],  vec!["a", "4"]))]
#[case("~1.2.3",             make_expanded_version_range_part("~1.2.3",             vec![1, 3],                         false, vec![], vec![], vec![1, 2, 3],    false, vec![],          vec![]))]
#[case("~1.2.3.1",           make_expanded_version_range_part("~1.2.3.1",           vec![1, 2, 4],                      false, vec![], vec![], vec![1, 2, 3, 1], false, vec![],          vec![]))]
#[case(">=1.2.3",            make_expanded_version_range_part(">=1.2.3",            vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![1, 2, 3],    false, vec![],          vec![]))]
#[case(">=1.2.3,<1.2.5",     make_expanded_version_range_part(">=1.2.3,<1.2.5",     vec![1, 2, 5],                      false, vec![], vec![], vec![1, 2, 3],    false, vec![],          vec![]))]
#[case(">=1.2.3+r.2,<1.2.5", make_expanded_version_range_part(">=1.2.3+r.2,<1.2.5", vec![1, 2, 5],                      false, vec![], vec![], vec![1, 2, 3],    false, vec!["r", "2"],  vec![]))]
#[case(">=2.3.4.1,<2.3.5",   make_expanded_version_range_part(">=2.3.4.1,<2.3.5",   vec![2, 3, 5],                      false, vec![], vec![], vec![2, 3, 4, 1], false, vec![],          vec![]))]
#[case("1.*",                make_expanded_version_range_part("1.*",                vec![2],                            false, vec![], vec![], vec![1, 0],       false, vec![],          vec![]))]
#[case("<1.2.3",             make_expanded_version_range_part("<1.2.3",             vec![1, 2, 3],                      false, vec![], vec![], vec![0, 0, 0],    false, vec![],          vec![]))]
#[case("=1.2.3",             make_expanded_version_range_part("=1.2.3",             vec![1, 2, 3],                      true,  vec![], vec![], vec![1, 2, 3],    false, vec![],          vec![]))]
#[case("^1.2.3",             make_expanded_version_range_part("^1.2.3",             vec![2],                            false, vec![], vec![], vec![1, 2, 3],    false, vec![],          vec![]))]
// A version with a build. This should be treated as if it was just the version
#[case("4.1.0/DIGEST",       make_expanded_version_range_part("4.1.0/DIGEST",       vec![u32::MAX, u32::MAX, u32::MAX], false, vec![], vec![], vec![4, 1, 0],    false, vec![],          vec![]))]
// These ones appear in the function's comments
#[case("~2.3.4-r.1",         make_expanded_version_range_part("~2.3.4-r.1",         vec![2, 3, 5],                      false, vec![], vec![], vec![2, 3, 4],    false, vec![],          vec!["r", "1"]))]
#[case("~2.3.4",             make_expanded_version_range_part("~2.3.4",             vec![2, 4],                         false, vec![], vec![], vec![2, 3, 4],    false, vec![],          vec![]))]
#[case("~2.3.4+r.2",         make_expanded_version_range_part("~2.3.4+r.2",         vec![2, 3, 5],                      false, vec![], vec![], vec![2, 3, 4],    false, vec!["r", "2"],  vec![]))]
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
               pretag: Some(vec![]),
           },
           min: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
               pretag: Some(vec![]),
           },
           tie_breaker: BuildKeyExpandedVersionRange::generate_tie_breaker("25.0.8-alpha.0,test.1")
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
               pretag: Some(vec![]),
           },
           min: BuildKeyVersionNumber {
               digits: vec![],
               plus_epsilon: false,
               posttag: Some(vec![]),
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
// Version ranges that include pre and post releases.
// Note: the expected ordering puts ~1.2.3 first. That's because its
// less_than() call returns '1.3.0' currently, whereas '~1.2.3+r2' and
// '~1.2.3-r.1' return '1.2.4' from less_than(). spk seems to be
// constraining version numbers that have tags to smaller ranges. If
// this is a bug and it gets fixed, these tests will fail and need to
// be updated.
#[case(vec!["~1.2.3", "~1.2.3+r.2", "~1.2.3-r.3"], vec!["~1.2.3", "~1.2.3+r.2", "~1.2.3-r.3"])]
// This one needs the tie_breaker hash third entry in the tuple to
// sort consistent between rust and python because because "1.2.3" and
// ">=1.2.3" have identical keys up to the tie-breaker.
#[case(vec!["=1.2.3", "1.2.3", ">=1.2.3", "~1.2.3"], vec!["1.2.3", ">=1.2.3", "~1.2.3", "=1.2.3"])]
// These values are used in the function's comments. Note: this is
// also a test that involves tags, see the comments the test 2 entries
// back.
#[case(vec!["~2.3.4-r.1", "~2.3.4", "~2.3.4+r.2"], vec!["~2.3.4", "~2.3.4+r.2", "~2.3.4-r.1"])]
fn test_build_piece_ordering(#[case] values: Vec<&str>, #[case] expected: Vec<&str>) {
    // Test all the permutations
    for perm in values.iter().permutations(values.len()) {
        // Turn the perm(utation) into the same type as the expected
        // value before sorting
        let mut sample: Vec<&str> = perm.iter().map(|s| **s).collect::<Vec<&str>>();
        sample.sort_by_cached_key(|v| {
            BuildKeyExpandedVersionRange::parse_from_range_value(v).unwrap()
        });
        sample.reverse();
        assert_eq!(sample, expected)
    }
}

// Test making build keys
#[rstest]
fn test_generating_build_key() {
    // Set up a binary package build spec
    let a_build = Spec::new(parse_ident("testpackage/1.0.0/TESTTEST").unwrap());

    // Set up some resolved build options
    let name1: String = "alib".to_string();
    let name2: String = "somevar".to_string();
    let name3: String = "notinthisbuild".to_string();
    let name4: String = "apkg".to_string();

    let value1: String = "1.2.3".to_string();
    let value2: String = "something".to_string();
    // value3 is left out deliberately to exercise unset value processing
    let value4: String = ">1".to_string();

    let mut resolved_options: OptionMap = OptionMap::default();
    resolved_options.insert(name1.clone(), value1);
    // value3 is left out deliberately to exercise unset value processing
    resolved_options.insert(name2.clone(), value2.clone());
    resolved_options.insert(name4.clone(), value4);

    // Generate the build's key based on the ordering of option names
    let ordering: Vec<String> = vec![name1, name2, name3, name4];
    let key = BuildKey::new(&a_build.pkg, &ordering, &resolved_options);

    // Expected build key structure for this ordering and build options:
    // "alib", "somevalue", "notinthisbuild", "apkg", build digest
    //  1.2.3,  something,     notset,         >1,    TESTTEST
    let expected = BuildKey::Binary(vec![
        // 1.2.3
        BuildKeyEntry::ExpandedVersion(make_expanded_version_range_part(
            "1.2.3",
            vec![u32::MAX, u32::MAX, u32::MAX],
            false,
            vec![],
            vec![],
            vec![1, 2, 3],
            false,
            vec![],
            vec![],
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
            vec![1],
            true,
            vec![],
            vec![],
        )),
        // build digest as a string, it is always the last entry
        BuildKeyEntry::Text("TESTTEST".to_string()),
    ]);

    assert_eq!(key, expected)
}

#[rstest]
fn test_generating_build_key_src_build() {
    // Set up a src package build spec
    let a_build = Spec::new(parse_ident("testpackage/1.0.0/src").unwrap());

    // Set up some resolved build options
    let name1: String = "alib".to_string();
    let name2: String = "somevar".to_string();
    let name3: String = "notinthisbuild".to_string();
    let name4: String = "apkg".to_string();

    let value1: String = "1.2.3".to_string();
    let value2: String = "something".to_string();
    // value3 is left out deliberately to exercise unset value processing
    let value4: String = "1.0.0,<1.5".to_string();

    let mut resolved_options: OptionMap = OptionMap::default();
    resolved_options.insert(name1.clone(), value1);
    // value3 is left out deliberately to exercise unset value processing
    resolved_options.insert(name2.clone(), value2);
    resolved_options.insert(name4.clone(), value4);

    // Generate the build's key based on the ordering of option
    // names. Note: because this is a source build it won't use any of
    // the ordering or option names in the key generation
    let ordering: Vec<String> = vec![name1, name2, name3, name4];
    let key = BuildKey::new(&a_build.pkg, &ordering, &resolved_options);

    // Expected build key structure
    let expected = BuildKey::Src;

    assert_eq!(key, expected)
}
