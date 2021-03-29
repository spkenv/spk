---
title: Design Overview
summary: General concepts and codebase layout
weight: 10
---

## Design Goals

First and foremost, spk is a package manager for spfs. It builds on the unique technical features of spfs by making the system more familiar and approachable to developers. By introducing the concept of a package, spk adds versioning and dependency resolution to the spfs filesystem layering system - making the whole system easier to use and manage in a large, dynamic software environment. With this in mind, spk also has some more specific design goals based on what we loved and didn't love about existing package mangers.

### Package Compatibility Beyond Version Numbers

In many cases, a version is not enough to understand the complex compatibility requirements of a package. Most package managers additionally include host architecture and/or platform information to fill this kind of need. Unfortunately, these kinds of labels are often baked into the package and cannot be defined in an extensible way.

The VFX Reference Platform, along with systems like SpComp2 and Rez Variants prove that in our world of Python, DCCs, and VFX libraries, there is a great need to extend and define these fundamental platform traits in order to properly organize, build and identify packages in a compatible way.

### Recipe and Source Publication

Similar to the above, there are many aspects of a platform or environment that can change which requires a new variant to be built. As the set of options grows, so too does the number of possible permutations and combinations. Though many may not be valid, it may also not be pragmatic to build and publish all those that are.

The ability to publish a source package along with its build recipe, enables the package manager to build new variants of a package on-demand. This allows package maintainers to build, test, and publish the most common binaries required for production, while enabling developers to generate more bespoke or alternative environments more easily.

### Fast, Dynamic Build and Runtime Environments

One of the great benefits of Rez in our context, is the ability to very quickly build multiple variants, and jump into multiple environments to test and execute them. As the size and complexity of environments increases, the meaning of “fast” may degrade, but the spirit remains in the simplicity of generating and executing a number of variants.

### Reliable and Natural Definition of Platforms and Constraints

In many ways, each DCC and each DCC version acts like it’s own platform. Often they impose constraints on the build environment, and come prepackaged with one or more libraries and tools. Sometimes this can be safely ignored, but often it becomes important for binaries to share a version or link against the bundled software, especially in the case of native DCC plugins.

In a similar way, we aim to provide software platforms that make it easier for TDs and developers to work within their appropriate domain. Our current set-shot + vctools development workflow functions this way, where the state of the SVN repository is a single platform that everyone builds to, which makes it easy to develop, contribute, and share code without a deep understanding of the runtime environment.

## Additional/Functional Goals

### Pre-Release Versioning

A pre-release package is one that is released for consumption but may not yet be considered stable for production use. These packages are not resolved into environments unless they are explicitly requested. This allows developers to safely publish and share bleeding-edge software with a select set of users and test through the actual production release process and runtime environment.

### Post-Release Versioning

When a package needs to be regenerated, but the software has not changed, there should be a mechanism to essentially bump the version of the package without changing the version of the software within it.

### Optional Constraints

AKA a weak reference in rez, the idea is that a package may not require another package, but has an opinion about what versions can be used if that package is required or requested by something else in the environment.

### Package Deprecation

When a package is determined to be incorrect or has problems, it’s important to be able to mark it as such. This ensures that those who need the package can continue to use it, but that they can be made aware of its danger and plan for an upgrade. Similarly, new environments would not be built with these packages, to ensure that the problems do not propagate.
