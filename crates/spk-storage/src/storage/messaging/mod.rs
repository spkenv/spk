// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod kafka;

use spk_config::MessageChannel;
use spk_schema::BuildIdent;

use crate::Result;

/// Types of events than can be reported about a package
#[derive(Copy, Clone, Debug, strum::Display)]
pub(crate) enum PackageEvent {
    #[strum(to_string = "package published")]
    Published,
    #[strum(to_string = "package modified")]
    Modified,
    #[strum(to_string = "package removed")]
    Removed,
}

pub(crate) async fn announce_package_event(
    event: PackageEvent,
    to: &url::Url,
    ident: &BuildIdent,
) -> Result<()> {
    let config = spk_config::get_config()?;
    for channel in &config.messaging {
        match channel {
            MessageChannel::Kafka(kafka_channel) => {
                kafka::announce_package_event(kafka_channel, event, to, ident).await?
            }
        }
    }

    Ok(())
}
