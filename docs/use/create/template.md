---
title: Spec Variables and Templating
summary: A framework to validate various aspects of your package.
weight: 60
---

SPK package spec files also supports the `jinja2` templating language via the [tera library in Rust](https://keats.github.io/tera/docs/#templates), so long as the spec file remains valid yaml. This means that often, templating logic is best placed into yaml comments, with some examples below.

The templating is rendered when the yaml file is read from disk, and before it's processed any further (to start a build, run tests, etc.). This means that it cannot, for example, be involved in rendering different specs for different variants of the package (unless you define and orchestrate those variants through a separate build system).

The data that's made available to the template takes the form:

```yaml
spk:
  version: "0.23.0" # the version of spk being used
opt: {} # a map of all build options specified (either host options or at the command line)
env: {} # a map of the current environment variables from the caller
```

One common templating use case is to allow your package spec to be reused to build many different versions, for example:

```yaml
# {% set opt = opt | default_opts(version="2.3.4") %}
pkg: my-package/{{ opt.version }}
```

Which could then be invoked for different versions at the command line:

```sh
spk build my-package.spk.yaml                  # builds the default 2.3.4
spk build my-package.spk.yaml -o version=2.4.0 # builds 2.4.0
```

### Template Extensions

In addition to the [default functions and filters](https://keats.github.io/tera/docs/#built-ins) within the tera library, spk provides a few additional ones to help package maintainers:

#### Filters

**default_opts**

The `default_opts` filter can be used to more easily declare default values for package options that can be overridden on the command line. The following two blocks are equivalent:

```jinja
{% set opt.foo = opt.foo | default(value="foo") %}
{% set opt.bar = opt.bar | default(value="bar") %}

{% set opt = opt | default_opts(foo="foo", bar="bar") %}
```

An additional benefit of the second block is that the names of options and their values will be validated using the spk library. Either approach is valid, depending on the use case and preferences.

**compare_version**

The `compare_version` allows for comparing spk versions using any of the [version comparison operators]({{< ref "../versioning" >}}). It takes one or two arguments, depending on the data that you have to give. In all cases, the arguments are concatenated together and parsed as a version range. For example, the following assignments to py_3 all end up checking the same statement.

```jinja
{% set python_version = "3.10" %}
{% set is_py3 = python_version | compare_version(op=">=3") %}
{% set is_py3 = python_version | compare_version(op=">=", rhs=3) %}
{% set three = 3 %}
{% set is_py3 = python_version | compare_version(op=">=", rhs=three) %}
```

**parse_version**

The `parse_version` filter breaks down an spk version string into its components, either returning an object or a single field from it, for example:

```jinja
{% assign v = "1.2.3.4-alpha.0+r.4" | parse_version %}
{{ v.base }}      # 1.2.3.4
{{ v.major }}     # 1
{{ v.minor }}     # 2
{{ v.patch }}     # 3
{{ v.parts[3] }}  # 4
{{ v.post.r }}    # 4
{{ v.pre.alpha }} # 0
{{ "1.2.3.4-alpha.0+r.4" | parse_version(field="minor") }} # 2
```

**replace_regex**

The `replace_regex` filter works like the built-in `replace` filter, except that it matches using a perl-style regular expression and allows group replacement in the output. These regular expressions do not support look-arounds or back-references. For example:

```jinja
{% set version = opt.version | default("2.3.4") %}
{% set major_minor = version | replace_regex(from="(\d+)\.(\d+).*", to="$1.$2") %}
{{ major_minor }} # 2.3
```

