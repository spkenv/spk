// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pretty_assertions::assert_eq;
use rstest::rstest;
use spk_schema_foundation::option_map::OptionMap;
use spk_schema_foundation::{option_map, FromYaml};

use super::WhenBlock;

#[rstest]
// The default value is `"when": Always`
#[case("Always", WhenBlock::Always)]
// `"when": "Requested"` is an alias for `"when": []`, which simply,
// means that the request is never explicitly included by this
// package, and must be instead brought in by another.
#[case("Requested", WhenBlock::when_requested())]
#[case("[]", WhenBlock::when_requested())]
fn test_parsing(#[case] yaml: &str, #[case] expected: WhenBlock) {
    let actual = WhenBlock::from_yaml(yaml).expect("when condition should parse");
    assert_eq!(actual, expected)
}

#[rstest]
// sometimes, you want to put a restriction on a dependency without
// requiring that it's included at all times. For example, I don't
// require python at runtime, but, if it's present, it must compatible
// with the version that was used when I was built. These are
// denoted with the `when` field.
#[case(
    "[{pkg: python}]",
    option_map!{},
    "[]",
    false,
)]
#[case(
    "[{pkg: python}]",
    option_map!{},
    "[{pkg: python/3.7.9/BGSHW3CN}]",
    true,
)]
// similarly, the inclusion of a package may depend on
// the version range of some other package
#[case(
    "[{pkg: 'python/<3'}]",
    option_map!{},
    "[{pkg: python/3.7.9/BGSHW3CN}]",
    false,
)]
#[case(
    "[{pkg: 'python/<3'}]",
    option_map!{},
    "[{pkg: python/2.7.5/BGSHW3CN}]",
    true,
)]
// similarly, the inclusion of a package may depend on
// the usage of some component, either from this package
// or any other
#[case(
    "[{pkg: 'this-package:gui'}]",
    option_map!{},
    "[{
        pkg: this-package/1.0.0/BGSHW3CN,
        package: {
          components: [{
            name: gui
          }]
        }
    }]",
    true,
)]
// the inclusion of a request may also be dependant on the
// value of some variable
#[case(
    "[{var: 'this-package.debug/on'}]",
    option_map!{},
    "[]",
    false,
)]
#[case(
    "[{var: 'this-package.debug/on'}]",
    option_map!{"debug" => "off"},
    "[]",
    false,
)]
#[case(
    "[{var: 'this-package.debug/on'}]",
    option_map!{"debug" => "on"},
    "[]",
    true,
)]
fn test_activation(
    #[case] yaml: &str,
    #[case] build_options: OptionMap,
    #[case] build_env: &str,
    #[case] expected: bool,
) {
    let when = WhenBlock::from_yaml(yaml).expect("when condition should parse");
    let build_env =
        Vec::<crate::v1::Package>::from_yaml(build_env).expect("build env package should parse");
    let actual = when.check_is_active(&(build_options, build_env));
    assert_eq!(actual.is_ok(), expected, "{actual}");
}
