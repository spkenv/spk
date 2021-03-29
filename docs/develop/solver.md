---
title: The SPK Solver
summary: The brain of spk
weight: 30
---


The solver, and solve process are made up of a combination of core pieces, each responsible from some portion of the solve

{{< mermaid >}}
graph LR;

solution --> solver
solver --> validation
solver --> graf[graph]
graf --> state
graf --> decision
solver --> package_iterator
{{< /mermaid >}}

### Graph Structure

At it's core, the solver operates over a graph structure where each `Node` represents an exact state of the resolve. Each edge in the graph is a `Decision` made by the solver which modifies the state. The solver identifies each state by a hash so that it can identify when any two decisions would lead to the same result.

The solver state is composed of the current unresolved package requests, variable requests, resolved packages, and build options. In addition to that state, each node also holds a set of package iterators.

### Resolve Process and Package Iterators

As the solver progresses, it does so in discrete steps. During each step, it looks at the next unresolved package request, and tries to find the best version to resolve it with. Starting with the latest version, spk walks downwards until it finds a version that works. This walking is done over a `PackageIterator` which combine the available packages from many repositories and are able to remember where they are in the list.

The state of these iterators is often saved and cloned, which is important behaviour to the solver. If the solver resolves a package, it will continue resolving more packages trying to reach a complete solution. If it cannot, it will undo some of its previous decisions and try again with different package versions or builds. Maintaining the state of these iterators is important so that it can return to an old node/state and continue with the next viable option.

### Compatibility and Validation

At each step, in order to determine if a package build or version is viable the solver must validate it. There are a number of pluggable validators, each of which take a solver `State` and validate if a package spec should be allowed. Validators are intended to only check one thing each, with the default set achieving the expected behaviour of the solver together.

### Solutions and Source Packages

If successful, the solver will generate a `Solution` object. Primarily, the solution contains a set of compatible, resolved packages. Each resolved package will also be attached to a `PackageSource` where it can be loaded. Usually, the source of a package is a package repository, but when allowed, can also simply be a package `Spec`. If the source of a package is a spec it denotes that the solver wants the package to be rebuilt from source.

The logic is slightly different for determining if a source package is allowed in a solution. In these cases, the solver will resolve the build environment for the package, and generate a new filled in build spec for the package build that would be created. In these cases, then, the build environment must be resolvable and the generated build spec must pass validation.
