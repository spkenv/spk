use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdRuntimes {}

impl CmdRuntimes {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let runtime_storage = config.get_runtime_storage()?;
        for runtime in runtime_storage.iter_runtimes() {
            let runtime = runtime?;
            println!("{}", runtime.reference().to_string_lossy());
        }
        Ok(())
    }
}
