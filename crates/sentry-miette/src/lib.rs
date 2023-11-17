// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

// TODO: this might be overkill for the integration changes we make/need

use sentry::protocol::Event;
use sentry::types::Uuid;
use sentry::Hub;

/// Helper function to capture an miette error/report for sentry
pub fn capture_miette(e: &miette::Error) -> Uuid {
    Hub::with_active(|hub| hub.capture_miette(e))
}

/// Helper function to create an event from a `miette::Error`.
pub fn event_from_error(err: &miette::Error) -> Event<'static> {
    let dyn_err: &dyn std::error::Error = err.as_ref();

    let mut event = sentry::event_from_error(dyn_err);

    // Use the miette formated version of the error in the sentry
    // event's message when it doesn't already have a message.
    if event.message.is_none() {
        event.message = Some(format!("{err:?}"));
    }

    event
}

/// Hub extension methods for working with [`miette`].
pub trait MietteHubExt {
    /// Captures an [`miette::Error`] on a specific hub.
    fn capture_miette(&self, e: &miette::Error) -> Uuid;
}

impl MietteHubExt for Hub {
    fn capture_miette(&self, miette_error: &miette::Error) -> Uuid {
        let event = event_from_error(miette_error);
        self.capture_event(event)
    }
}
