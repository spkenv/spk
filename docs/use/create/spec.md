---
title: Name, Version and Compatibility
summary: Spec files are recipes for authorship in spk.
weight: 10
---

> [!TIP]
> The [reference]({{< ref "../../ref/" >}}) pages provide a comprehensive list of fields and values.

### Name and Version

The only required field in a package spec file is the name and version number of the package. This is specified in the top-level `pkg` field. This field specifies the name and version number of the package being defined. We always recommend adding the api version as well, to ensure forward compatibility with spk releases.

```yaml
api: v0/package
pkg: my-package/1.0.0
```

> [!NOTE]
> Package names can only be composed of lowercase ascii letters, digits and dashes (`-`). This is done to try and make sure that packages are easier to find and predict, rather than having a whole bunch of different ways to name them (eg: myPackage, MyPackage, My_Package, my_package, my-package, etc...). This restricted character set also provides a greater freedom for the request specification to be expanded in the future.

### Compatibility

The optional `compat` field of a package specifies the compatibility between versions of this package. The compat field takes a version number, with each digit replaced by one or more characters denoting compatibility (`a` for api compatibility, `b` for binary compatibility and `x` for no compatibility). Multiple characters can be put together if necessary: `x.ab`.

If not specified, the default value for this field is: `x.a.b`. This means that at build time and on the command line, when API compatibility is needed, any minor version of this package can be considered compatible (eg `my-package/1.0.0` could resolve any `my-package/1.*`). When resolving dependencies however, when binary compatibility is needed, only the patch version is considered (eg `my-package/1.0.0` could resolve any `my-package/1.0.*`).

Pre-releases and post-releases of the same version are treated as compatible, however this can be controlled by adding an extra compatibility clause to the `compat` field. For example, `x.x.x-x+x` would mark a build as completely incompatible with any other build, including other pre- or post-releases of the same version.

```yaml
pkg: my-package/1.0.0
compat: x.a.b
# where major versions are not compatible
# minor versions are API-compatible
# patch versions are binary compatible
```

The compat field of the new version is checked before install/update. Because of this, the compat field is more af a contract with past versions rather than future ones. Although it's recommended that your version compatibility remain constant for all versions of a package, this is not strictly required.

### Metadata

Packages can also choose to augment their information with extended metadata. For all available fields, see the [reference]({{< ref "../../ref/" >}}) page.

```yaml
api: v0/package
pkg: my-pkg
meta:
  description: A short summary, avoid long bodies of text
  homepage: https://my-package.test
  license: Apache-2.0
  labels:
    my-studio:department: pipeline
```

Of particular interest is metadata labels, as seen above. These are short key-value pairs that can be used to identify packages. In a studio context, labels can be useful for adding team or information and other identifying data that might help track or further organize large repositories of packages.

> [!TIP]
> As convention, the label names are typically prefixed with some namespace so that there are no collisions with common names. For example, labels added by spk itself will always start with `spk:`, such as automatically converted pip packages (see [importing from pip]({{< ref "../convert" >}})).
