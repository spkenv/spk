use structopt::StructOpt;

use spfs::{self, prelude::*};

#[derive(Debug, StructOpt)]
pub struct CmdRead {
    #[structopt(
        value_name = "REF",
        about = "The tag or digest of the blob/payload to output"
    )]
    reference: String,
    #[structopt(
        value_name = "PATH",
        about = "If the given ref is not a blob, read the blob found at this path"
    )]
    path: Option<String>,
}

impl CmdRead {
    pub fn run(&mut self, config: &spfs::Config) -> spfs::Result<()> {
        let repo: RepositoryHandle = config.get_repository()?.into();
        let item = repo.read_ref(self.reference.as_str())?;
        use spfs::graph::Object;
        let blob = match item {
            Object::Blob(blob) => blob,
            _ => {
                let path = match &self.path {
                    None => {
                        return Err(
                            format!("PATH must be given to read from {:?}", item.kind()).into()
                        )
                    }
                    Some(p) => p.strip_prefix("/spfs").unwrap_or(&p).to_string(),
                };
                let manifest = spfs::compute_object_manifest(item, &repo)?;
                let entry = match manifest.get_path(&path) {
                    Some(e) => e,
                    None => {
                        tracing::error!("file does not exist: {}", path);
                        std::process::exit(1);
                    }
                };
                if !entry.kind.is_blob() {
                    tracing::error!("path is a directory or masked file: {}", path);
                    std::process::exit(1);
                }
                repo.read_blob(&entry.object)?
            }
        };

        let mut payload = repo.open_payload(&blob.digest())?;
        std::io::copy(&mut payload, &mut std::io::stdout())?;
        Ok(())
    }
}
