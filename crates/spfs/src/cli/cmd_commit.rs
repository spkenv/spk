use structopt::StructOpt;

use spfs::{encoding::Encodable, storage::TagStorage};

#[derive(Debug, StructOpt)]
pub struct CmdCommit {
    #[structopt(
        long = "tag",
        short = "t",
        about = "Can be given many times: human-readable tags to update with the resulting object"
    )]
    tags: Vec<String>,
    #[structopt(
        possible_values = &["layer", "platform"],
        about = "The desired object type to create"
    )]
    kind: String,
}

impl CmdCommit {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let mut runtime = spfs::active_runtime()?;

        if !runtime.is_editable() {
            tracing::error!("Active runtime is not editable, nothing to commmit");
            std::process::exit(1)
        }

        let mut repo = config.get_repository()?;

        let result: spfs::graph::Object = match self.kind.as_str() {
            "layer" => spfs::commit_layer(&mut runtime)?.into(),
            "platform" => spfs::commit_platform(&mut runtime)?.into(),
            _ => {
                tracing::error!("cannot commit {}", self.kind);
                std::process::exit(1);
            }
        };

        tracing::info!(digest = ?result.digest()?, "created");
        for tag in self.tags.iter() {
            let tag_spec = match spfs::tracking::TagSpec::parse(tag) {
                Ok(tag_spec) => tag_spec,
                Err(err) => {
                    tracing::warn!("cannot set invalid tag '{}': {:?}", tag, err);
                    continue;
                }
            };
            repo.push_tag(&tag_spec, &result.digest()?)?;
            tracing::info!(tag = ?tag, "created");
        }

        tracing::info!("edit mode disabled");
        Ok(())
    }
}
