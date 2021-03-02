//! Handles the setup and initialization of runtime environments

mod csh_exp;
mod overlayfs;
mod startup_csh;
mod startup_sh;
mod storage;

pub use overlayfs::is_removed_entry;
pub use storage::{makedirs_with_perms, Config, Runtime, Storage, STARTUP_FILES_LOCATION};
