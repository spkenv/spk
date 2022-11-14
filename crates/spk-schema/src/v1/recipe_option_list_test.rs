// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::RecipeOptionList;

#[rstest]
fn parse_example() {
    const EXAMPLE_YAML: &str = r#"
# a package option defines at least the acceptable range for
# each dependency. Without any other information, this is both
# build and runtime dependency, where the runtime dependency
# must be binary compatible with whatever was used at build-time
- pkg: python/3

# if a package is only required at build-time and not run time,
# then the runtime requirement can be explicitly disabled
- pkg: pybind11/2
  atRuntime: false
# similarly, if a package is only required at run time
- pkg: python:interpreter/3
  atBuild: false

# as noted, the default requirement at runtime will be something
# that is binary compatible with the version used at build-time.
# This is often unnecessarily restrictive, and can be further
# specified
- pkg: python/3
  atRuntime:
    version: 3.7
# you can also generate the runtime requirement using the version
# that is resolved at build-time. In this case, each 'x' is replaced
# with digits from the build-time version
- pkg: python/3
  atRuntime:
    version: x.x

# by default, the same components used at build time are used
# at runtime, but this can be overridden as well
- pkg: python:{build,interpreter}/3
  atRuntime:
    components: [interpreter]

# sometimes, you want to put a restriction on a dependency without
# requiring that it's included at all times. For example, I don't
# require gcc at runtime, but, if it's present, it must compatible
# with the version that was used when I was built. These are
# denoted with the `when` field.
- pkg: gcc/>=4
  atRuntime:
    when: Requested
# similarly, this can be used for build dependencies that are not
# needed for all variants
- pkg: python
  atBuild:
    when: Requested
  atRuntime:
    # TODO: should this be a default when atBuild is set?
    #       can we even reason about this for all atBuild cases?
    when: Requested

# a variable option defines at least a name, which means that
# any change to this variable generates a different package build
- var: os

# optionally, a default value can be provided for variables, which
# are used when no other value is given at build- or run-time.
- var: optimize/2

# in many cases, there are only a certain number of allowed values
# for a variable
- var: debug/off
  choices: [on, off]
# Note that when choices are given, if a default is not
# specified, then a value for this variable will need to be given
# explicitly by the downstream consumer or at the command line.
# This can be a useful tool to create explicit incompatibilities
# in a package's history, but should be used sparingly
- var: epoch
  choices: [2]

# just like packages, variables must have the same value at
# run-time that they do at build-time, but this can be overridden
- var: optimize/2
  atRuntime: false

# For both variable and package options, there are also cases
# when options need to propagate between packages.
# For example, when you compile against cPython the
# abi that was used should be specified on the resulting package
- var: abi/cp27m
  choices: [cp27m, cp27mu]
  # this will cause a validation error when this package
  # appears in a build environment but the package being built
  # does not specify python.abi as one of its options 'atBuild'
  atDownstreamBuild: true
  # this options will cause a validation error when this
  # package appears in a build environment but the package
  # being built does not specify python.api as one of its
  # options 'atRuntime'.
  atDownstreamRuntime: true

# For all types options, the `when` field can be used
# to further refine the contexts in which they are relevant
#
# The `when` field takes either one or a list of conditions.
# When any of these conditions is true, the requested package
# is included in the resolved environment.
#
# `when: Requested` is an alias for `when: []`, which simply
# means that the request is never explicitly included by this
# package, and must be instead brought in by another.
# The default value is `when: Always`
#
# the when field is valid for all of:
#   atBuild, atRuntime, atDownstreamBuild, atDownstreamRuntime

# Sometimes, the inclusion of one package depends on
# the inclusion of another
- pkg: python-requests
  atRuntime:
    when:
      - pkg: python

# similarly, the inclusion of a package may depend on
# the version range of some other package
- pkg: python-enum34
  atRuntime:
    when:
      - pkg: python/<3.4

# similarly, the inclusion of a package may depend on
# the usage of some component, either from this package
# or any other
- pkg: qt/5
  atRuntime:
    version: FromBuild
    when:
      - pkg: thispackage:gui

# the inclusion of a request may also be dependant on the
# value of some variable
- pkg: python:debug
  atBuild: false
  atRuntime:
    when:
      # when referring to a variable in this package,
      # the package name can optionally be excluded
      - var: thispackage.debug/on

# these when clauses can be combined in interesting ways,
# like making a choice between two different packages
# that both provide similar functionality
- var: jpeg/libjpeg-turbo
  choices: [libjpeg, libjpeg-turbo]
- pkg: libjpeg-turbo/8.8
  when:
    - var: jpeg/libjpeg-turbo
- pkg: libjpeg/8.8
  when:
    - var: jpeg/libjpeg
"#;
    let opts = match serde_yaml::from_str::<RecipeOptionList>(EXAMPLE_YAML) {
        Err(err) => {
            println!(
                "{}",
                format_serde_error::SerdeError::new(EXAMPLE_YAML.into(), err)
            );
            panic!("primary example should parse")
        }
        Ok(o) => o,
    };
    let yaml = serde_yaml::to_string(&opts).expect("example should serialize");
    let opts2 = match serde_yaml::from_str::<RecipeOptionList>(&yaml) {
        Err(err) => {
            println!("{}", format_serde_error::SerdeError::new(yaml, err));
            panic!("example should round-trip parse")
        }
        Ok(o) => o,
    };
    assert_eq!(opts2, opts, "yaml round trip should be the same");
}
