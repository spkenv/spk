// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema::foundation::name::OptName;
use spk_schema::foundation::option_map::OptionMap;

#[rstest]
#[case(&["hello:world"], &[("hello", "world")])]
#[case(&["hello=world"], &[("hello", "world")])]
#[case(&["{hello: world}"], &[("hello", "world")])]
#[case(&["{python: 2.7}"], &[("python", "2.7")])]
#[case(
    &["{python: 2.7, python.abi: py37m}"],
    &[("python", "2.7"), ("python.abi", "py37m")],
)]
#[should_panic]
#[case(&["{hello: [world]}"], &[])]
#[should_panic]
#[case(&["{python: {v: 2.7}}"], &[])]
#[should_panic]
#[case(&["value"], &[])]
fn test_option_flags_parsing(#[case] args: &[&str], #[case] expected: &[(&str, &str)]) {
    let options = super::Options {
        no_host: true,
        options_file: Default::default(),
        options: args.iter().map(ToString::to_string).collect(),
    };
    let actual = options.get_options().unwrap();
    let expected: OptionMap = expected
        .iter()
        .map(|(k, v)| (OptName::new(k).unwrap().to_owned(), (*v).into()))
        .collect();
    assert_eq!(actual, expected);
}
