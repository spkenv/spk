---
title: Workspaces
summary: Group recipe spec files so spk can find them by name, version, or path.
weight: 80
---

A _workspace_ is an optional way to tell spk about a collection of recipe spec
files that live together in a directory tree. When a workspace is defined, spk
commands can locate a recipe by package name or version rather than requiring
the exact path to its `*.spk.yaml` file.

Workspaces are entirely optional. When no workspace file is present, spk falls
back to looking for spec files in the current directory, which is the default
behaviour for a single-package checkout.

### The Workspace File

A workspace is declared by a `workspace.spk.yaml` file at the root of the
directory tree. spk discovers it by searching the current directory and its
parents, so commands run from any subdirectory share the same workspace.

The `recipes` field lists one or more glob patterns identifying the spec files
that belong to the workspace:

```yaml
api: v0/workspace

recipes:
  # collect every recipe under the packages directory
  - packages/**/*.spk.yaml
```

Once the workspace is defined, a recipe can be referenced by its package name
from anywhere within the tree:

```sh
spk build cmake        # finds packages/cmake/cmake.spk.yaml
spk build python       # finds packages/python/python3.spk.yaml
```

### Recipes That Produce Many Versions

A single recipe spec is often reused to build many versions of a package by
templating the version from a build option (see
[Spec Variables and Templating]({{< ref "create/template" >}})):

```yaml
# {% set opt = opt | default_opts(version="3.7.3") %}
pkg: python/{{ opt.version }}
```

Because the version isn't hard-coded in the file, the workspace needs to be
told which versions such a recipe is allowed to produce. This is done by
providing a mapping form of the `recipes` entry with a `versions` list. The
`versions` values support bash-style brace expansion so that ranges can be
written compactly:

```yaml
recipes:
  - packages/**/*.spk.yaml

  # augment a recipe that was already collected above with the
  # specific versions it can build
  - path: packages/python/python2.spk.yaml
    versions: [2.7.18]

  - path: packages/python/python3.spk.yaml
    versions:
      - '3.7.{0..17}'
      - '3.8.{0..20}'
      - '3.9.{0..21}'
      - '3.10.{0..16}'
      - '3.11.{0..11}'
      - '3.12.{0..8}'
      - '3.13.{0..1}'
```

With the versions declared, a request that includes a version selects the
matching recipe:

```sh
spk build python/3.11   # rendered from packages/python/python3.spk.yaml
```

A recipe that hard-codes its own version, or that should accept any version,
does not need to declare `versions`.
