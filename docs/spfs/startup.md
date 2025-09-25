---
title: Shell Startup and Environment
---

##

The `spfs` system uses shell startup files to configure your environment when you run or start a new shell session. These files allow you to set environment variables, define aliases, and run initialization scripts specific to your environment.

{{% notice info %}}
Startup files are built to be extremely lightweight and used sparingly. Overuse or complex scripts can cause unwanted delays in environment startup and command execution.
{{% /notice %}}

## How Startup Files Work

Whenever an spfs environment is executed, either as an interactive shell or direct command, the startup process will first `source` any files that it finds for the current shell within the `/spfs` filesystem.

### Location and Naming

Spfs looks for startup files under the `/spfs/etc/spfs/startup.d` folder. It looks for all files with an extension that matches the current shell, ordering them alphabetically, and then runs `source <filename>`.

| Shell     | Glob Pattern |
| --------- | ------------ |
| sh, bash  | `*.sh`       |
| csh, tcsh | `*.csh`      |
