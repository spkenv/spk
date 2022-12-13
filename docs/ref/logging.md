---
title: Logging and Verbosity
summary: Logging configuration and interaction with Verbosity
weight: 110
---

This document explains the verbosity levels and logging controls in spk.

## Verbosity

Spk commands have a `--verbose (-v)` flag that can be specified multiple times. Increasing the number of times `-v` is specified (e.g. `spk env -vvv my-package`) increases the amount of output and details spk produces, particularly during a solve.

The number of times `--verbose (-v)` is specified is the verbosity level. Verbosity levels also enable logging messages in spk and underlying libraries. Here are the current verbosity levels and what they enable:

| Verbosity | Log levels and modules         | Shows ...                                                                     |
| --------- | ------------------------------ | ----------------------------------------------------------------------------- |
| 0         | `error,spk=info,spfs=warn`     |                                                                               |
| 1         | `error,spk=debug,spfs=info`    | When the Solver resolves a Package, or takes a Step Back                      |
|           |                                | The Packages in the Solution (Installed Packages list)                        |
|           |                                | What requested each Package in the Solution (requested by)                    |
|           |                                | Fields in a Request that have changed from the default values                 |
| 2         | `error,spk=trace,spfs=debug`   | The Options of each Package in the Solution                                   |
|           |                                | When the Solver adds a Request, and the reasons for its decisions (`TRY ...`) |
| 3         | `error,spk=trace,spfs=trace`   | Log target names in log messages                                              |
|           |                                | When the Solver sets an Option                                                |
|           |                                | Solver search tree levels as numbers for levels above 5                       |
| 4         |                                |                                                                               |
| 5         |                                | All fields in a request regardless of value                                   |
| 6         |                                | Each State's resolved Packages and package Requests for each Solver step      |
| 7         |                                |                                                                               |
| 8         |                                |                                                                               |
| 9         |                                |                                                                               |
| 10        |                                | Each State's variable Requests and resolved Options for each Solver step      |


## Logging Controls

Logging output is primarily controlled by verbosity level, see above. But it can be overridden by the `SPK_LOG` and `RUST_LOG` enviroment variables. For spk commands, the settings based on verbosity are applied first, then those from `SPK_LOG`, and finally any from `RUST_LOG`. For spfs commands, only `RUST_LOG` is used.

`SPK_LOG` and `RUST_LOG` can contain comma-separated list of tracing directives. Each directive can be a `log-level` or `target=log-level`, e.g. `debug` or `spk=debug`. A log-level on its own sets the default logging level. The target names can be any module or crate name in spk or its dependencies, or any of the additional names defined in spk.

These additional target names are defined in spk:
- `build_sort` - for build key-generation and sorting
- `spk_solve::impossible_checks` - for impossible request checking


Examples of setting `SPK_LOG`:
- `env SPK_LOG="spk_solve::impossible_checks=debug" spk explain my-package` will turn on the impossible request checks debug messages

- `env SPK_LOG="build_sort=debug" spk explain my-package` will turn on build sorting debug messages


## Sentry Logging

Sentry logging integration is only enabled when `spk` has been compiled with the `sentry` feature enabled.


