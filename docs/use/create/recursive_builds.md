---
title: Recursive Builds
summary: Packages that require themselves as input.
weight: 70
---

By default, builds will fail if another version of the package being built ends up in the build environment, either as a direct or indirect dependency. There are packages, however, that bootstrap their own build process and require this (for example: compilers like gcc or package systems like pip). Furthermore, these recursive builds often perform an in-place upgrade, writing over some or all the previous versions files which is typically not allowed.

The [validation](#validation) rule `RecursiveBuild` can be used to reconfigure the validation process for these scenarios:

```yaml
build:
  validation:
    rules:
      - allow: RecursiveBuild
```
