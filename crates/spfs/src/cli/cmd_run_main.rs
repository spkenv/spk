// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use clap::Parser;

#[macro_use]
mod args;
mod cmd_run;

use cmd_run::CmdRun;

main!(CmdRun);
