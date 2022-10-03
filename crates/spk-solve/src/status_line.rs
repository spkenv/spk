// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossterm::cursor::{MoveTo, RestorePosition, SavePosition};
use crossterm::style::Print;
use crossterm::terminal::{self, Clear, ClearType, ScrollUp};
use crossterm::{csi, queue, Command, QueueableCommand};

use crate::{Error, Result};

struct ScrollRegion(u16, u16);

/// Set the terminal scroll region to (top line, bottom line).
impl Command for ScrollRegion {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, csi!("{};{}r"), self.0 + 1, self.1 + 1)
    }
}

/// Reset the terminal scroll region to the entire screen.
struct ResetScrollRegion;

impl Command for ResetScrollRegion {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, csi!("r"))
    }
}

#[derive(Debug)]
pub(crate) struct StatusLine {
    stdout: std::io::Stdout,
    status_height: u16,
    term_cols: u16,
    term_rows: u16,
    sig_winch_tripped: Arc<AtomicBool>,
}

impl StatusLine {
    pub(crate) fn flush(&mut self) -> Result<()> {
        self.stdout.flush().map_err(Error::StatusBarIOError)
    }

    pub(crate) fn new(stdout: std::io::Stdout, status_height: u16) -> Result<Self> {
        // Monitor SIGWINCH to know when the terminal has been resized,
        // to update our saved dimensions.
        let sig_winch_tripped = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(
            signal_hook::consts::SIGWINCH,
            Arc::clone(&sig_winch_tripped),
        );

        let (term_cols, term_rows) = terminal::size().map_err(Error::StatusBarIOError)?;

        let mut s = Self {
            stdout,
            status_height,
            term_cols,
            term_rows,
            sig_winch_tripped,
        };

        // Scroll the screen to make room for the status bar.
        s.stdout
            .queue(ScrollUp(status_height))
            .map_err(Error::StatusBarIOError)?;

        s.update_scroll_area()?;

        s.stdout.flush().map_err(Error::StatusBarIOError)?;

        Ok(s)
    }

    pub(crate) fn set_status<S>(&mut self, row: u16, msg: S) -> Result<()>
    where
        S: AsRef<str>,
    {
        // Check if the terminal was resized.
        if self
            .sig_winch_tripped
            .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            self.update_scroll_area()?
        }

        let msg = msg.as_ref();

        queue!(
            self.stdout,
            SavePosition,
            ResetScrollRegion,
            MoveTo(0, self.term_rows - self.status_height + row),
            Print(format!(
                "{msg:.max_cols$}",
                max_cols = (self.term_cols - 1) as usize
            )),
            Clear(ClearType::UntilNewLine),
            ScrollRegion(0, self.term_rows - self.status_height - 1),
            RestorePosition
        )
        .map_err(Error::StatusBarIOError)?;

        Ok(())
    }

    fn update_scroll_area(&mut self) -> Result<()> {
        (self.term_cols, self.term_rows) = terminal::size().map_err(Error::StatusBarIOError)?;

        // Set the scroll area to leave the bottom line available for a static
        // message.
        self.stdout
            .queue(ScrollRegion(0, self.term_rows - self.status_height - 1))
            .map_err(Error::StatusBarIOError)?;

        // Put the cursor above the status bar.
        // The `- 2` is necessary to make the scroll area reliable when using
        // `spk explain` and there is solver output already being printed.
        // With `- 1`, the solver output overwrites the last line of the
        // status bar instead of in the scroll area as expected.
        // But with `- 2`, when using `spk env` and there is no output, then
        // the cursor is one row higher than it should be.
        self.stdout
            .queue(MoveTo(0, self.term_rows - self.status_height - 2))
            .map_err(Error::StatusBarIOError)?;

        Ok(())
    }
}

impl Drop for StatusLine {
    fn drop(&mut self) {
        // Restore original terminal scroll area
        let _ = queue!(
            self.stdout,
            ResetScrollRegion,
            MoveTo(0, self.term_rows),
            Print("\n")
        );
        let _ = self.stdout.flush();
    }
}
