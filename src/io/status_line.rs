// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::Write;

use console::Term;

#[derive(Debug)]
pub(crate) struct StatusLine {
    term: Term,
    status_height: u16,
    term_cols: u16,
    term_rows: u16,
}

impl StatusLine {
    pub(crate) fn new(mut term: Term, status_height: u16) -> Self {
        // TODO: catch SIGWINCH to update sizes
        let (term_rows, term_cols) = term.size();

        let _ = term.clear_last_lines(status_height.into());

        // Set the scroll area to leave the bottom line available for a static
        // message.
        let _ = term.write_fmt(format_args!(
            "\x1b[{rows};0r",
            rows = term_rows - status_height
        ));

        // Put the cursor above the status bar.
        let _ = term.move_cursor_to(0, (term_rows - status_height - 1).into());

        Self {
            term,
            status_height,
            term_cols,
            term_rows,
        }
    }

    pub(crate) fn set_status<S>(&mut self, row: u16, msg: S)
    where
        S: AsRef<str>,
    {
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
}

impl Drop for StatusLine {
    fn drop(&mut self) {
        // Restore original terminal scroll area
        let _ = self.term.write_fmt(format_args!("\x1b[r"));
    }
}
