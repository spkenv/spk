---
title: Codebase Overview
summary: Overview of the codebase and implementation structure
weight: 20
---

## General Concepts and Structure

### Package Lifecycle

The most core concept of spk is the package. In order to take a package yaml file and turn it into a resolvable binary or source package, we move the package through the pipeline below. There are multiple data types (traits) that are used to represent a package at these different stages.

```mermaid
graph LR;

template[Template] ==> render([render])
vars{{variables}} -.-> render([render])
root{{root path}} -.-> generate_s
render ==> recipe[Recipe]
recipe ==> generate_s([generate source package])
recipe ==> generate_b([generate binary package])
options{{options}} -.-> generate_b
build_env{{build environment}} -.-> generate_b
generate_s ==> sp[Package]
generate_b ==> bp[Package]
```

The lifecycle begins with a `Template`, but only package spec file containing a top-level `template` block is treated as a template. The `render` step can use metadata within this block to discover and validate input `variables` (such as the version number) against external sources. Once validated, the metadata is stripped and the variables are injected into the rest of the file to produce a concrete, single-version `Recipe`. This recipe is then used to generate the source and binary packages. For more information, see the guide on [Templated Recipes]({{< ref "../use/create/templated-recipes" >}}).

### Metadata vs Payloads

Each package is made up of two pieces which are important to differentiate: the package `payload` and `specification (spec)`. The package payload is the set of files on disk that package 'contains'. When you install a package, the payload is the files that you actually see in `/spfs`. The package specification (or metadata) is information about the package: how it was built, what it's dependencies are, and everything else that's important for both spk and developers to know.

```mermaid
graph LR;

spk --> build
spk --> test
build --> solve
test --> solve
solve --> api
solve --> storage
```

### High-Level Modules

At the top of this graph are the `spk`, `build` and `test` modules. `spk` defines the highest level API for running spk environments, publishing packages, etc. One step down from that the `build` and `test` modules define how spk build and test environments are created and executed, with the `build` package also defining how both source and binary packages should be validated and captured in spfs.

### Environment Solver

Underpinning all of the high level logic is the spk `solve` module, which contains the implementation of the spk solver. The solver is responsible for all of the dependency and environment resolution in spk - the real meat and potatoes of what spk provides as a package manager. The solver architecture is covered in more detail [here]({{< ref "./solver" >}}).

At the time of writing, you will also find a `legacy` solver implementation which represents the codebase before some refactoring was done to clean up the structure and maintainability of the solver code. This is kept around as a regression testing baseline, but could reasonably be removed in a future release.

### API and Storage

The API module is where the package specification is defined, as well as the syntax and parsing logic for package identifiers in the form `name/version/build`. The `storage` module and it's children define and implement a repository interface for how packages are stored and persisted for reading back later. The main repository type that is used in production is a wrapper around an spfs repository - detailed below:

#### SPK and SPFS

SPK uses spfs as it's database, using spfs tags to identify and index all relevant information. Package payloads are managed entirely by spfs, with spk only needing to tag the layer to associate it to a package. Package specs are stored directly as blobs in spfs and tagged in a similar way so that they can be found and loaded back.

SpFS tags are largely the same as package names with the only difference being that spfs tags do not allow the use of the `+` character, which is instead encoded as `..`. Additionally, spk keeps all of it's tags under an `spk` directory and separates the package payload (`pkg`) from package spec (`spec`) tags using another directory. The following are some examples of packages mapped to their spk tags:

```
python/2.7.5/src          --> spk/spec/python/2.7.5/src           (spec)
                          --> spk/pkg/python/2.7.5/src            (layer)
python/2.7.5/BGSHW3CN     --> spk/spec/python/2.7.5/BGSHW3CN      (spec)
                          --> spk/pkg/python/2.7.5/BGSHW3CN       (layer)
python/2.7.5+r.1/BGSHW3CN --> spk/spec/python/2.7.5..r.1/BGSHW3CN (spec)
                          --> spk/pkg/python/2.7.5..r.1/BGSHW3CN  (layer)
```

Additionally, there are a few key points to know with this structure:

  - package specs for a specific build are published in a `rendered` state where all of the build options are locked down based on the actual packages that were resolved and used during the build.
  - spk publishes the unchanged spec file at the version level for use by the solver (eg `spk/spec/python/2.7.5` is also tagged)
