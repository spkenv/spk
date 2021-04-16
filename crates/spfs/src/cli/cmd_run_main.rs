// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

#[macro_use]
mod args;
mod cmd_run;

use structopt::StructOpt;

use cmd_run::CmdRun;

main!(CmdRun);
