#!/bin/sh

: ${ZOOKEEPER:=localhost:2181}

kafka-topics.sh \
    --create \
    --zookeeper "$ZOOKEEPER" \
    --topic expert \
    --partitions 1 \
    --replication-factor 1 \
    --config cleanup.policy=delete \
    --config compression.type=uncompressed \
    --config retention.ms=$((14 * 24 * 3600 * 1000)) \
    --config max.message.bytes=$((1 * 1024 * 1024))
