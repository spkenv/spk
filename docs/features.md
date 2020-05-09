---
title: Main Features
---

* Fast like rez - on the fly environments by collection of layers
* Allow multiple versions of a package - via layering (works for versioned so)
* Build packages on demand - via source packages and dependency graph
* Represent build options in resolution - for rebuild, debug packages, testing etc
- Support pre and post release packages and understand what that means
- Support for variants
- Support 'provided' packages when something like maya has bundled libraries
- Support conda channels idea - via spfs repos or internal tag path?
- weak refs?
- host instlls?
- track usage or metrics... what can be pruned - with source packages binaries can age out, even just older minor version numbers that are n ot being pointed too after the new ones become stable (spcomp2)
- nice to know when you are using older versions - get notifications about newer published versions
- deprecate packages, be able to warn or track at publish time - time based, build waring becomes build failure
