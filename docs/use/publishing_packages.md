---
title: Publishing Packages
summary: Publish locally built packages for others to use.
weight: 20
---

Packages built with `spk build` are only installed to your configured
local storage path via the root storage key in the SPFS
configuration file:

```angular2html
[storage]
root = "~/.local/spfs"
```

In order to be accessible by other members of your team, you need to
publish your packages. This can be done via the `spk publish` command:

```console
$ spk publish my-pkg/0.1.0
```

This will copy an already built local package called `my-pkg` to
the configured `remote.origin` address in the SPFS configuration
file:

```angular2html
[remote.origin]
address = "file:/tmp/spfs-origin"
```

This address can either be a local filesystem path, a NFS path, or
a SPFS server instance.

I'm not sure if the `spk publish` command does anything else. 
Need help here.
