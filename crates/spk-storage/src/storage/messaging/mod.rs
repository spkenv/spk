// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod kafka;

//use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use spk_config::{Indexer, MessageChannel};
use spk_schema::BuildIdent;
use spk_schema::name::RepositoryName;
use variantly::Variantly;

use super::{RepositoryHandle, remote_repository};
use crate::{Error, Result};

// TODO: make this pub for testing, put it back to pub(crate)
/// Types of events than can be reported about a package
#[derive(Copy, Clone, Debug, strum::Display, Deserialize, Serialize, PartialEq)]
pub enum PackageEvent {
    /// A package has been published to a repo
    #[serde(alias = "package published")]
    #[strum(to_string = "package published")]
    Published,
    /// A packaged has been modified, e.g. deprecated/undeprecated
    #[serde(alias = "package modified")]
    #[strum(to_string = "package modified")]
    Modified,
    /// A package has been removed from a repo
    #[serde(alias = "package removed")]
    #[strum(to_string = "package removed")]
    Removed,
    /// A special admin message to make an indexer generate a
    /// full index from scratch for the repo
    #[serde(alias = "generate index")]
    #[strum(to_string = "generate index")]
    GenerateFullIndex,
}

/// A package event message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PackageEventMessage {
    /// The kind of index update event
    pub event: PackageEvent,
    /// The address of the repo being indexed
    pub repo: String,
    /// The name of the repo being indexed
    pub repo_name: String,
    // TODO: this is optional for a GenerateFullIndex event
    /// The name of the repo being indexed
    pub package: String,
}

/// Send a package event to each configured messaging system
pub(crate) async fn announce_package_event(
    event: PackageEvent,
    to: &url::Url,
    repo_name: &RepositoryName,
    ident: &BuildIdent,
) -> Result<()> {
    let config = spk_config::get_config()?;
    // Sends an index event message to all configured messaging systems
    let name = repo_name.to_string();
    for channel in &config.messaging {
        match channel {
            MessageChannel::Kafka(kafka_channel) => {
                // Filter on the configured repo_name
                if kafka_channel.repo_names.contains(&name) {
                    kafka::announce_package_event(kafka_channel, event, to, repo_name, ident)
                        .await?
                }
            }
        }
    }

    Ok(())
}

/// Types of events than can be reported about an index
#[derive(Copy, Clone, Debug, strum::Display, Deserialize, Serialize, PartialEq, Variantly)]
pub(crate) enum IndexEvent {
    /// An index update or full generation has started
    #[serde(alias = "index started")]
    #[strum(to_string = "index started")]
    Started,
    /// An index update or full generation is in-progress
    #[serde(alias = "index in-progress")]
    #[strum(to_string = "index in-progress")]
    InProgress,
    /// An index update or full generation has completed
    #[serde(alias = "index completed")]
    #[strum(to_string = "index completed")]
    Completed,
    /// An indexer is alive and monitoring the repo for package updates
    #[serde(alias = "indexer heartbeat")]
    #[strum(to_string = "indexer heartbeat")]
    IndexerHeartbeat,
}

/// An index update event message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct IndexUpdateMessage {
    /// The kind of index update event
    pub event: IndexEvent,
    /// The address of the repo being indexed
    pub repo: String,
    /// The name of the repo being indexed
    pub name: String,
    // TODO: this is optional for indexer heartbeat events
    /// Timestamp when the index generation for this status update
    /// message first started.
    pub start: i64,
}

/// Send an index event to each configured messaging system
pub(crate) async fn announce_index_event(
    event: IndexEvent,
    to: &url::Url,
    repo_name: &RepositoryName,
    index_start_time: i64,
) -> Result<()> {
    let config = spk_config::get_config()?;

    // Sends an index event message to all configured messaging
    // systems.
    for channel in &config.messaging {
        match channel {
            MessageChannel::Kafka(kafka_channel) => {
                kafka::announce_index_event(kafka_channel, event, to, repo_name, index_start_time)
                    .await?
            }
        }
    }

    Ok(())
}

/// Subscribes to index updates topic and listens to updates until it
/// gets one that indicates the repo's index has been updated after
/// the given update time.
pub(crate) async fn listen_to_index_status_until_updated(
    repo: &RepositoryHandle,
    update_time: i64,
) -> Result<()> {
    let name = repo.name().to_string();

    let index_config = spk_config::get_index_config(&name);
    let update_channel_name = index_config.update_message_channel;

    let config = spk_config::get_config()?;
    for channel in &config.messaging {
        match channel {
            MessageChannel::Kafka(kafka_channel) => {
                // Only listen to index update channel with the
                // matching name.
                if kafka_channel.name == update_channel_name {
                    // Filter on the configured repo name
                    if kafka_channel.repo_names.contains(&name) {
                        kafka::listen_to_index_status_updates(kafka_channel, update_time, repo)
                            .await?
                    }
                }
            }
        }
    }

    Ok(())
}

/// Runs an index update server (an indexer) that listens publish
/// modification events on one channel, launches index updates, and
/// sends index status messages to another channel.
///
/// The indexer will run forever, or until it is interrupted or
/// encounters an error it cannot handle.
pub async fn run_index_update_server(indexer_id: String, indexer_config: &Indexer) -> Result<()> {
    // This only supports an indexer that uses kafka channels for both
    // message flows: listening to publish modifications, and sending
    // status updates.
    let mut message_channel = None;

    let config = spk_config::get_config()?;
    for channel in &config.messaging {
        match channel {
            MessageChannel::Kafka(kafka_channel) => {
                if kafka_channel.name == indexer_config.message_channel_name
                    && kafka_channel.repo_names.contains(&indexer_config.repo_name)
                {
                    message_channel = Some(kafka_channel);
                }
            }
        }
    }

    // Check indexer's configured repo has indexing enabled before getting it.
    let repo = match config.repositories.get(&indexer_config.repo_name) {
        Some(repo_config) => {
            if repo_config.use_index {
                match indexer_config.repo_name.as_str() {
                    "local" => {
                        return Err(Error::String(
                            "Unable to run indexer '{indexer_id}' due to: indexer is configured with the 'local' repo, which cannot be updated by an indexer".to_string()));
                    }
                    name => {
                        // Ensure the repo handle is not for an
                        // Indexed repo, or else all the subsequent
                        // processing will be using the index instead
                        // of the underlying repo.
                        let handle: RepositoryHandle = remote_repository(name).await?.into();
                        handle
                    }
                }
            } else {
                return Err(Error::String(format!(
                    "Unable to run indexer '{indexer_id}' due to: indexer's configured repo '{}' has 'use_index' set to false, so indexing is disabled on that repo",
                    indexer_config.repo_name
                )));
            }
        }
        None => {
            return Err(Error::String(
                "No indexer '{indexer_id}' configured in SPK config file".to_string(),
            ));
        }
    };

    // Run the indexer. This will run forever, or until it is
    // interrupted or encounters an error it cannot handle.
    match message_channel {
        Some(kafka_channel) => {
            kafka::listen_to_package_events_and_run_index_updates(
                indexer_id,
                kafka_channel,
                indexer_config,
                &repo,
            )
            .await
        }
        _ => Err(crate::Error::String(format!(
            "Unable to run indexer: Running an indexer requires a kafka messaging channel called '{}'. But it is not configured.",
            indexer_config.message_channel_name
        ))),
    }
}
