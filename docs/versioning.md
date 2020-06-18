---
title: Versioning
summary: Version compatibility syntax and semantics.
weight: 10
---
### Version Numbers

Version numbers in spk are made up of at least three dot-separated digits (eg: `1.2.3`), but can have as many digits as they want (eg: `1.2.3.4.5.6`...). In all cases, when you specify a version number with less than three digits, the others are assumed to be zero (eg: `1.1` == `1.1.0`).

#### Pre and Post Release Tags

Version numbers can have optional release tags appended to the end, which denote pre and post releases.

Pre-releases are understood to come before the normal release of the same number, and are not considered when resolving packages unless specifically requested. The `-` symbol preceeds pre-release tags. Each tag is made up of a name and single integer version (eg `name.0`). Multiple of these tags can be included, separated by a comma.

```
1.0.0-pre.1
1.2.2-alpha.0
25.0.8-alpha.0,test.1
```

Post-releases come after the normal release of the same number, and must come after and pre-release tags if both are specified.

```
1.0.0+rev.1
1.2.0+post.2,release.1
2.6.8-alpha.0+patch.6
```

##### Sorting of Release Tags

- A pre-release version will always be less than the same version number with no tags
- A post-release version will always be greater than the same verison number with no tags
- All release tags are sorted alphabetically, and by number

```
   1.0.0-alpha.1  <  1.0.0
   1.0.0-alpha.2  <  1.0.0-alpha.3
             6.3  <  6.3+post.0
         6.3+a.0  <  6.3+b.0
6.3-pre.0+post.1  <  6.3-pre.0+post.2
6.3-pre.0+post.1  <  6.3-pre.1+post.0
```

### Version Ranges

The version range specifiers are largely based on those from Rust's Cargo toolchain ([source](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)). The main difference is the support of package [compatibility specifications](../spec#Compatibility)

#### Default Compatbility

When a version number is specified with none of the operators below, it is assumes to follow the requested packages compatibility specification. In this case the given version number reresents a minimum version required, allowing newer version only if the requested package recognizes it as compatible with the desired minumum.

{{% notice tip %}}
Using the default compatibility is the recommended because it can be specified by the package maintainer.
{{% /notice %}}

#### Caret Requirements

Caret requirements allow SemVer compatible updates to a specified version. An update is allowed if the new version number does not modify the left-most non-zero digit in the major, minor, patch grouping.

Here are some more examples of caret requirements and the versions that would be allowed with them:

```
^1.2.3  :=  >=1.2.3, <2.0.0
^1.2    :=  >=1.2.0, <2.0.0
^1      :=  >=1.0.0, <2.0.0
^0.2.3  :=  >=0.2.3, <0.3.0
^0.2    :=  >=0.2.0, <0.3.0
^0.0.3  :=  >=0.0.3, <0.0.4
^0.0    :=  >=0.0.0, <0.1.0
^0      :=  >=0.0.0, <1.0.0
```

This compatibility convention is different from SemVer in the way it treats versions before 1.0.0. While SemVer says there is no compatibility before 1.0.0, we consider 0.x.y to be compatible with 0.x.z, where y ≥ z and x > 0.

#### Tilde Requirements

Tilde requirements specify a minimal version with some ability to update. If you specify a major, minor, and patch version or only a major and minor version, only patch-level changes are allowed. If you only specify a major version, then minor- and patch-level changes are allowed.

`~1.2.3` is an example of a tilde requirement.

```
~1.2.3  := >=1.2.3, <1.3.0
~1.2    := >=1.2.0, <1.3.0
~1      := >=1.0.0, <2.0.0
```

#### Wildcard Requirements

Wildcard requirements allow for any version where the wildcard is positioned.

`*`, `1.*` and `1.2.*` are examples of wildcard requirements.

```
*     := >=0.0.0
1.*   := >=1.0.0, <2.0.0
1.2.* := >=1.2.0, <1.3.0
```

{{% notice tip %}}
Although the `*` range is convenient, it is also unstable and may slow down your solve.
{{% /notice %}}

#### Comparison Requirements

Comparison requirements allow manually specifying a version range or an exact version to depend on.

Here are some examples of comparison requirements:

```
>= 1.2.0
> 1
< 2
= 1.2.3
```

#### Multiple Requirements

As shown in the examples above, multiple version requirements can be separated with a comma, e.g., `>= 1.2, < 1.5`.
