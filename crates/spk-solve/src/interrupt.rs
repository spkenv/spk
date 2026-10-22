// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use signal_hook::consts::SIGINT;

use crate::{Error, Result};

#[cfg(all(test, unix))]
#[path = "./interrupt_test.rs"]
mod interrupt_test;

static INTERRUPTS: Lazy<std::io::Result<InterruptState>> = Lazy::new(InterruptState::new);

struct InterruptState {
    cancelled: Arc<AtomicBool>,
    use_default: Arc<AtomicBool>,
    active_solves: Mutex<usize>,
}

impl InterruptState {
    /// Install handlers once, preserving normal termination outside a solve.
    fn new() -> std::io::Result<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let use_default = Arc::new(AtomicBool::new(true));
        // Unregistering a signal-hook action does not restore SIG_DFL.
        let default_handler =
            signal_hook::flag::register_conditional_default(SIGINT, Arc::clone(&use_default))?;
        if let Err(err) = signal_hook::flag::register(SIGINT, Arc::clone(&cancelled)) {
            signal_hook::low_level::unregister(default_handler);
            return Err(err);
        }
        Ok(Self {
            cancelled,
            use_default,
            active_solves: Mutex::new(0),
        })
    }
}

/// Keep graceful cancellation enabled until the last active solve finishes.
pub(crate) struct SolveInterruptGuard(&'static InterruptState);

impl SolveInterruptGuard {
    /// Enable solver cancellation, clearing any previous solve's interruption.
    pub(crate) fn new() -> Result<Self> {
        let state = INTERRUPTS.as_ref().map_err(|err| {
            Error::String(format!("Unable to set up solver Ctrl+C handling: {err}"))
        })?;
        let mut active = state.active_solves.lock().unwrap();
        if *active == 0 {
            state.cancelled.store(false, Ordering::Relaxed);
            state.use_default.store(false, Ordering::Relaxed);
        }
        *active += 1;
        Ok(Self(state))
    }

    /// Report whether Ctrl+C was received during the current solve.
    pub(crate) fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }
}

impl Drop for SolveInterruptGuard {
    fn drop(&mut self) {
        let mut active = self.0.active_solves.lock().unwrap();
        *active -= 1;
        if *active == 0 {
            self.0.use_default.store(true, Ordering::Relaxed);
        }
    }
}
