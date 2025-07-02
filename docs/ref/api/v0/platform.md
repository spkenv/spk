---
title: platform
summary: Detailed platform specification information
---

This document details each data structure and field that does or can exist within a platform spec file for spk.

## Platform Spec

The root package spec defines which fields can and should exist at the top level of a spec file.

| Field        | Type                                               | Description                                                                                                                                           |
| ------------ | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| platform     | _[Identifier]({{< ref "./package#identifier" >}})_ | The name and version number of this platform                                                                                                          |
| meta         | [Meta]({{< ref "./package#meta" >}})               | Extra package metadata such as description, license, etc                                                                                              |
| compat       | _[Compat]({{< ref "./package#compat" >}})_         | The compatibility semantics of this packages versioning scheme                                                                                        |
| deprecated   | _boolean_                                          | True if this package has been deprecated, this is usually reserved for internal use only and should not generally be specified directly in spec files |
| base         | _[Identifier]({{< ref "./package#identifier" >}})_ | (Optional) Base package to inherit requirements from                                                                                                  |
| requirements | _List[[Requirement](#requirement)]_                | The set of requirements for this platform                                                                                                             |

## Requirement

Each requirement in a platform can be either: a simple package [Identifier]({{< ref "./package#identifier" >}}) or a [RequirementPatch](#requirementpatch).

### RequirementPatch

Each patch is expected to have only one of the following fields:

| Field  | Type                                               | Description                                      |
| ------ | -------------------------------------------------- | ------------------------------------------------ |
| add    | _[Identifier]({{< ref "./package#identifier" >}})_ | The package request to add to this platform      |
| remove | _[Identifier]({{< ref "./package#identifier" >}})_ | The package request to remove from this platform |
