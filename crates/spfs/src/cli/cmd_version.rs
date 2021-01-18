use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdVersion {}

impl CmdVersion {
    pub fn run(&self) -> spfs::Result<()> {
        println!("{}", spfs::VERSION);
        Ok(())
    }
}
