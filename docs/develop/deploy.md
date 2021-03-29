---
title: Deploy and Release
summary: How to make new releases of spk and deploy to production
weight: 60
---

### Spdev Releases

New releases of spk are created through spdev. When ready, update the release notes and version number in `.spdev.yaml` and merge the changes into master. The created pipeline will have a manual job which releases the new version and deploys it.

`spk` is installed onto workstations using the rpm package that is created from the build process. Upon deployment, spdev places this rpm into the `spi-testing` repository in artifactory, which will automatically trigger all dev-configured workstations (including the gitlab runners) to install the new version.

### Updating Production

Once tested and stable, the `spk` package must be copied from the `rhel76-testing` into the `rhel-76` and `rhel-79` repositories in artifactory. This can be done by searching for the spk package in the artifactory UI, navigating to the relevant artifact, and selecting `copy` from the `Actions` dropdown.

![search Artifactory](../artifactory_search.png)
![show atifact in tree](../artifactory_show.png)
![copy artifact](../artifactory_copy.png)
