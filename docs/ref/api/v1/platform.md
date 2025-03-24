---
title: platform
summary: Detailed platform specification information
---

This document details each data structure and field that does or can exist within a platform spec file for spk.

## Platform Spec

The root package spec defines which fields can and should exist at the top level of a spec file.

| Field        | Type                                                         | Description                                                                                                                                           |
| ------------ | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| platform     | _[Identifier]({{< ref "../v0/package#identifier" >}})_       | The name and version number of this platform                                                                                                          |
| meta         | [Meta]({{< ref "../v0/package#meta" >}})                     | Extra package metadata such as description, license, etc                                                                                              |
| compat       | _[Compat]({{< ref "../v0/package#compat" >}})_               | The compatibility semantics of this packages versioning scheme                                                                                        |
| deprecated   | _boolean_                                                    | True if this package has been deprecated, this is usually reserved for internal use only and should not generally be specified directly in spec files |
| base         | _List[[Identifier]({{< ref "../v0/package#identifier" >}})]_ | (Optional) Base packages to inherit requirements from                                                                                                 |
| requirements | _List[[Requirement](#requirement)]_                          | The set of requirements for this platform                                                                                                             |

## Requirement

Each platform requirement names a package and the constraints for that package in downstream environments.

Like an [Identifier](#identifier) but with a version range rather than an exact version, see [versioning]({{< ref "../../../use/versioning" >}}).

| Field     | Type                                                                              | Description                                                                                                                                                                                                                              |
| --------- | --------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg       | _[Identifier]({{< ref "../v0/package#identifier" >}})_                            | The package request to add to this platform, often with just a name                                                                                                                                                                      |
| atBuild   | _[VersionRange]({{< ref "../../../use/versioning#version-ranges" >}}) or `false`_ | The restriction to apply to this package when the platform is being used in a downstream build environment (when a package is being built). If `false` it removes any inherited restriction for downstream build environments.           |
| atRuntime | _[VersionRange]({{< ref "../../../use/versioning#version-ranges" >}}) or `false`_ | The restriction to apply to this package when the platform is being used in a downstream runtime environment (when a package is _not_ being built). If `false` it removes any inherited restriction for downstream runtime environments. |
