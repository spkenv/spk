#[macro_use]
mod args;
mod cmd_run;

use structopt::StructOpt;

use cmd_run::CmdRun;

main!(CmdRun);
