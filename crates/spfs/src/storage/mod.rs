mod blob;
mod layer;
mod manifest;
mod payload;
mod platform;
mod repository;
mod tag;

pub mod fs;
pub mod prelude;
pub mod tar;

pub use blob::BlobStorage;
pub use layer::LayerStorage;
pub use manifest::{ManifestStorage, ManifestViewer};
pub use payload::PayloadStorage;
pub use platform::PlatformStorage;
pub use repository::Repository;
pub use tag::TagStorage;

#[derive(Debug)]
pub enum RepositoryHandle {
    FS(fs::FSRepository),
    //Tar(tar::TarRepository),
}

impl RepositoryHandle {
    pub fn to_repo(self) -> Box<dyn Repository> {
        match self {
            Self::FS(repo) => Box::new(repo),
        }
    }
}

impl std::ops::Deref for RepositoryHandle {
    type Target = dyn Repository;

    fn deref(&self) -> &Self::Target {
        match self {
            RepositoryHandle::FS(fs) => fs,
        }
    }
}

impl std::ops::DerefMut for RepositoryHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RepositoryHandle::FS(fs) => fs,
        }
    }
}

impl From<fs::FSRepository> for RepositoryHandle {
    fn from(repo: fs::FSRepository) -> Self {
        RepositoryHandle::FS(repo)
    }
}

pub fn open_repository<S: AsRef<str>>(address: S) -> crate::Result<RepositoryHandle> {
    use url::Url;

    let url = match Url::parse(address.as_ref()) {
        Ok(url) => url,
        Err(err) => return Err(format!("invalid repository url: {:?}", err).into()),
    };

    match url.scheme() {
        _ => todo!("open_repository"),
    }
}
