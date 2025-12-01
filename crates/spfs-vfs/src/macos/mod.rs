// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS-specific virtual filesystem implementation using macFUSE
//!
//! This module provides SPFS filesystem support on macOS using the macFUSE
//! kernel extension. Unlike Linux which uses mount namespaces for isolation,
//! macOS uses a WinFSP-style router that maps process IDs to filesystem views.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────┐
//! │   spfs-fuse-macos       │
//! │   service               │
//! │   ┌─────────────────┐   │
//! │   │ macFUSE mount   │   │
//! │   │ at /spfs        │   │
//! │   └────────┬────────┘   │
//! │            │            │
//! │   ┌────────▼────────┐   │
//! │   │     Router      │   │
//! │   │  PID → Mount    │   │
//! │   └────────┬────────┘   │
//! │            │            │
//! │   ┌────────▼────────┐   │
//! │   │  gRPC Service   │   │
//! │   │  (tonic)        │   │
//! │   └─────────────────┘   │
//! └─────────────────────────┘
//! ```

pub mod handle;
pub mod mount;
pub mod process;
pub mod router;
pub mod scratch;

pub use handle::Handle;
pub use mount::Mount;
pub use process::{ProcessError, get_parent_pid, get_parent_pids_macos, is_in_process_tree};
pub use router::Router;
pub use scratch::{ScratchDir, ScratchError};

mod config;
mod service;

pub use config::Config;
pub use service::Service;
