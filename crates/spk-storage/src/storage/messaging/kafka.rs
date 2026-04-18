// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::time::Duration;

use once_cell::sync::Lazy;
use rdkafka::ClientConfig;
use rdkafka::error::KafkaError;
use rdkafka::producer::{FutureProducer, FutureRecord};
use serde_json::json;
use spk_config::KafkaChannel;
use spk_schema::BuildIdent;
use tokio::sync::Mutex;

use crate::storage::messaging::PackageEvent;
use crate::{Error, Result};

type ProducersByBrokers = HashMap<Vec<String>, std::result::Result<FutureProducer, KafkaError>>;

static KAFKA_PRODUCERS: Lazy<Mutex<ProducersByBrokers>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) async fn announce_package_event(
    kafka_channel: &KafkaChannel,
    event: PackageEvent,
    to: &url::Url,
    ident: &BuildIdent,
) -> Result<()> {
    let Some(topic_name) = kafka_channel.package_updates_topic_name.as_ref() else {
        return Ok(());
    };

    let mut kafka_producers = KAFKA_PRODUCERS.lock().await;

    let producer = kafka_producers
        .entry(kafka_channel.brokers.clone())
        .or_insert_with(|| {
            ClientConfig::new()
                // XXX: probably should expose all configuration parameters in
                // spk config
                .set("bootstrap.servers", kafka_channel.brokers.join(","))
                .set("message.timeout.ms", kafka_channel.timeout_ms.to_string())
                .create()
        })
        .as_ref()
        .map_err(|err| {
            Error::String(format!(
                "failed to create kafka producer from config: {err}"
            ))
        })?;

    producer
        .send(
            FutureRecord::to(topic_name)
                // Using the package name as the key for purposes of sending
                // all messages about the same package to the same topic
                // partition.
                .key(ident.name().as_str())
                .payload(
                    &json!({
                        "event": event.to_string(),
                        "repo": to.to_string(),
                        "package": ident.to_string(),
                    })
                    .to_string(),
                ),
            Duration::from_secs(0),
        )
        .await
        .map_err(|err| Error::String(format!("failed to send kafka message: {err:?}")))?;

    Ok(())
}
