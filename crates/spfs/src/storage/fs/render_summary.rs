// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use super::render_reporter::RenderBlobResult;
use super::RenderReporter;

/// Statistics on the file copying and hard linking performed during a render.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RenderSummary {
    pub entry_count: AtomicUsize,
    pub already_existed_count: AtomicUsize,
    pub copy_count: AtomicUsize,
    pub copy_link_limit_count: AtomicUsize,
    pub copy_wrong_mode_count: AtomicUsize,
    pub copy_wrong_owner_count: AtomicUsize,
    pub link_count: AtomicUsize,
    pub symlink_count: AtomicUsize,

    pub total_bytes_rendered: AtomicUsize,
    pub total_bytes_already_existed: AtomicUsize,
    pub total_bytes_copied: AtomicUsize,
    pub total_bytes_copied_link_limit: AtomicUsize,
    pub total_bytes_copied_wrong_mode: AtomicUsize,
    pub total_bytes_copied_wrong_owner: AtomicUsize,
    pub total_bytes_linked: AtomicUsize,
}

/// Associate a `RenderBlobResult` with the size of the entry that was rendered.
struct RenderBlobResultWithEntrySize<'a>(&'a RenderBlobResult, usize);

impl RenderSummary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_zero(&self) -> bool {
        self.entry_count.load(Ordering::Relaxed) == 0
    }

    /// Add a blob render result into the accumulated totals.
    fn add(&self, rhs: RenderBlobResultWithEntrySize) {
        let entry_size = rhs.1;
        self.entry_count.fetch_add(1, Ordering::Relaxed);
        match rhs.0 {
            RenderBlobResult::PayloadAlreadyExists => {
                self.already_existed_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_already_existed
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::PayloadCopiedByRequest => {
                self.copy_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_rendered
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::PayloadCopiedLinkLimit => {
                self.copy_count.fetch_add(1, Ordering::Relaxed);
                self.copy_link_limit_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_rendered
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied_link_limit
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::PayloadCopiedWrongMode => {
                self.copy_count.fetch_add(1, Ordering::Relaxed);
                self.copy_wrong_mode_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_rendered
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied_wrong_mode
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::PayloadCopiedWrongOwner => {
                self.copy_count.fetch_add(1, Ordering::Relaxed);
                self.copy_wrong_owner_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_rendered
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied_wrong_owner
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::PayloadHardLinked => {
                self.link_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_rendered
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_linked
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::SymlinkAlreadyExists => {
                self.already_existed_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_already_existed
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
            RenderBlobResult::SymlinkWritten => {
                self.symlink_count.fetch_add(1, Ordering::Relaxed);

                self.total_bytes_rendered
                    .fetch_add(entry_size, Ordering::Relaxed);
                self.total_bytes_copied
                    .fetch_add(entry_size, Ordering::Relaxed);
            }
        }
    }
}

/// A render reporter that accumulates blob render statistics.
#[derive(Debug, Default)]
pub struct RenderSummaryReporter {
    render_summary: RenderSummary,
}

impl RenderReporter for RenderSummaryReporter {
    fn rendered_blob(&self, entry: &crate::graph::Entry, render_blob_result: &RenderBlobResult) {
        self.render_summary.add(RenderBlobResultWithEntrySize(
            render_blob_result,
            entry.size as usize,
        ));
    }
}
