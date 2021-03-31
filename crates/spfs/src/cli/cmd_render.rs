use structopt::StructOpt;

use spfs;
use spfs::prelude::*;

#[derive(Debug, StructOpt)]
pub struct CmdRender {
    #[structopt(
        long = "allow-existing",
        help = "Allow re-rendering when the target directory is not empty"
    )]
    allow_existing: bool,
    #[structopt(help = "The tag or digest of what to render, use a '+' to join multiple layers")]
    reference: String,
    #[structopt(help = "The path to render the manifest into")]
    target: std::path::PathBuf,
}

impl CmdRender {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<i32> {
        let env_spec = spfs::tracking::EnvSpec::new(&self.reference)?;
        let repo = config.get_repository()?;

        for target in &env_spec.items {
            let target = target.to_string();
            if !repo.has_ref(target.as_str()) {
                tracing::info!(reference = ?target, "pulling target ref");
                spfs::pull_ref(target.as_str())?;
            }
        }

        std::fs::create_dir_all(&self.target)?;
        let target_dir = self.target.canonicalize()?;
        for _ in std::fs::read_dir(&target_dir)? {
            if self.allow_existing {
                break;
            }
            return Err(format!("Directory is not empty {}", target_dir.display()).into());
        }
        tracing::info!("rendering into {}", target_dir.display());
        spfs::render_into_directory(&env_spec, &target_dir)?;
        tracing::info!("successfully rendered {}", target_dir.display());
        Ok(0)
    }
}
