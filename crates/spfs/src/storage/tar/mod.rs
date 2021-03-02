//! An spfs storage implementation where all data is unpacked and repacked
//! into a tar archive on disk

mod repository;
pub use repository::TarRepository;
