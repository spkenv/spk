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
use millisecond::prelude::*;
use once_cell::sync::Lazy;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
#[cfg(feature = "sentry")]
use rdkafka::message::{BorrowedMessage, Headers};
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

/// Amount of time to sleep while waiting for next message, when
/// listening for new messages.
const LISTEN_YIELD_SLEEP_TIME: Duration = Duration::from_millis(100);

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
        tracing::debug!(
            "Not sending package event: no 'package_updates_topic_name' configured for kafka messaging channel",
        );
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

    tracing::debug!(
        "Sent package update message: {event} - {repo_name} - {to} - {ident} as:\n{message:?}"
    );

    Ok(())
}

/// Send an index event message
pub(crate) async fn announce_index_event(
    kafka_channel: &KafkaChannel,
    event: IndexEvent,
    to: &url::Url,
    repo_name: &RepositoryName,
    index_start_time: &DateTime<Utc>,
) -> Result<()> {
    // Only send a message if the index updates topic is configured
    let Some(topic_name) = kafka_channel.index_updates_topic_name.as_ref() else {
        tracing::debug!(
            "Not sending index event: no 'index_updates_topic_name' configured for kafka messaging channel",
        );
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
        start: index_start_time.timestamp(),
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
            running.store(false, std::sync::atomic::Ordering::Relaxed);
        });
    }
    running
}

/// Helper for consumers that want to start with the message before
/// the latest message because they want to use it to check whether
/// the usual message producer is running by seeing if it has sent a
/// message recently.
///
/// This uses manual partition assignment (rather than subscribing to
/// the topic via consumer group management) so the starting offsets
/// can be set up front without depending on a group join/rebalance.
/// The consumers that use this have unique, ephemeral group ids and
/// never reuse committed offsets, so there is no need for consumer
/// group coordination here. Committing offsets before the group join
/// completed was the source of intermittent `IllegalGeneration`
/// ("Specified group generation id is not valid") errors.
///
/// Returns the offset each partition was positioned to start reading
/// from, keyed by partition id. Callers can compare this against the
/// offsets of the messages they actually receive: if the broker
/// delivers a message from *before* the assigned offset, the assigned
/// offset was silently discarded (see
/// [`warn_if_reading_before_assigned_offset`]), which is the usual
/// reason this misbehaves in production.
fn set_offsets_to_one_before_the_latest_message(
    consumer: Arc<StreamConsumer>,
    topic_name: &str,
    consumer_label: String,
    broker_fetch_timeout: u64,
) -> Result<HashMap<i32, i64>> {
    let timeout = Timeout::After(Duration::new(broker_fetch_timeout, 0));

    // Set up the message reading offset to the just before the last
    // message in the topics.
    let metadata = consumer
        .fetch_metadata(Some(topic_name), timeout)
        .map_err(|err| {
            Error::String(format!(
                "failed to fetch metadata for '{topic_name}' topic for a kafka {consumer_label} consumer: {err}"
            ))
        })?;

    let mut offsets = TopicPartitionList::new();
    let mut assigned_start_offsets = HashMap::new();
    for topic in metadata.topics() {
        for partition in topic.partitions() {
            let (low, high) = match consumer.fetch_watermarks(topic.name(), partition.id(), timeout)
            {
                Ok(watermarks) => watermarks,
                Err(err) => {
                    // Do NOT silently fall back to offset 0 here: that
                    // would make the consumer replay the entire
                    // partition from the start, which looks like the
                    // assignment "had no effect". Skip the partition
                    // instead and say so loudly, so the cause is
                    // visible in production logs.
                    tracing::warn!(
                        "kafka {consumer_label} consumer could not fetch watermarks for '{}' partition {} (skipping it; messages on this partition will NOT be read): {err}",
                        topic.name(),
                        partition.id(),
                    );
                    continue;
                }
            };

            // Go back one message from the latest
            let new_offset = std::cmp::max(0, high - 1);
            tracing::debug!(
                "Start offset for {} partition {}: low={low}, high={high}, start={new_offset}",
                topic.name(),
                partition.id(),
            );
            offsets
                .add_partition_offset(topic.name(), partition.id(), Offset::Offset(new_offset))
                .map_err(|err| {
                    Error::String(format!(
                        "failed to add partition offset to TopicPartitionList for the kafka {consumer_label} consumer: {err}"
                    ))
                })?;
            assigned_start_offsets.insert(partition.id(), new_offset);
        }
    }

    // Assign the partitions with their starting offsets so the
    // previous message(s) will be the first read from the stream.
    //
    // Manual assignment is used instead of committing offsets for a
    // subscribed (group-managed) consumer. The commit approach raced
    // against the group join/rebalance and intermittently failed with
    // `IllegalGeneration` because the consumer had no valid group
    // generation yet.
    // Diagnostic: report the offsets we are assigning each partition to
    // start from. This is the meaningful signal -- unlike
    // `consumer.position()`, which only reflects consume progress and so
    // reads back as `Invalid` until the first message is fetched. The
    // offsets logged here are what the broker should honor; compare them
    // against the offsets of the messages actually delivered (see
    // `warn_if_reading_before_assigned_offset`).
    tracing::debug!("kafka {consumer_label} consumer assigning start offsets: {offsets:?}");

    // Assign the partitions with their starting offsets so the
    // previous message(s) will be the first read from the stream.
    //
    // Manual assignment is used instead of committing offsets for a
    // subscribed (group-managed) consumer. The commit approach raced
    // against the group join/rebalance and intermittently failed with
    // `IllegalGeneration` because the consumer had no valid group
    // generation yet.
    consumer.assign(&offsets).map_err(|err| {
        Error::String(format!(
            "failed to assign starting offsets for the kafka {consumer_label} consumer: {err}"
        ))
    })?;

    Ok(assigned_start_offsets)
}

/// Emit a one-time-per-partition explanation when the broker delivers a
/// message from before the offset the consumer was assigned to start
/// at.
///
/// Reading from before the assigned offset means the assigned offset
/// was discarded and the consumer is replaying the partition from
/// (near) the start. The common causes are an assigned offset the
/// broker rejected as out of range -- which falls back to
/// `auto.offset.reset` (currently `earliest`) -- or a watermark fetch
/// that failed. In production this manifests as `spk publish` churning
/// through stale messages and behaving as though the index never
/// updated, so surface it explicitly rather than letting it look like
/// normal operation.
fn warn_if_reading_before_assigned_offset(
    consumer_label: &str,
    assigned_start_offsets: &HashMap<i32, i64>,
    already_warned: &mut HashSet<i32>,
    partition: i32,
    offset: i64,
) {
    let Some(&start) = first_read_before_assigned_offset(
        assigned_start_offsets,
        already_warned,
        partition,
        offset,
    ) else {
        return;
    };
    tracing::warn!(
        "kafka {consumer_label} consumer read offset {offset} from partition {partition}, \
         but was assigned to start at offset {start}. The broker is delivering messages \
         from before the assigned offset, so the assignment had no effect -- likely the \
         assigned offset was out of range (falling back to auto.offset.reset=earliest) or \
         a watermark fetch failed. This is why recent index status updates may be missed \
         and stale messages are being processed."
    );
}

/// The decision behind [`warn_if_reading_before_assigned_offset`], split
/// out so it can be tested without a broker.
///
/// Returns `Some(assigned_start)` the first time a message on
/// `partition` is seen at an `offset` before the offset that partition
/// was assigned to start at; returns `None` otherwise (offset is at or
/// after the assigned start, the partition wasn't assigned, or this
/// partition has already been reported). Records reported partitions in
/// `already_warned` so each is reported at most once.
fn first_read_before_assigned_offset<'a>(
    assigned_start_offsets: &'a HashMap<i32, i64>,
    already_warned: &mut HashSet<i32>,
    partition: i32,
    offset: i64,
) -> Option<&'a i64> {
    let start = assigned_start_offsets.get(&partition)?;
    if offset >= *start {
        return None;
    }
    // `insert` is false when the partition was already present, i.e.
    // we've already reported it; avoid repeating while it replays the
    // rest of the old messages.
    already_warned.insert(partition).then_some(start)
}

/// Listen to the index updates topic until there's an "index completed"
/// message for the given repository's index with an index update
/// start time after the given update timestamp. This will also
/// timeout if too much time passes without receiving any index update
/// messages.
pub(crate) async fn listen_to_index_status_updates(
    kafka_channel: &KafkaChannel,
    min_update_time: &DateTime<Utc>,
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
        .set("group.id", group_id.clone())
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
        // The starting offset is set explicitly below via manual
        // partition assignment. This reset policy is only the fallback
        // librdkafka uses if an assigned offset is rejected as out of
        // range; when that happens it replays from the start, which
        // `warn_if_reading_before_assigned_offset` detects and explains.
        .set("auto.offset.reset", "earliest")
        .create()
        .map_err(|err| {
            Error::String(format!(
                "failed to create kafka index updates consumer from config: {err}"
            ))
        })?;

    let consumer = Arc::new(consumer);

    // Manually assign the topic's partitions with the starting message
    // offset for reading set to just before the last message in the
    // topic. This intentionally does not use `subscribe` (consumer
    // group management); see the note on
    // set_offsets_to_one_before_the_latest_message.
    let assigned_start_offsets = set_offsets_to_one_before_the_latest_message(
        consumer.clone(),
        topic_name,
        format!("'{group_id}' index updates"),
        kafka_channel.index_update_listener_broker_fetch_timeout_ms,
    )?;

    // Tracks partitions we've already warned about replaying, so the
    // diagnostic below is logged at most once per partition.
    let mut replay_warned: HashSet<i32> = HashSet::new();

    // Set up signal handlers to stop if interrupted
    let running = setup_running_interrupt_handling();

    // Start listening to index update messages
    let utc_now: DateTime<Utc> = Utc::now();
    let now_timestamp = utc_now.timestamp_millis();
    let recent_past_messages_time = rdkafka::Timestamp::from(
        now_timestamp - kafka_channel.index_update_listener_recent_past_duration_ms,
    );

    let mut there_is_a_recent_message = false;
    let mut time_since_last_message = Duration::new(0, 0);

    let index_update_timeout =
        Duration::from_millis(kafka_channel.index_update_listener_timeout_ms);

    tracing::debug!(
        "Looking for a recent index update message that was sent after: {min_update_time}"
    );

    let mut stream = consumer.stream();

    while running.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::select! {
            maybe_message = stream.next() => {
                match maybe_message {
                    Some(Ok(message)) => {
                        // Surface the common production failure mode: the
                        // broker delivering messages from before the offset
                        // we assigned (i.e. the assignment had no effect).
                        warn_if_reading_before_assigned_offset(
                            "index updates",
                            &assigned_start_offsets,
                            &mut replay_warned,
                            message.partition(),
                            message.offset(),
                        );

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
                        let payload = match message.payload_view::<str>().map(|s| s.map(|s| s.to_owned())) {
                            Some(result) => match result {
                                Ok(pl) => pl,
                                Err(err) => {
                                    // This broken message will be discarded
                                    let warning_message = format!("Indexer get index update message payload as str: {err}");
                                    tracing::warn!(warning_message);
                                    #[cfg(feature = "sentry")]
                                    send_ignored_message_to_sentry(warning_message, &message);
                                    continue;
                                }
                            } ,
                            None => continue,
                        };

                        if let Ok(index_update) = serde_json::from_str::<IndexUpdateMessage>(&payload).inspect_err(|err| {
                            let warning_message = format!("Ignoring message. Index consumer failed parse index update message payload ('{payload}'): {err}");
                            tracing::warn!(warning_message);
                            #[cfg(feature = "sentry")]
                            send_ignored_message_to_sentry(warning_message, &message);
                        }) {
                            // This is a valid recent message, so reset the
                            // "indexer might be down" timeout timer.
                            time_since_last_message = Duration::new(0, 0);
                            tracing::debug!("Read index update message: {index_update:?}");

                            if index_update.start >= min_update_time.timestamp() {
                                tracing::debug!("Message is from an index update that started after the min_update_time: {}",  index_update.start - min_update_time.timestamp());

                                if repo.address().as_str() == index_update.repo {
                                    // The message is for correct repository
                                    if index_update.event.is_completed() {
                                        // And the index has been updated. So this can stop listening now.
                                        running.store(false, std::sync::atomic::Ordering::Relaxed);
                                        tracing::debug!("Recent index update completed. Stopping waiting: '{}'", index_update.event);
                                        break;
                                    }
                                }
                            } else {
                                // This message is about an older
                                // index update than we are interested in.
                                tracing::debug!("Index update message is not for recent enough update: {}",  index_update.start - min_update_time.timestamp());
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
            _ = tokio::time::sleep(LISTEN_YIELD_SLEEP_TIME) => {
                // Yield periodically so interrupting is responsive.
                if there_is_a_recent_message {
                    // If enough time has passed since the last recent
                    // message was read, then assume something has happened
                    // to the index updating process and stop waiting.
                    time_since_last_message += LISTEN_YIELD_SLEEP_TIME;
                    if time_since_last_message >= index_update_timeout {
                        let ms = Millisecond::from_millis(kafka_channel.index_update_listener_timeout_ms);
                        let time_description = ms.pretty_with(MillisecondOption::long());
                        tracing::warn!("Index updates consumer timed out after {time_description}. The index may not have been updated.");
                        running.store(false, std::sync::atomic::Ordering::Relaxed);
                    }

                } else {
                    // Without a recent enough message, assume the indexer is down
                    // and stop waiting for the index to update.
                    tracing::warn!("Index updates consumer cannot see a recent message. The indexer might be down. Not waiting for the index update.");
                    running.store(false, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }
    }

    Ok(())
}

/// Helper for sending non-fatal messaging issues to sentry, such as when the
/// message payload does not parse correctly.
#[cfg(feature = "sentry")]
pub(crate) fn send_ignored_message_to_sentry(issue: String, message: &BorrowedMessage) {
    // Get additional data from the message and add it to a sentry breadcrumb
    use std::collections::BTreeMap;
    let timestamp = message.timestamp().to_millis().unwrap_or_default();
    let key = match message.key() {
        Some(k) => String::from_utf8_lossy(k).to_string(),
        None => "None".to_string(),
    };

    let message_headers: BTreeMap<_, _> = if let Some(headers) = message.headers() {
        headers
            .iter()
            .map(|h| (h.key.to_string(), h.value))
            .collect()
    } else {
        Default::default()
    };

    let mut data = std::collections::BTreeMap::new();
    data.insert(String::from("created"), serde_json::json!(timestamp));
    data.insert(String::from("topic"), serde_json::json!(message.topic()));
    data.insert(
        String::from("partition"),
        serde_json::json!(message.partition()),
    );
    data.insert(String::from("offset"), serde_json::json!(message.offset()));
    data.insert(String::from("key"), serde_json::json!(key));
    data.insert(String::from("headers"), serde_json::json!(message_headers));

    sentry::add_breadcrumb(sentry::Breadcrumb {
        category: Some("Problem message".into()),
        message: Some("Kafka message details".into()),
        data,
        level: sentry::Level::Warning,
        ..Default::default()
    });

    // First closure configures the scope for this message.
    // Second closure makes and sends the message.
    sentry::with_scope(
        |_scope| {
            // Nothing to configure
        },
        || sentry::capture_message(&issue, sentry::Level::Warning),
    );
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
        &Utc::now(),
    )
    .await?;

    // Start listening to index update messages
    let mut time_since_last_message = Duration::new(0, 0);
    let mut read_messages = 0;
    let mut generate_full_index = false;
    let mut package_versions: HashSet<OptVersionIdent> = HashSet::new();

    let heartbeat_delay = Duration::from_millis(indexer_config.heartbeat_freq_ms);

    let mut stream = consumer.stream();

    while running.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::select! {
            maybe_message = stream.next() => {
                match maybe_message {
                    Some(Ok(message)) => {
                        // Get the package event details
                        let payload = match message.payload_view::<str>().map(|s| s.map(|s| s.to_owned())) {
                            Some(result) => match result {
                                Ok(pl) => pl,
                                Err(err) => {
                                    // This broken message will be discarded
                                    let warning_message = format!("Indexer get index update message payload as str: {err}");
                                    tracing::warn!(warning_message);
                                    #[cfg(feature = "sentry")]
                                    send_ignored_message_to_sentry(warning_message, &message);
                                    continue;
                                }
                            } ,
                            None => continue,
                        };

                        if let Ok(package_event) = serde_json::from_str::<PackageEventMessage>(&payload).inspect_err(|err| {
                            // This problem message will be discarded
                            let warning_message = format!("Ignoring message. Indexer failed to parse package event message payload ('{payload}'): {err}");
                            tracing::warn!(warning_message);
                            #[cfg(feature = "sentry")]
                            send_ignored_message_to_sentry(warning_message, &message);
                        }) {
                            // Process a valid message
                            tracing::debug!("Indexer read package event message: {package_event:?}");
                            time_since_last_message = Duration::new(0, 0);

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
                                    let package_ident = match parse_build_ident(&package_event.package) {
                                        Ok(ident) => ident,
                                        Err(err) => {
                                            let warning_message = format!(
                                                "Ignoring message. Indexer unable to get ident from a package field ('{}') in package event message: {err}", package_event.package
                                            );
                                            tracing::warn!(warning_message);
                                            #[cfg(feature = "sentry")]
                                            send_ignored_message_to_sentry(warning_message, &message);
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
            _ = tokio::time::sleep(LISTEN_YIELD_SLEEP_TIME) => {
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
                    if time_since_last_message >= heartbeat_delay {
                        announce_index_event(kafka_channel,
                                             IndexEvent::IndexerHeartbeat,
                                             repo_to_index.address(),
                                             indexer_name,
                                             // This does not have an index start time so
                                             // this is a placeholder.
                                             &Utc::now()
                        ).await?;

                        time_since_last_message = Duration::new(0, 0);
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
    use rdkafka::client::DefaultClientContext;
    use rdkafka::config::ClientConfig;
    use rdkafka::consumer::StreamConsumer;
    use rdkafka::producer::{FutureProducer, FutureRecord};

    use super::*;

    /// Read the broker(s) to test against from the environment.
    ///
    /// The kafka integration tests need a real broker. The
    /// `test-support/kafka` harness stands one up in a container and
    /// sets this variable; see that directory's README.
    fn test_brokers() -> Option<String> {
        std::env::var("SPK_TEST_KAFKA_BROKERS")
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// The replay-detection diagnostic only fires for messages before
    /// the assigned offset, and only once per partition. This needs no
    /// broker.
    #[test]
    fn detects_reading_before_assigned_offset_once_per_partition() {
        let assigned = HashMap::from([(2, 7_i64), (5, 0_i64)]);
        let mut warned = HashSet::new();

        // At or after the assigned offset: not a replay.
        assert_eq!(
            first_read_before_assigned_offset(&assigned, &mut warned, 2, 7),
            None
        );
        assert_eq!(
            first_read_before_assigned_offset(&assigned, &mut warned, 2, 9),
            None
        );

        // Before the assigned offset: reported, with the assigned start.
        assert_eq!(
            first_read_before_assigned_offset(&assigned, &mut warned, 2, 3),
            Some(&7)
        );
        // Same partition again is suppressed to avoid log spam.
        assert_eq!(
            first_read_before_assigned_offset(&assigned, &mut warned, 2, 4),
            None
        );

        // A partition assigned offset 0 can never read before it.
        assert_eq!(
            first_read_before_assigned_offset(&assigned, &mut warned, 5, 0),
            None
        );

        // An unassigned partition is ignored.
        assert_eq!(
            first_read_before_assigned_offset(&assigned, &mut warned, 11, 0),
            None
        );
    }

    /// Exercises [`set_offsets_to_one_before_the_latest_message`]
    /// against a real broker, reproducing production conditions: a
    /// multi-partition topic with messages in only some of the
    /// partitions.
    ///
    /// It must position a fresh consumer at the last message of every
    /// *non-empty* partition and read nothing older, and it must not
    /// replay anything from the empty partitions. This is the behavior
    /// `spk publish` relies on when waiting for an index update.
    ///
    /// The bug this guards against (the consumer silently reading from
    /// the beginning of partitions) is invisible without a broker to
    /// talk to, so this test is `#[ignore]`d and only runs when
    /// `SPK_TEST_KAFKA_BROKERS` points at a broker. Run it with the
    /// `test-support/kafka` harness, or `make test-kafka`.
    #[tokio::test]
    #[ignore = "requires a running kafka broker (set SPK_TEST_KAFKA_BROKERS); see test-support/kafka"]
    async fn assign_starts_at_one_before_the_latest_message() {
        let brokers = test_brokers().expect("SPK_TEST_KAFKA_BROKERS must be set to run this test");

        // Match production: a 12-partition topic with messages in only
        // some of the partitions (others left empty).
        const PARTITION_COUNT: i32 = 12;
        const MESSAGES_PER_PARTITION: i64 = 4;
        const POPULATED_PARTITIONS: &[i32] = &[2, 5, 9];

        // Use a unique topic so repeated/parallel runs don't interfere.
        let topic = format!("spk-test-index-updates-{}", Ulid::new());

        let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .create()
            .expect("create kafka admin client");
        admin
            .create_topics(
                &[NewTopic::new(
                    &topic,
                    PARTITION_COUNT,
                    TopicReplication::Fixed(1),
                )],
                &AdminOptions::new(),
            )
            .await
            .expect("create test topic");

        // Produce several messages to just the populated partitions,
        // recording the expected last offset/payload for each.
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .expect("create kafka producer");

        // partition -> (last offset, last payload) that the consumer
        // should be positioned to read.
        let mut expected_last: HashMap<i32, (i64, String)> = HashMap::new();
        for &partition in POPULATED_PARTITIONS {
            for i in 0..MESSAGES_PER_PARTITION {
                let payload = format!("p{partition}-msg{i}");
                let delivery = producer
                    .send(
                        FutureRecord::<(), _>::to(&topic)
                            .partition(partition)
                            .payload(&payload),
                        Duration::from_secs(5),
                    )
                    .await
                    .expect("produce test message");
                assert_eq!(delivery.partition, partition);
                expected_last.insert(partition, (delivery.offset, payload));
            }
        }

        // Build a consumer the same way `listen_to_index_status_updates`
        // does: a fresh, unique group with no committed offsets.
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("group.id", format!("spk-publish-{}", Ulid::new()))
            .set("enable.partition.eof", "false")
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "earliest")
            .create()
            .expect("create kafka consumer");
        let consumer = Arc::new(consumer);

        // The code under test: position the consumer at one before the
        // latest message of every partition via manual assignment.
        set_offsets_to_one_before_the_latest_message(
            consumer.clone(),
            &topic,
            "test".to_string(),
            10,
        )
        .expect("set starting offsets");

        // Drain everything the consumer is willing to deliver. With the
        // offsets set correctly this is exactly the last message of each
        // populated partition; an idle timeout marks the end.
        let mut stream = consumer.stream();
        let mut read: HashMap<i32, Vec<(i64, String)>> = HashMap::new();
        while let Ok(Some(result)) =
            tokio::time::timeout(Duration::from_secs(5), stream.next()).await
        {
            let message = result.expect("error reading message");
            let payload = message
                .payload_view::<str>()
                .expect("message has a payload")
                .expect("payload is valid utf-8")
                .to_string();
            read.entry(message.partition())
                .or_default()
                .push((message.offset(), payload));
        }

        // Every populated partition must yield exactly its last message,
        // at offset MESSAGES_PER_PARTITION - 1 (never an earlier offset,
        // which would mean it replayed from the start of the partition).
        for &partition in POPULATED_PARTITIONS {
            let messages = read
                .get(&partition)
                .unwrap_or_else(|| panic!("no message read from populated partition {partition}"));
            let (expected_offset, expected_payload) = &expected_last[&partition];
            assert_eq!(
                expected_offset,
                &(MESSAGES_PER_PARTITION - 1),
                "test setup: partition {partition} last offset"
            );
            assert_eq!(
                messages.as_slice(),
                std::slice::from_ref(&(*expected_offset, expected_payload.clone())),
                "partition {partition} should yield only its last message; \
                 reading earlier offsets means it replayed from the start"
            );
        }

        // No messages at all should come from the empty partitions.
        for (&partition, messages) in &read {
            assert!(
                POPULATED_PARTITIONS.contains(&partition),
                "read {} message(s) from partition {partition}, which was left empty",
                messages.len()
            );
        }

        // Best-effort cleanup of the test topic.
        let _ = admin.delete_topics(&[&topic], &AdminOptions::new()).await;
    }
}
