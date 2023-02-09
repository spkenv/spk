// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use once_cell::sync::OnceCell;

use crate::graph;

/// Receives updates from a render process to be reported.
///
/// Unless the render runs into errors, every call to visit_* is
/// followed up by a call to the corresponding rendered_*.
pub trait RenderReporter: Send + Sync {
    /// Called when a layer has been identified to render
    fn visit_layer(&self, _manifest: &graph::Manifest) {}

    /// Called when a layer has finished rendering
    fn rendered_layer(&self, _manifest: &graph::Manifest) {}

    /// Called when an entry has been identified to render
    fn visit_entry(&self, _entry: &graph::Entry) {}

    /// Called when an entry has finished rendering
    fn rendered_entry(&self, _entry: &graph::Entry) {}
}

#[derive(Default)]
pub struct SilentRenderReporter;
impl RenderReporter for SilentRenderReporter {}

/// Reports sync progress to an interactive console via progress bars
#[derive(Default)]
pub struct ConsoleRenderReporter {
    bars: OnceCell<ConsoleRenderReporterBars>,
}

impl ConsoleRenderReporter {
    fn get_bars(&self) -> &ConsoleRenderReporterBars {
        self.bars.get_or_init(Default::default)
    }
}

impl RenderReporter for ConsoleRenderReporter {
    fn visit_layer(&self, _: &graph::Manifest) {
        let bars = self.get_bars();
        bars.layers.inc_length(1);
    }

    fn rendered_layer(&self, _: &graph::Manifest) {
        let bars = self.get_bars();
        bars.layers.inc(1);
    }

    fn visit_entry(&self, entry: &graph::Entry) {
        let bars = self.get_bars();
        bars.entries.inc_length(1);
        if entry.kind.is_blob() {
            bars.bytes.inc_length(entry.size);
        }
    }

    fn rendered_entry(&self, entry: &graph::Entry) {
        let bars = self.get_bars();
        bars.entries.inc(1);
        if entry.kind.is_blob() {
            bars.bytes.inc(entry.size);
        }
    }
}

struct ConsoleRenderReporterBars {
    renderer: Option<std::thread::JoinHandle<()>>,
    layers: indicatif::ProgressBar,
    entries: indicatif::ProgressBar,
    bytes: indicatif::ProgressBar,
}

impl Default for ConsoleRenderReporterBars {
    fn default() -> Self {
        static TICK_STRINGS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        static PROGRESS_CHARS: &str = "=>-";
        let entries_style = indicatif::ProgressStyle::default_bar()
            .template("      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {pos:>8}/{len:6}")
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bytes_style = indicatif::ProgressStyle::default_bar()
            .template(
                "      {spinner} {msg:<16.green} [{bar:40.cyan/dim}] {bytes:>8}/{total_bytes:7}",
            )
            .tick_strings(TICK_STRINGS)
            .progress_chars(PROGRESS_CHARS);
        let bars = indicatif::MultiProgress::new();
        let layers = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(entries_style.clone())
                .with_message("rendering layers"),
        );
        let entries = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(entries_style)
                .with_message("rendering entries"),
        );
        let bytes = bars.add(
            indicatif::ProgressBar::new(0)
                .with_style(bytes_style)
                .with_message("processing data"),
        );
        entries.enable_steady_tick(100);
        bytes.enable_steady_tick(100);
        // the progress bar must be awaited from some thread
        // or nothing will be shown in the terminal
        let renderer = Some(std::thread::spawn(move || {
            if let Err(err) = bars.join() {
                tracing::error!("Failed to show render progress: {err}");
            }
        }));
        Self {
            renderer,
            layers,
            entries,
            bytes,
        }
    }
}

impl Drop for ConsoleRenderReporterBars {
    fn drop(&mut self) {
        self.bytes.finish_and_clear();
        self.entries.finish_and_clear();
        self.layers.finish_and_clear();
        if let Some(r) = self.renderer.take() {
            let _ = r.join();
        }
    }
}
