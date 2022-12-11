// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pretty_assertions::assert_eq;
use rstest::rstest;
use spk_schema_foundation::ident_component::Component;
use spk_schema_foundation::FromYaml;

use crate::{Package, Recipe};

#[rstest]
fn parse_example() {
    const EXAMPLE_YAML: &str = r#"
pkg: example/1.0.0
compat: x.a.b

# Package options define the set of dependencies and inputs variables
# to the package build process. This
options:
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
  # that both provide similar functionality. Additionally,
  # a global 'when' can be provided which is applied to all
  # the 'at*' fields that aren't otherwise specified
  - var: jpeg/libjpeg-turbo
    choices: [libjpeg, libjpeg-turbo]
  - pkg: libjpeg-turbo/8.8
    when:
      - var: jpeg/libjpeg-turbo
  - pkg: libjpeg/8.8
    when:
      - var: jpeg/libjpeg

# the source section defines how the source files are collected
source:
  collect:
    # this section is the same as the old `sources`
    # section of the package spec, and defines where
    # and how the necessary sources are collected
    # TODO: can we support the hacky "local or else
    #       download" workflow
    - path: ./
  test:
    # tests for the source package are defined here,
    # and are run immediately after the make-source
    # collection process is complete unless --no-test
    # is specified on the command line
    # ... more on test and build scripts below
    - script: echo "I am a test"
    # additionally, tests can include a when clause
    # and specify requests for the environment that
    # the tests should be run in
    - when: {var: os/linux}
      requests:
        - {pkg: python/3.7}
      script: python run_checks.py

# the build section defines how the source package
# is turned into a binary package / package  build
build:
  # these are the variants that are built by default
  # by the `spk build` command when no other options
  # are provided
  variants:
    - name: vfx2021 # still has an index, but now also a name (optionally)
      # default: false, or maybe something more rich to include publish-ability?
      requests:
       - {pkg: dependency/1.0.0}
       - {var: debug/on}
      # change duplicate variants to a warning or remove them entirely
      #   we could skip builds that have the same hash but don't need to error
  # a script to run which will do any necessary building
  # and then install the software into spfs
  script:
    # each line of the script can simply be a string
    # to be added to the build script
    - echo hello
    # alternatively, the `when` syntax can be used to filter
    # lines of script based on the build being executed
    - when: {var: debug/on}
      do: echo debug mode enabled
    # the when/do structure is also recursive, so larger
    # blocks of script can be divided as appropriate
    - when: {var: os/linux}
      do:
        - echo hello, linux
        - when: {pkg: python/3}
          do: echo building for python 3 on linux
  # build tests are run immediately after the binary
  # package has been created, with all of the build
  # artifacts still in place (unless --no-test is specified
  # at the command line)
  #
  # unlike other tests, the build tests CANNOT specify
  # additional packages/requests as this could mess with
  # the build environment
  test:
    - script: echo I, am a test
    # like before, tests can also specify a when clause
    - when: {var: os/linux}
      script: echo test on linux
    # as well as leverage the same recursive script
    # structure as needed
    - script:
      - when: {pkg: python/3}
        do: echo test against python 3

# the package section specifies additional details about
# how the installed build files should be packaged, and
# how that package should behave when used
package:
  environment:
    # environment variables can be set, appended, prepended, etc
    # the same as before
    - set: PYTHONDONTWRITEBYTECODE
      value: 1
    # additionally, the `when` clause can be used to limit
    # the application of any variable operation
    - append: PYTHONPATH
      value: /spfs/specialpath
      when:
        - {pkg: python} # when python is included in the build
        - {var: os/linux}

  # package components are also largely unchanged from
  # the past version of the spec
  components:
    # except that they too can use the when clause
    - name: python
      uses: [run]
      files:
        - site-packages/
      when:
        - {pkg: python}

  # package tests run against specific binary packages,
  # resolved into an environment and installed with the
  # above configuration. These tests ensure that it's
  # useful from a downstream perspective
  test:
    # the same semantics carry here as other test sections above
    - when:
        - var: os/linux
      requests:
        - pkg: python
      script:
        - python -m example.pythonmodule
"#;
    let opts = match serde_yaml::from_str::<super::Recipe>(EXAMPLE_YAML) {
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
    let opts2 = match serde_yaml::from_str::<super::Recipe>(&yaml) {
        Err(err) => {
            println!("{}", format_serde_error::SerdeError::new(yaml, err));
            panic!("example should round-trip parse")
        }
        Ok(o) => o,
    };
    assert_eq!(opts2, opts, "yaml round trip should be the same");
}

#[rstest]
fn test_source_generation() {
    let pkg = "my-package/1.0.0".parse().unwrap();
    let recipe = super::Recipe::new(pkg);

    let root = std::path::Path::new("/some/root");
    let source = recipe
        .generate_source_build(root)
        .expect("should not fail to generate source build");
    let components = source.components();
    let component_names = components.iter().map(|c| &c.name).collect::<Vec<_>>();
    assert_eq!(
        component_names,
        vec![&Component::Source],
        "source package should only have one component and it should be the source component"
    )
}

#[rstest]
fn test_sources_default_local_path() {
    let root = std::path::Path::new("/some/root");
    let spec = super::Recipe::from_yaml("pkg: my-pkg")
        .unwrap()
        .generate_source_build(root)
        .unwrap();
    let expected = vec![crate::SourceSpec::Local(crate::LocalSource::new(root))];
    assert_eq!(
        spec.sources(),
        &expected,
        "expected spec to have one local source"
    );
}
