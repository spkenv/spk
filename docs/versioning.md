---
title: Version Ranges
summary: Version compatibility syntax and semantics.
---

The version range specifiers are largely based on those from Rust's Cargo toolchain ([source](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)). The main difference is the support of package [compatibility specifications](../spec#Compatibility)

#### Default Compatbility

When a version number is specified with none of the operators below, it is assumes to follow the requested packages compatibility specification. In this case the given version number reresents a minimum version required, allowing newer version only if the requested package recognizes it as compatible with the desired minumum.

{{% notice tip %}}
Using the default compatibility is the recommended because it can be specified by the package maintainer.
{{% /notice %}}

#### Caret requirements

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

#### Tilde requirements

Tilde requirements specify a minimal version with some ability to update. If you specify a major, minor, and patch version or only a major and minor version, only patch-level changes are allowed. If you only specify a major version, then minor- and patch-level changes are allowed.

`~1.2.3` is an example of a tilde requirement.

```
~1.2.3  := >=1.2.3, <1.3.0
~1.2    := >=1.2.0, <1.3.0
~1      := >=1.0.0, <2.0.0
```

#### Wildcard requirements

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

#### Comparison requirements

Comparison requirements allow manually specifying a version range or an exact version to depend on.

Here are some examples of comparison requirements:

```
>= 1.2.0
> 1
< 2
= 1.2.3
```

#### Multiple requirements

As shown in the examples above, multiple version requirements can be separated with a comma, e.g., `>= 1.2, < 1.5`.
