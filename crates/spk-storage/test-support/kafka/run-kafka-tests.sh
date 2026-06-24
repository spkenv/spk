#!/usr/bin/env bash
# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk
#
# Stand up a Kafka broker in a container, run the spk-storage kafka
# integration tests against it, then tear everything down.
#
# By default this runs Kafka 2.8.1 (Confluent Platform 6.2.1) so the
# tests exercise the broker version spk publish must remain compatible
# with. Override the images to test against a different broker:
#
#   BROKER_IMAGE=my.registry/kafka:2.8.1 ./run-kafka-tests.sh
#
# Requires podman (default) or docker. Select with CONTAINER_ENGINE.

set -euo pipefail

CONTAINER_ENGINE="${CONTAINER_ENGINE:-podman}"
# Fully-qualified image names so podman (which has no default registry)
# resolves them without a registries.conf entry.
ZK_IMAGE="${ZK_IMAGE:-docker.io/confluentinc/cp-zookeeper:6.2.1}"
BROKER_IMAGE="${BROKER_IMAGE:-docker.io/confluentinc/cp-kafka:6.2.1}"   # Kafka 2.8.1
KAFKA_PORT="${KAFKA_PORT:-9092}"
POD_NAME="${POD_NAME:-spk-kafka-test}"
ZK_NAME="${POD_NAME}-zookeeper"
BROKER_NAME="${POD_NAME}-broker"

cleanup() {
    echo "==> Tearing down kafka test containers"
    "$CONTAINER_ENGINE" rm -f "$BROKER_NAME" "$ZK_NAME" >/dev/null 2>&1 || true
    if "$CONTAINER_ENGINE" pod exists "$POD_NAME" >/dev/null 2>&1; then
        "$CONTAINER_ENGINE" pod rm -f "$POD_NAME" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# Remove any leftovers from a previous interrupted run.
cleanup

echo "==> Starting kafka ${BROKER_IMAGE} on localhost:${KAFKA_PORT} (engine: ${CONTAINER_ENGINE})"

# podman uses a pod to share localhost between zookeeper and the broker;
# docker shares it via --network=host on the broker instead.
if [ "$(basename "$CONTAINER_ENGINE")" = "podman" ]; then
    "$CONTAINER_ENGINE" pod create --name "$POD_NAME" -p "${KAFKA_PORT}:${KAFKA_PORT}" >/dev/null
    POD_ARGS=(--pod "$POD_NAME")
    NET_ARGS=()
else
    POD_ARGS=()
    NET_ARGS=(--network host)
fi

"$CONTAINER_ENGINE" run -d --name "$ZK_NAME" "${POD_ARGS[@]}" "${NET_ARGS[@]}" \
    -e ZOOKEEPER_CLIENT_PORT=2181 \
    -e ZOOKEEPER_TICK_TIME=2000 \
    "$ZK_IMAGE" >/dev/null

"$CONTAINER_ENGINE" run -d --name "$BROKER_NAME" "${POD_ARGS[@]}" "${NET_ARGS[@]}" \
    -e KAFKA_BROKER_ID=1 \
    -e KAFKA_ZOOKEEPER_CONNECT=localhost:2181 \
    -e KAFKA_LISTENERS="PLAINTEXT://0.0.0.0:${KAFKA_PORT}" \
    -e KAFKA_ADVERTISED_LISTENERS="PLAINTEXT://localhost:${KAFKA_PORT}" \
    -e KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1 \
    -e KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR=1 \
    -e KAFKA_TRANSACTION_STATE_LOG_MIN_ISR=1 \
    -e KAFKA_AUTO_CREATE_TOPICS_ENABLE=true \
    "$BROKER_IMAGE" >/dev/null

echo "==> Waiting for the broker to become ready"
ready=""
for _ in $(seq 1 60); do
    if "$CONTAINER_ENGINE" exec "$BROKER_NAME" \
        kafka-broker-api-versions --bootstrap-server "localhost:${KAFKA_PORT}" \
        >/dev/null 2>&1; then
        ready=1
        break
    fi
    sleep 2
done
if [ -z "$ready" ]; then
    echo "ERROR: kafka broker did not become ready in time" >&2
    "$CONTAINER_ENGINE" logs "$BROKER_NAME" >&2 || true
    exit 1
fi
echo "==> Broker is ready"

export SPK_TEST_KAFKA_BROKERS="localhost:${KAFKA_PORT}"

# Run the ignored kafka integration tests. Extra args are forwarded to
# the test binary (e.g. --nocapture).
echo "==> Running spk-storage kafka integration tests"
${CARGO:-cargo} test -p spk-storage --lib messaging::kafka -- --ignored "$@"
