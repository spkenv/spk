use structopt::StructOpt;

use spfs::{self, prelude::*};

#[derive(Debug, StructOpt)]
pub struct CmdLs {
    #[structopt(
        value_name = "REF",
        about = "The tag or digest of the file tree to read from"
    )]
    reference: String,
    #[structopt(
        default_value = "/",
        about = "The subdirectory to list, defaults to the root ('/spfs')"
    )]
    path: String,
}

impl CmdLs {
    pub async fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let repo: RepositoryHandle = config.get_repository()?.into();
        let item = repo.read_ref(self.reference.as_str())?;

        let path = relative_path::RelativePathBuf::from(&self.path);
        let path = path.strip_prefix("/spfs").unwrap_or_else(|_| path.as_ref());
        let manifest = spfs::compute_object_manifest(item, &repo)?;
        if let Some(entries) = manifest.list_dir(path.as_str()) {
            for name in entries {
                println!("{}", name);
            }
        } else {
            tracing::error!("file not found in manifest");
            std::process::exit(1);
        }
        Ok(())
    }
}
