// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use itertools::Itertools;
use once_cell::sync::Lazy;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use rdkafka::{ClientConfig, Message, Offset, TopicPartitionList};
use serde_json::json;
use spk_config::{Indexer, KafkaChannel};
use spk_schema::BuildIdent;
use spk_schema::ident::{OptVersionIdent, parse_build_ident};
use spk_schema::name::RepositoryName;
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Mutex;
use ulid::Ulid;

use crate::storage::messaging::{
    IndexEvent,
    IndexUpdateMessage,
    PackageEvent,
    PackageEventMessage,
};
use crate::{Error, FlatBufferRepoIndex, RepositoryHandle, RepositoryIndexMut, Result};

/// Number of milliseconds to sleep while waiting for next message,
/// when listening for new messages.
const LISTEN_YIELD_SLEEP_TIME: u64 = 100;

/// Number of milliseconds in a second, used in time conversions
const NUM_MS_IN_ONE_SECOND: u64 = 1000;

type ProducersByBrokers = HashMap<Vec<String>, std::result::Result<FutureProducer, KafkaError>>;

static KAFKA_PRODUCERS: Lazy<Mutex<ProducersByBrokers>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Send a package update event message
pub(crate) async fn announce_package_event(
    kafka_channel: &KafkaChannel,
    event: PackageEvent,
    to: &url::Url,
    repo_name: &RepositoryName,
    ident: &BuildIdent,
) -> Result<()> {
    // Only send a message if the package events topic is configured
    let Some(topic_name) = kafka_channel.package_updates_topic_name.as_ref() else {
        return Ok(());
    };

    // Get the producer client connection
    let mut kafka_producers = KAFKA_PRODUCERS.lock().await;

    let producer = kafka_producers
        .entry(kafka_channel.brokers.clone())
        .or_insert_with(|| {
            ClientConfig::new()
                // XXX: probably should expose all configuration parameters in
                // spk config
                .set("bootstrap.servers", kafka_channel.brokers.join(","))
                .set("message.timeout.ms", kafka_channel.message_timeout_ms.to_string())
                .create()
        })
        .as_ref()
        .map_err(|err| {
            Error::String(format!(
                "failed to create kafka producer (for package updates events topic) from config: {err}"
            ))
        })?;

    // Assemble the message
    let message = PackageEventMessage {
        event,
        repo: to.to_string(),
        repo_name: repo_name.to_string(),
        package: ident.to_string(),
    };

    // Send the package event message
    producer
        .send(
            FutureRecord::to(topic_name)
                // Using the package name as the key for purposes of sending
                // all messages about the same package to the same topic
                // partition.
                .key(ident.name().as_str())
                .payload(&json!(message).to_string()),
            Duration::from_millis(kafka_channel.producer_queue_timeout_ms),
        )
        .await
        .map_err(|err| {
            Error::String(format!(
                "failed to send kafka package event message: {err:?}"
            ))
        })?;

    Ok(())
}

/// Send an index event message
pub(crate) async fn announce_index_event(
    kafka_channel: &KafkaChannel,
    event: IndexEvent,
    to: &url::Url,
    repo_name: &RepositoryName,
    index_start_time: i64,
) -> Result<()> {
    // Only send a message if the index updates topic is configured
    let Some(topic_name) = kafka_channel.index_updates_topic_name.as_ref() else {
        return Ok(());
    };

    // Get the producer client connection
    let mut kafka_producers = KAFKA_PRODUCERS.lock().await;

    let producer = kafka_producers
        .entry(kafka_channel.brokers.clone())
        .or_insert_with(|| {
            ClientConfig::new()
                // XXX: probably should expose all configuration parameters in
                // spk config
                .set("bootstrap.servers", kafka_channel.brokers.join(","))
                .set(
                    "message.timeout.ms",
                    kafka_channel.message_timeout_ms.to_string(),
                )
                .create()
        })
        .as_ref()
        .map_err(|err| {
            Error::String(format!(
                "failed to create kafka producer from config: {err}"
            ))
        })?;

    // Create the message
    let message = IndexUpdateMessage {
        event,
        repo: to.to_string(),
        name: repo_name.to_string(),
        start: index_start_time,
    };

    // Send the index update message
    producer
        .send(
            FutureRecord::to(topic_name)
                // Using the repo name as the key for purposes of sending
                // all messages about the same index to the same topic partition.
                .key(repo_name.as_str())
                .payload(&json!(message).to_string()),
            Duration::from_millis(kafka_channel.producer_queue_timeout_ms),
        )
        .await
        .map_err(|err| {
            Error::String(format!(
                "failed to send kafka index update message: {err:?}"
            ))
        })?;

    tracing::debug!(
        "Sent index update message: {event} - {repo_name} - {to} - {index_start_time} as:\n{message:?}"
    );

    Ok(())
}

/// Helper for interrupt handling in message stream consumer
/// loops. Note this only works on unix at the moment.
fn setup_running_interrupt_handling() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    #[cfg(unix)]
    {
        let running = Arc::clone(&running);
        tokio::spawn(async move {
            let mut sigint = signal(SignalKind::interrupt()).expect("sigint supported");
            let mut sigquit = signal(SignalKind::quit()).expect("sigquit supported");

            tokio::select! {
                _ = sigint.recv() => {
                    eprintln!("Received SIGINT, shutting down...");
                }
                _ = sigquit.recv() => {
                    eprintln!("Received SIGQUIT, shutting down...");
                }
            }
            running.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
    running
}

/// Helper for consumers that want to start with the message before
/// the latest message because they want to use it to check whether
/// the usual message producer is running by seeing if it has sent a
/// message recently.
fn set_offsets_to_one_before_the_latest_message(
    consumer: Arc<StreamConsumer>,
    topic_name: &str,
    consumer_label: String,
    broker_fetch_timeout: u64,
) -> Result<()> {
    let timeout = Timeout::After(Duration::new(broker_fetch_timeout, 0));

    // Set up the message reading offset to the just before the last
    // message in the topics.
    let metadata = consumer
        .fetch_metadata(Some(topic_name), timeout)
        .map_err(|err| {
            Error::String(format!(
                "failed to fetch metadata for '{topic_name}' topic for a kafka index update consumer: {err}"
            ))
        })?;

    let mut offsets = TopicPartitionList::new();
    for topic in metadata.topics() {
        for partition in topic.partitions() {
            let (_low, high) = consumer
                .fetch_watermarks(topic.name(), partition.id(), timeout)
                .unwrap_or((-1, -1));

            // Go back one message from the latest
            let new_offset = std::cmp::max(0, high - 1);
            tracing::debug!(
                "New offset for {} part: {}, was {}, will be {}",
                topic.name(),
                partition.id(),
                high,
                new_offset
            );
            offsets
                .add_partition_offset(topic.name(), partition.id(), Offset::Offset(new_offset))
                .map_err(|err| {
                    Error::String(format!(
                        "failed to add partition offset to TopicPartitionList for the kafka {consumer_label} consumer: {err}"
                    ))
                })?;
        }
    }

    // Update the offsets so the previous message(s) will be the first
    // read from the stream.
    consumer.commit(&offsets, CommitMode::Sync).map_err(|err| {
        Error::String(format!(
            "failed to commit new starting offsets for the kafka {consumer_label} consumer: {err}"
        ))
    })
}

/// Listen to the index updates topic until there's an "index completed"
/// message for the given repository's index with an index update
/// start time after the given update timestamp. This will also
/// timeout if too much time passes without receiving any index update
/// messages.
pub(crate) async fn listen_to_index_status_updates(
    kafka_channel: &KafkaChannel,
    min_update_time: i64,
    repo: &RepositoryHandle,
) -> Result<()> {
    // Only subscribe to index update messages if the index updates topic is configured
    let Some(topic_name) = kafka_channel.index_updates_topic_name.as_ref() else {
        return Ok(());
    };

    tracing::info!(
        "Waiting for '{}' repo's index to be updated ...",
        repo.name()
    );

    // Subscribe to index status updates with a unique group id that
    // is just for this consumer.
    let group_id = format!("spk-publish-{}", Ulid::new());
    tracing::debug!(
        "Index status update consumer's unique group id: {group_id} ({})",
        group_id.len()
    );

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", kafka_channel.brokers.join(","))
        .set("group.id", group_id)
        .set("enable.partition.eof", "false")
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .set(
            "session.timeout.ms",
            kafka_channel
                .index_update_listener_session_timeout_ms
                .to_string(),
        )
        .set(
            "max.poll.interval.ms",
            kafka_channel
                .index_update_listener_max_polling_interval_ms
                .to_string(),
        )
        // This will be changed below before the first message it read.
        // The offset will be set to the one before the latest message.
        .set("auto.offset.reset", "earliest")
        .create()
        .map_err(|err| {
            Error::String(format!(
                "failed to create kafka index updates consumer from config: {err}"
            ))
        })?;

    let consumer = Arc::new(consumer);
    consumer.subscribe(&[topic_name]).map_err(|err| {
        Error::String(format!(
            "failed to subscribe to kafka index updates topic: {err}"
        ))
    })?;

    // Change the starting message offset for reading to the just
    // before the last message in the topic.
    set_offsets_to_one_before_the_latest_message(
        consumer.clone(),
        topic_name,
        "index updates".to_string(),
        kafka_channel.index_update_listener_broker_fetch_timeout_s,
    )?;

    // Set up signal handlers to stop if interrupted
    let running = setup_running_interrupt_handling();

    // Start listening to index update messages
    let utc_now: DateTime<Utc> = Utc::now();
    let now_timestamp = utc_now.timestamp_millis();
    let recent_past_messages_time = rdkafka::Timestamp::from(
        now_timestamp - kafka_channel.index_update_listener_recent_past_duration_ms,
    );

    let mut there_is_a_recent_message = false;
    let mut time_since_last_message = 0;

    tracing::debug!(
        "Looking for a recent index update message that was sent after: {min_update_time}"
    );

    let mut stream = consumer.stream();

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::select! {
            maybe_message = stream.next() => {
                match maybe_message {
                    Some(Ok(message)) => {
                        // Get message header to see if this is a recent enough message.
                        if !there_is_a_recent_message && message.timestamp() < recent_past_messages_time {
                            // This message was sent so long ago that the indexer might not be running
                            tracing::debug!("Message read. Too old, ignoring it: {:?} < {:?}",  message.timestamp(), recent_past_messages_time);
                            continue;
                        }

                        tracing::debug!("New message read: {:?} >= {:?}",  message.timestamp(), recent_past_messages_time);
                        there_is_a_recent_message = true;

                        // Get the message details to check the index
                        // start time, repo it is for, and kind of
                        // update event.
                        let Some(payload) = message.payload_view::<str>().map(|s| s.map(|s| s.to_owned()).expect("payload is legal UTF-8")) else {
                            continue;
                        };

                        if let Ok(index_update) = serde_json::from_str::<IndexUpdateMessage>(&payload).inspect_err(|err| {
                            tracing::warn!("Failed to index update message parse payload ('{payload}') as IndexUpdateMessage: {err}");
                        }) {
                            // This is a valid recent message, so reset the
                            // "indexer might be down" timeout timer.
                            time_since_last_message = 0;
                            tracing::debug!("Read index update message: {index_update:?}");

                            if index_update.start >= min_update_time {
                                tracing::debug!("Message is from an index update that started after the min_update_time: {}",  index_update.start - min_update_time);

                                if repo.address().as_str() == index_update.repo {
                                    // The message is for correct repository
                                    if index_update.event.is_completed() {
                                        // And the index has been updated. So this can stop listening now.
                                        running.store(false, std::sync::atomic::Ordering::SeqCst);
                                        tracing::debug!("Recent index update completed. Stopping waiting: '{}'", index_update.event);
                                        break;
                                    }
                                }
                            } else {
                                // This message is about an older
                                // index update than we are interested in.
                                tracing::debug!("Index update message is not for recent enough update: {}",  index_update.start - min_update_time);
                            }
                        }
                    },
                    Some(err @ Err(_)) => {
                        return Err(Error::String(format!("Unexpected error consuming next index update message: {err:?}")));
                    }
                    None => {
                        // Stream ended?
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(LISTEN_YIELD_SLEEP_TIME)) => {
                // Yield periodically so interrupting is responsive.
                if there_is_a_recent_message {
                    // If enough time has passed since the last recent
                    // message was read, then assume something has happened
                    // to the index updating process and stop waiting.
                    time_since_last_message += LISTEN_YIELD_SLEEP_TIME;
                    if time_since_last_message >= kafka_channel.index_update_listener_timeout_ms {
                        tracing::warn!("Index updates  consumer timed out after {} seconds. The index may not have been updated.",
                                       kafka_channel.index_update_listener_timeout_ms / NUM_MS_IN_ONE_SECOND
                        );
                        running.store(false, std::sync::atomic::Ordering::SeqCst);
                    }

                } else {
                    // Without a recent enough message, assume the indexer is down
                    // and stop waiting for the index to update.
                    tracing::warn!("Index updates consumer cannot see a recent message. The indexer might be down. Not waiting for the index update.");
                    running.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }
    }

    Ok(())
}

fn send_issue_to_sentry(message: String, err: &Error) {
    tracing::warn!("Sending messaging issue to sentry is not implemented: {message} - {err}");
    // TODO: implement this
}

// Runs an indexer to continuously listen to package update messages,
// de-duplicate them by package/version, and kick off an index update
// (or full index generation) when there's a pause in messages.
//
// The indexer will also send a heartbeat message to the index status
// topic periodically when not running an index update. It relies on
// the index update processing to also emit status messages, in lieu
// of heartbeat messages, when an index update is happening.
#[allow(dead_code)]
pub async fn listen_to_package_events_and_run_index_updates(
    indexer_id: String,
    kafka_channel: &KafkaChannel,
    indexer_config: &Indexer,
    repo_to_index: &RepositoryHandle,
) -> Result<()> {
    // Check there is an incoming package modification events topic
    let Some(package_event_topic_name) = kafka_channel.package_updates_topic_name.as_ref() else {
        return Err(Error::String(
            "Indexer unable to start: package event topic name is required".to_string(),
        ));
    };

    // Check there is an index update topic to send heartbeat messages to.
    let Some(_index_update_topic_name) = kafka_channel.index_updates_topic_name.as_ref() else {
        return Err(Error::String(
            "Indexer unable to start: index update topic name is required".to_string(),
        ));
    };

    // Subscribe to package modification events
    let group_id = format!("spk-indexer-{}", indexer_id);

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", kafka_channel.brokers.join(","))
        .set("group.id", group_id)
        .set("enable.partition.eof", "false")
        .set("enable.auto.commit", "false")
        // Only commit offsets offsets stored via
        // the `consumer.store_offset()` calls
        .set("enable.auto.offset.store", "false")
        .set(
            "session.timeout.ms",
            indexer_config.session_timeout_ms.to_string(),
        )
        .set(
            "max.poll.interval.ms",
            indexer_config.max_polling_interval_ms.to_string(),
        )
        // Replaying lots of messages on restart is fine for an
        // indexer and its updates to an index.
        .set("auto.offset.reset", "earliest")
        .create()
        .map_err(|err| {
            Error::String(format!(
                "Indexer failed to create kafka consumer of package events from config: {err}"
            ))
        })?;

    let consumer = Arc::new(consumer);
    consumer
        .subscribe(&[package_event_topic_name])
        .map_err(|err| {
            Error::String(format!(
                "Indexer failed to subscribe to kafka package event topic: {err}"
            ))
        })?;

    // Set up signal handlers to stop if interrupted
    let running = setup_running_interrupt_handling();

    // Setup placeholder used in indexer's heartbeat messages
    let indexer_name = RepositoryName::new(&indexer_id)?;

    // Setup name-repo pair list, used when generating a full index
    // either because the index is missing or when a generate index
    // message is read.
    let repos = vec![(repo_to_index.name().to_string(), repo_to_index.clone())];

    // Send an initial heartbeat message to indicate this is now running.
    //
    // Without this step, a timing issue can occur with something
    // using the listen_to_index_status_updates() function to detect
    // an indexer or index status updates. It can miss that the
    // indexer is running, but hasn't started an index update yet.
    announce_index_event(
        kafka_channel,
        IndexEvent::IndexerHeartbeat,
        repo_to_index.address(),
        indexer_name,
        // This does not have an index start time so
        // this is a placeholder.
        0,
    )
    .await?;

    // Start listening to index update messages
    let mut time_since_last_message = 0;
    let mut read_messages = 0;
    let mut generate_full_index = false;
    let mut package_versions: HashSet<OptVersionIdent> = HashSet::new();

    let mut stream = consumer.stream();

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::select! {
            maybe_message = stream.next() => {
                match maybe_message {
                    Some(Ok(message)) => {
                        // Get the package event details
                        let Some(payload) = message.payload_view::<str>().map(|s| s.map(|s| s.to_owned()).expect("payload is legal UTF-8")) else { continue; };

                        if let Ok(package_event) = serde_json::from_str::<PackageEventMessage>(&payload).inspect_err(|err| {
                            // This problem message will be discarded
                            let message = format!("Indexer failed to parse package event message payload ('{payload}') as PackageEventMessage: {err}");
                            tracing::warn!(message);
                            send_issue_to_sentry(message, &Error::String(err.to_string()));
                        }) {
                            // Process a valid message
                            tracing::debug!("Indexer read package event message: {package_event:?}");
                            time_since_last_message = 0;

                            // Store the repo and the package that changed, but only if the repo
                            // matches the one this indexer cares about
                            if repo_to_index.address().to_string() == package_event.repo {

                                if PackageEvent::GenerateFullIndex == package_event.event {
                                    // This special event will trigger a full index (re-)generation
                                    // when the indexer next kicks off an index update.
                                    generate_full_index = true;
                                    tracing::info!("Got a special (re-)generate full index message for '{}' repo", repo_to_index.address());
                                } else {
                                    // Capture the package/version that was updated from the message,
                                    // It will have build ident (pkg/ver/build) in it, but only the
                                    // pkg/ver matters for index updates.
                                    let package_ident = match parse_build_ident(package_event.package) {
                                        Ok(ident) => ident,
                                        Err(err) => {
                                            let message = format!(
                                                "Ignoring message. Indexer unable to get package ident from package event message: {err}"
                                            );
                                            tracing::warn!(message);
                                            send_issue_to_sentry(message, &Error::SpkIdentError(err));
                                            continue;
                                        },
                                    };

                                    let version_ident = package_ident.to_version_ident();
                                    tracing::info!("Storing package ('{version_ident}') from package event message for '{}' repo", repo_to_index.address());
                                    package_versions.insert(
                                        OptVersionIdent::new(version_ident.base().clone(), Some(version_ident.version().clone())));
                                }
                            }
                        }

                        // Record message's offset in the consumer, for now.
                        // The accumulated stored offsets are committed once
                        // index update processing has completed, see below.
                        consumer.store_offset_from_message(&message).map_err(|err| {
                            Error::String(format!(
                                "Error while storing message offset in indexer's consumer: {err}"
                            ))
                        })?;
                        read_messages += 1;
                        tracing::info!("Stored message offset in indexer's consumer");

                    },
                    Some(err @ Err(_)) => {
                        return Err(Error::String(format!("Unexpected error consuming next index update message: {err:?}")));
                    }
                    None => {
                        // Stream ended?
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(LISTEN_YIELD_SLEEP_TIME)) => {
                // Yield periodically so interrupting is responsive,
                // to send heartbeat message, and to set off index updates.
                if !package_versions.is_empty() || generate_full_index {
                    tracing::info!("About to update the index. Package versions collected so far: {:?}", package_versions);

                    if generate_full_index {
                        // Ignore the accumulated package updates and
                        // rebuild the whole index instead.
                        tracing::info!("Generating a full index for '{}'", repo_to_index.name());
                        FlatBufferRepoIndex::index_repo(&repos).await.map_err(|err| Error::String(format!("Full index generation failed: {err}")))?;
                        tracing::info!("Full index generation finished");
                        generate_full_index = false;
                    } else {
                        // Run an index update to include any modified packages.
                        //
                        // The method to updating the index will also output index update
                        // status messages. Those messages will act as heartbeat messages
                        // for this indexer.
                        tracing::warn!("Reading index for '{}'", repo_to_index.name());
                        match FlatBufferRepoIndex::from_repo_file(repo_to_index).await {
                            Ok(current_index) => {
                                tracing::debug!("Got the current index for '{}'", repo_to_index.name());
                                let idents: Vec<_> = package_versions.iter().cloned().collect();

                                tracing::info!("Staring index update as if running: 'spk repo index -r {} {}'", repo_to_index.name(), idents.iter().map(|pv| format!("--update {pv}")).join(" "));

                                current_index.update_packages(repo_to_index, &idents).await.map_err(|err| Error::String(format!("Indexer has a problem updating index: {err}")))?;
                                tracing::info!("Index update finished");
                            }
                            Err(err) => {
                                // There isn't an existing index, so generate one from
                                // scratch. This will include all package updates so far.
                                tracing::warn!("Failed to load flatbuffer index: {err}");
                                tracing::warn!("No current index to update. Generating a new full index for '{}' repo", repo_to_index.name());
                                FlatBufferRepoIndex::index_repo(&repos).await.map_err(|err| Error::String(format!("Full index generation failed: {err}")))?;
                                tracing::warn!("Full index generation finished for previously missing index of '{}' repo", repo_to_index.name());
                            }
                        }
                    };

                    // Now the index update (or generation) is done, clear the versions
                    // and update the consumer's message offsets.
                    consumer.commit_consumer_state(CommitMode::Sync).map_err(|err| {
                        Error::String(format!(
                            "Error while committing indexer's consumer state: {err}"
                        ))
                    })?;
                    package_versions.clear();
                    read_messages = 0;
                    tracing::info!("Committed consumer state, updated broker with all processed package event message offsets.");
                } else {
                    // Not making an index this time around because no update
                    // messages have been read.
                    if read_messages > 0 {
                        // Commit any consumed messages that could not be processed due
                        // to problems with the message, e.g. parsing errors.
                        consumer.commit_consumer_state(CommitMode::Sync).map_err(|err| {
                             tracing::error!("{err}");
                             Error::String(format!("Error while committing indexer's consumer state: {err}"))
                         })?;
                        read_messages = 0;
                        tracing::info!("Committed indexer's consumer state for messages processed so far.");
                    }

                    // If enough time has passed since the last heartbeat,
                    // send another heartbeat message.
                    time_since_last_message += LISTEN_YIELD_SLEEP_TIME;
                    if time_since_last_message >= indexer_config.heartbeat_freq_ms {
                        announce_index_event(kafka_channel,
                                             IndexEvent::IndexerHeartbeat,
                                             repo_to_index.address(),
                                             indexer_name,
                                             // This does not have an index start time so
                                             // this is a placeholder.
                                             0
                        ).await?;

                        time_since_last_message = 0;
                    }
                }
            }
        }
    }

    Ok(())
}
