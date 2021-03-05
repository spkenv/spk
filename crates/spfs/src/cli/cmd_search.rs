use structopt::StructOpt;

use spfs;

#[derive(Debug, StructOpt)]
pub struct CmdSearch {
    #[structopt(value_name = "TERM", about = "The search term/substring to look for")]
    term: String,
}

impl CmdSearch {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let mut repos = Vec::with_capacity(config.remote.len());
        for name in config.list_remote_names() {
            let remote = match config.get_remote(&name) {
                Ok(remote) => remote,
                Err(err) => {
                    tracing::warn!(remote = %name, "failed to load remote repository");
                    tracing::debug!(" > {:?}", err);
                    continue;
                }
            };
            repos.push(remote);
        }
        repos.insert(0, config.get_repository()?.into());
        for repo in repos.into_iter() {
            for tag in repo.iter_tags() {
                let (tag, _) = tag?;
                if tag.to_string().contains(&self.term) {
                    println!("{:?}", tag);
                }
            }
        }
        Ok(())
    }
}
