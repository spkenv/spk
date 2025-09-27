---
title: Package Tests
summary: A framework to validate various aspects of your package.
weight: 50
---

Tests can also be defined in the package spec file. SPK currently supports three types of tests that validate different aspects of the package. Tests are defined by a bash script and _stage_.

```yaml
pkg: my-package/1.0.0

# the tests section can define any number of
# tests to validate the package
tests:
  - stage: build
    script: python -m "unittest"
```

> [!TIP]
> You can run package tests using the `spk test` command.

### Stages

The **stage** of each test identifies when and where the test should be run. There are three stages that can currently be tested:

| stage   | description                                                                                             |
| ------- | ------------------------------------------------------------------------------------------------------- |
| sources | runs against the created source package, to validate that source files are correctly laid out           |
| build   | runs in the package build environment, usually for unit testing                                         |
| install | runs in the installation environment against the compiled package, usually for integration-type testing |

### Variant Selectors

Like builds, tests are executed by default against all package variants defined in the build section of the spec file. Each test can optionally define a list of selectors to reduce the set of variants that is is run against.

```yaml
build:
  variants:
    - { python: 3 }
    - { python: 2 }

tests:
  - stage: install
    selectors:
      - { python: 3 }
    script:
      - "test python 3..."

  - stage: install
    selectors:
      - { python: 2 }
    script:
      - "test python 2..."
```

The test is executed if the variant in question matches at least one of the selectors.

> [!IMPORTANT]
> Selectors must match exactly the build option values from the build variants. For example: a `python: 2.7` selector will not match a `python: 2` build variant.

### Requirements

You can specify additional requirements for any defined test.

> [!IMPORTANT]
> These requirements are merged with those of test environment so be sure that they do not conflict with what you are testing.

```yaml
build:
  options:
    - pkg: python/3

tests:
  - stage: install
    requirements:
      - pkg: pytest
    script:
      - pytest
```
