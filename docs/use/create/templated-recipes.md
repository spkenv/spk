---
title: Templated Recipes & Version Discovery
summary: Create reusable package recipes that can build multiple software versions.
weight: 15
---

For many packages, especially foundational ones like `python` or `gcc`, it is inefficient to create and maintain a separate spec file for every single patch release. SPK provides a powerful solution to this problem: **Templated Recipes**.

A single templated recipe can define the build process for an entire family of versions (e.g., all Python 3.9 releases). It works by pairing a generic build script with a set of rules for discovering which versions are available from an upstream source, like a git repository.

### Becoming a Template: The `template` Metadata Block

A standard package spec file becomes a template by including a top-level `template` block. This block contains metadata that is used by the SPK templating engine. It is not part of the final, rendered recipe.

Here is an example of a recipe for `python` that has been converted into a template:

```yaml
# In packages/python/python3.spk.yaml

# This 'template' block is metadata for the build system.
# It signifies that this file is a template, not a
# direct recipe.
template:
  # Declares that this template is used for building the
  # package named 'python'.
  for: python
  # Defines how to discover the supported versions.
  versions:
    discover:
      # Strategy 1: Check a git repository's tags.
      git_tags:
        url: https://github.com/python/cpython.git
        # A pattern to match against the tags.
        match: v3.9.*
        # An optional regex to extract the clean version number from the tag.
        # This would turn "v3.9.22" into "3.9.22".
        extract: 'v(.*)'

# --- The actual recipe template begins below ---
# The build system will process the rest of this file using
# a jinja2 template engine, injecting the discovered version.
# The 'template' block above will be stripped before this stage.

api: v0/package
pkg: python/{{ version }}
build:
  # The build script can now use the {{ version }} variable,
  # which will be populated by the templating engine.
  script:
    - ./configure --version={{ version }}
    # ... etc
```

### The `template` Block In Detail

- `for`: A **required** string that links the template to a package name. When a platform requests to build `python`, SPK knows to look for a template with `for: python`.
- `versions`: An object that defines how the list of buildable versions is generated.
- `versions.discover`: This key tells SPK to look for versions dynamically from an external source.
- `versions.discover.git_tags`: This specifies the "git tag" discovery strategy.
    - `url`: The URL of the git repository to query.
    - `match`: A glob-style pattern to filter the tags.
    - `extract`: An optional regular expression with a capture group to extract the desired version string from the full tag name. If omitted, the full tag name is used.

### The Workflow in Action

This system makes adding new package versions effortless:

1.  A new version, `3.9.22`, is released, and a `v3.9.22` tag is pushed to the CPython git repository. **No changes are needed in your spk repository.**
2.  A developer wants to use it. They simply request it in a platform or on the command line.
3.  SPK finds the recipe template for `python`.
4.  It runs the `discover` logic, querying the git repository for tags matching `v3.9.*`. It finds `v3.9.22`.
5.  The request is now validated against this dynamically generated list of supported versions.
6.  SPK then renders the template in-memory, replacing `{{ version }}` with `3.9.22`, and proceeds with the build.

This "set it and forget it" approach removes the manual and error-prone step of updating a central allow-list of versions, and keeps the versioning logic tightly coupled with the recipe that uses it.
