// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::TimeSpec;

#[rstest]
#[case("~3y")]
#[case("~25w")]
#[case("~13d")]
#[case("~2h")]
#[case("~33m")]
#[case("~220s")]
#[case("@2020-01-31")]
#[case("@9am")]
#[case("@9pm")]
#[case("@9:00am")]
#[case("@9:30pm")]
#[case("@14:45")]
#[case("@2020-05-07T09:00:00+04:00")]
fn test_parsing(#[case] source: &str) {
    let spec = TimeSpec::parse(source).expect("Failed to parse time spec");
    let out = spec.to_string();
    let spec2 = TimeSpec::parse(out).expect("Should re-parse formatted spec");
    assert_eq!(
        spec2, spec,
        "Re-parsed spec should be the same as it's source"
    );
}
