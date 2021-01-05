mod blob;
mod layer;
mod manifest;
mod payload;
mod platform;
mod repository;
mod tag;

pub mod fs;
//pub mod tar;

pub use blob::BlobStorage;
pub use layer::LayerStorage;
pub use manifest::{ManifestStorage, ManifestViewer};
pub use payload::PayloadStorage;
pub use platform::PlatformStorage;
pub use repository::Repository;
pub use tag::TagStorage;

pub enum RepositoryHandle {
    //FS(fs::FSRepository),
//Tar(tar::TarRepository),
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
