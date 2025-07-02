---
title: Home
chapter: true
---

<img style="max-width: 200px"
alt="SPK Logo" src="/images/spk_black.png"/>
---

The **S**oftware **P**ackaging **K**it (SPK) provides package management and a software runtime for studio environments.

<div style="text-align: center; width: 100%">{{% button href="./use" %}}Getting Started{{% /button %}} {{% button href="./ref/api/v0/package" %}}Yaml Reference{{% /button %}} {{% button href="./error_codes" %}}Error Codes{{% /button %}} {{% button href="https://github.com/spkenv/spk" %}}<span class="fa-brands fa-github"></span> GitHub{{% /button %}} {{% button href="https://join.slack.com/t/spk-dev/shared_invite/zt-2o840nwp1-MjC2xyLpBqbdXXWdDwWGmQ" %}}<span class="fa-brands fa-slack"></span> Slack{{% /button %}}
</div>

Driven by the unique requirements of the film, vfx, and animation industries, SPK has a few primary goals:

- Package Compatibility Beyond Version Numbers
- Recipe and Source Publication
- Fast, Dynamic Build and Runtime Environments
- Reliable and Natural Definition of Platforms and Constraints
- _More details on these goals can be found [here]({{< ref "./develop/design" >}})_

Additionally, SPK is built on top of a technology called SPFS, which lends a few superpowers to the whole system:

- Per-process, isolated software runtimes
- A single, consistent file path for all software at runtime
- File-level de-duplication of package data
- Efficient sync, transfer and localization of software
- _More about spfs [here]({{< ref "./spfs" >}})_
