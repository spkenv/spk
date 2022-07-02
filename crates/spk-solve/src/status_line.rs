// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use console::Term;

#[derive(Debug)]
pub(crate) struct StatusLine {
    term: Term,
    status_height: u16,
    term_cols: u16,
    term_rows: u16,
    sig_winch_tripped: Arc<AtomicBool>,
}

impl StatusLine {
    pub(crate) fn new(term: Term, status_height: u16) -> Self {
        // Monitor SIGWINCH to know when the terminal has been resized,
        // to update our saved dimensions.
        let sig_winch_tripped = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(
            signal_hook::consts::SIGWINCH,
            Arc::clone(&sig_winch_tripped),
        );

        let (term_rows, term_cols) = term.size();

        let mut s = Self {
            term,
            status_height,
            term_cols,
            term_rows,
            sig_winch_tripped,
        };

        // Scroll the screen to make room for the status bar.
        for _ in 0..status_height {
            let _ = s.term.write("\x1bD".as_bytes());
        }

        s.update_scroll_area();

        s
    }

    pub(crate) fn set_status<S>(&mut self, row: u16, msg: S)
    where
        S: AsRef<str>,
    {
        // Check if the terminal was resized.
        if self
            .sig_winch_tripped
            .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            self.update_scroll_area();
        }

        let msg = msg.as_ref();

        // Escape chars save cursor position and then
        // move to where the status line should be printed.
        // Jediterm (IntelliJ terminal emulator) has a quirk
        // where printing when the cursor is outside the
        // scroll region will force the text to land inside
        // the scroll region. So here we have to extend the
        // scroll region while printing the status bar. No,
        // turning off "OriginMode" doesn't help.
        let _ = self.term.write_fmt(format_args!(
            "\x1b7\x1b[0;{rows}r\x1b[{rows};0H{msg:.max_cols$}\x1b[K\x1b[0;{rows_minus_height}r\x1b8",
            rows = self.term_rows - self.status_height + row + 1,
            rows_minus_height = self.term_rows - self.status_height,
            max_cols = (self.term_cols - 1) as usize,
        ));
    }

    fn update_scroll_area(&mut self) {
        (self.term_rows, self.term_cols) = self.term.size();

        // Set the scroll area to leave the bottom line available for a static
        // message.
        let _ = self.term.write_fmt(format_args!(
            "\x1b[{rows};0r",
            rows = self.term_rows - self.status_height
        ));

        // Put the cursor above the status bar.
        // The `- 2` is necessary to make the scroll area reliable when using
        // `spk explain` and there is solver output already being printed.
        // With `- 1`, the solver output overwrites the last line of the
        // status bar instead of in the scroll area as expected.
        // But with `- 2`, when using `spk env` and there is no output, then
        // the cursor is one row higher than it should be.
        let _ = self
            .term
            .move_cursor_to(0, (self.term_rows - self.status_height - 2).into());
    }
}

impl Drop for StatusLine {
    fn drop(&mut self) {
        // Restore original terminal scroll area
        let _ = self.term.write_fmt(format_args!("\x1b[r"));

        let _ = self.term.move_cursor_to(0, self.term_rows.into());
        let _ = self.term.write("\n".as_bytes());
    }
}
