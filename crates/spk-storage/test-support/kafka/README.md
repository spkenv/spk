<!--
Copyright (c) Contributors to the SPK project.
SPDX-License-Identifier: Apache-2.0
https://github.com/spkenv/spk
-->

# Kafka integration test harness

The `spk-storage` kafka integration tests need a real broker to talk
to, so they are marked `#[ignore]` and only run when the environment
variable `SPK_TEST_KAFKA_BROKERS` points at one. This directory stands
a broker up in a container for them.

These tests cover the offset/assignment behavior `spk publish` relies on
when waiting for an index update. That behavior depends on how the
broker responds to manual partition assignment, which can't be
meaningfully checked without a broker — hence the container.

By default the harness runs **Kafka 2.8.1** (Confluent Platform 6.2.1),
the broker version `spk publish` must remain compatible with.

## Quick start

From anywhere in the repo:

```sh
make test-kafka
```

or run the script directly:

```sh
crates/spk-storage/test-support/kafka/run-kafka-tests.sh
```

The script starts the broker, runs the ignored kafka tests against it,
and tears the containers down on exit (including on failure).

## Configuration

The script reads these environment variables:

| Variable           | Default                          | Purpose                                  |
| ------------------ | -------------------------------- | ---------------------------------------- |
| `CONTAINER_ENGINE` | `podman`                         | Container engine (`podman` or `docker`). |
| `BROKER_IMAGE`     | `confluentinc/cp-kafka:6.2.1`    | Broker image (Kafka 2.8.1).              |
| `ZK_IMAGE`         | `confluentinc/cp-zookeeper:6.2.1`| ZooKeeper image.                         |
| `KAFKA_PORT`       | `9092`                           | Host port the broker is published on.    |

To test against your own broker image (e.g. an internal 2.8.1 build):

```sh
BROKER_IMAGE=my.registry/kafka:2.8.1 \
  crates/spk-storage/test-support/kafka/run-kafka-tests.sh
```

Extra arguments are forwarded to the test binary, e.g. to see output:

```sh
crates/spk-storage/test-support/kafka/run-kafka-tests.sh --nocapture
```

## Running against an existing broker

If you already have a broker, skip the script and set the variable
yourself:

```sh
SPK_TEST_KAFKA_BROKERS=localhost:9092 \
  cargo test -p spk-storage --lib messaging::kafka -- --ignored
```

## docker compose alternative

`docker-compose.yaml` brings up the same broker for those who prefer
compose:

```sh
docker compose -f docker-compose.yaml up -d
SPK_TEST_KAFKA_BROKERS=localhost:9092 \
  cargo test -p spk-storage --lib messaging::kafka -- --ignored
docker compose -f docker-compose.yaml down
```
